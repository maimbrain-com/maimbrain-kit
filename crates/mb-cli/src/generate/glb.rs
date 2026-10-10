//! A small binary glTF (.glb) reader and writer for `mb model`: the parts
//! mb3d draws (SPEC §5.4 "glTF") in a plain in-memory scene that the
//! post-processing in `shape.rs` can change, written back as one tidy .glb.
//!
//! Read: triangle meshes (lists, strips and fans; float or integer
//! attributes, normalized or not; sparse accessors), POSITION, NORMAL,
//! TANGENT, TEXCOORD_0, COLOR_0, JOINTS_0/WEIGHTS_0 and morph targets
//! (POSITION/NORMAL), metallic-roughness materials, images (PNG, JPEG),
//! node hierarchies (TRS or matrix), skins and animations. Anything else
//! (cameras, lights, other UV sets, occlusion maps, unknown extensions) is
//! dropped, and each kind of drop is listed in `Scene::dropped` so the
//! sidecar can say what was removed. A file that needs a decoder mb doesn't
//! have (Draco, meshopt, KTX2, WebP) is refused with a message saying so.
//!
//! Written: one buffer, float attributes (u16 joints), u16 or u32 indices,
//! images embedded in buffer views, 4-byte aligned chunks, only extensions
//! mb3d supports. `write_quantized` stores the geometry in about half the
//! bytes instead (KHR_mesh_quantization, which mb3d reads on the web and on
//! iOS): see [`write_quantized`].

use std::collections::BTreeSet;

use serde_json::{Value, json};

pub type Mat4 = [f32; 16];

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub nodes: Vec<Node>,
    /// The default scene's root nodes.
    pub roots: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub images: Vec<ImageData>,
    pub skins: Vec<Skin>,
    pub anims: Vec<Anim>,
    /// What reading left out ("KHR_materials_specular", "occlusion map", "camera"…).
    pub dropped: BTreeSet<String>,
    /// The `asset.generator` string, if any.
    pub generator: String,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub t: [f32; 3],
    pub r: [f32; 4],
    pub s: [f32; 3],
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
}

