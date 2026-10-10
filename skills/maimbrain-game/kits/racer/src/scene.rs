//! The 3D scene, built once in `init` and moved every frame from the sim:
//! a sunset sky (also the lighting), fog that hides where the road ends, a
//! toon-shaded, outlined car made in code with spinning wheels, instanced
//! traffic, hazards, coins and boost pads, roadside props in chunks that
//! leapfrog ahead, particles (exhaust, boost flames, tyre smoke, wind,
//! sparks, smoke), 3D text pops and depth of field.
//!
//! Draw calls (mb.stats): ~70. Everything that repeats is one instanced node.

use maimbrain::Rng;
use maimbrain::gfx2d::Font;
use maimbrain::gfx3d::{
    self, DepthOfField, Emitter, EmitterDesc, Fog, FogMode, FxBlend, Instance, Material, MaterialDesc, MaterialStyle, MeshData, Node, Post, Quat, Sky, Sun,
    TextDesc, Transform, Vec3, srgb, srgba, vec3,
};

use crate::look;
use crate::sim::{self, Cue, Kind, Phase, Sim, lane_x};

/// Roadside chunks: meters each, and how many are alive (behind the camera into the fog).
const CHUNK: f32 = 32.0;
const CHUNKS: usize = 7;
/// The road mesh repeats every this many meters (lane dashes, rail posts).
const PERIOD: f32 = 8.0;
const POPS: usize = 6;
/// Boost flames move with this share of the road's speed (1 = left behind on the road).
const FLAME_DRIFT: f32 = 0.3;
/// A distance sign over the road every this many meters.
const SIGN_EVERY: u32 = 500;
/// Light gantries over the road: one every `GANTRY_EVERY` m for the first
/// `GANTRY_RUN` m of every `GANTRY_ZONE` m (a strobing light tunnel, with
/// shadow bands sweeping toward the camera), then open road.
const GANTRY_EVERY: f32 = 14.0;
const GANTRY_RUN: f32 = 210.0;
const GANTRY_ZONE: f32 = 640.0;

const PI: f32 = std::f32::consts::PI;

/// The grade at rest (Scene::update ramps fringing, vignette and bloom with speed).
const POST: Post = Post { exposure: 1.0, bloom_strength: 0.5, bloom_threshold: 1.0, vignette: 0.32, chromatic_aberration: 0.05, saturation: 1.2, contrast: 1.08, sharpen: 0.0 };

struct Chunk {
    root: Node,
    palms: Node,
    lamps: Node,
    glows: Node,
    rocks: Node,
    index: i64,
}

struct Pop {
    node: Node,
    born: f32,
    at: Vec3,
    text: String,
    color: [f32; 3],
}

pub struct Scene {
    road: Node,
    car: Node,
    wheels: [Node; 4],
    lights: Material,
    underglow: Node,
    flash: Node,
    flash_at: f32,
    chunks: Vec<Chunk>,
    cars: Node,
    trucks: Node,
    blinkers: Node,
    cones: Node,
    barriers: Node,
    beacons: Node,
    oil: Node,
    coins: Node,
    pads: Node,
    sign: Node,
    sign_label: Node,
    gantries: Node,
    gantry_glows: Node,
    sign_shown: u32,
    list: Vec<Instance>,
    exhaust: Emitter,
    flame: Emitter,
    tyre: Emitter,
    wind: Emitter,
    sparks: Emitter,
    fire: Emitter,
    smoke: Emitter,
    glass: Emitter,
    sparkle: Emitter,
    pops: Vec<Pop>,
    next_pop: usize,
    puff: f32,
    gust: f32,
    rng: Rng,
}

/// Sends an instance list, hiding the node when it's empty (a node with no
/// instances draws its mesh once, as a plain node would).
fn show(node: Node, list: &[Instance]) {
    node.set_visible(!list.is_empty());
    if !list.is_empty() {
        node.set_instances(list);
    }
}

fn mat(desc: MaterialDesc, style: MaterialStyle) -> Material {
    let m = Material::new(&desc).expect("material");
    m.set_style(&style);
    m
}

fn toon(outline: f32, rim: f32) -> MaterialStyle {
    MaterialStyle { outline_color: look::INK, outline_width: outline, toon_bands: 3, rim, ..Default::default() }
}

/// A piece of a model: colored, then moved into place.
fn part(m: MeshData, at: Vec3, rot: Quat, color: u32) -> MeshData {
    m.with_vertex_colors(color).transformed(&Transform::at(at).with_rot(rot).matrix())
}

fn at(m: MeshData, pos: Vec3, color: u32) -> MeshData {
    part(m, pos, Quat::IDENTITY, color)
}

/// A side profile (u along the car, + = front; v up) extruded across `width`,
/// turned so the front faces −z.
fn profile(points: &[[f32; 2]], width: f32) -> MeshData {
    MeshData::extrude(points, width).transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_y(PI / 2.0)).matrix())
}

fn hero_body() -> MeshData {
    let mut m = MeshData::new();
    let lower = [[-1.92, 0.22], [1.92, 0.22], [1.98, 0.42], [1.9, 0.64], [1.2, 0.8], [0.5, 0.86], [-1.4, 0.9], [-1.9, 0.84], [-1.98, 0.5]];
    m.merge(&profile(&lower, 1.72).with_vertex_colors_by(|p, n| if p.y < 0.36 && n.y < 0.5 { look::PAINT_DARK } else { look::PAINT }));
    // The cabin in paint, with glass set into its sides, windscreen and rear window.
    let cabin = [[-1.25, 0.84], [0.55, 0.84], [0.05, 1.32], [-0.95, 1.34], [-1.38, 0.9]];
    m.merge(&profile(&cabin, 1.44).with_vertex_colors(look::PAINT));
    let side = [[-1.12, 0.93], [0.42, 0.93], [0.04, 1.26], [-0.88, 1.27], [-1.2, 0.97]];
    m.merge(&profile(&side, 1.47).with_vertex_colors(look::GLASS));
    m.merge(&part(MeshData::cuboid(vec3(1.2, 0.03, 0.5)), vec3(0.0, 1.13, 1.18), Quat::from_rotation_x(0.79), look::GLASS));
    m.merge(&part(MeshData::cuboid(vec3(1.2, 0.03, 0.56)), vec3(0.0, 1.09, -0.31), Quat::from_rotation_x(-0.77), look::GLASS));
    // A diffuser and a plate on the back.
    m.merge(&part(MeshData::cuboid(vec3(1.2, 0.12, 0.08)), vec3(0.0, 0.26, 2.0), Quat::IDENTITY, 0x1a141fff));
    m.merge(&part(MeshData::cuboid(vec3(0.44, 0.14, 0.04)), vec3(0.0, 0.52, 1.99), Quat::IDENTITY, look::STRIPE));
    let flat = Quat::IDENTITY;
    m.merge(&part(MeshData::cuboid(vec3(1.32, 0.06, 1.02)), vec3(0.0, 1.355, 0.45), flat, look::PAINT));
    // Racing stripes on the roof and the rear deck.
    m.merge(&part(MeshData::cuboid(vec3(0.34, 0.03, 1.02)), vec3(0.0, 1.395, 0.45), flat, look::STRIPE));
    m.merge(&part(MeshData::cuboid(vec3(0.34, 0.03, 0.5)), vec3(0.0, 0.9, 1.62), Quat::from_rotation_x(-0.12), look::STRIPE));
    // Spoiler on two struts.
    m.merge(&part(MeshData::cuboid(vec3(1.78, 0.06, 0.4)), vec3(0.0, 1.14, 1.76), Quat::from_rotation_x(0.12), look::TRIM));
    for x in [-0.55, 0.55] {
        m.merge(&part(MeshData::cuboid(vec3(0.07, 0.26, 0.12)), vec3(x, 0.99, 1.74), flat, look::TRIM));
    }
    // Bumpers, skirts, exhausts.
    m.merge(&part(MeshData::cuboid(vec3(1.8, 0.18, 0.22)), vec3(0.0, 0.32, -1.94), flat, look::TRIM));
    m.merge(&part(MeshData::cuboid(vec3(1.8, 0.18, 0.22)), vec3(0.0, 0.32, 1.94), flat, look::TRIM));
    for x in [-0.87, 0.87] {
        m.merge(&part(MeshData::cuboid(vec3(0.06, 0.14, 2.4)), vec3(x, 0.3, 0.0), flat, look::TRIM));
    }
    for x in [-0.45, 0.45] {
        m.merge(&part(MeshData::cylinder(0.075, 0.26, 8), vec3(x, 0.3, 2.04), Quat::from_rotation_x(PI / 2.0), 0x8a8494ff));
    }
    m.flat_shaded()
}

