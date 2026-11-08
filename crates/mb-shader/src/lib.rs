//! Shader materials' checks (docs/STUDIO-2.md §4.15, Decided 49–51).
//!
//! A game's shader is WGSL that plugs into a pipeline the host owns, as
//! Godot's spatial and canvas shaders do:
//!
//! - a 3D material's (`Kind::Surface`): `fn surface(in: Surface) ->
//!   SurfaceOut`, and optionally `fn vertex(in: Vertex) -> Vertex`;
//! - a 2D sprite's (`Kind::Sprite`): `fn sprite(in: Sprite) -> vec4<f32>`.
//!
//! [`check`] is what every shader goes through before a GPU sees it, the
//! same in the app (mb-host) and in `mb` (`write_files`, `mb build`), so the
//! server refuses exactly what a phone would:
//!
//! 1. text checks before naga: the source's size and how deep its brackets
//!    nest (naga's parser recurses: 5,000 nested parentheses overflow its
//!    stack, an abort natively), and no names starting `mb_` but the ones a
//!    shader may read;
//! 2. the interface ([`interface`]) + the game's functions + a checking
//!    wrapper ([`check_entry_points`]), parsed and validated by naga with
//!    WebGPU's baseline capabilities;
//! 3. the module: only the host's bindings and entry points, no globals but
//!    `var<private>`, no atomics, storage, workgroup memory or `override`,
//!    at most 16,384 expressions and 4 KB of function-local variables;
//! 4. every loop gets an iteration cap, **inserted into the source text**
//!    (`var mb_nK: u32 = 0u;` before it, `if (mb_nK >= 256u) { break; }
//!    mb_nK += 1u;` first in its body), parsed again, and every loop must
//!    start with its cap. Not naga's IR written back out: WebKit refused
//!    three shaders as naga's writer printed them (S13);
//! 5. a static cost per invocation of the fragment function and of the
//!    vertex hook: operations × each loop's trips (256, or a proven constant
//!    for `for (var i = a; i < N; i += k)`), calls inlined, with S13's
//!    weights measured on the iPhone 16 Pro. Refused past 4,000.
//!
//! What can't be stopped statically (overdraw, a slow GPU) is stopped at run
//! time by mb-host (Decided 50).

use naga::{
    valid::{Capabilities, ModuleInfo, ValidationFlags, Validator},
    BinaryOperator, Block, Expression, Function, Handle, Literal, LocalVariable, MathFunction, Module, Span, Statement, TypeInner,
};
use std::collections::HashMap;

/// Iterations any one loop may run.
pub const LOOP_CAP: u32 = 256;
/// The static cost limit per invocation (S13: a full-screen layer at the
/// limit is ≤ 14 ms of GPU on the iPhone 16 Pro).
pub const MAX_COST: u64 = 4_000;
/// A texture sample or load: S13 measured ~250 ALU units for a dependent or
/// high-frequency sample at full screen (a coherent one ~45); the check
/// can't see the pattern, so it charges the worst.
pub const W_SAMPLE: u64 = 256;
pub const W_LOAD_IMAGE: u64 = 256;
/// Transcendental maths (sin, exp, pow, sqrt…) per component.
pub const W_TRANSCENDENTAL: u64 = 4;
/// Bytes a uniform block holds: 16 `vec4`s.
pub const UNIFORM_BYTES: u32 = 256;
/// Textures a shader may sample besides its own (a sprite's image is `mb_tex0`).
pub const TEXTURES: usize = 4;

/// Which plug-in point a shader fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A 3D material: `fn surface(in: Surface) -> SurfaceOut`, optionally `fn vertex(in: Vertex) -> Vertex`.
    Surface,
    /// A 2D sprite: `fn sprite(in: Sprite) -> vec4<f32>`.
    Sprite,
}

impl Kind {
    /// The function a shader of this kind must define.
    pub fn entry(self) -> &'static str {
        match self {
            Kind::Surface => "surface",
            Kind::Sprite => "sprite",
        }
    }
}

/// The limits a shader is held to.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub loop_cap: u32,
    /// Refuse past this static cost (`None`: report only).
    pub max_cost: Option<u64>,
    /// Source bytes, checked before parsing.
    pub max_source: usize,
    /// Bracket nesting depth, checked before parsing.
    pub max_depth: usize,
    /// Expressions in the whole module (a bound on compile time).
    pub max_expressions: usize,
    /// Bytes of function-local variables in any one function (WebKit's own limit is 8 KB).
    pub max_local_bytes: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { loop_cap: LOOP_CAP, max_cost: Some(MAX_COST), max_source: 64 * 1024, max_depth: 64, max_expressions: 16_384, max_local_bytes: 4096 }
    }
}

/// A shader that passed.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    /// The game's source with every loop's cap inserted: what the host
    /// composes between its prelude and its entry points.
    pub source: String,
    pub kind: Kind,
    /// Static cost of one fragment invocation (the game's function and the checking wrapper's few operations).
    pub cost_fragment: u64,
    /// Static cost of one vertex invocation (0 without a `vertex` hook).
    pub cost_vertex: u64,
    /// Loops given a cap, and loops whose trip count was proven below it.
    pub loops_capped: usize,
    pub loops_counted: usize,
    pub expressions: usize,
    /// A 3D shader with `fn vertex`.
    pub has_vertex: bool,
    /// The game's `struct Uniforms`, if it declares one: the fields `uniforms = { … }` sets, by name.
    pub uniforms: Option<Vec<Field>>,
    /// The loop that costs the most, if any: where it is and what it costs.
    pub heaviest: Option<Heaviest>,
}

