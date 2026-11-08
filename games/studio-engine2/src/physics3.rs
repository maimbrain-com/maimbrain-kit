//! 3D physics in a round (docs/STUDIO-2.md §5 [godot], [physics]; §9.4 row
//! 3a): Godot's bodies as 3D nodes over the SDK's Rapier 3D
//! (`maimbrain::physics3d`). The Lua API is api/bodies3d.rs; this is what a
//! round keeps and what runs each frame, in [`World3::end_frame`] after
//! the velocities and before the flush:
//!
//! 1. **What Lua moved.** A body's node written by Lua (or a tween, or a
//!    parent that moved) since physics last placed it: a dynamic body is
//!    teleported, a frozen, static or area one is moved there over the step
//!    (so it pushes what's in its way, a moving platform).
//! 2. **Vehicles' controls**, and the terrain's ground under the bodies:
//!    heightfield tiles from the true ground (`Terrain::heights`), made
//!    around every dynamic body before the step that needs them, so physics
//!    never depends on what's drawn.
//! 3. **The step**: the SDK's fixed 60 Hz steps from the frame's `dt`.
//! 4. **What happened**: contacts and overlaps become signals
//!    (`bodyEntered` and the rest, heard next frame before love.update);
//!    joints past their `breakForce` or `breakTorque` break.
//! 5. **Back to the nodes**: each moving body's node goes where the body is
//!    drawn (between the last two steps), and each wheel where its ray put
//!    it, turning with its spin and steering.

use std::collections::BTreeMap;

use maimbrain::gfx3d::terrain::Terrain;
use maimbrain::gfx3d::{Mat4, Quat, Vec3, vec3};
use maimbrain::physics3d::{self as p3, BodyId, BodyKind, Event, JointId, ShapeId, VehicleId, World};
use maimbrain::scene::three::Node3Id;

use crate::three::{Kind3, World3};

/// A shape's geometry (Godot's `Shape3D` resources).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Geom3 {
    Box {
        w: f32,
        h: f32,
        d: f32,
    },
    Sphere {
        r: f32,
    },
    Capsule {
        r: f32,
        h: f32,
    },
    Cylinder {
        r: f32,
        h: f32,
    },
    /// A `geometry.*`'s convex hull (by mesh index).
    Convex(u32),
    /// A `geometry.*`'s triangles: static bodies only.
    Concave(u32),
}

/// A shape and where it sits in its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape3 {
    pub geom: Geom3,
    pub pos: Vec3,
    pub rot: Quat,
}

impl Shape3 {
    pub fn type_name(&self) -> &'static str {
        match self.geom {
            Geom3::Box { .. } => "BoxShape3D",
            Geom3::Sphere { .. } => "SphereShape3D",
            Geom3::Capsule { .. } => "CapsuleShape3D",
            Geom3::Cylinder { .. } => "CylinderShape3D",
            Geom3::Convex(_) => "ConvexPolygonShape3D",
            Geom3::Concave(_) => "ConcavePolygonShape3D",
        }
    }
}

/// A joint's kind (Godot's `Joint3D`s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointType {
    Pin,
    Hinge,
    Slider,
    ConeTwist,
    Generic6DOF,
}

impl JointType {
    pub fn name(self) -> &'static str {
        match self {
            JointType::Pin => "PinJoint3D",
            JointType::Hinge => "HingeJoint3D",
            JointType::Slider => "SliderJoint3D",
            JointType::ConeTwist => "ConeTwistJoint3D",
            JointType::Generic6DOF => "Generic6DOFJoint3D",
        }
    }
}

/// A body node's physics.
pub struct Body3 {
    pub id: BodyId,
    pub shapes: Vec<(ShapeId, Shape3)>,
    /// Where physics last put the node (its world place and turn): a node
    /// somewhere else was moved by Lua.
    pub last: Option<(Vec3, Quat)>,
    pub mass: f32,
    pub friction: f32,
    pub bounce: f32,
    pub layer: u32,
    pub mask: u32,
    pub gravity_scale: f32,
    pub linear_damp: f32,
    pub angular_damp: f32,
    pub can_sleep: bool,
    pub ccd: bool,
    pub lock_rotation: bool,
    pub contact_monitor: bool,
    pub monitoring: bool,
    pub freeze: bool,
    /// A static body moved since it was made (it's kinematic from then on).
    pub moving: bool,
    pub vehicle: Option<VehicleId>,
    pub wheels: Vec<Node3Id>,
    pub engine: f32,
    pub brake: f32,
    pub steering: f32,
}

