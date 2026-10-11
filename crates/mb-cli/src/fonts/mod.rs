//! Game fonts: the curated library (fonts/library.toml, OFL and Apache fonts
//! from github.com/google/fonts at one pinned commit), `mb font add` (download,
//! verify, bake to assets/fonts/<name>.mbf, record in fonts.toml), the
//! re-bake `mb build` does from fonts.toml, and `mb fonts` (list, specimen).

pub mod bake;
mod specimen;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bumped when baking changes, so `mb build` re-bakes old atlases.
const BAKER: u32 = 1;

pub const LIBRARY_TOML: &str = include_str!("../../fonts/library.toml");
/// The SDK's identities (`Theme::preset`), read for their font names so
/// `mb font add --identity` and `mb new` bake what each was designed with.
const IDENTITY_RS: &str = include_str!("../../../../sdk/maimbrain/src/ui/identity.rs");

/// The identities and their (title, body, number) font names, from the SDK source.
pub fn identities() -> Vec<(String, [String; 3])> {
    let mut out = Vec::new();
    let mut rest = IDENTITY_RS;
    while let Some(at) = rest.find("=> theme(") {
        let line_start = rest[..at].rfind('\n').map_or(0, |i| i + 1);
        let head = rest[line_start..at].trim();
        let name = if head == "_" { "riso".to_string() } else { head.trim_matches('"').to_string() };
        rest = &rest[at + 9..];
        let Some(n) = rest.find("names(\"") else { break };
        let args = &rest[n + 6..];
        let Some(end) = args.find(')') else { break };
        let parts: Vec<String> = args[..end].split(',').map(|p| p.trim().trim_matches('"').to_string()).collect();
        if let [a, b, c] = &parts[..] {
            out.push((name, [a.clone(), b.clone(), c.clone()]));
        }
    }
    out
}

/// A font name as `mb font add` writes it (`id`, or `id-weight`) → (library id, weight).
pub fn split_font_name<'a>(lib: &'a Library, name: &str) -> Option<(&'a Entry, Option<u32>)> {
    if let Some(e) = lib.fonts.iter().find(|e| e.id == name) {
        return Some((e, None));
    }
    let (base, w) = name.rsplit_once('-')?;
    let w: u32 = w.parse().ok()?;
    lib.fonts.iter().find(|e| e.id == base).map(|e| (e, Some(w)))
}

