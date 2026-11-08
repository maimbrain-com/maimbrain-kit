//! Engine 1.0's 3D names on 2.0's VM and scene (docs/STUDIO-2.md §9.2 row
//! 1b; docs/STUDIO-3D-API.md is 1.0's page, docs/STUDIO-2-API.md [3d] this
//! one): `mesh.box(…)`, `material{…}`, `node(mesh, material)` with
//! `n:place(…)` and the rest, `model`, `spawn`, `emitter`, `camera(…)`,
//! `project`, the look (`sky`, `sun`, `fog`, `post`, `dof`, `grade`,
//! `shafts`, `halos`, `film`), the 2D drawn over it (`text`, `rect`) and
//! sound (`sfx`, `loop`, `lowpass`). Phase 2 adds three.js's and Godot's
//! names beside these, which stay as aliases (Decided 7, 15).
//!
//! What changes from 1.0 is underneath (§4.3): a node is a C userdata with
//! write-through properties, as a 2D node is (`x`, `y`, `z`, `rx`, `ry`,
//! `rz`, `scale`, `sx`, `sy`, `sz`, `vx`, `vy`, `vz`, `gravity`, `vrx`,
//! `vry`, `vrz`, `visible`, …), and 1.0's methods are methods on it. A write changes the
//! scene in Rust; mb3d hears about the nodes that changed once, at the end
//! of the update (three.rs). The rest of 1.0's 3D page (`score`,
//! `game_over`, `on("tick")`, `timer`, `save`, …) is Lua, in prelude3d.lua.
//!
//! Colours are 1.0's here, as its 3D page has them: `0xRRGGBBAA` (sRGB, as
//! picked) where a colour has alpha (`material`'s `color`, `m:color`,
//! instances, `text`, `rect`, an emitter's colours) and `0xRRGGBB` where it
//! doesn't (`emissive`, `light`, the sky, the sun, the fog), or a table
//! `{ r, g, b(, a) }` in linear light, which may pass 1 to glow.

use std::ffi::{CStr, c_int, c_void};

use maimbrain::audio::Bus;
use maimbrain::gfx2d::Font;
use maimbrain::gfx3d::{
    self, Camera, DepthOfField, Emitter, EmitterDesc, Film, Fog, FogMode, FxBlend, Grade, Halos, Instance, LightShafts, Material, MaterialDesc, MaterialStyle, MeshData, Post, Quat, Sky, Sun, TextDesc, Transform,
    Vec3, look_at, srgb, srgba, vec3,
};
use maimbrain::scene::three::Node3Id;
use maimbrain::scene::{Align, Cmd};

use super::atoms::A;
use super::*;
use crate::three::World3;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register_kind(L, ud, tag::NODE3, c"Node3D", &[(c"__index", n_index), (c"__newindex", n_newindex), (c"__namecall", n_namecall), (c"__tostring", n_tostring)]);
        register_methods(
            L,
            ud,
            tag::GEOMETRY,
            c"Geometry",
            &[
                (c"color", g_color),
                (c"split", g_split),
                (c"move", g_move),
                (c"turn", g_turn),
                (c"scale", g_scale),
                (c"clamp_y", g_clamp_y),
                (c"add", g_add),
                (c"flat", g_flat),
                // 2.0's edits (api/world.rs)
                (c"merge", super::world::g_merge),
                (c"paint", super::world::g_paint),
                (c"smooth", super::world::g_smooth),
                (c"subdivide", super::world::g_subdivide),
                (c"__eq", same_handle),
            ],
        );
        register_methods(L, ud, tag::MATERIAL, c"Material", &[(c"style", m_style), (c"__eq", same_handle)]);
        register_methods(L, ud, tag::MODEL, c"ModelFile", &[]);
        register_methods(L, ud, tag::EMITTER3, c"Emitter3D", &[(c"emit", e_emit)]);
        register_methods(L, ud, tag::VOICE, c"Voice", &[(c"set", v_set), (c"stop", v_stop)]);
        register(
            L,
            ud,
            "mesh",
            &[
                (c"box", mk_box),
                (c"sphere", mk_sphere),
                (c"ico", mk_ico),
                (c"cylinder", mk_cylinder),
                (c"cone", mk_cone),
                (c"plane", mk_plane),
                (c"torus", mk_torus),
                (c"empty", mk_empty),
                (c"lathe", mk_lathe),
                (c"extrude", mk_extrude),
                (c"tris", mk_tris),
            ],
        );
        // Globals, over the quick path's `spawn` (2D's: a 3D game's spawn is a model's).
        let globals: &[Reg] = &[
            (c"material", material),
            (c"node", node),
            (c"model", model),
            (c"spawn", spawn),
            (c"emitter", emitter),
            (c"project", project),
            (c"sky", sky),
            (c"sun", sun),
            (c"fog", fog),
            (c"post", post),
            (c"dof", dof),
            (c"grade", grade),
            (c"shafts", shafts),
            (c"halos", halos),
            (c"film", film),
            (c"text", hud_text),
            (c"rect", hud_rect),
            (c"sfx", sfx),
            (c"loop", sound_loop),
            (c"lowpass", lowpass),
        ];
        for (name, f) in globals {
            sv_push_rfn(L, *f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        let w = world(host(ud)).unwrap();
        for r in [&mut w.handles_ref, &mut w.data_ref] {
            lua_newtable(L);
            *r = lua_ref(L, -1);
            lua_pop(L, 1);
        }
    }
}

// `==` on geometries and materials: the same one (a node's `geometry` reads back a new handle).
b!(same_handle, |L, h| {
    let same = lua_type(L, 1) == LUA_TUSERDATA && lua_type(L, 2) == LUA_TUSERDATA && lua_userdatatag(L, 1) == lua_userdatatag(L, 2) && *(lua_touserdata(L, 1) as *const u32) == *(lua_touserdata(L, 2) as *const u32);
    retb(L, same)
});

/// The round's 3D world, if the game is 3D.
pub fn world(h: &mut Host) -> Option<&mut World3> {
    h.three.as_deref_mut()
}

pub(crate) const NOT_3D: &str = "this is 3D's (kind = \"3d\" in studio.toml)";

macro_rules! w {
    ($L:ident, $h:ident) => {
        match $h.three.as_deref_mut() {
            Some(w) => w,
            None => return err($L, NOT_3D),
        }
    };
}

// ------------------------------------------------------------- reading values

/// An absolute stack index.
pub(crate) unsafe fn abs(L: *mut lua_State, i: c_int) -> c_int {
    if i < 0 && i > LUA_REGISTRYINDEX { unsafe { lua_gettop(L) + i + 1 } } else { i }
}

/// A whole number as a `0x…` colour.
pub(crate) fn hex(v: f64) -> Option<u32> {
    (v.is_finite() && v >= 0.0 && v <= u32::MAX as f64 && v == v.trunc()).then_some(v as u32)
}

/// Up to `N` numbers from an array table at `i`.
pub(crate) unsafe fn nums_at<const N: usize>(L: *mut lua_State, i: c_int, d: [f32; N]) -> [f32; N] {
    unsafe {
        let mut out = d;
        if lua_type(L, i) == LUA_TTABLE {
            let i = abs(L, i);
            for (k, o) in out.iter_mut().enumerate() {
                lua_rawgeti(L, i, k as c_int + 1);
                if let Some(x) = num(L, -1).filter(|x| x.is_finite()) {
                    *o = x as f32;
                }
                lua_pop(L, 1);
            }
        }
        out
    }
}

