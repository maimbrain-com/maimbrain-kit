//! `mb studio-asset` for engine 2.0 projects (docs/STUDIO-2.md §7): the
//! pictures `generate` makes, landing as the 2.0 engine reads them. The
//! server decides each job's sizes and price and makes the provider calls;
//! this writes the prompts (`--plan`, from `mb art`'s, with the project's
//! look) and does `mb art`'s post-processing on what came back:
//!
//! - `image`: a thing on a transparent background, any size (`--dims`), or
//!   pixel art (`--pixel`), `images/<name>.png`;
//! - `frames`: frame 1 and edits of it, keyed and put side by side at one
//!   scale, `frame = [w, h]` in `images/<name>.toml`;
//! - `walker`: a body without feet and one foot, for `Walker{}`: `walker =
//!   true` with the body and foot measured, in `images/<name>.toml`;
//! - `background`: an opaque painting (`images/<name>.jpg`; PNG for pixel art);
//! - `layers`: parallax layers far to near, `images/<name>_0..n`: the far
//!   one opaque (JPEG), the nearer ones keyed (PNG), all wrapping
//!   sideways; the judge sees them stacked as the game shows them;
//! - `tile`: a tile of `tiles/<name>.png`, made to wrap, replacing or adding
//!   a frame of the set (`--frame`, from 1), with `frame = [w, h]` in
//!   `tiles/<name>.toml`;
//! - `ui`: a button, panel or badge on a transparent background,
//!   `images/ui/<name>.png`;
//! - `icon`: `icon.png`, 256×256 and opaque;
//! - `sky`: a 3D game's panorama, `images/<name>.jpg`, 2048×1024 and
//!   equirectangular, for `world.sky{ panorama = <name> }`.
//!
//! **Footings** (`--stands`): where a thing that stands meets the ground,
//! `footing = [x, y]` in its frame's pixels, goes in its `.toml`; a walker's
//! body and foot are its footing. Under the Arcade preset, `image`, `frames`,
//! `tile` and `icon` are 1.0's pixel art on the palette.
//!
//! The report: `{ kind, name, candidates: [{ files: [{ file, path }], toml:
//! { path, set, remove } | null, view, width, height, frames, warnings }] }`:
//! each candidate's files under DIR/<n>/ with their sidecars, the keys its
//! `.toml` gets, and `view`, the picture the judge and the card show when it
//! isn't the file itself (layers stacked, a tile repeated, a walker standing).

use std::path::Path;

use serde_json::{Value, json};
use toml::{Table, Value as Toml};

use super::asset::AssetArgs;
use crate::generate::art::{self, Job};
use crate::generate::img::{self, Anchor, Image};
use crate::generate::style::{self, PromptParts, Style};
use crate::generate::{Sidecar, sha256_hex};

/// The kinds a 2.0 project's pictures come in.
pub const KINDS: &[&str] = &["image", "frames", "walker", "background", "layers", "tile", "ui", "icon", "sky", "portrait"];
/// A portrait's most expressions, neutral among them (§7.2); the blink row is one more.
pub const MAX_EXPRESSIONS: usize = 7;
/// mb2d's largest picture, on either side (SPEC §5.3).
const MAX_IMAGE: u32 = 4096;
/// The longest side the judge and the card are shown.
const VIEW: u32 = 1024;
/// What a JPEG painting aims for (a layer is two screens wide).
const JPEG_KB: u32 = 450;
/// What a sky's JPEG aims for (`mb art sky`'s default).
const SKY_KB: u32 = 800;
/// A keyed painting's PNG past this is quantized (the startup set is 3 MB).
const PNG_KB: usize = 700;

pub fn parse_dims(s: &str) -> Result<[u32; 2], String> {
    let (w, h) = s.split_once(['x', 'X', '×']).ok_or("WxH, like 256x128")?;
    let n = |v: &str| v.trim().parse::<u32>().ok().filter(|n| (1..=4096).contains(n)).ok_or_else(|| format!("{v}: 1–4096"));
    Ok([n(w)?, n(h)?])
}

/// studio.toml says engine 2.0 or later.
pub fn is_v2(t: &Table) -> bool {
    let major = match t.get("engine") {
        Some(Toml::String(s)) => s.trim().split('.').next().and_then(|m| m.parse::<u32>().ok()).unwrap_or(1),
        Some(Toml::Float(f)) => *f as u32,
        Some(Toml::Integer(i)) => *i as u32,
        _ => 1,
    };
    major >= 2
}

fn arcade(t: &Table) -> bool {
    t.get("preset").and_then(Toml::as_str) == Some("arcade")
}

/// Where the job's file goes (layer `i`'s for layers).
fn main_path(kind: &str, name: &str, i: u32, pixel: bool) -> String {
    match kind {
        "background" if !pixel => format!("images/{name}.jpg"),
        "sky" => format!("images/{name}.jpg"),
        "layers" if !pixel && i == 0 => format!("images/{name}_0.jpg"),
        "layers" => format!("images/{name}_{i}.png"),
        "tile" => format!("tiles/{name}.png"),
        "ui" => format!("images/ui/{name}.png"),
        "portrait" => format!("images/portraits/{name}.png"),
        "icon" => "icon.png".into(),
        _ => format!("images/{name}.png"),
    }
}

/// A portrait's rows: neutral, then the expressions asked for (trimmed, lowercased, each once); the blink row follows them.
pub fn portrait_rows(asked: &[String]) -> Result<Vec<String>, String> {
    let mut out = vec!["neutral".to_string()];
    for e in asked {
        let e = e.trim().to_lowercase();
        if e.is_empty() || out.contains(&e) {
            continue;
        }
        if e == "blink" {
            return Err("the blink row is always added (the last row): leave it out of expressions".into());
        }
        if e.chars().count() > 24 || !e.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-') {
            return Err(format!("expression {e:?}: name each in a word or two (smug, angry, scared)"));
        }
        out.push(e);
    }
    if out.len() > MAX_EXPRESSIONS {
        return Err(format!("{} expressions with neutral is too many for one sheet (at most {MAX_EXPRESSIONS}): make two portraits", out.len()));
    }
    Ok(out)
}

fn toml_path(file: &str) -> String {
    format!("{}.toml", file.rsplit_once('.').map_or(file, |(s, _)| s))
}

/// The game's look for every prompt: art/style.toml when there is one, else
/// DESIGN.md's Look (what `create_project`'s `look` wrote).
fn load_style(project: &Path, a: &AssetArgs, t: &Table) -> Result<Style, String> {
    // A style guide (or DESIGN.md) that's a link out of the project is read as none.
    let (mut s, _) = if inside(project, "art/style.toml") { Style::load(project)? } else { (Style::default(), None) };
    if s.style.trim().is_empty()
        && let Some(look) = design_look(project)
    {
        s.style = look;
    }
    s.pixel_art = a.pixel || arcade(t);
    // A layer set's key: the one its nearer layers were asked for, or one away
    // from the colours its far layer came back in (a green key for a dusk sky).
    if a.kind == "layers" && !arcade(t) {
        if let Some(k) = &a.layer_key {
            img::parse_hex(k).map_err(|e| format!("--layer-key: {e}"))?;
            s.key = k.clone();
        } else if s.key == "auto" && !a.far.is_empty() {
            let far = a
                .far
                .iter()
                .map(|f| std::fs::read(f).map_err(|e| format!("{f}: {e}")).and_then(|b| Image::decode(&b).map_err(|e| format!("{f}: {e}"))))
                .collect::<Result<Vec<_>, _>>()?;
            s.key = img::hex(img::key_away_from(&far, &s.palette_rgb()?));
        }
    }
    // With no palette to keep clear of, `auto` would be pure blue (every
    // candidate ties): magenta, which skies, water and foliage rarely are.
    if s.key == "auto" && s.palette.is_empty() && !arcade(t) {
        s.key = "#ff00ff".into();
    }
    if arcade(t) {
        let (pal, _) = super::palette(t)?;
        s.palette = pal.iter().map(|c| format!("#{c:06x}")).collect();
        s.palette_lock = true;
    }
    Ok(s)
}

/// DESIGN.md's Look section, its first paragraph (none while it says "Not chosen yet.").
fn design_look(project: &Path) -> Option<String> {
    if !inside(project, "DESIGN.md") {
        return None;
    }
    let text = std::fs::read_to_string(project.join("DESIGN.md")).ok()?;
    let after = text.split("\n## Look").nth(1)?;
    let body = after.split_once('\n')?.1;
    let para: Vec<&str> = body.lines().map(str::trim).skip_while(|l| l.is_empty()).take_while(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let look = para.join(" ");
    (!look.is_empty() && look != "Not chosen yet.").then(|| look.chars().take(400).collect())
}

/// Whether `rel` names a file inside `project`: a relative path of plain
/// names (no `..`, no root), still inside once its links are followed. A
/// style's references and `--ref` are sent to the image provider, so
/// nothing else may be.
pub fn inside(project: &Path, rel: &str) -> bool {
    use std::path::Component;
    let path = Path::new(rel);
    if rel.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return false;
    }
    let (Ok(root), Ok(file)) = (project.canonicalize(), project.join(path).canonicalize()) else { return false };
    file.starts_with(&root) && file.is_file()
}

