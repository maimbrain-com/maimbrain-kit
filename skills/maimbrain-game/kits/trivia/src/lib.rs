//! Bright Spark: a fast quiz for the feed. Watt, a lightbulb quiz host,
//! asks; you tap one of four answers before the ring runs out. Faster
//! answers score more, streaks multiply, every fifth question is golden,
//! and the clock gets tighter every question. Three wrong and the bulb
//! blows.
//!
//! Layout: `sim` (rules, tested in `tests`), `questions` (the bank in
//! `questions.txt`), `look` (strings, palette, theme, category emblems),
//! `art` (the generated images), `draw` (the host, stage, card, buttons),
//! `sound` (cues → sound and haptics), and this file: input, juice and the
//! screens. The round flow itself (a tap on the title starting play, the
//! score, the bests, the game-over card, retrying) is
//! `maimbrain::rounds::Rounds`; `tests::keeps_the_platform_rules` checks it.

mod art;
mod draw;
mod look;
mod questions;
mod sim;
mod sound;
#[cfg(test)]
mod tests;

use maimbrain::input::{self, Event};
use maimbrain::juice::{Flash, Look, Particle, Particles, Popups, VignettePulse};
use maimbrain::motion::{Ease, HitStop, Pulse, Punch, Shake, stagger};
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::sensors::{self, Haptic};
use maimbrain::ui::{Button, ButtonKind, Hud, Icon, Layout, Rect, ResultsEvent, Tap, Theme, TitleCard, fade, pill, text, with_alpha};
use maimbrain::{Game, Rng, export_game, gfx2d, store, sys};

use draw::{Ans, Host, Mood};
use sim::{Cue, Phase, Sim};

const BOARD: u32 = 0;
const BOARD_DAILY: u32 = 1;
/// The how-to hint comes back when a player has been still this long on a
/// question and the clock is nearly out (reading isn't stalling; freezing is).
const HINT_IDLE: f32 = 3.0;
const HINT_LEFT: f32 = 2.5;
/// Seconds into the fail animation when the bulb pops.
const POP_AT: f32 = 0.8;
/// The title marquee: its top below the safe area's top, its width (its
/// height follows the image's shape), and how big Watt is on the title card.
const MARQUEE_Y: f32 = 8.0;
const MARQUEE_W: f32 = 280.0;
const TITLE_HOST: f32 = 1.4;
/// How big Watt is in play (1 = a glass of radius 34), peeking over the card's corner.
const PLAY_HOST: f32 = 1.2;
/// Room the marquee, tagline and best take at the top of the title card.
const TITLE_TOP: f32 = MARQUEE_Y + MARQUEE_W * art::MARQUEE.1 / art::MARQUEE.0 + 60.0;
/// The title card's loop, seconds: a card flips in, the clock drains, the
/// card turns over at ATTRACT_REVEAL to show the answer, the next one.
const ATTRACT_PERIOD: f32 = 3.8;
const ATTRACT_REVEAL: f32 = 2.2;
/// Set to e.g. 9 to start rounds at a later question (testing the hard end). Keep 0.
const DEBUG_SKIP: u32 = 0;

/// Where things sit, from the safe area (computed once).
#[derive(Clone, Copy)]
struct Geom {
    answers: [Rect; 4],
    card: Rect,
    host: (f32, f32),
    timer: (f32, f32),
    streak: (f32, f32),
}

impl Geom {
    fn new(l: &Layout) -> Geom {
        let s = l.safe;
        let (h, gap) = (50.0, 9.0);
        let bottom = s.bottom() - 14.0;
        let top = bottom - 4.0 * h - 3.0 * gap;
        let answers = std::array::from_fn(|i| Rect::new(s.x + 20.0, top + i as f32 * (h + gap), s.w - 40.0, h));
        let card_h = 128.0;
        let card = Rect::new(s.x + 18.0, top - 20.0 - card_h, s.w - 36.0, card_h);
        // Watt stands behind the card's top-left corner.
        let host = (s.x + 74.0, card.y - 56.0);
        Geom { answers, card, host, timer: (s.right() - 58.0, host.1 + 4.0), streak: (s.cx() + 6.0, host.1 + 6.0) }
    }