fn hero_lights() -> MeshData {
    let mut m = MeshData::new();
    for x in [-0.55, 0.55] {
        m.merge(&at(MeshData::cuboid(vec3(0.38, 0.13, 0.06)), vec3(x, 0.62, -1.96), 0xfff0c0ff));
        m.merge(&at(MeshData::cuboid(vec3(0.44, 0.12, 0.06)), vec3(x, 0.72, 1.92), 0xff2038ff));
    }
    m
}

fn wheel(radius: f32, width: f32) -> MeshData {
    // A tyre with a silver hub and two spokes, so the spin shows.
    let mut m = MeshData::cylinder(radius, width, 12).with_vertex_colors_by(|p, n| if n.y.abs() > 0.9 && (p.x * p.x + p.z * p.z).sqrt() < radius * 0.6 { 0xd8d4dcff } else { 0x231e29ff });
    for a in [0.0, PI / 2.0] {
        m.merge(&part(MeshData::cuboid(vec3(radius * 1.2, width + 0.02, 0.07)), Vec3::ZERO, Quat::from_rotation_y(a), 0xb8b2c0ff));
    }
    m.transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_z(PI / 2.0)).matrix()).flat_shaded()
}

/// A boxy sedan in white (instance colors paint it), wheels and lights included.
fn sedan() -> MeshData {
    let mut m = MeshData::new();
    let lower = [[-2.0, 0.24], [2.0, 0.24], [2.05, 0.52], [1.95, 0.74], [1.1, 0.82], [-1.6, 0.85], [-2.0, 0.8], [-2.05, 0.5]];
    m.merge(&profile(&lower, 1.76).with_vertex_colors(0xffffffff));
    let cabin = [[-1.3, 0.8], [0.9, 0.8], [0.35, 1.36], [-1.05, 1.38], [-1.48, 0.86]];
    m.merge(&profile(&cabin, 1.5).with_vertex_colors(0x3a4258ff));
    m.merge(&at(MeshData::cuboid(vec3(1.4, 0.06, 1.12)), vec3(0.0, 1.39, 0.35), 0xffffffff));
    for x in [-0.55, 0.55] {
        m.merge(&at(MeshData::cuboid(vec3(0.42, 0.14, 0.06)), vec3(x, 0.66, 2.03), 0xff3040ff));
    }
    m.merge(&at(MeshData::cuboid(vec3(1.82, 0.16, 0.2)), vec3(0.0, 0.32, 2.02), 0x4a4452ff));
    m.merge(&at(MeshData::cuboid(vec3(1.82, 0.16, 0.2)), vec3(0.0, 0.32, -2.02), 0x4a4452ff));
    let w = wheel(0.34, 0.26);
    for (x, z) in [(-0.8, -1.3), (0.8, -1.3), (-0.8, 1.3), (0.8, 1.3)] {
        m.merge(&w.transformed(&Transform::at(vec3(x, 0.34, z)).matrix()));
    }
    m.flat_shaded()
}

fn truck() -> MeshData {
    let mut m = MeshData::new();
    // Cab at the front (−z), trailer behind it; white so instances tint it.
    m.merge(&at(MeshData::cuboid(vec3(2.0, 2.2, 1.9)), vec3(0.0, 1.35, -3.1), 0xe8e2eaff));
    m.merge(&at(MeshData::cuboid(vec3(1.8, 0.75, 0.06)), vec3(0.0, 2.0, -4.06), 0x2c3346ff));
    m.merge(&at(MeshData::cuboid(vec3(1.6, 0.5, 0.06)), vec3(0.0, 0.9, -4.06), 0x3a3442ff));
    m.merge(&at(MeshData::cuboid(vec3(2.1, 2.75, 6.1)), vec3(0.0, 1.78, 1.0), 0xffffffff));
    // A stripe down the trailer.
    m.merge(&at(MeshData::cuboid(vec3(2.14, 0.3, 6.0)), vec3(0.0, 1.2, 1.0), 0xd2603cff));
    m.merge(&at(MeshData::cuboid(vec3(1.9, 0.14, 0.12)), vec3(0.0, 0.45, 4.1), 0x4a4452ff));
    for x in [-0.7, 0.7] {
        m.merge(&at(MeshData::cuboid(vec3(0.3, 0.16, 0.06)), vec3(x, 0.7, 4.09), 0xff3040ff));
    }
    let w = wheel(0.46, 0.34);
    for z in [-3.1, 2.2, 3.3] {
        for x in [-0.86, 0.86] {
            m.merge(&w.transformed(&Transform::at(vec3(x, 0.46, z)).matrix()));
        }
    }
    m.flat_shaded()
}

fn cone() -> MeshData {
    let mut m = MeshData::cone(0.2, 0.72, 10).transformed(&Transform::at(vec3(0.0, 0.41, 0.0)).matrix());
    m = m.with_vertex_colors_by(|p, _| if (0.32..0.48).contains(&p.y) { 0xfff4e8ff } else { look::CONE });
    m.merge(&at(MeshData::cuboid(vec3(0.46, 0.06, 0.46)), vec3(0.0, 0.03, 0.0), 0x2a2233ff));
    m.flat_shaded()
}

