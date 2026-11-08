//! Engine 2.0's own 3D nodes (docs/STUDIO-2.md §5 [3d], [godot], [models];
//! §9.3 row 2a): three.js's building blocks (`Mesh`, `Group`, `Light`, and
//! `Emitter`, `Trail` and a billboard `Text`), `Model` with its clips,
//! layers, rig, IK, gaits, flight and swim, Godot's `AnimationTree`, the 3D
//! camera's properties and its follow, and `fx` in 3D. They are the same
//! userdata as 1.0's 3D nodes (api/three.rs), whose metamethods hand the
//! names here; 1.0's methods work on every node.
//!
//! Colours here are 2.0's (§5 conventions): `0xRRGGBB` or `{ r, g, b }`
//! from 0 to 1, as picked (sRGB); they read back as `0xRRGGBB`.

use std::ffi::{CStr, c_int, c_void};

use maimbrain::gfx3d::{
    self, AnimTree, EmitterDesc, FxBlend, Humanoid, LookAt, Mask, MaterialStyle, MechGait, Motion, Quat, Skeleton, Swim, TextDesc, Trail, TrailDesc, Transition, TwoBoneIk, Vec3, attach_offset,
    look_at, srgba, vec3,
};
use maimbrain::scene::Preset;
use maimbrain::scene::three::Node3Id;

use super::atoms::A;
use super::three::{abs, fflag, flat64, fnum, key_at, node_at, push_node, set_any, with_field};
use super::*;
use crate::three::{Anim, Emit3, Follow3, Info, Kind3, Layer, Popup3, Rig, Text3, TreeInst, World3, heading_of};

/// The constructors only a 3D game has. `Group`, `Emitter` and `Text` are
/// node.rs's, which make a 3D node here in a 3D game ([`wants_3d`]).
pub const KINDS3: &[(&CStr, Kind3)] = &[(c"Mesh", Kind3::Mesh), (c"Model", Kind3::Model), (c"Light", Kind3::Light), (c"Trail", Kind3::Trail)];

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        let make: [RustFn; 4] = [mk_mesh, mk_model, mk_light, mk_trail];
        for ((name, _), f) in KINDS3.iter().zip(make) {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        sv_push_rfn(L, mk_tree, ud, c"AnimationTree".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"AnimationTree".as_ptr());
        register_kind(L, ud, tag::ANIMTREE, c"AnimationTree", &[(c"__index", t_index), (c"__namecall", t_namecall)]);
    }
}

// ------------------------------------------------------------- what's where

/// Properties and methods every 3D node has beyond 1.0's (api/three.rs's `is_prop`).
pub fn is_shared(a: A) -> bool {
    matches!(a, A::Name | A::Tag | A::Fade | A::Wake)
}

/// What a Terrain is beyond `name`, `tag`, `visible`, `data`, `alive` and
/// `destroy`: it stays where it is (its ground is the world's).
pub fn terrain_allows(a: A) -> bool {
    matches!(a, A::Name | A::Tag | A::Visible | A::Data | A::Alive | A::Destroy | A::Progress | A::Built | A::HeightAt | A::NormalAt | A::Raise)
}

/// A kind's names, beyond 1.0's (2.0's shared ones included).
pub fn has(kind: Kind3, a: A) -> bool {
    use A::*;
    use Kind3 as K;
    if super::bodies3d::is_physics(kind) && super::bodies3d::has(kind, a) {
        return true;
    }
    match a {
        Name | Tag | Destroy => true,
        Fade | Wake | FaceDirection | LookAt | WorldPosition | Instance | Find => kind != K::Terrain,
        Progress | Built | HeightAt | NormalAt | Raise => kind == K::Terrain,
        Geometry | Material => kind == K::Mesh,
        Model | Bone | Reach | HeadLook | PlantFeet | Gait | Flight | Swim | Restyle | ClipTime | Clip | Play | AnimationFinished | Stepped => kind == K::Model,
        Color => matches!(kind, K::Light | K::Text | K::Trail),
        Intensity | Range => kind == K::Light,
        Preset | Rate | Colors | Sizes | Life | Speed | Spread | Drag | Gravity | Additive | Emitting | Emit => kind == K::Emitter,
        Width | Length => kind == K::Trail,
        Text | Size | Billboard | Font => kind == K::Text,
        Footsteps => kind == K::Model,
        Hum => kind != K::Terrain,
        _ => false,
    }
}

/// 2.0's 3D methods.
pub fn is_method(a: A) -> bool {
    super::bodies3d::is_method(a)
        || matches!(
        a,
        A::FaceDirection | A::LookAt | A::WorldPosition | A::Instance | A::Destroy | A::Find | A::Bone | A::Reach | A::HeadLook | A::Restyle | A::Play | A::Emit | A::HeightAt | A::NormalAt | A::Raise
    )
}

pub fn is_method3(kind: Kind3, a: A) -> bool {
    has(kind, a) && is_method(a)
}

pub fn is_signal(a: A) -> bool {
    matches!(a, A::AnimationFinished | A::Stepped | A::Built) || super::bodies3d::is_signal(a)
}

/// The names a kind has, 1.0's included, for "did you mean".
fn names(kind: Kind3, method: bool) -> Vec<&'static str> {
    super::atoms::NAMES
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let a = A::from(i as i32)?;
            let m = super::three::is_method(a) || is_method3(kind, a);
            let p = (super::three::is_prop(a) || has(kind, a)) && !is_method(a);
            ((method && m) || (!method && p)).then_some(*n)
        })
        .collect()
}

/// "no property `colour` on Light; did you mean `color`?"
pub fn unknown(kind: Kind3, key: &str, method: bool) -> String {
    let what = if method { "method" } else { "property" };
    match key {
        "rotation" | "yaw" if !method => return "a 3D node turns with ry (yaw), rx (pitch) and rz (roll), or node:faceDirection(dx, dz); `rotation` is 2D's".into(),
        "vr" if !method => return "a 3D node spins with vry (yaw), vrx (pitch) and vrz (roll), radians a second; `vr` is 2D's".into(),
        "position" if !method => return "a 3D node's place is x, y and z (numbers, not a vector)".into(),
        _ => {}
    }
    let list = names(kind, method);
    let near = list
        .iter()
        .map(|n| (if n.eq_ignore_ascii_case(key) { 0 } else { super::node::distance(&key.to_ascii_lowercase(), &n.to_ascii_lowercase()) }, *n))
        .filter(|(d, _)| *d <= 1 || (*d == 2 && key.len() >= 6))
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n);
    match (near, method) {
        (Some(n), _) => format!("no {what} `{key}` on {}; did you mean `{n}`?", kind.name()),
        (None, true) => format!("no method `{key}` on {}", kind.name()),
        (None, false) => format!("no property `{key}` on {}: a game's own values go in `node.data` (node.data.{key} = …)", kind.name()),
    }
}

// ---------------------------------------------------------------- values

/// A 2.0 colour at `i` as `0xRRGGBBAA` (§5 conventions).
pub(crate) unsafe fn col(L: *mut lua_State, i: c_int) -> Result<u32, String> {
    unsafe { color_at(L, i, false) }
}

/// An f32 as the number it was written as (0.4, not 0.4000000059604645).
pub(crate) fn nice(x: f32) -> f64 {
    format!("{x}").parse().unwrap_or(x as f64)
}

/// A `0xRRGGBBAA` colour in linear light, for mb3d.
pub(crate) fn lin(c: u32) -> [f32; 4] {
    srgba(c)
}

unsafe fn need(L: *mut lua_State, i: c_int, f: &str) -> Result<f32, String> {
    unsafe {
        match num(L, i) {
            Some(v) if v.is_finite() => Ok(v as f32),
            _ => Err(format!("bad argument #{} to '{f}' (number expected, got {})", i - 1, type_name(L, i))),
        }
    }
}

unsafe fn number_at(L: *mut lua_State, v: c_int, key: &str) -> Result<f32, String> {
    unsafe {
        match num(L, v) {
            Some(x) if x.is_finite() => Ok(x as f32),
            _ => Err(format!("{key} takes a number, got {}", type_name(L, v))),
        }
    }
}

unsafe fn bool_at(L: *mut lua_State, v: c_int, key: &str) -> Result<bool, String> {
    unsafe {
        match lua_type(L, v) {
            LUA_TBOOLEAN => Ok(lua_toboolean(L, v) != 0),
            LUA_TNIL => Ok(false),
            _ => Err(format!("{key} takes true or false, got {}", type_name(L, v))),
        }
    }
}

/// `{ a, b }` or one number (both the same).
unsafe fn range_at(L: *mut lua_State, v: c_int, key: &str) -> Result<[f32; 2], String> {
    unsafe {
        match lua_type(L, v) {
            LUA_TNUMBER => {
                let x = lua_tonumber(L, v) as f32;
                Ok([x, x])
            }
            LUA_TTABLE => {
                let l = flat64(L, v, key)?;
                match l.as_slice() {
                    [a] => Ok([*a as f32, *a as f32]),
                    [a, b, ..] => Ok([*a as f32, *b as f32]),
                    [] => Err(format!("{key} takes {{ min, max }}")),
                }
            }
            _ => Err(format!("{key} takes a number or {{ min, max }}, got {}", type_name(L, v))),
        }
    }
}

/// One colour or a list of them, as `0xRRGGBBAA`.
unsafe fn colors_at(L: *mut lua_State, v: c_int) -> Result<Vec<u32>, String> {
    unsafe {
        let v = abs(L, v);
        if lua_type(L, v) == LUA_TTABLE {
            lua_rawgeti(L, v, 1);
            let list = lua_type(L, -1) == LUA_TTABLE || (lua_type(L, -1) == LUA_TNUMBER && lua_tonumber(L, -1) > 1.0);
            lua_pop(L, 1);
            if list {
                let mut out = Vec::new();
                for k in 1..=lua_objlen(L, v).min(4) {
                    lua_rawgeti(L, v, k);
                    let c = col(L, -1);
                    lua_pop(L, 1);
                    out.push(c?);
                }
                return Ok(out);
            }
        }
        Ok(vec![col(L, v)?])
    }
}

