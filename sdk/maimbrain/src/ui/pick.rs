//! What Studio's packer shares with the UI kit, in a file it compiles too
//! (crates/mb-cli's studio/v2.rs includes it by path, so this file uses
//! nothing else of the SDK): the identities' names and the words that pick
//! them, and the colours a picture gives an identity (`Theme::from_art`).

/// The identities, by name.
pub const IDENTITIES: [&str; 17] = [
    "riso", "crt", "neon", "storybook", "saloon", "swiss", "brutalist", "comic", "gothic", "bubblegum", "terminal", "field", "orbital", "gala", "notebook", "stadium", "cozy-pixel",
];

/// Words that pick each identity in `from_identity` (besides its name).
pub const KEYWORDS: [(&str, &[&str]); 17] = [
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

/// A string's FNV-1a hash (a description with no identity words still
/// picks one, from the text itself).
pub fn fnv(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// The identity a description picks (`Theme::from_identity`'s first step):
/// its name, how many of its words matched (0: picked from the text).
pub fn pick_identity(desc: &str) -> (&'static str, usize) {
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
    (best.1, best.0)
}

fn ch(c: u32, s: u32) -> f32 {
    ((c >> s) & 0xff) as f32
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let m = |s| ((ch(a, s) + (ch(b, s) - ch(a, s)) * t + 0.5) as u32) << s;
    m(24) | m(16) | m(8) | m(0)
}

fn lighten(c: u32, t: f32) -> u32 {
    (mix(c, 0xffffffff, t) & 0xffff_ff00) | (c & 0xff)
}

fn luma(c: u32) -> f32 {
    (0.299 * ch(c, 24) + 0.587 * ch(c, 16) + 0.114 * ch(c, 8)) / 255.0
}

fn hex(rgb: u32) -> u32 {
    rgb << 8 | 0xff
}

/// What [`Theme::from_art`] picks from a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtColors {
    pub bg: u32,
    pub panel: u32,
    pub accent: u32,
    pub highlight: u32,
}

/// The art's colours: a histogram of opaque pixels on a 4-bit grid (every
/// fourth pixel each way), so it's fast and the same everywhere.
pub fn art_colors(rgba: &[u8], w: u32, h: u32) -> Option<ArtColors> {
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return None;
    }
    let mut bins = vec![(0u32, [0u64; 3]); 4096];
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            let i = ((y * w + x) * 4) as usize;
            if rgba[i + 3] < 128 {
                continue;
            }
            let (r, g, b) = (rgba[i], rgba[i + 1], rgba[i + 2]);
            let k = ((r as usize >> 4) << 8) | ((g as usize >> 4) << 4) | (b as usize >> 4);
            bins[k].0 += 1;
            bins[k].1[0] += r as u64;
            bins[k].1[1] += g as u64;
            bins[k].1[2] += b as u64;
        }
    }
    // Each colour that's there, averaged within its bin, most common first.
    let mut found: Vec<(u32, u32)> = bins
        .iter()
        .enumerate()
        .filter(|(_, b)| b.0 > 0)
        .map(|(k, b)| {
            let n = b.0 as u64;
            let c = ((b.1[0] / n) as u32) << 24 | ((b.1[1] / n) as u32) << 16 | ((b.1[2] / n) as u32) << 8 | 0xff;
            let _ = k;
            (b.0, c)
        })
        .collect();
    if found.is_empty() {
        return None;
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let total: u32 = found.iter().map(|f| f.0).sum();
    // The backdrop: the darkest of the colours covering at least 5% (else the most common).
    let common: Vec<u32> = found.iter().filter(|f| f.0 * 20 >= total).map(|f| f.1).collect();
    let bg = common.iter().copied().min_by(|a, b| luma(*a).total_cmp(&luma(*b)).then(a.cmp(b))).unwrap_or(found[0].1);
    let sat = |c: u32| {
        let (r, g, b) = (((c >> 24) & 255) as f32, ((c >> 16) & 255) as f32, ((c >> 8) & 255) as f32);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        if mx <= 0.0 { 0.0 } else { (mx - mn) / mx }
    };
    // Panels: the bg a step toward the art's next common colour.
    let other = common.iter().copied().find(|c| *c != bg).unwrap_or(bg);
    let panel = mix(bg, other, 0.35);
    let panel = if (luma(panel) - luma(bg)).abs() < 0.04 { lighten(bg, 0.12) } else { panel };
    // The accent: the most vivid colour present (1% or more) that differs enough from the panels.
    let mut vivid: Vec<(f32, u32)> = found.iter().filter(|f| f.0 * 100 >= total).map(|f| (sat(f.1) * (0.4 + luma(f.1)), f.1)).collect();
    vivid.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let contrast = |c: u32| (luma(c) - luma(panel)).abs();
    let accent = vivid.iter().map(|v| v.1).find(|c| contrast(*c) > 0.25).unwrap_or_else(|| if luma(panel) < 0.5 { hex(0xffd23f) } else { hex(0x2a2a2a) });
    let highlight = vivid.iter().map(|v| v.1).find(|c| *c != accent && contrast(*c) > 0.3).unwrap_or(lighten(accent, 0.3));
    Some(ArtColors { bg, panel, accent, highlight })
}

