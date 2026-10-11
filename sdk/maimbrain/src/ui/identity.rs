//! Identities: 17 complete themes that look nothing alike (palette, shape
//! language, borders, texture, backdrop, title / results / HUD layouts,
//! motion, letter case and the library fonts they were designed with), and
//! `Theme::from_identity`, which composes a theme from a short description.
//!
//! ```ignore
//! let theme = Theme::preset("riso").load_fonts();          // after `mb font add` for its fonts
//! let theme = Theme::from_identity("haunted carnival, crimson, jittery, ticket stubs").load_fonts();
//! ```
//!
//! The table in docs/UI.md lists each identity's fonts and the `mb font add`
//! commands that bake them.

use super::color::{hex, luma, with_alpha};
use super::style::*;
use super::theme::{FontNames, Theme};
use crate::gfx2d::Font;

/// The identities, by name.
pub const IDENTITIES: [&str; 17] = [
    "riso", "crt", "neon", "storybook", "saloon", "swiss", "brutalist", "comic", "gothic", "bubblegum", "terminal", "field", "orbital", "gala", "notebook", "stadium", "cozy-pixel",
];

/// Words that pick each identity in `from_identity` (besides its name).
const KEYWORDS: [(&str, &[&str]); 17] = [
    ("riso", &["risograph", "zine", "print", "poster", "screenprint", "punk", "indie", "misregistered", "flyer"]),
    ("crt", &["arcade", "8-bit", "8bit", "phosphor", "cabinet", "coin-op", "retro", "80s", "chiptune"]),
    ("neon", &["synthwave", "outrun", "vaporwave", "retrowave", "miami", "nightclub", "laser", "club"]),
    ("storybook", &["fairytale", "fairy", "fable", "children", "book", "whimsical", "woodland", "forest", "watercolor", "gentle", "tale"]),
    ("saloon", &["western", "wild west", "cowboy", "frontier", "desert", "wanted", "sheriff", "circus", "carnival", "vaudeville", "old-timey"]),
    ("swiss", &["minimal", "minimalist", "bauhaus", "modernist", "clean", "editorial", "helvetica", "international", "magazine", "typographic"]),
    ("brutalist", &["brutal", "neo-brutalism", "neobrutalism", "raw", "blocky", "sticker", "chunky", "bold", "loud"]),
    ("comic", &["comics", "superhero", "cartoon", "pop art", "kapow", "halftone", "pulp", "action"]),
    ("gothic", &["horror", "haunted", "vampire", "medieval", "occult", "crypt", "metal", "castle", "spooky", "ghost", "creepy", "dark"]),
    ("bubblegum", &["cute", "kawaii", "candy", "pastel", "sweet", "bubbly", "playful", "kids", "toy", "sugar"]),
    ("terminal", &["hacker", "code", "console", "command line", "unix", "amber", "programmer", "computer", "ascii"]),
    ("field", &["military", "army", "tactical", "stencil", "camo", "mission", "war", "ops", "crate", "industrial", "hazard"]),
    ("orbital", &["space", "sci-fi", "scifi", "futuristic", "hud", "spaceship", "cockpit", "cyber", "tech", "orbit", "galaxy", "robot"]),
    ("gala", &["elegant", "luxury", "art deco", "deco", "gatsby", "classy", "fashion", "noir", "fancy", "opera", "casino"]),
    ("notebook", &["doodle", "sketch", "hand-drawn", "handdrawn", "handmade", "school", "paper", "scribble", "crayon", "homework", "diary"]),
    ("stadium", &["sports", "scoreboard", "football", "basketball", "soccer", "team", "jersey", "varsity", "arena", "league", "racing"]),
    ("cozy-pixel", &["pixel", "cozy", "farm", "rpg", "16-bit", "16bit", "snes", "adventure", "quest", "village", "garden"]),
];

