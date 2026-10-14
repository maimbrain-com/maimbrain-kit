//! A game's style guide, `art/style.toml`: the words, palette and reference
//! images every `mb art` generation for that game gets, so its art is one
//! consistent set. `mb art init` writes a starting one.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::img::{hex, oklab, oklab_dist, parse_hex};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KindStyle {
    /// Extra words for this kind of asset ("side view, facing right").
    #[serde(default)]
    pub prompt: String,
    /// Default output size in PNG pixels [w, h].
    #[serde(default)]
    pub size: Option<[u32; 2]>,
    /// Things never to draw in this kind of asset, on top of the guide's `avoid`:
    /// `[sprite] avoid = ["candles"]` when candles are the pickups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoid: Vec<String>,
    /// `[layer]` / `[background]`: put each painting's floor line this far down (0–1 from
    /// the top), so the floors of every painting line up (`--floor` per command).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Style {
    #[serde(default)]
    pub name: String,
    /// The look in a sentence or two: medium, shapes, line, light, mood.
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub avoid: Vec<String>,
    /// Hex colors, darkest to lightest reads best.
    #[serde(default)]
    pub palette: Vec<String>,
    /// Snap every pixel of every asset to the palette.
    #[serde(default)]
    pub palette_lock: bool,
    #[serde(default)]
    pub pixel_art: bool,
    /// Texels per art pixel for pixel art (SPEC §5.3: 4, drawn 2 units per art pixel).
    #[serde(default = "four")]
    pub pixel_scale: u32,
    /// Quantize to at most this many colors (0: off).
    #[serde(default)]
    pub colors: u32,
    /// Style reference images (paths relative to the game directory), up to 3.
    #[serde(default)]
    pub references: Vec<String>,
    /// The flat background color asked for on assets that get a transparent
    /// background: "auto" picks one far from the palette.
    #[serde(default = "auto")]
    pub key: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// "1K" (default) or "2K" etc.; backgrounds and icons default to 2K.
    #[serde(default)]
    pub resolution: Option<String>,
    /// Per-kind overrides: [sprite], [frames], [portrait], [background], [layer], [tile], [ui], [icon], [sky],
    /// and [music] / [sfx] (a `prompt` added to every `mb music` / `mb sfx`), [3d] (every
    /// `mb model` character) and [3d_object] (`mb model --kind part|prop|vehicle`).
    #[serde(flatten)]
    pub kinds: BTreeMap<String, KindStyle>,
}

fn four() -> u32 {
    4
}

fn auto() -> String {
    "auto".into()
}

impl Default for Style {
    fn default() -> Style {
        toml::from_str("").unwrap()
    }
}

pub const KINDS: &[&str] = &["sprite", "frames", "portrait", "background", "layer", "tile", "ui", "icon", "sky", "music", "sfx", "3d", "3d_object"];

impl Style {
    pub fn load(game: &Path) -> Result<(Style, Option<String>), String> {
        let path = game.join("art/style.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok((Style::default(), None));
        };
        let s: Style = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for k in s.kinds.keys() {
            if !KINDS.contains(&k.as_str()) {
                return Err(format!("{}: unknown section [{k}] (kinds: {})", path.display(), KINDS.join(", ")));
            }
        }
        s.palette_rgb().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((s, Some(super::sha256_hex(text.as_bytes()))))
    }

