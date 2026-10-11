//! Rule tests, and a bot that drives like a person, to tune difficulty.
//! `cargo test -p kit_racer --profile bot -- --ignored --nocapture difficulty`
//! prints round lengths and causes of death per skill preset
//! (`maimbrain::bot::Difficulty`).


use maimbrain::Rng;
use maimbrain::bot::{Difficulty, Outcome};

use crate::sim::{self, Cue, Kind, LANES, Phase, Sim};

const DT: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Seconds from seeing a threat to the first tap.
    pub react: (f32, f32),
    /// How far ahead they look, seconds (people scan the road; beginners stare at the car).
    pub horizon: f32,
    /// Reads truck blinkers before the truck moves.
    pub anticipate: bool,
    /// Chance a tap goes the wrong way (fat thumb, left/right mix-up).
    pub wrong: f32,
    /// Chance of an extra tap on a one-lane move (overshoot).
    pub overshoot: f32,
    /// Seconds between taps of a two-lane move.
    pub tap_gap: (f32, f32),
}

pub const FIRST_TIMER: Skill =
    Skill { name: "first-timer", react: (0.24, 0.42), horizon: 1.25, anticipate: false, wrong: 0.07, overshoot: 0.08, tap_gap: (0.2, 0.32) };
pub const DECENT: Skill = Skill { name: "decent", react: (0.2, 0.32), horizon: 1.6, anticipate: false, wrong: 0.035, overshoot: 0.04, tap_gap: (0.16, 0.24) };
pub const GOOD: Skill = Skill { name: "good", react: (0.15, 0.24), horizon: 2.1, anticipate: true, wrong: 0.012, overshoot: 0.015, tap_gap: (0.12, 0.18) };

pub struct Bot {
    skill: Skill,
    rng: Rng,
    /// Taps on their way: (seconds until it lands, direction).
    queue: Vec<(f32, i32)>,
    /// The lane it's committed to, until it gets there or rethinks.
    goal: Option<i32>,
    /// Seconds since the last decision (a person doesn't re-plan every frame).
    since: f32,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed), queue: Vec::new(), goal: None, since: 1.0 }
    }

    /// Taps due this frame (directions).
    pub fn think(&mut self, sim: &Sim, dt: f32) -> Vec<i32> {
        let mut taps = Vec::new();
        self.queue.retain_mut(|q| {
            q.0 -= dt;
            if q.0 <= 0.0 {
                taps.push(q.1);
            }
            q.0 > 0.0
        });
        self.since += dt;
        // Don't re-plan in the frame a tap lands (the lane hasn't changed yet).
        if !taps.is_empty() || !self.queue.is_empty() || self.since < 0.08 {
            return taps;
        }
        if self.goal == Some(sim.lane) {
            self.goal = None;
        }
        let want = sim.plan(self.skill.horizon, self.skill.anticipate);
        if want == sim.lane || self.goal == Some(want) {
            return taps;
        }
        self.since = 0.0;
        self.goal = Some(want);
        let (lo, hi) = self.skill.react;
        let mut at = self.rng.range(lo, hi);
        let steps = (want - sim.lane).abs();
        let dir = (want - sim.lane).signum();
        for k in 0..steps {
            let d = if self.rng.f32() < self.skill.wrong { -dir } else { dir };
            self.queue.push((at, d));
            if k + 1 < steps {
                at += self.rng.range(self.skill.tap_gap.0, self.skill.tap_gap.1);
            }
        }
        if steps == 1 && self.rng.f32() < self.skill.overshoot {
            at += self.rng.range(self.skill.tap_gap.0, self.skill.tap_gap.1);
            self.queue.push((at, dir));
        }
        taps
    }
}

pub struct RoundStats {
    pub seconds: f32,
    pub score: u32,
    /// What hit you.
    pub cause: Option<Kind>,
    pub near_misses: u32,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> RoundStats {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    sim.press(if seed.is_multiple_of(2) { -1 } else { 1 }); // the tap that starts the round
    let mut cause = None;
    let mut near = 0;
    while !sim.crashed() && sim.t < max_seconds {
        for dir in bot.think(&sim, DT) {
            sim.press(dir);
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Crash { .. } => {
                    cause = sim.obstacles.iter().find(|o| o.knock.is_some_and(|k| k.t < 0.1)).map(|o| o.kind);
                }
                Cue::NearMiss { .. } => near += 1,
                _ => {}
            }
        }
    }
    RoundStats { seconds: sim.t, score: sim.score(), cause, near_misses: near }
}

