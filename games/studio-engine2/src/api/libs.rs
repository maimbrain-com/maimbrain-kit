//! LÖVE's libraries from Lua (docs/STUDIO-2.md §5 [libraries]): flux,
//! hump.timer and anim8 over libs.rs (their per-frame work in Rust), bump
//! over the SDK's port (`maimbrain::bump`), and the engine's frame entry
//! point, `__engine.update`, which runs them and hands the prelude what has
//! to run in Lua. classic is the real library, bundled as Lua
//! (src/classic.lua). Each is under its own global name (`flux`, `timer`,
//! `anim8`, `bump`, `Object`), and `require` of its usual module name
//! returns it.
//!
//! What differs from the libraries, all for one reason (they run for you):
//! `flux.update`, `timer.update` and `animation:update` do nothing, since
//! the engine updates them each frame before `love.update`; hump.timer's
//! timers run in the order they were made (hump's `pairs` order isn't
//! fixed); bump answers in item order where bump.lua's order isn't fixed.
//! flux's `:yoyo()` is ours.
//!
//! The libraries' license notices (all MIT):
//!
//! flux — Copyright (c) 2016 rxi
//! hump.timer — Copyright (c) 2010-2013 Matthias Richter
//! anim8 — Copyright (c) 2011 Enrique García Cota
//! bump.lua — Copyright (c) 2014 Enrique García Cota (sdk/maimbrain/src/bump.rs)
//! classic — Copyright (c) 2014, rxi (src/classic.lua)
//!
//! > Permission is hereby granted, free of charge, to any person obtaining a
//! > copy of this software and associated documentation files (the
//! > "Software"), to deal in the Software without restriction, including
//! > without limitation the rights to use, copy, modify, merge, publish,
//! > distribute, sublicense, and/or sell copies of the Software, and to
//! > permit persons to whom the Software is furnished to do so, subject to
//! > the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be included
//! > in all copies or substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
//! > OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! > MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//! > IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! > CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
//! > TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
//! > SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::ffi::{c_int, c_void};

use maimbrain::bump::{self, Rect, Response};
use maimbrain::scene::{Cmd, Kind, NodeId};

use super::atoms::{A, NAMES};
use super::node::{node_at, push_node};
use super::*;
use crate::libs::{Anim, Due, OnLoop, Target, Timer, Tween, Values, Var, flux_ease};

/// A bump world: the SDK's, and its items both ways (registry tables:
/// id → Lua value, Lua value → id).
pub struct BumpWorld {
    pub world: bump::World,
    pub items_ref: c_int,
    pub ids_ref: c_int,
    pub next: u32,
    /// A move, check or query of this world is running its Lua filter:
    /// the world is out of `h.worlds` while it does, so calls on it then
    /// are errors (bump's filters only answer).
    pub busy: bool,
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "__engine", &[(c"update", update), (c"hooks", hooks), (c"timerCancel", timer_cancel), (c"drawing", drawing)]);
        register(L, ud, "flux", &[(c"to", flux_to), (c"update", noop)]);
        register_kind(L, ud, tag::TWEEN, c"Tween", &[(c"__namecall", tween_call)]);
        register(L, ud, "timer", &[(c"after", t_after), (c"every", t_every), (c"during", t_during), (c"cancel", t_cancel), (c"clear", t_clear), (c"update", noop)]);
        register_methods(L, ud, tag::TIMER, c"TimerHandle", &[(c"cancel", t_cancel)]);
        register(L, ud, "anim8", &[(c"newGrid", newGrid), (c"newAnimation", newAnimation)]);
        register_kind(L, ud, tag::GRID, c"Grid", &[(c"__call", grid_call), (c"__namecall", grid_namecall)]);
        register_kind(L, ud, tag::ANIM, c"Animation", &[(c"__index", anim_index), (c"__namecall", anim_namecall)]);
        register(L, ud, "bump", &[(c"newWorld", newWorld)]);
        register_kind(L, ud, tag::WORLD, c"World", &[(c"__namecall", world_call)]);
        sv_push_rfn(L, spring, ud, c"spring".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"spring".as_ptr());
    }
}

b!(noop, |L, h| 0);

b!(
    /// `__engine.drawing(on)`: love.draw is running, so love.physics bodies
    /// read where they're drawn (between their last two steps), and a text
    /// measure isn't a read-back (`Host::read_back`).
    drawing, |L, h| {
    h.in_draw = lua_toboolean(L, 1) != 0;
    #[cfg(phys2)]
    crate::api::lovephysics::set_drawing(h, h.in_draw);
    0
});

b!(
    /// `__engine.hooks(signalFor, nodeMethods)`: the prelude's signal maker and
    /// its table of node methods written in Lua (`createTween`).
    hooks, |L, h| {
    lua_pushvalue(L, 1);
    h.signal_ref = lua_ref(L, -1);
    lua_pop(L, 1);
    lua_pushvalue(L, 2);
    h.ext_ref = lua_ref(L, -1);
    lua_pop(L, 1);
    0
});

/// A node's numeric property by name, for tweens.
fn node_atom(key: &str) -> Option<A> {
    NAMES.iter().position(|n| *n == key).and_then(|i| A::from(i as i32))
}

pub fn node_number(h: &Host, id: NodeId, a: A) -> Option<f64> {
    let n = h.scene.get(id)?;
    Some(match a {
        A::X => n.x,
        A::Y => n.y,
        A::Rotation => n.rotation,
        A::Scale | A::Sx => n.sx,
        A::Sy => n.sy,
        A::Vx => n.vx,
        A::Vy => n.vy,
        A::Vr => n.vr,
        A::Gravity => n.gravity,
        A::Alpha => n.alpha,
        A::Color => return Some((n.color >> 8) as f64),
        A::Fill => return n.fill.map(|c| (c >> 8) as f64),
        A::Order => n.order,
        A::W if n.kind == Kind::Shape => n.w,
        A::H if n.kind == Kind::Shape => n.h,
        A::Radius => n.radius,
        A::AnchorX => n.anchor_x,
        A::AnchorY => n.anchor_y,
        A::Corner => n.corner,
        A::Line => n.line,
        A::Size => n.size,
        A::Width => n.width,
        A::Parallax => n.parallax,
        _ => return None,
    } as f64)
}

