//! What `mb model` does to a provider's .glb so it drops into an mb3d game
//! (SPEC §5.4): pose evaluation (for bounds and the preview), scale,
//! orientation and origin, a triangle budget (meshoptimizer's simplifier,
//! which keeps UV seams, skin weights and morph targets intact), texture
//! sizes and encodings, a derived glow, clip names, and the checks the
//! validator and host apply.

use serde_json::{Value, json};

use super::glb::{self, Anim, Mat4, Path, Prim, Scene};
use super::img::Image;

// --- poses --------------------------------------------------------------------------

/// Local matrices and morph weights for every node: the rest pose, or a clip at a time.
pub struct Pose {
    pub local: Vec<Mat4>,
    pub weights: Vec<Option<Vec<f32>>>,
}

fn sample(c: &glb::Channel, t: f32, n: usize) -> Vec<f32> {
    let keys = c.input.len();
    let cubic = c.interp == "CUBICSPLINE";
    let stride = if cubic { 3 * n } else { n };
    let value = |k: usize| -> &[f32] {
        let o = k * stride + if cubic { n } else { 0 };
        c.output.get(o..o + n).unwrap_or(&[])
    };
    if keys == 0 || c.output.len() < keys * stride {
        return Vec::new();
    }
    if t <= c.input[0] || keys == 1 {
        return value(0).to_vec();
    }
    if t >= c.input[keys - 1] {
        return value(keys - 1).to_vec();
    }
    let k = c.input.partition_point(|&x| x <= t).clamp(1, keys - 1) - 1;
    let (t0, t1) = (c.input[k], c.input[k + 1]);
    let td = (t1 - t0).max(1e-9);
    let s = (t - t0) / td;
    let (a, b) = (value(k), value(k + 1));
    let mut out: Vec<f32> = match c.interp.as_str() {
        "STEP" => a.to_vec(),
        "CUBICSPLINE" => {
            let out_a = &c.output[k * stride + 2 * n..k * stride + 3 * n];
            let in_b = &c.output[(k + 1) * stride..(k + 1) * stride + n];
            let (s2, s3) = (s * s, s * s * s);
            (0..n)
                .map(|i| (2.0 * s3 - 3.0 * s2 + 1.0) * a[i] + (s3 - 2.0 * s2 + s) * td * out_a[i] + (-2.0 * s3 + 3.0 * s2) * b[i] + (s3 - s2) * td * in_b[i])
                .collect()
        }
        _ => {
            if n == 4 && c.path == Path::R {
                // Shorter arc, then normalized lerp (close to slerp at animation key spacing).
                let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
                let sg = if dot < 0.0 { -1.0 } else { 1.0 };
                (0..4).map(|i| a[i] * (1.0 - s) + sg * b[i] * s).collect()
            } else {
                (0..n).map(|i| a[i] * (1.0 - s) + b[i] * s).collect()
            }
        }
    };
    if c.path == Path::R {
        let l = out.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        out.iter_mut().for_each(|x| *x /= l);
    }
    out
}

pub fn pose(sc: &Scene, clip: Option<(&Anim, f32)>) -> Pose {
    let mut trs: Vec<([f32; 3], [f32; 4], [f32; 3])> = sc.nodes.iter().map(|n| (n.t, n.r, n.s)).collect();
    let mut weights: Vec<Option<Vec<f32>>> = vec![None; sc.nodes.len()];
    if let Some((a, t)) = clip {
        for c in &a.channels {
            let n = match c.path {
                Path::T | Path::S => 3,
                Path::R => 4,
                Path::W => {
                    let targets = sc.nodes[c.node].mesh.and_then(|m| sc.meshes[m].prims.first()).map(|p| p.targets.len()).unwrap_or(0);
                    if targets == 0 {
                        continue;
                    }
                    targets
                }
            };
            let v = sample(c, t, n);
            if v.len() != n {
                continue;
            }
            let e = &mut trs[c.node];
            match c.path {
                Path::T => e.0 = [v[0], v[1], v[2]],
                Path::R => e.1 = [v[0], v[1], v[2], v[3]],
                Path::S => e.2 = [v[0], v[1], v[2]],
                Path::W => weights[c.node] = Some(v),
            }
        }
    }
    Pose { local: trs.iter().map(|&(t, r, s)| glb::trs(t, r, s)).collect(), weights }
}

/// One drawn primitive in world space.
pub struct Posed {
    pub mesh: usize,
    pub prim: usize,
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
}

/// Every drawn primitive's vertices in world space for a pose (morphs, then skinning).
pub fn posed(sc: &Scene, p: &Pose) -> Vec<Posed> {
    let world = sc.world(&p.local);
    let mut out = Vec::new();
    for (n, _) in sc.walk() {
        let node = &sc.nodes[n];
        let Some(mi) = node.mesh else { continue };
        let mesh = &sc.meshes[mi];
        let joints: Option<Vec<Mat4>> = node.skin.map(|s| {
            let sk = &sc.skins[s];
            sk.joints.iter().zip(&sk.ibm).map(|(&j, ibm)| glb::mul(&world[j], ibm)).collect()
        });
        let mw = p.weights[n].clone().unwrap_or_else(|| mesh.weights.clone());
        for (pi, prim) in mesh.prims.iter().enumerate() {
            let nrm0 = prim.nrm.clone().unwrap_or_else(|| smooth_normals(&prim.pos, &prim.indices));
            let mut pos = prim.pos.clone();
            let mut nrm = nrm0;
            for (k, t) in prim.targets.iter().enumerate() {
                let w = mw.get(k).copied().unwrap_or(0.0);
                if w == 0.0 {
                    continue;
                }
                for (v, d) in pos.iter_mut().zip(&t.pos) {
                    (0..3).for_each(|i| v[i] += w * d[i]);
                }
                if let Some(tn) = &t.nrm {
                    for (v, d) in nrm.iter_mut().zip(tn) {
                        (0..3).for_each(|i| v[i] += w * d[i]);
                    }
                }
            }
            match (&joints, &prim.joints, &prim.weights) {
                (Some(js), Some(jj), Some(ww)) => {
                    for v in 0..pos.len() {
                        let (mut pp, mut nn) = ([0.0f32; 3], [0.0f32; 3]);
                        let wsum: f32 = ww[v].iter().sum::<f32>().max(1e-9);
                        for k in 0..4 {
                            let w = ww[v][k] / wsum;
                            if w == 0.0 {
                                continue;
                            }
                            let Some(m) = js.get(jj[v][k] as usize) else { continue };
                            let a = glb::point(m, pos[v]);
                            let b = glb::dir(m, nrm[v]);
                            (0..3).for_each(|i| {
                                pp[i] += w * a[i];
                                nn[i] += w * b[i];
                            });
                        }
                        pos[v] = pp;
                        nrm[v] = glb::normalize(nn);
                    }
                }
                _ => {
                    let m = &world[n];
                    for v in 0..pos.len() {
                        pos[v] = glb::point(m, pos[v]);
                        nrm[v] = glb::normalize(glb::dir(m, nrm[v]));
                    }
                }
            }
            out.push(Posed { mesh: mi, prim: pi, pos, nrm });
        }
    }
    out
}

