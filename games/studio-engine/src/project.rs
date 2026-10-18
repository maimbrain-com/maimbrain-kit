//! A Studio project, read from its files (docs/STUDIO-API.md, "The project").
//!
//! In a bundle the project is under `assets/` with a generated
//! `assets/files.txt` listing it; `mb build` makes both (docs/SPEC.md "Studio
//! bundles"). Here it's a map of project paths (`studio.toml`,
//! `game/main.lua`, `sprites/cat.toml`, …) to bytes, however they arrived.
//!
//! Parsing is plain Rust with no Lua: a problem in a file is an error that
//! names the file (and the line, for maps), shown on screen as a Lua error
//! is.

use std::collections::BTreeMap;

use toml::{Table, Value};

/// The Arcade canvas in art pixels, upright and sideways.
pub const PORTRAIT: (u32, u32) = (144, 256);
pub const SIDEWAYS: (u32, u32) = (256, 144);
/// A tile's side in art pixels.
pub const TILE: u32 = 16;
/// Sprite frames: 16×16, or 32×32 for big things; at most this many frames.
pub const MAX_FRAMES: u32 = 8;

/// The palette presets (`palette = "sunset16"`), 0xRRGGBB. mb-cli keeps a
/// copy for its palette check; a test there reads this file to keep them equal.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Sideways,
}

/// A thing's look when it has no sprite: a box or a ball in a palette colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub color: u8,
    pub circle: bool,
}

/// `sprites/<name>.toml` (and its PNG): a kind of thing.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub name: String,
    /// The PNG's project path, if it has one.
    pub png: Option<String>,
    /// One frame, in art pixels.
    pub frame: (u32, u32),
    /// Frames in the PNG (side by side), when known.
    pub frames: u32,
    /// Texels per art pixel in the PNG as shipped (5 in a bundle, where
    /// `mb build` scales sprites up for crisp drawing; 1 in a project).
    pub scale: u32,
    /// x, y, w, h within the frame.
    pub hitbox: (f32, f32, f32, f32),
    /// Name, frames, frames per second.
    pub anims: Vec<(String, Vec<u32>, f32)>,
    pub behaviours: Vec<String>,
    pub knobs: Vec<(String, f64)>,
    /// Drawn in order of layer, then spawn order.
    pub layer: i32,
    pub shape: Option<Shape>,
    /// What a `spawner` or `enemy.shooter` makes.
    pub spawns: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub name: String,
    pub frame: u32,
    pub solid: bool,
    pub oneway: bool,
    pub hazard: bool,
}

/// `tiles/<name>.toml` and its PNG: frames side by side, 16×16 each.
#[derive(Clone, Debug, PartialEq)]
pub struct TileSet {
    pub name: String,
    pub png: Option<String>,
    pub scale: u32,
    pub tiles: Vec<Tile>,
    /// A colour for tiles without a PNG (and natively, where no image loads).
    pub color: u8,
}

/// A thing a map places (or spawns over and over, with `every`).
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub every: Option<f32>,
    /// Spawn anywhere this wide (art pixels), centred on the cell.
    pub spread: f32,
    /// Other fields (vx, vy, …) copied onto the thing.
    pub props: Vec<(String, f64)>,
}

/// `maps/<name>.map`.
#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub name: String,
    /// In tiles.
    pub w: u32,
    pub h: u32,
    /// Row by row: (tile set, tile) indices.
    pub cells: Vec<Option<(u16, u16)>>,
    pub spawns: Vec<Spawn>,
    pub background: Option<u8>,
    pub music: Option<String>,
}

impl Map {
    pub fn cell(&self, cx: i64, cy: i64) -> Option<(u16, u16)> {
        if cx < 0 || cy < 0 || cx >= self.w as i64 || cy >= self.h as i64 {
            return None;
        }
        self.cells[(cy as u32 * self.w + cx as u32) as usize]
    }
}

/// A value a scenario sets on a Lua global.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Num(f64),
    Str(String),
    Bool(bool),
}

/// One of `debug/scenarios.toml`'s named sets of debug picks.
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

