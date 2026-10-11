# Physics (`maimbrain::physics2d`, `maimbrain::physics3d`)

Deterministic rigid-body physics for Maimbrain games: stacking, toppling, ragdolls, marbles, cars, launchers, destruction. It's [Rapier](https://rapier.rs) compiled into the game's wasm, behind an API sized for games and agents. SPEC §5.4 links here; this document is normative for what the SDK guarantees.

Demos: `games/topple` (2D: a crane stacker with breakable mortar) and `games/knockdown` (3D: a can toss).

## Turning it on

Physics is an opt-in Cargo feature of the SDK, because it adds 0.4–0.75 MB to `game.wasm` (see Budgets). In the game's `Cargo.toml`:

```toml
[dependencies]
maimbrain = { path = "../../sdk/maimbrain", features = ["physics2d"] }   # or "physics3d"
# outside the monorepo: maimbrain = { git = "https://github.com/terracatta/maimbrain-kit", features = ["physics2d"] }
```

Nothing changes in `manifest.toml`: physics imports nothing from the host. 3D games still declare `stdlib = { mb3d = 1 }` to draw.

## Design: why it lives in the game, not the host

| | SDK-side (chosen) | Host-side (`mb_phys_*` imports) |
|---|---|---|
| Determinism | The same wasm runs everywhere; Wasm floats are IEEE-exact except NaN bits, so identical inputs give identical bits on every device and in every replay. Proven below. | The host is native code on iOS and wasm in the browser preview; keeping two builds bit-identical (FMA, libm, SIMD, iteration order) is a permanent risk. |
| ABI and review | No ABI change, no new imports, nothing for the validator, runtime, iOS app or server to learn. | A large new ABI (bodies, shapes, joints, queries, events) to version forever. |
| Cost | +0.43–0.57 MB wasm (2D), +0.6–0.75 MB (3D); per-step CPU in the guest, under loop metering. | Small wasm, native speed. |
| Evolving | A game pins its own Rapier inside its bundle, so an old game never changes behavior under a newer runtime. | Changing the engine changes every published game's replays. |

The size cost is paid only by games that turn the feature on, and both demos stay under the 1 MiB `game.wasm` SHOULD (SPEC §1). The CPU cost is small at game scales (below). If phones show physics-heavy games running hot, a host engine can come later without touching this API.

**Rapier version.** Pinned to 0.34 with `enhanced-determinism`. Rapier 0.35+ always compiles in soft bodies, which more than doubles the wasm (a minimal 2D world: 0.37 MB on 0.34, 0.57 MB on 0.35.3, 0.99 MB on 0.36 at opt-level "s"). Upgrading changes the simulation, so it needs the golden hashes in `tools/physics-probe` updated deliberately.

## Units and coordinates

| | 2D (`physics2d`) | 3D (`physics3d`) |
|---|---|---|
| Space | Logical pixels, same as `gfx2d` and input: +y down | Meters, same as `gfx3d`: right-handed, +y up |
| Internal scale | 100 px = 1 m (`PIXELS_PER_METER`), so a 40 px crate is a 0.4 m crate | 1:1 |
| Gravity | `GRAVITY` = 981 px/s² down (`World::new()`); `World::with_gravity(Vec2::ZERO)` for top-down | 9.81 m/s² along −y |
| Angles | Radians, positive turns **clockwise on screen** (what `gfx2d::rotate` does) | `Quat` |
| Mass | Density in kg/m² (a 100×100 px box of density 1 is 1 kg) | Density in kg/m³, default 1000 (water) |
| Velocity, force, impulse | px/s, kg·px/s², kg·px/s | m/s, N, N·s |
| Types | `physics2d::Vec2` / `vec2(x, y)` | `gfx3d::Vec3`, `Quat`, `Transform` |

Sizes are full sizes (a `rect(40, 40)` is 40 px wide); 3D shapes take the same arguments as the matching `MeshData` builder (`Shape::cylinder(radius, height)` ↔ `MeshData::cylinder(radius, height, segments)`), so a body and its mesh come from the same numbers.

