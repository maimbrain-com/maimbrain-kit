//! Godot's 3D physics nodes (docs/STUDIO-2.md §5 [godot], [physics]; §9.4
//! row 3a), with Godot's names and meanings and members in camelCase:
//! `RigidBody3D`, `StaticBody3D`, `Area3D`, `VehicleBody3D` with its
//! `VehicleWheel3D`s, the shapes (`BoxShape3D{ w, h, d }` and the rest), the
//! joints (`HingeJoint3D` and the rest, plus our `breakForce`), and
//! `physics.raycast`; and `CharacterBody3D` (Phase 3b): Rapier's character
//! controller with Godot's `move_and_slide` rules (the SDK's
//! `physics3d::character`). Bodies are 3D nodes: meshes and models go under
//! them as children and ride along; what runs each frame is physics3.rs.
//! The navigation nodes are nav3d.rs's, reached through here.
//!
//! ```lua
//! local ground = Terrain{}                         -- collision is on: bodies land on it
//! local crate = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, mass = 20, x = 0, y = 12, z = 0 }
//! Mesh{ geometry = geometry.box(1, 1, 1), parent = crate }
//! crate.bodyEntered:connect(function(other) fx.shake(0.2, 0.1) end)   -- with contactMonitor = true
//! crate:applyCentralImpulse(0, 200, 0)
//! ```

use std::ffi::{CStr, c_int, c_void};

use maimbrain::gfx3d::{self, Quat, Vec3, vec3};
use maimbrain::physics3d::{self as p3, BodyKind};
use maimbrain::scene::three::Node3Id;

use super::atoms::A;
use super::three::{abs, key_at, node_at, push_node, set_any, with_field};
use super::*;
use crate::physics3::{Body3, Char3, Geom3, Joint3, JointType, Phys3, Shape3, Wheel3, body_kind, trs_of};
use crate::three::{Info, Kind3, World3};

/// The body constructors and their kinds.
pub const BODIES: &[(&CStr, Kind3)] = &[
    (c"RigidBody3D", Kind3::RigidBody),
    (c"StaticBody3D", Kind3::StaticBody),
    (c"Area3D", Kind3::Area),
    (c"VehicleBody3D", Kind3::Vehicle),
    (c"VehicleWheel3D", Kind3::Wheel),
    (c"CharacterBody3D", Kind3::Character),
    (c"NavigationRegion3D", Kind3::NavRegion),
    (c"NavigationAgent3D", Kind3::NavAgent),
];

