//! Godot's navigation nodes (docs/STUDIO-2.md §5 [godot], §4.14; §9.4 row
//! 3b; Decided 45–47), made through bodies3d.rs's constructors:
//!
//! ```lua
//! local region = NavigationRegion3D{ bake = { agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 } }
//! region.bakeFinished:connect(function() end)      -- baked behind the title card
//! local goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 20, y = 8, z = 20 }
//! local agent = NavigationAgent3D{ parent = goblin, maxSpeed = 3.2, autoMove = true }
//! agent:setTargetPosition(knight.x, knight.y, knight.z)
//! agent.targetReached:connect(function() end)
//! local path = region:path(x1, y1, z1, x2, y2, z2)  -- { x, y, z, x, y, z, … } or nil
//! ```
//!
//! What runs each frame is nav3.rs.

use std::ffi::c_int;

use maimbrain::gfx3d::{Vec3, vec3};
use maimbrain::nav::Settings;
use maimbrain::scene::three::Node3Id;

use super::atoms::A;
use super::*;
use crate::nav3::{Agent3, Region3};
use crate::physics3::trs_of;
use crate::three::{Kind3, World3};

pub fn has(kind: Kind3, a: A) -> bool {
    use A::*;
    match kind {
        Kind3::NavRegion => matches!(a, Size | Baked | Progress | Bake | Path | BakeFinished),
        Kind3::NavAgent => matches!(
            a,
            MaxSpeed
                | MaxAcceleration
                | Radius
                | Height
                | PathDesiredDistance
                | TargetDesiredDistance
                | Avoidance
                | AutoMove
                | SetTargetPosition
                | GetNextPathPosition
                | IsNavigationFinished
                | IsTargetReached
                | IsTargetReachable
                | DistanceToTarget
                | GetFinalPosition
                | TargetReached
                | NavigationFinished
        ),
        _ => false,
    }
}

pub fn is_method(a: A) -> bool {
    use A::*;
    matches!(a, Bake | Path | SetTargetPosition | GetNextPathPosition | IsNavigationFinished | IsTargetReached | IsTargetReachable | DistanceToTarget | GetFinalPosition)
}

const BAKE_KEYS: &str = "agentRadius, agentHeight, maxSlope, maxClimb, cellSize, cellHeight";

/// A navigation node is made: its region (with the `bake` table at `at`) or agent.
pub unsafe fn made(L: *mut lua_State, w: &mut World3, id: Node3Id, kind: Kind3, at: c_int) -> Result<(), String> {
    unsafe {
        let k = id.to_bits();
        if kind == Kind3::NavAgent {
            if lua_type(L, at) != LUA_TNIL {
                return Err("bake is NavigationRegion3D's".into());
            }
            w.nav().agents.insert(k, Agent3::new());
            return Ok(());
        }
        let mut s = Settings::default();
        match lua_type(L, at) {
            LUA_TNIL => {}
            LUA_TTABLE => {
                lua_pushnil(L);
                while lua_next(L, at) != 0 {
                    let key = st(L, -2).map(str::to_string).unwrap_or_default();
                    let v = num(L, -1).filter(|v| v.is_finite()).map(|v| v as f32);
                    lua_pop(L, 1);
                    let Some(v) = v else {
                        lua_pop(L, 1);
                        return Err(format!("bake.{key} takes a number"));
                    };
                    let pos = |v: f32| if v > 0.0 { Ok(v) } else { Err(format!("bake.{key} is more than 0")) };
                    let r = match key.as_str() {
                        "agentRadius" => pos(v).map(|v| s.agent_radius = v),
                        "agentHeight" => pos(v).map(|v| s.agent_height = v),
                        "maxSlope" => pos(v).map(|v| s.max_slope = v.min(1.5)),
                        "maxClimb" => Ok(s.max_climb = v.max(0.0)),
                        "cellSize" => pos(v).map(|v| s.cell_size = v.clamp(0.05, 2.0)),
                        "cellHeight" => pos(v).map(|v| s.cell_height = v.clamp(0.05, 2.0)),
                        _ => Err(format!("bake has no `{key}` ({BAKE_KEYS}; maxSlope is radians, as floorMaxAngle)")),
                    };
                    if let Err(m) = r {
                        lua_pop(L, 1);
                        return Err(m);
                    }
                }
            }
            _ => return Err(format!("bake is a table: bake = {{ {BAKE_KEYS} }}")),
        }
        w.nav().regions.insert(k, Region3::new(s, None));
        Ok(())
    }
}