## Quick start

2D, inside the template's shape (sim in `sim.rs`, drawing in `render`):

```rust
use maimbrain::physics2d::{Body, Event, Joint, Shape, World, vec2};

let mut world = World::new();
world.add(Body::fixed(180.0, 620.0), Shape::rect(360.0, 40.0));                 // ground
let crate_ = world.add(Body::dynamic(180.0, 100.0).angle(0.3), Shape::rect(40.0, 40.0).friction(0.8));
let ball = world.add(Body::dynamic(100.0, 50.0).bullet(), Shape::circle(12.0).restitution(0.6));
let pin = world.add(Body::fixed(260.0, 80.0), Shape::circle(3.0));
let bob = world.add(Body::dynamic(260.0, 180.0), Shape::circle(15.0));
world.join(Joint::revolute(pin, bob, vec2(260.0, 80.0)));                         // a pendulum

// update(dt):
world.step(dt);                                        // fixed 60 Hz steps from the accumulated dt
for e in world.events() {
    match *e {
        Event::ContactBegin(c) if c.speed > 80.0 => { /* thud: volume (c.speed / 600).min(1), haptic */ }
        Event::SensorEnter { sensor, other, .. } => { /* goal, pickup, kill zone */ }
        _ => {}
    }
}
if tapped { world.apply_impulse(ball, vec2(0.0, -60.0)); }

// render():
world.with_transform(crate_, || gfx2d::rect(-20.0, -20.0, 40.0, 40.0, 0xd08c4aff));   // drawn centered on (0, 0)
// or world.draw_debug() while prototyping
```

3D, with mb3d nodes:

```rust
use maimbrain::gfx3d::{MeshData, Node, Quat, vec3};
use maimbrain::physics3d::{Body, Joint, Shape, World};

let mut world = World::new();
world.add(Body::fixed(vec3(0.0, -0.5, 0.0)), Shape::cuboid(vec3(20.0, 1.0, 20.0)));
let size = vec3(0.1, 1.0, 0.5);
let domino = world.add(Body::dynamic(vec3(0.0, 0.5, 0.0)), Shape::cuboid(size).density(600.0));
let node = Node::with_mesh(MeshData::cuboid(size).upload().unwrap(), wood).unwrap();
let marble = Shape::sphere(0.1).restitution(0.4);
let mesh = marble.mesh(16).unwrap();                    // a MeshData matching any primitive shape

// update(dt):  world.step(dt);
// render():    world.sync(domino, node); gfx3d::camera(&cam); gfx3d::render();
```

## API

The same in both modules unless noted (2D names first, 3D in parentheses).

**World.** `World::new()`, `with_gravity`, `set_gravity` (tilt games set it from `sensors::tilt()` each frame), `set_step_rate(hz)` (default 60), `set_solver_iterations(n)` (default 4; 6–8 for tall stacks and chains), `set_interpolation(bool)`.

**Stepping.** `step(dt) -> u32` runs as many fixed steps as the accumulated time allows (usually 1, sometimes 0 or 2; at most 8; time beyond that is dropped) and returns how many. `events()` lists what happened during that call. `steps()`, `alpha()`.

