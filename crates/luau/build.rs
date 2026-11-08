//! Builds Luau 0.743 (C++) into this crate: the VM (`vm`), the compiler
//! (`compiler`), or both, and natively the type checker (`analyze`). Natively with the host's C++ compiler; for
//! `wasm32-unknown-unknown` with Homebrew's clang (llvm 21) against wasi-sdk
//! 34's sysroot (libc, and libc++ built with wasm exception handling), as
//! spike S0 found works (docs/spike/studio2-vm.md §8).
//!
//! Luau's source and the wasm sysroot are fetched by hash into a cache
//! shared by every checkout (`MB_THIRD_PARTY`, else `~/.cache/maimbrain`),
//! once: Luau is 4 MB, the sysroot 120 MB (only for wasm builds).
//!
//! Two changes to Luau's own files, made on copies in OUT_DIR (the cache
//! stays as fetched), each checked to apply exactly once:
//! - `lgc.cpp`: `__mode` is ignored, so no table is ever weak. Luau reads
//!   `__mode` when it collects, so what a weak table keeps would depend on
//!   when the collector runs (docs/STUDIO-2.md §4.13, Decided 32).
//! - `lstrlib.cpp`: `string.format("%g", nan)` prints `nan` whatever the
//!   NaN's sign, as `tostring` does; wasm leaves that sign unspecified.
//!
//! Left out: `linit.cpp` (it opens `os` and `debug`), `loslib.cpp`,
//! `ldblib.cpp`, and in wasm `lperf.cpp` (a clock; csrc/wasm.cpp has a
//! `lua_clock` that returns 0, which only GC metrics read).
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// What's fetched: (url, tarball, its SHA-256, the directory it unpacks to).
const LUAU: (&str, &str, &str, &str) = (
    "https://github.com/luau-lang/luau/archive/refs/tags/0.743.tar.gz",
    "luau-0.743.tar.gz",
    "be23707d541710a978f2c559c069e22ba9c7c449e07d60472fb4e074914bc91d",
    "luau-0.743",
);
const SYSROOT: (&str, &str, &str, &str) = (
    "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sysroot-34.0.tar.gz",
    "wasi-sysroot-34.0.tar.gz",
    "9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be",
    "wasi-sysroot-34.0",
);
const CLANG_RT: (&str, &str, &str, &str) = (
    "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/libclang_rt-34.0.tar.gz",
    "libclang_rt-34.0.tar.gz",
    "eee3e634dcf71aa22b1333391623cf5c9965a637dc428a27b1a858c026c587f1",
    "libclang_rt-34.0",
);

/// The edits to Luau's files: (file under VM/src or Compiler/src, the
/// text, its replacement).
const PATCHES: &[(&str, &str, &str)] = &[
    (
        "lgc.cpp",
        "const TValue* mode = gfasttm(g, h->metatable, TM_MODE);",
        "const TValue* mode = NULL; // Maimbrain: __mode is ignored, no table is weak (docs/STUDIO-2.md §4.13)\n    (void)g;\n    (void)h;",
    ),
    (
        "lstrlib.cpp",
        "snprintf(buff, sizeof(buff), form, (double)luaL_checknumber(L, arg));",
        "double n = (double)luaL_checknumber(L, arg);\n                if (n != n)\n                {\n                    // Maimbrain: NaN prints as nan, whatever its sign (as tostring does)\n                    uint64_t bits = 0x7ff8000000000000ull;\n                    memcpy(&n, &bits, sizeof(n));\n                }\n                snprintf(buff, sizeof(buff), form, n);",
    ),
    // Bytecode is the same on every host (the server recompiles a creator's
    // Lua and compares byte for byte): the compiler folds only builtins
    // IEEE 754 rounds correctly, so no host's libm reaches the constants.
    (
        "BuiltinFolding.cpp",
        "{\n    switch (bfid)\n    {\n    case LBF_MATH_ABS:",
        "{\n    // Maimbrain: no folding through libm (not correctly rounded, so hosts differ).\n    switch (bfid)\n    {\n    case LBF_MATH_ACOS:\n    case LBF_MATH_ASIN:\n    case LBF_MATH_ATAN2:\n    case LBF_MATH_ATAN:\n    case LBF_MATH_COSH:\n    case LBF_MATH_COS:\n    case LBF_MATH_EXP:\n    case LBF_MATH_LOG10:\n    case LBF_MATH_LOG:\n    case LBF_MATH_POW:\n    case LBF_MATH_SINH:\n    case LBF_MATH_SIN:\n    case LBF_MATH_TANH:\n    case LBF_MATH_TAN:\n        return cvar();\n    default:\n        break;\n    }\n    switch (bfid)\n    {\n    case LBF_MATH_ABS:",
    ),
    (
        "ConstantFolding.cpp",
        "            result.type = Constant::Type_Number;\n            result.valueNumber = pow(la.valueNumber, ra.valueNumber);",
        "            // Maimbrain: `^` isn't folded (pow isn't correctly rounded, so hosts differ).",
    ),
];

