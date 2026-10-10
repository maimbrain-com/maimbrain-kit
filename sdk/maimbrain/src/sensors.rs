//! Device sensors (SPEC §5.7). Declare each one you use in the manifest's
//! `sensors`; list the ones the game can't be played without in `needs`.
//! Sensor readings are recorded per frame, so replays reproduce them.

use crate::ffi;

/// Gravity in the game's axes, in g: +x right, +y down the screen, +z out of
/// the screen toward the player. Held upright in portrait it reads about
/// (0, 1, 0); lying flat face-up, (0, 0, −1). Its length is about 1 when the
/// phone is still.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Tilt {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Tilt {
    /// Sideways tip in radians, −π/2…π/2: positive when the right edge is
    /// lower. Works the same held upright, slumped or lying flat, so it's the
    /// usual control angle for "tip left/right". Compare against the value
    /// when play started (or each level began) to make it relative.
    pub fn roll(self) -> f32 {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if len < 1e-3 { 0.0 } else { (self.x / len).clamp(-1.0, 1.0).asin() }
    }

    /// Forward/back tip in radians, 0 upright to π/2 lying flat face-up
    /// (negative past upright, face toward the floor), ignoring sideways tip.
    pub fn pitch(self) -> f32 {
        (-self.z).atan2(self.y)
    }
}

/// This frame's tilt (the same value for every call within a frame), or
/// `None` before the first sample has arrived. Needs `sensors = ["tilt"]`.
pub fn tilt() -> Option<Tilt> {
    let mut t = Tilt::default();
    let r = unsafe { ffi::mb_tilt(&mut t as *mut Tilt as *mut u8) };
    (r >= 0).then_some(t)
}

/// Acceleration without gravity, in g, in the same axes as [`Tilt`]: what the
/// player's hand is doing (shakes, jabs, flicks). About zero when the phone
/// is still, ±1–3 g in a vigorous shake, briefly more on a hard jolt.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Motion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Motion {
    /// Length of the acceleration vector, in g.
    pub fn magnitude(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// This frame's motion (one snapshot per frame), or `None` before the first
/// sample. Needs `sensors = ["motion"]`. A frame is ~1/60 s, so sum or peak-
/// track readings over time to measure a shake rather than reacting to one.
pub fn motion() -> Option<Motion> {
    let mut m = Motion::default();
    let r = unsafe { ffi::mb_motion(&mut m as *mut Motion as *mut u8) };
    (r >= 0).then_some(m)
}

/// The phone's orientation and how fast it's turning: what a game needs to
/// aim by pointing the phone (turning it left/right, which gravity can't see).
///
/// `q` is a unit quaternion `[x, y, z, w]` that turns the phone's axes (the
/// game's axes, as [`Tilt`]: +x right, +y down the screen, +z out of the
/// screen) into the world's: +y straight down, x and z level. The world's
/// heading is arbitrary (there's no compass): compare against a reference
/// taken when play starts ([`Attitude::relative_to`], or let [`Scope`] do
/// it), and expect it to drift slowly (around a degree a minute). `rate` is
/// the rotation rate in radians per second about the phone's x, y and z
/// axes (positive turns y toward z, z toward x and x toward y): upright and
/// turning right, `rate[1]` is negative.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Attitude {
    pub q: [f32; 4],
    pub rate: [f32; 3],
}

impl Default for Attitude {
    fn default() -> Self {
        Attitude { q: [0.0, 0.0, 0.0, 1.0], rate: [0.0; 3] }
    }
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn qaxis(axis: [f32; 3], angle: f32) -> [f32; 4] {
    let (s, c) = (angle * 0.5).sin_cos();
    [axis[0] * s, axis[1] * s, axis[2] * s, c]
}

impl Attitude {
    /// The attitude `yaw` radians to the right of the world's heading,
    /// `pitch` radians up (the back of the phone pointing above level) and
    /// `roll` radians right edge down, from upright in portrait, not turning.
    /// For tests and bots: [`yaw`](Self::yaw), [`pitch`](Self::pitch) and
    /// [`roll`](Self::roll) read the same angles back.
    pub fn from_angles(yaw: f32, pitch: f32, roll: f32) -> Attitude {
        let q = qmul(qmul(qaxis([0.0, -1.0, 0.0], yaw), qaxis([1.0, 0.0, 0.0], -pitch)), qaxis([0.0, 0.0, 1.0], roll));
        Attitude { q, rate: [0.0; 3] }
    }

    /// A vector in the phone's axes, in the world's.
    pub fn rotate(self, v: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = self.q;
        // v + w·t + u×t, where t = 2·u×v and u = (x, y, z).
        let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
        [
            v[0] + w * t[0] + (y * t[2] - z * t[1]),
            v[1] + w * t[1] + (z * t[0] - x * t[2]),
            v[2] + w * t[2] + (x * t[1] - y * t[0]),
        ]
    }

    /// Where the back of the phone points, in the world (a unit vector):
    /// what a camera on its back would look at.
    pub fn forward(self) -> [f32; 3] {
        self.rotate([0.0, 0.0, -1.0])
    }

    /// Heading in radians, −π…π, positive turned right. Measured from an
    /// arbitrary start that drifts slowly, so only differences mean anything
    /// (see [`relative_to`](Self::relative_to)). Taken from where the back of
    /// the phone points; within ~35° of lying flat (or pointing straight up)
    /// it blends over to where the top (or bottom) edge points, so it stays
    /// steady all the way to flat.
    pub fn yaw(self) -> f32 {
        let f = self.forward();
        let top = self.rotate([0.0, -1.0, 0.0]);
        let unit = |x: f32, z: f32| {
            let l = x.hypot(z);
            if l > 1e-6 { (x / l, z / l) } else { (0.0, 0.0) }
        };
        let (fx, fz) = unit(f[0], f[2]);
        let side = if f[1] >= 0.0 { 1.0 } else { -1.0 };
        let (tx, tz) = unit(top[0] * side, top[2] * side);
        let u = ((f[1].abs() - 0.8) / 0.18).clamp(0.0, 1.0);
        let w = u * u * (3.0 - 2.0 * u);
        (fx * (1.0 - w) + tx * w).atan2(-(fz * (1.0 - w) + tz * w))
    }

