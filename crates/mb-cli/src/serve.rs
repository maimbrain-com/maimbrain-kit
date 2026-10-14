//! `mb serve`: preview a game in a desktop browser (SPEC §9), using the web
//! runtime embedded in this binary.
//!
//! Without --watch, reloading the page rebuilds the game first if any of its
//! files changed, so the loop is: save, reload. With --watch (`watch` below)
//! the game is rebuilt as soon as a file is saved, with the fast build in
//! devbuild.rs, and the open pages are told over a WebSocket (live.rs) to
//! swap in the new code, refetch changed assets, or show the compiler's
//! errors.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use mb_format::Manifest;
use mb_format::bundle::ASSET_EXTENSIONS;
use serde_json::{Value, json};

use crate::devbuild;
use crate::live::{Files, Hub, Outcome, Site, failed_json, status, ws_accept};
use crate::watch::{self, Kind, Snapshot};

macro_rules! runtime_file {
    ($f:literal) => {
        ($f, include_bytes!(concat!(env!("OUT_DIR"), "/runtime/", $f)) as &[u8])
    };
}

const RUNTIME: [(&str, &[u8]); 5] = [
    runtime_file!("index.html"),
    runtime_file!("runtime.js"),
    runtime_file!("probe-worker.js"),
    runtime_file!("mb_host_bg.wasm"),
    runtime_file!("silence.wav"),
];

fn require_runtime() -> Result<(), String> {
    if env!("MB_RUNTIME_EMBEDDED").is_empty() {
        return Err(format!(
            "this mb was built without the web runtime ({} was missing). In the Maimbrain repo, run `just runtime`, \
             then run mb again through cargo (`cargo run -p mb-cli -- serve …`), which embeds it automatically. \
             Otherwise install a release build: curl -fsSL https://maimbrain.com/install.sh | sh",
            env!("MB_RUNTIME_DIST")
        ));
    }
    Ok(())
}

/// How many ports after the one asked for `mb serve` tries when it's taken.
const PORT_TRIES: u16 = 20;

/// Listens on 127.0.0.1:`port`, or on the next free port after it when it's
/// taken (two previews on one machine), saying which; 0 takes any free port.
pub fn listen(port: u16) -> Result<(TcpListener, u16), String> {
    let mut last = String::new();
    let tries = if port == 0 { 1 } else { PORT_TRIES.min(u16::MAX - port + 1) };
    for p in (0..tries).map(|i| port + i) {
        match TcpListener::bind(("127.0.0.1", p)) {
            Ok(server) => {
                let got = server.local_addr().map_err(|e| e.to_string())?.port();
                if got != port && port != 0 {
                    eprintln!("[mb] port {port} is taken (another preview?); using {got}");
                }
                return Ok((server, got));
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("can't listen on ports {port}–{}: {last} (pass --port)", port.saturating_add(tries - 1)))
}

pub fn serve(
    path: &Path,
    port: u16,
    pin: Option<&Pin>,
    build: impl Fn(&Path) -> Result<PathBuf, String> + Sync,
    stage: impl Fn(&Path, &Path, &Path) -> Result<(), String> + Sync,
) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!("mb-serve-{}", std::process::id()));
    let runtime_dir = work.join("runtime-src");
    write_runtime(&runtime_dir)?;
    let site = work.join("site");
    let is_dir = path.is_dir();
    let rebuild = |site: &Path| -> Result<(), String> {
        let bundle = if is_dir { build(path)? } else { path.to_path_buf() };
        stage(&bundle, site, &runtime_dir)
    };
    rebuild(&site)?;
    // When the site was last staged; held while it's staged again, so a page load waits for it.
    let built_at = Mutex::new(SystemTime::now());
    // Files are read under the read side, so none is read half-written.
    let staging = RwLock::new(());

    let (server, port) = listen(port)?;
    let page = page_path(pin);
    eprintln!("\nPreview: http://127.0.0.1:{port}{page}");
    if let Some(p) = pin {
        eprintln!("{}", p.describe(is_dir));
    }
    eprintln!("Reload the page after saving to rebuild. Ctrl-C to stop.\n");
    // Preview saves (`mb.screenshot`, `mb.save`) land here.
    let saves = if is_dir { path.join("screenshots") } else { std::env::current_dir().map_err(|e| e.to_string())?.join("screenshots") };
    serve_http(&server, &|req| {
        let url = req.path().to_string();
        let Some(req) = saves_route(&saves, req) else { return };
        let Some(req) = replay_route(pin, req) else { return };
        if url == "/" || url == "/runtime/index.html" {
            let mut at = built_at.lock().unwrap_or_else(|e| e.into_inner());
            if is_dir && newest_change(path) > *at {
                eprintln!("change detected, rebuilding…");
                let _w = staging.write().unwrap_or_else(|e| e.into_inner());
                match rebuild(&site) {
                    Ok(()) => *at = SystemTime::now(),
                    Err(e) => eprintln!("build failed: {e}"),
                }
            }
            drop(at);
            if url == "/" {
                return req.respond(302, &[("Location", &page)], b"");
            }
        }
        let _r = staging.read().unwrap_or_else(|e| e.into_inner());
        site_file(&site, req);
    });
    Ok(())
}

