//! 2D rigid-body physics (Rapier), in logical pixels. Enable it with
//! `maimbrain = { …, features = ["physics2d"] }`; docs/PHYSICS.md is the guide.
//!
//! Everything is in the same space as `gfx2d` and input: logical units
//! ("pixels"), +y down, angles in radians with positive turning clockwise on
//! screen (what `gfx2d::rotate` does). Internally Rapier works in meters at
//! [`PIXELS_PER_METER`] = 100, so a 40 px crate is a 0.4 m crate and the
//! default gravity is 981 px/s².
//!
//! ```ignore
//! use maimbrain::physics2d::{Body, Event, Shape, World, vec2};
//!
//! let mut world = World::new();
//! world.add(Body::fixed(180.0, 620.0), Shape::rect(360.0, 40.0));
//! let crate_ = world.add(Body::dynamic(180.0, 100.0).angle(0.3), Shape::rect(40.0, 40.0).friction(0.8));
//!
//! // update(dt):
//! world.step(dt); // fixed 60 Hz steps from the accumulated dt: deterministic
//! for e in world.events() {
//!     if let Event::ContactBegin(c) = e { /* c.speed in px/s → sound volume */ }
//! }
//! // render():
//! world.with_transform(crate_, || gfx2d::rect(-20.0, -20.0, 40.0, 40.0, 0xd08c4aff));
//! ```
//!
//! Determinism: Rapier is built with `enhanced-determinism`, every step has
//! the same length, and nothing reads a clock or iterates a hash map, so
//! the same inputs give bit-identical bodies on every device and in replays.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

use rapier2d::math::{Pose as RPose, Rotation as RRot, Vector as RVec};
use rapier2d::parry::shape::TypedShape;
use rapier2d::prelude as rp;

use crate::gfx2d;

#[path = "character2d.rs"]
pub mod character;
pub use character::{Character, CharacterSettings};
use crate::physics_clock::{Clock, Inbox, angle_delta};
pub use crate::physics_clock::{DEFAULT_STEP_RATE, MAX_STEPS_PER_CALL};

/// Logical pixels per Rapier meter. Densities are per square meter
/// (a 100×100 px box of density 1 weighs 1 kg).
pub const PIXELS_PER_METER: f32 = 100.0;
/// Earth gravity in px/s² (9.81 m/s²), the default for [`World::new`], +y down.
pub const GRAVITY: f32 = 981.0;

const PPM: f32 = PIXELS_PER_METER;
const MPP: f32 = 1.0 / PIXELS_PER_METER;

// ---------------------------------------------------------------- Vec2

/// A 2D vector in logical pixels (or px/s, px/s², …).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);
    pub const X: Vec2 = vec2(1.0, 0.0);
    pub const Y: Vec2 = vec2(0.0, 1.0);

    pub const fn new(x: f32, y: f32) -> Vec2 {
        vec2(x, y)
    }
    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    /// The z of the 3D cross product: positive when `o` is clockwise of `self` on screen.
    pub fn cross(self, o: Vec2) -> f32 {
        self.x * o.y - self.y * o.x
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn distance(self, o: Vec2) -> f32 {
        (self - o).length()
    }
    /// Unit length, or zero for a zero vector.
    pub fn normalize(self) -> Vec2 {
        let l = self.length();
        if l > 0.0 { self / l } else { Vec2::ZERO }
    }
    /// Rotated a quarter turn clockwise on screen.
    pub fn perp(self) -> Vec2 {
        vec2(-self.y, self.x)
    }
    /// Rotated by `angle` radians (clockwise on screen, like `gfx2d::rotate`).
    pub fn rotate(self, angle: f32) -> Vec2 {
        let (s, c) = libm::sincosf(angle);
        vec2(c * self.x - s * self.y, s * self.x + c * self.y)
    }
    pub fn lerp(self, o: Vec2, t: f32) -> Vec2 {
        self + (o - self) * t
    }
    /// The angle of this direction (atan2), as `gfx2d::rotate` measures it.
    pub fn angle(self) -> f32 {
        libm::atan2f(self.y, self.x)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        vec2(self.x * s, self.y * s)
    }
}
impl Mul<Vec2> for f32 {
    type Output = Vec2;
    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}
impl Div<f32> for Vec2 {
    type Output = Vec2;
    fn div(self, s: f32) -> Vec2 {
        vec2(self.x / s, self.y / s)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        *self = *self + o;
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        *self = *self - o;
    }
}
impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}
impl From<(f32, f32)> for Vec2 {
    fn from((x, y): (f32, f32)) -> Vec2 {
        vec2(x, y)
    }
}

fn to_m(v: Vec2) -> RVec {
    RVec::new(v.x * MPP, v.y * MPP)
}
fn to_px(v: RVec) -> Vec2 {
    vec2(v.x * PPM, v.y * PPM)
}

/// Where a body is and how it's turned: what to draw it with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: Vec2,
    /// Radians, clockwise on screen.
    pub angle: f32,
}

impl Pose {
    /// A point given in the body's own frame, in world (screen) space.
    pub fn apply(&self, local: Vec2) -> Vec2 {
        self.pos + local.rotate(self.angle)
    }
}

fn pose_from(p: &RPose) -> Pose {
    Pose { pos: to_px(p.translation), angle: p.rotation.angle() }
}

// ---------------------------------------------------------------- handles

/// A body in a [`World`]. Stale after [`World::remove`]; calls with a stale
/// id do nothing and queries return defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(rp::RigidBodyHandle);

/// One collision shape attached to a body (a body may have several).
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
    /// Moved only by you ([`World::set_velocity`], [`World::move_kinematic`]);
    /// pushes dynamic bodies but nothing pushes it (paddles, platforms, pistons).
    Kinematic,
}

