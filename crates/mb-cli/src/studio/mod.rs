//! Studio projects (docs/STUDIO.md, docs/STUDIO-API.md): a directory with a
//! `studio.toml` and no Rust of its own. `mb build`, `mb serve [--watch]`,
//! `mb shot`, `mb measure` and `mb publish` take one like a game directory:
//! the bundle is the Studio engine's wasm (built from its crate,
//! `games/studio-engine`) with the project under `assets/` (SPEC "Studio
//! bundles"). On the way in:
//! - the manifest is generated from `studio.toml`;
//! - sprites and tiles are checked against the project's 16 colours and
//!   scaled up 5× (nearest), so the engine draws them crisp;
//! - `.sfx.toml` sounds and `.song.toml` songs are rendered to Ogg beside them;
//! - `assets/files.txt` lists the project for the engine to load.
//!
//! A project that says `engine = "2.0"` is engine 2.0's instead (Luau,
//! LÖVE-style Lua, any size of picture): `v2.rs` packs it.

pub mod asset;
pub mod asset2;
pub mod asset_music;
pub mod asset_voice;
pub mod atlas;
pub mod ca;
pub mod check;
pub mod difficulty;
pub mod play;
pub mod readable;
pub mod sense;
pub mod smoothness;
pub mod synth;
pub mod v2;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use mb_format::Manifest;
use mb_format::manifest::{Capability, Fit, Input, Orientation, PerfTier, Remix, ReplayBanner, ScoreBoard, ScoreFormat, ScoreOrder, Sensor};
use mb_format::studio::Variant;
use toml::{Table, Value};

use crate::generate::img::Image;

/// Texels per art pixel in a bundle (the engine's `TEXELS`).
const TEXELS: u32 = 5;
/// The project's directories the engine reads, besides `studio.toml`.
pub const DIRS: [&str; 8] = ["game", "sprites", "tiles", "maps", "sounds", "music", "models", "debug"];
/// A copy of the engine's palette presets (games/studio-engine/src/project.rs;
/// a test keeps them equal), for the palette check.
const PALETTES: &[(&str, [u32; 16])] = &[
    (
        "sunset16",
        [
            0x1b1428, 0x3b2346, 0x6e2f4f, 0xb03a48, 0xe2683c, 0xf3a953, 0xfde49c, 0xfff8ec, 0x8fd16a, 0x3e8e5e, 0x245a6b, 0x3c9fc9, 0x9fdcf0, 0x6c6f98,
            0xb7a6c9, 0x8a5a44,
        ],
    ),
    (
        "night16",
        [
            0x0b0d1f, 0x1d2244, 0x2e3a78, 0x4361c2, 0x5fb4ff, 0xa6f0ff, 0xf2f5ff, 0xff5ea8, 0xb43fa0, 0x6a2c7a, 0x2dd4a3, 0x17866b, 0xffd23f, 0xff8b3d,
            0x8a8fb8, 0x4a4e70,
        ],
    ),
];

/// A Studio project: `studio.toml`, and no `manifest.toml` (a game crate's).
pub fn is_project(dir: &Path) -> bool {
    dir.join("studio.toml").is_file() && !dir.join("manifest.toml").exists()
}

/// This mb's published Studio engine (crates/mb-cli/build.rs): its version,
/// its builds (plain, tilt, 3d) and its source; empty without one.
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/studio_engine.rs"));
}

/// The engine a project is packed with.
pub enum Engine {
    /// Built from its crate with cargo (engine development, or an mb without
    /// a published engine).
    Crate(PathBuf),
    /// The published builds inside this mb: no Rust toolchain needed.
    Published,
}

/// The engine to pack with: the crate `MB_STUDIO_ENGINE` names (built with
/// cargo), else the published engine inside this mb, else the crate in the
/// checkout this mb was built from.
pub fn engine() -> Result<Engine, String> {
    if let Some(d) = std::env::var_os("MB_STUDIO_ENGINE") {
        let dir = PathBuf::from(d);
        if !dir.join("Cargo.toml").is_file() || !dir.join("src/lib.rs").is_file() {
            return Err(format!("{}: not the Studio engine's crate (MB_STUDIO_ENGINE)", dir.display()));
        }
        return Ok(Engine::Crate(dir));
    }
    if !embedded::PLAIN.is_empty() {
        return Ok(Engine::Published);
    }
    match env!("MB_STUDIO_ENGINE_DIR") {
        "" => Err("this mb has no Studio engine: set MB_STUDIO_ENGINE to its crate (games/studio-engine in the Maimbrain repo)".into()),
        d => Ok(Engine::Crate(PathBuf::from(d))),
    }
}

impl Engine {
    pub fn version(&self) -> Result<String, String> {
        match self {
            Engine::Crate(dir) => engine_version(dir),
            Engine::Published => Ok(embedded::VERSION.to_string()),
        }
    }

    /// `game.wasm` for this import set.
    pub fn wasm(&self, variant: Variant) -> Result<Vec<u8>, String> {
        if !Variant::ALL.contains(&variant) {
            return Err(format!("Studio engine 1.0 has no {} build (sensors beyond tilt are engine 2.0's)", variant.as_str()));
        }
        match (self, variant) {
            (Engine::Crate(dir), _) => build_engine(dir, variant),
            (Engine::Published, Variant::Plain) => Ok(embedded::PLAIN.to_vec()),
            (Engine::Published, Variant::Tilt) if !embedded::TILT.is_empty() => Ok(embedded::TILT.to_vec()),
            (Engine::Published, Variant::Tilt) => Err("this mb's published Studio engine has no tilt build".into()),
            (Engine::Published, Variant::ThreeD) if !embedded::THREE_D.is_empty() => Ok(embedded::THREE_D.to_vec()),
            (Engine::Published, Variant::ThreeD) => Err("this mb's published Studio engine has no 3D build".into()),
            (Engine::Published, v) => Err(format!("Studio engine 1.0 has no {} build", v.as_str())),
        }
    }

