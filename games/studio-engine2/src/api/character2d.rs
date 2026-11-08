//! `CharacterBody2D` (docs/STUDIO-2.md §5 [physics]; §9.4 row 3b): Godot's
//! character controller in a `love.physics` world, as LÖVE has none. The
//! SDK's `physics2d::character` (Rapier's controller with Godot's
//! `move_and_slide` rules) in the same world as the world's bodies, so
//! they meet it and it stands on them.
//!
//! ```lua
//! hero = CharacterBody2D{ world = world, shape = { w = 20, h = 40 }, x = 100, y = 300 }
//! Walker{ image = "fox", height = 46, parent = hero, y = 20 }       -- drawn with it
//! -- love.update: hero.vy = hero.vy + 900 * dt; hero:moveAndSlide()
//! ```
//!
//! It's a node (a `Group`) whose `x`, `y` are its shape's centre in the
//! world (so it has no parent), and whose `vx`, `vy` only `moveAndSlide`
//! moves it by (Godot's rule), in LÖVE's pixels: it scales by the world's
//! meter as the world's bodies do. `fixture:setOneWay(true)` (ours) makes a
//! fixture a one-way platform for characters: they land on it from above
//! and pass up through it (`moveAndSlide(true)` drops down through it). It's
//! what LÖVE games do with `preSolve` and `setEnabled(false)`, which can't
//! work here (§9.4 "As built (Phase 3a)").

use std::ffi::c_int;

use maimbrain::physics2d::{self as p2, vec2};
use maimbrain::scene::{Kind, NodeId};

use super::atoms::A;
use super::node::push_node;
use super::*;

/// A character: its node, its world and the SDK's controller.
pub struct LChar {
    pub node: NodeId,
    pub world: u32,
    pub ch: p2::Character,
    /// Where it last put its node (LÖVE px; elsewhere: Lua moved it).
    pub last: (f32, f32),
}

const KEYS: &str = "world, shape, x, y, name, tag, floorMaxAngle, floorSnapLength, maxStepHeight, floorStopOnSlope, floorBlockOnWall, wallMinSlideAngle, slideOnCeiling";

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        if host(ud).project.three {
            return;
        }
        sv_push_rfn(L, make, ud, c"CharacterBody2D".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"CharacterBody2D".as_ptr());
    }
}

/// A setting (Godot's member) on the controller's settings.
unsafe fn setting(L: *mut lua_State, s: &mut p2::CharacterSettings, a: A, v: c_int, k: f32) -> Result<(), String> {
    unsafe {
        let key = a.name();
        let n = || -> Result<f32, String> {
            match num(L, v) {
                Some(x) if x.is_finite() => Ok(x as f32),
                _ => Err(format!("{key} takes a number, got {}", type_name(L, v))),
            }
        };
        let b = || -> Result<bool, String> {
            match lua_type(L, v) {
                LUA_TBOOLEAN => Ok(lua_toboolean(L, v) != 0),
                _ => Err(format!("{key} takes true or false, got {}", type_name(L, v))),
            }
        };
        match a {
            A::FloorMaxAngle => s.floor_max_angle = n()?.clamp(0.0, std::f32::consts::FRAC_PI_2),
            A::FloorSnapLength => s.floor_snap_length = n()?.max(0.0) * k,
            A::MaxStepHeight => s.max_step_height = n()?.max(0.0) * k,
            A::FloorStopOnSlope => s.floor_stop_on_slope = b()?,
            A::FloorBlockOnWall => s.floor_block_on_wall = b()?,
            A::WallMinSlideAngle => s.wall_min_slide_angle = n()?.clamp(0.0, std::f32::consts::PI),
            A::SlideOnCeiling => s.slide_on_ceiling = b()?,
            _ => return Err(format!("CharacterBody2D has no property `{key}`")),
        }
        Ok(())
    }
}

fn is_setting(a: A) -> bool {
    matches!(a, A::FloorMaxAngle | A::FloorSnapLength | A::MaxStepHeight | A::FloorStopOnSlope | A::FloorBlockOnWall | A::WallMinSlideAngle | A::SlideOnCeiling)
}