fn set_node_number(h: &mut Host, id: NodeId, a: A, v: f64) {
    let Some(n) = h.scene.node_mut(id) else { return };
    let f = v as f32;
    match a {
        A::X => n.x = f,
        A::Y => n.y = f,
        A::Rotation => n.rotation = f,
        A::Scale => (n.sx, n.sy) = (f, f),
        A::Sx => n.sx = f,
        A::Sy => n.sy = f,
        A::Vx => n.vx = f,
        A::Vy => n.vy = f,
        A::Vr => n.vr = f,
        A::Gravity => n.gravity = f,
        A::Alpha => n.alpha = f.clamp(0.0, 1.0),
        A::Color => n.color = ((v as u32) << 8) | (n.color & 0xff),
        A::Fill => n.fill = Some(((v as u32) << 8) | 0xff),
        A::Order => n.order = f,
        A::W => n.w = f.max(0.0),
        A::H => n.h = f.max(0.0),
        A::Radius => n.radius = f.max(0.0),
        A::AnchorX => n.anchor_x = f,
        A::AnchorY => n.anchor_y = f,
        A::Corner => n.corner = f.max(0.0),
        A::Line => n.line = f.max(0.0),
        A::Size => n.size = f.clamp(1.0, 512.0),
        A::Width => n.width = f.max(0.0),
        A::Parallax => n.parallax = f,
        _ => {}
    }
}

/// What tweens read and write, for libs.rs.
struct HostValues<'a> {
    L: *mut lua_State,
    h: &'a mut Host,
}

impl Values for HostValues<'_> {
    fn get(&mut self, t: Target, key: &str) -> Option<f64> {
        match t {
            Target::Node(id) => node_number(self.h, id, node_atom(key)?),
            #[cfg(three)]
            Target::Node3(id) => super::three::number(self.h.three.as_deref()?, id, node_atom(key)?),
            #[cfg(not(three))]
            Target::Node3(_) => None,
            Target::Camera => {
                let c = &self.h.scene.camera;
                Some(match key {
                    "x" => c.x,
                    "y" => c.y,
                    "scale" => c.scale,
                    "rot" => c.rot,
                    _ => return None,
                } as f64)
            }
            Target::Bus(b) => super::sound::bus_number(self.h, b as usize, key),
            Target::Table(r, _) => unsafe {
                let k = std::ffi::CString::new(key).ok()?;
                lua_rawgeti(self.L, LUA_REGISTRYINDEX, r);
                lua_rawgetfield(self.L, -1, k.as_ptr());
                let v = num(self.L, -1);
                lua_pop(self.L, 2);
                v
            },
        }
    }

    fn set(&mut self, t: Target, key: &str, v: f64) {
        match t {
            Target::Node(id) => {
                if let Some(a) = node_atom(key) {
                    set_node_number(self.h, id, a, v);
                }
            }
            #[cfg(three)]
            Target::Node3(id) => {
                if let (Some(a), Some(w)) = (node_atom(key), self.h.three.as_deref_mut()) {
                    super::three::set_number(w, id, a, v as f32);
                }
            }
            #[cfg(not(three))]
            Target::Node3(_) => {}
            Target::Camera => {
                let c = &mut self.h.scene.camera;
                match key {
                    "x" => c.x = v as f32,
                    "y" => c.y = v as f32,
                    "scale" => c.scale = (v as f32).max(1e-3),
                    "rot" => c.rot = v as f32,
                    _ => {}
                }
            }
            Target::Bus(b) => super::sound::set_bus_number(self.h, b as usize, key, v),
            Target::Table(r, _) => unsafe {
                let Ok(k) = std::ffi::CString::new(key) else { return };
                lua_rawgeti(self.L, LUA_REGISTRYINDEX, r);
                lua_pushnumber(self.L, v);
                lua_rawsetfield(self.L, -2, k.as_ptr());
                lua_pop(self.L, 1);
            },
        }
    }

    fn is_color(&self, t: Target, key: &str) -> bool {
        matches!(t, Target::Node(_)) && matches!(key, "color" | "fill")
    }
}

b!(
    /// `__engine.update(dt)`: the libraries' frame. Returns the calls to make
    /// in Lua: `{ f, timer or false, args… , n = count }`, or `{ "signal",
    /// node, name, args… }`.
    update, |L, h| {
    let dt = opt(L, 1, 0.0);
    // Lines, dialogues and a cutscene move on first; what finished runs with the rest.
    super::voice::Voice::frame(h);
    let mut libs = std::mem::take(&mut h.libs);
    let due = {
        let mut values = HostValues { L, h };
        let mut frames = Vec::new();
        let due = libs.update(dt, &mut values, &mut |node, f| {
            frames.push((node, f));
            true
        });
        for (node, f) in frames {
            if let Some(n) = values.h.scene.node_mut(node) {
                n.frame = f;
            }
        }
        // Sprites destroyed since stop playing.
        libs.sprites.retain(|s| values.h.scene.alive(s.node));
        due
    };
    lua_createtable(L, due.len() as c_int, 0);
    for (i, d) in due.iter().enumerate() {
        lua_createtable(L, 6, 1);
        let n = match d {
            Due::Call { f, args, pass_self, timer } => {
                lua_rawgeti(L, LUA_REGISTRYINDEX, *f);
                lua_rawseti(L, -2, 1);
                match timer {
                    Some(t) => lua_pushnumber(L, *t as f64),
                    None => lua_pushboolean(L, 0),
                }
                lua_rawseti(L, -2, 2);
                let mut k = 2;
                if *pass_self {
                    k += 1;
                    lua_rawgeti(L, LUA_REGISTRYINDEX, *f);
                    lua_rawseti(L, -2, k);
                }
                for a in args {
                    k += 1;
                    lua_pushnumber(L, *a);
                    lua_rawseti(L, -2, k);
                }
                k
            }
            Due::Loop { f, anim, loops } => {
                lua_rawgeti(L, LUA_REGISTRYINDEX, *f);
                lua_rawseti(L, -2, 1);
                lua_pushboolean(L, 0);
                lua_rawseti(L, -2, 2);
                push_handle(L, tag::ANIM, *anim);
                lua_rawseti(L, -2, 3);
                lua_pushnumber(L, *loops);
                lua_rawseti(L, -2, 4);
                4
            }
            Due::Signal3 { node, name, args } => {
                push_str(L, "signal");
                lua_rawseti(L, -2, 1);
                #[cfg(three)]
                if let Some(w) = h.three.as_deref() {
                    super::three::push_node(L, w, *node);
                } else {
                    lua_pushnil(L);
                }
                #[cfg(not(three))]
                lua_pushnil(L);
                lua_rawseti(L, -2, 2);
                push_str(L, name);
                lua_rawseti(L, -2, 3);
                let mut k = 3;
                for a in args {
                    k += 1;
                    lua_pushnumber(L, *a);
                    lua_rawseti(L, -2, k);
                }
                k
            }
            Due::Signal3Node { node, name, other } => {
                push_str(L, "signal");
                lua_rawseti(L, -2, 1);
                #[cfg(three)]
                if let Some(w) = h.three.as_deref() {
                    super::three::push_node(L, w, *node);
                    lua_rawseti(L, -2, 2);
                    super::three::push_node(L, w, *other);
                    lua_rawseti(L, -2, 4);
                }
                #[cfg(not(three))]
                let _ = (node, other);
                push_str(L, name);
                lua_rawseti(L, -2, 3);
                4
            }
            Due::Signal { node, name, args } => {
                push_str(L, "signal");
                lua_rawseti(L, -2, 1);
                push_node(L, h, *node);
                lua_rawseti(L, -2, 2);
                push_str(L, name);
                lua_rawseti(L, -2, 3);
                let mut k = 3;
                for a in args {
                    k += 1;
                    lua_pushnumber(L, *a);
                    lua_rawseti(L, -2, k);
                }
                k
            }
        };
        lua_pushnumber(L, n as f64);
        lua_setfield(L, -2, c"n".as_ptr());
        lua_rawseti(L, -2, i as c_int + 1);
    }
    // The functions are in the list now: their refs can go.
    for r in libs.release.drain(..) {
        lua_unref(L, r);
    }
    h.libs = libs;
    1
});

