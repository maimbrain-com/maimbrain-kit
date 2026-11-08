//! Studio engine 2.0 projects (docs/STUDIO-2.md): `engine = "2.0"` in
//! `studio.toml`, LÖVE-style Lua in `game/`, pictures of any size in
//! `images/`. The bundle is engine 2.0's wasm (games/studio-engine2) with
//! the project under `assets/`, and on the way in:
//! - each `game/*.lua` is compiled to Luau bytecode beside it
//!   (`game/main.luac`) with the engine's pinned Luau and options, since
//!   the engine has no compiler (Decided 36); a syntax error stops the
//!   build with its file and line;
//! - small pictures are packed into atlases (`atlas/<n>.png`, atlas.rs);
//! - the fonts the game names (`fonts = [...]` in studio.toml, the library
//!   fonts its Lua asks for by name, and `fonts/*.ttf`) are baked to
//!   `fonts/<id>.mbf` with their licenses;
//! - `packed.toml` records what the packer knows and the engine needs: each
//!   image's size and where it is (its own file, or an atlas and a place
//!   there), and each font's family and file;
//! - `.sfx.toml` sounds and `.song.toml` songs are rendered to Ogg beside
//!   them, as for 1.0;
//! - a 3D game whose Lua makes a `Terrain` gets the built-in ground
//!   surfaces, a photo and its normal map each (`terrain/<surface>.jpg`,
//!   `terrain/<surface>_n.jpg` for grass, meadow, dirt, rock and snow;
//!   tools/terrain/ground.py; docs/STUDIO-2.md Decided 70), which the
//!   engine's ground shader blends by the terrain's paint;
//! - `assets/files.txt` lists the project for the engine to load.


/// The SDK's identity words and art colours (the engine's, exactly).
#[path = "../../../../sdk/maimbrain/src/ui/pick.rs"]
#[allow(dead_code)]
mod pick;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use mb_format::Manifest;
use mb_format::manifest::{Capability, Fit, Input, Orientation, PerfTier, Remix, ReplayBanner, ScoreBoard, ScoreFormat, ScoreOrder, Sensor};
use mb_format::studio::Variant;
use toml::{Table, Value};

use super::{at, render_sfx, render_song, s};

/// The project's directories engine 2.0 reads, besides `studio.toml`.
pub const DIRS: [&str; 11] = ["game", "images", "tiles", "maps", "fonts", "sounds", "music", "models", "debug", "voices", "shaders"];
/// What the packer writes beside the project (the engine's `project::PACKED`).
pub const PACKED: &str = "packed.toml";

/// The built-in ground surfaces a `Terrain` blends, photo and normal map
/// each (engine 2.0's `surface_image` and `surface_normal`; 512² JPEGs,
/// about 0.8 MB in all).
pub const GROUND_SURFACES: [(&str, &[u8]); 10] = [
    ("terrain/grass.jpg", include_bytes!("../../assets/terrain/grass.jpg")),
    ("terrain/grass_n.jpg", include_bytes!("../../assets/terrain/grass_n.jpg")),
    ("terrain/meadow.jpg", include_bytes!("../../assets/terrain/meadow.jpg")),
    ("terrain/meadow_n.jpg", include_bytes!("../../assets/terrain/meadow_n.jpg")),
    ("terrain/dirt.jpg", include_bytes!("../../assets/terrain/dirt.jpg")),
    ("terrain/dirt_n.jpg", include_bytes!("../../assets/terrain/dirt_n.jpg")),
    ("terrain/rock.jpg", include_bytes!("../../assets/terrain/rock.jpg")),
    ("terrain/rock_n.jpg", include_bytes!("../../assets/terrain/rock_n.jpg")),
    ("terrain/snow.jpg", include_bytes!("../../assets/terrain/snow.jpg")),
    ("terrain/snow_n.jpg", include_bytes!("../../assets/terrain/snow_n.jpg")),
];

/// Whether the project has shaders (`shaders/*.wgsl`): it's packed with a shader build.
pub fn has_shaders(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("shaders")).into_iter().flatten().flatten().any(|e| is_shader_path(&format!("shaders/{}", e.file_name().to_string_lossy())))
}

/// The one rule for what's a shader, here, in `mb studio-check` and in the
/// engine (project.rs): `shaders/<name>.wgsl`, no subfolder, the extension
/// in lowercase. `bundle_files` refuses anything else under shaders/.
pub fn is_shader_path(rel: &str) -> bool {
    rel.strip_prefix("shaders/").and_then(|n| n.strip_suffix(".wgsl")).is_some_and(|n| !n.is_empty() && !n.contains('/') && !n.starts_with('.'))
}

/// A shader's kind: a sprite's (`fn sprite`) or a material's (`fn surface`).
pub fn shader_kind(src: &str) -> mb_shader::Kind {
    let words: Vec<&str> = src.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
    if words.windows(2).any(|w| w == ["fn", "sprite"]) && !words.windows(2).any(|w| w == ["fn", "surface"]) { mb_shader::Kind::Sprite } else { mb_shader::Kind::Surface }
}

/// `shaders/<name>.wgsl` through the checks the phone runs: what passed, or
/// the refusal as `shaders/<name>.wgsl:<line>: …`.
pub fn check_shader(rel: &str, bytes: &[u8]) -> Result<mb_shader::Checked, String> {
    let src = std::str::from_utf8(bytes).map_err(|_| at(rel, "not UTF-8 text"))?;
    mb_shader::check(src, shader_kind(src), &mb_shader::Limits::default()).map_err(|e| match e.line {
        Some(l) => format!("{rel}:{l}: {}", e.message),
        None => format!("{rel}: {}", e.message),
    })
}

/// Whether a game's Lua makes a `Terrain` (`Terrain{` or `Terrain(`, outside comments).
pub fn makes_terrain(lua: &str) -> bool {
    lua.lines().map(|l| l.split("--").next().unwrap_or("")).any(|l| {
        l.match_indices("Terrain").any(|(i, _)| {
            let before = l[..i].chars().next_back();
            let after = l[i + 7..].trim_start().chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':') && matches!(after, Some('{' | '('))
        })
    })
}

/// This mb's published engine 2.0 (crates/mb-cli/build.rs).
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/studio_engine2.rs"));
}