pub fn bounds(posed: &[Posed]) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut any = false;
    for p in posed {
        for v in &p.pos {
            any = true;
            for i in 0..3 {
                lo[i] = lo[i].min(v[i]);
                hi[i] = hi[i].max(v[i]);
            }
        }
    }
    any.then_some((lo, hi))
}

pub fn smooth_normals(pos: &[[f32; 3]], idx: &[u32]) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in idx.chunks(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &i in t {
            (0..3).for_each(|k| n[i as usize][k] += f[k]);
        }
    }
    n.into_iter().map(glb::normalize).collect()
}

// --- placement ------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Standing on y = 0, centered over x = z = 0 (characters, props).
    #[default]
    Ground,
    /// Centered on the origin (swimmers, fliers, pickups).
    Center,
}

pub struct Placement {
    /// Longest side, meters.
    pub size: f32,
    pub origin: Origin,
    /// Turn about +y, radians, applied first (the provider's front is +z).
    pub turn: f32,
    /// Also turn so the longest horizontal side runs along z.
    pub lengthwise: bool,
    /// A rotation applied before `turn` (`--orient`'s), as a quaternion.
    pub pre: [f32; 4],
}

/// Puts every root under one new node that scales, turns and moves the model
/// into place (skins, clips and node names stay as they are). Returns facts.
pub fn place(sc: &mut Scene, name: &str, p: &Placement) -> Result<Value, String> {
    let rest = posed(sc, &pose(sc, None));
    let (lo, hi) = bounds(&rest).ok_or("the model has no triangles")?;
    let mut turn = p.turn;
    let mut turned_lengthwise = false;
    if p.lengthwise && (hi[0] - lo[0]) > (hi[2] - lo[2]) * 1.05 {
        turn += std::f32::consts::FRAC_PI_2;
        turned_lengthwise = true;
    }
    let q = glb::quat_mul(glb::quat_y(turn), p.pre);
    let r = glb::trs([0.0; 3], q, [1.0; 3]);
    let mut rlo = [f32::MAX; 3];
    let mut rhi = [f32::MIN; 3];
    for v in rest.iter().flat_map(|p| &p.pos) {
        let w = glb::point(&r, *v);
        for i in 0..3 {
            rlo[i] = rlo[i].min(w[i]);
            rhi[i] = rhi[i].max(w[i]);
        }
    }
    let ext = [rhi[0] - rlo[0], rhi[1] - rlo[1], rhi[2] - rlo[2]];
    let longest = ext[0].max(ext[1]).max(ext[2]);
    if !(longest > 1e-9) || !longest.is_finite() {
        return Err("the model has no size".into());
    }
    let k = p.size / longest;
    let c = [(rlo[0] + rhi[0]) / 2.0, (rlo[1] + rhi[1]) / 2.0, (rlo[2] + rhi[2]) / 2.0];
    let t = match p.origin {
        Origin::Ground => [-k * c[0], -k * rlo[1], -k * c[2]],
        Origin::Center => [-k * c[0], -k * c[1], -k * c[2]],
    };
    let wrapper = glb::Node { name: name.to_string(), t, r: q, s: [k; 3], children: sc.roots.clone(), mesh: None, skin: None };
    sc.nodes.push(wrapper);
    sc.roots = vec![sc.nodes.len() - 1];
    let dims = [ext[0] * k, ext[1] * k, ext[2] * k];
    Ok(json!({
        "size_m": dims,
        "source_size": [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
        "scale": k,
        "turn_degrees": turn.to_degrees().round(),
        "turned_lengthwise": turned_lengthwise,
        "origin": match p.origin { Origin::Ground => "ground", Origin::Center => "center" },
    }))
}

/// Stable names for the nodes that draw: `<asset>/body`, `<asset>/body2`… in
/// scene order (providers name them `tripo_node_<uuid>`, new every take), so
/// `model.find("heron/body")` works in every take. Bones keep their names.
pub fn name_nodes(sc: &mut Scene, name: &str) {
    let drawn: Vec<usize> = sc.walk().into_iter().map(|(n, _)| n).filter(|&n| sc.nodes[n].mesh.is_some()).collect();
    for (k, n) in drawn.into_iter().enumerate() {
        sc.nodes[n].name = if k == 0 { format!("{name}/body") } else { format!("{name}/body{}", k + 1) };
    }
}

// --- triangle budget ------------------------------------------------------------------------

/// Merges vertices whose every attribute is identical (providers often split them).
fn weld(p: &mut Prim) {
    let n = p.pos.len();
    let key_of = |v: usize| -> Vec<u32> {
        let mut k: Vec<u32> = p.pos[v].iter().map(|x| x.to_bits()).collect();
        if let Some(a) = &p.nrm {
            k.extend(a[v].iter().map(|x| x.to_bits()));
        }
        if let Some(a) = &p.tan {
            k.extend(a[v].iter().map(|x| x.to_bits()));
        }
        if let Some(a) = &p.uv {
            k.extend(a[v].iter().map(|x| x.to_bits()));
        }
        if let Some(a) = &p.color {
            k.extend(a[v].iter().map(|x| x.to_bits()));
        }
        if let Some(a) = &p.joints {
            k.extend(a[v].iter().map(|&x| x as u32));
        }
        if let Some(a) = &p.weights {
            k.extend(a[v].iter().map(|x| x.to_bits()));
        }
        for t in &p.targets {
            k.extend(t.pos[v].iter().map(|x| x.to_bits()));
            if let Some(tn) = &t.nrm {
                k.extend(tn[v].iter().map(|x| x.to_bits()));
            }
        }
        k
    };
    let mut seen: std::collections::HashMap<Vec<u32>, u32> = std::collections::HashMap::new();
    let mut remap = vec![0u32; n];
    let mut keep = Vec::new();
    for v in 0..n {
        let k = key_of(v);
        let next = keep.len() as u32;
        let id = *seen.entry(k).or_insert_with(|| {
            keep.push(v);
            next
        });
        remap[v] = id;
    }
    if keep.len() == n {
        return;
    }
    for i in &mut p.indices {
        *i = remap[*i as usize];
    }
    compact(p, &keep);
}

/// Keeps only vertices `keep` (old indices, in new order); indices must already be remapped.
fn compact(p: &mut Prim, keep: &[usize]) {
    fn pick<T: Copy>(v: &[T], keep: &[usize]) -> Vec<T> {
        keep.iter().map(|&i| v[i]).collect()
    }
    p.pos = pick(&p.pos, keep);
    p.nrm = p.nrm.as_ref().map(|a| pick(a, keep));
    p.tan = p.tan.as_ref().map(|a| pick(a, keep));
    p.uv = p.uv.as_ref().map(|a| pick(a, keep));
    p.color = p.color.as_ref().map(|a| pick(a, keep));
    p.joints = p.joints.as_ref().map(|a| pick(a, keep));
    p.weights = p.weights.as_ref().map(|a| pick(a, keep));
    for t in &mut p.targets {
        t.pos = pick(&t.pos, keep);
        t.nrm = t.nrm.as_ref().map(|a| pick(a, keep));
    }
}

/// Drops vertices no triangle uses and orders the rest for the GPU's caches.
fn tidy(p: &mut Prim) {
    let n = p.pos.len();
    p.indices = meshopt::optimize_vertex_cache(&p.indices, n);
    let mut remap = vec![u32::MAX; n];
    let mut keep = Vec::new();
    for i in &mut p.indices {
        let v = *i as usize;
        if remap[v] == u32::MAX {
            remap[v] = keep.len() as u32;
            keep.push(v);
        }
        *i = remap[v];
    }
    compact(p, &keep);
}

fn simplify_prim(p: &mut Prim, target_tris: usize) -> f32 {
    weld(p);
    let target = (target_tris.max(4) * 3).min(p.indices.len());
    if target >= p.indices.len() {
        tidy(p);
        return 0.0;
    }
    // Normals and UVs steer the simplifier so shading and texture seams survive.
    let (mut attrs, mut wts) = (Vec::new(), Vec::new());
    let stride_f = p.nrm.as_ref().map_or(0, |_| 3) + p.uv.as_ref().map_or(0, |_| 2);
    if stride_f > 0 {
        for v in 0..p.pos.len() {
            if let Some(n) = &p.nrm {
                attrs.extend_from_slice(&n[v]);
            }
            if let Some(t) = &p.uv {
                attrs.extend_from_slice(&t[v]);
            }
        }
        if p.nrm.is_some() {
            wts.extend([0.5, 0.5, 0.5]);
        }
        if p.uv.is_some() {
            wts.extend([1.0, 1.0]);
        }
    }
    let locks = vec![false; p.pos.len()];
    let mut best = p.indices.clone();
    let mut err = 0.0f32;
    for limit in [0.01f32, 0.03, 0.1, 0.3, 1.0] {
        let mut e = 0.0f32;
        let out = if stride_f > 0 {
            meshopt::simplify_with_attributes_and_locks_decoder(
                &p.indices,
                &p.pos,
                &attrs,
                &wts,
                stride_f * 4,
                &locks,
                target,
                limit,
                meshopt::SimplifyOptions::None,
                Some(&mut e),
            )
        } else {
            meshopt::simplify_decoder(&p.indices, &p.pos, target, limit, meshopt::SimplifyOptions::None, Some(&mut e))
        };
        if !out.is_empty() {
            best = out;
            err = e;
        }
        if best.len() <= target {
            break;
        }
    }
    p.indices = best;
    tidy(p);
    err
}

/// Fits the model in `budget` triangles, spread over its primitives by size. Returns (before, after, worst error).
pub fn fit_triangles(sc: &mut Scene, budget: usize) -> (usize, usize, f32) {
    let before = sc.triangles();
    let mut worst = 0.0f32;
    let over = before > budget;
    let total: usize = sc.meshes.iter().flat_map(|m| &m.prims).map(Prim::tris).sum::<usize>().max(1);
    for m in &mut sc.meshes {
        for p in &mut m.prims {
            if over {
                let share = (budget as f64 * p.tris() as f64 / total as f64).floor() as usize;
                worst = worst.max(simplify_prim(p, share));
            } else {
                weld(p);
                tidy(p);
            }
        }
    }
    (before, sc.triangles(), worst)
}

/// Adds smooth normals where a primitive has none (mb3d shades with them).
pub fn ensure_normals(sc: &mut Scene) -> bool {
    let mut added = false;
    for m in &mut sc.meshes {
        for p in &mut m.prims {
            if p.nrm.is_none() {
                p.nrm = Some(smooth_normals(&p.pos, &p.indices));
                added = true;
            }
        }
    }
    added
}

/// mb3d blends the first 4 morph targets; more are dropped.
pub fn limit_morphs(sc: &mut Scene) -> usize {
    let mut cut = 0;
    for m in &mut sc.meshes {
        for p in &mut m.prims {
            if p.targets.len() > 4 {
                cut = cut.max(p.targets.len());
                p.targets.truncate(4);
            }
        }
        m.weights.truncate(4);
    }
    cut
}

// --- textures ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum TexFormat {
    /// JPEG for opaque color, normal and metal-rough maps; PNG where alpha matters.
    #[default]
    Auto,
    Jpeg,
    Png,
}

