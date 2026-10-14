//! `mb upgrade <game dir>`: brings a game made with an older kit up to this
//! one, and says what it can adopt.
//!
//! - The SDK the game builds with: its Cargo.lock's `maimbrain` (version and
//!   git commit), and the SDK its newest bundle in `feed/` was built with
//!   (game.wasm's `maimbrain-sdk` section). The older of the two is where the
//!   notes start (`--since` picks another).
//! - The lock: a git dependency (what `mb new` writes) moves to this kit's SDK
//!   with `cargo update -p maimbrain` (`--apply`); a `path` dependency (games
//!   in the monorepo) has nothing to move.
//! - The upgrade notes of every release after the game's SDK, up to this
//!   kit's (notes.rs), and checks of the game's code and manifest that turn
//!   them into advice (checks.rs). `--apply` makes only the mechanical fixes.
//!
//! `nudge` is the one line `mb build`, `serve` and `publish` print when a
//! game's lock is behind this kit; release.rs is `mb doctor`'s check for a
//! newer `mb`.

pub mod checks;
pub mod notes;
pub mod release;

use std::path::{Path, PathBuf};
use std::process::Command;

use mb_format::Manifest;
use serde::Serialize;

use notes::{Group, Version};

#[derive(clap::Args)]
pub struct UpgradeArgs {
    /// The game directory.
    pub dir: PathBuf,
    /// Make the safe, mechanical changes: move Cargo.lock to this kit's SDK (`cargo update -p
    /// maimbrain`), and mark the board `Rounds::daily(n)` names `daily = true` in manifest.toml.
    /// Never adopts a capability or changes how the game plays or looks.
    #[arg(long)]
    pub apply: bool,
    /// Print JSON, for agents.
    #[arg(long)]
    pub json: bool,
    /// Show the notes of every release after this version (default: the game's SDK).
    #[arg(long, value_name = "VERSION")]
    pub since: Option<String>,
}

/// How a game depends on the SDK, from its Cargo.toml.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DepKind {
    Git,
    Path,
    Registry,
}

/// The SDK a game builds with.
#[derive(Debug, Serialize)]
pub struct Sdk {
    dependency: DepKind,
    /// The git URL or the path, as Cargo.toml gives it.
    source: String,
    /// A `rev`, `tag` or `branch` Cargo.toml pins a git dependency to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pinned: Option<String>,
    /// The version the lock records (for a path dependency without a lock, the path's).
    version: Option<Version>,
    /// The git commit the lock records.
    #[serde(skip_serializing_if = "Option::is_none")]
    rev: Option<String>,
    lock: Option<PathBuf>,
    /// The SDK the newest bundle of the game in feed/ was built with, and that bundle.
    built: Option<Built>,
}

#[derive(Debug, Serialize)]
struct Built {
    version: Version,
    bundle: PathBuf,
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(path).ok()?.parse().ok()
}

/// The directory of the Cargo workspace `dir`'s crate is in: its own, if
/// its Cargo.toml has `[workspace]`, else the nearest parent's that does.
fn workspace_root(dir: &Path) -> Option<PathBuf> {
    let dir = dir.canonicalize().ok()?;
    dir.ancestors().find(|d| read_toml(&d.join("Cargo.toml")).is_some_and(|t| t.contains_key("workspace"))).map(Path::to_path_buf)
}