    /// The title card: a full-size question card in the card area (clear of
    /// the feed's overlays) with Watt big behind it and the clock beside
    /// him, centred between the title text and the DAILY button.
    /// Returns (card, Watt's glass centre, the clock's centre).
    fn title(l: &Layout) -> (Rect, (f32, f32), (f32, f32)) {
        let top = l.safe.y + TITLE_TOP;
        let bottom = l.card.bottom() - 58.0;
        // Watt's glass (radius 34 × TITLE_HOST) peeks over the card's top-left corner.
        let r = 34.0 * TITLE_HOST;
        let (above, card_h) = (r * 2.65, 128.0);
        let y0 = top + ((bottom - top) - above - card_h).max(0.0) / 2.0;
        let card = Rect::new(l.card.x, y0 + above, l.card.w, card_h);
        (card, (card.x + 52.0, card.y - r * 1.65), (card.right() - 34.0, card.y - 52.0))
    }
}

/// The title card's loop: which question is showing and how far along.
struct Attract {
    t: f32,
    q: questions::Question,
}

impl Attract {
    /// A short easy question, so the card reads at a glance.
    fn pick(rng: &mut Rng) -> Attract {
        let pool: Vec<&questions::Question> = questions::bank().iter().filter(|q| q.tier == 1 && q.text.len() <= 48).collect();
        let q = *pool[(rng.next_u32() as usize) % pool.len()];
        Attract { t: 0.0, q }
    }
}

struct Puff {
    x: f32,
    y: f32,
    age: f32,
    life: f32,
    size: f32,
}

struct Trivia {
    /// Title (the feed card: question cards flip by and Watt answers them),
    /// the round, the game-over card; the boards, the bests, daily mode.
    rounds: Rounds,
    sim: Sim,
    attract: Attract,
    rng: Rng,
    time: f32,
    idle: f32,
    answered_once: bool,
    /// How the last question was answered (for drawing through the ending).
    last: Option<(Option<u8>, bool)>,
    popped: bool,
    geom: Geom,
    theme: Theme,
    hud: Hud,
    /// The game-over card's buttons are the kit's own, in a row low in the
    /// card area (the card's retry sits above): Rounds never sees their taps.
    daily_btn: Button,
    scores_btn: Button,
    host: Host,
    art: art::Art,
    /// Seconds since the title card (re)appeared: its entrance.
    title_t: f32,
    fx: Particles,
    popups: Popups,
    shake: Shake,
    stop: HitStop,
    flash: Flash,
    hurt: VignettePulse,
    press: [Punch; 4],
    /// Seconds since each button was judged wrong (for its shake).
    jolt: [f32; 4],
    streak_punch: Punch,
    tick: Pulse,
    smoke: Vec<Puff>,
    sound: sound::Sound,
}

fn haptic_tap() {
    sensors::haptic(Haptic::Tap);
}

impl Trivia {
    /// The round's sim, on the round's seed (today's in daily mode).
    fn new_sim(&self) -> Sim {
        let mut s = Sim::new(self.rounds.seed(), self.rounds.is_daily());
        for _ in 0..DEBUG_SKIP {
            while s.phase != Phase::Ask {
                s.step(0.05);
            }
            s.answer(s.cur.right_slot());
            while s.phase != Phase::Deal {
                s.step(0.05);
            }
            s.cues.clear();
        }
        s
    }

    /// A round has started (`Input::Start`: a tap on the title or the card).
    fn start(&mut self) {
        self.sim = self.new_sim();
        self.hud.reset(self.rounds.best());
        self.popped = false;
        self.last = None;
        self.idle = 0.0;
        self.smoke.clear();
        self.host = Host::new();
        self.host.rest = Mood::Think;
        self.sound.start();
        // The tap that started the round deals the first card at once: that
        // flip (and the jingle) is its answer, the warm-up question its first move.
        let cues = std::mem::take(&mut self.sim.cues);
        self.react(&cues);
        self.sound.cues(&cues);
    }

    /// A tap during play: which answer, if any.
    fn tap(&mut self, x: f32, y: f32) {
        self.idle = 0.0;
        if self.sim.phase != Phase::Ask {
            return;
        }
        for (i, r) in self.geom.answers.iter().enumerate() {
            if r.inset(-4.0).contains(x, y) && self.sim.answer(i as u8) {
                self.answered_once = true;
                self.press[i].kick(-0.08);
                self.sound.play(sound::TAP, 0.5, 1.0);
                return;
            }
        }
    }

    /// DAILY, on the title or the game-over card: switches mode (its own
    /// seed, board and best) and goes (back) to the title.
    fn toggle_daily(&mut self) {
        let daily = !self.rounds.is_daily();
        self.rounds.set_daily(daily);
        self.sound.play(sound::UI, 0.5, if daily { 1.12 } else { 1.0 });
        self.daily_btn.kind = if daily { ButtonKind::Primary } else { ButtonKind::Secondary };
        self.rounds.title.tagline = Some(if daily { look::DAILY_TAGLINE } else { look::TAGLINE }.to_string());
        self.title_t = 0.0;
        self.sound.stop_music();
        // Back to a lit, smiling host for the card.
        self.host = Host::new();
        self.popped = false;
        self.smoke.clear();
    }

