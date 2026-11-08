//! `mb difficulty`: Studio's difficulty bot (docs/STUDIO-2.md §8.1). Plays
//! seeded rounds of a Studio 2.0 game per kind of player (first-timer,
//! casual, good, expert), with the engine's bot (`game/bot.lua`, or the
//! quick path's default) and the person its `synth::Skill` adds, and
//! reports how long rounds last against each preset's target, how they end,
//! whether skill matters, and how the events the game handles are paced.
//!
//!     mb difficulty <bundle.mbx | project> [--presets first-timer,good] [--rounds 20|40]
//!         [--target first-timer=20-45,good=60-180] [--level] [--wins first-timer=15-40]
//!         [--runner auto|deno|native] [--report debug/bot-report.tsv] [--json]
//!
//! **Where rounds run** (spike S5). In one long-lived Deno worker (the
//! runtime's `botrun.js`: mb-host and the game's wasm compiled once, a fresh
//! session per round, frames never rendered), or natively: this mb carries
//! engine 2.0 built from the source its published wasm was built from, run
//! on the SDK's stubbed host (`studio_engine2::bot::native_round`), 2.5×
//! cheaper. Native is used (`--runner auto`) only for a 2D game whose
//! bundle carries exactly this mb's published engine, and only while every
//! round says its logic read nothing back from the host (text metrics); one
//! round per preset is then played again in Deno, and any difference sends
//! the whole run to Deno.
//!
//! **Rounds and caps.** Each preset plays seeds 1, 2, … (the same seeds for
//! every preset), each round capped at the preset's cap: its target's top
//! (first-timer 45 s, good 180 s), or casual 90 s and expert 180 s.
//!
//! **Early stopping** (the rule S5 left open). Rounds go round-robin over the
//! presets still playing; a preset stops at `--rounds` (20; 40 on request)
//! or, from its 8th round, once more rounds wouldn't change the answer:
//! - its target is decided: the median's distribution-free ~88% interval
//!   (order statistics) lies inside the band (on), below it (too short), or
//!   at its cap (too long, or past the cap: can't tell);
//! - a preset without a target, once each comparison with the presets
//!   beside it is decided (their medians' intervals, at the shorter cap,
//!   don't overlap, or both sit at the cap), which is when "skill matters"
//!   is decided;
//! - or its median has settled: from its 12th round, the p50 moved by at
//!   most a second (or 5%) over its last five.
//!
//! `--rounds` 20 or 40 with no stopping (`--all`) plays every round.
//!
//! **Levels.** A game played to a win (light every lantern, reach the
//! flag) is judged on how often each preset wins, not how long it lasts:
//! first-timer 15–40%, casual 30–60%, good 60–85%, expert 85% or more
//! (`--wins` sets others), with the time its wins take. Every preset is
//! then capped at 180 s (or its target's top, if longer), so a good
//! player's winning run has to fit in it. A game is a level when its
//! `studio.toml` says `level = true` (or `--level`), or when most of the
//! best preset's rounds are won: those runs then play on as a level (a
//! round that ended under the old cap is the same round, so only the ones
//! the cap cut short play again). A level's preset stops early once its
//! win rate's ~88% interval (Wilson) lies inside its band or outside it,
//! or from its 12th round once the win rate moved at most 5 points over
//! its last five.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Instant;

use maimbrain::bot::{Outcome, PresetReport, Report, Spread};
use serde_json::{Value, json};

#[derive(clap::Args)]
pub struct DifficultyArgs {
    /// The game: a Studio 2.0 bundle (.mbx) or project directory (built first).
    pub game: PathBuf,
    /// The kinds of player, least skilled first (default: all four).
    #[arg(long, value_delimiter = ',')]
    pub presets: Vec<String>,
    /// Rounds per preset at most: 20, or 40 for a final check.
    #[arg(long, default_value_t = 20)]
    pub rounds: usize,
    /// Round-length targets for a preset's p50, `name=lo-hi` in seconds (default first-timer=20-45,good=60-180).
    #[arg(long, value_delimiter = ',')]
    pub target: Vec<String>,
    /// Judge it as a level, on win rate (as `level = true` in studio.toml does; most rounds won does too).
    #[arg(long)]
    pub level: bool,
    /// A level's win-rate targets, `name=lo-hi` in % of rounds (default first-timer=15-40,casual=30-60,good=60-85,expert=85-100).
    #[arg(long, value_delimiter = ',')]
    pub wins: Vec<String>,
    /// Where rounds run: auto (native where it plays as Deno does), deno, or native.
    #[arg(long, default_value = "auto")]
    pub runner: String,
    /// Play every round (no early stopping).
    #[arg(long)]
    pub all: bool,
    /// Save the report's TSV here (and say what changed since the one there).
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Print one line of JSON.
    #[arg(long)]
    pub json: bool,
    /// A built runtime (runtime/dist) instead of the one in this mb (default: $MB_RUNTIME_DIR, if set).
    #[arg(long)]
    pub runtime: Option<PathBuf>,
}

pub const PRESETS: [&str; 4] = ["first-timer", "casual", "good", "expert"];
/// The kit's targets for a preset's p50 round length (s).
pub const TARGETS: [(&str, (f32, f32)); 2] = [("first-timer", (20.0, 45.0)), ("good", (60.0, 180.0))];
/// Caps for presets without a target (s); a target's top caps its own.
const CAPS: [(&str, f64); 4] = [("first-timer", 45.0), ("casual", 90.0), ("good", 180.0), ("expert", 180.0)];
/// The longest a round may be capped at (s).
pub const MAX_CAP: f64 = 300.0;
/// The kit's targets for a level: each preset's win rate (% of rounds).
pub const WIN_TARGETS: [(&str, (f32, f32)); 4] = [("first-timer", (15.0, 40.0)), ("casual", (30.0, 60.0)), ("good", (60.0, 85.0)), ("expert", (85.0, 100.0))];
/// A level's rounds are capped here (s), every preset's: time to finish it.
pub const LEVEL_CAP: f64 = 180.0;
/// A game whose best preset wins more than this share of its rounds (%) is a level.
const MOSTLY_WON: f64 = 50.0;
/// Rounds before a preset may stop early, and before its median may count as settled.
const MIN_ROUNDS: usize = 8;
const SETTLE_ROUNDS: usize = 12;

/// One round, as the engine's `studio-bot` line says.
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub preset: String,
    pub seed: u64,
    pub seconds: f64,
    pub score: f64,
    pub cause: String,
    pub won: bool,
    pub capped: bool,
    pub presses: u64,
    pub reads: bool,
    pub events: Vec<(String, f64)>,
    pub error: Option<String>,
    /// Game-seconds per wall-second, for the timings.
    pub wall_ms: f64,
}

