//! `mb art`: generate images for a game and post-process them to the
//! platform's rules.
//!
//!     mb art init games/moth                         # art/style.toml
//!     mb art sprite games/moth hero "a moth with stained-glass wings"
//!     mb art frames games/moth flap "the moth flapping" --frames 4
//!     mb art background games/moth sky "a night sky over a cathedral"
//!     mb art layers games/moth hills "rolling hills at dusk" --count 3
//!     mb art tile games/moth glass "leaded glass panes"
//!     mb art sky games/canal night "a moonless night over distant lantern-lit rooftops"
//!     mb art ui games/moth button "a round glass button"
//!     mb art icon games/moth "the moth's face, glowing"
//!     mb art sprite games/moth cat "our cat as the hero" --ref ~/cat.jpg
//!     mb art atlas games/moth sprites assets/hero.png assets/flap.png
//!     mb art check games/moth
//!
//! Every kind: the style guide is applied, the prompt is checked for
//! someone else's IP, the cost is shown (and capped), the raw output is kept
//! in art/raw/, the result goes to assets/ with a provenance sidecar, and a
//! preview goes to art/preview/ to look at.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::img::{self, Anchor, Image};
use super::provider::{self, ImageRequest, Media, MockHint, RefImage};
use super::style::{self, PromptParts, Style};
use super::{FileRef, Sidecar};

#[derive(Subcommand)]
pub enum ArtCmd {
    /// Write a starting style guide, art/style.toml (palette, style words, references).
    Init {
        game: PathBuf,
        /// Overwrite an existing one.
        #[arg(long)]
        force: bool,
    },
    /// A sprite with a transparent background (characters, enemies, items).
    Sprite(Gen),
    /// Animation frames packed into one sheet of equal cells.
    Frames {
        #[command(flatten)]
        gen_: Gen,
        /// Number of frames.
        #[arg(long, default_value_t = 4)]
        frames: u32,
        /// Columns in the sheet (default: all frames in one row, up to 8).
        #[arg(long)]
        cols: Option<u32>,
        /// A character for `maimbrain::walk`: two cells, the body without feet and one foot, at one
        /// scale; the game steps the feet by distance walked, so they never slide (`--frames` is ignored).
        #[arg(long)]
        walker: bool,
    },
    /// A talking portrait for dialogue (`ui::Portrait`): one character's head and shoulders in a sheet
    /// of square cells, 4 columns of mouth shapes (closed, slightly open, open, wide open) × one row
    /// per expression (neutral first), plus a last row with the eyes closed (blink).
    Portrait {
        #[command(flatten)]
        gen_: Gen,
        /// Expressions after neutral, one row each: --expressions smug,angry,scared (default: neutral only).
        #[arg(long, value_delimiter = ',')]
        expressions: Vec<String>,
    },
    /// A full-screen, opaque background (default: the game's logical size × 2).
    Background(Gen),
    /// Parallax layers, far to near (<name>_0.png …): the far one opaque, the rest transparent, all wrapping horizontally.
    Layers {
        #[command(flatten)]
        gen_: Gen,
        #[arg(long, default_value_t = 3)]
        count: u32,
        /// Layer width in screens (they scroll and wrap).
        #[arg(long, default_value_t = 2.0)]
        screens: f32,
    },
    /// A seamless, tileable square texture.
    Tile(Gen),
    /// An mb3d sky: a 2:1 equirectangular panorama (assets/<name>.jpg, 2048×1024) whose left and
    /// right edges meet and whose poles are calm, for `gfx3d::sky_texture`.
    Sky {
        #[command(flatten)]
        gen_: Gen,
        /// Where the generated image's horizon is, 0–1 from its top; `auto` (the default) finds it
        /// (the sharpest bright-to-dark step in the middle third), else 0.5.
        #[arg(long, default_value = "auto")]
        horizon: String,
        /// Degrees of elevation the generated image covers: 180 if it's a true equirect (the default);
        /// less if the model drew an ordinary wide view (the rest is filled with its top and bottom colors).
        #[arg(long, default_value_t = 180.0)]
        span: f32,
        /// Within this many degrees of straight up and straight down, the sky blends into one even color.
        #[arg(long, default_value_t = 12.0)]
        poles: f32,
    },
    /// A UI element (button, panel, badge) with a transparent background.
    Ui(Gen),
    /// The game's icon.png (256×256, opaque): a close-up of your hero or hook, no text.
    Icon {
        game: PathBuf,
        prompt: String,
        #[command(flatten)]
        opts: Opts,
    },
    /// Use one of the candidates made with --variants N.
    Pick { game: PathBuf, name: String, variant: u32 },
    /// Pack PNGs into one atlas PNG plus a Rust table of source rects.
    Atlas {
        game: PathBuf,
        /// Atlas name: writes assets/<name>.png and src/<name>.rs.
        name: String,
        /// PNG files to pack (default: every generated sprite and ui PNG in assets/).
        inputs: Vec<PathBuf>,
        /// Clear texels between sprites (stops linear filtering bleeding).
        #[arg(long, default_value_t = 2)]
        padding: u32,
        /// Trim transparent borders (adds <NAME>_OFFSET and <NAME>_SIZE constants).
        #[arg(long)]
        trim: bool,
        /// Where to write the Rust table (default src/<name>.rs).
        #[arg(long)]
        rust: Option<PathBuf>,
        /// Delete the packed source PNGs and their sidecars afterwards.
        #[arg(long)]
        remove_inputs: bool,
    },
    /// Check a game's assets against the budgets and list what was generated, and how.
    Check { game: PathBuf },
    /// Write src/art_meta.rs: each generated figure's feet and height and each painting's floor
    /// line, measured from the images as they are now (`mb art` does it after every generation).
    Meta { game: PathBuf },
    /// Record objects a generated layer or background shows that `mb art` didn't find in its
    /// prompt (look at its preview): `mb art shows assets/attic_1.png "rocking horse, birdcage"`.
    /// `mb art` then warns when a prop is one of them.
    Shows {
        /// The painting (it has a .gen.json).
        asset: PathBuf,
        /// What it shows, comma-separated.
        objects: String,
    },
    /// Record an edit made to a generated asset after mb wrote it (a script, a sound editor, by
    /// hand) in its sidecar, so `mb art check` says how the file was made. Run it after the edit:
    /// `mb art edited assets/plunge.ogg --tool tools/bright.py -- --shelf 6`.
    Edited {
        /// The edited asset (it has a .gen.json).
        asset: PathBuf,
        /// What edited it: a script in the game (tools/bright.py), a program, or "hand".
        #[arg(long)]
        tool: String,
        /// What the edit did, in a few words.
        #[arg(long)]
        note: Option<String>,
        /// The tool's arguments, after `--`.
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(Args, Clone)]
pub struct Gen {
    /// Game directory.
    pub game: PathBuf,
    /// Asset name: writes assets/<name>.png.
    pub name: String,
    /// What it is: describe it plainly, as something original.
    pub prompt: String,
    #[command(flatten)]
    pub opts: Opts,
}

#[derive(Args, Clone, Default)]
pub struct Opts {
    /// Output size in PNG pixels, e.g. 128x128 (the canvas has 2 pixels per logical unit).
    #[arg(long, value_parser = parse_wh)]
    pub size: Option<[u32; 2]>,
    /// Pixel art: the grid in art pixels, e.g. 32x32 (output = grid × the style's pixel_scale).
    #[arg(long, value_parser = parse_wh)]
    pub grid: Option<[u32; 2]>,
    /// A photo or sketch whose subject to redraw in the game's style (repeatable).
    #[arg(long = "ref")]
    pub refs: Vec<PathBuf>,
    /// Seed (recorded; reuse it to reproduce where the provider honors seeds).
    #[arg(long)]
    pub seed: Option<u64>,
    /// gemini (default), vertex, openai or mock; maimbrain uses Maimbrain's keys (if your account has access).
    #[arg(long)]
    pub provider: Option<String>,
    /// Model id (see `mb models`).
    #[arg(long)]
    pub model: Option<String>,
    /// Only use your own API keys, never Maimbrain's (also MB_KEYS_SOURCE=own).
    #[arg(long)]
    pub own_keys: bool,
    /// Generation resolution: 0.5K, 1K, 2K or 4K (cost goes up with it).
    #[arg(long)]
    pub res: Option<String>,
    /// OpenAI quality: low, medium (default) or high.
    #[arg(long)]
    pub quality: Option<String>,
    /// Make N candidates in art/variants/ plus a contact sheet; `mb art pick` one.
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    /// Where the subject sits: center, or bottom (feet on the bottom margin).
    #[arg(long, value_enum)]
    pub anchor: Option<Anchor>,
    /// Empty margin around a sprite, in output pixels (pixel art: art pixels).
    #[arg(long)]
    pub margin: Option<u32>,
    /// Sprites and frames: how tall the figure stands in the game, in logical units (`--height 96`),
    /// or against another generated figure (`--height 1.3xgirl`). Sizes the image to match, and
    /// `ground::Figure` draws it that tall (src/art_meta.rs), so figures come out the right size
    /// against each other.
    #[arg(long, value_parser = parse_height, value_name = "UNITS|NxNAME")]
    pub height: Option<Height>,
    /// Sprites and frames: how tall the thing is in the real world (`--real 1.6m`, `40cm`, `6ft`);
    /// default: a typical height for what the prompt names (a mirror 1.6 m, a bell 15 cm). With a
    /// figure already sized by --height, a prop without --height is sized against it (src/art_meta.rs,
    /// `ground::Scale`), and `mb art check` warns when sizes don't agree.
    #[arg(long, value_parser = super::scene::parse_real, value_name = "HEIGHT")]
    pub real: Option<f32>,
    /// Layers and backgrounds: objects the painting shows besides those its prompt names
    /// (`--shows "rocking horse,trunk"`), so `mb art` warns when a prop duplicates one.
    #[arg(long, value_delimiter = ',', value_name = "OBJECTS")]
    pub shows: Vec<String>,
    /// Layers and backgrounds: put the painting's floor line this far down (0–1 from the top;
    /// default `floor` under [layer] or [background] in art/style.toml), so every painting's floor
    /// is on the same line.
    #[arg(long, value_name = "0-1")]
    pub floor: Option<f32>,
    /// Quantize to at most N colors.
    #[arg(long)]
    pub colors: Option<u32>,
    /// Size budget for this PNG in KB: quantize until it fits.
    #[arg(long)]
    pub max_kb: Option<u32>,
    /// Output path relative to the game (default assets/<name>.png).
    #[arg(long)]
    pub out: Option<String>,
    /// Ignore art/style.toml.
    #[arg(long)]
    pub no_style: bool,
    /// Keep the generated background (no transparency).
    #[arg(long)]
    pub keep_background: bool,
    /// Background keying tolerance (Oklab distance; default 0.09; raise if a halo is left).
    #[arg(long)]
    pub tolerance: Option<f64>,
    /// Let a flagged name through (it's yours, or a false positive). Recorded in the sidecar.
    #[arg(long)]
    pub allow_name: Vec<String>,
    /// Refuse if the estimated cost of this command is above this many USD (default 1.00, or MB_MAX_COST).
    #[arg(long)]
    pub max_cost: Option<f64>,
    /// Print the prompt and the cost without calling the provider.
    #[arg(long)]
    pub dry_run: bool,
}

fn parse_horizon(s: &str) -> Result<Option<f32>, String> {
    if s == "auto" {
        return Ok(None);
    }
    match s.parse::<f32>() {
        Ok(v) if (0.0..=1.0).contains(&v) => Ok(Some(v)),
        _ => Err("expected auto, or 0–1 (the horizon's height from the image's top)".into()),
    }
}

/// `--height`: logical units, or a multiple of another figure's height.
#[derive(Clone, Debug, PartialEq)]
pub enum Height {
    Units(f32),
    Of(f32, String),
}

fn parse_height(s: &str) -> Result<Height, String> {
    let bad = || format!("--height {s}: logical units (96), or a multiple of another figure's height (1.3xgirl)");
    if let Ok(v) = s.trim().parse::<f32>() {
        return if v > 0.0 && v <= 2048.0 { Ok(Height::Units(v)) } else { Err(bad()) };
    }
    let (k, name) = s.split_once('x').ok_or_else(bad)?;
    let k: f32 = k.trim().parse().map_err(|_| bad())?;
    if !(k > 0.0 && k <= 20.0) || name.trim().is_empty() {
        return Err(bad());
    }
    Ok(Height::Of(k, name.trim().to_string()))
}

/// How tall another generated figure is drawn (its `--height`, else its measured height).
fn figure_height(game: &Path, name: &str) -> Result<f32, String> {
    let png = game.join("assets").join(format!("{name}.png"));
    match super::ground::meta_of(&png, game) {
        Some(super::ground::Meta::Figure { height, .. }) => Ok(height),
        _ => Err(format!("--height …x{name}: {} isn't a generated sprite or frames sheet", png.display())),
    }
}

/// How tall a new figure is drawn when nothing says: its real height (`--real`, else the table's
/// for its subject) at the scale of the figures already sized with --height. A thing held in a
/// hand is drawn at least 16 units, bigger than life, so it reads.
fn scaled_height(game: &Path, name: &str, subject: &str, real: Option<f32>) -> Option<(f32, String)> {
    let (m, what) = match real {
        Some(m) => (m, format!("{m} m")),
        None => super::scene::typical_height(subject).map(|(m, t)| (m, format!("a {m} m {t}")))?,
    };
    let metas: Vec<_> = super::ground::game_meta(game).into_iter().filter(|m| m.name() != name).collect();
    let (k, hero) = super::ground::scene_scale(&metas)?;
    let units = ((m * k).max(16.0) * 10.0).round() / 10.0;
    eprintln!("  sized {units} units tall: {what} at {hero}'s scale ({k:.1} units a meter); --height to choose, --real to correct");
    Some((units, hero))
}

/// The square image a figure `units` tall fills at 2 texels a unit, with the default margin.
fn size_for_height(units: f32, scale: u32) -> [u32; 2] {
    let side = (units * 2.0 * 8.0 / 7.0).ceil() as u32;
    let side = side.div_ceil(scale.max(1)) * scale.max(1);
    let side = side.clamp(16, 2048);
    [side, side]
}

fn parse_wh(s: &str) -> Result<[u32; 2], String> {
    let (w, h) = s.split_once(['x', 'X', '×']).ok_or("expected WxH, e.g. 128x128")?;
    let w: u32 = w.trim().parse().map_err(|_| "bad width")?;
    let h: u32 = h.trim().parse().map_err(|_| "bad height")?;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err("each side must be 1–4096 (mb2d's image limit)".into());
    }
    Ok([w, h])
}

