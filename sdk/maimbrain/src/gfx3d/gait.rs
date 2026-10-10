//! A heavy biped's walk, like [`Swim`](super::Swim) and
//! [`Flight`](super::Flight) for fish and birds: [`MechGait`] plants each
//! foot in the world and steps it to where the body is going (forward,
//! sideways, backward, turning), with step length, lift and cadence tuned
//! for heavy machines, a body that dips after each footfall and rolls over
//! the planted leg, and a [`Stomp`] for every footfall (dust, camera shake,
//! foley). Pair it with [`TwoBoneIk`] legs on a [`Humanoid`].
//!
//! The step phase follows the distance walked, landings are placed where
//! the body was at the exact moment of the footfall (interpolated within
//! the frame), and the dip is a closed-form spring from that moment, so it
//! replays exactly and 60 and 120 Hz walk the same steps.
//!
//! ```ignore
//! let rig = Humanoid::find(model, mech)?;
//! let legs = [TwoBoneIk::leg(&rig, 'l')?, TwoBoneIk::leg(&rig, 'r')?];
//! let mut gait = MechGait::from_rig(&rig, body)?;     // feet where they stand now
//! // every frame:
//! for s in gait.update(dt, pos, yaw, vel) {           // pos on the ground, vel in m/s
//!     dust(s.pos); shake.add(0.3 * s.strength); foley.step(s.pos, s.strength);
//! }
//! body.set_transform(&Transform::at(pos + gait.body_offset()).with_rot(Quat::from_rotation_y(yaw) * gait.body_tilt()));
//! gait.apply(&rig, &legs, Quat::from_rotation_y(yaw) * Vec3::Z);
//! ```

use std::f32::consts::PI;

use super::ik::TwoBoneIk;
use super::math::{Quat, Vec3};
use super::rig::Humanoid;
use super::Node;
use crate::motion::kick;

/// A footfall: which foot (0 or 1), where it landed (world), and how hard
/// (0–1, from the step's length): for dust, camera shake and a stomp sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stomp {
    pub foot: usize,
    pub pos: Vec3,
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Foot {
    planted: Vec3,
    from: Vec3,
    pos: Vec3,
}

