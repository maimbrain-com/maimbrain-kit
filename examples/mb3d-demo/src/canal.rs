//! The moonlit canal: a vignette for fog and reflective water (mb3d 2).
//!
//! A glowing koi races down a lantern-lit canal at night under a deep blue
//! sky and a low moon. Mist hugs the water and the far end melts into the
//! sky (mb3d 2 fog), and the water mirrors the lanterns, the houses' windows
//! and the vermilion torii, rippling (the reflector), and it looks like
//! water (water v2): a current carries small ripples toward the camera, the
//! moon lays a glitter path on them, lantern light glows in the water and
//! ripples as caustics on the walls, and it foams where it laps the walls,
//! the floating lanterns and the koi. Over it all, the beauty pass: a
//! moonlit grade (teal shadows, warm lantern light), moonbeams through the
//! torii and the mist, halos around the lanterns, a little film grain and
//! faint streaks. Tap to compare looks: everything, everything but the
//! beauty pass, everything from low by the wall, water v1 (the reflector
//! alone), fog without reflections, and neither. Tap the bottom of the
//! screen to change the sky (sky v2): a clear night with a horizon glow,
//! a haloed moon and drifting clouds; dusk; a painted panorama (made with
//! `mb art sky`) under the same moon and stars; and mb3d 1's space sky for
//! comparison.
//!
//! The water reacts: the koi leaves a wake (a V, a bow wave and churned
//! foam), and every few seconds it leaps, splashing out and back in, with
//! rings spreading where it broke the surface; now and then a fish rises
//! somewhere ahead. Cherry petals drift on the current, bob on the rings
//! and are pushed aside by the koi.
//!
//! The canal is endless the way a runner's course is: a few 24 m segments
//! leapfrog ahead of the camera, and the four nearest lanterns take the
//! point lights.

use maimbrain::sys::{Asset, AssetState};
use maimbrain::gfx3d::{
    self, Atmosphere, Camera, Clouds, Film, FishDesc, Fog, FogMode, Grade, Halos, Instance, LightShafts, Material, MaterialDesc, MaterialStyle, MeshData, Node, Post, Quat, Reflector, Ring,
    Skeleton, Sky, SkyDisc, SkyTexture, Splash, Sun, Swim, Texture, Transform, Vec3, Water, srgb, vec3,
};

/// Meters per segment, segments alive, and how fast the camera travels (m/s).
const SEG: f32 = 24.0;
const SEGS: usize = 4;
const SPEED: f32 = 4.5;
/// Canal half-width (water), walkway outer edge, and the walkway's top.
const HALF: f32 = 3.0;
const WALK: f32 = 4.4;
const WALK_TOP: f32 = 0.45;
/// Lanterns per segment: (side, z offset into the segment).
const LANTERNS: [(f32, f32); 4] = [(-1.0, -4.0), (1.0, -10.0), (-1.0, -16.0), (1.0, -22.0)];
const LANTERN_Y: f32 = 1.55;
const WARM: [f32; 3] = [1.0, 0.52, 0.2];
/// Point lights given to the nearest lanterns (the demo scene holds the other four).
const LIGHTS: usize = 4;
/// The mist: the sky's color near the horizon.
const MIST: [f32; 3] = [0.055, 0.075, 0.13];
/// The mist at dusk.
const DUSK_MIST: [f32; 3] = [0.24, 0.13, 0.11];
/// The koi's leaps: the first at LEAP_FIRST s, then every LEAP_EVERY s, each
/// LEAP_TIME s in the air, LEAP_HEIGHT m high at the top.
const LEAP_FIRST: f32 = 1.6;
const LEAP_EVERY: f32 = 4.2;
const LEAP_TIME: f32 = 0.8;
const LEAP_HEIGHT: f32 = 0.7;
/// The koi's size across (for its wake and splashes), its length, and how high it swims.
const KOI_SIZE: f32 = 0.3;
const KOI_LENGTH: f32 = 0.8;
const KOI_Y: f32 = 0.06;
/// What lights the splashes: the moon, warmed a little by the lanterns.
const SPLASH_LIGHT: [f32; 3] = [0.6, 0.62, 0.8];
/// Petals afloat, the stretch of canal they cover ahead of the camera, and the current (m/s, toward the camera).
const PETALS: usize = 44;
const PETAL_AHEAD: f32 = 30.0;
const CURRENT: f32 = 0.35;
/// A fish rises ahead every RISE_EVERY s.
const RISE_EVERY: f32 = 1.3;

/// A cheap deterministic hash → 0..1 (layouts and recycling).
fn hash01(k: u32) -> f32 {
    let h = k.wrapping_mul(2_654_435_761).rotate_left(13).wrapping_mul(0x9e37_79b9);
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// A petal on the water: where it is, its drift, and its turn.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Petal {
    x: f32,
    z: f32,
    vx: f32,
    vz: f32,
    yaw: f32,
    spin: f32,
}

/// The koi at a moment: position, velocity and heading (yaw, radians).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Koi {
    pos: Vec3,
    vel: Vec3,
    heading: f32,
}