/// The function at stack index `i`, kept (a registry ref).
unsafe fn keep_fn(L: *mut lua_State, i: c_int, f: &str) -> Result<c_int, c_int> {
    unsafe {
        if lua_type(L, i) != LUA_TFUNCTION {
            return Err(bad(L, i, f, "function"));
        }
        lua_pushvalue(L, i);
        let r = lua_ref(L, -1);
        lua_pop(L, 1);
        Ok(r)
    }
}

// ---------------------------------------------------------------- flux

/// What a tween or a spring moves, at stack index `at`: a node, the
/// camera, an audio bus, or a table (a registry ref of its own).
unsafe fn target_at(L: *mut lua_State, h: &Host, at: c_int, f: &str) -> Result<Target, String> {
    unsafe {
        Ok(match lua_type(L, at) {
            LUA_TUSERDATA if lua_userdatatag(L, at) == tag::CAMERA => Target::Camera,
            LUA_TUSERDATA if lua_userdatatag(L, at) == super::sound::BUS_TAG => Target::Bus(super::sound::bus_at(L, at).unwrap_or(0) as u8),
            #[cfg(three)]
            LUA_TUSERDATA if lua_userdatatag(L, at) == tag::NODE3 => match super::three::node_at(L, at) {
                Some(id) if h.three.as_ref().is_some_and(|w| w.scene.alive(id)) => Target::Node3(id),
                _ => return Err(format!("{f}: that node was destroyed")),
            },
            LUA_TUSERDATA => match node_at(L, at) {
                Some(id) if h.scene.alive(id) => Target::Node(id),
                Some(_) => return Err(format!("{f}: that node was destroyed")),
                None => return Err(format!("{f} moves a node, the camera, a bus or a table, got {}", type_name(L, at))),
            },
            LUA_TTABLE => {
                lua_pushvalue(L, at);
                let r = lua_ref(L, -1);
                lua_pop(L, 1);
                Target::Table(r, lua_topointer(L, at) as usize)
            }
            _ => return Err(format!("{f} moves a node, the camera, a bus or a table, got {}", type_name(L, at))),
        })
    }
}

/// "a Sprite has no number `foo` to move", by what the target is.
fn no_number(h: &Host, target: Target, f: &str, key: &str) -> String {
    match target {
        Target::Node(id) => format!("{f}: a {} has no number `{key}` to move", h.scene.get(id).map_or("node", |n| n.kind.name())),
        Target::Node3(_) => format!("{f}: a 3D node has no number `{key}` to move (x, y, z, rx, ry, rz, scale, sx, sy, sz, vx, vy, vz, gravity, vrx, vry, vrz)"),
        Target::Camera => format!("{f}: the camera moves x, y, scale and rot, not `{key}`"),
        Target::Bus(_) => format!("{f}: a bus moves volume, lowpass and highpass, not `{key}`"),
        Target::Table(..) => format!("{f}: bad value for key '{key}' (the table's `{key}` isn't a number)"),
    }
}

/// A tween's target and its variables, from `obj, time, vars` at `at`.
unsafe fn new_tween(L: *mut lua_State, h: &mut Host, at: c_int, f: &str) -> Result<Tween, String> {
    unsafe {
        let target = target_at(L, h, at, f)?;
        let time = num(L, at + 1).ok_or_else(|| format!("{f}: the time is a number of seconds, got {}", type_name(L, at + 1)))?;
        if lua_type(L, at + 2) != LUA_TTABLE {
            return Err(format!("{f}: the third argument is the values to move to, {{ x = 100 }}"));
        }
        let mut vars = Vec::new();
        let mut values = HostValues { L, h };
        lua_pushnil(L);
        while lua_next(L, at + 2) != 0 {
            let key = to_s(L, -2);
            let end = match lua_type(L, -1) {
                LUA_TNUMBER => lua_tonumber(L, -1),
                LUA_TTABLE if values.is_color(target, &key) => match color_at(L, -1, false) {
                    Ok(c) => (c >> 8) as f64,
                    Err(e) => {
                        lua_pop(L, 2);
                        return Err(format!("{f}: {key}: {e}"));
                    }
                },
                _ => {
                    let t = type_name(L, -1);
                    lua_pop(L, 2);
                    return Err(format!("{f}: bad value for key '{key}' (a number, got {t})"));
                }
            };
            if values.get(target, &key).is_none() {
                lua_pop(L, 2);
                return Err(no_number(values.h, target, f, &key));
            }
            vars.push(Var { key, end, start: 0.0, color: false });
            lua_pop(L, 1);
        }
        vars.sort_by(|a, b| a.key.cmp(&b.key));
        let id = h.libs.new_id();
        Ok(Tween {
            id,
            target,
            vars,
            rate: if time > 0.0 { 1.0 / time } else { 0.0 },
            progress: if time > 0.0 { 0.0 } else { 1.0 },
            delay: 0.0,
            ease: "quadout".into(),
            inited: false,
            onstart: Vec::new(),
            onupdate: Vec::new(),
            oncomplete: Vec::new(),
            then: Vec::new(),
            yoyo: false,
        })
    }
}

