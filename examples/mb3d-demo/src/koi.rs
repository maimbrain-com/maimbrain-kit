//! The koi pond: a vignette for organic meshes built in code and skinned
//! animation (mb3d 2).
//!
//! Five koi swim loops in a pond at dusk. Each is `MeshData::fish`: smooth
//! lofted bodies with a koi pattern in vertex colors, long translucent fins
//! that glow a little, all skinned to a chain of eight bones that `Swim`
//! turns in a traveling wave, beating faster when the fish speeds up and
//! bending into its turns. The same bones mechanism waves the carp
//! streamers (koinobori) on the pole and sways the tassel under the stone
//! lantern. Rocks and the lantern are coarse cages smoothed by subdivision.
//! The koi swim under clear water (water v2's `Water::pond()`): bent by its
//! ripples, fading into green with depth, caustics on them and the bed, the
//! lantern mirrored in it and a thin line of foam at the shore and stones.
//! Tap to compare: swimming, a close-up following the big butterfly koi,
//! the same fish stiff (bones at rest), and the faceted primitive koi this
//! replaced.

use std::f32::consts::{PI, TAU};

use maimbrain::gfx3d::{
    self, Camera, FishDesc, Fog, FogMode, Instance, Loft, Material, MaterialDesc, MaterialStyle, MeshData, Node, Post, Quat, Reflector, Skeleton, Sky, Sun, Swim,
    Transform, Vec3, Water, Wave, srgba, vec3,
};

/// Pond radius (water), and the depth of its floor.
const POND: f32 = 3.4;
const FLOOR: f32 = -0.75;
/// The water's surface: a little below the rim, so the bowl's shore rises out of it.
const SURFACE: f32 = -0.03;
/// Koi length (m).
const KOI: f32 = 0.95;
const BONES: u32 = 8;
const LANTERN: Vec3 = vec3(-1.55, 0.0, -4.3);
const POLE: Vec3 = vec3(1.45, 0.0, -4.7);
const WARM: [f32; 3] = [1.0, 0.55, 0.22];

/// What the pond shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Swimming,
    CloseUp,
    Stiff,
    Faceted,
}

impl Look {
    pub fn next(self) -> Look {
        match self {
            Look::Swimming => Look::CloseUp,
            Look::CloseUp => Look::Stiff,
            Look::Stiff => Look::Faceted,
            Look::Faceted => Look::Swimming,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Look::Swimming => "SMOOTH LOFTS · SKINNED · SWIMMING",
            Look::CloseUp => "8 BONES, A WAVE RUNNING DOWN THEM",
            Look::Stiff => "SAME MESH, BONES AT REST",
            Look::Faceted => "BEFORE: SPHERE + CONE, FLAT SHADED",
        }
    }
}

/// Koi varieties: the pattern on a white, gold or black body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variety {
    /// White with red patches.
    Kohaku,
    /// Kohaku with small black spots.
    Sanke,
    /// White with one red spot on the head.
    Tancho,
    /// Gold all over.
    Yamabuki,
    /// Black with red and white.
    Showa,
}

/// A koi's loop: center, radii, angular speed, phase, depth, and fin size.
struct Path {
    center: Vec3,
    radius: [f32; 2],
    speed: f32,
    phase: f32,
    variety: Variety,
    fins: f32,
    scale: f32,
}

const PATHS: [Path; 5] = [
    Path { center: vec3(0.1, -0.26, 0.2), radius: [1.15, 1.9], speed: 0.34, phase: 0.6, variety: Variety::Kohaku, fins: 2.4, scale: 1.1 },
    Path { center: vec3(-0.2, -0.36, -0.6), radius: [1.3, 2.0], speed: -0.27, phase: 2.0, variety: Variety::Sanke, fins: 1.0, scale: 1.0 },
    Path { center: vec3(0.3, -0.44, -0.9), radius: [1.0, 1.6], speed: 0.24, phase: 4.1, variety: Variety::Yamabuki, fins: 1.6, scale: 0.9 },
    Path { center: vec3(-0.1, -0.5, 0.1), radius: [0.9, 1.3], speed: -0.38, phase: 1.0, variety: Variety::Tancho, fins: 1.0, scale: 0.85 },
    Path { center: vec3(0.0, -0.4, -1.1), radius: [1.25, 1.2], speed: 0.3, phase: 5.2, variety: Variety::Showa, fins: 1.3, scale: 0.95 },
];