    /// Juice for the sim's cues (sounds and haptics are `sound`'s).
    fn react(&mut self, cues: &[Cue]) {
        let g = self.geom;
        let th = self.theme;
        let golden = self.sim.cur.golden;
        let lives = self.sim.lives;
        for &c in cues {
            match c {
                Cue::Deal { golden, .. } => {
                    self.press = [Punch::new(); 4];
                    self.jolt = [9.0; 4];
                    if golden {
                        self.host.react(Mood::Excited, 0.9);
                        self.fx.sparkles(g.card.cx(), g.card.cy(), 120.0, 16, 0xfff2b0ff);
                    }
                }
                Cue::Ask => {
                    self.host.rest = Mood::Think;
                    self.idle = 0.0;
                }
                Cue::Tick { .. } => self.tick.fire(),
                Cue::Right { slot, points, streak, fast } => {
                    let r = g.answers[slot as usize];
                    self.press[slot as usize].kick(0.22);
                    self.host.react(if streak >= 5 || golden { Mood::Excited } else { Mood::Happy }, 0.85);
                    self.fx.sparkles(r.cx(), r.cy(), 70.0, 12, 0xfff2b0ff);
                    self.fx.ring(r.x + 25.0, r.cy(), 60.0, with_alpha(look::RIGHT, 0.9));
                    self.fx.stars(self.geom.host.0, self.geom.host.1 - 30.0, 5 + streak.min(8) as usize, th.gold);
                    self.popups.score(r.right() - 50.0, r.y - 4.0, points as i64, th.gold);
                    if fast {
                        self.popups.spawn(r.right() - 130.0, r.y - 2.0, "QUICK!", 0xffffffff, 18.0);
                    }
                    self.hud.set_score(self.sim.score as i64);
                    self.shake.add(0.12);
                    self.flash.fire(0xfff3c030);
                }
                Cue::Wrong { slot } => {
                    self.jolt[slot as usize] = 0.0;
                    self.host.react(Mood::Sad, 1.4);
                    let r = g.answers[slot as usize];
                    self.fx.burst(r.x + 25.0, r.cy(), 14, look::WRONG);
                    self.shake.add(0.38);
                    self.stop.freeze(0.08);
                    self.hurt.fire();
                }
                Cue::Timeout => {
                    self.host.react(Mood::Sad, 1.4);
                    self.popups.spawn(g.timer.0, g.timer.1 + 44.0, look::TIME_UP, look::WRONG, 26.0);
                    self.shake.add(0.3);
                    self.hurt.fire();
                }
                Cue::LifeLost { left } => {
                    let (x, y) = self.heart_pos(left);
                    self.fx.burst(x, y, 16, look::WRONG);
                    self.fx.sparks(x, y, std::f32::consts::FRAC_PI_2, 2.5, 12, look::WRONG);
                }
                Cue::MultUp { mult } => {
                    self.streak_punch.kick(0.55);
                    let (x, y) = g.streak;
                    self.popups.spawn(x, y - 34.0, &format!("×{mult} STREAK!"), th.gold, 24.0);
                    self.fx.stars(x, y, 8, th.gold);
                }
                Cue::HeartBack => {
                    let (x, y) = self.heart_pos(lives - 1);
                    self.fx.sparkles(x, y, 30.0, 10, look::WRONG);
                    self.popups.spawn(x - 10.0, y + 26.0, "+♥", look::WRONG, 24.0);
                }
                Cue::Ending => {
                    self.host.rest = Mood::Sad;
                    self.host.react(Mood::Sad, 9.0);
                }
                _ => {}
            }
        }
    }

    /// Where the HUD draws heart `i` (0-based from the left).
    fn heart_pos(&self, i: u32) -> (f32, f32) {
        let (hs, max) = (22.0, sim::LIVES);
        let hud = self.rounds.layout.hud;
        (hud.right() - 16.0 - (max - 1 - i.min(max - 1)) as f32 * (hs + 4.0) - hs / 2.0, hud.y + 26.0)
    }