// `spring(target, { key = goal, … }, { frequency = 4, damping = 0.5 })`:
// each key heads for its goal on a spring (the SDK's `motion::Spring`),
// moved in Rust by the world's dt after flux; calling it again retargets
// from where it is, at its speed. A write to the key stops it (§4.3).
b!(spring, |L, h| {
    let f = "spring";
    if lua_type(L, 2) != LUA_TTABLE {
        return err(L, "spring(target, { x = 100 }, { frequency = 4, damping = 0.5 }): the second argument is the values to head for");
    }
    let mut frequency = 4.0f32;
    let mut damping = 0.5f32;
    if lua_type(L, 3) == LUA_TTABLE {
        for (k, d) in [(c"frequency", &mut frequency), (c"damping", &mut damping)] {
            lua_rawgetfield(L, 3, k.as_ptr());
            match lua_type(L, -1) {
                LUA_TNIL => {}
                LUA_TNUMBER if lua_tonumber(L, -1).is_finite() => *d = lua_tonumber(L, -1) as f32,
                _ => {
                    let t = to_s(L, -1);
                    return err(L, &format!("spring: {} takes a number, got {t}", k.to_str().unwrap_or("")));
                }
            }
            lua_pop(L, 1);
        }
    } else if lua_type(L, 3) != LUA_TNIL && lua_gettop(L) >= 3 {
        return err(L, "spring: the third argument is { frequency = 4, damping = 0.5 } (how many swings a second; 1 doesn't overshoot, under 1 bounces)");
    }
    let (frequency, damping) = (frequency.clamp(0.05, 30.0), damping.clamp(0.0, 4.0));
    // Each key in order, so a replay makes them in the same order.
    let mut goals = Vec::new();
    lua_pushnil(L);
    while lua_next(L, 2) != 0 {
        let key = to_s(L, -2);
        let Some(v) = num(L, -1).filter(|v| v.is_finite()) else {
            let t = type_name(L, -1);
            lua_pop(L, 2);
            return err(L, &format!("{f}: bad value for key '{key}' (a number, got {t})"));
        };
        goals.push((key, v));
        lua_pop(L, 1);
    }
    goals.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, goal) in goals {
        let target = match target_at(L, h, 1, f) {
            Ok(t) => t,
            Err(m) => return err(L, &m),
        };
        if (HostValues { L, h }).is_color(target, &key) {
            return err(L, &format!("{f}: `{key}` is a colour: tween it (flux.to)"));
        }
        let mut libs = std::mem::take(&mut h.libs);
        let ok = libs.spring(&mut HostValues { L, h }, target, &key, goal, frequency, damping);
        h.libs = libs;
        if !ok {
            return err(L, &no_number(h, target, f, &key));
        }
    }
    0
});

/// One frame of `fx.punch` (vm.rs, after Lua's frame), on real time.
pub fn punch_step(L: *mut lua_State, h: &mut Host, dt: f32) {
    if h.libs.punches.is_empty() {
        return;
    }
    let mut libs = std::mem::take(&mut h.libs);
    libs.step_punches(dt, &mut HostValues { L, h }, |t| matches!(t, Target::Node3(_)));
    h.libs = libs;
}

b!(flux_to, |L, h| match new_tween(L, h, 1, "flux.to") {
    Ok(t) => {
        let id = t.id;
        h.libs.tweens.push(t);
        push_handle(L, tag::TWEEN, id)
    }
    Err(m) => err(L, &m),
});

b!(tween_call, |L, h| {
    let Some(id) = handle(L, 1, tag::TWEEN) else { return err(L, "not a tween") };
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    match name.as_str() {
        "ease" => {
            let Some(e) = st(L, 2) else { return bad(L, 2, "ease", "easing name") };
            if flux_ease(e, 0.5).is_none() {
                return err(L, &format!("bad easing type '{e}' (flux's: linear, quadin, quadout, quadinout, cubic…, quart…, quint…, expo…, sine…, circ…, back…, elastic…; or easings.net's: outBack, inOutSine…)"));
            }
            if let Some(t) = h.libs.tween_mut(id) {
                t.ease = e.to_string();
            }
        }
        "delay" => {
            let Some(d) = num(L, 2) else { return bad(L, 2, "delay", "number") };
            if let Some(t) = h.libs.tween_mut(id) {
                t.delay = d;
            }
        }
        "onstart" | "onupdate" | "oncomplete" => {
            let r = match keep_fn(L, 2, &name) {
                Ok(r) => r,
                Err(e) => return e,
            };
            match h.libs.tween_mut(id) {
                Some(t) => match name.as_str() {
                    "onstart" => t.onstart.push(r),
                    "onupdate" => t.onupdate.push(r),
                    _ => t.oncomplete.push(r),
                },
                None => {
                    lua_unref(L, r);
                }
            }
        }
        "yoyo" => {
            if let Some(t) = h.libs.tween_mut(id) {
                t.yoyo = true;
            }
        }
        "stop" => h.libs.stop(id),
        "after" => {
            // after(obj, time, vars) or after(time, vars) on the same object.
            let same = lua_type(L, 2) == LUA_TNUMBER;
            let next = if same {
                let Some(target) = h.libs.tween_mut(id).map(|t| t.target) else { return err(L, "after: this tween has ended") };
                // Put the object before the time, as flux's tween.new takes them.
                match target {
                    Target::Node(n) => push_node(L, h, n),
                    #[cfg(three)]
                    Target::Node3(n) => super::three::push_node(L, h.three.as_deref().unwrap(), n),
                    #[cfg(not(three))]
                    Target::Node3(_) => lua_pushnil(L),
                    Target::Camera => {
                        lua_getfield(L, LUA_GLOBALSINDEX, c"camera".as_ptr());
                    }
                    Target::Table(r, _) => {
                        lua_rawgeti(L, LUA_REGISTRYINDEX, r);
                    }
                    Target::Bus(b) => {
                        push_handle(L, super::sound::BUS_TAG, b as u32);
                    }
                }
                lua_insert(L, 2);
                new_tween(L, h, 2, "after")
            } else {
                new_tween(L, h, 2, "after")
            };
            return match next {
                Ok(t) => {
                    let nid = t.id;
                    h.libs.waiting.push(t);
                    match h.libs.tween_mut(id) {
                        Some(t) => t.then.push(nid),
                        None => h.libs.stop(nid),
                    }
                    push_handle(L, tag::TWEEN, nid)
                }
                Err(m) => err(L, &m),
            };
        }
        _ => return err(L, &format!("no method `{name}` on a tween (ease, delay, onstart, onupdate, oncomplete, after, stop, yoyo)")),
    }
    lua_pushvalue(L, 1);
    1
});