fn dims(a: &AssetArgs) -> Result<[u32; 2], String> {
    a.dims.ok_or_else(|| "--dims WxH: the picture's size in pixels".to_string())
}

fn layer_count(a: &AssetArgs) -> u32 {
    a.count.unwrap_or(3).clamp(2, 4)
}

// --- the prompts -------------------------------------------------------------

/// `--plan`: each step's prompt and references, in the order the server
/// makes them for every candidate (frames: frame 1, then each frame as an
/// edit of it; layers: the far one, then each nearer one with it attached).
/// A reference is `{ file }` (in the project) or `{ step }` (that step's
/// raw output, the same candidate's).
pub fn plan(a: &AssetArgs, t: &Table) -> Result<Value, String> {
    let kind = a.kind.as_str();
    if kind == "voice" {
        return Err("voice has no --plan: the server writes what's said".into());
    }
    check_kind(kind, t)?;
    let style = load_style(&a.project, a, t)?;
    let subject = a.prompt.trim();
    if subject.is_empty() {
        return Err("--prompt: what it is".into());
    }
    let keyed = matches!(kind, "image" | "frames" | "walker" | "ui" | "layers" | "portrait");
    let key = if keyed { Some(style.key_color()?) } else { None };
    let [w, h] = a.dims.unwrap_or([512, 512]);
    // Only pictures inside the project: what's named here is sent to the provider.
    let style_refs: Vec<Value> = style.references.iter().filter(|r| inside(&a.project, r)).take(3).map(|r| json!({ "file": r })).collect();
    let mut own_refs = Vec::new();
    for r in &a.refs {
        if !inside(&a.project, r) {
            return Err(format!("--ref {r}: a picture in the project, like images/hero.png"));
        }
        own_refs.push(json!({ "file": r }));
    }
    let grid = style.pixel_art.then_some((w, h));
    let compose = |k: &str, subject: &str, keyed: Option<[u8; 3]>, srefs: usize, subj: usize, frames: u32| {
        style::compose(&style, &PromptParts { kind: k, subject, frames, expressions: &[], grid, keyed, portrait: h >= w, style_refs: srefs, subject_refs: subj })
    };
    let with = |extra: &[Value]| -> Vec<Value> { style_refs.iter().chain(extra).cloned().collect() };
    let mut steps = Vec::new();
    match kind {
        "frames" => {
            let n = a.frames.unwrap_or(4).clamp(2, 8);
            let motion = if a.motion.trim().is_empty() { "its animation" } else { a.motion.trim() };
            let first = format!("{subject}, the first frame of {motion}");
            steps.push(json!({ "prompt": compose("sprite", &first, key, style_refs.len(), own_refs.len(), 1), "refs": with(&own_refs), "transparent": true }));
            for f in 2..=n {
                steps.push(json!({ "prompt": frame_prompt(&style, subject, motion, f, n, key), "refs": [{ "step": 0 }], "transparent": true }));
            }
        }
        "layers" => {
            let n = layer_count(a);
            // The nearer layers are only their own shapes: the subject's sky
            // and far scenery go into the far layer alone, and so do the
            // look's scenery words (S7: "under a warm amber sky" in every
            // layer's prompt painted a sky into each, hiding the set).
            let scene = layer_scene(subject);
            let fits = |eg: &str| if scene.is_empty() { format!("{eg} or whatever fits the scene") } else { format!("{eg} or whatever fits {scene}") };
            let mut near_style = style.clone();
            near_style.style = without_scenery(&style.style);
            near_style.keywords = style.keywords.iter().map(|k| without_scenery(k)).filter(|k| !k.is_empty()).collect();
            for i in 0..n {
                // How much of the picture each nearer layer may cover: the game
                // draws them all over the far one, so a nearer one that fills
                // the picture hides everything behind it.
                let depth = if i == 0 {
                    format!("{subject} — the farthest layer: the sky and the most distant shapes, filling the whole image, low contrast")
                } else if i == n - 1 {
                    format!(
                        "the nearest layer, a cut-out like a theatre flat: dark foreground silhouettes ({}) along only the bottom fifth of the image, \
                         solid all along the bottom edge, their tops ragged and varied, with crisp hard edges. The top four fifths are the flat background colour and nothing else: \
                         no sky, no distant scenery, no mist, glow or light rays over it",
                        fits("grass, ferns, rocks, roots")
                    )
                } else {
                    format!(
                        "a middle-distance layer ({} of {n} from the back), a cut-out like a theatre flat: a band of mid-distance shapes ({}) \
                         across only the bottom {}% of the image, solid all along the bottom edge, their tops ragged and varied, with crisp hard edges, in muted, hazy colours of the scene. \
                         Everything above them is the flat background colour and nothing else: no sky, no clouds, no mist, glow or light rays over it",
                        i + 1,
                        fits("trunks, hills, buildings"),
                        if n == 4 && i == 1 { 60 } else { 45 }
                    )
                };
                // The nearer layers aren't shown the far one: with it attached,
                // models paint its sky into theirs instead of the flat key
                // (Phase 1d's look check). The look's words keep them one set.
                let refs = with(&[]);
                let (k, look) = if i == 0 { (None, &style) } else { (key, &near_style) };
                let prompt = style::compose(
                    look,
                    &PromptParts { kind: "layer", subject: &depth, frames: 1, expressions: &[], grid, keyed: k, portrait: h >= w, style_refs: refs.len(), subject_refs: 0 },
                );
                steps.push(json!({ "prompt": prompt, "refs": refs, "transparent": i > 0 }));
            }
        }
        "walker" => steps.push(json!({ "prompt": compose("walker", subject, key, style_refs.len(), own_refs.len(), 2), "refs": with(&own_refs), "transparent": true })),
        "portrait" => {
            let rows = portrait_rows(&a.expressions)?;
            let parts = PromptParts { kind: "portrait", subject, frames: 0, expressions: &rows, grid, keyed: key, portrait: true, style_refs: style_refs.len(), subject_refs: own_refs.len() };
            steps.push(json!({ "prompt": style::compose(&style, &parts), "refs": with(&own_refs), "transparent": true }));
        }
        "tile" if a.no_seamless => {
            let mut p = format!("A square 2D game tile: {subject}. It fills the square edge to edge, evenly lit, flat-on.");
            if !style.style.trim().is_empty() {
                p += &format!(" Art style: {}.", style.style.trim().trim_end_matches('.'));
            }
            if style.pixel_art {
                p += &format!(" Pixel art: big crisp square pixels as if drawn on a {w}×{h} grid, no anti-aliasing.");
            }
            p += " Avoid: text, letters, watermark, logo, frame or border. An original design: not any existing character, franchise, brand or logo.";
            steps.push(json!({ "prompt": p, "refs": with(&own_refs), "transparent": false }));
        }
        _ => {
            let k = if kind == "image" { "sprite" } else { kind };
            steps.push(json!({ "prompt": compose(k, subject, key, style_refs.len(), own_refs.len(), 1), "refs": with(&own_refs), "transparent": key.is_some() }));
        }
    }
    let n = if kind == "layers" { layer_count(a) } else { 1 };
    let files: Vec<String> = (0..n).map(|i| main_path(kind, &a.name, i, style.pixel_art)).collect();
    Ok(json!({ "kind": kind, "name": a.name, "key": key.map(img::hex), "files": files, "steps": steps }))
}

/// Frame `f` of `n`, as an edit of frame 1.
fn frame_prompt(style: &Style, subject: &str, motion: &str, f: u32, n: u32, key: Option<[u8; 3]>) -> String {
    let bg = key.map_or("the same plain flat background".to_string(), |k| format!("the same plain flat {} background", img::hex(k)));
    let mut p = format!(
        "The same character as the attached image, in the same art style, size, outline, colours and facing, on {bg}: frame {f} of {n} of {motion}. \
         {subject}. Whole and placed as in the attached image, nothing cut off, no shadow, no text."
    );
    if !style.style.trim().is_empty() {
        p += &format!(" Art style: {}.", style.style.trim().trim_end_matches('.'));
    }
    if style.pixel_art {
        p += " Pixel art: big crisp square pixels, no anti-aliasing.";
    }
    p
}

/// Words that put a sky, or scenery far off, into a picture.
const FAR_WORDS: &[&str] = &[
    "sky", "skies", "skyline", "cloud", "clouds", "cloudy", "sun", "sunset", "sunrise", "moon", "stars", "starry", "aurora", "horizon", "mountain",
    "mountains", "peak", "peaks", "distant", "far", "faraway", "far-off", "background", "backdrop", "vista", "panorama", "rainbow", "galaxy", "nebula",
    "sea", "ocean",
];