#[allow(clippy::too_many_arguments)]
fn theme(
    bg: (u32, u32),
    panel: (u32, u32),
    border: u32,
    text: u32,
    accent: u32,
    on_accent: u32,
    gold: u32,
    outline: u32,
    shadow: u32,
    radius: f32,
    host: [Font; 3],
    fonts: FontNames,
    style: Style,
    confetti: [u32; 5],
) -> Theme {
    Theme {
        bg_top: bg.0,
        bg_bottom: bg.1,
        panel_top: panel.0,
        panel_bottom: panel.1,
        panel_border: border,
        text,
        text_dim: with_alpha(text, 0.68),
        accent,
        on_accent,
        secondary: with_alpha(text, 0.12),
        on_secondary: text,
        good: hex(0x37c871),
        bad: hex(0xf0443a),
        gold,
        outline,
        shadow,
        radius,
        outline_em: if host[0].is_bitmap() { 0.125 } else { 0.06 },
        title_font: host[0],
        body_font: host[1],
        number_font: host[2],
        confetti,
        style,
        fonts,
    }
}

const fn names(title: &'static str, body: &'static str, number: &'static str) -> FontNames {
    FontNames { title, body, number }
}

fn style(shape: Shape, border: Border, shadow: Shadow, texture: Texture, backdrop: Backdrop, backdrop_texture: Texture) -> Style {
    Style { shape, border, shadow, texture, backdrop, backdrop_texture, outlined: false, ..Style::default() }
}

impl Style {
    fn layouts(mut self, title: TitleLayout, results: ResultsLayout, hud: HudLayout) -> Style {
        self.title = title;
        self.results = results;
        self.hud = hud;
        self
    }
    fn moving(mut self, motion: Motion, case: Case, tracking: f32) -> Style {
        self.motion = motion;
        self.case = case;
        self.tracking = tracking;
        self
    }
    fn outlined(mut self) -> Style {
        self.outlined = true;
        self
    }
}

const BOLD: [Font; 3] = [Font::SansBold, Font::Sans, Font::SansBold];

