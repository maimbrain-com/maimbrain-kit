//! The forest, built once and moved every frame. What it shows off (mb3d 2):
//! the skinned, animated fox (tools/make_fox.py) restyled with toon shading
//! and an outline, a lantern hung from its tail socket, instanced trees,
//! grass and glowing mushrooms in chunks that leapfrog ahead, instanced
//! coins rebuilt every frame, exponential ground fog, world-space and
//! billboard 3D text, depth of field focused on the fox, and the beauty
//! pass (a moonlit grade, moonbeams through the trees, a halo around the
//! lantern, film grain).

use maimbrain::gfx2d::Font;
use maimbrain::gfx3d::{
    self, Animator, Clip, DepthOfField, Emitter, EmitterDesc, Film, Fog, FogMode, FxBlend, Grade, Halos, Instance, LightShafts, Material, MaterialDesc, MaterialStyle, MeshData, Model, Node,
    Play, Post, Quat, Sky, Sun, TextDesc, Transform, Vec3, srgb, srgba, vec3,
};
use maimbrain::sys::{Asset, AssetState};
use maimbrain::Rng;

use crate::sim::{self, Sim};

/// Trail meters per chunk, and chunks alive at once (from just behind the camera into the fog).
pub const CHUNK: f32 = 16.0;
pub const CHUNKS: usize = 7;
const LOGS: usize = 10;
const POPS: usize = 6;

struct Chunk {
    root: Node,
    trees: Node,
    grass: Node,
    shrooms: Node,
    /// Which slab of trail it shows (`index × CHUNK` meters on), or −1 before the first layout.
    index: i64,
}

struct Fox {
    root: Node,
    anim: Animator,
    idle: Clip,
    run: Clip,
    jump: Clip,
}

struct Pop {
    node: Node,
    born: f32,
    /// Where it rose from, in trail meters / sideways / height.
    at: (f32, f32, f32),
}

pub struct World {
    fox_asset: Asset,
    fox: Option<Fox>,
    lantern: Node,
    chunks: Vec<Chunk>,
    logs: Vec<Node>,
    coins: Node,
    coin_list: Vec<Instance>,
    /// (board, label) and the distance it shows.
    signs: [(Node, Node, u32); 2],
    title: Node,
    pops: Vec<Pop>,
    next_pop: usize,
    fireflies: Emitter,
    sparkle: Emitter,
    dust: Emitter,
    next_fly: f32,
    rng: Rng,
}

fn mat(desc: MaterialDesc, style: MaterialStyle) -> Material {
    let m = Material::new(&desc).expect("material");
    m.set_style(&style);
    m
}

/// Ink outlines in the forest's darkest blue.
const INK: [f32; 3] = [0.012, 0.016, 0.03];

fn toon(outline: f32, rim: f32) -> MaterialStyle {
    MaterialStyle { outline_color: INK, outline_width: outline, toon_bands: 3, rim, ..Default::default() }
}

fn tree_mesh() -> MeshData {
    let mut m = MeshData::new();
    m.merge(&MeshData::cylinder(0.11, 1.3, 7).with_vertex_colors(0x3a2618ff).transformed(&Transform::at(vec3(0.0, 0.65, 0.0)).matrix()));
    for (y, r, h, c) in [(1.6, 1.05, 1.5, 0x173a2cff), (2.35, 0.82, 1.25, 0x1d4733ff), (3.0, 0.56, 1.0, 0x24553bff)] {
        m.merge(&MeshData::cone(r, h, 8).with_vertex_colors(c).transformed(&Transform::at(vec3(0.0, y, 0.0)).matrix()));
    }
    m.flat_shaded()
}

