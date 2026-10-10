//! Flight that looks alive: [`Flight`] flaps a bird's wings (the two-bone
//! wings `mb model --spine` gives a generated bird, or any skeleton with
//! `wing_l_0/1`, `wing_r_0/1` bones), harder when it climbs or is slow, glides
//! when it can, banks into turns, pitches with its climb, bobs with each
//! downstroke and keeps its head level; [`Strike`] is a short attack
//! maneuver: rear back, dive with wings swept back, flare, recover.
//!
//! Everything is a function of the velocities you pass and `dt`, so replays
//! match; feed it a smoothed path (`motion::Follow<Vec3>`), never a raw one.
//!
//! ```ignore
//! let heron = model.spawn(None)?;
//! let bones = Skeleton::spine(heron)?;          // spine_0.. and the wings
//! let mut flight = Flight::new(2.4);            // wingspan-ish size in meters
//! let mut path = Follow::new(start, 0.35);
//! // every frame:
//! let pos = path.update(target, dt);
//! flight.update(dt, path.velocity);
//! heron.set_transform(&Transform::at(pos + flight.bob()).with_rot(flight.rotation()));
//! flight.apply(&bones);
//! if tap { flight.strike(); }                     // dive at something below
//! ```

use std::f32::consts::{PI, TAU};

use super::math::{Quat, Vec3};
use super::skin::Skeleton;
use crate::motion::{Follow, wrap_angle};

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Piecewise keys `(time, value)` eased in and out between each pair.
fn keys(k: &[(f32, f32)], t: f32) -> f32 {
    if t <= k[0].0 {
        return k[0].1;
    }
    for w in k.windows(2) {
        if t <= w[1].0 {
            let u = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
            return w[0].1 + (w[1].1 - w[0].1) * smooth(u);
        }
    }
    k[k.len() - 1].1
}

/// A strike: anticipation (rear back, wings up), a dive (nose down, wings
/// swept back), a flare (nose up, wings spread wide, one hard beat) and the
/// recovery. Read its curves while it runs: `height()` to add to the flight
/// path (× how deep it dives), `pitch()`, `fold()`, `reach()` (0–1, the
/// moment the beak gets there: hit-test then).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    /// Seconds of each part: anticipation, dive, flare, recovery.
    pub timing: [f32; 4],
    t: f32,
    active: bool,
}

impl Default for Strike {
    fn default() -> Strike {
        Strike {
            timing: [0.3, 0.45, 0.35, 0.6],
            t: 0.0,
            active: false,
        }
    }
}

impl Strike {
    pub fn start(&mut self) {
        self.t = 0.0;
        self.active = true;
    }
    pub fn update(&mut self, dt: f32) {
        if self.active {
            self.t += dt;
            if self.t >= self.duration() {
                self.active = false;
            }
        }
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn duration(&self) -> f32 {
        self.timing.iter().sum()
    }
    /// Seconds since it started.
    pub fn time(&self) -> f32 {
        self.t
    }
    fn marks(&self) -> [f32; 5] {
        let [a, d, f, r] = self.timing;
        [0.0, a, a + d, a + d + f, a + d + f + r]
    }
    fn curve(&self, v: [f32; 5]) -> f32 {
        if !self.active {
            return 0.0;
        }
        let m = self.marks();
        keys(
            &[
                (m[0], 0.0),
                (m[1], v[1]),
                (m[2], v[2]),
                (m[3], v[3]),
                (m[4], v[4]),
            ],
            self.t,
        )
    }
    /// Vertical offset as a fraction of the dive's depth: up a little, down to −1, back to 0.
    pub fn height(&self) -> f32 {
        self.curve([0.0, 0.12, -1.0, -0.85, 0.0])
    }
    /// Extra body pitch (radians, + nose up).
    pub fn pitch(&self) -> f32 {
        self.curve([0.0, 0.3, -0.85, 0.55, 0.0])
    }
    /// How far the wings are swept back and tucked (0 spread … 1 tucked).
    pub fn fold(&self) -> f32 {
        self.curve([0.0, 0.25, 1.0, 0.0, 0.0])
    }
    /// 0–1, peaking as the dive bottoms out: when the beak arrives.
    pub fn reach(&self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let m = self.marks();
        1.0 - ((self.t - m[2]) / (0.5 * self.timing[1] + 0.5 * self.timing[2]))
            .abs()
            .min(1.0)
    }
}

/// The wing bones of a skeleton: shoulder and elbow on each side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WingBones {
    pub left: [usize; 2],
    pub right: [usize; 2],
}

