//! Rule tests and a bot that plays like a person, to tune difficulty.
//! `cargo test -p kit_match3 --profile bot -- --ignored --nocapture difficulty`
//! prints round lengths per skill preset (`maimbrain::bot::Difficulty`).

use maimbrain::Rng;
use maimbrain::bot::{Difficulty, Outcome};

use crate::sim::*;

const DT: f32 = 1.0 / 60.0;

// ---- The bot ------------------------------------------------------------------

/// How a person plays a swap puzzle: they need time to take in the board,
/// they don't see every move, they misjudge which move is best, and now and
/// then they swap two things that don't match at all.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    pub name: &'static str,
    /// Seconds from the board settling to starting to look (reaction).
    pub reaction: (f32, f32),
    /// Seconds spent scanning the board before acting.
    pub think: (f32, f32),
    /// Chance of spotting any given move per scan (more each extra scan).
    pub notice: f32,
    /// Misjudgement of a move's worth, in poppets.
    pub noise: f32,
    /// How much they value making and using specials (0 = blind to them).
    pub specials: f32,
    /// Chance a move is a misread: a swap that doesn't match.
    pub misread: f32,
    /// Share of the cascade time they spend already scanning (anticipation).
    pub overlap: f32,
    /// Seconds to put the thumb on the piece and swipe.
    pub motor: (f32, f32),
}

pub const FIRST_TIMER: Skill = Skill {
    name: "first-timer",
    reaction: (0.2, 0.35),
    think: (1.8, 3.6),
    notice: 0.3,
    noise: 1.5,
    specials: 0.15,
    misread: 0.15,
    overlap: 0.0,
    motor: (0.18, 0.35),
};
pub const DECENT: Skill = Skill {
    name: "decent",
    reaction: (0.17, 0.28),
    think: (1.0, 2.0),
    notice: 0.5,
    noise: 1.0,
    specials: 0.6,
    misread: 0.06,
    overlap: 0.3,
    motor: (0.13, 0.25),
};
pub const GOOD: Skill = Skill {
    name: "good",
    reaction: (0.15, 0.22),
    think: (0.45, 1.0),
    notice: 0.8,
    noise: 0.5,
    specials: 1.0,
    misread: 0.015,
    overlap: 0.6,
    motor: (0.1, 0.17),
};

pub struct Bot {
    skill: Skill,
    rng: Rng,
    seen: u32,
    /// Seconds until the next decision.
    timer: f32,
    scans: u32,
    /// Seconds spent watching the board settle since the last swap.
    watched: f32,
    pending: Option<(Move, f32)>,
}

impl Bot {
    pub fn new(skill: Skill, seed: u64) -> Bot {
        Bot { skill, rng: Rng::new(seed), seen: u32::MAX, timer: 0.0, scans: 0, watched: 0.0, pending: None }
    }

    fn r(&mut self, (lo, hi): (f32, f32)) -> f32 {
        self.rng.range(lo, hi)
    }

    fn gauss(&mut self) -> f32 {
        (0..4).map(|_| self.rng.f32()).sum::<f32>() - 2.0
    }

