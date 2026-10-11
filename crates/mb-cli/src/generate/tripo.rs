//! Tripo (3D models, the default for `mb model`): the V3 API. A key works
//! only in its own region: international keys (developers.tripo3d.ai) at
//! https://openapi.tripo3d.ai/v3, China-mainland keys (developers.tripo3d.com)
//! at https://openapi.tripo3d.com/v3. The first request tries the
//! international one and, on a 401, the other; `TRIPO_BASE_URL` pins one.
//! Every step is an async task: create it,
//! poll `GET /tasks/{id}` until `success`, download `output.model_url` (a
//! signed, short-lived URL) right away.
//!
//!   text   POST /generation/text-to-model  {prompt, model, face_limit, texture, pbr, seeds}
//!   image  POST /files (multipart) → file_token, then /generation/image-to-model {input}
//!   rig    POST /animations/rig-check {input} → {riggable, rig_type}
//!          POST /animations/rig {input, model, rig_type, spec, out_format}
//!   clips  POST /animations/retarget {input: rig task, animation: "preset:…", animate_in_place}
//!
//! Rig types: biped (rig model v1.0, 90+ `preset:biped:*` clips), and with
//! rig model v2.5 quadruped, hexapod, octopod, avian, serpentine and aquatic
//! (fish: `preset:aquatic:march`, a swim loop). 1 credit = $0.01; failed
//! tasks aren't charged. Docs: https://developers.tripo3d.com
//!
//! Concurrency (docs "Rate Limits & Concurrency"): an account runs at most 5
//! P-series and 10 H-series generation tasks at once (each category has its
//! own pool; animation 10); one more is refused with HTTP 429 code 2000, and
//! too many requests in a short time with code 1007. `mb` takes a slot of
//! the pool (a locked file in its config dir, released by the OS if the
//! process dies) before creating a generation task, so parallel `mb model`
//! runs on one machine queue instead of failing, and retries a refused
//! creation with backoff, for tasks started elsewhere on the account.
//!
//! On Maimbrain's keys (generate/maimbrain.rs) the same client talks to the
//! Maimbrain server, which passes these paths through to Tripo with its key
//! (`{server}/api/v1/generate/tripo`, the `mb login` token as the key): no
//! region to find, no slots or balance (the server has its own caps and the
//! budget), and only its "busy" answers (code 2000 or 1007) are retried.

use serde_json::{Value, json};

use super::provider::{AudioRequest, ImageRequest, ModelOutput, ModelRequest, Output, Poll, Provider, Transport, download};

const BASES: [&str; 2] = ["https://openapi.tripo3d.ai/v3", "https://openapi.tripo3d.com/v3"];

pub struct Tripo {
    key: String,
    http: Box<dyn Transport>,
    poll: Poll,
    /// The base URL in use: `TRIPO_BASE_URL`, else BASES[0] until a 401 moves it to BASES[1].
    base: std::sync::Mutex<String>,
    /// Whether the region is settled (a request got past authentication, or TRIPO_BASE_URL is set).
    settled: std::sync::atomic::AtomicBool,
    /// Where the concurrency slots' lock files live (None: no queueing across processes).
    slots: Option<std::path::PathBuf>,
    /// Through the Maimbrain server, on its key.
    maimbrain: bool,
}

/// Tasks an account runs at once in a generation pool: P-series 5, H-series 10.
pub fn pool_of(model: &str) -> (&'static str, usize) {
    if model.starts_with('P') { ("p-series", 5) } else { ("h-series", 10) }
}

/// Waits before retrying a creation refused for concurrency or rate: 5, 10,
/// 20, 40, then 60 s (scaled to the poll interval, so tests don't sleep).
fn backoff(poll: &Poll, attempt: u32) -> std::time::Duration {
    let s = (5u64 << attempt.min(4)).min(60);
    poll.every.mul_f64(s as f64 / 3.0)
}

/// A held slot of a concurrency pool (dropping it frees the slot).
pub struct Slot(#[allow(dead_code)] Option<std::fs::File>);

/// Tripo's biped clips (rig v1.0-20240301), `preset:biped:<name>`: the
/// retarget docs' list, October 2026.
pub const BIPED_PRESETS: &[&str] = &[
    "afraid", "agree", "angry_01", "angry_02", "angry_03", "basketball_shot", "bow", "box_01", "box_02", "box_03", "cast_a_spell", "cheer", "chop", "clap",
    "climb", "complain_01", "complain_02", "cross_body_crunch", "crossover_dribble", "cry", "dance_01", "dance_02", "dance_03", "dance_04", "dance_05",
    "dance_06", "defeat_02", "defeat_03", "depressed", "dig", "dive", "dribble", "fall", "fire", "flee_01", "flee_02", "flip", "fold_arms", "football_catch",
    "football_pass", "football_save", "freaky", "frightened", "front_kick_01", "front_kick_02", "frustrated_01", "frustrated_02", "golf", "greet_01",
    "greet_02", "greet_03", "greet_04", "heart_pose", "hit_to_body_01", "hit_to_body_02", "hit_to_head", "hit_to_side", "hit_to_stomach", "hug", "hurt",
    "idle", "jump", "jump_down", "jump_rope_01", "jump_rope_02", "laugh_01", "laugh_02", "lift_heavy", "look_around", "make_a_call_01", "make_a_call_02",
    "pitch_baseball", "play_mobile_game", "play_video_game", "press", "run", "run_upstairs", "scared_01", "scared_02", "scratch", "shoot", "shovel",
    "sing_01", "sing_02", "sing_03", "sing_04", "sit", "slash", "sob", "standing_relax", "surf", "swagger", "swim", "turn", "victory_celebration",
    "volleyball", "wait", "walk", "warm_up", "wave_goodbye_01", "wave_goodbye_02",
];

/// The other rig types' clips (rig v2.5-20260210).
pub const OTHER_PRESETS: &[&str] = &["quadruped:walk", "hexapod:walk", "octopod:walk", "serpentine:march", "aquatic:march"];

/// What came back from real runs (Bolt Brawl's mechs, October 2026): lengths,
/// duplicates and where the motion is.
pub const PRESET_NOTES: &[(&str, &str)] = &[
    ("idle", "15.4 s, mostly filler: trim it (--clip Idle=0..4)"),
    ("walk", "2.4 s forward walk in place; no strafe or back-step preset"),
    ("slash", "6.6 s: wind-up 0.6–1.8 s, strike ≈ 2.0 s, settled by 3.4 s (--clip Slash=0.6..3.4)"),
    ("chop", "came back the same motion as slash (6.6 s): ask for one of them"),
    ("hurt", "13.9 s, mostly filler; hit_to_body_01 is a 1.3 s recoil"),
    ("hit_to_body_01", "1.3 s recoil"),
    ("shoot", "9.1 s, mostly filler"),
    ("fall", "3.0 s"),
    ("defeat_02", "8.5 s"),
    ("jump_down", "3.7 s: the nearest to a heavy landing (there is no landing preset)"),
    ("box_01", "2.2 s: a punch (box_01–03 are the punches)"),
    ("pitch_baseball", "3.8 s: a big overarm throw"),
    ("lift_heavy", "19.5 s, mostly filler"),
];

/// Names people try that aren't presets, and what to ask for instead.
const NOT_PRESETS: &[(&str, &str)] = &[
    ("punch", "box_01, box_02 or box_03"),
    ("kick", "front_kick_01 or front_kick_02"),
    ("attack", "slash, chop, box_01 or front_kick_01"),
    ("die", "defeat_02 or defeat_03"),
    ("death", "defeat_02 or defeat_03"),
    ("defeat", "defeat_02 or defeat_03"),
    ("defeat_01", "defeat_02 or defeat_03"),
    ("hit", "hit_to_body_01, hit_to_head, hit_to_side or hit_to_stomach"),
    ("land", "jump_down"),
    ("landing", "jump_down"),
    ("strafe", "nothing (no strafe preset: walk is forward only; move the legs in code)"),
    ("wave", "wave_goodbye_01 or greet_01"),
    ("dance", "dance_01 … dance_06"),
    ("throw", "pitch_baseball or football_pass"),
    ("celebrate", "victory_celebration or cheer"),
    ("taunt", "swagger"),
];

/// The note for a preset from real runs, if any.
pub fn preset_note(name: &str) -> Option<&'static str> {
    let n = name.rsplit(':').next().unwrap_or(name).to_lowercase();
    PRESET_NOTES.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
}