/// What the canal shows: everything (water v2 and the beauty pass),
/// everything but the beauty pass, everything from low by the wall, the
/// reflector without water v2 or the beauty pass (water v1), fog only, or
/// neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Beauty,
    Full,
    CloseUp,
    Mirror,
    FogOnly,
    Plain,
}

impl Look {
    pub fn next(self) -> Look {
        match self {
            Look::Beauty => Look::Full,
            Look::Full => Look::CloseUp,
            Look::CloseUp => Look::Mirror,
            Look::Mirror => Look::FogOnly,
            Look::FogOnly => Look::Plain,
            Look::Plain => Look::Beauty,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Look::Beauty => "FOG + WATER V2 + BEAUTY PASS",
            Look::Full => "FOG + WATER V2, NO BEAUTY PASS",
            Look::CloseUp => "WATER V2 + BEAUTY PASS, BY THE WALL",
            Look::Mirror => "FOG + REFLECTIONS (WATER V1)",
            Look::FogOnly => "FOG, NO REFLECTIONS",
            Look::Plain => "NO FOG, NO REFLECTIONS",
        }
    }
}

/// The canal's sky.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyLook {
    /// Sky v2: a deep blue gradient, a glow along the horizon, a moon with a halo, stars.
    Night,
    /// Sky v2: an orange horizon under deep blue, a low sun.
    Dusk,
    /// Sky v2: a painted night panorama (`assets/sky_night.jpg`) with the moon and stars over it.
    Painted,
    /// mb3d 1's space sky (nebula, stars, a sun disc), as the canal had it before sky v2.
    Classic,
}

impl SkyLook {
    pub fn next(self) -> SkyLook {
        match self {
            SkyLook::Night => SkyLook::Dusk,
            SkyLook::Dusk => SkyLook::Painted,
            SkyLook::Painted => SkyLook::Classic,
            SkyLook::Classic => SkyLook::Night,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SkyLook::Night => "NIGHT SKY",
            SkyLook::Dusk => "DUSK SKY",
            SkyLook::Painted => "PAINTED SKY",
            SkyLook::Classic => "MB3D 1 SKY",
        }
    }
    fn mist(self) -> [f32; 3] {
        if self == SkyLook::Dusk { DUSK_MIST } else { MIST }
    }
}

pub struct Canal {
    root: Node,
    segments: Vec<Node>,
    /// The segment index each slot shows (segment k starts at z = −k × SEG).
    at: Vec<i32>,
    water: Node,
    koi: Node,
    /// The koi's spine, and its swimming beat.
    koi_bones: Skeleton,
    swim: Swim,
    lights: Vec<Node>,
    splash: Splash,
    /// Rings the splashes and rising fish started, with when (for the petals to bob on).
    rings: Vec<(Ring, f32)>,
    petals: Vec<Petal>,
    petal_node: Node,
    /// How many petals have been recycled (seeds where they reappear).
    recycled: u32,
    /// The time `update` last ran at (None: the first update since `enter`).
    last: Option<f32>,
    pub look: Look,
    pub sky: SkyLook,
    /// The painted sky's asset, and its texture once it has loaded.
    pano_asset: Asset,
    pano: Option<Texture>,
    /// The painted sky's grade LUT (`assets/grade_night.png`, a 32³ strip), once it has loaded.
    lut_asset: Asset,
    lut: Option<Texture>,
    /// Whether the canal is the scene on screen.
    pub shown: bool,
}

fn material(desc: MaterialDesc) -> Material {
    Material::new(&desc).expect("material")
}

/// A torii: two pillars, the tie beam (nuki) and the top beam (kasagi) with upturned ends.
fn torii() -> MeshData {
    let mut m = MeshData::new();
    let at = |mesh: MeshData, p: Vec3| mesh.transformed(&Transform::at(p).matrix());
    for x in [-HALF - 0.45, HALF + 0.45] {
        m.merge(&at(MeshData::cylinder(0.17, 3.6, 12), vec3(x, WALK_TOP + 1.8, 0.0)));
        m.merge(&at(MeshData::cylinder(0.24, 0.3, 12), vec3(x, WALK_TOP + 0.15, 0.0)));
    }
    m.merge(&at(MeshData::cuboid(vec3(2.0 * HALF + 2.2, 0.22, 0.24)), vec3(0.0, WALK_TOP + 2.85, 0.0)));
    m.merge(&at(MeshData::cuboid(vec3(0.22, 0.6, 0.2)), vec3(0.0, WALK_TOP + 3.2, 0.0)));
    for (x, tilt) in [(-1.0, 0.12), (0.0, 0.0), (1.0, -0.12)] {
        let beam = MeshData::cuboid(vec3(if x == 0.0 { 5.0 } else { 2.6 }, 0.26, 0.42));
        let p = vec3(x * 3.3, WALK_TOP + 3.6 + if x == 0.0 { 0.0 } else { 0.1 }, 0.0);
        m.merge(&beam.transformed(&Transform::at(p).with_rot(Quat::from_rotation_z(tilt)).matrix()));
    }
    m
}