    /// A random swap of two neighbours (a misread).
    fn random_swap(&mut self) -> Move {
        let c = (self.rng.next_u32() as usize % COLS, self.rng.next_u32() as usize % ROWS);
        let d = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)][self.rng.next_u32() as usize % 4];
        let (x, y) = ((c.0 as i32 + d.0).clamp(0, COLS as i32 - 1) as usize, (c.1 as i32 + d.1).clamp(0, ROWS as i32 - 1) as usize);
        if (x, y) == c { (c, if c.0 > 0 { (c.0 - 1, c.1) } else { (c.0 + 1, c.1) }) } else { (c, (x, y)) }
    }

    /// The swap the bot makes this frame, if any.
    pub fn think(&mut self, sim: &Sim, dt: f32) -> Option<Move> {
        if let Some((m, t)) = &mut self.pending {
            *t -= dt;
            if *t <= 0.0 {
                let m = *m;
                self.pending = None;
                // After a swap: look again (a bounce leaves the same board).
                self.timer = self.r(self.skill.reaction) + 0.5 * self.r(self.skill.think);
                return Some(m);
            }
            return None;
        }
        if !sim.ready() {
            self.watched += dt;
            return None;
        }
        if sim.settles != self.seen {
            self.seen = sim.settles;
            let think = self.r(self.skill.think);
            let credit = (self.watched * self.skill.overlap).min(think * 0.7);
            self.timer = self.r(self.skill.reaction) + think - credit;
            self.watched = 0.0;
            self.scans = 0;
        }
        self.timer -= dt;
        if self.timer > 0.0 {
            return None;
        }
        let notice = (self.skill.notice + 0.2 * self.scans as f32).min(1.0);
        let mut best: Option<(f32, Move)> = None;
        for &m in &sim.moves {
            if self.rng.f32() >= notice {
                continue;
            }
            let p = sim.preview(m);
            let v = p.pieces as f32 + self.skill.specials * (3.0 * p.made as f32 + 1.5 * p.fires as f32) + self.gauss() * self.skill.noise;
            if best.is_none_or(|(bv, _)| v > bv) {
                best = Some((v, m));
            }
        }
        let m = if self.rng.f32() < self.skill.misread {
            self.random_swap()
        } else if let Some((_, m)) = best {
            m
        } else {
            // Nothing spotted yet: keep looking.
            self.scans += 1;
            self.timer = 0.5 * self.r(self.skill.think);
            return None;
        };
        // Swipe one way or the other.
        let m = if self.rng.f32() < 0.5 { m } else { (m.1, m.0) };
        self.pending = Some((m, self.r(self.skill.motor)));
        None
    }
}

#[derive(Debug)]
pub struct RoundStats {
    pub seconds: f32,
    pub score: u32,
    pub level: u32,
    pub swaps: u32,
    pub best_chain: u32,
}

