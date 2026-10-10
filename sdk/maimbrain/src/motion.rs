//! Motion: easing curves, tweens, springs, sequences and staggers, screen
//! shake, scale punches, rolling number counters, hit-stop and pulses.
//!
//! Everything here advances only by the `dt` you pass to `update` (never a
//! clock of its own), so it replays exactly (SPEC §3): update motion state in
//! `Game::update`, read it in `Game::render`.
//!
//! ```
//! use maimbrain::motion::{Ease, Spring, Tween};
//! let mut fade = Tween::new(0.0, 1.0, 0.4, Ease::CubicOut);
//! let mut pos = Spring::bouncy(0.0, 3.0);
//! pos.target = 100.0;
//! for _ in 0..60 { fade.update(1.0 / 60.0); pos.update(1.0 / 60.0); }
//! assert_eq!(fade.value(), 1.0);
//! assert!((pos.value - 100.0).abs() < 5.0);
//! ```

use std::f32::consts::PI;
use std::ops::{Add, Mul, Sub};

/// Easing curves (easings.net names): `t` 0→1 maps to 0→1. Back and
/// elastic overshoot past 1 (and under 0); bounce stays within 0…1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Ease {
    #[default]
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    QuartIn,
    QuartOut,
    QuartInOut,
    QuintIn,
    QuintOut,
    QuintInOut,
    SineIn,
    SineOut,
    SineInOut,
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    CircIn,
    CircOut,
    CircInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
}

impl Ease {
    /// Every curve, for galleries and tests.
    pub const ALL: [Ease; 31] = [
        Ease::Linear, Ease::QuadIn, Ease::QuadOut, Ease::QuadInOut, Ease::CubicIn, Ease::CubicOut, Ease::CubicInOut,
        Ease::QuartIn, Ease::QuartOut, Ease::QuartInOut, Ease::QuintIn, Ease::QuintOut, Ease::QuintInOut,
        Ease::SineIn, Ease::SineOut, Ease::SineInOut, Ease::ExpoIn, Ease::ExpoOut, Ease::ExpoInOut,
        Ease::CircIn, Ease::CircOut, Ease::CircInOut, Ease::BackIn, Ease::BackOut, Ease::BackInOut,
        Ease::ElasticIn, Ease::ElasticOut, Ease::ElasticInOut, Ease::BounceIn, Ease::BounceOut, Ease::BounceInOut,
    ];

    /// The curve at `t` (clamped to 0…1).
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let inout = |f: fn(f32) -> f32, t: f32| if t < 0.5 { f(2.0 * t) / 2.0 } else { 1.0 - f(2.0 - 2.0 * t) / 2.0 };
        let out = |f: fn(f32) -> f32, t: f32| 1.0 - f(1.0 - t);
        use Ease::*;
        match self {
            Linear => t,
            QuadIn => t * t,
            QuadOut => out(|t| t * t, t),
            QuadInOut => inout(|t| t * t, t),
            CubicIn => t * t * t,
            CubicOut => out(|t| t * t * t, t),
            CubicInOut => inout(|t| t * t * t, t),
            QuartIn => t.powi(4),
            QuartOut => out(|t| t.powi(4), t),
            QuartInOut => inout(|t| t.powi(4), t),
            QuintIn => t.powi(5),
            QuintOut => out(|t| t.powi(5), t),
            QuintInOut => inout(|t| t.powi(5), t),
            SineIn => sine_in(t),
            SineOut => out(sine_in, t),
            SineInOut => inout(sine_in, t),
            ExpoIn => expo_in(t),
            ExpoOut => out(expo_in, t),
            ExpoInOut => inout(expo_in, t),
            CircIn => circ_in(t),
            CircOut => out(circ_in, t),
            CircInOut => inout(circ_in, t),
            BackIn => back_in(t),
            BackOut => out(back_in, t),
            BackInOut => inout(back_in, t),
            ElasticIn => elastic_in(t),
            ElasticOut => out(elastic_in, t),
            ElasticInOut => inout(elastic_in, t),
            BounceIn => out(bounce_out, t),
            BounceOut => bounce_out(t),
            BounceInOut => inout(|t| 1.0 - bounce_out(1.0 - t), t),
        }
    }
}