    /// Radians the back of the phone points above level, −π/2…π/2: 0 held
    /// upright, −π/2 lying flat face-up. (The opposite sign to
    /// [`Tilt::pitch`], which measures how far the screen tips back.)
    pub fn pitch(self) -> f32 {
        (-self.forward()[1]).clamp(-1.0, 1.0).asin()
    }

    /// Sideways tip in radians: positive when the right edge is lower, as
    /// [`Tilt::roll`].
    pub fn roll(self) -> f32 {
        self.rotate([1.0, 0.0, 0.0])[1].clamp(-1.0, 1.0).asin()
    }

    /// Gravity in the phone's axes, as [`tilt`] reads it (1 g).
    pub fn gravity(self) -> Tilt {
        let [x, y, z, w] = self.q;
        let g = Attitude { q: [-x, -y, -z, w], rate: self.rate }.rotate([0.0, 1.0, 0.0]);
        Tilt { x: g[0], y: g[1], z: g[2] }
    }

    /// This attitude with the world turned so `reference`'s heading is yaw
    /// 0: its [`yaw`](Self::yaw) is then how far the player has turned since
    /// the reference was taken. Pitch, roll and rate are unchanged (they're
    /// measured from gravity, which doesn't drift); compare pitches directly.
    pub fn relative_to(self, reference: Attitude) -> Attitude {
        Attitude { q: qmul(qaxis([0.0, -1.0, 0.0], -reference.yaw()), self.q), rate: self.rate }
    }

    /// This attitude with the rotation rate that turns `from` into it over
    /// `dt` seconds, as the sensor would report it (for synthesized samples).
    pub fn with_rate_from(self, from: Attitude, dt: f32) -> Attitude {
        let [x, y, z, w] = from.q;
        let mut d = qmul([-x, -y, -z, w], self.q);
        if d[3] < 0.0 {
            d = d.map(|c| -c);
        }
        let k = if dt > 0.0 { 2.0 / dt } else { 0.0 };
        Attitude { q: self.q, rate: [d[0] * k, d[1] * k, d[2] * k] }
    }
}

/// This frame's attitude (one snapshot per frame), or `None` before the
/// first sample or on a device without a gyroscope (or with it switched
/// off). Needs `sensors = ["attitude"]`.
pub fn attitude() -> Option<Attitude> {
    let mut a = Attitude::default();
    let r = unsafe { ffi::mb_attitude(&mut a as *mut Attitude as *mut u8) };
    (r >= 0).then_some(a)
}

/// Where a [`Scope`]'s aim came from on its last update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AimSource {
    /// Pointing the phone (the `attitude` sensor).
    Attitude,
    /// Tipping it (no gyroscope: `tilt` only): left/right is roll.
    Tilt,
    /// Neither sensor: only [`Scope::drag`] moves the aim. Tell the player.
    Drag,
}

/// How a [`Scope`] smooths the aim. It works on the angles (after the dead
/// zone), so the crosshair never smooths across a recenter or an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Smoothing {
    /// A fixed low-pass with this time constant in seconds (0: none). Every
    /// movement, fast or slow, trails the hand by about this much: more than
    /// ~0.01 feels like dragging the crosshair on a string.
    Fixed(f32),
    /// Speed-adaptive smoothing: the One Euro filter (Casiez, Roussel and
    /// Vogel, CHI 2012). The cutoff is `min_cutoff` Hz while the phone is
    /// still and rises by `beta` Hz for every °/s it turns, so a resting
    /// hand's tremor is smoothed hard while a swing passes almost
    /// untouched. Lower `min_cutoff` steadies a resting aim (and makes slow
    /// corrections trail a little more); higher `beta` takes lag off swings
    /// (and lets more tremor through while moving). The turn speed it
    /// reads is itself smoothed (3 Hz), so tremor's back-and-forth averages
    /// out instead of opening the filter.
    Adaptive { min_cutoff: f32, beta: f32 },
}

impl Smoothing {
    /// [`Scope::precise`]'s, `Adaptive { min_cutoff: 1.5, beta: 0.5 }`: still, a
    /// ~106 ms time constant, which cuts a resting hand's 8–12 Hz tremor to
    /// about a third; a slow 10°/s correction trails by ~25 ms; turning
    /// 60°/s, 31.5 Hz (~5 ms, under one 60 Hz frame); 120°/s, ~2.6 ms.
    pub const PRECISE: Smoothing = Smoothing::Adaptive { min_cutoff: 1.5, beta: 0.5 };

    /// Its time constant in seconds while turning at `speed` °/s.
    pub fn time_constant(self, speed: f32) -> f32 {
        match self {
            Smoothing::Fixed(t) => t.max(0.0),
            Smoothing::Adaptive { min_cutoff, beta } => {
                let fc = min_cutoff.max(0.0) + beta.max(0.0) * speed.max(0.0);
                if fc > 0.0 { 1.0 / (std::f32::consts::TAU * fc) } else { f32::INFINITY }
            }
        }
    }
}

/// How a [`Scope`]'s gain depends on how fast the phone turns ("gyro
/// acceleration", a precision zone): slow, careful corrections move the
/// crosshair less per degree than swings, so homing in is finer without
/// making a swing across the screen any longer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainCurve {
    /// The gain's multiplier for slow turns.
    pub slow: f32,
    /// At or below this speed (°/s) the multiplier is `slow`.
    pub slow_below: f32,
    /// At or above this speed (°/s) it's 1, the full gain; in between it
    /// eases from one to the other (smoothstep, no kink).
    pub full_above: f32,
}

impl GainCurve {
    /// The same gain at every speed (the default).
    pub const FLAT: GainCurve = GainCurve { slow: 1.0, slow_below: 0.0, full_above: 0.0 };
    /// [`Scope::precise`]'s: ×0.6 below 10°/s (a hand homing in on a target turns
    /// ~2–15°/s), easing to ×1 at 50°/s (a swing onto one is 60–200°/s).
    pub const PRECISE: GainCurve = GainCurve { slow: 0.6, slow_below: 10.0, full_above: 50.0 };