    /// The bulb blows: glass, sparks, smoke, a thump.
    fn pop(&mut self) {
        sys::log(sys::Level::Info, "trivia: pop");
        self.popped = true;
        self.host.cracked = true;
        self.host.lit = 0.0;
        self.host.react(Mood::Dead, 99.0);
        self.host.rest = Mood::Dead;
        let (x, y) = self.geom.host;
        for i in 0..18 {
            let mut p = Particle::new(Look::Confetti, x, y - 10.0, if i % 3 == 0 { 0xffffffd0 } else { 0xcfe6ffc0 });
            let a = self.fx.rng.range(0.0, std::f32::consts::TAU);
            let v = self.fx.rng.range(120.0, 320.0);
            (p.vx, p.vy) = (a.cos() * v, a.sin() * v - 120.0);
            p.gravity = 700.0;
            p.drag = 0.8;
            p.life = self.fx.rng.range(0.8, 1.4);
            p.size = self.fx.rng.range(3.0, 6.0);
            p.size_end = p.size;
            p.spin = self.fx.rng.range(-12.0, 12.0);
            p.flip_rate = self.fx.rng.range(6.0, 14.0);
            self.fx.add(p);
        }
        self.fx.sparks(x, y - 10.0, -std::f32::consts::FRAC_PI_2, 3.0, 26, look::GLOW);
        self.fx.ring(x, y, 90.0, 0xffffffd0);
        self.shake.add(0.85);
        self.flash.fire(0xffffffa0);
        self.sound.play(sound::POP, 0.55, 1.0);
        sensors::haptic(Haptic::Heavy);
    }

    /// The bulb has blown (the sim's own ending, played while still
    /// Playing): Rounds submits the score, saves the best, shows the card
    /// and reports Over, this frame.
    fn game_over(&mut self) {
        let score = self.sim.score as i64;
        let daily = self.rounds.is_daily();
        sys::log(sys::Level::Info, &format!("trivia: game over at {score} ({} right, daily: {daily})", self.sim.right));
        let prev = self.rounds.best();
        self.sound.verdict(score > prev && prev > 0);
        self.rounds.end(score).label(if daily { look::DAILY_LABEL } else { look::SCORE_LABEL });
        self.daily_btn.reset();
        self.scores_btn.reset();
    }

    /// The title card's loop: flips, a cheer when the answer shows.
    fn attract(&mut self, dt: f32) {
        let before = self.attract.t;
        self.attract.t += dt;
        let t = self.attract.t;
        if before < ATTRACT_REVEAL && t >= ATTRACT_REVEAL {
            self.host.react(Mood::Happy, 1.2);
            let (card, _, _) = Geom::title(&self.rounds.layout);
            self.fx.sparkles(card.cx(), card.cy(), 110.0, 14, 0xfff2b0ff);
            self.sound.play(sound::RIGHT, 0.16, 1.0);
        }
        if t >= ATTRACT_PERIOD {
            self.attract = Attract::pick(&mut self.rng);
            self.sound.play(sound::FLIP, 0.18, 1.0);
        }
        let asking = self.attract.t < ATTRACT_REVEAL;
        self.host.rest = if asking { Mood::Think } else { Mood::Idle };
        self.host.look_target = if asking { ((self.time * 0.8).sin() * 0.5, 0.8) } else { (0.3, 0.2) };
    }

