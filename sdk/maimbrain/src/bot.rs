//! The difficulty report every game's bot feeds (SKILL.md "Write a bot"):
//! many seeded rounds per kind of player, how long they last (p10, p50,
//! p90), how they end, and the game's own numbers, in one table. The game
//! supplies the player (its bot, built on `input::synth::Skill`) and a
//! function that plays one round; [`Difficulty`] runs the rounds, in
//! parallel, and reports.
//!
//! ```ignore
//! #[test]
//! fn difficulty() {
//!     let report = Difficulty::new(200)
//!         .preset("first-timer", FIRST_TIMER).target(20.0, 45.0)
//!         .preset("casual", CASUAL)
//!         .preset("good", GOOD).target(60.0, 180.0)
//!         .run(|skill, seed| {
//!             let r = play(skill, seed, None);     // your round: a bot through `control`, as lib.rs
//!             Outcome::new(r.seconds).cause(r.cause).stat("taps", r.taps as f32)
//!         });
//!     report.print();
//!     report.assert_skill_matters();
//! }
//! ```
//!
//! Native only (it's for `cargo test`).

use std::collections::BTreeMap;
use std::fmt;

/// How one round went, from the game's play function.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// How long the round lasted (s of game time).
    pub seconds: f32,
    /// How it ended ("hit a crate", "looked away"): the report counts them.
    pub cause: String,
    /// For a game with a fixed end (a boss, a level): whether the player won.
    pub won: Option<bool>,
    /// The game's own numbers for the round (score, taps, accuracy %…);
    /// the report shows each one's spread.
    pub stats: Vec<(&'static str, f32)>,
    /// Yes-or-no facts about the round ("under 10 s", "beat the boss"); the
    /// report shows the share of rounds where each was true.
    pub flags: Vec<(&'static str, bool)>,
    /// Counts to total over the rounds as a share: (name, part, whole), e.g.
    /// ("timeouts of misses", timeouts, misses). The report shows
    /// Σ part ÷ Σ whole, which a median of per-round shares can't.
    pub ratios: Vec<(&'static str, f32, f32)>,
}

impl Outcome {
    pub fn new(seconds: f32) -> Outcome {
        Outcome { seconds, cause: String::new(), won: None, stats: Vec::new(), flags: Vec::new(), ratios: Vec::new() }
    }
    pub fn cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = cause.into();
        self
    }
    pub fn won(mut self, won: bool) -> Self {
        self.won = Some(won);
        self
    }
    pub fn stat(mut self, name: &'static str, value: f32) -> Self {
        self.stats.push((name, value));
        self
    }
    pub fn flag(mut self, name: &'static str, on: bool) -> Self {
        self.flags.push((name, on));
        self
    }
    pub fn ratio(mut self, name: &'static str, part: f32, whole: f32) -> Self {
        self.ratios.push((name, part, whole));
        self
    }
}

/// Seconds at the 10th, 50th and 90th percentiles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spread {
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
}

impl Spread {
    /// Of these values (in any order; NaNs sort last, never panic). The
    /// p-th percentile is the value at index p·n/100 of the sorted values
    /// (the p50 of 200 rounds is the 101st, `v[n / 2]`).
    pub fn of(values: &[f32]) -> Spread {
        let mut v = values.to_vec();
        if v.is_empty() {
            return Spread::default();
        }
        v.sort_by(f32::total_cmp);
        let at = |p: usize| v[(p * v.len() / 100).min(v.len() - 1)];
        Spread { p10: at(10), p50: at(50), p90: at(90) }
    }
}

