//! `mb test`, `mb feedback` and `mb publish --fixes` against a fake Maimbrain
//! server on localhost (docs/UPDATES.md "Test builds"), and `mb serve
//! --replay --at` up to the page it serves.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const GAME: &str = "com.maimbrain.sam.summoned";

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    url: String,
    body: Value,
}

type Route = dyn Fn(&str, &str, &Value) -> Option<(u16, Vec<u8>, &'static str)> + Send + Sync;

/// Lists the game for /me/games, three friends for sam, the files notes point
/// at under /files/, and answers everything else with `route` (404 when it
/// gives nothing). Remembers every request.
struct FakeServer {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn json_reply(code: u16, v: Value) -> Option<(u16, Vec<u8>, &'static str)> {
    Some((code, v.to_string().into_bytes(), "application/json"))
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

const BUILD_3: &[u8] = b"build three's bundle";
const BUILD_2: &[u8] = b"build two's bundle";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0fake";

impl FakeServer {
    fn start(route: impl Fn(&str, &str, &Value) -> Option<(u16, Vec<u8>, &'static str)> + Send + Sync + 'static) -> FakeServer {
        let route: Arc<Route> = Arc::new(route);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = Vec::new();
                let _ = req.as_reader().read_to_end(&mut body);
                let (method, url) = (req.method().to_string(), req.url().to_string());
                let body: Value = serde_json::from_slice(&body).unwrap_or_default();
                log.lock().unwrap().push(Seen { method: method.clone(), url: url.clone(), body: body.clone() });
                let path = url.split('?').next().unwrap_or("");
                let reply = match (method.as_str(), path) {
                    ("GET", "/api/v1/me/games") => json_reply(200, json!([{ "id": 7, "game_id": GAME, "version": "0.2.0", "state": "draft" }])),
                    ("GET", "/api/v1/users/sam/friends") => {
                        json_reply(200, json!({ "users": [{ "username": "ann" }, { "username": "cleo" }, { "username": "riley" }], "next_cursor": null }))
                    }
                    ("GET", "/files/r10.json") => Some((200, br#"{"version":1,"game":"x","frames":[]}"#.to_vec(), "application/json")),
                    ("GET", "/files/r7.json" | "/files/r9.json") => Some((200, b"{}".to_vec(), "application/json")),
                    ("GET", "/files/m10.png" | "/files/m9") => Some((200, PNG.to_vec(), "image/png")),
                    ("GET", "/files/m7") => Some((200, JPEG.to_vec(), "image/jpeg")),
                    ("GET", "/files/b3.mbx") => Some((200, BUILD_3.to_vec(), "application/octet-stream")),
                    ("GET", "/files/b2.mbx") => Some((200, BUILD_2.to_vec(), "application/octet-stream")),
                    _ => route(&method, &url, &body),
                };
                let (code, bytes, ct) = reply.unwrap_or((404, br#"{"error":"not found"}"#.to_vec(), "application/json"));
                let resp = tiny_http::Response::from_data(bytes).with_status_code(code).with_header(tiny_http::Header::from_bytes("Content-Type", ct).unwrap());
                let _ = req.respond(resp);
            }
        });
        FakeServer { url: format!("http://127.0.0.1:{port}"), seen }
    }

    fn requests(&self, method: &str, prefix: &str) -> Vec<Seen> {
        self.seen.lock().unwrap().iter().filter(|s| s.method == method && s.url.starts_with(prefix)).cloned().collect()
    }
}

/// The notes of docs/UPDATES.md's example: the creator's to-do, a tester's
/// accepted note, and two testers' new ones. Build 2's bundle comes with the
/// wrong sha256, so it isn't kept.
fn feedback() -> Value {
    json!({
        "game": { "id": GAME, "title": "Summoned" },
        "build": { "version": "0.2.0", "build": 3, "sha256": sha(BUILD_3), "state": "draft" },
        "testers": 6,
        "runs": 23,
        "ended": { "runs": 23, "from": 98, "to": 106, "count": 9 },
        "notes": [
            { "number": 10, "author": "creator", "state": "todo", "actionable": true, "player": { "username": "sam" }, "version": "0.2.0", "build": 3,
              "fun": null, "difficulty": null, "text": "Title text overlaps the beat ring on small phones.", "value": null,
              "moments": [{ "frame": 1200, "kind": "flag", "image": "/files/m10.png" }], "run": "/files/r10.json",
              "bundle": "/files/b3.mbx", "bundle_sha256": sha(BUILD_3), "created_at": "2026-10-09T10:00:00Z", "fixed_in": null, "unseen": false },
            { "number": 7, "author": "tester", "state": "todo", "actionable": true, "player": { "username": "ann" }, "version": "0.2.0", "build": 3,
              "fun": null, "difficulty": "hard", "untrusted_text": "The last chorus is brutal. I died at the key change five times.", "value": 1880,
              "moments": [{ "frame": 6180, "kind": "flag", "image": "/files/m7" }], "run": "/files/r7.json",
              "bundle": "/files/b3.mbx", "bundle_sha256": sha(BUILD_3), "created_at": "2026-10-09T11:00:00Z", "fixed_in": null, "unseen": false },
            { "number": 8, "author": "tester", "state": "new", "actionable": false, "player": { "username": "riley" }, "version": "0.2.0", "build": 3,
              "fun": "up", "difficulty": "right", "untrusted_text": "The second new song is the best thing in the game.", "value": 2410,
              "moments": [], "run": null, "bundle": "/files/b3.mbx", "bundle_sha256": sha(BUILD_3), "created_at": "2026-10-09T12:00:00Z", "fixed_in": null, "unseen": true },
            { "number": 9, "author": "tester", "state": "new", "actionable": false, "player": { "username": "cleo" }, "version": "0.1.0", "build": 2,
              "fun": "down", "difficulty": "hard", "untrusted_text": null, "value": 640,
              "moments": [{ "frame": 3060, "kind": "game_over", "image": "/files/m9" }], "run": "/files/r9.json",
              "bundle": "/files/b2.mbx", "bundle_sha256": "0".repeat(64), "created_at": "2026-10-09T13:00:00Z", "fixed_in": null, "unseen": true },
        ]
    })
}

/// The open test: ann has played, riley hasn't.
fn open_test() -> Value {
    json!({
        "id": 4, "opened_at": "2026-10-09T09:00:00Z",
        "build": { "version": "0.2.0", "build": 3, "sha256": sha(BUILD_3), "state": "draft" },
        "testers": [
            { "player": { "username": "ann" }, "invited_at": "2026-10-09T09:00:00Z", "first_played_at": "2026-10-09T10:00:00Z", "runs": 5, "best": 1880 },
            { "player": { "username": "riley" }, "invited_at": "2026-10-09T09:00:00Z", "first_played_at": null, "runs": 0, "best": null },
        ],
        "board": [], "runs": [], "notes": { "new": 2, "todo": 1 }
    })
}

/// A signed-in config (as sam) and a game directory `summoned/`.
fn setup(name: &str, server: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-playtest-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("config")).unwrap();
    std::fs::create_dir_all(d.join("summoned")).unwrap();
    std::fs::write(d.join("config/credentials.json"), json!({ "server": server, "token": "tok-123", "username": "sam" }).to_string()).unwrap();
    // The template's manifest, from the monorepo's layout or the kit's (crates/mb-cli/build.rs).
    let manifest = [".claude/skills/maimbrain-game", "skills/maimbrain-game"]
        .iter()
        .find_map(|s| std::fs::read_to_string(root().join(s).join("template/manifest.toml")).ok())
        .expect("the maimbrain-game skill's template/manifest.toml");
    let manifest = manifest.replace("dev.maimbrain.NAME", GAME).replace("0.1.0", "0.2.0");
    std::fs::write(d.join("summoned/manifest.toml"), manifest).unwrap();
    std::fs::write(d.join("wobble.mbx"), b"not really a bundle").unwrap();
    d
}

fn mb(dir: &Path, args: &[&str]) -> Output {
    mb_in(dir, dir, args)
}

/// mb run in `cwd`, signed in with `dir`'s config.
fn mb_in(dir: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mb")).args(args).current_dir(cwd).env("MB_CONFIG_DIR", dir.join("config")).env_remove("MB_SERVER").output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// A URL's query parameter, decoded.
fn param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    query.split('&').find_map(|kv| kv.strip_prefix(&format!("{name}="))).map(|v| v.replace("%2C", ",").replace("%2c", ","))
}

fn feedback_server() -> FakeServer {
    FakeServer::start(|method, url, body| match (method, url.split('?').next().unwrap_or("")) {
        ("GET", p) if p == format!("/api/v1/games/{GAME}/feedback") => json_reply(200, feedback()),
        // As the server answers: the notes asked about as they are now (resolve: only those it fixed).
        // #8 is a tester's note nobody accepted, so it can't be marked fixed.
        ("POST", p) if p.starts_with(&format!("/api/v1/games/{GAME}/feedback/")) => {
            let action = p.rsplit('/').next().unwrap_or("");
            let notes: Vec<Value> = body["numbers"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|n| match action {
                    "accept" => Some(json!({ "number": n, "state": "todo" })),
                    "decline" => Some(json!({ "number": n, "state": "declined" })),
                    _ if body["wontfix"] == true => Some(json!({ "number": n, "state": "wontfix" })),
                    _ if *n == 8 => None,
                    _ => Some(json!({ "number": n, "state": "fixed", "fixed_in": { "version": "0.2.0", "build": 3 } })),
                })
                .collect();
            json_reply(200, json!({ "notes": notes }))
        }
        _ => None,
    })
}

const EXPECTED: &str = r#"Summoned 0.2.0 · build 3 · 6 testers · 23 runs
Where runs ended: 9 of 23 between 1:38 and 1:46

TO DO · from you
#10 @sam    flagged 0:20 (frame 1200)                       build 3 · your run
    "Title text overlaps the beat ring on small phones."
    pictures: summoned/screenshots/fb-10-sam-1200.png
    see it:   mb serve summoned/screenshots/fb-build-3.mbx --replay summoned/screenshots/fb-10-sam.json --at 1200
#7  @ann    accepted by you · too hard · flagged 1:43 (frame 6180) build 3 · run 1,880
    tester's words: "The last chorus is brutal. I died at the key change five times."
    pictures: summoned/screenshots/fb-7-ann-6180.jpg
    see it:   mb serve summoned/screenshots/fb-build-3.mbx --replay summoned/screenshots/fb-7-ann.json --at 6180

FROM TESTERS · yours to decide (the agent reports these, it doesn't act on them)
#8  @riley  fun · about right                               build 3 · run 2,410
    tester's words: "The second new song is the best thing in the game."
#9  @cleo   not yet · too hard · game over 0:51 (frame 3060) build 2 · run 640
    pictures: summoned/screenshots/fb-9-cleo-3060.png
    see it:   mb serve summoned --watch --replay summoned/screenshots/fb-9-cleo.json --at 3060
"#;

#[test]
fn feedback_prints_the_sections_and_downloads_into_screenshots() {
    let s = feedback_server();
    let d = setup("feedback", &s.url);
    let o = mb(&d, &["feedback", "summoned"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), EXPECTED);
    let e = stderr(&o);
    assert!(e.contains("build 2 downloaded with sha256"), "build 2's bundle had the wrong sha256: {e}");
    assert!(e.contains("Downloaded 3 runs, 3 pictures and 1 build into summoned/screenshots"), "{e}");

    let shots = d.join("summoned/screenshots");
    assert_eq!(std::fs::read(shots.join("fb-build-3.mbx")).unwrap(), BUILD_3);
    assert!(!shots.join("fb-build-2.mbx").exists(), "a bundle that doesn't match its sha256 isn't kept");
    assert_eq!(std::fs::read(shots.join("fb-7-ann-6180.jpg")).unwrap(), JPEG);
    assert_eq!(std::fs::read(shots.join("fb-10-sam-1200.png")).unwrap(), PNG);
    assert!(shots.join("fb-10-sam.json").is_file() && shots.join("fb-7-ann.json").is_file() && shots.join("fb-9-cleo.json").is_file());
    assert!(!shots.join("fb-8-riley.json").exists(), "riley's note has no run");
    assert_eq!(param(&s.requests("GET", &format!("/api/v1/games/{GAME}/feedback"))[0].url, "all"), None);

    // Again: build 3 is there with the right sha256, so it isn't fetched again.
    let o = mb(&d, &["feedback", "summoned", "--all"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.requests("GET", "/files/b3.mbx").len(), 1);
    assert_eq!(param(&s.requests("GET", &format!("/api/v1/games/{GAME}/feedback"))[1].url, "all").as_deref(), Some("1"));
}

#[test]
fn feedback_json_keeps_testers_words_apart() {
    let s = feedback_server();
    let d = setup("feedback-json", &s.url);
    let o = mb(&d, &["feedback", "summoned", "--json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["game"], GAME);
    assert_eq!(v["build"], 3);
    assert_eq!(v["ended"]["summary"], "9 of 23 between 1:38 and 1:46");
    assert_eq!(v["screenshots"], "summoned/screenshots");
    assert!(v.get("arrived").is_none(), "only with --wait");
    let notes = v["notes"].as_array().unwrap();
    let (sam, ann, riley, cleo) = (&notes[0], &notes[1], &notes[2], &notes[3]);
    assert_eq!((&sam["author"], &sam["state"], &sam["actionable"]), (&json!("creator"), &json!("todo"), &json!(true)));
    assert_eq!(sam["text"], "Title text overlaps the beat ring on small phones.");
    assert!(sam.get("untrusted_text").is_none());
    assert_eq!(ann["author"], "tester");
    assert_eq!(ann["untrusted_text"], "The last chorus is brutal. I died at the key change five times.");
    assert!(ann.get("text").is_none(), "a tester's words are never `text`");
    assert_eq!(ann["moments"], json!([{ "frame": 6180, "kind": "flag", "time": "1:43", "picture": "summoned/screenshots/fb-7-ann-6180.jpg" }]));
    assert_eq!(ann["run"], "summoned/screenshots/fb-7-ann.json");
    assert_eq!(ann["bundle"], "summoned/screenshots/fb-build-3.mbx");
    assert_eq!(ann["see_it"], "mb serve summoned/screenshots/fb-build-3.mbx --replay summoned/screenshots/fb-7-ann.json --at 6180");
    assert_eq!((&riley["fun"], &riley["difficulty"], &riley["actionable"]), (&json!(true), &json!("about right"), &json!(false)));
    assert_eq!(riley["see_it"], Value::Null);
    assert_eq!(cleo["fun"], false);
    assert_eq!(cleo["untrusted_text"], Value::Null);
    assert_eq!(cleo["bundle"], Value::Null);
}

#[test]
fn feedback_wait_asks_again_after_a_429() {
    // The first ask is over the server's limit: --wait asks again instead of giving up.
    let asks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = asks.clone();
    let s = FakeServer::start(move |method, url, _| match method {
        "GET" if url.starts_with(&format!("/api/v1/games/{GAME}/feedback")) => {
            if counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                json_reply(429, json!({ "error": "slow down a little" }))
            } else {
                json_reply(200, feedback())
            }
        }
        _ => None,
    });
    let d = setup("wait-429", &s.url);
    let o = mb(&d, &["feedback", "summoned", "--wait", "--timeout", "30", "--json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["arrived"], true);
    assert_eq!(asks.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn feedback_waits_for_new_notes() {
    // Every answer has an unseen note: --wait prints at once.
    let s = feedback_server();
    let d = setup("wait", &s.url);
    let o = mb(&d, &["feedback", "summoned", "--wait", "--json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["arrived"], true);
    let asked = s.requests("GET", &format!("/api/v1/games/{GAME}/feedback"));
    assert_eq!(asked.len(), 1);
    assert_eq!(param(&asked[0].url, "wait"), None, "polled: the server doesn't hold requests");

    // Nothing unseen: it gives up at --timeout and says so.
    let s = FakeServer::start(|method, url, _| match method {
        "GET" if url.starts_with(&format!("/api/v1/games/{GAME}/feedback")) => {
            let mut f = feedback();
            f["notes"] = json!([f["notes"][0].clone()]);
            json_reply(200, f)
        }
        _ => None,
    });
    let d = setup("wait-timeout", &s.url);
    let o = mb(&d, &["feedback", "summoned", "--wait", "--timeout", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("No new notes came within 1 s."), "{}", stderr(&o));
    assert!(stdout(&o).contains("TO DO · from you\n#10 @sam"), "{}", stdout(&o));
    assert_eq!(param(&s.requests("GET", &format!("/api/v1/games/{GAME}/feedback"))[0].url, "wait"), None);
}

#[test]
fn feedback_accept_decline_and_resolve_send_the_numbers() {
    let s = feedback_server();
    let d = setup("accept", &s.url);
    let o = mb(&d, &["feedback", "accept", "summoned", "9", "#10"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("✓ Accepted #9 #10: they're to-dos now."), "{}", stderr(&o));
    let o = mb(&d, &["feedback", "decline", "summoned", "8"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = mb(&d, &["feedback", "resolve", "summoned", "7,10"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("✓ Marked #7 #10 fixed in build 3."), "{}", stderr(&o));
    let o = mb(&d, &["feedback", "resolve", GAME, "8", "--wontfix"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("✓ Closed #8 as won't fix."), "{}", stderr(&o));
    // In a game directory the game can be left out.
    let o = mb_in(&d, &d.join("summoned"), &["feedback", "accept", "9"]);
    assert!(o.status.success(), "{}", stderr(&o));

    let sent: Vec<(String, Value)> = s.requests("POST", &format!("/api/v1/games/{GAME}/feedback/")).into_iter().map(|r| (r.url, r.body)).collect();
    let base = format!("/api/v1/games/{GAME}/feedback");
    assert_eq!(
        sent,
        [
            (format!("{base}/accept"), json!({ "numbers": [9, 10] })),
            (format!("{base}/decline"), json!({ "numbers": [8] })),
            (format!("{base}/resolve"), json!({ "numbers": [7, 10], "wontfix": false })),
            (format!("{base}/resolve"), json!({ "numbers": [8], "wontfix": true })),
            (format!("{base}/accept"), json!({ "numbers": [9] })),
        ]
    );
    let o = mb(&d, &["feedback", "accept", "summoned"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("give the notes' numbers"), "{}", stderr(&o));
    // What didn't move is said, and nothing moving is an error.
    let o = mb(&d, &["feedback", "resolve", "summoned", "7", "8"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("✓ Marked #7 fixed in build 3.") && stderr(&o).contains("  #8 wasn't changed: it isn't a to-do"), "{}", stderr(&o));
    let o = mb(&d, &["feedback", "resolve", "summoned", "8"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("error: no note was changed (mb feedback summoned --all lists them)"), "{}", stderr(&o));
}

fn test_server(current: Option<Value>, invite: (u16, Value)) -> FakeServer {
    FakeServer::start(move |method, url, body| {
        let path = url.split('?').next().unwrap_or("");
        let tests = format!("/api/v1/games/{GAME}/tests");
        match method {
            "GET" if path == format!("{tests}/current") => match &current {
                Some(t) => json_reply(200, t.clone()),
                None => json_reply(404, json!({ "error": "no open test" })),
            },
            "POST" if path == tests => {
                let mut t = invite.1.clone();
                if invite.0 == 200 {
                    t["testers"] = json!(body["testers"].as_array().unwrap().iter().map(|u| json!({ "player": { "username": u }, "runs": 0 })).collect::<Vec<_>>());
                }
                json_reply(invite.0, t)
            }
            "PATCH" if path == format!("{tests}/current") => json_reply(200, open_test()),
            "DELETE" if path == format!("{tests}/current") => Some((204, vec![], "application/json")),
            _ => None,
        }
    })
}

#[test]
fn test_shows_the_open_test_and_friends() {
    let s = test_server(Some(open_test()), (200, open_test()));
    let d = setup("test-show", &s.url);
    let o = mb(&d, &["test", "summoned"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        format!(
            "Test of {GAME}: build 3 (0.2.0, draft), opened 2026-10-09\n\
             Testers (2 of 25):\n  @ann    played · 5 runs · best 1,880\n  @riley  invited, hasn't played yet\n\
             Notes: 2 new from testers, 1 to do\n\
             \nYour friends (3): @ann (testing), @cleo, @riley (testing)\n\
             Invite more: mb test summoned --invite cleo (or --invite-all). Remove: --remove <username>. End it: --end.\n"
        )
    );

    let s = test_server(None, (200, open_test()));
    let d = setup("test-none", &s.url);
    let o = mb(&d, &["test", "summoned"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        format!("No open test of {GAME}.\n\nYour friends (3): @ann, @cleo, @riley\nOpen one: mb test summoned --invite ann,cleo,riley (or --invite-all: all your friends, up to 25)\n")
    );
}

#[test]
fn test_invites_removes_and_ends() {
    let s = test_server(None, (200, open_test()));
    let d = setup("test-invite", &s.url);
    let o = mb(&d, &["test", "summoned", "--invite", "@ann, cleo,ann"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains(&format!("✓ Invited @ann, @cleo to test {GAME} (build 3).")), "{}", stderr(&o));
    let o = mb(&d, &["test", "summoned", "--remove", "ann"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = mb(&d, &["test", "summoned", "--end"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains(&format!("✓ Ended the test of {GAME}.")), "{}", stderr(&o));
    let tests = format!("/api/v1/games/{GAME}/tests");
    assert_eq!(s.requests("POST", &tests)[0].body, json!({ "testers": ["ann", "cleo"] }));
    assert_eq!(s.requests("PATCH", &tests)[0].body, json!({ "remove": ["ann"] }));
    assert_eq!(s.requests("DELETE", &tests).len(), 1);

    // --invite-all: every friend not testing yet.
    let s = test_server(Some(open_test()), (200, open_test()));
    let d = setup("test-invite-all", &s.url);
    let o = mb(&d, &["test", "summoned", "--invite-all"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.requests("POST", &tests)[0].body, json!({ "testers": ["cleo"] }));
}

#[test]
fn test_passes_on_the_servers_refusal() {
    let why = "@zed isn't your friend: testers must be mutual friends";
    let s = test_server(None, (422, json!({ "error": why })));
    let d = setup("test-422", &s.url);
    let o = mb(&d, &["test", "summoned", "--invite", "zed"]);
    assert!(!o.status.success());
    assert_eq!(stderr(&o), format!("error: {why}\n"));
}

#[test]
fn publish_sends_the_fixes() {
    let s = FakeServer::start(|method, url, _| match method {
        "POST" if url.starts_with("/api/v1/games/versions") => json_reply(
            202,
            json!({ "id": 9, "game_id": GAME, "version": "0.2.0", "state": "draft", "status_url": "", "level": "update",
                    "suggested_level": "update", "season_reasons": [], "release_notes": "New songs.", "warnings": [], "fixed": [7] }),
        ),
        _ => None,
    });
    let d = setup("fixes", &s.url);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--fixes", "7,#10"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("  Fixes #7 (their testers hear when this build reaches them)"), "{}", stderr(&o));
    assert!(stderr(&o).contains("  Not marked fixed: #10 (--fixes takes to-dos only; see mb feedback wobble.mbx --all)"), "{}", stderr(&o));
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!stderr(&o).contains("Fixes"), "{}", stderr(&o));
    let uploads = s.requests("POST", "/api/v1/games/versions");
    assert_eq!(param(&uploads[0].url, "fixes").as_deref(), Some("7,10"));
    assert_eq!(param(&uploads[1].url, "fixes"), None);
    let o = mb(&d, &["publish", "wobble.mbx", "--model", "none", "--fixes", "seven"]);
    assert!(!o.status.success());
    assert_eq!(s.requests("POST", "/api/v1/games/versions").len(), 2);
}

// --- mb serve --replay ---------------------------------------------------------------

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The golden bundle and run, or None in the kit, which doesn't carry the
/// monorepo's fixtures/headless (tools/kit/build.sh).
fn knockdown() -> Option<(PathBuf, PathBuf)> {
    let dir = root().join("fixtures/headless");
    if !dir.is_dir() {
        eprintln!("skipped: no fixtures/headless here (the kit leaves the golden runs in the monorepo)");
        return None;
    }
    Some((dir.join("dev.maimbrain.knockdown-0.1.0.mbx"), dir.join("dev.maimbrain.knockdown.run.json")))
}

#[test]
fn serve_refuses_a_run_of_another_game_or_past_its_end() {
    let Some((bundle, run)) = knockdown() else { return };
    let other = root().join("fixtures/headless/dev.maimbrain.mistwood.run.json");
    let o = mb(&root(), &["serve", bundle.to_str().unwrap(), "--replay", other.to_str().unwrap(), "--at", "5"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("is a run of dev.maimbrain.mistwood, not of dev.maimbrain.knockdown"), "{}", stderr(&o));
    let o = mb(&root(), &["serve", bundle.to_str().unwrap(), "--replay", run.to_str().unwrap(), "--at", "99999"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("the run has 1500 frames, so --at 99999 is past its end"), "{}", stderr(&o));
}

/// `mb serve <bundle> --replay <log> --at <frame>`: the page's address asks for
/// the run, and the server hands it over (unwrapped from the fixture's `log`).
#[test]
fn serve_hands_the_page_the_run_to_hold_at_a_frame() {
    let version = Command::new(env!("CARGO_BIN_EXE_mb")).arg("--version").output().unwrap();
    if String::from_utf8_lossy(&version.stdout).contains("runtime none") {
        eprintln!("skipped: this mb was built without the web runtime (just runtime)");
        return;
    }
    let Some((bundle, run)) = knockdown() else { return };
    let mut child = Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(["serve", bundle.to_str().unwrap(), "--replay", run.to_str().unwrap(), "--at", "120", "--port", "0"])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let mut said = String::new();
    let page = loop {
        let line = lines.next().expect("mb serve said where the preview is").unwrap();
        said += &line;
        said.push('\n');
        if let Some(url) = line.strip_prefix("Preview: ") {
            break url.to_string();
        }
    };
    said += &lines.next().unwrap().unwrap();
    assert!(page.ends_with("/runtime/index.html?overlays&replay=play&at=120"), "{page}");
    assert!(said.contains("to frame 120 of 1500 (0:02 at 60 fps) and holding there: what the player saw"), "{said}");
    let origin = &page[..page.find("/runtime").unwrap()];
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).max_redirects(0).build().into();
    let served: Value = agent.get(format!("{origin}/__mb/replay")).call().unwrap().body_mut().read_json().unwrap();
    let want: Value = serde_json::from_str(&std::fs::read_to_string(&run).unwrap()).unwrap();
    assert_eq!(served, want["log"]);
    let root = agent.get(format!("{origin}/")).call().unwrap();
    let location = root.headers().get("location").map(|l| l.to_str().unwrap().to_string());
    let _ = child.kill();
    assert_eq!(location.as_deref(), Some("/runtime/index.html?overlays&replay=play&at=120"));
}
