//! The rules, with no host calls: `cargo test -p NAME` runs them natively.
//! The sim never plays anything: it pushes `Cue`s for the moments that make
//! a sound, and the host (lib.rs, or a test) hands them to `sound.rs`.

use maimbrain::Rng;

/// A bubble you must tap before it shrinks away.
pub struct Sim {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub score: u32,
    pub over: bool,
    /// What happened since the host last took them (`std::mem::take`).
    pub cues: Vec<Cue>,
    /// Shrink speed (units/s); rises with every pop.
    shrink: f32,
    rng: Rng,
}

/// A moment that makes a sound (sound.rs plays each).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A bubble popped; `score` is the new score.
    Pop { score: u32 },
    /// A tap that missed.
    Miss,
    /// The bubble shrank away: the round is over.
    Over,
}

pub const START_R: f32 = 60.0;
/// The smallest a tap may be off the bubble's centre and still pop it: a
/// finger lands within about 9 pt (1 SD) of where its owner means even when
/// careful (`synth::Skill`), so a target under ~44 pt across punishes aim the
/// player can't have.
pub const HIT_MIN: f32 = 26.0;
/// The difficulty curve, tuned against the bot (bot.rs): the shrink speed
/// is `SHRINK + SHRINK_RAMP × √pops`, so it climbs fast at first and ever
/// more slowly after. A first bubble lasts 4.3 s, the 25th 2.4 s, the 100th
/// 1.6 s: first-timers run out of time around 20 pops, good players play on
/// for about 110 (the bot's fingers move as fast as real ones, Fitts' law). A fixed step per pop ends every player's round at the
/// same place: a good player then lasts barely longer than a first-timer.
const SHRINK: f32 = 12.0;
const SHRINK_RAMP: f32 = 2.0;

impl Sim {
    pub fn new(seed: u64) -> Sim {
        Sim { x: 180.0, y: 320.0, r: START_R, score: 0, over: false, cues: Vec::new(), shrink: SHRINK, rng: Rng::new(seed) }
    }

    pub fn step(&mut self, dt: f32) {
        if self.over {
            return;
        }
        self.r -= self.shrink * dt;
        if self.r <= 8.0 {
            self.over = true;
            self.cues.push(Cue::Over);
        }
    }

    /// A tap at (x, y); true if it popped the bubble.
    pub fn tap(&mut self, x: f32, y: f32) -> bool {
        if self.over {
            return false;
        }
        if (x - self.x).hypot(y - self.y) > (self.r + 12.0).max(HIT_MIN) {
            self.cues.push(Cue::Miss);
            return false;
        }
        self.score += 1;
        self.cues.push(Cue::Pop { score: self.score });
        self.shrink = SHRINK + SHRINK_RAMP * (self.score as f32).sqrt();
        self.r = START_R;
        self.x = self.rng.range(70.0, 290.0);
        self.y = self.rng.range(160.0, 460.0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seconds_cannot_fail() {
        let mut s = Sim::new(1);
        for _ in 0..(60 * 3) {
            s.step(1.0 / 60.0);
        }
        assert!(!s.over);
    }

    #[test]
    fn tapping_the_bubble_scores() {
        let mut s = Sim::new(1);
        assert!(s.tap(s.x, s.y));
        assert_eq!(s.score, 1);
        assert!(!s.tap(-100.0, -100.0));
        assert_eq!(s.cues, [Cue::Pop { score: 1 }, Cue::Miss]);
    }
}