fn sine_in(t: f32) -> f32 {
    1.0 - (t * PI / 2.0).cos()
}
fn expo_in(t: f32) -> f32 {
    if t <= 0.0 { 0.0 } else { (10.0 * t - 10.0).exp2() }
}
fn circ_in(t: f32) -> f32 {
    1.0 - (1.0 - t * t).max(0.0).sqrt()
}
fn back_in(t: f32) -> f32 {
    const C1: f32 = 1.70158;
    (C1 + 1.0) * t * t * t - C1 * t * t
}
fn elastic_in(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    -(10.0 * t - 10.0).exp2() * ((10.0 * t - 10.75) * (2.0 * PI / 3.0)).sin()
}
fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// `ease(e, t)` = `e.at(t)`.
pub fn ease(e: Ease, t: f32) -> f32 {
    e.at(t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Where `v` sits between `a` and `b` (0 at a, 1 at b; not clamped).
pub fn inv_lerp(a: f32, b: f32, v: f32) -> f32 {
    if a == b { 0.0 } else { (v - a) / (b - a) }
}

/// Maps `v` from `a0..a1` to `b0..b1`, clamped.
pub fn remap(v: f32, a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    lerp(b0, b1, inv_lerp(a0, a1, v).clamp(0.0, 1.0))
}

/// Frame-rate independent smoothing: moves `current` toward `target`,
/// closing a fraction `1 − e^(−rate·dt)` of the gap (rate ≈ 1 / time constant).
/// Fine for a number on screen; for anything that moves (a camera, what it
/// follows, a character's position) use [`Follow`]: this one is first order,
/// so its speed jumps whenever the target jumps and every turn shows as a kink.
pub fn damp(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    target + (current - target) * (-rate * dt).exp()
}

/// Moves `current` toward `target` by at most `max_step`.
pub fn approach(current: f32, target: f32, max_step: f32) -> f32 {
    if current < target { (current + max_step).min(target) } else { (current - max_step).max(target) }
}

/// Values a [`Tween`] can animate.
pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl<const N: usize> Lerp for [f32; N] {
    fn lerp(a: Self, b: Self, t: f32) -> Self {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Once,
    /// Starts over at the end.
    Loop,
    /// Plays forward, then backward, forever (a yo-yo).
    PingPong,
}

/// A value easing from `from` to `to` over `duration` seconds, after `delay`.
#[derive(Clone, Copy, Debug)]
pub struct Tween<T: Lerp = f32> {
    pub from: T,
    pub to: T,
    pub duration: f32,
    pub delay: f32,
    pub ease: Ease,
    pub repeat: Repeat,
    t: f32,
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, duration: f32, ease: Ease) -> Self {
        Tween { from, to, duration: duration.max(1e-6), delay: 0.0, ease, repeat: Repeat::Once, t: 0.0 }
    }

    /// Already at `v`, with nothing to do (retarget it later).
    pub fn at_rest(v: T) -> Self {
        let mut t = Tween::new(v, v, 1e-6, Ease::Linear);
        t.t = 1.0;
        t
    }

    pub fn delay(mut self, seconds: f32) -> Self {
        self.delay = seconds;
        self
    }

    pub fn repeat(mut self, r: Repeat) -> Self {
        self.repeat = r;
        self
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }

    /// Seconds since the start, including the delay.
    pub fn elapsed(&self) -> f32 {
        self.t
    }

    /// Linear progress 0…1 within one play (after repeats are applied).
    pub fn progress(&self) -> f32 {
        let t = ((self.t - self.delay) / self.duration).max(0.0);
        match self.repeat {
            Repeat::Once => t.min(1.0),
            Repeat::Loop => t.fract(),
            Repeat::PingPong => {
                let k = t % 2.0;
                if k > 1.0 { 2.0 - k } else { k }
            }
        }
    }

    pub fn value(&self) -> T {
        T::lerp(self.from, self.to, self.ease.at(self.progress()))
    }

    /// True once a `Once` tween has reached `to` (never for repeating ones).
    pub fn done(&self) -> bool {
        self.repeat == Repeat::Once && self.t >= self.delay + self.duration
    }

    pub fn restart(&mut self) {
        self.t = 0.0;
    }

    /// Starts a new tween from the current value to `to` (no delay).
    pub fn retarget(&mut self, to: T, duration: f32) {
        self.from = self.value();
        self.to = to;
        self.duration = duration.max(1e-6);
        self.delay = 0.0;
        self.t = 0.0;
    }
}

/// A damped spring pulling `value` toward `target` (framer-motion style).
/// Integrated exactly for any `dt`, so it's stable at any frame rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    /// Natural frequency in Hz: how fast it moves (2–4 snappy UI, 1 lazy).
    pub frequency: f32,
    /// 1 = critically damped (fastest without overshoot), < 1 bouncy, > 1 sluggish.
    pub damping: f32,
}

impl Spring {
    pub fn new(value: f32, frequency: f32, damping: f32) -> Spring {
        Spring { value, velocity: 0.0, target: value, frequency, damping }
    }

    /// No overshoot: smooth follow, camera moves, sliders.
    pub fn critical(value: f32, frequency: f32) -> Spring {
        Spring::new(value, frequency, 1.0)
    }

    /// A lively overshoot or two: popping UI, landing, scale punches.
    pub fn bouncy(value: f32, frequency: f32) -> Spring {
        Spring::new(value, frequency, 0.4)
    }

    /// Lots of wobble: jelly, springy hinges.
    pub fn wobbly(value: f32, frequency: f32) -> Spring {
        Spring::new(value, frequency, 0.2)
    }

    pub fn to(mut self, target: f32) -> Spring {
        self.target = target;
        self
    }

    /// Adds velocity (units per second): a flick.
    pub fn kick(&mut self, velocity: f32) {
        self.velocity += velocity;
    }

    /// Jumps to `v` at rest.
    pub fn snap(&mut self, v: f32) {
        self.value = v;
        self.target = v;
        self.velocity = 0.0;
    }

    pub fn settled(&self) -> bool {
        (self.value - self.target).abs() < 1e-3 && self.velocity.abs() < 1e-2
    }

    pub fn update(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        if self.settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return;
        }
        let w = (self.frequency * 2.0 * PI).max(1e-3);
        let z = self.damping.max(0.0);
        let (x0, v0) = (self.value - self.target, self.velocity);
        let (x, v) = if (z - 1.0).abs() < 1e-3 {
            let e = (-w * dt).exp();
            let b = v0 + w * x0;
            ((x0 + b * dt) * e, (b - w * (x0 + b * dt)) * e)
        } else if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            let e = (-z * w * dt).exp();
            let (s, c) = (wd * dt).sin_cos();
            let b = (v0 + z * w * x0) / wd;
            (e * (x0 * c + b * s), e * ((b * wd - z * w * x0) * c - (x0 * wd + z * w * b) * s))
        } else {
            let q = (z * z - 1.0).sqrt();
            let (r1, r2) = (-w * (z - q), -w * (z + q));
            let c1 = (v0 - r2 * x0) / (r1 - r2);
            let c2 = x0 - c1;
            let (e1, e2) = ((r1 * dt).exp(), (r2 * dt).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        };
        self.value = self.target + x;
        self.velocity = v;
    }
}

/// Values a [`Follow`] can move: anything that adds, subtracts and scales by
/// an `f32` (`f32`, `gfx3d::Vec3`).
pub trait Vector: Copy + Default + Add<Output = Self> + Sub<Output = Self> + Mul<f32, Output = Self> {}
impl<T: Copy + Default + Add<Output = T> + Sub<Output = T> + Mul<f32, Output = T>> Vector for T {}

/// `ω·halflife` for a critically damped spring: the x where (1 + x)·e^(−x) = ½.
/// A [`Follow`] with a given `halflife` has angular frequency
/// `ω = HALFLIFE_W / halflife`, and trails a target moving steadily at speed
/// v by `2·v/ω` = [`Follow::lag_for`]`(halflife, v)` ≈ 1.19·v·halflife.
pub const HALFLIFE_W: f32 = 1.678_347;