impl Round {
    pub fn parse(v: &Value) -> Result<Round, String> {
        let num = |k: &str| v[k].as_f64().or_else(|| v[k].as_str().and_then(|s| s.parse().ok()));
        if v["preset"].is_null() || v["seconds"].is_null() {
            return Err(v["error"].as_str().map_or_else(|| format!("not a round: {v}"), str::to_string));
        }
        Ok(Round {
            preset: v["preset"].as_str().unwrap_or("").to_string(),
            seed: num("seed").unwrap_or(0.0) as u64,
            seconds: num("seconds").unwrap_or(0.0),
            score: num("score").unwrap_or(0.0),
            cause: v["cause"].as_str().unwrap_or("").to_string(),
            won: v["won"].as_bool().unwrap_or(false),
            capped: v["capped"].as_bool().unwrap_or(false),
            presses: num("presses").unwrap_or(0.0) as u64,
            reads: v["reads"].as_bool().unwrap_or(false),
            events: v["events"].as_array().into_iter().flatten().filter_map(|e| Some((e[0].as_str()?.to_string(), e[1].as_f64()?))).collect(),
            error: v["error"].as_str().map(str::to_string),
            wall_ms: num("wall_ms").unwrap_or(0.0),
        })
    }

    /// The same round, as far as play goes (native against Deno).
    fn same_play(&self, o: &Round) -> Result<(), String> {
        let diff = |what: &str, a: String, b: String| if a == b { Ok(()) } else { Err(format!("{} seed {}: {what} {a} natively, {b} in Deno", self.preset, self.seed)) };
        diff("length", format!("{:.3}", self.seconds), format!("{:.3}", o.seconds))?;
        diff("score", self.score.to_string(), o.score.to_string())?;
        diff("cause", self.cause.clone(), o.cause.clone())?;
        diff("presses", self.presses.to_string(), o.presses.to_string())?;
        diff("events", self.events.len().to_string(), o.events.len().to_string())?;
        diff("won", self.won.to_string(), o.won.to_string())
    }
}

/// Plays rounds: (preset, seed, cap) → the round.
pub trait Runner {
    fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String>;
}

/// The long-lived Deno worker (runtime/src/botrun.ts).
pub struct Deno {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pub setup_s: f64,
    work: PathBuf,
}

impl Deno {
    /// Stages the bundle (its wasm metered, as the app serves it) and starts the worker.
    pub fn start(b: &mb_format::Bundle, work: &Path, runtime_dir: Option<&Path>) -> Result<Deno, String> {
        let t0 = Instant::now();
        let game = work.join("game");
        crate::unpack(b, &game)?;
        std::fs::write(game.join("boot.json"), crate::boot_json(&b.manifest, 0, "2026-01-01").to_string()).map_err(|e| e.to_string())?;
        let runtime = work.join("runtime");
        std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
        for (name, embedded) in crate::replay::BOT_FILES {
            let path = runtime.join(name);
            match runtime_dir {
                Some(dir) => std::fs::copy(dir.join(name), &path).map(|_| ()).map_err(|e| format!("{}: {e}", dir.join(name).display()))?,
                None if embedded.is_empty() => return Err(crate::replay::missing_runtime()),
                None => std::fs::write(&path, embedded).map_err(|e| e.to_string())?,
            }
        }
        let deno = std::env::var_os("MB_DENO").map_or_else(|| PathBuf::from("deno"), PathBuf::from);
        let mut child = Command::new(&deno)
            .args(["run", "--quiet", "--no-prompt", "--no-config", "--no-npm", "--no-remote"])
            .arg(format!("--allow-read={}", work.display()))
            .arg(runtime.join("botrun.js"))
            .arg(&game)
            .env("DENO_DIR", work.join("deno"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { format!("mb difficulty needs Deno ({} wasn't found): install it, or set MB_DENO", deno.display()) } else { e.to_string() })?;
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        stdout.read_line(&mut first).map_err(|e| e.to_string())?;
        if !first.contains("\"ready\"") {
            let _ = child.kill();
            return Err(format!("the bot runner didn't start: {}", first.trim()));
        }
        Ok(Deno { child, stdin, stdout, setup_s: t0.elapsed().as_secs_f64(), work: work.to_path_buf() })
    }
}

impl Runner for Deno {
    fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String> {
        writeln!(self.stdin, "{}", json!({ "preset": preset, "seed": seed, "max": max })).map_err(|e| format!("the bot runner stopped: {e}"))?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if self.stdout.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("the bot runner stopped".into());
        }
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("the bot runner's output isn't JSON ({e}): {line}"))?;
        Round::parse(&v)
    }
}

impl Drop for Deno {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

/// Engine 2.0 natively, on the SDK's stubbed host.
pub struct Native {
    files: BTreeMap<String, Vec<u8>>,
}

impl Native {
    /// The project's files as the engine loads them from the bundle (`assets/`).
    pub fn new(b: &mb_format::Bundle) -> Result<Native, String> {
        let index = b.file(mb_format::studio::INDEX).ok_or("the bundle has no assets/files.txt (not a Studio 2.0 bundle)")?;
        let mut files = BTreeMap::new();
        for p in String::from_utf8_lossy(index).lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            files.insert(p.to_string(), b.file(&format!("assets/{p}")).unwrap_or_default().to_vec());
        }
        Ok(Native { files })
    }
}

impl Runner for Native {
    fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String> {
        let t = Instant::now();
        let (line, _) = studio_engine2::bot::native_round(&self.files, preset, seed, max, &[])?;
        let mut v: Value = serde_json::from_str(&line).map_err(|e| format!("the engine's line isn't JSON ({e}): {line}"))?;
        v["wall_ms"] = (t.elapsed().as_secs_f64() * 1000.0).into();
        Round::parse(&v)
    }
}

/// A preset's rounds and when it stopped.
#[derive(Clone, Debug)]
pub struct PresetRun {
    pub name: String,
    pub cap: f64,
    pub target: Option<(f32, f32)>,
    /// A level's win-rate band (%); the length target doesn't apply then.
    pub wins: Option<(f32, f32)>,
    pub rounds: Vec<Round>,
    pub stopped: Option<String>,
}

/// The p-th fraction's value of sorted values, as the SDK's `Spread`.
fn at(v: &[f64], p: usize) -> f64 {
    v[(p * v.len() / 100).min(v.len() - 1)]
}

fn sorted(v: impl IntoIterator<Item = f64>) -> Vec<f64> {
    let mut v: Vec<f64> = v.into_iter().collect();
    v.sort_by(f64::total_cmp);
    v
}

/// The median's distribution-free interval (~88%): order statistics k and
/// n + 1 − k of the sorted values, with the largest k whose binomial tail
/// P(Bin(n, ½) < k) is at most 6%.
pub fn median_interval(v: &[f64]) -> (f64, f64) {
    let v = sorted(v.iter().copied());
    let n = v.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let tail = |k: usize| -> f64 {
        // P(X ≤ k − 1) for X ~ Bin(n, 1/2).
        let mut c = 1.0f64;
        let mut sum = 0.0;
        for i in 0..k {
            if i > 0 {
                c = c * (n - i + 1) as f64 / i as f64;
            }
            sum += c;
        }
        sum / 2f64.powi(n as i32)
    };
    let mut k = 1;
    while k < n / 2 && tail(k + 1) <= 0.06 {
        k += 1;
    }
    (v[k - 1], v[n - k])
}

