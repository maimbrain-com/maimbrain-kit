//! One piece of ground as a mesh, and which pieces a camera wants (spike
//! S11's "tree" layout). Pure, but for `MeshData`, which is plain data:
//! native tests run all of it.
//!
//! Every tile has 32 × 32 quads; tiles at levels 1 and 2 cover 2× and 4× the
//! ground of a level-0 tile, so the far ground costs a quarter and a
//! sixteenth of the draws. A level-2 tile splits into four level-1 tiles
//! when its nearest corner is within `64 × detail` meters of the camera, a
//! level-1 tile into four level-0 ones within `32 × detail`.

use crate::gfx3d::{MeshData, Vec3, vec3};

use super::ground::Surface;
use super::heights::{Fnv, Heights};

/// Quads a side, every tile.
pub const QUADS: u32 = 32;
/// Levels of detail: always three.
pub const LEVELS: u8 = 3;

/// A tile: its level, and its place in a grid of tiles at that level's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub lod: u8,
    pub ix: i32,
    pub iz: i32,
}

/// How a terrain is cut into tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Meters between vertices at level 0 (`detail`).
    pub detail: f32,
    /// Meters drawn around the camera.
    pub reach: f32,
    /// A world with an edge: the half side of its square, around (0, 0).
    pub half: Option<f32>,
}

impl Layout {
    pub fn new(detail: f32, reach: f32, half: Option<f32>) -> Layout {
        Layout { detail, reach, half }
    }
    /// Meters a level-0 tile covers.
    pub fn chunk(&self) -> f32 {
        self.detail * QUADS as f32
    }
    /// Meters between vertices at a level.
    pub fn step(&self, lod: u8) -> f32 {
        self.detail * (1u32 << lod) as f32
    }
    /// Meters a tile covers at a level.
    pub fn size(&self, lod: u8) -> f32 {
        self.chunk() * (1u32 << lod) as f32
    }
    /// Where level 0 ends and where level 1 ends (a tile's nearest corner).
    pub fn split(&self, lod: u8) -> f32 {
        self.detail * if lod == 2 { 64.0 } else { 32.0 }
    }
    /// A tile's corner (x0, z0) and size.
    pub fn rect(&self, k: Key) -> (f32, f32, f32) {
        let s = self.size(k.lod);
        (k.ix as f32 * s, k.iz as f32 * s, s)
    }
    /// Vertices of every tile, skirts included.
    pub const fn verts() -> u32 {
        (QUADS + 1) * (QUADS + 1) + 4 * (QUADS + 1)
    }
    /// Triangles of every tile, skirts included.
    pub const fn tris() -> u32 {
        2 * QUADS * QUADS + 8 * QUADS
    }
    /// Bytes a tile takes in GPU buffers: mb-host's 52-byte vertex and u32 indices.
    pub const fn gpu_bytes() -> u32 {
        Layout::verts() * 52 + Layout::tris() * 12
    }
    /// How far a tile's skirt hangs. A tile can meet one two levels coarser,
    /// whose surface cuts the corners by up to about slope × its step.
    pub fn skirt(&self, lod: u8) -> f32 {
        (self.step(lod) * 3.0).max(self.step(2) * 1.5)
    }

    /// Samples around a tile past its edge: enough for its normals, and for
    /// the slope it paints by, which is measured over level 2's spacing at
    /// every level so a tile's colours don't change when its level does.
    fn ring(&self, lod: u8) -> usize {
        1 << (LEVELS - 1 - lod)
    }

    /// The tile at `lod` holding the point.
    fn tile_at(&self, lod: u8, x: f32, z: f32) -> Key {
        let s = self.size(lod);
        Key { lod, ix: (x / s).floor() as i32, iz: (z / s).floor() as i32 }
    }

    /// The nearest a point of the tile comes to (cx, cz).
    pub fn distance(&self, k: Key, cx: f32, cz: f32) -> f32 {
        let (x0, z0, s) = self.rect(k);
        let dx = (x0 - cx).max(cx - (x0 + s)).max(0.0);
        let dz = (z0 - cz).max(cz - (z0 + s)).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }

    /// Whether any of the tile is inside a world with an edge.
    fn inside(&self, k: Key) -> bool {
        let Some(h) = self.half else { return true };
        let (x0, z0, s) = self.rect(k);
        x0 < h && x0 + s > -h && z0 < h && z0 + s > -h
    }

