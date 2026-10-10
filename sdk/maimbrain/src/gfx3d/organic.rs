//! Organic shapes for meshes built in code: a smooth [`Spline`], lofts (a
//! cross-section swept along a spine), smooth normals with a crease angle,
//! Loop subdivision with creases, and a fish built from them
//! ([`MeshData::fish`], an example as much as a helper).
//!
//! Everything here is plain Rust on `MeshData`, so it runs (and is tested)
//! natively, and gives the same mesh every time for the same inputs.

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, DefaultHasher};
use std::f32::consts::{PI, TAU};

use super::math::{Vec3, vec3};
use super::mesh::MeshData;

/// A smooth curve through points (centripetal Catmull-Rom: no loops or
/// overshoot at uneven spacing), measured by arc length.
#[derive(Clone, Debug)]
pub struct Spline {
    samples: Vec<Vec3>,
    /// Arc length at each sample.
    lengths: Vec<f32>,
}

/// A lookup table with a fixed hasher. Only ever looked up, never iterated,
/// so results don't depend on its order (the meshes are the same every run).
type Lookup<K, V> = HashMap<K, V, BuildHasherDefault<DefaultHasher>>;

/// Samples per span between two control points.
const SPAN_SAMPLES: usize = 32;

fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let knot = |a: Vec3, b: Vec3| a.distance(b).sqrt().max(1e-4);
    let (t0, t1) = (0.0, knot(p0, p1));
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let tt = t1 + (t2 - t1) * t;
    let mix = |a: Vec3, b: Vec3, ta: f32, tb: f32| a * ((tb - tt) / (tb - ta)) + b * ((tt - ta) / (tb - ta));
    let (a1, a2, a3) = (mix(p0, p1, t0, t1), mix(p1, p2, t1, t2), mix(p2, p3, t2, t3));
    let (b1, b2) = (mix(a1, a2, t0, t2), mix(a2, a3, t1, t3));
    mix(b1, b2, t1, t2)
}

impl Spline {
    /// The curve through `points` in order (one point gives a point; two, a line).
    pub fn new(points: &[Vec3]) -> Spline {
        let mut samples = Vec::new();
        match points.len() {
            0 => samples.push(Vec3::ZERO),
            1 => samples.push(points[0]),
            n => {
                let at = |i: isize| -> Vec3 {
                    if i < 0 {
                        points[0] * 2.0 - points[1]
                    } else if i as usize >= n {
                        points[n - 1] * 2.0 - points[n - 2]
                    } else {
                        points[i as usize]
                    }
                };
                for i in 0..n - 1 {
                    let i = i as isize;
                    for k in 0..SPAN_SAMPLES {
                        samples.push(catmull_rom(at(i - 1), at(i), at(i + 1), at(i + 2), k as f32 / SPAN_SAMPLES as f32));
                    }
                }
                samples.push(points[n - 1]);
            }
        }
        let mut lengths = vec![0.0; samples.len()];
        for i in 1..samples.len() {
            lengths[i] = lengths[i - 1] + samples[i].distance(samples[i - 1]);
        }
        Spline { samples, lengths }
    }

    /// Its length in meters.
    pub fn length(&self) -> f32 {
        *self.lengths.last().unwrap()
    }

    /// The point `t` (0–1, clamped) of the way along it, by length.
    pub fn at(&self, t: f32) -> Vec3 {
        let n = self.samples.len();
        if n == 1 || self.length() <= 0.0 {
            return self.samples[0];
        }
        let s = t.clamp(0.0, 1.0) * self.length();
        let i = self.lengths.partition_point(|&l| l <= s).clamp(1, n - 1);
        let (l0, l1) = (self.lengths[i - 1], self.lengths[i]);
        self.samples[i - 1].lerp(self.samples[i], if l1 > l0 { (s - l0) / (l1 - l0) } else { 0.0 })
    }

    /// The unit direction of travel at `t` (+z for a curve that is a point).
    pub fn tangent(&self, t: f32) -> Vec3 {
        let e = 1.0 / (self.samples.len() as f32 * 2.0);
        let (a, b) = (self.at((t - e).max(0.0)), self.at((t + e).min(1.0)));
        let d = b - a;
        if d.length_squared() > 1e-16 { d.normalize() } else { Vec3::Z }
    }
}

/// How [`MeshData::loft`] sweeps a section along a spine.
#[derive(Clone, Debug)]
pub struct Loft {
    /// Points the spine passes through, start to end (≥ 2); it's a smooth [`Spline`] through them.
    pub spine: Vec<Vec3>,
    /// Which way the section's +y points at the start of the spine. Along
    /// the spine the section turns with it but never twists (a
    /// rotation-minimizing frame), so a tentacle curling sideways keeps its
    /// top on top.
    pub up: Vec3,
    /// Sections along the spine (≥ 2).
    pub rings: u32,
    /// Segments across each section (≥ 1 open, ≥ 3 closed).
    pub sides: u32,
    /// A closed section loops around (a tube, a body, a tentacle); an open one
    /// is a strip from one edge to the other (a fin, a leaf, a banner, a hull's side).
    pub closed: bool,
    /// Closes a closed section's two ends with flat caps (ends that shrink to
    /// a point close themselves and need none).
    pub caps: bool,
    /// 0–0.9: puts more sections near the two ends, where noses and tips curve fastest.
    pub ends: f32,
    /// Normals are smoothed across edges flatter than this angle (radians)
    /// and kept hard across sharper ones (caps, a keel).
    pub crease: f32,
}

impl Default for Loft {
    fn default() -> Loft {
        Loft { spine: Vec::new(), up: Vec3::Y, rings: 24, sides: 16, closed: true, caps: true, ends: 0.0, crease: 1.0 }
    }
}

