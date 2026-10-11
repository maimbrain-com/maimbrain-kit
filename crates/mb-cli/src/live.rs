//! The live-reload channel between `mb serve --watch` and its preview pages.
//!
//! Server → page: a WebSocket at `/__mb/ws` (JSON text frames). A WebSocket
//! rather than server-sent events because browsers allow only six HTTP/1.1
//! connections per host, shared by every tab, and an event stream holds one
//! per tab for as long as it's open; WebSockets don't count against that.
//! The server only writes to it (tiny_http hands over one stream that can't
//! be read and written from two threads), and sends an unsolicited pong, a
//! heartbeat that browsers don't answer (RFC 6455 §5.5.3), every couple of
//! seconds, so a closed tab is noticed when the write fails and nothing
//! piles up unread.
//!
//!   {"type":"hello","server":S,"build":N,"codeBuild":C,"state":"ok"|"building"|"failed",...}
//!   {"type":"building","build":N,"changed":[paths]}
//!   {"type":"reload","build":N,"codeBuild":N,"kind":"code"|"page"}   code: swap the game in place; page: reload
//!   {"type":"assets","build":N,"paths":["assets/hero.png"]}          refetch these, replay to the same frame
//!   {"type":"failed","build":N,"secs":0.4,"summary":"…","errors":[…],"text":"…"}
//!
//! Page → server: `POST /__mb/page` with
//!   {"page":"<per-tab id>","type":"running","build":N,"frames":F,"restored":{…}?}
//!   {"page":…,"type":"error","build":N,"message":"…"}   a trap, watchdog or failed boot
//!   {"page":…,"type":"log","message":"…"}               the game's warnings and errors
//!
//! For scripts and agents: `GET /__mb/status` (JSON, right away) and
//! `GET /__mb/wait?timeout=60`, which returns the same JSON once what's on
//! disk now has been built and every open page runs it (or the build
//! failed, or a page hit an error). See SPEC §9.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::devbuild::Failure;

/// Prints a status line: `[mb] …` on stdout, flushed (SPEC §9 documents the shapes).
macro_rules! status {
    ($($t:tt)*) => {{
        let mut out = std::io::stdout().lock();
        let _ = std::io::Write::write_fmt(&mut out, format_args!("[mb] {}\n", format_args!($($t)*)));
        let _ = std::io::Write::flush(&mut out);
    }};
}
pub(crate) use status;

const HEARTBEAT_EVERY: Duration = Duration::from_secs(2);

/// What a finished build did.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub build: u64,
    /// "code", "page" or "assets".
    pub kind: &'static str,
    pub ok: bool,
    pub secs: f64,
    pub finished: Instant,
    /// The newest modification time among the changed files (when the edit was saved).
    pub edited: Option<SystemTime>,
}

#[derive(Debug, Clone, Default)]
pub struct PageInfo {
    /// Small number for status lines, in order of appearance.
    pub num: u32,
    pub build: u64,
    /// "booting", "running" or "error".
    pub state: String,
    pub frames: u64,
    pub message: String,
    pub restored: Value,
}

#[derive(Default)]
pub struct State {
    pub build: u64,
    pub code_build: u64,
    pub building: bool,
    pub last: Option<Outcome>,
    /// The latest code build failed (sticky until one succeeds; asset changes don't clear it).
    pub failure: Option<(u64, Failure)>,
    /// Fingerprint of the files the latest build started from.
    pub fingerprint: u64,
    pub pages: BTreeMap<String, PageInfo>,
    /// What the preview serves under /game/ (the latest good build).
    pub site: Option<Arc<Site>>,
    clients: Vec<Client>,
    next_client: u64,
}

struct Client {
    id: u64,
    page: String,
    tx: Sender<Vec<u8>>,
}

pub struct Hub {
    pub server: String,
    pub state: Mutex<State>,
    changed: Condvar,
    /// Current fingerprint of the watched files (for /__mb/wait).
    disk: Box<dyn Fn() -> u64 + Send + Sync>,
}

