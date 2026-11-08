// A sea with real swells: the vertex hook lifts the plane into three waves
// (and tilts its normals with them), the surface adds two drifting ripple
// layers, deep and shallow colour by how steeply you look, and foam on the
// crests. The host lights it: a low roughness gives the sky's reflection.
struct Uniforms {
    deep: vec3<f32>,
    shallow: vec3<f32>,
    foam: vec3<f32>,
    swell: f32,
    // The game's clock, so its boats ride the same swell.
    time: f32,
}

// The swell's height at p (meters), at time t. The game bobs its boats on the same sum.
fn swell(p: vec2<f32>, t: f32) -> f32 {
    return sin(p.x * 0.31 + t * 1.05) * 0.26
        + sin(p.y * 0.47 - t * 1.32 + p.x * 0.17) * 0.17
        + sin((p.x - p.y) * 0.83 + t * 1.9) * 0.07;
}

fn vertex(in: Vertex) -> Vertex {
    var v = in;
    let p = in.position.xz;
    let h = swell(p, uniforms.time) * uniforms.swell;
    let e = 0.35;
    let hx = swell(p + vec2<f32>(e, 0.0), uniforms.time) * uniforms.swell - h;
    let hz = swell(p + vec2<f32>(0.0, e), uniforms.time) * uniforms.swell - h;
    v.position.y = in.position.y + h;
    v.normal = normalize(vec3<f32>(-hx / e, 1.0, -hz / e));
    return v;
}

fn surface(in: Surface) -> SurfaceOut {
    let t = uniforms.time;
    let p = in.local.xz;
    let a = textureSample(mb_tex0, mb_samp, p * 0.045 + vec2<f32>(t * 0.011, t * 0.007)).r;
    let b = textureSample(mb_tex0, mb_samp, p * 0.083 - vec2<f32>(t * 0.009, -t * 0.013)).r;
    let ripple = vec3<f32>(a - 0.5, 0.0, b - 0.5) * 0.3;
    let n = normalize(in.normal + ripple);
    let facing = clamp(dot(n, in.view), 0.0, 1.0);
    // Looking down you see into it (shallow); at a slant, its depth (deep).
    let body = mix(uniforms.deep, uniforms.shallow, pow(facing, 1.6) * 0.8);
    // Foam where a crest is high and the ripples agree.
    let crest = smoothstep(0.3, 0.44, in.local.y) * smoothstep(0.62, 0.86, a * 0.6 + b * 0.6);

    var o: SurfaceOut;
    o.albedo = mix(body, uniforms.foam, crest);
    o.alpha = 1.0;
    // Light scattered inside the water, so it reads as water, not a mirror.
    o.emission = body * 0.16;
    o.roughness = mix(0.16, 0.85, crest);
    o.metalness = 0.0;
    o.normal = n;
    return o;
}
