//! `mb play`: a Studio game played headless, for Studio's `play_test` and
//! `screenshot` (the server's `Studio::Builder`), and for anyone at a terminal.
//!
//! The run is the platform's own: the bundle in the headless runner `mb
//! replay` uses (Deno, the phone's runtime and mb-host, no GPU), fed a replay
//! log written here from a script of touches, keys, pad buttons and debug
//! picks. The engine reads three debug parameters (games/studio-engine
//! `headless.rs`): `studio_dump` makes it describe the canvas at the frames
//! asked for, `studio_trace` logs its events, `studio_debug` makes debug
//! picks by label. Frames are drawn here on the CPU from those descriptions
//! and the bundle's own PNGs: the playfield exactly, pixel for pixel, with
//! the score in a small pixel font. The title and game-over cards' text and
//! the particles aren't drawn (the summary names the mode instead).
//!
//!     mb play game.mbx --json --out shots --seconds 5 --frames 4
//!         [--at 1,2.5] [--script input.json] [--orientation portrait|sideways]
//!         [--debug picks.json] [--seed N] [--no-start]
//!
//! The script is JSON, times in seconds from the start and positions in the
//! game's logical units (360×640 upright, 640×360 sideways), as `mb shot`:
//!
//!     [{ "at": 1.0, "tap": [180, 500] },
//!      { "at": 1.5, "hold": [180, 500], "for": 0.8 },
//!      { "at": 2.5, "swipe": [100, 500, 260, 500], "for": 0.2 },
//!      { "at": 3.0, "key": "space", "for": 0.1 },
//!      { "at": 3.5, "pad": "a" },  { "at": 4, "pad": "lx", "value": -1, "for": 0.5 }]
//!
//! The debug file is `{ "scenario": "Gold rush" }` or
//! `{ "picks": ["Invincible", "Score=38", "cat · speed=90"] }`. Unless
//! `--no-start`, the run presses Enter (or taps, without a keyboard) at the
//! third frame, which starts the round as a player would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::generate::img::Image;

#[derive(clap::Args)]
pub struct PlayArgs {
    /// The game: a Studio bundle (.mbx) or project directory (built first).
    pub game: PathBuf,
    /// Print one line of JSON: { ok, errors, log, events, score, frames, summary }.
    #[arg(long)]
    pub json: bool,
    /// Where the frames' PNGs go (default: the game's screenshots/, or ./screenshots).
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// How long to play, in seconds of game time.
    #[arg(long, default_value_t = 5.0)]
    pub seconds: f64,
    /// How many frames to draw, evenly through the run (the last at its end).
    #[arg(long, default_value_t = 4)]
    pub frames: usize,
    /// Draw frames at these times instead (seconds, comma-separated).
    #[arg(long, value_delimiter = ',', value_name = "SECONDS")]
    pub at: Vec<f64>,
    /// Scripted input: a JSON file (see `mb play --help`).
    #[arg(long, value_name = "FILE")]
    pub script: Option<PathBuf>,
    /// How the phone is held: portrait (a sideways game's feed card) or sideways.
    #[arg(long)]
    pub orientation: Option<String>,
    /// Debug-menu picks: a JSON file, { "scenario": name } or { "picks": [label, "label=value", …] }.
    #[arg(long, value_name = "FILE")]
    pub debug: Option<PathBuf>,
    /// The session's seed.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Don't press Enter to start the round (play the title screen as it is).
    #[arg(long)]
    pub no_start: bool,
    /// A built runtime (runtime/dist) instead of the one in this mb (default: $MB_RUNTIME_DIR, if set).
    #[arg(long)]
    pub runtime: Option<PathBuf>,
    /// Stop a run that takes longer than this many seconds.
    #[arg(long, default_value_t = 90)]
    pub timeout: u64,
}

const HZ: f64 = 60.0;
/// What each frame is drawn at: art pixels to PNG pixels.
const ZOOM: u32 = 2;

fn key_code(name: &str) -> Option<u32> {
    use maimbrain_keys::*;
    Some(match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => ENTER,
        "space" => SPACE,
        "up" => UP,
        "down" => DOWN,
        "left" => LEFT,
        "right" => RIGHT,
        "w" => 26,
        "a" => 4,
        "s" => 22,
        "d" => 7,
        "z" => 29,
        "x" => 27,
        "j" => 13,
        "k" => 14,
        n => return n.parse().ok(),
    })
}