/// Writes the embedded web runtime (what `mb stage --runtime` takes) into `dir`.
pub fn write_runtime(dir: &Path) -> Result<(), String> {
    require_runtime()?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, bytes) in RUNTIME {
        std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Serves the staged `site` (runtime/ and game/) on 127.0.0.1:`port` (0: any
/// free port) from a background thread, for `mb shot` and `mb measure`.
/// Returns the port.
pub fn serve_site(site: PathBuf, port: u16) -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("can't listen on port {port}: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    std::thread::spawn(move || serve_http(&listener, &|req| site_file(&site, req)));
    Ok(port)
}

/// Answers with the file at the request's path under `site`, or 404.
fn site_file(site: &Path, req: Request) {
    let rel = req.path().trim_start_matches('/').to_string();
    let file = site.join(&rel);
    let bytes = (!rel.split('/').any(|c| c == "..") && file.is_file()).then(|| std::fs::read(&file).ok()).flatten();
    match bytes {
        Some(b) => req.respond(200, &[("Content-Type", content_type(&file)), NO_STORE], &b),
        None => req.text(404, "not found"),
    }
}

// --- the preview's HTTP server ---------------------------------------------------

/// `Cache-Control: no-store`: the preview's files change under the page.
const NO_STORE: (&str, &str) = ("Cache-Control", "no-store");

/// Answers every connection on `listener` with `handle`, forever: a thread per
/// connection, one request each, closed after the response (`Connection:
/// close`), so a long request (`/__mb/wait`, a page's WebSocket) holds only
/// its own thread. (tiny_http's thread pool, used before, could queue a new
/// connection behind keep-alive ones holding its threads; the browser's
/// fetch on it then hung until a socket closed, and a game's assets went
/// missing at random.)
fn serve_http(listener: &TcpListener, handle: &(dyn Fn(Request) + Sync)) {
    std::thread::scope(|s| {
        for stream in listener.incoming().flatten() {
            s.spawn(move || {
                if let Some(req) = Request::read(stream) {
                    handle(req);
                }
            });
        }
    });
}

/// A request read off a connection to the preview's server.
struct Request {
    method: String,
    /// The request target as sent: the path and any query.
    url: String,
    headers: Vec<(String, String)>,
    reader: BufReader<TcpStream>,
}

impl Request {
    /// The request line and headers, or None for a connection that sends none.
    fn read(stream: TcpStream) -> Option<Request> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let (method, url) = (parts.next()?.to_string(), parts.next()?.to_string());
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).ok()? == 0 || headers.len() > 100 {
                return None;
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        Some(Request { method, url, headers, reader })
    }

    /// The path, without the query.
    fn path(&self) -> &str {
        self.url.split('?').next().unwrap_or("/")
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The body, as long as `Content-Length` says (none without one).
    fn body(&mut self) -> impl Read + '_ {
        let n = self.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        (&mut self.reader).take(n)
    }

    fn respond(self, status: u16, headers: &[(&str, &str)], body: &[u8]) {
        let reason = match status {
            200 => "OK",
            204 => "No Content",
            302 => "Found",
            400 => "Bad Request",
            404 => "Not Found",
            503 => "Service Unavailable",
            _ => "",
        };
        let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
        for (k, v) in headers {
            head += &format!("{k}: {v}\r\n");
        }
        head += "\r\n";
        let head_only = self.method == "HEAD";
        let mut stream = self.reader.into_inner();
        let _ = stream.write_all(head.as_bytes());
        if !head_only {
            let _ = stream.write_all(body);
        }
        let _ = stream.flush();
    }

    /// A plain-text answer.
    fn text(self, status: u16, body: &str) {
        self.respond(status, &[("Content-Type", "text/plain; charset=utf-8")], body.as_bytes());
    }

    fn json(self, v: &Value) {
        self.respond(200, &[("Content-Type", "application/json"), NO_STORE], v.to_string().as_bytes());
    }

    /// Accepts a WebSocket (`accept`: its `Sec-WebSocket-Accept`) and hands over the connection.
    fn upgrade(self, accept: &str) -> TcpStream {
        let mut stream = self.reader.into_inner();
        let _ = stream.set_read_timeout(None);
        let head = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
        let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.flush());
        stream
    }
}

