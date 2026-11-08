//! A 2D character controller (docs/STUDIO-2.md §4.14, Decided 18): Godot's
//! `CharacterBody2D` on Rapier's `KinematicCharacterController`, as
//! physics3d's [`crate::physics3d::character`] is in 3D, with the same
//! rules ported from Godot 4.3's scene/2d/physics/character_body_2d.cpp
//! (MIT, Copyright (c) 2014-present Godot Engine contributors, (c) 2007-2014
//! Juan Linietsky, Ariel Manzur): floor, wall and ceiling by angle, holding
//! on to the floor, `floor_block_on_wall`, `wall_min_slide_angle`, and the
//! velocity left after the slide.
//!
//! Units are physics2d's: pixels, +y down, so "up" is −y. A shape made
//! [`super::Shape::one_way`] is a one-way platform for characters: it holds
//! one that comes down onto it from above, and lets it through otherwise.

use rapier2d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier2d::math::{Pose as RPose, Vector as RVec};
use rapier2d::parry::query::ShapeCastOptions;
use rapier2d::prelude as rp;

use super::{BodyId, MPP, ONE_WAY, Shape, Vec2, World, to_m, to_px, vec2};

const ANGLE_THRESHOLD: f32 = 0.01;
/// The gap Rapier keeps between the shape and what it touches, meters
/// (a fifth of a pixel: a pixel art figure doesn't hover).
const OFFSET: f32 = 0.002;

/// Godot's `CharacterBody2D` members and defaults (lengths in pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterSettings {
    pub up: Vec2,
    pub floor_max_angle: f32,
    pub floor_snap_length: f32,
    /// Ledges up to this tall (px) it steps up; 0, none.
    pub max_step_height: f32,
    pub floor_stop_on_slope: bool,
    pub floor_block_on_wall: bool,
    pub wall_min_slide_angle: f32,
    pub slide_on_ceiling: bool,
    pub mask: u32,
}

impl Default for CharacterSettings {
    fn default() -> CharacterSettings {
        CharacterSettings {
            up: vec2(0.0, -1.0),
            floor_max_angle: 45f32.to_radians(),
            floor_snap_length: 1.0,
            max_step_height: 0.0,
            floor_stop_on_slope: true,
            floor_block_on_wall: true,
            wall_min_slide_angle: 15f32.to_radians(),
            slide_on_ceiling: true,
            mask: u32::MAX,
        }
    }
}

/// Something it ran into in its last move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub body: Option<BodyId>,
    pub shape: super::ShapeId,
    /// The surface's normal, out towards the character.
    pub normal: Vec2,
}

pub struct Character {
    body: BodyId,
    collider: rp::ColliderHandle,
    shape: rp::SharedShape,
    pos: Vec2,
    pub settings: CharacterSettings,
    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_normal: Vec2,
    wall_normal: Vec2,
    ceiling_normal: Vec2,
    real_velocity: Vec2,
    hits: Vec<Hit>,
    /// One-way platforms it's passing through (it was in them when its move began).
    inside: Vec<rp::ColliderHandle>,
}

impl Character {
    /// A character at `pos` (its shape's centre, px).
    pub fn new(world: &mut World, pos: Vec2, shape: Shape, settings: CharacterSettings, tag: u64) -> Character {
        let rb = rp::RigidBodyBuilder::kinematic_position_based().pose(RPose::new(to_m(pos), 0.0)).user_data(tag as u128).build();
        let body = BodyId(world.w.insert_body(rb));
        let sid = world.add_shape(body, shape.friction(0.0));
        // Sensors on kinematic bodies and other characters meet it too.
        if let Some(c) = world.w.colliders.get_mut(sid.0) {
            c.set_active_collision_types(rp::ActiveCollisionTypes::default() | rp::ActiveCollisionTypes::KINEMATIC_KINEMATIC | rp::ActiveCollisionTypes::KINEMATIC_FIXED);
        }
        let shared = world.w.colliders.get(sid.0).expect("the character's shape").shared_shape().clone();
        Character {
            body,
            collider: sid.0,
            shape: shared,
            pos,
            settings,
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_normal: Vec2::ZERO,
            wall_normal: Vec2::ZERO,
            ceiling_normal: Vec2::ZERO,
            real_velocity: Vec2::ZERO,
            hits: Vec::new(),
            inside: Vec::new(),
        }
    }