fn roles(sc: &Scene) -> Vec<(bool, bool, bool)> {
    // Per image: (color, needs alpha, data map).
    let mut r = vec![(false, false, false); sc.images.len()];
    for m in &sc.materials {
        let blend = m.alpha_mode != "OPAQUE" || m.base[3] < 1.0;
        for (t, color) in [(&m.base_tex, true), (&m.emissive_tex, true), (&m.mr_tex, false), (&m.normal_tex, false)] {
            if let Some(i) = t.as_ref().and_then(|t| sc.texture_image(t)) {
                if color {
                    r[i].0 = true;
                } else {
                    r[i].2 = true;
                }
                if blend && std::ptr::eq(t, &m.base_tex) {
                    r[i].1 = true;
                }
            }
        }
    }
    r
}

pub fn encode_jpeg(im: &Image, quality: u8) -> Result<Vec<u8>, String> {
    let rgb: Vec<u8> = im.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.encode(&rgb, im.w as u16, im.h as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| format!("JPEG encoding: {e}"))?;
    Ok(out)
}

/// Resizes every texture to at most `max` px on its longer side (normal and
/// metal-rough maps to `maps_max`) and re-encodes it. Returns facts per image.
pub fn fit_textures(sc: &mut Scene, max: u32, maps_max: u32, format: TexFormat) -> Result<Vec<Value>, String> {
    let roles = roles(sc);
    let mut facts = Vec::new();
    for (i, im) in sc.images.iter_mut().enumerate() {
        let src = Image::decode(&im.bytes).map_err(|e| format!("texture {i}: {e}"))?;
        let (w0, h0) = (src.w, src.h);
        let (color, alpha, data) = roles[i];
        let side = if data && !color { maps_max.min(max) } else { max };
        let scale = (side as f32 / w0.max(h0) as f32).min(1.0);
        let (w, h) = (((w0 as f32 * scale).round() as u32).max(1), ((h0 as f32 * scale).round() as u32).max(1));
        let resized = if (w, h) != (w0, h0) { src.resize(w, h) } else { src };
        let keep_alpha = alpha && !resized.is_opaque();
        let png = format == TexFormat::Png || (format == TexFormat::Auto && keep_alpha);
        let bytes = if png {
            let mut im2 = resized.clone();
            if !keep_alpha {
                im2.px.iter_mut().for_each(|p| p[3] = 255);
            }
            im2.encode_png()
        } else {
            encode_jpeg(&resized, if data && !color { 92 } else { 85 })?
        };
        let role = match (color, data) {
            (true, false) => "color",
            (false, true) => "data",
            (true, true) => "color+data",
            _ => "unused",
        };
        facts.push(json!({ "size": [w, h], "source_size": [w0, h0], "format": if png { "png" } else { "jpeg" }, "kb": bytes.len().div_ceil(1024), "role": role }));
        im.bytes = bytes;
        im.mime = if png { "image/png" } else { "image/jpeg" }.into();
    }
    Ok(facts)
}

