//! `mb describe` against a fake Maimbrain server on localhost: which game it
//! means, where the text comes from, and what it sends.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    url: String,
    body: serde_json::Value,
}

/// Lists two uploads (frogger and wobble) for /me/games and answers a PATCH of
/// a game with what it was sent, or with `refuse` (a 422 message) if given.
struct FakeServer {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl FakeServer {
    fn start(refuse: Option<&'static str>) -> FakeServer {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let url = req.url().to_string();
                let method = req.method().to_string();
                let body: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                log.lock().unwrap().push(Seen { method: method.clone(), url: url.clone(), body: body.clone() });
                let (code, json) = match (method.as_str(), url.as_str()) {
                    ("GET", "/api/v1/me/games") => (
                        200,
                        serde_json::json!([
                            { "id": 2, "game_id": "com.maimbrain.ada.wobble", "version": "0.2.0", "state": "draft" },
                            { "id": 1, "game_id": "com.maimbrain.ada.frogger", "version": "0.1.0", "state": "approved" }
                        ]),
                    ),
                    ("PATCH", u) if u.starts_with("/api/v1/games/") => match refuse {
                        Some(msg) => (422, serde_json::json!({ "error": msg })),
                        None => (200, serde_json::json!({ "game_id": &u["/api/v1/games/".len()..], "description": body["description"] })),
                    },
                    _ => (404, serde_json::json!({ "error": "not found" })),
                };
                let resp = tiny_http::Response::from_string(json.to_string())
                    .with_status_code(code)
                    .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap());
                let _ = req.respond(resp);
            }
        });
        FakeServer { url: format!("http://127.0.0.1:{port}"), seen }
    }

    fn patches(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().iter().filter(|s| s.method == "PATCH").cloned().collect()
    }
}

/// A signed-in config and a game directory for com.maimbrain.ada.wobble.
fn setup(name: &str, server: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-describe-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("config")).unwrap();
    std::fs::create_dir_all(d.join("wobble")).unwrap();
    std::fs::write(d.join("config/credentials.json"), serde_json::json!({ "server": server, "token": "tok-123", "username": "ada" }).to_string()).unwrap();
    std::fs::write(
        d.join("wobble/manifest.toml"),
        "abi = 0\nid = \"com.maimbrain.ada.wobble\"\nname = \"Wobble\"\nversion = \"0.2.0\"\ncreator = \"@ada\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nstdlib = { mb2d = 1 }\n",
    )
    .unwrap();
    d
}

fn mb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mb")).args(args).current_dir(dir).env("MB_CONFIG_DIR", dir.join("config")).env_remove("MB_SERVER").output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn sets_the_text_for_a_game_named_by_its_directory_or_its_name() {
    let s = FakeServer::start(None);
    let d = setup("text", &s.url);
    let o = mb(&d, &["describe", "wobble", "  A clown on a unicycle, riding the rooftops at sunset. Tip to balance.\n"]);
    let e = stderr(&o);
    assert!(o.status.success(), "{e}");
    assert!(e.contains("✓ Described com.maimbrain.ada.wobble. Its card in the feed starts: A clown on a unicycle, riding the roofto…"), "{e}");
    let o = mb(&d, &["describe", "frogger", "Catch flies."]);
    assert!(o.status.success(), "{}", stderr(&o));
    let p = s.patches();
    assert_eq!(p.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(), ["/api/v1/games/com.maimbrain.ada.wobble", "/api/v1/games/com.maimbrain.ada.frogger"]);
    assert_eq!(p[0].body["description"], "A clown on a unicycle, riding the rooftops at sunset. Tip to balance.");
    assert_eq!(p[1].body["description"], "Catch flies.");
}

#[test]
fn reads_description_txt_or_a_file_and_clears() {
    let s = FakeServer::start(None);
    let d = setup("file", &s.url);
    let o = mb(&d, &["describe", "wobble"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("DESCRIPTION.txt"), "{}", stderr(&o));
    std::fs::write(d.join("wobble/DESCRIPTION.txt"), "Tip the phone.\nSlide to speed.\n").unwrap();
    assert!(mb(&d, &["describe", "wobble"]).status.success());
    std::fs::write(d.join("other.txt"), "From a file.").unwrap();
    assert!(mb(&d, &["describe", "com.maimbrain.ada.wobble", "--file", "other.txt"]).status.success());
    let o = mb(&d, &["describe", "wobble", "--clear"]);
    assert!(stderr(&o).contains("✓ Removed the description of com.maimbrain.ada.wobble."), "{}", stderr(&o));
    let bodies: Vec<serde_json::Value> = s.patches().into_iter().map(|r| r.body["description"].clone()).collect();
    assert_eq!(bodies, [serde_json::json!("Tip the phone.\nSlide to speed."), serde_json::json!("From a file."), serde_json::Value::Null]);
}

#[test]
fn refuses_unknown_games_empty_text_and_says_why_the_server_did() {
    let s = FakeServer::start(Some("Description can't contain links"));
    let d = setup("refuse", &s.url);
    let o = mb(&d, &["describe", "pong", "A paddle game."]);
    assert!(stderr(&o).contains("you haven't uploaded pong"), "{}", stderr(&o));
    let o = mb(&d, &["describe", "wobble", "   "]);
    assert!(stderr(&o).contains("--clear"), "{}", stderr(&o));
    let o = mb(&d, &["describe", "wobble", "Tips at paypal.me/ada"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("couldn't describe com.maimbrain.ada.wobble: Description can't contain links"), "{}", stderr(&o));
    assert_eq!(s.patches().len(), 1);
}