/// Words that name a place: a look that says them draws that place into
/// every picture (S7's "rolling hills and pine forests").
const SCENERY_WORDS: &[&str] = &[
    "landscape", "landscapes", "scenery", "scene", "scenes", "countryside", "hill", "hills", "forest", "forests", "woods", "woodland", "woodlands", "valley",
    "valleys", "meadow", "meadows", "field", "fields", "river", "rivers", "lake", "lakes", "coast", "coastline", "beach", "beaches", "desert", "deserts",
    "jungle", "jungles", "city", "cities", "cityscape", "town", "towns", "village", "villages", "trees", "pines", "cliffs", "canyon", "canyons", "island",
    "islands", "glade", "glades", "swamp", "marsh", "tundra",
];

/// A description cut into its phrases: at punctuation and at the words that
/// join a second place on ("under a warm amber sky", "with pine forests").
fn phrases(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in text.split([',', ';', '.', ':', '—', '–', '(', ')']) {
        let mut cur: Vec<&str> = Vec::new();
        for word in part.split_whitespace() {
            if matches!(word.to_lowercase().as_str(), "under" | "beneath" | "below" | "against" | "above" | "with" | "and" | "beyond" | "over") {
                if !cur.is_empty() {
                    out.push(cur.join(" "));
                }
                cur.clear();
            } else {
                cur.push(word);
            }
        }
        if !cur.is_empty() {
            out.push(cur.join(" "));
        }
    }
    out
}

fn names_any(phrase: &str, words: &[&str]) -> bool {
    phrase.split(|c: char| !c.is_alphanumeric() && c != '-').any(|w| words.contains(&w.to_lowercase().as_str()))
}

/// What a nearer layer is told the scene is: the subject without its sky and
/// far scenery ("rolling hills at dusk with pine forests, a far range of
/// blue mountains under an amber sky" -> "rolling hills at dusk, pine
/// forests"); empty when nothing's left.
fn layer_scene(subject: &str) -> String {
    phrases(subject).into_iter().filter(|p| !names_any(p, FAR_WORDS)).collect::<Vec<_>>().join(", ")
}

