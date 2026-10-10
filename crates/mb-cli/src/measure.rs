//! `mb measure`: how a game runs in real time, from a headless Chrome
//! (SPEC §9): frame intervals, what each frame cost and drew, input latency
//! and whether the run replays exactly.
//!
//!     mb measure games/frogger --autoplay                      # 10 s live, touched once at 0.5 s
//!     mb measure games/frogger --autoplay --swipe 80,500,280,500@2 --seconds 6 --json
//!     mb measure games/frogger --autoplay --gpu-at 5,20          # the GPU at those moments
//!     mb measure games/frogger --autoplay --quick                 # seconds: the GPU and draws only
//!     mb measure games/frogger --autoplay --final                 # the GPU over 3 browser sessions
//!     mb measure games/frogger --autoplay --ab feed/frogger-old.mbx   # this build vs that one
//!     mb measure games/frogger --autoplay --ab-param shafts=1,0      # one feature on vs off
//!
//! It stages and opens the game the way `mb shot` does (shot.rs), but runs
//! the session live: at 60 Hz on the browser's own animation frames (a
//! headless Chrome draws at 60 Hz whatever the screen), and at any other
//! `--hz` on a timer that steps a frame of 1/hz s at each refresh. Scripted
//! touches are real touch events sent at their wall-clock time, so they meet
//! the frame loop as a finger does, and the input latency probe
//! (`mb.latency`) times them. After the run it's replayed (`mb.verify()`).
//!
//! A live page can't time its GPU, and a live run never reaches the same
//! scene twice, so the GPU is timed apart: the script is stepped again from
//! frame 0 (as `mb shot` does), and at each `--gpu-at` moment the scene on
//! screen is drawn again and again (`mb.gpu`), each draw waiting for the
//! GPU, in a few repeats. That happens in several browser sessions (the
//! live run's browser, then fresh ones): repeats within one session agree
//! more closely than sessions do (the GPU process, its clocks and caches
//! start afresh), so the number is the sessions' average and its confidence
//! interval is over the sessions, not the repeats. `--ab` does that for two
//! builds side by side, alternating between them in each session, and gives
//! the difference with a confidence interval over the sessions.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::shot::{self, Plan, Player, Script, check_page, eval};
use crate::{budget, cdp, serve};

/// How long a run lasts without --seconds, at least (and 1 s past the last input).
const DEFAULT_SECS: f64 = 10.0;
/// How often the frames kept by `mb.stats()` (600) are collected during the run.
const COLLECT_EVERY: Duration = Duration::from_secs(4);
/// A frame interval this much longer than the median is a long frame (a missed refresh).
const LONG_FRAME: f64 = 1.5;
/// GPU timing: redraws per repeat, repeats per moment in each session (alone and
/// with --ab), redraws first thrown away at each moment (caches, clocks), and
/// browser sessions.
const REDRAWS: u32 = 25;
const REPEATS: u32 = 5;
const AB_REPEATS: u32 = 8;
const QUICK_REPEATS: u32 = 3;
const WARMUP: u32 = 5;
/// Browser sessions with --ab or --final (one otherwise: each costs a fresh browser
/// and the whole script stepped again).
const SESSIONS: u32 = 3;

#[derive(clap::Args)]
pub struct MeasureArgs {
    /// Game directory or .mbx bundle.
    path: PathBuf,
    /// How long to run, in seconds (default: 10, or 1 s past the last scripted input).
    #[arg(long, value_name = "SECS")]
    seconds: Option<f64>,
    #[command(flatten)]
    script: Script,
    /// Seconds into the run before which frames are startup (shaders compiling, assets
    /// arriving): reported on a line of their own, not in the frame summaries or long frames.
    #[arg(long, value_name = "SECS", default_value_t = 0.5)]
    from: f64,
    /// Game times at which to time the GPU: the script is stepped again to each, and the scene
    /// there is drawn 25 times in each of --gpu-repeats repeats, each draw waiting for the GPU
    /// (default: a third of the way, two thirds and the end of the run).
    #[arg(long, value_delimiter = ',', value_name = "SECS,…")]
    gpu_at: Vec<f64>,
    /// Repeats at each --gpu-at moment in each session (default 5, 8 with --ab, 3 with --quick).
    #[arg(long, value_name = "N")]
    gpu_repeats: Option<u32>,
    /// Browser sessions to time the GPU in (default 1, or 3 with --ab and --final): the live
    /// run's browser, then fresh ones. With several, the ± is a 95% confidence interval over the
    /// sessions, which differ by more than repeats within one do; with 1 it's over the repeats
    /// only, and another run can land outside it.
    #[arg(long, value_name = "N")]
    gpu_sessions: Option<u32>,
    /// The numbers to report or keep: the GPU timed in 3 browser sessions (unless
    /// --gpu-sessions), so its ± covers what separate runs differ by. Slower.
    #[arg(long = "final", conflicts_with = "quick")]
    final_: bool,
    /// "Did this change help?": no live run and no replay check, only the GPU at the --gpu-at
    /// moments in one short sample (3 repeats in one browser) and what the stepped frames
    /// cost and drew. About 20–30 s. Compare quick runs with each other.
    #[arg(long, conflicts_with = "ab")]
    quick: bool,
    /// Print every line of the report (the default is one short summary block).
    #[arg(long, short)]
    verbose: bool,
    /// Compare the GPU time with another build of the game (a game directory or .mbx): no live
    /// run; in each of --gpu-sessions browsers both step the script to each --gpu-at moment and
    /// are timed in turn, and the report gives B − A with a 95% confidence interval.
    /// A game directory is built against the same SDK as this one (its kit's `patch` applied when
    /// it names the kit's git repository), or refused if that can't be done.
    #[arg(long, value_name = "OTHER", conflicts_with = "ab_param")]
    ab: Option<PathBuf>,
    /// Compare two values of one of the game's debug parameters (`sys::param`) in this one build:
    /// A runs with NAME=A, B with NAME=B, otherwise as --ab (e.g. --ab-param shafts=1,0 prices
    /// the light shafts). Repeatable, for several parameters at once.
    #[arg(long, value_name = "NAME=A,B")]
    ab_param: Vec<String>,
    /// Print everything as one JSON object: the frames and their summary, `mb.latency()`, the
    /// GPU timings and `mb.verify()`'s result.
    #[arg(long)]
    json: bool,
    /// Port to serve the game on (default: any free one; with --ab, the other build is on the next).
    #[arg(long)]
    port: Option<u16>,
    /// The Chrome, Chromium or Edge executable (default: MB_CHROME, else the usual places).
    #[arg(long)]
    chrome: Option<PathBuf>,
}

/// Steps the held session on a timer, `hz` frames a second for `secs`: each
/// frame starts at its refresh, and one that runs past the next refresh
/// makes the following frame wait for the one after, with a dt to match (as
/// a display skips a refresh). Unpaced, as a display doesn't wait for the
/// GPU before the next frame's work. The wait yields through a
/// MessageChannel rather than setTimeout: Chrome holds timers back for up
/// to ~100 ms after a touchstart (expecting a scroll), which would read as
/// a long frame after every tap. Resolves to each frame's start time (ms,
/// performance.now).
const DRIVE: &str = r#"window.__mbDrive = (async (hz, secs) => {
  const period = 1000 / hz, t0 = performance.now(), end = t0 + secs * 1000, starts = [];
  const ch = new MessageChannel(), tick = () => new Promise((r) => { ch.port1.onmessage = r; ch.port2.postMessage(null); });
  let k = 0, ticks = 1;
  while (t0 + k * period < end) {
    while (performance.now() < t0 + k * period) await tick();
    starts.push(performance.now());
    await mb.step(1, { dt: Math.min(ticks / hz, 0.1), paced: false });
    const next = Math.max(k + 1, Math.ceil((performance.now() - t0) / period));
    ticks = next - k;
    k = next;
  }
  return starts;
})"#;

