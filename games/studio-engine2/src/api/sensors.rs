//! `sensors` (docs/STUDIO-2.md §5 [input], Phase 3e): the phone's tilt,
//! attitude, motion and the mic's loudness, and `sensors.scope`, aiming by
//! pointing the phone, over the SDK's `maimbrain::sensors`.
//!
//! A game declares what it reads in studio.toml's `sensors`, and `mb build`
//! packs it with the engine build that imports those sensors (§4.5): a
//! sensor the build doesn't import reads as unavailable. The engine reads
//! each sensor once a frame, before the frame's Lua, so every call in a
//! frame sees the same value; the platform records the readings, so a
//! replay reproduces them. The title card's attract mode and the world
//! under the game-over card get no readings, as they get no input.

use std::ffi::{c_int, c_void};

use maimbrain::sensors::{AimSource, Attitude, Motion, Scope, Tilt};

use super::*;

/// Which sensors studio.toml declares (`sensors = ["tilt", …]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    pub tilt: bool,
    pub motion: bool,
    pub attitude: bool,
    pub loudness: bool,
}

impl Declared {
    /// From studio.toml's `sensors` (`haptics` is every build's and needs no
    /// declaring; `light` isn't in Studio).
    pub fn parse(v: Option<&toml::Value>) -> Result<Declared, String> {
        let mut d = Declared::default();
        let Some(v) = v else { return Ok(d) };
        let list = v.as_array().ok_or("studio.toml: sensors is a list, like [\"haptics\", \"tilt\"]")?;
        for s in list {
            match s.as_str() {
                Some("haptics") => {}
                Some("tilt") => d.tilt = true,
                Some("motion") => d.motion = true,
                Some("attitude") => d.attitude = true,
                Some("loudness") => d.loudness = true,
                _ => return Err(format!("studio.toml: sensors can be haptics, tilt, motion, attitude and loudness, not {}", s.as_str().map_or_else(|| format!("a {}", s.type_str()), |x| format!("\"{x}\"")))),
            }
        }
        Ok(d)
    }
}

/// One frame's readings (None: unavailable, not declared, or no sample yet).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sensed {
    pub tilt: Option<Tilt>,
    pub motion: Option<Motion>,
    pub attitude: Option<Attitude>,
    pub loudness: Option<f32>,
}

impl Sensed {
    /// This frame's readings of the sensors this engine build imports
    /// (`cfg(sense_*)`, build.rs). The build is picked from what the game
    /// declares, so it reads those (the `sensors.*` functions check the
    /// declaration), and `player.tilt` reads tilt.
    #[allow(unused_mut)]
    pub fn read() -> Sensed {
        let mut s = Sensed::default();
        #[cfg(sense_motion)]
        {
            s.tilt = maimbrain::sensors::tilt();
            s.motion = maimbrain::sensors::motion();
            s.attitude = maimbrain::sensors::attitude();
        }
        #[cfg(sense_mic)]
        {
            s.loudness = maimbrain::sensors::loudness();
        }
        s
    }

    /// `player.tilt`'s steer, −1…1: the phone's sideways tip (1.0's `x × 2`).
    pub fn steer(&self) -> Option<f32> {
        self.tilt.map(|t| (t.x * 2.0).clamp(-1.0, 1.0))
    }
}

/// A round's sensor state: this frame's readings, the attitude when the
/// round first had one (yaw is measured from it), and its scopes.
#[derive(Default)]
pub struct Sensors {
    pub now: Sensed,
    pub reference: Option<Attitude>,
    pub scopes: Vec<Scope>,
}

impl Sensors {
    /// The frame's readings, then every scope moved by them.
    pub fn frame(&mut self, now: Sensed, dt: f32) {
        self.now = now;
        if self.reference.is_none() {
            self.reference = now.attitude;
        }
        for s in &mut self.scopes {
            s.update_with(dt, now.attitude, now.tilt);
        }
    }
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "sensors", &[(c"tilt", tilt), (c"attitude", attitude), (c"motion", motion), (c"loudness", loudness), (c"scope", scope)]);
        register_kind(L, ud, tag::SCOPE, c"Scope", &[(c"__index", scope_index), (c"__namecall", scope_namecall)]);
    }
}

/// An error for a sensor studio.toml doesn't declare.
unsafe fn undeclared(L: *mut lua_State, f: &str, name: &str) -> c_int {
    unsafe { err(L, &format!("sensors.{f}() needs sensors = [\"{name}\"] in studio.toml (and the game must still play by touch: docs [input])")) }
}

// `sensors.tilt()` → roll, pitch in radians (roll positive with the right
// edge down, pitch 0 upright to π/2 lying flat), or nil.
b!(tilt, |L, h| {
    if !h.project.sensors.tilt {
        return undeclared(L, "tilt", "tilt");
    }
    match h.sensors.now.tilt {
        Some(t) => ret2(L, t.roll() as f64, t.pitch() as f64),
        None => 0,
    }
});

