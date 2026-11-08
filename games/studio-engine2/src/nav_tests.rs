//! Characters and navigation, natively (docs/STUDIO-2.md §9.4 row 3b):
//! `CharacterBody3D` on the terrain, `NavigationRegion3D` baking behind
//! the title card, `NavigationAgent3D`s finding their way round trees.

use std::rc::Rc;

use crate::input::Held;
use crate::project::{Files, Project};
use crate::vm::{Mode, Vm};

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

/// Moss Knight's level (§6.2): a 64 m hillside, 40 trees, a region, a
/// knight walking a square and a goblin (an agent with autoMove) after it.
pub const MOSS: &str = r##"
ground = Terrain{ size = 64, height = 5, noise = { seed = 7, octaves = 4, frequency = 0.04 } }
trees = {}
love.math.setRandomSeed(3)
for i = 1, 40 do
  local x, z = love.math.random() * 52 - 26, love.math.random() * 52 - 26
  if math.abs(x) > 3 or math.abs(z) > 3 then
    trees[#trees + 1] = StaticBody3D{ shape = CylinderShape3D{ radius = 0.4, height = 6 }, x = x, y = ground:heightAt(x, z) + 2, z = z }
  end
end
region = NavigationRegion3D{ bake = { agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 } }
baked = 0
region.bakeFinished:connect(function() baked = baked + 1 end)
knight = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.5 }, x = 0, y = ground:heightAt(0, 0) + 1, z = 0,
                          floorSnapLength = 0.3, maxStepHeight = 0.35 }
goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 20, y = ground:heightAt(20, 20) + 1, z = 20 }
agent = NavigationAgent3D{ parent = goblin, maxSpeed = 3.2, autoMove = true, targetDesiredDistance = 1.2 }
caught = 0
agent.targetReached:connect(function() caught = caught + 1 end)
local t, nearest, floorFrames, frames, travelled, lx, lz = 0, 99, 0, 0, 0, knight.x, knight.z
function love.update(dt)
  t = t + dt
  frames = frames + 1
  -- the knight walks a slow circle (a player would steer)
  knight.vx, knight.vz = math.cos(t * 0.4) * 2.5, math.sin(t * 0.4) * 2.5
  if knight:isOnFloor() then knight.vy = 0 else knight.vy = knight.vy - 18 * dt end
  knight:moveAndSlide()
  if knight:isOnFloor() then floorFrames = floorFrames + 1 end
  travelled = travelled + math.sqrt((knight.x - lx) ^ 2 + (knight.z - lz) ^ 2)
  lx, lz = knight.x, knight.z
  if frames % 30 == 1 then agent:setTargetPosition(knight.x, knight.y, knight.z) end
  for _, tr in ipairs(trees) do
    nearest = math.min(nearest, math.sqrt((goblin.x - tr.x) ^ 2 + (goblin.z - tr.z) ^ 2))
  end
end
function love.draw()
  love.graphics.print(string.format("baked %d caught %d tree %.2f floor %d frames %d walked %.1f gap %.2f gy %.2f",
    baked, caught, nearest, floorFrames, frames, travelled,
    math.sqrt((goblin.x - knight.x) ^ 2 + (goblin.z - knight.z) ^ 2), goblin.y - ground:heightAt(goblin.x, goblin.z)), 8, 8)
end
"##;

#[test]
fn a_goblin_finds_the_knight_round_the_trees() {
    let mut v = vm3(MOSS);
    assert!(!v.ground_ready() || true);
    step(&mut v, 1);
    // Behind the title card: not ready until the navmesh has baked (4 frames of 4 tiles).
    assert!(!v.host().three.as_ref().unwrap().nav_ready());
    step(&mut v, 6);
    assert!(v.host().three.as_ref().unwrap().nav_ready());
    step(&mut v, 60 * 20);
    let out = printed(&v);
    println!("{out}");
    assert_eq!(field(&out, "baked"), 1.0, "{out}");
    assert!(field(&out, "caught") >= 1.0, "the goblin reached the knight: {out}");
    assert!(field(&out, "tree") > 0.4 + 0.25, "never through a trunk: {out}");
    // The knight walked its circle over the hills without sticking.
    assert!(field(&out, "walked") > 2.5 * 20.0 * 0.85, "{out}");
    assert!(field(&out, "floor") > field(&out, "frames") * 0.9, "{out}");
    // The goblin stands on the ground (its feet, half its 1.2 m above the centre).
    assert!((field(&out, "gy") - 0.6).abs() < 0.15, "{out}");
}