/// The 3D world, or the error a 2D game gets.
macro_rules! w3 {
    ($h:expr) => {
        match $h.three.as_deref_mut() {
            Some(w) => w,
            None => return Err(super::three::NOT_3D.into()),
        }
    };
}

// ------------------------------------------------------------- the look of things

/// A preset's particles in 3D (meters, linear light), tinted by `colors`.
pub fn preset_desc(p: Preset, colors: &[u32]) -> EmitterDesc {
    let warm = [0xffd27aff, 0xffb347ff];
    let (c0, blend, sizes, life, speed, spread, drag, gravity, stretch, glow) = match p {
        Preset::Sparkles => (warm[0], FxBlend::Additive, [0.09, 0.07, 0.05, 0.0], [0.5, 1.0], [1.0, 3.0], std::f32::consts::PI, 2.0, -1.0, 0.0, 2.0),
        Preset::Confetti => (0xff5c8aff, FxBlend::Alpha, [0.09, 0.09, 0.08, 0.06], [1.2, 2.0], [3.0, 6.0], 0.6, 1.5, -6.0, 0.0, 1.0),
        Preset::Stars => (0xfff6c8ff, FxBlend::Additive, [0.14, 0.12, 0.08, 0.0], [0.6, 1.2], [1.0, 2.5], std::f32::consts::PI, 1.5, -0.5, 0.0, 2.5),
        Preset::Burst => (0xff9a3cff, FxBlend::Additive, [0.18, 0.14, 0.08, 0.0], [0.3, 0.7], [2.0, 6.0], std::f32::consts::PI, 2.5, -2.0, 0.0, 2.0),
        Preset::Sparks => (0xffc060ff, FxBlend::Additive, [0.04, 0.035, 0.025, 0.0], [0.2, 0.5], [4.0, 9.0], 1.2, 1.0, -9.0, 0.04, 3.0),
        Preset::Ring => (0xbfe8ffff, FxBlend::Additive, [0.1, 0.09, 0.06, 0.0], [0.35, 0.45], [3.0, 3.4], std::f32::consts::FRAC_PI_2, 3.0, 0.0, 0.0, 2.0),
        Preset::Dust => (0x8c7b66ff, FxBlend::Alpha, [0.3, 0.6, 0.9, 1.0], [0.6, 1.2], [0.5, 1.5], 1.3, 3.0, 0.3, 0.0, 1.0),
        Preset::Smoke => (0x6b6b70ff, FxBlend::Alpha, [0.4, 0.9, 1.3, 1.6], [1.2, 2.4], [0.4, 1.0], 0.8, 1.0, 0.6, 0.0, 1.0),
    };
    let pick = |k: usize| {
        if colors.is_empty() { c0 } else { colors[(k * colors.len() / 4).min(colors.len() - 1)] }
    };
    let mut cs = [[0.0f32; 4]; 4];
    for (k, c) in cs.iter_mut().enumerate() {
        let l = lin(pick(k));
        let a = [1.0, 0.85, 0.5, 0.0][k] * l[3];
        *c = [l[0] * glow, l[1] * glow, l[2] * glow, a];
    }
    EmitterDesc { colors: cs, sizes, lifetime: life, speed, spread, drag, blend, texture: None, gravity: vec3(0.0, gravity, 0.0), stretch }
}

fn trail_desc(color: u32, width: f32, length: f32) -> TrailDesc {
    let l = lin(color);
    TrailDesc { color_start: [l[0] * 1.5, l[1] * 1.5, l[2] * 1.5, 1.0], color_end: [l[0], l[1], l[2], 0.0], width_start: width, width_end: 0.0, lifetime: length.max(0.05), ..Default::default() }
}

fn text_desc3(t: &Text3) -> TextDesc {
    let mut d = TextDesc::new(t.font, t.size).with_color(lin(t.color));
    if t.billboard {
        d.flags |= TextDesc::BILLBOARD;
    }
    d
}

/// 3D text sent while its font was loading, sent again in it once it's in.
pub fn fonts_arrived(w: &mut World3) {
    if w.fonts_pending.is_empty() {
        return;
    }
    for id in std::mem::take(&mut w.fonts_pending) {
        match w.info(id).and_then(|i| i.text.as_ref()).map(|t| t.font.ready()) {
            Some(true) => apply(w, id),
            Some(false) if w.scene.alive(id) => w.fonts_pending.push(id),
            _ => {}
        }
    }
}

/// Puts a node's light, text or trail in mb3d as its info says.
fn apply(w: &mut World3, id: Node3Id) {
    let Some(host) = w.scene.get(id).and_then(|n| n.host) else {
        return;
    };
    let Some(i) = w.info.get_mut(&id.to_bits()) else {
        return;
    };
    match i.kind {
        Kind3::Light => {
            let l = lin(i.color);
            host.point_light([l[0], l[1], l[2]], i.intensity, i.range);
        }
        Kind3::Text => {
            if let Some(t) = &i.text {
                if t.text.is_empty() {
                    host.clear_text();
                } else {
                    host.set_text(&text_desc3(t), &t.text);
                    // Sent in its fallback (a library font still loading:
                    // the title card's first frames): sent again once it's in.
                    if !t.font.ready() && !w.fonts_pending.contains(&id) {
                        w.fonts_pending.push(id);
                    }
                }
            }
        }
        Kind3::Trail => {
            if let Some((t, d)) = &mut i.trail {
                let nd = trail_desc(i.color, i.range, i.intensity);
                if nd != *d {
                    t.detach();
                    t.free();
                    if let Some(n) = Trail::new(&nd) {
                        n.attach(host);
                        *t = n;
                    }
                    *d = nd;
                }
            }
        }
        _ => {}
    }
}

/// Makes the emitter again after its look changed.
fn remake_emitter(i: &mut Info) {
    if let Some(e) = &mut i.emit
        && let Some(n) = gfx3d::Emitter::new(&e.desc)
    {
        e.emitter.free();
        e.emitter = n;
    }
}

// ------------------------------------------------------------------- rigs

pub fn new_anim(node: Node3Id, root: gfx3d::Node, model: gfx3d::Model) -> Anim {
    Anim { node, root, model, animator: gfx3d::Animator::new(), clips: Vec::new(), layers: Vec::new(), tree: None, told: false, rig: Rig::default() }
}

fn anim_index(w: &World3, id: Node3Id) -> Option<usize> {
    w.anims.iter().position(|a| a.node == id)
}

/// The model's humanoid rig, found the first time it's needed.
fn humanoid(w: &mut World3, ai: usize) -> Option<&Humanoid> {
    let a = &mut w.anims[ai];
    if a.rig.humanoid.is_none() {
        a.rig.humanoid = Some(Humanoid::find(a.model, a.root));
    }
    a.rig.humanoid.as_ref().unwrap().as_ref()
}

fn spine(w: &mut World3, ai: usize) -> bool {
    let a = &mut w.anims[ai];
    if a.rig.spine.is_none() {
        a.rig.spine = Some(Skeleton::spine(a.root));
    }
    matches!(a.rig.spine, Some(Some(_)))
}

/// The legs for planted feet and gaits: two-bone IK from thigh to foot,
/// feet flat, and how high each ankle is at rest.
fn legs(w: &mut World3, ai: usize) -> bool {
    if w.anims[ai].rig.legs.is_some() {
        return true;
    }
    let Some(rig) = humanoid(w, ai) else {
        return false;
    };
    let (Some(mut l), Some(mut r)) = (TwoBoneIk::leg(rig, 'l'), TwoBoneIk::leg(rig, 'r')) else {
        return false;
    };
    (l.flat, r.flat) = (true, true);
    let ankle = [rig.rest_point("foot_l").map_or(0.08, |p| p.y), rig.rest_point("foot_r").map_or(0.08, |p| p.y)];
    let a = &mut w.anims[ai];
    a.rig.legs = Some([l, r]);
    a.rig.ankle = ankle;
    true
}

fn warn(msg: &str) {
    maimbrain::sys::log(maimbrain::sys::Level::Warn, msg);
}

/// A bone by a standard name (`hand_r`) or the model's own; nil if neither.
fn bone_node(w: &mut World3, ai: usize, name: &str) -> Option<gfx3d::Node> {
    if let Some(n) = humanoid(w, ai).and_then(|r| r.node(name)) {
        return Some(n);
    }
    w.anims[ai].root.find(name)
}

/// A part of a model (by `find` or `bone`) as a node: the same one each time.
unsafe fn push_part(L: *mut lua_State, w: &mut World3, id: Node3Id, name: &str, part: Option<gfx3d::Node>) -> Result<c_int, String> {
    unsafe {
        if let Some((_, _, f)) = w.found.iter().find(|f| f.0 == id && f.1 == name).filter(|f| w.scene.alive(f.2)) {
            push_node(L, w, *f);
            return Ok(1);
        }
        match part {
            Some(found) => {
                let f = w.scene.add(Some(found), false, Some(id)).map_err(|_| "too many nodes (4096 at most)".to_string())?;
                w.found.push((id, name.to_string(), f));
                push_node(L, w, f);
            }
            None => lua_pushnil(L),
        }
        Ok(1)
    }
}

/// A clip of the model by name, found once.
fn clip_of(w: &mut World3, ai: usize, name: &str, strict: bool) -> Result<gfx3d::Clip, String> {
    let a = &mut w.anims[ai];
    if let Some(c) = a.clips.iter().find(|c| c.0 == name) {
        return Ok(c.1);
    }
    let c = match a.model.clip(name) {
        Some(c) => c,
        None if strict => {
            return Err(format!("the model has no clip \"{name}\" (generated models' clips are capitalised: \"Idle\", \"Run\")"));
        }
        None => gfx3d::Clip { index: 0, duration: 1.0 },
    };
    a.clips.push((name.to_string(), c));
    Ok(c)
}

fn clip_name(a: &Anim, c: gfx3d::Clip) -> Option<String> {
    a.clips.iter().find(|x| x.1 == c).map(|x| x.0.clone())
}