/// `mb.screenshot` / `mb.save` (POST) and `mb.load` (GET) at [`SAVE_URL`],
/// in both `mb serve` and `mb serve --watch`; any other request is handed back.
fn saves_route(saves: &Path, mut req: Request) -> Option<Request> {
    if req.path() != SAVE_URL {
        return Some(req);
    }
    if req.method == "POST" {
        let full = req.url.clone();
        let (status, body) = match save(saves, &full, &mut req.body()) {
            Ok(p) => (200, p.display().to_string()),
            Err(e) => (400, e),
        };
        req.text(status, &body);
        return None;
    }
    // `mb.load(name)`: a file saved earlier (e.g. a replay log to play back).
    let name = req.url.split_once("?name=").map(|(_, n)| n.to_string()).unwrap_or_default();
    let ok = !name.is_empty() && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    match ok.then(|| std::fs::read(saves.join(&name))).and_then(Result::ok) {
        Some(bytes) => req.respond(200, &[NO_STORE], &bytes),
        None => req.text(404, "not found"),
    }
    None
}

// --- mb serve --replay <log> --at <frame> -----------------------------------------

/// A run for the preview to replay to a frame and hold there (`mb serve
/// --replay`, docs/UPDATES.md "Feedback reaches the agent"). The page asks
/// for it at [`REPLAY_URL`] when its address says `?replay=play|restore&at=N`
/// (runtime/src/main.ts `pinnedRun`):
/// - `play`: a replay session, the run exactly as recorded, held at the frame
///   (`mb.restart({ replay, hold: true })` and `mb.step(at)`). A bundle, or a
///   game directory without --watch.
/// - `restore`: on a game directory with --watch, the run's input replayed
///   into a live session on the current code, as watch mode's restore does;
///   after each edit the page replays it onto the new code and the terminal
///   says from which frame it differs from the replay before.
#[derive(Debug, Clone)]
pub struct Pin {
    /// The replay log alone (unwrapped from a file that has it under `log`), as JSON.
    pub log: Vec<u8>,
    /// The frame to hold at: frames run (`mb_update` calls), at most the log's.
    pub at: u64,
    /// How many frames the log has.
    pub frames: u64,
    /// The game it's a run of, and the build it was recorded on (when it says).
    pub game: String,
    pub bundle: Option<String>,
    pub restore: bool,
    /// The file it came from, for messages.
    pub name: String,
}

