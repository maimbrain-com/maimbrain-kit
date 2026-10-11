//! Rule tests, and a bot that plays the sim like a person, to tune difficulty.
//!
//!     cargo test -p kit_runner
//!     cargo test -p kit_runner --profile bot -- --ignored --nocapture difficulty
//!
//! The second prints each skill preset's round lengths, and how and where it dies.

use maimbrain::Rng;

use crate::sim::*;

const DT: f32 = 1.0 / 60.0;

// ---- The bot -------------------------------------------------------------

/// How a kind of player plays.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Reaction delay range (s): nothing happens sooner than this after noticing.
    pub delay: (f32, f32),
    /// Average lateness of a press against the ideal moment (s; negative = early).
    pub bias: f32,
    /// Timing noise of a press (s, roughly one standard deviation).
    pub timing: f32,
    /// Relative error in how long they hold (0.3 = ±30%).
    pub hold: f32,
    /// Chance of jumping at something they should run under (a gull).
    pub panic: f32,
    /// Chance of saving a bad jump with the double jump.
    pub rescue: f32,
    /// How far ahead they look (px from the hero; the screen shows 250).
    pub view: f32,
}

pub const FIRST_TIMER: Skill = Skill { name: "first-timer", delay: (0.2, 0.35), bias: 0.03, timing: 0.11, hold: 0.35, panic: 0.35, rescue: 0.25, view: 170.0 };
pub const DECENT: Skill = Skill { name: "decent", delay: (0.17, 0.28), bias: 0.02, timing: 0.08, hold: 0.2, panic: 0.12, rescue: 0.55, view: 220.0 };
pub const GOOD: Skill = Skill { name: "good", delay: (0.14, 0.22), bias: 0.01, timing: 0.055, hold: 0.1, panic: 0.03, rescue: 0.7, view: 250.0 };

pub struct Bot {
    skill: Skill,
    rng: Rng,
    noticed: u32,
    press: Option<(f32, f32)>,
    release_at: Option<f32>,
    /// After a jump, a look at whether it's going to work (time, planned release).
    check: Option<(f32, f32)>,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed), noticed: 0, press: None, release_at: None, check: None }
    }

    fn gauss(&mut self) -> f32 {
        (0..4).map(|_| self.rng.f32()).sum::<f32>() - 2.0
    }

    fn delay(&mut self) -> f32 {
        self.rng.range(self.skill.delay.0, self.skill.delay.1)
    }

    /// Would the current jump end badly if they let go at `release`?
    /// (Only this jump: up to the landing.)
    fn doomed(sim: &Sim, release: f32) -> bool {
        let mut s = sim.clone();
        for _ in 0..90 {
            if s.t >= release {
                s.release();
            }
            s.step(DT);
            if !s.alive() {
                return true;
            }
            if s.hero.grounded {
                return false;
            }
        }
        false
    }

    /// One frame of thinking: (press, release) this frame.
    pub fn think(&mut self, sim: &Sim) -> (bool, bool) {
        let t = sim.t;
        let (mut press, mut release) = (false, false);
        if self.release_at.is_some_and(|at| t >= at) {
            release = true;
            self.release_at = None;
        }
        if let Some((at, hold)) = self.press
            && t >= at
        {
            press = true;
            self.press = None;
            self.release_at = Some(t + hold);
            let d = self.delay();
            self.check = Some((t + d, t + hold));
        }
        if let Some((at, rel)) = self.check
            && t >= at
        {
            self.check = None;
            let roll = self.rng.f32();
            if !sim.hero.grounded && sim.hero.jumps == 1 && roll < self.skill.rescue && Bot::doomed(sim, rel) {
                press = true;
                self.release_at = Some(t + MAX_HOLD);
            }
        }
        if self.press.is_some() {
            return (press, release);
        }
        let Some(o) = sim.obstacles.iter().find(|o| !o.passed && o.id > self.noticed) else { return (press, release) };
        let ahead = o.left(t) - sim.dist;
        let soon = sim.time_to(o);
        if ahead > self.skill.view && soon > 0.9 {
            return (press, release);
        }
        if !sim.hero.grounded && soon > 0.3 {
            return (press, release); // plan once back on the ground
        }
        self.noticed = o.id;
        let delay = self.delay();
        match sim.plan_for(o) {
            Some(Plan { press: Some(tau), hold, .. }) => {
                let late = self.skill.bias + self.gauss() * self.skill.timing;
                let hold = (hold * (1.0 + self.gauss() * self.skill.hold)).clamp(MIN_HOLD * 0.5, MAX_HOLD);
                self.press = Some((t + (tau + late).max(delay), hold));
            }
            Some(Plan { press: None, .. }) => {
                // It looks like something to jump: some players do.
                if self.rng.f32() < self.skill.panic {
                    self.press = Some((t + (soon - 0.25).max(delay), MIN_HOLD));
                }
            }
            None => {}
        }
        (press, release)
    }
}