/// This mb's published engine 2.0 build for `v` (empty if it has none).
fn embedded_wasm(v: Variant) -> &'static [u8] {
    match v {
        Variant::Plain => embedded::PLAIN,
        Variant::Motion => embedded::MOTION,
        Variant::Mic => embedded::MIC,
        Variant::ThreeD => embedded::THREE_D,
        Variant::ThreeDMotion => embedded::THREE_D_MOTION,
        Variant::ThreeDMic => embedded::THREE_D_MIC,
        Variant::PlainShaders => embedded::PLAIN_SHADERS,
        Variant::ThreeDShaders => embedded::THREE_D_SHADERS,
        Variant::Tilt => &[], // engine 1.0's
    }
}

/// Which of this mb's published engine 2.0 builds `wasm` is, if it's one:
/// `mb difficulty` plays such a bundle natively with the engine built from
/// the same source (which has every sensor, all reading unavailable).
pub fn published_variant(wasm: &[u8]) -> Option<Variant> {
    VARIANTS.into_iter().find(|v| !embedded_wasm(*v).is_empty() && embedded_wasm(*v) == wasm)
}

/// The API's type definitions this mb has (name, source): engine 2.0's
/// `types/*.d.luau`, for `mb studio-check`. Empty without the engine.
pub fn types() -> &'static [(&'static str, &'static str)] {
    embedded::TYPES
}

/// Whether a 2.0 project is 3D (`kind = "3d"`): the 3D build plays it.
pub fn is_3d(t: &Table) -> Result<bool, String> {
    match s(t, "kind").unwrap_or("2d") {
        "2d" => Ok(false),
        "3d" => Ok(true),
        k => Err(format!("studio.toml: kind = \"{k}\" (\"2d\" or \"3d\")")),
    }
}

/// Whether studio.toml pins engine 2.0 or later.
pub fn is_v2(t: &Table) -> bool {
    match t.get("engine") {
        Some(Value::String(v)) => v.split('.').next().and_then(|m| m.parse::<u32>().ok()).is_some_and(|m| m >= mb_format::studio::LUAU_MAJOR),
        Some(Value::Float(f)) => *f >= mb_format::studio::LUAU_MAJOR as f64,
        Some(Value::Integer(i)) => *i >= mb_format::studio::LUAU_MAJOR as i64,
        _ => false,
    }
}

/// The engine a 2.0 project is packed with.
pub enum Engine {
    /// Built from its crate with cargo (`MB_STUDIO_ENGINE2`, engine development).
    Crate(PathBuf),
    /// The published builds inside this mb.
    Published,
}

pub fn engine() -> Result<Engine, String> {
    if let Some(d) = std::env::var_os("MB_STUDIO_ENGINE2") {
        let dir = PathBuf::from(d);
        if !dir.join("Cargo.toml").is_file() || !dir.join("src/lib.rs").is_file() {
            return Err(format!("{}: not Studio engine 2.0's crate (MB_STUDIO_ENGINE2)", dir.display()));
        }
        return Ok(Engine::Crate(dir));
    }
    if !embedded::PLAIN.is_empty() {
        return Ok(Engine::Published);
    }
    match env!("MB_STUDIO_ENGINE2_DIR") {
        "" => Err("this mb has no Studio engine 2.0: set MB_STUDIO_ENGINE2 to its crate (games/studio-engine2 in the Maimbrain repo)".into()),
        d => Ok(Engine::Crate(PathBuf::from(d))),
    }
}

impl Engine {
    pub fn version(&self) -> Result<String, String> {
        match self {
            Engine::Crate(dir) => super::engine_version(dir),
            Engine::Published => Ok(embedded::VERSION.to_string()),
        }
    }

    pub fn wasm(&self, variant: Variant) -> Result<Vec<u8>, String> {
        if !VARIANTS.contains(&variant) {
            return Err(format!("engine 2.0 has no {} build", variant.as_str()));
        }
        match self {
            Engine::Crate(dir) => build_engine(dir, variant),
            Engine::Published if !embedded_wasm(variant).is_empty() => Ok(embedded_wasm(variant).to_vec()),
            Engine::Published => Err(format!("this mb's published engine 2.0 has no {} build", variant.as_str())),
        }
    }

    pub fn source(&self, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
        match self {
            Engine::Crate(dir) => {
                files.insert("src/Cargo.toml".into(), std::fs::read(dir.join("Cargo.toml")).map_err(|e| e.to_string())?);
                files.insert("src/build.rs".into(), std::fs::read(dir.join("build.rs")).map_err(|e| e.to_string())?);
                crate::add_tree(&dir.join("src"), "src/src", mb_format::bundle::SOURCE_EXTENSIONS, files)
            }
            Engine::Published => {
                for (rel, bytes) in embedded::SOURCE {
                    let ext = rel.rsplit_once('.').map_or("", |(_, e)| e);
                    if mb_format::bundle::SOURCE_EXTENSIONS.contains(&ext) {
                        files.insert(format!("src/{rel}"), bytes.to_vec());
                    }
                }
                Ok(())
            }
        }
    }
}

/// Engine 2.0's builds: 2D and 3D (mb3d, the full tier), each with every
/// sensor set (docs/STUDIO-2.md §4.5: none, motion, mic), and with shaders
/// (mb2d 4 and mb3d 5, no sensors) for projects with shaders/*.wgsl.
pub const VARIANTS: [Variant; 8] = Variant::V2;

/// The opt-level of the published engine's Rust (Luau's C++ stays at 3).
/// "z" was 12% smaller again but doubled 3D physics' time natively.
pub const ENGINE_OPT: &str = "s";

