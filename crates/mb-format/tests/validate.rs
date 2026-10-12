//! Validator rules, exercised with small hand-written modules and bundles.

use std::collections::BTreeMap;

use mb_format::{pack, validate};

const MANIFEST: &str = r#"
abi = 0
id = "dev.test.game"
name = "Test"
version = "0.1.0"
creator = "@test"
orientation = "portrait"
logical_size = [360, 640]
inputs = ["touch"]
perf_tier = "lite"
stdlib = { mb2d = 1 }
"#;

const EXPORTS: &str = r#"
  (memory (export "memory") 1 2048)
  (func (export "mb_init"))
  (func (export "mb_update") (param f32))
  (func (export "mb_render"))
  (func (export "mb_alloc") (param i32) (result i32) i32.const 0)
"#;

fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!("(module {body})")).unwrap()
}

fn icon(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([8, 6, 0, 0, 0]);
    b
}

fn bundle(manifest: &str, wasm: Vec<u8>, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut files = BTreeMap::new();
    files.insert("manifest.toml".to_string(), manifest.as_bytes().to_vec());
    files.insert("game.wasm".to_string(), wasm);
    files.insert("icon.png".to_string(), icon(256, 256));
    files.insert("src/src/lib.rs".to_string(), b"// source".to_vec());
    for (p, d) in extra {
        files.insert(p.to_string(), d.to_vec());
    }
    pack(&files).unwrap()
}

fn errors(manifest: &str, wasm: Vec<u8>) -> Vec<String> {
    validate(&bundle(manifest, wasm, &[])).errors
}

fn assert_error(errs: &[String], needle: &str) {
    assert!(errs.iter().any(|e| e.contains(needle)), "expected an error containing {needle:?}, got {errs:#?}");
}

