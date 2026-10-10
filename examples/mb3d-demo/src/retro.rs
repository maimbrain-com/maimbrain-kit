//! The retro look: "Mode 7 parallax backgrounds and SNES pixel art meets
//! Sega Saturn 3D". Two blocky mechs face off on a Mode 7 floor under a
//! banded dusk sky with far mountains and near hills scrolling at their own
//! rates (`gfx3d::backdrop`), the 3D drawn at 240 pixels across with 15-bit
//! color, a Bayer dither and wobbling vertices (`gfx3d::retro`), the HUD
//! drawn sharp at full resolution over it. `--param scene=retro`; add
//! `--param retro=off` for the same scene at full resolution. Tap to toggle.
//!
//! The pixel art is drawn by `tools/make_retro_art.py` (free, seeded).
//!
//! Every moving thing makes sound (`maimbrain::foley`): each mech stomps on
//! its stride (the bob's low points), its servos whine with how fast it
//! sways and turns, and every 7 s one of them hops on its thrusters and
//! lands with a thud as hard as the fall, all placed from the orbiting
//! camera (louder near it, panned to its side). The sounds are the skill's
//! procedural mech pack: `python3 <skill>/scripts/sfx.py --pack mech
//! games/mb3d-demo/assets` (free; `mb sfx <game> --pack mech` generates one).

use maimbrain::audio::Sound;
use maimbrain::foley::{Contact, Drive, Foley, Gait, Listener};
use maimbrain::gfx2d::{self, Font};
use maimbrain::gfx3d::{
    self, Camera, Fog, FogMode, Layer, Mat4, Material, MaterialDesc, Mesh, MeshData, Node, Post, Quat, Retro, Sky, Sun, Texture,
    Transform, Vec3, mode7_anchor, vec3,
};
use maimbrain::input::{self, Kind};
use maimbrain::sys::{self, Asset, AssetState};

const TILE: f32 = 4.0;
/// The fog's color: the near hills' darkest green (0x1F4E3C, linear), so the floor's far edge fades into them.
const HORIZON: [f32; 3] = [0.014, 0.075, 0.045];

pub struct Retro3 {
    assets: [Asset; 4],
    floor: Option<Node>,
    mechs: Vec<Node>,
    retro: bool,
    t: f32,
    sfx: [Asset; 4],
    sounds: Option<Sounds>,
    foley: Foley,
    legs: Vec<Legs>,
}

/// The mech pack, once loaded.
#[derive(Clone, Copy)]
struct Sounds {
    stomp: Sound,
    impact: Sound,
}

/// One mech's moving parts, for its foley: its stride, its servos, its thrusters, its feet.
struct Legs {
    gait: Gait,
    servo: Option<Drive>,
    jets: Option<Drive>,
    feet: Contact,
    /// Where it was a frame ago (its speed) and its turn (its turn rate).
    last: Option<(Vec3, f32)>,
}

/// Seconds between hops; each mech hops in turn, on its thrusters for 0.45 s.
const HOP_EVERY: f32 = 7.0;
const HOP_BURN: f32 = 0.45;

/// How high a mech is on its thrusters `t` seconds into the scene (mech `i`).
fn hop_height(t: f32, i: usize) -> f32 {
    let k = (t + i as f32 * HOP_EVERY * 0.5) % HOP_EVERY - (HOP_EVERY - 2.0);
    if k < 0.0 {
        return 0.0;
    }
    // Thrust up at 14 m/s² over gravity for HOP_BURN, then a ballistic fall.
    let (a, g) = (14.0f32, 9.8f32);
    let (h1, v1) = (0.5 * a * HOP_BURN * HOP_BURN, a * HOP_BURN);
    if k < HOP_BURN { 0.5 * a * k * k } else { (h1 + v1 * (k - HOP_BURN) - 0.5 * (a + g) * (k - HOP_BURN).powi(2)).max(0.0) }
}

