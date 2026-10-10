//! Vectors, quaternions and matrices for mb3d: right-handed, +y up, meters.
//! Cameras look down their local −z.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = vec3(1.0, 1.0, 1.0);
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        vec3(x, y, z)
    }
    pub const fn splat(v: f32) -> Vec3 {
        vec3(v, v, v)
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        vec3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn distance(self, o: Vec3) -> f32 {
        (self - o).length()
    }
    /// Unit vector in the same direction, or zero for a zero vector.
    pub fn normalize(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 { self / l } else { Vec3::ZERO }
    }
    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }
    pub fn min(self, o: Vec3) -> Vec3 {
        vec3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: Vec3) -> Vec3 {
        vec3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl From<[f32; 3]> for Vec3 {
    fn from(a: [f32; 3]) -> Vec3 {
        vec3(a[0], a[1], a[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f32) -> Vec3 {
        vec3(self.x * k, self.y * k, self.z * k)
    }
}
impl Mul<Vec3> for f32 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}
/// Component-wise.
impl Mul<Vec3> for Vec3 {
    type Output = Vec3;
    fn mul(self, o: Vec3) -> Vec3 {
        vec3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}
impl Div<f32> for Vec3 {
    type Output = Vec3;
    fn div(self, k: f32) -> Vec3 {
        vec3(self.x / k, self.y / k, self.z / k)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}
impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, k: f32) {
        *self = *self * k;
    }
}

/// A rotation (unit quaternion, x y z w).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Quat {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    /// Rotation of `angle` radians around `axis` (right-hand rule).
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let a = axis.normalize();
        let (s, c) = (angle * 0.5).sin_cos();
        Quat { x: a.x * s, y: a.y * s, z: a.z * s, w: c }
    }
    pub fn from_rotation_x(angle: f32) -> Quat {
        Quat::from_axis_angle(Vec3::X, angle)
    }
    pub fn from_rotation_y(angle: f32) -> Quat {
        Quat::from_axis_angle(Vec3::Y, angle)
    }
    pub fn from_rotation_z(angle: f32) -> Quat {
        Quat::from_axis_angle(Vec3::Z, angle)
    }
    /// Yaw around +y, then pitch around the new x, then roll around the new z.
    pub fn from_euler(yaw: f32, pitch: f32, roll: f32) -> Quat {
        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll)
    }

    /// A rotation from orthonormal basis columns (local x, y, z in world space).
    pub fn from_basis(x: Vec3, y: Vec3, z: Vec3) -> Quat {
        let tr = x.x + y.y + z.z;
        let q = if tr > 0.0 {
            let s = (tr + 1.0).sqrt() * 2.0;
            Quat { x: (y.z - z.y) / s, y: (z.x - x.z) / s, z: (x.y - y.x) / s, w: 0.25 * s }
        } else if x.x > y.y && x.x > z.z {
            let s = (1.0 + x.x - y.y - z.z).sqrt() * 2.0;
            Quat { x: 0.25 * s, y: (y.x + x.y) / s, z: (z.x + x.z) / s, w: (y.z - z.y) / s }
        } else if y.y > z.z {
            let s = (1.0 + y.y - x.x - z.z).sqrt() * 2.0;
            Quat { x: (y.x + x.y) / s, y: 0.25 * s, z: (z.y + y.z) / s, w: (z.x - x.z) / s }
        } else {
            let s = (1.0 + z.z - x.x - y.y).sqrt() * 2.0;
            Quat { x: (z.x + x.z) / s, y: (z.y + y.z) / s, z: 0.25 * s, w: (x.y - y.x) / s }
        };
        q.normalize()
    }

    /// The rotation that points local −z along `forward`, keeping local +y near `up`.
    pub fn look_rotation(forward: Vec3, up: Vec3) -> Quat {
        let z = (-forward).normalize();
        if z == Vec3::ZERO {
            return Quat::IDENTITY;
        }
        let mut x = up.cross(z).normalize();
        if x == Vec3::ZERO {
            x = if z.y.abs() < 0.9 { Vec3::Y.cross(z) } else { Vec3::X.cross(z) }.normalize();
        }
        let y = z.cross(x);
        Quat::from_basis(x, y, z)
    }

    pub fn normalize(self) -> Quat {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if l > 1e-12 { Quat { x: self.x / l, y: self.y / l, z: self.z / l, w: self.w / l } } else { Quat::IDENTITY }
    }
    pub fn conjugate(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }
    pub fn dot(self, o: Quat) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }
    /// Spherical interpolation along the shorter arc.
    pub fn slerp(self, mut o: Quat, t: f32) -> Quat {
        let mut d = self.dot(o);
        if d < 0.0 {
            o = Quat { x: -o.x, y: -o.y, z: -o.z, w: -o.w };
            d = -d;
        }
        if d > 0.9995 {
            let l = |a: f32, b: f32| a + (b - a) * t;
            return Quat { x: l(self.x, o.x), y: l(self.y, o.y), z: l(self.z, o.z), w: l(self.w, o.w) }.normalize();
        }
        let th = d.acos();
        let s = th.sin();
        let (a, b) = (((1.0 - t) * th).sin() / s, (t * th).sin() / s);
        Quat { x: self.x * a + o.x * b, y: self.y * a + o.y * b, z: self.z * a + o.z * b, w: self.w * a + o.w * b }
    }
    /// This rotation's local −z in world space.
    pub fn forward(self) -> Vec3 {
        self * -Vec3::Z
    }
    pub fn right(self) -> Vec3 {
        self * Vec3::X
    }
    pub fn up(self) -> Vec3 {
        self * Vec3::Y
    }
}

/// Composition: `a * b` applies `b` first.
impl Mul for Quat {
    type Output = Quat;
    fn mul(self, b: Quat) -> Quat {
        let a = self;
        Quat {
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        }
    }
}