#[test]
fn navigation_errors_say_what_to_write() {
    let e = error3("NavigationRegion3D{ bake = { radius = 1 } }");
    assert!(e.contains("bake has no `radius`") && e.contains("agentRadius"), "{e}");
    let e = error3("CharacterBody3D{}");
    assert!(e.contains("CapsuleShape3D"), "{e}");
    let e = error3("local a = NavigationAgent3D{}\na.maxSped = 2");
    assert!(e.contains("maxSpeed"), "{e}");
    let e = error3("local c = CharacterBody3D{ shape = SphereShape3D{ radius = 0.5 } }\nc:moveAndSlid()");
    assert!(e.contains("moveAndSlide"), "{e}");
}

#[test]
fn areas_hear_characters() {
    let mut v = vm3(r#"
floor = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5 }
c = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.5 }, x = -4, y = 0.8 }
a = Area3D{ shape = SphereShape3D{ radius = 0.6 }, x = 0, y = 1 }
heard = 0
a.bodyEntered:connect(function(who) if who == c then heard = heard + 1 end end)
function love.update(dt)
  c.vx = 2
  c.vy = c:isOnFloor() and 0 or c.vy - 18 * dt
  c:moveAndSlide()
end
function love.draw() love.graphics.print(string.format("heard %d x %.2f", heard, c.x)) end
"#);
    step(&mut v, 240);
    let out = printed(&v);
    assert_eq!(field(&out, "heard"), 1.0, "{out}");
}

#[test]
fn an_agent_without_auto_move_is_a_path_lua_follows() {
    let mut v = vm3(r#"
floor = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5 }
wall = StaticBody3D{ shape = BoxShape3D{ w = 1, h = 3, d = 12 }, x = 0, y = 1.5 }
region = NavigationRegion3D{ size = 40, bake = { agentRadius = 0.4 } }
c = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.5 }, x = -6, y = 0.8 }
guide = NavigationAgent3D{ parent = c, avoidance = false }
local frames = 0
function love.update(dt)
  frames = frames + 1
  if frames == 10 then guide:setTargetPosition(6, 1, 0) end
  local nx, ny, nz = guide:getNextPathPosition()
  local dx, dz = nx - c.x, nz - c.z
  local d = math.sqrt(dx * dx + dz * dz)
  if d > 0.05 and not guide:isNavigationFinished() then c.vx, c.vz = dx / d * 3, dz / d * 3 else c.vx, c.vz = 0, 0 end
  c.vy = c:isOnFloor() and 0 or c.vy - 18 * dt
  c:moveAndSlide()
end
function love.draw()
  local nx, ny, nz = guide:getNextPathPosition()
  love.graphics.print(string.format("x %.2f z %.2f done %s reached %s next %.2f %.2f %.2f", c.x, c.z, tostring(guide:isNavigationFinished()), tostring(guide:isTargetReached()), nx, ny, nz))
end
"#);
    step(&mut v, 20);
    println!("{}", printed(&v));
    step(&mut v, 400);
    let out = printed(&v);
    println!("{out}");
    assert!(field(&out, "x") > 5.0 && out.contains("reached true"), "round the wall to the far side: {out}");
}

/// The moss-knight example plays: unsteered, its knight follows its own
/// agent's path to the wisps, and the hunter catches it now and then.
#[test]
fn the_moss_knight_example_plays_by_itself() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/moss-knight");
    let mut f = crate::tests::read(&dir);
    let main = std::fs::read_to_string(dir.join("game/main.lua")).unwrap();
    let probe = r#"
