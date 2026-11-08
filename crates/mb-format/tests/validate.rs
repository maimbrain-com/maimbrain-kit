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
    assert!(errors(&v3, new()).is_empty(), "mb3d 3 keeps mb3d 2's imports");
    let v4 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 4 }");
    assert!(errors(&v4, new()).is_empty(), "mb3d 4 keeps mb3d 2's imports");
    let v5 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 5 }");
    assert!(errors(&v5, new()).is_empty(), "mb3d 5 keeps mb3d 2's imports");
    let v6 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 6 }");
    assert_error(&errors(&v6, old()), "stdlib mb3d = 6 is not available (host has 1–5)");
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

#[test]
fn one_daily_board_and_never_board_0() {
    let wasm = || module(EXPORTS);
    let board = |n: u32, daily: bool| format!("[[scores]]\nboard = {n}\nlabel = \"B{n}\"\norder = \"higher\"\ndaily = {daily}\n");
    let m = |boards: &str| format!("{MANIFEST}capabilities = [\"score\"]\n{boards}");
    assert!(errors(&m(&(board(0, false) + &board(1, true))), wasm()).is_empty());
    assert_error(&errors(&m(&board(0, true)), wasm()), "board 0 can't be daily");
    assert_error(&errors(&m(&(board(1, true) + &board(2, true))), wasm()), "at most one board can be daily");
}

