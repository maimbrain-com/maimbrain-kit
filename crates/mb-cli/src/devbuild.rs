//! The fast build `mb serve --watch` runs after every edit.
//!
//! `mb build` is the build you ship: release profile (fat LTO, one codegen
//! unit), wasm-opt -O3, pack, full validation. Most of its time goes to LTO
//! and wasm-opt. Here the game is compiled with its own release settings
//! except for those (profile `mb-watch`: release without LTO, 16 codegen
//! units, incremental), so a one-line change recompiles in well under a
//! second, and its artifacts live next to the release ones without
//! invalidating them. Then only what would stop the game running is checked:
//! the manifest, the wasm's imports and exports against the host table, and
//! loop metering. Bundle-level checks (sizes, the icon, packing) are left to
//! `mb build`, which `mb publish` runs anyway.

use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mb_format::Manifest;

/// The cargo profile for watch builds (see the top of this file).
pub const PROFILE: &str = "mb-watch";

/// A game's build lock, held from re-baking its fonts until its wasm is read
/// (dropping it frees it; so does the OS if `mb` dies). `mb shot`, `mb
/// measure`, `mb serve` and `mb build` on one game at once take turns: cargo
/// locks its target directory only while it runs, so without this one run's
/// cargo (another profile, `--ab`'s patched SDK, `mb build`'s wasm-opt) could
/// rewrite the wasm another is reading, or two runs bake the same font file.
/// The one that waited finds the build up to date and reuses it.
pub struct BuildLock(#[allow(dead_code)] Option<std::fs::File>);

/// Takes `dir`'s build lock, waiting (with a note) while another `mb` run
/// holds it. A lock that can't be made is no lock: the build goes ahead.
pub fn lock(dir: &Path) -> BuildLock {
    let Some(path) = lock_path(dir) else { return BuildLock(None) };
    let Ok(f) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path) else { return BuildLock(None) };
    match f.try_lock() {
        Ok(()) => BuildLock(Some(f)),
        Err(std::fs::TryLockError::WouldBlock) => {
            eprintln!("another mb run is building {}: waiting for it, then using its build", dir.display());
            match f.lock() {
                Ok(()) => BuildLock(Some(f)),
                Err(_) => BuildLock(None),
            }
        }
        Err(_) => BuildLock(None),
    }
}

/// Where `dir`'s build lock lives: one file per game directory, in the temp directory.
fn lock_path(dir: &Path) -> Option<PathBuf> {
    let canon = dir.canonicalize().ok()?;
    // FNV-1a: stable across runs and mb versions.
    let h = canon.to_string_lossy().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    let locks = std::env::temp_dir().join("mb-build-locks");
    std::fs::create_dir_all(&locks).ok()?;
    let name = canon.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some(locks.join(format!("{name}-{h:016x}.lock")))
}

/// A compiler diagnostic, for the status line and the preview's overlay.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Diag {
    pub level: String,
    /// e.g. "E0308"; empty when rustc gives none.
    pub code: String,
    pub message: String,
    /// The primary span, as cargo reports it (relative to the workspace root).
    pub file: String,
    pub line: u32,
    pub col: u32,
    /// The full human-readable diagnostic, without colour codes.
    pub rendered: String,
}

impl Diag {
    /// `src/sim.rs:12:5: error[E0308]: mismatched types`
    pub fn summary(&self) -> String {
        let code = if self.code.is_empty() { String::new() } else { format!("[{}]", self.code) };
        let at = if self.file.is_empty() { String::new() } else { format!("{}:{}:{}: ", self.file, self.line, self.col) };
        format!("{at}{}{code}: {}", self.level, self.message)
    }
}

/// Why a watch build failed: compiler errors, or a plain message (a bad
/// manifest, cargo itself failing, a wasm the host would refuse).
#[derive(Debug, Clone, Default)]
pub struct Failure {
    pub errors: Vec<Diag>,
    /// Everything to show: the rendered errors, or cargo's own output.
    pub text: String,
}

impl Failure {
    pub fn message(file: &str, text: String) -> Failure {
        let d = Diag { level: "error".into(), code: String::new(), message: text.clone(), file: file.into(), line: 0, col: 0, rendered: text.clone() };
        Failure { errors: vec![d], text }
    }

