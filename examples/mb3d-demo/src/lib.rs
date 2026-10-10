//! mb3d demo: the reference scene for the host 3D engine.
//!
//! A platform floats in a nebula. On it, a 5×5 grid of spheres sweeps
//! metallic (left → right) and roughness (front → back). A faceted ship
//! built in code loops around it with glowing engine trails, explosions go
//! off around the rim (particles, a shockwave, a flash of light), and a
//! glTF relic hovers overhead. Tap anywhere for an explosion there.
//!
//! Tap CANAL (top right) for the second scene, a moonlit canal showing fog,
//! reflective water that reacts and looks like water (water v2), and sky
//! v2 (`canal.rs`, mb3d 2), or KOI (top left) for a koi pond showing
//! organic meshes built in code and skinned animation under clear water
//! (`koi.rs`, mb3d 2); tap the top bar there to come
//! back, and the bottom of the canal to change its sky.
//!
//! `--param scene=canal` or `scene=koi` (`mb shot`, `mb measure`) starts in
//! that scene, for looking at it without tapping there. `scene=creatures`
//! shows generated models (a heron, a dragon, a koi from `mb model --spine`)
//! flying and swimming on code-driven bones (`creatures.rs`); it has no tap
//! to reach it. `scene=retro` shows the retro look: two mechs on a Mode 7
//! floor under SNES-style parallax backdrops, drawn like a Saturn
//! (`retro.rs`); tap there to compare with full resolution.
//! `scene=hero` lights a generated, textured model against a bright sky
//! (`Lighting::hero`, `hero.rs`); add `look=muddy` for the same model lit
//! the way that hides it, or `view=grazing` for a low camera along a
//! textured steel deck (anisotropic filtering; `sharpen=0.4` adds
//! `Post::sharpen`). `scene=mech` is a Tripo-rigged mech posed in code:
//! walking with `MechGait`, a keyed strike with a spring stop, a hit and a
//! part in its hand (`mech.rs`).

//!
//! Everything is a function of game time and taps, so replays match.

mod canal;
mod creatures;
mod hero;
mod koi;
mod mech;
mod retro;
mod ship;

use maimbrain::gfx2d::{self, Font};
use maimbrain::gfx3d::{
    self, Camera, Emitter, EmitterDesc, FxBlend, Material, MaterialDesc, MeshData, Model, Node, Post, Quat, Sky, Sun, Trail, TrailDesc, Transform,
    Vec3, srgb, srgba, vec3,
};
use maimbrain::input::{self, Kind};
use maimbrain::sys::{self, Asset, AssetState, Round};
use maimbrain::Game;

use canal::Canal;
use koi::Pond;
use ship::Slot;

const W: f32 = 360.0;
const H: f32 = 640.0;
const GRID: usize = 5;
const SPACING: f32 = 1.12;
const SPHERE_R: f32 = 0.42;
/// Seconds between scheduled explosions, and when the first goes off.
const BOOM_EVERY: f32 = 2.6;
const BOOM_FIRST: f32 = 1.4;
/// How long the opening camera move lasts.
const INTRO: f32 = 4.0;
/// Taps above this (logical px) switch scenes.
const TOP_BAR: f32 = 96.0;
/// In the canal, taps below this change the sky.
const SKY_BAR: f32 = H - 120.0;

struct Fx {
    sparks: Emitter,
    fire: Emitter,
    smoke: Emitter,
    embers: Emitter,
    flash: Node,
}

/// The demo scene's point lights and how they shine (off while the canal shows).
struct Lamp {
    node: Node,
    color: [f32; 3],
    intensity: f32,
    range: f32,
}

