//! The quick path's Lua (docs/STUDIO-2.md §5 [quick]): `kind`, `spawn`,
//! `all`, `count`, `nearest`, `speedUp`, `nextLevel`, and the engine's side
//! of `on`, `hurt` and `spawnEvery` (the prelude's). The behaviours run in
//! Rust (quick.rs); this hands their frame to Lua as events.

use std::ffi::{c_int, c_void};

use maimbrain::scene::{Kind, NodeId};

use super::node::{apply_props, build_map, make_thing, node_at, push_node, things};
use super::*;
use crate::input::LoveEvent;
use crate::quick::{Beh, Input};

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        for (name, f) in [(c"kind", kind as RustFn), (c"spawn", spawn), (c"all", all), (c"count", count), (c"nearest", nearest), (c"speedUp", speed_up), (c"nextLevel", next_level)] {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        register(
            L,
            ud,
            "__engine",
            &[(c"quick", quick), (c"watch", watch), (c"safe", safe), (c"level", level), (c"sfx", sfx), (c"invincible", invincible), (c"subject", subject)],
        );
        sv_push_rfn(L, spawn_every, ud, c"spawnEvery".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"spawnEvery".as_ptr());
        sv_push_rfn(L, stop_spawner, ud, c"stop".as_ptr());
        super::host(ud).stop_ref = lua_ref(L, -1);
        lua_pop(L, 1);
    }
}

/// A string field of the table at `t`.
unsafe fn sfield(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<String> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let s = st(L, -1).map(str::to_string);
        lua_pop(L, 1);
        s
    }
}

unsafe fn nfield(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<f64> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let v = num(L, -1);
        lua_pop(L, 1);
        v
    }
}

