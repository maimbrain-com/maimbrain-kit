//! `mb shot`: screenshots of a game at given moments, from a headless Chrome
//! (SPEC §9), so looking at the game is one command.
//!
//!     mb shot games/frogger                          # 0.5, 2, 5 and 10 s into a fresh session
//!     mb shot games/frogger --autoplay --sheet       # tap to start at 0.5 s; one sheet of all
//!     mb shot games/frogger --at 1,3 --tap 180,500@0.5 --swipe 100,400,260,400@2:0.2
//!     mb shot games/brawl --at mark:boss,mark:boss+0.5  # when the game calls sys::mark("boss"), and 0.5 s on
//!
//! The game is built with the fast build `mb serve --watch` uses
//! (devbuild.rs), staged with the embedded runtime and served on a free
//! local port. Chrome renders it at the game's logical size with 2 device
//! pixels per unit and touch emulation, like a phone. The session is held
//! (`mb.restart({hold, seed})`) and stepped 1/60 s at a time with `mb.step`
//! (1/120 s with `--hz 120`, as on a ProMotion display), so a shot at 2 s
//! is frame 120 every time; touches are real touch events
//! (CDP `Input.dispatchTouchEvent`) sent between steps, so the game sees a
//! tap or a swipe with its intermediate moves exactly as it would on a phone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;
use mb_format::Manifest;
use mb_format::bundle::ASSET_EXTENSIONS;
use serde_json::{Value, json};

use crate::cdp::Chrome;
use crate::generate::img::Image;
use crate::{budget, cdp, devbuild, fonts, grounding, readability, serve, smooth};

/// The display rates --hz takes: down to 1/MAX_DT (the runtime's longest frame), up to a ProMotion display and then some.
const HZ_RANGE: std::ops::RangeInclusive<f64> = 10.0..=240.0;
/// How long a tap's finger stays down (4 frames at 60 Hz, a quick real tap).
const TAP_SECS: f64 = 4.0 / 60.0;
/// A swipe's default duration, in seconds.
const SWIPE_SECS: f64 = 0.15;
/// When --autoplay taps, at the latest (the default first shot shows the title card first).
const AUTOPLAY_SECS: f64 = 0.5;
/// How long the page may take to boot (compile the wasm, load startup assets).
const BOOT_TIMEOUT: Duration = Duration::from_secs(60);
/// The UTC date for the daily seed, as `mb serve` uses.
const DATE: &str = "2026-10-06";

#[derive(clap::Args)]
pub struct ShotArgs {
    /// Game directory or .mbx bundle.
    path: PathBuf,
    /// When to capture: game times in seconds from a fresh session (60 frames a second, or --hz),
    /// or `mark:<label>[+SECS]`, the frame the game first calls `sys::mark("<label>")` (and SECS
    /// after it): --at 2,mark:boss,mark:boss+0.5.
    #[arg(long, value_delimiter = ',', default_value = "0.5,2,5,10", value_name = "SECS|mark:LABEL[+SECS],…")]
    at: Vec<String>,
    /// How long --at mark:… waits for the game to mark, in game seconds.
    #[arg(long, default_value_t = 60.0, value_name = "SECS")]
    mark_wait: f64,
    /// An `orientation = "any"` game held sideways: the phone is emulated landscape (the
    /// logical size's width and height swapped), as when it's turned.
    #[arg(long, conflicts_with = "screen")]
    landscape: bool,
    /// A landscape game as a card in the portrait feed: the phone is emulated upright (the
    /// logical size swapped), so the shot shows the vertical card it draws there.
    #[arg(long, conflicts_with_all = ["landscape", "screen"])]
    portrait: bool,
    #[command(flatten)]
    script: Script,
    /// Print the game's log lines (`sys::log`) at this level and up, each with its frame and game
    /// time, after the shots (and in the sheet's .txt). Without --logs: info and up; --logs alone:
    /// every level, debug too. A line logged again and again is listed once, with its count; only
    /// the first 20 are printed (all of them with --logs-match, and in the .txt).
    #[arg(long, value_enum, num_args = 0..=1, require_equals = true, default_missing_value = "debug", value_name = "LEVEL")]
    logs: Option<LogLevel>,
    /// Only the log lines that contain this text.
    #[arg(long, value_name = "TEXT")]
    logs_match: Option<String>,
    /// Shade where the feed draws over a card (the preview's ?overlays).
    #[arg(long)]
    overlays: bool,
    /// Print a line per shot: what its frame drew (draw calls, triangles, reflected draws, shadow
    /// casters, particles) and the p95 ms of the frames since the last shot (update, render,
    /// present, GPU). --stats=json prints all of `mb.stats()` instead. (A shot whose frames went
    /// over the skill's budgets is flagged, with the nodes that cost the most, either way.)
    #[arg(long, value_enum, num_args = 0..=1, require_equals = true, default_missing_value = "line", value_name = "json")]
    stats: Option<StatsFormat>,
    /// Also write one contact sheet with every shot side by side, labelled with its time.
    #[arg(long)]
    sheet: bool,
    /// Check that the subject reads: how much of the screen it covers, how its outline stands out
    /// from what's around it in lightness (greyscale) and color, its own tonal range, how much
    /// the HUD covers it and how dark the frame is; warns when it doesn't separate. The subject is
    /// the mb3d node the game names with `Node::mark_subject()` (or --subject). Also writes
    /// <shot>.readability.png: the frame in greyscale with the subject outlined.
    #[arg(long)]
    readability: bool,
    /// With --readability, the subject's mb3d node handle (a handle the game logs with
    /// `Node::raw()`), instead of the one it marks.
    #[arg(long, value_name = "NODE", requires = "readability")]
    subject: Option<u32>,
    /// Print what stands on what under each shot: the figures the game stands with
    /// `maimbrain::ground::Figure` and the floors under them. The check runs without this too
    /// (unless --no-grounding), warning under a shot where a figure floats above its floor, is
    /// cut off, has no contact shadow, stands in another or is drawn in the wrong order, or where
    /// a tap area (`ground::tap_area`) misses part of the figure it's for.
    #[arg(long, conflicts_with = "no_grounding")]
    grounding: bool,
    /// Draw the tap targets over the shots (the `mb.taps` debug parameter): each tap area the
    /// game declares with `ground::tap_area` in magenta, each figure's `Figure::hit` in cyan, and
    /// print their sizes under each shot. Check by eye that a tap on the head counts.
    #[arg(long, conflicts_with = "no_grounding")]
    taps: bool,
    /// Run the game without the grounding check's reports (the `mb.ground` debug parameter).
    #[arg(long)]
    no_grounding: bool,
    /// Check that the mb3d camera moves smoothly: run the script again at twice --hz and report
    /// the frames where its position, direction or field of view jumps, kinks or depends on the
    /// frame rate (and where its near or far plane steps).
    #[arg(long)]
    smoothness: bool,
    /// With --smoothness, also check this mb3d node's motion (a node handle the game logs, e.g.
    /// the player's).
    #[arg(long, value_name = "NODE", requires = "smoothness")]
    track_node: Option<u32>,
    /// Where to write the PNGs (default: the game's screenshots/; for a .mbx, ./screenshots/).
    #[arg(long)]
    out: Option<PathBuf>,
    /// File name prefix: <name>-<time>s.png and <name>-sheet.png.
    #[arg(long, default_value = "shot")]
    name: String,
    /// Port to serve the game on (default: any free one).
    #[arg(long)]
    port: Option<u16>,
    /// The Chrome, Chromium or Edge executable (default: MB_CHROME, else the usual places).
    #[arg(long)]
    chrome: Option<PathBuf>,
}

/// Scripted play, shared by `mb shot` and `mb measure`: input at game times, the seed and the
/// display rate.
#[derive(clap::Args, Clone)]
pub struct Script {
    /// A tap at x,y (the game's logical units) at a game time: --tap 180,500@1.5. Repeatable.
    #[arg(long, value_name = "X,Y@T")]
    tap: Vec<String>,
    /// A swipe from x1,y1 to x2,y2 starting at a game time, lasting 0.15 s or the given seconds
    /// (a swipe that doesn't move is a hold): --swipe 80,400,280,400@2:0.3. Repeatable.
    #[arg(long, value_name = "X1,Y1,X2,Y2@T[:SECS]")]
    swipe: Vec<String>,
    /// Tilt the phone (games that declare tilt): roll and optional pitch in degrees from
    /// upright, held until the next --tilt: --tilt -20@3 at once, or --tilt -20@3:0.4 turning
    /// there over 0.4 s as a hand does (smoothly, minimum jerk). Repeatable. Sampled at 100 Hz
    /// like a phone's. Until the first, the phone is upright and still: unlike in `mb serve`,
    /// touches don't tilt or shake it.
    #[arg(long, value_name = "ROLL[,PITCH]@T[:SECS]", allow_hyphen_values = true)]
    tilt: Vec<String>,
    /// Point the phone (games that declare attitude): turn right by YAW and point up by PITCH
    /// degrees from where it started (upright, facing ahead), held until the next --aim:
    /// --aim 15,-5@2 at once, or --aim 15,-5@2:0.3 turning there over 0.3 s as a hand does
    /// (minimum jerk, with the rotation rate a gyro would report). Repeatable. Sampled at
    /// 100 Hz; gravity follows. Not with --tilt.
    #[arg(long, value_name = "YAW[,PITCH]@T[:SECS]", allow_hyphen_values = true)]
    aim: Vec<String>,
    /// Tap the middle of the screen at 0.5 s (or right after an earlier first shot), to get past
    /// a title card.
    #[arg(long)]
    autoplay: bool,
    /// A debug parameter for the game, read with `sys::param("zone")` (SPEC §5.1): --param zone=3
    /// to start where you want to look without editing the game. Repeatable. Recorded with the
    /// session, so replays match; never set on a device.
    #[arg(long, value_name = "NAME=VALUE")]
    pub param: Vec<String>,
    /// Session seed.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Display rate: 120 runs the game in frames of 1/120 s, as on a ProMotion iPhone (a game
    /// whose motion depends on the frame rate looks different, or plays differently).
    #[arg(long, default_value_t = 60.0, value_name = "FPS")]
    pub hz: f64,
    /// The phone's screen in points, for a `fit = "fill"` game, whose Screen grows to its shape:
    /// WxH (--screen 360x780; wider than tall is the phone held sideways) or an iPhone: se
    /// (375x667), mini (375x812), iphone (393x852), pro-max (440x956). The page is emulated at
    /// the Screen the game gets there; a letterboxed game keeps its own size (with bars).
    #[arg(long, value_name = "WxH|se|mini|iphone|pro-max")]
    pub screen: Option<String>,
}