/// Studio bundles (SPEC "Studio bundles"): the project under assets/, the
/// engine named in the manifest, and the engine's Lua in src/.
#[test]
fn studio_bundles() {
    let studio = format!("{MANIFEST}engine = \"studio@1.0\"\n");
    let project: &[(&str, &[u8])] = &[
        ("assets/studio.toml", b"title = \"Cat Snack\"\n"),
        ("assets/game/main.lua", b"function start() end\n"),
        ("assets/maps/level1.map", b"[map]\n.\n"),
        ("assets/files.txt", b"studio.toml\ngame/main.lua\nmaps/level1.map\n"),
        ("src/src/prelude.lua", b"-- the engine's Lua\n"),
    ];
    let r = validate(&bundle(&studio, module(EXPORTS), project));
    assert!(r.ok(), "{:#?}", r.errors);
    let info = r.studio.expect("a Studio bundle");
    assert_eq!((info.engine.as_str(), info.variant.as_str(), info.published), ("1.0", "plain", false));
    assert!(r.warnings.iter().any(|w| w.contains("isn't a published Studio engine 1.0")), "{:?}", r.warnings);
    assert_eq!(r.manifest.unwrap().needs_formats(), ["studio"]);

    // The project has to be there, as files.txt says.
    let r = validate(&bundle(&studio, module(EXPORTS), &project[..2]));
    assert_error(&r.errors, "assets/files.txt is missing");
    let listed: &[(&str, &[u8])] = &[project[0], project[1], ("assets/files.txt", b"studio.toml\ngame/main.lua\nsprites/cat.png\n")];
    assert_error(&validate(&bundle(&studio, module(EXPORTS), listed)).errors, "lists sprites/cat.png, but assets/sprites/cat.png isn't in the bundle");
    let escape: &[(&str, &[u8])] = &[project[0], project[1], ("assets/files.txt", b"../manifest.toml\n")];
    assert_error(&validate(&bundle(&studio, module(EXPORTS), escape)).errors, "isn't a project path");
    assert_error(&errors(&format!("{MANIFEST}engine = \"lua@1\"\n"), module(EXPORTS)), "the only engine is Studio's");

    // A plain game may carry these types too, and needs nothing new.
    let r = validate(&bundle(MANIFEST, module(EXPORTS), &[("assets/level.toml", b"a = 1"), ("assets/ai.lua", b"x = 1")]));
    assert!(r.ok() && r.studio.is_none(), "{:#?}", r.errors);
    assert!(r.manifest.unwrap().needs_formats().is_empty());

    // The engine build with tilt is the tilt variant.
    let tilt = format!("{studio}sensors = [\"tilt\"]\n");
    let wasm = module(&format!(r#"(import "mb" "mb_tilt" (func (param i32) (result i32))) {EXPORTS}"#));
    let r = validate(&bundle(&tilt, wasm, project));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.studio.unwrap().variant.as_str(), "tilt");
}

/// Engine 2.0's bundles (docs/STUDIO-2.md §4.8, §4.13): format `studio2`,
/// Lua with its bytecode beside it, and wasm exception handling, which only a
/// Studio 2.0 engine may use.
#[test]
fn studio2_bundles() {
    let studio2 = format!("{MANIFEST}engine = \"studio@2.0\"\n");
    // exnref: a tag, a throw and a try_table, as Luau's C++ unwinding compiles.
    let eh = || {
        module(&format!(
            r#"(tag $e) (func $f (throw $e)) (func $g (block $out (try_table (catch_all $out) (call $f)))) {EXPORTS}"#
        ))
    };
    let project: &[(&str, &[u8])] = &[
        ("assets/studio.toml", b"title = \"Catch\"\nengine = \"2.0\"\n"),
        ("assets/game/main.lua", b"function love.load() end\n"),
        ("assets/game/main.luac", &[14, 3, 0, 0]),
        ("assets/files.txt", b"studio.toml\ngame/main.lua\ngame/main.luac\n"),
        ("src/src/prelude.lua", b"-- the engine's Lua\n"),
    ];
    let r = validate(&bundle(&studio2, eh(), project));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.studio.unwrap().engine, "2.0");
    // The loop-metering pass takes exnref as it is (S0).
    let opened = mb_format::Bundle::open(&bundle(&studio2, eh(), project)).expect("opens and meters");
    assert!(opened.runtime_wasm().len() > 0);
    assert_eq!(r.manifest.unwrap().needs_formats(), ["studio2"]);

    // Exceptions are for Studio 2.0's engine only: not 1.0's, not a kit game's.
    assert_error(&errors(&format!("{MANIFEST}engine = \"studio@1.0\"\n"), eh()), "disallowed feature");
    assert_error(&errors(MANIFEST, eh()), "disallowed feature");

    // Every chunk has its bytecode, of a version the VM loads, and every
    // bytecode file its source.
    let missing: &[(&str, &[u8])] = &[project[0], project[1], ("assets/files.txt", b"studio.toml\ngame/main.lua\n")];
    assert_error(&validate(&bundle(&studio2, eh(), missing)).errors, "game/main.lua has no bytecode beside it");
    let old: &[(&str, &[u8])] = &[project[0], project[1], ("assets/game/main.luac", &[2, 0]), project[3]];
    assert_error(&validate(&bundle(&studio2, eh(), old)).errors, "bytecode version 2");
    let error: &[(&str, &[u8])] = &[project[0], project[1], ("assets/game/main.luac", b"\0:1: Expected"), project[3]];
    assert_error(&validate(&bundle(&studio2, eh(), error)).errors, "is a compile error");
    let orphan: &[(&str, &[u8])] = &[project[0], project[1], project[2], ("assets/game/lib.luac", &[14]), ("assets/files.txt", b"studio.toml\ngame/main.lua\n")];
    assert_error(&validate(&bundle(&studio2, eh(), orphan)).errors, "lib.luac has no source beside it");
}

/// Bytes deflate can't shrink, so a bundle is about as big as its assets.
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// Bundles up to 25 MB (SPEC §1; docs/STUDIO-2.md §4.8): over 10 MB a bundle
/// is format `large`, which only apps whose validator opens it get.
#[test]
fn large_bundles() {
    const MIB: usize = 1024 * 1024;
    let small = validate(&bundle(MANIFEST, module(EXPORTS), &[("assets/a.bin", &noise(9 * MIB, 1))]));
    assert!(small.ok(), "{:#?}", small.errors);
    assert!(small.formats().is_empty());

    let large = validate(&bundle(MANIFEST, module(EXPORTS), &[("assets/a.bin", &noise(12 * MIB, 2))]));
    assert!(large.ok(), "{:#?}", large.errors);
    assert!(large.compressed_size > mb_format::bundle::LARGE);
    assert_eq!(large.formats(), ["large"]);
    assert!(mb_format::studio::FORMATS.contains(&"large"), "this validator opens them");

    // A Studio bundle over 10 MB needs both.
    let studio = format!("{MANIFEST}engine = \"studio@1.0\"\n");
    let project: &[(&str, &[u8])] = &[
        ("assets/studio.toml", b"title = \"Big\"\n"),
        ("assets/game/main.lua", b"function start() end\n"),
        ("assets/files.txt", b"studio.toml\ngame/main.lua\n"),
        ("src/src/prelude.lua", b"-- the engine's Lua\n"),
    ];
    let big = noise(11 * MIB, 3);
    let r = validate(&bundle(&studio, module(EXPORTS), &[project, &[("assets/painting.jpg", &big[..])]].concat()));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.formats(), ["studio", "large"]);

    let huge = noise(26 * MIB, 4);
    let r = validate(&bundle(MANIFEST, module(EXPORTS), &[("assets/a.bin", &huge[..])]));
    assert_error(&r.errors, "the limit is 26214400 (25 MiB)");
}