/// One kind of player's rounds.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetReport {
    pub name: String,
    pub rounds: usize,
    pub seconds: Spread,
    /// The p50 band the skill aims for, if one was given.
    pub target: Option<(f32, f32)>,
    /// How rounds ended, most common first: (cause, % of rounds).
    pub causes: Vec<(String, f32)>,
    /// % of rounds won, for games that report `won`.
    pub win_rate: Option<f32>,
    /// Each of the game's numbers: its spread over the rounds.
    pub stats: Vec<(&'static str, Spread)>,
    /// Each flag: % of rounds where it was true.
    pub flags: Vec<(&'static str, f32)>,
    /// Each ratio: Σ part ÷ Σ whole, as a %.
    pub ratios: Vec<(&'static str, f32)>,
}

impl PresetReport {
    /// The p50 is inside the target band (true with no target).
    pub fn on_target(&self) -> bool {
        self.target.is_none_or(|(lo, hi)| (lo..=hi).contains(&self.seconds.p50))
    }

    /// One of the game's numbers' spread.
    pub fn stat(&self, name: &str) -> Option<Spread> {
        self.stats.iter().find(|s| s.0 == name).map(|s| s.1)
    }
}

/// The whole report: one entry per preset, in the order they were added.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub presets: Vec<PresetReport>,
}

impl Report {
    /// The preset named `name`.
    pub fn get(&self, name: &str) -> &PresetReport {
        self.presets.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no preset {name:?}"))
    }

    /// Prints the table (run with `--nocapture` to see it).
    pub fn print(&self) {
        println!("{self}");
    }

    /// Fails unless each preset lasts longer (by p50) than the one before,
    /// or wins more often: if better players don't do better, skill doesn't
    /// matter, and the ramp is probably too steep at the end.
    pub fn assert_skill_matters(&self) {
        for w in self.presets.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let better = match (a.win_rate, b.win_rate) {
                (Some(x), Some(y)) if x != y => y > x,
                _ => b.seconds.p50 > a.seconds.p50,
            };
            assert!(better, "{} should do better than {}:\n{self}", b.name, a.name);
        }
    }

    /// Compares this report with the one the last run saved at `path`,
    /// prints what changed, and saves this one there for the next run.
    /// Rounds are seeded, so any change is the game's: after an edit to a
    /// course, the physics or a timing, it shows what that did to every
    /// preset and every way rounds end, not only to the spot you changed (a
    /// fix in one place can quadruple the falls somewhere else). Keep the
    /// file in the game's crate (`cargo test` runs there) and commit it, so
    /// the last tuning the owner approved is the baseline. Returns the lines
    /// it printed (empty when nothing changed).
    pub fn compare_with(&self, path: impl AsRef<std::path::Path>) -> Vec<String> {
        let path = path.as_ref();
        let old = std::fs::read_to_string(path).ok().map(|t| parse(&t));
        let new = parse(&self.to_tsv());
        let changes = match &old {
            Some(old) => changes(old, &new),
            None => Vec::new(),
        };
        match (&old, changes.is_empty()) {
            (None, _) => println!("no earlier report at {}: this one is the baseline", path.display()),
            (Some(_), true) => println!("no change since the report at {}", path.display()),
            (Some(_), false) => {
                println!("changed since the report at {}:", path.display());
                for c in &changes {
                    println!("  {c}");
                }
            }
        }
        if let Err(e) = std::fs::write(path, self.to_tsv()) {
            println!("couldn't save the report at {}: {e}", path.display());
        }
        changes
    }

    /// The report as the tab-separated lines `compare_with` saves.
    pub fn to_tsv(&self) -> String {
        let mut out = String::from("# The bot's difficulty report (maimbrain::bot::Difficulty::compare_with): the next\n# run prints what changed against it. Seeded, so it changes only when the game does.\n");
        for p in &self.presets {
            let s = p.seconds;
            out += &format!("seconds\t{}\t\t{:.1}\t{:.1}\t{:.1}\n", p.name, s.p10, s.p50, s.p90);
            if let Some(w) = p.win_rate {
                out += &format!("won\t{}\t\t{w:.1}\n", p.name);
            }
            for (k, s) in &p.stats {
                out += &format!("stat\t{}\t{k}\t{}\t{}\t{}\n", p.name, num(s.p10), num(s.p50), num(s.p90));
            }
            for (k, v) in p.flags.iter().chain(&p.ratios) {
                out += &format!("share\t{}\t{k}\t{v:.1}\n", p.name);
            }
            for (c, v) in &p.causes {
                out += &format!("ended\t{}\t{c}\t{v:.1}\n", p.name);
            }
        }
        out
    }

    /// Fails unless every preset with a target has its p50 inside it.
    pub fn assert_on_target(&self) {
        let off: Vec<&str> = self.presets.iter().filter(|p| !p.on_target()).map(|p| p.name.as_str()).collect();
        assert!(off.is_empty(), "off target: {}\n{self}", off.join(", "));
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.presets.first().map_or(0, |p| p.rounds);
        writeln!(f, "{n} rounds per preset (round length p10 / p50 / p90):")?;
        let w = self.presets.iter().map(|p| p.name.len()).max().unwrap_or(0).max(6);
        for p in &self.presets {
            let s = p.seconds;
            write!(f, "{:<w$}  {:6.1}s {:6.1}s {:6.1}s", p.name, s.p10, s.p50, s.p90)?;
            if let Some((lo, hi)) = p.target {
                write!(f, "   target p50 {lo:.0}-{hi:.0} s: {}", if p.on_target() { "ok" } else { "OFF" })?;
            }
            if let Some(r) = p.win_rate {
                write!(f, "   won {r:.0}%")?;
            }
            writeln!(f)?;
            if !p.stats.is_empty() {
                let stats: Vec<String> = p.stats.iter().map(|(k, s)| format!("{k} {} ({}-{})", num(s.p50), num(s.p10), num(s.p90))).collect();
                writeln!(f, "{:<w$}  {}", "", stats.join(", "))?;
            }
            let shares: Vec<String> = p.flags.iter().chain(&p.ratios).map(|(k, pc)| format!("{k} {}", pct(*pc))).collect();
            if !shares.is_empty() {
                writeln!(f, "{:<w$}  {}", "", shares.join(", "))?;
            }
            if !p.causes.is_empty() {
                let causes: Vec<String> = p.causes.iter().map(|(c, pc)| format!("{c} {}", pct(*pc))).collect();
                writeln!(f, "{:<w$}  ended: {}", "", causes.join(", "))?;
            }
        }
        Ok(())
    }
}

