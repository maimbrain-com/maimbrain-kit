//! A Studio 2.0 project, read from its files (docs/STUDIO-2.md §5
//! [project]).
//!
//! In a bundle the project is under `assets/`, with the generated
//! `assets/files.txt` listing it, each `game/*.lua` beside its bytecode
//! (`game/main.luac`), and `packed.toml` saying what the packer knows about
//! the files (an image's size, so nothing has to be decoded to lay it out).
//! Here it's a map of project paths to bytes, however they arrived.
//!
//! Parsing is plain Rust: a problem in a file is an error that names the
//! file, shown on screen as a Lua error is.

use std::collections::BTreeMap;

use maimbrain::scene::Tile;
use toml::{Table, Value};

/// A project's files: its path (`studio.toml`, `game/main.lua`, …) to bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// What `mb build` writes beside the project: per image, its size in
/// pixels (mb-cli's `studio::v2` writes it; the names must agree).
pub const PACKED: &str = "packed.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// `images/<name>.png` (or `.jpg`) with its optional `images/<name>.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageInfo {
    /// `"hero"`, `"bg/dusk_0"`: the path under `images/` without its extension.
    pub name: String,
    /// The file it's drawn from: its own (`images/hero.png`), or the atlas
    /// the packer put it in (`atlas/0.png`).
    pub path: String,
    /// Its top left in that file (0, 0 in its own).
    pub at: (u32, u32),
    /// In pixels: one pixel is one logical unit.
    pub w: u32,
    pub h: u32,
    /// One frame of a sheet (`frame = [w, h]`; the whole image without it).
    pub frame: (u32, u32),
    /// Pixel art (`pixel = true`): sampled nearest.
    pub pixel: bool,
    /// Named animations (`[anims]`): frames (from 1) and frames a second.
    pub anims: Vec<(String, Vec<u32>, f32)>,
    /// A walker's body and foot in the picture (`body`, `foot` in its
    /// `.toml`, or measured by the packer for `walker = true`).
    pub walker: Option<([f32; 4], [f32; 4])>,
    /// The quick path (§5 [quick]): `behaviour` or `behaviours`, `[knobs]`,
    /// `hitbox = [x, y, w, h]` (in the frame's pixels) and what it `spawns`.
    pub behaviours: Vec<String>,
    pub knobs: Vec<(String, f64)>,
    pub hitbox: Option<[f32; 4]>,
    pub spawns: Option<String>,
    /// A talking portrait's rows, top to bottom (`portrait = ["neutral",
    /// "smug", …, "blink"]` in its `.toml`): each row is the face with four
    /// mouths across, and a row named "blink" is the neutral face, eyes shut.
    pub portrait: Option<Vec<String>>,
}

/// One line of `voices/lines.csv` (§5 [voice]), with what `generate`
/// spoke for it: `voices/<who>/<line>.ogg` and its lip track
/// (`.lips.txt`), or its length from the packer.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceLineInfo {
    pub who: String,
    pub line: String,
    pub text: String,
    /// The `emotion` column: the portrait's expression for it.
    pub emotion: String,
    /// The bark pool it's in (the `tag` column, or the id without its number).
    pub tag: String,
    /// The script's tag column names its pool: a bark line, decoded from
    /// the round's start (api/voice.rs).
    pub tagged: bool,
    /// `voices/<who>/<line>.ogg` is in the project.
    pub ogg: bool,
    /// Its timing: the lip track, else its sound's length, else its text
    /// typed out at 30 characters a second. Never when its audio loads.
    pub lips: maimbrain::ui::Lips,
}

/// `tiles/<name>.png` with `tiles/<name>.toml` (§5 [quick]): its tiles,
/// each a frame of the sheet (from 1).
#[derive(Clone, Debug, PartialEq)]
pub struct TileSet {
    pub name: String,
    /// Its sheet, an image named `tiles/<name>`.
    pub image: usize,
    /// Sorted by frame: the set's bare name means the first.
    pub tiles: Vec<Tile>,
}

/// What a map's legend places in a cell: a thing (`{ thing = "lantern" }`,
/// a Sprite of that kind, with its numbers as properties) or a marker.
#[derive(Clone, Debug, PartialEq)]
pub struct Thing {
    pub kind: String,
    pub cell: (u32, u32),
    pub props: Vec<(String, f64)>,
    /// A spawner: one every this many seconds (not placed once).
    pub every: Option<f64>,
    /// Spawns anywhere this wide, centred on the cell.
    pub spread: f64,
}

/// A number, string or true/false (a scenario's `set` globals).
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Num(f64),
    Str(String),
    Bool(bool),
}

/// One of `debug/scenarios.toml`'s named sets of debug picks (§5 [debug]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub note: Option<String>,
    pub level: Option<String>,
    pub score: Option<f64>,
    pub lives: Option<f64>,
    pub invincible: Option<bool>,
    /// (kind, knob, value)
    pub knobs: Vec<(String, String, f64)>,
    pub spawn: Vec<String>,
    pub set: Vec<(String, Scalar)>,
}

/// `maps/<name>.map`, 1.0's text format.
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo {
    pub name: String,
    pub cols: u32,
    pub rows: u32,
    /// The tile set its tiles come from (a map draws from one), if it has any.
    pub set: Option<usize>,
    /// Row by row: an index into the set's tiles.
    pub cells: Vec<Option<u16>>,
    pub things: Vec<Thing>,
    pub markers: Vec<(String, (u32, u32))>,
}

