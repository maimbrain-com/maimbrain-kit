//! `game.wasm` checks (SPEC §3, §5): allowed features, imports only from the
//! `mb` table with exact signatures and declared requirements, required
//! exports, and a bounded linear memory.

use wasmparser::{ExternalKind, FuncType, Parser, Payload, TypeRef, ValType, Validator, WasmFeatures};

use crate::abi::{self, Ty};
use crate::manifest::Manifest;

/// Wasm 2.0: bulk memory, reference types, sign-ext, saturating float→int,
/// multi-value, 128-bit SIMD. No threads, exceptions, relaxed SIMD, memory64,
/// multi-memory, tail calls or GC.
pub fn features() -> WasmFeatures {
    WasmFeatures::WASM2
}

pub fn check(wasm: &[u8], manifest: &Manifest, errors: &mut Vec<String>) {
    let mut err = |m: String| errors.push(format!("game.wasm: {m}"));
    let types = match Validator::new_with_features(features()).validate_all(wasm) {
        Ok(t) => t,
        Err(e) => return err(format!("invalid or uses a disallowed feature: {e}")),
    };
    let types = types.as_ref();
    let func_type = |id: wasmparser::types::CoreTypeId| types[id].unwrap_func().clone();

    for payload in Parser::new(0).parse_all(wasm) {
        let Ok(payload) = payload else { return };
        match payload {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let Ok(import) = import else { return err("unreadable import section".into()) };
                    if import.module != "mb" {
                        err(format!("imports {}.{}: only the \"mb\" namespace is allowed (no WASI)", import.module, import.name));
                        continue;
                    }
                    let TypeRef::Func(ty) = import.ty else {
                        err(format!("imports mb.{} as a non-function; memory, tables and globals must be defined by the module", import.name));
                        continue;
                    };
                    let Some(spec) = abi::lookup(import.name) else {
                        err(unknown_import(import.name));
                        continue;
                    };
                    let actual = func_type(types.core_type_at_in_module(ty));
                    if !matches(&actual, spec.params, spec.results) {
                        err(format!(
                            "mb.{} has signature {} but the host defines {}",
                            import.name,
                            show_actual(&actual),
                            show(spec.params, spec.results)
                        ));
                    }
                    if let Err(missing) = spec.requires.satisfied_by(manifest) {
                        err(format!("imports mb.{} without declaring {missing} in manifest.toml", import.name));
                    }
                }
            }
            Payload::ExportSection(reader) => {
                let mut funcs = Vec::new();
                let mut has_memory = false;
                for export in reader {
                    let Ok(export) = export else { return err("unreadable export section".into()) };
                    if export.name.starts_with(crate::meter::RESERVED_PREFIX) {
                        err(format!("export {} uses the reserved prefix {}", export.name, crate::meter::RESERVED_PREFIX));
                    }
                    match export.kind {
                        ExternalKind::Memory if export.name == "memory" => has_memory = true,
                        ExternalKind::Func => {
                            funcs.push(export.name);
                            if let Some(spec) = abi::EXPORTS.iter().find(|e| e.name == export.name) {
                                let actual = func_type(types.core_function_at(export.index));
                                if !matches(&actual, spec.params, spec.results) {
                                    err(format!(
                                        "export {} has signature {} but must be {}",
                                        export.name,
                                        show_actual(&actual),
                                        show(spec.params, spec.results)
                                    ));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if !has_memory {
                    err("must export its linear memory as \"memory\"".into());
                }
                for spec in abi::EXPORTS.iter().filter(|e| e.required) {
                    if !funcs.contains(&spec.name) {
                        err(format!("missing required export function {}", spec.name));
                    }
                }
            }
            _ => {}
        }
    }

    if types.memory_count() != 1 {
        return err(format!("must define exactly one linear memory (found {})", types.memory_count()));
    }
    let mem = types.memory_at(0);
    let cap = manifest.perf_tier.max_memory_pages();
    match mem.maximum {
        None => err(format!(
            "linear memory must declare a maximum (≤ {cap} pages for perf_tier {:?}); link with --max-memory",
            manifest.perf_tier
        )),
        Some(max) if max > cap => err(format!(
            "linear memory maximum is {max} pages ({} MiB); perf_tier {:?} allows {cap} ({} MiB)",
            max / 16,
            manifest.perf_tier,
            cap / 16
        )),
        _ => {}
    }
}

/// The custom section the SDK's `export_game!` writes its version into.
pub const SDK_SECTION: &str = "maimbrain-sdk";

/// The SDK version recorded in `wasm` (None without the section, or if it
/// doesn't hold a short version string).
pub fn sdk_version(wasm: &[u8]) -> Option<String> {
    Parser::new(0).parse_all(wasm).find_map(|p| match p {
        Ok(Payload::CustomSection(c)) if c.name() == SDK_SECTION => {
            let v = std::str::from_utf8(c.data()).ok()?;
            let ok = !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphanumeric() || ".+-".contains(c));
            ok.then(|| v.to_string())
        }
        _ => None,
    })
}

/// Why an unknown import can't work and what to do instead: games reach the
/// host only through the SDK, so this is a hand-written declaration (or a
/// crate that isn't the Maimbrain SDK) for a function the host doesn't have.
fn unknown_import(name: &str) -> String {
    // Things agents often expect from the host that the SDK does game-side.
    let lower = name.to_ascii_lowercase();
    let concept = [
        (&["speed", "velocity", "timescale", "time_scale"][..], "Speeds are game-side: scale the dt or the time you pass (e.g. `gfx3d::Animator` speed, the time given to `mb3d_anim`)."),
        (&["physics", "rigid", "collide", "collision", "raycast", "gravity", "impulse"][..], "Physics is game-side: the SDK's `physics2d` / `physics3d` Cargo features (docs/PHYSICS.md)."),
        (&["tween", "ease", "spring", "shake", "confetti", "button", "ui_"][..], "Motion and UI are game-side: `maimbrain::motion`, `maimbrain::juice` and `maimbrain::ui` (docs/UI.md)."),
    ]
    .iter()
    .find(|(words, _)| words.iter().any(|w| lower.contains(w)))
    .map(|(_, h)| format!(" {h}"))
    .unwrap_or_default();
    let hint = concept + &match abi::suggest(name).as_slice() {
        [] => String::new(),
        [one] => format!(" Did you mean mb.{one}?"),
        many => format!(" Did you mean one of: {}?", many.iter().map(|n| format!("mb.{n}")).collect::<Vec<_>>().join(", ")),
    };
    format!(
        "imports unknown host function mb.{name}: the Maimbrain host has no such function, so this game can't run anywhere. \
         Something in the game declares it by hand (an `extern \"C\"` block, or a crate other than the Maimbrain SDK): \
         remove that and call the SDK (`maimbrain::…`) instead; SPEC §5 lists every host function.{hint}"
    )
}

fn ty(t: Ty) -> ValType {
    match t {
        Ty::I32 => ValType::I32,
        Ty::I64 => ValType::I64,
        Ty::F32 => ValType::F32,
        Ty::F64 => ValType::F64,
    }
}

fn matches(f: &FuncType, params: &[Ty], results: &[Ty]) -> bool {
    f.params().iter().copied().eq(params.iter().map(|&t| ty(t)))
        && f.results().iter().copied().eq(results.iter().map(|&t| ty(t)))
}

fn show(params: &[Ty], results: &[Ty]) -> String {
    let list = |v: &[Ty]| v.iter().map(|t| format!("{:?}", ty(*t)).to_lowercase()).collect::<Vec<_>>().join(", ");
    format!("({}) -> ({})", list(params), list(results))
}

fn show_actual(f: &FuncType) -> String {
    let list = |v: &[ValType]| v.iter().map(|t| format!("{t:?}").to_lowercase()).collect::<Vec<_>>().join(", ");
    format!("({}) -> ({})", list(f.params()), list(f.results()))
}

#[cfg(test)]
mod unknown_import_tests {
    use super::unknown_import;

    #[test]
    fn explains_and_points_somewhere_useful() {
        let speed = unknown_import("mb3d_speed");
        assert!(speed.contains("no such function") && speed.contains("extern"), "{speed}");
        assert!(speed.contains("Animator") && !speed.contains("mb3d_free"), "{speed}");
        assert!(unknown_import("mb2d_textt").contains("mb.mb2d_text"));
        assert!(unknown_import("mb_raycast").contains("physics2d"));
        assert!(!unknown_import("zzz_unrelated").contains("Did you mean"));
    }
}