/// `sys::log`'s levels, lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The level of a frame's log line as `mb.step()` lists it ("warn: …").
    fn of(line: &str) -> LogLevel {
        match line.split_once(':').map(|x| x.0) {
            Some("debug") => LogLevel::Debug,
            Some("warn") => LogLevel::Warn,
            Some("error") => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

/// The game's log lines in `mb.step()`'s timings, at `min` and up and
/// containing `matching`: "frame 31 (0.52 s)  warn: …", in frame order.
/// A line the game logs again and again (every frame, say) is listed once,
/// at its first frame, with how many times and until when.
fn game_logs(timings: &[Value], hz: f64, min: LogLevel, matching: Option<&str>) -> Vec<String> {
    // (first frame, line, times, last frame), in the order first logged.
    let mut seen: Vec<(u64, &str, u32, u64)> = Vec::new();
    let mut index: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for t in timings {
        let frame = t["frame"].as_u64().unwrap_or(0);
        for l in t["logs"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if LogLevel::of(l) >= min && matching.is_none_or(|m| l.contains(m)) {
                match index.get(l) {
                    Some(&i) => {
                        seen[i].2 += 1;
                        seen[i].3 = frame;
                    }
                    None => {
                        index.insert(l, seen.len());
                        seen.push((frame, l, 1, frame));
                    }
                }
            }
        }
    }
    seen.into_iter()
        .map(|(frame, l, n, last)| {
            let again = if n > 1 { format!("  (×{n}, until frame {last})") } else { String::new() };
            format!("frame {frame} ({:.2} s)  {l}{again}", frame as f64 / hz)
        })
        .collect()
}

/// How many of the game's log lines `mb shot` prints (the sheet's .txt has them all).
const LOG_LINES: usize = 20;

/// Set while `mb shot` lists the game's log lines itself (with their frames):
/// the page's console copies of them aren't printed as well.
static GAME_LOGS_LISTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum StatsFormat {
    /// One line per shot.
    Line,
    /// `mb.stats()` as JSON.
    Json,
}

/// What happens between two frames, besides shots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Act {
    /// The finger moves to x, y.
    Move(f64, f64),
    /// The finger lifts.
    Up,
    /// Gravity for this roll and pitch (degrees), until the next one.
    Tilt(f64, f64),
    /// The phone pointed this yaw and pitch (degrees), turning at these rates (degrees/s).
    Aim(f64, f64, f64, f64),
    /// A finger touches at x, y (logical units).
    Down(f64, f64),
}

impl Act {
    /// Order within one frame boundary: a swipe's last move before its lift,
    /// and a finger lifts before the next one lands.
    fn rank(&self) -> u8 {
        match self {
            Act::Move(..) => 0,
            Act::Up => 1,
            Act::Tilt(..) | Act::Aim(..) => 2,
            Act::Down(..) => 3,
        }
    }
}

/// When everything happens, in frames: a shot at frame f is taken after f
/// frames have run; an act at frame f is sent after f frames have run (after
/// that boundary's shot), so the game receives it in frame f + 1.
#[derive(Debug, PartialEq)]
pub(crate) struct Plan {
    /// (frame, the time asked for), in order.
    pub(crate) shots: Vec<(u32, f64)>,
    /// Shots at the game's marks (`--at mark:…`), taken when they come.
    pub(crate) marks: Vec<MarkShot>,
    /// How many frames to wait for the marks, at most.
    pub(crate) mark_wait: u32,
    pub(crate) acts: Vec<(u32, Act)>,
    /// Every tilt (or aim) sample at its own game time (s), at `TILT_HZ` while the
    /// phone turns: what a live run sends when (`acts` has, for each frame,
    /// the newest sample by its start).
    pub(crate) tilts: Vec<(f64, Act)>,
}

/// `--at mark:<label>[+SECS]`: a shot SECS after the frame where the game
/// first marks `label` (`sys::mark`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MarkShot {
    pub(crate) label: String,
    pub(crate) after: f64,
}

impl MarkShot {
    /// What the shot is called in file names and lists: `boss`, `boss+0.5`.
    fn name(&self) -> String {
        if self.after > 0.0 { format!("{}+{}", self.label, secs(self.after)) } else { self.label.clone() }
    }
}

/// Splits `--at`'s values into game times and marks.
fn moments(at: &[String]) -> Result<(Vec<f64>, Vec<MarkShot>), String> {
    let (mut times, mut marks) = (Vec::new(), Vec::new());
    for spec in at {
        let spec = spec.trim();
        if let Some(rest) = spec.strip_prefix("mark:") {
            let (label, after) = match rest.rsplit_once('+') {
                Some((l, a)) => (l, a.parse::<f64>().map_err(|_| format!("--at {spec}: expected mark:LABEL+SECS, e.g. mark:boss+0.5"))?),
                None => (rest, 0.0),
            };
            if label.is_empty() || label.len() > 64 {
                return Err(format!("--at {spec}: a mark's label is 1 to 64 bytes, as the game passes it to sys::mark"));
            }
            if !after.is_finite() || after < 0.0 {
                return Err(format!("--at {spec}: the seconds after a mark are ≥ 0"));
            }
            let m = MarkShot { label: label.to_string(), after };
            if !marks.contains(&m) {
                marks.push(m);
            }
        } else {
            times.push(spec.parse::<f64>().map_err(|_| format!("--at {spec}: expected seconds (2.5) or mark:LABEL[+SECS]"))?);
        }
    }
    Ok((times, marks))
}

/// How often a phone samples tilt (SPEC §5.7: CoreMotion's fused gravity at 100 Hz).
pub(crate) const TILT_HZ: f64 = 100.0;

/// A minimum-jerk turn from 0 to 1 as `u` goes from 0 to 1: how a hand
/// moves, with no jump in speed or acceleration at either end.
fn min_jerk(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * u * (10.0 - 15.0 * u + 6.0 * u * u)
}

/// The speed of [`min_jerk`] at `u` (per unit of `u`).
fn min_jerk_speed(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    30.0 * u * u * (1.0 - u) * (1.0 - u)
}

/// A phone turning (`--tilt`, `--aim`) as samples: every one at its own game
/// time, `[a, b, da/dt, db/dt]`, and for each frame boundary the newest by then.
type Turns = (Vec<(f64, [f64; 4])>, Vec<(u32, [f64; 4])>);

/// Plans the `--flag A[,B]@T[:SECS]` turns, each from where the last left the
/// phone (from 0, 0), angles within ±`limit` degrees.
fn plan_turns(specs: &[String], flag: &str, shape: &str, limit: f64, example: &str, hz: f64) -> Result<Turns, String> {
    let mut turns = Vec::new();
    for spec in specs {
        let (p, t, dur) = parse_timed(spec, flag, 1..=2, shape)?;
        if p.iter().any(|d| d.abs() > limit) {
            return Err(format!("--{flag} {spec}: expected angles in degrees (±{limit}), a time and how long it takes, e.g. {example}"));
        }
        turns.push((t, dur.unwrap_or(0.0), (p[0], p.get(1).copied().unwrap_or(0.0)), spec));
    }
    turns.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut all: Vec<(f64, [f64; 4])> = Vec::new();
    let mut frames: Vec<(u32, [f64; 4])> = Vec::new();
    let mut from = (0.0, 0.0);
    for (i, &(t, dur, to, spec)) in turns.iter().enumerate() {
        if let Some(&(t0, d0, _, prev)) = i.checked_sub(1).map(|j| &turns[j])
            && t < t0 + d0
        {
            return Err(format!("--{flag} {spec} starts while --{flag} {prev} is still turning: one turn at a time"));
        }
        if dur == 0.0 {
            let v = [to.0, to.1, 0.0, 0.0];
            all.push((t, v));
            // Not before the last turn's final frame: a step is the newer sample.
            let b = frame_of(t, hz).max(frames.last().map_or(0, |l: &(u32, [f64; 4])| l.0));
            frames.push((b, v));
        } else {
            // A sample every 1/TILT_HZ s; each frame sees the newest by its start (its boundary).
            let n = (dur * TILT_HZ - 1e-9).ceil().max(1.0) as u32;
            for k in 1..=n {
                let ts = t + k as f64 / TILT_HZ;
                let u = k as f64 / TILT_HZ / dur;
                let (e, de) = (min_jerk(u), min_jerk_speed(u) / dur);
                let v = [from.0 + (to.0 - from.0) * e, from.1 + (to.1 - from.1) * e, (to.0 - from.0) * de, (to.1 - from.1) * de];
                all.push((ts, v));
                let b = (ts * hz - 1e-6).ceil() as u32;
                match frames.last_mut() {
                    Some(last) if last.0 == b => *last = (b, v),
                    _ => frames.push((b, v)),
                }
            }
        }
        from = to;
    }
    Ok((all, frames))
}

/// The frame boundary nearest game time `t` at `hz` frames a second.
pub(crate) fn frame_of(t: f64, hz: f64) -> u32 {
    (t * hz).round() as u32
}

/// Splits `X,Y,…@T[:SECS]` into its numbers, its time and its duration.
fn parse_timed(spec: &str, flag: &str, n: std::ops::RangeInclusive<usize>, shape: &str) -> Result<(Vec<f64>, f64, Option<f64>), String> {
    let bad = |why: &str| format!("--{flag} {spec}: {why} (expected {shape})");
    let (body, when) = spec.split_once('@').ok_or_else(|| bad("no @time"))?;
    let (t, dur) = match when.split_once(':') {
        Some((t, d)) => (t, Some(d)),
        None => (when, None),
    };
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    let nums: Vec<f64> = body.split(',').map(|s| num(s).ok_or_else(|| bad(&format!("{s:?} is not a number")))).collect::<Result<_, _>>()?;
    if !n.contains(&nums.len()) {
        return Err(bad(&format!("{} numbers", nums.len())));
    }
    let t = num(t).filter(|t| *t >= 0.0).ok_or_else(|| bad("the time must be seconds ≥ 0"))?;
    let dur = match dur {
        Some(d) => Some(num(d).filter(|d| *d > 0.0).ok_or_else(|| bad("the duration must be seconds > 0"))?),
        None => None,
    };
    Ok((nums, t, dur))
}

impl Plan {
    /// Schedules the shots and inputs for a game `size` logical units big.
    pub(crate) fn new(s: &Script, at: &[f64], size: [u32; 2]) -> Result<Plan, String> {
        Plan::build(s, at, Vec::new(), 0.0, size)
    }

    /// A plan for `mb shot`: `--at`'s times and marks, waiting up to `wait` seconds for the marks.
    pub(crate) fn shoot(s: &Script, at: &[String], wait: f64, size: [u32; 2]) -> Result<Plan, String> {
        let (times, marks) = moments(at)?;
        if !marks.is_empty() && !(wait.is_finite() && wait > 0.0) {
            return Err(format!("--mark-wait {wait}: seconds > 0"));
        }
        Plan::build(s, &times, marks, wait, size)
    }

