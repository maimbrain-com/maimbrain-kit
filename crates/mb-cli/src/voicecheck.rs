//! Lines that will be shown but not heard, found before a phone does
//! (`mb build`, `mb voice list`, `mb voice script`): a character with lines
//! in a voice script but no voice files (left out of the cast, or never
//! spoken), a `VoiceLine::load` whose `.ogg` isn't there, a
//! `VoiceLine::subtitle` of a line that has a recording, and a character's
//! recordings that nothing in the code loads. At runtime the SDK logs the
//! same once per line or speaker (`VoiceLine`, `Dialogue`). Also a length
//! table (`src/vo_lengths.rs`) out of date with the recordings: narration
//! timed from it would overlap the next line or leave a gap.

use std::collections::BTreeMap;
use std::path::Path;

use crate::generate::voice::{LENGTHS_FILE, line_lengths, load_voices, parse_lengths, parse_script, subtitle};

/// Each problem as a warning line (empty when every line is heard).
pub fn warnings(game: &Path, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let ogg = |who: &str, id: &str| game.join(format!("assets/vo/{who}/{id}.ogg"));
    let cast = load_voices(game).unwrap_or_default();
    // Voiced lines' subtitles → their base path, from the scripts and the sidecars.
    let mut voiced: BTreeMap<String, String> = BTreeMap::new();

    for script in scripts(game) {
        let Ok(text) = std::fs::read_to_string(&script) else { continue };
        let Ok(lines) = parse_script(&script, &text) else { continue };
        let name = script.strip_prefix(game).unwrap_or(&script).display().to_string().replace('\\', "/");
        // Per character: (missing ids, lines), in the script's order.
        let mut by: Vec<(String, Vec<String>, usize)> = Vec::new();
        for l in &lines {
            let i = by.iter().position(|b| b.0 == l.character).unwrap_or_else(|| {
                by.push((l.character.clone(), Vec::new(), 0));
                by.len() - 1
            });
            by[i].2 += 1;
            if ogg(&l.character, &l.id).is_file() {
                voiced.insert(subtitle(&l.text), format!("assets/vo/{}/{}", l.character, l.id));
            } else {
                by[i].1.push(l.id.clone());
            }
        }
        for (who, missing, total) in by.into_iter().filter(|b| !b.1.is_empty()) {
            let speak = format!("mb voice script {} {name}", game.display());
            out.push(if missing.len() == total && !cast.contains_key(&who) {
                format!(
                    "{name}: {who} has {total} line(s) but no voice: {who} isn't cast in art/voices.toml, so nothing speaks them and they'll play as subtitles only. Cast {who} (`mb voice design {} {who} \"…\"`, or `mb voice cast … --premade <name>`), then `{speak}`; or take {who}'s lines out of the script if {who} is silent on purpose",
                    game.display()
                )
            } else {
                let few: Vec<&str> = missing.iter().take(6).map(String::as_str).collect();
                let more = if missing.len() > few.len() { format!(" and {} more", missing.len() - few.len()) } else { String::new() };
                format!("{name}: {} of {who}'s {total} line(s) have no voice file yet ({}{more}): they'll play as subtitles only until `{speak}`", missing.len(), few.join(", "))
            });
        }
    }
    // Lines spoken one by one (`mb voice say`) carry their subtitle in the sidecar.
    for (who, dir) in vo_dirs(game) {
        for f in oggs(&dir) {
            let id = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
            if let Ok(sc) = crate::generate::read_sidecar(&f)
                && let Some(sub) = sc.result["subtitle"].as_str()
            {
                voiced.entry(subtitle(sub)).or_insert_with(|| format!("assets/vo/{who}/{id}"));
            }
        }
    }

    for base in calls(src, "VoiceLine::load(") {
        if !base.contains('{') && !game.join(format!("{base}.ogg")).is_file() {
            out.push(format!("src: VoiceLine::load(\"{base}\"), but there's no {base}.ogg: the line plays as subtitles only (speak it with `mb voice say` or `mb voice script`)"));
        }
    }
    for text in calls(src, "VoiceLine::subtitle(") {
        if let Some(base) = voiced.get(&subtitle(&text)) {
            out.push(format!("src: VoiceLine::subtitle(\"{text}\") is a line with a recording ({base}.ogg), shown without it: load it with VoiceLine::load(\"{base}\") so it's heard"));
        }
    }
    out.extend(stale_lengths(game, src));
    // A pattern (`format!("assets/vo/{}/{}", who, id)`) or a script loads any character.
    let any = src.contains("assets/vo/{") || src.contains("load_script(");
    if !any {
        for (who, dir) in vo_dirs(game) {
            let n = oggs(&dir).len();
            if n > 0 && !src.contains(&format!("assets/vo/{who}/")) {
                out.push(format!("assets/vo/{who}/ has {n} recorded line(s), but nothing in src loads them (no \"assets/vo/{who}/…\"): {who} is never heard"));
            }
        }
    }
    out
}