    /// Every tile wanted from (cx, cz), with its distance, in key order.
    pub fn select(&self, cx: f32, cz: f32, out: &mut Vec<(Key, f32)>) {
        out.clear();
        let s = self.size(LEVELS - 1);
        let (i0, k0) = (((cx - self.reach) / s).floor() as i32, ((cz - self.reach) / s).floor() as i32);
        let (i1, k1) = (((cx + self.reach) / s).floor() as i32, ((cz + self.reach) / s).floor() as i32);
        for iz in k0..=k1 {
            for ix in i0..=i1 {
                self.walk(Key { lod: LEVELS - 1, ix, iz }, cx, cz, out);
            }
        }
        out.sort_by_key(|e| e.0);
    }

    fn walk(&self, k: Key, cx: f32, cz: f32, out: &mut Vec<(Key, f32)>) {
        let d = self.distance(k, cx, cz);
        if d >= self.reach || !self.inside(k) {
            return;
        }
        if k.lod == 0 || d >= self.split(k.lod) {
            out.push((k, d));
            return;
        }
        for (a, b) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            self.walk(Key { lod: k.lod - 1, ix: k.ix * 2 + a, iz: k.iz * 2 + b }, cx, cz, out);
        }
    }

    /// The tiles of `wanted` (sorted by key) that overlap tile `k`, and
    /// whether they cover all of it.
    pub fn cover(&self, k: Key, wanted: &[(Key, f32)], out: &mut Vec<Key>) -> bool {
        out.clear();
        let (x0, z0, s) = self.rect(k);
        let unit = self.size(0);
        let cells = (s / unit + 0.5) as i32;
        let mut full = true;
        for b in 0..cells {
            for a in 0..cells {
                let (x, z) = (x0 + (a as f32 + 0.5) * unit, z0 + (b as f32 + 0.5) * unit);
                let mut hit = None;
                for lod in 0..LEVELS {
                    let t = self.tile_at(lod, x, z);
                    if wanted.binary_search_by_key(&t, |e| e.0).is_ok() {
                        hit = Some(t);
                        break;
                    }
                }
                match hit {
                    Some(t) if !out.contains(&t) => out.push(t),
                    Some(_) => {}
                    None => full = false,
                }
            }
        }
        full
    }
}

/// What a terrain paints its ground with (`paint = { … }`), in order: the
/// first is the base, each later one blends in where its conditions hold.
/// Each names a built-in surface, a colour, or both (the surface recoloured):
/// [`super::ground::surfaces`] turns them into the ground shader's rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintRule {
    /// `0xRRGGBB`, as picked (sRGB).
    pub color: Option<u32>,
    pub surface: Option<Surface>,
    /// Where the ground is higher than this many meters.
    pub above: Option<f32>,
    /// Where it's lower.
    pub below: Option<f32>,
    /// Where it's steeper than this: `1 − normal.y`, so flat is 0, 30° is
    /// 0.13, 45° is 0.29 and 60° is 0.5.
    pub slope: Option<f32>,
    /// Meters (or, for `slope` alone, slope) the edge blends over. None: 3%
    /// of the terrain's height for heights, 0.04 for slopes.
    pub blend: Option<f32>,
}

impl PaintRule {
    pub fn color(c: u32) -> PaintRule {
        PaintRule { color: Some(c), surface: None, above: None, below: None, slope: None, blend: None }
    }
    pub fn surface(s: Surface) -> PaintRule {
        PaintRule { color: None, surface: Some(s), above: None, below: None, slope: None, blend: None }
    }
}

/// Where a vertex is: its place, its height, its slope (`1 − normal.y`) and
/// its hollowness (how far the ground around it, at level 2's spacing, is
/// above it ÷ that spacing: + in a gully or a valley's floor, − on a
/// crest). The slope and hollowness are measured the same way at every
/// level, so nothing changes when a tile changes level; they go to the
/// ground shader in the vertex's UV.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spot {
    pub x: f32,
    pub z: f32,
    pub y: f32,
    pub slope: f32,
    pub hollow: f32,
}

/// The vertex colour: a broad tint over the ground shader's surfaces
/// (sRGB 188 is none). The natural ground's makes the low ground's grass
/// several greens (lush, deep and yellower, in patches about 40 m and 14 m
/// across, fading out up high and on steep ground, where rock and snow
/// take no green), varies the light a little with a broad noise, and
/// lightens crests and shades hollows; a game's `paint` gets none.
pub enum Tint {
    None,
    Natural(Natural),
}

/// sRGB 188: 0.5 in linear light, which the ground shader doubles to 1.
pub const NEUTRAL: f32 = 188.0;