/// A font the packer baked (`fonts/<id>.mbf`).
#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    /// The library's id (`"press-start-2p"`), or a project font's file name.
    pub id: String,
    /// Its family name (`"Press Start 2P"`).
    pub family: String,
    /// The `.mbf`'s project path.
    pub path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub title: String,
    pub tagline: Option<String>,
    pub orientation: Orientation,
    /// The canvas in logical units (`size`, else 360×640 or 640×360).
    pub size: (u32, u32),
    /// `round.lives` at the start; at 0 the engine ends the round.
    pub lives: u32,
    /// The ending's heading when the lives run out (`[text] over`).
    pub over: String,
    /// Board 0's label and whether lower is better.
    pub label: String,
    pub lower_is_better: bool,
    /// Whether studio.toml names a board (`[[scores]]`): without one the
    /// HUD shows no score (a LÖVE game brought in keeps its own HUD).
    pub scored: bool,
    /// Every board, board 0 first (crate::boards: `round.post`'s and the daily one).
    pub boards: Vec<crate::boards::Board>,
    pub images: Vec<ImageInfo>,
    pub tilesets: Vec<TileSet>,
    pub maps: Vec<MapInfo>,
    pub fonts: Vec<FontInfo>,
    pub scenarios: Vec<Scenario>,
    /// `preset = "arcade"` (§4.9), its 16 colours and its background colour.
    pub arcade: bool,
    pub palette: [u32; 16],
    pub background: Option<u8>,
    /// The identity (§4.12), and the key art's colours the packer sampled.
    pub identity: crate::identity::Identity,
    pub art_colors: Option<maimbrain::ui::identity::ArtColors>,
    /// `sounds/<name>` and `music/<name>` (an `.ogg`, or a `.sfx.toml` or
    /// `.song.toml` the packer renders to one): name and length in seconds
    /// (0 when the packer hasn't said).
    pub sounds: Vec<(String, f32)>,
    pub music: Vec<(String, f32)>,
    /// `music/<name>.music.json`: music graphs the Director plays (§5 [sound]), by name.
    pub graphs: Vec<(String, maimbrain::music::Graph)>,
    /// `music/<name>.songs.json`: setlists, by name: each zone's graph (an index into `graphs`).
    pub setlists: Vec<(String, Vec<(String, usize)>)>,
    /// `kind = "3d"`: the 3D build plays it, with engine 1.0's 3D names (api/three.rs).
    pub three: bool,
    /// `models/<name>.glb`, by name (3D games).
    pub models: Vec<String>,
    /// `game/<name>.lua` → its bytecode, by module name (`"main"`, `"enemies"`).
    pub chunks: BTreeMap<String, Vec<u8>>,
    /// `shaders/<name>.wgsl` (docs/STUDIO-2-API.md [shaders]), by name.
    pub shaders: Vec<ShaderInfo>,
    /// `voices/lines.csv`'s lines (and any spoken line it doesn't list).
    pub voices: Vec<VoiceLineInfo>,
    /// The sensors studio.toml declares (`sensors`), which `sensors.*` read.
    pub sensors: crate::api::sensors::Declared,
}

/// A project's shader: its WGSL, its kind, and its `struct Uniforms`'s
/// layout (name, byte offset, floats), from what `mb build` packed
/// (`[shaders.<name>]` in packed.toml, from the checks).
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderInfo {
    pub name: String,
    pub source: String,
    /// A 2D sprite's (`fn sprite`), else a 3D material's (`fn surface`).
    pub sprite: bool,
    /// None: the shader reads `mb_u` (no `struct Uniforms`).
    pub uniforms: Option<Vec<(String, u32, u32)>>,
    /// Its static cost per pixel, as the checks put it.
    pub cost: u32,
}

/// The project's shaders: each `shaders/<name>.wgsl`, with what packed.toml says of it.
fn shaders(files: &Files, packed: &Table) -> Result<Vec<ShaderInfo>, String> {
    let mut out = Vec::new();
    let info = packed.get("shaders").and_then(Value::as_table);
    for (path, bytes) in files {
        // `shaders/<name>.wgsl` only, mb build's rule (v2::is_shader_path), which refuses the rest.
        let Some(name) = path.strip_prefix("shaders/").and_then(|p| p.strip_suffix(".wgsl")).filter(|n| !n.is_empty() && !n.contains('/') && !n.starts_with('.')) else { continue };
        let source = String::from_utf8(bytes.clone()).map_err(|_| format!("{path}: not UTF-8 text"))?;
        let t = info.and_then(|t| t.get(name)).and_then(Value::as_table);
        let sprite = match t.and_then(|t| t.get("kind")).and_then(Value::as_str) {
            Some("sprite") => true,
            Some(_) => false,
            None => source.contains("fn sprite"),
        };
        let uniforms = match t.and_then(|t| t.get("uniforms")).and_then(Value::as_array) {
            None => None,
            Some(list) => Some(
                list.iter()
                    .map(|f| {
                        let a = f.as_array().filter(|a| a.len() == 3).ok_or_else(|| format!("{PACKED}: shaders.{name}.uniforms is [[name, offset, floats], …]"))?;
                        let n = a[0].as_str().ok_or_else(|| format!("{PACKED}: shaders.{name}: a uniform's name"))?.to_string();
                        let o = a[1].as_integer().filter(|o| (0..256).contains(o)).ok_or_else(|| format!("{PACKED}: shaders.{name}.{n}'s offset"))? as u32;
                        let c = a[2].as_integer().filter(|c| (1..=4).contains(c)).ok_or_else(|| format!("{PACKED}: shaders.{name}.{n}'s floats"))? as u32;
                        Ok((n, o, c))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            ),
        };
        let cost = t.and_then(|t| t.get("cost")).and_then(Value::as_integer).unwrap_or(0).max(0) as u32;
        out.push(ShaderInfo { name: name.to_string(), source, sprite, uniforms, cost });
    }
    Ok(out)
}

/// `music/<name>.ogg` (or `.song.toml`) by the path a graph names it with:
/// `music/<name>.ogg`, or a kit graph's `assets/<name>.ogg`.
fn music_index(music: &[(String, f32)], path: &str) -> Option<usize> {
    let base = path.rsplit('/').next().unwrap_or(path);
    let n = base.strip_suffix(".ogg").unwrap_or(base);
    music.iter().position(|m| m.0 == n)
}

type Graphs = (Vec<(String, maimbrain::music::Graph)>, Vec<(String, Vec<(String, usize)>)>);

/// The project's music graphs and setlists, each checked: it parses, and
/// every file it plays is one of the project's music files.
fn music_graphs(files: &Files, music: &[(String, f32)]) -> Result<Graphs, String> {
    use maimbrain::music::{Graph, Setlist};
    let mut graphs = Vec::new();
    for (path, bytes) in files {
        let Some(name) = path.strip_prefix("music/").and_then(|p| p.strip_suffix(".music.json")) else { continue };
        let g = Graph::parse(bytes).map_err(|e| format!("{path}: {e}"))?;
        if let Some(f) = g.files().into_iter().find(|f| music_index(music, f).is_none()) {
            return Err(format!("{path} plays {f}, which isn't one of the project's music files"));
        }
        graphs.push((name.to_string(), g));
    }
    let mut setlists = Vec::new();
    for (path, bytes) in files {
        let Some(name) = path.strip_prefix("music/").and_then(|p| p.strip_suffix(".songs.json")) else { continue };
        let list = Setlist::parse(bytes).map_err(|e| format!("{path}: {e}"))?;
        let mut songs = Vec::new();
        for (zone, g) in list.songs {
            let gname = g.rsplit('/').next().unwrap_or(&g).trim_end_matches(".music.json");
            let i = graphs.iter().position(|x| x.0 == gname).ok_or_else(|| format!("{path}: {zone}'s graph {g} isn't in music/"))?;
            songs.push((zone, i));
        }
        if songs.is_empty() {
            return Err(format!("{path}: a setlist with no songs"));
        }
        setlists.push((name.to_string(), songs));
    }
    Ok((graphs, setlists))
}

fn read_toml(files: &Files, path: &str) -> Result<Option<Table>, String> {
    let Some(b) = files.get(path) else { return Ok(None) };
    let text = std::str::from_utf8(b).map_err(|_| format!("{path}: not UTF-8 text"))?;
    text.parse::<Table>().map(Some).map_err(|e| format!("{path}: {}", e.message()))
}

fn s<'a>(t: &'a Table, k: &str) -> Option<&'a str> {
    t.get(k).and_then(Value::as_str)
}

