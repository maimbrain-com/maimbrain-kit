//! Where a generated part points, and turning it into a convention: ported
//! from Bolt Brawl's socket_fit.py. Generators pose parts as they like (a
//! blade along a 3D diagonal, an armor plate leaning back 45°), so mb reads
//! the triangles: their area-weighted principal axes, which end of the long
//! axis is the tip (the slimmer end: a blade's point, a barrel's muzzle),
//! which side of the thin axis is the face (the side a glow is on, else the
//! convex side, where the middle stands proud of the rim), and turns the
//! model so those land on the axes `--orient` names. `mb model info` prints
//! the same analysis for any .glb, with an anchor and a socket offset.

use serde_json::{Value, json};

use super::glb::{self, Scene};
use super::img::Image;
use super::shape;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// A feature of a part's shape that `--orient` puts on an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// The long axis, pointing at its slimmer end (a blade's point, a barrel's muzzle).
    Tip,
    /// The thin axis, pointing out of its outer side (a plate's or shield's front).
    Face,
    /// The model's own up (+y as generated), square to the other feature.
    Up,
}

/// `--orient`: one or two features and the axes they go to.
#[derive(Clone, Debug, PartialEq)]
pub struct Orient {
    pub first: (Feature, V3),
    pub second: Option<(Feature, V3)>,
}

fn axis(s: &str) -> Option<V3> {
    Some(match s {
        "+x" | "x" => [1.0, 0.0, 0.0],
        "-x" => [-1.0, 0.0, 0.0],
        "+y" | "y" => [0.0, 1.0, 0.0],
        "-y" => [0.0, -1.0, 0.0],
        "+z" | "z" => [0.0, 0.0, 1.0],
        "-z" => [0.0, 0.0, -1.0],
        _ => return None,
    })
}

fn axis_name(v: V3) -> String {
    let k = (0..3).max_by(|&a, &b| v[a].abs().total_cmp(&v[b].abs())).unwrap();
    format!("{}{}", if v[k] < 0.0 { "-" } else { "+" }, ["x", "y", "z"][k])
}

impl Orient {
    /// "tip=+y", "tip=+z up=+y", "face=+z" (spaces or commas between).
    pub fn parse(s: &str) -> Result<Orient, String> {
        let bad = || format!("--orient {s:?}: say FEATURE=AXIS, e.g. tip=+y (blades, barrels), tip=+z up=+y (guns), face=+z (plates, shields); features tip, face, up; axes ±x, ±y, ±z");
        let mut out: Vec<(Feature, V3)> = Vec::new();
        for part in s.split([' ', ',']).filter(|p| !p.is_empty()) {
            let (f, a) = part.split_once('=').ok_or_else(bad)?;
            let f = match f.trim() {
                "tip" | "muzzle" | "point" => Feature::Tip,
                "face" | "front" => Feature::Face,
                "up" | "top" => Feature::Up,
                _ => return Err(bad()),
            };
            out.push((f, axis(a.trim()).ok_or_else(bad)?));
        }
        match out.as_slice() {
            [a] if a.0 != Feature::Up => Ok(Orient { first: *a, second: None }),
            [a, b] if a.0 != b.0 && dot(a.1, b.1).abs() < 0.5 => {
                // The shape feature leads; up only steers the turn about it.
                let (first, second) = if a.0 == Feature::Up { (*b, *a) } else { (*a, *b) };
                Ok(Orient { first, second: Some(second) })
            }
            _ => Err(bad()),
        }
    }

    /// The second feature, if not given: the model's own up to +y, unless the
    /// first is on ±y; then the flat side to +z (a blade facing the camera), or
    /// for a face on ±y (a lid), the tip to +z.
    fn second_or_default(&self) -> (Feature, V3) {
        if let Some(s) = self.second {
            return s;
        }
        if self.first.1[1].abs() < 0.5 {
            (Feature::Up, [0.0, 1.0, 0.0])
        } else if self.first.0 == Feature::Tip {
            (Feature::Face, [0.0, 0.0, 1.0])
        } else {
            (Feature::Tip, [0.0, 0.0, 1.0])
        }
    }
}

