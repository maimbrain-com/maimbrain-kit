//! The engine's own Lua, compiled to Luau bytecode when the engine is built
//! (docs/STUDIO-2.md §4.13, Decided 36): the engine has no compiler, and
//! `mb build` compiles a project's Lua the same way.
//!
//! Also `cfg(three)`: the 3D API is compiled in (api/three.rs). The
//! published 3D build has it (Cargo feature `3d`), and so does every native
//! build, so `cargo test` plays 2D and 3D projects; the plain wasm build
//! doesn't, so it imports nothing from mb3d. And `cfg(sense_motion)`,
//! `cfg(sense_mic)`: the sensors the build imports (features `motion`,
//! `mic`; api/sensors.rs), all of them natively.
//!
//! And `cfg(shaders)`: games' shaders (api/shaders.rs) call mb3d 5 and mb2d
//! 4, in the shader builds (Cargo feature `shaders`) and natively.
//!
//! And `cfg(phys2)`: `love.physics` (api/lovephysics.rs), in the 2D build
//! and natively; the 3D build's bodies are Godot's (api/bodies3d.rs).
use std::path::PathBuf;

/// The chunks, by name under src/ (their bytecode is `$OUT_DIR/<name>.luac`).
const CHUNKS: &[&str] = &["prelude", "prelude3d", "classic"];

fn main() {
    println!("cargo:rustc-check-cfg=cfg(three)");
    println!("cargo:rustc-check-cfg=cfg(sense_motion, sense_mic)");
    println!("cargo:rustc-check-cfg=cfg(phys2)");
    println!("cargo:rustc-check-cfg=cfg(shaders)");
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let three = std::env::var_os("CARGO_FEATURE_3D").is_some();
    if three || !wasm {
        println!("cargo:rustc-cfg=three");
    }
    // The sensors a build imports (docs/STUDIO-2.md §4.5): each set has the
    // one before it. Native builds have them all (the SDK's stubs).
    for (feature, cfg) in [("CARGO_FEATURE_MOTION", "sense_motion"), ("CARGO_FEATURE_MIC", "sense_mic")] {
        if std::env::var_os(feature).is_some() || !wasm {
            println!("cargo:rustc-cfg={cfg}");
        }
    }
    if !three || !wasm {
        println!("cargo:rustc-cfg=phys2");
    }
    // Games' shaders (Cargo feature `shaders`): only the shader builds import
    // mb3d 5 and mb2d 4; native builds always have them.
    if std::env::var_os("CARGO_FEATURE_SHADERS").is_some() || !wasm {
        println!("cargo:rustc-cfg=shaders");
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for name in CHUNKS {
        let src = format!("src/{name}.lua");
        println!("cargo:rerun-if-changed={src}");
        let text = std::fs::read(&src).unwrap();
        match luau::compile(&text) {
            Ok(bc) => std::fs::write(out.join(format!("{name}.luac")), bc).unwrap(),
            Err(e) => panic!("{src}:{}: {}", e.line, e.message),
        }
    }
}
