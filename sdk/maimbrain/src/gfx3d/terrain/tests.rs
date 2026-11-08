//! Terrain's checks, natively (where every mb3d constructor answers handle
//! 1): the height hash, a tile's counts and winding, the selection covering
//! the ground once, level swaps with no holes or overlaps, the draws and
//! triangles S11 measured, building within the budget, and `raise`.
//! `cargo test -p maimbrain --release -- --ignored --nocapture terrain` prints the timings.

use super::heights::{Fnv, Heights, NoiseDesc, Stamp};
use super::tile::{self, Key, Layout, Scratch, Tint};
use super::*;
use crate::gfx3d::{Camera, Quat, vec3};

/// The world the hashes are of: every kind of noise and stamp.
fn hash_desc() -> TerrainDesc {
    TerrainDesc {
        noise: Some(NoiseDesc { seed: 7, octaves: 5, frequency: 0.004, ridged: 0.4, warp: None }),
        height: 90.0,
        stamps: vec![
            Stamp::Hill { x: 120.0, z: -80.0, radius: 60.0, height: 18.0 },
            Stamp::Crater { x: -200.0, z: 150.0, radius: 30.0, depth: 12.0, rim: 3.0 },
            Stamp::River { points: vec![[-600.0, 0.0], [0.0, 40.0], [600.0, -20.0]], width: 12.0, bank: 20.0, bed: None },
        ],
        ..TerrainDesc::default()
    }
}

/// 256 × 256 heights over 4 km, and all of one tile's vertices, normals,
/// UVs, colours and indices.
fn hashes() -> (u32, u32) {
    let d = hash_desc();
    let h = Heights::new(d.noise.as_ref(), d.height, &d.stamps, d.detail, d.size);
    let mut f = Fnv::START;
    for j in 0..256 {
        for i in 0..256 {
            f.word(h.height(-2000.0 + i as f32 * 15.7, -2000.0 + j as f32 * 15.7).to_bits());
        }
    }
    let layout = Layout::new(1.0, 400.0, None);
    let m = tile::mesh(&h, &layout, Key { lod: 0, ix: 3, iz: -1 }, &Tint::natural(7, 90.0), &mut Scratch::default());
    (f.0, tile::mesh_hash(&m))
}

/// What these heights hash to everywhere. A change here is a change to
/// every terrain's land: make it on purpose, and say so.
const HEIGHTS: u32 = 0x4104a14e;
/// The tile: its UVs are the slope and hollowness and its colours a tint since the ground shader (mb3d 4).
const MESH: u32 = 0xb24547c7;

#[test]
fn terrain_heights_hash_is_the_same() {
    let (a, b) = hashes();
    println!("terrain hash heights={a:08x} mesh={b:08x}");
    assert_eq!((a, b), hashes(), "twice in one run");
    assert_eq!((a, b), (HEIGHTS, MESH), "the land changed, or this platform's floats differ");
}

/// The noise spans about `height`: its lowest near 0 and, over a big enough
/// piece of it, its highest near `height` (rolling land) or, with ranges,
/// its peaks near `height` with most of the land in the basins well below.
#[test]
fn terrain_height_is_about_the_span() {
    for (ridged, seed) in [(0.0, 1), (0.6, 2), (1.0, 3)] {
        let n = NoiseDesc { seed, ridged, ..NoiseDesc::default() };
        let h = Heights::new(Some(&n), 100.0, &[], 1.0, None);
        let mut v: Vec<f32> = (0..200 * 200).map(|i| h.height((i % 200) as f32 * 25.0, (i / 200) as f32 * 25.0)).collect();
        v.sort_by(f32::total_cmp);
        let (lo, mid, hi, top) = (v[v.len() / 100], v[v.len() / 2], v[v.len() * 99 / 100], v[v.len() - 1]);
        println!("ridged {ridged}: p1 {lo:.1} p50 {mid:.1} p99 {hi:.1} max {top:.1}");
        assert!((-15.0..20.0).contains(&lo), "ridged {ridged}: p1 {lo}");
        if ridged == 0.0 {
            assert!((75.0..122.0).contains(&hi), "rolling: p99 {hi}");
        } else {
            assert!(mid < 0.4 * top && (70.0..160.0).contains(&top), "ranges: p50 {mid}, max {top}");
        }
    }
}