impl Pin {
    /// Reads `file` (a replay log, or JSON with one under `log`) for `mb serve
    /// --replay`, held at `at` (default: its last frame). `restore`: replay it
    /// onto a game directory's code in watch mode.
    pub fn read(file: &Path, at: Option<u64>, restore: bool) -> Result<Pin, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", file.display()))?;
        let log = if v["frames"].is_array() { v } else { v["log"].clone() };
        let (Some(frames), Some(game)) = (log["frames"].as_array().map(Vec::len), log["game"].as_str()) else {
            return Err(format!("{}: not a replay log (no frames or game)", file.display()));
        };
        if !log["seed"].is_string() {
            return Err(format!("{}: not a replay log (no seed)", file.display()));
        }
        let frames = frames as u64;
        let at = at.unwrap_or(frames);
        if at > frames {
            return Err(format!("{}: the run has {frames} frames, so --at {at} is past its end", file.display()));
        }
        Ok(Pin {
            game: game.to_string(),
            bundle: log["bundle"].as_str().map(str::to_string),
            log: serde_json::to_vec(&log).unwrap(),
            at,
            frames,
            restore,
            name: file.display().to_string(),
        })
    }

    /// The page's query: `replay=play&at=1200`; for `restore`, also `restore=on`
    /// so that every edit replays the run onto the new code.
    pub fn query(&self) -> String {
        if self.restore { format!("replay=restore&at={}&restore=on", self.at) } else { format!("replay=play&at={}", self.at) }
    }

    /// What the terminal is told about it.
    pub fn describe(&self, is_dir: bool) -> String {
        let at = format!("frame {} of {} ({} at 60 fps)", self.at, self.frames, crate::playtest::clock(self.at));
        if self.restore {
            format!(
                "Replaying {} onto the current code to {at} and holding there. Where it differs from the run as recorded can't be \
                 told (its log keeps no per-frame draw hashes); after each edit the run is replayed onto the new code, and the \
                 page's line here says from which frame it differs from the replay before.",
                self.name
            )
        } else if is_dir {
            format!(
                "Replaying {} on the current build to {at} and holding there. Where it differs from the run as recorded isn't \
                 reported (no per-frame draw hashes); with --watch, each edit is compared with the replay before it.",
                self.name
            )
        } else {
            format!("Replaying {} to {at} and holding there: what the player saw. In the console, mb.step(1) steps on and mb.resume() plays on.", self.name)
        }
    }
}

/// The preview page's address, with the run to replay if there is one.
pub fn page_path(pin: Option<&Pin>) -> String {
    match pin {
        Some(p) => format!("/runtime/index.html?overlays&{}", p.query()),
        None => "/runtime/index.html?overlays".into(),
    }
}

/// Where the page fetches the run `mb serve --replay` was given.
const REPLAY_URL: &str = "/__mb/replay";

/// Answers [`REPLAY_URL`] with the run (404 without one); any other request is handed back.
fn replay_route(pin: Option<&Pin>, req: Request) -> Option<Request> {
    if req.path() != REPLAY_URL {
        return Some(req);
    }
    match pin {
        Some(p) => req.respond(200, &[("Content-Type", "application/json"), NO_STORE], &p.log),
        None => req.text(404, "no run: mb serve --replay <log> serves one"),
    }
    None
}

/// Where the preview posts files to save (SPEC §9: `mb.screenshot`, `mb.save`).
const SAVE_URL: &str = "/__mb/save";
/// Largest file the preview may save.
const MAX_SAVE: u64 = 32 << 20;