/// A `VehicleWheel3D`: its vehicle, its index there and its tuning.
pub struct Wheel3 {
    pub vehicle: Node3Id,
    pub index: usize,
    pub desc: p3::Wheel,
    pub traction: bool,
    pub steer: bool,
    pub engine: f32,
    pub brake: f32,
    pub steering: f32,
    pub state: p3::WheelState,
}

/// A joint node.
pub struct Joint3 {
    pub id: Option<JointId>,
    pub kind: JointType,
    pub a: Node3Id,
    pub b: Node3Id,
    pub break_force: f32,
    pub break_torque: f32,
    pub force: f32,
    pub torque: f32,
    pub limits: Option<(f32, f32)>,
    pub motor: Option<(f32, f32)>,
}

/// A `CharacterBody3D`: the SDK's character (Rapier's controller with
/// Godot's rules), its velocity (the node's `vx`, `vy`, `vz`, which only
/// `moveAndSlide` moves it by) and its settings as Lua set them.
pub struct Char3 {
    pub ch: p3::Character,
    pub vel: Vec3,
    pub shape: Shape3,
    pub layer: u32,
    /// Where it last put its node (somewhere else: Lua moved it).
    pub last: Option<Vec3>,
    /// An agent's `autoMove` drives it: it stands on the terrain's true
    /// heights rather than its heightfield (nav3.rs), so it needs no tiles.
    pub agent: bool,
    /// On the ground, as the agent's step last found it.
    pub grounded: bool,
}

/// A terrain's heightfield tile: its body and the frame it was last needed.
pub struct Tile {
    pub body: BodyId,
    pub used: u64,
}

/// Quads a side of a ground tile (at the terrain's `detail`).
pub const TILE_QUADS: usize = 32;
/// Frames a tile no body needs is kept before it goes.
const TILE_KEEP: u64 = 120;
/// Tiles at most (each ~5 KB); past this the oldest unneeded go first.
const TILE_MAX: usize = 96;

pub struct Phys3 {
    pub world: World,
    pub bodies: BTreeMap<u32, Body3>,
    pub wheels: BTreeMap<u32, Wheel3>,
    pub joints: BTreeMap<u32, Joint3>,
    pub chars: BTreeMap<u32, Char3>,
    /// The node of each SDK body.
    pub by_body: BTreeMap<u64, Node3Id>,
    pub tiles: BTreeMap<(i32, i32), Tile>,
    /// The terrain node tiles belong to (contacts with the ground name it).
    pub ground_node: Option<Node3Id>,
    pub frame: u64,
    /// The debug menu's Physics tab.
    pub show: bool,
    pub paused: bool,
    /// Steps run by the last frame, and how many in all.
    pub steps: u32,
    pub warned: std::collections::BTreeSet<&'static str>,
}

impl Phys3 {
    pub fn new() -> Phys3 {
        Phys3 {
            world: World::with_gravity(vec3(0.0, -9.8, 0.0)),
            bodies: BTreeMap::new(),
            wheels: BTreeMap::new(),
            joints: BTreeMap::new(),
            chars: BTreeMap::new(),
            by_body: BTreeMap::new(),
            tiles: BTreeMap::new(),
            ground_node: None,
            frame: 0,
            show: false,
            paused: false,
            steps: 0,
            warned: Default::default(),
        }
    }

    pub fn warn(&mut self, key: &'static str, msg: &str) {
        if self.warned.insert(key) {
            maimbrain::sys::log(maimbrain::sys::Level::Warn, msg);
        }
    }

    /// The node an SDK body belongs to (a ground tile's is the terrain).
    pub fn node_of(&self, b: BodyId) -> Option<Node3Id> {
        if let Some(n) = self.by_body.get(&b.to_bits()) {
            return Some(*n);
        }
        self.tiles.values().any(|t| t.body == b).then_some(self.ground_node).flatten()
    }

    /// Bodies awake and in all (the debug menu), ground tiles not counted.
    pub fn counts(&self) -> (usize, usize) {
        (self.world.awake_count(), self.bodies.len())
    }

