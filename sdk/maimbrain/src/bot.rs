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
//! **How events feel.** Difficulty numbers can look fine while one kind
//! of event wears the player down (wind that's "constant and annoying").
//! Tag what the world does with its time in the round,
//! `.event("gust", t)` (or `.span("wind", t0, t1)` for something that
//! lasts), and the report shows, per kind, how often it comes, how regular
//! its spacing is (the coefficient of variation of the gaps: 0 is a
//! metronome), how much of the round it covers and the longest stretch it
//! has the round to itself, and flags the ones that feel constant
//! ([`Pacing::constant`]); `report.assert_events_vary()` fails on them.
//!
//! **The mix.** Each kind's share of all the events tagged is shown too, and
//! one kind over [`Mix::DOMINANT`] % of them is flagged DOMINANT (the wind is
//! "too often"; `assert_events_vary` fails on it). Two kinds that read as
//! the same thing to a player count together: `.group("wind", &["gust",
//! "breath"])` on the [`Difficulty`].
//!
//! **What one miss costs.** A round's length can look fine while one missed
//! gust burns 90% of the flame. Report each miss as it happens,
//! `.miss("gust", t, lost / full)` (the share of a full life it took), and
//! the report shows per threat how often it's missed and what one costs
//! (p50 and worst), flagging HARSH ones that can take more than
//! [`MissCost::HARSH`] % of a life at once (`assert_misses_forgivable()`).
//!
//! **Do the hints work?** [`Literal`] is a player who does exactly what the
//! game's [`Hints`](crate::ui::Hints) say, with a first-timer's reaction
//! time: it holds from HOLD until LET GO and taps what TAP names. Play it as
//! a preset marked `.follows_hints(secs)`: the report shows whether
//! following the hints cost it life or the round in its first `secs`, and
//! `assert_hints_safe()` fails if it did (a hint that asks for something
//! that hurts, or a threat no hint told it about).
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
    /// What the world did, by kind: (kind, start, end) in seconds into the
    /// round (end = start for a moment). See [`Pacing`].
    pub events: Vec<(&'static str, f32, f32)>,
    /// What each miss cost: (threat, when, share of a full life lost, 0–1).
    /// See [`MissCost`].
    pub misses: Vec<(&'static str, f32, f32)>,
}

impl Outcome {
    pub fn new(seconds: f32) -> Outcome {
        Outcome { seconds, cause: String::new(), won: None, stats: Vec::new(), flags: Vec::new(), ratios: Vec::new(), events: Vec::new(), misses: Vec::new() }
    }
    /// The player missed a `threat` `t` s into the round (a gust not
    /// blocked, a lunge not stopped) and it cost `cost` of a full life
    /// (0–1: `lost / full`; 1 is all of it). Report each miss once, when its
    /// damage is done, so one long gust is one miss.
    pub fn miss(mut self, threat: &'static str, t: f32, cost: f32) -> Self {
        self.misses.push((threat, t, cost.max(0.0)));
        self
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
    /// Something the world did `t` s into the round (a gust, a wave, a
    /// falling rock): the things the player has to react to, not their own
    /// moves. Tag them in the sim as they happen and pass them on here.
    pub fn event(mut self, kind: &'static str, t: f32) -> Self {
        self.events.push((kind, t, t));
        self
    }
    /// Something that lasts, from `t0` to `t1` s into the round (the wind
    /// blowing, a storm, a boss on screen).
    pub fn span(mut self, kind: &'static str, t0: f32, t1: f32) -> Self {
        self.events.push((kind, t0, t1.max(t0)));
        self
    }
    /// Many at once: `(kind, start, end)`, as a sim's log keeps them.
    pub fn events(mut self, events: impl IntoIterator<Item = (&'static str, f32, f32)>) -> Self {
        self.events.extend(events.into_iter().map(|(k, a, b)| (k, a, b.max(a))));
        self
    }
}

/// How one kind of event is paced over a preset's rounds (from
/// [`Outcome::event`] and [`Outcome::span`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Pacing {
    pub kind: &'static str,
    /// How often it starts, per minute of play (all rounds' together).
    pub per_min: f32,
    /// How regular its spacing is: the coefficient of variation (SD ÷
    /// mean) of the gaps between its starts, the median of the rounds with
    /// 3 or more gaps. 0 is a metronome; random (Poisson) arrivals are about
    /// 1. None when too few rounds had enough of it.
    pub cv: Option<f32>,
    /// % of play time it's going on (spans; moments count for nothing).
    pub cover: f32,
    /// The longest stretch it has the round to itself, per round (s): from
    /// the start of a run of it with nothing else tagged in between to the
    /// end of the run's last. Only counted in rounds that tag other kinds too.
    pub alone: Spread,
    /// Rounds it came up in, %.
    pub rounds: f32,
    /// Its share of all the events tagged (each moment or span counts
    /// once), %. See [`Mix`].
    pub share: f32,
}

impl Pacing {
    /// Gaps this regular (CV under it: within about ±35%) with at least
    /// `REGULAR_MIN` a round read as clockwork. Random gaps in a range (say
    /// 6-20 s) are about 0.3.
    pub const REGULAR_CV: f32 = 0.2;
    pub const REGULAR_MIN: f32 = 6.0;
    /// This often (per minute) reads as relentless, however it's spaced.
    pub const FREQUENT_PER_MIN: f32 = 12.0;
    /// On this much of the time (%) reads as constant.
    pub const COVER: f32 = 40.0;
    /// Alone this long (p50, s) and for this share of the median round, it's all the round is.
    pub const ALONE_SECS: f32 = 20.0;
    pub const ALONE_SHARE: f32 = 0.5;

    /// Why it would feel constant (None if it doesn't): like clockwork,
    /// relentless, on most of the time, or the whole round to itself.
    /// `round_p50` is the preset's median round length.
    pub fn constant(&self, round_p50: f32) -> Option<String> {
        let mut why = Vec::new();
        let per_round = self.per_min * round_p50 / 60.0;
        if let Some(cv) = self.cv
            && cv < Self::REGULAR_CV
            && per_round >= Self::REGULAR_MIN
        {
            why.push(format!("like clockwork (gaps vary by {:.0}%)", cv * 100.0));
        }
        if self.per_min >= Self::FREQUENT_PER_MIN {
            why.push(format!("relentless (every {:.1} s)", 60.0 / self.per_min));
        }
        if self.cover >= Self::COVER {
            why.push(format!("on {:.0}% of the time", self.cover));
        }
        if self.alone.p50 >= Self::ALONE_SECS && self.alone.p50 >= Self::ALONE_SHARE * round_p50 {
            why.push(format!("alone for {:.0} s at a stretch", self.alone.p50));
        }
        (!why.is_empty()).then(|| why.join(", "))
    }
}

/// Each kind's [`Pacing`] over these rounds, in the order kinds first appear.
fn pacing(runs: &[Outcome]) -> Vec<Pacing> {
    let mut kinds: Vec<&'static str> = Vec::new();
    for r in runs {
        for e in &r.events {
            if !kinds.contains(&e.0) {
                kinds.push(e.0);
            }
        }
    }
    let play: f32 = runs.iter().map(|r| r.seconds.max(0.0)).filter(|s| s.is_finite()).sum::<f32>().max(1e-3);
    let all = runs.iter().map(|r| r.events.len()).sum::<usize>().max(1) as f32;
    kinds
        .into_iter()
        .map(|kind| {
            let mut count = 0usize;
            let mut on = 0.0f32;
            let mut cvs = Vec::new();
            let mut alone = Vec::new();
            let mut seen = 0usize;
            for r in runs {
                let mut ev: Vec<(&'static str, f32, f32)> = r.events.clone();
                ev.sort_by(|a, b| a.1.total_cmp(&b.1));
                let mine: Vec<&(&str, f32, f32)> = ev.iter().filter(|e| e.0 == kind).collect();
                if mine.is_empty() {
                    continue;
                }
                seen += 1;
                count += mine.len();
                // Time covered: the union of its spans.
                let mut end = f32::MIN;
                for e in &mine {
                    let a = e.1.max(end);
                    if e.2 > a {
                        on += e.2 - a;
                    }
                    end = end.max(e.2);
                }
                let gaps: Vec<f32> = mine.windows(2).map(|w| w[1].1 - w[0].1).collect();
                if gaps.len() >= 3 {
                    let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
                    if mean > 0.0 {
                        let var = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f32>() / gaps.len() as f32;
                        cvs.push(var.sqrt() / mean);
                    }
                }
                if ev.iter().any(|e| e.0 != kind) {
                    let (mut best, mut run): (f32, Option<(f32, f32)>) = (0.0, None);
                    for e in &ev {
                        if e.0 == kind {
                            let (a, b) = run.unwrap_or((e.1, e.2));
                            run = Some((a, b.max(e.2)));
                        } else if let Some((a, b)) = run.take() {
                            best = best.max(b.min(e.1) - a);
                        }
                    }
                    // A trailing run ends with its own last event (a span still going at the
                    // round's end already ends there), not the round: a one-off moment late
                    // in a round isn't "alone" for the rest of it.
                    if let Some((a, b)) = run {
                        best = best.max(b - a);
                    }
                    alone.push(best);
                }
            }
            let cv = (!cvs.is_empty() && cvs.len() * 4 >= seen).then(|| Spread::of(&cvs).p50);
            Pacing { kind, per_min: 60.0 * count as f32 / play, cv, cover: 100.0 * on / play, alone: Spread::of(&alone), rounds: 100.0 * seen as f32 / runs.len().max(1) as f32, share: 100.0 * count as f32 / all }
        })
        .collect()
}

/// Kinds of event that read as one thing to a player (a gust and the
/// witch's breath are both wind), and their share of all the events tagged.
/// From [`Difficulty::group`].
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    pub name: &'static str,
    pub kinds: Vec<&'static str>,
    /// % of all the events tagged.
    pub share: f32,
}

impl Mix {
    /// More than this share (%) of everything that happens being one thing
    /// (a kind, or a group) reads as "it's always that". 43% wind was "too
    /// often still". Judged only with three or more things tagged, so tag
    /// everything the world does.
    pub const DOMINANT: f32 = 35.0;
}

/// What missing one kind of threat costs, over a preset's rounds (from
/// [`Outcome::miss`]).
#[derive(Clone, Debug, PartialEq)]
pub struct MissCost {
    pub threat: &'static str,
    /// How often it's missed, per minute of play.
    pub per_min: f32,
    /// What one miss costs, % of a full life: the median, and the worst one.
    pub p50: f32,
    pub worst: f32,
    /// Rounds with at least one, %.
    pub rounds: f32,
}

impl MissCost {
    /// One miss taking more than this (% of a full life) reads as dying at
    /// random: a candle that "burned out very quickly, randomly" lost 90% of
    /// its flame to one missed gust. A game where any hit ends the round
    /// says so with its `cause` instead.
    pub const HARSH: f32 = 50.0;

    /// One miss of it can take more than [`HARSH`](Self::HARSH).
    pub fn harsh(&self) -> bool {
        self.worst > Self::HARSH
    }
}

/// How a preset that follows the hints ([`Literal`], marked with
/// [`Difficulty::follows_hints`]) fared in the stretch the hints cover.
#[derive(Clone, Debug, PartialEq)]
pub struct HintSafety {
    /// The stretch checked: the first `within` s of each round.
    pub within: f32,
    /// Rounds where a miss in it cost at least [`SCRATCH`](Self::SCRATCH), %.
    pub hurt: f32,
    /// Each threat that did: % of rounds.
    pub by: Vec<(&'static str, f32)>,
    /// Rounds lost inside it (ended, not won), %.
    pub lost: f32,
}

impl HintSafety {
    /// Misses costing less than this share of a life (0–1) are scratches,
    /// not the hints' fault.
    pub const SCRATCH: f32 = 0.05;

    /// Following the hints cost life or the round.
    pub fn hurts(&self) -> bool {
        self.hurt > 0.0 || self.lost > 0.0
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
    /// How each kind of event is paced (see [`Outcome::event`]).
    pub events: Vec<Pacing>,
    /// The groups' shares of all the events (see [`Difficulty::group`]).
    pub groups: Vec<Mix>,
    /// What one miss of each threat costs (see [`Outcome::miss`]).
    pub misses: Vec<MissCost>,
    /// For a preset that follows the hints: whether that hurt it.
    pub hints: Option<HintSafety>,
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

    /// One kind of event's pacing.
    pub fn pacing(&self, kind: &str) -> Option<&Pacing> {
        self.events.iter().find(|p| p.kind == kind)
    }

    /// The kinds of event that feel constant, and why.
    pub fn constant_events(&self) -> Vec<(&'static str, String)> {
        self.events.iter().filter_map(|p| p.constant(self.seconds.p50).map(|w| (p.kind, w))).collect()
    }

    /// The kinds and groups that are more than [`Mix::DOMINANT`] % of all
    /// events: (name, share %). A kind in a group counts only in its group.
    /// Nothing when fewer than three things (kinds or groups) are tagged:
    /// two always split it at least half and half.
    pub fn dominant_events(&self) -> Vec<(String, f32)> {
        let grouped = |k: &str| self.groups.iter().any(|g| g.kinds.contains(&k));
        let units: Vec<(String, f32)> = self
            .groups
            .iter()
            .filter(|g| g.share > 0.0)
            .map(|g| (format!("{} ({})", g.name, g.kinds.join(" + ")), g.share))
            .chain(self.events.iter().filter(|e| !grouped(e.kind)).map(|e| (e.kind.to_string(), e.share)))
            .collect();
        if units.len() < 3 {
            return Vec::new();
        }
        units.into_iter().filter(|u| u.1 > Mix::DOMINANT).collect()
    }

    /// The threats one miss of which can take more than [`MissCost::HARSH`].
    pub fn harsh_misses(&self) -> Vec<&MissCost> {
        self.misses.iter().filter(|m| m.harsh()).collect()
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
            for e in &p.events {
                out += &format!("event\t{}\t{}\t{:.1}\t{}\t{:.1}\t{:.1}\n", p.name, e.kind, e.per_min, e.cv.map_or("-".into(), |c| format!("{c:.2}")), e.cover, e.alone.p50);
            }
            for e in &p.events {
                out += &format!("mix\t{}\t{}\t{:.1}\n", p.name, e.kind, e.share);
            }
            for g in &p.groups {
                out += &format!("mix\t{}\t{} ({})\t{:.1}\n", p.name, g.name, g.kinds.join(" + "), g.share);
            }
            for m in &p.misses {
                out += &format!("miss\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\n", p.name, m.threat, m.per_min, m.p50, m.worst);
            }
            if let Some(h) = &p.hints {
                out += &format!("hints\t{}\t\t{:.1}\t{:.1}\t{:.1}\n", p.name, h.within, h.hurt, h.lost);
            }
        }
        out
    }

    /// Fails if any kind of event feels constant for any preset (like
    /// clockwork, relentless, on most of the time or alone for most of the
    /// round: [`Pacing::constant`]), or one kind or group is more than
    /// [`Mix::DOMINANT`] % of all events. Vary its spacing (random gaps
    /// within a range, lulls after a burst), its strength and what comes
    /// with it; give a dominant one's place to something else.
    pub fn assert_events_vary(&self) {
        let mut flagged: Vec<String> = self.presets.iter().flat_map(|p| p.constant_events().into_iter().map(move |(k, w)| format!("{}: {k} is {w}", p.name))).collect();
        flagged.extend(self.presets.iter().flat_map(|p| p.dominant_events().into_iter().map(move |(k, s)| format!("{}: {k} is {} of all events (over {:.0}%)", p.name, pct(s), Mix::DOMINANT))));
        assert!(flagged.is_empty(), "events that feel constant or crowd out the rest:\n  {}\n{self}", flagged.join("\n  "));
    }

    /// Fails if missing one threat once can cost more than
    /// [`MissCost::HARSH`] % of a life, for any preset. Make one miss
    /// cost less (a gust that dims the flame, not snuffs it), or show the
    /// danger rising so the player can still save it.
    pub fn assert_misses_forgivable(&self) {
        let harsh: Vec<String> = self.presets.iter().flat_map(|p| p.harsh_misses().into_iter().map(move |m| format!("{}: one {} miss costs up to {} of a life (p50 {})", p.name, m.threat, pct(m.worst), pct(m.p50)))).collect();
        assert!(harsh.is_empty(), "misses that cost too much at once:\n  {}\n{self}", harsh.join("\n  "));
    }

    /// Fails if a preset that follows the hints ([`Difficulty::follows_hints`])
    /// lost life or the round in the stretch they cover: a hint asked for
    /// something that hurts (a hold that suffocates the flame), showed too
    /// late to act on, or a threat came with no hint, or its hint was hidden
    /// by another (`Hints::unseen`). Trace a round with [`Literal::log`].
    pub fn assert_hints_safe(&self) {
        let checked: Vec<&PresetReport> = self.presets.iter().filter(|p| p.hints.is_some()).collect();
        assert!(!checked.is_empty(), "no preset follows the hints: mark the Literal bot's with .follows_hints(secs)");
        let hurt: Vec<String> = checked
            .iter()
            .filter_map(|p| {
                let h = p.hints.as_ref()?;
                let by: Vec<String> = h.by.iter().map(|(k, v)| format!("{k} {}", pct(*v))).collect();
                h.hurts().then(|| format!("{}: following the hints lost life in {} of rounds ({}) and the round in {}", p.name, pct(h.hurt), by.join(", "), pct(h.lost)))
            })
            .collect();
        assert!(hurt.is_empty(), "the hints hurt the player who follows them:\n  {}\n{self}", hurt.join("\n  "));
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
            let dominant = p.dominant_events();
            let flag_mix = |name: &str, share: f32| if dominant.iter().any(|d| d.0 == name) { format!("   DOMINANT: {} of all events", pct(share)) } else { String::new() };
            for e in &p.events {
                let cv = e.cv.map_or(String::new(), |c| format!(", gaps cv {c:.2}"));
                let cover = if e.cover > 0.0 { format!(", on {}", pct(e.cover)) } else { String::new() };
                let alone = if e.alone.p50 > 0.0 { format!(", alone up to {:.0} s", e.alone.p50) } else { String::new() };
                let flag = e.constant(p.seconds.p50).map_or(String::new(), |w| format!("   CONSTANT: {w}"));
                let mix = flag_mix(e.kind, e.share);
                writeln!(f, "{:<w$}  {} {:.1}/min{cv}{cover}{alone}, {} of events{flag}{mix}", "", e.kind, e.per_min, pct(e.share))?;
            }
            for g in &p.groups {
                let name = format!("{} ({})", g.name, g.kinds.join(" + "));
                writeln!(f, "{:<w$}  {name}: {} of events{}", "", pct(g.share), flag_mix(&name, g.share))?;
            }
            for m in &p.misses {
                let flag = if m.harsh() { format!("   HARSH: one miss can take {} of a life", pct(m.worst)) } else { String::new() };
                writeln!(f, "{:<w$}  missed {} {:.1}/min, one costs {} of a life (worst {}){flag}", "", m.threat, m.per_min, pct(m.p50), pct(m.worst))?;
            }
            if let Some(h) = &p.hints {
                let within = if h.within.is_finite() { format!("first {:.0} s", h.within) } else { "whole round".into() };
                let by: Vec<String> = h.by.iter().map(|(k, v)| format!("{k} {}", pct(*v))).collect();
                let by = if by.is_empty() { String::new() } else { format!(" ({})", by.join(", ")) };
                let flag = if h.hurts() { "   HINTS HURT" } else { "   ok" };
                writeln!(f, "{:<w$}  following the hints ({within}): lost life in {}{by}, lost the round in {}{flag}", "", pct(h.hurt), pct(h.lost))?;
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
            ("event", Some(w)) if w.len() != now.len() || w.iter().zip(now).any(|(a, b)| (a - b).abs() > 0.05 * a.abs().max(1.0)) => {
                let show = |v: &[f32]| format!("{:.1}/min{}, on {:.0}%", v.first().unwrap_or(&0.0), if v.len() == 4 { format!(" cv {:.2}", v[1]) } else { String::new() }, v.get(v.len().saturating_sub(2)).unwrap_or(&0.0));
                Some(format!("{preset}: {name} {} -> {}", show(w), show(now)))
            }
            ("mix", Some(w)) if (w[0] - now[0]).abs() >= 0.5 => Some(format!("{preset}: {name} {} of events -> {}", pct(w[0]), pct(now[0]))),
            ("miss", Some(w)) if w.len() == 3 && now.len() == 3 && (w[1] != now[1] || w[2] != now[2]) => {
                Some(format!("{preset}: one {name} miss costs {} (worst {}) -> {} (worst {})", pct(w[1]), pct(w[2]), pct(now[1]), pct(now[2])))
            }
            ("hints", Some(w)) if w.len() == 3 && now.len() == 3 && (w[1] != now[1] || w[2] != now[2]) => {
                Some(format!("{preset}: following the hints lost life in {} -> {}, the round in {} -> {}", pct(w[1]), pct(now[1]), pct(w[2]), pct(now[2])))
            }
            ("ended", None) => Some(format!("{preset}: new way to end: {name} {}", pct(now[0]))),
            ("miss", None) => Some(format!("{preset}: new miss {name}: costs {} (worst {})", pct(now.get(1).copied().unwrap_or(0.0)), pct(now.get(2).copied().unwrap_or(0.0)))),
            ("seconds" | "won" | "stat" | "share" | "event" | "mix" | "hints", None) => Some(format!("{preset}: new {kind} {name}")),
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
    groups: Vec<(&'static str, Vec<&'static str>)>,
}

/// A preset: its name, the player, its p50 target band, and the stretch
/// its hints cover if it follows them.
type Preset<P> = (String, P, Option<(f32, f32)>, Option<f32>);

impl<P: Copy + Send + Sync> Difficulty<P> {
    /// `rounds` rounds per preset (200 is enough to trust a p50 to a second
    /// or two), seeded 0, 1, 2…
    pub fn new(rounds: usize) -> Self {
        Difficulty { rounds: rounds.max(1), first_seed: 0, presets: Vec::new(), groups: Vec::new() }
    }

    /// Kinds of event that read as one thing to a player (a gust and the
    /// witch's breath are both "wind"): the report's mix counts them
    /// together ([`Mix`]).
    pub fn group(mut self, name: &'static str, kinds: &[&'static str]) -> Self {
        self.groups.push((name, kinds.to_vec()));
        self
    }

    /// The last preset only does what the game's hints say ([`Literal`]):
    /// the report checks it loses no life ([`Outcome::miss`]) and no round
    /// (ended before `secs` and not `won`) in the first `secs` of a round,
    /// the stretch the hints cover: the teaching stretch, or for a game that
    /// hints every threat, the round's time limit (`f32::INFINITY` if it can
    /// be won). [`Report::assert_hints_safe`].
    pub fn follows_hints(mut self, secs: f32) -> Self {
        if let Some(p) = self.presets.last_mut() {
            p.3 = Some(secs);
        }
        self
    }

    /// Seeds start here instead of 0.
    pub fn seeds_from(mut self, seed: u64) -> Self {
        self.first_seed = seed;
        self
    }

    /// A kind of player, by name: whatever your play function takes (your
    /// bot's skill: a `synth::Skill`, or yours built on one).
    pub fn preset(mut self, name: &str, preset: P) -> Self {
        self.presets.push((name.to_string(), preset, None, None));
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
        let presets = self.presets.iter().zip(outcomes).map(|((name, _, target, hints), runs)| report(name, *target, *hints, &self.groups, &runs)).collect();
        Report { presets }
    }
}

/// A player who does exactly what the game's hints say and nothing else:
/// it reads [`Hints::visible`](crate::ui::Hints::visible) every frame and,
/// once a hint has been up for its reaction time (a first-timer's, drawn
/// per hint; it never looks away), taps what a TAP hint names (again every
/// [`TAP_EVERY`](Self::TAP_EVERY) s while it shows), puts a finger down on a
/// HOLD and keeps it there until a LET GO, and ignores everything else. A
/// hint that shows for less than its reaction time is never acted on, as
/// with a person. Play it as a preset marked
/// [`Difficulty::follows_hints`]: if it loses life, the hints told it to
/// do something that hurts or didn't tell it what it needed.
///
/// ```ignore
/// // In the play function, each frame, before stepping the sim:
/// let touches = literal.step(t, DT, &sim.hints, |name| sim.where_is(name));
/// for e in &touches { control.event(e) ... }   // the same path lib.rs uses
/// ```
pub struct Literal {
    skill: crate::input::synth::Skill,
    rng: crate::Rng,
    home: (f32, f32),
    finger: (f32, f32),
    queue: crate::input::synth::Queue,
    /// The hints up now that it has seen: (text, its reaction time, the
    /// next time it may tap for it).
    seen: Vec<(String, f32, f64)>,
    /// Where the held finger is down, if it is.
    held: Option<(f32, f32)>,
    log: Vec<(f64, String)>,
}

impl Literal {
    /// Seconds between taps while a TAP hint stays up.
    pub const TAP_EVERY: f64 = 0.3;
    /// How big a hint's target is taken to be (pt across), for how long the
    /// finger takes to get there.
    pub const TARGET: f32 = 60.0;

    /// A first-timer reading the hints (`seed` draws its reactions).
    pub fn new(seed: u64) -> Literal {
        Literal::with_skill(crate::input::synth::Skill::first_timer(), seed)
    }

    /// Another kind of player's reactions and finger.
    pub fn with_skill(skill: crate::input::synth::Skill, seed: u64) -> Literal {
        Literal { skill, rng: crate::Rng::new(seed), home: (180.0, 480.0), finger: (180.0, 480.0), queue: crate::input::synth::Queue::new(), seen: Vec::new(), held: None, log: Vec::new() }
    }

    /// Where it touches for a hint that names no target, or one the game
    /// can't place (default: the middle of a 360-wide screen, low).
    pub fn home(mut self, x: f32, y: f32) -> Literal {
        self.home = (x, y);
        self.finger = (x, y);
        self
    }

    /// One frame at game time `now` (s), `dt` long: reads the hints up now
    /// and returns the touches due in this frame, to feed the game's control
    /// code as a player's. `locate` gives a target's position by the name
    /// its hint uses (`None`: [`home`](Self::home)).
    pub fn step(&mut self, now: f64, dt: f32, hints: &crate::ui::Hints, locate: impl Fn(&str) -> Option<(f32, f32)>) -> Vec<crate::input::Event> {
        use crate::input::{Kind, synth};
        let visible = hints.visible();
        self.seen.retain(|s| visible.iter().any(|(h, _)| h.text == s.0));
        for (hint, age) in visible {
            let i = match self.seen.iter().position(|s| s.0 == hint.text) {
                Some(i) => i,
                None => {
                    let r = self.skill.reaction_time(&mut self.rng);
                    self.seen.push((hint.text.clone(), r, 0.0));
                    self.seen.len() - 1
                }
            };
            if age < self.seen[i].1 {
                continue;
            }
            let at = hint.target.and_then(&locate).unwrap_or(self.home);
            match hint.ask {
                crate::ui::Ask::Tap if now >= self.seen[i].2 => {
                    let (mut touches, landed) = self.skill.tap(self.finger, at, Self::TARGET, now, &mut self.rng);
                    let id = if self.held.is_some() { 1 } else { 0 };
                    for e in &mut touches {
                        e.id = id;
                    }
                    let down = touches.first().map_or(now, |e| e.time);
                    self.seen[i].2 = down + Self::TAP_EVERY;
                    self.finger = landed;
                    self.log.push((down, format!("tap {} ({})", hint.target.unwrap_or("here"), hint.text)));
                    self.queue.push(touches);
                }
                crate::ui::Ask::Hold if self.held.is_none() => {
                    let d = (at.0 - self.finger.0).hypot(at.1 - self.finger.1);
                    let down = now + self.skill.touch.time(d, Self::TARGET) as f64;
                    self.queue.push([synth::touch(Kind::TouchDown, at.0, at.1, down)]);
                    self.held = Some(at);
                    self.finger = at;
                    self.log.push((down, format!("hold {} ({})", hint.target.unwrap_or("here"), hint.text)));
                }
                crate::ui::Ask::LetGo => {
                    if let Some((x, y)) = self.held.take() {
                        self.queue.push([synth::touch(Kind::TouchUp, x, y, now)]);
                        self.log.push((now, format!("let go ({})", hint.text)));
                    }
                }
                _ => {}
            }
        }
        self.queue.due(now + dt as f64)
    }

    /// A finger is held down.
    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    /// What it did, when and which hint said so ("hold here (KEEP
    /// HOLDING)"): print it beside the sim's trace of one round.
    pub fn log(&self) -> &[(f64, String)] {
        &self.log
    }
}

fn report(name: &str, target: Option<(f32, f32)>, hints: Option<f32>, groups: &[(&'static str, Vec<&'static str>)], runs: &[Outcome]) -> PresetReport {
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
    let events = pacing(runs);
    let groups = groups.iter().map(|(g, kinds)| Mix { name: g, kinds: kinds.clone(), share: events.iter().filter(|e| kinds.contains(&e.kind)).map(|e| e.share).sum() }).collect();
    PresetReport { name: name.to_string(), rounds: runs.len(), seconds, target, causes, win_rate, stats, flags, ratios, events, groups, misses: miss_costs(runs), hints: hints.map(|within| hint_safety(within, runs)) }
}

/// Each threat's [`MissCost`], in the order threats first appear.
fn miss_costs(runs: &[Outcome]) -> Vec<MissCost> {
    let mut threats: Vec<&'static str> = Vec::new();
    for r in runs {
        for m in &r.misses {
            if !threats.contains(&m.0) {
                threats.push(m.0);
            }
        }
    }
    let play: f32 = runs.iter().map(|r| r.seconds.max(0.0)).filter(|s| s.is_finite()).sum::<f32>().max(1e-3);
    let n = runs.len().max(1) as f32;
    threats
        .into_iter()
        .map(|threat| {
            let costs: Vec<f32> = runs.iter().flat_map(|r| r.misses.iter().filter(|m| m.0 == threat).map(|m| 100.0 * m.2)).collect();
            let worst = costs.iter().copied().fold(0.0, f32::max);
            let rounds = runs.iter().filter(|r| r.misses.iter().any(|m| m.0 == threat)).count();
            MissCost { threat, per_min: 60.0 * costs.len() as f32 / play, p50: Spread::of(&costs).p50, worst, rounds: 100.0 * rounds as f32 / n }
        })
        .collect()
}

/// How a preset that follows the hints fared in their first `within` s.
fn hint_safety(within: f32, runs: &[Outcome]) -> HintSafety {
    let n = runs.len().max(1) as f32;
    let hurts = |r: &Outcome, k: Option<&str>| r.misses.iter().any(|m| m.1 <= within && m.2 >= HintSafety::SCRATCH && k.is_none_or(|k| m.0 == k));
    let mut by: Vec<(&'static str, f32)> = Vec::new();
    for r in runs {
        for m in &r.misses {
            if !by.iter().any(|b| b.0 == m.0) && hurts(r, Some(m.0)) {
                by.push((m.0, 100.0 * runs.iter().filter(|r| hurts(r, Some(m.0))).count() as f32 / n));
            }
        }
    }
    by.sort_by(|a, b| b.1.total_cmp(&a.1));
    let hurt = 100.0 * runs.iter().filter(|r| hurts(r, None)).count() as f32 / n;
    let lost = 100.0 * runs.iter().filter(|r| r.seconds < within && r.won != Some(true)).count() as f32 / n;
    HintSafety { within, hurt, by, lost }
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

    /// A 60 s round with wind gusts `gap(i)` apart and a falling rock every 13 s.
    fn windy(seed: u64, gap: impl Fn(u64, usize) -> f32) -> Outcome {
        let mut o = Outcome::new(60.0);
        let (mut t, mut i) = (2.0, 0);
        while t < 60.0 {
            o = o.span("wind", t, t + 1.5);
            t += gap(seed, i);
            i += 1;
        }
        (1..5).fold(o, |o, k| o.event("rock", k as f32 * 13.0))
    }

    #[test]
    fn events_that_come_like_clockwork_are_flagged() {
        let r = Difficulty::new(20).preset("p", ()).run(|_, seed| windy(seed, |_, _| 5.0));
        let wind = r.get("p").pacing("wind").unwrap().clone();
        assert!((wind.per_min - 12.0).abs() < 0.5 && wind.cv == Some(0.0) && (wind.cover - 30.0).abs() < 1.0, "{wind:?}");
        let why = wind.constant(60.0).unwrap();
        assert!(why.contains("clockwork") && why.contains("relentless"), "{why}");
        assert!(r.get("p").pacing("rock").unwrap().constant(60.0).is_none(), "four rocks a round, evenly, aren't constant");
        assert!(r.to_string().contains("wind 12.0/min, gaps cv 0.00, on 30%") && r.to_string().contains("CONSTANT: like clockwork"), "{r}");
        let caught = std::panic::catch_unwind(|| r.assert_events_vary());
        assert!(caught.is_err());
    }

    #[test]
    fn varied_events_pass_and_lasting_ones_are_measured() {
        // Gusts 6-20 s apart, at random: not constant.
        let varied = |seed: u64, i: usize| 6.0 + (crate::Rng::new(seed * 31 + i as u64).next_u32() % 1400) as f32 / 100.0;
        let r = Difficulty::new(40).preset("p", ()).run(move |_, seed| windy(seed, varied));
        let wind = r.get("p").pacing("wind").unwrap();
        assert!(wind.cv.unwrap() > Pacing::REGULAR_CV && wind.per_min < 6.0, "{wind:?}");
        r.assert_events_vary();
        // Wind that blows 70% of the round is constant, however it's spaced.
        let r = Difficulty::new(5).preset("p", ()).run(|_, _| Outcome::new(50.0).span("wind", 5.0, 40.0).event("rock", 45.0));
        let wind = r.get("p").pacing("wind").unwrap();
        assert!((wind.cover - 70.0).abs() < 0.5 && (wind.alone.p50 - 35.0).abs() < 0.5, "{wind:?}");
        assert!(wind.constant(50.0).unwrap().contains("on 70% of the time"));
        // A one-off moment early in a round isn't alone for the rest of it.
        let r = Difficulty::new(5).preset("p", ()).run(|_, _| Outcome::new(60.0).event("rock", 2.0).event("boss", 10.0));
        let boss = r.get("p").pacing("boss").unwrap();
        assert!(boss.alone.p50 < 1.0 && boss.constant(60.0).is_none(), "{boss:?}");
        r.assert_events_vary();
    }

    #[test]
    fn a_saved_report_shows_what_changed_in_the_events() {
        let path = std::env::temp_dir().join(format!("mb-bot-events-{}.tsv", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let run = |gap: f32| Difficulty::new(10).preset("p", ()).run(move |_, seed| windy(seed, move |_, _| gap));
        assert!(run(5.0).compare_with(&path).is_empty());
        assert!(run(5.0).compare_with(&path).is_empty(), "the same events, no change");
        let changed = run(10.0).compare_with(&path);
        assert!(changed.iter().any(|c| c.starts_with("p: wind 12.0/min cv 0.00, on 30% -> 6.0/min")), "{changed:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nans_and_win_rates_are_reported_not_panicked_on() {
        let r = Difficulty::new(10).preset("x", ()).run(|_, s| Outcome::new(if s == 3 { f32::NAN } else { s as f32 }).won(s % 2 == 0));
        assert_eq!(r.get("x").win_rate, Some(50.0));
        assert_eq!(r.get("x").seconds.p50, 6.0, "the NaN sorts last");
        assert_eq!(pct(0.5), "0.5%");
    }

    #[test]
    fn the_mix_shows_each_kinds_share_and_flags_a_dominant_group() {
        // A round: 6 gusts, 3 breaths (both read as wind), 5 chairs, 3 lunges, 3 owls.
        let round = |_, seed: u64| {
            let mut o = Outcome::new(60.0);
            for (k, (kind, n)) in [("gust", 6), ("breath", 3), ("chair", 5), ("lunge", 3), ("owl", 3)].into_iter().enumerate() {
                let mut rng = crate::Rng::new(seed * 8 + k as u64);
                for _ in 0..n {
                    o = o.event(kind, rng.range(1.0, 59.0));
                }
            }
            o
        };
        let r = Difficulty::new(10).preset("p", ()).run(round);
        assert_eq!(r.get("p").pacing("gust").unwrap().share, 30.0);
        assert!(r.get("p").dominant_events().is_empty(), "no one kind is over {}%", Mix::DOMINANT);
        r.assert_events_vary();
        let r = Difficulty::new(10).group("wind", &["gust", "breath"]).preset("p", ()).run(round);
        assert_eq!(r.get("p").dominant_events(), [("wind (gust + breath)".to_string(), 45.0)]);
        let text = r.to_string();
        assert!(text.contains("gust 6.0/min") && text.contains("30% of events") && text.contains("wind (gust + breath): 45% of events   DOMINANT"), "{text}");
        assert!(r.to_tsv().contains("mix\tp\twind (gust + breath)\t45.0\n"));
        let caught = std::panic::catch_unwind(|| r.assert_events_vary()).unwrap_err();
        assert!(caught.downcast_ref::<String>().unwrap().contains("wind (gust + breath) is 45% of all events"));
    }

    /// A candle to keep alight, as "Her Last Candle" was: gusts 6-10 s
    /// apart, each blowing 2.2 s (the curtains stir 1.5 s before), that eat
    /// `gust` of the flame a second unless a finger shields it; a shield
    /// held longer than `smother` s starts to suffocate it. The hints say
    /// HOLD, KEEP HOLDING through a gust and LET GO after.
    struct Candle {
        gust: f32,
        smother: f32,
        t: f32,
        flame: f32,
        held: bool,
        held_for: f32,
        gusts: Vec<(f32, f32)>,
        lost: (f32, f32),
        hints: crate::ui::Hints,
        out: Outcome,
    }

    impl Candle {
        const DT: f32 = 1.0 / 60.0;
        const ROUND: f32 = 40.0;

        fn new(seed: u64, gust: f32, smother: f32) -> Candle {
            let mut rng = crate::Rng::new(seed);
            let (mut gusts, mut t) = (Vec::new(), 3.0);
            while t < Self::ROUND {
                gusts.push((t, t + 2.2));
                t += rng.range(6.0, 10.0);
            }
            Candle { gust, smother, t: 0.0, flame: 1.0, held: false, held_for: 0.0, gusts, lost: (0.0, 0.0), hints: crate::ui::Hints::new(), out: Outcome::new(0.0) }
        }

        fn touch(&mut self, e: &crate::input::Event) {
            if e.id == 0 && (e.is_press() || e.is_release()) {
                self.held = e.is_press();
            }
        }

        /// One frame; false once the round is over.
        fn step(&mut self) -> bool {
            use crate::ui::Hint;
            let (t, dt) = (self.t, Self::DT);
            let blowing = self.gusts.iter().any(|g| (g.0..g.1).contains(&t));
            let stirring = self.gusts.iter().any(|g| (g.0 - 1.5..g.0).contains(&t));
            let starts: Vec<f32> = self.gusts.iter().filter(|g| (g.0..g.0 + dt).contains(&t)).map(|g| g.0).collect();
            for g in starts {
                self.tag(|o| o.event("gust", g));
            }
            self.held_for = if self.held { self.held_for + dt } else { 0.0 };
            let mut lose = 0.0;
            if blowing && !self.held {
                self.lost.0 += self.gust * dt;
                lose += self.gust * dt;
            }
            if self.held_for > self.smother {
                self.lost.1 += 0.6 * dt;
                lose += 0.6 * dt;
            }
            self.flame -= lose;
            self.t += dt;
            let over = self.flame <= 0.0 || self.t >= Self::ROUND;
            let (now, lost) = (self.t, self.lost);
            if lost.0 > 0.0 && (!self.gusts.iter().any(|g| (g.0..g.1).contains(&now)) || over) {
                self.tag(|o| o.miss("gust", now, lost.0));
                self.lost.0 = 0.0;
            }
            if lost.1 > 0.0 && (!self.held || over) {
                self.tag(|o| o.miss("smother", now, lost.1));
                self.lost.1 = 0.0;
            }
            if (stirring || blowing) && !self.held {
                self.hints.want(Hint::hold("HOLD").on("candle").threat());
            } else if blowing {
                self.hints.want(Hint::hold("KEEP HOLDING").on("candle").threat());
            } else if self.held && !stirring {
                self.hints.want(Hint::let_go("LET GO").threat());
            }
            self.hints.update(dt);
            !over
        }

        fn tag(&mut self, f: impl FnOnce(Outcome) -> Outcome) {
            self.out = f(std::mem::replace(&mut self.out, Outcome::new(0.0)));
        }

        fn end(self) -> Outcome {
            let mut o = self.out;
            o.seconds = self.t;
            if self.flame <= 0.0 { o.cause("burned out") } else { o.won(true) }
        }
    }

    /// A round played by `literal` (or by nobody, None).
    fn candle(seed: u64, gust: f32, smother: f32, literal: bool) -> (Outcome, Vec<(f64, String)>) {
        let mut c = Candle::new(seed, gust, smother);
        let mut bot = Literal::new(seed);
        loop {
            if literal {
                for e in bot.step(c.t as f64, Candle::DT, &c.hints, |name| (name == "candle").then_some((180.0, 400.0))) {
                    c.touch(&e);
                }
            }
            if !c.step() {
                break;
            }
        }
        (c.end(), bot.log().to_vec())
    }

    #[test]
    fn one_miss_that_takes_most_of_a_life_is_flagged() {
        // An unshielded gust eats 0.45 of the flame a second: 99% of it in one.
        let r = Difficulty::new(20).preset("hands off", 0.45f32).run(|gust, seed| candle(seed, gust, 99.0, false).0);
        let m = r.get("hands off").misses[0].clone();
        assert!(m.threat == "gust" && m.worst > 95.0 && m.harsh(), "{m:?}");
        assert!(r.to_string().contains("HARSH: one miss can take 9"), "{r}");
        assert!(std::panic::catch_unwind(|| r.assert_misses_forgivable()).is_err());
        // A tenth a second: a miss costs 22%, and three in a row don't end it.
        let gentle = Difficulty::new(20).preset("hands off", 0.1f32).run(|gust, seed| candle(seed, gust, 99.0, false).0);
        assert!((gentle.get("hands off").misses[0].p50 - 22.0).abs() < 1.0, "{gentle}");
        gentle.assert_misses_forgivable();
        let changed = changes(&parse(&r.to_tsv()), &parse(&gentle.to_tsv()));
        assert!(changed.iter().any(|c| c.starts_with("hands off: one gust miss costs 9") && c.contains("-> 22%")), "{changed:?}");
    }

    #[test]
    fn a_hint_that_asks_for_a_deadly_hold_fails_the_literal_bot() {
        // KEEP HOLDING through a 2.2 s gust, but the flame suffocates after 1.45 s held.
        let report = |smother: f32| Difficulty::new(30).preset("follows the hints", smother).follows_hints(f32::INFINITY).run(|smother, seed| candle(seed, 0.45, smother, true).0);
        let r = report(1.45);
        let h = r.get("follows the hints").hints.clone().unwrap();
        assert!(h.hurts() && h.by[0].0 == "smother" && h.lost > 50.0, "{r}");
        assert!(r.to_string().contains("HINTS HURT"), "{r}");
        let caught = std::panic::catch_unwind(|| r.assert_hints_safe()).unwrap_err();
        assert!(caught.downcast_ref::<String>().unwrap().contains("following the hints lost life in 100% of rounds (smother 100%"), "{r}");
        // The fix: a flame that can breathe under a hand for 4 s. Doing what
        // the screen says now keeps it alight.
        let r = report(4.0);
        assert!(!r.get("follows the hints").hints.as_ref().unwrap().hurts(), "{r}");
        r.assert_hints_safe();
        let (_, log) = candle(1, 0.45, 4.0, true);
        let said: Vec<&str> = log.iter().map(|l| l.1.as_str()).take(2).collect();
        assert_eq!(said, ["hold candle (HOLD)", "let go (LET GO)"]);
        assert!(log[0].0 > 1.5 + 0.15 && log[0].0 < 3.0, "a first-timer's reaction and reach after the curtains stir at 1.5 s: {log:?}");
    }

    /// A witch who lunges at 5 s and 12 s for 1.5 s (a tap on her stops
    /// it; otherwise it takes 30% of a life), and a chair that starts
    /// rattling ("TAP TAP TAP") a second before she comes, played by the
    /// literal bot.
    fn witch(seed: u64, lunge_is_a_threat: bool) -> Outcome {
        use crate::ui::{Hint, Hints};
        let (mut hints, mut bot, mut o) = (Hints::new(), Literal::new(seed), Outcome::new(20.0));
        let (mut stopped, mut t) = (false, 0.0f32);
        let place = |name: &str| Some(if name == "witch" { (280.0, 300.0) } else { (80.0, 500.0) });
        while t < 20.0 {
            let lunge = [5.0, 12.0].into_iter().find(|a| (*a..a + 1.5).contains(&t));
            for e in bot.step(t as f64, 1.0 / 60.0, &hints, place) {
                stopped |= lunge.is_some() && e.is_press() && (e.x - 280.0).hypot(e.y - 300.0) < 60.0;
            }
            if [5.0, 12.0].into_iter().any(|a| (a - 1.0..a + 1.5).contains(&t)) {
                hints.want(Hint::tap("TAP TAP TAP", "chair"));
            }
            if lunge.is_some() && !stopped {
                hints.want(if lunge_is_a_threat { Hint::tap("TAP HER", "witch").threat() } else { Hint::tap("TAP HER", "witch") });
            }
            stopped &= lunge.is_some();
            if let Some(a) = lunge
                && !stopped
                && t + 1.0 / 60.0 >= a + 1.5
            {
                o = o.miss("lunge", t, 0.3);
            }
            hints.update(1.0 / 60.0);
            t += 1.0 / 60.0;
        }
        o.won(true).flag("a hint never showed", !hints.unseen().is_empty())
    }

    #[test]
    fn a_threats_hint_hidden_by_an_ambient_one_fails_the_literal_bot() {
        let run = |threat: bool| Difficulty::new(10).preset("literal", threat).follows_hints(f32::INFINITY).run(|threat, seed| witch(seed, threat));
        let r = run(false);
        let p = r.get("literal");
        assert!(p.hints.as_ref().unwrap().by == [("lunge", 100.0)] && p.flags == [("a hint never showed", 100.0)], "{r}");
        assert!(std::panic::catch_unwind(|| r.assert_hints_safe()).is_err());
        let r = run(true);
        assert_eq!(r.get("literal").flags, [("a hint never showed", 0.0)], "{r}");
        r.assert_hints_safe();
    }

    #[test]
    #[should_panic(expected = "no preset follows the hints")]
    fn checking_hints_needs_a_preset_that_follows_them() {
        Difficulty::new(2).preset("p", ()).run(|_, _| Outcome::new(1.0)).assert_hints_safe();
    }
}