fn cache_dir() -> PathBuf {
    if let Some(d) = env::var_os("MB_THIRD_PARTY") {
        return PathBuf::from(d);
    }
    if let Some(d) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(d).join("maimbrain");
    }
    if cfg!(windows)
        && let Some(d) = env::var_os("LOCALAPPDATA")
    {
        return PathBuf::from(d).join("maimbrain");
    }
    match env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join(".cache/maimbrain"),
        None => PathBuf::from(env::var("OUT_DIR").unwrap()).join("third_party"),
    }
}

/// The unpacked package, fetched and checked first if it isn't there.
/// Concurrent builds each unpack into a directory of their own and rename
/// it into place, so one never sees another's half-written files.
fn fetch(cache: &Path, (url, file, sha, dir): (&str, &str, &str, &str)) -> PathBuf {
    let done = cache.join(dir);
    if done.join(".fetched").is_file() {
        return done;
    }
    std::fs::create_dir_all(cache).unwrap_or_else(|e| panic!("{}: {e}", cache.display()));
    let tag = format!("{}-{}", std::process::id(), env::var("TARGET").unwrap_or_default());
    let tgz = cache.join(format!("{file}.{tag}.part"));
    println!("cargo:warning=fetching {url} into {}", cache.display());
    let ok = Command::new("curl").args(["-fsSL", "--retry", "3", "-o"]).arg(&tgz).arg(url).status().is_ok_and(|s| s.success())
        || Command::new("wget").args(["-q", "-O"]).arg(&tgz).arg(url).status().is_ok_and(|s| s.success());
    if !ok {
        panic!("couldn't download {url} (curl or wget, network): set MB_THIRD_PARTY to a directory that has {dir}/ unpacked from it, with an empty {dir}/.fetched");
    }
    let bytes = std::fs::read(&tgz).unwrap();
    let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if got != sha {
        let _ = std::fs::remove_file(&tgz);
        panic!("{url}: SHA-256 {got}, expected {sha}");
    }
    let tmp = cache.join(format!(".{dir}.{tag}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let status = Command::new("tar").arg("xzf").arg(&tgz).arg("-C").arg(&tmp).status();
    let _ = std::fs::remove_file(&tgz);
    if !status.is_ok_and(|s| s.success()) {
        panic!("couldn't unpack {file} with tar");
    }
    std::fs::write(tmp.join(dir).join(".fetched"), url).unwrap();
    if std::fs::rename(tmp.join(dir), &done).is_err() && !done.join(".fetched").is_file() {
        panic!("couldn't move {dir} into {}", cache.display());
    }
    let _ = std::fs::remove_dir_all(&tmp);
    done
}

fn cpp_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "cpp"))
        .filter(|p| !skip.iter().any(|s| p.file_name().unwrap() == *s))
        .collect();
    v.sort();
    v
}

/// A copy of `src` with `PATCHES`' edits for it, in `out`.
fn patched(src: &Path, out: &Path) -> PathBuf {
    let name = src.file_name().unwrap().to_str().unwrap();
    let mut text = std::fs::read_to_string(src).unwrap();
    for (_, from, to) in PATCHES.iter().filter(|p| p.0 == name) {
        assert_eq!(text.matches(from).count(), 1, "the {name} patch no longer applies to this Luau: check build.rs's PATCHES");
        text = text.replace(from, to);
    }
    let dst = out.join(name);
    std::fs::write(&dst, text).unwrap();
    dst
}

/// The clang that builds C++ for wasm32: `MB_WASM_CLANG` (a clang++ with
/// wasm exception handling, llvm 20 or later), else Homebrew's.
fn wasm_clang() -> (PathBuf, PathBuf) {
    if let Some(c) = env::var_os("MB_WASM_CLANG") {
        let c = PathBuf::from(c);
        let ar = c.with_file_name("llvm-ar");
        return (c, ar);
    }
    for d in ["/opt/homebrew/opt/llvm/bin", "/usr/local/opt/llvm/bin", "/home/linuxbrew/.linuxbrew/opt/llvm/bin"] {
        let c = Path::new(d).join("clang++");
        if c.is_file() {
            return (c, Path::new(d).join("llvm-ar"));
        }
    }
    panic!("no clang for wasm32: `brew install llvm` (llvm 21), or set MB_WASM_CLANG to a clang++ of llvm 20 or later");
}

