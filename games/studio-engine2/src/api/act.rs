//! `act`, the hands a bot policy is given (docs/STUDIO-2.md §8.1, bot.rs):
//! `function bot(act)` in `game/bot.lua` says what a player would do this
//! frame, and the engine adds the person. Bot runs only: the table lives in
//! the registry and is passed to `bot`, never a global.
//!
//! - `act.tap(x, y, { size, at, hold, new })`: a press at (x, y) (canvas
//!   units, as `love.mouse` reports them) on a target `size` across
//!   (default 60), ideally `at` seconds from now (0), held `hold` (0.06);
//!   `new = false` for the next beat of a rhythm (no reaction, no lapse);
//! - `act.hold(x, y, { size, new })`: keep a finger down there (it presses,
//!   or drags there within `size / 2`, default 40); `act.release()` lifts it;
//! - `act.swipe(angle, length, { x, y, at, new })`: a flick (radians, 0 is
//!   right, y down), from (x, y) or where the finger is;
//! - `act.press(button, { at, hold, new })`: a controller button ("a", "b",
//!   "x", "y", "leftshoulder", "rightshoulder", "dpup", …);
//! - `act.stick(ax, ay, { new })`: the left stick, -1 to 1 each;
//! - `act.finger()` → x, y, down: where the finger is now;
//! - `act.hints()` → the hints on screen now (`ui.hint`, lessons' prompts):
//!   `{ text, ask, priority, x, y, age }` each, the most important first;
//! - `act.followHints()`: this frame, also do what they say, as the bot
//!   does by itself in a game with no bot.lua and no quick-path player;
//! - `act.preset`: the kind of player the run plays as ("first-timer",
//!   "casual", "good" or "expert"), for skill the engine can't add, such
//!   as knowing a quiz's answers or how fast someone reads.

use std::ffi::{CStr, c_int, c_void};

use super::*;
use crate::bot::{HOLD_SIZE, Intent, TAP_HOLD, TAP_SIZE};

/// The `act` table, made once a round VM in a bot run: its registry ref.
pub unsafe fn make(L: *mut lua_State, ud: *mut c_void) -> c_int {
    unsafe {
        lua_createtable(L, 0, 10);
        for (name, f) in [(c"tap", tap as RustFn), (c"hold", hold), (c"release", release), (c"swipe", swipe), (c"press", press), (c"stick", stick), (c"finger", finger), (c"hints", hints), (c"followHints", follow_hints)] {
            set_fn(L, ud, name, f);
        }
        push_str(L, crate::bot::preset().map_or("good", |s| s.name));
        lua_setfield(L, -2, c"preset".as_ptr());
        lua_setreadonly(L, -1, 1);
        let r = lua_ref(L, -1);
        lua_pop(L, 1);
        r
    }
}

/// An option from the table at `i`, if there is one.
unsafe fn onum(L: *mut lua_State, i: c_int, k: &CStr) -> Option<f64> {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return None;
        }
        lua_rawgetfield(L, i, k.as_ptr());
        let v = num(L, -1);
        lua_pop(L, 1);
        v
    }
}

unsafe fn obool(L: *mut lua_State, i: c_int, k: &CStr) -> Option<bool> {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return None;
        }
        lua_rawgetfield(L, i, k.as_ptr());
        let v = (lua_type(L, -1) == LUA_TBOOLEAN).then(|| lua_toboolean(L, -1) != 0);
        lua_pop(L, 1);
        v
    }
}

/// The options table at `i` is a table or absent.
unsafe fn opts_ok(L: *mut lua_State, i: c_int, f: &str) -> Result<(), c_int> {
    unsafe {
        match lua_type(L, i) {
            LUA_TTABLE | LUA_TNIL | -1 => Ok(()),
            _ => Err(bad(L, i, f, "options table")),
        }
    }
}

fn add(h: &mut Host, it: Intent) {
    if let Some(b) = &mut h.bot {
        b.intents.push(it);
    }
}

/// `act:tap(…)` (a colon) passes the table first.
unsafe fn colon(L: *mut lua_State, f: &str) -> Option<c_int> {
    unsafe { (lua_type(L, 1) == LUA_TTABLE).then(|| err(L, &format!("act.{f} takes a dot, not a colon: act.{f}(…)"))) }
}

b!(tap, |L, h| {
    if let Some(e) = colon(L, "tap") {
        return e;
    }
    let (Some(x), Some(y)) = (num(L, 1), num(L, 2)) else {
        return err(L, "act.tap(x, y, { size, at, hold, new }): x and y are canvas units, as love.mouse's");
    };
    if let Err(e) = opts_ok(L, 3, "act.tap") {
        return e;
    }
    let size = onum(L, 3, c"size").unwrap_or(TAP_SIZE as f64) as f32;
    let at = onum(L, 3, c"at").unwrap_or(0.0) as f32;
    let hold = onum(L, 3, c"hold").unwrap_or(TAP_HOLD as f64) as f32;
    let new = obool(L, 3, c"new").unwrap_or(true);
    add(h, Intent::Tap { x: x as f32, y: y as f32, size: size.max(1.0), at, hold, new });
    0
});