**Bodies.** `add(Body, Shape) -> BodyId`, `add_body`, `add_shape(body, Shape) -> ShapeId` (compound bodies), `remove`, `remove_shape`, `contains`, `bodies()` (deterministic order), `len`, `shapes(body)`, `body_of(shape)`.
- `Body::dynamic(x, y)` / `fixed` / `kinematic` (3D: `Body::dynamic(pos)`), then `.angle(r)` (`.rotation(q)`), `.velocity(..)`, `.spin(..)`, `.gravity_scale(s)`, `.damping(linear, angular)`, `.bullet()` (continuous collision for small fast things), `.always_awake()`, `.asleep()` (a lone object only, see Pitfalls), `.fixed_rotation()`, `.tag(u64)` (your own number, read back with `world.tag(id)`).
- State: `position`, `angle` (`rotation`), `pose` (`transform`), `velocity`, `spin`, `mass`, `velocity_at(id, point)`, `kind`, `is_sleeping`, `awake_count()` (0 = everything settled).
- Changes: `set_position`, `set_angle` (`set_rotation`) (teleports), `set_velocity`, `set_spin`, `set_kind(id, BodyKind::Fixed)` (freeze a placed block), `set_gravity_scale`, `set_damping`, `set_fixed_rotation`, `wake`, `sleep`, `set_tag`.
- Pushing: `apply_impulse(id, v)` and `apply_impulse_at(id, v, point)` (instant, mass-aware), `add_velocity(id, dv)` (instant, mass-independent: jumps, launches), `apply_angular_impulse`, and `apply_force(id, f)` / `apply_force_at` / `apply_torque`: **call these every frame you want to push**. A force is applied as `f × dt` of impulse over the step(s) that follow, so a push is the same at 60 Hz, 120 Hz or in a frame that runs no step.
- Kinematic bodies: `move_kinematic(id, pos, angle)` (`pos, rot`) every frame drives it there over the next step, pushing what's in the way (paddles, platforms, a crane trolley); it stops at the first step without a target. Or `set_velocity` for constant motion.

**Shapes and materials.** 2D: `Shape::rect(w, h)`, `round_rect(w, h, r)`, `circle(r)`, `capsule(r, height)`, `capsule_between(a, b, r)`, `convex(&points)`, `regular_polygon(sides, r)`, `segment(a, b)`, `polyline(&points)` (terrain), `trimesh(&verts, &tris)`. 3D: `Shape::cuboid(size)`, `cube(s)`, `round_cuboid(size, r)`, `sphere(r)`, `capsule(r, height)`, `cylinder(r, height)`, `cone(r, height)`, `convex(&points)`, `convex_hull_of(&MeshData)`, `trimesh(&MeshData)` (fixed scenery), `shape.mesh(detail)` (a matching `MeshData` for primitives). Modifiers: `.at(offset)`, `.rotated(..)`, `.density(d)` or `.mass(kg)`, `.friction(f)` (default 0.5), `.restitution(r)` (default 0), `.sensor()`, `.groups(member, collides_with)` (default: layer 1, colliding with all), `.quiet()` (no events). Later: `set_friction`, `set_restitution`, `set_sensor`. Degenerate shapes (a hull of collinear points, an empty mesh) log a warning and become a tiny ball instead of panicking.

