//! Inverse kinematics for rigs posed in code: [`TwoBoneIk`] puts a foot (or
//! a hand) on a point with the knee (or elbow) bending toward a pole
//! direction and, optionally, the foot kept flat on the ground; [`LookAt`]
//! turns a head, a turret or a dish toward a target within yaw and pitch
//! limits at a limited speed.
//!
//! Both work in world space on the pose the bones have now (clips, code and
//! the body's own transform included), so call them last in a frame's
//! posing. Everything is a function of the inputs (and, for `LookAt`, of
//! the `dt`s), so replays match.
//!
//! ```ignore
//! let rig = Humanoid::find(model, mech)?;
//! let legs = [TwoBoneIk::leg(&rig, 'l')?, TwoBoneIk::leg(&rig, 'r')?];
//! let mut head = LookAt::new(rig.index("head")?);
//! // every frame, after posing the body:
//! let fwd = body_rot * Vec3::Z;
//! legs[0].apply(&rig.skeleton, planted_l, fwd);    // knees bend forward, feet flat
//! head.update(dt, &rig.skeleton, Some(player));
//! head.apply(&rig.skeleton);
//! ```

use super::math::{Quat, Vec3};
use super::rig::Humanoid;
use super::skin::Skeleton;
use crate::motion::wrap_angle;

/// The shortest rotation taking direction `a` to direction `b`.
pub fn arc(a: Vec3, b: Vec3) -> Quat {
    let (a, b) = (a.normalize(), b.normalize());
    let d = a.dot(b);
    if d < -0.99999 {
        let mut axis = Vec3::X.cross(a);
        if axis.length() < 1e-4 {
            axis = Vec3::Y.cross(a);
        }
        return Quat::from_axis_angle(axis, std::f32::consts::PI);
    }
    let c = a.cross(b);
    Quat { x: c.x, y: c.y, z: c.z, w: 1.0 + d }.normalize()
}

/// The rotation taking the frame (direction `u0`, plane normal `n0`) to
/// (`u1`, `n1`): turns a bone onto a new direction *and* keeps its hinge
/// in the new bend plane.
fn align(u0: Vec3, n0: Vec3, u1: Vec3, n1: Vec3) -> Quat {
    let frame = |u: Vec3, n: Vec3| {
        let u = u.normalize();
        let n = (n - u * n.dot(u)).normalize();
        Quat::from_basis(u, n, u.cross(n))
    };
    (frame(u1, n1) * frame(u0, n0).conjugate()).normalize()
}

/// Solves a two-bone chain: joints at `a` (hip/shoulder), `b` (knee/elbow)
/// and `c` (ankle/wrist), all in one space, reaching for `target`, the
/// middle joint bending toward `pole` (a direction: forward for knees,
/// backward for elbows). Returns the turn for the upper bone (about `a`),
/// the turn for the lower bone (about the new `b`, after the first) and how
/// far the chain reaches (0–1 of its length: 1 is straight). A target out
/// of reach is reached for (the chain straightens toward it).
pub fn two_bone(a: Vec3, b: Vec3, c: Vec3, target: Vec3, pole: Vec3) -> (Quat, Quat, f32) {
    let (l1, l2) = (a.distance(b), b.distance(c));
    if l1 < 1e-6 || l2 < 1e-6 {
        return (Quat::IDENTITY, Quat::IDENTITY, 0.0);
    }
    let to = target - a;
    let dir = if to.length() > 1e-6 { to.normalize() } else { (c - a).normalize() };
    let d = to.length().clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.9999);
    // The bend: the pole's part across the reach direction, else the chain's own bend.
    let across = |v: Vec3| v - dir * v.dot(dir);
    let mut bend = across(pole).normalize();
    if bend.length() < 1e-4 {
        bend = across(b - a).normalize();
    }
    if bend.length() < 1e-4 {
        bend = across(Vec3::Z).normalize();
    }
    let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
    let knee = a + dir * (l1 * cos_a) + bend * (l1 * (1.0 - cos_a * cos_a).max(0.0).sqrt());
    let end = a + dir * d;
    // The upper bone: onto the new knee, its bend plane onto the new one.
    let n0 = {
        let n = (b - a).cross(c - b);
        if n.length() > 1e-5 * l1 * l2 { n } else { (b - a).cross(bend) }
    };
    let n1 = (knee - a).cross(end - knee);
    let q1 = if n1.length() > 1e-6 && n0.length() > 1e-6 { align(b - a, n0, knee - a, n1) } else { arc(b - a, knee - a) };
    // The lower bone: a hinge in that plane onto the end.
    let q2 = arc(q1 * (c - b), end - knee);
    (q1, q2, d / (l1 + l2))
}