b!(hold, |L, h| {
    if let Some(e) = colon(L, "hold") {
        return e;
    }
    let (Some(x), Some(y)) = (num(L, 1), num(L, 2)) else {
        return err(L, "act.hold(x, y, { size, new }): x and y are canvas units, as love.mouse's");
    };
    if let Err(e) = opts_ok(L, 3, "act.hold") {
        return e;
    }
    let size = onum(L, 3, c"size").unwrap_or(HOLD_SIZE as f64) as f32;
    add(h, Intent::Hold { x: x as f32, y: y as f32, size: size.max(1.0), new: obool(L, 3, c"new") });
    0
});

b!(release, |L, h| {
    add(h, Intent::Release);
    0
});

b!(swipe, |L, h| {
    if let Some(e) = colon(L, "swipe") {
        return e;
    }
    let (Some(angle), Some(length)) = (num(L, 1), num(L, 2)) else {
        return err(L, "act.swipe(angle, length, { x, y, at, new }): the angle in radians (0 is right, y down) and the length in units");
    };
    if let Err(e) = opts_ok(L, 3, "act.swipe") {
        return e;
    }
    let from = onum(L, 3, c"x").zip(onum(L, 3, c"y")).map(|(x, y)| (x as f32, y as f32));
    let at = onum(L, 3, c"at").unwrap_or(0.0) as f32;
    add(h, Intent::Swipe { angle: angle as f32, length: length as f32, from, at, new: obool(L, 3, c"new").unwrap_or(true) });
    0
});

b!(press, |L, h| {
    if let Some(e) = colon(L, "press") {
        return e;
    }
    let Some(name) = st(L, 1) else {
        return bad(L, 1, "act.press", "button name");
    };
    let Some(button) = crate::bot::button(name) else {
        return err(L, &format!("act.press: no button \"{name}\" (a, b, x, y, leftshoulder, rightshoulder, dpup, dpdown, dpleft, dpright)"));
    };
    if let Err(e) = opts_ok(L, 2, "act.press") {
        return e;
    }
    let at = onum(L, 2, c"at").unwrap_or(0.0) as f32;
    let hold = onum(L, 2, c"hold").unwrap_or(TAP_HOLD as f64) as f32;
    add(h, Intent::Press { button, at, hold, new: obool(L, 2, c"new").unwrap_or(true) });
    0
});

b!(stick, |L, h| {
    if let Some(e) = colon(L, "stick") {
        return e;
    }
    let (Some(ax), Some(ay)) = (num(L, 1), num(L, 2)) else {
        return err(L, "act.stick(ax, ay, { new }): each -1 to 1 (y down)");
    };
    if let Err(e) = opts_ok(L, 3, "act.stick") {
        return e;
    }
    add(h, Intent::Stick { ax: ax as f32, ay: ay as f32, new: obool(L, 3, c"new").unwrap_or(true) });
    0
});

b!(finger, |L, h| {
    let (x, y, down) = h.bot.as_ref().map_or((0.0, 0.0, false), |b| b.finger);
    lua_pushnumber(L, x as f64);
    lua_pushnumber(L, y as f64);
    lua_pushboolean(L, down as c_int);
    3
});

b!(hints, |L, h| {
    let list = h.ui.hints.visible();
    lua_createtable(L, list.len() as c_int, 0);
    for (i, (hint, age)) in list.iter().enumerate() {
        lua_createtable(L, 0, 6);
        push_str(L, &hint.text);
        lua_setfield(L, -2, c"text".as_ptr());
        push_str(L, crate::api::ui::ask_name(hint.ask));
        lua_setfield(L, -2, c"ask".as_ptr());
        push_str(L, crate::api::ui::priority_name(hint.priority));
        lua_setfield(L, -2, c"priority".as_ptr());
        if let Some((x, y)) = hint.at {
            lua_pushnumber(L, x as f64);
            lua_setfield(L, -2, c"x".as_ptr());
            lua_pushnumber(L, y as f64);
            lua_setfield(L, -2, c"y".as_ptr());
        }
        lua_pushnumber(L, *age as f64);
        lua_setfield(L, -2, c"age".as_ptr());
        lua_rawseti(L, -2, i as c_int + 1);
    }
    1
});

b!(follow_hints, |L, h| {
    if let Some(b) = &mut h.bot {
        b.follow_hints = true;
    }
    0
});
