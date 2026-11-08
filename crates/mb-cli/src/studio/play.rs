//! `mb play`: a Studio game played headless, for Studio's `play_test` and
//! `screenshot` (the server's `Studio::Builder`), and for anyone at a terminal.
//!
//! The run is the platform's own: the bundle in the headless runner `mb
//! replay` uses (Deno, the phone's runtime and mb-host), fed a replay
//! log written here from a script of touches, keys, pad buttons and debug
//! picks. The engine reads three debug parameters (games/studio-engine and
//! games/studio-engine2 `headless.rs`): `studio_dump` makes it describe
//! what's on screen at the frames asked for (the summary, the score),
//! `studio_trace` logs its events, `studio_debug` makes debug picks by label.
//!
//! Every frame is the GPU's, 2D and 3D, engine 1.0 and 2.0 alike
//! (docs/STUDIO-2.md §4.11): the runner draws it as the phone does, with
//! mb-host on a WebGPU device with no canvas (Mesa's lavapipe on the server),
//! at `--scale` times the size of the screen it plays on (2, as a phone draws
//! it, or 1 for `write_files`' automatic play-test), turned when held
//! sideways (`replay::Capture`).
//!
//! The bundle is opened once, its published engine's metered wasm kept
//! between runs (`crate::engines`).
//!
//!     mb play game.mbx --json --out shots --seconds 5 --frames 4 [--scale 1|2]
//!         [--at 1,2.5] [--script input.json] [--orientation portrait|sideways]
//!         [--debug picks.json] [--seed N] [--no-start] [--check grounding,readability]
//!
//! The report's `budgets` are what the frames described used against
//! docs/STUDIO-2.md §4.6's budgets (2D nodes, `love.draw` commands, and Lua's
//! interrupts a frame where the engine counts them), and what they drew in 3D
//! (nodes, draws, triangles, skinned models, particles), at their peak.
//! `--check grounding` adds `checks.grounding`: lines on figures that float
//! above their floor, are cut off or stand on nothing, in the last frame
//! drawn, from what the engine reports of its footed figures (`ground` in
//! its frame description, the SDK's `ground:` records; grounding.rs).
//! `--check readability` adds `checks.readability`: whether the game's
//! subject (`round.subject`, else its player) stands out from the scene in
//! each frame of play, and the worst frame's picture in greyscale with the
//! subject outlined (`<frame>.readability.png`; readable.rs).
//! `--check smoothness` adds `checks.smoothness`: the run played again at
//! 120 Hz, and whether the camera, the subject and the things on the view
//! move smoothly at both rates (smoothness.rs).
//!
//! The script is JSON, times in seconds from the start and positions in the
//! game's logical units (360×640 upright, 640×360 sideways), as `mb shot`:
//!
//!     [{ "at": 1.0, "tap": [180, 500] },
//!      { "at": 1.5, "hold": [180, 500], "for": 0.8 },
//!      { "at": 2.5, "swipe": [100, 500, 260, 500], "for": 0.2 },
//!      { "at": 3.0, "key": "space", "for": 0.1 },
//!      { "at": 3.5, "pad": "a" },  { "at": 4, "pad": "lx", "value": -1, "for": 0.5 },
//!      { "at": 1, "tilt": [-20, 10], "for": 0.4 }, { "at": 2, "aim": [15, -5] },
//!      { "at": 3, "shake": 2, "for": 0.5 }, { "at": 4, "loud": 0.85, "for": 1 }]
//!
//! The sensor items (a game that declares the sensor: sense.rs) tilt the
//! phone (roll and pitch from upright, degrees), point it (yaw and pitch),
//! shake it (g) and blow into the mic (0–1).
//!
//! The debug file is `{ "scenario": "Gold rush" }` or
//! `{ "picks": ["Invincible", "Score=38", "cat · speed=90"] }`. Unless
//! `--no-start`, the run presses Enter (or taps, without a keyboard) at the
//! third frame, which starts the round as a player would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