/// A colour without alpha: `0xRRGGBB` (sRGB) or `{ r, g, b }` (linear).
pub(crate) unsafe fn rgb_at(L: *mut lua_State, i: c_int, d: [f32; 3]) -> [f32; 3] {
    unsafe {
        match lua_type(L, i) {
            LUA_TNUMBER => hex(lua_tonumber(L, i)).map_or(d, srgb),
            LUA_TTABLE => nums_at(L, i, d),
            _ => d,
        }
    }
}

/// A colour with alpha: `0xRRGGBBAA` (sRGB) or `{ r, g, b, a }` (linear).
pub(crate) unsafe fn rgba_at(L: *mut lua_State, i: c_int, d: [f32; 4]) -> [f32; 4] {
    unsafe {
        match lua_type(L, i) {
            LUA_TNUMBER => hex(lua_tonumber(L, i)).map_or(d, srgba),
            LUA_TTABLE => nums_at(L, i, d),
            _ => d,
        }
    }
}

/// Reads field `k` of the table at `t` with `f`, the field on top of the stack.
pub(crate) unsafe fn with_field<R>(L: *mut lua_State, t: c_int, k: &CStr, f: impl FnOnce() -> R) -> R {
    unsafe {
        let t = abs(L, t);
        if lua_type(L, t) == LUA_TTABLE {
            lua_rawgetfield(L, t, k.as_ptr());
        } else {
            lua_pushnil(L);
        }
        let r = f();
        lua_pop(L, 1);
        r
    }
}

pub(crate) unsafe fn fnum(L: *mut lua_State, t: c_int, k: &CStr, d: f32) -> f32 {
    unsafe { with_field(L, t, k, || num(L, -1).filter(|x| x.is_finite()).map_or(d, |x| x as f32)) }
}

pub(crate) unsafe fn fflag(L: *mut lua_State, t: c_int, k: &CStr) -> bool {
    unsafe { with_field(L, t, k, || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) != 0) }
}

pub(crate) unsafe fn frgb(L: *mut lua_State, t: c_int, k: &CStr, d: [f32; 3]) -> [f32; 3] {
    unsafe { with_field(L, t, k, || rgb_at(L, -1, d)) }
}

pub(crate) unsafe fn frgba(L: *mut lua_State, t: c_int, k: &CStr, d: [f32; 4]) -> [f32; 4] {
    unsafe { with_field(L, t, k, || rgba_at(L, -1, d)) }
}

pub(crate) unsafe fn fnums<const N: usize>(L: *mut lua_State, t: c_int, k: &CStr, d: [f32; N]) -> [f32; N] {
    unsafe { with_field(L, t, k, || nums_at(L, -1, d)) }
}

/// A flat array of numbers.
pub(crate) unsafe fn flat(L: *mut lua_State, i: c_int, f: &str) -> Result<Vec<f32>, String> {
    unsafe { Ok(flat64(L, i, f)?.into_iter().map(|x| x as f32).collect()) }
}

/// A flat array of numbers, as doubles (a `0xRRGGBBAA` colour needs all 32 bits).
pub(crate) unsafe fn flat64(L: *mut lua_State, i: c_int, f: &str) -> Result<Vec<f64>, String> {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return Err(format!("{f} takes a flat list of numbers, got {}", type_name(L, i)));
        }
        let n = lua_objlen(L, i);
        let mut out = Vec::with_capacity(n as usize);
        for k in 1..=n {
            lua_rawgeti(L, i, k);
            let x = num(L, -1);
            lua_pop(L, 1);
            out.push(x.ok_or_else(|| format!("{f}: item {k} isn't a number"))?);
        }
        Ok(out)
    }
}

/// A required number argument.
pub(crate) unsafe fn arg(L: *mut lua_State, i: c_int, f: &str) -> Result<f32, c_int> {
    unsafe {
        match num(L, i) {
            Some(v) if v.is_finite() => Ok(v as f32),
            _ => Err(bad(L, i, f, "number")),
        }
    }
}

/// Turns as 1.0's `turn(yaw, pitch, roll)` makes them: y, then x, then z.
fn rot(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_euler(yaw, pitch, roll)
}

// ------------------------------------------------------------------ nodes

pub unsafe fn node_at(L: *mut lua_State, i: c_int) -> Option<Node3Id> {
    unsafe { handle(L, i, tag::NODE3).map(Node3Id::from_bits) }
}

/// Pushes the node's handle: the same userdata every time.
pub unsafe fn push_node(L: *mut lua_State, w: &World3, id: Node3Id) {
    unsafe {
        lua_rawgeti(L, LUA_REGISTRYINDEX, w.handles_ref);
        lua_rawgeti(L, -1, id.to_bits() as c_int);
        if lua_type(L, -1) == LUA_TUSERDATA {
            lua_remove(L, -2);
            return;
        }
        lua_pop(L, 1);
        push_handle(L, tag::NODE3, id.to_bits());
        lua_pushvalue(L, -1);
        lua_rawseti(L, -3, id.to_bits() as c_int);
        lua_remove(L, -2);
    }
}

/// A 3D node's properties.
pub fn is_prop(a: A) -> bool {
    use A::*;
    matches!(a, X | Y | Z | Rx | Ry | Rz | Scale | Sx | Sy | Sz | Vx | Vy | Vz | Gravity | Vrx | Vry | Vrz | Visible | Parent | Children | Data | Alive)
}

/// A 3D node's methods (1.0's names; `parent` is a property and a method).
pub fn is_method(a: A) -> bool {
    use A::*;
    matches!(a, Place | Pos | Turn | Size | Parent | Show | Destroy | World | Fade | Light | Text | Instances | Find | Material | Play | Speed | Clip)
}

/// A node's number, for reads and tweens.
pub fn number(w: &World3, id: Node3Id, a: A) -> Option<f64> {
    if let Some(v) = super::bodies3d::velocity(w, id, a) {
        return Some(super::nodes3d::nice(v));
    }
    let n = w.scene.get(id)?;
    Some(super::nodes3d::nice(match a {
        A::X => n.x,
        A::Y => n.y,
        A::Z => n.z,
        A::Rx => n.rx,
        A::Ry => n.ry,
        A::Rz => n.rz,
        A::Scale | A::Sx => n.sx,
        A::Sy => n.sy,
        A::Sz => n.sz,
        A::Vx => n.vx,
        A::Vy => n.vy,
        A::Vz => n.vz,
        A::Gravity => n.gravity,
        A::Vrx => n.vrx,
        A::Vry => n.vry,
        A::Vrz => n.vrz,
        _ => return None,
    }))
}

/// Writes a node's number (checked already). Velocity, gravity and spin
/// don't send its transform: a model's part keeps its pose until it moves.
pub fn set_number(w: &mut World3, id: Node3Id, a: A, v: f32) -> bool {
    if super::bodies3d::set_velocity(w, id, a, v) {
        return true;
    }
    match a {
        A::Vx => return w.scene.set_motion(id, [Some(v), None, None], None),
        A::Vy => return w.scene.set_motion(id, [None, Some(v), None], None),
        A::Vz => return w.scene.set_motion(id, [None, None, Some(v)], None),
        A::Gravity => return w.scene.set_motion(id, [None; 3], Some(v)),
        A::Vrx => return w.scene.set_spin(id, [Some(v), None, None]),
        A::Vry => return w.scene.set_spin(id, [None, Some(v), None]),
        A::Vrz => return w.scene.set_spin(id, [None, None, Some(v)]),
        _ => {}
    }
    let Some(n) = w.scene.node_mut(id) else { return false };
    match a {
        A::X => n.x = v,
        A::Y => n.y = v,
        A::Z => n.z = v,
        A::Rx => n.rx = v,
        A::Ry => n.ry = v,
        A::Rz => n.rz = v,
        A::Scale => (n.sx, n.sy, n.sz) = (v, v, v),
        A::Sx => n.sx = v,
        A::Sy => n.sy = v,
        A::Sz => n.sz = v,
        _ => return false,
    }
    true
}

