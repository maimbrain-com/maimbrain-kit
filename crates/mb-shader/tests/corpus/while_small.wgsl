fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    while (x >= 0.0) { x = x + 1.0; }
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