#[derive(clap::Args)]
pub struct PlayArgs {
    /// The game: a Studio bundle (.mbx) or project directory (built first).
    pub game: PathBuf,
    /// Print one line of JSON: { ok, errors, log, events, score, frames, summary, progress }.
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
    /// Frame pixels a logical unit: 2 as a phone draws them, 1 for a smaller
    /// picture (`write_files`' automatic play-test, docs/STUDIO-2.md §4.11).
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=2))]
    pub scale: u32,
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
    /// Checks on the frames: grounding (figures standing on their floors),
    /// readability (the subject stands out from the scene), smoothness (the
    /// camera and what's on the view move smoothly, the run again at 120 Hz).
    #[arg(long, value_delimiter = ',', value_name = "CHECK")]
    pub check: Vec<String>,
    /// A built runtime (runtime/dist) instead of the one in this mb (default: $MB_RUNTIME_DIR, if set).
    #[arg(long)]
    pub runtime: Option<PathBuf>,
    /// Stop a run that takes longer than this many seconds.
    #[arg(long, default_value_t = 90)]
    pub timeout: u64,
    /// Saved data the session starts with: a JSON object of key → base64, as a
    /// replay log's `store` has it (progress an earlier run left, a checkpoint).
    #[arg(long, value_name = "FILE")]
    pub store: Option<PathBuf>,
    /// The UTC date whose daily seed the session gets (YYYY-MM-DD).
    #[arg(long, default_value = "2026-01-01")]
    pub date: String,
    /// Also write the run as a replay log, as the phone records one (what `mb
    /// replay` and the server's run checks take).
    #[arg(long, value_name = "FILE")]
    pub save_run: Option<PathBuf>,
}

const HZ: f64 = 60.0;

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

/// The log's frames' events from the script (and the start press), at `hz`
/// frames a second.
fn script_events(script: &Value, frames: usize, start: Option<(bool, f64, f64)>, hz: f64) -> Result<Vec<Vec<Value>>, String> {
    let mut out = vec![Vec::new(); frames];
    let mut put = |f: usize, e: Value| {
        if f < frames {
            out[f].push(e);
        }
    };
    let frame = |t: f64| (t * hz).round().max(0.0) as usize;
    // The press at 0.05 s, let go at 0.083 s (frames 3 and 5 at 60 Hz).
    let (press, lift) = (frame(3.0 / HZ), frame(5.0 / HZ));
    if let Some((keyboard, cx, cy)) = start {
        if keyboard {
            put(press, ev(5, 0, 0.0, 0.0, 0.0, maimbrain_keys::ENTER));
            put(lift, ev(6, 0, 0.0, 0.0, 0.0, maimbrain_keys::ENTER));
        } else {
            put(press, ev(1, 0, cx, cy, 1.0, 0));
            put(lift, ev(3, 0, cx, cy, 0.0, 0));
        }
    }
    let list = match script {
        Value::Null => return Ok(out),
        Value::Array(l) => l,
        _ => return Err("the script is a JSON list of { at, tap | hold | release | swipe | key | pad | tilt | aim | shake | loud }".into()),
    };
    for (i, s) in list.iter().enumerate().filter(|(_, s)| !super::sense::is_sensor(s)) {
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
            put(f + frame(4.0 / HZ), ev(3, 0, p[0], p[1], 0.0, 0));
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
            return Err(format!("{}: needs one of tap, hold, release, swipe, key, pad, tilt, aim, shake, loud", what()));
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

/// `name.lua:12: message` → (game/name.lua, 12); engine 2.0 names the
/// chunk by its path already (`game/enemies/boss.lua:12: …`).
fn lua_position(msg: &str) -> Option<(String, u64)> {
    let i = msg.find(".lua:")?;
    if let Some(start) = msg[..i].rfind("game/").filter(|s| msg[*s..i].bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'/')))
        && msg[..start].bytes().last().is_none_or(|c| !c.is_ascii_alphanumeric())
    {
        let line: String = msg[i + 5..].chars().take_while(char::is_ascii_digit).collect();
        return Some((msg[start..i + 4].to_string(), line.parse().ok()?));
    }
    let start = msg[..i].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).map_or(0, |j| j + 1);
    let name = &msg[start..i];
    let line: String = msg[i + 5..].chars().take_while(char::is_ascii_digit).collect();
    if name.is_empty() || name == "prelude" {
        return None;
    }
    Some((format!("game/{name}.lua"), line.parse().ok()?))
}