pub fn play_round(skill: Skill, seed: u64, max_seconds: f32) -> RoundStats {
    let mut sim = Sim::new(seed, false);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    while !sim.over && sim.t < max_seconds {
        if let Some(m) = bot.think(&sim, DT) {
            sim.swap(m.0, m.1);
        }
        sim.step(DT);
        sim.cues.clear();
    }
    RoundStats { seconds: sim.t, score: sim.score, level: sim.level, swaps: sim.swaps, best_chain: sim.best_chain }
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

// ---- Board helpers ------------------------------------------------------------

/// A board with no matches anywhere: color (c + 2r) mod 5. Tests paint color 5 on it.
fn plain(sim: &mut Sim) {
    for (c, r) in Board::cells() {
        let p = sim.board.get((c, r)).unwrap();
        sim.board.g[r][c] = Some(Piece { kind: Kind::Color(((c + 2 * r) % 5) as u8), special: Special::None, star: false, ..p });
    }
}

fn paint(sim: &mut Sim, cells: &[Cell], kind: Kind, special: Special) {
    for &(c, r) in cells {
        let p = sim.board.get((c, r)).unwrap();
        sim.board.g[r][c] = Some(Piece { kind, special, ..p });
    }
}

/// Runs the sim until the board is ready again (or `secs` pass).
fn settle(sim: &mut Sim, secs: f32) {
    let mut t = 0.0;
    sim.step(DT);
    while !sim.ready() && t < secs && !sim.over {
        sim.step(DT);
        t += DT;
    }
}

/// Steps until the swap resolves into its first pop.
fn until_pop(sim: &mut Sim) {
    for _ in 0..60 {
        sim.step(DT);
        if matches!(sim.phase, Phase::Pop { .. }) {
            return;
        }
    }
    panic!("no pop: {:?}", sim.phase);
}

// ---- Rules --------------------------------------------------------------------

#[test]
fn boards_start_with_no_matches_and_a_move() {
    for seed in 0..200 {
        let sim = Sim::new(seed, false);
        assert!(find_groups(&sim.board).is_empty(), "seed {seed}");
        assert!(!sim.moves.is_empty() && sim.hint.is_some(), "seed {seed}");
        assert!(Board::cells().all(|c| sim.board.get(c).is_some()));
    }
}

#[test]
fn the_first_seconds_cant_be_lost() {
    // Doing nothing at all: still awake after 3 s (and well after), asleep eventually.
    let mut sim = Sim::new(4, false);
    for _ in 0..180 {
        sim.step(DT);
    }
    assert!(sim.energy > 0.85 && sim.ready(), "energy {}", sim.energy);
    let mut t = 3.0;
    while !sim.over && t < 120.0 {
        sim.step(DT);
        t += DT;
    }
    assert!(sim.over, "an idle round never ended");
    assert!((15.0..35.0).contains(&sim.t), "idle round lasted {:.1}s", sim.t);
}

#[test]
fn a_match_pops_scores_and_refills_the_meter() {
    let mut sim = Sim::new(1, false);
    plain(&mut sim);
    // Two purples in row 7 and one above the gap: swap it down.
    paint(&mut sim, &[(0, 7), (1, 7), (2, 6)], Kind::Color(5), Special::None);
    sim.settle();
    sim.energy = 0.5;
    assert!(sim.swap((2, 6), (2, 7)));
    until_pop(&mut sim);
    assert_eq!(sim.last.pieces, 3);
    assert_eq!(sim.score, 3 * POINTS_PIECE);
    assert!(sim.energy > 0.5 + 2.5 * GAIN_PIECE);
    assert!(sim.last.made.is_empty());
    settle(&mut sim, 10.0);
    assert!(Board::cells().all(|c| sim.board.get(c).is_some()), "the holes refilled");
    assert!(find_groups(&sim.board).is_empty());
}

#[test]
fn a_swap_that_matches_nothing_bounces_back() {
    let mut sim = Sim::new(2, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 7), (1, 7), (2, 6)], Kind::Color(5), Special::None); // a move, so it doesn't shuffle
    sim.settle();
    let before = sim.board.clone();
    assert!(sim.swap((3, 3), (4, 3)));
    let mut bounced = false;
    for _ in 0..40 {
        sim.step(DT);
        bounced |= matches!(sim.phase, Phase::Bounce { .. });
    }
    assert!(bounced && sim.ready());
    assert_eq!(sim.score, 0);
    assert!(Board::cells().all(|c| sim.board.get(c) == before.get(c)));
    assert!(sim.cues.contains(&Cue::Bounce));
}

#[test]
fn four_makes_a_line_an_l_makes_a_bomb_five_makes_a_rainbow() {
    // 4 in a row → a line blaster where the swiped piece landed.
    let mut sim = Sim::new(3, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 7), (1, 7), (3, 7), (2, 6)], Kind::Color(5), Special::None);
    sim.settle();
    sim.swap((2, 6), (2, 7));
    until_pop(&mut sim);
    let p = sim.board.get((2, 7)).unwrap();
    assert_eq!((p.kind, p.special), (Kind::Color(5), Special::LineH));
    assert_eq!(sim.last.pieces, 3);

    // An L → a bomb.
    let mut sim = Sim::new(3, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 6), (1, 6), (2, 5), (2, 4), (3, 6)], Kind::Color(5), Special::None);
    sim.settle();
    sim.swap((3, 6), (2, 6));
    until_pop(&mut sim);
    let p = sim.board.get((2, 6)).unwrap();
    assert_eq!(p.special, Special::Bomb, "{:?}", sim.last.made);

    // 5 → a rainbow.
    let mut sim = Sim::new(3, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 7), (1, 7), (3, 7), (4, 7), (2, 6)], Kind::Color(5), Special::None);
    sim.settle();
    sim.swap((2, 6), (2, 7));
    until_pop(&mut sim);
    assert_eq!(sim.board.get((2, 7)).unwrap().kind, Kind::Rainbow);
}

