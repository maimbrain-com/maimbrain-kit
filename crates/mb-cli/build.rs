// Embeds what `mb new` and `mb serve` need, so the binary works anywhere:
// - the maimbrain-game skill's template/ and template-3d/ (under
//   .claude/skills in the monorepo, under skills/ in the public
//   maimbrain-kit repo)
// - its genre starter kits (kits/<genre>/, `mb new --kit`): every file of
//   each kit, listed in $OUT_DIR/kits.rs
// - its helper scripts (sfx.py, pixel.py, check_audio.py, mix_check.py, and make_glb.py and lut.py
//   for 3D), also in kits.rs: every new game gets copies in tools/
// - the built web runtime (runtime/dist in the monorepo, runtime-dist/ in
//   the kit). Without it, `mb serve` says how to build it.
// - what `mb --version` names besides the version: the commit it was built
//   from (git's short sha of this checkout, the monorepo's or the kit's, or
//   MB_BUILD_ID; "dev" without either) and the embedded runtime's id (a hash
//   of its files).
//
// The runtime is usually built *after* mb-cli has compiled once (`just
// runtime` itself runs `mb abi`), so the files are watched even while they
// don't exist yet: Cargo reruns a build script whose rerun-if-changed path is
// missing, so the next `cargo run -p mb-cli` after `just runtime` embeds it.
use std::path::{Path, PathBuf};

const RUNTIME_FILES: [&str; 6] = ["index.html", "runtime.js", "probe-worker.js", "headless.js", "mb_host_bg.wasm", "silence.wav"];

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").canonicalize().unwrap();
    // (skill dir, built runtime) for the monorepo, then for the kit.
    let layouts = [(".claude/skills/maimbrain-game", "runtime/dist"), ("skills/maimbrain-game", "runtime-dist")];
    let (skill, dist) = layouts
        .iter()
        .map(|(s, d)| (root.join(s), root.join(d)))
        .find(|(s, _)| s.join("template/manifest.toml").exists())
        .expect("can't find the maimbrain-game skill's template/");
    for t in ["template", "template-3d"] {
        let dir = skill.join(t).canonicalize().unwrap_or_else(|e| panic!("{}: {e}", skill.join(t).display()));
        println!("cargo:rerun-if-changed={}", dir.display());
        let var = if t == "template" { "MB_TEMPLATE_DIR" } else { "MB_TEMPLATE_3D_DIR" };
        println!("cargo:rustc-env={var}={}", dir.display());
    }

    kits(&skill);

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("runtime");
    std::fs::create_dir_all(&out).unwrap();
    let present = RUNTIME_FILES.iter().all(|f| dist.join(f).is_file());
    let mut runtime_id = Fnv::new();
    for f in RUNTIME_FILES {
        let src = dist.join(f);
        // Watched whether or not it exists yet (see the top of this file).
        println!("cargo:rerun-if-changed={}", src.display());
        let dest = out.join(f);
        if present {
            std::fs::copy(&src, &dest).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
            runtime_id.add(f.as_bytes());
            runtime_id.add(&std::fs::read(&src).unwrap());
        } else {
            std::fs::write(&dest, b"").unwrap();
        }
    }
    println!("cargo:rustc-env=MB_RUNTIME_EMBEDDED={}", if present { "1" } else { "" });
    println!("cargo:rustc-env=MB_RUNTIME_DIST={}", dist.display());
    println!("cargo:rustc-env=MB_RUNTIME_ID={}", if present { format!("{:08x}", runtime_id.0 >> 32) } else { "none".into() });
    println!("cargo:rustc-env=MB_BUILD_ID={}", build_id(&root));
}

/// FNV-1a (64-bit), for the runtime's id.
struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn add(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
}

/// The commit this is built from: MB_BUILD_ID if set (CI), else git's short
/// sha of the checkout at `root`, else "dev". Rebuilt when HEAD moves.
fn build_id(root: &Path) -> String {
    println!("cargo:rerun-if-env-changed=MB_BUILD_ID");
    if let Some(id) = std::env::var("MB_BUILD_ID").ok().filter(|s| !s.trim().is_empty()) {
        return id.trim().to_string();
    }
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git").arg("-C").arg(root).args(args).output().ok().filter(|o| o.status.success())?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
    };
    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]) else { return "dev".into() };
    // HEAD, and the branch it points at, loose or packed (in a worktree they
    // live in different git directories). Only files that exist: a missing
    // one would rerun this script on every build.
    let mut watch = vec!["HEAD".to_string(), "packed-refs".to_string()];
    watch.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for w in watch {
        if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", &w]).filter(|p| Path::new(p).is_file()) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    sha
}

