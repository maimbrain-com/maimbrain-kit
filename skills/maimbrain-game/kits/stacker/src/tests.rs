//! Rule tests and a bot that plays like a person, to tune difficulty.
//!
//!     cargo test -p kit_stacker                                    # rules (fast)
//!     cargo test -p kit_stacker --profile bot -- --ignored --nocapture difficulty
//!
//! The second prints round lengths, scores and how each skill preset loses.

use std::collections::BTreeMap;
use std::sync::Mutex;

use maimbrain::Rng;
use maimbrain::bot::{Difficulty, Outcome};
use maimbrain::physics2d::vec2;

use crate::sim::{Cue, ENDING, Kind, LIVES, PLATE_TOP, Sim, State, TOWER_TOP_ON_SCREEN, Twist, TwistKind, UNLOCKS};

const DT: f32 = 1.0 / 60.0;

// ---- The bot ---------------------------------------------------------------

/// How a person plays: they watch the landing ghost, wait until it's over
/// the top of the tower, and tap. They see late (reaction delay), misjudge
/// (noise), and the better ones tap a little early to make up for their own
/// delay (lead).
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Seconds from deciding to tap to the tap landing.
    pub delay: (f32, f32),
    /// How far off their read of the ghost is, px (per snack, roughly 1 sd).
    pub noise: f32,
    /// How much of their own delay they anticipate (0 none … 1 all).
    pub lead: f32,
    /// How close to the target counts as "now!", px.
    pub window: f32,
    /// Waits out gusts and tilts, and for the last snack to settle, instead
    /// of dropping into them.
    pub patient: bool,
    /// Aims this fraction of the way from the top snack's centre back
    /// towards the middle, straightening a leaning tower.
    pub correct: f32,
}

pub const FIRST_TIMER: Skill = Skill { name: "first-timer", delay: (0.2, 0.35), noise: 15.0, lead: 0.15, window: 12.0, patient: false, correct: 0.0 };
pub const DECENT: Skill = Skill { name: "decent", delay: (0.17, 0.28), noise: 9.0, lead: 0.55, window: 9.0, patient: true, correct: 0.15 };
pub const GOOD: Skill = Skill { name: "good", delay: (0.15, 0.22), noise: 5.0, lead: 0.9, window: 6.0, patient: true, correct: 0.35 };

pub struct Bot {
    skill: Skill,
    rng: Rng,
    /// A tap on its way: seconds until it lands.
    pending: Option<f32>,
    /// This snack's misjudgement, px.
    bias: f32,
    /// Seconds this snack has been carried without a good moment.
    waited: f32,
    /// Kinds met (a new kind is misjudged more: overshoot).
    seen: Vec<Kind>,
    /// After losing a heart: extra careful (narrower window) for a few drops.
    caution: u32,
    lives: u32,
    had: bool,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed), pending: None, bias: 0.0, waited: 0.0, seen: Vec::new(), caution: 0, lives: LIVES, had: false }
    }

    fn gauss(&mut self) -> f32 {
        (0..4).map(|_| self.rng.f32()).sum::<f32>() - 2.0
    }

    /// True when the bot taps this frame.
    pub fn think(&mut self, sim: &Sim, dt: f32) -> bool {
        if sim.lives < self.lives {
            self.lives = sim.lives;
            self.caution = 3;
        }
        if let Some(t) = &mut self.pending {
            *t -= dt;
            if *t <= 0.0 {
                self.pending = None;
                self.caution = self.caution.saturating_sub(1);
                return true;
            }
            return false;
        }
        let Some(held) = sim.held else {
            self.had = false;
            return false;
        };
        if !self.had {
            // A new snack: a fresh misjudgement, bigger for a kind never seen.
            self.had = true;
            let new = !self.seen.contains(&held.kind);
            if new {
                self.seen.push(held.kind);
            }
            self.bias = self.gauss() * self.skill.noise * if new { 1.6 } else { 1.0 };
            self.waited = 0.0;
        }
        self.waited += dt;
        let Some(pred) = sim.predict() else { return false };
        let x = pred.land.x;
        // The ghost moves with the bird (people see smooth motion, not the
        // ghost's jumps when it slides off an edge).
        let v = sim.bird_vx();
        if held.age < 0.35 {
            return false;
        }
        if self.waited > 6.0 {
            // Fed up: taps anyway.
            self.pending = Some(self.rng.range(self.skill.delay.0, self.skill.delay.1));
            return false;
        }
        let falling = sim.pieces.iter().any(|p| matches!(p.state, State::Falling { .. }));
        let stormy = falling || matches!(sim.twist, Twist::On { .. } | Twist::Warn { .. });
        if self.skill.patient && stormy && self.waited < 4.0 {
            return false;
        }
        let delay = self.rng.range(self.skill.delay.0, self.skill.delay.1);
        let top = sim.tower_top().x;
        let target = top + (crate::sim::W / 2.0 - top) * self.skill.correct;
        let seen = x + v * delay * self.skill.lead + self.bias;
        let window = if self.caution > 0 { self.skill.window * 0.6 } else { self.skill.window };
        if (seen - target).abs() < window {
            self.pending = Some(delay);
        }
        false
    }
}