/// The resolved recipe for one image: stored in the sidecar, replayed by `mb regen`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Job {
    pub kind: String,
    pub name: String,
    pub subject: String,
    pub out: String,
    pub size: [u32; 2],
    #[serde(default)]
    pub grid: Option<[u32; 2]>,
    /// The grid came from --grid (kept on reprocess); otherwise it follows the style's pixel_scale.
    #[serde(default)]
    pub grid_explicit: bool,
    #[serde(default)]
    pub refs: Vec<String>,
    pub seed: u64,
    pub provider: String,
    pub model: String,
    pub resolution: String,
    #[serde(default)]
    pub quality: String,
    pub aspect: String,
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default)]
    pub margin: u32,
    #[serde(default)]
    pub colors: Option<u32>,
    #[serde(default)]
    pub max_kb: Option<u32>,
    #[serde(default)]
    pub frames: u32,
    #[serde(default)]
    pub cols: u32,
    /// Talking portraits: the expression of each row, neutral first (the blink row follows).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<String>,
    /// (index, count) for parallax layers.
    #[serde(default)]
    pub layer: Option<(u32, u32)>,
    #[serde(default)]
    pub keyed: bool,
    /// The flat background color the model was asked for (`key` in
    /// art/style.toml when it was made), which keying looks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default = "tol")]
    pub tolerance: f64,
    #[serde(default)]
    pub no_style: bool,
    #[serde(default)]
    pub allow_names: Vec<String>,
    /// Skies: the generated image's horizon (0–1 from the top; None = find it), the degrees of
    /// elevation it covers, the calm polar caps (degrees).
    #[serde(default)]
    pub horizon: Option<f32>,
    #[serde(default = "full_span")]
    pub span: f32,
    #[serde(default = "pole_caps")]
    pub poles: f32,
    /// Sprites and frames: how tall the figure is drawn, in logical units (`--height`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Sprites and frames: the real-world height in meters (`--real`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real: Option<f32>,
    /// Sprites and frames sized by the scene's scale (no --height): the figure the scale came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sized_by: Option<String>,
    /// Layers and backgrounds: objects it shows that its prompt doesn't name (`--shows`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shows: Vec<String>,
    /// Opaque layers and backgrounds: where the floor line goes (0–1 from the top; `--floor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<f32>,
    /// Frames: a feetless body and a foot for `maimbrain::walk` (`--walker`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub walker: bool,
}

fn full_span() -> f32 {
    180.0
}

fn pole_caps() -> f32 {
    12.0
}

fn tol() -> f64 {
    0.09
}

pub const ASPECTS: &[(&str, f32)] = &[
    ("1:1", 1.0),
    ("2:3", 0.6667),
    ("3:2", 1.5),
    ("3:4", 0.75),
    ("4:3", 1.3333),
    ("4:5", 0.8),
    ("5:4", 1.25),
    ("9:16", 0.5625),
    ("16:9", 1.7778),
    ("21:9", 2.3333),
];

/// The supported aspect ratio closest to w/h (in log space).
pub fn nearest_aspect(w: f32, h: f32) -> &'static str {
    let r = (w / h).ln();
    ASPECTS.iter().min_by(|a, b| (a.1.ln() - r).abs().partial_cmp(&(b.1.ln() - r).abs()).unwrap()).unwrap().0
}

pub fn run(cmd: ArtCmd) -> Result<(), String> {
    match cmd {
        ArtCmd::Init { game, force } => init(&game, force),
        ArtCmd::Sprite(g) => generate_kind("sprite", &g.game, &g.name, &g.prompt, &g.opts, Extra::default()),
        ArtCmd::Frames { gen_: g, frames, cols, walker } => {
            let (frames, cols) = if walker { (2, Some(2)) } else { (frames, cols) };
            generate_kind("frames", &g.game, &g.name, &g.prompt, &g.opts, Extra { frames, cols: cols.unwrap_or(0), walker, ..Extra::default() })
        }
        ArtCmd::Portrait { gen_: g, expressions } => {
            let expressions = portrait_expressions(&expressions)?;
            generate_kind("portrait", &g.game, &g.name, &g.prompt, &g.opts, Extra { expressions, ..Extra::default() })
        }
        ArtCmd::Background(g) => generate_kind("background", &g.game, &g.name, &g.prompt, &g.opts, Extra::default()),
        ArtCmd::Layers { gen_: g, count, screens } => layers(&g, count, screens),
        ArtCmd::Tile(g) => generate_kind("tile", &g.game, &g.name, &g.prompt, &g.opts, Extra::default()),
        ArtCmd::Sky { gen_: g, horizon, span, poles } => {
            let horizon = parse_horizon(&horizon)?;
            if !(10.0..=180.0).contains(&span) || !(0.0..=45.0).contains(&poles) {
                return Err("--span is 10–180 degrees, --poles 0–45 degrees".into());
            }
            generate_kind("sky", &g.game, &g.name, &g.prompt, &g.opts, Extra { sky: Some((horizon, span, poles)), ..Extra::default() })
        }
        ArtCmd::Ui(g) => generate_kind("ui", &g.game, &g.name, &g.prompt, &g.opts, Extra::default()),
        ArtCmd::Icon { game, prompt, opts } => generate_kind("icon", &game, "icon", &prompt, &opts, Extra::default()),
        ArtCmd::Pick { game, name, variant } => pick(&game, &name, variant),
        ArtCmd::Atlas { game, name, inputs, padding, trim, rust, remove_inputs } => {
            super::atlas::run(&game, &name, &inputs, padding, trim, rust.as_deref(), remove_inputs)
        }
        ArtCmd::Check { game } => check(&game),
        ArtCmd::Meta { game } => match super::ground::write_meta(&game)? {
            Some(p) => {
                println!("{}", p.display());
                Ok(())
            }
            None if game.join("src").is_dir() => {
                eprintln!("src/art_meta.rs is up to date");
                Ok(())
            }
            None => Err(format!("{} has no src/ to write art_meta.rs into", game.display())),
        },
        ArtCmd::Shows { asset, objects } => record_shows(&asset, &objects),
        ArtCmd::Edited { asset, tool, note, args } => super::record_edit(&asset, &tool, &args, note.as_deref()),
    }
}

fn init(game: &Path, force: bool) -> Result<(), String> {
    let art = super::ensure_art_dir(game)?;
    let path = art.join("style.toml");
    if path.exists() && !force {
        return Err(format!("{} exists (--force to overwrite)", path.display()));
    }
    let name = std::fs::read_to_string(game.join("manifest.toml"))
        .ok()
        .and_then(|t| mb_format::Manifest::parse(&t).ok())
        .map(|m| m.name)
        .unwrap_or_else(|| game.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    std::fs::write(&path, style::TEMPLATE.replace("NAME", &name)).map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::create_dir_all(art.join("ref")).map_err(|e| e.to_string())?;
    eprintln!("wrote {} — set the style words and palette, then `mb art sprite {} hero \"...\"`", path.display(), game.display());
    eprintln!("put style reference images in {} and list them under references", art.join("ref").display());
    Ok(())
}

#[derive(Default, Clone)]
struct Extra {
    frames: u32,
    cols: u32,
    layer: Option<(u32, u32)>,
    screens: f32,
    /// Skies: horizon (None = find it), span, poles.
    sky: Option<(Option<f32>, f32, f32)>,
    /// Talking portraits: the rows' expressions, neutral first.
    expressions: Vec<String>,
    /// Frames: a body without feet and a foot, for `maimbrain::walk`.
    walker: bool,
}

/// A portrait's rows: neutral, then the creator's expressions (trimmed, lowercased, each once).
fn portrait_expressions(list: &[String]) -> Result<Vec<String>, String> {
    let mut out = vec!["neutral".to_string()];
    for e in list {
        let e = e.trim().to_lowercase();
        if e.is_empty() || out.contains(&e) {
            continue;
        }
        if e == "blink" {
            return Err("the blink row is always added (the last row); leave it out of --expressions".into());
        }
        if e.len() > 40 || e.contains(['\n', ';']) {
            return Err(format!("--expressions {e:?}: name each expression in a word or two (smug, angry, scared)"));
        }
        out.push(e);
    }
    if out.len() > 8 {
        return Err(format!("{} expressions is too many for one sheet (at most 8 with neutral); make two portraits", out.len()));
    }
    Ok(out)
}

fn default_size(kind: &str, style: &Style, screen: [u32; 2], extra: &Extra) -> [u32; 2] {
    if let Some(s) = style.kind(kind).size {
        return s;
    }
    match kind {
        // Portraits in pixel art: 48×48 art pixels a cell (a face needs that many to read).
        "portrait" if style.pixel_art => [48 * style.pixel_scale.max(1), 48 * style.pixel_scale.max(1)],
        "sprite" | "frames" | "portrait" => [128, 128],
        "background" => [screen[0] * 2, screen[1] * 2],
        "layer" => [((screen[0] * 2) as f32 * extra.screens.max(1.0)).round() as u32, screen[1] * 2],
        "tile" => [64, 64],
        "ui" => [256, 128],
        "icon" => [256, 256],
        "sky" => [2048, 1024],
        _ => [128, 128],
    }
}

fn keyed_kind(kind: &str, layer: Option<(u32, u32)>) -> bool {
    match kind {
        "sprite" | "frames" | "portrait" | "ui" => true,
        "layer" => layer.is_some_and(|(i, _)| i > 0),
        _ => false,
    }
}

fn build_job(kind: &str, game: &Path, name: &str, subject: &str, o: &Opts, style: &Style, extra: &Extra) -> Result<Job, String> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(format!("bad asset name {name:?}"));
    }
    let (screen, _) = super::game_screen(game);
    let scale = style.pixel_scale.max(1);
    let height = match &o.height {
        None => None,
        Some(_) if !matches!(kind, "sprite" | "frames") => return Err("--height is for sprites and frames (figures and props that stand)".into()),
        Some(Height::Units(u)) => Some(*u),
        Some(Height::Of(k, other)) => Some(k * figure_height(game, other)?),
    };
    if o.real.is_some() && !matches!(kind, "sprite" | "frames") {
        return Err("--real is for sprites and frames (figures and props)".into());
    }
    if !o.shows.is_empty() && !matches!(kind, "layer" | "background") {
        return Err("--shows is for layers and backgrounds (what a painting has in it)".into());
    }
    // A prop sized against the figures already sized: its real height at the scene's scale.
    let (height, sized_by) = match height {
        None if matches!(kind, "sprite" | "frames") && !extra.walker && o.size.is_none() && o.grid.is_none() => match scaled_height(game, name, subject, o.real) {
            Some((u, hero)) => (Some(u), Some(hero)),
            None => (None, None),
        },
        h => (h, None),
    };
    let opaque_painting = kind == "background" || (kind == "layer" && !keyed_kind(kind, extra.layer));
    let floor = match o.floor.or(style.kind(kind).floor) {
        Some(f) if !(0.2..=0.98).contains(&f) => return Err(format!("--floor {f}: the floor line's height, 0.2–0.98 of the image from its top")),
        Some(f) if opaque_painting => Some(f),
        Some(_) if o.floor.is_some() && !matches!(kind, "layer") => return Err("--floor is for layers and backgrounds".into()),
        _ => None,
    };
    let mut size = o.size.unwrap_or_else(|| match height {
        Some(u) => size_for_height(u, if style.pixel_art { scale } else { 1 }),
        _ => default_size(kind, style, screen, extra),
    });
    let mut grid = o.grid;
    if grid.is_none() && style.pixel_art && !["background", "icon", "sky"].contains(&kind) {
        grid = Some([(size[0] / scale).max(1), (size[1] / scale).max(1)]);
    }
    if let Some(g) = grid
        && o.size.is_none()
    {
        size = [g[0] * scale, g[1] * scale];
    }
    if kind == "icon" && size != [256, 256] {
        return Err("icon.png must be 256×256".into());
    }
    if kind == "sky" {
        if o.grid.is_some() {
            return Err("skies aren't pixel art: --grid doesn't apply".into());
        }
        if size[0] != 2 * size[1] || size[0] > 2048 || size[0] < 64 {
            return Err(format!("a sky is 2:1 and at most 2048×1024 (mb3d's texture limit), not {}×{}", size[0], size[1]));
        }
    }
    if let Some(g) = grid
        && (size[0] % g[0] != 0 || size[1] % g[1] != 0)
    {
        return Err(format!("--size {}x{} isn't a whole multiple of --grid {}x{}", size[0], size[1], g[0], g[1]));
    }
    if let Some(g) = grid
        && matches!(kind, "sprite" | "frames" | "portrait")
        && g[0].min(g[1]) < 24
    {
        eprintln!(
            "  note: {}×{} art pixels is very coarse; detail will be lost. For a character or detailed object use the default size ({}×{} art pixels), or --grid.",
            g[0], g[1], 128 / scale, 128 / scale
        );
    }
    let portrait_rows = extra.expressions.len() as u32 + 1;
    if kind == "portrait" {
        if size[0] != size[1] {
            return Err(format!("portrait cells are square, not {}×{}", size[0], size[1]));
        }
        if size[1] * portrait_rows > 4096 {
            return Err(format!("{portrait_rows} rows of {}-pixel cells is over mb2d's 4096-pixel limit; use a smaller --size or fewer expressions", size[1]));
        }
    }
    let frames = if kind == "portrait" { 4 * portrait_rows } else { extra.frames.max(1) };
    let cols = match kind {
        "frames" if extra.cols > 0 => extra.cols.min(frames),
        "frames" => frames.min(8),
        "portrait" => 4,
        _ => 0,
    };
    let rows = if kind == "frames" { frames.div_ceil(cols) } else { 1 };
    // The provider sees frames in a near-square grid (very wide images aren't supported);
    // portraits in their own layout, a row per expression.
    let (aw, ah) = match kind {
        "frames" => {
            let (gc, gr) = style::frame_grid(frames);
            (gc * size[0], gr * size[1])
        }
        "portrait" => (4 * size[0], portrait_rows * size[1]),
        _ => (size[0], size[1]),
    };
    let flag = super::maimbrain::take_flags(o.provider.as_deref(), o.own_keys)?;
    let provider = match provider::choose(flag.as_deref(), style.provider.as_deref(), &["gemini", "vertex", "openai"], "images") {
        Ok(p) => p,
        // A dry run shows what the default provider would get, key or not.
        Err(_) if o.dry_run => "gemini".to_string(),
        Err(e) => return Err(e),
    };
    let model = match o.model.clone() {
        Some(m) => m,
        None => match (&style.model, style.provider.as_deref()) {
            (Some(m), Some(p)) if p == provider => m.clone(),
            (Some(m), None) if provider::find_model(&provider, m).is_some() => m.clone(),
            _ => provider::default_model(&provider, Media::Image).map(|m| m.id.to_string()).ok_or(format!("{provider} has no image model"))?,
        },
    };
    // Portraits: about 200 generated pixels a cell at least (1K holds 5 rows of them).
    let big = size[0].max(size[1]).max(match kind {
        "frames" => aw.max(ah) / 2,
        "portrait" if portrait_rows > 5 => 2048,
        _ => 0,
    });
    let resolution = o.res.clone().or(style.resolution.clone()).unwrap_or_else(|| if big > 1100 || kind == "layer" || kind == "sky" { "2K".into() } else { "1K".into() });
    if !["0.5K", "1K", "2K", "4K"].contains(&resolution.as_str()) {
        return Err(format!("--res {resolution}: use 0.5K, 1K, 2K or 4K (uppercase K)"));
    }
    let refs = o.refs.iter().map(|r| ref_path(r, game)).collect();
    Ok(Job {
        kind: kind.into(),
        name: name.into(),
        subject: subject.into(),
        out: o.out.clone().unwrap_or_else(|| match kind {
            "icon" => "icon.png".into(),
            "sky" => format!("assets/{name}.jpg"),
            _ => format!("assets/{name}.png"),
        }),
        size,
        grid,
        grid_explicit: o.grid.is_some(),
        refs,
        seed: o.seed.unwrap_or_else(super::fresh_seed),
        provider,
        model,
        resolution,
        quality: o.quality.clone().unwrap_or_else(|| "medium".into()),
        // No provider makes 2:1; skies are asked for wider and cropped.
        aspect: if kind == "sky" { "21:9".into() } else { nearest_aspect(aw as f32, ah as f32).into() },
        anchor: o.anchor.unwrap_or(if matches!(kind, "frames" | "portrait") { Anchor::Bottom } else { Anchor::Center }),
        margin: o.margin.unwrap_or(if grid.is_some() {
            1
        } else if kind == "ui" {
            2
        } else {
            (size[0].min(size[1]) / 16).max(1)
        }),
        colors: o.colors,
        max_kb: o.max_kb,
        frames: if matches!(kind, "frames" | "portrait") { frames } else { 0 },
        cols,
        expressions: if kind == "portrait" { extra.expressions.clone() } else { Vec::new() },
        layer: extra.layer,
        keyed: keyed_kind(kind, extra.layer) && !o.keep_background,
        key: None,
        tolerance: o.tolerance.unwrap_or(0.09),
        no_style: o.no_style,
        allow_names: o.allow_name.clone(),
        horizon: extra.sky.and_then(|s| s.0),
        span: extra.sky.map_or(180.0, |s| s.1),
        poles: extra.sky.map_or(12.0, |s| s.2),
        height,
        real: o.real,
        sized_by,
        shows: o.shows.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        floor,
        walker: kind == "frames" && extra.walker,
    })
    .map(|mut j: Job| {
        if kind == "frames" && rows > 1 {
            j.cols = cols;
        }
        j
    })
}

