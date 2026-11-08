//! S13's corpus (tests/corpus, from the tag spike/s13-shaders-final) through
//! the shipped checks: every runaway and abuse case refused or capped as
//! S13 found, every realistic one passing at S13's costs.
//! `cargo test -p mb-shader -- --nocapture` prints the table.

use super::*;
use std::path::PathBuf;

/// S13's `unrolled(n)` (tools/gen.py): the two biggest aren't kept as files.
fn unrolled(n: usize) -> String {
    let mut lines = vec!["    var x = in.uv.x;".to_string(), "    var y = in.uv.y;".to_string()];
    for i in 0..n {
        lines.push(format!("    x = sin(x * {:.2} + y); y = fract(y * 1.3 + x);", 1.0 + (i % 7) as f64 * 0.01));
    }
    lines.push("    let c = vec3<f32>(x, y, 0.5);".into());
    format!(
        "\nfn surface(in: Surface) -> SurfaceOut {{\n{}\n\n    var o: SurfaceOut;\n    o.albedo = c;\n    o.alpha = 1.0;\n    o.emission = vec3<f32>(0.0);\n    o.roughness = 0.5;\n    o.metalness = 0.0;\n    o.normal = in.normal;\n    return o;\n\n}}\n",
        lines.join("\n")
    )
}

pub(crate) fn corpus() -> Vec<(String, String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let manifest: String = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    // Tiny JSON walk: top-level keys and each one's "expect".
    let mut out = vec![];
    for chunk in manifest.split("\n \"").skip(1) {
        let name = chunk.split('"').next().unwrap().to_string();
        let expect = chunk.split("\"expect\": \"").nth(1).unwrap().split('"').next().unwrap().to_string();
        let src = match name.as_str() {
            "unrolled_2k" => unrolled(2000),
            "unrolled_20k" => unrolled(20000),
            _ => std::fs::read_to_string(dir.join(format!("{name}.wgsl"))).unwrap(),
        };
        out.push((name, src, expect));
    }
    out
}

#[test]
fn s13_corpus() {
    let limits = Limits::default();
    let mut bad = vec![];
    let all = corpus();
    assert_eq!(all.len(), 58, "S13's 56 shaders and the review's 2");
    println!("{:<22} {:>9} {:>9} {:>6}  outcome", "shader", "frag", "vert", "loops");
    for (name, src, expect) in all {
        let r = check(&src, Kind::Surface, &limits);
        match &r {
            Ok(p) => {
                println!("{name:<22} {:>9} {:>9} {:>6}  pass ({} loops proven ≤ cap)", p.cost_fragment, p.cost_vertex, p.loops_capped, p.loops_counted);
                // What the host gets parses and validates again, around the interface.
                let full = format!("{}{CHECK_FRAME}{}\n{}\n{}", interface(Kind::Surface), bindings(p.uniforms.is_some()), p.source, check_entry_points(Kind::Surface, p.has_vertex));
                let m = naga::front::wgsl::parse_str(&full).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&full)));
                validate(&m).unwrap_or_else(|e| panic!("{name}: capped WGSL invalid: {}", e.0));
                assert_eq!(count_cap_heads(&m), (p.loops_capped, p.loops_capped), "{name}");
                assert!(p.cost_fragment.max(p.cost_vertex) <= MAX_COST, "{name}");
            }
            Err(e) => println!("{name:<22} {:>9} {:>9} {:>6}  REFUSED {}: {}", "", "", "", e.code, e.message.lines().next().unwrap_or("")),
        }
        let ok = match expect.as_str() {
            "pass" | "pass-capped" => r.is_ok(),
            "measure" => true,
            e if e.starts_with("refuse:") => r.as_ref().err().is_some_and(|x| x.code == &e[7..]),
            _ => false,
        };
        if !ok {
            bad.push(format!("{name}: expected {expect}, got {:?}", r.as_ref().map(|p| p.cost_fragment).map_err(|e| (e.code, e.message.clone()))));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

/// S13's costs for Phase 5's acceptance shaders hold within a few units
/// (the interface grew two fields).
#[test]
fn acceptance_shaders_cost_what_s13_measured() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    for (name, s13) in [("plain", 109), ("dissolve", 637), ("water", 579), ("toon", 161), ("fbm8", 2442)] {
        let src = std::fs::read_to_string(dir.join(format!("{name}.wgsl"))).unwrap();
        let c = check(&src, Kind::Surface, &Limits::default()).unwrap().cost_fragment;
        assert!(c.abs_diff(s13) <= 8, "{name}: {c}, S13 {s13}");
    }
}

/// A loop the checks cap stops at the cap: the counter test comes first in its body.
#[test]
fn a_loop_gets_its_cap_as_text() {
    let src = "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    var x = 0.0;\n    loop { x += 1.0; if (mb_u[0].x > 1e30) { break; } }\n    o.albedo = vec3<f32>(x); o.alpha = 1.0; o.normal = in.normal;\n    return o;\n}\n";
    let p = check(src, Kind::Surface, &Limits::default()).unwrap();
    assert_eq!(p.loops_capped, 1);
    assert!(p.source.contains("var mb_n1: u32 = 0u; loop { if (mb_n1 >= 256u) { break; } mb_n1 += 1u; x += 1.0;"), "{}", p.source);
    // The rest of the source is as written.
    assert!(p.source.starts_with("fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;"));
    assert_eq!(p.heaviest.as_ref().map(|h| (h.line, h.trips)), Some((4, 256)));
}

