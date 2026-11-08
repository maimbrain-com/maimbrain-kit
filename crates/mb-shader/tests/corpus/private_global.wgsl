var<private> seed: f32;
fn rnd() -> f32 { seed = fract(sin(seed * 91.7) * 43758.5); return seed; }

fn surface(in: Surface) -> SurfaceOut {
    seed = in.uv.x + in.uv.y * 7.0;
    let c = vec3<f32>(rnd(), rnd(), rnd());

    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