/// Words for a part's prompt asking for a plain rest pose that suits `orient`.
pub fn rest_pose_words(orient: Option<&str>) -> &'static str {
    let o = orient.and_then(|o| Orient::parse(o).ok());
    match o.map(|o| (o.first.0, o.first.1)) {
        Some((Feature::Tip, a)) if a[1].abs() > 0.5 => "Shown standing straight upright, its point or tip straight up and its flat side toward the viewer, not tilted.",
        Some((Feature::Tip, _)) => "Shown lying level and straight, pointing straight ahead, its top up, not tilted.",
        Some((Feature::Face, a)) if a[1].abs() > 0.5 => "Shown lying flat and level, its outer face straight up.",
        Some((Feature::Face, _)) => "Shown upright and face-on, its outer face straight toward the viewer, not leaning or tilted.",
        _ => "Shown in a plain rest pose, straight and level, square to the viewer, not tilted or at an angle.",
    }
}

/// What mb finds in a model's shape (its rest pose, in the file's space).
#[derive(Clone, Debug)]
pub struct Analysis {
    pub lo: V3,
    pub hi: V3,
    /// Area-weighted centroid of the surface.
    pub centroid: V3,
    /// Principal axes, longest first, and the model's extent along each.
    pub axes: [V3; 3],
    pub extents: [f32; 3],
    /// The long axis pointing at its slimmer end, and how many times slimmer it is.
    pub tip: V3,
    pub tip_ratio: f32,
    /// "slimmer end", or "away from the widest slice" when the ends are alike.
    pub tip_by: &'static str,
    /// The tip end's point (on the long axis through the centroid).
    pub tip_point: V3,
    /// The thin axis pointing out of the outer side, and how it was told ("glow", "convex", "unsure").
    pub face: V3,
    pub face_by: &'static str,
    /// Where the widest slice along the long axis is: a blade's guard, a gun's body.
    pub widest: V3,
    /// The glowing spot: middle and radius (of a disc of half its area).
    pub glow: Option<(V3, f32)>,
}

impl Analysis {
    /// Middle of the bounding box: what --origin center puts on the origin.
    pub fn center(&self) -> V3 {
        scale(add(self.lo, self.hi), 0.5)
    }

    /// Long, flat or a block: which feature `--orient` should use.
    pub fn shape(&self) -> &'static str {
        let [a, b, c] = self.extents;
        if a > 2.2 * b {
            "long"
        } else if c < 0.45 * b {
            "flat"
        } else {
            "block"
        }
    }

    pub fn facts(&self) -> Value {
        let r = |v: V3| json!(v.map(|x| (x * 1000.0).round() / 1000.0));
        json!({
            "bounds": [r(self.lo), r(self.hi)],
            "shape": self.shape(),
            "axes": self.axes.map(r),
            "extents": r(self.extents),
            "tip": r(self.tip),
            "tip_ratio": (self.tip_ratio * 100.0).round() / 100.0,
            "tip_by": self.tip_by,
            "face": r(self.face),
            "face_by": self.face_by,
            "anchor": r(self.center()),
            "centroid": r(self.centroid),
            "glow_spot": self.glow.map(|(c, rad)| json!({ "center": r(c), "radius": (rad * 1000.0).round() / 1000.0 })),
        })
    }
}

/// Symmetric 3×3 eigen-decomposition (Jacobi): eigenvalues and unit eigenvectors, largest first.
fn eigen(mut a: [[f32; 3]; 3]) -> ([f32; 3], [V3; 3]) {
    let mut v = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let (mut p, mut q, mut big) = (0, 1, 0.0f32);
        for i in 0..3 {
            for j in i + 1..3 {
                if a[i][j].abs() > big {
                    (p, q, big) = (i, j, a[i][j].abs());
                }
            }
        }
        if big < 1e-12 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        for k in 0..3 {
            let (akp, akq) = (a[k][p], a[k][q]);
            a[k][p] = c * akp - s * akq;
            a[k][q] = s * akp + c * akq;
        }
        for k in 0..3 {
            let (apk, aqk) = (a[p][k], a[q][k]);
            a[p][k] = c * apk - s * aqk;
            a[q][k] = s * apk + c * aqk;
        }
        for row in v.iter_mut() {
            let (vp, vq) = (row[p], row[q]);
            row[p] = c * vp - s * vq;
            row[q] = s * vp + c * vq;
        }
    }
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&i, &j| a[j][j].total_cmp(&a[i][i]));
    let vals = idx.map(|i| a[i][i]);
    let vecs = idx.map(|i| glb::normalize([v[0][i], v[1][i], v[2][i]]));
    (vals, vecs)
}

