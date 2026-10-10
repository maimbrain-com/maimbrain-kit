//! `mb serve`: preview a game in a desktop browser (SPEC §9), using the web
//! runtime embedded in this binary.
//!
//! Without --watch, reloading the page rebuilds the game first if any of its
//! files changed, so the loop is: save, reload. With --watch (`watch` below)
//! the game is rebuilt as soon as a file is saved, with the fast build in
//! devbuild.rs, and the open pages are told over a WebSocket (live.rs) to
//! swap in the new code, refetch changed assets, or show the compiler's
//! errors.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
pub fn listen(port: u16) -> Result<(tiny_http::Server, u16), String> {
    let mut last = String::new();
    let tries = if port == 0 { 1 } else { PORT_TRIES.min(u16::MAX - port + 1) };
    for p in (0..tries).map(|i| port + i) {
        match tiny_http::Server::http(("127.0.0.1", p)) {
            Ok(server) => {
                let got = server.server_addr().to_ip().map(|a| a.port()).ok_or("the server has no TCP address")?;
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
    build: impl Fn(&Path) -> Result<PathBuf, String>,
    stage: impl Fn(&Path, &Path, &Path) -> Result<(), String>,
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
    let mut built_at = SystemTime::now();

    let (server, port) = listen(port)?;
    eprintln!("\nPreview: http://127.0.0.1:{port}/runtime/index.html?overlays");
    eprintln!("Reload the page after saving to rebuild. Ctrl-C to stop.\n");
    // Preview saves (`mb.screenshot`, `mb.save`) land here.
    let saves = if is_dir { path.join("screenshots") } else { std::env::current_dir().map_err(|e| e.to_string())?.join("screenshots") };
    for req in server.incoming_requests() {
        let url = req.url().split('?').next().unwrap_or("/").to_string();
        let Some(req) = saves_route(&saves, req) else { continue };
        if url == "/" || url == "/runtime/index.html" {
            if is_dir && newest_change(path) > built_at {
                eprintln!("change detected, rebuilding…");
                match rebuild(&site) {
                    Ok(()) => built_at = SystemTime::now(),
                    Err(e) => eprintln!("build failed: {e}"),
                }
            }
            if url == "/" {
                let r = tiny_http::Response::empty(302).with_header(header("Location", "/runtime/index.html?overlays"));
                let _ = req.respond(r);
                continue;
            }
        }
        site_file(&site, &url, req);
    }
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
/// free port) from a background thread, for `mb shot`. Returns the port.
pub fn serve_site(site: PathBuf, port: u16) -> Result<u16, String> {
    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| format!("can't listen on port {port}: {e}"))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).ok_or("the server has no TCP address")?;
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let url = req.url().split('?').next().unwrap_or("/").to_string();
            site_file(&site, &url, req);
        }
    });
    Ok(port)
}

/// Answers with the file at `url` under `site`, or 404.
fn site_file(site: &Path, url: &str, req: tiny_http::Request) {
    let rel = url.trim_start_matches('/');
    let file = site.join(rel);
    if rel.split('/').any(|c| c == "..") || !file.is_file() {
        let _ = req.respond(tiny_http::Response::from_string("not found").with_status_code(404));
        return;
    }
    let bytes = std::fs::read(&file).unwrap_or_default();
    let r = tiny_http::Response::from_data(bytes)
        .with_header(header("Content-Type", content_type(&file)))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(r);
}

/// `mb.screenshot` / `mb.save` (POST) and `mb.load` (GET) at [`SAVE_URL`],
/// in both `mb serve` and `mb serve --watch`; any other request is handed back.
fn saves_route(saves: &Path, mut req: tiny_http::Request) -> Option<tiny_http::Request> {
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    if url != SAVE_URL {
        return Some(req);
    }
    if *req.method() == tiny_http::Method::Post {
        let full = req.url().to_string();
        let (status, body) = match save(saves, &full, req.as_reader()) {
            Ok(p) => (200, p.display().to_string()),
            Err(e) => (400, e),
        };
        let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(status));
        return None;
    }
    // `mb.load(name)`: a file saved earlier (e.g. a replay log to play back).
    let name = req.url().split_once("?name=").map(|(_, n)| n.to_string()).unwrap_or_default();
    let ok = !name.is_empty() && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    let r = match ok.then(|| std::fs::read(saves.join(&name))).and_then(Result::ok) {
        Some(bytes) => tiny_http::Response::from_data(bytes).with_header(header("Cache-Control", "no-store")),
        None => tiny_http::Response::from_data(b"not found".to_vec()).with_status_code(404),
    };
    let _ = req.respond(r);
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

