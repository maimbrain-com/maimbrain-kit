//! Template: copy this directory to games/<name>, replace NAME in Cargo.toml
//! and manifest.toml, and keep the shape — pure rules in `sim`, what touches
//! mean in `control`, a thin host layer here (saving, sound, drawing), what
//! each moment sounds like in `sound` (from the sim's cues), and in `bot`
//! (tests only) a player that plays through `control` with touches, to tune
//! difficulty and record a round's sound for mix_check.py.
//!
//! The round itself (the title card, a tap starting play, the failure
//! playing out, the score, the best, the game-over card, retrying) is
//! `maimbrain::rounds::Rounds`, which keeps the platform's rules for you;
//! `tests::keeps_the_platform_rules` checks them. The HUD and cards come from
//! the SDK's UI kit (`maimbrain::ui`, docs/UI.md), the pops' juice from
//! `maimbrain::juice`. Everything takes its look from one `Theme`, composed
//! from IDENTITY below: don't ship the identity `mb new` picked at random.
//! Choose the game's own (the skill's art-direction step), bake its fonts
//! with `mb font add`, and draw the scene in its palette.

#[cfg(test)]
mod bot;
mod control;
mod sim;
mod sound;

use maimbrain::input::Event;
use maimbrain::juice::{Particles, Popups};
use maimbrain::motion::Shake;
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::ui::{Hud, Theme, TitleCard, shape, with_alpha};
use maimbrain::{Game, export_game, gfx2d, input, sys};

use control::Control;
use sim::Sim;
use sound::Sound;

/// The game's identity: a kit identity (`riso`, `crt`, `neon`, `storybook`,
/// `saloon`, `swiss`, `brutalist`, `comic`, `gothic`, `bubblegum`, `terminal`,
/// `field`, `orbital`, `gala`, `notebook`, `stadium`, `cozy-pixel`) or a few
/// words ("1970s diner menu, mustard and teal, snappy") that compose one
/// (`Theme::from_identity`, docs/UI.md). Its fonts come from `mb font add`.
const IDENTITY: &str = "__IDENTITY__";

struct Bubble {
    rounds: Rounds,
    sim: Sim,
    control: Control,
    time: f32,
    theme: Theme,
    hud: Hud,
    fx: Particles,
    popups: Popups,
    shake: Shake,
    sound: Sound,
}

impl Bubble {
    /// A touch, through `control` (the bot plays through the same mapping).
    fn act(&mut self, e: &Event) {
        let Some(a) = self.control.event(e) else { return };
        let (bx, by) = (self.sim.x, self.sim.y);
        if control::apply(&mut self.sim, a) {
            // Juice: a puff and a ring where it popped, "+1" floating up, a little shake.
            self.fx.burst(bx, by, 18, self.theme.accent);
            self.fx.ring(bx, by, 70.0, 0xffffffc0);
            self.popups.score(bx, by - 20.0, 1, self.theme.gold);
            self.hud.set_score(self.sim.score as i64);
            self.shake.add(0.25);
        }
    }
}

impl Game for Bubble {
    fn init() -> Self {
        // The identity's look, in its library fonts (baked into assets/fonts/ by
        // `mb new` or `mb font add --identity`; until they load it draws in Inter).
        // Adjust any field after: theme.accent, theme.style.motion, theme.title_font…
        let theme = Theme::from_identity(IDENTITY).load_fonts();
        // The title is a feed card, an ad: a hook and a brag, never
        // instructions (it can't take input). How to play comes after the tap.
        // The game-over card's heading is the game's voice, not "GAME OVER".
        let rounds = Rounds::new(TitleCard::new("BUBBLE", theme).tagline("don't let it pop")).heading("POP!").label("SCORE");
        Bubble {
            sim: Sim::new(sys::rand_seed()),
            control: Control::default(),
            time: 0.0,
            theme,
            hud: Hud::new(theme, rounds.best()),
            rounds,
            fx: Particles::new(sys::rand_seed() ^ 0xf00d),
            popups: Popups::new(),
            shake: Shake::new(),
            sound: Sound::new(),
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        for e in input::poll() {
            match self.rounds.input(&e) {
                // The tap that enters play is also the first move (SPEC §5.2).
                Input::Start => {
                    self.sim = Sim::new(self.rounds.seed());
                    self.hud.reset(self.rounds.best());
                    self.act(&e);
                }
                Input::Play => self.act(&e),
                Input::Taken | Input::Extra => {}
            }
        }
        if self.rounds.stage() == Stage::Playing {
            self.sim.step(dt);
            if self.sim.over {
                // The failure lands now (its sound is the sim's Over cue). Rounds
                // plays it out, still Playing, then submits the score, saves the
                // best, shows the game-over card and reports Over.
                self.fx.burst(self.sim.x, self.sim.y, 30, self.theme.bad);
                self.shake.add(0.6);
                self.rounds.end(self.sim.score as i64);
            }
        }
        // The sim's cues (pops, misses, the end) become sound.
        self.sound.update(dt);
        for c in std::mem::take(&mut self.sim.cues) {
            self.sound.cue(c);
        }
        // Returns the card's Tick / NewBest / Landed: hook sounds and haptics here.
        self.rounds.update(dt);
        self.hud.update(dt);
        self.fx.update(dt);
        self.popups.update(dt);
        self.shake.update(dt);
    }

    fn render(&self) {
        let l = &self.rounds.layout;
        gfx2d::push();
        self.shake.apply(l.screen.cx(), l.screen.cy());
        let th = &self.theme;
        th.backdrop(l.screen, self.time);
        let s = &self.sim;
        let pulse = if self.rounds.stage() == Stage::Title { 1.0 + 0.06 * (self.time * 6.0).sin() } else { 1.0 };
        if !s.over {
            // The scene in the identity's palette (draw your own world here).
            shape::soft_disc(s.x, s.y, s.r * pulse + 6.0, 14.0, with_alpha(th.accent, 0.3));
            shape::disc(s.x, s.y, s.r * pulse, th.accent);
            shape::soft_disc(s.x - s.r * 0.35, s.y - s.r * 0.35, s.r * 0.16, s.r * 0.12, 0xffffffb0);
        }
        self.fx.draw();
        gfx2d::pop();
        self.popups.draw();
        if self.rounds.playing() {
            self.hud.draw(l);
            // Play teaches: an animated hint on the bubble until the first pop.
            if s.score == 0 && !s.over {
                let bob = (self.time * 5.0).sin() * 6.0;
                maimbrain::ui::text(&th.case("Pop it!")).font(th.body_font).size(th.fit(th.body_font, 20.0)).color(th.text).outline(0.1, th.outline).middle().draw(s.x, s.y + s.r + 24.0 + bob);
            }
        }
        // The title card, or the game-over card.
        self.rounds.draw();
    }
}

export_game!(Bubble);

#[cfg(test)]
mod tests {
    /// The platform's rules for a round (SKILL.md "Platform facts that bite"),
    /// on a fake host: a tap starts it, the score goes in before Over, a
    /// frantic tap doesn't restart, the best is kept, the session replays.
    #[test]
    fn keeps_the_platform_rules() {
        maimbrain::sys::test::Conformance::new().check::<super::Bubble>();
    }
}
