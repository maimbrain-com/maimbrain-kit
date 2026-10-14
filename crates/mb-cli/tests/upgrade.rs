//! `mb upgrade` on a scratch copy of the runner kit made to look like it was
//! built with an older SDK, the line `mb build`/`serve`/`publish` print when a
//! game is behind, and `mb doctor`'s check for a newer mb against a stand-in
//! releases server.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The runner kit (under .claude/skills in the monorepo, skills/ in the kit).
fn runner_kit() -> PathBuf {
    [".claude/skills/maimbrain-game/kits/runner", "skills/maimbrain-game/kits/runner"].iter().map(|p| root().join(p)).find(|p| p.is_dir()).expect("the runner kit")
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-upgrade-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.path().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

/// A copy of the runner kit's code and manifest whose manifest predates daily
/// boards (no `daily = true`), depending on the SDK as `dep` says.
fn old_runner(name: &str, dep: &str) -> PathBuf {
    let d = scratch(name).join("runner");
    copy_dir(&runner_kit().join("src"), &d.join("src"));
    let manifest = std::fs::read_to_string(runner_kit().join("manifest.toml")).unwrap();
    assert!(manifest.contains("daily = true\n"), "the runner kit's daily board");
    std::fs::write(d.join("manifest.toml"), manifest.replace("daily = true\n", "")).unwrap();
    std::fs::write(d.join("Cargo.toml"), format!("[package]\nname = \"runner\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nmaimbrain = {dep}\n\n[workspace]\n")).unwrap();
    d
}

const GIT: &str = "{ git = \"https://github.com/terracatta/maimbrain-kit\" }";
const OLD_LOCK: &str = "version = 4\n\n[[package]]\nname = \"maimbrain\"\nversion = \"0.8.0\"\nsource = \"git+https://github.com/terracatta/maimbrain-kit#0123456789abcdef0123456789abcdef01234567\"\n\n[[package]]\nname = \"runner\"\nversion = \"0.1.0\"\ndependencies = [\n \"maimbrain\",\n]\n";

fn mb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(args)
        .current_dir(dir)
        .env("MB_CONFIG_DIR", dir.join("config"))
        .env("MB_NO_UPDATE_CHECK", "1")
        .env_remove("MB_SERVER")
        .output()
        .unwrap()
}

fn json(o: &Output) -> serde_json::Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&o.stdout)))
}

fn check<'a>(doc: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    doc["checks"].as_array().unwrap().iter().find(|c| c["id"] == id)
}

#[test]
fn json_says_what_changed_since_an_older_sdk_and_what_applies() {
    let game = old_runner("json", GIT);
    std::fs::write(game.join("Cargo.lock"), OLD_LOCK).unwrap();
    let before = std::fs::read_to_string(game.join("manifest.toml")).unwrap();
    let doc = json(&mb(game.parent().unwrap(), &["upgrade", "runner", "--json"]));
    assert_eq!(doc["kit"], CURRENT);
    assert_eq!((&doc["sdk"]["dependency"], &doc["sdk"]["version"], &doc["from"]), (&"git".into(), &"0.8.0".into(), &"0.8.0".into()));
    assert_eq!(doc["sdk"]["rev"], "0123456789abcdef0123456789abcdef01234567");
    assert_eq!((&doc["lock"]["action"], &doc["lock"]["applied"]), (&"update".into(), &false.into()), "{}", doc["lock"]);

    // Every release after 0.8.0, oldest first, up to this kit's.
    let releases = doc["releases"].as_array().unwrap();
    let versions: Vec<&str> = releases.iter().map(|r| r["version"].as_str().unwrap()).collect();
    assert_eq!(versions.first(), Some(&"0.8.1"));
    assert_eq!(versions.last(), Some(&CURRENT));
    let this = releases.iter().find(|r| r["version"] == "0.8.2").unwrap();
    for g in ["required", "recommended", "available"] {
        assert!(this["notes"].as_array().unwrap().iter().any(|n| n["group"] == g && n["title"].is_string() && n["text"].is_string()), "0.8.2 {g}");
    }
    assert!(this["notes"].as_array().unwrap().iter().filter(|n| n["group"] == "available").all(|n| n["fits"].is_string()));

    // The game's own code: Rounds::daily(BOARD_DAILY) on a board that isn't daily.
    let daily = check(&doc, "daily-board").expect("daily-board");
    assert_eq!((&daily["group"], &daily["fix"], &daily["applied"]), (&"required".into(), &serde_json::json!({ "daily_board": 1 }), &false.into()));
    assert!(daily["at"][0].as_str().unwrap().starts_with("src/lib.rs:"), "{daily}");
    assert_eq!(std::fs::read_to_string(game.join("manifest.toml")).unwrap(), before, "nothing changes without --apply");

    // The same, for a person.
    let o = mb(game.parent().unwrap(), &["upgrade", "runner"]);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success());
    for want in ["SDK 0.8.0 (git https://github.com/terracatta/maimbrain-kit at 0123456)", &format!("This kit: {CURRENT}"), "Upgrade notes since 0.8.0", "Required (", "Available (", "Rounds::daily(1)", "[--apply does it]"] {
        assert!(text.contains(want), "{want}:\n{text}");
    }
}

