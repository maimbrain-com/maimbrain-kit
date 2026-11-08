//! Luau 0.743 (Roblox's Lua, MIT), built from source (build.rs): the VM
//! Studio engine 2.0 runs (`vm`) and the compiler `mb` packs a project's Lua
//! with (`compiler`). docs/STUDIO-2.md §4.13 has why and how; spike S0
//! (docs/spike/studio2-vm.md) measured it.
//!
//! The VM half is the raw C API the engine uses plus the shim's functions
//! (csrc/vm.cpp). It's deliberately thin: the engine talks to the stack
//! itself, as its hot paths need.
//!
//! **Lua errors and Rust frames.** Luau raises with a C++ exception, which
//! must not unwind through a Rust frame (natively that aborts; in the wasm
//! it skips the frame's cleanup). Almost every C API call *can* raise:
//! any push or table write allocates, and allocation fails at the heap's
//! cap; a non-raw get or set can run a metamethod. So:
//! - Rust raises only by returning: a function behind [`sv_push_rfn`]
//!   returns -1 (or -2) and the shim calls `lua_error` after it returns.
//! - Rust calls Lua only with `lua_pcall`, and never `lua_error`,
//!   `lua_call` or `luaL_check*`; on tables a game can reach it uses raw
//!   gets and sets, so no game metamethod runs from Rust.
//! - The heap's limit is met at interrupts, in the VM loop, as a Lua error;
//!   the allocator's hard cap is 8 MB above it, so the allocations Rust
//!   makes between Lua calls (a binding's tables and strings, a frame's
//!   events) don't fail in practice.
//! - Rust functions Lua calls are `extern "C-unwind"` ([`RustFn`]): natively
//!   a raise that does pass one unwinds through it (dropping what it owns);
//!   in the published wasm (panic = abort) it skips the frame, so its owned
//!   values leak with the round that errored.
//! - What Rust does to a state outside any Lua call (a round's frame, the
//!   debug menu's picks, setting a VM up) runs under [`sv_protect`]
//!   (`lua_cpcall`), so even a raise there ends that call cleanly.
#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

#[cfg(feature = "vm")]
pub use vm::*;

/// The bytecode versions this VM loads (Luau's `LBC_VERSION_MIN`/`_MAX`),
/// and the one its compiler writes with every rolled-out flag on ([`init`]:
/// the newest, not Luau's default `LBC_VERSION_TARGET`). A chunk's first
/// byte is its version (0 is a compile error). mb-format's validator has a
/// copy (`studio::BYTECODE_VERSIONS`); mb-cli's tests keep them equal.
pub const BYTECODE_VERSION_MIN: u8 = 3;
pub const BYTECODE_VERSION_MAX: u8 = 14;
pub const BYTECODE_VERSION_TARGET: u8 = 14;

unsafe extern "C" {
    fn sv_enable_flags();
    fn sv_flag_count() -> std::ffi::c_int;
}

/// Static constructors (Luau's flags register themselves in one) and the
/// rolled-out flags on, once per process, before any VM or compile.
pub fn init() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            unsafe extern "C" {
                fn __wasm_call_ctors();
            }
            __wasm_call_ctors();
        }
        unsafe { sv_enable_flags() };
    });
}

/// How many feature flags this Luau has (a check that `init` saw them).
pub fn flag_count() -> i32 {
    init();
    unsafe { sv_flag_count() }
}

/// A syntax error from [`compile`]: the line (1-based) and Luau's message.
#[cfg(feature = "compiler")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub line: u32,
    pub message: String,
}

/// Compiles a chunk to bytecode with the engine's options: optimization
/// level 2, debug level 1 (line info, so runtime errors name source lines)
/// and the engine's `disabledBuiltins` (csrc/compile.cpp).
#[cfg(feature = "compiler")]
pub fn compile(src: &[u8]) -> Result<Vec<u8>, CompileError> {
    unsafe extern "C" {
        fn sv_compile(src: *const std::ffi::c_char, len: usize, outlen: *mut usize) -> *mut std::ffi::c_char;
        fn free(p: *mut std::ffi::c_void);
    }
    init();
    let mut n = 0usize;
    let bc = unsafe { sv_compile(src.as_ptr() as *const std::ffi::c_char, src.len(), &mut n) };
    let out = unsafe { std::slice::from_raw_parts(bc as *const u8, n) }.to_vec();
    unsafe { free(bc as *mut std::ffi::c_void) };
    match out.first() {
        Some(0) => {
            // ":12: Expected ..." (Luau's form; the chunk has no name yet).
            let msg = String::from_utf8_lossy(&out[1..]).into_owned();
            let rest = msg.strip_prefix(':').unwrap_or(&msg);
            let (line, message) = match rest.split_once(": ") {
                Some((l, m)) if l.bytes().all(|c| c.is_ascii_digit()) && !l.is_empty() => (l.parse().unwrap_or(0), m.to_string()),
                _ => (0, msg.clone()),
            };
            Err(CompileError { line, message })
        }
        _ => Ok(out),
    }
}

