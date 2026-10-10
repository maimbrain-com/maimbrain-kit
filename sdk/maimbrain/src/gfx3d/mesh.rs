//! Mesh data built in code: primitives, lathes, extrusions, flat shading,
//! vertex colors, merging and transforming, and packing for `mb3d_mesh`.
//! Organic shapes (lofts, subdivision, smooth normals, the fish) are in
//! `organic.rs`; skin weights for meshes animated by bones (mb3d 2) are here.
//!
//! Triangles wind counter-clockwise seen from outside (the front face).

use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

use super::math::{Mat4, Vec3, vec3};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    /// Per-vertex sRGB colors `0xRRGGBBAA`, multiplied with the material's
    /// base color. Empty means white.
    pub colors: Vec<u32>,
    /// Skinning (mb3d 2): per vertex, four joint indices into the joints the
    /// mesh's node is bound to ([`Skeleton::bind`](super::Skeleton::bind),
    /// [`Node::set_skin`](super::Node::set_skin)), ≤ 63. Empty means not skinned.
    pub joints: Vec<[u16; 4]>,
    /// The four joints' weights per vertex (the host renormalizes them to sum to 1).
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// A vertex skinned wholly to joint 0.
const RIGID: ([u16; 4], [f32; 4]) = ([0; 4], [1.0, 0.0, 0.0, 0.0]);