    /// Forgets destroyed nodes' bodies, wheels and joints.
    pub fn forget(&mut self, gone: &[Node3Id]) {
        for id in gone {
            let k = id.to_bits();
            if let Some(b) = self.bodies.remove(&k) {
                if let Some(v) = b.vehicle {
                    self.world.remove_vehicle(v);
                }
                self.by_body.remove(&b.id.to_bits());
                self.world.remove(b.id);
            }
            if let Some(w) = self.wheels.remove(&k) {
                // A wheel taken off stops pushing.
                if let Some(v) = self.bodies.get(&w.vehicle.to_bits()).and_then(|b| b.vehicle) {
                    self.world.set_wheel(v, w.index, 0.0, 0.0, 0.0);
                    self.world.tune_wheel(v, w.index, &p3::Wheel { max_force: 0.0, stiffness: 0.0, ..w.desc });
                }
            }
            if let Some(c) = self.chars.remove(&k) {
                self.by_body.remove(&c.ch.body().to_bits());
                c.ch.remove(&mut self.world);
            }
            if let Some(j) = self.joints.remove(&k)
                && let Some(jid) = j.id
                && self.world.has_joint(jid)
            {
                self.world.unjoin(jid);
            }
        }
        // Joints whose bodies went.
        let dead: Vec<u32> = self.joints.iter().filter(|(_, j)| gone.contains(&j.a) || gone.contains(&j.b)).map(|(k, _)| *k).collect();
        for k in dead {
            if let Some(j) = self.joints.get_mut(&k) {
                j.id = None;
            }
        }
    }

    /// The ground's tiles near (x, z), radius `r`, are made again (after
    /// `raise`).
    pub fn ground_changed(&mut self, x: f32, z: f32, r: f32, tile: f32) {
        let keys: Vec<(i32, i32)> = self
            .tiles
            .keys()
            .copied()
            .filter(|&(i, j)| {
                let (x0, z0) = (i as f32 * tile, j as f32 * tile);
                x + r >= x0 && x - r <= x0 + tile && z + r >= z0 && z - r <= z0 + tile
            })
            .collect();
        for k in keys {
            if let Some(t) = self.tiles.remove(&k) {
                self.world.remove(t.body);
            }
        }
    }
}

impl Default for Phys3 {
    fn default() -> Phys3 {
        Phys3::new()
    }
}

/// A matrix's place and turn (scale taken out).
pub fn trs_of(m: &Mat4) -> (Vec3, Quat) {
    let c = |i: usize| vec3(m.0[i * 4], m.0[i * 4 + 1], m.0[i * 4 + 2]).normalize();
    (vec3(m.0[12], m.0[13], m.0[14]), Quat::from_basis(c(0), c(1), c(2)).normalize())
}

fn same(a: (Vec3, Quat), b: (Vec3, Quat)) -> bool {
    a.0.distance(b.0) < 1e-4 && a.1.dot(b.1).abs() > 1.0 - 1e-6
}

/// The SDK body for a body node's kind.
pub fn body_kind(kind: Kind3, freeze: bool, moving: bool) -> BodyKind {
    match kind {
        Kind3::RigidBody | Kind3::Vehicle if freeze => BodyKind::Kinematic,
        Kind3::RigidBody | Kind3::Vehicle => BodyKind::Dynamic,
        Kind3::StaticBody if moving => BodyKind::Kinematic,
        Kind3::StaticBody => BodyKind::Fixed,
        _ => BodyKind::Kinematic,
    }
}

