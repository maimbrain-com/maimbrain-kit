//! BUN RUN, the Maimbrain runner kit: a bun in red sneakers runs by itself;
//! tap to jump, hold to jump higher, tap again in the air to double jump.
//! Crates, pits, tall stacks, hungry gulls, bees, skimming gulls and logs
//! join every 15 s, the run speeds up every 12 s, coins arc over the ideal
//! jumps, and the sky goes day → dusk → night with distance. Daily mode runs
//! the same course for everyone today.
//!
//! This file is the thin host layer: input, juice, drawing order. The round
//! itself (the title card, a tap starting play, the score, the best, the
//! game-over card, retrying, daily mode's seed, board and best) is
//! `maimbrain::rounds::Rounds`. The rules are in `sim.rs` (tuning knobs at
//! the top), everything you'd reskin is in `look.rs`, sounds and haptics in
//! `sound.rs`. `cargo test -p kit_runner` runs the rules, a bot and the
//! platform's rules for a round (`keeps_the_platform_rules`).

mod look;
mod sim;
mod sound;
mod sprites;
#[cfg(test)]
mod tests;

use std::f32::consts::TAU;

use maimbrain::gfx2d::{self, Font};
use maimbrain::input::{self, Event, Kind as Ev};
use maimbrain::juice::{Flash, Particles, Popups};
use maimbrain::motion::{Ease, HitStop, Punch, Shake};
use maimbrain::sensors::{self, Haptic};
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::sys::{self, Level};
use maimbrain::ui::{self, Button, ButtonKind, Countdown, Hud, Icon, Rect, ResultsEvent, Tap, Theme, TitleCard, text};
use maimbrain::{Game, Rng, export_game, store};

use look::{Art, Face, GROUND_Y, Pose, W};
use sim::{Autopilot, Cause, Cue, HERO_X, JUMP_VY, Kind, MAX_SPEED, PX_PER_M, START_SPEED, Sim};
use sound::Sound;

const BOARD: u32 = 0;
const BOARD_DAILY: u32 = 1;
/// The tap hint comes back after this long without a tap (s).
const HINT_IDLE: f32 = 3.5;
/// A CLOSE! call slows time to this for this long (s): savour the escape.
const SLOWMO: f32 = 0.35;
const SLOWMO_TIME: f32 = 0.3;
/// Debug: an autopilot plays the round until this many seconds (0 = off).
/// Handy for screenshots of later obstacle families. Keep it 0.
const DEBUG_AUTOPLAY_UNTIL: f32 = 0.0;

/// A dust puff (alpha-blended, unlike the kit's additive dots).
struct Puff {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    r: f32,
}

struct Runner {
    rounds: Rounds,
    /// The round (on the game-over card: its aftermath).
    sim: Sim,
    /// The title card's run, playing itself.
    attract: Sim,
    pilot: Autopilot,
    rng: Rng,
    time: f32,
    play_t: f32,
    idle: f32,
    /// The player has jumped on purpose (after the tap that started the round).
    taught: bool,
    fingers: u32,
    theme: Theme,
    hud: Hud,
    daily_btn: Button,
    scores_btn: Button,
    /// Where DAILY sits on the title card (centred) and on the results card (beside SCORES).
    daily_title: Rect,
    daily_over: Rect,
    countdown: Countdown,
    fx: Particles,
    popups: Popups,
    puffs: Vec<Puff>,
    shake: Shake,
    hitstop: HitStop,
    flash: Flash,
    coin_punch: Punch,
    /// A new family's banner: which, and its age (s).
    banner: Option<(Kind, f32)>,
    faster_t: f32,
    slowmo: f32,
    sound: Sound,
    /// The generated pixel art (loads in the background).
    art: Art,
}

fn screen_x(sim: &Sim, x: f32) -> f32 {
    HERO_X + (x - sim.dist)
}

fn tap_haptic() {
    sensors::haptic(Haptic::Tap);
}

impl Runner {
    fn shown(&self) -> &Sim {
        if self.rounds.stage() == Stage::Title { &self.attract } else { &self.sim }
    }

    fn new_attract(&mut self) {
        let seed = self.rng.next_u32() as u64;
        self.attract = Sim::new(seed);
        self.pilot = Autopilot::new(seed ^ 0xa77, 0.05);
        // Skip the warm-up: the card should show jumps, not open ground.
        for _ in 0..170 {
            self.pilot.drive(&mut self.attract);
            self.attract.step(1.0 / 60.0);
        }
        self.attract.cues.clear();
    }

