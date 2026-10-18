//! Watching a game's files for `mb serve --watch`.
//!
//! The watcher polls: every POLL it lists the game's sources and assets with
//! their sizes and modification times. A game is a few dozen files, so a
//! scan costs well under a millisecond, and polling behaves the same on
//! macOS, Linux and Windows, on network drives and in WSL or a container
//! looking at a Windows folder, where native change events often never
//! arrive. Changes are debounced (an editor's save, `cargo fmt` or a script
//! regenerating twenty sprites becomes one build).

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use mb_format::bundle::ASSET_EXTENSIONS;

/// How often the game's files are scanned.
pub const POLL: Duration = Duration::from_millis(40);
/// A change is built once nothing more has changed for this long…
pub const QUIET: Duration = Duration::from_millis(60);
/// …or once changes have kept coming for this long.
pub const MAX_WAIT: Duration = Duration::from_millis(600);

/// What's on disk: path (relative to the game, with `/`) → (size, mtime).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Snapshot(BTreeMap<String, (u64, u128)>);

impl Snapshot {
    /// The files a watch build depends on: manifest.toml, Cargo.toml (and a
    /// build.rs or Cargo.lock beside it), everything under src/, and the
    /// asset files under assets/ that a bundle would carry.
    pub fn scan(dir: &Path) -> Snapshot {
        let mut s = Snapshot::default();
        for f in ["manifest.toml", "Cargo.toml", "Cargo.lock", "build.rs"] {
            s.add(&dir.join(f), f.to_string());
        }
        s.walk(&dir.join("src"), "src", false);
        s.walk(&dir.join("assets"), "assets", true);
        s
    }

    /// A Studio project's files: studio.toml, icon.png and what's under the
    /// directories the engine reads.
    pub fn scan_studio(dir: &Path) -> Snapshot {
        let mut s = Snapshot::default();
        for f in ["studio.toml", "icon.png"] {
            s.add(&dir.join(f), f.to_string());
        }
        for d in ["game", "sprites", "tiles", "maps", "sounds", "music", "debug"] {
            s.walk(&dir.join(d), d, false);
        }
        s
    }

    /// A single file (a .mbx being served).
    pub fn file(path: &Path) -> Snapshot {
        let mut s = Snapshot::default();
        s.add(path, path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()));
        s
    }

    fn add(&mut self, path: &Path, rel: String) {
        if let Ok(m) = std::fs::metadata(path)
            && m.is_file()
        {
            let mtime = m.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            self.0.insert(rel, (m.len(), mtime));
        }
    }

    fn walk(&mut self, path: &Path, rel: &str, assets: bool) {
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if ignored(&name) {
                continue;
            }
            let p = e.path();
            let r = format!("{rel}/{name}");
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                self.walk(&p, &r, assets);
            } else if !assets || is_asset(&name) {
                self.add(&p, r);
            }
        }
    }

    /// A short fingerprint of the whole snapshot.
    pub fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }

    /// Paths added, removed or changed between `self` and `newer`.
    pub fn changes(&self, newer: &Snapshot) -> Vec<String> {
        let mut out: Vec<String> = newer.0.iter().filter(|(k, v)| self.0.get(*k) != Some(v)).map(|(k, _)| k.clone()).collect();
        out.extend(self.0.keys().filter(|k| !newer.0.contains_key(*k)).cloned());
        out.sort();
        out
    }

    #[cfg(test)]
    fn of(entries: &[(&str, u64)]) -> Snapshot {
        Snapshot(entries.iter().map(|(k, t)| (k.to_string(), (1, *t as u128))).collect())
    }
}

/// Editor droppings and hidden files: never a reason to rebuild.
fn ignored(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('#') || name.ends_with('~') || name == "4913" // vim's write test
        || [".swp", ".swx", ".tmp", ".bak", ".orig"].iter().any(|x| name.ends_with(x))
}

fn is_asset(name: &str) -> bool {
    Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| ASSET_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// What a set of changes needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The manifest changed: rebuild, and reload the page (canvas size,
    /// inputs and sensors are fixed when it boots).
    Page,
    /// Sources changed: rebuild and swap the game's code in the open pages.
    Code,
    /// Only assets changed: nothing to build; the pages refetch these.
    Assets(Vec<String>),
}

pub fn classify(changed: &[String]) -> Kind {
    if changed.iter().any(|p| p == "manifest.toml") {
        Kind::Page
    } else if changed.iter().any(|p| !p.starts_with("assets/")) {
        Kind::Code
    } else {
        Kind::Assets(changed.to_vec())
    }
}

