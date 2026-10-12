//! Which keys `mb art` uses, against a fake Maimbrain server on localhost:
//! your own key first, then Maimbrain's keys when the server says your account
//! has access, else a message saying how to get either. Never reaches a real
//! provider or server.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use base64::Engine;

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    url: String,
    auth: String,
    body: serde_json::Value,
}

/// A fake server: answers /generate/status with `status` and /generate/image with
/// a tiny PNG (or `image_error`), recording every request.
struct FakeServer {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        let data: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, 40, 90]).collect();
        wr.write_image_data(&data).unwrap();
    }
    out
}

impl FakeServer {
    fn start(status: serde_json::Value, image_error: Option<(u16, &'static str)>) -> FakeServer {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string()).unwrap_or_default();
                let url = req.url().to_string();
                log.lock().unwrap().push(Seen {
                    method: req.method().to_string(),
                    url: url.clone(),
                    auth: auth.clone(),
                    body: serde_json::from_str(&body).unwrap_or_default(),
                });
                let (code, json) = if auth != "Bearer tok-123" {
                    (401, serde_json::json!({ "error": "sign in required" }))
                } else if url == "/api/v1/generate/status" {
                    (200, status.clone())
                } else if url == "/api/v1/generate/image" {
                    match image_error {
                        Some((code, msg)) => (code, serde_json::json!({ "error": msg })),
                        None => (
                            200,
                            serde_json::json!({
                                "data": base64::engine::general_purpose::STANDARD.encode(png_bytes(90, 160)),
                                "mime": "image/png", "cost_usd": 0.0336,
                                "usage": { "used_usd": 1.0336, "budget_usd": 10.0, "remaining_usd": 8.9664 }
                            }),
                        ),
                    }
                } else {
                    (404, serde_json::json!({ "error": "not found" }))
                };
                let resp = tiny_http::Response::from_string(json.to_string())
                    .with_status_code(code)
                    .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap());
                let _ = req.respond(resp);
            }
        });
        FakeServer { url: format!("http://127.0.0.1:{port}"), seen }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn allowed() -> serde_json::Value {
    serde_json::json!({
        "allowed": true, "username": "ada", "budget_usd": 10.0, "used_usd": 1.0, "remaining_usd": 9.0,
        "resets_at": "2026-11-01T00:00:00Z",
        "providers": [
            { "name": "gemini", "models": [ { "id": "gemini-nano-banana-2.1", "kind": "image" }, { "id": "lyria-3-clip-preview", "kind": "music" } ] },
            { "name": "elevenlabs", "models": [ { "id": "eleven_text_to_sound_v2", "kind": "sfx" } ] }
        ]
    })
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-mkeys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("g/assets")).unwrap();
    std::fs::write(d.join("g/manifest.toml"), "abi = 0\nid = \"com.example.g\"\nname = \"G\"\nversion = \"0.1.0\"\ncreator = \"@t\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nstdlib = { mb2d = 1 }\n").unwrap();
    d
}

/// Signs `dir`'s config in to `server` (what `mb login` leaves behind).
fn sign_in(dir: &Path, server: &str) {
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(dir.join("config/credentials.json"), serde_json::json!({ "server": server, "token": "tok-123", "username": "ada" }).to_string()).unwrap();
}

