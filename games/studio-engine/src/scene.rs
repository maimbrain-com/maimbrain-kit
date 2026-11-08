//! 3D games (`kind = "3d"`, the engine's `3d`
//! build): what a Lua game asks mb3d for, kept in a registry the VM owns.
//! docs/STUDIO-3D-API.md is the API; docs/spike/studio-lua-3d.md "Bindings"
//! has the rules behind it:
//!
//! - Lua sees small integer ids (1, 2, 3, … per kind), never host handles,
//!   so they're the same on every run and natively (where every host call
//!   returns handle 1).
//! - Calls take and return plain numbers (`place(id, x, y, z, yaw, s)`,
//!   `world(id)` → x, y, z): a userdata field would cost ~517 ns.
//! - Settings shaped like structs (`sky`, `fog`, `material`, `text`, …) are
//!   Lua tables, read once when set.
//! - What follows something follows it here: each spawned model's
//!   `Animator` is stepped by the engine after Lua's frame (`animate`).
//!
//! The prelude (`prelude.lua`) wraps these in `node:place(…)`-style methods.
//! Dropping the scene (with its VM) frees everything it made, so each
//! round's VM builds the scene afresh, as the Studio engine's rounds start
//! from nothing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use maimbrain::audio::{Bus, Sound, Voice};
use maimbrain::gfx2d::Font;
use maimbrain::gfx3d::{
    self, Animator, Camera, Clip, DepthOfField, Emitter, EmitterDesc, Film, Fog, FogMode, FxBlend, Grade, Halos, Instance, LightShafts, Material, MaterialDesc, MaterialStyle, Mesh, MeshData, Model, Node, Play,
    Post, Quat, Sky, Sun, TextDesc, Transform, Vec3, look_at, srgb, srgba, vec3,
};
use maimbrain::sensors::{self, Haptic};
use maimbrain::sys::{self, Asset};
use omnilua::{FromLuaMulti, IntoLuaMulti, Lua, Table, Value, Variadic};

use crate::vm::{rt_err, to_s};

/// The project's models by name (`fox` for `models/fox.glb`), loaded before any VM.
pub type Assets = Rc<BTreeMap<String, Asset>>;

/// Sound that outlives a round: the project's sounds and music (by name,
/// `jump` for `sounds/jump.ogg` or its `.sfx.toml`) and the music playing, so a new round's `music("theme")`
/// carries on rather than restarting it. Output only, like all audio: it
/// never affects the game, so replays don't see it.
#[derive(Default)]
pub struct Audio {
    pub sounds: BTreeMap<String, Sound>,
    music: Option<(String, Voice)>,
}

impl Audio {
    fn sound(&self, name: &str) -> omnilua::Result<Sound> {
        let key = name.trim_end_matches(".ogg");
        self.sounds.get(key).copied().ok_or_else(|| rt_err(format!("no sound \"{key}\": add sounds/{key}.sfx.toml or sounds/{key}.ogg (music/{key}.ogg for music)")))
    }
}

struct MeshSlot {
    data: MeshData,
    uploaded: Option<Mesh>,
}

struct NodeSlot {
    node: Node,
    t: Transform,
    /// Made by this scene (destroyed with it), or found inside a model.
    owned: bool,
    anim: Option<Box<Anim>>,
    /// The model it was spawned from, when it's a model's root.
    model: Option<Model>,
}

struct Anim {
    animator: Animator,
    clips: Vec<(String, Clip)>,
}

/// 2D drawn over the 3D in `render`, in logical units (360×640).
#[derive(Clone, Debug, PartialEq)]
pub enum Hud {
    Text { s: String, x: f32, y: f32, size: f32, color: u32, align: i8, shadow: bool },
    Rect { x: f32, y: f32, w: f32, h: f32, color: u32 },
}

pub struct Scene {
    assets: Assets,
    audio: Rc<RefCell<Audio>>,
    /// Looping voices this round started (`loop(...)`), stopped with it.
    voices: Vec<Voice>,
    meshes: Vec<MeshSlot>,
    materials: Vec<(Material, bool)>,
    default_material: Option<Material>,
    nodes: Vec<NodeSlot>,
    models: Vec<Model>,
    emitters: Vec<Emitter>,
    pub hud: Vec<Hud>,
    pub camera: Option<Camera>,
}


impl Scene {
    pub fn new(assets: Assets, audio: Rc<RefCell<Audio>>) -> Scene {
        Scene { assets, audio, voices: Vec::new(), meshes: Vec::new(), materials: Vec::new(), default_material: None, nodes: Vec::new(), models: Vec::new(), emitters: Vec::new(), hud: Vec::new(), camera: None }
    }

    pub fn begin_frame(&mut self) {
        self.hud.clear();
    }

    /// Steps and applies every spawned model's animation (after Lua's frame).
    pub fn animate(&mut self, dt: f32) {
        for n in &mut self.nodes {
            if let Some(a) = &mut n.anim {
                a.animator.update(dt);
                a.animator.apply(n.node);
            }
        }
    }