    /// The multiplier at a turn of `speed` °/s.
    pub fn at(&self, speed: f32) -> f32 {
        let span = self.full_above - self.slow_below;
        let u = if span > 0.0 { ((speed - self.slow_below) / span).clamp(0.0, 1.0) } else if speed >= self.full_above { 1.0 } else { 0.0 };
        self.slow + (1.0 - self.slow) * u * u * (3.0 - 2.0 * u)
    }
}

/// Something to aim at, for a [`Scope`]'s friction: a circle in the
/// scope's area coordinates (where [`Scope::position`] is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub pos: (f32, f32),
    pub radius: f32,
}

impl Target {
    pub fn new(x: f32, y: f32, radius: f32) -> Target {
        Target { pos: (x, y), radius }
    }
}

/// How fast (Hz) the turn speed a [`Scope`] reads is smoothed.
const SPEED_CUTOFF: f32 = 3.0;

/// A light-gun "scope": a crosshair the player aims by pointing the phone.
///
/// By default it's the simple light-gun mapping, which a playtest on device
/// preferred: turning the phone moves the crosshair in proportion (`gain`,
/// 32 degrees of turn sweep the screen's width; the same scale up and
/// down), every degree the same at any speed, with a light fixed smoothing
/// (8 ms) and a tiny backlash dead zone (wobble smaller than it moves
/// nothing). The crosshair goes where the phone points.
///
/// It takes its reference (where "center" is) on the first reading and on
/// [`recenter`](Self::recenter) (call it when play starts and on a recenter
/// gesture); a crosshair resting near the center (and not on a target)
/// drifts slowly back to it, soaking up the gyro's slow heading drift;
/// turning past an edge holds the crosshair there with a little give, and
/// turning back moves it off the edge at once (like a mouse at a screen
/// edge). Without a gyroscope it falls back to tilt, and without either to
/// dragging ([`drag`](Self::drag), which also nudges the aim when there is a
/// sensor).
///
/// [`Scope::precise`] turns on the standard gyro-aim precision tricks, for
/// games where fine aiming at small targets is the point:
/// - speed-adaptive smoothing ([`Smoothing::PRECISE`], a One Euro filter):
///   a resting hand's tremor is smoothed away, a swing isn't held back;
/// - a gain curve ([`GainCurve::PRECISE`]): slow corrections move the
///   crosshair 0.6× as far per degree as swings (with 40° across at full
///   gain);
/// - friction ([`set_targets`](Self::set_targets)): over a target the gain
///   drops (×0.6), so a swing stops on it more easily and small corrections
///   on it are finer. It only scales the hand's own movement: the crosshair
///   never moves by itself (unless you turn on [`magnetism`](Self::magnetism)).
///
/// Each is also a field you can set on its own. With the gain curve or
/// friction on, the crosshair adds up the hand's (gained) movements, so
/// where it sits isn't a fixed function of where the phone points: a slow
/// turn right and a fast turn back leave it left of where it started
/// (recentering and the drift back to the center take that up).
///
/// ```
/// use maimbrain::sensors::{Attitude, Scope};
/// let mut scope = Scope::new(360.0, 640.0);
/// let d = std::f32::consts::PI / 180.0;
/// scope.update_with(1.0 / 60.0, Some(Attitude::from_angles(0.0, 0.0, 0.0)), None);
/// for _ in 0..10 {
///     scope.update_with(1.0 / 60.0, Some(Attitude::from_angles(8.0 * d, 0.0, 0.0)), None);
/// }
/// let (x, y) = scope.position();
/// assert!((x - (180.0 + 89.0)).abs() < 1.0 && (y - 320.0).abs() < 1.0); // 8° of a 32° sweep: a quarter of the width
/// ```
#[derive(Clone, Debug)]
pub struct Scope {
    /// Degrees of turn that sweep the crosshair across the screen's width at
    /// full gain (default 32, so a comfortable ±16° wrist turn reaches both
    /// edges; [`precise`](Self::precise): 40; 30–45 feels like pointing).
    pub gain: f32,
    /// Degrees of wobble ignored (default 0.1): a steady hand holds still.
    pub dead_zone: f32,
    /// How the aim is smoothed (default `Smoothing::Fixed(0.008)`;
    /// [`precise`](Self::precise): [`Smoothing::PRECISE`]).
    pub smoothing: Smoothing,
    /// How the gain depends on turn speed (default [`GainCurve::FLAT`], none;
    /// [`precise`](Self::precise): [`GainCurve::PRECISE`]).
    pub curve: GainCurve,
    /// How much the gain drops over a target (from
    /// [`set_targets`](Self::set_targets)), 0…1, easing back to none at 1.5×
    /// its radius (default 0, off; [`precise`](Self::precise): 0.4, ×0.6 on it).
    pub friction: f32,
    /// A pull toward the target under the crosshair, as a fraction of the
    /// hand's own movement (default 0: off). It only acts while the hand
    /// moves, so a still crosshair stays put; 0.1–0.3 is a gentle assist.
    pub magnetism: f32,
    /// Seconds for a crosshair resting near the center to settle back to it
    /// (default 8; 0 turns drift correction off).
    pub recenter_secs: f32,
    /// How far past an edge the crosshair can stretch, as a fraction of the
    /// width (default 0.03).
    pub give: f32,
    size: (f32, f32),
    source: Option<AimSource>,
    /// The reading that is the center (degrees: yaw/pitch, or roll/pitch).
    reference: Option<(f32, f32)>,
    /// The last reading, from the reference (degrees, yaw wrapped to ±180).
    last: Option<(f32, f32)>,
    /// Degrees turned from the reference, unwrapped (turning through ±180° is just more turn).
    turned: (f32, f32),
    /// `turned` after the dead zone.
    held: (f32, f32),
    /// `held` smoothed.
    filt: (f32, f32),
    /// `held`'s rate of turn (°/s), smoothed.
    vel: (f32, f32),
    /// The crosshair from the center: the sum of the hand's gained movements, drags and corrections.
    aim: (f32, f32),
    /// Where it's drawn, from the center (`aim` with the edges' give).
    pos: (f32, f32),
    targets: Vec<Target>,
    /// Last update's gain multiplier, and smoothing time constant.
    scale: f32,
    tau: f32,
}