struct Demo {
    time: f32,
    /// Everything in the demo scene hangs under this, so hiding it hides the scene.
    root: Option<Node>,
    lamps: Vec<Lamp>,
    ship: Option<Node>,
    /// Trails and the nodes they follow.
    trails: Vec<(Trail, Node)>,
    fx: Option<Fx>,
    /// The last explosion: when and where (for the flash light).
    last_boom: Option<(f32, Vec3)>,
    next_boom: f32,
    booms: u32,
    relic_asset: Asset,
    relic: Option<Node>,
    labels: [Vec3; 2],
    canal: Option<Canal>,
    /// Seconds since the canal was entered.
    canal_time: f32,
    pond: Option<Pond>,
    /// The generated-creatures scene (`--param scene=creatures` only).
    creatures: Option<creatures::Creatures>,
    /// The retro look (`--param scene=retro` only).
    retro: Option<retro::Retro3>,
    /// A textured hero, lit well or muddily (`--param scene=hero` only).
    hero: Option<hero::Hero>,
    /// A rigged mech posed in code (`--param scene=mech` only).
    mech: Option<mech::MechScene>,
    /// Seconds since the pond was entered.
    pond_time: f32,
}

fn emitter(desc: EmitterDesc) -> Emitter {
    Emitter::new(&desc).expect("emitter")
}

fn material(desc: MaterialDesc) -> Material {
    Material::new(&desc).expect("material")
}

struct Built {
    root: Node,
    lamps: Vec<Lamp>,
    ship: Node,
    trails: Vec<(Trail, Node)>,
    fx: Fx,
}

impl Demo {
    fn set_look() {
        gfx3d::sky(&Sky {
            seed: 7,
            colors: [[0.002, 0.0025, 0.008], [0.05, 0.19, 0.42], [0.55, 0.07, 0.32]],
            nebula_density: 1.2,
            star_density: 0.7,
            star_brightness: 1.2,
            sun_disc: 0.02,
            ..Default::default()
        });
        gfx3d::sun(&Sun { direction: vec3(-0.55, -0.7, -0.45), color: [1.0, 0.93, 0.85], intensity: 3.2, shadows: 1, shadow_distance: 22.0 });
        gfx3d::post(&Post { exposure: 1.1, bloom_strength: 0.7, bloom_threshold: 1.0, vignette: 0.35, chromatic_aberration: 0.25, saturation: 1.08, contrast: 1.05, ..Default::default() });
    }