fn pair(v: &Value, what: &str) -> Result<(u32, u32), String> {
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| format!("{what} is [w, h]"))?;
    let n = |v: &Value| v.as_integer().filter(|n| (1..=8192).contains(n)).map(|n| n as u32).ok_or_else(|| format!("{what} is [w, h], whole numbers from 1"));
    Ok((n(&a[0])?, n(&a[1])?))
}

impl Project {
    pub fn parse(files: &Files) -> Result<Project, String> {
        let t = read_toml(files, "studio.toml")?.ok_or("studio.toml is missing")?;
        match t.get("engine") {
            Some(Value::String(v)) if v.starts_with("2.") => {}
            Some(Value::Float(f)) if (2.0..3.0).contains(f) => {}
            Some(v) => {
                let v = v.as_str().map_or_else(|| v.as_float().map_or("?".into(), |f| format!("{f:.1}")), |s| format!("\"{s}\""));
                return Err(format!("studio.toml: engine = {v}, but this is engine {} (1.0 projects play on engine 1.0)", crate::ENGINE_VERSION));
            }
            None => return Err(format!("studio.toml: say engine = \"{}\"", crate::ENGINE_VERSION)),
        }
        let three = match s(&t, "kind").unwrap_or("2d") {
            "2d" => false,
            "3d" => true,
            k => return Err(format!("studio.toml: kind = \"{k}\" (\"2d\" or \"3d\")")),
        };
        let orientation = match s(&t, "orientation").unwrap_or("portrait") {
            "portrait" => Orientation::Portrait,
            "landscape" => Orientation::Landscape,
            "sideways" => return Err("studio.toml: orientation = \"sideways\" is \"landscape\" in engine 2.0".into()),
            o => return Err(format!("studio.toml: orientation = \"{o}\" (portrait or landscape)")),
        };
        // The Arcade preset (§4.9): 1.0's look and limits, its world in art pixels.
        let arcade = match t.get("preset") {
            None => false,
            Some(Value::String(p)) if p == "arcade" => true,
            Some(v) => return Err(format!("studio.toml: preset = {v:?} (\"arcade\", or no preset)")),
        };
        let size = match t.get("size") {
            Some(_) if arcade => return Err("studio.toml: an Arcade game's world is 144×256 art pixels (256×144 landscape): leave out size, or leave out preset = \"arcade\"".into()),
            Some(v) => pair(v, "studio.toml: size")?,
            None if arcade && orientation == Orientation::Portrait => (144, 256),
            None if arcade => (256, 144),
            None if orientation == Orientation::Portrait => (360, 640),
            None => (640, 360),
        };
        if three && arcade {
            return Err("studio.toml: the Arcade preset is 2D: leave out preset = \"arcade\" in a 3D game".into());
        }
        let palette = palette(t.get("palette"))?;
        let background = match t.get("background") {
            None => None,
            Some(Value::Integer(i)) if (0..16).contains(i) => Some(*i as u8),
            Some(_) => return Err("studio.toml: background is a palette colour, 0–15".into()),
        };
        let lives = match t.get("lives") {
            None => 1,
            Some(v) => v.as_integer().filter(|n| (1..=99).contains(n)).ok_or("studio.toml: lives is a whole number from 1 to 99")? as u32,
        };
        let text = t.get("text").and_then(Value::as_table);
        let boards = crate::boards::parse(&t)?;
        let lower_is_better = boards.first().is_some_and(|b| b.lower);

        let packed = read_toml(files, PACKED)?.unwrap_or_default();
        let packed_images = packed.get("images").and_then(Value::as_table);
        // Every picture: what the packer recorded (its file may be an atlas),
        // and any image file it didn't (a project read natively).
        // name → (file, top left there, size)
        type Found = BTreeMap<String, (String, (u32, u32), (u32, u32))>;
        let mut found = Found::new();
        for (name, v) in packed_images.into_iter().flatten() {
            let t = v.as_table().ok_or_else(|| format!("{PACKED}: images.{name} isn't a table"))?;
            let size = pair(t.get("size").ok_or_else(|| format!("{PACKED}: images.{name} has no size"))?, &format!("{PACKED}: images.{name}.size"))?;
            let file = t.get("file").and_then(Value::as_str).ok_or_else(|| format!("{PACKED}: images.{name} has no file"))?.to_string();
            let at = match t.get("at") {
                Some(v) => {
                    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| format!("{PACKED}: images.{name}.at is [x, y]"))?;
                    (a[0].as_integer().unwrap_or(0) as u32, a[1].as_integer().unwrap_or(0) as u32)
                }
                None => (0, 0),
            };
            found.insert(name.clone(), (file, at, size));
        }
        // Tile sets' sheets are images too, named `tiles/<name>`.
        for (path, bytes) in files {
            let (name, ext) = match path.strip_prefix("images/").and_then(|r| r.rsplit_once('.')) {
                Some((n, e)) => (n.to_string(), e),
                None => match path.strip_prefix("tiles/").and_then(|r| r.rsplit_once('.')) {
                    Some((n, e)) if e.eq_ignore_ascii_case("png") => (format!("tiles/{n}"), e),
                    _ => continue,
                },
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg") || found.contains_key(&name) {
                continue;
            }
            let size = image_size(bytes).ok_or_else(|| format!("{path}: not a PNG or JPEG"))?;
            found.insert(name, (path.clone(), (0, 0), size));
        }
        let mut images = Vec::new();
        for (name, (path, at, (w, h))) in found {
            let tiles = name.strip_prefix("tiles/");
            let meta_path = match tiles {
                Some(t) => format!("tiles/{t}.toml"),
                None => format!("images/{name}.toml"),
            };
            let meta = read_toml(files, &meta_path)?;
            let meta = meta.as_ref();
            let frame = match meta.and_then(|m| m.get("frame")) {
                Some(v) => pair(v, &format!("{meta_path}: frame"))?,
                // A tile set's tiles are square without it.
                None if tiles.is_some() => (h.min(w), h.min(w)),
                // Arcade art is 1.0's: square frames side by side.
                None if arcade => (h.min(w), h),
                None => (w, h),
            };
            if frame.0 > w || frame.1 > h {
                return Err(format!("{meta_path}: frame = [{}, {}] is bigger than the {w}×{h} image", frame.0, frame.1));
            }
            let frames = (w / frame.0).max(1) * (h / frame.1).max(1);
            if arcade {
                let file = path.rsplit('/').next().unwrap_or(&path);
                if tiles.is_some() && frame != (16, 16) {
                    return Err(format!("{meta_path}: Arcade tiles are 16×16, and {file}'s are {}×{}. Without preset = \"arcade\" any size works", frame.0, frame.1));
                }
                if !matches!(frame, (16, 16) | (32, 32)) {
                    return Err(format!("Arcade images are 16×16 or 32×32, and {file} is {}×{}. Without preset = \"arcade\" any size works", frame.0, frame.1));
                }
                if frames > 8 && tiles.is_none() {
                    return Err(format!("Arcade images have up to 8 frames, and {file} has {frames}. Without preset = \"arcade\" any number works"));
                }
            }
            let mut anims = Vec::new();
            for (anim, v) in meta.and_then(|m| m.get("anims")).and_then(Value::as_table).into_iter().flatten() {
                let at = format!("images/{name}.toml: anims.{anim}");
                let t = v.as_table().ok_or_else(|| format!("{at} is {{ frames = \"1-4\", fps = 8 }}"))?;
                let list = frame_list(t.get("frames").ok_or_else(|| format!("{at} has no frames"))?, frames).map_err(|e| format!("{at}: {e}"))?;
                let fps = match t.get("fps") {
                    None => 8.0,
                    Some(v) => v.as_float().or(v.as_integer().map(|i| i as f64)).filter(|f| *f > 0.0 && *f <= 120.0).ok_or_else(|| format!("{at}: fps is a number from 0 to 120"))? as f32,
                };
                anims.push((anim.clone(), list, fps));
            }
            // Tiles are pixel art unless they say otherwise (1.0's were), and
            // so is every picture under the Arcade preset (1.0's art, drawn
            // 2.5 units an art pixel: sampled linearly it blurs).
            let pixel = meta.and_then(|m| m.get("pixel")).and_then(Value::as_bool).unwrap_or(tiles.is_some() || arcade);
            let rect = |t: Option<&Table>, k: &str| -> Result<Option<[f32; 4]>, String> {
                let Some(v) = t.and_then(|t| t.get(k)) else { return Ok(None) };
                let a = v.as_array().filter(|a| a.len() == 4).ok_or_else(|| format!("{meta_path}: {k} is [x, y, w, h] in the picture's pixels"))?;
                let mut r = [0.0f32; 4];
                for (slot, x) in r.iter_mut().zip(a) {
                    *slot = x.as_integer().map(|i| i as f64).or(x.as_float()).ok_or_else(|| format!("{meta_path}: {k} is [x, y, w, h] in the picture's pixels"))? as f32;
                }
                Ok(Some(r))
            };
            let measured = packed_images.and_then(|p| p.get(&name)).and_then(Value::as_table);
            let walker = match (rect(meta, "body")?.or(rect(measured, "body")?), rect(meta, "foot")?.or(rect(measured, "foot")?)) {
                (Some(b), Some(f)) => Some((b, f)),
                (None, None) => None,
                _ => return Err(format!("{meta_path}: a walker has both body and foot ([x, y, w, h] each)")),
            };
            let at_meta = |k: &str| format!("{meta_path}: {k}");
            let mut behaviours: Vec<String> = Vec::new();
            match (meta.and_then(|m| m.get("behaviour")), meta.and_then(|m| m.get("behaviours"))) {
                (Some(Value::String(b)), None) => behaviours.push(b.clone()),
                (None, Some(Value::Array(a))) => {
                    for b in a {
                        behaviours.push(b.as_str().ok_or_else(|| at_meta("behaviours is a list of names"))?.to_string());
                    }
                }
                (None, None) => {}
                (Some(_), Some(_)) => return Err(at_meta("say behaviour = \"…\" or behaviours = [\"…\", …], not both")),
                _ => return Err(at_meta("behaviour is a name (\"player.drag\"), behaviours a list of them")),
            }
            let mut knobs = Vec::new();
            for (k, v) in meta.and_then(|m| m.get("knobs")).and_then(Value::as_table).into_iter().flatten() {
                let n = v.as_float().or(v.as_integer().map(|i| i as f64)).ok_or_else(|| at_meta(&format!("knobs.{k} takes a number")))?;
                knobs.push((k.clone(), n));
            }
            let hitbox = rect(meta, "hitbox")?;
            let spawns = meta.and_then(|m| s(m, "spawns")).map(str::to_string);
            let portrait = match meta.and_then(|m| m.get("portrait")) {
                None => None,
                Some(Value::Array(a)) => {
                    let rows: Option<Vec<String>> = a.iter().map(|v| v.as_str().map(|r| r.trim().to_ascii_lowercase())).collect();
                    let rows = rows.filter(|r| !r.is_empty()).ok_or_else(|| at_meta("portrait is its rows' names, top to bottom: [\"neutral\", \"smug\", \"blink\"]"))?;
                    if frame.0 * 4 > w || frame.1 * rows.len() as u32 > h {
                        return Err(at_meta(&format!("a portrait is 4 mouths across and a row for each of its {} expressions, {}×{} cells, and the picture is {w}×{h}", rows.len(), frame.0, frame.1)));
                    }
                    Some(rows)
                }
                Some(_) => return Err(at_meta("portrait is its rows' names, top to bottom: [\"neutral\", \"smug\", \"blink\"]")),
            };
            images.push(ImageInfo { name, path, at, w, h, frame, pixel, anims, walker, behaviours, knobs, hitbox, spawns, portrait });
        }
        let tilesets = tilesets(files, &images)?;
        let mut maps = Vec::new();
        for (path, bytes) in files {
            let Some(name) = path.strip_prefix("maps/").and_then(|p| p.strip_suffix(".map")) else { continue };
            let src = std::str::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))?;
            maps.push(map(name, src, path, &tilesets)?);
        }
        let mut fonts = Vec::new();
        for (id, v) in packed.get("fonts").and_then(Value::as_table).into_iter().flatten() {
            let t = v.as_table().ok_or_else(|| format!("{PACKED}: fonts.{id} isn't a table"))?;
            let get = |k: &str| t.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("{PACKED}: fonts.{id} has no {k}"));
            fonts.push(FontInfo { id: id.clone(), family: get("family")?, path: get("file")? });
        }

        let sounds_in = |dir: &str, synth: &str| -> Vec<(String, f32)> {
            let mut names: Vec<String> = files.keys().filter_map(|p| p.strip_prefix(&format!("{dir}/"))).filter_map(|p| p.strip_suffix(".ogg").or_else(|| p.strip_suffix(synth))).map(str::to_string).collect();
            names.sort();
            names.dedup();
            let secs = packed.get(dir).and_then(Value::as_table);
            names.into_iter().map(|n| {
                let s = secs.and_then(|t| t.get(&n)).and_then(|t| t.get("secs")).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64))).unwrap_or(0.0) as f32;
                (n, s)
            }).collect()
        };
        let sounds = sounds_in("sounds", ".sfx.toml");
        let music = sounds_in("music", ".song.toml");
        let voices = voices(files, &packed)?;
        let (graphs, setlists) = music_graphs(files, &music)?;
        let models: Vec<String> = files.keys().filter_map(|p| p.strip_prefix("models/")?.strip_suffix(".glb")).map(str::to_string).collect();
        if !three && let Some(m) = models.first() {
            return Err(format!("models/{m}.glb: models are for 3D games (kind = \"3d\" in studio.toml)"));
        }

        let shaders = shaders(files, &packed)?;

        let mut chunks = BTreeMap::new();
        for (path, bytes) in files {
            if let Some(name) = path.strip_prefix("game/").and_then(|p| p.strip_suffix(".luac")) {
                chunks.insert(name.to_string(), bytes.clone());
            }
        }
        if !files.contains_key("game/main.lua") {
            return Err("game/main.lua is missing".into());
        }
        if !chunks.contains_key("main") {
            return Err("game/main.lua hasn't been compiled (build Studio projects with `mb build`)".into());
        }

        let sensors = crate::api::sensors::Declared::parse(t.get("sensors"))?;
        Ok(Project {
            sensors,
            title: s(&t, "title").unwrap_or("Untitled").to_string(),
            tagline: s(&t, "tagline").map(str::to_string),
            orientation,
            size,
            lives,
            over: text.and_then(|x| s(x, "over")).unwrap_or("GAME OVER").to_string(),
            label: boards.first().map_or_else(|| "Score".to_string(), |b| b.label.clone()),
            lower_is_better,
            scored: !boards.is_empty(),
            boards,
            images,
            tilesets,
            maps,
            fonts,
            identity: crate::identity::Identity::parse(t.get("identity"))?,
            arcade,
            palette,
            background,
            art_colors: packed.get("identity").and_then(Value::as_table).and_then(|i| {
                let c = |k: &str| i.get(k).and_then(Value::as_integer).map(|v| v as u32);
                Some(maimbrain::ui::identity::ArtColors { bg: c("bg")?, panel: c("panel")?, accent: c("accent")?, highlight: c("highlight")? })
            }),
            scenarios: match read_toml(files, "debug/scenarios.toml")? {
                Some(t) => scenarios(&t, "debug/scenarios.toml")?,
                None => Vec::new(),
            },
            sounds,
            music,
            graphs,
            setlists,
            three,
            models,
            chunks,
            shaders,
            voices,
        })
    }

    /// A line of `voices/lines.csv` by speaker and id.
    pub fn voice(&self, who: &str, line: &str) -> Option<usize> {
        self.voices.iter().position(|v| v.who == who && v.line == line)
    }

    /// Logical units an art pixel: 1.0's knobs and sizes times this.
    pub fn unit(&self) -> f32 {
        if self.arcade { 1.0 } else { 2.5 }
    }

    /// Palette colour `i` (0xRRGGBBAA), wrapping past 15.
    pub fn color(&self, i: u8) -> u32 {
        self.palette[(i & 15) as usize]
    }

    pub fn map(&self, name: &str) -> Option<usize> {
        let n = name.strip_prefix("maps/").unwrap_or(name);
        let n = n.strip_suffix(".map").unwrap_or(n);
        self.maps.iter().position(|m| m.name == n)
    }

    pub fn tileset(&self, name: &str) -> Option<usize> {
        let n = name.strip_prefix("tiles/").unwrap_or(name);
        let n = n.strip_suffix(".png").unwrap_or(n);
        self.tilesets.iter().position(|t| t.name == n)
    }

    pub fn image(&self, name: &str) -> Option<usize> {
        self.images.iter().position(|i| i.name == name)
    }

    /// A sound by name (`"jump"`), or LÖVE's path (`"sounds/jump.ogg"`).
    /// The music file a graph names (`music/run_take.ogg`, or a kit's
    /// `assets/run_take.ogg`): its index in `music`.
    pub fn music_file(&self, path: &str) -> Option<usize> {
        music_index(&self.music, path)
    }

    pub fn sound(&self, name: &str) -> Option<usize> {
        let n = name.strip_prefix("sounds/").unwrap_or(name);
        let n = n.strip_suffix(".ogg").or_else(|| n.strip_suffix(".sfx.toml")).unwrap_or(n);
        self.sounds.iter().position(|s| s.0 == n)
    }

    /// A baked font by its family (`"Press Start 2P"`) or id, in any case.
    pub fn font(&self, name: &str) -> Option<&FontInfo> {
        let key = name.to_ascii_lowercase();
        self.fonts.iter().find(|f| f.id == key || f.family.eq_ignore_ascii_case(name) || f.id == key.replace(' ', "-"))
    }
}

