//! A tiny software rasterizer for `mb model`'s preview: the model from six
//! sides, then each clip as a strip of frames, on one contact sheet
//! (`art/preview/<name>.png`). Orthographic, z-buffered, the base color
//! texture (bilinear) lit by one sun plus ambient, emissive on top, a ground
//! grid at y = 0 and an orange arrow on the ground pointing the way the
//! model should face. Good enough to see shape, texture, scale, orientation
//! and motion without a browser; `mb shot` shows it in the real renderer.

use super::glb::{self, Scene};
use super::img::Image;
use super::shape::{self, Pose};

struct Tex {
    im: Image,
}

impl Tex {
    fn sample(&self, uv: [f32; 2]) -> [f32; 3] {
        let (w, h) = (self.im.w as f32, self.im.h as f32);
        let x = (uv[0] - uv[0].floor()) * w - 0.5;
        let y = (uv[1] - uv[1].floor()) * h - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let px = |xx: f32, yy: f32| {
            let xi = (xx as i64).rem_euclid(self.im.w as i64) as u32;
            let yi = (yy as i64).rem_euclid(self.im.h as i64) as u32;
            let p = self.im.get(xi, yi);
            [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
        };
        let (a, b, c, d) = (px(x0, y0), px(x0 + 1.0, y0), px(x0, y0 + 1.0), px(x0 + 1.0, y0 + 1.0));
        std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
    }
}

pub struct View {
    /// Degrees about +y; 0 looks at the model's front (from −z toward +z).
    pub yaw: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
}

struct Cam {
    c: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fwd: [f32; 3],
    scale: f32,
    w: f32,
    h: f32,
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Cam {
    fn new(c: [f32; 3], radius: f32, v: &View, w: u32, h: u32) -> Cam {
        let (yaw, pitch) = (v.yaw.to_radians(), v.pitch.clamp(-80.0, 80.0).to_radians());
        // Direction from the target toward the camera; yaw 0 is in front (−z).
        let d = [yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos()];
        let fwd = [-d[0], -d[1], -d[2]];
        let right = glb::normalize(cross(fwd, [0.0, 1.0, 0.0]));
        let up = cross(right, fwd);
        Cam { c, right, up, fwd, scale: w.min(h) as f32 / (2.0 * radius.max(1e-6)), w: w as f32, h: h as f32 }
    }

    fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let q = [p[0] - self.c[0], p[1] - self.c[1], p[2] - self.c[2]];
        [self.w / 2.0 + dot(q, self.right) * self.scale, self.h / 2.0 - dot(q, self.up) * self.scale, dot(q, self.fwd)]
    }
}

struct Target {
    w: u32,
    h: u32,
    color: Vec<[f32; 3]>,
    depth: Vec<f32>,
}

impl Target {
    /// Fills a triangle; `shade` gets barycentric weights and returns a color.
    fn tri(&mut self, p: [[f32; 3]; 3], shade: &dyn Fn([f32; 3]) -> [f32; 3]) {
        let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if area.abs() < 1e-9 {
            return;
        }
        let minx = p.iter().map(|v| v[0]).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
        let maxx = (p.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil() as i64).clamp(0, self.w as i64) as u32;
        let miny = p.iter().map(|v| v[1]).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
        let maxy = (p.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil() as i64).clamp(0, self.h as i64) as u32;
        for y in miny..maxy {
            for x in minx..maxx {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let e = |a: [f32; 3], b: [f32; 3]| (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0]);
                let w0 = e(p[1], p[2]) / area;
                let w1 = e(p[2], p[0]) / area;
                let w2 = e(p[0], p[1]) / area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2];
                let i = (y * self.w + x) as usize;
                if z < self.depth[i] {
                    self.depth[i] = z;
                    self.color[i] = shade([w0, w1, w2]);
                }
            }
        }
    }
}

