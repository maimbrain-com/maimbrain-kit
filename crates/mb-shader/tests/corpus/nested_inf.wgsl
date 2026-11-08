fn surface(in: Surface) -> SurfaceOut {

    var x = in.uv.x;
    var i = 0.0;
    while (i < mb_u[3].x) {
        var j = 0.0;
        while (j < mb_u[3].x) {
            var k = 0.0;
            while (k < mb_u[3].x) {
                x = fract(x * 1.1 + 0.3);
                k += 1.0;
            }
            j += 1.0;
        }
        i += 1.0;
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
