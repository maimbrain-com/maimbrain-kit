//! `mb upgrade`'s checks of a game's own code and manifest: plain source
//! greps and manifest reads that turn the upgrade notes into advice for this
//! game, each a suggestion with the API to use. They read `src/` (not its
//! tests, except where a check is about the bot), so they're quick, and
//! they're hints, not proofs: a match is something to look at.

use std::path::Path;

use mb_format::Manifest;
use serde::Serialize;

use super::notes::Group;

/// Something `--apply` can do for a finding, as it is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// `daily = true` on this `[[scores]]` board in manifest.toml.
    DailyBoard(u32),
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    /// A stable name for the check (`daily-board`, `walker`, …).
    pub id: &'static str,
    pub group: Group,
    pub title: String,
    pub advice: String,
    /// Where in the game it was seen: `src/lib.rs:80`, `manifest.toml`.
    pub at: Vec<String>,
    /// Adopting it changes how the game plays or looks: ask the owner first.
    pub ask_owner: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// `--apply` made the fix.
    pub applied: bool,
}

impl Finding {
    fn new(id: &'static str, group: Group, title: impl Into<String>, advice: impl Into<String>, at: Vec<String>) -> Finding {
        Finding { id, group, title: title.into(), advice: advice.into(), at, ask_owner: false, fix: None, applied: false }
    }

    fn ask(mut self) -> Finding {
        self.ask_owner = true;
        self
    }
}

/// A game's Rust sources: `src/**/*.rs`, each line with whether it's game
/// code (not a comment, not in `tests.rs`, `tests/` or after a `#[cfg(test)]`).
pub struct Sources {
    lines: Vec<Line>,
}

struct Line {
    file: String,
    n: usize,
    text: String,
    code: bool,
}