/// `mb model presets`: Tripo's clips by rig type, with what real runs showed.
pub fn presets_text() -> String {
    let mut s = String::from("Tripo animation presets (--animate NAME, or the full preset:… id); $0.10 a clip, a bad name costs nothing.\n\n");
    s += &format!("biped (rig v1.0, {} clips: --animate idle,slash → preset:biped:idle, preset:biped:slash):\n", BIPED_PRESETS.len());
    let mut line = String::from(" ");
    for p in BIPED_PRESETS {
        let mark = if p == &"chop" { "*" } else { "" };
        if line.len() + p.len() + 2 > 96 {
            s += &line;
            s += "\n";
            line = String::from(" ");
        }
        line += &format!(" {p}{mark}");
    }
    s += &line;
    s += "\n\nother rigs (rig v2.5): ";
    s += &OTHER_PRESETS.join(", ");
    s += " (one clip each; mb maps walk/run/idle/swim to it)\n\nfrom real runs (* a duplicate):\n";
    for (k, v) in PRESET_NOTES {
        s += &format!("  {k:<16} {v}\n");
    }
    s += "\nnot presets:\n";
    for (k, v) in NOT_PRESETS {
        s += &format!("  {k:<16} → {v}\n");
    }
    s += "\nmissing (do them in code, on the rig): strafe, back-step, heavy landing, thruster hop, lunge.\n";
    s += "Trim the long ones to the part you use: --clip Idle=0..4, --clip Strike=Slash@1.8..3.4 (recorded in the recipe; --reprocess redoes it).\n";
    s
}

/// The clip preset for an action on a rig type, or why there isn't one.
pub fn preset(rig_type: &str, action: &str) -> Result<(String, Option<String>), String> {
    if action.starts_with("preset:") {
        return Ok((action.to_string(), None));
    }
    let a = action.to_lowercase();
    let moving = ["walk", "run", "move", "swim", "march", "slither", "fly", "crawl"].contains(&a.as_str());
    let sub = |p: &str| if a == p.rsplit(':').next().unwrap_or("") { None } else { Some(format!("Tripo has no {a} for a {rig_type} rig; used {p}")) };
    match rig_type {
        "biped" if BIPED_PRESETS.contains(&a.as_str()) => Ok((format!("preset:biped:{a}"), (a == "chop").then(|| "chop came back the same motion as slash on Tripo (October 2026)".to_string()))),
        "biped" => Err(match NOT_PRESETS.iter().find(|(k, _)| *k == a) {
            Some((_, instead)) => format!("{a} isn't a Tripo biped preset; try {instead} (`mb model presets` lists them)"),
            None => format!("{a} isn't a Tripo biped preset (`mb model presets` lists them; preset:biped:{a} sends it as is)"),
        }),
        "quadruped" | "hexapod" | "octopod" if moving || a == "idle" => {
            let p = format!("preset:{rig_type}:walk");
            let note = sub(&p);
            Ok((p, note))
        }
        "serpentine" | "aquatic" if moving || a == "idle" => {
            let p = format!("preset:{rig_type}:march");
            let note = if a == "swim" || a == "march" || a == "slither" { None } else { Some(format!("Tripo has one {rig_type} clip ({p}); used it for {a}")) };
            Ok((p, note))
        }
        _ => Err(format!("Tripo has no {a} animation for a {rig_type} rig (biped: preset:biped:*; quadruped, hexapod, octopod: walk; serpentine, aquatic: march)")),
    }
}

/// Checks `--animate` names before anything is spent: each must be a preset
/// for `rig_type`, or (rig type not known yet) for some rig type. Returns notes
/// (duplicates, fillers to trim).
pub fn check_actions(rig_type: Option<&str>, actions: &[String]) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    for a in actions {
        let ok = match rig_type {
            Some(t) => preset(t, a).map(|_| ()),
            None => ["biped", "quadruped", "serpentine"].iter().map(|t| preset(t, a).map(|_| ())).find(Result::is_ok).unwrap_or_else(|| preset("biped", a).map(|_| ())),
        };
        ok?;
        if let Some(n) = preset_note(a).filter(|_| rig_type.is_none_or(|t| t == "biped")) {
            notes.push(format!("{a}: {n}"));
        }
    }
    let has = |n: &str| actions.iter().any(|a| a.rsplit(':').next().unwrap_or(a).eq_ignore_ascii_case(n));
    if has("chop") && has("slash") {
        notes.push("chop and slash came back as the same motion (October 2026): one of them is $0.10 for nothing".into());
    }
    Ok(notes)
}