/// The ground under the bodies: a heightfield tile wherever a dynamic body
/// or a character is or soon will be (its bounds and a quarter second of
/// its velocity). Characters move in love.update, before the step, so
/// `moveAndSlide` asks for theirs first too.
pub fn ground_tiles(p: &mut Phys3, terrain: &Terrain) {
    let step = terrain.layout().step(0);
    let size = step * TILE_QUADS as f32;
    let mut need: Vec<(i32, i32)> = Vec::new();
    for b in p.bodies.values() {
        if p.world.kind(b.id) != BodyKind::Dynamic {
            continue;
        }
        let pos = p.world.position(b.id);
        let v = p.world.velocity(b.id);
        let reach = 3.0 + b.shapes.iter().map(|(_, s)| s.pos.length() + radius_of(&s.geom)).fold(0.0f32, f32::max);
        let ahead = pos + v * 0.25;
        let (lo, hi) = (pos.min(ahead) - Vec3::splat(reach), pos.max(ahead) + Vec3::splat(reach));
        let (i0, i1) = ((lo.x / size).floor() as i32, (hi.x / size).floor() as i32);
        let (j0, j1) = ((lo.z / size).floor() as i32, (hi.z / size).floor() as i32);
        for i in i0..=i1 {
            for j in j0..=j1 {
                need.push((i, j));
            }
        }
    }
    for c in p.chars.values().filter(|c| !c.agent) {
        let pos = c.ch.position();
        let reach = 3.0 + radius_of(&c.shape.geom) + c.vel.length() * 0.25;
        let (i0, i1) = (((pos.x - reach) / size).floor() as i32, ((pos.x + reach) / size).floor() as i32);
        let (j0, j1) = (((pos.z - reach) / size).floor() as i32, ((pos.z + reach) / size).floor() as i32);
        for i in i0..=i1 {
            for j in j0..=j1 {
                need.push((i, j));
            }
        }
    }
    need.sort();
    need.dedup();
    let frame = p.frame;
    for k in need {
        if let Some(t) = p.tiles.get_mut(&k) {
            t.used = frame;
            continue;
        }
        let n = TILE_QUADS + 1;
        let (x0, z0) = (k.0 as f32 * size, k.1 as f32 * size);
        let mut heights = Vec::with_capacity(n * n);
        terrain.heights().fill(x0, z0, step, n, &mut heights);
        let centre = vec3(x0 + size / 2.0, 0.0, z0 + size / 2.0);
        let body = p.world.add(p3::Body::fixed(centre), p3::Shape::heightfield(&heights, n, n, step).friction(1.0).layers(1, 1));
        p.tiles.insert(k, Tile { body, used: frame });
    }
    // Tiles nobody has needed for a while go, oldest first past the cap.
    let mut old: Vec<((i32, i32), u64)> = p.tiles.iter().filter(|(_, t)| frame - t.used > TILE_KEEP || p.tiles.len() > TILE_MAX && t.used < frame).map(|(k, t)| (*k, t.used)).collect();
    old.sort_by_key(|o| (o.1, o.0));
    let over = p.tiles.len().saturating_sub(TILE_MAX);
    for (i, (k, used)) in old.into_iter().enumerate() {
        if frame - used > TILE_KEEP || i < over {
            if let Some(t) = p.tiles.remove(&k) {
                p.world.remove(t.body);
            }
        }
    }
}

/// About how far a shape reaches from its centre.
pub fn radius_of(g: &Geom3) -> f32 {
    match *g {
        Geom3::Box { w, h, d } => 0.5 * (w * w + h * h + d * d).sqrt(),
        Geom3::Sphere { r } => r,
        Geom3::Capsule { r, h } => (h * 0.5).max(r),
        Geom3::Cylinder { r, h } => (r * r + h * h * 0.25).sqrt(),
        Geom3::Convex(_) | Geom3::Concave(_) => 2.0,
    }
}

impl World3 {
    /// The round's physics, made with its first body.
    pub fn phys(&mut self) -> &mut Phys3 {
        self.phys.get_or_insert_with(|| Box::new(Phys3::new()))
    }

