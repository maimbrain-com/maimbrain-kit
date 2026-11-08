//! 3D rigid-body physics (Rapier), in meters, for mb3d games. Enable it
//! with `maimbrain = { …, features = ["physics3d"] }`; docs/PHYSICS.md is the guide.
//!
//! The same space as `gfx3d`: meters, right-handed, +y up, using the SDK's
//! [`Vec3`], [`Quat`] and [`Transform`]. Sizes follow `MeshData`'s builders,
//! so a body's shape and its mesh are made from the same numbers.
//!
//! ```ignore
//! use maimbrain::gfx3d::{MeshData, Node, vec3};
//! use maimbrain::physics3d::{Body, Shape, World};
//!
//! let mut world = World::new();
//! world.add(Body::fixed(vec3(0.0, -0.5, 0.0)), Shape::cuboid(vec3(20.0, 1.0, 20.0)));
//! let size = vec3(0.2, 1.0, 0.5);
//! let domino = world.add(Body::dynamic(vec3(0.0, 0.5, 0.0)), Shape::cuboid(size));
//! let node = Node::with_mesh(MeshData::cuboid(size).upload().unwrap(), material).unwrap();
//!
//! // update(dt):   world.step(dt);
//! // render():     world.sync(domino, node); gfx3d::render();
//! ```
//!
//! Deterministic like `physics2d`: `enhanced-determinism`, fixed steps from
//! the accumulated `dt`, no clocks, no hash-map iteration.

use rapier3d::math::{Pose as RPose, Rotation as RRot, Vector as RVec};
use rapier3d::parry::shape::TypedShape;
use rapier3d::prelude as rp;

use crate::gfx3d::{MeshData, Node, Quat, Transform, Vec3, vec3};

#[path = "character3d.rs"]
pub mod character;
pub use character::{Character, CharacterSettings};
use crate::physics_clock::{Clock, Inbox};
pub use crate::physics_clock::{DEFAULT_STEP_RATE, MAX_STEPS_PER_CALL};

/// Earth gravity, m/s² (the default for [`World::new`], along −y).
pub const GRAVITY: f32 = 9.81;
/// How far ahead a [`World::set_bullet`] body's contacts are predicted in a
/// step at most (m): 120 m/s at 60 Hz. Faster, the sweep alone catches it.
const SOFT_CCD: f32 = 2.0;

fn rv(v: Vec3) -> RVec {
    RVec::new(v.x, v.y, v.z)
}
fn sv(v: RVec) -> Vec3 {
    vec3(v.x, v.y, v.z)
}
fn rq(q: Quat) -> RRot {
    let q = q.normalize();
    RRot::from_xyzw(q.x, q.y, q.z, q.w)
}
fn sq(q: RRot) -> Quat {
    Quat { x: q.x, y: q.y, z: q.z, w: q.w }
}
fn rpose(pos: Vec3, rot: Quat) -> RPose {
    RPose::from_parts(rv(pos), rq(rot))
}
fn transform_from(p: &RPose) -> Transform {
    Transform { pos: sv(p.translation), rot: sq(p.rotation), scale: Vec3::ONE }
}

// ---------------------------------------------------------------- handles

/// A body in a [`World`]. Stale after [`World::remove`]; calls with a stale
/// id do nothing and queries return defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(rp::RigidBodyHandle);

/// One collision shape attached to a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId(rp::ColliderHandle);

/// A joint between two bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JointId(rp::ImpulseJointHandle);

macro_rules! id_order {
    ($($t:ident),*) => {$(
        impl $t {
            /// A stable number for this id (generation and index), e.g. to sort or store.
            pub fn to_bits(self) -> u64 {
                let (i, g) = self.0.into_raw_parts();
                (g as u64) << 32 | i as u64
            }
        }
        impl PartialOrd for $t {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        /// By creation slot, so sorted lists come out the same every run.
        impl Ord for $t {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                let (a, b) = (self.0.into_raw_parts(), o.0.into_raw_parts());
                (a.0, a.1).cmp(&(b.0, b.1))
            }
        }
    )*};
}
id_order!(BodyId, ShapeId, JointId);

// ---------------------------------------------------------------- bodies

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Moved by forces, gravity and collisions.
    Dynamic,
    /// Never moves (ground, walls). Cheapest.
    Fixed,
    /// Moved only by you ([`World::set_velocity`], [`World::move_kinematic`]).
    Kinematic,
}

/// How to create a body: `Body::dynamic(vec3(0.0, 2.0, 0.0)).rotation(q)`.
#[derive(Clone, Debug)]
pub struct Body {
    kind: BodyKind,
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    spin: Vec3,
    gravity_scale: f32,
    linear_damping: f32,
    angular_damping: f32,
    bullet: bool,
    can_sleep: bool,
    asleep: bool,
    fixed_rotation: bool,
    tag: u64,
}

impl Body {
    fn new(kind: BodyKind, pos: Vec3) -> Body {
        Body {
            kind,
            pos,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            spin: Vec3::ZERO,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            bullet: false,
            can_sleep: true,
            asleep: false,
            fixed_rotation: false,
            tag: 0,
        }
    }
    pub fn dynamic(pos: Vec3) -> Body {
        Body::new(BodyKind::Dynamic, pos)
    }
    pub fn fixed(pos: Vec3) -> Body {
        Body::new(BodyKind::Fixed, pos)
    }
    pub fn kinematic(pos: Vec3) -> Body {
        Body::new(BodyKind::Kinematic, pos)
    }
    pub fn rotation(mut self, rot: Quat) -> Body {
        self.rot = rot;
        self
    }
    /// Initial velocity, m/s.
    pub fn velocity(mut self, v: Vec3) -> Body {
        self.vel = v;
        self
    }
    /// Initial angular velocity: axis × radians/s.
    pub fn spin(mut self, w: Vec3) -> Body {
        self.spin = w;
        self
    }
    pub fn gravity_scale(mut self, s: f32) -> Body {
        self.gravity_scale = s;
        self
    }
    /// Drag: about this fraction of velocity lost per second.
    pub fn damping(mut self, linear: f32, angular: f32) -> Body {
        self.linear_damping = linear;
        self.angular_damping = angular;
        self
    }
    /// Continuous collision detection, for small fast things.
    pub fn bullet(mut self) -> Body {
        self.bullet = true;
        self
    }
    pub fn always_awake(mut self) -> Body {
        self.can_sleep = false;
        self
    }
    /// Start asleep: a lone object that waits for a hit. Not for stacks or
    /// rows that lean on each other: sleeping bodies that were never
    /// simulated touching aren't linked, so the rest of a stack would float.
    pub fn asleep(mut self) -> Body {
        self.asleep = true;
        self
    }
    /// Never rotates (character capsules).
    pub fn fixed_rotation(mut self) -> Body {
        self.fixed_rotation = true;
        self
    }
    /// Your own number for this body; read it with [`World::tag`].
    pub fn tag(mut self, tag: u64) -> Body {
        self.tag = tag;
        self
    }

    fn build(&self) -> rp::RigidBody {
        let b = match self.kind {
            BodyKind::Dynamic => rp::RigidBodyBuilder::dynamic(),
            BodyKind::Fixed => rp::RigidBodyBuilder::fixed(),
            BodyKind::Kinematic => rp::RigidBodyBuilder::kinematic_velocity_based(),
        };
        let mut b = b
            .pose(rpose(self.pos, self.rot))
            .linvel(rv(self.vel))
            .angvel(rv(self.spin))
            .gravity_scale(self.gravity_scale)
            .linear_damping(self.linear_damping)
            .angular_damping(self.angular_damping)
            .ccd_enabled(self.bullet)
            .can_sleep(self.can_sleep)
            .sleeping(self.asleep)
            .user_data(self.tag as u128);
        if self.fixed_rotation {
            b = b.lock_rotations();
        }
        b.build()
    }
}

// ---------------------------------------------------------------- shapes

#[derive(Clone, Debug)]
enum Geom {
    Cuboid(Vec3),
    RoundCuboid(Vec3, f32),
    Sphere(f32),
    Capsule(f32, f32),
    Cylinder(f32, f32),
    Cone(f32, f32),
    Convex(Vec<Vec3>),
    Trimesh(Vec<Vec3>, Vec<[u32; 3]>),
    /// Heights in rows along z (x fastest), `nx` × `nz` of them, `step` m apart.
    HeightField(Vec<f32>, usize, usize, f32),
}

/// A collision shape and its material: `Shape::cuboid(vec3(1.0, 0.2, 0.5)).friction(0.8)`.
/// Centered on the body unless moved with [`Shape::at`].
#[derive(Clone, Debug)]
pub struct Shape {
    geom: Geom,
    offset: Vec3,
    rot: Quat,
    density: f32,
    mass: Option<f32>,
    friction: f32,
    restitution: f32,
    sensor: bool,
    groups: (u32, u32),
    /// Godot's rule for `groups`: either's layer in the other's mask.
    either: bool,
    events: bool,
}