/// Key and pad codes (SPEC §5.2, the SDK's `input::key` and `input::pad`).
mod maimbrain_keys {
    pub const ENTER: u32 = 40;
    pub const SPACE: u32 = 44;
    pub const RIGHT: u32 = 79;
    pub const LEFT: u32 = 80;
    pub const DOWN: u32 = 81;
    pub const UP: u32 = 82;
}

/// A pad button (kind 11) or axis (kind 12) by name.
fn pad_code(name: &str) -> Option<(u8, u32)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a" => (11, 0),
        "b" => (11, 1),
        "x" => (11, 2),
        "y" => (11, 3),
        "lb" => (11, 4),
        "rb" => (11, 5),
        "up" => (11, 12),
        "down" => (11, 13),
        "left" => (11, 14),
        "right" => (11, 15),
        "lx" => (12, 0),
        "ly" => (12, 1),
        n => (11, n.parse().ok()?),
    })
}

/// A raw event as the replay log holds it: [kind, id, pad, x, y, a, b, code].
fn ev(kind: u8, id: u8, x: f64, y: f64, a: f64, code: u32) -> Value {
    json!([kind, id, 0, x, y, a, 0, code])
}

/// The log's frames' events from the script (and the start press).
fn script_events(script: &Value, frames: usize, start: Option<(bool, f64, f64)>) -> Result<Vec<Vec<Value>>, String> {
    let mut out = vec![Vec::new(); frames];
    let mut put = |f: usize, e: Value| {
        if f < frames {
            out[f].push(e);
        }
    };
    let frame = |t: f64| (t * HZ).round().max(0.0) as usize;
    if let Some((keyboard, cx, cy)) = start {
        if keyboard {
            put(3, ev(5, 0, 0.0, 0.0, 0.0, maimbrain_keys::ENTER));
            put(5, ev(6, 0, 0.0, 0.0, 0.0, maimbrain_keys::ENTER));
        } else {
            put(3, ev(1, 0, cx, cy, 1.0, 0));
            put(5, ev(3, 0, cx, cy, 0.0, 0));
        }
    }
    let list = match script {
        Value::Null => return Ok(out),
        Value::Array(l) => l,
        _ => return Err("the script is a JSON list of { at, tap | hold | release | swipe | key | pad }".into()),
    };
    for (i, s) in list.iter().enumerate() {
        let what = || format!("script item {}", i + 1);
        let at = s["at"].as_f64().ok_or_else(|| format!("{}: no \"at\" (seconds)", what()))?;
        let f = frame(at);
        let dur = |d: f64| frame(s["for"].as_f64().unwrap_or(d)).max(1);
        let xy = |k: &str| -> Result<Vec<f64>, String> {
            // Two numbers at least (a swipe checks for its four).
            let p: Vec<f64> = s[k].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            if p.len() < 2 {
                let shape = if k == "swipe" { "[x1, y1, x2, y2]" } else { "[x, y]" };
                return Err(format!("{}: \"{k}\" is {shape}", what()));
            }
            Ok(p)
        };
        if s.get("tap").is_some() {
            let p = xy("tap")?;
            put(f, ev(1, 0, p[0], p[1], 1.0, 0));
            put(f + 4, ev(3, 0, p[0], p[1], 0.0, 0));
        } else if s.get("hold").is_some() {
            let p = xy("hold")?;
            put(f, ev(1, 0, p[0], p[1], 1.0, 0));
            put(f + dur(0.5), ev(3, 0, p[0], p[1], 0.0, 0));
        } else if s.get("release").is_some() {
            let p = xy("release")?;
            put(f, ev(3, 0, p[0], p[1], 0.0, 0));
        } else if s.get("swipe").is_some() {
            let p = xy("swipe")?;
            if p.len() != 4 {
                return Err(format!("{}: \"swipe\" is [x1, y1, x2, y2]", what()));
            }
            let n = dur(0.15);
            put(f, ev(1, 0, p[0], p[1], 1.0, 0));
            for k in 1..=n {
                let t = k as f64 / n as f64;
                put(f + k, ev(2, 0, p[0] + (p[2] - p[0]) * t, p[1] + (p[3] - p[1]) * t, 1.0, 0));
            }
            put(f + n + 1, ev(3, 0, p[2], p[3], 0.0, 0));
        } else if let Some(k) = s.get("key") {
            let code = k.as_str().and_then(key_code).or_else(|| k.as_u64().map(|c| c as u32)).ok_or_else(|| format!("{}: unknown key {k}", what()))?;
            put(f, ev(5, 0, 0.0, 0.0, 0.0, code));
            put(f + dur(0.1), ev(6, 0, 0.0, 0.0, 0.0, code));
        } else if let Some(p) = s.get("pad") {
            let (kind, code) = p.as_str().and_then(pad_code).ok_or_else(|| format!("{}: unknown pad button or axis {p}", what()))?;
            if kind == 12 {
                put(f, ev(12, 1, 0.0, 0.0, s["value"].as_f64().unwrap_or(1.0), code));
                put(f + dur(0.3), ev(12, 1, 0.0, 0.0, 0.0, code));
            } else {
                put(f, ev(11, 1, 0.0, 0.0, 1.0, code));
                put(f + dur(0.1), ev(11, 1, 0.0, 0.0, 0.0, code));
            }
        } else {
            return Err(format!("{}: needs one of tap, hold, release, swipe, key, pad", what()));
        }
    }
    Ok(out)
}