#[test]
fn a_line_blaster_pops_its_row_and_sets_off_others() {
    let mut sim = Sim::new(5, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 4), (1, 4), (2, 3)], Kind::Color(5), Special::None);
    paint(&mut sim, &[(1, 4)], Kind::Color(5), Special::LineH);
    paint(&mut sim, &[(5, 4)], Kind::Color(1), Special::LineV);
    sim.settle();
    sim.swap((2, 3), (2, 4));
    until_pop(&mut sim);
    let popped: Vec<Cell> = sim.last.popping.iter().map(|p| p.at).collect();
    for c in 0..COLS {
        assert!(popped.contains(&(c, 4)), "row cell {c} popped");
    }
    for r in 0..ROWS {
        assert!(popped.contains(&(5, r)), "the column blaster it hit went off too: row {r}");
    }
    assert_eq!(sim.last.fires, 2);
}

#[test]
fn a_rainbow_swapped_with_a_poppet_pops_all_of_that_color() {
    let mut sim = Sim::new(6, false);
    plain(&mut sim);
    paint(&mut sim, &[(3, 3)], Kind::Rainbow, Special::None);
    sim.settle();
    let color = sim.board.get((4, 3)).unwrap().color().unwrap();
    let n = sim.board.count(|p| p.color() == Some(color));
    assert!(sim.swap((3, 3), (4, 3)));
    until_pop(&mut sim);
    assert_eq!(sim.last.pieces, n + 1);
    assert!(sim.board.count(|p| p.color() == Some(color)) == 0);
    assert!(sim.cues.contains(&Cue::Combo));
}

#[test]
fn two_bombs_swapped_blow_a_5x5() {
    let mut sim = Sim::new(7, false);
    plain(&mut sim);
    paint(&mut sim, &[(3, 4)], Kind::Color(0), Special::Bomb);
    paint(&mut sim, &[(3, 3)], Kind::Color(1), Special::Bomb);
    sim.settle();
    sim.swap((3, 4), (3, 3));
    until_pop(&mut sim);
    assert_eq!(sim.last.pieces, 25);
}

#[test]
fn rocks_crack_next_to_a_match() {
    let mut sim = Sim::new(8, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 7), (1, 7), (2, 6)], Kind::Color(5), Special::None);
    paint(&mut sim, &[(0, 6)], Kind::Rock, Special::None);
    sim.settle();
    assert!(!sim.swap((0, 6), (1, 6)), "rocks don't move");
    sim.swap((2, 6), (2, 7));
    until_pop(&mut sim);
    assert_eq!(sim.last.rocks, 1);
}

#[test]
fn cascades_multiply() {
    // A column of three pops; what falls into its place lines up with row 7.
    let mut sim = Sim::new(9, false);
    plain(&mut sim);
    paint(&mut sim, &[(0, 6), (0, 7), (1, 5)], Kind::Color(5), Special::None); // the move
    paint(&mut sim, &[(0, 4), (1, 7), (2, 7)], Kind::Color(6), Special::None); // the cascade
    sim.settle();
    assert!(find_groups(&sim.board).is_empty());
    sim.swap((1, 5), (0, 5));
    let mut chains = 0;
    let mut t = 0.0;
    while t < 6.0 {
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::Pop { chain, .. } = c {
                chains = chains.max(chain);
            }
        }
        t += DT;
    }
    assert!(chains >= 2, "no cascade");
    assert!(sim.score >= 3 * POINTS_PIECE + 3 * POINTS_PIECE * 2);
}