    pub fn body(&self) -> BodyId {
        self.body
    }
    pub fn position(&self) -> Vec2 {
        self.pos
    }
    pub fn on_floor(&self) -> bool {
        self.on_floor
    }
    pub fn on_wall(&self) -> bool {
        self.on_wall
    }
    pub fn on_ceiling(&self) -> bool {
        self.on_ceiling
    }
    pub fn floor_normal(&self) -> Vec2 {
        self.floor_normal
    }
    pub fn wall_normal(&self) -> Vec2 {
        self.wall_normal
    }
    pub fn floor_angle(&self) -> f32 {
        if self.on_floor { angle(self.floor_normal, self.settings.up) } else { 0.0 }
    }
    pub fn real_velocity(&self) -> Vec2 {
        self.real_velocity
    }
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    /// The gap it keeps from what it stands on (px): a figure drawn from
    /// its box's bottom can be drawn that much lower.
    pub fn gap(&self) -> f32 {
        OFFSET / MPP
    }

    /// Half its shape's size (px).
    pub fn half_size(&self) -> Vec2 {
        let a = self.shape.compute_local_aabb();
        to_px(a.maxs)
    }

    pub fn set_position(&mut self, world: &mut World, pos: Vec2) {
        self.pos = pos;
        if let Some(b) = world.w.bodies.get_mut(self.body.0) {
            b.set_position(RPose::new(to_m(pos), 0.0), true);
        }
        self.place(world);
    }

    fn place(&mut self, world: &mut World) {
        let pose = RPose::new(to_m(self.pos), 0.0);
        if let Some(b) = world.w.bodies.get_mut(self.body.0) {
            b.set_next_kinematic_position(pose);
        }
        let pred = world.w.integration_parameters.prediction_distance();
        if let Some(c) = world.w.colliders.get_mut(self.collider) {
            c.set_position(pose);
            let aabb = c.compute_collision_aabb(pred);
            world.w.broad_phase.set_aabb(&world.w.integration_parameters, self.collider, aabb);
        }
    }