fn barrier() -> MeshData {
    let mut m = MeshData::new();
    for k in 0..6 {
        let c = if k % 2 == 0 { look::CONE } else { 0xfff4e8ff };
        m.merge(&at(MeshData::cuboid(vec3(0.36, 0.42, 0.08)), vec3(-0.9 + 0.36 * k as f32, 0.88, 0.0), c));
    }
    for x in [-0.86, 0.86] {
        m.merge(&at(MeshData::cuboid(vec3(0.08, 0.9, 0.36)), vec3(x, 0.45, 0.0), 0x4a4452ff));
    }
    m.merge(&at(MeshData::cuboid(vec3(0.24, 0.16, 0.16)), vec3(0.0, 1.17, 0.0), 0x2a2233ff));
    m.flat_shaded()
}

fn oil_mesh() -> MeshData {
    let mut pts = Vec::new();
    for k in 0..14 {
        let a = k as f32 / 14.0 * std::f32::consts::TAU;
        let r = 1.0 + 0.16 * (a * 3.0).sin() + 0.1 * (a * 5.0 + 1.0).cos();
        pts.push([0.95 * r * a.cos(), 1.7 * r * a.sin()]);
    }
    MeshData::extrude(&pts, 0.02).transformed(&Transform::at(vec3(0.0, 0.025, 0.0)).with_rot(Quat::from_rotation_x(-PI / 2.0)).matrix())
}

fn coin() -> MeshData {
    let mut m = MeshData::cylinder(0.34, 0.08, 16).with_vertex_colors(0xffffffff);
    m.merge(&MeshData::cylinder(0.2, 0.1, 12).with_vertex_colors(0xffe7a0ff));
    m.transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_x(PI / 2.0)).matrix())
}

/// A boost pad: three chevrons pointing down the road (toward the horizon,
/// the way you're driving), brightest at the front so they read as "go".
fn pad() -> MeshData {
    let mut m = at(MeshData::cuboid(vec3(1.9, 0.04, 3.4)), vec3(0.0, 0.02, 0.0), 0x0b2a33ff);
    // In the polygon's plane +y becomes -z (ahead) after the rotation below,
    // so the tip at y = +0.55 points away from the camera.
    let chev = [[-0.75, 0.0], [0.0, 0.55], [0.75, 0.0], [0.75, -0.3], [0.0, 0.25], [-0.75, -0.3]];
    for (k, color) in [0x3fb6c8ffu32, 0x5cd6e6ff, 0x7af4ffff].into_iter().enumerate() {
        let z = 1.0 - k as f32 * 1.0;
        let c = MeshData::extrude(&chev, 0.03).transformed(&Transform::at(vec3(0.0, 0.05, z)).with_rot(Quat::from_rotation_x(-PI / 2.0)).matrix());
        m.merge(&c.with_vertex_colors(color));
    }
    m
}

fn palm() -> MeshData {
    let mut m = MeshData::new();
    let mut base = vec3(0.0, 0.0, 0.0);
    for k in 0..6 {
        let r0 = 0.2 - k as f32 * 0.012;
        let seg = MeshData::frustum(r0, r0 - 0.012, 0.92, 7);
        let lean = 0.06 + 0.03 * k as f32;
        let top = base + vec3(0.92 * lean.sin(), 0.92 * lean.cos(), 0.0);
        let mid = (base + top) * 0.5;
        let c = if k % 2 == 0 { look::PALM_TRUNK } else { 0x8e6446ff };
        m.merge(&part(seg, mid, Quat::from_rotation_z(-lean), c));
        base = top;
    }
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU + 0.3;
        let droop = 0.35 + 0.25 * ((k * 5) % 3) as f32 / 2.0;
        let leaf = MeshData::cone(0.3, 2.3, 5).transformed(&Transform::at(Vec3::ZERO).with_scale3(vec3(1.0, 1.0, 0.22)).matrix());
        let q = Quat::from_rotation_y(a) * Quat::from_rotation_z(-(PI / 2.0 - droop));
        let c = if k % 2 == 0 { look::PALM_LEAF } else { 0x3f8558ff };
        m.merge(&part(leaf, base + q * vec3(0.0, 1.1, 0.0), q, c));
    }
    for k in 0..3 {
        let a = k as f32 * 2.1;
        m.merge(&at(MeshData::icosphere(0.11, 0), base + vec3(0.14 * a.cos(), -0.12, 0.14 * a.sin()), 0x4a3424ff));
    }
    m.flat_shaded()
}

/// A street lamp on the right of the road, its arm reaching over it (−x).
fn lamp() -> MeshData {
    let mut m = MeshData::new();
    m.merge(&at(MeshData::cylinder(0.09, 6.4, 8), vec3(0.0, 3.2, 0.0), 0x5a5262ff));
    m.merge(&at(MeshData::cuboid(vec3(1.7, 0.1, 0.1)), vec3(-0.8, 6.35, 0.0), 0x5a5262ff));
    m.merge(&at(MeshData::cuboid(vec3(0.55, 0.14, 0.3)), vec3(-1.6, 6.32, 0.0), 0x3a3442ff));
    m.flat_shaded()
}

fn lamp_glow() -> MeshData {
    MeshData::uv_sphere(1.0, 10, 5).transformed(&Transform::at(vec3(-1.6, 6.22, 0.0)).with_scale3(vec3(0.24, 0.05, 0.14)).matrix())
}

fn rock() -> MeshData {
    let mut m = MeshData::icosphere(0.6, 0);
    for (i, p) in m.positions.iter_mut().enumerate() {
        let k = 0.75 + 0.45 * ((i as f32 * 12.9898).sin() * 43758.545).fract().abs();
        *p = vec3(p.x * k, (p.y * k).max(-0.1) * 0.7, p.z * k);
    }
    m.flat_shaded().with_vertex_colors_by(|p, _| if p.y > 0.15 { 0xb87a5eff } else { 0x8a5446ff })
}