**Joints.** Anchors and axes are world points/directions where the bodies are *now*; create joints after placing bodies. `join(Joint) -> JointId`, `unjoin`, `has_joint`, `joint_bodies`, `joints()`.
- `Joint::revolute(a, b, anchor)` (3D: `+ axis`): wheels, doors, pendulums. `Joint::prismatic(a, b, anchor, axis)`: sliders, pistons. `Joint::fixed(a, b)`: glue (breakable, below). `Joint::rope(a, b, anchor_a, anchor_b)`: never longer than now (or `.length(l)`). `Joint::spring(a, b, anchor_a, anchor_b)`: rests at its current length (or `.length(l)`), tuned with `.stiffness(hz, damping_ratio)`, mass-independent. 3D also: `Joint::spherical(a, b, anchor)` (ragdoll shoulders, chains).
- `.limits(min, max)` (radians relative to now, or distance along the axis), `.motor(speed, max_force)` (`f32::INFINITY` for unlimited), `.collide(true)` (joined bodies don't collide with each other by default). Later: `set_motor`.
- Breakable joints: `joint_force(id)` and `joint_torque(id)` report how hard the joint pulled and twisted in its last step; `unjoin` past a limit (Topple's mortar).

**Events** (`world.events()`, cleared by every `step`). `Event::ContactBegin(Contact)` with `a`, `b`, `shape_a`, `shape_b`, `point`, `normal` (from `a` to `b`), `speed` (how fast they were closing just before touching: mass-independent, use it for sound volume and haptics; resting contacts are ~0) and `impulse`; `Event::ContactEnd { a, b, .. }`; `Event::SensorEnter { sensor, other, .. }` / `SensorExit`. Helpers: `involves(id)`, `bodies()`, `Contact::other(id)`. Every shape reports events unless `.quiet()`. A removed body's contacts end silently.

**Queries** (see bodies added or moved this frame at once). `raycast(from, to) -> Option<RayHit>` (`body`, `shape`, `point`, `normal`, `distance`; sensors ignored), `raycast_ignoring(from, to, body)`, `bodies_at(point)` (2D: "what did the player tap?"), `body_at(point)` (2D: the dynamic one first), `bodies_in_rect(min, max)` (2D), `contacts(id)`, `touching(a, b)`, `overlapping(sensor)`.

**Drawing.** Draw where `draw_pose(id)` (3D: `draw_transform(id)`) says: the body interpolated between its last two steps by `alpha()`, so motion is smooth at any frame rate (the preview runs at 120 Hz; phones at 60 or 120). 2D: `with_transform(id, || ...)` pushes the pose onto gfx2d so you draw centered on (0, 0); `draw_body(id, rgba)` fills a body's shapes; `draw_debug()` draws everything (dynamic orange, sleeping grey, fixed blue-grey, kinematic green, sensors faint, joints yellow). 3D: `sync(id, node)` / `sync_scaled(id, node, scale)` set a node's transform (world space: leave the node unparented). Draw calls read the world only; never change it in `render`.

**Escape hatch.** `world.rapier()` gives the `rapier2d`/`rapier3d` `PhysicsWorld` underneath (meters; 2D is +y down) for anything not covered. `state_hash()` hashes every body's bits.

## Determinism

What the SDK guarantees: the same sequence of calls with the same arguments (the same `dt`s, the same impulses at the same frames) produces bit-identical bodies, events and draw output on every device, in every replay, and natively in `cargo test`.

How:
- Rapier with `enhanced-determinism`: libm math everywhere (no platform intrinsics), deterministic hash maps (insertion-ordered, fixed hasher), no threads, no SIMD lanes that change results.
- **Fixed steps.** `step(dt)` accumulates the recorded `dt` (in f64) and runs whole steps of exactly 1/60 s. The step count depends only on the `dt` sequence, which the host records and replays.
- No clocks, no randomness, no iteration over unordered containers inside the SDK; events come out in Rapier's deterministic order; `bodies()`, `joints()` and query results are in creation-slot order.
- The SDK's own trig goes through libm, so a native `cargo test` matches the wasm run on a phone bit for bit.

What games must do (the same rules as SPEC §3, plus):
- Step the world only in `update`, with `update`'s `dt`. Never step, add, remove or push in `render`.
- Build worlds in a deterministic order (spawn from your seeded `Rng`, never from a `HashMap` iteration). Don't keep `BodyId`s in a `HashMap` you iterate; use a `Vec` or `BTreeMap`.
- Apply input-driven forces and impulses in `update`, when the input arrives.
- Sound and haptics from events are output only: fine.

Evidence:
- `cargo test -p maimbrain --features physics2d,physics3d`: busy scenes (pyramids, mixed shapes, joint chains, springs, motors, kinematic paddles, sensors, scripted kicks and forces, jittery `dt`) run twice for 3000–5000 frames; every frame's state hash, every event and every body's bits match.
- `tools/physics-probe`: the same scripted 2D and 3D scenes natively (aarch64, `cargo test -p physics-probe`, pinned golden hashes) and as wasm built like `mb build` does (opt-level 3, LTO, wasm-opt), plain and loop-metered, under V8 (node) and JavaScriptCore (`jsc`, WKWebView's engine): `tools/physics-probe/run.sh` checks that all of them produce the golden hashes. They do.
- `mb.verify()` in the preview: Topple matched over a 4305-frame held session (several rounds and a game over) and an 18 s live session at 120 Hz with real, jittery frame times; Knockdown over a 1310-frame held session and a 16.6 s live 120 Hz session.

## Budgets

**Size** (`game.wasm` after `mb build`'s wasm-opt; a template-sized game with a minimal world):

| | opt-level 3 (monorepo games) | opt-level "s" (`mb new` games) | gzip -9 (s) |
|---|---|---|---|
| no physics | 21 KB | 21 KB | 10 KB |
| `physics2d` | 575 KB | 449 KB | 192 KB |
| `physics3d` | 770 KB | 620 KB | 263 KB |
| both | 1246 KB | 983 KB | 411 KB |

Topple (2D, full game) is 647 KB, Knockdown (3D) 850 KB: under the 1 MiB SHOULD. Enabling both features crosses it in the monorepo profile; pick one. Use opt-level "s" (what `mb new` writes) if a game gets close.

**CPU** (one 60 Hz step while a pile of mixed bodies drops into a bin and piles up, everything awake and colliding; M1 Pro, wasm, loop-metered):

| Bodies | 2D V8 | 2D JSC | 3D V8 | 3D JSC |
|---|---|---|---|---|
| 50 | 0.05 ms | 0.04 ms | 0.05 ms | 0.06 ms |
| 100 | 0.10 ms | 0.09 ms | 0.10 ms | 0.13 ms |
| 200 | 0.20 ms | 0.19 ms | 0.35 ms | 0.42 ms |
| 400 | 0.40 ms | 0.45 ms | | |

Loop metering costs nothing measurable here. An iPhone 13 class phone is a few times slower than an M1 Pro core; budget **≤ 150 awake 2D bodies or ≤ 80 awake 3D bodies** for `lite` 2D and `full` 3D games, which keeps physics under ~1 ms of a 16.7 ms frame with room for two steps in a slow frame. Resting bodies sleep and cost almost nothing, so a big settled scene is fine.

Keep it cheap: let things sleep (don't `.always_awake()` without need); use `polyline`/`trimesh` only for fixed scenery and convex shapes for moving things; prefer one compound body over many jointed ones; raise `set_solver_iterations` (stiffness) before `set_step_rate` (doubles CPU); remove bodies that leave the play area.

## Pitfalls

- **Scale.** Rapier is tuned for objects of 0.1–10 m. In 2D that's 10–1000 px; a 2 px grain or a 5000 px wall is outside it. In 3D, keep things above ~5 cm (give marbles CCD with `.bullet()`).
- **Mass ratios.** A 50 kg block resting on a 0.1 kg one jitters. Keep touching masses within ~10:1, or use `.mass()` to fake it.
- **Stacks built asleep float.** Sleeping bodies that were never simulated touching aren't linked: knock the bottom one out and the rest hang in the air. Build stacks awake, a hair apart, and let them settle (they fall asleep by themselves within a second). `.asleep()` is for lone objects.
- **Fast and thin.** A small body moving more than its own size per step passes through thin walls; mark it `.bullet()`.
- **Joints are made from the current pose.** Place both bodies first, then `join`. Limits are measured from that moment.
- **Forces are per frame.** `apply_force` pushes during the frame you call it; call it every frame you want to keep pushing. For one-off kicks use `apply_impulse` or `add_velocity`.
- **Events are per `step` call.** Read `world.events()` right after `step`; the next `step` replaces them. Collect body ids first, then remove bodies after the loop.
- **Kinematic bodies don't collide with fixed ones.** They push dynamic bodies; walls don't stop them.
- **Draw the draw pose.** Drawing `position()` instead of `draw_pose()`/`sync` judders when steps and frames don't line up (most frames on a 120 Hz display run 0 or 1 steps).

## Tools

- `tools/physics-probe/run.sh`: rebuilds the probe wasm, checks node and jsc against the native golden hashes, and prints the per-step cost table above.
- `cargo test -p topple --profile bot -- --ignored --nocapture difficulty` and the same for `knockdown`: bot-driven difficulty reports for the demos.
