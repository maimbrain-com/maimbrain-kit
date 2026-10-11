//! Rule tests and a bot that plays like a person, to tune difficulty.
//! `cargo test -p kit_tower_defense --profile bot -- --ignored --nocapture difficulty`
//! prints round lengths and how each skill preset dies.

use maimbrain::Rng;

use crate::autopilot::{Action, Planner, Style};
use crate::sim::*;

const DT: f32 = 1.0 / 60.0;

/// How a person plays a tower defense game on a phone.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Seconds between glances at the coins (a person doesn't watch the
    /// counter every frame; they're watching jellies).
    pub glance: (f32, f32),
    /// Reaction delay before the first tap of an action (s).
    pub react: (f32, f32),
    /// Time for the second tap (the picker button) and aiming it (s).
    pub ui: (f32, f32),
    /// Chance a tap misses the pad or button and has to be redone.
    pub miss: f32,
    /// Chance of building on a random empty pad instead of a good one.
    pub wrong_pad: f32,
    /// Chance of picking a random affordable tower kind.
    pub wrong_kind: f32,
    /// Seconds into the round before they think of upgrading (the game's
    /// upgrade hint shows after a few idle seconds; first-timers find it late).
    pub upgrade_after: f32,
    /// Calls waves early when the board is calm.
    pub call_early: bool,
    /// Chance per action of a distraction (watching the jellies) and how long.
    pub lapse: f32,
    pub lapse_len: (f32, f32),
}

pub const FIRST_TIMER: Skill = Skill {
    name: "first-timer",
    glance: (3.0, 8.0),
    react: (0.2, 0.35),
    ui: (0.6, 1.4),
    miss: 0.15,
    wrong_pad: 0.5,
    wrong_kind: 0.5,
    upgrade_after: 60.0,
    call_early: false,
    lapse: 0.35,
    lapse_len: (3.0, 8.0),
};
pub const DECENT: Skill = Skill {
    name: "decent",
    glance: (2.0, 5.0),
    react: (0.18, 0.3),
    ui: (0.4, 0.8),
    miss: 0.07,
    wrong_pad: 0.3,
    wrong_kind: 0.25,
    upgrade_after: 35.0,
    call_early: false,
    lapse: 0.15,
    lapse_len: (1.5, 4.0),
};
pub const GOOD: Skill = Skill {
    name: "good",
    glance: (0.3, 0.8),
    react: (0.15, 0.22),
    ui: (0.25, 0.45),
    miss: 0.03,
    wrong_pad: 0.05,
    wrong_kind: 0.03,
    upgrade_after: 0.0,
    call_early: true,
    lapse: 0.03,
    lapse_len: (0.5, 1.5),
};

pub struct Bot {
    skill: Skill,
    rng: Rng,
    planner: Option<Planner>,
    /// Until the next glance at the board.
    wait: f32,
    /// An action on its way: seconds until the taps land.
    pending: Option<(f32, Action)>,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed), planner: None, wait: 0.4, pending: None }
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        self.rng.range(r.0, r.1)
    }

    /// Advances the bot; it acts on the sim when its taps land.
    pub fn think(&mut self, sim: &mut Sim, dt: f32) {
        if self.planner.is_none() {
            self.planner = Some(Planner::new(sim));
        }
        if let Some((t, a)) = &mut self.pending {
            *t -= dt;
            if *t <= 0.0 {
                let a = *a;
                self.pending = None;
                a.apply(sim);
                self.wait = self.range(self.skill.glance) * 0.5;
            }
            return;
        }
        self.wait -= dt;
        if self.wait > 0.0 {
            return;
        }
        self.wait = self.range(self.skill.glance);
        let style = Style { max_towers: 99, pads_below: 0.0, upgrades: sim.t >= self.skill.upgrade_after, call_early: self.skill.call_early };
        let planner = self.planner.as_ref().unwrap();
        let Some(mut action) = planner.want(sim, &style) else { return };
        // The first tower follows the in-game hint (a pulsing pad, POP pulsing in the picker).
        if sim.built == 0 {
            action = Action::Build(HINT_PAD, TowerKind::Pop);
        } else if let Action::Build(p, k) = action {
            let mut pad = p;
            let mut kind = k;
            if self.rng.f32() < self.skill.wrong_pad {
                let empty: Vec<usize> = (0..PADS.len()).filter(|&q| sim.towers[q].is_none()).collect();
                pad = empty[(self.rng.f32() * empty.len() as f32) as usize % empty.len()];
            }
            if self.rng.f32() < self.skill.wrong_kind {
                kind = TowerKind::ALL[(self.rng.f32() * 3.0) as usize % 3];
            }
            action = Action::Build(pad, kind);
        }
        if action.cost(sim) > sim.coins {
            return; // saving up
        }
        let mut delay = self.range(self.skill.react) + self.range(self.skill.ui);
        while self.rng.f32() < self.skill.miss {
            delay += self.range(self.skill.ui);
        }
        if self.rng.f32() < self.skill.lapse {
            delay += self.range(self.skill.lapse_len);
        }
        if action == Action::CallWave {
            delay = self.range(self.skill.react);
        }
        self.pending = Some((delay, action));
    }
}