    /// A round starts (`Rounds` has reported Playing).
    fn start(&mut self) {
        if DEBUG_AUTOPLAY_UNTIL > 0.0 {
            self.pilot = Autopilot::new(7, 0.02);
        }
        // Daily mode: `seed()` is today's, the same course for everyone.
        self.sim = Sim::new(self.rounds.seed());
        self.hud.reset(self.rounds.best());
        self.play_t = 0.0;
        self.idle = 0.0;
        self.taught = DEBUG_AUTOPLAY_UNTIL > 0.0;
        self.banner = None;
        self.slowmo = 0.0;
        self.faster_t = 9.0;
        self.fx.clear();
        self.popups.list.clear();
        self.puffs.clear();
        self.sound.start();
    }

    /// A touch for the round (a press comes here only as a move: the one that
    /// starts a round, then presses in play).
    fn touch(&mut self, e: &Event) {
        match e.kind() {
            Ev::TouchDown | Ev::MouseDown => self.down(),
            Ev::TouchUp | Ev::MouseUp | Ev::TouchCancel => self.up(),
            _ => {}
        }
    }

    fn down(&mut self) {
        self.fingers += 1;
        if self.play_t > 0.4 {
            self.taught = true;
        }
        self.idle = 0.0;
        self.sim.press();
    }

    fn up(&mut self) {
        self.fingers = self.fingers.saturating_sub(1);
        if self.fingers == 0 {
            self.sim.release();
        }
    }

    /// Offers an event to the kit's own buttons (DAILY, SCORES); true if one
    /// took it. They come before `Rounds`, which has the rest of the cards' input.
    fn ui_event(&mut self, e: &Event) -> bool {
        match self.rounds.stage() {
            Stage::Playing | Stage::Ending => false,
            Stage::Title => match self.daily_btn.handle(e) {
                Some(Tap::Clicked) => {
                    self.toggle_daily();
                    true
                }
                Some(_) => true,
                None => false,
            },
            Stage::Over => {
                // Like the card's own button, nothing during its guard against
                // a frantic tap (`Rounds` takes those).
                if !self.rounds.card_ready() {
                    return false;
                }
                if let Some(tap) = self.daily_btn.handle(e) {
                    if tap == Tap::Clicked {
                        self.toggle_daily();
                    }
                    return true;
                }
                if let Some(tap) = self.scores_btn.handle(e) {
                    if tap == Tap::Clicked {
                        self.sound.ui(false);
                        store::show_scores(if self.rounds.is_daily() { BOARD_DAILY } else { BOARD });
                    }
                    return true;
                }
                false
            }
        }
    }

    /// Daily on or off; from the game-over card, back to the title card
    /// (`Rounds` switches the seed, the board and the best).
    fn toggle_daily(&mut self) {
        let on = !self.rounds.is_daily();
        self.rounds.set_daily(on);
        self.sound.ui(on);
        self.daily_btn.kind = if on { ButtonKind::Primary } else { ButtonKind::Secondary };
        self.daily_btn.rect = self.daily_title;
    }

    /// The tumble is over (the sim plays it, so `Rounds` has no ending of
    /// its own): this round's score and heading. `Rounds` then submits the
    /// score, saves the best, shows the card and reports Over, this frame.
    fn finish(&mut self) {
        let score = self.sim.score() as i64;
        let pit = matches!(self.sim.dying.as_ref().map(|d| d.cause), Some(Cause::Pit));
        let daily = self.rounds.is_daily();
        sys::log(Level::Info, &format!("runner: over at {:.1}s, {} m, score {score} ({}, daily: {daily})", self.sim.t, self.sim.meters(), if pit { "a pit" } else { "a bonk" }));
        let prev = self.rounds.best();
        let heading = if pit { look::HEADING_PIT } else { look::HEADING_BONK };
        let label = if daily { look::DAILY_LABEL } else { look::SCORE_LABEL };
        self.rounds.end(score).heading(heading).label(label);
        self.daily_btn.reset();
        self.daily_btn.rect = self.daily_over;
        self.scores_btn.reset();
        self.sound.over(score > prev && prev > 0);
    }

