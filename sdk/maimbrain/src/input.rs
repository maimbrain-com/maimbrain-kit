//! Raw input events (SPEC §5.2). Events arrive at frame boundaries; poll them
//! in `update`. Coordinates are logical units. `time` is the game time the
//! event itself happened: within the span since the previous update
//! (`sys::time() - dt ..= sys::time()`), in order, never ahead of `sys::time()`.
//!
//! Controllers use the W3C standard mapping: [`pad`] names its buttons and
//! axes, and every pad event says which kind of controller sent it
//! ([`Event::pad_style`]), so prompts can show ✕ or A.

use crate::ffi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    TouchDown = 1,
    TouchMove = 2,
    TouchUp = 3,
    TouchCancel = 4,
    KeyDown = 5,
    KeyUp = 6,
    MouseDown = 7,
    MouseMove = 8,
    MouseUp = 9,
    MouseWheel = 10,
    PadButton = 11,
    PadAxis = 12,
    Text = 13,
    /// The screen changed (an `orientation = "any"` game turned, a `fill`
    /// game's screen changed shape; SPEC §5.2): read [`crate::sys::screen`]
    /// again and lay out for it. `x` and `y` are the new width and height.
    /// It comes first in its frame. Never sent to a portrait letterboxed game.
    Resize = 14,
    Unknown = 255,
}

/// Which face buttons a controller has (SPEC §5.2), from a pad event's `id`.
/// The buttons are the same standard-mapping indices either way; only what's
/// printed on them differs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PadStyle {
    /// Unknown: say "A" / "B", or draw the button's position.
    #[default]
    Generic,
    /// A B X Y (Xbox, Backbone, Razer Kishi and most phone docks).
    Xbox,
    /// ✕ ○ □ △ (DualShock, DualSense, Backbone PlayStation Edition).
    PlayStation,
    /// B A Y X, east and south swapped in name (Switch Pro, Joy-Con).
    Nintendo,
}

impl PadStyle {
    /// What's printed on a standard-mapping face button (`pad::A` … `pad::Y`),
    /// for prompts: `"A"`, `"✕"`, `"B"`. Other buttons get their usual names
    /// (`"LB"`, `"L1"`, `"L"`; `"START"`, `"OPTIONS"`, `"+"`).
    pub fn label(self, button: u32) -> &'static str {
        use PadStyle::*;
        match (self, button) {
            (PlayStation, pad::A) => "✕",
            (PlayStation, pad::B) => "○",
            (PlayStation, pad::X) => "□",
            (PlayStation, pad::Y) => "△",
            (PlayStation, pad::LB) => "L1",
            (PlayStation, pad::RB) => "R1",
            (PlayStation, pad::LT) => "L2",
            (PlayStation, pad::RT) => "R2",
            (PlayStation, pad::START) => "OPTIONS",
            (Nintendo, pad::A) => "B",
            (Nintendo, pad::B) => "A",
            (Nintendo, pad::X) => "Y",
            (Nintendo, pad::Y) => "X",
            (Nintendo, pad::LB) => "L",
            (Nintendo, pad::RB) => "R",
            (Nintendo, pad::LT) => "ZL",
            (Nintendo, pad::RT) => "ZR",
            (Nintendo, pad::START) => "+",
            (Nintendo, pad::SELECT) => "-",
            (_, pad::A) => "A",
            (_, pad::B) => "B",
            (_, pad::X) => "X",
            (_, pad::Y) => "Y",
            (_, pad::LB) => "LB",
            (_, pad::RB) => "RB",
            (_, pad::LT) => "LT",
            (_, pad::RT) => "RT",
            (_, pad::SELECT) => "VIEW",
            (_, pad::START) => "MENU",
            (_, pad::UP) => "↑",
            (_, pad::DOWN) => "↓",
            (_, pad::LEFT) => "←",
            (_, pad::RIGHT) => "→",
            _ => "?",
        }
    }

    /// The button's name as a word, for sentences and for fonts without
    /// ✕ ○ □ △ (most pixel fonts): "CROSS", "CIRCLE", "SQUARE", "TRIANGLE"
    /// on a PlayStation controller, [`label`](Self::label) otherwise.
    /// `ui::pad_button` draws the shapes instead.
    pub fn word(self, button: u32) -> &'static str {
        match (self, button) {
            (PadStyle::PlayStation, pad::A) => "CROSS",
            (PadStyle::PlayStation, pad::B) => "CIRCLE",
            (PadStyle::PlayStation, pad::X) => "SQUARE",
            (PadStyle::PlayStation, pad::Y) => "TRIANGLE",
            _ => self.label(button),
        }
    }
}

