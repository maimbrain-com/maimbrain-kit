use super::*;

const DT: f32 = 1.0 / 60.0;

fn ground(w: &mut World) -> BodyId {
    w.add(Body::fixed(180.0, 620.0), Shape::rect(360.0, 40.0))
}

/// Deterministic dt jitter like a real display (recorded, so a replay sees the same).
fn jitter_dt(i: u32) -> f32 {
    let x = (i.wrapping_mul(2654435761) >> 16) as f32 / 65536.0;
    1.0 / 60.0 + (x - 0.5) * 0.004
}

/// A busy scene: a pyramid, a pile of mixed shapes, a pendulum chain, a
/// spring, a motorized wheel, a kinematic paddle and a sensor.
fn busy_scene() -> (World, Vec<BodyId>) {
    let mut w = World::new();
    ground(&mut w);
    w.add(Body::fixed(0.0, 320.0), Shape::rect(20.0, 640.0));
    w.add(Body::fixed(360.0, 320.0), Shape::rect(20.0, 640.0));
    let mut ids = Vec::new();
    for row in 0..8 {
        for col in 0..(8 - row) {
            let x = 60.0 + col as f32 * 31.0 + row as f32 * 15.5;
            let y = 585.0 - row as f32 * 30.5;
            ids.push(w.add(Body::dynamic(x, y), Shape::rect(30.0, 30.0).friction(0.7)));
        }
    }
    for i in 0..12 {
        let x = 40.0 + (i * 23 % 280) as f32;
        let y = 40.0 + (i * 17 % 120) as f32;
        let s = match i % 4 {
            0 => Shape::circle(10.0).restitution(0.5),
            1 => Shape::capsule(7.0, 28.0),
            2 => Shape::regular_polygon(5, 12.0).density(2.0),
            _ => Shape::round_rect(24.0, 16.0, 4.0),
        };
        ids.push(w.add(Body::dynamic(x, y).angle(i as f32 * 0.3), s));
    }
    let anchor = w.add(Body::fixed(300.0, 60.0), Shape::circle(4.0));
    let mut prev = anchor;
    for k in 0..4 {
        let link = w.add(Body::dynamic(300.0, 80.0 + k as f32 * 20.0), Shape::rect(6.0, 20.0));
        w.join(Joint::revolute(prev, link, vec2(300.0, 70.0 + k as f32 * 20.0)));
        prev = link;
        ids.push(link);
    }
    let bob = w.add(Body::dynamic(80.0, 200.0), Shape::circle(14.0));
    w.join(Joint::spring(anchor, bob, vec2(300.0, 60.0), vec2(80.0, 200.0)).stiffness(1.5, 0.1));
    ids.push(bob);
    let hub = w.add(Body::fixed(200.0, 300.0), Shape::circle(3.0).sensor());
    let wheel = w.add(Body::dynamic(200.0, 300.0), Shape::regular_polygon(6, 30.0));
    w.join(Joint::revolute(hub, wheel, vec2(200.0, 300.0)).motor(4.0, 1.0e6));
    ids.push(wheel);
    let paddle = w.add(Body::kinematic(180.0, 450.0), Shape::rect(80.0, 10.0));
    ids.push(paddle);
    w.add(Body::fixed(180.0, 500.0), Shape::rect(300.0, 60.0).sensor());
    (w, ids)
}

/// Runs the busy scene with scripted "input" and returns the state hash
/// after every frame plus a log of every event.
fn run_busy(frames: u32) -> (Vec<u64>, Vec<String>) {
    let (mut w, ids) = busy_scene();
    let paddle = *ids.last().unwrap();
    let mut hashes = Vec::new();
    let mut log = Vec::new();
    for i in 0..frames {
        let t = i as f32 / 60.0;
        w.move_kinematic(paddle, vec2(180.0 + 100.0 * (t * 1.3).sin(), 450.0), 0.3 * (t * 0.7).sin());
        if i % 97 == 13 {
            let target = ids[(i as usize * 7) % (ids.len() - 1)];
            w.apply_impulse(target, vec2(((i % 5) as f32 - 2.0) * 40.0, -80.0));
        }
        if i % 3 == 0 {
            w.apply_force(ids[3], vec2(0.0, -30.0));
        }
        w.step(jitter_dt(i));
        for e in w.events() {
            log.push(format!("{i} {e:?}"));
        }
        hashes.push(w.state_hash());
    }
    (hashes, log)
}

