//! The VM as the engine builds it, natively: compiled chunks run, errors
//! carry their lines, and the two changes to Luau (build.rs) hold.
use std::ffi::{CString, c_char};

use super::*;

/// A sandboxed VM (the game's thread) with a 1 MB heap and a budget.
struct Vm {
    l: *mut lua_State,
    t: *mut lua_State,
}

impl Vm {
    fn new(heap: usize, budget: i64) -> Vm {
        init();
        unsafe {
            let l = sv_new(heap);
            let t = sv_sandbox(l);
            sv_set_budget(l, budget);
            Vm { l, t }
        }
    }

    /// Runs a chunk; its first result as a string, or the error.
    fn run(&self, src: &str) -> Result<String, String> {
        let bc = compile(src.as_bytes()).map_err(|e| format!("compile: {}: {}", e.line, e.message))?;
        let name = CString::new("=game/main.lua").unwrap();
        unsafe {
            sv_reset_budget(self.l);
            if sv_load_bytecode(self.t, name.as_ptr(), bc.as_ptr() as *const c_char, bc.len()) != 0 {
                return Err(String::from_utf8_lossy(lua_tostr(self.t, -1).unwrap_or_default()).into_owned());
            }
            let r = lua_pcall(self.t, 0, 1, 0);
            let s = lua_tostr(self.t, -1).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_else(|| "nil".into());
            lua_pop(self.t, 1);
            if r == LUA_OK { Ok(s) } else { Err(cstr(sv_trip_message(self.l)).unwrap_or(s)) }
        }
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        unsafe { sv_close(self.l) }
    }
}

#[test]
fn chunks_compile_and_run() {
    let vm = Vm::new(1 << 20, 0);
    assert_eq!(vm.run("local t = {} for i = 1, 10 do t[#t + 1] = i * i end return tostring(t[10])").unwrap(), "100");
    // Luau's own syntax runs too (allowed in projects, not taught).
    assert_eq!(vm.run("local n = 1 n += 2 return `{n}`").unwrap(), "3");
    assert!(flag_count() > 0);
}

#[test]
fn bytecode_is_the_target_version() {
    let bc = compile(b"return 1").unwrap();
    assert_eq!(bc[0], BYTECODE_VERSION_TARGET);
    assert!((BYTECODE_VERSION_MIN..=BYTECODE_VERSION_MAX).contains(&bc[0]));
}

#[test]
fn syntax_errors_have_their_line() {
    let e = compile(b"local a = 1\nlocal b = = 2\n").unwrap_err();
    assert_eq!(e.line, 2, "{e:?}");
    assert!(e.message.contains("Expected"), "{e:?}");
    // Lua 5.4's goto isn't Luau.
    assert!(compile(b"goto done\n::done::").is_err());
}

#[test]
fn runtime_errors_name_the_chunk_and_line() {
    let vm = Vm::new(1 << 20, 0);
    let e = vm.run("local x = nil\n\nreturn x.y").unwrap_err();
    assert!(e.starts_with("game/main.lua:3:"), "{e}");
}

#[test]
fn the_budget_trips_at_the_loops_own_line() {
    let vm = Vm::new(1 << 20, 100_000);
    let e = vm.run("local n = 0\nlocal ok = pcall(function()\n  while true do n = n + 1 end\nend)\nwhile true do end").unwrap_err();
    assert!(e.contains("sandbox: instruction budget exhausted"), "{e}");
    assert!(e.starts_with("game/main.lua:3:") || e.starts_with("game/main.lua:5:"), "{e}");
}

#[test]
fn the_heap_is_capped() {
    let vm = Vm::new(1 << 20, 0);
    let e = vm.run("local t = {} for i = 1, 1e7 do t[i] = tostring(i) end").unwrap_err();
    assert!(e.contains("not enough memory"), "{e}");
}

/// build.rs's lgc.cpp patch: `__mode` makes no table weak, so what a table
/// keeps never depends on when the collector runs.
#[test]
fn weak_tables_are_off() {
    let vm = Vm::new(8 << 20, 0);
    vm.run("weak = setmetatable({}, { __mode = 'k' }) for i = 1, 100 do weak[{}] = i end").unwrap();
    unsafe { lua_gc(vm.l, LUA_GCCOLLECT, 0) };
    assert_eq!(vm.run("local n = 0 for _ in pairs(weak) do n = n + 1 end return tostring(n)").unwrap(), "100");
}

/// build.rs's lstrlib.cpp patch: NaN formats as `nan` whatever its sign.
#[test]
fn nan_formats_as_nan() {
    let vm = Vm::new(1 << 20, 0);
    assert_eq!(vm.run("local n = 0/0 return string.format('%g %g %.2f', n, -n, -(0/0))").unwrap(), "nan nan nan");
    assert_eq!(vm.run("return tostring(-(0/0))").unwrap(), "nan");
}

#[test]
fn the_sandbox_has_no_way_out() {
    let vm = Vm::new(1 << 20, 0);
    for g in ["os", "debug", "loadstring", "getfenv", "setfenv", "collectgarbage", "gcinfo", "newproxy", "require", "load", "io"] {
        assert_eq!(vm.run(&format!("return tostring({g})")).unwrap(), "nil", "{g}");
    }
    // Library tables are read-only.
    assert!(vm.run("string.rep = nil").unwrap_err().contains("readonly"));
}

/// The compiler folds only what IEEE 754 rounds correctly: `math.sin(1)`
/// and `2 ^ 0.5` stay calls (a host's libm would bake its own bits in),
/// while `math.sqrt(2)` and `7.5 % 2` fold.
#[test]
fn only_exact_builtins_fold() {
    let has = |src: &str, v: f64| {
        let bc = compile(src.as_bytes()).unwrap();
        bc.windows(8).any(|w| w == v.to_le_bytes())
    };
    let one = std::hint::black_box(1.0f64);
    assert!(!has("return math.sin(1)", one.sin()));
    assert!(!has("return math.exp(1)", one.exp()));
    assert!(!has("return 2 ^ 0.5", 2f64.powf(0.5)));
    assert!(has("return math.sqrt(2)", 2f64.sqrt()));
    assert!(has("return 7.5 % 2", 1.5));
}