/// The atom of the key at `i` and its text.
pub(crate) unsafe fn key_at(L: *mut lua_State, i: c_int) -> (Option<A>, String) {
    unsafe {
        if lua_type(L, i) != LUA_TSTRING {
            return (None, to_s(L, i));
        }
        let mut atom = -1;
        let mut len = 0;
        let p = lua_tolstringatom(L, i, &mut len, &mut atom);
        (A::from(atom), String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len)).into_owned())
    }
}

fn unknown(key: &str, method: bool) -> String {
    fn near(names: &'static [&'static str], key: &str) -> Option<&'static str> {
        names.iter().find(|n| n.eq_ignore_ascii_case(key) || (key.len() > 3 && key.is_char_boundary(3) && n.starts_with(&key[..3]))).copied()
    }
    if method {
        const M: &[&str] = &["place", "pos", "turn", "size", "parent", "show", "destroy", "world", "fade", "light", "text", "instances", "find", "material", "play", "speed", "clip"];
        match near(M, key) {
            Some(n) => format!("no method `{key}` on a 3D node; did you mean `{n}`?"),
            None => format!("no method `{key}` on a 3D node ({})", M.join(", ")),
        }
    } else {
        const P: &[&str] = &["x", "y", "z", "rx", "ry", "rz", "scale", "sx", "sy", "sz", "vx", "vy", "vz", "gravity", "vrx", "vry", "vrz", "visible", "parent", "children", "data", "alive"];
        match (near(P, key), key) {
            (_, "rotation" | "yaw") => format!("a 3D node turns with ry (yaw), rx (pitch) and rz (roll), or n:turn(yaw, pitch, roll); `{key}` is 2D's"),
            (Some(n), _) => format!("no property `{key}` on a 3D node; did you mean `{n}`?"),
            (None, _) => format!("no property `{key}` on a 3D node: a game's own values go in `node.data` (node.data.{key} = …)"),
        }
    }
}

b!(n_index, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let (a, key) = key_at(L, 2);
    let w = w!(L, h);
    let alive = w.scene.alive(id);
    match a {
        Some(A::Alive) => retb(L, alive),
        Some(a) if is_prop(a) && !alive => err(L, &format!("this node was destroyed: reading {key} (check node.alive)")),
        Some(A::Visible) => retb(L, w.scene.get(id).unwrap().visible),
        Some(A::Parent) => match w.scene.get(id).unwrap().parent() {
            Some(p) => {
                push_node(L, w, p);
                1
            }
            None => {
                lua_pushnil(L);
                1
            }
        },
        Some(A::Children) => {
            let kids = w.scene.get(id).unwrap().children().to_vec();
            lua_createtable(L, kids.len() as c_int, 0);
            for (i, c) in kids.into_iter().enumerate() {
                push_node(L, w, c);
                lua_rawseti(L, -2, i as c_int + 1);
            }
            1
        }
        Some(A::Data) => {
            lua_rawgeti(L, LUA_REGISTRYINDEX, w.data_ref);
            lua_rawgeti(L, -1, id.to_bits() as c_int);
            if lua_type(L, -1) != LUA_TTABLE {
                lua_pop(L, 1);
                lua_newtable(L);
                lua_pushvalue(L, -1);
                lua_rawseti(L, -3, id.to_bits() as c_int);
            }
            lua_remove(L, -2);
            1
        }
        Some(a @ (A::Gravity | A::Speed)) if w.kind(id) == crate::three::Kind3::Emitter => super::nodes3d::get(L, h, id, a),
        Some(a) if super::nodes3d::is_shared(a) => super::nodes3d::get(L, h, id, a),
        Some(a) if is_prop(a) => ret(L, number(w, id, a).unwrap_or(0.0)),
        Some(a) if super::nodes3d::has(w.kind(id), a) => super::nodes3d::get(L, h, id, a),
        _ => err(L, &super::nodes3d::unknown(w.kind(id), &key, false)),
    }
});

b!(n_newindex, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let (a, key) = key_at(L, 2);
    match set_any(L, h, id, a, &key, 3) {
        Ok(()) => 0,
        Err(m) if m.starts_with(&key) => err(L, &format!("node.{m}")),
        Err(m) => err(L, &m),
    }
});

/// Writes property `a` (named `key`) of 3D node `id` from the value at
/// stack index `v`: `node.key = value`, and each key of a constructor's
/// table. A write beats motion: a tween moving the property stops.
pub(crate) unsafe fn set_any(L: *mut lua_State, h: &mut Host, id: Node3Id, a: Option<A>, key: &str, v: c_int) -> Result<(), String> {
    unsafe {
        let v = abs(L, v);
        let Some(w) = h.three.as_deref_mut() else { return Err(NOT_3D.into()) };
        if !w.scene.alive(id) {
            return Err(format!("this node was destroyed: setting {key} does nothing (check node.alive)"));
        }
        let kind = w.kind(id);
        if kind == crate::three::Kind3::Terrain && !a.is_some_and(super::nodes3d::terrain_allows) {
            return Err(super::terrain::fixed(key));
        }
        let r: Result<(), String> = match a {
            Some(A::Visible) if kind == crate::three::Kind3::Terrain => match lua_type(L, v) {
                LUA_TBOOLEAN => {
                    let on = lua_toboolean(L, v) != 0;
                    w.scene.set_visible(id, on);
                    if let Some(g) = w.ground.as_mut().filter(|g| g.node == id) {
                        g.terrain.set_visible(on);
                    }
                    Ok(())
                }
                _ => Err(format!("visible takes true or false, got {}", type_name(L, v))),
            },
            Some(A::Visible) => match lua_type(L, v) {
                LUA_TBOOLEAN => {
                    w.scene.set_visible(id, lua_toboolean(L, v) != 0);
                    Ok(())
                }
                _ => Err(format!("visible takes true or false, got {}", type_name(L, v))),
            },
            Some(A::Parent) => super::nodes3d::set_parent(L, w, id, v, true),
            Some(A::Children | A::Alive) => Err(format!("{key} is read-only")),
            Some(A::Data) => {
                if lua_type(L, v) != LUA_TTABLE {
                    Err(format!("data takes a table, got {}", type_name(L, v)))
                } else {
                    lua_rawgeti(L, LUA_REGISTRYINDEX, w.data_ref);
                    lua_pushvalue(L, v);
                    lua_rawseti(L, -2, id.to_bits() as c_int);
                    lua_pop(L, 1);
                    Ok(())
                }
            }
            Some(a @ (A::Gravity | A::Speed)) if kind == crate::three::Kind3::Emitter => super::nodes3d::set(L, h, id, a, v),
            Some(a) if is_prop(a) => match num(L, v) {
                Some(x) if x.is_finite() => {
                    set_number(w, id, a, x as f32);
                    Ok(())
                }
                Some(x) => Err(format!("{key} takes a number, got {}", fmt_num(x))),
                None => Err(format!("{key} takes a number, got {}", type_name(L, v))),
            },
            Some(a) if super::nodes3d::is_shared(a) || (super::nodes3d::has(kind, a) && !super::nodes3d::is_method(a)) => super::nodes3d::set(L, h, id, a, v),
            _ => Err(super::nodes3d::unknown(kind, key, false)),
        };
        if r.is_ok() {
            h.libs.written(crate::libs::Target::Node3(id), key);
        }
        r
    }
}

