//! The `.mbx` container (SPEC §1): a ZIP with a fixed layout and limits.
//! `pack` is deterministic (sorted entries, fixed timestamps) so the same
//! inputs always produce the same bytes and hash.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::manifest::Manifest;

/// SPEC §1: 25 MB, of which more than [`LARGE`] needs the `large` format.
pub const MAX_COMPRESSED: u64 = 25 * MIB;
/// A bundle bigger than this (compressed) is format `large`: only apps whose
/// validator opens 25 MB bundles get it (SPEC §1; docs/STUDIO-2.md §4.8).
pub const LARGE: u64 = 10 * MIB;
pub const MAX_WASM: u64 = 4 * MIB;
pub const MAX_STARTUP: u64 = 3 * MIB;
/// A published Studio engine's own cap, compressed: it isn't in the startup
/// set (SPEC "Studio bundles").
pub const MAX_PUBLISHED_ENGINE: u64 = 3 * MIB / 2;
pub const MAX_FILES: usize = 512;
/// Zip-bomb guard: total uncompressed bytes we're willing to inflate.
pub const MAX_UNCOMPRESSED: u64 = 64 * MIB;
const MIB: u64 = 1024 * 1024;

/// `lua`, `map` and `toml` carry a Studio project (SPEC "Studio bundles"),
/// and `luac` engine 2.0's Lua compiled to Luau bytecode.
pub const ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "ktx2", "glb", "ogg", "opus", "ttf", "mbf", "wgsl", "bin", "json", "txt", "lua", "luac", "map", "toml"];
/// Largest .glb asset (SPEC §1).
pub const MAX_GLB: usize = 8 * MIB as usize;
/// `lua`: the Studio engine's own Lua (its prelude) is part of its source.
pub const SOURCE_EXTENSIONS: &[&str] = &["rs", "toml", "lock", "md", "txt", "wgsl", "lua"];

/// A validated, fully inflated bundle.
#[derive(Debug)]
pub struct Bundle {
    pub manifest: Manifest,
    pub sha256: String,
    pub compressed_size: u64,
    files: BTreeMap<String, Vec<u8>>,
    metered_wasm: Vec<u8>,
    /// The metered wasm came from the cache ([`Bundle::open_cached`]), not
    /// the metering pass.
    pub engine_kept: bool,
}

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub manifest: Option<Manifest>,
    pub sha256: String,
    pub compressed_size: u64,
    pub wasm_size: u64,
    pub startup_size: u64,
    /// The Maimbrain SDK version the game was built with, if it used the SDK
    /// (game.wasm's `maimbrain-sdk` section, SPEC §3).
    pub sdk_version: Option<String>,
    /// A Studio bundle's engine (SPEC "Studio bundles").
    pub studio: Option<crate::studio::Info>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// The bundle formats an app must open to get this bundle (`formats` in
    /// `mb validate --json`, matched against [`crate::studio::FORMATS`]):
    /// the manifest's ([`Manifest::needs_formats`]), and `large` past
    /// [`LARGE`].
    pub fn formats(&self) -> Vec<&'static str> {
        let mut f = self.manifest.as_ref().map(Manifest::needs_formats).unwrap_or_default();
        if self.compressed_size > LARGE {
            f.push("large");
        }
        f
    }
}

/// Where a client keeps metered wasm between opens (SPEC "Studio bundles",
/// "validated and metered once"): by [`crate::meter::cache_key`], the
/// metering version and the SHA-256 of the unmetered bytes.
/// [`Bundle::open_cached`] uses it only for a published Studio engine, the
/// same bytes in every Studio game, so it holds a few entries, not one a game.
pub trait MeterCache {
    /// The metered wasm kept under `key`, if any.
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    /// Keeps `metered` under `key` (best effort).
    fn put(&self, key: &str, metered: &[u8]);
}

impl Bundle {
    /// Inflates and fully validates a bundle.
    pub fn open(bytes: &[u8]) -> Result<Bundle, Report> {
        Self::open_inner(bytes, None)
    }

    /// [`open`](Self::open), with a published Studio engine's metered wasm
    /// from `cache` when it's there (and put there when it isn't). Validation
    /// still runs in full; only the metering pass, most of an open's time,
    /// is skipped.
    pub fn open_cached(bytes: &[u8], cache: &dyn MeterCache) -> Result<Bundle, Report> {
        Self::open_inner(bytes, Some(cache))
    }