/// The whole project, parsed.
#[derive(Clone, Debug)]
pub struct Project {
    pub title: String,
    pub tagline: Option<String>,
    /// The game-over card's heading.
    pub over: String,
    /// The score's name on the card and the board.
    pub label: String,
    pub lower_is_better: bool,
    pub orientation: Orientation,
    /// Art pixels.
    pub canvas: (u32, u32),
    /// 0xRRGGBBAA.
    pub palette: [u32; 16],
    pub background: u8,
    pub lives: i64,
    /// The sprite the feed card shows.
    pub hero: Option<String>,
    pub sprites: Vec<Sprite>,
    pub tilesets: Vec<TileSet>,
    /// In level order.
    pub maps: Vec<Map>,
    /// `game/<name>.lua` by name ("main", …).
    pub scripts: BTreeMap<String, String>,
    pub sounds: Vec<String>,
    pub music: Vec<String>,
    pub scenarios: Vec<Scenario>,
}

/// Project paths to bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

impl Project {
    pub fn parse(files: &Files) -> Result<Project, String> {
        let text = |path: &str| -> Result<String, String> {
            let b = files.get(path).ok_or_else(|| format!("{path} is missing"))?;
            String::from_utf8(b.clone()).map_err(|_| format!("{path} isn't UTF-8 text"))
        };
        let st = table(&text("studio.toml")?, "studio.toml")?;
        let s = |k: &str| st.get(k).and_then(Value::as_str).map(str::to_string);
        let title = s("title").unwrap_or_else(|| "Untitled".into());
        let orientation = match s("orientation").as_deref() {
            None | Some("portrait") => Orientation::Portrait,
            Some("sideways") | Some("landscape") => Orientation::Sideways,
            Some(o) => return Err(format!("studio.toml: orientation = \"{o}\" (portrait or sideways)")),
        };
        let canvas = match orientation {
            Orientation::Portrait => PORTRAIT,
            Orientation::Sideways => SIDEWAYS,
        };
        if let Some(c) = st.get("canvas").and_then(Value::as_array) {
            let c: Vec<i64> = c.iter().filter_map(Value::as_integer).collect();
            if c != [canvas.0 as i64, canvas.1 as i64] {
                return Err(format!(
                    "studio.toml: canvas = {c:?}; an Arcade game is [{}, {}] {} (bigger canvases need the Lua rung)",
                    canvas.0,
                    canvas.1,
                    if orientation == Orientation::Portrait { "upright" } else { "sideways" }
                ));
            }
        }
        let palette = palette(st.get("palette"))?;
        let mut scores = st.get("scores").and_then(Value::as_array).cloned().unwrap_or_default();
        let board = if scores.is_empty() { Table::new() } else { scores.remove(0).as_table().cloned().unwrap_or_default() };
        let label = board.get("label").and_then(Value::as_str).unwrap_or("Score").to_string();
        let lower_is_better = matches!(board.get("order").and_then(Value::as_str), Some("low") | Some("lower"));
        let texts = st.get("text").and_then(Value::as_table).cloned().unwrap_or_default();
        let over = texts.get("over").and_then(Value::as_str).unwrap_or("GAME OVER").to_string();

        let mut p = Project {
            tagline: s("tagline"),
            over,
            label,
            lower_is_better,
            orientation,
            canvas,
            palette,
            background: color(st.get("background"), "studio.toml: background")?.unwrap_or(0),
            lives: st.get("lives").and_then(Value::as_integer).unwrap_or(1).clamp(1, 99),
            hero: s("hero"),
            title,
            sprites: Vec::new(),
            tilesets: Vec::new(),
            maps: Vec::new(),
            scripts: BTreeMap::new(),
            sounds: Vec::new(),
            music: Vec::new(),
            scenarios: Vec::new(),
        };

        for (path, bytes) in files {
            let Some((dir, file)) = path.split_once('/') else { continue };
            match (dir, file.rsplit_once('.')) {
                ("game", Some((name, "lua"))) => {
                    p.scripts.insert(name.to_string(), text(path)?);
                }
                ("sprites", Some((name, "toml"))) => {
                    let png = format!("sprites/{name}.png");
                    let png = files.get(&png).map(|b| (png, b.as_slice()));
                    p.sprites.push(sprite(name, &table(&text(path)?, path)?, png, path)?);
                }
                ("sprites", Some((name, "png"))) if !files.contains_key(&format!("sprites/{name}.toml")) => {
                    // A PNG with no table: one 16×16 frame (or 32×32), no behaviour.
                    p.sprites.push(sprite(name, &Table::new(), Some((path.clone(), bytes)), path)?);
                }
                ("tiles", Some((name, "toml"))) => {
                    let png = format!("tiles/{name}.png");
                    let png = files.get(&png).map(|b| (png, b.as_slice()));
                    p.tilesets.push(tileset(name, &table(&text(path)?, path)?, png, path)?);
                }
                ("sounds", Some((name, "ogg"))) => p.sounds.push(name.to_string()),
                ("music", Some((name, "ogg"))) => p.music.push(name.to_string()),
                _ => {}
            }
        }
        if !p.scripts.contains_key("main") {
            return Err("game/main.lua is missing".into());
        }
        let mut names: Vec<&str> = p.sprites.iter().map(|s| s.name.as_str()).collect();
        names.sort();
        if names.windows(2).any(|w| w[0] == w[1]) {
            return Err("two sprites have the same name".into());
        }

        // Levels: studio.toml's `levels`, or every map in name order.
        let mut order: Vec<String> = match st.get("levels").and_then(Value::as_array) {
            Some(l) => l.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            None => files.keys().filter_map(|k| k.strip_prefix("maps/")?.strip_suffix(".map").map(str::to_string)).collect(),
        };
        order.dedup();
        for name in order {
            let path = format!("maps/{name}.map");
            let src = text(&path).map_err(|e| format!("studio.toml: levels names {name}, but {e}"))?;
            p.maps.push(map(&name, &src, &path, &p)?);
        }
        if let Some(t) = files.get("debug/scenarios.toml") {
            let path = "debug/scenarios.toml";
            let t = String::from_utf8(t.clone()).map_err(|_| format!("{path} isn't UTF-8 text"))?;
            p.scenarios = scenarios(&table(&t, path)?, path)?;
        }
        if let Some(h) = &p.hero
            && !p.sprites.iter().any(|s| &s.name == h)
        {
            return Err(format!("studio.toml: hero = \"{h}\", but there's no sprites/{h}.toml or .png"));
        }
        Ok(p)
    }

    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.name == name)
    }

    pub fn map(&self, name: &str) -> Option<&Map> {
        self.maps.iter().find(|m| m.name == name)
    }

    /// The sprite the feed card shows: `hero`, else the first player.
    pub fn hero(&self) -> Option<&Sprite> {
        match &self.hero {
            Some(h) => self.sprite(h),
            None => self.sprites.iter().find(|s| s.behaviours.iter().any(|b| b.starts_with("player."))).or(self.sprites.first()),
        }
    }

    /// The palette colour `i` (0xRRGGBBAA), wrapping past 15.
    pub fn color(&self, i: u8) -> u32 {
        self.palette[(i & 15) as usize]
    }
}