#[derive(Default)]
pub struct RoundStats {
    pub seconds: f32,
    pub score: u32,
    pub height: f32,
    /// What fell, when each heart went: (kind, during a twist, seconds, a tumble rather than a miss).
    pub losses: Vec<(Kind, bool, f32, bool)>,
    pub max_awake: usize,
    pub max_bodies: usize,
}

/// Every heart one preset lost: (kind, during a twist, seconds, a tumble);
/// and the most bodies awake, and in the world, in any of its rounds.
#[derive(Default)]
struct Hearts {
    lost: Vec<(Kind, bool, f32, bool)>,
    max_awake: usize,
    max_bodies: usize,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> RoundStats {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut stats = RoundStats::default();
    sim.drop_piece(); // the tap that starts the round
    while !sim.over && sim.t < max_seconds {
        if bot.think(&sim, DT) {
            sim.drop_piece();
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::Lost { kind, counted: true, landed, .. } = c {
                stats.losses.push((kind, !matches!(sim.twist, Twist::Calm { .. }), sim.t, landed));
            }
        }
        stats.max_awake = stats.max_awake.max(sim.world.awake_count());
        stats.max_bodies = stats.max_bodies.max(sim.world.len());
    }
    stats.seconds = sim.t;
    stats.score = sim.score;
    stats.height = sim.best_height;
    stats
}

fn steps(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.step(DT);
    }
}

// ---- Rules -------------------------------------------------------------------

#[test]
fn the_first_drop_always_lands() {
    for seed in 0..8 {
        let mut sim = Sim::new(seed);
        assert_eq!(sim.held.unwrap().kind, Kind::Box, "the first snack is the easy one");
        assert!(sim.drop_piece());
        steps(&mut sim, 120);
        assert_eq!((sim.score, sim.lives), (1, LIVES), "seed {seed}");
        assert!(sim.cues.iter().any(|c| matches!(c, Cue::Land { neat: true, .. })), "dead centre is neat");
        assert!(sim.held.is_some(), "the bird has the next one");
    }
}

/// Whatever a player does in the first seconds (mash, or tap at random
/// moments), no heart is lost.
#[test]
fn the_first_seconds_cannot_fail() {
    for seed in 0..12 {
        let mut sim = Sim::new(seed);
        let mut rng = Rng::new(seed + 100);
        sim.drop_piece();
        let mut next = rng.range(0.6, 1.2);
        while sim.t < 3.5 {
            if sim.t >= next {
                sim.drop_piece();
                next = sim.t + rng.range(0.6, 1.2);
            }
            sim.step(DT);
        }
        assert_eq!(sim.lives, LIVES, "seed {seed} lost a heart in the first seconds");
    }
}

#[test]
fn a_fall_costs_a_heart_and_the_last_one_ends_the_round() {
    let mut sim = Sim::new(7);
    let (mut lost, mut collapse) = (0, 0);
    while !sim.over && sim.t < 60.0 {
        // Let go and fling it sideways: always a miss.
        if sim.held.is_some_and(|h| h.age > 0.3) && sim.drop_piece() {
            let id = sim.pieces.last().unwrap().id;
            sim.world.add_velocity(id, vec2(900.0, -150.0));
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Lost { counted: true, .. } => lost += 1,
                Cue::Collapse => collapse += 1,
                _ => {}
            }
        }
    }
    assert!(sim.over, "still going after a minute of misses");
    assert_eq!((lost, collapse, sim.lives, sim.score), (3, 1, 0, 0));
    assert!(sim.ending.unwrap() >= ENDING);
    assert!(!sim.drop_piece(), "no drops once it's over");
}

#[test]
fn a_tower_grows_and_the_camera_follows() {
    let mut sim = Sim::new(3);
    let mut bot = Bot::new(GOOD, 1);
    sim.drop_piece();
    while sim.score < 10 && sim.t < 90.0 && !sim.over {
        if bot.think(&sim, DT) {
            sim.drop_piece();
        }
        sim.step(DT);
        sim.cues.clear();
    }
    assert!(sim.score >= 10, "score {} after {:.0}s", sim.score, sim.t);
    assert!(sim.height > 200.0, "height {}", sim.height);
    steps(&mut sim, 120);
    let on_screen = PLATE_TOP - sim.height - sim.cam_y;
    assert!((on_screen - TOWER_TOP_ON_SCREEN).abs() < 60.0, "tower top at screen y {on_screen}");
    let bird = sim.bird().y - sim.cam_y;
    assert!(bird > 150.0 && bird < 260.0, "bird at screen y {bird}");
}