impl Theme {
    /// A named identity (`IDENTITIES`); an unknown name gives `riso`.
    pub fn preset(name: &str) -> Theme {
        use Backdrop as B;
        use HudLayout as H;
        use ResultsLayout as R;
        use Shape as S;
        use Texture as X;
        use TitleLayout as T;
        match name {
            "crt" => theme(
                (hex(0x071a0d), hex(0x020a04)),
                (hex(0x0b2414), hex(0x06160c)),
                hex(0x39ff6a),
                hex(0xb6ffc4),
                hex(0x39ff6a),
                hex(0x031a09),
                hex(0xe8ff6a),
                hex(0x020a04),
                hex(0x39ff6a),
                9.0,
                [Font::Pixel; 3],
                names("press-start-2p", "vt323", "press-start-2p"),
                style(S::Pixel, Border::Line(2.0), Shadow::Glow, X::Scanlines, B::Flat, X::Scanlines).layouts(T::Marquee, R::Scoreboard, H::Corner).moving(Motion::Mechanical, Case::Upper, 0.0),
                [hex(0x39ff6a), hex(0xe8ff6a), hex(0xb6ffc4), hex(0x00c853), hex(0xffffff)],
            ),
            "neon" => theme(
                (hex(0x0a0820), hex(0x1a0b2e)),
                (0x0e0e2ad0, 0x0a0a1ed0),
                hex(0xff3fd8),
                hex(0xfff0fb),
                hex(0x00f0ff),
                hex(0x001a1c),
                hex(0xffe95c),
                hex(0x2a0030),
                hex(0xff3fd8),
                22.0,
                BOLD,
                names("monoton", "exo-2", "orbitron-700"),
                style(S::Pill, Border::Line(2.0), Shadow::Glow, X::None, B::Horizon, X::None).layouts(T::Neon, R::Floating, H::Center).moving(Motion::Floaty, Case::Upper, 0.06),
                [hex(0xff3fd8), hex(0x00f0ff), hex(0xffe95c), hex(0x8a5cff), hex(0xffffff)],
            ),
            "storybook" => theme(
                (hex(0xf7ecd2), hex(0xe9d6ae)),
                (hex(0xfff8e6), hex(0xf4e6c4)),
                hex(0x5b3a24),
                hex(0x4a2e1c),
                hex(0x3f7d4e),
                hex(0xfff8e6),
                hex(0xd98e1f),
                hex(0xfff8e6),
                0x5b3a2440,
                18.0,
                BOLD,
                names("fraunces-800", "patrick-hand", "fraunces-800"),
                style(S::Wobbly, Border::Line(2.5), Shadow::Soft, X::Grain, B::Gradient, X::Grain).layouts(T::Arc, R::Panel, H::Badge).moving(Motion::Floaty, Case::AsWritten, 0.0),
                [hex(0x3f7d4e), hex(0xd98e1f), hex(0xc8553d), hex(0x7aa5c9), hex(0xfff8e6)],
            ),
            "saloon" => theme(
                (hex(0x2b1a0f), hex(0x140b05)),
                (hex(0x5a311b), hex(0x3e200f)),
                hex(0xd9a441),
                hex(0xf6e6c6),
                hex(0xc0392b),
                hex(0xfff1d6),
                hex(0xe8b04a),
                hex(0x140b05),
                hex(0x0d0603),
                0.0,
                BOLD,
                names("rye", "zilla-slab-700", "rye"),
                style(S::Ticket, Border::Double(2.0), Shadow::Hard(4.0, 4.0), X::Grain, B::Vignette, X::Grain).layouts(T::Plate, R::Stamp, H::Badge).moving(Motion::Mechanical, Case::Upper, 0.04),
                [hex(0xc0392b), hex(0xe8b04a), hex(0xf6e6c6), hex(0x6b3a1e), hex(0x2e7d6b)],
            ),
            "swiss" => theme(
                (hex(0xf4f4f0), hex(0xf4f4f0)),
                (hex(0xffffff), hex(0xffffff)),
                hex(0x111111),
                hex(0x111111),
                hex(0xe3001b),
                hex(0xffffff),
                hex(0xe3001b),
                hex(0xffffff),
                0,
                0.0,
                BOLD,
                names("archivo-800", "archivo", "archivo-800"),
                style(S::Sharp, Border::None, Shadow::None, X::None, B::Flat, X::None).layouts(T::Stack, R::Editorial, H::Corner).moving(Motion::Snappy, Case::Lower, -0.03),
                [hex(0xe3001b), hex(0x111111), hex(0xffffff), hex(0x9a9a9a), hex(0xe3001b)],
            ),
            "brutalist" => theme(
                (hex(0xffe14d), hex(0xffe14d)),
                (hex(0xffffff), hex(0xffffff)),
                hex(0x000000),
                hex(0x000000),
                hex(0xff5c39),
                hex(0x000000),
                hex(0x2f6bff),
                hex(0xffffff),
                hex(0x000000),
                0.0,
                [Font::SansBold; 3],
                names("rubik-mono-one", "space-grotesk-700", "rubik-mono-one"),
                style(S::Sharp, Border::Line(3.0), Shadow::Hard(6.0, 6.0), X::None, B::Flat, X::Grid).layouts(T::Banner, R::Panel, H::Badge).moving(Motion::Snappy, Case::Upper, 0.0),
                [hex(0xff5c39), hex(0x2f6bff), hex(0x000000), hex(0xffffff), hex(0x00c48c)],
            ),
            "comic" => theme(
                (hex(0xfff3c4), hex(0xffd9a0)),
                (hex(0xffffff), hex(0xfff6dc)),
                hex(0x111111),
                hex(0x111111),
                hex(0xe8322b),
                hex(0xffffff),
                hex(0xffcc00),
                hex(0xffffff),
                hex(0x111111),
                0.0,
                BOLD,
                names("bangers", "comic-neue-700", "bangers"),
                style(S::Sharp, Border::Line(3.0), Shadow::Hard(4.0, 4.0), X::Halftone, B::Rays, X::Halftone).layouts(T::Stamp, R::Stamp, H::Badge).moving(Motion::Elastic, Case::Upper, 0.03).outlined(),
                [hex(0xe8322b), hex(0xffcc00), hex(0x1f6fe5), hex(0x111111), hex(0xffffff)],
            ),
            "gothic" => theme(
                (hex(0x120b0d), hex(0x050304)),
                (hex(0x24171a), hex(0x160d10)),
                hex(0x8a6d3b),
                hex(0xe9dcc0),
                hex(0xa3121f),
                hex(0xf2e6cc),
                hex(0xc9a24b),
                hex(0x050304),
                0x000000b0,
                10.0,
                BOLD,
                names("unifrakturmaguntia", "eb-garamond", "eb-garamond-700"),
                style(S::Cut, Border::Double(1.5), Shadow::Soft, X::Grain, B::Vignette, X::Grain).layouts(T::Drop, R::Editorial, H::Center).moving(Motion::Jittery, Case::AsWritten, 0.02),
                [hex(0xa3121f), hex(0xc9a24b), hex(0xe9dcc0), hex(0x4a3b5c), hex(0x000000)],
            ),
            "bubblegum" => theme(
                (hex(0xffd6e8), hex(0xc9f2e7)),
                (hex(0xffffff), hex(0xfff3f8)),
                hex(0xff8cc2),
                hex(0x5a2a4a),
                hex(0xff5fa2),
                hex(0xffffff),
                hex(0xffb700),
                hex(0xffffff),
                0xff5fa255,
                28.0,
                [Font::SansBold; 3],
                names("fredoka-700", "nunito-800", "fredoka-700"),
                style(S::Pill, Border::Line(3.0), Shadow::Soft, X::Stripes, B::Gradient, X::None).layouts(T::Drop, R::Panel, H::Center).moving(Motion::Bouncy, Case::AsWritten, 0.0).outlined(),
                [hex(0xff5fa2), hex(0xffb700), hex(0x5fd4b5), hex(0x9b8cff), hex(0xffffff)],
            ),
            "terminal" => theme(
                (hex(0x1a1206), hex(0x0d0903)),
                (hex(0x21170a), hex(0x160f05)),
                hex(0xffb000),
                hex(0xffcc66),
                hex(0xffb000),
                hex(0x1a1206),
                hex(0xfff2a8),
                hex(0x0d0903),
                0,
                0.0,
                [Font::Sans; 3],
                names("jetbrains-mono-800", "jetbrains-mono", "jetbrains-mono-800"),
                style(S::Sharp, Border::Dashed(2.0), Shadow::None, X::Scanlines, B::Flat, X::Scanlines).layouts(T::Typewriter, R::Receipt, H::Corner).moving(Motion::Mechanical, Case::Lower, 0.0),
                [hex(0xffb000), hex(0xfff2a8), hex(0xff7a00), hex(0xffcc66), hex(0xffffff)],
            ),
            "field" => theme(
                (hex(0x4a5130), hex(0x2e3320)),
                (hex(0x3b4128), hex(0x2a2f1c)),
                hex(0xe3dbb8),
                hex(0xe3dbb8),
                hex(0xf2a900),
                hex(0x1c1f12),
                hex(0xf2a900),
                hex(0x1c1f12),
                0,
                10.0,
                [Font::SansBold; 3],
                names("black-ops-one", "chakra-petch-700", "big-shoulders-stencil-display-800"),
                style(S::Cut, Border::Line(2.0), Shadow::None, X::Stripes, B::Gradient, X::Grain).layouts(T::Stamp, R::Stamp, H::Corner).moving(Motion::Mechanical, Case::Upper, 0.08),
                [hex(0xf2a900), hex(0xe3dbb8), hex(0x7a8450), hex(0x1c1f12), hex(0xc0392b)],
            ),
            "orbital" => theme(
                (hex(0x030d1a), hex(0x000306)),
                (hex(0x041222), hex(0x020a14)),
                hex(0x3ad7ff),
                hex(0xd8f6ff),
                hex(0x3ad7ff),
                hex(0x001520),
                hex(0xffb547),
                hex(0x000306),
                hex(0x3ad7ff),
                12.0,
                BOLD,
                names("orbitron-900", "chakra-petch", "tektur-700"),
                style(S::Cut, Border::Line(1.5), Shadow::Glow, X::Grid, B::Gradient, X::Grid).layouts(T::Typewriter, R::Scoreboard, H::Corner).moving(Motion::Snappy, Case::Upper, 0.12),
                [hex(0x3ad7ff), hex(0xffb547), hex(0xd8f6ff), hex(0x5a7dff), hex(0xffffff)],
            ),
            "gala" => theme(
                (hex(0x0c0c0c), hex(0x000000)),
                (hex(0x141414), hex(0x0a0a0a)),
                hex(0xc9a85a),
                hex(0xf3ead2),
                hex(0xc9a85a),
                hex(0x0c0c0c),
                hex(0xe7cf8a),
                hex(0x000000),
                0,
                0.0,
                [Font::SansBold, Font::Sans, Font::Sans],
                names("playfair-display-900", "eb-garamond", "playfair-display-700"),
                style(S::Sharp, Border::Double(1.5), Shadow::None, X::None, B::Vignette, X::None).layouts(T::Plate, R::Editorial, H::Center).moving(Motion::Floaty, Case::Upper, 0.18),
                [hex(0xc9a85a), hex(0xe7cf8a), hex(0xf3ead2), hex(0x7a6230), hex(0xffffff)],
            ),
            "notebook" => theme(
                (hex(0xfbfbf6), hex(0xf3f3ea)),
                (hex(0xfffef8), hex(0xfbf9ee)),
                hex(0x2b4aa8),
                hex(0x22336e),
                hex(0xe5484d),
                hex(0xffffff),
                hex(0xf2b705),
                hex(0xfbfbf6),
                0,
                10.0,
                BOLD,
                names("caveat-700", "patrick-hand", "caveat-700"),
                style(S::Wobbly, Border::Line(2.0), Shadow::None, X::Lines, B::Flat, X::Lines).layouts(T::Stack, R::Receipt, H::Badge).moving(Motion::Jittery, Case::AsWritten, 0.0),
                [hex(0xe5484d), hex(0x2b4aa8), hex(0xf2b705), hex(0x37c871), hex(0x22336e)],
            ),
            "stadium" => theme(
                (hex(0x0d2a4a), hex(0x061528)),
                (hex(0x13355c), hex(0x0b2240)),
                hex(0xffffff),
                hex(0xffffff),
                hex(0xff7a00),
                hex(0xffffff),
                hex(0xffd400),
                hex(0x061528),
                hex(0x020a14),
                6.0,
                [Font::SansBold; 3],
                names("big-shoulders-display-900", "barlow-condensed-700", "jersey-10"),
                style(S::Sharp, Border::Line(3.0), Shadow::Hard(0.0, 5.0), X::Stripes, B::Gradient, X::Stripes).layouts(T::Banner, R::Scoreboard, H::Badge).moving(Motion::Snappy, Case::Upper, 0.04),
                [hex(0xff7a00), hex(0xffd400), hex(0xffffff), hex(0x2f6bff), hex(0x0d2a4a)],
            ),
            "cozy-pixel" => theme(
                (hex(0x2d3b2a), hex(0x1b2419)),
                (hex(0x3e5039), hex(0x2c3a29)),
                hex(0xf2e8cf),
                hex(0xf2e8cf),
                hex(0xe07a3f),
                hex(0x2d1a10),
                hex(0xffd166),
                hex(0x151c13),
                hex(0x151c13),
                9.0,
                [Font::Pixel; 3],
                names("pixelify-sans-700", "pixelify-sans", "pixelify-sans-700"),
                style(S::Pixel, Border::Line(3.0), Shadow::Hard(4.0, 4.0), X::None, B::Gradient, X::None).layouts(T::Drop, R::Panel, H::Badge).moving(Motion::Mechanical, Case::AsWritten, 0.0).outlined(),
                [hex(0xe07a3f), hex(0xffd166), hex(0x8ac16b), hex(0xf2e8cf), hex(0x6bb6c9)],
            ),
            // "riso" and anything unknown.
            _ => theme(
                (hex(0xf2ead8), hex(0xefe4cc)),
                (hex(0xfbf6ea), hex(0xf6eedc)),
                hex(0x22223b),
                hex(0x22223b),
                hex(0xff4f8b),
                hex(0xffffff),
                hex(0x00a7a0),
                hex(0xfbf6ea),
                hex(0x00a7a0),
                0.0,
                BOLD,
                names("bricolage-grotesque-800", "space-mono", "bricolage-grotesque-800"),
                style(S::Sharp, Border::Line(3.0), Shadow::Hard(5.0, 5.0), X::Grain, B::Flat, X::Grain).layouts(T::Stack, R::Receipt, H::Corner).moving(Motion::Snappy, Case::Upper, -0.01),
                [hex(0xff4f8b), hex(0x00a7a0), hex(0x22223b), hex(0xffd23f), hex(0xfbf6ea)],
            ),
        }
    }