// ---------------------------------------------------------------- hump.timer

fn new_timer(h: &mut Host, limit: f64, count: f64, during: Option<c_int>, after: Option<c_int>) -> u32 {
    let id = h.libs.new_id();
    h.libs.timers.push(Timer { id, time: 0.0, limit, count, during, after });
    id
}

b!(t_after, |L, h| {
    let Some(d) = num(L, 1) else { return bad(L, 1, "timer.after", "number (seconds)") };
    let f = match keep_fn(L, 2, "timer.after") {
        Ok(f) => f,
        Err(e) => return e,
    };
    let id = new_timer(h, d, 1.0, None, Some(f));
    push_handle(L, tag::TIMER, id)
});

b!(t_every, |L, h| {
    let Some(d) = num(L, 1) else { return bad(L, 1, "timer.every", "number (seconds)") };
    let f = match keep_fn(L, 2, "timer.every") {
        Ok(f) => f,
        Err(e) => return e,
    };
    let count = num(L, 3).unwrap_or(f64::INFINITY);
    let id = new_timer(h, d, count, None, Some(f));
    push_handle(L, tag::TIMER, id)
});

b!(t_during, |L, h| {
    let Some(d) = num(L, 1) else { return bad(L, 1, "timer.during", "number (seconds)") };
    let f = match keep_fn(L, 2, "timer.during") {
        Ok(f) => f,
        Err(e) => return e,
    };
    let after = if lua_type(L, 3) == LUA_TFUNCTION { keep_fn(L, 3, "timer.during").ok() } else { None };
    let id = new_timer(h, d, 1.0, Some(f), after);
    push_handle(L, tag::TIMER, id)
});

b!(t_cancel, |L, h| {
    if let Some(id) = handle(L, 1, tag::TIMER) {
        h.libs.cancel_timer(id);
    }
    0
});

b!(timer_cancel, |L, h| {
    if let Some(id) = num(L, 1) {
        h.libs.cancel_timer(id as u32);
    }
    0
});

b!(t_clear, |L, h| {
    let ids: Vec<u32> = h.libs.timers.iter().map(|t| t.id).collect();
    for id in ids {
        h.libs.cancel_timer(id);
    }
    0
});

// ---------------------------------------------------------------- anim8

/// anim8's intervals: a number, or `"1-3"` (backwards too).
unsafe fn interval(L: *mut lua_State, i: c_int) -> Option<(i64, i64)> {
    unsafe {
        match lua_type(L, i) {
            LUA_TNUMBER => {
                let n = lua_tonumber(L, i) as i64;
                Some((n, n))
            }
            LUA_TSTRING => {
                let s = st(L, i)?.replace(' ', "");
                let (a, b) = s.split_once('-').unwrap_or((&s, &s));
                Some((a.parse().ok()?, b.parse().ok()?))
            }
            _ => None,
        }
    }
}

fn span(a: i64, b: i64) -> Vec<i64> {
    if a <= b { (a..=b).collect() } else { (b..=a).rev().collect() }
}

b!(newGrid, |L, h| {
    let v: Vec<Option<f64>> = (1..=7).map(|i| num(L, i)).collect();
    let (Some(fw), Some(fh), Some(iw), Some(ih)) = (v[0], v[1], v[2], v[3]) else {
        return err(L, "anim8.newGrid(frameWidth, frameHeight, imageWidth, imageHeight, left, top, border): the first four are numbers");
    };
    if fw <= 0.0 || fh <= 0.0 {
        return err(L, "anim8.newGrid: the frame's width and height are above 0");
    }
    h.grids.push([fw as f32, fh as f32, iw as f32, ih as f32, v[4].unwrap_or(0.0) as f32, v[5].unwrap_or(0.0) as f32, v[6].unwrap_or(0.0) as f32]);
    push_handle(L, tag::GRID, h.grids.len() as u32 - 1)
});

/// grid(…)'s frames, as quads: pairs of column and row intervals.
unsafe fn grid_frames(L: *mut lua_State, h: &mut Host, from: c_int) -> c_int {
    unsafe {
        let Some(g) = handle(L, 1, tag::GRID).and_then(|g| h.grids.get(g as usize)).copied() else { return err(L, "not a grid") };
        let [fw, fh, iw, ih, left, top, border] = g;
        let n = lua_gettop(L);
        if n < from + 1 || (n - from + 1) % 2 != 0 {
            return err(L, "a grid takes columns and rows in pairs: grid(\"1-8\", 1)");
        }
        let cols = ((iw - left) / (fw + border)).floor().max(0.0) as i64;
        let rows = ((ih - top) / (fh + border)).floor().max(0.0) as i64;
        let mut quads = Vec::new();
        let mut i = from;
        while i < n {
            let (Some((x0, x1)), Some((y0, y1))) = (interval(L, i), interval(L, i + 1)) else {
                return err(L, "a grid's columns and rows are numbers or ranges like \"1-8\"");
            };
            for y in span(y0, y1) {
                for x in span(x0, x1) {
                    if x < 1 || y < 1 || x > cols || y > rows {
                        return err(L, &format!("There is no frame for x={x}, y={y} (the grid has {cols} columns and {rows} rows)"));
                    }
                    quads.push([left + (x - 1) as f32 * fw + x as f32 * border, top + (y - 1) as f32 * fh + y as f32 * border, fw, fh, iw, ih]);
                }
            }
            i += 2;
        }
        lua_createtable(L, quads.len() as c_int, 0);
        for (k, q) in quads.into_iter().enumerate() {
            push_value::<[f32; 6]>(L, tag::QUAD, q);
            lua_rawseti(L, -2, k as c_int + 1);
        }
        1
    }
}

b!(grid_call, |L, h| grid_frames(L, h, 2));
b!(grid_namecall, |L, h| {
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    match name.as_str() {
        "getFrames" => grid_frames(L, h, 2),
        _ => err(L, &format!("no method `{name}` on a grid (call it: grid(\"1-8\", 1), or grid:getFrames)")),
    }
});