impl Tint {
    /// The natural ground's tint, for a terrain `height` meters from lowest to highest.
    pub fn natural(seed: i32, height: f32) -> Tint {
        Tint::Natural(Natural::new(seed, height))
    }

    /// A vertex's colour, `0xRRGGBBAA`.
    pub fn tint(&self, at: &Spot) -> u32 {
        let c = match self {
            Tint::None => [NEUTRAL; 3],
            Tint::Natural(n) => n.tint(at),
        };
        let b = |v: f32| (v + 0.5).clamp(0.0, 255.0) as u32;
        (b(c[0]) << 24) | (b(c[1]) << 16) | (b(c[2]) << 8) | 255
    }
}

/// The natural ground's tint: greens that vary across a meadow, a little
/// lighter or darker in broad patches, lighter on crests and darker in hollows.
pub struct Natural {
    height: f32,
    broad: fastnoise_lite::FastNoiseLite,
    patch: fastnoise_lite::FastNoiseLite,
}

impl Natural {
    pub fn new(seed: i32, height: f32) -> Natural {
        let noise = |s: i32, f: f32| {
            let mut n = fastnoise_lite::FastNoiseLite::with_seed(s);
            n.set_noise_type(Some(fastnoise_lite::NoiseType::OpenSimplex2));
            n.set_frequency(Some(f));
            n
        };
        Natural { height: height.abs().max(1.0), broad: noise(seed.wrapping_add(101), 1.0 / 180.0), patch: noise(seed.wrapping_add(211), 1.0 / 40.0) }
    }

    fn tint(&self, at: &Spot) -> [f32; 3] {
        let broad = self.broad.get_noise_2d(at.x, at.z);
        let patch = self.patch.get_noise_2d(at.x, at.z);
        let tuft = self.patch.get_noise_2d(at.x * 2.9 + 31.0, at.z * 2.9);
        // Greens: lush (bluer, brighter green), deep (darker) and yellower,
        // only on the low, gentle ground the grass is on.
        let low = (1.0 - super::heights::smooth(0.42, 0.58, at.y / self.height)) * (1.0 - super::heights::smooth(0.18, 0.28, at.slope));
        let green = (0.6 * patch + 0.4 * tuft) * low;
        let deep = super::heights::smooth(0.2, 0.8, -patch * 0.7 + 0.3 * tuft) * low;
        let light = (1.0 - 0.03 * broad - 0.05 * deep) * (1.0 - (at.hollow * 0.5).clamp(-0.05, 0.06));
        [NEUTRAL * light * (1.0 - 0.1 * green), NEUTRAL * light * (1.0 + 0.06 * green), NEUTRAL * light * (1.0 - 0.06 * green)]
    }
}

/// Scratch space a builder keeps between tiles.
#[derive(Default)]
pub struct Scratch {
    heights: Vec<f32>,
}

/// The heights a tile needs: its own and a ring around it.
pub fn heights(world: &Heights, l: &Layout, k: Key, sc: &mut Scratch) -> usize {
    let (x0, z0, _) = l.rect(k);
    let step = l.step(k.lod);
    let ring = l.ring(k.lod);
    let n = QUADS as usize + 1 + 2 * ring;
    world.fill(x0 - ring as f32 * step, z0 - ring as f32 * step, step, n, &mut sc.heights);
    n * n
}

/// A tile's mesh, in the tile's own space: its middle at x, z = 0 (heights
/// stay the world's). Each vertex's UV is the slope and hollowness the
/// ground shader's rules read; its colour, the tint.
pub fn mesh(world: &Heights, l: &Layout, k: Key, tint: &Tint, sc: &mut Scratch) -> MeshData {
    heights(world, l, k, sc);
    mesh_from_heights(world, l, k, tint, sc)
}

