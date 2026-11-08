fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    loop {
        x = fract(x * 1.0001 + 0.1);
        if (mb_u[0].x > 1e30) { break; }
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