// `sensors.attitude()` → yaw, pitch, roll in radians, or nil. Yaw is from
// where the phone pointed when the round first read it (positive turned
// right); pitch is the back of the phone above level; roll as tilt's.
b!(attitude, |L, h| {
    if !h.project.sensors.attitude {
        return undeclared(L, "attitude", "attitude");
    }
    let (Some(a), Some(r)) = (h.sensors.now.attitude, h.sensors.reference) else { return 0 };
    lua_pushnumber(L, a.relative_to(r).yaw() as f64);
    lua_pushnumber(L, a.pitch() as f64);
    lua_pushnumber(L, a.roll() as f64);
    3
});

// `sensors.motion()` → x, y, z: the hand's acceleration without gravity,
// in g (x right, y down the screen, z out of it), or nil.
b!(motion, |L, h| {
    if !h.project.sensors.motion {
        return undeclared(L, "motion", "motion");
    }
    match h.sensors.now.motion {
        Some(m) => {
            lua_pushnumber(L, m.x as f64);
            lua_pushnumber(L, m.y as f64);
            lua_pushnumber(L, m.z as f64);
            3
        }
        None => 0,
    }
});

// `sensors.loudness()` → 0–1, or nil while the mic is unavailable.
b!(loudness, |L, h| {
    if !h.project.sensors.loudness {
        return undeclared(L, "loudness", "loudness");
    }
    match h.sensors.now.loudness {
        Some(v) => ret(L, v as f64),
        None => 0,
    }
});

// `sensors.scope{ precise = true, w = …, h = … }`: a crosshair aimed by
// pointing the phone (the attitude, else tilt, else dragging), over the
// canvas (or a w × h area).
b!(scope, |L, h| {
    if !h.project.sensors.attitude && !h.project.sensors.tilt {
        return err(L, "sensors.scope needs sensors = [\"attitude\"] in studio.toml (or \"tilt\"), and the game still aims by touch: scope:drag(dx, dy)");
    }
    let (mut w, mut hh) = (h.project.size.0 as f32, h.project.size.1 as f32);
    let mut precise = false;
    match lua_type(L, 1) {
        LUA_TNIL | -1 => {}
        LUA_TTABLE => {
            lua_getfield(L, 1, c"precise".as_ptr());
            precise = lua_toboolean(L, -1) != 0;
            lua_getfield(L, 1, c"w".as_ptr());
            if let Some(v) = num(L, -1) {
                w = v as f32;
            }
            lua_getfield(L, 1, c"h".as_ptr());
            if let Some(v) = num(L, -1) {
                hh = v as f32;
            }
            lua_pop(L, 3);
        }
        _ => return bad(L, 1, "sensors.scope", "table"),
    }
    if !(w > 0.0 && hh > 0.0 && w.is_finite() && hh.is_finite()) {
        return err(L, "sensors.scope: w and h are sizes above 0");
    }
    if h.sensors.scopes.len() >= 8 {
        return err(L, "sensors.scope: 8 scopes at most (make one in love.load and keep it)");
    }
    let s = if precise { Scope::precise(w, hh) } else { Scope::new(w, hh) };
    h.sensors.scopes.push(s);
    push_handle(L, tag::SCOPE, h.sensors.scopes.len() as u32 - 1)
});

b!(scope_index, |L, h| {
    let Some(s) = handle(L, 1, tag::SCOPE).and_then(|i| h.sensors.scopes.get(i as usize)) else { return err(L, "not a scope") };
    match st(L, 2).unwrap_or("") {
        "x" => ret(L, s.position().0 as f64),
        "y" => ret(L, s.position().1 as f64),
        "source" => match s.source() {
            Some(AimSource::Attitude) => rets(L, "attitude"),
            Some(AimSource::Tilt) => rets(L, "tilt"),
            Some(AimSource::Drag) => rets(L, "drag"),
            None => 0,
        },
        k => err(L, &format!("a scope has no `{k}` (x, y, source, and :recenter(), :drag(dx, dy))")),
    }
});

b!(scope_namecall, |L, h| {
    let Some(i) = handle(L, 1, tag::SCOPE).map(|i| i as usize).filter(|i| *i < h.sensors.scopes.len()) else { return err(L, "not a scope") };
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    let s = &mut h.sensors.scopes[i];
    match name.as_str() {
        "recenter" => s.recenter(),
        "drag" => {
            let (Some(dx), Some(dy)) = (num(L, 2), num(L, 3)) else { return err(L, "scope:drag(dx, dy) takes how far the finger moved, in canvas units") };
            s.drag(dx as f32, dy as f32);
        }
        n => return err(L, &format!("a scope has no :{n}() (:recenter(), :drag(dx, dy))")),
    }
    0
});