pub fn run(a: MeasureArgs) -> Result<(), String> {
    let manifest = shot::game_manifest(&a.path)?;
    let hz = a.script.hz;
    // When the script's input ends: planned against a far-off end, so none is cut off.
    let last_input = Plan::new(&a.script, &[3600.0], manifest.logical_size)?.timed(hz).last().map_or(0.0, |x| x.0);
    let secs = match a.seconds {
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(s) => return Err(format!("--seconds {s}: give seconds > 0")),
        None => DEFAULT_SECS.max(last_input + 1.0),
    };
    if !(a.from.is_finite() && a.from >= 0.0 && a.from < secs) {
        return Err(format!("--from {}: give seconds from 0 to the run's length ({secs} s)", a.from));
    }
    let plan = Plan::new(&a.script, &[secs], manifest.logical_size)?;
    plan.check_sensors(&manifest)?;
    let moments = gpu_moments(&a.gpu_at, secs)?;
    let ab_mode = a.ab.is_some() || !a.ab_param.is_empty();
    let repeats = a.gpu_repeats.unwrap_or(if ab_mode {
        AB_REPEATS
    } else if a.quick {
        QUICK_REPEATS
    } else {
        REPEATS
    });
    if !(2..=100).contains(&repeats) {
        return Err(format!("--gpu-repeats {repeats}: from 2 to 100"));
    }
    let sessions = a.gpu_sessions.unwrap_or(if ab_mode || a.final_ { SESSIONS } else { 1 });
    if !(1..=20).contains(&sessions) {
        return Err(format!("--gpu-sessions {sessions}: from 1 to 20"));
    }
    if a.quick && ab_mode {
        return Err("--quick doesn't compare: drop it for --ab or --ab-param".into());
    }
    if a.quick && sessions > 1 {
        return Err("--quick times the GPU in one browser session: leave out --gpu-sessions, or drop --quick".into());
    }
    let gpu_plan = Plan::new(&a.script, &moments, manifest.logical_size)?;
    let chrome_exe = cdp::find_chrome(a.chrome.as_deref())?;
    if let Some(other) = &a.ab {
        let sides = [(a.path.clone(), a.script.clone(), short_name(&a.path)), (other.clone(), a.script.clone(), short_name(other))];
        return ab(&a, sides, &chrome_exe, &moments, repeats, sessions);
    }
    if !a.ab_param.is_empty() {
        let [pa, pb] = ab_params(&a.ab_param)?;
        let side = |p: Vec<String>| {
            let mut s = a.script.clone();
            s.param.extend(p.iter().cloned());
            (a.path.clone(), s, p.join(" "))
        };
        return ab(&a, [side(pa), side(pb)], &chrome_exe, &moments, repeats, sessions);
    }
    if a.quick {
        return quick(&a, &manifest, &chrome_exe, &gpu_plan, repeats);
    }

    let work = std::env::temp_dir().join(format!("mb-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| {
        let site = work.join("site");
        shot::stage(&a.path, &site, a.script.seed)?;
        let port = serve::serve_site(site, a.port.unwrap_or(0))?;
        let url = format!("http://127.0.0.1:{port}/runtime/index.html");
        let (mut chrome, sid) = shot::open_page(&chrome_exe, &url, &manifest)?;
        let sid = sid.as_str();

        // Held until everything is set, so the first frame is the run's.
        eval(&mut chrome, sid, &shot::restart_held(&a.script)?)?;
        check_page(&mut chrome)?;
        if shot::has_motion_sensors(&manifest) {
            eval(&mut chrome, sid, &shot::pin_tilt(0.0, 0.0))?;
        }
        eval(&mut chrome, sid, "mb.latency(true), mb.stats({ reset: true }), undefined")?;
        let raf = hz == 60.0;
        let t0 = Instant::now();
        if raf {
            eval(&mut chrome, sid, "mb.resume()")?;
        } else {
            eval(&mut chrome, sid, &format!("{DRIVE}({hz}, {secs}), undefined"))?;
        }

        // Input at its wall-clock time; the frames collected now and then.
        let mut frames: Vec<Value> = Vec::new();
        let mut collected = Instant::now();
        let at = |t: f64| t0 + Duration::from_secs_f64(t);
        let end = t0 + Duration::from_secs_f64(secs);
        let timeline = plan.timed(hz);
        let mut acts = timeline.iter().filter(|x| x.0 < secs).peekable();
        loop {
            let next = acts.peek().map_or(end, |x| at(x.0)).min(collected + COLLECT_EVERY).min(end);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            while let Some(&&(f, act)) = acts.peek()
                && at(f) <= Instant::now()
            {
                shot::send_act(&mut chrome, sid, act)?;
                acts.next();
            }
            if collected.elapsed() >= COLLECT_EVERY {
                frames.extend(collect(&mut chrome, sid)?);
                collected = Instant::now();
                check_page(&mut chrome)?;
            }
            if Instant::now() >= end && acts.peek().is_none() {
                break;
            }
        }
        let starts = if raf {
            eval(&mut chrome, sid, "mb.suspend()")?;
            Value::Null
        } else {
            eval(&mut chrome, sid, "window.__mbDrive")?
        };
        frames.extend(collect(&mut chrome, sid)?);
        check_page(&mut chrome)?;
        let wall = t0.elapsed().as_secs_f64();
        let budgets = eval(&mut chrome, sid, "mb.stats().budgets")?;
        let latency = eval(&mut chrome, sid, "mb.latency()")?;
        let verify = eval(&mut chrome, sid, "mb.verify()")?;
        check_page(&mut chrome)?;
        // The GPU, at the same game times every run (a live page can't time it): in this
        // browser, then in fresh ones, one at a time.
        let mut gpu = Gpu { sessions: vec![time_session(&mut chrome, sid, &manifest, &gpu_plan, &a.script, repeats)?] };
        check_page(&mut chrome)?;
        drop(chrome);
        for k in 2..=sessions {
            eprintln!("  gpu session {k} of {sessions} (a fresh browser)");
            let (mut chrome, sid) = shot::open_page(&chrome_exe, &url, &manifest)?;
            gpu.sessions.push(time_session(&mut chrome, &sid, &manifest, &gpu_plan, &a.script, repeats)?);
            check_page(&mut chrome)?;
        }

        // Timer-stepped frames have no animation-frame interval: it's from one frame's start to the next's.
        if let Some(s) = starts.as_array() {
            for (i, f) in frames.iter_mut().enumerate().take(s.len()) {
                f["interval"] = match i.checked_sub(1) {
                    Some(j) => s[i].as_f64().zip(s[j].as_f64()).map_or(Value::Null, |(b, a)| json!(b - a)),
                    None => Value::Null,
                };
            }
        }
        let report = Report { secs: wall, hz, raf, from: a.from, frames: &frames, latency: &latency, gpu: &gpu, verify: &verify, budgets: &budgets, machine: busy_machine() };
        if a.json {
            println!("{}", report.json());
        } else if a.verbose {
            print!("{}", report.text(&manifest.name));
        } else {
            print!("{}", report.short(&manifest.name));
        }
        if verify["match"].as_bool() != Some(true) {
            return Err(match verify["firstMismatch"].as_u64() {
                Some(f) => format!("mb.verify(): the replay differs from frame {f} on: the game depends on something that isn't recorded (SPEC §9)"),
                None => "mb.verify(): the replay doesn't match the run".into(),
            });
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// `mb measure --quick`: no live run and no replay check. The script is
/// stepped once to each `--gpu-at` moment, where the scene is timed in a few
/// short repeats; the stepped frames give the CPU costs and what was drawn.
fn quick(a: &MeasureArgs, m: &mb_format::Manifest, exe: &std::path::Path, plan: &Plan, repeats: u32) -> Result<(), String> {
    let t0 = Instant::now();
    let work = std::env::temp_dir().join(format!("mb-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| {
        let site = work.join("site");
        shot::stage(&a.path, &site, a.script.seed)?;
        let port = serve::serve_site(site, a.port.unwrap_or(0))?;
        let url = format!("http://127.0.0.1:{port}/runtime/index.html");
        let (mut chrome, sid) = shot::open_page(exe, &url, m)?;
        let sid = sid.as_str();
        let mut player = Player::start(&mut chrome, sid, m, plan, &a.script, None)?;
        let (mut moments, mut frames) = (Vec::new(), Vec::new());
        for &(frame, at) in &plan.shots {
            player.run_to(&mut chrome, sid, frame)?;
            frames.extend(collect(&mut chrome, sid)?);
            time_gpu(&mut chrome, sid, WARMUP)?;
            let runs = (0..repeats).map(|_| time_gpu(&mut chrome, sid, REDRAWS)).collect::<Result<_, _>>()?;
            moments.push(Moment { at, frame, runs });
            // Not the redraws.
            eval(&mut chrome, sid, "void mb.stats({ reset: true })")?;
        }
        let budgets = eval(&mut chrome, sid, "mb.stats().budgets")?;
        check_page(&mut chrome)?;
        let gpu = Gpu { sessions: vec![moments] };
        let q = Quick { gpu: &gpu, frames: &frames, budgets: &budgets, secs: t0.elapsed().as_secs_f64() };
        if a.json {
            println!("{}", q.json());
        } else {
            print!("{}", q.text(&m.name));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// What `--quick` found.
struct Quick<'a> {
    gpu: &'a Gpu,
    /// The frames stepped to reach the moments (`mb.stats()`'s, without intervals).
    frames: &'a [Value],
    budgets: &'a Value,
    /// Wall-clock seconds the whole measurement took.
    secs: f64,
}

impl Quick<'_> {
    fn field(&self, k: &str) -> Option<Spread> {
        spread(self.frames.iter().filter_map(|f| f[k].as_f64()))
    }

    fn json(&self) -> Value {
        let summary: serde_json::Map<String, Value> = COST.iter().chain(&DREW).map(|k| (k.to_string(), Spread::json(self.field(k)))).collect();
        json!({ "quick": true, "seconds": self.secs, "frames": self.frames.len(), "summary": summary, "gpu": self.gpu.json(), "budget": budget::summary(self.frames, self.budgets).json() })
    }

    fn text(&self, game: &str) -> String {
        let (ms, ci) = self.gpu.ms();
        let p95 = |k: &str| self.field(k).map_or("–".to_string(), |s| format!("{:.2}", s.p95));
        let at: Vec<String> = self.gpu.times().iter().enumerate().map(|(k, t)| format!("{t} s {:.2}", self.gpu.moment(k).0)).collect();
        let mut out = format!(
            "{game} (quick, {:.0} s): gpu ms {ms:.2} ± {ci:.2}  ({}; {} repeats in one browser: compare quick runs with each other, --final for numbers to keep)\n",
            self.secs,
            at.join("  "),
            self.gpu.repeats()
        );
        out += &format!("  cpu ms p95    update {}  render {}  (the {} frames stepped there)\n", p95("mb_update"), p95("mb_render"), self.frames.len());
        out += &drew_line(|k| self.field(k));
        out += &budget::summary(self.frames, self.budgets).text();
        out
    }
}

/// "  drew p95      36+2 draws  120.0k tris …", from a frame field's spread.
fn drew_line(field: impl Fn(&str) -> Option<Spread>) -> String {
    let n = |k: &str| field(k).map_or("–".to_string(), |s| if s.p95 >= 10_000.0 { format!("{:.1}k", s.p95 / 1000.0) } else { format!("{}", s.p95) });
    let mut out = if field("triangles").is_some() {
        format!("  drew p95      {}+{} draws  {} tris  {} reflected  {} casters  {} particles\n", n("draws"), n("draws2d"), n("triangles"), n("reflected"), n("casters"), n("particles"))
    } else {
        format!("  drew p95      2D: {} draws  {} quads\n", n("draws2d"), n("quads2d"))
    };
    // The most texture memory any frame had, against mb3d's budget.
    let max = |k: &str| field(k).map_or(0.0, |s| s.max);
    if let Some(t) = field("textureBytes").and_then(|b| crate::budget::textures(b.max, max("textureBudget"), max("textures"), max("textureLimit"))) {
        out += &format!("  memory max    {t}\n");
    }
    out
}

/// What on this machine slows a browser's frames, for a run the browser paced:
/// running on battery or in Low Power Mode (macOS; the profile turns Chrome's
/// Energy Saver off, but the system may still slow it) and other headless
/// browsers (other `mb shot`/`mb measure` runs). Empty if nothing is known.
fn busy_machine() -> String {
    let run = |cmd: &str, args: &[&str]| std::process::Command::new(cmd).args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string());
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        if run("pmset", &["-g", "batt"]).is_some_and(|s| s.contains("'Battery Power'")) {
            out.push("this Mac is on battery".to_string());
        }
        if run("pmset", &["-g"]).is_some_and(|s| s.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ["lowpowermode", "1"])) {
            out.push("Low Power Mode is on".to_string());
        }
    }
    if let Some(ps) = run("ps", &["-Ao", "args"]) {
        // Browsers (not their helpers) started headless with DevTools, like ours.
        let n = ps.lines().filter(|l| l.contains("--headless") && l.contains("--remote-debugging-port") && !l.contains("--type=")).count();
        if n > 1 {
            out.push(format!("{} other headless browsers were running", n - 1));
        }
    }
    out.join(", ")
}

/// The frames `mb.stats()` kept since the last call, clearing them.
fn collect(chrome: &mut cdp::Chrome, sid: &str) -> Result<Vec<Value>, String> {
    Ok(eval(chrome, sid, "mb.stats({ reset: true }).frames")?.as_array().cloned().unwrap_or_default())
}

// --- the GPU ------------------------------------------------------------------

/// `--gpu-at`, checked and sorted, or by default a third of the way into
/// the run, two thirds and its end (to 0.1 s).
fn gpu_moments(at: &[f64], secs: f64) -> Result<Vec<f64>, String> {
    let mut v: Vec<f64> = if at.is_empty() { [secs / 3.0, secs * 2.0 / 3.0, secs].iter().map(|t| (t * 10.0).round() / 10.0).collect() } else { at.to_vec() };
    if let Some(t) = v.iter().find(|t| !(t.is_finite() && **t > 0.0)) {
        return Err(format!("--gpu-at {t}: game times in seconds > 0"));
    }
    v.sort_by(f64::total_cmp);
    v.dedup();
    Ok(v)
}

/// One browser session's GPU timing: the script stepped from frame 0 in a
/// fresh held session to each moment, where the scene is timed in repeats.
fn time_session(chrome: &mut cdp::Chrome, sid: &str, m: &mb_format::Manifest, plan: &Plan, script: &Script, repeats: u32) -> Result<Vec<Moment>, String> {
    let mut player = Player::start(chrome, sid, m, plan, script, None)?;
    let mut out = Vec::new();
    for &(frame, at) in &plan.shots {
        player.run_to(chrome, sid, frame)?;
        time_gpu(chrome, sid, WARMUP)?;
        let runs = (0..repeats).map(|_| time_gpu(chrome, sid, REDRAWS)).collect::<Result<_, _>>()?;
        out.push(Moment { at, frame, runs });
    }
    Ok(out)
}

/// GPU ms of `n` redraws of the frame on screen (`mb.gpu`).
fn time_gpu(chrome: &mut cdp::Chrome, sid: &str, n: u32) -> Result<Vec<f64>, String> {
    let ms: Vec<f64> = eval(chrome, sid, &format!("mb.gpu({n})"))?.as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
    if ms.len() != n as usize {
        return Err(format!("mb.gpu({n}) timed {} draws: the page lost its GPU or the game stopped", ms.len()));
    }
    Ok(ms)
}

/// The middle value (the mean of the middle two of an even count); NaN for none.
fn median(v: impl IntoIterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = v.into_iter().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// The scene at one game time, timed: each repeat's redraws' GPU ms.
#[derive(Debug, Clone, PartialEq)]
struct Moment {
    at: f64,
    frame: u32,
    runs: Vec<Vec<f64>>,
}

impl Moment {
    /// Each repeat's median.
    fn medians(&self) -> Vec<f64> {
        self.runs.iter().map(|r| median(r.iter().copied())).collect()
    }
    /// The mean of the repeats' medians, and the half-width of its 95% confidence interval.
    fn ms(&self) -> (f64, f64) {
        mean_ci(&self.medians())
    }
    fn json(&self) -> Value {
        let (ms, ci) = self.ms();
        json!({ "seconds": self.at, "frame": self.frame, "ms": ms, "ci95": ci, "repeats": self.medians(), "samples": self.runs })
    }
}

/// Over every moment, repeat by repeat (repeat k being every moment's k-th
/// run): the mean of the moments' medians, and its 95% confidence interval.
fn overall(moments: &[Moment]) -> (f64, f64) {
    let repeats = moments.iter().map(|m| m.runs.len()).min().unwrap_or(0);
    let per: Vec<f64> = (0..repeats).map(|k| moments.iter().map(|m| median(m.runs[k].iter().copied())).sum::<f64>() / moments.len() as f64).collect();
    mean_ci(&per)
}

/// The GPU timed in one or more browser sessions, each every moment's repeats.
///
/// Repeats within a session share its GPU process, clocks and caches, so
/// they agree more closely than sessions do: the interval over a session's
/// repeats understated how far two runs differ (3.96 ± 0.12 and 5.08 ± 0.43 ms
/// for one build). With several sessions the number is the mean of the
/// sessions' numbers and the interval is over them (Student's t, sessions − 1
/// degrees of freedom): the variance between sessions, which already holds
/// the repeats' own noise.
#[derive(Debug, Clone, PartialEq, Default)]
struct Gpu {
    sessions: Vec<Vec<Moment>>,
}

impl Gpu {
    /// Each session's number and the interval over its repeats.
    fn per_session(&self) -> Vec<(f64, f64)> {
        self.sessions.iter().map(|s| overall(s)).collect()
    }

    /// The number and its 95% confidence interval: over the sessions when there
    /// are several, else over the one session's repeats.
    fn ms(&self) -> (f64, f64) {
        let per = self.per_session();
        match per.len() {
            0 => (f64::NAN, f64::NAN),
            1 => per[0],
            _ => mean_ci(&per.iter().map(|p| p.0).collect::<Vec<_>>()),
        }
    }

    /// Moment `k`'s number and interval, the same way.
    fn moment(&self, k: usize) -> (f64, f64) {
        let per: Vec<f64> = self.sessions.iter().filter_map(|s| s.get(k)).map(|m| m.ms().0).collect();
        match self.sessions.len() {
            0 => (f64::NAN, f64::NAN),
            1 => self.sessions[0][k].ms(),
            _ => mean_ci(&per),
        }
    }

    fn times(&self) -> Vec<f64> {
        self.sessions.first().map_or(Vec::new(), |s| s.iter().map(|m| m.at).collect())
    }

    fn repeats(&self) -> usize {
        self.sessions.first().and_then(|s| s.first()).map_or(0, |m| m.runs.len())
    }

    /// The typical interval within one session (the mean of theirs).
    fn within(&self) -> f64 {
        let per = self.per_session();
        per.iter().map(|p| p.1).sum::<f64>() / per.len().max(1) as f64
    }

    fn json(&self) -> Value {
        let (ms, ci) = self.ms();
        let at: Vec<Value> = self
            .times()
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let (ms, ci) = self.moment(k);
                json!({ "seconds": t, "frame": self.sessions[0][k].frame, "ms": ms, "ci95": ci })
            })
            .collect();
        let sessions: Vec<Value> = self
            .sessions
            .iter()
            .map(|s| {
                let (ms, ci) = overall(s);
                json!({ "ms": ms, "ci95": ci, "at": s.iter().map(Moment::json).collect::<Vec<_>>() })
            })
            .collect();
        json!({ "ms": ms, "ci95": ci, "scope": if self.sessions.len() > 1 { "sessions" } else { "repeats" }, "redraws": REDRAWS, "within95": self.within(), "at": at, "sessions": sessions })
    }

    /// The report's lines, the first labelled `label` (padded like the others).
    fn text(&self, label: &str) -> String {
        let (ms, ci) = self.ms();
        let n = self.sessions.len();
        let times = and_list(&self.times());
        let what = format!("{} repeats of {REDRAWS} redraws of the scene at {times}, the median of each, averaged", self.repeats());
        let mut out = if n > 1 {
            format!("  {label:<14}{ms:.2} ± {ci:.2}  (95% CI over {n} browser sessions; in each, {what})\n")
        } else {
            format!("  {label:<14}{ms:.2} ± {ci:.2}  (one browser session, {what}: the ± is within it only; separate runs differ by more, --final)\n")
        };
        if n > 1 {
            let per: Vec<String> = self.per_session().iter().map(|p| format!("{:.2}", p.0)).collect();
            out += &format!("                sessions {}  (each ± {:.2} over its own repeats)\n", per.join(", "), self.within());
        }
        if self.times().len() > 1 {
            let at: Vec<String> = self.times().iter().enumerate().map(|(k, t)| format!("{t} s {:.2} ± {:.2}", self.moment(k).0, self.moment(k).1)).collect();
            out += &format!("                {}\n", at.join("   "));
        }
        out
    }
}

/// "3.3, 6.7 and 10 s".
fn and_list(times: &[f64]) -> String {
    let t: Vec<String> = times.iter().map(|t| format!("{t}")).collect();
    match t.split_last() {
        Some((last, [])) => format!("{last} s"),
        Some((last, rest)) => format!("{} and {last} s", rest.join(", ")),
        None => String::new(),
    }
}

/// The two-sided 95% point of Student's t with `df` degrees of freedom.
fn t95(df: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160, 2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080,
        2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::INFINITY,
        1..=30 => T[df - 1],
        31..=60 => 2.000,
        61..=120 => 1.980,
        _ => 1.960,
    }
}

/// The mean of `v` and the half-width of its 95% confidence interval (Student's t).
fn mean_ci(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, t95(v.len().saturating_sub(1)) * (var / n).sqrt())
}