impl Path {
    /// Position at time t: a loop with a figure-eight wobble, gently rising and sinking.
    fn at(&self, t: f32) -> Vec3 {
        let a = self.phase + self.speed * t;
        self.center + vec3(self.radius[0] * a.sin(), 0.06 * (a * 3.0).sin(), self.radius[1] * a.cos() + 0.35 * (2.0 * a).sin())
    }
}

/// One swimming koi: the node that moves (its body; the fins are its child), its bones and its swim.
struct Koi {
    body: Node,
    bones: Skeleton,
    swim: Swim,
    /// The faceted stand-in shown in `Look::Faceted`.
    old: Node,
}

/// A carp streamer on the pole, or the lantern's tassel: a skinned mesh and its bones.
struct Waver {
    bones: Skeleton,
    wave: Wave,
    phase: f32,
    rate: f32,
}

pub struct Pond {
    root: Node,
    koi: Vec<Koi>,
    streamers: Vec<Waver>,
    tassel: Waver,
    light: Node,
    pub look: Look,
    pub shown: bool,
    /// The last update's time (the swims advance by the difference).
    last: f32,
}

fn material(desc: MaterialDesc) -> Material {
    Material::new(&desc).expect("material")
}

/// Integer hash → 0..1.
fn hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (z as u32).wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth value noise, 0..1.
fn noise(p: Vec3, seed: u32) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (s(p.x - fx), s(p.y - fy), s(p.z - fz));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx, dy, dz| hash(ix + dx, iy + dy, iz + dz, seed);
    l(l(l(c(0, 0, 0), c(1, 0, 0), u), l(c(0, 1, 0), c(1, 1, 0), u), v), l(l(c(0, 0, 1), c(1, 0, 1), u), l(c(0, 1, 1), c(1, 1, 1), u), v), w)
}

/// Blends two 0xRRGGBBAA colors.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let mut out = 0;
    for s in [24, 16, 8, 0] {
        let (x, y) = (((a >> s) & 255) as f32, ((b >> s) & 255) as f32);
        out |= ((x + (y - x) * t).round() as u32) << s;
    }
    out
}

const WHITE: u32 = 0xf6f2eaff;
const RED: u32 = 0xe0401cff;
const BLACK: u32 = 0x18181cff;
const GOLD: u32 = 0xf0b43cff;

/// The koi's skin color at a point of the unit-length body (`n` its normal).
fn pattern(v: Variety, p: Vec3, n: Vec3, seed: u32) -> u32 {
    // Patches sit on the back and sides, never the belly; soft edges.
    let back = ((n.y + 0.35) / 0.5).clamp(0.0, 1.0);
    let patch = |freq: f32, cut: f32, seed: u32| {
        let q = vec3(p.x * 1.6, p.y, p.z) * freq;
        ((noise(q, seed) * 0.75 + noise(q * 2.3, seed + 7) * 0.25 - cut) / 0.06).clamp(0.0, 1.0)
    };
    let head = ((-p.z - 0.32) / 0.05).clamp(0.0, 1.0);
    match v {
        Variety::Kohaku => mix(WHITE, RED, patch(6.0, 0.5, seed) * back * (1.0 - 0.6 * head)),
        Variety::Sanke => {
            let c = mix(WHITE, RED, patch(6.0, 0.52, seed) * back * (1.0 - 0.5 * head));
            mix(c, BLACK, patch(13.0, 0.66, seed + 3) * back * (1.0 - head))
        }
        Variety::Tancho => {
            let d = vec3(p.x, (p.y - 0.06) * 1.3, p.z + 0.37).length();
            mix(WHITE, RED, ((0.07 - d) / 0.012).clamp(0.0, 1.0) * back)
        }
        Variety::Yamabuki => mix(GOLD, 0xf8e2a0ff, ((-n.y - 0.2) / 0.6).clamp(0.0, 1.0) + 0.25 * noise(p * 20.0, seed)),
        Variety::Showa => {
            let c = mix(WHITE, RED, patch(5.0, 0.45, seed) * back);
            mix(c, BLACK, (patch(4.5, 0.48, seed + 11) * (0.4 + 0.6 * back)).max(((-n.y - 0.3) / 0.4).clamp(0.0, 0.7)))
        }
    }
}