fn table(text: &str, path: &str) -> Result<Table, String> {
    text.parse::<Table>().map_err(|e| format!("{path}: {}", one_line(&e.to_string())))
}

fn one_line(s: &str) -> String {
    s.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

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

/// A palette index (0–15).
fn color(v: Option<&Value>, what: &str) -> Result<Option<u8>, String> {
    match v {
        None => Ok(None),
        Some(Value::Integer(i)) if (0..16).contains(i) => Ok(Some(*i as u8)),
        Some(_) => Err(format!("{what} is a palette colour, 0–15")),
    }
}

fn num(v: &Value) -> Option<f64> {
    v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
}

fn pair(v: Option<&Value>) -> Option<(f64, f64)> {
    let a = v?.as_array()?;
    match a.as_slice() {
        [x, y] => Some((num(x)?, num(y)?)),
        _ => None,
    }
}

/// Width and height from a PNG's IHDR.
pub fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    let be = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    Some((be(16), be(20)))
}

fn sprite(name: &str, t: &Table, png: Option<(String, &[u8])>, path: &str) -> Result<Sprite, String> {
    let size = png.as_ref().and_then(|(_, b)| png_size(b));
    let frame = match pair(t.get("frame")) {
        Some((w, h)) => (w as u32, h as u32),
        None => match size {
            // Without `frame`, a PNG 32 high is a big thing; else 16×16.
            Some((_, h)) if h == 32 || h == 32 * 5 => (32, 32),
            _ => (16, 16),
        },
    };
    if !matches!(frame, (16, 16) | (32, 32)) {
        return Err(format!("{path}: frame = [{}, {}]; Arcade sprites are 16×16, or 32×32 for big things", frame.0, frame.1));
    }
    let scale = size.map_or(1, |(_, h)| (h / frame.1).max(1));
    let frames = size.map_or(1, |(w, _)| (w / (frame.0 * scale)).max(1));
    if frames > MAX_FRAMES {
        return Err(format!("{path}: {frames} frames; an Arcade sprite has at most {MAX_FRAMES}"));
    }
    let hitbox = match t.get("hitbox").and_then(Value::as_array) {
        Some(a) if a.len() == 4 => {
            let v: Vec<f64> = a.iter().filter_map(num).collect();
            if v.len() != 4 || v[2] <= 0.0 || v[3] <= 0.0 {
                return Err(format!("{path}: hitbox = [x, y, w, h] in the frame's pixels"));
            }
            (v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32)
        }
        Some(_) => return Err(format!("{path}: hitbox = [x, y, w, h] in the frame's pixels")),
        None => match t.get("size").and_then(|v| pair(Some(v))) {
            // A shape's size when it has no PNG.
            Some((w, h)) => (0.0, 0.0, w as f32, h as f32),
            None => (0.0, 0.0, frame.0 as f32, frame.1 as f32),
        },
    };
    let fps = t.get("fps").and_then(num).unwrap_or(8.0) as f32;
    let mut anims = Vec::new();
    if let Some(a) = t.get("anims").and_then(Value::as_table) {
        for (k, v) in a {
            let (list, rate) = match v {
                Value::Array(l) => (l.clone(), fps),
                Value::Table(tt) => (
                    tt.get("frames").and_then(Value::as_array).cloned().unwrap_or_default(),
                    tt.get("fps").and_then(num).map_or(fps, |f| f as f32),
                ),
                _ => return Err(format!("{path}: anims.{k} is a list of frames or {{ frames = [...], fps = 8 }}")),
            };
            let list: Vec<u32> = list.iter().filter_map(Value::as_integer).map(|i| i.max(0) as u32).collect();
            if list.is_empty() {
                return Err(format!("{path}: anims.{k} has no frames"));
            }
            if let Some(f) = list.iter().find(|&&f| size.is_some() && f >= frames) {
                return Err(format!("{path}: anims.{k} uses frame {f}, but the PNG has {frames} (0–{})", frames - 1));
            }
            anims.push((k.clone(), list, rate.max(0.1)));
        }
    }
    if !anims.iter().any(|a| a.0 == "idle") {
        // Every frame, in order.
        anims.insert(0, ("idle".into(), (0..frames).collect(), fps));
    }
    let behaviours: Vec<String> = match (t.get("behaviour"), t.get("behaviours")) {
        (Some(Value::String(b)), _) => vec![b.clone()],
        (_, Some(Value::Array(l))) => l.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        (None, None) => Vec::new(),
        _ => return Err(format!("{path}: behaviour = \"player.drag\" (or behaviours = [\"enemy.patrol\", \"hazard\"])")),
    };
    let knobs = t
        .get("knobs")
        .and_then(Value::as_table)
        .map(|k| k.iter().filter_map(|(n, v)| Some((n.clone(), num(v)?))).collect())
        .unwrap_or_default();
    let shape = match png {
        Some(_) => None,
        None => Some(Shape { color: color(t.get("color"), &format!("{path}: color"))?.unwrap_or(7), circle: t.get("shape").and_then(Value::as_str) == Some("circle") }),
    };
    let layer = t.get("layer").and_then(Value::as_integer).unwrap_or(if behaviours.iter().any(|b| b.starts_with("player.")) { 1 } else { 0 }) as i32;
    let spawns = t.get("spawns").and_then(Value::as_str).map(str::to_string);
    Ok(Sprite { name: name.to_string(), png: png.map(|(p, _)| p), frame, frames, scale, hitbox, anims, behaviours, knobs, layer, shape, spawns })
}