/// A field of a shader's `struct Uniforms`: a float or a vector of 2–4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// Bytes into the uniform block.
    pub offset: u32,
    /// 1 to 4 floats.
    pub components: u32,
}

/// The loop that dominates a shader's cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heaviest {
    /// 1-based, in the game's file.
    pub line: u32,
    /// The trips it's costed at: the cap (256) unless proven fewer.
    pub trips: u64,
    /// Its share of the cost (all of its trips).
    pub cost: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// A short code: `size`, `depth`, `parse`, `invalid`, `binding`, `entry`,
    /// `atomic`, `override`, `expressions`, `locals`, `missing`, `uniforms`, `cost`, `rewrite`.
    pub code: &'static str,
    pub message: String,
    /// 1-based, in the game's file, where it's known.
    pub line: Option<u32>,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(f, "line {l}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

fn refuse(code: &'static str, message: impl Into<String>) -> Refusal {
    Refusal { code, message: message.into(), line: None }
}

fn refuse_at(code: &'static str, message: impl Into<String>, line: Option<u32>) -> Refusal {
    Refusal { code, message: message.into(), line }
}

/// What a 3D shader reads and returns. `Surface` is filled by the host for
/// every pixel; `SurfaceOut` is lit by the host as mb3d's own materials are
/// (emission is added after lighting; `alpha` blends where the material is
/// transparent). `Vertex` is in the mesh's own space.
pub const SURFACE_INTERFACE: &str = r#"struct Surface {
    uv: vec2<f32>,
    position: vec3<f32>,
    normal: vec3<f32>,
    view: vec3<f32>,
    color: vec4<f32>,
    time: f32,
    light: vec3<f32>,
    local: vec3<f32>,
}
struct SurfaceOut {
    albedo: vec3<f32>,
    alpha: f32,
    emission: vec3<f32>,
    roughness: f32,
    metalness: f32,
    normal: vec3<f32>,
}
struct Vertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    time: f32,
}
"#;

/// What a 2D sprite shader reads: its image's coordinate (`mb_tex0`'s,
/// which may be a page of an atlas), the draw's colour (LÖVE's `setColor`),
/// where it is on the canvas (logical units), the time, and where in the
/// drawn picture it is (`local`, 0–1 across it). It returns straight (not
/// premultiplied) colour.
pub const SPRITE_INTERFACE: &str = r#"struct Sprite {
    uv: vec2<f32>,
    color: vec4<f32>,
    position: vec2<f32>,
    time: f32,
    local: vec2<f32>,
}
"#;

pub fn interface(kind: Kind) -> &'static str {
    match kind {
        Kind::Surface => SURFACE_INTERFACE,
        Kind::Sprite => SPRITE_INTERFACE,
    }
}

/// The game's bindings, group 1, the same for both kinds: its uniforms
/// (`mb_u`, 16 `vec4`s, or its own `struct Uniforms` as `uniforms`), four
/// textures (a sprite's `mb_tex0` is its image) and a filtering, repeating sampler.
pub fn bindings(uniforms_struct: bool) -> String {
    let u = if uniforms_struct { "var<uniform> uniforms: Uniforms;" } else { "var<uniform> mb_u: array<vec4<f32>, 16>;" };
    format!(
        "@group(1) @binding(0) {u}
@group(1) @binding(1) var mb_tex0: texture_2d<f32>;
@group(1) @binding(2) var mb_tex1: texture_2d<f32>;
@group(1) @binding(3) var mb_tex2: texture_2d<f32>;
@group(1) @binding(4) var mb_tex3: texture_2d<f32>;
@group(1) @binding(5) var mb_samp: sampler;
"
    )
}

/// The `mb_` names a game's shader may use.
pub const GAME_NAMES: &[&str] = &["mb_u", "mb_tex0", "mb_tex1", "mb_tex2", "mb_tex3", "mb_samp"];

/// The checking wrapper's own (group 0): what its entry points read.
const CHECK_FRAME: &str = r#"struct MbFrame {
    clip: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
    time: vec4<f32>,
}
@group(0) @binding(0) var<uniform> mb_frame: MbFrame;
"#;

