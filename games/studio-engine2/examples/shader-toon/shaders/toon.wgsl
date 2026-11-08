// Cartoon shading: the sun's light in a few flat bands with soft steps, a
// warm shadow colour instead of grey, a bright rim on the lit side, and an
// ink line where the surface turns away (the silhouette's inside edge).
// The material is unlit (the bands are the lighting) and keeps its own
// outline (`outline`, `ink`) for the silhouette's outside edge.
struct Uniforms {
    shadow: vec3<f32>,
    bands: f32,
    ink: vec3<f32>,
    rim: f32,
    // A hit flashes white for a moment (0–1).
    flash: f32,
}

fn surface(in: Surface) -> SurfaceOut {
    let n = normalize(in.normal);
    let l = dot(n, normalize(in.light)) * 0.5 + 0.5;
    let x = l * uniforms.bands;
    let band = (floor(x) + smoothstep(0.4, 0.6, fract(x))) / uniforms.bands;
    let facing = max(dot(n, in.view), 0.0);
    let rim = smoothstep(0.62, 0.7, 1.0 - facing) * smoothstep(0.45, 0.7, l) * uniforms.rim;
    let line = 1.0 - smoothstep(0.1, 0.2, facing);
    let base = in.color.rgb;
    var c = mix(base * uniforms.shadow, base, band) + vec3<f32>(rim);
    c = mix(c, uniforms.ink, line * 0.85);
    c = mix(c, vec3<f32>(1.0), uniforms.flash);

    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 1.0;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;
}
