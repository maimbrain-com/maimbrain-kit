//! `mb build`'s check that the game's text has glyphs: characters in the
//! text it ships (voice scripts and other `art/*.csv`, string and char
//! literals in `src/*.rs` outside comments and tests, `src/*.txt` outside
//! `#` lines) that a font it draws with lacks, so they'd draw as `?`.
//!
//! The fonts: Inter (always: the kit's default and every fallback), the
//! pixel font when the code names `Font::Pixel` or `Theme::arcade()`, and
//! each baked `assets/fonts/*.mbf` with the whole alphabet (a digits-only
//! score face isn't expected to have letters).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::fonts::{self, bake};

/// What the host fonts cover beyond printable ASCII and Latin-1 (SPEC §5.3;
/// a test checks mb-host/assets/fonts.bin, which the kit's mb doesn't ship with).
const INTER_EXTRA: &str = "‘’“”•…–—←↑→↓★♥€™";
const PIXEL_EXTRA: &str = "‘’“”•…–—←↑→↓★♥€";
/// Locations listed per character, and characters listed per warning.
const MAX_PLACES: usize = 3;
const MAX_CHARS: usize = 12;

/// The characters of Inter (both weights), or of the pixel font.
fn host_font(pixel: bool) -> BTreeSet<char> {
    let latin1 = (0x20u32..0x7f).chain(0xa0..0x100).filter(|&c| c != 0xad).filter_map(char::from_u32);
    latin1.chain(if pixel { PIXEL_EXTRA } else { INTER_EXTRA }.chars()).collect()
}

/// A font the game draws with.
struct Face {
    name: String,
    chars: BTreeSet<char>,
    /// A baked game font: its fonts.toml recipe, if it has one.
    recipe: Option<fonts::Recipe>,
    game: bool,
}

/// A character worth checking: not ASCII (every text font has that), not a
/// control or invisible formatting character (zero-width joiners, variation selectors).
fn checked(c: char) -> bool {
    !c.is_ascii() && !c.is_control() && !matches!(c as u32, 0x200b..=0x200f | 0x2060 | 0xfe00..=0xfe0f | 0xfeff)
}