fn run_to_over(sim: &mut Sim) {
    for _ in 0..(60 * 10) {
        sim.step(DT);
        if sim.over {
            return;
        }
    }
}

#[test]
fn the_first_seconds_cannot_fail() {
    for seed in 0..40 {
        let mut sim = Sim::new(seed);
        // A player who does nothing at all, and one who taps madly.
        let mut busy = Sim::new(seed);
        for i in 0..(60.0 * sim::SAFE_START) as u32 {
            sim.step(DT);
            if i % 9 == 0 {
                busy.press(if (i / 9) % 3 == 0 { 1 } else { -1 });
            }
            busy.step(DT);
        }
        assert!(!sim.crashed(), "seed {seed}: crashed idle");
        assert!(!busy.crashed(), "seed {seed}: crashed tapping");
        assert!(busy.coins > 0, "seed {seed}: an early win (coins) in the warm-up");
    }
}

#[test]
fn taps_and_swipes_change_lanes() {
    let mut sim = Sim::new(1);
    sim.press(-1);
    assert_eq!(sim.lane, 0);
    sim.press(-1);
    assert_eq!(sim.lane, 0, "the edge stops you");
    assert!(sim.cues.iter().any(|c| matches!(c, Cue::Bump { dir: -1 })));
    for _ in 0..30 {
        sim.step(DT);
    }
    assert!((sim.x - sim::lane_x(0)).abs() < 0.2, "springs into the lane: x {}", sim.x);
    // A swipe right that began on the left half still goes right, one lane from where the touch began.
    sim.press(-1);
    sim.swipe(1);
    assert_eq!(sim.lane, 1);
    sim.press(1);
    sim.swipe(-1);
    assert_eq!(sim.lane, 0);
}

#[test]
fn the_lane_change_is_springy_and_quick() {
    let mut sim = Sim::new(2);
    sim.press(1);
    let target = sim::lane_x(2);
    let mut over = 0.0f32;
    let mut reached = None;
    for i in 0..60 {
        sim.step(DT);
        over = over.max(sim.x - target);
        if reached.is_none() && (sim.x - target).abs() < 0.3 {
            reached = Some(i as f32 * DT);
        }
    }
    let reached = reached.expect("reaches the lane");
    assert!(reached < 0.3, "takes {reached}s");
    assert!(over > 0.03 && over < 0.5, "overshoots a little: {over}");
}

#[test]
fn hitting_traffic_crashes_then_ends_the_round() {
    let mut sim = Sim::new(3);
    // Sit in the middle lane until something hits.
    while !sim.crashed() && sim.t < 120.0 {
        sim.step(DT);
        sim.cues.clear();
        sim.lane = 1;
    }
    assert!(sim.crashed(), "a car parked in the middle lane gets hit");
    assert!(!sim.over, "the crash plays out first, still in play");
    let score = sim.score();
    run_to_over(&mut sim);
    assert!(sim.over);
    assert!(sim.cues.contains(&Cue::Over));
    assert!(sim.score() >= score, "the score doesn't drop during the crash");
    if let Phase::Over { .. } = sim.phase {
    } else {
        panic!("phase {:?}", sim.phase)
    }
    let p = sim.pose();
    assert!(p.roll.abs() > 2.5, "it ends on its roof: roll {}", p.roll);
}

#[test]
fn scoring_counts_distance_coins_and_near_misses() {
    let mut sim = Sim::new(4);
    for _ in 0..120 {
        sim.step(DT);
    }
    assert!(sim.coins > 0);
    assert_eq!(sim.score(), sim.dist as u32 + sim.coins * sim::COIN_POINTS + sim.bonus);
    // A good driver gets near misses.
    let r = play_round(GOOD, 7, 120.0);
    assert!(r.near_misses > 0, "no near misses in {:.0}s", r.seconds);
    assert!(r.score > 1000);
}

