//! Teaching a move in play: a prompt and a moment of slow motion that
//! always ends. Slow motion that waits for the player ("time slows until
//! you jump") reads as a freeze on a phone: a player who doesn't get it
//! watches the world crawl for 10–15 s. Here it eases back to full speed
//! after at most [`SlowMo::MAX_HOLD`] seconds whether or not the player
//! acted, and the prompt stays up until they do.
//!
//! Teach every run: [`Lesson::reset`] when a run starts, never a flag saved
//! across sessions. A returning player makes the move at once and the
//! prompt is gone in a blink; one who forgot (or a friend handed the phone)
//! isn't left guessing.
//!
//! ```
//! use maimbrain::ui::{Lesson, Theme};
//! let mut jump = Lesson::new("TAP TO JUMP").slow(0.3, 2.5);
//! // update: the first log is close, so teach (once; later calls do nothing)
//! jump.teach();
//! let mut t = 0.0;
//! for _ in 0..60 * 4 {
//!     let world_dt = jump.update(1.0 / 60.0);   // step the world with this
//!     t += world_dt;
//! }
//! assert_eq!(jump.rate(), 1.0, "full speed again after 2.5 s, nobody tapped");
//! assert!(jump.showing(), "the prompt waits for the move");
//! jump.learned();                                // the player jumped
//! // render: jump.draw(&theme, x, y) over the world
//! # let _ = t;
//! ```

use super::color::with_alpha;
use super::theme::Theme;
use super::widgets::pill_in;

/// Slow motion that always ends: eases from full speed down to `rate`,
/// holds there for at most [`MAX_HOLD`](SlowMo::MAX_HOLD) seconds (less if
/// [`end`](SlowMo::end) comes first) and eases back. For a near miss, a hit
/// or a teaching moment ([`Lesson`] uses one). Real time drives it, so it
/// ends on time however slow the world is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlowMo {
    rate: f32,
    hold: f32,
    /// Real seconds since it started (None: idle).
    t: Option<f32>,
}

impl Default for SlowMo {
    fn default() -> Self {
        SlowMo::new()
    }
}

impl SlowMo {
    /// The longest it stays slowed (s). Past ~3 s, slow motion stops being
    /// drama and reads as the game freezing.
    pub const MAX_HOLD: f32 = 3.0;
    /// Seconds to slow down, and to come back to full speed.
    pub const EASE_IN: f32 = 0.2;
    pub const EASE_OUT: f32 = 0.6;

    /// Full speed, nothing going on.
    pub fn new() -> SlowMo {
        SlowMo { rate: 1.0, hold: 0.0, t: None }
    }

    /// Slows the world to `rate` (0.05–1 of full speed) for `hold` seconds
    /// (at most [`MAX_HOLD`](SlowMo::MAX_HOLD)), easing in and out.
    pub fn start(&mut self, rate: f32, hold: f32) {
        self.rate = rate.clamp(0.05, 1.0);
        self.hold = hold.clamp(0.0, Self::MAX_HOLD);
        self.t = Some(0.0);
    }

    /// Back to full speed from now (the player acted): eases out.
    pub fn end(&mut self) {
        if let Some(t) = self.t {
            self.hold = self.hold.min(t);
        }
    }

    /// Moves on by `dt` real seconds; returns the world's `dt` (`dt × rate`).
    pub fn update(&mut self, dt: f32) -> f32 {
        if let Some(t) = &mut self.t {
            *t += dt;
            if *t >= self.hold + Self::EASE_OUT {
                self.t = None;
            }
        }
        dt * self.rate()
    }

    /// How fast the world runs now (1 = full speed).
    pub fn rate(&self) -> f32 {
        let Some(t) = self.t else { return 1.0 };
        let smooth = |k: f32| {
            let k = k.clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        };
        let down = smooth(t / Self::EASE_IN);
        let up = if t > self.hold { smooth((t - self.hold) / Self::EASE_OUT) } else { 0.0 };
        let slowed = down * (1.0 - up);
        1.0 + (self.rate - 1.0) * slowed
    }

    /// Slowed (or easing back).
    pub fn active(&self) -> bool {
        self.t.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Waiting,
    Teaching,
    Learned,
}

/// One move to teach: its prompt (≤ 5 words, a verb: "TAP TO JUMP") and a
/// moment of slow motion when it's first needed. [`teach`](Lesson::teach)
/// when the moment comes (the first gap, the first gust), step the world by
/// what [`update`](Lesson::update) returns, [`learned`](Lesson::learned)
/// when the player does it. The slow motion ends by itself after its hold
/// (2.5 s by default, never more than [`SlowMo::MAX_HOLD`]); the prompt
/// stays until the move is made.
#[derive(Clone, Debug)]
pub struct Lesson {
    pub prompt: String,
    rate: f32,
    hold: f32,
    slow: SlowMo,
    stage: Stage,
    slowed: bool,
    learned: bool,
    /// Seconds the prompt has been up, and its opacity (0–1).
    shown: f32,
    alpha: f32,
}

impl Lesson {
    /// Seconds the prompt fades in or out.
    const FADE: f32 = 0.25;

