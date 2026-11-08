//! Terrain: an outdoor world on a heightmap (docs/STUDIO-2.md §4.14, §9.3
//! row 2c), as spike S11 measured it (docs/spike/s11-terrain.md on the
//! `spike/s11-terrain-final` tag).
//!
//! - **Heights** from FastNoise Lite (domain warp, FBm, ridged ranges), then
//!   stamps (`hill`, `crater`, `river`), then what [`Terrain::raise`] added.
//!   Only `+ − × ÷ sqrt abs floor`, so a seed is the same land on every
//!   device ([`heights`]).
//! - **Tiles** of 32 × 32 quads, always three levels, the far ones covering
//!   2× and 4× the ground; skirts close the cracks where levels meet
//!   ([`tile`]). `reach` is meters drawn around the camera, `detail` meters
//!   between vertices near it. About 30 draws and 70k triangles at
//!   `reach ÷ detail` = 400, as a phone held sideways sees it.
//! - **Building** is budgeted in vertices, not milliseconds (a game has no
//!   clock): several tiles a frame until the first view is [`built`], then
//!   one a frame, nearest first, faster again only while a hole shows (after
//!   a jump). Old tiles go only when what replaces them is built, and new
//!   ones show only when nothing old overlaps them: no holes, no overlaps.
//! - **The ground** is photographed surfaces (grass, dry meadow, dirt, rock,
//!   snow), blended per pixel by mb-host's ground shader (mb3d 4, [`ground`];
//!   docs/STUDIO-2.md Decided 70) by height, slope and hollowness, with
//!   noise-broken edges; rock laid from the side on cliffs; each surface's
//!   detail fading to its mean colour with distance, so the far ground stays
//!   crisp. Each vertex carries the slope and hollowness its rules read (in
//!   its UV, measured at level 2's spacing at every level, so nothing moves
//!   when a tile changes level) and a broad tint (its colour).
//!
//! ```ignore
//! let mut t = Terrain::new(TerrainDesc::default());
//! // every frame, after the camera moved:
//! t.update(camera.pos);
//! let y = t.height_at(x, z);
//! ```
//!
//! [`built`]: Terrain::built

pub mod ground;
pub mod heights;
pub mod tile;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use super::{Camera, Material, MaterialDesc, Mesh, Node, Quat, Transform, Vec3, vec3};

pub use ground::{Surface, SurfaceImage};
pub use heights::{Heights, NoiseDesc, Stamp};
pub use tile::{Key, LEVELS, Layout, PaintRule, QUADS, Spot};
use tile::{Scratch, Tint};

/// The `detail`s a terrain takes (meters between vertices near the camera).
pub const DETAILS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];
/// `reach ÷ detail` past this is over the phone's triangles on its own.
pub const MAX_RATIO: f32 = 800.0;
/// Past this, a terrain takes much of the frame's budget (a warning).
pub const WARN_RATIO: f32 = 400.0;
/// Vertices a frame until the first view is built (or while a hole shows):
/// eight tiles, about 4 ms on an A15 (S11: 0.27 ms a tile on an iPhone 18 Pro).
pub const FAST_VERTS: u32 = 8 * Layout::verts();

/// About how many triangles a terrain draws, at the most, as a phone held
/// sideways sees it while driving: S11's counts (`count_sweep`) by
/// `reach ÷ detail`, between them in a straight line.
pub fn triangles_at(ratio: f32) -> f32 {
    const AT: [(f32, f32); 11] =
        [(150.0, 43.8), (200.0, 48.4), (300.0, 55.3), (400.0, 69.1), (500.0, 85.2), (600.0, 103.7), (800.0, 140.5), (1000.0, 193.5), (1200.0, 253.4), (1600.0, 394.0), (2000.0, 578.3)];
    let k = match AT.iter().position(|p| p.0 >= ratio) {
        Some(0) => AT[0].1 * ratio / AT[0].0,
        Some(i) => {
            let (a, b) = (AT[i - 1], AT[i]);
            a.1 + (b.1 - a.1) * (ratio - a.0) / (b.0 - a.0)
        }
        // Past the sweep: it grows with the area drawn.
        None => AT[10].1 * (ratio / AT[10].0) * (ratio / AT[10].0),
    };
    k * 1000.0
}

