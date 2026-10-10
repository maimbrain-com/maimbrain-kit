//! What `mb build` (and `mb pack`) says about a bundle before the game runs:
//! the mb3d texture memory its images and models would take if they were all
//! loaded at once, against the 96 MB budget and 64 textures (SPEC §5.4), so
//! going over shows up here rather than as a sky that never appears; and,
//! when the bundle is near or over a size limit (10 MiB compressed, a 3 MiB
//! startup set, 4 MiB of wasm; SPEC §1), the biggest assets and the ones no
//! string in the game's source names, which are probably retired.

use std::collections::BTreeMap;

/// mb3d's texture limits (crates/mb-host/src/mb3d/mod.rs).
pub const TEXTURE_BUDGET: u64 = 96 << 20;
pub const MAX_TEXTURES: usize = 64;
pub const MAX_TEXTURE_SIZE: u32 = 2048;

const MIB: f64 = (1u64 << 20) as f64;

/// GPU bytes of a `w`×`h` RGBA8 texture with its mipmaps (mb3d's `gpu_bytes`).
pub fn gpu_bytes([w, h]: [u32; 2]) -> u64 {
    w as u64 * h as u64 * 4 * 4 / 3
}

/// Width and height from a PNG or JPEG header, without decoding.
pub fn image_size(b: &[u8]) -> Option<[u32; 2]> {
    if b.len() >= 24 && &b[..8] == b"\x89PNG\r\n\x1a\n" && &b[12..16] == b"IHDR" {
        let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
        return Some([be(16), be(20)]);
    }
    if b.len() < 4 || b[0] != 0xff || b[1] != 0xd8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xff {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xff {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            if i + 9 > b.len() {
                return None;
            }
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some([w, h]);
        }
        i += 2 + len;
    }
    None
}

/// One asset's share of texture memory.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureUse {
    pub path: String,
    /// Textures it makes (1 for an image, its images for a model).
    pub textures: usize,
    pub bytes: u64,
}

/// What every image and model in `files` (bundle paths → bytes) would take
/// as mb3d textures, biggest first.
pub fn texture_uses(files: &BTreeMap<String, Vec<u8>>) -> Vec<TextureUse> {
    let mut out = Vec::new();
    for (path, b) in files.iter().filter(|(p, _)| p.starts_with("assets/")) {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let sizes: Vec<[u32; 2]> = match ext.as_str() {
            "png" | "jpg" | "jpeg" => image_size(b).into_iter().collect(),
            "glb" => match crate::generate::glb::read(b) {
                Ok(sc) => sc.images.iter().filter_map(|i| image_size(&i.bytes)).collect(),
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        };
        let sizes: Vec<[u32; 2]> = sizes.into_iter().filter(|s| s[0] <= MAX_TEXTURE_SIZE && s[1] <= MAX_TEXTURE_SIZE).collect();
        if !sizes.is_empty() {
            out.push(TextureUse { path: path.clone(), textures: sizes.len(), bytes: sizes.into_iter().map(gpu_bytes).sum() });
        }
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    out
}

/// What a game loads together, from `art/budget.toml`, so the texture
/// estimate is the biggest scene's rather than every asset at once:
///
/// ```toml
/// [load]
/// always = ["p_*.png", "mech_hauler.glb"]      # loaded the whole game
/// harbor = ["sky_harbor.jpg", "bd_harbor_*.png", "floor_harbor.png"]
/// city = ["sky_city.jpg", "bd_city_*.png", "floor_city.png"]
/// ```
///
/// Each group is a list of files under `assets/` (`*` matches anything).
/// `always` is in every group, and so is an image or model no group lists
/// (it might be loaded at any time).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LoadGroups {
    pub always: Vec<String>,
    pub groups: Vec<(String, Vec<String>)>,
}

impl LoadGroups {
    pub fn parse(text: &str) -> Result<LoadGroups, String> {
        let v: toml::Table = toml::from_str(text).map_err(|e| format!("art/budget.toml: {e}"))?;
        let Some(load) = v.get("load").and_then(|l| l.as_table()) else {
            return Err("art/budget.toml: no [load] table (groups of assets loaded together)".into());
        };
        let mut out = LoadGroups::default();
        for (name, list) in load {
            let pats: Vec<String> = list
                .as_array()
                .ok_or_else(|| format!("art/budget.toml: load.{name} should be a list of files"))?
                .iter()
                .filter_map(|p| p.as_str().map(|p| p.trim_start_matches("assets/").to_string()))
                .collect();
            if name == "always" {
                out.always = pats;
            } else {
                out.groups.push((name.clone(), pats));
            }
        }
        Ok(out)
    }

    fn matches(pats: &[String], path: &str) -> bool {
        let rel = path.trim_start_matches("assets/");
        pats.iter().any(|p| glob(&p.split('*').map(str::to_string).collect::<Vec<_>>(), rel))
    }

    /// Each group's textures (with `always` and anything no group lists), biggest first.
    pub fn sums<'a>(&'a self, uses: &[TextureUse]) -> Vec<(&'a str, u64, usize)> {
        let listed = |p: &str| Self::matches(&self.always, p) || self.groups.iter().any(|(_, g)| Self::matches(g, p));
        let mut out: Vec<(&str, u64, usize)> = self
            .groups
            .iter()
            .map(|(name, g)| {
                let in_it: Vec<&TextureUse> = uses.iter().filter(|u| Self::matches(g, &u.path) || Self::matches(&self.always, &u.path) || !listed(&u.path)).collect();
                (name.as_str(), in_it.iter().map(|u| u.bytes).sum(), in_it.iter().map(|u| u.textures).sum())
            })
            .collect();
        if out.is_empty() {
            let in_it: Vec<&TextureUse> = uses.iter().collect();
            out.push(("always", in_it.iter().map(|u| u.bytes).sum(), in_it.iter().map(|u| u.textures).sum()));
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out
    }

    /// The images and models in `group` (with `always` and the unlisted).
    fn members<'a>(&self, group: &str, uses: &'a [TextureUse]) -> Vec<&'a TextureUse> {
        let g = self.groups.iter().find(|(n, _)| n == group).map(|(_, g)| g.as_slice()).unwrap_or(&[]);
        let listed = |p: &str| Self::matches(&self.always, p) || self.groups.iter().any(|(_, g)| Self::matches(g, p));
        uses.iter().filter(|u| Self::matches(g, &u.path) || Self::matches(&self.always, &u.path) || !listed(&u.path)).collect()
    }
}