b!(n_tostring, |L, h| {
    let Some(id) = node_at(L, 1) else { return rets(L, "node") };
    match h.three.as_deref().and_then(|w| w.scene.get(id)) {
        Some(n) if !n.owned => rets(L, "3D node (a model's part)"),
        Some(_) => rets(L, "3D node"),
        None => rets(L, "destroyed 3D node"),
    }
});

b!(n_namecall, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let mut atom = -1;
    let name = cstr(lua_namecallatom(L, &mut atom)).unwrap_or_default();
    let kind = h.three.as_deref().map_or(crate::three::Kind3::Node, |w| w.kind(id));
    let a = A::from(atom).filter(|a| is_method(*a) || super::nodes3d::is_method3(kind, *a));
    if kind == crate::three::Kind3::Terrain && !A::from(atom).is_some_and(super::nodes3d::terrain_allows) {
        return err(L, &super::terrain::fixed(&name));
    }
    if a.is_none() {
        // Methods written in Lua (the prelude's createTween).
        return super::node::node_ext(L, h, &name).unwrap_or_else(|| err(L, &super::nodes3d::unknown(kind, &name, true)));
    }
    if let Some(a) = a.filter(|a| super::nodes3d::is_method3(kind, *a) && !(is_method(*a) && kind == crate::three::Kind3::Node)) {
        let alive = h.three.as_deref().is_some_and(|w| w.scene.alive(id));
        if !alive {
            return if a == A::Destroy { 0 } else { err(L, &format!("this node was destroyed: calling {name} (check node.alive)")) };
        }
        return match super::nodes3d::method(L, h, id, a, &name) {
            Ok(n) => n,
            Err(m) => err(L, &m),
        };
    }
    let w = w!(L, h);
    if !w.scene.alive(id) {
        return match a {
            Some(A::Destroy) => 0,
            _ => err(L, &format!("this node was destroyed: calling {name} (check node.alive)")),
        };
    }
    match method(L, w, id, a.unwrap(), &name) {
        Ok(n) => n,
        Err(m) => err(L, &m),
    }
});

unsafe fn method(L: *mut lua_State, w: &mut World3, id: Node3Id, a: A, name: &str) -> Result<c_int, String> {
    unsafe {
        let need = |i: c_int, what: &str| -> Result<f32, String> {
            match num(L, i) {
                Some(v) if v.is_finite() => Ok(v as f32),
                _ => Err(format!("bad argument #{} to '{name}' ({what} expected, got {})", i - 1, type_name(L, i))),
            }
        };
        let host = w.scene.get(id).and_then(|n| n.host);
        Ok(match a {
            // place(x, y, z, yaw, size): where, which way it faces, how big, in one call.
            A::Place => {
                let (x, y, z) = (need(2, "number")?, need(3, "number")?, need(4, "number")?);
                let (yaw, s) = (opt(L, 5, 0.0) as f32, opt(L, 6, 1.0) as f32);
                let n = w.scene.node_mut(id).unwrap();
                (n.x, n.y, n.z, n.rx, n.ry, n.rz, n.sx, n.sy, n.sz) = (x, y, z, 0.0, yaw, 0.0, s, s, s);
                0
            }
            // pos(x, y, z), pos({ x, y, z }), or pos() → x, y, z.
            A::Pos => {
                if lua_type(L, 2) == LUA_TNIL || lua_gettop(L) < 2 {
                    let n = w.scene.get(id).unwrap();
                    lua_pushnumber(L, n.x as f64);
                    lua_pushnumber(L, n.y as f64);
                    lua_pushnumber(L, n.z as f64);
                    return Ok(3);
                }
                let [x, y, z] = if lua_type(L, 2) == LUA_TTABLE { nums_at(L, 2, [0.0; 3]) } else { [need(2, "number")?, need(3, "number")?, need(4, "number")?] };
                let n = w.scene.node_mut(id).unwrap();
                (n.x, n.y, n.z) = (x, y, z);
                0
            }
            A::Turn => {
                let (yaw, pitch, roll) = (need(2, "number")?, opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32);
                let n = w.scene.node_mut(id).unwrap();
                (n.ry, n.rx, n.rz) = (yaw, pitch, roll);
                0
            }
            A::Size => {
                let x = need(2, "number")?;
                let (y, z) = (opt(L, 3, x as f64) as f32, opt(L, 4, x as f64) as f32);
                let n = w.scene.node_mut(id).unwrap();
                (n.sx, n.sy, n.sz) = (x, y, z);
                0
            }
            A::Parent => {
                let p = match lua_type(L, 2) {
                    LUA_TNIL | -1 => None,
                    _ => match node_at(L, 2) {
                        Some(p) if w.scene.alive(p) => Some(p),
                        Some(_) => return Err("parent: that node was destroyed".into()),
                        None => return Err(format!("parent takes a 3D node or nil, got {}", type_name(L, 2))),
                    },
                };
                if !w.scene.set_parent(id, p) {
                    return Err("parent: a node can't go under itself or its own children".into());
                }
                0
            }
            A::Show => {
                w.scene.set_visible(id, !(lua_type(L, 2) == LUA_TBOOLEAN && lua_toboolean(L, 2) == 0));
                0
            }
            A::Destroy => {
                // A model's part is the model's: destroying it does nothing (as 1.0).
                if w.scene.get(id).unwrap().owned {
                    let gone = w.scene.destroy(id);
                    w.forget(&gone);
                }
                0
            }
            A::World => {
                let p = w.scene.world_position(id).unwrap_or(Vec3::ZERO);
                lua_pushnumber(L, p.x as f64);
                lua_pushnumber(L, p.y as f64);
                lua_pushnumber(L, p.z as f64);
                3
            }
            A::Fade => {
                let (from, to) = (need(2, "number")?, need(3, "number")?);
                if let Some(h) = host {
                    h.set_fade(from, to);
                }
                0
            }
            A::Light => {
                let c = rgb_at(L, 2, [1.0; 3]);
                let (i, r) = (need(3, "number")?, need(4, "number")?);
                let ok = host.is_some_and(|h| h.point_light(c, i, r));
                lua_pushboolean(L, ok as c_int);
                1
            }
            A::Text => {
                let s = to_s(L, 2);
                if let Some(h) = host {
                    if s.is_empty() || lua_type(L, 2) == LUA_TNIL {
                        h.clear_text();
                    } else {
                        h.set_text(&text_desc(L, 3), &s);
                    }
                }
                0
            }
            A::Instances => {
                let stride = (opt(L, 3, 3.0) as usize).clamp(3, 9);
                let out = instances(&flat64(L, 2, "instances")?, stride);
                let ok = host.is_some_and(|h| h.set_instances(&out));
                lua_pushboolean(L, ok as c_int);
                1
            }
            A::Find => {
                let Some(part) = st(L, 2).map(str::to_string) else { return Err(format!("bad argument #1 to 'find' (string expected, got {})", type_name(L, 2))) };
                if let Some((_, _, f)) = w.found.iter().find(|f| f.0 == id && f.1 == part).filter(|f| w.scene.alive(f.2)) {
                    push_node(L, w, *f);
                    return Ok(1);
                }
                match host.and_then(|h| h.find(&part)) {
                    Some(found) => {
                        let f = w.scene.add(Some(found), false, Some(id)).map_err(|_| "too many nodes (4096 at most)".to_string())?;
                        w.found.push((id, part, f));
                        push_node(L, w, f);
                    }
                    None => lua_pushnil(L),
                }
                1
            }
            A::Material => {
                match host.and_then(|h| h.material()) {
                    Some(m) => {
                        w.materials.push((m, false));
                        push_handle(L, tag::MATERIAL, w.materials.len() as u32);
                    }
                    None => lua_pushnil(L),
                }
                1
            }
            // play(clip, { once, fade, restart }): crossfades to a clip of a spawned model.
            // play(clip, { once, fade, restart, speed, layer, mask }): a spawned model's clips.
            A::Play => super::nodes3d::play(L, w, id)?,
            A::Speed => {
                let k = need(2, "number")?;
                if let Some(a) = w.anims.iter_mut().find(|a| a.node == id) {
                    a.animator.set_speed(k);
                }
                0
            }
            A::Clip => {
                let name = w.anims.iter().find(|a| a.node == id).and_then(|a| {
                    let cur = a.animator.current()?;
                    a.clips.iter().find(|c| c.1 == cur).map(|c| c.0.clone())
                });
                match name {
                    Some(n) => push_str(L, &n),
                    None => lua_pushnil(L),
                }
                1
            }
            _ => return Err(unknown(name, true)),
        })
    }
}