/// anim8's durations: one number, or a table by frame (keys may be ranges).
unsafe fn durations(L: *mut lua_State, i: c_int, count: usize) -> Result<Vec<f64>, String> {
    unsafe {
        match lua_type(L, i) {
            LUA_TNUMBER => Ok(vec![lua_tonumber(L, i); count]),
            LUA_TTABLE => {
                let mut out = vec![None; count];
                lua_pushnil(L);
                while lua_next(L, i) != 0 {
                    let Some(d) = num(L, -1) else {
                        let v = to_s(L, -1);
                        lua_pop(L, 2);
                        return Err(format!("The value [{v}] is not a valid duration"));
                    };
                    let Some((a, b)) = interval(L, -2) else {
                        lua_pop(L, 2);
                        return Err("a duration's key is a frame number or a range like \"1-3\"".into());
                    };
                    for f in span(a, b) {
                        if f >= 1 && (f as usize) <= count {
                            out[f as usize - 1] = Some(d);
                        }
                    }
                    lua_pop(L, 1);
                }
                let n = out.iter().filter(|d| d.is_some()).count();
                out.into_iter().collect::<Option<Vec<f64>>>().ok_or_else(|| format!("The durations table has length of {n}, but it should be >= {count}"))
            }
            _ => Err("durations are a number (seconds a frame) or a table of them".into()),
        }
    }
}

b!(newAnimation, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "anim8.newAnimation", "table of frames");
    }
    let mut frames = Vec::new();
    for i in 1..=lua_objlen(L, 1) {
        lua_rawgeti(L, 1, i);
        let q = value_at::<[f32; 6]>(L, -1, tag::QUAD);
        lua_pop(L, 1);
        match q {
            Some(q) => frames.push([q[0], q[1], q[2], q[3]]),
            None => return err(L, "anim8.newAnimation: the frames are quads (a grid's)"),
        }
    }
    if frames.is_empty() {
        return err(L, "anim8.newAnimation: no frames");
    }
    let d = match durations(L, 2, frames.len()) {
        Ok(d) => d,
        Err(m) => return err(L, &m),
    };
    let on_loop = match lua_type(L, 3) {
        LUA_TFUNCTION => match keep_fn(L, 3, "newAnimation") {
            Ok(f) => OnLoop::Function(f),
            Err(e) => return e,
        },
        LUA_TSTRING => match st(L, 3) {
            Some("pauseAtEnd") => OnLoop::PauseAtEnd,
            Some("pauseAtStart") => OnLoop::PauseAtStart,
            Some(o) => return err(L, &format!("newAnimation: onLoop \"{o}\" (a function, \"pauseAtEnd\" or \"pauseAtStart\")")),
            None => OnLoop::Nothing,
        },
        _ => OnLoop::Nothing,
    };
    if h.libs.anims.len() >= 4096 {
        return err(L, "more than 4096 animations in a round: clone fewer");
    }
    h.libs.anims.push(Anim::new(frames, d, on_loop));
    push_handle(L, tag::ANIM, h.libs.anims.len() as u32 - 1)
});

b!(anim_index, |L, h| {
    let Some(a) = handle(L, 1, tag::ANIM).and_then(|i| h.libs.anims.get(i as usize)) else { return err(L, "not an animation") };
    match st(L, 2).unwrap_or("") {
        "position" => ret(L, a.position as f64),
        "timer" => ret(L, a.timer),
        "status" => rets(L, if a.playing { "playing" } else { "paused" }),
        "flippedH" => retb(L, a.flipped_h),
        "flippedV" => retb(L, a.flipped_v),
        "totalDuration" => ret(L, a.total),
        "frames" => {
            lua_createtable(L, a.frames.len() as c_int, 0);
            for (i, q) in a.frames.iter().enumerate() {
                push_value::<[f32; 6]>(L, tag::QUAD, [q[0], q[1], q[2], q[3], 0.0, 0.0]);
                lua_rawseti(L, -2, i as c_int + 1);
            }
            1
        }
        "durations" => {
            lua_createtable(L, a.durations.len() as c_int, 0);
            for (i, d) in a.durations.iter().enumerate() {
                lua_pushnumber(L, *d);
                lua_rawseti(L, -2, i as c_int + 1);
            }
            1
        }
        k => err(L, &format!("an animation has no `{k}` (position, timer, status, flippedH, flippedV, frames, durations, totalDuration)")),
    }
});

b!(anim_namecall, |L, h| {
    let Some(i) = handle(L, 1, tag::ANIM).map(|i| i as usize).filter(|i| *i < h.libs.anims.len()) else { return err(L, "not an animation") };
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    let a = &mut h.libs.anims[i];
    match name.as_str() {
        "update" => {} // the engine updates animations each frame
        "draw" => {
            let Some(img) = handle(L, 2, tag::IMAGE) else { return bad(L, 2, "draw", "Image") };
            let q = a.frames[a.position - 1];
            let (x, y, r) = (opt(L, 3, 0.0) as f32, opt(L, 4, 0.0) as f32, opt(L, 5, 0.0) as f32);
            let mut sx = opt(L, 6, 1.0) as f32;
            let mut sy = opt(L, 7, sx as f64) as f32;
            let (mut ox, mut oy) = (opt(L, 8, 0.0) as f32, opt(L, 9, 0.0) as f32);
            if a.flipped_h {
                sx = -sx;
                ox = q[2] - ox;
            }
            if a.flipped_v {
                sy = -sy;
                oy = q[3] - oy;
            }
            let c = Cmd::Image { image: img as u16, frame: 0, quad: Some(q), x, y, r, sx, sy, ox, oy, color: h.color };
            if !h.draw.push(c) {
                return err(L, "love.draw drew too many things this frame");
            }
            return 0;
        }
        "gotoFrame" => {
            let n = opt(L, 2, 1.0) as usize;
            a.goto(n);
        }
        "pause" => a.playing = false,
        "resume" => a.playing = true,
        "pauseAtEnd" => a.pause_at_end(),
        "pauseAtStart" => a.pause_at_start(),
        "flipH" => a.flipped_h = !a.flipped_h,
        "flipV" => a.flipped_v = !a.flipped_v,
        "clone" => {
            let mut c = a.clone();
            c.flipped_h = false;
            c.flipped_v = false;
            // anim8's clone keeps onLoop; a function ref is shared.
            if let OnLoop::Function(f) = c.on_loop {
                lua_rawgeti(L, LUA_REGISTRYINDEX, f);
                c.on_loop = OnLoop::Function(lua_ref(L, -1));
                lua_pop(L, 1);
            }
            c.goto(1);
            c.playing = true;
            h.libs.anims.push(c);
            return push_handle(L, tag::ANIM, h.libs.anims.len() as u32 - 1);
        }
        "getDimensions" => {
            let q = a.frames[a.position - 1];
            return ret2(L, q[2] as f64, q[3] as f64);
        }
        _ => return err(L, &format!("no method `{name}` on an animation (draw, gotoFrame, pause, resume, pauseAtEnd, pauseAtStart, flipH, flipV, clone, getDimensions)")),
    }
    lua_pushvalue(L, 1);
    1
});