impl Sources {
    pub fn read(dir: &Path) -> Sources {
        let mut files = Vec::new();
        let mut stack = vec![dir.join("src")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let rel = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                    files.push((rel, std::fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
        files.sort();
        Sources::from_files(&files.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect::<Vec<_>>())
    }

    pub fn from_files(files: &[(&str, &str)]) -> Sources {
        let mut lines = Vec::new();
        // Files that are test modules: `#[cfg(test)] mod bot;` makes bot.rs one.
        let mut test_mods = Vec::new();
        for (file, text) in files {
            let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
            let mut after_cfg = false;
            for l in text.lines().map(str::trim) {
                if l.starts_with("#[cfg(test)]") {
                    after_cfg = true;
                } else if after_cfg && !l.starts_with("#[") {
                    let name = l.trim_start_matches("pub ").trim_start_matches("pub(crate) ").strip_prefix("mod ").and_then(|r| r.strip_suffix(';'));
                    if let Some(n) = name {
                        test_mods.push(format!("{dir}/{}.rs", n.trim()));
                        test_mods.push(format!("{dir}/{}/mod.rs", n.trim()));
                    }
                    after_cfg = false;
                }
            }
        }
        for (file, text) in files {
            let test_file = file.ends_with("/tests.rs") || file.contains("/tests/") || file.ends_with("_tests.rs") || test_mods.iter().any(|m| m == file);
            // The item after a `#[cfg(test)]` is test code: one line (`mod bot;`),
            // or a block to its closing brace (`mod tests { … }`).
            let (mut pending, mut depth) = (false, 0i64);
            for (i, l) in text.lines().enumerate() {
                let t = l.trim_start();
                let mut in_tests = test_file || pending || depth > 0;
                if t.starts_with("#[cfg(test)]") {
                    (pending, in_tests) = (true, true);
                } else if depth > 0 {
                    depth += braces(l);
                } else if pending && !t.starts_with("#[") && !t.starts_with("//") && !t.is_empty() {
                    pending = false;
                    depth = braces(l).max(0);
                }
                let code = !in_tests && !t.starts_with("//") && !t.is_empty();
                lines.push(Line { file: file.to_string(), n: i + 1, text: l.to_string(), code });
            }
        }
        Sources { lines }
    }

    fn code(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(|l| l.code)
    }

    /// Game code mentions `s`.
    fn has(&self, s: &str) -> bool {
        self.code().any(|l| l.text.contains(s))
    }

    /// Anything, tests and comments included, mentions `s`.
    fn any_has(&self, s: &str) -> bool {
        self.lines.iter().any(|l| l.text.contains(s))
    }

    /// The game-code lines `f` accepts, as `file:line`.
    fn find(&self, f: impl Fn(&str) -> bool) -> Vec<String> {
        self.code().filter(|l| f(&l.text)).map(|l| format!("{}:{}", l.file, l.n)).collect()
    }

    /// The value of `const NAME: <int> = N;` anywhere in the sources.
    fn const_u32(&self, name: &str) -> Option<u32> {
        self.lines.iter().find_map(|l| {
            let rest = l.text.trim_start().trim_start_matches("pub ").trim_start_matches("pub(crate) ").strip_prefix("const ")?.strip_prefix(name)?;
            let value = rest.trim_start().strip_prefix(':')?.split_once('=')?.1;
            value.trim().trim_end_matches(';').trim().trim_end_matches("u32").trim_end_matches('_').parse().ok()
        })
    }
}

/// `{` less `}` in a line of code (outside strings and chars, roughly).
fn braces(line: &str) -> i64 {
    let code = line.split("//").next().unwrap_or("");
    let mut in_str = false;
    let mut n = 0;
    let mut prev = ' ';
    for c in code.chars() {
        match c {
            '"' if prev != '\\' => in_str = !in_str,
            '{' if !in_str => n += 1,
            '}' if !in_str => n -= 1,
            _ => {}
        }
        prev = c;
    }
    n
}

/// At most `n` places, then how many more.
fn first(mut at: Vec<String>, n: usize) -> Vec<String> {
    if at.len() > n {
        let more = at.len() - n;
        at.truncate(n);
        at.push(format!("({more} more)"));
    }
    at
}

/// The boards `Rounds::daily(..)` is given in game code: (board, where).
/// A constant argument is looked up; one it can't resolve is left out.
fn daily_boards(src: &Sources) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for l in src.code() {
        let mut rest = l.text.as_str();
        while let Some(i) = rest.find(".daily(") {
            rest = &rest[i + ".daily(".len()..];
            let arg = rest.split(')').next().unwrap_or("").trim();
            let ident = arg.rsplit("::").next().unwrap_or(arg);
            let board = if arg.is_empty() { None } else { arg.trim_end_matches("u32").parse().ok().or_else(|| src.const_u32(ident)) };
            if let Some(b) = board {
                out.push((b, format!("{}:{}", l.file, l.n)));
            }
        }
    }
    out
}

/// Every check, in the order they're reported (by group, then as listed).
pub fn run(dir: &Path, manifest: Option<&Manifest>, src: &Sources) -> Vec<Finding> {
    let mut out = Vec::new();

    // Rounds::daily(n) and the manifest's daily board.
    if let Some(m) = manifest {
        for (board, at) in daily_boards(src) {
            let entry = m.scores.iter().find(|s| s.board == board);
            let (title, advice, fix) = match entry {
                Some(s) if s.daily => continue,
                Some(_) if board == 0 => (
                    format!("Rounds::daily({board}) is board 0, which can't be a daily board"),
                    "Give the daily mode its own `[[scores]]` board (1–15) with `daily = true` and pass that to `Rounds::daily`.".to_string(),
                    None,
                ),
                Some(_) => (
                    format!("Rounds::daily({board}), but board {board} in manifest.toml isn't `daily = true`"),
                    format!("Add `daily = true` to the `[[scores]]` board {board} (`mb upgrade --apply` does it): conformance fails without it, and with it the board's standings start over each UTC day."),
                    Some(Fix::DailyBoard(board)),
                ),
                None => (
                    format!("Rounds::daily({board}), but manifest.toml has no board {board}"),
                    format!("Add a `[[scores]]` board {board} with `daily = true`."),
                    None,
                ),
            };
            let mut f = Finding::new("daily-board", Group::Required, title, advice, vec![at, "manifest.toml".into()]);
            f.fix = fix;
            out.push(f);
        }
    }

    // An intro or tutorial remembered as seen.
    if src.has("store::") || src.has("progress::") {
        let seen = ["seen", "shown", "_done", "done_", "played", "watched", "skip_", "skipped", "saw_", "first_run", "first_time"];
        let topic = ["intro", "tutorial", "onboard", "cold_open", "opening", "cutscene", "directions", "how_to", "howto", "lesson", "taught"];
        let at = src.find(|l| {
            let l = l.to_lowercase();
            seen.iter().any(|w| l.contains(w)) && topic.iter().any(|w| l.contains(w))
        });
        if !at.is_empty() {
            out.push(
                Finding::new(
                    "intro-every-run",
                    Group::Recommended,
                    "The game seems to remember an intro or tutorial as seen",
                    "Intros now replay every run with HOLD TO SKIP on screen the whole time (`Sequence`; A, Enter and Space skip too): play the intro and directions each run instead of storing that they were seen, and call `Lesson::reset()` when a run starts. Story that unfolds over runs can stay in progress; it's the intro and the teaching that should come back.",
                    first(at, 3),
                )
                .ask(),
            );
        }
    }

    // A walk or run cycle stepped by a timer.
    if !src.has("Walker") {
        let gait = ["walk", "stride", "gait", "run0", "run_frame", "run_cycle", "run_anim"];
        let timer = ["anim(", "anim_t", "_t)", "_t]", "_t *", "_t*", "time", "timer", "elapsed", "clock", "tick"];
        let at = src.find(|l| {
            let lower = l.to_lowercase();
            !l.trim_start().starts_with("pub const") && !l.trim_start().starts_with("const") && gait.iter().any(|w| lower.contains(w)) && timer.iter().any(|w| lower.contains(w))
        });
        if !at.is_empty() {
            out.push(
                Finding::new(
                    "walker",
                    Group::Recommended,
                    "Walk or run frames seem to be stepped by time",
                    "A cycle stepped by a timer slides when the speed changes and stutters when slow: `walk::Walker` steps feet by distance walked (`Walker::new(height)`, `Walker::running`, `creeping`, `drive(dt, want)`), `walker.frame(n)` picks a sprite-sheet frame by distance, `steps()` gives footfalls, and `WalkSprite` draws a body and two feet (`mb art frames --walker`).",
                    first(at, 3),
                )
                .ask(),
            );
        }
    }

    // Story lines told at fixed points.
    if src.has("Dialogue") && !src.has("narration") && (src.has(".say(") || src.has(".bark(") || src.has(".play(")) {
        let at = src.find(|l| l.contains(".say(") || l.contains(".bark("));
        out.push(Finding::new(
            "narration",
            Group::Recommended,
            "Dialogue without narration",
            "If some of these lines are story rather than reactions, queue them with `dialogue.narration.tell(speaker, &line, expression)` so they're told in quiet stretches (`narration.set_busy(..)` while a threat is on, `narration.lengths = vo_lengths::ALL` from `mb voice lengths`). A reaction to a moment goes through `barks.react(tag)`, heard in that frame.",
            first(at, 3),
        ));
    }

    // Generated art without its measurements.
    let generated = count_files(&dir.join("assets"), ".gen.json");
    if generated > 0 && !dir.join("src/art_meta.rs").is_file() {
        out.push(Finding::new(
            "art-meta",
            Group::Recommended,
            format!("{generated} generated assets and no src/art_meta.rs"),
            format!(
                "`mb art meta {}` writes src/art_meta.rs (each figure's feet and height, each painting's floor line). Then `ground::Figure::new(img, art_meta::HERO).draw(x, floor)` stands a figure by its feet with a contact shadow, and `mb shot` warns about ones that float or are cut off.",
                dir.display()
            ),
            vec!["assets/".into()],
        ));
    }

    // Hints drawn by hand.
    if !src.has("Hint::") && src.find(|l| identifiers(l).any(|id| id == "Hints")).is_empty() {
        let at = src.find(|l| {
            let t = l.trim_start();
            !t.starts_with("pub const") && !t.starts_with("const") && identifiers(l).any(|id| hint_identifier(&id))
        });
        if !at.is_empty() {
            out.push(
                Finding::new(
                    "hints",
                    Group::Recommended,
                    "Hints seem to be drawn by hand",
                    "`ui::Hints` keeps hints from hiding each other or flickering: `want` the ones that apply each frame (`Hint::tap(\"TAP HER\", \"witch\").threat()`, `Hint::hold(..)`, `.ambient()`) and the most important shows; `unseen()` lists any that never did. `bot::Literal` can then play by them (`.follows_hints(secs)`, `report.assert_hints_safe()`).",
                    first(at, 3),
                )
                .ask(),
            );
        }
    }

    // A bot that doesn't report misses or events.
    if src.any_has("Outcome::new(") {
        let misses = src.any_has(".miss(") || src.any_has("assert_misses_forgivable");
        let events = src.any_has(".event(\"") || src.any_has(".events(") || src.any_has(".span(") || src.any_has("assert_events_vary");
        if !misses || !events {
            let what = match (misses, events) {
                (false, false) => "misses or events",
                (false, true) => "misses",
                _ => "events",
            };
            let mut advice = Vec::new();
            if !misses {
                advice.push("`Outcome::miss(threat, t, lost / full)` for each miss as it happens, so the report shows what one costs (`report.assert_misses_forgivable()` fails on HARSH ones)");
            }
            if !events {
                advice.push("`Outcome::event(kind, t)` and `span(..)` for the game's events, so it shows their pacing and mix (`report.assert_events_vary()` fails on CONSTANT and DOMINANT ones)");
            }
            let at = src.lines.iter().filter(|l| l.text.contains("Outcome::new(")).map(|l| format!("{}:{}", l.file, l.n)).collect();
            out.push(Finding::new("bot-reports", Group::Recommended, format!("The bot reports no {what}"), format!("Report {}.", advice.join("; and ")), first(at, 2)));
        }
    }

    out.sort_by_key(|f| f.group);
    out
}

/// The identifiers in a line of code.
fn identifiers(line: &str) -> impl Iterator<Item = String> + '_ {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')).map(str::to_string)
}