/// What `/__mb/wait` is waiting for, decided from a snapshot of the state.
#[derive(Debug, PartialEq, Eq)]
pub enum Settled {
    /// Not yet: a build is pending or running, or a page hasn't caught up.
    No,
    /// The latest build failed.
    Failed,
    /// Built, and no page is open to run it.
    Built,
    /// Every open page runs the latest build.
    Running,
    /// A page reported an error running the latest build.
    PageError,
}

impl State {
    /// Pages with an open connection.
    fn live_pages(&self) -> impl Iterator<Item = (&String, &PageInfo)> {
        self.pages.iter().filter(|(id, _)| self.clients.iter().any(|c| &c.page == *id))
    }

    pub fn settled(&self, disk: u64) -> Settled {
        if self.building || self.last.is_none() || disk != self.fingerprint {
            return Settled::No;
        }
        if self.failure.is_some() {
            return Settled::Failed;
        }
        let mut any = false;
        let mut error = false;
        for (_, p) in self.live_pages() {
            any = true;
            if p.build < self.build || p.state == "booting" {
                return Settled::No;
            }
            error |= p.state == "error";
        }
        match (any, error) {
            (false, _) => Settled::Built,
            (true, true) => Settled::PageError,
            (true, false) => Settled::Running,
        }
    }

    fn state_name(&self) -> &'static str {
        if self.building {
            "building"
        } else if self.failure.is_some() {
            "failed"
        } else if self.last.is_some() {
            "ok"
        } else {
            "starting"
        }
    }

    /// The JSON `/__mb/status` returns.
    pub fn status_json(&self, disk: u64) -> Value {
        let settled = self.settled(disk);
        let state = match settled {
            Settled::Running => "running",
            Settled::PageError => "error",
            Settled::Failed => "failed",
            Settled::Built => "ok",
            Settled::No if self.building || disk != self.fingerprint => "building",
            Settled::No => "loading",
        };
        let pages: Vec<Value> = self
            .live_pages()
            .map(|(_, p)| {
                json!({ "page": p.num, "build": p.build, "state": p.state, "frames": p.frames, "message": p.message, "restored": p.restored })
            })
            .collect();
        let mut v = json!({
            "state": state,
            "settled": settled != Settled::No,
            "build": self.build,
            "codeBuild": self.code_build,
            "kind": self.last.as_ref().map(|o| o.kind),
            "ok": self.last.as_ref().map(|o| o.ok),
            "secs": self.last.as_ref().map(|o| (o.secs * 1000.0).round() / 1000.0),
            "pages": pages,
        });
        if let Some((build, f)) = &self.failure {
            v["failedBuild"] = json!(build);
            v["error"] = json!(f.summary());
            v["errors"] = json!(f.errors);
        }
        v
    }

    fn hello(&self, server: &str) -> Value {
        let mut v = json!({ "type": "hello", "server": server, "build": self.build, "codeBuild": self.code_build, "state": self.state_name() });
        if let Some((build, f)) = &self.failure {
            v["failure"] = failed_json(*build, 0.0, f);
        }
        v
    }
}

pub fn failed_json(build: u64, secs: f64, f: &Failure) -> Value {
    json!({ "type": "failed", "build": build, "secs": secs, "summary": f.summary(), "errors": f.errors, "text": f.text })
}

impl Hub {
    pub fn new(disk: impl Fn() -> u64 + Send + Sync + 'static) -> Arc<Hub> {
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
        let server = format!("{:x}-{:x}", std::process::id(), t.as_millis());
        Arc::new(Hub { server, state: Mutex::new(State::default()), changed: Condvar::new(), disk: Box::new(disk) })
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends a message to every open page.
    pub fn broadcast(&self, msg: &Value) {
        let frame = ws_frame(1, msg.to_string().as_bytes());
        let mut s = self.lock();
        s.clients.retain(|c| c.tx.send(frame.clone()).is_ok());
        drop(s);
        self.changed.notify_all();
    }

    /// Updates the state and wakes everyone waiting on it.
    pub fn update<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let r = f(&mut self.lock());
        self.changed.notify_all();
        r
    }