/// Every `tiles/<name>.png`'s set, from its `.toml` (`[tiles.<name>]`
/// with `frame` from 1, `solid`, `oneway`, `hazard`; none is one solid tile).
fn tilesets(files: &Files, images: &[ImageInfo]) -> Result<Vec<TileSet>, String> {
    let mut out = Vec::new();
    for (i, im) in images.iter().enumerate() {
        let Some(name) = im.name.strip_prefix("tiles/") else { continue };
        let path = format!("tiles/{name}.toml");
        let meta = read_toml(files, &path)?;
        let frames = (im.w / im.frame.0).max(1) * (im.h / im.frame.1).max(1);
        let mut tiles = Vec::new();
        match meta.as_ref().and_then(|m| m.get("tiles")).and_then(Value::as_table) {
            Some(list) => {
                for (k, v) in list {
                    let v = v.as_table().ok_or_else(|| format!("{path}: tiles.{k} is a table: {{ frame = 1, solid = true }}"))?;
                    let b = |f: &str| v.get(f).and_then(Value::as_bool).unwrap_or(false);
                    let frame = v.get("frame").and_then(Value::as_integer).unwrap_or(1);
                    if frame < 1 || frame > frames as i64 {
                        return Err(format!("{path}: tiles.{k} is frame {frame}, but the sheet has frames 1 to {frames} (frames count from 1)"));
                    }
                    tiles.push(Tile { name: k.clone(), frame: frame as u32, solid: b("solid"), oneway: b("oneway"), hazard: b("hazard") });
                }
            }
            None => tiles.push(Tile { name: "tile".into(), frame: 1, solid: true, oneway: false, hazard: false }),
        }
        tiles.sort_by_key(|t| t.frame);
        out.push(TileSet { name: name.to_string(), image: i, tiles });
    }
    Ok(out)
}

