//! Rule tests, determinism, and the difficulty report from the bot.
//!
//! `cargo test -p kit_shooter` runs the rules; the full report is
//! `cargo test -p kit_shooter --profile bot -- --ignored --nocapture difficulty`.

use crate::bot::{Bot, DECENT, FIRST_TIMER, GOOD, Skill};
use crate::sim::*;

const DT: f32 = 1.0 / 60.0;

fn run(sim: &mut Sim, seconds: f32) {
    for _ in 0..(seconds / DT).round() as usize {
        sim.step(DT);
    }
}

#[test]
fn the_first_seconds_cannot_fail() {
    for seed in 0..20 {
        // Doing nothing at all…
        let mut sim = Sim::new(seed);
        run(&mut sim, 3.5);
        assert_eq!(sim.ship.hp, LIVES, "seed {seed}: hurt while idle");
        assert!(!sim.cues.iter().any(|c| matches!(c, Cue::Hurt { .. })));
        // …or wiggling wildly.
        let mut sim = Sim::new(seed);
        sim.touch_down(180.0, 560.0);
        for i in 0..210 {
            let t = i as f32 * DT;
            sim.touch_move(180.0 + (t * 5.0).sin() * 150.0, 560.0 - (t * 3.0).sin().abs() * 300.0);
            sim.step(DT);
        }
        assert_eq!(sim.ship.hp, LIVES, "seed {seed}: hurt while wiggling");
    }
}

#[test]
fn an_idle_ship_scores_its_first_kill_within_two_seconds() {
    for seed in 0..20 {
        let mut sim = Sim::new(seed);
        run(&mut sim, 2.0);
        assert!(sim.kills >= 1, "seed {seed}: no kill in 2 s");
        assert!(sim.score >= 100);
    }
}

#[test]
fn touching_down_does_not_move_the_ship_and_drags_are_relative() {
    let mut sim = Sim::new(1);
    let (x0, y0) = (sim.ship.x, sim.ship.y);
    // Touch far to the left and below: nothing moves.
    sim.touch_down(40.0, 620.0);
    run(&mut sim, 0.3);
    assert!((sim.ship.x - x0).abs() < 0.01 && (sim.ship.y - y0).abs() < 0.01);
    // Drag 40 right: the ship goes 40 × gain right.
    sim.touch_move(80.0, 620.0);
    run(&mut sim, 0.5);
    assert!((sim.ship.x - (x0 + 40.0 * DRAG_GAIN)).abs() < 0.5, "x {}", sim.ship.x);
    // Lift: touching right under the ship floats it up above the thumb.
    let mut sim = Sim::new(1);
    sim.touch_down(sim.ship.x, sim.ship.y + 20.0);
    run(&mut sim, 1.0);
    let gap = sim.grab.unwrap().y - sim.ship.y;
    assert!(gap > MIN_LIFT - 4.0, "the thumb covers the ship: gap {gap}");
}

#[test]
fn dragging_past_an_edge_responds_at_once_on_the_way_back() {
    let mut sim = Sim::new(1);
    sim.touch_down(180.0, 600.0);
    sim.touch_move(400.0, 600.0); // far past the right edge
    run(&mut sim, 0.5);
    assert!((sim.ship.x - SHIP_X.1).abs() < 1.0);
    sim.touch_move(380.0, 600.0); // 20 back
    run(&mut sim, 0.5);
    assert!((sim.ship.x - (SHIP_X.1 - 20.0 * DRAG_GAIN)).abs() < 1.0, "x {}", sim.ship.x);
}

#[test]
fn kills_chain_into_a_combo_multiplier() {
    let mut sim = Sim::new(3);
    sim.hold_waves();
    for i in 0..10 {
        sim.add_enemy(Kind::Jelly, SHIP_START.0, 300.0 - i as f32 * 40.0);
    }
    run(&mut sim, 1.4);
    let kills: Vec<(u64, u32)> = sim.cues.iter().filter_map(|c| if let Cue::Kill { points, mult, .. } = c { Some((*points, *mult)) } else { None }).collect();
    assert!(kills.len() >= 8, "{kills:?}");
    assert_eq!(kills[0], (100, 1));
    assert_eq!(kills[4], (200, 2), "the fifth kill in a row scores double");
    // The combo runs out after its window.
    run(&mut sim, COMBO_WINDOW + 0.5);
    assert_eq!(sim.combo, 0);
}

