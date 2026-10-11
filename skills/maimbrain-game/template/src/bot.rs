//! A bot that plays like a person, to tune difficulty against (SKILL.md,
//! "Write a bot"). `cargo test -p NAME -- --nocapture difficulty` plays 200
//! seeded rounds per preset and prints how long they last, how they end and
//! how they miss; `mix_log` records one round's sound for mix_check.py. It's a pattern to adapt as the game changes, not a library:
//! - It only *looks* at the sim (what's on screen) and only acts through touch
//!   events with their own times, what a finger produces (`input::synth`).
//!   `play` delivers them frame by frame through `control`, the same mapping
//!   lib.rs uses. Never poke sim state from a bot.
//! - It acts on what it saw `delay` ago, and sometimes looks away (a lapse).
//!   So after it acts it waits at least `delay` + a frame before judging the
//!   result: any sooner and it sees the moment before its own action.
//! - One action in flight: it plans the next move only once the current tap
//!   is committed and its result seen. A correction belongs to the move it
//!   corrects, so a stale one can't override a newer move.
//! - Timing: it tracks the target every frame and commits when the remaining
//!   time reaches its intended lead, drawing the timing noise *then*. Don't
//!   plan a tap's time at first sight: people aren't a second off because
//!   they saw the thing a second early.
//! - The first move to each new bubble overshoots; the correction is slower.

use std::collections::VecDeque;

use maimbrain::Rng;
use maimbrain::input::{Event, synth};

use crate::control::{self, Control};
use crate::sim::Sim;
use crate::sound::Sound;

/// How a kind of player plays: a person's reaction time, lapses, finger
/// speed and precision (Fitts' law) and timing, from `synth::Skill` (the
/// published human numbers; see its docs), plus how this game's player
/// commits a tap. Check them against a real run (SKILL.md, "Difficulty
/// sanity check") before trusting the report.
#[derive(Clone, Copy)]
pub struct Skill {
    pub human: synth::Skill,
    /// How long before the finger lands it means to tap (s; > 0 = hurried).
    pub lead: f32,
    /// The correction after an overshoot goes at this fraction of the
    /// first move's speed.
    pub caution: f32,
}

pub const FIRST_TIMER: Skill = Skill { human: synth::Skill::first_timer(), lead: 0.05, caution: 0.5 };
pub const CASUAL: Skill = Skill { human: synth::Skill::casual(), lead: 0.03, caution: 0.6 };
pub const GOOD: Skill = Skill { human: synth::Skill::good(), lead: 0.01, caution: 0.7 };

/// How long before the planned moment a tap is committed (s): the timing
/// noise is drawn here, close to the event, never at first sight. After
/// that the move is ballistic: the finger carries on to where it was going.
const COMMIT: f32 = 0.1;
/// A finger eases in: full speed, then slowing over about this long (s).
const SETTLE: f32 = 0.04;

/// What was on screen at time `t`.
#[derive(Clone, Copy)]
struct Seen { t: f32, x: f32, y: f32, r: f32, score: u32 }

/// The one action in flight.
#[derive(Clone, Copy)]
enum Act {
    /// Still until this time: reacting, looking away, or watching a tap land.
    Wait(f32),
    /// The finger moving to `to` from time `from`; after an overshoot, `then`
    /// is the point it really wants (the correction, owned by this move).
    Reach { to: (f32, f32), then: Option<(f32, f32)>, speed: f32, from: f32 },
    /// Committed: the finger carries on to `to` and touches down at `at`.
    Tap { at: f32, to: (f32, f32), speed: f32 },
}

