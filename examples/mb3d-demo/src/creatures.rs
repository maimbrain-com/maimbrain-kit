//! Generated creatures, alive: three models from `mb model` (a heron, a
//! dragon and a koi, imported with `--spine`, so each has code-driven bones
//! along its body) flying and swimming at dusk. `--param scene=creatures`.
//!
//! - The dragon undulates: `Skeleton::spine` + `Swim` with the wave turned
//!   to bend it up and down, along a smoothed figure-eight.
//! - The heron flies: `Flight` flaps (harder climbing), glides when it can,
//!   banks into the turns, keeps its head level and every 6 s strikes
//!   (rears back, dives with its wings swept back, flares, recovers).
//! - The koi swims side to side with `Swim`, beat following its speed.
//!
//! Every path is a `motion::Follow` of a moving target, never a raw curve,
//! so nothing changes direction instantly. The heron's node handle is shown
//! for `mb shot --smoothness --track-node <it>`.

use maimbrain::gfx2d::{self, Font};
use maimbrain::gfx3d::{
    self, Camera, Flight, Model, Node, Post, Quat, Skeleton, Sky, Sun, Swim, Transform, Vec3, Wave,
    vec3,
};
use maimbrain::motion::Follow;
use maimbrain::sys::{Asset, AssetState};

const STRIKE_EVERY: f32 = 6.0;
const STRIKE_DEPTH: f32 = 2.2;

struct Body {
    node: Node,
    bones: Skeleton,
    path: Follow<Vec3>,
}

pub struct Creatures {
    assets: [Asset; 3],
    heron: Option<Body>,
    dragon: Option<Body>,
    koi: Option<Body>,
    flight: Flight,
    dragon_swim: Swim,
    koi_swim: Swim,
    t: f32,
    next_strike: f32,
}

/// Where each creature is heading at time `t` (the paths follow these).
fn heron_target(t: f32) -> Vec3 {
    let a = t * 0.55;
    vec3(
        3.2 * a.sin(),
        4.4 + 0.9 * (t * 0.8).sin(),
        -3.0 + 2.2 * a.cos(),
    )
}
fn dragon_target(t: f32) -> Vec3 {
    let a = t * 0.42;
    vec3(
        3.4 * a.sin(),
        1.6 + 0.5 * (a * 2.0).sin(),
        -2.0 + 1.8 * (a * 2.0).sin(),
    )
}
fn koi_target(t: f32) -> Vec3 {
    let a = t * 0.9;
    vec3(
        1.1 * a.sin(),
        -0.6 + 0.08 * (t * 1.7).sin(),
        4.0 + 0.8 * a.cos(),
    )
}

fn spawn(asset: Asset, start: Vec3, halflife: f32) -> Option<Body> {
    let node = Model::load(asset)?.spawn(None)?;
    // Read the bones' rest pose right after spawning, before anything moves them.
    let bones = Skeleton::spine(node)?;
    let mut path = Follow::new(start, halflife);
    path.snap(start);
    Some(Body { node, bones, path })
}