    fn build_scene() -> Built {
        Demo::set_look();
        let root = Node::new().unwrap();
        let add = |mesh, mat| {
            let n = Node::with_mesh(mesh, mat).unwrap();
            n.set_parent(Some(root));
            n
        };
        let mut lamps = Vec::new();

        // The platform: a beveled disc (lathe) with a glowing rim.
        let disc = MeshData::lathe(&[[0.0, -0.45], [3.7, -0.45], [4.05, -0.22], [4.05, -0.04], [3.95, 0.0], [0.0, 0.0]], 48).flat_shaded();
        let deck = material(MaterialDesc { base_color: [0.11, 0.115, 0.13, 1.0], metallic: 0.35, roughness: 0.5, ..Default::default() });
        add(disc.upload().unwrap(), deck);
        let rim = MeshData::torus(4.02, 0.035, 96, 6);
        let rim_mat = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [0.25, 1.1, 2.2], emissive_strength: 1.1, flags: Material::UNLIT, ..Default::default() });
        let rim_node = add(rim.upload().unwrap(), rim_mat);
        rim_node.set_transform(&Transform::at(vec3(0.0, -0.13, 0.0)));

        // The material grid: metallic left → right, roughness front → back.
        let sphere = MeshData::uv_sphere(SPHERE_R, 48, 32).upload().unwrap();
        let base = srgb(0xe9a46f);
        for row in 0..GRID {
            for col in 0..GRID {
                let m = material(MaterialDesc {
                    base_color: [base[0], base[1], base[2], 1.0],
                    metallic: col as f32 / (GRID - 1) as f32,
                    roughness: (row as f32 / (GRID - 1) as f32).max(0.03),
                    ..Default::default()
                });
                let n = add(sphere, m);
                let (x, z) = ((col as f32 - 2.0) * SPACING, (2.0 - row as f32) * SPACING);
                n.set_transform(&Transform::at(vec3(x, SPHERE_R, z)));
            }
        }

        // Two colored lights on the rim show off point lighting on the spheres.
        for (pos, color) in [(vec3(-3.6, 0.6, 1.6), srgb(0xff3d9a)), (vec3(3.6, 0.6, -1.4), srgb(0x3dd8ff))] {
            let n = Node::new().unwrap();
            n.set_parent(Some(root));
            n.set_transform(&Transform::at(pos));
            n.point_light(color, 9.0, 6.0);
            lamps.push(Lamp { node: n, color, intensity: 9.0, range: 6.0 });
        }

        // The ship: a parent node with one child per part.
        let hull = material(MaterialDesc { base_color: srgba(0xc9d1dcff), metallic: 0.7, roughness: 0.3, ..Default::default() });
        let accent = material(MaterialDesc { base_color: srgba(0xe63946ff), metallic: 0.2, roughness: 0.4, ..Default::default() });
        let glass = material(MaterialDesc { base_color: srgba(0x1b2a4aff), metallic: 0.1, roughness: 0.05, emissive: [0.05, 0.25, 0.6], emissive_strength: 1.0, ..Default::default() });
        let dark = material(MaterialDesc { base_color: srgba(0x2b2f36ff), metallic: 0.9, roughness: 0.35, ..Default::default() });
        let glow = material(MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [0.4, 1.6, 4.0], emissive_strength: 3.0, flags: Material::UNLIT, ..Default::default() });
        let ship = Node::new().unwrap();
        ship.set_parent(Some(root));
        // `mb shot --readability` checks the ship stands out from the nebula.
        ship.mark_subject();
        for part in ship::parts() {
            let mat = match part.material {
                Slot::Hull => hull,
                Slot::Accent => accent,
                Slot::Glass => glass,
                Slot::Dark => dark,
                Slot::Glow => glow,
            };
            let n = Node::with_mesh(part.mesh.upload().unwrap(), mat).unwrap();
            n.set_parent(Some(ship));
        }
        let engine_light = Node::new().unwrap();
        engine_light.set_parent(Some(ship));
        engine_light.set_transform(&Transform::at(vec3(0.0, 0.0, 1.4)));
        engine_light.point_light([0.3, 0.8, 2.0], 5.0, 3.5);
        lamps.push(Lamp { node: engine_light, color: [0.3, 0.8, 2.0], intensity: 5.0, range: 3.5 });
        let trail = TrailDesc {
            color_start: [1.2, 3.2, 8.0, 1.0],
            color_end: [0.6, 0.2, 2.0, 0.0],
            width_start: 0.22,
            width_end: 0.02,
            lifetime: 1.1,
            blend: FxBlend::Additive,
            min_segment: 0.04,
            ..Default::default()
        };
        let mut trails = Vec::new();
        for p in ship::EXHAUSTS {
            let anchor = Node::new().unwrap();
            anchor.set_parent(Some(ship));
            anchor.set_transform(&Transform::at(p));
            let t = Trail::new(&trail).unwrap();
            t.attach(anchor);
            trails.push((t, anchor));
        }

        let fx = Fx {
            sparks: emitter(EmitterDesc {
                colors: [[9.0, 6.0, 2.5, 1.0], [6.0, 2.2, 0.5, 1.0], [2.5, 0.5, 0.1, 0.8], [0.4, 0.05, 0.0, 0.0]],
                sizes: [0.07, 0.06, 0.045, 0.02],
                lifetime: [0.5, 1.2],
                speed: [4.0, 11.0],
                spread: std::f32::consts::PI,
                drag: 1.6,
                gravity: vec3(0.0, -3.5, 0.0),
                stretch: 0.045,
                ..Default::default()
            }),
            fire: emitter(EmitterDesc {
                colors: [[4.5, 2.6, 1.0, 1.0], [2.6, 0.8, 0.2, 0.75], [0.7, 0.14, 0.05, 0.35], [0.0, 0.0, 0.0, 0.0]],
                sizes: [0.5, 1.3, 1.7, 1.9],
                lifetime: [0.35, 0.7],
                speed: [0.4, 2.6],
                drag: 2.5,
                gravity: vec3(0.0, 0.6, 0.0),
                ..Default::default()
            }),
            smoke: emitter(EmitterDesc {
                colors: [[0.2, 0.12, 0.08, 0.0], [0.13, 0.09, 0.09, 0.5], [0.07, 0.06, 0.08, 0.3], [0.04, 0.04, 0.05, 0.0]],
                sizes: [0.6, 1.3, 1.9, 2.4],
                lifetime: [1.4, 2.4],
                speed: [0.3, 1.4],
                drag: 1.2,
                blend: FxBlend::Alpha,
                gravity: vec3(0.0, 0.35, 0.0),
                ..Default::default()
            }),
            embers: emitter(EmitterDesc {
                colors: [[5.0, 2.5, 0.8, 1.0], [3.0, 1.0, 0.2, 1.0], [1.5, 0.3, 0.05, 1.0], [0.2, 0.02, 0.0, 0.0]],
                sizes: [0.05, 0.05, 0.04, 0.0],
                lifetime: [1.5, 2.6],
                speed: [1.0, 4.0],
                drag: 0.6,
                gravity: vec3(0.0, -1.2, 0.0),
                ..Default::default()
            }),
            flash: {
                let n = Node::new().unwrap();
                n.set_parent(Some(root));
                n.point_light([0.0; 3], 0.0, 8.0);
                n
            },
        };
        Built { root, lamps, ship, trails, fx }
    }

    fn in_canal(&self) -> bool {
        self.canal.as_ref().is_some_and(|c| c.shown)
    }

    fn in_pond(&self) -> bool {
        self.pond.as_ref().is_some_and(|p| p.shown)
    }

    /// Hides the demo scene (its lights and trails too) for another scene.
    fn hide_demo(&mut self) {
        let Some(root) = self.root else { return };
        root.set_visible(false);
        for l in &self.lamps {
            l.node.point_light(l.color, 0.0, l.range);
        }
        if let Some(fx) = &self.fx {
            fx.flash.point_light([0.0; 3], 0.0, 8.0);
        }
        for (t, _) in &self.trails {
            t.detach();
        }
    }

    /// Shows the demo scene again.
    fn show_demo(&mut self) {
        let Some(root) = self.root else { return };
        root.set_visible(true);
        Demo::set_look();
        for l in &self.lamps {
            l.node.point_light(l.color, l.intensity, l.range);
        }
        for (t, anchor) in &self.trails {
            t.attach(*anchor);
        }
    }

    /// Shows the koi pond (building it the first time) or the demo scene again.
    fn switch_pond(&mut self) {
        if self.in_pond() {
            if let Some(p) = &mut self.pond {
                p.leave();
            }
            self.show_demo();
            return;
        }
        self.hide_demo();
        let pond = self.pond.get_or_insert_with(Pond::build);
        self.pond_time = 0.0;
        pond.update(0.0);
        pond.enter();
    }

    /// Shows the canal (building it the first time) or the demo scene again.
    fn switch_scene(&mut self) {
        if self.in_canal() {
            if let Some(c) = &mut self.canal {
                c.leave();
            }
            self.show_demo();
            return;
        }
        self.hide_demo();
        let canal = self.canal.get_or_insert_with(Canal::build);
        self.canal_time = 0.0;
        canal.update(0.0);
        canal.enter();
    }

    /// The camera: an opening pull-back and rise over INTRO seconds, then a slow sway.
    fn camera(&self) -> Camera {
        let t = self.time;
        let k = (t / INTRO).min(1.0);
        let ease = 1.0 - (1.0 - k) * (1.0 - k) * (1.0 - k);
        let dist = 7.5 + 4.6 * ease;
        let yaw = 0.55 * (1.0 - ease) + 0.22 * (t * 0.13).sin();
        let pitch = 0.14 + 0.3 * ease + 0.03 * (t * 0.21).sin();
        let target = vec3(0.0, 0.7, 0.0);
        let pos = target + vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * dist;
        Camera { pos, rot: gfx3d::look_at(pos, target, Vec3::Y), fov_y: 0.95, near: 0.1, far: 200.0 }
    }

    fn boom(&mut self, at: Vec3) {
        let Some(fx) = &self.fx else { return };
        fx.fire.emit(at, Vec3::ZERO, 36);
        fx.sparks.emit(at, Vec3::ZERO, 170);
        fx.smoke.emit(at, Vec3::ZERO, 26);
        fx.embers.emit(at, Vec3::Y, 40);
        fx.flash.set_transform(&Transform::at(at));
        gfx3d::shockwave(at, 4.5, 0.012, 0.9);
        // `mb shot --at mark:boom` shoots the moment one goes off.
        sys::mark("boom");
        self.last_boom = Some((self.time, at));
        self.booms += 1;
    }

    /// The scheduled explosion positions: around the rim, alternating heights.
    fn boom_spot(k: u32) -> Vec3 {
        let a = k as f32 * 2.399_963 + 0.6;
        vec3(a.sin() * 4.6, 0.7 + 0.8 * ((k % 3) as f32 / 2.0), a.cos() * 3.4 - 0.8)
    }

    /// The point on the plane y = 1 under a tap, through the camera.
    fn tap_point(&self, x: f32, y: f32) -> Vec3 {
        let c = self.camera();
        let ty = (c.fov_y * 0.5).tan();
        let (nx, ny) = ((x / W) * 2.0 - 1.0, 1.0 - (y / H) * 2.0);
        let dir = (c.rot.forward() + c.rot.right() * (nx * ty * W / H) + c.rot.up() * (ny * ty)).normalize();
        let t = if dir.y.abs() > 1e-3 { ((1.0 - c.pos.y) / dir.y).clamp(0.5, 30.0) } else { 12.0 };
        c.pos + dir * t
    }
}