/// The entry points the checks compose around the game's functions (S13's):
/// a lit surface in the style of mb3d's, or a sprite. The cost of a
/// fragment is the game's function plus these few operations, which is
/// what S13's limit was measured with. The host's real entry points are its
/// own (mb-host), around the same interface and bindings.
pub fn check_entry_points(kind: Kind, has_vertex: bool) -> String {
    match kind {
        Kind::Surface => {
            let vertex_call = if has_vertex {
                "v = vertex(v); let mb_d = v.position - position; let mb_l = length(mb_d); if (!(mb_l <= mb_frame.eye.w)) { v.position = position + select(vec3<f32>(0.0), mb_d * (mb_frame.eye.w / mb_l), mb_l > 0.0 && mb_l < 3.0e38); }"
            } else {
                ""
            };
            format!(
                r#"
struct MbVaryings {{
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}}
@vertex
fn mb_vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> MbVaryings {{
    var v = Vertex(position, normal, uv, mb_frame.time.x);
    {vertex_call}
    var o: MbVaryings;
    o.clip = mb_frame.clip * vec4<f32>(v.position, 1.0);
    o.position = v.position;
    o.normal = v.normal;
    o.uv = v.uv;
    return o;
}}
@fragment
fn mb_fs(i: MbVaryings) -> @location(0) vec4<f32> {{
    let n = normalize(i.normal);
    let s = Surface(i.uv, i.position, n, normalize(mb_frame.eye.xyz - i.position), vec4<f32>(1.0), mb_frame.time.x, mb_frame.light.xyz, i.position);
    let o = surface(s);
    let l = max(dot(normalize(o.normal), normalize(mb_frame.light.xyz)), 0.0);
    let c = o.albedo * (0.25 + 0.75 * l) + o.emission;
    return vec4<f32>(c, clamp(o.alpha, 0.0, 1.0));
}}
"#
            )
        }
        Kind::Sprite => r#"
struct MbVaryings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) position: vec2<f32>,
}
@vertex
fn mb_vs(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> MbVaryings {
    var o: MbVaryings;
    o.clip = mb_frame.clip * vec4<f32>(position, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    o.position = position;
    return o;
}
@fragment
fn mb_fs(i: MbVaryings) -> @location(0) vec4<f32> {
    let c = sprite(Sprite(i.uv, i.color, i.position, mb_frame.time.x, i.uv));
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#
        .to_string(),
    }
}

/// The host's globals in the checking module: (name, group, binding).
const CHECK_BINDINGS: &[(&str, u32, u32)] = &[
    ("mb_frame", 0, 0),
    ("mb_u", 1, 0),
    ("uniforms", 1, 0),
    ("mb_tex0", 1, 1),
    ("mb_tex1", 1, 2),
    ("mb_tex2", 1, 3),
    ("mb_tex3", 1, 4),
    ("mb_samp", 1, 5),
];

/// Whether `src` declares `struct Uniforms` (its named uniforms).
pub fn declares_uniforms(src: &str) -> bool {
    words(src).collect::<Vec<_>>().windows(2).any(|w| w[0] == "struct" && w[1] == "Uniforms")
}

/// Whether `src` declares `fn vertex(` (the optional vertex hook).
pub fn declares_vertex(src: &str) -> bool {
    src.split("fn").skip(1).any(|rest| rest.trim_start().starts_with("vertex") && rest.trim_start()["vertex".len()..].trim_start().starts_with('('))
}

fn words(src: &str) -> impl Iterator<Item = &str> {
    src.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty())
}

/// Text checks before naga sees the source: its size, how deep brackets
/// nest (naga's parser and validator recurse), and the host's names.
pub fn precheck(src: &str, limits: &Limits) -> Result<(), Refusal> {
    if src.len() > limits.max_source {
        return Err(refuse("size", format!("the shader is {} KB; a shader is at most {} KB", src.len().div_ceil(1024), limits.max_source / 1024)));
    }
    let (mut depth, mut max, mut angle, mut line, mut at) = (0usize, 0usize, 0usize, 1u32, None);
    for b in src.bytes() {
        match b {
            b'\n' => line += 1,
            b'(' | b'{' | b'[' => {
                depth += 1;
                if b == b'{' {
                    angle = 0;
                }
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if b == b'}' {
                    angle = 0;
                }
            }
            // Templates (`array<array<...>>`) nest too. Comparisons count as
            // well, so this is per statement and over-counts a little.
            b'<' => angle += 1,
            b'>' => angle = angle.saturating_sub(1),
            b';' => angle = 0,
            _ => {}
        }
        if depth + angle > max {
            max = depth + angle;
            if max > limits.max_depth && at.is_none() {
                at = Some(line);
            }
        }
    }
    if max > limits.max_depth {
        return Err(refuse_at("depth", format!("brackets nest {max} deep; at most {}", limits.max_depth), at));
    }
    for (n, l) in src.lines().enumerate() {
        let code = l.split("//").next().unwrap_or("");
        if let Some(w) = words(code).find(|w| host_reserved(w)) {
            return Err(refuse_at("binding", format!("`{w}`: names starting mb_, Mb or MB_ are the host's (a shader reads mb_u, mb_tex0–3 and mb_samp)"), Some(n as u32 + 1)));
        }
    }
    Ok(())
}

/// A name the host's own WGSL may use (mb-host's shader3d.wgsl and
/// shader2d.wgsl name everything `mb_…`, `Mb…` or `MB_…`): a game's shader
/// can't, but for the bindings it reads.
pub fn host_reserved(w: &str) -> bool {
    (w.starts_with("mb_") && !GAME_NAMES.contains(&w)) || w.starts_with("Mb") || w.starts_with("MB_")
}

/// The module the checks parse: the interface, the bindings, the game's
/// source, the checking wrapper. Returns it and the line the game's source starts on (0-based).
fn compose(src: &str, kind: Kind, uniforms: bool, has_vertex: bool) -> (String, usize, usize) {
    let head = format!("{}{CHECK_FRAME}{}\n", interface(kind), bindings(uniforms));
    let start_line = head.matches('\n').count();
    let full = format!("{head}{src}\n{}", check_entry_points(kind, has_vertex));
    (full, start_line, head.len())
}