    fn open_inner(bytes: &[u8], cache: Option<&dyn MeterCache>) -> Result<Bundle, Report> {
        let (report, files) = inspect(bytes);
        Self::finish(report, files, cache)
    }

    /// A content-addressed preview's files (SPEC "Content-addressed
    /// previews"), opened as [`open_cached`](Self::open_cached) opens a
    /// bundle, with nothing packed: every per-path rule ([`check_paths`]),
    /// then every check after unzipping ([`check_files`]), the startup set
    /// counted from `game.wasm`'s and the startup assets' sizes as [`pack`]
    /// would store them. What a fresh page plays: a project's first open, or
    /// a change to its `game.wasm` or `manifest.toml`. Its `sha256` is
    /// [`files_digest`], the list's, as the server names the build
    /// (`files_sha256`); `compressed_size` is the files' total.
    pub fn open_files(files: BTreeMap<String, Vec<u8>>, cache: Option<&dyn MeterCache>) -> Result<Bundle, Report> {
        let mut r = Report { sha256: files_digest(&files), ..Default::default() };
        r.errors = check_paths(&files);
        let total = files.values().map(|d| d.len() as u64).sum::<u64>();
        r.compressed_size = total;
        if total > MAX_UNCOMPRESSED {
            r.errors.push(format!("the files add up to {total} bytes; the limit is {MAX_UNCOMPRESSED}"));
            return Err(r);
        }
        // Only the startup set reads packed sizes: game.wasm's and the startup assets'.
        let startup = files.get("manifest.toml").and_then(|m| std::str::from_utf8(m).ok()).and_then(|m| Manifest::parse(m).ok()).map(|m| m.startup_assets).unwrap_or_default();
        let compressed = files
            .iter()
            .filter(|(p, _)| *p == "game.wasm" || startup.contains(p))
            .map(|(p, d)| (p.clone(), packed_size(p, d)))
            .collect();
        check_files(&mut r, &files, &compressed, &|_| false);
        let ok = r.errors.is_empty();
        Self::finish(r, ok.then_some(files), cache)
    }