/// Mattes a chrome-like model: metalness times `metal` (0.15 keeps 15 %) and
/// roughness at least `min_rough`. Generated textures come back mostly metal,
/// which with nothing bright to reflect reads near-black outdoors. The floor is
/// baked into the metal-rough map (its green channel), since roughness is the
/// map times the factor and the factor is usually 1 already.
pub fn matte(sc: &mut Scene, metal: Option<f32>, min_rough: Option<f32>) -> Result<(), String> {
    let mut done = std::collections::HashSet::new();
    for mi in 0..sc.materials.len() {
        if let Some(k) = metal {
            sc.materials[mi].metallic *= k;
        }
        let Some(r) = min_rough else { continue };
        let rf = sc.materials[mi].roughness;
        match sc.materials[mi].mr_tex.as_ref().and_then(|t| sc.texture_image(t)) {
            Some(img) if done.insert(img) => {
                let floor = ((r / rf.max(1e-3)).min(1.0) * 255.0).round() as u8;
                let mut im = Image::decode(&sc.images[img].bytes)?;
                im.px.iter_mut().for_each(|p| p[1] = p[1].max(floor));
                sc.images[img].bytes = im.encode_png();
                sc.images[img].mime = "image/png".into();
            }
            Some(_) => {}
            None => sc.materials[mi].roughness = rf.max(r),
        }
    }
    Ok(())
}

/// The model's average metalness and roughness (map times factor, over each
/// material's map texels; the factor alone without a map).
pub fn metal_rough(sc: &Scene) -> Option<(f32, f32)> {
    let (mut m, mut r, mut n) = (0.0f32, 0.0f32, 0.0f32);
    for mat in &sc.materials {
        match mat.mr_tex.as_ref().and_then(|t| sc.texture_image(t)).and_then(|i| Image::decode(&sc.images[i].bytes).ok()) {
            Some(im) => {
                let k = im.px.len().max(1) as f32;
                m += mat.metallic * im.px.iter().map(|p| p[2] as f32 / 255.0).sum::<f32>() / k;
                r += mat.roughness * im.px.iter().map(|p| p[1] as f32 / 255.0).sum::<f32>() / k;
            }
            None => {
                m += mat.metallic;
                r += mat.roughness;
            }
        }
        n += 1.0;
    }
    (n > 0.0).then(|| (m / n, r / n))
}

/// Which texels of the base color glow.
#[derive(Clone, Debug)]
pub enum GlowBy {
    /// Value (the brightest channel, 0–1) above this; 0 = all of it.
    Brightness(f32),
    /// Near one of these sRGB colors: Oklab distance with lightness counted
    /// half (so a shaded texel of the color still matches) under `tolerance`.
    Colors(Vec<[u8; 3]>, f32),
}

/// How much a texel glows (0–1) for `by`.
fn glow_weight(p: [u8; 4], by: &GlowBy, targets: &[[f32; 3]]) -> f32 {
    match by {
        GlowBy::Brightness(t) => {
            let v = p[0].max(p[1]).max(p[2]) as f32 / 255.0;
            smoothstep(t - 0.08, t + 0.08, v)
        }
        GlowBy::Colors(_, tol) => {
            let o = super::img::oklab([p[0], p[1], p[2]]);
            let d = targets.iter().map(|c| ((0.5 * (o[0] - c[0])).powi(2) + (o[1] - c[1]).powi(2) + (o[2] - c[2]).powi(2)).sqrt()).fold(f32::MAX, f32::min);
            1.0 - smoothstep(tol * 0.75, tol * 1.25, d)
        }
    }
}

