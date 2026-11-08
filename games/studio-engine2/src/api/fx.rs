//! The game's effects (docs/STUDIO-2.md §5 [juice]): `fx.burst`,
//! `fx.popup`, `fx.flash`, `fx.shake` and `fx.hitstop`, kept by the scene
//! and the engine; and `love.graphics.newParticleSystem`, the same
//! particles for `love.draw`.

use std::ffi::{c_int, c_void};

use maimbrain::scene::{Cmd, Emitter};

use super::*;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "fx", &[(c"burst", burst), (c"popup", popup), (c"flash", flash), (c"shake", shake), (c"hitstop", hitstop), (c"slowmo", slowmo), (c"punch", punch)]);
        register(L, ud, "love.graphics", &[(c"newParticleSystem", newParticleSystem)]);
        register_methods(
            L,
            ud,
            tag::PARTICLES,
            c"ParticleSystem",
            &[
                (c"emit", ps_emit),
                (c"update", ps_update),
                (c"setPosition", ps_set_position),
                (c"moveTo", ps_set_position),
                (c"getPosition", ps_position),
                (c"setEmissionRate", ps_set_rate),
                (c"getEmissionRate", ps_rate),
                (c"setParticleLifetime", ps_set_life),
                (c"setSpeed", ps_set_speed),
                (c"setSpread", ps_set_spread),
                (c"setLinearAcceleration", ps_set_accel),
                (c"setColors", ps_set_colors),
                (c"setSizes", ps_set_sizes),
                (c"start", ps_start),
                (c"stop", ps_stop),
                (c"pause", ps_stop),
                (c"reset", ps_reset),
                (c"isActive", ps_active),
                (c"getCount", ps_count),
                (c"setBufferSize", ps_set_buffer),
                (c"getBufferSize", ps_buffer),
                (c"typeOf", super::graphics::type_of),
            ],
        );
    }
}

/// `opts.key` from the table at `i`, if it's there.
unsafe fn field(L: *mut lua_State, i: c_int, k: &std::ffi::CStr) -> bool {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return false;
        }
        lua_rawgetfield(L, i, k.as_ptr());
        if lua_type(L, -1) == LUA_TNIL {
            lua_pop(L, 1);
            return false;
        }
        true
    }
}

// `fx.burst(preset, x, y, { count, color })`: one-shot particles in the world.
b!(burst, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "fx.burst", "preset name") };
    let preset = match super::node::preset_named(name) {
        Ok(p) => p,
        Err(m) => return err(L, &format!("fx.burst: {m}")),
    };
    // In a 3D game, with x, y and z: the particles in the world (meters).
    #[cfg(three)]
    if h.three.is_some() && num(L, 4).is_some() {
        return super::nodes3d::burst(L, h, preset);
    }
    let (Some(x), Some(y)) = (num(L, 2), num(L, 3)) else { return bad(L, if num(L, 2).is_none() { 2 } else { 3 }, "fx.burst", "number") };
    let mut count = 16.0;
    if field(L, 4, c"count") {
        count = opt(L, -1, 16.0);
        lua_pop(L, 1);
    }
    let mut colors = Vec::new();
    if field(L, 4, c"color") {
        match super::node::colors_at(L, lua_gettop(L)) {
            Ok(c) => colors = c,
            Err(m) => return err(L, &format!("fx.burst: {m}")),
        }
        lua_pop(L, 1);
    }
    let fx = &mut h.scene.fx;
    fx.preset = preset;
    fx.colors = colors;
    fx.emit(x as f32, y as f32, count.clamp(0.0, 500.0) as usize);
    0
});

// `fx.popup(text, x, y, { color, size })`: rises and fades, in the world.
b!(popup, |L, h| {
    #[cfg(three)]
    if h.three.is_some() && num(L, 4).is_some() {
        return super::nodes3d::popup(L, h);
    }
    let text = to_s(L, 1);
    let (Some(x), Some(y)) = (num(L, 2), num(L, 3)) else { return bad(L, if num(L, 2).is_none() { 2 } else { 3 }, "fx.popup", "number") };
    let mut color = 0xffffffff;
    let mut size = 18.0;
    if field(L, 4, c"color") {
        match color_at(L, lua_gettop(L), false) {
            Ok(c) => color = c,
            Err(m) => return err(L, &format!("fx.popup: color: {m}")),
        }
        lua_pop(L, 1);
    }
    if field(L, 4, c"size") {
        size = opt(L, -1, 18.0).clamp(4.0, 128.0) as f32;
        lua_pop(L, 1);
    }
    if h.scene.popups.list.len() < 64 {
        h.scene.popups.spawn(x as f32, y as f32, &text, color, size);
    }
    0
});