/// What a terrain is.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainDesc {
    /// None: flat at 0, stamps only.
    pub noise: Option<NoiseDesc>,
    /// Meters from the noise's lowest to its highest.
    pub height: f32,
    pub stamps: Vec<Stamp>,
    /// Empty: the natural ground ([`ground`]) for `height`.
    pub paint: Vec<PaintRule>,
    /// Meters drawn around the camera.
    pub reach: f32,
    /// Meters between vertices near the camera: one of [`DETAILS`].
    pub detail: f32,
    /// A square world this many meters a side, around (0, 0); None: endless.
    pub size: Option<f32>,
    /// The built-in surfaces' images, by [`Surface`]; a surface without
    /// one is its mean colour, flat.
    pub surfaces: [Option<SurfaceImage>; 5],
    /// The game's own image for `paint` rules that name only a colour.
    pub texture: Option<SurfaceImage>,
}

impl Default for TerrainDesc {
    fn default() -> TerrainDesc {
        TerrainDesc { noise: Some(NoiseDesc::default()), height: 150.0, stamps: Vec::new(), paint: Vec::new(), reach: 400.0, detail: 1.0, size: None, surfaces: [None; 5], texture: None }
    }
}

impl TerrainDesc {
    /// Why this desc can't be built, in a creator's words, or None.
    pub fn check(&self) -> Option<String> {
        if !DETAILS.contains(&self.detail) {
            return Some(format!("detail is 0.5, 1, 2 or 4 (meters between the ground's points near the camera), got {}", self.detail));
        }
        if !(self.reach.is_finite() && self.reach >= 32.0) {
            return Some(format!("reach is meters of ground drawn around the camera, 32 or more, got {}", self.reach));
        }
        let ratio = self.reach / self.detail;
        if ratio > MAX_RATIO + 0.5 {
            return Some(too_far(self.reach, self.detail));
        }
        if let Some(s) = self.size
            && !(s.is_finite() && s >= self.detail * 2.0)
        {
            return Some(format!("size is the world's side in meters (none for an endless world), got {s}"));
        }
        if !self.height.is_finite() {
            return Some("height is meters from the lowest ground to the highest".into());
        }
        None
    }
}

/// `reach ÷ detail` over 800, with numbers: what it would draw and what would fit.
pub fn too_far(reach: f32, detail: f32) -> String {
    let k = (triangles_at(reach / detail) / 10_000.0).round() * 10.0;
    let fit = DETAILS.iter().find(|d| reach / **d <= MAX_RATIO);
    let either = match fit {
        Some(d) => format!("use detail = {} or reach = {}", fmt(*d), fmt(MAX_RATIO * detail)),
        None => format!("use reach = {} (with detail = 4)", fmt(MAX_RATIO * 4.0)),
    };
    format!("reach {} at detail {} is about {}k triangles of the phone's 150k; {either}", fmt(reach), fmt(detail), k)
}

fn fmt(x: f32) -> String {
    format!("{x}")
}

/// A tile on the host.
struct Tile {
    node: Node,
    mesh: Mesh,
    /// World bounding sphere, as the host sees it.
    center: Vec3,
    radius: f32,
    shown: bool,
}

/// What the camera sees this frame, counted as the host's culling would.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    pub draws: u32,
    pub tris: u32,
    /// Draws per level.
    pub by_lod: [u32; 3],
}

/// A terrain in mb3d: its heights, and the tiles of it alive on the host.
pub struct Terrain {
    pub desc: TerrainDesc,
    heights: Heights,
    layout: Layout,
    tint: Tint,
    /// Every level's: the ground shader's surfaces.
    material: Option<Material>,
    /// Every tile is under it: hides them all, and never fades them.
    root: Option<Node>,
    live: BTreeMap<Key, Tile>,
    wanted: Vec<(Key, f32)>,
    /// To build, farthest first (built from the end).
    queue: Vec<(f32, Key)>,
    /// Live tiles `raise` changed, to build again.
    dirty: Vec<Key>,
    spare: Vec<Node>,
    scratch: Scratch,
    cover: Vec<Key>,
    last_select: Option<(f32, f32)>,
    /// What's wanted changed: something old may be able to go.
    changed: bool,
    built: bool,
    /// The host refused a mesh (over 512 meshes or 1M vertices).
    pub refused: u32,
    /// Tiles built, ever.
    pub made: u32,
    /// Tiles built in the last update.
    pub made_last: u32,
}