#[test]
fn new_snacks_arrive_in_order() {
    let mut sim = Sim::new(5);
    let mut bot = Bot::new(GOOD, 2);
    let mut met = Vec::new();
    sim.drop_piece();
    while sim.t < 200.0 && !sim.over && met.len() < UNLOCKS.len() - 1 {
        if bot.think(&sim, DT) {
            sim.drop_piece();
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::NewKind(k) = c {
                met.push(k);
                assert_eq!(sim.held.unwrap().kind, k, "a new kind comes next");
            }
        }
    }
    let want: Vec<Kind> = UNLOCKS[1..].iter().map(|u| u.1).collect();
    assert_eq!(&met[..], &want[..met.len()]);
    assert!(met.len() >= 3, "met {met:?} by score {}", sim.score);
}

#[test]
fn twists_are_announced_then_push() {
    // Twists run on the clock: no drops needed (and no hearts to lose).
    let mut sim = Sim::new(9);
    let mut seen = Vec::new();
    let (mut wind, mut tilt) = (0.0f32, 0.0f32);
    while sim.t < 50.0 {
        sim.step(DT);
        wind = wind.max(sim.wind.abs());
        tilt = tilt.max(sim.plate_angle.abs());
        for c in sim.cues.drain(..) {
            match c {
                Cue::Warn(k, _) => seen.push(("warn", k)),
                Cue::Start(k) => seen.push(("start", k)),
                _ => {}
            }
        }
    }
    assert_eq!(&seen[..4], &[("warn", TwistKind::Gust), ("start", TwistKind::Gust), ("warn", TwistKind::Tilt), ("start", TwistKind::Tilt)]);
    assert!(wind > 50.0 && tilt > 0.05, "wind {wind} tilt {tilt}");
}

#[test]
fn faces_get_scared_when_things_go_wrong() {
    let mut sim = Sim::new(2);
    sim.drop_piece();
    let mut fear = 0.0f32;
    for _ in 0..40 {
        sim.step(DT);
        fear = fear.max(sim.pieces[0].fear);
    }
    assert!(fear > 0.4, "a falling snack is scared: {fear}");
    steps(&mut sim, 120);
    assert!(sim.pieces[0].fear < 0.2, "and calms down on the tower: {}", sim.pieces[0].fear);
}

#[test]
fn fallen_snacks_leave_the_world() {
    let mut sim = Sim::new(4);
    for _ in 0..2 {
        while sim.held.is_none() {
            sim.step(DT);
        }
        sim.drop_piece();
        let id = sim.pieces.last().unwrap().id;
        sim.world.add_velocity(id, vec2(-700.0, 0.0));
        steps(&mut sim, 240);
    }
    assert!(sim.pieces.iter().all(|p| p.state != State::Landed));
    assert!(sim.pieces.is_empty(), "{} left", sim.pieces.len());
    assert_eq!(sim.world.len(), 2, "only the plate and the ground");
}

#[test]
fn the_same_taps_replay_the_same_round() {
    let run = || {
        let mut sim = Sim::new(11);
        let mut bot = Bot::new(DECENT, 5);
        sim.drop_piece();
        let mut trace = Vec::new();
        for i in 0..2400 {
            if bot.think(&sim, DT) {
                sim.drop_piece();
            }
            // Uneven frames, as on a phone.
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            trace.push((sim.world.state_hash(), sim.score, sim.lives, sim.cues.len()));
            sim.cues.clear();
        }
        trace
    };
    assert_eq!(run(), run());
}

/// The platform's rules for a round (SKILL.md "Platform facts that bite"),
/// on a fake host: a tap starts it, the score goes in before Over, a
/// frantic tap doesn't restart, the best is kept, the session replays.
/// Nobody tapping never loses a heart, so the round is lost by tapping
/// every 0.8 s whatever the bird is doing.
#[test]
fn keeps_the_platform_rules() {
    maimbrain::sys::test::Conformance::new().tap_every(0.8).check::<crate::Picnic>();
}