// `fx.flash(color, secs)`: the whole view, fading. It starts at the colour's
// alpha when it gives one (`{ 1, 1, 1, 0.3 }`), else at 0xb0 (about 70%).
b!(flash, |L, h| {
    let c = match lua_type(L, 1) {
        LUA_TNIL | -1 => 0xffffffff,
        _ => match color_at(L, 1, false) {
            Ok(c) => c,
            Err(m) => return err(L, &format!("fx.flash: {m}")),
        },
    };
    let alpha_given = lua_type(L, 1) == LUA_TTABLE && {
        lua_rawgeti(L, 1, 4);
        let given = num(L, -1).is_some();
        lua_pop(L, 1);
        given
    };
    let secs = opt(L, 2, 0.12).clamp(0.01, 2.0) as f32;
    h.scene.flash = Some((if alpha_given { c } else { c & 0xffffff00 | 0xb0 }, secs, secs));
    0
});

// `fx.shake(amount, secs)`: the one shake, for the camera and love.draw's attached drawing.
b!(shake, |L, h| {
    let amount = opt(L, 1, 6.0).clamp(0.0, 100.0) as f32;
    let secs = opt(L, 2, 0.25).clamp(0.01, 3.0) as f32;
    // A bigger shake wins over a fading one.
    if amount >= h.shake.0 * (h.shake.2 / h.shake.1.max(1e-3)) {
        h.shake = (amount, secs, secs);
    }
    0
});

// `fx.hitstop(secs)`: the world holds still a moment (drawing goes on).
b!(hitstop, |L, h| {
    h.hitstop = h.hitstop.max(opt(L, 1, 0.06).clamp(0.0, 0.5) as f32);
    0
});

// `fx.slowmo(rate, secs)`: the world at `rate` of its speed (0.05–1, default
// 0.3) for `secs` (default 1, at most 3), easing in over 0.2 s and back over
// 0.6 s on real time: the SDK's `ui::SlowMo`. `fx.slowmo(1)` eases back now.
b!(slowmo, |L, h| {
    let Some(rate) = num(L, 1).or((lua_gettop(L) == 0).then_some(0.3)).filter(|r| r.is_finite()) else {
        return err(L, &format!("fx.slowmo(rate, secs): rate is the world's speed, 0.05–1 (0.3 is a third), got {}", to_s(L, 1)));
    };
    if rate >= 1.0 {
        h.slowmo.end();
    } else {
        // A hold that isn't a number (NaN would never end) is the default.
        let secs = num(L, 2).filter(|s| s.is_finite()).unwrap_or(1.0);
        h.slowmo.start(rate as f32, secs as f32);
    }
    0
});

// `fx.punch(node, amount)`: a kick that springs back, over whatever else
// moves the node: `amount` 0.2 makes it 20% bigger at once; a table
// `{ x, y, z, scale }` throws it about that far (units, meters in 3D) and
// back, and kicks its size too.
b!(punch, |L, h| {
    use crate::libs::{PunchKind, Target};
    let target = match lua_type(L, 1) {
        #[cfg(three)]
        LUA_TUSERDATA if lua_userdatatag(L, 1) == tag::NODE3 => match super::three::node_at(L, 1) {
            Some(id) if h.three.as_ref().is_some_and(|w| w.scene.alive(id)) => Target::Node3(id),
            _ => return err(L, "fx.punch: that node was destroyed"),
        },
        LUA_TUSERDATA => match super::node::node_at(L, 1) {
            Some(id) if h.scene.alive(id) => Target::Node(id),
            Some(_) => return err(L, "fx.punch: that node was destroyed"),
            None => return bad(L, 1, "fx.punch", "node"),
        },
        _ => return bad(L, 1, "fx.punch", "node"),
    };
    let clamp = |v: f64| v.clamp(-0.5, 1.0) as f32;
    match lua_type(L, 2) {
        LUA_TNIL | -1 => h.libs.punch(target, PunchKind::Scale, [0.2; 3]),
        // A kick that isn't a number (NaN would spoil the size for good) is none.
        LUA_TNUMBER if !lua_tonumber(L, 2).is_finite() => {}
        LUA_TNUMBER => h.libs.punch(target, PunchKind::Scale, [clamp(lua_tonumber(L, 2)); 3]),
        LUA_TTABLE => {
            let f = |k: &std::ffi::CStr| {
                lua_rawgetfield(L, 2, k.as_ptr());
                let v = num(L, -1).filter(|v| v.is_finite()).unwrap_or(0.0);
                lua_pop(L, 1);
                v
            };
            let (x, y, z, s) = (f(c"x"), f(c"y"), f(c"z"), f(c"scale"));
            if s != 0.0 {
                h.libs.punch(target, PunchKind::Scale, [clamp(s); 3]);
            }
            if x != 0.0 || y != 0.0 || z != 0.0 {
                // A 3D body's place is its physics': moving it would move the body.
                #[cfg(three)]
                if let Target::Node3(id) = target
                    && h.three.as_deref().is_some_and(|w| super::bodies3d::is_physics(w.kind(id)))
                {
                    return err(L, "fx.punch: a physics body's place is the physics' (moving it would move the body): punch its size, fx.punch(body, 0.2), or the place of a node under it");
                }
                let m = |v: f64| v.clamp(-1e4, 1e4) as f32;
                h.libs.punch(target, PunchKind::Move, [m(x), m(y), m(z)]);
            }
        }
        _ => return err(L, &format!("fx.punch(node, amount): amount is how much bigger (0.2 is 20%), or {{ x, y, z, scale }}, got {}", type_name(L, 2))),
    }
    0
});