/// `--debug`'s file as the engine's `studio_debug` parameter.
fn debug_param(v: &Value) -> Result<String, String> {
    let mut parts = Vec::new();
    if let Some(s) = v["scenario"].as_str() {
        parts.push(s.to_string());
    }
    if let Some(list) = v["picks"].as_array() {
        for p in list {
            match p {
                Value::String(s) => parts.push(s.clone()),
                Value::Object(o) => {
                    let label = o.get("label").and_then(Value::as_str).ok_or("a pick is \"label\", \"label=value\" or { label, value }")?;
                    match o.get("value") {
                        Some(Value::Bool(b)) => parts.push(format!("{label}={}", *b as u8)),
                        Some(v) if !v.is_null() => parts.push(format!("{label}={}", v.as_str().map_or_else(|| v.to_string(), str::to_string))),
                        _ => parts.push(label.to_string()),
                    }
                }
                _ => return Err("a pick is \"label\", \"label=value\" or { label, value }".into()),
            }
        }
    }
    if parts.is_empty() {
        return Err("the debug file is { \"scenario\": name } or { \"picks\": [ … ] }".into());
    }
    Ok(parts.join(";"))
}

fn read_json(p: &Path) -> Result<Value, String> {
    let t = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&t).map_err(|e| format!("{}: not JSON: {e}", p.display()))
}

/// `name.lua:12: message` → (game/name.lua, 12).
fn lua_position(msg: &str) -> Option<(String, u64)> {
    let i = msg.find(".lua:")?;
    let start = msg[..i].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).map_or(0, |j| j + 1);
    let name = &msg[start..i];
    let line: String = msg[i + 5..].chars().take_while(char::is_ascii_digit).collect();
    if name.is_empty() || name == "prelude" {
        return None;
    }
    Some((format!("game/{name}.lua"), line.parse().ok()?))
}