/// The engine's wasm: built (release, LTO, wasm-opt with exception
/// handling) once and reused until its source changes. 2D and 3D have a
/// target directory each (the sensor sets share their kind's: only the
/// engine's own crate and the link differ).
pub fn build_engine(dir: &Path, variant: Variant) -> Result<Vec<u8>, String> {
    static BUILDING: Mutex<()> = Mutex::new(());
    let _g = BUILDING.lock().unwrap_or_else(|e| e.into_inner());
    let _lock = crate::devbuild::lock(dir);
    let target = crate::workspace_target_dir(dir)?.join(format!("studio-engine2-{}", if variant.is_3d() { "3d" } else { "plain" }));
    let tier = if variant.is_3d() { PerfTier::Full } else { PerfTier::Lite };
    let max_memory = tier.max_memory_pages() * 65536;
    eprintln!("building Studio engine 2.0 ({} build)", variant.as_str());
    let mut cargo = Command::new("cargo");
    cargo
        .args(["build", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["-p", "studio-engine2", "--target", "wasm32-unknown-unknown", "--release", "--target-dir"])
        .arg(&target)
        // The engine's Rust at opt-level "s", Luau's C++ kept at -O3
        // (Recast's build.rs keeps its own at -O3): 12% smaller (−0.46 MB
        // raw in 3D) for ~0–5% more frame time (docs/STUDIO-2.md §4.5,
        // §10.1 risk 2, Decided 110).
        .args(["--config", &format!("profile.release.opt-level=\"{ENGINE_OPT}\""), "--config", "profile.release.package.luau.opt-level=3"])
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("-C link-arg=--max-memory={max_memory}"));
    let mut features: Vec<&str> = Vec::new();
    if variant.is_3d() {
        features.push("3d");
    }
    // The sensor set's feature (each has the one before it).
    match variant.sensors().len() {
        0 => {}
        3 => features.push("motion"),
        _ => features.push("mic"),
    }
    if variant.shaders() {
        features.push("shaders");
    }
    if !features.is_empty() {
        cargo.args(["--features", &features.join(",")]);
    }
    let status = cargo.status().map_err(|e| format!("running cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed for Studio engine 2.0".into());
    }
    let raw = target.join("wasm32-unknown-unknown/release/studio_engine2.wasm");
    // Luau's C++ unwinds Lua errors with wasm exception handling (exnref).
    crate::optimize_wasm_with(&raw, &["--enable-exception-handling"]).and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())))
}

/// A Lua chunk compiled with the engine's options; a syntax error is
/// `game/main.lua:12: …`, as the engine's runtime errors are.
pub fn compile(rel: &str, src: &[u8]) -> Result<Vec<u8>, String> {
    luau::compile(src).map_err(|e| format!("{rel}:{}: {}", e.line, e.message))
}

/// A Studio 2.0 bundle's bytecode is the pinned compiler's output for the
/// source beside it (SPEC §6): every `assets/game/**/*.lua` compiled again,
/// with the engine's options, must be its `.luac` byte for byte. Luau's VM
/// doesn't check bytecode, so a hand-made chunk could read and write
/// anywhere in the engine's memory; this compare, which `mb validate` (and
/// so the server) runs, is what keeps one out. The app's validator has no
/// compiler and trusts bundles the server validated. Returns the problems.
pub fn check_bytecode(b: &mb_format::Bundle) -> Vec<String> {
    let mut errors = Vec::new();
    for path in b.paths().filter(|p| p.starts_with("assets/game/") && p.ends_with(".lua")) {
        let src = b.file(path).unwrap_or_default();
        let bc = format!("{path}c");
        match (luau::compile(src), b.file(&bc)) {
            (Err(e), _) => errors.push(format!("studio: {path}:{}: {} (it doesn't compile, so it has no bytecode)", e.line, e.message)),
            (Ok(_), None) => errors.push(format!("studio: {path} has no bytecode beside it ({bc})")),
            (Ok(want), Some(got)) if want != got => {
                errors.push(format!("studio: {bc} isn't the pinned Luau compiler's output for {path}: build Studio 2.0 projects with `mb build`"))
            }
            _ => {}
        }
    }
    errors
}