    pub fn status(&self) -> Value {
        let disk = (self.disk)();
        self.lock().status_json(disk)
    }

    /// Blocks until `settled` (or the timeout), then returns the status.
    pub fn wait(&self, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        let mut s = self.lock();
        loop {
            let disk = (self.disk)();
            if s.settled(disk) != Settled::No {
                return s.status_json(disk);
            }
            let now = Instant::now();
            if now >= deadline {
                let mut v = s.status_json(disk);
                v["timeout"] = json!(true);
                return v;
            }
            // Re-checked at least this often: the files on disk change without telling us.
            let step = (deadline - now).min(Duration::from_millis(100));
            s = self.changed.wait_timeout(s, step).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    /// A page's report (`POST /__mb/page`): records it and prints a status line.
    pub fn report(&self, body: &str) -> Result<(), String> {
        let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
        let page = v["page"].as_str().ok_or("no page id")?.to_string();
        let kind = v["type"].as_str().unwrap_or("");
        let text = |k: &str| v[k].as_str().unwrap_or("").chars().take(2000).collect::<String>();
        let mut s = self.lock();
        let next = s.pages.len() as u32 + 1;
        let last = s.last.clone();
        let p = s.pages.entry(page).or_insert_with(|| PageInfo { num: next, state: "booting".into(), ..Default::default() });
        match kind {
            "running" => {
                let build = v["build"].as_u64().unwrap_or(0);
                p.frames = v["frames"].as_u64().unwrap_or(0);
                p.restored = v["restored"].clone();
                p.message.clear();
                let first = p.build != build || p.state != "running";
                p.build = build;
                p.state = "running".into();
                let mut line = format!("page {} running build {build} at frame {}", p.num, p.frames);
                if let Some(r) = p.restored.as_object() {
                    line += &restored_note(r);
                }
                if let Some(o) = last.filter(|o| o.build == build && first) {
                    match o.edited.and_then(|t| t.elapsed().ok()) {
                        Some(e) => line += &format!(", {:.2}s after the edit", e.as_secs_f64()),
                        None => line += &format!(", {:.2}s after the build", o.finished.elapsed().as_secs_f64()),
                    }
                }
                status!("{line}");
            }
            "error" => {
                p.build = v["build"].as_u64().unwrap_or(p.build);
                p.state = "error".into();
                p.message = text("message");
                status!("page {} error: {}", p.num, one_line(&p.message));
            }
            "log" => status!("page {} log: {}", p.num, one_line(&text("message"))),
            _ => return Err(format!("unknown report type {kind:?}")),
        }
        drop(s);
        self.changed.notify_all();
        Ok(())
    }

    /// Takes over an upgraded WebSocket connection for `page` until it closes.
    pub fn serve_socket(self: &Arc<Hub>, page: String, mut stream: Box<dyn std::io::Write + Send>) {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let id = self.update(|s| {
            s.next_client += 1;
            let id = s.next_client;
            let _ = tx.send(ws_frame(1, s.hello(&self.server).to_string().as_bytes()));
            let fresh = !s.clients.iter().any(|c| c.page == page);
            s.clients.push(Client { id, page: page.clone(), tx });
            let next = s.pages.len() as u32 + 1;
            let p = s.pages.entry(page.clone()).or_insert_with(|| PageInfo { num: next, state: "booting".into(), ..Default::default() });
            if fresh {
                status!("page {} connected", p.num);
            }
            id
        });
        let hub = self.clone();
        std::thread::spawn(move || {
            loop {
                let frame = match rx.recv_timeout(HEARTBEAT_EVERY) {
                    Ok(f) => f,
                    Err(RecvTimeoutError::Timeout) => ws_frame(10, b""),
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                if stream.write_all(&frame).and_then(|()| stream.flush()).is_err() {
                    break;
                }
            }
            hub.update(|s| {
                s.clients.retain(|c| c.id != id);
                if !s.clients.iter().any(|c| c.page == page)
                    && let Some(p) = s.pages.get(&page)
                {
                    status!("page {} closed", p.num);
                }
            });
        });
    }
}

/// A multi-line message (a panic: location, then what) on one status line.
/// What a page's report says about the session it restored: where the run
/// went differently from the one before it, or, for a run `mb serve --replay`
/// replayed onto this code, that there's nothing to compare it with yet.
fn restored_note(r: &serde_json::Map<String, Value>) -> String {
    let n = |k: &str| r.get(k).and_then(Value::as_u64);
    let (frames, of) = (n("frames").unwrap_or(0), n("of").unwrap_or(0));
    let mut extra = String::new();
    if frames < of {
        extra += &format!("; the game stopped at frame {frames} of {of}");
    }
    if let Some(m) = n("missingAssets").filter(|m| *m > 0) {
        extra += &format!("; {m} asset loads the run saw never happened, so it went differently");
    }
    if r.get("why").and_then(Value::as_str) == Some("replay") {
        return format!(
            " (replayed the run onto this code{extra}; where it differs from the run as recorded can't be told, since the log keeps no per-frame draw hashes: after each edit it's replayed again and compared with this)"
        );
    }
    match (n("firstMismatch"), n("compared")) {
        (Some(f), _) => format!(" (restored; differs from the old run from frame {f}{extra})"),
        (None, Some(0)) => format!(" (restored; no draw hashes were kept to compare{extra})"),
        (None, _) => format!(" (restored; same as the old run{extra})"),
    }
}

fn one_line(s: &str) -> String {
    let joined = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" | ");
    if joined.chars().count() > 400 { joined.chars().take(400).collect::<String>() + "…" } else { joined }
}

/// `Sec-WebSocket-Accept` for a client's `Sec-WebSocket-Key` (RFC 6455 §4.2.2).
pub fn ws_accept(key: &str) -> String {
    let mut sha = sha1_smol::Sha1::new();
    sha.update(key.trim().as_bytes());
    sha.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64(&sha.digest().bytes())
}

/// One unmasked, unfragmented server frame: opcode 1 text, 10 pong.
pub fn ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126 => f.push(n as u8),
        n if n <= 0xffff => {
            f.push(126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(payload);
    f
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Where the preview's game files come from in watch mode.
pub struct Site {
    /// Metered game.wasm.
    pub wasm: Arc<Vec<u8>>,
    pub boot: Arc<Vec<u8>>,
    pub files: Files,
}

pub enum Files {
    /// A game directory: assets are read from disk when asked for, so they're always current.
    Dir(PathBuf),
    /// A .mbx.
    Bundle(Arc<mb_format::Bundle>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_handshake_matches_rfc_6455() {
        // The example in RFC 6455 §1.3.
        assert_eq!(ws_accept("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"a"), "YQ==");
    }

    #[test]
    fn multi_line_messages_fit_one_status_line() {
        assert_eq!(one_line("panic: panicked at src/hud.rs:306:9:\nmin > max\n"), "panic: panicked at src/hud.rs:306:9: | min > max");
        assert_eq!(one_line(&"x".repeat(500)).chars().count(), 401);
    }

    #[test]
    fn websocket_frames() {
        assert_eq!(ws_frame(1, b"hi"), [0x81, 2, b'h', b'i']);
        assert_eq!(ws_frame(10, b""), [0x8a, 0]);
        let mid = ws_frame(1, &[b'x'; 300]);
        assert_eq!(&mid[..4], &[0x81, 126, 1, 44]);
        assert_eq!(mid.len(), 304);
        let big = ws_frame(1, &vec![b'x'; 70_000]);
        assert_eq!(&big[..2], &[0x81, 127]);
        assert_eq!(u64::from_be_bytes(big[2..10].try_into().unwrap()), 70_000);
    }

    fn built(s: &mut State, build: u64, ok: bool) {
        s.build = build;
        s.building = false;
        s.fingerprint = 7;
        s.last = Some(Outcome { build, kind: "code", ok, secs: 0.5, finished: Instant::now(), edited: None });
        s.failure = (!ok).then(|| (build, Failure::default()));
    }

    fn open_page(s: &mut State, id: &str, build: u64, state: &str) {
        let (tx, rx) = mpsc::channel();
        std::mem::forget(rx); // keep the "connection" open
        s.clients.push(Client { id: s.clients.len() as u64, page: id.into(), tx });
        s.pages.insert(id.into(), PageInfo { num: 1, build, state: state.into(), ..Default::default() });
    }

    #[test]
    fn wait_settles_once_every_open_page_runs_the_latest_build() {
        let mut s = State::default();
        assert_eq!(s.settled(7), Settled::No, "nothing built yet");
        built(&mut s, 1, true);
        assert_eq!(s.settled(7), Settled::Built, "no page open");
        assert_eq!(s.settled(8), Settled::No, "the files changed since that build");
        open_page(&mut s, "a", 0, "booting");
        assert_eq!(s.settled(7), Settled::No);
        s.pages.get_mut("a").unwrap().state = "running".into();
        s.pages.get_mut("a").unwrap().build = 1;
        assert_eq!(s.settled(7), Settled::Running);
        open_page(&mut s, "b", 1, "running");
        built(&mut s, 2, true);
        assert_eq!(s.settled(7), Settled::No, "pages still on build 1");
        s.pages.get_mut("a").unwrap().build = 2;
        s.pages.get_mut("b").unwrap().build = 2;
        assert_eq!(s.settled(7), Settled::Running);
        s.pages.get_mut("b").unwrap().state = "error".into();
        assert_eq!(s.settled(7), Settled::PageError);
        // A closed tab doesn't hold anyone up.
        s.clients.retain(|c| c.page != "b");
        assert_eq!(s.settled(7), Settled::Running);
        s.building = true;
        assert_eq!(s.settled(7), Settled::No);
        built(&mut s, 3, false);
        assert_eq!(s.settled(7), Settled::Failed, "a failed build settles at once");
        assert_eq!(s.status_json(7)["state"], "failed");
    }

    #[test]
    fn hello_carries_the_current_failure() {
        let mut s = State::default();
        built(&mut s, 4, false);
        let h = s.hello("srv");
        assert_eq!((h["type"].as_str(), h["build"].as_u64(), h["state"].as_str()), (Some("hello"), Some(4), Some("failed")));
        assert_eq!(h["failure"]["type"], "failed");
    }

    #[test]
    fn page_reports_update_the_status() {
        let hub = Hub::new(|| 7);
        hub.update(|s| built(s, 1, true));
        hub.report(r#"{"page":"tab1","type":"running","build":1,"frames":0}"#).unwrap();
        hub.update(|s| {
            let (tx, rx) = mpsc::channel();
            std::mem::forget(rx);
            s.clients.push(Client { id: 9, page: "tab1".into(), tx });
        });
        assert_eq!(hub.status()["state"], "running");
        hub.report(r#"{"page":"tab1","type":"error","build":1,"message":"mb_update trapped: unreachable"}"#).unwrap();
        let st = hub.wait(Duration::from_millis(10));
        assert_eq!(st["state"], "error");
        assert_eq!(st["pages"][0]["message"], "mb_update trapped: unreachable");
        assert!(hub.report("{}").is_err());
    }

    #[test]
    fn restores_say_where_the_run_differs_or_that_it_cannot_tell() {
        let note = |v: Value| restored_note(v.as_object().unwrap());
        assert_eq!(note(json!({ "frames": 90, "of": 90, "firstMismatch": 41, "compared": 91 })), " (restored; differs from the old run from frame 41)");
        assert_eq!(note(json!({ "frames": 90, "of": 90, "firstMismatch": null, "compared": 91 })), " (restored; same as the old run)");
        assert_eq!(note(json!({ "frames": 90, "of": 90, "firstMismatch": null, "compared": 0 })), " (restored; no draw hashes were kept to compare)");
        let r = note(json!({ "frames": 50, "of": 90, "firstMismatch": null, "compared": 0, "missingAssets": 2, "why": "replay" }));
        assert!(r.starts_with(" (replayed the run onto this code; the game stopped at frame 50 of 90; 2 asset loads"), "{r}");
        assert!(r.contains("can't be told"), "{r}");
    }
}