/// A first-timer's round lasts 20–45 s (engagement.md). A handful of seeds
/// here (the full report is `difficulty`, below).
#[test]
fn a_first_timer_lasts_long_enough() {
    let n = 5;
    let mut secs: Vec<f32> = (0..n).map(|s| play_round(FIRST_TIMER, 500 + s, 70.0).seconds).collect();
    secs.sort_by(f32::total_cmp);
    let median = secs[n as usize / 2];
    assert!((18.0..55.0).contains(&median), "first-timer median {median:.1}s: {secs:?}");
}

/// Difficulty report (slow; run in release). Targets (DESIGN.md, the
/// skill): a first-timer's round lasts 20–45 s, a good player's 1–3 min.
#[test]
#[ignore]
fn difficulty() {
    // Every heart lost, per preset, for the breakdown under the table (the
    // report itself is per round), and the physics' busiest moments.
    let hearts: Mutex<BTreeMap<&str, Hearts>> = Mutex::default();
    let report = Difficulty::new(200)
        .seeds_from(1000)
        .preset("first-timer", FIRST_TIMER)
        .target(20.0, 45.0)
        .preset("decent", DECENT)
        .preset("good", GOOD)
        .target(60.0, 180.0)
        .run(|skill, seed| {
            let r = play_round(skill, seed, 600.0);
            let mut all = hearts.lock().unwrap();
            let h = all.entry(skill.name).or_default();
            h.lost.extend(&r.losses);
            h.max_awake = h.max_awake.max(r.max_awake);
            h.max_bodies = h.max_bodies.max(r.max_bodies);
            let o = Outcome::new(r.seconds).stat("snacks", r.score as f32).stat("tower m", r.height / 100.0);
            match r.losses.first() {
                Some(first) => o.stat("first heart s", first.2),
                None => o,
            }
        });
    report.print();
    // What changed since the last run (committed: the last tuning that was kept).
    report.compare_with("bot-report.tsv");
    // How the hearts go: which snacks fall, and how many during a twist.
    let hearts = hearts.into_inner().unwrap();
    for p in &report.presets {
        let (name, h) = (&p.name, &hearts[p.name.as_str()]);
        let pc = |n: usize| n * 100 / h.lost.len().max(1);
        let by_kind: Vec<String> = [Kind::Box, Kind::Cube, Kind::Plank, Kind::Wedge, Kind::Bun, Kind::Ball].iter().map(|&k| format!("{k:?} {}%", pc(h.lost.iter().filter(|l| l.0 == k).count()))).collect();
        let (twist, tumbles) = (pc(h.lost.iter().filter(|l| l.1).count()), pc(h.lost.iter().filter(|l| l.3).count()));
        println!("{name:12} hearts lost: {}; {tumbles}% tumbles (rest misses); {twist}% during a twist; awake max {}, bodies max {}", by_kind.join(" "), h.max_awake, h.max_bodies);
    }
    report.assert_skill_matters();
    // One traced round, because medians swing.
    let mut sim = Sim::new(1234);
    let mut bot = Bot::new(DECENT, 77);
    sim.drop_piece();
    while !sim.over && sim.t < 300.0 {
        if bot.think(&sim, DT) {
            sim.drop_piece();
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Land { streak, neat, .. } => print!("[{:.1}s +1 #{} x{streak}{}] ", sim.t, sim.score, if neat { " neat" } else { "" }),
                Cue::Lost { kind, counted: true, .. } => print!("[{:.1}s LOST {kind:?} ♥{}] ", sim.t, sim.lives),
                Cue::NewKind(k) => print!("[{:.1}s new {k:?}] ", sim.t),
                Cue::Start(k) => print!("[{:.1}s {k:?}] ", sim.t),
                Cue::Over => print!("[{:.1}s over, {} snacks]", sim.t, sim.score),
                _ => {}
            }
        }
    }
    println!();
}

// ---- Mix ------------------------------------------------------------------------

/// A bot-played round through the real sound module, as an event log for
/// the skill's scripts/mix_check.py:
/// `MB_MIX_LOG=/tmp/st.log cargo test -p kit_stacker mix_log` then
/// `python3 .claude/skills/maimbrain-game/scripts/mix_check.py <kit>/assets /tmp/st.log`.
pub fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut snd = crate::sound::Sound::new();
    snd.log = Some(String::new());
    snd.start();
    sim.drop_piece();
    let mut tail = 3.0;
    while tail > 0.0 && sim.t < max_seconds {
        if !sim.over && bot.think(&sim, DT) {
            sim.drop_piece();
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        snd.update(DT, &cues, true, &sim);
        if sim.over {
            tail -= DT;
        }
    }
    snd.log.take().unwrap()
}

#[test]
fn mix_log() {
    let log = mix_round(DECENT, 3, 75.0);
    assert!(log.lines().count() > 50);
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}