#[test]
fn a_stuck_board_shuffles() {
    let mut sim = Sim::new(10, false);
    plain(&mut sim);
    sim.settle();
    if !sim.moves.is_empty() {
        // The plain board has moves; make one with none: stripes of 6 colors.
        for (c, r) in Board::cells() {
            let p = sim.board.get((c, r)).unwrap();
            sim.board.g[r][c] = Some(Piece { kind: Kind::Color(((c + 3 * r) % 6) as u8), ..p });
        }
        assert!(moves(&sim.board).is_empty(), "test board has a move");
        sim.settle();
    }
    assert!(matches!(sim.phase, Phase::Shuffle { .. }));
    assert!(!sim.moves.is_empty());
    assert!(find_groups(&sim.board).is_empty());
    assert!(sim.cues.contains(&Cue::Shuffle));
}

#[test]
fn levels_bring_something_new() {
    let mut sim = Sim::new(11, false);
    let mut news = Vec::new();
    while sim.t < 8.0 * LEVEL_TIME + 1.0 {
        sim.energy = 1.0; // stay awake
        sim.step(DT);
        for c in sim.cues.drain(..) {
            if let Cue::Level { news: n, .. } = c {
                news.push(n);
            }
        }
    }
    assert_eq!(news[..7], [News::Faster, News::NewColor, News::Rocks, News::Faster, News::Stars, News::Faster, News::MoreRocks]);
    assert_eq!(sim.colors, START_COLORS + 1);
    assert!(sim.rocks_on && sim.stars_on);
    assert!(sim.drain() > DRAIN_START * 2.0);
}

#[test]
fn the_same_swaps_replay_the_same_round() {
    let run = || {
        let mut sim = Sim::new(12, false);
        let mut bot = Bot::new(DECENT, 3);
        let mut trace = Vec::new();
        for i in 0..3000 {
            if let Some(m) = bot.think(&sim, DT) {
                sim.swap(m.0, m.1);
            }
            // Uneven frames, as on a phone.
            sim.step(DT + if i % 7 == 0 { 0.004 } else { -0.0006 });
            sim.cues.clear();
            let ids: u64 = Board::cells().map(|c| sim.board.get(c).map_or(0, |p| p.id as u64 * (c.0 as u64 + 1) + p.color().unwrap_or(9) as u64)).sum();
            trace.push((sim.score, sim.energy.to_bits(), ids));
        }
        trace
    };
    assert_eq!(run(), run());
}

#[test]
fn the_demo_never_ends() {
    let mut sim = Sim::new(13, true);
    let mut wait = 0.0;
    for _ in 0..60 * 90 {
        if sim.ready() {
            wait += DT;
            if wait > 0.8 {
                let m = sim.hint.unwrap();
                sim.swap(m.0, m.1);
                wait = 0.0;
            }
        }
        sim.step(DT);
        sim.cues.clear();
    }
    assert!(!sim.over && sim.score > 500, "score {}", sim.score);
}

/// The first-timer preset's median round must sit in the target band
/// (engagement.md: 20–45 s). Fast enough for debug builds.
#[test]
fn a_first_timer_lasts_20_to_45_seconds() {
    let mut secs: Vec<f32> = (0..24).map(|s| play_round(FIRST_TIMER, 500 + s, 300.0).seconds).collect();
    let m = median(&mut secs);
    assert!((20.0..45.0).contains(&m), "first-timer median {m:.1}s");
}

