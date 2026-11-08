fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    var y = in.uv.y;
    while (mb_u[1].x < 1.0) {
        x = fract(sin(x * 12.9898 + y) * 43758.5453);
        y = fract(y + x);
    }
    let c = vec3<f32>(x, y, 0.0);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