impl Creatures {
    pub fn new() -> Creatures {
        gfx3d::sky(&Sky {
            seed: 9,
            colors: [[0.1, 0.12, 0.26], [0.85, 0.42, 0.3], [0.3, 0.26, 0.5]],
            nebula_density: 0.35,
            star_density: 0.25,
            star_brightness: 0.7,
            sun_disc: 0.02,
            ..Default::default()
        });
        gfx3d::sun(&Sun {
            direction: vec3(-0.5, -0.55, -0.65),
            color: [1.0, 0.8, 0.6],
            intensity: 3.4,
            shadows: 0,
            shadow_distance: 20.0,
        });
        gfx3d::post(&Post {
            exposure: 1.2,
            bloom_strength: 0.4,
            bloom_threshold: 1.0,
            vignette: 0.3,
            saturation: 1.08,
            contrast: 1.05,
            ..Default::default()
        });
        let mut dragon_swim = Swim::new(4.5).with_phase(0.7);
        // A flying dragon bends up and down, three-quarters of a wave along its body.
        dragon_swim.wave = Wave {
            axis: Vec3::X,
            amplitude: 0.32,
            head: 0.25,
            waves: 0.75,
        };
        dragon_swim.stride = 0.55;
        let mut koi_swim = Swim::new(0.8);
        koi_swim.wave = Wave {
            amplitude: 0.45,
            head: 0.18,
            waves: 0.85,
            ..Wave::default()
        };
        Creatures {
            assets: [
                Asset::load("assets/heron.glb"),
                Asset::load("assets/dragon.glb"),
                Asset::load("assets/koi2.glb"),
            ],
            heron: None,
            dragon: None,
            koi: None,
            flight: Flight::new(2.4),
            dragon_swim,
            koi_swim,
            t: 0.0,
            next_strike: 2.5,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        let t = self.t;
        let ready = |a: &Asset| a.state() == AssetState::Ready;
        if self.heron.is_none() && ready(&self.assets[0]) {
            self.heron = spawn(self.assets[0], heron_target(t), 0.45);
            if let Some(h) = &self.heron {
                // Sculpted standing with its wings spread: tip it so its head-to-tail line flies level.
                self.flight = Flight::new(2.4).level_with(&h.bones);
                self.flight.flap_hz = 2.2;
            }
        }
        if self.dragon.is_none() && ready(&self.assets[1]) {
            self.dragon = spawn(self.assets[1], dragon_target(t), 0.6);
        }
        if self.koi.is_none() && ready(&self.assets[2]) {
            self.koi = spawn(self.assets[2], koi_target(t), 0.3);
        }
        if let Some(h) = &mut self.heron {
            if t >= self.next_strike {
                self.flight.strike();
                self.next_strike += STRIKE_EVERY;
            }
            let dip = vec3(0.0, self.flight.strike_move.height() * STRIKE_DEPTH, 0.0);
            let pos = h.path.update(heron_target(t) + dip, dt);
            self.flight.update(dt, h.path.velocity);
            h.node.set_transform(
                &Transform::at(pos + self.flight.bob())
                    .with_rot(self.flight.rotation())
                    .with_scale(0.8),
            );
            self.flight.apply(&h.bones);
        }
        if let Some(d) = &mut self.dragon {
            let pos = d.path.update(dragon_target(t), dt);
            let vel = d.path.velocity;
            self.dragon_swim.update(dt, vel.length());
            self.dragon_swim.apply(&d.bones, 0.0);
            d.node.set_transform(
                &Transform::at(pos)
                    .with_rot(Quat::look_rotation(vel, Vec3::Y))
                    .with_scale(0.8),
            );
        }
        if let Some(k) = &mut self.koi {
            let pos = k.path.update(koi_target(t), dt);
            let vel = k.path.velocity;
            self.koi_swim.update(dt, vel.length());
            self.koi_swim.apply(&k.bones, 0.0);
            k.node
                .set_transform(&Transform::at(pos).with_rot(Quat::look_rotation(vel, Vec3::Y)));
        }
    }

    pub fn camera(&self) -> Camera {
        let pos = vec3(0.0, 2.6, 11.5);
        Camera {
            pos,
            rot: gfx3d::look_at(pos, vec3(0.0, 2.0, -1.5), Vec3::Y),
            fov_y: 1.0,
            near: 0.1,
            far: 200.0,
        }
    }

    pub fn render(&self) {
        gfx3d::camera(&self.camera());
        gfx3d::render();
        gfx2d::text_centered(
            Font::SansBold,
            24.0,
            180.0,
            34.0,
            0xffffffff,
            "generated creatures",
        );
        let heron = self.heron.as_ref().map_or(0, |h| h.node.raw());
        let state = if self.flight.strike_move.active() {
            "strike"
        } else if self.flight.flapping() > 0.5 {
            "flap"
        } else {
            "glide"
        };
        gfx2d::text_centered(
            Font::Sans,
            11.0,
            180.0,
            60.0,
            0xffd29aff,
            &format!(
                "heron node {heron} · {state} · bank {:.0}°",
                self.flight.bank().to_degrees()
            ),
        );
    }
}