    /// The engine's source, as a bundle's `src/` holds it.
    pub fn source(&self, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
        match self {
            Engine::Crate(dir) => {
                files.insert("src/Cargo.toml".into(), std::fs::read(dir.join("Cargo.toml")).map_err(|e| e.to_string())?);
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

/// `mb studio-engines`: each engine version's builds this mb packs with.
pub fn engines_json() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    let builds = |e: Result<Engine, String>| -> Option<(String, Vec<&'static str>)> {
        // Only published builds: a crate's would be built with cargo to find out.
        let e = e.ok().filter(|e| matches!(e, Engine::Published))?;
        let v = e.version().ok()?;
        Some((v, Variant::ALL.into_iter().filter(|x| e.wasm(*x).is_ok_and(|w| !w.is_empty())).map(Variant::as_str).collect()))
    };
    if let Some((v, b)) = builds(engine()) {
        out.insert(v, b.into());
    }
    let two = v2::engine().ok().and_then(|e| {
        let v = e.version().ok()?;
        Some((v, v2::VARIANTS.into_iter().filter(|x| matches!(e, v2::Engine::Published) && e.wasm(*x).is_ok()).map(Variant::as_str).collect::<Vec<_>>()))
    });
    if let Some((v, b)) = two {
        out.insert(v, b.into());
    }
    serde_json::Value::Object(out)
}

/// The engine's version (`ENGINE_VERSION` in its lib.rs).
pub fn engine_version(dir: &Path) -> Result<String, String> {
    let src = std::fs::read_to_string(dir.join("src/lib.rs")).map_err(|e| e.to_string())?;
    src.lines()
        .find_map(|l| l.trim().strip_prefix("pub const ENGINE_VERSION: &str = \"")?.strip_suffix("\";").map(str::to_string))
        .ok_or_else(|| format!("{}: no ENGINE_VERSION", dir.join("src/lib.rs").display()))
}

/// A fingerprint of what the engine's wasm is built from: its Cargo.toml
/// and `src/` (but its tests). `published/SOURCE` keeps the one its
/// published builds were made from; a test says when they're stale.
pub fn source_fingerprint(dir: &Path) -> Result<String, String> {
    use sha2::Digest;
    let mut files = BTreeMap::new();
    files.insert("Cargo.toml".to_string(), std::fs::read(dir.join("Cargo.toml")).map_err(|e| e.to_string())?);
    if let Ok(b) = std::fs::read(dir.join("build.rs")) {
        files.insert("build.rs".to_string(), b);
    }
    crate::add_tree(&dir.join("src"), "src", &["rs", "lua", "toml"], &mut files)?;
    // Test-only files: changing them doesn't change the engine.
    files.remove("src/tests.rs");
    files.remove("src/three_tests.rs");
    files.remove("src/api_reference.rs");
    let mut h = sha2::Sha256::new();
    for (k, v) in &files {
        h.update(k.as_bytes());
        h.update([0]);
        h.update((v.len() as u64).to_le_bytes());
        h.update(v);
    }
    Ok(mb_format::bundle::hex(&h.finalize()))
}

/// Engine 2.0's fingerprint: [`source_fingerprint`] of its crate, and what
/// else its wasm is built from: `crates/luau` (Luau's pinned source, the
/// patches and compile options in build.rs, the shim in csrc/) and the SDK
/// it compiles (`sdk/maimbrain`: the scene, bump, the UI and the rest). A
/// sandbox or SDK change makes the published builds stale until the engine
/// is published again. (1.0's stays its crate's alone: its builds never
/// change, and the SDK has moved on since.)
pub fn source_fingerprint2(dir: &Path) -> Result<String, String> {
    use sha2::Digest;
    let root = dir.join("../..");
    let mut files = BTreeMap::new();
    files.insert("engine".to_string(), source_fingerprint(dir)?.into_bytes());
    // How it's built: the Rust's opt-level (v2.rs).
    files.insert("opt-level".to_string(), v2::ENGINE_OPT.as_bytes().to_vec());
    for (rel, exts) in [("crates/luau", &["rs", "cpp", "h", "toml"][..]), ("crates/recast", &["rs", "cpp", "h", "toml"][..]), ("sdk/maimbrain", &["rs", "toml"][..])] {
        let d = root.join(rel);
        files.insert(format!("{rel}/Cargo.toml"), std::fs::read(d.join("Cargo.toml")).map_err(|e| format!("{}: {e}", d.join("Cargo.toml").display()))?);
        if let Ok(b) = std::fs::read(d.join("build.rs")) {
            files.insert(format!("{rel}/build.rs"), b);
        }
        crate::add_tree(&d.join("src"), &format!("{rel}/src"), exts, &mut files)?;
        if d.join("csrc").is_dir() {
            crate::add_tree(&d.join("csrc"), &format!("{rel}/csrc"), exts, &mut files)?;
        }
    }
    // Tests don't change the engine.
    files.retain(|k, _| !k.ends_with("tests.rs"));
    let mut h = sha2::Sha256::new();
    for (k, v) in &files {
        h.update(k.as_bytes());
        h.update([0]);
        h.update((v.len() as u64).to_le_bytes());
        h.update(v);
    }
    Ok(mb_format::bundle::hex(&h.finalize()))
}

/// `just studio-engine`: builds every engine build from the crate and writes
/// them to its `published/`, with the source fingerprint; prints their
/// SHA-256 for `mb_format::studio::PUBLISHED`.
pub fn publish_engine() -> Result<(), String> {
    if let Some(d) = std::env::var_os("MB_STUDIO_ENGINE2") {
        return publish_engine2(&PathBuf::from(d));
    }
    let Engine::Crate(dir) = engine()? else {
        return Err("set MB_STUDIO_ENGINE to the engine's crate (games/studio-engine)".into());
    };
    let version = engine_version(&dir)?;
    let out = dir.join("published");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // An older version's builds go (their SHA-256 stay in PUBLISHED).
    for e in std::fs::read_dir(&out).map_err(|e| e.to_string())?.flatten() {
        if e.file_name().to_string_lossy().ends_with(".wasm") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    for v in [Variant::Plain, Variant::Tilt, Variant::ThreeD] {
        let wasm = build_engine(&dir, v)?;
        let name = format!("studio-{version}-{}.wasm", v.as_str());
        std::fs::write(out.join(&name), &wasm).map_err(|e| e.to_string())?;
        let sha = mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(&wasm));
        println!("Published {{ version: \"{version}\", variant: Variant::{v:?}, sha256: \"{sha}\" }}, // {name}, {} bytes", wasm.len());
    }
    std::fs::write(out.join("SOURCE"), format!("{}\n", source_fingerprint(&dir)?)).map_err(|e| e.to_string())?;
    eprintln!("wrote {} (rebuild mb to embed them)", out.display());
    Ok(())
}

/// `just studio-engine2`: engine 2.0's builds, as [`publish_engine`] does 1.0's.
fn publish_engine2(dir: &Path) -> Result<(), String> {
    let version = engine_version(dir)?;
    let out = dir.join("published");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(&out).map_err(|e| e.to_string())?.flatten() {
        if e.file_name().to_string_lossy().ends_with(".wasm") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    for v in v2::VARIANTS {
        let wasm = v2::build_engine(dir, v)?;
        let name = format!("studio-{version}-{}.wasm", v.as_str());
        std::fs::write(out.join(&name), &wasm).map_err(|e| e.to_string())?;
        let sha = mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(&wasm));
        println!("Published {{ version: \"{version}\", variant: Variant::{v:?}, sha256: \"{sha}\" }}, // {name}, {} bytes", wasm.len());
    }
    std::fs::write(out.join("SOURCE"), format!("{}\n", source_fingerprint2(dir)?)).map_err(|e| e.to_string())?;
    eprintln!("wrote {} (rebuild mb to embed them)", out.display());
    Ok(())
}

fn read_toml(path: &Path) -> Result<Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.parse::<Table>().map_err(|e| format!("{}: {}", path.display(), e.message()))
}

fn s<'a>(t: &'a Table, k: &str) -> Option<&'a str> {
    t.get(k).and_then(Value::as_str)
}

/// The project's palette, 0xRRGGBB.
pub(super) fn palette(t: &Table) -> Result<([u32; 16], String), String> {
    match t.get("palette") {
        None => Ok((PALETTES[0].1, PALETTES[0].0.into())),
        Some(Value::String(n)) => PALETTES.iter().find(|p| p.0 == n).map(|p| (p.1, n.clone())).ok_or_else(|| format!("studio.toml: no palette called {n:?}")),
        Some(Value::Array(a)) if a.len() == 16 => {
            let mut out = [0; 16];
            for (i, c) in a.iter().enumerate() {
                out[i] = c.as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()).ok_or_else(|| format!("studio.toml: palette colour {i} isn't \"#rrggbb\""))?;
            }
            Ok((out, "its own".into()))
        }
        Some(_) => Err("studio.toml: palette is a preset name or 16 \"#rrggbb\" colours".into()),
    }
}

/// Whether the project reads tilt: `inputs` says so, or a sprite is `player.tilt`.
fn uses_tilt(dir: &Path, t: &Table) -> bool {
    let listed = t.get("inputs").and_then(Value::as_array).is_some_and(|a| a.iter().any(|i| i.as_str() == Some("tilt")));
    listed
        || std::fs::read_dir(dir.join("sprites")).into_iter().flatten().flatten().any(|e| {
            e.path().extension().is_some_and(|x| x == "toml") && std::fs::read_to_string(e.path()).is_ok_and(|s| s.contains("player.tilt"))
        })
}

/// Whether studio.toml says it's a 3D game (`kind = "3d"`); 2D otherwise.
/// Without `kind`, the older `rung` and `view` that projects made before it
/// still say (`rung = "lua"` with `view = "3d"` is 3D). The engine's
/// project.rs (`Kind::read`) reads the same keys, with the same errors.
pub fn is_3d(t: &Table) -> Result<bool, String> {
    match (s(t, "kind"), s(t, "rung"), s(t, "view")) {
        (Some("2d"), ..) => Ok(false),
        (Some("3d"), ..) => Ok(true),
        (Some(k), ..) => Err(format!("studio.toml: kind = \"{k}\" (\"2d\" or \"3d\")")),
        (None, None | Some("arcade"), None | Some("2d")) => Ok(false),
        (None, Some("lua"), Some("3d")) => Ok(true),
        (None, ..) => Err("studio.toml: say kind = \"2d\" or kind = \"3d\" (rung and view were the old way to say it)".into()),
    }
}