    /// Why the style guide doesn't set a look, if it doesn't (an empty `style`,
    /// or no art/style.toml): generations then get the generator's own look.
    pub fn unset_warning(&self) -> Option<&'static str> {
        self.style.trim().is_empty().then_some(
            "art/style.toml sets no `style`, so this gets the generator's own generic look; write this game's look there (medium, shapes, line, light, mood), from DESIGN.md's identity",
        )
    }

    pub fn palette_rgb(&self) -> Result<Vec<[u8; 3]>, String> {
        self.palette.iter().map(|c| parse_hex(c)).collect()
    }

    pub fn kind(&self, kind: &str) -> KindStyle {
        self.kinds.get(kind).cloned().unwrap_or_default()
    }

    /// Objects the style words name that a model draws into every picture
    /// ("lit by a candle" puts a candle in every prop). Sprites, frames,
    /// portraits, UI and the icon avoid them unless their subject names them.
    pub fn leaky_objects(&self) -> Vec<&'static str> {
        let text = format!("{} {}", self.style, self.keywords.join(" ")).to_lowercase();
        LEAKS.iter().filter(|(_, words)| words.iter().any(|w| has_word(&text, w))).map(|(o, _)| *o).collect()
    }

    /// Why the style's words may put objects into every prop, if they may.
    pub fn leak_warning(&self) -> Option<String> {
        let leaks = self.leaky_objects();
        (!leaks.is_empty()).then(|| {
            format!(
                "art/style.toml's `style` names {} (light sources and things are objects to a model), so sprites, frames, portraits, UI and the icon avoid them unless their prompt asks for one. Say how the light looks instead (\"warm, low, flickering light from below\"); list anything else props must never have under [sprite] avoid",
                leaks.join(", ")
            )
        })
    }

    /// The flat background color to ask for: the style's, or the candidate
    /// farthest from every palette color.
    pub fn key_color(&self) -> Result<[u8; 3], String> {
        if self.key != "auto" {
            return parse_hex(&self.key);
        }
        let pal: Vec<[f32; 3]> = self.palette_rgb()?.into_iter().map(oklab).collect();
        let cands: [[u8; 3]; 4] = [[255, 0, 255], [0, 255, 0], [0, 255, 255], [0, 0, 255]];
        Ok(*cands
            .iter()
            .max_by(|a, b| {
                let d = |c: &[u8; 3]| pal.iter().map(|p| oklab_dist(oklab(*c), *p)).fold(f32::MAX, f32::min);
                d(a).partial_cmp(&d(b)).unwrap()
            })
            .unwrap())
    }
}

fn color_name(c: [u8; 3]) -> &'static str {
    match c {
        [255, 0, 255] => "magenta",
        [0, 255, 0] => "pure green",
        [0, 255, 255] => "cyan",
        [0, 0, 255] => "pure blue",
        _ => "flat",
    }
}

/// How frames are laid out in the image the model is asked for: a
/// near-square grid (very wide images aren't supported), columns × rows.
pub fn frame_grid(n: u32) -> (u32, u32) {
    let n = n.max(1);
    let c = (n as f32).sqrt().ceil() as u32;
    (c, n.div_ceil(c))
}

/// The mouth shapes of a talking-portrait sheet's columns, left to right.
pub const MOUTHS: [&str; 4] = ["closed", "slightly open", "open", "wide open"];

/// Objects that style words name, and the words that name them.
const LEAKS: &[(&str, &[&str])] = &[
    ("candles", &["candle", "candles", "candlelit", "candle-lit", "candlelight"]),
    ("lanterns", &["lantern", "lanterns", "lantern-lit", "lanternlight"]),
    ("torches", &["torch", "torches", "torchlit", "torch-lit", "torchlight"]),
    ("lamps", &["lamp", "lamps", "lamplit", "lamp-lit", "lamplight"]),
    ("open flames", &["firelight", "firelit", "fire-lit", "campfire", "bonfire", "fireplace", "hearth", "flame", "flames"]),
    ("chandeliers", &["chandelier", "chandeliers"]),
    ("light bulbs", &["lightbulb", "lightbulbs", "light bulb", "light bulbs"]),
    ("fireflies", &["firefly", "fireflies"]),
    ("neon signs", &["neon sign", "neon signs"]),
];

/// Whether `text` has `word` as a whole word.
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// "side view" → "Side view." (a sentence for the prompt).
fn sentence(s: &str) -> String {
    let s = s.trim().trim_end_matches('.');
    let mut c = s.chars();
    match c.next() {
        Some(f) => format!("{}{}.", f.to_uppercase(), c.as_str()),
        None => String::new(),
    }
}

/// What a prompt is built from.
pub struct PromptParts<'a> {
    pub kind: &'a str,
    pub subject: &'a str,
    pub frames: u32,
    /// Talking portraits: the expressions, one per row (the first is neutral); a blink row follows.
    pub expressions: &'a [String],
    pub grid: Option<(u32, u32)>,
    pub keyed: Option<[u8; 3]>,
    pub portrait: bool,
    pub style_refs: usize,
    pub subject_refs: usize,
}

