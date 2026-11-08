fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    for (var i = 1u; i != 0u; i = i + 2u) {
        x = fract(x * 1.37 + f32(i & 7u) * 0.01);
    }
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