/// A reference path as stored: relative to the game when inside it.
fn ref_path(p: &Path, game: &Path) -> String {
    let abs = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (ap, ag) = (abs(p), abs(game));
    match ap.strip_prefix(&ag) {
        Ok(r) => super::rel(r, Path::new("")),
        Err(_) => ap.to_string_lossy().to_string(),
    }
}

fn read_ref(game: &Path, r: &str) -> Result<(RefImage, FileRef), String> {
    let p = if Path::new(r).is_absolute() { PathBuf::from(r) } else { game.join(r) };
    let bytes = std::fs::read(&p).map_err(|e| format!("reference {}: {e}", p.display()))?;
    if bytes.len() > 7 << 20 {
        return Err(format!("reference {} is {} MB; shrink it below 7 MB", p.display(), bytes.len() >> 20));
    }
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err(format!("reference {} isn't a PNG, JPEG or WebP (convert HEIC photos to JPEG first)", p.display()));
    };
    let fr = FileRef { path: r.to_string(), sha256: super::sha256_hex(&bytes) };
    Ok((RefImage { bytes, mime: mime.into() }, fr))
}

fn load_style(game: &Path, no_style: bool) -> Result<(Style, Option<FileRef>), String> {
    if no_style {
        return Ok((Style::default(), None));
    }
    let (s, sha) = Style::load(game)?;
    Ok((s, sha.map(|sha| FileRef { path: "art/style.toml".into(), sha256: sha })))
}

fn generate_kind(kind: &str, game: &Path, name: &str, prompt: &str, o: &Opts, extra: Extra) -> Result<(), String> {
    if !game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", game.display()));
    }
    let (style, _) = load_style(game, o.no_style)?;
    let job = build_job(kind, game, name, prompt, o, &style, &extra)?;
    execute(game, &job, o.variants.max(1), o.dry_run, super::max_cost(o.max_cost)).map(|_| ())
}

fn layers(g: &Gen, count: u32, screens: f32) -> Result<(), String> {
    if !(1..=6).contains(&count) {
        return Err("--count must be 1–6".into());
    }
    let (style, _) = load_style(&g.game, g.opts.no_style)?;
    let mut jobs = Vec::new();
    for i in 0..count {
        let depth = if count == 1 {
            "the only layer, filling the whole image".to_string()
        } else if i == 0 {
            "the farthest layer: the sky and the most distant shapes, filling the whole image, low contrast".to_string()
        } else if i == count - 1 {
            "the nearest layer: foreground shapes along the bottom of the image, darkest and most detailed".to_string()
        } else {
            format!("a middle-distance layer ({} of {count} from the back): shapes in the lower part of the image", i + 1)
        };
        let mut o = g.opts.clone();
        o.out = None;
        if i > 0 {
            // Later layers match the far layer's palette and light.
            let far = g.game.join(format!("assets/{}_0.png", g.name));
            if far.exists() || g.opts.dry_run {
                o.refs.insert(0, far);
            }
        }
        let extra = Extra { layer: Some((i, count)), screens, ..Extra::default() };
        let job = build_job("layer", &g.game, &format!("{}_{i}", g.name), &format!("{} — {depth}", g.prompt), &o, &style, &extra)?;
        jobs.push(job);
    }
    let est = estimate_job(&jobs[0]);
    super::check_cost(est.map(|e| e * count as f64), g.opts.variants.max(1), super::max_cost(g.opts.max_cost), &format!("{count} layers"))?;
    for mut job in jobs {
        // The far layer now exists for the ones after it.
        if job.layer.is_some_and(|(i, _)| i > 0) && !job.refs.iter().any(|r| r.ends_with("_0.png")) {
            let far = format!("assets/{}_0.png", g.name);
            if g.game.join(&far).exists() {
                job.refs.insert(0, far);
            }
        }
        execute(&g.game, &job, g.opts.variants.max(1), g.opts.dry_run, f64::INFINITY)?;
    }
    if g.opts.variants <= 1 && !g.opts.dry_run {
        refresh_meta(&g.game);
    }
    eprintln!("draw layer 0 first and scroll layer i at a speed that grows with i (e.g. 0.2, 0.5, 1.0); each wraps horizontally");
    Ok(())
}

/// A parallax layer fitted to `w`×`h` without losing its ground: scaled to
/// the width, then cropped with its bottom kept (for a keyed layer, the
/// lowest row that is nearly all scenery, so a strip of empty background
/// below it goes), padded with transparent rows on top if it's short. An
/// opaque layer shorter than `h` at that width is covered instead.
fn layer_fit(src: &Image, w: u32, h: u32, keyed: bool) -> (Image, Option<String>) {
    let sh = ((src.h as f64) * (w as f64) / (src.w as f64)).round().max(1.0) as u32;
    if !keyed && sh < h {
        return (src.cover(w, h), None);
    }
    let scaled = src.resize(w, sh);
    let mut bottom = sh;
    if keyed {
        let solid_row = |y: u32| (0..w).filter(|&x| scaled.get(x, y)[3] >= 128).count() as f32 >= w as f32 * 0.9;
        if let Some(y) = (0..sh).rev().find(|&y| solid_row(y)) {
            bottom = y + 1;
        }
    }
    let top = bottom as i64 - h as i64;
    let mut out = Image::new(w, h, [0; 4]);
    let from = top.max(0) as u32;
    let crop = scaled.crop(0, from, w, bottom - from);
    out.blit(&crop, 0, (from as i64 - top) as u32);
    let mut note = None;
    if keyed && top > 0 && (0..w).any(|x| scaled.get(x, from)[3] >= 128) {
        note = Some("the layer's shapes are cut off at the top; ask for a taller --size".to_string());
    }
    if keyed && bottom == sh && !(0..w).all(|x| scaled.get(x, sh - 1)[3] >= 128) {
        note = Some("no row along the bottom is solid scenery; the backdrop repeats its bottom row downward, so ask for shapes that fill the bottom edge".to_string());
    }
    (out, note)
}

/// A painting with any letterbox the generator left inside it cropped off
/// (warned), so its floor lines up with the other paintings'. A full-screen
/// background keeps bars deeper than 15% of the image (cropping them would
/// zoom it a lot), with a warning to regenerate. A flat band on one side
/// only may be a flat floor or sky: warned about at the bottom and sides,
/// and left in.
fn unbox(im: &Image, key: Option<[u8; 3]>, layer: bool, warnings: &mut Vec<String>) -> Image {
    let lb = super::ground::letterbox(im, key);
    let bars: Vec<String> = (0..4).filter(|&i| lb.crop[i] > 0).map(|i| format!("{} {} px", super::ground::EDGES[i], lb.crop[i])).collect();
    let deep = (0..4).any(|i| lb.crop[i] as f32 > 0.15 * if i < 2 { im.h } else { im.w } as f32);
    if lb.any() && deep && !layer {
        warnings.push(format!(
            "the painting fills only part of the generated image (flat letterbox bars: {}); regenerate (--new-seed) for one that fills it",
            bars.join(", ")
        ));
        return im.clone();
    }
    if lb.any() {
        warnings.push(format!(
            "the painting was letterboxed inside the generated image (flat bars: {}); cropped them off, so its floor lines up with the other paintings'",
            bars.join(", ")
        ));
    } else if let Some((side, n)) = lb.lone.filter(|l| l.0 != 0) {
        warnings.push(format!(
            "a flat band of one color runs along the {} edge of the generated image ({n} px): if it's a letterbox bar, regenerate (--new-seed); its floor won't line up with the other paintings'",
            super::ground::EDGES[side]
        ));
    }
    lb.apply(im)
}

/// An opaque painting fitted to `w`×`h` with its floor line `target` (0–1)
/// of the way down, when it has a floor line to put there.
fn place_floor(src: &Image, w: u32, h: u32, target: Option<f32>, warnings: &mut Vec<String>) -> Option<Image> {
    let t = target?;
    match super::ground::find_floor(src) {
        Some(f) => Some(super::ground::fit_floor(src, w, h, f.y, t * h as f32)),
        None => {
            warnings.push("no floor line found to put at --floor (no level line across the painting): fitted as usual".into());
            None
        }
    }
}

/// Fills the holes in a keyed layer's last rows with their median color:
/// the backdrop repeats its bottom row downward, and a hole there would
/// run to the horizon.
fn solid_bottom(im: &mut Image) {
    let k = (im.h / 64).max(2).min(im.h);
    let rows = im.h - k..im.h;
    let mut opaque: Vec<[u8; 3]> = Vec::new();
    for y in rows.clone() {
        for x in 0..im.w {
            let p = im.get(x, y);
            if p[3] >= 128 {
                opaque.push([p[0], p[1], p[2]]);
            }
        }
    }
    if opaque.is_empty() {
        return;
    }
    let mut ground = [0u8; 3];
    for (c, g) in ground.iter_mut().enumerate() {
        let mut v: Vec<u8> = opaque.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *g = v[v.len() / 2];
    }
    for y in rows {
        for x in 0..im.w {
            let p = im.get(x, y);
            let a = p[3] as f32 / 255.0;
            let mix = |c: usize| (p[c] as f32 * a + ground[c] as f32 * (1.0 - a)).round() as u8;
            im.set(x, y, [mix(0), mix(1), mix(2), 255]);
        }
    }
}

fn estimate_job(job: &Job) -> Option<f64> {
    let q = ["low", "medium", "high"].iter().position(|q| *q == job.quality).unwrap_or(1);
    provider::find_model(&job.provider, &job.model).and_then(|m| provider::estimate(m, &job.resolution, q, 0.0))
}

