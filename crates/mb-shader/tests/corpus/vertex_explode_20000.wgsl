fn vertex(in: Vertex) -> Vertex {
    var v = in;
    // Every triangle covers the screen: corner 0 at (-3,-1), 1 at (1,-1), 2 at (1,3).
    let k = u32(floor(in.uv.x * 1000.0)) % 3u;
    v.position = select(select(vec3<f32>(1.0, 3.0, 0.5), vec3<f32>(1.0, -1.0, 0.5), k == 1u), vec3<f32>(-3.0, -1.0, 0.5), k == 0u);
    return v;
}

fn surface(in: Surface) -> SurfaceOut {
    let c = vec3<f32>(in.uv, 0.2);

    var o: SurfaceOut;
    o.albedo = c;
    o.alpha = 1.0;
    o.emission = vec3<f32>(0.0);
    o.roughness = 0.5;
    o.metalness = 0.0;
    o.normal = in.normal;
    return o;

}
