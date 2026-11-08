@group(2) @binding(0) var<storage, read_write> leak: array<f32>;

fn surface(in: Surface) -> SurfaceOut {
    leak[0] = in.uv.x;
    let c = vec3<f32>(in.uv, 0.0);

    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
