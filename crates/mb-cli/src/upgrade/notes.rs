//! The kit's upgrade notes: the `### Upgrading` part of each release in its
//! CHANGELOG.md (tools/kit/files/ in the monorepo), embedded in mb when it's
//! built (build.rs). Each note is in one of three groups, under a line that
//! names it, and is one list item:
//!
//! ```text
//! ## 0.8.2
//! …what's new, for a person…
//!
//! ### Upgrading
//!
//! Required:
//! - **A held start's release reaches the game.** What changed and what to fix.
//!
//! Recommended:
//! - **Play the intro every run.** A new default worth adopting.
//!
//! Available:
//! - **Daily boards** (fits: games with a seeded course): a new capability and the API to use.
//! ```
//!
//! An item's bold lead is its title and `(fits: …)` after it the kind of game
//! an Available one suits; an item can run on over indented lines.

use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;

/// The kit's CHANGELOG.md, as mb was built with it.
pub const CHANGELOG: &str = include_str!(env!("MB_KIT_CHANGELOG"));

/// A kit (and SDK) version: `0.8.2`, `v0.8.2`, `0.10.0-dev` (a pre-release,
/// older than `0.10.0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string())),
            None => (s.split('+').next().unwrap_or(s), None),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let v = Version { major: parts.next()??, minor: parts.next()??, patch: parts.next().unwrap_or(Some(0))?, pre };
        parts.next().is_none().then_some(v)
    }

    /// The version this mb (and its kit) is.
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("mb-cli's version is x.y.z")
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(o.major, o.minor, o.patch)).then_with(|| match (&self.pre, &o.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    /// A breaking change to fix.
    Required,
    /// A new default to adopt.
    Recommended,
    /// A new capability, and the games it fits.
    Available,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Required, Group::Recommended, Group::Available];

    pub fn name(self) -> &'static str {
        match self {
            Group::Required => "Required",
            Group::Recommended => "Recommended",
            Group::Available => "Available",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note {
    pub group: Group,
    pub title: Option<String>,
    /// The kind of game it fits (Available notes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fits: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Release {
    pub version: Version,
    pub notes: Vec<Note>,
}

/// Every release in a changelog, newest first as written, with its upgrade
/// notes (none for a release without an `### Upgrading` part).
pub fn parse(changelog: &str) -> Result<Vec<Release>, String> {
    let mut releases: Vec<Release> = Vec::new();
    let mut upgrading = false;
    let mut group = None;
    for (i, line) in changelog.lines().enumerate() {
        let at = || format!("CHANGELOG.md line {}", i + 1);
        if let Some(h) = line.strip_prefix("## ") {
            let v = h.split_whitespace().next().and_then(Version::parse).ok_or_else(|| format!("{}: a release heading without a version: {line}", at()))?;
            releases.push(Release { version: v, notes: Vec::new() });
            (upgrading, group) = (false, None);
        } else if let Some(h) = line.strip_prefix("### ") {
            upgrading = h.trim() == "Upgrading" && !releases.is_empty();
            group = None;
        } else if !upgrading {
        } else if let Some(g) = Group::ALL.into_iter().find(|g| line.trim_end() == format!("{}:", g.name())) {
            group = Some(g);
        } else if let Some(item) = line.strip_prefix("- ") {
            let g = group.ok_or_else(|| format!("{}: an upgrade note before Required:, Recommended: or Available:", at()))?;
            releases.last_mut().unwrap().notes.push(note(g, item));
        } else if line.starts_with("  ") && !line.trim().is_empty() {
            let n = releases.last_mut().unwrap().notes.last_mut().ok_or_else(|| format!("{}: an indented line outside a note", at()))?;
            n.text = format!("{} {}", n.text, line.trim());
        } else if !line.trim().is_empty() {
            return Err(format!("{}: in ### Upgrading, expected Required:, Recommended:, Available: or a `- ` note: {line}", at()));
        }
    }
    Ok(releases)
}

/// One list item: `**Title.** text` or `**Title** (fits: …): text`, or plain text.
fn note(group: Group, item: &str) -> Note {
    let item = item.trim();
    let (mut title, mut rest) = (None, item);
    if let Some(after) = item.strip_prefix("**")
        && let Some(end) = after.find("**")
    {
        title = Some(after[..end].trim_end_matches(['.', ':']).to_string());
        rest = after[end + 2..].trim_start_matches(':').trim_start();
    }
    let mut fits = None;
    if let Some(after) = rest.strip_prefix("(fits: ")
        && let Some(end) = after.find("):")
    {
        fits = Some(after[..end].to_string());
        rest = after[end + 2..].trim_start();
    }
    Note { group, title, fits, text: rest.to_string() }
}

/// The releases after `from`, up to and including `to`, oldest first.
pub fn between<'a>(releases: &'a [Release], from: &Version, to: &Version) -> Vec<&'a Release> {
    let mut r: Vec<&Release> = releases.iter().filter(|r| &r.version > from && &r.version <= to).collect();
    r.sort_by(|a, b| a.version.cmp(&b.version));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn versions_compare_as_numbers_and_a_pre_release_comes_first() {
        assert!(v("0.10.0") > v("0.9.0"));
        assert!(v("0.9.0") > v("0.8.1"));
        assert!(v("v1.0.0") > v("0.99.99"));
        assert!(v("0.10.0-dev") < v("0.10.0"));
        assert!(v("0.10.0-dev") > v("0.9.9"));
        assert_eq!(v("v0.9.0"), v("0.9.0"));
        assert_eq!(v("0.9"), v("0.9.0"));
        assert_eq!(v("0.10.0-dev").to_string(), "0.10.0-dev");
        for bad in ["", "x.y.z", "0", "1.2.3.4", "1..2"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    const LOG: &str = "# Creator kit changelog\n\n## 0.10.0\n\nStuff.\n\n- **A thing.** For people.\n\n### Upgrading\n\nRequired:\n- **Renamed `foo`.** Use `bar`.\n\nAvailable:\n- **Daily boards** (fits: puzzles, courses): `Rounds::daily`\n  over two lines.\n- Plain text.\n\n## 0.9.0\n\n### Upgrading\n\nRecommended:\n- **Intros every run.** No flags.\n\n## 0.8.1\n\nNothing.\n\n### Something else\n- not a note\n\n## 0.8.0\n";

    #[test]
    fn notes_parse_into_groups_with_titles_and_fits() {
        let r = parse(LOG).unwrap();
        assert_eq!(r.iter().map(|r| r.version.to_string()).collect::<Vec<_>>(), ["0.10.0", "0.9.0", "0.8.1", "0.8.0"]);
        assert_eq!(
            r[0].notes,
            [
                Note { group: Group::Required, title: Some("Renamed `foo`".into()), fits: None, text: "Use `bar`.".into() },
                Note { group: Group::Available, title: Some("Daily boards".into()), fits: Some("puzzles, courses".into()), text: "`Rounds::daily` over two lines.".into() },
                Note { group: Group::Available, title: None, fits: None, text: "Plain text.".into() },
            ]
        );
        assert_eq!(r[1].notes[0].group, Group::Recommended);
        assert!(r[2].notes.is_empty() && r[3].notes.is_empty(), "no ### Upgrading");
    }

    #[test]
    fn notes_are_chosen_by_range() {
        let r = parse(LOG).unwrap();
        let picked = |from: &str, to: &str| between(&r, &v(from), &v(to)).iter().map(|r| r.version.to_string()).collect::<Vec<_>>();
        assert_eq!(picked("0.8.0", "0.10.0"), ["0.8.1", "0.9.0", "0.10.0"], "oldest first, after the game's");
        assert_eq!(picked("0.8.1", "0.9.0"), ["0.9.0"], "never past the kit");
        assert_eq!(picked("0.10.0", "0.10.0"), Vec::<String>::new());
        assert_eq!(picked("0.7.0", "0.8.0"), ["0.8.0"]);
        assert_eq!(picked("0.10.0-dev", "0.10.0"), ["0.10.0"]);
    }

    #[test]
    fn a_misplaced_note_is_an_error() {
        assert!(parse("## 1.0.0\n### Upgrading\n- a note with no group\n").unwrap_err().contains("line 3"));
        assert!(parse("## 1.0.0\n### Upgrading\nRequired:\nsome stray text\n").unwrap_err().contains("expected"));
        assert!(parse("## soon\n").is_err());
    }

    /// The kit's own changelog parses and has a section for this version.
    #[test]
    fn the_kits_changelog_parses_and_has_this_release() {
        let r = parse(CHANGELOG).unwrap();
        assert!(r.iter().any(|r| r.version == Version::current()), "tools/kit/files/CHANGELOG.md has no ## {}", Version::current());
        let this = r.iter().find(|r| r.version == v("0.8.2")).unwrap();
        for g in Group::ALL {
            assert!(this.notes.iter().any(|n| n.group == g), "0.8.2 has no {} upgrade notes", g.name());
        }
        for n in r.iter().flat_map(|r| &r.notes) {
            assert!(n.title.is_some() && !n.text.is_empty(), "{n:?}");
            assert_eq!(n.fits.is_some(), n.group == Group::Available, "Available notes (only) say what they fit: {n:?}");
        }
        for old in ["0.8.0", "0.8.1"] {
            assert!(r.iter().any(|r| r.version == v(old) && !r.notes.is_empty()), "{old} has upgrade notes");
        }
    }
}