    /// A validated bundle's files, with its wasm metered (or the metered
    /// wasm kept for a published Studio engine, with `cache`).
    fn finish(report: Report, files: Option<BTreeMap<String, Vec<u8>>>, cache: Option<&dyn MeterCache>) -> Result<Bundle, Report> {
        match (report.ok(), report.manifest.clone(), files) {
            (true, Some(manifest), Some(files)) => {
                let wasm = &files["game.wasm"];
                let cache = cache.filter(|_| report.studio.as_ref().is_some_and(|s| s.published));
                let key = cache.map(|_| crate::meter::cache_key(wasm));
                let kept = cache.zip(key.as_deref()).and_then(|(c, k)| c.get(k)).filter(|w| w.starts_with(b"\0asm"));
                let engine_kept = kept.is_some();
                let metered_wasm = match kept {
                    Some(w) => w,
                    None => match crate::meter::instrument(wasm) {
                        Ok(w) => {
                            if let (Some(c), Some(k)) = (cache, key.as_deref()) {
                                c.put(k, &w);
                            }
                            w
                        }
                        Err(e) => {
                            let mut report = report;
                            report.errors.push(format!("game.wasm: {e}"));
                            return Err(report);
                        }
                    },
                };
                Ok(Bundle { manifest, sha256: report.sha256, compressed_size: report.compressed_size, files, metered_wasm, engine_kept })
            }
            _ => Err(report),
        }
    }

    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// The wasm to actually run: `game.wasm` with loop metering (see `meter`).
    pub fn runtime_wasm(&self) -> &[u8] {
        &self.metered_wasm
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

/// Validates a bundle without keeping it.
pub fn validate(bytes: &[u8]) -> Report {
    inspect(bytes).0
}

fn inspect(bytes: &[u8]) -> (Report, Option<BTreeMap<String, Vec<u8>>>) {
    let mut r = Report {
        sha256: hex(&Sha256::digest(bytes)),
        compressed_size: bytes.len() as u64,
        ..Default::default()
    };
    if r.compressed_size > MAX_COMPRESSED {
        r.errors.push(format!("bundle is {} bytes; the limit is {MAX_COMPRESSED} (25 MiB)", r.compressed_size));
    }
    let mut zip = match ZipArchive::new(Cursor::new(bytes)) {
        Ok(z) => z,
        Err(e) => {
            r.errors.push(format!("not a valid ZIP archive: {e}"));
            return (r, None);
        }
    };
    if zip.len() > MAX_FILES {
        r.errors.push(format!(
            "bundle has {} entries; the limit is {MAX_FILES} (every file is one: `mb build` leaves provenance sidecars out, packs lip timings into one file per character and says what the rest are)",
            zip.len()
        ));
        return (r, None);
    }

    let mut files = BTreeMap::new();
    let mut compressed = BTreeMap::new();
    let mut lowercase = BTreeMap::new();
    let mut inflated = 0u64;
    for i in 0..zip.len() {
        let mut entry = match zip.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                r.errors.push(format!("entry {i}: {e}"));
                continue;
            }
        };
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        if entry.is_symlink() {
            r.errors.push(format!("{name}: symlinks are not allowed"));
            continue;
        }
        if !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            r.errors.push(format!("{name}: only stored or deflate compression is allowed"));
            continue;
        }
        if let Err(e) = check_path(&name) {
            r.errors.push(format!("{name:?}: {e}"));
            continue;
        }
        if let Some(other) = lowercase.insert(name.to_lowercase(), name.clone()) {
            r.errors.push(format!("{name} and {other} differ only by case"));
            continue;
        }
        // Don't trust the header's size: read at most what's left of the budget.
        let budget = MAX_UNCOMPRESSED - inflated;
        let mut data = Vec::new();
        if let Err(e) = (&mut entry).take(budget + 1).read_to_end(&mut data) {
            r.errors.push(format!("{name}: {e}"));
            continue;
        }
        inflated += data.len() as u64;
        if inflated > MAX_UNCOMPRESSED {
            r.errors.push(format!("bundle inflates to more than {MAX_UNCOMPRESSED} bytes"));
            return (r, None);
        }
        compressed.insert(name.clone(), entry.compressed_size());
        files.insert(name, data);
    }

    check_files(&mut r, &files, &compressed, &|_| false);
    let ok = r.errors.is_empty();
    (r, ok.then_some(files))
}

/// Every check after unzipping, on a bundle's files as a map, so a
/// content-addressed preview (SPEC "Content-addressed previews") is checked
/// as a bundle is without being packed. `compressed`: each file's size as
/// [`pack`] stores it (the startup set counts those). `checked(path)`: that
/// file's bytes passed their own checks before (a `.glb`; `game.wasm`
/// against this same manifest), so they're skipped; every bundle-wide rule
/// still runs.
pub fn check_files(r: &mut Report, files: &BTreeMap<String, Vec<u8>>, compressed: &BTreeMap<String, u64>, checked: &dyn Fn(&str) -> bool) {
    for required in ["manifest.toml", "game.wasm", "icon.png"] {
        if !files.contains_key(required) {
            r.errors.push(format!("missing required file {required}"));
        }
    }
    if !files.keys().any(|p| p.starts_with("src/")) {
        r.errors.push("missing src/: bundles must include the source the wasm was built from".into());
    }

    if let Some(text) = files.get("manifest.toml") {
        match std::str::from_utf8(text).map_err(|e| e.to_string()).and_then(Manifest::parse) {
            Ok(m) => r.manifest = Some(m),
            Err(e) => r.errors.push(format!("manifest.toml: {e}")),
        }
    }
    if let Some(png) = files.get("icon.png") {
        match png_size(png) {
            Some((256, 256)) => {}
            Some((w, h)) => r.errors.push(format!("icon.png is {w}×{h}; it must be 256×256")),
            None => r.errors.push("icon.png is not a PNG".into()),
        }
    }

    for (path, data) in files {
        if path.starts_with("assets/")
            && path.to_ascii_lowercase().ends_with(".glb")
            && !checked(path)
            && let Err(e) = check_glb(data)
        {
            r.errors.push(format!("{path}: {e}"));
        }
    }
    check_fonts(files, r);

    if let Some(m) = r.manifest.clone() {
        m.check(&mut r.errors);
        for asset in &m.startup_assets {
            if !files.contains_key(asset) {
                r.errors.push(format!("manifest: startup asset {asset} is not in the bundle"));
            }
        }
        r.studio = crate::studio::check(&m, files, &mut r.errors, &mut r.warnings);
        let engine = r.studio.as_ref().is_some_and(outside_startup_set);
        let assets = m.startup_assets.iter().filter_map(|a| compressed.get(a)).sum::<u64>();
        r.startup_size = startup_set(engine, compressed.get("game.wasm").copied().unwrap_or(0), assets, &mut r.errors);
        if let Some(wasm) = files.get("game.wasm") {
            r.wasm_size = wasm.len() as u64;
            if r.wasm_size > MAX_WASM {
                r.errors.push(format!("game.wasm is {} bytes; the limit is {MAX_WASM}", r.wasm_size));
            }
            if !checked("game.wasm") {
                crate::wasm::check(wasm, &m, &mut r.errors);
            }
            r.sdk_version = crate::wasm::sdk_version(wasm);
        }
        // A published Studio engine's size isn't the creator's to change.
        if r.wasm_size > MIB && r.wasm_size <= MAX_WASM && !r.studio.as_ref().is_some_and(|s| s.published) {
            r.warnings.push(format!("game.wasm is {} KiB; it SHOULD be under 1 MiB", r.wasm_size / 1024));
        }
    }
}

