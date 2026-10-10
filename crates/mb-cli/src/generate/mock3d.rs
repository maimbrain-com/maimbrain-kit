//! The mock provider's 3D models: deterministic stand-ins shaped like what
//! real text-to-3D services return, so `mb model` runs (and is tested)
//! offline. Like a provider's output they face +z, sit off the origin at an
//! arbitrary scale, have far more triangles than a game wants, a 512²
//! texture, an extension mb3d doesn't read, and (when asked) a skeleton plus
//! one .glb per animation. A fish for prompts about fish; a four-legged
//! critter otherwise. Same prompt and seed, same bytes.

use std::f32::consts::{PI, TAU};

use super::glb::{self, Anim, Channel, ImageData, Material, Mesh, Node, Path, Prim, Scene, Skin, TexRef, Texture};
use super::img::Image;
use super::mock::hash;
use super::provider::{ModelOutput, ModelRequest};

const FISHY: &[&str] = &["fish", "koi", "carp", "shark", "whale", "dolphin", "eel", "salmon", "trout", "guppy", "minnow", "swim"];

pub fn is_fish(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    FISHY.iter().any(|w| p.contains(w))
}

struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// A lathe along z: rings of an elliptical cross-section, radius `r(t)` for t in 0..1 tail to head.
fn lathe(prim: &mut Prim, rings: usize, segs: usize, z0: f32, z1: f32, y: f32, squash: [f32; 2], r: &dyn Fn(f32) -> f32) {
    let base = prim.pos.len() as u32;
    for i in 0..=rings {
        let t = i as f32 / rings as f32;
        let z = z0 + (z1 - z0) * t;
        let rr = r(t).max(1e-4);
        for j in 0..=segs {
            let a = TAU * j as f32 / segs as f32;
            let (s, c) = a.sin_cos();
            prim.pos.push([c * rr * squash[0], y + s * rr * squash[1], z]);
            prim.uv.as_mut().unwrap().push([j as f32 / segs as f32, 1.0 - t]);
        }
    }
    let row = segs as u32 + 1;
    for i in 0..rings as u32 {
        for j in 0..segs as u32 {
            let (a, b, c, d) = (base + i * row + j, base + i * row + j + 1, base + (i + 1) * row + j, base + (i + 1) * row + j + 1);
            prim.indices.extend([a, b, c, b, d, c]);
        }
    }
}

/// A two-sided flat fin: a fan of quads between a root edge and a tip edge.
fn fin(prim: &mut Prim, root: [[f32; 3]; 2], tip: [[f32; 3]; 2], steps: usize) {
    for side in [false, true] {
        let base = prim.pos.len() as u32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            for (k, e) in [root, tip].iter().enumerate() {
                let p = [e[0][0] + (e[1][0] - e[0][0]) * t, e[0][1] + (e[1][1] - e[0][1]) * t, e[0][2] + (e[1][2] - e[0][2]) * t];
                prim.pos.push(p);
                prim.uv.as_mut().unwrap().push([0.02 + 0.06 * k as f32, 0.02 + 0.06 * t]);
            }
        }
        for i in 0..steps as u32 {
            let (a, b, c, d) = (base + 2 * i, base + 2 * i + 1, base + 2 * i + 2, base + 2 * i + 3);
            if side {
                prim.indices.extend([a, c, b, b, c, d]);
            } else {
                prim.indices.extend([a, b, c, b, d, c]);
            }
        }
    }
}

fn texture(seed: u64, fish: bool) -> Image {
    let mut rng = Rng(seed | 1);
    let n = 512u32;
    let base = if fish { [242, 236, 222] } else { [90 + (rng.f() * 80.0) as u8, 120 + (rng.f() * 60.0) as u8, 70 + (rng.f() * 90.0) as u8] };
    let spot = if fish { [235, 92 + (rng.f() * 40.0) as u8, 24] } else { [40, 36, 48] };
    let (f1, f2, p1, p2) = (3.0 + rng.f() * 4.0, 5.0 + rng.f() * 5.0, rng.f() * TAU, rng.f() * TAU);
    let mut im = Image::new(n, n, [base[0], base[1], base[2], 255]);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let s = (u * TAU * f1 / 2.0 + p1).sin() * (v * TAU * f2 / 2.0 + p2).sin() + 0.5 * (u * TAU * 7.0 + v * 11.0).sin();
            let fine = ((u * 150.0).sin() * (v * 150.0).sin()) * 6.0;
            let c = if s > 0.35 { spot } else { base };
            // The fins' corner of the atlas is the spot color, lighter.
            let c = if u < 0.1 && v < 0.1 { [255, 170, 90] } else { c };
            im.set(x, y, [(c[0] as f32 + fine).clamp(0.0, 255.0) as u8, (c[1] as f32 + fine).clamp(0.0, 255.0) as u8, (c[2] as f32 + fine).clamp(0.0, 255.0) as u8, 255]);
        }
    }
    im
}

