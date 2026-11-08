//! Navigation (docs/STUDIO-2.md §4.14, spike S10, Decided 45–48): navmeshes
//! baked from a level's triangles, paths on them and a crowd walking them,
//! on Recast, Detour and DetourCrowd 1.6.0 (C++, zlib; crates/recast). And
//! A* on a grid ([`grid_path`], the `pathfinding` crate), for tile maps.
//!
//! Features: `nav` is the navmesh (it builds C++, so it needs the wasm
//! clang crates/luau does); `gridpath` is [`grid_path`] alone.
//!
//! ```ignore
//! use maimbrain::nav::{Geometry, Region, Settings};
//! let mut region = Region::new(&Settings::default(), vec3(-32.0, -10.0, -32.0), vec3(32.0, 20.0, 32.0)).unwrap();
//! region.set_geometry(level_triangles);          // queues the tiles
//! // update(dt): a few tiles a frame, never all at once
//! region.bake_step(4);
//! if region.is_baked() { region.path(a, b, &mut corners); }
//! ```
//!
//! **Baking.** A region is cut into square tiles (16 m at the default
//! 0.25 m cell). [`Region::set_geometry`] only queues the tiles whose
//! triangles changed (all of them the first time); [`Region::bake_step`]
//! builds up to a budget of them (~1 ms each on an iPhone 16 Pro, S10). A
//! game has no clock, so the budget is in tiles. The bytes are the same
//! natively and in wasm ([`Region::hash`]).
//!
//! **The crowd.** DetourCrowd's agents ([`Region::add_agent`]), stepped by
//! [`Region::update_crowd`] once a frame by the recorded `dt`. With
//! obstacle avoidance on (Decided 47), at Recast's demo's best preset, as
//! S10 measured it.

#[cfg(feature = "nav")]
pub use region::*;

#[cfg(feature = "nav")]
mod region {
    use std::collections::VecDeque;

    use crate::gfx3d::{Vec3, vec3};

