//! A game's identity (docs/STUDIO-2.md §4.12): the SDK's theme its cards,
//! its HUD and its default font are drawn in. `studio.toml` names one of
//! the 17 (`identity = "saloon"`), describes one in words, or gives a table
//! with overrides; without one, a project is `greybox` (the starters'
//! placeholder), and under the Arcade preset 1.0's arcade look.
//!
//! The packer reads the same `identity` (crates/mb-cli/src/studio/v2.rs):
//! it bakes the theme's library fonts, and for `colors = "art"` samples the
//! key art into `packed.toml`'s `[identity]` colours, which `theme` applies.

use maimbrain::gfx2d::Font;
use maimbrain::ui::Theme;
use maimbrain::ui::identity::{ArtColors, IDENTITIES};
use maimbrain::ui::style::{HudLayout, ResultsLayout, TitleLayout};
use toml::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Identity {
    /// One of the 17, `greybox`, `arcade`, or `night` (engine 1.0's 3D cards).
    pub name: Option<String>,
    /// Words `Theme::from_identity` composes one from.
    pub describe: Option<String>,
    /// `colors = "art"`: the key art's colours (the packer samples them).
    pub art: bool,
    /// Which image is the key art (else the packer picks the biggest).
    pub art_image: Option<String>,
    pub accent: Option<u32>,
    /// Library families (or ids) for its titles, body text and numbers.
    pub title_font: Option<String>,
    pub body_font: Option<String>,
    pub number_font: Option<String>,
    pub hud: Option<HudLayout>,
    pub results: Option<ResultsLayout>,
    pub title: Option<TitleLayout>,
}

fn show(v: &Value) -> String {
    match v.as_str() {
        Some(s) => format!("{s:?}"),
        None => format!("{v:?}"),
    }
}

fn layout<T: Copy>(v: Option<&Value>, what: &str, list: &[(&str, T)]) -> Result<Option<T>, String> {
    let Some(v) = v else { return Ok(None) };
    let s = v.as_str().unwrap_or("");
    list.iter().find(|(n, _)| *n == s).map(|x| Some(x.1)).ok_or_else(|| format!("studio.toml: identity {what} = {} ({})", show(v), list.iter().map(|x| x.0).collect::<Vec<_>>().join(", ")))
}

impl Identity {
    /// `studio.toml`'s `identity`, a name or words, or a table.
    pub fn parse(v: Option<&Value>) -> Result<Identity, String> {
        let mut id = Identity::default();
        match v {
            None => {}
            Some(Value::String(s)) => id.set_name(s),
            Some(Value::Table(t)) => {
                let s = |k: &str| t.get(k).and_then(Value::as_str).map(str::to_string);
                if let Some(n) = s("name") {
                    id.set_name(&n);
                }
                if let Some(d) = s("describe") {
                    id.describe = Some(d);
                }
                match t.get("colors") {
                    None => {}
                    Some(Value::String(c)) if c == "art" => id.art = true,
                    Some(c) => return Err(format!("studio.toml: identity colors = {} (\"art\" samples the key art; the identity's own without it)", show(c))),
                }
                id.art_image = s("art");
                id.accent = match t.get("accent") {
                    None => None,
                    Some(Value::Integer(i)) if (0..=0xffffff).contains(i) => Some((*i as u32) << 8 | 0xff),
                    Some(Value::String(h)) if h.starts_with('#') && h.len() == 7 => u32::from_str_radix(&h[1..], 16).ok().map(|c| c << 8 | 0xff),
                    Some(a) => return Err(format!("studio.toml: identity accent = {} (0xRRGGBB or \"#rrggbb\")", show(a))),
                };
                (id.title_font, id.body_font, id.number_font) = (s("title_font"), s("body_font"), s("number_font"));
                id.hud = layout(t.get("hud"), "hud", &[("center", HudLayout::Center), ("corner", HudLayout::Corner), ("badge", HudLayout::Badge)])?;
                id.results = layout(
                    t.get("results"),
                    "results",
                    &[
                        ("panel", ResultsLayout::Panel),
                        ("floating", ResultsLayout::Floating),
                        ("receipt", ResultsLayout::Receipt),
                        ("scoreboard", ResultsLayout::Scoreboard),
                        ("stamp", ResultsLayout::Stamp),
                        ("editorial", ResultsLayout::Editorial),
                    ],
                )?;
                id.title = layout(
                    t.get("title"),
                    "title",
                    &[
                        ("drop", TitleLayout::Drop),
                        ("stack", TitleLayout::Stack),
                        ("banner", TitleLayout::Banner),
                        ("stamp", TitleLayout::Stamp),
                        ("typewriter", TitleLayout::Typewriter),
                        ("marquee", TitleLayout::Marquee),
                        ("arc", TitleLayout::Arc),
                        ("neon", TitleLayout::Neon),
                        ("plate", TitleLayout::Plate),
                    ],
                )?;
            }
            Some(v) => return Err(format!("studio.toml: identity = {} (a name like \"saloon\", words, or an [identity] table)", show(v))),
        }
        Ok(id)
    }