/// Checks, caps and costs a game's shader.
pub fn check(src: &str, kind: Kind, limits: &Limits) -> Result<Checked, Refusal> {
    precheck(src, limits)?;
    if !words(src).collect::<Vec<_>>().windows(2).any(|w| w[0] == "fn" && w[1] == kind.entry()) {
        return Err(missing(kind));
    }
    let has_vertex = kind == Kind::Surface && declares_vertex(src);
    let uniforms = declares_uniforms(src);
    let (full, start_line, head_len) = compose(src, kind, uniforms, has_vertex);
    let user_line = |loc_line: u32| -> Option<u32> {
        let l = loc_line as usize;
        (l > start_line && l <= start_line + src.lines().count().max(1)).then(|| (l - start_line) as u32)
    };
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| {
        let line = e.location(&full).and_then(|l| user_line(l.line_number));
        let mut msg = e.message().to_string();
        if let Some((_, label)) = e.labels().find(|(_, l)| !l.is_empty()) {
            msg.push_str(&format!(" ({label})"));
        }
        refuse_at("parse", msg, line)
    })?;
    validate(&module).map_err(|(msg, span)| refuse_at("invalid", msg, span.and_then(|s| user_line(s.location(&full).line_number))))?;
    check_module(&module, limits)?;
    if !module.functions.iter().any(|(_, f)| f.name.as_deref() == Some(kind.entry())) {
        return Err(missing(kind));
    }
    let fields = if uniforms { Some(uniform_fields(&module)?) } else { None };
    // The cap as text, at each loop's own place in the game's source.
    let (capped_full, n) = cap_source(&full, &module, limits.loop_cap, head_len..head_len + src.len())?;
    let module = naga::front::wgsl::parse_str(&capped_full).map_err(|e| refuse("rewrite", format!("the capped source doesn't parse: {}", e.emit_to_string(&capped_full))))?;
    let heads = count_cap_heads(&module);
    if heads != (n, n) {
        return Err(refuse("rewrite", format!("{n} loops but {heads:?} (loops, capped) after the rewrite")));
    }
    let info = validate(&module).map_err(|(e, _)| refuse("rewrite", format!("the capped module doesn't validate: {e}")))?;
    let mut cost = Cost::new(&module, &info, limits.loop_cap);
    let cost_fragment = cost.entry("mb_fs");
    let cost_vertex = if has_vertex { cost.entry("mb_vs") } else { 0 };
    let heaviest = cost.heaviest.and_then(|(span, trips, c)| {
        let line = span.location(&capped_full).line_number as usize;
        (line > start_line).then(|| Heaviest { line: (line - start_line) as u32, trips, cost: c })
    });
    if let Some(max) = limits.max_cost {
        let worst = cost_fragment.max(cost_vertex);
        if worst > max {
            let which = if cost_vertex > cost_fragment { "vertex" } else { "surface" };
            let mut msg = format!("estimated cost {worst} per pixel ({which}); the limit is {max}");
            if let Some(h) = &heaviest {
                if h.cost * 2 >= worst {
                    if h.trips >= limits.loop_cap as u64 {
                        msg.push_str(&format!(". The loop on line {} costs {}: it may run {} times (the cap); a `for (var i = 0; i < N; i++)` with a small N is costed at N", h.line, h.cost, h.trips));
                    } else {
                        msg.push_str(&format!(". The loop on line {} costs {}: {} trips; fewer, or less work in each", h.line, h.cost, h.trips));
                    }
                }
            }
            if module_samples(&module) * W_SAMPLE * 2 >= worst {
                msg.push_str(". A texture sample costs 256");
            }
            return Err(refuse_at("cost", msg, heaviest.as_ref().filter(|h| h.cost * 2 >= worst).map(|h| h.line)));
        }
    }
    let tail = capped_full.len() - (full.len() - head_len - src.len());
    let source = capped_full[head_len..tail].to_string();
    let expressions = module.functions.iter().map(|(_, f)| f.expressions.len()).sum::<usize>() + module.entry_points.iter().map(|e| e.function.expressions.len()).sum::<usize>();
    Ok(Checked { source, kind, cost_fragment, cost_vertex, loops_capped: n, loops_counted: cost.counted, expressions, has_vertex, uniforms: fields, heaviest })
}

fn missing(kind: Kind) -> Refusal {
    refuse(
        "missing",
        match kind {
            Kind::Surface => "no `fn surface(in: Surface) -> SurfaceOut`: a material's shader defines it",
            Kind::Sprite => "no `fn sprite(in: Sprite) -> vec4<f32>`: a sprite's shader defines it",
        },
    )
}

fn module_samples(module: &Module) -> u64 {
    let f = |func: &Function| func.expressions.iter().filter(|(_, e)| matches!(e, Expression::ImageSample { .. } | Expression::ImageLoad { .. })).count() as u64;
    module.functions.iter().map(|(_, x)| f(x)).sum::<u64>() + module.entry_points.iter().map(|e| f(&e.function)).sum::<u64>()
}

fn validate(module: &Module) -> Result<ModuleInfo, (String, Option<Span>)> {
    // WebGPU's baseline: no f64, f16, subgroups, push constants... (Capabilities::empty()).
    Validator::new(ValidationFlags::all(), Capabilities::empty()).validate(module).map_err(|e| {
        let mut s = format!("{}", e.as_inner());
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e.as_inner());
        while let Some(x) = src {
            s.push_str(&format!(": {x}"));
            src = x.source();
        }
        let span = e.spans().map(|(s, _)| *s).find(|s| s.is_defined());
        (s, span)
    })
}

