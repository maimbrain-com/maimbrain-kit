use super::*;

const DT: f32 = 1.0 / 60.0;

fn floor(w: &mut World) -> BodyId {
    w.add(Body::fixed(vec3(0.0, -0.5, 0.0)), Shape::cuboid(vec3(40.0, 1.0, 40.0)))
}

fn jitter_dt(i: u32) -> f32 {
    let x = (i.wrapping_mul(2654435761) >> 16) as f32 / 65536.0;
    1.0 / 60.0 + (x - 0.5) * 0.004
}

/// Dominoes in an arc, a marble run ramp (trimesh), marbles, a ragdoll-ish
/// chain of capsules on spherical joints and a motorized spinner.
fn scene() -> (World, BodyId, Vec<BodyId>) {
    let mut w = World::new();
    floor(&mut w);
    let mut dominoes = Vec::new();
    for i in 0..24 {
        let a = i as f32 * 0.12;
        let pos = vec3(a.cos() * 4.0, 0.5, a.sin() * 4.0);
        let rot = Quat::from_rotation_y(-a);
        dominoes.push(w.add(Body::dynamic(pos).rotation(rot), Shape::cuboid(vec3(0.1, 1.0, 0.5)).density(600.0)));
    }
    let ramp = MeshData::plane(2.0, 6.0).transformed(&Transform::at(vec3(-3.0, 1.0, 0.0)).with_rot(Quat::from_rotation_x(0.3)).matrix());
    w.add(Body::fixed(Vec3::ZERO), Shape::trimesh(&ramp));
    for i in 0..10 {
        w.add(Body::dynamic(vec3(-3.0 + (i % 3) as f32 * 0.3, 3.0 + i as f32 * 0.25, -2.0)), Shape::sphere(0.12).restitution(0.3));
    }
    let top = w.add(Body::fixed(vec3(2.0, 4.0, -3.0)), Shape::sphere(0.05));
    let mut prev = top;
    for k in 0..5 {
        let c = vec3(2.0, 3.6 - k as f32 * 0.5, -3.0);
        let seg = w.add(Body::dynamic(c), Shape::capsule(0.1, 0.5));
        w.join(Joint::spherical(prev, seg, c + vec3(0.0, 0.25, 0.0)));
        prev = seg;
    }
    let hub = w.add(Body::fixed(vec3(-1.0, 0.3, 2.0)), Shape::sphere(0.05).sensor());
    let spinner = w.add(Body::dynamic(vec3(-1.0, 0.3, 2.0)), Shape::cuboid(vec3(2.0, 0.1, 0.2)));
    w.join(Joint::revolute(hub, spinner, vec3(-1.0, 0.3, 2.0), Vec3::Y).motor(3.0, 500.0));
    (w, dominoes[0], dominoes)
}

#[test]
fn identical_inputs_give_bit_identical_worlds() {
    let run = || {
        let (mut w, first, _) = scene();
        let mut hashes = Vec::new();
        let mut events = Vec::new();
        for i in 0..3000u32 {
            if i == 30 {
                w.apply_impulse_at(first, vec3(0.0, 0.0, 2.0), w.position(first) + vec3(0.0, 0.4, 0.0));
            }
            w.step(jitter_dt(i));
            events.extend(w.events().iter().map(|e| format!("{i} {e:?}")));
            hashes.push(w.state_hash());
        }
        (hashes, events)
    };
    let (h1, e1) = run();
    let (h2, e2) = run();
    for (i, (a, b)) in h1.iter().zip(&h2).enumerate() {
        assert_eq!(a, b, "diverged at frame {i}");
    }
    assert_eq!(e1, e2);
    assert!(e1.len() > 50, "{} events", e1.len());
}

#[test]
fn dominoes_fall_in_order() {
    let (mut w, first, dominoes) = scene();
    for i in 0..600u32 {
        if i == 10 {
            w.apply_impulse_at(first, vec3(0.0, 0.0, 3.0), w.position(first) + vec3(0.0, 0.4, 0.0));
        }
        w.step(DT);
    }
    let fallen = dominoes.iter().filter(|&&d| w.position(d).y < 0.35).count();
    assert!(fallen >= 20, "{fallen} of {} fell", dominoes.len());
}