#[derive(Debug)]
pub struct RoundStats {
    pub seconds: f32,
    pub score: u32,
    pub wave: u32,
    /// The kind of the jelly that took the last heart.
    pub killer: Option<CreepKind>,
    pub towers: usize,
    pub upgrades: u32,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> RoundStats {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut killer = None;
    while !sim.over && sim.t < max_seconds {
        bot.think(&mut sim, DT);
        let before: Vec<(f32, CreepKind)> = sim.creeps.iter().map(|c| (c.d, c.kind)).collect();
        sim.step(DT);
        for c in &sim.cues {
            if let Cue::Bite { hearts: 0, .. } = c {
                killer = before.iter().max_by(|a, b| a.0.total_cmp(&b.0)).map(|b| b.1);
            }
        }
        sim.cues.clear();
    }
    RoundStats { seconds: sim.t, score: sim.score, wave: sim.wave, killer, towers: sim.towers.iter().flatten().count(), upgrades: sim.upgrades }
}

fn median(v: &mut [f32]) -> (f32, f32, f32) {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    (v[n / 2], v[n / 10], v[(n * 9 / 10).min(n - 1)])
}

#[test]
fn the_opening_cannot_fail() {
    // Doing nothing for the first seconds loses nothing: wave 1 waits.
    let mut sim = Sim::new(1);
    for _ in 0..(60 * 5) {
        sim.step(DT);
    }
    assert_eq!(sim.hearts, HEARTS);
    assert!(sim.creeps.iter().all(|c| c.d < sim.road_len() * 0.5));
    // Start coins buy one tower, not two.
    assert!(sim.coins >= TOWERS[0].cost[0] && sim.coins < 2 * TOWERS[0].cost[0]);
}

#[test]
fn wave_one_is_a_guaranteed_win_from_any_pad() {
    for pad in 0..PADS.len() {
        for seed in 0..4 {
            let mut sim = Sim::new(seed);
            assert!(sim.build(pad, TowerKind::Pop));
            // Wave 1 is the first four jellies (ids 1-4); they walk ahead of
            // everything else, so any bite before they're all gone is theirs.
            let n1 = sim.next.groups.iter().map(|g| g.1).sum::<u32>();
            let mut popped = 0;
            while !sim.over && sim.t < 120.0 && (sim.wave < 1 || sim.pending_spawns() > 0 && sim.wave == 1 || sim.creeps.iter().any(|c| c.id <= n1)) {
                sim.step(DT);
                popped += sim.cues.iter().filter(|c| matches!(c, Cue::Pop { .. })).count();
                sim.cues.clear();
            }
            assert_eq!(sim.hearts, HEARTS, "pad {pad} seed {seed}: wave 1 leaked");
            assert!(popped >= 4, "pad {pad}: popped {popped}");
        }
    }
}

#[test]
fn the_hint_pad_covers_the_most_road() {
    let sim = Sim::new(0);
    let p = Planner::new(&sim);
    let best = (0..PADS.len()).max_by(|&a, &b| p.coverage(a, TowerKind::Pop).total_cmp(&p.coverage(b, TowerKind::Pop))).unwrap();
    assert_eq!(best, HINT_PAD);
}

#[test]
fn pads_sit_beside_the_road_and_apart() {
    let sim = Sim::new(0);
    for (i, &(x, y)) in PADS.iter().enumerate() {
        assert!(PAD >= 44.0);
        assert!(x - PAD / 2.0 >= 0.0 && x + PAD / 2.0 <= 360.0, "pad {i} off screen");
        let mut d = 0.0;
        while d < sim.road_len() {
            let (rx, ry) = sim.point_at(d);
            let gap = (rx - x).abs().max((ry - y).abs());
            assert!(gap >= PAD / 2.0 + ROAD_W / 2.0 - 1.0, "pad {i} on the road at {d}");
            d += 2.0;
        }
        for (j, &(x2, y2)) in PADS.iter().enumerate().skip(i + 1) {
            assert!((x - x2).abs().max((y - y2).abs()) >= PAD + 8.0, "pads {i} and {j} overlap");
        }
        assert_eq!(Sim::pad_at(x, y), Some(i));
    }
}

#[test]
fn building_and_upgrading_cost_coins() {
    let mut sim = Sim::new(3);
    sim.coins = 1000;
    assert!(sim.build(0, TowerKind::Boom));
    assert!(!sim.build(0, TowerKind::Pop), "pad taken");
    assert_eq!(sim.coins, 1000 - TOWERS[2].cost[0]);
    assert!(sim.upgrade(0));
    assert!(sim.upgrade(0));
    assert!(!sim.upgrade(0), "maxed");
    assert_eq!(sim.towers[0].as_ref().unwrap().level, 2);
    sim.coins = 0;
    sim.cues.clear();
    assert!(!sim.build(1, TowerKind::Pop));
    assert_eq!(sim.cues, vec![Cue::Denied]);
}

#[test]
fn popping_scores_and_pays() {
    let mut sim = Sim::new(5);
    sim.build(HINT_PAD, TowerKind::Pop);
    let coins0 = sim.coins;
    while sim.score == 0 && sim.t < 30.0 {
        sim.step(DT);
    }
    assert_eq!(sim.score, 1);
    assert!(sim.coins > coins0);
}

#[test]
fn splitters_split_and_kings_bite_three() {
    let mut sim = Sim::new(2);
    sim.wave = 6;
    sim.next = WavePlan { n: 7, groups: vec![(CreepKind::Splitter, 1)], boss: false, new_kind: None, rush: false };
    sim.next_in = 0.0;
    sim.step(DT);
    sim.step(DT);
    assert_eq!(sim.creeps.len(), 1);
    sim.creeps[0].hp = 0.0;
    sim.step(DT);
    assert_eq!(sim.creeps.iter().filter(|c| c.kind == CreepKind::Mini).count(), SPLIT_INTO);
    let mut sim = Sim::new(2);
    sim.wave = 9;
    sim.next = WavePlan { n: 10, groups: vec![(CreepKind::King, 1)], boss: true, new_kind: None, rush: false };
    sim.next_in = 0.0;
    sim.step(DT);
    sim.next_in = 1e6; // only the king comes
    while sim.hearts == HEARTS && sim.t < 120.0 {
        sim.step(DT);
    }
    assert_eq!(sim.hearts, HEARTS - KING_BITE);
}

#[test]
fn losing_every_heart_plays_the_ending_then_ends() {
    let mut sim = Sim::new(4);
    let mut cues = Vec::new();
    while !sim.over && sim.t < 300.0 {
        sim.step(DT);
        cues.append(&mut sim.cues);
    }
    assert!(sim.over, "nobody defended; it should be over");
    assert_eq!(sim.hearts, 0);
    assert!(sim.ending.unwrap() >= ENDING);
    let bites = cues.iter().filter(|c| matches!(c, Cue::Bite { .. })).count();
    assert!(bites >= HEARTS as usize / KING_BITE as usize);
    assert_eq!(cues.iter().filter(|c| **c == Cue::Ending).count(), 1);
    assert_eq!(*cues.last().unwrap(), Cue::Over);
    // Nothing more happens once it's over.
    assert!(!sim.build(0, TowerKind::Pop));
}

#[test]
fn calling_a_wave_early_pays_a_bonus() {
    let mut sim = Sim::new(9);
    sim.build(HINT_PAD, TowerKind::Pop);
    while sim.wave < 1 {
        sim.step(DT);
    }
    for _ in 0..60 {
        sim.step(DT);
    }
    let coins = sim.coins;
    let bonus = sim.call_wave().unwrap();
    assert!(bonus > 10, "bonus {bonus}");
    assert_eq!(sim.wave, 2);
    assert!(sim.coins >= coins + bonus);
}

#[test]
fn new_jellies_arrive_every_couple_of_waves() {
    let mut rng = Rng::new(1);
    let firsts: Vec<u32> = (1..=12).filter(|&n| plan_wave(n, &mut rng).new_kind.is_some()).collect();
    assert_eq!(firsts, vec![3, 4, 7, 10]);
    for n in 11..60 {
        let p = plan_wave(n, &mut rng);
        assert!(p.groups.iter().all(|g| g.1 >= 1), "wave {n}: {:?}", p.groups);
        assert_eq!(p.boss, n % 5 == 0, "wave {n}");
        assert_eq!(p.rush, n % 5 == 3, "wave {n}");
    }
}

#[test]
fn the_same_inputs_replay_the_same_round() {
    let run = || {
        let mut sim = Sim::new(11);
        let mut bot = Bot::new(DECENT, 5);
        let mut trace = Vec::new();
        for i in 0..(60 * 90) {
            bot.think(&mut sim, DT);
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            let hp: f32 = sim.creeps.iter().map(|c| c.hp + c.d).sum();
            trace.push((sim.score, sim.coins, sim.hearts, sim.wave, hp.to_bits()));
            sim.cues.clear();
        }
        trace
    };
    assert_eq!(run(), run());
}

/// The fast check: a first-timer's median round is in the target band.
#[test]
fn first_timer_round_length_is_in_range() {
    let mut secs: Vec<f32> = (0..24).map(|s| play_round(FIRST_TIMER, 500 + s, 600.0).seconds).collect();
    let (med, _, _) = median(&mut secs);
    assert!((40.0..=90.0).contains(&med), "first-timer median {med:.1}s");
}

/// Difficulty report (slow; run in release). Targets: a first-timer's round
/// lasts ~45–75 s (TD rounds run longer than the feed's 20–45 s, see
/// DESIGN.md), a good player's 2–3 minutes.
#[test]
#[ignore]
fn difficulty() {
    for skill in [FIRST_TIMER, DECENT, GOOD] {
        let n = 200;
        let mut secs = Vec::new();
        let mut scores = Vec::new();
        let mut waves = Vec::new();
        let mut killers = [0u32; 6];
        let mut ups = 0u32;
        let mut towers = 0usize;
        for seed in 0..n {
            let r = play_round(skill, 1000 + seed, 900.0);
            secs.push(r.seconds);
            scores.push(r.score as f32);
            waves.push(r.wave as f32);
            if let Some(k) = r.killer {
                killers[k as usize] += 1;
            }
            ups += r.upgrades;
            towers += r.towers;
        }
        let (m, p10, p90) = median(&mut secs);
        let (sm, s10, s90) = median(&mut scores);
        let (wm, w10, w90) = median(&mut waves);
        println!(
            "{:12} median {:5.1}s (p10 {:5.1} p90 {:5.1})  popped {:.0} ({:.0}-{:.0})  wave {:.0} ({:.0}-{:.0})  towers {:.1} upgrades {:.1}",
            skill.name,
            m,
            p10,
            p90,
            sm,
            s10,
            s90,
            wm,
            w10,
            w90,
            towers as f32 / n as f32,
            ups as f32 / n as f32
        );
        let names: Vec<String> = killers.iter().enumerate().filter(|k| *k.1 > 0).map(|(i, k)| format!("{} {}", CREEPS[i].name, k)).collect();
        println!("{:12} last bite by: {}", "", names.join(", "));
    }
    // One traced round, because medians swing.
    let mut sim = Sim::new(1234);
    let mut bot = Bot::new(DECENT, 77);
    let mut last_wave = 0;
    while !sim.over && sim.t < 900.0 {
        bot.think(&mut sim, DT);
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Built { pad, kind } => println!("  {:6.1}s build {:?} on pad {pad} (coins left {})", sim.t, kind, sim.coins),
                Cue::Upgraded { pad, level } => println!("  {:6.1}s upgrade pad {pad} to L{}", sim.t, level + 1),
                Cue::Bite { hearts, .. } => println!("  {:6.1}s BITE, hearts {hearts}", sim.t),
                Cue::Early { bonus } => println!("  {:6.1}s called early +{bonus}", sim.t),
                _ => {}
            }
        }
        if sim.wave != last_wave {
            last_wave = sim.wave;
            println!("  {:6.1}s wave {} ({:?}) score {} coins {}", sim.t, sim.wave, sim.next.groups, sim.score, sim.coins);
        }
    }
    println!("  over at {:.1}s, popped {}", sim.t, sim.score);
}

// ---- Mix ------------------------------------------------------------------------

/// A bot-played round through the real sound module, as an event log for
/// the skill's scripts/mix_check.py:
/// `MB_MIX_LOG=/tmp/td.log cargo test -p kit_tower_defense mix_log` then
/// `python3 .claude/skills/maimbrain-game/scripts/mix_check.py <kit>/assets /tmp/td.log`.
pub fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut snd = crate::sound::Sound::new();
    snd.log = Some(String::new());
    snd.start();
    let mut tail = 3.0;
    while tail > 0.0 && sim.t < max_seconds {
        if !sim.over {
            bot.think(&mut sim, DT);
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        snd.cues(&cues, true);
        let king = sim.creeps.iter().any(|c| c.kind == CreepKind::King);
        snd.update(DT, sim.danger().max(if king { 1.0 } else { 0.0 }));
        if sim.over {
            tail -= DT;
        }
    }
    snd.log.take().unwrap()
}

#[test]
fn mix_log() {
    let log = mix_round(DECENT, 5, 120.0);
    assert!(log.lines().count() > 50);
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}
