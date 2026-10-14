//! Rule tests, a check of every line of the question bank, and a bot that
//! plays like a person to tune difficulty:
//! `cargo test -p kit_trivia --profile bot -- --ignored --nocapture difficulty`
//! prints round lengths and how each skill preset loses.

use maimbrain::Rng;
use maimbrain::bot::{Difficulty, Outcome};

use crate::questions::{MAX_ANSWER_CHARS, MAX_QUESTION_CHARS, RAW, parse};
use crate::sim::*;

const DT: f32 = 1.0 / 60.0;

// ---- the bank -----------------------------------------------------------

#[test]
fn every_question_is_well_formed() {
    let qs = parse(RAW).expect("questions.txt parses");
    assert!(qs.len() >= 150, "only {} questions", qs.len());
    for tier in 1..=3 {
        let n = qs.iter().filter(|q| q.tier == tier).count();
        assert!(n >= 30, "tier {tier} has only {n} questions");
    }
    let mut seen = std::collections::BTreeSet::new();
    for q in &qs {
        assert!(q.text.chars().count() <= MAX_QUESTION_CHARS, "question too long for the card: {:?}", q.text);
        assert!(seen.insert(q.text), "asked twice: {:?}", q.text);
        for a in q.answers {
            assert!(a.chars().count() <= MAX_ANSWER_CHARS, "answer too long for its button: {a:?} in {:?}", q.text);
        }
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(q.answers[i].to_lowercase(), q.answers[j].to_lowercase(), "two answers alike in {:?}", q.text);
            }
        }
        // The host fonts: Inter covers ASCII and Latin-1 (plus a few marks).
        for c in q.text.chars().chain(q.answers.iter().flat_map(|a| a.chars())) {
            assert!((' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c) || "‘’“”•…–—€".contains(c), "{c:?} isn't in Inter: {:?}", q.text);
        }
    }
}

#[test]
fn bad_lines_are_reported() {
    assert!(parse("1|A|q?|a|b|c").is_err());
    assert!(parse("4|A|q?|a|b|c|d").is_err());
    assert!(parse("# comment\n\n2|A|q?|a|b|c|d").unwrap().len() == 1);
}

// ---- rules ----------------------------------------------------------------

fn run_until(s: &mut Sim, mut done: impl FnMut(&Sim) -> bool) {
    for _ in 0..60 * 120 {
        if done(s) || s.over {
            return;
        }
        s.step(DT);
    }
}

fn wrong_slot(s: &Sim) -> u8 {
    (s.cur.right_slot() + 1) % 4
}

#[test]
fn the_opening_cannot_be_failed() {
    for seed in 0..20 {
        // The worst start: a wrong answer the instant answers go live, every time.
        let mut s = Sim::new(seed, false);
        assert_eq!(s.cur.tier, 1, "the warm-up is easy");
        while s.t < 2.5 {
            if s.phase == Phase::Ask {
                s.answer(wrong_slot(&s));
            }
            s.step(DT);
        }
        assert_eq!(s.lives, LIVES, "seed {seed}: a life lost in the first 2.5 s");
        // Running out of time on the warm-up is free too.
        let mut s = Sim::new(seed, false);
        run_until(&mut s, |s| matches!(s.phase, Phase::Reveal { .. }));
        assert_eq!(s.lives, LIVES);
    }
}

#[test]
fn faster_answers_score_more_and_streaks_multiply() {
    let mut a = Sim::new(3, false);
    run_until(&mut a, |s| s.phase == Phase::Ask);
    a.answer(a.cur.right_slot());
    let mut b = Sim::new(3, false);
    run_until(&mut b, |s| s.phase == Phase::Ask && s.time_left() < 2.0);
    b.answer(b.cur.right_slot());
    assert!(a.score > b.score, "{} vs {}", a.score, b.score);
    assert_eq!(a.score, POINTS[0]);
    // A streak brings ×2 at three in a row.
    let mut s = Sim::new(5, false);
    let mut mults = Vec::new();
    for _ in 0..4 {
        run_until(&mut s, |s| s.phase == Phase::Ask);
        s.answer(s.cur.right_slot());
        mults.push(s.multiplier());
        s.cues.clear();
    }
    assert_eq!(mults, vec![1, 1, 2, 2]);
    assert_eq!(multiplier(MULT_STEPS[2]), 4);
}