/// The faceted koi the canal used to have: a squashed sphere and a cone.
fn old_koi() -> MeshData {
    let mut fish = MeshData::uv_sphere(0.16, 16, 10).transformed(&Transform::at(Vec3::ZERO).with_scale3(vec3(0.75, 0.5, 2.0)).matrix());
    fish.merge(&MeshData::cone(0.13, 0.32, 8).transformed(&Transform::at(vec3(0.0, 0.0, 0.42)).with_rot(Quat::from_rotation_x(-PI / 2.0)).with_scale3(vec3(1.0, 1.0, 0.3)).matrix()));
    fish.flat_shaded().transformed(&Transform::at(Vec3::ZERO).with_scale(KOI / 0.9).matrix())
}

/// A carp streamer: an open cloth tube from the mouth (at the origin) back
/// along +x, with a forked tail and scales painted in vertex colors.
fn streamer(len: f32, body: u32, belly: u32) -> MeshData {
    let r = len * 0.12;
    MeshData::loft(&Loft { spine: vec![Vec3::ZERO, vec3(len, 0.0, 0.0)], up: Vec3::Y, rings: 48, sides: 32, caps: false, crease: PI, ..Default::default() }, |t, u| {
        // Wide at the mouth, a fish's bulge, then pinched before the tail fins flare.
        let bulge = 1.0 + 0.25 * (PI * (t / 0.7).min(1.0)).sin();
        let pinch = if t < 0.75 { 1.0 } else { 1.0 - 0.45 * ((t - 0.75) / 0.25).min(1.0).powf(0.7) };
        let flare = if t > 0.85 { 1.0 + 1.2 * ((t - 0.85) / 0.15).powf(1.5) } else { 1.0 };
        let a = u * TAU;
        let rr = r * bulge * pinch;
        // The tail flattens vertically into two lobes, and the end is forked.
        let fork = if t > 0.86 { -len * 0.06 * (1.0 - (a.sin()).abs()) } else { 0.0 };
        vec3(a.cos() * rr * (1.0 / flare.sqrt()), a.sin() * rr * flare.min(1.9), fork)
    })
    .with_vertex_colors(body)
    .with_vertex_colors_by(move |p, n| {
        let t = p.x / len;
        // Scales: offset rows of round scales, each with a light rim, from
        // behind the head to the tail; a pale belly; an eye near the mouth.
        let size = r * 1.25;
        let (x, y) = ((p.x - len * 0.2) / size, n.z.atan2(n.y) * r / size);
        let row = y.floor();
        let xs = x + if row as i32 % 2 == 0 { 0.0 } else { 0.5 };
        let d = vec3(xs.fract() - 0.15, y.fract() - 0.5, 0.0).length();
        let rim = ((d - 0.38) / 0.12).clamp(0.0, 1.0) * (1.0 - ((d - 0.62) / 0.1).clamp(0.0, 1.0));
        let scaled = (0.2..0.85).contains(&t);
        let mut c = mix(body, belly, ((-n.y - 0.35) / 0.3).clamp(0.0, 1.0));
        if scaled && n.y > -0.4 {
            c = mix(c, 0xf4f0e8ff, rim * 0.7);
        }
        // A white ring at the mouth and the tail's edge.
        if t < 0.04 || t > 0.97 {
            c = 0xf4f0e8ff;
        }
        let eye = vec3(p.x - len * 0.1, p.y - r * 0.25, p.z.abs() - r * 0.95).length();
        if t < 0.2 && eye < r * 0.32 {
            c = if eye < r * 0.16 { BLACK } else { 0xffffffff };
        }
        c
    })
}