#[test]
fn a_hit_costs_a_heart_then_blinks() {
    let mut sim = Sim::new(4);
    run(&mut sim, 0.1);
    let (x, y) = (sim.ship.x, sim.ship.y);
    sim.add_bullet(x, y - 4.0, 0.0, 10.0);
    sim.step(DT);
    assert_eq!(sim.ship.hp, LIVES - 1);
    assert!(sim.ship.invuln > 0.0);
    // Another bullet while blinking passes through.
    sim.add_bullet(x, y, 0.0, 10.0);
    sim.step(DT);
    assert_eq!(sim.ship.hp, LIVES - 1);
}

#[test]
fn a_shield_takes_the_hit() {
    let mut sim = Sim::new(5);
    run(&mut sim, 0.1);
    sim.add_drop(DropKind::Power(Power::Shield), sim.ship.x, sim.ship.y);
    sim.step(DT);
    assert!(sim.ship.shield);
    sim.add_bullet(sim.ship.x, sim.ship.y, 0.0, 0.0);
    sim.step(DT);
    assert_eq!(sim.ship.hp, LIVES);
    assert!(!sim.ship.shield);
    assert!(sim.cues.iter().any(|c| matches!(c, Cue::ShieldBreak { .. })));
}

#[test]
fn spread_fires_three_then_five() {
    let mut sim = Sim::new(6);
    sim.add_drop(DropKind::Power(Power::Spread), sim.ship.x, sim.ship.y);
    sim.step(DT);
    assert_eq!(sim.ship.spread, 1);
    sim.add_drop(DropKind::Power(Power::Spread), sim.ship.x, sim.ship.y);
    sim.step(DT);
    assert_eq!(sim.ship.spread, 2);
    run(&mut sim, POWER_TIME + 0.1);
    assert_eq!(sim.ship.spread, 0, "spread runs out");
}

#[test]
fn the_last_heart_plays_the_death_then_ends_the_round() {
    let mut sim = Sim::new(7);
    run(&mut sim, 0.1);
    for _ in 0..LIVES {
        sim.ship.invuln = 0.0;
        sim.add_bullet(sim.ship.x, sim.ship.y, 0.0, 0.0);
        sim.step(DT);
    }
    assert_eq!(sim.ship.hp, 0);
    assert!(sim.dying.is_some() && !sim.over, "the death plays while still in the round");
    assert!(sim.killer.is_some());
    run(&mut sim, DEATH_TIME + 0.1);
    assert!(sim.over);
    let overs = sim.cues.iter().filter(|c| **c == Cue::Over).count();
    assert_eq!(overs, 1);
    // The aftermath keeps running under the card without scoring.
    let score = sim.score;
    run(&mut sim, 2.0);
    assert_eq!(sim.score, score);
}

#[test]
fn the_boss_arrives_on_time_and_can_be_beaten() {
    let mut sim = Sim::new(8);
    sim.skip_to(BOSS_AT - WARNING_TIME - 0.5);
    sim.ship.invuln = 999.0;
    run(&mut sim, 1.0);
    assert!(sim.cues.contains(&Cue::Warning));
    run(&mut sim, WARNING_TIME);
    assert!(sim.boss.is_some(), "boss at {}", sim.phase);
    // Sit under it with spread and the boss goes down.
    sim.ship.spread = 2;
    sim.ship.spread_t = 999.0;
    let mut t = 0.0;
    while sim.loop_n == 0 && t < 60.0 {
        let bx = sim.boss.map_or(180.0, |b| b.x);
        sim.ship.tx = bx;
        sim.ship.ty = 420.0;
        sim.ship.invuln = 999.0;
        sim.step(DT);
        t += DT;
    }
    assert_eq!(sim.loop_n, 1, "boss still up after {t:.0}s");
    assert!(sim.cues.iter().any(|c| matches!(c, Cue::BossDown { .. })));
    assert!(sim.drops.iter().any(|d| d.kind == DropKind::Heart));
}