impl WingBones {
    /// `wing_l_0`, `wing_l_1`, `wing_r_0`, `wing_r_1` by name.
    pub fn find(sk: &Skeleton) -> Option<WingBones> {
        Some(WingBones {
            left: [sk.find("wing_l_0")?, sk.find("wing_l_1")?],
            right: [sk.find("wing_r_0")?, sk.find("wing_r_1")?],
        })
    }
}

/// Bird flight: wingbeats, glides, banking, pitch, bob, a level head and a strike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    /// Size in meters (wingspan): scales the bob.
    pub size: f32,
    /// Wingbeats per second at full effort (a heron ~2.5, a crow 4, a dragon 1).
    pub flap_hz: f32,
    /// Shoulder swing in radians either side of level (0.5–0.8).
    pub flap_amp: f32,
    /// How high the wings are held while gliding (radians, 0.05–0.2).
    pub dihedral: f32,
    /// Speed (m/s) at which level flight is mostly gliding.
    pub cruise: f32,
    /// Largest bank, radians.
    pub max_bank: f32,
    /// Effort (0–1) below which it glides.
    pub glide_below: f32,
    /// How much of the bank and pitch the head undoes (0 none, 1 rock steady).
    pub head_steady: f32,
    /// The model's own correction, applied first (e.g. `Quat::from_rotation_x(-1.3)` for a
    /// bird sculpted standing upright that should fly level).
    pub rest: Quat,
    pub strike_move: Strike,
    phase: f32,
    strength: f32,
    rate: f32,
    effort: Follow<f32>,
    bank: Follow<f32>,
    pitch: Follow<f32>,
    yaw: f32,
    yaw_rate: f32,
    last_vel: Vec3,
    started: bool,
}

impl Flight {
    pub fn new(size: f32) -> Flight {
        Flight {
            size,
            flap_hz: 2.6,
            flap_amp: 0.65,
            dihedral: 0.12,
            cruise: 3.0 * size.max(0.2),
            max_bank: 0.75,
            glide_below: 0.32,
            head_steady: 0.8,
            rest: Quat::IDENTITY,
            strike_move: Strike::default(),
            phase: 0.0,
            strength: 1.0,
            rate: 0.0,
            effort: Follow::new(0.6, 0.35),
            bank: Follow::new(0.0, 0.22),
            pitch: Follow::new(0.0, 0.3),
            yaw: 0.0,
            yaw_rate: 0.0,
            last_vel: Vec3::ZERO,
            started: false,
        }
    }

    /// Sets `rest` so the skeleton's head-to-tail line flies level: for a
    /// bird sculpted standing upright with its wings spread (most generated
    /// "in flight" birds), which would otherwise fly belly first.
    pub fn level_with(mut self, bones: &Skeleton) -> Flight {
        let n = bones.spine_len();
        if let (Some(h), Some(t)) = (bones.rest_point(0), bones.rest_point(n.saturating_sub(1))) {
            let d = h - t;
            if d.length() > 1e-5 {
                self.rest = Quat::from_rotation_x(-d.y.atan2(-d.z));
            }
        }
        self
    }

    /// Starts the strike (see [`Strike`]); ignored while one runs.
    pub fn strike(&mut self) {
        if !self.strike_move.active() {
            self.strike_move.start();
        }
    }

