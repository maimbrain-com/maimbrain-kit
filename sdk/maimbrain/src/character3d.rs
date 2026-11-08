//! A character controller (docs/STUDIO-2.md §4.14, Decided 18): Godot's
//! `CharacterBody3D` on Rapier's `KinematicCharacterController`.
//!
//! Rapier moves the shape: it slides along what it hits, climbs slopes up
//! to `floor_max_angle` and slides down steeper ones, and steps over ledges
//! up to `max_step_height`. On top are Godot's `move_and_slide` rules,
//! ported from Godot 4.3's scene/3d/physics/character_body_3d.cpp (MIT,
//! Copyright (c) 2014-present Godot Engine contributors, (c) 2007-2014 Juan
//! Linietsky, Ariel Manzur; the licence is in games/studio-engine2/NOTICES): what
//! counts as floor, wall and ceiling (`_set_collision_direction`), snapping
//! to the floor when it was on it and isn't going up (`_snap_on_floor`),
//! `floor_block_on_wall` and `wall_min_slide_angle`, and the velocity left
//! after the slide (into a wall or ceiling taken out, and on the floor the
//! fall reset).
//!
//! The character is a kinematic body in the same world as everything else,
//! so rigid bodies and areas see it: [`Character::move_and_slide`] moves it
//! at once (queries see it there) and its body follows at the next step.
//! It steps by the `dt` it's given, so it replays.
//!
//! ```ignore
//! let mut hero = Character::new(&mut world, vec3(0.0, 2.0, 0.0), Shape::capsule(0.35, 1.5), CharacterSettings::default());
//! // update(dt):
//! vel.y -= 18.0 * dt;
//! vel = hero.move_and_slide(&mut world, vel, dt);
//! if hero.on_floor() { /* jump? */ }
//! ```

use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::math::Vector as RVec;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude as rp;

use super::{BodyId, Shape, World, rpose, rv, sv};
use crate::gfx3d::{Quat, Vec3};

/// Godot's `FLOOR_ANGLE_THRESHOLD`.
const ANGLE_THRESHOLD: f32 = 0.01;
/// The gap Rapier keeps between the shape and what it touches (m).
const OFFSET: f32 = 0.01;

/// A character's settings: Godot's `CharacterBody3D` members and defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterSettings {
    pub up: Vec3,
    /// Radians: steeper than this is a wall (Godot's 45°).
    pub floor_max_angle: f32,
    /// How far down it holds on to a floor it was on (m).
    pub floor_snap_length: f32,
    /// Ledges up to this tall it steps up (m); 0, none (Godot has no steps).
    pub max_step_height: f32,
    /// Standing still on a slope, it doesn't slide down.
    pub floor_stop_on_slope: bool,
    /// On the floor, a wall stops it rather than it climbing the wall.
    pub floor_block_on_wall: bool,
    /// Running into a wall within this angle of head on stops it (radians, Godot's 15°).
    pub wall_min_slide_angle: f32,
    pub slide_on_ceiling: bool,
    /// The layers it collides with (Godot's `collision_mask`).
    pub mask: u32,
}

impl Default for CharacterSettings {
    fn default() -> CharacterSettings {
        CharacterSettings {
            up: Vec3::Y,
            floor_max_angle: 45f32.to_radians(),
            floor_snap_length: 0.1,
            max_step_height: 0.0,
            floor_stop_on_slope: true,
            floor_block_on_wall: true,
            wall_min_slide_angle: 15f32.to_radians(),
            slide_on_ceiling: true,
            mask: 1,
        }
    }
}

/// Something it ran into in its last move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub body: Option<BodyId>,
    /// The surface's normal, out towards the character.
    pub normal: Vec3,
    /// Where (about) it touched.
    pub point: Vec3,
}

/// A character in a [`World`].
pub struct Character {
    body: BodyId,
    collider: rp::ColliderHandle,
    shape: rp::SharedShape,
    pos: Vec3,
    pub settings: CharacterSettings,
    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_normal: Vec3,
    wall_normal: Vec3,
    ceiling_normal: Vec3,
    real_velocity: Vec3,
    last_motion: Vec3,
    hits: Vec<Hit>,
}

