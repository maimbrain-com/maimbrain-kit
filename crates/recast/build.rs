//! Builds Recast, Detour and DetourCrowd 1.6.0 (C++, zlib) with
//! csrc/shim.cpp into this crate (docs/STUDIO-2.md §4.14, spike S10,
//! Decided 45). Natively with the host's C++ compiler; for
//! `wasm32-unknown-unknown` with the clang crates/luau uses (Homebrew's
//! llvm, or `MB_WASM_CLANG`) against wasi-sdk 34's headers, fetched by hash
//! into the same cache.
//!
//! Without exceptions or RTTI, and nothing from libc is linked: memory is
//! the game's allocator (src/lib.rs's `mbrc_alloc`), and csrc/portable.h,
//! included before every file, sends `qsort` to a stable sort (Recast sorts
//! with it, and libcs order equal keys differently), `cosf`, `sinf` and
//! `atan2f` to Rust's `libm`, and `vsnprintf` (Recast's log) nowhere. With
//! `-ffp-contract=off`, a native build bakes the same bytes as the wasm.
//!
//! Two edits to Recast's own files, made on copies in OUT_DIR (the cache
//! stays as fetched), each checked to apply exactly once:
//! - RecastAlloc.cpp and DetourAlloc.cpp: their default allocators call
//!   `mbrc_alloc`/`mbrc_free` rather than `malloc`/`free`;
//! - Recast.h's `rcContext` and DetourNavMeshQuery's `dtPolyQuery` and its
//!   two kinds: their destructors aren't virtual, so nothing refers to
//!   `operator delete` (they're only ever on the stack). Then nothing links
//!   against libc++, whose `operator delete` would free with another
//!   allocator in a game that has one (the engine's Luau).
//!
//! Left out: RecastLayers.cpp and DetourTileCache (heightfield layers and
//! moving obstacles, unused).
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// What's fetched: (url, tarball, its SHA-256, the directory it unpacks to).
const RECAST: (&str, &str, &str, &str) = (
    "https://github.com/recastnavigation/recastnavigation/archive/refs/tags/v1.6.0.tar.gz",
    "recastnavigation-1.6.0.tar.gz",
    "d48ca0121962fa0639502c0f56c4e3ae72f98e55d88727225444f500775c0074",
    "recastnavigation-1.6.0",
);
/// wasi-sdk's headers (string.h, math.h, <new>), as crates/luau fetches them.
const SYSROOT: (&str, &str, &str, &str) = (
    "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sysroot-34.0.tar.gz",
    "wasi-sysroot-34.0.tar.gz",
    "9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be",
    "wasi-sysroot-34.0",
);

/// The edits: (file, the text, its replacement).
const PATCHES: &[(&str, &str, &str)] = &[
    ("RecastAlloc.cpp", "return malloc(size);", "return mbrc_alloc(size); // Maimbrain: the game's allocator"),
    ("RecastAlloc.cpp", "\tfree(ptr);", "\tmbrc_free(ptr);"),
    ("DetourAlloc.cpp", "return malloc(size);", "return mbrc_alloc(size); // Maimbrain: the game's allocator"),
    ("DetourAlloc.cpp", "\tfree(ptr);", "\tmbrc_free(ptr);"),
    ("Recast.h", "virtual ~rcContext() {}", "~rcContext() {} // Maimbrain: not virtual, so no operator delete"),
    ("DetourNavMeshQuery.h", "\tvirtual ~dtPolyQuery();", "\t~dtPolyQuery(); // Maimbrain: not virtual, so no operator delete"),
    ("DetourNavMeshQuery.cpp", "\tvirtual ~dtFindNearestPolyQuery();", "\t~dtFindNearestPolyQuery();"),
    ("DetourNavMeshQuery.cpp", "\tvirtual ~dtCollectPolysQuery();", "\t~dtCollectPolysQuery();"),
];