impl Shape {
    fn new(geom: Geom) -> Shape {
        Shape {
            geom,
            offset: Vec3::ZERO,
            rot: Quat::IDENTITY,
            density: 1000.0,
            mass: None,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            groups: (1, u32::MAX),
            either: false,
            events: true,
        }
    }
    /// A box `size` (full width, height, depth), like `MeshData::cuboid(size)`.
    pub fn cuboid(size: Vec3) -> Shape {
        Shape::new(Geom::Cuboid(vec3(size.x.abs(), size.y.abs(), size.z.abs()) * 0.5))
    }
    pub fn cube(size: f32) -> Shape {
        Shape::cuboid(Vec3::splat(size))
    }
    /// A box with rounded edges (rolls a little; stacks and tumbles nicely).
    pub fn round_cuboid(size: Vec3, radius: f32) -> Shape {
        let h = vec3(size.x.abs(), size.y.abs(), size.z.abs()) * 0.5;
        let r = radius.abs().min(h.x).min(h.y).min(h.z);
        Shape::new(Geom::RoundCuboid(h - Vec3::splat(r), r))
    }
    /// Like `MeshData::uv_sphere(radius, ..)` / `icosphere(radius, ..)`.
    pub fn sphere(radius: f32) -> Shape {
        Shape::new(Geom::Sphere(radius.abs()))
    }
    /// An upright pill `height` tall in all (caps included), along +y.
    pub fn capsule(radius: f32, height: f32) -> Shape {
        Shape::new(Geom::Capsule(radius.abs(), (height.abs() / 2.0 - radius.abs()).max(0.0)))
    }
    /// Upright along +y, centered, like `MeshData::cylinder(radius, height, ..)`.
    pub fn cylinder(radius: f32, height: f32) -> Shape {
        Shape::new(Geom::Cylinder(radius.abs(), height.abs() / 2.0))
    }
    /// Point up (+y), centered on half its height, like `MeshData::cone(radius, height, ..)`.
    pub fn cone(radius: f32, height: f32) -> Shape {
        Shape::new(Geom::Cone(radius.abs(), height.abs() / 2.0))
    }
    /// The convex hull of `points` (in the body's frame).
    pub fn convex(points: &[Vec3]) -> Shape {
        Shape::new(Geom::Convex(points.to_vec()))
    }
    /// The convex hull of a mesh's vertices: a faceted rock collides as drawn.
    pub fn convex_hull_of(mesh: &MeshData) -> Shape {
        Shape::convex(&mesh.positions)
    }
    /// A triangle mesh: exact but hollow, for fixed scenery (terrain, ramps,
    /// a marble run). Moving things should use convex shapes.
    pub fn trimesh(mesh: &MeshData) -> Shape {
        let tris = mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        Shape::new(Geom::Trimesh(mesh.positions.clone(), tris))
    }

    /// Ground from a grid of heights, centered on the body: `heights` in
    /// rows along z with x fastest, `nx` × `nz` of them (2 or more each),
    /// `step` meters apart. Fixed scenery only (a terrain's tiles). Its
    /// internal edges are smoothed, so things slide and roll across the
    /// grid without catching on it.
    pub fn heightfield(heights: &[f32], nx: usize, nz: usize, step: f32) -> Shape {
        Shape::new(Geom::HeightField(heights.to_vec(), nx, nz, step.abs()))
    }

    /// Moves the shape within its body (compound bodies).
    pub fn at(mut self, offset: Vec3) -> Shape {
        self.offset = offset;
        self
    }
    /// Turns the shape within its body.
    pub fn rotated(mut self, rot: Quat) -> Shape {
        self.rot = rot;
        self
    }
    /// kg/m³; default 1000 (water; wood ~600, stone ~2500, steel ~7800).
    pub fn density(mut self, d: f32) -> Shape {
        self.density = d.max(0.0);
        self.mass = None;
        self
    }
    /// This shape's mass in kg, instead of from density.
    pub fn mass(mut self, kg: f32) -> Shape {
        self.mass = Some(kg.max(0.0));
        self
    }
    /// 0 ice … 0.5 default … 1+ rubber.
    pub fn friction(mut self, f: f32) -> Shape {
        self.friction = f.max(0.0);
        self
    }
    /// 0 thud (default) … 1 perfectly bouncy.
    pub fn restitution(mut self, r: f32) -> Shape {
        self.restitution = r.max(0.0);
        self
    }
    /// Detects overlaps without pushing anything.
    pub fn sensor(mut self) -> Shape {
        self.sensor = true;
        self
    }
    /// Collision layers, as in `physics2d::Shape::groups`. Default: layer 1, colliding with all.
    pub fn groups(mut self, member: u32, collides_with: u32) -> Shape {
        self.groups = (member, collides_with);
        self
    }
    /// Godot's collision layers: the shape is on `layer` and looks for
    /// `mask`, and two shapes touch when either's layer is in the other's
    /// mask (where [`Shape::groups`] needs both).
    pub fn layers(mut self, layer: u32, mask: u32) -> Shape {
        self.groups = (layer, mask);
        self.either = true;
        self
    }
    /// No contact events for this shape.
    pub fn quiet(mut self) -> Shape {
        self.events = false;
        self
    }

    /// A mesh matching this shape (for primitives; `None` for convex hulls
    /// and trimeshes, which came from your own mesh). `detail` ≈ segments.
    pub fn mesh(&self, detail: u32) -> Option<MeshData> {
        let n = detail.clamp(6, 64);
        let m = match &self.geom {
            Geom::Cuboid(h) => MeshData::cuboid(*h * 2.0),
            Geom::RoundCuboid(h, r) => MeshData::cuboid((*h + Vec3::splat(*r)) * 2.0),
            Geom::Sphere(r) => MeshData::uv_sphere(*r, n, n / 2),
            Geom::Capsule(r, hh) => {
                let mut prof = Vec::new();
                let k = n / 2;
                for i in 0..=k {
                    let a = std::f32::consts::FRAC_PI_2 * i as f32 / k as f32;
                    prof.push([r * libm::sinf(a), -hh - r * libm::cosf(a)]);
                }
                for i in 0..=k {
                    let a = std::f32::consts::FRAC_PI_2 * i as f32 / k as f32;
                    prof.push([r * libm::cosf(a), hh + r * libm::sinf(a)]);
                }
                MeshData::lathe(&prof, n)
            }
            Geom::Cylinder(r, hh) => MeshData::cylinder(*r, hh * 2.0, n),
            Geom::Cone(r, hh) => MeshData::cone(*r, hh * 2.0, n),
            Geom::Convex(_) | Geom::Trimesh(..) | Geom::HeightField(..) => return None,
        };
        Some(m.transformed(&Transform::at(self.offset).with_rot(self.rot).matrix()))
    }

    fn build(&self) -> rp::ColliderBuilder {
        let fallback = || rp::ColliderBuilder::ball(0.01);
        let b = match &self.geom {
            Geom::Cuboid(h) => rp::ColliderBuilder::cuboid(h.x, h.y, h.z),
            Geom::RoundCuboid(h, r) => rp::ColliderBuilder::round_cuboid(h.x, h.y, h.z, *r),
            Geom::Sphere(r) => rp::ColliderBuilder::ball(*r),
            Geom::Capsule(r, hh) => rp::ColliderBuilder::capsule_y(*hh, *r),
            Geom::Cylinder(r, hh) => rp::ColliderBuilder::cylinder(*hh, *r),
            Geom::Cone(r, hh) => rp::ColliderBuilder::cone(*hh, *r),
            Geom::Convex(pts) => {
                let pts: Vec<RVec> = pts.iter().map(|p| rv(*p)).collect();
                rp::ColliderBuilder::convex_hull(&pts).unwrap_or_else(|| {
                    warn("Shape::convex needs 4+ points that aren't all on a plane; using a dot");
                    fallback()
                })
            }
            Geom::Trimesh(v, t) => {
                let n = v.len() as u32;
                if t.is_empty() || t.iter().flatten().any(|&i| i >= n) {
                    warn("Shape::trimesh: no triangles, or an index out of range; using a dot");
                    fallback()
                } else {
                    rp::ColliderBuilder::trimesh(v.iter().map(|p| rv(*p)).collect(), t.clone()).unwrap_or_else(|_| {
                        warn("Shape::trimesh: degenerate mesh; using a dot");
                        fallback()
                    })
                }
            }
            Geom::HeightField(h, nx, nz, step) => {
                if *nx < 2 || *nz < 2 || h.len() != nx * nz || h.iter().any(|v| !v.is_finite()) {
                    warn("Shape::heightfield: needs nx × nz finite heights, 2 or more each way; using a dot");
                    fallback()
                } else {
                    // parry's grid is column-major, rows along z: (i, j) at i + j × rows.
                    let mut data = vec![0.0; h.len()];
                    for i in 0..*nz {
                        for j in 0..*nx {
                            data[i + j * nz] = h[i * nx + j];
                        }
                    }
                    let grid = rapier3d::parry::utils::Array2::new(*nz, *nx, data);
                    let scale = RVec::new((nx - 1) as f32 * step, 1.0, (nz - 1) as f32 * step);
                    rp::ColliderBuilder::heightfield_with_flags(grid, scale, rapier3d::parry::shape::HeightFieldFlags::FIX_INTERNAL_EDGES)
                }
            }
        };
        let groups = rp::InteractionGroups::new(
            rp::Group::from_bits_retain(self.groups.0),
            rp::Group::from_bits_retain(self.groups.1),
            if self.either { rp::InteractionTestMode::Or } else { rp::InteractionTestMode::And },
        );
        let b = b
            .position(rpose(self.offset, self.rot))
            .friction(self.friction)
            .restitution(self.restitution)
            .sensor(self.sensor)
            .collision_groups(groups)
            .active_events(if self.events { rp::ActiveEvents::COLLISION_EVENTS } else { rp::ActiveEvents::empty() });
        match self.mass {
            Some(m) => b.mass(m),
            None => b.density(self.density),
        }
    }
}