/// A map: `[legend]` (one character to a tile, `"set"` or `"set.tile"`,
/// or `{ thing = "kind", … }`, or `{ marker = "name" }`), then `[map]` and
/// its rows ('.' and ' ' are empty).
fn map(name: &str, src: &str, path: &str, sets: &[TileSet]) -> Result<MapInfo, String> {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| l.trim() == "[map]").ok_or_else(|| format!("{path}: no [map] line (the rows of the map go after it)"))?;
    let header: Table = lines[..at].join("\n").parse().map_err(|e: toml::de::Error| format!("{path}: {}", e.message()))?;
    let mut legend: BTreeMap<char, Value> = BTreeMap::new();
    for (k, v) in header.get("legend").and_then(Value::as_table).into_iter().flatten() {
        let mut cs = k.chars();
        match (cs.next(), cs.next()) {
            (Some(c), None) if c != '.' && c != ' ' => {
                legend.insert(c, v.clone());
            }
            _ => return Err(format!("{path}: legend key {k:?} must be one character (not '.' or a space, which are empty)")),
        }
    }
    let rows: Vec<&str> = lines[at + 1..].iter().map(|l| l.trim_end()).collect();
    let end = rows.iter().rposition(|r| !r.is_empty()).map_or(0, |i| i + 1);
    let rows = &rows[..end];
    let rows_n = rows.len() as u32;
    let cols = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    if cols == 0 || rows_n == 0 {
        return Err(format!("{path}: the map has no rows"));
    }
    if cols > 1024 || rows_n > 1024 {
        return Err(format!("{path}: the map is {cols}×{rows_n} tiles; at most 1024×1024"));
    }
    let mut out = MapInfo { name: name.to_string(), cols, rows: rows_n, set: None, cells: vec![None; (cols * rows_n) as usize], things: Vec::new(), markers: Vec::new() };
    for (cy, row) in rows.iter().enumerate() {
        for (cx, c) in row.chars().enumerate() {
            if c == '.' || c == ' ' {
                continue;
            }
            let line = at + 2 + cy;
            let v = legend.get(&c).ok_or_else(|| format!("{path}:{line}: '{c}' isn't in the legend"))?;
            let cell = (cx as u32, cy as u32);
            match v {
                Value::String(r) => {
                    let (set, tile) = r.split_once('.').map_or((r.as_str(), None), |(a, b)| (a, Some(b)));
                    let si = sets.iter().position(|t| t.name == set).ok_or_else(|| format!("{path}:{line}: '{c}' is tile \"{r}\", but there's no tiles/{set}.png"))?;
                    match out.set {
                        Some(s) if s != si => return Err(format!("{path}:{line}: '{c}' is from tiles/{set}, but the map's tiles are from tiles/{}: a map draws from one tile set", sets[s].name)),
                        _ => out.set = Some(si),
                    }
                    let ts = &sets[si];
                    let ti = match tile {
                        None => 0,
                        Some(t) => ts.tiles.iter().position(|x| x.name == t).ok_or_else(|| {
                            format!("{path}:{line}: '{c}' is tile \"{r}\", but tiles/{set}.toml has no tile \"{t}\" ({})", ts.tiles.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "))
                        })?,
                    };
                    out.cells[cy * cols as usize + cx] = Some(ti as u16);
                }
                Value::Table(t) => {
                    if let Some(m) = t.get("marker").and_then(Value::as_str) {
                        out.markers.push((m.to_string(), cell));
                        continue;
                    }
                    let kind = t.get("thing").and_then(Value::as_str).ok_or_else(|| format!("{path}: legend '{c}' is {{ thing = \"<kind>\" }} or {{ marker = \"<name>\" }} (or a tile name)"))?;
                    let mut props = Vec::new();
                    let (mut every, mut spread) = (None, 0.0);
                    for (k, v) in t {
                        if k == "thing" {
                            continue;
                        }
                        let n = v.as_float().or(v.as_integer().map(|i| i as f64)).ok_or_else(|| format!("{path}: legend '{c}': {k} takes a number"))?;
                        match k.as_str() {
                            "every" if n > 0.0 => every = Some(n),
                            "every" => return Err(format!("{path}: legend '{c}': every is seconds, above 0")),
                            "spread" => spread = n.max(0.0),
                            _ => props.push((k.clone(), n)),
                        }
                    }
                    out.things.push(Thing { kind: kind.to_string(), cell, props, every, spread });
                }
                _ => return Err(format!("{path}: legend '{c}' is a tile name, {{ thing = \"<kind>\" }} or {{ marker = \"<name>\" }}")),
            }
        }
    }
    Ok(out)
}

