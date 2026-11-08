fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    var y = in.uv.y;
    for (var i = 0; i < 256; i++) {
        x = sin(x * 1.1 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.2 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.3 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.4 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.5 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.6 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.7 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.8 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.9 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.10 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.11 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.12 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.13 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.14 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.15 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.16 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.17 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.18 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.19 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.20 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.21 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.22 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.23 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.24 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.25 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.26 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.27 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.28 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.29 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.30 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.31 + y); y = fract(y + x * 0.7);
        x = sin(x * 1.32 + y); y = fract(y + x * 0.7);
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
