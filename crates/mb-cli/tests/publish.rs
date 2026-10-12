//! `mb publish` and `mb submit` against a fake Maimbrain server on localhost:
//! the AI model that built the game, which it must be told and sends with the
//! upload, what kind of release the server says it is, and the kind the
//! creator chooses when they submit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// Answers every upload with `upload` (a valid draft by default; sent for
/// review, as the level asked for, when it was uploaded with `submit=true`
/// and nothing kept it a draft), lists it as the game's one draft for `mb
/// submit` and answers its submit with `submit`.
/// Remembers the URLs of the uploads and submits it was sent.
struct FakeServer {
    url: String,
    uploads: Arc<Mutex<Vec<String>>>,
    submits: Arc<Mutex<Vec<String>>>,
}

/// A valid draft of 0.2.0 that its number suggests is an update.
fn draft() -> Value {
    json!({ "id": 7, "game_id": "com.maimbrain.ada.wobble", "version": "0.2.0", "state": "draft", "status_url": "",
            "level": "update", "suggested_level": "update", "season_reasons": [], "release_notes": null, "notes_from_file": false,
            "warnings": ["0.2.0 changes how scores on board 0 (Score) rank"] })
}

impl FakeServer {
    fn start() -> FakeServer {
        FakeServer::with(draft(), (200, json!({ "id": 7, "version": "0.2.0", "state": "pending", "level": "update" })))
    }

    fn with(upload: Value, submit: (u16, Value)) -> FakeServer {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let (uploads, submits) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let (up_log, sub_log) = (uploads.clone(), submits.clone());
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let _ = std::io::copy(req.as_reader(), &mut std::io::sink());
                let (method, url) = (req.method().as_str().to_string(), req.url().to_string());
                let (code, json) = if method == "POST" && url.starts_with("/api/v1/games/versions/7/submit") {
                    sub_log.lock().unwrap().push(url);
                    submit.clone()
                } else if method == "POST" && url.starts_with("/api/v1/games/versions") {
                    let mut v = upload.clone();
                    let kept = v["warnings"].as_array().is_some_and(|w| w.iter().any(|w| w.as_str().is_some_and(|w| w.starts_with("Not sent for review"))));
                    if param(&url, "submit").as_deref() == Some("true") && v["state"] == "draft" && !kept {
                        v["state"] = json!("pending");
                        if let Some(l) = param(&url, "level") {
                            v["level"] = json!(l);
                        }
                    }
                    up_log.lock().unwrap().push(url);
                    (202, v)
                } else if method == "GET" && url == "/api/v1/me/games" {
                    (200, json!([upload.clone()]))
                } else {
                    (404, json!({ "error": "not found" }))
                };
                let resp = tiny_http::Response::from_string(json.to_string())
                    .with_status_code(code)
                    .with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap());
                let _ = req.respond(resp);
            }
        });
        FakeServer { url: format!("http://127.0.0.1:{port}"), uploads, submits }
    }

    /// Each upload's `model` parameter (None without one).
    fn models(&self) -> Vec<Option<String>> {
        self.uploads.lock().unwrap().iter().map(|u| param(u, "model")).collect()
    }
}

/// A URL's query parameter.
fn param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    query.split('&').find_map(|kv| kv.strip_prefix(&format!("{name}="))).map(decode)
}

fn decode(v: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = v.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => out.push(b' '),
            b'%' => {
                let hex: String = [bytes.next().unwrap(), bytes.next().unwrap()].iter().map(|&c| c as char).collect();
                out.push(u8::from_str_radix(&hex, 16).unwrap());
            }
            b => out.push(b),
        }
    }
    String::from_utf8(out).unwrap()
}

/// A signed-in config and a bundle to upload (the fake server doesn't look inside).
fn setup(name: &str, server: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-publish-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("config")).unwrap();
    std::fs::write(d.join("config/credentials.json"), serde_json::json!({ "server": server, "token": "tok-123", "username": "ada" }).to_string()).unwrap();
    std::fs::write(d.join("wobble.mbx"), b"not really a bundle").unwrap();
    d
}

/// Runs mb with no terminal on stdin, the way an agent does.
fn mb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mb")).args(args).current_dir(dir).env("MB_CONFIG_DIR", dir.join("config")).env_remove("MB_SERVER").output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn an_agent_must_say_which_model_it_is() {
    let s = FakeServer::start();
    let d = setup("ask", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx"]);
    assert!(!o.status.success());
    let e = stderr(&o);
    assert!(e.contains("--model \"<name and version>\""), "{e}");
    assert!(e.contains("If you're an AI agent, that's you"), "{e}");
    assert!(e.contains("--model none"), "{e}");
    assert!(s.models().is_empty(), "nothing is uploaded without an answer");
}

