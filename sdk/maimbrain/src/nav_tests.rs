#[cfg(feature = "nav")]
mod region {
    use crate::gfx3d::terrain::{Heights, NoiseDesc};
    use crate::gfx3d::{Vec3, vec3};
    use crate::nav::*;

    /// S10's kind of level: 64 m of noise hills at 1 m, and 40 trunks.
    pub fn level() -> Geometry {
        let h = Heights::new(Some(&NoiseDesc { seed: 7, octaves: 4, frequency: 0.03, ridged: 0.0, warp: Some(5.0) }), 6.0, &[], 1.0, None);
        let mut g = Geometry::default();
        let mut hs = Vec::new();
        h.fill(-32.0, -32.0, 1.0, 65, &mut hs);
        g.add_heights(-32.0, -32.0, 1.0, 65, &hs);
        let mut rng = crate::Rng::new(3);
        for _ in 0..40 {
            let (x, z) = (rng.range(-28.0, 28.0), rng.range(-28.0, 28.0));
            let r = rng.range(0.25, 0.6);
            let y = h.height(x, z) - 1.0;
            trunk(&mut g, x, y, z, r, 6.0);
        }
        g
    }

    /// An octagonal prism, its sides and cap, and its solid.
    pub fn trunk(g: &mut Geometry, x: f32, y: f32, z: f32, r: f32, tall: f32) {
        let ring: Vec<[f32; 2]> = (0..8).map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            [x + r * a.cos(), z + r * a.sin()]
        }).collect();
        let mut v = Vec::new();
        for p in &ring {
            v.push(vec3(p[0], y, p[1]));
            v.push(vec3(p[0], y + tall, p[1]));
        }
        let mut t = Vec::new();
        for k in 0..8u32 {
            let (a0, a1, b0, b1) = (k * 2, k * 2 + 1, ((k + 1) % 8) * 2, ((k + 1) % 8) * 2 + 1);
            t.extend([a0, b0, b1, a0, b1, a1]);
        }
        for k in 1..7u32 {
            t.extend([1, k * 2 + 1, k * 2 + 3]);
        }
        g.add_mesh(&v, &t, |p| p);
        g.solids.push(Solid { outline: ring, y0: y, y1: y + tall });
    }

    fn region(g: Geometry) -> Region {
        let s = Settings { agent_radius: 0.4, agent_height: 1.6, max_climb: 0.4, ..Settings::default() };
        let mut r = Region::new(&s, vec3(-32.0, -10.0, -32.0), vec3(32.0, 12.0, 32.0)).unwrap();
        assert_eq!(r.set_geometry(g), 16);
        let mut frames = 0;
        while !r.is_baked() {
            assert!(r.bake_step(4) <= 4);
            frames += 1;
        }
        assert_eq!(frames, 4, "16 tiles, 4 a frame");
        r
    }

    #[test]
    fn a_level_bakes_the_same_bytes_and_rebakes_only_what_changed() {
        let g = level();
        let a = region(g.clone());
        let b = region(g.clone());
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.walkable_tiles(), 16);
        // A new trunk in one corner: only the tiles it touches go again.
        let mut a = a;
        let mut g2 = g;
        trunk(&mut g2, -24.0, -10.0, -24.0, 1.5, 30.0);
        assert_eq!(a.set_geometry(g2.clone()), 1);
        a.bake_step(10);
        assert!(a.is_baked());
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.hash(), region(g2).hash(), "a rebake ends where a whole bake does");
    }

    #[test]
    fn paths_go_round_trunks_and_a_crowd_walks_them() {
        let g = level();
        let mut r = region(g.clone());
        let mut path = Vec::new();
        let ends = [(vec3(-26.0, 0.0, -26.0), vec3(26.0, 0.0, 26.0)), (vec3(-26.0, 0.0, 20.0), vec3(24.0, 0.0, -22.0))];
        for (a, b) in ends {
            let a = r.nearest(a + Vec3::Y * 3.0).unwrap();
            let b = r.nearest(b + Vec3::Y * 3.0).unwrap();
            assert_eq!(r.path(a, b, &mut path), Some(true), "{a:?} → {b:?}");
            // No leg passes through a trunk.
            for w in path.windows(2) {
                for s in &g.solids {
                    let c = s.outline.iter().fold([0.0, 0.0], |m, p| [m[0] + p[0] / 8.0, m[1] + p[1] / 8.0]);
                    let rad = ((s.outline[0][0] - c[0]).powi(2) + (s.outline[0][1] - c[1]).powi(2)).sqrt();
                    let (p, q) = (w[0], w[1]);
                    let d = vec3(q.x - p.x, 0.0, q.z - p.z);
                    let t = (((c[0] - p.x) * d.x + (c[1] - p.z) * d.z) / d.dot(d).max(1e-6)).clamp(0.0, 1.0);
                    let near = ((p.x + d.x * t - c[0]).powi(2) + (p.z + d.z * t - c[1]).powi(2)).sqrt();
                    assert!(near > rad, "a path leg passes through a trunk at {c:?}");
                }
            }
        }
        // 20 agents cross to the far side (round hills too steep to climb) within a minute.
        let s = AgentSettings { radius: 0.4, height: 1.6, max_speed: 3.0, ..AgentSettings::default() };
        let mut ids = Vec::new();
        for k in 0..20 {
            let p = r.nearest(vec3(-24.0 + (k % 5) as f32 * 1.5, 3.0, -24.0 + (k / 5) as f32 * 1.5)).unwrap();
            let id = r.add_agent(p, &s).unwrap();
            assert!(r.set_target(id, vec3(22.0, 3.0, 22.0)));
            ids.push(id);
        }
        let mut hash = 2166136261u32;
        for _ in 0..3600 {
            r.update_crowd(1.0 / 60.0);
        }
        let mut arrived = 0;
        for id in &ids {
            let a = r.agent(*id).unwrap();
            for v in a.pos {
                hash = (hash ^ v.to_bits()).wrapping_mul(16777619);
            }
            if vec3(a.pos[0] - 22.0, 0.0, a.pos[2] - 22.0).length() < 4.0 {
                arrived += 1;
            } else {
                eprintln!("{a:?}");
            }
        }
        assert!(arrived >= 18, "{arrived} of 20 got there");
        let _ = hash;
    }
}