// ---------------------------------------------------------------- bump

b!(newWorld, |L, h| {
    let cell = opt(L, 1, 64.0);
    if h.worlds.iter().any(|w| w.busy) {
        return err(L, "bump.newWorld inside a bump filter: make worlds outside move, check and query");
    }
    if h.worlds.len() >= 64 {
        return err(L, "bump.newWorld: more than 64 worlds in a round");
    }
    let mut refs = [0; 2];
    for r in &mut refs {
        lua_newtable(L);
        *r = lua_ref(L, -1);
        lua_pop(L, 1);
    }
    h.worlds.push(BumpWorld { world: bump::World::new(cell), items_ref: refs[0], ids_ref: refs[1], next: 0, busy: false });
    push_handle(L, tag::WORLD, h.worlds.len() as u32 - 1)
});

/// The id of the Lua value at `i` in world `w`, if it's an item there.
unsafe fn item_id(L: *mut lua_State, w: &BumpWorld, i: c_int) -> Option<u32> {
    unsafe {
        if lua_type(L, i) == LUA_TNIL {
            return None;
        }
        lua_rawgeti(L, LUA_REGISTRYINDEX, w.ids_ref);
        lua_pushvalue(L, i);
        lua_rawget(L, -2);
        let id = num(L, -1).map(|n| n as u32);
        lua_pop(L, 2);
        id
    }
}

unsafe fn push_item(L: *mut lua_State, items_ref: c_int, id: u32) {
    unsafe {
        lua_rawgeti(L, LUA_REGISTRYINDEX, items_ref);
        lua_rawgeti(L, -1, id as c_int);
        lua_remove(L, -2);
    }
}

unsafe fn rect_args(L: *mut lua_State, at: c_int, f: &str) -> Result<Rect, String> {
    unsafe {
        let v: Vec<Option<f64>> = (at..at + 4).map(|i| num(L, i)).collect();
        match (v[0], v[1], v[2], v[3]) {
            (Some(x), Some(y), Some(w), Some(hh)) if w > 0.0 && hh > 0.0 => Ok(Rect { x, y, w, h: hh }),
            (Some(_), Some(_), Some(_), Some(_)) => Err(format!("{f}: an item's width and height are above 0")),
            _ => Err(format!("{f} takes x, y, w, h (numbers)")),
        }
    }
}

/// Calls a Lua filter at stack index `fi`: `filter(item[, other])`. Its
/// error is kept in `error` and the answer taken as none.
unsafe fn call_filter(L: *mut lua_State, items_ref: c_int, fi: c_int, item: Option<u32>, other: u32, error: &mut Option<String>) -> Option<String> {
    unsafe {
        if error.is_some() {
            return None;
        }
        lua_pushvalue(L, fi);
        let mut n = 0;
        if let Some(i) = item {
            push_item(L, items_ref, i);
            n += 1;
        }
        push_item(L, items_ref, other);
        n += 1;
        if lua_pcall(L, n, 1, 0) != LUA_OK {
            *error = Some(to_s(L, -1));
            lua_pop(L, 1);
            return None;
        }
        let r = match lua_type(L, -1) {
            LUA_TSTRING => Some(to_s(L, -1)),
            LUA_TBOOLEAN if lua_toboolean(L, -1) != 0 => Some(String::new()),
            _ => None,
        };
        lua_pop(L, 1);
        r
    }
}

unsafe fn push_xy(L: *mut lua_State, k: &std::ffi::CStr, (x, y): (f64, f64)) {
    unsafe {
        lua_createtable(L, 0, 2);
        lua_pushnumber(L, x);
        lua_setfield(L, -2, c"x".as_ptr());
        lua_pushnumber(L, y);
        lua_setfield(L, -2, c"y".as_ptr());
        lua_setfield(L, -2, k.as_ptr());
    }
}

unsafe fn push_rect(L: *mut lua_State, k: &std::ffi::CStr, r: Rect) {
    unsafe {
        lua_createtable(L, 0, 4);
        for (n, v) in [(c"x", r.x), (c"y", r.y), (c"w", r.w), (c"h", r.h)] {
            lua_pushnumber(L, v);
            lua_setfield(L, -2, n.as_ptr());
        }
        lua_setfield(L, -2, k.as_ptr());
    }
}

/// bump's `cols` table.
unsafe fn push_cols(L: *mut lua_State, items_ref: c_int, cols: &[bump::Collision]) {
    unsafe {
        lua_createtable(L, cols.len() as c_int, 0);
        for (i, c) in cols.iter().enumerate() {
            lua_createtable(L, 0, 12);
            push_item(L, items_ref, c.item);
            lua_setfield(L, -2, c"item".as_ptr());
            push_item(L, items_ref, c.other);
            lua_setfield(L, -2, c"other".as_ptr());
            push_str(L, c.response.name());
            lua_setfield(L, -2, c"type".as_ptr());
            lua_pushboolean(L, c.overlaps as c_int);
            lua_setfield(L, -2, c"overlaps".as_ptr());
            lua_pushnumber(L, c.ti);
            lua_setfield(L, -2, c"ti".as_ptr());
            push_xy(L, c"move", c.moved);
            push_xy(L, c"normal", c.normal);
            push_xy(L, c"touch", c.touch);
            push_rect(L, c"itemRect", c.item_rect);
            push_rect(L, c"otherRect", c.other_rect);
            if let Some(s) = c.slide {
                push_xy(L, c"slide", s);
            }
            if let Some(b) = c.bounce {
                push_xy(L, c"bounce", b);
            }
            lua_rawseti(L, -2, i as c_int + 1);
        }
    }
}

/// A filter's answer as a response (bump's default is slide).
fn response(r: Option<String>) -> Result<Option<Response>, String> {
    match r.as_deref() {
        None => Ok(None),
        Some("") => Ok(Some(Response::Slide)),
        Some(s) => Response::named(s).map(Some).ok_or_else(|| format!("Unknown collision type: {s} (slide, touch, cross or bounce)")),
    }
}

