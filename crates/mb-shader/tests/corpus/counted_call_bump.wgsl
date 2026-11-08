// A counted-looking loop whose counter a call also writes: `back(&i)` undoes
// each step, so it runs to the cap (256), not 4. (Phase 5 review.)
fn back(p: ptr<function, i32>) {
    *p = *p - 1;
}

fn surface(in: Surface) -> SurfaceOut {
    var x = in.uv.x;
    var i = 0;
    loop {
        if (i < 4) { } else { break; }
        x = sin(x * 1.1 + 0.3) + cos(x * 1.7) + sin(x * 2.3) + cos(x * 0.9);
        continuing {
            i = i + 1;
            back(&i);
        }
    }
    var o: SurfaceOut;
    o.albedo = vec3<f32>(x);
    o.alpha = 1.0;
    o.normal = in.normal;
    return o;
}