#[test]
fn new_enemies_join_every_ten_to_fifteen_seconds() {
    let mut sim = Sim::new(9);
    sim.ship.invuln = 999.0;
    let mut news = Vec::new();
    while sim.t < BOSS_AT + 1.0 {
        sim.ship.invuln = 999.0;
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::NewEnemy(k) = c {
                news.push((k, sim.t));
            }
        }
    }
    let kinds: Vec<Kind> = news.iter().map(|n| n.0).collect();
    assert_eq!(kinds, vec![Kind::Swooper, Kind::Darter, Kind::Bulb], "{news:?}");
    assert!(sim.boss.is_some());
}

#[test]
fn the_same_seed_and_thumb_replay_the_same_round() {
    let trace = |seed: u64| {
        let mut sim = Sim::new(seed);
        let mut bot = Bot::new(DECENT, 77);
        let mut out = Vec::new();
        for i in 0..3000 {
            bot.drive(&mut sim, DT);
            // Uneven frames, as on a phone.
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            out.push((sim.score, sim.ship.hp, sim.ship.x.to_bits(), sim.bullets.len(), sim.enemies.len()));
        }
        out
    };
    assert_eq!(trace(11), trace(11));
    assert_ne!(trace(11), trace(12));
}

#[test]
fn a_daily_seed_gives_everyone_the_same_waves() {
    // Two players with the same seed but different thumbs see the same formations.
    let spawns = |skill: Skill| {
        let mut sim = Sim::new(2026);
        let mut bot = Bot::new(skill, 1);
        let mut seen = Vec::new();
        while sim.t < 30.0 && sim.alive() {
            bot.drive(&mut sim, DT);
            sim.ship.invuln = 999.0; // keep both alive to compare
            sim.step(DT);
            for e in &sim.enemies {
                if e.age == DT {
                    seen.push((e.kind, (e.x0 * 10.0) as i32, (sim.t * 10.0) as i32));
                }
            }
        }
        seen
    };
    assert_eq!(spawns(FIRST_TIMER), spawns(GOOD));
}

// ---- Difficulty ---------------------------------------------------------

pub struct Round {
    pub seconds: f32,
    pub score: u64,
    pub bosses: u32,
    /// What killed it: "bullet", "body" or "boss", and the phase it was in.
    pub cause: &'static str,
    pub phase: &'static str,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> Round {
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut cause = "alive";
    let mut phase = "";
    while !sim.over && sim.t < max_seconds {
        bot.drive(&mut sim, DT);
        let had_boss = sim.boss.is_some();
        let n_enemies = sim.enemies.len();
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::Dying { .. } = c {
                cause = if had_boss && sim.bullets.is_empty() { "boss" } else if sim.enemies.len() < n_enemies { "body" } else { "bullet" };
                phase = if had_boss {
                    "boss"
                } else if sim.phase < 13.0 && sim.loop_n == 0 {
                    "jellies"
                } else if sim.phase < 24.0 && sim.loop_n == 0 {
                    "swoopers"
                } else if sim.phase < 35.0 && sim.loop_n == 0 {
                    "darters"
                } else if sim.loop_n == 0 {
                    "bulbs"
                } else {
                    "loop 2+"
                };
            }
        }
    }
    Round { seconds: sim.t, score: sim.score, bosses: sim.loop_n, cause, phase }
}

