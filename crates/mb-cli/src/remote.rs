//! Talking to the Maimbrain server: `mb login`, `mb publish`, `mb submit`, `mb describe`, `mb whoami`,
//! and whether you may use Maimbrain's generation keys (generate/maimbrain.rs makes those calls).
//! `mb test` and `mb feedback` (playtest.rs) use the helpers here.
//!
//! Credentials live in `~/.config/maimbrain/credentials.json` (mode 0600), or
//! `%APPDATA%\maimbrain\credentials.json` on Windows.
//! The server is https://maimbrain.com unless `MB_SERVER` or `--server` says
//! otherwise (e.g. http://localhost:3000 for local development).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_SERVER: &str = "https://maimbrain.com";

#[derive(Serialize, Deserialize, Clone)]
pub struct Credentials {
    pub server: String,
    pub token: String,
    pub username: String,
}

fn config_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("credentials.json"))
}

/// `~/.config/maimbrain` (or `$XDG_CONFIG_HOME/maimbrain`, `%APPDATA%\maimbrain` on Windows).
/// `MB_CONFIG_DIR` points somewhere else (tests use it).
pub fn config_dir() -> Result<PathBuf, String> {
    if let Some(d) = std::env::var_os("MB_CONFIG_DIR") {
        return Ok(PathBuf::from(d));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData").join("Roaming")))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
    .ok_or("can't find a home directory for credentials")?;
    Ok(base.join("maimbrain"))
}

pub fn load() -> Option<Credentials> {
    let text = std::fs::read_to_string(config_path().ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save(c: &Credentials) -> Result<(), String> {
    let path = config_path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_string_pretty(c).unwrap()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// The server to use: --server, else MB_SERVER, else the saved login's, else maimbrain.com.
pub fn server(flag: Option<&str>) -> String {
    flag.map(str::to_string)
        .or_else(|| std::env::var("MB_SERVER").ok())
        .or_else(|| load().map(|c| c.server))
        .unwrap_or_else(|| DEFAULT_SERVER.into())
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn agent() -> ureq::Agent {
    agent_with(Duration::from_secs(120))
}

fn agent_with(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(concat!("mb/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// (status, JSON body) for a request; network errors become Err.
pub(crate) fn json_of(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, Value), String> {
    let mut resp = resp.map_err(|e| format!("can't reach the server: {e}"))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&body).unwrap_or(Value::Null)))
}

pub(crate) fn error_text(status: u16, body: &Value) -> String {
    body["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"))
}

/// Device-code sign-in: shows a code, waits for approval on the web.
pub fn login(server_flag: Option<&str>) -> Result<(), String> {
    let server = server(server_flag);
    let host = std::env::var("HOST").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this computer".into());
    let (status, start) = json_of(
        agent().post(format!("{server}/api/v1/device_codes")).send_json(serde_json::json!({ "client_name": format!("mb on {host}") })),
    )?;
    if status != 201 {
        return Err(format!("couldn't start sign-in: {}", error_text(status, &start)));
    }
    let code = start["user_code"].as_str().unwrap_or("?");
    let url = start["verification_uri_complete"].as_str().unwrap_or("");
    eprintln!("To sign in, open:\n\n    {url}\n\nand confirm the code  {code}\n");
    if std::env::var_os("MB_NO_BROWSER").is_none() {
        let _ = open_browser(url);
    }
    let interval = Duration::from_secs(start["interval"].as_u64().unwrap_or(3));
    let deadline = Instant::now() + Duration::from_secs(start["expires_in"].as_u64().unwrap_or(600));
    let device_code = start["device_code"].as_str().unwrap_or_default().to_string();
    eprint!("Waiting for approval…");
    while Instant::now() < deadline {
        std::thread::sleep(interval);
        let (status, body) = json_of(
            agent().post(format!("{server}/api/v1/device_codes/token")).send_json(serde_json::json!({ "device_code": device_code })),
        )?;
        match status {
            200 => {
                let creds = Credentials {
                    server: server.clone(),
                    token: body["token"].as_str().unwrap_or_default().into(),
                    username: body["username"].as_str().unwrap_or_default().into(),
                };
                save(&creds)?;
                eprintln!("\nSigned in as @{} on {server}", creds.username);
                return Ok(());
            }
            428 => eprint!("."),
            _ => return Err(format!("\nsign-in failed: {}", error_text(status, &body))),
        }
    }
    Err("\nthe code expired; run mb login again".into())
}

fn open_browser(url: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(windows) {
        // `start` is a cmd built-in; its first quoted argument is a window title.
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
    };
    cmd.arg(url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map(|_| ())
}

pub fn credentials() -> Result<Credentials, String> {
    load().ok_or_else(|| "not signed in; run mb login".to_string())
}

/// Whether this account may use Maimbrain's generation keys (`GET /generate/status`).
/// None when signed out: then nothing is sent.
pub fn generation_status() -> Result<Option<Value>, String> {
    let Some(c) = load() else { return Ok(None) };
    let (status, body) = json_of(
        agent_with(Duration::from_secs(20))
            .get(format!("{}/api/v1/generate/status", c.server))
            .header("Authorization", format!("Bearer {}", c.token))
            .call(),
    )?;
    match status {
        200 => Ok(Some(body)),
        401 => Err(format!("your sign-in on {} has expired or was revoked; run mb login", c.server)),
        404 => Err(format!("{} doesn't offer Maimbrain's generation keys", c.server)),
        _ => Err(error_text(status, &body)),
    }
}

pub fn whoami() -> Result<Value, String> {
    let c = credentials()?;
    let (status, body) = json_of(agent().get(format!("{}/api/v1/cli/whoami", c.server)).header("Authorization", format!("Bearer {}", c.token)).call())?;
    if status == 200 { Ok(body) } else { Err(format!("{} (try mb login)", error_text(status, &body))) }
}

/// Uploads a bundle and waits for validation; prints where it ended up. It
/// becomes a private draft to play in the app, unless `submit` sends it
/// straight to review, as `level` (fix, update or season; None: what its
/// number suggests). `model` is the AI model that built it (None: a person);
/// `notes` what's new in it for players. `source` is what to pass to `mb submit`
/// afterwards. `fixes` are the to-dos it fixes (`--fixes 7,10`, docs/UPDATES.md
/// "Closing the loop"): once it's valid, the server marks them fixed in it.
pub fn publish(bundle: &Path, submit: bool, level: Option<&str>, model: Option<&str>, notes: Option<&str>, fixes: &[u64], source: &str) -> Result<(), String> {
    let c = credentials()?;
    let bytes = std::fs::read(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    eprintln!("Uploading {} ({} KB) to {}…", bundle.display(), bytes.len() / 1024, c.server);
    let mut request = agent().post(format!("{}/api/v1/games/versions", c.server));
    if submit {
        request = request.query("submit", "true");
        if let Some(l) = level {
            request = request.query("level", l);
        }
    }
    if let Some(m) = model {
        request = request.query("model", m);
    }
    if let Some(n) = notes {
        request = request.query("notes", n);
    }
    if !fixes.is_empty() {
        request = request.query("fixes", fixes.iter().map(u64::to_string).collect::<Vec<_>>().join(","));
    }
    let (status, body) = json_of(
        request
            .header("Authorization", format!("Bearer {}", c.token))
            .header("Content-Type", "application/octet-stream")
            .header("X-Filename", bundle.file_name().and_then(|n| n.to_str()).unwrap_or("game.mbx"))
            .send(&bytes[..]),
    )?;
    if status != 202 {
        return Err(format!("upload refused: {}", error_text(status, &body)));
    }
    let status_url = body["status_url"].as_str().unwrap_or_default().to_string();
    let mut v = body;
    let refresh = || -> Result<Value, String> {
        std::thread::sleep(Duration::from_secs(2));
        let (s, b) = json_of(agent().get(&status_url).header("Authorization", format!("Bearer {}", c.token)).call())?;
        if s != 200 {
            return Err(format!("checking status: {}", error_text(s, &b)));
        }
        Ok(b)
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    while v["state"] == "validating" && Instant::now() < deadline {
        v = refresh()?;
    }
    // The score check (docs/UPDATES.md "The score check") re-runs the season's best runs on it.
    if matches!(v["state"].as_str(), Some("draft" | "pending")) && v["score_check"]["state"] == "running" {
        eprintln!("  Scores: checking how your top {} runs score on this build…", v["score_check"]["runs"]);
        let deadline = Instant::now() + Duration::from_secs(90);
        while v["score_check"]["state"] == "running" && Instant::now() < deadline {
            v = refresh()?;
        }
    }
    let name = format!("{} {}", v["game_id"].as_str().unwrap_or("?"), v["version"].as_str().unwrap_or(""));
    // With --submit, why the server kept it a draft instead (the board rule
    // needed a level it wasn't given): shown as the outcome, not a warning.
    let warnings: Vec<&str> = v["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str()).collect();
    let not_sent = warnings.iter().find_map(|w| w.strip_prefix("Not sent for review: "));
    let season_only = !season_reasons(&v).is_empty();
    if matches!(v["state"].as_str(), Some("draft" | "pending")) {
        release_summary(&v, source);
        score_summary(&v, source, v["state"] == "draft");
        for w in warnings.iter().filter(|w| !w.starts_with("Not sent for review: ")) {
            eprintln!("  Warning: {w}");
        }
    }
    let submit_command = if season_only { format!("mb submit {source} --season") } else { format!("mb submit {source}") };
    match v["state"].as_str().unwrap_or("") {
        // Sent with submit, it waits for the score check (the score rule needs its result).
        "draft" if submit && v["score_check"]["state"] == "running" && v["score_check"].get("submit").is_some() => {
            eprintln!("✓ {name} passed validation. It goes to review by itself once the check of your top runs finishes,");
            eprintln!("  unless only a new season fits: then it stays a private draft, and mb publish {source} shows why.");
            eprintln!("  Track it at {}/create", c.server);
            Ok(())
        }
        "draft" if submit => {
            eprintln!("✗ {name} passed validation but was NOT sent for review.");
            if let Some(reason) = not_sent {
                eprintln!("  {reason}");
            }
            eprintln!("  It's a private draft: only you and your testers can play it.");
            Err(format!("{name} was not submitted; to submit it: {submit_command}"))
        }
        "draft" => {
            eprintln!("✓ {name} passed validation. It's a private draft: only you and your testers can play it.");
            fixes_summary(&v, fixes, source);
            eprintln!("  Test it on your phone: open Maimbrain → Account → My Game Studio (it's also first in your feed).");
            eprintln!("  Fix something? Run mb publish again; the new upload replaces the draft.");
            eprintln!("  When it's ready, submit it for review:  {submit_command}");
            eprintln!("  (or tap Submit in My Game Studio in the app, or at {}/create)", c.server);
            Ok(())
        }
        "pending" => {
            match v["level"].as_str().and_then(level_name) {
                Some(level) => eprintln!("✓ {name} passed validation and was submitted for review as {level}."),
                None => eprintln!("✓ {name} passed validation and is waiting for review."),
            }
            fixes_summary(&v, fixes, source);
            eprintln!("  Track it at {}/create", c.server);
            Ok(())
        }
        "invalid" => {
            let errors: Vec<&str> = v["errors"].as_array().map(|a| a.iter().filter_map(|e| e.as_str()).collect()).unwrap_or_default();
            Err(format!("the server rejected the bundle:\n  - {}", errors.join("\n  - ")))
        }
        "validating" => {
            eprintln!("Uploaded; validation is still running. Check {}/create", c.server);
            Ok(())
        }
        other => {
            eprintln!("{name}: {other}");
            Ok(())
        }
    }
}

/// Which to-dos a valid upload fixes: the notes the server marked fixed in it
/// (its `fixed`), or, when it doesn't say, the ones it was sent.
fn fixes_summary(v: &Value, sent: &[u64], source: &str) {
    if sent.is_empty() {
        return;
    }
    let fixed: Vec<u64> = match v["fixed"].as_array() {
        Some(a) => a.iter().filter_map(Value::as_u64).collect(),
        None => sent.to_vec(),
    };
    let list = |ns: &[u64]| ns.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(" ");
    if !fixed.is_empty() {
        eprintln!("  Fixes {} (their testers hear when this build reaches them)", list(&fixed));
    }
    let not: Vec<u64> = sent.iter().copied().filter(|n| !fixed.contains(n)).collect();
    if !not.is_empty() {
        eprintln!("  Not marked fixed: {} (--fixes takes to-dos only; see mb feedback {source} --all)", list(&not));
    }
}

/// A release level as players' kind of release: "an update".
fn level_name(level: &str) -> Option<&'static str> {
    match level {
        "launch" => Some("the first release"),
        "fix" => Some("a fix"),
        "update" => Some("an update"),
        "season" => Some("a new season"),
        _ => None,
    }
}

/// Why only a new season fits a draft (the board rule); empty when any level does.
fn season_reasons(v: &Value) -> Vec<&str> {
    v["season_reasons"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).collect()
}

/// What kind of release a valid upload is (docs/UPDATES.md "Version levels"):
/// for a draft what its number suggests and how to choose, or that only a new
/// season fits and why; and what's new in it, or a nudge to say when players
/// will want to know. A submitted one's level is said with the outcome.
fn release_summary(v: &Value, source: &str) {
    let reasons = season_reasons(v);
    let suggested = v["suggested_level"].as_str().or(v["level"].as_str());
    if v["state"] == "draft" {
        if !reasons.is_empty() {
            eprintln!("  Release: only a new season fits, because:");
            for r in &reasons {
                let mut chars = r.chars();
                let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
                eprintln!("    {first}{}.", chars.as_str());
            }
            eprintln!("  A new season gives every player fresh leaderboards (the old ones are archived). Submit it with --season.");
        } else {
            match suggested {
                Some("launch") => eprintln!("  Release: the first release."),
                Some(l @ ("fix" | "update" | "season")) => {
                    let why = match l {
                        "fix" => "only the last number changed",
                        "update" => "the middle number changed",
                        _ => "the first number changed",
                    };
                    eprintln!("  Suggested: {} ({why}).", level_name(l).unwrap_or(l));
                    if l == "season" {
                        eprintln!("  A new season gives every player fresh leaderboards (the old ones are archived).");
                    }
                    eprintln!("  When you submit, choose: mb submit {source} --fix | --update | --season");
                }
                _ => {}
            }
        }
    }
    match v["release_notes"].as_str() {
        Some(notes) => eprintln!("  What's new: {}", notes.lines().next().unwrap_or("")),
        None if matches!(v["level"].as_str(), Some("update" | "season")) => {
            if Path::new(source).is_dir() {
                let notes = Path::new(source).join("NOTES.md");
                eprintln!("  No notes for players, and an update or a new season needs them before it's submitted:");
                eprintln!("  add a `## {}` entry to {} and publish again, or write what's new in My Game Studio.", v["version"].as_str().unwrap_or("?"), notes.display());
            } else {
                eprintln!("  No notes for players, and an update or a new season needs them before it's submitted: write what's new in My Game Studio.");
            }
        }
        None => {}
    }
}

/// What the score check found (the version's `score_check`): whether the
/// season's best runs score the same on this build. When they don't, the
/// server says only a new season fits (`season_reasons`, which
/// `release_summary` shows); with `choose`, a server from before that rule
/// gets how to act on it here.
fn score_summary(v: &Value, source: &str, choose: bool) {
    let check = &v["score_check"];
    let n = |k: &str| check[k].as_u64().unwrap_or(0);
    let (runs, changed, unchecked) = (n("runs"), n("changed"), n("unchecked"));
    match check["state"].as_str() {
        Some("running") => eprintln!("  Scores: still checking your top {runs} runs; mb submit shows what it finds."),
        Some("failed") => eprintln!("  Scores: the check of your top runs didn't finish."),
        Some("done") if changed == 0 && unchecked == runs => eprintln!("  Scores: none of your top {runs} runs could be checked."),
        Some("done") if changed == 0 => {
            let skipped = if unchecked > 0 { format!(" ({unchecked} couldn't be checked)") } else { String::new() };
            eprintln!("  Scores: your top {} scores carry over unchanged{skipped}.", runs - unchecked);
        }
        Some("done") => {
            let ms = check["format"] == "milliseconds";
            let example = check["changes"].get(0).map(|c| {
                let name = c["player"]["display_name"].as_str().or(c["player"]["username"].as_str()).unwrap_or("someone");
                let was = score_text(c["was"].as_i64().unwrap_or(0), ms);
                match c["now"].as_i64() {
                    Some(now) => format!(" ({name}'s {was} would now be {})", score_text(now, ms)),
                    None => format!(" ({name}'s {was} no longer finishes)"),
                }
            });
            eprintln!("  Scores: your changes affect {changed} of the top {runs} scores{}.", example.unwrap_or_default());
            if choose && season_reasons(v).is_empty() {
                let season = check["season"].as_u64().unwrap_or(1);
                eprintln!("  Keep Season {season}'s scores (mb submit {source} --update or --fix), or start Season {} (--season).", season + 1);
            }
        }
        _ => {}
    }
}

/// A score as players see it: 1,240, or 1:23.410 on a board in milliseconds.
fn score_text(v: i64, milliseconds: bool) -> String {
    if milliseconds {
        return format!("{}:{:02}.{:03}", v / 60_000, (v / 1000) % 60, v % 1000);
    }
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(d);
    }
    if v < 0 { format!("-{out}") } else { out }
}

/// The game `mb submit` was given, and what it expects the draft to be.
pub struct SubmitTarget {
    /// A game id, or the last part of one (`frogger`).
    pub query: String,
    /// From a game directory or bundle: the draft must be this version…
    pub version: Option<String>,
    /// …and, from a bundle, exactly these bytes.
    pub sha256: Option<String>,
    /// What the user typed, for messages.
    pub source: String,
}

/// Your uploads (`GET /me/games`, newest first) and the id of the one game
/// among them that `query` names: its whole id, or the last part of it.
pub(crate) fn my_game(c: &Credentials, query: &str) -> Result<(Vec<Value>, String), String> {
    let (status, body) = json_of(agent().get(format!("{}/api/v1/me/games", c.server)).header("Authorization", format!("Bearer {}", c.token)).call())?;
    if status != 200 {
        return Err(format!("listing your games: {} (try mb login)", error_text(status, &body)));
    }
    let mine = body.as_array().cloned().unwrap_or_default();
    let suffix = format!(".{query}");
    let mut ids: Vec<String> = mine.iter().map(game_id).filter(|id| *id == query || id.ends_with(&suffix)).collect();
    ids.sort();
    ids.dedup();
    match ids.as_slice() {
        [] => Err(format!("you haven't uploaded {query}; run mb publish first")),
        [one] => Ok((mine, one.clone())),
        many => Err(format!("{query} matches several games: {}", many.join(", "))),
    }
}

fn game_id(v: &Value) -> String {
    v["game_id"].as_str().unwrap_or("").to_string()
}

/// Sets (or with None, removes) a game's description (`PATCH /games/:game_id`).
pub fn describe(query: &str, description: Option<&str>) -> Result<(), String> {
    let c = credentials()?;
    let (_, id) = my_game(&c, query)?;
    let (status, body) = json_of(
        agent()
            .patch(format!("{}/api/v1/games/{id}", c.server))
            .header("Authorization", format!("Bearer {}", c.token))
            .send_json(serde_json::json!({ "description": description })),
    )?;
    if status != 200 {
        return Err(format!("couldn't describe {id}: {}", error_text(status, &body)));
    }
    match body["description"].as_str() {
        Some(d) => {
            let first: String = d.lines().next().unwrap_or("").chars().take(40).collect();
            let more = if d.chars().count() > first.chars().count() { "…" } else { "" };
            eprintln!("✓ Described {id}. Its card in the feed starts: {first}{more}");
            eprintln!("  All of it is under About this game, and in links people share.");
        }
        None => eprintln!("✓ Removed the description of {id}."),
    }
    Ok(())
}

/// Sends the game's draft to review (`POST /games/versions/:id/submit`) as
/// `level` (fix, update or season; None: what its number suggests).
pub fn submit(t: &SubmitTarget, level: Option<&str>) -> Result<(), String> {
    let c = credentials()?;
    let auth = format!("Bearer {}", c.token);
    let (mine, id) = my_game(&c, &t.query)?;
    // Newest first (GET /me/games); a game has at most one draft.
    let versions: Vec<&Value> = mine.iter().filter(|v| game_id(v) == id).collect();
    let Some(draft) = versions.iter().find(|v| v["state"] == "draft") else {
        let newest = versions.iter().find(|v| v["state"] != "invalid");
        return Err(match newest.map(|v| (v["state"].as_str().unwrap_or(""), v["version"].as_str().unwrap_or(""))) {
            Some(("pending", ver)) => format!("{id} {ver} is already waiting for review"),
            Some(("validating", _)) => format!("{id}'s latest upload is still being validated; try again in a moment"),
            _ => format!("{id} has no draft to submit; upload one with mb publish"),
        });
    };
    let version = draft["version"].as_str().unwrap_or("");
    if let Some(want) = &t.version
        && want != version
    {
        return Err(format!(
            "your draft of {id} is {version}, but {} is {want}; run mb publish {} to upload {want} first (or mb submit {id} to submit {version})",
            t.source, t.source
        ));
    }
    if let Some(sha) = &t.sha256
        && draft["sha256"].as_str() != Some(sha.as_str())
    {
        return Err(format!("{} isn't your draft of {id} {version}; run mb publish {} to upload it first", t.source, t.source));
    }
    score_summary(draft, &t.source, level.is_none());
    let mut request = agent().post(format!("{}/api/v1/games/versions/{}/submit", c.server, draft["id"]));
    if let Some(l) = level {
        request = request.query("level", l);
    }
    let (status, body) = json_of(request.header("Authorization", &auth).send_empty())?;
    if status != 200 {
        // A 422 says why (the board rule needs a new season) and how to choose one.
        return Err(format!("couldn't submit {id} {version}: {}", error_text(status, &body)));
    }
    let what = body["level"].as_str().and_then(level_name).map(|l| format!(" as {l}")).unwrap_or_default();
    eprintln!("✓ Submitted {id} {version} for review{what}. A moderator will play it before it goes into the feed.");
    eprintln!("  Track it at {}/create (or in the app under My games).", c.server);
    Ok(())
}