/// The fields of the game's `struct Uniforms`: floats and vectors of floats, in 256 bytes.
fn uniform_fields(module: &Module) -> Result<Vec<Field>, Refusal> {
    let Some((h, t)) = module.types.iter().find(|(_, t)| t.name.as_deref() == Some("Uniforms")) else {
        return Err(refuse("uniforms", "`struct Uniforms` isn't a struct"));
    };
    let TypeInner::Struct { members, span } = &t.inner else { return Err(refuse("uniforms", "`Uniforms` must be a struct")) };
    if *span > UNIFORM_BYTES {
        return Err(refuse("uniforms", format!("`struct Uniforms` is {span} bytes; at most {UNIFORM_BYTES} (16 vec4s)")));
    }
    let _ = h;
    let mut out = vec![];
    for m in members {
        let comps = match module.types[m.ty].inner {
            TypeInner::Scalar(s) if s == naga::Scalar::F32 => 1,
            TypeInner::Vector { size, scalar } if scalar == naga::Scalar::F32 => size as u32,
            _ => return Err(refuse("uniforms", format!("`{}`: a uniform is an f32 or a vec2, vec3 or vec4 of f32", m.name.as_deref().unwrap_or("?")))),
        };
        out.push(Field { name: m.name.clone().unwrap_or_default(), offset: m.offset, components: comps });
    }
    Ok(out)
}

