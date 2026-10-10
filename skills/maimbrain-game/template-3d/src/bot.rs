//! A bot that plays like a person, to tune difficulty against (SKILL.md,
//! "Write a bot"). `cargo test -p NAME -- --nocapture difficulty` plays 200
//! seeded rounds per preset and prints how long they last, how they end and
//! how they miss. It's a pattern to adapt as the game changes, not a library:
//! - It only *looks* at the sim (the drone where it's drawn on screen) and only
//!   acts through touch events with their own times, what a finger produces
//!   (`input::synth`). `play` delivers them frame by frame through `control`,
//!   the same mapping lib.rs uses. Never poke sim state from a bot.
//! - It acts on what it saw `delay` ago, and sometimes looks away (a lapse).
//! - One action in flight: it plans the next move only once the current tap
//!   is committed and its result seen. A correction belongs to the move it
//!   corrects, so a stale one can't override a newer move.
//! - Timing: every frame it re-aims the finger where the drone will be when
//!   the finger gets there, and commits when the time left reaches its
//!   intended lead, drawing the timing noise *then*. Don't plan a tap's time
//!   at first sight: people aren't a second off because they saw the drone a
//!   second early.
//! - The first move after each catch overshoots; the correction is slower.

use std::collections::VecDeque;

use maimbrain::Rng;
use maimbrain::input::{Event, synth};

use crate::control::{self, Control};
use crate::sim::{H, Sim, TAP_RADIUS, W};

/// How a kind of player plays: a person's reaction time, lapses, finger
/// speed and precision (Fitts' law) and timing, from `synth::Skill` (the
/// published human numbers; see its docs), plus how this game's player
/// commits a tap. Check them against a real run (SKILL.md, "Difficulty
/// sanity check") before trusting the report.
#[derive(Clone, Copy)]
pub struct Skill {
    pub human: synth::Skill,
    /// How long before the finger lands on the drone it means to tap (s;
    /// > 0 = hurried).
    pub lead: f32,
    /// The correction after an overshoot goes at this fraction of the
    /// first move's speed.
    pub caution: f32,
}

pub const FIRST_TIMER: Skill = Skill { human: synth::Skill::first_timer(), lead: 0.04, caution: 0.5 };
pub const CASUAL: Skill = Skill { human: synth::Skill::casual(), lead: 0.02, caution: 0.6 };
pub const GOOD: Skill = Skill { human: synth::Skill::good(), lead: 0.01, caution: 0.7 };

/// How long before the planned moment a tap is committed (s): the timing
/// noise is drawn here, close to the event, never at first sight. After
/// that the move is ballistic: the finger carries on to where it was going.
const COMMIT: f32 = 0.1;
/// A finger eases in: full speed, then slowing over about this long (s).
const SETTLE: f32 = 0.04;

/// The drone on screen at time `t`.
#[derive(Clone, Copy)]
struct Seen { t: f32, x: f32, y: f32, score: u32 }

/// The one action in flight.
#[derive(Clone, Copy)]
enum Act {
    /// Still until this time: reacting, looking away, or watching a tap land.
    Wait(f32),
    /// The finger chasing the drone from time `from`, first via `over` (an
    /// overshoot: the correction after it is owned by this move).
    Reach { over: Option<(f32, f32)>, speed: f32, from: f32 },
    /// Committed: the finger carries on to `to` and touches down at `at`.
    Tap { at: f32, to: (f32, f32), speed: f32 },
}