#[test]
fn minimal_game_is_valid() {
    let wasm = module(&format!(r#"(import "mb" "mb2d_clear" (func (param i32))) {EXPORTS}"#));
    let r = validate(&bundle(MANIFEST, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
}

#[test]
fn reads_the_sdk_version() {
    let plain = validate(&bundle(MANIFEST, module(EXPORTS), &[]));
    assert!(plain.ok(), "{:#?}", plain.errors);
    assert_eq!(plain.sdk_version, None);
    let built = validate(&bundle(MANIFEST, module(&format!(r#"{EXPORTS} (@custom "maimbrain-sdk" "0.7.0")"#)), &[]));
    assert!(built.ok(), "{:#?}", built.errors);
    assert_eq!(built.sdk_version.as_deref(), Some("0.7.0"));
    let junk = validate(&bundle(MANIFEST, module(&format!(r#"{EXPORTS} (@custom "maimbrain-sdk" "<b>hi</b>")"#)), &[]));
    assert_eq!(junk.sdk_version, None);
}

#[test]
fn pack_is_deterministic() {
    let wasm = module(EXPORTS);
    assert_eq!(bundle(MANIFEST, wasm.clone(), &[]), bundle(MANIFEST, wasm, &[]));
}

#[test]
fn rejects_wasi_imports() {
    let wasm = module(&format!(r#"(import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "only the \"mb\" namespace");
}

#[test]
fn rejects_unknown_host_functions() {
    let wasm = module(&format!(r#"(import "mb" "mb_open_socket" (func)) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "unknown host function mb.mb_open_socket");
}

#[test]
fn audio_scheduling_and_time_lost_need_no_declaration() {
    let no_stdlib = MANIFEST.replace("stdlib = { mb2d = 1 }", "");
    let wasm = module(&format!(
        r#"(import "mb" "mb_play_at" (func (param i32 f32 f32 f32 i32 f64) (result i32)))
           (import "mb" "mb_time_lost" (func (result f32))) {EXPORTS}"#
    ));
    let r = validate(&bundle(&no_stdlib, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    // `at` is game time, an f64 like mb_time.
    let wasm = module(&format!(r#"(import "mb" "mb_play_at" (func (param i32 f32 f32 f32 i32 f32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "mb.mb_play_at has signature");
}

#[test]
fn playing_from_an_offset_and_releasing_need_no_declaration() {
    let no_stdlib = MANIFEST.replace("stdlib = { mb2d = 1 }", "");
    let wasm = module(&format!(
        r#"(import "mb" "mb_play_from" (func (param i32 f32 f32 f32 i32 f64 f64 f32) (result i32)))
           (import "mb" "mb_voice_stop_at" (func (param i32 f64 f32))) {EXPORTS}"#
    ));
    let r = validate(&bundle(&no_stdlib, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    // `from` is seconds into the sound, an f64 like `at`.
    let wasm = module(&format!(r#"(import "mb" "mb_play_from" (func (param i32 f32 f32 f32 i32 f64 f32 f32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "mb.mb_play_from has signature");
}

#[test]
fn bus_effects_need_no_declaration() {
    let no_stdlib = MANIFEST.replace("stdlib = { mb2d = 1 }", "");
    let wasm = module(&format!(
        r#"(import "mb" "mb_bus_fx" (func (param i32 i32 i32) (result i32)))
           (import "mb" "mb_voice_bus" (func (param i32 i32))) {EXPORTS}"#
    ));
    let r = validate(&bundle(&no_stdlib, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    // mb_bus_fx returns a status (0 or -2).
    let wasm = module(&format!(r#"(import "mb" "mb_bus_fx" (func (param i32 i32 i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "mb.mb_bus_fx has signature");
}

#[test]
fn rejects_wrong_signatures() {
    let wasm = module(&format!(r#"(import "mb" "mb2d_clear" (func (param f32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "mb.mb2d_clear has signature (f32) -> ()");
}

#[test]
fn rejects_undeclared_stdlib_sensors_and_capabilities() {
    let no_stdlib = MANIFEST.replace("stdlib = { mb2d = 1 }", "");
    let wasm = module(&format!(r#"(import "mb" "mb2d_clear" (func (param i32))) {EXPORTS}"#));
    assert_error(&errors(&no_stdlib, wasm), "without declaring stdlib { mb2d = 1 }");

    let wasm = module(&format!(r#"(import "mb" "mb_tilt" (func (param i32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "sensors = [\"tilt\"]");

    let wasm = || module(&format!(r#"(import "mb" "mb_attitude" (func (param i32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm()), "sensors = [\"attitude\"]");
    let declared = format!("{MANIFEST}sensors = [\"attitude\"]\nneeds = [\"attitude\"]\n");
    assert!(errors(&declared, wasm()).is_empty(), "{:?}", errors(&declared, wasm()));

    let wasm = module(&format!(r#"(import "mb" "mb_store_set" (func (param i32 i32 i32 i32) (result i32))) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm), "capabilities = [\"store\"]");
}

#[test]
fn needs_must_be_declared_input_sensors() {
    let wasm = || module(EXPORTS);
    let ok = format!("{MANIFEST}sensors = [\"tilt\", \"haptics\"]\nneeds = [\"tilt\"]\n");
    assert!(errors(&ok, wasm()).is_empty(), "{:?}", errors(&ok, wasm()));

    let undeclared = format!("{MANIFEST}needs = [\"tilt\"]\n");
    assert_error(&errors(&undeclared, wasm()), "also needs sensors = [\"tilt\"]");

    let haptics = format!("{MANIFEST}sensors = [\"haptics\"]\nneeds = [\"haptics\"]\n");
    assert_error(&errors(&haptics, wasm()), "haptics is output-only");
}

#[test]
fn orientation_fit_and_logical_size_agree() {
    use mb_format::manifest::{Fit, Orientation};
    let wasm = || module(EXPORTS);
    let landscape = MANIFEST.replace("\"portrait\"", "\"landscape\"").replace("[360, 640]", "[640, 360]");
    assert!(errors(&landscape, wasm()).is_empty(), "{:?}", errors(&landscape, wasm()));
    let any = format!("{}fit = \"fill\"\n", MANIFEST.replace("\"portrait\"", "\"any\""));
    assert!(errors(&any, wasm()).is_empty(), "{:?}", errors(&any, wasm()));
    let m = mb_format::Manifest::parse(&any).unwrap();
    assert_eq!((m.orientation, m.fit), (Orientation::Any, Fit::Fill));
    assert_eq!(mb_format::Manifest::parse(MANIFEST).unwrap().fit, Fit::Letterbox);

    assert_error(&errors(&MANIFEST.replace("\"portrait\"", "\"landscape\""), wasm()), "a landscape game is at least as wide");
    assert_error(&errors(&any.replace("[360, 640]", "[640, 360]"), wasm()), "an any game gives its portrait size");
    assert_error(&errors(&MANIFEST.replace("[360, 640]", "[640, 360]"), wasm()), "a portrait game gives its portrait size");
    assert_error(&errors(&format!("{MANIFEST}fit = \"stretch\"\n"), wasm()), "fit");
}

#[test]
fn progress_needs_store() {
    let wasm = || module(EXPORTS);
    let ok = format!("{MANIFEST}capabilities = [\"store\", \"progress\"]\n");
    assert!(errors(&ok, wasm()).is_empty(), "{:?}", errors(&ok, wasm()));
    assert_error(&errors(&format!("{MANIFEST}capabilities = [\"progress\"]\n"), wasm()), "also needs \"store\"");
}

#[test]
fn requires_lists_inputs_a_player_must_connect() {
    let wasm = || module(EXPORTS);
    let with_inputs = |inputs: &str, extra: &str| format!("{}{extra}", MANIFEST.replace("inputs = [\"touch\"]", inputs));
    let pad_only = with_inputs("inputs = [\"gamepad\", \"keyboard\"]", "requires = [\"gamepad\"]\n");
    assert!(errors(&pad_only, wasm()).is_empty(), "{:?}", errors(&pad_only, wasm()));
    let either = with_inputs("inputs = [\"touch\", \"gamepad\"]", "");
    assert!(errors(&either, wasm()).is_empty(), "{:?}", errors(&either, wasm()));

    assert_error(&errors(&with_inputs("inputs = [\"gamepad\"]", ""), wasm()), "a game without touch in inputs must say");
    assert_error(&errors(&with_inputs("inputs = [\"touch\"]", "requires = [\"gamepad\"]\n"), wasm()), "also needs inputs = [\"gamepad\"]");
    assert_error(&errors(&with_inputs("inputs = [\"touch\"]", "requires = [\"touch\"]\n"), wasm()), "touch is always there");
    assert_error(&errors(&with_inputs("inputs = [\"gamepad\"]", "requires = [\"gamepad\", \"gamepad\"]\n"), wasm()), "must not repeat");
}

#[test]
fn enforces_memory_maximum_by_tier() {
    let unbounded = EXPORTS.replace("1 2048", "1");
    assert_error(&errors(MANIFEST, module(&unbounded)), "must declare a maximum");

    let too_big = EXPORTS.replace("1 2048", "1 4096");
    assert_error(&errors(MANIFEST, module(&too_big)), "perf_tier Lite allows 2048");
    let full = MANIFEST.replace("\"lite\"", "\"full\"");
    assert!(errors(&full, module(&too_big)).is_empty());
}

#[test]
fn requires_exports_with_exact_signatures() {
    let missing = EXPORTS.replace(r#"(func (export "mb_render"))"#, "");
    assert_error(&errors(MANIFEST, module(&missing)), "missing required export function mb_render");

    let wrong = EXPORTS.replace(r#"(func (export "mb_update") (param f32))"#, r#"(func (export "mb_update") (param f64))"#);
    assert_error(&errors(MANIFEST, module(&wrong)), "export mb_update has signature (f64) -> ()");
}

#[test]
fn rejects_disallowed_wasm_features() {
    let shared = EXPORTS.replace("(memory (export \"memory\") 1 2048)", "(memory (export \"memory\") 1 2048 shared)");
    assert_error(&errors(MANIFEST, module(&shared)), "disallowed feature");
}

#[test]
fn checks_bundle_layout() {
    let wasm = module(EXPORTS);
    let bad = |extra: &[(&str, &[u8])]| validate(&bundle(MANIFEST, wasm.clone(), extra)).errors;
    assert_error(&bad(&[("../escape.txt", b"x")]), "'..'");
    assert_error(&bad(&[("notes.txt", b"x")]), "unexpected top-level file");
    assert_error(&bad(&[("assets/run.sh", b"x")]), "asset type .sh is not allowed");
    assert_error(&bad(&[("assets/A.png", b"x"), ("assets/a.png", b"y")]), "differ only by case");
}

#[test]
fn checks_icon_and_manifest_fields() {
    let mut files = BTreeMap::new();
    files.insert("manifest.toml".to_string(), MANIFEST.replace("dev.test.game", "Not An Id").into_bytes());
    files.insert("game.wasm".to_string(), module(EXPORTS));
    files.insert("icon.png".to_string(), icon(128, 128));
    let errs = validate(&pack(&files).unwrap()).errors;
    assert_error(&errs, "icon.png is 128×128");
    assert_error(&errs, "must be reverse-DNS");
    assert_error(&errs, "missing src/");
}

#[test]
fn checks_score_boards() {
    let wasm = || module(EXPORTS);
    let with = |extra: &str| format!("{MANIFEST}{extra}");
    let ok = with("capabilities = [\"score\"]\n[[scores]]\nboard = 0\nlabel = \"Height\"\norder = \"higher\"\n");
    assert!(errors(&ok, wasm()).is_empty(), "{:#?}", errors(&ok, wasm()));
    let no_cap = with("[[scores]]\nboard = 0\nlabel = \"Height\"\norder = \"higher\"\n");
    assert_error(&errors(&no_cap, wasm()), "need capabilities = [\"score\"]");
    let no_board = with("capabilities = [\"score\"]\n");
    assert_error(&errors(&no_board, wasm()), "needs at least one [[scores]] board");
    let dup = with("capabilities = [\"score\"]\n[[scores]]\nboard = 1\nlabel = \"A\"\norder = \"lower\"\nformat = \"milliseconds\"\n[[scores]]\nboard = 1\nlabel = \"B\"\norder = \"higher\"\n");
    assert_error(&errors(&dup, wasm()), "board numbers must be unique");
}

#[test]
fn replay_banner_is_bottom_or_top() {
    use mb_format::manifest::ReplayBanner;
    let parse = |extra: &str| mb_format::Manifest::parse(&format!("{MANIFEST}{extra}"));
    assert_eq!(parse("").unwrap().replay_banner, ReplayBanner::Bottom);
    assert_eq!(parse("replay_banner = \"top\"\n").unwrap().replay_banner, ReplayBanner::Top);
    assert!(errors(&format!("{MANIFEST}replay_banner = \"top\"\n"), module(EXPORTS)).is_empty());
    let err = parse("replay_banner = \"middle\"\n").unwrap_err();
    assert!(err.contains("`bottom`") && err.contains("`top`"), "{err}");
    assert_error(&errors(&format!("{MANIFEST}replay_banner = \"middle\"\n"), module(EXPORTS)), "replay_banner");
}

#[test]
fn daily_seed_is_stable_and_per_game() {
    // Locked: changing the derivation would change every game's daily challenge.
    let a = mb_format::daily_seed("dev.maimbrain.stack", "2026-10-06");
    assert_eq!(a, mb_format::daily_seed("dev.maimbrain.stack", "2026-10-06"));
    assert_ne!(a, mb_format::daily_seed("dev.maimbrain.stack", "2026-10-07"));
    assert_ne!(a, mb_format::daily_seed("dev.maimbrain.hello", "2026-10-06"));
    assert_eq!(a, 0x5e362aeb5507a232);
}

#[test]
fn gates_mb3d_on_its_stdlib() {
    let wasm = || module(&format!(r#"(import "mb" "mb3d_node" (func (result i32))) (import "mb" "mb3d_render" (func)) {EXPORTS}"#));
    assert_error(&errors(MANIFEST, wasm()), "without declaring stdlib { mb3d = 1 }");
    let both = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 1 }");
    assert!(errors(&both, wasm()).is_empty(), "{:#?}", errors(&both, wasm()));
    let wrong = module(&format!(r#"(import "mb" "mb3d_node_transform" (func (param i32))) {EXPORTS}"#));
    assert_error(&errors(&both, wrong), "mb.mb3d_node_transform has signature (i32) -> ()");
}

/// mb3d 2 only adds imports: a game declaring mb3d = 1 keeps validating, a
/// game using a version-2 import must declare mb3d = 2, and versions the host
/// doesn't have are rejected.
#[test]
fn mb3d_versions_add_imports_without_breaking_old_games() {
    let v1 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 1 }");
    let v2 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 2 }");
    let old = || module(&format!(r#"(import "mb" "mb3d_node" (func (result i32))) (import "mb" "mb3d_render" (func)) {EXPORTS}"#));
    let new = || module(&format!(r#"(import "mb" "mb3d_anim" (func (param i32 i32 f32 f32) (result i32))) (import "mb" "mb3d_free" (func (param i32 i32) (result i32))) {EXPORTS}"#));
    assert!(errors(&v1, old()).is_empty(), "{:#?}", errors(&v1, old()));
    assert!(errors(&v2, old()).is_empty(), "{:#?}", errors(&v2, old()));
    assert!(errors(&v2, new()).is_empty(), "{:#?}", errors(&v2, new()));
    assert_error(&errors(&v1, new()), "imports mb.mb3d_anim without declaring stdlib { mb3d = 2 }");
    let v3 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 3 }");
    assert_error(&errors(&v3, old()), "stdlib mb3d = 3 is not available (host has 1–2)");
    let v0 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 0 }");
    assert_error(&errors(&v0, old()), "stdlib mb3d = 0 is not available");
}

fn glb(json: &str, declared_extra: i64) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let total = 20 + j.len();
    let mut b = b"glTF".to_vec();
    b.extend(2u32.to_le_bytes());
    b.extend(((total as i64 + declared_extra) as u32).to_le_bytes());
    b.extend((j.len() as u32).to_le_bytes());
    b.extend(0x4E4F534Au32.to_le_bytes());
    b.extend(j);
    b
}

#[test]
fn checks_glb_assets() {
    let wasm = || module(EXPORTS);
    let ok = glb(r#"{"asset":{"version":"2.0"}}"#, 0);
    let r = validate(&bundle(MANIFEST, wasm(), &[("assets/ship.glb", &ok), ("assets/hull.jpg", &[0xff, 0xd8])]));
    assert!(r.ok(), "{:#?}", r.errors);
    let short = glb(r#"{"asset":{"version":"2.0"}}"#, 8);
    assert_error(&validate(&bundle(MANIFEST, wasm(), &[("assets/ship.glb", &short)])).errors, "declared length");
    assert_error(&validate(&bundle(MANIFEST, wasm(), &[("assets/ship.glb", b"not a gltf file at all")])).errors, "not a binary glTF");
}