fn check_module(module: &Module, limits: &Limits) -> Result<(), Refusal> {
    for (_, g) in module.global_variables.iter() {
        let name = g.name.as_deref().unwrap_or("");
        let ours = CHECK_BINDINGS.iter().any(|(n, grp, b)| *n == name && g.binding.as_ref().is_some_and(|rb| rb.group == *grp && rb.binding == *b));
        if ours {
            continue;
        }
        match g.space {
            naga::AddressSpace::Private => {}
            other => {
                let what = match other {
                    naga::AddressSpace::Storage { .. } => "storage buffers".to_string(),
                    naga::AddressSpace::WorkGroup => "workgroup memory".to_string(),
                    naga::AddressSpace::Uniform => "uniform buffers of its own (use mb_u or struct Uniforms)".to_string(),
                    naga::AddressSpace::Handle => "textures or samplers of its own (mb_tex0–3, mb_samp)".to_string(),
                    o => format!("{o:?}"),
                };
                return Err(refuse("binding", format!("`{name}`: a shader reads only the host's bindings (no {what})")));
            }
        }
    }
    let names: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
    if names.len() != 2 || !names.contains(&"mb_vs") || !names.contains(&"mb_fs") {
        return Err(refuse("entry", "a shader has no entry points of its own (no @vertex, @fragment or @compute)"));
    }
    for (_, t) in module.types.iter() {
        match t.inner {
            TypeInner::Atomic(_) => return Err(refuse("atomic", "no atomics")),
            TypeInner::BindingArray { .. } | TypeInner::AccelerationStructure { .. } | TypeInner::RayQuery { .. } => {
                return Err(refuse("binding", "not a type a shader can use"));
            }
            _ => {}
        }
    }
    if !module.overrides.is_empty() {
        return Err(refuse("override", "no `override` declarations (use a const)"));
    }
    let exprs: usize = module.functions.iter().map(|(_, f)| f.expressions.len()).sum::<usize>()
        + module.entry_points.iter().map(|e| e.function.expressions.len()).sum::<usize>()
        + module.global_expressions.len();
    if exprs > limits.max_expressions {
        return Err(refuse("expressions", format!("{exprs} expressions; at most {}", limits.max_expressions)));
    }
    let mut layouter = naga::proc::Layouter::default();
    layouter.update(module.to_ctx()).map_err(|e| refuse("invalid", format!("{e}")))?;
    let fns = module.functions.iter().map(|(_, f)| f).chain(module.entry_points.iter().map(|e| &e.function));
    for f in fns {
        let bytes: u32 = f.local_variables.iter().map(|(_, l)| layouter[l.ty].size).sum();
        if bytes > limits.max_local_bytes {
            return Err(refuse("locals", format!("{}: {bytes} bytes of local variables; at most {}", f.name.as_deref().unwrap_or("?"), limits.max_local_bytes)));
        }
    }
    // Private globals are per invocation too.
    let private: u32 = module.global_variables.iter().filter(|(_, g)| g.space == naga::AddressSpace::Private).map(|(_, g)| layouter[g.ty].size).sum();
    if private > limits.max_local_bytes {
        return Err(refuse("locals", format!("{private} bytes of var<private>; at most {}", limits.max_local_bytes)));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The loop cap.

/// The cap as text: before each loop statement `var mb_nK: u32 = 0u;`, and
/// first in its body `if (mb_nK >= cap) { break; } mb_nK += 1u;`. Loops are
/// found in naga's IR (each `Loop` statement's span: `loop`, `for`,
/// `while`), so nothing is guessed from the text but where the body's `{`
/// is; the result is parsed again and every loop must start with its cap
/// (`count_cap_heads`), or the shader is refused. Every loop must be in
/// `region` (the game's source).
fn cap_source(full: &str, module: &Module, cap: u32, region: std::ops::Range<usize>) -> Result<(String, usize), Refusal> {
    fn walk(b: &Block, starts: &mut Vec<usize>) -> Result<(), Refusal> {
        for (s, span) in b.span_iter() {
            match s {
                Statement::Block(b) => walk(b, starts)?,
                Statement::If { accept, reject, .. } => {
                    walk(accept, starts)?;
                    walk(reject, starts)?;
                }
                Statement::Switch { cases, .. } => {
                    for c in cases {
                        walk(&c.body, starts)?;
                    }
                }
                Statement::Loop { body, continuing, .. } => {
                    let r = span.to_range().ok_or_else(|| refuse("rewrite", "a loop without a source position"))?;
                    starts.push(r.start);
                    walk(body, starts)?;
                    walk(continuing, starts)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut starts = vec![];
    for (_, f) in module.functions.iter() {
        walk(&f.body, &mut starts)?;
    }
    for ep in module.entry_points.iter() {
        walk(&ep.function.body, &mut starts)?;
    }
    starts.sort_unstable();
    starts.dedup();
    if starts.iter().any(|s| !region.contains(s)) {
        return Err(refuse("rewrite", "a loop outside the shader's own source"));
    }
    let bytes = full.as_bytes();
    // (offset, text) insertions, applied from the end.
    let mut edits: Vec<(usize, String)> = vec![];
    for (k, &start) in starts.iter().enumerate() {
        let mut depth = 0i32;
        let mut brace = None;
        for (i, &c) in bytes.iter().enumerate().skip(start) {
            match c {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'{' if depth == 0 => {
                    brace = Some(i);
                    break;
                }
                b';' if depth == 0 => break,
                _ => {}
            }
        }
        let brace = brace.ok_or_else(|| refuse("rewrite", "can't find a loop's body"))?;
        let n = k + 1;
        edits.push((start, format!("var mb_n{n}: u32 = 0u; ")));
        edits.push((brace + 1, format!(" if (mb_n{n} >= {cap}u) {{ break; }} mb_n{n} += 1u;")));
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = full.to_string();
    for (at, t) in edits {
        out.insert_str(at, &t);
    }
    Ok((out, starts.len()))
}

/// (loops, loops whose body starts with the cap) in a module.
fn count_cap_heads(module: &Module) -> (usize, usize) {
    fn walk(f: &Function, b: &Block, n: &mut (usize, usize)) {
        for s in b.iter() {
            match s {
                Statement::Block(b) => walk(f, b, n),
                Statement::If { accept, reject, .. } => {
                    walk(f, accept, n);
                    walk(f, reject, n);
                }
                Statement::Switch { cases, .. } => cases.iter().for_each(|c| walk(f, &c.body, n)),
                Statement::Loop { body, continuing, .. } => {
                    n.0 += 1;
                    // `loop {` starts with the cap. `for` and `while` start with
                    // their condition (`if c {} else { break; }`), then the
                    // written body as a block, which starts with the cap.
                    let mut stmts: Vec<&Statement> = body.iter().filter(|s| !matches!(s, Statement::Emit(_))).collect();
                    if let Some(Statement::If { accept, reject, .. }) = stmts.first() {
                        let is_cond = accept.is_empty() && reject.len() == 1 && matches!(reject[0], Statement::Break);
                        if is_cond {
                            if let Some(Statement::Block(inner)) = stmts.get(1) {
                                stmts = inner.iter().filter(|s| !matches!(s, Statement::Emit(_))).collect();
                            }
                        }
                    }
                    if let Some(Statement::If { condition, accept, reject }) = stmts.first() {
                        if is_cap_head(f, *condition) && accept.len() == 1 && matches!(accept[0], Statement::Break) && reject.is_empty() {
                            n.1 += 1;
                        }
                    }
                    walk(f, body, n);
                    walk(f, continuing, n);
                }
                _ => {}
            }
        }
    }
    let mut n = (0, 0);
    for (_, f) in module.functions.iter() {
        walk(f, &f.body, &mut n);
    }
    for ep in module.entry_points.iter() {
        walk(&ep.function, &ep.function.body, &mut n);
    }
    n
}

// ---------------------------------------------------------------------------
// The static cost.

/// Operations per invocation, roughly what a GPU issues: scalar ops count 1
/// a component, transcendental maths 4, a texture sample 256, a matrix
/// multiply its multiply-adds. Loops multiply their body by the trip count
/// (the cap, or a provable smaller constant), calls are inlined.
struct Cost<'a> {
    module: &'a Module,
    info: &'a ModuleInfo,
    cap: u64,
    memo: HashMap<Handle<Function>, u64>,
    /// Loops whose trip count was proven below the cap.
    counted: usize,
    /// The most expensive loop seen: (its span, trips, total cost).
    heaviest: Option<(Span, u64, u64)>,
}

impl<'a> Cost<'a> {
    fn new(module: &'a Module, info: &'a ModuleInfo, cap: u32) -> Self {
        Cost { module, info, cap: cap as u64, memo: HashMap::new(), counted: 0, heaviest: None }
    }

    fn entry(&mut self, name: &str) -> u64 {
        let Some((i, ep)) = self.module.entry_points.iter().enumerate().find(|(_, e)| e.name == name) else { return 0 };
        let fi = self.info.get_entry_point(i);
        self.block(&ep.function, fi, &ep.function.body)
    }

    fn function(&mut self, h: Handle<Function>) -> u64 {
        if let Some(c) = self.memo.get(&h) {
            return *c;
        }
        let f = &self.module.functions[h];
        let fi = &self.info[h];
        let c = self.block(f, fi, &f.body) + 2;
        self.memo.insert(h, c);
        c
    }

    fn block(&mut self, f: &Function, fi: &naga::valid::FunctionInfo, b: &Block) -> u64 {
        let mut total: u64 = 0;
        for (k, (s, span)) in b.span_iter().enumerate() {
            let c = match s {
                Statement::Emit(r) => r.clone().map(|e| self.expr(f, fi, e)).fold(0u64, |a, x| a.saturating_add(x)),
                Statement::Block(b) => self.block(f, fi, b),
                Statement::If { accept, reject, .. } => 1 + self.block(f, fi, accept).max(self.block(f, fi, reject)),
                Statement::Switch { cases, .. } => 1 + cases.iter().map(|c| self.block(f, fi, &c.body)).fold(0u64, |a, x| a.saturating_add(x)),
                Statement::Loop { body, continuing, break_if } => {
                    let trips = self.trips(f, &b[..k], body, continuing);
                    let per = self.block(f, fi, body).saturating_add(self.block(f, fi, continuing)).saturating_add(if break_if.is_some() { 1 } else { 0 }).saturating_add(1);
                    let c = trips.saturating_mul(per);
                    if self.heaviest.is_none_or(|(_, _, h)| c > h) {
                        self.heaviest = Some((*span, trips, c));
                    }
                    c
                }
                Statement::Call { function, .. } => self.function(*function).saturating_add(2),
                Statement::Store { pointer, .. } => {
                    // A store of a whole array or struct copies every component.
                    fi[*pointer].ty.inner_with(&self.module.types).pointer_base_type().map_or(1, |t| match t {
                        naga::proc::TypeResolution::Handle(h) => comps(&self.module.types[h].inner, self.module),
                        naga::proc::TypeResolution::Value(v) => comps(&v, self.module),
                    })
                }
                _ => 1,
            };
            total = total.saturating_add(c);
        }
        total
    }

    /// A loop's trip count for costing: the cap, or less when the loop is
    /// `for (var i = a; i < N; i += k)` with literals and `i` written
    /// nowhere else.
    fn trips(&mut self, f: &Function, prefix: &[Statement], body: &Block, continuing: &Block) -> u64 {
        match counted_loop(f, prefix, body, continuing) {
            Some(n) if n < self.cap => {
                self.counted += 1;
                n
            }
            _ => self.cap,
        }
    }

    fn expr(&mut self, f: &Function, fi: &naga::valid::FunctionInfo, e: Handle<Expression>) -> u64 {
        let ty = fi[e].ty.inner_with(&self.module.types);
        let n = comps(ty, self.module);
        match &f.expressions[e] {
            Expression::Load { .. } | Expression::Compose { .. } | Expression::Splat { .. } => n.max(1) / 2 + 1,
            Expression::Access { .. } | Expression::AccessIndex { .. } | Expression::Swizzle { .. } => 0,
            Expression::ImageSample { .. } => W_SAMPLE,
            Expression::ImageLoad { .. } => W_LOAD_IMAGE,
            Expression::ImageQuery { .. } => 2,
            Expression::Unary { .. } | Expression::Select { .. } | Expression::Relational { .. } | Expression::As { .. } => n,
            Expression::Binary { op, left, right } => {
                let lt = fi[*left].ty.inner_with(&self.module.types);
                let rt = fi[*right].ty.inner_with(&self.module.types);
                match (op, lt, rt) {
                    (BinaryOperator::Multiply, TypeInner::Matrix { columns, rows, .. }, TypeInner::Vector { .. })
                    | (BinaryOperator::Multiply, TypeInner::Vector { .. }, TypeInner::Matrix { columns, rows, .. }) => *columns as u64 * *rows as u64,
                    (BinaryOperator::Multiply, TypeInner::Matrix { columns: c1, rows: r1, .. }, TypeInner::Matrix { columns: c2, .. }) => *c1 as u64 * *r1 as u64 * *c2 as u64,
                    (BinaryOperator::Divide | BinaryOperator::Modulo, _, _) => n * 2,
                    _ => n,
                }
            }
            Expression::Derivative { .. } => n * 2,
            Expression::Math { fun, arg, .. } => {
                let an = comps(fi[*arg].ty.inner_with(&self.module.types), self.module);
                use MathFunction as M;
                match fun {
                    M::Sin | M::Cos | M::Tan | M::Sinh | M::Cosh | M::Tanh | M::Asin | M::Acos | M::Atan | M::Atan2 | M::Asinh | M::Acosh | M::Atanh | M::Exp | M::Exp2 | M::Log | M::Log2 | M::Pow | M::Sqrt
                    | M::InverseSqrt => W_TRANSCENDENTAL * an,
                    M::Dot | M::Distance | M::Length => an + 2,
                    M::Normalize => an * 2 + 4,
                    M::Cross => 6,
                    M::Reflect | M::Refract | M::FaceForward => an * 4,
                    M::Inverse | M::Determinant => an * an,
                    M::Transpose | M::Outer => an,
                    M::SmoothStep => an * 4,
                    _ => an,
                }
            }
            _ => 0,
        }
    }
}

fn comps(t: &TypeInner, module: &Module) -> u64 {
    match t {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => *size as u64,
        TypeInner::Matrix { columns, rows, .. } => *columns as u64 * *rows as u64,
        TypeInner::Array { base, size: naga::ArraySize::Constant(n), .. } => n.get() as u64 * comps(&module.types[*base].inner, module),
        TypeInner::Struct { members, .. } => members.iter().map(|m| comps(&module.types[m.ty].inner, module)).sum(),
        TypeInner::Pointer { base, .. } => comps(&module.types[*base].inner, module),
        _ => 1,
    }
}

/// The trip count of `for (var i = a; i < N; i += k)` as naga lowers it, when
/// `a`, `N`, `k` are literals and `i` is stored nowhere but the increment.
fn counted_loop(f: &Function, prefix: &[Statement], body: &Block, continuing: &Block) -> Option<u64> {
    // body: [our cap head...] Emit, If { cond } { } else { break } ...
    let mut cond = None;
    for s in body.iter() {
        match s {
            Statement::Emit(_) | Statement::Store { .. } => continue,
            Statement::If { condition, accept, reject } => {
                let only_break = |b: &Block| b.len() == 1 && matches!(b[0], Statement::Break);
                if accept.is_empty() && only_break(reject) {
                    cond = Some(*condition);
                    break;
                }
                if only_break(accept) && reject.is_empty() && is_cap_head(f, *condition) {
                    continue;
                }
                return None;
            }
            _ => return None,
        }
    }
    let cond = cond?;
    let Expression::Binary { op, left, right } = f.expressions[cond] else { return None };
    let Expression::Load { pointer } = f.expressions[left] else { return None };
    let Expression::LocalVariable(var) = f.expressions[pointer] else { return None };
    let limit = literal_int(f, right)?;
    // Where `i` starts: a literal stored just before the loop (a `for` inside
    // another loop), or the variable's own literal init when nothing but this
    // loop's increment ever writes it (then it only grows, so this bounds every entry).
    let mut init = None;
    for s in prefix.iter().rev() {
        if let Statement::Store { pointer: p, value } = s {
            if points_to(f, *p, var) {
                init = Some(literal_int(f, *value)?);
                break;
            }
        }
        if writes_var(f, std::slice::from_ref(s), var) {
            return None;
        }
    }
    let init = match init {
        Some(v) => v,
        None => {
            if writes_var(f, &f.body, var) && count_writes(f, &f.body, var) != 1 {
                return None;
            }
            literal_int(f, f.local_variables[var].init?)?
        }
    };
    // continuing: exactly one write to `var` (a store, or its pointer passed
    // to a call), a store of `var + k`. A second write (`i = i + 2147483647`
    // after it, a call handed `&i`) could run the loop to its cap.
    if count_writes(f, continuing, var) != 1 {
        return None;
    }
    let mut step = None;
    for s in continuing.iter() {
        if let Statement::Store { pointer: p, value } = s {
            if points_to(f, *p, var) {
                let Expression::Binary { op: BinaryOperator::Add, left: l, right: r } = f.expressions[*value] else { return None };
                let Expression::Load { pointer: lp } = f.expressions[l] else { return None };
                if !points_to(f, lp, var) {
                    return None;
                }
                step = Some(literal_int(f, r)?);
            }
        }
    }
    let step = step?;
    if step <= 0 || writes_var(f, body, var) {
        return None;
    }
    // Small enough that nothing wraps: i32 (or u32) arithmetic past 2^31 would.
    const SMALL: i64 = 1 << 24;
    if [init, limit, step].iter().any(|v| v.abs() > SMALL) {
        return None;
    }
    let span = match op {
        BinaryOperator::Less => limit - init,
        BinaryOperator::LessEqual => limit - init + 1,
        _ => return None,
    };
    if span <= 0 {
        return Some(0);
    }
    Some(((span + step - 1) / step) as u64)
}

fn is_cap_head(f: &Function, cond: Handle<Expression>) -> bool {
    let Expression::Binary { op: BinaryOperator::GreaterEqual, left, .. } = f.expressions[cond] else { return false };
    let Expression::Load { pointer } = f.expressions[left] else { return false };
    let Expression::LocalVariable(v) = f.expressions[pointer] else { return false };
    f.local_variables[v].name.as_deref().is_some_and(|n| n.starts_with("mb_n"))
}

fn points_to(f: &Function, p: Handle<Expression>, var: Handle<LocalVariable>) -> bool {
    matches!(f.expressions[p], Expression::LocalVariable(v) if v == var)
}

/// Stores to `var` (or calls given its pointer) anywhere in `b`.
fn count_writes(f: &Function, b: &[Statement], var: Handle<LocalVariable>) -> usize {
    b.iter()
        .map(|s| match s {
            Statement::Store { pointer, .. } => points_to(f, *pointer, var) as usize,
            Statement::Call { arguments, .. } => arguments.iter().filter(|a| points_to(f, **a, var)).count(),
            Statement::Block(b) => count_writes(f, b, var),
            Statement::If { accept, reject, .. } => count_writes(f, accept, var) + count_writes(f, reject, var),
            Statement::Switch { cases, .. } => cases.iter().map(|c| count_writes(f, &c.body, var)).sum(),
            Statement::Loop { body, continuing, .. } => count_writes(f, body, var) + count_writes(f, continuing, var),
            _ => 0,
        })
        .sum()
}

/// `var` is stored, or its pointer passed to a call, anywhere in `b`.
fn writes_var(f: &Function, b: &[Statement], var: Handle<LocalVariable>) -> bool {
    b.iter().any(|s| match s {
        Statement::Store { pointer, .. } => points_to(f, *pointer, var),
        Statement::Call { arguments, .. } => arguments.iter().any(|a| points_to(f, *a, var)),
        Statement::Block(b) => writes_var(f, b, var),
        Statement::If { accept, reject, .. } => writes_var(f, accept, var) || writes_var(f, reject, var),
        Statement::Switch { cases, .. } => cases.iter().any(|c| writes_var(f, &c.body, var)),
        Statement::Loop { body, continuing, .. } => writes_var(f, body, var) || writes_var(f, continuing, var),
        _ => false,
    })
}

fn literal_int(f: &Function, e: Handle<Expression>) -> Option<i64> {
    match f.expressions[e] {
        Expression::Literal(Literal::I32(v)) => Some(v as i64),
        Expression::Literal(Literal::U32(v)) => Some(v as i64),
        Expression::Literal(Literal::AbstractInt(v)) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