fn mb(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_mb"));
    c.args(args)
        .current_dir(dir)
        .env("MB_KEYS_FILE", dir.join("keys.json"))
        .env("MB_CONFIG_DIR", dir.join("config"))
        .env("MB_GEN_DATE", "2026-10-07T00:00:00Z");
    for k in [
        "MB_PROVIDER",
        "MB_KEYS_SOURCE",
        "MB_SERVER",
        "MB_MAX_COST",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_CLOUD_PROJECT",
        "OPENAI_API_KEY",
        "ELEVENLABS_API_KEY",
        "XI_API_KEY",
    ] {
        c.env_remove(k);
    }
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

const BG: &[&str] = &["art", "background", "g", "sky", "a starry night over hills", "--seed", "5"];

#[test]
fn without_a_key_an_allowed_account_generates_on_maimbrains_keys() {
    let s = FakeServer::start(allowed(), None);
    let d = tmp("allowed");
    sign_in(&d, &s.url);
    let o = mb(&d, BG, &[]);
    let e = stderr(&o);
    assert!(o.status.success(), "{e}");
    assert!(e.contains("keys: Maimbrain's (@ada) · $1.00 of $10.00 used this month"), "{e}");
    assert!(e.contains("Maimbrain's keys: $1.03 of $10.00 used this month"), "{e}");
    let sc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join("g/assets/sky.png.gen.json")).unwrap()).unwrap();
    assert_eq!(sc["provider"], "gemini");
    assert_eq!(sc["via"], "maimbrain");
    assert_eq!(sc["cost_usd"], 0.0336);
    let seen = s.seen();
    assert_eq!(seen.iter().map(|r| (r.method.as_str(), r.url.as_str())).collect::<Vec<_>>(), [
        ("GET", "/api/v1/generate/status"),
        ("POST", "/api/v1/generate/image")
    ]);
    let body = &seen[1].body;
    assert_eq!(seen[1].auth, "Bearer tok-123");
    assert_eq!(body["provider"], "gemini");
    assert_eq!(body["model"], "gemini-nano-banana-2.1");
    assert_eq!(body["seed"], 5);
    assert_eq!(body["aspect"], "9:16");
    assert!(body["prompt"].as_str().unwrap().contains("a starry night over hills"));
    // Reprocessing keeps how it was made, and asks nobody.
    let o = mb(&d, &["regen", "g/assets/sky.png", "--reprocess"], &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let sc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join("g/assets/sky.png.gen.json")).unwrap()).unwrap();
    assert_eq!(sc["via"], "maimbrain");
    assert_eq!(s.seen().len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_account_without_access_gets_the_set_a_key_message() {
    let s = FakeServer::start(serde_json::json!({ "allowed": false, "username": "ada", "message": "no access" }), None);
    let d = tmp("denied");
    sign_in(&d, &s.url);
    let o = mb(&d, BG, &[]);
    let e = stderr(&o);
    assert!(!o.status.success());
    assert!(e.contains("GEMINI_API_KEY") && e.contains("ask the Maimbrain team for access to its keys (@ada doesn't have it)"), "{e}");
    assert!(s.seen().iter().all(|r| r.url == "/api/v1/generate/status"), "no generation call");
    // Forcing it: the server's own words.
    let e = stderr(&mb(&d, &[BG, &["--provider", "maimbrain"]].concat(), &[]));
    assert!(e.contains("no access"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn own_keys_never_asks_the_server_and_maimbrain_skips_your_key() {
    let s = FakeServer::start(allowed(), None);
    let d = tmp("modes");
    sign_in(&d, &s.url);
    let o = mb(&d, &[BG, &["--own-keys"]].concat(), &[]);
    let e = stderr(&o);
    assert!(!o.status.success() && e.contains("GEMINI_API_KEY") && !e.contains("Maimbrain team"), "{e}");
    let o = mb(&d, BG, &[("MB_KEYS_SOURCE", "own")]);
    assert!(!o.status.success());
    assert!(s.seen().is_empty(), "--own-keys and MB_KEYS_SOURCE=own don't contact the server");
    // With a key of your own, --provider maimbrain still goes through the server (not Google).
    let o = mb(&d, &[BG, &["--provider", "maimbrain"]].concat(), &[("GEMINI_API_KEY", "AIza-not-a-real-key-000")]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(s.seen().iter().any(|r| r.url == "/api/v1/generate/image"));
    assert!(stderr(&mb(&d, &[BG, &["--provider", "maimbrain", "--own-keys"]].concat(), &[])).contains("pick one"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn signed_out_the_message_mentions_mb_login() {
    let d = tmp("signedout");
    let e = stderr(&mb(&d, BG, &[]));
    assert!(e.contains("GEMINI_API_KEY") && e.contains("ask the Maimbrain team") && e.contains("mb login"), "{e}");
    assert!(e.find("mb login") < e.find("GEMINI_API_KEY"), "signing in comes before setting a key: {e}");
    let e = stderr(&mb(&d, &[BG, &["--provider", "maimbrain"]].concat(), &[]));
    assert!(e.contains("run mb login"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn budget_and_server_refusals_are_shown() {
    let s = FakeServer::start(allowed(), Some((402, "this would go over your $10.00 monthly budget on Maimbrain's keys ($0.01 left)")));
    let d = tmp("budget");
    sign_in(&d, &s.url);
    let e = stderr(&mb(&d, BG, &[]));
    assert!(e.contains("monthly budget") && e.contains("$0.01 left"), "{e}");
    assert!(!d.join("g/assets/sky.png").exists());
    // The CLI refuses up front when its own estimate is over what's left.
    let mut low = allowed();
    low["remaining_usd"] = serde_json::json!(0.01);
    let s2 = FakeServer::start(low, None);
    sign_in(&d, &s2.url);
    let e = stderr(&mb(&d, BG, &[]));
    assert!(e.contains("left of your $10.00 monthly budget"), "{e}");
    assert!(s2.seen().iter().all(|r| r.url == "/api/v1/generate/status"));
    // Models Maimbrain's keys don't cover.
    let e = stderr(&mb(&d, &[BG, &["--model", "gemini-3-pro-image"]].concat(), &[]));
    assert!(e.contains("don't cover gemini gemini-3-pro-image"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn models_and_doctor_show_maimbrains_keys() {
    let s = FakeServer::start(allowed(), None);
    let d = tmp("models");
    sign_in(&d, &s.url);
    let e = stderr(&mb(&d, &["models"], &[]));
    assert!(e.contains("M gemini     gemini-nano-banana-2.1"), "{e}");
    assert!(e.contains("Maimbrain's keys: @ada has access · $1.00 of $10.00 used this month · gemini, elevenlabs"), "{e}");
    let e = stderr(&mb(&d, &["doctor"], &[]));
    assert!(e.contains("✓ Maimbrain's keys: @ada has access"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn ip_check_prints_json() {
    let mut c = Command::new(env!("CARGO_BIN_EXE_mb")).arg("ip-check").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    use std::io::Write;
    c.stdin.take().unwrap().write_all(b"a plumber like Mario, in the style of Hayao Miyazaki").unwrap();
    let out = c.wait_with_output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["refused"][0], "mario");
    assert!(v["warnings"].as_array().unwrap().iter().any(|w| w == "hayao miyazaki"));
}