fn region(w: &World3, id: Node3Id) -> Result<&Region3, String> {
    w.nav.as_ref().and_then(|n| n.regions.get(&id.to_bits())).ok_or_else(|| "this region is gone".into())
}

fn agent(w: &World3, id: Node3Id) -> Result<&Agent3, String> {
    w.nav.as_ref().and_then(|n| n.agents.get(&id.to_bits())).ok_or_else(|| "this agent is gone".into())
}

pub unsafe fn get(L: *mut lua_State, w: &mut World3, id: Node3Id, a: A) -> Result<c_int, String> {
    unsafe {
        let nice = super::nodes3d::nice;
        if w.kind(id) == Kind3::NavRegion {
            let r = region(w, id)?;
            return Ok(match a {
                A::Size => match r.size {
                    Some(s) => ret(L, nice(s)),
                    None => {
                        lua_pushnil(L);
                        1
                    }
                },
                A::Baked => retb(L, r.is_baked()),
                A::Progress => ret(L, nice(if r.wanted { 0.0 } else { r.region.as_ref().map_or(0.0, |g| g.progress()) })),
                _ => return Err(format!("NavigationRegion3D has no property `{}`", a.name())),
            });
        }
        let g = agent(w, id)?;
        let s = &g.settings;
        Ok(match a {
            A::MaxSpeed => ret(L, nice(s.max_speed)),
            A::MaxAcceleration => ret(L, nice(s.max_accel)),
            A::Radius => ret(L, nice(s.radius)),
            A::Height => ret(L, nice(s.height)),
            A::PathDesiredDistance => ret(L, nice(g.path_desired)),
            A::TargetDesiredDistance => ret(L, nice(g.target_desired)),
            A::Avoidance => retb(L, s.avoidance),
            A::AutoMove => retb(L, g.auto_move),
            _ => return Err(format!("NavigationAgent3D has no property `{}`", a.name())),
        })
    }
}

pub unsafe fn set(L: *mut lua_State, w: &mut World3, id: Node3Id, a: A, v: c_int) -> Result<(), String> {
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
        let k = id.to_bits();
        if w.kind(id) == Kind3::NavRegion {
            return match a {
                A::Size => {
                    let s = n()?;
                    if !(s > 0.0) {
                        return Err("size is more than 0 meters (the region's square)".into());
                    }
                    let r = w.nav().regions.get_mut(&k).ok_or("this region is gone")?;
                    r.size = Some(s);
                    r.wanted = true;
                    Ok(())
                }
                A::Baked | A::Progress => Err(format!("{key} is read-only")),
                _ => Err(format!("NavigationRegion3D has no property `{key}`")),
            };
        }
        let nav = w.nav();
        let g = nav.agents.get_mut(&k).ok_or("this agent is gone")?;
        match a {
            A::MaxSpeed => g.settings.max_speed = n()?.max(0.0),
            A::MaxAcceleration => g.settings.max_accel = n()?.max(0.0),
            A::Radius => g.settings.radius = n()?.clamp(0.05, maimbrain::nav::MAX_AGENT_RADIUS),
            A::Height => g.settings.height = n()?.max(0.1),
            A::PathDesiredDistance => g.path_desired = n()?.max(0.01),
            A::TargetDesiredDistance => g.target_desired = n()?.max(0.01),
            A::Avoidance => g.settings.avoidance = b()?,
            A::AutoMove => g.auto_move = b()?,
            _ => return Err(format!("NavigationAgent3D has no property `{key}`")),
        }
        let (s, slot) = (g.settings, g.id);
        if let Some((rk, aid)) = slot
            && let Some(reg) = nav.regions.get_mut(&rk).and_then(|r| r.region.as_mut())
        {
            reg.set_agent(aid, &s);
        }
        Ok(())
    }
}

/// An agent's `vx`, `vy`, `vz`: the crowd's velocity for it (read-only).
pub fn velocity(w: &World3, id: Node3Id, a: A) -> Option<f32> {
    let g = w.nav.as_ref()?.agents.get(&id.to_bits())?;
    match a {
        A::Vx => Some(g.state.vel[0]),
        A::Vy => Some(g.state.vel[1]),
        A::Vz => Some(g.state.vel[2]),
        _ => None,
    }
}

