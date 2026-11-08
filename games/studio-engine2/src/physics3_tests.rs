//! 3D physics, natively (docs/STUDIO-2.md §9.4 row 3a): Godot's bodies,
//! areas, joints and vehicles on Rapier 3D, `physics.raycast`, and the
//! terrain's heightfield under them.

use std::rc::Rc;

use crate::input::Held;
use crate::project::{Files, Project};
use crate::vm::{Mode, Vm};

/// A 3D project with this main.lua.
fn three(main: &str) -> Files {
    let mut f = Files::new();
    f.insert("studio.toml".into(), b"title = \"T\"\nengine = \"2.0\"\nkind = \"3d\"\nlives = 3\n".to_vec());
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    crate::tests::compile(&mut f);
    f
}

fn vm3(main: &str) -> Vm {
    let p = Rc::new(Project::parse(&three(main)).unwrap());
    Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap_or_else(|e| panic!("{e}"))
}

fn error3(main: &str) -> String {
    let p = Rc::new(Project::parse(&three(main)).unwrap());
    match Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

fn step(v: &mut Vm, n: usize) {
    for _ in 0..n {
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

fn printed(v: &Vm) -> String {
    v.host().draw.texts().collect::<Vec<_>>().join("\n")
}

fn field(out: &str, k: &str) -> f32 {
    out.split_whitespace().skip_while(|w| *w != k).nth(1).unwrap_or_else(|| panic!("no {k} in {out}")).parse().unwrap_or_else(|_| panic!("{k} in {out}"))
}

/// Knockdown: a wall of crates on a static floor, hit by a heavy ball;
/// bodyEntered on the ball (contactMonitor), meshes riding their bodies,
/// and an Area3D that hears crates fall into it.
pub const KNOCKDOWN: &str = r##"
floor = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5 }
crates = {}
for row = 0, 3 do
  for col = -2, 2 do
    local c = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, mass = 10, x = col * 1.02, y = 0.5 + row * 1.01, z = 0 }
    Mesh{ geometry = geometry.box(1, 1, 1), parent = c }
    crates[#crates + 1] = c
  end
end
ball = RigidBody3D{ shape = SphereShape3D{ radius = 0.6 }, mass = 200, x = 0, y = 1.2, z = 12, contactMonitor = true }
hits = 0
ball.bodyEntered:connect(function(other) hits = hits + 1 end)
pit = Area3D{ shape = BoxShape3D{ w = 20, h = 4, d = 6 }, z = -6, y = 0 }
inPit = 0
pit.bodyEntered:connect(function(b) inPit = inPit + 1 end)
local t = 0
function love.update(dt)
  t = t + dt
  if t > 0.5 and not thrown then thrown = true; ball.vz = -18 end
end
function love.draw()
  local down = 0
  for _, c in ipairs(crates) do if c.y < 0.6 then down = down + 1 end end
  love.graphics.print(string.format("down %d hits %d pit %d ballz %.1f", down, hits, inPit, ball.z))
end
"##;

#[test]
fn knockdown_topples_the_wall() {
    let mut v = vm3(KNOCKDOWN);
    step(&mut v, 60 * 6);
    let out = printed(&v);
    assert!(field(&out, "down") >= 12.0, "most of the wall came down: {out}");
    assert!(field(&out, "hits") >= 1.0, "the ball heard its hits: {out}");
    assert!(field(&out, "pit") >= 1.0, "crates fell in the pit: {out}");
}

/// Knockdown's throw: an 11 m/s ball (0.09 m, `continuousCd`) at a 0.3 m
/// crate on a table, off its middle. It went 0.06 m into the crate in one
/// step, nearer the side face than the front, was pushed out sideways and
/// flew on through, leaving the crate where it was.
#[test]
fn a_fast_ball_knocks_a_crate_it_hits_off_middle() {
    for aim in [0.0f32, 0.06, 0.1, 0.13] {
        let mut v = vm3(&format!(
            r#"
StaticBody3D{{ shape = BoxShape3D{{ w = 30, h = 1, d = 30 }}, y = -0.5 }}
StaticBody3D{{ shape = BoxShape3D{{ w = 2.6, h = 0.12, d = 0.7 }}, y = 0.74, z = -5 }}
local G = -select(2, physics.getGravity())
crate = RigidBody3D{{ shape = BoxShape3D{{ w = 0.3, h = 0.3, d = 0.3 }}, mass = 4.86, friction = 0.7, bounce = 0.1, x = 0.32, y = 0.95, z = -5 }}
local clock = 0
function love.update(dt)
  clock = clock + dt
  if clock > 1 and not ball then
    local x0, y0, z0 = 0, 1.15, -2.2
    local dx, dy, dz = 0.32 + {aim} - x0, 0.95 - y0, -4.85 - z0
    local t = math.sqrt(dx * dx + dy * dy + dz * dz) / 11
    ball = RigidBody3D{{ shape = SphereShape3D{{ radius = 0.09 }}, mass = 6.72, bounce = 0.3, friction = 0.6, continuousCd = true, x = x0, y = y0, z = z0 }}
    ball.vx, ball.vy, ball.vz = dx / t, dy / t + 0.5 * G * t, dz / t
  end
end
function love.draw() love.graphics.print(string.format("moved %.2f ballz %.2f", math.abs(crate.z + 5), ball and ball.z or 0)) end
"#
        ));
        step(&mut v, 150);
        let out = printed(&v);
        assert!(field(&out, "moved") > 0.5, "aimed {aim} m off the middle: the crate stays and the ball goes on: {out}");
    }
}

/// A crate dropped on the terrain lands on the true ground (heightAt), far
/// from the origin; physics.raycast finds the ground and bodies; nothing
/// falls through at speed.
#[test]
fn bodies_land_on_the_terrain() {
    let mut v = vm3(r#"
        ground = Terrain{ noise = { seed = 3 }, height = 60 }
        local x, z = 230, -410
        crate = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, x = x, y = ground:heightAt(x, z) + 6, z = z }
        ball = RigidBody3D{ shape = SphereShape3D{ radius = 0.5 }, x = x + 20, y = ground:heightAt(x + 20, z) + 40, z = z, continuousCd = true }
        function love.draw()
          local h = ground:heightAt(crate.x, crate.z)
          local hb = ground:heightAt(ball.x, ball.z)
          local hit = physics.raycast(crate.x, 500, crate.z, crate.x, -500, crate.z)
          local miss = physics.raycast(x, 500, z, x, 400, z)
          love.graphics.print(string.format("over %.2f ballover %.2f ray %s %.2f miss %s",
            crate.y - h, ball.y - hb, hit and tostring(hit.node == crate) or "nil", hit and hit.y or 0, tostring(miss)))
        end
        "#);
    step(&mut v, 60 * 5);
    let out = printed(&v);
    // Resting on the ground: its centre half a metre up, give or take a slope.
    let over = field(&out, "over");
    assert!(over > 0.3 && over < 1.2, "{out}");
    let b = field(&out, "ballover");
    assert!(b > 0.3 && b < 1.2, "the ball from 40 m stayed on top: {out}");
    assert!(out.contains("ray true"), "the ray down hits the crate on the ground: {out}");
    assert!(out.ends_with("miss nil"), "{out}");
}

/// Joints: a hinged door swings and stops at its limit, a weld
/// (Generic6DOF) with breakForce breaks under a load and says so.
#[test]
fn joints_hold_and_break() {
    let mut v = vm3(r#"
        frame = StaticBody3D{ shape = BoxShape3D{ w = 0.2, h = 3, d = 0.2 }, y = 1.5 }
        door = RigidBody3D{ shape = BoxShape3D{ w = 1.2, h = 2, d = 0.1 }, x = 0.7, y = 1.5, mass = 5 }
        hinge = HingeJoint3D{ nodeA = frame, nodeB = door, x = 0.1, y = 1.5, rx = math.pi / 2, limitLower = -0.6, limitUpper = 0.6 }
        door:applyImpulse(0, 0, -30, 0.5, 0, 0)
        shelf = StaticBody3D{ shape = BoxShape3D{ w = 1, h = 0.2, d = 1 }, x = 5, y = 3 }
        load = RigidBody3D{ shape = BoxShape3D{ w = 0.5, h = 0.5, d = 0.5 }, x = 5, y = 2.6, mass = 2 }
        weld = Generic6DOFJoint3D{ nodeA = shelf, nodeB = load, x = 5, y = 2.8, breakForce = 200 }
        broke = 0
        weld.broken:connect(function(f) broke = broke + 1 end)
        maxTurn = 0
        function love.update(dt)
          maxTurn = math.max(maxTurn, math.abs(door.ry))
          if round.time > 1 and not heavy then heavy = true; load.mass = 60 end
        end
        function love.draw() love.graphics.print(string.format("turn %.2f broke %d loady %.1f force %.1f mass %.1f", maxTurn, broke, load.y, weld.force, load.mass)) end
        "#);
    step(&mut v, 60 * 3);
    let out = printed(&v);
    assert!(field(&out, "turn") < 0.7 && field(&out, "turn") > 0.2, "the door swung to its limit: {out}");
    assert!(field(&out, "broke") == 1.0, "the weld broke once: {out}");
    assert!(field(&out, "loady") < 1.0, "and the load fell: {out}");
}

/// What a game gets wrong says what to write.
#[test]
fn physics_errors_say_what_to_write() {
    assert!(error3("RigidBody3D{ x = 1 }").contains("needs a shape"));
    assert!(error3("RigidBody3D{ shape = 3 }").contains("shape is a BoxShape3D"));
    assert!(error3("BoxShape3D{ size = 2 }").contains("no key `size`"));
    assert!(error3("CapsuleShape3D{ radius = 1, height = 1 }").contains("whole capsule"));
    assert!(error3("local b = RigidBody3D{ shape = SphereShape3D{} } b.mas = 3").contains("did you mean `mass`"));
    assert!(error3("local b = RigidBody3D{ shape = ConcavePolygonShape3D{ geometry = geometry.box(1,1,1) } }").contains("StaticBody3D and Area3D only"));
    assert!(error3("VehicleWheel3D{}").contains("goes under its VehicleBody3D"));
    assert!(error3("HingeJoint3D{ nodeA = 1 }").contains("nodeA is a RigidBody3D"));
    assert!(error3("love.physics.newWorld()").contains("love.physics is 2D's"));
    assert!(error3("HeightMapShape3D{}").contains("a Terrain is the ground's shape"));
}

/// A buggy: a VehicleBody3D on four VehicleWheel3Ds, as Godot builds one.
pub const BUGGY: &str = r##"
function buggy(x, y, z)
  local car = VehicleBody3D{ shape = BoxShape3D{ w = 1.8, h = 0.5, d = 3.2 }, mass = 400, x = x, y = y, z = z,
                             linearDamp = 0.12, angularDamp = 0.6 }
  for _, p in ipairs({ { -0.95, -0.2, -1.25 }, { 0.95, -0.2, -1.25 }, { -0.95, -0.2, 1.25 }, { 0.95, -0.2, 1.25 } }) do
    local wheel = VehicleWheel3D{ parent = car, x = p[1], y = p[2], z = p[3], wheelRadius = 0.42, wheelRestLength = 0.35,
      suspensionTravel = 0.3, suspensionStiffness = 30, suspensionMaxForce = 20000, dampingCompression = 2.5,
      dampingRelaxation = 3.2, wheelFrictionSlip = 2.2, useAsTraction = true, useAsSteering = p[3] < 0 }
  end
  return car
end
"##;

/// The buggy drives toward its front (−z), turns left for positive
/// steering, rests on its springs, and brakes to a stop.
#[test]
fn a_vehicle_drives_steers_and_brakes() {
    let src = format!(
        r#"{BUGGY}
        floor = StaticBody3D{{ shape = BoxShape3D{{ w = 400, h = 1, d = 400 }}, y = -0.5 }}
        car = buggy(0, 1.2, 0)
        function love.update(dt)
          local t = round.time
          car.engineForce = (t > 0.5 and t < 3) and 900 or 0
          car.steering = (t > 2 and t < 3) and 0.4 or 0
          car.brake = t > 3.5 and 15 or 0
          if t > 1.9 and not z2 then z2, x2 = car.z, car.x end
          if t > 3.4 and not s1 then s1 = car.speed print("s1", s1, car.brake) end
          if t > 4.4 and not s2 then s2 = car.speed print("s2", s2) end
        end
        function love.draw()
          love.graphics.print(string.format("b %.1f w %s s1 %.2f s2 %.2f z2 %.1f x2 %.2f x %.1f z %.1f y %.2f speed %.2f ry %.2f", car.brake, tostring(car.children[1]:isInContact()), s1 or 0, s2 or 0,
            z2 or 0, x2 or 0, car.x, car.z, car.y, car.speed, car.ry))
        end
        "#
    );
    let mut v = vm3(&src);
    step(&mut v, 60 * 6);
    let out = printed(&v);
    assert!(field(&out, "z2") < -8.0, "drove toward −z: {out}");
    assert!(field(&out, "x2").abs() < 0.3, "straight while not steering: {out}");
    assert!(field(&out, "x") < field(&out, "x2") - 1.0, "turned left: {out}");
    assert!(field(&out, "speed").abs() < 0.5, "braked to a stop: {out}");
    let y = field(&out, "y");
    assert!(y > 0.4 && y < 1.0, "rests on its springs, wheels on the floor: {out}");
}

/// The owner's complaint: a buggy at speed over a crest leaves the ground
/// and lands. Over a hill stamp on a terrain, its wheels lose contact for a
/// while past the crest, then it lands on them, upright.
#[test]
fn a_buggy_catches_air_off_a_crest() {
    let src = format!(
        r#"{BUGGY}
        ground = Terrain{{ noise = false, height = 10, detail = 1, stamps = {{ {{ "hill", 0, -70, radius = 22, height = 3.5 }} }} }}
        car = buggy(0, ground:heightAt(0, 0) + 1.2, 0)
        air, longest, flips = 0, 0, 0
        local run = 0
        function love.update(dt)
          car.engineForce = 700
          local wheels = car.children
          local touching = 0
          for _, w in ipairs(wheels) do if w:isInContact() then touching = touching + 1 end end
          if touching == 0 then run = run + dt; air = air + dt else longest = math.max(longest, run); run = 0 end
        end
        function love.draw()
          local up = math.cos(car.rx) * math.cos(car.rz)
          love.graphics.print(string.format("air %.2f longest %.2f z %.1f speed %.1f up %.2f over %.2f",
            air, longest, car.z, car.speed, up, car.y - ground:heightAt(car.x, car.z)))
        end
        "#
    );
    let mut v = vm3(&src);
    step(&mut v, 60 * 9);
    let out = printed(&v);
    assert!(field(&out, "z") < -90.0, "drove over the hill: {out}");
    assert!(field(&out, "longest") > 0.3, "left the ground off the crest: {out}");
    assert!(field(&out, "up") > 0.8, "landed upright: {out}");
}

/// What a frame of physics costs natively, N bodies tumbling onto a floor
/// or a terrain (and a buggy on the terrain): `cargo test -p studio-engine2
/// --release --lib physics_cost -- --ignored --nocapture`. The frame's whole
/// end (the step, signals, nodes back to mb3d), not Lua.
#[test]
#[ignore]
fn physics_cost() {
    for (ground, n) in [("floor", 50), ("floor", 100), ("floor", 200), ("terrain", 50), ("terrain", 100)] {
        let floor = if ground == "floor" {
            "StaticBody3D{ shape = BoxShape3D{ w = 60, h = 1, d = 60 }, y = -0.5 }\nlocal function h() return 0 end"
        } else {
            "local g = Terrain{ noise = { seed = 2 }, height = 30, detail = 2 }\nlocal function h(x, z) return g:heightAt(x, z) end"
        };
        let src = format!(
            "{floor}\nfor i = 0, {n} - 1 do local x, z = (i % 10) * 1.3 - 6, math.floor(i / 10) % 10 * 1.3 - 6\n RigidBody3D{{ shape = (i % 3 == 0) and SphereShape3D{{ radius = 0.5 }} or BoxShape3D{{ w = 1, h = 1, d = 1 }}, x = x, y = h(x, z) + 1 + math.floor(i / 100) * 1.2 + (i % 7) * 0.05, z = z }} end"
        );
        let mut v = vm3(&src);
        let mut times = Vec::new();
        for f in 0..240 {
            let t = std::time::Instant::now();
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
            if f >= 30 && f < 150 {
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let p = v.host().three.as_ref().unwrap().phys.as_ref().unwrap();
        println!(
            "{ground} {n} bodies: median {:.3} ms, p95 {:.3} ms a frame (tiles {}, awake at the end {})",
            times[times.len() / 2],
            times[times.len() * 95 / 100],
            p.tiles.len(),
            p.world.awake_count()
        );
    }
    let mut v = vm3(&format!("{BUGGY}\nground = Terrain{{ detail = 2 }}\ncar = buggy(0, ground:heightAt(0, 0) + 1.4, 0)\nfunction love.update() car.engineForce = 900 end"));
    let mut times = Vec::new();
    for f in 0..600 {
        let t = std::time::Instant::now();
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
        if f >= 30 {
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let p = v.host().three.as_ref().unwrap().phys.as_ref().unwrap();
    let car = p.bodies.values().find(|b| b.vehicle.is_some()).unwrap();
    println!(
        "buggy on Terrain{{}}: median {:.3} ms, p95 {:.3} ms, max {:.3} ms a frame (the terrain's tiles made as it goes: {} now); it went {:.0} m",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() - 1],
        p.tiles.len(),
        p.world.position(car.id).length()
    );
}

/// The bodies Lua reads are the same natively as in wasm: this sum is what
/// the same Lua printed in V8 (Deno, through `mb play`) when 3a was built:
/// a buggy over a terrain's hill and twelve bodies falling onto the ground.
/// A change here changes how every 3D game's bodies move.
#[test]
fn bodies_are_the_same_in_v8() {
    let mut v = vm3(DET3);
    step(&mut v, 401);
    assert_eq!(printed(&v), "physics3 17348.4528");
}

const DET3: &str = r##"-- Phase 3a determinism: the same Lua natively (physics3_tests) and in V8 (mb play).
ground = Terrain{ noise = { seed = 4 }, height = 50, detail = 2, stamps = { { "hill", 0, -50, radius = 15, height = 3 } } }
local car = VehicleBody3D{ shape = BoxShape3D{ w = 1.7, h = 0.45, d = 3.2 }, mass = 450, x = 0, y = ground:heightAt(0, 0) + 1.4, z = 0 }
for _, p in ipairs({ { -0.95, -1.15 }, { 0.95, -1.15 }, { -0.95, 1.2 }, { 0.95, 1.2 } }) do
  VehicleWheel3D{ parent = car, x = p[1], y = -0.1, z = p[2], wheelRadius = 0.46, wheelRestLength = 0.4, suspensionTravel = 0.35,
    suspensionStiffness = 28, suspensionMaxForce = 30000, dampingCompression = 2.2, dampingRelaxation = 3.0, wheelFrictionSlip = 2.0,
    useAsTraction = true, useAsSteering = p[2] < 0 }
end
local things = { car }
for i = 0, 11 do
  local x, z = 30 + (i % 4) * 1.1, -20 - math.floor(i / 4) * 1.1
  things[#things + 1] = RigidBody3D{ shape = (i % 2 == 0) and BoxShape3D{ w = 1, h = 1, d = 1 } or SphereShape3D{ radius = 0.5 },
    mass = 5 + i, x = x, y = ground:heightAt(x, z) + 2 + i * 0.7, z = z }
end
local frames, text = 0, "running"
function love.update(dt)
  frames = frames + 1
  car.engineForce = 900
  car.steering = math.sin(frames * 0.02) * 0.2
  if frames == 400 then
    local s = 0
    for i, b in ipairs(things) do s = s + i * (b.x * 3 + b.y * 5 + b.z * 7 + b.ry) end
    text = string.format("physics3 %.4f", s)
  end
end
function love.draw() love.graphics.print(text, 8, 8) end
"##;