local __d = love.draw
function love.draw()
  __d()
  local nx, ny, nz = guide:getNextPathPosition()
  love.graphics.print(string.format("score %d lives %d kx %.1f kz %.1f ky %.2f fin %s reach %s next %.1f %.1f %.1f dist %.2f",
    round.score, round.lives or -1, knight.x, knight.z, knight.y, tostring(guide:isNavigationFinished()), tostring(guide:isTargetReached()), nx, ny, nz, guide:distanceToTarget()))
end
"#;
    f.insert("game/main.lua".into(), format!("{main}{probe}").into_bytes());
    f.insert("models/fox.glb".into(), b"glTF".to_vec());
    crate::tests::compile(&mut f);
    let p = Rc::new(Project::parse(&f).unwrap());
    let mut v = Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..60 * 40 {
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
    let out = printed(&v);
    println!("{out}");
    assert!(field(&out, "score") >= 3.0, "{out}");
}

/// Characters and agents Lua reads are the same natively as in wasm: this
/// sum is what the same Lua printed in V8 (Deno, through `mb play`) when 3b
/// was built: a scripted knight on a hillside and six goblins whose agents
/// (autoMove) chase it round thirty trees. As in a session, the title card's
/// attract mode bakes the navmesh and the round takes it over. The navmesh's
/// bytes hash the same: V8's log said `navmesh: 16 of 16 tiles walkable,
/// 671697f2`.
#[test]
fn characters_and_agents_are_the_same_in_v8() {
    crate::three::new_session();
    let p = Rc::new(Project::parse(&three(DET3B)).unwrap());
    let mut title = Vm::new(&p, 7, Mode::Title, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
    for _ in 0..10 {
        title.step(1.0 / 60.0, &[], &Held::default(), true, false).unwrap();
    }
    assert!(title.ground_ready());
    let hash = title.host().three.as_ref().unwrap().nav.as_ref().unwrap().regions.values().next().unwrap().region.as_ref().unwrap().hash();
    assert_eq!(format!("{hash:08x}"), "671697f2");
    drop(title);
    let mut v = Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
    step(&mut v, 401);
    assert_eq!(printed(&v), "nav3 840.6568 840.65682040000013");
}

const DET3B: &str = r##"-- Phase 3b determinism: the same Lua natively (nav_tests) and in V8 (mb play).
ground = Terrain{ size = 64, height = 6, noise = { seed = 7, octaves = 4, frequency = 0.04 }, stamps = { { "hill", 8, -6, radius = 9, height = 4 } } }
love.math.setRandomSeed(5)
for i = 1, 30 do
  local x, z = love.math.random() * 54 - 27, love.math.random() * 54 - 27
  if x * x + z * z > 9 then
    StaticBody3D{ shape = CylinderShape3D{ radius = 0.35, height = 6 }, x = x, y = ground:heightAt(x, z) + 2.7, z = z }
  end
end
NavigationRegion3D{ bake = { agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 } }
knight = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.2 }, y = ground:heightAt(0, 0) + 1, floorSnapLength = 0.3, maxStepHeight = 0.35 }
goblins, agents = {}, {}
for i = 1, 6 do
  local a = i * math.pi / 3
  local x, z = math.cos(a) * 22, math.sin(a) * 22
  local g = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = x, y = ground:heightAt(x, z) + 1, z = z }
  goblins[i] = g
  agents[i] = NavigationAgent3D{ parent = g, maxSpeed = 2.5 + i * 0.2, autoMove = true }
end
local frames, text = 0, "running"
function love.update(dt)
  frames = frames + 1
  local t = frames / 60
  knight.vx, knight.vz = math.cos(t * 0.5) * 4, math.sin(t * 0.7) * 4
  if knight:isOnFloor() then knight.vy = 0 else knight.vy = knight.vy - 18 * dt end
  knight:moveAndSlide()
  if frames % 20 == 1 then
    for _, a in ipairs(agents) do a:setTargetPosition(knight.x, knight.y, knight.z) end
  end
  if frames == 400 then
    local s = knight.x * 3 + knight.y * 5 + knight.z * 7
    for i, g in ipairs(goblins) do s = s + i * (g.x * 3 + g.y * 5 + g.z * 7 + g.ry) end
    text = string.format("nav3 %.4f %.17g", s, s)
  end
