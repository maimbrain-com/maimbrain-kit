//! `mb studio-check <project> [--json]`: a Studio 2.0 project's Lua checked
//! without building it, for the Studio server's `write_files` (docs/STUDIO-2.md
//! §4.13):
//!
//! - **syntax**: each `game/**/*.lua` compiled with the engine's pinned Luau
//!   and options, as `mb build` compiles it; a syntax error has its file and
//!   line. The server refuses nothing for it, but doesn't play a change that
//!   doesn't compile.
//! - **types**: the files type-checked against the API's definitions
//!   (`types/*.d.luau`) by luau-analyze's checker, the old solver in
//!   nonstrict mode (Decided 33): unknown API names, wrong enum strings, a
//!   string for a number, each with its file, line and column. Reported,
//!   never blocking. A file with a syntax error isn't type-checked.
//!
//!     { "ok": true, "syntax": [{ "path", "line", "message" }],
//!       "types": [{ "path", "line", "column", "message" }], "typed": true }
//!
//! `typed` is false when this mb has no definitions (the creator kit without
//! engine 2.0), so nothing was type-checked.
//!
//! - **shaders**: each `shaders/*.wgsl` through the checks a phone runs
//!   (crate `mb-shader`, docs/STUDIO-2.md §4.15): a refused shader is a
//!   problem in `syntax` with its line (it holds the play-test back, as a
//!   Lua syntax error does); each one that passes is listed in `shaders`
//!   with its cost per pixel against the limit, and the loop that costs
//!   the most:
//!
//!     "shaders": [{ "path", "kind", "cost", "limit", "loops",
//!                   "heaviest": { "line", "trips", "cost" } | null }]

use std::path::Path;

use serde_json::json;

/// The most type errors reported: past that, the first ones are what to fix.
const MAX_TYPES: usize = 40;