// `kind(name, { w, h, color, shape, image, behaviour(s), knobs, hitbox, spawns, order })`:
// a kind without an image, or changes to one.
b!(kind, |L, h| {
    let Some(name) = st(L, 1).map(str::to_string) else { return bad(L, 1, "kind", "name") };
    let t = lua_type(L, 2);
    if t != LUA_TTABLE && t != LUA_TNIL && t != -1 {
        return bad(L, 2, "kind", "table");
    }
    let mut behs: Option<Vec<Beh>> = None;
    let mut color = None;
    let mut knobs = Vec::new();
    let mut hitbox = None;
    let mut image = None;
    if t == LUA_TTABLE {
        if let Some(i) = sfield(L, 2, c"image") {
            match h.project.image(&i) {
                Some(x) => image = Some(x),
                None => return err(L, &format!("kind: no image \"{i}\": add images/{i}.png")),
            }
        }
        let mut list: Vec<String> = Vec::new();
        if let Some(b) = sfield(L, 2, c"behaviour") {
            list.push(b);
        }
        lua_rawgetfield(L, 2, c"behaviours".as_ptr());
        if lua_type(L, -1) == LUA_TTABLE {
            for i in 1..=lua_objlen(L, -1) {
                lua_rawgeti(L, -1, i);
                if let Some(s) = st(L, -1) {
                    list.push(s.to_string());
                }
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
        if !list.is_empty() {
            let mut v = Vec::new();
            for b in &list {
                match Beh::named(b) {
                    Some(x) => v.push(x),
                    None => return err(L, &format!("kind: {}", Beh::unknown(b))),
                }
            }
            behs = Some(v);
        }
        lua_rawgetfield(L, 2, c"color".as_ptr());
        if lua_type(L, -1) != LUA_TNIL {
            match color_at(L, -1, false) {
                Ok(c) => color = Some(c),
                Err(m) => {
                    lua_pop(L, 1);
                    return err(L, &format!("kind: color: {m}"));
                }
            }
        }
        lua_pop(L, 1);
        lua_rawgetfield(L, 2, c"knobs".as_ptr());
        if lua_type(L, -1) == LUA_TTABLE {
            lua_pushnil(L);
            while lua_next(L, -2) != 0 {
                if let (Some(k), Some(v)) = (st(L, -2), num(L, -1)) {
                    knobs.push((k.to_string(), v));
                }
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
        lua_rawgetfield(L, 2, c"hitbox".as_ptr());
        if lua_type(L, -1) == LUA_TTABLE {
            let mut r = [0.0f32; 4];
            for (i, k) in [c"x", c"y", c"w", c"h"].iter().enumerate() {
                r[i] = nfield(L, -1, k).or_else(|| {
                    lua_rawgeti(L, -1, i as c_int + 1);
                    let v = num(L, -1);
                    lua_pop(L, 1);
                    v
                }).unwrap_or(0.0) as f32;
            }
            hitbox = Some(r);
        }
        lua_pop(L, 1);
    }
    let k = h.quick.kind_mut(&name);
    if t == LUA_TTABLE {
        if let Some(w) = nfield(L, 2, c"w") {
            k.w = w as f32;
        }
        if let Some(x) = nfield(L, 2, c"h") {
            k.h = x as f32;
        }
        if let Some(o) = nfield(L, 2, c"order") {
            k.order = o as f32;
        }
        if let Some(s) = sfield(L, 2, c"shape") {
            k.circle = s == "circle";
        }
        if let Some(s) = sfield(L, 2, c"spawns") {
            k.spawns = Some(s);
        }
    }
    if let Some(i) = image {
        k.image = Some(i);
    }
    if let Some(c) = color {
        k.color = c;
    }
    if let Some(b) = behs {
        if b.iter().any(|b| b.player()) && k.order == 0.0 {
            k.order = 1.0;
        }
        k.behs = b;
    }
    for (n, v) in knobs {
        k.knobs.retain(|x| x.0 != n);
        k.knobs.push((n, v));
    }
    if hitbox.is_some() {
        k.hitbox = hitbox;
    }
    0
});

// `spawn(kind, props)` → the thing.
b!(spawn, |L, h| {
    let Some(name) = st(L, 1).map(str::to_string) else { return bad(L, 1, "spawn", "kind name") };
    let id = match make_thing(L, h, &name, &[]) {
        Ok(id) => id,
        Err(m) => return err(L, &format!("spawn: {m}")),
    };
    if lua_type(L, 2) == LUA_TTABLE
        && let Err(m) = apply_props(L, h, id, 2)
    {
        return err(L, &format!("spawn(\"{name}\", {{…}}): {m}"));
    }
    push_node(L, h, id);
    1
});

unsafe fn push_list(L: *mut lua_State, h: &Host, ids: &[NodeId]) -> c_int {
    unsafe {
        lua_createtable(L, ids.len() as c_int, 0);
        for (i, id) in ids.iter().enumerate() {
            push_node(L, h, *id);
            lua_rawseti(L, -2, i as c_int + 1);
        }
        1
    }
}

// `all(kind)`: the nodes tagged with it, in made order (`all()`: every thing).
b!(all, |L, h| {
    match st(L, 1) {
        Some(k) => {
            let ids = h.scene.all(k);
            push_list(L, h, &ids)
        }
        None => {
            let ids: Vec<NodeId> = h.quick.things.iter().map(|t| t.node).filter(|n| h.scene.alive(*n)).collect();
            push_list(L, h, &ids)
        }
    }
});

b!(count, |L, h| {
    let Some(k) = st(L, 1) else { return bad(L, 1, "count", "kind name") };
    ret(L, h.scene.all(k).len() as f64)
});

// `nearest(kind, x, y)`: the closest of a kind, or nil.
b!(nearest, |L, h| {
    let Some(k) = st(L, 1) else { return bad(L, 1, "nearest", "kind name") };
    let (x, y) = (opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    let best = h.scene.all(k).into_iter().filter_map(|id| h.scene.get(id).map(|n| (id, (n.x - x).powi(2) + (n.y - y).powi(2)))).min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((id, _)) => {
            push_node(L, h, id);
            1
        }
        None => {
            lua_pushnil(L);
            1
        }
    }
});

// `speedUp(f)`: the world (not the players) goes f times faster.
b!(speed_up, |L, h| match num(L, 1) {
    Some(f) if f > 0.0 && f.is_finite() => {
        h.quick.speed *= f;
        0
    }
    _ => err(L, "speedUp takes a number above 0 (1.1 is 10% faster)"),
});

// `nextLevel(name)`: the next map in order (or that one); past the last,
// the round ends, won. Everything the quick path made goes.
b!(next_level, |L, h| {
    let p = h.project.clone();
    let current = h.level_name.clone();
    let target = match st(L, 1) {
        Some(n) => match p.map(n) {
            Some(i) => Some(i),
            None => return err(L, &format!("nextLevel: no map \"{n}\" ({})", p.maps.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", "))),
        },
        None => {
            let at = current.as_deref().and_then(|c| p.map(c));
            match at {
                Some(i) if i + 1 < p.maps.len() => Some(i + 1),
                Some(_) => None,
                None if !p.maps.is_empty() => Some(0),
                None => None,
            }
        }
    };
    let Some(map) = target else {
        h.over = Some(Over { heading: None, cause: Some("levels".into()), won: true });
        return 0;
    };
    // The old level and its things go.
    let old = h.quick.level.take();
    let gone: Vec<NodeId> = h.quick.things.iter().map(|t| t.node).collect();
    h.quick.spawners.clear();
    let (mut x, mut y, mut set) = (0.0, 0.0, None);
    if let Some(l) = old {
        if let Some(n) = h.scene.get(l) {
            (x, y) = (n.x, n.y);
        }
        set = h.tilemaps.iter().find(|t| t.0 == l).and_then(|t| t.2);
    }
    for id in gone.into_iter().chain(old) {
        super::node::destroy_node(L, h, id);
    }
    if p.maps[map].set.is_some() {
        set = p.maps[map].set;
    }
    let id = match h.scene.add(Kind::Tilemap, None) {
        Ok(id) => id,
        Err(_) => return err(L, "nextLevel: too many nodes"),
    };
    if let Some(n) = h.scene.node_mut(id) {
        (n.x, n.y) = (x, y);
    }
    build_map(h, id, map, set);
    if let Err(m) = things(L, h, id) {
        return err(L, &format!("nextLevel: {m}"));
    }
    0
});

// `__engine.quick(dt)`: the quick path's frame. Returns its events as
// `{ name, a, b or false, value }`.
b!(quick, |L, h| {
    let dt = opt(L, 1, 0.0) as f32;
    let (w, hh) = (h.scene.width, h.scene.height);
    let cam = h.scene.camera;
    let world = |x: f32, y: f32| cam.to_world(w, hh, x, y);
    let mut input = Input::default();
    for e in &h.frame_events {
        match e {
            LoveEvent::MousePressed { x, y, button: 1, .. } => {
                input.tap = Some(world(*x, *y));
                input.press = input.tap;
            }
            // A finger that isn't the mouse (the second one down): it can flap and jump.
            LoveEvent::TouchPressed { x, y, .. } if input.press.is_none() => input.press = Some(world(*x, *y)),
            LoveEvent::MouseReleased { button: 1, .. } => input.release = true,
            LoveEvent::KeyPressed(k) if matches!(*k, "space" | "up" | "w" | "z") => input.jump = true,
            LoveEvent::KeyReleased(k) if matches!(*k, "space" | "up" | "w" | "z") => input.jumpup = true,
            LoveEvent::GamepadPressed(b) if matches!(*b, "a" | "b") => input.jump = true,
            LoveEvent::GamepadReleased(b) if matches!(*b, "a" | "b") => input.jumpup = true,
            _ => {}
        }
    }
    if h.held.mouse[0] {
        input.down = Some(world(h.held.mx, h.held.my));
    }
    (input.ax, input.ay) = h.held.steer();
    input.tilt = h.sensors.now.steer();
    let mut q = std::mem::take(&mut h.quick);
    // The walking finger's flick up and an early lift, on the screen (the
    // camera moving under the finger isn't a flick).
    let held = h.held.mouse[0].then_some((h.held.mx, h.held.my));
    (input.flick, input.early_lift) = q.gesture.feed(dt, held, input.tap.is_some(), hh);
    let (mut events, made) = q.step(&mut h.scene, &input, dt, &mut h.rng);
    h.quick = q;
    for m in made {
        match make_thing(L, h, &m.kind, &m.props) {
            Ok(id) => {
                if let Some(s) = m.shooter {
                    events.push(crate::quick::Event { name: "shoot", a: s, b: Some(id), value: 0.0 });
                }
            }
            Err(e) => return err(L, &format!("a spawner: {e}")),
        }
    }
    // A level bigger than the view: the camera follows the first player,
    // unless the game moves the camera itself.
    if !h.camera_touched
        && h.libs.follow.is_none()
        && let Some(l) = h.quick.level
        && let Some((mw, mh)) = h.scene.get(l).and_then(|n| n.tilemap.as_ref().map(|m| m.size()))
        && (mw > w || mh > hh)
        && let Some(p) = h.quick.players().next().map(|t| t.node)
    {
        let (x, y) = h.scene.world_position(l);
        h.libs.follow = Some(crate::libs::Follow { node: p, lag: 0.0, lead: 0.0, deadzone: (0.0, 0.0), bounds: Some([x, y, mw, mh]), lead_x: 0.0, at_edge: 0, reached_edge: false });
    }
    if h.bot.is_some() {
        crate::autoplay::heard(h, &events);
    }
    lua_createtable(L, events.len() as c_int, 0);
    for (i, e) in events.iter().enumerate() {
        lua_createtable(L, 4, 0);
        push_str(L, e.name);
        lua_rawseti(L, -2, 1);
        push_node(L, h, e.a);
        lua_rawseti(L, -2, 2);
        match e.b {
            Some(b) => push_node(L, h, b),
            None => lua_pushboolean(L, 0),
        }
        lua_rawseti(L, -2, 3);
        lua_pushnumber(L, e.value);
        lua_rawseti(L, -2, 4);
        lua_rawseti(L, -2, i as c_int + 1);
    }
    1
});

// `__engine.watch(event, a, b)`: what Rust looks for: an overlap pair
// (returns its index), or the kinds a `tick` or `leave` handler has.
b!(watch, |L, h| {
    let ev = st(L, 1).unwrap_or("");
    let a = st(L, 2).unwrap_or("*").to_string();
    match ev {
        "overlap" => {
            let b = st(L, 3).unwrap_or("*").to_string();
            let q = &mut h.quick;
            let i = match q.overlaps.iter().position(|p| p.0 == a && p.1 == b) {
                Some(i) => i,
                None => {
                    q.overlaps.push((a, b));
                    q.overlaps.len() - 1
                }
            };
            ret(L, i as f64 + 1.0)
        }
        "tick" => {
            if !h.quick.ticks.contains(&a) {
                h.quick.ticks.push(a);
            }
            0
        }
        _ => {
            if !h.quick.leaves.contains(&a) {
                h.quick.leaves.push(a);
            }
            0
        }
    }
});

// `__engine.safe(node)`: whether a hurt takes a life now (and it starts blinking).
b!(safe, |L, h| match node_at(L, 1) {
    Some(n) if h.scene.alive(n) => {
        let hurt = h.quick.hurt(n);
        if hurt && let Some(b) = &mut h.bot {
            let by = b.hurt_by.clone().unwrap_or_else(|| "hurt".into());
            b.events.push((format!("hurt {by}"), h.time));
        }
        retb(L, hurt)
    }
    _ => retb(L, false),
});

// `__engine.subject(round.subject)`, from the prelude's step when it
// changes: the node the readability check measures (headless.rs). Anything
// but a live node clears it, and the check falls back on its default.
b!(subject, |L, h| {
    h.subject = match node_at(L, 1) {
        Some(n) => Some(super::Subject::D2(n)),
        #[cfg(three)]
        None => super::three::node_at(L, 1).map(super::Subject::D3),
        #[cfg(not(three))]
        None => None,
    };
    0
});

// `__engine.spawnEvery(kind, secs, props)` → the spawner's id.
b!(spawn_every, |L, h| {
    let Some(k) = st(L, 1).map(str::to_string) else { return bad(L, 1, "spawnEvery", "kind name") };
    if h.quick.kind(&k).is_none() {
        return err(L, &format!("spawnEvery: no kind named '{k}' (add images/{k}.png, or kind(\"{k}\", {{ … }}))"));
    }
    let Some(secs) = num(L, 2).filter(|s| *s > 0.0) else { return err(L, "spawnEvery needs a positive number of seconds") };
    let mut props = Vec::new();
    match lua_type(L, 3) {
        LUA_TTABLE => {
            lua_pushnil(L);
            while lua_next(L, 3) != 0 {
                match (st(L, -2), num(L, -1)) {
                    (Some(key), Some(v)) => props.push((key.to_string(), v)),
                    _ => {
                        lua_pop(L, 2);
                        return err(L, "spawnEvery's props are names = numbers ({ x = 40, vy = 120 }); for more, timer.every(secs, function() spawn(kind, { … }) end)");
                    }
                }
                lua_pop(L, 1);
            }
        }
        LUA_TNIL | -1 => {}
        _ => return err(L, "spawnEvery's props are a table of numbers; for more, timer.every(secs, function() spawn(kind, { … }) end)"),
    }
    props.sort_by(|a, b| a.0.cmp(&b.0));
    let id = h.quick.spawn_every(&k, secs, None, 0.0, props);
    // A handle: `handle:stop()`.
    lua_createtable(L, 0, 2);
    lua_pushnumber(L, id as f64);
    lua_setfield(L, -2, c"id".as_ptr());
    lua_rawgeti(L, LUA_REGISTRYINDEX, h.stop_ref);
    lua_setfield(L, -2, c"stop".as_ptr());
    1
});

b!(stop_spawner, |L, h| {
    let id = if lua_type(L, 1) == LUA_TTABLE { nfield(L, 1, c"id").unwrap_or(0.0) as u32 } else { opt(L, 1, 0.0) as u32 };
    for s in h.quick.spawners.iter_mut().filter(|s| s.id == id) {
        s.on = false;
    }
    0
});

b!(level, |L, h| match &h.level_name {
    Some(n) => rets(L, n),
    None => {
        lua_pushnil(L);
        1
    }
});

// `__engine.sfx(name)`: an event's sound, if the project has one by its name.
b!(sfx, |L, h| {
    // `mb play`'s trace (`studio_trace`, as engine 1.0's): a round's quick-path
    // events as `studio-event <time> <event> <tag> <other> <x> <y>`, for play_test's
    // events and upgrade_engine's comparison.
    if !h.mute
        && h.trace
        && h.over.is_none()
        && h.last_lives > 0.0
        && let Some(ev) = st(L, 1)
    {
        let tag = |i| node_at(L, i).and_then(|n| h.scene.get(n).or_else(|| h.graveyard.iter().find(|(id, _)| *id == n).map(|(_, x)| x)));
        let (a, b) = (tag(2), tag(3));
        let word = |n: Option<&maimbrain::scene::Node>| n.map(|n| if n.tag.is_empty() { n.name.clone() } else { n.tag.clone() }).filter(|s| !s.is_empty()).unwrap_or_else(|| "-".into());
        // A thing's events only (not the pointer's taps and swipes).
        if let Some(n) = a {
            let (x, y) = (n.x, n.y);
            maimbrain::sys::log(maimbrain::sys::Level::Debug, &format!("studio-event {:.3} {ev} {} {} {x:.2} {y:.2}", maimbrain::sys::time(), word(a), word(b)));
        }
    }
    if let Some(name) = st(L, 1)
        && !h.mute
        && let Some(i) = h.project.sound(name)
        && let Some(snd) = h.sounds.get(i).copied().flatten()
    {
        snd.play(h.volume, 0.0, 1.0);
    }
    0
});

b!(invincible, |L, h| retb(L, h.quick.invincible));