#[test]
fn golden_questions_pay_double() {
    let mut s = Sim::new(9, false);
    while !s.cur.golden {
        run_until(&mut s, |s| s.phase == Phase::Ask);
        s.answer(s.cur.right_slot());
        run_until(&mut s, |s| s.phase == Phase::Deal);
    }
    assert_eq!(s.cur.index, GOLDEN_EVERY - 1);
    run_until(&mut s, |s| s.phase == Phase::Ask);
    let before = s.score;
    s.answer(s.cur.right_slot());
    let base = POINTS[(s.cur.tier - 1) as usize] * s.multiplier();
    assert_eq!(s.score - before, base * 2);
}

#[test]
fn three_misses_end_the_round_after_the_fail_animation() {
    let mut s = Sim::new(11, false);
    let mut misses = 0;
    while s.phase != Phase::Ending {
        run_until(&mut s, |s| s.phase == Phase::Ask || s.phase == Phase::Ending);
        if s.phase == Phase::Ask {
            s.answer(wrong_slot(&s));
            misses += 1;
            run_until(&mut s, |s| !matches!(s.phase, Phase::Reveal { .. }));
        }
    }
    assert_eq!(misses, LIVES + WARMUP, "the warm-up miss is free");
    assert!(!s.over, "the fail animation plays before Over");
    run_until(&mut s, |s| s.over);
    assert!(s.over && s.cues.contains(&Cue::Over));
    assert!(!s.answer(0), "nothing is live after the end");
}

#[test]
fn time_running_out_costs_a_life() {
    let mut s = Sim::new(2, false);
    run_until(&mut s, |s| s.phase == Phase::Ask);
    s.answer(s.cur.right_slot()); // past the warm-up
    run_until(&mut s, |s| s.phase == Phase::Ask);
    s.cues.clear();
    run_until(&mut s, |s| matches!(s.phase, Phase::Reveal { .. }));
    assert_eq!(s.lives, LIVES - 1);
    assert!(s.cues.contains(&Cue::Timeout));
    assert_eq!(s.cues.iter().filter(|c| matches!(c, Cue::Tick { .. })).count(), 3, "3, 2, 1");
}

#[test]
fn a_long_streak_gives_a_heart_back() {
    let mut s = Sim::new(4, false);
    run_until(&mut s, |s| s.phase == Phase::Ask);
    s.answer(wrong_slot(&s)); // warm-up: free
    run_until(&mut s, |s| s.phase == Phase::Ask);
    s.answer(wrong_slot(&s));
    assert_eq!(s.lives, LIVES - 1);
    for _ in 0..HEART_EVERY {
        run_until(&mut s, |s| s.phase == Phase::Ask);
        s.answer(s.cur.right_slot());
    }
    assert_eq!(s.lives, LIVES);
    assert!(s.cues.contains(&Cue::HeartBack));
}

#[test]
fn no_question_repeats_within_a_round() {
    let mut s = Sim::new(8, false);
    let mut asked = std::collections::BTreeSet::new();
    for _ in 0..120 {
        run_until(&mut s, |s| s.phase == Phase::Ask);
        assert!(asked.insert(s.cur.q.text), "repeat at question {}: {:?}", s.cur.index, s.cur.q.text);
        s.answer(s.cur.right_slot());
    }
}