#[test]
fn terrain_tiles_have_the_counts_the_budget_says() {
    let d = hash_desc();
    let h = Heights::new(d.noise.as_ref(), d.height, &d.stamps, 1.0, None);
    let painter = Tint::natural(7, 90.0);
    let l = Layout::new(1.0, 400.0, None);
    for lod in 0..3 {
        let m = tile::mesh(&h, &l, Key { lod, ix: 1, iz: 2 }, &painter, &mut Scratch::default());
        assert_eq!(m.positions.len() as u32, Layout::verts());
        assert_eq!(m.indices.len() as u32 / 3, Layout::tris());
    }
    assert_eq!((Layout::verts(), Layout::tris()), (1221, 2304));
}

/// Every triangle faces outward: the top faces up, a skirt away from its tile.
#[test]
fn terrain_triangles_wind_counter_clockwise_from_outside() {
    let d = hash_desc();
    let h = Heights::new(d.noise.as_ref(), d.height, &d.stamps, 1.0, None);
    let painter = Tint::natural(7, 90.0);
    for lod in 0..3 {
        let m = tile::mesh(&h, &Layout::new(1.0, 400.0, None), Key { lod, ix: 0, iz: 0 }, &painter, &mut Scratch::default());
        for t in m.indices.chunks_exact(3) {
            let [a, b, c] = [m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]];
            let n = (b - a).cross(c - a);
            let out = (a + b + c) * (1.0 / 3.0);
            if n.y.abs() < 1e-3 {
                assert!(n.x * out.x + n.z * out.z > 0.0, "a skirt faces in: {t:?}");
            } else {
                assert!(n.y > 0.0, "a top faces down: {t:?}");
            }
        }
    }
}

/// The wanted tiles cover the disc exactly once: no holes, no overlaps.
#[test]
fn terrain_selection_covers_the_ground_once() {
    for (detail, reach) in [(1.0, 400.0), (2.0, 800.0), (0.5, 150.0), (4.0, 1600.0)] {
        let l = Layout::new(detail, reach, None);
        for (cx, cz) in [(10.0, 20.0), (-333.0, 75.0), (640.0, -641.0)] {
            let cells = cover_cells(&l, cx, cz);
            let unit = l.chunk();
            let r = (reach * 0.9 / unit) as i32;
            let (ci, ck) = ((cx / unit).floor() as i32, (cz / unit).floor() as i32);
            for b in -r..=r {
                for a in -r..=r {
                    if ((a * a + b * b) as f32).sqrt() * unit < reach * 0.85 {
                        assert!(cells.contains_key(&(ci + a, ck + b)), "hole at {a},{b} in {l:?}");
                    }
                }
            }
        }
    }
}

/// Level-0 cells and the tile covering each, failing on any covered twice.
fn cover_cells(l: &Layout, cx: f32, cz: f32) -> std::collections::BTreeMap<(i32, i32), Key> {
    let mut out = Vec::new();
    l.select(cx, cz, &mut out);
    let unit = l.chunk();
    let mut cells = std::collections::BTreeMap::new();
    for (k, _) in &out {
        let (x0, z0, s) = l.rect(*k);
        let n = (s / unit) as i32;
        for b in 0..n {
            for a in 0..n {
                let cell = ((x0 / unit).round() as i32 + a, (z0 / unit).round() as i32 + b);
                assert!(cells.insert(cell, *k).is_none(), "{cell:?} twice");
            }
        }
    }
    cells
}

/// A world with an edge wants only tiles over it.
#[test]
fn terrain_with_a_size_ends() {
    let l = Layout::new(1.0, 400.0, Some(64.0));
    let mut out = Vec::new();
    l.select(0.0, 0.0, &mut out);
    for (k, _) in &out {
        let (x0, z0, s) = l.rect(*k);
        assert!(x0 < 64.0 && x0 + s > -64.0 && z0 < 64.0 && z0 + s > -64.0, "{k:?} is past the edge");
    }
    let h = Heights::new(Some(&NoiseDesc::default()), 30.0, &[], 1.0, Some(128.0));
    assert_eq!(h.height(500.0, 10.0), h.height(64.0, 10.0), "past the edge is the edge");
}