impl Terrain {
    /// A terrain from its desc (check it first: [`TerrainDesc::check`]).
    /// Nothing is built until [`update`](Terrain::update).
    pub fn new(desc: TerrainDesc) -> Terrain {
        let heights = Heights::new(desc.noise.as_ref(), desc.height, &desc.stamps, desc.detail, desc.size);
        let reach = match desc.size {
            // No further than the far corner of the world from its middle.
            Some(s) => desc.reach.min(s * 0.75),
            None => desc.reach,
        };
        Terrain {
            layout: Layout::new(desc.detail, reach, heights.half()),
            heights,
            tint: if desc.paint.is_empty() { Tint::natural(desc.noise.map_or(1, |n| n.seed), if desc.noise.is_some() { desc.height } else { desc.height.max(10.0) }) } else { Tint::None },
            desc,
            material: None,
            root: None,
            live: BTreeMap::new(),
            wanted: Vec::new(),
            queue: Vec::new(),
            dirty: Vec::new(),
            spare: Vec::new(),
            scratch: Scratch::default(),
            cover: Vec::new(),
            last_select: None,
            changed: false,
            built: false,
            refused: 0,
            made: 0,
            made_last: 0,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn heights(&self) -> &Heights {
        &self.heights
    }

    /// The true ground's height at (x, z): the same for every player,
    /// whatever level is drawn there.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.heights.height(x, z)
    }

    /// The true ground's normal at (x, z) (over `detail` either side).
    pub fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        let [a, b, c] = self.heights.normal(x, z, self.desc.detail);
        vec3(a, b, c)
    }

    /// Raises the ground by `amount` meters (negative digs) in a smooth
    /// bump out to `radius`; the tiles it touches are built again next update.
    pub fn raise(&mut self, x: f32, z: f32, radius: f32, amount: f32) {
        let r = self.heights.raise(x, z, radius, amount);
        self.touch(r);
    }

    /// Live tiles over the square `r` ([x0, z0, x1, z1]) are built again.
    fn touch(&mut self, r: [f32; 4]) {
        for k in self.live.keys() {
            let (x0, z0, s) = self.layout.rect(*k);
            if x0 < r[2] && x0 + s > r[0] && z0 < r[3] && z0 + s > r[1] && !self.dirty.contains(k) {
                self.dirty.push(*k);
            }
        }
    }

    /// Back to the ground as made: every `raise` taken back, and the first
    /// view built again (a new round taking over a terrain the last one left).
    pub fn restart(&mut self) {
        for r in self.heights.unraise() {
            self.touch(r);
        }
        self.built = false;
        self.last_select = None;
    }

    /// How much of the first view is built, 0–1.
    pub fn progress(&self) -> f32 {
        if self.built {
            return 1.0;
        }
        if self.wanted.is_empty() {
            return 0.0;
        }
        (self.wanted.len() - self.queue.len()) as f32 / self.wanted.len() as f32
    }

    /// The first view is built.
    pub fn built(&self) -> bool {
        self.built
    }

    /// Picks what's wanted from (cx, cz) and builds within the frame's
    /// budget. Returns true the frame the first view becomes built.
    pub fn update(&mut self, eye: Vec3) -> bool {
        self.look_from(eye.x, eye.z);
        let fast = !self.built || self.holes(self.layout.reach * 0.75) > 0;
        self.pump(if fast { FAST_VERTS } else { Layout::verts() });
        if !self.built && self.queue.is_empty() && self.last_select.is_some() {
            self.built = true;
            return true;
        }
        false
    }

    /// Shows or hides every tile.
    pub fn set_visible(&mut self, on: bool) {
        if let Some(r) = self.root {
            r.set_visible(on);
        }
    }