/// A problem `luau-analyze`'s checker found: the file, its line and
/// column (1-based), and Luau's message.
#[cfg(feature = "analyze")]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeError {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// Type-checks `files` (name, source; a project's are `game/main.lua` and
/// the rest, `require("x")` being `game/x.lua`) against the `definitions`
/// (name, source of a `.d.luau`), as `luau-analyze` would: the old solver,
/// nonstrict mode, type and syntax errors only (Decided 33). A definition
/// file that doesn't load is an error under its own name.
#[cfg(feature = "analyze")]
pub fn analyze(definitions: &[(&str, &str)], files: &[(&str, &str)]) -> Vec<TypeError> {
    use std::ffi::{CString, c_char, c_int};
    unsafe extern "C" {
        fn sv_analyze(
            def_names: *const *const c_char,
            defs: *const *const c_char,
            def_lens: *const usize,
            ndefs: c_int,
            names: *const *const c_char,
            sources: *const *const c_char,
            lens: *const usize,
            n: c_int,
        ) -> *mut c_char;
        fn sv_analyze_free(p: *mut c_char);
    }
    init();
    let cs = |v: &[(&str, &str)]| -> (Vec<CString>, Vec<*const c_char>, Vec<usize>) {
        let names: Vec<CString> = v.iter().map(|(n, _)| CString::new(*n).unwrap_or_default()).collect();
        let srcs = v.iter().map(|(_, s)| s.as_ptr() as *const c_char).collect();
        let lens = v.iter().map(|(_, s)| s.len()).collect();
        (names, srcs, lens)
    };
    let (dn, ds, dl) = cs(definitions);
    let (fnames, fs, fl) = cs(files);
    let dnp: Vec<*const c_char> = dn.iter().map(|c| c.as_ptr()).collect();
    let fnp: Vec<*const c_char> = fnames.iter().map(|c| c.as_ptr()).collect();
    let out = unsafe {
        let p = sv_analyze(dnp.as_ptr(), ds.as_ptr(), dl.as_ptr(), dnp.len() as c_int, fnp.as_ptr(), fs.as_ptr(), fl.as_ptr(), fnp.len() as c_int);
        let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
        sv_analyze_free(p);
        s
    };
    let mut errors: Vec<TypeError> = out
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\t');
            Some(TypeError { file: p.next()?.to_string(), line: p.next()?.parse().ok()?, column: p.next()?.parse().ok()?, message: p.next()?.to_string() })
        })
        .collect();
    // A required module's errors come back with each file that requires it.
    errors.sort();
    errors.dedup();
    errors
}

#[cfg(feature = "vm")]
mod vm {
    use std::ffi::{CStr, c_char, c_int, c_void};

    pub enum lua_State {}
    pub type lua_CFunction = unsafe extern "C" fn(*mut lua_State) -> c_int;
    /// A Rust function behind [`sv_push_rfn`]: returns how many results it
    /// pushed, or -1 to raise the error message it pushed with the position
    /// of the Lua that called it, or -2 to raise it as it is (an error from
    /// Lua it called, positioned already). The shim calls `lua_error`, so Lua
    /// errors never unwind through Rust frames.
    ///
    /// It's `C-unwind`: if a Lua API call inside it raises anyway (see the
    /// crate's notes), the error unwinds through it natively rather than
    /// aborting the process.
    pub type RustFn = unsafe extern "C-unwind" fn(*mut lua_State, *mut c_void) -> c_int;

    pub const LUA_REGISTRYINDEX: c_int = -8000 - 2000;
    pub const LUA_ENVIRONINDEX: c_int = -8000 - 2001;
    pub const LUA_GLOBALSINDEX: c_int = -8000 - 2002;
    pub const fn lua_upvalueindex(i: c_int) -> c_int {
        LUA_GLOBALSINDEX - i
    }
    pub const LUA_OK: c_int = 0;
    pub const LUA_GCCOLLECT: c_int = 2;
    pub const LUA_ERRMEM: c_int = 4;