pub struct Round {
    pub seconds: f32,
    pub score: u32,
    pub meters: u32,
    pub cause: Option<Cause>,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> Round {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    sim.press(); // the tap that starts the round is also the first jump
    sim.release();
    while sim.alive() && sim.t < max_seconds {
        let (p, r) = bot.think(&sim);
        if r {
            sim.release();
        }
        if p {
            sim.press();
        }
        sim.step(DT);
        sim.cues.clear();
    }
    Round { seconds: sim.t, score: sim.score(), meters: sim.meters(), cause: sim.dying.as_ref().map(|d| d.cause) }
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

// ---- Rule tests ----------------------------------------------------------

#[test]
fn the_first_seconds_cannot_be_failed() {
    for seed in 0..12 {
        let mut sim = Sim::new(seed);
        let mut rng = Rng::new(seed + 99);
        // Mash, hold, let go: whatever a nervous thumb does.
        for _ in 0..(3.0 / DT) as usize {
            match rng.next_u32() % 6 {
                0 => sim.press(),
                1 => sim.release(),
                _ => {}
            }
            sim.step(DT);
        }
        assert!(sim.alive(), "seed {seed} died in the warm-up");
    }
}

#[test]
fn the_warm_up_coins_are_an_early_win() {
    let mut sim = Sim::new(3);
    let mut first = None;
    while sim.t < 3.0 {
        sim.step(DT);
        if first.is_none() && sim.cues.iter().any(|c| matches!(c, Cue::Coin { .. })) {
            first = Some(sim.t);
        }
        sim.cues.clear();
    }
    assert!(first.unwrap() < 1.0, "first coin at {first:?}");
    assert_eq!(sim.coins_got, 9);
    assert_eq!(sim.score(), sim.meters() + 9 * COIN_POINTS);
}

#[test]
fn standing_still_ends_at_the_first_crate_then_the_tumble_plays() {
    let mut sim = Sim::new(5);
    let mut hit_at = None;
    let mut overs = 0;
    while !sim.over && sim.t < 20.0 {
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Hit { kind, .. } => {
                    assert_eq!(kind, Kind::Crate);
                    hit_at = Some(sim.t);
                }
                Cue::Over => overs += 1,
                _ => {}
            }
        }
    }
    let hit = hit_at.expect("never hit anything");
    assert!((3.0..6.0).contains(&hit), "hit at {hit}");
    assert!(sim.over && overs == 1);
    assert!((sim.t - hit - DEATH_TIME).abs() < 0.05, "the tumble lasts DEATH_TIME");
    // The score froze at the hit.
    let s = sim.score();
    sim.step(1.0);
    assert_eq!(sim.score(), s);
}

#[test]
fn a_tap_clears_a_crate_and_holding_jumps_higher() {
    let tap = jump_curve(MIN_HOLD);
    let full = jump_curve(MAX_HOLD);
    let peak = |c: &[f32]| c.iter().cloned().fold(0.0, f32::max);
    assert!(peak(&tap) > 60.0 && peak(&tap) < 110.0, "tap peak {}", peak(&tap));
    assert!(peak(&full) > 150.0, "full peak {}", peak(&full));
    assert!(full.len() > tap.len() + 20);
    // The first crate: a tap, with a comfortable window.
    let sim = Sim::new(1);
    let o = sim.obstacles.iter().find(|o| o.kind == Kind::Crate).unwrap();
    let p = plan(o, o.t0 - 1.2, o.x - START_SPEED * 1.2, START_SPEED).unwrap();
    assert_eq!(p.hold, MIN_HOLD);
    assert!(p.window >= 0.15, "window {}", p.window);
}