impl Default for Node {
    fn default() -> Node {
        Node { name: String::new(), t: [0.0; 3], r: [0.0, 0.0, 0.0, 1.0], s: [1.0; 3], children: Vec::new(), mesh: None, skin: None }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub name: String,
    pub prims: Vec<Prim>,
    /// Default morph weights.
    pub weights: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct Target {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Option<Vec<[f32; 3]>>,
}

#[derive(Clone, Debug, Default)]
pub struct Prim {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Option<Vec<[f32; 3]>>,
    pub tan: Option<Vec<[f32; 4]>>,
    pub uv: Option<Vec<[f32; 2]>>,
    pub color: Option<Vec<[f32; 4]>>,
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
    pub targets: Vec<Target>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

impl Prim {
    pub fn tris(&self) -> usize {
        self.indices.len() / 3
    }
}

#[derive(Clone, Debug)]
pub struct TexRef {
    pub texture: usize,
    /// KHR_texture_transform, kept on base color only (all mb3d reads).
    pub transform: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub base: [f32; 4],
    pub base_tex: Option<TexRef>,
    pub metallic: f32,
    pub roughness: f32,
    pub mr_tex: Option<TexRef>,
    pub normal_tex: Option<TexRef>,
    pub normal_scale: f32,
    pub emissive: [f32; 3],
    pub emissive_tex: Option<TexRef>,
    pub emissive_strength: Option<f32>,
    /// "OPAQUE", "MASK" or "BLEND".
    pub alpha_mode: String,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub unlit: bool,
}

impl Default for Material {
    fn default() -> Material {
        Material {
            name: String::new(),
            base: [1.0; 4],
            base_tex: None,
            metallic: 1.0,
            roughness: 1.0,
            mr_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            emissive: [0.0; 3],
            emissive_tex: None,
            emissive_strength: None,
            alpha_mode: "OPAQUE".into(),
            alpha_cutoff: 0.5,
            double_sided: false,
            unlit: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub image: usize,
    /// glTF sampler filters and wraps, as given (mb3d samples with repeat and trilinear anyway).
    pub sampler: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ImageData {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mime: String,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub name: String,
    pub joints: Vec<usize>,
    pub ibm: Vec<Mat4>,
    pub skeleton: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    T,
    R,
    S,
    W,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    /// "LINEAR", "STEP" or "CUBICSPLINE".
    pub interp: String,
    pub input: Vec<f32>,
    /// Flat values: components per key = 3 (T, S), 4 (R) or the morph target count (W),
    /// times 3 for CUBICSPLINE (in-tangent, value, out-tangent).
    pub output: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct Anim {
    pub name: String,
    pub channels: Vec<Channel>,
}

impl Anim {
    pub fn duration(&self) -> f32 {
        self.channels.iter().filter_map(|c| c.input.last().copied()).fold(0.0, f32::max)
    }
}

/// Extensions mb3d reads (SPEC §5.4); everything else is dropped on write.
pub const MB3D_EXTENSIONS: &[&str] = &[
    "EXT_meshopt_compression",
    "KHR_meshopt_compression",
    "KHR_mesh_quantization",
    "KHR_texture_transform",
    "KHR_materials_emissive_strength",
    "KHR_materials_unlit",
];

// --- reading ---------------------------------------------------------------------

struct Doc<'a> {
    j: &'a Value,
    buffers: Vec<Vec<u8>>,
}

fn u(v: &Value) -> Option<usize> {
    v.as_u64().map(|x| x as usize)
}

fn f(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn ncomp(t: &str) -> Option<usize> {
    Some(match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => return None,
    })
}

fn csize(ct: u64) -> Option<usize> {
    Some(match ct {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    })
}

fn read_comp(b: &[u8], ct: u64, normalized: bool) -> f32 {
    match ct {
        5120 => {
            let v = b[0] as i8 as f32;
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = b[0] as f32;
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let v = i16::from_le_bytes([b[0], b[1]]) as f32;
            if normalized { (v / 32767.0).max(-1.0) } else { v }
        }
        5123 => {
            let v = u16::from_le_bytes([b[0], b[1]]) as f32;
            if normalized { v / 65535.0 } else { v }
        }
        5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
        _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

fn read_u32(b: &[u8], ct: u64) -> u32 {
    match ct {
        5121 => b[0] as u32,
        5123 => u16::from_le_bytes([b[0], b[1]]) as u32,
        5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        _ => read_comp(b, ct, false) as u32,
    }
}

impl Doc<'_> {
    /// The bytes of a buffer view, and its stride (0 if tightly packed).
    fn view(&self, i: usize) -> Result<(&[u8], usize), String> {
        let v = self.j["bufferViews"].get(i).ok_or(format!("bufferView {i} is missing"))?;
        if v["extensions"].get("EXT_meshopt_compression").is_some() || v["extensions"].get("KHR_meshopt_compression").is_some() {
            return Err("the file is meshopt-compressed; mb model reads uncompressed glTF (ask the provider for a plain .glb)".into());
        }
        let buf = self.buffers.get(u(&v["buffer"]).unwrap_or(0)).ok_or(format!("bufferView {i}: no such buffer"))?;
        let off = u(&v["byteOffset"]).unwrap_or(0);
        let len = u(&v["byteLength"]).ok_or(format!("bufferView {i} has no byteLength"))?;
        let b = buf.get(off..off + len).ok_or(format!("bufferView {i} runs past its buffer"))?;
        Ok((b, u(&v["byteStride"]).unwrap_or(0)))
    }

    /// An accessor as raw element bytes: (bytes per component, comps, count, component type, normalized, fetch).
    fn walk<T>(&self, i: usize, mut each: impl FnMut(usize, &[u8], u64, bool, usize) -> T) -> Result<(usize, usize), String> {
        let a = self.j["accessors"].get(i).ok_or(format!("accessor {i} is missing"))?;
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let cs = csize(ct).ok_or(format!("accessor {i}: bad componentType"))?;
        let n = ncomp(a["type"].as_str().unwrap_or("")).ok_or(format!("accessor {i}: bad type"))?;
        let count = u(&a["count"]).ok_or(format!("accessor {i}: no count"))?;
        let norm = a["normalized"].as_bool().unwrap_or(false);
        if let Some(vi) = u(&a["bufferView"]) {
            let (b, stride) = self.view(vi)?;
            let off = u(&a["byteOffset"]).unwrap_or(0);
            let elem = cs * n;
            // Matrices pad columns to 4 bytes for 1- and 2-byte components; mb only reads float matrices.
            let stride = if stride == 0 { elem } else { stride };
            if count > 0 && off + stride * (count - 1) + elem > b.len() {
                return Err(format!("accessor {i} runs past its bufferView"));
            }
            for k in 0..count {
                for c in 0..n {
                    let at = off + k * stride + c * cs;
                    each(k * n + c, &b[at..at + cs], ct, norm, cs);
                }
            }
        }
        Ok((n, count))
    }

    fn floats(&self, i: usize) -> Result<(Vec<f32>, usize), String> {
        let a = &self.j["accessors"][i];
        let n = ncomp(a["type"].as_str().unwrap_or("")).unwrap_or(1);
        let count = u(&a["count"]).unwrap_or(0);
        let mut out = vec![0.0f32; n * count];
        self.walk(i, |k, b, ct, norm, _| out[k] = read_comp(b, ct, norm))?;
        self.sparse(i, n, &mut out, |b, ct, norm| read_comp(b, ct, norm))?;
        Ok((out, n))
    }

    fn uints(&self, i: usize) -> Result<Vec<u32>, String> {
        let a = &self.j["accessors"][i];
        let n = ncomp(a["type"].as_str().unwrap_or("")).unwrap_or(1);
        let count = u(&a["count"]).unwrap_or(0);
        let mut out = vec![0u32; n * count];
        self.walk(i, |k, b, ct, _, _| out[k] = read_u32(b, ct))?;
        self.sparse(i, n, &mut out, |b, ct, _| read_u32(b, ct))?;
        Ok(out)
    }

    fn sparse<T: Copy>(&self, i: usize, n: usize, out: &mut [T], conv: impl Fn(&[u8], u64, bool) -> T) -> Result<(), String> {
        let a = &self.j["accessors"][i];
        let sp = &a["sparse"];
        if sp.is_null() {
            return Ok(());
        }
        let count = u(&sp["count"]).unwrap_or(0);
        let ict = sp["indices"]["componentType"].as_u64().unwrap_or(5125);
        let ics = csize(ict).ok_or("sparse indices: bad componentType")?;
        let (ib, _) = self.view(u(&sp["indices"]["bufferView"]).ok_or("sparse indices: no bufferView")?)?;
        let ioff = u(&sp["indices"]["byteOffset"]).unwrap_or(0);
        let ct = a["componentType"].as_u64().unwrap_or(5126);
        let cs = csize(ct).ok_or("sparse: bad componentType")?;
        let norm = a["normalized"].as_bool().unwrap_or(false);
        let (vb, _) = self.view(u(&sp["values"]["bufferView"]).ok_or("sparse values: no bufferView")?)?;
        let voff = u(&sp["values"]["byteOffset"]).unwrap_or(0);
        for k in 0..count {
            let ia = ioff + k * ics;
            let idx = read_u32(ib.get(ia..ia + ics).ok_or("sparse indices run past their view")?, ict) as usize;
            for c in 0..n {
                let va = voff + (k * n + c) * cs;
                let slot = out.get_mut(idx * n + c).ok_or("sparse index out of range")?;
                *slot = conv(vb.get(va..va + cs).ok_or("sparse values run past their view")?, ct, norm);
            }
        }
        Ok(())
    }

    fn vecs<const N: usize>(&self, i: usize) -> Result<Vec<[f32; N]>, String> {
        let (d, c) = self.floats(i)?;
        if c < N {
            return Err(format!("accessor {i} has {c} components, wanted {N}"));
        }
        Ok(d.chunks(c).map(|e| std::array::from_fn(|k| e[k])).collect())
    }
}

/// Splits a .glb into its JSON and BIN chunks.
pub fn split_glb(b: &[u8]) -> Result<(Value, Vec<u8>), String> {
    let u32at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
    if b.len() < 20 || &b[..4] != b"glTF" {
        return Err("not a binary glTF (.glb)".into());
    }
    if u32at(4) != Some(2) {
        return Err("only glTF 2.0 is supported".into());
    }
    let jl = u32at(12).unwrap_or(0);
    if u32at(16) != Some(0x4E4F_534A) || 20 + jl > b.len() {
        return Err("the first chunk must be JSON".into());
    }
    let json: Value = serde_json::from_slice(&b[20..20 + jl]).map_err(|e| format!("the glTF JSON doesn't parse: {e}"))?;
    let mut bin = Vec::new();
    let mut at = 20 + jl;
    while at + 8 <= b.len() {
        let n = u32at(at).unwrap_or(0);
        let kind = u32at(at + 4).unwrap_or(0);
        let end = (at + 8 + n).min(b.len());
        if kind == 0x004E_4942 && bin.is_empty() {
            bin = b[at + 8..end].to_vec();
        }
        at += 8 + n;
    }
    Ok((json, bin))
}

fn data_uri(s: &str) -> Option<Vec<u8>> {
    let (_, data) = s.strip_prefix("data:")?.split_once(";base64,")?;
    super::provider::unb64(data).ok()
}

pub fn read(bytes: &[u8]) -> Result<Scene, String> {
    let (j, bin) = split_glb(bytes)?;
    for e in arr(&j["extensionsRequired"]) {
        let name = e.as_str().unwrap_or("");
        let why = match name {
            "KHR_draco_mesh_compression" => "Draco-compressed meshes",
            "EXT_meshopt_compression" | "KHR_meshopt_compression" => "meshopt-compressed buffers",
            "KHR_texture_basisu" => "KTX2/Basis textures",
            "EXT_texture_webp" => "WebP textures",
            "KHR_mesh_quantization" => continue,
            "KHR_texture_transform" | "KHR_materials_emissive_strength" | "KHR_materials_unlit" => continue,
            _ => "an extension mb can't read",
        };
        return Err(format!("the model requires {name} ({why}); ask the provider for an uncompressed .glb with PNG or JPEG textures"));
    }
    let mut buffers = Vec::new();
    for (i, b) in arr(&j["buffers"]).iter().enumerate() {
        match b["uri"].as_str() {
            None => buffers.push(bin.clone()),
            Some(s) => buffers.push(data_uri(s).ok_or(format!("buffer {i} points at an external file ({s}); a .glb must embed everything"))?),
        }
    }
    let doc = Doc { j: &j, buffers };
    let mut sc = Scene { generator: j["asset"]["generator"].as_str().unwrap_or("").to_string(), ..Scene::default() };
    for e in arr(&j["extensionsUsed"]) {
        let name = e.as_str().unwrap_or("");
        if !MB3D_EXTENSIONS.contains(&name) {
            sc.dropped.insert(name.to_string());
        }
    }
    if !arr(&j["cameras"]).is_empty() {
        sc.dropped.insert("cameras".into());
    }

    // Images and textures.
    for (i, im) in arr(&j["images"]).iter().enumerate() {
        let bytes = if let Some(v) = u(&im["bufferView"]) {
            doc.view(v)?.0.to_vec()
        } else if let Some(d) = im["uri"].as_str().and_then(data_uri) {
            d
        } else {
            return Err(format!("image {i} isn't embedded in the .glb"));
        };
        let mime = if bytes.starts_with(b"\x89PNG") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            "image/jpeg"
        } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
            return Err("the model has WebP textures, which mb3d can't load; ask the provider for PNG or JPEG".into());
        } else {
            return Err(format!("image {i} is neither PNG nor JPEG"));
        };
        sc.images.push(ImageData { name: im["name"].as_str().unwrap_or("").into(), bytes, mime: mime.into() });
    }
    for t in arr(&j["textures"]) {
        let src = u(&t["source"]);
        let Some(image) = src else {
            // Only an extension's source (KTX2, WebP): unusable.
            sc.textures.push(Texture { image: usize::MAX, sampler: None });
            continue;
        };
        let sampler = u(&t["sampler"]).and_then(|s| j["samplers"].get(s)).cloned();
        sc.textures.push(Texture { image, sampler });
    }
    let texref = |v: &Value, what: &str, dropped: &mut BTreeSet<String>, keep_transform: bool| -> Option<TexRef> {
        let t = u(&v["index"])?;
        if u(&v["texCoord"]).unwrap_or(0) != 0 {
            dropped.insert(format!("{what} on a second UV set"));
            return None;
        }
        let transform = v["extensions"].get("KHR_texture_transform").cloned();
        if transform.is_some() && !keep_transform {
            dropped.insert(format!("texture transform on the {what}"));
        }
        Some(TexRef { texture: t, transform: if keep_transform { transform } else { None } })
    };
    for m in arr(&j["materials"]) {
        let pbr = &m["pbrMetallicRoughness"];
        let mut mat = Material { name: m["name"].as_str().unwrap_or("").into(), ..Material::default() };
        if let Some(c) = pbr["baseColorFactor"].as_array() {
            for k in 0..4 {
                mat.base[k] = f(&c[k], 1.0);
            }
        }
        mat.base_tex = texref(&pbr["baseColorTexture"], "base color", &mut sc.dropped, true);
        mat.metallic = f(&pbr["metallicFactor"], 1.0);
        mat.roughness = f(&pbr["roughnessFactor"], 1.0);
        mat.mr_tex = texref(&pbr["metallicRoughnessTexture"], "metal-rough map", &mut sc.dropped, false);
        mat.normal_tex = texref(&m["normalTexture"], "normal map", &mut sc.dropped, false);
        mat.normal_scale = f(&m["normalTexture"]["scale"], 1.0);
        if !m["occlusionTexture"].is_null() {
            sc.dropped.insert("occlusion maps (mb3d doesn't use them)".into());
        }
        if let Some(c) = m["emissiveFactor"].as_array() {
            for k in 0..3 {
                mat.emissive[k] = f(&c[k], 0.0);
            }
        }
        mat.emissive_tex = texref(&m["emissiveTexture"], "emissive map", &mut sc.dropped, false);
        mat.emissive_strength = m["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"].as_f64().map(|x| x as f32);
        mat.unlit = m["extensions"].get("KHR_materials_unlit").is_some();
        mat.alpha_mode = m["alphaMode"].as_str().unwrap_or("OPAQUE").to_string();
        mat.alpha_cutoff = f(&m["alphaCutoff"], 0.5);
        mat.double_sided = m["doubleSided"].as_bool().unwrap_or(false);
        sc.materials.push(mat);
    }

    // Meshes.
    for (mi, m) in arr(&j["meshes"]).iter().enumerate() {
        let mut mesh = Mesh { name: m["name"].as_str().unwrap_or("").into(), ..Mesh::default() };
        mesh.weights = arr(&m["weights"]).iter().map(|w| f(w, 0.0)).collect();
        for p in arr(&m["primitives"]) {
            let mode = u(&p["mode"]).unwrap_or(4);
            if !(4..=6).contains(&mode) {
                sc.dropped.insert("points and lines".into());
                continue;
            }
            if p["extensions"].get("KHR_draco_mesh_compression").is_some() && p["attributes"]["POSITION"].is_null() {
                return Err("the model is Draco-compressed; ask the provider for an uncompressed .glb".into());
            }
            let at = &p["attributes"];
            let Some(pi) = u(&at["POSITION"]) else { continue };
            let pos = doc.vecs::<3>(pi)?;
            let n = pos.len();
            let get3 = |name: &str| -> Result<Option<Vec<[f32; 3]>>, String> { u(&at[name]).map(|i| doc.vecs::<3>(i)).transpose() };
            let nrm = get3("NORMAL")?;
            let tan = u(&at["TANGENT"]).map(|i| doc.vecs::<4>(i)).transpose()?;
            let uv = u(&at["TEXCOORD_0"]).map(|i| doc.vecs::<2>(i)).transpose()?;
            let color = match u(&at["COLOR_0"]) {
                Some(i) => {
                    let (d, c) = doc.floats(i)?;
                    Some(d.chunks(c).map(|e| [e[0], e[1], e[2], if c > 3 { e[3] } else { 1.0 }]).collect::<Vec<_>>())
                }
                None => None,
            };
            let joints = match u(&at["JOINTS_0"]) {
                Some(i) => Some(doc.uints(i)?.chunks(4).map(|e| [e[0] as u16, e[1] as u16, e[2] as u16, e[3] as u16]).collect::<Vec<_>>()),
                None => None,
            };
            let weights = u(&at["WEIGHTS_0"]).map(|i| doc.vecs::<4>(i)).transpose()?;
            for (k, _) in at.as_object().into_iter().flatten() {
                if !["POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "COLOR_0", "JOINTS_0", "WEIGHTS_0"].contains(&k.as_str()) {
                    sc.dropped.insert(format!("{k} attributes"));
                }
            }
            for (name, len) in [("NORMAL", nrm.as_ref().map(Vec::len)), ("TEXCOORD_0", uv.as_ref().map(Vec::len)), ("JOINTS_0", joints.as_ref().map(Vec::len))] {
                if len.is_some_and(|l| l != n) {
                    return Err(format!("mesh {mi}: {name} has a different count from POSITION"));
                }
            }
            let mut targets = Vec::new();
            for t in arr(&p["targets"]) {
                let Some(tp) = u(&t["POSITION"]) else {
                    targets.push(Target { pos: vec![[0.0; 3]; n], nrm: None });
                    continue;
                };
                targets.push(Target { pos: doc.vecs::<3>(tp)?, nrm: u(&t["NORMAL"]).map(|i| doc.vecs::<3>(i)).transpose()? });
            }
            let raw: Vec<u32> = match u(&p["indices"]) {
                Some(i) => doc.uints(i)?,
                None => (0..n as u32).collect(),
            };
            let mut indices = match mode {
                5 => (2..raw.len())
                    .map(|k| if k % 2 == 0 { [raw[k - 2], raw[k - 1], raw[k]] } else { [raw[k - 1], raw[k - 2], raw[k]] })
                    .flatten()
                    .collect(),
                6 => (2..raw.len()).flat_map(|k| [raw[0], raw[k - 1], raw[k]]).collect(),
                _ => raw,
            };
            indices.truncate(indices.len() / 3 * 3);
            if indices.iter().any(|&i| i as usize >= n) {
                return Err(format!("mesh {mi}: an index is out of range"));
            }
            mesh.prims.push(Prim { pos, nrm, tan, uv, color, joints, weights, targets, indices, material: u(&p["material"]) });
        }
        sc.meshes.push(mesh);
    }

    // Nodes.
    for n in arr(&j["nodes"]) {
        let mut node = Node { name: n["name"].as_str().unwrap_or("").into(), ..Node::default() };
        if let Some(m) = n["matrix"].as_array() {
            let mut mm = [0.0f32; 16];
            for k in 0..16 {
                mm[k] = f(&m[k], 0.0);
            }
            let (t, r, s) = decompose(&mm);
            (node.t, node.r, node.s) = (t, r, s);
        } else {
            if let Some(t) = n["translation"].as_array() {
                node.t = [f(&t[0], 0.0), f(&t[1], 0.0), f(&t[2], 0.0)];
            }
            if let Some(r) = n["rotation"].as_array() {
                node.r = [f(&r[0], 0.0), f(&r[1], 0.0), f(&r[2], 0.0), f(&r[3], 1.0)];
            }
            if let Some(s) = n["scale"].as_array() {
                node.s = [f(&s[0], 1.0), f(&s[1], 1.0), f(&s[2], 1.0)];
            }
        }
        node.children = arr(&n["children"]).iter().filter_map(u).collect();
        node.mesh = u(&n["mesh"]);
        node.skin = u(&n["skin"]);
        if !n["camera"].is_null() {
            sc.dropped.insert("cameras".into());
        }
        if n["extensions"].get("KHR_lights_punctual").is_some() {
            sc.dropped.insert("lights".into());
        }
        sc.nodes.push(node);
    }
    let scene = u(&j["scene"]).unwrap_or(0);
    sc.roots = match j["scenes"].get(scene) {
        Some(s) => arr(&s["nodes"]).iter().filter_map(u).collect(),
        // No scene: every node that isn't a child is a root.
        None => {
            let kids: BTreeSet<usize> = sc.nodes.iter().flat_map(|n| n.children.iter().copied()).collect();
            (0..sc.nodes.len()).filter(|i| !kids.contains(i)).collect()
        }
    };

    for s in arr(&j["skins"]) {
        let joints: Vec<usize> = arr(&s["joints"]).iter().filter_map(u).collect();
        let ibm = match u(&s["inverseBindMatrices"]) {
            Some(i) => doc.vecs::<16>(i)?,
            None => vec![identity(); joints.len()],
        };
        if ibm.len() < joints.len() {
            return Err("a skin has fewer inverse bind matrices than joints".into());
        }
        sc.skins.push(Skin { name: s["name"].as_str().unwrap_or("").into(), joints, ibm, skeleton: u(&s["skeleton"]) });
    }

    for (ai, a) in arr(&j["animations"]).iter().enumerate() {
        let samplers = arr(&a["samplers"]);
        let mut anim = Anim { name: a["name"].as_str().map(str::to_string).unwrap_or_else(|| format!("clip{ai}")), channels: Vec::new() };
        for c in arr(&a["channels"]) {
            let Some(node) = u(&c["target"]["node"]) else { continue };
            let path = match c["target"]["path"].as_str() {
                Some("translation") => Path::T,
                Some("rotation") => Path::R,
                Some("scale") => Path::S,
                Some("weights") => Path::W,
                _ => continue,
            };
            let Some(sm) = u(&c["sampler"]).and_then(|k| samplers.get(k)) else { continue };
            let input = doc.floats(u(&sm["input"]).ok_or("an animation sampler has no input")?)?.0;
            let output = doc.floats(u(&sm["output"]).ok_or("an animation sampler has no output")?)?.0;
            anim.channels.push(Channel { node, path, interp: sm["interpolation"].as_str().unwrap_or("LINEAR").to_string(), input, output });
        }
        sc.anims.push(anim);
    }
    sc.check_refs()?;
    Ok(sc)
}

impl Scene {
    fn check_refs(&self) -> Result<(), String> {
        let nn = self.nodes.len();
        for (i, n) in self.nodes.iter().enumerate() {
            if n.children.iter().any(|&c| c >= nn || c == i) || n.mesh.is_some_and(|m| m >= self.meshes.len()) || n.skin.is_some_and(|s| s >= self.skins.len()) {
                return Err(format!("node {i} refers to something that doesn't exist"));
            }
        }
        if self.roots.iter().any(|&r| r >= nn) {
            return Err("the scene refers to a node that doesn't exist".into());
        }
        for s in &self.skins {
            if s.joints.iter().any(|&j| j >= nn) {
                return Err("a skin refers to a node that doesn't exist".into());
            }
        }
        for m in &self.meshes {
            if m.prims.iter().any(|p| p.material.is_some_and(|k| k >= self.materials.len())) {
                return Err("a mesh refers to a material that doesn't exist".into());
            }
        }
        for t in &self.textures {
            if t.image != usize::MAX && t.image >= self.images.len() {
                return Err("a texture refers to an image that doesn't exist".into());
            }
        }
        for a in &self.anims {
            if a.channels.iter().any(|c| c.node >= nn) {
                return Err(format!("animation {:?} targets a node that doesn't exist", a.name));
            }
        }
        Ok(())
    }

    pub fn triangles(&self) -> usize {
        self.drawn_meshes().map(|m| self.meshes[m].prims.iter().map(Prim::tris).sum::<usize>()).sum()
    }

    pub fn vertices(&self) -> usize {
        self.drawn_meshes().map(|m| self.meshes[m].prims.iter().map(|p| p.pos.len()).sum::<usize>()).sum()
    }

    /// Mesh indices reachable from the scene (each once per node that draws it).
    pub fn drawn_meshes(&self) -> impl Iterator<Item = usize> + '_ {
        self.walk().into_iter().filter_map(|(n, _)| self.nodes[n].mesh)
    }

    /// The scene's nodes, parents first, with their parent.
    pub fn walk(&self) -> Vec<(usize, Option<usize>)> {
        let mut out = Vec::new();
        let mut seen = vec![false; self.nodes.len()];
        let mut stack: Vec<(usize, Option<usize>)> = self.roots.iter().rev().map(|&r| (r, None)).collect();
        while let Some((n, p)) = stack.pop() {
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            out.push((n, p));
            for &c in self.nodes[n].children.iter().rev() {
                stack.push((c, Some(n)));
            }
        }
        out
    }

    /// Node world matrices for local transforms `local` (index = node).
    pub fn world(&self, local: &[Mat4]) -> Vec<Mat4> {
        let mut w = vec![identity(); self.nodes.len()];
        for (n, p) in self.walk() {
            w[n] = match p {
                Some(p) => mul(&w[p], &local[n]),
                None => local[n],
            };
        }
        w
    }

    #[cfg(test)]
    pub fn rest_locals(&self) -> Vec<Mat4> {
        self.nodes.iter().map(|n| trs(n.t, n.r, n.s)).collect()
    }

    pub fn texture_image(&self, t: &TexRef) -> Option<usize> {
        self.textures.get(t.texture).map(|t| t.image).filter(|&i| i < self.images.len())
    }

    /// Removes images and textures nothing uses, and renumbers the rest.
    pub fn prune(&mut self) {
        let mut tex_used = vec![false; self.textures.len()];
        for m in &self.materials {
            for t in [&m.base_tex, &m.mr_tex, &m.normal_tex, &m.emissive_tex].into_iter().flatten() {
                if t.texture < tex_used.len() && self.textures[t.texture].image < self.images.len() {
                    tex_used[t.texture] = true;
                }
            }
        }
        let mut tmap = vec![usize::MAX; self.textures.len()];
        let mut textures = Vec::new();
        let mut img_map = vec![usize::MAX; self.images.len()];
        let mut images = Vec::new();
        for (i, t) in self.textures.iter().enumerate() {
            if !tex_used[i] {
                continue;
            }
            if img_map[t.image] == usize::MAX {
                img_map[t.image] = images.len();
                images.push(self.images[t.image].clone());
            }
            tmap[i] = textures.len();
            textures.push(Texture { image: img_map[t.image], sampler: t.sampler.clone() });
        }
        for m in &mut self.materials {
            for t in [&mut m.base_tex, &mut m.mr_tex, &mut m.normal_tex, &mut m.emissive_tex] {
                if let Some(r) = t {
                    if r.texture < tmap.len() && tmap[r.texture] != usize::MAX {
                        r.texture = tmap[r.texture];
                    } else {
                        *t = None;
                    }
                }
            }
        }
        self.textures = textures;
        self.images = images;
    }
}

// --- writing ----------------------------------------------------------------------

struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut v = json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.data.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn floats(&mut self, d: &[f32], n: usize, ty: &str, bounds: bool, target: Option<u32>) -> usize {
        let bytes: Vec<u8> = d.iter().flat_map(|x| x.to_le_bytes()).collect();
        let v = self.view(&bytes, target);
        let mut a = json!({ "bufferView": v, "componentType": 5126, "count": d.len() / n, "type": ty });
        if bounds && !d.is_empty() {
            let mut lo = vec![f32::MAX; n];
            let mut hi = vec![f32::MIN; n];
            for e in d.chunks(n) {
                for k in 0..n {
                    lo[k] = lo[k].min(e[k]);
                    hi[k] = hi[k].max(e[k]);
                }
            }
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn indices(&mut self, idx: &[u32], verts: usize) -> usize {
        let (bytes, ct): (Vec<u8>, u32) = if verts <= 65535 {
            (idx.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect(), 5123)
        } else {
            (idx.iter().flat_map(|i| i.to_le_bytes()).collect(), 5125)
        };
        let v = self.view(&bytes, Some(34963));
        self.accessors.push(json!({ "bufferView": v, "componentType": ct, "count": idx.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }

    fn joints(&mut self, j: &[[u16; 4]], small: bool) -> usize {
        // u8 when every joint index fits (and the file is quantized anyway).
        let (bytes, ct): (Vec<u8>, u32) = if small && j.iter().flatten().all(|&x| x < 256) {
            (j.iter().flatten().map(|&x| x as u8).collect(), 5121)
        } else {
            (j.iter().flatten().flat_map(|x| x.to_le_bytes()).collect(), 5123)
        };
        let v = self.view(&bytes, Some(34962));
        self.accessors.push(json!({ "bufferView": v, "componentType": ct, "count": j.len(), "type": "VEC4" }));
        self.accessors.len() - 1
    }

    /// Normalized integers: `ct` 5120 (i8), 5121 (u8), 5122 (i16) or 5123 (u16); each
    /// element padded to `stride` bytes (vertex attributes keep 4-byte alignment).
    fn normalized(&mut self, d: &[f32], n: usize, ty: &str, ct: u32, stride: usize, bounds: bool, target: Option<u32>) -> usize {
        let (size, scale) = match ct {
            5120 => (1, 127.0),
            5121 => (1, 255.0),
            5122 => (2, 32767.0),
            _ => (2, 65535.0),
        };
        let q: Vec<i32> = d.iter().map(|&x| (x * scale).round().clamp(if ct == 5120 || ct == 5122 { -scale } else { 0.0 }, scale) as i32).collect();
        let mut bytes = Vec::with_capacity(d.len() / n * stride);
        for e in q.chunks(n) {
            let start = bytes.len();
            for &x in e {
                if size == 1 {
                    bytes.push(x as u8);
                } else {
                    bytes.extend_from_slice(&(x as u16).to_le_bytes());
                }
            }
            bytes.resize(start + stride.max(n * size), 0);
        }
        let v = self.view(&bytes, target);
        if stride > n * size {
            self.views[v]["byteStride"] = json!(stride);
        }
        let mut a = json!({ "bufferView": v, "componentType": ct, "normalized": true, "count": d.len() / n, "type": ty });
        if bounds && !q.is_empty() {
            let lo: Vec<i32> = (0..n).map(|k| q.iter().skip(k).step_by(n).copied().min().unwrap()).collect();
            let hi: Vec<i32> = (0..n).map(|k| q.iter().skip(k).step_by(n).copied().max().unwrap()).collect();
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
}


/// The dequantizing offset and scale of each mesh whose positions are
/// quantized, and the skins whose inverse bind matrices take it.
fn quant_plan(sc: &Scene) -> (Vec<Option<([f32; 3], f32)>>, Vec<Option<usize>>) {
    let mut plan = vec![None; sc.meshes.len()];
    // Which skins each mesh is drawn with, and which meshes each skin draws.
    let mut mesh_skins: Vec<BTreeSet<Option<usize>>> = vec![BTreeSet::new(); sc.meshes.len()];
    let mut skin_meshes: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); sc.skins.len()];
    for n in &sc.nodes {
        if let Some(m) = n.mesh {
            mesh_skins[m].insert(n.skin);
            if let Some(s) = n.skin {
                skin_meshes[s].insert(m);
            }
        }
    }
    let mut skin_of = vec![None; sc.meshes.len()];
    for (mi, m) in sc.meshes.iter().enumerate() {
        if m.prims.is_empty() || m.prims.iter().any(|p| !p.targets.is_empty() || p.pos.is_empty()) {
            continue;
        }
        let skins: Vec<Option<usize>> = mesh_skins[mi].iter().copied().collect();
        // Drawn unskinned only, or by exactly one skin that draws only this mesh.
        let skin = match skins.as_slice() {
            [] | [None] => None,
            [Some(s)] if skin_meshes[*s].len() == 1 => Some(*s),
            _ => continue,
        };
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in m.prims.iter().flat_map(|p| &p.pos) {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let s = (0..3).map(|k| (hi[k] - lo[k]) / 2.0).fold(0.0f32, f32::max);
        if !(s > 1e-12 && s.is_finite()) {
            continue;
        }
        plan[mi] = Some((c, s));
        skin_of[mi] = skin;
    }
    (plan, skin_of)
}

/// `q`'s rotation applied to `v`.
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let w = q[3];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let t = cross(u, v);
    let t = [t[0] + w * v[0], t[1] + w * v[1], t[2] + w * v[2]];
    let c = cross(u, t);
    [v[0] + 2.0 * c[0], v[1] + 2.0 * c[1], v[2] + 2.0 * c[2]]
}

fn flat<const N: usize>(v: &[[f32; N]]) -> Vec<f32> {
    v.iter().flatten().copied().collect()
}

/// The scene as a .glb with float geometry.
pub fn write(sc: &Scene, generator: &str) -> Vec<u8> {
    write_with(sc, generator, false)
}

/// The scene as a .glb with quantized geometry (KHR_mesh_quantization, SPEC
/// §5.4), as Bolt Brawl's glb_quantize.py did: positions as normalized i16 in
/// the mesh's bounding cube (the dequantizing offset and scale go on the nodes
/// that draw it, or into its skin's inverse bind matrices), normals and
/// tangents as i8, UVs as u16 when they lie in 0–1, vertex colors as u8, skin
/// weights as u16 and joints as u8, rotation keys as i16. About half the bytes
/// of float geometry with no visible change (i16 over a 5 m model is a 0.15 mm
/// step); a bundle's deflate barely shrinks floats, so this is where its room
/// comes from. A mesh with morph targets keeps float positions.
pub fn write_quantized(sc: &Scene, generator: &str) -> Vec<u8> {
    write_with(sc, generator, true)
}

fn write_with(sc: &Scene, generator: &str, quantize: bool) -> Vec<u8> {
    let mut b = Bin { data: Vec::new(), views: Vec::new(), accessors: Vec::new() };
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut quantized = false;
    let (plan, skin_of) = if quantize { quant_plan(sc) } else { (vec![None; sc.meshes.len()], vec![None; sc.meshes.len()]) };
    let mut meshes = Vec::new();
    for (mi, m) in sc.meshes.iter().enumerate() {
        let mut prims = Vec::new();
        for p in &m.prims {
            let mut at = serde_json::Map::new();
            match plan[mi] {
                Some((c, s)) => {
                    let q: Vec<f32> = p.pos.iter().flat_map(|v| [(v[0] - c[0]) / s, (v[1] - c[1]) / s, (v[2] - c[2]) / s]).collect();
                    at.insert("POSITION".into(), json!(b.normalized(&q, 3, "VEC3", 5122, 8, true, Some(34962))));
                    quantized = true;
                }
                None => {
                    at.insert("POSITION".into(), json!(b.floats(&flat(&p.pos), 3, "VEC3", true, Some(34962))));
                }
            }
            if let Some(n) = &p.nrm {
                if quantize {
                    let d: Vec<f32> = n.iter().flat_map(|v| normalize(*v)).collect();
                    at.insert("NORMAL".into(), json!(b.normalized(&d, 3, "VEC3", 5120, 4, false, Some(34962))));
                    quantized = true;
                } else {
                    at.insert("NORMAL".into(), json!(b.floats(&flat(n), 3, "VEC3", false, Some(34962))));
                }
            }
            if let Some(t) = &p.tan {
                if quantize {
                    let d: Vec<f32> = t.iter().flat_map(|v| {
                        let n = normalize([v[0], v[1], v[2]]);
                        [n[0], n[1], n[2], if v[3] < 0.0 { -1.0 } else { 1.0 }]
                    }).collect();
                    at.insert("TANGENT".into(), json!(b.normalized(&d, 4, "VEC4", 5120, 4, false, Some(34962))));
                    quantized = true;
                } else {
                    at.insert("TANGENT".into(), json!(b.floats(&flat(t), 4, "VEC4", false, Some(34962))));
                }
            }
            if let Some(t) = &p.uv {
                if quantize && t.iter().flatten().all(|x| (0.0..=1.0).contains(x)) {
                    at.insert("TEXCOORD_0".into(), json!(b.normalized(&flat(t), 2, "VEC2", 5123, 4, false, Some(34962))));
                    quantized = true;
                } else {
                    at.insert("TEXCOORD_0".into(), json!(b.floats(&flat(t), 2, "VEC2", false, Some(34962))));
                }
            }
            if let Some(c) = &p.color {
                if quantize {
                    let d: Vec<f32> = flat(c).iter().map(|x| x.clamp(0.0, 1.0)).collect();
                    at.insert("COLOR_0".into(), json!(b.normalized(&d, 4, "VEC4", 5121, 4, false, Some(34962))));
                } else {
                    at.insert("COLOR_0".into(), json!(b.floats(&flat(c), 4, "VEC4", false, Some(34962))));
                }
            }
            if let (Some(j), Some(w)) = (&p.joints, &p.weights) {
                at.insert("JOINTS_0".into(), json!(b.joints(j, quantize)));
                if quantize {
                    // u16 weights that still sum to exactly 1: the largest takes the rounding.
                    let d: Vec<f32> = w
                        .iter()
                        .flat_map(|v| {
                            let sum: f32 = v.iter().map(|x| x.max(0.0)).sum::<f32>().max(1e-9);
                            let mut q = v.map(|x| (x.max(0.0) / sum * 65535.0).round());
                            let big = (0..4).max_by(|&a, &b| q[a].total_cmp(&q[b])).unwrap();
                            q[big] += 65535.0 - q.iter().sum::<f32>();
                            q.map(|x| x / 65535.0)
                        })
                        .collect();
                    at.insert("WEIGHTS_0".into(), json!(b.normalized(&d, 4, "VEC4", 5123, 8, false, Some(34962))));
                } else {
                    at.insert("WEIGHTS_0".into(), json!(b.floats(&flat(w), 4, "VEC4", false, Some(34962))));
                }
            }
            let mut prim = json!({ "attributes": at, "indices": b.indices(&p.indices, p.pos.len()), "mode": 4 });
            if let Some(mat) = p.material {
                prim["material"] = json!(mat);
            }
            if !p.targets.is_empty() {
                let ts: Vec<Value> = p
                    .targets
                    .iter()
                    .map(|t| {
                        let mut o = json!({ "POSITION": b.floats(&flat(&t.pos), 3, "VEC3", true, None) });
                        if let Some(n) = &t.nrm {
                            o["NORMAL"] = json!(b.floats(&flat(n), 3, "VEC3", false, None));
                        }
                        o
                    })
                    .collect();
                prim["targets"] = json!(ts);
            }
            prims.push(prim);
        }
        let mut mj = json!({ "primitives": prims });
        if !m.name.is_empty() {
            mj["name"] = json!(m.name);
        }
        if !m.weights.is_empty() {
            mj["weights"] = json!(m.weights);
        }
        meshes.push(mj);
    }
    let images: Vec<Value> = sc
        .images
        .iter()
        .map(|im| {
            let v = b.view(&im.bytes, None);
            let mut o = json!({ "bufferView": v, "mimeType": im.mime });
            if !im.name.is_empty() {
                o["name"] = json!(im.name);
            }
            o
        })
        .collect();
    let mut samplers: Vec<Value> = Vec::new();
    let textures: Vec<Value> = sc
        .textures
        .iter()
        .map(|t| {
            let mut o = json!({ "source": t.image });
            if let Some(s) = &t.sampler {
                let k = samplers.iter().position(|x| x == s).unwrap_or_else(|| {
                    samplers.push(s.clone());
                    samplers.len() - 1
                });
                o["sampler"] = json!(k);
            }
            o
        })
        .collect();
    let tref = |t: &TexRef, used: &mut BTreeSet<&str>| {
        let mut o = json!({ "index": t.texture });
        if let Some(x) = &t.transform {
            o["extensions"] = json!({ "KHR_texture_transform": x });
            used.insert("KHR_texture_transform");
        }
        o
    };
    let materials: Vec<Value> = sc
        .materials
        .iter()
        .map(|m| {
            let mut pbr = json!({ "baseColorFactor": m.base, "metallicFactor": m.metallic, "roughnessFactor": m.roughness });
            if let Some(t) = &m.base_tex {
                pbr["baseColorTexture"] = tref(t, &mut used);
            }
            if let Some(t) = &m.mr_tex {
                pbr["metallicRoughnessTexture"] = tref(t, &mut used);
            }
            let mut o = json!({ "pbrMetallicRoughness": pbr });
            if !m.name.is_empty() {
                o["name"] = json!(m.name);
            }
            if let Some(t) = &m.normal_tex {
                o["normalTexture"] = tref(t, &mut used);
                if m.normal_scale != 1.0 {
                    o["normalTexture"]["scale"] = json!(m.normal_scale);
                }
            }
            if m.emissive != [0.0; 3] {
                o["emissiveFactor"] = json!(m.emissive);
            }
            if let Some(t) = &m.emissive_tex {
                o["emissiveTexture"] = tref(t, &mut used);
            }
            let mut ext = serde_json::Map::new();
            if let Some(s) = m.emissive_strength.filter(|&s| s != 1.0) {
                ext.insert("KHR_materials_emissive_strength".into(), json!({ "emissiveStrength": s }));
                used.insert("KHR_materials_emissive_strength");
            }
            if m.unlit {
                ext.insert("KHR_materials_unlit".into(), json!({}));
                used.insert("KHR_materials_unlit");
            }
            if !ext.is_empty() {
                o["extensions"] = Value::Object(ext);
            }
            if m.alpha_mode != "OPAQUE" {
                o["alphaMode"] = json!(m.alpha_mode);
                if m.alpha_mode == "MASK" {
                    o["alphaCutoff"] = json!(m.alpha_cutoff);
                }
            }
            if m.double_sided {
                o["doubleSided"] = json!(true);
            }
            o
        })
        .collect();
    let nodes: Vec<Value> = sc
        .nodes
        .iter()
        .map(|n| {
            let mut o = json!({});
            if !n.name.is_empty() {
                o["name"] = json!(n.name);
            }
            // A quantized mesh drawn unskinned: its node dequantizes (offset, then scale).
            let (mut t, mut s) = (n.t, n.s);
            if let Some((c, k)) = n.mesh.and_then(|m| plan[m]).filter(|_| n.skin.is_none()) {
                let off = rotate(n.r, [s[0] * c[0], s[1] * c[1], s[2] * c[2]]);
                t = [t[0] + off[0], t[1] + off[1], t[2] + off[2]];
                s = [s[0] * k, s[1] * k, s[2] * k];
            }
            if t != [0.0; 3] {
                o["translation"] = json!(t);
            }
            if n.r != [0.0, 0.0, 0.0, 1.0] {
                o["rotation"] = json!(n.r);
            }
            if s != [1.0; 3] {
                o["scale"] = json!(s);
            }
            if !n.children.is_empty() {
                o["children"] = json!(n.children);
            }
            if let Some(m) = n.mesh {
                o["mesh"] = json!(m);
            }
            if let Some(s) = n.skin {
                o["skin"] = json!(s);
            }
            o
        })
        .collect();
    let skins: Vec<Value> = sc
        .skins
        .iter()
        .enumerate()
        .map(|(si, s)| {
            // A quantized skinned mesh dequantizes in its skin's inverse bind matrices.
            let deq = (0..sc.meshes.len()).find(|&m| skin_of[m] == Some(si)).and_then(|m| plan[m]);
            let ibm: Vec<f32> = s.ibm[..s.joints.len()]
                .iter()
                .flat_map(|m| match deq {
                    Some((c, k)) => mul(m, &trs(c, [0.0, 0.0, 0.0, 1.0], [k; 3])),
                    None => *m,
                })
                .collect();
            let ibm = b.floats(&ibm, 16, "MAT4", false, None);
            let mut o = json!({ "joints": s.joints, "inverseBindMatrices": ibm });
            if !s.name.is_empty() {
                o["name"] = json!(s.name);
            }
            if let Some(k) = s.skeleton {
                o["skeleton"] = json!(k);
            }
            o
        })
        .collect();
    let anims: Vec<Value> = sc
        .anims
        .iter()
        .map(|a| {
            let mut samplers = Vec::new();
            let mut channels = Vec::new();
            for c in &a.channels {
                let input = b.floats(&c.input, 1, "SCALAR", true, None);
                let (n, ty) = match c.path {
                    Path::T | Path::S => (3, "VEC3"),
                    Path::R => (4, "VEC4"),
                    Path::W => (1, "SCALAR"),
                };
                // Rotation keys as normalized i16 (core glTF); cubic tangents can pass 1, so stay float.
                let output = if quantize && c.path == Path::R && c.interp != "CUBICSPLINE" && c.output.iter().all(|x| x.abs() <= 1.0) {
                    quantized = true;
                    b.normalized(&c.output, n, ty, 5122, 8, false, None)
                } else {
                    b.floats(&c.output, n, ty, false, None)
                };
                samplers.push(json!({ "input": input, "output": output, "interpolation": c.interp }));
                let path = match c.path {
                    Path::T => "translation",
                    Path::R => "rotation",
                    Path::S => "scale",
                    Path::W => "weights",
                };
                channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": c.node, "path": path } }));
            }
            json!({ "name": a.name, "samplers": samplers, "channels": channels })
        })
        .collect();
    while b.data.len() % 4 != 0 {
        b.data.push(0);
    }
    let mut j = json!({
        "asset": { "version": "2.0", "generator": generator },
        "scene": 0,
        "scenes": [{ "nodes": sc.roots }],
        "nodes": nodes,
        "buffers": [{ "byteLength": b.data.len() }],
        "bufferViews": b.views,
        "accessors": b.accessors,
    });
    for (k, v) in [("meshes", meshes), ("materials", materials), ("textures", textures), ("images", images), ("samplers", samplers), ("skins", skins), ("animations", anims)] {
        if !v.is_empty() {
            j[k] = Value::Array(v);
        }
    }
    if quantized {
        used.insert("KHR_mesh_quantization");
        j["extensionsRequired"] = json!(["KHR_mesh_quantization"]);
    }
    if !used.is_empty() {
        j["extensionsUsed"] = json!(used.into_iter().collect::<Vec<_>>());
    }
    let mut text = serde_json::to_vec(&j).unwrap();
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + b.data.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&text);
    out.extend_from_slice(&(b.data.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(&b.data);
    out
}

// --- math (column-major, as glTF) ---------------------------------------------------

pub fn identity() -> Mat4 {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

pub fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Mat4 {
    let [x, y, z, w] = q;
    let (xx, yy, zz, xy, xz, yz, wx, wy, wz) = (x * x, y * y, z * z, x * y, x * z, y * z, w * x, w * y, w * z);
    [
        (1.0 - 2.0 * (yy + zz)) * s[0],
        2.0 * (xy + wz) * s[0],
        2.0 * (xz - wy) * s[0],
        0.0,
        2.0 * (xy - wz) * s[1],
        (1.0 - 2.0 * (xx + zz)) * s[1],
        2.0 * (yz + wx) * s[1],
        0.0,
        2.0 * (xz + wy) * s[2],
        2.0 * (yz - wx) * s[2],
        (1.0 - 2.0 * (xx + yy)) * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

pub fn point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

pub fn dir(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [m[0] * p[0] + m[4] * p[1] + m[8] * p[2], m[1] * p[0] + m[5] * p[1] + m[9] * p[2], m[2] * p[0] + m[6] * p[1] + m[10] * p[2]]
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-12 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 1.0, 0.0] }
}

pub fn quat_y(angle: f32) -> [f32; 4] {
    let (s, c) = (angle / 2.0).sin_cos();
    [0.0, s, 0.0, c]
}

pub fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Translation, rotation and scale from an affine matrix (shear is lost).
pub fn decompose(m: &Mat4) -> ([f32; 3], [f32; 4], [f32; 3]) {
    let t = [m[12], m[13], m[14]];
    let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let det = c0[0] * (c1[1] * c2[2] - c1[2] * c2[1]) - c1[0] * (c0[1] * c2[2] - c0[2] * c2[1]) + c2[0] * (c0[1] * c1[2] - c0[2] * c1[1]);
    let mut s = [len(c0), len(c1), len(c2)];
    if det < 0.0 {
        s[0] = -s[0];
    }
    let r = |v: [f32; 3], k: f32| if k.abs() > 1e-12 { [v[0] / k, v[1] / k, v[2] / k] } else { v };
    let (a, b, c) = (r(c0, s[0]), r(c1, s[1]), r(c2, s[2]));
    // Rotation matrix (columns a, b, c) to a quaternion.
    let (m00, m11, m22) = (a[0], b[1], c[2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let k = (tr + 1.0).sqrt() * 2.0;
        [(b[2] - c[1]) / k, (c[0] - a[2]) / k, (a[1] - b[0]) / k, 0.25 * k]
    } else if m00 > m11 && m00 > m22 {
        let k = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * k, (b[0] + a[1]) / k, (c[0] + a[2]) / k, (b[2] - c[1]) / k]
    } else if m11 > m22 {
        let k = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(b[0] + a[1]) / k, 0.25 * k, (c[1] + b[2]) / k, (c[0] - a[2]) / k]
    } else {
        let k = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(c[0] + a[2]) / k, (c[1] + b[2]) / k, 0.25 * k, (a[1] - b[0]) / k]
    };
    let ql = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-12);
    (t, [q[0] / ql, q[1] / ql, q[2] / ql, q[3] / ql], s)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A unit cube (24 vertices, 12 triangles) with one material.
    pub fn cube_scene() -> Scene {
        let mut p = Prim::default();
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        let (mut pos, mut nrm, mut uv) = (Vec::new(), Vec::new(), Vec::new());
        for (n, a, bb) in faces {
            let base = pos.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                pos.push([0.5 * (n[0] + su * a[0] + sv * bb[0]), 0.5 * (n[1] + su * a[1] + sv * bb[1]), 0.5 * (n[2] + su * a[2] + sv * bb[2])]);
                nrm.push(n);
                uv.push([(su + 1.0) / 2.0, (sv + 1.0) / 2.0]);
            }
            p.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        p.pos = pos;
        p.nrm = Some(nrm);
        p.uv = Some(uv);
        p.material = Some(0);
        Scene {
            nodes: vec![Node { name: "cube".into(), mesh: Some(0), ..Node::default() }],
            roots: vec![0],
            meshes: vec![Mesh { name: "cube".into(), prims: vec![p], weights: vec![] }],
            materials: vec![Material { base: [0.8, 0.3, 0.2, 1.0], metallic: 0.0, roughness: 0.6, ..Material::default() }],
            ..Scene::default()
        }
    }

    #[test]
    fn round_trips() {
        let mut sc = cube_scene();
        sc.nodes[0].t = [1.0, 2.0, 3.0];
        sc.nodes[0].r = quat_y(0.5);
        sc.anims.push(Anim {
            name: "Spin".into(),
            channels: vec![Channel { node: 0, path: Path::R, interp: "LINEAR".into(), input: vec![0.0, 1.0], output: [quat_y(0.0), quat_y(3.0)].concat() }],
        });
        let bytes = write(&sc, "test");
        mb_format::bundle::check_glb(&bytes).unwrap();
        let back = read(&bytes).unwrap();
        assert_eq!(back.triangles(), 12);
        assert_eq!(back.vertices(), 24);
        assert_eq!(back.nodes[0].t, [1.0, 2.0, 3.0]);
        assert_eq!(back.anims[0].name, "Spin");
        assert!((back.anims[0].duration() - 1.0).abs() < 1e-6);
        assert_eq!(back.materials[0].base, [0.8, 0.3, 0.2, 1.0]);
        assert_eq!(back.meshes[0].prims[0].indices, sc.meshes[0].prims[0].indices);
        // Same scene, same bytes.
        assert_eq!(write(&back, "test"), bytes);
    }

    /// Quantized geometry reads back where the float geometry was, unskinned and skinned.
    #[test]
    fn quantized_geometry_round_trips_in_about_half_the_bytes() {
        use super::super::shape;
        let mut sc = cube_scene();
        // Off the origin, scaled and turned, with a child, as a provider's nodes are.
        for p in &mut sc.meshes[0].prims[0].pos {
            *p = [p[0] * 3.0 + 10.0, p[1] * 0.5 - 2.0, p[2] + 4.0];
        }
        sc.nodes[0].t = [1.0, 2.0, 3.0];
        sc.nodes[0].r = quat_y(0.7);
        sc.nodes[0].s = [2.0, 1.0, 0.5];
        sc.nodes.push(Node { name: "child".into(), t: [0.0, 1.0, 0.0], ..Node::default() });
        sc.nodes[0].children = vec![1];
        let world = |s: &Scene| {
            let p = shape::posed(s, &shape::pose(s, None));
            p.into_iter().flat_map(|p| p.pos).collect::<Vec<_>>()
        };
        let floats = write(&sc, "t");
        let q = write_quantized(&sc, "t");
        mb_format::bundle::check_glb(&q).unwrap();
        let back = read(&q).unwrap();
        let (a, b2) = (world(&sc), world(&back));
        for (x, y) in a.iter().zip(&b2) {
            for k in 0..3 {
                assert!((x[k] - y[k]).abs() < 2e-3, "{x:?} vs {y:?}");
            }
        }
        // The child keeps its place: only the mesh node's own transform changed.
        assert_eq!(back.nodes[1].t, [0.0, 1.0, 0.0]);
        let n = back.meshes[0].prims[0].nrm.as_ref().unwrap();
        assert!(n.iter().zip(sc.meshes[0].prims[0].nrm.as_ref().unwrap()).all(|(a, b)| (0..3).all(|k| (a[k] - b[k]).abs() < 0.01)));
        assert_eq!(back.meshes[0].prims[0].uv, sc.meshes[0].prims[0].uv, "0, ½ and 1 are exact in u16");
        let (j, _) = split_glb(&q).unwrap();
        assert_eq!(j["extensionsRequired"], json!(["KHR_mesh_quantization"]));
        let geometry = |b: &[u8]| b.len();
        assert!(geometry(&q) < geometry(&floats), "{} vs {}", q.len(), floats.len());

        // Skinned: the dequantization goes into the inverse bind matrices.
        let mut sk = cube_scene();
        let p = &mut sk.meshes[0].prims[0];
        for v in &mut p.pos {
            *v = [v[0] + 5.0, v[1] * 4.0, v[2]];
        }
        p.joints = Some(vec![[0, 1, 0, 0]; p.pos.len()]);
        p.weights = Some(p.pos.iter().map(|v| [0.5 + v[1] / 8.0, 0.5 - v[1] / 8.0, 0.0, 0.0]).collect());
        sk.nodes.push(Node { name: "a".into(), t: [0.0, 1.0, 0.0], ..Node::default() });
        sk.nodes.push(Node { name: "b".into(), r: quat_y(0.4), ..Node::default() });
        sk.nodes[0].skin = Some(0);
        sk.roots = vec![0, 1, 2];
        sk.skins.push(Skin { name: "s".into(), joints: vec![1, 2], ibm: vec![trs([0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]), identity()], skeleton: None });
        let back = read(&write_quantized(&sk, "t")).unwrap();
        assert_eq!(back.nodes[0].s, [1.0; 3], "a skinned mesh's node is left alone");
        for (x, y) in world(&sk).iter().zip(&world(&back)) {
            for k in 0..3 {
                assert!((x[k] - y[k]).abs() < 2e-3, "{x:?} vs {y:?}");
            }
        }
        let w = back.meshes[0].prims[0].weights.as_ref().unwrap();
        assert!(w.iter().all(|w| (w.iter().sum::<f32>() - 1.0).abs() < 1e-4));
    }

    #[test]
    fn refuses_what_it_cant_decode() {
        let glb = |j: &str| {
            let mut t = j.as_bytes().to_vec();
            while t.len() % 4 != 0 {
                t.push(b' ');
            }
            let mut out = b"glTF".to_vec();
            out.extend(2u32.to_le_bytes());
            out.extend(((20 + t.len()) as u32).to_le_bytes());
            out.extend((t.len() as u32).to_le_bytes());
            out.extend(0x4E4F_534Au32.to_le_bytes());
            out.extend(t);
            out
        };
        let e = read(&glb(r#"{"asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]}"#)).unwrap_err();
        assert!(e.contains("Draco"), "{e}");
        let ok = read(&glb(r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_materials_specular"],"nodes":[{"name":"a","camera":0}],"scenes":[{"nodes":[0]}]}"#)).unwrap();
        assert!(ok.dropped.contains("KHR_materials_specular") && ok.dropped.contains("cameras"), "{:?}", ok.dropped);
        assert!(read(b"nope").is_err());
    }

    #[test]
    fn matrices_decompose() {
        let m = trs([1.0, -2.0, 0.5], quat_mul(quat_y(0.7), [0.2f32.sin(), 0.0, 0.0, 0.2f32.cos()]), [2.0, 2.0, 0.5]);
        let (t, r, s) = decompose(&m);
        let back = trs(t, r, s);
        for k in 0..16 {
            assert!((back[k] - m[k]).abs() < 1e-5, "{k}: {} vs {}", back[k], m[k]);
        }
    }
}
