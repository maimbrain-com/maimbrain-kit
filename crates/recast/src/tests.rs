use super::*;

/// A 32 m floor with a 4 m pillar in the middle: (verts, tris).
fn level() -> (Vec<f32>, Vec<i32>) {
    let mut v: Vec<f32> = vec![0.0, 0.0, 0.0, 32.0, 0.0, 0.0, 32.0, 0.0, 32.0, 0.0, 0.0, 32.0];
    let mut t: Vec<i32> = vec![0, 2, 1, 0, 3, 2];
    // The pillar: a box 14..18 × 14..18, 3 m tall (its sides and top).
    let b = (v.len() / 3) as i32;
    for (x, z) in [(14.0, 14.0), (18.0, 14.0), (18.0, 18.0), (14.0, 18.0)] {
        v.extend([x, 0.0, z]);
        v.extend([x, 3.0, z]);
    }
    for i in 0..4 {
        let (a0, a1, b0, b1) = (b + i * 2, b + i * 2 + 1, b + ((i + 1) % 4) * 2, b + ((i + 1) % 4) * 2 + 1);
        t.extend([a0, b0, b1, a0, b1, a1]);
    }
    t.extend([b + 1, b + 5, b + 3, b + 1, b + 7, b + 5]);
    (v, t)
}

fn config() -> Config {
    Config {
        cell_size: 0.25,
        cell_height: 0.2,
        agent_height: 1.6,
        agent_radius: 0.4,
        agent_climb: 0.4,
        max_slope_deg: 45.0,
        region_min: 2,
        region_merge: 20,
        edge_max_len: 12.0,
        edge_max_error: 1.3,
        verts_per_poly: 6,
        detail_sample_dist: 6.0,
        detail_sample_max_error: 1.0,
        tile_cells: 64,
        bmin: [0.0, -1.0, 0.0],
        bmax: [32.0, 4.0, 32.0],
        max_tiles: 4,
    }
}

fn fnv(mut h: u32, b: &[u8]) -> u32 {
    for x in b {
        h ^= *x as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

fn bake() -> (NavMesh, u32) {
    let (v, t) = level();
    let mut nav = NavMesh::new(&config()).unwrap();
    let mut h = 2166136261u32;
    for ty in 0..2 {
        for tx in 0..2 {
            let tile = nav.build_tile(&v, &t, &[4.0, 0.0, 3.0, 14.0, 14.0, 18.0, 14.0, 18.0, 18.0, 14.0, 18.0], tx, ty).expect("a walkable tile");
            h = fnv(h, tile.bytes());
            assert!(nav.add_tile(tile, tx, ty));
        }
    }
    (nav, h)
}

#[test]
fn bakes_a_path_around_a_pillar() {
    let (nav, h) = bake();
    let (_, h2) = bake();
    assert_eq!(h, h2, "the same level bakes the same bytes");
    let mut path = Vec::new();
    let whole = nav.path([4.0, 0.0, 16.0], [28.0, 0.0, 16.0], [2.0, 4.0, 2.0], &mut path);
    assert_eq!(whole, Some(true));
    // Around the pillar: a corner off the straight line, outside it.
    assert!(path.len() >= 3, "{path:?}");
    assert!(path.iter().any(|p| (p[2] - 16.0).abs() > 2.0 + 0.3), "{path:?}");
    let inside = nav.nearest([16.0, 0.0, 16.0], [0.5, 1.0, 0.5]);
    assert!(inside.is_none_or(|p| p[1] > 2.5), "nothing walkable inside the pillar (its top is), got {inside:?}");
}

#[test]
fn a_crowd_walks_to_its_target() {
    let (mut nav, _) = bake();
    assert!(nav.crowd_init(8, 0.6));
    let p = AgentParams { radius: 0.4, height: 1.6, max_accel: 8.0, max_speed: 3.0, avoidance: 3, separation: 2.0 };
    let a = nav.crowd_add([4.0, 0.0, 16.0], &p).unwrap();
    let b = nav.crowd_add([28.0, 0.0, 16.0], &p).unwrap();
    assert!(nav.crowd_target(a, [28.0, 0.0, 16.0], [2.0, 4.0, 2.0]));
    assert!(nav.crowd_target(b, [4.0, 0.0, 16.0], [2.0, 4.0, 2.0]));
    for _ in 0..900 {
        nav.crowd_update(1.0 / 60.0);
    }
    let (pa, pb) = (nav.crowd_agent(a), nav.crowd_agent(b));
    assert!(pa.walking && pb.walking);
    assert!((pa.pos[0] - 28.0).abs() < 1.0 && (pb.pos[0] - 4.0).abs() < 1.0, "{pa:?} {pb:?}");
}
