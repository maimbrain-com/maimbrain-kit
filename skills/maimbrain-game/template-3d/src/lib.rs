//! 3D template: copy this directory to games/<name>, replace NAME in
//! Cargo.toml and manifest.toml (`mb new --3d <name>` does both), and keep
//! the shape: pure rules in `sim`, what touches mean in `control`, the scene
//! built once in `scene`, and a thin host layer here (moving nodes, the HUD).
//! `bot` (tests only) plays through `control` with touches, to tune difficulty.
//!
//! The round (title card, a tap starting play, the escape playing out, the
//! score, the best, the game-over card, retrying) is
//! `maimbrain::rounds::Rounds`, which keeps the platform's rules;
//! `tests::keeps_the_platform_rules` checks them. The HUD and cards are the
//! SDK's UI kit (`maimbrain::ui`, docs/UI.md) drawn with mb2d over the 3D
//! image, in the look of IDENTITY below: replace the one `mb new` picked at
//! random with the game's own (the skill's art-direction step) and match the
//! scene to it.

#[cfg(test)]
mod bot;
mod control;
mod scene;
mod sim;

use maimbrain::gfx3d;
use maimbrain::input::Event;
use maimbrain::juice::Popups;
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::ui::{Hud, Theme, TitleCard, shape, text, with_alpha};
use maimbrain::{Game, export_game, gfx2d, input, sys};

use control::Control;
use sim::{H, Sim, W};

/// The game's identity: a kit identity name or a few words composing one
/// (`Theme::from_identity`, docs/UI.md); `mb font add --identity` bakes its fonts.
const IDENTITY: &str = "__IDENTITY__";

struct Drone {
    rounds: Rounds,
    sim: Sim,
    control: Control,
    scene: scene::Scene,
    /// Seconds since init: drives idle animation (a function of dt only).
    time: f32,
    /// When the last catch happened (game time), for the flash.
    caught_at: Option<f32>,
    theme: Theme,
    hud: Hud,
    popups: Popups,
}

impl Drone {
    fn start(&mut self) {
        self.sim = Sim::new(self.rounds.seed());
        // The drone jumps to the start of its loop: a new trail, not a streak.
        self.scene.trail.detach();
        self.scene.trail.attach(self.scene.drone);
        self.hud.reset(self.rounds.best());
    }
}

impl Game for Drone {
    fn init() -> Self {
        let theme = Theme::from_identity(IDENTITY).load_fonts();
        // The title is a feed card, an ad: a hook and a brag, never
        // instructions (it can't take input). How to play comes after the tap.
        let rounds = Rounds::new(TitleCard::new("DRONE", theme).tagline("catch it if you can")).heading("ESCAPED!").label("CAUGHT");
        Drone {
            sim: Sim::new(sys::rand_seed()),
            control: Control::default(),
            scene: scene::build(),
            time: 0.0,
            caught_at: None,
            theme,
            hud: Hud::new(theme, rounds.best()),
            rounds,
            popups: Popups::new(),
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        for e in input::poll() {
            match self.rounds.input(&e) {
                // The tap that enters play is also the first move (SPEC §5.2).
                Input::Start => {
                    self.start();
                    self.act(&e);
                }
                Input::Play => self.act(&e),
                Input::Taken | Input::Extra => {}
            }
        }
        match self.rounds.stage() {
            Stage::Playing => {
                self.sim.step(dt);
                self.hud.progress = Some(self.sim.clock / sim::START_CLOCK);
                if self.sim.over {
                    // The drone escapes now (it shoots up out of view). Rounds plays
                    // that out, still Playing, then the score, the best, the card, Over.
                    gfx3d::shockwave(self.sim.drone(), 2.5, 0.01, 0.6);
                    self.rounds.end(self.sim.score as i64);
                }
            }
            // The title card is live: let the drone fly its loop.
            Stage::Title => self.sim.phase += 0.6 * dt,
            Stage::Ending | Stage::Over => {}
        }
        // Escaping, the drone shoots up out of view, faster and faster, and stays gone.
        let away = match self.rounds.stage() {
            Stage::Ending => self.rounds.age(),
            Stage::Over => Rounds::ENDING,
            _ => 0.0,
        };
        self.scene.place_drone(self.sim.drone() + gfx3d::vec3(0.0, 12.0 * away * away, 0.0), self.time * 3.0);
        self.scene.flash_light(self.caught_at.map(|t| self.time - t));
        self.rounds.update(dt);
        self.hud.update(dt);
        self.popups.update(dt);
    }

    fn render(&self) {
        gfx3d::camera(&self.sim.camera());
        gfx3d::render();

        // The HUD: mb2d draws on top of the 3D image, in the same logical
        // coordinates `gfx3d::project` returns.
        let s = &self.sim;
        let p = gfx3d::project(s.drone());
        match self.rounds.stage() {
            Stage::Playing | Stage::Ending => {
                // The clock is the HUD's progress bar; red when it runs low.
                self.hud.draw(&self.rounds.layout);
                if p.on_screen && !s.over {
                    // A reticle on the drone, placed by projecting its world position.
                    let r = sim::TAP_RADIUS * (0.95 + 0.08 * (self.time * 8.0).sin());
                    let col = if s.clock / sim::START_CLOCK < 0.3 { self.theme.bad } else { 0xffffffe0 };
                    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        let (cx, cy) = (p.x + dx * r, p.y + dy * r);
                        shape::capsule(cx, cy, cx - dx * 14.0, cy, 3.5, col);
                        shape::capsule(cx, cy, cx, cy - dy * 14.0, 3.5, col);
                    }
                    // Play teaches: a hint until the first catch.
                    if s.score == 0 {
                        text("TAP IT!").size(20.0).outline(0.1, self.theme.outline).middle().draw(p.x, p.y + r + 22.0);
                    }
                }
            }
            Stage::Over => gfx2d::rect(0.0, 0.0, W, H, with_alpha(self.theme.outline, 0.35)),
            Stage::Title => {}
        }
        // The title card, or the game-over card.
        self.rounds.draw();
        self.popups.draw();
    }
}

impl Drone {
    /// A touch, through `control` (the bot plays through the same mapping).
    fn act(&mut self, e: &Event) {
        let Some(a) = self.control.event(e) else { return };
        let velocity = self.sim.drone_velocity();
        if let Some(at) = control::apply(&mut self.sim, a) {
            self.scene.caught(at, velocity);
            self.caught_at = Some(self.time);
            self.hud.set_score(self.sim.score as i64);
            // `Camera::project` is plain Rust, so this works in update (and in tests).
            let q = self.sim.camera().project(at, W, H);
            self.popups.score(q.x, q.y - 30.0, 1, self.theme.gold);
        }
    }
}

export_game!(Drone);

#[cfg(test)]
mod tests {
    /// The platform's rules for a round (SKILL.md "Platform facts that bite"),
    /// on a fake host: a tap starts it, the score goes in before Over, a
    /// frantic tap doesn't restart, the best is kept, the session replays.
    #[test]
    fn keeps_the_platform_rules() {
        maimbrain::sys::test::Conformance::new().check::<super::Drone>();
    }
}
