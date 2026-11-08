//! Physics, natively (docs/STUDIO-2.md §9.4 row 3a): `love.physics` on
//! Rapier 2D with LÖVE's names, units and callbacks, nodes that follow
//! bodies; and Godot's 3D bodies, joints, raycasts, the terrain's
//! heightfield and VehicleBody3D (physics3_tests below).

use std::rc::Rc;

use crate::input::Held;
use crate::project::{Files, Project};
use crate::tests::with_main;
use crate::vm::{Mode, Vm};

fn vm(files: &Files) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files).unwrap());
    let im: Vec<_> = p.images.iter().map(crate::scene_image).collect();
    Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0))
}

fn step(v: &mut Vm, n: usize, dt: f32) {
    for _ in 0..n {
        v.step(dt, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

fn error_of(main: &str) -> String {
    match vm(&with_main(main, &[])) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// What a frame costs natively, Lua-heavy (300 nodes moved and sorted from
/// Lua, 1,500 `love.draw` rectangles) and physics-heavy (150 love.physics
/// boxes piling up), to compare builds (docs/STUDIO-2.md §10.1 risk 2, the
/// engine's Rust at another opt-level): `cargo test -p studio-engine2
/// --release --lib engine_cost -- --ignored --nocapture`, and with
/// `--config 'profile.release.opt-level="s"' --config
/// 'profile.release.package.luau.opt-level=3'` for the other. The 3D
/// physics is `physics_cost` (physics3_tests.rs).
#[test]
#[ignore]
fn engine_cost() {
    let lua = r#"
local things = {}
for i = 1, 300 do
  local s = Shape{ x = i, y = i * 2, w = 6, h = 6 }
  things[i] = { node = s, vx = math.sin(i), vy = math.cos(i), d = 0 }
end
function love.update(dt)
  for i = 1, #things do
    local t = things[i]
    local n = t.node
    n.x = (n.x + t.vx * 60 * dt) % 360
    n.y = (n.y + t.vy * 60 * dt) % 640
    t.d = math.sqrt(n.x * n.x + n.y * n.y)
  end
  table.sort(things, function(a, b) return a.d < b.d end)
end
function love.draw()
  for i = 1, 1500 do love.graphics.rectangle("fill", (i * 7) % 360, (i * 13) % 640, 4, 4) end
end
"#;
    let phys = r#"
love.physics.setMeter(64)
world = love.physics.newWorld(0, 9.81 * 64, true)
local ground = love.physics.newBody(world, 180, 630, "static")
love.physics.newFixture(ground, love.physics.newRectangleShape(360, 20))
for i = 0, 149 do
  local b = love.physics.newBody(world, 30 + (i % 15) * 20 + (math.floor(i / 15) % 2) * 6, 560 - math.floor(i / 15) * 21, "dynamic")
  love.physics.newFixture(b, love.physics.newRectangleShape(18, 18), 1)
end
function love.update(dt) world:update(dt) end
"#;
    for (name, main) in [("Lua-heavy", lua), ("physics-heavy (150 boxes)", phys)] {
        let mut v = vm(&with_main(main, &[])).unwrap();
        let mut times = Vec::new();
        for f in 0..600 {
            let t = std::time::Instant::now();
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
            if f >= 60 {
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        println!("{name}: mean {mean:.3} ms, median {:.3} ms, p95 {:.3} ms a frame", times[times.len() / 2], times[times.len() * 95 / 100]);
    }
}

/// A crate stack as LÖVE code writes it: setMeter, a world with gravity,
/// static ground, dynamic boxes with friction, user data, callbacks with
/// contacts, a weld made after the step, sleep, and a sprite that follows
/// a body.
const STACK: &str = r##"
love.physics.setMeter(64)
world = love.physics.newWorld(0, 9.81 * 64, true)
local begins, posts, ends = 0, 0, 0
local firstNormal, firstPoints
local toWeld = {}
local welded
world:setCallbacks(function(a, b, c)
  begins = begins + 1
  if not firstNormal then
    firstNormal = { c:getNormal() }
    firstPoints = select("#", c:getPositions())
  end
  local ua, ub = a:getUserData(), b:getUserData()
  if ua == "crate" and ub == "crate" and not welded then
    local x, y = c:getPositions()
    toWeld[#toWeld + 1] = { a:getBody(), b:getBody(), x, y }
  end
end, function() ends = ends + 1 end, nil, function(a, b, c, ni) if ni > 0 then posts = posts + 1 end end)

local ground = love.physics.newBody(world, 180, 600, "static")
local gf = love.physics.newFixture(ground, love.physics.newRectangleShape(360, 40))
gf:setFriction(0.9)

crates = {}
for i = 1, 4 do
  local b = love.physics.newBody(world, 180, 560 - (i - 1) * 50, "dynamic")
  local f = love.physics.newFixture(b, love.physics.newRectangleShape(48, 48), 1)
  f:setFriction(0.8)
  f:setUserData("crate")
  crates[i] = b
end
sprite = Shape{ shape = "rect", w = 48, h = 48, color = 0xd08c4a }
sprite.body = crates[4]

local t = 0
function love.update(dt)
  world:update(dt)
  for _, w in ipairs(toWeld) do
    if not welded then welded = love.physics.newWeldJoint(w[1], w[2], w[3], w[4]) end
  end
  toWeld = {}
  t = t + dt
end

function love.draw()
  local x, y = crates[4]:getPosition()
  local awake = 0
  for i = 1, #crates do if crates[i]:isAwake() then awake = awake + 1 end end
  local wx, wy = crates[1]:getWorldPoints(love.physics.newRectangleShape(48, 48):getPoints())
  love.graphics.print(string.format("top %.1f %.1f awake %d begins %d posts %s ends %d weld %s mass %.3f n %.2f,%.2f pts %d sprite %.1f,%.1f type %s wp %.1f,%.1f",
    x, y, awake, begins, tostring(posts > 0), ends, tostring(welded ~= nil), crates[1]:getMass(),
    firstNormal and firstNormal[1] or 0, firstNormal and firstNormal[2] or 0, firstPoints or 0, sprite.x, sprite.y,
    gf:getBody():getType(), wx, wy))
end
"##;

#[test]
fn a_love_physics_stack_settles_welds_and_sleeps() {
    let mut v = vm(&with_main(STACK, &[])).unwrap();
    step(&mut v, 300, 1.0 / 60.0);
    let out = printed(&v).join("\n");
    // Settled: the top crate's centre is 3.5 crates over the ground's top
    // (580 − 24 − 3 × 48 = 412, within a few px for Rapier's skin), asleep.
    let y: f32 = out.split_whitespace().nth(2).unwrap().parse().unwrap();
    assert!((y - 412.0).abs() < 4.0, "{out}");
    assert!(out.contains("awake 0"), "{out}");
    assert!(out.contains("posts true"), "{out}");
    assert!(out.contains("weld true"), "{out}");
    // 48 px at 64 px a meter is 0.75 m: 0.5625 kg at density 1.
    assert!(out.contains("mass 0.562"), "{out}");
    // The ground's normal points from it up to the crate: (0, −1) in LÖVE's
    // y-down screen, or the reverse for the other order.
    assert!(out.contains("n 0.00,-1.00") || out.contains("n 0.00,1.00") || out.contains("n -0.00,-1.00") || out.contains("n -0.00,1.00"), "{out}");
    assert!(out.contains("type static"), "{out}");
    // The sprite follows the body.
    let x: f32 = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(out.contains(&format!("sprite {:.1},{:.1}", x, y)), "{out}");
    // World points: the bottom crate's corner, at the body's place.
    assert!(out.contains("wp 156.0,532.") || out.contains("wp 156.0,531.9"), "{out}");
}

/// The same stack replays to the same bits, at a phone's uneven 120 Hz.
#[test]
fn love_physics_replays_exactly() {
    let run = || {
        let mut v = vm(&with_main(STACK, &[])).unwrap();
        for i in 0..400u32 {
            let dt = 1.0 / 120.0 + ((i.wrapping_mul(2654435761) >> 20) as f32 / 4096.0 - 0.5) * 0.002;
            v.step(dt, &[], &Held::default(), false, false).unwrap();
        }
        printed(&v)
    };
    assert_eq!(run(), run());
}

/// Joints: a revolute pendulum swings, its limits hold, a rope never
/// stretches past its length, a wheel spins with its motor, a prismatic
/// slides on its axis, and a destroyed body takes its joints with it.
#[test]
fn love_physics_joints() {
    let src = r#"
    love.physics.setMeter(50)
    world = love.physics.newWorld(0, 500)
    local anchor = love.physics.newBody(world, 100, 100, "static")
    local bob = love.physics.newBody(world, 200, 100, "dynamic")
    love.physics.newFixture(bob, love.physics.newCircleShape(10))
    local rev = love.physics.newRevoluteJoint(anchor, bob, 100, 100)
    rev:setLimits(-0.5, 0.5)
    local a2 = love.physics.newBody(world, 300, 100, "static")
    local hang = love.physics.newBody(world, 300, 150, "dynamic")
    love.physics.newFixture(hang, love.physics.newRectangleShape(20, 20))
    local rope = love.physics.newRopeJoint(a2, hang, 300, 100, 300, 150, 80)
    local car = love.physics.newBody(world, 200, 400, "dynamic")
    love.physics.newFixture(car, love.physics.newRectangleShape(60, 10))
    car:setGravityScale(0)
    local wheel = love.physics.newBody(world, 200, 420, "dynamic")
    love.physics.newFixture(wheel, love.physics.newCircleShape(10))
    wheel:setGravityScale(0)
    local wj = love.physics.newWheelJoint(car, wheel, 200, 420, 0, 1)
    wj:setMotorEnabled(true); wj:setMotorSpeed(5); wj:setMaxMotorTorque(1e6)
    local s1 = love.physics.newBody(world, 50, 500, "static")
    local slider = love.physics.newBody(world, 50, 500, "dynamic")
    love.physics.newFixture(slider, love.physics.newRectangleShape(10, 10))
    local pj = love.physics.newPrismaticJoint(s1, slider, 50, 500, 0, 1)
    local doomed = love.physics.newBody(world, 0, 0, "dynamic")
    love.physics.newFixture(doomed, love.physics.newCircleShape(5))
    local dj = love.physics.newDistanceJoint(anchor, doomed, 100, 100, 0, 0)
    local maxAngle, maxRope, minWheelSpin = 0, 0, 0
    function love.update(dt)
      world:update(dt)
      maxAngle = math.max(maxAngle, math.abs(rev:getJointAngle()))
      local hx, hy = hang:getPosition()
      maxRope = math.max(maxRope, math.sqrt((hx - 300) ^ 2 + (hy - 100) ^ 2))
      if round.time > 1 and not doomed:isDestroyed() then doomed:destroy() end
    end
    function love.draw()
      local sx, sy = slider:getPosition()
      love.graphics.print(string.format("angle %.2f rope %.1f spin %.1f slide %.1f,%.0f dj %s joints %d %s",
        maxAngle, maxRope, wheel:getAngularVelocity(), sx, sy, tostring(dj:isDestroyed()), world:getJointCount(), rev:getType()))
    end
    "#;
    let mut v = vm(&with_main(src, &[])).unwrap();
    step(&mut v, 120, 1.0 / 60.0);
    let out = printed(&v).join("\n");
    let f = |k: &str| -> f32 { out.split_whitespace().skip_while(|w| *w != k).nth(1).unwrap().split(',').next().unwrap().parse().unwrap() };
    assert!(f("angle") < 0.56, "the revolute's limits hold: {out}");
    assert!(f("rope") < 81.0, "a rope never stretches: {out}");
    assert!(f("spin") > 4.0, "the wheel's motor turns it: {out}");
    assert!((f("slide") - 50.0).abs() < 0.5, "a prismatic slides only on its axis: {out}");
    assert!(out.contains("dj true"), "a destroyed body's joints go: {out}");
    assert!(out.contains("joints 4 revolute"), "{out}");
}

/// rayCast and queryBoundingBox call back in Lua; destroying a body inside
/// a callback is an error with Box2D's rule; a contact kept past its
/// callback says so; categories and masks filter.
#[test]
fn love_physics_queries_rules_and_filters() {
    let src = r#"
    world = love.physics.newWorld(0, 0)
    local a = love.physics.newBody(world, 100, 100, "static")
    love.physics.newFixture(a, love.physics.newCircleShape(20)):setUserData("a")
    local b = love.physics.newBody(world, 200, 100, "static")
    love.physics.newFixture(b, love.physics.newRectangleShape(20, 20)):setUserData("b")
    hits, found = {}, 0
    world:rayCast(0, 100, 400, 100, function(f, x, y, nx, ny, fr) hits[#hits + 1] = f:getUserData() .. string.format("@%.0f", x) return 1 end)
    world:queryBoundingBox(150, 50, 250, 150, function(f) found = found + 1 return true end)
    -- Categories: a ghost on 2 that masks 1 passes the floor.
    local floor = love.physics.newBody(world, 180, 400, "static")
    love.physics.newFixture(floor, love.physics.newRectangleShape(360, 20))
    world:setGravity(0, 300)
    ghost = love.physics.newBody(world, 180, 300, "dynamic")
    local gfx = love.physics.newFixture(ghost, love.physics.newCircleShape(10))
    gfx:setCategory(2); gfx:setMask(1)
    solid = love.physics.newBody(world, 120, 300, "dynamic")
    love.physics.newFixture(solid, love.physics.newCircleShape(10))
    function love.update(dt) world:update(dt) end
    function love.draw()
      local _, gy = ghost:getPosition()
      local _, sy = solid:getPosition()
      love.graphics.print(table.concat(hits, " ") .. " found " .. found .. string.format(" ghost %.0f solid %.0f", gy, sy) .. " cat " .. gfx:getCategory())
    end
    "#;
    let mut v = vm(&with_main(src, &[])).unwrap();
    step(&mut v, 90, 1.0 / 60.0);
    let out = printed(&v).join("\n");
    assert!(out.starts_with("a@80 b@190 found 1"), "{out}");
    let f = |k: &str| -> f32 { out.split_whitespace().skip_while(|w| *w != k).nth(1).unwrap().parse().unwrap() };
    assert!(f("ghost") > 420.0, "the ghost falls through the floor: {out}");
    assert!((f("solid") - 380.0).abs() < 3.0, "the other lands on it: {out}");
    assert!(out.ends_with("cat 2"), "{out}");

    let e = error_of(
        r#"
        world = love.physics.newWorld(0, 500)
        local g = love.physics.newBody(world, 0, 100, "static")
        love.physics.newFixture(g, love.physics.newRectangleShape(400, 10))
        local b = love.physics.newBody(world, 0, 80, "dynamic")
        love.physics.newFixture(b, love.physics.newCircleShape(10))
        world:setCallbacks(function(x, y) y:getBody():destroy() end)
        function love.update(dt) for i = 1, 30 do world:update(1 / 60) end end
        "#,
    );
    assert!(e.contains("Box2D's rule") && e.contains("game/main.lua:7"), "{e}");
    let e = error_of(
        r#"
        world = love.physics.newWorld(0, 500)
        local g = love.physics.newBody(world, 0, 100, "static")
        love.physics.newFixture(g, love.physics.newRectangleShape(400, 10))
        local b = love.physics.newBody(world, 0, 89, "dynamic")
        love.physics.newFixture(b, love.physics.newCircleShape(10))
        local kept
        world:setCallbacks(function(x, y, c) kept = c end)
        function love.update(dt) for i = 1, 10 do world:update(1 / 60) end kept:getNormal() end
        "#,
    );
    assert!(e.contains("this contact is gone"), "{e}");
    assert!(error_of("love.physics.newBody(1, 2, 3)").contains("World expected"));
    assert!(error_of("local w = love.physics.newWorld() love.physics.newBody(w, 0, 0, 'wobbly')").contains("isn't a body type"));
    assert!(error_of("local w = love.physics.newWorld() local b = love.physics.newBody(w) b:setLinearVelocty(1, 2)").contains("did you mean `setLinearVelocity`"));
}

/// LÖVE's numbers where Box2D's rules set them: `setMass` scales every
/// fixture alike (the centre of mass stays); `getAnchors` is where the
/// bodies carry them now; a mouse joint pulls with 1000 × the body's mass
/// whatever the gravity; a frictionless fixture stays frictionless against
/// a rough one; a wheel joint's limits are pixels, as a prismatic's.
#[test]
fn love_physics_numbers_follow_box2d() {
    let src = r#"
    love.physics.setMeter(64)
    world = love.physics.newWorld(0, 0)
    -- setMass: a small circle and a big one, off centre.
    local b = love.physics.newBody(world, 100, 100, "dynamic")
    love.physics.newFixture(b, love.physics.newCircleShape(-20, 0, 8))
    love.physics.newFixture(b, love.physics.newCircleShape(30, 0, 24))
    local cx1, cy1 = b:getWorldCenter()
    b:setMass(10)
    local cx2, cy2 = b:getWorldCenter()
    -- The mouse joint's force, with no gravity.
    local box = love.physics.newBody(world, 300, 300, "dynamic")
    love.physics.newFixture(box, love.physics.newRectangleShape(64, 64))
    local mj = love.physics.newMouseJoint(box, 300, 300)
    -- getAnchors: a weld between two bodies that move.
    local p = love.physics.newBody(world, 500, 100, "dynamic")
    love.physics.newFixture(p, love.physics.newRectangleShape(20, 20))
    local q = love.physics.newBody(world, 520, 100, "dynamic")
    love.physics.newFixture(q, love.physics.newRectangleShape(20, 20))
    local weld = love.physics.newWeldJoint(p, q, 510, 100)
    p:setLinearVelocity(0, 120); q:setLinearVelocity(0, 120)
    -- Friction: a frictionless puck on rough ground keeps sliding.
    local w2 = love.physics.newWorld(0, 9.81 * 64)
    local ground = love.physics.newBody(w2, 0, 500, "static")
    love.physics.newFixture(ground, love.physics.newRectangleShape(4000, 20)):setFriction(1)
    local puck = love.physics.newBody(w2, 0, 470, "dynamic")
    local pf = love.physics.newFixture(puck, love.physics.newRectangleShape(40, 40))
    pf:setFriction(0)
    puck:setLinearVelocity(300, 0)
    local mixed
    w2:setCallbacks(function(a, b2, c) mixed = c:getFriction() end)
    -- A wheel joint's limits: a weak spring, the wheel pulled down its axis.
    local w3 = love.physics.newWorld(0, 9.81 * 64)
    local car = love.physics.newBody(w3, 100, 100, "static")
    local wheel = love.physics.newBody(w3, 100, 100, "dynamic")
    love.physics.newFixture(wheel, love.physics.newCircleShape(10))
    local wj = love.physics.newWheelJoint(car, wheel, 100, 100, 0, 1)
    wj:setSpringFrequency(0.05)
    wj:setLimits(-5, 5)
    wj:setLimitsEnabled(true)
    local maxT = 0
    function love.update(dt)
      world:update(dt) w2:update(dt) w3:update(dt)
      maxT = math.max(maxT, wj:getJointTranslation())
    end
    function love.draw()
      local ax, ay, bx, by = weld:getAnchors()
      local _, py = p:getPosition()
      local vx = puck:getLinearVelocity()
      love.graphics.print(string.format("mass %.3f centre %.2f,%.2f -> %.2f,%.2f force %.1f anchors %.1f %.1f %.1f py %.1f puck %.1f mixed %s wheel %.2f",
        b:getMass(), cx1, cy1, cx2, cy2, mj:getMaxForce(), ay, by, ax, py, vx, tostring(mixed), maxT))
    end
    "#;
    let mut v = vm(&with_main(src, &[])).unwrap();
    step(&mut v, 60, 1.0 / 60.0);
    let out = printed(&v).join("");
    let f = |k: &str| -> f32 { out.split_whitespace().skip_while(|w| *w != k).nth(1).unwrap_or_else(|| panic!("{k}: {out}")).split(',').next().unwrap().parse().unwrap() };
    assert!((f("mass") - 10.0).abs() < 1e-3, "{out}");
    let centre: Vec<&str> = out.split("centre ").nth(1).unwrap().split(" force").next().unwrap().split(" -> ").collect();
    assert_eq!(centre[0], centre[1], "setMass keeps the centre of mass: {out}");
    // 1 kg (64 px at 64 px a meter), 1000 N in Box2D, × 64 in LÖVE's units.
    assert!((f("force") - 64000.0).abs() < 1.0, "{out}");
    // The weld moved down about 120 px (120 px/s for a second), its anchors with it.
    assert!(f("py") > 210.0 && (f("anchors") - f("py")).abs() < 0.01, "the weld's anchors moved with its bodies: {out}");
    assert!(f("puck") > 290.0, "a frictionless puck keeps its speed on rough ground: {out}");
    assert!(out.contains("mixed 0"), "the contact says the friction it used: {out}");
    let t = f("wheel");
    assert!(t > 4.5 && t < 5.5, "the wheel stopped at its 5 px limit: {out}");
}

/// `World:update(dt, velocityiterations)`, as LÖVE takes it, sets Rapier's
/// solver iterations: a beam of welded boxes off a wall sags less at
/// Box2D's 8 than at Rapier's default 4, and the wall's weld carries more
/// of the beam's weight (its moment is about 88,000 in LÖVE's units: ten
/// 0.06 kg boxes at 0.15 to 2.85 m), as the solver converges further.
#[test]
fn world_update_takes_velocity_iterations() {
    let run = |iters: &str| {
        let main = format!(
            r##"
love.physics.setMeter(100)
local world = love.physics.newWorld(0, 981, true)
local wall = love.physics.newBody(world, 20, 300, "static")
love.physics.newFixture(wall, love.physics.newRectangleShape(20, 200))
local last, first = wall, nil
for i = 1, 10 do
  local b = love.physics.newBody(world, 20 + i * 30, 300, "dynamic")
  love.physics.newFixture(b, love.physics.newRectangleShape(30, 20), 1)
  local j = love.physics.newWeldJoint(last, b, 5 + i * 30, 300)
  first = first or j
  last = b
end
function love.update(dt) world:update(dt{iters}) end
function love.draw() love.graphics.print(string.format("%.3f %.0f", last:getY() - 300, first:getReactionTorque(60))) end
"##
        );
        let mut v = vm(&with_main(&main, &[])).unwrap();
        step(&mut v, 240, 1.0 / 60.0);
        let out = printed(&v).join("");
        let mut it = out.split(' ').map(|s| s.parse::<f32>().unwrap());
        (it.next().unwrap(), it.next().unwrap())
    };
    let (sag4, torque4) = run("");
    let (sag8, torque8) = run(", 8, 3");
    assert!(sag4 > 0.0 && sag8 < sag4 * 0.9, "8 iterations hold the beam stiffer: sag {sag8} px against {sag4}");
    assert!(torque8 > torque4 && (torque8 - 88_000.0).abs() < 30_000.0, "the wall's weld at 8 iterations: {torque8}, at 4: {torque4}");
}

/// The Crate Stack example (§6.3's Crate Crane, drawn with shapes) plays:
/// crates dropped from the swinging hook stack on the dock and score.
#[test]
fn crate_stack_builds_a_stack() {
    use crate::input::LoveEvent;
    let f = crate::tests::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/crate-stack"));
    let mut v = vm(&f).unwrap();
    let tap = [LoveEvent::MousePressed { x: 180.0, y: 320.0, button: 1, istouch: true }, LoveEvent::MouseReleased { x: 180.0, y: 320.0, button: 1, istouch: true }];
    let mut drops = 0;
    for frame in 0..60 * 30 {
        let t = frame as f64 / 60.0;
        let ev: &[LoveEvent] = if drops < 8 && t > 1.0 + drops as f64 * 3.0 {
            drops += 1;
            &tap
        } else {
            &[]
        };
        v.step(1.0 / 60.0, ev, &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
    let p = v.host().phys2.as_deref().unwrap();
    let w = &p.worlds[0];
    let info: Vec<String> = p.bodies.iter().map(|b| format!("{:.0?} {}", w.w.position(b.id) / w.k, w.w.is_sleeping(b.id))).collect();
    let score = v.host().last_score;
    assert!(score > 1.0, "a stack of crates scores its height: {score} m; lives {}; {info:?}", v.host().last_lives);
}

/// love.physics' bodies are the same natively as in wasm: this sum is what
/// the same Lua printed in V8 (Deno, through `mb play`) when 3a was built
/// (again after love.physics took Box2D's friction rule, which changed it).
#[test]
fn love_physics_is_the_same_in_v8() {
    let mut v = vm(&with_main(DET2, &[])).unwrap();
    step(&mut v, 401, 1.0 / 60.0);
    assert_eq!(printed(&v).join(""), "physics2 3210696.1117");
}

const DET2: &str = r##"-- Phase 3a determinism: the same Lua natively (physics_tests) and in V8 (mb play).
love.physics.setMeter(64)
local world = love.physics.newWorld(0, 9.81 * 64, true)
local g = love.physics.newBody(world, 180, 600, "static")
love.physics.newFixture(g, love.physics.newRectangleShape(360, 40))
local bodies, prev = {}, nil
for i = 1, 14 do
  local b = love.physics.newBody(world, 150 + (i % 3) * 20, 540 - i * 46, "dynamic")
  local shape = (i % 4 == 0) and love.physics.newCircleShape(20) or love.physics.newRectangleShape(40 + (i % 3) * 8, 40)
  love.physics.newFixture(b, shape, 1):setFriction(0.7)
  if prev and i % 2 == 0 then love.physics.newWeldJoint(prev, b, 170, 540 - i * 46 + 23) end
  bodies[#bodies + 1] = b
  prev = b
end
local ball = love.physics.newBody(world, -40, 420, "dynamic")
love.physics.newFixture(ball, love.physics.newCircleShape(16), 8)
ball:setBullet(true)
local frames, text = 0, "running"
function love.update(dt)
  world:update(dt)
  frames = frames + 1
  if frames == 90 then ball:setLinearVelocity(900, -60) end
  if frames == 400 then
    local s = 0
    for i, b in ipairs(bodies) do
      local x, y = b:getPosition()
      s = s + i * (x * 3 + y * 5 + b:getAngle())
    end
    text = string.format("physics2 %.4f", s)
  end
end
function love.draw() love.graphics.print(text, 8, 8) end
"##;

/// `CharacterBody2D` in a love.physics world (Phase 3b): it lands on a
/// static body, jumps up through a one-way fixture and lands on it, drops
/// through it; a crate (a dynamic body) lands on the shelf beside it.
#[test]
fn a_character_body_2d_in_a_love_physics_world() {
    let main = r#"
        love.physics.setMeter(64)
        world = love.physics.newWorld(0, 9.81 * 64, true)
        local ground = love.physics.newBody(world, 180, 620, "static")
        love.physics.newFixture(ground, love.physics.newRectangleShape(360, 40))
        local shelf = love.physics.newBody(world, 180, 505, "static")
        love.physics.newFixture(shelf, love.physics.newRectangleShape(200, 10)):setOneWay(true)
        hero = CharacterBody2D{ world = world, shape = { w = 20, h = 40 }, x = 180, y = 560, floorSnapLength = 2 }
        local crate = love.physics.newBody(world, 250, 300, "dynamic")
        love.physics.newFixture(crate, love.physics.newRectangleShape(16, 16), 1)
        frame, onShelf, back = 0, false, false
        function love.update(dt)
          frame = frame + 1
          world:update(dt)
          hero.vy = hero:isOnFloor() and 0 or hero.vy + 1800 * dt
          if frame == 40 then hero.vy = -800 end
          hero:moveAndSlide(frame >= 120 and frame < 125)
          if frame == 110 then onShelf = hero:isOnFloor() and math.abs(hero.y - 480) < 1 end
          if frame == 200 then back = hero:isOnFloor() and math.abs(hero.y - 580) < 1 end
          crateY = crate:getY()
        end
        function love.draw() love.graphics.print(string.format("%s %s %.1f %.1f %d", tostring(onShelf), tostring(back), hero.y, crateY, hero:getSlideCollisionCount())) end
    "#;
    let mut v = vm(&with_main(main, &[])).unwrap();
    step(&mut v, 201, 1.0 / 60.0);
    let out = printed(&v);
    let p: Vec<&str> = out[0].split(' ').collect();
    assert_eq!((p[0], p[1]), ("true", "true"), "up through the shelf onto it, then down through it: {out:?}");
    let crate_y: f32 = p[3].parse().unwrap();
    assert!(crate_y < 500.0, "the crate rests on the shelf (one-way is for characters): {out:?}");
}