/// `studio.toml` as the manifest.
pub fn manifest(dir: &Path, t: &Table, startup: Vec<String>) -> Result<Manifest, String> {
    let title = s(t, "title").unwrap_or("Untitled").trim().to_string();
    let slug: String = title.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let id = s(t, "id").map(str::to_string).unwrap_or_else(|| format!("dev.studio.{}", if slug.is_empty() { "game".into() } else { slug }));
    let orientation = match s(t, "orientation").unwrap_or("portrait") {
        "portrait" => Orientation::Portrait,
        "landscape" => Orientation::Landscape,
        "sideways" => return Err("studio.toml: orientation = \"sideways\" is \"landscape\" in engine 2.0".into()),
        o => return Err(format!("studio.toml: orientation = \"{o}\" (portrait or landscape)")),
    };
    let three_d = is_3d(t)?;
    let logical_size = match t.get("size").and_then(Value::as_array) {
        Some(a) if a.len() == 2 => {
            let n = |v: &Value| v.as_integer().filter(|n| (1..=8192).contains(n)).map(|n| n as u32).ok_or("studio.toml: size is [w, h] in logical units");
            [n(&a[0])?, n(&a[1])?]
        }
        Some(_) => return Err("studio.toml: size is [w, h] in logical units".into()),
        None if orientation == Orientation::Portrait => [360, 640],
        None => [640, 360],
    };
    let mut inputs = Vec::new();
    match t.get("inputs").and_then(Value::as_array) {
        None => inputs = vec![Input::Touch, Input::Mouse, Input::Keyboard, Input::Gamepad],
        Some(list) => {
            for i in list.iter().filter_map(Value::as_str) {
                inputs.push(match i {
                    "touch" => Input::Touch,
                    "mouse" => Input::Mouse,
                    "keyboard" => Input::Keyboard,
                    "gamepad" => Input::Gamepad,
                    other => return Err(format!("studio.toml: inputs: {other:?} (touch, mouse, keyboard, gamepad)")),
                });
            }
        }
    }
    if !inputs.contains(&Input::Touch) {
        inputs.insert(0, Input::Touch); // every game plays by touch (games-touch-parity)
    }
    // The engine's own reading of [[scores]] (its boards.rs), so the
    // manifest's board numbers are the ones round.post and the daily mode use.
    let mut boards: Vec<ScoreBoard> = studio_engine2::boards::parse(t)?
        .into_iter()
        .map(|b| ScoreBoard { board: b.board, label: b.label, order: if b.lower { ScoreOrder::Lower } else { ScoreOrder::Higher }, format: ScoreFormat::Integer, daily: b.daily })
        .collect();
    // Board 0 even without a [[scores]] (round.score is always sent).
    if boards.is_empty() {
        boards.push(ScoreBoard { board: 0, label: "Score".into(), order: ScoreOrder::Higher, format: ScoreFormat::Integer, daily: false });
    }
    let version = engine()?.version()?;
    let pinned = match t.get("engine") {
        Some(Value::String(v)) => v.clone(),
        Some(Value::Float(f)) => format!("{f:.1}"),
        _ => return Err("studio.toml: engine = \"2.0\"".into()),
    };
    if pinned != version {
        return Err(format!("studio.toml: engine = \"{pinned}\", but this mb has Studio engine {version}"));
    }
    let _ = dir;
    // The shader builds have no sensors beyond haptics (§4.15): a game with
    // both waits for a build that has them.
    let sensor_set = mb_format::studio::Variant::v2(three_d, &reads_sensors(dir, t)?).sensors();
    if has_shaders(dir) && !sensor_set.is_empty() {
        return Err("shaders/ and sensors (tilt, motion, the microphone) can't be in one game yet: the shader builds have no sensors. Keep one or the other".into());
    }
    let remix = t.get("remix").and_then(Value::as_table);
    Ok(Manifest {
        abi: mb_format::manifest::ABI,
        id,
        name: title.chars().take(64).collect(),
        version: s(t, "version").unwrap_or("0.1.0").to_string(),
        creator: s(t, "creator").unwrap_or("@studio").to_string(),
        orientation,
        logical_size,
        fit: Fit::Letterbox,
        inputs,
        requires: Vec::new(),
        // A 3D game: the 3D build, mb3d 2 under the 2D (§4.5), and the full tier's memory and GPU.
        perf_tier: if three_d { PerfTier::Full } else { PerfTier::Lite },
        // Game fonts (mb2d 2) and the nearest filter (mb2d 3).
        // The 3D build batches its node transforms (mb3d 3, docs/STUDIO-2.md §4.3)
        // and draws terrains with ground surfaces (mb3d 4, Decided 70).
        // A project with shaders (shaders/*.wgsl) is packed with a shader build:
        // mb3d 5's materials and mb2d 4's sprite shaders (§4.15). Only those
        // games need the app that opens them.
        stdlib: match (three_d, has_shaders(dir)) {
            (true, false) => [("mb3d".to_string(), 4), ("mb2d".to_string(), 3)].into(),
            (true, true) => [("mb3d".to_string(), 5), ("mb2d".to_string(), 4)].into(),
            (false, false) => [("mb2d".to_string(), 3)].into(),
            (false, true) => [("mb2d".to_string(), 4)].into(),
        },
        // love.system.vibrate and haptic() are in every 2.0 build (§4.5); the
        // sensors the game reads pick the build, which declares its set.
        sensors: std::iter::once(Sensor::Haptics).chain(sensor_set.iter().copied()).collect(),
        needs: Vec::new(),
        // Progress (verified by the server's replay) only for a game that keeps it.
        capabilities: if uses_progress(dir) { vec![Capability::Store, Capability::Score, Capability::Progress] } else { vec![Capability::Store, Capability::Score] },
        startup_assets: startup,
        scores: boards,
        remix: Remix {
            allowed: remix.and_then(|r| r.get("allowed")).and_then(Value::as_bool).unwrap_or(true),
            parent: remix.and_then(|r| s(r, "parent")).unwrap_or("").to_string(),
        },
        replay_banner: ReplayBanner::Bottom,
        engine: Some(format!("studio@{version}")),
    })
}

