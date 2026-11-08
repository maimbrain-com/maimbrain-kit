//! Saves and progress (docs/STUDIO-2.md §5 [saves]), and the boards past
//! board 0 (`round.post`, `round.board`; boards.rs): `save.get`/`save.set`
//! on the SDK's store, and `progress.*` on its verified progress (SPEC §5.8),
//! which the server replays each run to check. Values are numbers,
//! strings, booleans and plain tables, written in one byte format with
//! tables' keys sorted, so the same value is always the same bytes (a
//! progress write the replay can match). Every NaN is saved as one NaN,
//! whatever its sign and payload bits were, since those differ between CPUs.

use std::ffi::{c_int, c_void};

use maimbrain::{progress, store};

use super::*;

/// Saved data's names, under the store's (1.0 used `studio:` too).
const PREFIX: &str = "studio:";
/// Tables inside tables, at most.
const DEPTH: usize = 32;
/// Every NaN is saved as this one (a quiet NaN, sign clear).
const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "save", &[(c"get", save_get), (c"set", save_set)]);
        register(L, ud, "progress", &[(c"get", progress_get), (c"set", progress_set), (c"checkpoint", checkpoint), (c"resume", resume), (c"clear", clear)]);
        // round.post and round.board (the prelude puts them on `round`).
        register(L, ud, "__engine", &[(c"post", post), (c"board", board)]);
    }
}

/// The value at stack index `i`, as bytes.
unsafe fn encode(L: *mut lua_State, i: c_int, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
    unsafe {
        match lua_type(L, i) {
            LUA_TBOOLEAN => out.extend([b'b', (lua_toboolean(L, i) != 0) as u8]),
            LUA_TNUMBER => {
                // One NaN: its sign and payload bits differ between CPUs (an ARM
                // phone and the server's x86 replay), and progress must match.
                let v = lua_tonumber(L, i);
                let v = if v.is_nan() { f64::from_bits(CANONICAL_NAN) } else { v };
                out.push(b'n');
                out.extend(v.to_le_bytes());
            }
            LUA_TSTRING => {
                let s = lua_tostr(L, i).unwrap_or_default();
                out.push(b's');
                out.extend((s.len() as u32).to_le_bytes());
                out.extend(s);
            }
            LUA_TTABLE => {
                if depth >= DEPTH {
                    return Err(format!("tables nest at most {DEPTH} deep (or one holds itself)"));
                }
                // Each level holds a key and a value on the stack: room for them
                // (a binding is given 20 slots, and past them pushes are unchecked).
                if lua_checkstack(L, 4) == 0 {
                    return Err("saved tables nest too deep".into());
                }
                let t = if i < 0 { lua_gettop(L) + i + 1 } else { i };
                // Each entry's key and value, encoded; then sorted by key.
                let mut entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
                lua_pushnil(L);
                while lua_next(L, t) != 0 {
                    let mut k = Vec::new();
                    match lua_type(L, -2) {
                        LUA_TNUMBER | LUA_TSTRING | LUA_TBOOLEAN => encode(L, -2, &mut k, depth + 1)?,
                        _ => {
                            let what = type_name(L, -2);
                            lua_pop(L, 2);
                            return Err(format!("a saved table's keys are numbers or strings, not {what}"));
                        }
                    }
                    let mut v = Vec::new();
                    if let Err(e) = encode(L, -1, &mut v, depth + 1) {
                        lua_pop(L, 2);
                        return Err(e);
                    }
                    entries.push((k, v));
                    lua_pop(L, 1);
                }
                entries.sort();
                out.push(b't');
                out.extend((entries.len() as u32).to_le_bytes());
                for (k, v) in entries {
                    out.extend(k);
                    out.extend(v);
                }
            }
            LUA_TNIL => out.push(b'z'),
            _ => return Err(format!("saves keep numbers, strings, true/false and plain tables, not {}", type_name(L, i))),
        }
        Ok(())
    }
}

