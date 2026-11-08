fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    var y = in.uv.y;
    for (var i = 0; i < 48; i++) {
        x = sin(x * 1.1 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.2 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.3 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.4 + y); y = fract(y + x * 0.7);
    }
    let c = vec3<f32>(fract(x), fract(y), 0.5);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