/// A game that declares mb2d 3 (JPEG, the nearest filter) needs format
/// `mb2d3`: validators before it refuse `mb2d = 3`, so the feed sends it
/// only to apps that claim it.
#[test]
fn mb2d_3_is_a_format() {
    let v3 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 3 }");
    let r = validate(&bundle(&v3, module(EXPORTS), &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.formats(), ["mb2d3"]);
    let v2 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 2 }");
    assert!(validate(&bundle(&v2, module(EXPORTS), &[])).formats().is_empty());
    assert!(mb_format::studio::FORMATS.contains(&"mb2d3"), "apps claim it from their 2.0 release (1e) on");
}

/// mb3d 3 (`mb3d_node_transforms`) is format `mb3d3` the same way, and a
/// game importing it must declare it.
#[test]
fn mb3d_3_is_a_format_and_its_import_needs_it() {
    let import = r#"(import "mb" "mb3d_node_transforms" (func (param i32 i32) (result i32)))"#;
    let wasm = module(&format!("{import} {EXPORTS}"));
    let v2 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 2 }").replace("perf_tier = \"lite\"", "perf_tier = \"full\"");
    let r = validate(&bundle(&v2, wasm.clone(), &[]));
    assert_error(&r.errors, "mb3d_node_transforms");
    let v3 = v2.replace("mb3d = 2", "mb3d = 3");
    let r = validate(&bundle(&v3, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.formats(), ["mb3d3"]);
    assert!(validate(&bundle(&v2, module(EXPORTS), &[])).formats().is_empty());
    assert!(mb_format::studio::FORMATS.contains(&"mb3d3"), "apps claim it from the release after Phase 2a");
}

/// mb3d 4 (`mb3d_material_surfaces`, Decided 70) is format `mb3d4`, which a
/// game declaring it needs beside `mb3d3`; its import needs it declared.
#[test]
fn mb3d_4_is_a_format_and_its_import_needs_it() {
    let import = r#"(import "mb" "mb3d_material_surfaces" (func (param i32 i32 i32) (result i32)))"#;
    let wasm = module(&format!("{import} {EXPORTS}"));
    let v3 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 1, mb3d = 3 }").replace("perf_tier = \"lite\"", "perf_tier = \"full\"");
    let r = validate(&bundle(&v3, wasm.clone(), &[]));
    assert_error(&r.errors, "mb3d_material_surfaces");
    let v4 = v3.replace("mb3d = 3", "mb3d = 4");
    let r = validate(&bundle(&v4, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.formats(), ["mb3d3", "mb3d4"]);
    assert!(mb_format::studio::FORMATS.contains(&"mb3d4"), "apps claim it from the release that brings ground surfaces");
}