/// The per-path rules [`validate`] applies while unzipping (how many
/// entries, each path's shape, no two differing only by case), for a
/// bundle checked as a map ([`check_files`]).
pub fn check_paths(files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut errors = Vec::new();
    if files.len() > MAX_FILES {
        errors.push(format!("bundle has {} entries; the limit is {MAX_FILES}", files.len()));
    }
    let mut lowercase = BTreeMap::new();
    for name in files.keys() {
        if let Err(e) = check_path(name) {
            errors.push(format!("{name:?}: {e}"));
        } else if let Some(other) = lowercase.insert(name.to_lowercase(), name.clone()) {
            errors.push(format!("{name} and {other} differ only by case"));
        }
    }
    errors
}

/// The checks a file arriving on its own needs (SPEC "Content-addressed
/// previews"): a content-addressed preview's changed file skipped the
/// whole-bundle validation, so the app checks its path (as [`validate`] does
/// each entry's: relative, `/`, no `.` or `..`, an allowed extension, under
/// `assets/` or `src/` or one of the three top-level files) and what §1 checks
/// of its type: a `.glb`'s header, length and chunks ([`check_glb`]), a game
/// font's `.mbf` structure, and a Lua chunk's bytecode version (Studio 2.0's
/// `.luac`, [`crate::studio::BYTECODE_VERSIONS`]). Bundle-wide rules (the
/// manifest, the startup set, the file count) are the server's, on the list.
pub fn check_file(path: &str, data: &[u8]) -> Result<(), String> {
    check_path(path).map_err(|e| format!("{path:?}: {e}"))?;
    if data.len() as u64 > MAX_UNCOMPRESSED {
        return Err(format!("{path}: {} bytes; the limit is {MAX_UNCOMPRESSED}", data.len()));
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if !path.starts_with("assets/") {
        return Ok(());
    }
    match ext.as_str() {
        "glb" => check_glb(data).map_err(|e| format!("{path}: {e}")),
        "mbf" => crate::font::MbFont::parse(data).map(|_| ()).map_err(|e| format!("{path}: {e}")),
        "luac" if path.starts_with("assets/game/") => match data.first() {
            Some(v) if crate::studio::BYTECODE_VERSIONS.contains(v) => Ok(()),
            Some(v) => Err(format!("{path}: bytecode version {v}; the engine loads {}–{}", crate::studio::BYTECODE_VERSIONS.start(), crate::studio::BYTECODE_VERSIONS.end())),
            None => Err(format!("{path}: empty bytecode")),
        },
        _ => Ok(()),
    }
}

/// A file list's digest (SPEC "Content-addressed previews"): SHA-256 of each
/// path, a NUL, its SHA-256 (hex) and a newline, in path order. It names a
/// content-addressed build as a bundle's SHA-256 names a bundle (the
/// server's `files_sha256`, `Studio::Previews.digest`).
pub fn files_digest(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut h = Sha256::new();
    for (path, data) in files {
        h.update(path.as_bytes());
        h.update(b"\0");
        h.update(hex(&Sha256::digest(data)).as_bytes());
        h.update(b"\n");
    }
    hex(&h.finalize())
}

/// Whether a Studio bundle's engine leaves the startup set: a published
/// engine 2.0 or later. Engine 1.0 counts, as every app before 2.0 counts
/// it: exempting it would pass 1.0 bundles those apps refuse.
fn outside_startup_set(s: &crate::studio::Info) -> bool {
    s.published && crate::studio::major(&format!("studio@{}", s.engine)).is_some_and(|m| m >= crate::studio::LUAU_MAJOR)
}

/// The startup set's compressed size, with its errors. A published Studio
/// engine is the same bytes in every Studio game, and clients keep it, so it
/// isn't in the set; it has its own cap instead (SPEC "Studio bundles").
fn startup_set(published_engine: bool, wasm: u64, assets: u64, errors: &mut Vec<String>) -> u64 {
    if published_engine && wasm > MAX_PUBLISHED_ENGINE {
        errors.push(format!("game.wasm, a published Studio engine, is {wasm} bytes compressed; a published engine's limit is {MAX_PUBLISHED_ENGINE}"));
    }
    let size = if published_engine { 0 } else { wasm } + assets;
    if size > MAX_STARTUP {
        let what = if published_engine { "startup_assets; the published Studio engine doesn't count" } else { "game.wasm + startup_assets" };
        errors.push(format!("startup set ({what}) is {size} bytes compressed; the limit is {MAX_STARTUP}"));
    }
    size
}

fn check_path(p: &str) -> Result<(), String> {
    if p.len() > 256 {
        return Err("path longer than 256 bytes".into());
    }
    if p.starts_with('/') || p.contains('\\') || p.chars().any(char::is_control) {
        return Err("paths must be relative, use '/', and contain no control characters".into());
    }
    if p.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return Err("paths must not contain empty, '.' or '..' components".into());
    }
    let ext = p.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match p.split_once('/') {
        None if matches!(p, "manifest.toml" | "game.wasm" | "icon.png") => Ok(()),
        None => Err("unexpected top-level file (allowed: manifest.toml, game.wasm, icon.png, src/, assets/)".into()),
        Some(("assets", _)) if ASSET_EXTENSIONS.contains(&ext.as_str()) => Ok(()),
        Some(("assets", _)) => Err(format!("asset type .{ext} is not allowed (allowed: {})", ASSET_EXTENSIONS.join(", "))),
        Some(("src", _)) if SOURCE_EXTENSIONS.contains(&ext.as_str()) => Ok(()),
        Some(("src", _)) => Err(format!("source file type .{ext} is not allowed (allowed: {})", SOURCE_EXTENSIONS.join(", "))),
        Some((dir, _)) => Err(format!("unexpected top-level directory {dir}/ (allowed: src/, assets/)")),
    }
}