fn scenarios(t: &Table, path: &str) -> Result<Vec<Scenario>, String> {
    let list = match t.get("scenario") {
        None => return Ok(Vec::new()),
        Some(Value::Array(l)) => l,
        Some(_) => return Err(format!("{path}: scenarios are [[scenario]] tables")),
    };
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let s = s.as_table().ok_or_else(|| format!("{path}: scenario {} isn't a table", i + 1))?;
        let name = s.get("name").and_then(Value::as_str).ok_or_else(|| format!("{path}: scenario {} has no name", i + 1))?;
        let mut sc = Scenario {
            name: name.to_string(),
            note: s.get("note").and_then(Value::as_str).map(str::to_string),
            level: s.get("level").and_then(Value::as_str).map(str::to_string),
            score: s.get("score").and_then(num),
            lives: s.get("lives").and_then(num),
            invincible: s.get("invincible").and_then(Value::as_bool),
            ..Default::default()
        };
        if let Some(k) = s.get("knobs").and_then(Value::as_table) {
            for (key, v) in k {
                let (kind, knob) = key.split_once('.').ok_or_else(|| format!("{path}: scenario \"{name}\": knob {key:?} is \"<kind>.<knob>\""))?;
                let v = num(v).ok_or_else(|| format!("{path}: scenario \"{name}\": knob {key} needs a number"))?;
                sc.knobs.push((kind.to_string(), knob.to_string(), v));
            }
        }
        if let Some(l) = s.get("spawn").and_then(Value::as_array) {
            sc.spawn = l.iter().filter_map(Value::as_str).map(str::to_string).collect();
        }
        if let Some(set) = s.get("set").and_then(Value::as_table) {
            for (k, v) in set {
                let v = match v {
                    Value::Boolean(b) => Scalar::Bool(*b),
                    Value::String(s) => Scalar::Str(s.clone()),
                    v => Scalar::Num(num(v).ok_or_else(|| format!("{path}: scenario \"{name}\": set.{k} is a number, string or true/false"))?),
                };
                sc.set.push((k.clone(), v));
            }
        }
        out.push(sc);
    }
    Ok(out)
}