/// W3C standard-mapping button indices (`Event::code` of a `PadButton`) and
/// axes (of a `PadAxis`; −1 is left/up, a 0.1 dead zone is already applied).
pub mod pad {
    /// Bottom face button (A / ✕ / Switch B).
    pub const A: u32 = 0;
    /// Right face button (B / ○ / Switch A).
    pub const B: u32 = 1;
    /// Left face button (X / □ / Switch Y).
    pub const X: u32 = 2;
    /// Top face button (Y / △ / Switch X).
    pub const Y: u32 = 3;
    pub const LB: u32 = 4;
    pub const RB: u32 = 5;
    /// Triggers: `a` is how far they're pulled, 0–1.
    pub const LT: u32 = 6;
    pub const RT: u32 = 7;
    pub const SELECT: u32 = 8;
    /// Start / Menu / Options: the platform's pause (it does what the pause
    /// pill does), so in the app and the web player games never receive it.
    pub const START: u32 = 9;
    /// Stick clicks.
    pub const LS: u32 = 10;
    pub const RS: u32 = 11;
    pub const UP: u32 = 12;
    pub const DOWN: u32 = 13;
    pub const LEFT: u32 = 14;
    pub const RIGHT: u32 = 15;
    /// Axes.
    pub const LX: u32 = 0;
    pub const LY: u32 = 1;
    pub const RX: u32 = 2;
    pub const RY: u32 = 3;
}

/// HID usage codes (`Event::code` of a key event) for the keys games use most.
pub mod key {
    pub const ENTER: u32 = 40;
    pub const ESCAPE: u32 = 41;
    pub const SPACE: u32 = 44;
    pub const RIGHT: u32 = 79;
    pub const LEFT: u32 = 80;
    pub const DOWN: u32 = 81;
    pub const UP: u32 = 82;
    /// Letters: `A + (letter − 'a')`, so W is `A + 22`.
    pub const A: u32 = 4;
    pub const W: u32 = 26;
    pub const S: u32 = 22;
    pub const D: u32 = 7;
    pub const J: u32 = 13;
    pub const K: u32 = 14;
    pub const L: u32 = 15;
    pub const Z: u32 = 29;
    pub const X: u32 = 27;
}

/// One input event, exactly as the host writes it (32 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Event {
    pub kind: u8,
    /// Touch/pointer id, or mouse button.
    pub id: u8,
    /// Gamepad index.
    pub pad: u16,
    pub x: f32,
    pub y: f32,
    /// Kind-specific: wheel dx / axis value / pressure.
    pub a: f32,
    /// Kind-specific: wheel dy.
    pub b: f32,
    /// Key code, gamepad button/axis, or a Unicode scalar for `Text`.
    pub code: u32,
    /// Game time of the event itself (finer than a frame for touches,
    /// mouse and keys; recorded, so replays match). Judge timing with this.
    pub time: f64,
}

const _: () = assert!(size_of::<Event>() == 32);

impl Event {
    pub fn kind(&self) -> Kind {
        match self.kind {
            1 => Kind::TouchDown,
            2 => Kind::TouchMove,
            3 => Kind::TouchUp,
            4 => Kind::TouchCancel,
            5 => Kind::KeyDown,
            6 => Kind::KeyUp,
            7 => Kind::MouseDown,
            8 => Kind::MouseMove,
            9 => Kind::MouseUp,
            10 => Kind::MouseWheel,
            11 => Kind::PadButton,
            12 => Kind::PadAxis,
            13 => Kind::Text,
            14 => Kind::Resize,
            _ => Kind::Unknown,
        }
    }

    /// True for a touch or mouse press.
    pub fn is_press(&self) -> bool {
        matches!(self.kind(), Kind::TouchDown | Kind::MouseDown)
    }