/// Characters of string and char literals in Rust source, with their line
/// numbers; comments, escapes and `#[cfg(test)]` items are skipped.
pub fn rust_literals(src: &str) -> Vec<(char, usize)> {
    let s: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut depth) = (0, 1, 0i32);
    let mut out = Vec::new();
    // A `#[cfg(test)]` item: skipped from its `{` until the brace depth is back.
    let (mut test_pending, mut skip_to): (bool, Option<i32>) = (false, None);
    let at = |i: usize, t: &str| t.chars().enumerate().all(|(k, c)| s.get(i + k) == Some(&c));
    let ident = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if at(i, "//") {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if at(i, "/*") {
            let mut nest = 0;
            while i < s.len() {
                if at(i, "/*") {
                    nest += 1;
                    i += 2;
                } else if at(i, "*/") {
                    nest -= 1;
                    i += 2;
                    if nest == 0 {
                        break;
                    }
                } else {
                    line += (s[i] == '\n') as usize;
                    i += 1;
                }
            }
        } else if at(i, "#[cfg(test)]") {
            test_pending = true;
            i += 12;
        } else if c == '{' {
            if test_pending && skip_to.is_none() {
                skip_to = Some(depth);
            }
            test_pending = false;
            depth += 1;
            i += 1;
        } else if c == '}' {
            depth -= 1;
            if skip_to == Some(depth) {
                skip_to = None;
            }
            i += 1;
        } else if c == ';' {
            // `#[cfg(test)] use …;` and the like end here.
            test_pending = false;
            i += 1;
        } else if (c == 'r' || at(i, "br")) && !ident(i.checked_sub(1).and_then(|p| s.get(p))) && {
            let mut j = i + if c == 'r' { 1 } else { 2 };
            while s.get(j) == Some(&'#') {
                j += 1;
            }
            s.get(j) == Some(&'"')
        } {
            // A raw string: r#"…"# (byte strings are scanned the same way; they're ASCII).
            let start = i + if c == 'r' { 1 } else { 2 };
            let mut j = start;
            while s[j] == '#' {
                j += 1;
            }
            let hashes = j - start;
            j += 1;
            while j < s.len() && !(s[j] == '"' && (0..hashes).all(|k| s.get(j + 1 + k) == Some(&'#'))) {
                if s[j] == '\n' {
                    line += 1;
                } else if skip_to.is_none() {
                    out.push((s[j], line));
                }
                j += 1;
            }
            i = j + 1 + hashes;
        } else if c == '"' {
            i += 1;
            while i < s.len() && s[i] != '"' {
                match s[i] {
                    '\\' => {
                        // An escape: \n, \", \u{e9}… (not checked), or a line continuation.
                        i += 1;
                        if s.get(i) == Some(&'\n') {
                            line += 1;
                        }
                        if s.get(i) == Some(&'u') {
                            while i < s.len() && s[i] != '}' {
                                i += 1;
                            }
                        }
                    }
                    '\n' => line += 1,
                    ch if skip_to.is_none() => out.push((ch, line)),
                    _ => {}
                }
                i += 1;
            }
            i += 1;
        } else if c == '\'' {
            // A char literal ('é', '\n', '\u{e9}') or a lifetime ('a).
            if s.get(i + 1) == Some(&'\\') {
                i += 3;
                while i < s.len() && s[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if s.get(i + 2) == Some(&'\'') {
                if skip_to.is_none() {
                    out.push((s[i + 1], line));
                }
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out.retain(|&(c, _)| checked(c));
    out
}

/// Every checked character of the game's text → where it appears (`file:line`).
fn game_text(game: &Path) -> BTreeMap<char, Vec<String>> {
    let mut found: BTreeMap<char, Vec<String>> = BTreeMap::new();
    let mut note = |c: char, place: String| {
        let v = found.entry(c).or_default();
        if !v.contains(&place) {
            v.push(place);
        }
    };
    let mut files = Vec::new();
    let mut stack = vec![game.join("src")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                files.push(p);
            }
        }
    }
    files.extend(std::fs::read_dir(game.join("art")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "csv")));
    files.sort();
    for p in files {
        let rel = p.strip_prefix(game).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        match ext {
            "rs" if p.file_name().is_some_and(|n| n != "tests.rs") => {
                for (c, line) in rust_literals(&text) {
                    note(c, format!("{rel}:{line}"));
                }
            }
            "csv" | "txt" => {
                for (n, l) in text.lines().enumerate() {
                    if ext == "txt" && l.trim_start().starts_with('#') {
                        continue;
                    }
                    for c in l.chars().filter(|&c| checked(c)) {
                        note(c, format!("{rel}:{}", n + 1));
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// The fonts the game draws its text with.
fn faces(game: &Path, src: &str) -> Vec<Face> {
    let mut out = vec![Face { name: "Inter".into(), chars: host_font(false), recipe: None, game: false }];
    if src.contains("Font::Pixel") || src.contains("Theme::arcade()") {
        out.push(Face { name: "the pixel font".into(), chars: host_font(true), recipe: None, game: false });
    }
    let recipes = fonts::read_fonts_toml(game).unwrap_or_default();
    let mut mbfs: Vec<_> = std::fs::read_dir(game.join("assets").join("fonts")).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "mbf")).collect();
    mbfs.sort();
    for p in mbfs {
        let Some(f) = std::fs::read(&p).ok().and_then(|b| mb_format::font::MbFont::parse(&b).ok()) else { continue };
        let chars: BTreeSet<char> = f.glyphs.iter().filter_map(|g| char::from_u32(g.codepoint)).collect();
        if !('A'..='Z').chain('a'..='z').all(|c| chars.contains(&c)) {
            continue;
        }
        let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let recipe = recipes.iter().find(|r| r.name == name).cloned();
        out.push(Face { name, chars, recipe, game: true });
    }
    out
}

/// The warning for a game whose text has characters its fonts lack, if any.
pub fn warning(game: &Path, src: &str) -> Option<String> {
    check(&game_text(game), &faces(game, src))
}

fn check(text: &BTreeMap<char, Vec<String>>, faces: &[Face]) -> Option<String> {
    let mut lines = Vec::new();
    // Per game font, the characters it lacks.
    let mut lacking: BTreeMap<usize, Vec<char>> = BTreeMap::new();
    let mut host_lacks = Vec::new();
    let mut count = 0;
    for (&c, places) in text {
        let without: Vec<usize> = (0..faces.len()).filter(|&f| !faces[f].chars.contains(&c)).collect();
        if without.is_empty() {
            continue;
        }
        count += 1;
        for &f in &without {
            if faces[f].game {
                lacking.entry(f).or_default().push(c);
            } else if !host_lacks.contains(&c) {
                host_lacks.push(c);
            }
        }
        if count > MAX_CHARS {
            continue;
        }
        let more = if places.len() > MAX_PLACES { format!(" and {} more", places.len() - MAX_PLACES) } else { String::new() };
        let names: Vec<&str> = without.iter().map(|&f| faces[f].name.as_str()).collect();
        lines.push(format!("  {c} (U+{:04X}) in {}{more}: not in {}", c as u32, places[..places.len().min(MAX_PLACES)].join(", "), names.join(", ")));
    }
    if count == 0 {
        return None;
    }
    if count > MAX_CHARS {
        lines.push(format!("  and {} more characters", count - MAX_CHARS));
    }
    let mut fixes = Vec::new();
    for (f, chars) in &lacking {
        let face = &faces[*f];
        let chars: Vec<char> = chars.iter().copied().filter(|&c| !emoji(c)).collect();
        if chars.is_empty() {
            continue;
        }
        let list: String = chars.iter().collect();
        let Some(r) = &face.recipe else {
            fixes.push(format!("re-bake {} with {list} (`mb font add <game> <font> --chars latin1`, plus `+…` for any outside Latin-1)", face.name));
            continue;
        };
        let have = bake::charset(&r.chars).unwrap_or_default();
        if chars.iter().all(|c| have.iter().any(|(h, _)| h == c)) {
            fixes.push(format!(
                "{} has chars = \"{}\", which takes {list}, but its atlas was baked without them: delete assets/fonts/{}.mbf and `mb build` re-bakes it (still missing after that? the font has no glyphs for them: replace them or pick another font)",
                face.name, r.chars, face.name
            ));
            continue;
        }
        fixes.push(format!("set chars = \"{}\" for {} in fonts.toml (`mb build` re-bakes it)", wider_chars(&r.chars, &chars), face.name));
    }
    let (pictures, others): (Vec<char>, Vec<char>) = host_lacks.iter().partition(|&&c| emoji(c));
    if !others.is_empty() {
        let list: String = others.iter().collect();
        fixes.push(format!("the built-in fonts have no {list}: replace them, or draw that text in a game font baked with them (`mb font add <game> <font> --chars latin1+{list}`)"));
    }
    if !pictures.is_empty() {
        let list: String = pictures.iter().collect();
        fixes.push(format!("fonts have no emoji like {list}: replace them with words, or draw them as images"));
    }
    Some(format!(
        "{count} character{} in the game's text {} in a font it draws with, so {} as \"?\":\n{}\n{}",
        if count == 1 { "" } else { "s" },
        if count == 1 { "isn't" } else { "aren't" },
        if count == 1 { "it draws" } else { "they draw" },
        lines.join("\n"),
        fixes.iter().map(|f| format!("  fix: {f}")).collect::<Vec<_>>().join("\n")
    ))
}

/// Emoji and pictographs: no font the kit bakes has them.
fn emoji(c: char) -> bool {
    c as u32 >= 0x1f000 || (0x2600..=0x27bf).contains(&(c as u32))
}

/// A `chars` value that also takes `missing`: `ascii` widened to `latin1`,
/// then any characters still outside it appended literally.
fn wider_chars(spec: &str, missing: &[char]) -> String {
    let mut parts: Vec<String> = spec.split('+').map(|p| if p == "ascii" || p.is_empty() { "latin1".to_string() } else { p.to_string() }).collect();
    parts.dedup();
    let have = bake::charset(&parts.join("+")).unwrap_or_default();
    let rest: String = missing.iter().filter(|c| !have.iter().any(|(h, _)| h == *c)).collect();
    if !rest.is_empty() {
        parts.push(rest);
    }
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_skip_comments_escapes_and_tests() {
        let src = r##"
// Touché in a comment
/* and « here » */
fn f() -> &'static str {
    let c = 'é'; let q = '\''; let u = '\u{e9}';
    let e = "\u{e9}\n";
    let raw = r#"naïve "quoted""#;
    "Señor"
}
#[cfg(test)]
mod tests {
    fn t() { let _ = "ü"; }
}
const LAST: &str = "→";
"##;
        let got = rust_literals(src);
        assert_eq!(got, vec![('é', 5), ('ï', 7), ('ñ', 8), ('→', 14)]);
    }

    #[test]
    fn host_fonts_cover_latin1() {
        let (inter, pixel) = (host_font(false), host_font(true));
        for c in "Touché «Señor» “ÿ” — ¿ü?".chars() {
            assert!(inter.contains(&c) && pixel.contains(&c), "{c:?}");
        }
        assert!(!inter.contains(&'Ł') && !pixel.contains(&'™'));
    }

    /// The lists above are what tools/fontbake baked (in the repo; the kit has no mb-host).
    #[test]
    fn host_lists_match_the_baked_fonts() {
        let Ok(b) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../mb-host/assets/fonts.bin")) else { return };
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        let mut at = 10;
        for _ in 0..b[8] {
            let (id, n) = (b[at], u16_at(at + 2));
            at += 24;
            let got: BTreeSet<char> = (0..n).filter_map(|i| char::from_u32(u32::from_le_bytes(b[at + i * 24..at + i * 24 + 4].try_into().unwrap()))).collect();
            assert_eq!(got, host_font(id == 2), "host font {id}");
            at += n * 24;
        }
    }

    fn face(name: &str, chars: &str, recipe: Option<&str>) -> Face {
        let set = bake::charset(chars).unwrap().into_iter().map(|(c, _)| c).collect();
        let recipe = recipe.map(|spec| fonts::Recipe { name: name.into(), source: name.into(), weight: None, chars: spec.into(), em: None, bitmap: None, license: None });
        Face { name: name.into(), chars: set, recipe, game: true }
    }

    #[test]
    fn warns_once_per_char_with_places_and_a_fix() {
        let inter = Face { name: "Inter".into(), chars: host_font(false), recipe: None, game: false };
        let mut text = BTreeMap::new();
        text.insert('é', (1..=5).map(|l| format!("art/lines.csv:{l}")).collect::<Vec<_>>());
        text.insert('Ł', vec!["src/lib.rs:9".to_string()]);
        let w = check(&text, &[inter, face("eb-garamond", "ascii", Some("ascii"))]).unwrap();
        assert!(w.starts_with("2 characters in the game's text aren't in a font it draws with"), "{w}");
        assert!(w.contains("é (U+00E9) in art/lines.csv:1, art/lines.csv:2, art/lines.csv:3 and 2 more: not in eb-garamond"), "{w}");
        assert!(w.contains("Ł (U+0141) in src/lib.rs:9: not in Inter, eb-garamond"), "{w}");
        assert!(w.contains("set chars = \"latin1+Ł\" for eb-garamond in fonts.toml"), "{w}");
        assert!(w.contains("the built-in fonts have no Ł: replace them"), "{w}");
        assert_eq!(w.matches("é (U+").count(), 1);
    }

    #[test]
    fn covered_text_is_quiet() {
        let inter = Face { name: "Inter".into(), chars: host_font(false), recipe: None, game: false };
        let mut text = BTreeMap::new();
        text.insert('é', vec!["art/lines.csv:2".to_string()]);
        text.insert('—', vec!["src/lib.rs:3".to_string()]);
        assert_eq!(check(&text, &[inter, face("body", "latin1", Some("latin1"))]), None);
    }

    #[test]
    fn wider_chars_keeps_the_rest() {
        assert_eq!(wider_chars("ascii", &['é']), "latin1");
        assert_eq!(wider_chars("caps+ÄÖ", &['Ł']), "caps+ÄÖ+Ł");
        assert_eq!(wider_chars("ascii+latin1", &['é']), "latin1");
    }

    #[test]
    fn game_text_reads_scripts_sources_and_text_files() {
        let dir = std::env::temp_dir().join(format!("mb-glyphs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("art")).unwrap();
        std::fs::write(dir.join("art/lines.csv"), "character,line,text\nrocko,win,\"Touché!\"\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "// Señor\nconst A: &str = \"Ça\";\n").unwrap();
        std::fs::write(dir.join("src/questions.txt"), "# ñ in a comment\nWhat's 6 ÷ 2?\n").unwrap();
        let t = game_text(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(t.keys().collect::<String>(), "Çé÷");
        assert_eq!(t[&'é'], vec!["art/lines.csv:2"]);
        assert_eq!(t[&'Ç'], vec!["src/lib.rs:2"]);
        assert_eq!(t[&'÷'], vec!["src/questions.txt:2"]);
    }

    /// The repo's showcase games draw everything they write.
    #[test]
    fn showcase_games_are_quiet() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games");
        for g in ["gallery", "mb3d-demo"] {
            let dir = root.join(g);
            if !dir.is_dir() {
                continue; // the kit has them under examples/
            }
            let src = std::fs::read_dir(dir.join("src")).unwrap().flatten().filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect::<String>();
            assert_eq!(warning(&dir, &src), None, "{g}");
        }
    }
}