/// How to create a body: `Body::dynamic(x, y).angle(0.2).bullet()`.
#[derive(Clone, Debug)]
pub struct Body {
    kind: BodyKind,
    pos: Vec2,
    angle: f32,
    vel: Vec2,
    spin: f32,
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
    fn new(kind: BodyKind, x: f32, y: f32) -> Body {
        Body {
            kind,
            pos: vec2(x, y),
            angle: 0.0,
            vel: Vec2::ZERO,
            spin: 0.0,
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
    /// A body moved by gravity, forces and collisions, centered at (x, y).
    pub fn dynamic(x: f32, y: f32) -> Body {
        Body::new(BodyKind::Dynamic, x, y)
    }
    /// A body that never moves.
    pub fn fixed(x: f32, y: f32) -> Body {
        Body::new(BodyKind::Fixed, x, y)
    }
    /// A body only you move (velocity or [`World::move_kinematic`]).
    pub fn kinematic(x: f32, y: f32) -> Body {
        Body::new(BodyKind::Kinematic, x, y)
    }
    /// Initial rotation, radians clockwise.
    pub fn angle(mut self, radians: f32) -> Body {
        self.angle = radians;
        self
    }
    /// Initial velocity, px/s.
    pub fn velocity(mut self, vx: f32, vy: f32) -> Body {
        self.vel = vec2(vx, vy);
        self
    }
    /// Initial angular velocity, radians/s clockwise.
    pub fn spin(mut self, radians_per_s: f32) -> Body {
        self.spin = radians_per_s;
        self
    }
    /// 0 floats, 1 normal, 2 heavy, −1 falls up.
    pub fn gravity_scale(mut self, s: f32) -> Body {
        self.gravity_scale = s;
        self
    }
    /// Air drag: velocity loses about this fraction per second (0 = none, 1–5 = floaty/underwater).
    pub fn damping(mut self, linear: f32, angular: f32) -> Body {
        self.linear_damping = linear;
        self.angular_damping = angular;
        self
    }
    /// Continuous collision detection: for small fast things (bullets,
    /// launched balls) that would otherwise pass through thin walls in one step.
    pub fn bullet(mut self) -> Body {
        self.bullet = true;
        self
    }
    /// Never fall asleep (bodies at rest normally sleep to save CPU).
    pub fn always_awake(mut self) -> Body {
        self.can_sleep = false;
        self
    }
    /// Start asleep: a lone object that waits for a hit. Not for stacks:
    /// sleeping bodies that were never simulated touching aren't linked, so
    /// knocking the bottom one out leaves the rest floating. Build stacks
    /// awake (slightly apart) and let them settle and fall asleep.
    pub fn asleep(mut self) -> Body {
        self.asleep = true;
        self
    }
    /// Never rotates (characters, pucks that should only slide).
    pub fn fixed_rotation(mut self) -> Body {
        self.fixed_rotation = true;
        self
    }
    /// Your own number for this body (a kind, an index into your arrays);
    /// read it back with [`World::tag`].
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
            .translation(to_m(self.pos))
            .rotation(self.angle)
            .linvel(to_m(self.vel))
            .angvel(self.spin)
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
    Rect(f32, f32),
    RoundRect(f32, f32, f32),
    Circle(f32),
    Capsule(Vec2, Vec2, f32),
    Convex(Vec<Vec2>),
    Segment(Vec2, Vec2),
    Polyline(Vec<Vec2>),
    Trimesh(Vec<Vec2>, Vec<[u32; 3]>),
}

/// A collision shape and its material: `Shape::rect(40.0, 40.0).friction(0.8)`.
/// Sizes are in pixels and centered on the body unless moved with [`Shape::at`].
#[derive(Clone, Debug)]
pub struct Shape {
    geom: Geom,
    offset: Vec2,
    angle: f32,
    density: f32,
    mass: Option<f32>,
    friction: f32,
    restitution: f32,
    sensor: bool,
    groups: (u32, u32),
    events: bool,
    bounce_max: bool,
    one_way: bool,
}

impl Shape {
    fn new(geom: Geom) -> Shape {
        Shape {
            geom,
            offset: Vec2::ZERO,
            angle: 0.0,
            density: 1.0,
            mass: None,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            groups: (1, u32::MAX),
            events: true,
            bounce_max: false,
            one_way: false,
        }
    }
    /// A `w`×`h` box.
    pub fn rect(w: f32, h: f32) -> Shape {
        Shape::new(Geom::Rect(w.abs() / 2.0, h.abs() / 2.0))
    }
    /// A box with rounded corners (rolls a little at the edges; stacks well).
    pub fn round_rect(w: f32, h: f32, radius: f32) -> Shape {
        let r = radius.abs().min(w.abs() / 2.0).min(h.abs() / 2.0);
        Shape::new(Geom::RoundRect(w.abs() / 2.0 - r, h.abs() / 2.0 - r, r))
    }
    pub fn circle(radius: f32) -> Shape {
        Shape::new(Geom::Circle(radius.abs()))
    }
    /// An upright pill `height` tall in all (caps included).
    pub fn capsule(radius: f32, height: f32) -> Shape {
        let half = (height.abs() / 2.0 - radius.abs()).max(0.0);
        Shape::new(Geom::Capsule(vec2(0.0, -half), vec2(0.0, half), radius.abs()))
    }
    /// A pill around the segment `a`–`b` (in the body's frame).
    pub fn capsule_between(a: Vec2, b: Vec2, radius: f32) -> Shape {
        Shape::new(Geom::Capsule(a, b, radius.abs()))
    }
    /// The convex hull of `points` (in the body's frame, around its center).
    /// For concave outlines, add several convex shapes to one body.
    pub fn convex(points: &[Vec2]) -> Shape {
        Shape::new(Geom::Convex(points.to_vec()))
    }
    /// A regular polygon with `sides` corners on a circle of `radius`.
    pub fn regular_polygon(sides: u32, radius: f32) -> Shape {
        let n = sides.clamp(3, 64);
        let pts: Vec<Vec2> =
            (0..n).map(|i| vec2(0.0, -radius).rotate(i as f32 * std::f32::consts::TAU / n as f32)).collect();
        Shape::convex(&pts)
    }
    /// A thin line from `a` to `b`. Best on fixed bodies.
    pub fn segment(a: Vec2, b: Vec2) -> Shape {
        Shape::new(Geom::Segment(a, b))
    }
    /// Connected segments through `points`: terrain, slopes, a funnel. Hollow
    /// (things collide with the line itself), so use it on fixed bodies.
    pub fn polyline(points: &[Vec2]) -> Shape {
        Shape::new(Geom::Polyline(points.to_vec()))
    }
    /// Triangles over `vertices`. Hollow like a polyline: for fixed scenery.
    pub fn trimesh(vertices: &[Vec2], triangles: &[[u32; 3]]) -> Shape {
        Shape::new(Geom::Trimesh(vertices.to_vec(), triangles.to_vec()))
    }

    /// Moves the shape within its body (compound bodies: an L of two rects).
    pub fn at(mut self, x: f32, y: f32) -> Shape {
        self.offset = vec2(x, y);
        self
    }
    /// Turns the shape within its body.
    pub fn rotated(mut self, radians: f32) -> Shape {
        self.angle = radians;
        self
    }
    /// kg per square meter (100×100 px); default 1. Mass follows from area.
    pub fn density(mut self, d: f32) -> Shape {
        self.density = d.max(0.0);
        self.mass = None;
        self
    }
    /// Sets this shape's mass in kg directly instead of from its density.
    pub fn mass(mut self, kg: f32) -> Shape {
        self.mass = Some(kg.max(0.0));
        self
    }
    /// 0 ice … 0.5 default … 1+ rubber. The two touching shapes' values are averaged.
    pub fn friction(mut self, f: f32) -> Shape {
        self.friction = f.max(0.0);
        self
    }
    /// Bounciness: 0 thud (default) … 1 bounces back as high. The larger of the two is used.
    pub fn restitution(mut self, r: f32) -> Shape {
        self.restitution = r.max(0.0);
        self
    }
    /// Detects overlaps ([`Event::SensorEnter`]/[`Event::SensorExit`]) without
    /// pushing anything: goals, pickups, kill zones.
    pub fn sensor(mut self) -> Shape {
        self.sensor = true;
        self
    }
    /// Collision layers (bit masks): this shape is in the layers set in
    /// `member` and collides only with shapes in a layer of `collides_with`;
    /// both shapes must accept each other. Default: layer 1 (`0b1`),
    /// colliding with every layer. E.g. ghosts that pass through each other
    /// but land on the ground: `.groups(0b10, 0b01)`.
    pub fn groups(mut self, member: u32, collides_with: u32) -> Shape {
        self.groups = (member, collides_with);
        self
    }
    /// Box2D's rules, as near as Rapier has them: two shapes bounce by the
    /// larger of their restitutions, and slide with the smaller of their
    /// frictions (Box2D's √(a·b) has no Rapier rule; the smaller keeps a
    /// frictionless shape frictionless, as Box2D does). Rapier averages
    /// both unless asked.
    pub fn box2d_mixing(mut self) -> Shape {
        self.bounce_max = true;
        self
    }
    /// A one-way platform for characters ([`character::Character`]): they
    /// land on its top and pass up through it from below and the sides.
    /// Rigid bodies meet it from every side.
    pub fn one_way(mut self) -> Shape {
        self.one_way = true;
        self
    }
    /// No contact events for this shape (saves a little work in big piles).
    pub fn quiet(mut self) -> Shape {
        self.events = false;
        self
    }

    fn build(&self) -> rp::ColliderBuilder {
        let fallback = || rp::ColliderBuilder::ball(1.0 * MPP);
        let b = match &self.geom {
            Geom::Rect(hx, hy) => rp::ColliderBuilder::cuboid(hx * MPP, hy * MPP),
            Geom::RoundRect(hx, hy, r) => rp::ColliderBuilder::round_cuboid(hx * MPP, hy * MPP, r * MPP),
            Geom::Circle(r) => rp::ColliderBuilder::ball(r * MPP),
            Geom::Capsule(a, b, r) => rp::ColliderBuilder::capsule_from_endpoints(to_m(*a), to_m(*b), r * MPP),
            Geom::Convex(pts) => {
                let pts: Vec<RVec> = pts.iter().map(|p| to_m(*p)).collect();
                rp::ColliderBuilder::convex_hull(&pts).unwrap_or_else(|| {
                    warn("Shape::convex needs 3+ points that aren't all on a line; using a dot");
                    fallback()
                })
            }
            Geom::Segment(a, b) => rp::ColliderBuilder::segment(to_m(*a), to_m(*b)),
            Geom::Polyline(pts) if pts.len() >= 2 => {
                rp::ColliderBuilder::polyline(pts.iter().map(|p| to_m(*p)).collect(), None)
            }
            Geom::Polyline(_) => {
                warn("Shape::polyline needs 2+ points; using a dot");
                fallback()
            }
            Geom::Trimesh(v, t) => {
                let n = v.len() as u32;
                if t.is_empty() || t.iter().flatten().any(|&i| i >= n) {
                    warn("Shape::trimesh: no triangles, or an index out of range; using a dot");
                    fallback()
                } else {
                    rp::ColliderBuilder::trimesh(v.iter().map(|p| to_m(*p)).collect(), t.clone()).unwrap_or_else(|_| {
                        warn("Shape::trimesh: degenerate mesh; using a dot");
                        fallback()
                    })
                }
            }
        };
        let groups = rp::InteractionGroups::new(
            rp::Group::from_bits_retain(self.groups.0),
            rp::Group::from_bits_retain(self.groups.1),
            rp::InteractionTestMode::And,
        );
        let b = if self.bounce_max {
            b.restitution_combine_rule(rp::CoefficientCombineRule::Max).friction_combine_rule(rp::CoefficientCombineRule::Min)
        } else {
            b
        };
        let b = b
            .position(RPose::new(to_m(self.offset), self.angle))
            .friction(self.friction)
            .restitution(self.restitution)
            .sensor(self.sensor)
            .collision_groups(groups)
            .user_data(if self.one_way { ONE_WAY } else { 0 })
            .active_events(if self.events { rp::ActiveEvents::COLLISION_EVENTS } else { rp::ActiveEvents::empty() });
        match self.mass {
            Some(m) => b.mass(m),
            None => b.density(self.density),
        }
    }
}

/// A collider's user data: a one-way platform ([`Shape::one_way`]).
const ONE_WAY: u128 = 1;

fn warn(msg: &str) {
    crate::sys::log(crate::sys::Level::Warn, &format!("physics2d: {msg}"));
}

// ---------------------------------------------------------------- joints

#[derive(Clone, Debug)]
enum JointKind {
    Revolute(Vec2),
    Prismatic(Vec2, Vec2),
    Fixed,
    Rope(Vec2, Vec2),
    Spring(Vec2, Vec2),
    Distance(Vec2, Vec2),
    Wheel(Vec2, Vec2),
    Mouse(Vec2),
}

/// How to connect two bodies. Anchors are world (screen) points where the
/// bodies are *now*; create joints after placing the bodies. Bodies joined
/// together don't collide with each other unless you ask ([`Joint::collide`]).
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
    /// A hinge at `anchor`: wheels, doors, pendulums, flails.
    pub fn revolute(a: BodyId, b: BodyId, anchor: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Revolute(anchor))
    }
    /// A slider through `anchor` along `axis`: pistons, elevators, suspension.
    pub fn prismatic(a: BodyId, b: BodyId, anchor: Vec2, axis: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Prismatic(anchor, axis))
    }
    /// Glues `b` to `a` as they are now: breakable structures (see [`World::joint_force`]).
    pub fn fixed(a: BodyId, b: BodyId) -> Joint {
        Joint::new(a, b, JointKind::Fixed)
    }
    /// A rope from `anchor_a` (on `a`) to `anchor_b` (on `b`): never longer
    /// than it is now (or [`Joint::length`]), free to go slack.
    pub fn rope(a: BodyId, b: BodyId, anchor_a: Vec2, anchor_b: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Rope(anchor_a, anchor_b))
    }
    /// A spring from `anchor_a` to `anchor_b`, resting at its current length
    /// (or [`Joint::length`]); tune it with [`Joint::spring`].
    pub fn spring(a: BodyId, b: BodyId, anchor_a: Vec2, anchor_b: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Spring(anchor_a, anchor_b))
    }
    /// Keeps `anchor_a` (on `a`) and `anchor_b` (on `b`) exactly as far
    /// apart as they are now (or [`Joint::length`]): Box2D's distance joint
    /// with no spring.
    pub fn distance(a: BodyId, b: BodyId, anchor_a: Vec2, anchor_b: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Distance(anchor_a, anchor_b))
    }
    /// A wheel on `b` at `anchor`: free to spin, sprung along `axis` (the
    /// suspension; tune it with [`Joint::stiffness`]), held across it. A
    /// motor drives the spin, as Box2D's wheel joint's does.
    pub fn wheel(a: BodyId, b: BodyId, anchor: Vec2, axis: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Wheel(anchor, axis))
    }
    /// Pulls `b`'s point at `anchor` toward `a`'s (a body you move to where
    /// the finger is) by a spring ([`Joint::stiffness`]), at most
    /// [`Joint::motor`]'s force: Box2D's mouse joint, for dragging.
    pub fn mouse(a: BodyId, b: BodyId, anchor: Vec2) -> Joint {
        Joint::new(a, b, JointKind::Mouse(anchor))
    }
    /// Revolute: min/max angle in radians relative to now. Prismatic: min/max px along the axis.
    pub fn limits(mut self, min: f32, max: f32) -> Joint {
        self.limits = Some((min.min(max), min.max(max)));
        self
    }
    /// Drives a revolute (radians/s, clockwise) or prismatic (px/s) joint at
    /// `speed`, with at most `max_force` (torque in kg·px²/s² for revolute,
    /// force in kg·px/s² for prismatic; `f32::INFINITY` for no limit).
    pub fn motor(mut self, speed: f32, max_force: f32) -> Joint {
        self.motor = Some((speed, max_force));
        self
    }
    /// Rope: maximum length. Spring: rest length. In px.
    pub fn length(mut self, px: f32) -> Joint {
        self.length = Some(px.max(0.0));
        self
    }
    /// Spring stiffness as a frequency (Hz, 1–5 is bouncy) and a damping
    /// ratio (0 wobbles forever, 1 settles without overshoot). Same feel at any mass.
    pub fn stiffness(mut self, hz: f32, damping_ratio: f32) -> Joint {
        self.spring = (hz.max(0.0), damping_ratio.max(0.0));
        self
    }
    /// Let the two joined bodies collide with each other too.
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
    /// Where they touch (screen px).
    pub point: Vec2,
    /// Unit normal pointing from `a` towards `b`.
    pub normal: Vec2,
    /// How fast they were closing along the normal, px/s, just before
    /// touching. Mass-independent: use it for sound volume and haptics
    /// (e.g. `(speed / 600.0).min(1.0)`); resting contacts are near 0.
    pub speed: f32,
    /// The impulse that stopped them, kg·px/s (heavy things hit harder).
    pub impulse: f32,
}