#[test]
fn sends_the_model_or_none() {
    let s = FakeServer::start();
    let d = setup("send", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "  Claude Opus 5.5 "]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--submit"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.models(), [Some("Claude Opus 5.5".to_string()), None]);
    assert!(s.uploads.lock().unwrap()[1].contains("submit=true"));
}

#[test]
fn refuses_a_model_name_that_is_not_one() {
    let s = FakeServer::start();
    let d = setup("refuse", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", " "]);
    assert!(stderr(&o).contains("give the AI model's name and version, or none"), "{}", stderr(&o));
    let o = mb(&d, &["publish", "wobble.mbx", "--model", &"x".repeat(61)]);
    assert!(stderr(&o).contains("over 60 characters"), "{}", stderr(&o));
    assert!(s.models().is_empty());
}

#[test]
fn says_what_kind_of_release_it_is_asks_for_notes_and_passes_on_warnings() {
    let s = FakeServer::start();
    let d = setup("level", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("Suggested: an update (the middle number changed)."), "{e}");
    assert!(e.contains("When you submit, choose: mb submit wobble.mbx --fix | --update | --season"), "{e}");
    assert!(e.contains("submit it for review:  mb submit wobble.mbx\n"), "{e}");
    assert!(e.contains("No notes for players, and an update or a new season needs them before it's submitted: write what's new in My Game Studio."), "{e}");
    assert!(e.contains("Warning: 0.2.0 changes how scores on board 0 (Score) rank"), "{e}");
}

#[test]
fn a_draft_that_changes_how_a_board_ranks_fits_only_a_new_season() {
    let mut v = draft();
    v["level"] = json!("season");
    v["suggested_level"] = json!("season");
    v["season_reasons"] = json!(["scores on board 0 (Score) now rank lowest first"]);
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("season-only", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("Release: only a new season fits, because:\n    Scores on board 0 (Score) now rank lowest first.\n"), "{e}");
    assert!(e.contains("Submit it with --season."), "{e}");
    assert!(!e.contains("Suggested:"), "{e}");
    assert!(e.contains("submit it for review:  mb submit wobble.mbx --season"), "{e}");
}

/// A draft whose score check found 7 of the season's top 20 scores change:
/// only a new season fits it.
fn scores_changed() -> Value {
    let mut v = draft();
    v["score_check"] = json!({ "state": "done", "season": 1, "runs": 20, "changed": 7, "unchecked": 0, "format": "integer",
                               "changes": [{ "player": { "username": "riley", "display_name": "Riley" }, "was": 1240, "now": 980 }] });
    v["suggested_level"] = json!("season");
    v["season_reasons"] = json!(["7 of Season 1's top 20 runs score differently on it (Riley's 1,240 would now be 980)"]);
    v
}

#[test]
fn says_whether_the_seasons_best_scores_carry_over() {
    let s = FakeServer::with(scores_changed(), (200, json!({ "id": 7, "version": "0.2.0", "state": "pending", "level": "update" })));
    let d = setup("scores", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("Scores: your changes affect 7 of the top 20 scores (Riley's 1,240 would now be 980).\n"), "{e}");
    assert!(e.contains("Release: only a new season fits, because:\n    7 of Season 1's top 20 runs score differently on it (Riley's 1,240 would now be 980).\n"), "{e}");
    assert!(e.contains("submit it for review:  mb submit wobble.mbx --season"), "{e}");
    assert!(!e.contains("Keep Season 1's scores"), "{e}");

    // Submitting says it again; having chosen, there's nothing to choose.
    let o = mb(&d, &["submit", "wobble", "--update"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("Scores: your changes affect 7 of the top 20 scores"), "{e}");
    assert!(!e.contains("Keep Season 1's scores"), "{e}");

    // A server from before the rule that changed scores need a new season: the choice is said here.
    let mut v = scores_changed();
    v["suggested_level"] = json!("update");
    v["season_reasons"] = json!([]);
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("scores-older-server", &s.url);
    let e = stderr(&mb(&d, &["publish", "wobble.mbx", "--model", "none"]));
    assert!(e.contains("Keep Season 1's scores (mb submit wobble.mbx --update or --fix), or start Season 2 (--season)."), "{e}");

    let mut v = scores_changed();
    v["score_check"]["changed"] = json!(0);
    v["score_check"]["unchecked"] = json!(2);
    v["suggested_level"] = json!("update");
    v["season_reasons"] = json!([]);
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("scores-same", &s.url);
    let e = stderr(&mb(&d, &["publish", "wobble.mbx", "--model", "none"]));
    assert!(e.contains("Scores: your top 18 scores carry over unchanged (2 couldn't be checked).\n"), "{e}");

    let mut v = scores_changed();
    v["score_check"]["format"] = json!("milliseconds");
    v["score_check"]["changes"][0]["was"] = json!(83_410);
    v["score_check"]["changes"][0]["now"] = json!(null);
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("scores-time", &s.url);
    let e = stderr(&mb(&d, &["publish", "wobble.mbx", "--model", "none"]));
    assert!(e.contains("(Riley's 1:23.410 no longer finishes)."), "{e}");
}

#[test]
fn publish_submits_as_the_level_it_is_given() {
    let mut v = draft();
    v["release_notes"] = json!("Fresh boards.");
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("submit-season", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--submit", "--season"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let e = stderr(&o);
    assert!(e.contains("com.maimbrain.ada.wobble 0.2.0 passed validation and was submitted for review as a new season."), "{e}");
    assert!(!e.contains("Suggested:"), "{e}");
    let uploads = s.uploads.lock().unwrap();
    assert_eq!(param(&uploads[0], "level").as_deref(), Some("season"));
    assert_eq!(param(&uploads[0], "submit").as_deref(), Some("true"));
}

#[test]
fn publish_without_a_level_sends_none() {
    let s = FakeServer::start();
    let d = setup("no-level", &s.url);
    assert!(mb(&d, &["publish", "wobble.mbx", "--model", "none", "--submit"]).status.success());
    assert_eq!(param(&s.uploads.lock().unwrap()[0], "level"), None);
}

#[test]
fn a_level_needs_submit_and_only_one() {
    let s = FakeServer::start();
    let d = setup("level-needs-submit", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--season"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("--season needs --submit: it says what kind of release you're submitting"), "{}", stderr(&o));
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--submit", "--fix", "--update"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("cannot be used with"), "{}", stderr(&o));
    let o = mb(&d, &["submit", "wobble", "--fix", "--season"]);
    assert!(!o.status.success());
    assert!(s.uploads.lock().unwrap().is_empty() && s.submits.lock().unwrap().is_empty());
}

#[test]
fn an_upload_the_server_kept_a_draft_says_it_was_not_sent() {
    let mut v = draft();
    v["level"] = json!("season");
    v["suggested_level"] = json!("season");
    v["season_reasons"] = json!(["scores on board 0 (Score) now rank lowest first"]);
    v["warnings"] = json!(["Not sent for review: 0.2.0 needs a new season: scores on board 0 (Score) now rank lowest first. Submit it as a new season (in My Game Studio, or mb submit --season); the scores so far stay on this season's boards"]);
    let s = FakeServer::with(v, (200, json!({})));
    let d = setup("not-sent", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--submit", "--update"]);
    assert!(!o.status.success(), "it wasn't submitted, so it fails");
    let e = stderr(&o);
    assert!(e.contains("✗ com.maimbrain.ada.wobble 0.2.0 passed validation but was NOT sent for review.\n  0.2.0 needs a new season:"), "{e}");
    assert!(!e.contains("Warning: Not sent"), "{e}");
    assert!(e.contains("error: com.maimbrain.ada.wobble 0.2.0 was not submitted; to submit it: mb submit wobble.mbx --season"), "{e}");
    assert_eq!(param(&s.uploads.lock().unwrap()[0], "level").as_deref(), Some("update"));
}

#[test]
fn submit_sends_the_level_and_says_what_it_was_submitted_as() {
    let s = FakeServer::with(draft(), (200, json!({ "id": 7, "version": "0.2.0", "state": "pending", "level": "season" })));
    let d = setup("submit", &s.url);
    let o = mb(&d, &["submit", "wobble", "--season"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("✓ Submitted com.maimbrain.ada.wobble 0.2.0 for review as a new season."), "{}", stderr(&o));
    let o = mb(&d, &["submit", "wobble"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let submits = s.submits.lock().unwrap();
    assert_eq!(submits.iter().map(|u| param(u, "level")).collect::<Vec<_>>(), [Some("season".to_string()), None]);
}

#[test]
fn submit_shows_why_the_server_refused_the_level() {
    let why = "0.2.0 needs a new season: scores on board 0 (Score) now rank lowest first. Submit it as a new season (in My Game Studio, or mb submit --season); the scores so far stay on this season's boards";
    let s = FakeServer::with(draft(), (422, json!({ "error": why })));
    let d = setup("submit-refused", &s.url);
    let o = mb(&d, &["submit", "wobble", "--update"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains(&format!("couldn't submit com.maimbrain.ada.wobble 0.2.0: {why}")), "{}", stderr(&o));
    assert_eq!(param(&s.submits.lock().unwrap()[0], "level").as_deref(), Some("update"));
}

/// The SDK stamps its version into every game (shown under About this game),
/// and that version means "the kit with this mb": bump them together.
#[test]
fn the_sdk_has_the_kits_version() {
    let sdk = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdk/maimbrain/Cargo.toml")).unwrap();
    let doc: toml::Table = sdk.parse().unwrap();
    assert_eq!(doc["package"]["version"].as_str(), Some(env!("CARGO_PKG_VERSION")), "set sdk/maimbrain's version to mb-cli's");
}