#[test]
fn free_fall_and_contact_speed() {
    let mut w = World::new();
    let f = floor(&mut w);
    let ball = w.add(Body::dynamic(vec3(0.0, 5.1, 0.0)), Shape::sphere(0.1));
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
    assert!(c.involves(ball) && c.other(ball) == Some(f));
    // √(2·9.81·5) ≈ 9.9 m/s
    assert!((c.speed - 9.9).abs() < 0.5, "speed {}", c.speed);
    assert!(c.impulse > 0.0);
    assert!(c.normal.dot(w.position(c.b) - w.position(c.a)) > 0.0);
}

#[test]
fn queries_sync_and_mesh() {
    let mut w = World::new();
    let f = floor(&mut w);
    let b = w.add(Body::dynamic(vec3(0.0, 0.25, 0.0)), Shape::cube(0.5));
    w.step(DT);
    let hit = w.raycast(vec3(3.0, 5.0, 3.0), vec3(3.0, -5.0, 3.0)).unwrap();
    assert_eq!(hit.body, f);
    assert!(hit.point.y.abs() < 1e-3 && (hit.normal.y - 1.0).abs() < 1e-3 && (hit.distance - 5.0).abs() < 1e-3);
    assert_eq!(w.raycast(vec3(-2.0, 0.25, 0.0), vec3(2.0, 0.25, 0.0)).map(|h| h.body), Some(b));
    assert_eq!(w.bodies_at(vec3(0.0, 0.25, 0.0)), vec![b]);
    let t = w.draw_transform(b);
    assert!((t.pos.y - 0.25).abs() < 0.05);
    w.sync(b, Node::new().unwrap()); // stub host: no panic
    let m = Shape::capsule(0.2, 1.0).mesh(16).unwrap();
    let ys: Vec<f32> = m.positions.iter().map(|p| p.y).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
    assert!((lo + 0.5).abs() < 1e-3 && (hi - 0.5).abs() < 1e-3, "{lo} {hi}");
    assert!(Shape::convex_hull_of(&MeshData::icosphere(1.0, 1)).mesh(8).is_none());
}

#[test]
fn joints_and_motor() {
    let mut w = World::with_gravity(Vec3::ZERO);
    let hub = w.add(Body::fixed(Vec3::ZERO), Shape::sphere(0.05));
    let wheel = w.add(Body::dynamic(Vec3::ZERO).rotation(Quat::from_rotation_x(0.7)), Shape::cylinder(0.5, 0.2));
    let j = w.join(Joint::revolute(hub, wheel, Vec3::ZERO, vec3(0.0, 0.0, 1.0)).motor(2.0, f32::INFINITY));
    let slide = w.add(Body::dynamic(vec3(2.0, 0.0, 0.0)).rotation(Quat::from_rotation_y(0.4)), Shape::cube(0.2));
    w.join(Joint::prismatic(hub, slide, vec3(2.0, 0.0, 0.0), Vec3::X).limits(-0.5, 0.5).motor(1.0, f32::INFINITY));
    let rot0 = w.rotation(slide);
    for _ in 0..120 {
        w.step(DT);
    }
    let s = w.spin(wheel);
    assert!((s.z - 2.0).abs() < 0.05 && s.x.abs() < 0.05 && s.y.abs() < 0.05, "spin {s:?}");
    let p = w.position(slide);
    assert!((p.x - 2.5).abs() < 0.02 && p.y.abs() < 1e-3 && p.z.abs() < 1e-3, "slider {p:?}");
    assert!(w.rotation(slide).dot(rot0).abs() > 0.9999, "the slider kept its rotation");
    w.set_motor(j, -1.0, f32::INFINITY);
    for _ in 0..60 {
        w.step(DT);
    }
    assert!((w.spin(wheel).z + 1.0).abs() < 0.05);
}