/// One frame of the shake: an offset from the frame number (the same in a
/// replay), shrinking as it ends.
pub fn shake_step(h: &mut Host, dt: f32) {
    let (amount, secs, left) = h.shake;
    if left <= 0.0 {
        (h.scene.camera.shake_x, h.scene.camera.shake_y) = (0.0, 0.0);
        #[cfg(three)]
        if let Some(w) = &mut h.three {
            w.shake = maimbrain::gfx3d::Vec3::ZERO;
        }
        return;
    }
    let k = amount * (left / secs.max(1e-3));
    let f = h.frames as f32;
    let (sx, sy) = (k * (f * 2.1).sin() * (f * 0.73).cos(), k * (f * 1.7 + 1.3).sin() * (f * 0.51).cos());
    // A 3D game's shake is the 3D camera's, in meters; its 2D over it stays put (the HUD).
    #[cfg(three)]
    if let Some(w) = &mut h.three {
        w.shake = maimbrain::gfx3d::vec3(sx, sy, 0.0);
        h.shake.2 = (left - dt).max(0.0);
        return;
    }
    h.scene.camera.shake_x = sx;
    h.scene.camera.shake_y = sy;
    h.shake.2 = (left - dt).max(0.0);
}

// ---------------------------------------------------------------- particle systems

// LÖVE's `newParticleSystem(image, buffer)`: our particles take a preset's
// look, so the first argument is a preset name (an image is taken, and the
// particles are the `burst` preset's).
b!(newParticleSystem, |L, h| {
    let preset = match st(L, 1) {
        Some(n) => match super::node::preset_named(n) {
            Ok(p) => p,
            Err(m) => return err(L, &format!("newParticleSystem: {m}")),
        },
        None => maimbrain::scene::Preset::Burst,
    };
    if h.scene.systems.len() >= 64 {
        return err(L, "newParticleSystem: more than 64 particle systems in a round: reuse them (ps:reset())");
    }
    let mut e = Emitter::new(h.rng.next_u32() as u64);
    e.preset = preset;
    e.emitting = false;
    e.particles.cap = opt(L, 2, 1000.0).clamp(1.0, 2000.0) as usize;
    h.scene.systems.push(e);
    h.ps_pos.push((0.0, 0.0));
    push_handle(L, tag::PARTICLES, h.scene.systems.len() as u32 - 1)
});

/// The system at stack index 1.
unsafe fn ps(L: *mut lua_State, h: &mut Host) -> Option<(usize, &mut Emitter)> {
    unsafe {
        let i = handle(L, 1, tag::PARTICLES)? as usize;
        h.scene.systems.get_mut(i).map(|e| (i, e))
    }
}

/// `love.graphics.draw(ps, x, y)`.
pub unsafe fn draw_particles(L: *mut lua_State, h: &mut Host) -> c_int {
    unsafe {
        let Some(i) = handle(L, 1, tag::PARTICLES) else { return 0 };
        let c = Cmd::Particles { system: i as u16, x: opt(L, 2, 0.0) as f32, y: opt(L, 3, 0.0) as f32 };
        super::graphics::rec(L, h, c)
    }
}

b!(ps_emit, |L, h| {
    let n = opt(L, 2, 1.0).clamp(0.0, 2000.0) as usize;
    let Some(i) = handle(L, 1, tag::PARTICLES) else { return bad(L, 1, "emit", "ParticleSystem") };
    let (x, y) = h.ps_pos.get(i as usize).copied().unwrap_or_default();
    if let Some((_, e)) = ps(L, h) {
        e.emit(x, y, n);
    }
    0
});