pub fn set_velocity(w: &mut World3, id: Node3Id, a: A) -> bool {
    let is_agent = w.nav.as_ref().is_some_and(|n| n.agents.contains_key(&id.to_bits()));
    if is_agent && matches!(a, A::Vx | A::Vy | A::Vz) {
        maimbrain::sys::log(maimbrain::sys::Level::Warn, "a NavigationAgent3D's vx, vy, vz are the crowd's velocity for it, read-only: move its body (autoMove = true, or body.vx = agent.vx)");
        return true;
    }
    false
}

unsafe fn vec_at(L: *mut lua_State, i: c_int, f: &str) -> Result<Vec3, String> {
    unsafe {
        let n = |k: c_int| -> Result<f32, String> {
            match num(L, k) {
                Some(v) if v.is_finite() => Ok(v as f32),
                _ => Err(format!("bad argument #{} to '{f}' (number expected, got {})", k - 1, type_name(L, k))),
            }
        };
        Ok(vec3(n(i)?, n(i + 1)?, n(i + 2)?))
    }
}

unsafe fn push3(L: *mut lua_State, v: Vec3) -> c_int {
    unsafe {
        for x in [v.x, v.y, v.z] {
            lua_pushnumber(L, super::nodes3d::nice(x));
        }
        3
    }
}

pub unsafe fn method(L: *mut lua_State, w: &mut World3, id: Node3Id, a: A, name: &str) -> Result<c_int, String> {
    unsafe {
        let k = id.to_bits();
        if w.kind(id) == Kind3::NavRegion {
            return match a {
                // Bakes again, from the level as it is now: only the tiles that changed.
                A::Bake => {
                    w.nav().regions.get_mut(&k).ok_or("this region is gone")?.wanted = true;
                    Ok(0)
                }
                A::Path => {
                    let (from, to) = (vec_at(L, 2, name)?, vec_at(L, 5, name)?);
                    let r = region(w, id)?;
                    let mut out = Vec::new();
                    let found = r.region.as_ref().filter(|g| g.is_baked()).and_then(|g| g.path(from, to, &mut out));
                    if found.is_none() {
                        lua_pushnil(L);
                        return Ok(1);
                    }
                    lua_createtable(L, (out.len() * 3) as c_int, 0);
                    for (i, p) in out.iter().enumerate() {
                        for (j, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                            lua_pushnumber(L, super::nodes3d::nice(v));
                            lua_rawseti(L, -2, (i * 3 + j + 1) as c_int);
                        }
                    }
                    Ok(1)
                }
                _ => Err(format!("no method `{name}` on NavigationRegion3D")),
            };
        }
        let here = w.scene.world(id).map(|m| trs_of(&m).0).unwrap_or(Vec3::ZERO);
        match a {
            A::SetTargetPosition => {
                let t = vec_at(L, 2, name)?;
                let g = w.nav().agents.get_mut(&k).ok_or("this agent is gone")?;
                g.target = Some(t);
                g.fresh = true;
                g.reached = false;
                g.finished = false;
                return Ok(0);
            }
            _ => {}
        }
        let g = agent(w, id)?;
        let s = &g.state;
        let pos = if g.id.is_some() { vec3(s.pos[0], s.pos[1], s.pos[2]) } else { here };
        Ok(match a {
            // Godot's: the next corner more than pathDesiredDistance away
            // (across), else the last one ahead.
            A::GetNextPathPosition => {
                if g.id.is_none() || g.finished || s.corners == 0 {
                    push3(L, g.target.filter(|_| !g.finished).unwrap_or(pos))
                } else {
                    let n = (s.corners as usize).min(4);
                    let far = s.ahead[..n].iter().find(|c| ((c[0] - pos.x).powi(2) + (c[2] - pos.z).powi(2)).sqrt() > g.path_desired).unwrap_or(&s.ahead[n - 1]);
                    push3(L, vec3(far[0], far[1], far[2]))
                }
            }
            A::IsNavigationFinished => retb(L, g.finished),
            A::IsTargetReached => retb(L, g.reached),
            A::IsTargetReachable => retb(L, g.target.is_some() && g.id.is_some() && s.target_state != 1 && !s.partial),
            A::DistanceToTarget => ret(L, super::nodes3d::nice(g.target.map_or(0.0, |t| pos.distance(t)))),
            A::GetFinalPosition => push3(L, if g.id.is_some() && s.target_state == 2 { vec3(s.target[0], s.target[1], s.target[2]) } else { g.target.unwrap_or(pos) }),
            _ => return Err(format!("no method `{name}` on NavigationAgent3D")),
        })
    }
}
