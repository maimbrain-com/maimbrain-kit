//! The rest of what Lua has in this build (docs/STUDIO-2.md §5 [love],
//! [rounds], [maths]): `love.timer` (game time), `love.math` and
//! `math.random` (the round's PCG32, seeded by the platform), what's held on
//! `love.keyboard`, `love.mouse` and `love.touch`, `love.window`,
//! `love.system`, `round.over`, `print`, and the chunk loader `require`
//! uses.

use std::ffi::{c_int, c_void};

use maimbrain::Rng;
use maimbrain::sys::{self, Level};

use super::*;
use crate::input::is_key_name;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "love.timer", &[(c"getDelta", getDelta), (c"getTime", getTime), (c"getFPS", getFPS), (c"getAverageDelta", getDelta)]);
        register(L, ud, "love.math", &[(c"random", random), (c"randomNormal", randomNormal), (c"setRandomSeed", setRandomSeed)]);
        // math.random is the same generator (§5 [maths]); the library table
        // is made read-only by the sandbox after this.
        lua_getfield(L, LUA_GLOBALSINDEX, c"math".as_ptr());
        set_fn(L, ud, c"random", random);
        set_fn(L, ud, c"randomseed", setRandomSeed);
        lua_pop(L, 1);
        register(L, ud, "love.keyboard", &[(c"isDown", isDown), (c"isScancodeDown", isDown)]);
        register(L, ud, "love.mouse", &[(c"isDown", mouseDown), (c"getPosition", mousePosition), (c"getX", mouseX), (c"getY", mouseY)]);
        register(L, ud, "love.touch", &[(c"getTouches", getTouches), (c"getPosition", touchPosition)]);
        register(L, ud, "love.joystick", &[(c"getJoysticks", getJoysticks), (c"getJoystickCount", getJoystickCount)]);
        register_methods(
            L,
            ud,
            tag::JOYSTICK,
            c"Joystick",
            &[
                (c"isGamepadDown", j_isGamepadDown),
                (c"getGamepadAxis", j_getGamepadAxis),
                (c"getName", j_getName),
                (c"isConnected", j_isConnected),
                (c"getID", j_getID),
                (c"isGamepad", j_isGamepad),
                (c"getAxis", j_getAxis),
                (c"getAxes", j_getAxes),
                (c"getAxisCount", j_getAxisCount),
                (c"typeOf", super::graphics::type_of),
            ],
        );
        register(
            L,
            ud,
            "love.math",
            &[(c"noise", noise), (c"triangulate", triangulate), (c"isConvex", isConvex), (c"colorFromBytes", colorFromBytes), (c"colorToBytes", colorToBytes), (c"newRandomGenerator", newRandomGenerator)],
        );
        register_methods(L, ud, tag::RNG, c"RandomGenerator", &[(c"random", r_random), (c"randomNormal", r_randomNormal), (c"setSeed", r_setSeed), (c"typeOf", super::graphics::type_of)]);
        register(
            L,
            ud,
            "love.window",
            &[(c"getSafeArea", getSafeArea), (c"getDimensions", windowDimensions), (c"getDPIScale", getDPIScale), (c"setMode", setMode), (c"getMode", getMode)],
        );
        // The pointer is a finger on a phone: cursors are accepted and do nothing.
        register(
            L,
            ud,
            "love.mouse",
            &[(c"getSystemCursor", getSystemCursor), (c"setCursor", cursorNoop), (c"getCursor", getCursor), (c"setVisible", cursorNoop), (c"isVisible", isVisible), (c"isCursorSupported", isCursorSupported)],
        );
        register_methods(L, ud, tag::CURSOR, c"Cursor", &[(c"getType", cursorType), (c"typeOf", super::graphics::type_of)]);
        // Lua's os, as far as a round can have it: a clock that's the round's
        // (game time, and a date picked by the round's seed), so
        // `math.randomseed(os.time())` gives each round its own numbers and
        // a replay the same ones; `os.exit()` ends the round.
        register(L, ud, "os", &[(c"time", osTime), (c"clock", getTime), (c"difftime", osDifftime), (c"exit", osExit)]);
        register(L, ud, "love.system", &[(c"getOS", getOS)]);
        register(L, ud, "__engine", &[(c"over", over), (c"chunk", chunk), (c"joystick", joystick), (c"steer", steer)]);
        sv_push_rfn(L, print, ud, c"print".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"print".as_ptr());
    }
}

b!(getDelta, |L, h| ret(L, h.dt as f64));
b!(getTime, |L, h| ret(L, h.time));
// The platform's loop is 60 a second; frames it drops aren't the game's to see.
b!(getFPS, |L, h| ret(L, 60.0));