/// A mask of the bones from `names`' bones down.
unsafe fn mask_at(L: *mut lua_State, w: &mut World3, ai: usize, v: c_int) -> Result<Mask, String> {
    unsafe {
        let mut list = Vec::new();
        match lua_type(L, v) {
            LUA_TSTRING => list.push(st(L, v).unwrap_or_default().to_string()),
            LUA_TTABLE => {
                for k in 1..=lua_objlen(L, v) {
                    lua_rawgeti(L, v, k);
                    if let Some(s) = st(L, -1) {
                        list.push(s.to_string());
                    }
                    lua_pop(L, 1);
                }
            }
            _ => return Err("play: mask is a bone's name (\"spine\") or a list of them".into()),
        }
        let mut m = Mask::new();
        for name in list {
            let Some(b) = bone_node(w, ai, &name) else {
                return Err(format!("play: the model has no bone \"{name}\" for the mask (the standard names: hips, spine, chest, neck, head, upper_arm_l, …)"));
            };
            let a = &w.anims[ai];
            for &(n, k) in Mask::below(a.model, a.root, b, 1.0).entries() {
                m.set(n, k.max(m.weight(n)));
            }
        }
        Ok(m)
    }
}

/// `play(clip, { once, fade, restart, speed, layer, mask })`; `play(nil, { layer })` fades a layer out.
pub unsafe fn play(L: *mut lua_State, w: &mut World3, id: Node3Id) -> Result<c_int, String> {
    unsafe {
        let strict = w.kind(id) == Kind3::Model;
        let Some(ai) = anim_index(w, id) else {
            return Err("play: not a model (Model{ model = \"fox\" }, or 1.0's spawn(model(\"fox\")))".into());
        };
        let layer = with_field(L, 3, c"layer", || st(L, -1).map(str::to_string));
        let (once, restart) = (fflag(L, 3, c"once"), fflag(L, 3, c"restart"));
        let fade = fnum(L, 3, c"fade", 0.15).max(0.0);
        let speed = with_field(L, 3, c"speed", || num(L, -1).filter(|x| x.is_finite()));
        let mode = if once { gfx3d::Play::Once } else { gfx3d::Play::Loop };
        let rate = if fade > 1e-4 { 1.0 / fade } else { f32::INFINITY };
        let Some(name) = st(L, 2).map(str::to_string) else {
            if let Some(ln) = layer {
                if let Some(l) = w.anims[ai].layers.iter_mut().find(|l| l.name == ln) {
                    (l.target, l.rate) = (0.0, rate);
                }
                return Ok(0);
            }
            return Err("play(clip): the clip's name (play(nil, { layer = \"…\" }) stops a layer)".into());
        };
        let c = clip_of(w, ai, &name, strict)?;
        if let Some(ln) = layer {
            if !w.anims[ai].layers.iter().any(|l| l.name == ln) {
                let m = with_field(L, 3, c"mask", || lua_type(L, -1) != LUA_TNIL);
                if !m {
                    return Err(format!("play: a new layer needs a mask, the bone it starts at: {{ layer = \"{ln}\", mask = \"spine\" }}"));
                }
                lua_rawgetfield(L, 3, c"mask".as_ptr());
                let mask = mask_at(L, w, ai, lua_gettop(L));
                lua_pop(L, 1);
                w.anims[ai].layers.push(Layer { name: ln.clone(), animator: gfx3d::Animator::new(), mask: mask?, weight: 0.0, target: 1.0, rate });
            }
            let l = w.anims[ai].layers.iter_mut().find(|l| l.name == ln).unwrap();
            (l.target, l.rate) = (1.0, rate);
            if restart {
                l.animator.restart(c, mode, 0.0)
            } else {
                l.animator.play(c, mode, 0.0)
            }
            if let Some(s) = speed {
                l.animator.set_speed(s as f32);
            }
            return Ok(0);
        }
        let a = &mut w.anims[ai];
        if restart {
            a.animator.restart(c, mode, fade)
        } else {
            a.animator.play(c, mode, fade)
        }
        if let Some(s) = speed {
            a.animator.set_speed(s as f32);
        }
        Ok(0)
    }
}

// ------------------------------------------------------------- properties