/// The road: asphalt, lines, shoulders, guard rails and the sand, one mesh
/// that repeats every PERIOD meters (moved back by `dist mod PERIOD`).
fn road_mesh() -> MeshData {
    let len = 264.0;
    let z0 = 24.0; // reaches this far behind the camera
    let mut m = MeshData::new();
    let strip = |m: &mut MeshData, x: f32, w: f32, y: f32, color: u32| {
        m.merge(&at(MeshData::plane(w, len), vec3(x, y, z0 - len / 2.0), color));
    };
    strip(&mut m, 0.0, 8.4, 0.0, look::ASPHALT);
    for side in [-1.0f32, 1.0] {
        strip(&mut m, side * 3.62, 0.14, 0.01, look::LINES);
        strip(&mut m, side * 4.55, 1.3, -0.01, look::SHOULDER);
        // Sand out to the horizon, in two tones.
        strip(&mut m, side * 12.0, 13.8, -0.03, look::SAND);
        strip(&mut m, side * 70.0, 102.0, -0.05, look::SAND_DARK);
        // The guard rail: a beam on posts.
        m.merge(&at(MeshData::cuboid(vec3(0.08, 0.26, len)), vec3(side * 5.05, 0.6, z0 - len / 2.0), look::RAIL));
    }
    let mut z = z0;
    while z > z0 - len {
        // Lane dashes (3 m of every 8).
        for x in [-lane_x(0) / 2.0, lane_x(0) / 2.0] {
            m.merge(&at(MeshData::plane(0.14, 3.0), vec3(x, 0.01, z - 1.5), look::LINES));
        }
        // Sense of speed (DESIGN.md): edges passing per second sell speed
        // more than anything, so the near field is dense with them. Faint
        // bands across the asphalt (2 m of every 4)...
        for k in 0..2 {
            m.merge(&at(MeshData::plane(7.2, 2.0), vec3(0.0, 0.004, z - 1.0 - k as f32 * 4.0), look::ASPHALT_BAND));
        }
        for side in [-1.0f32, 1.0] {
            // ...red and white rumble strips, a block a meter...
            for k in 0..8 {
                let c = if k % 2 == 0 { look::KERB } else { look::LINES };
                m.merge(&at(MeshData::plane(0.5, 1.0), vec3(side * 3.97, 0.014, z - 0.5 - k as f32), c));
            }
            // ...and a rail post every 2 m with a bright reflector.
            for k in 0..4 {
                let pz = z - k as f32 * 2.0;
                m.merge(&at(MeshData::cuboid(vec3(0.12, 0.62, 0.12)), vec3(side * 5.12, 0.31, pz), 0x8a8494ff));
                m.merge(&part(MeshData::plane(0.14, 0.12), vec3(side * 5.12, 0.76, pz + 0.065), Quat::from_rotation_x(PI / 2.0), if k % 2 == 0 { 0xfff4e8ff } else { 0xffa64aff }));
            }
        }
        z -= PERIOD;
    }
    m
}

/// Mesas on the horizon (static: they're far enough not to need parallax).
fn mesas() -> MeshData {
    let mut m = MeshData::new();
    let mut r = Rng::new(77);
    for k in 0..18 {
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let x = side * r.range(18.0, 190.0);
        let z = -r.range(150.0, 260.0);
        let w = r.range(10.0, 26.0);
        let h = r.range(9.0, 26.0) * if x.abs() < 40.0 { 0.6 } else { 1.0 };
        let mut f = MeshData::frustum(w, w * r.range(0.55, 0.8), h, 7);
        for (i, p) in f.positions.iter_mut().enumerate() {
            let j = ((i as f32 * 7.31 + k as f32).sin() * 0.12) + 1.0;
            p.x *= j;
            p.z *= j;
        }
        let f = f.transformed(&Transform::at(vec3(x, h / 2.0 - 0.5, z)).with_rot(Quat::from_rotation_y(r.range(0.0, 3.0))).matrix());
        m.merge(&f.with_vertex_colors_by(|p, _| if p.y > h * 0.75 { 0xc06e58ff } else { look::MESA }));
    }
    m.flat_shaded()
}

/// A light gantry over the road: two posts and a beam (a glow strip goes under it).
fn gantry() -> MeshData {
    let mut m = MeshData::new();
    for x in [-5.5, 5.5] {
        m.merge(&at(MeshData::cuboid(vec3(0.3, 6.4, 0.3)), vec3(x, 3.2, 0.0), look::GANTRY));
    }
    m.merge(&at(MeshData::cuboid(vec3(11.6, 0.4, 0.5)), vec3(0.0, 6.3, 0.0), look::GANTRY));
    m.flat_shaded()
}

fn gantry_glow() -> MeshData {
    MeshData::cuboid(vec3(10.2, 0.08, 0.22)).transformed(&Transform::at(vec3(0.0, 6.06, 0.0)).matrix())
}

fn sign_mesh() -> MeshData {
    let mut m = MeshData::new();
    for x in [-5.7, 5.7] {
        m.merge(&at(MeshData::cylinder(0.14, 6.4, 8), vec3(x, 3.2, 0.0), 0x5a5262ff));
    }
    m.merge(&at(MeshData::cuboid(vec3(11.6, 0.3, 0.3)), vec3(0.0, 6.3, 0.0), 0x5a5262ff));
    m.merge(&at(MeshData::cuboid(vec3(3.8, 1.4, 0.12)), vec3(0.0, 5.75, 0.12), 0x1f5a5aff));
    m.merge(&at(MeshData::cuboid(vec3(3.6, 1.2, 0.04)), vec3(0.0, 5.75, 0.2), 0x2a7470ff));
    m.flat_shaded()
}