/// Difficulty report (slow; run in release). Targets: a first-timer's round
/// lasts 20–45 s, a good player's 1–3 minutes.
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
            let r = play_round(skill, seed, 900.0);
            Outcome::new(r.seconds)
                .cause(format!("asleep in L{}", r.level))
                .stat("score", r.score as f32)
                .stat("swaps/min", r.swaps as f32 / r.seconds * 60.0)
                .stat("best cascade", r.best_chain as f32)
        });
    report.print();
    // What changed since the last run (committed: the last tuning that was kept).
    report.compare_with("bot-report.tsv");
    report.assert_skill_matters();
    // One traced first-timer round.
    let mut sim = Sim::new(1000, false);
    let mut bot = Bot::new(FIRST_TIMER, 1000 ^ 0x9e37_79b9);
    let mut next = 0.0;
    while !sim.over {
        if let Some(m) = bot.think(&sim, DT) {
            sim.swap(m.0, m.1);
        }
        sim.step(DT);
        for c in sim.cues.drain(..) {
            match c {
                Cue::Pop { chain, pieces } => print!("[{:.1}s pop {pieces}{}] ", sim.t, if chain > 1 { format!(" ×{chain}") } else { String::new() }),
                Cue::Bounce => print!("[{:.1}s bounce] ", sim.t),
                Cue::Level { level, news } => print!("[{:.1}s L{level} {news:?}] ", sim.t),
                Cue::Asleep => print!("[{:.1}s asleep] ", sim.t),
                _ => {}
            }
        }
        if sim.t >= next {
            print!("({:.0}% {:.0}s) ", sim.energy * 100.0, sim.t);
            next += 5.0;
        }
    }
    println!();
}

// ---- Mix ------------------------------------------------------------------------

/// A bot-played round through the real sound module, as an event log for
/// the skill's scripts/mix_check.py:
/// `MB_MIX_LOG=/tmp/m3.log cargo test -p kit_match3 mix_log` then
/// `python3 .claude/skills/maimbrain-game/scripts/mix_check.py <kit>/assets /tmp/m3.log`.
pub fn mix_round(skill: Skill, seed: u64, max_seconds: f32) -> String {
    let mut sim = Sim::new(seed, false);
    let mut bot = Bot::new(skill, seed ^ 0x9e37_79b9);
    let mut sfx = crate::sound::Sfx::new();
    sfx.log = Some(String::new());
    sfx.start();
    let mut tail = 3.0;
    while tail > 0.0 && sim.t < max_seconds {
        if !sim.over
            && let Some(m) = bot.think(&sim, DT)
        {
            sfx.select();
            if !sim.swap(m.0, m.1) {
                sfx.play_rock_poke();
            }
        }
        sim.step(DT);
        let cues = std::mem::take(&mut sim.cues);
        sfx.cues(&cues, true);
        sfx.tension(sim.drowsiness() * 1.4);
        let drowsy = sim.energy < DROWSY && !matches!(sim.phase, Phase::Asleep { .. });
        sfx.update(DT, drowsy);
        if sim.over {
            tail -= DT;
        }
    }
    sfx.log.take().unwrap()
}

#[test]
fn mix_log() {
    let log = mix_round(DECENT, 7, 75.0);
    assert!(log.lines().count() > 50);
    if let Ok(path) = std::env::var("MB_MIX_LOG") {
        std::fs::write(path, log).unwrap();
    }
}

// ---- Platform ---------------------------------------------------------------------

/// The platform's rules for a round (SKILL.md "Platform facts that bite"),
/// on a fake host: a tap starts it, the score goes in before Over, a frantic
/// tap doesn't restart, the best is kept, the session replays. A few blind
/// swipes in the first seconds (some match, so there's a best to save), then
/// nobody swipes and the board nods off.
#[test]
fn keeps_the_platform_rules() {
    use crate::look::{BOARD_X, BOARD_Y, CELL};
    use maimbrain::input::synth;
    maimbrain::sys::test::Conformance::new()
        .play(|t| {
            // Every half second for 6 s: a cell swiped into its right-hand neighbour.
            let i = (t / 0.5) as usize;
            if !(1..=12).contains(&i) || t - i as f64 * 0.5 >= DT as f64 {
                return Vec::new();
            }
            let (c, r) = (i % (COLS - 1), 2 + i / (COLS - 1));
            let (x, y) = (BOARD_X + (c as f32 + 0.5) * CELL, BOARD_Y + (r as f32 + 0.5) * CELL);
            synth::stroke(&[(x, y), (x + CELL, y)], 0.0, 0.1, synth::PANEL_HZ)
        })
        .check::<crate::Poppets>();
}