#[test]
fn kinematic_targets_and_forces() {
    let mut w = World::with_gravity(Vec3::ZERO);
    let k = w.add(Body::kinematic(Vec3::ZERO), Shape::cube(1.0));
    let target_rot = Quat::from_rotation_y(1.0);
    for i in 1..=60 {
        let t = i as f32 / 60.0;
        w.move_kinematic(k, vec3(t * 2.0, 0.0, 0.0), Quat::IDENTITY.slerp(target_rot, t));
        w.step(DT);
    }
    assert!((w.position(k).x - 2.0).abs() < 0.01);
    assert!(w.rotation(k).dot(target_rot).abs() > 0.9999);
    let b = w.add(Body::dynamic(vec3(10.0, 0.0, 0.0)), Shape::cube(0.1)); // 1 kg
    assert!((w.mass(b) - 1.0).abs() < 1e-3);
    for _ in 0..120 {
        w.apply_force(b, vec3(0.0, 3.0, 0.0));
        w.step(1.0 / 120.0);
    }
    assert!((w.velocity(b).y - 3.0).abs() < 0.05, "{:?}", w.velocity(b));
}

/// A ray-cast vehicle rests on its springs, drives toward −z, turns left
/// for positive steering and brakes to a stop; on a heightfield too.
#[test]
fn a_vehicle_drives_and_brakes() {
    for ground in ["box", "heightfield"] {
        let mut w = World::new();
        match ground {
            "box" => {
                w.add(Body::fixed(vec3(0.0, -0.5, 0.0)), Shape::cuboid(vec3(400.0, 1.0, 400.0)));
            }
            _ => {
                let n = 33;
                let heights = vec![0.0; n * n];
                w.add(Body::fixed(Vec3::ZERO), Shape::heightfield(&heights, n, n, 4.0));
            }
        }
        let car = w.add(Body::dynamic(vec3(0.0, 1.0, 0.0)), Shape::cuboid(vec3(1.8, 0.5, 3.2)).mass(400.0));
        let wheel = |x: f32, z: f32| Wheel { at: vec3(x, -0.2, z), radius: 0.42, rest: 0.35, travel: 0.3, stiffness: 30.0, compression: 2.5, relaxation: 3.2, friction_slip: 2.2, side_friction: 1.0, max_force: 20000.0 };
        let v = w.add_vehicle(car, &[wheel(-0.95, -1.25), wheel(0.95, -1.25), wheel(-0.95, 1.25), wheel(0.95, 1.25)]);
        let mut speeds = Vec::new();
        for f in 0..360 {
            let t = f as f32 / 60.0;
            let (engine, brake) = if t < 2.0 { (900.0, 0.0) } else if t < 3.0 { (0.0, 0.0) } else { (0.0, 15.0) };
            for i in 0..4 {
                w.set_wheel(v, i, engine, brake, 0.0);
            }
            w.step(1.0 / 60.0);
            speeds.push(w.vehicle_speed(v));
        }
        assert!(w.position(car).z < -5.0, "{ground}: drove toward −z: {:?}", w.position(car));
        assert!(speeds[179] > 5.0, "{ground}: speed {}", speeds[179]);
        assert!(speeds[359].abs() < 0.3, "{ground}: braked from {} to {}: {:?}", speeds[179], speeds[359], speeds.iter().step_by(10).collect::<Vec<_>>());
        assert!(w.wheels(v).iter().all(|s| s.on_ground), "{ground}: {:?} {:?}", w.wheels(v), w.position(car));
    }
}

/// A ramp at `deg` degrees, rising along +x from x = 2, wide in z.
fn ramp(w: &mut World, deg: f32) {
    let a = deg.to_radians();
    let len = 12.0;
    let c = vec3(2.0 + len / 2.0 * a.cos(), len / 2.0 * a.sin(), 0.0);
    w.add(Body::fixed(c).rotation(Quat::from_rotation_z(a)), Shape::cuboid(vec3(len, 0.2, 10.0)));
}