    /// The kit's own buttons see an event before Rounds does: DAILY on the
    /// title; DAILY and SCORES on the game-over card once it takes taps
    /// (past its guard). True if one took it.
    fn ui_event(&mut self, e: &Event) -> bool {
        match self.rounds.stage() {
            Stage::Title => match self.daily_btn.handle(e) {
                Some(Tap::Clicked) => {
                    self.toggle_daily();
                    true
                }
                Some(_) => true,
                None => false,
            },
            Stage::Over if self.rounds.card_ready() => {
                if let Some(tp) = self.daily_btn.handle(e) {
                    if tp == Tap::Clicked {
                        self.toggle_daily();
                    }
                    return true;
                }
                if let Some(tp) = self.scores_btn.handle(e) {
                    if tp == Tap::Clicked {
                        self.sound.play(sound::UI, 0.5, 1.0);
                        store::show_scores(if self.rounds.is_daily() { BOARD_DAILY } else { BOARD });
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

impl Game for Trivia {
    fn init() -> Self {
        let theme = look::theme();
        // The sim plays its own ending (the reveal, the flicker, the pop)
        // before it reports over, so Rounds shows the card at once.
        let rounds = Rounds::new(TitleCard::new(look::TITLE, theme).tagline(look::TAGLINE))
            .board(BOARD)
            .daily(BOARD_DAILY)
            .heading(look::HEADING)
            .label(look::SCORE_LABEL)
            .floating()
            .ending(0.0);
        let geom = Geom::new(&rounds.layout);
        let mut rng = Rng::from_host();
        let sim = Sim::new(rng.next_u32() as u64, false);
        let attract = Attract::pick(&mut rng);
        // SCORES and DAILY sit low in the card area, clear of the feed's overlays.
        let c = rounds.layout.card;
        let row = Rect::new(c.x, c.bottom() - 46.0, c.w, 44.0).columns(2, 12.0);
        let mut hud = Hud::new(theme, rounds.best());
        hud.size = 44.0;
        // Hearts are drawn by `hearts` (on a paper strip, to read on the curtain).
        hud.lives = None;
        let mut fx = Particles::new(rng.next_u32() as u64);
        fx.cap = 400;
        let mut popups = Popups::new();
        popups.outline = Some(look::INK);
        let mut shake = Shake::new();
        shake.max_offset = 10.0;
        Trivia {
            rounds,
            sim,
            attract,
            time: 0.0,
            idle: 0.0,
            answered_once: false,
            last: None,
            popped: false,
            hud,
            daily_btn: Button::new(Rect::centered(c.cx(), c.bottom() - 24.0, 140.0, 44.0), "DAILY").kind(ButtonKind::Secondary).with_icon(Icon::STAR).with_haptic(haptic_tap),
            scores_btn: Button::new(row[0], "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY).with_haptic(haptic_tap),
            host: Host::new(),
            art: art::Art::new(),
            title_t: 0.0,
            fx,
            popups,
            shake,
            stop: HitStop::default(),
            flash: Flash::new(0.35),
            hurt: {
                let mut v = VignettePulse::new(look::WRONG);
                v.size = 80.0;
                v
            },
            press: [Punch::new(); 4],
            jolt: [9.0; 4],
            streak_punch: Punch::new(),
            tick: Pulse::new(0.35),
            smoke: Vec::new(),
            sound: sound::Sound::new(),
            geom,
            theme,
            rng,
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.idle += dt;
        self.title_t += dt;
        self.art.update();
        self.sound.update(dt);
        for e in input::poll() {
            if self.ui_event(&e) {
                continue;
            }
            match self.rounds.input(&e) {
                // The tap that enters play starts the round (SPEC §5.2).
                Input::Start => self.start(),
                Input::Play if e.is_press() => self.tap(e.x, e.y),
                _ => {}
            }
        }
        // Where the daily button sits depends on the screen.
        let title = self.rounds.stage() == Stage::Title;
        let c = self.rounds.layout.card;
        let row = Rect::new(c.x, c.bottom() - 46.0, c.w, 44.0).columns(2, 12.0);
        self.daily_btn.rect = if title { Rect::centered(c.cx(), c.bottom() - 24.0, 140.0, 44.0) } else { row[1] };

        match self.rounds.stage() {
            Stage::Title => self.attract(dt),
            Stage::Playing => {
                let sdt = self.stop.step(dt);
                self.sim.step(sdt);
                let cues = std::mem::take(&mut self.sim.cues);
                self.react(&cues);
                self.sound.cues(&cues);
                if let Phase::Reveal { picked, right } = self.sim.phase {
                    self.last = Some((picked, right));
                }
                match self.sim.phase {
                    Phase::Ask if self.sim.danger() > 0.35 => self.host.rest = Mood::Nervous,
                    Phase::Ask => self.host.rest = Mood::Think,
                    _ => {}
                }
                if self.sim.phase == Phase::Ending && !self.popped && self.sim.phase_t >= POP_AT {
                    self.pop();
                }
                if self.sim.over {
                    self.game_over();
                }
            }
            Stage::Ending | Stage::Over => {}
        }
        // The card's roll-up landing on a new best gets its sting.
        if self.rounds.update(dt) == Some(ResultsEvent::NewBest) {
            self.sound.new_best();
        }
        let play = self.rounds.playing();

        // Watt looks at what matters: the answers while asked, the clock when it's tight.
        let sim = &self.sim;
        if !title && matches!(self.host.mood, Mood::Think | Mood::Nervous | Mood::Idle) {
            self.host.look_target = if sim.danger() > 0.35 {
                (0.9, -0.2)
            } else if sim.phase == Phase::Ask {
                ((self.time * 0.7).sin() * 0.6, 0.75)
            } else {
                ((self.time * 0.4).sin() * 0.4, 0.0)
            };
        }
        let mult = if title { 1 } else { sim.multiplier() };
        self.host.glow_target = 0.15 + 0.22 * (mult - 1) as f32;
        let danger = if play { self.sim.danger() } else { 0.0 };
        self.sound.hi_target = if play { danger.max(0.25 * (mult - 1) as f32).min(1.0) } else { 0.0 };
        // The stopwatch ticks in under the band as the clock runs down (only while asking).
        self.sound.tension_target = if play && self.sim.phase == Phase::Ask {
            let frac = self.sim.time_left() / self.sim.cur.limit.max(0.1);
            ((0.6 - frac) / 0.45).clamp(0.0, 1.0).max(danger)
        } else {
            0.0
        };

        // Smoke from a blown bulb keeps curling up on the results card.
        if self.popped {
            let (x, y) = self.smoke_origin();
            if self.smoke.len() < 14 && (self.time * 7.0).fract() < (dt * 7.0) {
                let jitter = (self.time * 12.9898).sin() * 6.0;
                self.smoke.push(Puff { x: x + jitter, y, age: 0.0, life: 2.2, size: 12.0 });
            }
        }
        for p in &mut self.smoke {
            p.age += dt;
            p.y -= 26.0 * dt;
            p.x += (p.age * 2.0 + p.size).sin() * 8.0 * dt;
        }
        self.smoke.retain(|p| p.age < p.life);

        self.host.update(dt);
        self.hud.update(dt);
        self.daily_btn.update(dt);
        self.scores_btn.update(dt);
        self.fx.update(dt);
        self.popups.update(dt);
        self.shake.update(dt);
        self.flash.update(dt);
        self.hurt.update(dt);
        self.tick.update(dt);
        self.streak_punch.update(dt);
        for p in &mut self.press {
            p.update(dt);
        }
        for j in &mut self.jolt {
            *j += dt;
        }
    }

    fn render(&self) {
        let l = &self.rounds.layout;
        let screen = self.rounds.stage();
        let play = self.rounds.playing();
        let heat = if play { ((self.sim.multiplier() - 1) as f32 / 3.0).min(1.0) } else { 0.0 };
        let alarm = if play { self.sim.danger() * 0.7 } else { 0.0 };
        draw::backdrop(l, self.art.stage(), self.time, heat, alarm);
        if screen == Stage::Over {
            // Lights out: the studio dims behind the results.
            maimbrain::ui::shape::rrect(l.screen, 0.0, with_alpha(look::INK, 0.5 * (self.rounds.age() / 0.6).min(1.0)));
        }
        gfx2d::push();
        self.shake.apply(l.screen.cx(), l.screen.cy());
        match screen {
            Stage::Title => self.title_scene(),
            Stage::Playing | Stage::Ending => self.scene(&self.sim),
            Stage::Over => {
                // The aftermath: the blown bulb, smoking.
                let (x, y) = self.over_host();
                self.host.draw(self.art.host(), x, y, 1.5);
                for p in &self.smoke {
                    draw::puff(p.x, p.y, p.age, p.life, p.size);
                }
            }
        }
        self.fx.draw();
        if play {
            for p in &self.smoke {
                draw::puff(p.x, p.y, p.age, p.life, p.size);
            }
        }
        gfx2d::pop();
        self.popups.draw();
        match screen {
            Stage::Title => {
                self.title_text();
                self.button(&self.daily_btn);
            }
            Stage::Playing | Stage::Ending => {
                self.hud.draw(l);
                self.hearts();
                if self.rounds.is_daily() {
                    let z = l.pill_zone();
                    pill("DAILY", z.x + 52.0, z.bottom() + 16.0, 24.0, with_alpha(look::INK, 0.6), self.theme.gold, gfx2d::Font::SansBold, Some(Icon::STAR));
                }
                self.hint();
            }
            Stage::Over => {
                // The game-over card only: the title is the kit's own, drawn above
                // (so the stock title card isn't linked in).
                self.rounds.draw_card();
                let (x, y) = self.over_host();
                let line = format!("{} RIGHT · BEST STREAK {}", self.sim.right, self.sim.best_streak);
                text(&line).size(16.0).color(0xffffffe0).outline(0.1, look::INK).middle().draw(x, y + 104.0);
                self.button(&self.scores_btn);
                self.button(&self.daily_btn);
            }
        }
        self.flash.draw(l.screen);
        self.hurt.draw(l.screen);
    }

    fn suspend(&mut self) {
        self.sound.stop_music();
    }

    fn resume(&mut self) {
        if self.rounds.playing() && !matches!(self.sim.phase, Phase::Ending | Phase::Over) {
            self.sound.start_music();
        }
    }
}

impl Trivia {
    fn over_host(&self) -> (f32, f32) {
        let c = self.rounds.layout.card;
        let top = c.y + 24.0 + 200.0 + 60.0;
        let bottom = c.bottom() - 56.0 - 110.0;
        (c.cx(), (top + bottom) / 2.0)
    }

    fn smoke_origin(&self) -> (f32, f32) {
        match self.rounds.stage() {
            Stage::Over => {
                let (x, y) = self.over_host();
                (x, y - 50.0)
            }
            _ => (self.geom.host.0, self.geom.host.1 - 34.0),
        }
    }

    /// The title: the name lettered on the marquee, the tagline, the best.
    fn title_text(&self) {
        let l = &self.rounds.layout;
        let t = self.title_t;
        let enter = Ease::BackOut.at((t / 0.5).min(1.0));
        let sign = draw::marquee(self.art.marquee(), &look::TITLE_LINES, l.screen.cx(), l.safe.y + MARQUEE_Y - (1.0 - enter) * 160.0, MARQUEE_W, t, &self.theme);
        if let Some(tag) = &self.rounds.title.tagline {
            let a = Ease::QuadOut.at(((t - 0.45) / 0.4).clamp(0.0, 1.0));
            text(tag).size(18.0).color(look::ON_STAGE).outline(0.12, look::INK).shadow(0.0, 2.0, with_alpha(look::INK, 0.7)).alpha(a).middle().draw(l.screen.cx(), sign.bottom() + 15.0 + (1.0 - a) * 8.0);
        }
        if let Some(b) = self.rounds.title.best {
            let k = ((t - 0.7) / 0.5).clamp(0.0, 1.0);
            if k > 0.0 && b > 0 {
                let s = Ease::BackOut.at(k);
                gfx2d::push();
                gfx2d::translate(l.screen.cx(), sign.bottom() + 43.0);
                gfx2d::scale(s, s);
                let label = format!("BEST {}", maimbrain::ui::fmt_int(b));
                let w = maimbrain::ui::pill_width(&label, 30.0, gfx2d::Font::SansBold, true);
                maimbrain::ui::shape::rrect(Rect::centered(0.0, 0.0, w + 5.0, 35.0), 17.5, look::INK);
                pill(&label, 0.0, 0.0, 30.0, look::MUSTARD, look::INK, gfx2d::Font::SansBold, Some(Icon::TROPHY));
                gfx2d::pop();
            }
        }
    }

    /// The lives: hearts on an ink-rimmed paper strip, top-right.
    fn hearts(&self) {
        use maimbrain::ui::shape;
        let (x0, y) = self.heart_pos(0);
        let (x1, _) = self.heart_pos(sim::LIVES - 1);
        let strip = Rect::new(x0 - 17.0, y - 15.0, x1 - x0 + 34.0, 30.0);
        shape::rrect(strip.offset(look::DROP.0 * 0.6, look::DROP.1 * 0.6).inset(-2.5), 17.5, with_alpha(look::INK, 0.55));
        shape::rrect(strip.inset(-2.5), 17.5, look::INK);
        shape::rrect(strip, 15.0, look::PAPER);
        for i in 0..sim::LIVES {
            let (x, y) = self.heart_pos(i);
            let on = i < self.sim.lives;
            Icon::HEART.draw(x, y + 0.5, 23.0, if on { look::INK } else { with_alpha(look::INK, 0.25) });
            Icon::HEART.draw(x, y, 19.0, if on { look::WRONG } else { look::PAPER_SHADE });
        }
    }

    /// A UI kit button with the print look's ink rim behind it.
    fn button(&self, b: &Button) {
        maimbrain::ui::shape::rrect(b.rect.offset(look::DROP.0 * 0.6, look::DROP.1 * 0.6).inset(-2.5), self.theme.radius * 0.6 + 2.5, with_alpha(look::INK, 0.55));
        maimbrain::ui::shape::rrect(b.rect.inset(-2.5), self.theme.radius * 0.6 + 2.5, look::INK);
        b.draw(&self.theme);
    }

    /// The title card's loop: Watt, the clock, a card that turns over.
    fn title_scene(&self) {
        let (card, (hx, hy), (tx, ty)) = Geom::title(&self.rounds.layout);
        let t = self.attract.t;
        self.host.draw(self.art.host(), hx, hy, TITLE_HOST);
        // A 5-second clock that drains until Watt "answers".
        let used = ((t.min(ATTRACT_REVEAL) - 0.4).max(0.0)) * 1.6;
        draw::timer(tx, ty, 5.0 - used, 5.0, 0.0, 1.0);
        let turn = 0.15;
        if t < ATTRACT_REVEAL + turn {
            let flip = if t < ATTRACT_REVEAL { Ease::BackOut.at((t / 0.4).min(1.0)) } else { 1.0 - (t - ATTRACT_REVEAL) / turn };
            draw::card(card, &self.attract.q, "", false, flip, self.time, self.art.cats(), self.art.burst());
        } else {
            let open = ((t - ATTRACT_REVEAL - turn) / turn).min(1.0);
            let close = ((t - (ATTRACT_PERIOD - 0.25)) / 0.25).clamp(0.0, 1.0);
            draw::card_back(card, self.attract.q.answers[0], open * (1.0 - close));
        }
    }

    /// Watt, the clock, the streak, the card and the answers for `sim`.
    fn scene(&self, sim: &Sim) {
        let g = &self.geom;
        let d = &sim.cur;
        let t = sim.phase_t;
        let ending = matches!(sim.phase, Phase::Ending | Phase::Over);
        // How far the card has turned in (and, at the end of a reveal, out).
        let flip = match sim.phase {
            Phase::Deal => Ease::BackOut.at((t / (sim::DEAL_TIME * 0.7)).min(1.0)),
            Phase::Reveal { right, .. } => {
                let hold = if right { sim::REVEAL_RIGHT } else { sim::REVEAL_WRONG };
                1.0 - Ease::QuadIn.at(((t - (hold - 0.16)) / 0.16).clamp(0.0, 1.0))
            }
            _ => 1.0,
        };
        let (hx, hy) = g.host;
        self.host.draw(self.art.host(), hx, hy, PLAY_HOST);
        let next = sim::MULT_STEPS.iter().copied().find(|&s| s > sim.streak);
        let prev = sim::MULT_STEPS.iter().copied().filter(|&s| s <= sim.streak).max().unwrap_or(0);
        draw::streak(self.art.burst(), g.streak.0, g.streak.1, sim.streak, sim.multiplier(), next, prev, self.streak_punch.scale(), self.time);
        let tag = if d.golden { look::GOLDEN.to_string() } else if d.warmup() { look::WARMUP.to_string() } else { format!("Q{}", d.index + 1) };
        draw::card(g.card, &d.q, &tag, d.golden, flip, self.time, self.art.cats(), self.art.burst());
        // The clock over the card, so a golden card's starburst rays out behind it.
        if !ending {
            let pulse = self.tick.value();
            draw::timer(g.timer.0, g.timer.1, sim.time_left(), d.limit, pulse, 1.0);
        }
        // Answers: slide in staggered while the card turns, then live, then judged.
        for i in 0..4 {
            let r = g.answers[i];
            let label = d.answer(i);
            let (mut st, mut k, mut dx, mut alpha) = (Ans::Plain, 0.0, 0.0, 1.0);
            match sim.phase {
                Phase::Deal => {
                    let e = stagger(t, i, 0.06, 0.3);
                    dx = (1.0 - Ease::CubicOut.at(e)) * 70.0;
                    alpha = e;
                }
                Phase::Ask => {}
                _ => {
                    if let Some((picked, right)) = self.last {
                        let right_slot = d.right_slot() as usize;
                        st = if picked == Some(i as u8) {
                            if right { Ans::Right } else { Ans::Wrong }
                        } else if i == right_slot && !right {
                            Ans::Revealed
                        } else {
                            Ans::Dim
                        };
                        k = t;
                    }
                    alpha = flip;
                    let j = self.jolt[i];
                    if j < 0.45 {
                        dx = (j * 60.0).sin() * 9.0 * (1.0 - j / 0.45);
                    }
                }
            }
            draw::answer(r, i, label, st, k, self.press[i].scale(), dx, alpha);
        }
    }

    /// Teaches the verb: a finger tapping the answers in turn, until the
    /// first answer, and again whenever the player stalls.
    fn hint(&self) {
        let stalled = self.idle > HINT_IDLE && self.sim.time_left() < HINT_LEFT;
        if self.sim.phase != Phase::Ask || (self.answered_once && !stalled) {
            return;
        }
        let a = if self.answered_once { 0.7 } else { 1.0 } * ((self.sim.phase_t - 0.3) / 0.3).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let cycle = self.time * 1.2;
        let i = (cycle as usize) % 4;
        let ph = cycle.fract();
        let r = self.geom.answers[i];
        let press = if ph < 0.5 { 0.0 } else { ((ph - 0.5) / 0.35).min(1.0) };
        draw::finger(r.right() - 46.0, r.cy() + 4.0, press, a * 0.95);
        if !self.answered_once {
            let c = self.geom.card;
            let w = text(look::HINT).size(16.0).measure().0 + 28.0;
            let bob = (self.time * 5.0).sin() * 2.0;
            maimbrain::ui::shape::rrect(Rect::centered(c.cx(), c.bottom() + bob, w, 28.0), 14.0, fade(look::INK, 0.92 * a));
            text(look::HINT).size(16.0).color(fade(look::GLOW, a)).middle().draw(c.cx(), c.bottom() + bob);
        }
    }
}

export_game!(Trivia);