fn warn(msg: &str) {
    crate::sys::log(crate::sys::Level::Warn, &format!("physics3d: {msg}"));
}

// ---------------------------------------------------------------- joints

#[derive(Clone, Debug)]
enum JointKind {
    Revolute(Vec3, Vec3),
    Prismatic(Vec3, Vec3),
    Spherical(Vec3),
    Fixed,
    Rope(Vec3, Vec3),
    Spring(Vec3, Vec3),
}

/// How to connect two bodies. Anchors and axes are in world space, where the
/// bodies are *now*: create joints after placing the bodies. Joined bodies
/// don't collide with each other unless you ask ([`Joint::collide`]).
#[derive(Clone, Debug)]
pub struct Joint {
    a: BodyId,
    b: BodyId,
    kind: JointKind,
    limits: Option<(f32, f32)>,
    motor: Option<(f32, f32)>,
    length: Option<f32>,
    spring: (f32, f32),
    collide: bool,
}

impl Joint {
    fn new(a: BodyId, b: BodyId, kind: JointKind) -> Joint {
        Joint { a, b, kind, limits: None, motor: None, length: None, spring: (2.0, 0.3), collide: false }
    }
    /// A hinge at `anchor` turning about `axis`: wheels, doors, flippers.
    pub fn revolute(a: BodyId, b: BodyId, anchor: Vec3, axis: Vec3) -> Joint {
        Joint::new(a, b, JointKind::Revolute(anchor, axis))
    }
    /// A slider through `anchor` along `axis`.
    pub fn prismatic(a: BodyId, b: BodyId, anchor: Vec3, axis: Vec3) -> Joint {
        Joint::new(a, b, JointKind::Prismatic(anchor, axis))
    }
    /// A ball-and-socket at `anchor`: ragdoll shoulders and hips, chains.
    pub fn spherical(a: BodyId, b: BodyId, anchor: Vec3) -> Joint {
        Joint::new(a, b, JointKind::Spherical(anchor))
    }
    /// Glues `b` to `a` as they are now.
    pub fn fixed(a: BodyId, b: BodyId) -> Joint {
        Joint::new(a, b, JointKind::Fixed)
    }
    /// A rope, never longer than now (or [`Joint::length`]).
    pub fn rope(a: BodyId, b: BodyId, anchor_a: Vec3, anchor_b: Vec3) -> Joint {
        Joint::new(a, b, JointKind::Rope(anchor_a, anchor_b))
    }
    /// A spring resting at its current length (or [`Joint::length`]).
    pub fn spring(a: BodyId, b: BodyId, anchor_a: Vec3, anchor_b: Vec3) -> Joint {
        Joint::new(a, b, JointKind::Spring(anchor_a, anchor_b))
    }
    /// Revolute: min/max angle (radians, relative to now). Prismatic: min/max meters.
    pub fn limits(mut self, min: f32, max: f32) -> Joint {
        self.limits = Some((min.min(max), min.max(max)));
        self
    }
    /// Drive a revolute (radians/s about the axis) or prismatic (m/s)
    /// joint, with at most `max_force` (N·m or N; `f32::INFINITY` for no limit).
    pub fn motor(mut self, speed: f32, max_force: f32) -> Joint {
        self.motor = Some((speed, max_force));
        self
    }
    /// Rope: maximum length. Spring: rest length. Meters.
    pub fn length(mut self, m: f32) -> Joint {
        self.length = Some(m.max(0.0));
        self
    }
    /// Spring frequency (Hz) and damping ratio (0–1); mass-independent.
    pub fn stiffness(mut self, hz: f32, damping_ratio: f32) -> Joint {
        self.spring = (hz.max(0.0), damping_ratio.max(0.0));
        self
    }
    pub fn collide(mut self, yes: bool) -> Joint {
        self.collide = yes;
        self
    }
}

// ---------------------------------------------------------------- events

/// Two solid shapes started touching.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub a: BodyId,
    pub b: BodyId,
    pub shape_a: ShapeId,
    pub shape_b: ShapeId,
    pub point: Vec3,
    /// Unit normal from `a` towards `b`.
    pub normal: Vec3,
    /// Closing speed along the normal just before touching, m/s: use it for
    /// sound volume and haptics. Resting contacts are near 0.
    pub speed: f32,
    /// The impulse that stopped them, N·s.
    pub impulse: f32,
}

impl Contact {
    pub fn involves(&self, id: BodyId) -> bool {
        self.a == id || self.b == id
    }
    pub fn other(&self, id: BodyId) -> Option<BodyId> {
        if self.a == id {
            Some(self.b)
        } else if self.b == id {
            Some(self.a)
        } else {
            None
        }
    }
}

/// What happened during the last [`World::step`], in a deterministic order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    ContactBegin(Contact),
    ContactEnd { a: BodyId, b: BodyId, shape_a: ShapeId, shape_b: ShapeId },
    SensorEnter { sensor: BodyId, other: BodyId, sensor_shape: ShapeId, other_shape: ShapeId },
    SensorExit { sensor: BodyId, other: BodyId, sensor_shape: ShapeId, other_shape: ShapeId },
}