/// `studio.toml` as the manifest (`startup` is every asset the engine loads).
pub fn manifest(dir: &Path, startup: Vec<String>) -> Result<Manifest, String> {
    let t = read_toml(&dir.join("studio.toml"))?;
    if v2::is_v2(&t) {
        return v2::manifest(dir, &t, startup);
    }
    let title = s(&t, "title").unwrap_or("Untitled").trim().to_string();
    let slug: String = title.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let id = s(&t, "id").map(str::to_string).unwrap_or_else(|| format!("dev.studio.{}", if slug.is_empty() { "game".into() } else { slug }));
    let orientation = match s(&t, "orientation").unwrap_or("portrait") {
        "portrait" => Orientation::Portrait,
        "sideways" | "landscape" => Orientation::Landscape,
        o => return Err(format!("studio.toml: orientation = \"{o}\" (portrait or sideways)")),
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
                    "tilt" => continue,
                    other => return Err(format!("studio.toml: inputs: {other:?} (touch, mouse, keyboard, gamepad, tilt)")),
                });
            }
        }
    }
    if !inputs.contains(&Input::Touch) {
        inputs.insert(0, Input::Touch); // every behaviour plays by touch (games-touch-parity)
    }
    let board = t.get("scores").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_table).cloned().unwrap_or_default();
    let label = s(&board, "label").unwrap_or("Score").to_string();
    let order = match s(&board, "order").unwrap_or("high") {
        "high" | "higher" => ScoreOrder::Higher,
        "low" | "lower" => ScoreOrder::Lower,
        o => return Err(format!("studio.toml: scores order = \"{o}\" (high or low)")),
    };
    if t.get("scores").and_then(Value::as_array).is_some_and(|a| a.len() > 1) {
        return Err("studio.toml: an Arcade game has one [[scores]] board (the global `score`)".into());
    }
    let remix = t.get("remix").and_then(Value::as_table);
    let version = engine()?.version()?;
    let pinned = match t.get("engine") {
        Some(Value::String(v)) => v.clone(),
        Some(Value::Float(f)) => format!("{f:.1}"),
        None => version.clone(),
        Some(_) => return Err("studio.toml: engine = \"1.0\"".into()),
    };
    if pinned != version {
        return Err(format!("studio.toml: engine = \"{pinned}\", but this mb has Studio engine {version}"));
    }
    let three_d = is_3d(&t)?;
    if three_d && uses_tilt(dir, &t) {
        return Err("studio.toml: tilt isn't in the 3D view yet (inputs = [\"tilt\"])".into());
    }
    Ok(Manifest {
        abi: mb_format::manifest::ABI,
        id,
        name: title.chars().take(64).collect(),
        version: s(&t, "version").unwrap_or("0.1.0").to_string(),
        creator: s(&t, "creator").unwrap_or("@studio").to_string(),
        orientation,
        logical_size: if orientation == Orientation::Portrait { [360, 640] } else { [640, 360] },
        fit: Fit::Letterbox,
        inputs,
        requires: Vec::new(),
        // The 3D build: mb3d 2 under an mb2d HUD, haptics, and the full tier's memory and GPU.
        perf_tier: if three_d { PerfTier::Full } else { PerfTier::Lite },
        stdlib: if three_d { [("mb3d".to_string(), 2), ("mb2d".to_string(), 1)].into() } else { [("mb2d".to_string(), 1)].into() },
        sensors: if three_d {
            vec![Sensor::Haptics]
        } else if uses_tilt(dir, &t) {
            vec![Sensor::Tilt]
        } else {
            Vec::new()
        },
        needs: Vec::new(),
        capabilities: vec![Capability::Store, Capability::Score],
        startup_assets: startup,
        scores: vec![ScoreBoard { board: 0, label: label.chars().take(32).collect(), order, format: ScoreFormat::Integer, daily: false }],
        remix: Remix {
            allowed: remix.and_then(|r| r.get("allowed")).and_then(Value::as_bool).unwrap_or(true),
            parent: remix.and_then(|r| s(r, "parent")).unwrap_or("").to_string(),
        },
        replay_banner: ReplayBanner::Bottom,
        engine: Some(format!("studio@{version}")),
    })
}

/// The engine's wasm for `variant`: built (release, LTO, wasm-opt) once and
/// reused until its source changes. Each variant has a target directory of
/// its own, so switching between them doesn't rebuild.
fn build_engine(dir: &Path, variant: Variant) -> Result<Vec<u8>, String> {
    // One build at a time in this process (`mb serve --watch` and a build).
    static BUILDING: Mutex<()> = Mutex::new(());
    let _g = BUILDING.lock().unwrap_or_else(|e| e.into_inner());
    let _lock = crate::devbuild::lock(dir);
    let target = crate::workspace_target_dir(dir)?.join(format!("studio-engine-{}", variant.as_str()));
    let tier = if variant == Variant::ThreeD { PerfTier::Full } else { PerfTier::Lite };
    let max_memory = tier.max_memory_pages() * 65536;
    let mut cargo = Command::new("cargo");
    cargo
        .args(["build", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["-p", "studio-engine", "--target", "wasm32-unknown-unknown", "--release", "--target-dir"])
        .arg(&target)
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("-C link-arg=--max-memory={max_memory}"));
    match variant {
        Variant::Plain => {}
        Variant::Tilt => {
            cargo.args(["--features", "sensors"]);
        }
        Variant::ThreeD => {
            cargo.args(["--features", "3d"]);
        }
        v => return Err(format!("Studio engine 1.0 has no {} build", v.as_str())),
    }
    eprintln!("building the Studio engine ({} build)", variant.as_str());
    let status = cargo.status().map_err(|e| format!("running cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed for the Studio engine".into());
    }
    let raw = target.join("wasm32-unknown-unknown/release/studio_engine.wasm");
    crate::optimize_wasm(&raw).and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())))
}

thread_local! {
    /// Where rendered sounds and songs are kept while a build runs, if
    /// anywhere ([`with_render_cache`]): `mb build --ca` keeps them in its store.
    static RENDER_CACHE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with rendered sounds and songs kept in (and read from) `dir`,
/// by their source's SHA-256 and this mb's version.
pub fn with_render_cache<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    RENDER_CACHE.with(|c| *c.borrow_mut() = Some(dir.to_path_buf()));
    let out = f();
    RENDER_CACHE.with(|c| *c.borrow_mut() = None);
    out
}