/// `mb build`'s texture estimate for an mb3d game: a line always (so the
/// number is known before it matters), a warning when it's near or over a
/// limit, with the biggest users and what to do. With load groups
/// (`art/budget.toml`) the estimate is the biggest group's, and the
/// all-at-once number follows on a second line. `frees` says the game's code
/// frees textures or models, so a game without groups is told about them.
pub fn texture_notes(uses: &[TextureUse], groups: Option<&LoadGroups>, frees: bool) -> (String, Vec<String>) {
    let all_bytes: u64 = uses.iter().map(|u| u.bytes).sum();
    let all_count: usize = uses.iter().map(|u| u.textures).sum();
    let mb = |b: u64| b as f64 / MIB;
    let (bytes, count, line, top_of): (u64, usize, String, Vec<&TextureUse>) = match groups {
        Some(g) => {
            let sums = g.sums(uses);
            let (name, bytes, count) = sums[0];
            let line = format!(
                "textures: {:.1} of mb3d's {:.0} MB and {count} of {MAX_TEXTURES} textures at most, in load group `{name}` (art/budget.toml; with `always` and anything no group lists; decoded, with mipmaps)\n  textures: {:.1} MB and {all_count} textures if every image and model in assets/ were loaded at once",
                mb(bytes),
                mb(TEXTURE_BUDGET),
                mb(all_bytes)
            );
            (bytes, count, line, g.members(name, uses))
        }
        None => (
            all_bytes,
            all_count,
            format!(
                "textures: {:.1} of mb3d's {:.0} MB and {all_count} of {MAX_TEXTURES} textures if every image and model in assets/ is loaded at once (decoded, with mipmaps)",
                mb(all_bytes),
                mb(TEXTURE_BUDGET)
            ),
            uses.iter().collect(),
        ),
    };
    let mut warnings = Vec::new();
    let over = bytes > TEXTURE_BUDGET || count > MAX_TEXTURES;
    if over || bytes as f64 > TEXTURE_BUDGET as f64 * 0.85 {
        let top: Vec<String> = top_of.iter().take(5).map(|u| format!("{} {:.1} MB", u.path.trim_start_matches("assets/"), mb(u.bytes))).collect();
        let mut w = format!(
            "{} mb3d's texture budget{}: loaded together, a texture or model past it is refused (Texture::new / Model::load stay None; the page logs `runtime: … refused`). Biggest: {}. Load only what a scene shows and free the rest (Texture::free, Model::free), or give props and models seen small or far smaller maps (`mb model . <name> --reprocess --texture-size 512`, or 256 for small props; free, from the raw download). Keep the hero and anything the camera gets close to at 1024: its texture is what makes it read",
            if over { "over" } else { "near" },
            groups.map_or(String::new(), |g| format!(" in load group `{}`", g.sums(uses)[0].0)),
            top.join(", ")
        );
        if groups.is_none() && frees {
            w += ". This game frees textures or models: list what each scene loads in art/budget.toml ([load] always = [...], one list per scene) and mb build checks the biggest scene instead";
        }
        warnings.push(w);
    }
    (line, warnings)
}

/// Near a limit: within 15% of it.
const NEAR: f64 = 0.85;

/// What a game's code and scripts name, to tell the assets it uses: its
/// Rust source as text (a path, a file name or a quoted stem names an asset),
/// the string literals in it as patterns (`format!("assets/vo/{}/{}", ch,
/// id)` names everything under `assets/vo/`, `"bd_"` everything starting
/// so, `"assets/sky_{a}.jpg"` every sky), and voice scripts (`art/*.csv`,
/// whose `character,line` rows load `assets/vo/<character>/<line>.*`).
pub struct Refs {
    src: String,
    /// Each pattern's fixed pieces, anything between them.
    globs: Vec<Vec<String>>,
}