    fn build(s: &Script, at: &[f64], marks: Vec<MarkShot>, wait: f64, size: [u32; 2]) -> Result<Plan, String> {
        let hz = s.hz;
        if !HZ_RANGE.contains(&hz) {
            return Err(format!("--hz {hz}: frames a second from {} to {}", HZ_RANGE.start(), HZ_RANGE.end()));
        }
        let tap_frames = frame_of(TAP_SECS, hz).max(1);
        let mut times: Vec<f64> = Vec::new();
        for &t in at {
            if !t.is_finite() || t < 0.0 {
                return Err(format!("--at {t}: times are seconds ≥ 0"));
            }
            if !times.iter().any(|&x| frame_of(x, hz).max(1) == frame_of(t, hz).max(1)) {
                times.push(t);
            }
        }
        if times.is_empty() && marks.is_empty() {
            return Err("--at: give at least one time".into());
        }
        times.sort_by(f64::total_cmp);
        // Frame 0 is mb_init alone, which draws nothing: the first shot is after frame 1.
        let shots: Vec<(u32, f64)> = times.iter().map(|&t| (frame_of(t, hz).max(1), t)).collect();

        let mut acts = Vec::new();
        // Each finger's (first, last) frame, to keep to one finger at a time.
        let mut fingers: Vec<(u32, u32, String)> = Vec::new();
        let in_bounds = |x: f64, y: f64| x >= 0.0 && y >= 0.0 && x <= size[0] as f64 && y <= size[1] as f64;
        let mut tap = |x: f64, y: f64, t: f64, what: String| -> Result<(), String> {
            if !in_bounds(x, y) {
                return Err(format!("{what}: {x},{y} is outside the {}×{} screen", size[0], size[1]));
            }
            let f = frame_of(t, hz);
            acts.push((f, Act::Down(x, y)));
            acts.push((f + tap_frames, Act::Up));
            fingers.push((f, f + tap_frames, what));
            Ok(())
        };
        if s.autoplay {
            tap(size[0] as f64 / 2.0, size[1] as f64 / 2.0, shots.first().map_or(AUTOPLAY_SECS, |s| s.1.min(AUTOPLAY_SECS)), "--autoplay".into())?;
        }
        for spec in &s.tap {
            let (p, t, dur) = parse_timed(spec, "tap", 2..=2, "X,Y@T")?;
            if dur.is_some() {
                return Err(format!("--tap {spec}: a tap has no duration (a --swipe that doesn't move is a hold)"));
            }
            tap(p[0], p[1], t, format!("--tap {spec}"))?;
        }
        for spec in &s.swipe {
            let (p, t, dur) = parse_timed(spec, "swipe", 4..=4, "X1,Y1,X2,Y2@T[:SECS]")?;
            if !in_bounds(p[0], p[1]) || !in_bounds(p[2], p[3]) {
                return Err(format!("--swipe {spec}: outside the {}×{} screen", size[0], size[1]));
            }
            let f = frame_of(t, hz);
            let k = frame_of(dur.unwrap_or(SWIPE_SECS), hz).max(1);
            acts.push((f, Act::Down(p[0], p[1])));
            for i in 1..=k {
                let u = i as f64 / k as f64;
                acts.push((f + i, Act::Move(p[0] + (p[2] - p[0]) * u, p[1] + (p[3] - p[1]) * u)));
            }
            acts.push((f + k, Act::Up));
            fingers.push((f, f + k, format!("--swipe {spec}")));
        }
        // Tilts (or aims) in time order, each turning from where the last one left the phone.
        if !s.tilt.is_empty() && !s.aim.is_empty() {
            return Err("--tilt and --aim both turn the phone: use one or the other".into());
        }
        let (tilt_all, tilt_frames) = plan_turns(&s.tilt, "tilt", "ROLL[,PITCH]@T[:SECS]", 90.0, "-20@3 or -20@3:0.4", hz)?;
        let (aim_all, aim_frames) = plan_turns(&s.aim, "aim", "YAW[,PITCH]@T[:SECS]", 80.0, "15,-5@2 or 15,-5@2:0.3", hz)?;
        let tilt = |v: [f64; 4]| Act::Tilt(v[0], v[1]);
        let aim = |v: [f64; 4]| Act::Aim(v[0], v[1], v[2], v[3]);
        let tilts: Vec<(f64, Act)> = tilt_all.into_iter().map(|(t, v)| (t, tilt(v))).chain(aim_all.into_iter().map(|(t, v)| (t, aim(v)))).collect();
        let turning: Vec<(u32, Act)> = tilt_frames.into_iter().map(|(f, v)| (f, tilt(v))).chain(aim_frames.into_iter().map(|(f, v)| (f, aim(v)))).collect();
        acts.extend(turning);
        fingers.sort_by_key(|f| f.0);
        for w in fingers.windows(2) {
            if w[1].0 < w[0].1 {
                return Err(format!("{} starts while {} is still touching: one finger at a time", w[1].2, w[0].2));
            }
        }
        acts.sort_by_key(|(f, a)| (*f, a.rank()));
        let mark_wait = if marks.is_empty() { 0 } else { frame_of(wait, hz).max(1) };
        Ok(Plan { shots, marks, mark_wait, acts, tilts })
    }

    /// What a live run sends when, at game times (s): touches at their frame
    /// boundaries' times, tilt samples at their own.
    pub(crate) fn timed(&self, hz: f64) -> Vec<(f64, Act)> {
        let touches = self.acts.iter().filter(|a| !matches!(a.1, Act::Tilt(..) | Act::Aim(..))).map(|&(f, a)| (f as f64 / hz, a));
        let mut v: Vec<(f64, Act)> = touches.chain(self.tilts.iter().copied()).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.rank().cmp(&b.1.rank())));
        v
    }

    /// Refuses a --tilt for a game without the tilt sensor, and an --aim without attitude.
    pub(crate) fn check_sensors(&self, m: &Manifest) -> Result<(), String> {
        use mb_format::manifest::Sensor;
        if self.acts.iter().any(|(_, x)| matches!(x, Act::Tilt(..))) && !m.sensors.contains(&Sensor::Tilt) {
            return Err("--tilt: this game doesn't declare the tilt sensor (manifest.toml sensors)".into());
        }
        if self.acts.iter().any(|(_, x)| matches!(x, Act::Aim(..))) && !m.sensors.contains(&Sensor::Attitude) {
            return Err("--aim: this game doesn't declare the attitude sensor (manifest.toml sensors)".into());
        }
        Ok(())
    }

    /// The frame boundaries where something happens, in order.
    pub(crate) fn boundaries(&self) -> Vec<u32> {
        // With marks to wait for, the last shot's frame isn't known yet: every act may run.
        let last = if self.marks.is_empty() { self.shots.last().map_or(0, |s| s.0) } else { u32::MAX };
        let mut b: Vec<u32> = self.shots.iter().map(|s| s.0).chain(self.acts.iter().map(|a| a.0).filter(|&f| f < last)).collect();
        b.sort_unstable();
        b.dedup();
        b
    }
}

/// `1.5` → "1.5", `2.0` → "2": how a time appears in file names and labels.
fn secs(t: f64) -> String {
    format!("{t}")
}

pub fn run(mut a: ShotArgs) -> Result<(), String> {
    let started = Instant::now();
    if !a.no_grounding {
        // The SDK's ground::Figure reports where it stands while this is set (grounding.rs).
        a.script.param.push(format!("{}=1", grounding::PARAM));
        if a.taps {
            a.script.param.push(format!("{}=1", grounding::TAPS_PARAM));
        }
    }
    let is_dir = a.path.is_dir();
    if a.name.is_empty() || a.name.starts_with('.') || !a.name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) {
        return Err(format!("--name {:?}: use letters, digits, - _ .", a.name));
    }
    let mut manifest = game_manifest(&a.path)?;
    if a.landscape {
        if manifest.orientation != mb_format::manifest::Orientation::Any {
            return Err(format!("--landscape is for orientation = \"any\" games; this one is {}", manifest.orientation.as_str()));
        }
        manifest.logical_size.swap(0, 1);
    }
    if a.portrait {
        if manifest.orientation != mb_format::manifest::Orientation::Landscape {
            return Err(format!("--portrait is for orientation = \"landscape\" games (their feed card); this one is {}", manifest.orientation.as_str()));
        }
        manifest.logical_size.swap(0, 1);
    }
    if let Some(spec) = &a.script.screen {
        eprintln!("{}", fit_screen(&mut manifest, spec)?);
    }
    let size = manifest.logical_size;
    let plan = Plan::shoot(&a.script, &a.at, a.mark_wait, size)?;
    plan.check_sensors(&manifest)?;
    if a.smoothness && !HZ_RANGE.contains(&(a.script.hz * 2.0)) {
        return Err(format!("--smoothness runs the game again at twice --hz: --hz up to {}", HZ_RANGE.end() / 2.0));
    }
    let chrome_exe = cdp::find_chrome(a.chrome.as_deref())?;
    let out = match &a.out {
        Some(o) => o.clone(),
        None if is_dir => a.path.join("screenshots"),
        None => std::env::current_dir().map_err(|e| e.to_string())?.join("screenshots"),
    };

    let work = std::env::temp_dir().join(format!("mb-shot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| {
        let site = work.join("site");
        stage(&a.path, &site, a.script.seed)?;
        let built = started.elapsed();
        let port = serve::serve_site(site, a.port.unwrap_or(0))?;
        let mut q = Vec::new();
        if a.overlays {
            q.push("overlays");
        }
        if a.stats.is_some() {
            q.push("stats");
        }
        let url = format!("http://127.0.0.1:{port}/runtime/index.html{}{}", if q.is_empty() { "" } else { "?" }, q.join("&"));
        GAME_LOGS_LISTED.store(true, std::sync::atomic::Ordering::Relaxed);
        let (shots, smoothness, logs) = capture(&chrome_exe, &url, &manifest, &plan, &a, &out)?;
        let sheet = if a.sheet { Some(write_sheet(&shots, size, &out, &a.name)?) } else { None };
        if let Some(p) = &sheet {
            let txt = p.with_extension("txt");
            std::fs::write(&txt, sheet_text(&manifest.name, &shots, &logs)).map_err(|e| format!("{}: {e}", txt.display()))?;
        }
        if !logs.is_empty() {
            eprintln!("game log ({} line{}):", logs.len(), if logs.len() == 1 { "" } else { "s" });
            // --logs-match picked these; otherwise the first few, and where the rest are.
            let shown = if a.logs_match.is_some() { logs.len() } else { logs.len().min(LOG_LINES) };
            for l in &logs[..shown] {
                eprintln!("  {l}");
            }
            if shown < logs.len() {
                let rest = match &sheet {
                    Some(p) => format!("in {}", p.with_extension("txt").display()),
                    None => "(--sheet lists them all; --logs-match <text> picks lines)".into(),
                };
                eprintln!("  … {} more {rest}", logs.len() - shown);
            }
        }
        for s in &shots {
            println!("{}", s.path.display());
        }
        if let Some(p) = sheet {
            println!("{}", p.display());
        }
        if let Some(r) = smoothness {
            eprint!("{}", r.text());
        }
        let n = shots.len();
        eprintln!("{n} shot{} in {:.1}s (build {:.1}s)", if n == 1 { "" } else { "s" }, started.elapsed().as_secs_f64(), built.as_secs_f64());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// The contact sheet's sidecar: what each tile is, and the game's log lines.
fn sheet_text(game: &str, shots: &[Shot], logs: &[String]) -> String {
    let mut out = format!("{game}: contact sheet\n");
    for s in shots {
        let mark = s.mark.as_ref().map_or(String::new(), |m| format!("  mark:{m}"));
        out += &format!("{:>6}s  frame {}  {}{mark}\n", secs(s.t), s.frame, s.path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned()));
    }
    out += &format!("\ngame log ({} lines):\n", logs.len());
    for l in logs {
        out += &format!("  {l}\n");
    }
    out
}

/// `--screen`'s phones, upright, in points.
const SCREENS: &[(&str, [u32; 2])] = &[("se", [375, 667]), ("mini", [375, 812]), ("iphone", [393, 852]), ("pro-max", [440, 956])];
/// How far a `fill` game's Screen grows, long side over short (runtime/src/main.ts `MAX_FILL_ASPECT`).
const MAX_FILL_ASPECT: f64 = 2.4;

/// `--screen`'s value: a preset or WxH, in points.
fn parse_screen(spec: &str) -> Result<[u32; 2], String> {
    let s = spec.to_ascii_lowercase();
    if let Some((_, size)) = SCREENS.iter().find(|(n, _)| *n == s) {
        return Ok(*size);
    }
    let names = SCREENS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
    let bad = || format!("--screen {spec}: expected WxH in points (360x780) or one of {names}");
    let (w, h) = s.split_once(['x', '×']).ok_or_else(bad)?;
    let (w, h) = (w.trim().parse::<u32>().map_err(|_| bad())?, h.trim().parse::<u32>().map_err(|_| bad())?);
    if !(100..=4096).contains(&w) || !(100..=4096).contains(&h) {
        return Err(format!("--screen {spec}: each side is 100 to 4096 points"));
    }
    Ok([w, h])
}

/// `--screen`: the Screen the game gets on a phone of that size (SPEC §2 "Orientation and
/// fit", as runtime/src/main.ts `fit` works it out), which becomes the manifest's logical
/// size so the page is emulated at it, 2 device pixels per unit like any phone (a bigger
/// phone shows the same picture larger). Returns a line saying what the game gets there.
pub(crate) fn fit_screen(m: &mut Manifest, spec: &str) -> Result<String, String> {
    use mb_format::manifest::{Fit, Orientation};
    let [w, h] = parse_screen(spec)?;
    let sideways = w > h;
    let [mut lw, mut lh] = m.logical_size;
    match m.orientation {
        Orientation::Portrait if sideways => {
            return Err(format!("--screen {w}x{h} is a phone held sideways, and a portrait game is played upright: give it as {h}x{w}"));
        }
        // An `any` game turns with the phone; a landscape one upright is its feed card.
        Orientation::Any if sideways => std::mem::swap(&mut lw, &mut lh),
        Orientation::Landscape if !sideways => std::mem::swap(&mut lw, &mut lh),
        _ => {}
    }
    if m.fit != Fit::Fill {
        m.logical_size = [lw, lh];
        return Ok(format!("screen {w}x{h}: this game letterboxes (fit = \"letterbox\"), so it gets its own {lw}x{lh} there, with bars; shot at {lw}x{lh}"));
    }
    let scale = (w as f64 / lw as f64).min(h as f64 / lh as f64);
    let (mut gw, mut gh) = (w as f64 / scale, h as f64 / scale);
    let capped = gw / gh > MAX_FILL_ASPECT || gh / gw > MAX_FILL_ASPECT;
    if gw / gh > MAX_FILL_ASPECT {
        gw = gh * MAX_FILL_ASPECT;
    }
    if gh / gw > MAX_FILL_ASPECT {
        gh = gw * MAX_FILL_ASPECT;
    }
    // Whole units, never less than declared.
    let size = [(gw.round() as u32).clamp(lw, 4096), (gh.round() as u32).clamp(lh, 4096)];
    m.logical_size = size;
    let bars = if capped { format!(" (a fill game grows to {MAX_FILL_ASPECT} : 1 and no further: bars beyond)") } else { String::new() };
    Ok(format!("screen {w}x{h}: the game's Screen is {}x{}{bars}", size[0], size[1]))
}

/// The manifest of a game directory or .mbx.
pub(crate) fn game_manifest(path: &Path) -> Result<Manifest, String> {
    if crate::studio::is_project(path) {
        crate::studio::manifest(path, Vec::new())
    } else if path.is_dir() {
        devbuild::manifest(path).map_err(|f| f.text)
    } else if path.is_file() {
        Ok(open_bundle(path)?.manifest)
    } else {
        Err(format!("{}: no such game directory or bundle", path.display()))
    }
}

/// Builds a game directory (or opens a .mbx) and lays it out with the runtime under `site`.
pub(crate) fn stage(path: &Path, site: &Path, seed: u64) -> Result<(), String> {
    stage_with(path, site, seed, &[])
}

/// `stage`, a game directory built with extra cargo `--config` settings.
pub(crate) fn stage_with(path: &Path, site: &Path, seed: u64, config: &[String]) -> Result<(), String> {
    if crate::studio::is_project(path) {
        stage_studio(path, site, seed)
    } else if path.is_dir() {
        stage_dir(path, site, seed, config)
    } else {
        stage_bundle(path, site, seed)
    }
}

/// A Studio project, laid out as its bundle would be.
fn stage_studio(dir: &Path, site: &Path, seed: u64) -> Result<(), String> {
    let (manifest, files) = crate::studio::bundle_files(dir, None)?;
    let game = site.join("game");
    for (p, data) in &files {
        let dest = game.join(p);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        let data = if p == "game.wasm" { mb_format::meter::instrument(data)? } else { data.clone() };
        std::fs::write(&dest, data).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    std::fs::write(game.join("boot.json"), serde_json::to_vec_pretty(&crate::boot_json(&manifest, seed, DATE)).unwrap()).map_err(|e| e.to_string())?;
    serve::write_runtime(&site.join("runtime"))
}

fn open_bundle(path: &Path) -> Result<mb_format::Bundle, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    mb_format::Bundle::open(&bytes).map_err(|r| format!("{} is invalid: {}", path.display(), r.errors.join("; ")))
}

/// Builds the game in `dir` (the fast watch build) and lays it out with the
/// runtime under `site`, as `mb stage` does for a bundle.
fn stage_dir(dir: &Path, site: &Path, seed: u64, config: &[String]) -> Result<(), String> {
    // Held until the wasm and assets are copied out: parallel runs on this game take turns.
    let _lock = devbuild::lock(dir);
    fonts::rebake(dir)?;
    let name = crate::package_name(dir)?;
    let built = devbuild::build_with(dir, &name, config).map_err(|f| format!("build failed: {}", f.summary()))?;
    let game = site.join("game");
    std::fs::create_dir_all(&game).map_err(|e| e.to_string())?;
    let write = |rel: &str, data: &[u8]| -> Result<(), String> {
        let dest = game.join(rel);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&dest, data).map_err(|e| format!("{}: {e}", dest.display()))
    };
    write("game.wasm", &built.wasm)?;
    write("manifest.toml", &std::fs::read(dir.join("manifest.toml")).map_err(|e| e.to_string())?)?;
    // The files a bundle would carry under assets/.
    let mut stack = vec![dir.join("assets")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| ASSET_EXTENSIONS.contains(&x.to_ascii_lowercase().as_str())) {
                let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                write(&rel, &std::fs::read(&p).map_err(|e| e.to_string())?)?;
            }
        }
    }
    write("boot.json", &serde_json::to_vec_pretty(&crate::boot_json(&built.manifest, seed, DATE)).unwrap())?;
    serve::write_runtime(&site.join("runtime"))
}