pub fn run(a: PlayArgs) -> Result<(), String> {
    // A project directory is built first (to a temporary bundle).
    let work = std::env::temp_dir().join(format!("mb-play-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = play(&a, &work);
    let _ = std::fs::remove_dir_all(&work);
    let report = result?;
    if a.json {
        println!("{report}");
    } else {
        print!("{}", human(&report));
    }
    Ok(())
}

fn human(r: &Value) -> String {
    let mut s = format!("{}\n", r["summary"].as_str().unwrap_or(""));
    for e in r["errors"].as_array().into_iter().flatten() {
        s += &format!("  error: {}\n", e["message"].as_str().unwrap_or(""));
    }
    for e in r["events"].as_array().into_iter().flatten().take(30) {
        s += &format!("  {e}\n");
    }
    for f in r["frames"].as_array().into_iter().flatten() {
        s += &format!("  frame {:.2} s: {}\n", f["at"].as_f64().unwrap_or(0.0), f["file"].as_str().unwrap_or(""));
    }
    s
}

fn play(a: &PlayArgs, work: &Path) -> Result<Value, String> {
    let bundle = if a.game.is_dir() {
        if !super::is_project(&a.game) {
            return Err(format!("{}: not a Studio project (no studio.toml) or bundle", a.game.display()));
        }
        super::build_and_pack(&a.game, &work.join("built"))?
    } else {
        a.game.clone()
    };
    let bytes = std::fs::read(&bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let b = mb_format::Bundle::open(&bytes).map_err(|r| format!("{} is invalid: {}", bundle.display(), r.errors.join("; ")))?;
    let m = &b.manifest;
    if m.engine.is_none() {
        return Err(format!("{} isn't a Studio game (mb play plays Studio games; mb shot and mb measure play any game)", bundle.display()));
    }
    let listed = b.file(mb_format::studio::INDEX).map(|f| String::from_utf8_lossy(f).lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).count()).unwrap_or(0);
    let total = ((a.seconds.clamp(0.1, 120.0)) * HZ).round() as usize + 1;
    let mut shots: Vec<usize> = if a.at.is_empty() {
        let k = a.frames.clamp(0, 30);
        (1..=k).map(|i| (total - 1) * i / k).collect()
    } else {
        a.at.iter().map(|t| ((t * HZ).round() as usize).min(total - 1)).collect()
    };
    shots.sort();
    shots.dedup();
    // The phone's screen: the game's own shape, or as `--orientation` holds it.
    let [lw, lh] = m.logical_size;
    let (w, h) = match a.orientation.as_deref() {
        None => (lw, lh),
        Some("portrait") => (lw.min(lh), lw.max(lh)),
        Some("sideways" | "landscape") => (lw.max(lh), lw.min(lh)),
        Some(o) => return Err(format!("--orientation {o}: portrait or sideways")),
    };
    let script = match &a.script {
        Some(p) => read_json(p)?,
        None => Value::Null,
    };
    let keyboard = m.inputs.contains(&mb_format::manifest::Input::Keyboard);
    let start = (!a.no_start).then_some((keyboard, w as f64 / 2.0, h as f64 * 0.6));
    let events = script_events(&script, total, start)?;
    let mut params = serde_json::Map::new();
    // The engine counts updates from 1: log frame f is its frame f + 1.
    params.insert("studio_dump".into(), shots.iter().map(|f| (f + 1).to_string()).collect::<Vec<_>>().join(",").into());
    params.insert("studio_trace".into(), "1".into());
    let debug = match &a.debug {
        Some(p) => Some(debug_param(&read_json(p)?)?),
        None => None,
    };
    if let Some(d) = &debug {
        params.insert("studio_debug".into(), d.clone().into());
    }
    let dt = (1.0f32 / 60.0) as f64;
    let frames: Vec<Value> = (0..total)
        .map(|f| {
            let mut o = serde_json::Map::new();
            o.insert("dt".into(), dt.into());
            // The engine asks for files.txt in mb_init and for what it lists in
            // its first update; each is ready at the next frame, as on a fast phone.
            match f {
                0 => {
                    o.insert("assets".into(), json!([1]));
                }
                1 if listed > 0 => {
                    o.insert("assets".into(), json!((2..listed + 2).collect::<Vec<_>>()));
                }
                _ => {}
            }
            if !events[f].is_empty() {
                o.insert("events".into(), Value::Array(events[f].clone()));
            }
            Value::Object(o)
        })
        .collect();
    let mut log = json!({
        "version": 1,
        "game": m.id,
        "bundle": b.sha256,
        "seed": a.seed.to_string(),
        "dailySeed": mb_format::daily_seed(&m.id, "2026-01-01").to_string(),
        "store": {},
        "playerId": "00112233445566778899aabbccddeeff",
        "locale": "en-US",
        "screen": [w, h, 0, 0, 0, 0, 2, 0],
        "params": params,
        "frames": frames,
    });
    if debug.is_some() {
        log["debug"] = true.into();
    }
    let runtime = a.runtime.clone().or_else(|| std::env::var_os("MB_RUNTIME_DIR").map(PathBuf::from));
    let opts = crate::replay::Options { json: true, runtime, timeout: std::time::Duration::from_secs(a.timeout) };
    let (result, stderr) = crate::replay::run(&bundle, &log, &work.join("run"), &opts)?;

    // What the game logged: errors, events, frame descriptions, the rest.
    let mut errors = Vec::new();
    let mut lines = Vec::new();
    let mut evs = Vec::new();
    let mut dumps: Vec<Value> = Vec::new();
    for l in stderr.lines() {
        let Some(msg) = l.strip_prefix("[game] ") else { continue };
        if let Some(j) = msg.strip_prefix("studio-frame ") {
            if let Ok(v) = serde_json::from_str::<Value>(j) {
                dumps.push(v);
            }
        } else if let Some(e) = msg.strip_prefix("studio-event ") {
            let w: Vec<&str> = e.split_whitespace().collect();
            if let [t, name, rest @ ..] = &w[..] {
                let word = |i: usize| rest.get(i).filter(|w| **w != "-").map(|w| Value::from(*w)).unwrap_or(Value::Null);
                let num = |i: usize| rest.get(i).and_then(|w| w.parse::<f64>().ok()).map_or(Value::Null, Value::from);
                evs.push(json!({ "at": t.parse::<f64>().unwrap_or(0.0), "event": name, "thing": word(0), "other": word(1), "x": num(2), "y": num(3) }));
            }
        } else {
            if let Some(m) = msg.strip_prefix("studio: ").filter(|m| !errors.iter().any(|e: &Value| e["message"] == *m)) {
                let (path, line) = lua_position(m).map_or((Value::Null, Value::Null), |(p, l)| (p.into(), l.into()));
                errors.push(json!({ "message": m, "path": path, "line": line }));
            }
            if lines.len() < 200 {
                lines.push(msg.to_string());
            }
        }
    }
    if let Some(e) = result["error"].as_str() {
        errors.push(json!({ "message": format!("the game stopped: {e}"), "path": null, "line": null }));
    }
    for r in result["rounds"].as_array().into_iter().flatten() {
        let state = match r["state"].as_u64() {
            Some(1) => "playing",
            Some(2) => "over",
            _ => "idle",
        };
        evs.push(json!({ "at": r["frame"].as_f64().unwrap_or(0.0) / HZ, "round": state }));
    }
    for s in result["scores"].as_array().into_iter().flatten() {
        evs.push(json!({ "at": s["frame"].as_f64().unwrap_or(0.0) / HZ, "score": s["value"].as_str().and_then(|v| v.parse::<i64>().ok()) }));
    }

    evs.sort_by(|a, b| a["at"].as_f64().unwrap_or(0.0).total_cmp(&b["at"].as_f64().unwrap_or(0.0)));

    // The frames, drawn.
    let out = a.out.clone().unwrap_or_else(|| if a.game.is_dir() { a.game.join("screenshots") } else { PathBuf::from("screenshots") });
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut images = Images::default();
    let mut shots_out = Vec::new();
    for d in &dumps {
        let frame = d["frame"].as_u64().unwrap_or(1).saturating_sub(1) as f64;
        let at = frame / HZ;
        let png = draw(d, &b, &mut images)?;
        let name = format!("play-{at:.2}s.png");
        std::fs::write(out.join(&name), png).map_err(|e| format!("{}: {e}", out.join(&name).display()))?;
        shots_out.push(json!({ "at": at, "file": name }));
    }
    let score = dumps.iter().rev().find_map(|d| d["score"].as_f64()).map(|s| s.round() as i64).or_else(|| {
        result["scores"].as_array().and_then(|s| s.last()).and_then(|s| s["value"].as_str()?.parse().ok())
    });
    let summary = summary(dumps.last(), &errors, total);
    Ok(json!({
        "ok": errors.is_empty(),
        "errors": errors,
        "log": lines,
        "events": evs,
        "score": score,
        "frames": shots_out,
        "summary": summary,
        "seconds": (total - 1) as f64 / HZ,
        "hash": result["hash"],
    }))
}

/// What's on screen at the end, in words.
fn summary(last: Option<&Value>, errors: &[Value], total: usize) -> String {
    let secs = (total - 1) as f64 / HZ;
    let Some(d) = last else {
        return format!("After {secs:.1} s: nothing to describe (no frames were asked for)");
    };
    let mode = match d["mode"].as_str().unwrap_or("") {
        "play" => "playing",
        "title" => "on the title card",
        "over" => "on the game-over card",
        "card" => "an upright feed card",
        "loading" => "still loading",
        m => m,
    };
    let mut s = format!("After {:.1} s: {mode}", d["time"].as_f64().unwrap_or(secs));
    if let Some(l) = d["level"].as_str() {
        s += &format!(", level {l}");
    }
    if let Some(sc) = d["score"].as_f64() {
        s += &format!(", score {}", sc.round());
    }
    if let Some(l) = d["lives"].as_f64() {
        s += &format!(", lives {}", l.round());
    }
    let mut kinds: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for t in d["things"].as_array().into_iter().flatten() {
        if let (Some(k), Some(x), Some(y)) = (t[0].as_str(), t[1].as_f64(), t[2].as_f64()) {
            kinds.entry(k.to_string()).or_default().push((x, y));
        }
    }
    if !kinds.is_empty() {
        let parts: Vec<String> = kinds
            .iter()
            .map(|(k, ps)| match &ps[..] {
                [(x, y)] => format!("{k} at ({x:.0}, {y:.0})"),
                _ => {
                    let (x0, x1) = ps.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
                    let (y0, y1) = ps.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
                    format!("{k} ×{} (x {x0:.0}–{x1:.0}, y {y0:.0}–{y1:.0})", ps.len())
                }
            })
            .collect();
        s += &format!(". On screen (art pixels, {}×{}): {}", d["w"], d["h"], parts.join("; "));
    }
    let texts: Vec<&str> = d["texts"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !texts.is_empty() {
        s += &format!(". Text: {}", texts.join(" · "));
    }
    if let Some(e) = d["error"].as_str() {
        s += &format!(". Showing an error: {e}");
    } else if !errors.is_empty() {
        s += &format!(". {} error(s)", errors.len());
    }
    s.push('.');
    s
}

/// The bundle's images, decoded once.
#[derive(Default)]
struct Images(BTreeMap<String, Option<Image>>);

impl Images {
    fn get(&mut self, b: &mb_format::Bundle, path: &str) -> Option<&Image> {
        self.0.entry(path.to_string()).or_insert_with(|| b.file(path).and_then(|bytes| Image::decode(bytes).ok())).as_ref()
    }
}

fn rgba(c: u64) -> [u8; 4] {
    [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8]
}

fn blend(img: &mut Image, x: i64, y: i64, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= img.w as i64 || y >= img.h as i64 || c[3] == 0 {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    if c[3] == 255 {
        img.set(x, y, c);
        return;
    }
    let d = img.get(x, y);
    let a = c[3] as u32;
    let mix = |s: u8, t: u8| ((s as u32 * a + t as u32 * (255 - a)) / 255) as u8;
    img.set(x, y, [mix(c[0], d[0]), mix(c[1], d[1]), mix(c[2], d[2]), 255]);
}

/// One frame from the engine's description: its draw list at ZOOM pixels an
/// art pixel, then the score (and the mode, off a round) in a pixel font.
fn draw(d: &Value, b: &mb_format::Bundle, images: &mut Images) -> Result<Vec<u8>, String> {
    let (w, h) = (d["w"].as_u64().unwrap_or(144) as u32, d["h"].as_u64().unwrap_or(256) as u32);
    let z = ZOOM as f64;
    let mut img = Image::new(w * ZOOM, h * ZOOM, [0, 0, 0, 255]);
    for c in d["draw"].as_array().into_iter().flatten() {
        let n = |i: usize| c[i].as_f64().unwrap_or(0.0);
        match c[0].as_str() {
            Some("rect") => {
                let col = rgba(c[5].as_u64().unwrap_or(0));
                let (x0, y0) = ((n(1) * z).round() as i64, (n(2) * z).round() as i64);
                let (x1, y1) = (((n(1) + n(3)) * z).round() as i64, ((n(2) + n(4)) * z).round() as i64);
                for y in y0.max(0)..y1.min(img.h as i64) {
                    for x in x0.max(0)..x1.min(img.w as i64) {
                        blend(&mut img, x, y, col);
                    }
                }
            }
            Some("circle") => {
                let col = rgba(c[4].as_u64().unwrap_or(0));
                let (cx, cy, r) = (n(1) * z, n(2) * z, n(3) * z);
                for y in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
                    for x in (cx - r).floor() as i64..=(cx + r).ceil() as i64 {
                        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                        if dx * dx + dy * dy <= r * r {
                            blend(&mut img, x, y, col);
                        }
                    }
                }
            }
            Some("img") => {
                let Some(src) = c[1].as_str().and_then(|p| images.get(b, p)).cloned() else { continue };
                let (sx, sy, sw, sh) = (n(2), n(3), n(4), n(5));
                let (dx, dy, dw, dh) = (n(6) * z, n(7) * z, n(8) * z, n(9) * z);
                let flip = n(10) != 0.0;
                for y in 0..dh.round() as i64 {
                    for x in 0..dw.round() as i64 {
                        let u = (x as f64 + 0.5) / dw;
                        let u = if flip { 1.0 - u } else { u };
                        let v = (y as f64 + 0.5) / dh;
                        let (px, py) = ((sx + u * sw).floor() as i64, (sy + v * sh).floor() as i64);
                        if px < 0 || py < 0 || px >= src.w as i64 || py >= src.h as i64 {
                            continue;
                        }
                        blend(&mut img, dx.round() as i64 + x, dy.round() as i64 + y, src.get(px as u32, py as u32));
                    }
                }
            }
            _ => {}
        }
    }
    let (iw, ih) = (img.w as i64, img.h as i64);
    let mode = d["mode"].as_str().unwrap_or("");
    if let Some(s) = d["score"].as_f64().filter(|_| mode == "play") {
        text(&mut img, &format!("{}", s.round()), iw / 2, 8, 3, true);
    }
    let banner = match mode {
        "title" => Some("TITLE"),
        "over" => Some("GAME OVER"),
        "card" => Some("FEED CARD"),
        "loading" => Some("LOADING"),
        _ => None,
    };
    if let Some(t) = banner {
        let y = img.h as i64 / 4;
        for yy in y - 6..y + 22 {
            for x in 0..img.w as i64 {
                blend(&mut img, x, yy, [0, 0, 0, 150]);
            }
        }
        text(&mut img, t, iw / 2, y, 3, true);
    }
    if let Some(e) = d["error"].as_str() {
        let short: String = e.chars().take(36).collect::<String>().to_uppercase();
        text(&mut img, &short, iw / 2, ih - 24, 1, true);
    }
    Ok(img.encode_png())
}

/// A 3×5 pixel font: digits, capitals and a little punctuation.
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 3, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'A' => [2, 5, 7, 5, 5],
        'B' => [6, 5, 6, 5, 6],
        'C' => [3, 4, 4, 4, 3],
        'D' => [6, 5, 5, 5, 6],
        'E' => [7, 4, 6, 4, 7],
        'F' => [7, 4, 6, 4, 4],
        'G' => [3, 4, 5, 5, 3],
        'H' => [5, 5, 7, 5, 5],
        'I' => [7, 2, 2, 2, 7],
        'J' => [1, 1, 1, 5, 2],
        'K' => [5, 5, 6, 5, 5],
        'L' => [4, 4, 4, 4, 7],
        'M' => [5, 7, 7, 5, 5],
        'N' => [6, 5, 5, 5, 5],
        'O' => [2, 5, 5, 5, 2],
        'P' => [6, 5, 6, 4, 4],
        'Q' => [2, 5, 5, 6, 3],
        'R' => [6, 5, 6, 5, 5],
        'S' => [3, 4, 2, 1, 6],
        'T' => [7, 2, 2, 2, 2],
        'U' => [5, 5, 5, 5, 7],
        'V' => [5, 5, 5, 5, 2],
        'W' => [5, 5, 7, 7, 5],
        'X' => [5, 5, 2, 5, 5],
        'Y' => [5, 5, 2, 2, 2],
        'Z' => [7, 1, 2, 4, 7],
        '-' => [0, 0, 7, 0, 0],
        '.' => [0, 0, 0, 0, 2],
        ':' => [0, 2, 0, 2, 0],
        '!' => [2, 2, 2, 0, 2],
        '\'' => [2, 2, 0, 0, 0],
        _ => [0, 0, 0, 0, 0],
    }
}

/// Text in the 3×5 font, `k` pixels a font pixel, white with a dark outline.
fn text(img: &mut Image, s: &str, cx: i64, y: i64, k: i64, centred: bool) {
    let width = s.chars().count() as i64 * 4 * k - k;
    let x0 = if centred { cx - width / 2 } else { cx };
    for pass in 0..2 {
        for (i, c) in s.chars().enumerate() {
            let g = glyph(c.to_ascii_uppercase());
            for (row, bits) in g.iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) == 0 {
                        continue;
                    }
                    let (px, py) = (x0 + (i as i64 * 4 + col) * k, y + row as i64 * k);
                    for yy in 0..k {
                        for xx in 0..k {
                            if pass == 0 {
                                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                                    blend(img, px + xx + ox, py + yy + oy, [20, 16, 40, 255]);
                                }
                            } else {
                                blend(img, px + xx, py + yy, [255, 255, 255, 255]);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_become_log_events() {
        let s = json!([
            { "at": 0.5, "tap": [180, 500] },
            { "at": 1.0, "swipe": [100, 500, 200, 500], "for": 0.05 },
            { "at": 1.5, "key": "space" },
            { "at": 2.0, "pad": "lx", "value": -1, "for": 0.1 },
        ]);
        let e = script_events(&s, 200, Some((true, 0.0, 0.0))).unwrap();
        assert_eq!(e[3], vec![json!([5, 0, 0, 0.0, 0.0, 0.0, 0, 40])], "Enter starts the round");
        assert_eq!(e[30], vec![json!([1, 0, 0, 180.0, 500.0, 1.0, 0, 0])]);
        assert_eq!(e[34], vec![json!([3, 0, 0, 180.0, 500.0, 0.0, 0, 0])]);
        assert_eq!(e[60][0][0], 1);
        assert_eq!(e[63][0][0], 2, "the swipe moves");
        assert_eq!(e[90], vec![json!([5, 0, 0, 0.0, 0.0, 0.0, 0, 44])]);
        assert_eq!(e[120], vec![json!([12, 1, 0, 0.0, 0.0, -1.0, 0, 0])]);
        assert!(script_events(&json!([{ "at": 1, "kick": 1 }]), 10, None).unwrap_err().contains("needs one of"));
        assert!(script_events(&json!([{ "at": 1, "key": "banana" }]), 10, None).unwrap_err().contains("unknown key"));
        let swipe = script_events(&json!([{ "at": 1, "swipe": "left" }]), 10, None).unwrap_err();
        assert!(swipe.contains("[x1, y1, x2, y2]"), "{swipe}");
        for bad in [json!([{ "at": 1, "tap": [5] }]), json!([{ "at": 1, "hold": [] }]), json!([{ "at": 1, "release": "here" }])] {
            assert!(script_events(&bad, 10, None).unwrap_err().contains("is [x, y]"), "{bad}");
        }
    }

    #[test]
    fn debug_files_become_picks_by_label() {
        assert_eq!(debug_param(&json!({ "scenario": "Gold rush" })).unwrap(), "Gold rush");
        assert_eq!(debug_param(&json!({ "picks": ["Invincible", { "label": "Score", "value": 38 }, { "label": "Show hitboxes", "value": true }] })).unwrap(), "Invincible;Score=38;Show hitboxes=1");
        assert!(debug_param(&json!({})).is_err());
    }

    #[test]
    fn lua_errors_name_their_file_and_line() {
        assert_eq!(lua_position("main.lua:12: attempt to index a nil value"), Some(("game/main.lua".into(), 12)));
        assert_eq!(lua_position("enemies.lua:3: boom"), Some(("game/enemies.lua".into(), 3)));
        assert_eq!(lua_position("maps/a.map:4: 'X' isn't in the legend"), None);
    }
}