impl Event {
    pub fn bodies(&self) -> (BodyId, BodyId) {
        match *self {
            Event::ContactBegin(c) => (c.a, c.b),
            Event::ContactEnd { a, b, .. } => (a, b),
            Event::SensorEnter { sensor, other, .. } | Event::SensorExit { sensor, other, .. } => (sensor, other),
        }
    }
    pub fn involves(&self, id: BodyId) -> bool {
        let (a, b) = self.bodies();
        a == id || b == id
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub body: BodyId,
    pub shape: ShapeId,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

#[derive(Clone, Copy)]
struct RawEvent {
    begin: bool,
    c1: rp::ColliderHandle,
    c2: rp::ColliderHandle,
    sensor: bool,
    speed: f32,
    point: RVec,
    normal: RVec,
}

struct Collector(Inbox<RawEvent>);

impl rp::EventHandler for Collector {
    fn handle_collision_event(
        &self,
        bodies: &rp::RigidBodySet,
        colliders: &rp::ColliderSet,
        event: rp::CollisionEvent,
        pair: Option<&rp::ContactPair>,
    ) {
        let (c1, c2) = (event.collider1(), event.collider2());
        let mut raw = RawEvent {
            begin: event.started(),
            c1,
            c2,
            sensor: event.sensor(),
            speed: 0.0,
            point: RVec::ZERO,
            normal: RVec::ZERO,
        };
        if raw.begin && !raw.sensor {
            let body_of = |c| colliders.get(c).and_then(|c: &rp::Collider| c.parent()).and_then(|b| bodies.get(b));
            let (b1, b2) = (body_of(c1), body_of(c2));
            if let Some((m, sc)) = pair.and_then(|p| {
                p.manifolds.iter().filter_map(|m| m.data.solver_contacts.first().map(|sc| (m, sc))).next()
            }) {
                raw.point = sc.point;
                raw.normal = m.data.normal;
            } else if let Some(c) = colliders.get(c1) {
                raw.point = c.position().translation;
            }
            let vel = |b: Option<&rp::RigidBody>| b.map_or(RVec::ZERO, |b| b.velocity_at_point(raw.point));
            let rel = vel(b2) - vel(b1);
            raw.speed = if raw.normal != RVec::ZERO { rel.dot(raw.normal).abs() } else { rel.length() };
        }
        self.0.push(raw);
    }

    fn handle_contact_force_event(
        &self,
        _dt: f32,
        _bodies: &rp::RigidBodySet,
        _colliders: &rp::ColliderSet,
        _pair: &rp::ContactPair,
        _total_force_magnitude: f32,
    ) {
    }
}

// ---------------------------------------------------------------- world

/// The physics world. Create it in `init`, [`World::step`] it in `update`,
/// [`World::sync`] nodes to bodies in `render`.
pub struct World {
    w: rp::PhysicsWorld,
    clock: Clock,
    inbox: Collector,
    events: Vec<Event>,
    prev: Vec<Option<(rp::RigidBodyHandle, RPose)>>,
    interpolate: bool,
    forces: Vec<(rp::RigidBodyHandle, RVec, RVec)>,
    owed: Vec<(rp::RigidBodyHandle, RVec, RVec)>,
    /// Kinematic bodies moved by the last step's targets (stopped at the
    /// next step unless given a new target).
    moved: Vec<rp::RigidBodyHandle>,
    targets: Vec<(rp::RigidBodyHandle, Vec3, Quat)>,
    steps: u64,
    /// Ray-cast vehicles, by [`VehicleId`] (None once removed).
    vehicles: Vec<Option<Vehicle>>,
}

impl Default for World {
    fn default() -> World {
        World::new()
    }
}

impl World {
    /// Earth gravity along −y.
    pub fn new() -> World {
        World::with_gravity(vec3(0.0, -GRAVITY, 0.0))
    }

    pub fn with_gravity(g: Vec3) -> World {
        let mut w = rp::PhysicsWorld::new();
        w.gravity = rv(g);
        w.integration_parameters.dt = 1.0 / DEFAULT_STEP_RATE;
        World {
            w,
            clock: Clock::new(),
            inbox: Collector(Inbox::new()),
            events: Vec::new(),
            prev: Vec::new(),
            interpolate: true,
            forces: Vec::new(),
            owed: Vec::new(),
            targets: Vec::new(),
            moved: Vec::new(),
            steps: 0,
            vehicles: Vec::new(),
        }
    }

    /// Solver iterations per step: Rapier's substeps, so a joint's impulse
    /// ([`World::joint_force`]) is one substep's, and × this is the step's.
    pub fn solver_iterations(&self) -> u32 {
        self.w.integration_parameters.num_solver_iterations as u32
    }
    pub fn gravity(&self) -> Vec3 {
        sv(self.w.gravity)
    }
    pub fn set_gravity(&mut self, g: Vec3) {
        self.w.gravity = rv(g);
    }
    /// Fixed steps per second (default 60). 15–480.
    pub fn set_step_rate(&mut self, hz: f32) {
        self.clock.set_rate(hz);
        self.w.integration_parameters.dt = self.clock.h();
    }
    /// Solver iterations per step (default 4).
    pub fn set_solver_iterations(&mut self, n: u32) {
        self.w.integration_parameters.num_solver_iterations = n.clamp(1, 32) as usize;
    }
    pub fn set_interpolation(&mut self, on: bool) {
        self.interpolate = on;
    }

    // ------------------------------------------------------------ stepping

    /// Advances by `dt` (pass `update`'s `dt`) in fixed steps; returns how
    /// many ran. Deterministic. Clears and refills [`World::events`].
    pub fn step(&mut self, dt: f32) -> u32 {
        self.events.clear();
        let n = self.clock.advance(dt);
        for (h, f, t) in self.forces.drain(..) {
            match self.owed.iter_mut().find(|o| o.0 == h) {
                Some(o) => {
                    o.1 += f * dt;
                    o.2 += t * dt;
                }
                None => self.owed.push((h, f * dt, t * dt)),
            }
        }
        if n == 0 {
            return 0;
        }
        let span = n as f32 * self.clock.h();
        for &(h, imp, ang) in &self.owed {
            if let Some(b) = self.w.bodies.get_mut(h) {
                b.reset_forces(true);
                b.reset_torques(true);
                b.add_force(imp / span, true);
                b.add_torque(ang / span, true);
            }
        }
        for h in std::mem::take(&mut self.moved) {
            if !self.targets.iter().any(|t| t.0 == h)
                && let Some(b) = self.w.bodies.get_mut(h)
            {
                b.set_linvel(RVec::ZERO, false);
                b.set_angvel(RVec::ZERO, false);
            }
        }
        for &(h, pos, rot) in &self.targets {
            if let Some(b) = self.w.bodies.get_mut(h) {
                let cur = *b.position();
                b.set_linvel((rv(pos) - cur.translation) / span, true);
                // The rotation from here to there, as axis × angle / time.
                let mut d = rq(rot) * cur.rotation.inverse();
                if d.w < 0.0 {
                    d = -d;
                }
                b.set_angvel(d.to_scaled_axis() / span, true);
            }
        }
        for i in 0..n {
            if i == n - 1 && self.interpolate {
                self.snapshot();
            }
            self.update_vehicles();
            self.w.step_with_events(&(), &self.inbox);
            self.steps += 1;
            self.collect_events();
        }
        for (h, _, _) in self.owed.drain(..) {
            if let Some(b) = self.w.bodies.get_mut(h) {
                b.reset_forces(false);
                b.reset_torques(false);
            }
        }
        self.moved = self.targets.drain(..).map(|t| t.0).collect();
        n
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn alpha(&self) -> f32 {
        self.clock.alpha()
    }

    fn snapshot(&mut self) {
        self.prev.clear();
        for (h, b) in self.w.bodies.iter() {
            if b.is_fixed() {
                continue;
            }
            let i = h.into_raw_parts().0 as usize;
            if i >= self.prev.len() {
                self.prev.resize(i + 1, None);
            }
            self.prev[i] = Some((h, *b.position()));
        }
    }

    fn collect_events(&mut self) {
        for raw in self.inbox.0.take() {
            let parent = |c| self.w.colliders.get(c).and_then(|c| c.parent()).map(BodyId);
            let (Some(a), Some(b)) = (parent(raw.c1), parent(raw.c2)) else { continue };
            let (s1, s2) = (ShapeId(raw.c1), ShapeId(raw.c2));
            let ev = if raw.sensor {
                let first_is_sensor = self.w.colliders.get(raw.c1).is_some_and(|c| c.is_sensor());
                let (sensor, other, sensor_shape, other_shape) =
                    if first_is_sensor { (a, b, s1, s2) } else { (b, a, s2, s1) };
                if raw.begin {
                    Event::SensorEnter { sensor, other, sensor_shape, other_shape }
                } else {
                    Event::SensorExit { sensor, other, sensor_shape, other_shape }
                }
            } else if raw.begin {
                let pair = self.w.narrow_phase.contact_pair(raw.c1, raw.c2);
                let impulse = pair.map_or(0.0, |p| p.total_impulse_magnitude());
                let flip = pair.is_some_and(|p| p.collider1 != raw.c1);
                Event::ContactBegin(Contact {
                    a,
                    b,
                    shape_a: s1,
                    shape_b: s2,
                    point: sv(raw.point),
                    normal: sv(if flip { -raw.normal } else { raw.normal }).normalize(),
                    speed: raw.speed,
                    impulse,
                })
            } else {
                Event::ContactEnd { a, b, shape_a: s1, shape_b: s2 }
            };
            self.events.push(ev);
        }
    }

    // ------------------------------------------------------------ bodies

    pub fn add(&mut self, body: Body, shape: Shape) -> BodyId {
        let id = self.add_body(body);
        self.add_shape(id, shape);
        id
    }
    pub fn add_body(&mut self, body: Body) -> BodyId {
        BodyId(self.w.insert_body(body.build()))
    }
    pub fn add_shape(&mut self, body: BodyId, shape: Shape) -> ShapeId {
        let id = ShapeId(self.w.insert_collider(shape.build(), Some(body.0)));
        self.refresh(body);
        id
    }

    /// Puts a body's shapes where the body is now in the query structures,
    /// so raycasts and point queries see bodies added or teleported since
    /// the last step right away (the step itself then carries on as usual;
    /// removed bodies are skipped by queries already).
    fn refresh(&mut self, id: BodyId) {
        let Some(b) = self.w.bodies.get(id.0) else { return };
        let pose = *b.position();
        let handles: Vec<_> = b.colliders().to_vec();
        let pred = self.w.integration_parameters.prediction_distance();
        for h in handles {
            if let Some(c) = self.w.colliders.get_mut(h) {
                let local = c.position_wrt_parent().copied().unwrap_or(RPose::identity());
                c.set_position(pose * local);
                let aabb = c.compute_collision_aabb(pred);
                self.w.broad_phase.set_aabb(&self.w.integration_parameters, h, aabb);
            }
        }
    }
    /// Removes a body with its shapes and joints (its contacts end silently).
    pub fn remove(&mut self, id: BodyId) {
        self.w.remove_body(id.0);
    }
    pub fn remove_shape(&mut self, id: ShapeId) {
        self.w.remove_collider(id.0);
    }
    pub fn contains(&self, id: BodyId) -> bool {
        self.w.bodies.contains(id.0)
    }
    pub fn bodies(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.w.bodies.iter().map(|(h, _)| BodyId(h))
    }
    pub fn len(&self) -> usize {
        self.w.bodies.len()
    }
    pub fn is_empty(&self) -> bool {
        self.w.bodies.is_empty()
    }
    pub fn shapes(&self, id: BodyId) -> Vec<ShapeId> {
        self.w.bodies.get(id.0).map_or_else(Vec::new, |b| b.colliders().iter().map(|&c| ShapeId(c)).collect())
    }
    pub fn body_of(&self, shape: ShapeId) -> Option<BodyId> {
        self.w.colliders.get(shape.0).and_then(|c| c.parent()).map(BodyId)
    }

    fn body(&self, id: BodyId) -> Option<&rp::RigidBody> {
        self.w.bodies.get(id.0)
    }
    fn body_mut(&mut self, id: BodyId) -> Option<&mut rp::RigidBody> {
        self.w.bodies.get_mut(id.0)
    }

    pub fn kind(&self, id: BodyId) -> BodyKind {
        match self.body(id).map(|b| b.body_type()) {
            Some(rp::RigidBodyType::Dynamic) => BodyKind::Dynamic,
            Some(rp::RigidBodyType::KinematicVelocityBased | rp::RigidBodyType::KinematicPositionBased) => {
                BodyKind::Kinematic
            }
            _ => BodyKind::Fixed,
        }
    }
    pub fn set_kind(&mut self, id: BodyId, kind: BodyKind) {
        let t = match kind {
            BodyKind::Dynamic => rp::RigidBodyType::Dynamic,
            BodyKind::Fixed => rp::RigidBodyType::Fixed,
            BodyKind::Kinematic => rp::RigidBodyType::KinematicVelocityBased,
        };
        if let Some(b) = self.body_mut(id) {
            b.set_body_type(t, true);
        }
    }
    pub fn tag(&self, id: BodyId) -> u64 {
        self.body(id).map_or(0, |b| b.user_data as u64)
    }
    pub fn set_tag(&mut self, id: BodyId, tag: u64) {
        if let Some(b) = self.body_mut(id) {
            b.user_data = tag as u128;
        }
    }

    /// The simulated position (meters).
    pub fn position(&self, id: BodyId) -> Vec3 {
        self.body(id).map_or(Vec3::ZERO, |b| sv(b.translation()))
    }
    pub fn rotation(&self, id: BodyId) -> Quat {
        self.body(id).map_or(Quat::IDENTITY, |b| sq(*b.rotation()))
    }
    /// The simulated pose. Draw with [`World::sync`] / [`World::draw_transform`].
    pub fn transform(&self, id: BodyId) -> Transform {
        self.body(id).map_or(Transform::default(), |b| transform_from(b.position()))
    }
    pub fn velocity(&self, id: BodyId) -> Vec3 {
        self.body(id).map_or(Vec3::ZERO, |b| sv(b.linvel()))
    }
    /// Angular velocity: axis × radians/s.
    pub fn spin(&self, id: BodyId) -> Vec3 {
        self.body(id).map_or(Vec3::ZERO, |b| sv(b.angvel()))
    }
    pub fn mass(&self, id: BodyId) -> f32 {
        self.body(id).map_or(0.0, |b| b.mass())
    }
    pub fn velocity_at(&self, id: BodyId, point: Vec3) -> Vec3 {
        self.body(id).map_or(Vec3::ZERO, |b| sv(b.velocity_at_point(rv(point))))
    }

    /// Teleports a body (no interpolation smear).
    pub fn set_position(&mut self, id: BodyId, pos: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.set_translation(rv(pos), true);
        }
        self.forget_prev(id);
        self.refresh(id);
    }
    pub fn set_rotation(&mut self, id: BodyId, rot: Quat) {
        if let Some(b) = self.body_mut(id) {
            b.set_rotation(rq(rot), true);
        }
        self.forget_prev(id);
        self.refresh(id);
    }
    pub fn set_velocity(&mut self, id: BodyId, v: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.set_linvel(rv(v), true);
        }
    }
    pub fn set_spin(&mut self, id: BodyId, w: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.set_angvel(rv(w), true);
        }
    }
    pub fn set_gravity_scale(&mut self, id: BodyId, s: f32) {
        if let Some(b) = self.body_mut(id) {
            b.set_gravity_scale(s, true);
        }
    }
    pub fn set_damping(&mut self, id: BodyId, linear: f32, angular: f32) {
        if let Some(b) = self.body_mut(id) {
            b.set_linear_damping(linear);
            b.set_angular_damping(angular);
        }
    }
    /// Stops (or allows) the body turning: a crate held level on a rope,
    /// a character that mustn't fall over.
    pub fn set_fixed_rotation(&mut self, id: BodyId, fixed: bool) {
        if let Some(b) = self.body_mut(id) {
            b.lock_rotations(fixed, true);
        }
    }