    /// Picks what's wanted from here, when the camera has moved a quarter of
    /// a chunk since it last did.
    fn look_from(&mut self, cx: f32, cz: f32) {
        if let Some((x, z)) = self.last_select {
            let (dx, dz) = (cx - x, cz - z);
            let q = self.layout.chunk() * 0.25;
            if dx * dx + dz * dz < q * q {
                return;
            }
        }
        self.last_select = Some((cx, cz));
        self.changed = true;
        self.layout.select(cx, cz, &mut self.wanted);
        self.queue.clear();
        for &(k, d) in &self.wanted {
            if !self.live.contains_key(&k) {
                self.queue.push((d, k));
            }
        }
        self.queue.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    }

    /// Builds what `raise` changed, then tiles until `budget` vertices are
    /// spent (always one if any is waiting). Returns the vertices built.
    fn pump(&mut self, budget: u32) -> u32 {
        self.made_last = 0;
        for k in std::mem::take(&mut self.dirty) {
            if self.live.contains_key(&k) {
                self.rebuild(k);
            }
        }
        let mut spent = 0u32;
        let cost = Layout::verts();
        while let Some(&(_, k)) = self.queue.last() {
            if spent > 0 && spent + cost > budget {
                break;
            }
            self.queue.pop();
            if !self.build(k) {
                // Over mb3d's limits: nothing more fits until something goes.
                self.refused += 1;
                self.queue.clear();
                break;
            }
            spent += cost;
        }
        if std::mem::take(&mut self.changed) || spent > 0 {
            self.retire();
        }
        spent
    }

    /// The ground's surfaces, as the ground shader takes them (`paint`, or the natural ground).
    pub fn surfaces(&self) -> super::SurfacesDesc {
        let span = if self.desc.noise.is_some() { self.desc.height } else { self.desc.height.max(10.0) };
        ground::surfaces(&self.desc.paint, &self.desc.surfaces, self.desc.texture, span)
    }

    /// The tiles' material, the same at every level: the ground shader
    /// draws each surface's detail near and its mean colour far.
    fn material(&mut self) -> Option<Material> {
        if self.material.is_none() {
            let m = Material::new(&MaterialDesc { base_color: [1.0, 1.0, 1.0, 1.0], roughness: 0.92, ..MaterialDesc::default() })?;
            m.set_surfaces(&self.surfaces());
            self.material = Some(m);
        }
        self.material
    }

    fn root(&mut self) -> Option<Node> {
        if self.root.is_none() {
            self.root = Node::new();
            // Tiles are the ground: the scene's draw distance never fades them.
            if let Some(r) = self.root {
                r.never_fade();
            }
        }
        self.root
    }

    fn build(&mut self, k: Key) -> bool {
        let m = tile::mesh(&self.heights, &self.layout, k, &self.tint, &mut self.scratch);
        let (center, radius) = tile::bounds(&m);
        let (Some(material), Some(root)) = (self.material(), self.root()) else {
            return false;
        };
        let Some(mesh) = m.upload() else { return false };
        let Some(node) = self.spare.pop().or_else(Node::new) else {
            mesh.free();
            return false;
        };
        let (x0, z0, s) = self.layout.rect(k);
        let at = vec3(x0 + s * 0.5, 0.0, z0 + s * 0.5);
        node.set_parent(Some(root));
        node.set_transform(&Transform { pos: at, rot: Quat::IDENTITY, scale: Vec3::ONE });
        node.set_mesh(mesh, material);
        node.set_visible(false);
        self.made += 1;
        self.made_last += 1;
        self.live.insert(k, Tile { node, mesh, center: center + at, radius, shown: false });
        true
    }

    /// A live tile's mesh made again (after `raise`), swapped in place.
    fn rebuild(&mut self, k: Key) {
        let m = tile::mesh(&self.heights, &self.layout, k, &self.tint, &mut self.scratch);
        let (center, radius) = tile::bounds(&m);
        let Some(material) = self.material() else {
            return;
        };
        let Some(mesh) = m.upload() else {
            self.refused += 1;
            return;
        };
        let (x0, z0, s) = self.layout.rect(k);
        let at = vec3(x0 + s * 0.5, 0.0, z0 + s * 0.5);
        if let Some(t) = self.live.get_mut(&k) {
            t.node.set_mesh(mesh, material);
            t.mesh.free();
            t.mesh = mesh;
            t.center = center + at;
            t.radius = radius;
            self.made += 1;
            self.made_last += 1;
        }
    }