/// A two-bone limb (thigh, shin, foot or upper arm, forearm, hand) put on a
/// target by IK.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBoneIk {
    pub upper: usize,
    pub lower: usize,
    /// The bone at the end (foot or hand): `flat` turns it, the others carry it.
    pub end: usize,
    /// Keep the end bone as it is at rest relative to the ground (the
    /// model's heading, no pitch or roll): feet stay flat on the floor
    /// whatever the knee and the body do.
    pub flat: bool,
}

impl TwoBoneIk {
    pub fn new(upper: usize, lower: usize, end: usize) -> TwoBoneIk {
        TwoBoneIk { upper, lower, end, flat: false }
    }

    /// A humanoid's leg (`side` 'l' or 'r'), feet flat.
    pub fn leg(rig: &Humanoid, side: char) -> Option<TwoBoneIk> {
        Some(TwoBoneIk { flat: true, ..TwoBoneIk::new(rig.index(&format!("thigh_{side}"))?, rig.index(&format!("shin_{side}"))?, rig.index(&format!("foot_{side}"))?) })
    }

    /// A humanoid's arm (`side` 'l' or 'r').
    pub fn arm(rig: &Humanoid, side: char) -> Option<TwoBoneIk> {
        Some(TwoBoneIk::new(rig.index(&format!("upper_arm_{side}"))?, rig.index(&format!("forearm_{side}"))?, rig.index(&format!("hand_{side}"))?))
    }

    /// Puts the end bone's joint (the ankle) on `target` (world), the middle
    /// joint bending toward `pole` (a world direction). Returns how far the
    /// limb reaches (0–1; near 1 it's straight). Call it after everything
    /// else that poses the limb's parents this frame.
    pub fn apply(&self, sk: &Skeleton, target: Vec3, pole: Vec3) -> f32 {
        let (Some(u), Some(l), Some(e)) = (sk.world(self.upper), sk.world(self.lower), sk.world(self.end)) else { return 0.0 };
        let (q1, q2, reach) = two_bone(u.pos, l.pos, e.pos, target, pole);
        sk.set_world_rot(self.upper, q1 * u.rot);
        // Read the lower bone again: the upper bone's turn carried it (and any twist bones between).
        let l2 = sk.world(self.lower).map_or(q1 * l.rot, |t| t.rot);
        sk.set_world_rot(self.lower, q2 * l2);
        if self.flat {
            sk.set_world_rot(self.end, sk.ground_rot() * sk.rest_rot(self.end));
        }
        reach
    }
}

/// A head, turret or dish turning toward a target: yaw (about the model's
/// up) and pitch within limits, at most `speed` radians per second, back to
/// straight ahead without a target. Set its state with `update` and pose
/// the bone with `apply`, after the body and chest are posed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookAt {
    pub bone: usize,
    /// Which way the bone faces at rest, in the model's frame (+z for a
    /// model facing +z; −z for one facing −z).
    pub forward: Vec3,
    /// Largest yaw either side (radians).
    pub yaw_limit: f32,
    /// Largest pitch up and down (radians).
    pub pitch_limit: f32,
    /// Fastest turn (radians per second): a heavy turret 1, a head 4.
    pub speed: f32,
    yaw: f32,
    pitch: f32,
}

impl LookAt {
    /// A head facing +z: ±70° yaw, ±35° pitch, 3 rad/s.
    pub fn new(bone: usize) -> LookAt {
        LookAt { bone, forward: Vec3::Z, yaw_limit: 1.2, pitch_limit: 0.6, speed: 3.0, yaw: 0.0, pitch: 0.0 }
    }

    /// The yaw and pitch (radians) that point `forward` along `dir` (both in
    /// the model's frame), before limits.
    pub fn aim(forward: Vec3, dir: Vec3) -> (f32, f32) {
        let ang = |v: Vec3| v.x.atan2(v.z);
        let elev = |v: Vec3| v.y.atan2((v.x * v.x + v.z * v.z).sqrt());
        (wrap_angle(ang(dir) - ang(forward)), elev(dir) - elev(forward))
    }

    /// Turns toward `target` (world; None: back to straight ahead) by at
    /// most `speed × dt`.
    pub fn update(&mut self, dt: f32, sk: &Skeleton, target: Option<Vec3>) {
        let want = match (target, sk.root().and_then(|r| r.world()), sk.world(self.bone)) {
            (Some(t), Some(root), Some(bone)) => {
                let inv = root.inverse();
                let dir = inv.transform_point(t) - inv.transform_point(bone.pos);
                LookAt::aim(self.forward, dir)
            }
            _ => (0.0, 0.0),
        };
        self.step(dt, want);
    }