/// The length table (`src/vo_lengths.rs`), when the game has one or uses
/// one, against the recordings' `.lips.txt` lengths.
fn stale_lengths(game: &Path, src: &str) -> Option<String> {
    let path = game.join(LENGTHS_FILE);
    let fix = format!("`mb voice lengths {}`", game.display());
    let Ok(text) = std::fs::read_to_string(&path) else {
        return src.contains("vo_lengths::").then(|| format!("src uses vo_lengths, but there's no {LENGTHS_FILE}: {fix} writes it"));
    };
    let (want, have) = (line_lengths(game), parse_lengths(&text));
    let find = |v: &[(String, f32)], k: &str| v.iter().find(|(key, _)| key == k).map(|e| e.1);
    let changed: Vec<&str> = want.iter().filter(|(k, d)| find(&have, k).is_some_and(|h| (h - d).abs() > 0.002)).map(|(k, _)| k.as_str()).collect();
    let missing: Vec<&str> = want.iter().filter(|(k, _)| find(&have, k).is_none()).map(|(k, _)| k.as_str()).collect();
    let gone: Vec<&str> = have.iter().filter(|(k, _)| find(&want, k).is_none()).map(|(k, _)| k.as_str()).collect();
    if changed.is_empty() && missing.is_empty() && gone.is_empty() {
        return None;
    }
    let mut what = Vec::new();
    for (n, list, label) in [(changed.len(), &changed, "re-recorded at another length"), (missing.len(), &missing, "not in it"), (gone.len(), &gone, "no longer recorded")] {
        if n > 0 {
            let few: Vec<&str> = list.iter().take(4).copied().collect();
            let more = if n > few.len() { format!(" and {} more", n - few.len()) } else { String::new() };
            what.push(format!("{n} {label} ({}{more})", few.join(", ")));
        }
    }
    Some(format!("{LENGTHS_FILE} is out of date with assets/vo: {}. Narration timed from it would overlap the next line or leave a gap; {fix} rewrites it", what.join("; ")))
}

/// The voice scripts: art/*.csv and art/*.toml with `[[line]]` tables.
fn scripts(game: &Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(game.join("art"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| match p.extension().and_then(|x| x.to_str()) {
            Some("csv") => true,
            Some("toml") => std::fs::read_to_string(p).is_ok_and(|t| t.contains("[[line]]")),
            _ => false,
        })
        .collect();
    out.sort();
    out
}

fn vo_dirs(game: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out: Vec<_> = std::fs::read_dir(game.join("assets/vo")).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).collect();
    out.sort();
    out
}

fn oggs(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ogg")).collect();
    out.sort();
    out
}