/// Builds the full prompt: what the asset is, the style guide, the
/// background rule for keyed assets, and what to avoid.
pub fn compose(style: &Style, p: &PromptParts) -> String {
    // A walker (`mb art frames --walker`) is a frames sheet with its own description.
    let walker = p.kind == "walker";
    let p = &PromptParts { kind: if walker { "frames" } else { p.kind }, ..*p };
    let mut out = Vec::new();
    let subject = p.subject.trim().trim_end_matches('.');
    out.push(match p.kind {
        "frames" if walker => format!(
            "A 2D game sprite sheet of two drawings side by side in one horizontal row, with a wide gap of flat background between them so they don't touch. \
             Left: {subject}, standing in side view, drawn without legs or feet: the body ends cleanly just above the ankles (at the hem of a dress, robe or coat, or the bottom of the trousers), nothing below it. \
             Right: one of that character's feet on its own (its shoe, boot or paw), in side view with the toes pointing the way the body faces, drawn at exactly the same scale as the body, so it is small next to it. Nothing else in the image."
        ),
        "sprite" => format!("A single 2D game sprite of {subject}. Centered, the whole subject visible with empty margin around it, a clear readable silhouette, no ground and no cast shadow."),
        "frames" => {
            let (c, r) = frame_grid(p.frames);
            let layout = if r == 1 { "in one horizontal row".to_string() } else { format!("in a grid of {c} columns and {r} rows, in reading order") };
            let cycle = super::walkcheck::gait_word("", subject).and_then(|g| super::walkcheck::cycle_prompt(g, p.frames, subject));
            format!(
                "An animation sprite sheet: {} frames of {subject}, {layout}, evenly spaced with clear gaps so no two frames touch, the same size, scale and facing in every frame, each frame the next step of the motion so they loop.{} Nothing else in the image.",
                p.frames,
                cycle.map(|c| format!(" {c}")).unwrap_or_default()
            )
        }
        "portrait" => {
            let rows = p.expressions.len() + 1;
            let mut row_list: Vec<String> = p.expressions.iter().enumerate().map(|(i, e)| format!("row {} {}", i + 1, damage_row(e))).collect();
            row_list.push(format!("row {rows} the neutral face with both eyes fully closed (a blink), the mouth still changing across the columns"));
            let damaged = p.expressions.iter().any(|e| damage_row(e) != *e);
            format!(
                "A talking-portrait sprite sheet for a game's dialogue: one character, {subject}, drawn {} times in a grid of 4 columns and {rows} rows of equal square cells, with clear gaps of flat background between the cells so no two portraits touch. \
                 Every cell is the same head-and-shoulders bust facing the viewer, centered in its cell, at exactly the same position, size, pose, framing, lighting and colors, the shoulders cut off by a straight horizontal line near the bottom of the cell. \
                 Keep the character identical in every cell (same face shape, hair, clothes, accessories, proportions and outline): it is one drawing whose mouth, eyes and eyebrows are redrawn, and nothing else moves{}. \
                 Columns are mouth shapes for lip-sync, left to right: 1 mouth {}, 2 mouth {}, 3 mouth {}, 4 mouth {} as if shouting. \
                 Rows are expressions, top to bottom: {}. Nothing else in the image.",
                4 * rows,
                if damaged { ", except that the damaged rows add their marks (bruises, soot, scratches, a cracked visor) on top of the same drawing" } else { "" },
                MOUTHS[0],
                MOUTHS[1],
                MOUTHS[2],
                MOUTHS[3],
                row_list.join("; ")
            )
        }
        "background" => format!(
            "A full-screen 2D game background, {} composition: {subject}. No characters, no text; keep the middle calm and lower in contrast so game objects read on top. The picture runs to the image's edges (no bars or border); any floor meets the wall along one straight, level line.",
            if p.portrait { "tall portrait" } else { "wide landscape" }
        ),
        "layer" => format!(
            "One parallax layer of a side-scrolling 2D game background: {subject}. Only this layer's shapes, wide and horizontally continuous, the rest of the image empty. Any floor or ground meets what's behind it along one straight, level line across the whole width, and the picture runs to the image's edges (no bars or border)."
        ),
        "tile" => format!("A seamless, tileable square texture for a 2D game: {subject}. Evenly lit, no vignette, no border; the pattern continues across all four edges."),
        "sky" => format!(
            "A seamless 360-degree equirectangular panorama for a 3D game's sky, like a photo sphere unwrapped: {subject}. The horizon is one straight, level line across the exact vertical middle of the image; the top edge is straight overhead and the bottom edge straight down, so the top and bottom rows are each one even color; the left and right edges are the same direction and continue into each other. Everything is far away: sky above the middle, only distant scenery along the horizon, nothing in the foreground."
        ),
        "ui" => format!("A 2D game UI element: {subject}. Flat-on, centered, crisp edges, no text or letters."),
        "icon" => format!("A square app icon for a mobile game: {subject}. A bold close-up of the hero or hook that fills the frame, strong silhouette, rich color, no text, letters or numbers, no border."),
        _ => subject.to_string(),
    });
    let ks = style.kind(p.kind);
    if !ks.prompt.trim().is_empty() {
        out.push(sentence(&ks.prompt));
    }
    if !style.style.trim().is_empty() {
        out.push(format!("Art style: {}.", style.style.trim().trim_end_matches('.')));
    }
    // A prop is only its subject: style words like "lit by a candle" say how it's lit, not that a candle is in it.
    let props = matches!(p.kind, "sprite" | "frames" | "portrait" | "ui" | "icon");
    let subject_lower = subject.to_lowercase();
    let leaked: Vec<&str> = if props {
        LEAKS.iter().filter(|(o, words)| style.leaky_objects().contains(o) && !words.iter().any(|w| has_word(&subject_lower, w))).map(|(o, _)| *o).collect()
    } else {
        Vec::new()
    };
    if props && !style.style.trim().is_empty() {
        out.push("The art style says how the subject is drawn, colored and lit; draw nothing it names that the subject doesn't have (no extra light sources, props or scenery).".into());
    }
    if matches!(p.kind, "sprite" | "frames") {
        out.push("Nothing cut off: the whole subject from top to bottom (a figure from its head down to its feet or hem), with empty background below and around it.".into());
    }
    if !style.keywords.is_empty() {
        out.push(sentence(&style.keywords.join(", ")));
    }
    if style.pixel_art || p.grid.is_some() {
        match p.grid {
            Some((w, h)) => out.push(format!(
                "Pixel art: big crisp square pixels as if {}drawn on a {w}×{h} grid, no anti-aliasing, no dithering noise, a limited palette.",
                if p.kind == "portrait" { "each cell were " } else { "" }
            )),
            None => out.push("Pixel art: big crisp square pixels, no anti-aliasing, a limited palette.".into()),
        }
    }
    if !style.palette.is_empty() {
        out.push(format!("Use only these colors: {}.", style.palette.join(", ")));
    }
    if p.style_refs > 0 {
        let which = if p.style_refs == 1 { "the first attached image".to_string() } else { format!("the first {} attached images", p.style_refs) };
        out.push(format!("Match the art style of {which} (palette, line work, shading, proportions); don't copy their subjects."));
    }
    if p.subject_refs > 0 {
        let which = if p.subject_refs == 1 { "The last attached image shows".to_string() } else { format!("The last {} attached images show", p.subject_refs) };
        out.push(format!("{which} the subject: redraw it in this art style, keeping what makes it recognizable."));
    }
    if let Some(k) = p.keyed {
        out.push(format!(
            "Background: one flat, solid {} color ({}) filling everything behind the subject: no gradient, no texture, no shadow, no glow. Don't use that color anywhere in the subject.",
            color_name(k),
            hex(k)
        ));
    }
    let mut avoid = vec!["text", "letters", "watermark", "signature", "logo", "frame or border"];
    if p.kind == "icon" {
        avoid.retain(|a| *a != "frame or border");
    }
    let extra: Vec<&str> = style.avoid.iter().chain(&ks.avoid).map(String::as_str).chain(leaked).collect();
    out.push(format!("Avoid: {}.", avoid.into_iter().chain(extra).collect::<Vec<_>>().join(", ")));
    out.push("An original design: not any existing character, franchise, brand or logo.".into());
    out.join(" ")
}

