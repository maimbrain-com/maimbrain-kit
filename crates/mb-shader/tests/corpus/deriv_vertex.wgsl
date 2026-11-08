fn vertex(in: Vertex) -> Vertex {
    var v = in;
    v.position.x += dpdx(in.uv.x);
    return v;
}

fn surface(in: Surface) -> SurfaceOut {
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