/// The win rate's ~88% interval (Wilson score, z 1.555, as the median's), in %.
pub fn win_interval(won: usize, n: usize) -> (f64, f64) {
    if n == 0 {
        return (0.0, 100.0);
    }
    let (z, n, p) = (1.555f64, n as f64, won as f64 / n as f64);
    let mid = (p + z * z / (2.0 * n)) / (1.0 + z * z / n);
    let half = z / (1.0 + z * z / n) * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt();
    (100.0 * (mid - half).max(0.0), 100.0 * (mid + half).min(1.0))
}

impl PresetRun {
    fn lengths(&self, cap: f64) -> Vec<f64> {
        self.rounds.iter().map(|r| r.seconds.min(cap)).collect()
    }

    /// % of rounds won (of the first `n`).
    fn win_rate(&self, n: usize) -> f64 {
        let n = n.min(self.rounds.len());
        100.0 * self.rounds[..n].iter().filter(|r| r.won).count() as f64 / n.max(1) as f64
    }

    /// p10, p50, p90 of the won rounds' length (s): the time a win takes.
    pub fn time_to_win(&self) -> Option<(f64, f64, f64)> {
        let v = sorted(self.rounds.iter().filter(|r| r.won).map(|r| r.seconds));
        (!v.is_empty()).then(|| (at(&v, 10), at(&v, 50), at(&v, 90)))
    }

    /// p10, p50, p90 of round length (s).
    pub fn spread(&self) -> (f64, f64, f64) {
        let v = sorted(self.lengths(self.cap));
        if v.is_empty() { (0.0, 0.0, 0.0) } else { (at(&v, 10), at(&v, 50), at(&v, 90)) }
    }

    /// What the target says: on, short, long, past the cap (can't tell),
    /// or not yet clear; None without a target.
    pub fn verdict(&self, sure: bool) -> Option<&'static str> {
        if let Some((lo, hi)) = self.wins {
            return Some(self.level_verdict(lo as f64, hi as f64, sure));
        }
        let (lo, hi) = self.target?;
        let (lo, hi) = (lo as f64, hi as f64);
        let (a, b) = if sure { median_interval(&self.lengths(self.cap)) } else {
            let p = self.spread().1;
            (p, p)
        };
        let at_cap = a >= self.cap - 0.05;
        Some(if at_cap && self.cap < lo {
            "past the cap (can't tell)"
        } else if at_cap {
            "too long (over half its rounds reach the cap)"
        } else if b < lo {
            "too short"
        } else if a > hi {
            "too long"
        } else if a >= lo && b <= hi {
            "on"
        } else {
            "not clear yet"
        })
    }

    /// A level's: its win rate in its band (on), under it (too hard, or too
    /// long when most rounds it didn't win ran out at the cap), over it (too
    /// easy), or not clear yet.
    fn level_verdict(&self, lo: f64, hi: f64, sure: bool) -> &'static str {
        let n = self.rounds.len();
        let won = self.rounds.iter().filter(|r| r.won).count();
        let (a, b) = if sure { win_interval(won, n) } else {
            let r = self.win_rate(n);
            (r, r)
        };
        let lost = n - won;
        let ran_out = self.rounds.iter().filter(|r| !r.won && r.capped).count();
        if b < lo {
            if 2 * ran_out > lost { "too long (its runs reach the cap before they win)" } else { "too hard" }
        } else if a > hi {
            "too easy"
        } else if a >= lo && b <= hi {
            "on"
        } else {
            "not clear yet"
        }
    }
}

/// Whether `b` does better than `a` (the preset below it), as the kit's
/// `assert_skill_matters` asks, at the shorter of their caps: on win rate
/// where the game can be won (as often: winning sooner), else on median
/// length or the share reaching the cap. ("yes" | "no" | "can't tell", why).
pub fn compare(a: &PresetRun, b: &PresetRun, winnable: bool) -> (&'static str, String) {
    let cap = a.cap.min(b.cap);
    let win = |p: &PresetRun| 100.0 * p.rounds.iter().filter(|r| r.won).count() as f64 / p.rounds.len().max(1) as f64;
    let capped = |p: &PresetRun| 100.0 * p.rounds.iter().filter(|r| r.seconds >= cap - 0.05 && !r.won).count() as f64 / p.rounds.len().max(1) as f64;
    let med = |p: &PresetRun| at(&sorted(p.lengths(cap)), 50);
    let (ma, mb) = (med(a), med(b));
    let (ca, cb) = (capped(a), capped(b));
    let why = format!("p50 {ma:.1} → {mb:.1} s, reaching {cap:.0} s {ca:.0} → {cb:.0}%");
    if winnable {
        let (wa, wb) = (win(a), win(b));
        let why = format!("won {wa:.0} → {wb:.0}%, {why}");
        if wb > wa {
            return ("yes", why);
        }
        if wb < wa {
            return ("no", why);
        }
        if let (Some(ta), Some(tb)) = (a.time_to_win(), b.time_to_win()) {
            let why = format!("won {wa:.0}% each, wins take p50 {:.1} → {:.1} s", ta.1, tb.1);
            return (if tb.1 < ta.1 - 0.5 { "yes" } else { "no" }, why);
        }
    }
    if mb > ma + 0.5 || cb > ca + 5.0 {
        ("yes", why)
    } else if ma >= cap - 0.5 && mb >= cap - 0.5 {
        ("can't tell", format!("both at the {cap:.0} s cap"))
    } else {
        ("no", why)
    }
}

/// Whether more rounds of `a` and `b` could change their comparison.
fn compared(a: &PresetRun, b: &PresetRun) -> bool {
    if a.rounds.len() < MIN_ROUNDS || b.rounds.len() < MIN_ROUNDS {
        return false;
    }
    let cap = a.cap.min(b.cap);
    let (ia, ib) = (median_interval(&a.lengths(cap)), median_interval(&b.lengths(cap)));
    ia.1 < ib.0 || ib.1 < ia.0 || (ia.0 >= cap - 0.05 && ib.0 >= cap - 0.05)
}

/// Why a preset can stop now, if it can.
fn stop_reason(runs: &[PresetRun], i: usize) -> Option<String> {
    let p = &runs[i];
    let n = p.rounds.len();
    if n < MIN_ROUNDS {
        return None;
    }
    if let Some(v) = p.verdict(true).filter(|v| *v != "not clear yet") {
        return Some(format!("target decided ({v})"));
    }
    if p.target.is_none() && p.wins.is_none() {
        let near: Vec<&PresetRun> = [i.checked_sub(1), Some(i + 1)].into_iter().flatten().filter_map(|j| runs.get(j)).collect();
        if !near.is_empty() && near.iter().all(|q| compared(p, q)) {
            return Some("skill matters decided".into());
        }
    }
    if n >= SETTLE_ROUNDS && p.wins.is_some() {
        if (p.win_rate(n) - p.win_rate(n - 5)).abs() <= 5.0 {
            return Some("win rate settled".into());
        }
        return None;
    }
    if n >= SETTLE_ROUNDS {
        let now = at(&sorted(p.lengths(p.cap)), 50);
        let before = at(&sorted(p.rounds[..n - 5].iter().map(|r| r.seconds.min(p.cap))), 50);
        if (now - before).abs() <= (0.05 * now).max(1.0) {
            return Some("p50 settled".into());
        }
    }
    None
}