/// Renders the model in a pose from one view, framed on (center, radius).
fn render(sc: &Scene, texs: &[Option<Tex>], pose: &Pose, frame: ([f32; 3], f32), ground: Option<f32>, v: &View, size: u32) -> Image {
    let ss = 2;
    let (w, h) = (size * ss, size * ss);
    let cam = Cam::new(frame.0, frame.1, v, w, h);
    let mut t = Target { w, h, color: vec![[0.0; 3]; (w * h) as usize], depth: vec![f32::MAX; (w * h) as usize] };
    // Background: a dark vertical gradient.
    for y in 0..h {
        let k = y as f32 / h as f32;
        for x in 0..w {
            t.color[(y * w + x) as usize] = [0.10 + 0.05 * k, 0.11 + 0.05 * k, 0.15 + 0.06 * k];
        }
    }
    let r = frame.1;
    if let Some(gy) = ground {
        // Grid lines every 0.25 of the frame radius, and the facing arrow toward −z.
        let step = (r / 4.0).max(1e-4);
        for k in -8..=8 {
            let o = k as f32 * step;
            let line = |a: [f32; 3], b: [f32; 3], wdt: f32, t: &mut Target, col: [f32; 3]| {
                let d = glb::normalize([b[0] - a[0], 0.0, b[2] - a[2]]);
                let n = [-d[2] * wdt, 0.0, d[0] * wdt];
                let q = [[a[0] - n[0], gy, a[2] - n[2]], [a[0] + n[0], gy, a[2] + n[2]], [b[0] + n[0], gy, b[2] + n[2]], [b[0] - n[0], gy, b[2] - n[2]]];
                let pq: Vec<[f32; 3]> = q.iter().map(|p| cam.project(*p)).collect();
                let push = |p: [f32; 3]| [p[0], p[1], p[2] + r * 0.002];
                t.tri([push(pq[0]), push(pq[1]), push(pq[2])], &|_| col);
                t.tri([push(pq[0]), push(pq[2]), push(pq[3])], &|_| col);
            };
            let c = frame.0;
            let col = [0.2, 0.23, 0.3];
            line([c[0] + o, gy, c[2] - 2.0 * r], [c[0] + o, gy, c[2] + 2.0 * r], r * 0.004, &mut t, col);
            line([c[0] - 2.0 * r, gy, c[2] + o], [c[0] + 2.0 * r, gy, c[2] + o], r * 0.004, &mut t, col);
        }
        let c = frame.0;
        let (z0, z1) = (c[2] - r * 0.95, c[2] - r * 1.35);
        let a = [[c[0] - r * 0.12, gy, z0], [c[0] + r * 0.12, gy, z0], [c[0], gy, z1]];
        let pa = a.map(|p| {
            let q = cam.project(p);
            [q[0], q[1], q[2] - r * 0.001]
        });
        t.tri(pa, &|_| [1.0, 0.55, 0.15]);
    }
    let light = glb::normalize([0.35, 0.8, -0.5]);
    for p in shape::posed(sc, pose) {
        let prim = &sc.meshes[p.mesh].prims[p.prim];
        let mat = prim.material.and_then(|m| sc.materials.get(m)).cloned().unwrap_or_default();
        let base_tex = mat.base_tex.as_ref().and_then(|tr| sc.texture_image(tr)).and_then(|i| texs[i].as_ref());
        let em_tex = mat.emissive_tex.as_ref().and_then(|tr| sc.texture_image(tr)).and_then(|i| texs[i].as_ref());
        let es = mat.emissive_strength.unwrap_or(1.0);
        let proj: Vec<[f32; 3]> = p.pos.iter().map(|v| cam.project(*v)).collect();
        for tri in prim.indices.chunks(3) {
            let ix = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let pts = ix.map(|i| proj[i]);
            let shade = |b: [f32; 3]| {
                let n = glb::normalize(std::array::from_fn(|k| (0..3).map(|j| b[j] * p.nrm[ix[j]][k]).sum()));
                let uv: Option<[f32; 2]> = prim.uv.as_ref().map(|u| std::array::from_fn(|k| (0..3).map(|j| b[j] * u[ix[j]][k]).sum()));
                let vc: [f32; 3] = prim.color.as_ref().map(|c| std::array::from_fn(|k| (0..3).map(|j| b[j] * c[ix[j]][k]).sum())).unwrap_or([1.0; 3]);
                let tc = match (base_tex, uv) {
                    (Some(t), Some(uv)) => t.sample(uv),
                    _ => [1.0; 3],
                };
                // Two-sided: light whichever side faces the camera.
                let nn = if dot(n, cam.fwd) > 0.0 { [-n[0], -n[1], -n[2]] } else { n };
                let lit = if mat.unlit { 1.0 } else { 0.38 + 0.8 * dot(nn, light).max(0.0) };
                let em = match (em_tex, uv) {
                    (Some(t), Some(uv)) => t.sample(uv),
                    _ => [1.0; 3],
                };
                std::array::from_fn(|k| {
                    let base = lin_to_srgb(mat.base[k]) * tc[k] * vc[k] * lit;
                    let glow = lin_to_srgb(mat.emissive[k]) * em[k] * (es.min(4.0) * 0.35);
                    (base + glow).min(1.0)
                })
            };
            t.tri(pts, &shade);
        }
    }
    let big = Image { w, h, px: t.color.iter().map(|c| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]).collect() };
    big.resize(size, size)
}

fn lin_to_srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