/// The shown tiles cover the ground under the camera once, frame after
/// frame, while the camera flies at 40 m/s (levels swapping all the way):
/// no hole near the camera, never two levels in one place.
#[test]
fn terrain_level_swaps_leave_no_holes_or_overlaps() {
    let mut t = Terrain::new(TerrainDesc { reach: 300.0, ..hash_desc() });
    let (mut x, z) = (0.0f32, 0.0f32);
    let mut built_at = None;
    for frame in 0..900 {
        if t.update(vec3(x, 0.0, z)) {
            built_at = Some(frame);
        }
        if t.built() {
            // Shown tiles: no level-0 cell twice.
            let unit = t.layout().chunk();
            let mut cells = std::collections::BTreeSet::new();
            for (k, shown) in t.tiles() {
                if !shown {
                    continue;
                }
                let (x0, z0, s) = t.layout().rect(k);
                let n = (s / unit) as i32;
                for b in 0..n {
                    for a in 0..n {
                        assert!(cells.insert(((x0 / unit).round() as i32 + a, (z0 / unit).round() as i32 + b)), "frame {frame}: two tiles over one place");
                    }
                }
            }
            assert_eq!(t.holes(t.layout().reach * 0.75), 0, "frame {frame}: a hole");
        }
        x += 40.0 / 60.0;
    }
    assert!(built_at.is_some_and(|f| f < 12), "the first view took {built_at:?} frames");
    assert_eq!(t.refused, 0);
}

/// Building: several tiles a frame until the first view is built, then
/// at most one a frame while moving, nearest first; progress goes 0 → 1.
#[test]
fn terrain_builds_within_its_budget() {
    let mut t = Terrain::new(hash_desc());
    let mut last = -1.0;
    let mut frames = 0;
    while !t.built() {
        t.update(Vec3::ZERO);
        assert!(t.made_last <= FAST_VERTS / Layout::verts());
        assert!(t.progress() >= last);
        last = t.progress();
        frames += 1;
        assert!(frames < 30);
    }
    assert_eq!(t.progress(), 1.0);
    let first = t.made;
    assert!((50..=90).contains(&first), "{first} tiles in the first view at 400 m");
    // Walking: one a frame.
    for f in 0..600 {
        t.update(vec3(f as f32 * 0.1, 0.0, 0.0));
        assert!(t.made_last <= 1, "frame {f}: {} tiles", t.made_last);
    }
    // A jump far away: holes, so fast again until they're filled.
    t.update(vec3(5000.0, 0.0, 0.0));
    assert!(t.made_last > 1);
}

/// `raise` changes the true ground at once and the tiles under it next update.
#[test]
fn terrain_raise_changes_the_ground_and_its_tiles() {
    let mut t = Terrain::new(hash_desc());
    while !t.built() {
        t.update(Vec3::ZERO);
    }
    let before = t.height_at(10.0, 10.0);
    t.raise(10.0, 10.0, 6.0, 3.0);
    assert!((t.height_at(10.0, 10.0) - before - 3.0).abs() < 1e-3);
    assert!((t.height_at(40.0, 40.0) - Terrain::new(hash_desc()).height_at(40.0, 40.0)).abs() < 1e-6, "far away, untouched");
    let made = t.made;
    t.update(Vec3::ZERO);
    assert!(t.made > made, "the tile under it was built again");
    t.restart();
    assert_eq!(t.height_at(10.0, 10.0), before);
    assert!(!t.built());
    t.update(Vec3::ZERO);
    assert!(t.built(), "a restarted terrain over the same view is built at once");
}