fn tileset(name: &str, t: &Table, png: Option<(String, &[u8])>, path: &str) -> Result<TileSet, String> {
    let size = png.as_ref().and_then(|(_, b)| png_size(b));
    let scale = size.map_or(1, |(_, h)| (h / TILE).max(1));
    let frames = size.map_or(u32::MAX, |(w, _)| (w / (TILE * scale)).max(1));
    let mut tiles = Vec::new();
    match t.get("tiles").and_then(Value::as_table) {
        Some(list) => {
            for (k, v) in list {
                let v = v.as_table().ok_or_else(|| format!("{path}: tiles.{k} is a table: {{ frame = 0, solid = true }}"))?;
                let b = |f: &str| v.get(f).and_then(Value::as_bool).unwrap_or(false);
                let frame = v.get("frame").and_then(Value::as_integer).unwrap_or(0).max(0) as u32;
                if frame >= frames {
                    return Err(format!("{path}: tiles.{k} is frame {frame}, but the PNG has {frames}"));
                }
                tiles.push(Tile { name: k.clone(), frame, solid: b("solid"), oneway: b("oneway"), hazard: b("hazard") });
            }
        }
        None => tiles.push(Tile { name: "tile".into(), frame: 0, solid: true, oneway: false, hazard: false }),
    }
    // The bare set name ("ground") means its frame-0 tile.
    tiles.sort_by_key(|t| t.frame);
    Ok(TileSet { name: name.to_string(), png: png.map(|(p, _)| p), scale, tiles, color: color(t.get("color"), &format!("{path}: color"))?.unwrap_or(13) })
}