/// The `maimbrain` dependency: (kind, source, pin), resolving `workspace = true`.
fn dependency(dir: &Path) -> Result<(DepKind, String, Option<String>, PathBuf), String> {
    let path = dir.join("Cargo.toml");
    let doc = read_toml(&path).ok_or_else(|| format!("{}: not found or not TOML", path.display()))?;
    let mut base = dir.to_path_buf();
    let mut dep = doc.get("dependencies").and_then(|d| d.get("maimbrain")).cloned();
    if dep.as_ref().and_then(|d| d.get("workspace")).and_then(|w| w.as_bool()) == Some(true) {
        let root = workspace_root(dir).ok_or_else(|| format!("{}: maimbrain comes from a workspace, but there's none", path.display()))?;
        dep = read_toml(&root.join("Cargo.toml")).and_then(|t| t.get("workspace")?.get("dependencies")?.get("maimbrain").cloned());
        base = root;
    }
    let dep = dep.ok_or_else(|| format!("{}: no maimbrain dependency (is this a Maimbrain game?)", path.display()))?;
    let field = |k: &str| dep.get(k).and_then(|v| v.as_str()).map(str::to_string);
    if let Some(p) = field("path") {
        Ok((DepKind::Path, p, None, base))
    } else if let Some(g) = field("git") {
        let pin = ["rev", "tag", "branch"].into_iter().find_map(|k| field(k).map(|v| format!("{k} = \"{v}\"")));
        Ok((DepKind::Git, g, pin, base))
    } else {
        Ok((DepKind::Registry, dep.as_str().map(str::to_string).or_else(|| field("version")).unwrap_or_default(), None, base))
    }
}

/// The lock's `maimbrain` package: (version, git commit).
fn locked(lock: &Path, kind: &DepKind) -> Option<(Version, Option<String>)> {
    let doc = read_toml(lock)?;
    let packages = doc.get("package")?.as_array()?;
    let ours: Vec<&toml::Value> = packages.iter().filter(|p| p.get("name").and_then(|n| n.as_str()) == Some("maimbrain")).collect();
    let source = |p: &toml::Value| p.get("source").and_then(|s| s.as_str()).map(str::to_string);
    let pick = ours.iter().find(|p| match kind {
        DepKind::Git => source(p).is_some_and(|s| s.starts_with("git+")),
        DepKind::Path => source(p).is_none(),
        DepKind::Registry => source(p).is_some_and(|s| s.starts_with("registry+")),
    });
    let p = pick.or(ours.first())?;
    let version = Version::parse(p.get("version")?.as_str()?)?;
    let rev = source(p).and_then(|s| s.split_once('#').map(|(_, r)| r.to_string()));
    Some((version, rev))
}

/// The newest bundle of game `id` in the game's feed/ (or ./feed), and the SDK in it.
fn built(dir: &Path, id: &str) -> Option<Built> {
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = [dir.join("feed"), PathBuf::from("feed")]
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mbx") && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{id}-"))))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    found.sort();
    let (_, path) = found.pop()?;
    let bytes = std::fs::read(&path).ok()?;
    let bundle = mb_format::Bundle::open(&bytes).ok()?;
    let version = bundle.file("game.wasm").and_then(mb_format::wasm::sdk_version).and_then(|v| Version::parse(&v))?;
    Some(Built { version, bundle: path })
}

/// What the game builds with.
pub fn sdk(dir: &Path, id: Option<&str>) -> Result<Sdk, String> {
    let (kind, source, pinned, base) = dependency(dir)?;
    let lock = workspace_root(dir).unwrap_or_else(|| dir.to_path_buf()).join("Cargo.lock");
    let lock = lock.is_file().then_some(lock);
    let (mut version, rev) = lock.as_deref().and_then(|l| locked(l, &kind)).map_or((None, None), |(v, r)| (Some(v), r));
    if version.is_none() && kind == DepKind::Path {
        version = read_toml(&base.join(&source).join("Cargo.toml")).and_then(|t| Version::parse(t.get("package")?.get("version")?.as_str()?));
    }
    Ok(Sdk { dependency: kind, source, pinned, version, rev, lock, built: id.and_then(|id| built(dir, id)) })
}

/// `mb build`, `serve` and `publish`'s line when a game's lock holds an older
/// SDK than this kit's (not for a path dependency, which has nothing to bump).
pub fn nudge(path: &Path) {
    if !path.is_dir() {
        return;
    }
    let Ok(s) = sdk(path, None) else { return };
    let kit = Version::current();
    if s.dependency != DepKind::Path
        && let Some(v) = s.version.filter(|v| *v < kit)
    {
        eprintln!("built with SDK {v}; this kit is {kit}: run mb upgrade {}", path.display());
    }
}