impl MeshData {
    /// Sweeps a cross-section along a smooth spine: bodies, tails, fins,
    /// snakes, tentacles, leaves, hulls, banners.
    ///
    /// `section(t, u)` gives the section's point at `t` (0–1 along the spine,
    /// by length) and `u` (0–1 across the section; a closed section's u = 1
    /// is u = 0 again) in the section's frame: x to the side, y up (along
    /// [`Loft::up`], kept twist-free), z along the spine (an offset, e.g. to
    /// cut a fork into a tail's trailing edge). A section that goes counter-
    /// clockwise in x/y (x right, y up) faces outward, as
    /// `[(u * TAU).cos() * w, (u * TAU).sin() * h, 0.0]` does; an open strip
    /// faces where `d/du × d/dt` points (draw it with
    /// [`Material::DOUBLE_SIDED`](super::Material::DOUBLE_SIDED) or add
    /// [`double_sided`](Self::double_sided)). Scale, twist or change the
    /// shape per `t` inside the closure.
    ///
    /// UVs: u across the section, v along the spine (0 at the start). Normals
    /// are smooth, hard across edges sharper than `crease`.
    ///
    /// ```ignore
    /// // A tentacle: a circle shrinking along a curling spine.
    /// let arm = MeshData::loft(
    ///     &Loft { spine: vec![vec3(0.0, 0.0, 0.0), vec3(0.0, -0.6, 0.2), vec3(0.3, -1.0, 0.1), vec3(0.4, -1.1, -0.2)], rings: 32, sides: 12, ..Default::default() },
    ///     |t, u| { let r = 0.12 * (1.0 - t); vec3((u * TAU).cos() * r, (u * TAU).sin() * r, 0.0) },
    /// );
    /// ```
    pub fn loft(desc: &Loft, section: impl Fn(f32, f32) -> Vec3) -> MeshData {
        let mut m = MeshData::new();
        if desc.spine.len() < 2 {
            return m;
        }
        let spline = Spline::new(&desc.spine);
        let rings = desc.rings.max(2) as usize;
        let sides = desc.sides.max(if desc.closed { 3 } else { 1 }) as usize;
        let ends = desc.ends.clamp(0.0, 0.9);
        // Stations and their twist-free frames (double reflection, Wang et al. 2008).
        let ts: Vec<f32> = (0..rings)
            .map(|r| {
                let t = r as f32 / (rings - 1) as f32;
                (t - ends * (TAU * t).sin() / TAU).clamp(0.0, 1.0)
            })
            .collect();
        let pos: Vec<Vec3> = ts.iter().map(|&t| spline.at(t)).collect();
        let tan: Vec<Vec3> = ts.iter().map(|&t| spline.tangent(t)).collect();
        let mut up = Vec::with_capacity(rings);
        let mut u0 = desc.up - tan[0] * desc.up.dot(tan[0]);
        if u0.length_squared() < 1e-10 {
            u0 = if tan[0].y.abs() < 0.9 { Vec3::Y - tan[0] * tan[0].y } else { Vec3::Z - tan[0] * tan[0].z };
        }
        up.push(u0.normalize());
        for i in 0..rings - 1 {
            let v1 = pos[i + 1] - pos[i];
            let c1 = v1.length_squared();
            let (mut r, mut t) = (up[i], tan[i]);
            if c1 > 1e-12 {
                r = r - v1 * (2.0 / c1 * v1.dot(r));
                t = t - v1 * (2.0 / c1 * v1.dot(t));
            }
            let v2 = tan[i + 1] - t;
            let c2 = v2.length_squared();
            if c2 > 1e-12 {
                r = r - v2 * (2.0 / c2 * v2.dot(r));
            }
            // Keep it exactly perpendicular and unit despite rounding.
            let r = (r - tan[i + 1] * r.dot(tan[i + 1])).normalize();
            up.push(if r == Vec3::ZERO { up[i] } else { r });
        }
        let point = |r: usize, k: usize| -> Vec3 {
            let u = if desc.closed && k == sides { 0.0 } else { k as f32 / sides as f32 };
            let s = section(ts[r], u);
            let x = up[r].cross(tan[r]);
            pos[r] + x * s.x + up[r] * s.y + tan[r] * s.z
        };
        let w = sides as u32 + 1;
        for r in 0..rings {
            for k in 0..=sides {
                m.push(point(r, k), Vec3::Y, [k as f32 / sides as f32, ts[r]]);
            }
        }
        for r in 0..rings as u32 - 1 {
            for k in 0..sides as u32 {
                let (a, b, c, d) = (r * w + k, r * w + k + 1, (r + 1) * w + k, (r + 1) * w + k + 1);
                m.tri(a, b, c);
                m.tri(b, d, c);
            }
        }
        if desc.closed && desc.caps {
            for (r, out) in [(0, -tan[0]), (rings - 1, tan[rings - 1])] {
                let ring: Vec<Vec3> = (0..sides).map(|k| m.positions[r * w as usize + k]).collect();
                let center = ring.iter().fold(Vec3::ZERO, |a, &p| a + p) / sides as f32;
                let reach = ring.iter().map(|p| p.distance(center)).fold(0.0, f32::max);
                if reach < 1e-5 {
                    continue;
                }
                let x = up[r].cross(tan[r]);
                let uv = |p: Vec3| [0.5 + (p - center).dot(x) / (2.0 * reach), 0.5 - (p - center).dot(up[r]) / (2.0 * reach)];
                let c = m.push(center, out, [0.5, 0.5]);
                let first = m.vertex_count() as u32;
                for p in &ring {
                    m.push(*p, out, uv(*p));
                }
                for k in 0..sides as u32 {
                    let (a, b) = (first + k, first + (k + 1) % sides as u32);
                    if r == 0 { m.tri(c, b, a) } else { m.tri(c, a, b) }
                }
            }
        }
        m.drop_degenerate();
        m.smooth_normals(desc.crease)
    }