b!(ps_update, |L, h| {
    let dt = opt(L, 2, 0.0).clamp(0.0, 1.0) as f32;
    let Some(i) = handle(L, 1, tag::PARTICLES) else { return bad(L, 1, "update", "ParticleSystem") };
    let (x, y) = h.ps_pos.get(i as usize).copied().unwrap_or_default();
    if let Some((_, e)) = ps(L, h) {
        e.step(dt, x, y);
    }
    0
});

b!(ps_set_position, |L, h| {
    if let Some(i) = handle(L, 1, tag::PARTICLES)
        && let Some(p) = h.ps_pos.get_mut(i as usize)
    {
        *p = (opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    }
    0
});

b!(ps_position, |L, h| {
    let (x, y) = handle(L, 1, tag::PARTICLES).and_then(|i| h.ps_pos.get(i as usize).copied()).unwrap_or_default();
    ret2(L, x as f64, y as f64)
});

b!(ps_set_rate, |L, h| {
    let r = opt(L, 2, 0.0).clamp(0.0, 2000.0) as f32;
    if let Some((_, e)) = ps(L, h) {
        e.rate = r;
    }
    0
});

b!(ps_rate, |L, h| ret(L, ps(L, h).map_or(0.0, |(_, e)| e.rate) as f64));

// Ranges take their middle: our particles vary about the preset's own.
b!(ps_set_life, |L, h| {
    let (a, b) = (opt(L, 2, 1.0), opt(L, 3, opt(L, 2, 1.0)));
    if let Some((_, e)) = ps(L, h) {
        e.life = Some(((a + b) / 2.0).max(0.01) as f32);
    }
    0
});

b!(ps_set_speed, |L, h| {
    let (a, b) = (opt(L, 2, 0.0), opt(L, 3, opt(L, 2, 0.0)));
    if let Some((_, e)) = ps(L, h) {
        e.speed = Some(((a + b) / 2.0) as f32);
    }
    0
});

// LÖVE's spread is the whole angle; ours is either side of up.
b!(ps_set_spread, |L, h| {
    let s = opt(L, 2, 0.0) as f32 / 2.0;
    if let Some((_, e)) = ps(L, h) {
        e.spread = Some(s);
    }
    0
});

b!(ps_set_accel, |L, h| {
    let (ymin, ymax) = (opt(L, 3, 0.0), opt(L, 5, opt(L, 3, 0.0)));
    if let Some((_, e)) = ps(L, h) {
        e.gravity = Some(((ymin + ymax) / 2.0) as f32);
    }
    0
});

// setColors(r, g, b, a, …) or ({ r, g, b, a }, …): the colours particles
// come in (LÖVE fades through them over a life; ours pick one each).
b!(ps_set_colors, |L, h| {
    let top = lua_gettop(L);
    let mut colors = Vec::new();
    let mut i = 2;
    while i <= top && colors.len() < 8 {
        if lua_type(L, i) == LUA_TTABLE {
            match color_at(L, i, false) {
                Ok(c) => colors.push(c),
                Err(m) => return err(L, &format!("setColors: {m}")),
            }
            i += 1;
        } else {
            let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            colors.push(c(opt(L, i, 1.0)) << 24 | c(opt(L, i + 1, 1.0)) << 16 | c(opt(L, i + 2, 1.0)) << 8 | c(opt(L, i + 3, 1.0)));
            i += 4;
        }
    }
    if let Some((_, e)) = ps(L, h) {
        e.colors = colors;
    }
    0
});

// setSizes(s): the first, in units (LÖVE's are a texture's scale).
b!(ps_set_sizes, |L, h| {
    let s = num(L, 2).map(|s| s.max(0.0) as f32);
    if let Some((_, e)) = ps(L, h) {
        e.size = s;
    }
    0
});

b!(ps_start, |L, h| {
    if let Some((_, e)) = ps(L, h) {
        e.emitting = true;
    }
    0
});

b!(ps_stop, |L, h| {
    if let Some((_, e)) = ps(L, h) {
        e.emitting = false;
    }
    0
});

b!(ps_reset, |L, h| {
    if let Some((_, e)) = ps(L, h) {
        e.particles.clear();
    }
    0
});

b!(ps_active, |L, h| retb(L, ps(L, h).is_some_and(|(_, e)| e.emitting)));
b!(ps_count, |L, h| ret(L, ps(L, h).map_or(0, |(_, e)| e.particles.len()) as f64));

b!(ps_set_buffer, |L, h| {
    let n = opt(L, 2, 1000.0).clamp(1.0, 2000.0) as usize;
    if let Some((_, e)) = ps(L, h) {
        e.particles.cap = n;
    }
    0
});

b!(ps_buffer, |L, h| ret(L, ps(L, h).map_or(0, |(_, e)| e.particles.cap) as f64));