fn thrusting(t: f32, i: usize) -> bool {
    let k = (t + i as f32 * HOP_EVERY * 0.5) % HOP_EVERY - (HOP_EVERY - 2.0);
    (0.0..HOP_BURN).contains(&k)
}

fn srgb(c: u32) -> [f32; 3] {
    let f = |s: u32| ((s & 255) as f32 / 255.0).powf(2.2);
    [f(c >> 16), f(c >> 8), f(c)]
}

/// A flat-shaded box of `size` at `at`, colored `rgb` (0xRRGGBB).
fn part(m: &mut MeshData, size: Vec3, at: Vec3, rot: f32, rgb: u32) {
    let mut b = MeshData::cuboid(size).transformed(&Mat4::from_trs(at, Quat::from_rotation_y(rot), Vec3::ONE));
    b.colors = vec![(rgb << 8) | 0xff; b.positions.len()];
    m.merge(&b);
}

/// A Saturn-era mech: a few dozen flat-shaded boxes in two team colors.
fn mech(main: u32, trim: u32) -> MeshData {
    let mut m = MeshData::new();
    let dark = 0x2a2a34;
    // Legs: thighs, shins, feet.
    for s in [-1.0f32, 1.0] {
        part(&mut m, vec3(0.45, 0.9, 0.5), vec3(0.45 * s, 1.55, 0.0), 0.0, dark);
        part(&mut m, vec3(0.55, 1.0, 0.6), vec3(0.5 * s, 0.75, 0.08), 0.0, main);
        part(&mut m, vec3(0.7, 0.25, 1.0), vec3(0.5 * s, 0.13, 0.15), 0.0, trim);
        // Shoulders, arms, fists.
        part(&mut m, vec3(0.7, 0.55, 0.8), vec3(1.05 * s, 2.95, 0.0), 0.0, trim);
        part(&mut m, vec3(0.4, 1.1, 0.45), vec3(1.1 * s, 2.2, 0.05), 0.0, main);
        part(&mut m, vec3(0.45, 0.4, 0.5), vec3(1.1 * s, 1.55, 0.1), 0.0, dark);
    }
    // Hips, torso, chest plate, head with a visor, a cannon on the right shoulder.
    part(&mut m, vec3(1.2, 0.4, 0.7), vec3(0.0, 2.0, 0.0), 0.0, dark);
    part(&mut m, vec3(1.5, 1.1, 1.0), vec3(0.0, 2.7, 0.0), 0.0, main);
    part(&mut m, vec3(1.0, 0.6, 0.2), vec3(0.0, 2.8, 0.55), 0.0, trim);
    part(&mut m, vec3(0.55, 0.45, 0.55), vec3(0.0, 3.5, 0.05), 0.0, main);
    part(&mut m, vec3(0.45, 0.12, 0.1), vec3(0.0, 3.52, 0.34), 0.0, 0xffd040);
    part(&mut m, vec3(0.3, 0.3, 1.4), vec3(1.05, 3.4, 0.35), 0.0, dark);
    m.flat_shaded()
}