impl MeshData {
    pub fn new() -> MeshData {
        MeshData::default()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Adds a vertex and returns its index.
    pub fn push(&mut self, pos: Vec3, normal: Vec3, uv: [f32; 2]) -> u32 {
        self.positions.push(pos);
        self.normals.push(normal);
        self.uvs.push(uv);
        if !self.colors.is_empty() {
            self.colors.push(0xffff_ffff);
        }
        if self.is_skinned() {
            self.joints.push(RIGID.0);
            self.weights.push(RIGID.1);
        }
        self.positions.len() as u32 - 1
    }

    /// Whether the mesh has joints and weights (mb3d 2).
    pub fn is_skinned(&self) -> bool {
        !self.joints.is_empty()
    }

    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    /// A quad centered at `c` spanning ±`u`, ±`v` (front face toward u × v).
    fn quad(&mut self, c: Vec3, u: Vec3, v: Vec3) {
        let n = u.cross(v).normalize();
        let a = self.push(c - u - v, n, [0.0, 1.0]);
        let b = self.push(c + u - v, n, [1.0, 1.0]);
        let d = self.push(c + u + v, n, [1.0, 0.0]);
        let e = self.push(c - u + v, n, [0.0, 0.0]);
        self.tri(a, b, d);
        self.tri(a, d, e);
    }

    /// An axis-aligned box centered at the origin.
    pub fn cuboid(size: Vec3) -> MeshData {
        let h = size * 0.5;
        let mut m = MeshData::new();
        let (x, y, z) = (vec3(h.x, 0.0, 0.0), vec3(0.0, h.y, 0.0), vec3(0.0, 0.0, h.z));
        m.quad(x, -z, y);
        m.quad(-x, z, y);
        m.quad(y, x, -z);
        m.quad(-y, x, z);
        m.quad(z, x, y);
        m.quad(-z, -x, y);
        m
    }

    pub fn cube(size: f32) -> MeshData {
        MeshData::cuboid(Vec3::splat(size))
    }

    /// A flat rectangle in the xz plane facing +y.
    pub fn plane(width: f32, depth: f32) -> MeshData {
        let mut m = MeshData::new();
        m.quad(Vec3::ZERO, vec3(width * 0.5, 0.0, 0.0), vec3(0.0, 0.0, -depth * 0.5));
        m
    }

    /// A Mode 7 floor (mb3d 2): a big square ground plane `2 × radius` across
    /// in the xz plane, facing +y, cut into `segments × segments` quads (so
    /// fog, lighting and the retro look's vertex snap have vertices to work
    /// with), its UVs the position over `tile` meters: a tiling pixel-art
    /// texture repeats once per tile, in world units. Draw it with
    /// [`Texture::nearest`](super::Texture::nearest) and an unlit material
    /// (the SNES look) and keep it centered under the camera with
    /// [`mode7_anchor`](super::mode7_anchor) so it never runs out; fog with
    /// the backdrop's horizon color fades its far edge into the
    /// [`backdrop`](super::backdrop). Turning or sliding the node rotates and
    /// scrolls the ground, as Mode 7 did.
    pub fn mode7_floor(radius: f32, tile: f32, segments: u32) -> MeshData {
        let n = segments.clamp(1, 128);
        let tile = if tile > 0.0 { tile } else { 1.0 };
        let mut m = MeshData::new();
        for j in 0..=n {
            for i in 0..=n {
                let x = -radius + 2.0 * radius * i as f32 / n as f32;
                let z = -radius + 2.0 * radius * j as f32 / n as f32;
                m.push(vec3(x, 0.0, z), vec3(0.0, 1.0, 0.0), [x / tile, z / tile]);
            }
        }
        let w = n + 1;
        for j in 0..n {
            for i in 0..n {
                let (a, b) = (j * w + i, (j + 1) * w + i);
                m.tri(a, b, a + 1);
                m.tri(a + 1, b, b + 1);
            }
        }
        m
    }

    /// A latitude/longitude sphere: `segments` around, `rings` from pole to pole.

    pub fn uv_sphere(radius: f32, segments: u32, rings: u32) -> MeshData {
        let (segments, rings) = (segments.max(3), rings.max(2));
        let mut m = MeshData::new();
        for r in 0..=rings {
            let th = r as f32 / rings as f32 * PI;
            for s in 0..=segments {
                let ph = s as f32 / segments as f32 * TAU;
                let n = vec3(th.sin() * ph.sin(), th.cos(), th.sin() * ph.cos());
                m.push(n * radius, n, [s as f32 / segments as f32, r as f32 / rings as f32]);
            }
        }
        let w = segments + 1;
        for r in 0..rings {
            for s in 0..segments {
                let (a, b) = (r * w + s, (r + 1) * w + s);
                if r > 0 {
                    m.tri(a, b, a + 1);
                }
                if r < rings - 1 {
                    m.tri(a + 1, b, b + 1);
                }
            }
        }
        m
    }

    /// A subdivided icosahedron: evenly spread triangles. `subdivisions` 0
    /// is the 20-face icosahedron; each step multiplies faces by 4. Smooth
    /// normals; call `flat_shaded()` for facets.
    pub fn icosphere(radius: f32, subdivisions: u32) -> MeshData {
        let t = (1.0 + 5f32.sqrt()) / 2.0;
        let mut pos: Vec<Vec3> = [
            (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
            (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
            (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| vec3(x, y, z).normalize())
        .collect();
        let mut faces: Vec<[u32; 3]> = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        for _ in 0..subdivisions.min(6) {
            let mut mid = BTreeMap::new();
            let mut midpoint = |a: u32, b: u32, pos: &mut Vec<Vec3>| -> u32 {
                *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    pos.push(((pos[a as usize] + pos[b as usize]) * 0.5).normalize());
                    pos.len() as u32 - 1
                })
            };
            let mut next = Vec::with_capacity(faces.len() * 4);
            for [a, b, c] in faces {
                let (ab, bc, ca) = (midpoint(a, b, &mut pos), midpoint(b, c, &mut pos), midpoint(c, a, &mut pos));
                next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
            }
            faces = next;
        }
        let mut m = MeshData::new();
        for p in &pos {
            let uv = [0.5 + p.x.atan2(p.z) / TAU, 0.5 - p.y.asin() / PI];
            m.push(*p * radius, *p, uv);
        }
        for [a, b, c] in faces {
            m.tri(a, b, c);
        }
        m
    }

    /// A capped cylinder along y, centered at the origin.
    pub fn cylinder(radius: f32, height: f32, segments: u32) -> MeshData {
        MeshData::frustum(radius, radius, height, segments)
    }

    /// A capped cone along y with its tip at +height/2.
    pub fn cone(radius: f32, height: f32, segments: u32) -> MeshData {
        MeshData::frustum(radius, 0.0, height, segments)
    }

    /// A capped truncated cone along y: `bottom` and `top` radii.
    pub fn frustum(bottom: f32, top: f32, height: f32, segments: u32) -> MeshData {
        let segments = segments.max(3);
        let h = height * 0.5;
        let mut m = MeshData::new();
        let slope = (bottom - top) / height.max(1e-6);
        for s in 0..=segments {
            let ph = s as f32 / segments as f32 * TAU;
            let (x, z) = (ph.sin(), ph.cos());
            let n = vec3(x, slope, z).normalize();
            let u = s as f32 / segments as f32;
            m.push(vec3(x * top, h, z * top), n, [u, 0.0]);
            m.push(vec3(x * bottom, -h, z * bottom), n, [u, 1.0]);
        }
        for s in 0..segments {
            let (a, b) = (s * 2, s * 2 + 1);
            if top > 0.0 {
                m.tri(a, b, a + 2);
            }
            if bottom > 0.0 {
                m.tri(a + 2, b, b + 2);
            }
        }
        for (y, r, up) in [(h, top, true), (-h, bottom, false)] {
            if r <= 0.0 {
                continue;
            }
            let n = if up { Vec3::Y } else { -Vec3::Y };
            let c = m.push(vec3(0.0, y, 0.0), n, [0.5, 0.5]);
            let first = m.vertex_count() as u32;
            for s in 0..=segments {
                let ph = s as f32 / segments as f32 * TAU;
                m.push(vec3(ph.sin() * r, y, ph.cos() * r), n, [0.5 + ph.sin() * 0.5, 0.5 - ph.cos() * 0.5]);
            }
            for s in 0..segments {
                let (a, b) = (first + s, first + s + 1);
                if up { m.tri(c, a, b) } else { m.tri(c, b, a) }
            }
        }
        m
    }

    /// A ring around y: `major` radius to the tube's center, `minor` tube radius.
    pub fn torus(major: f32, minor: f32, segments: u32, sides: u32) -> MeshData {
        let (segments, sides) = (segments.max(3), sides.max(3));
        let mut m = MeshData::new();
        for s in 0..=segments {
            let ph = s as f32 / segments as f32 * TAU;
            for k in 0..=sides {
                let th = k as f32 / sides as f32 * TAU;
                let n = vec3(th.cos() * ph.sin(), th.sin(), th.cos() * ph.cos());
                let p = vec3((major + minor * th.cos()) * ph.sin(), minor * th.sin(), (major + minor * th.cos()) * ph.cos());
                m.push(p, n, [s as f32 / segments as f32, k as f32 / sides as f32]);
            }
        }
        let w = sides + 1;
        for s in 0..segments {
            for k in 0..sides {
                let (a, b) = (s * w + k, (s + 1) * w + k);
                m.tri(a, b, a + 1);
                m.tri(a + 1, b, b + 1);
            }
        }
        m
    }

    /// Revolves a profile of (radius, y) points, listed bottom to top, around
    /// the y axis. Smooth normals along the profile; use `flat_shaded()` for
    /// a faceted look.
    pub fn lathe(profile: &[[f32; 2]], segments: u32) -> MeshData {
        let segments = segments.max(3);
        let mut m = MeshData::new();
        let n = profile.len();
        if n < 2 {
            return m;
        }
        // 2D outward normal per profile point: average of its edges' normals.
        let edge_n = |i: usize| {
            let (a, b) = (profile[i], profile[i + 1]);
            let (dr, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dr * dr + dy * dy).sqrt().max(1e-9);
            [dy / l, -dr / l]
        };
        let mut len = vec![0.0f32; n];
        for i in 1..n {
            let (dr, dy) = (profile[i][0] - profile[i - 1][0], profile[i][1] - profile[i - 1][1]);
            len[i] = len[i - 1] + (dr * dr + dy * dy).sqrt();
        }
        let total = len[n - 1].max(1e-9);
        for s in 0..=segments {
            let ph = s as f32 / segments as f32 * TAU;
            let (sx, cz) = (ph.sin(), ph.cos());
            for (i, p) in profile.iter().enumerate() {
                let e = match i {
                    0 => edge_n(0),
                    _ if i == n - 1 => edge_n(n - 2),
                    _ => {
                        let (a, b) = (edge_n(i - 1), edge_n(i));
                        [a[0] + b[0], a[1] + b[1]]
                    }
                };
                let nn = vec3(e[0] * sx, e[1], e[0] * cz).normalize();
                m.push(vec3(p[0] * sx, p[1], p[0] * cz), nn, [s as f32 / segments as f32, 1.0 - len[i] / total]);
            }
        }
        let w = n as u32;
        for s in 0..segments {
            for i in 0..w - 1 {
                let (a, b) = (s * w + i, (s + 1) * w + i);
                m.tri(a, b, b + 1);
                m.tri(a, b + 1, a + 1);
            }
        }
        m.drop_degenerate();
        m
    }

    /// Extrudes a simple 2D polygon (x, y; convex or concave) along z,
    /// centered: caps at ±depth/2 and flat sides.
    pub fn extrude(polygon: &[[f32; 2]], depth: f32) -> MeshData {
        let mut m = MeshData::new();
        if polygon.len() < 3 {
            return m;
        }
        let area: f32 = (0..polygon.len()).map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - b[0] * a[1]
        }).sum();
        let mut pts = polygon.to_vec();
        if area < 0.0 {
            pts.reverse();
        }
        let (lo, hi) = pts.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        let uv = |p: [f32; 2]| [(p[0] - lo[0]) / (hi[0] - lo[0]).max(1e-9), 1.0 - (p[1] - lo[1]) / (hi[1] - lo[1]).max(1e-9)];
        let tris = triangulate(&pts);
        let h = depth * 0.5;
        for (z, n) in [(h, Vec3::Z), (-h, -Vec3::Z)] {
            let base = m.vertex_count() as u32;
            for p in &pts {
                m.push(vec3(p[0], p[1], z), n, uv(*p));
            }
            for t in &tris {
                if z > 0.0 { m.tri(base + t[0], base + t[1], base + t[2]) } else { m.tri(base + t[0], base + t[2], base + t[1]) }
            }
        }
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let nn = vec3(b[1] - a[1], a[0] - b[0], 0.0).normalize();
            let v0 = m.push(vec3(a[0], a[1], h), nn, [0.0, 0.0]);
            let v1 = m.push(vec3(a[0], a[1], -h), nn, [0.0, 1.0]);
            let v2 = m.push(vec3(b[0], b[1], -h), nn, [1.0, 1.0]);
            let v3 = m.push(vec3(b[0], b[1], h), nn, [1.0, 0.0]);
            m.tri(v0, v1, v2);
            m.tri(v0, v2, v3);
        }
        m
    }

    /// Every triangle gets its own vertices and its face normal: the faceted low-poly look.
    pub fn flat_shaded(&self) -> MeshData {
        let mut m = MeshData::new();
        let colored = !self.colors.is_empty();
        for t in self.indices.chunks_exact(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let n = (self.positions[b] - self.positions[a]).cross(self.positions[c] - self.positions[a]).normalize();
            for i in [a, b, c] {
                m.positions.push(self.positions[i]);
                m.normals.push(n);
                m.uvs.push(self.uvs.get(i).copied().unwrap_or([0.0; 2]));
                if colored {
                    m.colors.push(self.colors[i]);
                }
                if self.is_skinned() {
                    m.joints.push(self.joints[i]);
                    m.weights.push(self.weights[i]);
                }
            }
            let k = m.positions.len() as u32;
            m.tri(k - 3, k - 2, k - 1);
        }
        m
    }

    /// Sets every vertex's color (sRGB `0xRRGGBBAA`).
    pub fn with_vertex_colors(mut self, rgba: u32) -> MeshData {
        self.colors = vec![rgba; self.positions.len()];
        self
    }

    /// Colors each vertex by a function of its position and normal.
    pub fn with_vertex_colors_by(mut self, f: impl Fn(Vec3, Vec3) -> u32) -> MeshData {
        self.colors = self.positions.iter().zip(&self.normals).map(|(p, n)| f(*p, *n)).collect();
        self
    }

    /// Skins every vertex wholly to joint `joint` (mb3d 2): a rigid part (an
    /// eye, a horn, a lantern on a pole) that moves with one bone. Merge it
    /// into a skinned mesh.
    pub fn with_joint(mut self, joint: u16) -> MeshData {
        self.joints = vec![[joint, 0, 0, 0]; self.positions.len()];
        self.weights = vec![[1.0, 0.0, 0.0, 0.0]; self.positions.len()];
        self
    }

    /// Skins the mesh to a chain of joints at `points` (mb3d 2), in the same
    /// space as the mesh: each vertex follows the joint whose bone (the
    /// segment to the next point) it's nearest, blending smoothly into the
    /// next joint around each joint so bends are soft. Vertices beyond the
    /// ends follow the first or last joint. Pass the same points to
    /// [`Skeleton::chain`](super::Skeleton::chain). At most 64 points.
    pub fn with_chain_weights(mut self, points: &[Vec3]) -> MeshData {
        let n = points.len().min(64);
        let weights: Vec<([u16; 4], [f32; 4])> = self.positions.iter().map(|&p| chain_weight(&points[..n], p)).collect();
        self.joints = weights.iter().map(|w| w.0).collect();
        self.weights = weights.iter().map(|w| w.1).collect();
        self
    }

    /// Appends `other`'s geometry.
    pub fn merge(&mut self, other: &MeshData) {
        let base = self.positions.len() as u32;
        // Colored if either side is (an empty mesh merging a colored one too: Mothglass friction #1).
        let colored = !self.colors.is_empty() || !other.colors.is_empty();
        if self.colors.is_empty() && !other.colors.is_empty() {
            self.colors = vec![0xffff_ffff; self.positions.len()];
        }
        // Likewise skinned if either side is: unskinned vertices follow joint 0
        // (use `with_joint` first to put a rigid part on another joint).
        let skinned = self.is_skinned() || other.is_skinned();
        if !self.is_skinned() && other.is_skinned() {
            self.joints = vec![RIGID.0; self.positions.len()];
            self.weights = vec![RIGID.1; self.positions.len()];
        }
        self.positions.extend(&other.positions);
        self.normals.extend(&other.normals);
        self.uvs.extend(&other.uvs);
        if colored {
            if other.colors.is_empty() {
                self.colors.extend(std::iter::repeat_n(0xffff_ffff, other.positions.len()));
            } else {
                self.colors.extend(&other.colors);
            }
        }
        if skinned {
            if other.is_skinned() {
                self.joints.extend(&other.joints);
                self.weights.extend(&other.weights);
            } else {
                self.joints.extend(std::iter::repeat_n(RIGID.0, other.positions.len()));
                self.weights.extend(std::iter::repeat_n(RIGID.1, other.positions.len()));
            }
        }
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// A copy with positions and normals transformed by `m`.
    pub fn transformed(&self, m: &Mat4) -> MeshData {
        let mut out = self.clone();
        for p in &mut out.positions {
            *p = m.transform_point(*p);
        }
        for n in &mut out.normals {
            *n = m.transform_normal(*n);
        }
        // A mirroring transform flips winding; flip it back.
        let mm = &m.0;
        let det = vec3(mm[0], mm[1], mm[2]).dot(vec3(mm[4], mm[5], mm[6]).cross(vec3(mm[8], mm[9], mm[10])));
        if det < 0.0 {
            for t in out.indices.chunks_exact_mut(3) {
                t.swap(1, 2);
            }
        }
        out
    }

    pub(super) fn drop_degenerate(&mut self) {
        let p = &self.positions;
        let mut kept = Vec::with_capacity(self.indices.len());
        for t in self.indices.chunks_exact(3) {
            let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
            if (b - a).cross(c - a).length_squared() > 1e-14 {
                kept.extend_from_slice(t);
            }
        }
        self.indices = kept;
    }

    /// The packed form `mb3d_mesh` takes (SPEC §5.4).
    pub fn pack(&self) -> Vec<u8> {
        let colored = !self.colors.is_empty();
        let skinned = self.is_skinned();
        let stride = 32 + if colored { 4 } else { 0 } + if skinned { 16 } else { 0 };
        let mut b = Vec::with_capacity(12 + self.positions.len() * stride + self.indices.len() * 4);
        let flags = if colored { 2 } else { 0 } | if skinned { 4 } else { 0 };
        for w in [self.positions.len() as u32, self.indices.len() as u32, flags] {
            b.extend(w.to_le_bytes());
        }
        for i in 0..self.positions.len() {
            let n = self.normals.get(i).copied().unwrap_or(Vec3::Y);
            let uv = self.uvs.get(i).copied().unwrap_or([0.0; 2]);
            for x in [self.positions[i].x, self.positions[i].y, self.positions[i].z, n.x, n.y, n.z, uv[0], uv[1]] {
                b.extend(x.to_le_bytes());
            }
            if colored {
                b.extend(self.colors.get(i).copied().unwrap_or(0xffff_ffff).to_be_bytes());
            }
            if skinned {
                let (j, w) = (self.joints.get(i).copied().unwrap_or(RIGID.0), self.weights.get(i).copied().unwrap_or(RIGID.1));
                let w = w.map(|x| if x.is_finite() { x.max(0.0) } else { 0.0 });
                let sum: f32 = w.iter().sum();
                for k in j {
                    b.extend(k.to_le_bytes());
                }
                for x in w {
                    let q = if sum > 1e-9 { (x / sum * 65535.0).round() as u16 } else { 0 };
                    b.extend(q.to_le_bytes());
                }
            }
        }
        for i in &self.indices {
            b.extend(i.to_le_bytes());
        }
        b
    }
}