    /// Every identity with its name.
    pub fn identities() -> Vec<(&'static str, Theme)> {
        IDENTITIES.iter().map(|n| (*n, Theme::preset(n))).collect()
    }

    /// Composes a theme from a short description of the game's identity, e.g.
    /// `"risograph zine, mustard and teal, wobbly, floaty"`. The words pick the
    /// closest identity (an era, medium or genre: "western", "space",
    /// "notebook", "synthwave"…), then any of these override parts of it:
    /// - colors (first = accent, second = highlight): red, crimson, orange,
    ///   mustard, yellow, gold, lime, green, olive, teal, cyan, blue, navy,
    ///   purple, violet, pink, magenta, brown, black, white, cream
    /// - shape: sharp, rounded, pill, chamfered, wobbly / hand-drawn, pixelated, bevel, ticket
    ///   ("pixel" alone picks the cozy-pixel identity)
    /// - motion: snappy, floaty, bouncy, mechanical, jittery, elastic
    /// - texture: grain, scanlines, dots (halftone), stripes, grid, lined, flat
    /// - shadow: hard (offset print shadow), glow, flat
    /// - layouts: receipt, scoreboard, stamp, typewriter, marquee, banner, neon, arc, plate, editorial
    ///
    /// With no recognisable words it still picks an identity, from the text
    /// itself, so two different descriptions rarely land on the same look.
    ///
    /// It logs what it understood, once, at info (`mb shot` prints it, as
    /// does the preview's console): `identity "wild west saloon, crimson,
    /// wobbly": saloon (wild west, saloon); crimson (accent), wobbly (shape)`.
    /// [`Theme::explain_identity`] returns the same text.
    pub fn from_identity(desc: &str) -> Theme {
        let (t, notes) = Theme::compose_identity(desc);
        crate::sys::log(crate::sys::Level::Info, &format!("identity {desc:?}: {notes}"));
        t
    }