pub const TEMPLATE: &str = r##"# The style guide for this game's generated art. `mb art` applies it to every
# image (sprites, sheets, backgrounds, layers, tiles, UI, the icon, skies) so they
# look like one set. Edit freely; regenerate with `mb regen <asset>`.

name = "NAME"

# The look in a sentence or two: medium, shapes, line, light, mood, from the
# game's identity (DESIGN.md). Games are crisp pixel art by default (`pixel_art`
# below); say what kind. Left empty, every image gets the image model's own
# generic look, and `mb art` and `mb model` warn. For example:
#   "16-bit console pixel art, two shading steps per color, cool night palette, moonlit"
#   "Game Boy pixel art: four greens, chunky clusters, no dithering"
#   "1-bit pixel art, black on paper white, careful silhouettes"
# Or turn pixel_art off for another look:
#   "gouache storybook painting, visible brush texture, muted earth tones, soft overcast light"
#   "black ink and grey wash on rough paper, one red accent, stark and quiet"
style = ""

# Short style words added to every prompt.
keywords = ["readable silhouettes", "high contrast"]

# Things this game's art should never have (text, watermarks and logos are always avoided).
# `style` says how things look and are lit, never what's in them: "lit by a candle"
# puts a candle in every prop (mb then keeps light sources it names out of props).
avoid = ["busy detail"]