impl Game for Demo {
    fn init() -> Self {
        let b = Demo::build_scene();
        sys::round(Round::Idle);
        let mut demo = Demo {
            time: 0.0,
            root: Some(b.root),
            lamps: b.lamps,
            ship: Some(b.ship),
            trails: b.trails,
            fx: Some(b.fx),
            last_boom: None,
            next_boom: BOOM_FIRST,
            booms: 0,
            relic_asset: Asset::load("assets/relic.glb"),
            relic: None,
            labels: [vec3(0.0, 0.0, 3.2), vec3(-3.25, 0.0, 0.0)],
            canal: None,
            canal_time: 0.0,
            pond: None,
            creatures: None,
            hero: None,
            mech: None,
            retro: None,
            pond_time: 0.0,
        };
        // Previews can start in another scene: `mb shot games/mb3d-demo --param scene=koi`
        // (never set on a device, so the feed always opens on the demo).
        match sys::param("scene").as_deref() {
            Some("canal") => demo.switch_scene(),
            Some("koi") => demo.switch_pond(),
            Some("creatures") => {
                demo.hide_demo();
                demo.creatures = Some(creatures::Creatures::new());
            }
            Some("retro") => {
                demo.hide_demo();
                demo.retro = Some(retro::Retro3::new());
            }
            Some("hero") => {
                demo.hide_demo();
                demo.hero = Some(hero::Hero::new());
            }
            Some("mech") => {
                demo.hide_demo();
                demo.mech = Some(mech::MechScene::new());
            }
            _ => {}
        }
        demo
    }