/// `mb font add <game> --identity <name>`: bakes the fonts an identity names.
pub fn add_identity(game: &Path, identity: &str) -> Result<(), String> {
    let all = identities();
    let Some((_, fonts)) = all.iter().find(|(n, _)| n == identity) else {
        let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
        return Err(format!("no identity {identity:?} (identities: {})", names.join(", ")));
    };
    let lib = library()?;
    let mut done: Vec<&str> = Vec::new();
    for f in fonts.iter().filter(|f| !f.is_empty()) {
        if done.contains(&f.as_str()) {
            continue;
        }
        done.push(f);
        let (e, weight) = split_font_name(&lib, f).ok_or_else(|| format!("identity {identity}: {f} isn't a library font"))?;
        add(game, &e.id, weight, None, None, Some(f.clone()), false, false, None)?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct Library {
    pub source: Source,
    #[serde(default, rename = "font")]
    pub fonts: Vec<Entry>,
}

#[derive(Deserialize)]
pub struct Source {
    pub repo: String,
    pub commit: String,
    pub base: String,
}

#[derive(Deserialize, Clone)]
pub struct Entry {
    pub id: String,
    pub family: String,
    pub category: String,
    pub license: String,
    pub license_path: String,
    pub license_sha256: String,
    #[serde(default)]
    pub designer: String,
    #[serde(default)]
    pub copyright: String,
    #[serde(default)]
    pub reserved_name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mood: Vec<String>,
    #[serde(default)]
    pub era: String,
    #[serde(default)]
    pub best_for: Vec<String>,
    #[serde(default)]
    pub pairs_with: Vec<String>,
    #[serde(default)]
    pub pixel_em: Option<u32>,
    #[serde(default)]
    pub pixel_em_estimated: Option<bool>,
    #[serde(default)]
    pub caps_only: Option<bool>,
    /// Pixel fonts bake 1-bit on their grid unless this is false.
    #[serde(default)]
    pub bitmap: Option<bool>,
    pub files: Vec<FileEntry>,
}

#[derive(Deserialize, Clone)]
pub struct FileEntry {
    pub weight: u32,
    pub path: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub axes: BTreeMap<String, [f32; 2]>,
}

impl Entry {
    /// The file to bake for a weight, and the `wght` to set if it's variable.
    pub fn file_for(&self, weight: Option<u32>) -> Result<(&FileEntry, Option<f32>), String> {
        let want = weight.unwrap_or(400);
        // A variable file covering the weight wins; else the nearest static weight.
        if let Some(f) = self.files.iter().find(|f| f.axes.get("wght").is_some_and(|r| (r[0]..=r[1]).contains(&(want as f32)))) {
            return Ok((f, Some(want as f32)));
        }
        let f = self.files.iter().min_by_key(|f| (f.weight as i64 - want as i64).abs()).ok_or("no files")?;
        if weight.is_some() && f.weight != want && f.axes.is_empty() {
            let have: Vec<String> = self.files.iter().map(|f| f.weight.to_string()).collect();
            return Err(format!("{} has no weight {want} (available: {})", self.family, have.join(", ")));
        }
        Ok((f, None))
    }

    pub fn weights(&self) -> String {
        self.files
            .iter()
            .map(|f| match f.axes.get("wght") {
                Some(r) => format!("{}–{}", r[0], r[1]),
                None => f.weight.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn library() -> Result<Library, String> {
    toml::from_str(LIBRARY_TOML).map_err(|e| format!("fonts/library.toml: {e}"))
}

/// Where downloaded fonts are kept: `MB_FONT_CACHE`, else the platform cache
/// directory (`~/.cache/maimbrain/fonts`, `%LOCALAPPDATA%\maimbrain\fonts`).
pub fn cache_dir() -> Result<PathBuf, String> {
    if let Some(d) = std::env::var_os("MB_FONT_CACHE") {
        return Ok(PathBuf::from(d));
    }
    let base = crate::user_cache_dir().ok_or("can't find a cache directory for fonts (set MB_FONT_CACHE)")?;
    Ok(base.join("maimbrain").join("fonts"))
}

fn sha256_hex(b: &[u8]) -> String {
    mb_format::bundle::hex(&Sha256::digest(b))
}

/// A file from the library's pinned source, from the cache or downloaded,
/// checked against its sha256 either way.
pub fn fetch(lib: &Library, path: &str, sha256: &str, ext: &str) -> Result<Vec<u8>, String> {
    let dir = cache_dir()?;
    let cached = dir.join(format!("{sha256}.{ext}"));
    if let Ok(b) = std::fs::read(&cached)
        && sha256_hex(&b) == sha256
    {
        return Ok(b);
    }
    let url = format!("{}{path}", lib.source.base);
    eprintln!("downloading {url}");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).build().into();
    let mut resp = agent.get(&url).call().map_err(|e| format!("{url}: {e}"))?;
    let b = resp.body_mut().with_config().limit(32 << 20).read_to_vec().map_err(|e| format!("{url}: {e}"))?;
    let got = sha256_hex(&b);
    if got != sha256 {
        return Err(format!("{url}: sha256 {got} doesn't match the library's {sha256}; not using it"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _ = std::fs::write(&cached, &b);
    Ok(b)
}

/// One font a game bakes (`fonts.toml`, next to manifest.toml).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recipe {
    /// assets/fonts/<name>.mbf
    pub name: String,
    /// A library id (`mb fonts`), or a .ttf/.otf path relative to the game directory.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// `latin1` (default), `ascii`, `caps`, `digits`, literal characters, joined with `+`.
    #[serde(default = "default_chars")]
    pub chars: String,
    /// Pixels per em in the atlas (SDF; 16–64, default 32). Bigger keeps corners sharper at large sizes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub em: Option<u32>,
    /// Bake a 1-bit atlas on the font's pixel grid (library pixel fonts do this by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitmap: Option<bool>,
    /// For a font file of your own: its license file (relative to the game directory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

fn default_chars() -> String {
    "latin1".into()
}

#[derive(Serialize, Deserialize, Default)]
struct FontsToml {
    #[serde(default, rename = "font")]
    fonts: Vec<Recipe>,
}

const FONTS_TOML_HEADER: &str = "# Fonts this game bakes into assets/fonts/ (written by `mb font add`; `mb build` re-bakes\n# when an entry changes). Load one with Font::asset(\"assets/fonts/<name>.mbf\", fallback).\n\n";

pub fn read_fonts_toml(game: &Path) -> Result<Vec<Recipe>, String> {
    let path = game.join("fonts.toml");
    match std::fs::read_to_string(&path) {
        Ok(t) => toml::from_str::<FontsToml>(&t).map(|f| f.fonts).map_err(|e| format!("{}: {e}", path.display())),
        Err(_) => Ok(vec![]),
    }
}

fn write_fonts_toml(game: &Path, fonts: &[Recipe]) -> Result<(), String> {
    let body = toml::to_string_pretty(&FontsToml { fonts: fonts.to_vec() }).map_err(|e| e.to_string())?;
    let path = game.join("fonts.toml");
    std::fs::write(&path, format!("{FONTS_TOML_HEADER}{body}")).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a recipe resolves to: the font bytes, license text, and descriptive metadata.
struct Resolved {
    data: Vec<u8>,
    license_text: Option<String>,
    family: String,
    license: String,
    copyright: String,
    source: String,
    weight: Option<f32>,
    pixel_em: Option<u32>,
}

fn resolve(game: &Path, r: &Recipe, lib: &Library) -> Result<Resolved, String> {
    if let Some(e) = lib.fonts.iter().find(|e| e.id == r.source) {
        let (file, wght) = e.file_for(r.weight)?;
        let ext = file.path.rsplit('.').next().unwrap_or("ttf");
        let data = fetch(lib, &file.path, &file.sha256, ext)?;
        let license_text = String::from_utf8_lossy(&fetch(lib, &e.license_path, &e.license_sha256, "txt")?).into_owned();
        return Ok(Resolved {
            data,
            license_text: Some(license_text),
            family: e.family.clone(),
            license: e.license.clone(),
            copyright: e.copyright.clone(),
            source: format!("{}@{}/{}", lib.source.repo, &lib.source.commit[..12.min(lib.source.commit.len())], file.path),
            weight: wght.or(Some(file.weight as f32)),
            pixel_em: e.pixel_em.filter(|_| e.bitmap != Some(false)),
        });
    }
    let path = game.join(&r.source);
    if !path.is_file() {
        return Err(format!("{} is neither a library font (`mb fonts` lists them) nor a font file", r.source));
    }
    let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let license_text = r.license.as_ref().map(|l| std::fs::read_to_string(game.join(l)).map_err(|e| format!("{l}: {e}"))).transpose()?;
    let family = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let license = match &license_text {
        Some(t) if t.contains("SIL OPEN FONT LICENSE") => "OFL-1.1",
        Some(t) if t.contains("Apache License") => "Apache-2.0",
        Some(_) => "see license file",
        None => "unknown",
    };
    Ok(Resolved {
        data,
        license_text,
        family,
        license: license.into(),
        copyright: String::new(),
        source: r.source.clone(),
        weight: r.weight.map(|w| w as f32),
        pixel_em: None,
    })
}

/// Hash of everything that decides a bake's output.
fn recipe_key(r: &Recipe, font_sha: &str) -> String {
    let s = format!("{BAKER}|{font_sha}|{:?}|{}|{:?}|{:?}", r.weight, r.chars, r.em, r.bitmap);
    sha256_hex(s.as_bytes())[..16].to_string()
}

/// Bakes one recipe into <game>/assets/fonts/<name>.mbf (and its license file).
fn bake_recipe(game: &Path, r: &Recipe, lib: &Library) -> Result<String, String> {
    check_name(&r.name)?;
    let res = resolve(game, r, lib)?;
    let font_sha = sha256_hex(&res.data);
    let key = recipe_key(r, &font_sha);
    let bitmap = r.bitmap.unwrap_or(res.pixel_em.is_some());
    let em = if bitmap { r.em.or(res.pixel_em).unwrap_or(8) as f32 } else { r.em.unwrap_or(32).clamp(16, 64) as f32 };
    let meta = [
        format!("family={}", res.family),
        res.weight.map(|w| format!("weight={w}")).unwrap_or_default(),
        format!("license={}", res.license),
        if res.copyright.is_empty() { String::new() } else { format!("copyright={}", res.copyright) },
        format!("source={}", res.source),
        format!("sha256={font_sha}"),
        format!("chars={}", r.chars),
        format!("recipe={key}"),
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    let opts = bake::Options { em, bitmap, weight: res.weight, chars: bake::charset(&r.chars)?, meta };
    let font = bake::bake(&res.data, &opts).map_err(|e| format!("{}: {e}", r.source))?;
    let bytes = font.encode();
    mb_format::font::MbFont::parse(&bytes).map_err(|e| format!("baked {}: {e}", r.name))?;
    let dir = game.join("assets").join("fonts");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = dir.join(format!("{}.mbf", r.name));
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    let lic = dir.join(format!("{}-LICENSE.txt", r.name));
    match &res.license_text {
        Some(t) => std::fs::write(&lic, t).map_err(|e| format!("{}: {e}", lic.display()))?,
        None => eprintln!("warning: {} has no license file; pass --license with the font's license before publishing", r.source),
    }
    Ok(format!(
        "{}: {} {}{}, {} glyphs, {} kerning pairs, atlas {}×{}, {:.1} KiB",
        out.strip_prefix(game).unwrap_or(&out).display(),
        res.family,
        res.weight.map(|w| format!("{w} ")).unwrap_or_default(),
        if bitmap { format!("bitmap {em} px/em") } else { format!("SDF {em} px/em") },
        font.glyphs.len(),
        font.kerns.len(),
        font.atlas[0],
        font.atlas[1],
        bytes.len() as f32 / 1024.0
    ))
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 48 || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(format!("font name {name:?}: use 1–48 lowercase letters, digits, - or _"));
    }
    Ok(())
}

/// `mb font add`.
pub fn add(game: &Path, font: &str, weight: Option<u32>, chars: Option<String>, em: Option<u32>, name: Option<String>, bitmap: bool, sdf: bool, license: Option<String>) -> Result<(), String> {
    if !game.join("manifest.toml").is_file() {
        return Err(format!("{} is not a game directory (no manifest.toml)", game.display()));
    }
    let lib = library()?;
    let in_lib = lib.fonts.iter().find(|e| e.id == font);
    // A file path is stored relative to the game directory.
    let source = match in_lib {
        Some(e) => e.id.clone(),
        None => {
            let p = Path::new(font);
            if !p.is_file() {
                let close: Vec<&str> = lib.fonts.iter().filter(|e| e.id.contains(font) || e.family.to_lowercase().contains(&font.to_lowercase())).map(|e| e.id.as_str()).take(6).collect();
                return Err(format!("no library font {font:?}{} (see `mb fonts`), and no such file", if close.is_empty() { String::new() } else { format!("; did you mean {}?", close.join(", ")) }));
            }
            relative_to(p, game)?
        }
    };
    let base = in_lib.map(|e| e.id.clone()).unwrap_or_else(|| Path::new(font).file_stem().map(|s| s.to_string_lossy().to_lowercase().replace([' ', '_', '.'], "-")).unwrap_or_else(|| "font".into()));
    let name = name.unwrap_or_else(|| match weight {
        Some(w) if w != 400 => format!("{base}-{w}"),
        _ => base,
    });
    let default_chars = match in_lib {
        Some(e) if e.caps_only == Some(true) => "caps".to_string(),
        _ => default_chars(),
    };
    let recipe = Recipe {
        name: name.clone(),
        source,
        weight,
        chars: chars.unwrap_or(default_chars),
        em,
        bitmap: if bitmap { Some(true) } else if sdf { Some(false) } else { None },
        license: license.map(|l| relative_to(Path::new(&l), game)).transpose()?,
    };
    let line = bake_recipe(game, &recipe, &lib)?;
    let mut fonts = read_fonts_toml(game)?;
    match fonts.iter_mut().find(|f| f.name == name) {
        Some(f) => *f = recipe,
        None => fonts.push(recipe),
    }
    write_fonts_toml(game, &fonts)?;
    println!("{line}");
    if let Some(e) = in_lib {
        println!("  {} · {} · {}", e.family, e.category, e.description);
        if let Some(rfn) = &e.reserved_name {
            println!("  license: {} (Reserved Font Name \"{rfn}\": fine to use and subset as-is; don't publish modified versions under that name)", e.license);
        } else {
            println!("  license: {}", e.license);
        }
    }
    let path = format!("assets/fonts/{name}.mbf");
    println!("  use it:  let font = Font::asset(\"{path}\", Font::SansBold);   (manifest: stdlib = {{ mb2d = 2 }})");
    Ok(())
}

/// `mb font remove`: takes fonts out of a game: their fonts.toml entries,
/// `assets/fonts/<name>.mbf` and `<name>-LICENSE.txt`. `all` removes every
/// baked font (to start over from `mb new`'s identity with the game's own).
pub fn remove(game: &Path, names: &[String], all: bool) -> Result<(), String> {
    if !game.join("manifest.toml").is_file() {
        return Err(format!("{} is not a game directory (no manifest.toml)", game.display()));
    }
    let mut fonts = read_fonts_toml(game)?;
    let dir = game.join("assets").join("fonts");
    let baked: Vec<String> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(Result::ok).filter_map(|e| e.path().file_name()?.to_str()?.strip_suffix(".mbf").map(str::to_string)).collect())
        .unwrap_or_default();
    let mut targets: Vec<String> = if all { fonts.iter().map(|f| f.name.clone()).chain(baked.iter().cloned()).collect() } else { names.to_vec() };
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        println!("{} has no fonts to remove", game.display());
        return Ok(());
    }
    // Check every name before deleting anything.
    for n in &targets {
        check_name(n)?;
        if !fonts.iter().any(|f| &f.name == n) && !baked.contains(n) && !dir.join(format!("{n}-LICENSE.txt")).exists() {
            let mut have: Vec<&str> = fonts.iter().map(|f| f.name.as_str()).chain(baked.iter().map(String::as_str)).collect();
            have.sort();
            have.dedup();
            let have = if have.is_empty() { "none".to_string() } else { have.join(", ") };
            return Err(format!("{} has no font {n:?} (its fonts: {have})", game.display()));
        }
    }
    for n in &targets {
        for f in [format!("{n}.mbf"), format!("{n}-LICENSE.txt")] {
            let p = dir.join(&f);
            if p.exists() {
                std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
        fonts.retain(|f| &f.name != n);
        println!("removed {n} (assets/fonts/{n}.mbf, its license and its fonts.toml entry)");
    }
    if fonts.is_empty() {
        let p = game.join("fonts.toml");
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    } else {
        write_fonts_toml(game, &fonts)?;
    }
    if dir.read_dir().is_ok_and(|mut d| d.next().is_none()) {
        let _ = std::fs::remove_dir(&dir);
    }
    // Drawing with a removed font falls back to the built-in one it names;
    // say where the game still asks for one.
    let src = crate::rust_sources(&game.join("src"));
    let used: Vec<&String> = targets.iter().filter(|n| src.contains(&format!("assets/fonts/{n}.mbf"))).collect();
    if !used.is_empty() {
        let used: Vec<&str> = used.iter().map(|s| s.as_str()).collect();
        eprintln!("note: src/ still loads {} (Font::asset): it draws in the fallback font until you change it or `mb font add` one by that name", used.join(", "));
    }
    if src.contains(".load_fonts()") {
        eprintln!("note: Theme::load_fonts() loads the theme's own fonts by name (Theme.fonts); any it can't find draw in the built-in fonts");
    }
    Ok(())
}

fn relative_to(p: &Path, game: &Path) -> Result<String, String> {
    let abs = std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let g = std::fs::canonicalize(game).map_err(|e| format!("{}: {e}", game.display()))?;
    match abs.strip_prefix(&g) {
        Ok(r) => Ok(r.to_string_lossy().replace('\\', "/")),
        Err(_) => Err(format!("{} must be inside the game directory (copy it there, e.g. into {}/fonts-src/)", p.display(), game.display())),
    }
}

/// `mb build`: re-bakes fonts.toml entries whose .mbf is missing or out of date.
pub fn rebake(game: &Path) -> Result<(), String> {
    let fonts = read_fonts_toml(game)?;
    if fonts.is_empty() {
        return Ok(());
    }
    let mut lib: Option<Library> = None;
    for r in &fonts {
        let out = game.join("assets").join("fonts").join(format!("{}.mbf", r.name));
        let current = std::fs::read(&out).ok().and_then(|b| mb_format::font::MbFont::parse(&b).ok());
        if let Some(f) = &current
            && let Some(sha) = f.meta_value("sha256")
            && f.meta_value("recipe") == Some(recipe_key(r, sha).as_str())
        {
            continue;
        }
        let lib = match &mut lib {
            Some(l) => l,
            None => lib.insert(library()?),
        };
        eprintln!("baking font {}", bake_recipe(game, r, lib)?);
    }
    Ok(())
}

/// Whether a game uses fonts of its own (for `mb build`'s sameness nudge).
pub fn game_has_fonts(game: &Path) -> bool {
    std::fs::read_dir(game.join("assets").join("fonts")).is_ok_and(|d| d.filter_map(Result::ok).any(|e| e.path().extension().is_some_and(|x| x == "mbf")))
}

/// `mb fonts`.
pub fn list(category: Option<&str>, query: Option<&str>, json: bool) -> Result<(), String> {
    let lib = library()?;
    let matches = |e: &Entry| {
        category.is_none_or(|c| e.category == c)
            && query.is_none_or(|q| {
                let q = q.to_lowercase();
                [&e.id, &e.family, &e.category, &e.description, &e.era].iter().any(|s| s.to_lowercase().contains(&q))
                    || e.mood.iter().chain(&e.best_for).any(|m| m.to_lowercase().contains(&q))
            })
    };
    let shown: Vec<&Entry> = lib.fonts.iter().filter(|e| matches(e)).collect();
    if json {
        let v: Vec<_> = shown
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id, "family": e.family, "category": e.category, "description": e.description,
                    "mood": e.mood, "era": e.era, "best_for": e.best_for, "pairs_with": e.pairs_with,
                    "weights": e.weights(), "pixel_em": e.pixel_em, "pixel_em_estimated": e.pixel_em_estimated.unwrap_or(false), "designer": e.designer, "bytes": e.files.iter().map(|f| f.size).sum::<u64>(), "caps_only": e.caps_only.unwrap_or(false), "license": e.license,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let mut last = "";
    for e in &shown {
        if e.category != last {
            println!("\n{}", e.category.to_uppercase());
            last = &e.category;
        }
        println!("  {:<24} {} ({})", e.id, e.description, e.weights());
        println!("  {:<24} mood: {} · era: {} · for: {} · pairs: {}", "", e.mood.join(", "), e.era, e.best_for.join(", "), e.pairs_with.join(", "));
    }
    println!(
        "\n{} of {} fonts ({}, {} @ {}). `mb font add <game> <id>` bakes one; `mb fonts --preview` draws them.",
        shown.len(),
        lib.fonts.len(),
        "OFL/Apache",
        lib.source.repo,
        &lib.source.commit[..12.min(lib.source.commit.len())]
    );
    Ok(())
}

/// `mb fonts --preview`: a specimen sheet PNG.
pub fn preview(category: Option<&str>, query: Option<&str>, out: &Path) -> Result<(), String> {
    let lib = library()?;
    let q = query.map(str::to_lowercase);
    let entries: Vec<&Entry> = lib
        .fonts
        .iter()
        .filter(|e| category.is_none_or(|c| e.category == c))
        .filter(|e| q.as_ref().is_none_or(|q| e.id.contains(q.as_str()) || e.mood.iter().any(|m| m.contains(q.as_str()))))
        .collect();
    if entries.is_empty() {
        return Err("no fonts match".into());
    }
    let mut fonts = Vec::new();
    for e in &entries {
        let (file, wght) = e.file_for(None)?;
        let ext = file.path.rsplit('.').next().unwrap_or("ttf");
        fonts.push((*e, fetch(&lib, &file.path, &file.sha256, ext)?, wght));
    }
    // Labels in a plain sans from the library.
    let label = lib
        .fonts
        .iter()
        .find(|e| e.category == "humanist-sans")
        .or_else(|| lib.fonts.iter().find(|e| e.category.ends_with("sans")))
        .ok_or("the library has no sans for labels")?;
    let (lf, _) = label.file_for(None)?;
    let label_data = fetch(&lib, &lf.path, &lf.sha256, lf.path.rsplit('.').next().unwrap_or("ttf"))?;
    let png = specimen::sheet(&fonts, &label_data)?;
    std::fs::write(out, png).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("wrote {} ({} fonts)", out.display(), fonts.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mb font remove` takes a font's files and entry out and leaves the rest;
    /// `--all` empties the game of fonts; an unknown name changes nothing.
    #[test]
    fn remove_takes_fonts_out() {
        let game = std::env::temp_dir().join(format!("mb-font-remove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        let dir = game.join("assets").join("fonts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(game.join("manifest.toml"), "").unwrap();
        let recipe = |name: &str| Recipe { name: name.into(), source: name.into(), weight: None, chars: "latin1".into(), em: None, bitmap: None, license: None };
        write_fonts_toml(&game, &[recipe("rye"), recipe("space-mono")]).unwrap();
        for n in ["rye", "space-mono", "stray"] {
            std::fs::write(dir.join(format!("{n}.mbf")), b"x").unwrap();
            std::fs::write(dir.join(format!("{n}-LICENSE.txt")), b"x").unwrap();
        }
        let err = remove(&game, &["nope".into()], false).unwrap_err();
        assert!(err.contains("no font \"nope\"") && err.contains("rye, space-mono, stray"), "{err}");
        assert!(dir.join("rye.mbf").exists(), "nothing removed on an unknown name");
        remove(&game, &["rye".into()], false).unwrap();
        assert!(!dir.join("rye.mbf").exists() && !dir.join("rye-LICENSE.txt").exists());
        assert_eq!(read_fonts_toml(&game).unwrap(), vec![recipe("space-mono")]);
        // A baked file without an entry (copied in by hand) goes too.
        remove(&game, &[], true).unwrap();
        assert!(!dir.exists() && !game.join("fonts.toml").exists());
        std::fs::remove_dir_all(&game).unwrap();
    }

    /// The shipped library parses, ids are unique, pairings resolve, and
    /// every font has an OFL or Apache license and a pinned file.
    #[test]
    fn library_is_consistent() {
        let lib = library().unwrap();
        assert!(lib.fonts.len() >= 60, "{} fonts", lib.fonts.len());
        assert!(lib.source.base.contains(&lib.source.commit));
        let mut ids = std::collections::BTreeSet::new();
        for e in &lib.fonts {
            assert!(ids.insert(e.id.as_str()), "duplicate id {}", e.id);
            assert!(matches!(e.license.as_str(), "OFL-1.1" | "Apache-2.0"), "{}: {}", e.id, e.license);
            assert!(!e.files.is_empty() && e.files.iter().all(|f| f.sha256.len() == 64), "{}", e.id);
            assert!(!e.description.is_empty() && !e.mood.is_empty(), "{}", e.id);
        }
        for e in &lib.fonts {
            for p in &e.pairs_with {
                assert!(ids.contains(p.as_str()), "{} pairs with unknown {p}", e.id);
            }
        }
    }

    /// Every identity's fonts are library fonts, named the way `mb font add` names them.
    #[test]
    fn identities_name_library_fonts() {
        let lib = library().unwrap();
        let ids = identities();
        assert_eq!(ids.len(), 17, "{:?}", ids.iter().map(|i| &i.0).collect::<Vec<_>>());
        for (name, fonts) in &ids {
            for f in fonts.iter().filter(|f| !f.is_empty()) {
                let (e, w) = split_font_name(&lib, f).unwrap_or_else(|| panic!("{name}: {f} isn't in the library"));
                e.file_for(w).unwrap_or_else(|err| panic!("{name}: {f}: {err}"));
            }
        }
    }

    #[test]
    fn recipes_change_their_key() {
        let r = Recipe { name: "t".into(), source: "bungee".into(), weight: None, chars: "ascii".into(), em: None, bitmap: None, license: None };
        let k = recipe_key(&r, "abc");
        assert_ne!(k, recipe_key(&Recipe { chars: "caps".into(), ..r.clone() }, "abc"));
        assert_ne!(k, recipe_key(&r, "abd"));
    }
}