#[test]
fn double_jump_once_then_a_press_waits_for_the_ground() {
    let mut sim = Sim::new(2);
    sim.press();
    sim.release();
    sim.step(0.15);
    sim.press();
    sim.release();
    assert_eq!((sim.jumps, sim.double_jumps), (1, 1));
    sim.step(0.1);
    sim.press(); // a third press in the air is buffered
    sim.release();
    assert_eq!(sim.double_jumps, 1);
    let mut landed = 0;
    while sim.t < 1.5 {
        sim.step(DT);
        landed += sim.cues.iter().filter(|c| matches!(c, Cue::Land { .. })).count();
        sim.cues.clear();
    }
    assert!(landed >= 1);
}

#[test]
fn the_same_seed_and_taps_replay_the_same_run() {
    let run = || {
        let mut sim = Sim::new(11);
        let mut bot = Bot::new(DECENT, 5);
        sim.press();
        let mut trace = Vec::new();
        for i in 0..3600 {
            let (p, r) = bot.think(&sim);
            if r {
                sim.release();
            }
            if p {
                sim.press();
            }
            // Uneven frames, as on a phone.
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            sim.cues.clear();
            trace.push((sim.dist.to_bits(), sim.hero.h.to_bits(), sim.score(), sim.obstacles.len()));
        }
        trace
    };
    assert_eq!(run(), run());
}

/// Fair by construction: a player who presses at the right moment (inside
/// the window the generator guarantees) survives every course.
#[test]
fn every_course_can_be_run_by_a_careful_player() {
    for seed in 0..16 {
        let mut sim = Sim::new(seed * 7 + 1);
        let mut pilot = Autopilot::new(seed, 0.03);
        while sim.alive() && sim.t < 150.0 {
            pilot.drive(&mut sim);
            sim.step(DT);
            sim.cues.clear();
        }
        assert!(sim.alive(), "seed {seed}: the autopilot died at {:.1}s ({:?})", sim.t, sim.dying.as_ref().map(|d| d.cause));
    }
}

#[test]
fn never_two_nasty_ones_in_a_row_and_something_new_every_15s() {
    let mut sim = Sim::new(42);
    let mut pilot = Autopilot::new(1, 0.0);
    let mut seen = Vec::new();
    let mut news = Vec::new();
    while sim.alive() && sim.t < 120.0 {
        pilot.drive(&mut sim);
        sim.step(DT);
        for o in &sim.obstacles {
            if seen.last().is_none_or(|&(id, _)| o.id > id) {
                seen.push((o.id, o.nasty));
            }
        }
        for c in sim.cues.drain(..) {
            if let Cue::NewKind(k) = c {
                news.push((sim.t, k));
            }
        }
    }
    assert!(seen.windows(2).all(|w| !(w[0].1 && w[1].1)), "two hard obstacles in a row");
    assert!(news.len() >= 5, "new families: {news:?}");
    for w in news.windows(2) {
        assert!(w[1].0 - w[0].0 < 25.0, "a long wait for something new: {news:?}");
    }
}

#[test]
fn first_timers_last_20_to_45_seconds() {
    let mut secs: Vec<f32> = (0..24).map(|s| play_round(FIRST_TIMER, 500 + s, 240.0).seconds).collect();
    let m = median(&mut secs);
    assert!((18.0..50.0).contains(&m), "first-timer median {m:.1}s");
}