/// Generates (or reprocesses) one job; returns the written asset path.
pub fn execute(game: &Path, job: &Job, variants: u32, dry_run: bool, limit: f64) -> Result<PathBuf, String> {
    let (style, style_ref) = load_style(game, job.no_style)?;
    let palette = style.palette_rgb()?;
    // Originality guard over everything that goes into the prompt.
    let verdict = super::ip::check(&[&job.subject, &style.style, &style.keywords.join(" "), &style.kind(&job.kind).prompt], &job.allow_names);
    if !verdict.ok() {
        return Err(super::ip::refusal_message(&verdict));
    }
    for w in &verdict.warnings {
        eprintln!("  warning: the prompt references a named style ({w}); make sure the result is your own look");
    }
    if let Some(w) = style.unset_warning().filter(|_| !job.no_style) {
        eprintln!("  warning: {w}");
    }
    if let Some(w) = style.leak_warning().filter(|_| !job.no_style && matches!(job.kind.as_str(), "sprite" | "frames" | "portrait" | "ui" | "icon")) {
        eprintln!("  note: {w}");
    }
    let key = if job.keyed { Some(style.key_color()?) } else { None };
    let mut refs = Vec::new();
    let mut ref_files = Vec::new();
    let style_refs: Vec<String> = if job.no_style { Vec::new() } else { style.references.iter().take(3).cloned().collect() };
    for r in style_refs.iter().chain(&job.refs) {
        if dry_run && !(if Path::new(r).is_absolute() { PathBuf::from(r) } else { game.join(r) }).exists() {
            continue;
        }
        let (ri, fr) = read_ref(game, r)?;
        refs.push(ri);
        ref_files.push(fr);
    }
    // A nearer parallax layer gets the far one (`<name>_0.png`, right after the
    // style's references) for its palette and light only: it's a style
    // reference, not a subject to redraw.
    let far_refs = if job.layer.is_some_and(|(i, _)| i > 0) {
        ref_files.iter().skip(style_refs.len().min(ref_files.len())).take_while(|r| r.path.ends_with("_0.png")).count()
    } else {
        0
    };
    let (_, portrait) = super::game_screen(game);
    let full_prompt = style::compose(
        &style,
        &PromptParts {
            // A walker sheet (body + foot) is frames with its own prompt.
            kind: if job.walker { "walker" } else { &job.kind },
            subject: &job.subject,
            frames: job.frames,
            expressions: &job.expressions,
            grid: job.grid.map(|g| (g[0], g[1])),
            // OpenAI makes real transparency (background: "transparent"); others get a flat color to key out.
            keyed: if job.provider == "openai" { None } else { key },
            portrait: job.size[1] >= job.size[0] || (job.kind == "background" && portrait),
            style_refs: (style_refs.len() + far_refs).min(refs.len()),
            subject_refs: refs.len().saturating_sub(style_refs.len() + far_refs),
        },
    );
    let model = provider::find_model(&job.provider, &job.model);
    eprintln!(
        "{} {} → {} ({}×{}{})",
        job.kind,
        job.name,
        job.out,
        job.size[0],
        job.size[1],
        job.grid.map(|g| format!(", {}×{} art pixels", g[0], g[1])).unwrap_or_default()
    );
    eprintln!(
        "  {} {} · {} · aspect {} · seed {}{}",
        job.provider,
        job.model,
        job.resolution,
        job.aspect,
        job.seed,
        if model.is_some_and(|m| !m.seed) { " (not honored)" } else { "" }
    );
    if model.is_none() && job.provider != "mock" {
        eprintln!("  note: {} isn't in mb's catalog (`mb models`); sending it anyway, cost unknown", job.model);
    }
    let est = estimate_job(job);
    super::check_cost(est, variants, limit, "this image")?;
    if dry_run {
        eprintln!("  prompt: {full_prompt}");
        if !ref_files.is_empty() {
            eprintln!("  references: {}", ref_files.iter().map(|r| r.path.as_str()).collect::<Vec<_>>().join(", "));
        }
        eprintln!("  (dry run: nothing generated)");
        return Ok(game.join(&job.out));
    }
    let prov = provider::pick(&job.provider, &job.model, est.map(|e| e * variants as f64))?;
    if prov.via().is_some() && !verdict.allowed.is_empty() {
        return Err(super::maimbrain::allow_name_refused(&verdict.allowed, &job.provider));
    }
    let art = super::ensure_art_dir(game)?;
    let mut outs = Vec::new();
    let mut previews = Vec::new();
    for v in 0..variants {
        let seed = job.seed + v as u64;
        let req = ImageRequest {
            model: job.model.clone(),
            prompt: full_prompt.clone(),
            refs: refs.clone(),
            aspect: job.aspect.clone(),
            resolution: job.resolution.clone(),
            transparent: job.keyed,
            seed: Some(seed),
            quality: ["low", "medium", "high"].iter().position(|q| *q == job.quality).unwrap_or(1),
            hint: MockHint { kind: job.kind.clone(), frames: job.frames, key, palette: palette.clone(), pixel_art: job.grid.is_some() },
        };
        eprint!("  generating with {}{}…", prov.name(), if variants > 1 { format!(" {}/{variants}", v + 1) } else { String::new() });
        let t0 = std::time::Instant::now();
        let out = prov.image(&req)?;
        eprintln!(" {:.1} s", t0.elapsed().as_secs_f32());
        if let Some(n) = prov.note() {
            eprintln!("  {n}");
        }
        let ext = if out.bytes.starts_with(&[0xff, 0xd8]) { "jpg" } else { "png" };
        let raw_rel = format!("art/raw/{}-{seed}.{ext}", job.name);
        std::fs::write(game.join(&raw_rel), &out.bytes).map_err(|e| format!("{raw_rel}: {e}"))?;
        let raw_img = Image::decode(&out.bytes).map_err(|e| format!("{}: {e}", job.provider))?;
        let mut j = job.clone();
        j.seed = seed;
        if j.key.is_none() {
            j.key = key.map(img::hex);
        }
        let (im, mut facts) = process(&j, &style, &palette, &raw_img)?;
        scene_facts(game, &j, out.text.as_deref(), &mut facts);
        let png = encode_budget(&im, &j, &mut facts)?;
        let dest = if variants == 1 { game.join(&j.out) } else { art.join("variants").join(format!("{}-{}.png", j.name, v + 1)) };
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, &png).map_err(|e| format!("{}: {e}", dest.display()))?;
        let sc = Sidecar {
            generator: format!("mb {} art {}", env!("CARGO_PKG_VERSION"), j.kind),
            date: super::now_utc(),
            provider: j.provider.clone(),
            model: j.model.clone(),
            via: prov.via().map(str::to_string),
            prompt: j.subject.clone(),
            full_prompt: full_prompt.clone(),
            seed,
            seed_honored: model.is_none_or(|m| m.seed),
            references: ref_files.clone(),
            style: style_ref.clone(),
            est_cost_usd: est,
            cost_usd: out.cost_usd,
            raw: raw_rel,
            raw_sha256: super::sha256_hex(&out.bytes),
            output_sha256: super::sha256_hex(&png),
            allowed_names: verdict.allowed.clone(),
            model_text: out.text.clone(),
            recipe: serde_json::to_value(&j).unwrap(),
            result: facts.clone(),
            note: super::note_for(&j.provider),
            derived: Vec::new(),
        };
        super::write_sidecar(&dest, &sc)?;
        let preview = im.preview();
        std::fs::create_dir_all(art.join("preview")).map_err(|e| e.to_string())?;
        let pv = if variants == 1 {
            art.join("preview").join(format!("{}.png", j.name))
        } else {
            art.join("variants").join(format!("{}-{}.preview.png", j.name, v + 1))
        };
        std::fs::write(&pv, preview.encode_png()).map_err(|e| e.to_string())?;
        super::walkcheck::write_onion(&pv, &im, j.frames, j.cols, j.size, &facts)?;
        report(&dest, &im, png.len(), &facts, out.cost_usd);
        outs.push(dest);
        previews.push(im);
    }
    if variants > 1 {
        let sheet = art.join("variants").join(format!("{}-sheet.png", job.name));
        std::fs::write(&sheet, img::contact_sheet(&previews).encode_png()).map_err(|e| e.to_string())?;
        eprintln!("  compare: {} (variants 1–{variants}, left to right, top to bottom)", sheet.display());
        eprintln!("  then: mb art pick {} {} <n>", game.display(), job.name);
        println!("{}", sheet.display());
    } else {
        eprintln!("  look at it: {}", art.join("preview").join(format!("{}.png", job.name)).display());
        println!("{}", outs[0].display());
        // `mb art layers` writes it once, after its last layer.
        if job.layer.is_none() {
            refresh_meta(game);
        }
    }
    Ok(outs.remove(0))
}

fn report(dest: &Path, im: &Image, bytes: usize, facts: &serde_json::Value, cost: Option<f64>) {
    let colors = im.distinct_colors(256).len();
    eprintln!(
        "  wrote {} ({}×{}, {} KB, {} colors{}){}",
        dest.display(),
        im.w,
        im.h,
        bytes.div_ceil(1024),
        if colors > 256 { "256+".to_string() } else { colors.to_string() },
        if im.is_opaque() { ", opaque" } else { ", transparent" },
        cost.filter(|c| *c > 0.0).map(|c| format!(" · actual cost ${c:.4}")).unwrap_or_default()
    );
    if let Some(w) = facts["warnings"].as_array() {
        for w in w {
            eprintln!("  warning: {}", w.as_str().unwrap_or(""));
        }
    }
    if let Some(r) = facts["rust"].as_str() {
        eprintln!("  {r}");
    }
    if let Some(r) = facts["stand"].as_str() {
        eprintln!("  {r}");
    }
}

/// What a new image adds to the scene: a figure's real height (`result.real`), a painting's
/// objects (`result.shows`), and a warning when a prop is of something a painting already shows,
/// or a painting shows something there's a prop of, or a figure's size disagrees with the scene's.
fn scene_facts(game: &Path, job: &Job, model_text: Option<&str>, facts: &mut serde_json::Value) {
    use super::ground::Meta;
    let metas: Vec<Meta> = super::ground::game_meta(game).into_iter().filter(|m| m.name() != job.name).collect();
    let mut warnings: Vec<String> = Vec::new();
    match job.kind.as_str() {
        "sprite" | "frames" => {
            let real = super::scene::real_json(job.real, &job.subject);
            for m in &metas {
                if let Meta::Backdrop { path, shows, .. } = m {
                    for o in shows.iter().filter(|o| super::scene::same_thing(o, &job.subject)) {
                        warnings.push(dup_warning(path, o, &job.out));
                    }
                }
            }
            if let (Some(h), Some(r), Some((k, hero))) = (job.height, real["m"].as_f64(), super::ground::scene_scale(&metas))
                && let Some(want) = super::scene::implausible(h, r as f32, k)
            {
                warnings.push(format!(
                    "{h:.0} units tall for {r} m in the world, where {hero} is {k:.1} units a meter: about {want:.0} units would match (--height {want:.0}, or --real if it isn't {r} m)"
                ));
            }
            if !real.is_null() {
                facts["real"] = real;
            }
        }
        "layer" | "background" => {
            // A layer's subject ends with its place in the stack ("— the nearest layer: …").
            let subject = job.subject.split(" — ").next().unwrap_or(&job.subject);
            let shows = super::scene::shows_json(subject, model_text, &job.shows);
            for m in &metas {
                if let Meta::Figure { path, subject, .. } = m {
                    for o in shows.iter().filter(|o| super::scene::same_thing(o, subject)) {
                        warnings.push(dup_warning(&job.out, o, path));
                    }
                }
            }
            facts["shows"] = json!(shows);
        }
        _ => {}
    }
    if !warnings.is_empty() {
        let mut all: Vec<serde_json::Value> = facts["warnings"].as_array().cloned().unwrap_or_default();
        all.extend(warnings.into_iter().map(serde_json::Value::from));
        facts["warnings"] = json!(all);
    }
}

fn dup_warning(painting: &str, object: &str, prop: &str) -> String {
    format!(
        "{painting} already shows a {object} (painted in), and {prop} is one too: a prop beside its painted twin reads as a mistake. Regenerate the painting without it (\"no {object}\" in its prompt) or make the prop something else"
    )
}

/// Rewrites src/art_meta.rs after a generation; a failure there doesn't fail the image.
fn refresh_meta(game: &Path) {
    if let Err(e) = super::ground::write_meta(game) {
        eprintln!("  warning: src/art_meta.rs: {e}");
    }
}

/// Encodes, quantizing until the PNG fits job.max_kb (if set); warns on big
/// files. Skies are JPEG, at the best quality that fits (800 KB by default).
fn encode_budget(im: &Image, job: &Job, facts: &mut serde_json::Value) -> Result<Vec<u8>, String> {
    if job.kind == "sky" {
        let (jpg, q) = super::sky::encode_budget(im, job.max_kb.unwrap_or(800))?;
        facts["jpeg_quality"] = json!(q);
        return Ok(jpg);
    }
    let mut png = im.encode_png();
    if let Some(kb) = job.max_kb {
        let limit = kb as usize * 1024;
        let mut n = 256;
        while png.len() > limit && n >= 8 {
            let mut q = im.clone();
            img::quantize(&mut q, n);
            png = q.encode_png();
            n /= 2;
        }
        if png.len() > limit {
            eprintln!("  warning: still {} KB after quantizing to 8 colors (budget {kb} KB); use a smaller --size", png.len() / 1024);
        }
    } else {
        let warn_kb = match job.kind.as_str() {
            "tile" | "ui" | "icon" => 200,
            "sprite" => 400,
            _ => 1200,
        };
        if png.len() > warn_kb * 1024 {
            eprintln!(
                "  warning: {} KB is a lot for one {} (bundle ≤ 10 MB, startup set ≤ 3 MB); try --colors 128 or --max-kb {}",
                png.len() / 1024,
                job.kind,
                warn_kb / 2
            );
        }
    }
    Ok(png)
}