/// An angle in degrees in −180…180.
fn wrap_deg(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

impl Scope {
    /// A scope over a `width` × `height` area (logical units), with the
    /// simple light-gun mapping: the crosshair starts in its middle.
    pub fn new(width: f32, height: f32) -> Scope {
        Scope {
            gain: 32.0,
            dead_zone: 0.1,
            smoothing: Smoothing::Fixed(0.008),
            curve: GainCurve::FLAT,
            friction: 0.0,
            magnetism: 0.0,
            recenter_secs: 8.0,
            give: 0.03,
            size: (width, height),
            source: None,
            reference: None,
            last: None,
            turned: (0.0, 0.0),
            held: (0.0, 0.0),
            filt: (0.0, 0.0),
            vel: (0.0, 0.0),
            aim: (0.0, 0.0),
            pos: (0.0, 0.0),
            targets: Vec::new(),
            scale: 1.0,
            tau: 0.0,
        }
    }

    /// A scope with the precision tricks on: 40° across at full gain,
    /// [`Smoothing::PRECISE`], [`GainCurve::PRECISE`] and friction 0.4 on
    /// the targets you give it with [`set_targets`](Self::set_targets).
    /// Try it when aiming finely at small targets is the game (a sniper, a
    /// shooting gallery of tiny targets), and compare it with [`new`](Self::new)
    /// on a phone: the playtest preferred the simple mapping for general aiming.
    pub fn precise(width: f32, height: f32) -> Scope {
        Scope { gain: 40.0, smoothing: Smoothing::PRECISE, curve: GainCurve::PRECISE, friction: 0.4, ..Scope::new(width, height) }
    }

    /// Makes the way the phone points now the center, on the next update
    /// (and puts the crosshair there).
    pub fn recenter(&mut self) {
        self.reference = None;
    }

    /// Moves the aim by a finger's movement (logical units). The only way
    /// to aim when [`source`](Self::source) is [`AimSource::Drag`].
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.aim.0 += dx;
        self.aim.1 += dy;
    }

    /// The targets on screen this frame, for friction (and magnetism): call
    /// it before [`update`](Self::update) whenever they move, appear or go.
    /// They stay until the next call; `&[]` clears them.
    pub fn set_targets(&mut self, targets: &[Target]) {
        self.targets.clear();
        self.targets.extend_from_slice(targets);
    }

    /// Where the crosshair is, in the area's coordinates (it can be a
    /// little past an edge, by `give`).
    pub fn position(&self) -> (f32, f32) {
        (self.size.0 * 0.5 + self.pos.0, self.size.1 * 0.5 + self.pos.1)
    }

    /// What aimed on the last update (`None` before the first).
    pub fn source(&self) -> Option<AimSource> {
        self.source
    }

    /// Degrees turned right and pointed up from the reference, as of the
    /// last update (roll and pitch when aiming with tilt).
    pub fn angles(&self) -> (f32, f32) {
        self.last.unwrap_or((0.0, 0.0))
    }

    /// How fast the aim is turning (°/s, smoothed), as the filter and the
    /// gain curve see it.
    pub fn speed(&self) -> f32 {
        self.vel.0.hypot(self.vel.1)
    }

    /// The multiplier on the gain at the last update: the gain curve's
    /// times friction's (1 is the full gain).
    pub fn gain_scale(&self) -> f32 {
        self.scale
    }

    /// The smoothing's time constant at the last update, in seconds (with
    /// [`Smoothing::Adaptive`] it shrinks as the phone turns faster).
    pub fn time_constant(&self) -> f32 {
        self.tau
    }

    /// How much the crosshair is over a target: 1 inside one, easing to 0
    /// at 1.5× its radius, with the target it's over most.
    fn over(&self) -> (f32, Option<Target>) {
        let (x, y) = self.position();
        let mut best = (0.0, None);
        for t in &self.targets {
            let r = t.radius.max(0.0);
            let d = (t.pos.0 - x).hypot(t.pos.1 - y);
            let k = if d <= r {
                1.0
            } else if d >= 1.5 * r {
                0.0
            } else {
                let u = (d - r) / (0.5 * r);
                1.0 - u * u * (3.0 - 2.0 * u)
            };
            if k > best.0 {
                best = (k, Some(*t));
            }
        }
        best
    }

    /// Reads this frame's attitude (or tilt) and moves the crosshair. Call
    /// once per `update`.
    pub fn update(&mut self, dt: f32) {
        self.update_with(dt, attitude(), tilt());
    }

    /// [`update`](Self::update) with the readings given (for tests and bots).
    pub fn update_with(&mut self, dt: f32, attitude: Option<Attitude>, tilt: Option<Tilt>) {
        let deg = 180.0 / std::f32::consts::PI;
        let (source, now) = match (attitude, tilt) {
            (Some(a), _) => (AimSource::Attitude, Some((a.yaw() * deg, a.pitch() * deg))),
            (None, Some(t)) => (AimSource::Tilt, Some((t.roll() * deg, -t.pitch() * deg))),
            (None, None) => (AimSource::Drag, None),
        };
        if self.source != Some(source) {
            self.source = Some(source);
            self.reference = None;
        }
        let half = (self.size.0 * 0.5, self.size.1 * 0.5);
        let px = self.size.0.max(1.0) / self.gain.max(1.0);
        let (over, near) = self.over();
        if let Some(now) = now {
            let Some(r) = self.reference else {
                // Center here, crosshair in the middle.
                self.reference = Some(now);
                self.last = Some((0.0, 0.0));
                self.turned = (0.0, 0.0);
                self.held = (0.0, 0.0);
                self.filt = (0.0, 0.0);
                self.vel = (0.0, 0.0);
                self.aim = (0.0, 0.0);
                self.pos = (0.0, 0.0);
                return;
            };
            let rel = (wrap_deg(now.0 - r.0), now.1 - r.1);
            let prev = self.last.unwrap_or(rel);
            self.last = Some(rel);
            self.turned = (self.turned.0 + wrap_deg(rel.0 - prev.0), rel.1);
            // Backlash dead zone: wobble smaller than it moves nothing, and it costs no lag.
            let before = self.held;
            let dz = self.dead_zone.max(0.0);
            for (h, v) in [(&mut self.held.0, self.turned.0), (&mut self.held.1, self.turned.1)] {
                if v - *h > dz {
                    *h = v - dz;
                } else if *h - v > dz {
                    *h = v + dz;
                }
            }
            if dt > 0.0 {
                // How fast it's turning, smoothed so tremor's back-and-forth averages out.
                let k = 1.0 - (-std::f32::consts::TAU * SPEED_CUTOFF * dt).exp();
                self.vel.0 += ((self.held.0 - before.0) / dt - self.vel.0) * k;
                self.vel.1 += ((self.held.1 - before.1) / dt - self.vel.1) * k;
                let speed = self.speed();
                // Smoothing (a fixed 8 ms by default; the One Euro filter is heavy when still, next to none in a swing).
                self.tau = self.smoothing.time_constant(speed);
                let a = if self.tau > 0.0 { 1.0 - (-dt / self.tau).exp() } else { 1.0 };
                let step = ((self.held.0 - self.filt.0) * a, (self.held.1 - self.filt.1) * a);
                self.filt.0 += step.0;
                self.filt.1 += step.1;
                // The gain: lower for slow turns and over a target. It scales the
                // hand's movement, so a still hand still holds the crosshair still.
                self.scale = self.curve.at(speed) * (1.0 - self.friction.clamp(0.0, 1.0) * over);
                let moved = (step.0 * px * self.scale, -step.1 * px * self.scale);
                self.aim.0 += moved.0;
                self.aim.1 += moved.1;
                // Magnetism: a pull toward the target, in proportion to the hand's own movement.
                let m = moved.0.hypot(moved.1);
                if let Some(t) = near
                    && self.magnetism > 0.0
                    && m > 0.0
                {
                    let to = (t.pos.0 - half.0 - self.aim.0, t.pos.1 - half.1 - self.aim.1);
                    let d = to.0.hypot(to.1);
                    if d > 1e-4 {
                        let s = (self.magnetism * m * over).min(d) / d;
                        self.aim.0 += to.0 * s;
                        self.aim.1 += to.1 * s;
                    }
                }
            }
        }
        // Past an edge: hold it there, so turning back leaves the edge at once.
        let give = (self.give * self.size.0).max(0.0);
        for (a, h) in [(&mut self.aim.0, half.0), (&mut self.aim.1, half.1)] {
            if a.abs() > h + 2.0 * give {
                *a = (h + 2.0 * give) * a.signum();
            }
        }
        // Drift correction: a crosshair resting near the center (not on a target) eases back to it.
        if now.is_some() && self.recenter_secs > 0.0 && dt > 0.0 && self.speed() < 4.0 && over == 0.0 && self.aim.0.hypot(self.aim.1) < 0.12 * half.0 {
            let k = 1.0 - (-dt / self.recenter_secs).exp();
            self.aim.0 -= self.aim.0 * k;
            self.aim.1 -= self.aim.1 * k;
        }
        for (p, a, h) in [(&mut self.pos.0, self.aim.0, half.0), (&mut self.pos.1, self.aim.1, half.1)] {
            let over = a.abs() - h;
            *p = if over > 0.0 {
                let soft = if give > 0.0 { give * (1.0 - (-over / give).exp()) } else { 0.0 };
                (h + soft) * a.signum()
            } else {
                a
            };
        }
    }
}