#[test]
fn identical_inputs_give_bit_identical_worlds() {
    let (h1, e1) = run_busy(3000);
    let (h2, e2) = run_busy(3000);
    assert_eq!(h1.len(), 3000);
    for (i, (a, b)) in h1.iter().zip(&h2).enumerate() {
        assert_eq!(a, b, "worlds diverged at frame {i}");
    }
    assert_eq!(e1, e2, "event streams differ");
    assert!(e1.len() > 50, "the scene should be busy: {} events", e1.len());
    // And the scene actually moves (a hash that never changes proves nothing).
    let distinct: std::collections::BTreeSet<_> = h1.iter().collect();
    assert!(distinct.len() > 1000, "{} distinct states", distinct.len());
}

#[test]
fn identical_after_thousands_of_steps_with_every_body_compared() {
    let dump = |w: &World| -> Vec<u32> {
        w.bodies()
            .flat_map(|b| {
                let p = w.pose(b);
                let v = w.velocity(b);
                [p.pos.x, p.pos.y, p.angle, v.x, v.y, w.spin(b)].map(f32::to_bits)
            })
            .collect()
    };
    let run = || {
        let (mut w, _) = busy_scene();
        for i in 0..5000 {
            w.step(jitter_dt(i));
        }
        (w.steps(), dump(&w))
    };
    let (s1, d1) = run();
    let (s2, d2) = run();
    assert_eq!(s1, s2);
    assert!(s1 >= 4950, "{s1} steps");
    assert_eq!(d1, d2);
}

#[test]
fn free_fall_matches_gravity_in_pixels() {
    let mut w = World::new();
    let b = w.add(Body::dynamic(100.0, 0.0), Shape::circle(5.0));
    for _ in 0..60 {
        w.step(DT);
    }
    let y = w.position(b).y;
    // ½·g·t² = 490.5 px after 1 s (semi-implicit Euler lands a step's worth further).
    assert!((y - 490.5).abs() < 12.0, "fell {y} px");
    assert!((w.velocity(b).y - GRAVITY).abs() < 1.0);
}

#[test]
fn step_counts_follow_accumulated_time() {
    let mut w = World::new();
    let n: u32 = (0..120).map(|_| w.step(1.0 / 120.0)).sum();
    assert_eq!(n, 60);
    assert_eq!(w.steps(), 60);
    assert_eq!(w.step(0.1), 6);
}

#[test]
fn a_stack_settles_and_sleeps() {
    let mut w = World::new();
    ground(&mut w);
    let boxes: Vec<_> = (0..6).map(|i| w.add(Body::dynamic(180.0, 580.0 - i as f32 * 41.0), Shape::rect(40.0, 40.0))).collect();
    for _ in 0..600 {
        w.step(DT);
    }
    assert_eq!(w.awake_count(), 0, "still awake after 10 s");
    for (i, &b) in boxes.iter().enumerate() {
        let p = w.position(b);
        assert!((p.x - 180.0).abs() < 1.0 && (p.y - (580.0 - i as f32 * 40.0)).abs() < 2.0, "box {i} at {p:?}");
        assert!(w.is_sleeping(b));
    }
    // A hit wakes it up.
    w.apply_impulse(boxes[5], vec2(5.0, 0.0));
    w.step(DT);
    assert!(w.awake_count() > 0);
}

#[test]
fn contact_begin_reports_speed_impulse_and_normal() {
    let mut w = World::new();
    let g = ground(&mut w);
    let ball = w.add(Body::dynamic(180.0, 400.0), Shape::circle(10.0));
    let mut hit = None;
    for _ in 0..120 {
        w.step(DT);
        for e in w.events() {
            if let Event::ContactBegin(c) = *e {
                hit.get_or_insert(c);
            }
        }
    }
    let c = hit.expect("no contact");
    assert!(c.involves(ball) && c.involves(g) && c.other(ball) == Some(g));
    // Dropped from 190 px above the ground's top: v = √(2·g·h) ≈ 610 px/s.
    assert!((c.speed - 610.0).abs() < 40.0, "speed {}", c.speed);
    assert!(c.impulse > 0.0);
    assert!((c.point.y - 600.0).abs() < 8.0 && (c.point.x - 180.0).abs() < 1.0, "point {:?}", c.point);
    // From a to b.
    let towards_b = w.position(c.b) - w.position(c.a);
    assert!(c.normal.dot(towards_b) > 0.0, "normal {:?}", c.normal);
    assert!((c.normal.length() - 1.0).abs() < 1e-3);
}