/// Walks a capsule at +x for `secs` with gravity (as a game does), returns its velocity.
fn walk(w: &mut World, c: &mut Character, secs: f32) -> Vec3 {
    let mut v = Vec3::ZERO;
    for _ in 0..(secs / DT) as usize {
        v.x = 3.0;
        v.y -= 18.0 * DT;
        v = c.move_and_slide(w, v, DT);
        w.step(DT);
    }
    v
}

#[test]
fn a_character_climbs_gentle_slopes_and_not_steep_ones() {
    for (deg, climbs) in [(25.0, true), (60.0, false)] {
        let mut w = World::new();
        floor(&mut w);
        ramp(&mut w, deg);
        let mut c = Character::new(&mut w, vec3(0.0, 0.76, 0.0), Shape::capsule(0.35, 1.5), CharacterSettings::default(), 0);
        walk(&mut w, &mut c, 3.0);
        let p = c.position();
        if climbs {
            assert!(p.x > 6.0 && p.y > 2.0, "{deg}°: climbed to {p:?}");
            assert!(c.on_floor(), "{deg}°: on the slope is on the floor");
        } else {
            assert!(p.x < 3.0 && p.y < 1.2, "{deg}°: stayed at the foot, at {p:?}");
            assert!(c.on_floor() && c.on_wall(), "{deg}°: on the floor, against a wall");
        }
    }
}

#[test]
fn a_character_slides_down_steep_slopes_and_stands_on_gentle_ones() {
    for (deg, slides) in [(25.0f32, false), (60.0, true)] {
        let mut w = World::new();
        floor(&mut w);
        ramp(&mut w, deg);
        let a = deg.to_radians();
        // Dropped onto the slope's middle.
        let x = 2.0 + 4.0 * a.cos();
        let mut c = Character::new(&mut w, vec3(x, 4.0 * a.sin() + 2.0, 0.0), Shape::capsule(0.35, 1.5), CharacterSettings::default(), 0);
        let mut v = Vec3::ZERO;
        for _ in 0..120 {
            v.x = 0.0;
            v.y -= 18.0 * DT;
            v = c.move_and_slide(&mut w, v, DT);
        }
        let p = c.position();
        if slides {
            assert!(p.x < 2.5 && p.y < 1.0, "{deg}°: slid to the bottom, at {p:?}");
        } else {
            assert!((p.x - x).abs() < 0.1, "{deg}°: stood still, at {p:?}");
            assert!(c.on_floor());
        }
    }
}

#[test]
fn a_character_steps_snaps_and_stops_at_walls() {
    let mut w = World::new();
    floor(&mut w);
    // A 0.25 m step at x = 3, a wall at x = 8.
    w.add(Body::fixed(vec3(5.5, 0.125, 0.0)), Shape::cuboid(vec3(5.0, 0.25, 10.0)));
    w.add(Body::fixed(vec3(8.5, 2.0, 0.0)), Shape::cuboid(vec3(1.0, 4.0, 10.0)));
    let s = CharacterSettings { max_step_height: 0.3, ..CharacterSettings::default() };
    let mut c = Character::new(&mut w, vec3(0.0, 0.76, 0.0), Shape::capsule(0.35, 1.5), s, 0);
    let v = walk(&mut w, &mut c, 4.0);
    let p = c.position();
    assert!((p.x - (8.0 - 0.35)).abs() < 0.05 && (p.y - 1.0).abs() < 0.05, "up the step, against the wall: {p:?}");
    assert!(c.on_floor() && c.on_wall());
    assert!(v.x.abs() < 1e-3, "the wall took its velocity: {v:?}");
    assert!(v.y == 0.0, "the floor took its fall: {v:?}");
    // Without steps it stops at the step.
    let mut w = World::new();
    floor(&mut w);
    w.add(Body::fixed(vec3(5.5, 0.125, 0.0)), Shape::cuboid(vec3(5.0, 0.25, 10.0)));
    let mut c = Character::new(&mut w, vec3(0.0, 0.76, 0.0), Shape::capsule(0.35, 1.5), CharacterSettings::default(), 0);
    walk(&mut w, &mut c, 2.0);
    assert!(c.position().x < 3.0, "no step: {:?}", c.position());
}