impl Refs {
    pub fn new(src: &str, scripts: &[String]) -> Refs {
        let mut globs = Vec::new();
        for lit in string_literals(src) {
            let pieces = format_pieces(&lit);
            let fixed: usize = pieces.iter().map(|p| p.trim_start_matches("assets/").len()).sum();
            let prefix = pieces.len() == 1 && (lit.ends_with('_') || lit.ends_with('/') || lit.ends_with('-'));
            if pieces.len() > 1 && fixed >= 3 && pieces.iter().any(|p| p.chars().any(char::is_alphanumeric)) {
                globs.push(pieces);
            } else if prefix && fixed >= 3 {
                globs.push(vec![lit.clone(), String::new()]);
            }
        }
        for script in scripts {
            for row in script.lines().skip(1) {
                let mut cells = row.split(',').map(|c| c.trim().trim_matches('"'));
                if let (Some(who), Some(id)) = (cells.next(), cells.next())
                    && !who.is_empty()
                    && !id.is_empty()
                    && !who.starts_with('#')
                {
                    globs.push(vec![format!("assets/vo/{who}/{id}."), String::new()]);
                }
            }
        }
        Refs { src: src.to_string(), globs }
    }

    /// Whether the code or scripts name `path` (a bundle path, `assets/…`).
    pub fn names(&self, path: &str) -> bool {
        let rel = path.trim_start_matches("assets/");
        let base = rel.rsplit('/').next().unwrap_or(rel);
        let stem = base.split('.').next().unwrap_or(base);
        if self.src.contains(rel) || self.src.contains(base) || (stem.len() >= 3 && self.src.contains(&format!("\"{stem}\""))) {
            return true;
        }
        self.globs.iter().any(|g| [path, rel, base].iter().any(|p| glob(g, p)))
    }
}

/// The string literals in Rust source (escapes kept as written; raw strings
/// read up to their first quote, which is enough for asset names).
fn string_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            // A char literal like '"' isn't a string.
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 2,
            b'\'' if b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'') => i += 3,
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                if let Some(s) = src.get(start..i.min(b.len())) {
                    out.push(s.to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// A format string's fixed pieces between its `{…}` placeholders (`{{` is a brace).
fn format_pieces(lit: &str) -> Vec<String> {
    let mut pieces = vec![String::new()];
    let mut chars = lit.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                pieces.last_mut().unwrap().push('{');
            }
            '{' => {
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
                pieces.push(String::new());
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                pieces.last_mut().unwrap().push('}');
            }
            c => pieces.last_mut().unwrap().push(c),
        }
    }
    pieces
}

/// Whether `s` is `g[0]`, anything, `g[1]`, anything … `g[n]`.
fn glob(g: &[String], s: &str) -> bool {
    if g.len() == 1 {
        return s == g[0];
    }
    let (first, last) = (&g[0], &g[g.len() - 1]);
    if !s.starts_with(first.as_str()) || s.len() < first.len() + last.len() || !s.ends_with(last.as_str()) {
        return false;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    for piece in &g[1..g.len() - 1] {
        match rest.find(piece.as_str()) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    true
}

/// Assets nothing in the game names ([`Refs`]), directly or through an index
/// file it names ([`listed`]). Generation sidecars, the packs `mb build`
/// writes, licenses and baked fonts (`mb font add` manages those) aren't listed.
pub fn maybe_unused<'a>(files: &'a BTreeMap<String, Vec<u8>>, refs: &Refs) -> Vec<(&'a str, u64)> {
    let listed = listed(files, refs);
    files
        .iter()
        .filter(|(p, _)| p.starts_with("assets/") && !p.starts_with("assets/fonts/") && !p.ends_with(".gen.json") && !p.ends_with("LICENSE.txt"))
        .filter(|(p, _)| p.as_str() != PROVENANCE && !(p.contains("/vo/") && p.ends_with(&format!("/{LIPS_PACK}"))))
        .filter(|(p, _)| !refs.names(p) && !listed.contains(p.as_str()))
        .map(|(p, b)| (p.as_str(), b.len() as u64))
        .collect()
}

/// The assets that index files the game uses list, followed through: a
/// setlist (`<name>.songs.json`) lists music graphs, a graph
/// (`<name>.music.json`) its take, layers, stingers, swell and tension kit,
/// and any other JSON asset the game names (an atlas, a level list) the
/// files it names. A string in one counts when it is a bundle path
/// (`assets/…`) or a path next to the index file, of a file that's there.
pub fn listed<'a>(files: &'a BTreeMap<String, Vec<u8>>, refs: &Refs) -> std::collections::BTreeSet<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo: Vec<&str> = files.keys().map(String::as_str).filter(|p| is_index(p) && refs.names(p)).collect();
    while let Some(index) = todo.pop() {
        let Some(v) = files.get(index).and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok()) else { continue };
        let dir = index.rsplit_once('/').map_or("", |d| d.0);
        let mut strings = Vec::new();
        json_strings(&v, &mut strings);
        for s in strings {
            let s = s.trim_start_matches("./");
            let hit = [s.to_string(), format!("{dir}/{s}")].into_iter().find_map(|c| files.get_key_value(&c).map(|kv| kv.0.as_str()));
            if let Some(p) = hit
                && p != index
                && seen.insert(p)
                && is_index(p)
            {
                todo.push(p);
            }
        }
    }
    seen
}