    /// What [`Theme::from_identity`] makes of `desc`: the identity it picked
    /// and the words that chose it, then each word that changed a part of it.
    pub fn explain_identity(desc: &str) -> String {
        Theme::compose_identity(desc).1
    }

    fn compose_identity(desc: &str) -> (Theme, String) {
        let d = desc.to_lowercase();
        let words: Vec<&str> = d.split(|c: char| !(c.is_alphanumeric() || c == '-')).filter(|w| !w.is_empty()).collect();
        let has = |k: &str| if k.contains(' ') { d.contains(k) } else { words.contains(&k) };
        let mut best = (0usize, IDENTITIES[(fnv(&d) % IDENTITIES.len() as u32) as usize]);
        for (name, keys) in KEYWORDS {
            let score = 3 * has(name) as usize + keys.iter().filter(|k| has(k)).count();
            if score > best.0 {
                best = (score, name);
            }
        }
        let mut t = Theme::preset(best.1);
        let mut notes = match KEYWORDS.iter().find(|(n, _)| *n == best.1) {
            Some((name, keys)) if best.0 > 0 => {
                let hits: Vec<&str> = std::iter::once(*name).chain(keys.iter().copied()).filter(|k| has(k)).collect();
                format!("{} ({})", best.1, hits.join(", "))
            }
            _ => format!("{} (no identity words: picked from the text)", best.1),
        };
        let mut changed: Vec<String> = Vec::new();
        // Colors.
        const COLORS: [(&str, u32); 21] = [
            ("red", 0xe5484d), ("crimson", 0xa3121f), ("orange", 0xff7a1a), ("mustard", 0xe0a526), ("yellow", 0xffd23f), ("gold", 0xd4af37),
            ("lime", 0xa3e635), ("green", 0x2fbf71), ("olive", 0x7a8450), ("teal", 0x0fb5ae), ("cyan", 0x3ad7ff), ("blue", 0x2f6bff),
            ("navy", 0x1b2a4a), ("purple", 0x7c3aed), ("violet", 0x8b5cf6), ("pink", 0xff4f8b), ("magenta", 0xff3fd8), ("brown", 0x7a4a2a),
            ("black", 0x111111), ("white", 0xf8f8f8), ("cream", 0xf4ecd8),
        ];
        let picked: Vec<u32> = words.iter().filter_map(|w| COLORS.iter().find(|(n, _)| n == w).map(|(_, c)| hex(*c))).collect();
        let color_words: Vec<&str> = words.iter().copied().filter(|w| COLORS.iter().any(|(n, _)| n == w)).collect();
        if let Some(&a) = picked.first() {
            t = t.with_accent(a);
            changed.push(format!("{} (accent)", color_words[0]));
        }
        if let Some(&g) = picked.get(1) {
            changed.push(format!("{} (highlight)", color_words[1]));
            t.gold = g;
            if luma(t.bg_top) > 0.55 && luma(g) > 0.75 {
                t.gold = super::color::darken(g, 0.35);
            }
        }
        let st = &mut t.style;
        for w in &words {
            let before = *st;
            match *w {
                "sharp" | "square" | "angular" => st.shape = Shape::Sharp,
                "rounded" | "round" | "soft" => st.shape = Shape::Rounded,
                "pill" | "pills" | "capsule" => st.shape = Shape::Pill,
                "chamfer" | "chamfered" | "cut" => st.shape = Shape::Cut,
                "wobbly" | "hand-drawn" | "handdrawn" | "sketchy" | "scribbly" => st.shape = Shape::Wobbly,
                "pixelated" | "stepped" => st.shape = Shape::Pixel,
                "bevel" | "beveled" | "bevelled" | "win95" => st.shape = Shape::Bevel,
                "ticket" | "tickets" | "coupon" | "stub" | "stubs" => st.shape = Shape::Ticket,
                "snappy" | "quick" | "crisp" | "fast" => st.motion = Motion::Snappy,
                "floaty" | "dreamy" | "slow" | "calm" | "drifting" => st.motion = Motion::Floaty,
                "bouncy" | "springy" => st.motion = Motion::Bouncy,
                "mechanical" | "robotic" | "clockwork" | "ticking" => st.motion = Motion::Mechanical,
                "jittery" | "glitchy" | "glitch" | "nervous" | "twitchy" | "flicker" | "flickering" => st.motion = Motion::Jittery,
                "elastic" | "rubbery" | "stretchy" | "wobbling" => st.motion = Motion::Elastic,
                "grain" | "grainy" | "noisy" => st.texture = Texture::Grain,
                "scanlines" | "scanline" | "crt" => st.texture = Texture::Scanlines,
                "dots" | "dotted" => st.texture = Texture::Halftone,
                "stripes" | "striped" => st.texture = Texture::Stripes,
                "grid" | "graph" => st.texture = Texture::Grid,
                "lined" | "ruled" => st.texture = Texture::Lines,
                "flat" => {
                    st.texture = Texture::None;
                    st.shadow = Shadow::None;
                }
                "hard" | "offset" => st.shadow = Shadow::Hard(5.0, 5.0),
                "glow" | "glowing" => st.shadow = Shadow::Glow,
                "receipt" => st.results = ResultsLayout::Receipt,
                "stamp" | "stamped" => {
                    st.results = ResultsLayout::Stamp;
                    st.title = TitleLayout::Stamp;
                }
                "typewriter" | "typed" => st.title = TitleLayout::Typewriter,
                "marquee" | "bulbs" => st.title = TitleLayout::Marquee,
                "banner" => st.title = TitleLayout::Banner,
                "arc" | "arched" => st.title = TitleLayout::Arc,
                "plate" | "plaque" | "engraved" => st.title = TitleLayout::Plate,
                "editorial" => st.results = ResultsLayout::Editorial,
                "uppercase" | "caps" => st.case = Case::Upper,
                "lowercase" => st.case = Case::Lower,
                _ => {}
            }
            let parts = [
                (before.shape != st.shape, "shape"),
                (before.motion != st.motion, "motion"),
                (before.texture != st.texture, "texture"),
                (before.shadow != st.shadow, "shadow"),
                (before.title != st.title, "title layout"),
                (before.results != st.results, "results layout"),
                (before.case != st.case, "letter case"),
            ];
            let parts: Vec<&str> = parts.iter().filter(|p| p.0).map(|p| p.1).collect();
            if !parts.is_empty() {
                changed.push(format!("{w} ({})", parts.join(", ")));
            }
        }
        // Words that can't be one-word matches above; claimed only when they change something.
        if has("scoreboard") && t.style.results != ResultsLayout::Scoreboard {
            t.style.results = ResultsLayout::Scoreboard;
            changed.push("scoreboard (results layout)".into());
        }
        if has("neon") {
            let parts: Vec<&str> = [(t.style.title != TitleLayout::Neon, "title layout"), (t.style.shadow != Shadow::Glow, "shadow")].iter().filter(|p| p.0).map(|p| p.1).collect();
            t.style.title = TitleLayout::Neon;
            t.style.shadow = Shadow::Glow;
            if !parts.is_empty() {
                changed.push(format!("neon ({})", parts.join(", ")));
            }
        }
        notes += &if changed.is_empty() { "; no color, shape, motion, texture, shadow or layout words".to_string() } else { format!("; {}", changed.join(", ")) };
        (t, notes)
    }
}

