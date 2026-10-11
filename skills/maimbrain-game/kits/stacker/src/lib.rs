//! Picnic Pile: a bird flies back and forth over a plate balanced on a pole,
//! carrying snacks with faces. Tap to let go. Everything is real rigid-body
//! physics (`maimbrain::physics2d`): snacks land, wobble, wedge and tumble.
//! Three tumbles and lunch is over. Wind gusts and a tilting plate join in.
//!
//! The stacker starter kit. Layout:
//! - `sim.rs`: rules + physics + tuning knobs (tested, no host calls)
//! - `look.rs`: names, words, colors, the UI theme (what a reskin changes)
//! - `draw.rs`: the world (snacks, faces, bird, plate, ghost) from the art
//! - `sprites.rs`: source rects in assets/sprites.png (made by `mb art atlas`)
//! - `sound.rs`: sounds, music stems and haptics from the sim's cues
//! - this file: input, the DAILY and SCORES buttons, HUD and juice
//!
//! The round itself (the title card, the tap that starts play, the score,
//! the best, the game-over card, the guard and the retry) is
//! `maimbrain::rounds::Rounds`; `tests::keeps_the_platform_rules` checks it.
//! Daily mode plays the same snacks and twists for everyone today.

mod draw;
mod look;
mod sim;
mod sound;
mod sprites;
#[cfg(test)]
mod tests;

use maimbrain::input::{self, Event};
use maimbrain::juice::{Particles, Popups, VignettePulse};
use maimbrain::motion::{Ease, HitStop, Shake};
use maimbrain::physics2d::vec2;
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::ui::{self, Button, ButtonKind, Hud, Icon, Rect, ResultsEvent, Tap, Theme, TitleCard, text, with_alpha};
use maimbrain::{Game, Rng, export_game, gfx2d, store};

use sim::{Cue, LIVES, Sim, TwistKind};

const BOARD: u32 = 0;
const BOARD_DAILY: u32 = 1;
/// The how-to hint comes back after this long without a tap.
const HINT_IDLE: f32 = 3.0;
/// The title card's own tower starts over after this long (or when it falls).
const ATTRACT_ROUND: f32 = 40.0;
/// Debug: the autopilot plays the rounds too (to reach late states for screenshots).
const DEBUG_AUTOPLAY: bool = false;

/// A big word in the middle of the screen for a moment ("WIND!", "NEW: JELLY!").
struct Banner {
    text: String,
    color: u32,
    icon: Option<Icon>,
    t: f32,
}

struct Picnic {
    /// Title card, round, game-over card, retry. (On the title card the
    /// bird stacks by itself, a little sloppily.)
    rounds: Rounds,
    sim: Sim,
    /// Seeds the title card's own towers.
    rng: Rng,
    time: f32,
    /// Seconds since the player last dropped (for the hint).
    idle: f32,
    theme: Theme,
    hud: Hud,
    daily_btn: Button,
    scores_btn: Button,
    fx: Particles,
    popups: Popups,
    shake: Shake,
    hitstop: HitStop,
    hurt: VignettePulse,
    banner: Option<Banner>,
    art: draw::Art,
    sound: sound::Sound,
}