    /// A copy with smooth normals: vertices at the same place share a normal
    /// (the angle-weighted average of the faces around them) across edges
    /// flatter than `crease` radians, and split across sharper ones. 0 gives
    /// facets like [`flat_shaded`](Self::flat_shaded); π smooths everything.
    /// UV seams and parts that merely touch are welded for shading too.
    pub fn smooth_normals(&self, crease: f32) -> MeshData {
        let mut src = self.clone();
        src.drop_degenerate();
        let cos = if crease >= PI - 1e-4 { -2.0 } else { crease.max(0.0).cos() };
        let weld = src.weld();
        let groups = weld.iter().map(|&g| g as usize + 1).max().unwrap_or(0);
        let faces: Vec<[usize; 3]> = src.indices.chunks_exact(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
        let p = &src.positions;
        let face_n: Vec<Vec3> = faces.iter().map(|&[a, b, c]| (p[b] - p[a]).cross(p[c] - p[a]).normalize()).collect();
        let angle: Vec<[f32; 3]> = faces
            .iter()
            .map(|t| {
                std::array::from_fn(|c| {
                    let (e1, e2) = ((p[t[(c + 1) % 3]] - p[t[c]]).normalize(), (p[t[(c + 2) % 3]] - p[t[c]]).normalize());
                    e1.dot(e2).clamp(-1.0, 1.0).acos()
                })
            })
            .collect();
        let mut around: Vec<Vec<(u32, u8)>> = vec![Vec::new(); groups];
        for (f, t) in faces.iter().enumerate() {
            for (c, &v) in t.iter().enumerate() {
                around[weld[v] as usize].push((f as u32, c as u8));
            }
        }
        let colored = !src.colors.is_empty();
        let skinned = src.is_skinned();
        // Source vertices that differ only by index (same place, UV, color
        // and skin) share an id, so their copies merge when normals agree too.
        let mut ids: Lookup<[u32; 15], u32> = Lookup::default();
        let same: Vec<u32> = (0..src.positions.len())
            .map(|v| {
                let mut key = [0u32; 15];
                key[0] = weld[v];
                let uv = src.uvs.get(v).copied().unwrap_or([0.0; 2]);
                key[1..3].copy_from_slice(&uv.map(f32::to_bits));
                if colored {
                    key[3] = src.colors[v];
                }
                if skinned {
                    key[4..8].copy_from_slice(&src.joints[v].map(u32::from));
                    key[8..12].copy_from_slice(&src.weights[v].map(f32::to_bits));
                }
                let next = ids.len() as u32;
                *ids.entry(key).or_insert(next)
            })
            .collect();
        let mut out = MeshData::new();
        let mut made: Lookup<(u32, [u32; 3]), u32> = Lookup::default();
        for (f, t) in faces.iter().enumerate() {
            let mut tri = [0u32; 3];
            for (c, &v) in t.iter().enumerate() {
                let mut n = Vec3::ZERO;
                for &(g, gc) in &around[weld[v] as usize] {
                    let g = g as usize;
                    if face_n[g].dot(face_n[f]) >= cos {
                        n += face_n[g] * angle[g][gc as usize];
                    }
                }
                let n = if n.length_squared() > 1e-20 { n.normalize() } else { face_n[f] };
                tri[c] = *made.entry((same[v], [n.x.to_bits(), n.y.to_bits(), n.z.to_bits()])).or_insert_with(|| {
                    out.positions.push(src.positions[v]);
                    out.normals.push(n);
                    out.uvs.push(src.uvs.get(v).copied().unwrap_or([0.0; 2]));
                    if colored {
                        out.colors.push(src.colors[v]);
                    }
                    if skinned {
                        out.joints.push(src.joints[v]);
                        out.weights.push(src.weights[v]);
                    }
                    out.positions.len() as u32 - 1
                });
            }
            out.indices.extend(tri);
        }
        out
    }

    /// Loop subdivision: each step splits every triangle in four and moves
    /// the vertices toward a smooth limit surface, so a coarse cage (a
    /// jittered icosphere, a boxy lantern, a low-poly hull) becomes smooth.
    /// Edges sharper than `crease` radians, and open borders, stay sharp
    /// (they're smoothed along their length only); π smooths everything.
    /// Shapes shrink a little toward their cage, as subdivision surfaces do.
    /// UVs, colors and skin weights are interpolated; normals are
    /// recomputed with the same crease. Each level multiplies triangles by
    /// 4: 1–3 levels.
    pub fn subdivide(&self, levels: u32, crease: f32) -> MeshData {
        let mut m = self.clone();
        m.drop_degenerate();
        for _ in 0..levels.min(5) {
            m = m.subdivide_once(crease);
        }
        m.smooth_normals(crease)
    }

    fn subdivide_once(&self, crease: f32) -> MeshData {
        let cos = if crease >= PI - 1e-4 { -2.0 } else { crease.max(0.0).cos() };
        let weld = self.weld();
        let nt = weld.iter().map(|&g| g as usize + 1).max().unwrap_or(0);
        // Where each welded vertex is (the first input vertex there).
        let mut tp = vec![Vec3::ZERO; nt];
        for (i, &g) in weld.iter().enumerate().rev() {
            tp[g as usize] = self.positions[i];
        }
        let faces: Vec<[u32; 3]> = self.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let tf: Vec<[u32; 3]> = faces.iter().map(|t| t.map(|v| weld[v as usize])).collect();
        let face_n: Vec<Vec3> = tf.iter().map(|&[a, b, c]| (tp[b as usize] - tp[a as usize]).cross(tp[c as usize] - tp[a as usize]).normalize()).collect();
        // Welded edges: the faces on them and the vertex opposite in each.
        let mut edges: BTreeMap<(u32, u32), Vec<(usize, u32)>> = BTreeMap::new();
        for (f, t) in tf.iter().enumerate() {
            if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
                continue;
            }
            for c in 0..3 {
                let (a, b) = (t[c], t[(c + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push((f, t[(c + 2) % 3]));
            }
        }
        let sharp = |list: &Vec<(usize, u32)>| list.len() != 2 || face_n[list[0].0].dot(face_n[list[1].0]) < cos;
        let edge_point: BTreeMap<(u32, u32), Vec3> = edges
            .iter()
            .map(|(&(a, b), list)| {
                let (pa, pb) = (tp[a as usize], tp[b as usize]);
                let p = if sharp(list) { (pa + pb) * 0.5 } else { (pa + pb) * 0.375 + (tp[list[0].1 as usize] + tp[list[1].1 as usize]) * 0.125 };
                ((a, b), p)
            })
            .collect();
        // New positions of the old vertices.
        let mut ring: Vec<Vec<u32>> = vec![Vec::new(); nt];
        let mut creases: Vec<Vec<u32>> = vec![Vec::new(); nt];
        for (&(a, b), list) in &edges {
            ring[a as usize].push(b);
            ring[b as usize].push(a);
            if sharp(list) {
                creases[a as usize].push(b);
                creases[b as usize].push(a);
            }
        }
        let moved: Vec<Vec3> = (0..nt)
            .map(|v| {
                let p = tp[v];
                match creases[v].len() {
                    0 if !ring[v].is_empty() => {
                        let n = ring[v].len() as f32;
                        let beta = if ring[v].len() == 3 { 3.0 / 16.0 } else { 3.0 / (8.0 * n) };
                        p * (1.0 - n * beta) + ring[v].iter().fold(Vec3::ZERO, |a, &q| a + tp[q as usize]) * beta
                    }
                    2 => p * 0.75 + (tp[creases[v][0] as usize] + tp[creases[v][1] as usize]) * 0.125,
                    _ => p,
                }
            })
            .collect();

        let colored = !self.colors.is_empty();
        let skinned = self.is_skinned();
        let mut out = MeshData::new();
        let push = |out: &mut MeshData, p: Vec3, uv: [f32; 2], color: Option<u32>, skin: Option<([u16; 4], [f32; 4])>| -> u32 {
            out.positions.push(p);
            out.normals.push(Vec3::Y);
            out.uvs.push(uv);
            if let Some(c) = color {
                out.colors.push(c);
            }
            if let Some((j, w)) = skin {
                out.joints.push(j);
                out.weights.push(w);
            }
            out.positions.len() as u32 - 1
        };
        let uv = |i: usize| self.uvs.get(i).copied().unwrap_or([0.0; 2]);
        let color = |i: usize| colored.then(|| self.colors[i]);
        let skin = |i: usize| skinned.then(|| (self.joints[i], self.weights[i]));
        let old: Vec<u32> = (0..self.positions.len()).map(|i| push(&mut out, moved[weld[i] as usize], uv(i), color(i), skin(i))).collect();
        let mut mids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for (f, t) in faces.iter().enumerate() {
            let w = tf[f];
            if w[0] == w[1] || w[1] == w[2] || w[2] == w[0] {
                continue;
            }
            let mut mid = [0u32; 3];
            for c in 0..3 {
                let (a, b) = (t[c], t[(c + 1) % 3]);
                let (wa, wb) = (w[c], w[(c + 1) % 3]);
                mid[c] = *mids.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    let (a, b) = (a as usize, b as usize);
                    let (ua, ub) = (uv(a), uv(b));
                    let c = colored.then(|| mix_color(self.colors[a], self.colors[b]));
                    let s = skinned.then(|| mix_skin((self.joints[a], self.weights[a]), (self.joints[b], self.weights[b])));
                    push(&mut out, edge_point[&(wa.min(wb), wa.max(wb))], [(ua[0] + ub[0]) * 0.5, (ua[1] + ub[1]) * 0.5], c, s)
                });
            }
            let [a, b, c] = t.map(|v| old[v as usize]);
            let [ab, bc, ca] = mid;
            out.indices.extend([a, ab, ca, b, bc, ab, c, ca, bc, ab, bc, ca]);
        }
        out
    }

    /// Adds a back face for every triangle (reversed winding, flipped
    /// normals), for thin things seen from both sides that should still be
    /// outlined or cast shadows with an ordinary material.
    pub fn double_sided(&self) -> MeshData {
        let mut back = self.clone();
        for n in &mut back.normals {
            *n = -*n;
        }
        for t in back.indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
        let mut m = self.clone();
        m.merge(&back);
        m
    }

    /// Per vertex, an id shared by every vertex at the same position (to 10 µm).
    fn weld(&self) -> Vec<u32> {
        let mut ids: Lookup<[i64; 3], u32> = Lookup::default();
        self.positions
            .iter()
            .map(|p| {
                let key = [p.x, p.y, p.z].map(|c| (c as f64 * 1e5).round() as i64);
                let next = ids.len() as u32;
                *ids.entry(key).or_insert(next)
            })
            .collect()
    }
}

fn mix_color(a: u32, b: u32) -> u32 {
    let mut out = 0;
    for s in [24, 16, 8, 0] {
        out |= ((((a >> s) & 255) + ((b >> s) & 255) + 1) / 2) << s;
    }
    out
}

/// Half of each vertex's skin, keeping the four heaviest joints.
fn mix_skin(a: ([u16; 4], [f32; 4]), b: ([u16; 4], [f32; 4])) -> ([u16; 4], [f32; 4]) {
    let mut acc: Vec<(u16, f32)> = Vec::with_capacity(8);
    for (j, w) in [a, b] {
        let sum: f32 = w.iter().sum::<f32>().max(1e-9);
        for k in 0..4 {
            if w[k] > 0.0 {
                match acc.iter_mut().find(|e| e.0 == j[k]) {
                    Some(e) => e.1 += w[k] / sum * 0.5,
                    None => acc.push((j[k], w[k] / sum * 0.5)),
                }
            }
        }
    }
    acc.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
    let mut out = ([0u16; 4], [0.0f32; 4]);
    for (k, (j, w)) in acc.into_iter().take(4).enumerate() {
        out.0[k] = j;
        out.1[k] = w;
    }
    if out.1.iter().sum::<f32>() <= 0.0 {
        out.1[0] = 1.0;
    }
    out
}

/// The shape of a fish for [`MeshData::fish`]. The defaults are a common koi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FishDesc {
    /// Nose to the tip of the tail fin, in meters. The fish faces −z (so
    /// `Quat::look_rotation(velocity, Vec3::Y)` points it along its path),
    /// centered on the origin, back up.
    pub length: f32,
    /// Body height and width where it's deepest, as fractions of `length`.
    pub height: f32,
    pub width: f32,
    /// Where the body is deepest, from the nose (0–1 of the body).
    pub peak: f32,
    /// The tail stalk's height, as a fraction of the deepest.
    pub stalk: f32,
    /// The tail fin's length (a fraction of `length`), its height (a
    /// fraction of the body's) and how deep its fork cuts in (0–1).
    pub tail: f32,
    pub tail_height: f32,
    pub fork: f32,
    /// Fin size: 1 a common koi; 2–3 a butterfly koi with long, flowing fins.
    pub fins: f32,
    /// Whiskers (barbels) at the mouth.
    pub barbels: bool,
    /// Bones along the spine for [`Skeleton::chain`](super::Skeleton::chain)
    /// with [`FishDesc::spine`] (2–64), so the body can swim; 0 for meshes without skin weights.
    pub bones: u32,
    /// Sections along the body, and points around it.
    pub rings: u32,
    pub sides: u32,
}