    /// A lesson with this prompt: the world slows to 30% for 2.5 s when it's taught.
    pub fn new(prompt: &str) -> Lesson {
        Lesson { prompt: prompt.to_string(), rate: 0.3, hold: 2.5, slow: SlowMo::new(), stage: Stage::Waiting, slowed: false, learned: false, shown: 0.0, alpha: 0.0 }
    }

    /// How slow (`rate`, 1 = no slow motion) and for how long (`hold`,
    /// capped at [`SlowMo::MAX_HOLD`]).
    pub fn slow(mut self, rate: f32, hold: f32) -> Lesson {
        self.rate = rate.clamp(0.05, 1.0);
        self.hold = hold.clamp(0.0, SlowMo::MAX_HOLD);
        self
    }

    /// The moment has come: the prompt shows and (the first time only) the
    /// world slows. Calls while it's showing do nothing; after
    /// [`learned`](Lesson::learned) it shows the prompt again, without
    /// slowing (a reminder when the player stalls).
    pub fn teach(&mut self) {
        if self.stage == Stage::Teaching {
            return;
        }
        self.stage = Stage::Teaching;
        self.shown = 0.0;
        if !self.slowed && self.rate < 1.0 {
            self.slowed = true;
            self.slow.start(self.rate, self.hold);
        }
    }

    /// A new run: teaches again from the start, slow motion and all.
    pub fn reset(&mut self) {
        *self = Lesson::new(&self.prompt).slow(self.rate, self.hold);
    }

    /// The player made the move: the prompt fades and time eases back.
    pub fn learned(&mut self) {
        if self.stage == Stage::Teaching {
            self.stage = Stage::Learned;
            self.learned = true;
        }
        self.slow.end();
    }

    /// Moves on by `dt` real seconds; returns the world's `dt` (slowed
    /// while the slow motion lasts).
    pub fn update(&mut self, dt: f32) -> f32 {
        let on = self.stage == Stage::Teaching;
        if on {
            self.shown += dt;
        }
        let step = dt / Self::FADE;
        self.alpha = if on { (self.alpha + step).min(1.0) } else { (self.alpha - step).max(0.0) };
        self.slow.update(dt)
    }

    /// How fast the world runs now (1 = full speed).
    pub fn rate(&self) -> f32 {
        self.slow.rate()
    }

    /// The prompt is up (or fading).
    pub fn showing(&self) -> bool {
        self.alpha > 0.0
    }

    /// The move has been made at least once.
    pub fn is_learned(&self) -> bool {
        self.learned
    }

    /// The prompt in a pill centred at (cx, cy), in the theme's accent,
    /// pulsing gently. Draw it near where the move happens (over the hero,
    /// by the gap), clear of the HUD.
    pub fn draw(&self, theme: &Theme, cx: f32, cy: f32) {
        if self.alpha <= 0.0 {
            return;
        }
        let pulse = 1.0 + 0.06 * (self.shown * std::f32::consts::TAU * 1.2).sin().max(0.0);
        pill_in(theme, &self.prompt, cx, cy, 34.0 * pulse, with_alpha(theme.accent, self.alpha), with_alpha(theme.on_accent, self.alpha), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut SlowMo, secs: f32) -> f32 {
        let mut world = 0.0;
        for _ in 0..(secs * 60.0).round() as usize {
            world += s.update(1.0 / 60.0);
        }
        world
    }

    #[test]
    fn slow_motion_never_lasts_past_its_cap() {
        let mut s = SlowMo::new();
        s.start(0.3, 30.0);
        run(&mut s, 0.5);
        assert!((s.rate() - 0.3).abs() < 1e-3);
        run(&mut s, SlowMo::MAX_HOLD + SlowMo::EASE_OUT - 0.5 + 0.02);
        assert!(!s.active() && s.rate() == 1.0, "the 30 s asked for is capped at 3 s");
        // Ended early: eases back at once.
        s.start(0.3, 2.5);
        run(&mut s, 0.5);
        s.end();
        run(&mut s, SlowMo::EASE_OUT + 0.02);
        assert_eq!(s.rate(), 1.0);
    }

    #[test]
    fn a_lesson_slows_once_and_keeps_its_prompt_until_learned() {
        let mut l = Lesson::new("JUMP").slow(0.3, 2.5);
        assert_eq!(l.update(0.1), 0.1, "nothing until it's taught");
        l.teach();
        let mut world = 0.0;
        for _ in 0..60 * 4 {
            world += l.update(1.0 / 60.0);
        }
        assert!(world > 1.5 && world < 3.0, "slowed, then full speed: {world}");
        assert_eq!(l.rate(), 1.0);
        assert!(l.showing() && !l.is_learned());
        l.learned();
        for _ in 0..30 {
            l.update(1.0 / 60.0);
        }
        assert!(!l.showing() && l.is_learned());
        l.teach();
        assert_eq!(l.update(0.1), 0.1, "a reminder doesn't slow the world again");
        assert!(l.showing());
        // The next run teaches it again, slow motion and all.
        l.reset();
        assert!(!l.showing() && !l.is_learned());
        l.teach();
        for _ in 0..30 {
            l.update(1.0 / 60.0);
        }
        assert!(l.rate() < 0.5);
    }
}