    fn puff(&mut self, x: f32, y: f32, n: usize, spread: f32, speed: f32) {
        for _ in 0..n {
            let r = &mut self.fx.rng;
            self.puffs.push(Puff { x: x + r.range(-spread, spread), y: y - r.range(0.0, 4.0), vx: -speed * r.range(0.4, 0.8) + r.range(-40.0, 40.0), vy: -r.range(15.0, 60.0), age: 0.0, life: r.range(0.3, 0.55), r: r.range(3.0, 6.0) });
        }
    }

    /// Juice (and sound) for one cue. `playing`: the round, not the attract run.
    fn juice(&mut self, c: Cue, playing: bool, dist: f32, hero_h: f32, speed: f32) {
        self.sound.cue(&c, playing);
        let sx = |x: f32| HERO_X + (x - dist);
        let hero_y = GROUND_Y - hero_h - 20.0;
        let gold = self.theme.gold;
        match c {
            Cue::Jump => {
                if hero_h < 2.0 {
                    self.puff(HERO_X, GROUND_Y, 5, 8.0, speed);
                }
            }
            Cue::DoubleJump => {
                self.fx.ring(HERO_X, hero_y, 42.0, 0xffffffc0);
                self.fx.sparkles(HERO_X, hero_y, 26.0, 6, 0xfff2b0ff);
            }
            Cue::Land { speed: fall } => {
                let n = (2.0 + fall / 160.0) as usize;
                self.puff(HERO_X - 10.0, GROUND_Y, n, 6.0, speed);
                self.puff(HERO_X + 10.0, GROUND_Y, n, 6.0, speed * 0.3);
                if fall > 650.0 && playing {
                    self.shake.add(0.12);
                }
            }
            Cue::Coin { x, y, .. } => {
                self.fx.sparkles(sx(x), GROUND_Y - y, 16.0, 4, 0xfff2b0ff);
                self.coin_punch.kick(0.35);
            }
            Cue::ArcDone { x, y } => {
                // Every coin of an arc: the jump was spot on.
                self.fx.stars(sx(x), GROUND_Y - y, 6, gold);
                if playing {
                    self.popups.spawn(sx(x) + 10.0, GROUND_Y - y - 26.0, look::NICE, gold, 20.0);
                }
            }
            Cue::Close { .. } => {
                self.popups.spawn(HERO_X + 16.0, hero_y - 44.0, &format!("{} +{}", look::CLOSE, sim::CLOSE_POINTS), gold, 26.0);
                self.fx.sparkles(HERO_X, hero_y, 30.0, 8, 0xffffffff);
                if playing {
                    self.slowmo = SLOWMO_TIME;
                    self.flash.fire(0xffffff38);
                }
            }
            Cue::SpeedUp if playing => {
                self.popups.spawn(W / 2.0, GROUND_Y - 120.0, look::FASTER, 0xffffffff, 26.0);
                self.faster_t = 0.0;
            }
            Cue::NewKind(k) if playing => self.banner = Some((k, 0.0)),
            Cue::Milestone(m) if playing => {
                self.popups.spawn(W / 2.0, self.rounds.layout.hud.y + 170.0, &format!("{m} m"), 0xffffffff, 22.0);
                self.fx.confetti(HERO_X, hero_y - 40.0, 26, &self.theme.confetti);
            }
            Cue::Hit { x, y, .. } => {
                let (hx, hy) = (sx(x), GROUND_Y - y);
                self.hitstop.freeze(0.1);
                self.shake.add(0.8);
                self.fx.stars(hx, hy, 9, gold);
                self.fx.burst(hx, hy, 10, 0xfff2d0ff);
                self.flash.fire(0xffffff70);
            }
            Cue::Fall => self.shake.add(0.3),
            Cue::Bounce => self.puff(HERO_X, GROUND_Y, 6, 14.0, 0.0),
            Cue::Stuck => {
                self.shake.add(0.25);
                self.puff(HERO_X + 20.0, GROUND_Y, 8, 16.0, 0.0);
            }
            _ => {}
        }
    }