    pub const LUA_TNIL: c_int = 0;
    pub const LUA_TBOOLEAN: c_int = 1;
    pub const LUA_TLIGHTUSERDATA: c_int = 2;
    pub const LUA_TNUMBER: c_int = 3;
    pub const LUA_TINTEGER: c_int = 4;
    pub const LUA_TVECTOR: c_int = 5;
    pub const LUA_TSTRING: c_int = 6;
    pub const LUA_TTABLE: c_int = 7;
    pub const LUA_TFUNCTION: c_int = 8;
    pub const LUA_TUSERDATA: c_int = 9;
    pub const LUA_TTHREAD: c_int = 10;
    pub const LUA_TBUFFER: c_int = 11;

    unsafe extern "C" {
        // lua.h
        pub fn lua_gettop(L: *mut lua_State) -> c_int;
        pub fn lua_settop(L: *mut lua_State, idx: c_int);
        pub fn lua_pushvalue(L: *mut lua_State, idx: c_int);
        pub fn lua_remove(L: *mut lua_State, idx: c_int);
        pub fn lua_insert(L: *mut lua_State, idx: c_int);
        pub fn lua_replace(L: *mut lua_State, idx: c_int);
        pub fn lua_type(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_rawequal(L: *mut lua_State, a: c_int, b: c_int) -> c_int;
        pub fn lua_tonumberx(L: *mut lua_State, idx: c_int, isnum: *mut c_int) -> f64;
        pub fn lua_toboolean(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_tolstring(L: *mut lua_State, idx: c_int, len: *mut usize) -> *const c_char;
        pub fn lua_tolstringatom(L: *mut lua_State, idx: c_int, len: *mut usize, atom: *mut c_int) -> *const c_char;
        pub fn lua_namecallatom(L: *mut lua_State, atom: *mut c_int) -> *const c_char;
        pub fn lua_touserdata(L: *mut lua_State, idx: c_int) -> *mut c_void;
        pub fn lua_topointer(L: *mut lua_State, idx: c_int) -> *const c_void;
        pub fn lua_userdatatag(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_objlen(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_pushnil(L: *mut lua_State);
        pub fn lua_pushnumber(L: *mut lua_State, n: f64);
        pub fn lua_pushboolean(L: *mut lua_State, b: c_int);
        pub fn lua_pushlstring(L: *mut lua_State, s: *const c_char, l: usize);
        pub fn lua_pushstring(L: *mut lua_State, s: *const c_char);
        pub fn lua_pushcclosurek(L: *mut lua_State, f: lua_CFunction, name: *const c_char, nup: c_int, cont: *const c_void);
        pub fn lua_newuserdatataggedwithmetatable(L: *mut lua_State, sz: usize, tag: c_int) -> *mut c_void;
        pub fn lua_getfield(L: *mut lua_State, idx: c_int, k: *const c_char) -> c_int;
        pub fn lua_rawgetfield(L: *mut lua_State, idx: c_int, k: *const c_char) -> c_int;
        pub fn lua_rawget(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_rawgeti(L: *mut lua_State, idx: c_int, n: c_int) -> c_int;
        pub fn lua_createtable(L: *mut lua_State, narr: c_int, nrec: c_int);
        pub fn lua_setfield(L: *mut lua_State, idx: c_int, k: *const c_char);
        pub fn lua_rawsetfield(L: *mut lua_State, idx: c_int, k: *const c_char);
        pub fn lua_rawseti(L: *mut lua_State, idx: c_int, n: c_int);
        pub fn lua_rawset(L: *mut lua_State, idx: c_int);
        pub fn lua_setmetatable(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_setreadonly(L: *mut lua_State, idx: c_int, enabled: c_int);
        pub fn lua_setsafeenv(L: *mut lua_State, idx: c_int, enabled: c_int);
        pub fn lua_next(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_pcall(L: *mut lua_State, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int;
        /// Room for `sz` more values on the stack (a C function is given 20,
        /// `LUA_MINSTACK`; pushes past what it has are unchecked): 0 if it
        /// can't grow that far.
        pub fn lua_checkstack(L: *mut lua_State, sz: c_int) -> c_int;
        pub fn lua_ref(L: *mut lua_State, idx: c_int) -> c_int;
        pub fn lua_unref(L: *mut lua_State, r: c_int) -> c_int;
        pub fn lua_setuserdatametatable(L: *mut lua_State, tag: c_int);
        pub fn lua_callbacks(L: *mut lua_State) -> *mut lua_Callbacks;
        pub fn lua_gc(L: *mut lua_State, what: c_int, data: c_int) -> c_int;
        // shim (csrc/vm.cpp)
        pub fn sv_new(mem_limit: usize) -> *mut lua_State;
        pub fn sv_close(L: *mut lua_State);
        pub fn sv_set_host(L: *mut lua_State, host: *mut c_void);
        pub fn sv_host(L: *mut lua_State) -> *mut c_void;
        pub fn sv_sandbox(L: *mut lua_State) -> *mut lua_State;
        pub fn sv_load_bytecode(L: *mut lua_State, chunkname: *const c_char, bc: *const c_char, len: usize) -> c_int;
        pub fn sv_push_rfn(L: *mut lua_State, f: RustFn, ud: *mut c_void, name: *const c_char);
        pub fn sv_set_budget(L: *mut lua_State, n: i64);
        pub fn sv_reset_budget(L: *mut lua_State);
        pub fn sv_counted(L: *mut lua_State) -> i64;
        pub fn sv_trip_message(L: *mut lua_State) -> *const c_char;
        pub fn sv_mem_used(L: *mut lua_State) -> usize;
        pub fn sv_protect(L: *mut lua_State, f: RustFn, ud: *mut c_void, result: *mut c_int) -> c_int;
        pub fn sv_set_mem_limit(L: *mut lua_State, limit: usize);
        pub fn sv_mem_peak(L: *mut lua_State) -> usize;
    }

    /// The part of `lua_Callbacks` the engine touches (the struct is longer;
    /// only pointers to it are used).
    #[repr(C)]
    pub struct lua_Callbacks {
        pub userdata: *mut c_void,
        pub interrupt: Option<unsafe extern "C" fn(*mut lua_State, c_int)>,
        pub panic: Option<unsafe extern "C" fn(*mut lua_State, c_int)>,
        pub userthread: Option<unsafe extern "C" fn(*mut lua_State, *mut lua_State)>,
        pub useratom: Option<unsafe extern "C" fn(*mut lua_State, *const c_char, usize) -> i16>,
    }

    pub unsafe fn lua_pop(L: *mut lua_State, n: c_int) {
        unsafe { lua_settop(L, -n - 1) }
    }
    pub unsafe fn lua_tonumber(L: *mut lua_State, i: c_int) -> f64 {
        unsafe { lua_tonumberx(L, i, std::ptr::null_mut()) }
    }
    pub unsafe fn lua_newtable(L: *mut lua_State) {
        unsafe { lua_createtable(L, 0, 0) }
    }
    /// The bytes of a string (or a number, converted in place, as Lua does).
    pub unsafe fn lua_tostr<'a>(L: *mut lua_State, i: c_int) -> Option<&'a [u8]> {
        let mut n = 0;
        let p = unsafe { lua_tolstring(L, i, &mut n) };
        (!p.is_null()).then(|| unsafe { std::slice::from_raw_parts(p as *const u8, n) })
    }
    pub unsafe fn push_str(L: *mut lua_State, s: &str) {
        unsafe { lua_pushlstring(L, s.as_ptr() as *const c_char, s.len()) }
    }

    pub unsafe fn cstr(p: *const c_char) -> Option<String> {
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }

    /// One allocator for Rust and C++ in the wasm: wasi-libc's dlmalloc (C++'s
    /// `new` and Luau's heap use it). Two allocators each growing memory would
    /// step on each other (wasi-libc's `sbrk` assumes it owns the end of
    /// memory), and defining `malloc` in Rust would export it from the cdylib.
    #[cfg(target_arch = "wasm32")]
    mod libc_alloc {
        use std::alloc::{GlobalAlloc, Layout};
        unsafe extern "C" {
            fn malloc(n: usize) -> *mut u8;
            fn free(p: *mut u8);
            fn realloc(p: *mut u8, n: usize) -> *mut u8;
            fn aligned_alloc(align: usize, n: usize) -> *mut u8;
        }
        struct Libc;
        unsafe impl GlobalAlloc for Libc {
            unsafe fn alloc(&self, l: Layout) -> *mut u8 {
                unsafe { if l.align() <= 16 { malloc(l.size()) } else { aligned_alloc(l.align(), l.size().next_multiple_of(l.align())) } }
            }
            unsafe fn dealloc(&self, p: *mut u8, _: Layout) {
                unsafe { free(p) }
            }
            unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
                if l.align() <= 16 {
                    return unsafe { realloc(p, n) };
                }
                unsafe {
                    let q = self.alloc(Layout::from_size_align_unchecked(n, l.align()));
                    if !q.is_null() {
                        std::ptr::copy_nonoverlapping(p, q, l.size().min(n));
                        free(p);
                    }
                    q
                }
            }
        }
        #[global_allocator]
        static A: Libc = Libc;
    }
}

#[cfg(all(test, feature = "vm", feature = "compiler"))]
mod tests;