/// Games' shaders (mb3d 5, mb2d 4; docs/STUDIO-2.md §4.15) are formats
/// `mb3d5` and `mb2d4`; their imports need them declared, and a shader
/// game is packed with a shader build (`studio::variant`).
#[test]
fn shaders_are_formats_and_their_imports_need_them() {
    let imports = r#"(import "mb" "mb3d_shader" (func (param i32 i32) (result i32))) (import "mb" "mb2d_shader_use" (func (param i32 i32) (result i32)))"#;
    let wasm = module(&format!("{imports} {EXPORTS}"));
    let v4 = MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 3, mb3d = 4 }").replace("perf_tier = \"lite\"", "perf_tier = \"full\"");
    let r = validate(&bundle(&v4, wasm.clone(), &[]));
    assert_error(&r.errors, "mb3d_shader");
    assert_error(&r.errors, "mb2d_shader_use");
    let v5 = v4.replace("mb2d = 3, mb3d = 4", "mb2d = 4, mb3d = 5");
    let r = validate(&bundle(&v5, wasm, &[]));
    assert!(r.ok(), "{:#?}", r.errors);
    assert_eq!(r.formats(), ["mb2d3", "mb3d3", "mb3d4", "mb2d4", "mb3d5"]);
    let m = mb_format::Manifest::parse(&v5).unwrap();
    assert_eq!(mb_format::studio::variant(&m), mb_format::studio::Variant::ThreeDShaders);
    let m = mb_format::Manifest::parse(&MANIFEST.replace("stdlib = { mb2d = 1 }", "stdlib = { mb2d = 4 }")).unwrap();
    assert_eq!(mb_format::studio::variant(&m), mb_format::studio::Variant::PlainShaders);
    for f in ["mb2d4", "mb3d5"] {
        assert!(mb_format::studio::FORMATS.contains(&f), "apps claim {f} from the release that brings shaders");
    }
}

/// PNG, JPEG, Ogg and glb are stored, not deflated (Decided 40): they're
/// compressed already. Everything else is deflated, and the bundle is still
/// deterministic and valid.
#[test]
fn media_is_stored_and_the_rest_deflated() {
    use std::io::Cursor;
    let art = noise(4096, 7);
    let text = vec![b'a'; 4096];
    let bytes = bundle(
        MANIFEST,
        module(EXPORTS),
        &[("assets/a.png", &art), ("assets/b.JPG", &art), ("assets/c.ogg", &art), ("assets/d.jpeg", &art), ("assets/e.toml", &text)],
    );
    let r = validate(&bytes);
    assert!(r.ok(), "{:#?}", r.errors);
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
    for name in ["assets/a.png", "assets/b.JPG", "assets/c.ogg", "assets/d.jpeg", "icon.png"] {
        assert_eq!(zip.by_name(name).unwrap().compression(), zip::CompressionMethod::Stored, "{name}");
    }
    for name in ["assets/e.toml", "manifest.toml", "game.wasm", "src/src/lib.rs"] {
        assert_eq!(zip.by_name(name).unwrap().compression(), zip::CompressionMethod::Deflated, "{name}");
    }
    assert_eq!(mb_format::bundle::packed_size("assets/a.png", &art), 4096);
    assert!(mb_format::bundle::packed_size("assets/e.toml", &text) < 100);
}