/// The C++'s optimisation. `-O3` (Decided 81, docs/STUDIO-2.md §9.4 "As
/// built (Phase 3b)"): `-Os` saved 38 KB in S10, but costs bake and crowd
/// time. `MB_RECAST_OPT=s` builds it at `-Os` to measure again.
const OPT: &str = "3";

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

/// The unpacked package, fetched and checked first if it isn't there (as
/// crates/luau's build.rs: concurrent builds unpack apart and rename).
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
        assert_eq!(text.matches(from).count(), 1, "the {name} patch no longer applies to this Recast: check build.rs's PATCHES");
        text = text.replace(from, to);
    }
    let dst = out.join(name);
    std::fs::write(&dst, text).unwrap();
    dst
}

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
    let wasm = env::var("TARGET").unwrap().starts_with("wasm32");
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MB_THIRD_PARTY");
    println!("cargo:rerun-if-env-changed=MB_WASM_CLANG");
    println!("cargo:rerun-if-env-changed=MB_RECAST_OPT");

    let cache = cache_dir();
    let rc = fetch(&cache, RECAST);
    let patches = out.join("patched");
    let include = patches.join("include");
    std::fs::create_dir_all(&include).unwrap();
    // Recast.h, patched, first on the include path.
    patched(&rc.join("Recast/Include/Recast.h"), &include);
    patched(&rc.join("Detour/Include/DetourNavMeshQuery.h"), &include);
    let patch = |f: PathBuf| if PATCHES.iter().any(|p| p.0 == f.file_name().unwrap().to_str().unwrap()) { patched(&f, &patches) } else { f };

    let mut b = cc::Build::new();
    b.cpp(true).std("c++17").warnings(false);
    b.include(&include);
    for d in ["Recast/Include", "Detour/Include", "DetourCrowd/Include"] {
        b.include(rc.join(d));
    }
    b.define("NDEBUG", None);
    b.opt_level_str(&env::var("MB_RECAST_OPT").unwrap_or_else(|_| OPT.to_string()));
    // portable.h before every file: `/FI` for MSVC (the kit's Windows `mb`), `-include` for the rest.
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc {
        b.flag(format!("/FI{}", here.join("csrc/portable.h").display()));
    } else {
        b.flag(format!("-include{}", here.join("csrc/portable.h").display()));
    }
    // No fused multiply-add where the source has a multiply and an add, so
    // native matches wasm (as crates/luau).
    b.flag_if_supported("-ffp-contract=off");
    if msvc {
        b.flag("/fp:precise");
    }
    b.flag_if_supported("-fno-exceptions");
    b.flag_if_supported("-fno-rtti");
    b.flag_if_supported("-fno-threadsafe-statics");
    let mut files: Vec<PathBuf> = cpp_files(&rc.join("Recast/Source"), &["RecastLayers.cpp"]).into_iter().map(&patch).collect();
    files.extend(cpp_files(&rc.join("Detour/Source"), &[]).into_iter().map(&patch));
    files.extend(cpp_files(&rc.join("DetourCrowd/Source"), &[]));
    files.push(here.join("csrc/shim.cpp"));
    b.files(&files);

    if wasm {
        let sysroot = fetch(&cache, SYSROOT);
        let (clang, ar) = wasm_clang();
        b.compiler(clang);
        b.archiver(ar);
        b.cpp_link_stdlib(None);
        // wasi's headers; nothing of wasi-libc is linked (the target is
        // still wasm32-unknown-unknown when it links).
        b.flag("--target=wasm32-wasip1");
        b.flag(format!("--sysroot={}", sysroot.display()));
        b.flag("-nostdinc++");
        b.flag(format!("-isystem{}", sysroot.join("include/wasm32-wasip1/noeh/c++/v1").display()));
        b.flag("-fvisibility=hidden");
        b.flag("-ffunction-sections");
        b.flag("-fdata-sections");
    }
    b.compile("recast");
}
