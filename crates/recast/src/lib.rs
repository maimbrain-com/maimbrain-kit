//! Recast, Detour and DetourCrowd 1.6.0 (C++, zlib, by Mikko Mononen),
//! built from source (build.rs), behind a small C shim (csrc/shim.cpp):
//! what the SDK's `nav` module (sdk/maimbrain/src/nav.rs) bakes navmeshes,
//! finds paths and moves crowds with (docs/STUDIO-2.md §4.14, spike S10,
//! Decided 45).
//!
//! This crate is the shim's Rust side and what the C++ needs from Rust: its
//! allocator ([`mbrc_alloc`]) and the trig csrc/portable.h sends here, so a
//! native build bakes the same bytes as the wasm one. Everything is in
//! world space: meters, +y up.

use std::alloc::{Layout, alloc, dealloc};
use std::ffi::c_void;

/// A navmesh's build settings (Recast's `rcConfig`, in meters and degrees),
/// and the box it covers, cut into square tiles of `tile_cells` cells.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub cell_size: f32,
    pub cell_height: f32,
    pub agent_height: f32,
    pub agent_radius: f32,
    pub agent_climb: f32,
    pub max_slope_deg: f32,
    /// Regions smaller than this many cells a side go.
    pub region_min: i32,
    /// Regions smaller than this many cells a side merge into a neighbour.
    pub region_merge: i32,
    pub edge_max_len: f32,
    pub edge_max_error: f32,
    pub verts_per_poly: i32,
    /// In cells (Recast's sample distance ÷ the cell size); under 0.9, none.
    pub detail_sample_dist: f32,
    /// In cell heights.
    pub detail_sample_max_error: f32,
    pub tile_cells: i32,
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
    pub max_tiles: i32,
}

/// An agent's settings (DetourCrowd's `dtCrowdAgentParams`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentParams {
    pub radius: f32,
    pub height: f32,
    pub max_accel: f32,
    pub max_speed: f32,
    /// Recast's demo's obstacle-avoidance preset, 0 (low) to 3 (high); −1 for none.
    pub avoidance: i32,
    /// How hard agents keep apart (0: not at all).
    pub separation: f32,
}

unsafe extern "C" {
    fn mbrc_nav_new(cfg: *const Config) -> *mut c_void;
    fn mbrc_nav_free(s: *mut c_void);
    fn mbrc_free_data(data: *mut u8);
    fn mbrc_build_tile(s: *mut c_void, verts: *const f32, nv: i32, tris: *const i32, nt: i32, vols: *const f32, nvol: i32, tx: i32, ty: i32, out: *mut *mut u8, stats: *mut i32) -> i32;
    fn mbrc_add_tile(s: *mut c_void, data: *mut u8, size: i32, tx: i32, ty: i32) -> i32;
    fn mbrc_remove_tile(s: *mut c_void, tx: i32, ty: i32);
    fn mbrc_nearest(s: *mut c_void, p: *const f32, ext: *const f32, out: *mut f32) -> i32;
    fn mbrc_path(s: *mut c_void, a: *const f32, b: *const f32, ext: *const f32, out: *mut f32, max: i32, whole: *mut i32) -> i32;
    fn mbrc_outlines(s: *const c_void, out: *mut f32, max: i32) -> i32;
    fn mbrc_crowd_init(s: *mut c_void, max_agents: i32, max_radius: f32) -> i32;
    fn mbrc_crowd_add(s: *mut c_void, pos: *const f32, a: *const AgentParams) -> i32;
    fn mbrc_crowd_params(s: *mut c_void, idx: i32, a: *const AgentParams);
    fn mbrc_crowd_remove(s: *mut c_void, idx: i32);
    fn mbrc_crowd_target(s: *mut c_void, idx: i32, pos: *const f32, ext: *const f32) -> i32;
    fn mbrc_crowd_stop(s: *mut c_void, idx: i32);
    fn mbrc_crowd_update(s: *mut c_void, dt: f32);
    fn mbrc_crowd_agent(s: *mut c_void, idx: i32, out: *mut f32);
    fn mbrc_crowd_place(s: *mut c_void, idx: i32, pos: *const f32);
}

const HEAD: usize = 16;