    fn pose(&self, sim: &Sim) -> Pose {
        if let Some(d) = &sim.dying {
            let (face, run) = match d.cause {
                // Jammed in a pit: legs kicking in the air.
                Cause::Pit => (Face::Scared, self.time * 3.5),
                _ if d.resting || d.t > 0.6 => (Face::Dizzy, 0.0),
                _ => (Face::Scared, 0.0),
            };
            return Pose { x: screen_x(sim, d.x), y: GROUND_Y - d.h, sx: 1.0, sy: 1.0, rot: d.rot, run, grounded: d.resting, rising: false, face, t: self.time };
        }
        let h = &sim.hero;
        let (mut sx, mut sy, rot);
        if h.grounded {
            // Squash on landing (harder landings squash more), a bob while running.
            let k = (1.0 - h.land_t / 0.16).max(0.0) * (h.land_speed / 700.0).min(1.2);
            let bob = (h.run_phase * TAU * 2.0).sin() * 0.035;
            sy = 1.0 - 0.28 * k + bob;
            sx = 1.0 + 0.22 * k - bob * 0.5;
            rot = 0.05;
        } else {
            // Stretch with speed, extra right at launch.
            let s = (h.vy / JUMP_VY).clamp(-1.0, 1.0).abs();
            let launch = (1.0 - h.jump_t / 0.08).max(0.0);
            sy = 1.0 + 0.16 * s + 0.12 * launch;
            sx = 1.0 - 0.1 * s - 0.08 * launch;
            rot = if h.flip > 0.0 { h.flip } else { (-h.vy / 3500.0).clamp(-0.18, 0.18) };
        }
        let near = sim.next_obstacle().map(|o| sim.time_to(o)).unwrap_or(9.0);
        let face = if h.flip > 0.0 {
            Face::Spin
        } else if !h.grounded {
            Face::Jump
        } else if sim.close_t < 0.8 {
            Face::Phew
        } else if sim.coin_t < 0.25 {
            Face::Happy
        } else if near < 0.45 && near > 0.0 {
            Face::Worried
        } else {
            Face::Run
        };
        if sim.hero.h < 0.0 {
            sx = 1.0;
            sy = 1.0;
        }
        Pose { x: HERO_X, y: GROUND_Y - h.h, sx, sy, rot, run: h.run_phase, grounded: h.grounded, rising: h.vy > 0.0, face, t: self.time }
    }

    fn draw_world(&self, sim: &Sim) {
        let sky = look::sky_at(sim.dist / PX_PER_M);
        look::draw_backdrop(&self.art, &sky, sim.dist, self.time);
        let t = sim.t;
        // Pits: their insides first (a falling hero shows in them), ground last.
        let mut pits: Vec<(f32, f32)> = Vec::new();
        for o in sim.obstacles.iter().filter(|o| o.kind == Kind::Pit) {
            let x0 = screen_x(sim, o.x);
            if x0 < W + 10.0 && x0 + o.w > -10.0 {
                pits.push((x0, x0 + o.w));
                look::draw_pit(&self.art, &sky, x0, x0 + o.w);
            }
        }
        for c in sim.coins.iter().filter(|c| !c.taken) {
            let x = screen_x(sim, c.x);
            if (-20.0..W + 20.0).contains(&x) {
                look::draw_coin(&self.art, x, GROUND_Y - c.y, self.time * 5.0 + c.x * 0.02);
            }
        }
        for o in sim.obstacles.iter().filter(|o| o.kind != Kind::Pit) {
            let x = screen_x(sim, o.left(t));
            if x > W + 20.0 || x + o.w < -60.0 {
                continue;
            }
            if o.kind == Kind::Bee {
                look::draw_bee_track(x + o.w * 0.5, GROUND_Y - o.y - o.bob - o.h * 0.5, GROUND_Y - o.y - o.h * 0.5);
            }
            look::draw_obstacle(&self.art, &sky, o.kind, x, o.w, o.h, o.bottom(t), self.time);
        }
        let p = self.pose(sim);
        let over_pit = pits.iter().any(|&(a, b)| p.x > a + 6.0 && p.x < b - 6.0);
        if !over_pit && p.y <= GROUND_Y + 1.0 {
            look::draw_shadow(p.x, GROUND_Y - p.y);
        }
        look::draw_hero(&self.art, &p);
        // The ground, in stretches between the pits.
        pits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut x = -40.0;
        for (a, b) in pits {
            look::draw_ground(&self.art, &sky, x, a, sim.dist);
            x = x.max(b);
        }
        look::draw_ground(&self.art, &sky, x, W + 40.0, sim.dist);
        for pf in &self.puffs {
            let k = pf.age / pf.life;
            // Chunky pixel dust: squares of whole art pixels, shrinking.
            let s = ((pf.r * (1.0 - 0.6 * k)) / 1.5).round().max(1.0) * 1.5;
            gfx2d::rect(((pf.x - s * 0.5) * 2.0).round() * 0.5, ((pf.y - s * 0.5) * 2.0).round() * 0.5, s, s, ui::with_alpha(0xfff1c2ff, 0.85 * (1.0 - k)));
        }
        self.fx.draw();
        // Speed lines after a speed-up.
        if self.faster_t < 0.9 {
            let a = 1.0 - self.faster_t / 0.9;
            for i in 0..9 {
                let y = 110.0 + ((i * 53) % 290) as f32;
                let x = W + 60.0 - ((self.faster_t * 1100.0 + i as f32 * 131.0) % (W + 160.0));
                gfx2d::rect(x.round(), y, 48.0, 3.0, ui::with_alpha(0xffffffff, 0.55 * a));
            }
        }
    }