/// A preset to play: its name, its cap (s), its target band (s) and, for a
/// level, its win-rate band (%).
pub type Preset = (String, f64, Option<(f32, f32)>, Option<(f32, f32)>);

/// The presets to play: as a level (win rates, every preset capped at
/// `LEVEL_CAP` or its target's top), or on round length.
pub fn presets(names: &[String], targets: &BTreeMap<String, (f32, f32)>, wins: &BTreeMap<String, (f32, f32)>, level: bool) -> Vec<Preset> {
    names
        .iter()
        .map(|n| {
            let target = targets.get(n).copied();
            if level { (n.clone(), cap(n, target).max(LEVEL_CAP), None, wins.get(n).copied()) } else { (n.clone(), cap(n, target), target, None) }
        })
        .collect()
}

/// A runner that hands back the rounds already played where a longer cap
/// can't change them (they ended before the old one), and plays the rest.
struct Again<'a> {
    runner: &'a mut dyn Runner,
    done: BTreeMap<(String, u64), Round>,
}

impl Runner for Again<'_> {
    fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String> {
        match self.done.get(&(preset.to_string(), seed)) {
            Some(r) if !r.capped && r.seconds <= max => Ok(r.clone()),
            _ => self.runner.play(preset, seed, max),
        }
    }
}

/// Plays the presets as `play` does, as a level if `level` says why; else,
/// if most of the best preset's rounds were won, again as a level (only the
/// rounds the cap cut short play again). The runs, and why it's a level.
pub fn play_game(runner: &mut dyn Runner, names: &[String], targets: &BTreeMap<String, (f32, f32)>, wins: &BTreeMap<String, (f32, f32)>, level: Option<String>, max: usize, all: bool) -> Result<(Vec<PresetRun>, Option<String>), String> {
    if level.is_some() {
        return Ok((play(runner, &presets(names, targets, wins, true), max, all)?, level));
    }
    let runs = play(runner, &presets(names, targets, wins, false), max, all)?;
    let Some(best) = runs.last() else { return Ok((runs, None)) };
    let rate = best.win_rate(best.rounds.len());
    if rate <= MOSTLY_WON {
        return Ok((runs, None));
    }
    let why = format!("{} won {rate:.0}% of its rounds", best.name);
    let done = runs.into_iter().flat_map(|p| p.rounds).map(|r| ((r.preset.clone(), r.seed), r)).collect();
    let mut again = Again { runner, done };
    Ok((play(&mut again, &presets(names, targets, wins, true), max, all)?, Some(why)))
}

/// Plays the presets' rounds round-robin, each until it stops (or `max` rounds).
pub fn play(runner: &mut dyn Runner, presets: &[Preset], max: usize, all: bool) -> Result<Vec<PresetRun>, String> {
    let mut runs: Vec<PresetRun> = presets.iter().map(|(n, cap, t, w)| PresetRun { name: n.clone(), cap: *cap, target: *t, wins: *w, rounds: Vec::new(), stopped: None }).collect();
    loop {
        let mut played = false;
        for i in 0..runs.len() {
            if runs[i].stopped.is_some() {
                continue;
            }
            if runs[i].rounds.len() >= max {
                runs[i].stopped = Some(format!("{max} rounds"));
                continue;
            }
            let seed = runs[i].rounds.len() as u64 + 1;
            let r = runner.play(&runs[i].name, seed, runs[i].cap)?;
            if let Some(e) = &r.error {
                return Err(format!("{} seed {seed}: {e}", runs[i].name));
            }
            runs[i].rounds.push(r);
            played = true;
            if !all {
                let n = runs.len();
                for j in [i.checked_sub(1), Some(i), Some(i + 1)].into_iter().flatten().filter(|j| *j < n) {
                    if runs[j].stopped.is_none()
                        && let Some(why) = stop_reason(&runs, j)
                    {
                        runs[j].stopped = Some(why);
                    }
                }
            }
        }
        if !played {
            return Ok(runs);
        }
    }
}

/// The preset's rounds as the SDK's report sees them.
fn outcomes(p: &PresetRun, winnable: bool, kinds: &mut Vec<&'static str>) -> Vec<Outcome> {
    let mut kind = |k: &str| -> &'static str {
        if let Some(s) = kinds.iter().find(|s| **s == k) {
            return s;
        }
        let s: &'static str = Box::leak(k.to_string().into_boxed_str());
        kinds.push(s);
        s
    };
    p.rounds
        .iter()
        .map(|r| {
            let mut o = Outcome::new(r.seconds as f32).stat("score", r.score as f32).stat("presses/min", (60.0 * r.presses as f64 / r.seconds.max(0.1)) as f32).flag("capped", r.capped);
            // A capped round didn't end: its cause isn't one.
            if !r.capped {
                o = o.cause(r.cause.clone());
            }
            if winnable {
                o = o.won(r.won);
            }
            o.events(r.events.iter().map(|(k, t)| (kind(k), *t as f32, *t as f32)).collect::<Vec<_>>())
        })
        .collect()
}