impl Scene {
    pub fn build(seed: u64) -> Scene {
        gfx3d::sky(&Sky { seed: 5, colors: look::SKY, nebula_density: 0.9, star_density: 0.12, star_brightness: 0.6, sun_disc: 0.07, ..Default::default() });
        // A low sun straight ahead: the scene is backlit, so rims and outlines carry the shapes.
        gfx3d::sun(&Sun { direction: vec3(-0.32, -0.6, 0.74).normalize(), color: [1.0, 0.72, 0.5], intensity: 3.6, shadows: 1, shadow_distance: 26.0 });
        gfx3d::post(&POST);
        gfx3d::fog(&Fog { color: look::FOG, mode: FogMode::Exponential, density: 0.011, height: 0.0, height_falloff: 0.05, sky: 1.0, ..Default::default() });

        let plain = MaterialDesc { roughness: 0.85, ..Default::default() };
        let road = Node::with_mesh(road_mesh().upload().unwrap(), mat(plain, toon(0.0, 0.0))).unwrap();
        Node::with_mesh(mesas().upload().unwrap(), mat(plain, toon(1.2, 0.3)));

        // The hero car: a root you move, the body under it, four wheels.
        let car = Node::new().unwrap();
        let body_mat = mat(
            MaterialDesc { roughness: 0.35, metallic: 0.1, ..Default::default() },
            MaterialStyle { flags: MaterialStyle::NO_FOG, ..toon(2.2, 0.55) },
        );
        let body = Node::with_mesh(hero_body().upload().unwrap(), body_mat).unwrap();
        body.set_parent(Some(car));
        body.set_transform(&Transform::at(vec3(0.0, -0.6, 0.0)));
        let lights = Material::new(&MaterialDesc { base_color: [2.4, 2.4, 2.4, 1.0], flags: Material::UNLIT, ..Default::default() }).unwrap();
        let lamp_node = Node::with_mesh(hero_lights().upload().unwrap(), lights).unwrap();
        lamp_node.set_parent(Some(body));
        let wheel_mesh = wheel(0.36, 0.3).upload().unwrap();
        let wheel_mat = mat(MaterialDesc { roughness: 0.7, ..Default::default() }, MaterialStyle { flags: MaterialStyle::NO_FOG, ..toon(1.6, 0.2) });
        let wheels = [(-0.79, -1.25), (0.79, -1.25), (-0.79, 1.25), (0.79, 1.25)].map(|(x, z)| {
            let n = Node::with_mesh(wheel_mesh, wheel_mat).unwrap();
            n.set_parent(Some(body));
            n.set_transform(&Transform::at(vec3(x, 0.36, z)));
            n
        });
        let underglow = Node::new().unwrap();
        underglow.set_parent(Some(car));
        underglow.set_transform(&Transform::at(vec3(0.0, -0.3, 0.6)));
        underglow.point_light(look::BOOST_GLOW, 0.0, 6.0);
        let flash = Node::new().unwrap();
        flash.point_light([1.0, 0.6, 0.3], 0.0, 12.0);

        // Roadside chunks: palms, lamps (and their glow), rocks.
        let palm_m = palm().upload().unwrap();
        let lamp_m = lamp().upload().unwrap();
        let glow_m = lamp_glow().upload().unwrap();
        let rock_m = rock().upload().unwrap();
        let palm_mat = mat(plain, toon(1.5, 0.35));
        let lamp_mat = mat(MaterialDesc { roughness: 0.5, metallic: 0.3, ..Default::default() }, toon(1.2, 0.3));
        let glow_mat = Material::new(&MaterialDesc { base_color: [3.2, 2.2, 1.1, 1.0], flags: Material::UNLIT, ..Default::default() }).unwrap();
        let rock_mat = mat(plain, toon(1.2, 0.2));
        let chunks = (0..CHUNKS)
            .map(|_| {
                let root = Node::new().unwrap();
                let child = |mesh, m| {
                    let n = Node::with_mesh(mesh, m).unwrap();
                    n.set_parent(Some(root));
                    n
                };
                Chunk { root, palms: child(palm_m, palm_mat), lamps: child(lamp_m, lamp_mat), glows: child(glow_m, glow_mat), rocks: child(rock_m, rock_mat), index: -1 }
            })
            .collect();

        let car_mat = mat(MaterialDesc { roughness: 0.4, metallic: 0.05, ..Default::default() }, toon(1.8, 0.45));
        let cars = Node::with_mesh(sedan().upload().unwrap(), car_mat).unwrap();
        let trucks = Node::with_mesh(truck().upload().unwrap(), car_mat).unwrap();
        let blink_mat = Material::new(&MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: look::BLINKER, emissive_strength: 2.0, flags: Material::UNLIT, ..Default::default() }).unwrap();
        let blinkers = Node::with_mesh(MeshData::cuboid(vec3(0.3, 0.2, 0.12)).upload().unwrap(), blink_mat).unwrap();
        let cones = Node::with_mesh(cone().upload().unwrap(), mat(plain, toon(1.4, 0.3))).unwrap();
        let barriers = Node::with_mesh(barrier().upload().unwrap(), mat(plain, toon(1.6, 0.3))).unwrap();
        let beacons = Node::with_mesh(MeshData::icosphere(0.11, 1).upload().unwrap(), blink_mat).unwrap();
        let oil = Node::with_mesh(
            oil_mesh().upload().unwrap(),
            Material::new(&MaterialDesc { base_color: srgba(0x0d0a12ff), metallic: 0.0, roughness: 0.06, ..Default::default() }).unwrap(),
        )
        .unwrap();
        let coin_mat = mat(
            MaterialDesc { base_color: srgba(look::COIN), metallic: 0.9, roughness: 0.3, emissive: srgb(0xffa21f), emissive_strength: 0.9, ..Default::default() },
            MaterialStyle { toon_bands: 2, rim: 0.6, outline_color: [0.14, 0.06, 0.0], outline_width: 1.3, ..Default::default() },
        );
        let coins = Node::with_mesh(coin().upload().unwrap(), coin_mat).unwrap();
        let pad_mat = Material::new(&MaterialDesc { base_color: [2.6, 2.6, 2.6, 1.0], flags: Material::UNLIT, ..Default::default() }).unwrap();
        let pads = Node::with_mesh(pad().upload().unwrap(), pad_mat).unwrap();

        let sign = Node::with_mesh(sign_mesh().upload().unwrap(), mat(MaterialDesc { roughness: 0.6, ..Default::default() }, toon(1.5, 0.3))).unwrap();
        let sign_label = Node::new().unwrap();
        sign_label.set_parent(Some(sign));
        sign_label.set_transform(&Transform::at(vec3(0.0, 5.75, 0.24)));
        sign.set_visible(false);
        let gantries = Node::with_mesh(gantry().upload().unwrap(), mat(MaterialDesc { roughness: 0.6, metallic: 0.2, ..Default::default() }, toon(1.4, 0.3))).unwrap();
        let gantry_glows = Node::with_mesh(
            gantry_glow().upload().unwrap(),
            Material::new(&MaterialDesc { base_color: [look::GANTRY_GLOW[0], look::GANTRY_GLOW[1], look::GANTRY_GLOW[2], 1.0], flags: Material::UNLIT, ..Default::default() }).unwrap(),
        )
        .unwrap();

        let pops = (0..POPS)
            .map(|_| {
                let node = Node::new().unwrap();
                node.set_visible(false);
                Pop { node, born: -10.0, at: Vec3::ZERO, text: String::new(), color: [1.0; 3] }
            })
            .collect();