impl Contact {
    pub fn involves(&self, id: BodyId) -> bool {
        self.a == id || self.b == id
    }
    /// The body `id` touched, if `id` is one of the two.
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
    /// Two solid shapes stopped touching.
    ContactEnd { a: BodyId, b: BodyId, shape_a: ShapeId, shape_b: ShapeId },
    /// `other` started overlapping a sensor shape on body `sensor`.
    SensorEnter { sensor: BodyId, other: BodyId, sensor_shape: ShapeId, other_shape: ShapeId },
    SensorExit { sensor: BodyId, other: BodyId, sensor_shape: ShapeId, other_shape: ShapeId },
}

impl Event {
    /// The two bodies involved.
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

/// Two solid shapes touching now ([`World::touches`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub shape_a: ShapeId,
    pub shape_b: ShapeId,
    /// Unit normal from `shape_a` toward `shape_b`.
    pub normal: Vec2,
    /// Up to two points where they touch (screen px).
    pub points: [Option<Vec2>; 2],
    pub normal_impulse: f32,
    pub tangent_impulse: f32,
}

/// A raycast hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub body: BodyId,
    pub shape: ShapeId,
    pub point: Vec2,
    /// Surface normal at the hit, facing the ray.
    pub normal: Vec2,
    /// px from the ray's start.
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
            // Velocities here are from before this step's solver: the approach speed.
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
            raw.speed = if raw.normal != RVec::ZERO { rel.dot(raw.normal).abs() } else { rel.length() } * PPM;
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
/// read poses in `render`.
pub struct World {
    w: rp::PhysicsWorld,
    clock: Clock,
    inbox: Collector,
    events: Vec<Event>,
    /// Poses before the latest step, by body index (for interpolation).
    prev: Vec<Option<(rp::RigidBodyHandle, RPose)>>,
    interpolate: bool,
    /// Forces applied since the last step (force, torque), in meters.
    forces: Vec<(rp::RigidBodyHandle, RVec, f32)>,
    /// Impulse owed from forces in frames that ran no step.
    owed: Vec<(rp::RigidBodyHandle, RVec, f32)>,
    /// Kinematic targets (pose) for the next step.
    /// Kinematic bodies moved by the last step's targets (stopped at the
    /// next step unless given a new target).
    moved: Vec<rp::RigidBodyHandle>,
    targets: Vec<(rp::RigidBodyHandle, Vec2, f32)>,
    steps: u64,
}