pub struct Bot {
    skill: Skill,
    rng: Rng,
    seen: VecDeque<Seen>,
    delay: f32,
    finger: (f32, f32),
    act: Act,
    /// The score when the current bubble appeared (a change = a new bubble).
    score: Option<u32>,
    /// About the current bubble: it looked away, it has tapped (and missed).
    lapsed: bool,
    tapped: bool,
    /// Taps that touched down > 10 units short, the finger still on its way (report).
    pub early: u32,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        let mut rng = Rng::new(seed);
        let delay = skill.human.reaction_time(&mut rng);
        Bot { skill, rng, seen: VecDeque::new(), delay, finger: (180.0, 560.0), act: Act::Wait(0.0), score: None, lapsed: false, tapped: false, early: 0 }
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

    /// Sees the sim as shown at time `t`; returns the touches it starts in
    /// (t, t + dt] (a tap's release comes a little later).
    pub fn update(&mut self, sim: &Sim, t: f32, dt: f32) -> Vec<Event> {
        self.seen.push_back(Seen { t, x: sim.x, y: sim.y, r: sim.r, score: sim.score });
        while self.seen.len() > 1 && self.seen[0].t < t - 1.0 {
            self.seen.pop_front();
        }
        // What it has taken in: the newest frame at least `delay` old.
        let s = *self.seen.iter().rev().find(|s| s.t <= t - self.delay).unwrap_or(&self.seen[0]);
        if self.score != Some(s.score) {
            // A new bubble: everything about the old one is dropped.
            self.score = Some(s.score);
            self.lapsed = self.rng.f32() < self.skill.human.lapse.0;
            self.tapped = false;
            let away = if self.lapsed { self.skill.human.lapse.1 * self.rng.range(0.5, 1.5) } else { 0.0 };
            self.act = Act::Wait(t + away);
        }
        match self.act {
            Act::Wait(until) if t >= until => {
                // Aim at the bubble as seen, as a finger does (Fitts' law): the
                // move takes longer the farther and smaller the target, and
                // lands scattered by what that time allows. It aims for the middle
                // half of a bubble that is shrinking, not its edge. The first move
                // overshoots; a retry after a miss is cautious from the start.
                let touch = self.skill.human.touch;
                let d = (s.x - self.finger.0).hypot(s.y - self.finger.1);
                let time = touch.time(d, s.r);
                let sd = touch.scatter(d, time);
                let speed = (d / (time - touch.a).max(0.05)).max(100.0);
                let aim = (s.x + self.gauss() * sd, s.y + self.gauss() * sd);
                let (to, then, speed) = if self.tapped {
                    (aim, None, speed * self.skill.caution)
                } else {
                    let k = self.rng.f32() * 2.0 * touch.overshoot;
                    let over = (aim.0 + (aim.0 - self.finger.0) * k, aim.1 + (aim.1 - self.finger.1) * k);
                    (over, Some(aim), speed)
                };
                self.act = Act::Reach { to, then, speed, from: t };
            }
            Act::Reach { to, then, speed, from } if t >= from => {
                // Tracked every frame: how long until the finger lands.
                let left = self.time_to(to, speed);
                if then.is_none() && left <= self.skill.lead + COMMIT {
                    // Commit, drawing the timing noise now.
                    let at = t + (left - self.skill.lead + self.gauss() * self.skill.human.timing).max(0.0);
                    self.act = Act::Tap { at, to, speed };
                } else if self.glide(to, speed, dt) {
                    if let Some(aim) = then {
                        // Overshot: notice (half a reaction), then correct, slower.
                        let from = t + self.delay * 0.5;
                        self.act = Act::Reach { to: aim, then: None, speed: speed * self.skill.caution, from };
                    }
                }
            }
            Act::Tap { at, to, speed } => {
                // A tap before the finger gets there falls short along the way.
                self.glide(to, speed, (at - t).clamp(0.0, dt));
                if at <= t + dt {
                    let ((x, y), at) = (self.finger, at.max(t));
                    self.early += ((to.0 - x).hypot(to.1 - y) > 10.0) as u32;
                    self.tapped = true;
                    // Watch it land: a pop shows up as a new bubble; if none, it missed.
                    // Keep this wait at least `delay` + a frame past the touch: the bot sees
                    // the sim as it was `delay` ago, so sooner it still sees the frame before
                    // its own tap (no a new bubble yet), takes it for a miss and taps the same
                    // target again. Every action that changes what the bot looks at needs it.
                    self.act = Act::Wait(at + self.delay + 2.0 * dt);
                    return synth::tap(x, y, at);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// What it was doing when the round ended: how this round was lost. (This
    /// sim has one way to lose; when yours has several, report the sim's own
    /// cause next to this.)
    pub fn doing(&self) -> &'static str {
        if self.tapped {
            "missed with no time to retry"
        } else if self.lapsed {
            "looked away"
        } else if matches!(self.act, Act::Wait(_)) {
            "still reacting"
        } else {
            "finger too slow"
        }
    }
}

const DT: f32 = 1.0 / 60.0;

struct Round { seconds: f32, taps: u32, missed: u32, early: u32, cause: &'static str }

/// One round. Each frame, as the host runs it: the touches made up to the
/// frame's end, through `control` as in lib.rs, then the step, then the
/// sim's cues into `sound` (if given) as lib.rs does.
fn play(skill: Skill, seed: u64, mut sound: Option<&mut Sound>) -> Round {
    let (mut sim, mut bot) = (Sim::new(seed), Bot::new(skill, seed ^ 0x5eed));
    let (mut finger, mut control) = (synth::Queue::new(), Control::default());
    let (mut t, mut taps, mut missed) = (0.0, 0, 0);
    while !sim.over && t < 300.0 {
        finger.push(bot.update(&sim, t, DT));
        for a in finger.due(t + DT).iter().filter_map(|e| control.event(e)) {
            taps += 1;
            missed += !control::apply(&mut sim, a) as u32;
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        if let Some(sound) = sound.as_deref_mut() {
            sound.update(DT);
            cues.into_iter().for_each(|c| sound.cue(c));
        }
        t += DT;
    }
    let cause = if sim.over { bot.doing() } else { "still going at 300 s" };
    Round { seconds: t, taps, missed, early: bot.early, cause }
}

#[test]
fn difficulty() {
    println!("\n200 rounds per preset. Targets (SKILL.md): first-timer p50 20-45 s, good 60-180 s.");
    let mut p50 = Vec::new();
    for skill in [FIRST_TIMER, CASUAL, GOOD] {
        let runs: Vec<Round> = (0..200).map(|seed| play(skill, seed, None)).collect();
        let mut secs: Vec<f32> = runs.iter().map(|r| r.seconds).collect();
        secs.sort_by(f32::total_cmp);
        let pct = |p: usize| secs[p * (secs.len() - 1) / 100];
        let total = |f: fn(&Round) -> u32| runs.iter().map(f).sum::<u32>() as f32;
        let taps = total(|r| r.taps).max(1.0);
        let pc = |f: fn(&Round) -> u32| 100.0 * total(f) / taps;
        println!("{:<12} round p10 {:5.1}s  p50 {:5.1}s  p90 {:5.1}s | {:.1} taps/round, {:.0}% missed, {:.0}% touched down short",
            skill.human.name, pct(10), pct(50), pct(90), taps / runs.len() as f32, pc(|r| r.missed), pc(|r| r.early));
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

/// One casual round's sound, as `sound.rs` plays it, for the skill's
/// `scripts/mix_check.py` (sound.rs has the commands). `MB_MIX_SEED` picks
/// the round. Rerun it after changing a sound, a level or a cue.
#[test]
fn mix_log() {
    let seed = std::env::var("MB_MIX_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut sound = Sound::new();
    play(CASUAL, seed, Some(&mut sound));
    let log = sound.log.join("\n") + "\n";
    assert!(log.contains(" pop ") && log.contains(" over "), "a round pops bubbles and ends:\n{log}");
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(&path, log).unwrap();
        println!("wrote {path}");
    }
}
