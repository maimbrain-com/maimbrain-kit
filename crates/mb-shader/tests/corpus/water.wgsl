fn surface(in: Surface) -> SurfaceOut {

    var h = 0.0;
    var p = in.uv * 8.0;
    for (var i = 0; i < 4; i++) {
        h += sin(p.x + in.time * 1.3) * cos(p.y * 1.1 + in.time) / f32(i + 1);
        p = p * 1.9 + vec2<f32>(1.7, 9.2);
    }
    let foam = textureSample(mb_tex2, mb_samp, in.uv * 4.0 + vec2<f32>(in.time * 0.05)).r;
    let c = mix(vec3<f32>(0.05, 0.25, 0.4), vec3<f32>(0.6, 0.9, 1.0), clamp(h * 0.5 + 0.5 + foam * 0.2, 0.0, 1.0));


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
