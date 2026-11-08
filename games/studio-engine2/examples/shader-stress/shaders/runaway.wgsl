// S13's capped runaway (corpus loop_never_small): a loop whose break is
// never true, which the checks cap at 256 trips. It passes at ~3,960 of
// the 4,000 limit, so each full-screen layer of it is about 13 ms of GPU
// on an iPhone 16 Pro: the adversarial case Phase 5's phone run plays.
struct Uniforms { tint: vec3<f32>, alpha: f32 }

fn surface(in: Surface) -> SurfaceOut {
    var x = in.uv.x;
    loop {
        x = x + 1.0;
        if (x < -1.0) { break; }
    }
    var o: SurfaceOut;
    o.albedo = vec3<f32>(0.0);
    o.emission = uniforms.tint * (0.5 + 0.5 * fract(x * 0.001));
    o.alpha = uniforms.alpha;
    o.roughness = 1.0;
    o.normal = in.normal;
    return o;
}