/// The asset handles that arrive at log frame `f` (published at its start),
/// as on a fast phone: the engine asks for files.txt (handle 1) in mb_init,
/// for the `listed` files it names in its first update, and then, in some
/// later update, for each of the `fonts` the packer baked as a font of its
/// own (the next handles). Each is ready at the frame after it's asked for.
/// The runner publishes only the handles a frame lists and skips those not
/// asked for yet, so the fonts' handles are listed at every frame from the
/// third on; without them a library font never loaded, and every identity
/// and `newFont` drew in the host's sans (docs/STUDIO-2.md, as built in 1g).
fn arrivals(f: usize, listed: usize, fonts: usize) -> Option<Vec<usize>> {
    match f {
        0 => Some(vec![1]),
        1 if listed > 0 => Some((2..listed + 2).collect()),
        1 => None,
        _ if fonts > 0 => Some((listed + 2..listed + 2 + fonts).collect()),
        _ => None,
    }
}

/// docs/STUDIO-2.md §4.6's budgets a frame description can be held to:
/// (its key, the budget, what it's called, what to do past it). `draws` is
/// the engine's real limit (the SDK's `MAX_CMDS`, past which `love.draw`
/// errors), as docs/STUDIO-2-API.md [budgets] says; what many draws cost
/// before that is Lua time, which `lua` measures.
const BUDGETS: &[(&str, f64, &str, &str)] = &[
    ("nodes", 4096.0, "2D nodes", "destroy what's gone, reuse nodes, or use a Tilemap"),
    ("draws", maimbrain::scene::draw::MAX_CMDS as f64, "love.draw commands a frame", "use nodes for anything that stays"),
    ("lua", 1_000_000.0, "Luau interrupts a frame", "set goals (flux, vx, follow) rather than positions every frame"),
];

/// A 3D game's, from what the frames drew (runtime/src/stats.ts `BUDGETS` and
/// SPEC's node limit): (the key the runner gives, the key here, the budget, ...).
const BUDGETS_3D: &[(&str, &str, f64, &str, &str)] = &[
    ("nodes", "nodes3d", 4096.0, "3D nodes", "destroy what's gone, or draw copies with node:instance"),
    ("draws", "draws3d", 200.0, "3D draws a frame", "share with node:instance or scatter, or merge geometry"),
    ("triangles", "triangles", 150_000.0, "triangles a frame", "lower a model's polys, or use fewer copies"),
    ("skinned", "skinned", 16.0, "skinned models a frame", "use fewer animated models at once"),
    ("particles", "particles", 5000.0, "live particles", "lower a rate or a life"),
];

/// The peaks of the frames described and drawn, against the budgets: { key:
/// { peak, budget, share } }. What a frame draws counts only if it drew any:
/// a 2D game has no 3D lines.
fn budgets(dumps: &[Value], drawn: &[Value]) -> Value {
    let mut out = serde_json::Map::new();
    let mut add = |key: &str, peak: f64, budget: f64, what: &str, fix: &str| {
        let share = peak / budget;
        let mut b = json!({ "peak": peak, "budget": budget, "share": (share * 1000.0).round() / 1000.0, "what": what });
        if share > 0.75 {
            b["note"] = format!("{:.0}% of the budget for {what}: {fix}", share * 100.0).into();
        }
        out.insert(key.to_string(), b);
    };
    for (key, budget, what, fix) in BUDGETS {
        if let Some(peak) = dumps.iter().filter_map(|d| d[*key].as_f64()).reduce(f64::max) {
            add(key, peak, *budget, what, fix);
        }
    }
    for (from, key, budget, what, fix) in BUDGETS_3D {
        if let Some(peak) = drawn.iter().filter_map(|d| d[*from].as_f64()).reduce(f64::max).filter(|p| *p > 0.0) {
            add(key, peak, *budget, what, fix);
        }
    }
    // A Terrain's reach ÷ detail: past 400 it takes much of the phone's
    // triangles (about 70k at 400; docs/STUDIO-2-API.md [terrain]).
    if let Some(ratio) = dumps.iter().filter_map(|d| d["terrain"].as_f64()).reduce(f64::max) {
        let mut b = json!({ "peak": ratio, "budget": 400.0, "share": (ratio / 400.0 * 1000.0).round() / 1000.0, "what": "a Terrain's reach ÷ detail" });
        if ratio > 400.0 {
            b["note"] = format!("the Terrain's reach ÷ detail is {ratio:.0}, past 400 (about 70k of the phone's 150k triangles): a smaller reach or a larger detail leaves more for the game").into();
        }
        out.insert("terrain".into(), b);
    }
    Value::Object(out)
}