/// Difficulty report (slow; run in release). Targets from the skill: a
/// first-timer's round lasts 20–45 s, a good player's 1–3 minutes.
#[test]
#[ignore]
fn difficulty() {
    let n = 200;
    for skill in [FIRST_TIMER, DECENT, GOOD] {
        let mut secs = Vec::new();
        let mut scores = Vec::new();
        let mut causes = [0u32; 8];
        let mut where_: Vec<f32> = Vec::new();
        for seed in 0..n {
            let r = play_round(skill, 1000 + seed, 600.0);
            secs.push(r.seconds);
            scores.push(r.score as f32);
            where_.push(r.meters as f32);
            match r.cause {
                Some(Cause::Bonk(k)) => causes[Kind::ORDER.iter().position(|o| *o == k).unwrap()] += 1,
                Some(Cause::Pit) => causes[7] += 1,
                None => {}
            }
        }
        let mut s2 = secs.clone();
        s2.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p = |q: f32| s2[((n as f32 - 1.0) * q) as usize];
        println!(
            "{:12} median {:5.1}s  p10 {:5.1}  p90 {:5.1}   score median {:5.0}   meters median {:5.0}",
            skill.name,
            median(&mut secs),
            p(0.1),
            p(0.9),
            median(&mut scores),
            median(&mut where_)
        );
        let names = ["crate", "pit(bonk)", "stack", "gull", "bee", "skimmer", "log", "fell in pit"];
        let deaths: Vec<String> = names.iter().zip(causes).filter(|(_, c)| *c > 0).map(|(n, c)| format!("{n} {c}")).collect();
        println!("{:12} deaths: {}", "", deaths.join(", "));
    }
    // One traced run, since medians swing on small changes.
    let mut sim = Sim::new(1234);
    let mut bot = Bot::new(FIRST_TIMER, 77);
    sim.press();
    sim.release();
    let mut last = -1.0;
    while sim.alive() && sim.t < 300.0 {
        let (p, r) = bot.think(&sim);
        if r {
            sim.release();
        }
        if p {
            sim.press();
        }
        sim.step(DT);
        let m = sim.meters();
        for c in std::mem::take(&mut sim.cues) {
            match c {
                Cue::NewKind(k) => println!("  {:5.1}s  new: {k:?}", sim.t),
                Cue::SpeedUp => println!("  {:5.1}s  faster: {:.0} px/s", sim.t, sim.speed),
                Cue::Close { .. } => println!("  {:5.1}s  close!", sim.t),
                Cue::Hit { kind, .. } => println!("  {:5.1}s  hit a {kind:?} at {m} m", sim.t),
                Cue::Fall => println!("  {:5.1}s  fell in a pit at {m} m", sim.t),
                _ => {}
            }
        }
        if sim.t - last > 10.0 {
            last = sim.t;
            println!("  {:5.1}s  {} m, score {}", sim.t, sim.meters(), sim.score());
        }
    }
}

/// What `sound.rs` plays over a bot-played round, as an event log for the
/// skill's `scripts/mix_check.py` (every play, voice stop and music level):
///
///     MB_MIX_LOG=/tmp/mix.log cargo test -p kit_runner --profile bot -- --ignored mix_log
///     python3 <skill>/scripts/mix_check.py assets /tmp/mix.log
///
/// `MB_MIX_SEED` and `MB_MIX_SKILL` (first-timer, decent, good) pick the round.
#[test]
#[ignore]
fn mix_log() {
    let seed = std::env::var("MB_MIX_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(11);
    let skill = match std::env::var("MB_MIX_SKILL").as_deref() {
        Ok("first-timer") => FIRST_TIMER,
        Ok("good") => GOOD,
        _ => DECENT,
    };
    let log = mix_round(skill, seed, 75.0);
    assert!(log.lines().any(|l| l.contains(" coin ")) && log.lines().any(|l| l.contains(" loop")));
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}

/// Plays a round like the host does (cues into `Sound`, its update each
/// frame with the music's intensity) and returns the sound log.
fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    use crate::sound::Sound;
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut sound = Sound::new();
    sound.start();
    sim.press();
    sim.release();
    let mut after = 0.0;
    while after < 2.5 && sim.t < max_seconds {
        if sim.over {
            if after == 0.0 {
                sound.over(false);
            }
            after += DT;
        } else {
            let (p, r) = bot.think(&sim);
            if r {
                sim.release();
            }
            if p {
                sim.press();
            }
            sim.step(DT);
        }
        for c in std::mem::take(&mut sim.cues) {
            sound.cue(&c, true);
        }
        // As lib.rs: the high stem rises with speed and a hard one coming.
        let fast = (sim.speed - START_SPEED) / (MAX_SPEED - START_SPEED);
        let threat = sim.next_obstacle().filter(|o| o.nasty && sim.time_to(o) < 1.2).is_some() as u32 as f32;
        let intensity = if sim.alive() && !sim.over { (0.15 + fast * 1.1 + 0.25 * threat).min(1.0) } else { 0.0 };
        sound.update(DT, intensity);
    }
    sound.log.join("\n") + "\n"
}