b!(make, |L, h| {
    let r = (|| -> Result<c_int, String> {
        if lua_type(L, 1) != LUA_TTABLE {
            return Err(format!("CharacterBody2D takes one table: CharacterBody2D{{ world = world, shape = {{ w = 20, h = 40 }}, x = 100, y = 300 }} (got {})", type_name(L, 1)));
        }
        lua_rawgetfield(L, 1, c"world".as_ptr());
        let p = h.phys2.get_or_insert_with(|| Box::new(super::lovephysics::Phys2::new()));
        let wi = match super::lovephysics::world_index(L, p, -1) {
            Some(w) => w,
            None => {
                lua_pop(L, 1);
                return Err("CharacterBody2D{…} needs world = a love.physics World (love.physics.newWorld)".into());
            }
        };
        lua_pop(L, 1);
        let k = p.worlds[wi as usize].k;
        // The shape: a box { w, h }, a circle { radius }, or a capsule { radius, h }.
        lua_rawgetfield(L, 1, c"shape".as_ptr());
        let shape = if lua_type(L, -1) == LUA_TTABLE {
            let f = |key: &std::ffi::CStr| -> Option<f32> {
                lua_rawgetfield(L, -1, key.as_ptr());
                let v = num(L, -1).filter(|v| v.is_finite() && *v > 0.0).map(|v| v as f32);
                lua_pop(L, 1);
                v
            };
            match (f(c"w"), f(c"h"), f(c"radius")) {
                (Some(w), Some(hh), None) => Some(p2::Shape::rect(w * k, hh * k)),
                (None, Some(hh), Some(r)) if hh >= 2.0 * r => Some(p2::Shape::capsule(r * k, hh * k)),
                (None, None, Some(r)) => Some(p2::Shape::circle(r * k)),
                _ => None,
            }
        } else {
            None
        };
        lua_pop(L, 1);
        let Some(shape) = shape else {
            return Err("CharacterBody2D{…}: shape is { w = 20, h = 40 } (a box), { radius = 12 } (a circle) or { radius = 10, h = 40 } (a capsule), in pixels".into());
        };
        let mut s = p2::CharacterSettings { floor_snap_length: 1.0 * k, ..Default::default() };
        let (mut x, mut y) = (0.0f32, 0.0f32);
        let (mut name, mut tagged) = (String::new(), String::new());
        lua_pushnil(L);
        while lua_next(L, 1) != 0 {
            let key = st(L, -2).map(str::to_string).unwrap_or_default();
            let r = match key.as_str() {
                "world" | "shape" => Ok(()),
                "x" | "y" => match num(L, -1).filter(|v| v.is_finite()) {
                    Some(v) if key == "x" => Ok(x = v as f32),
                    Some(v) => Ok(y = v as f32),
                    None => Err(format!("{key} takes a number")),
                },
                "name" | "tag" => match st(L, -1) {
                    Some(v) if key == "name" => Ok(name = v.to_string()),
                    Some(v) => Ok(tagged = v.to_string()),
                    None => Err(format!("{key} takes a string")),
                },
                _ => match super::atoms::NAMES.iter().position(|n| *n == key).and_then(|i| A::from(i as i32)).filter(|a| is_setting(*a)) {
                    Some(a) => setting(L, &mut s, a, -1, k),
                    None => Err(format!("no key `{key}` ({KEYS})")),
                },
            };
            lua_pop(L, 1);
            if let Err(m) = r {
                lua_pop(L, 1);
                return Err(format!("CharacterBody2D{{…}}: {m}"));
            }
        }
        let id = h.scene.add(Kind::Group, None).map_err(|_| "too many nodes".to_string())?;
        if let Some(n) = h.scene.node_mut(id) {
            (n.x, n.y, n.driven) = (x, y, true);
            n.name = name;
            n.tag = tagged;
        }
        let p = h.phys2.as_deref_mut().unwrap();
        let ch = p2::Character::new(&mut p.worlds[wi as usize].w, vec2(x * k, y * k), shape, s, id.to_bits() as u64);
        p.chars.push(LChar { node: id, world: wi, ch, last: (x, y) });
        push_node(L, h, id);
        Ok(1)
    })();
    match r {
        Ok(n) => n,
        Err(m) => err(L, &m),
    }
});

/// An f32 as the number it was written as (0.4, not 0.4000000059604645).
fn nice(x: f32) -> f64 {
    format!("{x}").parse().unwrap_or(x as f64)
}

fn find(h: &Host, id: NodeId) -> Option<usize> {
    h.phys2.as_deref()?.chars.iter().position(|c| c.node == id)
}

/// A character node's property (`node.floorMaxAngle`): Some when it is one.
pub unsafe fn index(L: *mut lua_State, h: &mut Host, id: NodeId, a: A) -> Option<c_int> {
    unsafe {
        let i = find(h, id)?;
        if !is_setting(a) {
            return None;
        }
        let p = h.phys2.as_deref().unwrap();
        let c = &p.chars[i];
        let k = p.worlds[c.world as usize].k;
        let s = &c.ch.settings;
        Some(match a {
            A::FloorMaxAngle => ret(L, nice(s.floor_max_angle)),
            A::FloorSnapLength => ret(L, nice(s.floor_snap_length / k)),
            A::MaxStepHeight => ret(L, nice(s.max_step_height / k)),
            A::FloorStopOnSlope => retb(L, s.floor_stop_on_slope),
            A::FloorBlockOnWall => retb(L, s.floor_block_on_wall),
            A::WallMinSlideAngle => ret(L, nice(s.wall_min_slide_angle)),
            _ => retb(L, s.slide_on_ceiling),
        })
    }
}

