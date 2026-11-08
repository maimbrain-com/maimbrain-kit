fn surface(in: Surface) -> SurfaceOut {

    var v = 0.0;
    var a = 0.5;
    var p = in.uv * 4.0;
    for (var i = 0; i < 8; i++) {
        v += a * textureSampleLevel(mb_tex0, mb_samp, p, 0.0).r;
        p = p * 2.03 + vec2<f32>(0.31, 0.17);
        a *= 0.5;
    }
    let c = vec3<f32>(v);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