/// A vertex's joints and weights along a chain (see `with_chain_weights`).
pub(super) fn chain_weight(points: &[Vec3], p: Vec3) -> ([u16; 4], [f32; 4]) {
    if points.len() < 2 {
        return RIGID;
    }
    // Where the vertex is along the chain, in joints: i + t on the nearest
    // bone (clamped to the chain, so vertices past an end follow that end's joint).
    let mut best = (f32::MAX, 0.0f32);
    for i in 0..points.len() - 1 {
        let (a, ab) = (points[i], points[i + 1] - points[i]);
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        let d = (a + ab * t).distance(p);
        if d < best.0 - 1e-6 {
            best = (d, i as f32 + t);
        }
    }
    let s = best.1;
    // Around joint k (k ≥ 1) the weight moves from joint k − 1 to joint k
    // over half a bone either side (a smoothstep); the last joint takes
    // everything past it.
    let k = s.round();
    let d = s - k;
    if k < 0.5 {
        return RIGID;
    }
    let k = k as u16;
    if k as usize == points.len() - 1 && d >= 0.0 {
        return ([k, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]);
    }
    let x = (d + 0.5).clamp(0.0, 1.0);
    let w = x * x * (3.0 - 2.0 * x);
    ([k, k - 1, 0, 0], [w, 1.0 - w, 0.0, 0.0])
}