/// Post-processing by kind: keying, trimming, fitting, pixel grids, frames,
/// seamless wrapping, palettes. Returns the image and facts for the sidecar.
pub fn process(job: &Job, style: &Style, palette: &[[u8; 3]], raw: &Image) -> Result<(Image, serde_json::Value), String> {
    let mut warnings: Vec<String> = Vec::new();
    let mut facts = json!({});
    let keyed_bg = std::cell::Cell::new(None);
    let keyed_extra = std::cell::Cell::new(None);
    let cut_off: std::cell::RefCell<Vec<&str>> = std::cell::RefCell::new(Vec::new());
    // A figure or prop running off the generated image's edge was cut off there (a witch without
    // her hem can't stand on a floor); `ground::cut_edges` says where.
    let check_cut = |k: &Image, bgs: &[[u8; 3]], warnings: &mut Vec<String>| -> bool {
        if !matches!(job.kind.as_str(), "sprite" | "frames") {
            return false;
        }
        let cut = super::ground::cut_edges(super::ground::edge_cover(k, bgs, job.tolerance as f32), [k.w, k.h]);
        if cut.is_empty() {
            return false;
        }
        let what = if job.kind == "frames" { "a frame" } else { "the subject" };
        warnings.push(if cut.contains(&"bottom") {
            format!(
                "{what} runs off the bottom of the generated image ({}): its feet or hem are missing, so it can't stand on a floor. Regenerate (mb regen {} --new-seed)",
                cut.join(", "),
                job.out
            )
        } else {
            format!("{what} runs off the generated image's {} edge: part of it is missing. Regenerate (mb regen {} --new-seed)", cut.join(" and "), job.out)
        });
        *cut_off.borrow_mut() = cut;
        true
    };
    let [w, h] = job.size;
    let flat_bg = palette.first().copied().unwrap_or([0, 0, 0]);
    // Transparency: keep a provider's real alpha, else key out the flat background.
    let keyed = |raw: &Image, warnings: &mut Vec<String>| -> Image {
        if raw.has_transparent_border() {
            check_cut(raw, &[], warnings);
            return raw.clone();
        }
        let expected = job.key.as_deref().and_then(|k| img::parse_hex(k).ok()).or_else(|| style.key_color().ok());
        let hint = img::KeyHint { expected, top: job.kind == "layer" };
        let (k, rep) = img::key_out_with(raw, job.tolerance as f32, hint);
        keyed_bg.set(Some(rep.background));
        if rep.backgrounds.len() > 1 {
            keyed_extra.set(rep.backgrounds.get(1).copied());
        }
        // An uneven background is only a problem if some of it was left: background-colored
        // pixels still opaque along the edges of the image.
        let left = background_left(&k, &rep.backgrounds, job.tolerance as f32);
        if left > 0.02 {
            warnings.push(format!(
                "the background wasn't one flat color (variation {:.2}): {:.0}% of the image's edge is background left behind; raise --tolerance (mb regen <asset> --reprocess --tolerance {:.2}) or regenerate",
                rep.noise,
                left * 100.0,
                (rep.noise * 1.3).max(job.tolerance as f32 + 0.02)
            ));
        }
        if !check_cut(&k, &rep.backgrounds, warnings) && job.kind != "layer" && rep.border_bg < 0.6 {
            warnings.push(format!(
                "the subject touches the image edges ({:.0}% of the border isn't background); it may be cut off. Regenerate",
                100.0 * (1.0 - rep.border_bg)
            ));
        }
        if rep.cleared < 0.05 {
            warnings.push("almost nothing was keyed out: the subject may fill the frame or the background isn't flat".into());
        }
        if rep.cleared > 0.985 {
            warnings.push("almost everything was keyed out: the subject may be the background's color; set a different `key` in art/style.toml".into());
        }
        k
    };
    let mut out = match job.kind.as_str() {
        "sprite" | "ui" => {
            let src = if job.keyed { keyed(raw, &mut warnings).trimmed() } else { raw.clone() };
            match job.grid {
                Some(g) => fit_pixel(&src, g, job.margin, job.anchor).scale_nearest(w / g[0]),
                None if job.keyed => src.contain(w, h, job.margin, job.anchor),
                None => src.cover(w, h),
            }
        }
        "frames" => {
            let k = if job.keyed { keyed(raw, &mut warnings) } else { raw.clone() };
            let n = job.frames.max(1);
            let (gc, gr) = style::frame_grid(n);
            let parts = split_frames(&k, n, gc, gr, &mut warnings);
            let cols = job.cols.max(1);
            let rows = n.div_ceil(cols);
            let mut sheet = Image::new(cols * w, rows * h, [0; 4]);
            // One scale for every frame, so the character doesn't change size.
            let maxw = parts.iter().map(|p| p.w).max().unwrap_or(1) as f32;
            let maxh = parts.iter().map(|p| p.h).max().unwrap_or(1) as f32;
            for (i, p) in parts.iter().enumerate() {
                let cell = match job.grid {
                    Some(g) => {
                        let (cw, ch) = (g[0] * 8, g[1] * 8);
                        let m = job.margin * 8;
                        let k = ((cw.saturating_sub(2 * m)) as f32 / maxw).min((ch.saturating_sub(2 * m)) as f32 / maxh);
                        let placed = place(p, k, cw, ch, m, job.anchor);
                        let mut c = img::pixelate(&placed, g[0], g[1]);
                        if style.palette_lock {
                            img::lock_palette(&mut c, palette);
                        }
                        c.scale_nearest(w / g[0])
                    }
                    None => {
                        let k = ((w.saturating_sub(2 * job.margin)) as f32 / maxw).min((h.saturating_sub(2 * job.margin)) as f32 / maxh);
                        place(p, k, w, h, job.margin, job.anchor)
                    }
                };
                sheet.blit(&cell, (i as u32 % cols) * w, (i as u32 / cols) * h);
            }
            facts["frames"] = json!(n);
            facts["cell"] = json!([w, h]);
            facts["cols"] = json!(cols);
            facts["rust"] = json!(format!("frame i's source rect: [(i % {cols}) × {w}, (i / {cols}) × {h}, {w}, {h}] (e.g. {} frames at 8–12 fps)", n));
            // Walking: a walker sheet's rects, or a walk cycle's feet checked (super::walkcheck).
            if job.walker {
                match super::walkcheck::walker_facts(&sheet, cols, w, h, &job.out) {
                    Some((f, rust, more)) => {
                        facts["walker"] = f;
                        facts["rust"] = json!(rust);
                        warnings.extend(more);
                    }
                    None => warnings.push("a walker sheet is a body (cell 0) and a foot (cell 1), and one of them is empty: regenerate".into()),
                }
            } else if let Some(g) = super::walkcheck::gait_word(&job.name, &job.subject) {
                facts["rust"] = json!(format!(
                    "frame i's source rect: [(i % {cols}) × {w}, (i / {cols}) × {h}, {w}, {h}]; a {g} cycle: pick the frame by distance walked, never a timer, or the feet slide: `maimbrain::walk::Walker`'s `frame({n})`, with `stride` the distance one cycle covers at the size you draw it"
                ));
                if let Some(r) = super::walkcheck::measure(&sheet, n, cols, w, h) {
                    facts["walk"] = r.facts();
                    warnings.extend(r.warnings(g, &job.name));
                }
            }
            sheet
        }
        "portrait" => {
            let k = if job.keyed { keyed(raw, &mut warnings) } else { raw.clone() };
            let rows = (job.frames / 4).max(1);
            let parts = split_frames(&k, 4 * rows, 4, rows, &mut warnings);
            // Aligned in a working cell (pixel art: 8 texels per art pixel), then reduced.
            let (cw, ch, m) = match job.grid {
                Some(g) => (g[0] * 8, g[1] * 8, job.margin * 8),
                None => (w, h, job.margin),
            };
            let cells = align_portraits(&parts, cw, ch, m, &mut warnings);
            let mut sheet = Image::new(4 * w, rows * h, [0; 4]);
            for (i, c) in cells.iter().enumerate() {
                let cell = match job.grid {
                    Some(g) => {
                        let mut c = img::pixelate(c, g[0], g[1]);
                        if style.palette_lock {
                            img::lock_palette(&mut c, palette);
                        }
                        c.scale_nearest(w / g[0])
                    }
                    None => c.clone(),
                };
                sheet.blit(&cell, (i as u32 % 4) * w, (i as u32 / 4) * h);
            }
            let mut names = job.expressions.clone();
            names.resize(rows as usize - 1, String::new());
            facts["portrait"] = json!({
                "cols": 4,
                "rows": rows,
                "cell": [w, h],
                "expressions": names,
                "blink_row": rows - 1,
                "mouths": style::MOUTHS,
            });
            facts["cell"] = json!([w, h]);
            facts["cols"] = json!(4);
            let rows_list: Vec<String> = names.iter().enumerate().map(|(i, e)| format!("{i} {e}")).chain([format!("{} blink", rows - 1)]).collect();
            // Pixel art: say how many art pixels a cell is, so faces scale by whole device pixels per art pixel.
            let art = job.grid.map_or(String::new(), |g| format!("; pixel art, {} art pixels across: Portrait::load(path, {w}, {h}).art_pixels({})", g[0], g[0]));
            facts["rust"] = json!(format!(
                "talking portrait for ui::Portrait (docs/UI.md): cell (col, row) at [col × {w}, row × {h}, {w}, {h}]; col = mouth 0 closed, 1 slightly open, 2 open, 3 wide open; row = {}{art}",
                rows_list.join(", ")
            ));
            sheet
        }
        "background" | "icon" => {
            let base = if raw.is_opaque() { raw.clone() } else { raw.flatten(flat_bg) };
            let painting = job.kind == "background";
            let base = if painting { unbox(&base, None, false, &mut warnings) } else { base };
            let mut fit = |tw: u32, th: u32| place_floor(&base, tw, th, job.floor, &mut warnings).unwrap_or_else(|| base.cover(tw, th));
            let bg = match job.grid {
                Some(g) => img::pixelate(&fit(g[0] * 8, g[1] * 8), g[0], g[1]).scale_nearest(w / g[0]),
                None => fit(w, h),
            };
            if painting {
                facts["floor"] = super::ground::find_floor(&bg).map_or(serde_json::Value::Null, |f| f.to_json(bg.h, job.floor.is_some()));
            }
            bg
        }
        "layer" => {
            // A letterbox the generator left inside the painting goes first, so its floor lines up.
            let key = if job.keyed { job.key.as_deref().and_then(|k| img::parse_hex(k).ok()).or_else(|| style.key_color().ok()) } else { None };
            let unboxed = unbox(raw, key, true, &mut warnings);
            let raw = &unboxed;
            let src = if job.keyed {
                keyed(raw, &mut warnings)
            } else if raw.is_opaque() {
                raw.clone()
            } else {
                raw.flatten(flat_bg)
            };
            let band = 0.08;
            let wide = ((w as f32) * (1.0 + band)).round() as u32;
            let placed = if job.keyed { None } else { place_floor(&src, wide, h, job.floor, &mut warnings) };
            let (fit, note) = match placed {
                Some(p) => (p, None),
                None => layer_fit(&src, wide, h, job.keyed),
            };
            if let Some(n) = note {
                warnings.push(n);
            }
            let mut s = img::seamless(&fit, true, false, band / (1.0 + band));
            if s.w != w {
                s = s.resize(w, h);
            }
            if job.keyed {
                solid_bottom(&mut s);
            }
            facts["wraps"] = json!("x");
            if let Some((i, n)) = job.layer {
                facts["layer"] = json!({
                    "index": i,
                    "of": n,
                    // The farthest layer is opaque by design (it's the sky and the farthest shapes);
                    // the nearer ones are keyed, with the far one attached only for its palette and light.
                    "transparent": job.keyed,
                    "crop": if job.keyed { "bottom at the lowest solid row" } else { "anchored at the bottom" },
                });
            }
            let layer = match job.grid {
                Some(g) => img::pixelate(&s.resize(g[0] * 8, g[1] * 8), g[0], g[1]).scale_nearest(w / g[0]),
                None => s,
            };
            // Where things stand on it: an opaque painting's floor line, a transparent layer's ground top.
            let floor = if job.keyed { super::ground::solid_top(&layer) } else { super::ground::find_floor(&layer) };
            facts["floor"] = floor.map_or(serde_json::Value::Null, |f| f.to_json(layer.h, job.floor.is_some()));
            layer
        }
        "tile" => {
            let band = 0.125;
            let work = if let Some(g) = job.grid { (g[0] * 8, g[1] * 8) } else { (w, h) };
            let src = if raw.is_opaque() { raw.clone() } else { raw.flatten(flat_bg) };
            let (ww, wh) = (((work.0 as f32) * (1.0 + band)).round() as u32, ((work.1 as f32) * (1.0 + band)).round() as u32);
            let s = img::seamless(&src.cover(ww, wh), true, true, band / (1.0 + band));
            let s = if (s.w, s.h) != work { s.resize(work.0, work.1) } else { s };
            facts["wraps"] = json!("xy");
            match job.grid {
                Some(g) => img::pixelate(&s, g[0], g[1]).scale_nearest(w / g[0]),
                None => s,
            }
        }
        "sky" => {
            let src = if raw.is_opaque() { raw.clone() } else { raw.flatten(flat_bg) };
            let found = if job.horizon.is_none() { super::sky::find_horizon(&src) } else { None };
            let horizon = job.horizon.or(found.map(|f| f.at)).unwrap_or(0.5);
            match (job.horizon, found) {
                (Some(_), _) => {}
                (None, None) => warnings.push("no horizon found (an all-sky painting?); placed the image's middle on the horizon. Look at art/preview and pass --horizon 0–1 if it's off".into()),
                (None, Some(f)) if f.confidence < super::sky::SURE => warnings.push(format!(
                    "the horizon is a guess (found at {:.2} with confidence {:.2}); look at art/preview and pass --horizon 0–1 if it's off",
                    f.at, f.confidence
                )),
                _ => {}
            }
            let (sky, rep) = super::sky::equirect(&src, w, h, horizon, job.span, job.poles);
            let raw_aspect = raw.w as f32 / raw.h as f32;
            if !(1.6..=2.6).contains(&raw_aspect) {
                warnings.push(format!(
                    "the provider returned {}×{} ({raw_aspect:.2}:1), not a wide panorama; {:.0}% of it was cropped to make 2:1",
                    raw.w,
                    raw.h,
                    rep.cropped * 100.0
                ));
            }
            if rep.extended != [0, 0] {
                warnings.push(format!(
                    "--span {} covers {} of {h} rows; the {} above and {} below are its edge colors stretched toward the poles, not painted",
                    job.span,
                    h - rep.extended[0] - rep.extended[1],
                    rep.extended[0],
                    rep.extended[1]
                ));
            }
            facts["projection"] = json!("equirectangular");
            facts["wraps"] = json!("x");
            facts["horizon_row"] = json!(h / 2);
            facts["source_horizon"] = json!(((horizon * 1000.0).round() / 1000.0, if job.horizon.is_some() { "given" } else { "found" }));
            if let Some(f) = found {
                facts["horizon_confidence"] = json!((f.confidence * 100.0).round() / 100.0);
            }
            facts["span_deg"] = json!(job.span);
            facts["calm_poles_deg"] = json!(job.poles);
            facts["cropped"] = json!((rep.cropped * 1000.0).round() / 1000.0);
            facts["extended_rows"] = json!(rep.extended);
            facts["rust"] = json!(format!("gfx3d::sky_texture(Texture::new(Asset::load(\"{}\")), &SkyTexture::default()), once the asset is ready", job.out));
            sky
        }
        k => return Err(format!("unknown kind {k}")),
    };
    if !matches!(job.kind.as_str(), "frames" | "portrait") && (out.w, out.h) != (w, h) {
        out = out.resize(w, h);
    }
    if style.palette_lock && !palette.is_empty() {
        img::lock_palette(&mut out, palette);
    }
    let colors = job.colors.unwrap_or(style.colors);
    if colors > 0 {
        img::quantize(&mut out, colors as usize);
    }
    if job.kind == "icon" && !out.is_opaque() {
        out = out.flatten(flat_bg);
    }
    if let Some(g) = job.grid {
        facts["pixel_scale"] = json!(w / g[0]);
    }
    // Where it stands: its feet (the lowest opaque row) and how tall it's drawn (src/art_meta.rs).
    // (Not a --walker sheet: its body has no feet; `walk::WalkSprite` stands it on the floor.)
    if matches!(job.kind.as_str(), "sprite" | "frames") && !job.walker {
        let (cell, cells) = if job.kind == "frames" { ([w, h], job.frames.max(1)) } else { ([out.w, out.h], 1) };
        if let Some(ft) = super::ground::footing(&out, cell, cells) {
            facts["ground"] = ft.to_json(job.height.unwrap_or(ft.height() / 2.0));
            facts["stand"] = json!(format!(
                "stand it on a floor by its feet (row {} of {}): ground::Figure::new(image, art_meta::{}){}.draw(x, floor_y)",
                ft.foot[1],
                cell[1],
                super::ground::const_name(&job.name),
                if job.kind == "frames" { ".frame(i)" } else { "" }
            ));
        }
    }
    let cut = cut_off.borrow().clone();
    if !cut.is_empty() {
        facts["cut_off"] = json!(cut);
    }
    if let Some(bg) = keyed_bg.get() {
        facts["keyed_background"] = json!(img::hex(bg));
        if let Some(second) = keyed_extra.get() {
            facts["keyed_background_2"] = json!(img::hex(second));
        }
    }
    facts["size"] = json!([out.w, out.h]);
    facts["warnings"] = json!(warnings);
    Ok((out, facts))
}

/// Scales trimmed content by k and places it in a cw×ch cell.
fn place(p: &Image, k: f32, cw: u32, ch: u32, margin: u32, anchor: Anchor) -> Image {
    let nw = ((p.w as f32 * k).round() as u32).clamp(1, cw);
    let nh = ((p.h as f32 * k).round() as u32).clamp(1, ch);
    let s = p.resize(nw, nh);
    let mut c = Image::new(cw, ch, [0; 4]);
    let x = (cw - nw) / 2;
    let y = match anchor {
        Anchor::Center => (ch - nh) / 2,
        Anchor::Bottom => ch - margin.min(ch - nh) - nh,
    };
    c.blit(&s, x, y);
    c
}

/// Fits content into a pixel grid: scaled into a working canvas 8 texels per
/// art pixel, then reduced to the grid.
fn fit_pixel(src: &Image, g: [u32; 2], margin: u32, anchor: Anchor) -> Image {
    let work = src.contain(g[0] * 8, g[1] * 8, margin * 8, anchor);
    img::pixelate(&work, g[0], g[1])
}

/// Places a talking portrait's busts in cw×ch cells so the head holds still from cell to cell:
/// each is scaled to the first's width (a bust's shoulders are its steadiest measure; within
/// ±15%), all by one factor that fits the widest and tallest, stood on the cell's bottom (the
/// shoulders' cut) with its centre of mass over the first's, then nudged a few pixels to where
/// its silhouette (the head counting double) best covers the first's.
fn align_portraits(parts: &[Image], cw: u32, ch: u32, margin: u32, warnings: &mut Vec<String>) -> Vec<Image> {
    let Some(first) = parts.first() else { return Vec::new() };
    let mut odd = Vec::new();
    let norm: Vec<f32> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let s = first.w as f32 / p.w.max(1) as f32;
            if !(0.85..=1.15).contains(&s) {
                odd.push(format!("row {} col {}", i / 4, i % 4));
            }
            s.clamp(0.85, 1.15)
        })
        .collect();
    if !odd.is_empty() {
        warnings.push(format!("{} drawn at a different size from the first cell; check the preview, regenerate if a face jumps", odd.join(", ")));
    }
    let maxw = parts.iter().zip(&norm).map(|(p, s)| p.w as f32 * s).fold(1.0, f32::max);
    let maxh = parts.iter().zip(&norm).map(|(p, s)| p.h as f32 * s).fold(1.0, f32::max);
    let k = (cw.saturating_sub(2 * margin).max(1) as f32 / maxw).min(ch.saturating_sub(margin).max(1) as f32 / maxh);
    let scaled: Vec<Image> = parts
        .iter()
        .zip(&norm)
        .map(|(p, s)| p.resize(((p.w as f32 * k * s).round() as u32).clamp(1, cw), ((p.h as f32 * k * s).round() as u32).clamp(1, ch)))
        .collect();
    let target = (cw - scaled[0].w) as f32 / 2.0 + centroid_x(&scaled[0]);
    let mut cells: Vec<Image> = scaled
        .iter()
        .map(|s| {
            let x = (target - centroid_x(s)).round().clamp(0.0, (cw - s.w) as f32) as u32;
            let mut c = Image::new(cw, ch, [0; 4]);
            c.blit(s, x, ch - s.h);
            c
        })
        .collect();
    let f = (cw.max(ch) / 64).max(1);
    let reference = coverage(&cells[0], f);
    for c in cells.iter_mut().skip(1) {
        let (dx, dy) = best_shift(&reference, &coverage(c, f), 3);
        if (dx, dy) != (0, 0) {
            *c = shifted(c, dx * f as i32, dy * f as i32);
        }
    }
    cells
}