/// Pushes the value at the start of `b`; returns the bytes it used.
unsafe fn decode(L: *mut lua_State, b: &[u8]) -> Option<usize> {
    unsafe {
        let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
        match *b.first()? {
            b'b' => {
                lua_pushboolean(L, (*b.get(1)? != 0) as c_int);
                Some(2)
            }
            b'n' => {
                lua_pushnumber(L, f64::from_le_bytes(b.get(1..9)?.try_into().ok()?));
                Some(9)
            }
            b's' => {
                let n = u32_at(1)?;
                let s = b.get(5..5 + n)?;
                lua_pushlstring(L, s.as_ptr() as *const _, s.len());
                Some(5 + n)
            }
            b'z' => {
                lua_pushnil(L);
                Some(1)
            }
            b't' => {
                let n = u32_at(1)?;
                // This level's table, key and value (see encode).
                if lua_checkstack(L, 4) == 0 {
                    return None;
                }
                lua_createtable(L, 0, n.min(1024) as c_int);
                let mut at = 5;
                for _ in 0..n {
                    let Some(k) = decode(L, &b[at..]) else {
                        lua_pop(L, 1);
                        return None;
                    };
                    at += k;
                    let Some(v) = decode(L, &b[at..]) else {
                        lua_pop(L, 2);
                        return None;
                    };
                    at += v;
                    lua_rawset(L, -3);
                }
                Some(at)
            }
            _ => None,
        }
    }
}

unsafe fn key_arg(L: *mut lua_State, f: &str, max: usize) -> Result<String, c_int> {
    unsafe {
        let Some(k) = st(L, 1) else { return Err(bad(L, 1, f, "key (a string)")) };
        if k.is_empty() || k.len() > max {
            return Err(err(L, &format!("{f}: keys are 1 to {max} characters, got \"{k}\"")));
        }
        Ok(k.to_string())
    }
}

unsafe fn push_saved(L: *mut lua_State, bytes: Option<Vec<u8>>) -> c_int {
    unsafe {
        let Some(b) = bytes.filter(|b| !b.is_empty()) else {
            lua_pushnil(L);
            return 1;
        };
        // Engine 1.0's text first (a game moved up keeps its players'
        // saves, docs/STUDIO-2.md §4.10): it's never this format's bytes.
        if push_text_save(L, &b) {
            return 1;
        }
        let top = lua_gettop(L);
        match decode(L, &b) {
            Some(n) if n == b.len() => 1,
            _ => {
                lua_settop(L, top);
                lua_pushnil(L);
                1
            }
        }
    }
}

/// Engine 1.0's saved value, as its `save()` wrote it: `b:1`, `n:3.5` or
/// `s:text`. Pushes it and says so; false for anything else. This format
/// can't be mistaken for one: its booleans are `b` and a 0 or 1 byte, never
/// `:`, and a number or string here (`n`, `s` then `:`) would need the
/// rest of its bytes to be exactly 1.0's text, checked below.
unsafe fn push_text_save(L: *mut lua_State, b: &[u8]) -> bool {
    unsafe {
        let (Some(&kind), Some(b':')) = (b.first(), b.get(1)) else { return false };
        let Ok(text) = std::str::from_utf8(&b[2..]) else { return false };
        match kind {
            b'b' if text == "1" || text == "0" => lua_pushboolean(L, (text == "1") as c_int),
            // A number as Rust and Lua print one, and nothing else (9 bytes
            // of this format's would need 7 of its float's bytes to be digits).
            b'n' if !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit() || b"+-.eEinfa".contains(&c)) => match text.parse::<f64>() {
                Ok(v) => lua_pushnumber(L, v),
                Err(_) => return false,
            },
            // This format's string is `s` and a 4-byte length: `s:` then
            // three more length bytes that are all text, and a length that
            // matches, never happen together.
            b's' if b.len() < 5 || u32::from_le_bytes(b[1..5].try_into().unwrap()) as usize != b.len() - 5 => {
                lua_pushlstring(L, text.as_ptr() as *const _, text.len())
            }
            _ => return false,
        }
        true
    }
}

unsafe fn value_arg(L: *mut lua_State, f: &str) -> Result<Vec<u8>, c_int> {
    unsafe {
        if lua_type(L, 2) == LUA_TNIL || lua_gettop(L) < 2 {
            return Ok(Vec::new());
        }
        let mut b = Vec::new();
        encode(L, 2, &mut b, 0).map_err(|e| err(L, &format!("{f}: {e}")))?;
        Ok(b)
    }
}