/// Low houses along one side of a segment (x > 0; mirrored for the other side), and their lit windows.
fn houses(seed: u32) -> (MeshData, MeshData) {
    let (mut walls, mut windows) = (MeshData::new(), MeshData::new());
    let mut z = -1.5;
    let mut k = seed;
    while z > -SEG + 2.0 {
        k = k.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let r = |s: u32| ((k >> s) & 255) as f32 / 255.0;
        let (w, h) = (3.0 + 2.0 * r(3), 1.8 + 2.4 * r(11));
        let x = WALK + 1.6;
        let body = MeshData::cuboid(vec3(3.0, h, w));
        walls.merge(&body.transformed(&Transform::at(vec3(x, WALK_TOP + h * 0.5, z - w * 0.5)).matrix()));
        // A pitched roof: a box rotated 45° along z, squashed.
        let roof = MeshData::cuboid(vec3(2.4, 2.4, w + 0.3)).transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_z(0.785)).with_scale3(vec3(1.0, 0.45, 1.0)).matrix());
        walls.merge(&roof.transformed(&Transform::at(vec3(x, WALK_TOP + h, z - w * 0.5)).matrix()));
        if r(19) > 0.3 {
            let win = MeshData::cuboid(vec3(0.05, 0.55, 0.7));
            windows.merge(&win.transformed(&Transform::at(vec3(x - 1.52, WALK_TOP + h * 0.55, z - w * 0.5 + 0.4 * (r(23) - 0.5))).matrix()));
        }
        z -= w + 0.4 + 1.5 * r(27);
    }
    (walls, windows)
}