    /// One line for the status output.
    pub fn summary(&self) -> String {
        match self.errors.first() {
            Some(d) => {
                let first = if d.line == 0 && !d.file.is_empty() {
                    format!("{}: {}", d.file, d.message.lines().next().unwrap_or(""))
                } else {
                    d.summary()
                };
                if self.errors.len() > 1 { format!("{first} (+{} more)", self.errors.len() - 1) } else { first }
            }
            None => self.text.lines().find(|l| l.starts_with("error")).unwrap_or("build failed").to_string(),
        }
    }
}

/// A built, metered game ready to serve.
pub struct Built {
    pub manifest: Manifest,
    /// game.wasm with loop metering, as the client serves it.
    pub wasm: Vec<u8>,
    /// Seconds spent in cargo.
    pub cargo_s: f64,
}

/// Parses the manifest and checks its fields.
pub fn manifest(dir: &Path) -> Result<Manifest, Failure> {
    let path = dir.join("manifest.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| Failure::message("manifest.toml", format!("{}: {e}", path.display())))?;
    let m = Manifest::parse(&text).map_err(|e| Failure::message("manifest.toml", format!("manifest.toml: {e}")))?;
    let mut errors = Vec::new();
    m.check(&mut errors);
    if errors.is_empty() { Ok(m) } else { Err(Failure::message("manifest.toml", errors.join("\n"))) }
}

/// Compiles the game in `dir` (package `name`) with the watch profile and
/// checks the result the way the runtime would.
pub fn build(dir: &Path, name: &str) -> Result<Built, Failure> {
    build_with(dir, name, &[])
}

/// `build`, with extra `--config` settings for cargo (e.g. a `patch` that
/// points the SDK at another kit's).
pub fn build_with(dir: &Path, name: &str, config: &[String]) -> Result<Built, Failure> {
    let manifest = manifest(dir)?;
    let started = std::time::Instant::now();
    let wasm_path = cargo(dir, name, &manifest, config)?;
    let cargo_s = started.elapsed().as_secs_f64();
    let raw = std::fs::read(&wasm_path).map_err(|e| Failure::message("", format!("{}: {e}", wasm_path.display())))?;
    let mut errors = Vec::new();
    mb_format::wasm::check(&raw, &manifest, &mut errors);
    for asset in &manifest.startup_assets {
        if !dir.join(asset).is_file() {
            errors.push(format!("manifest: startup asset {asset} doesn't exist"));
        }
    }
    if !errors.is_empty() {
        return Err(Failure::message("game.wasm", errors.join("\n")));
    }
    let wasm = mb_format::meter::instrument(&raw).map_err(|e| Failure::message("game.wasm", e))?;
    Ok(Built { manifest, wasm, cargo_s })
}

/// The cargo arguments for a watch build (also what the tests check).
pub fn cargo_args(dir: &Path, name: &str, color: bool) -> Vec<String> {
    let p = PROFILE;
    let mut args: Vec<String> = vec!["build".into(), "--manifest-path".into(), dir.join("Cargo.toml").to_string_lossy().into_owned()];
    args.extend(["-p", name, "--target", "wasm32-unknown-unknown", "--profile", p].map(String::from));
    // A custom profile defined on the command line, so a game needs nothing in its Cargo.toml.
    for setting in [format!("profile.{p}.inherits=\"release\""), format!("profile.{p}.lto=false"), format!("profile.{p}.codegen-units=16"), format!("profile.{p}.incremental=true")] {
        args.push("--config".into());
        args.push(setting);
    }
    args.push("--message-format=json-diagnostic-rendered-ansi".into());
    args.push(format!("--color={}", if color { "always" } else { "never" }));
    args
}

fn cargo(dir: &Path, name: &str, manifest: &Manifest, config: &[String]) -> Result<PathBuf, Failure> {
    let max_memory = manifest.perf_tier.max_memory_pages() * 65536;
    // Colour for a person at a terminal (Windows consoles may not take ANSI codes from us).
    let color = std::io::stderr().is_terminal() && !cfg!(windows);
    let mut child = Command::new("cargo")
        .args(cargo_args(dir, name, color))
        .args(config.iter().flat_map(|c| ["--config", c.as_str()]))
        // Exactly what `mb build` sets (SPEC §3), so dependencies aren't rebuilt for a flag change.
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("-C link-arg=--max-memory={max_memory}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Failure::message("", format!("running cargo: {e}")))?;
    // cargo's own messages (Compiling …, manifest errors) go to stderr: pass them through and keep them.
    let mut err_pipe = child.stderr.take().unwrap();
    let stderr = std::thread::spawn(move || {
        let mut all = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = err_pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            let _ = std::io::stderr().write_all(&buf[..n]);
            all.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&all).into_owned()
    });
    let mut diags = Vec::new();
    let mut wasm = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        match parse_message(&line, name) {
            Some(Message::Diag(d, rendered)) => {
                // Shown like cargo shows it.
                let _ = std::io::stderr().write_all(if color { rendered.as_bytes() } else { d.rendered.as_bytes() });
                if d.level == "error" {
                    diags.push(d);
                }
            }
            Some(Message::Artifact(p)) => wasm = Some(p),
            None => {}
        }
    }
    let status = child.wait().map_err(|e| Failure::message("", format!("cargo: {e}")))?;
    let stderr = strip_ansi(&stderr.join().unwrap_or_default());
    match (status.success(), wasm) {
        (true, Some(p)) => Ok(p),
        (true, None) => Err(Failure::message("Cargo.toml", format!("cargo built no wasm for {name}: is it a cdylib ([lib] crate-type = [\"cdylib\"])?"))),
        (false, _) if !diags.is_empty() => {
            let text = diags.iter().map(|d| d.rendered.trim_end()).collect::<Vec<_>>().join("\n\n");
            Err(Failure { errors: diags, text })
        }
        (false, _) => {
            // No compiler errors: cargo itself failed (a bad Cargo.toml, a missing target…).
            let text = stderr.lines().filter(|l| !l.trim_start().starts_with("Compiling ")).collect::<Vec<_>>().join("\n");
            let first = text.lines().find(|l| l.starts_with("error")).unwrap_or("cargo build failed").to_string();
            let d = Diag { level: "error".into(), code: String::new(), message: first, file: String::new(), line: 0, col: 0, rendered: text.clone() };
            Err(Failure { errors: vec![d], text })
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    /// A diagnostic, and its rendering with colour codes.
    Diag(Diag, String),
    /// The game's wasm.
    Artifact(PathBuf),
}

/// One line of `cargo --message-format=json-diagnostic-rendered-ansi`.
pub fn parse_message(line: &str, package: &str) -> Option<Message> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    match v["reason"].as_str()? {
        "compiler-message" => {
            let m = &v["message"];
            let level = m["level"].as_str().unwrap_or("error");
            if !matches!(level, "error" | "warning") {
                return None;
            }
            let rendered = m["rendered"].as_str().unwrap_or("").to_string();
            let span = m["spans"].as_array().and_then(|s| s.iter().find(|s| s["is_primary"].as_bool() == Some(true)));
            let d = Diag {
                level: level.into(),
                code: m["code"]["code"].as_str().unwrap_or("").into(),
                message: m["message"].as_str().unwrap_or("").into(),
                file: span.and_then(|s| s["file_name"].as_str()).unwrap_or("").into(),
                line: span.and_then(|s| s["line_start"].as_u64()).unwrap_or(0) as u32,
                col: span.and_then(|s| s["column_start"].as_u64()).unwrap_or(0) as u32,
                rendered: strip_ansi(&rendered),
            };
            Some(Message::Diag(d, rendered))
        }
        "compiler-artifact" => {
            // Cargo names the target with underscores in the file but not in `target.name`.
            let target = v["target"]["name"].as_str()?;
            if target.replace('-', "_") != package.replace('-', "_") {
                return None;
            }
            let files = v["filenames"].as_array()?;
            files.iter().filter_map(|f| f.as_str()).find(|f| f.ends_with(".wasm")).map(|f| Message::Artifact(f.into()))
        }
        _ => None,
    }
}

/// Removes ANSI escape sequences (colour codes).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // Parameters and intermediates, then one final byte in @–~.
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_compiler_error() {
        let line = r#"{"reason":"compiler-message","package_id":"x","target":{"name":"frog"},"message":{"rendered":"\u001b[1m\u001b[38;5;9merror[E0308]\u001b[0m: mismatched types\n --> src/sim.rs:12:5\n","level":"error","code":{"code":"E0308","explanation":"..."},"message":"mismatched types","spans":[{"file_name":"src/lib.rs","line_start":3,"column_start":1,"is_primary":false},{"file_name":"src/sim.rs","line_start":12,"column_start":5,"is_primary":true}],"children":[]}}"#;
        let Some(Message::Diag(d, raw)) = parse_message(line, "frog") else { panic!("not a diagnostic") };
        assert_eq!((d.file.as_str(), d.line, d.col, d.code.as_str()), ("src/sim.rs", 12, 5, "E0308"));
        assert_eq!(d.summary(), "src/sim.rs:12:5: error[E0308]: mismatched types");
        assert!(raw.contains('\u{1b}') && !d.rendered.contains('\u{1b}'));
        assert!(d.rendered.starts_with("error[E0308]: mismatched types"));
    }

    #[test]
    fn finds_the_games_wasm_and_skips_other_messages() {
        let dep = r#"{"reason":"compiler-artifact","target":{"name":"maimbrain","kind":["lib"]},"filenames":["/t/libmaimbrain.rlib"]}"#;
        let game = r#"{"reason":"compiler-artifact","target":{"name":"frog-hop","kind":["cdylib"]},"filenames":["/t/wasm32-unknown-unknown/mb-watch/frog_hop.wasm"]}"#;
        assert_eq!(parse_message(dep, "frog-hop"), None);
        assert_eq!(parse_message(game, "frog-hop"), Some(Message::Artifact("/t/wasm32-unknown-unknown/mb-watch/frog_hop.wasm".into())));
        assert_eq!(parse_message(r#"{"reason":"build-finished","success":true}"#, "frog-hop"), None);
        assert_eq!(parse_message("   Compiling frog v0.1.0", "frog"), None);
        let note = r#"{"reason":"compiler-message","message":{"rendered":"","level":"failure-note","message":"x","spans":[]}}"#;
        assert_eq!(parse_message(note, "frog"), None);
    }

    #[test]
    fn strips_colour_codes() {
        assert_eq!(strip_ansi("\u{1b}[0m\u{1b}[1m\u{1b}[38;5;12m-->\u{1b}[0m src"), "--> src");
        assert_eq!(strip_ansi("plain → text"), "plain → text");
    }

    #[test]
    fn runs_on_one_game_take_turns_building_it() {
        let game = std::env::temp_dir().join(format!("mb-lock-test-{}", std::process::id()));
        std::fs::create_dir_all(&game).unwrap();
        let first = lock(&game);
        assert!(first.0.is_some());
        let path = lock_path(&game).unwrap();
        assert_eq!(path, lock_path(&game.join(".")).unwrap(), "one lock per directory, however it's named");
        let held = std::time::Instant::now();
        let waiter = std::thread::spawn(move || {
            let l = lock(&game);
            (l.0.is_some(), game)
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(first);
        let (got, game) = waiter.join().unwrap();
        assert!(got && held.elapsed() >= std::time::Duration::from_millis(300), "the second run waited for the first");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn watch_builds_use_their_own_profile() {
        let args = cargo_args(Path::new("games/frog"), "frog", false).join(" ");
        assert!(args.contains("--profile mb-watch"));
        assert!(args.contains("--config profile.mb-watch.inherits=\"release\""));
        assert!(args.contains("profile.mb-watch.lto=false") && args.contains("profile.mb-watch.incremental=true"));
        assert!(!args.contains("--release"), "release artifacts stay as mb build left them");
    }

    #[test]
    fn failure_summaries() {
        let f = Failure::message("manifest.toml", "manifest: name must be 1–64 characters\nmanifest: inputs must list…".into());
        assert_eq!(f.summary(), "manifest.toml: manifest: name must be 1–64 characters");
        let d = |m: &str| Diag { level: "error".into(), code: String::new(), message: m.into(), file: "src/lib.rs".into(), line: 1, col: 2, rendered: String::new() };
        let f = Failure { errors: vec![d("a"), d("b"), d("c")], text: String::new() };
        assert_eq!(f.summary(), "src/lib.rs:1:2: error: a (+2 more)");
    }
}