/// A JSON asset, which may list others (sidecars don't count).
fn is_index(p: &str) -> bool {
    p.starts_with("assets/") && p.ends_with(".json") && !p.ends_with(".gen.json")
}

fn json_strings<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match v {
        serde_json::Value::String(s) => out.push(s),
        serde_json::Value::Array(a) => a.iter().for_each(|x| json_strings(x, out)),
        serde_json::Value::Object(o) => o.values().for_each(|x| json_strings(x, out)),
        _ => {}
    }
}

/// How many to name in the maybe-unused list.
const UNUSED_SHOWN: usize = 15;

/// The maybe-unused part of a near-limit warning, biggest first, at most
/// [`UNUSED_SHOWN`] named.
fn unused_note(files: &BTreeMap<String, Vec<u8>>, refs: &Refs) -> Option<String> {
    let mut unused = maybe_unused(files, refs);
    if unused.is_empty() {
        return None;
    }
    unused.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let total: u64 = unused.iter().map(|u| u.1).sum();
    let names: Vec<&str> = unused.iter().take(UNUSED_SHOWN).map(|u| u.0.trim_start_matches("assets/")).collect();
    let more = unused.len().saturating_sub(UNUSED_SHOWN);
    Some(format!(
        "Maybe unused ({} assets, {:.1} MB): {}{}: nothing in src/ or art/*.csv names them (as a path, a file name or a format pattern), nor a JSON asset they name (a setlist, a music graph); move retired ones out of assets/ (to art/retired/)",
        unused.len(),
        total as f64 / MIB,
        names.join(", "),
        if more > 0 { format!(" and {more} more") } else { String::new() }
    ))
}

/// `mb build`'s size warnings: near or over the bundle's (`compressed`),
/// the startup set's or the wasm's limit, with what to look at.
pub fn size_notes(files: &BTreeMap<String, Vec<u8>>, refs: &Refs, compressed: u64, startup: u64, wasm: u64, startup_assets: &[String]) -> Vec<String> {
    use mb_format::bundle::{MAX_COMPRESSED, MAX_STARTUP, MAX_WASM};
    let mb = |b: u64| format!("{:.1} MB", b as f64 / MIB);
    let state = |v: u64, max: u64| if v > max { Some("over") } else if v as f64 > max as f64 * NEAR { Some("near") } else { None };
    let mut out = Vec::new();
    if let Some(s) = state(compressed, MAX_COMPRESSED) {
        let mut assets: Vec<(&str, u64)> = files.iter().filter(|(p, _)| p.starts_with("assets/")).map(|(p, b)| (p.as_str(), b.len() as u64)).collect();
        assets.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let top: Vec<String> = assets.iter().take(5).map(|(p, n)| format!("{} {}", p.trim_start_matches("assets/"), mb(*n))).collect();
        let mut w = format!("the bundle is {} of its {} limit ({s} it). Biggest assets: {}", mb(compressed), mb(MAX_COMPRESSED), top.join(", "));
        if let Some(u) = unused_note(files, refs) {
            w += &format!(". {u}");
        } else {
            w += ". Shrink props and background models with `mb model . <name> --reprocess --polys N --texture-size 512` (free; keep the hero and close-ups at 1024), images with smaller sizes or JPEG, music with shorter loops";
        }
        out.push(w);
    }
    if let Some(s) = state(startup, MAX_STARTUP) {
        let mut st: Vec<(&str, u64)> = startup_assets.iter().filter_map(|a| files.get(a).map(|b| (a.as_str(), b.len() as u64))).collect();
        st.sort_by(|a, b| b.1.cmp(&a.1));
        let top: Vec<String> = st.iter().take(5).map(|(p, n)| format!("{} {}", p.trim_start_matches("assets/"), mb(*n))).collect();
        out.push(format!(
            "the startup set (game.wasm + startup_assets) is {} of its {} limit ({s} it){}: list only what the first screen needs in startup_assets; the rest loads while the game runs",
            mb(startup),
            mb(MAX_STARTUP),
            if top.is_empty() { String::new() } else { format!("; biggest: {}", top.join(", ")) }
        ));
    }
    if let Some(s) = state(wasm, MAX_WASM) {
        out.push(format!("game.wasm is {} of its {} limit ({s} it): data baked into the code (include_bytes!, big tables) belongs in assets/", mb(wasm), mb(MAX_WASM)));
    }
    out
}