/// Writes a posted file to `dir/<name>`. Names are plain file names
/// (letters, digits, `-`, `_`, `.`), so a page can't write anywhere else.
fn save(dir: &Path, url: &str, body: &mut dyn std::io::Read) -> Result<PathBuf, String> {
    let name = url.split_once("?name=").map(|(_, n)| n).unwrap_or("");
    let ok = !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    if !ok {
        return Err(format!("bad file name {name:?}: use letters, digits, - _ ."));
    }
    let mut data = Vec::new();
    body.take(MAX_SAVE + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_SAVE {
        return Err("file too large".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(name);
    std::fs::write(&file, &data).map_err(|e| format!("{}: {e}", file.display()))?;
    eprintln!("saved {}", file.display());
    Ok(file)
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "glb" => "model/gltf-binary",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// The latest modification time of the game's sources.
fn newest_change(dir: &Path) -> SystemTime {
    let mut newest = SystemTime::UNIX_EPOCH;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') || name == "target" || name == "screenshots" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                newest = newest.max(m);
            }
        }
    }
    newest
}

// --- mb serve --watch ---------------------------------------------------------

/// `mb serve --watch`: serves the runtime and the latest good build from
/// memory (assets straight from the game directory), rebuilds on every save
/// and pushes the result to the open pages. `boot` makes game/boot.json.
pub fn watch(path: &Path, port: u16, pin: Option<&Pin>, boot: fn(&Manifest) -> Value) -> Result<(), String> {
    require_runtime()?;
    let is_dir = path.is_dir();
    if !is_dir && !path.is_file() {
        return Err(format!("{}: no such game directory or bundle", path.display()));
    }
    let scan: Scan = {
        let p = path.to_path_buf();
        if is_dir { Arc::new(move || Snapshot::scan(&p)) } else { Arc::new(move || Snapshot::file(&p)) }
    };
    let hub = {
        let scan = scan.clone();
        Hub::new(move || scan().fingerprint())
    };
    let (server, port) = listen(port)?;
    if is_dir {
        status!("watching {}", watch::describe(path));
    } else {
        status!("watching {}", path.display());
    }
    let page = page_path(pin);
    status!("preview http://127.0.0.1:{port}{page}");
    if let Some(p) = pin {
        status!("{}", p.describe(is_dir));
    }
    status!("status http://127.0.0.1:{port}/__mb/status, wait http://127.0.0.1:{port}/__mb/wait");
    {
        let hub = hub.clone();
        let path = path.to_path_buf();
        // Runs for as long as the server does.
        std::thread::spawn(move || builder(&hub, &path, is_dir, boot, scan));
    }
    // Preview saves (`mb.screenshot`, `mb.save`) land in the game's screenshots/.
    let saves = if is_dir { path.join("screenshots") } else { std::env::current_dir().map_err(|e| e.to_string())?.join("screenshots") };
    serve_http(&server, &|req| {
        if let Some(req) = saves_route(&saves, req).and_then(|r| replay_route(pin, r)) {
            handle(&hub, req, &page);
        }
    });
    Ok(())
}

type Scan = Arc<dyn Fn() -> Snapshot + Send + Sync>;

/// Builds once, then after every debounced batch of changes, forever.
fn builder(hub: &Arc<Hub>, path: &Path, is_dir: bool, boot: fn(&Manifest) -> Value, scan: Scan) {
    let mut b = Builder { hub: hub.clone(), path: path.to_path_buf(), is_dir, boot, n: 0, package: None, page_owed: true };
    let first = scan();
    b.run(Kind::Page, vec![], &first);
    watch::run(&*scan, first, |changed, snap| {
        let kind = if is_dir { watch::classify(&changed) } else { Kind::Page };
        b.run(kind, changed, snap);
    });
}

struct Builder {
    hub: Arc<Hub>,
    path: PathBuf,
    is_dir: bool,
    boot: fn(&Manifest) -> Value,
    n: u64,
    /// The crate's package name (read again when Cargo.toml changes).
    package: Option<String>,
    /// The pages need a full reload with the next good build (the first
    /// one, or after a manifest change that didn't build).
    page_owed: bool,
}

impl Builder {
    fn run(&mut self, kind: Kind, changed: Vec<String>, snap: &Snapshot) {
        self.n += 1;
        let n = self.n;
        let started = Instant::now();
        let edited = changed.iter().filter_map(|p| std::fs::metadata(self.file(p)).and_then(|m| m.modified()).ok()).max();
        let what = if changed.is_empty() { "first build".to_string() } else { short_list(&changed) };
        status!("build {n} started: {what}");
        self.hub.update(|s| {
            s.building = true;
            s.build = n;
        });
        self.hub.broadcast(&json!({ "type": "building", "build": n, "changed": changed }));
        let fingerprint = snap.fingerprint();
        let outcome = |kind: &'static str, ok: bool| Outcome { build: n, kind, ok, secs: started.elapsed().as_secs_f64(), finished: Instant::now(), edited };

        if let Kind::Assets(paths) = kind {
            let o = outcome("assets", true);
            status!("build {n} ok {:.2}s assets: {}", o.secs, short_list(&paths));
            self.hub.update(|s| {
                s.building = false;
                s.fingerprint = fingerprint;
                s.last = Some(o);
            });
            self.hub.broadcast(&json!({ "type": "assets", "build": n, "paths": paths }));
            return;
        }
        if changed.iter().any(|p| p == "Cargo.toml") {
            self.package = None;
        }
        let page = kind == Kind::Page || self.page_owed;
        let reload = if page { "page" } else { "code" };
        match self.site(n) {
            Ok((site, cargo_s)) => {
                let o = outcome(reload, true);
                let kib = site.wasm.len() / 1024;
                let cargo = if self.is_dir { format!("cargo {cargo_s:.2}s, ") } else { String::new() };
                status!("build {n} ok {:.2}s {reload} ({cargo}wasm {kib} KiB)", o.secs);
                self.page_owed = false;
                self.hub.update(|s| {
                    s.building = false;
                    s.fingerprint = fingerprint;
                    s.code_build = n;
                    s.failure = None;
                    s.site = Some(Arc::new(site));
                    s.last = Some(o);
                });
                self.hub.broadcast(&json!({ "type": "reload", "build": n, "codeBuild": n, "kind": reload }));
            }
            Err(f) => {
                let o = outcome(reload, false);
                status!("build {n} failed {:.2}s: {}", o.secs, f.summary());
                self.page_owed |= page;
                let msg = failed_json(n, o.secs, &f);
                self.hub.update(|s| {
                    s.building = false;
                    s.fingerprint = fingerprint;
                    s.failure = Some((n, f));
                    s.last = Some(o);
                });
                self.hub.broadcast(&msg);
            }
        }
    }

    fn file(&self, rel: &str) -> PathBuf {
        if self.is_dir { self.path.join(rel) } else { self.path.clone() }
    }

    /// Builds (or, for a .mbx, opens) the game: what to serve, and the seconds spent in cargo.
    fn site(&mut self, n: u64) -> Result<(Site, f64), devbuild::Failure> {
        let (manifest, wasm, files, cargo_s) = if self.is_dir {
            let name = match &self.package {
                Some(n) => n.clone(),
                None => crate::package_name(&self.path).map_err(|e| devbuild::Failure::message("Cargo.toml", e))?,
            };
            self.package = Some(name.clone());
            let b = {
                let _lock = devbuild::lock(&self.path);
                devbuild::build(&self.path, &name)?
            };
            (b.manifest, b.wasm, Files::Dir(self.path.clone()), b.cargo_s)
        } else {
            let fail = |e: String| devbuild::Failure::message("", e);
            let bytes = std::fs::read(&self.path).map_err(|e| fail(format!("{}: {e}", self.path.display())))?;
            let b = mb_format::Bundle::open(&bytes).map_err(|r| fail(format!("{} is invalid: {}", self.path.display(), r.errors.join("; "))))?;
            let wasm = b.runtime_wasm().to_vec();
            (b.manifest.clone(), wasm, Files::Bundle(Arc::new(b)), 0.0)
        };
        let mut boot = (self.boot)(&manifest);
        // Lets a page tell whether it runs this server's latest build (runtime/src/dev.ts).
        boot["dev"] = json!({ "server": self.hub.server, "build": n });
        let boot = serde_json::to_vec_pretty(&boot).unwrap();
        Ok((Site { wasm: Arc::new(wasm), boot: Arc::new(boot), files }, cargo_s))
    }
}

/// "src/sim.rs, src/lib.rs" or "src/sim.rs, src/lib.rs and 4 more".
fn short_list(paths: &[String]) -> String {
    match paths.len() {
        0..=3 => paths.join(", "),
        n => format!("{} and {} more", paths[..2].join(", "), n - 2),
    }
}

fn query<'a>(q: &'a str, key: &str) -> Option<&'a str> {
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn handle(hub: &Arc<Hub>, mut req: Request, page: &str) {
    let full = req.url.clone();
    let (url, q) = full.split_once('?').unwrap_or((&full, ""));
    match url {
        "/" => req.respond(302, &[("Location", page)], b""),
        "/__mb/ws" => {
            let Some(key) = req.header("Sec-WebSocket-Key").map(str::to_string) else { return req.text(404, "websocket only") };
            let page = query(q, "page").filter(|p| !p.is_empty() && p.len() <= 64 && p.bytes().all(|b| b.is_ascii_alphanumeric()));
            let page = page.unwrap_or("anon").to_string();
            let stream = req.upgrade(&ws_accept(&key));
            hub.serve_socket(page, Box::new(stream));
        }
        "/__mb/status" => req.json(&hub.status()),
        "/__mb/wait" => {
            // Its own connection's thread: waiting holds nothing else up.
            let secs = query(q, "timeout").and_then(|t| t.parse::<f64>().ok()).unwrap_or(60.0).clamp(0.0, 600.0);
            let v = hub.wait(Duration::from_secs_f64(secs));
            req.json(&v);
        }
        "/__mb/page" => {
            let mut body = String::new();
            let _ = req.body().take(64 * 1024).read_to_string(&mut body);
            match hub.report(&body) {
                Ok(()) => req.respond(204, &[], b""),
                Err(e) => req.text(400, &e),
            }
        }
        "/runtime/index.html" => {
            let host = req.header("Host").filter(|h| h.bytes().all(|b| b.is_ascii_alphanumeric() || b".:-[]".contains(&b)));
            let html = dev_index(host.unwrap_or("127.0.0.1"));
            req.respond(200, &[("Content-Type", "text/html; charset=utf-8"), NO_STORE], html.as_bytes());
        }
        _ => {
            let bytes = if let Some(name) = url.strip_prefix("/runtime/") {
                RUNTIME.iter().find(|(n, _)| *n == name).map(|(_, b)| b.to_vec())
            } else if let Some(rel) = url.strip_prefix("/game/") {
                let Some(site) = hub.lock().site.clone() else {
                    return req.text(503, "the first build hasn't finished");
                };
                match rel {
                    "boot.json" => Some(site.boot.to_vec()),
                    "game.wasm" => Some(site.wasm.to_vec()),
                    _ => game_file(&site, rel),
                }
            } else {
                None
            };
            match bytes {
                Some(b) => req.respond(200, &[("Content-Type", content_type(Path::new(url))), NO_STORE], &b),
                None => req.text(404, "not found"),
            }
        }
    }
}