        let emitter = |d: EmitterDesc| Emitter::new(&d).unwrap();
        let exhaust = emitter(EmitterDesc {
            colors: [[0.55, 0.45, 0.5, 0.0], [0.5, 0.42, 0.48, 0.32], [0.42, 0.34, 0.4, 0.14], [0.4, 0.3, 0.36, 0.0]],
            sizes: [0.12, 0.3, 0.5, 0.7],
            lifetime: [0.35, 0.6],
            speed: [0.4, 1.0],
            spread: 0.5,
            drag: 2.0,
            blend: FxBlend::Alpha,
            gravity: vec3(0.0, 0.5, 0.0),
            ..Default::default()
        });
        let flame = emitter(EmitterDesc {
            colors: [[1.2, 4.0, 5.0, 1.0], [0.8, 1.8, 5.0, 0.9], [2.2, 0.7, 1.6, 0.5], [0.0, 0.0, 0.0, 0.0]],
            // A jet: fattest at the nozzle, tapering as it trails back.
            sizes: [0.3, 0.26, 0.16, 0.04],
            lifetime: [0.1, 0.2],
            speed: [2.0, 7.0],
            spread: 0.18,
            drag: 1.0,
            gravity: Vec3::ZERO,
            stretch: 0.05,
            ..Default::default()
        });
        let tyre = emitter(EmitterDesc {
            colors: [[0.8, 0.74, 0.78, 0.0], [0.75, 0.7, 0.74, 0.45], [0.6, 0.55, 0.6, 0.2], [0.5, 0.45, 0.5, 0.0]],
            sizes: [0.2, 0.55, 0.9, 1.2],
            lifetime: [0.4, 0.8],
            speed: [0.3, 1.2],
            spread: 1.2,
            drag: 2.5,
            blend: FxBlend::Alpha,
            gravity: vec3(0.0, 0.6, 0.0),
            ..Default::default()
        });
        let wind = emitter(EmitterDesc {
            colors: [[0.0, 0.0, 0.0, 0.0], [1.4, 1.2, 1.05, 0.7], [1.2, 1.0, 0.9, 0.55], [0.0, 0.0, 0.0, 0.0]],
            sizes: [0.04, 0.05, 0.05, 0.03],
            lifetime: [0.5, 0.8],
            speed: [2.0, 8.0],
            spread: 0.01,
            drag: 0.0,
            gravity: Vec3::ZERO,
            stretch: 0.07,
            ..Default::default()
        });
        let sparks = emitter(EmitterDesc {
            colors: [[9.0, 6.0, 2.5, 1.0], [6.0, 2.2, 0.5, 1.0], [2.5, 0.5, 0.1, 0.8], [0.4, 0.05, 0.0, 0.0]],
            sizes: [0.08, 0.07, 0.05, 0.02],
            lifetime: [0.5, 1.2],
            speed: [3.0, 10.0],
            drag: 1.4,
            gravity: vec3(0.0, -5.0, 0.0),
            stretch: 0.05,
            ..Default::default()
        });
        let fire = emitter(EmitterDesc {
            colors: [[4.5, 2.6, 1.0, 1.0], [2.6, 0.8, 0.2, 0.75], [0.7, 0.14, 0.05, 0.35], [0.0; 4]],
            sizes: [0.5, 1.2, 1.6, 1.8],
            lifetime: [0.35, 0.7],
            speed: [0.4, 2.6],
            drag: 2.5,
            gravity: vec3(0.0, 0.8, 0.0),
            ..Default::default()
        });
        let smoke = emitter(EmitterDesc {
            colors: [[0.2, 0.12, 0.1, 0.0], [0.16, 0.1, 0.12, 0.55], [0.1, 0.08, 0.1, 0.3], [0.06, 0.05, 0.07, 0.0]],
            sizes: [0.6, 1.3, 2.0, 2.6],
            lifetime: [1.4, 2.4],
            speed: [0.3, 1.2],
            drag: 1.2,
            blend: FxBlend::Alpha,
            gravity: vec3(0.0, 0.7, 0.0),
            ..Default::default()
        });
        let glass = emitter(EmitterDesc {
            colors: [[3.0, 4.0, 5.0, 1.0], [1.2, 1.8, 2.6, 1.0], [0.5, 0.8, 1.2, 0.7], [0.1, 0.2, 0.3, 0.0]],
            sizes: [0.06, 0.05, 0.04, 0.02],
            lifetime: [0.4, 0.9],
            speed: [2.0, 7.0],
            drag: 1.0,
            gravity: vec3(0.0, -8.0, 0.0),
            ..Default::default()
        });
        let sparkle = emitter(EmitterDesc {
            colors: [[6.0, 4.5, 1.6, 1.0], [3.0, 1.8, 0.4, 1.0], [1.0, 0.5, 0.1, 0.6], [0.0; 4]],
            sizes: [0.08, 0.07, 0.05, 0.0],
            lifetime: [0.25, 0.5],
            speed: [1.5, 4.0],
            drag: 3.0,
            gravity: vec3(0.0, -2.0, 0.0),
            stretch: 0.03,
            ..Default::default()
        });