/// A critically damped spring following a moving target (what other engines
/// call SmoothDamp): use it for anything the camera follows, the camera
/// itself, and anything that chases something else on screen.
///
/// - Its position and velocity stay continuous whatever the target does: a
///   target that jumps gives a smooth start, a turn gives a curve, never a kink.
/// - It never overshoots a target that stops (from rest, or from following it
///   at a steady speed).
/// - It's solved in closed form, not stepped, and each `update` treats the
///   target as moving in a straight line from where it was at the last
///   update, so 30, 60, 120 and 144 Hz trace the same path. A target that
///   jumps (a creature switching phase) breaks that assumption: tell it with
///   [`Follow::jump_target`] in that frame (or [`Follow::snap`] for a cut).
///
/// `halflife` is the seconds it takes to close half the gap to a target that
/// jumped and holds still: 0.08 tight, 0.15 a chase camera, 0.3 lazy. A
/// target moving at a steady speed v is trailed by `lag(v)` = 1.19·v·halflife;
/// aim at `target + velocity * lag` instead to sit on it. Change `halflife`
/// between updates freely (pull back fast, catch up slowly).
/// ```
/// use maimbrain::gfx3d::{Vec3, vec3};
/// use maimbrain::motion::Follow;
/// let mut look = Follow::new(Vec3::ZERO, 0.15);
/// for i in 1..=60 {
///     let runner = vec3(0.0, 1.0, -10.0 * i as f32 / 60.0);   // 10 m/s down −z
///     look.update(runner, 1.0 / 60.0);
/// }
/// assert!((look.value.z - (-10.0 + look.lag(10.0))).abs() < 0.05);
/// ```
/// For angles, `update_angle` takes the short way round. For a rotation,
/// follow the point it looks at (`Follow<Vec3>` + `gfx3d::look_at`) or its yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follow<T: Vector = f32> {
    pub value: T,
    /// Units per second.
    pub velocity: T,
    pub halflife: f32,
    /// The target at the last update.
    target: T,
}

impl<T: Vector> Follow<T> {
    /// At rest at `value`.
    pub fn new(value: T, halflife: f32) -> Self {
        Follow { value, velocity: T::default(), halflife, target: value }
    }

    /// Jumps to `v` at rest (a cut, a respawn): the one discontinuity it has.
    pub fn snap(&mut self, v: T) {
        *self = Follow::new(v, self.halflife);
    }

    /// The target jumped to `target` (a creature's next phase, a new
    /// waypoint, a lure that teleports): follow it from here as a target
    /// that was there all along. `update` treats a target as moving in a
    /// straight line since the last update, so a jump reads as one frame of
    /// very fast motion, and the path after it depends on the frame rate
    /// (`mb shot --smoothness` reports the creature's position jumping, worse
    /// at one rate than the other). Call this in the frame the target jumps,
    /// before `update`: the value keeps its position and velocity and the
    /// path is the same at 60 and 120 Hz. [`Follow::snap`] jumps the value too.
    /// ```
    /// use maimbrain::motion::Follow;
    /// // The value 0.1 s after the target jumps from 0 to 5, at 60 and 120 Hz.
    /// let after = |hz: f32, told: bool| {
    ///     let mut f = Follow::new(0.0, 0.3);
    ///     if told { f.jump_target(5.0) }
    ///     for _ in 0..(hz / 10.0) as usize { f.update(5.0, 1.0 / hz); }
    ///     f.value
    /// };
    /// assert!((after(60.0, true) - after(120.0, true)).abs() < 1e-4);
    /// assert!((after(60.0, false) - after(120.0, false)).abs() > 0.02);
    /// ```
    pub fn jump_target(&mut self, target: T) {
        self.target = target;
    }

    /// The distance it trails a target moving steadily at `speed` (units/s).
    pub fn lag(&self, speed: f32) -> f32 {
        Self::lag_for(self.halflife, speed)
    }

    /// The distance a `Follow` with this `halflife` trails a target moving
    /// steadily at `speed`, without one at hand: for tuning (how far behind
    /// does a 0.15 s camera sit at 12 m/s?) or for aiming a camera built
    /// elsewhere (`gfx3d::SpringCamera`) ahead of what it follows.
    /// ```
    /// use maimbrain::motion::Follow;
    /// let lag = Follow::<f32>::lag_for(0.15, 12.0);
    /// assert!((lag - 2.145).abs() < 0.01, "{lag}");
    /// assert_eq!(lag, Follow::new(0.0, 0.15).lag(12.0));
    /// ```
    pub fn lag_for(halflife: f32, speed: f32) -> f32 {
        2.0 * speed * halflife.max(1e-4) / HALFLIFE_W
    }

    /// Moves `dt` seconds toward `target` (where the target is now); returns the value.
    pub fn update(&mut self, target: T, dt: f32) -> T {
        if dt <= 0.0 {
            return self.value;
        }
        let w = HALFLIFE_W / self.halflife.max(1e-4);
        // Over this step the target moves at u; a spring chasing that settles
        // `lead` behind it, and the gap e to there decays freely:
        // e(t) = (e0 + (e0' + w·e0)·t)·e^(−w·t).
        let u = (target - self.target) * (1.0 / dt);
        let lead = u * (2.0 / w);
        let (e0, e1) = (self.value - self.target + lead, self.velocity - u);
        let j = e1 + e0 * w;
        let k = (-w * dt).exp();
        self.value = target - lead + (e0 + j * dt) * k;
        self.velocity = u + (e1 - j * (w * dt)) * k;
        self.target = target;
        self.value
    }
}

impl Follow<f32> {
    /// `update` for an angle in radians: it follows `target` the short way
    /// round. `value` winds on past ±π with the target rather than jumping
    /// (draw with it as is, or `wrap_angle` it).
    pub fn update_angle(&mut self, target: f32, dt: f32) -> f32 {
        let target = self.target + wrap_angle(target - self.target);
        self.update(target, dt)
    }
}

/// `a` wrapped to −π…π.
pub fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

