fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    var y = in.uv.y;
    for (var i = 0; i < 256; i++) {
        for (var j = 0; j < 256; j++) {
            x = sin(x * 1.7 + y) * cos(y * 1.3 - x);
            y = pow(abs(y) + 0.5, 1.01) + exp2(-abs(x));
        }
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
