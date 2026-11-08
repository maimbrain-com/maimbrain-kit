//! The ground's heights: FastNoise Lite (MIT, its Rust port), stamps on top,
//! then what `raise` added. Pure: no host calls, so native tests hash and time
//! the same code a phone runs.
//!
//! **Only `+ − × ÷ sqrt abs floor` here**, and FastNoise Lite uses only those,
//! `trunc` and bit tricks: IEEE-exact everywhere. `sin`, `exp` and `powf` come
//! from a different libm natively than in wasm, so they stay out (spike S11's
//! "Determinism", tag spike/s11-terrain-final).

use std::collections::BTreeMap;

use fastnoise_lite::{CellularDistanceFunction, CellularReturnType, DomainWarpType, FastNoiseLite, FractalType, NoiseType};

/// The noise a terrain's heights come from (`noise = { … }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseDesc {
    pub seed: i32,
    /// Layers of detail, each half the size of the last (1–8).
    pub octaves: u32,
    /// About 1 ÷ the width of a hill, in meters (0.004: hills ~250 m apart).
    pub frequency: f32,
    /// 0–1: how tall the mountain ranges are that ring the land's basins
    /// (ridged noise along the borders of cells about 450 m across at the
    /// default frequency; 0 for rolling land everywhere). Their crests
    /// saw-tooth at a distance, where a coarse level samples them.
    pub ridged: f32,
    /// Meters the land is pushed around by (domain warp): bends valleys and
    /// ridges so they don't look like noise. None: `0.12 ÷ frequency`.
    pub warp: Option<f32>,
}

impl Default for NoiseDesc {
    fn default() -> NoiseDesc {
        NoiseDesc { seed: 1, octaves: 6, frequency: 0.002, ridged: 0.6, warp: None }
    }
}

/// Something stamped on the noise, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Stamp {
    /// A smooth bump `height` meters at its middle (negative: a dip), gone at `radius`.
    Hill { x: f32, z: f32, radius: f32, height: f32 },
    /// A bowl `depth` deep with a raised rim `rim` high at `radius`, gone at 1.6 × `radius`.
    Crater { x: f32, z: f32, radius: f32, depth: f32, rim: f32 },
    /// A river bed along the points (x, z, x, z, …): flat at `bed` meters
    /// (None: a little under the lowest land along it) across `width`, back
    /// up to the land over `bank` either side, or wider where the land is
    /// high above the bed (a valley's sides no steeper than about 35°, not
    /// a canyon's walls). It only cuts, never fills.
    River { points: Vec<[f32; 2]>, width: f32, bank: f32, bed: Option<f32> },
}

/// A stamp as heights apply it: bed resolved, its square found.
enum Shaped {
    Hill { x: f32, z: f32, r: f32, h: f32 },
    Crater { x: f32, z: f32, r: f32, depth: f32, rim: f32 },
    River { points: Vec<[f32; 2]>, half: f32, bank: f32, bed: f32, widest: f32, meander: Box<FastNoiseLite> },
}

impl Shaped {
    /// The square it can touch: [x0, z0, x1, z1].
    fn reach(&self) -> [f32; 4] {
        match self {
            Shaped::Hill { x, z, r, .. } => [x - r, z - r, x + r, z + r],
            Shaped::Crater { x, z, r, .. } => [x - 1.6 * r, z - 1.6 * r, x + 1.6 * r, z + 1.6 * r],
            Shaped::River { points, half, widest, .. } => {
                let k = half + widest + meander_amp(*half);
                let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for p in points {
                    b = [b[0].min(p[0] - k), b[1].min(p[1] - k), b[2].max(p[0] + k), b[3].max(p[1] + k)];
                }
                b
            }
        }
    }