impl Game for Picnic {
    fn init() -> Self {
        let mut rng = Rng::from_host();
        let theme = look::theme();
        // The score floats over the pile on the blanket. Daily rounds go to
        // their own board, with their own best. The sim plays the collapse
        // itself before it reports over, so no extra ending.
        let rounds = Rounds::new(TitleCard::new(look::TITLE, theme).tagline(look::TAGLINE).size(52.0)).board(BOARD).daily(BOARD_DAILY).heading(look::OVER_HEADING).label(look::SCORE_LABEL).floating().ending(0.0);
        let layout = &rounds.layout;
        // DAILY (and SCORES after a round) sit low in the card area, clear of
        // the feed's overlays, at its edges so the tower stays in view.
        let row = [Rect::new(layout.card.x, layout.card.bottom() - 46.0, 112.0, 44.0), Rect::new(layout.card.right() - 112.0, layout.card.bottom() - 46.0, 112.0, 44.0)];
        let mut g = Picnic {
            sim: Sim::new(rng.next_u32() as u64),
            time: 0.0,
            idle: 0.0,
            hud: Hud::new(theme, 0),
            daily_btn: Button::new(row[1], "DAILY").kind(ButtonKind::Secondary).with_icon(Icon::STAR),
            scores_btn: Button::new(row[0], "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY),
            fx: Particles::new(rng.next_u32() as u64),
            popups: Popups::new(),
            shake: Shake::new(),
            hitstop: HitStop::default(),
            hurt: VignettePulse::new(look::DANGER),
            banner: None,
            art: draw::Art::new(),
            sound: sound::Sound::new(),
            rounds,
            theme,
            rng,
        };
        g.hud.lives = Some((LIVES, LIVES));
        g
    }

    fn update(&mut self, dt: f32) {
        self.art.update();
        self.time += dt;
        self.idle += dt;
        for e in input::poll() {
            // The kit's own buttons first; then the round flow, which says
            // what the rest means.
            if self.ui_event(&e) {
                continue;
            }
            match self.rounds.input(&e) {
                // The tap that starts the round is also its first drop.
                Input::Start => self.start(),
                Input::Play if e.is_press() => {
                    if self.sim.drop_piece() {
                        self.idle = 0.0;
                    }
                }
                Input::Play | Input::Taken | Input::Extra => {}
            }
        }
        let phase = self.rounds.stage();
        if phase == Stage::Playing && DEBUG_AUTOPLAY {
            self.autopilot();
        }
        if phase == Stage::Title {
            self.autopilot();
            if self.sim.over || self.sim.t > ATTRACT_ROUND {
                self.sim = Sim::new(self.rng.next_u32() as u64);
            }
        }
        // A hit-stop freezes the physics for a moment when a heart goes.
        self.sim.step(self.hitstop.step(dt));
        let cues = std::mem::take(&mut self.sim.cues);
        self.juice(&cues);
        self.sound.update(dt, &cues, phase != Stage::Title, &self.sim);
        if phase == Stage::Playing && self.sim.over {
            // The plate has tipped and the bird flown off (the sim's own
            // ending): Rounds submits the score, saves the best, shows the
            // card and reports Over, all in its `update` below.
            let label = if self.rounds.is_daily() { look::DAILY_LABEL } else { look::SCORE_LABEL };
            self.rounds.end(self.sim.score as i64).label(label);
            self.daily_btn.reset();
            self.scores_btn.reset();
        }
        if self.rounds.update(dt) == Some(ResultsEvent::NewBest) {
            self.sound.best();
        }
        self.hud.lives = Some((self.sim.lives, LIVES));
        self.hud.update(dt);
        self.daily_btn.kind = if self.rounds.is_daily() { ButtonKind::Primary } else { ButtonKind::Secondary };
        self.daily_btn.update(dt);
        self.scores_btn.update(dt);
        self.fx.update(dt);
        self.popups.update(dt);
        self.shake.update(dt);
        self.hurt.level = if self.rounds.stage() == Stage::Playing && self.sim.lives == 1 { 0.22 + 0.08 * (self.time * 4.0).sin() } else { 0.0 };
        self.hurt.update(dt);
        if let Some(b) = &mut self.banner {
            b.t += dt;
            if b.t > 1.6 {
                self.banner = None;
            }
        }
    }

    fn render(&self) {
        let l = &self.rounds.layout;
        let phase = self.rounds.stage();
        draw::sky(&self.art, &self.sim, self.time);
        gfx2d::push();
        self.shake.apply(l.screen.cx(), l.screen.cy());
        gfx2d::translate(0.0, -self.sim.cam_y);
        draw::world(&self.art, &self.sim, self.time, phase != Stage::Over && self.sim.ending.is_none());
        if self.show_hint() {
            self.hint_ring();
        }
        self.fx.draw();
        self.popups.draw();
        gfx2d::pop();
        self.hurt.draw(l.screen);
        if self.rounds.playing() {
            self.hud.draw(l);
            if self.rounds.is_daily() {
                // Under the pause pill, clear of the score.
                let z = l.pill_zone();
                ui::pill("DAILY", z.x + 52.0, z.bottom() + 18.0, 22.0, with_alpha(self.theme.outline, 0.5), 0xffffffff, self.theme.body_font, Some(Icon::STAR));
            }
            self.draw_banner();
            if self.show_hint() {
                let b = self.sim.bird();
                let a = 0.75 + 0.25 * (self.time * 6.0).sin();
                text(look::HINT).size(22.0).color(0xffffffff).outline(0.1, self.theme.outline).alpha(a).middle().draw(l.screen.cx(), b.y - self.sim.cam_y - 50.0);
            }
        }
        // The title card (it sells: a name, a hook, the best; how to play
        // waits for play), or the game-over card. DAILY on both, SCORES after a round.
        self.rounds.draw();
        if phase == Stage::Over {
            self.scores_btn.draw(&self.theme);
        }
        if !self.rounds.playing() {
            self.daily_btn.draw(&self.theme);
        }
    }

    fn suspend(&mut self) {
        self.sound.pause();
    }
}

impl Picnic {
    /// A round starts (Rounds has reported Playing and picked its seed).
    fn start(&mut self) {
        self.sim = Sim::new(self.rounds.seed());
        self.hud.reset(self.rounds.best());
        self.banner = None;
        self.popups.list.clear();
        self.idle = 0.0;
        self.sound.start();
        // The tap that starts the round is the first drop: the first snack
        // is a slice of toast over the middle of a still bird, so it lands.
        self.sim.drop_piece();
    }