fn num(v: &Value) -> Option<f64> {
    v.as_float().or(v.as_integer().map(|i| i as f64))
}

/// The palette presets (`palette = "sunset16"`), 0xRRGGBB. As engine 1.0's
/// (the Arcade preset's), and mb-cli's copy.
pub const PALETTES: &[(&str, [u32; 16])] = &[
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

fn palette(v: Option<&Value>) -> Result<[u32; 16], String> {
    let rgba = |c: u32| (c << 8) | 0xff;
    match v {
        None => Ok(PALETTES[0].1.map(rgba)),
        Some(Value::String(name)) => PALETTES
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.map(rgba))
            .ok_or_else(|| format!("studio.toml: no palette called {name:?} (presets: {}, or 16 \"#rrggbb\" colours)", PALETTES.iter().map(|p| p.0).collect::<Vec<_>>().join(", "))),
        Some(Value::Array(list)) if list.len() == 16 => {
            let mut out = [0; 16];
            for (i, c) in list.iter().enumerate() {
                let hex = c.as_str().map(|s| s.trim_start_matches('#')).filter(|s| s.len() == 6);
                let n = hex.and_then(|h| u32::from_str_radix(h, 16).ok()).ok_or_else(|| format!("studio.toml: palette colour {} isn't \"#rrggbb\"", i))?;
                out[i] = rgba(n);
            }
            Ok(out)
        }
        Some(_) => Err("studio.toml: palette is a preset name or 16 \"#rrggbb\" colours".into()),
    }
}

/// anim8's frame lists, as `images/<name>.toml` and `anim8` take them:
/// `"1-8"`, `"3"`, `{ 1, 2, "5-7" }` (frames count from 1, up to `max`).
pub fn frame_list(v: &Value, max: u32) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    let mut add = |s: &str| -> Result<(), String> {
        let (a, b) = s.split_once('-').unwrap_or((s, s));
        let n = |x: &str| x.trim().parse::<u32>().ok().filter(|n| (1..=max).contains(n)).ok_or_else(|| format!("frames: {s:?} (frames 1 to {max})"));
        let (a, b) = (n(a)?, n(b)?);
        if a <= b { out.extend(a..=b) } else { out.extend((b..=a).rev()) }
        Ok(())
    };
    match v {
        Value::String(s) => add(s)?,
        Value::Integer(i) => add(&i.to_string())?,
        Value::Array(a) => {
            for x in a {
                match x {
                    Value::String(s) => add(s)?,
                    Value::Integer(i) => add(&i.to_string())?,
                    _ => return Err("frames are numbers or ranges like \"1-4\"".into()),
                }
            }
        }
        _ => return Err("frames are \"1-4\" or { 1, 2, 3 }".into()),
    }
    if out.is_empty() {
        return Err("frames: none".into());
    }
    Ok(out)
}