/// Lays out a .mbx with the runtime under `site` (what `mb stage` does).
fn stage_bundle(path: &Path, site: &Path, seed: u64) -> Result<(), String> {
    let b = open_bundle(path)?;
    let game = site.join("game");
    for p in b.paths() {
        let dest = game.join(p);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        // The metered wasm, as the client serves it.
        let data = if p == "game.wasm" { b.runtime_wasm() } else { b.file(p).unwrap() };
        std::fs::write(&dest, data).map_err(|e| e.to_string())?;
    }
    std::fs::write(game.join("boot.json"), serde_json::to_vec_pretty(&crate::boot_json(&b.manifest, seed, DATE)).unwrap()).map_err(|e| e.to_string())?;
    serve::write_runtime(&site.join("runtime"))
}

struct Shot {
    t: f64,
    /// The `--at mark:…` it was taken for.
    mark: Option<String>,
    path: PathBuf,
    frame: u32,
    png: Vec<u8>,
}

/// Opens the staged game at `url` in a headless Chrome emulating a phone and
/// waits until it's running. Returns the browser and the page's session id.
pub(crate) fn open_page(exe: &Path, url: &str, m: &Manifest) -> Result<(Chrome, String), String> {
    let [w, h] = m.logical_size;
    let mut chrome = Chrome::launch(exe, w, h)?;
    let sid = open_tab(&mut chrome, url, m)?;
    Ok((chrome, sid))
}

/// Opens another page in `chrome` as `open_page` does; returns its session id.
pub(crate) fn open_tab(chrome: &mut Chrome, url: &str, m: &Manifest) -> Result<String, String> {
    let [w, h] = m.logical_size;
    let target = chrome.call(None, "Target.createTarget", json!({ "url": "about:blank" }))?;
    let attached = chrome.call(None, "Target.attachToTarget", json!({ "targetId": target["targetId"], "flatten": true }))?;
    let sid = attached["sessionId"].as_str().ok_or("Target.attachToTarget: no session")?.to_string();
    let s = Some(sid.as_str());
    chrome.call(s, "Runtime.enable", json!({}))?;
    chrome.call(s, "Page.enable", json!({}))?;
    // A phone: the game's logical size, 2 device pixels per unit, touch.
    chrome.call(s, "Emulation.setDeviceMetricsOverride", json!({ "width": w, "height": h, "deviceScaleFactor": 2, "mobile": true }))?;
    chrome.call(s, "Emulation.setTouchEmulationEnabled", json!({ "enabled": true, "maxTouchPoints": 5 }))?;
    chrome.call(s, "Page.navigate", json!({ "url": url }))?;

    // Wait for the runtime's "[mb] ready" (or its error) on the console.
    let deadline = Instant::now() + BOOT_TIMEOUT;
    loop {
        let Some(ev) = chrome.event(deadline)? else {
            return Err(format!("the game didn't start within {} s", BOOT_TIMEOUT.as_secs()));
        };
        // Another page's (`mb measure --ab` opens two).
        if ev["sessionId"].as_str().is_some_and(|s| s != sid) {
            continue;
        }
        match page_message(&ev) {
            Some(Page::Ready(info)) => {
                if let Some(gpu) = info["adapter"].as_str().filter(|s| !s.is_empty()) {
                    eprintln!("rendering with {gpu}");
                }
                break;
            }
            Some(Page::Error(e)) if e.contains("adapter") => {
                return Err(format!("the game failed to start: {e}\n(this browser has no WebGPU here: on Linux it needs a GPU with Vulkan; or try another with --chrome)"));
            }
            Some(Page::Error(e)) => return Err(format!("the game failed to start: {e}")),
            Some(Page::Log(l)) => page_log(&l),
            None => {}
        }
    }
    Ok(sid)
}

/// Runs the plan in a headless Chrome and saves a PNG per shot. With
/// --smoothness, runs it again at twice the frame rate and compares how the
/// camera moved.
#[allow(clippy::type_complexity)]
fn capture(exe: &Path, url: &str, m: &Manifest, plan: &Plan, a: &ShotArgs, out: &Path) -> Result<(Vec<Shot>, Option<smooth::Report>, Vec<String>), String> {
    let (mut chrome, sid) = open_page(exe, url, m)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut shots = Vec::new();
    // GPU-paced stepping only buys --stats its GPU times: a shot's pixels are the same either way.
    let paced = a.stats.is_some();
    let (poses, timings) = play(&mut chrome, &sid, m, plan, &a.script, a.track_node, paced, |chrome, b, t, mark, marked_subject, ground| {
        let png = screenshot(chrome, &sid)?;
        let file = match mark {
            Some(m) => format!("{}-{}.png", a.name, file_safe(m)),
            None => format!("{}-{}s.png", a.name, secs(t)),
        };
        let path = out.join(file);
        std::fs::write(&path, &png).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut line = format!("{:>6}s  frame {b}", secs(t));
        if let Some(m) = mark {
            line.push_str(&format!("  mark:{m}"));
        }
        if let Ok(img) = Image::decode(&png)
            && let Some(c) = img.px.first().filter(|c| img.px.iter().all(|p| p == *c))
        {
            line.push_str(&format!("  warning: the whole frame is #{:02x}{:02x}{:02x} (nothing drawn?)", c[0], c[1], c[2]));
        }
        eprintln!("{line}");
        // The frames since the last shot (cleared after each), so a summary is about this stretch.
        let st = eval(chrome, &sid, "mb.stats({ reset: true })")?;
        match a.stats {
            Some(StatsFormat::Line) => eprintln!("         {}", stats_line(&st)),
            Some(StatsFormat::Json) => eprintln!("         stats {st}"),
            None => {}
        }
        let frames = st["frames"].as_array().cloned().unwrap_or_default();
        if let Some(w) = frames.last().and_then(|last| budget::warning(last, &frames, &st["budgets"])) {
            eprintln!("         {}", w.replace('\n', "\n         "));
        }
        // Stepping waits for what the game asks for (`mb.step`): this one didn't come in time.
        for p in st["loading"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            eprintln!("         warning: {p} not loaded at {} s", secs(t));
        }
        if a.readability {
            let subject = a.subject.or(marked_subject);
            // A frame without mb3d is all HUD: measure it as shown, with no subject.
            let three_d = st["triangles"].is_number();
            for l in check_readability(chrome, &sid, &png, subject.filter(|_| three_d), three_d, st["retro"].is_object(), &path)? {
                eprintln!("         {l}");
            }
        }
        if !a.no_grounding
            && let Ok(img) = Image::decode(&png)
        {
            let (lines, warnings) = grounding::check(ground, &img, m.logical_size);
            if a.grounding || a.taps {
                for l in lines.into_iter().filter(|l| a.grounding || l.starts_with("tap area")) {
                    eprintln!("         {l}");
                }
            }
            for w in warnings {
                eprintln!("         warning: {w}");
            }
        }
        shots.push(Shot { t, mark: mark.map(str::to_string), path, frame: b, png });
        Ok(())
    })?;
    let logs = game_logs(&timings, a.script.hz, a.logs.unwrap_or(LogLevel::Info), a.logs_match.as_deref());
    if !a.smoothness {
        return Ok((shots, None, logs));
    }
    let twice = Script { hz: a.script.hz * 2.0, ..a.script.clone() };
    let (again, _) = play(&mut chrome, &sid, m, &Plan::shoot(&twice, &a.at, a.mark_wait, m.logical_size)?, &twice, a.track_node, paced, |_, _, _, _, _, _| Ok(()))?;
    let lo = smooth::Run { hz: a.script.hz, poses };
    Ok((shots, Some(smooth::check(&lo, &smooth::Run { hz: twice.hz, poses: again }, a.track_node)), logs))
}