/// Drops rendered files nobody read for a week (a song that changed, a
/// sound that's gone), so the cache stays a project's size.
pub fn prune_render_cache(dir: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7 * 86_400);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < old) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// An Ogg rendered from `src` by `render`, from the render cache when it's
/// there (its time touched, so pruning keeps it).
fn cached_render(kind: &str, src: &[u8], render: impl FnOnce() -> Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
    let Some(dir) = RENDER_CACHE.with(|c| c.borrow().clone()) else { return render() };
    let key = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(kind.as_bytes());
        h.update([0]);
        h.update(env!("CARGO_PKG_VERSION").as_bytes());
        h.update([0]);
        h.update(src);
        mb_format::bundle::hex(&h.finalize())
    };
    let file = dir.join(format!("{key}.ogg"));
    if let Ok(b) = std::fs::read(&file) {
        let _ = std::fs::File::options().append(true).open(&file).and_then(|f| f.set_modified(std::time::SystemTime::now()));
        return Ok(b);
    }
    let ogg = render()?;
    let tmp = dir.join(format!(".{key}.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, &ogg).and_then(|()| std::fs::rename(&tmp, &file)).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(ogg)
}

/// A `.sfx.toml` rendered to Ogg, as a bundle carries it.
pub(super) fn render_sfx(src: &[u8], rel: &str) -> Result<Vec<u8>, String> {
    cached_render("sfx", src, || {
        let x = synth::sfx(&String::from_utf8_lossy(src), rel)?;
        let x = crate::generate::audio::normalize(&x, crate::generate::audio::Kind::Sfx, synth::RATE);
        crate::generate::audio::ogg_bytes(&x, synth::RATE, 4.0)
    })
}

/// A `.song.toml` rendered to one seamless Ogg loop.
pub(super) fn render_song(src: &[u8], rel: &str) -> Result<Vec<u8>, String> {
    cached_render("song", src, || {
        let x = synth::song(&String::from_utf8_lossy(src), rel)?;
        let g = crate::generate::audio::gain_for(&x, crate::generate::audio::Kind::Music);
        let x = crate::generate::audio::limit_loop(&x, g, synth::RATE);
        crate::generate::audio::ogg_bytes(&x, synth::RATE, 4.0)
    })
}

/// An error in one of the project's files.
fn at(path: &str, e: impl std::fmt::Display) -> String {
    format!("{path}: {e}")
}

/// Every pixel solid or clear, in the project's 16 colours (Arcade art).
pub(super) fn palette_check(img: &Image, rel: &str, pal: &[u32; 16], pal_name: &str) -> Result<(), String> {
    let mut bad = 0;
    let mut first = None;
    for y in 0..img.h {
        for x in 0..img.w {
            let [r, g, b, a] = img.get(x, y);
            if a == 0 {
                continue;
            }
            let rgb = (r as u32) << 16 | (g as u32) << 8 | b as u32;
            if a != 255 || !pal.contains(&rgb) {
                bad += 1;
                first.get_or_insert((x, y, rgb, a));
            }
        }
    }
    if let Some((x, y, rgb, a)) = first {
        let what = if a != 255 { format!("half see-through (alpha {a})") } else { format!("#{rgb:06x}") };
        return Err(at(rel, format!("{bad} pixels aren't the project's colours (palette {pal_name}), the first at ({x}, {y}): {what}. Arcade art uses its 16 colours, each pixel solid or clear")));
    }
    Ok(())
}

/// A sprite or tile sheet checked against the palette and scaled up.
fn art(bytes: &[u8], rel: &str, pal: &[u32; 16], pal_name: &str, tile: bool) -> Result<Vec<u8>, String> {
    let img = Image::decode(bytes).map_err(|e| at(rel, e))?;
    let sizes = if tile { "16" } else { "16 or 32" };
    if !(img.h == 16 || (!tile && img.h == 32)) {
        return Err(at(rel, format!("{}×{} pixels; Arcade art is {sizes} pixels high, its frames side by side", img.w, img.h)));
    }
    if img.w % img.h != 0 || img.w / img.h > 8 && !tile {
        return Err(at(rel, format!("{}×{} pixels: frames are {h}×{h}, side by side, at most 8", img.w, img.h, h = img.h)));
    }
    palette_check(&img, rel, pal, pal_name)?;
    let mut big = Image::new(img.w * TEXELS, img.h * TEXELS, [0, 0, 0, 0]);
    for y in 0..big.h {
        for x in 0..big.w {
            big.set(x, y, img.get(x / TEXELS, y / TEXELS));
        }
    }
    Ok(big.encode_png())
}

/// An icon when the project has none: the hero, big, on the background colour.
fn make_icon(dir: &Path, t: &Table, pal: &[u32; 16]) -> Result<Vec<u8>, String> {
    let bg = t.get("background").and_then(Value::as_integer).unwrap_or(0).clamp(0, 15) as usize;
    let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
    let mut icon = Image::new(256, 256, rgb(pal[bg]));
    let hero = s(t, "hero").map(str::to_string).or_else(|| {
        let mut names: Vec<String> = std::fs::read_dir(dir.join("sprites")).into_iter().flatten().flatten().filter_map(|e| e.path().file_stem()?.to_str().map(str::to_string)).collect();
        names.sort();
        names.dedup();
        names.into_iter().find(|n| dir.join(format!("sprites/{n}.png")).is_file())
    });
    if let Some(h) = hero
        && let Ok(b) = std::fs::read(dir.join(format!("sprites/{h}.png")))
        && let Ok(img) = Image::decode(&b)
    {
        let k = 192 / img.h.max(1);
        let (ox, oy) = ((256 - img.h * k) / 2, (256 - img.h * k) / 2);
        for y in 0..img.h {
            for x in 0..img.h.min(img.w) {
                let p = img.get(x, y);
                if p[3] == 0 {
                    continue;
                }
                for yy in 0..k {
                    for xx in 0..k {
                        icon.set(ox + x * k + xx, oy + y * k + yy, p);
                    }
                }
            }
        }
    }
    Ok(icon.encode_png())
}

/// The bundle's files for a project (with `game.wasm` from `engine`).
pub fn bundle_files(dir: &Path, engine: Option<Vec<u8>>) -> Result<(Manifest, BTreeMap<String, Vec<u8>>), String> {
    let t = read_toml(&dir.join("studio.toml"))?;
    if v2::is_v2(&t) {
        return v2::bundle_files(dir, &t, engine);
    }
    let (pal, pal_name) = palette(&t)?;
    let three_d = is_3d(&t)?;
    let mut assets: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    assets.insert("studio.toml".into(), std::fs::read(dir.join("studio.toml")).map_err(|e| e.to_string())?);
    let mut rendered: Vec<(String, String)> = Vec::new();
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
                let bytes = std::fs::read(&path).map_err(|e| at(&rel, e))?;
                let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
                match (d, ext.as_str()) {
                    (_, "json") if name.ends_with(".gen.json") => {} // provenance stays in the project
                    ("game", "lua") | ("maps", "map") | ("sprites" | "tiles" | "debug", "toml") | ("sounds" | "music", "ogg") => {
                        assets.insert(rel, bytes);
                    }
                    ("models", "glb") if three_d => {
                        assets.insert(rel, bytes);
                    }
                    // The 3D view has no palette or grid for pictures.
                    ("sprites" | "tiles", "png") if three_d => {
                        assets.insert(rel, bytes);
                    }
                    ("sprites" | "tiles", "png") => {
                        assets.insert(rel.clone(), art(&bytes, &rel, &pal, &pal_name, d == "tiles")?);
                    }
                    ("sounds", "toml") if name.ends_with(".sfx.toml") => {
                        let ogg = rel.trim_end_matches(".sfx.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, render_sfx(&bytes, &rel)?);
                        assets.insert(rel, bytes);
                    }
                    ("music", "toml") if name.ends_with(".song.toml") => {
                        let ogg = rel.trim_end_matches(".song.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, render_song(&bytes, &rel)?);
                        assets.insert(rel, bytes);
                    }
                    _ => eprintln!("warning: skipping {rel} (a Studio project's {d}/ holds {})", kinds(d)),
                }
            }
        }
    }
    for (ogg, src) in &rendered {
        if dir.join(ogg).is_file() {
            return Err(format!("{src} and {ogg} are the same sound: keep one (the .ogg is what {src} becomes in the bundle)"));
        }
    }
    let list: Vec<String> = assets.keys().cloned().collect();
    let mut index = String::from("# The project's files, for the engine to load (written by mb build).\n");
    for p in &list {
        index.push_str(p);
        index.push('\n');
    }
    let mut files: BTreeMap<String, Vec<u8>> = assets.into_iter().map(|(k, v)| (format!("assets/{k}"), v)).collect();
    files.insert(mb_format::studio::INDEX.into(), index.into_bytes());
    // Everything is loaded before the game starts, but models and music (the
    // big files) load after the runtime starts, keeping the startup set small.
    let big = |k: &str| three_d && (k.ends_with(".glb") || k.starts_with("assets/music/"));
    let startup: Vec<String> = files.keys().filter(|k| k.starts_with("assets/") && !big(k)).cloned().collect();
    let manifest = manifest(dir, startup)?;
    let source = self::engine()?;
    let wasm = match engine {
        Some(w) => w,
        None => source.wasm(mb_format::studio::variant(&manifest))?,
    };
    files.insert("game.wasm".into(), wasm);
    files.insert("manifest.toml".into(), toml::to_string(&manifest).map_err(|e| e.to_string())?.into_bytes());
    let icon = match std::fs::read(dir.join("icon.png")) {
        Ok(b) => b,
        Err(_) => make_icon(dir, &t, &pal)?,
    };
    files.insert("icon.png".into(), icon);
    // SPEC "Studio bundles": the source is the engine at its version, plus the project.
    source.source(&mut files)?;
    Ok((manifest, files))
}

fn kinds(d: &str) -> &'static str {
    match d {
        "game" => ".lua",
        "sprites" | "tiles" => ".png and .toml",
        "maps" => ".map",
        "sounds" => ".sfx.toml and .ogg",
        "music" => ".song.toml and .ogg",
        "models" => ".glb (the 3D view's)",
        _ => "scenarios.toml",
    }
}

