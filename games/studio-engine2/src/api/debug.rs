//! The game's debug-menu items (docs/STUDIO-2.md §5 [debug], as 1.0's):
//! `debug.action`, `debug.toggle`, `debug.number`, `debug.choice` and
//! `debug.invincible()`, beside Luau's own `debug.traceback` and
//! `debug.info`. The engine shows them on the platform's menu
//! (debugmenu.rs) and calls their functions when they're picked.

use std::ffi::{c_int, c_void};

use super::*;

/// One item: its kind ("action", "toggle", "number", "choice"), label,
/// value and range, options, and its function (a registry ref).
#[derive(Clone, Debug)]
pub struct DebugItem {
    pub kind: &'static str,
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub options: Vec<String>,
    pub f: c_int,
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "debug", &[(c"action", action), (c"toggle", toggle), (c"number", number), (c"choice", choice), (c"invincible", invincible), (c"mark", mark)]);
    }
}

unsafe fn item(L: *mut lua_State, h: &mut Host, kind: &'static str, fi: c_int) -> Result<DebugItem, c_int> {
    unsafe {
        let f = format!("debug.{kind}");
        let Some(label) = st(L, 1).map(str::to_string) else { return Err(bad(L, 1, &f, "label")) };
        if lua_type(L, fi) != LUA_TFUNCTION {
            return Err(bad(L, fi, &f, "function"));
        }
        if h.debug_items.len() >= 60 {
            return Err(err(L, &format!("{f}: 60 items at most")));
        }
        lua_pushvalue(L, fi);
        let r = lua_ref(L, -1);
        lua_pop(L, 1);
        Ok(DebugItem { kind, label, value: 0.0, min: 0.0, max: 0.0, step: 0.0, options: Vec::new(), f: r })
    }
}

// debug.action(label, fn)
b!(action, |L, h| match item(L, h, "action", 2) {
    Ok(it) => {
        h.debug_items.push(it);
        0
    }
    Err(r) => r,
});

// debug.toggle(label, fn, on)
b!(toggle, |L, h| match item(L, h, "toggle", 2) {
    Ok(mut it) => {
        it.value = (lua_toboolean(L, 3) != 0) as u8 as f64;
        h.debug_items.push(it);
        0
    }
    Err(r) => r,
});

// debug.number(label, value, fn, min, max, step)
b!(number, |L, h| match item(L, h, "number", 3) {
    Ok(mut it) => {
        (it.value, it.min, it.max, it.step) = (opt(L, 2, 0.0), opt(L, 4, 0.0), opt(L, 5, 100.0), opt(L, 6, 1.0));
        h.debug_items.push(it);
        0
    }
    Err(r) => r,
});

// debug.choice(label, options, fn, index)
b!(choice, |L, h| {
    if lua_type(L, 2) != LUA_TTABLE || lua_objlen(L, 2) == 0 {
        return err(L, "debug.choice needs a list of options");
    }
    match item(L, h, "choice", 3) {
        Ok(mut it) => {
            for i in 1..=lua_objlen(L, 2).min(20) {
                lua_rawgeti(L, 2, i);
                it.options.push(to_s(L, -1));
                lua_pop(L, 1);
            }
            it.value = (opt(L, 4, 1.0) - 1.0).max(0.0);
            h.debug_items.push(it);
            0
        }
        Err(r) => r,
    }
});

// debug.invincible(): the menu's Invincible is on (a game that takes lives itself skips it).
b!(invincible, |L, h| retb(L, h.quick.invincible));

// `debug.mark(label)`: this frame does something sudden on purpose (a cut,
// a respawn, a dash): the smoothness check lists a jump within two frames
// of it as marked, not as a problem. Nothing else sees it.
b!(mark, |L, h| {
    let Some(label) = st(L, 1).filter(|l| !l.is_empty() && l.len() <= 64) else { return err(L, "debug.mark(label) takes a short name for what happens (\"cut\", \"respawn\")") };
    if h.marks.len() < 8 {
        h.marks.push(label.to_string());
    }
    0
});