    fn forget_prev(&mut self, id: BodyId) {
        let i = id.0.into_raw_parts().0 as usize;
        if let Some(p) = self.prev.get_mut(i) {
            *p = None;
        }
    }

    /// Moves a kinematic body to `pos`/`rot` over the next step(s), pushing
    /// what's in the way. Call it every frame with where it should be.
    pub fn move_kinematic(&mut self, id: BodyId, pos: Vec3, rot: Quat) {
        self.targets.retain(|t| t.0 != id.0);
        self.targets.push((id.0, pos, rot));
    }

    /// An instant kick, N·s (changes velocity by impulse / mass).
    pub fn apply_impulse(&mut self, id: BodyId, impulse: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.apply_impulse(rv(impulse), true);
        }
    }
    pub fn apply_impulse_at(&mut self, id: BodyId, impulse: Vec3, point: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.apply_impulse_at_point(rv(impulse), rv(point), true);
        }
    }
    pub fn apply_angular_impulse(&mut self, id: BodyId, impulse: Vec3) {
        if let Some(b) = self.body_mut(id) {
            b.apply_torque_impulse(rv(impulse), true);
        }
    }
    /// Changes velocity by `dv` m/s regardless of mass.
    pub fn add_velocity(&mut self, id: BodyId, dv: Vec3) {
        if let Some(b) = self.body_mut(id) {
            let v = b.linvel() + rv(dv);
            b.set_linvel(v, true);
        }
    }
    /// Pushes with force `f` (N) during this frame; call it every frame you
    /// want to push. Frame-rate independent (applied as `f × dt` of impulse).
    pub fn apply_force(&mut self, id: BodyId, f: Vec3) {
        self.add_force(id, rv(f), RVec::ZERO);
    }
    pub fn apply_force_at(&mut self, id: BodyId, f: Vec3, point: Vec3) {
        let Some(b) = self.body(id) else { return };
        let r = rv(point) - b.center_of_mass();
        let fm = rv(f);
        self.add_force(id, fm, r.cross(fm));
    }
    /// A torque (N·m, axis × magnitude) during this frame.
    pub fn apply_torque(&mut self, id: BodyId, torque: Vec3) {
        self.add_force(id, RVec::ZERO, rv(torque));
    }
    fn add_force(&mut self, id: BodyId, f: RVec, t: RVec) {
        if !self.contains(id) {
            return;
        }
        match self.forces.iter_mut().find(|e| e.0 == id.0) {
            Some(e) => {
                e.1 += f;
                e.2 += t;
            }
            None => self.forces.push((id.0, f, t)),
        }
    }

    pub fn is_sleeping(&self, id: BodyId) -> bool {
        self.body(id).is_some_and(|b| b.is_sleeping())
    }
    pub fn wake(&mut self, id: BodyId) {
        self.w.wake_up(id.0, true);
    }
    pub fn sleep(&mut self, id: BodyId) {
        if let Some(b) = self.body_mut(id) {
            b.sleep();
        }
    }
    /// Dynamic bodies still awake (0 = everything has settled).
    pub fn awake_count(&self) -> usize {
        self.w.bodies.iter().filter(|(_, b)| b.is_dynamic() && !b.is_sleeping()).count()
    }
    pub fn set_friction(&mut self, shape: ShapeId, f: f32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_friction(f.max(0.0));
        }
    }
    pub fn set_restitution(&mut self, shape: ShapeId, r: f32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_restitution(r.max(0.0));
        }
    }
    pub fn set_sensor(&mut self, shape: ShapeId, sensor: bool) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_sensor(sensor);
        }
    }


    /// Moves a shape to collision layers by Godot's rule (see [`Shape::layers`]).
    pub fn set_layers(&mut self, shape: ShapeId, layer: u32, mask: u32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_collision_groups(rp::InteractionGroups::new(
                rp::Group::from_bits_retain(layer),
                rp::Group::from_bits_retain(mask),
                rp::InteractionTestMode::Or,
            ));
        }
    }
    /// A shape's mass in kg (its body's mass is its shapes' together).
    pub fn set_shape_mass(&mut self, shape: ShapeId, kg: f32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_mass(kg.max(1e-4));
        }
    }
    /// Continuous collision detection on or off (small, fast things): the
    /// sweep, and contacts predicted along the step's motion (Rapier's soft
    /// CCD). The sweep alone lets a body a step into what it meets, and a
    /// ball 0.06 m into a box's front near its corner is nearer the side
    /// face, so the contact pushed it out sideways and on through.
    /// [`Body::bullet`] is the sweep alone, as kit games have it.
    pub fn set_bullet(&mut self, id: BodyId, on: bool) {
        if let Some(b) = self.body_mut(id) {
            b.enable_ccd(on);
            b.set_soft_ccd_prediction(if on { SOFT_CCD } else { 0.0 });
        }
    }
    /// Whether the body may fall asleep when it settles.
    pub fn set_can_sleep(&mut self, id: BodyId, on: bool) {
        if let Some(b) = self.body_mut(id) {
            let a = b.activation_mut();
            a.normalized_linear_threshold = if on { rp::RigidBodyActivation::default_normalized_linear_threshold() } else { -1.0 };
            a.angular_threshold = if on { rp::RigidBodyActivation::default_angular_threshold() } else { -1.0 };
            if !on {
                b.wake_up(true);
            }
        }
    }

    // ------------------------------------------------------------ joints

    pub fn join(&mut self, j: Joint) -> JointId {
        let pa = self.body(j.a).map_or(RPose::IDENTITY, |b| *b.position());
        let pb = self.body(j.b).map_or(RPose::IDENTITY, |b| *b.position());
        let local_a = |p: Vec3| pa.inverse_transform_point(rv(p));
        let local_b = |p: Vec3| pb.inverse_transform_point(rv(p));
        // Frames whose x axis is `axis` in the world, identical for both
        // bodies now, so angles and offsets are measured from here.
        let frames = |anchor: Vec3, axis: Vec3, g: &mut rp::GenericJoint| {
            let axis = rv(axis).try_normalize().unwrap_or(RVec::X);
            let r = RRot::from_rotation_arc(RVec::X, axis);
            g.local_frame1 = RPose::from_parts(local_a(anchor), pa.rotation.inverse() * r);
            g.local_frame2 = RPose::from_parts(local_b(anchor), pb.rotation.inverse() * r);
        };
        let mut data: rp::GenericJoint = match j.kind {
            JointKind::Revolute(anchor, axis) => {
                let mut r = rp::RevoluteJointBuilder::new(RVec::X);
                if let Some((lo, hi)) = j.limits {
                    r = r.limits([lo, hi]);
                }
                if let Some((speed, max)) = j.motor {
                    r = r.motor_velocity(speed, MOTOR_GAIN).motor_max_force(max);
                }
                let mut g: rp::GenericJoint = r.build().into();
                frames(anchor, axis, &mut g);
                g
            }
            JointKind::Prismatic(anchor, axis) => {
                let mut p = rp::PrismaticJointBuilder::new(RVec::X);
                if let Some((lo, hi)) = j.limits {
                    p = p.limits([lo, hi]);
                }
                if let Some((speed, max)) = j.motor {
                    p = p.motor_velocity(speed, MOTOR_GAIN).motor_max_force(max);
                }
                let mut g: rp::GenericJoint = p.build().into();
                frames(anchor, axis, &mut g);
                g
            }
            JointKind::Spherical(anchor) => {
                rp::SphericalJointBuilder::new().local_anchor1(local_a(anchor)).local_anchor2(local_b(anchor)).build().into()
            }
            JointKind::Fixed => {
                rp::FixedJointBuilder::new().local_frame1(pa.inv_mul(&pb)).local_frame2(RPose::IDENTITY).build().into()
            }
            JointKind::Rope(a, b) => {
                let len = j.length.unwrap_or_else(|| a.distance(b));
                rp::RopeJointBuilder::new(len).local_anchor1(local_a(a)).local_anchor2(local_b(b)).build().into()
            }
            JointKind::Spring(a, b) => {
                let rest = j.length.unwrap_or_else(|| a.distance(b));
                let w = std::f32::consts::TAU * j.spring.0;
                rp::SpringJointBuilder::new(rest, w * w, 2.0 * j.spring.1 * w)
                    .spring_model(rp::MotorModel::AccelerationBased)
                    .local_anchor1(local_a(a))
                    .local_anchor2(local_b(b))
                    .build()
                    .into()
            }
        };
        data.set_contacts_enabled(j.collide);
        JointId(self.w.impulse_joints.insert(j.a.0, j.b.0, data, true))
    }

    pub fn unjoin(&mut self, id: JointId) {
        self.w.remove_impulse_joint(id.0);
    }
    pub fn has_joint(&self, id: JointId) -> bool {
        self.w.impulse_joints.get(id.0).is_some()
    }
    /// How hard the joint pulled in its last step, N (for breakable joints).
    pub fn joint_force(&self, id: JointId) -> f32 {
        self.w.impulse_joints.get(id.0).map_or(0.0, |j| {
            let i = j.impulses;
            (i[0] * i[0] + i[1] * i[1] + i[2] * i[2]).sqrt() / self.clock.h()
        })
    }
    /// How hard the joint twisted in its last step, N·m.
    pub fn joint_torque(&self, id: JointId) -> f32 {
        self.w.impulse_joints.get(id.0).map_or(0.0, |j| {
            let i = j.impulses;
            (i[3] * i[3] + i[4] * i[4] + i[5] * i[5]).sqrt() / self.clock.h()
        })
    }
    /// Changes a revolute/prismatic joint's motor.
    pub fn set_motor(&mut self, id: JointId, speed: f32, max_force: f32) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        let axis = if j.data.locked_axes.contains(rp::JointAxesMask::LIN_X) {
            rp::JointAxis::AngX
        } else {
            rp::JointAxis::LinX
        };
        j.data.set_motor_velocity(axis, speed, MOTOR_GAIN);
        j.data.set_motor_max_force(axis, max_force);
    }
    pub fn joint_bodies(&self, id: JointId) -> Option<(BodyId, BodyId)> {
        self.w.impulse_joints.get(id.0).map(|j| (BodyId(j.body1()), BodyId(j.body2())))
    }
    pub fn joints(&self) -> impl Iterator<Item = JointId> + '_ {
        self.w.impulse_joints.iter().map(|(h, _)| JointId(h))
    }

    // ------------------------------------------------------------ queries

    /// The first solid shape along `from` → `to`.
    pub fn raycast(&self, from: Vec3, to: Vec3) -> Option<RayHit> {
        self.raycast_filtered(from, to, None)
    }
    pub fn raycast_ignoring(&self, from: Vec3, to: Vec3, ignore: BodyId) -> Option<RayHit> {
        self.raycast_filtered(from, to, Some(ignore))
    }
    fn raycast_filtered(&self, from: Vec3, to: Vec3, ignore: Option<BodyId>) -> Option<RayHit> {
        let d = to - from;
        let len = d.length();
        if len <= 0.0 {
            return None;
        }
        let mut filter = rp::QueryFilter::new().exclude_sensors();
        if let Some(i) = ignore {
            filter = filter.exclude_rigid_body(i.0);
        }
        let dir = d / len;
        let ray = rp::Ray::new(rv(from), rv(dir));
        let (c, hit) = self.w.query_pipeline_with_filter(filter).cast_ray_and_get_normal(&ray, len, true)?;
        let body = self.w.colliders.get(c)?.parent()?;
        Some(RayHit {
            body: BodyId(body),
            shape: ShapeId(c),
            point: from + dir * hit.time_of_impact,
            normal: sv(hit.normal).normalize(),
            distance: hit.time_of_impact,
        })
    }

    /// The first shape along `from` → `to` whose layer is in `mask`, past
    /// the bodies in `ignore`; sensors too when `sensors` (Godot's
    /// `intersect_ray` with `collide_with_areas`).
    pub fn raycast_masked(&self, from: Vec3, to: Vec3, mask: u32, ignore: &[BodyId], sensors: bool) -> Option<RayHit> {
        let d = to - from;
        let len = d.length();
        if !(len > 0.0) {
            return None;
        }
        let pred = |_h: rp::ColliderHandle, c: &rp::Collider| {
            c.collision_groups().memberships.bits() & mask != 0 && (sensors || !c.is_sensor()) && !c.parent().is_some_and(|p| ignore.iter().any(|i| i.0 == p))
        };
        let filter = rp::QueryFilter::new().predicate(&pred);
        let dir = d / len;
        let ray = rp::Ray::new(rv(from), rv(dir));
        let (c, hit) = self.w.query_pipeline_with_filter(filter).cast_ray_and_get_normal(&ray, len, true)?;
        let body = self.w.colliders.get(c)?.parent()?;
        Some(RayHit { body: BodyId(body), shape: ShapeId(c), point: from + dir * hit.time_of_impact, normal: sv(hit.normal).normalize(), distance: hit.time_of_impact })
    }
    /// The bodies whose shapes contain `point`, sorted.
    pub fn bodies_at(&self, point: Vec3) -> Vec<BodyId> {
        let mut out: Vec<BodyId> =
            self.w.query_pipeline().intersect_point(rv(point)).filter_map(|(_, c)| c.parent().map(BodyId)).collect();
        out.sort();
        out.dedup();
        out
    }
    /// Whether two bodies are touching now.
    pub fn touching(&self, a: BodyId, b: BodyId) -> bool {
        self.contacts(a).contains(&b)
    }
    /// The bodies touching `id` now (solid contacts), sorted.
    pub fn contacts(&self, id: BodyId) -> Vec<BodyId> {
        let mut out = Vec::new();
        for c in self.shapes(id) {
            for p in self.w.narrow_phase.contact_pairs_with(c.0) {
                if !p.has_any_active_contact() {
                    continue;
                }
                let other = if p.collider1 == c.0 { p.collider2 } else { p.collider1 };
                if let Some(b) = self.body_of(ShapeId(other)) {
                    out.push(b);
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
    /// The bodies overlapping a sensor body's shapes now, sorted.
    pub fn overlapping(&self, sensor: BodyId) -> Vec<BodyId> {
        let mut out = Vec::new();
        for c in self.shapes(sensor) {
            for (c1, c2, hit) in self.w.narrow_phase.intersection_pairs_with(c.0) {
                if !hit {
                    continue;
                }
                let other = if c1 == c.0 { c2 } else { c1 };
                if let Some(b) = self.body_of(ShapeId(other)) {
                    out.push(b);
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    // ------------------------------------------------------------ drawing

    /// Where to draw a body this frame (interpolated between the last two steps).
    pub fn draw_transform(&self, id: BodyId) -> Transform {
        let Some(b) = self.body(id) else { return Transform::default() };
        let cur = *b.position();
        let i = id.0.into_raw_parts().0 as usize;
        match self.prev.get(i).copied().flatten() {
            Some((h, prev)) if self.interpolate && h == id.0 && !b.is_sleeping() => {
                transform_from(&prev.lerp(&cur, self.clock.alpha()))
            }
            _ => transform_from(&cur),
        }
    }
    /// Moves `node` to the body's draw transform. Call it in `render` for
    /// every body you show (parent the node to nothing, or to a node at the
    /// origin: the transform is in world space).
    pub fn sync(&self, id: BodyId, node: Node) {
        node.set_transform(&self.draw_transform(id));
    }
    /// The same with a scale (a node whose mesh is a unit cube scaled to size).
    pub fn sync_scaled(&self, id: BodyId, node: Node, scale: Vec3) {
        let mut t = self.draw_transform(id);
        t.scale = scale;
        node.set_transform(&t);
    }


    /// Every shape within `radius` of `center` as line segments in the
    /// world (at their draw transforms), for a debug view: each with its
    /// colour (`0xRRGGBBAA`: dynamic orange, sleeping grey, fixed
    /// blue-grey, kinematic green, sensors faint cyan). Heightfields draw
    /// their grid near `center`. At most `max` segments.
    pub fn debug_lines(&self, center: Vec3, radius: f32, max: usize, out: &mut Vec<(Vec3, Vec3, u32)>) {
        for (_, c) in self.w.colliders.iter() {
            if out.len() >= max {
                return;
            }
            let Some(bh) = c.parent() else { continue };
            let Some(b) = self.w.bodies.get(bh) else { continue };
            let aabb = c.compute_aabb();
            let (lo, hi) = (sv(aabb.mins), sv(aabb.maxs));
            let near = vec3(center.x.clamp(lo.x, hi.x), center.y.clamp(lo.y, hi.y), center.z.clamp(lo.z, hi.z));
            if near.distance(center) > radius {
                continue;
            }
            let color = if c.is_sensor() {
                0x5ee6ff80
            } else if b.is_fixed() {
                0x8fa3b8ff
            } else if b.is_kinematic() {
                0x57e389ff
            } else if b.is_sleeping() {
                0x9a9a9aff
            } else {
                0xff9f43ff
            };
            // The body's draw transform, then the shape within it.
            let body_t = self.draw_transform(BodyId(bh));
            let local = c.position_wrt_parent().copied().unwrap_or(RPose::IDENTITY);
            let m = body_t.matrix() * transform_from(&local).matrix();
            let mut seg = |a: Vec3, b: Vec3| {
                if out.len() < max {
                    out.push((m.transform_point(a), m.transform_point(b), color));
                }
            };
            match c.shape().as_typed_shape() {
                TypedShape::Cuboid(k) => box_lines(sv(k.half_extents), &mut seg),
                TypedShape::RoundCuboid(k) => box_lines(sv(k.inner_shape.half_extents) + Vec3::splat(k.border_radius), &mut seg),
                TypedShape::Ball(k) => ring_lines(k.radius, 0.0, &mut seg, true),
                TypedShape::Capsule(k) => {
                    let (r, hh) = (k.radius, k.half_height());
                    ring_lines(r, hh, &mut seg, false);
                    ring_lines(r, -hh, &mut seg, false);
                    for (x, z) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
                        seg(vec3(x, -hh, z), vec3(x, hh, z));
                    }
                }
                TypedShape::Cylinder(k) => {
                    let (r, hh) = (k.radius, k.half_height);
                    ring_lines(r, hh, &mut seg, false);
                    ring_lines(r, -hh, &mut seg, false);
                    for (x, z) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
                        seg(vec3(x, -hh, z), vec3(x, hh, z));
                    }
                }
                TypedShape::Cone(k) => {
                    let (r, hh) = (k.radius, k.half_height);
                    ring_lines(r, -hh, &mut seg, false);
                    for (x, z) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
                        seg(vec3(x, -hh, z), vec3(0.0, hh, 0.0));
                    }
                }
                TypedShape::ConvexPolyhedron(k) => {
                    let v = k.points();
                    for e in k.edges() {
                        seg(sv(v[e.vertices[0] as usize]), sv(v[e.vertices[1] as usize]));
                    }
                }
                TypedShape::TriMesh(k) => {
                    for t in k.triangles().take(4000) {
                        let (a, b2, c2) = (sv(t.a), sv(t.b), sv(t.c));
                        seg(a, b2);
                        seg(b2, c2);
                    }
                }
                TypedShape::HeightField(k) => {
                    // Only the cells near `center` (in the shape's frame).
                    let lc = m.inverse().transform_point(center);
                    let r2 = radius * radius * 0.25;
                    for t in k.triangles() {
                        let (a, b2, c2) = (sv(t.a), sv(t.b), sv(t.c));
                        let (dx, dz) = (a.x - lc.x, a.z - lc.z);
                        if dx * dx + dz * dz > r2 {
                            continue;
                        }
                        seg(a, b2);
                        seg(b2, c2);
                    }
                }
                _ => {}
            }
        }
    }

    /// Equal hashes mean bit-identical worlds.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut eat = |x: f32| {
            for byte in x.to_bits().to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        };
        for (_, b) in self.w.bodies.iter() {
            let p = b.position();
            for x in [p.translation.x, p.translation.y, p.translation.z, p.rotation.x, p.rotation.y, p.rotation.z, p.rotation.w]
            {
                eat(x);
            }
            let (v, w) = (b.linvel(), b.angvel());
            for x in [v.x, v.y, v.z, w.x, w.y, w.z] {
                eat(x);
            }
        }
        h
    }

    /// The shape kinds on a body (for building matching meshes, debugging).
    pub fn shape_kind(&self, shape: ShapeId) -> &'static str {
        match self.w.colliders.get(shape.0).map(|c| c.shape().as_typed_shape()) {
            Some(TypedShape::Cuboid(_)) => "cuboid",
            Some(TypedShape::RoundCuboid(_)) => "round_cuboid",
            Some(TypedShape::Ball(_)) => "sphere",
            Some(TypedShape::Capsule(_)) => "capsule",
            Some(TypedShape::Cylinder(_)) => "cylinder",
            Some(TypedShape::Cone(_)) => "cone",
            Some(TypedShape::ConvexPolyhedron(_)) => "convex",
            Some(TypedShape::TriMesh(_)) => "trimesh",
            Some(TypedShape::HeightField(_)) => "heightfield",
            Some(_) => "other",
            None => "none",
        }
    }

    /// The Rapier world underneath (meters, +y up).
    pub fn rapier(&mut self) -> &mut rp::PhysicsWorld {
        &mut self.w
    }
}

const MOTOR_GAIN: f32 = 60.0;

// ---------------------------------------------------------------- vehicles

/// A ray-cast vehicle (Godot's `VehicleBody3D`, on Rapier's port of
/// Bullet's `btRaycastVehicle`): a dynamic chassis held up by wheels that
/// are rays with springs. It catches air off a crest when its speed carries
/// it past where the springs reach, and lands on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VehicleId(usize);

/// A wheel: where its suspension starts on the chassis (the chassis's own
/// frame), and its tuning. The suspension points down the chassis's −y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    pub at: Vec3,
    pub radius: f32,
    /// The spring's length at rest, m.
    pub rest: f32,
    /// How far it can go either side of `rest`, m.
    pub travel: f32,
    /// Spring stiffness (Godot's `suspension_stiffness`; scaled by the
    /// chassis's mass, so it holds any car up alike).
    pub stiffness: f32,
    /// Damping as it compresses and as it relaxes.
    pub compression: f32,
    pub relaxation: f32,
    /// Grip: how hard it can push before it slides (Godot's `wheel_friction_slip`).
    pub friction_slip: f32,
    /// Sideways grip (1 is full).
    pub side_friction: f32,
    /// The most force its spring makes, N.
    pub max_force: f32,
}