/// Instances from a flat list: x, y, z, then by `stride` yaw, scale,
/// colour (`0xRRGGBBAA`), pitch, roll and tall (a y-stretch on the scale).
pub fn instances(list: &[f64], stride: usize) -> Vec<Instance> {
    let mut out = Vec::with_capacity(list.len() / stride);
    for row in list.chunks_exact(stride) {
        let g = |k: usize, d: f32| if k < stride { row[k] as f32 } else { d };
        let s = g(4, 1.0);
        out.push(Instance {
            pos: vec3(g(0, 0.0), g(1, 0.0), g(2, 0.0)),
            rot: rot(g(3, 0.0), g(6, 0.0), g(7, 0.0)),
            scale: vec3(s, s * g(8, 1.0), s),
            // The host takes it byte-swapped (Instance::with_color).
            color: if stride > 5 { (row[5] as i64 as u32).swap_bytes() } else { 0xffffffff },
        });
    }
    out
}

/// `n:text(s, { size, color, outline, width, font = "pixel", billboard, screen, ontop, nofog })`.
pub(crate) unsafe fn text_desc(L: *mut lua_State, t: c_int) -> TextDesc {
    unsafe {
        if lua_type(L, t) != LUA_TTABLE {
            return TextDesc::new(Font::SansBold, 0.3);
        }
        let font = if with_field(L, t, c"font", || st(L, -1) == Some("pixel")) { Font::Pixel } else { Font::SansBold };
        let mut d = TextDesc::new(font, fnum(L, t, c"size", 0.3)).with_color(frgba(L, t, c"color", [1.0; 4]));
        if with_field(L, t, c"outline", || lua_type(L, -1) != LUA_TNIL) {
            d = d.with_outline(frgba(L, t, c"outline", [0.0, 0.0, 0.0, 1.0]), fnum(L, t, c"width", 0.06));
        }
        for (k, bit) in [(c"billboard", TextDesc::BILLBOARD), (c"screen", TextDesc::SCREEN), (c"ontop", TextDesc::ON_TOP), (c"nofog", TextDesc::NO_FOG)] {
            if fflag(L, t, k) {
                d.flags |= bit;
            }
        }
        d
    }
}

// node(mesh, material), node(): something drawn, or an empty node.
b!(node, |L, h| {
    let w = w!(L, h);
    let mesh = match lua_type(L, 1) {
        LUA_TNIL | -1 => None,
        _ => match handle(L, 1, tag::GEOMETRY) {
            Some(m) => Some(m as usize - 1),
            None => return bad(L, 1, "node", "mesh (mesh.box(…) and the rest)"),
        },
    };
    let mat = match lua_type(L, 2) {
        LUA_TNIL | -1 => None,
        _ => match handle(L, 2, tag::MATERIAL) {
            Some(m) => Some(m as usize - 1),
            None => return bad(L, 2, "node", "material (material{ … })"),
        },
    };
    let made = match mesh {
        Some(m) => {
            let up = match w.uploaded(m) {
                Ok(u) => u,
                Err(e) => return err(L, &format!("node: {e}")),
            };
            let mat = match mat {
                Some(i) => w.materials[i].0,
                None => match w.default_material() {
                    Ok(m) => m,
                    Err(e) => return err(L, &format!("node: {e}")),
                },
            };
            gfx3d::Node::with_mesh(up, mat)
        }
        None => gfx3d::Node::new(),
    };
    let Some(n) = made else { return err(L, "too many nodes (4096 at most): reuse nodes, or draw many with n:instances") };
    match w.scene.add(Some(n), true, None) {
        Ok(id) => {
            push_node(L, w, id);
            1
        }
        Err(_) => {
            n.destroy();
            err(L, "too many nodes (4096 at most): reuse nodes, or draw many with n:instances")
        }
    }
});

// --------------------------------------------------------------- meshes

pub(crate) unsafe fn push_mesh(L: *mut lua_State, h: &mut Host, data: MeshData) -> c_int {
    unsafe {
        let w = w!(L, h);
        let i = w.add_mesh(data, false);
        push_handle(L, tag::GEOMETRY, i as u32 + 1)
    }
}

/// Up to four numbers from the arguments, with defaults.
pub(crate) unsafe fn args4(L: *mut lua_State, d: [f32; 4]) -> [f32; 4] {
    let mut out = d;
    for (i, o) in out.iter_mut().enumerate() {
        if let Some(v) = unsafe { num(L, i as c_int + 1) }.filter(|v| v.is_finite()) {
            *o = v as f32;
        }
    }
    out
}

b!(mk_box, |L, h| {
    let [x, y, z, _] = args4(L, [1.0, 1.0, 1.0, 0.0]);
    push_mesh(L, h, MeshData::cuboid(vec3(x, y, z)))
});
b!(mk_sphere, |L, h| {
    let [r, s, k, _] = args4(L, [0.5, 16.0, 10.0, 0.0]);
    push_mesh(L, h, MeshData::uv_sphere(r, s as u32, k as u32))
});
b!(mk_ico, |L, h| {
    let [r, n, _, _] = args4(L, [0.5, 1.0, 0.0, 0.0]);
    push_mesh(L, h, MeshData::icosphere(r, n as u32))
});
b!(mk_cylinder, |L, h| {
    let [r, hh, s, _] = args4(L, [0.5, 1.0, 16.0, 0.0]);
    push_mesh(L, h, MeshData::cylinder(r, hh, s as u32))
});
b!(mk_cone, |L, h| {
    let [r, hh, s, _] = args4(L, [0.5, 1.0, 16.0, 0.0]);
    push_mesh(L, h, MeshData::cone(r, hh, s as u32))
});
b!(mk_plane, |L, h| {
    let [x, z, _, _] = args4(L, [1.0, 1.0, 0.0, 0.0]);
    push_mesh(L, h, MeshData::plane(x, z))
});
b!(mk_torus, |L, h| {
    let [r, t, s, k] = args4(L, [0.5, 0.1, 16.0, 8.0]);
    push_mesh(L, h, MeshData::torus(r, t, s as u32, k as u32))
});
b!(mk_empty, |L, h| push_mesh(L, h, MeshData::new()));
// mesh.lathe({ r1, y1, r2, y2, … }, segs): a profile turned around y.
b!(mk_lathe, |L, h| {
    let p = match flat(L, 1, "mesh.lathe") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
    let pts: Vec<[f32; 2]> = p.as_chunks::<2>().0.to_vec();
    push_mesh(L, h, MeshData::lathe(&pts, opt(L, 2, 16.0) as u32))
});
// mesh.extrude({ x1, y1, … }, depth): a polygon pushed along z.
b!(mk_extrude, |L, h| {
    let p = match flat(L, 1, "mesh.extrude") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
    let depth = match arg(L, 2, "mesh.extrude") {
        Ok(d) => d,
        Err(r) => return r,
    };
    let pts: Vec<[f32; 2]> = p.as_chunks::<2>().0.to_vec();
    push_mesh(L, h, MeshData::extrude(&pts, depth))
});
// mesh.tris({ x, y, z, … }): triangles, 9 numbers each, with face normals.
b!(mk_tris, |L, h| {
    let p = match flat(L, 1, "mesh.tris") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
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
    push_mesh(L, h, m)
});