/// The C++'s allocator: Rust's (the game's), with the size in a 16-byte header.
#[unsafe(no_mangle)]
pub extern "C" fn mbrc_alloc(size: usize) -> *mut u8 {
    unsafe {
        let p = alloc(Layout::from_size_align_unchecked(size + HEAD, HEAD));
        if p.is_null() {
            return p;
        }
        (p as *mut usize).write(size);
        p.add(HEAD)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mbrc_free(p: *mut u8) {
    if p.is_null() {
        return;
    }
    unsafe {
        let base = p.sub(HEAD);
        let size = (base as *const usize).read();
        dealloc(base, Layout::from_size_align_unchecked(size + HEAD, HEAD));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mbrc_cosf(x: f32) -> f32 {
    libm::cosf(x)
}
#[unsafe(no_mangle)]
pub extern "C" fn mbrc_sinf(x: f32) -> f32 {
    libm::sinf(x)
}
#[unsafe(no_mangle)]
pub extern "C" fn mbrc_roundf(x: f32) -> f32 {
    libm::roundf(x)
}
#[unsafe(no_mangle)]
pub extern "C" fn mbrc_atan2f(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

/// One tile's navmesh data, as `dtCreateNavMeshData` made it (freed when
/// dropped, unless [`NavMesh::add_tile`] took it).
pub struct TileData {
    ptr: *mut u8,
    len: usize,
    /// Polygons, vertices and detail triangles.
    pub stats: [i32; 3],
}

impl TileData {
    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for TileData {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { mbrc_free_data(self.ptr) }
        }
    }
}

/// An agent as DetourCrowd last moved it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Agent {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub desired: [f32; 3],
    /// On the navmesh (DetourCrowd's state is walking).
    pub walking: bool,
    /// DetourCrowd's target state: 0 none, 1 failed, 2 valid, 3–5 asking, 6 a velocity.
    pub target_state: u8,
    pub corners: u8,
    /// The next corner of its path (its position when it has none).
    pub next: [f32; 3],
    /// Its path ends as near as it can get, not at the target.
    pub partial: bool,
    pub target: [f32; 3],
    /// The corners of its path ahead (`corners` of them, at most 4).
    pub ahead: [[f32; 3]; 4],
}

/// A tiled navmesh with its query and, once [`NavMesh::crowd_init`] made
/// it, its crowd.
pub struct NavMesh {
    s: *mut c_void,
    crowd: bool,
}

impl NavMesh {
    /// An empty navmesh covering `cfg`'s box; None when out of memory.
    pub fn new(cfg: &Config) -> Option<NavMesh> {
        let s = unsafe { mbrc_nav_new(cfg) };
        (!s.is_null()).then_some(NavMesh { s, crowd: false })
    }

    /// Builds tile (tx, ty) from triangles (`verts` x, y, z; `tris` three
    /// vertex indices each): those touching the tile and its border. `vols`
    /// are solids whose insides aren't walkable, each `[n, ymin, ymax, x1,
    /// z1, … xn, zn]` (a convex outline from above, n ≤ 64). None when
    /// nothing there is walkable.
    pub fn build_tile(&self, verts: &[f32], tris: &[i32], vols: &[f32], tx: i32, ty: i32) -> Option<TileData> {
        let mut out = std::ptr::null_mut();
        let mut stats = [0i32; 3];
        let n = unsafe { mbrc_build_tile(self.s, verts.as_ptr(), (verts.len() / 3) as i32, tris.as_ptr(), (tris.len() / 3) as i32, vols.as_ptr(), vols.len() as i32, tx, ty, &mut out, stats.as_mut_ptr()) };
        (n > 0 && !out.is_null()).then_some(TileData { ptr: out, len: n as usize, stats })
    }

    /// Puts a tile in (replacing any tile at (tx, ty)); false if it didn't fit.
    pub fn add_tile(&mut self, mut data: TileData, tx: i32, ty: i32) -> bool {
        let (ptr, len) = (data.ptr, data.len);
        data.ptr = std::ptr::null_mut();
        unsafe { mbrc_add_tile(self.s, ptr, len as i32, tx, ty) == 1 }
    }

    pub fn remove_tile(&mut self, tx: i32, ty: i32) {
        unsafe { mbrc_remove_tile(self.s, tx, ty) }
    }

    /// The navmesh's nearest point to `p` within `ext` (half extents).
    pub fn nearest(&self, p: [f32; 3], ext: [f32; 3]) -> Option<[f32; 3]> {
        let mut out = [0f32; 3];
        (unsafe { mbrc_nearest(self.s, p.as_ptr(), ext.as_ptr(), out.as_mut_ptr()) } == 1).then_some(out)
    }

    /// A straight path from `a` to `b` (each found within `ext` of the
    /// navmesh) into `out`: Some(true) when it reaches `b`, Some(false)
    /// when it stops as near as it can, None when there's none.
    pub fn path(&self, a: [f32; 3], b: [f32; 3], ext: [f32; 3], out: &mut Vec<[f32; 3]>) -> Option<bool> {
        const MAX: usize = 128;
        let mut buf = [0f32; 3 * MAX];
        let mut whole = 0;
        let n = unsafe { mbrc_path(self.s, a.as_ptr(), b.as_ptr(), ext.as_ptr(), buf.as_mut_ptr(), MAX as i32, &mut whole) };
        out.clear();
        if n <= 0 {
            return None;
        }
        out.extend((0..n as usize).map(|i| [buf[i * 3], buf[i * 3 + 1], buf[i * 3 + 2]]));
        Some(whole == 1)
    }

    /// Every polygon's edges, as segments (x1, y1, z1, x2, y2, z2), at most
    /// `max` of them.
    pub fn outlines(&self, max: usize, out: &mut Vec<f32>) {
        out.clear();
        out.resize(max * 6, 0.0);
        let n = unsafe { mbrc_outlines(self.s, out.as_mut_ptr(), out.len() as i32) };
        out.truncate(n.max(0) as usize);
    }

    /// Makes (or makes again, empty) the crowd: at most `max` agents, none
    /// wider than `max_radius`.
    pub fn crowd_init(&mut self, max: i32, max_radius: f32) -> bool {
        self.crowd = unsafe { mbrc_crowd_init(self.s, max, max_radius) } == 1;
        self.crowd
    }

    pub fn has_crowd(&self) -> bool {
        self.crowd
    }

    /// A new agent at `pos` (snapped to the navmesh near it): its index, or None when the crowd is full.
    pub fn crowd_add(&mut self, pos: [f32; 3], p: &AgentParams) -> Option<i32> {
        if !self.crowd {
            return None;
        }
        let i = unsafe { mbrc_crowd_add(self.s, pos.as_ptr(), p) };
        (i >= 0).then_some(i)
    }

    pub fn crowd_params(&mut self, idx: i32, p: &AgentParams) {
        unsafe { mbrc_crowd_params(self.s, idx, p) }
    }

    pub fn crowd_remove(&mut self, idx: i32) {
        unsafe { mbrc_crowd_remove(self.s, idx) }
    }

    /// Sends an agent to the navmesh's nearest point to `pos` within `ext`.
    pub fn crowd_target(&mut self, idx: i32, pos: [f32; 3], ext: [f32; 3]) -> bool {
        unsafe { mbrc_crowd_target(self.s, idx, pos.as_ptr(), ext.as_ptr()) == 1 }
    }

    pub fn crowd_stop(&mut self, idx: i32) {
        unsafe { mbrc_crowd_stop(self.s, idx) }
    }

    /// One `dtCrowd::update`, by `dt`.
    pub fn crowd_update(&mut self, dt: f32) {
        if self.crowd {
            unsafe { mbrc_crowd_update(self.s, dt) }
        }
    }

    pub fn crowd_agent(&self, idx: i32) -> Agent {
        let mut o = [0f32; 31];
        unsafe { mbrc_crowd_agent(self.s, idx, o.as_mut_ptr()) };
        let v = |i: usize| [o[i], o[i + 1], o[i + 2]];
        Agent {
            pos: v(0),
            vel: v(3),
            desired: v(6),
            walking: o[9] == 1.0,
            target_state: o[10] as u8,
            corners: o[11] as u8,
            next: v(12),
            partial: o[15] != 0.0,
            target: v(16),
            ahead: [v(19), v(22), v(25), v(28)],
        }
    }

    /// Puts an agent back where its body ended (along its corridor, on the navmesh).
    pub fn crowd_place(&mut self, idx: i32, pos: [f32; 3]) {
        unsafe { mbrc_crowd_place(self.s, idx, pos.as_ptr()) }
    }
}

impl Drop for NavMesh {
    fn drop(&mut self) {
        unsafe { mbrc_nav_free(self.s) }
    }
}

#[cfg(test)]
mod tests;