/// The bundle's files for a 2.0 project (with `game.wasm` from `engine`).
pub fn bundle_files(dir: &Path, t: &Table, engine_wasm: Option<Vec<u8>>) -> Result<(Manifest, BTreeMap<String, Vec<u8>>), String> {
    let three_d = is_3d(t)?;
    let mut assets: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    assets.insert("studio.toml".into(), std::fs::read(dir.join("studio.toml")).map_err(|e| e.to_string())?);
    let mut rendered: Vec<(String, String)> = Vec::new();
    let mut sizes: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    // The Arcade preset (§4.9): every picture in the project's 16 colours.
    let arcade = t.get("preset").and_then(Value::as_str) == Some("arcade");
    let (pal, pal_name) = super::palette(t)?;
    let mut walkers: BTreeMap<String, ([u32; 4], [u32; 4])> = BTreeMap::new();
    // Pictures: decoded (for the atlases), and their files as they are.
    let mut pictures: Vec<(String, crate::generate::img::Image, bool)> = Vec::new();
    let mut whole: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut project_fonts: Vec<String> = Vec::new();
    let mut shaders: Vec<(String, mb_shader::Checked)> = Vec::new();
    for d in DIRS {
        let mut stack = vec![dir.join(d)];
        while let Some(p) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&p) else { continue };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                if d == "shaders" && !is_shader_path(&rel) {
                    return Err(at(&rel, "a shader is shaders/<name>.wgsl: no subfolders, and .wgsl in lowercase"));
                }
                let bytes = std::fs::read(&path).map_err(|e| at(&rel, e))?;
                let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
                match (d, ext.as_str()) {
                    (_, "json") if name.ends_with(".gen.json") => {} // provenance stays in the project
                    ("game", "lua") => {
                        let bc = compile(&rel, &bytes)?;
                        assets.insert(format!("{rel}c"), bc);
                        assets.insert(rel, bytes);
                    }
                    ("images", "png" | "jpg" | "jpeg") => {
                        let img = crate::generate::img::Image::decode(&bytes).map_err(|e| at(&rel, format!("not a PNG or JPEG this packer reads: {e}")))?;
                        let (w, h) = (img.w, img.h);
                        if arcade {
                            if ext != "png" {
                                return Err(at(&rel, "Arcade pictures are PNGs in the project's 16 colours (a JPEG can't keep to them). Without preset = \"arcade\" any picture works"));
                            }
                            super::palette_check(&img, &rel, &pal, &pal_name)?;
                        }
                        if w > 4096 || h > 4096 {
                            return Err(at(&rel, format!("{w}×{h} pixels; images are at most 4096×4096 (SPEC §5.3)")));
                        }
                        let stem = rel.strip_prefix("images/").unwrap().rsplit_once('.').unwrap().0.to_string();
                        if sizes.insert(stem.clone(), (w, h)).is_some() {
                            return Err(format!("images/{stem} has two pictures (.png and .jpg): keep one"));
                        }
                        let meta = std::fs::read_to_string(dir.join(format!("images/{stem}.toml"))).ok().and_then(|t| t.parse::<Table>().ok());
                        let pixel = meta.as_ref().is_some_and(|t| t.get("pixel").and_then(Value::as_bool) == Some(true));
                        // A walker's picture (`walker = true`): its body and foot measured, unless given.
                        if let Some(m) = meta.as_ref().filter(|m| m.get("walker").and_then(Value::as_bool) == Some(true) && !m.contains_key("body")) {
                            let cell = m.get("frame").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_integer()? as u32, a.get(1)?.as_integer()? as u32))).unwrap_or((w / 2, h));
                            walkers.insert(stem.clone(), walker_rects(&img, cell).ok_or_else(|| at(&rel, "a walker's picture is its body (the first cell) and a foot (the second), and one of them is empty"))?);
                        }
                        // JPEG paintings stay whole; PNGs may share an atlas.
                        if ext == "png" {
                            pictures.push((stem, img, pixel));
                        }
                        whole.insert(rel, bytes);
                    }
                    ("tiles", "png") => {
                        // A tile set's sheet: an image named tiles/<name>, pixel art unless its .toml says not.
                        let img = crate::generate::img::Image::decode(&bytes).map_err(|e| at(&rel, format!("not a PNG this packer reads: {e}")))?;
                        if arcade {
                            super::palette_check(&img, &rel, &pal, &pal_name)?;
                        }
                        let stem = rel.rsplit_once('.').unwrap().0.to_string();
                        let pixel = std::fs::read_to_string(dir.join(format!("{stem}.toml"))).ok().and_then(|t| t.parse::<Table>().ok()).is_none_or(|t| t.get("pixel").and_then(Value::as_bool) != Some(false));
                        sizes.insert(stem.clone(), (img.w, img.h));
                        pictures.push((stem, img, pixel));
                        whole.insert(rel, bytes);
                    }
                    ("images" | "tiles" | "debug", "toml") | ("sounds" | "music", "ogg") | ("maps", "map") => {
                        assets.insert(rel, bytes);
                    }
                    // Voices (§5 [voice]): the script, each line's sound and
                    // lip track. The cast is generate's (which voice speaks), and
                    // reference takes stay in the project.
                    // A bundle carries no .csv (SPEC's asset types): the script
                    // goes in as text, which the engine reads by either name.
                    ("voices", "csv") if rel == "voices/lines.csv" => {
                        assets.insert("voices/lines.txt".into(), bytes);
                    }
                    ("voices", "ogg" | "txt") if rel.matches('/').count() == 2 && !rel.starts_with("voices/ref/") => {
                        assets.insert(rel, bytes);
                    }
                    ("voices", "toml") if rel == "voices/cast.toml" => {}
                    ("models", "glb") if three_d => {
                        assets.insert(rel, bytes);
                    }
                    ("models", "glb") => return Err(at(&rel, "models are for 3D games: say kind = \"3d\" in studio.toml")),
                    ("shaders", "wgsl") => {
                        // The checks the phone runs (crate mb-shader): a shader that
                        // fails them is refused here, with its line, never shipped.
                        let c = check_shader(&rel, &bytes)?;
                        if c.kind == mb_shader::Kind::Surface && !three_d {
                            return Err(at(&rel, "fn surface is a 3D material's shader: a 2D game's are sprite shaders (fn sprite(in: Sprite) -> vec4<f32>)"));
                        }
                        shaders.push((rel.trim_start_matches("shaders/").trim_end_matches(".wgsl").to_string(), c));
                        assets.insert(rel, bytes);
                    }
                    ("fonts", "ttf" | "otf") => project_fonts.push(rel),
                    ("fonts", "txt") => {} // a font's license, which its bake carries
                    ("sounds", "toml") if name.ends_with(".sfx.toml") => {
                        let ogg = rel.trim_end_matches(".sfx.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, render_sfx(&bytes, &rel)?);
                        assets.insert(rel, bytes);
                    }
                    ("music", "json") if name.ends_with(".music.json") || name.ends_with(".songs.json") => {
                        music_json(dir, &rel, &bytes)?;
                        assets.insert(rel, bytes);
                    }
                    ("music", "toml") if name.ends_with(".song.toml") => {
                        let ogg = rel.trim_end_matches(".song.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, render_song(&bytes, &rel)?);
                        assets.insert(rel, bytes);
                    }
                    _ => eprintln!("warning: skipping {rel} (a Studio 2.0 project's {d}/ holds {})", kinds(d)),
                }
            }
        }
    }
    for (ogg, src) in &rendered {
        if dir.join(ogg).is_file() {
            return Err(format!("{src} and {ogg} are the same sound: keep one (the .ogg is what {src} becomes in the bundle)"));
        }
    }
    if three_d && assets.iter().any(|(p, b)| p.starts_with("game/") && p.ends_with(".lua") && makes_terrain(&String::from_utf8_lossy(b))) {
        for (path, bytes) in GROUND_SURFACES {
            assets.insert(path.into(), bytes.to_vec());
        }
    }
    if !assets.contains_key("game/main.lua") {
        return Err("game/main.lua is missing: a 2.0 game starts there (love.load, love.update, love.draw)".into());
    }
    let mut packed = String::from("# What `mb build` knows about the project's files, for the engine (generated).\n");
    let atlas = super::atlas::pack(&pictures);
    for (i, page) in atlas.pages.iter().enumerate() {
        assets.insert(format!("atlas/{i}.png"), page.encode_png());
    }
    for (name, (w, h)) in &sizes {
        packed.push_str(&format!("[images.{}]\nsize = [{w}, {h}]\n", toml_key(name)));
        if let Some((b, f)) = walkers.get(name) {
            packed.push_str(&format!("body = [{}, {}, {}, {}]\nfoot = [{}, {}, {}, {}]\n", b[0], b[1], b[2], b[3], f[0], f[1], f[2], f[3]));
        }
        match atlas.placed.get(name) {
            Some(p) => packed.push_str(&format!("file = \"atlas/{}.png\"\nat = [{}, {}]\n", p.page, p.at[0], p.at[1])),
            None => {
                let (rel, bytes) = whole.iter().find(|(r, _)| r.strip_prefix("images/").unwrap_or(r).rsplit_once('.').is_some_and(|(n, _)| n == name)).unwrap();
                packed.push_str(&format!("file = {rel:?}\n"));
                assets.insert(rel.clone(), bytes.clone());
            }
        }
    }
    // The identity (§4.12): the key art's colours for `colors = "art"`.
    if let Some(c) = art_colors(dir, t, &sizes)? {
        packed.push_str(&format!("[identity]\nbg = {}\npanel = {}\naccent = {}\nhighlight = {}\n", c.bg, c.panel, c.accent, c.highlight));
    }
    for (id, family, rel, mbf, license) in fonts(dir, t, &assets, &project_fonts)? {
        packed.push_str(&format!("[fonts.{}]\nfamily = {family:?}\nfile = {rel:?}\n", toml_key(&id)));
        assets.insert(rel.clone(), mbf);
        if let Some(l) = license {
            assets.insert(rel.replace(".mbf", "-LICENSE.txt"), l.into_bytes());
        }
    }
    // Each sound's length, so the engine can say whether a source is still
    // playing (and time a voice line that has no lip track).
    for (rel, bytes) in assets.iter().filter(|(p, _)| (p.starts_with("sounds/") || p.starts_with("music/") || p.starts_with("voices/")) && p.ends_with(".ogg")) {
        let secs = crate::generate::audio::decode(bytes, "audio/ogg").map(|(ch, rate)| ch.first().map_or(0, Vec::len) as f64 / rate.max(1) as f64).map_err(|e| at(rel, e))?;
        let (dir, name) = rel.trim_end_matches(".ogg").split_once('/').unwrap();
        packed.push_str(&format!("[{dir}.{}]\nsecs = {secs:.3}\n", toml_key(name)));
    }
    // Each shader's kind, cost and named uniforms (its struct Uniforms, laid out by the checks).
    for (name, c) in &shaders {
        let kind = if c.kind == mb_shader::Kind::Sprite { "sprite" } else { "surface" };
        packed.push_str(&format!("[shaders.{}]\nkind = \"{kind}\"\ncost = {}\n", toml_key(name), c.cost_fragment.max(c.cost_vertex)));
        if let Some(f) = &c.uniforms {
            let list: Vec<String> = f.iter().map(|f| format!("[{:?}, {}, {}]", f.name, f.offset, f.components)).collect();
            packed.push_str(&format!("uniforms = [{}]\n", list.join(", ")));
        }
    }
    assets.insert(PACKED.into(), packed.into_bytes());

    let list: Vec<String> = assets.keys().cloned().collect();
    let mut index = String::from("# The project's files, for the engine to load (written by mb build).\n");
    for p in &list {
        index.push_str(p);
        index.push('\n');
    }
    let mut files: BTreeMap<String, Vec<u8>> = assets.into_iter().map(|(k, v)| (format!("assets/{k}"), v)).collect();
    files.insert(mb_format::studio::INDEX.into(), index.into_bytes());
    // Music, voices and models load after the runtime starts, keeping the startup set small.
    let startup: Vec<String> = files.keys().filter(|k| k.starts_with("assets/") && !k.starts_with("assets/music/") && !(k.starts_with("assets/voices/") && k.ends_with(".ogg")) && !k.ends_with(".glb")).cloned().collect();
    let manifest = manifest(dir, t, startup)?;
    let source = engine()?;
    debug_assert_eq!(!shaders.is_empty(), mb_format::studio::variant(&manifest).shaders());
    let wasm = match engine_wasm {
        Some(w) => w,
        None => source.wasm(mb_format::studio::variant(&manifest))?,
    };
    files.insert("game.wasm".into(), wasm);
    files.insert("manifest.toml".into(), toml::to_string(&manifest).map_err(|e| e.to_string())?.into_bytes());
    let icon = match std::fs::read(dir.join("icon.png")) {
        Ok(b) => b,
        Err(_) => make_icon(dir, &sizes)?,
    };
    files.insert("icon.png".into(), icon);
    source.source(&mut files)?;
    Ok((manifest, files))
}