fn status_of(d: &Value) -> String {
    let s = d["status"].as_str().unwrap_or("?");
    match d["progress"].as_u64() {
        Some(p) if s == "running" => format!("{s} {p}%"),
        _ => s.to_string(),
    }
}

impl Tripo {
    pub fn new(key: String, http: Box<dyn Transport>, poll: Poll) -> Tripo {
        let pinned = std::env::var("TRIPO_BASE_URL").ok().filter(|u| !u.is_empty());
        let settled = std::sync::atomic::AtomicBool::new(pinned.is_some());
        let base = std::sync::Mutex::new(pinned.map(|u| u.trim_end_matches('/').to_string()).unwrap_or_else(|| BASES[0].to_string()));
        Tripo { key, http, poll, base, settled, slots: None, maimbrain: false }
    }

    /// On Maimbrain's key: the server at `server`, signed in with `token`.
    pub fn on_maimbrain(server: &str, token: &str, http: Box<dyn Transport>, poll: Poll) -> Tripo {
        let base = format!("{}/api/v1/generate/tripo", server.trim_end_matches('/'));
        Tripo { key: token.to_string(), http, poll, base: std::sync::Mutex::new(base), settled: true.into(), slots: None, maimbrain: true }
    }

    /// With the `mb login` credentials.
    pub fn signed_in(http: Box<dyn Transport>, poll: Poll) -> Result<Tripo, String> {
        let c = crate::remote::credentials()?;
        Ok(Tripo::on_maimbrain(&c.server, &c.token, http, poll))
    }

    /// Queues generation tasks with other `mb` processes through lock files in `dir`.
    pub fn with_slots(mut self, dir: Option<std::path::PathBuf>) -> Tripo {
        self.slots = dir;
        self
    }

    /// Takes a free slot of `pool` (`n` of them), waiting while all are
    /// held by other `mb` runs on this machine. Gives up waiting after the
    /// task timeout and goes ahead (the server's refusal is retried anyway).
    pub fn slot(&self, pool: &str, n: usize) -> Slot {
        let Some(dir) = self.slots.as_ref().filter(|d| std::fs::create_dir_all(d).is_ok()) else { return Slot(None) };
        let t0 = std::time::Instant::now();
        let mut said = false;
        loop {
            for i in 0..n {
                let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(format!("{pool}-{i}.lock")));
                if let Ok(f) = f
                    && f.try_lock().is_ok()
                {
                    if said {
                        eprintln!("    tripo: a {pool} slot is free after {:.0} s; starting", t0.elapsed().as_secs_f32());
                    }
                    return Slot(Some(f));
                }
            }
            if !said {
                eprintln!("    tripo: {n} {pool} tasks are already running from mb on this machine (Tripo runs {n} at once per account); waiting for one to finish");
                said = true;
            }
            if t0.elapsed() > self.poll.timeout {
                return Slot(None);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.lock().unwrap())
    }

    /// POSTs, trying the other region once if the key is refused before the region is settled.
    fn post(&self, path: &str, ctype: &str, body: Vec<u8>) -> Result<super::provider::Response, String> {
        use std::sync::atomic::Ordering;
        let r = self.http.post(&self.url(path), &self.auth(), ctype, body.clone())?;
        if r.status == 401 && !self.settled.load(Ordering::Relaxed) {
            *self.base.lock().unwrap() = BASES[1].to_string();
            self.settled.store(true, Ordering::Relaxed);
            let r2 = self.http.post(&self.url(path), &self.auth(), ctype, body)?;
            if r2.status != 401 {
                return Ok(r2);
            }
            *self.base.lock().unwrap() = BASES[0].to_string();
            return Ok(r);
        }
        self.settled.store(true, Ordering::Relaxed);
        Ok(r)
    }

    /// The account's spendable credits (`balance`; `frozen` ones are held by
    /// tasks still running). `GET /user/balance` on the V3 base, then on the
    /// V2 OpenAPI base the SDKs use.
    pub fn balance(&self) -> Result<f64, String> {
        if self.maimbrain {
            return Err("tripo on Maimbrain's keys has no balance (the budget is mb models' line)".into());
        }
        let base = self.base.lock().unwrap().clone();
        let v2 = if base.contains("tripo3d.com") { "https://api.tripo3d.com/v2/openapi" } else { "https://api.tripo3d.ai/v2/openapi" };
        let mut last = String::new();
        for url in [format!("{base}/user/balance"), format!("{v2}/user/balance")] {
            match self.http.get(&url, &self.auth()) {
                Ok(r) => {
                    let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
                    if r.status == 200
                        && v["code"].as_i64() == Some(0)
                        && let Some(b) = v["data"]["balance"].as_f64()
                    {
                        return Ok(b);
                    }
                    last = format!("HTTP {}{}", r.status, v["message"].as_str().map(|m| format!(": {m}")).unwrap_or_default());
                }
                Err(e) => last = e,
            }
        }
        Err(format!("tripo: couldn't read the account's balance ({last})"))
    }

    fn auth(&self) -> Vec<(String, String)> {
        vec![("Authorization".to_string(), format!("Bearer {}", self.key))]
    }