/// What happens to the lock.
#[derive(Debug, Serialize)]
struct Lock {
    /// `update` (cargo update -p maimbrain moves it), `none` (nothing to do) or `manual`.
    action: &'static str,
    message: String,
    applied: bool,
    /// The SDK the lock holds after --apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    version_after: Option<Version>,
}

fn lock_plan(dir: &Path, s: &Sdk, kit: &Version) -> Lock {
    let lock = |action, message: String| Lock { action, message, applied: false, version_after: None };
    match (&s.dependency, &s.version) {
        (DepKind::Path, v) => lock(
            "none",
            format!(
                "the SDK is a path dependency ({}){}: it builds with what's there, so there's nothing to bump",
                s.source,
                v.as_ref().map(|v| format!(", {v}")).unwrap_or_default()
            ),
        ),
        (DepKind::Registry, _) => lock("manual", format!("maimbrain comes from a registry ({}): set its version to \"{kit}\" in Cargo.toml", s.source)),
        (DepKind::Git, _) if s.pinned.is_some() => lock(
            "manual",
            format!("Cargo.toml pins maimbrain to {}: change it to tag = \"v{kit}\" (or drop the pin), then mb upgrade --apply", s.pinned.as_deref().unwrap_or("")),
        ),
        (DepKind::Git, None) => lock("none", "no Cargo.lock yet: the next build fetches the kit's latest SDK".into()),
        (DepKind::Git, Some(v)) if v < kit => lock("update", format!("Cargo.lock holds SDK {v}: `cargo update -p maimbrain` in {} moves it to the kit's latest", dir.display())),
        (DepKind::Git, Some(v)) if v > kit => {
            lock("none", format!("Cargo.lock holds SDK {v}, newer than this mb ({kit}): update mb (mb doctor says how) to get its notes and checks"))
        }
        (DepKind::Git, Some(_)) => lock("none", "Cargo.lock already holds this kit's SDK".into()),
    }
}