/// Runs `plan` in a fresh held session, `script.hz` frames a second, calling
/// `shot` after each shot's frame (with the frame, its game time, the
/// `--at mark:…` it's for, and the subject the game last marked). Returns
/// every frame's pose, and `mb.step`'s timings they came from.
fn play(
    chrome: &mut Chrome,
    sid: &str,
    m: &Manifest,
    plan: &Plan,
    script: &Script,
    track: Option<u32>,
    paced: bool,
    mut shot: impl FnMut(&mut Chrome, u32, f64, Option<&str>, Option<u32>, &grounding::Ground) -> Result<(), String>,
) -> Result<(Vec<smooth::Pose>, Vec<Value>), String> {
    let hz = script.hz;
    let mut player = Player::start(chrome, sid, m, plan, script, track)?;
    player.paced = paced;
    // Shots whose frame is known: (frame, time, mark), taken in frame order.
    let mut due: Vec<(u32, f64, Option<String>)> = plan.shots.iter().map(|&(b, t)| (b, t, None)).collect();
    let mut waiting: Vec<MarkShot> = plan.marks.clone();
    loop {
        due.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
        let next = due.first().map(|d| d.0);
        let target = match (next, waiting.is_empty()) {
            (Some(b), true) => b,
            (Some(b), false) => b.min(plan.mark_wait),
            (None, false) => plan.mark_wait,
            (None, true) => break,
        };
        let labels: Vec<String> = waiting.iter().map(|w| w.label.clone()).collect();
        if let Some(label) = player.run_until(chrome, sid, target, &labels)? {
            let at = player.now;
            for w in waiting.iter().filter(|w| w.label == label) {
                due.push((at + frame_of(w.after, hz), at as f64 / hz + w.after, Some(w.name())));
            }
            waiting.retain(|w| w.label != label);
            continue;
        }
        let now = player.now;
        while let Some(i) = due.iter().position(|d| d.0 <= now) {
            let (b, t, mark) = due.remove(i);
            // A mark's shot is labelled with its game time, to the millisecond.
            let t = if mark.is_some() { (now as f64 / hz * 1000.0).round() / 1000.0 } else { t };
            shot(chrome, b.max(now), t, mark.as_deref(), readability::marked_subject(&player.timings), &grounding::of_last_frame(&player.timings))?;
        }
        if now >= plan.mark_wait && !waiting.is_empty() {
            return Err(missing_marks(&waiting, &player.timings, plan.mark_wait as f64 / hz));
        }
    }
    Ok((player.poses, player.timings))
}

/// The page as a PNG.
fn screenshot(chrome: &mut Chrome, sid: &str) -> Result<Vec<u8>, String> {
    let r = chrome.call(Some(sid), "Page.captureScreenshot", json!({ "format": "png" }))?;
    base64::engine::general_purpose::STANDARD.decode(r["data"].as_str().unwrap_or("")).map_err(|e| format!("screenshot: {e}"))
}

/// `--readability` for the shot just taken (`png`, saved at `path`): draws
/// the frame again without the HUD, then plain with and without the subject
/// (its silhouette), measures, and writes the greyscale picture. Returns the
/// lines to print.
fn check_readability(chrome: &mut Chrome, sid: &str, png: &[u8], subject: Option<u32>, three_d: bool, retro: bool, path: &Path) -> Result<Vec<String>, String> {
    let mut redraw = |node: u32, plain: bool| -> Result<Image, String> {
        eval(chrome, sid, &format!("mb.isolate({node}, {{ hud: false, plain: {plain} }})"))?;
        settle(chrome, sid)?;
        Image::decode(&screenshot(chrome, sid)?)
    };
    let clean = if three_d { redraw(0, false)? } else { Image::decode(png)? };
    let pair = match subject {
        Some(n) => Some((redraw(0, true)?, redraw(n, true)?)),
        None => None,
    };
    // Back to the frame as it was, for anything that looks at the page next.
    eval(chrome, sid, "mb.isolate(0)")?;
    let shown = Image::decode(png)?;
    let mask = pair.as_ref().map(|(a, b)| (a, b));
    let r = readability::measure(&shown, &clean, mask, 2);
    let pic = path.with_extension("readability.png");
    std::fs::write(&pic, readability::picture(&clean, mask).encode_png()).map_err(|e| format!("{}: {e}", pic.display()))?;
    let mut lines = vec![match (subject, &r.subject) {
        (Some(n), None) => format!("{} (subject node {n} drew nothing in this frame: off screen, hidden or gone?)", r.line().split(" (no subject").next().unwrap_or_default()),
        _ => r.line(),
    }];
    lines.extend(r.warnings(retro).into_iter().map(|w| format!("warning: {w}")));
    Ok(lines)
}

/// Why `--at mark:…` shots weren't taken: the labels, and what the game did mark.
fn missing_marks(waiting: &[MarkShot], timings: &[Value], secs_waited: f64) -> String {
    let mut seen: Vec<String> = Vec::new();
    for t in timings {
        for m in t["marks"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|m| !grounding::is_ground(m)) {
            if !seen.iter().any(|s| s == m) {
                seen.push(m.to_string());
            }
        }
    }
    let labels: Vec<String> = waiting.iter().map(|w| format!("\"{}\"", w.label)).collect();
    let marked = if seen.is_empty() { "nothing".to_string() } else { seen.iter().take(12).map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ") };
    format!(
        "--at mark:…: the game didn't mark {} in {} s of play (it marked {marked}). Does the moment need input the script doesn't give (--tap, --autoplay, --param)? --mark-wait waits longer.",
        labels.join(" or "),
        secs(secs_waited)
    )
}

/// A mark's label as part of a file name.
fn file_safe(label: &str) -> String {
    label.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.+".contains(c) { c } else { '_' }).collect()
}

/// A plan being played in a fresh held session, a stretch at a time, so a
/// caller can stop at each shot's frame (and keep several pages in step).
pub(crate) struct Player<'p> {
    plan: &'p Plan,
    hz: f64,
    /// Frame boundaries where something happens, and the next one to reach.
    bounds: Vec<u32>,
    next: usize,
    /// Frames run so far.
    pub(crate) now: u32,
    /// The boundary stopped at, whose input is sent when play goes on.
    unsent: Option<u32>,
    /// Every frame's pose, and its timings as `mb.step` gave them.
    pub(crate) poses: Vec<smooth::Pose>,
    pub(crate) timings: Vec<Value>,
    /// Whether each step waits for the GPU (and times it): `mb.step`'s `paced`.
    pub(crate) paced: bool,
}

impl<'p> Player<'p> {
    /// Replaces the page's session with a held one at frame 0 (`script`'s seed).
    pub(crate) fn start(chrome: &mut Chrome, sid: &str, m: &Manifest, plan: &'p Plan, script: &Script, track: Option<u32>) -> Result<Player<'p>, String> {
        eval(chrome, sid, &restart_held(script)?)?;
        check_page(chrome)?;
        // Not the frames the page ran live before the held session.
        eval(chrome, sid, "void mb.stats({ reset: true })")?;
        if has_motion_sensors(m) {
            // Upright and still until a --tilt: touches move the pointer, which would tilt the phone.
            eval(chrome, sid, &pin_tilt(0.0, 0.0))?;
        }
        if let Some(n) = track {
            eval(chrome, sid, &format!("mb.track({n})"))?;
        }
        Ok(Player { plan, hz: script.hz, bounds: plan.boundaries(), next: 0, now: 0, unsent: None, poses: Vec::new(), timings: Vec::new(), paced: true })
    }

    /// Plays on until `target` frames have run (a shot's frame), sending the
    /// input of every boundary before it; that of `target` is sent by the next call.
    pub(crate) fn run_to(&mut self, chrome: &mut Chrome, sid: &str, target: u32) -> Result<(), String> {
        self.run_until(chrome, sid, target, &[]).map(|_| ())
    }

    /// `run_to`, but stops early after the first frame the game marks with
    /// one of `labels` (`sys::mark`), and returns that label; `self.now` is
    /// then that frame. Input planned for a frame stopped at is sent by the next call.
    pub(crate) fn run_until(&mut self, chrome: &mut Chrome, sid: &str, target: u32, labels: &[String]) -> Result<Option<String>, String> {
        if let Some(b) = self.unsent.take() {
            self.send(chrome, sid, b)?;
        }
        while let Some(&b) = self.bounds.get(self.next).filter(|&&b| b <= target) {
            let hit = self.step_to(chrome, sid, b, labels)?;
            if self.now == b {
                self.next += 1;
                if b == target || hit.is_some() {
                    self.unsent = Some(b);
                    return Ok(hit);
                }
                self.send(chrome, sid, b)?;
            } else {
                return Ok(hit);
            }
        }
        self.step_to(chrome, sid, target, labels)
    }

    fn step_to(&mut self, chrome: &mut Chrome, sid: &str, b: u32, labels: &[String]) -> Result<Option<String>, String> {
        if b <= self.now {
            return Ok(None);
        }
        let until = if labels.is_empty() { String::new() } else { format!(", until: {}", serde_json::to_string(labels).unwrap()) };
        let r = eval(chrome, sid, &format!("mb.step({}, {{ dt: 1 / {}, paced: {}{until} }})", b - self.now, self.hz, self.paced))?;
        let timings = r["timings"].as_array().cloned().unwrap_or_default();
        let hit = timings.last().and_then(|t| marked(t, labels));
        self.poses.extend(timings.iter().filter_map(smooth::Pose::from_timing));
        self.timings.extend(timings);
        let ran = r["frames"].as_u64().unwrap_or(0);
        settle(chrome, sid)?;
        check_page(chrome)?;
        if hit.is_some() && ran <= b as u64 {
            self.now = ran as u32;
            return Ok(hit);
        }
        if ran != b as u64 {
            return Err(format!("the game stopped at frame {ran} of {b}"));
        }
        self.now = b;
        Ok(None)
    }

    /// Sends the input planned at boundary `b`.
    fn send(&mut self, chrome: &mut Chrome, sid: &str, b: u32) -> Result<(), String> {
        let mut touched = false;
        for (_, act) in self.plan.acts.iter().filter(|x| x.0 == b) {
            touched |= send_act(chrome, sid, *act)?;
        }
        if touched {
            settle(chrome, sid)?;
        }
        Ok(())
    }
}

/// The first of `labels` a frame's timing (`mb.step`) lists among its marks.
fn marked(timing: &Value, labels: &[String]) -> Option<String> {
    let marks = timing["marks"].as_array()?;
    marks.iter().filter_map(Value::as_str).find(|m| labels.iter().any(|l| l == m)).map(str::to_string)
}

/// The console call that replaces the page's session with a held one with
/// the script's seed and debug parameters.
pub(crate) fn restart_held(script: &Script) -> Result<String, String> {
    let opts = json!({ "hold": true, "seed": script.seed.to_string(), "params": params(&script.param)? });
    Ok(format!("mb.restart({})", serde_json::to_string(&opts.to_string()).unwrap()))
}

/// `--param name=value`s as a JSON object, checked as the runtime takes them
/// (names of letters, digits, `_ - .` up to 64 bytes, values up to 256, at most 32).
pub(crate) fn params(specs: &[String]) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for spec in specs {
        let (k, v) = spec.split_once('=').ok_or_else(|| format!("--param {spec}: expected NAME=VALUE, e.g. --param zone=3"))?;
        if k.is_empty() || k.len() > 64 || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)) {
            return Err(format!("--param {spec}: a name is letters, digits, _ - . (up to 64)"));
        }
        if v.len() > 256 {
            return Err(format!("--param {k}: a value is up to 256 bytes"));
        }
        out.insert(k.to_string(), Value::String(v.to_string()));
    }
    if out.len() > 32 {
        return Err("--param: at most 32".into());
    }
    Ok(Value::Object(out))
}