impl Canal {
    /// Builds the canal (hidden until `enter`).
    pub fn build() -> Canal {
        let root = Node::new().unwrap();
        root.set_visible(false);

        let stone = material(MaterialDesc { base_color: [0.05, 0.055, 0.065, 1.0], roughness: 0.85, ..Default::default() });
        let wood = material(MaterialDesc { base_color: [0.035, 0.028, 0.026, 1.0], roughness: 0.7, ..Default::default() });
        let lit = srgb(0xffb15c);
        let window = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: lit, emissive_strength: 1.6, flags: Material::UNLIT, ..Default::default() });
        let paper = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: WARM, emissive_strength: 3.2, flags: Material::UNLIT, ..Default::default() });
        let red = srgb(0xc8321e);
        let vermilion = material(MaterialDesc { base_color: [red[0] * 0.6, red[1] * 0.6, red[2] * 0.6, 1.0], emissive: [0.9, 0.12, 0.04], emissive_strength: 1.0, roughness: 0.45, ..Default::default() });
        let water = material(MaterialDesc { base_color: [0.006, 0.01, 0.018, 1.0], roughness: 0.06, flags: Material::REFLECTIVE, ..Default::default() });
        let koi_skin = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [1.0, 0.45, 0.15], emissive_strength: 2.6, flags: Material::UNLIT, ..Default::default() });
        let koi_fins = material(MaterialDesc {
            base_color: [0.0, 0.0, 0.0, 0.75],
            emissive: [1.0, 0.55, 0.25],
            emissive_strength: 1.6,
            flags: Material::UNLIT | Material::ALPHA | Material::DOUBLE_SIDED,
            ..Default::default()
        });

        // One segment's meshes, shared by every segment.
        let walkway = MeshData::cuboid(vec3(WALK - HALF, 1.6, SEG));
        let mut sides = MeshData::new();
        for s in [-1.0, 1.0] {
            sides.merge(&walkway.transformed(&Transform::at(vec3(s * (HALF + WALK) * 0.5, WALK_TOP - 0.8, -SEG * 0.5)).matrix()));
        }
        let sides = sides.upload().unwrap();
        let torii = torii().upload().unwrap();
        let post = MeshData::cylinder(0.06, LANTERN_Y - WALK_TOP, 8).transformed(&Transform::at(vec3(0.0, (LANTERN_Y + WALK_TOP) * 0.5 - LANTERN_Y, 0.0)).matrix());
        let mut post = post;
        post.merge(&MeshData::cuboid(vec3(0.32, 0.05, 0.32)).transformed(&Transform::at(vec3(0.0, 0.22, 0.0)).matrix()));
        let post = post.upload().unwrap();
        let lantern = MeshData::uv_sphere(0.19, 16, 10).transformed(&Transform::at(Vec3::ZERO).with_scale3(vec3(1.0, 1.25, 1.0)).matrix()).upload().unwrap();
        let mut house_meshes = Vec::new();
        for k in 0..2u32 {
            let (mut w, mut win) = houses(17 + k * 7);
            let (w2, win2) = houses(91 + k * 13);
            let flip = Transform::at(Vec3::ZERO).with_scale3(vec3(-1.0, 1.0, 1.0)).matrix();
            w.merge(&w2.transformed(&flip));
            win.merge(&win2.transformed(&flip));
            house_meshes.push((w.upload().unwrap(), win.upload().unwrap()));
        }

        let mut floater = MeshData::cuboid(vec3(0.18, 0.16, 0.18));
        floater.merge(&MeshData::cuboid(vec3(0.26, 0.03, 0.26)).transformed(&Transform::at(vec3(0.0, -0.08, 0.0)).matrix()));
        let floater = floater.upload().unwrap();
        let float_mat = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [1.0, 0.75, 0.45], emissive_strength: 1.8, flags: Material::UNLIT, ..Default::default() });

        let mut segments = Vec::new();
        for k in 0..SEGS {
            let seg = Node::new().unwrap();
            seg.set_parent(Some(root));
            let child = |mesh, mat, t: Transform| {
                let n = Node::with_mesh(mesh, mat).unwrap();
                n.set_parent(Some(seg));
                n.set_transform(&t);
            };
            child(sides, stone, Transform::at(Vec3::ZERO));
            let (w, win) = house_meshes[k % 2];
            child(w, wood, Transform::at(Vec3::ZERO));
            child(win, window, Transform::at(Vec3::ZERO));
            if k % 2 == 0 {
                child(torii, vermilion, Transform::at(vec3(0.0, 0.0, -1.0)));
            }
            for (side, z) in LANTERNS {
                let p = vec3(side * (HALF + 0.4), LANTERN_Y, z);
                child(post, wood, Transform::at(p));
                child(lantern, paper, Transform::at(p));
            }
            // Floating lanterns: one instanced draw per segment, tinted three ways.
            let boats = Node::with_mesh(floater, float_mat).unwrap();
            boats.set_parent(Some(seg));
            let list: Vec<Instance> = (0..7u32)
                .map(|j| {
                    let h = j.wrapping_mul(2_654_435_761).wrapping_add(k as u32 * 40_503) >> 8;
                    let x = ((h & 1023) as f32 / 1023.0 - 0.5) * (2.0 * HALF - 1.0);
                    let z = -2.0 - j as f32 * 3.1 - ((h >> 10) & 255) as f32 / 255.0 * 1.5;
                    Instance::at(vec3(x, 0.07, z)).with_rot(Quat::from_rotation_y(h as f32)).with_color([0xffe2b0ff, 0xffb070ff, 0xff8a9aff][(h % 3) as usize])
                })
                .collect();
            boats.set_instances(&list);
            segments.push(seg);
        }

        let water_node = Node::with_mesh(MeshData::plane(2.0 * HALF + 0.2, SEG * SEGS as f32).upload().unwrap(), water).unwrap();
        water_node.set_parent(Some(root));

        // The koi: a glowing lofted fish (MeshData::fish) with flowing fins, on a spine of bones that swims.
        let desc = FishDesc { length: KOI_LENGTH, fins: 1.8, bones: 8, ..Default::default() };
        let fish = MeshData::fish(&desc);
        let koi = Node::with_mesh(fish.body.upload().unwrap(), koi_skin).unwrap();
        koi.set_parent(Some(root));
        let fins = Node::with_mesh(fish.fins.upload().unwrap(), koi_fins).unwrap();
        fins.set_parent(Some(koi));
        let koi_bones = Skeleton::chain(koi, &desc.spine()).unwrap();
        koi_bones.bind(koi);
        koi_bones.bind(fins);
        // It races along with the camera: a long glide per beat, or it would flutter.
        let mut swim = Swim::new(KOI_LENGTH);
        swim.stride = 2.4;

        // Cherry petals: one instanced draw of a small flat oval, lit (lanterns tint them), not worth reflecting.
        let petal_mesh = MeshData::cylinder(0.045, 0.004, 10).transformed(&Transform::at(Vec3::ZERO).with_scale3(vec3(1.0, 1.0, 1.6)).matrix()).upload().unwrap();
        let blossom = srgb(0xf6b8c8);
        let petal_mat = material(MaterialDesc { base_color: [blossom[0], blossom[1], blossom[2], 1.0], emissive: [0.16, 0.05, 0.09], emissive_strength: 1.0, roughness: 0.55, ..Default::default() });
        petal_mat.set_style(&MaterialStyle { flags: MaterialStyle::NO_REFLECTION, ..Default::default() });
        let petal_node = Node::with_mesh(petal_mesh, petal_mat).unwrap();
        petal_node.set_parent(Some(root));

        let lights = (0..LIGHTS)
            .map(|_| {
                let n = Node::new().unwrap();
                n.set_parent(Some(root));
                n.point_light(WARM, 0.0, 7.0);
                n
            })
            .collect();
        Canal {
            root,
            segments,
            at: vec![-1; SEGS],
            shown: false,
            water: water_node,
            koi,
            koi_bones,
            swim,
            lights,
            splash: Splash::new(SPLASH_LIGHT, KOI_SIZE).unwrap(),
            rings: Vec::new(),
            petals: Vec::new(),
            petal_node,
            recycled: 0,
            last: None,
            look: Look::Beauty,
            sky: SkyLook::Night,
            pano_asset: Asset::load("assets/sky_night.jpg"),
            pano: None,
            lut_asset: Asset::load("assets/grade_night.png"),
            lut: None,
        }
    }

    /// Shows the canal and sets the scene-wide sky, sun, post, fog (and reflections) for its look.
    pub fn enter(&mut self) {
        self.shown = true;
        self.root.set_visible(true);
        self.last = None;
        self.rings.clear();
        self.recycled = 0;
        // Petals strewn over the canal ahead of where the camera starts.
        self.petals = (0..PETALS as u32)
            .map(|k| Petal {
                x: (hash01(k * 3) - 0.5) * (2.0 * HALF - 0.4),
                z: 1.5 - hash01(k * 3 + 1) * PETAL_AHEAD,
                vx: 0.0,
                vz: CURRENT,
                yaw: hash01(k * 3 + 2) * std::f32::consts::TAU,
                spin: (hash01(k * 7 + 5) - 0.5) * 0.6,
            })
            .collect();
        gfx3d::post(&Post { exposure: 1.35, bloom_strength: 0.8, bloom_threshold: 0.9, vignette: 0.35, saturation: 1.05, contrast: 1.04, ..Default::default() });
        self.apply_sky();
        self.apply_look();
    }

    /// The sky, its sun (or moon) and the atmosphere over it.
    pub fn apply_sky(&self) {
        let stars = Sky {
            seed: 21,
            colors: [[0.004, 0.009, 0.03], [0.018, 0.05, 0.13], [0.05, 0.03, 0.11]],
            nebula_density: 0.55,
            star_density: 0.9,
            star_brightness: 0.9,
            sun_disc: 0.025,
            ..Default::default()
        };
        // The moon, low ahead: its light travels back toward the camera.
        let moon = Sun { direction: vec3(-0.06, -0.17, 0.98), color: [0.62, 0.72, 1.0], intensity: 0.5, shadows: 0, shadow_distance: 30.0 };
        if self.sky != SkyLook::Painted {
            gfx3d::sky_texture(None, &SkyTexture::default());
        }
        match self.sky {
            SkyLook::Classic => {
                gfx3d::sky(&stars);
                gfx3d::sun(&moon);
                gfx3d::atmosphere(&Atmosphere::default());
                gfx3d::clouds(&Clouds::default());
                gfx3d::sky_disc(&SkyDisc::default());
            }
            SkyLook::Night => {
                // A faint Milky Way over the gradient; the moon a little higher, off to the left.
                gfx3d::sky(&Sky { nebula_density: 0.3, star_density: 1.1, star_brightness: 1.1, sun_disc: 0.0, ..stars });
                gfx3d::sun(&Sun { direction: vec3(0.12, -0.22, 0.97), ..moon });
                gfx3d::atmosphere(&Atmosphere { glow: [0.05, 0.065, 0.1], ..Atmosphere::night() });
                gfx3d::clouds(&Clouds::night());
                gfx3d::sky_disc(&SkyDisc::moon());
            }
            SkyLook::Dusk => {
                gfx3d::sky(&Sky { nebula_density: 0.0, star_density: 0.8, star_brightness: 0.8, sun_disc: 0.0, ..stars });
                gfx3d::sun(&Sun { direction: vec3(0.2, -0.07, 0.98), color: [1.0, 0.55, 0.3], intensity: 1.6, ..moon });
                gfx3d::atmosphere(&Atmosphere::dusk());
                gfx3d::clouds(&Clouds::dusk());
                // A low sun, reddened by the air.
                gfx3d::sky_disc(&SkyDisc { radius: 0.035, color: [14.0, 6.0, 2.2], halo: [1.4, 0.6, 0.2], ..SkyDisc::sun() });
            }
            SkyLook::Painted => {
                // The painting is the background; the night's stars, halo and moon draw over it.
                // Until its texture is ready, the night gradient stands in.
                gfx3d::sky(&Sky { nebula_density: 0.0, star_density: 0.5, star_brightness: 0.8, sun_disc: 0.0, ..stars });
                gfx3d::sun(&Sun { direction: vec3(0.12, -0.22, 0.97), ..moon });
                gfx3d::atmosphere(&Atmosphere { glow: [0.05, 0.065, 0.1], ..Atmosphere::night() });
                gfx3d::clouds(&Clouds::default());
                gfx3d::sky_disc(&SkyDisc::moon());
                // Turned so the Milky Way's foot rises over the far end of the canal.
                gfx3d::sky_texture(self.pano, &SkyTexture { intensity: 0.7, yaw: 1.9, ..Default::default() });
            }
        }
    }

    pub fn apply_look(&self) {
        let mist = Fog { mode: FogMode::Exponential, color: self.sky.mist(), density: 0.045, height: 0.2, height_falloff: 0.3, sky: 0.8, ..Default::default() };
        // The water: rippling, softly blurred, nothing past where the mist hides it.
        let mirror = Reflector { max_distance: 70.0, ..Reflector::water(0.0) };
        // Water v2: a canal current flowing toward the camera with the petals, ripples on it, foam at the walls and boats.
        let canal = Water::canal().flowing(Vec3::Z, CURRENT);
        let (fog, reflector, water) = match self.look {
            // With water v2 the swell can be gentler: its own ripples carry the detail.
            Look::Beauty | Look::CloseUp | Look::Full => (mist, Reflector { ripple: 0.15, ..mirror }, canal),
            Look::Mirror => (mist, mirror, Water::default()),
            Look::FogOnly => (mist, Reflector::default(), Water::default()),
            Look::Plain => (Fog::default(), Reflector::default(), Water::default()),
        };
        gfx3d::fog(&fog);
        gfx3d::reflector(&reflector);
        gfx3d::water(&water);
        self.apply_beauty();
    }

    /// The beauty pass for the sky shown (in the first two looks, and not over mb3d 1's sky).
    fn apply_beauty(&self) {
        let on = matches!(self.look, Look::Beauty | Look::CloseUp) && self.sky != SkyLook::Classic;
        if !on {
            Canal::beauty_off();
            return;
        }
        if self.sky == SkyLook::Dusk {
            gfx3d::grade(&Grade::golden_hour());
            gfx3d::shafts(&LightShafts { intensity: 0.45, ..LightShafts::sunbeams() });
        } else {
            // The painted sky's grade ends in a LUT baked by hand (tools/lut.py), once it has loaded.
            let lut = if self.sky == SkyLook::Painted { self.lut } else { None };
            gfx3d::grade(&Grade { lut, ..Grade::moonlit() });
            gfx3d::shafts(&LightShafts::moonbeams());
        }
        // The lanterns' warm glow hanging in the mist; streaks only off the brightest lamps.
        gfx3d::halos(&Halos::mist());
        gfx3d::film(&Film { streak_threshold: 6.0, ..Film::subtle() });
    }

    fn beauty_off() {
        gfx3d::grade(&Grade::none());
        gfx3d::shafts(&LightShafts::default());
        gfx3d::halos(&Halos::default());
        gfx3d::film(&Film::default());
    }

    pub fn leave(&mut self) {
        self.shown = false;
        self.root.set_visible(false);
        for l in &self.lights {
            l.point_light(WARM, 0.0, 7.0);
        }
        gfx3d::fog(&Fog::default());
        gfx3d::reflector(&Reflector::default());
        gfx3d::water(&Water::default());
        gfx3d::atmosphere(&Atmosphere::default());
        gfx3d::clouds(&Clouds::default());
        gfx3d::sky_disc(&SkyDisc::default());
        gfx3d::sky_texture(None, &SkyTexture::default());
        gfx3d::wake(0, Vec3::ZERO, Vec3::ZERO, KOI_SIZE, 0.0);
        Canal::beauty_off();
    }

    /// Where the camera is along the canal at time `t` (it travels toward −z).
    fn travel(t: f32) -> f32 {
        -SPEED * t
    }

    /// The camera at `t`: down the middle of the canal, or (close up) low by the right-hand wall, looking along it.
    pub fn camera(&self, t: f32) -> Camera {
        let z = Canal::travel(t);
        if self.look == Look::CloseUp {
            let pos = vec3(HALF - 0.85, 0.42 + 0.03 * (t * 0.9).sin(), z);
            let target = vec3(HALF - 0.2, 0.05, z - 5.0);
            return Camera { pos, rot: gfx3d::look_at(pos, target, Vec3::Y), fov_y: 0.95, near: 0.05, far: 160.0 };
        }
        let sway = 0.35 * (t * 0.4).sin();
        let pos = vec3(sway, 1.55 + 0.08 * (t * 0.9).sin(), z);
        let target = vec3(sway * 0.4, 0.75, z - 12.0);
        Camera { pos, rot: gfx3d::look_at(pos, target, Vec3::Y), fov_y: 0.95, near: 0.1, far: 160.0 }
    }

    /// Leapfrogs segments ahead of the camera, swims the koi, and gives the lights to the nearest lanterns.
    pub fn update(&mut self, t: f32) {
        // Decoding the 2048×1024 painting takes the host one long frame: do it when it's first shown.
        if self.pano.is_none() && self.sky == SkyLook::Painted && self.pano_asset.state() == AssetState::Ready {
            self.pano = Texture::new(self.pano_asset);
            self.apply_sky();
        }
        if self.lut.is_none() && self.lut_asset.state() == AssetState::Ready {
            // 1024×32: small enough to decode any time.
            self.lut = Texture::new(self.lut_asset);
            self.apply_beauty();
        }
        let z = Canal::travel(t);
        // The segment the camera is in, starting one behind it.
        let first = ((-z) / SEG).floor() as i32 - 1;
        for (slot, seg) in self.segments.iter().enumerate() {
            let k = first + (slot as i32 - first).rem_euclid(SEGS as i32);
            if self.at[slot] != k {
                self.at[slot] = k;
                seg.set_transform(&Transform::at(vec3(0.0, 0.0, -(k as f32) * SEG)));
            }
        }
        self.water.set_transform(&Transform::at(vec3(0.0, 0.0, z - SEG * SEGS as f32 * 0.5 + SEG)));
        let dt = self.last.map_or(0.0, |l| (t - l).max(0.0));
        let koi = Canal::koi(t);
        let pitch = koi.vel.y.atan2(SPEED);
        self.koi.set_transform(&Transform::at(koi.pos).with_rot(Quat::from_rotation_y(-koi.heading) * Quat::from_rotation_x(pitch)));
        // Its body swims: a beat that follows its speed (slower in the air), bending into its weave.
        let e = 1.0 / 30.0;
        let turn = (Canal::koi(t + e).heading - Canal::koi(t - e).heading) / (2.0 * e);
        let speed = vec3(koi.vel.x, 0.0, koi.vel.z).length();
        self.swim.update(dt, if Canal::leap_phase(t).is_some() { 0.3 * speed } else { speed });
        self.swim.apply(&self.koi_bones, -turn * 0.08);
        self.water_life(t, dt, koi);
        self.last = Some(t);

        // The lanterns from just behind the camera onward, nearest first.
        let mut lanterns: Vec<Vec3> = self
            .at
            .iter()
            .flat_map(|&k| LANTERNS.iter().map(move |&(side, dz)| vec3(side * (HALF + 0.4), LANTERN_Y, -(k as f32) * SEG + dz)))
            .filter(|p| p.z < z + 2.0)
            .collect();
        lanterns.sort_by(|a, b| b.z.total_cmp(&a.z));
        for (l, p) in self.lights.iter().zip(&lanterns) {
            l.set_transform(&Transform::at(*p));
            let flicker = 1.0 + 0.06 * (t * 13.0 + p.z).sin() + 0.04 * (t * 23.0 + p.x * 3.0).sin();
            l.point_light(WARM, 7.0 * flicker, 7.0);
        }
    }

    /// Where the koi is at `t`, how it's moving, and which way it heads (yaw).
    /// It weaves along just under the surface ahead of the camera and leaps
    /// every LEAP_EVERY s.
    fn koi(t: f32) -> Koi {
        let z = Canal::travel(t) - 6.5 - 1.2 * (t * 0.7).sin();
        let x = 1.2 * (t * 1.3).sin();
        let (vx, vz) = (1.2 * 1.3 * (t * 1.3).cos(), -SPEED - 1.2 * 0.7 * (t * 0.7).cos());
        let (lift, vy) = match Canal::leap_phase(t) {
            Some(s) => (4.0 * LEAP_HEIGHT * s * (1.0 - s), 4.0 * LEAP_HEIGHT * (1.0 - 2.0 * s) / LEAP_TIME),
            None => (0.0, 0.0),
        };
        Koi { pos: vec3(x, KOI_Y + lift, z), vel: vec3(vx, vy, vz), heading: vx.atan2(-vz) }
    }

    /// How far through a leap the koi is at `t` (0–1), or None in the water.
    fn leap_phase(t: f32) -> Option<f32> {
        if t < LEAP_FIRST {
            return None;
        }
        let into = (t - LEAP_FIRST) % LEAP_EVERY;
        (into < LEAP_TIME).then_some(into / LEAP_TIME)
    }

    /// The water's life: the koi's wake, its splashes out and back in, fish
    /// rising ahead, and the petals drifting, bobbing and pushed aside.
    fn water_life(&mut self, t: f32, dt: f32, koi: Koi) {
        let airborne = Canal::leap_phase(t).is_some();
        // The wake: on while it swims, off while it's in the air (what it made fades where it is).
        gfx3d::wake(0, koi.pos, koi.vel, KOI_SIZE, if airborne { 0.0 } else { 0.7 });
        let Some(last) = self.last else { return };
        let was_airborne = Canal::leap_phase(last).is_some();
        let surface = vec3(koi.pos.x, 0.0, koi.pos.z);
        if airborne != was_airborne {
            // Out (slower) or back in (faster): drops, mist and rings where it broke the surface.
            let rings = self.splash.splash(surface, KOI_SIZE, if airborne { 2.5 } else { 4.5 });
            self.rings.extend(rings.map(|r| (r, t)));
        }
        // Spray off its bow, ten times a second while it swims.
        if !airborne && (t * 10.0).floor() != (last * 10.0).floor() {
            let bow = koi.pos + vec3(koi.vel.x, 0.0, koi.vel.z).normalize() * 0.3;
            self.splash.spray(vec3(bow.x, 0.03, bow.z), koi.vel, 2);
        }
        // A fish rising somewhere ahead.
        let k = (t / RISE_EVERY).floor();
        if k != (last / RISE_EVERY).floor() {
            let n = k as u32;
            let at = vec3((hash01(n * 5 + 1) - 0.5) * (2.0 * HALF - 1.0), 0.0, Canal::travel(t) - 9.0 - 14.0 * hash01(n * 5 + 2));
            let r = Ring { center: at, strength: 0.5, speed: 0.7, seconds: 2.4 };
            r.start();
            self.rings.push((r, t));
        }
        self.rings.retain(|(r, t0)| t - t0 < r.seconds);

        // Petals: the current carries them toward the camera; the koi pushes them aside as it passes.
        let cam_z = Canal::travel(t);
        let mut list = Vec::with_capacity(self.petals.len());
        for p in &mut self.petals {
            let (dx, dz) = (p.x - koi.pos.x, p.z - koi.pos.z);
            let d = (dx * dx + dz * dz).sqrt().max(1e-3);
            if !airborne && d < 1.0 {
                let push = 6.0 * (1.0 - d) * dt;
                p.vx += dx / d * push;
                p.vz += dz / d * push;
            }
            // Water drag pulls them back to the current's drift.
            let k = (-1.5 * dt).exp();
            p.vx *= k;
            p.vz = CURRENT + (p.vz - CURRENT) * k;
            p.x += p.vx * dt;
            p.z += p.vz * dt;
            p.yaw += (p.spin + 0.6 * (p.vx.abs() + (p.vz - CURRENT).abs())) * dt;
            let wall = HALF - 0.15;
            if p.x.abs() > wall {
                p.x = p.x.clamp(-wall, wall);
                p.vx = -p.vx * 0.3;
            }
            if p.z > cam_z + 1.5 {
                // Drifted past the camera: float in again far ahead.
                self.recycled += 1;
                let n = self.recycled;
                *p = Petal { x: (hash01(n * 11) - 0.5) * (2.0 * HALF - 0.4), z: cam_z - PETAL_AHEAD + 4.0 * hash01(n * 11 + 1), vx: 0.0, vz: CURRENT, yaw: hash01(n * 11 + 2) * std::f32::consts::TAU, spin: p.spin };
            }
            let here = vec3(p.x, 0.0, p.z);
            let lift: f32 = self.rings.iter().map(|(r, t0)| r.height(here, t - t0)).sum();
            list.push(Instance::at(vec3(p.x, 0.012 + lift, p.z)).with_rot(Quat::from_rotation_y(p.yaw)));
        }
        self.petal_node.set_instances(&list);
    }

    pub fn cycle_look(&mut self) {
        self.look = self.look.next();
        self.apply_look();
    }

    pub fn cycle_sky(&mut self) {
        self.sky = self.sky.next();
        self.apply_sky();
        self.apply_look();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_leapfrog_ahead_of_the_camera() {
        let mut c = Canal::build();
        for k in 0..900 {
            let t = k as f32 / 30.0;
            c.update(t);
            let mut at = c.at.clone();
            at.sort();
            // The segment behind the camera's, the camera's, and two ahead.
            let here = (SPEED * t / SEG).floor() as i32;
            assert_eq!(at, (here - 1..here - 1 + SEGS as i32).collect::<Vec<_>>(), "t = {t}");
        }
    }

    #[test]
    fn the_beauty_pass_is_the_first_look_and_one_tap_compares_it() {
        let mut c = Canal::build();
        c.enter();
        assert_eq!(c.look, Look::Beauty);
        let mut seen = vec![c.look];
        for _ in 0..5 {
            c.cycle_look();
            seen.push(c.look);
        }
        assert_eq!(seen, [Look::Beauty, Look::Full, Look::CloseUp, Look::Mirror, Look::FogOnly, Look::Plain]);
        c.cycle_look();
        assert_eq!(c.look, Look::Beauty);
        // Every sky with the beauty pass on, then off again on leaving.
        for _ in 0..4 {
            c.cycle_sky();
        }
        c.leave();
    }

    #[test]
    fn the_koi_leaps_out_and_lands_back_in() {
        assert!(Canal::leap_phase(LEAP_FIRST - 0.01).is_none());
        let top = Canal::koi(LEAP_FIRST + LEAP_TIME * 0.5);
        assert!((top.pos.y - KOI_Y - LEAP_HEIGHT).abs() < 1e-3 && top.vel.y.abs() < 1e-3, "{top:?}");
        assert!(Canal::koi(LEAP_FIRST + 0.05).vel.y > 0.0 && Canal::koi(LEAP_FIRST + LEAP_TIME - 0.05).vel.y < 0.0);
        let back = Canal::koi(LEAP_FIRST + LEAP_TIME + 0.01);
        assert!((back.pos.y - KOI_Y).abs() < 1e-6 && Canal::leap_phase(LEAP_FIRST + LEAP_EVERY + 0.1).is_some());
        // Its velocity is its motion's derivative (the wake and spray ride on it).
        let (a, b) = (Canal::koi(3.0), Canal::koi(3.0 + 1e-3));
        assert!(((b.pos - a.pos) * 1e3 - a.vel).length() < 0.05);
    }

    #[test]
    fn petals_stay_on_the_canal_ahead_of_the_camera() {
        let mut c = Canal::build();
        c.enter();
        for k in 0..1200 {
            c.update(k as f32 / 60.0);
        }
        let cam = Canal::travel(1199.0 / 60.0);
        assert_eq!(c.petals.len(), PETALS);
        assert!(c.recycled > 0);
        assert!(c.petals.iter().all(|p| p.x.abs() <= HALF && p.z <= cam + 1.6 && p.z > cam - PETAL_AHEAD - 2.0), "{:?}", c.petals);
        assert!(!c.rings.is_empty() && c.rings.len() <= 6, "{}", c.rings.len());
    }
}