/// Rotates a vector.
impl Mul<Vec3> for Quat {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        let u = vec3(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
}

/// The rotation that makes something at `eye` look at `target`.
pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Quat {
    Quat::look_rotation(target - eye, up)
}

/// Column-major 4×4 matrix (for transforming mesh data).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

    pub fn from_trs(t: Vec3, r: Quat, s: Vec3) -> Mat4 {
        let x = r * Vec3::X * s.x;
        let y = r * Vec3::Y * s.y;
        let z = r * Vec3::Z * s.z;
        Mat4([x.x, x.y, x.z, 0.0, y.x, y.y, y.z, 0.0, z.x, z.y, z.z, 0.0, t.x, t.y, t.z, 1.0])
    }
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        vec3(m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12], m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13], m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14])
    }
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        vec3(m[0] * v.x + m[4] * v.y + m[8] * v.z, m[1] * v.x + m[5] * v.y + m[9] * v.z, m[2] * v.x + m[6] * v.y + m[10] * v.z)
    }
    /// For normals: the inverse-transpose of the upper 3×3, applied to `n`.
    pub fn transform_normal(&self, n: Vec3) -> Vec3 {
        let m = &self.0;
        let (a, b, c) = (vec3(m[0], m[1], m[2]), vec3(m[4], m[5], m[6]), vec3(m[8], m[9], m[10]));
        let (x, y, z) = (b.cross(c), c.cross(a), a.cross(b));
        (x * n.x + y * n.y + z * n.z).normalize() * if a.dot(x) < 0.0 { -1.0 } else { 1.0 }
    }
    /// The inverse of an affine matrix (rotation, scale and translation, as
    /// `from_trs` makes); the identity if it's singular.
    pub fn inverse(&self) -> Mat4 {
        let m = &self.0;
        let (a, b, c) = (vec3(m[0], m[1], m[2]), vec3(m[4], m[5], m[6]), vec3(m[8], m[9], m[10]));
        let det = a.dot(b.cross(c));
        if det.abs() < 1e-20 || !det.is_finite() {
            return Mat4::IDENTITY;
        }
        // Rows of the inverse 3×3 are the cross products over the determinant.
        let (r0, r1, r2) = (b.cross(c) / det, c.cross(a) / det, a.cross(b) / det);
        let t = vec3(m[12], m[13], m[14]);
        let it = -vec3(r0.dot(t), r1.dot(t), r2.dot(t));
        Mat4([r0.x, r1.x, r2.x, 0.0, r0.y, r1.y, r2.y, 0.0, r0.z, r1.z, r2.z, 0.0, it.x, it.y, it.z, 1.0])
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut r = [0.0; 16];
        for c in 0..4 {
            for row in 0..4 {
                r[c * 4 + row] = (0..4).map(|k| self.0[k * 4 + row] * o.0[c * 4 + k]).sum();
            }
        }
        Mat4(r)
    }
}

/// A node's local transform: position, rotation, scale (40 bytes, SPEC §5.4).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Transform {
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform { pos: Vec3::ZERO, rot: Quat::IDENTITY, scale: Vec3::ONE }
    }
}

impl Transform {
    pub fn at(pos: Vec3) -> Transform {
        Transform { pos, ..Default::default() }
    }
    pub fn with_rot(mut self, rot: Quat) -> Transform {
        self.rot = rot;
        self
    }
    pub fn with_scale(mut self, s: f32) -> Transform {
        self.scale = Vec3::splat(s);
        self
    }
    /// A different scale per axis.
    pub fn with_scale3(mut self, s: Vec3) -> Transform {
        self.scale = s;
        self
    }
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(self.pos, self.rot, self.scale)
    }
}

/// Linear RGB from an sRGB hex color `0xRRGGBB` (what color pickers show).
pub fn srgb(hex: u32) -> [f32; 3] {
    let c = |s: u32| {
        let v = ((hex >> s) & 0xff) as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    [c(16), c(8), c(0)]
}

/// Linear RGBA from `0xRRGGBBAA` (sRGB color, linear alpha).
pub fn srgba(hex: u32) -> [f32; 4] {
    let [r, g, b] = srgb(hex >> 8);
    [r, g, b, (hex & 0xff) as f32 / 255.0]
}

/// Tweens and keyed poses (`motion::PoseKeys`) of positions.
impl crate::motion::Lerp for Vec3 {
    fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
        a.lerp(b, t)
    }
}

/// Tweens and keyed poses of rotations: along the shorter arc.
impl crate::motion::Lerp for Quat {
    fn lerp(a: Quat, b: Quat, t: f32) -> Quat {
        a.slerp(b, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn look_rotation_points_minus_z_at_the_target() {
        let q = look_at(vec3(0.0, 0.0, 0.0), vec3(3.0, 4.0, -5.0), Vec3::Y);
        assert!(close(q.forward(), vec3(3.0, 4.0, -5.0).normalize()));
        assert!(q.up().y > 0.0);
    }

    #[test]
    fn quaternions_compose_and_rotate() {
        let a = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(close(a * Vec3::X, -Vec3::Z));
        let b = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        // b first, then a.
        assert!(close((a * b) * Vec3::Y, a * (b * Vec3::Y)));
        let h = Quat::IDENTITY.slerp(a, 0.5);
        assert!(close(h * Vec3::X, vec3(0.5f32.sqrt(), 0.0, -(0.5f32.sqrt()))));
    }

    #[test]
    fn srgb_round_numbers() {
        assert_eq!(srgb(0xffffff), [1.0, 1.0, 1.0]);
        assert!((srgb(0x808080)[0] - 0.2158).abs() < 1e-3);
    }
}
