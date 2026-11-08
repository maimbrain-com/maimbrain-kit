//! Content-addressed previews (docs/STUDIO-2.md §4.8, Decided 40; SPEC
//! "Content-addressed previews"): `mb build <project> --ca <store>`.
//!
//! A preview build is a file list (each bundle path's SHA-256 and size) plus
//! the files themselves, stored by SHA-256. Nothing is zipped. The Studio
//! server builds one per commit from a working tree it keeps per project
//! (writing only the paths the commit changed), uploads the files it hasn't
//! uploaded before, and puts the list in the phone's `build` message; the
//! phone fetches only the files it doesn't have. A live update then costs
//! the changed files, not the project (S6: a one-line Lua change on a 20 MB
//! project went from 9.9 s to 0.39 s on the phone).
//!
//! The store is one project's: after a build it holds exactly that build's
//! files (`<sha>`), its `index.json` (each file's size as `pack` writes it,
//! by sha, so a big file is measured once, and which files' own checks
//! passed) and `render/` (sounds and songs rendered from `.sfx.toml` and
//! `.song.toml`, by their source's SHA-256, so an unchanged song isn't
//! rendered again). A file the build no longer has leaves the store.
//!
//! The list is checked as `mb validate` checks a bundle
//! ([`mb_format::bundle::check_files`]), except that a `.glb` or `game.wasm`
//! whose bytes passed before isn't checked again, and the bytecode isn't
//! compiled a second time (this build just compiled it from the source
//! beside it). So the server needs no second `mb validate`. Writes
//! `<out>/preview.json`:
//!
//!     { "id", "version", "engine", "files": { "<path>": { "sha", "size" } },
//!       "startup_size", "size", "formats", "written": ["<sha>", …], "published" }
//!
//! `size` is about the bundle's size as `pack` would write it, `written` the
//! files this build stored that the store didn't have.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The store's index: each file's packed size, and what passed its own checks.
const INDEX: &str = "index.json";
/// Rendered sounds and songs, by their source's SHA-256.
pub const RENDER: &str = "render";
/// A zip's bytes for each file besides its data (its two headers, about).
const ZIP_ENTRY: u64 = 100;

fn sha_hex(b: &[u8]) -> String {
    mb_format::bundle::hex(&<sha2::Sha256 as sha2::Digest>::digest(b))
}