/// [0, 1) with 53 bits.
fn unit(r: &mut Rng) -> f64 {
    let hi = (r.next_u32() >> 6) as u64;
    let lo = (r.next_u32() >> 5) as u64;
    ((hi << 27) | lo) as f64 / (1u64 << 53) as f64
}

b!(random, |L, h| random_from(L, &mut h.rng, 1));

/// LÖVE's random(), random(max), random(min, max) from `r`, arguments from `at`.
unsafe fn random_from(L: *mut lua_State, r: &mut Rng, at: c_int) -> c_int {
    unsafe {
    match (num(L, at), num(L, at + 1)) {
        (None, None) if lua_gettop(L) < at => ret(L, unit(r)),
        (Some(m), None) if m >= 1.0 => ret(L, 1.0 + (unit(r) * m.floor()).floor()),
        (Some(lo), Some(hi)) if hi.floor() >= lo.floor() => {
            let (lo, hi) = (lo.floor(), hi.floor());
            ret(L, lo + (unit(r) * (hi - lo + 1.0)).floor())
        }
        (Some(_), _) => err(L, "bad argument to 'random' (interval is empty)"),
        _ if lua_gettop(L) < at => ret(L, unit(r)),
        _ => bad(L, at, "random", "number"),
    }
    }
}

b!(randomNormal, |L, h| normal_from(L, &mut h.rng, 1));

unsafe fn normal_from(L: *mut lua_State, r: &mut Rng, at: c_int) -> c_int {
    unsafe {
        let (sd, mean) = (opt(L, at, 1.0), opt(L, at + 1, 0.0));
        // Box–Muller, as LÖVE does.
        let u1 = 1.0 - unit(r);
        let u2 = unit(r);
        ret(L, mean + sd * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos())
    }
}

b!(setRandomSeed, |L, h| match num(L, 1) {
    Some(s) if s.is_finite() => {
        h.rng = Rng::new(s as i64 as u64);
        0
    }
    _ => bad(L, 1, "setRandomSeed", "number"),
});

b!(isDown, |L, h| {
    let n = lua_gettop(L);
    if n == 0 {
        return bad(L, 1, "isDown", "key name");
    }
    for i in 1..=n {
        let Some(k) = st(L, i) else { return bad(L, i, "isDown", "key name") };
        if !is_key_name(k) {
            let hint = match k {
                "enter" => " (LÖVE calls it \"return\")",
                "shift" | "ctrl" | "alt" => " (\"lshift\" or \"rshift\", \"lctrl\", \"lalt\")",
                " " => " (\"space\")",
                _ => "",
            };
            return err(L, &format!("love.keyboard.isDown: no key \"{k}\"{hint}"));
        }
        if h.held.key(k) {
            return retb(L, true);
        }
    }
    retb(L, false)
});

b!(mouseDown, |L, h| {
    let n = lua_gettop(L).max(1);
    for i in 1..=n {
        let b = opt(L, i, 1.0) as usize;
        if (1..=3).contains(&b) && h.held.mouse[b - 1] {
            return retb(L, true);
        }
    }
    retb(L, false)
});
b!(mousePosition, |L, h| ret2(L, h.held.mx as f64, h.held.my as f64));
b!(mouseX, |L, h| ret(L, h.held.mx as f64));
b!(mouseY, |L, h| ret(L, h.held.my as f64));

b!(getTouches, |L, h| {
    lua_createtable(L, h.held.touches.len() as c_int, 0);
    for (i, t) in h.held.touches.iter().enumerate() {
        lua_pushnumber(L, t.0 as f64);
        lua_rawseti(L, -2, i as c_int + 1);
    }
    1
});

b!(touchPosition, |L, h| {
    let id = opt(L, 1, -1.0);
    match h.held.touches.iter().find(|t| t.0 as f64 == id) {
        Some(t) => ret2(L, t.1 as f64, t.2 as f64),
        None => err(L, "love.touch.getPosition: no touch with that id (ids come from getTouches or touchpressed)"),
    }
});

b!(getSafeArea, |L, h| {
    let (x, y, w, hh) = h.safe;
    for v in [x, y, w, hh] {
        lua_pushnumber(L, v as f64);
    }
    4
});
b!(windowDimensions, |L, h| ret2(L, h.scene.width as f64, h.scene.height as f64));