    #[inline]
    fn apply(&self, x: f32, z: f32, h: f32) -> f32 {
        match *self {
            Shaped::Hill { x: cx, z: cz, r, h: top } => {
                let d = dist(x - cx, z - cz) / r;
                if d >= 1.0 { h } else { h + top * bump(d) }
            }
            Shaped::Crater { x: cx, z: cz, r, depth, rim } => {
                let d = dist(x - cx, z - cz) / r;
                if d >= 1.6 {
                    h
                } else if d < 1.0 {
                    // The bowl: −depth at the middle, up to the rim at d = 1.
                    h + (d * d) * (depth + rim) - depth
                } else {
                    h + rim * smooth(1.6, 1.0, d)
                }
            }
            Shaped::River { ref points, half, bank, bed, widest, ref meander } => {
                // Meandering: the line wanders up to about its width either
                // side, slowly, so a straight stretch doesn't read as a ruler.
                let d = (to_line(points, x, z) + meander_amp(half) * meander.get_noise_2d(x, z)).max(0.0);
                if d >= half + widest {
                    return h;
                }
                // A bank at least VALLEY m out for each meter of land above
                // the bed, an even slope eased in at the bed and out at the
                // top: with the meander, sides a figure can walk (under
                // about 38°) where the land is within WALKABLE m of the bed.
                // Higher land, a mountain the river cuts past, comes down
                // over a meter more for each meter more (about 45°), so the
                // valley doesn't take the range away.
                let above = h - bed;
                let bank = bank.max(above.min(WALKABLE) * VALLEY + (above - WALKABLE).max(0.0)).min(widest);
                if d >= half + bank {
                    h
                } else {
                    let k = ramp(((d - half) / bank).max(0.0));
                    h.min(bed + (h - bed) * k)
                }
            }
        }
    }
}

/// Meters a river's bank reaches out for each meter of land above its bed.
const VALLEY: f32 = 2.0;
/// How high over a river's bed its banks stay walkable, in meters.
const WALKABLE: f32 = 30.0;

/// 0 to 1 over 0 to 1: even in the middle, eased over a quarter at each end,
/// so its steepest is only a third steeper than its average (a smoothstep's
/// is half again).
fn ramp(u: f32) -> f32 {
    const E: f32 = 0.25;
    let u = u.min(1.0);
    let k = 1.0 / (2.0 * E * (1.0 - E));
    if u < E {
        u * u * k
    } else if u > 1.0 - E {
        1.0 - (1.0 - u) * (1.0 - u) * k
    } else {
        (u - E * 0.5) / (1.0 - E)
    }
}

/// How far a river wanders from its line, either side.
fn meander_amp(half: f32) -> f32 {
    (half * 1.5).max(5.0)
}

#[inline]
fn dist(dx: f32, dz: f32) -> f32 {
    (dx * dx + dz * dz).sqrt()
}