/// One build in an A/B comparison: what it is and what was timed.
struct Side {
    name: String,
    gpu: Gpu,
    /// Every stepped frame's `mb_update` and `mb_render` ms.
    cpu: [Vec<f64>; 2],
}

/// `--ab-param NAME=A,B …`: the debug parameters of side A and of side B.
fn ab_params(specs: &[String]) -> Result<[Vec<String>; 2], String> {
    let mut out = [Vec::new(), Vec::new()];
    for spec in specs {
        let parts = spec.split_once('=').and_then(|(k, v)| Some((k, v.split_once(',')?)));
        let Some((k, (x, y))) = parts else {
            return Err(format!("--ab-param {spec}: expected NAME=A,B, e.g. --ab-param shafts=1,0"));
        };
        out[0].push(format!("{k}={x}"));
        out[1].push(format!("{k}={y}"));
    }
    for p in &out {
        shot::params(p)?;
    }
    if out[0] == out[1] {
        return Err("--ab-param: A and B are the same".into());
    }
    Ok(out)
}

/// The SDK a game directory compiles against, as cargo resolves it: the
/// `maimbrain` package's directory and its source (None for a path
/// dependency, else e.g. "git+https://github.com/terracatta/maimbrain-kit#3f2a…").
fn sdk_of(dir: &std::path::Path, config: &[String]) -> Result<(PathBuf, Option<String>), String> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(config.iter().flat_map(|c| ["--config", c.as_str()]))
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let p = v["packages"].as_array().into_iter().flatten().find(|p| p["name"] == "maimbrain").ok_or_else(|| format!("{} doesn't depend on maimbrain", dir.display()))?;
    let manifest = PathBuf::from(p["manifest_path"].as_str().unwrap_or_default());
    let dir = manifest.parent().map(|d| d.canonicalize().unwrap_or(d.to_path_buf())).unwrap_or_default();
    Ok((dir, p["source"].as_str().map(String::from)))
}