pub unsafe fn get(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A) -> c_int {
    unsafe {
        if is_signal(a) {
            return super::node::push_signal(L, h, a);
        }
        if matches!(a, A::Footsteps | A::Hum) {
            return match super::sound::get_foley(h, super::sound::FoleyNode::D3(id), a == A::Hum) {
                Some(s) => rets(L, &s),
                None => {
                    lua_pushnil(L);
                    1
                }
            };
        }
        if let Some(k) = h.three.as_deref().map(|w| w.kind(id))
            && super::bodies3d::is_physics(k)
            && super::bodies3d::has(k, a)
        {
            return match super::bodies3d::get(L, h, id, a) {
                Ok(n) => n,
                Err(m) => err(L, &m),
            };
        }
        let Some(w) = h.three.as_deref_mut() else {
            return err(L, super::three::NOT_3D);
        };
        let ai = anim_index(w, id);
        let i = w.info_mut(id);
        let e = i.emit.as_deref();
        match a {
            A::Name => rets(L, &i.name),
            A::Tag => rets(L, &i.tag),
            A::Fade => match i.fade {
                Some((f, t)) => {
                    lua_createtable(L, 2, 0);
                    lua_pushnumber(L, nice(f));
                    lua_rawseti(L, -2, 1);
                    lua_pushnumber(L, nice(t));
                    lua_rawseti(L, -2, 2);
                    1
                }
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Wake => retb(L, i.wake.is_some()),
            A::Geometry => match i.geometry {
                Some(g) => push_handle(L, tag::GEOMETRY, g as u32 + 1),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Material => match i.material {
                Some(m) => push_handle(L, tag::MATERIAL, m as u32 + 1),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Model => rets(L, &i.src),
            A::Progress => ret(L, nice(w.ground.as_ref().filter(|g| g.node == id).map_or(0.0, |g| g.terrain.progress()))),
            A::Color => ret(L, (i.color >> 8) as f64),
            A::Intensity => ret(L, nice(i.intensity)),
            A::Range => ret(L, nice(i.range)),
            A::Width => ret(L, nice(i.range)),
            A::Length => ret(L, nice(i.intensity)),
            A::Text => rets(L, i.text.as_ref().map_or("", |t| t.text.as_str())),
            A::Size => ret(L, nice(i.text.as_ref().map_or(0.0, |t| t.size))),
            A::Billboard => retb(L, i.text.as_ref().is_some_and(|t| t.billboard)),
            A::Font => rets(L, if i.text.as_ref().is_some_and(|t| t.font == maimbrain::gfx2d::Font::Pixel) { "pixel" } else { "bold" }),
            A::Preset => rets(L, e.and_then(|e| maimbrain::scene::emitter::PRESETS.iter().find(|p| p.1 == e.preset)).map_or("sparkles", |p| p.0)),
            A::Rate => ret(L, nice(e.map_or(0.0, |e| e.rate))),
            A::Emitting => retb(L, e.is_some_and(|e| e.emitting)),
            A::Additive => retb(L, e.is_some_and(|e| e.desc.blend == FxBlend::Additive)),
            A::Spread => ret(L, nice(e.map_or(0.0, |e| e.desc.spread))),
            A::Drag => ret(L, nice(e.map_or(0.0, |e| e.desc.drag))),
            A::Gravity => ret(L, nice(e.map_or(0.0, |e| -e.desc.gravity.y))),
            A::Speed | A::Life | A::Sizes | A::Colors => {
                let v: Vec<f64> = match (a, e) {
                    (A::Speed, Some(e)) => e.desc.speed.iter().map(|x| nice(*x)).collect(),
                    (A::Life, Some(e)) => e.desc.lifetime.iter().map(|x| nice(*x)).collect(),
                    (A::Sizes, Some(e)) => e.desc.sizes.iter().map(|x| nice(*x)).collect(),
                    _ => Vec::new(),
                };
                lua_createtable(L, v.len() as c_int, 0);
                for (k, x) in v.into_iter().enumerate() {
                    lua_pushnumber(L, x);
                    lua_rawseti(L, -2, k as c_int + 1);
                }
                1
            }
            A::Clip | A::ClipTime | A::PlantFeet | A::Gait | A::Flight | A::Swim => {
                let Some(ai) = ai else {
                    return err(L, "not a model");
                };
                let an = &w.anims[ai];
                let tree = an.tree.and_then(|t| w.trees.get(t));
                match a {
                    A::Clip => {
                        let cur = match tree {
                            Some(t) => t.tree.samples().into_iter().max_by(|x, y| x.2.total_cmp(&y.2)).map(|s| s.0),
                            None => an.animator.current(),
                        };
                        match cur.and_then(|c| clip_name(an, c)) {
                            Some(n) => rets(L, &n),
                            None => {
                                lua_pushnil(L);
                                1
                            }
                        }
                    }
                    A::ClipTime => ret(L, nice(tree.map_or(an.animator.time(), |t| t.tree.time()))),
                    A::PlantFeet => retb(L, an.rig.plant),
                    A::Gait => {
                        if an.rig.gait.is_some() {
                            rets(L, "mech")
                        } else {
                            lua_pushnil(L);
                            1
                        }
                    }
                    A::Flight => retb(L, an.rig.flight.is_some()),
                    _ => retb(L, an.rig.swim.is_some()),
                }
            }
            _ => err(L, &format!("{} has no property `{}`", w.kind(id).name(), a.name())),
        }
    }
}

pub unsafe fn set(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A, v: c_int) -> Result<(), String> {
    unsafe {
        if let Some(k) = h.three.as_deref().map(|w| w.kind(id))
            && super::bodies3d::is_physics(k)
            && super::bodies3d::has(k, a)
        {
            return super::bodies3d::set(L, h, id, a, v);
        }
        let key = a.name();
        if matches!(a, A::Footsteps | A::Hum) {
            return super::sound::set_foley(L, h, super::sound::FoleyNode::D3(id), a == A::Hum, v);
        }
        let font = if a == A::Font { Some(resolve_font(h, st(L, v).unwrap_or("bold"))?) } else { None };
        let w = w3!(h);
        let host = w.scene.get(id).and_then(|n| n.host);
        let ai = anim_index(w, id);
        match a {
            A::Name | A::Tag => {
                let s = match lua_type(L, v) {
                    LUA_TSTRING => st(L, v).unwrap_or_default().to_string(),
                    LUA_TNIL => String::new(),
                    _ => return Err(format!("{key} takes a string, got {}", type_name(L, v))),
                };
                let i = w.info_mut(id);
                if a == A::Name { i.name = s } else { i.tag = s }
            }
            A::Fade => {
                let f = match lua_type(L, v) {
                    LUA_TNIL => None,
                    LUA_TTABLE => match flat64(L, v, "fade")?.as_slice() {
                        [a, b] if a.is_finite() && b.is_finite() && b > a => Some((*a as f32, *b as f32)),
                        _ => return Err("fade = { from, to }: the distances (meters) it fades out between, to farther than from".into()),
                    },
                    _ => return Err(format!("fade takes {{ from, to }} or nil, got {}", type_name(L, v))),
                };
                if let Some(h) = host {
                    match f {
                        Some((a, b)) => h.set_fade(a, b),
                        None => h.clear_fade(),
                    }
                }
                w.info_mut(id).fade = f;
            }
            A::Wake => {
                let on = bool_at(L, v, key)?;
                let had = w.info_mut(id).wake;
                match (on, had) {
                    (true, None) => {
                        let Some(slot) = w.wakes.iter().position(|u| !u) else {
                            return Err("wake: 8 wakes at most".into());
                        };
                        w.wakes[slot] = true;
                        w.info_mut(id).wake = Some(slot as u32);
                    }
                    (false, Some(s)) => {
                        w.wakes[s as usize] = false;
                        gfx3d::wake(s, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0);
                        w.info_mut(id).wake = None;
                    }
                    _ => {}
                }
            }
            A::Geometry | A::Material => {
                let (g, m) = {
                    let i = w.info_mut(id);
                    (i.geometry, i.material)
                };
                let (g, m) = if a == A::Geometry {
                    match handle(L, v, tag::GEOMETRY) {
                        Some(x) => (Some(x as usize - 1), m),
                        None => {
                            return Err(format!("geometry takes a geometry (geometry.box(1, 1, 1)), got {}", type_name(L, v)));
                        }
                    }
                } else {
                    match handle(L, v, tag::MATERIAL) {
                        Some(x) => (g, Some(x as usize - 1)),
                        None if lua_type(L, v) == LUA_TNIL => (g, None),
                        None => {
                            return Err(format!("material takes a material (material{{ color = 0xb07a45 }}), got {}", type_name(L, v)));
                        }
                    }
                };
                let mesh = w.uploaded(g.ok_or("a Mesh needs geometry")?)?;
                let mat = match m {
                    Some(i) => w.materials[i].0,
                    None => w.default_material()?,
                };
                if let Some(h) = host {
                    h.set_mesh(mesh, mat);
                }
                let i = w.info_mut(id);
                (i.geometry, i.material) = (g, m);
            }
            A::Model => {
                return Err("model is set when the Model is made: Model{ model = \"fox\" }".into());
            }
            A::Clip | A::ClipTime => {
                return Err(format!("{key} is read-only: fox:play(\"Run\") plays a clip"));
            }
            A::Progress => {
                return Err("progress is read-only: how much of the first view is built, 0–1".into());
            }
            A::Color => {
                let c = col(L, v)?;
                w.info_mut(id).color = c;
                if let Some(t) = &mut w.info_mut(id).text {
                    t.color = c;
                }
                apply(w, id);
            }
            A::Intensity | A::Range | A::Width | A::Length => {
                let x = number_at(L, v, key)?;
                if x < 0.0 {
                    return Err(format!("{key} is 0 or more, got {}", fmt_num(x as f64)));
                }
                let i = w.info_mut(id);
                // A trail keeps its width in `range` and its length in `intensity`.
                if matches!(a, A::Intensity | A::Length) {
                    i.intensity = x
                } else {
                    i.range = x
                }
                apply(w, id);
            }
            A::Text | A::Size | A::Billboard => {
                let i = w.info_mut(id);
                let t = i.text.get_or_insert_with(|| Box::new(Text3 { text: String::new(), size: 0.3, color: 0xffffffff, billboard: true, font: maimbrain::gfx2d::Font::SansBold }));
                match a {
                    A::Text => t.text = to_s(L, v),
                    A::Size => t.size = number_at(L, v, key)?.max(0.01),
                    _ => t.billboard = bool_at(L, v, key)?,
                }
                apply(w, id);
            }
            A::Font => {
                let i = w.info_mut(id);
                if let Some(t) = &mut i.text {
                    t.font = font.unwrap_or(maimbrain::gfx2d::Font::SansBold);
                }
                apply(w, id);
            }
            A::Preset | A::Colors | A::Sizes | A::Life | A::Speed | A::Spread | A::Drag | A::Gravity | A::Additive | A::Rate | A::Emitting => {
                let i = w.info_mut(id);
                let Some(e) = i.emit.as_mut() else {
                    return Err(format!("{key} is an Emitter's"));
                };
                match a {
                    A::Rate => e.rate = number_at(L, v, key)?.max(0.0),
                    A::Emitting => e.emitting = bool_at(L, v, key)?,
                    A::Preset => {
                        let p = super::node::preset_named(st(L, v).unwrap_or("?"))?;
                        e.preset = p;
                        e.desc = preset_desc(p, &[]);
                    }
                    A::Colors => {
                        let cs = colors_at(L, v)?;
                        let d = preset_desc(e.preset, &cs);
                        e.desc.colors = d.colors;
                    }
                    A::Sizes => {
                        let l = match lua_type(L, v) {
                            LUA_TNUMBER => vec![lua_tonumber(L, v)],
                            _ => flat64(L, v, "sizes")?,
                        };
                        for (k, s) in e.desc.sizes.iter_mut().enumerate() {
                            *s = l.get(k.min(l.len().saturating_sub(1))).copied().unwrap_or(0.1) as f32;
                        }
                    }
                    A::Life => e.desc.lifetime = range_at(L, v, key)?,
                    A::Speed => e.desc.speed = range_at(L, v, key)?,
                    A::Spread => e.desc.spread = number_at(L, v, key)?.clamp(0.0, std::f32::consts::PI),
                    A::Drag => e.desc.drag = number_at(L, v, key)?.max(0.0),
                    A::Gravity => e.desc.gravity = vec3(0.0, -number_at(L, v, key)?, 0.0),
                    _ => e.desc.blend = if bool_at(L, v, key)? { FxBlend::Additive } else { FxBlend::Alpha },
                }
                if !matches!(a, A::Rate | A::Emitting) {
                    remake_emitter(i);
                }
            }
            A::PlantFeet | A::Gait | A::Flight | A::Swim => {
                let Some(ai) = ai else {
                    return Err(format!("{key} is a Model's"));
                };
                set_motion(L, w, ai, a, v)?;
            }
            _ => return Err(unknown(w.kind(id), key, false)),
        }
        Ok(())
    }
}

/// `plantFeet`, `gait`, `flight` and `swim` on a model.
unsafe fn set_motion(L: *mut lua_State, w: &mut World3, ai: usize, a: A, v: c_int) -> Result<(), String> {
    unsafe {
        let off = matches!(lua_type(L, v), LUA_TNIL) || (lua_type(L, v) == LUA_TBOOLEAN && lua_toboolean(L, v) == 0);
        match a {
            A::PlantFeet => {
                let on = bool_at(L, v, "plantFeet")?;
                if on && !legs(w, ai) {
                    warn("plantFeet: this model has no rig with the standard leg bones (thigh, shin, foot), so nothing is planted");
                }
                w.anims[ai].rig.plant = on;
            }
            A::Gait => {
                if off {
                    w.anims[ai].rig.gait = None;
                    w.anims[ai].root.set_transform(&gfx3d::Transform::default());
                    return Ok(());
                }
                if st(L, v) != Some("mech") {
                    return Err(format!("gait = \"mech\" (a heavy biped's planted-foot walk) or nil, got {}", to_s(L, v)));
                }
                if !legs(w, ai) {
                    warn("gait: this model has no rig with the standard leg bones (thigh, shin, foot), so it doesn't walk");
                    return Ok(());
                }
                let node = w.anims[ai].node;
                let Some(body) = w.scene.get(node).and_then(|n| n.host) else {
                    return Ok(());
                };
                let rig = humanoid(w, ai).unwrap();
                let hips = rig.rest_point("hips").map_or(2.6, |p| p.y.abs().max(0.1));
                // Tuned for a mech about 5 m tall (hips ~2.6 m): scaled to this one.
                let g = MechGait::from_rig(rig, body).map(|g| g.scaled(hips / 2.6));
                w.anims[ai].rig.gait = g;
            }
            A::Flight => {
                if off {
                    w.anims[ai].rig.flight = None;
                    w.anims[ai].root.set_transform(&gfx3d::Transform::default());
                    return Ok(());
                }
                if !spine(w, ai) {
                    warn("flight: this model has no spine_0, spine_1, … bones (a `spine` rig), so it doesn't flap");
                }
                let size = super::three::fnum(L, v, c"size", 1.0).max(0.05);
                let mut f = gfx3d::Flight::new(size);
                f.flap_hz = super::three::fnum(L, v, c"flap", f.flap_hz).max(0.0);
                f.max_bank = super::three::fnum(L, v, c"bank", f.max_bank).clamp(0.0, 1.5);
                // A bird sculpted standing (most generated ones) flies level.
                if let Some(Some(sk)) = &w.anims[ai].rig.spine {
                    f = f.level_with(sk);
                }
                w.anims[ai].rig.flight = Some(f);
            }
            _ => {
                if off {
                    w.anims[ai].rig.swim = None;
                    return Ok(());
                }
                if !spine(w, ai) {
                    warn("swim: this model has no spine_0, spine_1, … bones (a `spine` rig), so it doesn't swim");
                }
                let mut s = Swim::new(super::three::fnum(L, v, c"length", 1.0).max(0.05));
                s.wave.amplitude = super::three::fnum(L, v, c"amplitude", s.wave.amplitude);
                if super::three::fflag(L, v, c"vertical") {
                    s.wave.axis = Vec3::X;
                }
                w.anims[ai].rig.swim = Some(s);
            }
        }
        Ok(())
    }
}

/// `node.parent = p` (keeping where it is in the world when `keep`) and a
/// constructor's `parent` (its place is then in `p`'s space).
pub unsafe fn set_parent(L: *mut lua_State, w: &mut World3, id: Node3Id, v: c_int, keep: bool) -> Result<(), String> {
    unsafe {
        let p = match lua_type(L, v) {
            LUA_TNIL => None,
            _ => match node_at(L, v) {
                Some(p) if w.scene.alive(p) => Some(p),
                Some(_) => return Err("parent: that node was destroyed".into()),
                None if lua_type(L, v) == LUA_TUSERDATA && lua_userdatatag(L, v) == tag::NODE => {
                    return Err("parent: a 3D node's parent is a 3D node (2D nodes draw over the world, as the HUD)".into());
                }
                None => {
                    return Err(format!("parent takes a 3D node or nil, got {}", type_name(L, v)));
                }
            },
        };
        let before = if keep { w.world_of(id) } else { None };
        if !w.scene.set_parent(id, p) {
            return Err("parent: a node can't go under itself or its own children".into());
        }
        if let Some(m) = before {
            let pm = p.and_then(|p| w.world_of(p)).unwrap_or(gfx3d::Mat4::IDENTITY);
            let t = attach_offset(&pm, &m);
            let (ry, rx, rz) = euler(t.rot);
            let n = w.scene.node_mut(id).unwrap();
            (n.x, n.y, n.z, n.rx, n.ry, n.rz, n.sx, n.sy, n.sz) = (t.pos.x, t.pos.y, t.pos.z, rx, ry, rz, t.scale.x, t.scale.y, t.scale.z);
        }
        Ok(())
    }
}

/// A rotation as the y, x, z turns (yaw, pitch, roll) that make it.
pub fn euler(q: Quat) -> (f32, f32, f32) {
    let m = gfx3d::Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE).0;
    let pitch = (-m[9]).clamp(-1.0, 1.0).asin();
    (m[8].atan2(m[10]), pitch, m[1].atan2(m[5]))
}

// --------------------------------------------------------------- methods

pub unsafe fn method(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A, name: &str) -> Result<c_int, String> {
    unsafe {
        if super::bodies3d::is_method(a) {
            return super::bodies3d::method(L, h, id, a, name);
        }
        let w = w3!(h);
        let host = w.scene.get(id).and_then(|n| n.host);
        let kind = w.kind(id);
        Ok(match a {
            // faceDirection(dx, dz): its front (−z) along the direction, by its yaw.
            A::FaceDirection => {
                let (dx, dz) = (need(L, 2, name)?, need(L, 3, name)?);
                if dx * dx + dz * dz > 1e-12 {
                    w.scene.node_mut(id).unwrap().ry = (-dx).atan2(-dz);
                }
                0
            }
            // lookAt(x, y, z): its front (−z) at the point (yaw and pitch, no roll).
            A::LookAt => {
                let t = vec3(need(L, 2, name)?, need(L, 3, name)?, need(L, 4, name)?);
                let parent = w.scene.get(id).and_then(|n| n.parent());
                let inv = parent.and_then(|p| w.world_of(p)).map_or(gfx3d::Mat4::IDENTITY, |m| m.inverse());
                let local = inv.transform_point(t);
                let n = w.scene.node_mut(id).unwrap();
                let d = local - vec3(n.x, n.y, n.z);
                if d.length_squared() > 1e-12 {
                    n.ry = (-d.x).atan2(-d.z);
                    n.rx = d.y.atan2((d.x * d.x + d.z * d.z).sqrt());
                    n.rz = 0.0;
                }
                0
            }
            A::WorldPosition => {
                let m = w.world_of(id).unwrap_or(gfx3d::Mat4::IDENTITY);
                lua_pushnumber(L, m.0[12] as f64);
                lua_pushnumber(L, m.0[13] as f64);
                lua_pushnumber(L, m.0[14] as f64);
                3
            }
            // instance(list, stride): x, y, z, then yaw, scale, colour (0xRRGGBB), pitch, roll, tall.
            A::Instance => {
                let stride = (opt(L, 3, 3.0) as usize).clamp(3, 9);
                let mut list = flat64(L, 2, "instance")?;
                if stride > 5 {
                    for row in list.chunks_exact_mut(stride) {
                        row[5] = (((row[5] as i64 as u32) << 8) | 0xff) as f64;
                    }
                }
                let out = super::three::instances(&list, stride);
                let ok = host.is_some_and(|h| h.set_instances(&out));
                w.info_mut(id).instances = out.len();
                lua_pushboolean(L, ok as c_int);
                1
            }
            A::Destroy => {
                if w.scene.get(id).is_some_and(|n| n.owned) {
                    let gone = w.scene.destroy(id);
                    w.forget(&gone);
                }
                0
            }
            A::Find | A::Bone => {
                let Some(part) = st(L, 2).map(str::to_string) else {
                    return Err(format!("bad argument #1 to '{name}' (string expected, got {})", type_name(L, 2)));
                };
                let found = match anim_index(w, id) {
                    Some(ai) if a == A::Bone => bone_node(w, ai, &part),
                    Some(ai) => w.anims[ai].root.find(&part),
                    None => host.and_then(|h| h.find(&part)),
                };
                push_part(L, w, id, &part, found)?
            }
            // reach(bone, x, y, z): two-bone IK to the point, each frame until reach(bone, nil).
            A::Reach => {
                let ai = anim_index(w, id).ok_or("reach: not a model")?;
                let Some(bone) = st(L, 2).map(str::to_string) else {
                    return Err("reach(bone, x, y, z): a hand or a foot (\"hand_r\", \"foot_l\")".into());
                };
                if lua_type(L, 3) == LUA_TNIL || lua_gettop(L) < 3 {
                    w.anims[ai].rig.reach.retain(|r| r.0 != bone);
                    return Ok(0);
                }
                let t = vec3(need(L, 3, name)?, need(L, 4, name)?, need(L, 5, name)?);
                let side = if bone.ends_with("_l") { 'l' } else { 'r' };
                let limb = match humanoid(w, ai) {
                    Some(r) if bone.starts_with("hand") => TwoBoneIk::arm(r, side),
                    Some(r) if bone.starts_with("foot") => TwoBoneIk::leg(r, side),
                    Some(_) => {
                        return Err(format!("reach: \"{bone}\" isn't a hand or a foot (hand_l, hand_r, foot_l, foot_r)"));
                    }
                    None => {
                        warn("reach: this model has no rig with the standard bones, so nothing reaches");
                        return Ok(0);
                    }
                };
                let Some(limb) = limb else { return Ok(0) };
                let r = &mut w.anims[ai].rig.reach;
                r.retain(|x| x.0 != bone);
                r.push((bone, limb, t));
                0
            }
            // headLook(x, y, z, { limit }): only the head turns toward the point; headLook(nil) lets go.
            A::HeadLook => {
                let ai = anim_index(w, id).ok_or("headLook: not a model")?;
                if lua_type(L, 2) == LUA_TNIL || lua_gettop(L) < 2 {
                    if let Some(hd) = &mut w.anims[ai].rig.head {
                        hd.1 = None;
                    }
                    return Ok(0);
                }
                let t = vec3(need(L, 2, name)?, need(L, 3, name)?, need(L, 4, name)?);
                let limit = super::three::fnum(L, 5, c"limit", 1.2).clamp(0.05, 3.0);
                let Some(bone) = humanoid(w, ai).and_then(|r| r.index("head")) else {
                    warn("headLook: this model has no head bone it recognises");
                    return Ok(0);
                };
                let a = &mut w.anims[ai];
                let look = a.rig.head.get_or_insert_with(|| {
                    let mut l = LookAt::new(bone);
                    l.forward = vec3(0.0, 0.0, -1.0);
                    (l, None)
                });
                look.0.yaw_limit = limit;
                look.0.pitch_limit = limit * 0.5;
                look.1 = Some(t);
                0
            }
            // restyle{ toon, outline, rim, ink, fog }: every material the model came with.
            A::Restyle => {
                let ai = anim_index(w, id).ok_or("restyle: not a model")?;
                if lua_type(L, 2) != LUA_TTABLE {
                    return Err("restyle{ toon = 3, outline = 1.2, rim = 0.4, ink = 0x101018, fog = true }".into());
                }
                let ink = with_field(L, 2, c"ink", || if lua_type(L, -1) == LUA_TNIL { Ok(0x000000ff) } else { col(L, -1) })?;
                let l = lin(ink);
                let fog = with_field(L, 2, c"fog", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
                let style = MaterialStyle {
                    outline_color: [l[0], l[1], l[2]],
                    outline_width: fnum(L, 2, c"outline", 0.0).clamp(0.0, 8.0),
                    toon_bands: fnum(L, 2, c"toon", 0.0).clamp(0.0, 8.0) as u32,
                    rim: fnum(L, 2, c"rim", 0.0).clamp(0.0, 1.0),
                    flags: if fog { MaterialStyle::NO_FOG } else { 0 },
                    ..Default::default()
                };
                for m in w.anims[ai].model.materials() {
                    m.set_style(&style);
                }
                0
            }
            A::Play => play(L, w, id)?,
            A::HeightAt | A::NormalAt | A::Raise => super::terrain::method(L, w, id, a, name)?,
            // e:emit(n, { dx, dy, dz }): a burst at the emitter, along the direction (all around without one).
            A::Emit => {
                let n = opt(L, 2, 16.0).clamp(0.0, 4096.0) as u32;
                let d = vec3(fnum(L, 3, c"dx", 0.0), fnum(L, 3, c"dy", 0.0), fnum(L, 3, c"dz", 0.0));
                let m = w.world_of(id).unwrap_or(gfx3d::Mat4::IDENTITY);
                if let Some(e) = w.info_mut(id).emit.as_ref() {
                    e.emitter.emit(vec3(m.0[12], m.0[13], m.0[14]), d, n);
                }
                0
            }
            _ => return Err(unknown(kind, name, true)),
        })
    }
}

// ------------------------------------------------------------ constructors

/// Whether node.rs's `Group`, `Emitter` or `Text` makes a 3D node: in a 3D
/// game a `Group` or `Emitter` is 3D unless its `parent` is a 2D node, and
/// a `Text` is 3D with `billboard = true` or a 3D `parent`.
pub unsafe fn wants_3d(L: *mut lua_State, h: &Host, kind: maimbrain::scene::Kind) -> bool {
    unsafe {
        use maimbrain::scene::Kind;
        if h.three.is_none() || !matches!(kind, Kind::Group | Kind::Emitter | Kind::Text) {
            return false;
        }
        let parent = with_field(L, 1, c"parent", || if lua_type(L, -1) == LUA_TUSERDATA { lua_userdatatag(L, -1) } else { 0 });
        match kind {
            Kind::Text => parent == tag::NODE3 || with_field(L, 1, c"billboard", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) != 0),
            _ => parent != tag::NODE,
        }
    }
}

pub unsafe fn make_from_2d(L: *mut lua_State, h: &mut Host, kind: maimbrain::scene::Kind) -> c_int {
    use maimbrain::scene::Kind;
    let k = match kind {
        Kind::Group => Kind3::Group,
        Kind::Emitter => Kind3::Emitter,
        _ => Kind3::Text,
    };
    unsafe { make(L, h, k) }
}

b!(mk_mesh, |L, h| make(L, h, Kind3::Mesh));
b!(mk_model, |L, h| make(L, h, Kind3::Model));
b!(mk_light, |L, h| make(L, h, Kind3::Light));
b!(mk_trail, |L, h| make(L, h, Kind3::Trail));

/// What a constructor reads before the rest: what the node is made of.
const FIRST: &[A] = &[A::Parent, A::Geometry, A::Material, A::Model, A::Preset];

unsafe fn make(L: *mut lua_State, h: &mut Host, kind: Kind3) -> c_int {
    unsafe {
        let ctor = kind.name();
        let t = lua_type(L, 1);
        if t != LUA_TTABLE && t != LUA_TNIL && t != -1 {
            return err(L, &format!("{ctor} takes one table: {ctor}{{ x = 1, y = 2 }} (got {})", type_name(L, 1)));
        }
        let field = |k: &CStr| -> c_int {
            if t == LUA_TTABLE {
                lua_rawgetfield(L, 1, k.as_ptr());
            } else {
                lua_pushnil(L);
            }
            lua_gettop(L)
        };
        // The model, read before the world is borrowed.
        let mut model: Option<(usize, String)> = None;
        if kind == Kind3::Model {
            let at = field(c"model");
            let name = st(L, at).map(str::to_string);
            lua_pop(L, 1);
            let Some(name) = name else {
                return err(L, "Model{…} needs model = \"<name>\": models/<name>.glb");
            };
            let key = name.strip_prefix("models/").unwrap_or(&name).trim_end_matches(".glb").to_string();
            let Some(i) = h.project.models.iter().position(|m| *m == key) else {
                return err(L, &format!("Model{{…}}: there's no models/{key}.glb (generate(kind: \"model\", name: \"{key}\") makes one)"));
            };
            model = Some((i, key));
        }
        let Some(w) = h.three.as_deref_mut() else {
            return err(L, &format!("{ctor}{{…}} is 3D's (kind = \"3d\" in studio.toml)"));
        };
        // The parent.
        let at = field(c"parent");
        let parent = match lua_type(L, at) {
            LUA_TNIL => None,
            _ => match node_at(L, at).filter(|p| w.scene.alive(*p)) {
                Some(p) => Some(p),
                None => {
                    lua_pop(L, 1);
                    return err(L, &format!("{ctor}{{…}}: parent is a 3D node (got {})", type_name(L, at)));
                }
            },
        };
        lua_pop(L, 1);
        let parent_host = parent.and_then(|p| w.scene.get(p)).and_then(|p| p.host);
        let mut info = Info::new(kind);
        let mut spawned = None;
        let made = match kind {
            Kind3::Mesh => {
                let g = field(c"geometry");
                let geo = handle(L, g, tag::GEOMETRY);
                let m = field(c"material");
                let mat = handle(L, m, tag::MATERIAL);
                let bad_mat = lua_type(L, m) != LUA_TNIL && mat.is_none();
                lua_pop(L, 2);
                let Some(geo) = geo else {
                    return err(L, "Mesh{…} needs geometry = geometry.box(1, 1, 1) (or any geometry.*)");
                };
                if bad_mat {
                    return err(L, "Mesh{…}: material is a material{ … }");
                }
                let up = match w.uploaded(geo as usize - 1) {
                    Ok(u) => u,
                    Err(e) => return err(L, &format!("Mesh{{…}}: {e}")),
                };
                let mat_h = match mat {
                    Some(i) => w.materials[i as usize - 1].0,
                    None => match w.default_material() {
                        Ok(m) => m,
                        Err(e) => return err(L, &format!("Mesh{{…}}: {e}")),
                    },
                };
                info.geometry = Some(geo as usize - 1);
                info.material = mat.map(|m| m as usize - 1);
                gfx3d::Node::with_mesh(up, mat_h)
            }
            Kind3::Model => {
                let (i, key) = model.clone().unwrap();
                if w.models.get(i).copied().flatten().is_none() {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if w.models.len() <= i {
                            w.models.resize(i + 1, None);
                        }
                        w.models[i] = gfx3d::Model::load(maimbrain::sys::Asset(1));
                    }
                }
                let Some(m) = w.models.get(i).copied().flatten() else {
                    return err(L, &format!("Model{{…}}: models/{key}.glb didn't load"));
                };
                info.src = key;
                let holder = gfx3d::Node::new();
                if let Some(hd) = holder {
                    match m.spawn(Some(hd)) {
                        Some(root) => spawned = Some((root, m)),
                        None => {
                            hd.destroy();
                            return err(L, "Model{…}: the model didn't spawn (too many nodes? 4096 at most)");
                        }
                    }
                }
                holder
            }
            _ => gfx3d::Node::new(),
        };
        let Some(n) = made else {
            return err(L, "too many nodes (4096 at most): reuse nodes, or draw many with node:instance");
        };
        if let Some(p) = parent.filter(|p| w.scene.get(*p).is_some_and(|x| !x.owned)) {
            // Under a bone: a socket that cancels the bone's turn and size, so
            // the node's place, turn and scale are meters in the model's frame.
            info.socket = socket(w, p);
            n.set_parent(info.socket.or(parent_host));
        } else if parent_host.is_some() {
            n.set_parent(parent_host);
        }
        let id = match w.scene.add(Some(n), true, parent) {
            Ok(id) => id,
            Err(_) => {
                n.destroy();
                return err(L, "too many nodes (4096 at most): reuse nodes, or draw many with node:instance");
            }
        };
        match kind {
            Kind3::Emitter => {
                let pf = field(c"preset");
                let p = match lua_type(L, pf) {
                    LUA_TNIL => Ok(Preset::Sparkles),
                    _ => super::node::preset_named(st(L, pf).unwrap_or("?")),
                };
                lua_pop(L, 1);
                let p = match p {
                    Ok(p) => p,
                    Err(m) => {
                        destroy(w, id);
                        return err(L, &format!("Emitter{{…}}: {m}"));
                    }
                };
                let desc = preset_desc(p, &[]);
                match gfx3d::Emitter::new(&desc) {
                    Some(e) => info.emit = Some(Box::new(Emit3 { emitter: e, preset: p, rate: 0.0, carry: 0.0, emitting: true, desc })),
                    None => {
                        destroy(w, id);
                        return err(L, "Emitter{…}: 256 emitters at most");
                    }
                }
            }
            Kind3::Trail => {
                (info.color, info.range, info.intensity) = (0x9fdcffff, 0.2, 0.5);
                let d = trail_desc(info.color, info.range, info.intensity);
                match Trail::new(&d) {
                    Some(t) => {
                        t.attach(n);
                        info.trail = Some((t, d));
                    }
                    None => {
                        destroy(w, id);
                        return err(L, "Trail{…}: 64 trails at most");
                    }
                }
            }
            Kind3::Text => {
                info.text = Some(Box::new(Text3 { text: String::new(), size: 0.3, color: 0xffffffff, billboard: true, font: maimbrain::gfx2d::Font::SansBold }));
            }
            Kind3::Light => (info.color, info.intensity, info.range) = (0xffffffff, 1.0, 6.0),
            _ => {}
        }
        w.info.insert(id.to_bits(), info);
        if let Some((root, m)) = spawned {
            w.anims.push(new_anim(id, root, m));
        }
        if kind == Kind3::Light {
            apply(w, id);
        }
        // The rest of the table, in its order.
        if t == LUA_TTABLE {
            lua_pushnil(L);
            while lua_next(L, 1) != 0 {
                let (a, key) = key_at(L, -2);
                let r = match a {
                    Some(a) if FIRST.contains(&a) => Ok(()),
                    _ if lua_type(L, -2) != LUA_TSTRING => Err(format!("{ctor}{{…}} takes names = values, got a {} key", type_name(L, -2))),
                    Some(a) if is_method(a) || is_signal(a) || super::three::is_method(a) && !super::three::is_prop(a) && !has(kind, a) => {
                        Err(format!("{key} is a method: call node:{key}(…) after making it"))
                    }
                    _ => set_any(L, h, id, a, &key, -1),
                };
                if let Err(m) = r {
                    lua_pop(L, 2);
                    if let Some(w) = h.three.as_deref_mut() {
                        destroy(w, id);
                    }
                    return err(L, &format!("{ctor}{{…}}: {m}"));
                }
                lua_pop(L, 1);
            }
        }
        let w = h.three.as_deref_mut().unwrap();
        if kind == Kind3::Text {
            apply(w, id);
        }
        push_node(L, w, id);
        1
    }
}

/// A node under bone `part`, where the bone is now, turned as the model
/// it's part of and at size 1: what a prop held in a hand hangs from.
fn socket(w: &mut World3, part: Node3Id) -> Option<gfx3d::Node> {
    let bone = w.scene.get(part)?.host?;
    let bw = w.world_of(part)?;
    let model = w.found.iter().find(|f| f.2 == part).map(|f| f.0);
    let rot = model.and_then(|m| w.world_of(m)).map_or(Quat::IDENTITY, |m| {
        let c = |i: usize| vec3(m.0[i * 4], m.0[i * 4 + 1], m.0[i * 4 + 2]).normalize();
        Quat::from_basis(c(0), c(1), c(2))
    });
    let want = gfx3d::Transform { pos: vec3(bw.0[12], bw.0[13], bw.0[14]), rot, scale: Vec3::ONE };
    let s = gfx3d::Node::new()?;
    s.set_parent(Some(bone));
    s.set_transform(&attach_offset(&bw, &want.matrix()));
    Some(s)
}

fn destroy(w: &mut World3, id: Node3Id) {
    let gone = w.scene.destroy(id);
    w.forget(&gone);
}

// ---------------------------------------------------------- AnimationTree

unsafe fn tree_at(L: *mut lua_State) -> Option<usize> {
    unsafe { handle(L, 1, tag::ANIMTREE).map(|i| i as usize - 1) }
}

// `AnimationTree{ model, states, transitions, start }` (Godot's, §5 [models]).
b!(mk_tree, |L, h| match tree(L, h) {
    Ok(n) => n,
    Err(m) => err(L, &format!("AnimationTree{{…}}: {m}")),
});

unsafe fn tree(L: *mut lua_State, h: &mut Host) -> Result<c_int, String> {
    unsafe {
        if lua_type(L, 1) != LUA_TTABLE {
            return Err("takes one table: AnimationTree{ model = fox, states = { … }, transitions = { … }, start = \"move\" }".into());
        }
        let w = w3!(h);
        lua_rawgetfield(L, 1, c"model".as_ptr());
        let model = node_at(L, -1).filter(|n| w.scene.alive(*n));
        lua_pop(L, 1);
        let Some(model) = model else {
            return Err("model is the Model it runs (model = fox)".into());
        };
        let Some(ai) = anim_index(w, model) else {
            return Err("model is a Model (Model{ model = \"fox\" })".into());
        };
        // States, by name in order (so the same project builds the same tree).
        lua_rawgetfield(L, 1, c"states".as_ptr());
        let st_at = lua_gettop(L);
        if lua_type(L, st_at) != LUA_TTABLE {
            return Err("states = { move = { blend = { Idle = 0, Run = 5 }, by = \"speed\" }, jump = { clip = \"Jump\", once = true } }".into());
        }
        let mut names = Vec::new();
        lua_pushnil(L);
        while lua_next(L, st_at) != 0 {
            if let Some(k) = st(L, -2) {
                names.push(k.to_string());
            }
            lua_pop(L, 1);
        }
        names.sort();
        if names.is_empty() {
            return Err("states is empty: name at least one".into());
        }
        let mut t = AnimTree::new();
        let mut nexts: Vec<(usize, String, f32)> = Vec::new();
        for name in &names {
            let k = std::ffi::CString::new(name.as_str()).unwrap_or_default();
            lua_rawgetfield(L, st_at, k.as_ptr());
            let s = lua_gettop(L);
            if lua_type(L, s) != LUA_TTABLE {
                return Err(format!("states.{name} is a table: {{ clip = \"Run\" }} or {{ blend = {{ … }}, by = \"speed\" }}"));
            }
            let clip = with_field(L, s, c"clip", || st(L, -1).map(str::to_string));
            let motion = if let Some(c) = clip {
                let c = clip_of(w, ai, &c, true).map_err(|e| format!("states.{name}: {e}"))?;
                if fflag(L, s, c"once") { Motion::once(c) } else { Motion::looping(c) }
            } else {
                lua_rawgetfield(L, s, c"blend".as_ptr());
                let b = lua_gettop(L);
                if lua_type(L, b) != LUA_TTABLE {
                    return Err(format!("states.{name} needs clip = \"…\" or blend = {{ Clip = value, … }} with by"));
                }
                lua_rawgetfield(L, s, c"by".as_ptr());
                let by = lua_gettop(L);
                let mut pts1: Vec<(f32, gfx3d::Clip)> = Vec::new();
                let mut pts2: Vec<(String, [f32; 2], gfx3d::Clip)> = Vec::new();
                lua_pushnil(L);
                while lua_next(L, b) != 0 {
                    let Some(cn) = st(L, -2).map(str::to_string) else {
                        lua_pop(L, 2);
                        return Err(format!("states.{name}.blend's keys are clip names"));
                    };
                    let c = clip_of(w, ai, &cn, true).map_err(|e| format!("states.{name}.blend: {e}"))?;
                    match lua_type(L, -1) {
                        LUA_TNUMBER => pts1.push((lua_tonumber(L, -1) as f32, c)),
                        LUA_TTABLE => {
                            let v = super::three::nums_at(L, -1, [0.0f32; 2]);
                            pts2.push((cn, v, c));
                        }
                        _ => {
                            lua_pop(L, 2);
                            return Err(format!("states.{name}.blend.{cn} is a number (1D) or {{ x, y }} (2D)"));
                        }
                    }
                    lua_pop(L, 1);
                }
                let m = if !pts2.is_empty() {
                    let names2 = [1, 2].map(|k| {
                        lua_rawgeti(L, by, k);
                        let s = st(L, -1).unwrap_or("").to_string();
                        lua_pop(L, 1);
                        s
                    });
                    if lua_type(L, by) != LUA_TTABLE || names2.iter().any(String::is_empty) {
                        return Err(format!("states.{name}: a 2D blend's by is two parameters: by = {{ \"x\", \"y\" }}"));
                    }
                    pts2.sort_by(|a, b| a.0.cmp(&b.0));
                    let pts: Vec<([f32; 2], gfx3d::Clip)> = pts2.into_iter().map(|p| (p.1, p.2)).collect();
                    Motion::blend2d([&names2[0], &names2[1]], &pts)
                } else {
                    let Some(by) = st(L, by).map(str::to_string) else {
                        return Err(format!("states.{name}: a blend needs by = \"<parameter>\""));
                    };
                    Motion::blend1d(&by, &pts1)
                };
                lua_pop(L, 2);
                m
            };
            let i = t.add_state(name, motion);
            t.states[i].speed = fnum(L, s, c"speed", 1.0).max(0.0);
            if let Some(nx) = with_field(L, s, c"next", || st(L, -1).map(str::to_string)) {
                nexts.push((i, nx, fnum(L, s, c"fade", 0.1)));
            }
            lua_pop(L, 1);
        }
        lua_pop(L, 1);
        for (i, nx, fade) in nexts {
            let Some(to) = t.state_named(&nx) else {
                return Err(format!("states.{}.next = \"{nx}\": there's no state \"{nx}\"", t.states[i].name));
            };
            t.states[i].next = Some(to);
            t.states[i].next_fade = fade.max(0.0);
        }
        // Transitions, in order.
        lua_rawgetfield(L, 1, c"transitions".as_ptr());
        let tr = lua_gettop(L);
        if lua_type(L, tr) == LUA_TTABLE {
            for k in 1..=lua_objlen(L, tr) {
                lua_rawgeti(L, tr, k);
                let e = lua_gettop(L);
                let get = |f: &CStr| with_field(L, e, f, || st(L, -1).map(str::to_string));
                let (from, to, when) = (get(c"from"), get(c"to"), get(c"when"));
                let state = |n: &Option<String>, what: &str| -> Result<Option<usize>, String> {
                    match n.as_deref() {
                        None | Some("*") => Ok(None),
                        Some(n) => t.state_named(n).map(Some).ok_or(format!("transitions[{k}].{what} = \"{n}\": there's no state \"{n}\"")),
                    }
                };
                let f = state(&from, "from")?;
                let Some(to) = state(&to, "to")? else {
                    return Err(format!("transitions[{k}] needs to = \"<state>\""));
                };
                let mut x = Transition::new(f, to).fade(fnum(L, e, c"fade", 0.2));
                if let Some(wn) = when {
                    x = x.when(&wn);
                }
                t.add_transition(x);
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
        let start = with_field(L, 1, c"start", || st(L, -1).map(str::to_string));
        let s = match start {
            Some(n) => t.state_named(&n).ok_or(format!("start = \"{n}\": there's no state \"{n}\""))?,
            None => 0,
        };
        t.start(s);
        w.trees.push(TreeInst { tree: t, model });
        let ti = w.trees.len() - 1;
        if let Some(old) = w.anims[ai].tree.replace(ti) {
            w.trees[old].tree = AnimTree::new();
        }
        Ok(push_handle(L, tag::ANIMTREE, ti as u32 + 1))
    }
}

b!(t_index, |L, h| {
    let Some(i) = tree_at(L) else {
        return err(L, "not an AnimationTree");
    };
    let (a, key) = key_at(L, 2);
    let Some(w) = h.three.as_deref_mut() else {
        return err(L, super::three::NOT_3D);
    };
    let t = &w.trees[i];
    match a {
        Some(A::State) => match t.tree.current() {
            Some(s) => rets(L, &t.tree.states[s].name),
            None => {
                lua_pushnil(L);
                1
            }
        },
        Some(A::Model) => {
            let m = t.model;
            push_node(L, w, m);
            1
        }
        _ => err(L, &format!("no property `{key}` on an AnimationTree (state, model; methods travel, set, get)")),
    }
});

b!(t_namecall, |L, h| {
    let Some(i) = tree_at(L) else {
        return err(L, "not an AnimationTree");
    };
    let mut atom = -1;
    let name = cstr(lua_namecallatom(L, &mut atom)).unwrap_or_default();
    let Some(w) = h.three.as_deref_mut() else {
        return err(L, super::three::NOT_3D);
    };
    let t = &mut w.trees[i].tree;
    match A::from(atom) {
        Some(A::Travel) => {
            let Some(s) = st(L, 2) else {
                return bad(L, 2, "travel", "state name");
            };
            match t.state_named(s) {
                Some(s) => {
                    t.travel(s);
                    0
                }
                None => err(L, &format!("travel: there's no state \"{s}\"")),
            }
        }
        Some(A::Set) => {
            let Some(p) = st(L, 2).map(str::to_string) else {
                return bad(L, 2, "set", "parameter name");
            };
            match lua_type(L, 3) {
                LUA_TBOOLEAN => t.set_bool(&p, lua_toboolean(L, 3) != 0),
                LUA_TNUMBER => t.set(&p, lua_tonumber(L, 3) as f32),
                _ => {
                    return err(L, &format!("set(\"{p}\", value): a number (a blend's by) or true/false (a transition's when), got {}", type_name(L, 3)));
                }
            }
            0
        }
        Some(A::Get) => {
            let Some(p) = st(L, 2) else {
                return bad(L, 2, "get", "parameter name");
            };
            ret(L, t.get(p) as f64)
        }
        _ => err(L, &format!("no method `{name}` on an AnimationTree (travel, set, get)")),
    }
});

// --------------------------------------------------------------- the camera

/// The 3D camera's properties and methods (beyond the 2D camera's).
pub fn cam_prop(a: A) -> bool {
    matches!(a, A::X | A::Y | A::Z | A::Fov | A::Roll | A::Near | A::Far)
}
pub fn cam_method(a: A) -> bool {
    matches!(a, A::LookAt | A::Follow | A::Project | A::Ray)
}

/// The type names the reference and the type definitions use for 3D kinds
/// (2D's `Emitter` and `Text` are other types).
pub const TYPES3: &[(&str, Kind3)] =
    &[
        ("Mesh", Kind3::Mesh),
        ("Model", Kind3::Model),
        ("Light", Kind3::Light),
        ("Trail", Kind3::Trail),
        ("Emitter3D", Kind3::Emitter),
        ("Text3D", Kind3::Text),
        ("Terrain", Kind3::Terrain),
        ("RigidBody3D", Kind3::RigidBody),
        ("StaticBody3D", Kind3::StaticBody),
        ("Area3D", Kind3::Area),
        ("VehicleBody3D", Kind3::Vehicle),
        ("VehicleWheel3D", Kind3::Wheel),
        ("Joint3D", Kind3::Joint),
        ("CharacterBody3D", Kind3::Character),
        ("NavigationRegion3D", Kind3::NavRegion),
        ("NavigationAgent3D", Kind3::NavAgent),
    ];

/// The 3D camera's properties (§5 [camera]): x, y, z, fov (degrees), roll, near, far.
pub unsafe fn cam_index(L: *mut lua_State, h: &mut Host, a: Option<A>) -> Option<c_int> {
    unsafe {
        let w = h.three.as_deref()?;
        let c = &w.cam;
        let v = match a? {
            A::X => c.pos.x,
            A::Y => c.pos.y,
            A::Z => c.pos.z,
            A::Fov => c.fov.to_degrees(),
            A::Roll => c.roll,
            A::Near => c.near,
            A::Far => c.far,
            _ => return None,
        };
        Some(ret(L, v as f64))
    }
}

pub unsafe fn cam_newindex(L: *mut lua_State, h: &mut Host, a: Option<A>, key: &str) -> Option<c_int> {
    unsafe {
        let w = h.three.as_deref_mut()?;
        if !matches!(a?, A::X | A::Y | A::Z | A::Fov | A::Roll | A::Near | A::Far) {
            return None;
        }
        let Some(v) = num(L, 3).filter(|v| v.is_finite()) else {
            return Some(err(L, &format!("camera.{key} takes a number, got {}", type_name(L, 3))));
        };
        let v = v as f32;
        let c = &mut w.cam;
        match a? {
            A::X | A::Y | A::Z => {
                match a? {
                    A::X => c.pos.x = v,
                    A::Y => c.pos.y = v,
                    _ => c.pos.z = v,
                }
                w.follow = None;
            }
            A::Fov if (1.0..=170.0).contains(&v) => c.fov = v.to_radians(),
            A::Fov => {
                return Some(err(L, &format!("camera.fov is in degrees, 1 to 170 (about 50–70 looks natural), got {}", fmt_num(v as f64))));
            }
            A::Roll => c.roll = v,
            A::Near if v > 0.0 => c.near = v,
            A::Far if v > c.near => {
                c.far = v;
                w.far_set = true;
            }
            _ => {
                return Some(err(L, &format!("camera.{key}: near is above 0 and far beyond near")));
            }
        }
        w.cam_set = true;
        h.libs.written(crate::libs::Target::Camera, key);
        Some(0)
    }
}

/// `lookAt(x, y, z)`, `follow(node, { behind, above, lag, lookAhead, bank })`, `project`, `ray`.
pub unsafe fn cam_namecall(L: *mut lua_State, h: &mut Host, a: Option<A>) -> Option<c_int> {
    unsafe {
        let screen = h.screen;
        let w = h.three.as_deref_mut()?;
        let g = |i: c_int| opt(L, i, 0.0) as f32;
        Some(match a? {
            A::LookAt => {
                let t = vec3(g(2), g(3), g(4));
                if (t - w.cam.pos).length_squared() > 1e-10 {
                    w.cam.rot = look_at(w.cam.pos, t, Vec3::Y);
                }
                w.follow = None;
                w.cam_set = true;
                0
            }
            A::Follow => {
                if lua_type(L, 2) == LUA_TNIL || lua_gettop(L) < 2 {
                    w.follow = None;
                    return Some(0);
                }
                let Some(node) = node_at(L, 2).filter(|n| w.scene.alive(*n)) else {
                    return Some(bad(L, 2, "camera:follow", "3D node"));
                };
                w.follow = Some(Follow3 {
                    node,
                    behind: fnum(L, 3, c"behind", 5.0),
                    above: fnum(L, 3, c"above", 2.0),
                    lag: fnum(L, 3, c"lag", 0.25).max(0.0),
                    look_ahead: fnum(L, 3, c"lookAhead", 1.0),
                    bank: fnum(L, 3, c"bank", 0.0),
                    spring: None,
                    last: None,
                });
                w.cam_set = true;
                0
            }
            A::Project => {
                let [sw, sh, ox, oy, s] = screen;
                let p = w.camera().project(vec3(g(2), g(3), g(4)), sw, sh);
                lua_pushnumber(L, ((p.x - ox) / s) as f64);
                lua_pushnumber(L, ((p.y - oy) / s) as f64);
                lua_pushboolean(L, p.on_screen as c_int);
                3
            }
            A::Ray => {
                let [sw, sh, ox, oy, s] = screen;
                let (px, py) = (ox + g(2) * s, oy + g(3) * s);
                let (nx, ny) = (px / sw.max(1.0) * 2.0 - 1.0, 1.0 - py / sh.max(1.0) * 2.0);
                let c = w.camera();
                let k = (c.fov_y * 0.5).tan();
                let d = (c.rot * vec3(nx * k * sw / sh.max(1.0), ny * k, -1.0)).normalize();
                for x in [c.pos.x, c.pos.y, c.pos.z, d.x, d.y, d.z] {
                    lua_pushnumber(L, x as f64);
                }
                6
            }
            _ => return None,
        })
    }
}

// -------------------------------------------------------------- fx in 3D

/// `fx.burst(preset, x, y, z, { count, color })`.
pub unsafe fn burst(L: *mut lua_State, h: &mut Host, preset: Preset) -> c_int {
    unsafe {
        let p = vec3(opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32);
        let count = fnum(L, 5, c"count", 16.0).clamp(0.0, 4096.0) as u32;
        let colors = if with_field(L, 5, c"color", || lua_type(L, -1) != LUA_TNIL) {
            lua_rawgetfield(L, 5, c"color".as_ptr());
            let c = colors_at(L, -1);
            lua_pop(L, 1);
            match c {
                Ok(c) => c,
                Err(m) => return err(L, &format!("fx.burst: {m}")),
            }
        } else {
            Vec::new()
        };
        let Some(w) = h.three.as_deref_mut() else {
            return 0;
        };
        let e = match w.bursts.iter().find(|b| b.0 == preset && b.1 == colors) {
            Some(b) => b.2,
            None => {
                let Some(e) = gfx3d::Emitter::new(&preset_desc(preset, &colors)) else {
                    return err(L, "fx.burst: too many particle looks (256 emitters)");
                };
                w.bursts.push((preset, colors, e));
                e
            }
        };
        let dir = if matches!(preset, Preset::Confetti | Preset::Ring) { Vec3::Y } else { Vec3::ZERO };
        e.emit(p, dir, count);
        0
    }
}

/// `fx.popup(text, x, y, z, { color, size })`: rises and fades, facing the camera.
pub unsafe fn popup(L: *mut lua_State, h: &mut Host) -> c_int {
    unsafe {
        let text = to_s(L, 1);
        let p = vec3(opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32);
        let color = if with_field(L, 5, c"color", || lua_type(L, -1) != LUA_TNIL) {
            lua_rawgetfield(L, 5, c"color".as_ptr());
            let c = col(L, -1);
            lua_pop(L, 1);
            match c {
                Ok(c) => c,
                Err(m) => return err(L, &format!("fx.popup: color: {m}")),
            }
        } else {
            0xffffffff
        };
        let size = fnum(L, 5, c"size", 0.4).clamp(0.05, 5.0);
        let Some(w) = h.three.as_deref_mut() else {
            return 0;
        };
        if w.popups.len() >= 32 {
            return 0;
        }
        let Some(n) = gfx3d::Node::new() else {
            return 0;
        };
        let mut d = TextDesc::new(maimbrain::gfx2d::Font::SansBold, size).with_color(lin(color)).billboard().with_outline([0.0, 0.0, 0.0, 0.8], 0.06);
        d.flags |= TextDesc::ON_TOP;
        n.set_transform(&gfx3d::Transform::at(p));
        n.set_text(&d, &text);
        w.popups.push(Popup3 { node: n, text, desc: d, pos: p, age: 0.0 });
        0
    }
}

/// The node's heading, for tests.
#[allow(dead_code)]
pub fn heading(w: &mut World3, id: Node3Id) -> Option<f32> {
    w.world_of(id).map(|m| heading_of(&m))
}