/// Ear clipping for a counter-clockwise simple polygon.
fn triangulate(p: &[[f32; 2]]) -> Vec<[u32; 3]> {
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let inside = |q: [f32; 2], a, b, c| cross(a, b, q) >= 0.0 && cross(b, c, q) >= 0.0 && cross(c, a, q) >= 0.0;
    let mut idx: Vec<usize> = (0..p.len()).collect();
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < p.len() * p.len() {
        guard += 1;
        let n = idx.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            cross(p[a], p[b], p[c]) > 0.0 && !idx.iter().any(|&j| j != a && j != b && j != c && inside(p[j], p[a], p[b], p[c]))
        });
        match ear {
            Some(i) => {
                out.push([idx[(i + n - 1) % n] as u32, idx[i] as u32, idx[(i + 1) % n] as u32]);
                idx.remove(i);
            }
            None => break,
        }
    }
    for i in 1..idx.len().saturating_sub(1) {
        out.push([idx[0] as u32, idx[i] as u32, idx[i + 1] as u32]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::math::{Quat, Transform};

    /// Every triangle's winding agrees with its vertices' normals (front faces point outward).
    fn check_winding(m: &MeshData, name: &str) {
        assert!(!m.indices.is_empty(), "{name}: no triangles");
        assert_eq!(m.positions.len(), m.normals.len());
        for t in m.indices.chunks_exact(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let f = (m.positions[b] - m.positions[a]).cross(m.positions[c] - m.positions[a]);
            if f.length() < 1e-9 {
                continue;
            }
            let n = m.normals[a] + m.normals[b] + m.normals[c];
            assert!(f.dot(n) > 0.0, "{name}: triangle {t:?} winds inward");
        }
    }

    #[test]
    fn builders_wind_outward() {
        check_winding(&MeshData::cube(1.0), "cube");
        check_winding(&MeshData::plane(2.0, 3.0), "plane");
        check_winding(&MeshData::uv_sphere(1.0, 12, 8), "uv_sphere");
        check_winding(&MeshData::icosphere(1.0, 2), "icosphere");
        check_winding(&MeshData::icosphere(1.0, 1).flat_shaded(), "faceted icosphere");
        check_winding(&MeshData::cylinder(0.5, 2.0, 10), "cylinder");
        check_winding(&MeshData::cone(0.5, 2.0, 10), "cone");
        check_winding(&MeshData::torus(1.0, 0.3, 16, 8), "torus");
        check_winding(&MeshData::lathe(&[[0.0, -1.0], [0.6, -0.8], [0.8, 0.0], [0.4, 0.9], [0.0, 1.0]], 12), "lathe");
        // A concave (L-shaped) extrusion, given clockwise to exercise the reversal.
        let l = [[0.0, 0.0], [0.0, 2.0], [1.0, 2.0], [1.0, 1.0], [2.0, 1.0], [2.0, 0.0]];
        check_winding(&MeshData::extrude(&l, 0.5), "extrude");
        let mirror = Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, vec3(-1.0, 1.0, 1.0));
        check_winding(&MeshData::uv_sphere(1.0, 8, 6).transformed(&mirror), "mirrored sphere");
    }

    /// The skill's "meshes in code" example, kept compiling.
    #[test]
    fn skill_rock_example() {
        let mut m = MeshData::icosphere(1.0, 1);
        for (i, p) in m.positions.iter_mut().enumerate() {
            *p = *p * (0.8 + 0.4 * (i as f32 * 12.9898).sin().abs());
        }
        let rock = m.flat_shaded().with_vertex_colors_by(|p, _| if p.y > 0.3 { 0xb8b0a8ff } else { 0x6b625cff });
        check_winding(&rock, "rock");
        assert!(rock.upload().is_some());
    }

    #[test]
    fn icosphere_counts_and_radius() {
        let m = MeshData::icosphere(2.0, 2);
        assert_eq!(m.indices.len() / 3, 20 * 16);
        assert_eq!(m.vertex_count(), 162);
        assert!(m.positions.iter().all(|p| (p.length() - 2.0).abs() < 1e-4));
    }

    #[test]
    fn merge_and_pack() {
        let mut a = MeshData::cube(1.0);
        let b = MeshData::cube(1.0).with_vertex_colors(0x11223344).transformed(&Transform::at(vec3(2.0, 0.0, 0.0)).matrix());
        a.merge(&b);
        assert_eq!(a.vertex_count(), 48);
        assert_eq!(a.colors.len(), 48);
        assert_eq!(a.colors[0], 0xffffffff);
        assert_eq!(*a.indices.iter().max().unwrap(), 47);
        let p = a.pack();
        assert_eq!(&p[0..4], &48u32.to_le_bytes());
        assert_eq!(&p[8..12], &2u32.to_le_bytes());
        assert_eq!(p.len(), 12 + 48 * 36 + a.indices.len() * 4);
        // Colors are stored as bytes r, g, b, a.
        let first_colored = 12 + 24 * 36 + 32;
        assert_eq!(&p[first_colored..first_colored + 4], &[0x11, 0x22, 0x33, 0x44]);
    }

    /// Skinned meshes (mb3d 2) pack joints u16×4 and weights unorm16×4 after
    /// the color, with flag 4; merging keeps skins, rigid parts on joint 0.
    #[test]
    fn skinned_meshes_pack_joints_and_weights() {
        let mut m = MeshData::cube(1.0).with_vertex_colors(0x11223344).with_joint(3);
        m.weights[0] = [3.0, 1.0, 0.0, 0.0];
        m.joints[0] = [3, 5, 0, 0];
        let p = m.pack();
        assert_eq!(&p[8..12], &6u32.to_le_bytes(), "colors and skin");
        assert_eq!(p.len(), 12 + 24 * 52 + 36 * 4);
        let skin = &p[12 + 36..12 + 52];
        let u16s: Vec<u16> = skin.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(u16s, vec![3, 5, 0, 0, 49151, 16384, 0, 0], "weights normalized to unorm16");
        let mut whole = MeshData::cube(1.0);
        whole.merge(&m);
        assert_eq!((whole.joints.len(), whole.joints[0], whole.joints[24]), (48, [0; 4], [3, 5, 0, 0]));
        whole.merge(&MeshData::cube(1.0));
        assert_eq!((whole.weights.len(), whole.weights[47][0], whole.weights[48][0]), (72, 1.0, 1.0));
        assert_eq!(whole.flat_shaded().joints.len(), whole.indices.len());
    }

    /// Mothglass friction #1: merging colored meshes into an empty one keeps their colors.
    #[test]
    fn merging_into_an_empty_mesh_keeps_colors() {
        let mut m = MeshData::new();
        m.merge(&MeshData::cube(1.0).with_vertex_colors(0x11223344));
        m.merge(&MeshData::cube(1.0).with_vertex_colors(0x55667788));
        assert_eq!(m.colors.len(), m.vertex_count());
        assert_eq!((m.colors[0], m.colors[24]), (0x11223344, 0x55667788));
    }
}