    /// Godot's `move_and_slide` (velocity in px/s); returns the velocity left.
    /// `drop` lets it fall through the one-way platforms it stands on.
    pub fn move_and_slide(&mut self, world: &mut World, mut velocity: Vec2, dt: f32, drop: bool) -> Vec2 {
        let s = self.settings;
        let up = s.up.normalize();
        let was_on_floor = self.on_floor;
        let facing_up = velocity.dot(up) > 0.0;
        let start = self.pos;
        (self.on_floor, self.on_wall, self.on_ceiling) = (false, false, false);
        (self.floor_normal, self.wall_normal, self.ceiling_normal) = (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO);
        self.hits.clear();
        if !(dt > 0.0) || !(velocity.x.is_finite() && velocity.y.is_finite()) {
            self.real_velocity = Vec2::ZERO;
            return if velocity.x.is_finite() && velocity.y.is_finite() { velocity } else { Vec2::ZERO };
        }
        let half = self.half_size();
        let kcc = KinematicCharacterController {
            up: to_m(up) / MPP,
            offset: CharacterLength::Absolute(OFFSET),
            slide: true,
            autostep: (s.max_step_height > 0.0).then(|| CharacterAutostep {
                max_height: CharacterLength::Absolute(s.max_step_height * MPP),
                min_width: CharacterLength::Absolute((half.x + 2.0) * MPP),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: s.floor_max_angle,
            // Rapier 2D measures a slope's angle signed (`angle_to`), so a slope
            // leaning one way reads as negative: below −π, any slope slides.
            min_slope_slide_angle: if s.floor_stop_on_slope { s.floor_max_angle } else { -std::f32::consts::TAU },
            snap_to_ground: None,
            normal_nudge_factor: 1.0e-4,
        };
        // One-way platforms: solid only to a character above their top
        // (along `up`) when its move began, unless it's dropping through.
        // One it was already inside stays passable until it's out.
        let foot0 = (start.dot(up) - half_extent(half, up)) * MPP;
        let me = self.body.0;
        let mask = s.mask;
        let inside = self.inside.clone();
        let pred = move |h: rp::ColliderHandle, c: &rp::Collider| {
            if c.is_sensor() || c.parent() == Some(me) || c.collision_groups().memberships.bits() & mask == 0 {
                return false;
            }
            if c.user_data & ONE_WAY != 0 {
                if drop || inside.contains(&h) {
                    return false;
                }
                // The platform's top: its box's far side against `up`.
                let a = c.compute_aabb();
                let top = if up.y <= 0.0 { -a.mins.y } else { a.maxs.y };
                return foot0 >= top - OFFSET * 2.0;
            }
            true
        };
        let filter = rp::QueryFilter::new().predicate(&pred);
        let shared = self.shape.clone();
        let shape = &*shared;
        let mut raw: Vec<(rp::ColliderHandle, RVec)> = Vec::new();
        // On the floor, the fall (gravity a game adds every frame) isn't
        // swept into it: Rapier's sweep along a floor it already touches can
        // stall for a frame. The floor snap below holds it down instead.
        let mut motion = velocity * dt;
        // Godot's `floor_stop_on_slope`: off, standing on a slope with no
        // move across it, the fall goes down along the floor (Rapier slides
        // it), so it slides.
        let slides = !s.floor_stop_on_slope && (velocity - up * velocity.dot(up)).length() < 1e-3;
        if was_on_floor && motion.dot(up) < 0.0 && !slides {
            motion = slide(motion, up);
        }
        let moved = {
            let qp = world.w.query_pipeline_with_filter(filter);
            let r = kcc.move_shape(dt, &qp, shape, &RPose::new(to_m(self.pos), 0.0), to_m(motion), |c| raw.push((c.handle, c.hit.normal1)));
            to_px(r.translation)
        };
        let mut pos = self.pos + moved;
        for (h, n) in &raw {
            let n = vec2(n.x, n.y).normalize();
            let body = world.w.colliders.get(*h).and_then(|c| c.parent()).map(BodyId);
            self.hits.push(Hit { body, shape: super::ShapeId(*h), normal: n });
            if angle(n, up) <= s.floor_max_angle + ANGLE_THRESHOLD {
                self.on_floor = true;
                self.floor_normal = n;
            } else if angle(n, -up) <= s.floor_max_angle + ANGLE_THRESHOLD {
                self.on_ceiling = true;
                self.ceiling_normal = n;
            } else {
                self.on_wall = true;
                self.wall_normal = n;
            }
        }
        let cast = |world: &World, from: Vec2, dist: f32| -> Option<(f32, Vec2)> {
            let qp = world.w.query_pipeline_with_filter(rp::QueryFilter::new().predicate(&pred));
            let opts = ShapeCastOptions { target_distance: OFFSET, stop_at_penetration: false, max_time_of_impact: dist * MPP, compute_impact_geometry_on_penetration: true };
            qp.cast_shape(&RPose::new(to_m(from), 0.0), to_m(-up) / MPP, shape, opts).map(|(_, h)| (h.time_of_impact / MPP, vec2(h.normal1.x, h.normal1.y).normalize()))
        };
        if !self.on_floor {
            let snap = if was_on_floor && !facing_up { s.floor_snap_length.max(2.0) } else { 3.0 };
            if let Some((toi, n)) = cast(world, pos, snap)
                && angle(n, up) <= s.floor_max_angle + ANGLE_THRESHOLD
                && (!facing_up || toi <= 3.0)
            {
                if was_on_floor && !facing_up {
                    pos -= up * toi;
                }
                self.on_floor = true;
                self.floor_normal = n;
            }
        }
        if self.on_wall && (velocity - up * velocity.dot(up)).dot(self.wall_normal) < 0.0 {
            let forward = slide(self.wall_normal, up).normalize();
            if was_on_floor && s.wall_min_slide_angle > 0.0 {
                let horizontal = slide(velocity, up).normalize();
                if horizontal != Vec2::ZERO && libm::acosf((-forward.dot(horizontal)).clamp(-1.0, 1.0)) < s.wall_min_slide_angle {
                    velocity = up * velocity.dot(up);
                }
            }
            velocity = if s.floor_block_on_wall && forward != Vec2::ZERO {
                if facing_up { up * up.dot(velocity) + slide(slide(velocity, self.wall_normal), up) } else { slide(velocity, forward) }
            } else {
                slide(velocity, self.wall_normal)
            };
        }
        if self.on_ceiling && facing_up {
            velocity = if s.slide_on_ceiling { slide(velocity, self.ceiling_normal) } else { slide(velocity, up) };
        }
        if self.on_floor && !facing_up {
            velocity = slide(velocity, up);
        }
        self.pos = pos;
        self.real_velocity = (pos - start) / dt;
        self.place(world);
        // The one-way platforms it's inside now stay passable until it's out.
        let me_pose = RPose::new(to_m(pos), 0.0);
        let qp = world.w.query_pipeline();
        self.inside = qp
            .intersect_shape(me_pose, shape)
            .filter(|(h, c)| c.user_data & ONE_WAY != 0 && *h != self.collider)
            .map(|(h, _)| h)
            .collect();
        velocity
    }

    pub fn remove(self, world: &mut World) {
        world.remove(self.body);
    }
}

/// How far the shape reaches from its centre against `up` (to its feet).
fn half_extent(half: Vec2, up: Vec2) -> f32 {
    half.x * up.x.abs() + half.y * up.y.abs()
}

fn angle(a: Vec2, b: Vec2) -> f32 {
    libm::acosf(a.dot(b).clamp(-1.0, 1.0))
}

fn slide(v: Vec2, n: Vec2) -> Vec2 {
    v - n * v.dot(n)
}
