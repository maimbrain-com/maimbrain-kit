fn surface(in: Surface) -> SurfaceOut {

    var a: array<vec4<f32>, 4096>;
    for (var i = 0; i < 256; i++) { a[(i * 37) & 4095] = vec4<f32>(f32(i) * in.uv.x); }
    let c = a[u32(in.uv.x * 4095.0)].xyz;


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