/// A heavy biped's stomping walk (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct MechGait {
    /// Meters the body travels per full cycle (each foot steps once): 1.2–2 for a 5 m mech.
    pub stride: f32,
    /// How high a foot lifts (meters).
    pub lift: f32,
    /// The share of a cycle each foot is planted (0.5–0.7; heavier is higher: both feet down longer).
    pub duty: f32,
    /// How far the body drops after a footfall (meters), springing back.
    pub dip: f32,
    /// How fast the dip springs back (Hz).
    pub dip_hz: f32,
    /// How far the hips roll over the planted leg (radians).
    pub roll: f32,
    /// Steps per second when stopping: a foot in the air finishes its step,
    /// a foot left behind steps back under the body.
    pub settle_hz: f32,
    /// Slower than this (m/s) is standing.
    pub min_speed: f32,
    rest: [Vec3; 2],
    feet: [Foot; 2],
    phase: f32,
    time: f32,
    land_time: f32,
    land_strength: f32,
    roll_now: f32,
    last: Option<(Vec3, f32)>,
    stomps: Vec<Stomp>,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl MechGait {
    /// A gait for feet (ankles) resting at `rest` in the body's frame (the
    /// frame of the position and yaw you pass to `update`): tuned for a
    /// mech about 5 m tall.
    pub fn new(rest: [Vec3; 2]) -> MechGait {
        let foot = Foot { planted: Vec3::ZERO, from: Vec3::ZERO, pos: Vec3::ZERO };
        MechGait {
            stride: 1.6,
            lift: 0.5,
            duty: 0.6,
            dip: 0.18,
            dip_hz: 2.2,
            roll: 0.07,
            settle_hz: 1.4,
            min_speed: 0.05,
            rest,
            feet: [foot; 2],
            phase: 0.0,
            time: 0.0,
            land_time: -10.0,
            land_strength: 0.0,
            roll_now: 0.0,
            last: None,
            stomps: Vec::new(),
        }
    }

    /// A gait for `rig`'s feet as they stand now, in the frame of `body`
    /// (the node you move: its position and yaw are what `update` takes).
    pub fn from_rig(rig: &Humanoid, body: Node) -> Option<MechGait> {
        let inv = body.world()?.inverse();
        let at = |n: &str| Some(inv.transform_point(rig.skeleton.world(rig.index(n)?)?.pos));
        Some(MechGait::new([at("foot_l")?, at("foot_r")?]))
    }

    /// The same gait scaled for a body `k` times the size (stride, lift, dip).
    pub fn scaled(mut self, k: f32) -> MechGait {
        self.stride *= k;
        self.lift *= k;
        self.dip *= k;
        self
    }

    fn home(&self, i: usize, pos: Vec3, yaw: f32) -> Vec3 {
        pos + Quat::from_rotation_y(yaw) * self.rest[i]
    }

    /// The cycle's phase (0–1; foot 0 lifts at `duty`, foot 1 half a cycle later).
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Foot `i`'s swing (0–1) if it's in the air, None if planted.
    pub fn swing(&self, i: usize) -> Option<f32> {
        let q = (self.phase + 0.5 * i as f32).fract();
        (q >= self.duty).then(|| (q - self.duty) / (1.0 - self.duty).max(1e-4))
    }

    /// Advances `dt` seconds with the body at `pos` (on the ground), facing
    /// `yaw` (radians about +y; 0 faces the body frame's own +z), moving at
    /// `vel` (m/s). Returns this frame's footfalls.
    pub fn update(&mut self, dt: f32, pos: Vec3, yaw: f32, vel: Vec3) -> &[Stomp] {
        self.stomps.clear();
        let (prev_pos, prev_yaw) = self.last.unwrap_or((pos, yaw));
        if self.last.is_none() {
            for i in 0..2 {
                let h = self.home(i, pos, yaw);
                self.feet[i] = Foot { planted: h, from: h, pos: h };
            }
        }
        self.last = Some((pos, yaw));
        if !(dt > 0.0) {
            return &self.stomps;
        }
        let flat = Vec3::new(vel.x, 0.0, vel.z);
        let speed = flat.length();
        let moving = speed > self.min_speed;
        let off_home = (0..2).any(|i| self.swing(i).is_none() && self.feet[i].planted.distance(self.home(i, pos, yaw)) > 0.12 * self.stride);
        let swinging = (0..2).any(|i| self.swing(i).is_some());
        let dphase = if moving {
            speed * dt / self.stride.max(1e-3)
        } else if swinging || off_home {
            self.settle_hz * dt
        } else {
            0.0
        };
        // Where a foot lands: under the hip, ahead by half the distance the body covers while it's planted.
        let lead = if moving { flat.normalize() * (0.5 * self.duty * self.stride * smooth(speed / (3.0 * self.min_speed))) } else { Vec3::ZERO };
        let old = self.phase;
        self.phase = (old + dphase).rem_euclid(64.0);
        let t0 = self.time;
        self.time += dt;
        for i in 0..2 {
            let off = 0.5 * i as f32;
            let (qa, qb) = (old + off, old + off + dphase);
            // Lift-offs and landings crossed this frame, in order (big frames can hold both).
            let mut k = qa.floor();
            while k <= qb {
                for (edge, landing) in [(k + self.duty, false), (k + 1.0, true)] {
                    if edge > qa && edge <= qb {
                        let f = if dphase > 0.0 { (edge - qa) / dphase } else { 1.0 };
                        let p = prev_pos.lerp(pos, f);
                        let y = prev_yaw + crate::motion::wrap_angle(yaw - prev_yaw) * f;
                        if landing {
                            let at = self.home(i, p, y) + lead;
                            let len = self.feet[i].from.distance(at);
                            let strength = (0.35 + 0.65 * len / (self.duty * self.stride).max(1e-3)).clamp(0.0, 1.0);
                            self.feet[i].planted = at;
                            self.stomps.push(Stomp { foot: i, pos: at, strength });
                            self.land_time = t0 + f * dt;
                            self.land_strength = strength;
                        } else {
                            self.feet[i].from = self.feet[i].planted;
                        }
                    }
                }
                k += 1.0;
            }
            self.feet[i].pos = match self.swing(i) {
                None => self.feet[i].planted,
                Some(s) => {
                    let to = self.home(i, pos, yaw) + lead;
                    // Up fast, down hard: the lift peaks early and the foot drops onto the ground.
                    let h = self.lift * (PI * s.powf(0.8)).sin();
                    self.feet[i].from.lerp(to, smooth(s)) + Vec3::Y * h
                }
            };
        }
        // Roll over the planted leg (toward its side), eased.
        let side = |i: usize| self.rest[i].x.signum();
        let want = match (self.swing(0), self.swing(1)) {
            (Some(_), None) => side(1),
            (None, Some(_)) => side(0),
            _ => 0.0,
        } * self.roll
            * if moving { 1.0 } else { 0.5 };
        self.roll_now += (want - self.roll_now) * (1.0 - (-dt * 6.0).exp());
        &self.stomps
    }

    /// The footfalls of the last `update`.
    pub fn stomps(&self) -> &[Stomp] {
        &self.stomps
    }

    /// Where foot `i`'s ankle goes now (world): its IK target.
    pub fn foot(&self, i: usize) -> Vec3 {
        self.feet[i.min(1)].pos
    }

    /// Seconds since the last footfall.
    pub fn since_stomp(&self) -> f32 {
        self.time - self.land_time
    }

    /// Add to the body's position: the dip after a footfall (a spring from
    /// the moment of the stomp, deeper for a harder one).
    pub fn body_offset(&self) -> Vec3 {
        let d = self.dip * self.land_strength * kick(self.since_stomp(), self.dip_hz, 1.0);
        Vec3::new(0.0, -d, 0.0)
    }

    /// The body's roll over the planted leg (radians about its forward axis).
    pub fn roll_angle(&self) -> f32 {
        self.roll_now
    }

    /// Turn the body by this (after its heading): the hip roll.
    pub fn body_tilt(&self) -> Quat {
        Quat::from_rotation_z(-self.roll_now)
    }

    /// Puts the legs on the feet: `legs[i]` (thigh, shin, foot) to `foot(i)`,
    /// knees toward `forward` (a world direction, the body's facing).
    pub fn apply(&self, rig: &Humanoid, legs: &[TwoBoneIk; 2], forward: Vec3) {
        for (i, leg) in legs.iter().enumerate() {
            leg.apply(&rig.skeleton, self.foot(i), forward);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::vec3;

    const REST: [Vec3; 2] = [vec3(-0.4, 0.3, 0.0), vec3(0.4, 0.3, 0.0)];

    /// Walks `seconds` at `vel` in frames of `dt`; returns the stomps and the gait.
    fn walk(vel: Vec3, seconds: f32, dt: f32) -> (Vec<Stomp>, MechGait) {
        let mut g = MechGait::new(REST);
        let mut out = Vec::new();
        let n = (seconds / dt).round() as usize;
        for k in 0..=n {
            let t = k as f32 * dt;
            out.extend_from_slice(g.update(if k == 0 { 0.0 } else { dt }, vel * t, 0.0, vel));
        }
        (out, g)
    }

    #[test]
    fn steps_follow_the_body_forward_sideways_and_back() {
        for vel in [vec3(0.0, 0.0, 1.2), vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, -0.8)] {
            let (stomps, g) = walk(vel, 4.0, 1.0 / 60.0);
            let dist = vel.length() * 4.0;
            let expect = 2.0 * dist / g.stride;
            assert!((stomps.len() as f32 - expect).abs() <= 2.0, "{} stomps for {expect}", stomps.len());
            // Feet alternate, land ahead in the direction of travel, and stay where they land.
            assert!(stomps.windows(2).all(|w| w[0].foot != w[1].foot));
            let body = vel * 4.0;
            for i in 0..2 {
                let f = g.foot(i);
                assert!(f.distance(body + REST[i]) < g.stride, "foot {i} keeps up: {f:?}");
            }
            assert!(stomps.iter().all(|s| s.strength > 0.3 && s.strength <= 1.0 && s.pos.y == 0.3));
        }
    }

    #[test]
    fn it_replays_and_matches_at_120_hz() {
        let v = vec3(0.6, 0.0, 1.1);
        let (a, ga) = walk(v, 3.0, 1.0 / 60.0);
        let (b, _) = walk(v, 3.0, 1.0 / 60.0);
        assert_eq!(a, b, "same inputs, same steps");
        let (c, gc) = walk(v, 3.0, 1.0 / 120.0);
        assert_eq!(a.len(), c.len());
        for (x, y) in a.iter().zip(&c) {
            assert!(x.pos.distance(y.pos) < 0.02 && (x.strength - y.strength).abs() < 0.02, "{x:?} {y:?}");
        }
        assert!((ga.body_offset().y - gc.body_offset().y).abs() < 0.01);
    }

    #[test]
    fn stopping_finishes_the_step_and_settles() {
        let mut g = MechGait::new(REST);
        let v = vec3(0.0, 0.0, 1.0);
        let mut p = Vec3::ZERO;
        for _ in 0..100 {
            p += v * (1.0 / 60.0);
            g.update(1.0 / 60.0, p, 0.0, v);
        }
        for _ in 0..300 {
            g.update(1.0 / 60.0, p, 0.0, Vec3::ZERO);
        }
        for i in 0..2 {
            assert!(g.swing(i).is_none(), "both feet down");
            assert!(g.foot(i).distance(p + REST[i]) < 0.12 * g.stride + 1e-3, "under the body: {:?}", g.foot(i));
        }
        assert!(g.body_offset().y.abs() < 1e-3 && g.roll_angle().abs() < 0.01, "settled");
        let before = g.phase();
        g.update(1.0 / 60.0, p, 0.0, Vec3::ZERO);
        assert_eq!(g.phase(), before, "standing still doesn't step");
    }

    #[test]
    fn the_body_dips_after_a_footfall() {
        let mut g = MechGait::new(REST);
        let v = vec3(0.0, 0.0, 1.5);
        let mut deepest = 0.0f32;
        for k in 0..240 {
            let s = g.update(1.0 / 60.0, v * (k as f32 / 60.0), 0.0, v).len();
            let _ = s;
            deepest = deepest.min(g.body_offset().y);
        }
        assert!(deepest < -0.08, "{deepest}");
        assert!(g.body_tilt().dot(Quat::IDENTITY).abs() < 1.0);
    }
}