/// The grounding check on the last frame drawn (`shot`, a PNG): its lines
/// and warnings, or a line saying the engine reports no figures.
fn grounding_check(last: Option<&Value>, shot: Option<&Path>, size: [u32; 2]) -> Value {
    let Some(stream) = last.and_then(|d| d["ground"].as_str()) else {
        return json!({ "lines": ["grounding: this engine build doesn't report its figures yet, so nothing was checked"], "warnings": [] });
    };
    // The engine's records, as the SDK's marks carry them.
    let g = crate::grounding::parse([format!("{}{stream}", crate::grounding::MARK).as_str()]);
    let img = shot.and_then(|p| std::fs::read(p).ok()).and_then(|b| crate::generate::img::Image::decode(&b).ok());
    let Some(img) = img else {
        return json!({ "lines": ["grounding: no frame was drawn to check"], "warnings": [] });
    };
    let (lines, warnings) = crate::grounding::check(&g, &img, size);
    json!({ "lines": lines.iter().map(|l| studio_words(l)).collect::<Vec<_>>(), "warnings": warnings.iter().map(|w| studio_words(w)).collect::<Vec<_>>() })
}

/// The grounding check's advice in Studio's words: it was written for the
/// kit's `maimbrain::ground`, and in 2.0 a Walker's `y` is its feet and
/// `order` what's in front.
fn studio_words(line: &str) -> String {
    const SWAPS: &[(&str, &str)] = &[
        (": stand it on the floor `Backdrop::draw` returns", ": set its y (its feet) to the floor's top"),
        ("the painting's floor line at", "the tiles' top at"),
        ("; no floor is marked there: draw the painting with ground::Backdrop (or mark it with ground::floor_line) and stand ", "; there's no tile under it: stand "),
        (" on its floor", " on the level's tiles"),
        (": draw by the feet, back to front (ground::Scene), or give one a layer (Figure::layer)", ": give the nearer one (lower on the screen) the higher order"),
        (" (ground::stop_short: half a body clear), or put the one passing behind on a lower layer (Figure::layer)", ", or give the one passing behind a lower order"),
        (
            "nothing stood on a floor in this frame (stand characters and props with maimbrain::ground::Figure so this can check them)",
            "no Walker on the screen in this frame (the check looks at Walkers)",
        ),
    ];
    SWAPS.iter().fold(line.to_string(), |s, (from, to)| s.replace(from, to))
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
    // Opened once, and run as opened (`replay::run`).
    let b = crate::engines::open(&bundle)?;
    let m = &b.manifest;
    if m.engine.is_none() {
        return Err(format!("{} isn't a Studio game (mb play plays Studio games; mb shot and mb measure play any game)", bundle.display()));
    }
    let listed = b.file(mb_format::studio::INDEX).map(|f| String::from_utf8_lossy(f).lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).count()).unwrap_or(0);
    // Engine 2.0 asks for each baked font again as a font (the SDK's
    // `Font::asset`, its own handle): the identity's as the project loads,
    // the Lua's as `newFont` names them.
    let fonts = b.file("assets/packed.toml").map(|f| String::from_utf8_lossy(f).lines().filter(|l| l.trim_start().starts_with("[fonts.")).count()).unwrap_or(0);
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
    let events = script_events(&script, total, start, HZ)?;
    // The script's sensor readings, for the sensors the game declares (sense.rs).
    // Scripted for what studio.toml lists (the build may import more), sent as the manifest declares.
    let declared = b.file("assets/studio.toml").and_then(|t| String::from_utf8_lossy(t).parse::<toml::Table>().ok()).map(|t| {
        // Tilt as well where the build reads it (a quick-path player.tilt).
        let mut l = super::sense::listed(&t);
        l.extend(m.sensors.iter().filter(|s| **s == mb_format::manifest::Sensor::Tilt));
        l
    });
    let track = super::sense::Track::parse(&script, declared.as_deref().unwrap_or(&m.sensors))?;
    let mut params = serde_json::Map::new();
    // The engine counts updates from 1: log frame f is its frame f + 1.
    params.insert("studio_dump".into(), shots.iter().map(|f| (f + 1).to_string()).collect::<Vec<_>>().join(",").into());
    params.insert("studio_trace".into(), "1".into());
    for c in &a.check {
        match c.as_str() {
            // The engine describes its footed figures and floors in each frame's description.
            "grounding" => {
                params.insert(crate::grounding::PARAM.into(), "1".into());
            }
            // The engine names its subject, and draws the frame's variants when asked (readable.rs).
            "readability" => {
                params.insert("studio_readability".into(), "1".into());
            }
            // The engine logs a pose a frame (smoothness.rs).
            "smoothness" => {
                params.insert("studio_smooth".into(), "1".into());
            }
            other => return Err(format!("--check {other}: the checks are grounding, readability and smoothness")),
        }
    }
    let debug = match &a.debug {
        Some(p) => Some(debug_param(&read_json(p)?)?),
        None => None,
    };
    if let Some(d) = &debug {
        params.insert("studio_debug".into(), d.clone().into());
    }
    let store = match &a.store {
        Some(p) => match read_json(p)? {
            Value::Object(o) if o.values().all(Value::is_string) => Value::Object(o),
            _ => return Err(format!("{}: --store is a JSON object of key → base64", p.display())),
        },
        None => json!({}),
    };
    if !(a.date.len() == 10 && a.date.as_bytes()[4] == b'-' && a.date.as_bytes()[7] == b'-') {
        return Err(format!("--date {}: a UTC date, YYYY-MM-DD", a.date));
    }
    // The replay log the run plays, at `hz` frames a second.
    let log_at = |hz: f64, total: usize, events: &[Vec<Value>], params: &serde_json::Map<String, Value>| -> Value {
        let dt = (1.0 / hz) as f32 as f64;
        let sensed = track.frames(total, hz, &m.sensors);
        let frames: Vec<Value> = (0..total)
            .map(|f| {
                let mut o = serde_json::Map::new();
                o.insert("dt".into(), dt.into());
                if let Some(handles) = arrivals(f, listed, fonts) {
                    o.insert("assets".into(), json!(handles));
                }
                if !events[f].is_empty() {
                    o.insert("events".into(), Value::Array(events[f].clone()));
                }
                o.extend(sensed[f].clone());
                Value::Object(o)
            })
            .collect();
        let mut log = json!({
            "version": 1,
            "game": m.id,
            "bundle": b.sha256,
            "seed": a.seed.to_string(),
            "dailySeed": mb_format::daily_seed(&m.id, &a.date).to_string(),
            "store": store.clone(),
            "playerId": "00112233445566778899aabbccddeeff",
            "locale": "en-US",
            "screen": [w, h, 0, 0, 0, 0, 2, 0],
            "params": params,
            "frames": frames,
        });
        if debug.is_some() {
            log["debug"] = true.into();
        }
        log
    };
    let log = log_at(HZ, total, &events, &params);
    if let Some(p) = &a.save_run {
        // Without the tool's own params (its frame dumps): a phone's run has none.
        let mut run = log.clone();
        if let Some(o) = run.as_object_mut() {
            o.remove("params");
        }
        std::fs::write(p, serde_json::to_vec(&run).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let runtime = a.runtime.clone().or_else(|| std::env::var_os("MB_RUNTIME_DIR").map(PathBuf::from));
    let opts = crate::replay::Options { json: true, runtime, timeout: std::time::Duration::from_secs(a.timeout) };
    // Every frame is the GPU's (docs/STUDIO-2.md §4.11).
    let readability = a.check.iter().any(|c| c == "readability");
    let capture = crate::replay::Capture { frames: shots.iter().map(|f| f + 1).collect(), size: [w * a.scale, h * a.scale], readability };
    let (result, stderr) = crate::replay::run(&b, &log, &work.join("run"), &opts, Some(&capture))?;
    // The smoothness check's second run: the same play at twice the rate, nothing drawn.
    let smoothness = if a.check.iter().any(|c| c == "smoothness") {
        let hz2 = HZ * 2.0;
        let total2 = (total - 1) * 2 + 1;
        let events2 = script_events(&script, total2, start, hz2)?;
        let mut params2 = params.clone();
        params2.remove("studio_dump");
        params2.remove("studio_readability");
        params2.remove(crate::grounding::PARAM);
        let (_, stderr2) = crate::replay::run(&b, &log_at(hz2, total2, &events2, &params2), &work.join("run120"), &opts, None)?;        Some(super::smoothness::check(&super::smoothness::parse(&stderr), HZ, &super::smoothness::parse(&stderr2), hz2, [w as f64, h as f64]))
    } else {
        None
    };

    // What the game logged: errors, events, frame descriptions, the rest.
    let mut errors = Vec::new();
    let mut lines = Vec::new();
    let mut evs = Vec::new();
    let mut sounds = Vec::new();
    let mut dumps: Vec<Value> = Vec::new();
    let mut plain_shaders: Option<String> = None;
    for l in stderr.lines() {
        // The GPU's own messages (a shader refused, a validation error): the log's.
        if let Some(g) = l.strip_prefix("[gpu] ") {
            if lines.len() < 200 {
                lines.push(g.to_string());
            }
            continue;
        }
        let Some(msg) = l.strip_prefix("[game] ") else { continue };
        // The server's frame budget for shader materials (Decided 51): from here they're drawn plain.
        if let Some(s) = msg.strip_prefix("studio-shaders-plain ") {
            let w: Vec<&str> = s.split_whitespace().collect();
            if let [frame, ms, ..] = &w[..] {
                let at = frame.parse::<f64>().unwrap_or(1.0).max(1.0) - 1.0;
                plain_shaders = Some(format!(
                    "Shader materials are drawn plain in these frames from {:.1} s: a frame with them took {ms} ms here, over the server's frame budget (a phone's GPU draws them, so check them there)",
                    at / HZ
                ));
            }
            continue;
        }
        if msg.starts_with("studio-pose ") {
            continue; // the smoothness check's (smoothness.rs)
        }
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
        } else if let Some(s) = msg.strip_prefix("studio-sound ") {
            if let Some(v) = sound_line(s)
                && sounds.len() < MAX_SOUNDS
            {
                sounds.push(v);
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
        evs.push(json!({ "at": s["frame"].as_f64().unwrap_or(0.0) / HZ, "score": s["value"].as_str().and_then(|v| v.parse::<i64>().ok()), "board": s["board"] }));
    }

    evs.sort_by(|a, b| a["at"].as_f64().unwrap_or(0.0).total_cmp(&b["at"].as_f64().unwrap_or(0.0)));

    // The frames, drawn.
    let out = a.out.clone().unwrap_or_else(|| if a.game.is_dir() { a.game.join("screenshots") } else { PathBuf::from("screenshots") });
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut shots_out = Vec::new();
    for (f, name) in crate::replay::save_captures(&work.join("run"), &capture, &out, |f| format!("play-{:.2}s.png", (f - 1) as f64 / HZ))? {
        shots_out.push(json!({ "at": (f - 1) as f64 / HZ, "file": name }));
    }
    if let Some(e) = result["captureError"].as_str() {
        lines.push(format!("mb play: the frames couldn't be drawn ({e})"));
    }
    if let Some(p) = &plain_shaders {
        lines.push(p.clone());
    }
    let score = dumps.iter().rev().find_map(|d| d["score"].as_f64()).map(|s| s.round() as i64).or_else(|| {
        result["scores"].as_array().and_then(|s| s.last()).and_then(|s| s["value"].as_str()?.parse().ok())
    });
    let mut summary = summary(dumps.last(), &errors, total);
    if let Some(p) = &plain_shaders {
        summary.push(' ');
        summary.push_str(p);
        summary.push('.');
    }
    let mut checks = serde_json::Map::new();
    if a.check.iter().any(|c| c == "grounding") {
        let last = shots_out.last().and_then(|f| f["file"].as_str()).map(|f| out.join(f));
        // The description of the last frame drawn: the dumps are in frame order, one a shot.
        let described = dumps.iter().rev().find(|d| d["ground"].is_string()).or(dumps.last());
        checks.insert("grounding".into(), grounding_check(described, last.as_deref(), [w, h]));
    }
    if readability {
        let run = work.join("run");
        let frames: Vec<super::readable::Frame> = shots_out
            .iter()
            .filter_map(|s| {
                let at = s["at"].as_f64()?;
                let file = s["file"].as_str()?.to_string();
                // The log's frame (and the engine's count) for this shot.
                let f = (at * HZ).round() as usize + 1;
                let shown = crate::generate::img::Image::decode(&std::fs::read(out.join(&file)).ok()?).ok()?;
                let described = dumps.iter().find(|d| d["frame"].as_u64() == Some(f as u64)).cloned().unwrap_or(Value::Null);
                let variant = |v: &str| crate::replay::captured_variant(&run, &capture, f, v);
                let plain = variant("plain").zip(variant("plain-without"));
                Some(super::readable::Frame { at, described, shown, clean: variant("clean"), alone: variant("subject"), plain, file })
            })
            .collect();
        checks.insert("readability".into(), super::readable::check(&frames, a.scale, &out));
    }
    if let Some(s) = smoothness {
        checks.insert("smoothness".into(), s);
    }
    Ok(json!({
        "budgets": budgets(&dumps, result["drawn"].as_array().map_or(&[], Vec::as_slice)),
        "checks": checks,
        "ok": errors.is_empty(),
        "errors": errors,
        "log": lines,
        "events": evs,
        "sounds": sounds,
        "score": score,
        "frames": shots_out,
        "summary": summary,
        "seconds": (total - 1) as f64 / HZ,
        "progress": result["progress"],
        "hash": result["hash"],
    }))
}

/// Sounds a run lists, at most (a hum's level changes count).
const MAX_SOUNDS: usize = 2000;

/// A `studio-sound` line from engine 2.0 (games/studio-engine2's
/// api/sound.rs): `<seconds> <sound> <vol> <pitch> play|loop|set|stop
/// id=<voice> [pan=<p>]`, the sounds a round started (sound can't be heard
/// headless, but when it plays can be checked).
fn sound_line(s: &str) -> Option<Value> {
    let w: Vec<&str> = s.split_whitespace().collect();
    let [t, name, vol, pitch, kind, id, rest @ ..] = &w[..] else { return None };
    let pan = rest.iter().find_map(|w| w.strip_prefix("pan=")).and_then(|p| p.parse::<f64>().ok()).unwrap_or(0.0);
    Some(json!({
        "at": t.parse::<f64>().ok()?,
        "sound": name,
        "kind": kind,
        "volume": vol.parse::<f64>().ok()?,
        "pitch": pitch.parse::<f64>().ok()?,
        "pan": pan,
        "voice": id.strip_prefix("id=")?,
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
    // Engine 2.0 draws the cards from the identity, so it says what they say.
    if let Some(c) = d["card"].as_str() {
        s += &format!(" (\"{c}\"");
        if let Some(i) = d["identity"].as_str() {
            s += &format!(", in the {i} identity");
        }
        s.push(')');
    }
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
        let units = if d["units"] == "logical" { "units" } else { "art pixels" };
        s += &format!(". On screen ({units}, {}×{}): {}", d["w"], d["h"], parts.join("; "));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// files.txt, then what it lists, then (every frame on) the baked fonts'
    /// own handles, which the engine asks for after the listed files.
    #[test]
    fn baked_fonts_arrive_after_the_listed_files() {
        assert_eq!(arrivals(0, 6, 2), Some(vec![1]));
        assert_eq!(arrivals(1, 6, 2), Some(vec![2, 3, 4, 5, 6, 7]));
        assert_eq!(arrivals(2, 6, 2), Some(vec![8, 9]), "the fonts' handles, the frame after the files");
        assert_eq!(arrivals(500, 6, 2), Some(vec![8, 9]), "and later, for a font a round asks for first");
        assert_eq!(arrivals(2, 6, 0), None);
    }

    #[test]
    fn sound_lines_become_sounds() {
        let v = sound_line("1.250 step 0.512 0.981 play id=f3 pan=-0.20").unwrap();
        assert_eq!(v, json!({ "at": 1.25, "sound": "step", "kind": "play", "volume": 0.512, "pitch": 0.981, "pan": -0.2, "voice": "f3" }));
        assert_eq!(sound_line("2.000 hum 0 1 stop id=f4").unwrap()["kind"], "stop");
        assert!(sound_line("garbage").is_none());
    }

    #[test]
    fn scripts_become_log_events() {
        let s = json!([
            { "at": 0.5, "tap": [180, 500] },
            { "at": 1.0, "swipe": [100, 500, 200, 500], "for": 0.05 },
            { "at": 1.5, "key": "space" },
            { "at": 2.0, "pad": "lx", "value": -1, "for": 0.1 },
        ]);
        let e = script_events(&s, 200, Some((true, 0.0, 0.0)), HZ).unwrap();
        assert_eq!(e[3], vec![json!([5, 0, 0, 0.0, 0.0, 0.0, 0, 40])], "Enter starts the round");
        assert_eq!(e[30], vec![json!([1, 0, 0, 180.0, 500.0, 1.0, 0, 0])]);
        assert_eq!(e[34], vec![json!([3, 0, 0, 180.0, 500.0, 0.0, 0, 0])]);
        assert_eq!(e[60][0][0], 1);
        assert_eq!(e[63][0][0], 2, "the swipe moves");
        assert_eq!(e[90], vec![json!([5, 0, 0, 0.0, 0.0, 0.0, 0, 44])]);
        assert_eq!(e[120], vec![json!([12, 1, 0, 0.0, 0.0, -1.0, 0, 0])]);
        assert!(script_events(&json!([{ "at": 1, "kick": 1 }]), 10, None, HZ).unwrap_err().contains("needs one of"));
        // At 120 Hz the same script lands at the same times; sensor items aren't events.
        let e = script_events(&json!([{ "at": 0.5, "tap": [180, 500] }, { "at": 1, "tilt": [20] }]), 400, Some((true, 0.0, 0.0)), 120.0).unwrap();
        assert_eq!((e[6].len(), e[10].len(), e[60].len(), e[68].len()), (1, 1, 1, 1), "the start press and the tap, at their times");
        assert_eq!(e.iter().map(Vec::len).sum::<usize>(), 4);
        assert!(script_events(&json!([{ "at": 1, "key": "banana" }]), 10, None, HZ).unwrap_err().contains("unknown key"));
        let swipe = script_events(&json!([{ "at": 1, "swipe": "left" }]), 10, None, HZ).unwrap_err();
        assert!(swipe.contains("[x1, y1, x2, y2]"), "{swipe}");
        for bad in [json!([{ "at": 1, "tap": [5] }]), json!([{ "at": 1, "hold": [] }]), json!([{ "at": 1, "release": "here" }])] {
            assert!(script_events(&bad, 10, None, HZ).unwrap_err().contains("is [x, y]"), "{bad}");
        }
    }

    #[test]
    fn budgets_are_the_peaks_against_the_design() {
        let b = budgets(&[json!({ "nodes": 120, "draws": 10 }), json!({ "nodes": 3500, "draws": 4 }), json!({ "mode": "title" })], &[json!({ "nodes": 90, "draws": 60, "triangles": 130000, "skinned": 0 })]);
        assert_eq!(b["nodes"]["peak"], 3500.0);
        assert!(b["nodes"]["note"].as_str().unwrap().contains("85% of the budget for 2D nodes"));
        assert_eq!(b["draws"]["peak"], 10.0);
        assert!(b["draws"].get("note").is_none());
        // The draw budget is the engine's limit, as the API page says.
        assert_eq!(b["draws"]["budget"], 32768.0);
        let api = include_str!("../../../../docs/STUDIO-2-API.md");
        assert!(api.contains("`love.draw`: 32,768 draws a frame"), "the API page's draw limit");
        assert!(b.get("lua").is_none(), "only what the engine reports");
        assert!(b.get("terrain").is_none(), "no terrain, no line");
        let t = budgets(&[json!({ "terrain": 300 }), json!({ "terrain": 600 })], &[]);
        assert!(t["terrain"]["note"].as_str().unwrap().contains("reach ÷ detail is 600, past 400"));
        assert!(budgets(&[json!({ "terrain": 400 })], &[])["terrain"].get("note").is_none());
        let g = grounding_check(Some(&json!({ "mode": "play" })), None, [360, 640]);
        assert!(g["lines"][0].as_str().unwrap().contains("doesn't report its figures yet"));
        // A walker whose feet are 20 units above the floor under it floats.
        let dir = std::env::temp_dir().join(format!("mb-play-ground-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shot = dir.join("shot.png");
        std::fs::write(&shot, crate::generate::img::Image::new(360, 640, [40, 40, 40, 255]).encode_png()).unwrap();
        let d = json!({ "ground": "ffox,30.0,40.0,70.0,76.0,1,50.0,20.0;F96.0,32.0,64.0;" });
        let g = grounding_check(Some(&d), Some(&shot), [360, 640]);
        assert_eq!(g["warnings"][0], "fox floats 20 units above the floor (its feet at y 76, the tiles' top at 96): set its y (its feet) to the floor's top");
        let _ = std::fs::remove_dir_all(&dir);
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
        // Engine 2.0's chunks are named by their paths.
        assert_eq!(lua_position("game/enemies/boss.lua:12: node.alpha takes 0–1"), Some(("game/enemies/boss.lua".into(), 12)));
        assert_eq!(lua_position("Sprite{…}: game/main.lua:3: boom"), Some(("game/main.lua".into(), 3)));
    }
}
