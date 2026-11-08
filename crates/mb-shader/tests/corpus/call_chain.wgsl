fn f0(x: f32) -> f32 {
    var y = x;
    for (var i = 0; i < 256; i++) { y = sin(y * 1.3 + 0.1); }
    return y;
}
fn f1(x: f32) -> f32 {
    var y = x;
    for (var i = 0; i < 256; i++) { y = f0(y); }
    return y;
}
fn f2(x: f32) -> f32 {
    var y = x;
    for (var i = 0; i < 256; i++) { y = f1(y); }
    return y;
}

fn surface(in: Surface) -> SurfaceOut {
    let c = vec3<f32>(f2(in.uv.x));

    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