/// The most fonts a game has (SPEC §5.3).
const MAX_FONTS: usize = 16;

/// The fonts a project names, baked: (id, family, bundle path under
/// assets/, .mbf bytes, license). Library fonts come from `fonts = [...]`
/// in studio.toml and from the names its Lua passes to `newFont` or a
/// `font =` (a name that isn't a library font is left for the engine to
/// explain); a project's own `fonts/<name>.ttf` is baked under its name.
#[allow(clippy::type_complexity)]
fn fonts(dir: &Path, t: &Table, assets: &BTreeMap<String, Vec<u8>>, own: &[String]) -> Result<Vec<(String, String, String, Vec<u8>, Option<String>)>, String> {
    let mut wanted: Vec<String> = t.get("fonts").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    for (rel, src) in assets.iter().filter(|(p, _)| p.starts_with("game/") && p.ends_with(".lua")) {
        let _ = rel;
        wanted.extend(font_names_in(&String::from_utf8_lossy(src)));
    }
    // The identity's fonts: its own (as the theme names them, with weights),
    // and the families its overrides name.
    if let Some(Value::Table(i)) = t.get("identity") {
        for k in ["title_font", "body_font", "number_font"] {
            if let Some(f) = i.get(k).and_then(Value::as_str) {
                wanted.push(f.to_string());
            }
        }
    }
    let theme_fonts = identity_fonts(t);
    if wanted.is_empty() && own.is_empty() && theme_fonts.is_empty() {
        return Ok(Vec::new());
    }
    let lib = crate::fonts::library()?;
    let mut out: Vec<(String, String, String, Vec<u8>, Option<String>)> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for (name, id, weight) in &theme_fonts {
        let Some(e) = lib.fonts.iter().find(|e| e.id == *id) else { continue };
        let r = crate::fonts::Recipe { name: name.clone(), source: id.clone(), weight: *weight, chars: "latin1".into(), em: None, bitmap: None, license: None };
        let b = crate::fonts::bake_bytes(dir, &r, &lib)?;
        ids.push(name.clone());
        out.push((name.clone(), e.family.clone(), format!("fonts/{name}.mbf"), b.mbf, b.license_text));
    }
    for w in &wanted {
        let key = w.to_ascii_lowercase();
        let Some(e) = lib.fonts.iter().find(|e| e.id == key || e.family.eq_ignore_ascii_case(w) || e.id == key.replace(' ', "-")) else {
            if t.get("fonts").and_then(Value::as_array).is_some_and(|a| a.iter().any(|v| v.as_str() == Some(w))) {
                return Err(format!("studio.toml: fonts: {w:?} isn't in the font library (`mb fonts` lists it), and there's no fonts/{w}.ttf"));
            }
            continue;
        };
        if !ids.contains(&e.id) {
            ids.push(e.id.clone());
            let r = crate::fonts::Recipe { name: e.id.clone(), source: e.id.clone(), weight: None, chars: "latin1".into(), em: None, bitmap: None, license: None };
            let b = crate::fonts::bake_bytes(dir, &r, &lib)?;
            out.push((e.id.clone(), e.family.clone(), format!("fonts/{}.mbf", e.id), b.mbf, b.license_text));
        }
    }
    for rel in own {
        let stem = rel.strip_prefix("fonts/").unwrap().rsplit_once('.').unwrap().0;
        let id: String = stem.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
        let license = ["txt", "md"].iter().map(|x| format!("fonts/{stem}.{x}")).find(|l| dir.join(l).is_file()).or_else(|| ["OFL.txt", "LICENSE.txt"].iter().map(|l| format!("fonts/{l}")).find(|l| dir.join(l).is_file()));
        let r = crate::fonts::Recipe { name: id.clone(), source: rel.clone(), weight: None, chars: "latin1".into(), em: None, bitmap: None, license };
        let b = crate::fonts::bake_bytes(dir, &r, &lib)?;
        if b.license_text.is_none() {
            eprintln!("warning: {rel} has no license beside it (fonts/{stem}.txt): add the font's license before publishing");
        }
        out.push((id, stem.to_string(), format!("fonts/{}.mbf", r.name), b.mbf, b.license_text));
    }
    if out.len() > MAX_FONTS {
        return Err(format!("{} fonts; a game has at most {MAX_FONTS}", out.len()));
    }
    Ok(out)
}