impl Default for World {
    fn default() -> World {
        World::new()
    }
}

impl World {
    /// A world with Earth gravity ([`GRAVITY`] px/s², down the screen).
    pub fn new() -> World {
        World::with_gravity(vec2(0.0, GRAVITY))
    }

    /// A world with gravity in px/s² (`Vec2::ZERO` for top-down games).
    pub fn with_gravity(g: Vec2) -> World {
        let mut w = rp::PhysicsWorld::new();
        w.gravity = to_m(g);
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
        }
    }

    /// Solver iterations per step: Rapier's substeps, so a joint's impulse
    /// ([`World::joint_force`]) is one substep's, and × this is the step's.
    pub fn solver_iterations(&self) -> u32 {
        self.w.integration_parameters.num_solver_iterations as u32
    }
    pub fn gravity(&self) -> Vec2 {
        to_px(self.w.gravity)
    }
    /// px/s². Tilt games set it from `sensors::tilt()` every frame.
    pub fn set_gravity(&mut self, g: Vec2) {
        self.w.gravity = to_m(g);
    }
    /// Fixed steps per second (default 60; 120 for very fast or stiff
    /// contraptions at twice the CPU). 15–480.
    pub fn set_step_rate(&mut self, hz: f32) {
        self.clock.set_rate(hz);
        self.w.integration_parameters.dt = self.clock.h();
    }
    /// Solver iterations per step (default 4). More = stiffer stacks and
    /// chains, at more CPU.
    pub fn set_solver_iterations(&mut self, n: u32) {
        self.w.integration_parameters.num_solver_iterations = n.clamp(1, 32) as usize;
    }
    /// Draw at interpolated poses between the last two steps (default on).
    /// Off draws exactly the simulated state, up to one step stale.
    pub fn set_interpolation(&mut self, on: bool) {
        self.interpolate = on;
    }

    // ------------------------------------------------------------ stepping

    /// Advances the simulation by `dt` seconds (pass `update`'s `dt`), in
    /// fixed steps: it runs as many whole steps as the accumulated time
    /// allows (often 1, sometimes 0 or 2) and returns how many. Deterministic:
    /// the same `dt`s always give the same steps. Clears and refills
    /// [`World::events`].
    pub fn step(&mut self, dt: f32) -> u32 {
        self.events.clear();
        let n = self.clock.advance(dt);
        // Forces applied this frame are owed as impulse (F·dt), so a push
        // is the same whether this frame runs 0, 1 or 2 steps.
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
        // Kinematic bodies with no new target stop; the rest get the
        // velocity that takes them there over these steps.
        for h in std::mem::take(&mut self.moved) {
            if !self.targets.iter().any(|t| t.0 == h)
                && let Some(b) = self.w.bodies.get_mut(h)
            {
                b.set_linvel(RVec::ZERO, false);
                b.set_angvel(0.0, false);
            }
        }
        for &(h, pos, angle) in &self.targets {
            if let Some(b) = self.w.bodies.get_mut(h) {
                let p = pose_from(b.position());
                let v = (pos - p.pos) / span;
                b.set_linvel(to_m(v), true);
                b.set_angvel(angle_delta(p.angle, angle) / span, true);
            }
        }
        for i in 0..n {
            if i == n - 1 && self.interpolate {
                self.snapshot();
            }
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

    /// Contacts and sensor overlaps that began or ended during the last
    /// [`World::step`] call (all its steps), in a deterministic order.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Fixed steps run since the world was created.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// How far game time is between the last step and the next, 0–1.
    pub fn alpha(&self) -> f32 {
        self.clock.alpha()
    }

    fn snapshot(&mut self) {
        let len = self.w.bodies.len();
        self.prev.clear();
        self.prev.resize(len.max(self.prev.len()), None);
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
            let (Some(a), Some(b)) = (parent(raw.c1), parent(raw.c2)) else {
                continue; // a removed body's contacts end silently
            };
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
                let impulse = self.w.narrow_phase.contact_pair(raw.c1, raw.c2).map_or(0.0, |p| p.total_impulse_magnitude());
                // Rapier's normal points from the pair's first collider to its second.
                let flip = self.w.narrow_phase.contact_pair(raw.c1, raw.c2).is_some_and(|p| p.collider1 != raw.c1);
                let normal = to_px(if flip { -raw.normal } else { raw.normal }).normalize();
                Event::ContactBegin(Contact {
                    a,
                    b,
                    shape_a: s1,
                    shape_b: s2,
                    point: to_px(raw.point),
                    normal,
                    speed: raw.speed,
                    impulse: impulse * PPM,
                })
            } else {
                Event::ContactEnd { a, b, shape_a: s1, shape_b: s2 }
            };
            self.events.push(ev);
        }
    }

    // ------------------------------------------------------------ bodies

    /// Adds a body with one shape and returns it.
    pub fn add(&mut self, body: Body, shape: Shape) -> BodyId {
        let id = self.add_body(body);
        self.add_shape(id, shape);
        id
    }

    /// Adds a body with no shapes yet (add them with [`World::add_shape`]).
    pub fn add_body(&mut self, body: Body) -> BodyId {
        BodyId(self.w.insert_body(body.build()))
    }

    /// Adds another shape to a body (compound shapes; mass adds up).
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

    /// Removes a body with its shapes and joints. Its contacts end without
    /// [`Event::ContactEnd`] events.
    pub fn remove(&mut self, id: BodyId) {
        self.w.remove_body(id.0);
    }

    pub fn remove_shape(&mut self, id: ShapeId) {
        self.w.remove_collider(id.0);
    }

    /// False once removed.
    pub fn contains(&self, id: BodyId) -> bool {
        self.w.bodies.contains(id.0)
    }

    /// Every body, in a deterministic order.
    pub fn bodies(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.w.bodies.iter().map(|(h, _)| BodyId(h))
    }

    pub fn len(&self) -> usize {
        self.w.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.w.bodies.is_empty()
    }

    /// The shapes on a body.
    pub fn shapes(&self, id: BodyId) -> Vec<ShapeId> {
        self.w.bodies.get(id.0).map_or_else(Vec::new, |b| b.colliders().iter().map(|&c| ShapeId(c)).collect())
    }

    /// The body a shape belongs to.
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
    /// Changes a body's kind, e.g. freezes a placed block (`Fixed`) or
    /// releases a held one (`Dynamic`).
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

    /// Your number from [`Body::tag`] (0 for a stale id).
    pub fn tag(&self, id: BodyId) -> u64 {
        self.body(id).map_or(0, |b| b.user_data as u64)
    }
    pub fn set_tag(&mut self, id: BodyId, tag: u64) {
        if let Some(b) = self.body_mut(id) {
            b.user_data = tag as u128;
        }
    }

    /// Center position (the simulated state), px.
    pub fn position(&self, id: BodyId) -> Vec2 {
        self.body(id).map_or(Vec2::ZERO, |b| to_px(b.translation()))
    }
    /// Rotation, radians clockwise (not wrapped: a wheel keeps counting).
    pub fn angle(&self, id: BodyId) -> f32 {
        self.body(id).map_or(0.0, |b| b.rotation().angle())
    }
    /// The simulated pose. Draw with [`World::draw_pose`] instead.
    pub fn pose(&self, id: BodyId) -> Pose {
        self.body(id).map_or(Pose::default(), |b| pose_from(b.position()))
    }
    /// px/s.
    pub fn velocity(&self, id: BodyId) -> Vec2 {
        self.body(id).map_or(Vec2::ZERO, |b| to_px(b.linvel()))
    }
    /// radians/s, clockwise.
    pub fn spin(&self, id: BodyId) -> f32 {
        self.body(id).map_or(0.0, |b| b.angvel())
    }
    /// kg (density × area in m²).
    pub fn mass(&self, id: BodyId) -> f32 {
        self.body(id).map_or(0.0, |b| b.mass())
    }
    /// The velocity of a point on the body, px/s.
    pub fn velocity_at(&self, id: BodyId, point: Vec2) -> Vec2 {
        self.body(id).map_or(Vec2::ZERO, |b| to_px(b.velocity_at_point(to_m(point))))
    }

    /// Teleports a body (no collision on the way; it doesn't interpolate).
    pub fn set_position(&mut self, id: BodyId, pos: Vec2) {
        if let Some(b) = self.body_mut(id) {
            b.set_translation(to_m(pos), true);
        }
        self.forget_prev(id);
        self.refresh(id);
    }
    pub fn set_angle(&mut self, id: BodyId, radians: f32) {
        if let Some(b) = self.body_mut(id) {
            b.set_rotation(RRot::new(radians), true);
        }
        self.forget_prev(id);
        self.refresh(id);
    }
    pub fn set_velocity(&mut self, id: BodyId, v: Vec2) {
        if let Some(b) = self.body_mut(id) {
            b.set_linvel(to_m(v), true);
        }
    }
    pub fn set_spin(&mut self, id: BodyId, radians_per_s: f32) {
        if let Some(b) = self.body_mut(id) {
            b.set_angvel(radians_per_s, true);
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

    /// Moves a kinematic body to `pos`/`angle` over the next step(s),
    /// sweeping (and pushing) what's in the way. Call it every frame with
    /// where it should be (a finger-dragged paddle); it stops at the first
    /// step after you stop. [`World::velocity`] reads the speed it moves at.
    pub fn move_kinematic(&mut self, id: BodyId, pos: Vec2, angle: f32) {
        self.targets.retain(|t| t.0 != id.0);
        self.targets.push((id.0, pos, angle));
    }

    /// An instant kick: changes velocity by `impulse / mass` (kg·px/s).
    pub fn apply_impulse(&mut self, id: BodyId, impulse: Vec2) {
        if let Some(b) = self.body_mut(id) {
            b.apply_impulse(to_m(impulse), true);
        }
    }
    /// A kick at a world point (off-center kicks also spin it).
    pub fn apply_impulse_at(&mut self, id: BodyId, impulse: Vec2, point: Vec2) {
        if let Some(b) = self.body_mut(id) {
            b.apply_impulse_at_point(to_m(impulse), to_m(point), true);
        }
    }
    /// An instant spin kick (kg·px²/s; changes spin by it over the moment of inertia).
    pub fn apply_angular_impulse(&mut self, id: BodyId, impulse: f32) {
        if let Some(b) = self.body_mut(id) {
            b.apply_torque_impulse(impulse * MPP * MPP, true);
        }
    }
    /// Changes velocity by `dv` px/s regardless of mass (a jump, a launch).
    pub fn add_velocity(&mut self, id: BodyId, dv: Vec2) {
        if let Some(b) = self.body_mut(id) {
            let v = b.linvel() + to_m(dv);
            b.set_linvel(v, true);
        }
    }
    /// Pushes with force `f` (kg·px/s²) during this frame. Call it every
    /// frame you want to push (thrusters, wind, magnets): it's frame-rate
    /// independent, applied over the next step(s) as `f × dt` of impulse.
    pub fn apply_force(&mut self, id: BodyId, f: Vec2) {
        self.add_force(id, to_m(f), 0.0);
    }
    /// A force at a world point this frame (spins it if off-center).
    pub fn apply_force_at(&mut self, id: BodyId, f: Vec2, point: Vec2) {
        let Some(b) = self.body(id) else { return };
        let r = to_m(point) - b.center_of_mass();
        let fm = to_m(f);
        self.add_force(id, fm, r.perp_dot(fm));
    }
    /// A turning force this frame (kg·px²/s², clockwise).
    pub fn apply_torque(&mut self, id: BodyId, torque: f32) {
        self.add_force(id, RVec::ZERO, torque * MPP * MPP);
    }
    fn add_force(&mut self, id: BodyId, f: RVec, t: f32) {
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
    /// Dynamic bodies still awake: 0 means everything has settled (a
    /// stacker's "is the tower stable yet?").
    pub fn awake_count(&self) -> usize {
        self.w.bodies.iter().filter(|(_, b)| b.is_dynamic() && !b.is_sleeping()).count()
    }

    /// Changes a shape's material after creation.
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
    /// Turns a shape into a sensor or back.
    pub fn set_sensor(&mut self, shape: ShapeId, sensor: bool) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_sensor(sensor);
        }
    }
    /// Makes a shape a one-way platform for characters, or not ([`Shape::one_way`]).
    pub fn set_one_way(&mut self, shape: ShapeId, on: bool) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.user_data = if on { c.user_data | ONE_WAY } else { c.user_data & !ONE_WAY };
        }
    }
    pub fn is_one_way(&self, shape: ShapeId) -> bool {
        self.w.colliders.get(shape.0).is_some_and(|c| c.user_data & ONE_WAY != 0)
    }

    // ------------------------------------------------------------ joints

    /// Connects two bodies (anchors in world px where they are now).
    pub fn join(&mut self, j: Joint) -> JointId {
        let pa = self.body(j.a).map_or(RPose::identity(), |b| *b.position());
        let pb = self.body(j.b).map_or(RPose::identity(), |b| *b.position());
        let local_a = |p: Vec2| pa.inverse_transform_point(to_m(p));
        let local_b = |p: Vec2| pb.inverse_transform_point(to_m(p));
        let mut data: rp::GenericJoint = match j.kind {
            JointKind::Revolute(anchor) => {
                let mut r = rp::RevoluteJointBuilder::new().local_anchor1(local_a(anchor)).local_anchor2(local_b(anchor));
                if let Some((lo, hi)) = j.limits {
                    r = r.limits([lo, hi]);
                }
                if let Some((speed, max)) = j.motor {
                    r = r.motor_velocity(speed, MOTOR_GAIN).motor_max_force(max * MPP * MPP);
                }
                let mut g: rp::GenericJoint = r.build().into();
                // Measure the angle from how the bodies are turned now.
                g.local_frame1.rotation = pa.rotation.inverse() * pb.rotation;
                g.local_frame2.rotation = RRot::identity();
                g
            }
            JointKind::Prismatic(anchor, axis) => {
                let axis = to_m(axis.normalize());
                let axis = if axis == RVec::ZERO { RVec::X } else { axis.normalize() };
                let mut p = rp::PrismaticJointBuilder::new(axis)
                    .local_axis1(pa.rotation.inverse_transform_vector(axis))
                    .local_axis2(pb.rotation.inverse_transform_vector(axis))
                    .local_anchor1(local_a(anchor))
                    .local_anchor2(local_b(anchor));
                if let Some((lo, hi)) = j.limits {
                    p = p.limits([lo * MPP, hi * MPP]);
                }
                if let Some((speed, max)) = j.motor {
                    p = p.motor_velocity(speed * MPP, MOTOR_GAIN).motor_max_force(max * MPP);
                }
                p.build().into()
            }
            JointKind::Fixed => {
                rp::FixedJointBuilder::new().local_frame1(pa.inv_mul(&pb)).local_frame2(RPose::identity()).build().into()
            }
            JointKind::Rope(a, b) => {
                let len = j.length.unwrap_or_else(|| a.distance(b));
                rp::RopeJointBuilder::new(len * MPP).local_anchor1(local_a(a)).local_anchor2(local_b(b)).build().into()
            }
            JointKind::Spring(a, b) => {
                let rest = j.length.unwrap_or_else(|| a.distance(b));
                let w = std::f32::consts::TAU * j.spring.0;
                rp::SpringJointBuilder::new(rest * MPP, w * w, 2.0 * j.spring.1 * w)
                    .spring_model(rp::MotorModel::AccelerationBased)
                    .local_anchor1(local_a(a))
                    .local_anchor2(local_b(b))
                    .build()
                    .into()
            }
            JointKind::Distance(a, b) => {
                let len = j.length.unwrap_or_else(|| a.distance(b)) * MPP;
                let mut g: rp::GenericJoint = rp::RopeJointBuilder::new(len).local_anchor1(local_a(a)).local_anchor2(local_b(b)).build().into();
                g.set_limits(rp::JointAxis::LinX, [len, len]);
                g
            }
            JointKind::Wheel(anchor, axis) => {
                let axis = if axis.length() > 0.0 { axis.normalize() } else { Vec2::Y };
                let world = axis.angle();
                let mut g = rp::GenericJoint::new(rp::JointAxesMask::LIN_Y);
                g.local_frame1 = RPose::new(local_a(anchor), world - pa.rotation.angle());
                g.local_frame2 = RPose::new(local_b(anchor), world - pb.rotation.angle());
                let w = std::f32::consts::TAU * j.spring.0;
                g.set_motor_model(rp::JointAxis::LinX, rp::MotorModel::AccelerationBased);
                g.set_motor_position(rp::JointAxis::LinX, 0.0, w * w, 2.0 * j.spring.1 * w);
                if let Some((speed, max)) = j.motor {
                    g.set_motor_velocity(rp::JointAxis::AngX, speed, MOTOR_GAIN);
                    g.set_motor_max_force(rp::JointAxis::AngX, max * MPP * MPP);
                }
                g
            }
            JointKind::Mouse(anchor) => {
                let mut g = rp::GenericJoint::new(rp::JointAxesMask::empty());
                g.local_frame1 = RPose::new(local_a(anchor), -pa.rotation.angle());
                g.local_frame2 = RPose::new(local_b(anchor), -pb.rotation.angle());
                let w = std::f32::consts::TAU * j.spring.0;
                for axis in [rp::JointAxis::LinX, rp::JointAxis::LinY] {
                    g.set_motor_model(axis, rp::MotorModel::AccelerationBased);
                    g.set_motor_position(axis, 0.0, w * w, 2.0 * j.spring.1 * w);
                    if let Some((_, max)) = j.motor {
                        g.set_motor_max_force(axis, max * MPP);
                    }
                }
                g
            }
        };
        data.set_contacts_enabled(j.collide);
        JointId(self.w.impulse_joints.insert(j.a.0, j.b.0, data, true))
    }

    /// Removes a joint (breaking it).
    pub fn unjoin(&mut self, id: JointId) {
        self.w.remove_impulse_joint(id.0);
    }

    pub fn has_joint(&self, id: JointId) -> bool {
        self.w.impulse_joints.get(id.0).is_some()
    }

    /// How hard the joint pulled in its last step, kg·px/s² (compare
    /// against a threshold and [`World::unjoin`] to make it breakable).
    pub fn joint_force(&self, id: JointId) -> f32 {
        self.w.impulse_joints.get(id.0).map_or(0.0, |j| {
            let lin = RVec::new(j.impulses.x, j.impulses.y);
            lin.length() / self.clock.h() * PPM
        })
    }

    /// How hard the joint twisted in its last step, kg·px²/s² (a fixed
    /// joint holding up a leaning stack twists; break it past a limit).
    pub fn joint_torque(&self, id: JointId) -> f32 {
        self.w.impulse_joints.get(id.0).map_or(0.0, |j| j.impulses.z.abs() / self.clock.h() * PPM * PPM)
    }

    /// Changes a revolute/prismatic joint's motor (units as [`Joint::motor`]).
    pub fn set_motor(&mut self, id: JointId, speed: f32, max_force: f32) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        let (axis, s, f) = if j.data.locked_axes.contains(rp::JointAxesMask::LIN_X) || !j.data.locked_axes.contains(rp::JointAxesMask::ANG_X) {
            (rp::JointAxis::AngX, speed, max_force * MPP * MPP)
        } else {
            (rp::JointAxis::LinX, speed * MPP, max_force * MPP)
        };
        j.data.set_motor_velocity(axis, s, MOTOR_GAIN);
        j.data.set_motor_max_force(axis, f);
    }

    /// The two bodies a joint connects.
    pub fn joint_bodies(&self, id: JointId) -> Option<(BodyId, BodyId)> {
        self.w.impulse_joints.get(id.0).map(|j| (BodyId(j.body1()), BodyId(j.body2())))
    }

    /// Every joint, in a deterministic order.
    pub fn joints(&self) -> impl Iterator<Item = JointId> + '_ {
        self.w.impulse_joints.iter().map(|(h, _)| JointId(h))
    }


    /// Changes a revolute (radians) or prismatic (px) joint's limits; None frees it.
    pub fn set_limits(&mut self, id: JointId, limits: Option<(f32, f32)>) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        let (axis, k) = if j.data.locked_axes.contains(rp::JointAxesMask::LIN_X) { (rp::JointAxis::AngX, 1.0) } else { (rp::JointAxis::LinX, MPP) };
        match limits {
            Some((lo, hi)) => {
                j.data.set_limits(axis, [lo.min(hi) * k, lo.max(hi) * k]);
            }
            None => {
                j.data.limit_axes.remove(axis.into());
            }
        }
    }
    /// A rope's longest or a distance or spring joint's length, px.
    pub fn set_joint_length(&mut self, id: JointId, px: f32) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        let m = px.max(0.0) * MPP;
        if j.data.motor_axes.contains(rp::JointAxesMask::LIN_X) && j.data.coupled_axes.contains(rp::JointAxesMask::LIN_AXES) {
            // A spring: its rest length.
            let (stiff, damp) = (j.data.motors[0].stiffness, j.data.motors[0].damping);
            j.data.set_motor_position(rp::JointAxis::LinX, m, stiff, damp);
        } else if j.data.limit_axes.contains(rp::JointAxesMask::LIN_X) {
            let lo = if j.data.limits[0].min > 0.0 { m } else { 0.0 };
            j.data.set_limits(rp::JointAxis::LinX, [lo, m]);
        }
    }
    /// A spring, wheel or mouse joint's stiffness (Hz) and damping ratio.
    pub fn set_joint_spring(&mut self, id: JointId, hz: f32, damping_ratio: f32) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        let w = std::f32::consts::TAU * hz.max(0.0);
        for (i, axis, mask) in [(0, rp::JointAxis::LinX, rp::JointAxesMask::LIN_X), (1, rp::JointAxis::LinY, rp::JointAxesMask::LIN_Y)] {
            if i == 0 || j.data.motor_axes.contains(mask) {
                let target = j.data.motors[i].target_pos;
                j.data.set_motor_position(axis, target, w * w, 2.0 * damping_ratio.max(0.0) * w);
            }
        }
    }
    /// A mouse joint's most force (kg·px/s²).
    pub fn set_joint_max_force(&mut self, id: JointId, max_force: f32) {
        let Some(j) = self.w.impulse_joints.get_mut(id.0, true) else { return };
        for axis in [rp::JointAxis::LinX, rp::JointAxis::LinY] {
            j.data.set_motor_max_force(axis, max_force * MPP);
        }
    }
    pub fn is_fixed_rotation(&self, id: BodyId) -> bool {
        self.body(id).is_some_and(|b| b.locked_axes().contains(rp::LockedAxes::ROTATION_LOCKED))
    }
    pub fn is_bullet(&self, id: BodyId) -> bool {
        self.body(id).is_some_and(|b| b.is_ccd_enabled())
    }
    pub fn gravity_scale(&self, id: BodyId) -> f32 {
        self.body(id).map_or(1.0, |b| b.gravity_scale())
    }
    /// Linear and angular damping.
    pub fn damping(&self, id: BodyId) -> (f32, f32) {
        self.body(id).map_or((0.0, 0.0), |b| (b.linear_damping(), b.angular_damping()))
    }

    /// Changes a shape's collision layers (see [`Shape::groups`]).
    pub fn set_groups(&mut self, shape: ShapeId, member: u32, collides_with: u32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_collision_groups(rp::InteractionGroups::new(
                rp::Group::from_bits_retain(member),
                rp::Group::from_bits_retain(collides_with),
                rp::InteractionTestMode::And,
            ));
        }
    }
    /// Changes a shape's density (kg per 100×100 px); its body's mass follows.
    pub fn set_density(&mut self, shape: ShapeId, d: f32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_density(d.max(0.0));
        }
    }
    /// A shape's own mass in kg.
    pub fn shape_mass(&self, shape: ShapeId) -> f32 {
        self.w.colliders.get(shape.0).map_or(0.0, |c| c.mass())
    }
    /// A shape's mass in kg, instead of from its density.
    pub fn set_shape_mass(&mut self, shape: ShapeId, kg: f32) {
        if let Some(c) = self.w.colliders.get_mut(shape.0) {
            c.set_mass(kg.max(0.0));
        }
    }
    /// The body's moment of inertia about its centre of mass, kg·px².
    pub fn inertia(&self, id: BodyId) -> f32 {
        self.body(id).map_or(0.0, |b| b.mass_properties().local_mprops.principal_inertia() * PPM * PPM)
    }
    /// The body's centre of mass in the world (px).
    pub fn center_of_mass(&self, id: BodyId) -> Vec2 {
        self.body(id).map_or(Vec2::ZERO, |b| to_px(b.center_of_mass()))
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
    /// Continuous collision detection on or off.
    pub fn set_bullet(&mut self, id: BodyId, on: bool) {
        if let Some(b) = self.body_mut(id) {
            b.enable_ccd(on);
        }
    }

    /// Every pair of solid shapes touching now, in a deterministic order:
    /// where they touch, the normal from `shape_a` to `shape_b`, and the
    /// impulses the last step pushed them apart (along the normal) and
    /// held them from sliding with (along the surface), kg·px/s.
    pub fn touches(&self) -> Vec<Touch> {
        let mut out: Vec<Touch> = self.w.narrow_phase.contact_pairs().filter(|p| p.has_any_active_contact()).filter_map(|p| self.touch_of(p)).collect();
        out.sort_by_key(|t| (t.shape_a, t.shape_b));
        out
    }
    /// How two shapes touch now, if they do (the normal from `a` to `b`).
    pub fn touch(&self, a: ShapeId, b: ShapeId) -> Option<Touch> {
        let p = self.w.narrow_phase.contact_pair(a.0, b.0)?;
        let mut t = self.touch_of(p)?;
        if t.shape_a != a {
            std::mem::swap(&mut t.shape_a, &mut t.shape_b);
            t.normal = -t.normal;
        }
        Some(t)
    }
    fn touch_of(&self, p: &rp::ContactPair) -> Option<Touch> {
        let m = p.manifolds.iter().find(|m| !m.data.solver_contacts.is_empty())?;
        let mut points = [None, None];
        for (i, sc) in m.data.solver_contacts.iter().take(2).enumerate() {
            points[i] = Some(to_px(sc.point));
        }
        let (mut normal_impulse, mut tangent_impulse) = (0.0, 0.0);
        for pt in &m.points {
            normal_impulse += pt.data.impulse;
            tangent_impulse += pt.data.tangent_impulse.x;
        }
        Some(Touch {
            shape_a: ShapeId(p.collider1),
            shape_b: ShapeId(p.collider2),
            normal: to_px(m.data.normal).normalize(),
            points,
            normal_impulse: normal_impulse * PPM,
            tangent_impulse: tangent_impulse * PPM,
        })
    }

    // ------------------------------------------------------------ queries

    /// The first shape along the segment `from` → `to` (sensors ignored).
    pub fn raycast(&self, from: Vec2, to: Vec2) -> Option<RayHit> {
        self.raycast_filtered(from, to, None)
    }
    /// Like [`World::raycast`] but never hits `ignore` (e.g. the shooter itself).
    pub fn raycast_ignoring(&self, from: Vec2, to: Vec2, ignore: BodyId) -> Option<RayHit> {
        self.raycast_filtered(from, to, Some(ignore))
    }
    fn raycast_filtered(&self, from: Vec2, to: Vec2, ignore: Option<BodyId>) -> Option<RayHit> {
        let d = to - from;
        let len = d.length();
        if len <= 0.0 {
            return None;
        }
        let mut filter = rp::QueryFilter::new().exclude_sensors();
        if let Some(i) = ignore {
            filter = filter.exclude_rigid_body(i.0);
        }
        let ray = rp::Ray::new(to_m(from), RVec::new(d.x / len, d.y / len));
        let (c, hit) = self.w.query_pipeline_with_filter(filter).cast_ray_and_get_normal(&ray, len * MPP, true)?;
        let body = self.w.colliders.get(c)?.parent()?;
        Some(RayHit {
            body: BodyId(body),
            shape: ShapeId(c),
            point: from + d / len * (hit.time_of_impact * PPM),
            normal: to_px(hit.normal).normalize(),
            distance: hit.time_of_impact * PPM,
        })
    }


    /// Every shape along the segment `from` → `to`, sensors included,
    /// nearest first (Box2D's `RayCast` sees them all).
    pub fn raycast_all(&self, from: Vec2, to: Vec2) -> Vec<RayHit> {
        let d = to - from;
        let len = d.length();
        if !(len > 0.0) {
            return Vec::new();
        }
        let dir = d / len;
        let ray = rp::Ray::new(to_m(from), RVec::new(dir.x, dir.y));
        let mut out: Vec<RayHit> = self
            .w
            .query_pipeline()
            .intersect_ray(ray, len * MPP, true)
            .filter_map(|(c, col, hit)| {
                Some(RayHit { body: BodyId(col.parent()?), shape: ShapeId(c), point: from + dir * (hit.time_of_impact * PPM), normal: to_px(hit.normal).normalize(), distance: hit.time_of_impact * PPM })
            })
            .collect();
        out.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.shape.cmp(&b.shape)));
        out
    }
    /// The shapes whose bounding boxes overlap the box `min`–`max`, sorted.
    pub fn shapes_in_rect(&self, min: Vec2, max: Vec2) -> Vec<ShapeId> {
        let aabb = rapier2d::parry::bounding_volume::Aabb::new(to_m(min), to_m(max));
        let mut out: Vec<ShapeId> = self
            .w
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
            .filter(|(_, c)| {
                let b = c.compute_aabb();
                b.mins.x <= aabb.maxs.x && b.maxs.x >= aabb.mins.x && b.mins.y <= aabb.maxs.y && b.maxs.y >= aabb.mins.y
            })
            .map(|(h, _)| ShapeId(h))
            .collect();
        out.sort();
        out.dedup();
        out
    }
    /// A shape's bounding box in the world (px): min, max.
    pub fn shape_bounds(&self, shape: ShapeId) -> (Vec2, Vec2) {
        self.w.colliders.get(shape.0).map_or((Vec2::ZERO, Vec2::ZERO), |c| {
            let b = c.compute_aabb();
            (to_px(b.mins), to_px(b.maxs))
        })
    }
    /// Whether a shape covers a world point.
    pub fn shape_contains(&self, shape: ShapeId, point: Vec2) -> bool {
        self.w.colliders.get(shape.0).is_some_and(|c| c.shape().contains_point(c.position(), to_m(point)))
    }

    /// The bodies whose shapes contain `point` (sensors included), in a
    /// deterministic order: "what did the player tap?".
    pub fn bodies_at(&self, point: Vec2) -> Vec<BodyId> {
        let mut out: Vec<BodyId> = self
            .w
            .query_pipeline()
            .intersect_point(to_m(point))
            .filter_map(|(_, c)| c.parent().map(BodyId))
            .collect();
        out.sort();
        out.dedup();
        out
    }
    /// The first dynamic body at `point`, else any body there.
    pub fn body_at(&self, point: Vec2) -> Option<BodyId> {
        let all = self.bodies_at(point);
        all.iter().copied().find(|&b| self.kind(b) == BodyKind::Dynamic).or(all.first().copied())
    }
    /// The bodies with a shape overlapping the box `min`–`max` (by bounding box).
    pub fn bodies_in_rect(&self, min: Vec2, max: Vec2) -> Vec<BodyId> {
        let aabb = rapier2d::parry::bounding_volume::Aabb::new(to_m(min), to_m(max));
        let mut out: Vec<BodyId> = self
            .w
            .query_pipeline()
            .intersect_aabb_conservative(aabb)
            .filter(|(_, c)| {
                let b = c.compute_aabb();
                b.mins.x <= aabb.maxs.x && b.maxs.x >= aabb.mins.x && b.mins.y <= aabb.maxs.y && b.maxs.y >= aabb.mins.y
            })
            .filter_map(|(_, c)| c.parent().map(BodyId))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Whether two bodies are touching now (solid contact).
    pub fn touching(&self, a: BodyId, b: BodyId) -> bool {
        self.contacts(a).contains(&b)
    }
    /// The bodies touching `id` now (solid contacts), sorted. A sensor's
    /// overlaps are in [`World::overlapping`].
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
    /// The bodies overlapping any sensor shape on `sensor` now, sorted.
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

    /// Where to draw a body this frame: between its last two simulated
    /// poses by [`World::alpha`], so motion is smooth whatever the frame rate.
    pub fn draw_pose(&self, id: BodyId) -> Pose {
        let Some(b) = self.body(id) else { return Pose::default() };
        let cur = *b.position();
        let i = id.0.into_raw_parts().0 as usize;
        match self.prev.get(i).copied().flatten() {
            Some((h, prev)) if self.interpolate && h == id.0 && !b.is_sleeping() => {
                pose_from(&prev.lerp(&cur, self.clock.alpha()))
            }
            _ => pose_from(&cur),
        }
    }

    /// Runs `draw` with gfx2d moved and turned to the body's draw pose, so
    /// you draw it centered on (0, 0): `world.with_transform(id, || gfx2d::rect(-20.0, -20.0, 40.0, 40.0, c))`.
    pub fn with_transform(&self, id: BodyId, draw: impl FnOnce()) {
        let p = self.draw_pose(id);
        gfx2d::push();
        gfx2d::translate(p.pos.x, p.pos.y);
        gfx2d::rotate(p.angle);
        draw();
        gfx2d::pop();
    }

    /// Draws a body's shapes filled with `rgba` (prototypes, debris, simple art).
    pub fn draw_body(&self, id: BodyId, rgba: u32) {
        let Some(b) = self.body(id) else { return };
        let colliders: Vec<_> = b.colliders().to_vec();
        self.with_transform(id, || {
            for c in colliders {
                if let Some(c) = self.w.colliders.get(c) {
                    let local = c.position_wrt_parent().copied().unwrap_or(RPose::identity());
                    draw_shape(c.shape().as_typed_shape(), &local, rgba);
                }
            }
        });
    }

    /// Draws every body (dynamic orange, asleep grey, fixed blue-grey,
    /// kinematic green, sensors faint) and every joint as a line. For
    /// checking that the art matches the physics.
    pub fn draw_debug(&self) {
        for (h, b) in self.w.bodies.iter() {
            let rgba = if b.is_fixed() {
                0x6d7b8dff
            } else if b.is_kinematic() {
                0x52c47aff
            } else if b.is_sleeping() {
                0x9a8f86ff
            } else {
                0xf2994aff
            };
            let id = BodyId(h);
            let colliders: Vec<_> = b.colliders().to_vec();
            self.with_transform(id, || {
                for c in colliders {
                    if let Some(c) = self.w.colliders.get(c) {
                        let local = c.position_wrt_parent().copied().unwrap_or(RPose::identity());
                        let col = if c.is_sensor() { (rgba & 0xffffff00) | 0x40 } else { (rgba & 0xffffff00) | 0xa0 };
                        draw_shape(c.shape().as_typed_shape(), &local, col);
                    }
                }
                gfx2d::line(0.0, 0.0, 8.0, 0.0, 2.0, 0xffffffc0);
            });
        }
        for (_, j) in self.w.impulse_joints.iter() {
            let a = self.draw_pose(BodyId(j.body1()));
            let b = self.draw_pose(BodyId(j.body2()));
            let pa = a.apply(to_px(j.data.local_frame1.translation));
            let pb = b.apply(to_px(j.data.local_frame2.translation));
            gfx2d::line(a.pos.x, a.pos.y, pa.x, pa.y, 1.5, 0xffe066c0);
            gfx2d::line(pa.x, pa.y, pb.x, pb.y, 2.0, 0xffe066ff);
            gfx2d::line(pb.x, pb.y, b.pos.x, b.pos.y, 1.5, 0xffe066c0);
        }
    }

    /// A hash of every body's position, rotation and velocity bits: equal
    /// hashes mean bit-identical worlds (for determinism tests and logs).
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
            eat(p.translation.x);
            eat(p.translation.y);
            eat(p.rotation.re);
            eat(p.rotation.im);
            eat(b.linvel().x);
            eat(b.linvel().y);
            eat(b.angvel());
        }
        h
    }

    /// The Rapier world underneath, for anything this API doesn't cover.
    /// It's in meters with +y down; changes you make through it are still
    /// deterministic as long as they depend only on game state.
    pub fn rapier(&mut self) -> &mut rp::PhysicsWorld {
        &mut self.w
    }
}