    fn update(&mut self, dt: f32) {
        if let Some(c) = &mut self.creatures {
            c.update(dt);
            return;
        }
        if let Some(r) = &mut self.retro {
            r.update(dt);
            return;
        }
        if let Some(h) = &mut self.hero {
            h.update(dt);
            return;
        }
        if let Some(m) = &mut self.mech {
            m.update(dt);
            return;
        }
        let taps: Vec<(f32, f32)> = input::poll().iter().filter(|e| e.kind() == Kind::TouchDown).map(|e| (e.x, e.y)).collect();
        for &(x, y) in &taps {
            if y < TOP_BAR && (self.in_pond() || (!self.in_canal() && x < W / 2.0)) {
                self.switch_pond();
            } else if self.in_pond() {
                if let Some(p) = &mut self.pond {
                    p.cycle_look();
                }
            } else if y < TOP_BAR {
                self.switch_scene();
            } else if self.in_canal() {
                if let Some(c) = &mut self.canal {
                    if y > SKY_BAR { c.cycle_sky() } else { c.cycle_look() }
                }
            }
        }
        if self.in_pond() {
            self.pond_time += dt;
            if let Some(p) = &mut self.pond {
                p.update(self.pond_time);
            }
            return;
        }
        if self.in_canal() {
            self.canal_time += dt;
            if let Some(c) = &mut self.canal {
                c.update(self.canal_time);
            }
            return;
        }
        self.time += dt;
        for (x, y) in taps {
            if y >= TOP_BAR {
                let p = self.tap_point(x, y);
                self.boom(p);
            }
        }
        while self.time >= self.next_boom {
            let p = Demo::boom_spot(self.booms);
            self.boom(p);
            self.next_boom += BOOM_EVERY;
        }

        if let Some(ship) = self.ship {
            let (pos, rot) = ship::flight(self.time);
            ship.set_transform(&Transform { pos, rot, scale: Vec3::splat(0.8) });
        }
        if let Some(fx) = &self.fx {
            let i = match self.last_boom {
                Some((t, _)) => 90.0 * (-(self.time - t) * 7.0).exp(),
                None => 0.0,
            };
            fx.flash.point_light([1.0, 0.55, 0.2], if i > 0.05 { i } else { 0.0 }, 9.0);
        }

        // The glTF relic appears once its asset has loaded.
        if self.relic.is_none() && self.relic_asset.state() == AssetState::Ready {
            self.relic = Model::load(self.relic_asset).and_then(|m| m.spawn(self.root));
        }
        if let Some(r) = self.relic {
            let t = self.time;
            r.set_transform(&Transform { pos: vec3(0.0, 2.75 + 0.12 * (t * 1.3).sin(), -0.4), rot: Quat::from_rotation_y(t * 0.5) * Quat::from_rotation_x(0.35), scale: Vec3::splat(0.55) });
        }
    }