// love.window.setMode(w, h, flags): the platform owns the window, so it
// changes nothing; it's accepted when it asks for the canvas studio.toml's
// size makes, as conf.lua's window becomes studio.toml.
b!(setMode, |L, h| {
    let (w, hh) = (num(L, 1), num(L, 2));
    let (cw, ch) = (h.scene.width as f64, h.scene.height as f64);
    match (w, hh) {
        (Some(w), Some(hh)) if (w - cw).abs() < 0.5 && (hh - ch).abs() < 0.5 => retb(L, true),
        (Some(w), Some(hh)) => err(
            L,
            &format!("love.window.setMode({}, {}): the platform owns the window, and the canvas is {}×{}: say size = [{}, {}] in studio.toml", fmt_num(w), fmt_num(hh), fmt_num(cw), fmt_num(ch), fmt_num(w), fmt_num(hh)),
        ),
        _ => bad(L, 1, "setMode", "width and height"),
    }
});
b!(getMode, |L, h| {
    lua_pushnumber(L, h.scene.width as f64);
    lua_pushnumber(L, h.scene.height as f64);
    lua_createtable(L, 0, 0);
    3
});

b!(getSystemCursor, |L, h| match st(L, 1) {
    Some(_) => push_handle(L, tag::CURSOR, 0),
    None => bad(L, 1, "getSystemCursor", "cursor type"),
});
b!(cursorNoop, |L, h| 0);
b!(getCursor, |L, h| {
    lua_pushnil(L);
    1
});
b!(isVisible, |L, h| retb(L, true));
b!(isCursorSupported, |L, h| retb(L, false));
b!(cursorType, |L, h| rets(L, "arrow"));

// The round's clock as seconds since 1970: a moment in 2026 picked by the
// round's seed, then game time. `os.time(table)` isn't a date library.
b!(osTime, |L, h| {
    if lua_gettop(L) >= 1 && lua_type(L, 1) != LUA_TNIL {
        return err(L, "os.time(date): there's no calendar in the sandbox; os.time() is the round's clock");
    }
    const YEAR_2026: u64 = 1_767_225_600;
    ret(L, (YEAR_2026 + h.seed % 31_536_000) as f64 + h.time.floor())
});
b!(osDifftime, |L, h| ret(L, opt(L, 1, 0.0) - opt(L, 2, 0.0)));
b!(osExit, |L, h| {
    if h.over.is_none() {
        h.over = Some(Over { heading: None, cause: Some("exit".into()), won: false });
    }
    0
});
b!(getDPIScale, |L, h| ret(L, 2.0));
b!(getOS, |L, h| rets(L, "Maimbrain"));

b!(
    /// `round.over(heading, cause, won)` (the prelude's `round` calls it).
    over, |L, h| {
    if h.over.is_none() {
        let heading = (lua_type(L, 1) != LUA_TNIL && lua_gettop(L) >= 1).then(|| to_s(L, 1));
        let cause = (lua_type(L, 2) != LUA_TNIL && lua_gettop(L) >= 2).then(|| to_s(L, 2));
        h.over = Some(Over { heading, cause, won: lua_toboolean(L, 3) != 0 });
    }
    0
});

b!(
    /// `game/<name>.lua`'s chunk, loaded (not run): `require`'s loader.
    chunk, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "require", "module name") };
    let name = name.trim_end_matches(".lua").replace('.', "/");
    let Some(bc) = h.project.chunks.get(&name) else {
        return err(L, &format!("require(\"{name}\"): there's no game/{name}.lua"));
    };
    let chunkname = std::ffi::CString::new(format!("=game/{name}.lua")).unwrap();
    if crate::vm::load_game_chunk(L, chunkname.as_ptr(), bc) != 0 {
        // The message is on the stack; raise it.
        return -1;
    }
    1
});

b!(print, |L, h| {
    let n = lua_gettop(L);
    let parts: Vec<String> = (1..=n).map(|i| to_s(L, i)).collect();
    sys::log(Level::Info, &parts.join("\t"));
    0
});

// ---------------------------------------------------------------- joysticks