/// Sends one scripted input to the page: a touch event, or a pinned tilt.
/// Returns whether it was a touch.
pub(crate) fn send_act(chrome: &mut Chrome, sid: &str, act: Act) -> Result<bool, String> {
    let (kind, points) = match act {
        Act::Down(x, y) => ("touchStart", json!([{ "x": x, "y": y, "id": 0 }])),
        Act::Move(x, y) => ("touchMove", json!([{ "x": x, "y": y, "id": 0 }])),
        Act::Up => ("touchEnd", json!([])),
        Act::Tilt(roll, pitch) => {
            eval(chrome, sid, &pin_tilt(roll, pitch))?;
            return Ok(false);
        }
        Act::Aim(yaw, pitch, dyaw, dpitch) => {
            eval(chrome, sid, &pin_aim(yaw, pitch, dyaw, dpitch))?;
            return Ok(false);
        }
    };
    chrome.call(Some(sid), "Input.dispatchTouchEvent", json!({ "type": kind, "touchPoints": points }))?;
    Ok(true)
}

/// Whether the game reads tilt, motion or attitude, which the preview emulates from the pointer.
pub(crate) fn has_motion_sensors(m: &Manifest) -> bool {
    use mb_format::manifest::Sensor;
    m.sensors.iter().any(|s| matches!(s, Sensor::Tilt | Sensor::Motion | Sensor::Attitude))
}

type Quat = [f64; 4];

fn qmul(a: Quat, b: Quat) -> Quat {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// The phone `yaw` right, `pitch` up (where its back points) and `roll` right
/// edge down from upright, in radians, as the attitude quaternion (SPEC §5.7,
/// the SDK's `Attitude::from_angles`).
fn pose(yaw: f64, pitch: f64, roll: f64) -> Quat {
    let axis = |x: f64, y: f64, z: f64, a: f64| [x * (a / 2.0).sin(), y * (a / 2.0).sin(), z * (a / 2.0).sin(), (a / 2.0).cos()];
    qmul(qmul(axis(0.0, -1.0, 0.0, yaw), axis(1.0, 0.0, 0.0, -pitch)), axis(0.0, 0.0, 1.0, roll))
}

/// The console call that pins a whole sample: gravity from the pose, no
/// acceleration, the attitude and its rotation rate (rad/s, phone axes).
fn pin_pose(q: Quat, rate: [f64; 3]) -> String {
    let [x, y, z, w] = q;
    let g = [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)];
    // No "-0" or rounding dust: the same pose prints the same call.
    let n = |v: f64| if v.abs() < 1e-12 { "0".to_string() } else { format!("{v}") };
    let v: Vec<String> = g.iter().map(|&v| n(v)).chain(["0".into(), "0".into(), "0".into(), "0".into()]).chain(q.iter().chain(&rate).map(|&v| n(v))).collect();
    format!("mb.sensors({})", v.join(", "))
}

/// The console call that holds the phone at `roll` and `pitch` degrees from
/// upright, still (and, for a game with attitude, facing where it started).
/// Setting a sample pins tilt, motion and attitude in the preview: the pointer
/// (and so a scripted touch, which moves it) no longer changes them.
pub(crate) fn pin_tilt(roll: f64, pitch: f64) -> String {
    pin_pose(pose(0.0, -pitch.to_radians(), roll.to_radians()), [0.0; 3])
}

/// The console call that points the phone `yaw` degrees right and `pitch` up
/// from where it started, turning at `dyaw` and `dpitch` degrees a second.
pub(crate) fn pin_aim(yaw: f64, pitch: f64, dyaw: f64, dpitch: f64) -> String {
    let q = pose(yaw.to_radians(), pitch.to_radians(), 0.0);
    // The rate a gyro reports: the turn over a tiny step, in the phone's axes.
    let h = 1e-4;
    let q1 = pose((yaw + dyaw * h).to_radians(), (pitch + dpitch * h).to_radians(), 0.0);
    let d = qmul([-q[0], -q[1], -q[2], q[3]], q1);
    pin_pose(q, [2.0 * d[0] / h, 2.0 * d[1] / h, 2.0 * d[2] / h])
}

/// `mb.stats()` in a line: what the last frame drew, and the p95 cost of the
/// frames it summarizes, e.g.
/// `57+1 draws  412.3k tris  0 reflected  28 casters  3 particles | p95 ms: update 0.12  render 0.03  present 0.50  gpu 2.10 (90 frames)`.
fn stats_line(st: &Value) -> String {
    let n = |k: &str| st[k].as_f64();
    let count = |v: f64| if v >= 10_000.0 { format!("{:.1}k", v / 1000.0) } else { format!("{v}") };
    let drew = match n("triangles") {
        // A frame that rendered mb3d.
        Some(tris) => format!(
            "{}+{} draws  {} tris  {} reflected  {} casters  {} particles",
            n("draws").unwrap_or(0.0),
            n("draws2d").unwrap_or(0.0),
            count(tris),
            n("reflected").unwrap_or(0.0),
            n("casters").unwrap_or(0.0),
            count(n("particles").unwrap_or(0.0))
        ),
        None => format!("2D: {} draws  {} quads", n("draws2d").unwrap_or(0.0), count(n("quads2d").unwrap_or(0.0))),
    };
    let p95 = |k: &str| st["summary"][k]["p95"].as_f64().map_or("–".to_string(), |v| format!("{v:.2}"));
    let frames = st["frames"].as_array().map_or(0, Vec::len);
    let textures = crate::budget::textures_of(st).map_or(String::new(), |t| format!(" | {t}"));
    format!(
        "{drew} | p95 ms: update {}  render {}  present {}  gpu {} ({frames} frames){textures}",
        p95("mb_update"),
        p95("mb_render"),
        p95("present"),
        p95("gpu")
    )
}

/// Lets the page run one animation frame: touch moves are delivered with
/// the next frame, and a stepped frame reaches the screen with it. Gives up
/// after 100 ms (a page that runs no animation frames).
pub(crate) fn settle(chrome: &mut Chrome, sid: &str) -> Result<(), String> {
    eval(chrome, sid, "new Promise((r) => { requestAnimationFrame(() => r()); setTimeout(r, 100); })").map(|_| ())
}

/// Evaluates `expr` in the page, awaiting a promise, and returns its value.
pub(crate) fn eval(chrome: &mut Chrome, sid: &str, expr: &str) -> Result<Value, String> {
    let r = chrome.call(Some(sid), "Runtime.evaluate", json!({ "expression": expr, "awaitPromise": true, "returnByValue": true }))?;
    if let Some(e) = r.get("exceptionDetails") {
        let msg = e["exception"]["description"].as_str().or(e["text"].as_str()).unwrap_or("exception");
        return Err(format!("{expr}: {msg}"));
    }
    Ok(r["result"]["value"].clone())
}

/// Fails with the page's error if it reported one; prints its log lines.
pub(crate) fn check_page(chrome: &mut Chrome) -> Result<(), String> {
    for ev in chrome.drain_events()? {
        match page_message(&ev) {
            Some(Page::Error(e)) => return Err(e),
            Some(Page::Log(l)) => page_log(&l),
            _ => {}
        }
    }
    Ok(())
}

/// Prints a log line the page put on its console, unless it's the game's
/// and `mb shot` lists those itself, with their frames.
fn page_log(l: &str) {
    if !(GAME_LOGS_LISTED.load(std::sync::atomic::Ordering::Relaxed) && !l.starts_with("runtime: ")) {
        eprintln!("page: {l}");
    }
}

enum Page {
    Ready(Value),
    Error(String),
    Log(String),
}

/// What a DevTools event says about the game: the runtime's `[mb] <op> <json>`
/// console lines (bridge.ts), uncaught exceptions, a crashed page.
fn page_message(ev: &Value) -> Option<Page> {
    let p = &ev["params"];
    match ev["method"].as_str()? {
        "Runtime.consoleAPICalled" => {
            let args = p["args"].as_array()?;
            let text = |i: usize| args.get(i).and_then(|a| a["value"].as_str());
            if text(0) != Some("[mb]") {
                return None;
            }
            let msg: Value = text(2).and_then(|j| serde_json::from_str(j).ok()).unwrap_or(Value::Null);
            match text(1)? {
                "ready" => Some(Page::Ready(msg["info"].clone())),
                "error" | "gpu-lost" => Some(Page::Error(msg["message"].as_str().unwrap_or("error").to_string())),
                "watchdog" => Some(Page::Error(format!("watchdog: {} ran {:.0} ms", msg["callback"].as_str().unwrap_or("?"), msg["ms"].as_f64().unwrap_or(0.0)))),
                // Warnings and errors (level 1 and 2) the game or runtime logged.
                "log" if msg["level"].as_u64().unwrap_or(0) >= 1 => Some(Page::Log(msg["msg"].as_str().unwrap_or("").to_string())),
                _ => None,
            }
        }
        "Runtime.exceptionThrown" => {
            let d = &p["exceptionDetails"];
            Some(Page::Error(d["exception"]["description"].as_str().or(d["text"].as_str()).unwrap_or("exception").to_string()))
        }
        "Inspector.targetCrashed" => Some(Page::Error("the page crashed".into())),
        _ => None,
    }
}

// --- the contact sheet ---------------------------------------------------------

/// Gap between tiles, and the label strip above each, in sheet pixels.
const GAP: u32 = 16;
const LABEL: u32 = 34;
/// Sheets are kept about this wide (more shots wrap to more rows).
const SHEET_WIDTH: u32 = 1800;

/// Every shot at the game's logical size (how big it is on a phone), side
/// by side, each labelled with its time.
fn write_sheet(shots: &[Shot], size: [u32; 2], out: &Path, name: &str) -> Result<PathBuf, String> {
    let [w, h] = size;
    let cols = (SHEET_WIDTH / (w + GAP)).clamp(1, shots.len().max(1) as u32);
    let rows = (shots.len() as u32).div_ceil(cols);
    let mut sheet = Image::new(GAP + cols * (w + GAP), GAP + rows * (h + LABEL + GAP), [24, 24, 28, 255]);
    for (i, s) in shots.iter().enumerate() {
        let (x, y) = (GAP + (i as u32 % cols) * (w + GAP), GAP + (i as u32 / cols) * (h + LABEL + GAP));
        draw_text(&mut sheet, &format!("{}s", secs(s.t)), x, y + 4, 3, [235, 235, 240, 255]);
        let img = Image::decode(&s.png)?;
        sheet.blit(&img.resize(w, h), x, y + LABEL);
    }
    let path = out.join(format!("{name}-sheet.png"));
    std::fs::write(&path, encode_rgb(&sheet)?).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A plain RGB PNG at the default compression: the sheet is for looking at,
/// so it's written quickly rather than as small as possible.
fn encode_rgb(img: &Image) -> Result<Vec<u8>, String> {
    let data: Vec<u8> = img.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.w, img.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(&data)).map_err(|e| format!("sheet: {e}"))?;
    Ok(out)
}