/// The alpha-weighted mean x of an image's pixels.
fn centroid_x(im: &Image) -> f32 {
    let (mut sum, mut wsum) = (0.0f64, 0.0f64);
    for y in 0..im.h {
        for x in 0..im.w {
            let a = im.get(x, y)[3] as f64;
            sum += a * (x as f64 + 0.5);
            wsum += a;
        }
    }
    if wsum > 0.0 { (sum / wsum) as f32 } else { im.w as f32 / 2.0 }
}

/// Coverage on a coarse grid (f×f blocks): (width, height, mean alpha 0–1 per block).
fn coverage(im: &Image, f: u32) -> (usize, usize, Vec<f32>) {
    let (w, h) = ((im.w / f) as usize, (im.h / f) as usize);
    let mut out = vec![0.0; w * h];
    for (i, v) in out.iter_mut().enumerate() {
        let (bx, by) = ((i % w) as u32 * f, (i / w) as u32 * f);
        let mut s = 0u32;
        for y in by..by + f {
            for x in bx..bx + f {
                s += im.get(x, y)[3] as u32;
            }
        }
        *v = s as f32 / (255 * f * f) as f32;
    }
    (w, h, out)
}

/// The shift (within ±r blocks) that best lays `cur` over `reference`; the top 60% (the head)
/// counts double. Ties go to the smaller shift.
fn best_shift(reference: &(usize, usize, Vec<f32>), cur: &(usize, usize, Vec<f32>), r: i32) -> (i32, i32) {
    let (w, h, a) = reference;
    let b = &cur.2;
    let (w, h) = (*w as i32, *h as i32);
    let cost = |dx: i32, dy: i32| -> f32 {
        let mut c = 0.0;
        for y in 0..h {
            let weight = if (y as f32) < h as f32 * 0.6 { 2.0 } else { 1.0 };
            for x in 0..w {
                let (sx, sy) = (x - dx, y - dy);
                let bv = if sx >= 0 && sy >= 0 && sx < w && sy < h { b[(sy * w + sx) as usize] } else { 0.0 };
                c += weight * (a[(y * w + x) as usize] - bv).abs();
            }
        }
        c
    };
    let mut shifts: Vec<(i32, i32)> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).collect();
    shifts.sort_by_key(|(dx, dy)| dx.abs() + dy.abs());
    let mut best = ((0, 0), f32::MAX);
    for (dx, dy) in shifts {
        let c = cost(dx, dy);
        if c < best.1 - 1e-3 {
            best = ((dx, dy), c);
        }
    }
    best.0
}

/// The image moved by (dx, dy), clipped to its bounds.
fn shifted(im: &Image, dx: i32, dy: i32) -> Image {
    let mut out = Image::new(im.w, im.h, [0; 4]);
    for y in 0..im.h as i32 {
        for x in 0..im.w as i32 {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && nx < im.w as i32 && ny < im.h as i32 {
                out.set(nx as u32, ny as u32, im.get(x as u32, y as u32));
            }
        }
    }
    out
}

/// The share of the image's edge pixels still opaque and close to one of the
/// keyed background colors (within twice the tolerance): background the key
/// left behind.
fn background_left(k: &Image, bgs: &[[u8; 3]], tolerance: f32) -> f32 {
    let labs: Vec<[f32; 3]> = bgs.iter().map(|c| img::oklab(*c)).collect();
    let (w, h) = (k.w, k.h);
    let edge: Vec<(u32, u32)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((1..h.saturating_sub(1)).flat_map(|y| [(0, y), (w - 1, y)])).collect();
    let left = edge
        .iter()
        .filter(|&&(x, y)| {
            let p = k.get(x, y);
            p[3] > 128 && labs.iter().any(|b| img::oklab_dist(img::oklab([p[0], p[1], p[2]]), *b) < tolerance * 2.0)
        })
        .count();
    left as f32 / edge.len().max(1) as f32
}

/// How much an even `gc`×`gr` grid cuts through drawings: the share of
/// points on its inner column lines, in the top 55% of each row (where the
/// heads are), with opaque pixels a few pixels to both sides. Shoulders and
/// bodies meeting at the bottom of the cells, or a bust touching the row
/// below, are how such sheets are drawn and don't count.
fn grid_straddle(k: &Image, gc: u32, gr: u32) -> f32 {
    let (cw, ch) = (k.w / gc.max(1), k.h / gr.max(1));
    let d = (k.w.min(k.h) / 200).max(3);
    let solid = |x: u32, y: u32| x < k.w && y < k.h && k.get(x, y)[3] >= 128;
    let (mut on, mut all) = (0usize, 0usize);
    for i in 1..gc {
        let x = i * cw;
        for j in 0..gr {
            for y in j * ch..j * ch + ch * 55 / 100 {
                all += 1;
                on += (x >= d && solid(x - d, y) && solid(x + d, y)) as usize;
            }
        }
    }
    on as f32 / all.max(1) as f32
}

/// Splits a keyed sprite sheet into its frames: connected shapes (with small
/// bits joined to the nearest big one), in reading order; falls back to an
/// even grid when the count doesn't match.
fn split_frames(k: &Image, n: u32, gc: u32, gr: u32, warnings: &mut Vec<String>) -> Vec<Image> {
    let (w, h) = (k.w as usize, k.h as usize);
    let mut label = vec![u32::MAX; w * h];
    let mut comps: Vec<(usize, [usize; 4])> = Vec::new(); // area, bbox x0 y0 x1 y1
    let mut stack = Vec::new();
    for s in 0..w * h {
        if k.px[s][3] < 24 || label[s] != u32::MAX {
            continue;
        }
        let id = comps.len() as u32;
        let mut area = 0;
        let mut bb = [usize::MAX, usize::MAX, 0, 0];
        stack.push(s);
        label[s] = id;
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if k.px[j][3] >= 24 && label[j] == u32::MAX {
                    label[j] = id;
                    stack.push(j);
                }
            }
        }
        comps.push((area, bb));
    }
    let largest = comps.iter().map(|c| c.0).max().unwrap_or(0);
    let mut big: Vec<usize> = (0..comps.len()).filter(|&i| comps[i].0 * 4 >= largest).collect();
    if big.len() == n as usize {
        // Owner of each small component: the big one with the nearest bbox centre.
        let center = |b: &[usize; 4]| ((b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0);
        let mut owner: Vec<usize> = (0..comps.len()).collect();
        for (i, c) in comps.iter().enumerate() {
            if !big.contains(&i) {
                let (cx, cy) = center(&c.1);
                owner[i] = *big
                    .iter()
                    .min_by(|a, b| {
                        let (ax, ay) = center(&comps[**a].1);
                        let (bx, by) = center(&comps[**b].1);
                        ((ax - cx).powi(2) + (ay - cy).powi(2)).partial_cmp(&((bx - cx).powi(2) + (by - cy).powi(2))).unwrap()
                    })
                    .unwrap();
            }
        }
        // Reading order: rows by centre y (row height = sheet height / rows), then x.
        let row_h = h as f32 / gr.max(1) as f32;
        big.sort_by(|a, b| {
            let (ax, ay) = center(&comps[*a].1);
            let (bx, by) = center(&comps[*b].1);
            ((ay / row_h) as u32, ax as u32).cmp(&((by / row_h) as u32, bx as u32))
        });
        return big
            .iter()
            .map(|&b| {
                let members: Vec<usize> = (0..comps.len()).filter(|&i| owner[i] == b).collect();
                let mut bb = comps[b].1;
                for &m in &members {
                    let c = comps[m].1;
                    bb = [bb[0].min(c[0]), bb[1].min(c[1]), bb[2].max(c[2]), bb[3].max(c[3])];
                }
                let mut im = Image::new((bb[2] - bb[0] + 1) as u32, (bb[3] - bb[1] + 1) as u32, [0; 4]);
                for y in bb[1]..=bb[3] {
                    for x in bb[0]..=bb[2] {
                        let l = label[y * w + x];
                        if l != u32::MAX && owner[l as usize] == b {
                            im.set((x - bb[0]) as u32, (y - bb[1]) as u32, k.px[y * w + x]);
                        }
                    }
                }
                im
            })
            .collect();
    }
    // Shapes that touch (busts whose shoulders meet) or a missing one: the even grid is
    // what the model was asked to draw, and fine unless its lines cut through drawings.
    let straddle = grid_straddle(k, gc, gr);
    if straddle > 0.03 {
        warnings.push(format!(
            "found {} separate shapes for {n} frames, and an even {gc}×{gr} grid cuts through drawings ({:.0}% of its lines between the heads); check the preview and regenerate if a frame is cut",
            big.len(),
            straddle * 100.0
        ));
    }
    let (cw, ch) = (k.w / gc, k.h / gr);
    (0..n).map(|i| k.crop((i % gc) * cw, (i / gc) * ch, cw, ch).trimmed()).collect()
}

fn pick(game: &Path, name: &str, variant: u32) -> Result<(), String> {
    let src = game.join("art/variants").join(format!("{name}-{variant}.png"));
    let sc = super::read_sidecar(&src)?;
    let out = sc.recipe["out"].as_str().ok_or("variant sidecar has no output path")?.to_string();
    let dest = game.join(&out);
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    super::write_sidecar(&dest, &sc)?;
    let pv = game.join("art/variants").join(format!("{name}-{variant}.preview.png"));
    if pv.exists() {
        let _ = std::fs::create_dir_all(game.join("art/preview"));
        let _ = std::fs::copy(&pv, game.join("art/preview").join(format!("{name}.png")));
    }
    eprintln!("picked variant {variant} (seed {}) → {}", sc.seed, dest.display());
    refresh_meta(game);
    println!("{}", dest.display());
    Ok(())
}

/// The flat background color a prompt asked for ("… solid pure blue color (#0000ff) …").
fn key_in_prompt(prompt: &str) -> Option<String> {
    let at = prompt.find("Background: one flat")?;
    let rest = &prompt[at..];
    let i = rest.find("(#")?;
    let hex = rest.get(i + 1..i + 8)?;
    img::parse_hex(hex).ok().map(|_| hex.to_string())
}

pub fn regen(a: &super::RegenArgs, sc: &Sidecar) -> Result<(), String> {
    let mut job: Job = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("sidecar recipe: {e}"))?;
    let asset_rel = super::rel(&a.asset, Path::new(""));
    if !asset_rel.ends_with(&job.out) {
        return Err(format!("{} is a candidate; `mb art pick` it first, then regenerate {}", a.asset.display(), job.out));
    }
    let game = super::game_of(&a.asset, &job.out);
    if let Some(s) = a.seed {
        job.seed = s;
    } else if a.new_seed {
        job.seed = super::fresh_seed();
    }
    if let Some(p) = &a.prompt {
        job.subject = p.clone();
    }
    if let Some(p) = &a.provider {
        job.provider = p.clone();
        if a.model.is_none() {
            job.model = provider::default_model(p, Media::Image).map(|m| m.id.to_string()).unwrap_or_default();
        }
    }
    if let Some(m) = &a.model {
        job.model = m.clone();
    }
    if let Some(t) = a.tolerance {
        job.tolerance = t;
    }
    if let Some(k) = &a.key {
        img::parse_hex(k).map_err(|e| format!("--key {k}: {e}"))?;
        job.key = Some(k.clone());
    } else if job.key.is_none() && job.keyed {
        // Sidecars from before the key was recorded: the color the prompt asked for.
        job.key = key_in_prompt(&sc.full_prompt);
    }
    if a.reprocess {
        let raw_path = game.join(&sc.raw);
        let raw =
            std::fs::read(&raw_path).map_err(|e| format!("{}: {e} (raw outputs live in art/raw/; regenerate without --reprocess)", raw_path.display()))?;
        let (style, style_ref) = load_style(&game, job.no_style)?;
        let palette = style.palette_rgb()?;
        // A grid --grid didn't fix follows the style as it is now (pixel_art, pixel_scale).
        if !job.grid_explicit {
            let scale = style.pixel_scale.max(1);
            job.grid = (style.pixel_art && !["background", "icon", "sky"].contains(&job.kind.as_str()) && job.size[0] % scale == 0 && job.size[1] % scale == 0)
                .then(|| [(job.size[0] / scale).max(1), (job.size[1] / scale).max(1)]);
        }
        let (im, mut facts) = process(&job, &style, &palette, &Image::decode(&raw)?)?;
        scene_facts(&game, &job, sc.model_text.as_deref(), &mut facts);
        let png = encode_budget(&im, &job, &mut facts)?;
        let dest = game.join(&job.out);
        std::fs::write(&dest, &png).map_err(|e| format!("{}: {e}", dest.display()))?;
        let mut sc2 = sc.clone();
        sc2.output_sha256 = super::sha256_hex(&png);
        sc2.derived.clear(); // a fresh output: earlier edits aren't in it
        sc2.style = style_ref;
        sc2.recipe = serde_json::to_value(&job).unwrap();
        sc2.result = facts.clone();
        super::write_sidecar(&dest, &sc2)?;
        let art = super::ensure_art_dir(&game)?;
        std::fs::create_dir_all(art.join("preview")).map_err(|e| e.to_string())?;
        std::fs::write(art.join("preview").join(format!("{}.png", job.name)), im.preview().encode_png()).map_err(|e| e.to_string())?;
        super::walkcheck::write_onion(&art.join("preview").join(format!("{}.png", job.name)), &im, job.frames, job.cols, job.size, &facts)?;
        eprintln!("reprocessed {} from {} (no provider call)", job.out, sc.raw);
        report(&dest, &im, png.len(), &facts, None);
        println!("{}", dest.display());
        refresh_meta(&game);
        return Ok(());
    }
    execute(&game, &job, a.variants.max(1), a.dry_run, super::max_cost(a.max_cost)).map(|_| ())
}