/// The mesh `self` names, to edit; each edit returns it, so they chain.
pub(crate) unsafe fn edit(L: *mut lua_State, h: &mut Host, f: &str, e: impl FnOnce(&mut MeshData, &World3) -> Result<(), String>) -> c_int {
    unsafe {
        let Some(i) = handle(L, 1, tag::GEOMETRY) else { return bad(L, 1, f, "mesh") };
        let w = w!(L, h);
        let mut data = std::mem::take(&mut w.meshes[i as usize - 1].data);
        let r = e(&mut data, w);
        w.meshes[i as usize - 1].data = data;
        if let Err(m) = r {
            return err(L, &format!("{f}: {m}"));
        }
        lua_pushvalue(L, 1);
        1
    }
}

b!(g_color, |L, h| {
    let c = num(L, 2).and_then(hex);
    edit(L, h, "color", |m, _| {
        let c = c.ok_or("takes a colour 0xRRGGBBAA")?;
        *m = std::mem::take(m).with_vertex_colors(c);
        Ok(())
    })
});
// split(y, below, above): two colours by height.
b!(g_split, |L, h| {
    let (y, below, above) = (num(L, 2), num(L, 3).and_then(hex), num(L, 4).and_then(hex));
    edit(L, h, "split", |m, _| {
        let (Some(y), Some(below), Some(above)) = (y, below, above) else { return Err("split(y, below, above): a height and two colours 0xRRGGBBAA".into()) };
        let y = y as f32;
        *m = std::mem::take(m).with_vertex_colors_by(|p, _| if p.y > y { above } else { below });
        Ok(())
    })
});

pub(crate) unsafe fn transform_mesh(L: *mut lua_State, h: &mut Host, f: &str, t: Transform) -> c_int {
    unsafe {
        edit(L, h, f, |m, _| {
            *m = m.transformed(&t.matrix());
            Ok(())
        })
    }
}

b!(g_move, |L, h| transform_mesh(L, h, "move", Transform::at(vec3(opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32))));
b!(g_turn, |L, h| transform_mesh(L, h, "turn", Transform::default().with_rot(rot(opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32))));
b!(g_scale, |L, h| {
    let x = opt(L, 2, 1.0);
    transform_mesh(L, h, "scale", Transform::default().with_scale3(vec3(x as f32, opt(L, 3, x) as f32, opt(L, 4, x) as f32)))
});
b!(g_clamp_y, |L, h| {
    let lo = num(L, 2);
    edit(L, h, "clamp_y", |m, _| {
        let lo = lo.ok_or("clamp_y(lo) takes a height")? as f32;
        for p in &mut m.positions {
            p.y = p.y.max(lo);
        }
        Ok(())
    })
});
// add(other): merges another mesh in (one mesh is one draw).
b!(g_add, |L, h| {
    let other = handle(L, 2, tag::GEOMETRY);
    let itself = other.is_some() && other == handle(L, 1, tag::GEOMETRY);
    edit(L, h, "add", |m, w| {
        let o = other.ok_or("add(other) takes a mesh")? as usize - 1;
        if itself {
            // The mesh itself (it's out for the edit): a copy of it doubled.
            let copy = m.clone();
            m.merge(&copy);
        } else {
            m.merge(&w.meshes[o].data);
        }
        Ok(())
    })
});
b!(g_flat, |L, h| edit(L, h, "flat", |m, _| {
    *m = m.flat_shaded();
    Ok(())
}));

// ------------------------------------------------------------ materials

/// `style = { toon, rim, outline, ink, nofog }`.
pub(crate) unsafe fn style_of(L: *mut lua_State, t: c_int) -> MaterialStyle {
    unsafe {
        MaterialStyle {
            outline_color: frgb(L, t, c"ink", [0.0; 3]),
            outline_width: fnum(L, t, c"outline", 0.0),
            toon_bands: fnum(L, t, c"toon", 0.0) as u32,
            rim: fnum(L, t, c"rim", 0.0),
            flags: if fflag(L, t, c"nofog") { MaterialStyle::NO_FOG } else { 0 },
            ..Default::default()
        }
    }
}

// material{ color, emissive, glow, metal, rough, unlit, double, alpha, additive, style = { … } }
b!(material, |L, h| {
    if !matches!(lua_type(L, 1), LUA_TTABLE | LUA_TNIL | -1) {
        return bad(L, 1, "material", "table");
    }
    let w = w!(L, h);
    let mut flags = 0;
    for (k, bit) in [(c"unlit", Material::UNLIT), (c"double", Material::DOUBLE_SIDED), (c"alpha", Material::ALPHA), (c"additive", Material::ADDITIVE)] {
        if fflag(L, 1, k) {
            flags |= bit;
        }
    }
    let d = MaterialDesc::default();
    let desc = MaterialDesc {
        base_color: frgba(L, 1, c"color", d.base_color),
        emissive: frgb(L, 1, c"emissive", d.emissive),
        emissive_strength: fnum(L, 1, c"glow", d.emissive_strength),
        metallic: fnum(L, 1, c"metal", d.metallic),
        roughness: fnum(L, 1, c"rough", d.roughness),
        flags,
        ..d
    };
    let Some(m) = Material::new(&desc) else { return err(L, "too many materials") };
    if lua_type(L, 1) == LUA_TTABLE && with_field(L, 1, c"style", || lua_type(L, -1) == LUA_TTABLE) {
        lua_rawgetfield(L, 1, c"style".as_ptr());
        m.set_style(&style_of(L, -1));
        lua_pop(L, 1);
    }
    w.materials.push((m, true));
    push_handle(L, tag::MATERIAL, w.materials.len() as u32)
});

b!(m_style, |L, h| {
    let Some(i) = handle(L, 1, tag::MATERIAL) else { return bad(L, 1, "style", "material") };
    if lua_type(L, 2) != LUA_TTABLE {
        return bad(L, 2, "style", "table { toon, rim, outline, ink, nofog }");
    }
    let style = style_of(L, 2);
    let w = w!(L, h);
    w.materials[i as usize - 1].0.set_style(&style);
    lua_pushvalue(L, 1);
    1
});

// ------------------------------------------------------- models, particles

// model("fox"): models/fox.glb, to spawn.
b!(model, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "model", "model name") };
    let key = name.strip_prefix("models/").unwrap_or(name).trim_end_matches(".glb").to_string();
    let Some(i) = h.project.models.iter().position(|m| *m == key) else {
        return err(L, &format!("model(\"{key}\"): there's no models/{key}.glb"));
    };
    let w = w!(L, h);
    if w.models.get(i).copied().flatten().is_none() {
        // Natively nothing loads (SPEC §9.8): every handle is 1.
        #[cfg(not(target_arch = "wasm32"))]
        if w.models.len() <= i {
            w.models.resize(i + 1, None);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            w.models[i] = gfx3d::Model::load(maimbrain::sys::Asset(1));
        }
        if w.models.get(i).copied().flatten().is_none() {
            return err(L, &format!("model(\"{key}\"): models/{key}.glb didn't load"));
        }
    }
    push_handle(L, tag::MODEL, i as u32 + 1)
});

