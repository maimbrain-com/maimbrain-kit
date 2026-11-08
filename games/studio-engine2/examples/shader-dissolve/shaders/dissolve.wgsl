// A crate burning away: wherever the noise is under `t` it's gone, and a
// hot rim glows where the burn is eating in. t = 0 is whole, 1 is gone.
struct Uniforms {
    t: f32,
    edge: vec3<f32>,
    width: f32,
}

fn surface(in: Surface) -> SurfaceOut {
    // The burn's pattern, and the wood's grain (the same picture, stretched).
    let n = textureSample(mb_tex0, mb_samp, in.uv * 0.7 + in.local.xz * 0.2).r;
    let grain = textureSample(mb_tex0, mb_samp, vec2<f32>(in.uv.x * 0.25, in.uv.y * 3.0)).r;
    let cut = uniforms.t * 1.1;
    if (n < cut) {
        discard;
    }
    // 1 right at the burning edge, 0 a rim's width in.
    let rim = 1.0 - smoothstep(cut, cut + uniforms.width, n);
    let char = smoothstep(0.0, 0.5, rim) * step(0.001, uniforms.t);
    let wood = in.color.rgb * (0.78 + 0.4 * grain);

    var o: SurfaceOut;
    o.albedo = mix(wood, vec3<f32>(0.05, 0.03, 0.02), char);
    o.alpha = 1.0;
    o.emission = uniforms.edge * pow(rim, 3.0) * 7.0 * step(0.001, uniforms.t);
    o.roughness = 0.75;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;
}