    fn render(&self) {
        if let Some(c) = &self.creatures {
            c.render();
            return;
        }
        if let Some(r) = &self.retro {
            r.render();
            return;
        }
        if let Some(h) = &self.hero {
            h.render();
            return;
        }
        if let Some(m) = &self.mech {
            m.render();
            return;
        }
        if let Some(p) = self.pond.as_ref().filter(|p| p.shown) {
            gfx3d::camera(&p.camera(self.pond_time));
            gfx3d::render();
            let a = (((self.pond_time - 0.2) / 0.8).clamp(0.0, 1.0) * 255.0) as u32;
            gfx2d::text_centered(Font::SansBold, 26.0, W / 2.0, 34.0, 0xffffff00 | a, "koi pond");
            gfx2d::text_centered(Font::Sans, 11.0, W / 2.0, 64.0, 0xffd29a00 | (a * 3 / 4), p.look.label());
            gfx2d::text_centered(Font::Sans, 11.0, W / 2.0, H - 28.0, 0xffffff00 | (a / 2), "tap: compare  ·  top: back to the demo");
            return;
        }
        if let Some(c) = self.canal.as_ref().filter(|c| c.shown) {
            gfx3d::camera(&c.camera(self.canal_time));
            gfx3d::render();
            let a = (((self.canal_time - 0.2) / 0.8).clamp(0.0, 1.0) * 255.0) as u32;
            gfx2d::text_centered(Font::SansBold, 26.0, W / 2.0, 34.0, 0xffffff00 | a, "moonlit canal");
            gfx2d::text_centered(Font::Sans, 11.0, W / 2.0, 64.0, 0xffd29a00 | (a * 3 / 4), c.look.label());
            gfx2d::text_centered(Font::Sans, 11.0, W / 2.0, 80.0, 0x9fd8ff00 | (a * 3 / 4), c.sky.label());
            gfx2d::text_centered(Font::Sans, 11.0, W / 2.0, H - 28.0, 0xffffff00 | (a / 2), "tap: compare looks  ·  bottom: sky  ·  top: back");
            return;
        }
        gfx3d::camera(&self.camera());
        gfx3d::render();

        // HUD (mb2d over the 3D image), with labels placed by projecting world points.
        let fade = ((self.time - 0.3) / 0.8).clamp(0.0, 1.0);
        let a = (fade * 255.0) as u32;
        gfx2d::text_centered(Font::SansBold, 12.0, W - 44.0, 34.0, 0xffd29a00 | (a * 3 / 4), "CANAL >");
        gfx2d::text_centered(Font::SansBold, 12.0, 38.0, 34.0, 0xffd29a00 | (a * 3 / 4), "< KOI");
        gfx2d::text_centered(Font::SansBold, 30.0, W / 2.0, 34.0, 0xffffff00 | a, "mb3d");
        gfx2d::text_centered(Font::Sans, 12.0, W / 2.0, 70.0, 0x9fd8ff00 | (a * 3 / 4), "PBR · IBL · SHADOWS · BLOOM · PARTICLES · TRAILS · glTF");
        let label = |p: Vec3, s: &str| {
            let q = gfx3d::project(p);
            if q.on_screen {
                gfx2d::text_centered(Font::Sans, 11.0, q.x, q.y, 0xffffff00 | (a * 2 / 3), s);
            }
        };
        label(self.labels[0], "metallic  →");
        label(self.labels[1], "rough ↑");
    }
}