fn header(k: &str, v: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
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
pub fn watch(path: &Path, port: u16, boot: fn(&Manifest) -> Value) -> Result<(), String> {
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
    status!("preview http://127.0.0.1:{port}/runtime/index.html?overlays");
    status!("status http://127.0.0.1:{port}/__mb/status, wait http://127.0.0.1:{port}/__mb/wait");
    {
        let hub = hub.clone();
        let path = path.to_path_buf();
        // Runs for as long as the server does.
        std::thread::spawn(move || builder(&hub, &path, is_dir, boot, scan));
    }
    // Preview saves (`mb.screenshot`, `mb.save`) land in the game's screenshots/.
    let saves = if is_dir { path.join("screenshots") } else { std::env::current_dir().map_err(|e| e.to_string())?.join("screenshots") };
    for req in server.incoming_requests() {
        if let Some(req) = saves_route(&saves, req) {
            handle(&hub, req);
        }
    }
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

fn json_response(v: &Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(v.to_string())
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn not_found(req: tiny_http::Request, why: &str) {
    let _ = req.respond(tiny_http::Response::from_string(why).with_status_code(404));
}

fn header_value(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_string())
}

fn handle(hub: &Arc<Hub>, mut req: tiny_http::Request) {
    let full = req.url().to_string();
    let (url, q) = full.split_once('?').unwrap_or((&full, ""));
    match url {
        "/" => {
            let _ = req.respond(tiny_http::Response::empty(302).with_header(header("Location", "/runtime/index.html?overlays")));
        }
        "/__mb/ws" => {
            let Some(key) = header_value(&req, "Sec-WebSocket-Key") else { return not_found(req, "websocket only") };
            let page = query(q, "page").filter(|p| !p.is_empty() && p.len() <= 64 && p.bytes().all(|b| b.is_ascii_alphanumeric()));
            let page = page.unwrap_or("anon").to_string();
            let resp = tiny_http::Response::empty(101).with_header(header("Sec-WebSocket-Accept", &ws_accept(&key)));
            let stream = req.upgrade("websocket", resp);
            hub.serve_socket(page, stream);
        }
        "/__mb/status" => {
            let _ = req.respond(json_response(&hub.status()));
        }
        "/__mb/wait" => {
            let secs = query(q, "timeout").and_then(|t| t.parse::<f64>().ok()).unwrap_or(60.0).clamp(0.0, 600.0);
            let hub = hub.clone();
            std::thread::spawn(move || {
                let v = hub.wait(Duration::from_secs_f64(secs));
                let _ = req.respond(json_response(&v));
            });
        }
        "/__mb/page" => {
            let mut body = String::new();
            let _ = req.as_reader().take(64 * 1024).read_to_string(&mut body);
            let r = match hub.report(&body) {
                Ok(()) => tiny_http::Response::empty(204).boxed(),
                Err(e) => tiny_http::Response::from_string(e).with_status_code(400).boxed(),
            };
            let _ = req.respond(r);
        }
        "/runtime/index.html" => {
            let host = header_value(&req, "Host").filter(|h| h.bytes().all(|b| b.is_ascii_alphanumeric() || b".:-[]".contains(&b)));
            let html = dev_index(host.as_deref().unwrap_or("127.0.0.1"));
            let r = tiny_http::Response::from_string(html)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = req.respond(r);
        }
        _ => {
            let bytes = if let Some(name) = url.strip_prefix("/runtime/") {
                RUNTIME.iter().find(|(n, _)| *n == name).map(|(_, b)| b.to_vec())
            } else if let Some(rel) = url.strip_prefix("/game/") {
                let Some(site) = hub.lock().site.clone() else {
                    let _ = req.respond(tiny_http::Response::from_string("the first build hasn't finished").with_status_code(503));
                    return;
                };
                match rel {
                    "boot.json" => Some(site.boot.to_vec()),
                    "game.wasm" => Some(site.wasm.to_vec()),
                    _ => game_file(&site, rel),
                }
            } else {
                None
            };
            let Some(bytes) = bytes else { return not_found(req, "not found") };
            let r = tiny_http::Response::from_data(bytes)
                .with_header(header("Content-Type", content_type(Path::new(url))))
                .with_header(header("Cache-Control", "no-store"));
            let _ = req.respond(r);
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