/// Writes `bytes` to `path` whole: a temporary file, then a rename.
fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// `mb build <dir> --ca <store>`: the preview's files into `store`, its list
/// in `<out>/preview.json`. Returns preview.json's path.
pub fn build(dir: &Path, out: &Path, store: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(store.join(RENDER)).map_err(|e| format!("{}: {e}", store.display()))?;
    let (manifest, files) = super::with_render_cache(&store.join(RENDER), || super::bundle_files(dir, None))?;
    let shas: BTreeMap<&str, String> = files.iter().map(|(p, b)| (p.as_str(), sha_hex(b))).collect();
    let index_path = store.join(INDEX);
    let index: Value = std::fs::read(&index_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}));
    let checked: BTreeSet<String> = index["checked"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();

    // Each file's size as pack writes it: kept by whether it's stored and its sha.
    let size_key = |p: &str| format!("{}:{}", mb_format::bundle::stored(p) as u8, shas[p]);
    let packed: BTreeMap<String, u64> =
        files.iter().map(|(p, b)| (p.clone(), index["sizes"].get(size_key(p)).and_then(Value::as_u64).unwrap_or_else(|| mb_format::bundle::packed_size(p, b)))).collect();
    // game.wasm is checked against the manifest's imports: its key is both.
    let key = |p: &str| if p == "game.wasm" { format!("{}+{}", shas[p], shas.get("manifest.toml").map_or("", |s| s.as_str())) } else { shas[p].clone() };
    let mut r = mb_format::Report { errors: mb_format::bundle::check_paths(&files), ..Default::default() };
    mb_format::bundle::check_files(&mut r, &files, &packed, &|p| checked.contains(&key(p)));
    r.compressed_size = packed.values().sum::<u64>() + ZIP_ENTRY * files.len() as u64;
    if r.compressed_size > mb_format::bundle::MAX_COMPRESSED {
        r.errors.push(format!("the bundle would be about {} bytes; the limit is {} (25 MiB)", r.compressed_size, mb_format::bundle::MAX_COMPRESSED));
    }
    if !r.ok() {
        return Err(format!("the preview isn't valid:\n  {}", r.errors.join("\n  ")));
    }

    // Store what's new, and drop what this build no longer has.
    let mut written = Vec::new();
    for (p, b) in &files {
        let sha = &shas[p.as_str()];
        let f = store.join(sha);
        if !f.is_file() {
            write_whole(&f, b)?;
            if !written.contains(sha) {
                written.push(sha.clone());
            }
        }
    }
    let keep: BTreeSet<&str> = shas.values().map(String::as_str).collect();
    for e in std::fs::read_dir(store).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.len() == 64 && name.bytes().all(|c| c.is_ascii_hexdigit()) && !keep.contains(name.as_str()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let sizes: serde_json::Map<String, Value> = files.keys().map(|p| (size_key(p), packed[p].into())).collect();
    let passed: Vec<String> = files.keys().filter(|p| p.to_ascii_lowercase().ends_with(".glb") || *p == "game.wasm").map(|p| key(p)).collect();
    write_whole(&index_path, &serde_json::to_vec(&json!({ "sizes": sizes, "checked": passed })).unwrap())?;
    super::prune_render_cache(&store.join(RENDER));

    let list: serde_json::Map<String, Value> = files.iter().map(|(p, b)| (p.clone(), json!({ "sha": shas[p.as_str()], "size": b.len() }))).collect();
    let preview = json!({
        "id": manifest.id, "version": manifest.version, "engine": manifest.engine, "files": list,
        "startup_size": r.startup_size, "size": r.compressed_size, "formats": r.formats(), "written": written,
        // A Studio bundle's engine is a published build (the server plays only those).
        "published": r.studio.as_ref().map(|s| s.published),
    });
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("preview.json");
    write_whole(&path, &serde_json::to_vec(&preview).unwrap())?;
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    println!("ok: {} ({} files, {} new; about {} bytes as a bundle)", path.display(), files.len(), written.len(), r.compressed_size);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mb-ca-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn copy(from: &Path, to: &Path) {
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let (p, q) = (e.path(), to.join(e.file_name()));
            if p.is_dir() {
                std::fs::create_dir_all(&q).unwrap();
                copy(&p, &q);
            } else {
                std::fs::copy(&p, &q).unwrap();
            }
        }
    }

    /// A 2.0 starter as a preview: every bundle file listed by sha and
    /// stored; a one-line Lua change stores only its source and bytecode;
    /// the list is what the packed bundle holds.
    #[test]
    fn a_preview_lists_and_stores_only_whats_new() {
        let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/starters/catch");
        if !starter.is_dir() || super::super::v2::engine().is_err() {
            eprintln!("skipped: no Studio engine 2.0 here");
            return;
        }
        let root = temp("starter");
        let (dir, store, out) = (root.join("project"), root.join("store"), root.join("out"));
        std::fs::create_dir_all(&dir).unwrap();
        copy(&starter, &dir);
        let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };

        let first = read(&build(&dir, &out, &store).unwrap());
        let files = first["files"].as_object().unwrap();
        assert!(files.contains_key("game.wasm") && files.contains_key("assets/game/main.luac"));
        let distinct: BTreeSet<&str> = files.values().map(|f| f["sha"].as_str().unwrap()).collect();
        assert_eq!(first["written"].as_array().unwrap().len(), distinct.len());
        assert_eq!(first["formats"], json!(["studio2", "mb2d3"]));
        assert_eq!(first["published"], true, "packed with the published engine");
        for f in files.values() {
            let sha = f["sha"].as_str().unwrap();
            let bytes = std::fs::read(store.join(sha)).unwrap();
            assert_eq!((sha_hex(&bytes), bytes.len() as u64), (sha.to_string(), f["size"].as_u64().unwrap()));
        }
        // The same files as a packed bundle, about its size.
        let (_, packed) = super::super::bundle_files(&dir, None).unwrap();
        assert_eq!(packed.keys().collect::<Vec<_>>(), files.keys().collect::<Vec<_>>());
        let zipped = mb_format::pack(&packed).unwrap().len() as f64;
        let size = first["size"].as_f64().unwrap();
        assert!((size - zipped).abs() / zipped < 0.05, "{size} vs {zipped}");

        // Unchanged: nothing new.
        let again = read(&build(&dir, &out, &store).unwrap());
        assert_eq!(again["written"], json!([]));
        assert_eq!(again["files"], first["files"]);

        // One line of Lua: the source and its bytecode, nothing else.
        let main = dir.join("game/main.lua");
        let src = std::fs::read_to_string(&main).unwrap().replace("local SPEED = 340", "local SPEED = 300");
        std::fs::write(&main, src).unwrap();
        let changed = read(&build(&dir, &out, &store).unwrap());
        assert_eq!(changed["written"].as_array().unwrap().len(), 2, "{}", changed["written"]);
        let differ: Vec<&String> = changed["files"].as_object().unwrap().iter().filter(|(p, f)| first["files"][p.as_str()] != **f).map(|(p, _)| p).collect();
        assert_eq!(differ, ["assets/game/main.lua", "assets/game/main.luac"]);
        // The old Lua's files left the store.
        assert!(!store.join(first["files"]["assets/game/main.lua"]["sha"].as_str().unwrap()).exists());
        // Sounds were rendered once, into the store's render cache.
        assert!(std::fs::read_dir(store.join(RENDER)).unwrap().count() >= 3);

        // A syntax error fails the build with its line.
        std::fs::write(&main, "function love.load(\n").unwrap();
        let e = build(&dir, &out, &store).unwrap_err();
        assert!(e.contains("game/main.lua:2"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