/// Coalesces bursts of changes: `ready` turns true once QUIET has passed
/// since the last change, or MAX_WAIT since the first.
#[derive(Debug)]
pub struct Debounce {
    quiet: Duration,
    max_wait: Duration,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Debounce {
    pub fn new(quiet: Duration, max_wait: Duration) -> Debounce {
        Debounce { quiet, max_wait, first: None, last: None }
    }

    /// Something changed at `now`.
    pub fn changed(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// Whether to act now; resets once it says yes.
    pub fn ready(&mut self, now: Instant) -> bool {
        let (Some(first), Some(last)) = (self.first, self.last) else { return false };
        if now.duration_since(last) >= self.quiet || now.duration_since(first) >= self.max_wait {
            self.first = None;
            self.last = None;
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub fn pending(&self) -> bool {
        self.first.is_some()
    }
}

/// Polls with `scan` and calls `build` with each debounced batch of changes
/// since `built` (relative paths), forever. `build` runs on this thread, so
/// changes made during a build are picked up, and built, right after it.
pub fn run(scan: &dyn Fn() -> Snapshot, mut built: Snapshot, mut build: impl FnMut(Vec<String>, &Snapshot)) -> ! {
    let mut seen = scan();
    let mut debounce = Debounce::new(QUIET, MAX_WAIT);
    if seen != built {
        // Changed while the first build ran.
        debounce.changed(Instant::now());
    }
    loop {
        std::thread::sleep(POLL);
        let now = scan();
        if now != seen {
            debounce.changed(Instant::now());
            seen = now;
        }
        if debounce.ready(Instant::now()) {
            let changed = built.changes(&seen);
            if !changed.is_empty() {
                built = seen.clone();
                build(changed, &built);
            }
        }
    }
}

/// Where a game's files are, for messages.
pub fn describe(dir: &Path) -> String {
    if crate::studio::is_project(dir) {
        return format!("{} (a Studio project: studio.toml, game/, sprites/, tiles/, maps/, sounds/, music/, debug/)", dir.display());
    }
    let mut parts = vec!["src/"];
    if dir.join("assets").is_dir() {
        parts.push("assets/");
    }
    parts.extend(["manifest.toml", "Cargo.toml"]);
    format!("{} ({})", dir.display(), parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn debounce_waits_for_quiet() {
        let t = Instant::now();
        let mut d = Debounce::new(ms(60), ms(600));
        assert!(!d.ready(t), "nothing changed");
        d.changed(t);
        assert!(!d.ready(t + ms(30)));
        assert!(d.ready(t + ms(60)));
        assert!(!d.ready(t + ms(200)), "it fires once per batch");
    }

    #[test]
    fn debounce_coalesces_a_burst() {
        let t = Instant::now();
        let mut d = Debounce::new(ms(60), ms(600));
        // A save every 40 ms (a script writing sprites): one build, at the end.
        for i in 0..5 {
            d.changed(t + ms(40 * i));
            assert!(!d.ready(t + ms(40 * i + 20)));
        }
        assert!(d.ready(t + ms(160 + 60)));
        assert!(!d.pending());
    }

    #[test]
    fn debounce_gives_up_waiting_after_max_wait() {
        let t = Instant::now();
        let mut d = Debounce::new(ms(60), ms(600));
        let mut fired = None;
        for i in 0..40 {
            let now = t + ms(30 * i);
            d.changed(now);
            if d.ready(now) {
                fired = Some(30 * i);
                break;
            }
        }
        assert_eq!(fired, Some(600), "changes that never stop still build every MAX_WAIT");
    }

    #[test]
    fn changes_and_kinds() {
        let a = Snapshot::of(&[("Cargo.toml", 1), ("manifest.toml", 1), ("src/lib.rs", 1), ("assets/a.png", 1), ("assets/b.ogg", 1)]);
        let b = Snapshot::of(&[("Cargo.toml", 1), ("manifest.toml", 1), ("src/lib.rs", 1), ("assets/a.png", 2), ("assets/c.ogg", 1)]);
        let changed = a.changes(&b);
        assert_eq!(changed, ["assets/a.png", "assets/b.ogg", "assets/c.ogg"]);
        assert_eq!(classify(&changed), Kind::Assets(changed.clone()));
        assert_eq!(classify(&["assets/a.png".into(), "src/sim.rs".into()]), Kind::Code);
        assert_eq!(classify(&["Cargo.toml".into()]), Kind::Code);
        assert_eq!(classify(&["src/sim.rs".into(), "manifest.toml".into()]), Kind::Page);
        assert!(a.changes(&a).is_empty());
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn scans_what_a_build_depends_on() {
        let dir = std::env::temp_dir().join(format!("mb-watch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for f in ["src/lib.rs", "src/levels/one.txt", "assets/hero.png", "assets/notes.psd", "assets/.hero.png.swp", "tools/make.py", "manifest.toml", "Cargo.toml", "src/lib.rs~"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, f).unwrap();
        }
        let s = Snapshot::scan(&dir);
        let keys: Vec<&str> = s.0.keys().map(String::as_str).collect();
        assert_eq!(keys, ["Cargo.toml", "assets/hero.png", "manifest.toml", "src/levels/one.txt", "src/lib.rs"]);
        std::fs::write(dir.join("src/lib.rs"), "changed, and longer").unwrap();
        assert_eq!(s.changes(&Snapshot::scan(&dir)), ["src/lib.rs"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