pub fn mesh_from_heights(world: &Heights, l: &Layout, k: Key, tint: &Tint, sc: &Scratch) -> MeshData {
    let (x0, z0, size) = l.rect(k);
    let step = l.step(k.lod);
    let n = QUADS as usize;
    let ring = l.ring(k.lod);
    let row = n + 1 + 2 * ring;
    let h = &sc.heights;
    // Across a tile's own edge too: `i` and `j` may be up to `ring` outside it.
    let at = |i: usize, j: usize| h[j.wrapping_add(ring) * row + i.wrapping_add(ring)];
    let (cx, cz) = (x0 + size * 0.5, z0 + size * 0.5);
    let w = n + 1;
    let mut m = MeshData::new();
    let total = w * w + 4 * w;
    m.positions.reserve_exact(total);
    m.normals.reserve_exact(total);
    m.uvs.reserve_exact(total);
    m.colors.reserve_exact(total);
    m.indices.reserve_exact(6 * n * n + 24 * n);
    let span = ring as f32 * step * 2.0;
    let reach = ring as f32 * step;
    for j in 0..w {
        for i in 0..w {
            let (x, z) = world.clamp(x0 + i as f32 * step, z0 + j as f32 * step);
            let y = at(i, j);
            // Central differences on this tile's own spacing (the ring of
            // samples outside it makes borders agree with a same-level neighbour).
            let dx = (at(i + 1, j) - at(i.wrapping_sub(1), j)) / (2.0 * step);
            let dz = (at(i, j + 1) - at(i, j.wrapping_sub(1))) / (2.0 * step);
            let inv = 1.0 / (dx * dx + dz * dz + 1.0).sqrt();
            m.positions.push(vec3(x - cx, y, z - cz));
            m.normals.push(vec3(-dx * inv, inv, -dz * inv));
            // The slope the ground's rules read, over level 2's spacing whatever its level.
            let px = (at(i + ring, j) - at(i.wrapping_sub(ring), j)) / span;
            let pz = (at(i, j + ring) - at(i, j.wrapping_sub(ring))) / span;
            let slope = 1.0 - 1.0 / (px * px + pz * pz + 1.0).sqrt();
            let around = (at(i + ring, j) + at(i.wrapping_sub(ring), j) + at(i, j + ring) + at(i, j.wrapping_sub(ring))) * 0.25;
            let hollow = (around - y) / reach;
            m.uvs.push([slope, hollow]);
            m.colors.push(tint.tint(&Spot { x, z, y, slope, hollow }));
        }
    }
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let (a, b) = (j * w as u32 + i, (j + 1) * w as u32 + i);
            m.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let drop = l.skirt(k.lod);
    let (w32, n32) = (w as u32, n as u32);
    // Each edge's vertices, and whether k→k+1 winds outward as is.
    skirt(&mut m, drop, 0..w32, true); // z = z0 (faces −z)
    skirt(&mut m, drop, (0..w32).map(|i| n32 * w32 + i), false); // +z
    skirt(&mut m, drop, (0..w32).map(|j| j * w32), false); // x = x0 (−x)
    skirt(&mut m, drop, (0..w32).map(|j| j * w32 + n32), true); // +x
    m
}

/// A curtain hanging `drop` meters under an edge, so where a tile meets a
/// coarser one no sky shows through the gap. It takes the edge's normal and
/// colour, so it doesn't show as a wall.
fn skirt(m: &mut MeshData, drop: f32, edge: impl Iterator<Item = u32>, forward: bool) {
    let mut last: Option<(u32, u32)> = None;
    for top in edge {
        let p = m.positions[top as usize];
        let (normal, uv, color) = (m.normals[top as usize], m.uvs[top as usize], m.colors[top as usize]);
        let bottom = m.positions.len() as u32;
        m.positions.push(p - Vec3::Y * drop);
        m.normals.push(normal);
        m.uvs.push(uv);
        m.colors.push(color);
        if let Some((t0, b0)) = last {
            if forward {
                m.indices.extend([t0, top, b0, top, bottom, b0]);
            } else {
                m.indices.extend([top, t0, b0, top, b0, bottom]);
            }
        }
        last = Some((top, bottom));
    }
}

/// A hash of everything a tile uploads: positions, normals, colours and indices.
pub fn mesh_hash(m: &MeshData) -> u32 {
    let mut f = Fnv::START;
    for (p, n) in m.positions.iter().zip(&m.normals) {
        for v in [p.x, p.y, p.z, n.x, n.y, n.z] {
            f.word(v.to_bits());
        }
    }
    for uv in &m.uvs {
        f.word(uv[0].to_bits());
        f.word(uv[1].to_bits());
    }
    for c in &m.colors {
        f.word(*c);
    }
    for i in &m.indices {
        f.word(*i);
    }
    f.0
}

/// What a mesh's bounding sphere is on the host (mb-host's `bounds`): the
/// middle of its box, and the farthest vertex from that.
pub fn bounds(m: &MeshData) -> (Vec3, f32) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &m.positions {
        lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let c = (lo + hi) * 0.5;
    let mut r = 0.0f32;
    for p in &m.positions {
        r = r.max((*p - c).length());
    }
    (c, r)
}