/// A bundle file other than game.wasm and boot.json: from disk for a game
/// directory (an asset a bundle would carry), from the bundle for a .mbx.
fn game_file(site: &Site, rel: &str) -> Option<Vec<u8>> {
    // The runtime's own rule for asset paths (session.ts), and no hidden files.
    let ok = rel.bytes().all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b)) && rel.split('/').all(|c| !c.is_empty() && !c.starts_with('.'));
    if !ok {
        return None;
    }
    match &site.files {
        Files::Bundle(b) => b.file(rel).map(<[u8]>::to_vec),
        Files::Dir(root) if rel == "manifest.toml" => std::fs::read(root.join(rel)).ok(),
        Files::Dir(root) => {
            let ext = Path::new(rel).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            let asset = rel.starts_with("assets/") && ext.is_some_and(|e| ASSET_EXTENSIONS.contains(&e.as_str()));
            if asset { std::fs::read(root.join(rel)).ok() } else { None }
        }
    }
}

/// The runtime's page, marked as a watch-mode preview (runtime/src/dev.ts
/// connects back only then) and allowed to open a WebSocket to `host`.
fn dev_index(host: &str) -> String {
    let html = String::from_utf8_lossy(RUNTIME[0].1).into_owned();
    let html = html.replace("connect-src 'self'", &format!("connect-src 'self' ws://{host}"));
    let charset = "<meta charset=\"utf-8\">";
    let meta = "<meta name=\"mb-dev\" content=\"watch\">";
    match html.find(charset) {
        Some(i) => format!("{}\n{meta}{}", &html[..i + charset.len()], &html[i + charset.len()..]),
        None => format!("{meta}\n{html}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries() {
        assert_eq!(query("page=ab12&timeout=5", "timeout"), Some("5"));
        assert_eq!(query("page=ab12", "timeout"), None);
        assert_eq!(query("", "page"), None);
    }

    #[test]
    fn short_lists() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(short_list(&p(&["src/lib.rs"])), "src/lib.rs");
        assert_eq!(short_list(&p(&["a", "b", "c", "d", "e"])), "a, b and 3 more");
    }

    #[test]
    fn a_pinned_run_is_unwrapped_and_asked_for_by_the_page() {
        let dir = std::env::temp_dir().join(format!("mb-serve-pin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = json!({ "version": 1, "game": "dev.x.y", "seed": "1", "bundle": "ab", "frames": [{ "dt": 0.016 }, { "dt": 0.016 }, { "dt": 0.016 }] });
        std::fs::write(dir.join("wrapped.json"), json!({ "note": "hi", "log": log }).to_string()).unwrap();
        let p = Pin::read(&dir.join("wrapped.json"), Some(2), false).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&p.log).unwrap(), log);
        assert_eq!((p.at, p.frames, p.game.as_str(), p.bundle.as_deref()), (2, 3, "dev.x.y", Some("ab")));
        assert_eq!(page_path(Some(&p)), "/runtime/index.html?overlays&replay=play&at=2");
        let p = Pin::read(&dir.join("wrapped.json"), None, true).unwrap();
        assert_eq!(page_path(Some(&p)), "/runtime/index.html?overlays&replay=restore&at=3&restore=on", "default: its last frame");
        assert_eq!(page_path(None), "/runtime/index.html?overlays");
        assert!(Pin::read(&dir.join("wrapped.json"), Some(4), false).unwrap_err().contains("the run has 3 frames, so --at 4 is past its end"));
        std::fs::write(dir.join("not.json"), r#"{"frames":[]}"#).unwrap();
        assert!(Pin::read(&dir.join("not.json"), None, false).unwrap_err().contains("not a replay log"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_taken_port_moves_to_the_next_free_one() {
        // Ports 8820–8823 are this crate's to test with.
        let held = std::net::TcpListener::bind(("127.0.0.1", 8821)).expect("port 8821 for the test");
        let (_s, port) = listen(8821).unwrap();
        assert!(port > 8821 && port < 8821 + PORT_TRIES, "{port}");
        drop(held);
        let (_s, any) = listen(0).unwrap();
        assert_ne!(any, 0, "0 takes any free port");
    }

    #[test]
    fn dev_index_marks_the_page_and_allows_the_socket() {
        if env!("MB_RUNTIME_EMBEDDED").is_empty() {
            return; // built without the runtime: nothing to check
        }
        let html = dev_index("127.0.0.1:8765");
        assert!(html.contains("<meta name=\"mb-dev\" content=\"watch\">"));
        assert!(html.contains("connect-src 'self' ws://127.0.0.1:8765"));
    }

    #[test]
    fn game_files_follow_the_bundle_rules() {
        let dir = std::env::temp_dir().join(format!("mb-serve-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/hero.png"), b"png").unwrap();
        std::fs::write(dir.join("assets/notes.psd"), b"psd").unwrap();
        std::fs::write(dir.join("secret.txt"), b"no").unwrap();
        let site = Site { wasm: Arc::new(vec![]), boot: Arc::new(vec![]), files: Files::Dir(dir.clone()) };
        assert_eq!(game_file(&site, "assets/hero.png").as_deref(), Some(&b"png"[..]));
        assert_eq!(game_file(&site, "assets/notes.psd"), None, "not an asset type a bundle carries");
        assert_eq!(game_file(&site, "secret.txt"), None);
        assert_eq!(game_file(&site, "assets/../secret.txt"), None);
        assert_eq!(game_file(&site, "assets//hero.png"), None);
        assert_eq!(game_file(&site, "assets/.hero.png"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