    /// Moves toward `want` (yaw, pitch), limited.
    pub fn step(&mut self, dt: f32, want: (f32, f32)) {
        let step = self.speed.max(0.0) * dt.max(0.0);
        let (y, p) = (want.0.clamp(-self.yaw_limit, self.yaw_limit), want.1.clamp(-self.pitch_limit, self.pitch_limit));
        self.yaw += (y - self.yaw).clamp(-step, step);
        self.pitch += (p - self.pitch).clamp(-step, step);
    }

    /// The yaw and pitch it's at now (radians).
    pub fn angles(&self) -> (f32, f32) {
        (self.yaw, self.pitch)
    }

    /// The turn it makes, in the model's frame.
    pub fn rotation(&self) -> Quat {
        let side = self.forward.cross(Vec3::Y);
        let side = if side.length() > 1e-5 { side.normalize() } else { Vec3::X };
        Quat::from_rotation_y(self.yaw) * Quat::from_axis_angle(side, self.pitch)
    }

    /// Turns the bone (absolutely, in the model's frame: it keeps looking
    /// whatever the chest does).
    pub fn apply(&self, sk: &Skeleton) {
        sk.orient(self.bone, self.rotation());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::vec3;

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn a_leg_reaches_its_target_and_bends_toward_the_pole() {
        // A straight leg: hip 1 m up, knee at 0.5, ankle at 0.
        let (a, b, c) = (vec3(0.0, 1.0, 0.0), vec3(0.0, 0.5, 0.0), Vec3::ZERO);
        for (target, pole) in [(vec3(0.0, 0.3, 0.1), Vec3::Z), (vec3(0.3, 0.4, -0.2), Vec3::Z), (vec3(0.2, 0.5, 0.0), -Vec3::Z)] {
            let (q1, q2, reach) = two_bone(a, b, c, target, pole);
            let knee = a + q1 * (b - a);
            let end = knee + q2 * (q1 * (c - b));
            assert!(close(end, target), "{end:?} vs {target:?}");
            assert!((knee - a).length() - 0.5 < 1e-4 && (end - knee).length() - 0.5 < 1e-4, "bone lengths kept");
            let mid = (a + target) * 0.5;
            assert!((knee - mid).dot(pole) > 0.0, "the knee bends toward the pole");
            assert!(reach < 1.0);
        }
        // Out of reach: straight toward it.
        let (q1, q2, reach) = two_bone(a, b, c, vec3(0.0, -5.0, 0.0), Vec3::Z);
        let end = a + q1 * (b - a) + q2 * (q1 * (c - b));
        assert!(end.y < 0.01 && reach > 0.99);
    }

    #[test]
    fn a_bent_knee_stays_a_hinge() {
        // A knee already bent forward bends further forward, not sideways.
        let (a, b, c) = (vec3(0.0, 1.0, 0.0), vec3(0.0, 0.52, 0.1), vec3(0.0, 0.0, 0.0));
        let (q1, _, _) = two_bone(a, b, c, vec3(0.0, 0.4, 0.0), Vec3::Z);
        let n = q1 * Vec3::X;
        assert!(n.x > 0.999, "the knee's hinge axis (x) stays x: {n:?}");
    }

    #[test]
    fn look_at_limits_and_rate() {
        let (y, p) = LookAt::aim(Vec3::Z, vec3(1.0, 0.0, 1.0));
        assert!((y - std::f32::consts::FRAC_PI_4).abs() < 1e-5 && p.abs() < 1e-6);
        let mut l = LookAt { speed: 1.0, ..LookAt::new(0) };
        l.step(0.1, (2.0, -2.0));
        assert!((l.angles().0 - 0.1).abs() < 1e-6 && (l.angles().1 + 0.1).abs() < 1e-6, "rate limited");
        for _ in 0..100 {
            l.step(0.1, (2.0, -2.0));
        }
        assert_eq!(l.angles(), (1.2, -0.6), "held at the limits");
        // Pitch up turns the forward up.
        let up = LookAt { pitch: 0.5, ..LookAt::new(0) }.rotation() * Vec3::Z;
        assert!(up.y > 0.4, "{up:?}");
        // 60 Hz and 120 Hz end in the same place.
        let (mut a, mut b) = (LookAt::new(0), LookAt::new(0));
        for _ in 0..30 {
            a.step(1.0 / 60.0, (0.8, 0.2));
        }
        for _ in 0..60 {
            b.step(1.0 / 120.0, (0.8, 0.2));
        }
        assert!((a.angles().0 - b.angles().0).abs() < 1e-4);
        let sk = Skeleton::chain(crate::gfx3d::Node::new().unwrap(), &[Vec3::ZERO, Vec3::Y, Vec3::Y * 2.0]).unwrap();
        a.update(1.0 / 60.0, &sk, Some(vec3(0.0, 0.0, 5.0)));
        a.apply(&sk);
        TwoBoneIk { flat: true, ..TwoBoneIk::new(0, 1, 2) }.apply(&sk, Vec3::ZERO, Vec3::Z);
    }
}