impl Default for Wheel {
    fn default() -> Wheel {
        Wheel { at: Vec3::ZERO, radius: 0.5, rest: 0.15, travel: 0.2, stiffness: 5.88, compression: 0.83, relaxation: 0.88, friction_slip: 10.5, side_friction: 1.0, max_force: 6000.0 }
    }
}

/// How a wheel is now (world space).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelState {
    /// The wheel's centre.
    pub center: Vec3,
    /// How far it has turned about its axle, radians.
    pub rotation: f32,
    pub steering: f32,
    pub on_ground: bool,
    /// The spring's length now, m.
    pub suspension: f32,
    /// What it stands on.
    pub ground: Option<BodyId>,
}

struct Vehicle {
    ctrl: rapier3d::control::DynamicRayCastVehicleController,
}

impl World {
    /// Makes `chassis` (a dynamic body) a vehicle on these wheels. It drives
    /// toward the chassis's −z (every 2.0 node's front), and its springs
    /// point down its −y.
    pub fn add_vehicle(&mut self, chassis: BodyId, wheels: &[Wheel]) -> VehicleId {
        let mut ctrl = rapier3d::control::DynamicRayCastVehicleController::new(chassis.0);
        ctrl.index_up_axis = 1;
        ctrl.index_forward_axis = 2;
        for w in wheels {
            let tuning = rapier3d::control::WheelTuning {
                suspension_stiffness: w.stiffness,
                suspension_compression: w.compression,
                suspension_damping: w.relaxation,
                max_suspension_travel: w.travel,
                side_friction_stiffness: w.side_friction,
                friction_slip: w.friction_slip,
                max_suspension_force: w.max_force,
            };
            ctrl.add_wheel(rv(w.at), RVec::new(0.0, -1.0, 0.0), RVec::new(-1.0, 0.0, 0.0), w.rest, w.radius, &tuning);
        }
        self.vehicles.push(Some(Vehicle { ctrl }));
        VehicleId(self.vehicles.len() - 1)
    }