#[cfg(feature = "gridpath")]
#[test]
fn grid_paths_take_the_same_cells_every_time() {
    // A wall with a gap; 4- and 8-connected.
    let open = |x: i32, y: i32| !(x == 5 && y != 8);
    let p = crate::nav::grid_path(10, 10, (1, 1), (8, 1), false, open).unwrap();
    assert_eq!(p.first(), Some(&(1, 1)));
    assert_eq!(p.last(), Some(&(8, 1)));
    assert!(p.contains(&(5, 8)));
    assert_eq!(p.len(), 1 + 7 + 14);
    assert_eq!(crate::nav::grid_path(10, 10, (1, 1), (8, 1), false, open), Some(p));
    let d = crate::nav::grid_path(10, 10, (1, 1), (8, 1), true, open).unwrap();
    assert!(d.len() < 22);
    assert_eq!(crate::nav::grid_path(10, 10, (1, 1), (5, 1), false, open), None, "into a wall");
}

/// What a bake and a crowd step cost natively (the C++'s optimisation is
/// build.rs's `OPT`; `MB_RECAST_OPT=s` builds it at -Os to compare):
/// `cargo test --release -p maimbrain --features nav bake_cost -- --ignored --nocapture`.
#[cfg(feature = "nav")]
#[test]
#[ignore]
fn bake_cost() {
    use crate::gfx3d::vec3;
    use crate::nav::*;
    let g = self::region::level();
    let s = Settings { agent_radius: 0.4, agent_height: 1.6, max_climb: 0.4, ..Settings::default() };
    let t = std::time::Instant::now();
    let n = 20;
    let mut last = None;
    for _ in 0..n {
        let mut r = Region::new(&s, vec3(-32.0, -10.0, -32.0), vec3(32.0, 12.0, 32.0)).unwrap();
        r.set_geometry(g.clone());
        while !r.is_baked() {
            r.bake_step(16);
        }
        last = Some(r);
    }
    let bake = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    let mut r = last.unwrap();
    let a = AgentSettings { radius: 0.4, height: 1.6, max_speed: 3.0, ..AgentSettings::default() };
    let mut ids = Vec::new();
    for k in 0..50 {
        let p = r.nearest(vec3(-24.0 + (k % 10) as f32 * 1.5, 3.0, -24.0 + (k / 10) as f32 * 1.5)).unwrap();
        ids.push(r.add_agent(p, &a).unwrap());
    }
    let t = std::time::Instant::now();
    let frames = 1200;
    for f in 0..frames {
        if f % 60 == 0 {
            for (i, id) in ids.iter().enumerate() {
                let x = if (f / 60 + i) % 2 == 0 { 22.0 } else { -22.0 };
                r.set_target(*id, vec3(x, 3.0, 22.0 - (i % 10) as f32 * 4.0));
            }
        }
        r.update_crowd(1.0 / 60.0);
    }
    let crowd = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!("bake of the 64 m level: {bake:.2} ms; 50 agents a frame: {crowd:.3} ms");
}