#[test]
fn rigid_bodies_see_a_character() {
    // A ball dropped on it rests on it.
    let mut w = World::new();
    floor(&mut w);
    let c = Character::new(&mut w, vec3(0.0, 0.76, 0.0), Shape::capsule(0.35, 1.5), CharacterSettings::default(), 7);
    let ball = w.add(Body::dynamic(vec3(0.0, 3.0, 0.0)), Shape::sphere(0.2));
    for _ in 0..120 {
        w.step(DT);
    }
    assert!(w.position(ball).y > 1.5, "the ball rests on the character: {:?}", w.position(ball));
    assert_eq!(w.tag(c.body()), 7);
}

#[test]
fn a_character_slides_round_a_trunk_where_round_is_uphill() {
    // Moss Knight's stall: walking up a slope into a tree trunk off its
    // middle, sliding round it climbs the slope. Rapier's sweep goes back
    // and forth between the trunk (a wall: no climbing along it) and the
    // slope (climbing), so it stopped dead in the crease. Each case: a slope
    // rising toward +x (or −x) across the walk, a trunk on it at z = −4, the
    // character 2 m before it walking toward a point `off` to its side.
    for tilt in [0.4f32, 0.6, 0.77, -0.6, -0.77] {
        for off in [-0.4f32, -0.25, 0.25, 0.4] {
            let mut w = World::new();
            w.add(Body::fixed(vec3(0.0, -0.5, 0.0)).rotation(Quat::from_rotation_z(tilt)), Shape::cuboid(vec3(40.0, 1.0, 40.0)));
            w.add(Body::fixed(vec3(0.0, 3.0, -4.0)), Shape::cylinder(0.35, 6.0));
            let s = CharacterSettings { floor_snap_length: 0.3, max_step_height: 0.35, floor_max_angle: 0.87, ..CharacterSettings::default() };
            let mut c = Character::new(&mut w, vec3(0.0, 1.0, -2.0), Shape::capsule(0.35, 1.2), s, 0);
            let d = vec3(off, 0.0, -2.0).normalize() * 5.0;
            let mut v = Vec3::ZERO;
            for _ in 0..90 {
                (v.x, v.z) = (d.x, d.z);
                v.y = if c.on_floor() { 0.0 } else { v.y - 18.0 * DT };
                v = c.move_and_slide(&mut w, v, DT);
            }
            let p = c.position();
            assert!(p.z < -6.0, "slope {:.0}°, aimed {off} m off the trunk: stopped at {p:?}", tilt.to_degrees());
        }
    }
}

#[test]
fn floor_stop_on_slope_off_lets_a_character_slide_down_a_gentle_slope() {
    for stop in [true, false] {
        let mut w = World::new();
        floor(&mut w);
        ramp(&mut w, 25.0);
        let a = 25f32.to_radians();
        let x = 2.0 + 6.0 * a.cos();
        let s = CharacterSettings { floor_stop_on_slope: stop, ..CharacterSettings::default() };
        let mut c = Character::new(&mut w, vec3(x, 6.0 * a.sin() + 1.2, 0.0), Shape::capsule(0.35, 1.5), s, 0);
        let mut v = Vec3::ZERO;
        for _ in 0..90 {
            v.x = 0.0;
            v.y -= 18.0 * DT;
            v = c.move_and_slide(&mut w, v, DT);
        }
        let moved = x - c.position().x;
        // Godot's creep: the game's gravity, reset on the floor each frame, slid along it.
        if stop {
            assert!(moved.abs() < 0.01, "stops on the slope: moved {moved}");
        } else {
            assert!(moved > 0.1, "slides down it: moved {moved}");
        }
    }
}