/// `HINT_JUMP`, `hint_alpha`, `draw_hint`, `show_hints`: a hint of the game's
/// own (not `lowpass_hint`).
fn hint_identifier(id: &str) -> bool {
    let parts: Vec<String> = id.split('_').filter(|p| !p.is_empty()).map(str::to_lowercase).collect();
    let hint = |p: &String| p == "hint" || p == "hints";
    parts.iter().any(hint) && parts.first().is_some_and(|p| hint(p) || ["draw", "show", "render", "fade", "prompt", "tutorial"].contains(&p.as_str()))
}

fn count_files(dir: &Path, suffix: &str) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.to_string_lossy().ends_with(suffix) {
                n += 1;
            }
        }
    }
    n
}

/// manifest.toml's text with `daily = true` on `[[scores]]` board `board`
/// (replacing a `daily = false`), comments and layout kept. None if it has
/// no such board table.
pub fn set_daily(manifest: &str, board: u32) -> Option<String> {
    let lines: Vec<&str> = manifest.lines().collect();
    let key = |l: &str| l.split('#').next().unwrap_or("").split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()));
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "[[scores]]" {
            i += 1;
            continue;
        }
        // The table runs to the next header.
        let end = (i + 1..lines.len()).find(|&j| lines[j].trim_start().starts_with('[')).unwrap_or(lines.len());
        let ours = (i + 1..end).any(|j| key(lines[j]).is_some_and(|(k, v)| k == "board" && v.parse() == Ok(board)));
        if ours {
            let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            if let Some(j) = (i + 1..end).find(|&j| key(lines[j]).is_some_and(|(k, _)| k == "daily")) {
                out[j] = "daily = true".into();
            } else {
                // After the table's last key, before blank lines and comments.
                let last = (i + 1..end).rev().find(|&j| key(lines[j]).is_some()).unwrap_or(i);
                out.insert(last + 1, "daily = true".into());
            }
            let mut text = out.join("\n");
            if manifest.ends_with('\n') {
                text.push('\n');
            }
            return Some(text);
        }
        i = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "abi = 0\nid = \"dev.ada.hop\"\nname = \"Hop\"\nversion = \"0.1.0\"\ncreator = \"@ada\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nstdlib = { mb2d = 2 }\ncapabilities = [\"store\", \"score\"]\n\n[[scores]]\nboard = 0\nlabel = \"Best\"\norder = \"higher\"\n\n[[scores]]\nboard = 1   # the daily one\nlabel = \"Daily\"\norder = \"higher\"\n# comment after\n";

    fn manifest(text: &str) -> Manifest {
        Manifest::parse(text).unwrap()
    }

    fn ids(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|f| f.id).collect()
    }

    fn check(files: &[(&str, &str)], m: &str) -> Vec<Finding> {
        let dir = std::env::temp_dir().join(format!("mb-upgrade-checks-none-{}", std::process::id()));
        run(&dir, Some(&manifest(m)), &Sources::from_files(files))
    }

    #[test]
    fn daily_is_checked_against_the_manifest() {
        let lib = "const BOARD_DAILY: u32 = 1;\nfn init() {\n    let r = Rounds::new(title).board(0).daily(BOARD_DAILY);\n    let mode = Mode::new().daily();\n}\n";
        let f = check(&[("src/lib.rs", lib)], MANIFEST);
        assert_eq!(ids(&f), ["daily-board"]);
        assert_eq!(f[0].group, Group::Required);
        assert_eq!(f[0].fix, Some(Fix::DailyBoard(1)));
        assert_eq!(f[0].at, ["src/lib.rs:3", "manifest.toml"]);

        // Marked daily: nothing.
        let m = MANIFEST.replace("board = 1   # the daily one\n", "board = 1\ndaily = true\n");
        assert!(check(&[("src/lib.rs", lib)], &m).is_empty());
        // A literal board the manifest doesn't have.
        let f = check(&[("src/lib.rs", "let r = Rounds::new(t).daily(3);")], MANIFEST);
        assert_eq!(ids(&f), ["daily-board"]);
        assert!(f[0].title.contains("no board 3") && f[0].fix.is_none());
        // Board 0 can't be daily.
        let f = check(&[("src/lib.rs", "let r = Rounds::new(t).daily(0);")], MANIFEST);
        assert!(f[0].title.contains("board 0") && f[0].fix.is_none());
    }

    #[test]
    fn a_remembered_intro_a_timed_walk_and_hand_drawn_hints_are_found_outside_tests() {
        let lib = "#[cfg(test)]\nmod bot;\nuse maimbrain::store;\nconst HINT_JUMP: &str = \"JUMP\";\nfn update(&mut self) {\n    if store::get(\"intro_seen\").is_none() { self.play_intro(); }\n    let f = [WALK0, WALK1][(self.walk_t * 8.0) as usize % 2];\n    ui::text(HINT_JUMP, 10.0, 20.0, self.hint_alpha);\n    self.lowpass_hint = 0.3;\n}\n// the intro_seen flag, in a comment\n#[cfg(test)]\nmod tests {\n    fn t() { store::set(\"tutorial_done\", &[1]); }\n}\nfn after() { let seen_intro = store::get(\"seen_intro\"); }\n";
        let bot = "fn hints() { let doc = \"**Hints**\"; store::set(\"intro_seen\", &[1]); }\n";
        let f = check(&[("src/lib.rs", lib), ("src/bot.rs", bot)], MANIFEST);
        assert_eq!(ids(&f), ["intro-every-run", "walker", "hints"], "bot.rs is a test module");
        assert_eq!(f[0].at, ["src/lib.rs:6", "src/lib.rs:16"], "not in the tests module, but after it");
        assert_eq!(f[1].at, ["src/lib.rs:7"]);
        assert_eq!(f[2].at, ["src/lib.rs:8"], "not the const, nor lowpass_hint");
        assert!(f.iter().all(|f| f.ask_owner));
        // Adopted: no findings.
        let adopted = "fn update(&mut self) {\n    self.lesson.reset();\n    self.walker.update(dt, x);\n    let w: Walker = Walker::new(40.0);\n    self.hints.want(Hint::tap(\"TAP\", \"door\"));\n}\n";
        assert!(check(&[("src/lib.rs", adopted)], MANIFEST).is_empty());
    }

    #[test]
    fn dialogue_without_narration_and_a_quiet_bot() {
        let lib = "fn f(d: &mut Dialogue) { d.say(\"ann\", &line); }\n";
        let bot = "#[cfg(test)]\nfn play() -> Outcome { Outcome::new(secs).cause(\"fell\") }\n";
        let f = check(&[("src/lib.rs", lib), ("src/bot.rs", bot)], MANIFEST);
        assert_eq!(ids(&f), ["narration", "bot-reports"]);
        assert_eq!(f[0].at, ["src/lib.rs:1"]);
        assert!(f[1].title.ends_with("misses or events") && f[1].at == ["src/bot.rs:2"], "{:?}", f[1]);
        let bot = "fn play() -> Outcome { Outcome::new(secs).miss(\"gust\", t, 0.2).event(\"gust\", t) }\n";
        let lib = "fn f(d: &mut Dialogue) { d.narration.tell(\"ann\", &line, \"happy\"); }\n";
        assert!(check(&[("src/lib.rs", lib), ("src/bot.rs", bot)], MANIFEST).is_empty());
    }

    #[test]
    fn generated_art_wants_art_meta() {
        let dir = std::env::temp_dir().join(format!("mb-upgrade-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets/hero")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("assets/hero/hero.png.gen.json"), "{}").unwrap();
        let src = Sources::read(&dir);
        assert_eq!(ids(&run(&dir, Some(&manifest(MANIFEST)), &src)), ["art-meta"]);
        std::fs::write(dir.join("src/art_meta.rs"), "").unwrap();
        assert!(run(&dir, Some(&manifest(MANIFEST)), &src).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_marks_the_board_daily_and_keeps_the_rest() {
        let out = set_daily(MANIFEST, 1).unwrap();
        assert!(out.contains("board = 1   # the daily one\nlabel = \"Daily\"\norder = \"higher\"\ndaily = true\n# comment after\n"), "{out}");
        assert!(manifest(&out).scores.iter().find(|s| s.board == 1).unwrap().daily);
        assert!(!manifest(&out).scores[0].daily);
        let off = MANIFEST.replace("order = \"higher\"\n# comment", "order = \"higher\"\ndaily = false\n# comment");
        assert!(manifest(&set_daily(&off, 1).unwrap()).scores[1].daily);
        assert_eq!(set_daily(MANIFEST, 5), None);
    }
}