/// The project's Lua: (path from the project, source), sorted.
pub fn lua_files(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("game")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "lua") {
                let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                let bytes = std::fs::read(&p).map_err(|e| format!("{rel}: {e}"))?;
                out.push((rel, String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The checks' report, as JSON.
pub fn check(dir: &Path) -> Result<serde_json::Value, String> {
    let files = lua_files(dir)?;
    let mut syntax = Vec::new();
    let mut broken = std::collections::BTreeSet::new();
    for (rel, src) in &files {
        if let Err(e) = luau::compile(src.as_bytes()) {
            syntax.push(json!({ "path": rel, "line": e.line, "message": e.message }));
            broken.insert(rel.as_str());
        }
    }
    let defs = super::v2::types();
    let mut types = Vec::new();
    if !defs.is_empty() {
        let list: Vec<(&str, &str)> = files.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
        for e in luau::analyze(defs, &list) {
            if broken.contains(e.file.as_str()) || types.len() >= MAX_TYPES {
                continue;
            }
            types.push(json!({ "path": e.file, "line": e.line, "column": e.column, "message": e.message }));
        }
    }
    let mut shaders = Vec::new();
    // Everything under shaders/: a shader is shaders/<name>.wgsl (v2::is_shader_path), and anything else there is refused, as mb build refuses it.
    let mut paths = Vec::new();
    let mut stack = vec![dir.join("shaders")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if e.path().is_dir() { stack.push(e.path()) } else { paths.push(e.path()) }
        }
    }
    paths.sort();
    for p in paths {
        let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
        if !super::v2::is_shader_path(&rel) {
            syntax.push(json!({ "path": rel, "line": 1, "message": "a shader is shaders/<name>.wgsl: no subfolders, and .wgsl in lowercase" }));
            continue;
        }
        let bytes = std::fs::read(&p).map_err(|e| format!("{rel}: {e}"))?;
        let src = String::from_utf8_lossy(&bytes).into_owned();
        let kind = super::v2::shader_kind(&src);
        match mb_shader::check(&src, kind, &mb_shader::Limits::default()) {
            Ok(c) => shaders.push(json!({
                "path": rel,
                "kind": if kind == mb_shader::Kind::Sprite { "sprite" } else { "surface" },
                "cost": c.cost_fragment.max(c.cost_vertex),
                "limit": mb_shader::MAX_COST,
                "loops": c.loops_capped,
                "heaviest": c.heaviest.map(|h| json!({ "line": h.line, "trips": h.trips, "cost": h.cost })),
            })),
            Err(e) => syntax.push(json!({ "path": rel, "line": e.line.unwrap_or(1), "message": format!("shader refused ({}): {}", e.code, e.message) })),
        }
    }
    Ok(json!({ "ok": syntax.is_empty(), "syntax": syntax, "types": types, "typed": !defs.is_empty(), "shaders": shaders }))
}

pub fn run(dir: &Path, json: bool) -> Result<(), String> {
    if !super::is_project(dir) {
        return Err(format!("{}: not a Studio project (no studio.toml)", dir.display()));
    }
    let r = check(dir)?;
    if json {
        println!("{r}");
        return Ok(());
    }
    for e in r["syntax"].as_array().into_iter().flatten() {
        println!("{}:{}: {}", e["path"].as_str().unwrap_or(""), e["line"], e["message"].as_str().unwrap_or(""));
    }
    for e in r["types"].as_array().into_iter().flatten() {
        println!("{}:{}:{}: {}", e["path"].as_str().unwrap_or(""), e["line"], e["column"], e["message"].as_str().unwrap_or(""));
    }
    for e in r["shaders"].as_array().into_iter().flatten() {
        println!("{}: cost {} of {} a pixel", e["path"].as_str().unwrap_or(""), e["cost"], e["limit"]);
    }
    if r["ok"] == true { Ok(()) } else { Err("the Lua doesn't compile, or a shader was refused".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A syntax error has its line; type errors have theirs and a column,
    /// and a file that doesn't compile isn't type-checked.
    #[test]
    fn syntax_and_type_errors_with_their_lines() {
        if super::super::v2::types().is_empty() {
            eprintln!("skipped: this mb has no engine 2.0 type definitions");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mb-studio-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::write(dir.join("studio.toml"), "title = \"T\"\nengine = \"2.0\"\n").unwrap();
        std::fs::write(dir.join("game/main.lua"), "function love.draw()\n  love.graphics.rectangel(\"fill\", 0, 0, 10, 10)\nend\n").unwrap();
        std::fs::write(dir.join("game/broken.lua"), "fx.shaek(1)\nlocal x = = 1\n").unwrap();
        let r = check(&dir).unwrap();
        assert_eq!(r["ok"], false);
        assert_eq!(r["syntax"][0]["path"], "game/broken.lua");
        assert_eq!(r["syntax"][0]["line"], 2);
        let types = r["types"].as_array().unwrap();
        assert_eq!(types.len(), 1, "{types:?}");
        assert_eq!((types[0]["path"].as_str(), types[0]["line"].as_u64()), (Some("game/main.lua"), Some(2)));
        assert!(types[0]["column"].as_u64().unwrap() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The starters are clean.
    #[test]
    fn the_starters_check_clean() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/studio-engine2/starters");
        if !root.is_dir() || super::super::v2::types().is_empty() {
            return;
        }
        for e in std::fs::read_dir(&root).unwrap().flatten() {
            let r = check(&e.path()).unwrap();
            assert_eq!((r["syntax"].as_array().unwrap().len(), r["types"].as_array().unwrap().len()), (0, 0), "{}: {r}", e.path().display());
        }
    }

    /// A project's shaders are checked as the phone checks them: a refusal
    /// is a problem with its line; a passing one has its cost.
    #[test]
    fn shaders_are_checked() {
        let dir = std::env::temp_dir().join(format!("mb-check-shaders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("shaders")).unwrap();
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::write(dir.join("game/main.lua"), "local x = 1\n").unwrap();
        std::fs::write(dir.join("shaders/ok.wgsl"), "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    o.albedo = in.color.rgb;\n    o.normal = in.normal;\n    return o;\n}\n").unwrap();
        std::fs::write(dir.join("shaders/spin.wgsl"), "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    var x = 0.0;\n    loop {\n        x = sin(x) + cos(x) + sin(x * 2.0) + cos(x * 3.0);\n        if (x > 9.0) { break; }\n    }\n    return o;\n}\n").unwrap();
        let r = check(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r["ok"], false);
        let s = &r["syntax"][0];
        assert_eq!((s["path"].as_str(), s["line"].as_u64()), (Some("shaders/spin.wgsl"), Some(4)), "{r}");
        assert!(s["message"].as_str().unwrap().contains("the limit is 4000"), "{r}");
        assert_eq!(r["shaders"][0]["path"], "shaders/ok.wgsl");
        assert!(r["shaders"][0]["cost"].as_u64().unwrap() < 200);
    }
}