/// A bundle checked as a map (a content-addressed preview) gets the same
/// errors as the same files packed, and what passed before can be skipped.
#[test]
fn files_check_as_a_map_as_they_do_packed() {
    let files = |extra: &[(&str, &[u8])]| {
        let mut f = BTreeMap::new();
        f.insert("manifest.toml".to_string(), MANIFEST.as_bytes().to_vec());
        f.insert("game.wasm".to_string(), module(&format!(r#"(import "mb" "nope" (func)) {EXPORTS}"#)));
        f.insert("icon.png".to_string(), icon(128, 128));
        f.insert("src/src/lib.rs".to_string(), b"// source".to_vec());
        for (p, d) in extra {
            f.insert(p.to_string(), d.to_vec());
        }
        f
    };
    let f = files(&[("assets/m.glb", b"not a glb")]);
    let packed = validate(&pack(&f).unwrap());
    let sizes: BTreeMap<String, u64> = f.iter().map(|(p, d)| (p.clone(), mb_format::bundle::packed_size(p, d))).collect();
    let mut r = mb_format::Report::default();
    mb_format::bundle::check_files(&mut r, &f, &sizes, &|_| false);
    assert_eq!(r.errors, packed.errors);
    assert!(r.errors.iter().any(|e| e.contains("assets/m.glb")) && r.errors.iter().any(|e| e.contains("icon.png is 128")));
    // Checked before: the glb and the wasm aren't looked at again; the rest still is.
    let mut r = mb_format::Report::default();
    mb_format::bundle::check_files(&mut r, &f, &sizes, &|p| p == "assets/m.glb" || p == "game.wasm");
    assert!(!r.errors.iter().any(|e| e.contains("assets/m.glb") || e.contains("nope")), "{:?}", r.errors);
    assert!(r.errors.iter().any(|e| e.contains("icon.png is 128")));
    // The per-path rules.
    let bad = files(&[("assets/x.exe", b""), ("assets/A.png", b""), ("assets/a.png", b"")]);
    let e = mb_format::bundle::check_paths(&bad);
    assert!(e.iter().any(|e| e.contains(".exe")) && e.iter().any(|e| e.contains("differ only by case")), "{e:?}");
}

/// A content-addressed preview's files (SPEC "Content-addressed previews"):
/// a fresh page opens them as a map, checked as a bundle is, named by the
/// list's digest; a file arriving on its own gets its own checks.
#[test]
fn content_addressed_files() {
    use mb_format::Bundle;
    use mb_format::bundle::{check_file, files_digest};
    let files = |extra: &[(&str, &[u8])]| {
        let mut f = BTreeMap::new();
        f.insert("manifest.toml".to_string(), MANIFEST.as_bytes().to_vec());
        f.insert("game.wasm".to_string(), module(EXPORTS));
        f.insert("icon.png".to_string(), icon(256, 256));
        f.insert("src/src/lib.rs".to_string(), b"// source".to_vec());
        for (p, d) in extra {
            f.insert(p.to_string(), d.to_vec());
        }
        f
    };
    let ok = files(&[("assets/a.txt", b"hello")]);
    let opened = Bundle::open_files(ok.clone(), None).expect("opens");
    let packed = Bundle::open(&pack(&ok).unwrap()).unwrap();
    assert_eq!(opened.runtime_wasm(), packed.runtime_wasm(), "metered as a bundle is");
    assert_eq!(opened.file("assets/a.txt"), Some(&b"hello"[..]));
    // Named as the server names the build: SHA-256 of "path\0sha\n" lines in path order.
    let lines: String = ok.iter().map(|(p, d)| format!("{p}\0{}\n", mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(d)))).collect();
    assert_eq!(files_digest(&ok), mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(lines.as_bytes())));
    assert_eq!(opened.sha256, files_digest(&ok));

    // Every rule a bundle's open has: paths, required files, the files' own checks.
    let Err(r) = Bundle::open_files(files(&[("assets/x.exe", b"MZ")]), None) else { panic!("opened") };
    assert_error(&r.errors, "asset type .exe is not allowed");
    let mut no_icon = ok.clone();
    no_icon.remove("icon.png");
    let Err(r) = Bundle::open_files(no_icon, None) else { panic!("opened") };
    assert_error(&r.errors, "missing required file icon.png");
    let Err(r) = Bundle::open_files(files(&[("assets/m.glb", b"glTF junk")]), None) else { panic!("opened") };
    assert_error(&r.errors, "assets/m.glb");
    let Err(r) = Bundle::open_files(files(&[("assets/A.txt", b"1"), ("assets/a.txt", b"2")]), None) else { panic!("opened") };
    assert_error(&r.errors, "differ only by case");

    // One file on its own.
    assert!(check_file("assets/game/main.lua", b"print(1)").is_ok());
    assert!(check_file("assets/game/main.luac", &[14, 0]).is_ok());
    assert!(check_file("assets/game/main.luac", &[0, b':']).unwrap_err().contains("bytecode version 0"));
    assert!(check_file("assets/game/main.luac", &[]).unwrap_err().contains("empty"));
    assert!(check_file("assets/m.glb", b"glTF junk").unwrap_err().contains("binary glTF"));
    assert!(check_file("assets/fonts/f.mbf", b"junk").is_err());
    assert!(check_file("assets/../x.png", b"").unwrap_err().contains("'..'"));
    assert!(check_file("/etc/passwd", b"").is_err());
    assert!(check_file("assets/x.exe", b"").unwrap_err().contains("not allowed"));
    assert!(check_file("game.wasm", b"\0asm").is_ok(), "checked with the manifest on a fresh page");
}