    /// True for a "go" that isn't a pointer: Enter or Space going down, or a
    /// controller's A (✕) pressed. Title cards and game-over cards
    /// take it as a tap ([`crate::rounds`] does), so a game played with a
    /// controller or keyboard never needs the screen touched.
    pub fn is_confirm(&self) -> bool {
        match self.kind() {
            Kind::KeyDown => self.code == key::ENTER || self.code == key::SPACE,
            Kind::PadButton => self.a > 0.5 && self.code == pad::A,
            _ => false,
        }
    }

    /// A pad button pressed (not released): `Some(button)`.
    pub fn pad_pressed(&self) -> Option<u32> {
        (self.kind() == Kind::PadButton && self.a > 0.5).then_some(self.code)
    }

    /// A pad button released: `Some(button)`.
    pub fn pad_released(&self) -> Option<u32> {
        (self.kind() == Kind::PadButton && self.a <= 0.5).then_some(self.code)
    }

    /// For pad events, which kind of controller sent it (for button prompts).
    pub fn pad_style(&self) -> PadStyle {
        match (self.kind(), self.id) {
            (Kind::PadButton | Kind::PadAxis, 1) => PadStyle::Xbox,
            (Kind::PadButton | Kind::PadAxis, 2) => PadStyle::PlayStation,
            (Kind::PadButton | Kind::PadAxis, 3) => PadStyle::Nintendo,
            _ => PadStyle::Generic,
        }
    }

    /// The screen changed: lay out again for [`crate::sys::screen`].
    pub fn is_resize(&self) -> bool {
        self.kind() == Kind::Resize
    }

    /// True for a touch or mouse moving (a drag, or the mouse hovering in
    /// the preview: check you're tracking a press).
    pub fn is_move(&self) -> bool {
        matches!(self.kind(), Kind::TouchMove | Kind::MouseMove)
    }

    /// True for a touch or mouse let go, including a touch the system took
    /// away (`TouchCancel`: a notification, a gesture): end the drag or hold
    /// either way.
    pub fn is_release(&self) -> bool {
        matches!(self.kind(), Kind::TouchUp | Kind::TouchCancel | Kind::MouseUp)
    }
}

/// Touches made in code, for bots and tests: the events a finger produces,
/// with their times, to feed through the same input mapping `update` uses.
/// ```
/// use maimbrain::input::{Kind, synth};
/// let mut finger = synth::Queue::new();
/// finger.push(synth::tap(100.0, 200.0, 0.5));
/// finger.push(synth::stroke(&[(180.0, 500.0), (180.0, 300.0)], 1.0, 0.15, synth::PANEL_HZ));
/// let frame = finger.due(1.0 / 60.0 * 31.0);       // what the host delivers at an update ending then
/// assert_eq!(frame[0].kind(), Kind::TouchDown);
/// assert!(frame.iter().all(|e| e.time <= 31.0 / 60.0));
/// ```
pub mod synth {
    use super::{Event, Kind};
    use std::collections::VecDeque;

    mod skill;
    pub use skill::{Pointing, Skill, Swipe, gauss};

    /// How often an iPhone's touch panel reports a moving finger (Hz).
    pub const PANEL_HZ: f32 = 120.0;
    /// How long a quick tap stays down (s).
    pub const TAP_HOLD: f64 = 0.06;

    /// One touch event (finger 0) at `time` (s, game time).
    pub fn touch(kind: Kind, x: f32, y: f32, time: impl Into<f64>) -> Event {
        Event { kind: kind as u8, x, y, time: time.into(), ..Event::default() }
    }

    /// A quick tap at (x, y): down at `t`, up `TAP_HOLD` later, not moving.
    pub fn tap(x: f32, y: f32, t: impl Into<f64>) -> Vec<Event> {
        hold(x, y, t, TAP_HOLD)
    }

    /// A finger held still at (x, y) from `t` for `duration` seconds.
    pub fn hold(x: f32, y: f32, t: impl Into<f64>, duration: impl Into<f64>) -> Vec<Event> {
        let t = t.into();
        vec![touch(Kind::TouchDown, x, y, t), touch(Kind::TouchUp, x, y, t + duration.into())]
    }