// spawn(model, parent): a model's root node (its parts by n:find(name)).
b!(spawn, |L, h| {
    let Some(m) = handle(L, 1, tag::MODEL) else {
        if lua_type(L, 1) == LUA_TSTRING {
            return err(L, "spawn(kind) is the quick path's, which is 2D: in a 3D game spawn(model(\"fox\")) spawns a model");
        }
        return bad(L, 1, "spawn", "model (model(\"fox\"))");
    };
    let w = w!(L, h);
    let parent = match lua_type(L, 2) {
        LUA_TNIL | -1 => None,
        _ => match node_at(L, 2).filter(|p| w.scene.alive(*p)) {
            Some(p) => Some(p),
            None => return bad(L, 2, "spawn", "3D node"),
        },
    };
    let Some(model) = w.models.get(m as usize - 1).copied().flatten() else { return err(L, "spawn: that model didn't load") };
    let Some(root) = model.spawn(parent.and_then(|p| w.scene.get(p)).and_then(|p| p.host)) else { return err(L, "the model didn't spawn (too many nodes?)") };
    match w.scene.add(Some(root), true, parent) {
        Ok(id) => {
            w.anims.push(super::nodes3d::new_anim(id, root, model));
            push_node(L, w, id);
            1
        }
        Err(_) => {
            root.destroy();
            err(L, "too many nodes (4096 at most)")
        }
    }
});

// emitter{ colors = { 4 × rgba }, sizes = { 4 }, life = { a, b }, speed = { a, b }, spread, drag, gravity = { x, y, z }, stretch, alpha }
b!(emitter, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "emitter", "table");
    }
    let w = w!(L, h);
    let mut colors = [[1.0f32; 4]; 4];
    lua_rawgetfield(L, 1, c"colors".as_ptr());
    if lua_type(L, -1) == LUA_TTABLE {
        for (i, o) in colors.iter_mut().enumerate() {
            lua_rawgeti(L, -1, i as c_int + 1);
            *o = rgba_at(L, -1, [1.0; 4]);
            lua_pop(L, 1);
        }
    }
    lua_pop(L, 1);
    let base = EmitterDesc::default();
    let d = EmitterDesc {
        colors,
        sizes: fnums(L, 1, c"sizes", base.sizes),
        lifetime: fnums(L, 1, c"life", base.lifetime),
        speed: fnums(L, 1, c"speed", base.speed),
        spread: fnum(L, 1, c"spread", base.spread),
        drag: fnum(L, 1, c"drag", base.drag),
        blend: if fflag(L, 1, c"alpha") { FxBlend::Alpha } else { FxBlend::Additive },
        gravity: Vec3::from(fnums(L, 1, c"gravity", base.gravity.to_array())),
        stretch: fnum(L, 1, c"stretch", base.stretch),
        ..base
    };
    let Some(e) = Emitter::new(&d) else { return err(L, "too many emitters") };
    w.emitters.push(e);
    push_handle(L, tag::EMITTER3, w.emitters.len() as u32)
});

// e:emit(x, y, z, dx, dy, dz, count[, vx, vy, vz]): a burst; with a velocity, in a frame moving that fast.
b!(e_emit, |L, h| {
    let Some(i) = handle(L, 1, tag::EMITTER3) else { return bad(L, 1, "emit", "emitter") };
    let w = w!(L, h);
    let g = |k: c_int| opt(L, k, 0.0) as f32;
    let e = w.emitters[i as usize - 1];
    let (p, d, n) = (vec3(g(2), g(3), g(4)), vec3(g(5), g(6), g(7)), opt(L, 8, 1.0).clamp(0.0, 4096.0) as u32);
    if num(L, 9).is_some() {
        e.emit_moving(p, d, n, vec3(g(9), g(10), g(11)));
    } else {
        e.emit(p, d, n);
    }
    0
});

// ------------------------------------------------------ the camera, the look

/// `camera(x, y, z, look_x, look_y, look_z, fov, near, far)`: the camera's
/// `__call` (the 2D camera's properties and methods are as in a 2D game).
/// `fov` is 1.0's, in radians.
pub unsafe fn camera_call(L: *mut lua_State, h: &mut Host) -> c_int {
    unsafe {
        let w = w!(L, h);
        let g = |i: c_int, d: f64| opt(L, i, d) as f32;
        let pos = vec3(g(2, 0.0), g(3, 1.0), g(4, 5.0));
        let target = vec3(g(5, 0.0), g(6, 0.0), g(7, 0.0));
        w.set_camera(Camera { pos, rot: look_at(pos, target, Vec3::Y), fov_y: g(8, 0.9), near: g(9, 0.1), far: g(10, 200.0) });
        0
    }
}