/// The emissive map for a base color: the glowing texels (a little brighter,
/// as glow_mask.py did), grown by a texel and softened when lit by color so the
/// edge of a gem or a stripe glows too. Returns it and the share of texels lit.
pub fn glow_map(src: &Image, by: &GlowBy) -> (Image, f32) {
    let targets: Vec<[f32; 3]> = match by {
        GlowBy::Colors(c, _) => c.iter().map(|&c| super::img::oklab(c)).collect(),
        GlowBy::Brightness(_) => Vec::new(),
    };
    let (w, h) = (src.w as usize, src.h as usize);
    let mut k: Vec<f32> = src.px.iter().map(|&p| glow_weight(p, by, &targets)).collect();
    if matches!(by, GlowBy::Colors(..)) {
        // Grow by one texel (a 3×3 max), then a 3×3 blur.
        for pass in 0..2 {
            let prev = k.clone();
            for y in 0..h {
                for x in 0..w {
                    let (mut m, mut s, mut n) = (0.0f32, 0.0f32, 0.0f32);
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                            if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                                continue;
                            }
                            let v = prev[yy as usize * w + xx as usize];
                            m = m.max(v);
                            s += v;
                            n += 1.0;
                        }
                    }
                    k[y * w + x] = if pass == 0 { m } else { s / n };
                }
            }
        }
    }
    let boost = if matches!(by, GlowBy::Colors(..)) { 1.15 } else { 1.0 };
    let mut out = src.clone();
    let mut lit = 0usize;
    for (p, &kk) in out.px.iter_mut().zip(&k) {
        if kk > 0.5 {
            lit += 1;
        }
        for c in 0..3 {
            p[c] = (p[c] as f32 * kk * boost).round().min(255.0) as u8;
        }
        p[3] = 255;
    }
    let share = lit as f32 / out.px.len().max(1) as f32;
    (out, share)
}

/// Makes the model glow: an emissive map from the base color's bright parts
/// (value above `threshold`, 0 = all of it), at `strength`.
pub fn add_glow(sc: &mut Scene, strength: f32, threshold: f32) -> Result<usize, String> {
    add_glow_by(sc, strength, &GlowBy::Brightness(threshold)).map(|(n, _)| n)
}