/// Mic loudness this frame, 0 (silence) to 1 (shouting or blowing straight
/// into the mic), on a −60…0 dBFS scale with fast attack and slower
/// release. `None` while the mic is unavailable: before permission is
/// granted, if the player denied it, or before the first reading. Needs
/// `sensors = ["loudness"]`; show the player what to do when it's `None`.
pub fn loudness() -> Option<f32> {
    let v = unsafe { ffi::mb_loudness() };
    (v >= 0.0).then_some(v)
}

/// A haptic effect. Needs `sensors = ["haptics"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Haptic {
    /// A light tick.
    Tap = 0,
    Success = 1,
    Fail = 2,
    /// A strong thud.
    Heavy = 3,
}

/// Plays a haptic effect. Output only (like audio, it never affects the game);
/// the platform may drop it, e.g. while the game is a feed card rather than
/// being played, or when requests come faster than about 30 per second.
pub fn haptic(kind: Haptic) {
    unsafe { ffi::mb_haptic(kind as u32) }
}

#[cfg(test)]
mod attitude_tests {
    use super::*;

    const D: f32 = std::f32::consts::PI / 180.0;

    #[test]
    fn angles_read_back() {
        for &(y, p, r) in &[(0.0, 0.0, 0.0), (30.0, 0.0, 0.0), (-50.0, 20.0, 0.0), (10.0, -35.0, 5.0), (170.0, 10.0, -8.0), (40.0, -80.0, 0.0)] {
            let a = Attitude::from_angles(y * D, p * D, r * D);
            assert!((a.yaw() / D - y).abs() < 0.01, "yaw {y}: {}", a.yaw() / D);
            assert!((a.pitch() / D - p).abs() < 0.01, "pitch {p}: {}", a.pitch() / D);
            // Like Tilt::roll: how far the right edge drops, asin(sin roll · cos pitch).
            let drop = ((r * D).sin() * (p * D).cos()).asin();
            assert!((a.roll() - drop).abs() < 1e-4, "roll {r}: {}", a.roll() / D);
        }
    }

    #[test]
    fn gravity_matches_tilt() {
        let a = Attitude::from_angles(25.0 * D, -30.0 * D, 12.0 * D);
        let g = a.gravity();
        assert!((g.roll() - a.roll()).abs() < 1e-4, "{}", g.roll() / D);
        // The preview's tilt for roll 12°, pitch 30° (screen tipped back): (sin r cos p, cos r cos p, −sin p).
        let (r, p) = (12.0 * D, 30.0 * D);
        let want = [r.sin() * p.cos(), r.cos() * p.cos(), -p.sin()];
        assert!((g.x - want[0]).abs() < 1e-5 && (g.y - want[1]).abs() < 1e-5 && (g.z - want[2]).abs() < 1e-5, "{g:?}");
        let up = Attitude::default().gravity();
        assert!((up.y - 1.0).abs() < 1e-6 && up.x.abs() < 1e-6 && up.z.abs() < 1e-6);
    }