    /// Advances `dt` seconds flying at `vel` (m/s, world).
    pub fn update(&mut self, dt: f32, vel: Vec3) {
        if dt <= 0.0 {
            return;
        }
        let horiz = (vel.x * vel.x + vel.z * vel.z).sqrt();
        let speed = vel.length();
        if !self.started && horiz > 1e-4 {
            self.yaw = (-vel.x).atan2(-vel.z);
            self.last_vel = vel;
            self.started = true;
        }
        if horiz > 0.05 {
            let yaw = (-vel.x).atan2(-vel.z);
            let d = wrap_angle(yaw - self.yaw);
            self.yaw += d;
            self.yaw_rate += (d / dt - self.yaw_rate) * (1.0 - (-dt * 8.0).exp());
        }
        let fwd = if speed > 1e-4 {
            vel * (1.0 / speed)
        } else {
            Vec3::ZERO
        };
        let accel = (vel - self.last_vel) * (1.0 / dt);
        self.last_vel = vel;
        // Effort: climbing, slow (near the stall) and speeding up all need beats; diving doesn't.
        let slow = ((self.cruise - speed) / self.cruise.max(1e-3)).clamp(0.0, 1.0);
        let raw = 0.22 + vel.y * 0.35 + slow * 0.7 + accel.dot(fwd).clamp(-4.0, 4.0) * 0.06;
        let effort = self.effort.update(raw.clamp(0.0, 1.0), dt);
        self.strike_move.update(dt);
        let s = &self.strike_move;
        let want = if s.active() {
            // Tucked in the dive, one hard beat in the flare.
            1.0 - s.fold()
        } else if effort > self.glide_below {
            1.0
        } else {
            0.0
        };
        // Ease in and out of a glide (never snap the wings).
        self.strength += (want - self.strength) * (1.0 - (-dt * 3.5).exp());
        let hz = self.flap_hz
            * (0.75 + 0.5 * effort)
            * if s.active() { 1.6 } else { 1.0 }
            * self.strength.max(0.2);
        // The beat's rate eases too, so the bob never changes speed within a frame.
        self.rate += (hz - self.rate) * (1.0 - (-dt * 5.0).exp());
        self.phase = (self.phase + TAU * self.rate * dt) % (TAU * 64.0);
        // Coordinated turn: tan(bank) = v·ω/g, into the turn.
        let bank = (horiz * self.yaw_rate / 9.81)
            .atan()
            .clamp(-self.max_bank, self.max_bank);
        self.bank.update(bank, dt);
        let climb = vel.y.atan2(horiz.max(1e-3)).clamp(-1.0, 1.0) * 0.7;
        self.pitch.update(climb, dt);
    }