/// The joints: a chain along z from the head (front, +z) to the tail.
fn chain(z_head: f32, z_tail: f32, y: f32, n: usize) -> Vec<(String, [f32; 3])> {
    (0..n).map(|k| (if k == 0 { "Root".to_string() } else { format!("Spine{k}") }, [0.0, y, z_head + (z_tail - z_head) * k as f32 / (n - 1) as f32])).collect()
}

fn skin_weights(prim: &mut Prim, joints: &[(String, [f32; 3])]) {
    let zs: Vec<f32> = joints.iter().map(|j| j.1[2]).collect();
    let (mut jj, mut ww) = (Vec::new(), Vec::new());
    for p in &prim.pos {
        // The two nearest joints along the chain, blended.
        let mut k = 0;
        while k + 1 < zs.len() && p[2] < zs[k + 1] {
            k += 1;
        }
        let k1 = (k + 1).min(zs.len() - 1);
        let t = if k1 == k { 0.0 } else { ((zs[k] - p[2]) / (zs[k] - zs[k1])).clamp(0.0, 1.0) };
        jj.push([k as u16, k1 as u16, 0, 0]);
        ww.push([1.0 - t, t, 0.0, 0.0]);
    }
    prim.joints = Some(jj);
    prim.weights = Some(ww);
}

fn clip(action: &str, joints: usize, fish: bool) -> Anim {
    let (secs, keys) = match action {
        "run" => (0.5, 9),
        "idle" => (2.0, 17),
        _ => (1.0, 13),
    };
    let input: Vec<f32> = (0..keys).map(|k| secs * k as f32 / (keys - 1) as f32).collect();
    let mut channels = Vec::new();
    for j in 1..joints {
        let amp = match action {
            "idle" => 0.04,
            "run" => 0.22,
            _ if fish => 0.12 + 0.1 * j as f32,
            _ => 0.12,
        };
        let output: Vec<f32> = input.iter().flat_map(|t| glb::quat_y(amp * (TAU * t / secs - j as f32 * 0.9).sin())).collect();
        channels.push(Channel { node: j, path: Path::R, interp: "LINEAR".into(), input: input.clone(), output });
    }
    if !fish {
        let bob = if action == "idle" { 0.01 } else { 0.05 };
        let output: Vec<f32> = input.iter().flat_map(|t| [0.0, 0.9 + bob * (2.0 * TAU * t / secs).sin().abs(), 0.4]).collect();
        channels.push(Channel { node: 0, path: Path::T, interp: "LINEAR".into(), input: input.clone(), output });
    }
    Anim { name: format!("Armature|{action}"), channels }
}