/// Game fonts (SPEC §5.3): every `.mbf` parses within its limits, at most
/// `font::MAX_FONTS` of them, and a warning when they add up to a lot.
fn check_fonts(files: &BTreeMap<String, Vec<u8>>, r: &mut Report) {
    let fonts: Vec<_> = files.iter().filter(|(p, _)| p.starts_with("assets/") && p.to_ascii_lowercase().ends_with(".mbf")).collect();
    if fonts.len() > crate::font::MAX_FONTS {
        r.errors.push(format!("{} fonts (.mbf); a game may have at most {}", fonts.len(), crate::font::MAX_FONTS));
    }
    let mut total = 0;
    for (path, data) in &fonts {
        total += data.len();
        match crate::font::MbFont::parse(data) {
            Ok(f) if f.meta_value("license").is_none() => r.warnings.push(format!("{path}: no license in its metadata (re-bake with `mb font add`)")),
            Ok(_) => {}
            Err(e) => r.errors.push(format!("{path}: {e}")),
        }
    }
    if total > 1024 * 1024 {
        r.warnings.push(format!("fonts add up to {} KiB; subset them (`mb font add --chars`) to keep downloads small", total / 1024));
    }
    if files.keys().any(|p| p.starts_with("assets/") && p.to_ascii_lowercase().ends_with(".ttf")) {
        r.warnings.push("a .ttf asset can't be drawn: bake fonts to .mbf with `mb font add`".into());
    }
}