/// Reads the model's rest-pose triangles and finds its axes, tip and face.
pub fn analyze(sc: &Scene) -> Option<Analysis> {
    let posed = shape::posed(sc, &shape::pose(sc, None));
    let (lo, hi) = shape::bounds(&posed)?;
    // Triangles: centroid, area, and whether they glow (the emissive map at their UV middle).
    let mut glow_maps: Vec<Option<Image>> = Vec::new();
    for m in &sc.materials {
        let img = m.emissive_tex.as_ref().and_then(|t| sc.texture_image(t)).and_then(|i| Image::decode(&sc.images[i].bytes).ok());
        glow_maps.push(img);
    }
    let mut tris: Vec<(V3, f32, bool)> = Vec::new();
    let mut pts: Vec<V3> = Vec::new();
    for p in &posed {
        let prim = &sc.meshes[p.mesh].prims[p.prim];
        let glow = prim.material.and_then(|m| glow_maps.get(m)).and_then(|g| g.as_ref());
        pts.extend(&p.pos);
        for t in prim.indices.chunks(3) {
            let (a, b, c) = (p.pos[t[0] as usize], p.pos[t[1] as usize], p.pos[t[2] as usize]);
            let area = len(cross(sub(b, a), sub(c, a))) / 2.0;
            let cen = scale(add(add(a, b), c), 1.0 / 3.0);
            let lit = match (glow, &prim.uv) {
                (Some(g), Some(uv)) => {
                    let u = (uv[t[0] as usize][0] + uv[t[1] as usize][0] + uv[t[2] as usize][0]) / 3.0;
                    let v = (uv[t[0] as usize][1] + uv[t[1] as usize][1] + uv[t[2] as usize][1]) / 3.0;
                    let (x, y) = ((u.rem_euclid(1.0) * g.w as f32) as u32, (v.rem_euclid(1.0) * g.h as f32) as u32);
                    let px = g.get(x.min(g.w - 1), y.min(g.h - 1));
                    px[0].max(px[1]).max(px[2]) > 96
                }
                _ => false,
            };
            tris.push((cen, area, lit));
        }
    }
    let total: f32 = tris.iter().map(|t| t.1).sum();
    if !(total > 0.0) {
        return None;
    }
    let mu = scale(tris.iter().fold([0.0; 3], |s, t| add(s, scale(t.0, t.1))), 1.0 / total);
    let mut cov = [[0.0f32; 3]; 3];
    for (c, a, _) in &tris {
        let d = sub(*c, mu);
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += a * d[i] * d[j] / total;
            }
        }
    }
    let (_, mut axes) = eigen(cov);
    // Extents along each axis (from the vertices).
    let span = |ax: V3| {
        let (mut a, mut b) = (f32::MAX, f32::MIN);
        for p in &pts {
            let s = dot(sub(*p, mu), ax);
            a = a.min(s);
            b = b.max(s);
        }
        (a, b)
    };
    let mut extents = [0.0; 3];
    for k in 0..3 {
        let (a, b) = span(axes[k]);
        extents[k] = b - a;
    }
    // Keep the order longest first by extent (PCA's order can differ for hollow shapes).
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| extents[j].total_cmp(&extents[i]));
    axes = order.map(|i| axes[i]);
    extents = order.map(|i| extents[i]);
    let (e0, e1, e2) = (axes[0], axes[1], axes[2]);

    // The tip's axis: the principal long axis of a long shape (a blade on a diagonal); for a
    // blockier one (a gun with a big body), its own longer horizontal axis, as it was
    // generated level (socket_fit.py's muzzle()).
    let (t0, t1, t2) = if extents[0] > 1.6 * extents[1] {
        (e0, e1, e2)
    } else if hi[0] - lo[0] >= hi[2] - lo[2] {
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0])
    } else {
        ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0])
    };
    let (s_lo, s_hi) = span(t0);
    let l = (s_hi - s_lo).max(1e-9);
    let section = |from: f32, to: f32| {
        let (mut a1, mut b1, mut a2, mut b2, mut n) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN, 0);
        for p in &pts {
            let d = sub(*p, mu);
            let s = dot(d, t0);
            if s >= from && s <= to {
                let (x, y) = (dot(d, t1), dot(d, t2));
                (a1, b1, a2, b2) = (a1.min(x), b1.max(x), a2.min(y), b2.max(y));
                n += 1;
            }
        }
        if n < 3 { 0.0 } else { ((b1 - a1) * (b2 - a2)).max(1e-12) }
    };
    // The widest of 30 slices along it (by its width along the middle axis): a guard, a body.
    let mut best = (0.0f32, mu, 0.0f32);
    for k in 0..30 {
        let (a, b) = (s_lo + l * k as f32 / 30.0, s_lo + l * (k + 1) as f32 / 30.0);
        let (mut w0, mut w1, mut n) = (f32::MAX, f32::MIN, 0);
        for p in &pts {
            let d = sub(*p, mu);
            let s = dot(d, t0);
            if s >= a && s < b {
                let x = dot(d, t1);
                (w0, w1, n) = (w0.min(x), w1.max(x), n + 1);
            }
        }
        if n > 2 && w1 - w0 > best.0 {
            best = (w1 - w0, add(mu, scale(t0, (a + b) / 2.0)), (a + b) / 2.0);
        }
    }
    // The tip is the slimmer end (cross-sections of the last fifth at each end); when the ends
    // are alike (a blade whose hilt is as slim as its point), the end farther from the widest slice.
    let (lo_sec, hi_sec) = (section(s_lo, s_lo + 0.2 * l), section(s_hi - 0.2 * l, s_hi));
    let ratio = lo_sec.max(hi_sec) / lo_sec.min(hi_sec).max(1e-12);
    let hi_is_tip = if ratio >= 1.5 { hi_sec <= lo_sec } else { s_hi - best.2 > best.2 - s_lo };
    let (tip, tip_s) = if hi_is_tip { (t0, s_hi) } else { (scale(t0, -1.0), s_lo) };
    let tip_ratio = if ratio >= 1.5 { ratio } else { (s_hi - best.2).max(best.2 - s_lo) / (s_hi - best.2).min(best.2 - s_lo).max(1e-6) };
    let tip_point = add(mu, scale(t0, tip_s));
    let tip_by = if ratio >= 1.5 { "slimmer end" } else { "away from the widest slice" };

    // Glow spot: area-weighted middle of the glowing triangles.
    let lit: f32 = tris.iter().filter(|t| t.2).map(|t| t.1).sum();
    let glow = (lit > 0.0).then(|| {
        let c = scale(tris.iter().filter(|t| t.2).fold([0.0; 3], |s, t| add(s, scale(t.0, t.1))), 1.0 / lit);
        (c, (lit / 2.0 / std::f32::consts::PI).sqrt())
    });

    // Face: the side the glow is on, else the convex side (the middle stands proud of the rim).
    let mut face = e2;
    let mut face_by = "unsure";
    if let Some((c, _)) = glow.filter(|(c, _)| dot(sub(*c, mu), e2).abs() > 0.05 * extents[2]) {
        if dot(sub(c, mu), e2) < 0.0 {
            face = scale(e2, -1.0);
        }
        face_by = "glow";
    } else {
        let mut rs: Vec<f32> = tris.iter().map(|t| {
            let d = sub(t.0, mu);
            (dot(d, e0).powi(2) + dot(d, e1).powi(2)).sqrt()
        }).collect();
        let mut sorted = rs.clone();
        sorted.sort_by(f32::total_cmp);
        let (r30, r80) = (sorted[sorted.len() * 3 / 10], sorted[(sorted.len() * 8 / 10).min(sorted.len() - 1)]);
        let (mut mid, mut mid_w, mut rim, mut rim_w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (t, r) in tris.iter().zip(rs.drain(..)) {
            let h = dot(sub(t.0, mu), e2);
            if r < r30 {
                (mid, mid_w) = (mid + h * t.1, mid_w + t.1);
            } else if r > r80 {
                (rim, rim_w) = (rim + h * t.1, rim_w + t.1);
            }
        }
        if mid_w > 0.0 && rim_w > 0.0 {
            let d = mid / mid_w - rim / rim_w;
            if d.abs() > 0.02 * extents[2] {
                face_by = "convex";
                if d < 0.0 {
                    face = scale(e2, -1.0);
                }
            }
        }
    }
    Some(Analysis { lo, hi, centroid: mu, axes, extents, tip, tip_ratio, tip_by, tip_point, face, face_by, widest: best.1, glow })
}