# Hex colors, darkest first. Sent with every prompt; `palette_lock` snaps every pixel to them.
palette = ["#1d1b2f", "#3b3f6b", "#5d8aa8", "#f2c14e", "#f78154", "#fdf6e3"]
palette_lock = false

# Pixel art (the default): generate, then reduce to a grid of art pixels stored
# `pixel_scale` texels wide (SPEC §5.3: 4 texels per art pixel, drawn 2 units
# wide). Backgrounds, skies and the icon keep their full resolution. false for
# another look, and for a 3D game unless it uses the retro look (MB3D.md).
pixel_art = true
pixel_scale = 4

# Quantize every asset to at most this many colors (0: off). Smaller PNGs.
colors = 0

# Up to 3 images (paths relative to the game directory) whose style every
# generation should match: a mood board, your hero sprite once you like it.
references = []

# Background color asked for on assets that get a transparent background
# ("auto" picks one far from the palette).
key = "auto"

# Defaults for this game (override per command with --provider/--model/--res).
# provider = "gemini"
# model = "gemini-nano-banana-2.1"
# resolution = "1K"

# Per-kind additions and default sizes (PNG pixels; the canvas is 2 px per logical unit).
[sprite]
prompt = "side view"
size = [128, 128]
# Never in a sprite (on top of `avoid`): what the game means by pickups, say.
# avoid = ["candles"]

# Side-scrollers: every painting's floor line this far down (0–1), so floors line up.
# [layer]
# floor = 0.85

[background]
prompt = ""

[icon]
prompt = "the hero's face, three-quarter view"

# Words added to every `mb music` and `mb sfx` prompt, so the sound is one set too.
[music]
prompt = "warm analog synths and soft percussion, playful"

[sfx]
prompt = "crisp and short, soft attack, no reverb tail"
"##;