impl Retro3 {
    pub fn new() -> Retro3 {
        gfx3d::sky(&Sky { colors: [srgb(0x2a1a5e), srgb(0xe86a5a), srgb(0x46206e)], star_density: 0.0, nebula_density: 0.0, ..Default::default() });
        gfx3d::sun(&Sun { direction: vec3(-0.6, -0.5, -0.6), color: [1.0, 0.85, 0.7], intensity: 3.0, shadows: 1, shadow_distance: 30.0 });
        gfx3d::post(&Post { exposure: 1.0, bloom_strength: 0.15, bloom_threshold: 1.5, vignette: 0.15, saturation: 1.1, contrast: 1.05, ..Default::default() });
        gfx3d::fog(&Fog { color: HORIZON, mode: FogMode::Linear, start: 40.0, end: 140.0, sky: 0.0, ..Default::default() });
        let retro = sys::param("retro").as_deref() != Some("off");
        let mut r = Retro3 {
            assets: [
                Asset::load("assets/retro_sky.png"),
                Asset::load("assets/retro_mountains.png"),
                Asset::load("assets/retro_hills.png"),
                Asset::load("assets/retro_ground.png"),
            ],
            floor: None,
            mechs: Vec::new(),
            retro,
            t: 0.0,
            sfx: [
                Asset::load("assets/mech_stomp.ogg"),
                Asset::load("assets/mech_servo.ogg"),
                Asset::load("assets/mech_thruster.ogg"),
                Asset::load("assets/mech_impact.ogg"),
            ],
            sounds: None,
            foley: Foley::new(sys::rand_seed()),
            legs: Vec::new(),
        };
        r.set_look();
        let body = Material::new(&MaterialDesc { base_color: [1.0; 4], roughness: 0.7, ..Default::default() }).expect("material");
        for (i, (main, trim)) in [(0x3a6cd0, 0xe8e8f0), (0xc83c3c, 0x303038)].into_iter().enumerate() {
            let mesh = Mesh::new(&mech(main, trim)).expect("mesh");
            let node = Node::with_mesh(mesh, body).expect("node");
            let x = if i == 0 { -3.0 } else { 3.0 };
            node.set_transform(&Transform { pos: vec3(x, 0.0, -2.0), rot: Quat::from_rotation_y(if i == 0 { -1.2 } else { 1.2 }), ..Default::default() });
            if i == 0 {
                // `mb shot --readability` checks the blue mech stands out.
                node.mark_subject();
            }
            r.mechs.push(node);
            r.legs.push(Legs { gait: Gait::biped(), servo: None, jets: None, feet: Contact::with(0.02, 0.15), last: None });
        }
        r
    }

    /// The mechs' foley for this frame: stomps on the stride, servos with the sway and turn,
    /// thrusters on a hop, a thud on landing.
    fn foley(&mut self, dt: f32, places: &[(Vec3, f32, f32)]) {
        if self.sounds.is_none() && self.sfx.iter().all(|a| a.state() == AssetState::Ready) {
            let s = |i: usize| Sound::new(self.sfx[i]);
            if let (Some(stomp), Some(servo), Some(jets), Some(impact)) = (s(0), s(1), s(2), s(3)) {
                self.sounds = Some(Sounds { stomp, impact });
                for l in &mut self.legs {
                    l.servo = Some(Drive::servo(servo));
                    l.jets = Some(Drive::thruster(jets));
                }
            }
        }
        let Some(snd) = self.sounds else { return };
        let ear = Listener::from_camera(&self.camera());
        self.foley.update(dt, ear);
        for (i, (l, &(pos, turn, phase))) in self.legs.iter_mut().zip(places).enumerate() {
            let h = pos.y;
            let (speed, turn_rate) = match l.last {
                Some((p, a)) if dt > 0.0 => (vec3(pos.x - p.x, 0.0, pos.z - p.z).length() / dt, (turn - a).abs() / dt),
                _ => (0.0, 0.0),
            };
            l.last = Some((pos, turn));
            let grounded = h < 0.1;
            for _ in l.gait.update(phase) {
                if grounded {
                    self.foley.step(snd.stomp, pos, 0.45);
                }
            }
            if let Some(d) = &mut l.servo {
                d.update(&mut self.foley, turn_rate * 2.0 + speed, pos + vec3(0.0, 2.2, 0.0), dt);
            }
            if let Some(d) = &mut l.jets {
                d.update(&mut self.foley, if thrusting(self.t, i) { 1.0 } else { 0.0 }, pos + vec3(0.0, 1.0, 0.0), dt);
            }
            if let Some(v) = l.feet.update(h, dt) {
                self.foley.impact(snd.stomp, pos, v, 8.0);
                self.foley.impact(snd.impact, pos, v * 0.6, 8.0);
            }
        }
    }