/// The string literal each `call` (like `VoiceLine::load(`) starts with,
/// when it does (`&format!(…)` and variables are skipped).
fn calls(src: &str, call: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in src.match_indices(call) {
        let rest = src[i + call.len()..].trim_start();
        let rest = rest.strip_prefix('&').unwrap_or(rest);
        let Some(body) = rest.strip_prefix('"') else { continue };
        let mut lit = String::new();
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => lit.extend(chars.next()),
                '"' => {
                    out.push(lit);
                    break;
                }
                _ => lit.push(c),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_that_wont_be_heard_are_named() {
        let game = std::env::temp_dir().join(format!("mb-voicecheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        for d in ["art", "assets/vo/girl", "assets/vo/ghost"] {
            std::fs::create_dir_all(game.join(d)).unwrap();
        }
        std::fs::write(game.join("art/lines.csv"), "character,line,text\ngirl,l1,Mum?\nmother,l2,I'm here.\nmother,l3,[softly] Always.\ngirl,l4,Where are you?\n").unwrap();
        std::fs::write(game.join("art/voices.toml"), "[girl]\nprovider = \"mock\"\ndescription = \"a girl\"\ndesign_model = \"mock\"\nseed = 1\ndate = \"2026-10-13\"\n").unwrap();
        std::fs::write(game.join("assets/vo/girl/l1.ogg"), b"OggS").unwrap();
        std::fs::write(game.join("assets/vo/ghost/boo.ogg"), b"OggS").unwrap();
        let src = r#"
            let a = VoiceLine::load("assets/vo/girl/l1");
            let b = VoiceLine::load("assets/vo/girl/l9");
            let c = VoiceLine::load(&format!("assets/vo/girl/{}", id));
            let d = VoiceLine::subtitle("Mum?");
            let e = VoiceLine::subtitle("Some other words.");
        "#;
        let w = warnings(&game, src);
        assert!(w.iter().any(|w| w.starts_with("art/lines.csv: mother has 2 line(s) but no voice: mother isn't cast")), "{w:#?}");
        assert!(w.iter().any(|w| w.starts_with("art/lines.csv: 1 of girl's 2 line(s) have no voice file yet (l4)")), "{w:#?}");
        assert!(w.iter().any(|w| w.contains("VoiceLine::load(\"assets/vo/girl/l9\"), but there's no assets/vo/girl/l9.ogg")), "{w:#?}");
        assert!(w.iter().any(|w| w.contains("VoiceLine::subtitle(\"Mum?\") is a line with a recording (assets/vo/girl/l1.ogg)")), "{w:#?}");
        assert!(w.iter().any(|w| w.starts_with("assets/vo/ghost/ has 1 recorded line(s), but nothing in src loads them")), "{w:#?}");
        assert_eq!(w.len(), 5, "{w:#?}");
        // The length table: missing while src uses it, then current, then stale.
        std::fs::write(game.join("assets/vo/girl/l1.lips.txt"), "mblips 1\ndur 1.250\ntext Mum?\n").unwrap();
        let uses = "talk.narration.lengths = vo_lengths::ALL; VoiceLine::load(\"assets/vo/girl/l1\");";
        std::fs::create_dir_all(game.join("src")).unwrap();
        assert!(warnings(&game, uses).iter().any(|w| w.starts_with("src uses vo_lengths, but there's no src/vo_lengths.rs")));
        crate::generate::voice::refresh_lengths(&game).unwrap();
        assert!(!warnings(&game, uses).iter().any(|w| w.contains("vo_lengths")), "{:#?}", warnings(&game, uses));
        std::fs::write(game.join("assets/vo/girl/l1.lips.txt"), "mblips 1\ndur 2.5\ntext Mum?\n").unwrap();
        let w = warnings(&game, uses);
        assert!(w.iter().any(|w| w.starts_with("src/vo_lengths.rs is out of date with assets/vo: 1 re-recorded at another length (girl/l1)")), "{w:#?}");
        crate::generate::voice::refresh_lengths(&game).unwrap();
        std::fs::remove_dir_all(game.join("src")).unwrap();
        // A game that loads every line of its script by pattern, all spoken: quiet.
        std::fs::write(game.join("art/lines.csv"), "character,line,text\ngirl,l1,Mum?\n").unwrap();
        std::fs::remove_dir_all(game.join("assets/vo/ghost")).unwrap();
        assert!(warnings(&game, "talk.barks.load_script(include_str!(\"../art/lines.csv\"));").is_empty());
        let _ = std::fs::remove_dir_all(&game);
    }
}