impl Character {
    /// A character at `pos` (its shape's centre), its shape upright. `tag`
    /// is the body's ([`World::tag`]).
    pub fn new(world: &mut World, pos: Vec3, shape: Shape, settings: CharacterSettings, tag: u64) -> Character {
        let rb = rp::RigidBodyBuilder::kinematic_position_based().pose(rpose(pos, Quat::IDENTITY)).user_data(tag as u128).build();
        let body = BodyId(world.w.insert_body(rb));
        let sid = world.add_shape(body, shape.friction(0.0));
        // Areas (kinematic sensors) and other characters meet it too.
        if let Some(c) = world.w.colliders.get_mut(sid.0) {
            c.set_active_collision_types(rp::ActiveCollisionTypes::default() | rp::ActiveCollisionTypes::KINEMATIC_KINEMATIC);
        }
        let c = world.w.colliders.get(sid.0).expect("the character's shape");
        let shared = c.shared_shape().clone();
        Character {
            body,
            collider: sid.0,
            shape: shared,
            pos,
            settings,
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_normal: Vec3::ZERO,
            wall_normal: Vec3::ZERO,
            ceiling_normal: Vec3::ZERO,
            real_velocity: Vec3::ZERO,
            last_motion: Vec3::ZERO,
            hits: Vec::new(),
        }
    }

