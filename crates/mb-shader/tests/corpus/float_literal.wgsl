fn surface(in: Surface) -> SurfaceOut {

    var c = vec3<f32>(in.uv, 0.5);
    if (mb_u[0].x > 1000000000000000000000000000000.0) { c = vec3<f32>(1.0); }


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