/// The model's direction for a feature.
fn feature_dir(a: &Analysis, f: Feature) -> V3 {
    match f {
        Feature::Tip => a.tip,
        Feature::Face => a.face,
        Feature::Up => [0.0, 1.0, 0.0],
    }
}

/// An orthonormal frame (columns) from a direction and a second one made square to it.
fn frame(a: V3, b: V3) -> Option<[V3; 3]> {
    let a = glb::normalize(a);
    let b = sub(b, scale(a, dot(a, b)));
    if len(b) < 1e-3 {
        return None;
    }
    let b = glb::normalize(b);
    Some([a, b, cross(a, b)])
}

/// Rotation matrix (columns) to a quaternion.
fn quat_of(m: [V3; 3]) -> [f32; 4] {
    // m[c][r]: column c.
    let (m00, m11, m22) = (m[0][0], m[1][1], m[2][2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let k = (tr + 1.0).sqrt() * 2.0;
        [(m[1][2] - m[2][1]) / k, (m[2][0] - m[0][2]) / k, (m[0][1] - m[1][0]) / k, 0.25 * k]
    } else if m00 > m11 && m00 > m22 {
        let k = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * k, (m[1][0] + m[0][1]) / k, (m[2][0] + m[0][2]) / k, (m[1][2] - m[2][1]) / k]
    } else if m11 > m22 {
        let k = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m[1][0] + m[0][1]) / k, 0.25 * k, (m[2][1] + m[1][2]) / k, (m[2][0] - m[0][2]) / k]
    } else {
        let k = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m[2][0] + m[0][2]) / k, (m[2][1] + m[1][2]) / k, 0.25 * k, (m[0][1] - m[1][0]) / k]
    };
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|x| x / l)
}

