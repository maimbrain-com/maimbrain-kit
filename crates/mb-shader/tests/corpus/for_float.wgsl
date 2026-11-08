fn surface(in: Surface) -> SurfaceOut {

    var x = 0.0;
    for (var f = 0.0; f < 1.0; f = f + mb_u[2].x) {
        x = fract(x + in.uv.x * 0.37);
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
