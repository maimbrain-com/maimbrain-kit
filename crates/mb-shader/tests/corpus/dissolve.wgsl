fn surface(in: Surface) -> SurfaceOut {

    let n = textureSample(mb_tex0, mb_samp, in.uv * 2.0).r;
    if (n < mb_u[0].y) { discard; }
    let edge = smoothstep(mb_u[0].y, mb_u[0].y + 0.05, n);
    let c = mix(vec3<f32>(1.0, 0.4, 0.1) * 4.0, textureSample(mb_tex1, mb_samp, in.uv).rgb, edge);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