    fn draw_hints(&self) {
        let sim = &self.sim;
        if !sim.alive() {
            return;
        }
        let th = &self.theme;
        let bob = (self.time * 5.0).sin() * 5.0;
        let big = |s: &str, x: f32, y: f32| {
            text(s).font(Font::SansBold).size(24.0).color(0xffffffff).outline(0.1, th.outline).soft_shadow(0.0, 3.0, 0.06, th.shadow).middle().draw(x, y);
        };
        // The first of a new family on screen: its own hint takes over.
        let intro = sim.obstacles.iter().find(|o| o.intro && !o.passed && o.kind != Kind::Crate && screen_x(sim, o.left(sim.t)) < W - 10.0);
        let intro_hint = intro.and_then(|o| look::kind_hint(o.kind).filter(|_| !(o.kind == Kind::Stack && sim.longest_hold > 0.3)));
        // TAP TO JUMP: a dotted hop over what's next, until they've jumped
        // on purpose, and again when they stall.
        // (Not while the right move is to stay low.)
        let jump_next = sim.next_obstacle().is_none_or(|o| o.window < 1.0);
        if intro_hint.is_none() && (!self.taught || (self.idle > HINT_IDLE && jump_next)) {
            let target = sim.next_obstacle().map(|o| screen_x(sim, o.left(sim.t)) + o.w * 0.5).unwrap_or(HERO_X + 150.0);
            let span = (target - HERO_X).clamp(80.0, 220.0) + 20.0;
            let n = 9;
            for i in 0..n {
                let u = (i as f32 + (self.time * 2.0).fract()) / n as f32;
                let x = HERO_X + span * u;
                let y = GROUND_Y - 24.0 - 4.0 * 100.0 * u * (1.0 - u);
                ui::shape::disc(x, y, 3.2, ui::with_alpha(0xffffffff, 0.9 * (1.0 - u * 0.5)));
            }
            big(look::HINT_TAP, W / 2.0, GROUND_Y - 175.0 + bob);
            // A thumb tapping, where the thumb is.
            let k = (self.time * 1.6).fract();
            let press = Ease::QuadOut.at((k / 0.25).min(1.0));
            ui::shape::ring(W / 2.0, 545.0, 16.0 + 18.0 * press, 3.0, ui::with_alpha(0xffffffff, 0.8 * (1.0 - press)));
            ui::shape::disc(W / 2.0, 545.0, 13.0 - 3.0 * (1.0 - press), 0xffffffc0);
        }
        // The first of a new family: what to do about it.
        if let (Some(o), Some(hint)) = (intro, intro_hint) {
            let x = screen_x(sim, o.left(sim.t));
            {
                big(hint, W / 2.0, GROUND_Y - 215.0 + bob);
                if matches!(o.kind, Kind::Stack | Kind::Log) {
                    // Chevrons pointing up over it: go high.
                    for i in 0..3 {
                        let y = GROUND_Y - o.h - 30.0 - i as f32 * 14.0 - (self.time * 30.0) % 14.0;
                        let cx = x + o.w * 0.5;
                        ui::shape::polyline(&[cx - 9.0, y + 6.0, cx, y - 3.0, cx + 9.0, y + 6.0], 3.5, 0xffffffd0);
                    }
                }
                if o.kind == Kind::Stack && !sim.hero.grounded && sim.hero.jumps == 1 && sim.double_jumps == 0 {
                    big(look::HINT_AGAIN, HERO_X + 30.0, GROUND_Y - sim.hero.h - 80.0);
                }
            }
        }
        // Something flying in from off screen.
        for o in sim.obstacles.iter().filter(|o| matches!(o.kind, Kind::Gull | Kind::Skimmer) && !o.passed) {
            if screen_x(sim, o.left(sim.t)) > W && sim.time_to(o) < 1.4 {
                look::draw_incoming(GROUND_Y - o.y - o.h * 0.5, self.time);
            }
        }
        // A new family's banner.
        if let Some((k, age)) = self.banner {
            let s = Ease::BackOut.at((age / 0.3).min(1.0));
            let a = if age > 1.4 { 1.0 - (age - 1.4) / 0.4 } else { 1.0 };
            if a > 0.0 {
                gfx2d::push();
                gfx2d::translate(W / 2.0, 150.0);
                gfx2d::scale(s, s);
                ui::ribbon(look::kind_name(k), 0.0, 0.0, 40.0, ui::fade(th.accent, a), ui::fade(th.outline, a), th.title_font, -0.04);
                gfx2d::pop();
            }
        }
    }