/// The whole run: the report as JSON (with its text and TSV). `level` says
/// why it's judged as a level, if it is.
pub fn report(game: &str, runs: &[PresetRun], level: Option<&str>, old_tsv: Option<&str>) -> Value {
    let winnable = level.is_some() || runs.iter().any(|p| p.rounds.iter().any(|r| r.won));
    let mut kinds = Vec::new();
    let sdk = Report { presets: runs.iter().map(|p| PresetReport::of(&p.name, None, &outcomes(p, winnable, &mut kinds))).collect() };
    let mut text = format!("{game}: {} rounds a preset at most, each from its own seed\n", runs.iter().map(|p| p.rounds.len()).max().unwrap_or(0));
    if let Some(why) = level {
        text += &format!("a level ({why}): judged on how often each preset wins, and how long its wins take\n");
    }
    let mut presets = Vec::new();
    for (p, s) in runs.iter().zip(&sdk.presets) {
        let (p10, p50, p90) = p.spread();
        let capped = 100.0 * p.rounds.iter().filter(|r| r.capped).count() as f64 / p.rounds.len().max(1) as f64;
        let verdict = p.verdict(false);
        let score = s.stat("score").unwrap_or_default();
        let presses = s.stat("presses/min").unwrap_or_default();
        let won = s.win_rate;
        text += &format!("{:<12} {:>2} rounds  length p10 {p10:5.1}  p50 {p50:5.1}  p90 {p90:5.1} s  capped at {:.0} s {capped:3.0}%", p.name, p.rounds.len(), p.cap);
        if let Some(w) = won {
            text += &format!("  won {w:.0}%");
        }
        let to_win = p.time_to_win();
        if let (Some((lo, hi)), Some(v)) = (p.wins, verdict) {
            text += &format!(" (target {lo:.0}-{hi:.0}%: {v})");
        }
        if let (Some(_), Some((_, p50, p90))) = (p.wins, to_win) {
            text += &format!("  wins take p50 {p50:.1} s, p90 {p90:.1} s");
        }
        text += &format!("  score p50 {}  presses {:.0}/min", fmt(score.p50), presses.p50);
        if let (Some((lo, hi)), Some(v)) = (p.target, verdict) {
            text += &format!("  target {lo:.0}-{hi:.0} s: {v}");
        }
        text.push('\n');
        if !s.causes.is_empty() {
            text += &format!("{:<12} ended by: {}\n", "", s.causes.iter().map(|(c, v)| format!("{c} {v:.0}%")).collect::<Vec<_>>().join(", "));
        }
        let constant = s.constant_events();
        for e in &s.events {
            let flag = constant.iter().find(|c| c.0 == e.kind).map_or(String::new(), |c| format!("  CONSTANT: {}", c.1));
            text += &format!("{:<12} {} {:.1}/min{}, {:.0}% of events{flag}\n", "", e.kind, e.per_min, e.cv.map_or(String::new(), |c| format!(", gaps cv {c:.2}")), e.share);
        }
        presets.push(json!({
            "name": p.name, "rounds": p.rounds.len(), "cap": p.cap, "stopped": p.stopped,
            "seconds": { "p10": p10, "p50": p50, "p90": p90 },
            "target": p.target.map(|(lo, hi)| json!([lo, hi])), "wins": p.wins.map(|(lo, hi)| json!([lo, hi])), "target_verdict": verdict,
            "capped": capped, "won": won,
            "time_to_win": to_win.map(|(p10, p50, p90)| json!({ "p10": p10, "p50": p50, "p90": p90 })),
            "score": spread(score), "presses_per_min": presses.p50,
            "causes": s.causes.iter().map(|(c, v)| json!([c, v])).collect::<Vec<_>>(),
            "events": s.events.iter().map(|e| json!({ "kind": e.kind, "per_min": e.per_min, "cv": e.cv, "share": e.share,
                "constant": constant.iter().find(|c| c.0 == e.kind).map(|c| c.1.clone()) })).collect::<Vec<_>>(),
        }));
    }
    // Skill matters: each preset against the one below it.
    let mut pairs = Vec::new();
    for w in runs.windows(2) {
        let (v, why) = compare(&w[0], &w[1], winnable);
        pairs.push(json!({ "better": w[1].name, "than": w[0].name, "verdict": v, "why": why }));
    }
    // With one preset there's nothing to compare: no verdict.
    let overall = if pairs.is_empty() {
        None
    } else if pairs.iter().all(|p| p["verdict"] == "yes") {
        Some("yes")
    } else if pairs.iter().any(|p| p["verdict"] == "no") {
        Some("no")
    } else {
        Some("can't tell past the cap")
    };
    if let Some(overall) = overall {
        text += &format!("skill matters: {overall}\n");
        for p in &pairs {
            text += &format!("  {} beats {}: {} ({})\n", p["better"].as_str().unwrap_or(""), p["than"].as_str().unwrap_or(""), p["verdict"].as_str().unwrap_or(""), p["why"].as_str().unwrap_or(""));
        }
    }
    // Which cause dominates, over every round that ended.
    let mut causes: BTreeMap<&str, usize> = BTreeMap::new();
    let ended: Vec<&Round> = runs.iter().flat_map(|p| &p.rounds).filter(|r| !r.capped).collect();
    for r in &ended {
        *causes.entry(&r.cause).or_default() += 1;
    }
    let dominant = causes.iter().max_by_key(|c| c.1).map(|(c, n)| (c.to_string(), 100.0 * *n as f64 / ended.len().max(1) as f64));
    if let Some((c, share)) = &dominant {
        text += &format!("most rounds end by: {c} ({share:.0}% of those that ended)\n");
    }
    if runs.iter().all(|p| p.rounds.iter().all(|r| r.presses == 0)) {
        text += "the bot never pressed anything: write game/bot.lua (docs: reference \"bot\"), or give the player a behaviour\n";
    }
    let mut tsv = sdk.to_tsv();
    for p in runs {
        tsv += &format!("cap\t{}\t\t{:.0}\n", p.name, p.cap);
        if let Some((lo, hi)) = p.target {
            tsv += &format!("target\t{}\t\t{lo:.0}\t{hi:.0}\n", p.name);
        }
        if let Some((lo, hi)) = p.wins {
            tsv += &format!("wins\t{}\t\t{lo:.0}\t{hi:.0}\n", p.name);
        }
    }
    let changes = old_tsv.map(|t| sdk.changes_since(t));
    if let Some(c) = &changes {
        if c.is_empty() {
            text += "no change since the last report\n";
        } else {
            text += "changed since the last report:\n";
            for l in c {
                text += &format!("  {l}\n");
            }
        }
    }
    json!({
        "presets": presets,
        "level": level.map(|why| json!({ "why": why })),
        "skill_matters": { "verdict": overall, "pairs": pairs },
        "dominant_cause": dominant.map(|(c, s)| json!({ "cause": c, "share": s })),
        "changes": changes,
        "text": text,
        "tsv": tsv,
    })
}

fn spread(s: Spread) -> Value {
    json!({ "p10": s.p10, "p50": s.p50, "p90": s.p90 })
}

fn fmt(v: f32) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// `--target first-timer=20-45` (and the defaults) as each preset's band.
pub fn targets(given: &[String]) -> Result<BTreeMap<String, (f32, f32)>, String> {
    bands(&TARGETS, given, "--target", "in seconds (first-timer=20-45)")
}

/// `--wins first-timer=15-40` (and the defaults) as each preset's win-rate band.
pub fn win_targets(given: &[String]) -> Result<BTreeMap<String, (f32, f32)>, String> {
    let out = bands(&WIN_TARGETS, given, "--wins", "in % of rounds (first-timer=15-40)")?;
    match out.iter().find(|(_, b)| b.1 > 100.0) {
        Some((n, _)) => Err(format!("--wins {n}: at most 100%")),
        None => Ok(out),
    }
}