    /// Adds a wheel to a vehicle; returns its index.
    pub fn add_wheel(&mut self, id: VehicleId, w: &Wheel) -> usize {
        let Some(Some(v)) = self.vehicles.get_mut(id.0) else { return usize::MAX };
        let tuning = rapier3d::control::WheelTuning {
            suspension_stiffness: w.stiffness,
            suspension_compression: w.compression,
            suspension_damping: w.relaxation,
            max_suspension_travel: w.travel,
            side_friction_stiffness: w.side_friction,
            friction_slip: w.friction_slip,
            max_suspension_force: w.max_force,
        };
        v.ctrl.add_wheel(rv(w.at), RVec::new(0.0, -1.0, 0.0), RVec::new(-1.0, 0.0, 0.0), w.rest, w.radius, &tuning);
        v.ctrl.wheels().len() - 1
    }

    pub fn remove_vehicle(&mut self, id: VehicleId) {
        if let Some(v) = self.vehicles.get_mut(id.0) {
            *v = None;
        }
    }

    /// Drive, brake and steer one wheel: `engine` N forward (toward −z),
    /// `brake` N, `steering` radians (positive turns left).
    pub fn set_wheel(&mut self, id: VehicleId, wheel: usize, engine: f32, brake: f32, steering: f32) {
        let Some(Some(v)) = self.vehicles.get_mut(id.0) else { return };
        let chassis = v.ctrl.chassis;
        if let Some(w) = v.ctrl.wheels_mut().get_mut(wheel) {
            // A parked car asleep wakes when it's driven or steered.
            let wake = engine != 0.0 && w.engine_force != -engine || w.steering != steering;
            w.engine_force = -engine;
            w.brake = brake.max(0.0);
            w.steering = steering;
            if wake {
                self.w.wake_up(chassis, true);
            }
        }
    }