/// `mb art check`: budgets and provenance for a game's assets.
fn check(game: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(game.join("manifest.toml")).map_err(|e| format!("{}: {e}", game.join("manifest.toml").display()))?;
    let manifest = mb_format::Manifest::parse(&text)?;
    let (_, style_sha) = Style::load(game)?;
    let mut files = Vec::new();
    collect(&game.join("assets"), &mut files);
    files.sort();
    // Bundle entries (SPEC §1: 512): mb build packs the sidecars into one file
    // and each directory's lip timings into one.
    let names: Vec<String> = files.iter().filter(|p| p.is_file()).map(|p| super::rel(p, game)).collect();
    let sidecars = names.iter().filter(|n| n.ends_with(".gen.json")).count();
    let lips_dirs: std::collections::BTreeSet<&str> = names.iter().filter(|n| n.ends_with(".lips.txt")).map(|n| n.rsplit_once('/').map_or("", |d| d.0)).collect();
    let lips = names.iter().filter(|n| n.ends_with(".lips.txt")).count();
    let packed = names.len() - sidecars - lips + (sidecars > 0) as usize + lips_dirs.len();
    let mut total = 0usize;
    let mut startup = 0usize;
    let mut spent = 0.0;
    let mut problems = 0;
    let mut generated = 0;
    let mut images = 0;
    // Warnings the generation logged (a figure cut off at the image's edge, a letterboxed painting…).
    let mut logged: Vec<(String, String)> = Vec::new();
    eprintln!("{:34} {:>9} {:>11}  made by", "asset", "KB", "size");
    let icon = game.join("icon.png");
    for path in std::iter::once(icon.clone()).chain(files) {
        let name = super::rel(&path, game);
        if name.ends_with(".gen.json") || !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        if path != icon {
            total += bytes.len();
            if manifest.startup_assets.contains(&name) {
                startup += bytes.len();
            }
        }
        let mut notes = Vec::new();
        let mut dims = String::new();
        if name.ends_with(".png") {
            images += (path != icon) as usize;
            if let Some((w, h)) = img::png_dims(&bytes) {
                dims = format!("{w}×{h}");
                if w > 4096 || h > 4096 {
                    notes.push("over mb2d's 4096² limit".to_string());
                    problems += 1;
                }
                if path == icon && (w, h) != (256, 256) {
                    notes.push("icon.png must be 256×256".into());
                    problems += 1;
                }
            }
            if bytes.len() > 1200 << 10 {
                notes.push("big: quantize (--colors) or shrink".into());
            }
        }
        if (name.ends_with(".jpg") || name.ends_with(".jpeg"))
            && let Some((w, h)) = img::jpeg_dims(&bytes)
        {
            images += 1;
            dims = format!("{w}×{h}");
            if w > 2048 || h > 2048 {
                notes.push("over mb3d's 2048² texture limit".to_string());
                problems += 1;
            }
            if bytes.len() > 1200 << 10 {
                notes.push("big: mb regen it with --max-kb, or shrink".into());
            }
        }
        if name.ends_with(".ogg")
            && let Ok((ch, rate)) = super::audio::decode(&bytes, "audio/ogg")
        {
            dims = format!("{:.1} s", ch[0].len() as f32 / rate as f32);
            if ch.len() > 1 {
                notes.push("stereo (SPEC: use mono)".into());
            }
        }
        if name.to_ascii_lowercase().ends_with(".glb") {
            match super::shape::check(&bytes) {
                Ok((f, w)) => {
                    dims = format!("{} tris", f["triangles"]);
                    notes.extend(w);
                }
                Err(e) => {
                    notes.push(e);
                    problems += 1;
                }
            }
        }
        let made = match super::read_sidecar(&path) {
            Ok(sc) => {
                generated += 1;
                spent += sc.cost_usd.or(sc.est_cost_usd).unwrap_or(0.0);
                if sc.provider == "mock" {
                    notes.push("mock placeholder: replace before publishing".into());
                }
                if let (Some(s), Some(cur)) = (&sc.style, &style_sha)
                    && &s.sha256 != cur
                {
                    notes.push("made with an older style guide (mb regen to refresh)".into());
                }
                if let Some(n) = sc.provenance_of(&bytes) {
                    notes.push(n);
                }
                notes.extend(super::walkcheck::check_notes(&sc.recipe, &sc.result, &bytes));
                if let Some(cut) = sc.result["cut_off"].as_array().filter(|c| !c.is_empty()) {
                    notes.push(format!("cut off at the {} edge when generated", cut.iter().filter_map(|c| c.as_str()).collect::<Vec<_>>().join(" and ")));
                }
                for w in sc.result["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str()) {
                    logged.push((name.clone(), w.to_string()));
                }
                format!("{} {} seed {}", sc.provider, sc.model, sc.seed)
            }
            Err(_) => match std::fs::read_to_string(super::sidecar_path(&path)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
                Some(v) if v["packed_from"].is_array() => format!("mb art atlas of {} images", v["packed_from"].as_array().unwrap().len()),
                _ => "by hand or a script".into(),
            },
        };
        eprintln!(
            "{:34} {:>9} {:>11}  {made}{}",
            name,
            bytes.len().div_ceil(1024),
            dims,
            if notes.is_empty() { String::new() } else { format!("  ← {}", notes.join("; ")) }
        );
    }
    if !logged.is_empty() {
        eprintln!();
        eprintln!("warnings from when they were generated (look at art/preview/, regenerate what's wrong):");
        for (n, w) in &logged {
            eprintln!("  {n}: {w}");
        }
    }
    let metas = super::ground::game_meta(game);
    for l in scene_report(&metas) {
        eprintln!("{l}");
    }
    let meta_path = game.join("src/art_meta.rs");
    if game.join("src").is_dir() && !metas.is_empty() && std::fs::read_to_string(&meta_path).ok() != Some(super::ground::meta_rust(&metas)) {
        eprintln!("src/art_meta.rs is out of date with the images: mb art meta {}", game.display());
    }
    if let Some(w) = Style::load(game)?.0.leak_warning() {
        eprintln!("note: {w}");
    }
    eprintln!();
    eprintln!(
        "assets {:.2} MB of the 10 MB bundle (plus wasm); startup assets {:.2} MB of 3 MB (plus wasm); {images} images (≤ 128)",
        total as f64 / 1048576.0,
        startup as f64 / 1048576.0
    );
    eprintln!(
        "{} files in assets/, {packed} bundle entries once `mb build` packs {sidecars} sidecars and {lips} lip timings (of 512, with the manifest, icon, wasm and src/)",
        names.len()
    );
    eprintln!("{generated} generated asset(s), about ${spent:.2} spent on them");
    if total > 9 << 20 || startup > 3 << 20 || images > 128 {
        problems += 1;
        eprintln!("over budget: quantize big PNGs (mb regen <asset> --reprocess after setting colors in art/style.toml), lower music quality, or cut assets");
    }
    if problems > 0 { Err(format!("{problems} problem(s)")) } else { Ok(()) }
}

/// `mb art shows`: adds objects to a painting's `result.shows`.
fn record_shows(asset: &Path, objects: &str) -> Result<(), String> {
    let mut sc = super::read_sidecar(asset)?;
    let kind = sc.recipe["kind"].as_str().unwrap_or("");
    if !matches!(kind, "layer" | "background") {
        return Err(format!("{} is a {kind}: `mb art shows` is for layers and backgrounds", asset.display()));
    }
    let given: Vec<String> = objects.split(',').map(str::trim).filter(|o| !o.is_empty()).map(str::to_string).collect();
    let mut shows: Vec<String> = match sc.result["shows"].as_array() {
        Some(a) => a.iter().filter_map(|o| o.as_str().map(str::to_string)).collect(),
        None => super::scene::objects_in(sc.recipe["subject"].as_str().unwrap_or("")),
    };
    for o in super::scene::shows_json("", None, &given) {
        if !shows.contains(&o) {
            shows.push(o);
        }
    }
    sc.result["shows"] = json!(shows);
    super::write_sidecar(asset, &sc)?;
    let game = super::game_of(asset, sc.recipe["out"].as_str().unwrap_or(""));
    eprintln!("{} shows: {}", asset.display(), shows.join(", "));
    refresh_meta(&game);
    for l in scene_report(&super::ground::game_meta(&game)).iter().filter(|l| l.contains("already shows")) {
        eprintln!("warning: {}", l.trim());
    }
    Ok(())
}