/// Motor velocity gain: how quickly a motor reaches its target speed
/// (acceleration-based, so independent of mass); `max_force` caps it.
const MOTOR_GAIN: f32 = 60.0;

fn draw_shape(s: TypedShape, local: &RPose, rgba: u32) {
    gfx2d::push();
    let o = to_px(local.translation);
    gfx2d::translate(o.x, o.y);
    gfx2d::rotate(local.rotation.angle());
    match s {
        TypedShape::Cuboid(c) => {
            let (w, h) = (c.half_extents.x * PPM, c.half_extents.y * PPM);
            gfx2d::rect(-w, -h, 2.0 * w, 2.0 * h, rgba);
        }
        TypedShape::RoundCuboid(c) => {
            let r = c.border_radius * PPM;
            let (w, h) = (c.inner_shape.half_extents.x * PPM, c.inner_shape.half_extents.y * PPM);
            gfx2d::rect(-w - r, -h, 2.0 * (w + r), 2.0 * h, rgba);
            gfx2d::rect(-w, -h - r, 2.0 * w, 2.0 * (h + r), rgba);
            for (x, y) in [(-w, -h), (w, -h), (-w, h), (w, h)] {
                gfx2d::circle(x, y, r, rgba);
            }
        }
        TypedShape::Ball(b) => gfx2d::circle(0.0, 0.0, b.radius * PPM, rgba),
        TypedShape::Capsule(c) => {
            let (a, b, r) = (to_px(c.segment.a), to_px(c.segment.b), c.radius * PPM);
            gfx2d::line(a.x, a.y, b.x, b.y, 2.0 * r, rgba);
            gfx2d::circle(a.x, a.y, r, rgba);
            gfx2d::circle(b.x, b.y, r, rgba);
        }
        TypedShape::ConvexPolygon(p) => {
            let pts: Vec<f32> = p.points().iter().flat_map(|q| [q.x * PPM, q.y * PPM]).collect();
            gfx2d::poly(&pts, rgba);
        }
        TypedShape::Segment(seg) => {
            let (a, b) = (to_px(seg.a), to_px(seg.b));
            gfx2d::line(a.x, a.y, b.x, b.y, 2.0, rgba);
        }
        TypedShape::Polyline(p) => {
            for seg in p.segments() {
                let (a, b) = (to_px(seg.a), to_px(seg.b));
                gfx2d::line(a.x, a.y, b.x, b.y, 3.0, rgba);
            }
        }
        TypedShape::TriMesh(m) => {
            for t in m.triangles() {
                let pts = [t.a.x * PPM, t.a.y * PPM, t.b.x * PPM, t.b.y * PPM, t.c.x * PPM, t.c.y * PPM];
                gfx2d::poly(&pts, rgba);
            }
        }
        _ => {}
    }
    gfx2d::pop();
}

#[cfg(test)]
#[path = "physics2d_tests.rs"]
mod tests;
