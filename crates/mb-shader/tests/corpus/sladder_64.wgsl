fn surface(in: Surface) -> SurfaceOut {

    var p = in.uv;
    var acc = vec4<f32>(0.0);
    for (var i = 0; i < 64; i++) {
        let a = textureSampleLevel(mb_tex0, mb_samp, p, 0.0);
        acc += a;
        p = fract(p + a.xy * 0.37 + vec2<f32>(0.123, 0.311));
    }
    let c = acc.rgb / 64.0;


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
