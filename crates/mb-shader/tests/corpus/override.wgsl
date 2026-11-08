override N: u32 = 1000000000u;

fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    for (var i = 0u; i < N; i++) { x = fract(x * 1.3 + 0.1); }
    let c = vec3<f32>(x);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