    /// Old tiles go when everything replacing them is built; new tiles show
    /// when nothing old overlaps them. So the ground never has a hole in it
    /// and two levels never draw in the same place.
    fn retire(&mut self) {
        let wanted = &self.wanted;
        let is_wanted = |k: &Key| wanted.binary_search_by_key(k, |e| e.0).is_ok();
        let old: Vec<Key> = self.live.keys().filter(|k| !is_wanted(k)).copied().collect();
        let mut remaining = Vec::new();
        for k in old {
            self.layout.cover(k, &self.wanted, &mut self.cover);
            if self.cover.iter().all(|c| self.live.contains_key(c)) {
                self.remove(k);
            } else {
                remaining.push(k);
            }
        }
        let layout = self.layout;
        let rect = |k: Key| {
            let (x, z, s) = layout.rect(k);
            (x, z, x + s, z + s)
        };
        for (k, t) in self.live.iter_mut() {
            if t.shown || self.wanted.binary_search_by_key(k, |e| e.0).is_err() {
                continue;
            }
            let a = rect(*k);
            let blocked = remaining.iter().any(|o| {
                let b = rect(*o);
                a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
            });
            if !blocked {
                t.node.set_visible(true);
                t.shown = true;
            }
        }
    }

    fn remove(&mut self, k: Key) {
        if let Some(t) = self.live.remove(&k) {
            t.node.clear_mesh();
            t.mesh.free();
            t.node.set_visible(false);
            self.spare.push(t.node);
        }
    }

    /// Wanted tiles nearer than `within` that nothing shown covers: holes.
    pub fn holes(&self, within: f32) -> u32 {
        let mut n = 0;
        for &(d, k) in &self.queue {
            if d >= within {
                continue;
            }
            let (x, z, s) = self.layout.rect(k);
            let covered = self.live.iter().any(|(o, t)| {
                let (ox, oz, os) = self.layout.rect(*o);
                t.shown && x < ox + os && ox < x + s && z < oz + os && oz < z + s
            });
            if !covered {
                n += 1;
            }
        }
        n
    }

    /// Tiles alive on the host, and how many are waiting to be built.
    pub fn counts(&self) -> (usize, usize) {
        (self.live.len(), self.queue.len())
    }

    /// The live tiles' keys, and whether each is shown.
    pub fn tiles(&self) -> impl Iterator<Item = (Key, bool)> + '_ {
        self.live.iter().map(|(k, t)| (*k, t.shown))
    }

    /// What the host's frustum culling leaves of the shown tiles: the same
    /// six planes and the same spheres.
    pub fn seen(&self, cam: &Camera, aspect: f32) -> Seen {
        let rot = cam.rot.normalize();
        let (f, r, u) = (rot.forward(), rot.right(), rot * Vec3::Y);
        let tv = (cam.fov_y * 0.5).tan();
        let th = tv * aspect;
        let (lh, lv) = ((1.0 + th * th).sqrt(), (1.0 + tv * tv).sqrt());
        let mut out = Seen::default();
        for (k, t) in &self.live {
            if !t.shown {
                continue;
            }
            let d = t.center - cam.pos;
            let (dz, dx, dy) = (d.dot(f), d.dot(r), d.dot(u));
            let rad = t.radius;
            let inside = dz + rad >= cam.near && dz - rad <= cam.far && (dz * th - dx) / lh >= -rad && (dz * th + dx) / lh >= -rad && (dz * tv - dy) / lv >= -rad && (dz * tv + dy) / lv >= -rad;
            if inside {
                out.draws += 1;
                out.tris += Layout::tris();
                out.by_lod[k.lod as usize] += 1;
            }
        }
        out
    }

    /// Frees everything on the host: its tiles, nodes and material (not the
    /// texture, which is the game's).
    pub fn free(mut self) {
        for (_, t) in std::mem::take(&mut self.live) {
            t.mesh.free();
        }
        if let Some(r) = self.root.take() {
            r.destroy();
        }
        if let Some(m) = self.material.take() {
            m.free();
        }
        // Spare nodes were under the root, destroyed with it.
        self.spare.clear();
    }
}