/// One value moved by a list of steps: `to` (ease there), `wait`, `set`.
/// ```
/// use maimbrain::motion::{Ease, Seq};
/// let mut s = Seq::new(0.0).to(1.0, 0.2, Ease::BackOut).wait(1.0).to(0.0, 0.3, Ease::QuadIn);
/// s.update(0.6);
/// assert_eq!(s.value(), 1.0);
/// ```
#[derive(Clone, Debug)]
pub struct Seq {
    start: f32,
    steps: Vec<(f32, f32, Ease)>,
    t: f32,
    pub looping: bool,
}

impl Seq {
    pub fn new(start: f32) -> Seq {
        Seq { start, steps: Vec::new(), t: 0.0, looping: false }
    }
    pub fn to(mut self, value: f32, duration: f32, ease: Ease) -> Seq {
        self.steps.push((value, duration.max(0.0), ease));
        self
    }
    pub fn wait(self, duration: f32) -> Seq {
        let v = self.end_value();
        self.to(v, duration, Ease::Linear)
    }
    /// Jumps to `value` instantly.
    pub fn set(self, value: f32) -> Seq {
        self.to(value, 0.0, Ease::Linear)
    }
    pub fn looped(mut self) -> Seq {
        self.looping = true;
        self
    }
    fn end_value(&self) -> f32 {
        self.steps.last().map_or(self.start, |s| s.0)
    }
    pub fn duration(&self) -> f32 {
        self.steps.iter().map(|s| s.1).sum()
    }
    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }
    pub fn restart(&mut self) {
        self.t = 0.0;
    }
    pub fn done(&self) -> bool {
        !self.looping && self.t >= self.duration()
    }
    pub fn value(&self) -> f32 {
        let total = self.duration();
        let mut t = if self.looping && total > 0.0 { self.t % total } else { self.t };
        let mut from = self.start;
        for &(to, d, e) in &self.steps {
            if t < d {
                return lerp(from, to, e.at(t / d));
            }
            t -= d;
            from = to;
        }
        from
    }
}

/// Progress 0…1 of item `index` in a staggered group: item i starts
/// `i × each` seconds after `t` = 0 and takes `duration`.
pub fn stagger(t: f32, index: usize, each: f32, duration: f32) -> f32 {
    ((t - index as f32 * each) / duration.max(1e-6)).clamp(0.0, 1.0)
}

/// Counts up from 0; `fired` when it passes `period` (and wraps).
#[derive(Clone, Copy, Debug, Default)]
pub struct Timer {
    pub t: f32,
    pub period: f32,
}

impl Timer {
    pub fn new(period: f32) -> Timer {
        Timer { t: 0.0, period }
    }
    /// True on the update that crosses the period (once per period).
    pub fn update(&mut self, dt: f32) -> bool {
        self.t += dt;
        if self.period > 0.0 && self.t >= self.period {
            self.t %= self.period;
            true
        } else {
            false
        }
    }
    pub fn progress(&self) -> f32 {
        if self.period > 0.0 { (self.t / self.period).min(1.0) } else { 0.0 }
    }
}

/// A smooth pseudo-random signal in −1…1 (value noise on a seeded lattice).
pub fn noise1(seed: u32, x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let h = |n: i32| {
        let mut v = (n as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA6B);
        v ^= v >> 15;
        v = v.wrapping_mul(0x2C1B_3C6D);
        v ^= v >> 12;
        (v & 0xffff) as f32 / 32767.5 - 1.0
    };
    let u = f * f * (3.0 - 2.0 * f);
    lerp(h(i as i32), h(i as i32 + 1), u)
}

/// Screen shake with "trauma": `add` some (0…1) on a hit; the shake grows
/// with trauma² and decays on its own. Apply it around a frame with
/// [`Shake::apply`] (inside `gfx2d::push`/`pop`).
#[derive(Clone, Copy, Debug)]
pub struct Shake {
    pub trauma: f32,
    /// Offset at full trauma, in logical units.
    pub max_offset: f32,
    /// Rotation at full trauma, in radians.
    pub max_angle: f32,
    /// Trauma lost per second.
    pub decay: f32,
    /// Shake frequency in Hz.
    pub frequency: f32,
    pub seed: u32,
    t: f32,
}

impl Default for Shake {
    fn default() -> Self {
        Shake::new()
    }
}

impl Shake {
    pub fn new() -> Shake {
        Shake { trauma: 0.0, max_offset: 14.0, max_angle: 0.06, decay: 1.8, frequency: 22.0, seed: 7, t: 0.0 }
    }
    /// Small hit ≈ 0.25, big hit ≈ 0.5, death ≈ 0.8.
    pub fn add(&mut self, trauma: f32) {
        self.trauma = (self.trauma + trauma).clamp(0.0, 1.0);
    }
    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        self.trauma = (self.trauma - self.decay * dt).max(0.0);
    }
    fn amount(&self) -> f32 {
        self.trauma * self.trauma
    }
    /// The current offset (dx, dy).
    pub fn offset(&self) -> (f32, f32) {
        let (k, x) = (self.amount() * self.max_offset, self.t * self.frequency);
        (k * noise1(self.seed, x), k * noise1(self.seed ^ 0x5bd1, x))
    }
    pub fn angle(&self) -> f32 {
        self.amount() * self.max_angle * noise1(self.seed ^ 0x27d4, self.t * self.frequency)
    }
    /// Translates and rotates about (cx, cy). Call between push and pop.
    pub fn apply(&self, cx: f32, cy: f32) {
        if self.trauma <= 0.0 {
            return;
        }
        let (dx, dy) = self.offset();
        crate::gfx2d::translate(cx + dx, cy + dy);
        crate::gfx2d::rotate(self.angle());
        crate::gfx2d::translate(-cx, -cy);
    }
}

/// A scale punch: `kick` makes `scale()` jump above 1 and spring back with
/// a wobble. Score labels, buttons, anything that just changed.
#[derive(Clone, Copy, Debug)]
pub struct Punch {
    pub spring: Spring,
}

impl Default for Punch {
    fn default() -> Self {
        Punch::new()
    }
}