fn bands(defaults: &[(&str, (f32, f32))], given: &[String], flag: &str, unit: &str) -> Result<BTreeMap<String, (f32, f32)>, String> {
    let mut out: BTreeMap<String, (f32, f32)> = defaults.iter().map(|(n, t)| (n.to_string(), *t)).collect();
    for t in given {
        let bad = || format!("{flag} {t}: name=lo-hi {unit}");
        let (name, band) = t.split_once('=').ok_or_else(bad)?;
        let (lo, hi) = band.split_once('-').ok_or_else(bad)?;
        let (lo, hi): (f32, f32) = (lo.trim().parse().map_err(|_| bad())?, hi.trim().parse().map_err(|_| bad())?);
        if !(lo >= 0.0 && hi > lo) {
            return Err(bad());
        }
        out.insert(name.trim().to_string(), (lo, hi));
    }
    Ok(out)
}

/// A preset's cap: its target's top (at most `MAX_CAP`), else the table's.
pub fn cap(name: &str, target: Option<(f32, f32)>) -> f64 {
    match target {
        Some((_, hi)) => (hi as f64).clamp(10.0, MAX_CAP),
        None => CAPS.iter().find(|c| c.0 == name).map_or(90.0, |c| c.1),
    }
}

pub fn run(a: DifficultyArgs) -> Result<(), String> {
    let t0 = Instant::now();
    let work = std::env::temp_dir().join(format!("mb-difficulty-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = difficulty(&a, &work, t0);
    let _ = std::fs::remove_dir_all(&work);
    let out = result?;
    if a.json {
        println!("{out}");
    } else {
        print!("{}", out["text"].as_str().unwrap_or(""));
        println!("{}", out["timings"]);
    }
    Ok(())
}

fn difficulty(a: &DifficultyArgs, work: &Path, t0: Instant) -> Result<Value, String> {
    if !matches!(a.rounds, 1..=40) {
        return Err("--rounds: 1 to 40 (20 by default, 40 for a final check)".into());
    }
    let bundle = if a.game.is_dir() {
        if !super::is_project(&a.game) {
            return Err(format!("{}: not a Studio project (no studio.toml) or bundle", a.game.display()));
        }
        super::build_and_pack(&a.game, &work.join("built"))?
    } else {
        a.game.clone()
    };
    let b = crate::engines::open(&bundle)?;
    let engine = b.manifest.engine.clone().unwrap_or_default();
    if !engine.starts_with("studio@2") {
        return Err(format!("{}: difficulty plays Studio 2.0 games (this one is {})", bundle.display(), if engine.is_empty() { "not a Studio game".into() } else { engine }));
    }
    let names: Vec<String> = if a.presets.is_empty() { PRESETS.iter().map(|s| s.to_string()).collect() } else { a.presets.clone() };
    let mut names = names;
    for n in &names {
        if !PRESETS.contains(&n.as_str()) {
            return Err(format!("--presets: no preset {n:?} ({})", PRESETS.join(", ")));
        }
    }
    names.sort_by_key(|n| PRESETS.iter().position(|p| p == n));
    names.dedup();
    let targets = targets(&a.target)?;
    let wins = win_targets(&a.wins)?;
    let level = if a.level { Some("--level".to_string()) } else if says_level(&b) { Some("studio.toml says level = true".to_string()) } else { None };
    let three = b.file("game.wasm").and_then(super::v2::published_variant).is_some_and(|v| v.is_3d()) || b.manifest.stdlib.get("mb3d").is_some_and(|v| *v > 0);
    let published = b.file("game.wasm").is_some_and(|w| super::v2::published_variant(w).is_some());
    let runtime = a.runtime.clone().or_else(|| std::env::var_os("MB_RUNTIME_DIR").map(PathBuf::from));

    // Natively where it can, checked against Deno; else Deno.
    let why_not_native = if three {
        Some("a 3D game reads back from mb3d")
    } else if !published {
        Some("the bundle's engine isn't the published one this mb carries")
    } else {
        None
    };
    let native = match (a.runner.as_str(), why_not_native) {
        ("native", _) => true,
        ("deno", _) => false,
        ("auto", None) => true,
        ("auto", Some(_)) => false,
        (r, _) => return Err(format!("--runner {r}: auto, deno or native")),
    };
    let mut check = Value::Null;
    let mut setup_s = 0.0;
    let mut runs = None;
    let mut runner_name = "deno";
    if native {
        let mut n = Native::new(&b)?;
        // `found`: whether these native rounds make it a level (or `level` said so).
        let (played, found) = play_game(&mut n, &names, &targets, &wins, level.clone(), a.rounds, a.all)?;
        let reads = played.iter().flat_map(|p| &p.rounds).any(|r| r.reads);
        if reads {
            check = json!({ "native": false, "why": "the game's logic reads text metrics back from the host, which the native runner stubs" });
        } else if a.runner == "native" {
            runs = Some((played, found));
            runner_name = "native";
        } else {
            let mut deno = Deno::start(&b, &work.join("deno"), runtime.as_deref())?;
            setup_s = deno.setup_s;
            let (c, r, matched) = checked((played, found), &mut deno, &names, &targets, &wins, level.clone(), a.rounds, a.all)?;
            (check, runs) = (c, Some(r));
            if matched {
                runner_name = "native";
            }
        }
    } else if let Some(w) = why_not_native {
        check = json!({ "native": false, "why": w });
    }
    let (runs, level) = match runs {
        Some(r) => r,
        None => {
            let mut deno = Deno::start(&b, &work.join("deno"), runtime.as_deref())?;
            setup_s = deno.setup_s;
            play_game(&mut deno, &names, &targets, &wins, level, a.rounds, a.all)?
        }
    };
    let old = a.report.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    let mut out = report(&b.manifest.name, &runs, level.as_deref(), old.as_deref());
    if let Some(p) = &a.report {
        if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(p, out["tsv"].as_str().unwrap_or("")).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let game_s: f64 = runs.iter().flat_map(|p| &p.rounds).map(|r| r.seconds).sum();
    let n: usize = runs.iter().map(|p| p.rounds.len()).sum();
    out["rounds"] = runs
        .iter()
        .flat_map(|p| &p.rounds)
        .map(|r| json!({ "preset": r.preset, "seed": r.seed, "seconds": r.seconds, "score": r.score, "cause": r.cause, "won": r.won, "capped": r.capped, "presses": r.presses, "events": r.events.len() }))
        .collect();
    out["runner"] = runner_name.into();
    out["native_check"] = check;
    out["timings"] = json!({ "wall_s": round3(t0.elapsed().as_secs_f64()), "setup_s": round3(setup_s), "rounds": n, "game_s": round3(game_s) });
    Ok(out)
}

/// The runs and why it's a level, if it is.
type Played = (Vec<PresetRun>, Option<String>);

/// The native run checked against Deno: one round a preset played again
/// there (the one nearest its p50, at most 60 s of play). The check's JSON,
/// the runs, and whether the native ones stand. On any difference Deno
/// plays the whole run, deciding for itself whether it's a level from its
/// own rounds: `declared` (--level, studio.toml) carries over, a level the
/// native rounds found doesn't.
#[allow(clippy::too_many_arguments)]
fn checked(native: Played, deno: &mut dyn Runner, names: &[String], targets: &BTreeMap<String, (f32, f32)>, wins: &BTreeMap<String, (f32, f32)>, declared: Option<String>, max: usize, all: bool) -> Result<(Value, Played, bool), String> {
    let mut checked = 0;
    for p in &native.0 {
        let p50 = p.spread().1.min(60.0);
        let Some(r) = p.rounds.iter().min_by(|x, y| (x.seconds - p50).abs().total_cmp(&(y.seconds - p50).abs())) else { continue };
        let d = deno.play(&r.preset, r.seed, p.cap)?;
        checked += 1;
        if let Err(e) = r.same_play(&d) {
            let check = json!({ "native": false, "why": format!("native didn't match Deno: {e}") });
            return Ok((check, play_game(deno, names, targets, wins, declared, max, all)?, false));
        }
    }
    Ok((json!({ "native": true, "checked": checked }), native, true))
}

/// The bundle's studio.toml says `level = true`.
fn says_level(b: &mb_format::Bundle) -> bool {
    let toml = b.file("assets/studio.toml").map(String::from_utf8_lossy).unwrap_or_default();
    toml.parse::<toml::Table>().is_ok_and(|t| t.get("level").and_then(toml::Value::as_bool) == Some(true))
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, cap: f64, target: Option<(f32, f32)>, lengths: &[f64], won: bool) -> PresetRun {
        let rounds = lengths
            .iter()
            .enumerate()
            .map(|(i, s)| Round { preset: name.into(), seed: i as u64 + 1, seconds: s.min(cap), score: 0.0, cause: if *s >= cap { "time".into() } else { "fell".into() }, won, capped: *s >= cap, presses: 3, reads: false, events: Vec::new(), error: None, wall_ms: 0.0 })
            .collect();
        PresetRun { name: name.into(), cap, target, wins: None, rounds, stopped: None }
    }

    #[test]
    fn the_median_interval_widens_with_fewer_rounds() {
        let v: Vec<f64> = (1..=20).map(f64::from).collect();
        let (a, b) = median_interval(&v);
        assert!(a < 10.5 && b > 10.5 && b - a < 10.0, "{a} {b}");
        let (a8, b8) = median_interval(&v[..8]);
        assert_eq!((a8, b8), (2.0, 7.0));
        assert_eq!(median_interval(&[5.0]), (5.0, 5.0));
    }

    #[test]
    fn targets_and_caps() {
        let t = targets(&["good=30-90".into()]).unwrap();
        assert_eq!(t["first-timer"], (20.0, 45.0));
        assert_eq!(t["good"], (30.0, 90.0));
        assert_eq!(cap("good", t.get("good").copied()), 90.0);
        assert_eq!(cap("casual", None), 90.0);
        assert!(targets(&["good=fast".into()]).is_err());
    }

    #[test]
    fn verdicts_say_on_short_long_and_past_the_cap() {
        let on = run("first-timer", 45.0, Some((20.0, 45.0)), &[25.0, 30.0, 28.0, 33.0, 31.0, 27.0, 35.0, 29.0, 30.0, 32.0], false);
        assert_eq!(on.verdict(true), Some("on"));
        let short = run("first-timer", 45.0, Some((20.0, 45.0)), &[3.0, 4.0, 5.0, 2.0, 6.0, 4.0, 3.0, 5.0], false);
        assert_eq!(short.verdict(true), Some("too short"));
        let capped = run("first-timer", 45.0, Some((20.0, 45.0)), &[45.0; 8], false);
        assert!(capped.verdict(true).unwrap().starts_with("too long"));
        let past = run("good", 45.0, Some((60.0, 180.0)), &[45.0; 8], false);
        assert_eq!(past.verdict(true), Some("past the cap (can't tell)"));
    }

    #[test]
    fn skill_matters_compares_at_the_shorter_cap_and_on_wins() {
        let a = run("first-timer", 45.0, None, &[10.0, 12.0, 9.0, 14.0], false);
        let b = run("casual", 90.0, None, &[30.0, 50.0, 70.0, 90.0], false);
        assert_eq!(compare(&a, &b, false).0, "yes");
        assert_eq!(compare(&b, &a, false).0, "no");
        let c = run("good", 45.0, None, &[45.0; 4], false);
        let d = run("expert", 45.0, None, &[45.0; 4], false);
        assert_eq!(compare(&c, &d, false).0, "can't tell");
        let w1 = run("good", 60.0, None, &[20.0, 30.0], false);
        let mut w2 = run("expert", 60.0, None, &[20.0, 30.0], true);
        w2.rounds[1].won = false;
        assert_eq!(compare(&w1, &w2, true).0, "yes");
        // Winning as often, sooner: better at a level.
        let slow = run("good", 180.0, None, &[60.0, 70.0, 20.0], true);
        let fast = run("expert", 180.0, None, &[40.0, 45.0, 20.0], true);
        assert_eq!(compare(&slow, &fast, true).0, "yes");
        assert_eq!(compare(&fast, &slow, true).0, "no");
    }

    /// A stand-in runner: rounds whose length is the seed times the preset's skill.
    struct Fake(Vec<(String, u64)>);
    impl Runner for Fake {
        fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String> {
            self.0.push((preset.into(), seed));
            let k = PRESETS.iter().position(|p| *p == preset).unwrap() as f64 + 1.0;
            let s = (k * 10.0 + (seed % 5) as f64).min(max);
            Ok(Round { preset: preset.into(), seed, seconds: s, score: s, cause: "fell".into(), won: false, capped: s >= max, presses: 4, reads: false, events: vec![("pickup coin".into(), 1.0)], error: None, wall_ms: 1.0 })
        }
    }

    #[test]
    fn rounds_go_round_robin_and_stop_early_once_decided() {
        let names: Vec<String> = PRESETS.iter().map(|n| n.to_string()).collect();
        let presets = presets(&names, &targets(&[]).unwrap(), &win_targets(&[]).unwrap(), false);
        let mut f = Fake(Vec::new());
        let runs = play(&mut f, &presets, 20, false).unwrap();
        assert_eq!(f.0[..4].iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), PRESETS, "round-robin");
        assert!(runs.iter().all(|p| p.rounds.len() >= MIN_ROUNDS && p.rounds.len() < 20), "{:?}", runs.iter().map(|p| (p.rounds.len(), p.stopped.clone())).collect::<Vec<_>>());
        assert!(runs[0].stopped.as_deref().unwrap().contains("too short"));
        let mut f = Fake(Vec::new());
        assert!(play(&mut f, &presets, 20, true).unwrap().iter().all(|p| p.rounds.len() == 20));
        let r = report("Test", &runs, None, None);
        let text = r["text"].as_str().unwrap();
        assert!(text.contains("skill matters: yes") && text.contains("target 20-45 s: too short"), "{text}");
        assert!(r["tsv"].as_str().unwrap().contains("cap\tgood\t\t180"));
        let again = report("Test", &runs, None, Some(r["tsv"].as_str().unwrap()));
        assert_eq!(again["changes"], json!([]));
        // One preset: nothing to compare, so no verdict on skill.
        let one = report("Test", &runs[..1], None, None);
        assert_eq!(one["skill_matters"], json!({ "verdict": null, "pairs": [] }));
        assert!(!one["text"].as_str().unwrap().contains("skill matters"));
    }

    /// A level: rounds won with a chance by skill (first-timer 1 in 4 …
    /// expert 9 in 10), each won round taking 30 + seed s; a lost one runs
    /// on past an endless game's caps when `slow`.
    struct Level {
        slow: bool,
        played: Vec<(String, u64, f64)>,
    }
    impl Runner for Level {
        fn play(&mut self, preset: &str, seed: u64, max: f64) -> Result<Round, String> {
            self.played.push((preset.into(), seed, max));
            let wins_in_10 = [3, 5, 7, 9][PRESETS.iter().position(|p| *p == preset).unwrap()];
            let won = seed % 10 < wins_in_10;
            let s = if won { 30.0 + seed as f64 } else if self.slow { 100.0 } else { 20.0 };
            let (s, capped) = if s >= max { (max, true) } else { (s, false) };
            Ok(Round { preset: preset.into(), seed, seconds: s, score: 0.0, cause: if won { "lit".into() } else if capped { "time".into() } else { "fell".into() }, won: won && !capped, capped, presses: 4, reads: false, events: Vec::new(), error: None, wall_ms: 1.0 })
        }
    }

    #[test]
    fn a_level_is_judged_on_wins_within_its_cap() {
        let names: Vec<String> = PRESETS.iter().map(|n| n.to_string()).collect();
        let (t, w) = (targets(&[]).unwrap(), win_targets(&[]).unwrap());
        // Declared: every preset capped at 180 s, judged on its win rate.
        let mut l = Level { slow: false, played: Vec::new() };
        let (runs, why) = play_game(&mut l, &names, &t, &w, Some("studio.toml says level = true".into()), 20, true).unwrap();
        assert_eq!(why.as_deref(), Some("studio.toml says level = true"));
        assert!(runs.iter().all(|p| p.cap == LEVEL_CAP && p.target.is_none() && p.wins.is_some()));
        let v: Vec<_> = runs.iter().map(|p| p.verdict(false).unwrap()).collect();
        assert_eq!(v, ["on"; 4], "won 30, 50, 70 and 90%");
        let r = report("Test", &runs, why.as_deref(), None);
        let text = r["text"].as_str().unwrap();
        assert!(text.contains("a level (studio.toml says level = true)") && text.contains("won 30% (target 15-40%: on)") && text.contains("wins take p50"), "{text}");
        assert_eq!(r["presets"][0]["wins"], json!([15.0, 40.0]));
        assert!(r["presets"][3]["time_to_win"]["p50"].as_f64().unwrap() > 30.0);
        assert_eq!(r["skill_matters"]["verdict"], "yes");
        assert!(r["tsv"].as_str().unwrap().contains("wins\tgood\t\t60\t85"));
        // Found: most of the expert's rounds won, so it plays again as a
        // level, replaying only the rounds the endless caps cut short.
        let mut l = Level { slow: true, played: Vec::new() };
        let (runs, why) = play_game(&mut l, &names, &t, &w, None, 10, true).unwrap();
        assert_eq!(why.as_deref(), Some("expert won 90% of its rounds"));
        assert!(runs.iter().all(|p| p.cap == LEVEL_CAP));
        let again: Vec<_> = l.played[40..].iter().map(|p| (p.0.as_str(), p.1)).collect();
        assert!(!again.contains(&("first-timer", 1)) && !again.contains(&("good", 2)), "a round that ended under the old cap is the same round: {again:?}");
        assert!(again.contains(&("first-timer", 3)) && again.contains(&("casual", 5)), "the runs the 45 and 90 s caps cut short play on: {again:?}");
        // Too few wins: too hard, or too long when the runs keep running out at the cap.
        let mut lost = run("good", 180.0, None, &[20.0; 8], false);
        lost.wins = Some((60.0, 85.0));
        assert_eq!(lost.verdict(true), Some("too hard"));
        let mut ran_out = run("good", 180.0, None, &[180.0; 8], false);
        ran_out.wins = Some((60.0, 85.0));
        assert_eq!(ran_out.verdict(true), Some("too long (its runs reach the cap before they win)"));
        let mut easy = run("first-timer", 180.0, None, &[50.0; 12], true);
        easy.wins = Some((15.0, 40.0));
        assert_eq!(easy.verdict(true), Some("too easy"));
        // An endless game stays one.
        let mut f = Fake(Vec::new());
        assert_eq!(play_game(&mut f, &names, &t, &w, None, 10, true).unwrap().1, None);
    }

    #[test]
    fn deno_decides_the_level_itself_when_native_didnt_match() {
        let names: Vec<String> = PRESETS.iter().map(|n| n.to_string()).collect();
        let (t, w) = (targets(&[]).unwrap(), win_targets(&[]).unwrap());
        // Natively a level (the expert won 90%), but Deno plays it differently: endless.
        let native = play_game(&mut Level { slow: true, played: Vec::new() }, &names, &t, &w, None, 10, true).unwrap();
        assert!(native.1.is_some());
        let (check, (runs, level), matched) = checked(native, &mut Fake(Vec::new()), &names, &t, &w, None, 10, true).unwrap();
        assert!(!matched && check["native"] == false, "{check}");
        assert_eq!(level, None, "the rejected native rounds' level doesn't carry over");
        assert!(runs.iter().all(|p| p.wins.is_none()), "played on round length");
        // Declared (studio.toml, --level): still a level in Deno.
        let native = play_game(&mut Level { slow: true, played: Vec::new() }, &names, &t, &w, None, 10, true).unwrap();
        let (_, (_, level), _) = checked(native, &mut Fake(Vec::new()), &names, &t, &w, Some("--level".into()), 10, true).unwrap();
        assert_eq!(level.as_deref(), Some("--level"));
        // Matched: the native run stands, its level with it.
        let native = play_game(&mut Level { slow: true, played: Vec::new() }, &names, &t, &w, None, 10, true).unwrap();
        let (check, (_, level), matched) = checked(native, &mut Level { slow: true, played: Vec::new() }, &names, &t, &w, None, 10, true).unwrap();
        assert!(matched && check["native"] == true, "{check}");
        assert_eq!(level.as_deref(), Some("expert won 90% of its rounds"));
    }

    #[test]
    fn win_intervals_and_bands() {
        let (a, b) = win_interval(5, 10);
        assert!(a > 20.0 && a < 50.0 && b > 50.0 && b < 80.0, "{a} {b}");
        assert_eq!(win_interval(0, 0), (0.0, 100.0));
        assert!(win_interval(20, 20).1 <= 100.0 && win_interval(0, 20).0 >= 0.0);
        assert_eq!(win_targets(&["expert=90-100".into()]).unwrap()["expert"], (90.0, 100.0));
        assert!(win_targets(&["expert=90-120".into()]).is_err());
    }
}