    /// A finger drawn along `points` at an even speed: down at the first at
    /// `t0`, a move every 1/`hz` s (`PANEL_HZ` is what a phone reports), up
    /// at the last at `t0 + duration`. A swipe is two points 0.1–0.2 s apart.
    pub fn stroke(points: &[(f32, f32)], t0: impl Into<f64>, duration: impl Into<f64>, hz: f32) -> Vec<Event> {
        let (t0, duration) = (t0.into(), duration.into().max(0.0));
        let Some(&(x0, y0)) = points.first() else { return Vec::new() };
        let lens: Vec<f32> = points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).collect();
        let total: f32 = lens.iter().sum();
        // The point a fraction k of the way along the path.
        let at = |k: f32| {
            let mut d = k * total;
            for (w, &len) in points.windows(2).zip(&lens) {
                if d <= len && len > 0.0 {
                    let f = d / len;
                    return (w[0].0 + (w[1].0 - w[0].0) * f, w[0].1 + (w[1].1 - w[0].1) * f);
                }
                d -= len;
            }
            *points.last().unwrap()
        };
        let n = ((duration * hz as f64).round() as usize).max(1);
        let mut out = vec![touch(Kind::TouchDown, x0, y0, t0)];
        for i in 1..=n {
            let k = i as f64 / n as f64;
            let (x, y) = at(k as f32);
            out.push(touch(Kind::TouchMove, x, y, t0 + duration * k));
        }
        let (x, y) = at(1.0);
        out.push(touch(Kind::TouchUp, x, y, t0 + duration));
        out
    }

    /// A flick's lift: the finger leaves the glass this far (0–1) through the
    /// smooth reach it began, still moving at about 40% of its top speed.
    const FLICK_LIFT: f64 = 0.8;
    /// How far a flick's path bows to one side, as a fraction of its length.
    const FLICK_BOW: f32 = 0.04;
    /// Noise on each move a flick reports: position (logical units) and time (s).
    const FLICK_JITTER: (f32, f64) = (0.4, 0.001);

    /// A swipe as a finger makes it, from `from` to `to` (logical units; the
    /// vector between them is the direction and length), starting at `t0`
    /// and lifting `duration` s later (0.08–0.2 s is a flick). Unlike
    /// `stroke`'s even speed, its speed rises and falls in a bell (minimum
    /// jerk) and the finger lifts while still moving, its path bows slightly
    /// to one side, and each move comes at the panel rate (`PANEL_HZ`) with a
    /// little noise in where and when. `seed` picks the side and the noise:
    /// the same seed gives the same flick.
    /// ```
    /// use maimbrain::input::{Kind, synth};
    /// let f = synth::flick((180.0, 500.0), (180.0, 380.0), 1.0, 0.12, 7);
    /// assert_eq!((f[0].kind(), f[0].y, f[0].time), (Kind::TouchDown, 500.0, 1.0));
    /// let up = f.last().unwrap();
    /// assert!(up.kind() == Kind::TouchUp && (up.time - 1.12).abs() < 1e-9);
    /// assert!((up.y - 380.0).abs() < 1.0);
    /// ```
    pub fn flick(from: (f32, f32), to: (f32, f32), t0: impl Into<f64>, duration: impl Into<f64>, seed: u64) -> Vec<Event> {
        let (t0, duration) = (t0.into(), duration.into().max(0.0));
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = dx.hypot(dy);
        // Minimum jerk, cut off at the lift and scaled so the lift is at `to`.
        let reach = |u: f64| u * u * u * (10.0 - 15.0 * u + 6.0 * u * u);
        let along = |k: f64| (reach(k * FLICK_LIFT) / reach(FLICK_LIFT)) as f32;
        let mut rng = crate::Rng::new(seed);
        let side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        let (nx, ny) = if len > 0.0 { (-dy / len, dx / len) } else { (0.0, 0.0) };
        let at = |k: f64| {
            let s = along(k);
            let bow = side * FLICK_BOW * len * (std::f32::consts::PI * s).sin();
            (from.0 + dx * s + nx * bow, from.1 + dy * s + ny * bow)
        };
        let n = ((duration * PANEL_HZ as f64).round() as usize).max(1);
        let mut out = vec![touch(Kind::TouchDown, from.0, from.1, t0)];
        let mut last = t0;
        for i in 1..=n {
            let k = i as f64 / n as f64;
            let (x, y) = at(k);
            let (jx, jy) = (rng.range(-1.0, 1.0) * FLICK_JITTER.0, rng.range(-1.0, 1.0) * FLICK_JITTER.0);
            // The last move is where the finger lifts; the others wobble a little in time, never out of order.
            let t = if i == n { t0 + duration } else { (t0 + duration * k + rng.range(-1.0, 1.0) as f64 * FLICK_JITTER.1).clamp(last, t0 + duration) };
            last = t;
            out.push(touch(Kind::TouchMove, x + jx, y + jy, t));
        }
        let end = out.last().map_or(from, |e| (e.x, e.y));
        out.push(touch(Kind::TouchUp, end.0, end.1, t0 + duration));
        out
    }

    /// A hand pointing the phone from one aim to another (degrees: yaw
    /// right, pitch up): a quick minimum-jerk swing that overshoots the
    /// target by a few percent, settles back onto it, and keeps a faint
    /// tremor, as a person aiming does. Seeded, so the same seed is the same
    /// hand. For bots and tests of games that aim with
    /// [`sensors::Scope`](crate::sensors::Scope): feed `at(t)` to
    /// `Scope::update_with` each frame. This hand lands exactly on `to` in
    /// `duration`; for a bot, [`Skill::aim`] takes the time, the landing
    /// error and the tremor from how precise people really are.
    ///
    /// ```
    /// use maimbrain::input::synth;
    /// let path = synth::aim((0.0, 0.0), (12.0, 4.0), 1.0, 0.35, 3);
    /// let d = 180.0 / std::f32::consts::PI;
    /// let end = path.at(2.0);
    /// assert!((end.yaw() * d - 12.0).abs() < 0.2 && (end.pitch() * d - 4.0).abs() < 0.2);
    /// let swing = (1..60).map(|i| path.at(1.0 + i as f64 / 100.0).yaw() * d).fold(0.0, f32::max);
    /// assert!(swing > 12.2, "it overshoots a little: {swing}");
    /// ```
    pub fn aim(from: (f32, f32), to: (f32, f32), t0: impl Into<f64>, duration: impl Into<f64>, seed: u64) -> AimPath {
        let mut rng = crate::rng::Rng::new(seed);
        let overshoot = rng.range(0.04, 0.1);
        AimPath::new(from, to, t0.into(), duration.into(), overshoot, 0.03, &mut rng)
    }

    /// See [`aim`] and [`Skill::aim`].
    #[derive(Clone, Copy, Debug)]
    pub struct AimPath {
        from: (f32, f32),
        to: (f32, f32),
        t0: f64,
        duration: f64,
        overshoot: f32,
        /// Tremor on each axis (degrees, RMS).
        tremor: f32,
        phases: [f32; 4],
    }

    impl AimPath {
        pub(crate) fn new(from: (f32, f32), to: (f32, f32), t0: f64, duration: f64, overshoot: f32, tremor: f32, rng: &mut crate::rng::Rng) -> AimPath {
            // 6.28, not TAU: changing it would change every seeded bot's hand, and the
            // games' committed bot reports with it.
            #[allow(clippy::approx_constant)]
            let phases = [rng.range(0.0, 6.28), rng.range(0.0, 6.28), rng.range(0.0, 6.28), rng.range(0.0, 6.28)];
            AimPath { from, to, t0, duration: duration.max(0.01), overshoot, tremor, phases }
        }

        /// When the swing has settled (game time, s).
        pub fn landed(&self) -> f64 {
            self.t0 + self.duration
        }

        /// Where it settles (degrees), before tremor: a skilled hand's
        /// [`Skill::aim`] lands near its target, not on it.
        pub fn end(&self) -> (f32, f32) {
            self.to
        }

        /// The aim at game time `t` (degrees: yaw right, pitch up).
        pub fn angles(&self, t: f64) -> (f32, f32) {
            let mj = |u: f64| {
                let u = u.clamp(0.0, 1.0) as f32;
                u * u * u * (10.0 - 15.0 * u + 6.0 * u * u)
            };
            let u = (t - self.t0) / self.duration;
            let past = (self.to.0 + (self.to.0 - self.from.0) * self.overshoot, self.to.1 + (self.to.1 - self.from.1) * self.overshoot);
            let lerp = |a: (f32, f32), b: (f32, f32), k: f32| (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k);
            // 70% of the time swinging past the target, the rest settling back.
            let (y, p) = if u < 0.7 { lerp(self.from, past, mj(u / 0.7)) } else { lerp(past, self.to, mj((u - 0.7) / 0.3)) };
            // Physiological tremor: two sines per axis at 8–11 Hz.
            let s = t as f32 * std::f32::consts::TAU;
            let (ph, k) = (self.phases, self.tremor);
            (y + k * ((8.3 * s + ph[0]).sin() + (10.7 * s + ph[1]).sin()), p + k * ((9.1 * s + ph[2]).sin() + (11.3 * s + ph[3]).sin()))
        }

        /// The attitude at game time `t`, with the rotation rate a gyroscope
        /// would report.
        pub fn at(&self, t: f64) -> crate::sensors::Attitude {
            use crate::sensors::Attitude;
            let r = std::f32::consts::PI / 180.0;
            let pose = |t: f64| {
                let (y, p) = self.angles(t);
                Attitude::from_angles(y * r, p * r, 0.0)
            };
            let h = 0.005;
            pose(t).with_rate_from(pose(t - h), h as f32)
        }
    }

    /// Touches waiting to happen, handed out frame by frame as the host
    /// would: `due(now)` is every event up to `now`, in time order.
    #[derive(Clone, Debug, Default)]
    pub struct Queue {
        events: VecDeque<Event>,
    }

    impl Queue {
        pub fn new() -> Queue {
            Queue::default()
        }
        /// Adds touches (any order; they're kept sorted by time).
        pub fn push(&mut self, events: impl IntoIterator<Item = Event>) {
            for e in events {
                let i = self.events.partition_point(|q| q.time <= e.time);
                self.events.insert(i, e);
            }
        }
        /// The events at or before `now` (game time), removed from the queue.
        pub fn due(&mut self, now: impl Into<f64>) -> Vec<Event> {
            let now = now.into();
            let n = self.events.partition_point(|e| e.time <= now);
            self.events.drain(..n).collect()
        }
        pub fn is_empty(&self) -> bool {
            self.events.is_empty()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn strokes_report_at_the_panel_rate_and_end_where_they_should() {
            let s = stroke(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)], 2.0, 0.25, PANEL_HZ);
            assert_eq!(s.len(), 32, "down, 30 moves (0.25 s at 120 Hz), up");
            assert_eq!((s[0].kind(), s[0].time), (Kind::TouchDown, 2.0));
            assert!(s[1..31].iter().all(|e| e.kind() == Kind::TouchMove));
            assert!(s.windows(2).all(|w| w[1].time >= w[0].time));
            assert!((s[1].time - (2.0 + 1.0 / 120.0)).abs() < 1e-9);
            // Even speed along the path: halfway is the corner.
            assert_eq!((s[15].x, s[15].y), (100.0, 0.0));
            let up = s.last().unwrap();
            assert_eq!((up.kind(), up.x, up.y, up.time), (Kind::TouchUp, 100.0, 100.0, 2.25));
        }

        #[test]
        fn a_flick_speeds_up_and_lifts_while_moving_on_a_slightly_bowed_path() {
            let f = flick((100.0, 500.0), (100.0, 300.0), 2.0, 0.15, 3);
            assert_eq!(f.len(), 2 + 18, "down, 18 moves (0.15 s at 120 Hz), up");
            assert!(f.windows(2).all(|w| w[1].time >= w[0].time), "in order");
            assert!(f[1..19].iter().all(|e| e.kind() == Kind::TouchMove));
            let up = f.last().unwrap();
            assert!((up.time - 2.15).abs() < 1e-9 && (up.y - 300.0).abs() < 0.5 && (up.x - 100.0).abs() < 0.5, "{up:?}");
            // Speed between moves: a bell, cut off while the finger still moves.
            let speed: Vec<f32> = f[..19].windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).collect();
            let peak = speed.iter().cloned().fold(0.0, f32::max);
            let at_peak = speed.iter().position(|&s| s == peak).unwrap();
            assert!((6..=14).contains(&at_peak), "fastest mid-flick: {speed:?}");
            assert!(speed[0] < peak * 0.3, "slow to start: {speed:?}");
            assert!(speed[17] > peak * 0.25, "still moving when it lifts: {speed:?}");
            // It bows sideways, a few units at most, to the same side throughout.
            let off: Vec<f32> = f.iter().map(|e| e.x - 100.0).collect();
            let widest = off.iter().cloned().fold(0.0f32, |m, x| if x.abs() > m.abs() { x } else { m });
            assert!((5.0..=9.5).contains(&widest.abs()), "about 4% of 200: {off:?}");
            // The same seed, the same flick; another, another.
            assert_eq!(flick((100.0, 500.0), (100.0, 300.0), 2.0, 0.15, 3), f);
            assert_ne!(flick((100.0, 500.0), (100.0, 300.0), 2.0, 0.15, 4), f);
        }

        #[test]
        fn a_queue_hands_out_touches_when_they_happen() {
            let mut q = Queue::new();
            q.push(tap(10.0, 10.0, 0.5));
            q.push(tap(20.0, 20.0, 0.1));
            let first = q.due(0.2);
            assert_eq!(first.len(), 2);
            assert!(first[0].is_press() && first[1].kind() == Kind::TouchUp && first[0].x == 20.0);
            assert_eq!(q.due(0.5).len(), 1);
            assert_eq!(q.due(0.5 + TAP_HOLD).len(), 1);
            assert!(q.is_empty());
        }
    }
}