    /// A navmesh's settings: Godot's `NavigationMesh` defaults but a finer
    /// cell height. Lengths are meters, `max_slope` radians.
    ///
    /// Recast reads a slope as a ledge when a cell's neighbours differ by
    /// more than `max_climb` (in whole cell heights), so a slope is walkable
    /// up to about atan(`max_climb` ÷ (2 × `cell_size`)) as well as
    /// `max_slope`: 26° at Godot's 0.25 m climb, 39° at 0.4 m. The cell
    /// height is 0.05 m (Godot's is 0.25), so the climb is hardly rounded down
    /// (0.4 m was one 0.25 m cell); the detail mesh still keeps within 0.25 m.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Settings {
        pub cell_size: f32,
        pub cell_height: f32,
        pub agent_radius: f32,
        pub agent_height: f32,
        /// The highest step an agent walks up.
        pub max_climb: f32,
        /// The steepest slope it walks on, radians.
        pub max_slope: f32,
        /// Regions smaller than this many cells a side go (islands).
        pub region_min_size: i32,
        pub region_merge_size: i32,
        pub edge_max_length: f32,
        pub edge_max_error: f32,
        /// Cells between detail samples (the navmesh's heights), 0 for none.
        pub detail_sample_distance: f32,
        pub detail_sample_max_error: f32,
        /// A tile's side in cells: 64 × 0.25 m = 16 m.
        pub tile_cells: i32,
    }

    impl Default for Settings {
        fn default() -> Settings {
            Settings {
                cell_size: 0.25,
                cell_height: 0.05,
                agent_radius: 0.5,
                agent_height: 1.5,
                max_climb: 0.25,
                max_slope: 45f32.to_radians(),
                region_min_size: 2,
                region_merge_size: 20,
                edge_max_length: 12.0,
                edge_max_error: 1.3,
                detail_sample_distance: 6.0,
                detail_sample_max_error: 5.0,
                tile_cells: 64,
            }
        }
    }

    /// The level as the bake sees it: triangles in world space, and solids
    /// whose insides aren't walkable (a trunk, a box: its outline from above).
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Geometry {
        /// x, y, z per vertex.
        pub verts: Vec<f32>,
        /// Three vertex indices per triangle, wound either way.
        pub tris: Vec<i32>,
        pub solids: Vec<Solid>,
    }

    /// A solid: a convex outline (x, z pairs, at most 64) from `y0` to `y1`.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Solid {
        pub outline: Vec<[f32; 2]>,
        pub y0: f32,
        pub y1: f32,
    }

    impl Geometry {
        /// Adds triangles (three vertices each) moved by `f`.
        pub fn add_mesh(&mut self, verts: &[Vec3], tris: &[u32], f: impl Fn(Vec3) -> Vec3) {
            let base = (self.verts.len() / 3) as i32;
            for v in verts {
                let p = f(*v);
                self.verts.extend([p.x, p.y, p.z]);
            }
            self.tris.extend(tris.iter().map(|i| base + *i as i32));
        }

        /// A height grid as triangles: `n × n` samples `step` apart from
        /// (x0, z0), row by row along x (as `terrain::Heights::fill` gives them).
        pub fn add_heights(&mut self, x0: f32, z0: f32, step: f32, n: usize, heights: &[f32]) {
            let base = (self.verts.len() / 3) as i32;
            for j in 0..n {
                for i in 0..n {
                    self.verts.extend([x0 + i as f32 * step, heights[j * n + i], z0 + j as f32 * step]);
                }
            }
            for j in 0..n - 1 {
                for i in 0..n - 1 {
                    let a = base + (j * n + i) as i32;
                    let (b, c, d) = (a + 1, a + n as i32, a + n as i32 + 1);
                    self.tris.extend([a, c, b, b, c, d]);
                }
            }
        }

        pub fn triangles(&self) -> usize {
            self.tris.len() / 3
        }

        /// FNV-1a over everything, for whether a bake can be reused.
        pub fn hash(&self) -> u32 {
            let mut h = FNV;
            for v in &self.verts {
                h = fnv(h, &v.to_bits().to_le_bytes());
            }
            for t in &self.tris {
                h = fnv(h, &t.to_le_bytes());
            }
            for s in &self.solids {
                for p in &s.outline {
                    h = fnv(h, &p[0].to_bits().to_le_bytes());
                    h = fnv(h, &p[1].to_bits().to_le_bytes());
                }
                h = fnv(h, &s.y0.to_bits().to_le_bytes());
                h = fnv(h, &s.y1.to_bits().to_le_bytes());
            }
            h
        }
    }

    const FNV: u32 = 2166136261;

    fn fnv(mut h: u32, b: &[u8]) -> u32 {
        for x in b {
            h ^= *x as u32;
            h = h.wrapping_mul(16777619);
        }
        h
    }

    /// An agent's settings (Godot's `NavigationAgent3D`'s, those DetourCrowd has).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct AgentSettings {
        pub radius: f32,
        pub height: f32,
        pub max_speed: f32,
        /// m/s², how fast it changes velocity.
        pub max_accel: f32,
        /// Detour's obstacle avoidance (others and walls ahead), on by default (Decided 47).
        pub avoidance: bool,
    }

    impl Default for AgentSettings {
        fn default() -> AgentSettings {
            AgentSettings { radius: 0.5, height: 1.5, max_speed: 5.0, max_accel: 20.0, avoidance: true }
        }
    }

    impl AgentSettings {
        fn raw(&self) -> recast::AgentParams {
            recast::AgentParams {
                radius: self.radius.clamp(0.05, MAX_AGENT_RADIUS),
                height: self.height.max(0.1),
                max_accel: self.max_accel.max(0.0),
                max_speed: self.max_speed.max(0.0),
                avoidance: if self.avoidance { 3 } else { -1 },
                separation: if self.avoidance { 2.0 } else { 0.0 },
            }
        }
    }

    /// An agent in a [`Region`]'s crowd.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct AgentId(i32);

    pub use recast::Agent;

    /// Agents a crowd has room for (§4.6's budget is 50).
    pub const MAX_AGENTS: i32 = 128;
    /// The widest agent.
    pub const MAX_AGENT_RADIUS: f32 = 2.0;
    /// How far from the navmesh a point is looked for (half extents, m).
    pub const REACH: [f32; 3] = [2.0, 4.0, 2.0];

    /// A tiled navmesh over a box: its tiles queued and baked a few at a
    /// time, its paths and its crowd.
    pub struct Region {
        nav: recast::NavMesh,
        settings: Settings,
        cfg: recast::Config,
        tiles: (i32, i32),
        geom: Geometry,
        /// Per tile: the hash of what it's built from, and of what it built (0: nothing walkable).
        input: Vec<u32>,
        built: Vec<u32>,
        queue: VecDeque<usize>,
        tiles_built: u64,
        /// The crowd's agents and whether each is in it (an agent made off the
        /// navmesh, or before its tile, joins once it can).
        agents: Vec<(i32, Vec3, AgentSettings, Option<Vec3>)>,
    }

    impl Region {
        /// A region over the box `min`–`max` (its height range bounds what's baked).
        pub fn new(settings: &Settings, min: Vec3, max: Vec3) -> Option<Region> {
            let s = *settings;
            let tile = s.tile_cells as f32 * s.cell_size;
            let tiles = (((max.x - min.x) / tile).ceil().max(1.0) as i32, ((max.z - min.z) / tile).ceil().max(1.0) as i32);
            let cfg = recast::Config {
                cell_size: s.cell_size,
                cell_height: s.cell_height,
                agent_height: s.agent_height,
                agent_radius: s.agent_radius,
                agent_climb: s.max_climb,
                max_slope_deg: s.max_slope.to_degrees(),
                region_min: s.region_min_size,
                region_merge: s.region_merge_size,
                edge_max_len: s.edge_max_length,
                edge_max_error: s.edge_max_error,
                verts_per_poly: 6,
                detail_sample_dist: s.detail_sample_distance,
                detail_sample_max_error: s.detail_sample_max_error,
                tile_cells: s.tile_cells,
                bmin: [min.x, min.y, min.z],
                bmax: [max.x, max.y, max.z],
                max_tiles: tiles.0 * tiles.1,
            };
            let mut nav = recast::NavMesh::new(&cfg)?;
            nav.crowd_init(MAX_AGENTS, MAX_AGENT_RADIUS);
            let n = (tiles.0 * tiles.1) as usize;
            Some(Region { nav, settings: s, cfg, tiles, geom: Geometry::default(), input: vec![0; n], built: vec![0; n], queue: VecDeque::new(), tiles_built: 0, agents: Vec::new() })
        }

        pub fn settings(&self) -> &Settings {
            &self.settings
        }

        /// Its box: min and max.
        pub fn bounds(&self) -> (Vec3, Vec3) {
            let (a, b) = (self.cfg.bmin, self.cfg.bmax);
            (vec3(a[0], a[1], a[2]), vec3(b[0], b[1], b[2]))
        }

        /// Tiles across x and z.
        pub fn tile_count(&self) -> (i32, i32) {
            self.tiles
        }

        /// The level to bake: queues every tile whose triangles (or solids)
        /// changed since the last geometry, and returns how many.
        pub fn set_geometry(&mut self, g: Geometry) -> usize {
            let (tw, th) = self.tiles;
            let tile = self.cfg.tile_cells as f32 * self.cfg.cell_size;
            // The border Recast adds round a tile: the agent's radius and 3 cells.
            let border = ((self.cfg.agent_radius / self.cfg.cell_size).ceil() + 3.0) * self.cfg.cell_size;
            let mut hashes = vec![FNV; (tw * th) as usize];
            let span = |lo: f32, hi: f32, o: f32, n: i32| -> (i32, i32) {
                let a = ((lo - border - o) / tile).floor() as i32;
                let b = ((hi + border - o) / tile).floor() as i32;
                (a.max(0), b.min(n - 1))
            };
            for t in g.tris.chunks_exact(3) {
                let p = |i: i32| {
                    let i = i as usize * 3;
                    [g.verts[i], g.verts[i + 1], g.verts[i + 2]]
                };
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                let (x0, x1) = span(a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]), self.cfg.bmin[0], tw);
                let (z0, z1) = span(a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]), self.cfg.bmin[2], th);
                for tz in z0..=z1 {
                    for tx in x0..=x1 {
                        let h = &mut hashes[(tz * tw + tx) as usize];
                        for v in a.iter().chain(&b).chain(&c) {
                            *h = fnv(*h, &v.to_bits().to_le_bytes());
                        }
                    }
                }
            }
            for s in &g.solids {
                let (lx, hx) = s.outline.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]), m.1.max(p[0])));
                let (lz, hz) = s.outline.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[1]), m.1.max(p[1])));
                let (x0, x1) = span(lx, hx, self.cfg.bmin[0], tw);
                let (z0, z1) = span(lz, hz, self.cfg.bmin[2], th);
                for tz in z0..=z1 {
                    for tx in x0..=x1 {
                        let h = &mut hashes[(tz * tw + tx) as usize];
                        for v in s.outline.iter().flatten().chain([&s.y0, &s.y1]) {
                            *h = fnv(*h, &v.to_bits().to_le_bytes());
                        }
                        *h = fnv(*h, b"solid");
                    }
                }
            }
            self.geom = g;
            let mut n = 0;
            for (i, h) in hashes.into_iter().enumerate() {
                if self.input[i] != h || self.tiles_built == 0 {
                    self.input[i] = h;
                    if !self.queue.contains(&i) {
                        self.queue.push_back(i);
                    }
                    n += 1;
                }
            }
            n
        }

        /// Builds up to `max` queued tiles; returns how many it built.
        pub fn bake_step(&mut self, max: usize) -> usize {
            let mut done = 0;
            let (tw, _) = self.tiles;
            let tile = self.cfg.tile_cells as f32 * self.cfg.cell_size;
            let border = ((self.cfg.agent_radius / self.cfg.cell_size).ceil() + 3.0) * self.cfg.cell_size;
            let mut tris: Vec<i32> = Vec::new();
            let mut vols: Vec<f32> = Vec::new();
            while done < max {
                let Some(i) = self.queue.pop_front() else { break };
                let (tx, tz) = (i as i32 % tw, i as i32 / tw);
                let (x0, z0) = (self.cfg.bmin[0] + tx as f32 * tile - border, self.cfg.bmin[2] + tz as f32 * tile - border);
                let (x1, z1) = (x0 + tile + 2.0 * border, z0 + tile + 2.0 * border);
                // The triangles and solids that touch it.
                tris.clear();
                let g = &self.geom;
                for t in g.tris.chunks_exact(3) {
                    let x = |k: usize| g.verts[t[k] as usize * 3];
                    let z = |k: usize| g.verts[t[k] as usize * 3 + 2];
                    if x(0).max(x(1)).max(x(2)) < x0 || x(0).min(x(1)).min(x(2)) > x1 || z(0).max(z(1)).max(z(2)) < z0 || z(0).min(z(1)).min(z(2)) > z1 {
                        continue;
                    }
                    tris.extend_from_slice(t);
                }
                vols.clear();
                for s in &g.solids {
                    if s.outline.len() < 3 || s.outline.len() > 64 {
                        continue;
                    }
                    if s.outline.iter().all(|p| p[0] < x0) || s.outline.iter().all(|p| p[0] > x1) || s.outline.iter().all(|p| p[1] < z0) || s.outline.iter().all(|p| p[1] > z1) {
                        continue;
                    }
                    vols.extend([s.outline.len() as f32, s.y0, s.y1]);
                    vols.extend(s.outline.iter().flatten());
                }
                match self.nav.build_tile(&g.verts, &tris, &vols, tx, tz) {
                    Some(data) => {
                        self.built[i] = fnv(FNV, data.bytes()) | 1;
                        if !self.nav.add_tile(data, tx, tz) {
                            self.built[i] = 0;
                        }
                    }
                    None => {
                        self.nav.remove_tile(tx, tz);
                        self.built[i] = 0;
                    }
                }
                self.tiles_built += 1;
                done += 1;
            }
            if done > 0 {
                self.join_agents();
            }
            done
        }

        /// Every queued tile built.
        pub fn is_baked(&self) -> bool {
            self.queue.is_empty() && self.tiles_built > 0
        }

        /// 0–1: how much of the queue is built.
        pub fn progress(&self) -> f32 {
            let n = (self.tiles.0 * self.tiles.1) as f32;
            if self.is_baked() { 1.0 } else { 1.0 - self.queue.len() as f32 / n }
        }

        /// Tiles waiting.
        pub fn queued(&self) -> usize {
            self.queue.len()
        }

        /// FNV-1a over every tile's navmesh bytes, in order: the same
        /// natively and in wasm.
        pub fn hash(&self) -> u32 {
            self.built.iter().fold(FNV, |h, b| fnv(h, &b.to_le_bytes()))
        }

        /// Walkable tiles built.
        pub fn walkable_tiles(&self) -> usize {
            self.built.iter().filter(|b| **b != 0).count()
        }

        /// The navmesh's nearest point to `p`, within `REACH`.
        pub fn nearest(&self, p: Vec3) -> Option<Vec3> {
            self.nav.nearest([p.x, p.y, p.z], REACH).map(|q| vec3(q[0], q[1], q[2]))
        }

        /// A path's corners from `from` to `to` (each found within `REACH` of
        /// the navmesh): Some(true) when it gets there, Some(false) when it
        /// ends as near as it can, None when either end is off the navmesh.
        pub fn path(&self, from: Vec3, to: Vec3, out: &mut Vec<Vec3>) -> Option<bool> {
            let mut raw = Vec::new();
            let r = self.nav.path([from.x, from.y, from.z], [to.x, to.y, to.z], REACH, &mut raw);
            out.clear();
            out.extend(raw.iter().map(|p| vec3(p[0], p[1], p[2])));
            r
        }

        /// The polygons' edges, for drawing: at most `max` segments.
        pub fn outlines(&self, max: usize, out: &mut Vec<(Vec3, Vec3)>) {
            let mut raw = Vec::new();
            self.nav.outlines(max, &mut raw);
            out.clear();
            out.extend(raw.chunks_exact(6).map(|s| (vec3(s[0], s[1], s[2]), vec3(s[3], s[4], s[5]))));
        }

        // -------------------------------------------------------------- crowd

        /// A new agent at `pos` (its feet). It joins the crowd at once if
        /// the navmesh is there, else as soon as it is. None when the crowd is full.
        pub fn add_agent(&mut self, pos: Vec3, s: &AgentSettings) -> Option<AgentId> {
            if self.agents.iter().filter(|a| a.0 != -2).count() >= MAX_AGENTS as usize {
                return None;
            }
            let id = self.agents.len() as i32;
            self.agents.push((-1, pos, *s, None));
            self.join(id as usize);
            Some(AgentId(id))
        }

        fn join(&mut self, i: usize) {
            let (slot, pos, s, target) = self.agents[i];
            if slot != -1 || self.nearest(pos).is_none() {
                return;
            }
            if let Some(k) = self.nav.crowd_add([pos.x, pos.y, pos.z], &s.raw()) {
                self.agents[i].0 = k;
                if let Some(t) = target {
                    self.nav.crowd_target(k, [t.x, t.y, t.z], REACH);
                }
            }
        }

        fn join_agents(&mut self) {
            for i in 0..self.agents.len() {
                self.join(i);
            }
        }

        pub fn remove_agent(&mut self, id: AgentId) {
            let Some(a) = self.agents.get_mut(id.0 as usize) else { return };
            if a.0 >= 0 {
                self.nav.crowd_remove(a.0);
            }
            a.0 = -2;
        }

        pub fn set_agent(&mut self, id: AgentId, s: &AgentSettings) {
            let Some(a) = self.agents.get_mut(id.0 as usize) else { return };
            a.2 = *s;
            if a.0 >= 0 {
                let k = a.0;
                self.nav.crowd_params(k, &s.raw());
            }
        }

        /// Sends it to the navmesh's nearest point to `to` (false: there's none near).
        pub fn set_target(&mut self, id: AgentId, to: Vec3) -> bool {
            let Some(a) = self.agents.get_mut(id.0 as usize) else { return false };
            a.3 = Some(to);
            if a.0 >= 0 {
                let k = a.0;
                return self.nav.crowd_target(k, [to.x, to.y, to.z], REACH);
            }
            false
        }

        /// It stops and forgets its target.
        pub fn stop(&mut self, id: AgentId) {
            let Some(a) = self.agents.get_mut(id.0 as usize) else { return };
            a.3 = None;
            if a.0 >= 0 {
                let k = a.0;
                self.nav.crowd_stop(k);
            }
        }

        /// One step of the crowd, by `dt`.
        pub fn update_crowd(&mut self, dt: f32) {
            if self.agents.iter().any(|a| a.0 >= 0) {
                self.nav.crowd_update(dt);
            }
        }

        /// Where the crowd has it (None until it's on the navmesh).
        pub fn agent(&self, id: AgentId) -> Option<Agent> {
            let a = self.agents.get(id.0 as usize)?;
            (a.0 >= 0).then(|| self.nav.crowd_agent(a.0))
        }

        /// Puts it back where its body ended up (along its corridor).
        pub fn place_agent(&mut self, id: AgentId, pos: Vec3) {
            let Some(a) = self.agents.get_mut(id.0 as usize) else { return };
            a.1 = pos;
            if a.0 >= 0 {
                let k = a.0;
                self.nav.crowd_place(k, [pos.x, pos.y, pos.z]);
            } else if a.0 == -1 {
                self.join(id.0 as usize);
            }
        }

        /// Puts it at `pos` at once, wherever that is (a respawn): it leaves
        /// the crowd and joins again there, still on its way to its target.
        /// [`Region::place_agent`] only follows its corridor, so a jump is lost.
        pub fn teleport_agent(&mut self, id: AgentId, pos: Vec3) {
            let i = id.0 as usize;
            let Some(a) = self.agents.get_mut(i) else { return };
            if a.0 == -2 {
                return;
            }
            if a.0 >= 0 {
                self.nav.crowd_remove(a.0);
            }
            (a.0, a.1) = (-1, pos);
            self.join(i);
        }

        /// Agents in the crowd now.
        pub fn agent_count(&self) -> usize {
            self.agents.iter().filter(|a| a.0 >= 0).count()
        }

        /// A new, empty crowd (a region kept for another round starts as a
        /// new one would: nothing of the last round's crowd carries over).
        pub fn clear_agents(&mut self) {
            self.nav.crowd_init(MAX_AGENTS, MAX_AGENT_RADIUS);
            self.agents.clear();
        }
    }
}