impl Punch {
    pub fn new() -> Punch {
        Punch { spring: Spring::new(0.0, 4.5, 0.32) }
    }
    /// 0.15 subtle, 0.3 lively, 0.6 huge.
    pub fn kick(&mut self, amount: f32) {
        let cap = amount.abs() * 2.0;
        self.spring.value = (self.spring.value + amount).max(-cap).min(cap);
        self.spring.velocity = 0.0;
    }
    pub fn update(&mut self, dt: f32) {
        self.spring.update(dt);
    }
    pub fn scale(&self) -> f32 {
        1.0 + self.spring.value
    }
}

/// A number that rolls toward its target (scores, coins), with a punch each
/// time it's set. `update` says when the shown value changed (tick sounds).
#[derive(Clone, Copy, Debug)]
pub struct Counter {
    from: f64,
    target: i64,
    t: f32,
    duration: f32,
    pub ease: Ease,
    /// Longest roll, in seconds (bigger jumps roll longer, up to this).
    pub max_duration: f32,
    pub punch: Punch,
    last_shown: i64,
}

impl Counter {
    pub fn new(value: i64) -> Counter {
        Counter { from: value as f64, target: value, t: 1.0, duration: 1.0, ease: Ease::QuartOut, max_duration: 1.2, punch: Punch::new(), last_shown: value }
    }
    /// Rolls from what's shown now to `target`.
    pub fn set(&mut self, target: i64) {
        if target == self.target {
            return;
        }
        self.from = self.shown_f();
        let delta = (target as f64 - self.from).abs().max(1.0);
        self.duration = (0.25 + 0.22 * delta.log10() as f32).min(self.max_duration).max(0.2);
        self.target = target;
        self.t = 0.0;
        self.punch.kick(0.25);
    }
    /// Rolls with a fixed duration (e.g. a results screen's big roll-up).
    pub fn roll_to(&mut self, target: i64, duration: f32) {
        self.from = self.shown_f();
        self.target = target;
        self.duration = duration.max(1e-3);
        self.t = 0.0;
    }
    /// Shows `v` at once.
    pub fn snap(&mut self, v: i64) {
        self.from = v as f64;
        self.target = v;
        self.t = self.duration;
        self.last_shown = v;
    }
    /// Advances; true if the shown number changed.
    pub fn update(&mut self, dt: f32) -> bool {
        self.t += dt;
        self.punch.update(dt);
        let v = self.value();
        let changed = v != self.last_shown;
        self.last_shown = v;
        changed
    }
    fn shown_f(&self) -> f64 {
        let k = self.ease.at(self.t / self.duration) as f64;
        self.from + (self.target as f64 - self.from) * k
    }
    /// The number to draw.
    pub fn value(&self) -> i64 {
        if self.done() { self.target } else { self.shown_f().round() as i64 }
    }
    pub fn target(&self) -> i64 {
        self.target
    }
    pub fn done(&self) -> bool {
        self.t >= self.duration
    }
    pub fn scale(&self) -> f32 {
        self.punch.scale()
    }
}

/// Hit-stop (freeze frame): `freeze` for 50–80 ms on a big impact, and run
/// the simulation with `step(dt)`, which is 0 while frozen.
#[derive(Clone, Copy, Debug, Default)]
pub struct HitStop {
    left: f32,
}

impl HitStop {
    pub fn freeze(&mut self, seconds: f32) {
        self.left = self.left.max(seconds);
    }
    /// The dt the simulation should use this frame (what's left after the freeze ends).
    pub fn step(&mut self, dt: f32) -> f32 {
        let used = self.left.min(dt);
        self.left -= used;
        dt - used
    }
    pub fn frozen(&self) -> bool {
        self.left > 0.0
    }
}

/// A one-shot 1 → 0 fade (flashes, glows, "hit" tints): `fire`, `update`,
/// read `value()`.
#[derive(Clone, Copy, Debug)]
pub struct Pulse {
    pub duration: f32,
    pub ease: Ease,
    t: f32,
}

impl Pulse {
    pub fn new(duration: f32) -> Pulse {
        Pulse { duration: duration.max(1e-6), ease: Ease::QuadOut, t: f32::INFINITY }
    }
    pub fn fire(&mut self) {
        self.t = 0.0;
    }
    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }
    pub fn active(&self) -> bool {
        self.t < self.duration
    }
    /// 1 when fired, easing to 0 over `duration`.
    pub fn value(&self) -> f32 {
        if self.t >= self.duration { 0.0 } else { 1.0 - self.ease.at(self.t / self.duration) }
    }
    /// Seconds since `fire`.
    pub fn age(&self) -> f32 {
        self.t
    }
}

/// One key of [`PoseKeys`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key<T> {
    /// When it's reached, on the timeline's clock.
    pub at: f32,
    pub value: T,
    /// How the move from the key before eases into this one.
    pub ease: Ease,
    /// How long it's held after `at` before moving on.
    pub hold: f32,
    /// How hard it trembles while held (and as it's reached): see [`PoseKeys::tremble`].
    pub tremble: f32,
}

/// Poses (or any [`Lerp`] value: an angle, a `Vec3`, a `Quat`, your own
/// pose struct) keyed on a timeline, eased between keys, with holds and a
/// tremble. A pure function of the clock you pass, which can be seconds
/// since an attack started or a tell's progress 0–1, so the pose is a
/// function of the game's state and replays match at any frame rate.
///
/// ```
/// use maimbrain::motion::{Ease, PoseKeys};
/// // A lunge's tell (0–1 of the tell): ease into the windup, hold it cocked and trembling, then strike.
/// let arm = PoseKeys::new(0.0f32).to(0.6, -1.2, Ease::CubicInOut).hold(0.25).trembling(1.0).to(1.0, 0.4, Ease::QuadIn);
/// assert_eq!(arm.sample(0.0), 0.0);
/// assert_eq!(arm.sample(0.7), -1.2);           // held
/// assert!(arm.tremble(0.7, 40.0).abs() <= 1.0); // the shake that says "now"
/// assert_eq!(arm.sample(1.0), 0.4);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct PoseKeys<T: Lerp> {
    keys: Vec<Key<T>>,
}