    /// The beat's phase in radians (downstroke while `sin` falls).
    pub fn phase(&self) -> f32 {
        self.phase
    }
    /// 0 gliding … 1 flapping, eased.
    pub fn flapping(&self) -> f32 {
        self.strength
    }
    /// The bank angle now (radians, + rolls left into a left turn).
    pub fn bank(&self) -> f32 {
        self.bank.value
    }
    /// The body's pitch now (radians, + nose up), strike included.
    pub fn pitch(&self) -> f32 {
        self.pitch.value + self.strike_move.pitch()
    }
    /// Heading (radians about +y; 0 faces −z).
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// The body's rotation: heading, then pitch, then bank, then `rest`.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
            * Quat::from_rotation_x(self.pitch())
            * Quat::from_rotation_z(self.bank.value)
            * self.rest
    }

    /// The body rises a little on each downstroke: add this to its position.
    pub fn bob(&self) -> Vec3 {
        Vec3::new(
            0.0,
            -0.035 * self.size * self.strength * self.phase.cos(),
            0.0,
        )
    }

    /// Poses the wings, the head and the tail of `bones` (from [`Skeleton::spine`]).
    pub fn apply(&self, bones: &Skeleton) {
        let n = bones.spine_len();
        // Body axes in the model's own frame (before `rest`): forward = head − tail.
        let fwd = match (bones.rest_point(0), bones.rest_point(n.saturating_sub(1))) {
            (Some(h), Some(t)) if (h - t).length() > 1e-5 => (h - t).normalize(),
            _ => Vec3::new(0.0, 0.0, -1.0),
        };
        let up = Vec3::X.cross(fwd).normalize();
        let st = self.strength;
        let fold = self.strike_move.fold();
        // A faster downstroke than upstroke, the tip lagging the shoulder.
        let p = self.phase + 0.3 * self.phase.sin();
        let glide = (self.phase * 0.25).sin() * 0.03;
        let shoulder =
            self.dihedral * (1.0 - st) + glide + st * (self.flap_amp * p.sin() + 0.12) + fold * 0.5;
        let elbow = st * self.flap_amp * 0.6 * (p - 0.9).sin() - fold * 0.7;
        // Fold the hand a little on the upstroke, and sweep it all back to tuck.
        let up_fold = st * 0.35 * (0.5 + 0.5 * p.cos());
        let sweep = fold * 0.9;
        if let Some(w) = WingBones::find(bones) {
            for (side, b) in [(1.0f32, w.left), (-1.0, w.right)] {
                bones.turn(
                    b[0],
                    Quat::from_axis_angle(up, side * sweep)
                        * Quat::from_axis_angle(fwd, side * shoulder),
                );
                bones.turn(
                    b[1],
                    Quat::from_axis_angle(up, side * up_fold)
                        * Quat::from_axis_angle(fwd, side * elbow),
                );
            }
        }
        // A level head: bone 0 (the head, the chain's root) undoes most of the bank and pitch,
        // bone 1 turns the rest of the body back; the tail lags pitch changes.
        let k = self.head_steady;
        let head = Quat::from_axis_angle(fwd, -self.bank.value * k)
            * Quat::from_axis_angle(Vec3::X, -self.pitch() * k * 0.6);
        let tail = -self.pitch.velocity.clamp(-2.0, 2.0) * 0.08 - self.strike_move.fold() * 0.15;
        for i in 0..n {
            let bend = Quat::from_axis_angle(Vec3::X, tail * (i as f32 / n.max(1) as f32));
            let r = match i {
                0 => head,
                1 => head.conjugate() * bend,
                _ => bend,
            };
            bones.turn(i, r);
        }
    }
}

/// Small helpers for the secondary motion that makes any creature read as alive.
pub mod alive {
    use super::*;

    /// Breathing: a slow 0…1…0 swell (≈ 0.25 Hz resting, faster after effort).
    /// Scale a chest bone or the body by `1 + 0.015 * breath(t, rate)`.
    pub fn breath(t: f32, rate_hz: f32) -> f32 {
        0.5 - 0.5 * (t * TAU * rate_hz).cos()
    }

    /// Anticipation before an action: a short move the opposite way first,
    /// then the action. `t` is seconds since the action was asked for; returns
    /// −`back`…1 (a 0.15 s wind-up, then an eased go).
    pub fn anticipate(t: f32, back: f32) -> f32 {
        keys(&[(0.0, 0.0), (0.15, -back), (0.45, 1.0)], t)
    }

    /// The yaw a head should turn (radians, clamped to ±`limit`) to look at
    /// `target` from `head` while the body faces `body_yaw`: feed it through a
    /// `Follow` (halflife ≈ 0.12) so the glance eases, never snaps.
    pub fn look_yaw(head: Vec3, target: Vec3, body_yaw: f32, limit: f32) -> f32 {
        let d = target - head;
        wrap_angle((-d.x).atan2(-d.z) - body_yaw).clamp(-limit, limit)
    }

    /// Clamp an angle's change per frame (no instant direction changes).
    pub fn turn_toward(current: f32, target: f32, max_rate: f32, dt: f32) -> f32 {
        current + wrap_angle(target - current).clamp(-max_rate * dt, max_rate * dt)
    }