    /// One frame of physics (the module's notes, 1–5).
    pub fn physics_step(&mut self, dt: f32) {
        if self.phys.is_none() {
            return;
        }
        // 1. What Lua moved.
        let keys: Vec<u32> = self.phys.as_ref().unwrap().bodies.keys().copied().collect();
        for k in &keys {
            let id = Node3Id::from_bits(*k);
            let Some(m) = self.scene.world(id) else { continue };
            let now = trs_of(&m);
            let kind = self.kind(id);
            let p = self.phys.as_deref_mut().unwrap();
            let Some(b) = p.bodies.get_mut(k) else { continue };
            match b.last {
                Some(last) if same(last, now) => continue,
                None => {
                    // Placed for the first time.
                    p.world.set_position(b.id, now.0);
                    p.world.set_rotation(b.id, now.1);
                }
                Some(_) => match p.world.kind(b.id) {
                    BodyKind::Dynamic => {
                        p.world.set_position(b.id, now.0);
                        p.world.set_rotation(b.id, now.1);
                        p.world.wake(b.id);
                    }
                    BodyKind::Fixed => {
                        // A static body that moves is a moving platform from now on.
                        b.moving = true;
                        p.world.set_kind(b.id, body_kind(kind, b.freeze, true));
                        p.world.move_kinematic(b.id, now.0, now.1);
                    }
                    BodyKind::Kinematic => p.world.move_kinematic(b.id, now.0, now.1),
                },
            }
            b.last = Some(now);
        }
        // Characters Lua moved are put there.
        let keys: Vec<u32> = self.phys.as_ref().unwrap().chars.keys().copied().collect();
        for k in keys {
            let Some(m) = self.scene.world(Node3Id::from_bits(k)) else { continue };
            let now = trs_of(&m).0;
            let p = self.phys.as_deref_mut().unwrap();
            let Some(c) = p.chars.get_mut(&k) else { continue };
            if c.last.is_none_or(|l| l.distance(now) > 1e-4) {
                c.ch.set_position(&mut p.world, now);
                c.last = Some(now);
            }
        }
        let p = self.phys.as_deref_mut().unwrap();
        // 2. Vehicles' controls.
        for w in p.wheels.values() {
            let Some(body) = p.bodies.get(&w.vehicle.to_bits()) else { continue };
            let Some(v) = body.vehicle else { continue };
            let engine = w.engine + if w.traction { body.engine } else { 0.0 };
            let steer = w.steering + if w.steer { body.steering } else { 0.0 };
            p.world.set_wheel(v, w.index, engine, w.brake + body.brake, steer);
        }
        // The ground's tiles.
        p.frame += 1;
        match self.ground.as_ref().filter(|g| g.collision) {
            Some(g) => {
                p.ground_node = Some(g.node);
                ground_tiles(p, &g.terrain);
            }
            None => {
                for (_, t) in std::mem::take(&mut p.tiles) {
                    p.world.remove(t.body);
                }
            }
        }
        // 3. The step.
        p.steps = if p.paused { 0 } else { p.world.step(dt) };
        // 4. What happened: signals, and joints that break.
        let events: Vec<Event> = p.world.events().to_vec();
        for e in events {
            match e {
                Event::ContactBegin(c) => contact_signal(self, c.a, c.b, "bodyEntered"),
                Event::ContactEnd { a, b, .. } => contact_signal(self, a, b, "bodyExited"),
                Event::SensorEnter { sensor, other, .. } => sensor_signal(self, sensor, other, true),
                Event::SensorExit { sensor, other, .. } => sensor_signal(self, sensor, other, false),
            }
        }
        let p = self.phys.as_deref_mut().unwrap();
        let mut broke = Vec::new();
        for (k, j) in p.joints.iter_mut() {
            let Some(jid) = j.id else { continue };
            // The SDK's are one substep's impulse over the step: × the substeps is newtons.
            let n = p.world.solver_iterations() as f32;
            j.force = p.world.joint_force(jid) * n;
            j.torque = p.world.joint_torque(jid) * n;
            if (j.break_force > 0.0 && j.force > j.break_force) || (j.break_torque > 0.0 && j.torque > j.break_torque) {
                p.world.unjoin(jid);
                j.id = None;
                broke.push((Node3Id::from_bits(*k), j.force, j.torque));
            }
        }
        for (n, f, t) in broke {
            self.signals.push((n, "broken", vec![f as f64, t as f64]));
        }
        // 5. Back to the nodes.
        let p = self.phys.as_deref_mut().unwrap();
        let mut moves: Vec<(Node3Id, Vec3, Quat)> = Vec::new();
        for (k, b) in &p.bodies {
            if p.world.kind(b.id) != BodyKind::Dynamic {
                continue;
            }
            let t = p.world.draw_transform(b.id);
            moves.push((Node3Id::from_bits(*k), t.pos, t.rot));
        }
        let mut wheel_moves: Vec<(Node3Id, Vec3, Quat)> = Vec::new();
        for (k, w) in p.wheels.iter_mut() {
            let Some(body) = p.bodies.get(&w.vehicle.to_bits()) else { continue };
            let Some(v) = body.vehicle else { continue };
            let Some(s) = p.world.wheels(v).get(w.index).copied() else { continue };
            w.state = s;
            // In the chassis's frame at the step (the node is drawn under
            // the chassis's drawn place, so it rides along smoothly).
            let ct = p.world.transform(body.id);
            let local = ct.rot.conjugate() * (s.center - ct.pos);
            let rot = Quat::from_rotation_y(s.steering) * Quat::from_rotation_x(s.rotation);
            wheel_moves.push((Node3Id::from_bits(*k), local, rot));
        }
        for (id, pos, rot) in moves {
            self.place(id, pos, rot);
            if let Some(b) = self.phys.as_deref_mut().unwrap().bodies.get_mut(&id.to_bits()) {
                b.last = Some((pos, rot));
            }
        }
        for (id, pos, rot) in wheel_moves {
            if let Some(n) = self.scene.node_mut(id) {
                let (ry, rx, rz) = crate::api::nodes3d::euler(rot);
                (n.x, n.y, n.z, n.rx, n.ry, n.rz) = (pos.x, pos.y, pos.z, rx, ry, rz);
            }
        }
    }

