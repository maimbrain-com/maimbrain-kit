//! Opening a bundle to run it, with the published Studio engines' metered
//! wasm kept between runs (docs/STUDIO-2.md §4.8 and §4.11, Decided 38–39).
//!
//! Opening a bundle validates it and loop-meters its `game.wasm`
//! (`mb_format::meter`), about 0.15–0.25 s for an engine-sized wasm. Every
//! Studio game carries one of a few published engines, so `mb play`, `mb
//! replay` and `mb serve` keep a published engine's metered wasm on disk, by
//! the metering version and the SHA-256 of its unmetered bytes
//! (`mb_format::meter::cache_key`), and only validate it on later opens. Any
//! other wasm is metered every time.
//!
//! The cache is `$MB_ENGINE_CACHE` (empty: none), else `maimbrain/engines`
//! in the user's cache directory (`mb_format::meter::DiskCache`, which the
//! app uses too).

use std::path::{Path, PathBuf};

pub use mb_format::meter::DiskCache;
use mb_format::Bundle;

/// Where the engines are kept, if anywhere.
pub fn cache_dir() -> Option<PathBuf> {
    match std::env::var_os("MB_ENGINE_CACHE") {
        Some(d) if !d.is_empty() => Some(PathBuf::from(d)),
        Some(_) => None,
        None => crate::user_cache_dir().map(|c| c.join("maimbrain").join("engines")),
    }
}

/// Opens bundle bytes, its engine's metered wasm from `dir` when it's kept there.
pub fn open_bytes(bytes: &[u8], dir: Option<&Path>) -> Result<Bundle, mb_format::Report> {
    match dir {
        Some(d) => Bundle::open_cached(bytes, &DiskCache(d.to_path_buf())),
        None => Bundle::open(bytes),
    }
}

/// Reads and opens a bundle to run it, with the engine cache.
pub fn open(path: &Path) -> Result<Bundle, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    open_bytes(&bytes, cache_dir().as_deref()).map_err(|r| format!("{} is invalid: {}", path.display(), r.errors.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mb_format::bundle::MeterCache;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mb-engines-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A Studio 2.0 starter packed with the published engine: its first open
    /// meters the engine and keeps it; the next uses what's kept (here a
    /// stand-in, to see it's read and not metered again).
    #[test]
    fn a_published_engine_is_metered_once_and_kept_by_version_and_sha() {
        let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/starters/catch");
        if !starter.is_dir() {
            eprintln!("skipped: no games/studio-engine2 here (the creator kit)");
            return;
        }
        let (_, files) = crate::studio::bundle_files(&starter, None).unwrap();
        let bytes = mb_format::pack(&files).unwrap();
        let dir = temp("published");
        std::fs::create_dir_all(&dir).unwrap();
        // An entry of another metering version goes when one is written.
        std::fs::write(dir.join("0-stale.wasm"), b"old").unwrap();

        let first = open_bytes(&bytes, Some(&dir)).unwrap();
        let key = mb_format::meter::cache_key(&files["game.wasm"]);
        assert!(key.starts_with(&format!("{}-", mb_format::meter::METER_VERSION)));
        let cache = DiskCache(dir.clone());
        let kept = cache.get(&key).expect("kept");
        assert_eq!(kept, first.runtime_wasm(), "the metered engine, kept");
        assert_eq!(kept, mb_format::meter::instrument(&files["game.wasm"]).unwrap());
        assert!(!dir.join("0-stale.wasm").exists(), "another version's entry is discarded");

        let stand_in = b"\0asm\x01\0\0\0 kept".to_vec();
        cache.put(&key, &stand_in);
        assert_eq!(open_bytes(&bytes, Some(&dir)).unwrap().runtime_wasm(), stand_in, "reused, not metered again");
        // Without the cache it's metered as always.
        assert_eq!(open_bytes(&bytes, None).unwrap().runtime_wasm(), first.runtime_wasm());
        // A file cut short, or changed, is metered again.
        let file = dir.join(format!("{key}.wasm"));
        let whole = std::fs::read(&file).unwrap();
        std::fs::write(&file, &whole[..whole.len() - 1]).unwrap();
        assert_eq!(open_bytes(&bytes, Some(&dir)).unwrap().runtime_wasm(), first.runtime_wasm());
        let mut changed = std::fs::read(&file).unwrap();
        changed[20] ^= 1;
        std::fs::write(&file, &changed).unwrap();
        assert_eq!(open_bytes(&bytes, Some(&dir)).unwrap().runtime_wasm(), first.runtime_wasm());
        std::fs::write(&file, b"junk").unwrap();
        assert_eq!(open_bytes(&bytes, Some(&dir)).unwrap().runtime_wasm(), first.runtime_wasm());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Threads of one process writing the same entry at once each finish
    /// whole, and what's left reads back whole.
    #[test]
    fn writers_at_once_dont_share_a_temporary_file() {
        let dir = temp("threads");
        let data: Vec<Vec<u8>> = (0..16u8).map(|i| [b"\0asm".as_slice(), &vec![i; 1 << 20]].concat()).collect();
        let torn = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for d in &data {
                let (dir, torn) = (dir.clone(), &torn);
                s.spawn(move || {
                    for _ in 0..8 {
                        let c = DiskCache(dir.clone());
                        c.put("1-same", d);
                        // Whatever is there now is someone's whole entry.
                        if std::fs::read(dir.join("1-same.wasm")).is_ok_and(|b| b.len() != d.len() + 32) {
                            torn.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        assert_eq!(torn.into_inner(), 0, "an entry was read half-written");
        let got = DiskCache(dir.clone()).get("1-same").expect("a whole entry");
        assert!(data.contains(&got));
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "no temporary files left: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A kit game's own wasm is metered every open and never kept.
    #[test]
    fn a_games_own_wasm_isnt_kept() {
        // The monorepo's fixture; the kit ships without fixtures/, so its copy skips.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/headless/dev.maimbrain.mistwood-0.1.0.mbx");
        let Ok(bytes) = std::fs::read(&path) else { return };
        let dir = temp("kit");
        open_bytes(&bytes, Some(&dir)).unwrap();
        assert!(std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_none()), "nothing kept for a kit game");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