    fn data(&self, r: super::provider::Response) -> Result<Value, String> {
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        if self.maimbrain && (r.status != 200 || v["code"].as_i64().unwrap_or(-1) != 0) {
            let msg = v["message"].as_str().or(v["error"].as_str()).map(str::to_string).unwrap_or_else(|| format!("HTTP {}", r.status));
            return Err(match r.status {
                401 => "Maimbrain's keys: your sign-in has expired or was revoked; run mb login".into(),
                402..=404 => format!("Maimbrain's keys: {msg}"),
                _ => format!("Maimbrain's keys: tripo: {msg}{}", v["suggestion"].as_str().map(|s| format!(" ({s})")).unwrap_or_default()),
            });
        }
        if r.status != 200 || v["code"].as_i64().unwrap_or(-1) != 0 {
            let msg = v["message"].as_str().map(str::to_string).unwrap_or_else(|| String::from_utf8_lossy(&r.body).chars().take(300).collect());
            let hint = match (r.status, v["code"].as_i64()) {
                (401, _) | (_, Some(1000 | 1001)) => " (refused by both the international and China regions: check the key with `mb keys list`, or set TRIPO_BASE_URL)".to_string(),
                (_, Some(2010)) => " (top up at https://developers.tripo3d.ai)".to_string(),
                (429, _) | (_, Some(2000 | 1007)) => " (too many tasks at once: Tripo runs 5 P-series or 10 H-series per account; mb waited MB_MODEL_TIMEOUT seconds for a slot)".to_string(),
                _ => v["suggestion"].as_str().map(|s| format!(" ({s})")).unwrap_or_default(),
            };
            return Err(format!("tripo: HTTP {}{}: {msg}{hint}", r.status, v["code"].as_i64().map(|c| format!(" code {c}")).unwrap_or_default()));
        }
        Ok(v["data"].clone())
    }

    /// Creates a task, retrying with backoff while the account is at its
    /// concurrency limit (429 code 2000) or rate limit (code 1007), up to the
    /// task timeout.
    fn create(&self, path: &str, body: &Value) -> Result<String, String> {
        let t0 = std::time::Instant::now();
        let mut attempt = 0;
        loop {
            let r = self.post(path, "application/json", serde_json::to_vec(body).unwrap())?;
            let v = serde_json::from_slice::<Value>(&r.body).unwrap_or_default();
            let code = v["code"].as_i64();
            // The server's other refusals (a daily limit) are 429s too, without these codes.
            let busy = (r.status == 429 && !self.maimbrain) || matches!(code, Some(2000 | 1007));
            if busy && t0.elapsed() < self.poll.timeout {
                let wait = backoff(&self.poll, attempt);
                let why = match (self.maimbrain, v["message"].as_str()) {
                    (true, Some(m)) => m.to_string(),
                    _ if code == Some(1007) => "too many requests just now".into(),
                    _ => "the account's tasks are at Tripo's limit (5 P-series or 10 H-series at once)".into(),
                };
                eprintln!("    tripo: {why}; trying again in {:.0} s", wait.as_secs_f32());
                std::thread::sleep(wait);
                attempt += 1;
                continue;
            }
            let d = self.data(r)?;
            return d["task_id"].as_str().map(str::to_string).ok_or_else(|| "tripo: no task_id in the response".into());
        }
    }

    /// Waits for a task; returns its data and adds its credits to `credits`.
    fn wait(&self, what: &str, id: &str, credits: &mut f64) -> Result<Value, String> {
        let d = self.poll.wait(&format!("tripo {what}"), || {
            let d = self.data(self.http.get(&self.url(&format!("/tasks/{id}")), &self.auth())?)?;
            let st = d["status"].as_str().unwrap_or("");
            match st {
                "queued" | "running" => Ok((None, status_of(&d))),
                "success" => Ok((Some(d.clone()), "done".into())),
                _ => Err(format!(
                    "tripo {what} {st}: {}{} (task {id})",
                    d["error_message"].as_str().unwrap_or("no reason given"),
                    d["error_code"].as_i64().map(|c| format!(", code {c}")).unwrap_or_default()
                )),
            }
        })?;
        *credits += d["credits_consumed"].as_f64().unwrap_or(0.0);
        Ok(d)
    }

    fn model_url(d: &Value) -> Option<String> {
        let o = &d["output"];
        o["model_url"]
            .as_str()
            .or_else(|| o["model_urls"].as_array().and_then(|a| a.first()).and_then(Value::as_str))
            .or_else(|| o["model_urls"].as_object().and_then(|m| m.values().next()).and_then(Value::as_str))
            .or_else(|| o["pbr_model"].as_str())
            .or_else(|| o["model"].as_str())
            .map(str::to_string)
    }

    fn fetch(&self, what: &str, d: &Value) -> Result<Vec<u8>, String> {
        let url = Tripo::model_url(d).ok_or(format!("tripo {what}: the result has no model_url"))?;
        let b = download(self.http.as_ref(), "tripo", &url)?;
        if !b.starts_with(b"glTF") {
            return Err(format!("tripo {what}: the download isn't a .glb"));
        }
        Ok(b)
    }

    fn upload(&self, img: &super::provider::RefImage) -> Result<String, String> {
        let boundary = format!("mbboundary{:016x}", super::mock::hash(&[&img.bytes]));
        let ext = if img.mime == "image/jpeg" { "jpg" } else { "png" };
        let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"ref.{ext}\"\r\nContent-Type: {}\r\n\r\n", img.mime).into_bytes();
        body.extend_from_slice(&img.bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let r = self.post("/files", &format!("multipart/form-data; boundary={boundary}"), body)?;
        let d = self.data(r)?;
        d["file_token"].as_str().map(str::to_string).ok_or_else(|| "tripo: the upload returned no file_token".into())
    }
}