/// Font names in Lua source: the first argument of `newFont("…"` and the
/// value of `font = "…"` (a `Text` node's), when it's a string literal.
pub fn font_names_in(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lit = |rest: &str| -> Option<String> {
        let rest = rest.trim_start();
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let body = &rest[1..];
        body.find(q).map(|e| body[..e].to_string())
    };
    for (i, _) in src.match_indices("newFont(") {
        if let Some(s) = lit(&src[i + 8..]) {
            out.push(s);
        }
    }
    for (i, _) in src.match_indices("font") {
        let rest = src[i + 4..].trim_start();
        let before = src[..i].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        if let Some(r) = rest.strip_prefix('=').filter(|r| !r.starts_with('='))
            && let Some(s) = lit(r)
        {
            out.push(s);
        }
    }
    out.retain(|s| !matches!(s.as_str(), "pixel" | "sans" | "bold" | "") && !s.contains('/'));
    out
}

/// A bare key, or a quoted one (`bg/dusk_0` → `"bg/dusk_0"`).
fn toml_key(k: &str) -> String {
    if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { k.to_string() } else { format!("{k:?}") }
}

/// A music graph (`.music.json`) or a setlist (`.songs.json`) the engine's
/// Director can play: it parses (the SDK's own reader), and every file it
/// names is in the project's `music/` (a setlist's graphs, and theirs).
fn music_json(dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    use maimbrain::music::{Graph, Setlist};
    let graphs: Vec<(String, Vec<u8>)> = if rel.ends_with(".songs.json") {
        let list = Setlist::parse(bytes).map_err(|e| at(rel, e))?;
        if list.songs.is_empty() {
            return Err(at(rel, "a setlist with no songs"));
        }
        let mut v = Vec::new();
        for (zone, g) in &list.songs {
            let b = std::fs::read(dir.join(g)).map_err(|_| at(rel, format!("{zone}'s graph {g} isn't in the project")))?;
            v.push((g.clone(), b));
        }
        v
    } else {
        vec![(rel.to_string(), bytes.to_vec())]
    };
    for (path, b) in graphs {
        let g = Graph::parse(&b).map_err(|e| at(&path, e))?;
        for f in g.files() {
            if !f.starts_with("music/") || !f.ends_with(".ogg") || !dir.join(f).is_file() {
                return Err(at(&path, format!("it plays {f}, which isn't a music/….ogg in the project")));
            }
        }
    }
    Ok(())
}

fn kinds(d: &str) -> &'static str {
    match d {
        "game" => ".lua",
        "models" => ".glb (a 3D game's)",
        "images" => ".png, .jpg and .toml",
        "sounds" => ".sfx.toml and .ogg",
        "music" => ".ogg, .song.toml, .music.json and .songs.json",
        "voices" => "lines.csv, cast.toml and each line's .ogg and .lips.txt (voices/<who>/<line>)",
        _ => "scenarios.toml",
    }
}

/// An icon when the project has none: its first picture (by name), fitted
/// into 256×256 over a dark grey.
fn make_icon(dir: &Path, sizes: &BTreeMap<String, (u32, u32)>) -> Result<Vec<u8>, String> {
    use crate::generate::img::Image;
    let mut icon = Image::new(256, 256, [0x2a, 0x2a, 0x2e, 255]);
    let first = sizes.keys().find_map(|n| ["png", "jpg", "jpeg"].iter().map(|x| dir.join(format!("images/{n}.{x}"))).find(|p| p.is_file()));
    if let Some(p) = first
        && let Ok(b) = std::fs::read(&p)
        && let Ok(img) = Image::decode(&b)
    {
        let k = (200.0 / img.w.max(img.h).max(1) as f32).min(16.0);
        let (w, h) = ((img.w as f32 * k) as u32, (img.h as f32 * k) as u32);
        let (ox, oy) = ((256 - w.min(256)) / 2, (256 - h.min(256)) / 2);
        for y in 0..h.min(256) {
            for x in 0..w.min(256) {
                let p = img.get(((x as f32 / k) as u32).min(img.w - 1), ((y as f32 / k) as u32).min(img.h - 1));
                if p[3] > 127 {
                    icon.set(ox + x, oy + y, p);
                }
            }
        }
    }
    Ok(icon.encode_png())
}