/// A share: whole percent from 10 up, one decimal under (1 round in 200 is 0.5%, not 0%).
fn pct(v: f32) -> String {
    if v >= 10.0 || v == 0.0 { format!("{v:.0}%") } else { format!("{v:.1}%") }
}

/// A saved report: (kind, preset, name) → its numbers.
type Saved = Vec<((String, String, String), Vec<f32>)>;

fn parse(text: &str) -> Saved {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let nums = f.get(3..)?.iter().filter_map(|v| v.parse().ok()).collect();
            Some(((f[0].to_string(), f.get(1)?.to_string(), f.get(2)?.to_string()), nums))
        })
        .collect()
}

/// What differs between two saved reports, a line each, in the new one's order.
fn changes(old: &Saved, new: &Saved) -> Vec<String> {
    let find = |s: &Saved, k: &(String, String, String)| s.iter().find(|e| e.0 == *k).map(|e| e.1.clone());
    let mut out = Vec::new();
    for (key, now) in new {
        let (kind, preset, name) = key;
        let was = find(old, key);
        let line = match (kind.as_str(), &was) {
            ("seconds", Some(w)) if w != now && now.len() == 3 && w.len() == 3 => {
                let pc = if w[1] > 0.0 { format!(" ({:+.0}%)", 100.0 * (now[1] - w[1]) / w[1]) } else { String::new() };
                Some(format!("{preset}: round p10/p50/p90 {:.1}/{:.1}/{:.1} s -> {:.1}/{:.1}/{:.1} s{pc}", w[0], w[1], w[2], now[0], now[1], now[2]))
            }
            ("won", Some(w)) if w != now => Some(format!("{preset}: won {} -> {}", pct(w[0]), pct(now[0]))),
            ("stat", Some(w)) if w.get(1) != now.get(1) => Some(format!("{preset}: {name} p50 {} -> {}", num(w[1]), num(now[1]))),
            ("share" | "ended", Some(w)) if (w[0] - now[0]).abs() >= 0.5 => {
                let what = if kind == "ended" { format!("ended by {name}") } else { name.clone() };
                Some(format!("{preset}: {what} {} -> {}", pct(w[0]), pct(now[0])))
            }
            ("ended", None) => Some(format!("{preset}: new way to end: {name} {}", pct(now[0]))),
            ("seconds" | "won" | "stat" | "share", None) => Some(format!("{preset}: new {kind} {name}")),
            _ => None,
        };
        out.extend(line);
    }
    for (key, w) in old {
        if find(new, key).is_none() && key.0 == "ended" {
            out.push(format!("{}: no longer ends by {} (was {})", key.1, key.2, pct(w[0])));
        }
    }
    out
}