const JOINTS: &[(&CStr, JointType)] = &[
    (c"PinJoint3D", JointType::Pin),
    (c"HingeJoint3D", JointType::Hinge),
    (c"SliderJoint3D", JointType::Slider),
    (c"ConeTwistJoint3D", JointType::ConeTwist),
    (c"Generic6DOFJoint3D", JointType::Generic6DOF),
];

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        let make: [RustFn; 8] = [mk_rigid, mk_static, mk_area, mk_vehicle, mk_wheel, mk_character, mk_region, mk_agent];
        for ((name, _), f) in BODIES.iter().zip(make) {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        let joints: [RustFn; 5] = [mk_pin, mk_hinge, mk_slider, mk_cone, mk_6dof];
        for ((name, _), f) in JOINTS.iter().zip(joints) {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        for (name, f) in [
            (c"BoxShape3D", s_box as RustFn),
            (c"SphereShape3D", s_sphere),
            (c"CapsuleShape3D", s_capsule),
            (c"CylinderShape3D", s_cylinder),
            (c"ConvexPolygonShape3D", s_convex),
            (c"ConcavePolygonShape3D", s_concave),
            (c"HeightMapShape3D", s_heightmap),
        ] {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        register_kind(L, ud, tag::SHAPE3, c"Shape3D", &[(c"__index", shape_index), (c"__newindex", shape_newindex)]);
        register(L, ud, "physics", &[(c"raycast", raycast), (c"setGravity", set_gravity), (c"getGravity", get_gravity)]);
    }
}

// ------------------------------------------------------------- what's where

pub fn is_body(kind: Kind3) -> bool {
    matches!(kind, Kind3::RigidBody | Kind3::StaticBody | Kind3::Area | Kind3::Vehicle)
}

pub fn is_physics(kind: Kind3) -> bool {
    is_body(kind) || matches!(kind, Kind3::Wheel | Kind3::Joint | Kind3::Character | Kind3::NavRegion | Kind3::NavAgent)
}

/// A `CharacterBody3D`'s own names.
fn character_has(a: A) -> bool {
    use A::*;
    matches!(
        a,
        Shape
            | CollisionLayer
            | CollisionMask
            | FloorMaxAngle
            | FloorSnapLength
            | MaxStepHeight
            | FloorStopOnSlope
            | FloorBlockOnWall
            | WallMinSlideAngle
            | SlideOnCeiling
            | MoveAndSlide
            | IsOnFloor
            | IsOnFloorOnly
            | IsOnWall
            | IsOnWallOnly
            | IsOnCeiling
            | GetFloorNormal
            | GetWallNormal
            | GetFloorAngle
            | GetRealVelocity
            | GetSlideCollisionCount
            | GetSlideCollision
    )
}

/// What a physics node has beyond every node's.
pub fn has(kind: Kind3, a: A) -> bool {
    use A::*;
    use Kind3 as K;
    let rigid = matches!(kind, K::RigidBody | K::Vehicle);
    match kind {
        K::Character => return character_has(a),
        K::NavRegion | K::NavAgent => return super::nav3d::has(kind, a),
        _ => {}
    }
    match a {
        Shape | CollisionLayer | CollisionMask => is_body(kind),
        Friction | Bounce => matches!(kind, K::RigidBody | K::Vehicle | K::StaticBody),
        Mass | GravityScale | LinearDamp | AngularDamp | ContactMonitor | Sleeping | Freeze | CanSleep | ContinuousCd | LockRotation | ApplyImpulse | ApplyCentralImpulse | ApplyForce
        | ApplyCentralForce | ApplyTorque | ApplyTorqueImpulse | GetCollidingBodies => rigid,
        BodyEntered | BodyExited => rigid || kind == K::Area,
        Monitoring | AreaEntered | AreaExited | GetOverlappingBodies | GetOverlappingAreas | OverlapsBody => kind == K::Area,
        EngineForce | Brake | Steering => matches!(kind, K::Vehicle | K::Wheel),
        Speed => kind == K::Vehicle,
        UseAsTraction | UseAsSteering | WheelRadius | WheelRestLength | SuspensionTravel | SuspensionStiffness | SuspensionMaxForce | DampingCompression | DampingRelaxation | WheelFrictionSlip
        | IsInContact => kind == K::Wheel,
        NodeA | NodeB | BreakForce | BreakTorque | Force | Torque | LimitLower | LimitUpper | MotorVelocity | MotorMaxImpulse | Broken => kind == K::Joint,
        _ => false,
    }
}

pub fn is_method(a: A) -> bool {
    use A::*;
    matches!(
        a,
        ApplyImpulse
            | ApplyCentralImpulse
            | ApplyForce
            | ApplyCentralForce
            | ApplyTorque
            | ApplyTorqueImpulse
            | GetCollidingBodies
            | GetOverlappingBodies
            | GetOverlappingAreas
            | OverlapsBody
            | IsInContact
            | MoveAndSlide
            | IsOnFloor
            | IsOnFloorOnly
            | IsOnWall
            | IsOnWallOnly
            | IsOnCeiling
            | GetFloorNormal
            | GetWallNormal
            | GetFloorAngle
            | GetRealVelocity
            | GetSlideCollisionCount
            | GetSlideCollision
    ) || super::nav3d::is_method(a)
}

pub fn is_signal(a: A) -> bool {
    matches!(a, A::BodyEntered | A::BodyExited | A::AreaEntered | A::AreaExited | A::Broken | A::BakeFinished | A::TargetReached | A::NavigationFinished)
}

fn read_only(a: A) -> bool {
    matches!(a, A::Speed | A::NodeA | A::NodeB | A::Force | A::Torque)
}

// ------------------------------------------------------------- shapes

unsafe fn shape_at(L: *mut lua_State, i: c_int) -> Option<Shape3> {
    unsafe { value_at::<Shape3>(L, i, tag::SHAPE3) }
}

/// A shape's table: its own numbers, and where it sits in the body (x, y,
/// z, rx, ry, rz).
unsafe fn shape_ctor(L: *mut lua_State, h: &mut Host, name: &str, keys: &[&str], make: impl FnOnce(&dyn Fn(&CStr, f32) -> f32) -> Result<Geom3, String>) -> c_int {
    unsafe {
        let t = lua_type(L, 1);
        if t != LUA_TTABLE && t != LUA_TNIL && t != -1 {
            return err(L, &format!("{name} takes one table: {name}{{ … }} (got {})", type_name(L, 1)));
        }
        if t == LUA_TTABLE {
            lua_pushnil(L);
            while lua_next(L, 1) != 0 {
                let k = st(L, -2).map(str::to_string).unwrap_or_default();
                lua_pop(L, 1);
                if !keys.contains(&k.as_str()) && !["x", "y", "z", "rx", "ry", "rz"].contains(&k.as_str()) {
                    lua_pop(L, 1);
                    return err(L, &format!("{name}{{…}}: no key `{k}` ({}, and x, y, z, rx, ry, rz for where it sits in its body)", keys.join(", ")));
                }
            }
        }
        let n = |k: &CStr, d: f32| super::three::fnum(L, 1, k, d);
        let geom = match make(&n) {
            Ok(g) => g,
            Err(m) => return err(L, &format!("{name}{{…}}: {m}")),
        };
        let pos = vec3(n(c"x", 0.0), n(c"y", 0.0), n(c"z", 0.0));
        let rot = Quat::from_euler(n(c"ry", 0.0), n(c"rx", 0.0), n(c"rz", 0.0));
        let _ = h;
        push_value(L, tag::SHAPE3, Shape3 { geom, pos, rot })
    }
}

fn positive(v: f32, what: &str) -> Result<f32, String> {
    if v > 0.0 && v.is_finite() { Ok(v) } else { Err(format!("{what} is more than 0 meters, got {}", fmt_num(v as f64))) }
}

b!(s_box, |L, h| shape_ctor(L, h, "BoxShape3D", &["w", "h", "d"], |n| Ok(Geom3::Box { w: positive(n(c"w", 1.0), "w")?, h: positive(n(c"h", 1.0), "h")?, d: positive(n(c"d", 1.0), "d")? })));
b!(s_sphere, |L, h| shape_ctor(L, h, "SphereShape3D", &["radius"], |n| Ok(Geom3::Sphere { r: positive(n(c"radius", 0.5), "radius")? })));
b!(s_capsule, |L, h| shape_ctor(L, h, "CapsuleShape3D", &["radius", "height"], |n| {
    let (r, ht) = (positive(n(c"radius", 0.5), "radius")?, positive(n(c"height", 2.0), "height")?);
    if ht < 2.0 * r {
        return Err(format!("height is the whole capsule, caps and all (Godot 4's): at least 2 × radius = {}", fmt_num((2.0 * r) as f64)));
    }
    Ok(Geom3::Capsule { r, h: ht })
}));
b!(s_cylinder, |L, h| shape_ctor(L, h, "CylinderShape3D", &["radius", "height"], |n| Ok(Geom3::Cylinder { r: positive(n(c"radius", 0.5), "radius")?, h: positive(n(c"height", 2.0), "height")? })));

unsafe fn geometry_of(L: *mut lua_State, h: &Host) -> Result<u32, String> {
    unsafe {
        let g = with_field(L, 1, c"geometry", || handle(L, -1, tag::GEOMETRY));
        let Some(g) = g else { return Err("needs geometry = geometry.rock(1.5, 7) (any geometry.*)".into()) };
        let n = h.three.as_deref().map_or(0, |w| w.meshes.len());
        if g == 0 || g as usize > n {
            return Err("that geometry isn't this round's".into());
        }
        Ok(g - 1)
    }
}

b!(s_convex, |L, h| {
    let g = geometry_of(L, h);
    shape_ctor(L, h, "ConvexPolygonShape3D", &["geometry"], |_| Ok(Geom3::Convex(g?)))
});
b!(s_concave, |L, h| {
    let g = geometry_of(L, h);
    shape_ctor(L, h, "ConcavePolygonShape3D", &["geometry"], |_| Ok(Geom3::Concave(g?)))
});
b!(s_heightmap, |L, h| err(L, "HeightMapShape3D: a Terrain is the ground's shape already (Terrain{ collision = true }, the default): bodies land on it"));

b!(shape_index, |L, h| {
    let Some(s) = shape_at(L, 1) else { return err(L, "not a shape") };
    let k = st(L, 2).unwrap_or("");
    let v = match (k, s.geom) {
        ("type", _) => return rets(L, s.type_name()),
        ("w", Geom3::Box { w, .. }) => w,
        ("h", Geom3::Box { h, .. }) => h,
        ("d", Geom3::Box { d, .. }) => d,
        ("radius", Geom3::Sphere { r } | Geom3::Capsule { r, .. } | Geom3::Cylinder { r, .. }) => r,
        ("height", Geom3::Capsule { h, .. } | Geom3::Cylinder { h, .. }) => h,
        ("x", _) => s.pos.x,
        ("y", _) => s.pos.y,
        ("z", _) => s.pos.z,
        _ => return err(L, &format!("{} has no `{k}`", s.type_name())),
    };
    ret(L, super::nodes3d::nice(v))
});

b!(shape_newindex, |L, h| err(L, "a shape is made once: make a new one and set the body's shape"));

// ------------------------------------------------------------- bodies

b!(mk_rigid, |L, h| make(L, h, Kind3::RigidBody, None));
b!(mk_static, |L, h| make(L, h, Kind3::StaticBody, None));
b!(mk_area, |L, h| make(L, h, Kind3::Area, None));
b!(mk_vehicle, |L, h| make(L, h, Kind3::Vehicle, None));
b!(mk_wheel, |L, h| make(L, h, Kind3::Wheel, None));
b!(mk_character, |L, h| make(L, h, Kind3::Character, None));
b!(mk_region, |L, h| make(L, h, Kind3::NavRegion, None));
b!(mk_agent, |L, h| make(L, h, Kind3::NavAgent, None));
b!(mk_pin, |L, h| make(L, h, Kind3::Joint, Some(JointType::Pin)));
b!(mk_hinge, |L, h| make(L, h, Kind3::Joint, Some(JointType::Hinge)));
b!(mk_slider, |L, h| make(L, h, Kind3::Joint, Some(JointType::Slider)));
b!(mk_cone, |L, h| make(L, h, Kind3::Joint, Some(JointType::ConeTwist)));
b!(mk_6dof, |L, h| make(L, h, Kind3::Joint, Some(JointType::Generic6DOF)));

/// The shapes at `i`: one shape, or a list of them.
unsafe fn shapes_at(L: *mut lua_State, i: c_int) -> Result<Vec<Shape3>, String> {
    unsafe {
        if let Some(s) = shape_at(L, i) {
            return Ok(vec![s]);
        }
        if lua_type(L, i) == LUA_TTABLE && lua_objlen(L, i) > 0 {
            let mut out = Vec::new();
            for k in 1..=lua_objlen(L, i) {
                lua_rawgeti(L, i, k);
                let s = shape_at(L, -1);
                lua_pop(L, 1);
                out.push(s.ok_or_else(|| format!("shape[{k}] isn't a shape (BoxShape3D{{…}}, SphereShape3D{{…}}, …)"))?);
            }
            return Ok(out);
        }
        Err(format!("shape is a BoxShape3D{{…}}, SphereShape3D{{…}}, CapsuleShape3D{{…}}, CylinderShape3D{{…}}, ConvexPolygonShape3D{{…}} or a list of them, got {}", type_name(L, i)))
    }
}

/// The SDK's shape for one of ours, with the body's material and layers.
fn sdk_shape(w: &World3, s: &Shape3, b: &Body3, sensor: bool, kind: Kind3) -> Result<p3::Shape, String> {
    let shape = geom_shape(w, s, kind)?;
    let shape = shape.at(s.pos).rotated(s.rot).friction(b.friction).restitution(b.bounce).layers(b.layer, b.mask);
    Ok(if sensor { shape.sensor() } else { shape })
}

/// A shape's geometry as the SDK's.
fn geom_shape(w: &World3, s: &Shape3, kind: Kind3) -> Result<p3::Shape, String> {
    Ok(match s.geom {
        Geom3::Box { w, h, d } => p3::Shape::cuboid(vec3(w, h, d)),
        Geom3::Sphere { r } => p3::Shape::sphere(r),
        Geom3::Capsule { r, h } => p3::Shape::capsule(r, h),
        Geom3::Cylinder { r, h } => p3::Shape::cylinder(r, h),
        Geom3::Convex(g) => p3::Shape::convex_hull_of(&w.meshes.get(g as usize).ok_or("that geometry is gone")?.data),
        Geom3::Concave(g) => {
            if matches!(kind, Kind3::RigidBody | Kind3::Vehicle | Kind3::Character) {
                return Err("ConcavePolygonShape3D is hollow, so it's for StaticBody3D and Area3D only: a moving body uses ConvexPolygonShape3D".into());
            }
            p3::Shape::trimesh(&w.meshes.get(g as usize).ok_or("that geometry is gone")?.data)
        }
    })
}

/// A character's SDK shape (its own, centred: its origin is its shape's centre).
fn character_shape(w: &World3, s: &Shape3, layer: u32) -> Result<p3::Shape, String> {
    Ok(geom_shape(w, s, Kind3::Character)?.at(s.pos).rotated(s.rot).layers(layer, u32::MAX))
}

/// Makes (or makes again, with a new shape or layer) a character's controller.
fn set_character(w: &mut World3, id: Node3Id, shape: Shape3, layer: u32, settings: Option<p3::CharacterSettings>) -> Result<(), String> {
    let made = character_shape(w, &shape, layer)?;
    let pos = w.scene.world(id).map(|m| trs_of(&m).0).unwrap_or(Vec3::ZERO);
    let p = w.phys();
    let (settings, vel) = match p.chars.remove(&id.to_bits()) {
        Some(c) => {
            p.by_body.remove(&c.ch.body().to_bits());
            let (s, v) = (c.ch.settings, c.vel);
            c.ch.remove(&mut p.world);
            (settings.unwrap_or(s), v)
        }
        None => (settings.unwrap_or_default(), Vec3::ZERO),
    };
    let ch = p3::Character::new(&mut p.world, pos, made, settings, id.to_bits() as u64);
    p.by_body.insert(ch.body().to_bits(), id);
    p.chars.insert(id.to_bits(), Char3 { ch, vel, shape, layer, last: Some(pos), agent: false, grounded: false });
    Ok(())
}

/// `node:moveAndSlide()`: Godot's, over the frame's `dt`.
fn move_and_slide(w: &mut World3, id: Node3Id, dt: f32) -> bool {
    let now = w.scene.world(id).map(|m| trs_of(&m));
    let ground = w.ground.as_ref().filter(|g| g.collision).map(|g| &g.terrain);
    let Some(p) = w.phys.as_deref_mut() else { return false };
    let k = id.to_bits();
    // Lua moved it: it starts from there.
    if let (Some(c), Some((pos, _))) = (p.chars.get_mut(&k), now)
        && c.last.is_none_or(|l| l.distance(pos) > 1e-4)
    {
        c.ch.set_position(&mut p.world, pos);
    }
    if let Some(t) = ground {
        crate::physics3::ground_tiles(p, t);
    }
    let Some(c) = p.chars.get_mut(&k) else { return false };
    let v = c.vel;
    c.vel = c.ch.move_and_slide(&mut p.world, v, dt);
    let pos = c.ch.position();
    c.last = Some(pos);
    let hit = !c.ch.hits().is_empty();
    w.place_pos(id, pos);
    hit
}

/// Sets a body's shapes (replacing any it had), and its mass over them.
fn set_shapes(w: &mut World3, id: Node3Id, shapes: Vec<Shape3>) -> Result<(), String> {
    let kind = w.kind(id);
    let p = w.phys.as_deref().ok_or("no physics")?;
    let b = p.bodies.get(&id.to_bits()).ok_or("not a body")?;
    let made: Vec<p3::Shape> = shapes.iter().map(|s| sdk_shape(w, s, b, kind == Kind3::Area, kind)).collect::<Result<_, _>>()?;
    let p = w.phys.as_deref_mut().unwrap();
    let b = p.bodies.get_mut(&id.to_bits()).unwrap();
    for (sid, _) in b.shapes.drain(..) {
        p.world.remove_shape(sid);
    }
    // A static body or an area has no mass: things that read it (a
    // vehicle's wheels braking on it) see it as immovable.
    let each = if matches!(kind, Kind3::RigidBody | Kind3::Vehicle) { b.mass / made.len().max(1) as f32 } else { 0.0 };
    for (s, desc) in made.into_iter().zip(shapes) {
        let s = if each > 0.0 { s.mass(each) } else { s.density(0.0) };
        let sid = p.world.add_shape(b.id, s);
        b.shapes.push((sid, desc));
    }
    Ok(())
}

/// Puts a body where its node is now (made, or moved before a joint reads it).
pub fn place_now(w: &mut World3, id: Node3Id) {
    let Some(m) = w.scene.world(id) else { return };
    let now = trs_of(&m);
    let Some(p) = w.phys.as_deref_mut() else { return };
    let Some(b) = p.bodies.get_mut(&id.to_bits()) else { return };
    if b.last.is_some_and(|l| l.0.distance(now.0) < 1e-5 && l.1.dot(now.1).abs() > 1.0 - 1e-7) {
        return;
    }
    p.world.set_position(b.id, now.0);
    p.world.set_rotation(b.id, now.1);
    b.last = Some(now);
}

const FIRST: &[&str] = &["parent", "shape", "nodeA", "nodeB", "bake"];

unsafe fn make(L: *mut lua_State, h: &mut Host, kind: Kind3, joint: Option<JointType>) -> c_int {
    unsafe {
        let ctor = joint.map_or(kind.name(), |j| j.name());
        let t = lua_type(L, 1);
        if t != LUA_TTABLE && t != LUA_TNIL && t != -1 {
            return err(L, &format!("{ctor} takes one table: {ctor}{{ x = 1, y = 2 }} (got {})", type_name(L, 1)));
        }
        if t != LUA_TTABLE {
            lua_settop(L, 0);
            lua_newtable(L);
        }
        let Some(w) = h.three.as_deref_mut() else {
            return err(L, &format!("{ctor}{{…}} is 3D's (kind = \"3d\" in studio.toml)"));
        };
        let field = |k: &CStr| -> c_int {
            lua_rawgetfield(L, 1, k.as_ptr());
            lua_gettop(L)
        };
        // The parent.
        let at = field(c"parent");
        let parent = match lua_type(L, at) {
            LUA_TNIL => None,
            _ => match node_at(L, at).filter(|p| w.scene.alive(*p)) {
                Some(p) => Some(p),
                None => {
                    lua_pop(L, 1);
                    return err(L, &format!("{ctor}{{…}}: parent is a 3D node (got {})", type_name(L, at)));
                }
            },
        };
        lua_pop(L, 1);
        if kind == Kind3::Wheel && !parent.is_some_and(|p| w.kind(p) == Kind3::Vehicle) {
            return err(L, "VehicleWheel3D{…} goes under its VehicleBody3D: VehicleWheel3D{ parent = car, x = 0.8, y = 0, z = -1.2 }");
        }
        if kind == Kind3::RigidBody && parent.is_some_and(|p| is_body(w.kind(p))) {
            return err(L, "RigidBody3D{…}: a body can't go under another body (join them with a joint, or give one body several shapes)");
        }
        // The shapes.
        let shapes = if is_body(kind) || kind == Kind3::Character {
            let at = field(c"shape");
            let s = shapes_at(L, at);
            lua_pop(L, 1);
            match s {
                Ok(s) => Some(s),
                Err(m) => {
                    let example = if kind == Kind3::Character { "CapsuleShape3D{ radius = 0.35, height = 1.5 }" } else { "BoxShape3D{ w = 1, h = 1, d = 1 }" };
                    let m = if m.contains("got nil") { format!("needs a shape: {ctor}{{ shape = {example} }}") } else { m };
                    return err(L, &format!("{ctor}{{…}}: {m}"));
                }
            }
        } else {
            None
        };
        // A joint's bodies.
        let mut ends = (None, None);
        if kind == Kind3::Joint {
            for (k, slot) in [(c"nodeA", &mut ends.0), (c"nodeB", &mut ends.1)] {
                let at = field(k);
                let n = node_at(L, at).filter(|n| w.scene.alive(*n) && matches!(w.kind(*n), Kind3::RigidBody | Kind3::StaticBody | Kind3::Vehicle));
                let given = lua_type(L, at) != LUA_TNIL;
                lua_pop(L, 1);
                if given && n.is_none() {
                    return err(L, &format!("{ctor}{{…}}: {} is a RigidBody3D, StaticBody3D or VehicleBody3D", k.to_str().unwrap_or("?")));
                }
                *slot = n;
            }
            if ends.0.is_none() || ends.1.is_none() {
                return err(L, &format!("{ctor}{{…}} needs nodeA and nodeB: the two bodies it joins"));
            }
            if ends.0 == ends.1 {
                return err(L, &format!("{ctor}{{…}}: nodeA and nodeB are two different bodies"));
            }
        }
        let parent_host = parent.and_then(|p| w.scene.get(p)).and_then(|p| p.host);
        let Some(n) = gfx3d::Node::new() else {
            return err(L, "too many nodes (4096 at most)");
        };
        if parent_host.is_some() {
            n.set_parent(parent_host);
        }
        let id = match w.scene.add(Some(n), true, parent) {
            Ok(id) => id,
            Err(_) => {
                n.destroy();
                return err(L, "too many nodes (4096 at most)");
            }
        };
        let mut info = Info::new(kind);
        if let Some(j) = joint {
            info.src = j.name().into();
        }
        w.info.insert(id.to_bits(), info);
        // The body, at the origin until its node's place is known.
        if is_body(kind) {
            let p = w.phys();
            let def = match body_kind(kind, false, false) {
                BodyKind::Dynamic => p3::Body::dynamic(Vec3::ZERO),
                BodyKind::Fixed => p3::Body::fixed(Vec3::ZERO),
                BodyKind::Kinematic => p3::Body::kinematic(Vec3::ZERO),
            };
            let bid = p.world.add_body(def.tag(id.to_bits() as u64));
            p.by_body.insert(bid.to_bits(), id);
            let vehicle = (kind == Kind3::Vehicle).then(|| p.world.add_vehicle(bid, &[]));
            p.bodies.insert(
                id.to_bits(),
                Body3 {
                    id: bid,
                    shapes: Vec::new(),
                    last: None,
                    mass: if kind == Kind3::Vehicle { 40.0 } else { 1.0 },
                    friction: 1.0,
                    bounce: 0.0,
                    layer: 1,
                    mask: 1,
                    gravity_scale: 1.0,
                    linear_damp: 0.0,
                    angular_damp: 0.0,
                    can_sleep: true,
                    ccd: false,
                    lock_rotation: false,
                    contact_monitor: false,
                    monitoring: true,
                    freeze: false,
                    moving: false,
                    vehicle,
                    wheels: Vec::new(),
                    engine: 0.0,
                    brake: 0.0,
                    steering: 0.0,
                },
            );
            if let Err(m) = set_shapes(w, id, shapes.clone().unwrap_or_default()) {
                destroy(w, id);
                return err(L, &format!("{ctor}{{…}}: {m}"));
            }
        }
        if kind == Kind3::Character {
            let mut shapes = shapes.unwrap_or_default();
            if shapes.len() != 1 {
                destroy(w, id);
                return err(L, "CharacterBody3D{…} has one shape: CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.5 } }");
            }
            if let Err(m) = set_character(w, id, shapes.remove(0), 1, None) {
                destroy(w, id);
                return err(L, &format!("CharacterBody3D{{…}}: {m}"));
            }
        }
        if matches!(kind, Kind3::NavRegion | Kind3::NavAgent) {
            let at = field(c"bake");
            let r = super::nav3d::made(L, w, id, kind, at);
            lua_pop(L, 1);
            if let Err(m) = r {
                destroy(w, id);
                return err(L, &format!("{ctor}{{…}}: {m}"));
            }
        }
        if kind == Kind3::Wheel {
            let v = parent.unwrap();
            let p = w.phys();
            p.wheels.insert(
                id.to_bits(),
                Wheel3 { vehicle: v, index: usize::MAX, desc: p3::Wheel::default(), traction: false, steer: false, engine: 0.0, brake: 0.0, steering: 0.0, state: Default::default() },
            );
        }
        if let (Some(j), (Some(a), Some(b))) = (joint, ends) {
            w.phys().joints.insert(id.to_bits(), Joint3 { id: None, kind: j, a, b, break_force: 0.0, break_torque: 0.0, force: 0.0, torque: 0.0, limits: None, motor: None });
        }
        // The rest of the table, in its order.
        lua_pushnil(L);
        while lua_next(L, 1) != 0 {
            let (a, key) = key_at(L, -2);
            let r = match a {
                _ if FIRST.contains(&key.as_str()) => Ok(()),
                _ if lua_type(L, -2) != LUA_TSTRING => Err(format!("{ctor}{{…}} takes names = values, got a {} key", type_name(L, -2))),
                Some(a) if is_signal(a) || is_method(a) || super::nodes3d::is_method(a) && !super::three::is_prop(a) => {
                    Err(format!("{key} is a method or signal: use node:{key}(…) or node.{key}:connect(fn) after making it"))
                }
                _ => set_any(L, h, id, a, &key, -1),
            };
            if let Err(m) = r {
                lua_pop(L, 2);
                if let Some(w) = h.three.as_deref_mut() {
                    destroy(w, id);
                }
                return err(L, &format!("{ctor}{{…}}: {m}"));
            }
            lua_pop(L, 1);
        }
        let w = h.three.as_deref_mut().unwrap();
        if is_body(kind) {
            place_now(w, id);
        }
        if kind == Kind3::Character {
            let pos = w.scene.world(id).map(|m| trs_of(&m).0).unwrap_or(Vec3::ZERO);
            let p = w.phys();
            if let Some(c) = p.chars.get_mut(&id.to_bits()) {
                c.ch.set_position(&mut p.world, pos);
                c.last = Some(pos);
            }
        }
        if kind == Kind3::Wheel {
            add_wheel(w, id);
        }
        if kind == Kind3::Joint
            && let Err(m) = join(w, id)
        {
            destroy(w, id);
            return err(L, &format!("{ctor}{{…}}: {m}"));
        }
        push_node(L, w, id);
        1
    }
}

fn destroy(w: &mut World3, id: Node3Id) {
    let gone = w.scene.destroy(id);
    w.forget(&gone);
}

/// A wheel joins its vehicle where it was placed (its suspension's top).
fn add_wheel(w: &mut World3, id: Node3Id) {
    let Some(n) = w.scene.get(id) else { return };
    let at = vec3(n.x, n.y, n.z);
    let p = w.phys();
    let Some(wh) = p.wheels.get_mut(&id.to_bits()) else { return };
    let Some(body) = p.bodies.get_mut(&wh.vehicle.to_bits()) else { return };
    let Some(v) = body.vehicle else { return };
    wh.desc.at = at;
    wh.index = p.world.add_wheel(v, &wh.desc);
    body.wheels.push(id);
}

/// Makes a joint node's joint, where the node is, between bodies placed now.
fn join(w: &mut World3, id: Node3Id) -> Result<(), String> {
    let Some(j) = w.phys.as_deref().and_then(|p| p.joints.get(&id.to_bits())) else { return Ok(()) };
    let (a, b, kind) = (j.a, j.b, j.kind);
    place_now(w, a);
    place_now(w, b);
    let m = w.scene.world(id).ok_or("the joint has no place")?;
    let (pos, rot) = trs_of(&m);
    let p = w.phys.as_deref_mut().unwrap();
    let (Some(ba), Some(bb)) = (p.bodies.get(&a.to_bits()).map(|b| b.id), p.bodies.get(&b.to_bits()).map(|b| b.id)) else {
        return Err("one of its bodies is gone".into());
    };
    let j = p.joints.get_mut(&id.to_bits()).unwrap();
    let mut def = match kind {
        JointType::Pin | JointType::ConeTwist => p3::Joint::spherical(ba, bb, pos),
        // Godot's hinge turns about the joint's z axis; its slider slides along x.
        JointType::Hinge => p3::Joint::revolute(ba, bb, pos, rot * vec3(0.0, 0.0, 1.0)),
        JointType::Slider => p3::Joint::prismatic(ba, bb, pos, rot * vec3(1.0, 0.0, 0.0)),
        JointType::Generic6DOF => p3::Joint::fixed(ba, bb),
    };
    if let Some((lo, hi)) = j.limits
        && matches!(kind, JointType::Hinge | JointType::Slider)
    {
        def = def.limits(lo, hi);
    }
    if let Some((speed, max)) = j.motor
        && matches!(kind, JointType::Hinge | JointType::Slider)
    {
        def = def.motor(speed, max);
    }
    if let Some(old) = j.id.take()
        && p.world.has_joint(old)
    {
        p.world.unjoin(old);
    }
    let jid = p.world.join(def);
    p.joints.get_mut(&id.to_bits()).unwrap().id = Some(jid);
    Ok(())
}

// ------------------------------------------------------------- properties

macro_rules! body {
    ($w:expr, $id:expr) => {
        match $w.phys.as_deref_mut().and_then(|p| p.bodies.get_mut(&$id.to_bits())) {
            Some(b) => b,
            None => return err_s("this body has no physics (destroyed?)"),
        }
    };
}

fn err_s<T>(m: &str) -> Result<T, String> {
    Err(m.to_string())
}

/// A body node's or wheel's or joint's property.
pub unsafe fn get(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A) -> Result<c_int, String> {
    unsafe {
        let w = h.three.as_deref_mut().ok_or(super::three::NOT_3D)?;
        let kind = w.kind(id);
        let k = id.to_bits();
        if matches!(kind, Kind3::NavRegion | Kind3::NavAgent) {
            return super::nav3d::get(L, w, id, a);
        }
        let p = w.phys.as_deref_mut().ok_or("no physics")?;
        let nice = super::nodes3d::nice;
        if kind == Kind3::Character {
            let c = p.chars.get(&k).ok_or("this character is gone")?;
            let s = &c.ch.settings;
            return Ok(match a {
                A::Shape => push_value(L, tag::SHAPE3, c.shape),
                A::CollisionLayer => ret(L, c.layer as f64),
                A::CollisionMask => ret(L, s.mask as f64),
                A::FloorMaxAngle => ret(L, nice(s.floor_max_angle)),
                A::FloorSnapLength => ret(L, nice(s.floor_snap_length)),
                A::MaxStepHeight => ret(L, nice(s.max_step_height)),
                A::FloorStopOnSlope => retb(L, s.floor_stop_on_slope),
                A::FloorBlockOnWall => retb(L, s.floor_block_on_wall),
                A::WallMinSlideAngle => ret(L, nice(s.wall_min_slide_angle)),
                A::SlideOnCeiling => retb(L, s.slide_on_ceiling),
                _ => return Err(format!("CharacterBody3D has no property `{}`", a.name())),
            });
        }
        if kind == Kind3::Wheel {
            let wh = p.wheels.get(&k).ok_or("this wheel is gone")?;
            let d = &wh.desc;
            return Ok(match a {
                A::UseAsTraction => retb(L, wh.traction),
                A::UseAsSteering => retb(L, wh.steer),
                A::WheelRadius => ret(L, nice(d.radius)),
                A::WheelRestLength => ret(L, nice(d.rest)),
                A::SuspensionTravel => ret(L, nice(d.travel)),
                A::SuspensionStiffness => ret(L, nice(d.stiffness)),
                A::SuspensionMaxForce => ret(L, nice(d.max_force)),
                A::DampingCompression => ret(L, nice(d.compression)),
                A::DampingRelaxation => ret(L, nice(d.relaxation)),
                A::WheelFrictionSlip => ret(L, nice(d.friction_slip)),
                A::EngineForce => ret(L, nice(wh.engine)),
                A::Brake => ret(L, nice(wh.brake)),
                A::Steering => ret(L, nice(wh.steering)),
                _ => return Err(format!("VehicleWheel3D has no property `{}`", a.name())),
            });
        }
        if kind == Kind3::Joint {
            let j = p.joints.get(&k).ok_or("this joint is gone")?;
            return Ok(match a {
                A::NodeA | A::NodeB => {
                    let n = if a == A::NodeA { j.a } else { j.b };
                    push_node(L, w, n);
                    1
                }
                A::BreakForce => ret(L, nice(j.break_force)),
                A::BreakTorque => ret(L, nice(j.break_torque)),
                A::Force => ret(L, nice(j.force)),
                A::Torque => ret(L, nice(j.torque)),
                A::LimitLower => ret(L, nice(j.limits.map_or(0.0, |l| l.0))),
                A::LimitUpper => ret(L, nice(j.limits.map_or(0.0, |l| l.1))),
                A::MotorVelocity => ret(L, nice(j.motor.map_or(0.0, |m| m.0))),
                A::MotorMaxImpulse => ret(L, nice(j.motor.map_or(0.0, |m| m.1 / 60.0))),
                _ => return Err(format!("{} has no property `{}`", j.kind.name(), a.name())),
            });
        }
        let b = p.bodies.get(&k).ok_or("this body has no physics (destroyed?)")?;
        Ok(match a {
            A::Shape => match b.shapes.first() {
                Some((_, s)) => push_value(L, tag::SHAPE3, *s),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Mass => ret(L, nice(b.mass)),
            A::Friction => ret(L, nice(b.friction)),
            A::Bounce => ret(L, nice(b.bounce)),
            A::CollisionLayer => ret(L, b.layer as f64),
            A::CollisionMask => ret(L, b.mask as f64),
            A::GravityScale => ret(L, nice(b.gravity_scale)),
            A::LinearDamp => ret(L, nice(b.linear_damp)),
            A::AngularDamp => ret(L, nice(b.angular_damp)),
            A::ContactMonitor => retb(L, b.contact_monitor),
            A::Monitoring => retb(L, b.monitoring),
            A::Sleeping => retb(L, p.world.is_sleeping(b.id)),
            A::Freeze => retb(L, b.freeze),
            A::CanSleep => retb(L, b.can_sleep),
            A::ContinuousCd => retb(L, b.ccd),
            A::LockRotation => retb(L, b.lock_rotation),
            A::EngineForce => ret(L, nice(b.engine)),
            A::Brake => ret(L, nice(b.brake)),
            A::Steering => ret(L, nice(b.steering)),
            A::Speed => ret(L, nice(b.vehicle.map_or(0.0, |v| p.world.vehicle_speed(v)))),
            _ => return Err(format!("{} has no property `{}`", kind.name(), a.name())),
        })
    }
}

unsafe fn number(L: *mut lua_State, v: c_int, key: &str) -> Result<f32, String> {
    unsafe {
        match num(L, v) {
            Some(x) if x.is_finite() => Ok(x as f32),
            _ => Err(format!("{key} takes a number, got {}", type_name(L, v))),
        }
    }
}

unsafe fn flag(L: *mut lua_State, v: c_int, key: &str) -> Result<bool, String> {
    unsafe {
        match lua_type(L, v) {
            LUA_TBOOLEAN => Ok(lua_toboolean(L, v) != 0),
            _ => Err(format!("{key} takes true or false, got {}", type_name(L, v))),
        }
    }
}

/// Godot's layers: a whole number whose bits are layers 1–32.
fn layer_bits(x: f32, key: &str) -> Result<u32, String> {
    if x >= 0.0 && x <= u32::MAX as f32 && x.fract() == 0.0 { Ok(x as u32) } else { Err(format!("{key} is a whole number whose bits are layers 1–32 (layer 1 is 1, layer 2 is 2, both are 3)")) }
}

pub unsafe fn set(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A, v: c_int) -> Result<(), String> {
    unsafe {
        let key = a.name();
        if read_only(a) {
            return Err(format!("{key} is read-only"));
        }
        let w = h.three.as_deref_mut().ok_or(super::three::NOT_3D)?;
        let kind = w.kind(id);
        let k = id.to_bits();
        if matches!(kind, Kind3::NavRegion | Kind3::NavAgent) {
            return super::nav3d::set(L, w, id, a, v);
        }
        if kind == Kind3::Character {
            let c = w.phys().chars.get(&k).ok_or("this character is gone")?;
            let (shape, layer) = (c.shape, c.layer);
            let mut s = c.ch.settings;
            match a {
                A::Shape => {
                    let mut list = shapes_at(L, v)?;
                    if list.len() != 1 {
                        return Err("a CharacterBody3D has one shape".into());
                    }
                    return set_character(w, id, list.remove(0), layer, None);
                }
                A::CollisionLayer => return set_character(w, id, shape, layer_bits(number(L, v, key)?, key)?, None),
                A::CollisionMask => s.mask = layer_bits(number(L, v, key)?, key)?,
                A::FloorMaxAngle => s.floor_max_angle = number(L, v, key)?.clamp(0.0, std::f32::consts::FRAC_PI_2),
                A::FloorSnapLength => s.floor_snap_length = number(L, v, key)?.max(0.0),
                A::MaxStepHeight => s.max_step_height = number(L, v, key)?.max(0.0),
                A::FloorStopOnSlope => s.floor_stop_on_slope = flag(L, v, key)?,
                A::FloorBlockOnWall => s.floor_block_on_wall = flag(L, v, key)?,
                A::WallMinSlideAngle => s.wall_min_slide_angle = number(L, v, key)?.clamp(0.0, std::f32::consts::PI),
                A::SlideOnCeiling => s.slide_on_ceiling = flag(L, v, key)?,
                _ => return Err(format!("CharacterBody3D has no property `{key}`")),
            }
            w.phys().chars.get_mut(&k).unwrap().ch.settings = s;
            return Ok(());
        }
        if a == A::Shape {
            let shapes = shapes_at(L, v)?;
            return set_shapes(w, id, shapes);
        }
        if kind == Kind3::Wheel {
            let p = w.phys();
            let wh = p.wheels.get_mut(&k).ok_or("this wheel is gone")?;
            let d = &mut wh.desc;
            match a {
                A::UseAsTraction => wh.traction = flag(L, v, key)?,
                A::UseAsSteering => wh.steer = flag(L, v, key)?,
                A::EngineForce => wh.engine = number(L, v, key)?,
                A::Brake => wh.brake = number(L, v, key)?.max(0.0),
                A::Steering => wh.steering = number(L, v, key)?,
                A::WheelRadius => d.radius = number(L, v, key)?.max(0.01),
                A::WheelRestLength => d.rest = number(L, v, key)?.max(0.0),
                A::SuspensionTravel => d.travel = number(L, v, key)?.max(0.0),
                A::SuspensionStiffness => d.stiffness = number(L, v, key)?.max(0.0),
                A::SuspensionMaxForce => d.max_force = number(L, v, key)?.max(0.0),
                A::DampingCompression => d.compression = number(L, v, key)?.max(0.0),
                A::DampingRelaxation => d.relaxation = number(L, v, key)?.max(0.0),
                A::WheelFrictionSlip => d.friction_slip = number(L, v, key)?.max(0.0),
                _ => return Err(format!("VehicleWheel3D has no property `{key}`")),
            }
            // Tuning reaches a wheel that's on its vehicle already.
            let (desc, index, vehicle) = (wh.desc, wh.index, wh.vehicle);
            if index != usize::MAX
                && let Some(veh) = p.bodies.get(&vehicle.to_bits()).and_then(|b| b.vehicle)
            {
                p.world.tune_wheel(veh, index, &desc);
            }
            return Ok(());
        }
        if kind == Kind3::Joint {
            let p = w.phys();
            let j = p.joints.get_mut(&k).ok_or("this joint is gone")?;
            let mut remake = false;
            match a {
                A::BreakForce => j.break_force = number(L, v, key)?.max(0.0),
                A::BreakTorque => j.break_torque = number(L, v, key)?.max(0.0),
                A::LimitLower | A::LimitUpper => {
                    let x = number(L, v, key)?;
                    let mut l = j.limits.unwrap_or((0.0, 0.0));
                    if a == A::LimitLower {
                        l.0 = x;
                    } else {
                        l.1 = x;
                    }
                    j.limits = Some(l);
                    remake = j.id.is_some();
                }
                A::MotorVelocity | A::MotorMaxImpulse => {
                    let x = number(L, v, key)?;
                    let mut m = j.motor.unwrap_or((0.0, f32::INFINITY));
                    if a == A::MotorVelocity {
                        m.0 = x;
                    } else {
                        m.1 = x.max(0.0) * 60.0;
                    }
                    j.motor = Some(m);
                    if let Some(jid) = j.id {
                        p.world.set_motor(jid, m.0, m.1);
                    }
                }
                _ => return Err(format!("{} has no property `{key}`", j.kind.name())),
            }
            if remake {
                join(w, id)?;
            }
            return Ok(());
        }
        let x = || number(L, v, key);
        let b = body!(w, id);
        let bid = b.id;
        let mut reshape = false;
        match a {
            A::Mass => {
                b.mass = positive(x()?, "mass")?;
                reshape = true;
            }
            A::Friction => {
                b.friction = x()?.max(0.0);
                reshape = true;
            }
            A::Bounce => {
                b.bounce = x()?.clamp(0.0, 1.0);
                reshape = true;
            }
            A::CollisionLayer | A::CollisionMask => {
                let bits = layer_bits(x()?, key)?;
                if a == A::CollisionLayer {
                    b.layer = bits;
                } else {
                    b.mask = bits;
                }
                reshape = true;
            }
            A::GravityScale => {
                b.gravity_scale = x()?;
                let g = b.gravity_scale;
                w.phys().world.set_gravity_scale(bid, g);
            }
            A::LinearDamp | A::AngularDamp => {
                let d = x()?.max(0.0);
                if a == A::LinearDamp {
                    b.linear_damp = d;
                } else {
                    b.angular_damp = d;
                }
                let (l, an) = (b.linear_damp, b.angular_damp);
                w.phys().world.set_damping(bid, l, an);
            }
            A::ContactMonitor => b.contact_monitor = flag(L, v, key)?,
            A::Monitoring => b.monitoring = flag(L, v, key)?,
            A::Sleeping => {
                if flag(L, v, key)? {
                    w.phys().world.sleep(bid);
                } else {
                    w.phys().world.wake(bid);
                }
            }
            A::Freeze => {
                b.freeze = flag(L, v, key)?;
                let kd = body_kind(kind, b.freeze, b.moving);
                w.phys().world.set_kind(bid, kd);
            }
            A::CanSleep => {
                b.can_sleep = flag(L, v, key)?;
                let on = b.can_sleep;
                w.phys().world.set_can_sleep(bid, on);
            }
            A::ContinuousCd => {
                b.ccd = flag(L, v, key)?;
                let on = b.ccd;
                w.phys().world.set_bullet(bid, on);
            }
            A::LockRotation => {
                b.lock_rotation = flag(L, v, key)?;
                let on = b.lock_rotation;
                w.phys().world.set_fixed_rotation(bid, on);
            }
            A::EngineForce => b.engine = x()?,
            A::Brake => b.brake = x()?.max(0.0),
            A::Steering => b.steering = x()?,
            _ => return Err(format!("{} has no property `{key}`", kind.name())),
        }
        if reshape {
            let shapes: Vec<Shape3> = body!(w, id).shapes.iter().map(|s| s.1).collect();
            set_shapes(w, id, shapes)?;
            w.phys().world.wake(bid);
        }
        Ok(())
    }
}

/// A body's velocity and spin are its node's `vx`… and `vrx`…: read and
/// written on the body.
pub fn velocity(w: &World3, id: Node3Id, a: A) -> Option<f32> {
    if let Some(v) = super::nav3d::velocity(w, id, a) {
        return Some(v);
    }
    let p = w.phys.as_deref()?;
    if let Some(c) = p.chars.get(&id.to_bits()) {
        return Some(match a {
            A::Vx => c.vel.x,
            A::Vy => c.vel.y,
            A::Vz => c.vel.z,
            A::Vrx | A::Vry | A::Vrz | A::Gravity => 0.0,
            _ => return None,
        });
    }
    let b = p.bodies.get(&id.to_bits())?;
    let v = p.world.velocity(b.id);
    let s = p.world.spin(b.id);
    Some(match a {
        A::Vx => v.x,
        A::Vy => v.y,
        A::Vz => v.z,
        A::Vrx => s.x,
        A::Vry => s.y,
        A::Vrz => s.z,
        A::Gravity => 0.0,
        _ => return None,
    })
}

/// Writes a body's velocity or spin; true when it was a body's.
pub fn set_velocity(w: &mut World3, id: Node3Id, a: A, x: f32) -> bool {
    if super::nav3d::set_velocity(w, id, a) {
        return true;
    }
    let Some(p) = w.phys.as_deref_mut() else { return false };
    if let Some(c) = p.chars.get_mut(&id.to_bits()) {
        match a {
            A::Vx => c.vel.x = x,
            A::Vy => c.vel.y = x,
            A::Vz => c.vel.z = x,
            A::Gravity => p.warn("cgravity", "a CharacterBody3D has no gravity of its own: add it to vy in love.update (vy = vy - 18 * dt), as Godot's do"),
            A::Vrx | A::Vry | A::Vrz => p.warn("cspin", "a CharacterBody3D doesn't spin: turn it with ry or faceDirection"),
            _ => return false,
        }
        return true;
    }
    let Some(b) = p.bodies.get(&id.to_bits()) else { return false };
    let bid = b.id;
    let mut v = p.world.velocity(bid);
    let mut s = p.world.spin(bid);
    match a {
        A::Vx => v.x = x,
        A::Vy => v.y = x,
        A::Vz => v.z = x,
        A::Vrx => s.x = x,
        A::Vry => s.y = x,
        A::Vrz => s.z = x,
        A::Gravity => {
            p.warn("gravity", "a body's gravity is the world's (physics.setGravity) times its gravityScale: node.gravity does nothing on a body");
            return true;
        }
        _ => return false,
    }
    p.world.set_velocity(bid, v);
    p.world.set_spin(bid, s);
    p.world.wake(bid);
    true
}

// ------------------------------------------------------------- methods

unsafe fn vec_at(L: *mut lua_State, i: c_int, f: &str) -> Result<Vec3, String> {
    unsafe {
        let n = |k: c_int| -> Result<f32, String> {
            match num(L, k) {
                Some(v) if v.is_finite() => Ok(v as f32),
                _ => Err(format!("bad argument #{} to '{f}' (number expected, got {})", k - 1, type_name(L, k))),
            }
        };
        Ok(vec3(n(i)?, n(i + 1)?, n(i + 2)?))
    }
}

unsafe fn push_nodes(L: *mut lua_State, w: &World3, list: &[Node3Id]) -> c_int {
    unsafe {
        lua_createtable(L, list.len() as c_int, 0);
        for (i, n) in list.iter().enumerate() {
            push_node(L, w, *n);
            lua_rawseti(L, -2, i as c_int + 1);
        }
        1
    }
}

pub unsafe fn method(L: *mut lua_State, h: &mut Host, id: Node3Id, a: A, name: &str) -> Result<c_int, String> {
    unsafe {
        let dt = h.dt;
        let w = h.three.as_deref_mut().ok_or(super::three::NOT_3D)?;
        let k = id.to_bits();
        let kind = w.kind(id);
        if matches!(kind, Kind3::NavRegion | Kind3::NavAgent) {
            return super::nav3d::method(L, w, id, a, name);
        }
        if kind == Kind3::Character {
            let nice = super::nodes3d::nice;
            if a == A::MoveAndSlide {
                return Ok(retb(L, move_and_slide(w, id, dt)));
            }
            let p = w.phys.as_deref().ok_or("no physics")?;
            let c = p.chars.get(&k).ok_or("this character is gone")?;
            let ch = &c.ch;
            let v3 = |L: *mut lua_State, v: Vec3| {
                for x in [v.x, v.y, v.z] {
                    lua_pushnumber(L, nice(x));
                }
                3
            };
            return Ok(match a {
                A::IsOnFloor => retb(L, ch.on_floor()),
                A::IsOnFloorOnly => retb(L, ch.on_floor() && !ch.on_wall() && !ch.on_ceiling()),
                A::IsOnWall => retb(L, ch.on_wall()),
                A::IsOnWallOnly => retb(L, ch.on_wall() && !ch.on_floor() && !ch.on_ceiling()),
                A::IsOnCeiling => retb(L, ch.on_ceiling()),
                A::GetFloorNormal => v3(L, ch.floor_normal()),
                A::GetWallNormal => v3(L, ch.wall_normal()),
                A::GetFloorAngle => ret(L, nice(ch.floor_angle())),
                A::GetRealVelocity => v3(L, ch.real_velocity()),
                A::GetSlideCollisionCount => ret(L, ch.hits().len() as f64),
                A::GetSlideCollision => {
                    let i = match num(L, 2) {
                        Some(i) if i >= 1.0 && (i as usize) <= ch.hits().len() => i as usize - 1,
                        _ => return Err(format!("getSlideCollision(i): i is 1 to getSlideCollisionCount() ({})", ch.hits().len())),
                    };
                    let hit = ch.hits()[i];
                    lua_createtable(L, 0, 7);
                    if let Some(n) = hit.body.and_then(|b| p.node_of(b)) {
                        push_node(L, w, n);
                        lua_setfield(L, -2, c"node".as_ptr());
                    }
                    for (key, v) in [(c"x", hit.point.x), (c"y", hit.point.y), (c"z", hit.point.z), (c"nx", hit.normal.x), (c"ny", hit.normal.y), (c"nz", hit.normal.z)] {
                        lua_pushnumber(L, nice(v));
                        lua_setfield(L, -2, key.as_ptr());
                    }
                    1
                }
                _ => return Err(format!("no method `{name}` on CharacterBody3D")),
            });
        }
        if a == A::IsInContact {
            let p = w.phys();
            return Ok(retb(L, p.wheels.get(&k).is_some_and(|wh| wh.state.on_ground)));
        }
        place_now(w, id);
        let p = w.phys.as_deref_mut().ok_or("no physics")?;
        let b = p.bodies.get(&k).ok_or("this body has no physics (destroyed?)")?;
        let bid = b.id;
        let world = &mut p.world;
        Ok(match a {
            A::ApplyImpulse | A::ApplyForce => {
                let f = vec_at(L, 2, name)?;
                // Godot's position: an offset from the body's origin, in world axes.
                let at = if lua_type(L, 5) == LUA_TNUMBER { vec_at(L, 5, name)? } else { Vec3::ZERO };
                let point = world.position(bid) + at;
                if a == A::ApplyImpulse {
                    world.apply_impulse_at(bid, f, point);
                } else {
                    world.apply_force_at(bid, f, point);
                }
                world.wake(bid);
                0
            }
            A::ApplyCentralImpulse => {
                world.apply_impulse(bid, vec_at(L, 2, name)?);
                world.wake(bid);
                0
            }
            A::ApplyCentralForce => {
                world.apply_force(bid, vec_at(L, 2, name)?);
                world.wake(bid);
                0
            }
            A::ApplyTorque => {
                world.apply_torque(bid, vec_at(L, 2, name)?);
                world.wake(bid);
                0
            }
            A::ApplyTorqueImpulse => {
                world.apply_angular_impulse(bid, vec_at(L, 2, name)?);
                world.wake(bid);
                0
            }
            A::GetCollidingBodies => {
                let list: Vec<Node3Id> = world.contacts(bid).into_iter().filter_map(|o| p.node_of(o)).collect();
                push_nodes(L, w, &list)
            }
            A::GetOverlappingBodies | A::GetOverlappingAreas => {
                let list: Vec<Node3Id> = world.overlapping(bid).into_iter().filter_map(|o| p.node_of(o)).collect();
                let want_area = a == A::GetOverlappingAreas;
                let list: Vec<Node3Id> = list.into_iter().filter(|n| (w.kind(*n) == Kind3::Area) == want_area).collect();
                push_nodes(L, w, &list)
            }
            A::OverlapsBody => {
                let Some(o) = node_at(L, 2) else { return Err(format!("bad argument #1 to '{name}' (3D node expected, got {})", type_name(L, 2))) };
                let ob = p.bodies.get(&o.to_bits()).map(|b| b.id);
                let over = ob.is_some_and(|ob| world.overlapping(bid).contains(&ob)) || (p.ground_node == Some(o) && world.overlapping(bid).iter().any(|x| !p.by_body.contains_key(&x.to_bits())));
                retb(L, over)
            }
            _ => return Err(format!("no method `{name}` on {}", w.kind(id).name())),
        })
    }
}

// ------------------------------------------------------------- physics.*

b!(
    /// `physics.raycast(x1, y1, z1, x2, y2, z2, { mask, exclude, areas })` →
    /// `{ node, x, y, z, nx, ny, nz, distance }` or nil: Godot's `intersect_ray`.
    /// The ground counts wherever the ray meets it, tiles or not.
    raycast, |L, h| {
    let f = "raycast";
    let from = match vec_at(L, 1, f) {
        Ok(v) => v,
        Err(m) => return err(L, &m),
    };
    let to = match vec_at(L, 4, f) {
        Ok(v) => v,
        Err(m) => return err(L, &m),
    };
    let opts = abs(L, 7);
    let mask = super::three::fnum(L, opts, c"mask", u32::MAX as f32);
    let mask = if mask >= u32::MAX as f32 { u32::MAX } else { mask.max(0.0) as u32 };
    let areas = super::three::fflag(L, opts, c"areas");
    let mut exclude: Vec<Node3Id> = Vec::new();
    if lua_type(L, opts) == LUA_TTABLE {
        lua_rawgetfield(L, opts, c"exclude".as_ptr());
        let e = abs(L, -1);
        if let Some(n) = node_at(L, e) {
            exclude.push(n);
        } else if lua_type(L, e) == LUA_TTABLE {
            for k in 1..=lua_objlen(L, e) {
                lua_rawgeti(L, e, k);
                if let Some(n) = node_at(L, -1) {
                    exclude.push(n);
                }
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
    }
    let Some(w) = h.three.as_deref_mut() else { return err(L, super::three::NOT_3D) };
    let mut best: Option<(f32, Node3Id, Vec3, Vec3)> = None;
    if let Some(p) = w.phys.as_deref() {
        let ignore: Vec<p3::BodyId> = exclude.iter().filter_map(|n| p.bodies.get(&n.to_bits()).map(|b| b.id)).collect();
        if let Some(hit) = p.world.raycast_masked(from, to, mask, &ignore, areas) {
            // A ground tile's hit is the terrain's, found below with the rest of the ground.
            if let Some(n) = p.by_body.get(&hit.body.to_bits()) {
                best = Some((hit.distance, *n, hit.point, hit.normal));
            }
        }
    }
    if let Some(g) = w.ground.as_ref().filter(|g| g.collision && mask & 1 != 0 && !exclude.contains(&g.node))
        && let Some((d, at)) = ground_hit(&g.terrain, from, to)
        && best.is_none_or(|b| d < b.0)
    {
        best = Some((d, g.node, at, g.terrain.normal_at(at.x, at.z)));
    }
    let Some((d, n, at, nm)) = best else {
        lua_pushnil(L);
        return 1;
    };
    lua_createtable(L, 0, 8);
    push_node(L, w, n);
    lua_setfield(L, -2, c"node".as_ptr());
    for (k, v) in [(c"x", at.x), (c"y", at.y), (c"z", at.z), (c"nx", nm.x), (c"ny", nm.y), (c"nz", nm.z), (c"distance", d)] {
        lua_pushnumber(L, super::nodes3d::nice(v));
        lua_setfield(L, -2, k.as_ptr());
    }
    1
});

/// Where a segment first meets the true ground: marched in steps of the
/// terrain's detail, then halved to a centimetre.
pub fn ground_hit(t: &gfx3d::terrain::Terrain, from: Vec3, to: Vec3) -> Option<(f32, Vec3)> {
    let len = from.distance(to);
    if !(len > 0.0) {
        return None;
    }
    let dir = (to - from) / len;
    let above = |p: Vec3| p.y - t.height_at(p.x, p.z);
    if above(from) <= 0.0 {
        return Some((0.0, from));
    }
    let step = t.layout().step(0).max(0.25);
    let n = (len / step).ceil().min(20_000.0) as usize;
    let mut prev = 0.0f32;
    for i in 1..=n {
        let d = (i as f32 * step).min(len);
        if above(from + dir * d) <= 0.0 {
            let (mut lo, mut hi) = (prev, d);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if above(from + dir * mid) > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
                if hi - lo < 0.005 {
                    break;
                }
            }
            return Some((hi, from + dir * hi));
        }
        prev = d;
    }
    None
}

b!(set_gravity, |L, h| {
    let g = match vec_at(L, 1, "setGravity") {
        Ok(v) => v,
        Err(m) => return err(L, &m),
    };
    let Some(w) = h.three.as_deref_mut() else { return err(L, super::three::NOT_3D) };
    w.phys().world.set_gravity(g);
    0
});

b!(get_gravity, |L, h| {
    let Some(w) = h.three.as_deref_mut() else { return err(L, super::three::NOT_3D) };
    let g = w.phys().world.gravity();
    for v in [g.x, g.y, g.z] {
        lua_pushnumber(L, super::nodes3d::nice(v));
    }
    3
});

/// For physics3.rs's users: the round's physics, if it has any.
pub fn phys(h: &Host) -> Option<&Phys3> {
    h.three.as_deref().and_then(|w| w.phys.as_deref())
}