/// The distance from (x, z) to the nearest point of the line through `points`.
fn to_line(points: &[[f32; 2]], x: f32, z: f32) -> f32 {
    let mut best = f32::MAX;
    for w in points.windows(2) {
        let ([ax, az], [bx, bz]) = (w[0], w[1]);
        let (dx, dz) = (bx - ax, bz - az);
        let len2 = dx * dx + dz * dz;
        let t = if len2 > 0.0 { (((x - ax) * dx + (z - az) * dz) / len2).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min(dist(x - (ax + t * dx), z - (az + t * dz)));
    }
    if points.len() == 1 {
        best = dist(x - points[0][0], z - points[0][1]);
    }
    best
}

/// 0 at `a`, 1 at `b`, smooth between (either order).
#[inline]
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 at 0, 0 at 1, flat at both ends.
#[inline]
fn bump(d: f32) -> f32 {
    let k = 1.0 - d * d;
    k * k
}

/// Each octave's share of the last: under FBm's usual 0.5, so slopes stay
/// those of mountains (a median about 20° at the defaults), not of gravel.
const GAIN: f32 = 0.38;

/// How much the low ground is flattened (1 none): 0.2 makes valleys broad and
/// gentle, the mountains rising steeply from them.
const PROFILE: f32 = 0.2;

/// FBm of OpenSimplex2 at `GAIN` mostly stays within ±0.7: this makes it about ±1.
const FBM_SPAN: f32 = 0.71;

/// With ranges, the rolling ground is under half the height and the ranges
/// the rest: this keeps the highest peaks near `height` at `ridged` 0.6.
const RANGES: f32 = 1.05;

struct Noises {
    warp: Option<FastNoiseLite>,
    base: FastNoiseLite,
    /// Ridged FBm, and the cells whose borders the ranges run along.
    ridge: Option<(FastNoiseLite, FastNoiseLite)>,
    ridged: f32,
}

impl Noises {
    fn new(d: &NoiseDesc) -> Noises {
        let f = d.frequency.max(1e-5);
        let warp_amp = d.warp.unwrap_or(0.12 / f);
        let warp = (warp_amp > 0.0).then(|| {
            let mut w = FastNoiseLite::with_seed(d.seed.wrapping_add(11));
            w.set_domain_warp_type(Some(DomainWarpType::OpenSimplex2));
            w.set_domain_warp_amp(Some(warp_amp));
            w.set_frequency(Some(f * 0.8));
            w
        });
        let mut base = FastNoiseLite::with_seed(d.seed);
        base.set_noise_type(Some(NoiseType::OpenSimplex2));
        base.set_frequency(Some(f));
        base.set_fractal_type(Some(FractalType::FBm));
        base.set_fractal_octaves(Some(d.octaves.clamp(1, 8) as i32));
        // Each smaller layer well under half the last (FBm's usual): the land
        // keeps its detail without its slopes coming from the smallest bumps.
        base.set_fractal_gain(Some(GAIN));
        let ridged = d.ridged.clamp(0.0, 1.0);
        let ridge = (ridged > 0.0).then(|| {
            let mut r = FastNoiseLite::with_seed(d.seed.wrapping_add(23));
            r.set_noise_type(Some(NoiseType::OpenSimplex2));
            r.set_frequency(Some(f * 1.2));
            r.set_fractal_type(Some(FractalType::Ridged));
            r.set_fractal_octaves(Some(d.octaves.clamp(1, 4) as i32));
            r.set_fractal_gain(Some(GAIN));
            // Basins about 450 m across (at the defaults), ranges along
            // their borders: F2 − F1 of cellular noise is 0 on a border.
            let mut m = FastNoiseLite::with_seed(d.seed.wrapping_add(37));
            m.set_noise_type(Some(NoiseType::Cellular));
            m.set_cellular_distance_function(Some(CellularDistanceFunction::Euclidean));
            m.set_cellular_return_type(Some(CellularReturnType::Distance2Sub));
            m.set_frequency(Some(f * 1.1));
            (r, m)
        });
        Noises { warp, base, ridge, ridged }
    }

    /// 0 (lowest) to about 1 (highest).
    #[inline]
    fn sample(&self, x: f32, z: f32) -> f32 {
        let (wx, wz) = match &self.warp {
            Some(w) => w.domain_warp_2d(x, z),
            None => (x, z),
        };
        let base = 0.5 + 0.5 * (self.base.get_noise_2d(wx, wz) / FBM_SPAN);
        // The rolling ground: broad low valleys, the higher ground steeper.
        let rolling = base * (PROFILE + (1.0 - PROFILE) * base);
        match &self.ridge {
            // Basins and ranges: the land rolls inside a cell of the
            // cellular noise, and a range of sharp ridged peaks runs along
            // its border, rising out of foothills, so from anywhere in a
            // basin the ranges stand on the horizon a few hundred meters off
            // and the ground underfoot stays gentle. A basin is kept around
            // (0, 0), where a game most often starts. All of it a function of
            // where on the land, never of where the camera is.
            Some((r, cells)) => {
                let edge = cells.get_noise_2d(wx, wz) + 1.0;
                let from_origin = (x * x + z * z).sqrt();
                let k = self.ridged * (1.0 - smooth(0.04, 0.6, edge)) * smooth(60.0, 180.0, from_origin);
                let range = if k > 0.0 {
                    let crest = 0.5 + 0.5 * r.get_noise_2d(wx, wz);
                    k * (0.55 + 0.75 * crest * crest) * (0.7 + 0.3 * base)
                } else {
                    0.0
                };
                (0.45 * rolling + range) / RANGES
            }
            None => rolling,
        }
    }
}

/// Meters a `raise` grid cell is: the terrain's `detail`, so level 0's
/// vertices are exactly its points.
#[derive(Clone, Debug, Default)]
struct Raised {
    step: f32,
    /// Offsets by level-0 chunk (32 × 32 cells, 33 × 33 points shared at the edges).
    chunks: BTreeMap<(i32, i32), Box<[f32]>>,
}

const SIDE: i32 = 32;

impl Raised {
    fn point(&self, i: i32, j: i32) -> f32 {
        let (ci, cj) = (i.div_euclid(SIDE), j.div_euclid(SIDE));
        match self.chunks.get(&(ci, cj)) {
            Some(c) => c[(j.rem_euclid(SIDE) * SIDE + i.rem_euclid(SIDE)) as usize],
            None => 0.0,
        }
    }

    fn add(&mut self, i: i32, j: i32, v: f32) {
        let (ci, cj) = (i.div_euclid(SIDE), j.div_euclid(SIDE));
        let c = self.chunks.entry((ci, cj)).or_insert_with(|| vec![0.0; (SIDE * SIDE) as usize].into_boxed_slice());
        c[(j.rem_euclid(SIDE) * SIDE + i.rem_euclid(SIDE)) as usize] += v;
    }

    /// Bilinear between the grid's points (exact at them).
    fn at(&self, x: f32, z: f32) -> f32 {
        if self.chunks.is_empty() {
            return 0.0;
        }
        let (gx, gz) = (x / self.step, z / self.step);
        let (i, j) = (gx.floor(), gz.floor());
        let (fx, fz) = (gx - i, gz - j);
        let (i, j) = (i as i32, j as i32);
        let a = self.point(i, j) + (self.point(i + 1, j) - self.point(i, j)) * fx;
        let b = self.point(i, j + 1) + (self.point(i + 1, j + 1) - self.point(i, j + 1)) * fx;
        a + (b - a) * fz
    }
}

/// A terrain's heights: what [`Heights::height`] says is the true ground,
/// the same for every player whatever is drawn.
pub struct Heights {
    noise: Option<Noises>,
    height: f32,
    stamps: Vec<(Shaped, [f32; 4])>,
    raised: Raised,
    /// A world with an edge: the half side of its square, around (0, 0).
    half: Option<f32>,
}

impl Heights {
    /// `noise` None is flat ground at 0 (stamps only); `height` is meters
    /// from the noise's lowest to its highest; `detail` the spacing `raise`
    /// works at; `size` a square world's side, around (0, 0).
    pub fn new(noise: Option<&NoiseDesc>, height: f32, stamps: &[Stamp], detail: f32, size: Option<f32>) -> Heights {
        let mut h = Heights { noise: noise.map(Noises::new), height, stamps: Vec::new(), raised: Raised { step: detail.max(0.01), ..Default::default() }, half: size.map(|s| s * 0.5) };
        for s in stamps {
            let shaped = match s {
                Stamp::Hill { x, z, radius, height } => Shaped::Hill { x: *x, z: *z, r: radius.max(0.01), h: *height },
                Stamp::Crater { x, z, radius, depth, rim } => Shaped::Crater { x: *x, z: *z, r: radius.max(0.01), depth: *depth, rim: *rim },
                Stamp::River { points, width, bank, bed } => {
                    let half = (width * 0.5).max(0.01);
                    let bed = bed.unwrap_or_else(|| h.lowest_along(points, half) - 1.5);
                    // As wide as the bank can get: the land's whole span above the bed.
                    let widest = bank.max((h.height.abs() + 40.0 - bed.min(0.0)) * VALLEY);
                    let mut meander = FastNoiseLite::with_seed(points.len() as i32 * 7919 + (points[0][0] as i32).wrapping_mul(31) + points[0][1] as i32);
                    meander.set_noise_type(Some(NoiseType::OpenSimplex2));
                    meander.set_frequency(Some(1.0 / 150.0));
                    Shaped::River { points: points.clone(), half, bank: bank.max(0.0), bed, widest, meander: Box::new(meander) }
                }
            };
            let r = shaped.reach();
            h.stamps.push((shaped, r));
        }
        h
    }

    /// The lowest the ground is along a line so far (the stamps before it),
    /// sampled every `step` meters: a river's default bed.
    fn lowest_along(&self, points: &[[f32; 2]], step: f32) -> f32 {
        let mut low = f32::MAX;
        for w in points.windows(2).chain(std::iter::once(&points[..points.len().min(1)])) {
            let (a, b) = match w {
                [a, b] => (*a, *b),
                [a] => (*a, *a),
                _ => continue,
            };
            let n = ((dist(b[0] - a[0], b[1] - a[1]) / step.max(1.0)) as u32).clamp(1, 4096);
            for k in 0..=n {
                let t = k as f32 / n as f32;
                low = low.min(self.height(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t));
            }
        }
        if low == f32::MAX { 0.0 } else { low }
    }

    /// A point clamped into the world's square (a world with a `size`).
    #[inline]
    pub fn clamp(&self, x: f32, z: f32) -> (f32, f32) {
        match self.half {
            Some(s) => (x.clamp(-s, s), z.clamp(-s, s)),
            None => (x, z),
        }
    }

    /// The noise alone at one point.
    #[inline]
    fn noise(&self, x: f32, z: f32) -> f32 {
        match &self.noise {
            Some(n) => self.height * n.sample(x, z),
            None => 0.0,
        }
    }

    /// The ground's height at (x, z): noise, every stamp, and what `raise` added.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let (x, z) = self.clamp(x, z);
        let mut h = self.noise(x, z);
        for (s, _) in &self.stamps {
            h = s.apply(x, z, h);
        }
        h + self.raised.at(x, z)
    }

    /// The ground's normal at (x, z), from heights `e` meters either side.
    pub fn normal(&self, x: f32, z: f32, e: f32) -> [f32; 3] {
        let dx = (self.height(x + e, z) - self.height(x - e, z)) / (2.0 * e);
        let dz = (self.height(x, z + e) - self.height(x, z - e)) / (2.0 * e);
        let inv = 1.0 / (dx * dx + dz * dz + 1.0).sqrt();
        [-dx * inv, inv, -dz * inv]
    }

    /// `n × n` heights from (x0, z0), `step` apart, row by row (z, then x),
    /// with only the stamps that can reach the square.
    pub fn fill(&self, x0: f32, z0: f32, step: f32, n: usize, out: &mut Vec<f32>) {
        let far = step * (n as f32 - 1.0);
        let near: Vec<&Shaped> = self.stamps.iter().filter(|(_, r)| r[0] <= x0 + far && r[2] >= x0 && r[1] <= z0 + far && r[3] >= z0).map(|(s, _)| s).collect();
        let raised = !self.raised.chunks.is_empty();
        out.clear();
        out.reserve(n * n);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = self.clamp(x0 + i as f32 * step, z0 + j as f32 * step);
                let mut h = self.noise(x, z);
                for s in &near {
                    h = s.apply(x, z, h);
                }
                if raised {
                    h += self.raised.at(x, z);
                }
                out.push(h);
            }
        }
    }

    /// Adds a smooth bump `amount` meters high (negative digs) out to
    /// `radius`, on the grid `raise` keeps. Returns the square it changed.
    pub fn raise(&mut self, x: f32, z: f32, radius: f32, amount: f32) -> [f32; 4] {
        let step = self.raised.step;
        let r = radius.max(step * 1.5);
        let (i0, i1) = (((x - r) / step).floor() as i32, ((x + r) / step).floor() as i32 + 1);
        let (j0, j1) = (((z - r) / step).floor() as i32, ((z + r) / step).floor() as i32 + 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let d = dist(i as f32 * step - x, j as f32 * step - z) / r;
                if d < 1.0 {
                    self.raised.add(i, j, amount * bump(d));
                }
            }
        }
        [x - r - step, z - r - step, x + r + step, z + r + step]
    }

    /// Takes back every `raise`. Returns the squares they had changed.
    pub fn unraise(&mut self) -> Vec<[f32; 4]> {
        let side = SIDE as f32 * self.raised.step;
        let out = self.raised.chunks.keys().map(|(i, j)| [*i as f32 * side, *j as f32 * side, (*i + 1) as f32 * side, (*j + 1) as f32 * side]).collect();
        self.raised.chunks.clear();
        out
    }

    pub fn raised(&self) -> bool {
        !self.raised.chunks.is_empty()
    }

    /// The half side of a world with an edge.
    pub fn half(&self) -> Option<f32> {
        self.half
    }

    /// Meters from the noise's lowest to its highest.
    pub fn span(&self) -> f32 {
        self.height
    }
}

/// FNV-1a over 32-bit words: the determinism checks' hash.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(pub u32);

impl Fnv {
    pub const START: Fnv = Fnv(0x811c_9dc5);
    #[inline]
    pub fn word(&mut self, w: u32) {
        for b in w.to_le_bytes() {
            self.0 ^= b as u32;
            self.0 = self.0.wrapping_mul(0x0100_0193);
        }
    }
}