/// A* on a grid of `w × h` cells from `from` to `to` (the `pathfinding`
/// crate, Decided 48): the cells of a shortest path, both ends included, or
/// None. `open(x, y)` says a cell can be walked. Neighbours are tried in a
/// fixed order (east, west, south, north, then the diagonals when
/// `diagonal`; a diagonal never cuts a blocked corner), so ties between
/// equal paths break the same way on every device.
#[cfg(feature = "gridpath")]
pub fn grid_path(w: i32, h: i32, from: (i32, i32), to: (i32, i32), diagonal: bool, open: impl Fn(i32, i32) -> bool) -> Option<Vec<(i32, i32)>> {
    let inside = |(x, y): (i32, i32)| x >= 0 && y >= 0 && x < w && y < h;
    if !inside(from) || !inside(to) || !open(to.0, to.1) {
        return None;
    }
    const STEPS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)];
    let n = if diagonal { 8 } else { 4 };
    let ok = |x: i32, y: i32| inside((x, y)) && open(x, y);
    pathfinding::prelude::astar(
        &from,
        |&(x, y)| {
            STEPS[..n]
                .iter()
                .filter(|(dx, dy)| ok(x + dx, y + dy) && (*dx == 0 || *dy == 0 || (ok(x + dx, y) && ok(x, y + dy))))
                .map(|(dx, dy)| ((x + dx, y + dy), if *dx != 0 && *dy != 0 { 14u32 } else { 10 }))
                .collect::<Vec<_>>()
        },
        |&(x, y)| {
            let (dx, dy) = ((x - to.0).unsigned_abs(), (y - to.1).unsigned_abs());
            if diagonal { 10 * dx.max(dy) + 4 * dx.min(dy) } else { 10 * (dx + dy) }
        },
        |&p| p == to,
    )
    .map(|(p, _)| p)
}

#[cfg(test)]
#[path = "nav_tests.rs"]
mod tests;