/// 5×7 glyphs for times ("12.5s"), one row per byte, high bit on the left.
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        's' => [0, 0, 0x0e, 0x10, 0x0e, 0x01, 0x1e],
        _ => return None,
    })
}

/// Draws `text` with its top-left at x, y, each glyph pixel `k`×`k`.
fn draw_text(img: &mut Image, text: &str, x: u32, y: u32, k: u32, color: [u8; 4]) {
    for (i, c) in text.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        for (r, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) == 0 {
                    continue;
                }
                for dy in 0..k {
                    for dx in 0..k {
                        let (px, py) = (x + (i as u32 * 6 + col) * k + dx, y + r as u32 * k + dy);
                        if px < img.w && py < img.h {
                            img.set(px, py, color);
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
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        shot: ShotArgs,
    }

    fn plan(args: &[&str]) -> Result<Plan, String> {
        let cli = Cli::try_parse_from(std::iter::once("mb").chain(std::iter::once("game")).chain(args.iter().copied())).map_err(|e| e.to_string())?;
        Plan::shoot(&cli.shot.script, &cli.shot.at, cli.shot.mark_wait, [360, 640])
    }

    #[test]
    fn shots_can_wait_for_the_games_marks() {
        let p = plan(&["--at", "2,mark:boss,mark:boss+0.5,mark:break"]).unwrap();
        assert_eq!(p.shots, vec![(120, 2.0)]);
        let names: Vec<String> = p.marks.iter().map(MarkShot::name).collect();
        assert_eq!(names, ["boss", "boss+0.5", "break"]);
        assert_eq!(p.mark_wait, 3600, "60 s at 60 Hz by default");
        let p = plan(&["--at", "mark:boss", "--mark-wait", "5", "--tap", "10,10@30"]).unwrap();
        assert!(p.shots.is_empty());
        assert_eq!(p.mark_wait, 300);
        assert_eq!(p.boundaries(), vec![1800, 1804], "input after the last timed shot still runs while marks are awaited");
        assert!(plan(&["--at", "mark:"]).unwrap_err().contains("label"));
        assert!(plan(&["--at", "mark:boss+x"]).unwrap_err().contains("mark:LABEL+SECS"));
        assert!(plan(&["--at", "soon"]).unwrap_err().contains("mark:LABEL"));
        assert!(plan(&["--at", "mark:boss", "--mark-wait", "0"]).is_err());
        assert_eq!(file_safe("boss/arrives+0.5"), "boss_arrives+0.5");
    }

    #[test]
    fn a_frames_marks_are_matched_by_label() {
        let t = json!({ "frame": 12, "marks": ["jump", "boss"] });
        assert_eq!(marked(&t, &["boss".into()]), Some("boss".into()));
        assert_eq!(marked(&t, &["break".into()]), None);
        assert_eq!(marked(&json!({ "frame": 3 }), &["boss".into()]), None);
        let e = missing_marks(&[MarkShot { label: "boss".into(), after: 0.0 }], &[t], 60.0);
        assert!(e.contains("\"boss\"") && e.contains("\"jump\"") && e.contains("60 s"), "{e}");
    }

    #[test]
    fn shots_default_to_four_moments_in_order() {
        let p = plan(&[]).unwrap();
        assert_eq!(p.shots, vec![(30, 0.5), (120, 2.0), (300, 5.0), (600, 10.0)]);
        assert!(p.acts.is_empty());
        let p = plan(&["--at", "3,0,1", "--at", "1.004"]).unwrap();
        assert_eq!(p.shots, vec![(1, 0.0), (60, 1.0), (180, 3.0)], "sorted, frame 0 means frame 1, same frame once");
        assert!(plan(&["--at", "-1"]).is_err());
    }

    #[test]
    fn taps_last_a_few_frames() {
        let p = plan(&["--at", "1", "--tap", "180,500@0.5"]).unwrap();
        assert_eq!(p.acts, vec![(30, Act::Down(180.0, 500.0)), (34, Act::Up)]);
        assert_eq!(p.boundaries(), vec![30, 34, 60]);
    }

    #[test]
    fn autoplay_taps_the_middle_after_the_title_card() {
        let p = plan(&["--autoplay"]).unwrap();
        assert_eq!(p.acts[0], (30, Act::Down(180.0, 320.0)));
        assert_eq!(p.boundaries()[0], 30, "the title card is shot first, then tapped");
        let p = plan(&["--autoplay", "--at", "3,4"]).unwrap();
        assert_eq!(p.acts[0], (30, Act::Down(180.0, 320.0)), "at 0.5 s when the first shot is later");
        let p = plan(&["--autoplay", "--at", "0.2,4"]).unwrap();
        assert_eq!(p.acts[0], (12, Act::Down(180.0, 320.0)), "after an earlier first shot");
    }

    #[test]
    fn swipes_move_every_frame() {
        let p = plan(&["--at", "2", "--swipe", "0,100,60,100@1:0.05"]).unwrap();
        let want = vec![(60, Act::Down(0.0, 100.0)), (61, Act::Move(20.0, 100.0)), (62, Act::Move(40.0, 100.0)), (63, Act::Move(60.0, 100.0)), (63, Act::Up)];
        assert_eq!(p.acts, want, "the last move, then the lift");
        let p = plan(&["--at", "2", "--swipe", "0,100,60,100@1"]).unwrap();
        assert_eq!(p.acts.len(), 2 + frame_of(SWIPE_SECS, 60.0) as usize);
    }

    #[test]
    fn at_120_hz_everything_takes_twice_the_frames() {
        let p = plan(&["--hz", "120", "--at", "1", "--tap", "180,500@0.5", "--swipe", "0,100,60,100@0.7:0.05"]).unwrap();
        assert_eq!(p.shots, vec![(120, 1.0)]);
        assert_eq!(&p.acts[..2], &[(60, Act::Down(180.0, 500.0)), (68, Act::Up)], "a tap is down as long in seconds");
        let moves = p.acts.iter().filter(|a| matches!(a.1, Act::Move(..))).count();
        assert_eq!(moves, 6, "a swipe moves every frame: 0.05 s is 6 frames");
        assert!(plan(&["--hz", "5"]).unwrap_err().contains("--hz 5"));
    }

    #[test]
    fn one_finger_at_a_time() {
        let e = plan(&["--tap", "10,10@1", "--swipe", "0,0,50,50@1.02"]).unwrap_err();
        assert!(e.contains("one finger at a time"), "{e}");
        // Lifting and landing on the same frame boundary is fine: the lift goes first.
        let p = plan(&["--at", "1", "--tap", "10,10@0.1", "--tap", "20,20@0.1667"]).unwrap();
        let at10: Vec<_> = p.acts.iter().filter(|a| a.0 == 10).map(|a| a.1).collect();
        assert_eq!(at10, vec![Act::Up, Act::Down(20.0, 20.0)]);
    }

    #[test]
    fn bad_specs_say_what_was_expected() {
        for (args, want) in [
            (vec!["--tap", "180,500"], "no @time"),
            (vec!["--tap", "180@1"], "expected X,Y@T"),
            (vec!["--tap", "180,x@1"], "not a number"),
            (vec!["--tap", "500,100@1"], "outside the 360×640 screen"),
            (vec!["--tap", "10,10@1:2"], "a tap has no duration"),
            (vec!["--swipe", "1,2,3@1"], "X1,Y1,X2,Y2@T[:SECS]"),
            (vec!["--swipe", "1,2,3,4@1:0"], "duration must be seconds > 0"),
            (vec!["--tilt", "120@1"], "±90"),
        ] {
            let e = plan(&args).unwrap_err();
            assert!(e.contains(want), "{args:?}: {e}");
        }
    }

    #[test]
    fn tilts_take_negative_angles() {
        let p = plan(&["--at", "5", "--tilt", "-20@3", "--tilt", "10,-5@4"]).unwrap();
        assert_eq!(p.acts, vec![(180, Act::Tilt(-20.0, 0.0)), (240, Act::Tilt(10.0, -5.0))]);
    }

    #[test]
    fn a_tilt_with_a_duration_turns_smoothly_sampled_at_100_hz() {
        let p = plan(&["--at", "2", "--tilt", "30@1:0.5", "--tap", "10,10@1.2"]).unwrap();
        // Live runs get every 100 Hz sample at its own time.
        assert_eq!(p.tilts.len(), 50);
        assert!((p.tilts[0].0 - 1.01).abs() < 1e-12);
        assert_eq!(p.tilts.last(), Some(&(1.5, Act::Tilt(30.0, 0.0))));
        let rolls: Vec<f64> = p.tilts.iter().map(|t| if let Act::Tilt(r, _) = t.1 { r } else { f64::NAN }).collect();
        assert!(rolls.windows(2).all(|w| w[1] > w[0]), "it only turns one way: {rolls:?}");
        assert!(rolls[0] < 0.1 && (rolls[24] - 15.0).abs() < 0.5, "slow to start, halfway at half time: {rolls:?}");
        // Stepped frames see the newest sample by their start: one per frame from 1.01 s to 1.5 s.
        let frames: Vec<(u32, f64)> = p.acts.iter().filter_map(|&(f, a)| if let Act::Tilt(r, _) = a { Some((f, r)) } else { None }).collect();
        assert_eq!(frames.len(), 30);
        assert_eq!((frames[0].0, frames.last().unwrap().0), (61, 90));
        assert_eq!(frames.last().unwrap().1, 30.0);
        let k = |f: u32| frames.iter().find(|x| x.0 == f).unwrap().1;
        assert_eq!(k(75), rolls[24], "frame 76 starts at 1.25 s: the 1.25 s sample");
        assert_eq!(k(76), rolls[25], "frame 77 starts at 1.2667 s: still the 1.26 s sample");
        // A live run interleaves them with the touches by time.
        let timed = p.timed(60.0);
        assert_eq!(timed.len(), 52);
        assert!(timed.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(timed[20], (1.2, Act::Down(10.0, 10.0)), "after the 1.2 s sample");
    }

    #[test]
    fn tilts_take_turns_and_a_step_after_a_turn_wins() {
        let e = plan(&["--at", "5", "--tilt", "20@1:0.5", "--tilt", "-20@1.2"]).unwrap_err();
        assert!(e.contains("still turning: one turn at a time"), "{e}");
        let p = plan(&["--at", "5", "--tilt", "-20@1.2", "--tilt", "20@1:0.2", "--tilt", "0@1.205"]).unwrap();
        let tilts: Vec<(u32, Act)> = p.acts.iter().copied().filter(|a| matches!(a.1, Act::Tilt(..))).collect();
        assert_eq!(tilts.last(), Some(&(72, Act::Tilt(0.0, 0.0))), "the last step is the newest sample at frame 72, where the turn ends");
        let p = plan(&["--at", "5", "--tilt", "10@1", "--tilt", "-10@2:0.3"]).unwrap();
        let first_turn = p.tilts.iter().find(|t| t.0 > 2.0).unwrap();
        assert!(matches!(first_turn.1, Act::Tilt(r, _) if r < 10.0 && r > 9.9), "turns from where the last tilt left it: {first_turn:?}");
        assert!((min_jerk(0.5) - 0.5).abs() < 1e-12 && min_jerk(0.0) == 0.0 && min_jerk(1.0) == 1.0);
    }

    #[test]
    fn tilt_is_gravity_in_the_games_axes() {
        assert_eq!(pin_tilt(0.0, 0.0), "mb.sensors(0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0)", "upright, still, facing ahead");
        let g: Vec<f64> = pin_tilt(90.0, 0.0)["mb.sensors(".len()..].split(", ").take(3).map(|v| v.parse().unwrap()).collect();
        assert!((g[0] - 1.0).abs() < 1e-9 && g[1].abs() < 1e-9 && g[2] == 0.0, "rolled right: gravity along +x, {g:?}");
        let g: Vec<f64> = pin_tilt(0.0, 30.0)["mb.sensors(".len()..].split(", ").take(3).map(|v| v.parse().unwrap()).collect();
        assert!(g[0] == 0.0 && (g[1] - 0.75f64.sqrt()).abs() < 1e-9 && (g[2] + 0.5).abs() < 1e-9, "tipped back: {g:?}");
    }

    #[test]
    fn aims_turn_the_phone_with_a_gyros_rate() {
        let p = plan(&["--at", "2", "--aim", "20,-10@1:0.4"]).unwrap();
        assert_eq!(p.tilts.len(), 40);
        assert_eq!(p.tilts.last(), Some(&(1.4, Act::Aim(20.0, -10.0, 0.0, 0.0))));
        let Act::Aim(y, _, dy, _) = p.tilts[19].1 else { panic!() };
        assert!((y - 10.0).abs() < 1.0 && (dy - 20.0 * 1.875 / 0.4).abs() < 3.0, "halfway, at peak speed: {:?}", p.tilts[19]);
        // Upright and turning right: about the phone's −y, as a gyro reports it.
        let v: Vec<f64> = pin_aim(0.0, 0.0, 90.0, 0.0)["mb.sensors(".len()..].trim_end_matches(')').split(", ").map(|v| v.parse().unwrap()).collect();
        assert_eq!(v.len(), 14);
        assert!((v[12] + 90f64.to_radians()).abs() < 1e-3 && v[11].abs() < 1e-6, "{v:?}");
        let e = plan(&["--at", "2", "--aim", "10@1", "--tilt", "5@1"]).unwrap_err();
        assert!(e.contains("use one or the other"), "{e}");
    }

    #[test]
    fn input_after_the_last_shot_is_not_run() {
        let p = plan(&["--at", "1", "--tap", "10,10@3"]).unwrap();
        assert_eq!(p.boundaries(), vec![60]);
    }

    #[test]
    fn page_messages_come_from_the_runtimes_console_lines() {
        let console = |op: &str, msg: Value| json!({ "method": "Runtime.consoleAPICalled", "params": { "args": [{ "value": "[mb]" }, { "value": op }, { "value": msg.to_string() }] } });
        assert!(matches!(page_message(&console("ready", json!({ "op": "ready", "info": { "adapter": "" } }))), Some(Page::Ready(_))));
        let Some(Page::Error(e)) = page_message(&console("error", json!({ "op": "error", "message": "mb_update trapped: unreachable" }))) else { panic!() };
        assert_eq!(e, "mb_update trapped: unreachable");
        let Some(Page::Error(e)) = page_message(&console("watchdog", json!({ "callback": "mb_init", "ms": 612.4 }))) else { panic!() };
        assert_eq!(e, "watchdog: mb_init ran 612 ms");
        assert!(matches!(page_message(&console("log", json!({ "level": 2, "msg": "panic" }))), Some(Page::Log(_))));
        assert!(page_message(&console("log", json!({ "level": 0, "msg": "info" }))).is_none(), "info lines stay quiet");
        assert!(page_message(&json!({ "method": "Runtime.consoleAPICalled", "params": { "args": [{ "value": "hello" }] } })).is_none());
        let thrown = json!({ "method": "Runtime.exceptionThrown", "params": { "exceptionDetails": { "text": "Uncaught", "exception": { "description": "TypeError: x" } } } });
        assert!(matches!(page_message(&thrown), Some(Page::Error(e)) if e == "TypeError: x"));
    }

    #[test]
    fn stats_print_as_one_line() {
        let spread = |p95: f64| json!({ "p50": p95 / 2.0, "p95": p95, "max": p95 * 2.0 });
        let summary = json!({ "mb_update": spread(0.123), "mb_render": spread(0.03), "present": spread(0.5), "gpu": null });
        let st = json!({ "draws": 57, "draws2d": 1, "triangles": 412_345, "reflected": 0, "casters": 28, "particles": 3, "summary": summary, "frames": [{}, {}] });
        assert_eq!(stats_line(&st), "57+1 draws  412.3k tris  0 reflected  28 casters  3 particles | p95 ms: update 0.12  render 0.03  present 0.50  gpu – (2 frames)");
        let st = json!({ "draws": 0, "draws2d": 3, "quads2d": 412, "summary": summary, "frames": [] });
        assert!(stats_line(&st).starts_with("2D: 3 draws  412 quads | "), "{}", stats_line(&st));
    }

    #[test]
    fn the_games_log_lines_come_with_their_frame_and_time() {
        let timings = vec![
            json!({ "frame": 1 }),
            json!({ "frame": 30, "logs": ["debug: node 65539 is the bank", "info: zone 2"] }),
            json!({ "frame": 90, "logs": ["warn: slow", "error: lost"] }),
        ];
        assert_eq!(game_logs(&timings, 60.0, LogLevel::Info, None), ["frame 30 (0.50 s)  info: zone 2", "frame 90 (1.50 s)  warn: slow", "frame 90 (1.50 s)  error: lost"]);
        assert_eq!(game_logs(&timings, 60.0, LogLevel::Debug, None).len(), 4);
        assert_eq!(game_logs(&timings, 120.0, LogLevel::Debug, Some("node")), ["frame 30 (0.25 s)  debug: node 65539 is the bank"]);
        assert!(game_logs(&timings, 60.0, LogLevel::Error, Some("slow")).is_empty());
        let every_frame: Vec<Value> = (1..=90).map(|f| json!({ "frame": f, "logs": ["info: speed 3", format!("info: frame {f}")] })).collect();
        let folded = game_logs(&every_frame, 60.0, LogLevel::Info, None);
        assert_eq!(folded.len(), 91, "a repeated line is listed once");
        assert_eq!(folded[0], "frame 1 (0.02 s)  info: speed 3  (×90, until frame 90)");
        let cli = Cli::try_parse_from(["mb", "game", "--logs"]).unwrap();
        assert_eq!(cli.shot.logs, Some(LogLevel::Debug), "--logs alone: every level");
        let cli = Cli::try_parse_from(["mb", "game", "--logs=warn", "--logs-match", "bank"]).unwrap();
        assert_eq!((cli.shot.logs, cli.shot.logs_match.as_deref()), (Some(LogLevel::Warn), Some("bank")));
        let shots = [Shot { t: 0.5, mark: None, path: "shots/shot-0.5s.png".into(), frame: 30, png: Vec::new() }];
        let text = sheet_text("Hang", &shots, &game_logs(&timings, 60.0, LogLevel::Warn, None));
        assert!(text.contains("   0.5s  frame 30  shot-0.5s.png\n") && text.contains("game log (2 lines):\n  frame 90 (1.50 s)  warn: slow\n"), "{text}");
    }

    #[test]
    fn params_reach_the_held_session() {
        let cli = Cli::try_parse_from(["mb", "game", "--param", "zone=3", "--param", "note=a=b", "--seed", "7"]).unwrap();
        let call = restart_held(&cli.shot.script).unwrap();
        let inner: String = serde_json::from_str(&call["mb.restart(".len()..call.len() - 1]).unwrap();
        let opts: Value = serde_json::from_str(&inner).unwrap();
        assert_eq!(opts, json!({ "hold": true, "seed": "7", "params": { "zone": "3", "note": "a=b" } }));
        for (bad, want) in [("zone", "NAME=VALUE"), ("=3", "a name is"), ("a b=1", "a name is")] {
            assert!(params(&[bad.to_string()]).unwrap_err().contains(want), "{bad}");
        }
        assert!(params(&[format!("x={}", "v".repeat(257))]).unwrap_err().contains("256"));
    }

    #[test]
    fn times_print_plainly() {
        assert_eq!(secs(2.0), "2");
        assert_eq!(secs(0.5), "0.5");
        assert_eq!(secs(12.25), "12.25");
    }

    #[test]
    fn the_sheet_labels_times() {
        let mut img = Image::new(40, 10, [0, 0, 0, 255]);
        draw_text(&mut img, "1.5s", 0, 0, 1, [255, 255, 255, 255]);
        let lit = img.px.iter().filter(|p| p[0] == 255).count();
        assert!(lit > 20, "glyphs were drawn ({lit} pixels)");
        assert!("0123456789.s".chars().all(|c| glyph(c).is_some()));
    }

    fn manifest(orientation: &str, size: [u32; 2], fit: &str) -> Manifest {
        let text = format!(
            "abi = 0\nid = \"com.maimbrain.test.fit\"\nname = \"Fit\"\nversion = \"0.1.0\"\ncreator = \"@test\"\norientation = \"{orientation}\"\nlogical_size = [{}, {}]\nfit = \"{fit}\"\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nstdlib = {{ mb2d = 1 }}\n",
            size[0], size[1]
        );
        Manifest::parse(&text).unwrap()
    }

    fn screen(m: &Manifest, spec: &str) -> Result<[u32; 2], String> {
        let mut m = m.clone();
        fit_screen(&mut m, spec).map(|_| m.logical_size)
    }

    #[test]
    fn a_fill_game_grows_to_the_screens_shape() {
        let m = manifest("portrait", [360, 640], "fill");
        assert_eq!(screen(&m, "360x780"), Ok([360, 780]));
        // A bigger phone: the same Screen, larger (393×852 is 1.09 points a unit).
        assert_eq!(screen(&m, "iphone"), Ok([360, 780]));
        assert_eq!(screen(&m, "SE"), Ok([360, 640]));
        assert_eq!(screen(&m, "pro-max"), Ok([360, 782]));
        // Never less than declared, and no further than 2.4 : 1.
        assert_eq!(screen(&m, "320x480"), Ok([427, 640]));
        assert_eq!(screen(&m, "200x900"), Ok([360, 864]));
        let mut capped = m.clone();
        assert!(fit_screen(&mut capped, "200x900").unwrap().contains("no further"));
    }

    #[test]
    fn a_screen_follows_the_games_orientation() {
        let any = manifest("any", [360, 640], "fill");
        assert_eq!(screen(&any, "780x360"), Ok([780, 360]), "turned sideways");
        let land = manifest("landscape", [640, 360], "fill");
        assert_eq!(screen(&land, "852x393"), Ok([780, 360]));
        assert_eq!(screen(&land, "393x852"), Ok([360, 780]), "upright: its feed card");
        let e = screen(&manifest("portrait", [360, 640], "fill"), "780x360").unwrap_err();
        assert!(e.contains("held sideways") && e.contains("360x780"), "{e}");
    }

    #[test]
    fn a_letterboxed_game_keeps_its_size_and_says_so() {
        let mut m = manifest("portrait", [360, 640], "letterbox");
        let note = fit_screen(&mut m, "pro-max").unwrap();
        assert_eq!(m.logical_size, [360, 640]);
        assert!(note.contains("letterbox") && note.contains("bars"), "{note}");
    }

    #[test]
    fn bad_screens_say_what_was_expected() {
        let m = manifest("portrait", [360, 640], "fill");
        assert!(screen(&m, "big").unwrap_err().contains("pro-max"));
        assert!(screen(&m, "360by780").unwrap_err().contains("WxH"));
        assert!(screen(&m, "36x78").unwrap_err().contains("100 to 4096"));
        assert_eq!(screen(&m, "360×780"), Ok([360, 780]));
        // Not with the flags that turn the phone themselves.
        assert!(Cli::try_parse_from(["mb", "g", "--screen", "se", "--landscape"]).is_err());
        assert!(Cli::try_parse_from(["mb", "g", "--screen", "se", "--portrait"]).is_err());
    }
}