    #[test]
    fn relative_to_removes_the_reference_heading() {
        let r = Attitude::from_angles(100.0 * D, 10.0 * D, 0.0);
        let a = Attitude::from_angles(115.0 * D, -5.0 * D, 0.0).relative_to(r);
        assert!((a.yaw() / D - 15.0).abs() < 0.01);
        assert!((a.pitch() / D + 5.0).abs() < 0.01);
        // Across ±180°.
        let a = Attitude::from_angles(-170.0 * D, 0.0, 0.0).relative_to(Attitude::from_angles(170.0 * D, 0.0, 0.0));
        assert!((a.yaw() / D - 20.0).abs() < 0.01);
    }

    #[test]
    fn rate_is_in_the_phones_axes() {
        let a = Attitude::from_angles(0.0, 0.0, 0.0);
        let b = Attitude::from_angles(1.0 * D, 0.0, 0.0).with_rate_from(a, 0.01);
        // Upright, turning right is a turn about the phone's −y (its top).
        assert!((b.rate[1] + 100.0 * D).abs() < 0.01 && b.rate[0].abs() < 1e-3, "{:?}", b.rate);
        let c = Attitude::from_angles(0.0, 1.0 * D, 0.0).with_rate_from(a, 0.01);
        assert!((c.rate[0] + 100.0 * D).abs() < 0.01, "{:?}", c.rate);
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    const D: f32 = std::f32::consts::PI / 180.0;
    const DT: f32 = 1.0 / 60.0;
    /// Logical units per degree at the default gain (32° across 360).
    const PX: f32 = 11.25;
    /// And with `Scope::precise` (40° across 360).
    const PRECISE_PX: f32 = 9.0;

    fn at(yaw: f32, pitch: f32) -> Option<Attitude> {
        Some(Attitude::from_angles(yaw * D, pitch * D, 0.0))
    }

    /// The scope `s` started facing (yaw0, 0), then held at (yaw, pitch) for `frames`.
    fn aim(mut s: Scope, yaw0: f32, yaw: f32, pitch: f32, frames: usize) -> Scope {
        s.update_with(DT, at(yaw0, 0.0), None);
        for _ in 0..frames {
            s.update_with(DT, at(yaw, pitch), None);
        }
        s
    }

    /// A default scope, aimed.
    fn aimed(yaw0: f32, yaw: f32, pitch: f32, frames: usize) -> Scope {
        aim(Scope::new(360.0, 640.0), yaw0, yaw, pitch, frames)
    }

    /// A precise scope, aimed.
    fn aimed_precise(yaw0: f32, yaw: f32, pitch: f32, frames: usize) -> Scope {
        aim(Scope::precise(360.0, 640.0), yaw0, yaw, pitch, frames)
    }

    /// A resting hand at `yaw`: physiological tremor of `amp` degrees at 8–12 Hz, time `t`.
    fn tremor(yaw: f32, amp: f32, t: f32) -> Option<Attitude> {
        let s = t * std::f32::consts::TAU;
        at(yaw + amp * 0.5 * ((8.3 * s).sin() + (10.7 * s + 1.0).sin()), amp * 0.5 * ((9.1 * s + 2.0).sin() + (11.3 * s).sin()))
    }

    #[test]
    fn gain_maps_degrees_across_the_width() {
        let s = aimed(50.0, 58.0, 4.0, 30);
        let (x, y) = s.position();
        // 8° of 32° is a quarter of 360, less the 0.1° dead zone; 4° up is 44 units up.
        assert!((x - (180.0 + 7.9 / 32.0 * 360.0)).abs() < 0.5, "{x}");
        assert!((y - (320.0 - 3.9 / 32.0 * 360.0)).abs() < 0.5, "{y}");
        assert_eq!(s.source(), Some(AimSource::Attitude));
        // Precise: 40° across at full gain, and with the gain curve a quick turn still gets (nearly) all of it.
        let mut flat = Scope::precise(360.0, 640.0);
        flat.curve = GainCurve::FLAT;
        let px = aim(flat, 50.0, 58.0, 4.0, 30).position().0;
        assert!((px - (180.0 + 7.9 / 40.0 * 360.0)).abs() < 0.5, "{px}");
        let quick = aimed_precise(50.0, 58.0, 4.0, 30);
        assert!((quick.position().0 - px).abs() < 0.03 * (px - 180.0), "{} vs {px}", quick.position().0);
        let mut wide = Scope::new(360.0, 640.0);
        wide.gain = 50.0;
        let wide = aim(wide, 0.0, 8.0, 0.0, 30);
        assert!(wide.position().0 < x, "more degrees across the screen: less movement per degree");
    }

    #[test]
    fn it_follows_within_a_frame_or_two() {
        for (mut s, px) in [(Scope::new(360.0, 640.0), PX), (Scope::precise(360.0, 640.0), PRECISE_PX)] {
            s.update_with(DT, at(0.0, 0.0), None);
            s.update_with(DT, at(10.0, 0.0), None);
            let want = 180.0 + 9.9 * px;
            assert!(s.position().0 - 180.0 > 0.85 * (want - 180.0), "most of the way in one frame: {}", s.position().0);
            s.update_with(DT, at(10.0, 0.0), None);
            assert!((s.position().0 - want).abs() < 2.0, "there in two: {}", s.position().0);
        }
    }

    #[test]
    fn a_tiny_wobble_moves_nothing() {
        for mut s in [aimed(0.0, 5.0, 0.0, 10), aimed_precise(0.0, 5.0, 0.0, 10)] {
            let x = s.position().0;
            for i in 0..30 {
                let w = if i % 2 == 0 { 0.08 } else { 0.0 };
                s.update_with(DT, at(5.0 - w, 0.0), None);
            }
            assert!((s.position().0 - x).abs() < 0.05, "{} vs {x}", s.position().0);
        }
    }

    #[test]
    fn precise_smooths_a_resting_hands_tremor_away() {
        // 0.25° of tremor (a firm hand at arm's length is 0.05–0.2°; a tired one more).
        let jitter = |mut s: Scope| {
            s.update_with(DT, at(0.0, 0.0), None);
            let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
            let mut first = None;
            for i in 0..(60 * 4) {
                s.update_with(DT, tremor(5.0, 0.25, i as f32 * DT), None);
                if i >= 60 {
                    let (x, y) = s.position();
                    first.get_or_insert((x, y));
                    lo = (lo.0.min(x), lo.1.min(y));
                    hi = (hi.0.max(x), hi.1.max(y));
                }
            }
            let end = s.position();
            let start = first.unwrap();
            ((hi.0 - lo.0).max(hi.1 - lo.1), (end.0 - start.0).hypot(end.1 - start.1))
        };
        let (plain, _) = jitter(Scope::new(360.0, 640.0));
        let (precise, creep) = jitter(Scope::precise(360.0, 640.0));
        assert!(plain > 2.0, "the default mapping shows it: {plain}");
        assert!(precise < 0.3 * plain && precise < 1.0, "smoothed: {precise} vs {plain}");
        assert!(creep < 0.5, "and it doesn't creep: {creep}");
    }

    #[test]
    fn precise_barely_smooths_a_fast_swing() {
        for v in [60.0, 120.0] {
            for hz in [60.0, 120.0] {
                let dt = 1.0 / hz;
                let mut s = Scope::precise(2000.0, 640.0);
                s.gain = 2000.0 / PX;
                s.update_with(dt, at(0.0, 0.0), None);
                let frames = (0.3 * hz) as usize;
                let mut x = 0.0;
                for i in 1..=frames {
                    x = s.position().0;
                    s.update_with(dt, at(v * i as f32 * dt, 0.0), None);
                }
                // How far the smoothed aim trails the hand (dead zone included), as time.
                let lag = (v * frames as f32 * dt - s.filt.0) / v;
                assert!(lag < 0.01, "{v}°/s at {hz} Hz: {} ms behind", lag * 1000.0);
                let step = s.position().0 - x;
                assert!((step - v * dt * PX).abs() < 0.02 * v * dt * PX, "{v}°/s at {hz} Hz at full gain: {step}");
            }
        }
    }

    #[test]
    fn the_gain_curve_eases_without_a_step() {
        let c = GainCurve::PRECISE;
        assert_eq!(c.at(0.0), 0.6);
        assert_eq!(c.at(10.0), 0.6);
        assert_eq!(c.at(50.0), 1.0);
        assert_eq!(c.at(500.0), 1.0);
        let mut prev = c.at(0.0);
        for i in 1..=10000 {
            let m = c.at(i as f32 * 0.01);
            assert!(m >= prev && m - prev < 0.001, "at {}°/s: {prev} → {m}", i as f32 * 0.01);
            prev = m;
        }
        for v in [0.0, 5.0, 30.0, 300.0] {
            assert_eq!(GainCurve::FLAT.at(v), 1.0);
        }
    }

    #[test]
    fn precise_moves_a_slow_correction_less_than_a_swing() {
        let moved = |mut s: Scope, frames: usize| {
            s.recenter_secs = 0.0;
            s.update_with(DT, at(0.0, 0.0), None);
            for i in 1..=frames {
                s.update_with(DT, at(3.0 * i as f32 / frames as f32, 0.0), None);
            }
            for _ in 0..60 {
                s.update_with(DT, at(3.0, 0.0), None);
            }
            s.position().0 - 180.0
        };
        let precise = || Scope::precise(360.0, 640.0);
        let (slow, fast) = (moved(precise(), 60 * 3 / 2), moved(precise(), 1));
        // 2°/s against a snap: 0.6 of the distance, give or take the ease.
        assert!((fast - 2.9 * PRECISE_PX).abs() < 0.03 * fast, "{fast}");
        assert!((slow / fast - 0.6).abs() < 0.03, "{slow} vs {fast}");
        // The default: every degree the same, at any speed.
        let plain = || Scope::new(360.0, 640.0);
        let (slow, fast) = (moved(plain(), 60 * 3 / 2), moved(plain(), 1));
        assert!((fast - 2.9 * PX).abs() < 0.01 * fast && (slow - fast).abs() < 0.01 * fast, "{slow} vs {fast}");
    }

    #[test]
    fn friction_slows_the_crosshair_over_a_target_but_never_moves_it() {
        // Resting on a target, perfectly still, near the center (where drift correction would act):
        // nothing moves, magnetism or not.
        for magnetism in [0.0, 0.3] {
            // (Two seconds in, the filter has long caught up with the hand.)
            let mut s = aimed_precise(0.0, 1.0, 0.0, 120);
            let (x, y) = s.position();
            s.magnetism = magnetism;
            s.set_targets(&[Target::new(x + 25.0, y - 6.0, 22.0)]);
            for _ in 0..(60 * 5) {
                s.update_with(DT, at(1.0, 0.0), None);
            }
            assert!((s.position().0 - x).abs() < 1e-3 && (s.position().1 - y).abs() < 1e-3, "{:?} vs ({x}, {y})", s.position());
        }
        // The same slow turn moves it less over a target.
        let turn = |s: Scope, target: bool| {
            let mut s = aim(s, 0.0, 5.0, 0.0, 30);
            let friction = s.friction;
            let (x, y) = s.position();
            if target {
                s.set_targets(&[Target::new(x, y, 22.0)]);
            }
            for i in 1..=60 {
                s.update_with(DT, at(5.0 + i as f32 / 60.0, 0.0), None);
            }
            for _ in 0..30 {
                s.update_with(DT, at(6.0, 0.0), None);
            }
            if target {
                assert!(s.gain_scale() <= 1.0 - friction + 1e-6);
            }
            s.position().0 - x
        };
        let precise = || Scope::precise(360.0, 640.0);
        let (free, held) = (turn(precise(), false), turn(precise(), true));
        assert!(free > 4.0 && (held / free - 0.6).abs() < 0.03, "{held} vs {free}");
        // The default has no friction: targets change nothing.
        let plain = || Scope::new(360.0, 640.0);
        let (free, held) = (turn(plain(), false), turn(plain(), true));
        assert!(free > 4.0 && (held - free).abs() < 1e-3, "{held} vs {free}");
    }

    #[test]
    fn magnetism_pulls_only_while_the_hand_moves() {
        let mut s = aimed_precise(0.0, 5.0, 0.0, 120);
        let (x, y) = s.position();
        s.magnetism = 0.3;
        s.set_targets(&[Target::new(x + 5.0, y - 12.0, 22.0)]);
        for _ in 0..60 {
            s.update_with(DT, at(5.0, 0.0), None);
        }
        assert_eq!(s.position(), (x, y));
        // Turning right only: it also slides up toward the target.
        for i in 1..=30 {
            s.update_with(DT, at(5.0 + i as f32 * 0.05, 0.0), None);
        }
        assert!(s.position().1 < y - 0.5, "{:?}", s.position());
    }

    #[test]
    fn it_feels_the_same_at_60_and_120_hz() {
        let hand = crate::input::synth::aim((0.0, 0.0), (9.0, 5.0), 0.2, 0.4, 11);
        let run = |mut s: Scope, hz: usize| {
            s.set_targets(&[Target::new(255.0, 280.0, 22.0)]);
            let mut out = Vec::new();
            for i in 0..=hz {
                s.update_with(1.0 / hz as f32, Some(hand.at(i as f64 / hz as f64)), None);
                if i % (hz / 10) == 0 {
                    out.push(s.position());
                }
            }
            out
        };
        for new in [Scope::new, Scope::precise] {
            let (a, b) = (run(new(360.0, 640.0), 60), run(new(360.0, 640.0), 120));
            for (i, (p, q)) in a.iter().zip(&b).enumerate() {
                let d = (p.0 - q.0).hypot(p.1 - q.1);
                assert!(d < if i < 10 { 2.0 } else { 1.0 }, "t = {}: {p:?} vs {q:?}", i as f32 / 10.0);
            }
        }
    }

    #[test]
    fn a_resting_aim_near_the_center_drifts_back_but_not_one_far_out() {
        let mut s = aimed(0.0, 1.5, 0.0, 10);
        let x0 = s.position().0 - 180.0;
        for _ in 0..(60 * 10) {
            s.update_with(DT, at(1.5, 0.0), None);
        }
        let x1 = s.position().0 - 180.0;
        assert!(x0 > 7.0 && x1 < x0 * 0.5, "{x0} → {x1}");
        let mut far = aimed(0.0, 10.0, 0.0, 30);
        let f0 = far.position().0;
        for _ in 0..(60 * 10) {
            far.update_with(DT, at(10.0, 0.0), None);
        }
        assert!((far.position().0 - f0).abs() < 0.01, "aiming at something off-center isn't fought");
    }

    #[test]
    fn edges_hold_with_a_little_give_and_let_go_at_once() {
        let mut s = aimed(0.0, 40.0, 0.0, 30);
        let x = s.position().0;
        assert!(x > 360.0 && x <= 360.0 + 0.03 * 360.0, "{x}");
        // Turn back 5°: off the edge right away, not after unwinding 20° of overshoot.
        for _ in 0..5 {
            s.update_with(DT, at(35.0, 0.0), None);
        }
        assert!(s.position().0 < 360.0 - 20.0, "{}", s.position().0);
    }

    #[test]
    fn recenter_makes_now_the_center() {
        let mut s = aimed(0.0, 12.0, -6.0, 10);
        assert!(s.position().0 > 250.0);
        s.recenter();
        s.update_with(DT, at(12.0, -6.0), None);
        assert_eq!(s.position(), (180.0, 320.0));
        s.update_with(DT, at(15.0, -6.0), None);
        assert!(s.position().0 > 190.0);
        // Across ±180°, and on round past it.
        let mut s = aimed(179.0, -179.0, 0.0, 10);
        assert!(s.position().0 > 180.0 && s.position().0 < 210.0, "{}", s.position().0);
        for _ in 0..10 {
            s.update_with(DT, at(-175.0, 0.0), None);
        }
        assert!(s.position().0 > 215.0 && s.position().0 < 260.0, "{}", s.position().0);
    }

    #[test]
    fn a_synthesized_hand_lands_the_crosshair_on_its_target() {
        // A bot wants the crosshair on a target at (270, 250): it swings there, then makes
        // small corrections the way a player does, reading how far the crosshair is off.
        let want = (270.0, 250.0);
        for (mut s, px) in [(Scope::new(360.0, 640.0), PX), (Scope::precise(360.0, 640.0), PRECISE_PX)] {
            s.set_targets(&[Target::new(want.0, want.1, 22.0)]);
            let mut from = (0.0, 0.0);
            let mut to = ((want.0 - 180.0) / px, (320.0 - want.1) / px);
            let mut t = 0.0;
            let mut misses = Vec::new();
            for (secs, frames) in [(0.4, 60), (0.35, 45), (0.35, 45)] {
                let hand = crate::input::synth::aim(from, to, t + 0.05, secs, 7);
                for _ in 0..frames {
                    t += 1.0 / 60.0;
                    s.update_with(DT, Some(hand.at(t)), None);
                }
                let (x, y) = s.position();
                misses.push((x - want.0).hypot(y - want.1));
                // Turn by the miss at the gain a slow correction gets here.
                let k = px * s.gain_scale();
                from = to;
                to = (to.0 + (want.0 - x) / k, to.1 - (want.1 - y) / k);
            }
            assert!(misses[0] < 22.0, "the swing lands on the target: {misses:?}");
            assert!(misses[2] < 3.0 && misses[2] <= misses[0], "corrections close in: {misses:?}");
        }
        let hand = crate::input::synth::aim((0.0, 0.0), (10.0, 0.0), 0.2, 0.4, 7);
        assert!(hand.at(0.4).rate[1] < -0.1, "turning right while it swings");
    }

    #[test]
    fn without_a_gyro_it_aims_with_tilt_then_with_a_finger() {
        let mut s = Scope::new(360.0, 640.0);
        let tilt = |roll: f32| Some(Tilt { x: (roll * D).sin(), y: (roll * D).cos(), z: 0.0 });
        s.update_with(DT, None, tilt(0.0));
        for _ in 0..5 {
            s.update_with(DT, None, tilt(10.0));
        }
        assert_eq!(s.source(), Some(AimSource::Tilt));
        assert!(s.position().0 > 250.0);
        let mut s = Scope::new(360.0, 640.0);
        s.update_with(DT, None, None);
        s.drag(30.0, -20.0);
        for _ in 0..5 {
            s.update_with(DT, None, None);
        }
        assert_eq!(s.source(), Some(AimSource::Drag));
        assert!((s.position().0 - 210.0).abs() < 0.01 && (s.position().1 - 300.0).abs() < 0.01);
    }
}