b!(joystick, |L, h| push_handle(L, tag::JOYSTICK, 1));
b!(steer, |L, h| {
    let (x, y) = h.held.steer();
    lua_pushnumber(L, x as f64);
    lua_pushnumber(L, y as f64);
    if h.held.pad {
        push_str(L, "xbox");
    } else {
        lua_pushnil(L);
    }
    3
});
b!(getJoysticks, |L, h| {
    lua_createtable(L, 1, 0);
    if h.held.pad {
        push_handle(L, tag::JOYSTICK, 1);
        lua_rawseti(L, -2, 1);
    }
    1
});
b!(getJoystickCount, |L, h| ret(L, h.held.pad as u8 as f64));
b!(j_isGamepadDown, |L, h| {
    let n = lua_gettop(L);
    for i in 2..=n {
        let Some(b) = st(L, i) else { return bad(L, i, "isGamepadDown", "button name") };
        if !crate::input::BUTTONS.contains(&b) {
            return err(L, &format!("isGamepadDown: no button \"{b}\" ({})", crate::input::BUTTONS.join(", ")));
        }
        if h.held.button(b) {
            return retb(L, true);
        }
    }
    retb(L, false)
});
b!(j_getGamepadAxis, |L, h| {
    let Some(a) = st(L, 2) else { return bad(L, 2, "getGamepadAxis", "axis name") };
    if !crate::input::AXES.contains(&a) {
        return err(L, &format!("getGamepadAxis: no axis \"{a}\" ({})", crate::input::AXES.join(", ")));
    }
    ret(L, h.held.axis(a) as f64)
});
b!(j_getName, |L, h| rets(L, "Controller"));
b!(j_isConnected, |L, h| retb(L, h.held.pad));
b!(j_getID, |L, h| ret2(L, 1.0, 1.0));
b!(j_isGamepad, |L, h| retb(L, true));
b!(j_getAxis, |L, h| {
    let i = opt(L, 2, 1.0) as usize;
    ret(L, crate::input::AXES.get(i.wrapping_sub(1)).map_or(0.0, |a| h.held.axis(a)) as f64)
});
b!(j_getAxes, |L, h| {
    for a in crate::input::AXES {
        lua_pushnumber(L, h.held.axis(a) as f64);
    }
    crate::input::AXES.len() as c_int
});
b!(j_getAxisCount, |L, h| ret(L, crate::input::AXES.len() as f64));

// ---------------------------------------------------------------- love.math

/// Ken Perlin's improved noise's permutation.
const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117,
    35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60, 211, 133, 230, 220, 105, 92, 41,
    55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64, 52, 217, 226,
    250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43,
    172, 9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107,
    49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
];

