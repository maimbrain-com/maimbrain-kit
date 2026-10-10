//! What the player's touches mean, with no host calls. lib.rs feeds it
//! `input::poll()` and the bot (bot.rs) feeds it the touches it makes, so
//! tests play through the same mapping a finger does. Every rule about raw
//! input lives here: which events count, swipe distances and speeds, hold
//! times, dead zones. A gesture that needs memory (where a swipe started,
//! when a hold began) keeps it in `Control`.

use maimbrain::gfx3d::Vec3;
use maimbrain::input::Event;

use crate::sim::Sim;

/// What the player did, in the game's terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// A tap at (x, y), logical units.
    Tap { x: f32, y: f32 },
}

#[derive(Default)]
pub struct Control {}

impl Control {
    /// The action an event makes, if any. A tap acts on touch-down, the
    /// earliest moment there is (SPEC §5.2), never on release.
    pub fn event(&mut self, e: &Event) -> Option<Action> {
        e.is_press().then_some(Action::Tap { x: e.x, y: e.y })
    }
}

/// Applies an action to the sim; where the drone was caught, if it was.
pub fn apply(sim: &mut Sim, a: Action) -> Option<Vec3> {
    match a {
        Action::Tap { x, y } => sim.tap(x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{H, W};
    use maimbrain::input::synth;

    #[test]
    fn a_tap_on_the_drone_catches_it_on_touch_down() {
        let (mut sim, mut c) = (Sim::new(1), Control::default());
        sim.step(0.5);
        let p = sim.camera().project(sim.drone(), W, H);
        let actions: Vec<Action> = synth::tap(p.x, p.y, 0.5).iter().filter_map(|e| c.event(e)).collect();
        assert_eq!(actions, [Action::Tap { x: p.x, y: p.y }], "the down taps, the up doesn't");
        assert!(apply(&mut sim, actions[0]).is_some());
    }
}