#[test]
fn boost_smashes_through_traffic() {
    let mut sim = Sim::new(5);
    let mut smashed = false;
    while sim.t < 60.0 && !sim.crashed() {
        sim.boost = 1.0;
        sim.step(DT);
        smashed |= sim.cues.iter().any(|c| matches!(c, Cue::Smash { .. }));
        sim.cues.clear();
        sim.lane = 1;
    }
    assert!(!sim.crashed(), "boosting never crashes");
    assert!(smashed);
}

#[test]
fn every_row_leaves_a_lane_open() {
    for seed in 0..30 {
        let mut sim = Sim::new(seed);
        let mut bot = Bot::new(GOOD, seed);
        while sim.t < 150.0 && !sim.crashed() {
            for d in bot.think(&sim, DT) {
                sim.press(d);
            }
            sim.step(DT);
            sim.cues.clear();
            // Nothing deadly covers all three lanes at once near the car (oil only slides you).
            let deadly = |l: i32| {
                sim.obstacles.iter().any(|o| {
                    let rel = o.d - sim.dist;
                    o.kind != Kind::Oil && o.knock.is_none() && o.covers(l) && rel + o.kind.half().1 > -sim::CAR_HALF.1 && rel - o.kind.half().1 < sim::CAR_HALF.1 + 3.0
                })
            };
            let blocked = (0..LANES).filter(|&l| deadly(l)).count();
            assert!(blocked < 3, "seed {seed} t {:.1}: all lanes blocked", sim.t);
        }
    }
}

#[test]
fn hazards_arrive_on_schedule() {
    let mut sim = Sim::new(9);
    let mut seen = Vec::new();
    let mut bot = Bot::new(GOOD, 1);
    while sim.t < 75.0 && !sim.crashed() {
        for d in bot.think(&sim, DT) {
            sim.press(d);
        }
        sim.boost = 1.0; // can't die: we want to see everything
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::New(k) = c {
                seen.push((k, sim.t));
            }
        }
    }
    let kinds: Vec<Kind> = seen.iter().map(|s| s.0).collect();
    assert!(kinds.contains(&Kind::Truck) && kinds.contains(&Kind::Cones) && kinds.contains(&Kind::Oil), "{seen:?}");
    assert!(sim.tier >= 5, "sped up every {}s: tier {}", sim::SPEED_EVERY, sim.tier);
}

#[test]
fn same_seed_same_inputs_same_round() {
    let run = |seed| {
        let mut sim = Sim::new(seed);
        let mut bot = Bot::new(DECENT, 11);
        sim.press(1);
        let mut trace = Vec::new();
        for i in 0..3600 {
            for d in bot.think(&sim, DT) {
                sim.press(d);
            }
            // Uneven frames, as on a phone.
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            sim.cues.clear();
            trace.push((sim.dist.to_bits(), sim.x.to_bits(), sim.score(), sim.obstacles.len()));
        }
        trace
    };
    assert_eq!(run(21), run(21));
    assert_ne!(run(21), run(22));
}

#[test]
fn the_car_is_framed_on_the_title_and_in_play() {
    use maimbrain::gfx3d::vec3;
    let sim = Sim::new(1);
    for (title, lo, hi) in [(1.0, (90.0, 300.0), (330.0, 560.0)), (0.0, (110.0, 250.0), (420.0, 600.0))] {
        let cam = sim.camera(title, 0.0);
        let p = cam.project(vec3(0.0, 0.6, 0.0), sim::W, sim::H);
        assert!(p.on_screen && (lo.0..lo.1).contains(&p.x) && (hi.0..hi.1).contains(&p.y), "title {title}: car at {:.0}, {:.0}", p.x, p.y);
    }
}

#[test]
fn the_attract_mode_drives_well() {
    let mut crashes = 0;
    let mut near = 0;
    for seed in 0..10 {
        let mut sim = Sim::new(seed);
        while sim.t < 60.0 && !sim.crashed() {
            sim.autopilot();
            sim.step(DT);
            near += sim.cues.iter().filter(|c| matches!(c, Cue::NearMiss { .. })).count();
            sim.cues.clear();
        }
        crashes += sim.crashed() as u32;
    }
    assert!(crashes <= 3, "the title card crashes too often: {crashes}/10");
    assert!(near >= 10, "the title card shows near misses: {near}");
}