    /// Changes a wheel's tuning (not where it is).
    pub fn tune_wheel(&mut self, id: VehicleId, wheel: usize, t: &Wheel) {
        let Some(Some(v)) = self.vehicles.get_mut(id.0) else { return };
        if let Some(w) = v.ctrl.wheels_mut().get_mut(wheel) {
            w.radius = t.radius;
            w.suspension_rest_length = t.rest;
            w.max_suspension_travel = t.travel;
            w.suspension_stiffness = t.stiffness;
            w.damping_compression = t.compression;
            w.damping_relaxation = t.relaxation;
            w.friction_slip = t.friction_slip;
            w.side_friction_stiffness = t.side_friction;
            w.max_suspension_force = t.max_force;
        }
    }

    /// The wheels as they are after the last step.
    pub fn wheels(&self, id: VehicleId) -> Vec<WheelState> {
        let Some(Some(v)) = self.vehicles.get(id.0) else { return Vec::new() };
        v.ctrl
            .wheels()
            .iter()
            .map(|w| {
                let info = w.raycast_info();
                WheelState {
                    center: sv(w.center()),
                    rotation: w.rotation,
                    steering: w.steering,
                    on_ground: info.is_in_contact,
                    suspension: info.suspension_length,
                    ground: info.ground_object.and_then(|c| self.w.colliders.get(c)).and_then(|c| c.parent()).map(BodyId),
                }
            })
            .collect()
    }

    /// The vehicle's speed along its front, m/s (negative backing up).
    pub fn vehicle_speed(&self, id: VehicleId) -> f32 {
        let Some(Some(v)) = self.vehicles.get(id.0) else { return 0.0 };
        let Some(b) = self.w.bodies.get(v.ctrl.chassis) else { return 0.0 };
        let front = b.position().rotation * RVec::new(0.0, 0.0, -1.0);
        b.linvel().dot(front)
    }

    fn update_vehicles(&mut self) {
        let h = self.clock.h();
        let w = &mut self.w;
        for v in self.vehicles.iter_mut().flatten() {
            let chassis = v.ctrl.chassis;
            if !w.bodies.contains(chassis) {
                continue;
            }
            let filter = rp::QueryFilter::new().exclude_rigid_body(chassis).exclude_sensors();
            let q = w.broad_phase.as_query_pipeline_mut(w.narrow_phase.query_dispatcher(), &mut w.bodies, &mut w.colliders, filter);
            v.ctrl.update_vehicle(h, q);
        }
    }
}

fn box_lines(h: Vec3, seg: &mut impl FnMut(Vec3, Vec3)) {
    let c = |i: u32| vec3(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z });
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        seg(c(a), c(b));
    }
}

/// A ring of radius `r` at height `y` (and, for a ball, two more upright).
fn ring_lines(r: f32, y: f32, seg: &mut impl FnMut(Vec3, Vec3), ball: bool) {
    const N: u32 = 16;
    for i in 0..N {
        let (a0, a1) = (std::f32::consts::TAU * i as f32 / N as f32, std::f32::consts::TAU * (i + 1) as f32 / N as f32);
        let (s0, c0) = libm::sincosf(a0);
        let (s1, c1) = libm::sincosf(a1);
        seg(vec3(c0 * r, y, s0 * r), vec3(c1 * r, y, s1 * r));
        if ball {
            seg(vec3(c0 * r, s0 * r, 0.0), vec3(c1 * r, s1 * r, 0.0));
            seg(vec3(0.0, c0 * r, s0 * r), vec3(0.0, c1 * r, s1 * r));
        }
    }
}

#[cfg(test)]
#[path = "physics3d_tests.rs"]
mod tests;