/// Writes $OUT_DIR/kits.rs: `KITS`, one entry per kits/<genre>/ directory
/// (sorted), with its kit.toml summary and tech and every file to copy
/// (everything except kit.toml, screenshots/, target/ and dotfiles), and
/// `SCRIPTS` and `SCRIPTS_3D`, the skill's helper scripts every new game
/// (and every new 3D game) gets in tools/.
fn kits(skill: &Path) {
    let dir = skill.join("kits");
    // A directory: Cargo rescans everything under it for changes.
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("Cargo.toml").is_file()).collect())
        .unwrap_or_default();
    dirs.sort();
    let mut code = String::from("pub static KITS: &[Kit] = &[\n");
    for kit in dirs {
        let name = kit.file_name().unwrap().to_str().unwrap().to_string();
        let meta = std::fs::read_to_string(kit.join("kit.toml")).unwrap_or_default();
        let field = |key: &str| {
            meta.lines()
                .filter_map(|l| l.split_once('='))
                .find(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
                .unwrap_or_default()
        };
        let mut files = Vec::new();
        collect(&kit, &kit, &mut files);
        files.sort();
        code += &format!("    Kit {{ name: {name:?}, summary: {:?}, tech: {:?}, files: &[\n", field("summary"), field("tech"));
        for (rel, abs) in files {
            code += &format!("        ({rel:?}, include_bytes!({abs:?})),\n");
        }
        code += "    ] },\n";
    }
    code += "];\n";
    for (name, scripts) in [("SCRIPTS", &["sfx.py", "pixel.py", "check_audio.py", "mix_check.py"][..]), ("SCRIPTS_3D", &["make_glb.py", "lut.py"])] {
        code += &format!("\npub const {name}: &[(&str, &[u8])] = &[\n");
        for s in scripts {
            let p = skill.join("scripts").join(s);
            println!("cargo:rerun-if-changed={}", p.display());
            code += &format!("    ({s:?}, include_bytes!({:?})),\n", p.display().to_string());
        }
        code += "];\n";
    }
    std::fs::write(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("kits.rs"), code).unwrap();
}

/// Every file under `dir` as (path relative to `root` with `/`, absolute path).
/// Honors the simple `.gitignore` files kits keep (e.g. art/.gitignore leaving
/// out raw/, variants/, preview/ and unpacked sprites/*.png) and keeps those
/// `.gitignore` files themselves, so a new game ignores the same things.
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    collect_in(root, dir, &[], out)
}

/// One `.gitignore` rule: the directory it lives in, and its pattern.
type Ignore = (PathBuf, String);

fn collect_in(root: &Path, dir: &Path, ignores: &[Ignore], out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut ignores = ignores.to_vec();
    if let Ok(text) = std::fs::read_to_string(dir.join(".gitignore")) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            ignores.push((dir.to_path_buf(), line.to_string()));
        }
    }
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let name = e.file_name().to_string_lossy().to_string();
        let skip = (name.starts_with('.') && name != ".gitignore")
            || name == "__pycache__"
            || ["kit.toml", "screenshots", "target"].contains(&rel.as_str())
            || ignores.iter().any(|(base, pat)| ignored(base, pat, &p));
        if skip {
            continue;
        }
        if p.is_dir() {
            collect_in(root, &p, &ignores, out);
        } else {
            out.push((rel, p.display().to_string()));
        }
    }
}

/// Whether `path` matches a `.gitignore` pattern from `base`: `name/` (a
/// directory anywhere below), `name` (anything anywhere below), or a path like
/// `sprites/*.png` relative to `base` with `*` globs.
fn ignored(base: &Path, pat: &str, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(base) else { return false };
    let rel = rel.to_string_lossy().replace('\\', "/");
    let (pat, dir_only) = match pat.strip_suffix('/') {
        Some(p) => (p, true),
        None => (pat, false),
    };
    if dir_only && !path.is_dir() {
        return false;
    }
    let pat = pat.trim_start_matches('/');
    if pat.contains('/') {
        glob(pat, &rel)
    } else {
        glob(pat, rel.rsplit('/').next().unwrap_or(&rel))
    }
}

/// `*` matches any run of characters except `/`; everything else is literal.
fn glob(pat: &str, s: &str) -> bool {
    match pat.split_once('*') {
        None => pat == s,
        Some((head, tail)) => {
            let Some(rest) = s.strip_prefix(head) else { return false };
            (0..=rest.len()).filter(|&i| rest.is_char_boundary(i)).take_while(|&i| !rest[..i].contains('/')).any(|i| glob(tail, &rest[i..]))
        }
    }
}