    pub const HALF_TURN: f32 = PI;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::{Node, vec3};

    #[test]
    fn climbing_flaps_and_level_cruise_glides() {
        let run = |vel: Vec3| {
            let mut f = Flight::new(2.0);
            for _ in 0..240 {
                f.update(1.0 / 60.0, vel);
            }
            f
        };
        let climb = run(vec3(0.0, 2.0, -3.0));
        let cruise = run(vec3(0.0, -0.3, -8.0));
        assert!(climb.flapping() > 0.9, "{}", climb.flapping());
        assert!(cruise.flapping() < 0.1, "{}", cruise.flapping());
        assert!(climb.pitch() > 0.2 && cruise.pitch() < 0.0);
    }

    #[test]
    fn it_banks_into_a_turn_smoothly() {
        let mut f = Flight::new(2.0);
        let mut last = 0.0f32;
        let mut worst = 0.0f32;
        for i in 0..240 {
            let a = i as f32 / 60.0 * 0.8; // turning left at 0.8 rad/s
            let vel = vec3(-a.sin(), 0.0, -a.cos()) * 6.0;
            f.update(1.0 / 60.0, vel);
            worst = worst.max((f.bank() - last).abs());
            last = f.bank();
        }
        assert!(f.bank() > 0.3, "rolls left into a left turn: {}", f.bank());
        assert!(worst < 0.05, "no jumps: {worst}");
    }

    #[test]
    fn a_strike_dives_then_flares_and_ends() {
        let mut f = Flight::new(2.0);
        f.strike();
        let (mut low, mut min_pitch, mut max_fold, mut reach) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for _ in 0..240 {
            f.update(1.0 / 60.0, vec3(0.0, 0.0, -5.0));
            low = low.min(f.strike_move.height());
            min_pitch = min_pitch.min(f.strike_move.pitch());
            max_fold = max_fold.max(f.strike_move.fold());
            reach = reach.max(f.strike_move.reach());
        }
        assert!(low < -0.95 && min_pitch < -0.8 && max_fold > 0.95 && reach > 0.95);
        assert!(!f.strike_move.active() && f.strike_move.height() == 0.0);
    }

    #[test]
    fn apply_poses_a_spine_with_wings() {
        let root = Node::new().unwrap();
        let sk = Skeleton::new(
            root,
            &[
                (None, crate::gfx3d::Transform::at(vec3(0.0, 0.0, -1.0))),
                (Some(0), crate::gfx3d::Transform::at(vec3(0.0, 0.0, 1.0))),
                (Some(1), crate::gfx3d::Transform::at(vec3(0.0, 0.0, 1.0))),
                (Some(1), crate::gfx3d::Transform::at(vec3(-0.2, 0.0, 0.0))),
                (Some(3), crate::gfx3d::Transform::at(vec3(-0.5, 0.0, 0.0))),
                (Some(1), crate::gfx3d::Transform::at(vec3(0.2, 0.0, 0.0))),
                (Some(5), crate::gfx3d::Transform::at(vec3(0.5, 0.0, 0.0))),
            ],
        )
        .unwrap()
        .with_names(&[
            "spine_0", "spine_1", "spine_2", "wing_l_0", "wing_l_1", "wing_r_0", "wing_r_1",
        ]);
        assert_eq!(sk.spine_len(), 3);
        assert_eq!(
            WingBones::find(&sk),
            Some(WingBones {
                left: [3, 4],
                right: [5, 6]
            })
        );
        let mut f = Flight::new(1.0);
        f.update(1.0 / 60.0, vec3(0.0, 1.0, -2.0));
        f.apply(&sk);
        assert!(alive::anticipate(0.15, 0.2) < -0.19 && alive::anticipate(1.0, 0.2) == 1.0);
        assert!(
            (alive::look_yaw(Vec3::ZERO, vec3(-1.0, 0.0, -1.0), 0.0, 2.0) - PI / 4.0).abs() < 1e-5
        );
    }
}
