fn surface(in: Surface) -> SurfaceOut {

    let l = max(dot(normalize(in.normal), normalize(vec3<f32>(0.3, 0.8, 0.5))), 0.0);
    let band = floor(l * 3.0) / 3.0;
    let rim = pow(1.0 - max(dot(normalize(in.normal), in.view), 0.0), 3.0);
    let c = vec3<f32>(0.9, 0.5, 0.3) * (0.3 + band) + vec3<f32>(rim);


    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