    pub fn set_name(&mut self, s: &str) {
        if IDENTITIES.contains(&s) || s == "greybox" || s == "arcade" || s == "night" {
            self.name = Some(s.to_string());
        } else {
            self.describe = Some(s.to_string());
        }
    }

    /// What it's called in the log: its name, or its words.
    pub fn label(&self) -> String {
        self.name.clone().or_else(|| self.describe.clone()).unwrap_or_else(|| "greybox".into())
    }

    /// The theme, before fonts load: the identity, its overrides, the art's colours.
    pub fn base(&self, arcade: bool) -> Theme {
        let mut t = match (&self.name, &self.describe) {
            (Some(n), _) if n == "arcade" => Theme::arcade().with_font(Font::Pixel),
            // Engine 1.0's 3D cards, which upgrade_engine keeps for a 3D game.
            (Some(n), _) if n == "night" => Theme::night(),
            (Some(n), _) => Theme::preset(n),
            (None, Some(d)) => Theme::from_identity(d),
            (None, None) if arcade => Theme::arcade().with_font(Font::Pixel),
            (None, None) => Theme::preset("greybox"),
        };
        if let Some(h) = self.hud {
            t.style.hud = h;
        }
        if let Some(r) = self.results {
            t.style.results = r;
        }
        if let Some(l) = self.title {
            t.style.title = l;
        }
        t
    }
}

/// The game's theme: the identity's base, the art's colours, then the
/// accent; its library fonts (as the packer baked them), and any font
/// overrides (`font(name)` finds a baked font's file).
pub fn theme(id: &Identity, arcade: bool, art: Option<ArtColors>, font: &dyn Fn(&str) -> Option<String>) -> Theme {
    let mut t = id.base(arcade);
    if let Some(c) = art {
        t = t.with_art(c);
    }
    if let Some(a) = id.accent {
        t = t.with_accent(a);
    }
    t = t.load_fonts();
    let f = |name: &Option<String>, current: Font| name.as_deref().and_then(font).map_or(current, |path| Font::asset(&format!("assets/{path}"), current));
    t.title_font = f(&id.title_font, t.title_font);
    t.body_font = f(&id.body_font, t.body_font);
    t.number_font = f(&id.number_font, t.number_font);
    t
}

/// The library fonts a theme was designed with, as `mb build` bakes them:
/// (file name, library id, weight).
pub fn theme_fonts(t: &Theme) -> Vec<(String, String, Option<u32>)> {
    let mut out: Vec<(String, String, Option<u32>)> = Vec::new();
    for name in [t.fonts.title, t.fonts.body, t.fonts.number] {
        if name.is_empty() || out.iter().any(|o| o.0 == name) {
            continue;
        }
        let (id, w) = match name.rsplit_once('-') {
            Some((id, w)) if w.len() == 3 && w.chars().all(|c| c.is_ascii_digit()) => (id.to_string(), w.parse().ok()),
            _ => (name.to_string(), None),
        };
        out.push((name.to_string(), id, w));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_parse_by_name_words_or_table() {
        let t = |s: &str| Identity::parse(s.parse::<toml::Table>().unwrap().get("identity")).unwrap();
        assert_eq!(t("identity = \"saloon\"").name.as_deref(), Some("saloon"));
        assert_eq!(t("identity = \"haunted carnival, crimson\"").describe.as_deref(), Some("haunted carnival, crimson"));
        let id = t("[identity]\nname = \"storybook\"\ncolors = \"art\"\naccent = 0xd4312a\nhud = \"badge\"\n");
        assert!(id.art && id.accent == Some(0xd4312aff) && id.hud == Some(HudLayout::Badge));
        assert!(Identity::parse("[identity]\nhud = \"left\"".parse::<toml::Table>().unwrap().get("identity")).unwrap_err().contains("center, corner, badge"));
        assert_eq!(Identity::default().base(false), Theme::preset("greybox"));
        assert_eq!(theme_fonts(&Theme::preset("swiss")), [("archivo-800".to_string(), "archivo".to_string(), Some(800)), ("archivo".to_string(), "archivo".to_string(), None)]);
    }
}