impl<T: Lerp> PoseKeys<T> {
    /// Starts at `value` at time 0.
    pub fn new(value: T) -> PoseKeys<T> {
        PoseKeys { keys: vec![Key { at: 0.0, value, ease: Ease::Linear, hold: 0.0, tremble: 0.0 }] }
    }

    /// Moves to `value`, reaching it at `at` (after the previous key and its
    /// hold), eased by `ease`.
    pub fn to(mut self, at: f32, value: T, ease: Ease) -> PoseKeys<T> {
        let last = self.keys.last().map_or(0.0, |k| k.at + k.hold);
        self.keys.push(Key { at: at.max(last), value, ease, hold: 0.0, tremble: 0.0 });
        self
    }

    /// Holds the last key for `seconds` before the next move starts.
    pub fn hold(mut self, seconds: f32) -> PoseKeys<T> {
        if let Some(k) = self.keys.last_mut() {
            k.hold = seconds.max(0.0);
        }
        self
    }

    /// Makes the last key tremble with `amount` while held.
    pub fn trembling(mut self, amount: f32) -> PoseKeys<T> {
        if let Some(k) = self.keys.last_mut() {
            k.tremble = amount;
        }
        self
    }

    pub fn keys(&self) -> &[Key<T>] {
        &self.keys
    }

    /// When the last key (and its hold) ends.
    pub fn duration(&self) -> f32 {
        self.keys.last().map_or(0.0, |k| k.at + k.hold)
    }

    /// The value at `t`: the first key's before it, the last key's after.
    pub fn sample(&self, t: f32) -> T {
        let k = &self.keys;
        for w in k.windows(2) {
            let start = w[0].at + w[0].hold;
            if t <= start {
                return w[0].value;
            }
            if t < w[1].at {
                let u = (t - start) / (w[1].at - start).max(1e-6);
                return T::lerp(w[0].value, w[1].value, ease(w[1].ease, u));
            }
        }
        k[k.len() - 1].value
    }

    /// The tremble at `t`: a ±1 shake at `hz` (30–45 reads as straining
    /// metal) times the key's `tremble`, growing in over the last fifth of
    /// the move into that key and kept through its hold; 0 elsewhere.
    /// Scale a small turn by it (0.02–0.05 rad).
    pub fn tremble(&self, t: f32, hz: f32) -> f32 {
        let k = &self.keys;
        let mut amount = 0.0f32;
        for (i, key) in k.iter().enumerate() {
            if key.tremble == 0.0 || t > key.at + key.hold {
                continue;
            }
            let from = if i > 0 { k[i - 1].at + k[i - 1].hold } else { key.at };
            let ramp_start = key.at - 0.2 * (key.at - from);
            if t >= ramp_start {
                let r = if key.at > ramp_start { ((t - ramp_start) / (key.at - ramp_start)).clamp(0.0, 1.0) } else { 1.0 };
                amount = amount.max(key.tremble * r);
            }
        }
        // Two incommensurate sines: a shake, not a hum.
        amount * (0.7 * (std::f32::consts::TAU * hz * t).sin() + 0.3 * (std::f32::consts::TAU * hz * 1.618 * t + 1.3).sin())
    }
}

/// A spring's response to a kick, `t` seconds after it: 0 before, up to 1
/// at its first peak, then settling back to 0. A pure function of the time
/// since the kick (unlike [`Spring::kick`], which integrates `dt`s), so a
/// replay or a 120 Hz phone gets the same curve: strike stops, hit recoils,
/// landing squash. `hz` is how fast it swings (8–25 for a heavy mech's
/// parts), `damping` its damping ratio: 1 or more settles without
/// overshooting (critically damped), 0.2–0.5 wobbles back past 0 once or
/// twice.
pub fn kick(t: f32, hz: f32, damping: f32) -> f32 {
    if !(t > 0.0) || !(hz > 0.0) {
        return 0.0;
    }
    let w = std::f32::consts::TAU * hz;
    if damping >= 0.999 {
        // Critically damped: w·t·e^(1 − w·t) peaks at 1 when w·t = 1.
        let x = w * t;
        return x * (1.0 - x).exp();
    }
    let z = damping.max(0.0);
    let wd = w * (1.0 - z * z).sqrt();
    let peak_t = (wd / (z * w).max(1e-6)).atan() / wd;
    let peak = (-z * w * peak_t).exp() * (wd * peak_t).sin();
    (-z * w * t).exp() * (wd * t).sin() / peak.max(1e-6)
}

/// A kick at a moment, as data: `amount × kick(t − at, hz, damping)`. Keep
/// the time something happened (a hit's time in the fight), not a spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kick {
    /// When it hit, on your clock.
    pub at: f32,
    pub amount: f32,
    pub hz: f32,
    pub damping: f32,
}