#[test]
fn apply_marks_the_daily_board_and_a_path_sdk_has_nothing_to_bump() {
    let sdk = root().join("sdk/maimbrain").canonicalize().unwrap();
    let game = old_runner("apply", &format!("{{ path = {:?} }}", sdk.to_string_lossy()));
    let dir = game.parent().unwrap();
    let doc = json(&mb(dir, &["upgrade", "runner", "--apply", "--json"]));
    assert_eq!(doc["lock"]["action"], "none");
    assert!(doc["lock"]["message"].as_str().unwrap().contains("nothing to bump"), "{}", doc["lock"]);
    assert_eq!(doc["sdk"]["version"], CURRENT, "the path's version");
    assert_eq!(doc["releases"], serde_json::json!([]), "already on this kit's SDK");
    assert_eq!(check(&doc, "daily-board").unwrap()["applied"], true);
    let manifest = std::fs::read_to_string(game.join("manifest.toml")).unwrap();
    assert_eq!(manifest.matches("daily = true").count(), 1, "{manifest}");
    let again = json(&mb(dir, &["upgrade", "runner", "--json"]));
    assert!(check(&again, "daily-board").is_none(), "fixed");
    // --since reads the notes from a version of your choice.
    let since = json(&mb(dir, &["upgrade", "runner", "--json", "--since", "0.8.0"]));
    assert_eq!(since["releases"][0]["version"], "0.8.1");
}

#[test]
fn build_commands_say_once_when_the_sdk_is_behind() {
    let game = old_runner("nudge", GIT);
    std::fs::write(game.join("Cargo.lock"), OLD_LOCK).unwrap();
    // publish without --model and no terminal fails right after the nudge, offline.
    let o = mb(game.parent().unwrap(), &["publish", "runner"]);
    let err = String::from_utf8_lossy(&o.stderr);
    let line = format!("built with SDK 0.8.0; this kit is {CURRENT}: run mb upgrade runner");
    assert_eq!(err.matches(&line).count(), 1, "{err}");
    // On this kit's SDK, or a path dependency: nothing.
    std::fs::write(game.join("Cargo.lock"), OLD_LOCK.replace("0.8.0", CURRENT)).unwrap();
    let o = mb(game.parent().unwrap(), &["publish", "runner"]);
    assert!(!String::from_utf8_lossy(&o.stderr).contains("built with SDK"));
    let path = old_runner("nudge-path", "{ path = \"../../sdk/maimbrain\" }");
    std::fs::write(path.join("Cargo.lock"), OLD_LOCK.replace("source = \"git+https://github.com/terracatta/maimbrain-kit#0123456789abcdef0123456789abcdef01234567\"\n", "")).unwrap();
    let o = mb(path.parent().unwrap(), &["publish", "runner"]);
    assert!(!String::from_utf8_lossy(&o.stderr).contains("built with SDK"));
}

/// Answers GET /releases/latest with `tag`, counting the requests.
struct Releases {
    url: String,
    hits: Arc<Mutex<usize>>,
    tag: Arc<Mutex<String>>,
}

impl Releases {
    fn start(tag: &str) -> Releases {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let (hits, tag) = (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(tag.to_string())));
        let (h, t) = (hits.clone(), tag.clone());
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let resp = if req.url() == "/releases/latest" {
                    *h.lock().unwrap() += 1;
                    tiny_http::Response::from_string(serde_json::json!({ "tag_name": *t.lock().unwrap() }).to_string())
                } else {
                    tiny_http::Response::from_string("{}").with_status_code(404)
                };
                let _ = req.respond(resp);
            }
        });
        Releases { url: format!("http://127.0.0.1:{port}/releases/latest"), hits, tag }
    }

    fn hits(&self) -> usize {
        *self.hits.lock().unwrap()
    }
}

fn doctor(config: &Path, url: &str, off: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_mb"));
    c.arg("doctor").env("MB_CONFIG_DIR", config).env("MB_RELEASES_URL", url).env("MB_SERVER", "http://127.0.0.1:9").env_remove("MB_NO_UPDATE_CHECK");
    if off {
        c.env("MB_NO_UPDATE_CHECK", "1");
    }
    String::from_utf8_lossy(&c.output().unwrap().stderr).to_string()
}

#[test]
fn doctor_says_when_a_newer_mb_is_out_at_most_once_a_day() {
    let s = Releases::start("v99.0.0");
    let d = scratch("doctor");
    let config = d.join("config");
    let err = doctor(&config, &s.url, false);
    assert!(err.contains(&format!("! mb {CURRENT}: 99.0.0 is out")), "{err}");
    assert!(err.contains("/plugin update maimbrain@maimbrain-kit"), "{err}");
    assert_eq!(s.hits(), 1);
    // Within the day: from the cache.
    assert!(doctor(&config, &s.url, false).contains("99.0.0 is out"));
    assert_eq!(s.hits(), 1);
    // A day later it asks again.
    let cache = config.join("release-check.json");
    let mut c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cache).unwrap()).unwrap();
    assert_eq!(c["latest"], "99.0.0");
    c["checked"] = (c["checked"].as_u64().unwrap() - 2 * 24 * 3600).into();
    std::fs::write(&cache, c.to_string()).unwrap();
    *s.tag.lock().unwrap() = format!("v{CURRENT}");
    assert!(doctor(&config, &s.url, false).contains(&format!("✓ mb {CURRENT} is the latest release")));
    assert_eq!(s.hits(), 2);
    // Turned off: no request, no line.
    std::fs::remove_file(&cache).unwrap();
    let err = doctor(&config, &s.url, true);
    assert!(!err.contains("latest release") && !err.contains("is out"), "{err}");
    assert_eq!(s.hits(), 2);
    // Offline: silent.
    let err = doctor(&d.join("offline"), "http://127.0.0.1:9/releases/latest", false);
    assert!(!err.contains("latest release") && !err.contains("is out"), "{err}");
}