b!(world_call, |L, h| {
    let Some(wi) = handle(L, 1, tag::WORLD).map(|w| w as usize).filter(|w| *w < h.worlds.len()) else { return err(L, "not a bump world") };
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    if h.worlds[wi].busy {
        return err(L, &format!("world:{name} inside this world's own filter: a filter only answers (\"slide\", \"cross\", …); change the world after move, check or query returns"));
    }
    let w = &mut h.worlds[wi];
    match name.as_str() {
        "add" => {
            if lua_type(L, 2) == LUA_TNIL {
                return err(L, "world:add: the item can't be nil");
            }
            if item_id(L, w, 2).is_some() {
                return err(L, "Item already added to the world: remove it first, or use world:update");
            }
            let r = match rect_args(L, 3, "world:add") {
                Ok(r) => r,
                Err(m) => return err(L, &m),
            };
            w.next += 1;
            let id = w.next;
            lua_rawgeti(L, LUA_REGISTRYINDEX, w.ids_ref);
            lua_pushvalue(L, 2);
            lua_pushnumber(L, id as f64);
            lua_rawset(L, -3);
            lua_pop(L, 1);
            lua_rawgeti(L, LUA_REGISTRYINDEX, w.items_ref);
            lua_pushvalue(L, 2);
            lua_rawseti(L, -2, id as c_int);
            lua_pop(L, 1);
            let _ = w.world.add(id, r);
            lua_pushvalue(L, 2);
            1
        }
        "remove" => {
            let Some(id) = item_id(L, w, 2) else { return err(L, "Item must be added to the world before being removed") };
            let _ = w.world.remove(id);
            lua_rawgeti(L, LUA_REGISTRYINDEX, w.ids_ref);
            lua_pushvalue(L, 2);
            lua_pushnil(L);
            lua_rawset(L, -3);
            lua_pop(L, 1);
            lua_rawgeti(L, LUA_REGISTRYINDEX, w.items_ref);
            lua_pushnil(L);
            lua_rawseti(L, -2, id as c_int);
            lua_pop(L, 1);
            0
        }
        "update" => {
            let Some(id) = item_id(L, w, 2) else { return err(L, "Item must be added to the world before being updated") };
            let old = w.world.rect(id).unwrap();
            let (Some(x), Some(y)) = (num(L, 3), num(L, 4)) else { return err(L, "world:update takes the item, x, y[, w, h]") };
            let r = Rect { x, y, w: num(L, 5).unwrap_or(old.w), h: num(L, 6).unwrap_or(old.h) };
            let _ = w.world.update(id, r);
            0
        }
        "move" | "check" => {
            let Some(id) = item_id(L, w, 2) else { return err(L, &format!("Item must be added to the world before world:{name}")) };
            let (Some(gx), Some(gy)) = (num(L, 3), num(L, 4)) else { return err(L, &format!("world:{name} takes the item, goalX, goalY[, filter]")) };
            let has_filter = lua_type(L, 5) == LUA_TFUNCTION;
            let items_ref = w.items_ref;
            let mut error = None;
            let mut bad_type = None;
            let mut filter = |item: u32, other: u32| -> Option<Response> {
                let r = if has_filter { call_filter(L, items_ref, 5, Some(item), other, &mut error) } else { Some(String::new()) };
                match response(r) {
                    Ok(r) => r,
                    Err(m) => {
                        bad_type.get_or_insert(m);
                        None
                    }
                }
            };
            // The world leaves `h` while the filter runs Lua (nothing in `h` is
            // held across a call), and calls on it then are errors (`busy`).
            let mut world = std::mem::replace(&mut h.worlds[wi].world, bump::World::new(64.0));
            h.worlds[wi].busy = true;
            let result = if name == "move" { world.move_item(id, gx, gy, &mut filter) } else { world.check(id, gx, gy, &mut filter) };
            h.worlds[wi].world = world;
            h.worlds[wi].busy = false;
            if let Some(e) = error {
                push_str(L, &e);
                return -2;
            }
            if let Some(m) = bad_type {
                return err(L, &m);
            }
            let Ok((x, y, cols)) = result else { return err(L, "world: no such item") };
            lua_pushnumber(L, x);
            lua_pushnumber(L, y);
            push_cols(L, items_ref, &cols);
            lua_pushnumber(L, cols.len() as f64);
            4
        }
        "queryRect" | "queryPoint" => {
            let has_filter = lua_type(L, if name == "queryRect" { 6 } else { 4 }) == LUA_TFUNCTION;
            let fi = if name == "queryRect" { 6 } else { 4 };
            let mut error = None;
            let items_ref = w.items_ref;
            let at = if name == "queryRect" {
                match rect_args(L, 2, "world:queryRect") {
                    Ok(r) => Ok(r),
                    Err(m) => return err(L, &m),
                }
            } else {
                let (Some(x), Some(y)) = (num(L, 2), num(L, 3)) else { return err(L, "world:queryPoint takes x, y[, filter]") };
                Err((x, y))
            };
            // As in move: the world is out of `h` while the filter runs.
            let world = std::mem::replace(&mut h.worlds[wi].world, bump::World::new(64.0));
            h.worlds[wi].busy = true;
            let items = {
                let mut keep = |i: u32| !has_filter || call_filter(L, items_ref, fi, None, i, &mut error).is_some();
                match at {
                    Ok(r) => world.query_rect(r, &mut keep),
                    Err((x, y)) => world.query_point(x, y, &mut keep),
                }
            };
            h.worlds[wi].world = world;
            h.worlds[wi].busy = false;
            if let Some(e) = error {
                push_str(L, &e);
                return -2;
            }
            lua_createtable(L, items.len() as c_int, 0);
            for (k, i) in items.iter().enumerate() {
                push_item(L, items_ref, *i);
                lua_rawseti(L, -2, k as c_int + 1);
            }
            lua_pushnumber(L, items.len() as f64);
            2
        }
        "hasItem" => retb(L, item_id(L, w, 2).is_some()),
        "getRect" => {
            let Some(r) = item_id(L, w, 2).and_then(|id| w.world.rect(id)) else { return err(L, "Item must be added to the world before getting its rect") };
            for v in [r.x, r.y, r.w, r.h] {
                lua_pushnumber(L, v);
            }
            4
        }
        "countItems" => ret(L, w.world.len() as f64),
        "getItems" => {
            let ids: Vec<u32> = w.world.items().collect();
            lua_createtable(L, ids.len() as c_int, 0);
            for (k, i) in ids.iter().enumerate() {
                push_item(L, w.items_ref, *i);
                lua_rawseti(L, -2, k as c_int + 1);
            }
            lua_pushnumber(L, ids.len() as f64);
            2
        }
        "getCellSize" => ret(L, w.world.cell_size()),
        _ => err(L, &format!("no method `{name}` on a bump world (add, remove, update, move, check, queryRect, queryPoint, hasItem, getRect, countItems, getItems)")),
    }
});