impl Provider for Tripo {
    fn name(&self) -> &'static str {
        if self.maimbrain { "tripo on Maimbrain's keys" } else { "tripo" }
    }

    fn via(&self) -> Option<&'static str> {
        self.maimbrain.then_some("maimbrain")
    }

    fn image(&self, _req: &ImageRequest) -> Result<Output, String> {
        Err("tripo makes 3D models (mb model), not images".into())
    }

    fn balance_usd(&self) -> Option<Result<f64, String>> {
        (!self.maimbrain).then(|| self.balance().map(|credits| credits * 0.01))
    }

    fn audio(&self, _req: &AudioRequest) -> Result<Output, String> {
        Err("tripo makes 3D models (mb model), not sound".into())
    }

    fn model(&self, req: &ModelRequest) -> Result<ModelOutput, String> {
        let mut credits = 0.0;
        let mut out = ModelOutput::default();
        let p_series = req.model.starts_with('P');
        let face_limit = if p_series { req.polys.clamp(50, 20_000) } else { req.polys.clamp(500, 1_500_000) };
        let seed = req.seed.map(|s| (s % 2_000_000_000) as i64);
        let mut body = json!({ "model": req.model, "face_limit": face_limit, "texture": true, "pbr": true });
        if let Some(s) = seed {
            body["model_seed"] = json!(s);
            body["texture_seed"] = json!(s);
        }
        let (path, what) = match &req.image {
            Some(img) => {
                body["input"] = json!(self.upload(img)?);
                ("/generation/image-to-model", "image-to-model")
            }
            None => {
                body["prompt"] = json!(req.prompt.chars().take(1024).collect::<String>());
                if let Some(s) = seed {
                    body["image_seed"] = json!(s);
                }
                if let Some(n) = req.negative.as_ref().filter(|n| !n.is_empty()) {
                    body["negative_prompt"] = json!(n.chars().take(255).collect::<String>());
                }
                ("/generation/text-to-model", "text-to-model")
            }
        };
        // One of the account's generation slots, held until the model is done.
        let (pool, n) = pool_of(&req.model);
        let slot = self.slot(pool, n);
        let gen_id = self.create(path, &body)?;
        out.tasks.push((what.into(), gen_id.clone()));
        let d = self.wait(what, &gen_id, &mut credits)?;
        drop(slot);
        out.glb = self.fetch(what, &d)?;
        out.cost_usd = Some(credits * 0.01);
        if !(req.rig || !req.animations.is_empty()) {
            return Ok(out);
        }
        // Rigging and animation: a failure here keeps the model and says why.
        if let Err(e) = self.rig_and_animate(&gen_id, req.rig_type.as_deref(), None, &req.animations, &mut out, &mut credits) {
            out.notes.push(format!("{e}; kept it unrigged"));
        }
        out.cost_usd = Some(credits * 0.01);
        Ok(out)
    }

    /// Rigs a model made earlier, from its generation task (no upload) or its
    /// uploaded .glb, and/or retargets clips onto a rig made earlier.
    fn rig(&self, req: &super::provider::RigRequest) -> Result<ModelOutput, String> {
        let mut credits = 0.0;
        let mut out = ModelOutput::default();
        let input = match (&req.rig_task, &req.task) {
            (Some(_), _) => String::new(),
            (None, Some(t)) => t.clone(),
            (None, None) => {
                if req.glb.is_empty() {
                    return Err("tripo: no generation task recorded and no .glb to upload".into());
                }
                eprintln!("    tripo: no generation task recorded; uploading the model ({} KB)", req.glb.len().div_ceil(1024));
                let token = self.upload_glb(&req.glb)?;
                out.tasks.push(("upload".into(), token.clone()));
                token
            }
        };
        let r = self.rig_and_animate(&input, req.rig_type.as_deref(), req.rig_task.as_deref(), &req.animations, &mut out, &mut credits);
        out.cost_usd = Some(credits * 0.01);
        r.map(|()| out)
    }
}

impl Tripo {
    /// Rigs `input` (a generation task or a file token) unless `rig_task` is
    /// a rig made earlier, then retargets each clip onto the rig. A rig that
    /// can't be made is the error; a clip that fails is a note.
    fn rig_and_animate(&self, input: &str, rig_type: Option<&str>, rig_task: Option<&str>, animations: &[String], out: &mut ModelOutput, credits: &mut f64) -> Result<(), String> {
        let (rig_id, rig_type) = match rig_task {
            Some(id) => (id.to_string(), rig_type.unwrap_or("biped").to_string()),
            None => {
                let rig_type = match rig_type {
                    Some(t) => t.to_string(),
                    None => {
                        let id = self.create("/animations/rig-check", &json!({ "input": input }))?;
                        out.tasks.push(("rig-check".into(), id.clone()));
                        let d = self.wait("rig-check", &id, credits)?;
                        if d["output"]["riggable"].as_bool() == Some(false) {
                            return Err("Tripo says this model can't be rigged (try --rig-type)".into());
                        }
                        d["output"]["rig_type"].as_str().unwrap_or("biped").to_string()
                    }
                };
                let rig_model = if rig_type == "biped" { "v1.0-20240301" } else { "v2.5-20260210" };
                let id = self.create("/animations/rig", &json!({ "input": input, "model": rig_model, "rig_type": rig_type, "spec": "tripo", "out_format": "glb" }))?;
                out.tasks.push((format!("rig ({rig_type})"), id.clone()));
                let d = self.wait("rig", &id, credits)?;
                out.glb = self.fetch("rig", &d)?;
                out.rigged = true;
                out.text = Some(format!("rig type {rig_type}"));
                (id, rig_type)
            }
        };
        out.rig = Some((rig_id.clone(), rig_type.clone()));
        for a in animations {
            let (p, note) = match preset(&rig_type, a) {
                Ok(x) => x,
                Err(e) => {
                    out.notes.push(e);
                    continue;
                }
            };
            out.notes.extend(note);
            let body = json!({ "input": rig_id, "animation": p, "out_format": "glb", "bake_animation": true, "export_with_geometry": true, "animate_in_place": true });
            let got = self.create("/animations/retarget", &body).and_then(|id| {
                out.tasks.push((format!("animate {a}"), id.clone()));
                let d = self.wait(&format!("animate {a}"), &id, credits)?;
                self.fetch("animate", &d)
            });
            match got {
                Ok(b) => out.clips.push((a.clone(), b)),
                Err(e) => out.notes.push(format!("{e}; no {a} clip")),
            }
        }
        Ok(())
    }

    /// Uploads a .glb (`POST /files`) for a rig of a model with no generation task.
    fn upload_glb(&self, glb: &[u8]) -> Result<String, String> {
        let (ctype, body) = super::provider::multipart(&[], &[("file", "model.glb", "model/gltf-binary", glb)]);
        let d = self.data(self.post("/files", &ctype, body)?)?;
        d["file_token"].as_str().map(str::to_string).ok_or_else(|| "tripo: the upload returned no file_token".into())
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::tests::{Fake, json as reply};
    use super::super::provider::{RefImage, Response};
    use super::*;
    use std::rc::Rc;
    use std::time::Duration;

    fn tripo(fake: &Rc<Fake>) -> Tripo {
        Tripo::new("tsk_test".into(), Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::from_secs(5) })
    }

    fn task(id: &str, status: &str, progress: u32, output: Value, credits: f64) -> Response {
        reply(200, json!({ "code": 0, "data": { "task_id": id, "type": "x", "status": status, "progress": progress, "output": output, "credits_consumed": credits, "created_at": "2026-10-09T12:00:00Z" } }))
    }