impl Default for FishDesc {
    fn default() -> FishDesc {
        FishDesc {
            length: 1.0,
            height: 0.24,
            width: 0.19,
            peak: 0.36,
            stalk: 0.26,
            tail: 0.24,
            tail_height: 1.05,
            fork: 0.35,
            fins: 1.0,
            barbels: true,
            bones: 8,
            rings: 44,
            sides: 28,
        }
    }
}

/// The parts of a fish ([`MeshData::fish`]), each skinned to the same bones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FishMesh {
    /// The body (with barbels): closed and smooth. UVs: u around the body,
    /// starting at its right side and going over the back; v from the nose
    /// (0) to the tail stalk (1). Color it with `with_vertex_colors_by` (its
    /// positions are the fish's own space) or a texture.
    pub body: MeshData,
    /// Tail, dorsal, pectoral, pelvic and anal fins: one-sided strips (use
    /// `Material::DOUBLE_SIDED`, usually with `Material::ALPHA`). Their vertex
    /// colors are white, with fin rays and alpha fading toward the edges.
    pub fins: MeshData,
    /// The two eyes: small spheres.
    pub eyes: MeshData,
}

impl FishDesc {
    /// Where the body ends (the tail stalk), as z.
    fn body_end(&self) -> f32 {
        -0.5 * self.length + self.length * (1.0 - self.tail * 0.85)
    }