fn grass_mesh() -> MeshData {
    // Three crossed blades, dark at the root and pale at the tip (double-sided material).
    let mut m = MeshData::new();
    m.colors = Vec::new();
    for k in 0..3 {
        let a = k as f32 * 2.1;
        let (s, c) = a.sin_cos();
        let n = vec3(c, 0.0, -s);
        let base = m.vertex_count() as u32;
        m.push(vec3(-0.06 * s, 0.0, -0.06 * c), n, [0.0, 1.0]);
        m.push(vec3(0.06 * s, 0.0, 0.06 * c), n, [1.0, 1.0]);
        m.push(vec3(0.12 * c, 0.42, -0.12 * s), n, [0.5, 0.0]);
        m.tri(base, base + 1, base + 2);
    }
    m.with_vertex_colors_by(|p, _| if p.y > 0.2 { 0x6f9a6aff } else { 0x1b3324ff })
}

fn shroom_mesh() -> MeshData {
    let mut m = MeshData::new();
    m.merge(&MeshData::cylinder(0.035, 0.16, 6).with_vertex_colors(0xd9d2c3ff).transformed(&Transform::at(vec3(0.0, 0.08, 0.0)).matrix()));
    let mut cap = MeshData::uv_sphere(0.11, 10, 6);
    for p in &mut cap.positions {
        p.y = p.y.max(0.0) * 0.7;
    }
    m.merge(&cap.with_vertex_colors(0xffffffff).transformed(&Transform::at(vec3(0.0, 0.15, 0.0)).matrix()));
    m
}

fn ground_mesh() -> MeshData {
    // 40 m wide, CHUNK long (local z from 0 to −CHUNK), a pale dirt path down the middle.
    let mut m = MeshData::plane(40.0, CHUNK);
    for p in &mut m.positions {
        p.z -= CHUNK / 2.0;
    }
    let mut m = m.with_vertex_colors(0x22362bff);
    let path = MeshData::plane(1.5, CHUNK).with_vertex_colors(0x5a4a3cff).transformed(&Transform::at(vec3(0.0, 0.01, -CHUNK / 2.0)).matrix());
    m.merge(&path);
    m
}

fn sign_mesh() -> MeshData {
    let mut m = MeshData::cuboid(vec3(0.1, 1.3, 0.1)).with_vertex_colors(0x4a3220ff).transformed(&Transform::at(vec3(0.0, 0.65, 0.0)).matrix());
    m.merge(&MeshData::cuboid(vec3(1.2, 0.5, 0.06)).with_vertex_colors(0x6b4a2eff).transformed(&Transform::at(vec3(0.0, 1.2, 0.05)).matrix()));
    m
}

impl World {
    pub fn build(seed: u64) -> World {
        gfx3d::sky(&Sky {
            seed: 3,
            colors: [[0.012, 0.02, 0.045], [0.07, 0.13, 0.22], [0.12, 0.07, 0.2]],
            nebula_density: 0.7,
            star_density: 0.45,
            star_brightness: 0.8,
            sun_disc: 0.035,
            ..Default::default()
        });
        // The moon, low ahead: the scene is backlit, so outlines and rims read.
        gfx3d::sun(&Sun { direction: vec3(-0.25, -0.42, 0.87), color: [0.62, 0.72, 1.0], intensity: 2.8, shadows: 1, shadow_distance: 22.0 });
        gfx3d::post(&Post { exposure: 1.25, bloom_strength: 0.85, bloom_threshold: 0.9, vignette: 0.4, chromatic_aberration: 0.1, saturation: 1.08, contrast: 1.04, ..Default::default() });
        gfx3d::fog(&Fog { color: [0.05, 0.075, 0.12], mode: FogMode::Exponential, density: 0.045, height: 0.0, height_falloff: 0.22, sky: 0.9, ..Default::default() });
        // The beauty pass: a moonlit grade, moonbeams slanting through the
        // trees into the mist, the fox's lantern glowing in it, a little grain.
        gfx3d::grade(&Grade { amount: 0.85, ..Grade::moonlit() });
        gfx3d::shafts(&LightShafts::moonbeams());
        gfx3d::halos(&Halos { reach: 1.5, ..Halos::mist() });
        gfx3d::film(&Film { streaks: 0.0, ..Film::subtle() });

        let tree = tree_mesh().upload().unwrap();
        let grass = grass_mesh().upload().unwrap();
        let shroom = shroom_mesh().upload().unwrap();
        let ground = ground_mesh().upload().unwrap();
        let tree_mat = mat(MaterialDesc { roughness: 0.9, ..Default::default() }, toon(1.4, 0.25));
        let grass_mat = mat(MaterialDesc { roughness: 0.9, flags: Material::DOUBLE_SIDED, ..Default::default() }, toon(0.0, 0.0));
        // Caps glow; each instance's color tints the glow.
        let shroom_mat = mat(MaterialDesc { roughness: 0.6, emissive: [1.0, 1.0, 1.0], emissive_strength: 2.2, ..Default::default() }, toon(1.0, 0.0));
        let ground_mat = mat(MaterialDesc { roughness: 1.0, ..Default::default() }, toon(0.0, 0.0));
        let chunks = (0..CHUNKS)
            .map(|_| {
                let root = Node::new().unwrap();
                let child = |mesh, mat| {
                    let n = Node::with_mesh(mesh, mat).unwrap();
                    n.set_parent(Some(root));
                    n
                };
                child(ground, ground_mat);
                Chunk { root, trees: child(tree, tree_mat), grass: child(grass, grass_mat), shrooms: child(shroom, shroom_mat), index: -1 }
            })
            .collect();

        // Logs: toon bark with a thick outline, one node each (a pool).
        let log_mesh = MeshData::cylinder(0.135, 1.7, 10).transformed(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).matrix()).with_vertex_colors(0x5b3b24ff).upload().unwrap();
        let log_mat = mat(MaterialDesc { roughness: 0.85, ..Default::default() }, toon(2.2, 0.35));
        let logs = (0..LOGS)
            .map(|_| {
                let n = Node::with_mesh(log_mesh, log_mat).unwrap();
                n.set_visible(false);
                n
            })
            .collect();