pub struct Bot {
    skill: Skill,
    rng: Rng,
    seen: VecDeque<Seen>,
    delay: f32,
    finger: (f32, f32),
    /// This move's aim error.
    err: (f32, f32),
    act: Act,
    /// The score when the current chase began (a change = a catch).
    score: Option<u32>,
    /// About the current chase: it looked away, it tapped (and missed).
    lapsed: bool,
    tapped: bool,
    /// Taps that touched down > 10 units short, the finger still on its way,
    /// or late, the finger there and waiting while the drone moved on (report).
    pub early: u32,
    pub late: u32,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        let mut rng = Rng::new(seed);
        let delay = skill.human.reaction_time(&mut rng);
        let finger = (W * 0.5, H * 0.85);
        Bot { skill, rng, seen: VecDeque::new(), delay, finger, err: (0.0, 0.0), act: Act::Wait(0.0), score: None, lapsed: false, tapped: false, early: 0, late: 0 }
    }

    /// Roughly normal, mean 0, SD 1.
    fn gauss(&mut self) -> f32 {
        synth::gauss(&mut self.rng)
    }

    /// Moves the finger toward `to` for `dt`; true once it's there.
    fn glide(&mut self, to: (f32, f32), speed: f32, dt: f32) -> bool {
        let (dx, dy) = (to.0 - self.finger.0, to.1 - self.finger.1);
        let d = dx.hypot(dy);
        let step = (speed.min(d / SETTLE) * dt).min(d);
        if d > 1e-3 {
            self.finger = (self.finger.0 + dx / d * step, self.finger.1 + dy / d * step);
        }
        d - step < 2.0
    }

    /// How long until the finger, moving at `speed`, is within 2 units of `to`.
    fn time_to(&self, to: (f32, f32), speed: f32) -> f32 {
        let d = (to.0 - self.finger.0).hypot(to.1 - self.finger.1);
        let near = speed * SETTLE;
        (d - near).max(0.0) / speed + SETTLE * (d.min(near) / 2.0).max(1.0).ln()
    }

    /// Where it thinks the drone will be when a finger moving at `speed`
    /// reaches it: judged from what it saw `delay` ago (and 0.1 s before
    /// that), extrapolated in a straight line, so curves fool it a little.
    /// None just after a catch, until it has seen the drone move.
    fn intercept(&self, t: f32, speed: f32) -> Option<(f32, f32)> {
        let at = |when: f32| self.seen.iter().rev().find(|s| s.t <= when).copied();
        let (a, b) = (at(t - self.delay)?, at(t - self.delay - 0.1)?);
        if a.score != b.score || a.t <= b.t {
            return None;
        }
        let v = ((a.x - b.x) / (a.t - b.t), (a.y - b.y) / (a.t - b.t));
        let mut to = (a.x, a.y);
        for _ in 0..3 {
            let k = t - a.t + self.time_to(to, speed);
            to = (a.x + v.0 * k + self.err.0, a.y + v.1 * k + self.err.1);
        }
        Some(to)
    }

    /// A new move toward the drone. The first of a chase overshoots; a retry
    /// after a miss doesn't, and goes cautiously.
    ///
    /// The move is a finger's (Fitts' law): it lands scattered by what its
    /// time allows on the tap area, and chasing a moving drone it goes at
    /// the peak speed of a half-screen move (1.875 × the average, for a
    /// minimum-jerk reach), never slower.
    fn reach(&mut self, t: f32) {
        let Some(s) = self.seen.iter().rev().find(|s| s.t <= t - self.delay).copied() else { return };
        let touch = self.skill.human.touch;
        let d = (s.x - self.finger.0).hypot(s.y - self.finger.1).max(W * 0.5);
        let time = touch.time(d, 2.0 * TAP_RADIUS);
        let sd = touch.scatter(d, time);
        let speed = 1.875 * d / (time - touch.a).max(0.05);
        self.err = (self.gauss() * sd, self.gauss() * sd);
        let Some(to) = self.intercept(t, speed) else { return };
        self.act = if self.tapped {
            Act::Reach { over: None, speed: speed * self.skill.caution, from: t }
        } else {
            let o = self.rng.f32() * 2.0 * touch.overshoot;
            let over = (to.0 + (to.0 - self.finger.0) * o, to.1 + (to.1 - self.finger.1) * o);
            Act::Reach { over: Some(over), speed, from: t }
        };
    }

    /// Sees the sim as shown at time `t`; returns the touches it starts in
    /// (t, t + dt] (a tap's release comes a little later).
    pub fn update(&mut self, sim: &Sim, t: f32, dt: f32) -> Vec<Event> {
        let p = sim.camera().project(sim.drone(), W, H);
        self.seen.push_back(Seen { t, x: p.x, y: p.y, score: sim.score });
        while self.seen.len() > 1 && self.seen[0].t < t - 1.0 {
            self.seen.pop_front();
        }
        let score = self.seen.iter().rev().find(|s| s.t <= t - self.delay).unwrap_or(&self.seen[0]).score;
        if self.score != Some(score) {
            // A catch (or the start): everything about the last chase is dropped.
            self.score = Some(score);
            self.lapsed = self.rng.f32() < self.skill.human.lapse.0;
            self.tapped = false;
            let away = if self.lapsed { self.skill.human.lapse.1 * self.rng.range(0.5, 1.5) } else { 0.0 };
            self.act = Act::Wait(t + away);
        }
        match self.act {
            Act::Wait(until) if t >= until => self.reach(t),
            Act::Reach { over, speed, from } if t >= from => {
                // Tracked every frame: where to meet the drone, and how long
                // until the finger lands there.
                let Some(to) = over.or_else(|| self.intercept(t, speed)) else { return Vec::new() };
                let left = self.time_to(to, speed);
                if over.is_none() && left <= self.skill.lead + COMMIT {
                    // Commit, drawing the timing noise now.
                    let at = t + (left - self.skill.lead + self.gauss() * self.skill.human.timing).max(0.0);
                    self.late += (at - t - left > 0.05) as u32;
                    self.act = Act::Tap { at, to, speed };
                } else if self.glide(to, speed, dt) && over.is_some() {
                    // Overshot: notice (half a reaction), then correct, slower.
                    self.act = Act::Reach { over: None, speed: speed * self.skill.caution, from: t + self.delay * 0.5 };
                }
            }
            Act::Tap { at, to, speed } => {
                // Early, the tap falls short along the way; late, the finger
                // waits where it meant to meet the drone, which moves on.
                self.glide(to, speed, (at - t).clamp(0.0, dt));
                if at <= t + dt {
                    let ((x, y), at) = (self.finger, at.max(t));
                    self.early += ((to.0 - x).hypot(to.1 - y) > 10.0) as u32;
                    self.tapped = true;
                    // Watch it land: a catch shows up as a new score; if not, it missed.
                    self.act = Act::Wait(at + self.delay + 2.0 * dt);
                    return synth::tap(x, y, at);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// What it was doing when the clock ran out: how this round was lost.
    /// (This sim has one way to lose; when yours has several, report the
    /// sim's own cause next to this.)
    pub fn doing(&self) -> &'static str {
        if self.tapped {
            "missed with no time to retry"
        } else if self.lapsed {
            "looked away"
        } else if matches!(self.act, Act::Wait(_)) {
            "still reacting"
        } else {
            "finger couldn't catch it"
        }
    }
}

const DT: f32 = 1.0 / 60.0;

struct Round { seconds: f32, taps: u32, missed: u32, early: u32, late: u32, cause: &'static str }

/// One round. Each frame, as the host runs it: the touches made up to the
/// frame's end, through `control` as in lib.rs, then the step.
fn play(skill: Skill, seed: u64) -> Round {
    let (mut sim, mut bot) = (Sim::new(seed), Bot::new(skill, seed ^ 0x5eed));
    let (mut finger, mut control) = (synth::Queue::new(), Control::default());
    let (mut taps, mut missed) = (0, 0);
    while !sim.over && sim.t < 300.0 {
        finger.push(bot.update(&sim, sim.t, DT));
        for a in finger.due(sim.t + DT).iter().filter_map(|e| control.event(e)) {
            taps += 1;
            missed += control::apply(&mut sim, a).is_none() as u32;
        }
        sim.step(DT);
    }
    let cause = if sim.over { bot.doing() } else { "still going at 300 s" };
    Round { seconds: sim.t, taps, missed, early: bot.early, late: bot.late, cause }
}

#[test]
fn difficulty() {
    println!("\n200 rounds per preset. Targets (SKILL.md): first-timer p50 20-45 s, good 60-180 s.");
    let mut p50 = Vec::new();
    for skill in [FIRST_TIMER, CASUAL, GOOD] {
        let runs: Vec<Round> = (0..200).map(|seed| play(skill, seed)).collect();
        let mut secs: Vec<f32> = runs.iter().map(|r| r.seconds).collect();
        secs.sort_by(f32::total_cmp);
        let pct = |p: usize| secs[p * (secs.len() - 1) / 100];
        let total = |f: fn(&Round) -> u32| runs.iter().map(f).sum::<u32>() as f32;
        let taps = total(|r| r.taps).max(1.0);
        let pc = |f: fn(&Round) -> u32| 100.0 * total(f) / taps;
        println!("{:<12} round p10 {:5.1}s  p50 {:5.1}s  p90 {:5.1}s | {:.1} taps/round, {:.0}% missed; {:.0}% touched down short, {:.0}% late",
            skill.human.name, pct(10), pct(50), pct(90), taps / runs.len() as f32, pc(|r| r.missed), pc(|r| r.early), pc(|r| r.late));
        let mut causes = std::collections::BTreeMap::new();
        for r in &runs {
            *causes.entry(r.cause).or_insert(0) += 1;
        }
        let mut causes: Vec<(&str, usize)> = causes.into_iter().collect();
        causes.sort_by_key(|c| std::cmp::Reverse(c.1));
        let causes: Vec<String> = causes.iter().map(|(c, n)| format!("{c} {}%", n * 100 / runs.len())).collect();
        println!("{:<12} lost: {}", "", causes.join(", "));
        p50.push(pct(50));
    }
    assert!(p50[0] < p50[2], "a good player should outlast a first-timer: {p50:?}");
}