/// The fast check: a first-timer's median round is in the target range.
#[test]
fn first_timer_median_in_range() {
    let mut secs: Vec<f32> = (0..40).map(|s| play_round(FIRST_TIMER, 1000 + s, 150.0).seconds).collect();
    secs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = secs[secs.len() / 2];
    assert!((18.0..50.0).contains(&median), "first-timer median {median:.1}s");
}

/// Difficulty report (run in release). Targets (engagement.md): a first-timer's
/// round lasts 20–45 s, a good player's 1–3 minutes.
#[test]
#[ignore]
fn difficulty() {
    let report = Difficulty::new(200)
        .seeds_from(1000)
        .preset(FIRST_TIMER.name, FIRST_TIMER)
        .target(20.0, 45.0)
        .preset(DECENT.name, DECENT)
        .preset(GOOD.name, GOOD)
        .target(60.0, 180.0)
        .run(|skill, seed| {
            let r = play_round(skill, seed, 600.0);
            let cause = match r.cause {
                Some(Kind::Car) => "car",
                Some(Kind::Truck) => "truck",
                Some(Kind::Cones) => "cones",
                Some(Kind::Oil) => "oil→",
                None => "none",
            };
            Outcome::new(r.seconds).cause(cause).stat("score", r.score as f32).stat("near misses/min", r.near_misses as f32 / r.seconds.max(1.0) * 60.0).flag("over in under 10 s", r.seconds < 10.0)
        });
    report.print();
    // What changed since the last run (committed: the last tuning that was kept).
    report.compare_with("bot-report.tsv");
    report.assert_skill_matters();
    // One traced run, because medians swing on small changes.
    let mut sim = Sim::new(1234);
    let mut bot = Bot::new(DECENT, 99);
    sim.press(1);
    let mut next = 0.0;
    while !sim.crashed() && sim.t < 300.0 {
        for d in bot.think(&sim, DT) {
            sim.press(d);
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::SpeedUp { .. } | Cue::New(_) | Cue::Crash { .. } | Cue::Boost { .. } => println!("  {:5.1}s {:?}", sim.t, c),
                _ => {}
            }
        }
        if sim.t >= next {
            next += 10.0;
            println!("  {:5.1}s speed {:4.1} m/s  score {:5}  obstacles {}", sim.t, sim.speed, sim.score(), sim.obstacles.len());
        }
    }
}

/// The platform's rules for a round (SKILL.md "Platform facts that bite"),
/// on a fake host: a tap starts it, the score goes in before Over, a
/// frantic tap doesn't restart, the best is kept, the session replays.
/// Nobody steers, so the car runs into the first car in its lane.
#[test]
fn keeps_the_platform_rules() {
    maimbrain::sys::test::Conformance::new().check::<crate::Racer>();
}

/// A bot-played round through the real sound module (the same cue and
/// update calls the game makes), as an event log for the skill's
/// `scripts/mix_check.py`:
/// `MB_MIX_LOG=/tmp/racer.log cargo test -p kit_racer mix_log`, then
/// `python3 .claude/skills/maimbrain-game/scripts/mix_check.py <kit>/assets /tmp/racer.log`.
/// `MB_MIX_SEED` picks another round.
pub fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut audio = crate::sound::Audio::recording();
    audio.start();
    sim.press(1);
    let mut tail = 3.0;
    while tail > 0.0 && sim.t < max_seconds {
        if !sim.crashed() {
            for dir in bot.think(&sim, DT) {
                sim.press(dir);
            }
        }
        sim.step(DT);
        for c in std::mem::take(&mut sim.cues) {
            audio.cue(&c, true);
        }
        audio.update(&sim, !sim.over, DT);
        if sim.over {
            tail -= DT;
        }
    }
    audio.log.take().unwrap()
}

#[test]
fn mix_log() {
    let seed = std::env::var("MB_MIX_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let log = mix_round(DECENT, seed, 120.0);
    assert!(log.lines().any(|l| l.contains(" coin ")) && log.lines().any(|l| l.contains(" pass ")));
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}
