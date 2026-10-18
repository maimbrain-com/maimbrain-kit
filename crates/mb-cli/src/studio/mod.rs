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

pub mod play;
pub mod synth;

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
const DIRS: [&str; 7] = ["game", "sprites", "tiles", "maps", "sounds", "music", "debug"];
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
/// its two builds and its source; empty without one.
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
        match (self, variant) {
            (Engine::Crate(dir), _) => build_engine(dir, variant),
            (Engine::Published, Variant::Plain) => Ok(embedded::PLAIN.to_vec()),
            (Engine::Published, Variant::Tilt) if !embedded::TILT.is_empty() => Ok(embedded::TILT.to_vec()),
            (Engine::Published, Variant::Tilt) => Err("this mb's published Studio engine has no tilt build".into()),
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
    crate::add_tree(&dir.join("src"), "src", &["rs", "lua", "toml"], &mut files)?;
    files.remove("src/tests.rs");
    let mut h = sha2::Sha256::new();
    for (k, v) in &files {
        h.update(k.as_bytes());
        h.update([0]);
        h.update((v.len() as u64).to_le_bytes());
        h.update(v);
    }
    Ok(mb_format::bundle::hex(&h.finalize()))
}

/// `just studio-engine`: builds both engine builds from the crate and writes
/// them to its `published/`, with the source fingerprint; prints their
/// SHA-256 for `mb_format::studio::PUBLISHED`.
pub fn publish_engine() -> Result<(), String> {
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
    for v in [Variant::Plain, Variant::Tilt] {
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

fn read_toml(path: &Path) -> Result<Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.parse::<Table>().map_err(|e| format!("{}: {}", path.display(), e.message()))
}

fn s<'a>(t: &'a Table, k: &str) -> Option<&'a str> {
    t.get(k).and_then(Value::as_str)
}

/// The project's palette, 0xRRGGBB.
fn palette(t: &Table) -> Result<([u32; 16], String), String> {
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

/// `studio.toml` as the manifest (`startup` is every asset the engine loads).
pub fn manifest(dir: &Path, startup: Vec<String>) -> Result<Manifest, String> {
    let t = read_toml(&dir.join("studio.toml"))?;
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
    if let Some(r) = s(&t, "rung")
        && r != "arcade"
    {
        return Err(format!("studio.toml: rung = \"{r}\": this engine plays the Arcade rung (the Lua rung comes later)"));
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
        perf_tier: PerfTier::Lite,
        stdlib: [("mb2d".to_string(), 1)].into(),
        sensors: if uses_tilt(dir, &t) { vec![Sensor::Tilt] } else { Vec::new() },
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
    let max_memory = PerfTier::Lite.max_memory_pages() * 65536;
    let mut cargo = Command::new("cargo");
    cargo
        .args(["build", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["-p", "studio-engine", "--target", "wasm32-unknown-unknown", "--release", "--target-dir"])
        .arg(&target)
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("-C link-arg=--max-memory={max_memory}"));
    if variant == Variant::Tilt {
        cargo.args(["--features", "sensors"]);
    }
    eprintln!("building the Studio engine ({} build)", variant.as_str());
    let status = cargo.status().map_err(|e| format!("running cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed for the Studio engine".into());
    }
    let raw = target.join("wasm32-unknown-unknown/release/studio_engine.wasm");
    crate::optimize_wasm(&raw).and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())))
}

/// An error in one of the project's files.
fn at(path: &str, e: impl std::fmt::Display) -> String {
    format!("{path}: {e}")
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
    let (pal, pal_name) = palette(&t)?;
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
                    ("sprites" | "tiles", "png") => {
                        assets.insert(rel.clone(), art(&bytes, &rel, &pal, &pal_name, d == "tiles")?);
                    }
                    ("sounds", "toml") if name.ends_with(".sfx.toml") => {
                        let text = String::from_utf8_lossy(&bytes);
                        let x = synth::sfx(&text, &rel)?;
                        let x = crate::generate::audio::normalize(&x, crate::generate::audio::Kind::Sfx, synth::RATE);
                        let ogg = rel.trim_end_matches(".sfx.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, crate::generate::audio::ogg_bytes(&x, synth::RATE, 4.0)?);
                        assets.insert(rel, bytes);
                    }
                    ("music", "toml") if name.ends_with(".song.toml") => {
                        let text = String::from_utf8_lossy(&bytes);
                        let x = synth::song(&text, &rel)?;
                        let g = crate::generate::audio::gain_for(&x, crate::generate::audio::Kind::Music);
                        let x = crate::generate::audio::limit_loop(&x, g, synth::RATE);
                        let ogg = rel.trim_end_matches(".song.toml").to_string() + ".ogg";
                        rendered.push((ogg.clone(), rel.clone()));
                        assets.insert(ogg, crate::generate::audio::ogg_bytes(&x, synth::RATE, 4.0)?);
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
    let startup: Vec<String> = files.keys().filter(|k| k.starts_with("assets/")).cloned().collect();
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
        _ => "scenarios.toml",
    }
}

/// `mb build` for a project: pack the bundle and validate it.
pub fn build_and_pack(dir: &Path, out: &Path) -> Result<PathBuf, String> {
    let (manifest, files) = bundle_files(dir, None)?;
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
    fn a_starter_becomes_a_bundles_files() {
        if !have_engine() {
            return;
        }
        let dir = starters().join("catch");
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
        for (variant, wasm) in [(Variant::Plain, embedded::PLAIN), (Variant::Tilt, embedded::TILT)] {
            assert!(
                mb_format::studio::PUBLISHED.iter().any(|p| p.version == embedded::VERSION && p.variant == variant && p.sha256 == sha(wasm)),
                "the {} build inside mb isn't in mb_format::studio::PUBLISHED (run `just studio-engine` and paste its lines)",
                variant.as_str()
            );
        }
        if !have_engine() || std::env::var_os("MB_STUDIO_ENGINE").is_some() {
            return;
        }
        let (_, files) = bundle_files(&starters().join("catch"), None).unwrap();
        let r = mb_format::validate(&mb_format::pack(&files).unwrap());
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.wasm_size > 1024 * 1024, "the engine is over 1 MiB, so the next line tests something");
        assert!(!r.warnings.iter().any(|w| w.contains("SHOULD be under 1 MiB")), "no size warning for the published engine: {:?}", r.warnings);
        assert!(r.studio.unwrap().published, "packed with the published engine");
        assert!(files.contains_key("src/src/prelude.lua"), "the engine's source, embedded too");
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