    pub fn body(&self) -> BodyId {
        self.body
    }
    /// Its shape's centre.
    pub fn position(&self) -> Vec3 {
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
    pub fn floor_normal(&self) -> Vec3 {
        self.floor_normal
    }
    pub fn wall_normal(&self) -> Vec3 {
        self.wall_normal
    }
    /// The floor's angle from flat, radians (0 off the floor).
    pub fn floor_angle(&self) -> f32 {
        if self.on_floor { angle(self.floor_normal, self.settings.up) } else { 0.0 }
    }
    /// How fast it really moved in its last move (m/s).
    pub fn real_velocity(&self) -> Vec3 {
        self.real_velocity
    }
    pub fn last_motion(&self) -> Vec3 {
        self.last_motion
    }
    /// What it ran into in its last move.
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    /// How far its shape reaches below its centre (to its feet).
    pub fn half_height(&self) -> f32 {
        let aabb = self.shape.compute_local_aabb();
        -aabb.mins.y
    }

    /// How far its shape reaches sideways from its centre.
    pub fn half_width(&self) -> f32 {
        let aabb = self.shape.compute_local_aabb();
        aabb.maxs.x.max(aabb.maxs.z)
    }

    /// Puts it somewhere (no sweep), off the floor until its next move says.
    pub fn set_position(&mut self, world: &mut World, pos: Vec3) {
        self.pos = pos;
        if let Some(b) = world.w.bodies.get_mut(self.body.0) {
            b.set_position(rpose(pos, Quat::IDENTITY), true);
        }
        self.place(world);
    }

    /// The collider where the character is, in the query structures now.
    fn place(&mut self, world: &mut World) {
        let pose = rpose(self.pos, Quat::IDENTITY);
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

    /// Godot's `move_and_slide`: moves by `velocity × dt`, sliding along
    /// what it meets, and returns the velocity left (what Godot writes back
    /// to `velocity`).
    pub fn move_and_slide(&mut self, world: &mut World, velocity: Vec3, dt: f32) -> Vec3 {
        self.move_and_slide_past(world, velocity, dt, &[])
    }

    /// [`Character::move_and_slide`], passing through the bodies in `past`
    /// (their [`BodyId::to_bits`], sorted): a crowd's agents, which keep apart by themselves.
    pub fn move_and_slide_past(&mut self, world: &mut World, mut velocity: Vec3, dt: f32, past: &[u64]) -> Vec3 {
        let s = self.settings;
        let up = s.up.normalize();
        let was_on_floor = self.on_floor;
        let facing_up = velocity.dot(up) > 0.0;
        let start = self.pos;
        let prev_floor = self.floor_normal;
        (self.on_floor, self.on_wall, self.on_ceiling) = (false, false, false);
        (self.floor_normal, self.wall_normal, self.ceiling_normal) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        self.hits.clear();
        if !(dt > 0.0) || !finite(velocity) {
            self.real_velocity = Vec3::ZERO;
            self.last_motion = Vec3::ZERO;
            return if finite(velocity) { velocity } else { Vec3::ZERO };
        }
        let kcc = KinematicCharacterController {
            up: rv(up),
            offset: CharacterLength::Absolute(OFFSET),
            slide: true,
            autostep: (s.max_step_height > 0.0).then(|| CharacterAutostep {
                max_height: CharacterLength::Absolute(s.max_step_height),
                // Far enough onto the step that its round bottom stands on it.
                min_width: CharacterLength::Absolute(self.half_width() + 0.05),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: s.floor_max_angle,
            min_slope_slide_angle: if s.floor_stop_on_slope { s.floor_max_angle } else { 0.0 },
            snap_to_ground: None,
            normal_nudge_factor: 1.0e-4,
        };
        let me = self.body.0;
        let mask = s.mask;
        let pred = move |_h: rp::ColliderHandle, c: &rp::Collider| {
            !c.is_sensor() && c.parent() != Some(me) && c.collision_groups().memberships.bits() & mask != 0 && c.parent().is_none_or(|p| past.binary_search(&BodyId(p).to_bits()).is_err())
        };
        let filter = rp::QueryFilter::new().predicate(&pred);
        let shape = &*self.shape;
        let mut raw: Vec<(rp::ColliderHandle, RVec, RVec)> = Vec::new();
        // On the floor, the fall (gravity a game adds every frame) isn't
        // swept into it: Rapier's sweep along a floor it already touches can
        // stall for a frame. The floor snap below holds it down instead.
        let mut motion = velocity * dt;
        // Godot's `floor_stop_on_slope`: off, standing on a slope with no
        // move across it, the fall goes down along the floor (Rapier slides
        // it), so it slides.
        let slides = !s.floor_stop_on_slope && (velocity - up * velocity.dot(up)).length() < 1e-3;
        if was_on_floor && motion.dot(up) < 0.0 && !slides {
            motion = flat(motion, up);
        }
        let (mut moved, mut grounded) = {
            let qp = world.w.query_pipeline_with_filter(filter);
            let r = kcc.move_shape(dt, &qp, shape, &rpose(self.pos, Quat::IDENTITY), rv(motion), |c| raw.push((c.handle, c.hit.normal1, c.hit.witness1)));
            (sv(r.translation), r.grounded && !r.is_sliding_down_slope)
        };
        // The crease between a wall and the floor. Rapier won't climb along
        // a wall, so where the way round one (a tree trunk) goes up a
        // walkable slope its sweep goes back and forth between the two and
        // stops. What's left of the move goes along the line where they
        // meet instead, which climbs no steeper than the floor.
        if !facing_up {
            let kind = |n: Vec3| if angle(n, up) <= s.floor_max_angle + ANGLE_THRESHOLD { 1 } else if angle(n, -up) <= s.floor_max_angle + ANGLE_THRESHOLD { 2 } else { 0 };
            let normals = raw.iter().map(|h| sv(h.1).normalize());
            let floor_n = normals.clone().filter(|n| kind(*n) == 1).last().or((was_on_floor && prev_floor != Vec3::ZERO).then_some(prev_floor));
            let wall_n = normals.filter(|n| kind(*n) == 0).last();
            if let (Some(f), Some(wl)) = (floor_n, wall_n) {
                let crease = wl.cross(f).normalize();
                let along = crease * crease.dot(flat(motion - moved, up));
                if along.length() > 1e-4 {
                    let qp = world.w.query_pipeline_with_filter(rp::QueryFilter::new().predicate(&pred));
                    let r = kcc.move_shape(dt, &qp, shape, &rpose(self.pos + moved, Quat::IDENTITY), rv(along), |c| raw.push((c.handle, c.hit.normal1, c.hit.witness1)));
                    moved += sv(r.translation);
                    grounded |= r.grounded && !r.is_sliding_down_slope;
                }
            }
        }
        let mut pos = self.pos + moved;
        // Floor, wall or ceiling, by Godot's angles.
        for (h, n, p) in &raw {
            let n = sv(*n).normalize();
            let body = world.w.colliders.get(*h).and_then(|c| c.parent()).map(BodyId);
            self.hits.push(Hit { body, normal: n, point: sv(*p) });
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
        // Holding on to the floor (Godot's _snap_on_floor), or touching it.
        let cast = |world: &World, from: Vec3, dist: f32| -> Option<(f32, Vec3)> {
            let qp = world.w.query_pipeline_with_filter(rp::QueryFilter::new().predicate(&pred));
            let opts = ShapeCastOptions { target_distance: OFFSET, stop_at_penetration: false, max_time_of_impact: dist, compute_impact_geometry_on_penetration: true };
            qp.cast_shape(&rpose(from, Quat::IDENTITY), rv(-up), shape, opts).map(|(_, h)| (h.time_of_impact, sv(h.normal1).normalize()))
        };
        // Held on to the floor it was on (Godot's _snap_on_floor), or
        // standing on ground Rapier found within a few centimetres: either
        // way a short cast down says what it is (a slope too steep isn't).
        let reach = if was_on_floor { s.floor_snap_length.max(OFFSET * 2.0) } else if grounded { OFFSET * 6.0 } else { 0.0 };
        if !self.on_floor
            && !facing_up
            && reach > 0.0
            && let Some((toi, n)) = cast(world, pos, reach)
            && angle(n, up) <= s.floor_max_angle + ANGLE_THRESHOLD
        {
            pos -= up * toi;
            self.on_floor = true;
            self.floor_normal = n;
        }
        // The velocity left (Godot's rules).
        if self.on_wall && (velocity - up * velocity.dot(up)).dot(self.wall_normal) < 0.0 {
            let forward = flat(self.wall_normal, up).normalize();
            if was_on_floor && s.wall_min_slide_angle > 0.0 {
                let horizontal = flat(velocity, up).normalize();
                if horizontal != Vec3::ZERO && libm::acosf((-forward.dot(horizontal)).clamp(-1.0, 1.0)) < s.wall_min_slide_angle {
                    velocity = up * velocity.dot(up);
                }
            }
            if s.floor_block_on_wall && forward != Vec3::ZERO {
                velocity = if facing_up { up * up.dot(velocity) + flat(slide(velocity, self.wall_normal), up) } else { slide(velocity, forward) };
            } else {
                velocity = slide(velocity, self.wall_normal);
            }
        }
        if self.on_ceiling && facing_up {
            velocity = if s.slide_on_ceiling { slide(velocity, self.ceiling_normal) } else { flat(velocity, up) };
        }
        if self.on_floor && !facing_up {
            velocity = flat(velocity, up);
        }
        self.pos = pos;
        self.last_motion = pos - start;
        self.real_velocity = self.last_motion / dt;
        self.place(world);
        velocity
    }

    /// Takes it out of the world.
    pub fn remove(self, world: &mut World) {
        world.remove(self.body);
    }
}

fn finite(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

fn angle(a: Vec3, b: Vec3) -> f32 {
    libm::acosf(a.dot(b).clamp(-1.0, 1.0))
}

/// `v` without its part along unit `n` (Godot's `slide`).
fn slide(v: Vec3, n: Vec3) -> Vec3 {
    v - n * v.dot(n)
}

/// `v` without its part along `up`.
fn flat(v: Vec3, up: Vec3) -> Vec3 {
    slide(v, up)
}