fn fnv(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_distinct() {
        let all = Theme::identities();
        assert_eq!(all.len(), 17);
        for (i, (a, ta)) in all.iter().enumerate() {
            for (b, tb) in &all[i + 1..] {
                // Two identities never share palette, shape and layouts all at once.
                let same = ta.bg_top == tb.bg_top || (ta.style.shape == tb.style.shape && ta.style.title == tb.style.title && ta.style.results == tb.style.results && ta.style.motion == tb.style.motion);
                assert!(!same, "{a} and {b} are too alike");
                assert_ne!(ta.fonts, tb.fonts, "{a} and {b} use the same fonts");
            }
        }
    }

    #[test]
    fn descriptions_pick_and_modify() {
        let t = Theme::from_identity("Wild west saloon, crimson, wobbly, floaty");
        assert_eq!(t.fonts.title, "rye");
        assert_eq!(t.style.shape, Shape::Wobbly);
        assert_eq!(t.style.motion, Motion::Floaty);
        assert_eq!(t.accent, hex(0xa3121f));
        assert_eq!(Theme::from_identity("a calm synthwave drive").style.title, TitleLayout::Neon);
        assert_eq!(Theme::from_identity("space cockpit HUD with a scoreboard").style.results, ResultsLayout::Scoreboard);
        // Nothing recognisable still gives a full identity, picked from the text.
        let x = Theme::from_identity("qwzx");
        assert!(IDENTITIES.iter().any(|n| Theme::preset(n).fonts == x.fonts));
        assert_eq!(Theme::preset("nope"), Theme::preset("riso"));
    }

    #[test]
    fn explains_what_it_matched() {
        assert_eq!(
            Theme::explain_identity("Wild west saloon, crimson, teal, wobbly, floaty"),
            "saloon (saloon, wild west); crimson (accent), teal (highlight), wobbly (shape), floaty (motion)"
        );
        let none = Theme::explain_identity("qwzx");
        assert!(none.contains("no identity words") && none.ends_with("no color, shape, motion, texture, shadow or layout words"), "{none}");
        // A word that leaves a part as it was isn't claimed.
        assert!(!Theme::explain_identity("riso, grain").contains("grain ("), "riso is already grainy");
        assert_eq!(Theme::explain_identity("neon"), "neon (neon); no color, shape, motion, texture, shadow or layout words", "the neon preset is already neon");
        assert!(!Theme::explain_identity("stadium scoreboard").contains("scoreboard ("), "stadium already has the scoreboard");
        assert!(Theme::explain_identity("riso with a neon sign").contains("; neon (title layout, shadow)"));
    }
}