        Scene {
            road,
            car,
            wheels,
            lights,
            underglow,
            flash,
            flash_at: -10.0,
            chunks,
            cars,
            trucks,
            blinkers,
            cones,
            barriers,
            beacons,
            oil,
            coins,
            pads,
            sign,
            sign_label,
            sign_shown: 0,
            gantries,
            gantry_glows,
            list: Vec::new(),
            exhaust,
            flame,
            tyre,
            wind,
            sparks,
            fire,
            smoke,
            glass,
            sparkle,
            pops,
            next_pop: 0,
            puff: 0.0,
            gust: 0.0,
            rng: Rng::new(seed),
        }
    }

    /// Lays out a chunk's props for its slab of road, seeded by the slab:
    /// the same stretch of road looks the same every time.
    fn layout(c: &mut Chunk, index: i64) {
        c.index = index;
        let mut r = Rng::new(0x5eed_0000 ^ index as u64);
        let mut palms = Vec::new();
        let mut rocks = Vec::new();
        for side in [-1.0f32, 1.0] {
            for k in 0..3 {
                let x = side * (6.6 + r.range(0.0, 1.0).powf(1.5) * 9.0);
                let z = -(k as f32 + r.range(0.1, 0.9)) * CHUNK / 3.0;
                let s = r.range(0.85, 1.3);
                palms.push(Instance::at(vec3(x, 0.0, z)).with_rot(Quat::from_rotation_y(r.range(0.0, std::f32::consts::TAU))).with_scale3(vec3(s, s * r.range(0.9, 1.25), s)));
            }
            // Scrub and stones close to the road: near-field flow at the screen's bottom corners.
            for _ in 0..10 {
                let x = side * r.range(5.6, 9.5);
                let s = r.range(0.14, 0.34);
                rocks.push(Instance::at(vec3(x, 0.0, -r.range(0.0, CHUNK))).with_rot(Quat::from_rotation_y(r.range(0.0, std::f32::consts::TAU))).with_scale3(vec3(s, s * r.range(0.6, 1.4), s)));
            }
            for _ in 0..4 {
                let x = side * r.range(6.0, 26.0);
                let s = r.range(0.4, 1.6);
                rocks.push(Instance::at(vec3(x, 0.0, -r.range(0.0, CHUNK))).with_rot(Quat::from_rotation_y(r.range(0.0, std::f32::consts::TAU))).with_scale3(vec3(s, s * r.range(0.6, 1.2), s)));
            }
        }
        c.palms.set_instances(&palms);
        c.rocks.set_instances(&rocks);
        // One lamp a side, staggered.
        let lamps = [Instance::at(vec3(5.6, 0.0, -4.0)), Instance::at(vec3(-5.6, 0.0, -4.0 - CHUNK / 2.0)).with_rot(Quat::from_rotation_y(PI))];
        c.lamps.set_instances(&lamps);
        c.glows.set_instances(&lamps);
    }

    /// Moves everything to where the sim says, once per frame (in update).
    pub fn update(&mut self, sim: &Sim, title: f32, time: f32, dt: f32) {
        let dist = sim.dist;
        let z_of = |d: f32| -(d - dist);
        self.road.set_transform(&Transform::at(vec3(0.0, 0.0, dist.rem_euclid(PERIOD))));
        let first = ((dist - 20.0) / CHUNK).floor() as i64;
        for k in 0..CHUNKS as i64 {
            let index = first + k;
            let c = &mut self.chunks[index.rem_euclid(CHUNKS as i64) as usize];
            if c.index != index {
                Scene::layout(c, index);
            }
            c.root.set_transform(&Transform::at(vec3(0.0, 0.0, z_of(index as f32 * CHUNK))));
        }

        // The car.
        let pose = sim.pose();
        let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_z(pose.roll) * Quat::from_rotation_x(pose.pitch);
        self.car.set_transform(&Transform::at(vec3(pose.x, 0.6 + pose.lift, 0.0)).with_rot(rot));
        let spin = Quat::from_rotation_x(sim.wheel_angle());
        let steer = Quat::from_rotation_y((-sim.vx * 0.06).clamp(-0.4, 0.4));
        for (i, w) in self.wheels.iter().enumerate() {
            let x = if i % 2 == 0 { -0.79 } else { 0.79 };
            let z = if i < 2 { -1.25 } else { 1.25 };
            let q = if i < 2 { steer * spin } else { spin };
            w.set_transform(&Transform::at(vec3(x, 0.36, z)).with_rot(q));
        }
        let b = sim.boost_k;
        self.lights.set(&MaterialDesc { base_color: [2.4 + 2.5 * b, 2.4 + 1.0 * b, 2.4 + 1.0 * b, 1.0], flags: Material::UNLIT, ..Default::default() });
        self.underglow.point_light(look::BOOST_GLOW, 9.0 * b, 6.0);
        let age = time - self.flash_at;
        let fi = if age < 2.0 { 90.0 * (-age * 6.0).exp() } else { 0.0 };
        self.flash.point_light([1.0, 0.62, 0.32], if fi > 0.05 { fi } else { 0.0 }, 12.0);

        self.obstacles(sim, time);
        self.pickups(sim, time);
        self.place_sign(sim, title > 0.5);
        self.place_gantries(sim);
        self.effects(sim, dt);
        self.update_pops(time);
        let blur = 2.5 + 1.0 * title;
        // The near field stays crisp in play (its edges are what read as speed).
        gfx3d::depth_of_field(&DepthOfField { focus: sim.focus(title, time), range: 2.2, blur, near: 0.35 * title });
        // Post effects ramp with the speed feel: fringing and a darker frame
        // at the edges, more bloom on boost.
        let lines = sim.feel.lines() * (1.0 - title);
        let b = sim.boost_k * (1.0 - title);
        gfx3d::post(&Post {
            chromatic_aberration: 0.05 + 0.22 * lines + 0.15 * b,
            vignette: 0.32 + 0.1 * lines + 0.08 * b,
            bloom_strength: 0.5 + 0.25 * b,
            ..POST
        });
    }

    fn obstacles(&mut self, sim: &Sim, time: f32) {
        let dist = sim.dist;
        let blink_on = (time * 4.0).fract() < 0.55;
        let mut cars = Vec::new();
        let mut trucks = Vec::new();
        let mut blinks = Vec::new();
        let mut cones = Vec::new();
        let mut barriers = Vec::new();
        let mut beacons = Vec::new();
        let mut oil = Vec::new();
        for (i, o) in sim.obstacles.iter().enumerate() {
            let z = -(o.d - dist);
            if z < -sim::AHEAD - 10.0 || z > 30.0 {
                continue;
            }
            let (off, q) = match o.knock {
                Some(k) => (k.off, Quat::from_rotation_y(k.angle * 0.4) * Quat::from_rotation_z(k.angle)),
                None => {
                    // A truck moving over noses into its new lane.
                    let yaw = match o.change {
                        Some(to) => -(to - o.from_lane).signum() as f32 * 0.14 * (PI * o.shift()).sin(),
                        None => 0.0,
                    };
                    (Vec3::ZERO, Quat::from_rotation_y(yaw))
                }
            };
            let pos = vec3(o.x, 0.0, z) + off;
            match o.kind {
                Kind::Car => cars.push(Instance::at(pos).with_rot(q).with_color(look::TRAFFIC[o.tint as usize % 8])),
                Kind::Truck => {
                    trucks.push(Instance::at(pos).with_rot(q).with_color(look::TRUCKS[o.tint as usize % 8]));
                    if let Some(to) = o.change
                        && o.signalling()
                        && o.blink.is_some()
                        && blink_on
                    {
                        let side = (to - o.from_lane).signum() as f32;
                        for dz in [-4.0, 4.1] {
                            blinks.push(Instance::at(pos + q * vec3(side * 1.02, 0.75, dz)));
                        }
                    }
                }
                Kind::Cones => {
                    // A barrier at the near end, then a taper of cones down the lane.
                    let near = pos + vec3(0.0, 0.0, 6.8);
                    barriers.push(Instance::at(near).with_rot(q));
                    if (time * 2.5 + i as f32 * 0.37).fract() < 0.5 {
                        beacons.push(Instance::at(near + vec3(0.0, 1.32, 0.0)));
                    }
                    for k in 0..6 {
                        let t = k as f32 / 5.0;
                        let dx = 0.75 * (1.0 - t) * if k % 2 == 0 { 1.0 } else { -1.0 };
                        cones.push(Instance::at(pos + vec3(dx, 0.0, 5.2 - t * 11.8)).with_rot(q));
                    }
                }
                Kind::Oil => oil.push(Instance::at(pos).with_rot(Quat::from_rotation_y(o.d * 0.7))),
            }
        }
        show(self.cars, &cars);
        show(self.trucks, &trucks);
        show(self.blinkers, &blinks);
        show(self.cones, &cones);
        show(self.barriers, &barriers);
        show(self.beacons, &beacons);
        show(self.oil, &oil);
    }

    fn pickups(&mut self, sim: &Sim, time: f32) {
        let dist = sim.dist;
        self.list.clear();
        for c in sim.coin_list.iter().filter(|c| !c.taken) {
            let z = -(c.d - dist);
            if z < -sim::AHEAD || z > 2.5 {
                continue;
            }
            let spin = time * 4.0 + c.d * 0.4;
            self.list.push(Instance::at(vec3(lane_x(c.lane), 0.75 + 0.08 * (time * 3.0 + c.d).sin(), z)).with_rot(Quat::from_rotation_y(spin)));
        }
        show(self.coins, &self.list);
        self.list.clear();
        let pulse = 0.75 + 0.25 * (time * 9.0).sin();
        let shade = (pulse * 255.0) as u32;
        for p in sim.pads.iter() {
            let z = -(p.d - dist);
            if z < -sim::AHEAD || z > 8.0 {
                continue;
            }
            let c = if p.used { 0x606060ff } else { (shade << 24) | (shade << 16) | (shade << 8) | 0xff };
            self.list.push(Instance::at(vec3(lane_x(p.lane), 0.0, z)).with_color(c));
        }
        show(self.pads, &self.list);
    }

    fn place_sign(&mut self, sim: &Sim, title: bool) {
        let next = ((sim.dist / SIGN_EVERY as f32).floor() as u32 + 1) * SIGN_EVERY;
        let z = -(next as f32 - sim.dist);
        let show = !title && z > -sim::AHEAD - 20.0;
        self.sign.set_visible(show);
        if !show {
            return;
        }
        self.sign.set_transform(&Transform::at(vec3(0.0, 0.0, z)));
        if self.sign_shown != next {
            self.sign_shown = next;
            let label = if next.is_multiple_of(1000) { format!("{} KM", next / 1000) } else { format!("{next} M") };
            self.sign_label.set_text(&TextDesc::new(Font::SansBold, 0.62).with_color([1.6, 1.5, 1.3, 1.0]).with_outline([0.02, 0.06, 0.06, 1.0], 0.06), &label);
        }
    }

    /// Light gantries in their zones, from just behind the camera into the fog.
    fn place_gantries(&mut self, sim: &Sim) {
        self.list.clear();
        let first = ((sim.dist - 12.0) / GANTRY_EVERY).ceil() as i64;
        let last = ((sim.dist + sim::AHEAD) / GANTRY_EVERY).floor() as i64;
        for n in first..=last {
            let d = n as f32 * GANTRY_EVERY;
            if d.rem_euclid(GANTRY_ZONE) < GANTRY_RUN && d > 120.0 {
                self.list.push(Instance::at(vec3(0.0, 0.0, -(d - sim.dist))));
            }
        }
        show(self.gantries, &self.list);
        show(self.gantry_glows, &self.list);
    }

    /// Continuous effects: exhaust, boost flames, wind streaks, the wreck smoking.
    fn effects(&mut self, sim: &Sim, dt: f32) {
        let pose = sim.pose();
        let p = sim.speed_now();
        let road = vec3(0.0, 0.0, p);
        match sim.phase {
            Phase::Running => {
                self.puff -= dt;
                while self.puff <= 0.0 {
                    self.puff += 1.0 / 30.0;
                    for x in [-0.45, 0.45] {
                        let at = vec3(pose.x + x, 0.32, 2.15);
                        if sim.boost_k > 0.3 {
                            // Carried along with the car (only part of the road's
                            // speed), so the flame is one tongue trailing back from
                            // the pipes, not puffs flung at the camera.
                            self.flame.emit_moving(at, vec3(0.0, 0.05, 1.0), 3, road * FLAME_DRIFT);
                        } else {
                            self.exhaust.emit_moving(at, vec3(0.0, 0.25, 1.0), 1, road);
                        }
                    }
                }
                // Wind streaks streaming past with the road, more the faster it feels.
                let rate = 70.0 * (sim.feel.k - 0.2).max(0.0) + 40.0 * sim.boost_k;
                self.gust += rate * dt;
                while self.gust >= 1.0 {
                    self.gust -= 1.0;
                    let side = if self.rng.f32() < 0.5 { -1.0 } else { 1.0 };
                    let at = vec3(side * self.rng.range(1.4, 6.5), self.rng.range(0.15, 3.6), -self.rng.range(10.0, 40.0));
                    self.wind.emit_moving(at, Vec3::Z, 1, road * 1.15);
                }
            }
            Phase::Crashing { .. } | Phase::Over { .. } => {
                self.puff -= dt;
                while self.puff <= 0.0 {
                    self.puff += 0.09;
                    let at = vec3(pose.x, 0.6 + pose.lift, 0.3);
                    self.smoke.emit_moving(at, Vec3::Y, 1, road);
                    if self.rng.f32() < 0.4 {
                        self.fire.emit_moving(at, Vec3::Y, 1, road);
                    }
                }
            }
        }
    }

    /// One-off effects for this frame's cues.
    pub fn cue(&mut self, c: &Cue, sim: &Sim, time: f32) {
        let pose = sim.pose();
        let road = vec3(0.0, 0.0, sim.speed_now());
        match *c {
            // Tyre smoke: a wisp per lane change, a cloud when it squeals.
            Cue::Lane { quick, .. } => {
                for x in [-0.79, 0.79] {
                    self.tyre.emit_moving(vec3(pose.x + x, 0.15, 1.25), Vec3::Y, if quick { 4 } else { 1 }, road);
                }
            }
            Cue::Oil { .. } => {
                for x in [-0.79, 0.79] {
                    self.tyre.emit_moving(vec3(pose.x + x, 0.15, 1.25), Vec3::Y, 6, road);
                }
            }
            Cue::Coin { at, .. } => self.sparkle.emit_moving(at, Vec3::ZERO, 14, road * 0.3),
            Cue::NearMiss { chain, points, at } => {
                let text = if chain > 1 { format!("{} ×{chain}  +{points}", look::NEAR_MISS) } else { format!("{}  +{points}", look::NEAR_MISS) };
                self.pop(at, text, [2.4, 2.0, 0.9], time);
                self.sparkle.emit_moving(at, Vec3::ZERO, 10, road);
            }
            Cue::Boost { at } => {
                gfx3d::shockwave(at, 3.0, 0.012, 0.5);
                self.sparkle.emit(at, Vec3::Y, 20);
            }
            Cue::Smash { at } => {
                self.sparks.emit_moving(at, Vec3::ZERO, 70, road * 0.4);
                self.glass.emit_moving(at, Vec3::Y, 30, road * 0.4);
                gfx3d::shockwave(at, 3.0, 0.015, 0.5);
                self.flash.set_transform(&Transform::at(at));
                self.flash_at = time;
                self.pop(at + vec3(0.0, 0.6, 0.0), format!("SMASH  +{}", sim::SMASH_POINTS), [3.0, 1.6, 0.6], time);
            }
            Cue::Crash { at } => {
                let v = road * 0.25;
                self.sparks.emit_moving(at, Vec3::ZERO, 170, v);
                self.glass.emit_moving(at, Vec3::Y, 60, v);
                self.fire.emit_moving(at, Vec3::ZERO, 30, v);
                self.smoke.emit_moving(at, Vec3::ZERO, 18, v);
                gfx3d::shockwave(at, 5.0, 0.022, 0.9);
                self.flash.set_transform(&Transform::at(at + vec3(0.0, 0.8, 0.0)));
                self.flash_at = time;
            }
            _ => {}
        }
    }

    fn pop(&mut self, at: Vec3, text: String, color: [f32; 3], time: f32) {
        let p = &mut self.pops[self.next_pop];
        self.next_pop = (self.next_pop + 1) % POPS;
        p.born = time;
        p.at = at;
        p.text = text;
        p.color = color;
    }

    fn update_pops(&mut self, time: f32) {
        for p in &self.pops {
            let age = time - p.born;
            if age > 1.0 || age < 0.0 {
                p.node.set_visible(false);
                continue;
            }
            let fade = 1.0 - (age / 1.0).powi(3);
            let rise = 1.0 - (1.0 - age.min(0.5) / 0.5).powi(2);
            p.node.set_visible(true);
            p.node.set_transform(&Transform::at(p.at + vec3(0.0, 0.4 + rise * 1.2, 0.0)));
            let size = 22.0 + 8.0 * (1.0 - (age * 6.0).min(1.0));
            let desc = TextDesc { flags: TextDesc::BILLBOARD | TextDesc::SCREEN | TextDesc::NO_FOG, ..TextDesc::new(Font::SansBold, size) }
                .with_color([p.color[0], p.color[1], p.color[2], fade])
                .with_outline([0.06, 0.02, 0.06, fade], 0.09);
            p.node.set_text(&desc, &p.text);
        }
    }
}