/// `cargo update -p maimbrain` for the game, its output captured.
fn update_lock(dir: &Path) -> Result<(), String> {
    let out = Command::new("cargo")
        .args(["update", "-p", "maimbrain", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("running cargo: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("cargo update -p maimbrain failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

pub fn run(a: UpgradeArgs) -> Result<(), String> {
    let dir = &a.dir;
    if !dir.join("Cargo.toml").is_file() {
        return Err(format!("{}: no Cargo.toml (give a game directory)", dir.display()));
    }
    let manifest_path = dir.join("manifest.toml");
    let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest = Manifest::parse(&manifest_text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let kit = Version::current();
    let since = a.since.as_deref().map(|s| Version::parse(s).ok_or_else(|| format!("--since {s}: not a version (like 0.8.0)"))).transpose()?;
    let mut sdk = sdk(dir, Some(&manifest.id))?;
    let from = since.clone().or_else(|| [sdk.version.clone(), sdk.built.as_ref().map(|b| b.version.clone())].into_iter().flatten().min());

    let mut lock = lock_plan(dir, &sdk, &kit);
    let mut errors = Vec::new();
    if a.apply && lock.action == "update" {
        match update_lock(dir) {
            Ok(()) => {
                let after = sdk.lock.as_deref().and_then(|l| locked(l, &sdk.dependency));
                lock.applied = true;
                lock.version_after = after.as_ref().map(|(v, _)| v.clone());
                if let Some((v, rev)) = after {
                    lock.message = format!("Cargo.lock now holds SDK {v}{}", rev.as_deref().map(|r| format!(" ({})", &r[..r.len().min(7)])).unwrap_or_default());
                    if v < kit {
                        lock.message += &format!(", still older than this kit ({kit}): the kit's repository may not have it yet");
                    }
                    (sdk.version, sdk.rev) = (Some(v), rev);
                }
            }
            Err(e) => errors.push(e),
        }
    }

    let releases = notes::parse(notes::CHANGELOG)?;
    let newer: Vec<&notes::Release> = from.as_ref().map(|f| notes::between(&releases, f, &kit)).unwrap_or_default();

    let src = checks::Sources::read(dir);
    let mut findings = checks::run(dir, Some(&manifest), &src);
    if a.apply {
        let mut text = manifest_text.clone();
        for f in findings.iter_mut() {
            if let Some(checks::Fix::DailyBoard(b)) = f.fix
                && let Some(t) = checks::set_daily(&text, b)
                && Manifest::parse(&t).is_ok_and(|m| m.scores.iter().any(|s| s.board == b && s.daily))
            {
                text = t;
                f.applied = true;
            }
        }
        if text != manifest_text {
            std::fs::write(&manifest_path, &text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        }
    }

    if a.json {
        let doc = serde_json::json!({
            "game": { "dir": dir, "id": manifest.id, "name": manifest.name },
            "kit": kit,
            "sdk": sdk,
            "from": from,
            "lock": lock,
            "releases": newer,
            "checks": findings,
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
    } else {
        print!("{}", text_report(dir, &manifest, &kit, &sdk, from.as_ref(), &lock, &newer, &findings, a.apply));
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

#[allow(clippy::too_many_arguments)]
fn text_report(
    dir: &Path,
    m: &Manifest,
    kit: &Version,
    sdk: &Sdk,
    from: Option<&Version>,
    lock: &Lock,
    newer: &[&notes::Release],
    findings: &[checks::Finding],
    applied: bool,
) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let _ = writeln!(o, "{} ({}), {}", m.name, m.id, dir.display());
    let short = |r: &str| r[..r.len().min(7)].to_string();
    let source = match sdk.dependency {
        DepKind::Git => format!("git {}{}", sdk.source, sdk.rev.as_deref().map(|r| format!(" at {}", short(r))).unwrap_or_default()),
        DepKind::Path => format!("path {}", sdk.source),
        DepKind::Registry => format!("registry {}", sdk.source),
    };
    let version = sdk.version.as_ref().map_or("unknown".to_string(), |v| v.to_string());
    let _ = write!(o, "SDK {version} ({source})");
    if let Some(b) = &sdk.built {
        let _ = write!(o, "; last built with {} ({})", b.version, b.bundle.display());
    }
    let _ = writeln!(o, "\nThis kit: {kit}");
    let done = if lock.applied { "done: " } else { "" };
    let hint = if lock.action == "update" && !applied { " (mb upgrade --apply does it)" } else { "" };
    let _ = writeln!(o, "Lock: {done}{}{hint}", lock.message);

    match from {
        None => {
            let _ = writeln!(o, "\nThe game's SDK version isn't known: pass --since <version> (like 0.8.0) for the notes since then.");
        }
        Some(f) if newer.is_empty() => {
            let _ = writeln!(o, "\nNo releases after {f}: no upgrade notes.");
        }
        Some(f) => {
            let versions: Vec<String> = newer.iter().map(|r| r.version.to_string()).collect();
            let _ = writeln!(o, "\nUpgrade notes since {f} ({}):", versions.join(", "));
            for g in Group::ALL {
                let notes: Vec<(&Version, &notes::Note)> = newer.iter().rev().flat_map(|r| r.notes.iter().filter(|n| n.group == g).map(move |n| (&r.version, n))).collect();
                if notes.is_empty() {
                    continue;
                }
                let what = match g {
                    Group::Required => "breaking changes to fix",
                    Group::Recommended => "new defaults worth adopting",
                    Group::Available => "new capabilities, and the games they fit",
                };
                let _ = writeln!(o, "\n{} ({what})", g.name());
                for (v, n) in notes {
                    let title = n.title.as_deref().map(|t| format!("{t}. ")).unwrap_or_default();
                    let fits = n.fits.as_deref().map(|f| format!(" Fits: {f}.")).unwrap_or_default();
                    let mut text = n.text.chars();
                    let text: String = text.next().map(|c| c.to_uppercase().chain(text).collect()).unwrap_or_default();
                    let _ = writeln!(o, "- [{v}] {title}{text}{fits}");
                }
            }
            let quiet: Vec<String> = newer.iter().filter(|r| r.notes.is_empty()).map(|r| r.version.to_string()).collect();
            if !quiet.is_empty() {
                let _ = writeln!(o, "\n{} had nothing to change for an existing game (the kit's CHANGELOG.md says what's new).", quiet.join(", "));
            }
        }
    }

    if findings.is_empty() {
        let _ = writeln!(o, "\nThis game's code and manifest: nothing to suggest.");
    } else {
        let _ = writeln!(o, "\nIn this game's code and manifest:");
        for f in findings {
            let at = if f.at.is_empty() { String::new() } else { format!(" ({})", f.at.join(", ")) };
            let tag = match (f.applied, &f.fix, f.ask_owner) {
                (true, _, _) => " [done by --apply]",
                (false, Some(_), _) => " [--apply does it]",
                (_, _, true) => " [ask the owner first]",
                _ => "",
            };
            let _ = writeln!(o, "- {}: {}{at}{tag}\n  {}", f.group.name(), f.title, f.advice);
        }
    }
    let _ = writeln!(
        o,
        "\nFix what's Required, then `mb build` and `cargo test`. Recommended and Available items are suggestions: ask the owner before adopting anything that changes how the game plays or looks."
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mb-upgrade-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    const LOCK: &str = "version = 4\n\n[[package]]\nname = \"maimbrain\"\nversion = \"0.8.0\"\nsource = \"git+https://github.com/terracatta/maimbrain-kit#0123456789abcdef0123456789abcdef01234567\"\n\n[[package]]\nname = \"hop\"\nversion = \"0.1.0\"\n";

    #[test]
    fn a_git_game_reads_its_lock_and_a_path_game_has_nothing_to_bump() {
        let d = scratch("git");
        std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"hop\"\nversion = \"0.1.0\"\n\n[dependencies]\nmaimbrain = { git = \"https://github.com/terracatta/maimbrain-kit\" }\n\n[workspace]\n").unwrap();
        let kit = Version::parse("0.8.2").unwrap();
        let s = sdk(&d, None).unwrap();
        assert_eq!((s.dependency.clone(), s.version.clone()), (DepKind::Git, None));
        assert_eq!(lock_plan(&d, &s, &kit).action, "none", "no lock yet");
        std::fs::write(d.join("Cargo.lock"), LOCK).unwrap();
        let s = sdk(&d, None).unwrap();
        assert_eq!(s.version, Version::parse("0.8.0"));
        assert_eq!(s.rev.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
        assert_eq!(lock_plan(&d, &s, &kit).action, "update");
        assert_eq!(lock_plan(&d, &s, &Version::parse("0.8.0").unwrap()).action, "none");
        // Pinned to a tag: it won't move by itself.
        std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"hop\"\nversion = \"0.1.0\"\n\n[dependencies]\nmaimbrain = { git = \"https://github.com/terracatta/maimbrain-kit\", tag = \"v0.8.0\" }\n\n[workspace]\n").unwrap();
        let plan = lock_plan(&d, &sdk(&d, None).unwrap(), &kit);
        assert_eq!(plan.action, "manual");
        assert!(plan.message.contains("tag = \"v0.8.0\""), "{}", plan.message);

        // A member of a workspace whose SDK is a path: its version, and nothing to do.
        let w = scratch("path");
        std::fs::create_dir_all(w.join("sdk/maimbrain")).unwrap();
        std::fs::create_dir_all(w.join("games/hop/src")).unwrap();
        std::fs::write(w.join("Cargo.toml"), "[workspace]\nmembers = [\"games/*\"]\n").unwrap();
        std::fs::write(w.join("sdk/maimbrain/Cargo.toml"), "[package]\nname = \"maimbrain\"\nversion = \"0.8.2\"\n").unwrap();
        std::fs::write(w.join("games/hop/Cargo.toml"), "[package]\nname = \"hop\"\n\n[dependencies]\nmaimbrain = { path = \"../../sdk/maimbrain\" }\n").unwrap();
        let s = sdk(&w.join("games/hop"), None).unwrap();
        assert_eq!((s.dependency.clone(), s.version.clone()), (DepKind::Path, Version::parse("0.8.2")));
        let plan = lock_plan(&w.join("games/hop"), &s, &kit);
        assert_eq!(plan.action, "none");
        assert!(plan.message.contains("nothing to bump"), "{}", plan.message);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&w);
    }
}