    /// 0 at the nose, 1 at the tail stalk → z.
    fn z(&self, t: f32) -> f32 {
        let z0 = -0.5 * self.length;
        z0 + (self.body_end() - z0) * t
    }

    /// The body's depth along it (0–1 of the deepest): a rounded head, a
    /// smooth taper to the stalk, and a rounded end inside the tail fin.
    fn depth(&self, t: f32) -> f32 {
        let pk = self.peak.clamp(0.1, 0.8);
        let d = if t < pk {
            let a = 1.0 - (1.0 - t / pk).powf(2.2);
            a.max(0.0).sqrt()
        } else {
            let k = ((t - pk) / (1.0 - pk)).clamp(0.0, 1.0);
            self.stalk + (1.0 - self.stalk) * (0.5 + 0.5 * (PI * k).cos())
        };
        let end = ((t - 0.9) / 0.1).clamp(0.0, 1.0);
        d * (1.0 - end * end).max(0.0).sqrt()
    }

    /// The body's half height and half width, and its midline's height, at `t`.
    fn shape(&self, t: f32) -> (f32, f32, f32) {
        let (h, w) = (self.height * self.length * 0.5, self.width * self.length * 0.5);
        let d = self.depth(t);
        // An arched back and a mouth a little below the midline.
        let mid = h * (0.08 * (PI * t).sin() - 0.12 * (1.0 - t).powi(6));
        (h * d, w * d * (1.0 - 0.35 * t), mid)
    }

    /// The chain of joints along the spine, from behind the head to the
    /// tail fin, for [`Skeleton::chain`](super::Skeleton::chain).
    pub fn spine(&self) -> Vec<Vec3> {
        let n = self.bones.clamp(2, 64);
        let (a, b) = (-0.5 * self.length + 0.2 * self.length, 0.5 * self.length - 0.08 * self.length);
        (0..n).map(|i| vec3(0.0, 0.0, a + (b - a) * i as f32 / (n - 1) as f32)).collect()
    }
}