    /// What's in the scene, for the tests and the spike's log.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.nodes.len(), self.meshes.len(), self.materials.len())
    }

    fn node(&self, id: i64) -> omnilua::Result<&NodeSlot> {
        self.nodes.get((id - 1) as usize).ok_or_else(|| rt_err(format!("no node {id}")))
    }

    fn node_mut(&mut self, id: i64) -> omnilua::Result<&mut NodeSlot> {
        self.nodes.get_mut((id - 1) as usize).ok_or_else(|| rt_err(format!("no node {id}")))
    }

    fn mesh(&mut self, id: i64) -> omnilua::Result<&mut MeshSlot> {
        self.meshes.get_mut((id - 1) as usize).ok_or_else(|| rt_err(format!("no mesh {id}")))
    }

    fn material(&self, id: i64) -> omnilua::Result<Material> {
        self.materials.get((id - 1) as usize).map(|m| m.0).ok_or_else(|| rt_err(format!("no material {id}")))
    }

    fn add_node(&mut self, node: Node, owned: bool, model: Option<Model>) -> i64 {
        self.nodes.push(NodeSlot { node, t: Transform::at(Vec3::ZERO), owned, anim: None, model });
        self.nodes.len() as i64
    }

    fn add_mesh(&mut self, data: MeshData) -> i64 {
        self.meshes.push(MeshSlot { data, uploaded: None });
        self.meshes.len() as i64
    }

    fn uploaded(&mut self, id: i64) -> omnilua::Result<Mesh> {
        let m = self.mesh(id)?;
        if m.uploaded.is_none() {
            m.uploaded = Some(m.data.upload().ok_or_else(|| rt_err("the mesh didn't upload (too many meshes or vertices?)"))?);
        }
        Ok(m.uploaded.unwrap())
    }

    /// The material a node gets when none is given: one a scene, made the
    /// first time it's needed and freed with the scene like any other.
    fn default_material(&mut self) -> omnilua::Result<Material> {
        if let Some(m) = self.default_material {
            return Ok(m);
        }
        let m = Material::new(&MaterialDesc::default()).ok_or_else(|| rt_err("too many materials"))?;
        self.materials.push((m, true));
        self.default_material = Some(m);
        Ok(m)
    }

    fn set_t(&mut self, id: i64, f: impl FnOnce(&mut Transform)) -> omnilua::Result<()> {
        let n = self.node_mut(id)?;
        f(&mut n.t);
        n.node.set_transform(&n.t);
        Ok(())
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        for v in &self.voices {
            v.stop();
        }
        // Newest first: a model's root before the nodes parented under it
        // that came earlier (destroying a node takes its children with it).
        for n in self.nodes.iter().rev().filter(|n| n.owned) {
            n.node.destroy();
        }
        for m in self.meshes.iter().filter_map(|m| m.uploaded) {
            m.free();
        }
        for (m, owned) in &self.materials {
            if *owned {
                m.free();
            }
        }
        for e in &self.emitters {
            e.free();
        }
        for m in &self.models {
            m.free();
        }
    }
}

// --- reading Lua values -----------------------------------------------------

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(f) if f.is_finite() => Some(*f),
        _ => None,
    }
}

fn field(t: &Table, k: &str) -> Value {
    t.raw_get::<_, Value>(k).unwrap_or(Value::Nil)
}

fn f(t: &Table, k: &str, d: f32) -> f32 {
    num(&field(t, k)).map_or(d, |v| v as f32)
}

fn flag(t: &Table, k: &str) -> bool {
    matches!(field(t, k), Value::Boolean(true))
}

/// Up to `n` numbers from an array table.
fn nums<const N: usize>(v: &Value, d: [f32; N]) -> [f32; N] {
    let mut out = d;
    if let Value::Table(t) = v {
        for (i, o) in out.iter_mut().enumerate() {
            if let Some(x) = t.raw_get::<_, Value>(i as i64 + 1).ok().as_ref().and_then(num) {
                *o = x as f32;
            }
        }
    }
    out
}

/// A colour: `0xRRGGBB` (sRGB) or `{ r, g, b }` (linear, may exceed 1).
fn rgb(v: &Value, d: [f32; 3]) -> [f32; 3] {
    match v {
        Value::Integer(i) => srgb(*i as u32),
        Value::Table(_) => nums(v, d),
        _ => d,
    }
}

/// A colour with alpha: `0xRRGGBBAA` (sRGB) or `{ r, g, b, a }` (linear).
fn rgba(v: &Value, d: [f32; 4]) -> [f32; 4] {
    match v {
        Value::Integer(i) => srgba(*i as u32),
        Value::Table(_) => nums(v, d),
        _ => d,
    }
}

fn v3(v: &Value, d: Vec3) -> Vec3 {
    let [x, y, z] = nums(v, [d.x, d.y, d.z]);
    vec3(x, y, z)
}

/// A flat array of numbers.
fn flat(t: &Table) -> omnilua::Result<Vec<f32>> {
    let n = t.len()? as i64;
    (1..=n).map(|i| t.raw_get::<_, f64>(i).map(|v| v as f32)).collect()
}

fn rot(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll)
}

fn style_of(t: &Table) -> MaterialStyle {
    MaterialStyle {
        outline_color: rgb(&field(t, "ink"), [0.0; 3]),
        outline_width: f(t, "outline", 0.0),
        toon_bands: f(t, "toon", 0.0) as u32,
        rim: f(t, "rim", 0.0),
        flags: if flag(t, "nofog") { MaterialStyle::NO_FOG } else { 0 },
        ..Default::default()
    }
}