/// Structural checks on a binary glTF (SPEC §1): header, declared length,
/// and chunk bounds. The host parses the rest at load time.
pub fn check_glb(b: &[u8]) -> Result<(), String> {
    if b.len() > MAX_GLB {
        return Err(format!("{} bytes; a .glb may be at most {MAX_GLB}", b.len()));
    }
    let u = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
    if b.len() < 20 || &b[..4] != b"glTF" {
        return Err("not a binary glTF (.glb)".into());
    }
    if u(4) != Some(2) {
        return Err("only glTF 2.0 is supported".into());
    }
    if u(8) != Some(b.len()) {
        return Err("declared length doesn't match the file size".into());
    }
    let json = u(12).unwrap_or(0);
    if u(16) != Some(0x4E4F_534A) || 20 + json > b.len() {
        return Err("the first chunk must be JSON and fit in the file".into());
    }
    let mut at = 20 + json;
    while at + 8 <= b.len() {
        let n = u(at).unwrap_or(0);
        if at + 8 + n > b.len() {
            return Err("a chunk runs past the end of the file".into());
        }
        at += 8 + n;
    }
    Ok(())
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    Some((be(16), be(20)))
}

/// Whether [`pack`] stores a file rather than deflating it: PNG, JPEG, Ogg
/// and glb are compressed already, and deflating them saved 0.4% of a 20 MB
/// project for a third of a second a pack (docs/spike/studio2-live.md,
/// Decided 40). The startup set and the size limits count them as stored.
pub fn stored(path: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "ogg" | "glb")
}

/// Packs files (bundle path → bytes) into a deterministic `.mbx`: sorted,
/// with fixed times, each deflated unless it's [`stored`].
pub fn pack(files: &BTreeMap<String, Vec<u8>>) -> std::io::Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = |path: &str| {
        SimpleFileOptions::default()
            .compression_method(if stored(path) { CompressionMethod::Stored } else { CompressionMethod::Deflated })
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644)
    };
    for (path, data) in files {
        zip.start_file(path.as_str(), opts(path)).map_err(std::io::Error::other)?;
        zip.write_all(data)?;
    }
    Ok(zip.finish().map_err(std::io::Error::other)?.into_inner())
}

/// A file's size as [`pack`] writes it (deflated, or stored): what the
/// startup set and the bundle's limit count, for a bundle checked as a map.
pub fn packed_size(path: &str, data: &[u8]) -> u64 {
    if stored(path) {
        return data.len() as u64;
    }
    let one = BTreeMap::from([(path.to_string(), data.to_vec())]);
    pack(&one)
        .ok()
        .and_then(|z| ZipArchive::new(Cursor::new(z)).ok())
        .and_then(|mut a| a.by_index(0).ok().map(|e| e.compressed_size()))
        .unwrap_or(data.len() as u64)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_published_engine_2_leaves_the_startup_set() {
        use crate::studio::{Info, Variant};
        let info = |engine: &str, published| Info { engine: engine.into(), variant: Variant::Plain, published };
        assert!(outside_startup_set(&info("2.0", true)));
        assert!(!outside_startup_set(&info("2.0", false)), "a local build counts");
        assert!(!outside_startup_set(&info("1.0", true)), "every app before 2.0 counts engine 1.0");
    }

    #[test]
    fn a_published_engine_is_outside_the_startup_set_with_a_cap_of_its_own() {
        let mut e = Vec::new();
        // A kit game's wasm counts.
        assert_eq!(startup_set(false, 2 * MIB, MIB, &mut e), 3 * MIB);
        assert!(e.is_empty());
        assert_eq!(startup_set(false, 2 * MIB, MIB + 1, &mut e), 3 * MIB + 1);
        assert!(e[0].contains("game.wasm + startup_assets") && e[0].contains("the limit is 3145728"), "{e:?}");
        // A published engine doesn't, up to 1.5 MiB compressed.
        let mut e = Vec::new();
        assert_eq!(startup_set(true, MAX_PUBLISHED_ENGINE, 3 * MIB, &mut e), 3 * MIB);
        assert!(e.is_empty(), "{e:?}");
        startup_set(true, MAX_PUBLISHED_ENGINE + 1, 0, &mut e);
        assert!(e[0].contains("a published engine's limit is 1572864"), "{e:?}");
        let mut e = Vec::new();
        startup_set(true, MIB, 3 * MIB + 1, &mut e);
        assert!(e[0].contains("the published Studio engine doesn't count"), "{e:?}");
    }
}