fn pct(v: &[f32], p: f32) -> f32 {
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// Fast check (debug build too): a first-timer's median round is in range.
#[test]
fn a_first_timer_lasts_twenty_to_fortyfive_seconds() {
    let mut secs: Vec<f32> = (0..24).map(|s| play_round(FIRST_TIMER, 500 + s, 120.0).seconds).collect();
    secs.sort_by(f32::total_cmp);
    let median = pct(&secs, 0.5);
    assert!((18.0..=50.0).contains(&median), "first-timer median {median:.1}s: {secs:?}");
}

/// The full report (slow; run in release).
#[test]
#[ignore]
fn difficulty() {
    for skill in [FIRST_TIMER, DECENT, GOOD] {
        let n = 200;
        let rounds: Vec<Round> = (0..n).map(|s| play_round(skill, 1000 + s, 600.0)).collect();
        let mut secs: Vec<f32> = rounds.iter().map(|r| r.seconds).collect();
        secs.sort_by(f32::total_cmp);
        let mut scores: Vec<f32> = rounds.iter().map(|r| r.score as f32).collect();
        scores.sort_by(f32::total_cmp);
        let bosses = rounds.iter().filter(|r| r.bosses > 0).count();
        let tally = |key: fn(&Round) -> &'static str| {
            let mut t: Vec<(&str, usize)> = Vec::new();
            for r in &rounds {
                let k = key(r);
                match t.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1 += 1,
                    None => t.push((k, 1)),
                }
            }
            t.sort_by(|a, b| b.1.cmp(&a.1));
            t.iter().map(|(k, c)| format!("{k} {}%", c * 100 / n as usize)).collect::<Vec<_>>().join(", ")
        };
        println!(
            "{:12} median {:5.1}s (p10 {:5.1}, p90 {:5.1})  score median {:6.0}  beat the boss {:3}%\n             died in: {}\n             killed by: {}",
            skill.name,
            pct(&secs, 0.5),
            pct(&secs, 0.1),
            pct(&secs, 0.9),
            pct(&scores, 0.5),
            bosses * 100 / n as usize,
            tally(|r| r.phase),
            tally(|r| r.cause),
        );
    }
    // One traced run, because medians swing on small changes.
    let mut sim = Sim::new(1001);
    let mut bot = Bot::new(FIRST_TIMER, 1001 ^ 0x9e37_79b9);
    let mut next = 0.0;
    while !sim.over && sim.t < 300.0 {
        bot.drive(&mut sim, DT);
        sim.step(DT);
        for c in std::mem::take(&mut sim.cues) {
            match c {
                Cue::Hurt { hp, .. } => println!("  {:5.1}s hurt, {hp} left (bullets {}, heat {:.2})", sim.t, sim.bullets.len(), sim.heat()),
                Cue::NewEnemy(k) => println!("  {:5.1}s new: {k:?}", sim.t),
                Cue::BossArrive => println!("  {:5.1}s boss", sim.t),
                Cue::BossDown { .. } => println!("  {:5.1}s boss down", sim.t),
                Cue::Power { power, .. } => println!("  {:5.1}s power {power:?}", sim.t),
                _ => {}
            }
        }
        if sim.t >= next {
            next += 10.0;
            println!("  {:5.1}s score {:6} kills {:3} bullets {:3} enemies {:2} heat {:.2}", sim.t, sim.score, sim.kills, sim.bullets.len(), sim.enemies.len(), sim.heat());
        }
    }
    println!("  over at {:.1}s, score {}", sim.t, sim.score);
}

/// Writes a bot-played round's sound events (plays, loop starts, track
/// levels) to `$MIX_TRACE` for an offline mix: `tools/mixsim.py` renders it
/// with the real assets and measures the effects against the music.
///   MIX_TRACE=/tmp/trace.csv cargo test -p kit_shooter --profile bot -- --ignored mix_trace
#[test]
#[ignore]
fn mix_trace() {
    let path = std::env::var("MIX_TRACE").unwrap_or_else(|_| "mix_trace.csv".into());
    let seed: u64 = std::env::var("MIX_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1003);
    let skill = match std::env::var("MIX_SKILL").as_deref() {
        Ok("first") => FIRST_TIMER,
        Ok("good") => GOOD,
        _ => DECENT,
    };
    let mut sim = Sim::new(seed);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut snd = crate::sound::Sound::new();
    snd.update(DT, &[], false, 0.0, false, false);
    snd.start();
    let mut after = 0.0;
    while after < 3.0 && sim.t < 240.0 {
        let playing = !sim.over;
        if playing {
            bot.drive(&mut sim, DT);
        } else {
            after += DT;
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        if cues.contains(&Cue::Over) {
            snd.verdict(false);
        }
        let boss = sim.boss.as_ref().is_some_and(|b| b.dying.is_none());
        let shield = sim.ship.shield && sim.alive();
        snd.update(DT, &cues, playing, sim.intensity(), boss, shield);
    }
    let names = crate::sound::FILES;
    let mut out = String::from("t,kind,sound,vol,pitch,voice\n");
    for (t, k, i, v, p, voice) in &snd.log {
        out += &format!("{t:.4},{k},{},{v:.4},{p:.4},{voice}\n", names[*i]);
    }
    std::fs::write(&path, out).unwrap();
    println!("{} events, round {:.1}s, score {}, wrote {path}", snd.log.len(), sim.t, sim.score);
}