// `save.get(key)` → the value, or nil.
b!(save_get, |L, h| {
    let k = match key_arg(L, "save.get", 64 - PREFIX.len()) {
        Ok(k) => k,
        Err(r) => return r,
    };
    push_saved(L, store::get(&format!("{PREFIX}{k}")))
});

// `save.set(key, value)`: nil deletes it.
b!(save_set, |L, h| {
    let k = match key_arg(L, "save.set", 64 - PREFIX.len()) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let b = match value_arg(L, "save.set") {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !store::set(&format!("{PREFIX}{k}"), &b) {
        return err(L, "save.set: over the 256 KB a game can save");
    }
    0
});

b!(progress_get, |L, h| {
    let k = match key_arg(L, "progress.get", 55) {
        Ok(k) => k,
        Err(r) => return r,
    };
    push_saved(L, progress::get(&k))
});

// `progress.set(key, value)`: verified by the server's replay of the run.
b!(progress_set, |L, h| {
    let k = match key_arg(L, "progress.set", 55) {
        Ok(k) => k,
        Err(r) => return r,
    };
    if k == "run" {
        return err(L, "progress.set: \"run\" is the checkpoint's: use progress.checkpoint(state)");
    }
    let b = match value_arg(L, "progress.set") {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !progress::set(&k, &b) {
        return err(L, "progress.set: over the 64 KB of progress a game keeps");
    }
    0
});

// `progress.checkpoint(state)`: where a long run is, to resume in a later session.
b!(checkpoint, |L, h| {
    let mut b = Vec::new();
    if let Err(e) = encode(L, 1, &mut b, 0) {
        return err(L, &format!("progress.checkpoint: {e}"));
    }
    if !progress::save_checkpoint(&b) {
        return err(L, "progress.checkpoint: over the 64 KB of progress a game keeps");
    }
    0
});

b!(resume, |L, h| push_saved(L, progress::checkpoint()));

b!(clear, |L, h| {
    progress::clear_checkpoint();
    0
});

/// The board `round.post` or `round.board` names (argument 1), or the error.
unsafe fn named_board(L: *mut lua_State, h: &Host, f: &str) -> Result<crate::boards::Board, c_int> {
    unsafe {
        let Some(id) = st(L, 1) else { return Err(bad(L, 1, f, "board id (a string, its id in studio.toml)")) };
        match h.project.boards.iter().find(|b| !b.id.is_empty() && b.id == id) {
            Some(b) => Ok(b.clone()),
            None => {
                let ids: Vec<String> = h.project.boards.iter().filter(|b| !b.id.is_empty()).map(|b| format!("\"{}\"", b.id)).collect();
                let have = if ids.is_empty() { "none (add a [[scores]] board with an id after the first)".to_string() } else { ids.join(", ") };
                Err(err(L, &format!("{f}: studio.toml has no board \"{id}\" (its boards with ids: {have})")))
            }
        }
    }
}

// `round.post(id, value)`: the round's best on that board goes up when its
// game-over card does.
b!(post, |L, h| {
    let b = match named_board(L, h, "round.post") {
        Ok(b) => b,
        Err(r) => return r,
    };
    if b.board == 0 {
        return err(L, &format!("round.post: \"{}\" is board 0, round.score's: set round.score", b.id));
    }
    if b.daily {
        return err(L, &format!("round.post: \"{}\" is the daily board: round.score goes there in a daily round (round.daily)", b.id));
    }
    let Some(v) = num(L, 2).filter(|v| v.is_finite()) else { return bad(L, 2, "round.post", "value (a number)") };
    h.boards.post(&b, v.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64);
    0
});

// `round.board(id)`: that board's leaderboard (the round's own without an id).
b!(board, |L, h| {
    let n = if lua_type(L, 1) <= LUA_TNIL {
        match crate::boards::daily(&h.project.boards) {
            Some(d) if h.boards.daily => d.board,
            _ => 0,
        }
    } else {
        match named_board(L, h, "round.board") {
            Ok(b) => b.board,
            Err(r) => return r,
        }
    };
    if !h.boards.title && h.project.scored {
        maimbrain::store::show_scores(n);
    }
    0
});