impl MeshData {
    /// A fish built from lofts: a smooth body with a rounded head and a
    /// tapering stalk, a forked tail fin, dorsal, pectoral, pelvic and anal
    /// fins, eyes and optional barbels, skinned to `desc.bones` joints along
    /// [`FishDesc::spine`] so [`Swim`](super::Swim) can make it undulate.
    /// `games/mb3d-demo/src/koi.rs` is the worked example (pattern, materials,
    /// swimming); read this function as a recipe for other creatures.
    pub fn fish(desc: &FishDesc) -> FishMesh {
        let d = *desc;
        let len = d.length;
        let mut body = MeshData::loft(
            &Loft { spine: vec![vec3(0.0, 0.0, d.z(0.0)), vec3(0.0, 0.0, d.z(1.0))], rings: d.rings.max(8), sides: d.sides.max(8), ends: 0.6, crease: 1.2, ..Default::default() },
            |t, u| {
                let (h, w, mid) = d.shape(t);
                let a = u * TAU;
                // A fuller-than-elliptical section, a little flatter below.
                let (c, s) = (a.cos(), a.sin());
                let full = |v: f32| v.signum() * v.abs().powf(0.85);
                vec3(full(c) * w, mid + full(s) * h * if s < 0.0 { 0.9 } else { 1.0 }, 0.0)
            },
        );
        if d.barbels {
            for side in [-1.0f32, 1.0] {
                let (h, w, mid) = d.shape(0.04);
                let root = vec3(side * w * 0.55, mid - h * 0.35, d.z(0.04));
                let spine = vec![root, root + vec3(side * 0.03, -0.015, -0.03) * len, root + vec3(side * 0.07, -0.04, -0.01) * len, root + vec3(side * 0.09, -0.05, 0.03) * len];
                let r = len * 0.0055;
                body.merge(&MeshData::loft(&Loft { spine, rings: 10, sides: 6, ..Default::default() }, |t, u| {
                    let r = r * (1.0 - 0.8 * t);
                    vec3((u * TAU).cos() * r, (u * TAU).sin() * r, 0.0)
                }));
            }
        }

        // Fins: open strips, colored white with rays and alpha that fades toward the edge.
        let mut fins = MeshData::new();
        let fin_color = |root_alpha: f32, edge_alpha: f32| {
            move |m: MeshData| -> MeshData {
                let mut m = m;
                m.colors = m
                    .uvs
                    .iter()
                    .map(|uv| {
                        // u across the fin (rays run along it), v from root (0) to edge (1).
                        let ray = 0.5 + 0.5 * (uv[0] * 22.0 * PI).cos();
                        let a = root_alpha + (edge_alpha - root_alpha) * uv[1].powf(0.8);
                        let shade = (255.0 * (1.0 - 0.18 * ray * (1.0 - uv[1] * 0.5))) as u32;
                        let a = ((a * (1.0 - 0.15 * ray)).clamp(0.0, 1.0) * 255.0) as u32;
                        (shade << 24) | (shade << 16) | (shade << 8) | a
                    })
                    .collect();
                m
            }
        };
        // The tail: a vertical strip from the stalk back, widening, with a forked trailing edge.
        let (_, _, smid) = d.shape(0.9);
        let sh = d.height * len * 0.5 * d.stalk;
        let tail_len = len * d.tail;
        let tail_h = d.height * len * 0.5 * d.tail_height * d.fins.sqrt();
        let z0 = d.z(0.86);
        let tail = MeshData::loft(
            &Loft { spine: vec![vec3(0.0, smid, z0), vec3(0.0, smid + sh * 0.1, (z0 + 0.5 * len) * 0.5), vec3(0.0, smid + sh * 0.15, 0.5 * len)], rings: 22, sides: 20, closed: false, ..Default::default() },
            |t, u| {
                let v = 2.0 * u - 1.0;
                let h = sh * 0.9 + (tail_h - sh * 0.9) * t.powf(0.6);
                // The lobes stay long; the middle of the trailing edge is cut in.
                let notch = d.fork * tail_len * t.powf(1.5) * (1.0 - v.abs().powf(1.4));
                // A soft S along the fin, so it isn't a flat card.
                vec3(0.012 * len * (PI * t).sin() * v, v * h, -notch)
            },
        );
        fins.merge(&fin_color(0.92, 0.45)(tail));

        // The dorsal fin: long and low along the top of the back, rising
        // quickly at its front and easing down toward the tail.
        let (ta, tb) = (0.3, 0.8);
        let top = |t: f32| {
            let (h, _, mid) = d.shape(t);
            vec3(0.0, mid + h * 0.9, d.z(t))
        };
        let dorsal_h = d.height * len * 0.24 * d.fins.powf(0.7);
        let dorsal = MeshData::loft(
            &Loft { spine: (0..6).map(|k| top(ta + (tb - ta) * k as f32 / 5.0)).collect(), rings: 26, sides: 8, closed: false, ..Default::default() },
            |t, u| {
                let rise = (t / 0.12).min(1.0).sqrt();
                let h = dorsal_h * rise * (1.0 - 0.55 * t) * (1.0 - t.powi(8)) + 0.004 * len;
                vec3(0.0, u * h, u * h * 0.4)
            },
        );
        // Here u runs root → edge and v along the base: make rays follow u.
        fins.merge(&fin_color(0.9, 0.4)(swap_uv(dorsal)));

        // A paddle fin rooted at `root`, reaching `reach` (meters) along `dir`, `width` across.
        let paddle = |root: Vec3, dir: Vec3, reach: f32, width: f32, droop: f32| {
            let dir = dir.normalize();
            let spine = vec![root, root + dir * reach * 0.5 + vec3(0.0, -droop * 0.3, 0.0), root + dir * reach + vec3(0.0, -droop, 0.0)];
            MeshData::loft(&Loft { spine, rings: 22, sides: 12, closed: false, ends: 0.3, ..Default::default() }, move |t, u| {
                let v = u - 0.5;
                // A fan: narrow at the root, widest past the middle, a rounded tip.
                let k = 0.55;
                let w = width * if t < k { 0.35 + 0.65 * (0.5 * PI * t / k).sin() } else { (1.0 - ((t - k) / (1.0 - k)).powi(2)).max(0.0).sqrt() };
                vec3(v * w, 0.25 * w * (2.0 * v) * (2.0 * v), 0.0)
            })
        };
        let mirror = super::math::Mat4::from_trs(Vec3::ZERO, super::math::Quat::IDENTITY, vec3(-1.0, 1.0, 1.0));
        let pair = |t: f32, below: f32, size: f32, reach: f32, out: f32| {
            let (h, w, mid) = d.shape(t);
            let root = vec3(w * 0.6, mid - h * below, d.z(t));
            let fin = paddle(root, vec3(out, -0.35, 1.0), reach * len * size, len * 0.07 * size.powf(0.7), len * 0.02 * size);
            let mut both = fin.clone();
            both.merge(&fin.transformed(&mirror));
            both
        };
        // Paired fins are skinned as if they reached back along the body (by
        // `trail` meters at their tips), so they sway behind the body wave
        // instead of sticking out rigidly.
        let mut trail = vec![0.0f32; fins.vertex_count()];
        for (t, below, size, reach, out) in [(0.27, 0.5, d.fins, 0.17, 0.9), (0.6, 0.75, d.fins * 0.6, 0.14, 0.45)] {
            let part = fin_color(0.9, 0.35)(pair(t, below, size, reach, out));
            trail.extend(part.uvs.iter().map(|uv| uv[1] * reach * len * size * 0.45));
            fins.merge(&part);
        }
        // The anal fin: under the tail end, swept back.
        let bottom = |t: f32| {
            let (h, _, mid) = d.shape(t);
            vec3(0.0, mid - h * 0.85, d.z(t))
        };
        let anal_h = d.height * len * 0.22 * d.fins.sqrt();
        let anal = MeshData::loft(
            &Loft { spine: (0..3).map(|k| bottom(0.74 + 0.12 * k as f32 / 2.0)).collect(), rings: 8, sides: 6, closed: false, ..Default::default() },
            |t, u| {
                let h = anal_h * (PI * t.powf(0.6)).sin().max(0.0) + 0.003 * len;
                vec3(0.0, -u * h, u * h * 0.6)
            },
        );
        fins.merge(&fin_color(0.9, 0.4)(swap_uv(anal)));

        // Eyes: a little proud of the head.
        let mut eyes = MeshData::new();
        for side in [-1.0f32, 1.0] {
            let t = 0.11;
            let (h, w, mid) = d.shape(t);
            let r = len * 0.021;
            let c = vec3(side * (w * 0.78), mid + h * 0.28, d.z(t));
            eyes.merge(&MeshData::uv_sphere(r, 12, 8).transformed(&super::math::Transform::at(c).with_scale3(vec3(0.6, 1.0, 1.0)).matrix()));
        }

        trail.resize(fins.vertex_count(), 0.0);
        let mut fish = FishMesh { body, fins, eyes };
        if d.bones > 0 {
            let spine = d.spine();
            fish.body = fish.body.with_chain_weights(&spine);
            fish.eyes = fish.eyes.with_chain_weights(&spine);
            let (joints, weights) = fish.fins.positions.iter().zip(&trail).map(|(&p, &dz)| super::mesh::chain_weight(&spine, p + vec3(0.0, 0.0, dz))).unzip();
            fish.fins.joints = joints;
            fish.fins.weights = weights;
        }
        fish
    }
}