/// Where `mb build` packs every `.gen.json` sidecar in `assets/` when the
/// game's code reads it (names `provenance.json`): one JSON object, bundle
/// path of the asset → its sidecar.
pub const PROVENANCE: &str = "assets/provenance.json";
/// The file `mb build` packs a directory's `.lips.txt` timings into.
pub const LIPS_PACK: &str = "lips.txt";
/// The first line of a lips pack. The SDK's `VoiceLine` (0.7.0 on) contains
/// this string, so finding it in game.wasm says the game reads packs.
pub const LIPS_PACK_HEADER: &str = "mblipspack 1";

/// What [`fold`] did: entries (and bytes) saved by leaving out or packing
/// together small per-asset files.
#[derive(Debug, Default, PartialEq)]
pub struct Folded {
    /// Sidecars packed into [`PROVENANCE`] (the game reads it).
    pub sidecars: usize,
    /// Sidecars left out of the bundle (nothing in the game reads them), and their bytes.
    pub sidecars_left_out: usize,
    pub left_out_bytes: usize,
    pub lips: usize,
    pub lips_packs: usize,
    /// Why lips weren't packed, when there were some.
    pub lips_kept: Option<String>,
}

impl Folded {
    pub fn saved(&self) -> usize {
        self.sidecars.saturating_sub(1) + self.sidecars_left_out + self.lips - self.lips_packs
    }
}

/// Keeps the bundle's small per-asset files from costing an entry each,
/// since every file is one of the bundle's 512 entries (SPEC §1). The
/// provenance sidecars (`assets/**/*.gen.json`) are for `mb regen`, `mb art
/// check` and audits, not the game, so they're left out of the bundle unless
/// `src` (the game's Rust) names them: naming a `.gen.json` keeps them as
/// they are, naming `provenance.json` packs them into [`PROVENANCE`]. When
/// the game's SDK reads packs (`wasm` contains [`LIPS_PACK_HEADER`]), each
/// directory's lip timings (`assets/vo/<who>/<id>.lips.txt`) are packed into
/// `assets/vo/<who>/lips.txt`. The source tree keeps its per-file sidecars
/// and timings; only the bundle changes.
pub fn fold(files: &mut BTreeMap<String, Vec<u8>>, src: &str) -> Folded {
    let mut out = Folded::default();
    let sidecars: Vec<String> = files.keys().filter(|p| p.starts_with("assets/") && p.ends_with(".gen.json")).cloned().collect();
    let keeps = src.contains(".gen.json");
    if !sidecars.is_empty() && !keeps && !src.contains("provenance.json") {
        for p in &sidecars {
            out.left_out_bytes += files.remove(p).map_or(0, |b| b.len());
        }
        out.sidecars_left_out = sidecars.len();
    } else if !sidecars.is_empty() && !files.contains_key(PROVENANCE) && !keeps {
        let mut index = serde_json::Map::new();
        for p in &sidecars {
            let b = files.remove(p).unwrap_or_default();
            let asset = p.trim_end_matches(".gen.json").to_string();
            let v = serde_json::from_slice::<serde_json::Value>(&b).unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&b).into_owned()));
            index.insert(asset, v);
        }
        let mut json = serde_json::to_vec_pretty(&serde_json::Value::Object(index)).unwrap_or_default();
        json.push(b'\n');
        files.insert(PROVENANCE.to_string(), json);
        out.sidecars = sidecars.len();
    }
    let lips: Vec<String> = files.keys().filter(|p| p.starts_with("assets/") && p.ends_with(".lips.txt")).cloned().collect();
    if lips.is_empty() {
        return out;
    }
    let reads_packs = files.get("game.wasm").is_some_and(|w| w.windows(LIPS_PACK_HEADER.len()).any(|x| x == LIPS_PACK_HEADER.as_bytes()));
    if !reads_packs {
        out.lips_kept = Some("the game's maimbrain SDK predates lips packs (0.7.0); update it to pack them".into());
        return out;
    }
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in lips {
        let (dir, _) = p.rsplit_once('/').unwrap_or(("", &p));
        dirs.entry(dir.to_string()).or_default().push(p);
    }
    for (dir, paths) in dirs {
        let pack = format!("{dir}/{LIPS_PACK}");
        if files.contains_key(&pack) {
            continue;
        }
        let mut text = format!("{LIPS_PACK_HEADER}\n");
        for p in &paths {
            let b = files.remove(p).unwrap_or_default();
            let id = p.rsplit('/').next().unwrap_or(p).trim_end_matches(".lips.txt");
            text += &format!("@ {id}\n{}\n", String::from_utf8_lossy(&b).trim_end());
            out.lips += 1;
        }
        files.insert(pack, text.into_bytes());
        out.lips_packs += 1;
    }
    out
}

/// What kind of entry a bundle path is, for the entry breakdown.
fn entry_kind(p: &str) -> &'static str {
    let ext = p.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if p.starts_with("src/") {
        "source files"
    } else if p.ends_with(".gen.json") {
        "provenance sidecars"
    } else if p.ends_with(".lips.txt") || p.ends_with(LIPS_PACK) && p.contains("/vo/") {
        "lip timings"
    } else if p.contains("/vo/") && matches!(ext.as_str(), "ogg" | "opus") {
        "voice lines"
    } else {
        match ext.as_str() {
            "ogg" | "opus" => "sounds",
            "png" | "jpg" | "jpeg" | "ktx2" => "images",
            "glb" => "models",
            "mbf" | "ttf" => "fonts",
            _ if !p.starts_with("assets/") => "manifest, icon and wasm",
            _ => "other assets",
        }
    }
}