        // Coins: one instanced node, rebuilt every frame (they spin).
        let coin_mesh = MeshData::cylinder(0.2, 0.045, 18).upload().unwrap();
        let coin_mat = mat(
            MaterialDesc { base_color: srgba(0xffc94aff), metallic: 1.0, roughness: 0.3, emissive: srgb(0xffa21f), emissive_strength: 0.9, ..Default::default() },
            MaterialStyle { toon_bands: 2, rim: 0.6, outline_color: [0.12, 0.06, 0.0], outline_width: 1.2, ..Default::default() },
        );
        let coins = Node::with_mesh(coin_mesh, coin_mat).unwrap();

        // Distance signs (world-space text on the board) and the title board.
        let sign = sign_mesh().upload().unwrap();
        let sign_mat = mat(MaterialDesc { roughness: 0.8, ..Default::default() }, toon(1.8, 0.3));
        let board = |text: &str, size: f32| {
            let n = Node::with_mesh(sign, sign_mat).unwrap();
            let label = Node::new().unwrap();
            label.set_parent(Some(n));
            label.set_transform(&Transform::at(vec3(0.0, 1.2, 0.09)));
            label.set_text(&TextDesc::new(Font::SansBold, size).with_color([1.6, 1.35, 0.9, 1.0]).with_outline([0.05, 0.03, 0.02, 1.0], 0.07), text);
            (n, label)
        };
        let (s1, l1) = board("100 m", 0.3);
        let (s2, l2) = board("200 m", 0.3);
        let signs = [(s1, l1, 100), (s2, l2, 200)];
        let (title, _) = board("MISTWOOD", 0.22);
        title.set_transform(&Transform::at(vec3(-1.4, 0.0, -1.5)).with_rot(Quat::from_rotation_y(0.9)).with_scale(1.25));

        // "+10" pops: billboards a fixed size on screen, rising and fading.
        let pops = (0..POPS)
            .map(|_| {
                let node = Node::new().unwrap();
                node.set_visible(false);
                Pop { node, born: -10.0, at: (0.0, 0.0, 0.0) }
            })
            .collect();