/// Swaps a mesh's u and v.
fn swap_uv(mut m: MeshData) -> MeshData {
    for uv in &mut m.uvs {
        uv.swap(0, 1);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every edge (between welded positions) borders exactly two triangles.
    pub(crate) fn watertight(m: &MeshData) -> bool {
        let w = m.weld();
        let mut edges: BTreeMap<(u32, u32), i32> = BTreeMap::new();
        for t in m.indices.chunks_exact(3) {
            for c in 0..3 {
                let (a, b) = (w[t[c] as usize], w[t[(c + 1) % 3] as usize]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        edges.values().all(|&n| n == 2)
    }

    fn check(m: &MeshData, name: &str) {
        assert!(!m.indices.is_empty(), "{name}: empty");
        assert_eq!(m.normals.len(), m.positions.len(), "{name}");
        assert_eq!(m.uvs.len(), m.positions.len(), "{name}");
        for (i, n) in m.normals.iter().enumerate() {
            assert!((n.length() - 1.0).abs() < 1e-3, "{name}: normal {i} is {n:?}");
        }
        for uv in &m.uvs {
            assert!(uv.iter().all(|c| (-1e-5..=1.0 + 1e-5).contains(c)), "{name}: uv {uv:?}");
        }
        assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()), "{name}");
        assert!(m.positions.iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()), "{name}");
        if m.is_skinned() {
            assert_eq!(m.joints.len(), m.positions.len(), "{name}");
            assert_eq!(m.weights.len(), m.positions.len(), "{name}");
            for w in &m.weights {
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{name}: weights {w:?}");
            }
        }
    }

    /// Every triangle's winding agrees with its normals (front faces point outward).
    fn winds_outward(m: &MeshData) -> bool {
        m.indices.chunks_exact(3).all(|t| {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let f = (m.positions[b] - m.positions[a]).cross(m.positions[c] - m.positions[a]);
            f.length() < 1e-9 || f.dot(m.normals[a] + m.normals[b] + m.normals[c]) > 0.0
        })
    }

    fn tube(closed: bool, caps: bool) -> MeshData {
        MeshData::loft(&Loft { spine: vec![vec3(0.0, 0.0, 0.0), vec3(0.0, 0.5, -1.0), vec3(0.8, 0.6, -2.0)], rings: 20, sides: 12, closed, caps, ..Default::default() }, |t, u| {
            let r = 0.3 * (1.0 - 0.5 * t);
            vec3((u * TAU).cos() * r, (u * TAU).sin() * r, 0.0)
        })
    }

    #[test]
    fn splines_pass_through_their_points_and_measure_length() {
        let pts = [vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(1.0, 1.0, 0.0), vec3(3.0, 1.0, 0.0)];
        let s = Spline::new(&pts);
        assert!(s.at(0.0).distance(pts[0]) < 1e-6 && s.at(1.0).distance(pts[3]) < 1e-6);
        assert!(s.length() > 4.0 && s.length() < 4.6, "{}", s.length());
        let line = Spline::new(&[Vec3::ZERO, vec3(0.0, 0.0, -2.0)]);
        assert!((line.length() - 2.0).abs() < 1e-5);
        assert!(line.at(0.25).distance(vec3(0.0, 0.0, -0.5)) < 1e-5);
        assert!(line.tangent(0.5).distance(-Vec3::Z) < 1e-5);
    }

    #[test]
    fn lofts_are_watertight_smooth_and_wind_outward() {
        let capped = tube(true, true);
        check(&capped, "capped tube");
        assert!(watertight(&capped), "capped tube");
        assert!(winds_outward(&capped));
        // 20 rings of 13 points (the seam point twice, for its UVs), plus two caps of a center and 12.
        assert_eq!(capped.vertex_count(), 20 * 13 + 2 * 13);
        let open = tube(true, false);
        assert!(!watertight(&open), "an uncapped tube has open ends");
        // Seam normals are equal on both sides (smooth across the UV seam).
        let n0 = open.positions.iter().zip(&open.normals).filter(|(p, _)| p.distance(open.positions[0]) < 1e-6).map(|(_, n)| *n).collect::<Vec<_>>();
        assert!(n0.len() >= 2 && n0.windows(2).all(|w| w[0].distance(w[1]) < 1e-6), "{n0:?}");
        // A strip: open section, one-sided.
        let strip = tube(false, true);
        check(&strip, "strip");
        assert_eq!(strip.indices.len() / 3, 19 * 12 * 2);
        // A section shrinking to a point closes itself (a cone-like nose).
        let nose = MeshData::loft(&Loft { spine: vec![vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0)], rings: 16, sides: 10, ends: 0.5, ..Default::default() }, |t, u| {
            let r = 0.4 * (t * PI).sin();
            vec3((u * TAU).cos() * r, (u * TAU).sin() * r, 0.0)
        });
        check(&nose, "lemon");
        assert!(watertight(&nose) && winds_outward(&nose), "lemon");
    }

    #[test]
    fn smooth_normals_split_at_the_crease() {
        let cube = MeshData::cube(1.0);
        // Hard at 90°: face normals.
        assert_eq!(cube.smooth_normals(0.6).vertex_count(), 24);
        // Corners share a normal (vertices whose UVs differ stay apart).
        let round = cube.smooth_normals(PI);
        assert!((8..24).contains(&round.vertex_count()));
        assert!(round.normals.iter().zip(&round.positions).all(|(n, p)| n.distance(p.normalize()) < 1e-5));
        // A faceted sphere becomes smooth again.
        let s = MeshData::icosphere(1.0, 2).flat_shaded().smooth_normals(0.8);
        check(&s, "resmoothed sphere");
        assert_eq!(s.vertex_count(), 162);
        assert!(s.normals.iter().zip(&s.positions).all(|(n, p)| n.dot(p.normalize()) > 0.995));
    }

    #[test]
    fn subdivision_rounds_a_cage_and_keeps_creases() {
        let cage = MeshData::icosphere(1.0, 0).flat_shaded();
        let smooth = cage.subdivide(2, PI);
        check(&smooth, "subdivided icosahedron");
        assert_eq!(smooth.indices.len() / 3, 20 * 16);
        assert!(watertight(&smooth) && winds_outward(&smooth));
        // Points end up between the cage's inscribed and circumscribed spheres, close to round.
        let r: Vec<f32> = smooth.positions.iter().map(|p| p.length()).collect();
        let (lo, hi) = r.iter().fold((f32::MAX, 0.0f32), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(lo > 0.7 && hi < 1.0 && hi - lo < 0.12, "{lo} {hi}");
        // A cube with creases on every edge keeps its corners where they were.
        let cube = MeshData::cube(2.0).subdivide(2, 0.5);
        check(&cube, "creased cube");
        assert!(cube.positions.iter().any(|p| p.distance(vec3(1.0, 1.0, 1.0)) < 1e-5));
        assert!(cube.positions.iter().all(|p| p.x.abs().max(p.y.abs()).max(p.z.abs()) > 1.0 - 1e-5), "the faces stay flat");
        // Smoothed all over, it rounds off (and shrinks toward its center).
        let pebble = MeshData::cube(2.0).subdivide(3, PI);
        assert!(pebble.positions.iter().all(|p| p.length() < 1.6) && watertight(&pebble));
        // Colors and skin weights come along.
        let colored = MeshData::cube(1.0).with_vertex_colors(0x204060ff).with_joint(3).subdivide(1, PI);
        check(&colored, "colored");
        assert!(colored.colors.iter().all(|&c| c == 0x204060ff) && colored.joints.iter().all(|j| j[0] == 3));
    }

    #[test]
    fn the_fish_is_whole_skinned_and_faces_minus_z() {
        let d = FishDesc::default();
        let f = MeshData::fish(&d);
        check(&f.body, "body");
        check(&f.fins, "fins");
        check(&f.eyes, "eyes");
        assert!(watertight(&f.body), "the body (with its barbels) is closed");
        assert!(winds_outward(&f.body));
        let nose = f.body.positions.iter().map(|p| p.z).fold(f32::MAX, f32::min);
        let tip = f.fins.positions.iter().map(|p| p.z).fold(f32::MIN, f32::max);
        assert!((nose + 0.5).abs() < 0.02 && (tip - 0.5).abs() < 0.02, "{nose} {tip}");
        let depth = f.body.positions.iter().map(|p| p.y).fold(f32::MIN, f32::max) - f.body.positions.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!((depth - d.height).abs() < 0.05, "{depth}");
        // Skinned to the chain: the head follows joint 0, the tail the last joint.
        let last = d.bones as u16 - 1;
        let head = f.body.positions.iter().position(|p| p.z < -0.45).unwrap();
        assert_eq!((f.body.joints[head][0], f.body.weights[head][0]), (0, 1.0));
        let tail = f.fins.positions.iter().position(|p| p.z > 0.47).unwrap();
        assert_eq!(f.fins.joints[tail][0], last);
        assert!(f.fins.joints.iter().flatten().all(|&j| j <= last));
        // A koi's worth of triangles, not a million.
        let tris = (f.body.indices.len() + f.fins.indices.len() + f.eyes.indices.len()) / 3;
        assert!((3000..9000).contains(&tris), "{tris}");
        // Fins carry alpha that fades toward their edges.
        assert!(f.fins.colors.iter().any(|c| c & 255 < 140) && f.fins.colors.iter().any(|c| c & 255 > 200));
        // The same description gives the same mesh.
        assert_eq!(MeshData::fish(&d), f);
        assert!(MeshData::fish(&FishDesc { bones: 0, ..d }).body.joints.is_empty());
    }

    #[test]
    fn chain_weights_blend_around_joints() {
        let pts = [vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, 2.0)];
        let mut m = MeshData::new();
        for z in [-0.3, 0.2, 0.75, 1.0, 1.25, 1.9, 2.6] {
            m.push(vec3(0.1, 0.0, z), Vec3::Y, [0.0; 2]);
        }
        let m = m.with_chain_weights(&pts);
        let got: Vec<([u16; 4], [f32; 4])> = m.joints.iter().copied().zip(m.weights.iter().copied()).collect();
        assert_eq!(got[0], ([0; 4], [1.0, 0.0, 0.0, 0.0]), "before the chain: joint 0");
        assert_eq!(got[1], ([0; 4], [1.0, 0.0, 0.0, 0.0]));
        assert_eq!(got[3].0[..2], [1, 0]);
        assert!((got[3].1[0] - 0.5).abs() < 1e-5, "halfway at a joint");
        assert!(got[2].1[0] < 0.5 && got[4].1[0] > 0.5 && got[4].1[0] < 1.0, "blending around joint 1");
        assert_eq!(got[6], ([2, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]), "past the end: the last joint");
        assert!(got.iter().all(|(_, w)| (w.iter().sum::<f32>() - 1.0).abs() < 1e-6));
    }

    /// What building costs, natively (wasm is ~1.5–2× slower):
    /// `cargo test -p maimbrain --release build_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn build_cost() {
        let t = std::time::Instant::now();
        let f = MeshData::fish(&FishDesc::default());
        eprintln!("fish {:?} body {} fins {} verts", t.elapsed(), f.body.vertex_count(), f.fins.vertex_count());
        let t = std::time::Instant::now();
        let _ = f.body.smooth_normals(1.0);
        eprintln!("smooth body {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let l = MeshData::loft(&Loft { spine: vec![Vec3::ZERO, vec3(0.0, 0.0, 1.0)], rings: 44, sides: 28, ..Default::default() }, |_, u| vec3((u * TAU).cos(), (u * TAU).sin(), 0.0));
        eprintln!("loft {:?} {}", t.elapsed(), l.vertex_count());
        let t = std::time::Instant::now();
        let _ = MeshData::cube(1.0).subdivide(3, 0.9);
        eprintln!("subdivide {:?}", t.elapsed());
    }
}