/// The raw model a real provider might return for the request.
pub fn model(req: &ModelRequest) -> ModelOutput {
    let seed = hash(&[req.prompt.as_bytes(), &req.seed.unwrap_or(0).to_le_bytes()]);
    let fish = is_fish(&req.prompt);
    let mut prim = Prim { uv: Some(Vec::new()), material: Some(0), ..Prim::default() };
    // Provider-ish units and an off-center origin: 1.8 units long, sitting at (0.3, 0.9, 0.4).
    let (y, zc) = (0.9, 0.4);
    let joints;
    if fish {
        lathe(&mut prim, 120, 96, zc - 0.9, zc + 0.7, y, [0.55, 0.8], &|t| 0.32 * (PI * t.powf(0.8)).sin() + 0.02);
        fin(&mut prim, [[0.0, y, zc - 0.85], [0.0, y + 0.02, zc - 0.7]], [[0.0, y + 0.35, zc - 1.15], [0.0, y - 0.35, zc - 1.15]], 24);
        fin(&mut prim, [[0.0, y + 0.24, zc - 0.2], [0.0, y + 0.25, zc + 0.25]], [[0.0, y + 0.42, zc - 0.35], [0.0, y + 0.36, zc + 0.05]], 16);
        joints = chain(zc + 0.6, zc - 1.1, y, 5);
    } else {
        lathe(&mut prim, 100, 80, zc - 0.7, zc + 0.5, y, [0.8, 0.7], &|t| 0.35 * (PI * t).sin().sqrt() + 0.01);
        lathe(&mut prim, 40, 48, zc + 0.45, zc + 0.95, y + 0.25, [1.0, 1.0], &|t| 0.24 * (PI * t).sin().sqrt() + 0.01);
        for (lx, lz) in [(-0.2, zc - 0.45), (0.2, zc - 0.45), (-0.2, zc + 0.3), (0.2, zc + 0.3)] {
            let base = prim.pos.len();
            lathe(&mut prim, 12, 16, -0.45, 0.0, 0.0, [1.0, 1.0], &|_| 0.07);
            // Stand the leg up: its z runs 0 → −0.45 down to the ground (y = 0.45 at the hip).
            for p in &mut prim.pos[base..] {
                *p = [p[0] + lx, y - 0.45 - p[2] - 0.45 + 0.45, lz + p[1]];
            }
        }
        joints = chain(zc + 0.9, zc - 0.7, y, 4);
    }
    prim.nrm = Some(super::shape::smooth_normals(&prim.pos, &prim.indices));
    let im = texture(seed, fish);
    let mut sc = Scene {
        nodes: vec![Node { name: if fish { "fish" } else { "critter" }.into(), mesh: Some(0), ..Node::default() }],
        roots: vec![0],
        meshes: vec![Mesh { name: "body".into(), prims: vec![], weights: vec![] }],
        materials: vec![Material { name: "skin".into(), metallic: 0.0, roughness: 0.55, base_tex: Some(TexRef { texture: 0, transform: None }), ..Material::default() }],
        textures: vec![Texture { image: 0, sampler: None }],
        images: vec![ImageData { name: "base".into(), bytes: im.encode_png(), mime: "image/png".into() }],
        ..Scene::default()
    };
    let rigged = req.rig || !req.animations.is_empty();
    let mut clips = Vec::new();
    if rigged {
        skin_weights(&mut prim, &joints);
        // Joints first (0..n), the mesh node last.
        let mut nodes = Vec::new();
        for (k, (name, p)) in joints.iter().enumerate() {
            let parent = if k == 0 { [0.0; 3] } else { joints[k - 1].1 };
            nodes.push(Node { name: name.clone(), t: [p[0] - parent[0], p[1] - parent[1], p[2] - parent[2]], children: if k + 1 < joints.len() { vec![k + 1] } else { vec![] }, ..Node::default() });
        }
        nodes.push(Node { name: "body".into(), mesh: Some(0), skin: Some(0), ..Node::default() });
        let ibm = joints.iter().map(|(_, p)| glb::trs([-p[0], -p[1], -p[2]], [0.0, 0.0, 0.0, 1.0], [1.0; 3])).collect();
        sc.skins.push(Skin { name: "Armature".into(), joints: (0..joints.len()).collect(), ibm, skeleton: Some(0) });
        let n = nodes.len();
        sc.nodes = nodes;
        sc.roots = vec![0, n - 1];
    }
    sc.meshes[0].prims.push(prim);
    let mut raw = glb::write(&sc, "mb mock 3D");
    // Like many exporters, declare an extension mb3d doesn't read.
    raw = with_extension(&raw, "KHR_materials_specular");
    for a in &req.animations {
        let mut one = sc.clone();
        one.anims = vec![clip(a, joints.len(), fish)];
        clips.push((a.clone(), glb::write(&one, "mb mock 3D")));
    }
    let tris = sc.triangles();
    ModelOutput {
        glb: raw,
        clips,
        rigged,
        cost_usd: Some(0.0),
        text: Some(format!("mock {} ({tris} triangles{})", if fish { "fish" } else { "critter" }, if rigged { ", rigged" } else { "" })),
        notes: Vec::new(),
        tasks: if rigged { vec![("mock".into(), format!("{seed:016x}")), ("rig (mock)".into(), format!("mock-rig-{seed:016x}"))] } else { vec![("mock".into(), format!("{seed:016x}"))] },
        rig: rigged.then(|| (format!("mock-rig-{seed:016x}"), "mock".to_string())),
    }
}

fn with_extension(glb_bytes: &[u8], ext: &str) -> Vec<u8> {
    let (mut j, bin) = glb::split_glb(glb_bytes).unwrap();
    j["extensionsUsed"] = serde_json::json!([ext]);
    let mut text = serde_json::to_vec(&j).unwrap();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 28 + text.len() + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((text.len() as u32).to_le_bytes());
    out.extend(0x4E4F_534Au32.to_le_bytes());
    out.extend(text);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004E_4942u32.to_le_bytes());
    out.extend(bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(prompt: &str, anims: &[&str]) -> ModelRequest {
        ModelRequest { prompt: prompt.into(), seed: Some(3), rig: false, animations: anims.iter().map(|s| s.to_string()).collect(), ..ModelRequest::default() }
    }

    #[test]
    fn fish_and_critters_deterministic_and_heavy() {
        let a = model(&req("a koi", &[]));
        assert_eq!(a.glb, model(&req("a koi", &[])).glb);
        let sc = glb::read(&a.glb).unwrap();
        assert!(sc.triangles() > 20_000, "{}", sc.triangles());
        assert!(sc.dropped.contains("KHR_materials_specular"));
        assert!(sc.skins.is_empty() && a.clips.is_empty());
        let c = model(&req("a fox", &["walk", "run"]));
        assert!(c.rigged);
        assert_eq!(c.clips.len(), 2);
        let walk = glb::read(&c.clips[0].1).unwrap();
        assert_eq!(walk.anims[0].name, "Armature|walk");
        assert_eq!(walk.skins[0].joints.len(), 4);
    }
}
