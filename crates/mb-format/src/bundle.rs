//! The `.mbx` container (SPEC §1): a ZIP with a fixed layout and limits.
//! `pack` is deterministic (sorted entries, fixed timestamps) so the same
//! inputs always produce the same bytes and hash.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::manifest::Manifest;

pub const MAX_COMPRESSED: u64 = 10 * MIB;
pub const MAX_WASM: u64 = 4 * MIB;
pub const MAX_STARTUP: u64 = 3 * MIB;
pub const MAX_FILES: usize = 512;
/// Zip-bomb guard: total uncompressed bytes we're willing to inflate.
pub const MAX_UNCOMPRESSED: u64 = 64 * MIB;
const MIB: u64 = 1024 * 1024;

/// `lua`, `map` and `toml` carry a Studio project (SPEC "Studio bundles").
pub const ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "ktx2", "glb", "ogg", "opus", "ttf", "mbf", "wgsl", "bin", "json", "txt", "lua", "map", "toml"];
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
}

impl Bundle {
    /// Inflates and fully validates a bundle.
    pub fn open(bytes: &[u8]) -> Result<Bundle, Report> {
        let (report, files) = inspect(bytes);
        match (report.ok(), report.manifest.clone(), files) {
            (true, Some(manifest), Some(files)) => {
                let metered_wasm = match crate::meter::instrument(&files["game.wasm"]) {
                    Ok(w) => w,
                    Err(e) => {
                        let mut report = report;
                        report.errors.push(format!("game.wasm: {e}"));
                        return Err(report);
                    }
                };
                Ok(Bundle { manifest, sha256: report.sha256, compressed_size: report.compressed_size, files, metered_wasm })
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
        r.errors.push(format!("bundle is {} bytes; the limit is {MAX_COMPRESSED} (10 MiB)", r.compressed_size));
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

    for (path, data) in &files {
        if path.starts_with("assets/")
            && path.to_ascii_lowercase().ends_with(".glb")
            && let Err(e) = check_glb(data)
        {
            r.errors.push(format!("{path}: {e}"));
        }
    }
    check_fonts(&files, &mut r);

    if let Some(m) = r.manifest.clone() {
        m.check(&mut r.errors);
        for asset in &m.startup_assets {
            if !files.contains_key(asset) {
                r.errors.push(format!("manifest: startup asset {asset} is not in the bundle"));
            }
        }
        r.startup_size = compressed.get("game.wasm").copied().unwrap_or(0)
            + m.startup_assets.iter().filter_map(|a| compressed.get(a)).sum::<u64>();
        if r.startup_size > MAX_STARTUP {
            r.errors.push(format!("startup set (game.wasm + startup_assets) is {} bytes compressed; the limit is {MAX_STARTUP}", r.startup_size));
        }
        if let Some(wasm) = files.get("game.wasm") {
            r.wasm_size = wasm.len() as u64;
            if r.wasm_size > MAX_WASM {
                r.errors.push(format!("game.wasm is {} bytes; the limit is {MAX_WASM}", r.wasm_size));
            }
            crate::wasm::check(wasm, &m, &mut r.errors);
            r.sdk_version = crate::wasm::sdk_version(wasm);
        }
        r.studio = crate::studio::check(&m, &files, &mut r.errors, &mut r.warnings);
        // A published Studio engine's size isn't the creator's to change.
        if r.wasm_size > MIB && r.wasm_size <= MAX_WASM && !r.studio.as_ref().is_some_and(|s| s.published) {
            r.warnings.push(format!("game.wasm is {} KiB; it SHOULD be under 1 MiB", r.wasm_size / 1024));
        }
    }
    let ok = r.errors.is_empty();
    (r, ok.then_some(files))
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

/// Packs files (bundle path → bytes) into a deterministic `.mbx`.
pub fn pack(files: &BTreeMap<String, Vec<u8>>) -> std::io::Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for (path, data) in files {
        zip.start_file(path.as_str(), opts).map_err(std::io::Error::other)?;
        zip.write_all(data)?;
    }
    Ok(zip.finish().map_err(std::io::Error::other)?.into_inner())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