/// The contact sheet: row 1 the rest pose from the front, front-right, right,
/// back, left and above; then one row per clip (up to 4), six frames across
/// its length from a three-quarter view.
pub fn contact_sheet(sc: &Scene, cell: u32) -> Image {
    let texs: Vec<Option<Tex>> = sc.images.iter().map(|im| Image::decode(&im.bytes).ok().map(|im| Tex { im })).collect();
    let rest = shape::pose(sc, None);
    let (lo, hi) = shape::bounds(&shape::posed(sc, &rest)).unwrap_or(([-0.5; 3], [0.5; 3]));
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = 0.5 * ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 1.15;
    // A grid under the model (y = 0 when it stands on the ground) with the facing arrow.
    let ground = Some(if lo[1].abs() < 0.02 * r.max(1e-6) { 0.0 } else { lo[1] });
    let mut rows: Vec<Vec<Image>> = Vec::new();
    let views = [(0.0, 12.0), (45.0, 20.0), (90.0, 8.0), (180.0, 12.0), (270.0, 8.0), (30.0, 60.0)];
    rows.push(views.iter().map(|&(yaw, pitch)| render(sc, &texs, &rest, (c, r), ground, &View { yaw, pitch }, cell)).collect());
    for a in sc.anims.iter().take(4) {
        let d = a.duration();
        let frames = (0..6).map(|k| {
            let p = shape::pose(sc, Some((a, d * k as f32 / 6.0)));
            render(sc, &texs, &p, (c, r * 1.2), ground, &View { yaw: 35.0, pitch: 25.0 }, cell)
        });
        rows.push(frames.collect());
    }
    let pad = 6;
    let cols = 6;
    let mut out = Image::new(cols * (cell + pad) + pad, rows.len() as u32 * (cell + pad) + pad, [24, 24, 30, 255]);
    for (ri, row) in rows.iter().enumerate() {
        for (ci, im) in row.iter().enumerate() {
            out.blit(im, pad + ci as u32 * (cell + pad), pad + ri as u32 * (cell + pad));
        }
    }
    out
}

/// Before and after a weight change (`--rigid`, `--pin`): for each clip (up
/// to 3), a row of six frames of `before` over the same frames of `after`,
/// from behind and to the side (where a cape shows).
pub fn compare_clips(before: &Scene, after: &Scene, facing_plus_z: bool, cell: u32) -> Image {
    let rest = shape::pose(after, None);
    let (lo, hi) = shape::bounds(&shape::posed(after, &rest)).unwrap_or(([-0.5; 3], [0.5; 3]));
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = 0.5 * ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 1.3;
    let ground = Some(if lo[1].abs() < 0.02 * r.max(1e-6) { 0.0 } else { lo[1] });
    let mut rows: Vec<Vec<Image>> = Vec::new();
    for (k, a) in after.anims.iter().enumerate().take(3) {
        for sc in [before, after] {
            let texs: Vec<Option<Tex>> = sc.images.iter().map(|im| Image::decode(&im.bytes).ok().map(|im| Tex { im })).collect();
            let anim = sc.anims.get(k).unwrap_or(a);
            let d = anim.duration();
            rows.push((0..6).map(|f| render(sc, &texs, &shape::pose(sc, Some((anim, d * f as f32 / 5.0))), (c, r), ground, &View { yaw: if facing_plus_z { 305.0 } else { 125.0 }, pitch: 12.0 }, cell)).collect());
        }
    }
    let pad = 6;
    let mut out = Image::new(6 * (cell + pad) + pad, rows.len().max(1) as u32 * (cell + pad) + pad, [24, 24, 30, 255]);
    for (ri, row) in rows.iter().enumerate() {
        // A thin bar left of each "after" row.
        for (ci, im) in row.iter().enumerate() {
            out.blit(im, pad + ci as u32 * (cell + pad), pad + ri as u32 * (cell + pad));
        }
        if ri % 2 == 1 {
            for y in 0..cell {
                for x in 0..3 {
                    out.set(x, pad + ri as u32 * (cell + pad) + y, [90, 200, 120, 255]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::glb::tests::cube_scene;
    use super::*;

    #[test]
    fn draws_the_model_and_its_clips() {
        let mut sc = cube_scene();
        sc.anims.push(glb::Anim {
            name: "Hop".into(),
            channels: vec![glb::Channel { node: 0, path: glb::Path::T, interp: "LINEAR".into(), input: vec![0.0, 1.0], output: vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.0] }],
        });
        let sheet = contact_sheet(&sc, 64);
        assert_eq!((sheet.w, sheet.h), (6 * 70 + 6, 2 * 70 + 6));
        // The cube's reddish color shows in the middle of the first cell.
        let p = sheet.get(6 + 32, 6 + 32);
        assert!(p[0] > p[2] + 40, "{p:?}");
        // Same scene, same picture.
        assert_eq!(contact_sheet(&sc, 64), sheet);
    }
}