/// Same seed + same inputs → same round, cue for cue.
#[test]
fn the_sim_is_deterministic() {
    let play = |seed: u64| {
        let mut s = Sim::new(seed, false);
        let mut bot = Bot::new(DECENT, seed);
        let mut log = Vec::new();
        while !s.over && s.t < 300.0 {
            if let Some(slot) = bot.think(&s, DT) {
                s.answer(slot);
            }
            s.step(DT);
            log.extend(s.cues.drain(..).map(|c| format!("{:.4} {c:?}", s.t)));
        }
        (s.score, s.t.to_bits(), log)
    };
    assert_eq!(play(42), play(42));
    assert_ne!(play(42).2, play(43).2);
}

/// Daily mode: everyone gets the same questions in the same order, with the
/// answers in the same places, whatever they answer.
#[test]
fn daily_rounds_are_the_same_for_everyone() {
    let seq = |seed: u64, policy: u32| {
        let mut s = Sim::new(seed, true);
        let mut out = Vec::new();
        for i in 0..12 {
            run_until(&mut s, |s| s.phase == Phase::Ask || s.over);
            if s.over {
                break;
            }
            out.push((s.cur.q.text, s.cur.order));
            let slot = if (i + policy) % 3 == 0 { wrong_slot(&s) } else { s.cur.right_slot() };
            s.answer(slot);
        }
        out
    };
    let a = seq(77, 0);
    let b = seq(77, 1);
    let n = a.len().min(b.len());
    assert!(n >= 6);
    assert_eq!(a[..n], b[..n]);
    assert_ne!(seq(77, 0), seq(78, 0));
}

#[test]
fn tiers_ramp_with_the_streak_and_the_question_number() {
    assert_eq!(tier_for(0, 9, false), 1, "warm-up");
    assert_eq!(tier_for(1, 0, false), 1);
    assert_eq!(tier_for(1, TIER2_STREAK, false), 2);
    assert_eq!(tier_for(2, TIER3_STREAK, false), 3);
    assert_eq!(tier_for(TIER3_FROM, 0, false), 3);
    assert_eq!(tier_for(DAILY_TIER3_FROM, 0, true), 3);
    assert!(pace(100) == PACE_MIN && pace(0) == 1.0);
    let q = crate::questions::bank()[0];
    assert!(time_limit(&q, 1) < time_limit(&q, 0));
}

// ---- layout -------------------------------------------------------------------

/// The play screen fits a phone's safe area (a 59 pt top inset with the
/// pause pill, a 34 pt bottom one) as well as the preview's: nothing overlaps.
#[test]
fn the_layout_fits_phones_and_the_preview() {
    use maimbrain::sys::Screen;
    use maimbrain::ui::Layout;
    for (top, bottom) in [(59.0, 34.0), (0.0, 0.0), (47.0, 34.0)] {
        let l = Layout::from_screen(&Screen { width: 360.0, height: 640.0, inset_top: top, inset_bottom: bottom, ..Default::default() });
        let g = crate::Geom::new(&l);
        let a = g.answers;
        assert!(a[3].bottom() <= l.safe.bottom() - 8.0, "answers above the home indicator");
        assert!(a[0].y >= g.card.bottom() + 12.0, "card clear of the answers");
        for w in a.windows(2) {
            assert!(w[1].y - w[0].bottom() >= 8.0 && w[0].h >= 44.0, "44 pt buttons with gaps");
        }
        // Watt's glass (radius 34) and the clock (radius 33) sit below the HUD.
        let hud_bottom = l.hud.y + 72.0;
        assert!(g.host.1 - 34.0 >= l.pill_zone().bottom() + 40.0, "Watt clear of the pause pill");
        assert!(g.timer.1 - 33.0 >= l.hud.y + 40.0, "clock clear of the hearts");
        assert!(g.streak.1 - 30.0 >= hud_bottom - 20.0, "streak badge clear of the score");
        // The title card: the card inside the card area, Watt and the clock
        // below the title text, all clear of the DAILY button.
        let (card, host, timer) = crate::Geom::title(&l);
        assert!(card.x >= l.card.x && card.right() <= l.card.right(), "title card inside the card area");
        assert!(host.1 - 34.0 * crate::TITLE_HOST >= l.safe.y + crate::TITLE_TOP - 4.0, "Watt below the title text");
        assert!(timer.0 + 33.0 <= l.card.right() + 1.0 && timer.1 - 33.0 >= l.safe.y + crate::TITLE_TOP - 4.0, "clock clear of the overlays and the title");
        assert!(card.bottom() <= l.card.bottom() - 50.0, "card clear of the DAILY button");
    }
}