/// The turn that puts the analysed model's features on `o`'s axes.
pub fn rotation(a: &Analysis, o: &Orient) -> [f32; 4] {
    let (f2, d2) = o.second_or_default();
    let s1 = feature_dir(a, o.first.0);
    // A second feature along the first (an up that is the tip): fall back to another.
    let candidates = [feature_dir(a, f2), a.face, a.axes[1], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
    let src = candidates.iter().find_map(|&s2| frame(s1, s2));
    let dst = frame(o.first.1, d2);
    let (Some(src), Some(dst)) = (src, dst) else { return [0.0, 0.0, 0.0, 1.0] };
    // R = dst · srcᵀ, column by column.
    let mut r = [[0.0f32; 3]; 3];
    for c in 0..3 {
        for row in 0..3 {
            r[c][row] = (0..3).map(|k| dst[k][row] * src[k][c]).sum();
        }
    }
    quat_of(r)
}

/// `mb model info <glb>`: the analysis, as text.
pub fn info(path: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sc = glb::read(&bytes)?;
    let a = analyze(&sc).ok_or("the model has no triangles")?;
    let f = |v: V3| format!("({:+.3}, {:+.3}, {:+.3})", v[0], v[1], v[2]);
    let size = sub(a.hi, a.lo);
    println!("{}: {:.3} × {:.3} × {:.3} m (x × y × z), {} triangles", path.display(), size[0], size[1], size[2], sc.triangles());
    println!("  bounds {} to {}", f(a.lo), f(a.hi));
    for (k, name) in ["long", "middle", "thin"].iter().enumerate() {
        println!("  {name} axis {} ≈ {}, {:.3} m", f(a.axes[k]), axis_name(a.axes[k]), a.extents[k]);
    }
    println!("  shape: {}", a.shape());
    if let Some((fwd, by, unsure)) = super::weights::rig_forward(&sc) {
        let (axis, to_z) = super::weights::axis_name(fwd);
        println!(
            "  rig: faces {} ≈ {axis} by its {}{} (a turn of {:.0}° about +y faces it +z)",
            f(fwd),
            by.join(" and "),
            if unsure { ", which disagree" } else { "" },
            to_z.to_degrees()
        );
    }
    match a.tip_by {
        "slimmer end" => println!("  tip (the slimmer end): {} ≈ {}, {:.1}× slimmer than the other, at {}", f(a.tip), axis_name(a.tip), a.tip_ratio, f(a.tip_point)),
        _ => println!("  tip (ends alike; the end farther from the widest slice, {:.1}× as far): {} ≈ {}, at {}", a.tip_ratio, f(a.tip), axis_name(a.tip), f(a.tip_point)),
    }
    println!("  widest slice (a guard, a gun's body): {}", f(a.widest));
    println!("  face (out of the thin axis): {} ≈ {}, by {}", f(a.face), axis_name(a.face), a.face_by);
    if let Some((c, r)) = a.glow {
        println!("  glow spot: {} radius {:.3} m", f(c), r);
    }
    let c = a.center();
    println!("  anchor (the middle): {}; hang it on a socket with the model at {} under the socket's node", f(c), f(scale(c, -1.0)));
    let suggest = match a.shape() {
        "long" => Some(if a.extents[1] > 1.6 * a.extents[2] { "tip=+y face=+z (a blade) or tip=+z up=+y (a gun, a barrel)" } else { "tip=+z up=+y (a gun, a barrel) or tip=+y (a pole, a blade)" }),
        "flat" => Some("face=+z (a plate, a shield)"),
        _ if a.tip_by == "slimmer end" && a.tip_ratio >= 3.0 => Some("tip=+z up=+y (a gun: its muzzle forward)"),
        _ => None,
    };
    match suggest {
        Some(s) => println!("  --orient {s}"),
        None => println!("  a block: no tip or face stands out; --facing/--turn place it"),
    }
    if let Some(o) = suggest.and_then(|s| Orient::parse(s.split(" (").next().unwrap_or(s).split(" or ").next().unwrap_or(s)).ok()) {
        let q = rotation(&a, &o);
        println!("  that turn: quaternion [{:.4}, {:.4}, {:.4}, {:.4}] (x, y, z, w)", q[0], q[1], q[2], q[3]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::glb::{Material, Mesh, Node, Prim};
    use super::*;

    /// A blade along a tilted diagonal: a long thin box tapering to a point, with a wide guard
    /// near its other end.
    pub fn tilted_blade(dir: V3, flat: V3) -> Scene {
        let d = glb::normalize(dir);
        let n = glb::normalize(sub(flat, scale(d, dot(flat, d))));
        let w = cross(d, n);
        let mut p = Prim::default();
        // Rings along the blade: (position along, half width, half thickness).
        let rings: &[(f32, f32, f32)] = &[(-0.3, 0.04, 0.04), (-0.05, 0.04, 0.04), (-0.05, 0.25, 0.06), (0.0, 0.25, 0.06), (0.0, 0.08, 0.02), (1.4, 0.06, 0.015), (1.6, 0.005, 0.005)];
        for &(s, hw, ht) in rings {
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                p.pos.push(add(add(add([0.3, 1.0, -0.2], scale(d, s)), scale(w, a * hw)), scale(n, b * ht)));
            }
        }
        for r in 0..rings.len() as u32 - 1 {
            for k in 0..4u32 {
                let (a, b, c, e) = (r * 4 + k, r * 4 + (k + 1) % 4, (r + 1) * 4 + k, (r + 1) * 4 + (k + 1) % 4);
                p.indices.extend([a, b, c, b, e, c]);
            }
        }
        p.indices.extend([0, 1, 2, 0, 2, 3]);
        Scene {
            nodes: vec![Node { name: "blade".into(), mesh: Some(0), ..Node::default() }],
            roots: vec![0],
            meshes: vec![Mesh { name: "blade".into(), prims: vec![p], weights: vec![] }],
            materials: vec![Material::default()],
            ..Scene::default()
        }
    }

    /// A curved plate (a section of a cylinder bulging toward `out`), leaning.
    fn leaning_plate(out: V3, up: V3) -> Scene {
        let o = glb::normalize(out);
        let u = glb::normalize(sub(up, scale(o, dot(up, o))));
        let s = cross(u, o);
        let mut p = Prim::default();
        let (nx, ny) = (12u32, 16u32);
        for j in 0..=ny {
            for i in 0..=nx {
                let x = i as f32 / nx as f32 * 2.0 - 1.0;
                let y = j as f32 / ny as f32 * 2.0 - 1.0;
                let bulge = 0.25 * (1.0 - x * x);
                p.pos.push(add(add(scale(s, 0.7 * x), scale(u, y)), scale(o, bulge)));
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                let a = j * (nx + 1) + i;
                p.indices.extend([a, a + 1, a + nx + 1, a + 1, a + nx + 2, a + nx + 1]);
            }
        }
        Scene {
            nodes: vec![Node { name: "plate".into(), mesh: Some(0), ..Node::default() }],
            roots: vec![0],
            meshes: vec![Mesh { name: "plate".into(), prims: vec![p], weights: vec![] }],
            materials: vec![Material::default()],
            ..Scene::default()
        }
    }

    fn close(a: V3, b: V3) -> bool {
        len(sub(a, b)) < 0.05
    }

    #[test]
    fn parses_conventions() {
        assert_eq!(Orient::parse("tip=+y").unwrap().first, (Feature::Tip, [0.0, 1.0, 0.0]));
        let o = Orient::parse("up=+y,tip=-z").unwrap();
        assert_eq!(o.first, (Feature::Tip, [0.0, 0.0, -1.0]));
        assert_eq!(o.second, Some((Feature::Up, [0.0, 1.0, 0.0])));
        for bad in ["tip", "tip=+w", "up=+y", "tip=+y face=+y", "tip=+y tip=+z", "nose=+z"] {
            assert!(Orient::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn finds_a_blades_tip_and_stands_it_up() {
        let dir = [0.6, -0.3, 0.74];
        let sc = tilted_blade(dir, [0.0, 1.0, 0.2]);
        let a = analyze(&sc).unwrap();
        assert_eq!(a.shape(), "long");
        assert!(close(a.tip, glb::normalize(dir)), "tip {:?}", a.tip);
        assert!(a.tip_ratio > 3.0, "{}", a.tip_ratio);
        let q = rotation(&a, &Orient::parse("tip=+y").unwrap());
        assert!(close(glb::rotate(q, a.tip), [0.0, 1.0, 0.0]), "{:?}", glb::rotate(q, a.tip));
        // Its flat side faces the camera (±z).
        assert!(glb::rotate(q, a.axes[2])[2].abs() > 0.95);
        // A gun's convention: muzzle forward, top up.
        let q = rotation(&a, &Orient::parse("tip=+z up=+y").unwrap());
        assert!(close(glb::rotate(q, a.tip), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn finds_a_plates_outer_face() {
        let out = glb::normalize([0.0, 0.7, 0.7]);
        let sc = leaning_plate(out, [0.0, 1.0, 0.0]);
        let a = analyze(&sc).unwrap();
        assert_eq!(a.shape(), "flat");
        assert_eq!(a.face_by, "convex");
        assert!(close(a.face, out), "face {:?}", a.face);
        let q = rotation(&a, &Orient::parse("face=+z").unwrap());
        assert!(close(glb::rotate(q, a.face), [0.0, 0.0, 1.0]));
        // Its top stays up as far as it can: the plate's own up lands on +y.
        assert!(glb::rotate(q, glb::normalize([0.0, 0.7, -0.7]))[1] > 0.95);
        // Upside down (bulging the other way): the face flips.
        let a = analyze(&leaning_plate(scale(out, -1.0), [0.0, 1.0, 0.0])).unwrap();
        assert!(close(a.face, scale(out, -1.0)));
    }
}