maimbrain::export_game!(Demo);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosions_follow_the_schedule() {
        let mut d = Demo::init();
        for _ in 0..(60 * 10) {
            d.update(1.0 / 60.0);
        }
        // 10 s: booms at 1.4, 4.0, 6.6, 9.2.
        assert_eq!(d.booms, 4);
        let spots: Vec<Vec3> = (0..8).map(Demo::boom_spot).collect();
        assert!(spots.iter().all(|p| p.length() > 3.0 && p.length() < 6.0));
    }

    #[test]
    fn the_canal_comes_and_goes_without_stopping_the_demo() {
        let mut d = Demo::init();
        d.switch_scene();
        assert!(d.in_canal());
        for _ in 0..120 {
            d.update(1.0 / 60.0);
        }
        assert!((d.canal_time - 2.0).abs() < 1e-3 && d.time == 0.0, "the demo's clock waits");
        d.switch_scene();
        assert!(!d.in_canal());
        for _ in 0..(60 * 3) {
            d.update(1.0 / 60.0);
        }
        assert_eq!(d.booms, 1, "the schedule starts when the demo does (the first at 1.4 s)");
    }

    #[test]
    fn taps_land_on_the_plane() {
        let mut d = Demo::init();
        d.time = 10.0;
        let p = d.tap_point(W / 2.0, H * 0.6);
        assert!((p.y - 1.0).abs() < 1e-3 && p.length() < 10.0, "{p:?}");
    }
}