/// Makes the model glow where `by` picks texels of its base color, at
/// `strength`. Returns the materials lit and the largest share of texels lit.
pub fn add_glow_by(sc: &mut Scene, strength: f32, by: &GlowBy) -> Result<(usize, f32), String> {
    let mut share = 0.0f32;
    let mut made = 0;
    let mut cache: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for mi in 0..sc.materials.len() {
        let m = sc.materials[mi].clone();
        // Lit by brightness, a material that already glows keeps its own map; lit by
        // color, the map is replaced so only the chosen colors glow.
        if (m.emissive_tex.is_some() || m.emissive != [0.0; 3]) && matches!(by, GlowBy::Brightness(_)) {
            sc.materials[mi].emissive_strength = Some(strength.max(m.emissive_strength.unwrap_or(1.0)));
            continue;
        }
        match m.base_tex.as_ref().and_then(|t| sc.texture_image(t)) {
            Some(img) => {
                let tex = match cache.get(&img) {
                    Some(&t) => t,
                    None => {
                        let src = Image::decode(&sc.images[img].bytes)?;
                        let (out, s) = glow_map(&src, by);
                        share = share.max(s);
                        sc.images.push(glb::ImageData { name: "glow".into(), bytes: encode_jpeg(&out, 85)?, mime: "image/jpeg".into() });
                        sc.textures.push(glb::Texture { image: sc.images.len() - 1, sampler: None });
                        cache.insert(img, sc.textures.len() - 1);
                        sc.textures.len() - 1
                    }
                };
                let mm = &mut sc.materials[mi];
                mm.emissive_tex = Some(glb::TexRef { texture: tex, transform: None });
                mm.emissive = [m.base[0], m.base[1], m.base[2]];
            }
            None => {
                // No texture: the whole material glows if its color is picked.
                let c = m.base.map(|x| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8);
                let targets: Vec<[f32; 3]> = match by {
                    GlowBy::Colors(cs, _) => cs.iter().map(|&c| super::img::oklab(c)).collect(),
                    GlowBy::Brightness(_) => Vec::new(),
                };
                let k = if targets.is_empty() { 1.0 } else { glow_weight([c[0], c[1], c[2], 255], by, &targets) };
                if k < 0.5 {
                    sc.materials[mi].emissive = [0.0; 3];
                    sc.materials[mi].emissive_tex = None;
                    continue;
                }
                sc.materials[mi].emissive = [m.base[0], m.base[1], m.base[2]];
                sc.materials[mi].emissive_tex = None;
            }
        }
        sc.materials[mi].emissive_strength = Some(strength);
        made += 1;
    }
    Ok((made, share))
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    if a >= b {
        return if x >= b { 1.0 } else { 0.0 };
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// --- clips ------------------------------------------------------------------------------------

/// "swim" → "Swim", "run_fast" → "RunFast": the names games look clips up by.
pub fn clip_name(action: &str) -> String {
    action
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

/// Copies `other`'s clips into `sc`, matching nodes by name (both must come
/// from the same rig). The first copied clip is named `name`; returns how many
/// channels found no node.
pub fn merge_clips(sc: &mut Scene, other: &Scene, name: &str) -> Result<usize, String> {
    let by_name: std::collections::HashMap<&str, usize> = sc.nodes.iter().enumerate().filter(|(_, n)| !n.name.is_empty()).map(|(i, n)| (n.name.as_str(), i)).collect();
    let mut lost = 0;
    let mut added = 0;
    for a in &other.anims {
        let mut out = Anim { name: if added == 0 { name.to_string() } else { format!("{name}{}", added + 1) }, channels: Vec::new() };
        for c in &a.channels {
            match by_name.get(other.nodes[c.node].name.as_str()) {
                Some(&n) => out.channels.push(glb::Channel { node: n, ..c.clone() }),
                None => lost += 1,
            }
        }
        if !out.channels.is_empty() {
            sc.anims.push(out);
            added += 1;
        }
    }
    if added == 0 {
        return Err(format!("the {name} animation shares no bones with the rigged model"));
    }
    Ok(lost)
}

/// One `--clip` edit: `Slash=0.6..3.4` (trim), `Strike=Slash` (rename),
/// `Strike=Slash@1.8..3.4` (a new clip from part of one), `Chop=drop`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipEdit {
    pub name: String,
    pub source: String,
    /// Seconds into the source clip; None: its whole length (either end).
    pub range: Option<(Option<f32>, Option<f32>)>,
    pub drop: bool,
}

impl ClipEdit {
    pub fn parse(s: &str) -> Result<ClipEdit, String> {
        let bad = || format!("--clip {s:?}: write Name=0.6..3.4 (trim), New=Old (rename), New=Old@0.6..3.4 (a new clip from part of one) or Name=drop");
        let (name, rhs) = s.split_once('=').ok_or_else(bad)?;
        let (name, rhs) = (name.trim(), rhs.trim());
        if name.is_empty() || rhs.is_empty() {
            return Err(bad());
        }
        let num = |x: &str| -> Result<Option<f32>, String> {
            let x = x.trim().trim_end_matches('s');
            if x.is_empty() { Ok(None) } else { x.parse::<f32>().map(Some).map_err(|_| bad()) }
        };
        let range = |r: &str| -> Result<(Option<f32>, Option<f32>), String> {
            let (a, b) = r.split_once("..").ok_or_else(bad)?;
            let (a, b) = (num(a)?, num(b)?);
            if a.is_some_and(|a| a < 0.0) || matches!((a, b), (Some(a), Some(b)) if b <= a) {
                return Err(format!("--clip {s:?}: the range must run forward from 0 s or later"));
            }
            Ok((a, b))
        };
        if rhs.eq_ignore_ascii_case("drop") {
            return Ok(ClipEdit { name: name.into(), source: name.into(), range: None, drop: true });
        }
        if rhs.contains("..") && !rhs.contains('@') {
            return Ok(ClipEdit { name: name.into(), source: name.into(), range: Some(range(rhs)?), drop: false });
        }
        let (source, r) = match rhs.split_once('@') {
            Some((src, r)) => (src.trim(), Some(range(r)?)),
            None => (rhs, None),
        };
        if source.is_empty() {
            return Err(bad());
        }
        Ok(ClipEdit { name: name.into(), source: source.into(), range: r, drop: false })
    }

    /// Whether this edit's source is the clip named `clip` (`slash`, `Slash`, `preset:biped:slash` alike).
    fn takes(&self, clip: &str) -> bool {
        same_clip(&self.source, clip)
    }
}

/// Clip names compared the way `--animate` names them: `hit_to_body_01` is `HitToBody01`.
pub fn same_clip(a: &str, b: &str) -> bool {
    let n = |s: &str| clip_name(s.rsplit(':').next().unwrap_or(s)).to_lowercase();
    n(a) == n(b)
}

/// `anim` cut to `t0..t1` seconds (its keys inside, plus a sampled key at each
/// end), starting at 0.
pub fn trim_clip(sc: &Scene, anim: &Anim, t0: f32, t1: f32) -> Anim {
    let channels = anim
        .channels
        .iter()
        .filter_map(|c| {
            let n = match c.path {
                Path::T | Path::S => 3,
                Path::R => 4,
                Path::W => sc.nodes[c.node].mesh.and_then(|m| sc.meshes[m].prims.first()).map(|p| p.targets.len()).unwrap_or(0),
            };
            if n == 0 {
                return None;
            }
            let mut times = vec![t0];
            times.extend(c.input.iter().copied().filter(|&t| t > t0 + 1e-4 && t < t1 - 1e-4));
            times.push(t1);
            let mut output = Vec::with_capacity(times.len() * n);
            for &t in &times {
                let v = sample(c, t, n);
                if v.len() != n {
                    return None;
                }
                output.extend(v);
            }
            let interp = if c.interp == "STEP" { "STEP" } else { "LINEAR" };
            Some(glb::Channel { node: c.node, path: c.path, interp: interp.into(), input: times.iter().map(|t| t - t0).collect(), output })
        })
        .collect();
    Anim { name: anim.name.clone(), channels }
}

/// Applies `--clip` edits to the merged clips: trims, renames, new clips cut
/// from part of one (each in its source's place), drops. A clip another one
/// was cut from or renamed to goes, unless an edit keeps it under its own
/// name. Returns notes (a range past the clip's end, clamped).
pub fn edit_clips(sc: &mut Scene, edits: &[ClipEdit]) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    if edits.is_empty() {
        return Ok(notes);
    }
    for e in edits {
        if !sc.anims.iter().any(|a| e.takes(&a.name)) {
            let have: Vec<&str> = sc.anims.iter().map(|a| a.name.as_str()).collect();
            return Err(format!(
                "--clip {}: no clip named {} (it has {})",
                e.name,
                e.source,
                if have.is_empty() { "none".to_string() } else { have.join(", ") }
            ));
        }
    }
    let old = std::mem::take(&mut sc.anims);
    for a in &old {
        let mine: Vec<&ClipEdit> = edits.iter().filter(|e| e.takes(&a.name)).collect();
        if mine.is_empty() {
            sc.anims.push(a.clone());
            continue;
        }
        // The clip itself stays only through an edit under its own name; the
        // others are new clips cut from it, in its place.
        for e in mine.iter().filter(|e| !e.drop) {
            let d = a.duration();
            let mut out = match e.range {
                Some((t0, t1)) => {
                    let (t0, t1) = (t0.unwrap_or(0.0), t1.unwrap_or(d));
                    if t1 > d + 0.01 || t0 >= d {
                        notes.push(format!("--clip {}: {} is {d:.2} s long; cut at {:.2}..{:.2} s", e.name, a.name, t0.min(d), t1.min(d)));
                    }
                    let (t0, t1) = (t0.min(d), t1.min(d));
                    if t1 - t0 < 0.02 {
                        return Err(format!("--clip {}: {t0:.2}..{t1:.2} s leaves nothing of {} ({d:.2} s long)", e.name, a.name));
                    }
                    trim_clip(sc, a, t0, t1)
                }
                None => a.clone(),
            };
            out.name = if same_clip(&e.name, &a.name) { a.name.clone() } else { e.name.clone() };
            sc.anims.push(out);
        }
    }
    Ok(notes)
}

// --- checks -----------------------------------------------------------------------------------

/// What the validator (mb-format) and mb3d's loader would say about a .glb,
/// plus facts for the sidecar. Errors make it unusable; warnings are budgets.
pub fn check(bytes: &[u8]) -> Result<(Value, Vec<String>), String> {
    mb_format::bundle::check_glb(bytes)?;
    let sc = glb::read(bytes)?;
    let mut warnings = Vec::new();
    let mut tex = Vec::new();
    for (i, im) in sc.images.iter().enumerate() {
        let (w, h) = match Image::decode(&im.bytes) {
            Ok(d) => (d.w, d.h),
            Err(e) => return Err(format!("texture {i}: {e}")),
        };
        if w > 2048 || h > 2048 {
            return Err(format!("texture {i} is {w}×{h}; mb3d takes at most 2048×2048"));
        }
        tex.push(json!([w, h]));
    }
    if sc.textures.len() > 16 {
        warnings.push(format!("{} textures: a game has 64 in all (SPEC §5.4)", sc.textures.len()));
    }
    for (i, s) in sc.skins.iter().enumerate() {
        if s.joints.len() > 64 {
            warnings.push(format!("skin {i} has {} joints; mb3d drops skins with more than 64 (the mesh draws unskinned)", s.joints.len()));
        }
    }
    let tris = sc.triangles();
    let verts = sc.vertices();
    if tris > 30_000 {
        warnings.push(format!("{tris} triangles is a lot of a frame's ~150 k budget; pass --polys"));
    }
    if bytes.len() > 3 << 20 {
        warnings.push(format!("{} KB: big for a bundle of 10 MB (startup assets ≤ 3 MB); lower --texture-size", bytes.len() / 1024));
    }
    let clips: Vec<Value> = sc.anims.iter().map(|a| json!({ "name": a.name, "seconds": (a.duration() * 1000.0).round() / 1000.0 })).collect();
    Ok((
        json!({
            "triangles": tris,
            "vertices": verts,
            "meshes": sc.meshes.len(),
            "materials": sc.materials.len(),
            "textures": tex,
            "joints": sc.skins.iter().map(|s| s.joints.len()).max().unwrap_or(0),
            "clips": clips,
            "bytes": bytes.len(),
        }),
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::glb::tests::cube_scene;
    use super::*;

    fn sphere(seg: usize) -> Scene {
        let mut sc = cube_scene();
        let p = &mut sc.meshes[0].prims[0];
        let (mut pos, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..=seg {
            for j in 0..=seg * 2 {
                let (th, ph) = (std::f32::consts::PI * i as f32 / seg as f32, std::f32::consts::TAU * j as f32 / (seg * 2) as f32);
                pos.push([th.sin() * ph.cos() * 2.0, th.cos() * 0.5 + 3.0, th.sin() * ph.sin()]);
                uv.push([j as f32 / (seg * 2) as f32, i as f32 / seg as f32]);
            }
        }
        let row = seg * 2 + 1;
        for i in 0..seg {
            for j in 0..seg * 2 {
                let (a, b, c, d) = (i * row + j, i * row + j + 1, (i + 1) * row + j, (i + 1) * row + j + 1);
                idx.extend([a as u32, c as u32, b as u32, b as u32, c as u32, d as u32]);
            }
        }
        p.nrm = Some(smooth_normals(&pos, &idx));
        p.pos = pos;
        p.uv = Some(uv);
        p.indices = idx;
        sc
    }

    #[test]
    fn places_on_the_ground_facing_minus_z() {
        let mut sc = sphere(8);
        let facts = place(&mut sc, "blob", &Placement { size: 1.0, origin: Origin::Ground, turn: std::f32::consts::PI, lengthwise: true, pre: [0.0, 0.0, 0.0, 1.0] }).unwrap();
        let (lo, hi) = bounds(&posed(&sc, &pose(&sc, None))).unwrap();
        assert!(lo[1].abs() < 1e-4, "on the ground: {lo:?}");
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        assert!((ext[2] - 1.0).abs() < 1e-3, "long side along z after the lengthwise turn: {ext:?}");
        assert!(ext[0] < 0.6 && (lo[0] + hi[0]).abs() < 1e-3);
        assert_eq!(facts["turned_lengthwise"], true);
        assert_eq!(sc.roots.len(), 1);
        assert_eq!(sc.nodes[sc.roots[0]].name, "blob");
    }

    #[test]
    fn simplifies_to_a_budget_and_keeps_uvs() {
        let mut sc = sphere(40);
        let before = sc.triangles();
        let (b, a, _) = fit_triangles(&mut sc, 600);
        assert_eq!(b, before);
        assert!(a <= 600 && a > 200, "{a}");
        let p = &sc.meshes[0].prims[0];
        assert_eq!(p.uv.as_ref().unwrap().len(), p.pos.len());
        assert!(p.indices.iter().all(|&i| (i as usize) < p.pos.len()));
        // Under budget: only welded and tidied.
        let mut small = sphere(4);
        let t = small.triangles();
        assert_eq!(fit_triangles(&mut small, 10_000).1, t);
    }

    #[test]
    fn samples_clips() {
        let mut sc = cube_scene();
        sc.anims.push(Anim {
            name: "Move".into(),
            channels: vec![glb::Channel { node: 0, path: Path::T, interp: "LINEAR".into(), input: vec![0.0, 2.0], output: vec![0.0, 0.0, 0.0, 4.0, 0.0, 0.0] }],
        });
        let p = posed(&sc, &pose(&sc, Some((&sc.anims[0], 1.0))));
        let (lo, _) = bounds(&p).unwrap();
        assert!((lo[0] - 1.5).abs() < 1e-5, "{lo:?}");
        assert_eq!(clip_name("swim"), "Swim");
        assert_eq!(clip_name("run_fast"), "RunFast");
    }

    #[test]
    fn clips_are_trimmed_renamed_cut_and_dropped() {
        let e = |s: &str| ClipEdit::parse(s).unwrap();
        assert_eq!(e("Slash=0.6..3.4"), ClipEdit { name: "Slash".into(), source: "Slash".into(), range: Some((Some(0.6), Some(3.4))), drop: false });
        assert_eq!(e("Strike=slash@1.8..").range, Some((Some(1.8), None)));
        assert_eq!(e("Strike=slash").source, "slash");
        assert!(e("Chop=drop").drop);
        for bad in ["Slash", "Slash=3..1", "=x", "Slash=a..b", "Slash=-1..2"] {
            assert!(ClipEdit::parse(bad).is_err(), "{bad}");
        }
        // A 6.6 s Slash moving x from 0 to 6.6, a 15 s Idle, a Chop and a HitToBody01.
        let mut sc = cube_scene();
        let clip = |name: &str, d: f32| Anim {
            name: name.into(),
            channels: vec![glb::Channel { node: 0, path: Path::T, interp: "LINEAR".into(), input: vec![0.0, d / 2.0, d], output: vec![0.0, 0.0, 0.0, d / 2.0, 0.0, 0.0, d, 0.0, 0.0] }],
        };
        sc.anims = vec![clip("Idle", 15.0), clip("Slash", 6.6), clip("Chop", 6.6), clip("HitToBody01", 1.3)];
        let edits = [e("Idle=0..4"), e("Windup=slash@0.6..1.8"), e("Strike=Slash@1.8..3.4"), e("Chop=drop"), e("hit_to_body_01=..0.9")];
        let notes = edit_clips(&mut sc, &edits).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let names: Vec<(&str, f32)> = sc.anims.iter().map(|a| (a.name.as_str(), (a.duration() * 100.0).round() / 100.0)).collect();
        assert_eq!(names, [("Idle", 4.0), ("Windup", 1.2), ("Strike", 1.6), ("HitToBody01", 0.9)], "Slash went into Windup and Strike, Chop was dropped");
        // The strike starts where the slash was at 1.8 s, and its keys moved to 0.
        let s = &sc.anims[2].channels[0];
        assert!((s.output[0] - 1.8).abs() < 1e-4 && s.input[0] == 0.0, "{s:?}");
        assert!((s.output[s.output.len() - 3] - 3.4).abs() < 1e-4);
        // Past the end: clamped, with a note; a clip that isn't there is an error.
        let notes = edit_clips(&mut sc, &[e("Idle=2..9")]).unwrap();
        assert!(notes[0].contains("4.00 s long"), "{notes:?}");
        assert!(edit_clips(&mut sc, &[e("Kick=front_kick@0..1")]).unwrap_err().contains("no clip named front_kick"));
    }

    #[test]
    fn textures_shrink_and_glow() {
        let mut sc = cube_scene();
        let mut im = Image::new(64, 32, [255, 120, 40, 255]);
        for x in 0..32 {
            for y in 0..32 {
                im.set(x, y, [20, 20, 30, 255]);
            }
        }
        sc.images.push(glb::ImageData { name: String::new(), bytes: im.encode_png(), mime: "image/png".into() });
        sc.textures.push(glb::Texture { image: 0, sampler: None });
        sc.materials[0].base_tex = Some(glb::TexRef { texture: 0, transform: None });
        assert_eq!(add_glow(&mut sc, 3.0, 0.5).unwrap(), 1);
        let facts = fit_textures(&mut sc, 16, 16, TexFormat::Auto).unwrap();
        assert_eq!(facts[0]["size"], json!([16, 8]));
        assert_eq!(facts[0]["format"], "jpeg");
        assert_eq!(facts[1]["role"], "color");
        let glow = Image::decode(&sc.images[1].bytes).unwrap();
        assert!(glow.get(2, 4)[0] < 30 && glow.get(13, 4)[0] > 200, "dark half dark, bright half glows");
        let bytes = glb::write(&sc, "t");
        let (facts, warnings) = check(&bytes).unwrap();
        assert_eq!(facts["textures"], json!([[16, 8], [16, 8]]));
        assert!(warnings.is_empty());
        let back = glb::read(&bytes).unwrap();
        assert_eq!(back.materials[0].emissive_strength, Some(3.0));
    }

    #[test]
    fn maps_get_their_own_size_and_chrome_gets_matte() {
        let mut sc = cube_scene();
        let base = Image::new(64, 64, [200, 60, 40, 255]);
        // A chrome metal-rough map: roughness 0.2 (green), metal 1 (blue).
        let mr = Image::new(64, 64, [255, 51, 255, 255]);
        for im in [&base, &mr, &base] {
            sc.images.push(glb::ImageData { name: String::new(), bytes: im.encode_png(), mime: "image/png".into() });
        }
        for i in 0..3 {
            sc.textures.push(glb::Texture { image: i, sampler: None });
        }
        let m = &mut sc.materials[0];
        (m.metallic, m.roughness) = (1.0, 1.0);
        m.base_tex = Some(glb::TexRef { texture: 0, transform: None });
        m.mr_tex = Some(glb::TexRef { texture: 1, transform: None });
        m.normal_tex = Some(glb::TexRef { texture: 2, transform: None });
        let (metal, rough) = metal_rough(&sc).unwrap();
        assert!(metal > 0.99 && (rough - 0.2).abs() < 0.01, "{metal} {rough}");
        matte(&mut sc, Some(0.15), Some(0.6)).unwrap();
        let (metal, rough) = metal_rough(&sc).unwrap();
        assert!((metal - 0.15).abs() < 0.01 && (rough - 0.6).abs() < 0.01, "{metal} {rough}");
        // The base color keeps 32 px; the metal-rough and normal maps go to 16.
        let facts = fit_textures(&mut sc, 32, 16, TexFormat::Auto).unwrap();
        assert_eq!(facts[0]["size"], json!([32, 32]));
        assert_eq!(facts[1]["size"], json!([16, 16]));
        assert_eq!(facts[2]["size"], json!([16, 16]));
    }

    #[test]
    fn glows_by_color_not_brightness() {
        // A pearl-white part with a lime gem, a shaded (darker) lime band and a yellow stripe:
        // by brightness all the white glows; by color only the lime, shaded or not.
        let mut im = Image::new(40, 10, [235, 232, 225, 255]);
        for y in 0..10 {
            for x in 10..15 {
                im.set(x, y, [91, 202, 43, 255]);
            }
            for x in 20..25 {
                im.set(x, y, [60, 140, 30, 255]);
            }
            for x in 30..35 {
                im.set(x, y, [255, 200, 20, 255]);
            }
        }
        let (bright, share_b) = glow_map(&im, &GlowBy::Brightness(0.6));
        assert!(bright.get(2, 5)[0] > 200 && share_b > 0.8, "brightness lights the white");
        let (m, share) = glow_map(&im, &GlowBy::Colors(vec![[100, 210, 50]], 0.15));
        assert!(m.get(2, 5)[0] == 0 && m.get(37, 5)[0] == 0, "white and the far side stay dark");
        assert!(m.get(32, 5)[1] < 40, "yellow isn't lime: {:?}", m.get(32, 5));
        assert!(m.get(12, 5)[1] > 200, "the gem glows: {:?}", m.get(12, 5));
        assert!(m.get(22, 5)[1] > 100, "so does its shaded band: {:?}", m.get(22, 5));
        assert!((0.25..0.45).contains(&share), "{share}");
        // Two colors: lime and yellow.
        let (m2, _) = glow_map(&im, &GlowBy::Colors(vec![[100, 210, 50], [255, 200, 20]], 0.15));
        assert!(m2.get(32, 5)[0] > 200);
    }
}