#[test]
fn sensors_report_enter_and_exit_without_pushing() {
    let mut w = World::new();
    let zone = w.add(Body::fixed(180.0, 300.0), Shape::rect(200.0, 40.0).sensor());
    let ball = w.add(Body::dynamic(180.0, 200.0), Shape::circle(10.0));
    let (mut enter, mut exit) = (0, 0);
    for _ in 0..90 {
        w.step(DT);
        for e in w.events() {
            match *e {
                Event::SensorEnter { sensor, other, .. } => {
                    assert_eq!((sensor, other), (zone, ball));
                    enter += 1;
                    assert_eq!(w.overlapping(zone), vec![ball]);
                }
                Event::SensorExit { sensor, other, .. } => {
                    assert_eq!((sensor, other), (zone, ball));
                    exit += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!((enter, exit), (1, 1));
    assert!(w.position(ball).y > 400.0, "the sensor stopped the ball");
}

#[test]
fn raycasts_and_point_queries() {
    let mut w = World::new();
    let g = ground(&mut w);
    let b = w.add(Body::dynamic(100.0, 300.0), Shape::rect(40.0, 40.0));
    // Queries see new bodies at once, before any step.
    assert_eq!(w.body_at(vec2(100.0, 300.0)), Some(b));
    w.step(DT);
    let hit = w.raycast(vec2(250.0, 0.0), vec2(250.0, 640.0)).expect("hit the ground");
    assert_eq!(hit.body, g);
    assert!((hit.point.y - 600.0).abs() < 0.5 && (hit.distance - 600.0).abs() < 0.5);
    assert!((hit.normal.y + 1.0).abs() < 1e-3, "normal faces up the screen: {:?}", hit.normal);
    let y = w.position(b).y;
    assert_eq!(w.raycast(vec2(0.0, y), vec2(360.0, y)).map(|h| h.body), Some(b));
    assert_eq!(w.raycast_ignoring(vec2(0.0, y), vec2(360.0, y), b).map(|h| h.body), None);
    assert_eq!(w.body_at(w.position(b)), Some(b));
    assert_eq!(w.bodies_at(vec2(10.0, 10.0)), vec![]);
    assert!(w.bodies_in_rect(vec2(0.0, 590.0), vec2(50.0, 610.0)).contains(&g));
}

#[test]
fn joints_hold() {
    let mut w = World::new();
    let pin = w.add(Body::fixed(180.0, 100.0), Shape::circle(3.0));
    let bob = w.add(Body::dynamic(260.0, 100.0), Shape::circle(10.0));
    w.join(Joint::revolute(pin, bob, vec2(180.0, 100.0)));
    let rope_end = w.add(Body::dynamic(100.0, 150.0), Shape::circle(5.0));
    w.join(Joint::rope(pin, rope_end, vec2(180.0, 100.0), vec2(100.0, 150.0)));
    let a = w.add(Body::dynamic(60.0, 300.0), Shape::rect(20.0, 20.0));
    let b = w.add(Body::dynamic(80.0, 300.0).angle(0.5), Shape::rect(20.0, 20.0));
    let weld = w.join(Joint::fixed(a, b));
    let slider_base = w.add(Body::fixed(300.0, 400.0), Shape::circle(2.0));
    let slider = w.add(Body::dynamic(300.0, 400.0), Shape::rect(10.0, 10.0));
    w.join(Joint::prismatic(slider_base, slider, vec2(300.0, 400.0), vec2(0.0, 1.0)).limits(-20.0, 50.0));
    let max_rope = vec2(180.0, 100.0).distance(vec2(100.0, 150.0));
    let rel0 = w.angle(b) - w.angle(a);
    for _ in 0..180 {
        w.step(DT);
        assert!((w.position(bob).distance(vec2(180.0, 100.0)) - 80.0).abs() < 1.5);
        assert!(w.position(rope_end).distance(vec2(180.0, 100.0)) < max_rope + 1.5);
    }
    assert!((w.angle(b) - w.angle(a) - rel0).abs() < 0.02, "the weld turned");
    assert!((w.position(b) - w.position(a)).length() - 20.0 < 0.5);
    assert!(w.joint_force(weld) >= 0.0);
    let s = w.position(slider);
    assert!((s.x - 300.0).abs() < 0.5 && (s.y - 450.0).abs() < 1.5, "slider at {s:?}");
    w.unjoin(weld);
    assert!(!w.has_joint(weld));
}

#[test]
fn a_motor_turns_and_a_spring_rests_at_its_length() {
    let mut w = World::with_gravity(Vec2::ZERO);
    let hub = w.add(Body::fixed(100.0, 100.0), Shape::circle(2.0));
    let wheel = w.add(Body::dynamic(100.0, 100.0), Shape::circle(20.0));
    let m = w.join(Joint::revolute(hub, wheel, vec2(100.0, 100.0)).motor(3.0, f32::INFINITY));
    let bob = w.add(Body::dynamic(300.0, 100.0), Shape::circle(5.0));
    w.join(Joint::spring(hub, bob, vec2(100.0, 100.0), vec2(300.0, 100.0)).length(150.0).stiffness(2.0, 0.7));
    for _ in 0..240 {
        w.step(DT);
    }
    assert!((w.spin(wheel) - 3.0).abs() < 0.05, "spin {}", w.spin(wheel));
    assert!((w.position(bob).distance(vec2(100.0, 100.0)) - 150.0).abs() < 2.0, "{:?}", w.position(bob));
    w.set_motor(m, -2.0, f32::INFINITY);
    for _ in 0..60 {
        w.step(DT);
    }
    assert!((w.spin(wheel) + 2.0).abs() < 0.05);
}

#[test]
fn forces_are_frame_rate_independent() {
    let push = |frame_dt: f32| {
        let mut w = World::with_gravity(Vec2::ZERO);
        let b = w.add(Body::dynamic(0.0, 0.0), Shape::rect(100.0, 100.0)); // 1 kg
        let frames = (1.0 / frame_dt).round() as u32;
        for _ in 0..frames {
            w.apply_force(b, vec2(100.0, 0.0)); // 100 px/s² on 1 kg
            w.step(frame_dt);
        }
        w.velocity(b).x
    };
    let (v60, v120, v30) = (push(1.0 / 60.0), push(1.0 / 120.0), push(1.0 / 30.0));
    assert!((v60 - 100.0).abs() < 0.5, "{v60}");
    assert!((v120 - v60).abs() < 1.0, "{v120} vs {v60}");
    assert!((v30 - v60).abs() < 1.0, "{v30} vs {v60}");
    let mut w = World::with_gravity(Vec2::ZERO);
    let b = w.add(Body::dynamic(0.0, 0.0), Shape::rect(100.0, 100.0));
    assert!((w.mass(b) - 1.0).abs() < 1e-4);
    w.apply_impulse(b, vec2(0.0, 50.0));
    w.add_velocity(b, vec2(10.0, 0.0));
    assert!((w.velocity(b) - vec2(10.0, 50.0)).length() < 1e-3);
}

#[test]
fn kinematic_bodies_follow_targets_and_push() {
    let mut w = World::new();
    ground(&mut w);
    let paddle = w.add(Body::kinematic(100.0, 560.0), Shape::rect(40.0, 40.0));
    let crate_ = w.add(Body::dynamic(160.0, 580.0), Shape::rect(40.0, 40.0));
    for i in 0..120 {
        w.move_kinematic(paddle, vec2(100.0 + i as f32 * 1.5, 560.0), 0.0);
        w.step(DT);
    }
    assert!((w.position(paddle).x - 280.0).abs() < 2.0, "{:?}", w.position(paddle));
    assert!(w.position(crate_).x > 290.0, "the crate was pushed: {:?}", w.position(crate_));
    // Reads its speed between steps, and stops when you stop.
    assert!((w.velocity(paddle).x - 90.0).abs() < 1.0, "{:?}", w.velocity(paddle));
    w.step(DT);
    assert_eq!(w.velocity(paddle), Vec2::ZERO);
}

#[test]
fn removing_and_freezing() {
    let mut w = World::new();
    let g = ground(&mut w);
    let b = w.add(Body::dynamic(180.0, 570.0).tag(42), Shape::rect(40.0, 40.0));
    for _ in 0..30 {
        w.step(DT);
    }
    assert_eq!(w.tag(b), 42);
    assert_eq!(w.contacts(b), vec![g]);
    assert!(w.touching(b, g));
    w.set_kind(b, BodyKind::Fixed);
    assert_eq!(w.kind(b), BodyKind::Fixed);
    w.remove(b);
    assert!(!w.contains(b));
    for _ in 0..5 {
        w.step(DT);
        assert!(w.events().iter().all(|e| !e.involves(b)));
    }
    assert_eq!(w.position(b), Vec2::ZERO, "stale ids read as defaults");
    w.apply_impulse(b, vec2(1.0, 1.0)); // no panic
    assert_eq!(w.len(), 1);
}

#[test]
fn draw_pose_interpolates_between_steps() {
    let mut w = World::with_gravity(Vec2::ZERO);
    let b = w.add(Body::dynamic(0.0, 0.0).velocity(600.0, 0.0), Shape::circle(5.0));
    w.step(DT);
    let after_one = w.position(b).x;
    w.step(DT / 2.0); // half a step: nothing simulated, draw halfway back
    assert_eq!(w.position(b).x, after_one);
    let drawn = w.draw_pose(b).pos.x;
    assert!((drawn - (after_one - 5.0)).abs() < 0.01, "drawn at {drawn}, sim at {after_one}");
    w.set_interpolation(false);
    assert_eq!(w.draw_pose(b).pos.x, after_one);
    w.set_interpolation(true);
    w.set_position(b, vec2(50.0, 50.0));
    assert_eq!(w.draw_pose(b).pos, vec2(50.0, 50.0), "teleports don't smear");
}

#[test]
fn compound_and_mesh_shapes() {
    let mut w = World::new();
    let floor = [vec2(0.0, 500.0), vec2(120.0, 560.0), vec2(240.0, 560.0), vec2(360.0, 500.0)];
    w.add(Body::fixed(0.0, 0.0), Shape::polyline(&floor));
    let tri = w.add(
        Body::fixed(0.0, 0.0),
        Shape::trimesh(&[vec2(0.0, 640.0), vec2(360.0, 640.0), vec2(180.0, 600.0)], &[[0, 1, 2]]),
    );
    let l = w.add_body(Body::dynamic(180.0, 300.0));
    w.add_shape(l, Shape::rect(60.0, 20.0));
    w.add_shape(l, Shape::rect(20.0, 60.0).at(-20.0, -20.0));
    assert_eq!(w.shapes(l).len(), 2);
    assert!((w.mass(l) - (0.12 + 0.12)).abs() < 1e-3, "mass {}", w.mass(l));
    let bad = w.add(Body::dynamic(50.0, 50.0), Shape::convex(&[vec2(0.0, 0.0), vec2(1.0, 1.0)]));
    assert!(w.contains(bad), "degenerate shapes fall back instead of panicking");
    for _ in 0..240 {
        w.step(DT);
    }
    assert!(w.position(l).y < 560.0 && w.position(l).y > 450.0, "rests in the valley: {:?}", w.position(l));
    assert!(w.contains(tri));
}

#[test]
fn collision_groups_filter() {
    let mut w = World::new();
    ground(&mut w);
    let ghost = w.add(Body::dynamic(180.0, 500.0), Shape::circle(10.0).groups(0b10, 0b10));
    let lander = w.add(Body::dynamic(100.0, 500.0), Shape::circle(10.0).groups(0b10, 0b01));
    for _ in 0..90 {
        w.step(DT);
    }
    assert!(w.position(ghost).y > 640.0, "ghost fell through the ground");
    assert!((w.position(lander).y - 590.0).abs() < 2.0, "lander landed: {:?}", w.position(lander));
}

/// Ground (top at y = 600), a one-way ledge (top at y = 500, x 100–260)
/// and a wall at x = 330.
fn platforms(w: &mut World) {
    ground(w);
    w.add(Body::fixed(180.0, 505.0), Shape::rect(160.0, 10.0).one_way());
    w.add(Body::fixed(340.0, 400.0), Shape::rect(20.0, 400.0));
}

#[test]
fn a_2d_character_jumps_up_through_a_one_way_ledge_lands_on_it_and_drops_through() {
    let mut w = World::new();
    platforms(&mut w);
    let mut c = Character::new(&mut w, vec2(180.0, 570.0), Shape::rect(20.0, 40.0), CharacterSettings::default(), 0);
    let mut v = Vec2::ZERO;
    let step = |w: &mut World, c: &mut Character, v: &mut Vec2, jump: bool, drop: bool| {
        if jump && c.on_floor() {
            v.y = -700.0;
        }
        v.y += 1800.0 * DT;
        *v = c.move_and_slide(w, *v, DT, drop);
        w.step(DT);
    };
    for _ in 0..30 {
        step(&mut w, &mut c, &mut v, false, false);
    }
    assert!(c.on_floor() && (c.position().y - 580.0).abs() < 1.5, "on the ground: {:?}", c.position());
    // Up through the ledge from below, and down onto it.
    step(&mut w, &mut c, &mut v, true, false);
    for _ in 0..60 {
        step(&mut w, &mut c, &mut v, false, false);
    }
    assert!(c.on_floor() && (c.position().y - 480.0).abs() < 1.5, "on the ledge: {:?}", c.position());
    // Down through it.
    for _ in 0..40 {
        step(&mut w, &mut c, &mut v, false, true);
    }
    assert!(c.on_floor() && (c.position().y - 580.0).abs() < 1.5, "back on the ground: {:?}", c.position());
    // Walks into the wall and stops; the wall takes its velocity.
    for _ in 0..90 {
        v.x = 200.0;
        step(&mut w, &mut c, &mut v, false, false);
    }
    assert!((c.position().x - (330.0 - 10.0)).abs() < 1.5 && c.on_wall(), "at the wall: {:?}", c.position());
    assert!(v.x.abs() < 1e-3);
}


#[test]
fn floor_stop_on_slope_off_lets_a_2d_character_slide_down_a_gentle_slope() {
    for stop in [true, false] {
        let mut w = World::new();
        // A 25° slope (pixels, +y down), rising to the right.
        w.add(Body::fixed(200.0, 400.0), Shape::rect(400.0, 20.0).rotated(-25f32.to_radians()));
        let s = CharacterSettings { floor_stop_on_slope: stop, ..CharacterSettings::default() };
        let mut c = Character::new(&mut w, vec2(200.0, 340.0), Shape::rect(20.0, 40.0), s, 0);
        let mut v = Vec2::ZERO;
        for _ in 0..30 {
            v.y += 1800.0 * DT;
            v = c.move_and_slide(&mut w, v, DT, false);
        }
        let x0 = c.position().x;
        for _ in 0..60 {
            v.x = 0.0;
            v.y += 1800.0 * DT;
            v = c.move_and_slide(&mut w, v, DT, false);
        }
        let moved = x0 - c.position().x;
        assert!(c.on_floor());
        if stop {
            assert!(moved.abs() < 1.0, "stops on the slope: moved {moved}");
        } else {
            assert!(moved > 10.0, "slides down it: moved {moved}");
        }
    }
}

/// A platform's character that went leaves nothing behind in its world.
#[test]
fn a_removed_2d_character_leaves_nothing_in_the_way() {
    let mut w = World::new();
    ground(&mut w);
    let a = Character::new(&mut w, vec2(180.0, 570.0), Shape::rect(20.0, 40.0), CharacterSettings::default(), 0);
    a.remove(&mut w);
    let mut b = Character::new(&mut w, vec2(120.0, 570.0), Shape::rect(20.0, 40.0), CharacterSettings::default(), 0);
    let mut v = Vec2::ZERO;
    for _ in 0..60 {
        v.x = 200.0;
        v.y += 1800.0 * DT;
        v = b.move_and_slide(&mut w, v, DT, false);
    }
    assert!(b.position().x > 300.0, "walked through where the first was: {:?}", b.position());
}