    fn file(b: &[u8]) -> Response {
        Response { status: 200, content_type: "model/gltf-binary".into(), body: b.to_vec() }
    }

    #[test]
    fn a_swimming_fish_text_rig_check_rig_and_retarget() {
        let glb = b"glTF\x02\0\0\0fake".to_vec();
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "code": 0, "data": { "task_id": "task_gen" } })),
            task("task_gen", "queued", 0, json!(null), 0.0),
            task("task_gen", "running", 40, json!(null), 0.0),
            task("task_gen", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/output/model.glb?auth_key=1", "rendered_image_url": "https://cdn.tripo3d.ai/p.webp" }), 40.0),
            file(&glb),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_check" } })),
            task("task_check", "success", 100, json!({ "riggable": true, "rig_type": "aquatic" }), 0.0),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_rig" } })),
            task("task_rig", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/output/rigged.glb" }), 25.0),
            file(&glb),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_swim" } })),
            // Batch results have used model_urls; read both.
            task("task_swim", "success", 100, json!({ "model_urls": ["https://cdn.tripo3d.ai/output/swim.glb"] }), 10.0),
            file(&glb),
        ]));
        let req = ModelRequest {
            model: "P1-20260311".into(),
            prompt: "a stylized koi".into(),
            polys: 8000,
            rig: true,
            animations: vec!["swim".into()],
            seed: Some(7),
            ..ModelRequest::default()
        };
        let out = tripo(&fake).model(&req).unwrap();
        assert!(out.rigged && out.notes.is_empty(), "{:?}", out.notes);
        assert_eq!(out.clips.len(), 1);
        assert!((out.cost_usd.unwrap() - 0.75).abs() < 1e-9);
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://openapi.tripo3d.ai/v3/generation/text-to-model");
        assert_eq!(sent[0].1[0], ("Authorization".to_string(), "Bearer tsk_test".to_string()));
        assert_eq!(sent[0].2["model"], "P1-20260311");
        assert_eq!(sent[0].2["face_limit"], 8000);
        assert_eq!(sent[0].2["model_seed"], 7);
        assert_eq!(sent[1].0, "GET https://openapi.tripo3d.ai/v3/tasks/task_gen");
        assert!(sent[4].0.starts_with("GET https://cdn.tripo3d.ai/output/model.glb") && sent[4].1.is_empty(), "no key to the file host");
        assert_eq!(sent[5].2["input"], "task_gen");
        assert_eq!(sent[7].2["rig_type"], "aquatic");
        assert_eq!(sent[7].2["model"], "v2.5-20260210");
        assert_eq!(sent[10].2["animation"], "preset:aquatic:march");
        assert_eq!(sent[10].2["input"], "task_rig");
        assert_eq!(sent[10].2["animate_in_place"], true);
    }

    #[test]
    fn a_model_made_earlier_is_rigged_from_its_task_and_clips_are_added_to_its_rig() {
        use super::super::provider::RigRequest;
        let glb = b"glTF\x02\0\0\0fake".to_vec();
        // Rig from the generation task (no upload, no rig-check with a rig type), then one clip.
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "code": 0, "data": { "task_id": "task_rig" } })),
            task("task_rig", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/rigged.glb" }), 25.0),
            file(&glb),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_slash" } })),
            task("task_slash", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/slash.glb" }), 10.0),
            file(&glb),
        ]));
        let req = RigRequest { task: Some("task_gen".into()), rig_type: Some("biped".into()), animations: vec!["slash".into()], ..RigRequest::default() };
        let out = tripo(&fake).rig(&req).unwrap();
        assert!(out.rigged && out.glb == glb && out.clips.len() == 1);
        assert_eq!(out.rig, Some(("task_rig".to_string(), "biped".to_string())));
        assert!((out.cost_usd.unwrap() - 0.35).abs() < 1e-9);
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://openapi.tripo3d.ai/v3/animations/rig");
        assert_eq!((sent[0].2["input"].as_str(), sent[0].2["model"].as_str()), (Some("task_gen"), Some("v1.0-20240301")));
        assert_eq!(sent[3].2["input"], "task_rig");
        assert_eq!(sent[3].2["animation"], "preset:biped:slash");
        drop(sent);
        // More clips later: only the retarget, on the recorded rig.
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "code": 0, "data": { "task_id": "task_hit" } })),
            task("task_hit", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/hit.glb" }), 10.0),
            file(&glb),
        ]));
        let req = RigRequest { rig_task: Some("task_rig".into()), rig_type: Some("biped".into()), animations: vec!["hit_to_body_01".into()], ..RigRequest::default() };
        let out = tripo(&fake).rig(&req).unwrap();
        assert!(out.glb.is_empty() && out.clips.len() == 1 && (out.cost_usd.unwrap() - 0.10).abs() < 1e-9);
        assert_eq!(fake.sent.borrow()[0].2["input"], "task_rig");
        // No generation task (an imported model): the .glb is uploaded and rigged from its file token.
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "code": 0, "data": { "file_token": "file_1" } })),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_check" } })),
            task("task_check", "success", 100, json!({ "riggable": true, "rig_type": "quadruped" }), 0.0),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_rig2" } })),
            task("task_rig2", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/r.glb" }), 25.0),
            file(&glb),
        ]));
        let out = tripo(&fake).rig(&RigRequest { glb: glb.clone(), ..RigRequest::default() }).unwrap();
        assert_eq!(out.rig, Some(("task_rig2".to_string(), "quadruped".to_string())));
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://openapi.tripo3d.ai/v3/files");
        assert!(sent[0].2["text"].as_str().unwrap().contains("filename=\"model.glb\""), "uploaded as a .glb");
        assert_eq!(sent[1].2["input"], "file_1");
        assert_eq!(sent[3].2["model"], "v2.5-20260210");
        drop(sent);
        // Not riggable: an error (there's nothing to keep).
        let fake = Rc::new(Fake::new(vec![reply(200, json!({ "code": 0, "data": { "task_id": "c" } })), task("c", "success", 100, json!({ "riggable": false }), 0.0)]));
        assert!(tripo(&fake).rig(&RigRequest { task: Some("t".into()), ..RigRequest::default() }).unwrap_err().contains("can't be rigged"));
    }

    #[test]
    fn errors_and_partial_results() {
        let glb = b"glTF\x02\0\0\0".to_vec();
        // An image upload, then the rig fails: the unrigged model comes back with a note.
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "code": 0, "data": { "file_token": "file_1" } })),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_gen" } })),
            task("task_gen", "success", 100, json!({ "model_url": "https://x/m.glb" }), 50.0),
            file(&glb),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_rig" } })),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_rig", "status": "failed", "error_code": 2018, "error_message": "Model too complex" } })),
        ]));
        let req = ModelRequest {
            model: "P1-20260311".into(),
            prompt: "x".into(),
            image: Some(RefImage { bytes: b"\x89PNG....".to_vec(), mime: "image/png".into() }),
            polys: 90_000,
            rig: true,
            rig_type: Some("biped".into()),
            ..ModelRequest::default()
        };
        let out = tripo(&fake).model(&req).unwrap();
        assert!(!out.rigged);
        assert!(out.notes[0].contains("Model too complex") && out.notes[0].contains("unrigged"), "{:?}", out.notes);
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://openapi.tripo3d.ai/v3/files");
        assert_eq!(sent[1].2["input"], "file_1");
        assert_eq!(sent[1].2["face_limit"], 20_000, "P1's maximum");
        assert_eq!(sent[4].2["model"], "v1.0-20240301");
        // A refused request is an error with Tripo's message.
        let fake = Rc::new(Fake::new(vec![reply(403, json!({ "code": 2010, "message": "Insufficient credits", "suggestion": "top up" }))]));
        let e = tripo(&fake).model(&ModelRequest { model: "P1-20260311".into(), prompt: "x".into(), ..ModelRequest::default() }).unwrap_err();
        assert!(e.contains("Insufficient credits") && e.contains("developers.tripo3d.ai"), "{e}");
    }

    #[test]
    fn the_balance_is_checked_before_a_batch() {
        let fake = Rc::new(Fake::new(vec![reply(200, json!({ "code": 0, "data": { "balance": 250.0, "frozen": 60.0 } }))]));
        let t = tripo(&fake);
        assert_eq!(t.balance(), Ok(250.0));
        assert_eq!(fake.sent.borrow()[0].0, "GET https://openapi.tripo3d.ai/v3/user/balance");
        // Not on V3: the V2 path.
        let fake = Rc::new(Fake::new(vec![reply(404, json!({ "message": "not found" })), reply(200, json!({ "code": 0, "data": { "balance": 40.0, "frozen": 0 } }))]));
        let t = tripo(&fake);
        assert_eq!(t.balance(), Ok(40.0));
        assert_eq!(fake.sent.borrow()[1].0, "GET https://api.tripo3d.ai/v2/openapi/user/balance");
        // $0.40 can't cover two $0.40 models: refused, with the shortfall.
        let fake2 = Rc::new(Fake::new(vec![reply(200, json!({ "code": 0, "data": { "balance": 40.0, "frozen": 0 } }))]));
        let e = super::super::provider::check_balance(&tripo(&fake2), 0.8).unwrap_err();
        assert!(e.contains("$0.40 (40 credits) and this needs about $0.80 (80 credits): $0.40 (40 credits) short"), "{e}");
        let fake3 = Rc::new(Fake::new(vec![reply(200, json!({ "code": 0, "data": { "balance": 400.0, "frozen": 0 } }))]));
        assert!(super::super::provider::check_balance(&tripo(&fake3), 0.8).is_ok());
        // Unreadable: a note, and the batch goes ahead.
        let fake4 = Rc::new(Fake::new(vec![]));
        assert!(super::super::provider::check_balance(&tripo(&fake4), 0.8).is_ok());
    }

    #[test]
    fn a_key_from_the_other_region_is_found() {
        let refused = || reply(401, json!({ "code": 2, "message": "Invalid API key" }));
        // An international key: settled on the first request.
        let fake = Rc::new(Fake::new(vec![task("t1", "queued", 0, json!({}), 0.0), task("t1", "queued", 0, json!({}), 0.0)]));
        let t = tripo(&fake);
        t.create("/generation/text-to-model", &json!({})).unwrap();
        t.create("/generation/text-to-model", &json!({})).unwrap();
        let sent: Vec<String> = fake.sent.borrow().iter().map(|s| s.0.clone()).collect();
        assert_eq!(sent, ["https://openapi.tripo3d.ai/v3/generation/text-to-model"; 2]);
        // A China-mainland key: the international region refuses it once, then everything goes to .com.
        let fake = Rc::new(Fake::new(vec![refused(), task("t1", "queued", 0, json!({}), 0.0), task("t1", "running", 5, json!({}), 0.0)]));
        let t = tripo(&fake);
        t.create("/generation/text-to-model", &json!({})).unwrap();
        t.data(t.http.get(&t.url("/tasks/t1"), &t.auth()).unwrap()).unwrap();
        let sent: Vec<String> = fake.sent.borrow().iter().map(|s| s.0.clone()).collect();
        assert_eq!(sent, [
            "https://openapi.tripo3d.ai/v3/generation/text-to-model",
            "https://openapi.tripo3d.com/v3/generation/text-to-model",
            "GET https://openapi.tripo3d.com/v3/tasks/t1",
        ]);
        // A bad key: refused by both, reported once with the hint.
        let fake = Rc::new(Fake::new(vec![refused(), refused()]));
        let e = tripo(&fake).create("/generation/text-to-model", &json!({})).unwrap_err();
        assert!(e.contains("HTTP 401") && e.contains("both"), "{e}");
    }

    #[test]
    fn a_creation_refused_for_concurrency_is_retried() {
        let busy = || reply(429, json!({ "code": 2000, "message": "You have exceeded the limit of generation", "suggestion": "Try again later." }));
        let fake = Rc::new(Fake::new(vec![
            busy(),
            reply(429, json!({ "code": 1007, "message": "Rate limit exceeded" })),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_gen" } })),
            task("task_gen", "success", 100, json!({ "model_url": "https://cdn.example/m.glb" }), 40.0),
            file(b"glTF\x02\0\0\0"),
        ]));
        let out = tripo(&fake).model(&ModelRequest { prompt: "a mech".into(), model: "P1-20260311".into(), polys: 4000, ..Default::default() }).unwrap();
        assert_eq!(out.tasks, vec![("text-to-model".to_string(), "task_gen".to_string())]);
        assert_eq!(fake.sent.borrow().iter().filter(|c| c.0.ends_with("/generation/text-to-model")).count(), 3);
        // Past the timeout, the refusal is the error.
        let fake = Rc::new(Fake::new((0..3).map(|_| busy()).collect()));
        let t = Tripo::new("tsk_test".into(), Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::ZERO });
        let e = t.model(&ModelRequest { prompt: "a mech".into(), model: "P1-20260311".into(), ..Default::default() }).unwrap_err();
        assert!(e.contains("code 2000") && e.contains("5 P-series"), "{e}");
    }

    #[test]
    fn on_maimbrains_key_the_same_calls_go_to_the_server() {
        let glb = b"glTF\x02\0\0\0fake".to_vec();
        let fake = Rc::new(Fake::new(vec![
            reply(429, json!({ "code": 2000, "message": "2 of your p-series tasks are already running on Maimbrain's Tripo key" })),
            reply(200, json!({ "code": 0, "data": { "task_id": "task_gen" } })),
            task("task_gen", "success", 100, json!({ "model_url": "https://cdn.tripo3d.ai/m.glb?sig=1" }), 40.0),
            file(&glb),
        ]));
        let t = Tripo::on_maimbrain("https://maimbrain.test/", "cli_tok", Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::from_secs(5) });
        assert_eq!((t.name(), t.via()), ("tripo on Maimbrain's keys", Some("maimbrain")));
        assert!(t.balance_usd().is_none(), "the budget, not a Tripo balance");
        let out = t.model(&ModelRequest { prompt: "a koi".into(), model: "P1-20260311".into(), polys: 4000, ..Default::default() }).unwrap();
        assert_eq!(out.glb, glb);
        assert!((out.cost_usd.unwrap() - 0.40).abs() < 1e-9);
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://maimbrain.test/api/v1/generate/tripo/generation/text-to-model");
        assert_eq!(sent[1].0, sent[0].0, "the server's busy answer is retried");
        assert_eq!(sent[0].1[0], ("Authorization".to_string(), "Bearer cli_tok".to_string()));
        assert_eq!(sent[2].0, "GET https://maimbrain.test/api/v1/generate/tripo/tasks/task_gen");
        assert!(sent[3].0.starts_with("GET https://cdn.tripo3d.ai/") && sent[3].1.is_empty(), "no token to the file host");
        drop(sent);
        // The server's refusals: no other region, no retry, its message.
        let fake = Rc::new(Fake::new(vec![
            reply(401, json!({ "error": "sign in required" })),
            reply(402, json!({ "code": "over_budget", "message": "this would cost about $0.4000, more than the $0.10 left" })),
            reply(429, json!({ "code": "daily_limit", "message": "that's the most 3D tasks for one day on Maimbrain's keys; try tomorrow" })),
            reply(403, json!({ "code": "not_allowed", "message": "@ada can't make 3D models on Maimbrain's keys" })),
        ]));
        let t = Tripo::on_maimbrain("https://maimbrain.test", "cli_tok", Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::from_secs(5) });
        let req = ModelRequest { prompt: "a koi".into(), model: "P1-20260311".into(), ..Default::default() };
        assert!(t.model(&req).unwrap_err().contains("run mb login"));
        assert!(t.model(&req).unwrap_err().contains("$0.10 left"));
        assert!(t.model(&req).unwrap_err().contains("try tomorrow"));
        assert_eq!(t.model(&req).unwrap_err(), "Maimbrain's keys: @ada can't make 3D models on Maimbrain's keys");
        assert_eq!(fake.sent.borrow().len(), 4);
        assert!(fake.sent.borrow().iter().all(|s| s.0.starts_with("https://maimbrain.test/")));
        assert!(t.balance().is_err());
    }

    #[test]
    fn slots_queue_tasks_across_runs() {
        let dir = std::env::temp_dir().join(format!("mb-tripo-slots-{}", std::process::id()));
        let fake = Rc::new(Fake::new(vec![]));
        let t = Tripo::new("k".into(), Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::ZERO }).with_slots(Some(dir.clone()));
        let held: Vec<Slot> = (0..2).map(|_| t.slot("p-series", 2)).collect();
        assert!(held.iter().all(|s| s.0.is_some()), "two free slots");
        assert!(t.slot("p-series", 2).0.is_none(), "a third waits (here: gives up at once)");
        drop(held);
        assert!(t.slot("p-series", 2).0.is_some(), "freed when dropped");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn presets() {
        assert_eq!(preset("biped", "run").unwrap(), ("preset:biped:run".to_string(), None));
        assert_eq!(preset("aquatic", "swim").unwrap().0, "preset:aquatic:march");
        assert!(preset("aquatic", "idle").unwrap().1.is_some());
        assert!(preset("quadruped", "run").unwrap().1.unwrap().contains("used preset:quadruped:walk"));
        assert!(preset("avian", "fly").is_err());
        assert_eq!(preset("aquatic", "preset:biped:swim").unwrap().0, "preset:biped:swim");
        // The real catalog: a bad name is refused before anything is paid, with what to ask for.
        assert!(preset("biped", "punch").unwrap_err().contains("box_01"));
        assert!(preset("biped", "defeat_01").unwrap_err().contains("defeat_02"));
        assert!(preset("biped", "moonwalk").unwrap_err().contains("mb model presets"));
        assert!(preset("biped", "chop").unwrap().1.unwrap().contains("same motion as slash"));
        assert_eq!(preset("biped", "hit_to_body_01").unwrap().0, "preset:biped:hit_to_body_01");
        assert!(check_actions(Some("biped"), &["idle".into(), "punch".into()]).is_err());
        let notes = check_actions(None, &["slash".into(), "chop".into(), "idle".into()]).unwrap();
        assert!(notes.iter().any(|n| n.contains("Idle=0..4")) && notes.iter().any(|n| n.contains("$0.10 for nothing")), "{notes:?}");
        assert!(check_actions(None, &["swim".into(), "walk".into()]).is_ok());
        let text = presets_text();
        assert!(text.contains("chop*") && text.contains("wave_goodbye_02") && text.contains("strafe"), "{text}");
        assert_eq!(BIPED_PRESETS.len(), 101);
    }
}