/// "https://github.com/terracatta/maimbrain-kit" of "git+https://github.com/terracatta/maimbrain-kit?branch=x#3f2a…".
fn git_url(source: &str) -> Option<&str> {
    let s = source.strip_prefix("git+")?;
    Some(s.split(['?', '#']).next().unwrap_or(s))
}

/// The cargo `--config` settings to build `other` against the SDK `this`
/// builds against: none when they already agree; a `patch` of the kit's
/// git repository to this one's SDK directory when `other` names the
/// repository (a copy of an older commit, outside the workspace whose
/// `.cargo/config.toml` patches it); otherwise an error, so an A/B never
/// silently compares two SDKs.
fn same_sdk(this: &std::path::Path, other: &std::path::Path) -> Result<Vec<String>, String> {
    let (a_dir, a_src) = sdk_of(this, &[])?;
    let (b_dir, b_src) = sdk_of(other, &[])?;
    if a_dir == b_dir {
        return Ok(Vec::new());
    }
    let shown = |d: &std::path::Path, s: &Option<String>| s.clone().unwrap_or_else(|| d.display().to_string());
    let differ = format!("A builds against the SDK {}, B against {}", shown(&a_dir, &a_src), shown(&b_dir, &b_src));
    let Some(url) = b_src.as_deref().and_then(git_url) else {
        return Err(format!("--ab: {differ}: compare builds made with one SDK (point B's Cargo.toml at A's, or give B as a .mbx built with it)"));
    };
    let patch = vec![format!("patch.\"{url}\".maimbrain.path={}", toml_string(&a_dir.display().to_string()))];
    let (patched, _) = sdk_of(other, &patch)?;
    if patched != a_dir {
        return Err(format!("--ab: {differ}, and patching B's {url} to A's SDK didn't take ({})", patched.display()));
    }
    eprintln!("  B builds against A's SDK ({}), not {}: patched as A's workspace does", a_dir.display(), shown(&b_dir, &b_src));
    Ok(patch)
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `mb measure A --ab B` (or `--ab-param`): both builds in one browser,
/// stepped through the same script to each moment; there they're timed in
/// turn, k repeats each, alternating which goes first, so drift (clocks,
/// heat) falls on both alike. Again in each of `--gpu-sessions` fresh
/// browsers. Each side is a build, the script it plays (its debug
/// parameters) and its name in the report.
fn ab(a: &MeasureArgs, sides_in: [(PathBuf, Script, String); 2], exe: &std::path::Path, moments: &[f64], repeats: u32, sessions: u32) -> Result<(), String> {
    let manifests = [shot::game_manifest(&sides_in[0].0)?, shot::game_manifest(&sides_in[1].0)?];
    let plans = manifests.iter().zip(&sides_in).map(|(m, s)| Plan::new(&s.1, moments, m.logical_size)).collect::<Result<Vec<_>, _>>()?;
    for (p, m) in plans.iter().zip(&manifests) {
        p.check_sensors(m)?;
    }
    let same_build = sides_in[0].0 == sides_in[1].0;
    let config = if !same_build && sides_in[0].0.is_dir() && sides_in[1].0.is_dir() { same_sdk(&sides_in[0].0, &sides_in[1].0)? } else { Vec::new() };
    let work = std::env::temp_dir().join(format!("mb-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| {
        let mut urls: Vec<String> = Vec::new();
        for (i, (path, ..)) in sides_in.iter().enumerate() {
            if i == 1 && same_build {
                urls.push(urls[0].clone());
                continue;
            }
            let site = work.join(format!("site-{i}"));
            shot::stage_with(path, &site, a.script.seed, if i == 1 { &config } else { &[] })?;
            let port = serve::serve_site(site, a.port.map_or(0, |p| p + i as u16))?;
            urls.push(format!("http://127.0.0.1:{port}/runtime/index.html"));
        }
        let mut sides = sides_in.each_ref().map(|s| Side { name: s.2.clone(), gpu: Gpu::default(), cpu: [Vec::new(), Vec::new()] });
        for session in 1..=sessions {
            let (mut chrome, sid_a) = shot::open_page(exe, &urls[0], &manifests[0])?;
            let sid_b = shot::open_tab(&mut chrome, &urls[1], &manifests[1])?;
            let sids = [sid_a.as_str(), sid_b.as_str()];
            let mut players = Vec::new();
            for i in 0..2 {
                front(&mut chrome, sids[i])?;
                players.push(Player::start(&mut chrome, sids[i], &manifests[i], &plans[i], &sides_in[i].1, None)?);
            }
            let mut runs: [Vec<Moment>; 2] = [Vec::new(), Vec::new()];
            for (k, &(frame, at)) in plans[0].shots.iter().enumerate() {
                for i in 0..2 {
                    front(&mut chrome, sids[i])?;
                    players[i].run_to(&mut chrome, sids[i], plans[i].shots[k].0)?;
                    time_gpu(&mut chrome, sids[i], WARMUP)?;
                    runs[i].push(Moment { at, frame, runs: Vec::new() });
                }
                // Alternating which goes first, and starting each session with the other one.
                for r in 0..repeats as usize {
                    for i in if (r + session as usize) % 2 == 0 { [0, 1] } else { [1, 0] } {
                        front(&mut chrome, sids[i])?;
                        let ms = time_gpu(&mut chrome, sids[i], REDRAWS)?;
                        runs[i][k].runs.push(ms);
                    }
                }
                eprintln!("  session {session} of {sessions}: timed {at} s");
            }
            check_page(&mut chrome)?;
            for (i, (side, moments)) in sides.iter_mut().zip(runs).enumerate() {
                side.gpu.sessions.push(moments);
                for (j, k) in ["mb_update", "mb_render"].iter().enumerate() {
                    side.cpu[j].extend(players[i].timings.iter().filter_map(|t| t[*k].as_f64()));
                }
            }
        }
        let r = AbReport { sides, redraws: REDRAWS, hz: a.script.hz };
        if a.json {
            println!("{}", r.json());
        } else {
            print!("{}", r.text());
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Makes this page the one in front: a page behind another gets no
/// animation frames, and touches sent to it wait for one.
fn front(chrome: &mut cdp::Chrome, sid: &str) -> Result<(), String> {
    chrome.call(Some(sid), "Page.bringToFront", json!({})).map(|_| ())
}

struct AbReport {
    sides: [Side; 2],
    redraws: u32,
    hz: f64,
}

impl AbReport {
    /// Paired differences B − A, session by session: one per repeat at each
    /// moment (the medians of their redraws).
    fn diffs(&self) -> Vec<Vec<f64>> {
        let [a, b] = &self.sides;
        a.gpu
            .sessions
            .iter()
            .zip(&b.gpu.sessions)
            .map(|(sa, sb)| sa.iter().zip(sb).flat_map(|(x, y)| x.medians().into_iter().zip(y.medians()).map(|(p, q)| q - p).collect::<Vec<_>>()).collect())
            .collect()
    }

    /// B − A and its 95% confidence interval: over the sessions' mean
    /// differences when there are several, else over the one session's pairs.
    fn diff(&self) -> (f64, f64) {
        let d = self.diffs();
        match d.len() {
            0 => (f64::NAN, f64::NAN),
            1 => mean_ci(&d[0]),
            _ => mean_ci(&d.iter().map(|s| mean_of(s)).collect::<Vec<_>>()),
        }
    }

    fn json(&self) -> Value {
        let (mean, ci) = self.diff();
        let side = |s: &Side| json!({ "path": s.name, "gpu": s.gpu.json(), "cpu": { "mb_update": mean_of(&s.cpu[0]), "mb_render": mean_of(&s.cpu[1]) } });
        let per: Vec<f64> = self.diffs().iter().map(|s| mean_of(s)).collect();
        json!({
            "redraws": self.redraws,
            "hz": self.hz,
            "a": side(&self.sides[0]),
            "b": side(&self.sides[1]),
            "diff": { "ms": mean, "ci95": [mean - ci, mean + ci], "pairs": self.diffs().iter().map(Vec::len).sum::<usize>(), "sessions": per },
        })
    }

    fn text(&self) -> String {
        let [a, b] = &self.sides;
        let times = a.gpu.times();
        let n = a.gpu.sessions.len();
        let scope = if n > 1 { format!("95% CI over {n} browser sessions") } else { "95% CI over the repeats of one browser session".into() };
        let mut out = format!(
            "A/B, gpu ms at {} ({} alternating repeats of {} redraws at each, in each of {n} browser session{}; the median of each, averaged, ± {scope}):\n",
            and_list(&times),
            a.gpu.repeats(),
            self.redraws,
            if n == 1 { "" } else { "s" }
        );
        let width = a.name.len().max(b.name.len());
        for (tag, s) in [("A", a), ("B", b)] {
            let (ms, ci) = s.gpu.ms();
            let at: Vec<String> = (0..times.len()).map(|k| format!("{} s {:.2}", times[k], s.gpu.moment(k).0)).collect();
            out += &format!("  {tag}  {:<width$}  {ms:.2} ± {ci:.2}   ({})\n", s.name, at.join("  "));
        }
        let diffs = self.diffs();
        let (mean, ci) = self.diff();
        let base = a.gpu.ms().0;
        let verdict = if mean - ci > 0.0 {
            "B is slower"
        } else if mean + ci < 0.0 {
            "B is faster"
        } else {
            "no difference this can tell apart (the interval includes 0)"
        };
        let over = if n > 1 {
            let per: Vec<String> = diffs.iter().map(|s| format!("{:+.3}", mean_of(s))).collect();
            format!("{n} sessions ({})", per.join(", "))
        } else {
            format!("{} pairs", diffs.iter().map(Vec::len).sum::<usize>())
        };
        out += &format!("  B − A  {mean:+.3} ms ({:+.1}%), 95% CI {:+.3} … {:+.3} ms over {over}: {verdict}\n", 100.0 * mean / base, mean - ci, mean + ci);
        out += &format!(
            "  cpu ms mean  update A {:.3} B {:.3}  render A {:.3} B {:.3}  (the frames stepped to get there; no CI)\n",
            mean_of(&a.cpu[0]),
            mean_of(&b.cpu[0]),
            mean_of(&a.cpu[1]),
            mean_of(&b.cpu[1])
        );
        out
    }
}

/// The mean: CPU ms come at the browser's timer resolution (0.1 ms), so a median is mostly 0 or 0.1.
fn mean_of(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// How a build is named in the A/B report: as given, or its file name if that's long.
fn short_name(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    if s.len() <= 40 { s } else { p.file_name().map_or(s, |f| f.to_string_lossy().into_owned()) }
}

/// p50, p95 and max, as `mb.stats()` computes them (stats.ts `spread`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Spread {
    p50: f64,
    p95: f64,
    max: f64,
}

fn spread(values: impl IntoIterator<Item = f64>) -> Option<Spread> {
    let mut v: Vec<f64> = values.into_iter().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let at = |p: f64| v[((p * v.len() as f64) as usize).min(v.len() - 1)];
    Some(Spread { p50: at(0.5), p95: at(0.95), max: v[v.len() - 1] })
}

impl Spread {
    fn json(s: Option<Spread>) -> Value {
        s.map_or(Value::Null, |s| json!({ "p50": s.p50, "p95": s.p95, "max": s.max }))
    }
}

struct Report<'a> {
    /// Wall-clock seconds the run took.
    secs: f64,
    hz: f64,
    /// Ran on the browser's animation frames (else on the stepping timer).
    raf: bool,
    /// Seconds into the run where startup ends (`--from`).
    from: f64,
    /// Every frame of the run, each with its `interval` (ms from the previous frame's start; null for the first).
    frames: &'a [Value],
    latency: &'a Value,
    /// The GPU at each `--gpu-at` moment, in each session.
    gpu: &'a Gpu,
    verify: &'a Value,
    /// The page's per-frame budgets (`mb.stats().budgets`).
    budgets: &'a Value,
    /// What else was going on on this machine that slows a browser's frames
    /// (on battery, other headless browsers), said when the browser set the pace.
    machine: String,
}

/// Fields of a frame worth summarizing, in the order they're printed.
const COST: [&str; 4] = ["mb_update", "mb_render", "present", "gpu"];
const DREW: [&str; 11] = ["draws", "draws2d", "triangles", "reflected", "casters", "particles", "quads2d", "textureBytes", "textureBudget", "textures", "textureLimit"];
/// Latency stages (latency.ts `report`): received − stamp, frameStart − received, submitted − stamp,
/// and the page's own part, submitted − received.
const STAGES: [(&str, &str, &str); 4] =
    [("deliver", "received", "stamp"), ("toFrame", "frameStart", "received"), ("toSubmit", "submitted", "stamp"), ("receivedToSubmit", "submitted", "received")];

impl Report<'_> {
    /// How many frames, from the first, are startup: those whose interval
    /// began before `from` (so a stall that starts then, compiling shaders
    /// for the first frames, is startup however long it lasts).
    fn startup(&self) -> usize {
        let mut t = 0.0;
        self.frames
            .iter()
            .take_while(|f| {
                let began = t;
                t += f["interval"].as_f64().unwrap_or(0.0);
                began < self.from * 1000.0
            })
            .count()
    }

    /// The frames after startup, which the summaries are about.
    fn steady(&self) -> &[Value] {
        &self.frames[self.startup()..]
    }

    fn intervals(frames: &[Value]) -> Vec<f64> {
        frames.iter().filter_map(|f| f["interval"].as_f64()).collect()
    }

    fn field(&self, k: &str) -> Option<Spread> {
        spread(self.steady().iter().filter_map(|f| f[k].as_f64()))
    }


    /// Latency events grouped by kind ("touch down", "touch move", "tilt", …), with each stage's spread.
    fn latency_by_kind(&self) -> Vec<(String, usize, [Option<Spread>; 4])> {
        let events = self.latency["events"].as_array().cloned().unwrap_or_default();
        let mut kinds: Vec<String> = Vec::new();
        let name = |e: &Value| {
            let (g, k) = (e["group"].as_str().unwrap_or("?"), e["kind"].as_str().unwrap_or("?"));
            if g == k { g.to_string() } else { format!("{g} {k}") }
        };
        for e in &events {
            let n = name(e);
            if !kinds.contains(&n) {
                kinds.push(n);
            }
        }
        kinds
            .into_iter()
            .map(|k| {
                let es: Vec<&Value> = events.iter().filter(|e| name(e) == k).collect();
                let stage = |(_, a, b): (&str, &str, &str)| spread(es.iter().filter_map(|e| Some(e[a].as_f64()? - e[b].as_f64()?)));
                (k, es.len(), STAGES.map(stage))
            })
            .collect()
    }

    /// Long frames after startup, and during it (both against the median after startup).
    fn long_frames(&self) -> [usize; 2] {
        let steady = Self::intervals(self.steady());
        let Some(s) = spread(steady.iter().copied()) else { return [0, 0] };
        let long = |v: &[f64]| v.iter().filter(|&&i| i > s.p50 * LONG_FRAME).count();
        [long(&steady), long(&Self::intervals(&self.frames[..self.startup()]))]
    }

    /// The browser, not the game, set the frame rate: frames came much less
    /// often than `hz` asked while the game's own work per frame (CPU p95 of
    /// its callbacks and presenting, plus the timed GPU) was a small part of
    /// the interval. Returns (fps, the game's ms a frame, CPU, GPU or None).
    fn browser_paced(&self) -> Option<(f64, f64, f64, Option<f64>)> {
        let interval = spread(Self::intervals(self.steady()))?;
        let want = 1000.0 / self.hz;
        let cpu: f64 = ["mb_update", "mb_render", "present"].iter().filter_map(|k| self.field(k)).map(|s| s.p95).sum();
        let gpu = Some(self.gpu.ms().0).filter(|g| g.is_finite());
        let work = cpu + gpu.unwrap_or(0.0);
        (interval.p50 > want * 1.35 && work < interval.p50 * 0.5).then_some((1000.0 / interval.p50, work, cpu, gpu))
    }

    /// "  pacing …" when [`Report::browser_paced`], else nothing.
    fn pacing_line(&self) -> String {
        let Some((fps, work, cpu, gpu)) = self.browser_paced() else { return String::new() };
        let gpu = gpu.map_or(" (GPU not timed)".to_string(), |g| format!(" + GPU {g:.2}"));
        let why = if self.machine.is_empty() { "headless Chrome slows its frames when the machine is busy (other browsers, builds)".to_string() } else { self.machine.clone() };
        format!(
            "  pacing        the browser paced frames at {fps:.1} fps, not the {} Hz asked for: the game used ~{work:.1} ms a frame (CPU p95 {cpu:.2}{gpu}) of the {:.1} it has, so not the game's fault.\n                {why}; rerun when it's quieter. The cpu and gpu lines are the game's own cost\n",
            self.hz,
            1000.0 / self.hz
        )
    }

    fn json(&self) -> Value {
        let mut summary = serde_json::Map::new();
        summary.insert("interval".into(), Spread::json(spread(Self::intervals(self.steady()))));
        for k in COST.iter().chain(&DREW) {
            summary.insert(k.to_string(), Spread::json(self.field(k)));
        }
        let latency: serde_json::Map<String, Value> = self
            .latency_by_kind()
            .into_iter()
            .map(|(k, n, s)| {
                let mut m = serde_json::Map::from_iter([("events".to_string(), json!(n))]);
                for (i, (stage, ..)) in STAGES.iter().enumerate() {
                    m.insert(stage.to_string(), Spread::json(s[i]));
                }
                (k, Value::Object(m))
            })
            .collect();
        let (k, [long, startup_long]) = (self.startup(), self.long_frames());
        json!({
            "seconds": self.secs,
            "hz": self.hz,
            "clock": if self.raf { "raf" } else { "timer" },
            // The browser set a slower pace than `hz` while the game's work was small.
            "browserPaced": self.browser_paced().map(|(fps, work, ..)| json!({ "fps": fps, "gameMs": work, "machine": self.machine })),
            "frames": self.frames.len(),
            "longFrames": long,
            "startup": { "seconds": self.from, "frames": k, "longFrames": startup_long, "interval": Spread::json(spread(Self::intervals(&self.frames[..k]))) },
            "intervals": self.frames.iter().map(|f| f["interval"].clone()).collect::<Vec<_>>(),
            "summary": summary,
            "latencyByKind": latency,
            "gpu": self.gpu.json(),
            "budget": budget::summary(self.steady(), self.budgets).json(),
            "stats": { "frames": self.frames },
            "latency": self.latency,
            "verify": self.verify,
        })
    }

    /// The default report: one short block (--verbose for every line, --json for everything).
    fn short(&self, game: &str) -> String {
        let interval = spread(Self::intervals(self.steady()));
        let (k, [long, _]) = (self.startup(), self.long_frames());
        let p95 = |k: &str| self.field(k).map_or("–".to_string(), |s| format!("{:.2}", s.p95));
        let fps = interval.map_or("–".to_string(), |s| format!("p50 {:.1} p95 {:.1} max {:.1} ms ({:.1} fps)", s.p50, s.p95, s.max, 1000.0 / s.p50));
        let mut out = format!("{game}: {:.1} s live at {} Hz, {} frames: {fps}, {long} long", self.secs, self.hz, self.frames.len());
        if k > 0 {
            out += &format!(" (+{k} startup frames)");
        }
        out += "\n";
        out += &self.pacing_line();
        out += &format!("  cpu ms p95    update {}  render {}  present {}\n", p95("mb_update"), p95("mb_render"), p95("present"));
        if !self.gpu.sessions.is_empty() {
            let (ms, ci) = self.gpu.ms();
            let n = self.gpu.sessions.len();
            let scope = if n > 1 { format!("95% CI over {n} browser sessions") } else { "the ± is within one browser session only: --final for numbers to keep".into() };
            out += &format!("  gpu ms        {ms:.2} ± {ci:.2} at {}  ({scope})\n", and_list(&self.gpu.times()));
        }
        out += &drew_line(|k| self.field(k));
        out += &budget::summary(self.steady(), self.budgets).text();
        let kinds = self.latency_by_kind();
        if !kinds.is_empty() {
            let each: Vec<String> = kinds.iter().map(|(k, n, s)| format!("{k} ×{n} {}", s[3].map_or("–".to_string(), |s| format!("{:.1}/{:.1}", s.p50, s.p95)))).collect();
            out += &format!("  latency       received→submit p50/p95 ms: {}\n", each.join(", "));
        }
        out += &match self.verify["match"].as_bool() {
            Some(true) => format!("  verify        match ({} frames)   (--verbose for every line, --json for everything)\n", self.verify["frames"]),
            _ => format!("  verify        MISMATCH from frame {} ({} frames)\n", self.verify["firstMismatch"], self.verify["frames"]),
        };
        out
    }

    fn text(&self, game: &str) -> String {
        let ms = |s: Option<Spread>| s.map_or("–".to_string(), |s| format!("p50 {:.1}  p95 {:.1}  max {:.1}", s.p50, s.p95, s.max));
        let p95 = |k: &str| self.field(k).map_or("–".to_string(), |s| format!("{:.2}", s.p95));
        let mut out = String::new();
        let clock = if self.raf { "on the browser's animation frames (headless Chrome: 60 Hz)" } else { "stepped on a timer" };
        out += &format!("{game}: {:.1} s live at {} Hz, {clock}, {} frames\n", self.secs, self.hz, self.frames.len());
        let interval = spread(Self::intervals(self.steady()));
        let fps = interval.map_or(String::new(), |s| format!("  ({:.1} fps)", 1000.0 / s.p50));
        let (k, [long, startup_long]) = (self.startup(), self.long_frames());
        out += &format!("  interval ms   {}{fps}, {long} long (> {LONG_FRAME}× median)\n", ms(interval));
        out += &self.pacing_line();
        if k > 0 {
            let max = spread(Self::intervals(&self.frames[..k])).map_or("–".to_string(), |s| format!("{:.1}", s.max));
            out += &format!("  startup       first {} s: {k} frames, {startup_long} long, max {max} ms (not in the lines above or below; --from)\n", self.from);
        }
        out += &format!("  cpu ms p95    update {}  render {}  present {}\n", p95("mb_update"), p95("mb_render"), p95("present"));
        if !self.gpu.sessions.is_empty() {
            out += &self.gpu.text("gpu ms");
        }
        out += &drew_line(|k| self.field(k));
        out += &budget::summary(self.steady(), self.budgets).text();
        let kinds = self.latency_by_kind();
        if kinds.is_empty() {
            out += "  latency       no input\n";
        }
        let synthetic = kinds.iter().any(|k| !k.0.starts_with("tilt"));
        for (k, count, s) in kinds {
            let pair = |s: Option<Spread>| s.map_or("–".to_string(), |s| format!("{:.1}/{:.1}", s.p50, s.p95));
            out += &format!(
                "  latency {k:<12} ×{count:<4} p50/p95 ms: deliver {}  to frame {}  to submit {}  (received→submit {})\n",
                pair(s[0]),
                pair(s[1]),
                pair(s[2]),
                pair(s[3])
            );
        }
        if synthetic {
            out += "  note          these touches are synthetic (DevTools): deliver, and so to submit, is mostly Chrome's input plumbing,\n                not a phone's (SPEC §5.2 has an iPhone's); received→submit is the page's own part (waiting for a frame, running it)\n";
        }
        let v = self.verify;
        out += &match v["match"].as_bool() {
            Some(true) => format!("  verify        match ({} frames)\n", v["frames"]),
            _ => format!("  verify        MISMATCH from frame {} ({} frames)\n", v["firstMismatch"], v["frames"]),
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_match_the_runtimes() {
        let s = spread((1..=100).map(f64::from)).unwrap();
        assert_eq!(s, Spread { p50: 51.0, p95: 96.0, max: 100.0 }, "stats.ts: v[floor(p·n)]");
        assert_eq!(spread([f64::NAN]), None);
    }

    static BUDGETS: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!({ "draws": 200, "triangles": 150000, "skinned": 16, "particles": 5000 }));

    fn report_of<'a>(frames: &'a [Value], latency: &'a Value, gpu: &'a Gpu, verify: &'a Value) -> Report<'a> {
        Report { secs: 2.0, hz: 60.0, raf: true, from: 0.0, frames, latency, gpu, verify, budgets: &BUDGETS, machine: String::new() }
    }

    #[test]
    fn a_run_the_browser_paced_at_30_fps_says_its_not_the_game() {
        let frames: Vec<Value> = (0..60).map(|i| json!({ "interval": if i == 0 { Value::Null } else { json!(33.3) }, "mb_update": 0.4, "mb_render": 0.2, "present": 0.7 })).collect();
        let gpu = Gpu { sessions: vec![vec![Moment { at: 1.0, frame: 60, runs: vec![vec![4.6; 5]; 3] }]] };
        let (latency, verify) = (json!([]), json!({ "match": true, "frames": 60 }));
        let mut r = report_of(&frames, &latency, &gpu, &verify);
        r.machine = "2 other headless browsers were running".into();
        let text = r.short("koi");
        assert!(text.contains("  pacing        the browser paced frames at 30.0 fps, not the 60 Hz asked for: the game used ~5.9 ms a frame (CPU p95 1.30 + GPU 4.60) of the 16.7 it has, so not the game's fault.\n                2 other headless browsers were running; rerun"), "{text}");
        assert_eq!(r.json()["browserPaced"]["fps"].as_f64().map(|f| f.round()), Some(30.0));
        // A game that takes the time itself is the game's.
        let slow: Vec<Value> = frames.iter().map(|f| json!({ "interval": f["interval"], "mb_update": 14.0, "mb_render": 1.0, "present": 0.7 })).collect();
        let r = report_of(&slow, &latency, &gpu, &verify);
        assert!(!r.short("koi").contains("pacing"));
        assert!(r.json()["browserPaced"].is_null());
    }

    #[test]
    fn the_summary_names_long_frames_latency_by_kind_and_the_verdict() {
        let intervals = [16.7, 16.6, 33.4, 16.7];
        let frames: Vec<Value> = (0..4).map(|i| json!({ "interval": intervals[i], "mb_update": 0.1 * i as f64, "draws": 10, "draws2d": 1, "triangles": 20_000, "casters": 3, "reflected": 0, "particles": 2 })).collect();
        let ev = |kind: &str, stamp: f64| json!({ "group": "touch", "kind": kind, "stamp": stamp, "received": stamp + 1.0, "frameStart": stamp + 5.0, "submitted": stamp + 20.0 });
        let latency = json!({ "events": [ev("down", 0.0), ev("move", 10.0), ev("move", 20.0), { "group": "tilt", "kind": "tilt", "stamp": 0.0, "received": 0.0, "frameStart": null, "submitted": 3.0 }] });
        let gpu = Gpu { sessions: vec![vec![Moment { at: 1.0, frame: 60, runs: vec![vec![2.0, 2.2, 2.1], vec![2.3, 2.4, 2.2]] }, Moment { at: 2.0, frame: 120, runs: vec![vec![3.0, 3.0, 3.1], vec![3.3, 3.1, 3.2]] }]] };
        let verify = json!({ "match": true, "frames": 120 });
        let r = report_of(&frames, &latency, &gpu, &verify);
        assert_eq!(r.long_frames(), [1, 0]);
        let text = r.text("Mistwood");
        assert!(text.contains("Mistwood: 2.0 s live at 60 Hz"), "{text}");
        assert!(text.contains("10+1 draws  20.0k tris  0 reflected  3 casters  2 particles"), "{text}");
        assert!(text.contains("latency touch move   ×2    p50/p95 ms: deliver 1.0/1.0  to frame 4.0/4.0  to submit 20.0/20.0  (received→submit 19.0/19.0)"), "{text}");
        assert!(text.contains("latency tilt") && text.contains("to frame –"), "{text}");
        assert!(text.contains("note          these touches are synthetic (DevTools)"), "{text}");
        assert!(
            text.contains("gpu ms        2.65 ± 1.27  (one browser session, 2 repeats of 25 redraws of the scene at 1 and 2 s, the median of each, averaged: the ± is within it only; separate runs differ by more, --final)\n                1 s 2.20 ± 1.27   2 s 3.10 ± 1.27\n"),
            "{text}"
        );
        assert!(text.contains("verify        match (120 frames)"), "{text}");
        assert!(text.contains("  budget        every frame within the skill's (draws ≤ 200, triangles ≤ 150k"), "{text}");
        assert!(!text.contains("startup"), "--from 0: no startup: {text}");
        // The default report: one short block.
        let short = r.short("Mistwood");
        assert!(short.lines().count() <= 8, "{short}");
        assert!(short.starts_with("Mistwood: 2.0 s live at 60 Hz, 4 frames: p50 16.7"), "{short}");
        assert!(short.contains("gpu ms        2.65 ± 1.27 at 1 and 2 s  (the ± is within one browser session only: --final for numbers to keep)"), "{short}");
        assert!(short.contains("latency       received→submit p50/p95 ms: touch down ×1 19.0/19.0, touch move ×2 19.0/19.0"), "{short}");
        assert!(short.contains("verify        match (120 frames)   (--verbose"), "{short}");
        let q = Quick { gpu: &gpu, frames: &frames, budgets: &BUDGETS, secs: 21.4 };
        let qt = q.text("Mistwood");
        assert!(qt.starts_with("Mistwood (quick, 21 s): gpu ms 2.65 ± 1.27  (1 s 2.20  2 s 3.10; 2 repeats in one browser"), "{qt}");
        assert!(qt.contains("10+1 draws  20.0k tris"), "{qt}");
        assert_eq!(q.json()["quick"], true);
        let j = r.json();
        assert_eq!(j["latencyByKind"]["touch down"]["events"], 1);
        assert_eq!(j["latencyByKind"]["touch down"]["receivedToSubmit"]["p50"], 19.0);
        assert_eq!(j["summary"]["interval"]["max"], 33.4);
        assert_eq!(j["intervals"], json!(intervals));

        let verify = json!({ "match": false, "frames": 120, "firstMismatch": 37 });
        assert!(report_of(&frames, &latency, &gpu, &verify).text("x").contains("MISMATCH from frame 37"));
        let tilt_only = json!({ "events": [{ "group": "tilt", "kind": "tilt", "stamp": 0.0, "received": 0.0, "frameStart": 1.0, "submitted": 3.0 }] });
        assert!(!report_of(&frames, &tilt_only, &gpu, &verify).text("x").contains("synthetic"), "tilt samples come from mb.sensors, not DevTools");
    }

    #[test]
    fn gpu_moments_default_to_thirds_of_the_run() {
        assert_eq!(gpu_moments(&[], 10.0).unwrap(), [3.3, 6.7, 10.0]);
        assert_eq!(gpu_moments(&[20.0, 5.0, 5.0], 10.0).unwrap(), [5.0, 20.0], "sorted, once each, past the run is fine");
        assert!(gpu_moments(&[0.0], 10.0).unwrap_err().contains("--gpu-at 0"));
        assert_eq!(and_list(&[3.3, 6.7, 10.0]), "3.3, 6.7 and 10 s");
        assert_eq!(median([3.0, 1.0, 2.0, 10.0]), 2.5);
    }

    /// A moment whose repeats' medians are `base + wobble[i]`.
    fn moment(base: f64, wobble: &[f64]) -> Moment {
        Moment { at: 5.0, frame: 300, runs: wobble.iter().map(|w| vec![base + w; 3]).collect() }
    }

    #[test]
    fn the_gpu_interval_is_over_sessions_when_there_are_several() {
        // Three browser sessions of one build: tight within each, apart from each other
        // (as two runs of a builder's game were: 3.96 ± 0.12 and 5.08 ± 0.43 ms).
        let wobble = [0.0, 0.02, -0.02, 0.01, -0.01];
        let gpu = Gpu { sessions: [3.96, 4.89, 5.08].iter().map(|&b| vec![moment(b, &wobble)]).collect() };
        let within = gpu.within();
        let (ms, ci) = gpu.ms();
        assert!((ms - 4.643).abs() < 1e-3, "{ms}");
        assert!(within < 0.05 && ci > 1.0, "the sessions' spread, not the repeats': ± {ci} vs ± {within}");
        for s in &gpu.sessions {
            let (m, _) = overall(s);
            assert!((m - ms).abs() < ci, "each session's number is inside the interval: {m} in {ms} ± {ci}");
        }
        let text = gpu.text("gpu ms");
        assert!(text.starts_with(&format!("  gpu ms        4.64 ± {ci:.2}  (95% CI over 3 browser sessions; in each, 5 repeats of 25 redraws of the scene at 5 s")), "{text}");
        assert!(text.contains("\n                sessions 3.96, 4.89, 5.08  (each ± 0.02 over its own repeats)\n"), "{text}");
        let j = gpu.json();
        assert_eq!((j["scope"].as_str(), j["sessions"].as_array().map(Vec::len)), (Some("sessions"), Some(3)));
        assert_eq!(j["at"][0]["ms"], j["ms"]);
        // One session: the repeats' interval, said to be only that.
        let one = Gpu { sessions: gpu.sessions[..1].to_vec() };
        assert_eq!(one.json()["scope"], "repeats");
        assert!(one.text("gpu ms").contains("the ± is within it only"));
    }

    #[test]
    fn ab_gives_the_difference_with_a_confidence_interval() {
        let side = |name: &str, m: Moment| Side { name: name.into(), gpu: Gpu { sessions: vec![vec![m]] }, cpu: [vec![0.2, 0.3, 0.2], vec![0.1]] };
        let wobble = [0.0, 0.05, -0.04, 0.02, 0.01, -0.03];
        // B is 0.3 ms faster, give or take a little.
        let noise = [0.01, -0.02, 0.0, 0.015, -0.01, 0.005];
        let b: Vec<f64> = wobble.iter().zip(noise).map(|(w, n)| w + n).collect();
        let r = AbReport { sides: [side("games/x", moment(4.0, &wobble)), side("old.mbx", moment(3.7, &b))], redraws: 3, hz: 60.0 };
        let (mean, ci) = r.diff();
        assert!((mean + 0.3).abs() < 0.01 && ci > 0.0 && ci < 0.05, "{mean} ± {ci}");
        let text = r.text();
        assert!(text.contains("B − A  -0.300 ms (-7.5%), 95% CI") && text.contains(" ms over 6 pairs: B is faster"), "{text}");
        assert!(text.ends_with("(the frames stepped to get there; no CI)\n") && text.contains(": B is faster"), "{text}");
        assert!(text.contains("  A  games/x  4.00 ± 0.03   (5 s 4.00)"), "{text}");
        let j = r.json();
        assert_eq!(j["diff"]["pairs"], 6);
        assert!(j["diff"]["ci95"][1].as_f64().unwrap() < 0.0);
        // The same build twice: the interval straddles 0.
        let r = AbReport { sides: [side("a", moment(4.0, &wobble)), side("b", moment(4.0, &noise))], redraws: 3, hz: 60.0 };
        assert!(r.text().contains("no difference this can tell apart"), "{}", r.text());

        // Three sessions, each a different machine state for both builds (they move together):
        // B is 0.3 ms faster in each, and the interval is over the sessions' differences.
        let sessions = |base: f64| Gpu { sessions: [0.0, 1.1, 0.4].iter().map(|&shift| vec![moment(base + shift, &wobble)]).collect() };
        let mut r = AbReport { sides: [side("a", moment(0.0, &wobble)), side("b", moment(0.0, &wobble))], redraws: 3, hz: 60.0 };
        r.sides[0].gpu = sessions(4.0);
        r.sides[1].gpu = Gpu { sessions: [0.0, 1.1, 0.4].iter().zip([0.01, -0.02, 0.0]).map(|(&shift, n)| vec![moment(3.7 + shift + n, &noise)]).collect() };
        let (mean, ci) = r.diff();
        assert!((mean + 0.3).abs() < 0.02 && ci < 0.1, "the machine's drift cancels in the difference: {mean} ± {ci}");
        assert!(r.sides[0].gpu.ms().1 > 1.0, "but not in each build's own number: {:?}", r.sides[0].gpu.ms());
        let text = r.text();
        assert!(text.contains("in each of 3 browser sessions") && text.contains("over 3 sessions (") && text.contains("B is faster"), "{text}");
        assert_eq!(r.json()["diff"]["sessions"].as_array().map(Vec::len), Some(3));
        assert!((t95(5) - 2.571).abs() < 1e-9 && t95(500) == 1.96);
    }

    #[test]
    fn ab_params_split_into_two_sides() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(ab_params(&s(&["shafts=1,0", "gpu_off=0,128"])).unwrap(), [s(&["shafts=1", "gpu_off=0"]), s(&["shafts=0", "gpu_off=128"])]);
        assert_eq!(ab_params(&s(&["note=a,b,c"])).unwrap(), [s(&["note=a"]), s(&["note=b,c"])], "B's value is the rest");
        assert!(ab_params(&s(&["shafts=1"])).unwrap_err().contains("expected NAME=A,B"));
        assert!(ab_params(&s(&["shafts=1,1"])).unwrap_err().contains("the same"));
        assert!(ab_params(&s(&["bad name=1,0"])).is_err());
        assert_eq!(git_url("git+https://github.com/terracatta/maimbrain-kit#3f2a9c"), Some("https://github.com/terracatta/maimbrain-kit"));
        assert_eq!(git_url("git+https://github.com/terracatta/maimbrain-kit?branch=main#3f2a9c"), Some("https://github.com/terracatta/maimbrain-kit"));
        assert_eq!(git_url("registry+https://github.com/rust-lang/crates.io-index"), None);
        assert_eq!(toml_string(r#"C:\kit "x""#), r#""C:\\kit \"x\"""#);
    }

    #[test]
    fn startup_frames_are_reported_apart() {
        // A first frame held up 3.7 s compiling shaders, then a steady 60 Hz with one hitch.
        let mut intervals = vec![Value::Null, json!(3666.6)];
        intervals.extend((0..60).map(|i| json!(if i == 40 { 50.0 } else { 16.7 })));
        let frames: Vec<Value> = intervals.iter().enumerate().map(|(i, v)| json!({ "frame": i + 1, "interval": v, "mb_update": if i < 2 { 9.0 } else { 0.2 } })).collect();
        let (latency, gpu, verify) = (json!({ "events": [] }), Gpu::default(), json!({ "match": true, "frames": 62 }));
        let r = Report { from: 0.5, ..report_of(&frames, &latency, &gpu, &verify) };
        assert_eq!(r.startup(), 2, "the stall began in the first 0.5 s");
        assert_eq!(r.long_frames(), [1, 1]);
        let text = r.text("Mistwood");
        assert!(text.contains("interval ms   p50 16.7  p95 16.7  max 50.0  (59.9 fps), 1 long"), "{text}");
        assert!(text.contains("startup       first 0.5 s: 2 frames, 1 long, max 3666.6 ms"), "{text}");
        assert!(text.contains("update 0.20"), "startup's costs aren't in the summary: {text}");
        let j = r.json();
        assert_eq!((j["startup"]["frames"].clone(), j["longFrames"].clone()), (json!(2), json!(1)));
        // At 60 Hz, the first 0.5 s is the first 31 frames.
        let steady: Vec<Value> = (0..100).map(|i| json!({ "interval": if i == 0 { Value::Null } else { json!(1000.0 / 60.0) } })).collect();
        assert_eq!(Report { from: 0.5, ..report_of(&steady, &latency, &gpu, &verify) }.startup(), 31);
    }
}