// ---- the bot ----------------------------------------------------------------

/// How a person plays a quiz.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Chance of knowing the answer, by tier (easy, medium, hard).
    pub know: [f32; 3],
    /// Reading speed for the question, words per second (answers are
    /// scanned at twice that).
    pub wps: f32,
    /// Seconds to pick once read, when they know it / when they're guessing.
    pub think_known: (f32, f32),
    pub think_guess: (f32, f32),
    /// Reaction: from deciding to the finger landing.
    pub react: (f32, f32),
    /// A guess rules out one wrong answer first this often.
    pub eliminate: f32,
    /// Taps the wrong button by mistake (fat finger, misread) this often.
    pub slip: f32,
    /// When the clock is about to run out, taps *something* this often
    /// (the rest freeze and time out).
    pub clutch: f32,
}

pub const FIRST_TIMER: Skill = Skill {
    name: "first-timer",
    know: [0.86, 0.6, 0.42],
    wps: 3.2,
    think_known: (0.4, 1.0),
    think_guess: (1.5, 3.5),
    react: (0.2, 0.35),
    eliminate: 0.25,
    slip: 0.03,
    clutch: 0.5,
};
pub const DECENT: Skill = Skill {
    name: "decent",
    know: [0.92, 0.74, 0.56],
    wps: 4.0,
    think_known: (0.3, 0.8),
    think_guess: (1.2, 3.0),
    react: (0.17, 0.3),
    eliminate: 0.4,
    slip: 0.02,
    clutch: 0.7,
};
pub const GOOD: Skill = Skill {
    name: "good",
    know: [0.98, 0.9, 0.76],
    wps: 5.2,
    think_known: (0.2, 0.55),
    think_guess: (1.0, 2.4),
    react: (0.15, 0.25),
    eliminate: 0.6,
    slip: 0.01,
    clutch: 0.9,
};

pub struct Bot {
    skill: Skill,
    rng: Rng,
    /// The plan for the current question: (seconds into Ask to tap, slot), or None to freeze.
    plan: Option<(f32, u8)>,
    planned_for: Option<u32>,
}