    /// Offers an event to the kit's own buttons (DAILY on the title and the
    /// game-over card, SCORES on the card); true if one took it. On the card
    /// they wait out its guard, like the rest of the screen.
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
                if !self.rounds.card_ready() {
                    return false; // Rounds takes a frantic tap during the fall
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

    /// Daily mode on or off; from the game-over card, back to the title.
    fn toggle_daily(&mut self) {
        let from_card = self.rounds.stage() == Stage::Over;
        self.rounds.set_daily(!self.rounds.is_daily());
        self.sound.ui(self.rounds.is_daily());
        if from_card {
            self.sim = Sim::new(self.rng.next_u32() as u64);
            self.sound.stop_music();
        }
    }

    /// The title card's bird stacks by itself: mostly neat drops, every
    /// fifth one sloppy, so the card wobbles and now and then tumbles.
    /// Deterministic (it reads only the sim).
    fn autopilot(&mut self) {
        let Some(held) = self.sim.held else { return };
        if held.age < 0.7 {
            return;
        }
        let Some(pred) = self.sim.predict() else { return };
        let slack = if self.sim.drops % 5 == 4 { 24.0 } else { 5.0 };
        if (pred.land.x - self.sim.tower_top().x).abs() < slack {
            self.sim.drop_piece();
        }
    }

    /// Until the player has dropped a few themselves, and whenever they stall.
    fn show_hint(&self) -> bool {
        self.rounds.stage() == Stage::Playing && self.sim.ending.is_none() && self.sim.held.is_some_and(|h| h.age > 0.4) && (self.sim.drops < 3 || self.idle > HINT_IDLE)
    }

    /// A ring pulsing out of the carried snack (in world space).
    fn hint_ring(&self) {
        let Some(p) = self.sim.held_pos() else { return };
        let k = (self.time * 1.4).fract();
        let r = 18.0 + Ease::CubicOut.at(k) * 34.0;
        ui::shape::ring(p.x, p.y, r, 3.0, with_alpha(0xffffffff, 0.8 * (1.0 - k)));
    }

    fn set_banner(&mut self, text: String, color: u32, icon: Option<Icon>) {
        self.banner = Some(Banner { text, color, icon, t: 0.0 });
    }

    fn draw_banner(&self) {
        let Some(b) = &self.banner else { return };
        let pop = Ease::BackOut.at((b.t / 0.25).min(1.0));
        let a = if b.t > 1.3 { 1.0 - (b.t - 1.3) / 0.3 } else { 1.0 };
        let (cx, cy) = (self.rounds.layout.screen.cx(), self.rounds.layout.hud.y + 128.0);
        gfx2d::push();
        gfx2d::translate(cx, cy);
        gfx2d::scale(pop.max(0.01), pop.max(0.01));
        let r = text(&b.text).size(30.0).color(b.color).outline(0.1, self.theme.outline).soft_shadow(0.0, 3.0, 0.06, self.theme.shadow).alpha(a).middle().draw(0.0, 0.0);
        if let Some(icon) = b.icon {
            let wob = (self.time * 10.0).sin() * 3.0;
            icon.draw(r.right() + 22.0 + wob, 0.0, 26.0, with_alpha(b.color, a));
        }
        gfx2d::pop();
    }

    /// Particles, popups, shake and banners from the sim's cues.
    fn juice(&mut self, cues: &[Cue]) {
        let playing = self.rounds.playing();
        let th = self.theme;
        for &cue in cues {
            match cue {
                Cue::Drop => {
                    let b = self.sim.bird();
                    self.fx.burst(b.x, b.y + 16.0, 4, 0xffffffa0);
                }
                Cue::Thud { x, y, vol, heavy } => {
                    self.fx.burst(x, y, if heavy { 7 } else { 3 }, 0xfff3dcb0);
                    if heavy {
                        self.shake.add(0.12 + 0.1 * vol);
                    }
                }
                Cue::Land { x, y, streak, neat, close } if playing => {
                    self.hud.set_score(self.sim.score as i64);
                    if close {
                        self.popups.spawn(x, y - 34.0, look::CLOSE, 0xffffffff, 22.0);
                        self.fx.burst(x, y, 6, 0xfff3dcb0);
                    } else if neat {
                        self.popups.spawn(x, y - 34.0, look::NEAT, th.gold, 24.0);
                        self.fx.sparkles(x, y, 40.0, 10, 0xfff2b0ff);
                    } else {
                        self.popups.score(x, y - 30.0, 1, 0xffffffff);
                        self.fx.sparkles(x, y, 30.0, 5, 0xfff2b0ff);
                    }
                    if streak >= 5 && streak.is_multiple_of(5) {
                        self.popups.spawn(x, y - 62.0, &format!("{streak} IN A ROW!"), th.gold, 20.0);
                        self.fx.stars(x, y, 8, th.gold);
                    }
                }
                Cue::Lost { x, y, counted: true, landed, .. } if playing => {
                    // Show it on screen even if it fell out of view.
                    let (x, y) = (x.clamp(80.0, 280.0), y.clamp(self.sim.cam_y + 140.0, self.sim.cam_y + 540.0));
                    self.popups.spawn(x, y, if landed { look::TUMBLE } else { look::MISS }, th.bad, 26.0);
                    self.fx.burst(x, y, 14, th.bad);
                    self.shake.add(0.45);
                    self.hitstop.freeze(0.07);
                    self.hurt.fire();
                    if self.sim.lives == 1 {
                        self.set_banner(look::LAST_HEART.into(), th.bad, Some(Icon::HEART));
                    }
                }
                Cue::HeartBack if playing => {
                    self.set_banner(look::HEART_BACK.into(), th.bad, Some(Icon::HEART));
                    let b = self.sim.bird();
                    self.fx.stars(b.x, b.y, 8, th.bad);
                }
                Cue::NewKind(kind) if playing => {
                    self.set_banner(format!("NEW: {}!", look::snack(kind).name), th.gold, None);
                    if let Some(p) = self.sim.held_pos() {
                        self.fx.sparkles(p.x, p.y, 50.0, 14, 0xfff2b0ff);
                    }
                }
                Cue::Warn(kind, dir) if playing => {
                    let (word, icon) = match kind {
                        TwistKind::Gust => (look::GUST, if dir > 0.0 { Icon::ARROW_RIGHT } else { Icon::ARROW_LEFT }),
                        TwistKind::Tilt => (look::TILT, if dir > 0.0 { Icon::ARROW_RIGHT } else { Icon::ARROW_LEFT }),
                    };
                    self.set_banner(word.into(), 0xffffffff, Some(icon));
                }
                Cue::Collapse if playing => {
                    self.shake.add(0.8);
                    let p = vec2(sim::W / 2.0, sim::PLATE_TOP);
                    self.fx.burst(p.x, p.y, 20, 0xfff3dcd0);
                }
                _ => {}
            }
        }
    }
}

export_game!(Picnic);
