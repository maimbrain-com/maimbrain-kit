//! Studio bundles (SPEC "Studio bundles", docs/STUDIO.md): a game whose
//! `game.wasm` is the published Studio engine and whose project ships under
//! `assets/`, named by the manifest's `engine = "studio@<major>.<minor>"`.
//!
//! What's checked here, on top of every other bundle rule:
//! - `engine` is `studio@MAJOR.MINOR`;
//! - the project is there: `assets/studio.toml`, `assets/game/main.lua` and
//!   the generated `assets/files.txt`, every file it lists, and nothing it
//!   lists outside `assets/`;
//! - `game.wasm` is the engine build for the manifest's import set (with
//!   tilt or without, [`variant`]);
//! - whether `game.wasm` is a published engine build ([`PUBLISHED`]). A
//!   local build isn't, so `mb build` says so as a warning; the server
//!   refuses a Studio upload that isn't (`Report::studio.published`).

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::manifest::{Manifest, Sensor};

/// Bundle formats this validator opens beyond the plain one. An app sends
/// them with its feed requests, so the server only offers games it can play
/// (an older app's validator refuses a Studio bundle: `engine` is a field it
/// doesn't know).
pub const FORMATS: &[&str] = &["studio"];

/// The generated list of the project's files (paths under `assets/`).
pub const INDEX: &str = "assets/files.txt";

/// The engine's two builds: one per import set (SPEC "Studio bundles").
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    /// No sensors.
    Plain,
    /// Imports `mb_tilt` (`player.tilt`): the manifest declares `sensors = ["tilt"]`.
    Tilt,
}

impl Variant {
    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Plain => "plain",
            Variant::Tilt => "tilt",
        }
    }
}

/// A published engine build: the CDN serves it, and the server accepts
/// Studio bundles carrying exactly these bytes.
#[derive(Clone, Copy, Debug)]
pub struct Published {
    /// "1.0".
    pub version: &'static str,
    pub variant: Variant,
    /// SHA-256 of its `game.wasm`, hex.
    pub sha256: &'static str,
}

/// Every published engine build: `just studio-engine` makes them
/// (games/studio-engine/published/, which `mb` embeds and packs with) and
/// prints these lines. A build stays listed after it's replaced, so bundles
/// packed with it stay accepted.
pub const PUBLISHED: &[Published] = &[
    Published { version: "1.0", variant: Variant::Plain, sha256: "867329d1dea0dd04726bcfb906389f47ba9030dadb6285de8e04d81c15ef7ade" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "1b2d6711048ca91e37c5deccfb4f2952d019feef21c9d824604ad12cc8ad143c" },
    // render reads the round's score, lives and level from update's copy, not the VM.
    Published { version: "1.0", variant: Variant::Plain, sha256: "8575418f35be1bb6031def1dc1a84823d1d7f7f90260432dcf06634b03b5ab20" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "cae3f50d0b1491b7cb8e2f9dbfecc90a222cd32d40981a5977628d83c13144e1" },
    // The sandbox keeps the saved-data load(key).
    Published { version: "1.0", variant: Variant::Plain, sha256: "9345f2d32612a0c009fc7e14d2da6ad58fae38a4d6a0b077e5e200c3683c2267" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "b526a56348076b965ee5f6db5000d4bf5d36adc177b0b4c99f7bc7cb2776ef6f" },
];

/// What the report says about a Studio bundle.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Info {
    /// "1.0".
    pub engine: String,
    pub variant: Variant,
    /// `game.wasm` is a published engine build of that version and variant.
    pub published: bool,
}

/// `studio@1.0` → (1, 0).
pub fn parse_engine(s: &str) -> Option<(u32, u32)> {
    let v = s.strip_prefix("studio@")?;
    let (a, b) = v.split_once('.')?;
    let num = |x: &str| (!x.is_empty() && x.len() <= 4 && x.bytes().all(|c| c.is_ascii_digit())).then(|| x.parse().ok()).flatten();
    Some((num(a)?, num(b)?))
}

/// The engine build a manifest needs.
pub fn variant(m: &Manifest) -> Variant {
    if m.sensors.contains(&Sensor::Tilt) { Variant::Tilt } else { Variant::Plain }
}

/// Whether `wasm` is the published build of `version` for `variant`, in `table`.
pub fn is_published(table: &[Published], version: &str, variant: Variant, wasm: &[u8]) -> bool {
    let sha = crate::bundle::hex(&Sha256::digest(wasm));
    table.iter().any(|p| p.version == version && p.variant == variant && p.sha256 == sha)
}

/// Checks a Studio bundle's files (the manifest names an engine).
pub fn check(m: &Manifest, files: &BTreeMap<String, Vec<u8>>, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> Option<Info> {
    let engine = m.engine.as_deref()?;
    let Some((major, minor)) = parse_engine(engine) else {
        errors.push(format!("manifest: engine = {engine:?} isn't a Studio engine (\"studio@1.0\")"));
        return None;
    };
    for required in ["assets/studio.toml", "assets/game/main.lua", "assets/files.txt"] {
        if !files.contains_key(required) {
            errors.push(format!("studio: {required} is missing (build Studio projects with `mb build`)"));
        }
    }
    if let Some(list) = files.get("assets/files.txt") {
        let list = String::from_utf8_lossy(list);
        for line in list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            if line.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
                errors.push(format!("studio: assets/files.txt lists {line:?}, which isn't a project path"));
            } else if !files.contains_key(&format!("assets/{line}")) {
                errors.push(format!("studio: assets/files.txt lists {line}, but assets/{line} isn't in the bundle"));
            }
        }
    }
    let variant = variant(m);
    let version = format!("{major}.{minor}");
    let published = files.get("game.wasm").is_some_and(|w| is_published(PUBLISHED, &version, variant, w));
    if !published {
        warnings.push(format!(
            "game.wasm isn't a published Studio engine {version} ({} build): fine for previews and tests, but maimbrain.com accepts only published engines",
            variant.as_str()
        ));
    }
    Some(Info { engine: version, variant, published })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names() {
        assert_eq!(parse_engine("studio@1.0"), Some((1, 0)));
        assert_eq!(parse_engine("studio@12.3"), Some((12, 3)));
        for bad in ["studio@1", "studio@1.0.0", "studio@", "lua@1.0", "studio@1.x", "studio@.1"] {
            assert_eq!(parse_engine(bad), None, "{bad}");
        }
    }

    #[test]
    fn published_builds_are_matched_by_sha_version_and_variant() {
        let wasm = b"\0asm engine";
        let sha = crate::bundle::hex(&Sha256::digest(wasm));
        let sha: &'static str = Box::leak(sha.into_boxed_str());
        let table = [Published { version: "1.0", variant: Variant::Plain, sha256: sha }];
        assert!(is_published(&table, "1.0", Variant::Plain, wasm));
        assert!(!is_published(&table, "1.1", Variant::Plain, wasm));
        assert!(!is_published(&table, "1.0", Variant::Tilt, wasm));
        assert!(!is_published(&table, "1.0", Variant::Plain, b"\0asm other"));
    }
}
