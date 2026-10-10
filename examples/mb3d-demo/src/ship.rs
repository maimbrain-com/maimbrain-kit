//! The faceted low-poly ship, built entirely in code from SDK mesh builders.
//! Its nose points down −z, up is +y.

use std::f32::consts::{FRAC_PI_2, PI};

use maimbrain::gfx3d::{Mat4, MeshData, Quat, Transform, Vec3, vec3};

/// One part of the ship: its mesh and which material slot it uses.
pub struct Part {
    pub mesh: MeshData,
    pub material: Slot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Hull,
    Accent,
    Glass,
    Dark,
    Glow,
}

fn rot_x(m: MeshData, a: f32) -> MeshData {
    m.transformed(&Mat4::from_trs(Vec3::ZERO, Quat::from_rotation_x(a), Vec3::ONE))
}

fn place(m: &MeshData, t: Transform) -> MeshData {
    m.transformed(&t.matrix())
}

pub fn parts() -> Vec<Part> {
    // Fuselage: a lathe with few segments, flat shaded, laid along −z.
    let body = MeshData::lathe(&[[0.0, -0.95], [0.17, -0.95], [0.27, -0.45], [0.3, 0.25], [0.2, 0.8], [0.0, 1.3]], 6);
    let body = rot_x(body.flat_shaded(), -FRAC_PI_2);

    // Wings: a swept planform extruded thin, then laid flat (polygon y → −z).
    let wing = MeshData::extrude(&[[0.22, 0.35], [1.55, -0.5], [1.62, -0.78], [0.24, -0.72]], 0.07);
    let right = rot_x(wing, -FRAC_PI_2);
    let left = right.transformed(&Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, vec3(-1.0, 1.0, 1.0)));
    let mut wings = right;
    wings.merge(&left);
    let wings = wings.flat_shaded();

    // Tail fin: a polygon in the (back, up) plane turned to stand on the spine.
    let fin = MeshData::extrude(&[[-0.25, 0.1], [-0.95, 0.1], [-1.1, 0.62], [-0.85, 0.62]], 0.05);
    let fin = fin.transformed(&Mat4::from_trs(Vec3::ZERO, Quat::from_rotation_y(FRAC_PI_2), Vec3::ONE)).flat_shaded();

    // Cockpit canopy: a squashed, faceted icosphere.
    let canopy = MeshData::icosphere(0.2, 1).flat_shaded();
    let canopy = canopy.transformed(&Mat4::from_trs(vec3(0.0, 0.17, -0.4), Quat::IDENTITY, vec3(0.9, 0.65, 2.0)));

    // Engine nacelles along the wings' trailing edge.
    let nacelle = rot_x(MeshData::frustum(0.13, 0.1, 0.75, 8).flat_shaded(), FRAC_PI_2);
    let mut nacelles = place(&nacelle, Transform::at(vec3(0.5, -0.04, 0.45)));
    nacelles.merge(&place(&nacelle, Transform::at(vec3(-0.5, -0.04, 0.45))));
    let nose_cone = rot_x(MeshData::cone(0.11, 0.3, 6).flat_shaded(), -FRAC_PI_2);
    nacelles.merge(&place(&nose_cone, Transform::at(vec3(0.5, -0.04, -0.07))));
    nacelles.merge(&place(&nose_cone, Transform::at(vec3(-0.5, -0.04, -0.07))));

    // Engine glow discs (an emissive, unlit material: they bloom).
    let disc = rot_x(MeshData::cylinder(0.085, 0.04, 10), FRAC_PI_2);
    let mut glow = place(&disc, Transform::at(vec3(0.5, -0.04, 0.84)));
    glow.merge(&place(&disc, Transform::at(vec3(-0.5, -0.04, 0.84))));
    glow.merge(&place(&rot_x(MeshData::cylinder(0.12, 0.04, 10), FRAC_PI_2), Transform::at(vec3(0.0, 0.0, 0.97))));

    vec![
        Part { mesh: body, material: Slot::Hull },
        Part { mesh: wings, material: Slot::Accent },
        Part { mesh: fin, material: Slot::Accent },
        Part { mesh: canopy, material: Slot::Glass },
        Part { mesh: nacelles, material: Slot::Dark },
        Part { mesh: glow, material: Slot::Glow },
    ]
}

/// Where the engines' exhausts are, in ship space (trails attach here).
pub const EXHAUSTS: [Vec3; 2] = [vec3(0.5, -0.04, 0.9), vec3(-0.5, -0.04, 0.9)];

/// The flight path: a tilted loop around the platform. Returns position and
/// orientation (nose along the velocity, banked into turns) at time `t`.
pub fn flight(t: f32) -> (Vec3, Quat) {
    let p = |t: f32| {
        let a = t * 0.42;
        vec3(3.4 * a.sin(), 1.75 + 0.55 * (2.0 * a + 0.7).sin(), 2.6 * a.cos() - 0.3)
    };
    let h = 1.0 / 120.0;
    let (p0, p1, p2) = (p(t - h), p(t), p(t + h));
    let vel = (p2 - p0) / (2.0 * h);
    let acc = (p2 - p1 * 2.0 + p0) / (h * h);
    let fwd = vel.normalize();
    let right = fwd.cross(Vec3::Y).normalize();
    let bank = (acc.dot(right) * 0.12).clamp(-1.0, 1.0);
    let rot = Quat::look_rotation(fwd, Vec3::Y) * Quat::from_rotation_z(-bank * PI * 0.25);
    (p1, rot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_is_low_poly_and_points_down_minus_z() {
        let parts = parts();
        let verts: usize = parts.iter().map(|p| p.mesh.vertex_count()).sum();
        assert!(verts < 3000, "{verts} vertices");
        let body = &parts[0].mesh;
        let nose = body.positions.iter().fold(0.0f32, |m, p| m.min(p.z));
        assert!(nose < -1.2, "nose at z = {nose}");
    }

    #[test]
    fn flight_is_smooth() {
        let mut last = flight(0.0).0;
        for i in 1..2000 {
            let (p, q) = flight(i as f32 / 60.0);
            assert!(p.distance(last) < 0.1);
            assert!((q.forward().length() - 1.0).abs() < 1e-3);
            last = p;
        }
    }
}