    /// Puts a node at a world place, under whatever parent it has, its turn
    /// left as it is (a character's: no trig, so it's the same in wasm).
    pub fn place_pos(&mut self, id: Node3Id, pos: Vec3) {
        let parent = self.scene.get(id).and_then(|n| n.parent());
        let pos = match parent.and_then(|p| self.scene.world(p)) {
            Some(pm) => pm.inverse().transform_point(pos),
            None => pos,
        };
        if let Some(n) = self.scene.node_mut(id) {
            (n.x, n.y, n.z) = (pos.x, pos.y, pos.z);
        }
    }

    /// Puts a node at a world place and turn, under whatever parent it has.
    pub fn place(&mut self, id: Node3Id, pos: Vec3, rot: Quat) {
        let parent = self.scene.get(id).and_then(|n| n.parent());
        let (pos, rot) = match parent.and_then(|p| self.scene.world(p)) {
            Some(pm) => {
                let (pp, pr) = trs_of(&pm);
                (pr.conjugate() * (pos - pp), (pr.conjugate() * rot).normalize())
            }
            None => (pos, rot),
        };
        let (ry, rx, rz) = crate::api::nodes3d::euler(rot);
        if let Some(n) = self.scene.node_mut(id) {
            (n.x, n.y, n.z, n.rx, n.ry, n.rz) = (pos.x, pos.y, pos.z, rx, ry, rz);
        }
    }

    /// Where the debug menu's Physics tab draws: every collider near the
    /// camera as lines on the screen (`w`, `h`: the screen's size).
    pub fn physics_lines(&self, out: &mut Vec<(Vec3, Vec3, u32)>) {
        let Some(p) = self.phys.as_deref().filter(|p| p.show) else { return };
        p.world.debug_lines(self.cam.pos, 60.0, 6000, out);
        self.nav_lines(out);
    }
}

/// `bodyEntered`/`bodyExited` for a RigidBody3D with `contactMonitor`.
fn contact_signal(w: &mut World3, a: BodyId, b: BodyId, name: &'static str) {
    let p = w.phys.as_deref().unwrap();
    let (Some(na), Some(nb)) = (p.node_of(a), p.node_of(b)) else { return };
    for (me, other) in [(na, nb), (nb, na)] {
        if p.bodies.get(&me.to_bits()).is_some_and(|x| x.contact_monitor) {
            w.node_signals.push((me, name, other));
        }
    }
}

/// An Area3D's `bodyEntered`, `areaEntered` and their exits.
fn sensor_signal(w: &mut World3, sensor: BodyId, other: BodyId, enter: bool) {
    let p = w.phys.as_deref().unwrap();
    let (Some(ns), Some(no)) = (p.node_of(sensor), p.node_of(other)) else { return };
    let other_area = w.kind(no) == Kind3::Area;
    let monitoring = p.bodies.get(&ns.to_bits()).is_some_and(|b| b.monitoring);
    let other_monitors = other_area && p.bodies.get(&no.to_bits()).is_some_and(|b| b.monitoring);
    if monitoring {
        let name = match (other_area, enter) {
            (true, true) => "areaEntered",
            (true, false) => "areaExited",
            (false, true) => "bodyEntered",
            (false, false) => "bodyExited",
        };
        w.node_signals.push((ns, name, no));
    }
    // Two areas: the other one hears it too.
    if other_monitors {
        w.node_signals.push((no, if enter { "areaEntered" } else { "areaExited" }, ns));
    }
}