fn text_of(t: Option<&Table>) -> TextDesc {
    let Some(t) = t else { return TextDesc::new(Font::SansBold, 0.3) };
    let font = match field(t, "font") {
        Value::String(s) if s.to_str().is_ok_and(|s| s == "pixel") => Font::Pixel,
        _ => Font::SansBold,
    };
    let mut d = TextDesc::new(font, f(t, "size", 0.3)).with_color(rgba(&field(t, "color"), [1.0; 4]));
    if !matches!(field(t, "outline"), Value::Nil) {
        d = d.with_outline(rgba(&field(t, "outline"), [0.0, 0.0, 0.0, 1.0]), f(t, "width", 0.06));
    }
    for (k, bit) in [("billboard", TextDesc::BILLBOARD), ("screen", TextDesc::SCREEN), ("ontop", TextDesc::ON_TOP), ("nofog", TextDesc::NO_FOG)] {
        if flag(t, k) {
            d.flags |= bit;
        }
    }
    d
}


/// Arguments past three (omnilua converts tuples of up to three).
struct Args(Vec<Value>);

impl Args {
    fn get(&self, i: usize) -> &Value {
        self.0.get(i).unwrap_or(&Value::Nil)
    }
    fn num(&self, i: usize) -> omnilua::Result<f64> {
        num(self.get(i)).ok_or_else(|| rt_err(format!("argument {} should be a number", i + 1)))
    }
    fn opt(&self, i: usize) -> Option<f64> {
        num(self.get(i))
    }
    fn int(&self, i: usize) -> omnilua::Result<i64> {
        Ok(self.num(i)? as i64)
    }
    fn flag(&self, i: usize) -> Option<bool> {
        match self.get(i) {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

fn args(v: Variadic<Value>) -> Args {
    Args(v.into_vec())
}

// --- the bindings -----------------------------------------------------------

fn reg<A, R>(lua: &Lua, api: &Table, name: &str, s: &Rc<RefCell<Scene>>, func: impl Fn(&Lua, &mut Scene, A) -> omnilua::Result<R> + 'static) -> omnilua::Result<()>
where
    A: FromLuaMulti + 'static,
    R: IntoLuaMulti + 'static,
{
    let s = s.clone();
    api.set(name, lua.create_function(move |lua, a: A| func(lua, &mut s.borrow_mut(), a))?)
}

/// The table the prelude builds the API from.
pub fn install(lua: &Lua, scene: &Rc<RefCell<Scene>>) -> omnilua::Result<Table> {
    let api = lua.create_table()?;
    let s = scene;

    // Meshes: built here, uploaded the first time a node draws one.
    let shape = |lua: &Lua, api: &Table, name: &str, make: fn(&[f32]) -> MeshData| {
        reg(lua, api, name, s, move |_, sc, v: Variadic<f64>| {
            let a: Vec<f32> = v.into_iter().map(|x| x as f32).collect();
            Ok(sc.add_mesh(make(&a)))
        })
    };
    shape(lua, &api, "box", |a| MeshData::cuboid(vec3(a.first().copied().unwrap_or(1.0), a.get(1).copied().unwrap_or(1.0), a.get(2).copied().unwrap_or(1.0))))?;
    shape(lua, &api, "sphere", |a| MeshData::uv_sphere(a.first().copied().unwrap_or(0.5), a.get(1).copied().unwrap_or(16.0) as u32, a.get(2).copied().unwrap_or(10.0) as u32))?;
    shape(lua, &api, "ico", |a| MeshData::icosphere(a.first().copied().unwrap_or(0.5), a.get(1).copied().unwrap_or(1.0) as u32))?;
    shape(lua, &api, "cylinder", |a| MeshData::cylinder(a.first().copied().unwrap_or(0.5), a.get(1).copied().unwrap_or(1.0), a.get(2).copied().unwrap_or(16.0) as u32))?;
    shape(lua, &api, "cone", |a| MeshData::cone(a.first().copied().unwrap_or(0.5), a.get(1).copied().unwrap_or(1.0), a.get(2).copied().unwrap_or(16.0) as u32))?;
    shape(lua, &api, "plane", |a| MeshData::plane(a.first().copied().unwrap_or(1.0), a.get(1).copied().unwrap_or(1.0)))?;
    shape(lua, &api, "torus", |a| {
        MeshData::torus(a.first().copied().unwrap_or(0.5), a.get(1).copied().unwrap_or(0.1), a.get(2).copied().unwrap_or(16.0) as u32, a.get(3).copied().unwrap_or(8.0) as u32)
    })?;
    shape(lua, &api, "empty", |_| MeshData::new())?;
    reg(lua, &api, "lathe", s, |_, sc, (profile, segments): (Table, Option<f64>)| {
        let p = flat(&profile)?;
        let pts: Vec<[f32; 2]> = p.as_chunks::<2>().0.to_vec();
        Ok(sc.add_mesh(MeshData::lathe(&pts, segments.unwrap_or(16.0) as u32)))
    })?;
    reg(lua, &api, "extrude", s, |_, sc, (poly, depth): (Table, f64)| {
        let p = flat(&poly)?;
        let pts: Vec<[f32; 2]> = p.as_chunks::<2>().0.to_vec();
        Ok(sc.add_mesh(MeshData::extrude(&pts, depth as f32)))
    })?;
    // Triangles from a flat list (9 numbers each), with face normals.
    reg(lua, &api, "tris", s, |_, sc, list: Table| {
        let p = flat(&list)?;
        let mut m = MeshData::new();
        m.colors = Vec::new();
        for t in p.as_chunks::<9>().0 {
            let (a, b, c) = (vec3(t[0], t[1], t[2]), vec3(t[3], t[4], t[5]), vec3(t[6], t[7], t[8]));
            let n = (b - a).cross(c - a).normalize();
            let base = m.vertex_count() as u32;
            m.push(a, n, [0.0, 1.0]);
            m.push(b, n, [1.0, 1.0]);
            m.push(c, n, [0.5, 0.0]);
            m.tri(base, base + 1, base + 2);
        }
        Ok(sc.add_mesh(m))
    })?;
    // Edits, in place.
    reg(lua, &api, "mesh_color", s, |_, sc, (id, c): (i64, i64)| {
        let m = sc.mesh(id)?;
        m.data = std::mem::take(&mut m.data).with_vertex_colors(c as u32);
        Ok(())
    })?;
    reg(lua, &api, "mesh_split", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let (id, y, below, above) = (a.int(0)?, a.num(1)? as f32, a.int(2)?, a.int(3)?);
        let m = sc.mesh(id)?;
        m.data = std::mem::take(&mut m.data).with_vertex_colors_by(|p, _| if p.y > y { above as u32 } else { below as u32 });
        Ok(())
    })?;
    reg(lua, &api, "mesh_transform", s, |_, sc, v: Variadic<f64>| {
        let a: Vec<f32> = v.into_iter().map(|x| x as f32).collect();
        let g = |i: usize, d: f32| a.get(i).copied().unwrap_or(d);
        let id = g(0, 0.0) as i64;
        let t = Transform::at(vec3(g(1, 0.0), g(2, 0.0), g(3, 0.0))).with_rot(rot(g(4, 0.0), g(5, 0.0), g(6, 0.0))).with_scale3(vec3(g(7, 1.0), g(8, 1.0), g(9, 1.0)));
        let m = sc.mesh(id)?;
        m.data = m.data.transformed(&t.matrix());
        Ok(())
    })?;
    reg(lua, &api, "mesh_clamp_y", s, |_, sc, (id, lo): (i64, f64)| {
        for p in &mut sc.mesh(id)?.data.positions {
            p.y = p.y.max(lo as f32);
        }
        Ok(())
    })?;
    reg(lua, &api, "mesh_add", s, |_, sc, (id, other): (i64, i64)| {
        let o = sc.mesh(other)?.data.clone();
        sc.mesh(id)?.data.merge(&o);
        Ok(())
    })?;
    reg(lua, &api, "mesh_flat", s, |_, sc, id: i64| {
        let m = sc.mesh(id)?;
        m.data = m.data.flat_shaded();
        Ok(())
    })?;

    // Materials.
    reg(lua, &api, "material", s, |_, sc, t: Table| {
        let mut flags = 0;
        for (k, bit) in [("unlit", Material::UNLIT), ("double", Material::DOUBLE_SIDED), ("alpha", Material::ALPHA), ("additive", Material::ADDITIVE)] {
            if flag(&t, k) {
                flags |= bit;
            }
        }
        let d = MaterialDesc::default();
        let desc = MaterialDesc {
            base_color: rgba(&field(&t, "color"), d.base_color),
            emissive: rgb(&field(&t, "emissive"), d.emissive),
            emissive_strength: f(&t, "glow", d.emissive_strength),
            metallic: f(&t, "metal", d.metallic),
            roughness: f(&t, "rough", d.roughness),
            flags,
            ..d
        };
        let m = Material::new(&desc).ok_or_else(|| rt_err("too many materials"))?;
        if let Value::Table(st) = field(&t, "style") {
            m.set_style(&style_of(&st));
        }
        sc.materials.push((m, true));
        Ok(sc.materials.len() as i64)
    })?;
    reg(lua, &api, "material_style", s, |_, sc, (id, t): (i64, Table)| {
        sc.material(id)?.set_style(&style_of(&t));
        Ok(())
    })?;

    // Nodes.
    reg(lua, &api, "node", s, |_, sc, (mesh, mat): (Option<i64>, Option<i64>)| {
        let node = match (mesh, mat) {
            (Some(m), mat) => {
                let up = sc.uploaded(m)?;
                let mat = match mat {
                    Some(id) => sc.material(id)?,
                    None => sc.default_material()?,
                };
                Node::with_mesh(up, mat)
            }
            (None, _) => Node::new(),
        }
        .ok_or_else(|| rt_err("too many nodes (4096 at most)"))?;
        Ok(sc.add_node(node, true, None))
    })?;
    reg(lua, &api, "place", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let (id, x, y, z, yaw, size) = (a.int(0)?, a.num(1)?, a.num(2)?, a.num(3)?, a.opt(4), a.opt(5));
        sc.set_t(id, |t| {
            t.pos = vec3(x as f32, y as f32, z as f32);
            t.rot = Quat::from_rotation_y(yaw.unwrap_or(0.0) as f32);
            t.scale = Vec3::splat(size.unwrap_or(1.0) as f32);
        })
    })?;
    reg(lua, &api, "pos", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let p = vec3(a.num(1)? as f32, a.num(2)? as f32, a.num(3)? as f32);
        sc.set_t(a.int(0)?, |t| t.pos = p)
    })?;
    reg(lua, &api, "turn", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let r = rot(a.num(1)? as f32, a.opt(2).unwrap_or(0.0) as f32, a.opt(3).unwrap_or(0.0) as f32);
        sc.set_t(a.int(0)?, |t| t.rot = r)
    })?;
    reg(lua, &api, "size", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let x = a.num(1)?;
        let k = vec3(x as f32, a.opt(2).unwrap_or(x) as f32, a.opt(3).unwrap_or(x) as f32);
        sc.set_t(a.int(0)?, |t| t.scale = k)
    })?;
    reg(lua, &api, "get_pos", s, |_, sc, id: i64| {
        let p = sc.node(id)?.t.pos;
        Ok((p.x as f64, p.y as f64, p.z as f64))
    })?;
    reg(lua, &api, "parent", s, |_, sc, (id, p): (i64, Option<i64>)| {
        let parent = match p {
            Some(p) => Some(sc.node(p)?.node),
            None => None,
        };
        sc.node(id)?.node.set_parent(parent);
        Ok(())
    })?;
    reg(lua, &api, "show", s, |_, sc, (id, on): (i64, bool)| {
        sc.node(id)?.node.set_visible(on);
        Ok(())
    })?;
    reg(lua, &api, "destroy", s, |_, sc, id: i64| {
        let n = sc.node_mut(id)?;
        if n.owned {
            n.node.destroy();
            n.owned = false;
        }
        Ok(())
    })?;
    reg(lua, &api, "world", s, |_, sc, id: i64| {
        let m = sc.node(id)?.node.world().map(|m| m.0).unwrap_or([0.0; 16]);
        Ok((m[12] as f64, m[13] as f64, m[14] as f64))
    })?;
    reg(lua, &api, "fade", s, |_, sc, (id, a, b): (i64, f64, f64)| {
        sc.node(id)?.node.set_fade(a as f32, b as f32);
        Ok(())
    })?;
    reg(lua, &api, "light", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        Ok(sc.node(a.int(0)?)?.node.point_light(rgb(a.get(1), [1.0; 3]), a.num(2)? as f32, a.num(3)? as f32))
    })?;
    reg(lua, &api, "text", s, |_, sc, (id, text, t): (i64, String, Option<Table>)| {
        let n = sc.node(id)?.node;
        if text.is_empty() {
            n.clear_text();
        } else {
            n.set_text(&text_of(t.as_ref()), &text);
        }
        Ok(())
    })?;
    // Instances from a flat array: x, y, z, then (by `stride`) yaw, scale,
    // colour, pitch, roll and tall (a y-stretch on top of the scale).
    reg(lua, &api, "instances", s, |_, sc, (id, list, stride): (i64, Table, Option<i64>)| {
        let stride = stride.unwrap_or(3).clamp(3, 9) as usize;
        let n = sc.node(id)?.node;
        let len = list.len()? as usize;
        let mut out = Vec::with_capacity(len / stride);
        let mut row = [0.0f64; 9];
        let mut i = 0;
        while i + stride <= len {
            for (k, r) in row.iter_mut().enumerate().take(stride) {
                *r = list.raw_get::<_, f64>((i + k + 1) as i64)?;
            }
            let g = |k: usize, d: f64| if k < stride { row[k] } else { d } as f32;
            let s = g(4, 1.0);
            out.push(Instance {
                pos: vec3(g(0, 0.0), g(1, 0.0), g(2, 0.0)),
                rot: rot(g(3, 0.0), g(6, 0.0), g(7, 0.0)),
                scale: vec3(s, s * g(8, 1.0), s),
                // 0xRRGGBBAA, as everywhere else; the host takes it byte-swapped (Instance::with_color).
                color: if stride > 5 { (row[5] as i64 as u32).swap_bytes() } else { 0xffffffff },
            });
            i += stride;
        }
        Ok(n.set_instances(&out))
    })?;

    // Models: loaded from the bundle, spawned, animated here.
    reg(lua, &api, "model", s, |_, sc, name: String| {
        let key = name.trim_end_matches(".glb");
        let Some(&asset) = sc.assets.get(key) else { return Err(rt_err(format!("model(\"{key}\"): there's no models/{key}.glb"))) };
        let m = Model::load(asset).ok_or_else(|| rt_err(format!("model(\"{name}\") didn't load")))?;
        sc.models.push(m);
        Ok(sc.models.len() as i64)
    })?;
    reg(lua, &api, "spawn", s, |_, sc, (model, parent): (i64, Option<i64>)| {
        let m = *sc.models.get((model - 1) as usize).ok_or_else(|| rt_err(format!("no model {model}")))?;
        let parent = match parent {
            Some(p) => Some(sc.node(p)?.node),
            None => None,
        };
        let root = m.spawn(parent).ok_or_else(|| rt_err("the model didn't spawn (too many nodes?)"))?;
        Ok(sc.add_node(root, true, Some(m)))
    })?;
    reg(lua, &api, "find", s, |_, sc, (id, name): (i64, String)| {
        let n = sc.node(id)?.node;
        Ok(n.find(&name).map(|found| sc.add_node(found, false, None)))
    })?;
    reg(lua, &api, "node_material", s, |_, sc, id: i64| {
        Ok(match sc.node(id)?.node.material() {
            Some(m) => {
                sc.materials.push((m, false));
                Some(sc.materials.len() as i64)
            }
            None => None,
        })
    })?;
    // play(id, clip, once, fade, restart): crossfades to a clip of the model spawned at id.
    reg(lua, &api, "play", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let name = match a.get(1) {
            Value::String(s) => s.to_str()?,
            _ => return Err(rt_err("play(clip): the clip's name")),
        };
        let (once, fade, restart) = (a.flag(2), a.opt(3), a.flag(4));
        let n = sc.node_mut(a.int(0)?)?;
        let Some(model) = n.model else { return Err(rt_err("play: not a spawned model")) };
        let a = n.anim.get_or_insert_with(|| Box::new(Anim { animator: Animator::new(), clips: Vec::new() }));
        let clip = match a.clips.iter().find(|c| c.0 == name) {
            Some(c) => c.1,
            None => {
                let c = model.clip(&name).unwrap_or(Clip { index: 0, duration: 1.0 });
                a.clips.push((name.clone(), c));
                c
            }
        };
        let mode = if once.unwrap_or(false) { Play::Once } else { Play::Loop };
        let fade = fade.unwrap_or(0.15) as f32;
        if restart.unwrap_or(false) {
            a.animator.restart(clip, mode, fade);
        } else {
            a.animator.play(clip, mode, fade);
        }
        Ok(())
    })?;
    reg(lua, &api, "anim_speed", s, |_, sc, (id, k): (i64, f64)| {
        if let Some(a) = &mut sc.node_mut(id)?.anim {
            a.animator.set_speed(k as f32);
        }
        Ok(())
    })?;
    reg(lua, &api, "clip", s, |_, sc, id: i64| {
        let n = sc.node(id)?;
        Ok(n.anim.as_ref().and_then(|a| {
            let cur = a.animator.current()?;
            a.clips.iter().find(|c| c.1 == cur).map(|c| c.0.clone())
        }))
    })?;

    // Particles.
    reg(lua, &api, "emitter", s, |_, sc, t: Table| {
        let colors = match field(&t, "colors") {
            Value::Table(c) => {
                let mut out = [[0.0f32; 4]; 4];
                for (i, o) in out.iter_mut().enumerate() {
                    *o = rgba(&c.raw_get::<_, Value>(i as i64 + 1).unwrap_or(Value::Nil), [1.0; 4]);
                }
                out
            }
            _ => [[1.0; 4]; 4],
        };
        let base = EmitterDesc::default();
        let d = EmitterDesc {
            colors,
            sizes: nums(&field(&t, "sizes"), base.sizes),
            lifetime: nums(&field(&t, "life"), base.lifetime),
            speed: nums(&field(&t, "speed"), base.speed),
            spread: f(&t, "spread", base.spread),
            drag: f(&t, "drag", base.drag),
            blend: if flag(&t, "alpha") { FxBlend::Alpha } else { FxBlend::Additive },
            gravity: v3(&field(&t, "gravity"), base.gravity),
            stretch: f(&t, "stretch", base.stretch),
            ..base
        };
        let e = Emitter::new(&d).ok_or_else(|| rt_err("too many emitters"))?;
        sc.emitters.push(e);
        Ok(sc.emitters.len() as i64)
    })?;
    reg(lua, &api, "emit", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let g = |i: usize| a.opt(i).unwrap_or(0.0) as f32;
        let id = a.int(0)?;
        let e = *sc.emitters.get((id - 1) as usize).ok_or_else(|| rt_err(format!("no emitter {id}")))?;
        let (p, d, n) = (vec3(g(1), g(2), g(3)), vec3(g(4), g(5), g(6)), a.opt(7).unwrap_or(1.0) as u32);
        if a.opt(8).is_some() {
            e.emit_moving(p, d, n, vec3(g(8), g(9), g(10)));
        } else {
            e.emit(p, d, n);
        }
        Ok(())
    })?;

    // The camera, the light and the look.
    reg(lua, &api, "camera", s, |_, sc, v: Variadic<f64>| {
        let a: Vec<f32> = v.into_iter().map(|x| x as f32).collect();
        let g = |i: usize, d: f32| a.get(i).copied().unwrap_or(d);
        let pos = vec3(g(0, 0.0), g(1, 1.0), g(2, 5.0));
        let target = vec3(g(3, 0.0), g(4, 0.0), g(5, 0.0));
        let c = Camera { pos, rot: look_at(pos, target, Vec3::Y), fov_y: g(6, 0.9), near: g(7, 0.1), far: g(8, 200.0) };
        gfx3d::camera(&c);
        sc.camera = Some(c);
        Ok(())
    })?;
    reg(lua, &api, "project", s, |_, _, (x, y, z): (f64, f64, f64)| {
        let p = gfx3d::project(vec3(x as f32, y as f32, z as f32));
        Ok((p.x as f64, p.y as f64, p.on_screen))
    })?;
    reg(lua, &api, "sky", s, |_, _, t: Table| {
        let c = match field(&t, "colors") {
            Value::Table(c) => {
                let mut out = [[0.0f32; 3]; 3];
                for (i, o) in out.iter_mut().enumerate() {
                    *o = rgb(&c.raw_get::<_, Value>(i as i64 + 1).unwrap_or(Value::Nil), [0.0; 3]);
                }
                out
            }
            _ => Sky::default().colors,
        };
        let d = Sky::default();
        gfx3d::sky(&Sky {
            seed: f(&t, "seed", d.seed as f32) as u32,
            colors: c,
            nebula_density: f(&t, "nebula", d.nebula_density),
            star_density: f(&t, "stars", d.star_density),
            star_brightness: f(&t, "star_brightness", d.star_brightness),
            sun_disc: f(&t, "sun_disc", d.sun_disc),
            ..d
        });
        Ok(())
    })?;
    reg(lua, &api, "sun", s, |_, _, t: Table| {
        let d = Sun::default();
        gfx3d::sun(&Sun {
            direction: v3(&field(&t, "dir"), d.direction),
            color: rgb(&field(&t, "color"), d.color),
            intensity: f(&t, "intensity", d.intensity),
            shadows: flag(&t, "shadows") as u32,
            shadow_distance: f(&t, "shadow_distance", d.shadow_distance),
        });
        Ok(())
    })?;
    reg(lua, &api, "post", s, |_, _, t: Table| {
        let d = Post::default();
        gfx3d::post(&Post {
            exposure: f(&t, "exposure", d.exposure),
            bloom_strength: f(&t, "bloom", d.bloom_strength),
            bloom_threshold: f(&t, "bloom_threshold", d.bloom_threshold),
            vignette: f(&t, "vignette", d.vignette),
            chromatic_aberration: f(&t, "aberration", d.chromatic_aberration),
            saturation: f(&t, "saturation", d.saturation),
            contrast: f(&t, "contrast", d.contrast),
            sharpen: f(&t, "sharpen", d.sharpen),
        });
        Ok(())
    })?;
    reg(lua, &api, "fog", s, |_, _, t: Table| {
        let mode = match field(&t, "mode") {
            Value::String(m) => match m.to_str()?.as_str() {
                "linear" => FogMode::Linear,
                "exp2" => FogMode::ExponentialSquared,
                "off" => FogMode::Off,
                _ => FogMode::Exponential,
            },
            _ => FogMode::Exponential,
        };
        let d = Fog::default();
        gfx3d::fog(&Fog {
            color: rgb(&field(&t, "color"), d.color),
            mode,
            start: f(&t, "start", d.start),
            end: f(&t, "end", d.end),
            density: f(&t, "density", d.density),
            height: f(&t, "height", d.height),
            height_falloff: f(&t, "falloff", d.height_falloff),
            sky: f(&t, "sky", d.sky),
            ..d
        });
        Ok(())
    })?;
    reg(lua, &api, "dof", s, |_, _, t: Option<Table>| {
        let d = match t {
            Some(t) => DepthOfField { focus: f(&t, "focus", 5.0), range: f(&t, "range", 1.0), blur: f(&t, "blur", 4.0), near: f(&t, "near", 0.4) },
            None => DepthOfField { focus: 0.0, range: 0.0, blur: 0.0, near: 0.0 },
        };
        gfx3d::depth_of_field(&d);
        Ok(())
    })?;
    // The beauty pass by the SDK's presets, with a few overrides.
    reg(lua, &api, "grade", s, |_, _, (name, t): (String, Option<Table>)| {
        let mut g = match name.as_str() {
            "moonlit" => Grade::moonlit(),
            _ => Grade::default(),
        };
        if let Some(t) = &t {
            g.amount = f(t, "amount", g.amount);
        }
        gfx3d::grade(&g);
        Ok(())
    })?;
    reg(lua, &api, "shafts", s, |_, _, name: String| {
        gfx3d::shafts(&if name == "moonbeams" { LightShafts::moonbeams() } else { LightShafts::default() });
        Ok(())
    })?;
    reg(lua, &api, "halos", s, |_, _, (name, t): (String, Option<Table>)| {
        let mut h = if name == "mist" { Halos::mist() } else { Halos::default() };
        if let Some(t) = &t {
            h.reach = f(t, "reach", h.reach);
        }
        gfx3d::halos(&h);
        Ok(())
    })?;
    reg(lua, &api, "film", s, |_, _, (name, t): (String, Option<Table>)| {
        let mut fl = if name == "subtle" { Film::subtle() } else { Film::default() };
        if let Some(t) = &t {
            fl.streaks = f(t, "streaks", fl.streaks);
            fl.grain = f(t, "grain", fl.grain);
        }
        gfx3d::film(&fl);
        Ok(())
    })?;

    // Sound and haptics: output only, so replays never see them.
    reg(lua, &api, "sfx", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let name = match a.get(0) {
            Value::String(n) => n.to_str()?,
            _ => return Err(rt_err("sfx(name): the sound's name")),
        };
        let snd = sc.audio.borrow().sound(&name)?;
        snd.play(a.opt(1).unwrap_or(1.0) as f32, a.opt(2).unwrap_or(0.0) as f32, a.opt(3).unwrap_or(1.0) as f32);
        Ok(())
    })?;
    // music("theme", vol, fade): loops on the music bus, and carries on into the next round; music(nil) fades it out.
    reg(lua, &api, "music", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let (vol, fade) = (a.opt(1).unwrap_or(0.7) as f32, a.opt(2).unwrap_or(0.5) as f32);
        let name = match a.get(0) {
            Value::String(n) => Some(n.to_str()?.trim_end_matches(".ogg").to_string()),
            _ => None,
        };
        let mut au = sc.audio.borrow_mut();
        if let (Some(n), Some((cur, voice))) = (&name, &au.music)
            && n == cur
        {
            voice.set(vol, 0.0, 1.0);
            return Ok(());
        }
        if let Some((_, voice)) = au.music.take() {
            voice.stop_at(sys::time(), fade);
        }
        if let Some(n) = name {
            let snd = au.sound(&n)?;
            au.music = Some((n, snd.play_looped(vol, 0.0, 1.0).bus(Bus::Music)));
        }
        Ok(())
    })?;
    // loop(name, vol, pan, pitch): a sound that repeats until stopped (an engine hum), for this round.
    reg(lua, &api, "loop", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let name = match a.get(0) {
            Value::String(n) => n.to_str()?,
            _ => return Err(rt_err("loop(name): the sound's name")),
        };
        let snd = sc.audio.borrow().sound(&name)?;
        sc.voices.push(snd.play_looped(a.opt(1).unwrap_or(1.0) as f32, a.opt(2).unwrap_or(0.0) as f32, a.opt(3).unwrap_or(1.0) as f32));
        Ok(sc.voices.len() as i64)
    })?;
    reg(lua, &api, "voice_set", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        let id = a.int(0)?;
        let voice = *sc.voices.get((id - 1) as usize).ok_or_else(|| rt_err(format!("no voice {id}")))?;
        voice.set(a.opt(1).unwrap_or(1.0) as f32, a.opt(2).unwrap_or(0.0) as f32, a.opt(3).unwrap_or(1.0) as f32);
        Ok(())
    })?;
    reg(lua, &api, "voice_stop", s, |_, sc, (id, fade): (i64, Option<f64>)| {
        let voice = *sc.voices.get((id - 1) as usize).ok_or_else(|| rt_err(format!("no voice {id}")))?;
        voice.stop_at(sys::time(), fade.unwrap_or(0.1) as f32);
        Ok(())
    })?;
    // lowpass("music" | "sfx" | "master", cutoff Hz, seconds): muffle a bus (20000 opens it again).
    reg(lua, &api, "lowpass", s, |_, _, (bus, cutoff, ramp): (String, f64, Option<f64>)| {
        let b = match bus.as_str() {
            "music" => Bus::Music,
            "sfx" => Bus::Sfx,
            "master" => Bus::Master,
            o => return Err(rt_err(format!("lowpass(\"{o}\"): music, sfx or master"))),
        };
        b.lowpass(cutoff as f32, 0.707, ramp.unwrap_or(0.2) as f32);
        Ok(())
    })?;
    reg(lua, &api, "haptic", s, |_, _, kind: Option<String>| {
        sensors::haptic(match kind.as_deref().unwrap_or("tap") {
            "tap" => Haptic::Tap,
            "success" => Haptic::Success,
            "fail" => Haptic::Fail,
            "heavy" => Haptic::Heavy,
            o => return Err(rt_err(format!("haptic(\"{o}\"): tap, success, fail or heavy"))),
        });
        Ok(())
    })?;

    // 2D over the 3D, this frame (logical units). Not through `reg`: the
    // text comes from Lua's `tostring`, which may run a `__tostring` that
    // calls a binding, so it's made before the scene is borrowed. (Any
    // binding that calls back into Lua has to do the same.)
    let sc_text = s.clone();
    api.set("hud_text", lua.create_function(move |lua, v: Variadic<Value>| {
        let a = args(v);
        let s = to_s(lua, a.get(0).clone())?;
        let (x, y) = (a.num(1)?, a.num(2)?);
        let t = match a.get(3) {
            Value::Table(t) => Some(t.clone()),
            _ => None,
        };
        let (size, color, align, shadow) = match &t {
            Some(t) => (
                f(t, "size", 16.0),
                match field(t, "color") {
                    Value::Integer(c) => c as u32,
                    _ => 0xffffffff,
                },
                match field(t, "align") {
                    Value::String(a) => match a.to_str()?.as_str() {
                        "left" => -1,
                        "right" => 1,
                        _ => 0,
                    },
                    _ => 0,
                },
                !matches!(field(t, "shadow"), Value::Boolean(false)),
            ),
            None => (16.0, 0xffffffff, 0, true),
        };
        sc_text.borrow_mut().hud.push(Hud::Text { s, x: x as f32, y: y as f32, size, color, align, shadow });
        Ok(())
    })?)?;
    reg(lua, &api, "hud_rect", s, |_, sc, v: Variadic<Value>| {
        let a = args(v);
        sc.hud.push(Hud::Rect { x: a.num(0)? as f32, y: a.num(1)? as f32, w: a.num(2)? as f32, h: a.num(3)? as f32, color: a.int(4)? as u32 });
        Ok(())
    })?;
    Ok(api)
}