    fn set_look(&self) {
        gfx3d::retro(&if self.retro { Retro::saturn() } else { Retro::default() });
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        for e in input::poll().iter() {
            if e.kind() == Kind::TouchDown {
                self.retro = !self.retro;
                self.set_look();
            }
        }
        if self.floor.is_none() && self.assets.iter().all(|a| a.state() == AssetState::Ready) {
            let tex = |a: Asset| Texture::new(a).map(Texture::nearest);
            let (Some(sky), Some(far), Some(near), Some(ground)) = (tex(self.assets[0]), tex(self.assets[1]), tex(self.assets[2]), tex(self.assets[3])) else { return };
            gfx3d::backdrop(&[
                Layer::new(sky).scroll(0.2).repeat(2.0).at(-0.3).fill_below(),
                Layer::new(far).scroll(0.45).repeat(3.0).at(-0.02).scale(0.8),
                Layer::new(near).scroll(0.75).repeat(4.0).at(-0.03).scale(0.8),
            ]);
            let mat = Material::new(&MaterialDesc { base_color: [1.0; 4], base_tex: Some(ground), roughness: 1.0, flags: Material::UNLIT, ..Default::default() }).expect("material");
            let mesh = Mesh::new(&MeshData::mode7_floor(160.0, TILE, 40)).expect("mesh");
            self.floor = Node::with_mesh(mesh, mat);
        }
        // The mechs bob and turn toward each other, and hop on their thrusters now and then.
        let mut places = Vec::new();
        for (i, m) in self.mechs.iter().enumerate() {
            let s = if i == 0 { 1.0 } else { -1.0 };
            let x = s * -3.0 + 0.4 * (self.t * 0.9 + i as f32).sin();
            let turn = -1.2 * s + 0.25 * (self.t * 0.7 + i as f32 * 2.0).sin();
            let bob = 0.05 * (self.t * 3.0 + i as f32).sin().abs();
            let pos = vec3(x, bob + hop_height(self.t, i), -2.0);
            m.set_transform(&Transform { pos, rot: Quat::from_rotation_y(turn), ..Default::default() });
            // A foot comes down at each low point of the bob: two a stride.
            let phase = (self.t * 3.0 + i as f32) / (2.0 * std::f32::consts::PI);
            places.push((pos, turn, phase));
        }
        self.foley(dt, &places);
    }

    pub fn camera(&self) -> Camera {
        // A slow orbit, so the backdrop layers scroll at their own rates.
        let a = 0.6 * (self.t * 0.25).sin() + self.t * 0.12;
        let pos = vec3(0.0, 0.0, -2.0) + vec3(a.sin() * 15.0, 3.6, a.cos() * 15.0);
        let mut c = Camera::looking_at(pos, vec3(0.0, 2.2, -2.0));
        c.fov_y = 0.95;
        c.far = 400.0;
        c
    }

    pub fn render(&self) {
        let cam = self.camera();
        if let Some(f) = self.floor {
            f.set_transform(&Transform { pos: mode7_anchor(cam.pos, TILE, 0.0), ..Default::default() });
        }
        gfx3d::camera(&cam);
        gfx3d::render();
        // The HUD at full resolution: crisp over the low-res 3D.
        let w = 360.0;
        for (i, (x, c)) in [(16.0, 0x3a6cd0ffu32), (w - 16.0 - 140.0, 0xc83c3cff)].into_iter().enumerate() {
            gfx2d::rect(x - 2.0, 30.0, 144.0, 16.0, 0x000000ff);
            let hp = 0.6 + 0.35 * ((self.t * 0.5 + i as f32 * 1.7).sin() * 0.5 + 0.5);
            gfx2d::rect(x, 32.0, 140.0 * hp, 12.0, c);
        }
        gfx2d::text(Font::Pixel, 16.0, 16.0, 56.0, 0xffffffff, "BLUE ROOK");
        gfx2d::text(Font::Pixel, 16.0, w - 16.0 - 104.0, 56.0, 0xffffffff, "RED NIBS");
        let label = if self.retro { "SATURN MODE  TAP: OFF" } else { "FULL RES  TAP: SATURN" };
        gfx2d::text(Font::Pixel, 12.0, 16.0, 610.0, 0xffe080ff, label);
    }
}