/// A PNG's or a baseline/progressive JPEG's size from its header.
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() >= 24 && &b[..8] == b"\x89PNG\r\n\x1a\n" && &b[12..16] == b"IHDR" {
        let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
        return Some((be(16), be(20)));
    }
    if b.len() < 4 || b[0] != 0xff || b[1] != 0xd8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xff {
            return None;
        }
        let marker = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// The project's voice lines: `voices/lines.csv` (`character,line,text,
/// emotion,tag`), each with its lip track (`voices/<who>/<line>.lips.txt`)
/// or, without one, its `.ogg`'s length from the packer (`[voices]` in
/// packed.toml) or its text's. Lines spoken but not in the script are
/// there too, from their lip tracks. A line is always timed by what's in
/// the bundle's text, so a replay never depends on when its audio decoded.
fn voices(files: &Files, packed: &Table) -> Result<Vec<VoiceLineInfo>, String> {
    const SCRIPT: &str = "voices/lines.csv";
    let mut out = Vec::new();
    // In a bundle it's `voices/lines.txt` (a bundle has no .csv files).
    let (script, tagged) = match files.get(SCRIPT).or_else(|| files.get("voices/lines.txt")) {
        Some(b) => {
            let text = std::str::from_utf8(b).map_err(|_| format!("{SCRIPT}: not UTF-8 text"))?;
            (maimbrain::ui::barks::parse_script(text).map_err(|e| format!("{SCRIPT}: {e}"))?, tag_cells(text))
        }
        None => (Vec::new(), Vec::new()),
    };
    let secs = packed.get("voices").and_then(Value::as_table);
    let lips_of = |who: &str, line: &str, text: &str| -> Result<maimbrain::ui::Lips, String> {
        let path = format!("voices/{who}/{line}.lips.txt");
        if let Some(b) = files.get(&path) {
            return maimbrain::ui::Lips::parse(b).ok_or_else(|| format!("{path}: not a lip track (it starts `mblips 1`)"));
        }
        let shown = maimbrain::ui::subtitle_text(text);
        let len = secs.and_then(|t| t.get(&format!("{who}/{line}"))).and_then(|t| t.get("secs")).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
        Ok(match len {
            // Its sound's length, the text spread over it.
            Some(d) if d > 0.0 => maimbrain::ui::Lips::parse(format!("mblips 1\ndur {d}\ntext {shown}\n").as_bytes()).unwrap_or_else(|| maimbrain::ui::Lips::from_text(&shown)),
            _ => maimbrain::ui::Lips::from_text(&shown),
        })
    };
    for (n, l) in script.into_iter().enumerate() {
        let tagged = tagged.get(n).copied().unwrap_or(false) || l.tag != maimbrain::ui::barks::tag_of(&l.id);
        let check = |what: &str, v: &str| -> Result<(), String> {
            if v.is_empty() || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(format!("{SCRIPT}: {what} {v:?} is a name in letters, digits, _ and -"));
            }
            Ok(())
        };
        check("character", &l.speaker)?;
        check("line", &l.id)?;
        if out.iter().any(|v: &VoiceLineInfo| v.who == l.speaker && v.line == l.id) {
            return Err(format!("{SCRIPT}: {}'s line {} is there twice", l.speaker, l.id));
        }
        let lips = lips_of(&l.speaker, &l.id, &l.text)?;
        let ogg = files.contains_key(&format!("voices/{}/{}.ogg", l.speaker, l.id));
        out.push(VoiceLineInfo { who: l.speaker, line: l.id.clone(), text: maimbrain::ui::subtitle_text(&l.text), emotion: l.expression, tag: l.tag, tagged, ogg, lips });
    }
    // Lines spoken without a row in the script (their lip track has the text).
    for path in files.keys() {
        let Some(rest) = path.strip_prefix("voices/").and_then(|p| p.strip_suffix(".lips.txt")) else { continue };
        let Some((who, line)) = rest.split_once('/') else { continue };
        if line.contains('/') || out.iter().any(|v| v.who == who && v.line == line) {
            continue;
        }
        let lips = lips_of(who, line, "")?;
        let ogg = files.contains_key(&format!("voices/{who}/{line}.ogg"));
        out.push(VoiceLineInfo { who: who.into(), line: line.into(), text: lips.text().to_string(), emotion: String::new(), tag: maimbrain::ui::barks::tag_of(line), tagged: false, ogg, lips });
    }
    Ok(out)
}

/// For each line of a CSV script (as `parse_script` reads its rows: blank
/// and `#` lines skipped, a header of column names), whether its tag
/// column has something in it. A TOML script's are told by their tag
/// differing from the id's (`voices`).
fn tag_cells(text: &str) -> Vec<bool> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).peekable();
    if rows.peek().is_some_and(|l| l.starts_with('[')) {
        return Vec::new();
    }
    let cells = |row: &str| -> Vec<String> {
        let (mut out, mut quoted) = (vec![String::new()], false);
        for c in row.chars() {
            match c {
                '"' => quoted = !quoted,
                ',' if !quoted => out.push(String::new()),
                _ => out.last_mut().unwrap().push(c),
            }
        }
        out.into_iter().map(|c| c.trim().to_ascii_lowercase()).collect()
    };
    let mut col = 4;
    if let Some(first) = rows.peek() {
        let h = cells(first);
        if h.iter().any(|c| c == "text") && let Some(i) = h.iter().position(|c| c == "tag" || c == "pool" || c == "situation") {
            col = i;
            rows.next();
        } else if h.iter().any(|c| c == "text") {
            return Vec::new();
        }
    }
    rows.map(|r| cells(r).get(col).is_some_and(|c| !c.is_empty())).collect()
}