    fn draw_hud(&self) {
        let th = &self.theme;
        let l = &self.rounds.layout;
        self.hud.draw(l);
        let s = format!("{}", self.sim.coins_got);
        let w = ui::pill_width(&s, 30.0, th.body_font, true);
        let (cx, cy) = (l.hud.right() - 14.0 - w / 2.0, l.hud.y + 26.0);
        gfx2d::push();
        gfx2d::translate(cx, cy);
        let k = self.coin_punch.scale();
        gfx2d::scale(k, k);
        ui::pill(&s, 0.0, 0.0, 30.0, ui::with_alpha(th.outline, 0.55), th.gold, th.body_font, Some(Icon::COIN));
        gfx2d::pop();
        if self.rounds.is_daily() {
            let x = l.pill_zone().right() + 34.0;
            ui::pill("DAILY", x, l.hud.y + 26.0, 24.0, ui::with_alpha(th.outline, 0.5), 0xffffffff, th.body_font, Some(Icon::STAR));
        }
    }
}

impl Game for Runner {
    fn init() -> Self {
        let theme = look::theme();
        // The title is a feed card, an ad: a name, a hook and a brag, never
        // instructions (it can't take input). How to play comes after the tap.
        // The game-over card floats over the aftermath; its heading names how
        // the round was lost (`finish`).
        let rounds = Rounds::new(TitleCard::new(look::TITLE, theme).tagline(look::TAGLINE)).board(BOARD).daily(BOARD_DAILY).floating().ending(0.0);
        let layout = rounds.layout;
        let mut rng = Rng::from_host();
        // DAILY (and SCORES after a round) sit low in the card area, clear of
        // the feed's overlays; on the game-over card, above the challenge strip too.
        let daily_title = Rect::centered(layout.card.cx(), layout.card.bottom() - 24.0, (layout.card.w - 12.0) / 2.0, 44.0);
        let row = Rect::new(layout.results.x, layout.results.bottom() - 46.0, layout.results.w, 44.0).columns(2, 12.0);
        let seed = rng.next_u32() as u64;
        let mut g = Runner {
            sim: Sim::new(seed),
            attract: Sim::new(seed),
            pilot: Autopilot::new(seed, 0.05),
            time: 0.0,
            play_t: 0.0,
            idle: 0.0,
            taught: false,
            fingers: 0,
            theme,
            hud: Hud::new(theme, rounds.best()),
            rounds,
            daily_title,
            daily_over: row[1],
            daily_btn: Button::new(daily_title, "DAILY").kind(ButtonKind::Secondary).with_icon(Icon::STAR).with_haptic(tap_haptic),
            scores_btn: Button::new(row[0], "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY).with_haptic(tap_haptic),
            countdown: Countdown::default(),
            fx: Particles::new(rng.next_u32() as u64),
            popups: Popups::new(),
            puffs: Vec::new(),
            shake: Shake::new(),
            hitstop: HitStop::default(),
            flash: Flash::new(0.25),
            coin_punch: Punch::new(),
            banner: None,
            faster_t: 9.0,
            slowmo: 0.0,
            sound: Sound::new(),
            art: Art::load(),
            rng,
        };
        g.new_attract();
        g
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.art.poll();
        for e in input::poll() {
            if self.ui_event(&e) {
                continue;
            }
            match self.rounds.input(&e) {
                // The tap that enters play is also the first jump (SPEC §5.2).
                // (From the card's retry button it's the release: no jump.)
                Input::Start => {
                    self.start();
                    self.touch(&e);
                }
                Input::Play => self.touch(&e),
                Input::Taken | Input::Extra => {}
            }
        }
        let mut intensity = 0.0;
        match self.rounds.stage() {
            Stage::Title => {
                self.pilot.drive(&mut self.attract);
                self.attract.step(dt);
                let (d, h, v) = (self.attract.dist, self.attract.hero.h, self.attract.speed);
                for c in std::mem::take(&mut self.attract.cues) {
                    self.juice(c, false, d, h, v);
                }
                if self.attract.over {
                    self.new_attract();
                }
            }
            Stage::Playing => {
                if self.countdown.active() {
                    self.countdown.update(dt);
                } else {
                    self.countdown.update(dt);
                    self.play_t += dt;
                    self.idle += dt;
                    let mut k = 1.0;
                    if self.slowmo > 0.0 {
                        self.slowmo -= dt;
                        k = SLOWMO;
                    }
                    if self.sim.t < DEBUG_AUTOPLAY_UNTIL {
                        self.pilot.drive(&mut self.sim);
                    }
                    let sdt = self.hitstop.step(dt) * k;
                    self.sim.step(sdt);
                    let (d, h, v) = (self.sim.dist, self.sim.hero.h, self.sim.speed);
                    for c in std::mem::take(&mut self.sim.cues) {
                        self.juice(c, true, d, h, v);
                    }
                    self.hud.set_score(self.sim.score() as i64);
                    // The tension stem rises with speed, and with a hard one coming.
                    let fast = (self.sim.speed - START_SPEED) / (MAX_SPEED - START_SPEED);
                    let threat = self.sim.next_obstacle().filter(|o| o.nasty && self.sim.time_to(o) < 1.2).is_some() as u32 as f32;
                    intensity = if self.sim.alive() { (0.15 + fast * 1.1 + 0.25 * threat).min(1.0) } else { 0.0 };
                    if self.sim.over {
                        self.finish();
                    }
                }
            }
            Stage::Ending | Stage::Over => {}
        }
        self.sound.update(dt, intensity);
        if let Some((_, age)) = &mut self.banner {
            *age += dt;
            if *age > 2.0 {
                self.banner = None;
            }
        }
        self.faster_t += dt;
        // The cards' animations (and, the frame a round ends, the card itself).
        if self.rounds.update(dt) == Some(ResultsEvent::NewBest) {
            self.fx.celebrate(W / 2.0, 200.0, &self.theme);
        }
        self.hud.update(dt);
        self.daily_btn.update(dt);
        self.scores_btn.update(dt);
        self.fx.update(dt);
        self.popups.update(dt);
        for p in &mut self.puffs {
            p.age += dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy *= (-3.0 * dt).exp();
        }
        self.puffs.retain(|p| p.age < p.life);
        self.shake.update(dt);
        self.flash.update(dt);
        self.coin_punch.update(dt);
    }

    fn render(&self) {
        let l = &self.rounds.layout;
        gfx2d::push();
        self.shake.apply(l.screen.cx(), l.screen.cy());
        self.draw_world(self.shown());
        gfx2d::pop();
        self.flash.draw(l.screen);
        self.popups.draw();
        match self.rounds.stage() {
            Stage::Title => {
                self.rounds.draw();
                self.daily_btn.draw(&self.theme);
            }
            Stage::Playing | Stage::Ending => {
                self.draw_hud();
                self.draw_hints();
                self.countdown.draw(l, &self.theme);
            }
            Stage::Over => {
                self.rounds.draw();
                // How far it got, by the aftermath, above SCORES and DAILY.
                let a = ((self.rounds.age() - 0.8) / 0.3).clamp(0.0, 1.0);
                if a > 0.0 {
                    let th = &self.theme;
                    ui::pill(&format!("{} m", self.sim.meters()), W / 2.0, self.daily_over.y - 24.0, 30.0, ui::with_alpha(th.outline, 0.6 * a), ui::fade(0xffffffff, a), th.body_font, Some(Icon::FLAG));
                }
                self.scores_btn.draw(&self.theme);
                self.daily_btn.draw(&self.theme);
            }
        }
    }

    fn resume(&mut self) {
        // Back from a pause mid-run: a run-up before the obstacles move again.
        if self.rounds.stage() == Stage::Playing && self.sim.alive() {
            self.countdown.start(2);
            self.sim.release();
            self.fingers = 0;
        }
    }
}

export_game!(Runner);