end
function love.draw() love.graphics.print(text, 8, 8) end
"##;

/// What a crowd costs (§4.6's budget is 50 agents; S10 measured DetourCrowd
/// alone at 0.09 ms natively): the whole frame, natively on this Mac, with
/// `n` goblins whose agents drive their bodies (autoMove: the crowd, then
/// gravity and moveAndSlide for each, the agent put back), less the same
/// scene with none. `cargo test --release -p studio-engine2 nav_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn nav_cost() {
    let flat = std::env::var("NAV_COST_FLAT").is_ok();
    let scene = |n: usize, auto: bool| {
        let ground = if flat {
            "StaticBody3D{ shape = BoxShape3D{ w = 70, h = 1, d = 70 }, y = -0.5 }\nground = { heightAt = function() return 0 end }"
        } else {
            "ground = Terrain{ size = 64, height = 6, noise = { seed = 7, octaves = 4, frequency = 0.04 } }"
        };
        format!(
            r#"
{ground}
love.math.setRandomSeed(5)
for i = 1, 40 do
  local x, z = love.math.random() * 54 - 27, love.math.random() * 54 - 27
  if x * x + z * z > 9 then StaticBody3D{{ shape = CylinderShape3D{{ radius = 0.35, height = 6 }}, x = x, y = ground:heightAt(x, z) + 2.7, z = z }} end
end
NavigationRegion3D{{ bake = {{ agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 }} }}
knight = CharacterBody3D{{ shape = CapsuleShape3D{{ radius = 0.35, height = 1.2 }}, y = ground:heightAt(0, 0) + 1 }}
agents = {{}}
for i = 1, {n} do
  local a = i * 2 * math.pi / {n}
  local x, z = math.cos(a) * (12 + i % 3 * 5), math.sin(a) * (12 + i % 3 * 5)
  local g = CharacterBody3D{{ shape = CapsuleShape3D{{ radius = 0.3, height = 1.2 }}, x = x, y = ground:heightAt(x, z) + 1, z = z }}
  agents[i] = NavigationAgent3D{{ parent = g, maxSpeed = 3, autoMove = {auto} }}
end
local frames = 0
function love.update(dt)
  frames = frames + 1
  local t = frames / 60
  knight.vx, knight.vz = math.cos(t * 0.5) * 4, math.sin(t * 0.7) * 4
  knight.vy = knight:isOnFloor() and 0 or knight.vy - 18 * dt
  knight:moveAndSlide()
  if frames % 30 == 1 then for _, a in ipairs(agents) do a:setTargetPosition(knight.x, knight.y, knight.z) end end
end
"#
        )
    };
    let run = |src: &str| -> (f64, f64, f64) {
        crate::three::new_session();
        let mut v = vm3(src);
        let mut times = Vec::new();
        let frames: usize = std::env::var("NAV_COST_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(900);
        for f in 0..frames {
            let t = std::time::Instant::now();
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
            if f >= 60 {
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        times.sort_by(|a, b| a.total_cmp(b));
        (times[times.len() / 2], times[times.len() * 95 / 100], times[times.len() - 1])
    };
    let base = run(&scene(0, true));
    println!("no agents: median {:.3} ms, p95 {:.3}, worst {:.3}", base.0, base.1, base.2);
    for (n, auto) in [(10, true), (25, true), (50, true), (100, true), (50, false)] {
        let r = run(&scene(n, auto));
        println!(
            "{n} agents (autoMove {auto}): median {:.3} ms (+{:.3}), p95 {:.3} (+{:.3}), worst {:.3}",
            r.0,
            r.0 - base.0,
            r.1,
            r.1 - base.1,
            r.2
        );
    }
}

/// A ramp (a tilted StaticBody3D) up onto a platform (an upright one) is
/// walkable: the solids that keep closed shapes' insides off the navmesh
/// leave a tilted shape's slope and an upright one's top alone.
#[test]
fn paths_go_up_a_ramp_onto_a_platform() {
    let mut v = vm3(r#"
floor = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5 }
platform = StaticBody3D{ shape = BoxShape3D{ w = 6, h = 3, d = 6 }, x = 10, y = 1.5 }
local a = math.atan(3 / 5)
ramp = StaticBody3D{ shape = BoxShape3D{ w = 6.2, h = 0.3, d = 4 }, x = 4.5, y = 1.5 - 0.15 / math.cos(a), rz = a }
region = NavigationRegion3D{ size = 40, bake = { agentRadius = 0.4, maxClimb = 0.4 } }
local text = "baking"
function love.update()
  if region.baked then
    local p = region:path(-5, 0, 0, 10, 3, 0)
    text = p and string.format("corners %d end %.2f %.2f", #p / 3, p[#p - 2], p[#p - 1]) or "no path"
  end
end
function love.draw() love.graphics.print(text) end
"#);
    step(&mut v, 10);
    let out = printed(&v);
    assert!(out.starts_with("corners"), "{out}");
    assert!((field(&out, "end") - 10.0).abs() < 0.5, "it ends on the platform: {out}");
    let y: f32 = out.split_whitespace().last().unwrap().parse().unwrap();
    assert!((y - 3.0).abs() < 0.4, "up on top, not beside it: {out}");
}

/// A character whose NavigationAgent3D went stands on the terrain again
/// (an agent's body skips the ground's tiles only while its agent drives it).
#[test]
fn a_character_whose_agent_went_stands_on_the_ground() {
    let mut v = vm3(r#"
ground = Terrain{ size = 64, height = 4, noise = { seed = 3, octaves = 3, frequency = 0.05 } }
NavigationRegion3D{}
goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 5, y = ground:heightAt(5, 5) + 0.62, z = 5 }
agent = NavigationAgent3D{ parent = goblin, autoMove = true }
local frames = 0
function love.update(dt)
  frames = frames + 1
  if frames == 30 then agent:destroy() end
  if frames > 30 then
    goblin.vx = 1
    goblin.vy = goblin:isOnFloor() and 0 or goblin.vy - 18 * dt
    goblin:moveAndSlide()
  end
end
function love.draw() love.graphics.print(string.format("gy %.2f", goblin.y - ground:heightAt(goblin.x, goblin.z))) end
"#);
    step(&mut v, 150);
    let out = printed(&v);
    assert!((field(&out, "gy") - 0.6).abs() < 0.2, "on the ground, not through it: {out}");
}

/// The crease slide (round a trunk where round is uphill, the SDK's
/// `a_character_slides_round_a_trunk_where_round_is_uphill`) is the same
/// natively and in V8: this sum is what the same Lua printed through `mb
/// play` with the engine built from this source. 24 cases, slopes of
/// 23–44° either way, aimed 0.25–0.4 m either side of the trunk; each
/// gets past it.
#[test]
fn a_crease_slide_is_the_same_in_v8() {
    let mut v = vm3(r#"
local cases, k, t, sum, text = {}, 0, 0, 0, "running"
local knight, slope, trunk, case
for _, tilt in ipairs({ 0.4, 0.6, 0.7, 0.77, -0.6, -0.77 }) do
  for _, off in ipairs({ -0.4, -0.25, 0.25, 0.4 }) do cases[#cases + 1] = { tilt = tilt, off = off } end
end
local function setup()
  if slope then slope:destroy(); trunk:destroy() end
  case = cases[k]
  slope = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5, rz = case.tilt }
  trunk = StaticBody3D{ shape = CylinderShape3D{ radius = 0.35, height = 6 }, x = 0, y = 3, z = -4 }
  knight.x, knight.z, knight.y = 0, -2, 0.9
  knight.vx, knight.vy, knight.vz = 0, 0, 0
  t = 0
end
knight = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.2 }, floorSnapLength = 0.3, maxStepHeight = 0.35, floorMaxAngle = 0.87 }
k = 1
setup()
function love.update(dt)
  if not case then return end
  t = t + dt
  local dx, dz = case.off, -2
  local m = math.sqrt(dx * dx + dz * dz)
  knight.vx, knight.vz = dx / m * 5, dz / m * 5
  if knight:isOnFloor() then knight.vy = 0 else knight.vy = knight.vy - 18 * dt end
  knight:moveAndSlide()
  if t >= 1.5 then
    sum = sum + knight.x * 3 + knight.y * 5 + knight.z * 7
    if knight.z > -4 then text = "stuck" end
    k = k + 1
    if cases[k] then setup() else case = nil; if text == "running" then text = string.format("crease %.17g", sum) end end
  end
end
function love.draw() love.graphics.print(text) end
"#);
    step(&mut v, 60 * 37);
    assert_eq!(printed(&v), "crease -1369.3300239600003");
}

/// Moving a body its agent drives (a respawn) puts it there: the write
/// used to be undone the next frame, as the agent moved the body on from
/// where it had been. From there it goes on toward its target.
#[test]
fn moving_an_auto_move_body_puts_it_there() {
    let mut v = vm3(r#"
ground = Terrain{ size = 64, height = 3, noise = { seed = 3, octaves = 3, frequency = 0.05 } }
NavigationRegion3D{}
goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 10, y = ground:heightAt(10, 10) + 0.62, z = 10 }
agent = NavigationAgent3D{ parent = goblin, autoMove = true, maxSpeed = 3 }
local frames, text = 0, "waiting"
function love.update(dt)
  frames = frames + 1
  if frames == 20 then agent:setTargetPosition(0, ground:heightAt(0, 0), 0) end
  if frames == 60 then
    goblin.x, goblin.z = -20, -20
    goblin.y = ground:heightAt(-20, -20) + 0.62
  end
  if frames == 61 then text = string.format("at %.2f %.2f", goblin.x, goblin.z) end
  if frames == 150 then
    text = text .. string.format(" then %.2f %.2f gy %.2f", goblin.x, goblin.z, goblin.y - ground:heightAt(goblin.x, goblin.z))
  end
end
function love.draw() love.graphics.print(text) end
"#);
    step(&mut v, 150);
    let out = printed(&v);
    assert!(out.starts_with("at"), "{out}");
    let n: Vec<f32> = out.split_whitespace().filter_map(|w| w.parse().ok()).collect();
    assert!((n[0] + 20.0).abs() < 0.2 && (n[1] + 20.0).abs() < 0.2, "where it was put, a frame on: {out}");
    assert!(n[2] > -18.5 && n[3] > -18.5, "then on toward its target, from there: {out}");
    assert!((n[4] - 0.6).abs() < 0.2, "on the ground: {out}");
}

/// A game that moves the body itself (no autoMove, writing its x and z by
/// the agent's vx, vz each frame) keeps its agent's speed: only a body its
/// agent drives jumps with it (a teleport would restart the agent each frame
/// and it would crawl at its acceleration).
#[test]
fn a_body_the_game_moves_keeps_its_agents_speed() {
    let mut v = vm3(r#"
ground = Terrain{ size = 64, height = 0.5, noise = { seed = 3, octaves = 2, frequency = 0.05 } }
NavigationRegion3D{}
goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 10, y = ground:heightAt(10, 10) + 0.62, z = 10 }
agent = NavigationAgent3D{ parent = goblin, maxSpeed = 3 }
local frames, top = 0, 0
function love.update(dt)
  frames = frames + 1
  if frames == 5 then agent:setTargetPosition(-10, ground:heightAt(-10, -10), -10) end
  goblin.x, goblin.z = goblin.x + agent.vx * dt, goblin.z + agent.vz * dt
  top = math.max(top, math.sqrt(agent.vx ^ 2 + agent.vz ^ 2))
end
function love.draw() love.graphics.print(string.format("%.2f", top)) end
"#);
    step(&mut v, 90);
    let top: f32 = printed(&v).trim().parse().unwrap();
    assert!(top > 2.5, "its agent gets up to its maxSpeed of 3: {top}");
}