/// Writing a character node's setting: Some when it is one.
pub unsafe fn newindex(L: *mut lua_State, h: &mut Host, id: NodeId, a: A, v: c_int) -> Option<Result<(), String>> {
    unsafe {
        let i = find(h, id)?;
        if !is_setting(a) {
            return None;
        }
        let p = h.phys2.as_deref_mut().unwrap();
        let k = p.worlds[p.chars[i].world as usize].k;
        let mut s = p.chars[i].ch.settings;
        Some(setting(L, &mut s, a, v, k).map(|_| p.chars[i].ch.settings = s))
    }
}

/// A character node's method: Some when it is one.
pub unsafe fn call(L: *mut lua_State, h: &mut Host, id: NodeId, a: Option<A>) -> Option<c_int> {
    unsafe {
        let a = a?;
        if !matches!(
            a,
            A::MoveAndSlide
                | A::IsOnFloor
                | A::IsOnFloorOnly
                | A::IsOnWall
                | A::IsOnWallOnly
                | A::IsOnCeiling
                | A::GetFloorNormal
                | A::GetWallNormal
                | A::GetFloorAngle
                | A::GetRealVelocity
                | A::GetSlideCollisionCount
                | A::GetSlideCollision
        ) {
            return None;
        }
        let i = find(h, id)?;
        let dt = h.dt;
        if a == A::MoveAndSlide {
            let drop = lua_type(L, 2) == LUA_TBOOLEAN && lua_toboolean(L, 2) != 0;
            let (x, y, vx, vy) = match h.scene.get(id) {
                Some(n) => (n.x, n.y, n.vx, n.vy),
                None => return Some(err(L, "this CharacterBody2D was destroyed")),
            };
            let p = h.phys2.as_deref_mut().unwrap();
            let c = &mut p.chars[i];
            let lw = &mut p.worlds[c.world as usize];
            let k = lw.k;
            // Lua moved it: it starts from there.
            if (c.last.0 - x).abs() > 1e-3 || (c.last.1 - y).abs() > 1e-3 {
                c.ch.set_position(&mut lw.w, vec2(x * k, y * k));
            }
            let v = c.ch.move_and_slide(&mut lw.w, vec2(vx * k, vy * k), dt, drop);
            let pos = c.ch.position();
            c.last = (pos.x / k, pos.y / k);
            let hit = !c.ch.hits().is_empty();
            let last = c.last;
            if let Some(n) = h.scene.node_mut(id) {
                (n.x, n.y, n.vx, n.vy) = (last.0, last.1, v.x / k, v.y / k);
            }
            return Some(retb(L, hit));
        }
        let p = h.phys2.as_deref().unwrap();
        let c = &p.chars[i];
        let k = p.worlds[c.world as usize].k;
        let ch = &c.ch;
        let v2 = |L: *mut lua_State, v: p2::Vec2| {
            lua_pushnumber(L, nice(v.x));
            lua_pushnumber(L, nice(v.y));
            2
        };
        Some(match a {
            A::IsOnFloor => retb(L, ch.on_floor()),
            A::IsOnFloorOnly => retb(L, ch.on_floor() && !ch.on_wall() && !ch.on_ceiling()),
            A::IsOnWall => retb(L, ch.on_wall()),
            A::IsOnWallOnly => retb(L, ch.on_wall() && !ch.on_floor() && !ch.on_ceiling()),
            A::IsOnCeiling => retb(L, ch.on_ceiling()),
            A::GetFloorNormal => v2(L, ch.floor_normal()),
            A::GetWallNormal => v2(L, ch.wall_normal()),
            A::GetFloorAngle => ret(L, nice(ch.floor_angle())),
            A::GetRealVelocity => v2(L, ch.real_velocity() / k),
            A::GetSlideCollisionCount => ret(L, ch.hits().len() as f64),
            _ => {
                let n = ch.hits().len();
                let i = match num(L, 2) {
                    Some(i) if i >= 1.0 && (i as usize) <= n => i as usize - 1,
                    _ => return Some(err(L, &format!("getSlideCollision(i): i is 1 to getSlideCollisionCount() ({n})"))),
                };
                let hit = ch.hits()[i];
                lua_createtable(L, 0, 2);
                lua_pushnumber(L, nice(hit.normal.x));
                lua_setfield(L, -2, c"nx".as_ptr());
                lua_pushnumber(L, nice(hit.normal.y));
                lua_setfield(L, -2, c"ny".as_ptr());
                1
            }
        })
    }
}

/// Characters whose node went go too.
pub fn forget(h: &mut Host) {
    let Some(p) = h.phys2.as_deref_mut() else { return };
    let dead: Vec<usize> = (0..p.chars.len()).filter(|i| !h.scene.alive(p.chars[*i].node)).collect();
    for i in dead.into_iter().rev() {
        let c = p.chars.remove(i);
        let w = c.world as usize;
        c.ch.remove(&mut p.worlds[w].w);
    }
}