/// `mb build`'s warning when the bundle's files near (80%) or pass the 512
/// entries a bundle may hold: how many of each kind, biggest first.
pub fn entry_notes(files: &BTreeMap<String, Vec<u8>>, folded: &Folded, refs: &Refs) -> Option<String> {
    use mb_format::bundle::MAX_FILES;
    let n = files.len();
    if (n as f64) < MAX_FILES as f64 * 0.8 {
        return None;
    }
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for p in files.keys() {
        *kinds.entry(entry_kind(p)).or_default() += 1;
    }
    let mut kinds: Vec<(&str, usize)> = kinds.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let list: Vec<String> = kinds.iter().map(|(k, c)| format!("{c} {k}")).collect();
    let mut w = format!(
        "the bundle has {n} of its {MAX_FILES} entries ({} it; every file is one): {}",
        if n > MAX_FILES { "over" } else { "near" },
        list.join(", ")
    );
    if folded.saved() > 0 {
        w += &format!(
            " (after leaving out {} sidecars and packing {} sidecars and {} lip timings into {} files)",
            folded.sidecars_left_out,
            folded.sidecars,
            folded.lips,
            folded.lips_packs + (folded.sidecars > 0) as usize
        );
    }
    if let Some(why) = &folded.lips_kept {
        w += &format!("; lip timings ship one file per line because {why}");
    }
    w += ". Move retired assets out of assets/ (to art/retired/), pack small images into an atlas (`mb art atlas`), or join short sounds";
    if let Some(u) = unused_note(files, refs) {
        w += &format!(". {u}");
    }
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b
    }

    #[test]
    fn a_bundle_over_its_limit_lists_the_biggest_and_the_maybe_unused_assets() {
        let mut files = BTreeMap::new();
        for (p, n) in [("assets/heron.glb", 3_000_000), ("assets/heron2.glb", 3_100_000), ("assets/boat.glb", 1_000_000), ("assets/heron2.glb.gen.json", 900), ("assets/fonts/a.mbf", 9000)] {
            files.insert(p.to_string(), vec![0u8; n]);
        }
        let src = Refs::new(r#"let a = Asset::load("assets/heron2.glb"); let b = "boat.glb";"#, &[]);
        let src = &src;
        let unused = maybe_unused(&files, src);
        assert_eq!(unused, vec![("assets/heron.glb", 3_000_000)]);
        let w = size_notes(&files, src, 10_946_089, 1_000_000, 500_000, &[]);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("the bundle is 10.4 MB of its 10.0 MB limit (over it). Biggest assets: heron2.glb 3.0 MB, heron.glb 2.9 MB, boat.glb 1.0 MB"), "{w:?}");
        assert!(w[0].contains("Maybe unused (1 assets, 2.9 MB): heron.glb: nothing in src/ or art/*.csv names them"), "{w:?}");
        assert!(size_notes(&files, src, 5_000_000, 1_000_000, 500_000, &[]).is_empty());
        let near = size_notes(&files, src, 5_000_000, 2_900_000, 3_900_000, &["assets/boat.glb".into()]);
        assert!(near[0].contains("startup set (game.wasm + startup_assets) is 2.8 MB of its 3.0 MB limit (near it); biggest: boat.glb 1.0 MB"), "{near:?}");
        assert!(near[1].starts_with("game.wasm is 3.7 MB of its 4.0 MB limit (near it)"), "{near:?}");
    }

    #[test]
    fn what_a_setlist_and_its_graphs_list_is_used() {
        let mut files = BTreeMap::new();
        let mut put = |p: &str, b: &str| files.insert(p.to_string(), b.as_bytes().to_vec());
        put("assets/brawl.songs.json", r#"{ "format": "maimbrain-songs", "songs": [ { "zone": "harbor", "graph": "assets/brawl_harbor.music.json", "prompt": "rock, 128 bpm" } ] }"#);
        put(
            "assets/brawl_harbor.music.json",
            r#"{ "format": "maimbrain-music-graph", "files": { "take": "assets/brawl_harbor_take.ogg", "tension": { "tick": "assets/brawl_harbor_tick.ogg" } }, "zones": [ { "stinger": { "file": "assets/brawl_harbor_into_harbor.ogg" } } ] }"#,
        );
        put("assets/levels/list.json", r#"["one.json", "./two.png", "missing.png"]"#);
        for p in ["brawl_harbor_take.ogg", "brawl_harbor_tick.ogg", "brawl_harbor_into_harbor.ogg", "brawl_neon_take.ogg", "levels/one.json", "levels/two.png", "old.json"] {
            put(&format!("assets/{p}"), "{}");
        }
        put("assets/old.music.json", r#"{ "files": { "take": "assets/brawl_neon_take.ogg" } }"#);
        let refs = Refs::new(r#"let s = Setlist::parse(load("assets/brawl.songs.json")); let l = "levels/list.json";"#, &[]);
        let unused: Vec<&str> = maybe_unused(&files, &refs).into_iter().map(|u| u.0).collect();
        assert_eq!(unused, vec!["assets/brawl_neon_take.ogg", "assets/old.json", "assets/old.music.json"], "a graph nothing names lists nothing");
    }

    #[test]
    fn sidecars_and_lip_timings_ship_packed_one_file_each() {
        let mut files = BTreeMap::new();
        files.insert("game.wasm".to_string(), format!("\0asm..{LIPS_PACK_HEADER}..").into_bytes());
        for who in ["kai", "mags"] {
            for id in ["a1", "a2", "a3"] {
                files.insert(format!("assets/vo/{who}/{id}.ogg"), vec![1]);
                files.insert(format!("assets/vo/{who}/{id}.lips.txt"), format!("mblips 1\ntext {who} {id}\n").into_bytes());
                files.insert(format!("assets/vo/{who}/{id}.ogg.gen.json"), br#"{"provider":"elevenlabs"}"#.to_vec());
            }
        }
        files.insert("assets/sky.png.gen.json".into(), b"not json".to_vec());
        let before = files.len();
        let sidecar_bytes: usize = files.iter().filter(|(p, _)| p.ends_with(".gen.json")).map(|(_, b)| b.len()).sum();
        let voiced = "VoiceLine::load(&format!(\"assets/vo/{}/{}\", who, id))";
        let mut packed = files.clone();
        // Nothing reads the sidecars: they stay out of the bundle.
        let f = fold(&mut files, voiced);
        assert_eq!((f.sidecars, f.sidecars_left_out, f.left_out_bytes, f.lips, f.lips_packs), (0, 7, sidecar_bytes, 6, 2));
        assert_eq!(files.len(), before - f.saved());
        assert!(!files.contains_key(PROVENANCE));
        let pack = String::from_utf8(files["assets/vo/kai/lips.txt"].clone()).unwrap();
        assert_eq!(pack, "mblipspack 1\n@ a1\nmblips 1\ntext kai a1\n@ a2\nmblips 1\ntext kai a2\n@ a3\nmblips 1\ntext kai a3\n");
        assert!(!files.keys().any(|p| p.ends_with(".gen.json") || p.ends_with(".lips.txt")));
        // The SDK that reads packs is the one whose header this is.
        assert!(include_str!("../../../sdk/maimbrain/src/ui/dialogue.rs").contains(&format!("\"{LIPS_PACK_HEADER}\"")));

        // A game that reads assets/provenance.json gets them packed into it.
        let f = fold(&mut packed, &format!("{voiced}; Asset::load(\"assets/provenance.json\")"));
        assert_eq!((f.sidecars, f.sidecars_left_out), (7, 0));
        assert_eq!(packed.len(), before - f.saved());
        let prov: serde_json::Value = serde_json::from_slice(&packed[PROVENANCE]).unwrap();
        assert_eq!(prov["assets/vo/kai/a1.ogg"]["provider"], "elevenlabs");
        assert_eq!(prov["assets/sky.png"], "not json");

        // An older SDK keeps its per-line timings; a game that reads a sidecar keeps them all.
        let mut old = BTreeMap::new();
        old.insert("game.wasm".to_string(), b"\0asm".to_vec());
        old.insert("assets/vo/kai/a1.lips.txt".to_string(), b"mblips 1\n".to_vec());
        old.insert("assets/a.png.gen.json".to_string(), b"{}".to_vec());
        let f = fold(&mut old.clone(), "");
        assert_eq!((f.sidecars_left_out, f.lips), (1, 0));
        assert!(f.lips_kept.unwrap().contains("predates lips packs"));
        let f = fold(&mut old, r#"Asset::load("assets/a.png.gen.json")"#);
        assert_eq!((f.sidecars, f.sidecars_left_out), (0, 0));
        assert!(old.contains_key("assets/a.png.gen.json"));
    }

    #[test]
    fn names_built_at_runtime_and_voice_scripts_count_as_uses() {
        let src = r#"
            // "assets/old_comment.png" isn't code, but the text still names it
            let sky = format!("assets/sky_{a}.jpg");
            let far = format!("assets/bd_{}_far.png", arena);
            let line = VoiceLine::load(&format!("assets/vo/{who}/{id}"));
            let sfx = ["boom", "clank"].map(|s| format!("assets/{s}.ogg"));
            let p = "port_";
            let any = format!("{}", x); let c = '"';
            let tex = Texture::load("assets/floor.png");
        "#;
        let script = "character,line,text\nmags,nice1,\"Nice shot, Kai.\"\n".to_string();
        let refs = Refs::new(src, &[script]);
        for used in ["assets/sky_city.jpg", "assets/bd_harbor_far.png", "assets/vo/kai/c1.ogg", "assets/boom.ogg", "assets/port_kai.png", "assets/floor.png", "assets/old_comment.png"] {
            assert!(refs.names(used), "{used}");
        }
        for unused in ["assets/bd_harbor_near.png", "assets/sky_city.png", "assets/hero.glb", "assets/floor2.png"] {
            assert!(!refs.names(unused), "{unused}");
        }
        let refs = Refs::new("", &["character,line\nmags,nice1\n".to_string()]);
        assert!(refs.names("assets/vo/mags/nice1.ogg") && !refs.names("assets/vo/mags/nice2.ogg"));

        // At most 15 named, biggest first, with a count of the rest.
        let mut files = BTreeMap::new();
        for i in 0..40 {
            files.insert(format!("assets/x{i:02}.png"), vec![0u8; 1000 + i]);
        }
        files.insert(PROVENANCE.into(), vec![0; 5000]);
        files.insert("assets/vo/kai/lips.txt".into(), vec![0; 5000]);
        let note = unused_note(&files, &Refs::new("", &[])).unwrap();
        assert!(note.starts_with("Maybe unused (40 assets, 0.0 MB): x39.png, x38.png,"), "{note}");
        assert!(note.contains("x25.png and 25 more: nothing in src/"), "{note}");
    }

    #[test]
    fn near_the_entry_limit_it_says_what_the_entries_are() {
        let mut files = BTreeMap::new();
        files.insert("game.wasm".to_string(), vec![]);
        for i in 0..120 {
            files.insert(format!("assets/vo/kai/l{i}.ogg"), vec![]);
            files.insert(format!("assets/vo/kai/l{i}.lips.txt"), vec![]);
            files.insert(format!("assets/vo/kai/l{i}.ogg.gen.json"), vec![]);
        }
        assert!(entry_notes(&files, &Folded::default(), &Refs::new("", &[])).is_none());
        for i in 0..150 {
            files.insert(format!("assets/s{i}.png"), vec![]);
        }
        let w = entry_notes(&files, &Folded::default(), &Refs::new("", &[])).unwrap();
        assert!(w.starts_with("the bundle has 511 of its 512 entries (near it; every file is one): 150 images, 120 lip timings, 120 provenance sidecars, 120 voice lines, 1 manifest, icon and wasm"), "{w}");
    }

    #[test]
    fn sums_images_with_mipmaps_and_warns_over_the_budget() {
        let mut files = BTreeMap::new();
        files.insert("assets/sky.png".to_string(), png(2048, 1024));
        files.insert("assets/ui/icon.png".to_string(), png(64, 64));
        files.insert("src/src/main.rs".to_string(), png(2048, 2048));
        let uses = texture_uses(&files);
        assert_eq!(uses[0].path, "assets/sky.png");
        assert_eq!(uses[0].bytes, 2048 * 1024 * 4 * 4 / 3);
        assert_eq!(uses.len(), 2);
        let (line, warnings) = texture_notes(&uses, None, false);
        assert!(line.starts_with("textures: 10.7 of mb3d's 96 MB and 2 of 64 textures"), "{line}");
        assert!(warnings.is_empty());
        for i in 0..9 {
            files.insert(format!("assets/big{i}.png"), png(2048, 2048));
        }
        let (_, warnings) = texture_notes(&texture_uses(&files), None, true);
        assert!(warnings[0].starts_with("over mb3d's texture budget"), "{warnings:?}");
        assert!(warnings[0].contains("Biggest: big0.png 21.3 MB"), "{warnings:?}");
        assert!(warnings[0].contains("list what each scene loads in art/budget.toml"), "{warnings:?}");
    }

    #[test]
    fn load_groups_count_the_biggest_scene_not_everything() {
        let mut files = BTreeMap::new();
        for a in ["harbor", "city", "desert", "colony"] {
            files.insert(format!("assets/sky_{a}.jpg"), png(2048, 1024)); // 10.7 MB each
            files.insert(format!("assets/bd_{a}_far.png"), png(1024, 512));
        }
        files.insert("assets/p_kai.png".into(), png(512, 512));
        files.insert("assets/loose.png".into(), png(256, 256));
        let uses = texture_uses(&files);
        let g = LoadGroups::parse(
            "[load]\nalways = [\"p_*.png\"]\nharbor = [\"sky_harbor.jpg\", \"bd_harbor_*\"]\ncity = [\"assets/sky_city.jpg\", \"bd_city_*\"]\ndesert = [\"sky_desert.jpg\"]\n",
        )
        .unwrap();
        let sums = g.sums(&uses);
        // city and harbor tie with sky + backdrop + portrait + the unlisted (colony's and loose.png).
        assert_eq!(sums[0].0, "city");
        assert_eq!(sums[0].2, 1 + 1 + 1 + 4);
        let (line, warnings) = texture_notes(&uses, Some(&g), true);
        assert!(line.starts_with("textures: 31.0 of mb3d's 96 MB and 7 of 64 textures at most, in load group `city`"), "{line}");
        assert!(line.contains("\n  textures: 55.0 MB and 10 textures if every image and model in assets/ were loaded at once"), "{line}");
        assert!(warnings.is_empty());
        assert!(LoadGroups::parse("[groups]\n").is_err());
    }
}