/// A walker sheet's body (the first `cell`) and foot (the second, beside
/// it or under it): each one's drawn pixels, as `[x, y, w, h]` in the picture.
fn walker_rects(img: &crate::generate::img::Image, (cw, ch): (u32, u32)) -> Option<([u32; 4], [u32; 4])> {
    let cols = (img.w / cw.max(1)).max(1);
    let rect = |i: u32| {
        let (x0, y0) = ((i % cols) * cw, (i / cols) * ch);
        if x0 + cw > img.w || y0 + ch > img.h {
            return None;
        }
        img.crop(x0, y0, cw, ch).content_bbox(128).map(|(x, y, w, h)| [x0 + x, y0 + y, w, h])
    };
    Some((rect(0)?, rect(1)?))
}

/// Whether the game's Lua writes progress (`progress.set`, `progress.checkpoint`).
/// The sensors a 2.0 game reads: studio.toml's `sensors` (haptics are
/// every build's), and tilt for a quick-path `player.tilt`.
pub fn reads_sensors(dir: &Path, t: &Table) -> Result<Vec<Sensor>, String> {
    let mut out = Vec::new();
    if let Some(v) = t.get("sensors") {
        let list = v.as_array().ok_or("studio.toml: sensors is a list, like [\"haptics\", \"tilt\"]")?;
        for s in list {
            out.push(match s.as_str() {
                Some("haptics") => continue,
                Some("tilt") => Sensor::Tilt,
                Some("motion") => Sensor::Motion,
                Some("attitude") => Sensor::Attitude,
                Some("loudness") => Sensor::Loudness,
                _ => return Err(format!("studio.toml: sensors can be haptics, tilt, motion, attitude and loudness, not {}", s.as_str().map_or_else(|| format!("a {}", s.type_str()), |x| format!("\"{x}\"")))),
            });
        }
    }
    if !out.contains(&Sensor::Tilt) && uses_text(dir, "player.tilt") {
        out.push(Sensor::Tilt);
    }
    Ok(out)
}

/// Whether any of the game's Lua, or an image's table, says `needle`.
fn uses_text(dir: &Path, needle: &str) -> bool {
    let mut stack = vec![dir.join("game"), dir.join("images")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "lua" || x == "toml") && std::fs::read_to_string(&p).is_ok_and(|s| s.contains(needle)) {
                return true;
            }
        }
    }
    false
}

fn uses_progress(dir: &Path) -> bool {
    let mut stack = vec![dir.join("game")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "lua") && std::fs::read_to_string(&p).is_ok_and(|s| s.contains("progress.")) {
                return true;
            }
        }
    }
    false
}

/// The library fonts the project's identity was designed with, as the
/// engine's `Theme::load_fonts` finds them (`assets/fonts/<name>.mbf`):
/// (file name, library id, weight). None for `greybox`, `arcade` and none.
fn identity_fonts(t: &Table) -> Vec<(String, String, Option<u32>)> {
    let (name, describe) = match t.get("identity") {
        Some(Value::String(s)) => (Some(s.clone()), None),
        Some(Value::Table(i)) => (i.get("name").and_then(Value::as_str).map(str::to_string), i.get("describe").and_then(Value::as_str).map(str::to_string)),
        _ => (None, None),
    };
    let picked = match (name, describe) {
        (Some(n), _) if pick::IDENTITIES.contains(&n.as_str()) => n,
        (Some(n), _) if n == "greybox" || n == "arcade" || n == "night" => return Vec::new(),
        (Some(d), _) | (None, Some(d)) => pick::pick_identity(&d).0.to_string(),
        (None, None) => return Vec::new(),
    };
    let Some((_, fonts)) = crate::fonts::identities().into_iter().find(|(n, _)| *n == picked) else { return Vec::new() };
    let mut out: Vec<(String, String, Option<u32>)> = Vec::new();
    for name in fonts {
        if name.is_empty() || out.iter().any(|o| o.0 == name) {
            continue;
        }
        let (id, w) = match name.rsplit_once('-') {
            Some((id, w)) if w.len() == 3 && w.chars().all(|c| c.is_ascii_digit()) => (id.to_string(), w.parse().ok()),
            _ => (name.clone(), None),
        };
        out.push((name, id, w));
    }
    out
}

/// `[identity] colors = "art"`: the key art's colours (`art = "name"`, else
/// the biggest picture), sampled as `Theme::from_art` does.
fn art_colors(dir: &Path, t: &Table, sizes: &BTreeMap<String, (u32, u32)>) -> Result<Option<pick::ArtColors>, String> {
    let Some(Value::Table(i)) = t.get("identity") else { return Ok(None) };
    if i.get("colors").and_then(Value::as_str) != Some("art") {
        return Ok(None);
    }
    let name = match i.get("art").and_then(Value::as_str) {
        Some(n) => {
            if !sizes.contains_key(n) {
                return Err(format!("studio.toml: identity art = {n:?}, but there's no images/{n}.png or .jpg"));
            }
            n.to_string()
        }
        None => match sizes.iter().filter(|(n, _)| !n.starts_with("tiles/")).max_by_key(|(n, (w, h))| (w * h, std::cmp::Reverse((*n).clone()))) {
            Some((n, _)) => n.clone(),
            None => return Err("studio.toml: identity colors = \"art\", but there are no images to sample (add the key art, or art = \"name\")".into()),
        },
    };
    let path = ["png", "jpg", "jpeg"].iter().map(|x| dir.join(format!("images/{name}.{x}"))).find(|p| p.is_file()).ok_or_else(|| format!("images/{name}: not found"))?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let img = crate::generate::img::Image::decode(&bytes).map_err(|e| format!("images/{name}: {e}"))?;
    let mut rgba = Vec::with_capacity((img.w * img.h * 4) as usize);
    for y in 0..img.h {
        for x in 0..img.w {
            rgba.extend(img.get(x, y));
        }
    }
    Ok(pick::art_colors(&rgba, img.w, img.h))
}