/// `mb art check`'s scene: every figure's drawn height beside its real one, against the
/// scene's scale (warning where they disagree), and each painting's objects (warning where a
/// prop is one of them).
fn scene_report(metas: &[super::ground::Meta]) -> Vec<String> {
    use super::ground::Meta;
    let mut out = Vec::new();
    let scale = super::ground::scene_scale(metas);
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for m in metas {
        let Meta::Figure { name, height, real, .. } = m else { continue };
        let world = match real {
            Some((r, from)) => format!("{r} m{}", if from == "given" { String::new() } else { format!(" (a typical {from})") }),
            None => "real size unknown (--real)".into(),
        };
        let per = real.as_ref().map(|(r, _)| format!("{:.1} units/m", height / r)).unwrap_or_default();
        rows.push(format!("  {name:24} {height:>6.0} units  {world:28} {per}"));
        if let (Some((r, _)), Some((k, hero))) = (real, &scale)
            && let Some(want) = super::scene::implausible(*height, *r, *k)
        {
            warnings.push(format!("  {name} is {height:.0} units tall for {r} m: at {hero}'s scale ({k:.1} units a meter) it'd be about {want:.0} (Figure::sized(scale), or mb regen it with --height {want:.0})"));
        }
    }
    if !rows.is_empty() {
        out.push(String::new());
        out.push(match &scale {
            Some((k, hero)) => format!("figures, as tall as ground::Figure draws them and in the world (the scene: {k:.1} units a meter, from {hero}):"),
            None => "figures, as tall as ground::Figure draws them (size the hero with --height and give things --real so they can be checked against each other):".into(),
        });
        out.extend(rows);
        out.extend(warnings);
    }
    let mut dups = Vec::new();
    for m in metas {
        let Meta::Backdrop { path, shows, .. } = m else { continue };
        if shows.is_empty() {
            continue;
        }
        if !out.iter().any(|l| l.contains(" shows: ")) {
            out.push(String::new());
            out.push("what the paintings show (`mb art shows` adds what you see in one):".into());
        }
        out.push(format!("  {path} shows: {}", shows.join(", ")));
        for f in metas {
            if let Meta::Figure { path: prop, subject, .. } = f {
                for o in shows.iter().filter(|o| super::scene::same_thing(o, subject)) {
                    dups.push(format!("  {}", dup_warning(path, o, prop)));
                }
            }
        }
    }
    out.extend(dups);
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_near_layer_keys_its_sky_and_keeps_its_ground() {
        // Sky (the key color) over scenery that fills the sides, then a strip of
        // empty sky under the scenery, as a model draws it.
        let (sky, rock) = ([0u8, 0, 255, 255], [70u8, 50, 40, 255]);
        let mut raw = Image::new(400, 300, sky);
        for y in 0..300 {
            for x in 0..400 {
                let hill = 120 + ((x as f32 / 30.0).sin() * 20.0) as u32;
                if (y >= hill && y < 260) || x < 30 || x >= 370 {
                    raw.set(x, y, rock);
                }
            }
        }
        let mut j = job("layer");
        j.size = [200, 100];
        j.layer = Some((1, 2));
        j.key = Some("#0000ff".into());
        let (im, facts) = process(&j, &Style::default(), &[], &raw).unwrap();
        assert_eq!((im.w, im.h), (200, 100));
        // Keyed from the sky, not the scenery around the border.
        assert_eq!(facts["keyed_background"], "#0000ff");
        assert_eq!(im.get(100, 0)[3], 0, "sky keyed out");
        // The bottom is the scenery's lowest solid row (the empty strip is gone), solid all along.
        assert!((0..200).all(|x| im.get(x, 99)[3] == 255 && im.get(x, 99)[2] < 100), "{:?}", im.get(100, 99));
        assert_eq!(facts["layer"]["transparent"], true);
        // The far layer stays opaque, cropped from the bottom.
        let mut far = job("layer");
        far.size = [200, 100];
        far.layer = Some((0, 2));
        far.keyed = false;
        let (im, facts) = process(&far, &Style::default(), &[], &raw).unwrap();
        assert!(im.is_opaque());
        assert_eq!(facts["layer"]["transparent"], false);
        assert_eq!(im.get(100, 99)[2], 255, "the bottom strip is kept: it's the bottom of the image");
    }

    #[test]
    fn a_portrait_grid_warns_only_when_it_cuts_a_head() {
        // Four busts whose shoulders meet: one shape, but the grid is clean.
        let mut sheet = Image::new(400, 100, [0; 4]);
        for c in 0..4u32 {
            for y in 10..60 {
                for x in c * 100 + 30..c * 100 + 70 {
                    sheet.set(x, y, [200, 150, 120, 255]); // head
                }
            }
        }
        for y in 60..100 {
            for x in 0..400 {
                sheet.set(x, y, [40, 40, 90, 255]); // shoulders, wall to wall
            }
        }
        let mut w = Vec::new();
        let parts = split_frames(&sheet, 4, 4, 1, &mut w);
        assert_eq!(parts.len(), 4);
        assert!(w.is_empty(), "{w:?}");
        // Heads drawn across the cell lines: warned.
        let mut off = Image::new(400, 100, [0; 4]);
        for y in 0..100 {
            for x in 0..400 {
                if (x + 50) % 100 < 60 || y >= 60 {
                    off.set(x, y, [200, 150, 120, 255]);
                }
            }
        }
        let mut w = Vec::new();
        split_frames(&off, 4, 4, 1, &mut w);
        assert!(w[0].contains("cuts through drawings"), "{w:?}");
        // Background left at the image's edge is measured against the keyed colors.
        let mut k = Image::new(100, 100, [0; 4]);
        for x in 0..100 {
            k.set(x, 0, [250, 5, 250, 255]);
        }
        assert!((background_left(&k, &[[255, 0, 255]], 0.09) - 0.25).abs() < 0.01);
        assert_eq!(background_left(&k, &[[0, 255, 0]], 0.09), 0.0);
    }

    #[test]
    fn a_figure_cut_off_at_the_bottom_is_warned_and_footed() {
        // A witch on magenta whose skirt runs off the bottom of the image.
        let mut raw = Image::new(200, 200, [255, 0, 255, 255]);
        for y in 30..200 {
            for x in 80..120 {
                raw.set(x, y, [40, 30, 60, 255]);
            }
        }
        let mut j = job("sprite");
        j.anchor = Anchor::Bottom;
        j.key = Some("#ff00ff".into());
        let (_, facts) = process(&j, &Style::default(), &[], &raw).unwrap();
        assert_eq!(facts["cut_off"], json!(["bottom"]));
        assert!(facts["warnings"][0].as_str().unwrap().contains("runs off the bottom of the generated image"), "{facts}");
        // Whole, she gets a footing: feet on the bottom margin.
        for y in 180..200 {
            for x in 80..120 {
                raw.set(x, y, [255, 0, 255, 255]);
            }
        }
        let (_, facts) = process(&j, &Style::default(), &[], &raw).unwrap();
        assert!(facts["cut_off"].is_null() && facts["warnings"].as_array().unwrap().is_empty(), "{facts}");
        assert_eq!(facts["ground"]["foot"][1], 62.0);
        assert!(facts["stand"].as_str().unwrap().contains("art_meta::T"));
    }

    #[test]
    fn a_letterboxed_painting_is_unboxed_and_its_floor_placed() {
        // A corridor (wall, floor below row 160 of 200) inside black bars.
        let mut raw = Image::new(400, 300, [0, 0, 0, 255]);
        for y in 0..200u32 {
            for x in 0..400u32 {
                let n = ((x * 7 + y * 13) % 11) as u8;
                let c = if y < 160 { [120 + n, 90 + n, 70 + n, 255] } else { [40 + n, 28 + n, 22 + n, 255] };
                raw.set(x, y + 50, c);
            }
        }
        let mut j = job("layer");
        j.size = [200, 100];
        j.layer = Some((0, 1));
        j.keyed = false;
        j.floor = Some(0.8);
        let (im, facts) = process(&j, &Style::default(), &[], &raw).unwrap();
        assert!(facts["warnings"][0].as_str().unwrap().contains("letterboxed"), "{facts}");
        assert_eq!(facts["floor"]["found"], "given");
        let y = facts["floor"]["y"].as_f64().unwrap();
        assert!((y - 80.0).abs() <= 2.0, "{facts}");
        assert!(im.is_opaque() && im.get(10, 0)[0] > 60, "no black bar left at the top");
    }

    fn figure_meta(name: &str, subject: &str, height: f32, requested: bool, real: Option<(f32, &str)>, made: &str) -> super::super::ground::Meta {
        super::super::ground::Meta::Figure {
            name: name.into(),
            path: format!("assets/{name}.png"),
            footing: super::super::ground::Footing { cell: [128, 128], foot: [64.0, 120.0], top: 20, base: 30, span: [40, 90] },
            cells: 1,
            cols: 1,
            height,
            requested,
            subject: subject.into(),
            real: real.map(|(m, f)| (m, f.to_string())),
            scaled: false,
            made: made.into(),
        }
    }

    #[test]
    fn the_scene_lists_sizes_against_the_hero_and_painted_twins() {
        use super::super::ground::{Meta, scene_scale};
        let metas = vec![
            Meta::Backdrop { name: "attic_1".into(), path: "assets/attic_1.png".into(), size: [1440, 640], floor: None, given: false, shows: vec!["rocking horse".into(), "mirror".into()] },
            figure_meta("girl", "a girl in a nightgown holding a candle", 60.0, true, Some((1.3, "girl")), "2026-10-01T00:00:00Z"),
            figure_meta("mirror", "a tall standing mirror", 30.0, true, Some((1.6, "mirror")), "2026-10-02T00:00:00Z"),
            figure_meta("horse", "a wooden rocking horse", 41.0, true, Some((0.9, "rocking horse")), "2026-10-03T00:00:00Z"),
            figure_meta("bell", "a brass bell", 16.0, true, Some((0.15, "bell")), "2026-10-04T00:00:00Z"),
            figure_meta("orb", "a glowing orb", 20.0, false, None, "2026-10-05T00:00:00Z"),
        ];
        // The scale is the first figure sized with --height: the girl, 60 units for 1.3 m.
        let (k, hero) = scene_scale(&metas).unwrap();
        assert_eq!(hero, "girl");
        assert!((k - 46.15).abs() < 0.01);
        let lines = scene_report(&metas).join("\n");
        // The mirror at half the girl's height is called out, with the height that would match.
        assert!(lines.contains("mirror is 30 units tall for 1.6 m: at girl's scale (46.2 units a meter) it'd be about 74"), "{lines}");
        assert!(!lines.contains("bell is") && !lines.contains("horse is") && lines.contains("real size unknown"), "{lines}");
        // The rocking horse prop beside the painted one.
        assert!(lines.contains("assets/attic_1.png already shows a rocking horse (painted in), and assets/horse.png is one too"), "{lines}");
        assert!(lines.contains("assets/attic_1.png already shows a mirror"), "{lines}");
        assert!(!lines.contains("girl.png is one too"), "a girl holding a candle isn't a mirror: {lines}");
    }

    #[test]
    fn heights_size_figures() {
        assert_eq!(parse_height("96").unwrap(), Height::Units(96.0));
        assert_eq!(parse_height("1.3xgirl").unwrap(), Height::Of(1.3, "girl".into()));
        assert!(parse_height("tall").is_err() && parse_height("0").is_err());
        // 60 units at 2 texels a unit, with the default margin, in whole art pixels.
        assert_eq!(size_for_height(60.0, 1), [138, 138]);
        assert_eq!(size_for_height(60.0, 4), [140, 140]);
    }

    #[test]
    fn an_older_sidecars_key_comes_from_its_prompt() {
        let p = "A moth. Background: one flat, solid pure magenta color (#ff00ff) filling everything behind the subject: no gradient.";
        assert_eq!(key_in_prompt(p).as_deref(), Some("#ff00ff"));
        assert_eq!(key_in_prompt("A moth (#ff00ff)."), None);
    }

    #[test]
    fn aspects() {
        assert_eq!(nearest_aspect(128.0, 128.0), "1:1");
        assert_eq!(nearest_aspect(720.0, 1280.0), "9:16");
        assert_eq!(nearest_aspect(1440.0, 1280.0), "5:4");
        assert_eq!(nearest_aspect(1000.0, 100.0), "21:9");
        assert!(parse_wh("128x64").is_ok());
        assert!(parse_wh("5000x64").is_err());
    }

    fn job(kind: &str) -> Job {
        Job {
            kind: kind.into(),
            name: "t".into(),
            subject: "x".into(),
            out: "assets/t.png".into(),
            size: [64, 64],
            grid_explicit: false,
            grid: None,
            refs: vec![],
            seed: 1,
            provider: "mock".into(),
            model: "mock-image".into(),
            resolution: "0.5K".into(),
            quality: "medium".into(),
            aspect: "1:1".into(),
            anchor: Anchor::Center,
            margin: 2,
            colors: None,
            max_kb: None,
            frames: 0,
            cols: 0,
            expressions: vec![],
            layer: None,
            keyed: true,
            key: None,
            tolerance: 0.09,
            no_style: false,
            allow_names: vec![],
            horizon: None,
            span: 180.0,
            poles: 12.0,
            height: None,
            real: None,
            sized_by: None,
            shows: Vec::new(),
            floor: None,
            walker: false,
        }
    }

    fn mock_raw(j: &Job) -> Image {
        let req = ImageRequest {
            model: "mock-image".into(),
            prompt: "a creature".into(),
            refs: vec![],
            aspect: j.aspect.clone(),
            resolution: "0.5K".into(),
            transparent: j.keyed,
            seed: Some(3),
            quality: 1,
            hint: MockHint { kind: j.kind.clone(), frames: j.frames, key: Some([255, 0, 255]), palette: vec![], pixel_art: j.grid.is_some() },
        };
        super::super::mock::image(&req)
    }

    #[test]
    fn sprite_is_exact_transparent_and_clean() {
        let j = job("sprite");
        let (im, facts) = process(&j, &Style::default(), &[], &mock_raw(&j)).unwrap();
        assert_eq!((im.w, im.h), (64, 64));
        assert_eq!(im.get(0, 0)[3], 0);
        assert_eq!(im.get(32, 32)[3], 255);
        let (x, y, w, h) = im.content_bbox(0).unwrap();
        assert!(x >= 2 && y >= 2 && x + w <= 62 && y + h <= 62, "margin kept: {x} {y} {w} {h}");
        // No magenta fringe survives.
        assert!(!im.px.iter().any(|p| p[3] > 60 && p[0] > 200 && p[1] < 60 && p[2] > 200), "magenta left");
        assert!(facts["warnings"].as_array().unwrap().is_empty(), "{facts}");
    }

    #[test]
    fn pixel_sprite_has_whole_art_pixels_in_palette() {
        let mut j = job("sprite");
        j.grid = Some([16, 16]);
        j.size = [64, 64];
        j.margin = 1;
        let style = Style { palette: vec!["#1d1b2f".into(), "#f2c14e".into(), "#5d8aa8".into(), "#fdf6e3".into()], palette_lock: true, ..Style::default() };
        let pal = style.palette_rgb().unwrap();
        let (im, _) = process(&j, &style, &pal, &mock_raw(&j)).unwrap();
        assert_eq!((im.w, im.h), (64, 64));
        for y in 0..64 {
            for x in 0..64 {
                assert_eq!(im.get(x, y), im.get(x / 4 * 4, y / 4 * 4), "4×4 blocks");
            }
        }
        for p in im.px.iter().filter(|p| p[3] > 0) {
            assert!(pal.contains(&[p[0], p[1], p[2]]) && p[3] == 255);
        }
    }

    #[test]
    fn frames_sheet() {
        let mut j = job("frames");
        j.frames = 4;
        j.cols = 4;
        j.aspect = "1:1".into();
        j.anchor = Anchor::Bottom;
        let (im, facts) = process(&j, &Style::default(), &[], &mock_raw(&j)).unwrap();
        assert_eq!((im.w, im.h), (256, 64));
        assert!(facts["warnings"].as_array().unwrap().is_empty(), "{facts}");
        // Every cell has a character standing on the bottom margin.
        for i in 0..4 {
            let cell = im.crop(i * 64, 0, 64, 64);
            let (_, y, _, h) = cell.content_bbox(0).expect("frame drawn");
            assert_eq!(y + h, 62, "frame {i} bottom-anchored");
        }
    }

    fn portrait_job(expressions: &[&str], size: u32, grid: Option<[u32; 2]>) -> (Job, Image) {
        let mut j = job("portrait");
        j.expressions = expressions.iter().map(|e| e.to_string()).collect();
        let rows = j.expressions.len() as u32 + 1;
        j.frames = 4 * rows;
        j.cols = 4;
        j.size = [size, size];
        j.grid = grid;
        j.margin = if grid.is_some() { 1 } else { 2 };
        j.anchor = Anchor::Bottom;
        j.aspect = nearest_aspect(4.0, rows as f32).into();
        let req = ImageRequest {
            model: "mock-image".into(),
            prompt: "a pilot".into(),
            refs: vec![],
            aspect: j.aspect.clone(),
            resolution: "0.5K".into(),
            transparent: true,
            seed: Some(5),
            quality: 1,
            hint: MockHint { kind: "portrait".into(), frames: j.frames, key: Some([255, 0, 255]), palette: PAL.to_vec(), pixel_art: grid.is_some() },
        };
        let raw = super::super::mock::image(&req);
        (j, raw)
    }

    const PAL: [[u8; 3]; 3] = [[20, 20, 30], [60, 110, 200], [250, 215, 170]];

    /// Dark pixels around the mouth: the middle of the face, from 45% to 72% of the cell's height
    /// (below the eyes, above the chin) and 30% to 70% of its width (inside the outline).
    fn dark_pixels(cell: &Image) -> usize {
        let band = cell.crop(cell.w * 30 / 100, cell.h * 45 / 100, cell.w * 40 / 100, cell.h * 27 / 100);
        band.px.iter().filter(|p| p[3] > 128 && (p[0] as i32 - 20).abs() + (p[1] as i32 - 20).abs() + (p[2] as i32 - 30).abs() < 60).count()
    }

    #[test]
    fn portrait_sheet_layout_mouths_and_alignment() {
        let (j, raw) = portrait_job(&["neutral", "smug", "angry"], 64, None);
        let (im, facts) = process(&j, &Style::default(), &PAL, &raw).unwrap();
        assert_eq!((im.w, im.h), (256, 256), "4 columns × (3 expressions + blink) rows of 64×64");
        let p = &facts["portrait"];
        assert_eq!(p["cols"], 4);
        assert_eq!(p["rows"], 4);
        assert_eq!(p["cell"], json!([64, 64]));
        assert_eq!(p["expressions"], json!(["neutral", "smug", "angry"]));
        assert_eq!(p["blink_row"], 3);
        assert!(facts["warnings"].as_array().unwrap().is_empty(), "{facts}");
        let cell = |c: u32, r: u32| im.crop(c * 64, r * 64, 64, 64);
        for r in 0..4 {
            // The mouth opens left to right: more dark mouth pixels in every column.
            let dark: Vec<usize> = (0..4).map(|c| dark_pixels(&cell(c, r))).collect();
            assert!(dark.windows(2).all(|w| w[1] > w[0]), "row {r}: mouth should open left to right: {dark:?}");
        }
        // The blink row has no eye whites; the others do.
        let whites = |c: &Image| c.px.iter().filter(|p| p[3] > 128 && p[0] > 240 && p[1] > 240 && p[2] > 240).count();
        assert!(whites(&cell(0, 0)) > 4 && whites(&cell(0, 3)) == 0, "{} {}", whites(&cell(0, 0)), whites(&cell(0, 3)));
        // The heads hold still: every cell's silhouette starts within a pixel or two of the first's.
        let (x0, y0, w0, _) = cell(0, 0).content_bbox(128).unwrap();
        for i in 1..16 {
            let (x, y, w, h) = cell(i % 4, i / 4).content_bbox(128).unwrap();
            assert!(x.abs_diff(x0) <= 2 && y.abs_diff(y0) <= 2 && w.abs_diff(w0) <= 3, "cell {i}: {x} {y} {w} vs {x0} {y0} {w0}");
            assert!(y + h >= 62, "cell {i} stands on the bottom");
        }
    }

    #[test]
    fn pixel_portrait_is_whole_art_pixels() {
        let (j, raw) = portrait_job(&["neutral"], 128, Some([32, 32]));
        let style = Style { palette: PAL.iter().map(|c| img::hex(*c)).collect(), palette_lock: true, pixel_art: true, ..Style::default() };
        let (im, facts) = process(&j, &style, &PAL, &raw).unwrap();
        assert_eq!((im.w, im.h), (512, 256));
        assert_eq!(facts["portrait"]["blink_row"], 1);
        assert!(facts["rust"].as_str().unwrap().ends_with("Portrait::load(path, 128, 128).art_pixels(32)"), "{}", facts["rust"]);
        for y in 0..im.h {
            for x in 0..im.w {
                assert_eq!(im.get(x, y), im.get(x / 4 * 4, y / 4 * 4), "4×4 blocks");
            }
        }
        for p in im.px.iter().filter(|p| p[3] > 0) {
            assert!(PAL.contains(&[p[0], p[1], p[2]]) && p[3] == 255);
        }
        let dark: Vec<usize> = (0..4).map(|c| dark_pixels(&im.crop(c * 128, 0, 128, 128))).collect();
        assert!(dark.windows(2).all(|w| w[1] > w[0]), "mouth opens left to right: {dark:?}");
    }

    #[test]
    fn portrait_expressions_and_job() {
        assert_eq!(portrait_expressions(&[]).unwrap(), vec!["neutral"]);
        assert_eq!(portrait_expressions(&["Smug".into(), " angry".into(), "neutral".into(), "smug".into()]).unwrap(), vec!["neutral", "smug", "angry"]);
        assert!(portrait_expressions(&["blink".into()]).is_err());
        assert!(portrait_expressions(&(0..8).map(|i| format!("e{i}")).collect::<Vec<_>>()).is_err());
    }

    #[test]
    fn tile_wraps_and_background_is_opaque() {
        let mut j = job("tile");
        j.keyed = false;
        let (t, _) = process(&j, &Style::default(), &[], &mock_raw(&j)).unwrap();
        assert_eq!((t.w, t.h), (64, 64));
        assert!(img::wrap_error(&t, true) < 0.05 && img::wrap_error(&t, false) < 0.05);
        let mut b = job("background");
        b.keyed = false;
        b.size = [90, 160];
        b.aspect = "9:16".into();
        let (bg, _) = process(&b, &Style::default(), &[], &mock_raw(&b)).unwrap();
        assert_eq!((bg.w, bg.h), (90, 160));
        assert!(bg.is_opaque());
    }

    #[test]
    fn budget_quantizes() {
        let mut j = job("background");
        j.keyed = false;
        j.size = [256, 256];
        j.max_kb = Some(12);
        let (bg, mut facts) = process(&j, &Style::default(), &[], &mock_raw(&j)).unwrap();
        let png = encode_budget(&bg, &j, &mut facts).unwrap();
        assert!(png.len() <= 12 * 1024, "{}", png.len());
    }
}