impl Kick {
    /// A kick of `amount` at `at` that swings at `hz` with a 0.3 damping ratio.
    pub fn new(at: f32, amount: f32, hz: f32) -> Kick {
        Kick { at, amount, hz, damping: 0.3 }
    }
    pub fn with_damping(mut self, damping: f32) -> Kick {
        self.damping = damping;
        self
    }
    /// Its value at `t` (your clock).
    pub fn value(&self, t: f32) -> f32 {
        self.amount * kick(t - self.at, self.hz, self.damping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_keys_ease_hold_and_tremble() {
        let k = PoseKeys::new(0.0f32).to(1.0, 10.0, Ease::Linear).hold(0.5).trembling(1.0).to(2.0, 0.0, Ease::QuadIn);
        assert_eq!(k.duration(), 2.0);
        assert_eq!((k.sample(-1.0), k.sample(0.5), k.sample(1.2), k.sample(1.5), k.sample(9.0)), (0.0, 5.0, 10.0, 10.0, 0.0));
        assert!((k.sample(1.75) - 7.5).abs() < 1e-5, "quad-in: slow off the hold {}", k.sample(1.75));
        assert_eq!(k.tremble(0.5, 40.0), 0.0, "no shake early in the move");
        let shake = (0..100).map(|i| k.tremble(1.0 + i as f32 * 0.005, 40.0).abs()).fold(0.0f32, f32::max);
        assert!(shake > 0.5 && shake <= 1.0, "{shake}");
        assert_eq!(k.tremble(1.6, 40.0), 0.0, "stops when the hold ends");
        // A key earlier than the one before is pushed after it.
        let late = PoseKeys::new(0.0f32).to(1.0, 1.0, Ease::Linear).hold(1.0).to(0.5, 2.0, Ease::Linear);
        assert_eq!(late.keys()[2].at, 2.0);
    }

    #[test]
    fn kicks_are_pure_functions_of_time() {
        assert_eq!(kick(-0.1, 10.0, 0.3), 0.0);
        // Critically damped: peaks at 1 at 1/ω, never negative.
        let w = std::f32::consts::TAU * 10.0;
        assert!((kick(1.0 / w, 10.0, 1.0) - 1.0).abs() < 1e-5);
        assert!((0..200).all(|i| kick(i as f32 * 0.005, 10.0, 1.0) >= 0.0));
        // Underdamped: peaks at 1, swings back below 0, settles.
        let max = (0..400).map(|i| kick(i as f32 * 0.001, 5.0, 0.3)).fold(0.0f32, f32::max);
        let min = (0..400).map(|i| kick(i as f32 * 0.001, 5.0, 0.3)).fold(0.0f32, f32::min);
        assert!((max - 1.0).abs() < 1e-3 && min < -0.2, "{max} {min}");
        assert!(kick(3.0, 5.0, 0.3).abs() < 1e-3);
        let k = Kick::new(2.0, 0.5, 8.0);
        assert_eq!(k.value(1.0), 0.0);
        assert_eq!(k.value(2.1), 0.5 * kick(2.1 - 2.0, 8.0, 0.3));
    }

    #[test]
    fn every_ease_starts_at_0_and_ends_at_1() {
        for e in Ease::ALL {
            assert!(e.at(0.0).abs() < 1e-5, "{e:?}(0) = {}", e.at(0.0));
            assert!((e.at(1.0) - 1.0).abs() < 1e-5, "{e:?}(1) = {}", e.at(1.0));
            assert!(e.at(0.5).is_finite());
        }
        // Symmetric in/outs pass through the middle.
        for e in [Ease::QuadInOut, Ease::CubicInOut, Ease::SineInOut, Ease::ExpoInOut, Ease::BackInOut, Ease::ElasticInOut, Ease::BounceInOut] {
            assert!((e.at(0.5) - 0.5).abs() < 1e-4, "{e:?}");
        }
        assert!(Ease::BackOut.at(0.6) > 1.0, "back overshoots");
        assert!(Ease::BackIn.at(0.2) < 0.0, "back pulls back first");
        assert!((0..=100).all(|i| (0.0..=1.0001).contains(&Ease::BounceOut.at(i as f32 / 100.0))));
    }

    #[test]
    fn tweens_delay_repeat_and_retarget() {
        let mut t = Tween::new(10.0, 20.0, 1.0, Ease::Linear).delay(0.5);
        t.update(0.5);
        assert_eq!(t.value(), 10.0);
        t.update(0.5);
        assert!((t.value() - 15.0).abs() < 1e-5);
        t.update(1.0);
        assert!(t.done() && t.value() == 20.0);
        t.retarget(0.0, 2.0);
        t.update(1.0);
        assert!((t.value() - 10.0).abs() < 1e-5);
        let mut p = Tween::new(0.0, 1.0, 1.0, Ease::Linear).repeat(Repeat::PingPong);
        p.update(1.5);
        assert!((p.value() - 0.5).abs() < 1e-5 && !p.done());
        let mut v = Tween::new([0.0, 0.0], [2.0, 4.0], 1.0, Ease::Linear);
        v.update(0.5);
        assert_eq!(v.value(), [1.0, 2.0]);
    }

    #[test]
    fn springs_settle_without_blowing_up_at_any_dt() {
        for damping in [0.2, 0.4, 1.0, 2.0] {
            for dt in [1.0 / 120.0, 1.0 / 60.0, 0.1] {
                let mut s = Spring::new(0.0, 3.0, damping).to(100.0);
                let mut peak: f32 = 0.0;
                for _ in 0..(4.0 / dt) as usize {
                    s.update(dt);
                    peak = peak.max(s.value);
                }
                assert!((s.value - 100.0).abs() < 1.0, "damping {damping} dt {dt}: {}", s.value);
                if damping >= 1.0 {
                    assert!(peak <= 100.0 + 1e-3, "critical/over-damped doesn't overshoot ({peak})");
                } else {
                    assert!(peak > 100.0, "bouncy overshoots");
                }
            }
        }
        // Same result whether stepped in one go or in pieces of the same total (exact integration).
        let (mut a, mut b) = (Spring::bouncy(0.0, 2.0).to(1.0), Spring::bouncy(0.0, 2.0).to(1.0));
        a.update(0.1);
        b.update(0.05);
        b.update(0.05);
        assert!((a.value - b.value).abs() < 1e-4);
    }

    use crate::gfx3d::{Vec3, vec3};

    /// A runner: 10 m/s down −z, a sharp 90° turn to +x at 1 s (its velocity
    /// snaps), a weave from 2 s, and a stop at 3 s.
    fn runner(t: f32) -> Vec3 {
        let (a, b, c) = (t.min(1.0), (t - 1.0).clamp(0.0, 2.0), (t - 2.0).clamp(0.0, 1.0));
        vec3(10.0 * b, 0.0, -10.0 * a + 0.8 * (c * std::f32::consts::TAU).sin())
    }

    /// Follows `runner` at `hz` for 4 s; the value at every 1/6 s.
    fn follow_at(hz: u32) -> Vec<Vec3> {
        let mut f = Follow::new(Vec3::ZERO, 0.15);
        let dt = 1.0 / hz as f32;
        let mut out = Vec::new();
        for i in 1..=4 * hz {
            f.update(runner(i as f32 / hz as f32), dt);
            if i % (hz / 6) == 0 {
                out.push(f.value);
            }
        }
        out
    }

    #[test]
    fn follow_traces_the_same_path_at_any_frame_rate() {
        let reference = follow_at(144);
        for hz in [30, 60, 120] {
            for (k, (a, b)) in follow_at(hz).iter().zip(&reference).enumerate() {
                assert!(a.distance(*b) < 0.01, "{hz} Hz at {:.2} s: {a:?} vs {b:?}", (k + 1) as f32 / 6.0);
            }
        }
    }

    #[test]
    fn follow_halflife_lag_and_no_overshoot() {
        for hz in [30.0f32, 60.0, 120.0, 144.0] {
            let dt = 1.0 / hz;
            // A target that's there already: half the gap is gone after one halflife.
            let mut f = Follow::new(0.0, 1.0 / 6.0);
            f.target = 1.0;
            for _ in 0..(hz / 6.0).round() as usize {
                f.update(1.0, dt);
            }
            assert!((f.value - 0.5).abs() < 1e-3, "{hz} Hz: {}", f.value);
            // From rest to a target that jumps: no overshoot, ever.
            let mut f = Follow::new(0.0, 0.1);
            let mut last = 0.0;
            for _ in 0..(3.0 * hz) as usize {
                f.update(1.0, dt);
                assert!(f.value >= last - 1e-6 && f.value <= 1.0 + 1e-6, "{hz} Hz: {}", f.value);
                last = f.value;
            }
            // Chasing a steady target, then the target stops: trails by lag(),
            // then settles without passing it.
            let mut f = Follow::new(0.0, 0.15);
            let mut t = 0.0;
            while t < 2.0 {
                t += dt;
                f.update(5.0 * t, dt);
            }
            assert!((5.0 * t - f.value - f.lag(5.0)).abs() < 1e-3, "{hz} Hz lag");
            let stop = 5.0 * t;
            for _ in 0..(2.0 * hz) as usize {
                f.update(stop, dt);
                assert!(f.value <= stop + 1e-5, "{hz} Hz: passed a stopped target");
            }
        }
    }

    #[test]
    fn follow_stays_smooth_where_first_order_smoothing_kinks() {
        // Peak acceleration (second difference / dt²) following a target that
        // jumps 1 unit. A follow's is the same at 60 and 120 Hz: its velocity
        // is continuous. `damp`'s doubles with the rate: its velocity jumps.
        let peak = |hz: f32, spring: bool| {
            let dt = 1.0 / hz;
            let (mut f, mut d) = (Follow::new(0.0, 0.15), 0.0);
            let mut xs = vec![0.0, 0.0];
            for _ in 0..hz as usize {
                xs.push(if spring { f.update(1.0, dt) } else { d = damp(d, 1.0, 4.6, dt); d });
            }
            xs.windows(3).map(|w| ((w[2] - 2.0 * w[1] + w[0]) / (dt * dt)).abs()).fold(0.0, f32::max)
        };
        let (s60, s120) = (peak(60.0, true), peak(120.0, true));
        assert!((s120 / s60 - 1.0).abs() < 0.15, "follow: {s60} vs {s120}");
        let (d60, d120) = (peak(60.0, false), peak(120.0, false));
        assert!(d120 / d60 > 1.8, "damp: {d60} vs {d120}");
    }

    #[test]
    fn follow_angle_takes_the_short_way_round() {
        let mut f = Follow::new(3.0, 0.1);
        for _ in 0..120 {
            f.update_angle(-3.0, 1.0 / 60.0);
        }
        // −3 is 0.28 rad past +π from 3: it goes up through π, not down through 0.
        assert!((wrap_angle(f.value) + 3.0).abs() < 1e-3 && f.value > 3.0, "{}", f.value);
        assert!((wrap_angle(7.0) - (7.0 - std::f32::consts::TAU)).abs() < 1e-5);
    }

    #[test]
    fn sequences_and_staggers() {
        let mut s = Seq::new(0.0).to(1.0, 1.0, Ease::Linear).wait(1.0).set(5.0).to(0.0, 1.0, Ease::Linear);
        assert_eq!(s.duration(), 3.0);
        s.update(0.5);
        assert!((s.value() - 0.5).abs() < 1e-5);
        s.update(1.0);
        assert_eq!(s.value(), 1.0);
        s.update(1.0);
        assert!((s.value() - 2.5).abs() < 1e-5);
        assert!((stagger(0.3, 2, 0.1, 0.2) - 0.5).abs() < 1e-5);
        assert_eq!(stagger(0.0, 2, 0.1, 0.2), 0.0);
    }

    #[test]
    fn counters_roll_and_report_ticks() {
        let mut c = Counter::new(0);
        c.set(1000);
        let mut ticks = 0;
        let mut last = 0;
        for _ in 0..120 {
            if c.update(1.0 / 60.0) {
                ticks += 1;
            }
            assert!(c.value() >= last, "rolls up monotonically");
            last = c.value();
        }
        assert_eq!(c.value(), 1000);
        assert!(ticks > 10);
        assert!(c.done());
    }

    #[test]
    fn hit_stop_eats_dt_then_releases_it() {
        let mut h = HitStop::default();
        h.freeze(0.05);
        assert_eq!(h.step(1.0 / 60.0), 0.0);
        assert_eq!(h.step(1.0 / 60.0), 0.0);
        let rest = h.step(1.0 / 60.0);
        assert!(rest > 0.0 && rest < 1.0 / 60.0);
        assert_eq!(h.step(0.016), 0.016);
    }

    #[test]
    fn shake_decays_and_noise_is_bounded() {
        let mut s = Shake::new();
        s.add(1.0);
        s.update(0.1);
        let (dx, dy) = s.offset();
        assert!(dx.abs() <= s.max_offset && dy.abs() <= s.max_offset);
        for _ in 0..60 {
            s.update(1.0 / 60.0);
        }
        assert_eq!(s.trauma, 0.0);
        assert_eq!(s.offset(), (0.0, 0.0));
        assert!((0..1000).all(|i| noise1(3, i as f32 * 0.37).abs() <= 1.0));
    }
}