#[test]
fn counted_loops_cost_their_trips() {
    let src = "fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; var x = 0.0; for (var i = 0; i < 8; i++) { for (var j = 0; j < 4; j++) { x += 1.0; } } o.albedo = vec3<f32>(x); o.alpha = 1.0; o.normal = in.normal; return o; }";
    let p = check(src, Kind::Surface, &Limits::default()).unwrap();
    assert_eq!(p.loops_counted, 2);
    assert!(p.cost_fragment < 2000, "{}", p.cost_fragment);
}

/// Refusals say where, in the game's own file.
#[test]
fn errors_have_the_games_lines() {
    let src = "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    o.albedo = vec3<f32>(1.0) +;\n    return o;\n}\n";
    let e = check(src, Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!((e.code, e.line), ("parse", Some(3)), "{e:?}");
    let src = "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    o.albedo = nope;\n    return o;\n}\n";
    let e = check(src, Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!((e.code, e.line), ("parse", Some(3)), "{e:?}");
    let e = check("fn helper() -> f32 { return 1.0; }\n", Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "missing");
    // The cost names the loop that dominates it.
    let src = "fn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    var x = 0.0;\n    loop {\n        x = sin(x) + cos(x) + sin(x * 2.0) + cos(x * 3.0);\n        if (x > 9.0) { break; }\n    }\n    o.albedo = vec3<f32>(x);\n    return o;\n}\n";
    let e = check(src, Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!((e.code, e.line), ("cost", Some(4)), "{e:?}");
    assert!(e.message.contains("may run 256 times (the cap)"), "{}", e.message);
    // mb_ names that aren't the game's.
    let e = check("fn surface(in: Surface) -> SurfaceOut {\n var o: SurfaceOut; let t = mb_frame.time.x; return o; }", Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!((e.code, e.line), ("binding", Some(2)));
}

/// `struct Uniforms` names the uniforms; `uniforms.t` reads them.
#[test]
fn named_uniforms() {
    let src = "struct Uniforms { t: f32, edge: vec3<f32>, glow: vec4<f32> }\nfn surface(in: Surface) -> SurfaceOut {\n    var o: SurfaceOut;\n    o.albedo = uniforms.edge * uniforms.t + uniforms.glow.rgb;\n    o.normal = in.normal;\n    return o;\n}\n";
    let p = check(src, Kind::Surface, &Limits::default()).unwrap();
    let f = p.uniforms.unwrap();
    assert_eq!(f.iter().map(|f| (f.name.as_str(), f.offset, f.components)).collect::<Vec<_>>(), [("t", 0, 1), ("edge", 16, 3), ("glow", 32, 4)]);
    let e = check("struct Uniforms { n: i32 }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "uniforms");
    let e = check("struct Uniforms { a: array<vec4<f32>, 17> }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", Kind::Surface, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "uniforms");
}

#[test]
fn sprite_shaders() {
    let src = "fn sprite(in: Sprite) -> vec4<f32> {\n    let c = textureSample(mb_tex0, mb_samp, in.uv) * in.color;\n    let w = 0.5 + 0.5 * sin(in.time * 4.0 + in.position.x * 0.05);\n    return vec4<f32>(c.rgb + vec3<f32>(w * 0.3), c.a);\n}\n";
    let p = check(src, Kind::Sprite, &Limits::default()).unwrap();
    assert!(p.cost_fragment > 256 && p.cost_fragment < 400, "{}", p.cost_fragment);
    assert!(!p.has_vertex);
    let e = check("fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", Kind::Sprite, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "missing", "{e:?}");
    let e = check("fn sprite(in: Surface) -> vec4<f32> { return vec4<f32>(1.0); }", Kind::Sprite, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "parse", "{e:?}"); // no Surface in a sprite's interface
    let e = check("fn sprite(in: Sprite) -> vec4<f32> { var c = vec4<f32>(0.0); loop { c += textureSample(mb_tex0, mb_samp, c.xy); } return c; }", Kind::Sprite, &Limits::default()).unwrap_err();
    assert_eq!(e.code, "cost");
}

/// Names the host's WGSL uses (`mb_…`, `Mb…`, `MB_…`) are refused before
/// they could collide with it on the phone.
#[test]
fn host_prefixes_are_refused() {
    for (src, name) in [
        ("struct MbObject { a: f32 }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", "MbObject"),
        ("const MB_PI: f32 = 3.0;\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", "MB_PI"),
        ("fn mb_lit() -> f32 { return 1.0; }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }", "mb_lit"),
        ("struct MbGlobals { t: f32 }\nfn sprite(in: Sprite) -> vec4<f32> { return in.color; }", "MbGlobals"),
    ] {
        let kind = if src.contains("fn sprite") { Kind::Sprite } else { Kind::Surface };
        let e = check(src, kind, &Limits::default()).unwrap_err();
        assert_eq!((e.code, e.line), ("binding", Some(1)), "{name}: {e:?}");
        assert!(e.message.contains(name), "{e:?}");
    }
    assert!(check("fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; o.albedo = textureSample(mb_tex0, mb_samp, in.uv).rgb * mb_u[0].x; return o; }", Kind::Surface, &Limits::default()).is_ok());
}