        // The lantern: hung from the fox's tail socket once the model has loaded.
        let lantern = Node::new().unwrap();
        let cage = Node::with_mesh(
            MeshData::torus(0.045, 0.008, 12, 4).transformed(&Transform::at(vec3(0.0, 0.05, 0.0)).matrix()).upload().unwrap(),
            mat(MaterialDesc { base_color: srgba(0x2a2f38ff), metallic: 0.8, roughness: 0.4, ..Default::default() }, toon(1.0, 0.0)),
        )
        .unwrap();
        cage.set_parent(Some(lantern));
        cage.set_transform(&Transform::at(vec3(0.0, -0.09, 0.0)));
        let glow = Node::with_mesh(
            MeshData::icosphere(0.04, 1).upload().unwrap(),
            Material::new(&MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [3.0, 1.7, 0.6], emissive_strength: 2.5, flags: Material::UNLIT, ..Default::default() }).unwrap(),
        )
        .unwrap();
        glow.set_parent(Some(lantern));
        glow.set_transform(&Transform::at(vec3(0.0, -0.09, 0.0)));
        let light = Node::new().unwrap();
        light.set_parent(Some(lantern));
        light.set_transform(&Transform::at(vec3(0.0, -0.09, 0.0)));
        light.point_light([1.0, 0.62, 0.3], 2.2, 4.5);
        lantern.set_visible(false);

        let fireflies = Emitter::new(&EmitterDesc {
            colors: [[0.0, 0.0, 0.0, 0.0], [1.8, 2.4, 0.6, 1.0], [1.4, 2.0, 0.4, 1.0], [0.0, 0.0, 0.0, 0.0]],
            sizes: [0.05, 0.06, 0.06, 0.04],
            lifetime: [2.5, 4.0],
            speed: [0.1, 0.4],
            drag: 0.3,
            gravity: vec3(0.0, 0.08, 0.0),
            ..Default::default()
        })
        .unwrap();
        let sparkle = Emitter::new(&EmitterDesc {
            colors: [[6.0, 4.5, 1.6, 1.0], [3.0, 1.8, 0.4, 1.0], [1.0, 0.5, 0.1, 0.6], [0.0, 0.0, 0.0, 0.0]],
            sizes: [0.07, 0.06, 0.04, 0.0],
            lifetime: [0.3, 0.6],
            speed: [1.5, 3.5],
            drag: 3.0,
            gravity: vec3(0.0, -2.0, 0.0),
            stretch: 0.03,
            ..Default::default()
        })
        .unwrap();
        let dust = Emitter::new(&EmitterDesc {
            colors: [[0.25, 0.22, 0.2, 0.0], [0.2, 0.18, 0.17, 0.55], [0.12, 0.12, 0.14, 0.3], [0.08, 0.08, 0.1, 0.0]],
            sizes: [0.3, 0.6, 0.9, 1.1],
            lifetime: [0.8, 1.3],
            speed: [0.4, 1.4],
            drag: 2.0,
            blend: FxBlend::Alpha,
            gravity: vec3(0.0, 0.2, 0.0),
            ..Default::default()
        })
        .unwrap();

        World {
            fox_asset: Asset::load("assets/fox.glb"),
            fox: None,
            lantern,
            chunks,
            logs,
            coins,
            coin_list: Vec::new(),
            signs,
            title,
            pops,
            next_pop: 0,
            fireflies,
            sparkle,
            dust,
            next_fly: 0.0,
            rng: Rng::new(seed),
        }
    }

    /// Spawns the fox once its asset has arrived (one model per frame, in update).
    fn load_fox(&mut self) {
        if self.fox.is_some() || self.fox_asset.state() != AssetState::Ready {
            return;
        }
        let Some(model) = Model::load(self.fox_asset) else { return };
        let Some(root) = model.spawn(None) else { return };
        // The model faces +z; the trail runs toward −z.
        root.set_transform(&Transform::at(Vec3::ZERO).with_rot(Quat::from_rotation_y(std::f32::consts::PI)));
        // Its glTF material, restyled: cel bands, an ink outline, a moonlit rim.
        if let Some(m) = root.find("Fox").and_then(Node::material) {
            m.set_style(&MaterialStyle { outline_color: [0.03, 0.012, 0.005], outline_width: 1.8, toon_bands: 3, rim: 0.5, flags: MaterialStyle::NO_FOG, ..Default::default() });
        }
        // Attaching is parenting: the lantern swings with the tail.
        self.lantern.set_parent(root.find("Socket.Tail"));
        self.lantern.set_visible(true);
        let clip = |name| model.clip(name).unwrap_or(Clip { index: 0, duration: 1.0 });
        let mut anim = Animator::new();
        let idle = clip("Idle");
        anim.play(idle, Play::Loop, 0.0);
        self.fox = Some(Fox { root, anim, idle, run: clip("Run"), jump: clip("Jump") });
    }

    /// Lays out a chunk's trees, grass and mushrooms for its slab of trail.
    /// Seeded by the slab, so the same stretch of forest looks the same every run.
    fn layout(c: &mut Chunk, index: i64) {
        c.index = index;
        let mut r = Rng::new(0x5eed_0000 ^ index as u64);
        let mut trees = Vec::new();
        for side in [-1.0f32, 1.0] {
            for _ in 0..20 {
                let x = side * (2.8 + r.range(0.0, 1.0).powf(1.6) * 12.0);
                let z = -r.range(0.0, CHUNK);
                let s = r.range(0.75, 1.55);
                let tint = [0xffffffffu32, 0xd8f0e0ff, 0xc8dcffff, 0xe8ffe8ff][(r.next_u32() % 4) as usize];
                trees.push(Instance::at(vec3(x, 0.0, z)).with_rot(Quat::from_rotation_y(r.range(0.0, 6.28))).with_scale3(vec3(s, s * r.range(0.85, 1.25), s)).with_color(tint));
            }
        }
        c.trees.set_instances(&trees);
        let mut grass = Vec::new();
        for _ in 0..280 {
            let x = (r.range(0.85, 9.0)) * if r.next_u32() % 2 == 0 { -1.0 } else { 1.0 };
            grass.push(Instance::at(vec3(x, 0.0, -r.range(0.0, CHUNK))).with_rot(Quat::from_rotation_y(r.range(0.0, 6.28))).with_scale(r.range(0.6, 1.4)));
        }
        c.grass.set_instances(&grass);
        let mut shrooms = Vec::new();
        for _ in 0..6 {
            let x = r.range(0.95, 3.0) * if r.next_u32() % 2 == 0 { -1.0 } else { 1.0 };
            let glow = [0x7ff7ffff, 0xff8ff0ff, 0xb2ff8aff][(r.next_u32() % 3) as usize];
            shrooms.push(Instance::at(vec3(x, 0.0, -r.range(0.0, CHUNK))).with_scale(r.range(0.8, 1.6)).with_color(glow));
        }
        c.shrooms.set_instances(&shrooms);
    }

    /// Moves everything to where the sim says, once per frame.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, sim: &Sim, title: bool, over: bool, time: f32, dt: f32) {
        let _ = title;
        self.load_fox();
        let dist = sim.dist;
        let z_of = |d: f32| -(d - dist);
        // Chunks leapfrog: each shows the slab it's assigned, laid out when it changes.
        let first = ((dist - 10.0) / CHUNK).floor() as i64;
        for k in 0..CHUNKS as i64 {
            let index = first + k;
            let slot = index.rem_euclid(CHUNKS as i64) as usize;
            let c = &mut self.chunks[slot];
            if c.index != index {
                World::layout(c, index);
            }
            c.root.set_transform(&Transform::at(vec3(0.0, 0.0, z_of(index as f32 * CHUNK))));
        }
        // Logs from the pool.
        for (i, n) in self.logs.iter().enumerate() {
            match sim.logs.get(i) {
                Some(&d) => {
                    n.set_visible(true);
                    n.set_transform(&Transform::at(vec3(0.0, 0.135, z_of(d))).with_rot(Quat::from_rotation_y(0.08 * ((d * 1.7).sin()))));
                }
                None => n.set_visible(false),
            }
        }
        // Coins, spinning, as one instanced draw.
        self.coin_list.clear();
        for c in sim.coin_list.iter().filter(|c| !c.taken) {
            let z = z_of(c.d);
            if z < -sim::AHEAD || z > 6.0 {
                continue;
            }
            let spin = time * 3.0 + c.d * 0.5;
            self.coin_list.push(Instance::at(vec3(c.x, c.y, z)).with_rot(Quat::from_rotation_y(spin) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)));
        }
        self.coins.set_instances(&self.coin_list);
        // Distance signs at every 100 m, to the right of the trail.
        // The next marker not yet well behind the camera, and the one after it.
        let first_marker = (((dist - 8.0) / 100.0).ceil().max(1.0) as u32) * 100;
        for (i, (board, label, shown)) in self.signs.iter_mut().enumerate() {
            let next = first_marker + 100 * i as u32;
            board.set_transform(&Transform::at(vec3(1.4, 0.0, z_of(next as f32))).with_rot(Quat::from_rotation_y(-0.35)));
            board.set_visible(!title);
            if *shown != next {
                *shown = next;
                label.set_text(&TextDesc::new(Font::SansBold, 0.3).with_color([1.6, 1.35, 0.9, 1.0]).with_outline([0.05, 0.03, 0.02, 1.0], 0.07), &format!("{next} m"));
            }
        }
        self.title.set_visible(dist < 30.0);
        self.title.set_transform(&Transform::at(vec3(-1.4, 0.0, z_of(1.5))).with_rot(Quat::from_rotation_y(0.5)).with_scale(1.25));

        // Fireflies drift with the forest (they're emitted in its moving frame).
        let forest = vec3(0.0, 0.0, if over { 0.0 } else { sim.speed });
        self.next_fly -= dt;
        while self.next_fly <= 0.0 {
            self.next_fly += 0.12;
            let p = vec3(self.rng.range(-6.0, 6.0), self.rng.range(0.3, 2.2), -self.rng.range(6.0, 40.0));
            self.fireflies.emit_moving(p, Vec3::ZERO, 1, forest);
        }
        for e in &sim.events {
            match *e {
                sim::Event::Coin(c) => {
                    self.sparkle.emit_moving(vec3(c.x, c.y, z_of(c.d)), Vec3::ZERO, 24, forest);
                    let p = &mut self.pops[self.next_pop];
                    self.next_pop = (self.next_pop + 1) % POPS;
                    p.born = time;
                    p.at = (c.d, c.x, c.y);
                }
                sim::Event::Tripped => self.dust.emit(vec3(0.0, 0.2, -0.3), Vec3::Y, 30),
                _ => {}
            }
        }
        for p in &self.pops {
            let age = time - p.born;
            if age > 0.9 {
                p.node.set_visible(false);
                continue;
            }
            let fade = 1.0 - (age / 0.9).powi(2);
            p.node.set_visible(true);
            p.node.set_transform(&Transform::at(vec3(p.at.1 + 0.3, p.at.2 + 0.3 + age * 1.1, z_of(p.at.0))));
            let desc = TextDesc { flags: TextDesc::BILLBOARD | TextDesc::SCREEN | TextDesc::NO_FOG, ..TextDesc::new(Font::SansBold, 26.0) }
                .with_color([2.4, 1.8, 0.6, fade])
                .with_outline([0.1, 0.04, 0.0, fade], 0.08);
            p.node.set_text(&desc, "+10");
        }

        // The fox: idle on the title card and after a fall, running (faster
        // as the trail speeds up), the Jump clip on a jump.
        if let Some(f) = &mut self.fox {
            if over {
                f.anim.play(f.idle, Play::Loop, 0.35);
            } else if sim.jump.is_some() {
                if f.anim.current() != Some(f.jump) {
                    f.anim.restart(f.jump, Play::Once, 0.06);
                }
            } else {
                f.anim.play(f.run, Play::Loop, 0.12);
                f.anim.set_speed(sim.speed / 8.0);
            }
            f.anim.update(dt);
            f.anim.apply(f.root);
        }
        gfx3d::depth_of_field(&DepthOfField { focus: sim.focus(title, time), range: 1.3, blur: 5.0, near: 0.4 });
    }
}