fn main() {
    let here = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    let wasm = target.starts_with("wasm32");
    let vm = env::var_os("CARGO_FEATURE_VM").is_some();
    let compiler = env::var_os("CARGO_FEATURE_COMPILER").is_some();
    let analyze = env::var_os("CARGO_FEATURE_ANALYZE").is_some();
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MB_THIRD_PARTY");
    println!("cargo:rerun-if-env-changed=MB_WASM_CLANG");
    if !vm && !compiler {
        return;
    }
    assert!(!(wasm && compiler), "the engine's wasm has no Luau compiler (Lua is compiled when a project is packed): turn off luau's `compiler` feature for wasm32");
    assert!(!(wasm && analyze), "the type checker is native only: turn off luau's `analyze` feature for wasm32");

    let cache = cache_dir();
    let luau = fetch(&cache, LUAU);
    let mut b = cc::Build::new();
    b.cpp(true).std("c++17").warnings(false);
    for d in ["Common/include", "Ast/include", "Bytecode/include", "Compiler/include", "Compiler/src", "VM/include", "VM/src", "Config/include", "Analysis/include"] {
        b.include(luau.join(d));
    }
    // C linkage for the public API, so Rust can call it.
    b.define("LUA_API", "extern \"C\"");
    b.define("LUACODE_API", "extern \"C\"");
    // Lua errors unwind as C++ exceptions, as in Roblox's own builds (and
    // the compiler throws them regardless).
    b.define("LUA_USE_LONGJMP", "0");
    // Native test builds check Luau's API contracts (a push past the stack
    // a C function was given, a bad index): its asserts, off in releases.
    if !wasm && env::var("PROFILE").as_deref() == Ok("debug") {
        b.define("LUAU_ENABLE_ASSERT", None);
    }
    // No fused multiply-add where the source has a multiply and an add:
    // constant folding (`a % b` is a - floor(a / b) * b) gives the same
    // bits on every host, and so does the VM natively.
    b.flag_if_supported("-ffp-contract=off");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        b.flag("/fp:precise");
    }
    // TimeTrace is the checker's clock; nothing else reads it.
    let patches = out.join("patched");
    std::fs::create_dir_all(&patches).unwrap();
    let patch = |f: PathBuf| if PATCHES.iter().any(|p| p.0 == f.file_name().unwrap().to_str().unwrap()) { patched(&f, &patches) } else { f };
    let mut files = cpp_files(&luau.join("Common/src"), if analyze { &[] } else { &["TimeTrace.cpp"] });
    files.push(here.join("csrc/flags.cpp"));
    if vm {
        // The checker's type functions run in a sandboxed VM of their own
        // (linit.cpp's luaL_sandbox), so it has them; the engine doesn't.
        let mut skip = if analyze { vec![] } else { vec!["linit.cpp", "loslib.cpp", "ldblib.cpp"] };
        if wasm {
            skip.push("lperf.cpp");
        }
        for f in cpp_files(&luau.join("VM/src"), &skip) {
            files.push(patch(f));
        }
        files.push(here.join("csrc/vm.cpp"));
        if wasm {
            files.push(here.join("csrc/wasm.cpp"));
        }
    }
    if compiler {
        files.extend(cpp_files(&luau.join("Ast/src"), &["PrettyPrinter.cpp"]));
        files.extend(cpp_files(&luau.join("Bytecode/src"), &["BytecodeDump.cpp"]));
        files.extend(cpp_files(&luau.join("Compiler/src"), &[]).into_iter().map(patch));
        files.push(here.join("csrc/compile.cpp"));
    }
    if analyze {
        files.extend(cpp_files(&luau.join("Config/src"), &[]));
        files.extend(cpp_files(&luau.join("Analysis/src"), &[]));
        files.push(here.join("csrc/analyze.cpp"));
    }
    b.files(&files);

    if wasm {
        let sysroot = fetch(&cache, SYSROOT);
        let rt = fetch(&cache, CLANG_RT);
        let (clang, ar) = wasm_clang();
        b.compiler(clang);
        b.archiver(ar);
        b.cpp_link_stdlib(None);
        // wasi's headers; the target is still wasm32-unknown-unknown when it links.
        b.flag("--target=wasm32-wasip1");
        b.flag(format!("--sysroot={}", sysroot.display()));
        b.flag("-nostdinc++");
        b.flag(format!("-isystem{}", sysroot.join("include/wasm32-wasip1/eh/c++/v1").display()));
        b.flag("-fwasm-exceptions");
        // wasi-sdk 34's libc++abi uses the standard (exnref) proposal:
        // try_table / throw_ref. The legacy form would need a libc++abi built
        // for it, which wasi-sdk doesn't ship.
        b.flag("-mllvm");
        b.flag("-wasm-use-legacy-eh=false");
        b.flag("-fvisibility=hidden");
        b.flag("-ffunction-sections");
        b.flag("-fdata-sections");
        b.compile("luau");
        println!("cargo:rustc-link-search=native={}", sysroot.join("lib/wasm32-wasip1/eh").display());
        println!("cargo:rustc-link-search=native={}", sysroot.join("lib/wasm32-wasip1").display());
        println!("cargo:rustc-link-search=native={}", rt.join("wasm32-unknown-wasip1").display());
        for l in ["c++", "c++abi", "unwind", "c", "clang_rt.builtins"] {
            println!("cargo:rustc-link-lib=static={l}");
        }
    } else {
        b.compile("luau");
    }
}