/// What a phone held sideways (16:9, 60° high) sees at most while driving at
/// 40 m/s: S11's `t400v` (30 draws, 69k triangles) and `T800v` (29, 67k).
#[test]
fn terrain_draws_what_s11_measured() {
    for (detail, reach, draws, ktris) in [(1.0, 400.0, 30, 69), (2.0, 800.0, 29, 67)] {
        let mut t = Terrain::new(TerrainDesc { detail, reach, ..hash_desc() });
        let (mut peak_d, mut peak_t) = (0, 0);
        for f in 0..900 {
            let a = f as f32 * 40.0 / 60.0 / 1500.0;
            let (x, z) = (1500.0 * a.sin(), 1500.0 * (1.0 - a.cos()));
            let eye = vec3(x, t.height_at(x, z) + 10.0, z);
            t.update(eye);
            let ahead = vec3(1500.0 * (a + 0.06).sin(), eye.y - 3.0, 1500.0 * (1.0 - (a + 0.06).cos()));
            let mut cam = Camera::looking_at(eye, ahead);
            cam.fov_y = 60f32.to_radians();
            cam.far = reach * 1.25;
            let s = t.seen(&cam, 16.0 / 9.0);
            peak_d = peak_d.max(s.draws);
            peak_t = peak_t.max(s.tris);
        }
        println!("detail {detail} reach {reach}: {peak_d} draws, {} k triangles", peak_t / 1000);
        assert!(peak_d.abs_diff(draws) <= 4, "{peak_d} draws against S11's {draws}");
        assert!((peak_t / 1000).abs_diff(ktris) <= 9, "{peak_t} triangles against S11's {ktris}k");
        assert!((triangles_at(reach / detail) / 1000.0 - ktris as f32).abs() < 8.0);
    }
    let _ = Quat::IDENTITY;
}

#[test]
fn terrain_checks_say_what_to_do() {
    assert_eq!(TerrainDesc::default().check(), None);
    let e = TerrainDesc { reach: 1600.0, ..TerrainDesc::default() }.check().unwrap();
    assert_eq!(e, "reach 1600 at detail 1 is about 390k triangles of the phone's 150k; use detail = 2 or reach = 800");
    assert!(TerrainDesc { detail: 3.0, ..TerrainDesc::default() }.check().unwrap().contains("0.5, 1, 2 or 4"));
    assert!(TerrainDesc { reach: 800.0, detail: 2.0, ..TerrainDesc::default() }.check().is_none());
    assert!(TerrainDesc { reach: 4000.0, detail: 4.0, ..TerrainDesc::default() }.check().unwrap().ends_with("use reach = 3200 (with detail = 4)"));
}

/// A tile carries what the ground shader's rules read: each vertex's slope
/// and hollowness in its UV, the same at every level where levels share a
/// point, and a tint near neutral as its colour (exactly neutral for `paint`).
#[test]
fn terrain_tiles_carry_slope_hollowness_and_a_tint() {
    let d = hash_desc();
    let h = Heights::new(d.noise.as_ref(), d.height, &d.stamps, d.detail, d.size);
    let layout = Layout::new(1.0, 400.0, None);
    let near = tile::mesh(&h, &layout, Key { lod: 0, ix: 0, iz: 0 }, &Tint::natural(7, 90.0), &mut Scratch::default());
    let far = tile::mesh(&h, &layout, Key { lod: 2, ix: 0, iz: 0 }, &Tint::None, &mut Scratch::default());
    // The level-2 tile's corner vertex is the level-0 tile's: the same slope and hollowness.
    let (a, b) = (near.uvs[0], far.uvs[0]);
    assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5, "{a:?} {b:?}");
    for (uv, n) in near.uvs.iter().zip(&near.normals).take(33 * 33) {
        assert!((0.0..1.0).contains(&uv[0]) && uv[1].abs() < 2.0 && n.y > 0.0);
    }
    assert!(far.colors.iter().all(|&c| c == 0xbcbcbcff), "paint gets no tint");
    let grey = |c: u32| [(c >> 24) & 255, (c >> 16) & 255, (c >> 8) & 255];
    assert!(near.colors.iter().all(|&c| grey(c).iter().all(|v| v.abs_diff(188) < 40)), "the natural tint stays a tint: within a fifth of neutral");
}