// project(x, y, z) → sx, sy, onScreen: where a point is on the canvas, by the camera (in Rust, as mb3d does it).
b!(project, |L, h| {
    let (x, y, z) = (opt(L, 1, 0.0) as f32, opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    let [sw, sh, ox, oy, s] = h.screen;
    let w = w!(L, h);
    let p = w.camera().project(vec3(x, y, z), sw, sh);
    lua_pushnumber(L, ((p.x - ox) / s) as f64);
    lua_pushnumber(L, ((p.y - oy) / s) as f64);
    lua_pushboolean(L, p.on_screen as c_int);
    3
});

b!(sky, |L, h| {
    let d = Sky::default();
    let mut colors = d.colors;
    lua_rawgetfield(L, 1, c"colors".as_ptr());
    if lua_type(L, -1) == LUA_TTABLE {
        for (i, o) in colors.iter_mut().enumerate() {
            lua_rawgeti(L, -1, i as c_int + 1);
            *o = rgb_at(L, -1, [0.0; 3]);
            lua_pop(L, 1);
        }
    }
    lua_pop(L, 1);
    gfx3d::sky(&Sky {
        seed: fnum(L, 1, c"seed", d.seed as f32) as u32,
        colors,
        nebula_density: fnum(L, 1, c"nebula", d.nebula_density),
        star_density: fnum(L, 1, c"stars", d.star_density),
        star_brightness: fnum(L, 1, c"star_brightness", d.star_brightness),
        sun_disc: fnum(L, 1, c"sun_disc", d.sun_disc),
        ..d
    });
    0
});

b!(sun, |L, h| {
    if let Some(w) = h.three.as_deref_mut() {
        w.sun_set = true;
    }
    let d = Sun::default();
    gfx3d::sun(&Sun {
        direction: Vec3::from(fnums(L, 1, c"dir", d.direction.to_array())),
        color: frgb(L, 1, c"color", d.color),
        intensity: fnum(L, 1, c"intensity", d.intensity),
        shadows: fflag(L, 1, c"shadows") as u32,
        shadow_distance: fnum(L, 1, c"shadow_distance", d.shadow_distance),
    });
    0
});

b!(post, |L, h| {
    let d = Post::default();
    gfx3d::post(&Post {
        exposure: fnum(L, 1, c"exposure", d.exposure),
        bloom_strength: fnum(L, 1, c"bloom", d.bloom_strength),
        bloom_threshold: fnum(L, 1, c"bloom_threshold", d.bloom_threshold),
        vignette: fnum(L, 1, c"vignette", d.vignette),
        chromatic_aberration: fnum(L, 1, c"aberration", d.chromatic_aberration),
        saturation: fnum(L, 1, c"saturation", d.saturation),
        contrast: fnum(L, 1, c"contrast", d.contrast),
        sharpen: fnum(L, 1, c"sharpen", d.sharpen),
    });
    0
});

b!(fog, |L, h| {
    let mode = match with_field(L, 1, c"mode", || st(L, -1).map(str::to_string)).as_deref() {
        Some("linear") => FogMode::Linear,
        Some("exp2") => FogMode::ExponentialSquared,
        Some("off") => FogMode::Off,
        Some("exp") | None => FogMode::Exponential,
        Some(o) => return err(L, &format!("fog: mode = \"{o}\" (\"exp\", \"exp2\", \"linear\" or \"off\")")),
    };
    let d = Fog::default();
    // The game's fog: a terrain's own gives way to it.
    if let Some(w) = h.three.as_deref_mut() {
        w.fog_set = true;
    }
    gfx3d::fog(&Fog {
        color: frgb(L, 1, c"color", d.color),
        mode,
        start: fnum(L, 1, c"start", d.start),
        end: fnum(L, 1, c"end", d.end),
        density: fnum(L, 1, c"density", d.density),
        height: fnum(L, 1, c"height", d.height),
        height_falloff: fnum(L, 1, c"falloff", d.height_falloff),
        sky: fnum(L, 1, c"sky", d.sky),
        ..d
    });
    0
});

// dof{ focus, range, blur, near }, or dof() for none.
b!(dof, |L, h| {
    let d = if lua_type(L, 1) == LUA_TTABLE {
        DepthOfField { focus: fnum(L, 1, c"focus", 5.0), range: fnum(L, 1, c"range", 1.0), blur: fnum(L, 1, c"blur", 4.0), near: fnum(L, 1, c"near", 0.4) }
    } else {
        DepthOfField { focus: 0.0, range: 0.0, blur: 0.0, near: 0.0 }
    };
    gfx3d::depth_of_field(&d);
    0
});

// The beauty pass, by the SDK's presets.
b!(grade, |L, h| {
    let mut g = match st(L, 1) {
        Some("moonlit") => Grade::moonlit(),
        _ => Grade::default(),
    };
    g.amount = fnum(L, 2, c"amount", g.amount);
    gfx3d::grade(&g);
    0
});
b!(shafts, |L, h| {
    gfx3d::shafts(&if st(L, 1) == Some("moonbeams") { LightShafts::moonbeams() } else { LightShafts::default() });
    0
});
b!(halos, |L, h| {
    let mut x = if st(L, 1) == Some("mist") { Halos::mist() } else { Halos::default() };
    x.reach = fnum(L, 2, c"reach", x.reach);
    gfx3d::halos(&x);
    0
});
b!(film, |L, h| {
    let mut f = if st(L, 1) == Some("subtle") { Film::subtle() } else { Film::default() };
    f.streaks = fnum(L, 2, c"streaks", f.streaks);
    f.grain = fnum(L, 2, c"grain", f.grain);
    gfx3d::film(&f);
    0
});

// --------------------------------------------------------- the 2D over it

// text(s, x, y, { size, color = 0xRRGGBBAA, align, shadow }): drawn this frame over the scene, in canvas units.
b!(hud_text, |L, h| {
    let s = to_s(L, 1);
    let (x, y) = match (arg(L, 2, "text"), arg(L, 3, "text")) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    let size = fnum(L, 4, c"size", 16.0).clamp(1.0, 512.0);
    let color = with_field(L, 4, c"color", || num(L, -1).and_then(hex)).unwrap_or(0xffffffff);
    let align = match with_field(L, 4, c"align", || st(L, -1).map(str::to_string)).as_deref() {
        Some("left") => Align::Left,
        Some("right") => Align::Right,
        _ => Align::Center,
    };
    let shadow = !with_field(L, 4, c"shadow", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
    if shadow {
        h.draw.push(Cmd::Text { s: s.clone(), font: Font::SansBold, size, x: x + 1.5, y: y + 1.5, color: 0x00000090, wrap: 0.0, align });
    }
    h.draw.push(Cmd::Text { s, font: Font::SansBold, size, x, y, color, wrap: 0.0, align });
    0
});

// rect(x, y, w, h, color = 0xRRGGBBAA)
b!(hud_rect, |L, h| {
    let mut v = [0.0f32; 4];
    for (i, o) in v.iter_mut().enumerate() {
        match arg(L, i as c_int + 1, "rect") {
            Ok(x) => *o = x,
            Err(r) => return r,
        }
    }
    let Some(color) = num(L, 5).and_then(hex) else { return bad(L, 5, "rect", "colour 0xRRGGBBAA") };
    h.draw.push(Cmd::Rect { x: v[0], y: v[1], w: v[2], h: v[3], r: 0.0, line: 0.0, color });
    0
});

// ------------------------------------------------------------------ sound

/// A project sound by name, decoded (or not yet).
pub(crate) unsafe fn sound_named(L: *mut lua_State, h: &Host, f: &str) -> Result<Option<maimbrain::audio::Sound>, c_int> {
    unsafe {
        let Some(name) = st(L, 1) else { return Err(bad(L, 1, f, "sound name")) };
        match h.project.sound(name) {
            Some(i) => Ok(h.sounds.get(i).copied().flatten()),
            None => {
                let key = name.strip_prefix("sounds/").unwrap_or(name).trim_end_matches(".ogg");
                Err(err(L, &format!("no sound \"{key}\": add sounds/{key}.sfx.toml or sounds/{key}.ogg")))
            }
        }
    }
}

// sfx(name, vol, pan, pitch): once. Silent in the title's attract mode, as all 2.0 sound is.
b!(sfx, |L, h| {
    let snd = match sound_named(L, h, "sfx") {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Some(s) = snd.filter(|_| !h.mute) {
        s.play(opt(L, 2, 1.0) as f32 * h.volume, opt(L, 3, 0.0) as f32, opt(L, 4, 1.0) as f32);
    }
    0
});

// loop(name, vol, pan, pitch): a sound that repeats until :stop(), for this round.
b!(sound_loop, |L, h| {
    let snd = match sound_named(L, h, "loop") {
        Ok(s) => s,
        Err(r) => return r,
    };
    let mute = h.mute;
    let volume = h.volume;
    let w = w!(L, h);
    let v = snd.filter(|_| !mute).map(|s| s.play_looped(opt(L, 2, 1.0) as f32 * volume, opt(L, 3, 0.0) as f32, opt(L, 4, 1.0) as f32));
    w.voices.push(v);
    push_handle(L, tag::VOICE, w.voices.len() as u32)
});

b!(v_set, |L, h| {
    let Some(i) = handle(L, 1, tag::VOICE) else { return bad(L, 1, "set", "loop") };
    let w = w!(L, h);
    if let Some(v) = w.voices[i as usize - 1] {
        v.set(opt(L, 2, 1.0) as f32, opt(L, 3, 0.0) as f32, opt(L, 4, 1.0) as f32);
    }
    0
});

b!(v_stop, |L, h| {
    let Some(i) = handle(L, 1, tag::VOICE) else { return bad(L, 1, "stop", "loop") };
    let w = w!(L, h);
    if let Some(v) = w.voices[i as usize - 1].take() {
        v.stop_at(maimbrain::sys::time(), opt(L, 2, 0.1) as f32);
    }
    0
});

// lowpass("music" | "sfx" | "master", hz, seconds): muffle a bus, gliding there; 20000 opens it.
b!(lowpass, |L, h| {
    let b = match st(L, 1) {
        Some("music") => Bus::Music,
        Some("sfx") => Bus::Sfx,
        Some("master") => Bus::Master,
        o => return err(L, &format!("lowpass(\"{}\"): \"music\", \"sfx\" or \"master\"", o.unwrap_or("?"))),
    };
    let hz = match arg(L, 2, "lowpass") {
        Ok(x) => x,
        Err(r) => return r,
    };
    b.lowpass(hz, 0.707, opt(L, 3, 0.2) as f32);
    0
});