/// Improved Perlin noise in 3D, about -1..1.
fn perlin(x: f64, y: f64, z: f64) -> f64 {
    let p = |i: i64| PERM[(i & 255) as usize] as i64;
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let grad = |hash: i64, x: f64, y: f64, z: f64| {
        let h = hash & 15;
        let u = if h < 8 { x } else { y };
        let v = if h < 4 { y } else if h == 12 || h == 14 { x } else { z };
        (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
    };
    let (xi, yi, zi) = (x.floor() as i64, y.floor() as i64, z.floor() as i64);
    let (x, y, z) = (x - x.floor(), y - y.floor(), z - z.floor());
    let (u, v, w) = (fade(x), fade(y), fade(z));
    let a = p(xi) + yi;
    let (aa, ab) = (p(a) + zi, p(a + 1) + zi);
    let b = p(xi + 1) + yi;
    let (ba, bb) = (p(b) + zi, p(b + 1) + zi);
    let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
    lerp(
        w,
        lerp(v, lerp(u, grad(p(aa), x, y, z), grad(p(ba), x - 1.0, y, z)), lerp(u, grad(p(ab), x, y - 1.0, z), grad(p(bb), x - 1.0, y - 1.0, z))),
        lerp(v, lerp(u, grad(p(aa + 1), x, y, z - 1.0), grad(p(ba + 1), x - 1.0, y, z - 1.0)), lerp(u, grad(p(ab + 1), x, y - 1.0, z - 1.0), grad(p(bb + 1), x - 1.0, y - 1.0, z - 1.0))),
    )
}

b!(noise, |L, h| {
    let n = lua_gettop(L);
    if num(L, 1).is_none() {
        return bad(L, 1, "noise", "number");
    }
    let v: Vec<f64> = (1..=n.min(4)).map(|i| opt(L, i, 0.0)).collect();
    // 1–4 dimensions, 0–1 as LÖVE's; a fourth folds into the third.
    let (x, y, z) = match v.len() {
        1 => (v[0], 0.5, 0.25),
        2 => (v[0], v[1], 0.25),
        3 => (v[0], v[1], v[2]),
        _ => (v[0], v[1], v[2] + v[3] * std::f64::consts::GOLDEN_RATIO),
    };
    ret(L, (perlin(x, y, z) * 0.5 + 0.5).clamp(0.0, 1.0))
});

/// A polygon's points from `at`: a flat table or numbers.
unsafe fn poly(L: *mut lua_State, at: c_int, f: &str) -> Result<Vec<(f64, f64)>, c_int> {
    unsafe {
        let mut v = Vec::new();
        if lua_type(L, at) == LUA_TTABLE {
            for i in 1..=lua_objlen(L, at) {
                lua_rawgeti(L, at, i);
                v.push(num(L, -1));
                lua_pop(L, 1);
            }
        } else {
            for i in at..=lua_gettop(L) {
                v.push(num(L, i));
            }
        }
        let v: Option<Vec<f64>> = v.into_iter().collect();
        match v {
            Some(v) if v.len() >= 6 && v.len() % 2 == 0 => Ok(v.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()),
            _ => Err(err(L, &format!("{f} takes a polygon: three points or more, x1, y1, x2, y2, …"))),
        }
    }
}

fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

b!(isConvex, |L, h| {
    let p = match poly(L, 1, "isConvex") {
        Ok(p) => p,
        Err(r) => return r,
    };
    let n = p.len();
    let signs: Vec<f64> = (0..n).map(|i| cross(p[i], p[(i + 1) % n], p[(i + 2) % n])).filter(|c| c.abs() > 1e-12).collect();
    retb(L, signs.iter().all(|c| *c > 0.0) || signs.iter().all(|c| *c < 0.0))
});

b!(triangulate, |L, h| {
    let mut p = match poly(L, 1, "triangulate") {
        Ok(p) => p,
        Err(r) => return r,
    };
    // Ear clipping, counter-clockwise.
    let area: f64 = (0..p.len()).map(|i| { let (a, b) = (p[i], p[(i + 1) % p.len()]); a.0 * b.1 - b.0 * a.1 }).sum();
    if area < 0.0 {
        p.reverse();
    }
    let mut idx: Vec<usize> = (0..p.len()).collect();
    let mut tris = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (p[idx[(i + n - 1) % n]], p[idx[i]], p[idx[(i + 1) % n]]);
            cross(a, b, c) > 0.0 && idx.iter().all(|&j| { let q = p[j]; q == a || q == b || q == c || !(cross(a, b, q) >= 0.0 && cross(b, c, q) >= 0.0 && cross(c, a, q) >= 0.0) })
        });
        let Some(i) = ear else { return err(L, "triangulate: the polygon crosses itself") };
        tris.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
        idx.remove(i);
    }
    tris.push([idx[0], idx[1], idx[2]]);
    lua_createtable(L, tris.len() as c_int, 0);
    for (k, t) in tris.iter().enumerate() {
        lua_createtable(L, 6, 0);
        for (m, &j) in t.iter().enumerate() {
            lua_pushnumber(L, p[j].0);
            lua_rawseti(L, -2, m as c_int * 2 + 1);
            lua_pushnumber(L, p[j].1);
            lua_rawseti(L, -2, m as c_int * 2 + 2);
        }
        lua_rawseti(L, -2, k as c_int + 1);
    }
    1
});

b!(colorFromBytes, |L, h| {
    let n = lua_gettop(L).clamp(3, 4);
    for i in 1..=n {
        lua_pushnumber(L, (opt(L, i, 255.0) / 255.0).clamp(0.0, 1.0));
    }
    n
});
b!(colorToBytes, |L, h| {
    let n = lua_gettop(L).clamp(3, 4);
    for i in 1..=n {
        lua_pushnumber(L, (opt(L, i, 1.0).clamp(0.0, 1.0) * 255.0).round());
    }
    n
});

b!(newRandomGenerator, |L, h| {
    if h.rngs.len() >= 256 {
        return err(L, "newRandomGenerator: more than 256 in a round: make one and reuse it, rng:setSeed(seed) for each new sequence");
    }
    let seed = num(L, 1).map_or_else(|| h.rng.next_u32() as u64, |s| s as i64 as u64);
    h.rngs.push(Rng::new(seed));
    push_handle(L, tag::RNG, h.rngs.len() as u32 - 1)
});
b!(r_random, |L, h| match handle(L, 1, tag::RNG) {
    Some(i) => random_from(L, &mut h.rngs[i as usize], 2),
    None => bad(L, 1, "random", "RandomGenerator"),
});
b!(r_randomNormal, |L, h| match handle(L, 1, tag::RNG) {
    Some(i) => normal_from(L, &mut h.rngs[i as usize], 2),
    None => bad(L, 1, "randomNormal", "RandomGenerator"),
});
b!(r_setSeed, |L, h| match (handle(L, 1, tag::RNG), num(L, 2)) {
    (Some(i), Some(s)) => {
        h.rngs[i as usize] = Rng::new(s as i64 as u64);
        0
    }
    _ => bad(L, 2, "setSeed", "number"),
});