/// What a tile and a sample cost natively (release).
#[test]
#[ignore]
fn terrain_timing() {
    use std::time::Instant;
    let d = hash_desc();
    let h = Heights::new(d.noise.as_ref(), d.height, &d.stamps, 1.0, None);
    let n = 1_000_000;
    let t = Instant::now();
    let mut s = 0.0;
    for i in 0..n {
        s += h.height(i as f32 * 0.37, i as f32 * 0.11);
    }
    println!("height(): {:.0} ns ({s})", t.elapsed().as_nanos() as f64 / n as f64);
    let painter = Tint::natural(7, 90.0);
    let l = Layout::new(1.0, 400.0, None);
    let mut sc = Scratch::default();
    let t = Instant::now();
    for i in 0..200 {
        let _ = tile::mesh(&h, &l, Key { lod: 0, ix: i, iz: 3 }, &painter, &mut sc);
    }
    println!("a level-0 tile: {:.0} µs", t.elapsed().as_secs_f64() * 1e6 / 200.0);
}

/// How steep the default land is: slope (`1 − normal.y`) percentiles over
/// 4 km at 2 m, and the share past 60° (a wall a heightmap can't paint).
#[test]
#[ignore]
fn terrain_default_slopes() {
    for seed in [1, 11] {
        let d = TerrainDesc { noise: Some(NoiseDesc { seed, ..NoiseDesc::default() }), ..TerrainDesc::default() };
        let h = Heights::new(d.noise.as_ref(), d.height, &[], 1.0, None);
        let mut s: Vec<f32> = (0..400 * 400).map(|i| 1.0 - h.normal((i % 400) as f32 * 10.0 - 2000.0, (i / 400) as f32 * 10.0 - 2000.0, 2.0)[1]).collect();
        s.sort_by(f32::total_cmp);
        let past = |k: f32| s.iter().filter(|x| **x > k).count() as f32 / s.len() as f32 * 100.0;
        println!("seed {seed}: p50 {:.3} p95 {:.3} p99 {:.3}; past 45° {:.1}%, past 60° {:.2}%", s[s.len() / 2], s[s.len() * 95 / 100], s[s.len() * 99 / 100], past(0.293), past(0.5));
    }
}

/// A river cut deep into a hillside makes a valley a figure can walk down,
/// not a cliff: where the stamp lowered gentle land within 30 m of its bed,
/// the sides stay under about 40° (Hill Walk's river, 27 m into the hill
/// the hauler walks down, meandering).
#[test]
fn terrain_river_banks_are_walkable() {
    let noise = NoiseDesc { seed: 11, ..NoiseDesc::default() };
    let river = Stamp::River { points: vec![[-300.0, -60.0], [-40.0, -80.0], [200.0, -40.0]], width: 8.0, bank: 18.0, bed: None };
    let (with, without) = (Heights::new(Some(&noise), 150.0, &[river], 1.0, None), Heights::new(Some(&noise), 150.0, &[], 1.0, None));
    let bed = (0..21 * 21).map(|i| with.height(-50.0 + (i % 21) as f32, -90.0 + (i / 21) as f32)).fold(f32::MAX, f32::min);
    let mut s = Vec::new();
    for j in 0..240 {
        for i in 0..400 {
            let (x, z) = (-200.0 + i as f32, -200.0 + j as f32);
            // Only the cut, where the land was gentle and within 30 m of the bed.
            let land = without.height(x, z);
            if land - with.height(x, z) > 0.5 && land - bed < 30.0 && without.normal(x, z, 1.0)[1] > 0.9 {
                s.push(1.0 - with.normal(x, z, 1.0)[1]);
            }
        }
    }
    s.sort_by(f32::total_cmp);
    let (p99, top) = (s[s.len() * 99 / 100], s[s.len() - 1]);
    let deg = |k: f32| (1.0 - k).acos().to_degrees();
    println!("river banks: {} points, p99 {:.0}°, steepest {:.0}°", s.len(), deg(p99), deg(top));
    assert!(deg(p99) < 40.0, "river banks: p99 {:.0}°", deg(p99));
}