/// `mb build` for a project: pack the bundle and validate it. With
/// `MB_RENDER_CACHE` set (the Studio server's whole builds), rendered sounds
/// and songs are kept there, as `mb build --ca` keeps them in its store.
pub fn build_and_pack(dir: &Path, out: &Path) -> Result<PathBuf, String> {
    let (manifest, files) = match std::env::var_os("MB_RENDER_CACHE").filter(|d| !d.is_empty()) {
        Some(d) => {
            let d = PathBuf::from(d);
            std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
            with_render_cache(&d, || bundle_files(dir, None))?
        }
        None => bundle_files(dir, None)?,
    };
    let bytes = mb_format::pack(&files).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(format!("{}-{}.mbx", manifest.id, manifest.version));
    std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let report = mb_format::validate(&bytes);
    crate::print_report(&path, &report);
    if report.ok() { Ok(path) } else { Err(format!("{} failed validation", path.display())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starters() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine/starters")
    }

    /// The engine's finished games (Cat Snack, Comet Lanes…), which these tests play.
    fn examples() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine/examples")
    }

    /// The engine is in the monorepo, not in the creator kit (whose tests run these too).
    fn have_engine() -> bool {
        let here = starters().is_dir();
        if !here {
            eprintln!("skipped: no games/studio-engine here (the creator kit)");
        }
        here
    }

    /// mb's palette check and the engine use the same presets.
    #[test]
    fn palettes_match_the_engines() {
        if !have_engine() {
            return;
        }
        let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine/src/project.rs")).unwrap();
        for (name, colours) in PALETTES {
            let at = src.find(&format!("\"{name}\",")).unwrap_or_else(|| panic!("the engine has no {name}"));
            let block: String = src[at..].chars().take_while(|c| *c != ']').collect();
            let theirs: Vec<u32> = block.split(|c: char| !c.is_ascii_alphanumeric()).filter_map(|w| w.strip_prefix("0x")).map(|h| u32::from_str_radix(h, 16).unwrap()).collect();
            assert_eq!(theirs, colours, "{name}");
        }
    }

    #[test]
    fn a_project_becomes_a_bundles_files() {
        if !have_engine() {
            return;
        }
        let dir = examples().join("catch");
        let (m, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        assert_eq!((m.engine.as_deref(), m.logical_size, m.id.as_str()), (Some("studio@1.0"), [360, 640], "dev.maimbrain.studio.catsnack"));
        assert!(m.sensors.is_empty());
        let list = String::from_utf8(files["assets/files.txt"].clone()).unwrap();
        for p in ["studio.toml", "game/main.lua", "sprites/cat.png", "sprites/cat.toml", "maps/level1.map", "sounds/chomp.ogg", "sounds/chomp.sfx.toml", "music/theme.ogg"] {
            assert!(list.lines().any(|l| l == p), "{p} in {list}");
            assert!(files.contains_key(&format!("assets/{p}")), "{p}");
        }
        // Sprites are scaled up 5×.
        let cat = Image::decode(&files["assets/sprites/cat.png"]).unwrap();
        assert_eq!((cat.w, cat.h), (160, 80));
        assert!(files.contains_key("src/src/prelude.lua") && files.contains_key("src/src/lib.rs"));
        assert!(m.startup_assets.iter().all(|a| files.contains_key(a)));
    }

    /// The published builds were made from the engine's source as it is: after
    /// changing the engine, `just studio-engine` (and the printed PUBLISHED lines).
    #[test]
    fn the_published_engine_is_built_from_this_source() {
        if !have_engine() {
            return;
        }
        let dir = starters().parent().unwrap().to_path_buf();
        let published = std::fs::read_to_string(dir.join("published/SOURCE")).unwrap_or_default();
        assert_eq!(
            published.trim(),
            source_fingerprint(&dir).unwrap(),
            "games/studio-engine changed since its builds were published: run `just studio-engine` and paste its lines into mb_format::studio::PUBLISHED"
        );
    }

    /// This mb packs with its published engine, which the validator knows.
    #[test]
    fn projects_pack_with_the_published_engine() {
        if embedded::PLAIN.is_empty() {
            eprintln!("skipped: this mb has no published Studio engine");
            return;
        }
        let sha = |w: &[u8]| mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(w));
        for (variant, wasm) in [(Variant::Plain, embedded::PLAIN), (Variant::Tilt, embedded::TILT), (Variant::ThreeD, embedded::THREE_D)] {
            assert!(
                mb_format::studio::PUBLISHED.iter().any(|p| p.version == embedded::VERSION && p.variant == variant && p.sha256 == sha(wasm)),
                "the {} build inside mb isn't in mb_format::studio::PUBLISHED (run `just studio-engine` and paste its lines)",
                variant.as_str()
            );
        }
        if !have_engine() || std::env::var_os("MB_STUDIO_ENGINE").is_some() {
            return;
        }
        let (_, files) = bundle_files(&examples().join("catch"), None).unwrap();
        let r = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.wasm_size > 1024 * 1024, "the engine is over 1 MiB, so the next line tests something");
        assert!(!r.warnings.iter().any(|w| w.contains("SHOULD be under 1 MiB")), "no size warning for the published engine: {:?}", r.warnings);
        assert!(r.studio.unwrap().published, "packed with the published engine");
        assert!(r.startup_size >= 400 * 1024, "engine 1.0 counts in the startup set, as every app before 2.0 counts it ({} bytes)", r.startup_size);
        assert!(files.contains_key("src/src/prelude.lua"), "the engine's source, embedded too");

        // A 3D game: its own build, the full tier, mb3d and haptics.
        let (m, files) = bundle_files(&examples().join("comet"), None).unwrap();
        assert_eq!((m.perf_tier, m.sensors.clone()), (PerfTier::Full, vec![Sensor::Haptics]));
        assert_eq!(m.stdlib.get("mb3d"), Some(&2));
        let r = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(r.ok(), "{:?}", r.errors);
        let info = r.studio.unwrap();
        assert_eq!(info.variant, Variant::ThreeD);
        assert!(info.published, "packed with the published 3D build");
        assert!(files.contains_key("src/src/prelude3d.lua") && files.contains_key("src/src/scene.rs"));

        // Every starter (the greybox a new game begins as) packs and validates.
        for dir in std::fs::read_dir(starters()).unwrap().flatten().map(|e| e.path()).filter(|d| d.join("studio.toml").is_file()) {
            let (_, files) = bundle_files(&dir, None).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            let r = mb_format::validate(&mb_format::pack(&files).unwrap());
            assert!(r.ok(), "{}: {:?}", dir.display(), r.errors);
        }
    }

    fn starters2() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/starters")
    }

    /// Engine 2.0's projects: Lua compiled beside its source, what the
    /// packer knows in packed.toml, format studio2.
    #[test]
    fn a_2_0_project_becomes_a_bundles_files() {
        if !have_engine() {
            return;
        }
        let (m, files) = bundle_files(&starters2().join("catch"), Some(b"\0asm".to_vec())).unwrap();
        assert_eq!((m.engine.as_deref(), m.logical_size, m.id.as_str()), (Some("studio@2.0"), [360, 640], "dev.maimbrain.studio2.catch"));
        assert_eq!(m.needs_formats(), ["studio2", "mb2d3"]);
        let list = String::from_utf8(files["assets/files.txt"].clone()).unwrap();
        for p in ["studio.toml", "game/main.lua", "game/main.luac", "atlas/0.png", "images/player.toml", "packed.toml"] {
            assert!(list.lines().any(|l| l == p), "{p} in {list}");
            assert!(files.contains_key(&format!("assets/{p}")), "{p}");
        }
        assert_eq!(files["assets/game/main.luac"][0], luau::BYTECODE_VERSION_TARGET);
        let packed: Table = String::from_utf8(files["assets/packed.toml"].clone()).unwrap().parse().unwrap();
        let player = &packed["images"]["player"];
        assert_eq!(player["size"].as_array().unwrap().iter().map(|v| v.as_integer().unwrap()).collect::<Vec<_>>(), [128, 40]);
        // Its small pictures share an atlas (no palette, no scaling); the originals stay out.
        assert_eq!(player["file"].as_str(), Some("atlas/0.png"));
        assert!(player["at"].as_array().is_some() && !files.contains_key("assets/images/player.png"));
        assert!(files.contains_key("src/src/prelude.lua") && files.contains_key("src/build.rs"), "engine 2.0's source");
    }

    /// A 2.0 3D project (`kind = "3d"`) packs with the 3D build: mb3d
    /// under mb2d 3, the full tier, haptics; its models ship and stream
    /// after the start, and a 2D project's model is an error.
    #[test]
    fn a_2_0_3d_project_packs_with_the_3d_build() {
        if !have_engine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio2-3d-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        assert!(std::process::Command::new("cp").arg("-R").arg(starters2().join("tunnel/game")).arg(dir.join("game")).status().unwrap().success());
        std::fs::copy(starters2().join("tunnel/studio.toml"), dir.join("studio.toml")).unwrap();
        std::fs::write(dir.join("models/fox.glb"), b"glTF").unwrap();
        let (m, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        assert_eq!((m.perf_tier, m.sensors.clone()), (PerfTier::Full, vec![Sensor::Haptics]));
        assert_eq!((m.stdlib.get("mb3d"), m.stdlib.get("mb2d")), (Some(&4), Some(&3)));
        assert_eq!(m.needs_formats(), ["studio2", "mb2d3", "mb3d3", "mb3d4"]);
        assert_eq!(mb_format::studio::variant(&m), Variant::ThreeD);
        assert!(files.contains_key("assets/models/fox.glb") && !m.startup_assets.iter().any(|a| a.ends_with(".glb")));
        std::fs::write(dir.join("studio.toml"), std::fs::read_to_string(dir.join("studio.toml")).unwrap().replace("kind = \"3d\"", "kind = \"2d\"")).unwrap();
        let e = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap_err();
        assert!(e.contains("models/fox.glb: models are for 3D games"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Voices (§5 [voice]): the script ships as text (a bundle has no .csv),
    /// each line's sound and lip track ship, the sounds stream after the
    /// start with their lengths in packed.toml, and the cast stays behind.
    #[test]
    fn voices_are_packed() {
        if !have_engine() {
            return;
        }
        let toll = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/examples/the-toll");
        let (m, files) = bundle_files(&toll, Some(b"\0asm".to_vec())).unwrap();
        for p in ["voices/lines.txt", "voices/gruk/halt.ogg", "voices/gruk/halt.lips.txt", "voices/pip/please.ogg", "images/portraits/gruk.toml"] {
            assert!(files.contains_key(&format!("assets/{p}")), "{p}");
        }
        assert!(!files.keys().any(|k| k.ends_with(".csv") || k.ends_with(".gen.json")));
        assert!(m.startup_assets.iter().any(|a| a == "assets/voices/gruk/halt.lips.txt"), "lip tracks are in the startup set");
        assert!(!m.startup_assets.iter().any(|a| a.starts_with("assets/voices/") && a.ends_with(".ogg")), "voices stream");
        let packed: Table = String::from_utf8(files["assets/packed.toml"].clone()).unwrap().parse().unwrap();
        let secs = packed["voices"]["gruk/halt"]["secs"].as_float().unwrap();
        assert!((secs - 2.82).abs() < 0.05, "{secs}");
        let r = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(r.errors.iter().all(|e| e.contains("game.wasm") || e.contains("wasm")), "{:?}", r.errors);
    }

    /// studio.toml's boards are the manifest's, numbered as the engine
    /// numbers them for round.post, with the daily one marked; a game whose
    /// Lua keeps progress declares it (Tally, Phase 4c's test game).
    #[test]
    fn a_2_0_projects_boards_and_progress_are_its_manifests() {
        if !have_engine() {
            return;
        }
        let tally = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/fixtures/tally");
        let (m, files) = bundle_files(&tally, Some(b"\0asm".to_vec())).unwrap();
        let boards: Vec<(u32, &str, bool)> = m.scores.iter().map(|b| (b.board, b.label.as_str(), b.daily)).collect();
        assert_eq!(boards, [(0, "Height", false), (1, "Taps", false), (2, "Today", true)]);
        assert!(m.capabilities.contains(&mb_format::manifest::Capability::Progress));
        let r = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(r.errors.iter().all(|e| !e.contains("scores")), "{:?}", r.errors);
        // A starter's one board stays one, and it keeps no progress.
        let (m, _) = bundle_files(&starters2().join("catch"), Some(b"\0asm".to_vec())).unwrap();
        assert_eq!(m.scores.len(), 1);
        assert!(!m.capabilities.contains(&mb_format::manifest::Capability::Progress));
    }

    /// A tile set's sheet is packed as an image (pixel art), its map ships
    /// as it is, and a walker picture's body and foot are measured.
    #[test]
    fn tiles_maps_and_walkers_are_packed() {
        if !have_engine() {
            return;
        }
        let (_, files) = bundle_files(&starters2().join("runner"), Some(b"\0asm".to_vec())).unwrap();
        let packed: Table = String::from_utf8(files["assets/packed.toml"].clone()).unwrap().parse().unwrap();
        assert_eq!(packed["images"]["tiles/ground"]["size"].as_array().unwrap().len(), 2);
        assert!(files.contains_key("assets/maps/level1.map") && files.contains_key("assets/tiles/ground.toml"));
        // The font its Lua names (newFont("Rye")), beside its identity's (saloon's).
        assert!(packed["fonts"].get("rye").is_some(), "{packed:?}");

        let dir = std::env::temp_dir().join(format!("mb-studio2-walker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::create_dir_all(dir.join("images")).unwrap();
        std::fs::copy(starters2().join("catch/studio.toml"), dir.join("studio.toml")).unwrap();
        std::fs::write(dir.join("game/main.lua"), "Walker{ image = 'fox' }\n").unwrap();
        // A body (cell 1) and a small foot (cell 2), side by side.
        let mut img = crate::generate::img::Image::new(64, 32, [0, 0, 0, 0]);
        for (x0, y0, w, h) in [(4, 2, 20, 26), (36, 20, 8, 6)] {
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    img.set(x, y, [200, 100, 50, 255]);
                }
            }
        }
        std::fs::write(dir.join("images/fox.png"), img.encode_png()).unwrap();
        std::fs::write(dir.join("images/fox.toml"), "walker = true\n").unwrap();
        let (_, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let packed = String::from_utf8(files["assets/packed.toml"].clone()).unwrap();
        assert!(packed.contains("body = [4, 2, 20, 26]\nfoot = [36, 20, 8, 6]"), "{packed}");
    }

    /// Every identity's number font, baked as `mb build` bakes it: the digits'
    /// ink, read from the atlas, reaches no higher above the baseline than
    /// the line the HUD budgets for a score (the SDK's `ui::layouts::ink_cap`,
    /// its capitals' middle plus half of max(cap, 2 × ascent − cap): so the
    /// ink stays under max(cap, ascent)). The HUD's score keeps its ink by
    /// that budget under the screen's top and the safe area; a font whose
    /// digits overran it would clip.
    #[test]
    fn every_identitys_digits_stay_inside_the_huds_budget() {
        if !have_engine() {
            return;
        }
        let lib = crate::fonts::library().unwrap();
        let dir = std::env::temp_dir().join(format!("mb-studio2-digits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut report = Vec::new();
        for (identity, [_, _, number]) in crate::fonts::identities() {
            let (id, weight) = match number.rsplit_once('-') {
                Some((id, w)) if w.len() == 3 && w.chars().all(|c| c.is_ascii_digit()) => (id.to_string(), w.parse().ok()),
                _ => (number.clone(), None),
            };
            let r = crate::fonts::Recipe { name: number.clone(), source: id, weight, chars: "latin1".into(), em: None, bitmap: None, license: None };
            let baked = crate::fonts::bake_bytes(&dir, &r, &lib).unwrap();
            let f = mb_format::font::MbFont::parse(&baked.mbf).unwrap();
            let atlas = crate::generate::img::Image::decode(&f.png).unwrap();
            // Ink: a bitmap's set texels; a distance field's inside the edge (≥ half).
            let inked = |x: u32, y: u32| atlas.get(x, y)[0] >= 128;
            let mut top = f32::MIN; // ink's highest point above the baseline, in ems
            for d in '0'..='9' {
                let g = f.glyphs.iter().find(|g| g.codepoint == d as u32).unwrap_or_else(|| panic!("{identity}: {number} has no {d}"));
                let [x, y, w, h] = g.rect.map(u32::from);
                let Some(row) = (0..h).find(|r| (0..w).any(|c| inked(x + c, y + r))) else { continue };
                top = top.max(-(g.offset[1] + row as f32) / f.em_px);
            }
            let (cap, ascent) = (f.cap_height / f.em_px, f.ascent / f.em_px);
            report.push(format!("{identity} {number}: digits reach {top:.3} em, cap {cap:.3}, ascent {ascent:.3}"));
            assert!(top <= cap.max(ascent) + 0.01, "{identity}: {number}'s digits reach {top:.3} em above the baseline, past max(cap {cap:.3}, ascent {ascent:.3})");
        }
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("{}", report.join("\n"));
    }

    /// The identity's fonts are baked, and `colors = "art"` samples the key art.
    #[test]
    fn identities_bake_their_fonts_and_sample_the_art() {
        if !have_engine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio2-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::create_dir_all(dir.join("images")).unwrap();
        // Its own studio.toml: the starters have identities of their own.
        let toml = "title = \"T\"\nengine = \"2.0\"\nkind = \"2d\"\nid = \"dev.maimbrain.studio2.identity\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\n\n[identity]\nname = \"swiss\"\ncolors = \"art\"\n";
        std::fs::write(dir.join("studio.toml"), toml).unwrap();
        std::fs::write(dir.join("game/main.lua"), "function love.load() end\n").unwrap();
        let mut img = crate::generate::img::Image::new(64, 64, [20, 28, 70, 255]);
        for y in 40..48 {
            for x in 0..64 {
                img.set(x, y, [255, 150, 40, 255]);
            }
        }
        std::fs::write(dir.join("images/sky.png"), img.encode_png()).unwrap();
        let (_, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let packed: Table = String::from_utf8(files["assets/packed.toml"].clone()).unwrap().parse().unwrap();
        assert!(files.contains_key("assets/fonts/archivo-800.mbf") && files.contains_key("assets/fonts/archivo.mbf"), "{:?}", files.keys().collect::<Vec<_>>());
        assert_eq!(packed["identity"]["bg"].as_integer(), Some(0x141c46ff));
        assert_eq!(packed["identity"]["accent"].as_integer(), Some(0xff9628ff));
    }

    /// Lua the compiler folds, every kind: arithmetic, `%` and `//`, the
    /// exact builtins it folds and the libm ones it mustn't.
    const FOLDS: &str = "local t = {}\nt[1] = 0.1 + 0.2\nt[2] = 1 / 3\nt[3] = 7.5 % 2.3\nt[4] = -7.5 % 2.3\nt[5] = 10 // 3\nt[6] = -10.5 // 4\nt[7] = 2 ^ 0.5\nt[8] = 3 * 0.1 + 0.7\nt[9] = math.sqrt(2)\nt[10] = math.floor(2.5) + math.ceil(-2.5)\nt[11] = math.abs(-3.25)\nt[12] = math.max(1, 2.5, -4)\nt[13] = math.min(1, 2.5, -4)\nt[14] = math.fmod(7.5, 2.3)\nt[15] = math.rad(30)\nt[16] = math.deg(1)\nt[17] = math.sin(1) + math.cos(2) + math.tan(0.5)\nt[18] = math.exp(1) + math.log(10) + math.log(8, 2) + math.log10(7)\nt[19] = math.atan2(1, 2) + math.atan(0.3) + math.asin(0.2) + math.acos(0.4)\nt[20] = math.sinh(1) + math.cosh(1) + math.tanh(0.5) + math.pow(1.1, 7.3)\nt[21] = math.pi * 2 / 3\nt[22] = math.ldexp(0.75, 5)\nreturn t\n";

    /// The server recompiles a creator's Lua and compares it byte for byte
    /// (`mb validate`), so the compiler gives the same bytes on every host:
    /// this hash is the same on macOS, Linux and Windows (the kit's tests run
    /// it on all three).
    #[test]
    fn bytecode_is_the_same_on_every_host() {
        let bc = luau::compile(FOLDS.as_bytes()).unwrap();
        let hash = mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(&bc));
        assert_eq!(hash, "769aeb711f4f88149565677543be4c292d5a6013597bad95bc0a32a358341772");
    }

    #[test]
    fn the_fonts_lua_names() {
        let src = "local f = love.graphics.newFont(\"Fraunces\", 24)\nlocal g = love.graphics.newFont(30)\nText{ text = 'x', font = 'Press Start 2P' }\nlocal t = { font = \"pixel\" }\nif a.font == 'x' then end\nnewFont('fonts/mine.ttf', 12)";
        assert_eq!(v2::font_names_in(src), ["Fraunces", "Press Start 2P"]);
    }

    /// A 3D game whose Lua makes a `Terrain` gets the ground image; a
    /// mention in a comment, a field or another name doesn't.
    #[test]
    fn a_terrain_game_gets_the_ground_image() {
        assert!(v2::makes_terrain("local g = Terrain{ height = 40 }"));
        assert!(v2::makes_terrain("ground = Terrain ( { } )"));
        for no in ["-- Terrain{ }", "self.Terrain{}", "MyTerrain{}", "local t = 'Terrain'", "t:Terrain()"] {
            assert!(!v2::makes_terrain(no), "{no}");
        }
        if !have_engine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio2-terrain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        let toml = std::fs::read_to_string(starters2().join("catch/studio.toml")).unwrap().replace("kind = \"2d\"", "kind = \"3d\"");
        std::fs::write(dir.join("studio.toml"), toml).unwrap();
        std::fs::write(dir.join("game/main.lua"), "local g = Terrain{}\n").unwrap();
        let (_, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        std::fs::write(dir.join("game/main.lua"), "local g = Group{}\n").unwrap();
        let (_, without) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        for (path, bytes) in v2::GROUND_SURFACES {
            assert_eq!(files.get(&format!("assets/{path}")).map(Vec::as_slice), Some(bytes));
            assert!(String::from_utf8_lossy(&files["assets/files.txt"]).lines().any(|l| l == path));
            assert!(!without.contains_key(&format!("assets/{path}")));
        }
    }

    /// A project with shaders/<name>.wgsl gets a shader build (mb2d 4, its
    /// shader's uniforms in packed.toml); a shader anywhere else under
    /// shaders/ (a subfolder, `.WGSL`) is refused rather than packed into a
    /// build without shaders; and a refused shader stops the build at its line.
    #[test]
    fn shaders_get_a_shader_build_and_only_shaders_name_wgsl() {
        if !have_engine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio2-shaders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::create_dir_all(dir.join("shaders/fx")).unwrap();
        std::fs::write(dir.join("studio.toml"), std::fs::read_to_string(starters2().join("catch/studio.toml")).unwrap()).unwrap();
        std::fs::write(dir.join("game/main.lua"), "local s = love.graphics.newShader(\"glow\")\n").unwrap();
        let glow = "struct Uniforms { t: f32 }\nfn sprite(in: Sprite) -> vec4<f32> { return in.color * uniforms.t; }\n";
        std::fs::write(dir.join("shaders/glow.wgsl"), glow).unwrap();
        let (m, files) = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap();
        assert_eq!(mb_format::studio::variant(&m), Variant::PlainShaders);
        assert!(m.needs_formats().contains(&"mb2d4"));
        assert!(String::from_utf8_lossy(&files["assets/packed.toml"]).contains("[shaders.glow]\nkind = \"sprite\""));
        for wrong in ["shaders/fx/glow2.wgsl", "shaders/GLOW3.WGSL", "shaders/glow4.Wgsl"] {
            std::fs::write(dir.join(wrong), glow).unwrap();
            let e = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap_err();
            assert!(e.contains(wrong) && e.contains("shaders/<name>.wgsl"), "{wrong}: {e}");
            let r = super::check::check(&dir).unwrap();
            assert!(r["syntax"].as_array().unwrap().iter().any(|s| s["path"] == wrong), "{wrong}: {r}");
            std::fs::remove_file(dir.join(wrong)).unwrap();
        }
        std::fs::write(dir.join("shaders/glow.wgsl"), "fn sprite(in: Sprite) -> vec4<f32> {\n    return nope;\n}\n").unwrap();
        let e = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(e.starts_with("shaders/glow.wgsl:2: "), "{e}");
    }

    /// A syntax error stops the build with its file and line, as the
    /// engine's runtime errors name them.
    #[test]
    fn lua_syntax_errors_stop_the_build_at_their_line() {
        if !have_engine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio2-syntax-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::copy(starters2().join("catch/studio.toml"), dir.join("studio.toml")).unwrap();
        std::fs::write(dir.join("game/main.lua"), "function love.load()\n  local x = = 1\nend\n").unwrap();
        let e = bundle_files(&dir, Some(b"\0asm".to_vec())).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(e.starts_with("game/main.lua:2: "), "{e}");
    }

    /// The validator's bytecode versions are the engine's Luau's.
    #[test]
    fn bytecode_versions_match_the_engines_luau() {
        assert_eq!(mb_format::studio::BYTECODE_VERSIONS, luau::BYTECODE_VERSION_MIN..=luau::BYTECODE_VERSION_MAX);
    }

    /// Engine 2.0's published build is made from its source as it is, it's
    /// in PUBLISHED, and its starters pack and validate with it.
    #[test]
    fn the_published_engine_2_0_is_current_and_packs_its_starters() {
        if !have_engine() {
            return;
        }
        let dir = starters2().parent().unwrap().to_path_buf();
        let published = std::fs::read_to_string(dir.join("published/SOURCE")).unwrap_or_default();
        assert_eq!(published.trim(), source_fingerprint2(&dir).unwrap(), "games/studio-engine2, crates/luau, crates/recast or sdk/maimbrain changed since engine 2.0's builds were published: run `just studio-engine2` and paste its line into mb_format::studio::PUBLISHED");
        if std::env::var_os("MB_STUDIO_ENGINE2").is_some() {
            return;
        }
        for d in std::fs::read_dir(starters2()).unwrap().flatten().map(|e| e.path()).filter(|d| d.join("studio.toml").is_file()) {
            let (_, files) = bundle_files(&d, None).unwrap_or_else(|e| panic!("{}: {e}", d.display()));
            let r = mb_format::validate(&mb_format::pack(&files).unwrap());
            assert!(r.ok(), "{}: {:?}", d.display(), r.errors);
            let info = r.studio.unwrap();
            assert!(info.published && info.engine == "2.0", "{}: packed with the published engine 2.0", d.display());
        }
    }

    /// The published engine is outside the startup set (SPEC "Studio
    /// bundles", Decided 39); a local build of it isn't.
    #[test]
    fn the_published_engine_doesnt_count_in_the_startup_set() {
        if !have_engine() || std::env::var_os("MB_STUDIO_ENGINE2").is_some() {
            return;
        }
        let (_, mut files) = bundle_files(&starters2().join("catch"), None).unwrap();
        let published = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(published.ok() && published.studio.as_ref().unwrap().published, "{:?}", published.errors);
        // The same engine with a custom section on: valid, but not a published build.
        files.get_mut("game.wasm").unwrap().extend([0, 5, 4, b'l', b'o', b'c', b'l']);
        let local = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(local.ok() && !local.studio.as_ref().unwrap().published, "{:?}", local.errors);
        assert!(local.startup_size > published.startup_size + 300_000, "{} vs {}", local.startup_size, published.startup_size);
    }

    /// A Studio 2.0 bundle's bytecode is checked against its source: the
    /// packer's is the compiler's output, byte for byte (reproducible), and a
    /// chunk changed by hand, or a source changed under its bytecode, is an
    /// error. `mb validate --json`, which the server runs, says so.
    #[test]
    fn bytecode_is_checked_against_its_source() {
        if !have_engine() || std::env::var_os("MB_STUDIO_ENGINE2").is_some() {
            return;
        }
        let (_, files) = bundle_files(&starters2().join("runner"), None).unwrap();
        let (_, again) = bundle_files(&starters2().join("runner"), None).unwrap();
        assert_eq!(files["assets/game/main.luac"], again["assets/game/main.luac"], "the packer's bytecode is reproducible");
        let open = |f: &BTreeMap<String, Vec<u8>>| mb_format::Bundle::open(&mb_format::pack(f).unwrap()).unwrap();
        assert!(v2::check_bytecode(&open(&files)).is_empty());

        // A crafted chunk: the version byte is right, the rest isn't the compiler's.
        let mut crafted = files.clone();
        let bc = crafted.get_mut("assets/game/main.luac").unwrap();
        let last = bc.len() - 1;
        bc[last] ^= 0xff;
        assert!(mb_format::validate(&mb_format::pack(&crafted).unwrap()).ok(), "the core's validator can't tell (it has no compiler)");
        let e = v2::check_bytecode(&open(&crafted));
        assert!(e.len() == 1 && e[0].contains("main.luac isn't the pinned Luau compiler's output"), "{e:?}");
        // Source changed under its bytecode.
        let mut edited = files.clone();
        edited.get_mut("assets/game/main.lua").unwrap().extend(b"\nlocal _ = 1\n");
        assert_eq!(v2::check_bytecode(&open(&edited)).len(), 1);

        // Through `mb validate --json`, as the server runs it.
        let dir = std::env::temp_dir().join(format!("mb-bytecode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, f, ok) in [("good.mbx", &files, true), ("crafted.mbx", &crafted, false)] {
            let p = dir.join(name);
            std::fs::write(&p, mb_format::pack(f).unwrap()).unwrap();
            assert_eq!(crate::validate(&p, false, None).is_ok(), ok, "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Engine 2.0's fingerprint covers Luau's sandbox and the SDK, not only
    /// the engine's crate.
    #[test]
    fn engine_2_s_fingerprint_covers_luau_and_the_sdk() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if !root.join("games/studio-engine2/src").is_dir() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("mb-fingerprint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for rel in ["games/studio-engine2", "crates/luau", "crates/recast", "sdk/maimbrain"] {
            let to = tmp.join(rel);
            std::fs::create_dir_all(&to).unwrap();
            assert!(std::process::Command::new("cp").arg("-R").arg(root.join(rel)).arg(to.parent().unwrap()).status().unwrap().success());
        }
        let engine = tmp.join("games/studio-engine2");
        let base = source_fingerprint2(&engine).unwrap();
        for (file, change) in [("crates/luau/csrc/vm.cpp", "// a sandbox change\n"), ("crates/recast/csrc/shim.cpp", "// a navmesh change\n"), ("sdk/maimbrain/src/scene/mod.rs", "// a scene change\n")] {
            let p = tmp.join(file);
            let before = std::fs::read(&p).unwrap();
            std::fs::write(&p, [before.clone(), change.as_bytes().to_vec()].concat()).unwrap();
            assert_ne!(source_fingerprint2(&engine).unwrap(), base, "{file} is in engine 2.0's fingerprint");
            std::fs::write(&p, before).unwrap();
        }
        // A test file isn't.
        std::fs::write(tmp.join("crates/luau/src/tests.rs"), "// changed").unwrap();
        assert_eq!(source_fingerprint2(&engine).unwrap(), base);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn kind_picks_the_build() {
        let t = |s: &str| is_3d(&s.parse::<Table>().unwrap());
        assert_eq!(t(""), Ok(false));
        assert_eq!(t("kind = \"2d\""), Ok(false));
        assert_eq!(t("kind = \"3d\""), Ok(true));
        assert!(t("kind = \"lua\"").unwrap_err().contains("\"2d\" or \"3d\""));
        // As projects made before `kind` say it.
        assert_eq!(t("rung = \"arcade\""), Ok(false));
        assert_eq!(t("rung = \"lua\"\nview = \"3d\""), Ok(true));
        assert!(t("rung = \"lua\"").unwrap_err().contains("the old way"));
        assert!(t("view = \"3d\"").unwrap_err().contains("the old way"));
    }

    #[test]
    fn art_off_the_palette_is_refused() {
        let mut img = Image::new(16, 16, [0, 0, 0, 0]);
        img.set(3, 7, [1, 2, 3, 255]);
        let e = art(&img.encode_png(), "sprites/x.png", &PALETTES[0].1, "sunset16", false).unwrap_err();
        assert!(e.contains("1 pixels aren't the project's colours (palette sunset16), the first at (3, 7): #010203"), "{e}");
        let e = art(&Image::new(24, 24, [0, 0, 0, 0]).encode_png(), "sprites/x.png", &PALETTES[0].1, "sunset16", false).unwrap_err();
        assert!(e.contains("16 or 32 pixels high"), "{e}");
    }
}