impl Pond {
    /// Builds the pond (hidden until `enter`).
    pub fn build() -> Pond {
        let root = Node::new().unwrap();
        root.set_visible(false);
        let child = |mesh: MeshData, mat: Material, t: Transform| {
            let n = Node::with_mesh(mesh.upload().unwrap(), mat).unwrap();
            n.set_parent(Some(root));
            n.set_transform(&t);
            n
        };

        // The pond: a smooth bowl, darker in the middle, inside a ring of moss.
        let bowl = MeshData::lathe(&[[POND + 0.6, 0.0], [POND, -0.08], [POND * 0.85, FLOOR * 0.7], [POND * 0.55, FLOOR], [0.0, FLOOR]], 72)
            .smooth_normals(PI)
            .with_vertex_colors_by(|p, _| mix(0x15504eff, 0x6a8a5aff, ((p.x * p.x + p.z * p.z).sqrt() / POND).powi(3) + 0.15 * (noise(p * 1.5, 4) - 0.5)));
        let floor_mat = material(MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.6, ..Default::default() });
        child(bowl, floor_mat, Transform::default());
        let moss = MeshData::lathe(&[[30.0, 0.0], [POND + 0.55, 0.0]], 72).with_vertex_colors_by(|p, _| mix(0x4a7a3cff, 0x2e5230ff, (noise(p * 0.7, 3) - 0.3) * 2.0));
        let moss_mat = material(MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.9, ..Default::default() });
        child(moss, moss_mat, Transform::default());

        // The water: a reflective disc reaching into the bowl's shore (water v2 makes it see-through).
        let water = material(MaterialDesc { base_color: [0.004, 0.008, 0.008, 1.0], roughness: 0.05, flags: Material::REFLECTIVE, ..Default::default() });
        child(MeshData::lathe(&[[POND + 0.45, 0.0], [0.0, 0.0]], 72), water, Transform::at(vec3(0.0, SURFACE, 0.0)));

        // Rocks around the rim: jittered icosahedra, subdivided smooth. One instanced draw.
        let mut cage = MeshData::icosphere(1.0, 0);
        for (i, p) in cage.positions.iter_mut().enumerate() {
            *p = *p * (0.75 + 0.5 * hash(i as i32, 0, 0, 9)) * vec3(1.0, 0.6, 1.0);
        }
        let rock = cage.subdivide(2, PI).with_vertex_colors_by(|p, n| mix(0x6a6a66ff, 0xa8a49aff, 0.5 + 0.5 * n.y + 0.2 * (noise(p * 3.0, 5) - 0.5)));
        let stone = material(MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], roughness: 0.8, ..Default::default() });
        let rocks = child(rock, stone, Transform::default());
        let ring: Vec<Instance> = (0..28)
            .map(|k| {
                let a = k as f32 / 28.0 * TAU + 0.3 * hash(k, 1, 0, 2);
                let r = POND + 0.2 + 0.2 * hash(k, 2, 0, 2);
                let s = 0.28 + 0.22 * hash(k, 3, 0, 2);
                Instance::at(vec3(a.sin() * r, 0.02, a.cos() * r)).with_rot(Quat::from_rotation_y(hash(k, 4, 0, 2) * TAU)).with_scale(s).with_color(mix(0xffffffff, 0xb8c0b0ff, hash(k, 5, 0, 2)))
            })
            .collect();
        rocks.set_instances(&ring);

        // Lily pads on the surface (the fish pass under them): a notched disc, instanced.
        let pad: Vec<[f32; 2]> = (0..=28).map(|k| {
            let a = 0.35 + k as f32 / 28.0 * (TAU - 0.7);
            [a.sin() * 0.5, a.cos() * 0.5]
        }).chain([[0.0, 0.0]]).collect();
        let pad = MeshData::extrude(&pad, 0.02).transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_x(-PI / 2.0)).matrix()).with_vertex_colors_by(|p, n| {
            if n.y > 0.5 { mix(0x3f7a34ff, 0x6fa048ff, (p.x * p.x + p.z * p.z).sqrt() * 2.0) } else { 0x2a4a24ff }
        });
        let leaf = material(MaterialDesc { base_color: [0.9, 1.0, 0.9, 1.0], roughness: 0.35, ..Default::default() });
        let pads = child(pad, leaf, Transform::default());
        let spots = [(-1.7, 1.5, 0.7), (-1.3, 2.1, 0.45), (1.8, 0.9, 0.6), (1.5, -2.2, 0.8), (-2.0, -1.3, 0.55), (0.6, 2.6, 0.4), (-0.5, -2.8, 0.6)];
        let list: Vec<Instance> = spots.iter().enumerate().map(|(k, &(x, z, s))| Instance::at(vec3(x, SURFACE + 0.012, z)).with_rot(Quat::from_rotation_y(k as f32 * 2.1)).with_scale(s)).collect();
        pads.set_instances(&list);

        // The stone lantern: boxy cages with creases kept, smoothed into soft stone.
        let mut lantern = MeshData::new();
        let part = |size: Vec3, y: f32| MeshData::cuboid(size).transformed(&Transform::at(vec3(0.0, y, 0.0)).matrix());
        lantern.merge(&part(vec3(0.9, 0.25, 0.9), 0.125));
        lantern.merge(&part(vec3(0.32, 0.9, 0.32), 0.7));
        lantern.merge(&part(vec3(0.8, 0.18, 0.8), 1.2));
        lantern.merge(&part(vec3(0.6, 0.12, 0.6), 1.88));
        let roof = MeshData::frustum(0.75, 0.12, 0.42, 4).transformed(&Transform::at(vec3(0.0, 2.15, 0.0)).with_rot(Quat::from_rotation_y(PI / 4.0)).matrix());
        lantern.merge(&roof);
        lantern.merge(&part(vec3(0.16, 0.2, 0.16), 2.45));
        let lantern = lantern.subdivide(2, 0.9).with_vertex_colors_by(|p, n| mix(0x8a877eff, 0x5e5c56ff, 0.4 - 0.4 * n.y + 0.4 * noise(p * 4.0, 8)));
        let lantern_node = child(lantern, stone, Transform::at(LANTERN));
        // Its glowing window box between the shelves.
        let paper = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: WARM, emissive_strength: 3.0, flags: Material::UNLIT, ..Default::default() });
        let glow = Node::with_mesh(MeshData::cuboid(vec3(0.46, 0.5, 0.46)).subdivide(1, 0.9).upload().unwrap(), paper).unwrap();
        glow.set_parent(Some(lantern_node));
        glow.set_transform(&Transform::at(vec3(0.0, 1.55, 0.0)));
        let light = Node::new().unwrap();
        light.set_parent(Some(root));
        light.set_transform(&Transform::at(LANTERN + vec3(0.0, 1.55, 0.6)));

        // The tassel hanging under the roof's corner: a cord flaring into a fringe, skinned to a chain.
        let tassel_len = 0.75;
        let pts: Vec<Vec3> = (0..6).map(|i| vec3(0.0, -tassel_len * i as f32 / 5.0, 0.0)).collect();
        let tassel_mesh = MeshData::loft(&Loft { spine: vec![Vec3::ZERO, vec3(0.0, -tassel_len, 0.0)], up: Vec3::Z, rings: 24, sides: 10, ends: 0.3, ..Default::default() }, |t, u| {
            let r = if t < 0.55 { 0.018 } else if t < 0.62 { 0.045 } else { 0.03 + 0.07 * ((t - 0.62) / 0.38).powf(0.8) };
            vec3((u * TAU).cos() * r, (u * TAU).sin() * r, 0.0)
        })
        .with_vertex_colors_by(|p, _| if p.y > -0.55 * 0.75 { 0xc0302aff } else { 0xe8a838ff })
        .with_chain_weights(&pts);
        let cord = material(MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.6, emissive: [0.3, 0.06, 0.02], emissive_strength: 1.0, ..Default::default() });
        let tassel_node = Node::with_mesh(tassel_mesh.upload().unwrap(), cord).unwrap();
        tassel_node.set_parent(Some(lantern_node));
        tassel_node.set_transform(&Transform::at(vec3(0.5, 2.0, 0.5)));
        let tassel_bones = Skeleton::chain(tassel_node, &pts).unwrap();
        tassel_bones.bind(tassel_node);
        let tassel = Waver { bones: tassel_bones, wave: Wave { axis: Vec3::Z, amplitude: 0.16, head: 0.5, waves: 0.35 }, phase: 0.0, rate: 1.9 };

        // The koinobori pole and its three streamers.
        let wood = material(MaterialDesc { base_color: srgba(0x5a4030ff), roughness: 0.7, ..Default::default() });
        child(MeshData::cylinder(0.045, 2.2, 10), wood, Transform::at(POLE + vec3(0.0, 1.1, 0.0)));
        let gold = material(MaterialDesc { base_color: [1.0, 0.7, 0.25, 1.0], metallic: 1.0, roughness: 0.3, ..Default::default() });
        child(MeshData::icosphere(0.1, 2), gold, Transform::at(POLE + vec3(0.0, 2.22, 0.0)));
        let cloth = material(MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.65, flags: Material::DOUBLE_SIDED, ..Default::default() });
        let mut streamers = Vec::new();
        for (k, (len, body, belly)) in [(1.3, 0x1e2230ffu32, 0xd8d0c0ffu32), (1.1, 0xd23a2aff, 0xf0d8c8ff), (0.9, 0x2a62c8ff, 0xe0e8f0ff)].into_iter().enumerate() {
            let pts: Vec<Vec3> = (0..8).map(|i| vec3(len * i as f32 / 7.0, 0.0, 0.0)).collect();
            let mesh = streamer(len, body, belly).with_chain_weights(&pts);
            let node = Node::with_mesh(mesh.upload().unwrap(), cloth).unwrap();
            node.set_parent(Some(root));
            // Mouths on the pole, the wind blowing them toward the camera's right.
            node.set_transform(&Transform::at(POLE + vec3(-0.05, 1.98 - k as f32 * 0.44, 0.0)).with_rot(Quat::from_rotation_y(PI + 0.35)));
            let bones = Skeleton::chain(node, &pts).unwrap();
            bones.bind(node);
            streamers.push(Waver { bones, wave: Wave { axis: Vec3::Y, amplitude: 0.3, head: 0.35, waves: 1.2 }, phase: k as f32 * 1.7, rate: 3.6 + 0.5 * k as f32 });
        }

        // The koi.
        let fin_mat = material(MaterialDesc {
            base_color: [1.0, 0.93, 0.88, 0.7],
            emissive: [1.0, 0.42, 0.2],
            emissive_strength: 0.4,
            roughness: 0.4,
            flags: Material::ALPHA | Material::DOUBLE_SIDED,
            ..Default::default()
        });
        let skin = material(MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.28, ..Default::default() });
        skin.set_style(&MaterialStyle { rim: 0.35, ..Default::default() });
        let old_mat = material(MaterialDesc { base_color: srgba(0xf08a4aff), roughness: 0.4, ..Default::default() });
        let old_mesh = old_koi().upload().unwrap();
        let mut koi = Vec::new();
        for (k, path) in PATHS.iter().enumerate() {
            let desc = FishDesc { fins: path.fins, bones: BONES, tail: if path.fins > 2.0 { 0.3 } else { 0.24 }, ..Default::default() };
            let fish = MeshData::fish(&desc);
            let seed = 17 + k as u32 * 31;
            let mut body = fish.body.with_vertex_colors_by(|p, n| pattern(path.variety, p, n, seed));
            body.merge(&fish.eyes.with_vertex_colors(0x0a0a0cff));
            let node = Node::with_mesh(body.upload().unwrap(), skin).unwrap();
            node.set_parent(Some(root));
            node.set_transform(&Transform::default().with_scale(KOI * path.scale));
            if k == 0 {
                // `mb shot --readability` checks the first koi stands out from the water.
                node.mark_subject();
            }
            let fins = Node::with_mesh(fish.fins.upload().unwrap(), fin_mat).unwrap();
            fins.set_parent(Some(node));
            let bones = Skeleton::chain(node, &desc.spine()).unwrap();
            bones.bind(node);
            bones.bind(fins);
            let old = Node::with_mesh(old_mesh, old_mat).unwrap();
            old.set_parent(Some(root));
            old.set_visible(false);
            let mut swim = Swim::new(KOI * path.scale).with_phase(k as f32 * 1.3);
            swim.wave = Wave { amplitude: 0.42, head: 0.18, waves: 0.85, ..Wave::default() };
            koi.push(Koi { body: node, bones, swim, old });
        }

        Pond { root, koi, streamers, tassel, light, look: Look::Swimming, shown: false, last: 0.0 }
    }

    /// Shows the pond and sets the dusk sky, sun, post and mist.
    pub fn enter(&mut self) {
        self.shown = true;
        self.root.set_visible(true);
        gfx3d::sky(&Sky {
            seed: 4,
            colors: [[0.16, 0.2, 0.34], [0.9, 0.45, 0.25], [0.35, 0.3, 0.55]],
            nebula_density: 0.45,
            star_density: 0.05,
            star_brightness: 0.6,
            sun_disc: 0.03,
            ..Default::default()
        });
        // A low evening sun from the right, so the shadows fall away from the pond.
        gfx3d::sun(&Sun { direction: vec3(-0.75, -0.6, -0.28), color: [1.0, 0.78, 0.55], intensity: 3.2, shadows: 1, shadow_distance: 14.0 });
        gfx3d::post(&Post { exposure: 1.15, bloom_strength: 0.55, bloom_threshold: 1.0, vignette: 0.35, saturation: 1.1, contrast: 1.06, ..Default::default() });
        gfx3d::fog(&Fog { mode: FogMode::Exponential, color: [0.32, 0.3, 0.4], density: 0.02, sky: 0.6, ..Default::default() });
        // Calm water: a faint swell, a clear pond look (water v2) over it.
        gfx3d::reflector(&Reflector { ripple: 0.12, max_distance: 40.0, ..Reflector::water(SURFACE) });
        gfx3d::water(&Water::pond());
        self.light.point_light(WARM, 6.0, 6.0);
    }

    pub fn leave(&mut self) {
        self.shown = false;
        self.root.set_visible(false);
        self.light.point_light(WARM, 0.0, 6.0);
        gfx3d::fog(&Fog::default());
        gfx3d::reflector(&Reflector::default());
        gfx3d::water(&Water::default());
    }

    pub fn camera(&self, t: f32) -> Camera {
        if self.look == Look::CloseUp {
            // Above and a little behind the big koi, looking down at it.
            let p = PATHS[0].at(t);
            let pos = p + vec3(0.5, 1.6, 0.6);
            return Camera { pos, rot: gfx3d::look_at(pos, p, Vec3::Y), fov_y: 0.95, near: 0.05, far: 120.0 };
        }
        let yaw = 0.18 * (t * 0.07).sin();
        let target = vec3(0.0, -0.3, -0.5);
        let pos = target + vec3(yaw.sin() * 4.0, 3.2, yaw.cos() * 4.0);
        Camera { pos, rot: gfx3d::look_at(pos, target + vec3(0.0, 0.7, -0.6), Vec3::Y), fov_y: 0.95, near: 0.1, far: 120.0 }
    }

    /// Swims the koi along their loops and waves the streamers and tassel.
    pub fn update(&mut self, t: f32) {
        let dt = (t - self.last).max(0.0);
        self.last = t;
        let swimming = matches!(self.look, Look::Swimming | Look::CloseUp);
        for (koi, path) in self.koi.iter_mut().zip(&PATHS) {
            let p = path.at(t);
            let e = 1.0 / 30.0;
            let vel = (path.at(t + e) - path.at(t - e)) / (2.0 * e);
            let heading = vel.x.atan2(vel.z);
            let ahead = path.at(t + e * 2.0) - path.at(t);
            let turn = {
                let h2 = ahead.x.atan2(ahead.z);
                let mut d = h2 - heading;
                while d > PI {
                    d -= TAU;
                }
                while d < -PI {
                    d += TAU;
                }
                d / (e * 2.0)
            };
            let rot = Quat::look_rotation(vel, Vec3::Y) * Quat::from_rotation_z(-0.12 * turn);
            let scale = KOI * path.scale;
            koi.body.set_transform(&Transform { pos: p, rot, scale: Vec3::splat(scale) });
            koi.old.set_transform(&Transform { pos: p, rot, scale: Vec3::splat(path.scale) });
            if swimming {
                koi.swim.update(dt, vel.length());
                koi.swim.apply(&koi.bones, turn * 0.12);
            } else {
                koi.bones.reset();
            }
            let faceted = self.look == Look::Faceted;
            koi.body.set_visible(!faceted);
            koi.old.set_visible(faceted);
        }
        for s in self.streamers.iter_mut().chain(std::iter::once(&mut self.tassel)) {
            s.phase += TAU * s.rate * 0.25 * dt;
            if !swimming {
                s.bones.reset();
            }
        }
        // Streamers: a ripple running down them side to side, a slower one up
        // and down, and a droop of their own weight.
        if swimming {
            for s in &self.streamers {
                let n = s.bones.len();
                let lift = Wave { axis: Vec3::Z, amplitude: 0.12, head: 0.4, waves: 0.8 };
                for i in 0..n {
                    let side = s.wave.angle(i, n, s.phase);
                    let up = lift.angle(i, n, s.phase * 0.6 + 0.9) - 0.035;
                    s.bones.turn(i, Quat::from_rotation_y(side) * Quat::from_rotation_z(up));
                }
            }
        }
        // The tassel also swings front to back, a quarter beat behind.
        if swimming {
            let n = self.tassel.bones.len();
            let w = Wave { axis: Vec3::X, amplitude: 0.1, ..self.tassel.wave };
            for i in 0..n {
                let a = self.tassel.wave.angle(i, n, self.tassel.phase);
                let b = w.angle(i, n, self.tassel.phase * 0.7 + 1.3);
                self.tassel.bones.turn(i, Quat::from_rotation_z(a) * Quat::from_rotation_x(b));
            }
        }
        let flicker = 1.0 + 0.07 * (t * 11.0).sin() + 0.04 * (t * 23.0).sin();
        self.light.point_light(WARM, 6.0 * flicker, 6.0);
    }

    pub fn cycle_look(&mut self) {
        self.look = self.look.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koi_stay_in_the_pond() {
        for path in &PATHS {
            for k in 0..600 {
                let p = path.at(k as f32 * 0.1);
                assert!((p.x * p.x + p.z * p.z).sqrt() < POND - 0.5, "{p:?}");
                assert!(p.y < -0.15 && p.y > FLOOR + 0.15, "{p:?}");
            }
        }
    }

    #[test]
    fn patterns_are_colors_and_the_pond_builds() {
        let mut pond = Pond::build();
        pond.enter();
        for k in 0..120 {
            pond.update(k as f32 / 60.0);
        }
        pond.cycle_look();
        pond.cycle_look();
        pond.update(2.1);
        assert_eq!(pond.look, Look::Stiff);
        assert_eq!(pattern(Variety::Tancho, vec3(0.0, 0.06, -0.37), Vec3::Y, 1), RED);
        assert_eq!(pattern(Variety::Kohaku, vec3(0.0, -0.1, 0.0), -Vec3::Y, 1), WHITE, "white belly");
    }
}