/// The look for a nearer layer: how things are drawn, without the places it
/// names. A phrase that names one keeps its other words when two or more
/// are left ("soft storybook landscapes" -> "soft storybook").
fn without_scenery(look: &str) -> String {
    let all: Vec<&str> = FAR_WORDS.iter().chain(SCENERY_WORDS).copied().collect();
    phrases(look)
        .into_iter()
        .filter_map(|p| {
            if !names_any(&p, &all) {
                return Some(p);
            }
            let left: Vec<&str> = p.split_whitespace().filter(|w| !names_any(w, &all)).collect();
            let content = left.iter().filter(|w| !matches!(w.to_lowercase().as_str(), "a" | "an" | "the" | "of" | "in" | "at" | "on" | "to")).count();
            (content >= 2).then(|| left.join(" "))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn check_kind(kind: &str, t: &Table) -> Result<(), String> {
    if !KINDS.contains(&kind) {
        return Err(format!("unknown kind {kind:?}: {}, voice, music or sfx", KINDS.join(", ")));
    }
    if arcade(t) && !matches!(kind, "image" | "frames" | "tile" | "icon") {
        return Err(format!(
            "the Arcade preset's art is 16×16 or 32×32 pictures and 16×16 tiles in the project's 16 colours: no {kind}. Without preset = \"arcade\" every kind works"
        ));
    }
    Ok(())
}

// --- fitting -----------------------------------------------------------------

/// One candidate's outputs before they're written: (path, bytes, the sidecar's facts).
struct Made {
    files: Vec<(String, Vec<u8>, Value)>,
    toml: Option<Value>,
    view: Option<Image>,
    size: [u32; 2],
    frames: u32,
    warnings: Vec<String>,
}

/// Fits every `--candidate` (its steps' raw files, in order) and writes them
/// under DIR/<n>/, with sidecars; the report.
pub fn process(a: &AssetArgs, t: &Table) -> Result<Value, String> {
    let kind = a.kind.as_str();
    if kind == "voice" {
        return super::asset_voice::process(a);
    }
    check_kind(kind, t)?;
    if a.candidates.is_empty() {
        return Err("--candidate: at least one".into());
    }
    let style = load_style(&a.project, a, t)?;
    let palette = style.palette_rgb()?;
    let calls = super::asset::Made::read(a)?;
    let mut out = Vec::new();
    for (i, c) in a.candidates.iter().enumerate() {
        let n = i + 1;
        let mut raws = Vec::new();
        let mut raw_bytes = Vec::new();
        for f in c.split(',').filter(|s| !s.is_empty()) {
            let bytes = std::fs::read(f).map_err(|e| format!("{f}: {e}"))?;
            raws.push(Image::decode(&bytes).map_err(|e| format!("{f}: {e}"))?);
            raw_bytes.push(bytes);
        }
        if raws.is_empty() {
            return Err(format!("candidate {n} names no files"));
        }
        let made = if arcade(t) { fit_arcade(a, t, &raws)? } else { fit(a, &style, &palette, &raws)? };
        let dir = a.out.join(n.to_string());
        let mut files = Vec::new();
        for (f, (path, bytes, facts)) in made.files.iter().enumerate() {
            let dest = dir.join(path);
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
            // A layer is its own step's picture; anything else is made of every step's.
            let steps: Vec<usize> = if kind == "layers" && !arcade(t) { vec![f] } else { (0..raw_bytes.len()).collect() };
            let raw: Vec<u8> = steps.iter().filter_map(|&s| raw_bytes.get(s)).flatten().copied().collect();
            let made_by: Vec<super::asset::Call> = steps.iter().filter_map(|&s| calls.call(i, s).cloned()).collect();
            let mut facts = facts.clone();
            if !made_by.is_empty() {
                facts["calls"] = json!(made_by);
            }
            let first = steps.first().copied().unwrap_or(0);
            let prompt = calls.prompt(first, &a.full_prompt);
            let seed = calls.call(i, first).map_or(a.seed, |c| c.seed);
            sidecar(a, &dest, bytes, &raw, prompt, seed, super::asset::cost_of(&made_by, a.cost), facts.clone())?;
            let mut entry = json!({ "file": format!("{n}/{path}"), "path": path });
            if let Some(cover) = facts.get("cover") {
                entry["cover"] = cover.clone();
            }
            files.push(entry);
        }
        let view = match &made.view {
            Some(v) => {
                let (name, bytes) = if v.is_opaque() { ("view.jpg", crate::generate::sky::encode_jpeg(v, 85)?) } else { ("view.png", v.encode_png()) };
                std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
                Value::from(format!("{n}/{name}"))
            }
            None => Value::Null,
        };
        out.push(json!({
            "files": files, "toml": made.toml, "view": view, "width": made.size[0], "height": made.size[1], "frames": made.frames, "warnings": made.warnings
        }));
    }
    let mut report = json!({ "kind": kind, "name": a.name, "candidates": out });
    if kind == "layers" && !arcade(t) {
        report["key"] = json!(img::hex(style.key_color()?));
    }
    Ok(report)
}

/// `mb art`'s recipe for one picture, from what the server asked for.
fn job(kind: &str, a: &AssetArgs, size: [u32; 2], style: &Style, keyed: bool, extra: Value) -> Result<Job, String> {
    let mut j = json!({
        "kind": kind, "name": a.name, "subject": a.prompt, "out": format!("images/{}.png", a.name), "size": size,
        "seed": a.seed, "provider": a.provider, "model": a.model, "resolution": "1K", "aspect": "1:1",
        "keyed": keyed, "key": if keyed { Some(img::hex(style.key_color()?)) } else { None },
        "grid": if style.pixel_art && kind != "icon" { Some(size) } else { None },
        "margin": if style.pixel_art { 1 } else { (size[0].min(size[1]) / 16).max(1) },
    });
    if let (Value::Object(m), Value::Object(e)) = (&mut j, extra) {
        m.extend(e);
    }
    serde_json::from_value(j).map_err(|e| format!("recipe: {e}"))
}

fn warnings_of(facts: &Value) -> Vec<String> {
    facts["warnings"].as_array().map(|w| w.iter().filter_map(|s| s.as_str().map(studio_words)).collect()).unwrap_or_default()
}

/// A warning of `mb art`'s in Studio's words: its command-line fixes
/// (`mb regen …`, `--tolerance`, `--size`) are generating it again here.
fn studio_words(w: &str) -> String {
    let mut out = String::new();
    let mut rest = w;
    while let Some(at) = rest.find('(') {
        let Some(end) = rest[at..].find(')') else {
            break;
        };
        let inner = &rest[at + 1..at + end];
        out.push_str(&rest[..at]);
        if !(inner.starts_with("mb ") || inner.contains("--")) {
            out.push_str(&rest[at..=at + end]);
        }
        rest = &rest[at + end + 1..];
    }
    out.push_str(rest);
    out.replace("raise --tolerance  or regenerate", "generate it again")
        .replace("ask for a taller --size", "generate it again with its shapes lower down")
        .replace("set a different `key` in art/style.toml", "describe it in other colours")
        .replace("Regenerate", "Generate it again")
        .replace("regenerate", "generate it again")
        .replace(" .", ".")
        .replace(" ;", ";")
        .trim_end()
        .to_string()
}

/// What a fitted picture's sidecar keeps: `mb art`'s facts without its Rust hints.
fn facts_for(mut facts: Value) -> Value {
    if let Value::Object(m) = &mut facts {
        m.remove("rust");
        m.remove("stand");
        m.remove("ground");
    }
    facts
}

fn fit(a: &AssetArgs, style: &Style, palette: &[[u8; 3]], raws: &[Image]) -> Result<Made, String> {
    let kind = a.kind.as_str();
    let size = if kind == "icon" { [256, 256] } else { dims(a)? };
    let pixel = style.pixel_art;
    let mut set = serde_json::Map::new();
    let mut remove: Vec<&str> = Vec::new();
    let mut warnings = Vec::new();
    match (pixel, kind) {
        (_, "tile") => {}
        (true, _) => {
            set.insert("pixel".into(), json!(true));
        }
        (false, _) => remove.push("pixel"),
    }
    let one = || -> Result<&Image, String> { raws.first().filter(|_| raws.len() == 1).ok_or_else(|| format!("a {kind} candidate is one image")) };
    let mut frames = 1;
    let mut view = None;
    let files: Vec<(String, Vec<u8>, Value)> = match kind {
        "image" | "ui" => {
            let anchor = if a.stands { "bottom" } else { "center" };
            let j = job(if kind == "ui" { "ui" } else { "sprite" }, a, size, style, true, json!({ "anchor": anchor }))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            remove.extend(["frame", "walker", "body", "foot"]);
            let mut facts = facts_for(facts);
            footing(&im, [im.w, im.h], 1, a, &mut set, &mut remove, &mut facts);
            vec![(main_path(kind, &a.name, 0, pixel), im.encode_png(), facts)]
        }
        "frames" => {
            if size[0] * raws.len() as u32 > MAX_IMAGE {
                return Err(format!("{} frames {} wide is over {MAX_IMAGE} pixels: fewer frames or a smaller size", raws.len(), size[0]));
            }
            let sheet = frame_sheet(raws, size, pixel, style.key_color()?, &mut warnings);
            frames = raws.len() as u32;
            set.insert("frame".into(), json!(size));
            remove.extend(["walker", "body", "foot"]);
            let mut facts = json!({ "frames": frames, "cell": size, "size": [sheet.w, sheet.h], "warnings": warnings.clone() });
            footing(&sheet, size, frames, a, &mut set, &mut remove, &mut facts);
            vec![(main_path(kind, &a.name, 0, pixel), sheet.encode_png(), facts)]
        }
        "walker" => {
            let j = job("frames", a, size, style, true, json!({ "frames": 2, "cols": 2, "walker": true, "anchor": "bottom" }))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            let w = facts["walker"].clone();
            if w.is_null() {
                warnings.push("the body or the foot came out empty: generate it again".into());
            } else {
                set.insert("walker".into(), json!(true));
                set.insert("frame".into(), json!(size));
                set.insert("body".into(), w["body"].clone());
                set.insert("foot".into(), w["foot"].clone());
                view = Some(standing(&im, &w));
            }
            remove.push("footing");
            frames = 2;
            vec![(main_path(kind, &a.name, 0, pixel), im.encode_png(), facts_for(facts))]
        }
        "portrait" => {
            if size[0] != size[1] {
                return Err(format!("a portrait's cells are square, not {}×{}", size[0], size[1]));
            }
            let rows = portrait_rows(&a.expressions)?;
            let n = rows.len() as u32 + 1;
            if size[1] * n > MAX_IMAGE {
                return Err(format!("{n} rows of {}-pixel cells is over {MAX_IMAGE} pixels: fewer expressions or smaller cells", size[1]));
            }
            let j = job("portrait", a, size, style, true, json!({ "frames": 4 * n, "cols": 4, "expressions": rows, "anchor": "bottom" }))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            let mut names = rows.clone();
            names.push("blink".into());
            set.insert("frame".into(), json!(size));
            set.insert("portrait".into(), json!(names));
            remove.extend(["walker", "body", "foot", "footing"]);
            frames = 4 * n;
            view = Some(portrait_view(&im, size, n, pixel));
            vec![(main_path(kind, &a.name, 0, pixel), im.encode_png(), facts_for(facts))]
        }
        "sky" => {
            let j = job("sky", a, [2048, 1024], style, false, json!({ "aspect": "21:9", "grid": null, "horizon": a.horizon, "span": 180.0, "poles": 12.0 }))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            let path = main_path(kind, &a.name, 0, false);
            vec![(path.clone(), crate::generate::sky::encode_budget(&im, SKY_KB).map(|(b, _)| b)?, facts_for(facts))]
        }
        "background" => {
            let j = job("background", a, size, style, false, json!({}))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            let path = main_path(kind, &a.name, 0, pixel);
            vec![(path.clone(), encode(&im, &path, false)?, facts_for(facts))]
        }
        "layers" => {
            let n = raws.len() as u32;
            if !(2..=4).contains(&n) {
                return Err(format!("layers come 2 to 4 at a time, not {n}"));
            }
            let mut out = Vec::new();
            let mut fitted = Vec::new();
            for (i, raw) in raws.iter().enumerate() {
                let i = i as u32;
                let j = job("layer", a, size, style, i > 0, json!({ "layer": [i, n], "name": format!("{}_{i}", a.name) }))?;
                let (im, facts) = art::process(&j, style, palette, raw)?;
                warnings.extend(warnings_of(&facts).into_iter().map(|w| format!("layer {i}: {w}")));
                let path = main_path(kind, &a.name, i, pixel);
                out.push((path.clone(), encode(&im, &path, i > 0)?, facts_for(facts)));
                fitted.push(im);
            }
            view = Some(stacked(&fitted));
            out
        }
        "tile" => {
            let raw = one()?;
            let tile = if a.no_seamless {
                let base = if raw.is_opaque() { raw.clone() } else { raw.flatten([0, 0, 0]) };
                if pixel { img::pixelate(&base.cover(size[0] * 8, size[1] * 8), size[0], size[1]) } else { base.cover(size[0], size[1]) }
            } else {
                let j = job("tile", a, size, style, false, json!({}))?;
                let (im, facts) = art::process(&j, style, palette, raw)?;
                warnings.extend(warnings_of(&facts));
                im
            };
            view = Some(repeated(&tile, 3));
            let (sheet, count) = into_set(&a.project, &a.name, &tile, a.frame)?;
            frames = count;
            set.insert("frame".into(), json!(size));
            if pixel {
                remove.push("pixel");
            } else {
                set.insert("pixel".into(), json!(false));
            }
            let facts = json!({ "tile": size, "tiles": count, "wraps": if a.no_seamless { "no" } else { "xy" }, "warnings": warnings.clone() });
            vec![(main_path(kind, &a.name, 0, pixel), sheet.encode_png(), facts)]
        }
        "icon" => {
            let j = job("icon", a, size, style, false, json!({}))?;
            let (im, facts) = art::process(&j, style, palette, one()?)?;
            warnings.extend(warnings_of(&facts));
            vec![("icon.png".into(), im.encode_png(), facts_for(facts))]
        }
        k => return Err(format!("unknown kind {k:?}")),
    };
    let size = match kind {
        "frames" | "walker" | "tile" => [size[0] * frames, size[1]],
        "portrait" => [size[0] * 4, size[1] * (frames / 4)],
        _ => size,
    };
    let toml = toml_for(&files[0].0, kind, set, remove, &a.project);
    Ok(Made { files, toml, view, size, frames, warnings })
}

/// The keys a picture's `.toml` gets (none when there's nothing to set and no file yet).
fn toml_for(main: &str, kind: &str, set: serde_json::Map<String, Value>, remove: Vec<&str>, project: &Path) -> Option<Value> {
    let path = toml_path(main);
    if kind == "icon" || (set.is_empty() && !project.join(&path).is_file()) {
        return None;
    }
    Some(json!({ "path": path, "set": set, "remove": remove }))
}

/// `footing = [x, y]` for a thing that stands (`--stands`): the middle of
/// its lowest row, in its frame's pixels. Its height in the world, when the
/// prompt names something with a typical one, goes in the sidecar.
fn footing(im: &Image, cell: [u32; 2], cells: u32, a: &AssetArgs, set: &mut serde_json::Map<String, Value>, remove: &mut Vec<&str>, facts: &mut Value) {
    let found = if a.stands { crate::generate::ground::footing(im, cell, cells) } else { None };
    match found {
        Some(f) => {
            let at = [f.foot[0].round() as u32, f.foot[1].round() as u32];
            set.insert("footing".into(), json!(at));
            facts["footing"] = json!({ "at": at, "base": f.base, "top": f.top });
            let real = crate::generate::scene::real_json(None, &a.prompt);
            if !real.is_null() {
                facts["real"] = real;
            }
        }
        None => remove.push("footing"),
    }
}

/// Frames side by side in `cell`-sized cells: each keyed, all at one scale,
/// standing on the cell's bottom margin (pixel art: on a grid of `cell` art pixels).
fn frame_sheet(raws: &[Image], cell: [u32; 2], pixel: bool, key: [u8; 3], warnings: &mut Vec<String>) -> Image {
    let hint = img::KeyHint { expected: Some(key), top: false };
    let keyed: Vec<Image> = raws
        .iter()
        .map(|r| {
            if r.has_transparent_border() {
                return r.trimmed();
            }
            let (k, rep) = img::key_out_with(r, 0.09, hint);
            if rep.cleared < 0.05 {
                warnings.push("almost nothing was keyed out of a frame: its background may not be flat".into());
            }
            k.trimmed()
        })
        .collect();
    let work = if pixel { 8 } else { 1 };
    let (cw, ch) = (cell[0] * work, cell[1] * work);
    let margin = if pixel { work } else { (cell[0].min(cell[1]) / 16).max(1) };
    let maxw = keyed.iter().map(|k| k.w).max().unwrap_or(1).max(1) as f32;
    let maxh = keyed.iter().map(|k| k.h).max().unwrap_or(1).max(1) as f32;
    let k = ((cw - 2 * margin) as f32 / maxw).min((ch - 2 * margin) as f32 / maxh);
    let mut sheet = Image::new(cell[0] * keyed.len() as u32, cell[1], [0; 4]);
    for (i, p) in keyed.iter().enumerate() {
        let placed = art::place(p, k, cw, ch, margin, Anchor::Bottom);
        let c = if pixel { img::pixelate(&placed, cell[0], cell[1]) } else { placed };
        sheet.blit(&c, i as u32 * cell[0], 0);
    }
    sheet
}

/// A JPEG for an opaque painting under `images/*.jpg`, else a PNG; a keyed
/// layer's PNG quantized when it's big (the startup set is 3 MB).
fn encode(im: &Image, path: &str, keyed: bool) -> Result<Vec<u8>, String> {
    if path.ends_with(".jpg") {
        return crate::generate::sky::encode_budget(im, JPEG_KB).map(|(b, _)| b);
    }
    let png = im.encode_png();
    if keyed && png.len() > PNG_KB * 1024 {
        let mut q = im.clone();
        img::quantize(&mut q, 256);
        return Ok(q.encode_png());
    }
    Ok(png)
}

/// Layers stacked far to near, as the game draws them, turned half a width
/// so where each one wraps is in the middle; at most VIEW wide.
fn stacked(layers: &[Image]) -> Image {
    let (w, h) = (layers[0].w, layers[0].h);
    let mut out = Image::new(w, h, [0, 0, 0, 255]);
    for l in layers {
        for y in 0..h.min(l.h) {
            for x in 0..w {
                let p = l.get((x + w / 2) % l.w, y);
                out.set(x, y, img::over(p, out.get(x, y)));
            }
        }
    }
    fit_view(&out)
}

/// A tile repeated n×n, so its seams show.
fn repeated(t: &Image, n: u32) -> Image {
    let mut out = Image::new(t.w * n, t.h * n, [0; 4]);
    for j in 0..n {
        for i in 0..n {
            out.blit(t, i * t.w, j * t.h);
        }
    }
    enlarge(&out)
}

/// A walker as `Walker{}` stands it: the body over a foot at each side.
fn standing(sheet: &Image, w: &Value) -> Image {
    let r = |k: &str| -> [u32; 4] { std::array::from_fn(|i| w[k][i].as_u64().unwrap_or(0) as u32) };
    let (b, f) = (r("body"), r("foot"));
    let body = sheet.crop(b[0], b[1], b[2].max(1), b[3].max(1));
    let foot = sheet.crop(f[0], f[1], f[2].max(1), f[3].max(1));
    let width = body.w.max(foot.w * 3) + 8;
    let height = body.h + foot.h / 2 + 8;
    let mut out = Image::new(width, height, [0; 4]);
    let bx = (width - body.w) / 2;
    let ground = height - 4;
    for fx in [bx + body.w / 4, bx + body.w * 3 / 4] {
        let x = fx.saturating_sub(foot.w / 2).min(width - foot.w);
        over_at(&mut out, &foot, x, ground - foot.h);
    }
    over_at(&mut out, &body, bx, ground.saturating_sub(foot.h / 2 + body.h));
    enlarge(&out)
}

/// A portrait as the judge and the card see it (§7.4): the sheet, and beside
/// it two cells enlarged, the neutral face with its mouth open and the blink.
fn portrait_view(sheet: &Image, cell: [u32; 2], rows: u32, pixel: bool) -> Image {
    const TALL: u32 = 768;
    let scale = |im: &Image, h: u32| -> Image {
        let w = ((im.w as f32 * h as f32 / im.h.max(1) as f32).round() as u32).max(1);
        if pixel && h % im.h == 0 { im.scale_nearest(h / im.h) } else { im.resize(w, h) }
    };
    let side = TALL / 2;
    let open = scale(&sheet.crop(2 * cell[0], 0, cell[0], cell[1]), side);
    let blink = scale(&sheet.crop(0, (rows - 1) * cell[1], cell[0], cell[1]), side);
    let whole = scale(sheet, if pixel { (TALL / sheet.h).max(1) * sheet.h } else { TALL });
    let gap = 16;
    let mut out = Image::new(whole.w + gap + open.w.max(blink.w), whole.h.max(open.h + blink.h), [0; 4]);
    out.blit(&whole, 0, 0);
    out.blit(&open, whole.w + gap, 0);
    out.blit(&blink, whole.w + gap, open.h);
    fit_view(&out)
}

fn over_at(dst: &mut Image, src: &Image, x: u32, y: u32) {
    for j in 0..src.h {
        for i in 0..src.w {
            if x + i < dst.w && y + j < dst.h {
                let d = dst.get(x + i, y + j);
                dst.set(x + i, y + j, img::over(src.get(i, j), d));
            }
        }
    }
}

/// Small pictures scaled up by whole pixels to about 256, big ones down to VIEW.
fn enlarge(im: &Image) -> Image {
    let k = (256 / im.w.max(im.h)).max(1);
    if k > 1 { im.scale_nearest(k) } else { fit_view(im) }
}

fn fit_view(im: &Image) -> Image {
    let long = im.w.max(im.h);
    if long <= VIEW {
        return im.clone();
    }
    let k = VIEW as f32 / long as f32;
    im.resize(((im.w as f32 * k).round() as u32).max(1), ((im.h as f32 * k).round() as u32).max(1))
}

/// The tile set `tiles/<name>.png` with `tile` as frame `frame` (from 1:
/// replacing it, or one past the last, added); a new set without `frame` or
/// without a set.
fn into_set(project: &Path, name: &str, tile: &Image, frame: Option<u32>) -> Result<(Image, u32), String> {
    let Some(f) = frame else {
        return Ok((tile.clone(), 1));
    };
    let rel = format!("tiles/{name}.png");
    let found = if inside(project, &rel) { std::fs::read(project.join(&rel)) } else { Err(std::io::ErrorKind::NotFound.into()) };
    let set = match found {
        Ok(b) => Image::decode(&b).map_err(|e| format!("tiles/{name}.png: {e}"))?,
        Err(_) if f <= 1 => return Ok((tile.clone(), 1)),
        Err(_) => {
            return Err(format!("tiles/{name}.png doesn't exist yet, so its first tile is frame 1"));
        }
    };
    let (tw, th) = (tile.w, tile.h);
    if set.h != th || set.w % tw != 0 {
        return Err(format!("tiles/{name}.png is {}×{}: its tiles are {}×{}, and this one is {tw}×{th}; make it the set's size", set.w, set.h, set.h, set.h));
    }
    let count = set.w / tw;
    if f == 0 || f > count + 1 {
        return Err(format!("tiles/{name}.png has {count} tiles (frames 1–{count}): frame {} adds one", count + 1));
    }
    let total = count.max(f);
    if tw * total > MAX_IMAGE {
        return Err(format!("tiles/{name}.png would be {}×{th} with {total} tiles: a picture is at most {MAX_IMAGE} wide, so start another set", tw * total));
    }
    let mut out = Image::new(tw * total, th, [0; 4]);
    out.blit(&set, 0, 0);
    out.blit(tile, (f - 1) * tw, 0);
    Ok((out, total))
}

/// The Arcade preset (§4.9): 1.0's pixel art on the palette, at 2.0's paths.
fn fit_arcade(a: &AssetArgs, t: &Table, raws: &[Image]) -> Result<Made, String> {
    let (pal, _) = super::palette(t)?;
    let palette: Vec<[u8; 3]> = pal.iter().map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect();
    let background = t.get("background").and_then(Toml::as_integer).unwrap_or(0).clamp(0, 15) as usize;
    let mut warnings = Vec::new();
    let (path, im, frames) = match a.kind.as_str() {
        "image" | "frames" => {
            let size = a.size.unwrap_or(16);
            if size != 16 && size != 32 {
                return Err("Arcade pictures are 16 or 32 pixels".into());
            }
            if a.kind == "image" && raws.len() > 1 {
                return Err("an image is one picture: use frames for more".into());
            }
            if raws.len() > 8 {
                return Err(format!("{} frames: Arcade pictures have 8 at most", raws.len()));
            }
            (format!("images/{}.png", a.name), super::asset::sprite_sheet(raws, size, &palette, &a.key, &mut warnings), raws.len() as u32)
        }
        "tile" => {
            let (set, count) = into_set(&a.project, &a.name, &super::asset::tile(&raws[0], &palette), a.frame)?;
            (format!("tiles/{}.png", a.name), set, count)
        }
        _ => ("icon.png".to_string(), super::asset::icon(&raws[0], &palette, palette[background]), 1),
    };
    let facts = json!({ "size": [im.w, im.h], "frames": frames, "warnings": warnings.clone() });
    Ok(Made { size: [im.w, im.h], files: vec![(path, im.encode_png(), facts)], toml: None, view: None, frames, warnings })
}

/// A file's sidecar: `prompt` and `seed` are what its own provider call was
/// sent (a layer's, an option's), `raw` the bytes it came back as.
fn sidecar(a: &AssetArgs, dest: &Path, out: &[u8], raw: &[u8], prompt: &str, seed: u64, cost: Option<f64>, result: Value) -> Result<(), String> {
    let recipe = json!({
        "kind": a.kind, "engine": "2.0", "dims": a.dims, "pixel": a.pixel, "frames": a.frames, "motion": a.motion,
        "count": a.count, "stands": a.stands, "seamless": !a.no_seamless, "frame": a.frame, "refs": a.refs, "layer_key": a.layer_key,
    });
    let mut recipe = recipe;
    if a.kind == "portrait" {
        recipe["expressions"] = json!(portrait_rows(&a.expressions)?);
    }
    let sc = Sidecar {
        generator: format!("mb {} studio-asset {}", env!("CARGO_PKG_VERSION"), a.kind),
        date: crate::generate::now_utc(),
        provider: a.provider.clone(),
        model: a.model.clone(),
        via: Some("maimbrain".into()),
        prompt: a.prompt.clone(),
        full_prompt: prompt.to_string(),
        seed,
        seed_honored: true,
        references: Vec::new(),
        style: None,
        est_cost_usd: cost,
        cost_usd: cost,
        raw: "(not kept: made by the Maimbrain server)".into(),
        raw_sha256: sha256_hex(raw),
        output_sha256: sha256_hex(out),
        allowed_names: Vec::new(),
        model_text: None,
        recipe,
        result,
        note: String::new(),
        derived: Vec::new(),
    };
    crate::generate::write_sidecar(dest, &sc)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mb-studio-asset2-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("studio.toml"), "title = \"T\"\nengine = \"2.0\"\nkind = \"2d\"\norientation = \"landscape\"\n").unwrap();
        std::fs::write(dir.join("DESIGN.md"), "# T\n\nA game.\n\n## Look\n\nstorybook gouache at dusk\n\nMore.\n").unwrap();
        dir
    }

    pub(crate) fn args(dir: &Path, kind: &str, dims: [u32; 2], candidates: Vec<String>) -> AssetArgs {
        AssetArgs {
            project: dir.to_path_buf(),
            kind: kind.into(),
            name: "fox".into(),
            out: dir.join("out"),
            candidates,
            size: None,
            frame: None,
            looping: false,
            key: "#ff00ff".into(),
            prompt: "a fox".into(),
            full_prompt: String::new(),
            provider: "gemini".into(),
            model: "test".into(),
            seed: 7,
            cost: None,
            plan: false,
            dims: Some(dims),
            pixel: false,
            frames: None,
            motion: String::new(),
            count: None,
            stands: false,
            no_seamless: false,
            horizon: None,
            refs: Vec::new(),
            expressions: Vec::new(),
            voice: Default::default(),
            zones: None,
            sections: None,
            setlist: None,
            layers: None,
            bpm: None,
            far: Vec::new(),
            layer_key: None,
            made: None,
        }
    }

    fn rect(im: &mut Image, x: u32, y: u32, w: u32, h: u32, c: [u8; 4]) {
        for j in y..y + h {
            for i in x..x + w {
                im.set(i, j, c);
            }
        }
    }

    fn write(dir: &Path, name: &str, im: &Image) -> String {
        let p = dir.join(name);
        std::fs::write(&p, im.encode_png()).unwrap();
        p.display().to_string()
    }

    fn table(dir: &Path) -> Table {
        super::super::read_toml(&dir.join("studio.toml")).unwrap()
    }

    /// A model's portrait sheet: 4 mouths × the rows, busts on magenta, the last row's eyes shut.
    fn portrait_raw(rows: u32) -> Image {
        let cell = 256;
        let mut im = Image::new(4 * cell, rows * cell, [255, 0, 255, 255]);
        for r in 0..rows {
            for c in 0..4 {
                let (x, y) = (c * cell, r * cell);
                rect(&mut im, x + 48, y + 170, 160, 70, [60, 90, 160, 255]);
                rect(&mut im, x + 78, y + 50, 100, 120, [230, 180, 140, 255]);
                if r + 1 < rows {
                    rect(&mut im, x + 98, y + 90, 14, 10, [250, 250, 250, 255]);
                    rect(&mut im, x + 144, y + 90, 14, 10, [250, 250, 250, 255]);
                }
                rect(&mut im, x + 108, y + 140, 40, 2 + c * 6, [90, 20, 20, 255]);
            }
        }
        im
    }

    #[test]
    fn a_portrait_is_a_sheet_of_mouths_and_rows_with_their_names_in_its_toml() {
        let dir = project("portrait");
        let mut a = args(&dir, "portrait", [192, 192], Vec::new());
        a.name = "gruk".into();
        a.prompt = "a bored old troll".into();
        a.expressions = vec!["Smug".into(), "annoyed".into(), "neutral".into()];
        let p = plan(&a, &table(&dir)).unwrap();
        assert_eq!(p["files"], json!(["images/portraits/gruk.png"]));
        let prompt = p["steps"][0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("4 columns and 4 rows") && prompt.contains("row 2 smug") && prompt.contains("eyes fully closed"), "{prompt}");
        a.candidates = vec![write(&dir, "raw.png", &portrait_raw(4))];
        let r = process(&a, &table(&dir)).unwrap();
        let c = &r["candidates"][0];
        assert_eq!(c["files"][0]["path"], "images/portraits/gruk.png");
        assert_eq!((c["width"].as_u64(), c["height"].as_u64()), (Some(768), Some(768)), "4 cells of 192 across, 4 rows");
        assert_eq!(c["toml"]["path"], "images/portraits/gruk.toml");
        assert_eq!(c["toml"]["set"], json!({ "frame": [192, 192], "portrait": ["neutral", "smug", "annoyed", "blink"] }));
        assert!(c["toml"]["remove"].as_array().unwrap().contains(&json!("pixel")));
        let view = Image::decode(&std::fs::read(dir.join("out").join(c["view"].as_str().unwrap())).unwrap()).unwrap();
        assert!(view.w > view.h, "the sheet with two cells enlarged beside it: {}×{}", view.w, view.h);
        let sheet = Image::decode(&std::fs::read(dir.join("out/1/images/portraits/gruk.png")).unwrap()).unwrap();
        assert_eq!(sheet.get(0, 0)[3], 0, "keyed");

        a.pixel = true;
        a.dims = Some([48, 48]);
        let r = process(&a, &table(&dir)).unwrap();
        assert_eq!(r["candidates"][0]["toml"]["set"]["pixel"], true);
        assert_eq!(r["candidates"][0]["width"], 192);

        a.expressions = (0..7).map(|i| format!("e{i}")).collect();
        assert!(plan(&a, &table(&dir)).unwrap_err().contains("at most 7"));
        a.expressions = vec!["blink".into()];
        assert!(plan(&a, &table(&dir)).is_err());
    }

    #[test]
    fn the_prompts_have_the_look_and_the_layers_cover_less_the_nearer_they_are() {
        let dir = project("prompts");
        let mut a = args(&dir, "layers", [2560, 720], Vec::new());
        a.count = Some(3);
        a.prompt = "a forest path".into();
        let p = plan(&a, &table(&dir)).unwrap();
        let steps = p["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(p["files"], json!(["images/fox_0.jpg", "images/fox_1.png", "images/fox_2.png"]));
        assert_eq!(p["key"], "#ff00ff", "magenta without a palette");
        assert!(steps.iter().all(|s| s["prompt"].as_str().unwrap().contains("storybook gouache at dusk")), "DESIGN.md's look");
        assert!(steps.iter().all(|s| s["refs"].as_array().unwrap().is_empty()), "no layer is shown another");
        assert!(steps[1]["prompt"].as_str().unwrap().contains("bottom 45%") && steps[2]["prompt"].as_str().unwrap().contains("bottom fifth"));
        let mut f = args(&dir, "frames", [96, 96], Vec::new());
        f.frames = Some(3);
        let p = plan(&f, &table(&dir)).unwrap();
        assert_eq!(p["steps"][2]["refs"], json!([{ "step": 0 }]), "each frame an edit of frame 1");
    }

    #[test]
    fn a_sky_is_a_two_to_one_jpeg() {
        let dir = project("sky");
        let mut raw = Image::new(2016, 864, [120, 170, 230, 255]);
        rect(&mut raw, 0, 432, 2016, 432, [70, 90, 60, 255]);
        let a = args(&dir, "sky", [2048, 1024], vec![write(&dir, "s.png", &raw)]);
        let p = plan(&a, &table(&dir)).unwrap();
        assert_eq!(p["files"], json!(["images/fox.jpg"]));
        let r = process(&a, &table(&dir)).unwrap();
        let c = &r["candidates"][0];
        assert_eq!(c["files"][0]["path"], "images/fox.jpg");
        let bytes = std::fs::read(a.out.join("1/images/fox.jpg")).unwrap();
        let im = Image::decode(&bytes).unwrap();
        assert_eq!((im.w, im.h), (2048, 1024));
        assert!(im.is_opaque());
    }

    #[test]
    fn a_walker_is_measured_and_shown_standing() {
        let dir = project("walker");
        let mut raw = Image::new(768, 384, [255, 0, 255, 255]);
        rect(&mut raw, 100, 90, 150, 220, [200, 90, 40, 255]);
        rect(&mut raw, 560, 280, 40, 24, [90, 50, 30, 255]);
        let a = args(&dir, "walker", [128, 128], vec![write(&dir, "w.png", &raw)]);
        let r = process(&a, &table(&dir)).unwrap();
        let c = &r["candidates"][0];
        let set = &c["toml"]["set"];
        assert_eq!((set["walker"].clone(), set["frame"].clone()), (json!(true), json!([128, 128])));
        let (body, foot) = (set["body"].as_array().unwrap(), set["foot"].as_array().unwrap());
        assert!(body[3].as_u64().unwrap() > 3 * foot[3].as_u64().unwrap(), "{body:?} {foot:?}");
        assert!(foot[0].as_u64().unwrap() >= 128, "the foot is the second cell");
        assert_eq!(c["files"][0]["path"], "images/fox.png");
        assert!(dir.join("out").join(c["view"].as_str().unwrap()).is_file());
        assert!(dir.join("out/1/images/fox.png.gen.json").is_file());
    }

    #[test]
    fn layers_are_a_jpeg_and_keyed_pngs_and_their_view_is_stacked() {
        let dir = project("layers");
        let mut far = Image::new(2048, 880, [90, 70, 140, 255]);
        rect(&mut far, 0, 600, 2048, 280, [30, 50, 60, 255]);
        let mut near = Image::new(2048, 880, [255, 0, 255, 255]);
        rect(&mut near, 0, 700, 2048, 180, [20, 30, 30, 255]);
        let (f, n) = (write(&dir, "far.png", &far), write(&dir, "near.png", &near));
        let mut a = args(&dir, "layers", [2560, 720], vec![format!("{f},{n},{n}")]);
        a.count = Some(3);
        let r = process(&a, &table(&dir)).unwrap();
        let c = &r["candidates"][0];
        let paths: Vec<&str> = c["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["images/fox_0.jpg", "images/fox_1.png", "images/fox_2.png"]);
        let near = Image::decode(&std::fs::read(dir.join("out/1/images/fox_1.png")).unwrap()).unwrap();
        assert_eq!((near.w, near.h), (2560, 720));
        assert_eq!(near.get(1280, 10)[3], 0, "the key is clear");
        let view = Image::decode(&std::fs::read(dir.join("out").join(c["view"].as_str().unwrap())).unwrap()).unwrap();
        assert_eq!((view.w, view.h), (1024, 288));
        assert!(c["toml"].is_null());
    }

    #[test]
    fn nearer_layers_leave_out_the_sky_far_scenery_and_the_looks_places() {
        let dir = project("near");
        std::fs::create_dir_all(dir.join("art")).unwrap();
        std::fs::write(dir.join("art/style.toml"), "style = \"soft painterly storybook landscapes, rolling hills and pine forests, warm golden light\"\n").unwrap();
        let mut a = args(&dir, "layers", [2560, 720], Vec::new());
        a.count = Some(3);
        a.prompt = "rolling hills at dusk with pine forests, a far range of soft blue mountains under a warm amber sky".into();
        let p = plan(&a, &table(&dir)).unwrap();
        let prompts: Vec<&str> = p["steps"].as_array().unwrap().iter().map(|s| s["prompt"].as_str().unwrap()).collect();
        assert!(prompts[0].contains("under a warm amber sky") && prompts[0].contains("rolling hills and pine forests"), "the far layer has it all: {}", prompts[0]);
        for near in &prompts[1..] {
            for gone in ["amber sky", "mountains", "rolling hills and pine forests", "landscapes"] {
                assert!(!near.contains(gone), "{gone:?} in {near}");
            }
            assert!(near.contains("whatever fits rolling hills at dusk, pine forests"), "{near}");
            assert!(near.contains("Art style: soft painterly storybook, warm golden light."), "{near}");
        }
        assert_eq!(layer_scene("a neon city under a starry sky"), "a neon city");
        assert_eq!(layer_scene("a sky full of stars"), "");
        assert_eq!(without_scenery("storybook gouache at dusk"), "storybook gouache at dusk");
    }

    #[test]
    fn the_key_is_away_from_the_far_layers_colours() {
        let dir = project("key");
        // A dusk sky, pink to orange: magenta would key it away.
        let mut dusk = Image::new(400, 200, [214, 92, 160, 255]);
        rect(&mut dusk, 0, 100, 400, 100, [240, 150, 80, 255]);
        let mut a = args(&dir, "layers", [2560, 720], Vec::new());
        a.count = Some(2);
        a.far = vec![write(&dir, "dusk.png", &dusk)];
        let p = plan(&a, &table(&dir)).unwrap();
        assert_ne!(p["key"], "#ff00ff");
        assert!(p["steps"][1]["prompt"].as_str().unwrap().contains(p["key"].as_str().unwrap()), "the nearer layer is asked for it");
        // A green valley under a blue sky: magenta.
        let mut valley = Image::new(400, 200, [120, 170, 230, 255]);
        rect(&mut valley, 0, 100, 400, 100, [60, 120, 50, 255]);
        a.far = vec![write(&dir, "valley.png", &valley)];
        assert_eq!(plan(&a, &table(&dir)).unwrap()["key"], "#ff00ff");
        // A key the style sets is kept.
        std::fs::create_dir_all(dir.join("art")).unwrap();
        std::fs::write(dir.join("art/style.toml"), "style = \"ink\"\nkey = \"#00ffff\"\n").unwrap();
        a.far = vec![write(&dir, "dusk2.png", &dusk)];
        assert_eq!(plan(&a, &table(&dir)).unwrap()["key"], "#00ffff");
    }

    #[test]
    fn key_specks_in_small_gaps_go_and_the_keys_hue_alone_stays() {
        // Dark needles with one-pixel gaps the model blended magenta into.
        let mut im = Image::new(40, 40, [0; 4]);
        rect(&mut im, 0, 20, 40, 20, [20, 40, 25, 255]);
        for x in (2..38).step_by(4) {
            im.set(x, 30, [137, 20, 140, 255]);
        }
        im.set(10, 25, [250, 5, 250, 255]);
        let changed = img::clear_key_spill(&mut im, [255, 0, 255]);
        assert!(changed >= 10, "{changed}");
        assert!(img::spill(im.get(6, 30), [255, 0, 255]) < 24 && im.get(6, 30)[3] < 160, "{:?}", im.get(6, 30));
        assert_eq!(im.get(10, 25)[3], 0, "almost all key: gone");
        // Bright grass on a green key, nothing else around: it stays.
        let mut grass = Image::new(20, 20, [80, 170, 60, 255]);
        assert_eq!(img::clear_key_spill(&mut grass, [0, 255, 0]), 0);
        // A red berry among leaves on a magenta key: no mix of the two makes it, so it stays.
        let mut berry = Image::new(20, 20, [40, 90, 40, 255]);
        rect(&mut berry, 9, 9, 2, 2, [200, 30, 40, 255]);
        assert_eq!(img::clear_key_spill(&mut berry, [255, 0, 255]), 0);
    }

    #[test]
    fn each_layers_sidecar_has_its_own_prompt_seed_and_raw_and_says_how_much_it_covers() {
        let dir = project("sidecars");
        let mut far = Image::new(2048, 880, [90, 70, 140, 255]);
        rect(&mut far, 0, 600, 2048, 280, [30, 50, 60, 255]);
        let mut mid = Image::new(2048, 880, [255, 0, 255, 255]);
        rect(&mut mid, 0, 500, 2048, 380, [60, 70, 80, 255]);
        let mut near = Image::new(2048, 880, [255, 0, 255, 255]);
        rect(&mut near, 0, 700, 2048, 180, [20, 30, 30, 255]);
        // A near layer that painted a whole scene, its sky a gradient keying can't take.
        let mut whole = Image::new(2048, 880, [0; 4]);
        for y in 0..880 {
            let t = y as f32 / 880.0;
            rect(&mut whole, 0, y, 2048, 1, [(230.0 - 170.0 * t) as u8, (140.0 - 90.0 * t) as u8, (60.0 + 40.0 * t) as u8, 255]);
        }
        let (f, m, n) = (write(&dir, "far.png", &far), write(&dir, "mid.png", &mid), write(&dir, "near.png", &near));
        let whole = write(&dir, "whole.png", &whole);
        let made = dir.join("made.json");
        std::fs::write(
            &made,
            json!({ "prompts": ["far words", "mid words", "near words"],
                    "candidates": [[{ "seed": 100, "image": 1, "of": 2, "usd": 0.009 }, { "seed": 1100, "image": 1, "of": 2, "usd": 0.009 }, { "seed": 2100, "image": 1, "of": 2, "usd": 0.009 }],
                                   [{ "seed": 100, "image": 2, "of": 2, "usd": 0.009 }, { "seed": 1100, "image": 2, "of": 2, "usd": 0.009 }, { "seed": 2600, "usd": 0.018 }]] })
            .to_string(),
        )
        .unwrap();
        let mut a = args(&dir, "layers", [2560, 720], vec![format!("{f},{m},{n}"), format!("{f},{m},{whole}")]);
        a.count = Some(3);
        a.made = Some(made);
        a.layer_key = Some("#ff00ff".into());
        let r = process(&a, &table(&dir)).unwrap();
        assert_eq!(r["key"], "#ff00ff");
        let side = |c: u32, i: u32| -> Value {
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("out/{c}/images/fox_{i}.{}.gen.json", if i == 0 { "jpg" } else { "png" }))).unwrap()).unwrap()
        };
        let (s0, s1, s2) = (side(1, 0), side(1, 1), side(1, 2));
        assert_eq!([&s0["full_prompt"], &s1["full_prompt"], &s2["full_prompt"]], [&json!("far words"), &json!("mid words"), &json!("near words")]);
        assert_eq!([&s0["seed"], &s1["seed"], &s2["seed"]], [&json!(100), &json!(1100), &json!(2100)]);
        assert_eq!(s1["raw_sha256"], json!(sha256_hex(&std::fs::read(&m).unwrap())));
        assert_ne!(s1["raw_sha256"], s2["raw_sha256"]);
        assert_eq!(s2["result"]["calls"], json!([{ "seed": 2100, "image": 1, "of": 2, "usd": 0.009 }]));
        assert_eq!(side(2, 2)["seed"], json!(2600), "option 2's near layer was made again, at its own seed");
        assert_eq!((s2["cost_usd"].clone(), side(2, 2)["cost_usd"].clone()), (json!(0.009), json!(0.018)), "each file what its own calls cost");
        let cover = |c: usize| r["candidates"][c]["files"][2]["cover"].as_f64().unwrap();
        assert!(cover(0) < 0.3, "{}", cover(0));
        assert!(cover(1) > 0.5, "a whole scene as the near layer covers it all: {}", cover(1));
        assert!(r["candidates"][0]["files"][0].get("cover").is_none(), "the far layer is opaque by design");
    }

    #[test]
    fn only_pictures_inside_the_project_are_sent() {
        let dir = project("inside");
        std::fs::create_dir_all(dir.join("art/ref")).unwrap();
        std::fs::write(dir.join("art/ref/ok.png"), Image::new(4, 4, [1, 2, 3, 255]).encode_png()).unwrap();
        let outside = std::env::temp_dir().join(format!("mb-studio-asset2-secret-{}", std::process::id()));
        std::fs::write(&outside, "SECRET=1").unwrap();
        // Up and out of the project (the temp dir is the project's parent).
        let up = format!("../{}", outside.file_name().unwrap().to_string_lossy());
        std::fs::write(dir.join("art/style.toml"), format!("style = \"ink\"\nreferences = [{up:?}, \"/etc/hosts\", \"art/ref/ok.png\"]\n")).unwrap();
        let a = args(&dir, "image", [256, 256], Vec::new());
        let p = plan(&a, &table(&dir)).unwrap();
        assert_eq!(p["steps"][0]["refs"], json!([{ "file": "art/ref/ok.png" }]), "no `..`, no root");
        for bad in [up.as_str(), "/etc/hosts", "art/../art/ref/ok.png", "art/ref/missing.png"] {
            let mut a = args(&dir, "image", [256, 256], Vec::new());
            a.refs = vec![bad.into()];
            assert!(plan(&a, &table(&dir)).unwrap_err().contains("a picture in the project"), "{bad}");
        }
        // Links out of the project (Unix: Windows needs a privilege to make them).
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, dir.join("art/ref/link.png")).unwrap();
            assert!(!inside(&dir, "art/ref/link.png"));
            std::fs::write(dir.join("art/style.toml"), "style = \"ink\"\nreferences = [\"art/ref/link.png\", \"art/ref/ok.png\"]\n").unwrap();
            let p = plan(&args(&dir, "image", [256, 256], Vec::new()), &table(&dir)).unwrap();
            assert_eq!(p["steps"][0]["refs"], json!([{ "file": "art/ref/ok.png" }]), "no link out");
            // A style guide or DESIGN.md that links out is read as none.
            std::fs::remove_file(dir.join("DESIGN.md")).unwrap();
            std::os::unix::fs::symlink(&outside, dir.join("DESIGN.md")).unwrap();
            std::fs::remove_file(dir.join("art/style.toml")).unwrap();
            std::fs::write(&outside, "# T\n\n## Look\n\nSECRET=1\n").unwrap();
            let p = plan(&args(&dir, "image", [256, 256], Vec::new()), &table(&dir)).unwrap();
            assert!(!p["steps"][0]["prompt"].as_str().unwrap().contains("SECRET"));
        }
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn a_set_or_sheet_past_4096_is_refused() {
        let dir = project("wide");
        std::fs::create_dir_all(dir.join("tiles")).unwrap();
        std::fs::write(dir.join("tiles/fox.png"), Image::new(4096, 32, [10, 10, 10, 255]).encode_png()).unwrap();
        let raw = Image::new(64, 64, [140, 210, 100, 255]);
        let mut a = args(&dir, "tile", [32, 32], vec![write(&dir, "t.png", &raw)]);
        a.frame = Some(129);
        assert!(process(&a, &table(&dir)).unwrap_err().contains("at most 4096 wide"));
        let f = write(&dir, "f.png", &raw);
        let a = args(&dir, "frames", [600, 600], vec![[f.as_str(); 8].join(",")]);
        assert!(process(&a, &table(&dir)).unwrap_err().contains("over 4096 pixels"));
    }

    #[test]
    fn a_tile_joins_its_set_from_frame_1() {
        let dir = project("tile");
        std::fs::create_dir_all(dir.join("tiles")).unwrap();
        std::fs::write(dir.join("tiles/fox.png"), Image::new(64, 32, [10, 10, 10, 255]).encode_png()).unwrap();
        let raw = Image::new(512, 512, [140, 210, 100, 255]);
        let mut a = args(&dir, "tile", [32, 32], vec![write(&dir, "t.png", &raw)]);
        a.frame = Some(3);
        let r = process(&a, &table(&dir)).unwrap();
        assert_eq!(r["candidates"][0]["frames"], 3);
        let set = Image::decode(&std::fs::read(dir.join("out/1/tiles/fox.png")).unwrap()).unwrap();
        assert_eq!((set.w, set.h, set.get(0, 0)), (96, 32, [10, 10, 10, 255]));
        assert_eq!(r["candidates"][0]["toml"]["set"], json!({ "frame": [32, 32], "pixel": false }));
        a.frame = Some(5);
        assert!(process(&a, &table(&dir)).unwrap_err().contains("frame 3 adds one"));
    }

    #[test]
    fn warnings_are_in_studio_words() {
        assert_eq!(
            studio_words(
                "the background wasn't one flat color (variation 0.11): 10% of the image's edge is background left behind; raise --tolerance (mb regen <asset> --reprocess --tolerance 0.15) or regenerate"
            ),
            "the background wasn't one flat color (variation 0.11): 10% of the image's edge is background left behind; generate it again"
        );
        assert_eq!(
            studio_words("the subject runs off the generated image's left edge: part of it is missing. Regenerate (mb regen images/x.png --new-seed)"),
            "the subject runs off the generated image's left edge: part of it is missing. Generate it again"
        );
    }
}