/// A portrait row's description: damage words ("hurt", "battered"…) spelled
/// out, so the row shows the marks the game's damage stages call for.
fn damage_row(e: &str) -> String {
    match e {
        "hurt" | "pained" | "wounded" | "injured" => format!("{e} (wincing in pain: eyes squeezed, teeth gritted, a scratch or bruise, a bead of sweat)"),
        "battered" | "beaten" | "bruised" | "damaged" | "wrecked" => {
            format!("{e} (the same bust after a beating: black eye and bruises, soot smudges, scratches, a cracked visor or goggles if worn, exhausted and hurting)")
        }
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let s: Style = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(s.palette.len(), 6);
        assert_eq!(s.kind("sprite").size, Some([128, 128]));
        assert_eq!(s.pixel_scale, 4);
        s.key_color().unwrap();
    }

    #[test]
    fn template_is_pixel_art_with_no_look_words() {
        // Pixel art by default; the game writes what kind.
        let s: Style = toml::from_str(TEMPLATE).unwrap();
        assert!(s.pixel_art);
        assert!(s.unset_warning().is_some());
        assert!(Style { style: "ink and grey wash".into(), ..s }.unset_warning().is_none());
    }

    #[test]
    fn unknown_fields_are_errors() {
        // A misspelled key lands in the per-kind map, where it isn't a table.
        assert!(toml::from_str::<Style>("colour = 3").is_err());
    }

    #[test]
    fn key_color_avoids_palette() {
        let s = Style { palette: vec!["#ff10f0".into(), "#e000e0".into()], ..Style::default() };
        assert_ne!(s.key_color().unwrap(), [255, 0, 255]);
        let g = Style { palette: vec!["#10ff10".into()], ..Style::default() };
        assert_ne!(g.key_color().unwrap(), [0, 255, 0]);
        let fixed = Style { key: "#123456".into(), ..Style::default() };
        assert_eq!(fixed.key_color().unwrap(), [0x12, 0x34, 0x56]);
    }

    #[test]
    fn portrait_prompt_spells_out_the_layout() {
        let ex = vec!["neutral".to_string(), "smug".to_string(), "angry".to_string()];
        let p = compose(
            &Style::default(),
            &PromptParts {
                kind: "portrait",
                subject: "a grinning mech pilot with a dented orange helmet",
                frames: 16,
                expressions: &ex,
                grid: Some((48, 48)),
                keyed: Some([0, 255, 0]),
                portrait: false,
                style_refs: 0,
                subject_refs: 0,
            },
        );
        for want in [
            "4 columns and 4 rows",
            "drawn 16 times",
            "row 1 neutral; row 2 smug; row 3 angry; row 4 the neutral face with both eyes fully closed",
            "1 mouth closed, 2 mouth slightly open, 3 mouth open, 4 mouth wide open",
            "exactly the same position, size",
            "each cell were drawn on a 48×48 grid",
            "pure green",
        ] {
            assert!(p.contains(want), "missing {want:?} in {p}");
        }
    }

    #[test]
    fn damage_rows_are_spelled_out() {
        let ex = vec!["neutral".to_string(), "hurt".to_string(), "battered".to_string()];
        let parts = PromptParts { kind: "portrait", subject: "a mech pilot", frames: 0, expressions: &ex, grid: None, keyed: None, portrait: false, style_refs: 0, subject_refs: 0 };
        let p = compose(&Style::default(), &parts);
        assert!(p.contains("row 2 hurt (wincing") && p.contains("row 3 battered (the same bust after a beating"), "{p}");
        assert!(p.contains("the damaged rows add their marks"));
        let plain = vec!["neutral".to_string(), "smug".to_string()];
        assert!(!compose(&Style::default(), &PromptParts { expressions: &plain, ..parts }).contains("damaged rows"));
    }

    #[test]
    fn style_words_never_put_objects_into_props() {
        let s = Style {
            style: "gouache storybook painting, a dim manor lit by a candle, deep shadows".into(),
            avoid: vec!["photorealism".into()],
            kinds: [("sprite".to_string(), KindStyle { avoid: vec!["keys".into()], ..KindStyle::default() })].into(),
            ..Style::default()
        };
        assert_eq!(s.leaky_objects(), vec!["candles"]);
        assert!(s.leak_warning().unwrap().contains("names candles"));
        let parts = |kind, subject| PromptParts { kind, subject, frames: 4, expressions: &[], grid: None, keyed: None, portrait: true, style_refs: 0, subject_refs: 0 };
        let doll = compose(&s, &parts("sprite", "a porcelain doll"));
        assert!(doll.contains("Avoid: text, letters, watermark, signature, logo, frame or border, photorealism, keys, candles."), "{doll}");
        assert!(doll.contains("draw nothing it names that the subject doesn't have"), "{doll}");
        assert!(doll.contains("from its head down to its feet or hem"), "{doll}");
        // The pickup itself is a candle: not avoided.
        assert!(!compose(&s, &parts("sprite", "a lit candle in a brass holder")).contains("candles."));
        // Paintings may have them; the kind's avoid list is the kind's.
        let hall = compose(&s, &parts("layer", "a long manor corridor"));
        assert!(!hall.contains("candles") || hall.contains("lit by a candle"), "{hall}");
        assert!(!hall.contains("keys") && hall.contains("straight, level line"), "{hall}");
        // Whole words only: "lamplighter" names no lamp; "candlelit" names a candle.
        assert!(Style { style: "a lamplighter's town".into(), ..Style::default() }.leaky_objects().is_empty());
        assert_eq!(Style { keywords: vec!["candlelit".into()], ..Style::default() }.leaky_objects(), vec!["candles"]);
    }

    #[test]
    fn prompt_has_style_palette_key_and_originality() {
        let mut s: Style = toml::from_str(TEMPLATE).unwrap();
        s.style = "ink and grey wash, one red accent".into();
        s.avoid.push("photorealism".into());
        let p = compose(
            &s,
            &PromptParts {
                kind: "sprite",
                subject: "a moth with glass wings",
                frames: 0,
                expressions: &[],
                grid: Some((32, 32)),
                keyed: Some([255, 0, 255]),
                portrait: true,
                style_refs: 1,
                subject_refs: 1,
            },
        );
        for want in [
            "a moth with glass wings",
            "side view",
            "art style: ink and grey wash, one red accent.",
            "#f2c14e",
            "32×32",
            "magenta color (#ff00ff)",
            "the first attached image",
            "the last attached image shows",
            "photorealism",
            "original design",
        ] {
            assert!(p.to_lowercase().contains(&want.to_lowercase()), "missing {want:?} in {p}");
        }
    }
}