fn num(v: f32) -> String {
    if v.abs() >= 100.0 || v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// Plays `rounds` seeded rounds for each preset and reports. See the
/// [module docs](self).
pub struct Difficulty<P> {
    rounds: usize,
    first_seed: u64,
    presets: Vec<Preset<P>>,
}

/// A preset: its name, the player, and its p50 target band.
type Preset<P> = (String, P, Option<(f32, f32)>);

impl<P: Copy + Send + Sync> Difficulty<P> {
    /// `rounds` rounds per preset (200 is enough to trust a p50 to a second
    /// or two), seeded 0, 1, 2…
    pub fn new(rounds: usize) -> Self {
        Difficulty { rounds: rounds.max(1), first_seed: 0, presets: Vec::new() }
    }

    /// Seeds start here instead of 0.
    pub fn seeds_from(mut self, seed: u64) -> Self {
        self.first_seed = seed;
        self
    }

    /// A kind of player, by name: whatever your play function takes (your
    /// bot's skill: a `synth::Skill`, or yours built on one).
    pub fn preset(mut self, name: &str, preset: P) -> Self {
        self.presets.push((name.to_string(), preset, None));
        self
    }

    /// The band the last preset's p50 round length should fall in (s): the
    /// skill's targets are a first-timer 20–45 s and a good player 60–180 s.
    pub fn target(mut self, lo: f32, hi: f32) -> Self {
        if let Some(p) = self.presets.last_mut() {
            p.2 = Some((lo, hi));
        }
        self
    }

    /// Plays every round (`play(preset, seed)`), spread over the machine's
    /// cores, and reports. Each round starts on a fresh native clock
    /// (`sys::test::reset`), so the same seed always plays the same round.
    pub fn run(&self, play: impl Fn(P, u64) -> Outcome + Sync) -> Report {
        let jobs: Vec<(usize, u64)> = (0..self.presets.len()).flat_map(|p| (0..self.rounds as u64).map(move |s| (p, s))).collect();
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len()).max(1);
        let chunk = jobs.len().div_ceil(threads);
        let mut outcomes: Vec<Vec<Outcome>> = (0..self.presets.len()).map(|_| Vec::with_capacity(self.rounds)).collect();
        let done: Vec<Vec<(usize, Outcome)>> = std::thread::scope(|scope| {
            let handles: Vec<_> = jobs
                .chunks(chunk)
                .map(|part| {
                    let play = &play;
                    scope.spawn(move || {
                        part.iter()
                            .map(|&(p, s)| {
                                crate::sys::test::reset();
                                (p, play(self.presets[p].1, self.first_seed + s))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
        });
        // Chunks are in job order, so each preset's outcomes stay in seed order.
        for (p, o) in done.into_iter().flatten() {
            outcomes[p].push(o);
        }
        let presets = self.presets.iter().zip(outcomes).map(|((name, _, target), runs)| report(name, *target, &runs)).collect();
        Report { presets }
    }
}

fn report(name: &str, target: Option<(f32, f32)>, runs: &[Outcome]) -> PresetReport {
    let n = runs.len().max(1) as f32;
    let mut causes: BTreeMap<&str, usize> = BTreeMap::new();
    for r in runs.iter().filter(|r| !r.cause.is_empty()) {
        *causes.entry(&r.cause).or_default() += 1;
    }
    let mut causes: Vec<(String, f32)> = causes.into_iter().map(|(c, k)| (c.to_string(), 100.0 * k as f32 / n)).collect();
    causes.sort_by(|a, b| b.1.total_cmp(&a.1));
    let wins: Vec<bool> = runs.iter().filter_map(|r| r.won).collect();
    let win_rate = (!wins.is_empty()).then(|| 100.0 * wins.iter().filter(|w| **w).count() as f32 / wins.len() as f32);
    let mut names: Vec<&'static str> = Vec::new();
    for r in runs {
        for (k, _) in &r.stats {
            if !names.contains(k) {
                names.push(k);
            }
        }
    }
    let stats = names.into_iter().map(|k| (k, Spread::of(&runs.iter().filter_map(|r| r.stats.iter().find(|s| s.0 == k).map(|s| s.1)).collect::<Vec<_>>()))).collect();
    let seconds = Spread::of(&runs.iter().map(|r| r.seconds).collect::<Vec<_>>());
    let mut flags: Vec<(&'static str, f32)> = Vec::new();
    for r in runs {
        for (k, _) in &r.flags {
            if !flags.iter().any(|f| f.0 == *k) {
                let on = runs.iter().filter(|r| r.flags.iter().any(|f| f.0 == *k && f.1)).count();
                flags.push((k, 100.0 * on as f32 / n));
            }
        }
    }
    let mut ratios: Vec<(&'static str, f32)> = Vec::new();
    for r in runs {
        for (k, _, _) in &r.ratios {
            if !ratios.iter().any(|f| f.0 == *k) {
                let (part, whole) = runs.iter().flat_map(|r| r.ratios.iter().filter(|x| x.0 == *k)).fold((0.0, 0.0), |a, x| (a.0 + x.1, a.1 + x.2));
                ratios.push((k, if whole > 0.0 { 100.0 * part / whole } else { 0.0 }));
            }
        }
    }
    PresetReport { name: name.to_string(), rounds: runs.len(), seconds, target, causes, win_rate, stats, flags, ratios }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A "game" whose round lasts skill × (1 + seed % 10) seconds.
    fn play(skill: f32, seed: u64) -> Outcome {
        let s = skill * (1 + seed % 10) as f32;
        Outcome::new(s).cause(if seed.is_multiple_of(4) { "slipped" } else { "too slow" }).stat("score", s * 10.0).flag("short", s < 3.0).ratio("hits", 1.0, 4.0)
    }

    #[test]
    fn the_report_has_each_presets_spread_causes_and_stats() {
        let r = Difficulty::new(100).preset("slow", 1.0f32).target(4.0, 7.0).preset("fast", 10.0).target(100.0, 200.0).run(play);
        let slow = r.get("slow");
        assert_eq!(slow.seconds, Spread { p10: 2.0, p50: 6.0, p90: 10.0 });
        assert!(slow.on_target() && !r.get("fast").on_target());
        assert_eq!(slow.causes, [("too slow".to_string(), 75.0), ("slipped".to_string(), 25.0)]);
        assert_eq!(slow.stat("score").unwrap().p50, 60.0);
        assert_eq!((slow.flags.clone(), slow.ratios.clone()), (vec![("short", 20.0)], vec![("hits", 25.0)]));
        r.assert_skill_matters();
        let text = r.to_string();
        assert!(text.contains("slow") && text.contains("target p50 4-7 s: ok") && text.contains("OFF") && text.contains("short 20%"), "{text}");
        // Parallel or not, the same seeds give the same report.
        assert_eq!(r, Difficulty::new(100).preset("slow", 1.0f32).target(4.0, 7.0).preset("fast", 10.0).target(100.0, 200.0).run(play));
    }

    #[test]
    fn a_saved_report_shows_what_an_edit_changed_everywhere() {
        let path = std::env::temp_dir().join(format!("mb-bot-report-{}.tsv", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let run = |k: f32| Difficulty::new(100).preset("slow", k).preset("fast", 10.0).run(play);
        assert!(run(1.0).compare_with(&path).is_empty(), "the first run is the baseline");
        assert!(run(1.0).compare_with(&path).is_empty(), "seeded: the same game, the same report");
        let changed = run(2.0).compare_with(&path);
        assert!(changed.iter().any(|c| c.starts_with("slow: round p10/p50/p90 2.0/6.0/10.0 s -> 4.0/12.0/20.0 s (+100%)")), "{changed:?}");
        assert!(changed.iter().any(|c| c.starts_with("slow: short 20% -> ")), "{changed:?}");
        assert!(!changed.iter().any(|c| c.starts_with("fast")), "fast didn't change: {changed:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[should_panic(expected = "should do better")]
    fn skill_that_doesnt_matter_fails() {
        Difficulty::new(20).preset("a", 2.0f32).preset("b", 2.0).run(play).assert_skill_matters();
    }

    #[test]
    fn nans_and_win_rates_are_reported_not_panicked_on() {
        let r = Difficulty::new(10).preset("x", ()).run(|_, s| Outcome::new(if s == 3 { f32::NAN } else { s as f32 }).won(s % 2 == 0));
        assert_eq!(r.get("x").win_rate, Some(50.0));
        assert_eq!(r.get("x").seconds.p50, 6.0, "the NaN sorts last");
        assert_eq!(pct(0.5), "0.5%");
    }
}