fn map(name: &str, src: &str, path: &str, p: &Project) -> Result<Map, String> {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| l.trim() == "[map]").ok_or_else(|| format!("{path}: no [map] line (the rows of the map go after it)"))?;
    let header = table(&lines[..at].join("\n"), path)?;
    let mut legend: BTreeMap<char, Value> = BTreeMap::new();
    if let Some(l) = header.get("legend").and_then(Value::as_table) {
        for (k, v) in l {
            let mut cs = k.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) if c != '.' && c != ' ' => {
                    legend.insert(c, v.clone());
                }
                _ => return Err(format!("{path}: legend key {k:?} must be one character (not '.' or a space, which are empty)")),
            }
        }
    }
    let rows: Vec<&str> = lines[at + 1..].iter().map(|l| l.trim_end()).collect();
    let rows: Vec<&str> = {
        let end = rows.iter().rposition(|r| !r.is_empty()).map_or(0, |i| i + 1);
        rows[..end].to_vec()
    };
    let h = rows.len() as u32;
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    if w == 0 || h == 0 {
        return Err(format!("{path}: the map has no rows"));
    }
    if w > 256 || h > 256 {
        return Err(format!("{path}: the map is {w}×{h} tiles; at most 256×256"));
    }
    let mut cells = vec![None; (w * h) as usize];
    let mut spawns = Vec::new();
    for (cy, row) in rows.iter().enumerate() {
        for (cx, c) in row.chars().enumerate() {
            if c == '.' || c == ' ' {
                continue;
            }
            let line = at + 2 + cy;
            let v = legend.get(&c).ok_or_else(|| format!("{path}:{line}: '{c}' isn't in the legend"))?;
            let (x, y) = ((cx as u32 * TILE + TILE / 2) as f32, (cy as u32 * TILE + TILE / 2) as f32);
            match v {
                Value::String(r) => {
                    let (set, tile) = r.split_once('.').map_or((r.as_str(), None), |(a, b)| (a, Some(b)));
                    let si = p.tilesets.iter().position(|t| t.name == set).ok_or_else(|| format!("{path}:{line}: '{c}' is tile \"{r}\", but there's no tiles/{set}.toml"))?;
                    let ts = &p.tilesets[si];
                    let ti = match tile {
                        None => 0,
                        Some(t) => ts.tiles.iter().position(|x| x.name == t).ok_or_else(|| {
                            format!("{path}:{line}: '{c}' is tile \"{r}\", but tiles/{set}.toml has no tile \"{t}\" ({})", ts.tiles.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "))
                        })?,
                    };
                    cells[cy * w as usize + cx] = Some((si as u16, ti as u16));
                }
                Value::Table(t) => {
                    let kind = t.get("thing").and_then(Value::as_str).ok_or_else(|| format!("{path}: legend '{c}' needs thing = \"<kind>\" (or a tile name)"))?;
                    let mut props = Vec::new();
                    for (k, v) in t {
                        if !matches!(k.as_str(), "thing" | "every" | "spread")
                            && let Some(n) = num(v)
                        {
                            props.push((k.clone(), n));
                        }
                    }
                    spawns.push(Spawn {
                        kind: kind.to_string(),
                        x,
                        y,
                        every: t.get("every").and_then(num).map(|e| e as f32).filter(|e| *e > 0.0),
                        spread: t.get("spread").and_then(num).unwrap_or(0.0) as f32,
                        props,
                    });
                }
                _ => return Err(format!("{path}: legend '{c}' is a tile name or {{ thing = \"<kind>\" }}")),
            }
        }
    }
    Ok(Map {
        name: name.to_string(),
        w,
        h,
        cells,
        spawns,
        background: color(header.get("background"), &format!("{path}: background"))?,
        music: header.get("music").and_then(Value::as_str).map(str::to_string),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Files {
        list.iter().map(|(k, v)| (k.to_string(), v.as_bytes().to_vec())).collect()
    }

    #[test]
    fn a_small_project_parses() {
        let f = files(&[
            ("studio.toml", "title = \"Cat Snack\"\npalette = \"sunset16\"\nlives = 3\n[[scores]]\nlabel = \"Fish\"\norder = \"high\"\n"),
            ("game/main.lua", "function start() end"),
            ("sprites/cat.toml", "behaviour = \"player.drag\"\nhitbox = [2, 6, 12, 10]\n[knobs]\nspeed = 140\n"),
            ("sprites/fish.toml", "behaviours = [\"pickup\"]\ncolor = 11\nshape = \"circle\"\nsize = [8, 8]\n"),
            ("tiles/ground.toml", "[tiles.top]\nframe = 0\nsolid = true\n[tiles.ledge]\nframe = 1\noneway = true\n"),
            ("maps/level1.map", "[legend]\n\"#\" = \"ground\"\n\"=\" = \"ground.ledge\"\n\"F\" = { thing = \"fish\", every = 1.2, spread = 120 }\n\"C\" = { thing = \"cat\" }\n\n[map]\n....F....\n.........\n..===....\n....C....\n#########\n"),
            ("debug/scenarios.toml", "[[scenario]]\nname = \"Gold rush\"\nscore = 38\n[scenario.knobs]\n\"fish.value\" = 5\n[scenario.set]\ngold = true\n"),
        ]);
        let p = Project::parse(&f).unwrap();
        assert_eq!((p.title.as_str(), p.label.as_str(), p.lives, p.lower_is_better), ("Cat Snack", "Fish", 3, false));
        assert_eq!(p.canvas, PORTRAIT);
        let cat = p.sprite("cat").unwrap();
        assert_eq!((cat.hitbox, cat.layer, &cat.knobs[..]), ((2.0, 6.0, 12.0, 10.0), 1, &[("speed".to_string(), 140.0)][..]));
        assert_eq!(p.sprite("fish").unwrap().shape, Some(Shape { color: 11, circle: true }));
        let m = &p.maps[0];
        assert_eq!((m.w, m.h), (9, 5));
        assert_eq!(m.cell(0, 4), Some((0, 0)));
        assert_eq!(m.cell(2, 2), Some((0, 1)));
        assert_eq!(m.spawns[0], Spawn { kind: "fish".into(), x: 72.0, y: 8.0, every: Some(1.2), spread: 120.0, props: vec![] });
        assert_eq!(p.scenarios[0].knobs, vec![("fish".into(), "value".into(), 5.0)]);
        assert_eq!(p.scenarios[0].set, vec![("gold".into(), Scalar::Bool(true))]);
        assert_eq!(p.hero().unwrap().name, "cat");
    }

    #[test]
    fn problems_name_the_file_and_line() {
        let base = [("studio.toml", "title = \"x\""), ("game/main.lua", "")];
        let with = |extra: &[(&str, &str)]| {
            let mut f = files(&base);
            f.extend(files(extra));
            Project::parse(&f).unwrap_err()
        };
        assert_eq!(with(&[("maps/a.map", "[legend]\n[map]\n..\n.X\n")]), "maps/a.map:4: 'X' isn't in the legend");
        assert!(with(&[("sprites/a.toml", "frame = [24, 24]")]).contains("16×16, or 32×32"));
        assert!(with(&[("maps/a.map", "[legend]\n\"#\" = \"rock\"\n[map]\n#\n")]).contains("no tiles/rock.toml"));
        let mut f = files(&base);
        f.remove("game/main.lua");
        assert_eq!(Project::parse(&f).unwrap_err(), "game/main.lua is missing");
        let f = files(&[("studio.toml", "palette = \"nope\""), ("game/main.lua", "")]);
        assert!(Project::parse(&f).unwrap_err().contains("presets: sunset16, night16"));
    }
}