/// All events queued for this frame.
pub fn poll() -> Vec<Event> {
    let mut out = Vec::new();
    let mut buf = [Event::default(); 32];
    loop {
        let n = unsafe { ffi::mb_input_poll(buf.as_mut_ptr() as *mut u8, buf.len() as u32) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
        if (n as usize) < buf.len() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: Kind, id: u8, code: u32, a: f32) -> Event {
        Event { kind: kind as u8, id, code, a, ..Event::default() }
    }

    #[test]
    fn confirm_is_enter_space_or_a_going_down() {
        assert!(ev(Kind::KeyDown, 0, key::ENTER, 0.0).is_confirm());
        assert!(ev(Kind::KeyDown, 0, key::SPACE, 0.0).is_confirm());
        assert!(!ev(Kind::KeyUp, 0, key::ENTER, 0.0).is_confirm());
        assert!(ev(Kind::PadButton, 1, pad::A, 1.0).is_confirm());
        assert!(!ev(Kind::PadButton, 1, pad::START, 1.0).is_confirm(), "Start is the platform's pause");
        assert!(!ev(Kind::PadButton, 1, pad::A, 0.0).is_confirm(), "a release isn't a go");
        assert!(!ev(Kind::PadButton, 1, pad::B, 1.0).is_confirm());
        assert!(!ev(Kind::TouchDown, 0, 0, 1.0).is_confirm(), "touches are presses, not confirms");
    }

    #[test]
    fn pad_events_say_which_controller_and_label_its_buttons() {
        assert_eq!(ev(Kind::PadButton, 2, pad::A, 1.0).pad_style(), PadStyle::PlayStation);
        assert_eq!(ev(Kind::PadAxis, 3, pad::LX, 0.5).pad_style(), PadStyle::Nintendo);
        assert_eq!(ev(Kind::TouchDown, 2, 0, 1.0).pad_style(), PadStyle::Generic, "a touch's id is its finger");
        assert_eq!(PadStyle::PlayStation.label(pad::A), "✕");
        assert_eq!(PadStyle::Nintendo.label(pad::A), "B");
        assert_eq!(PadStyle::Xbox.label(pad::START), "MENU");
        assert_eq!(PadStyle::PlayStation.word(pad::Y), "TRIANGLE");
        assert_eq!(PadStyle::Xbox.word(pad::Y), "Y");
        assert_eq!(ev(Kind::PadButton, 1, pad::X, 1.0).pad_pressed(), Some(pad::X));
        assert_eq!(ev(Kind::PadButton, 1, pad::X, 0.0).pad_released(), Some(pad::X));
        assert_eq!(Event { kind: 14, ..Event::default() }.kind(), Kind::Resize);
    }
}