fn words(s: &str) -> f32 {
    s.split_whitespace().count() as f32
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed ^ 0xb07), plan: None, planned_for: None }
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        self.rng.range(r.0, r.1)
    }

    /// Plans when and what to tap as the card lands, reading from the
    /// moment it starts flipping in. Returns a slot on the frame it taps.
    pub fn think(&mut self, sim: &Sim, dt: f32) -> Option<u8> {
        let q = sim.cur;
        if self.planned_for != Some(q.index) && sim.phase == Phase::Deal {
            self.planned_for = Some(q.index);
            let sk = self.skill;
            let knows = self.rng.f32() < sk.know[(q.tier - 1) as usize];
            let answers: f32 = q.q.answers.iter().map(|a| words(a)).sum();
            let read = (words(q.q.text) + answers * 0.5) / sk.wps * self.rng.range(0.85, 1.2);
            let think = if knows { self.range(sk.think_known) } else { self.range(sk.think_guess) };
            // Reading starts as the card flips in; the tap lands after a reaction.
            let mut at = read + think + self.range(sk.react) - DEAL_TIME * 0.5;
            let right = q.right_slot();
            let mut slot = if knows {
                if self.rng.f32() < sk.slip { (right + 1 + (self.rng.next_u32() % 3) as u8) % 4 } else { right }
            } else {
                // A guess: maybe rule one wrong answer out, then pick among the rest.
                let n = if self.rng.f32() < sk.eliminate { 3 } else { 4 };
                let k = self.rng.next_u32() % n;
                if k == 0 { right } else { (right + k as u8) % 4 }
            };
            if at > q.limit - 0.15 {
                if self.rng.f32() < sk.clutch {
                    // The ring is nearly empty: tap whatever they lean toward.
                    at = q.limit - self.rng.range(0.15, 0.9);
                    if !knows || self.rng.f32() < 0.5 {
                        slot = (right + (self.rng.next_u32() % 4) as u8) % 4;
                    }
                } else {
                    self.plan = None;
                    return None;
                }
            }
            self.plan = Some((at.max(0.05), slot));
        }
        let _ = dt;
        match (sim.phase, self.plan) {
            (Phase::Ask, Some((at, slot))) if sim.phase_t >= at => {
                self.plan = None;
                Some(slot)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoundStats {
    pub seconds: f32,
    pub score: u32,
    pub questions: u32,
    pub right: u32,
    pub best_streak: u32,
    pub timeouts: u32,
    pub wrongs: u32,
    /// The tier of the question that took the last life.
    pub last_tier: u8,
}

pub fn play_round(skill: Skill, seed: u64, daily: bool, trace: bool) -> RoundStats {
    let mut sim = Sim::new(seed, daily);
    let mut bot = Bot::new(skill, seed);
    let (mut timeouts, mut wrongs) = (0, 0);
    while !sim.over && sim.t < 600.0 {
        if let Some(slot) = bot.think(&sim, DT) {
            sim.answer(slot);
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Timeout => timeouts += 1,
                Cue::Wrong { .. } => wrongs += 1,
                _ => {}
            }
            if trace {
                match c {
                    Cue::Deal { .. } => println!(
                        "  {:5.1}s  Q{:<2} tier {} {:>4.1}s{}  {}",
                        sim.t,
                        sim.cur.index + 1,
                        sim.cur.tier,
                        sim.cur.limit,
                        if sim.cur.golden { " GOLD" } else { "" },
                        sim.cur.q.text
                    ),
                    Cue::Right { points, streak, .. } => println!("           right  +{points}  streak {streak}  (answered with {:.1}s left)", sim.answered_left),
                    Cue::Wrong { .. } => println!("           WRONG  ({:.1}s left)", sim.answered_left),
                    Cue::Timeout => println!("           TIME UP"),
                    Cue::LifeLost { left } => println!("           lives {left}"),
                    Cue::HeartBack => println!("           heart back!"),
                    _ => {}
                }
            }
        }
    }
    RoundStats {
        seconds: sim.t,
        score: sim.score,
        questions: sim.cur.index + 1,
        right: sim.right,
        best_streak: sim.best_streak,
        timeouts,
        wrongs,
        last_tier: sim.cur.tier,
    }
}

fn pct(v: &[f32], p: f32) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[((s.len() - 1) as f32 * p).round() as usize]
}

/// The fast guard: a first-timer's median round stays in 20–45 s.
#[test]
fn first_timer_rounds_last_20_to_45_seconds() {
    let secs: Vec<f32> = (0..40).map(|seed| play_round(FIRST_TIMER, 1000 + seed, false, false).seconds).collect();
    let median = pct(&secs, 0.5);
    assert!((20.0..=45.0).contains(&median), "first-timer median {median:.1}s");
}

/// Round lengths per preset, endless and daily, and how rounds are lost.
/// Targets (DESIGN.md, the skill): a first-timer's median round 20–45 s, a
/// good player's 1–3 min.
#[test]
#[ignore]
fn difficulty() {
    let tiers = ["easy", "medium", "hard"];
    for daily in [false, true] {
        println!("\n{}", if daily { "DAILY" } else { "ENDLESS" });
        let report = Difficulty::new(200)
            .preset(FIRST_TIMER.name, FIRST_TIMER)
            .target(20.0, 45.0)
            .preset(DECENT.name, DECENT)
            .preset(GOOD.name, GOOD)
            .target(60.0, 180.0)
            // Seeds 1, 8, 15…: what this report has always played.
            .run(|skill, seed| {
                let r = play_round(skill, seed * 7 + 1, daily, false);
                Outcome::new(r.seconds)
                    .cause(format!("last miss {}", tiers[(r.last_tier - 1) as usize]))
                    .stat("questions", r.questions as f32)
                    .stat("right", r.right as f32)
                    .stat("score", r.score as f32)
                    .stat("best streak", r.best_streak as f32)
                    .ratio("misses timed out", r.timeouts as f32, (r.timeouts + r.wrongs) as f32)
            });
        report.print();
        // What changed since the last run (committed: the last tuning that was kept).
        report.compare_with(if daily { "bot-report-daily.tsv" } else { "bot-report.tsv" });
        report.assert_skill_matters();
    }
    println!("\nOne traced first-timer round:");
    let r = play_round(FIRST_TIMER, 12, false, true);
    println!("  -> {:.1}s, {} points, {} of {} right", r.seconds, r.score, r.right, r.questions);
}

// ---- the platform's rules ---------------------------------------------------------

/// The how-to hint shows at the start of every round, not only the first
/// one a player ever plays (skill: directions every run).
#[test]
fn every_round_shows_how_to_answer_again() {
    use maimbrain::Game;
    let mut g = crate::Trivia::init();
    g.answered_once = true;
    g.start();
    assert!(!g.answered_once, "a new round shows the directions again");
}

/// The round flow on a fake host (SKILL.md "Platform facts that bite"): a
/// tap starts it, the score goes in before Over, a frantic tap doesn't
/// restart, the best is kept, the session replays. Idling loses too (three
/// timeouts after the free warm-up), but scores nothing, so the player
/// always taps the top answer: some are right, the third miss blows the bulb.
#[test]
fn keeps_the_platform_rules() {
    let a = crate::Geom::new(&maimbrain::ui::Layout::new()).answers[0];
    maimbrain::sys::test::Conformance::new()
        .play(move |t| if t % 0.5 < 1.0 / 60.0 { maimbrain::input::synth::tap(a.cx(), a.cy(), 0.0) } else { Vec::new() })
        .check::<crate::Trivia>();
}

// ---- Mix ------------------------------------------------------------------------

/// A bot-played round through the real sound module, as an event log for
/// the skill's scripts/mix_check.py:
/// `MB_MIX_LOG=/tmp/tr.log cargo test -p kit_trivia mix_log` then
/// `python3 .claude/skills/maimbrain-game/scripts/mix_check.py <kit>/assets /tmp/tr.log --fx-loop tension`.
pub fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    use crate::sim::Phase;
    use crate::sound;
    let mut sim = Sim::new(seed, false);
    let mut bot = Bot::new(skill, seed);
    let mut snd = sound::Sound::new();
    snd.log = Some(String::new());
    snd.start();
    let mut tail = 3.0;
    let mut ended = false;
    while tail > 0.0 && sim.t < max_seconds {
        if !sim.over
            && let Some(slot) = bot.think(&sim, DT)
            && sim.answer(slot)
        {
            snd.play(sound::TAP, 0.5, 1.0);
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        snd.cues(&cues);
        // As lib.rs sets them.
        let danger = sim.danger();
        snd.hi_target = danger.max(0.25 * (sim.multiplier() - 1) as f32).min(1.0);
        snd.tension_target = if sim.phase == Phase::Ask {
            let frac = sim.time_left() / sim.cur.limit.max(0.1);
            ((0.6 - frac) / 0.45).clamp(0.0, 1.0).max(danger)
        } else {
            0.0
        };
        snd.update(DT);
        if sim.over {
            if !ended {
                ended = true;
                snd.play(sound::POP, 0.55, 1.0);
                snd.verdict(false);
            }
            tail -= DT;
        }
    }
    snd.log.take().unwrap()
}

#[test]
fn mix_log() {
    let log = mix_round(DECENT, 11, 90.0);
    assert!(log.lines().count() > 50);
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}
