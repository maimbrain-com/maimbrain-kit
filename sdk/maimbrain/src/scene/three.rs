//! The retained scene's 3D side (docs/STUDIO-2.md §4.1, §4.3): nodes that a
//! game makes once and changes by property (`x`, `y`, `z`, `rx`, `ry`,
//! `rz`, `sx`, `sy`, `sz`, `visible`), which Rust keeps, moves by their
//! velocity, gravity and spin, and sends to mb3d once an update: a write changes
//! the node here and marks it, and [`Scene3::flush`] at the end of the
//! update makes one host call for each node that changed, not one per write
//! (mb3d 2), or [`Scene3::flush_batched`] one call for all of them (mb3d 3's
//! `mb3d_node_transforms`, §4.3).
//!
//! ```ignore
//! use maimbrain::scene::three::Scene3;
//! let mut scene = Scene3::new();
//! let crate_ = scene.add(gfx3d::Node::with_mesh(mesh, material), true, None).unwrap();
//! scene.node_mut(crate_).unwrap().y = 0.5;
//! // end of update: scene.step(dt); scene.flush();
//! ```
//!
//! - **Space.** Meters, +y up, right-handed. A node's place, turn and size
//!   are in its parent's space.
//! - **Turns** are Euler angles in radians applied y, then x, then z (yaw,
//!   pitch, roll), as engine 1.0's `turn` and [`Quat::from_euler`] have them.
//! - **Gravity** pulls down: `gravity = 9.8` falls, as a 2D node's positive
//!   gravity falls down the screen.
//! - **Host nodes.** A node stands for an mb3d node ([`gfx3d::Node`]): one
//!   the scene made (`owned`, destroyed with the node or the scene), or one
//!   inside a spawned model (a bone, a part), found by name, which the model
//!   owns. A part's transform is the model's until the game writes it.
//!
//! Ids carry a generation, as the 2D scene's do.

use crate::gfx3d::{self, Mat4, Quat, Transform, Vec3, vec3};

use super::{Full, MAX_NODES};

/// A 3D node: its slot and generation. 0 is never an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node3Id(u32);

impl Node3Id {
    fn new(slot: usize, gen_: u32) -> Node3Id {
        Node3Id((gen_ << 12) | slot as u32)
    }
    fn slot(self) -> usize {
        (self.0 & 0xfff) as usize
    }
    pub fn to_bits(self) -> u32 {
        self.0
    }
    pub fn from_bits(b: u32) -> Node3Id {
        Node3Id(b)
    }
}

/// What changed since the last flush.
const MOVED: u8 = 1;

/// One 3D node. Fields are public: the binding that changes one checks the
/// value first, and changes it through [`Scene3::node_mut`], which marks it.
#[derive(Clone, Debug)]
pub struct Node3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Radians, applied y, then x, then z.
    pub rx: f32,
    pub ry: f32,
    pub rz: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    /// Meters a second, and meters a second² pulling down on `vy`.
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub gravity: f32,
    /// Radians a second onto `rx`, `ry`, `rz` (a spin).
    pub vrx: f32,
    pub vry: f32,
    pub vrz: f32,
    pub visible: bool,
    /// The mb3d node it stands for (none: a node only Rust knows).
    pub host: Option<gfx3d::Node>,
    /// The scene made `host` (it destroys it); else a model's part.
    pub owned: bool,
    parent: Option<Node3Id>,
    children: Vec<Node3Id>,
    dirty: u8,
    /// The visibility mb3d has.
    sent_visible: bool,
}

impl Node3 {
    fn new(host: Option<gfx3d::Node>, owned: bool) -> Node3 {
        Node3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rx: 0.0,
            ry: 0.0,
            rz: 0.0,
            sx: 1.0,
            sy: 1.0,
            sz: 1.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            gravity: 0.0,
            vrx: 0.0,
            vry: 0.0,
            vrz: 0.0,
            visible: true,
            host,
            owned,
            parent: None,
            children: Vec::new(),
            dirty: 0,
            sent_visible: true,
        }
    }

    pub fn parent(&self) -> Option<Node3Id> {
        self.parent
    }

    pub fn children(&self) -> &[Node3Id] {
        &self.children
    }

    /// Its turn as a rotation.
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(self.ry, self.rx, self.rz)
    }

    /// Its place, turn and size in its parent's space.
    pub fn transform(&self) -> Transform {
        Transform { pos: vec3(self.x, self.y, self.z), rot: self.rotation(), scale: vec3(self.sx, self.sy, self.sz) }
    }
}

struct Slot {
    gen_: u32,
    node: Option<Node3>,
    /// When its node was made (the scene's count then), for [`Scene3::ids_made`].
    seq: u64,
}

/// Every 3D node a game has.
#[derive(Default)]
pub struct Scene3 {
    slots: Vec<Slot>,
    free: Vec<usize>,
    live: usize,
    /// Nodes made so far (each slot's `seq`).
    seq: u64,
    /// [`Scene3::flush_batched`]'s buffer, kept between frames.
    batch: Vec<gfx3d::NodeTransform>,
}

impl Scene3 {
    pub fn new() -> Scene3 {
        Scene3::default()
    }

    /// Live nodes.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// A node for `host` (made by the caller, at the root or where it put
    /// it), under `parent` in this scene. A model's part (`owned` false)
    /// keeps the host parent it has; give it the node it was found under.
    pub fn add(&mut self, host: Option<gfx3d::Node>, owned: bool, parent: Option<Node3Id>) -> Result<Node3Id, Full> {
        let slot = match self.free.pop() {
            Some(s) => s,
            None if self.slots.len() < MAX_NODES => {
                self.slots.push(Slot { gen_: 0, node: None, seq: 0 });
                self.slots.len() - 1
            }
            None => return Err(Full),
        };
        let s = &mut self.slots[slot];
        s.gen_ = (s.gen_ + 1) & 0x7_ffff;
        if s.gen_ == 0 {
            s.gen_ = 1;
        }
        s.node = Some(Node3::new(host, owned));
        self.seq += 1;
        s.seq = self.seq;
        let id = Node3Id::new(slot, s.gen_);
        self.live += 1;
        if let Some(p) = parent.filter(|p| self.alive(*p)) {
            self.slots[p.slot()].node.as_mut().unwrap().children.push(id);
            self.slots[slot].node.as_mut().unwrap().parent = Some(p);
        }
        Ok(id)
    }

    pub fn get(&self, id: Node3Id) -> Option<&Node3> {
        let s = self.slots.get(id.slot())?;
        if Node3Id::new(id.slot(), s.gen_) != id {
            return None;
        }
        s.node.as_ref()
    }

    /// The node, to change its place, turn or size: its whole transform is
    /// sent to mb3d at the next flush (a model's part's from then on: the
    /// pose mb3d had for it is replaced, as engine 1.0's writes did). For
    /// visibility, velocity and spin, [`Scene3::set_visible`],
    /// [`Scene3::set_motion`] and [`Scene3::set_spin`] send no transform.
    pub fn node_mut(&mut self, id: Node3Id) -> Option<&mut Node3> {
        let n = self.slot_mut(id)?;
        n.dirty |= MOVED;
        Some(n)
    }

    /// Shows or hides a node. Only its visibility goes to mb3d: a model's
    /// part keeps the pose mb3d has for it.
    pub fn set_visible(&mut self, id: Node3Id, on: bool) -> bool {
        let Some(n) = self.slot_mut(id) else { return false };
        n.visible = on;
        true
    }

    /// Sets a node's velocity and gravity (`None` leaves one as it is).
    /// That moves nothing yet: [`Scene3::step`] moves it, and marks it then.
    pub fn set_motion(&mut self, id: Node3Id, v: [Option<f32>; 3], gravity: Option<f32>) -> bool {
        let Some(n) = self.slot_mut(id) else { return false };
        for (slot, x) in [&mut n.vx, &mut n.vy, &mut n.vz].into_iter().zip(v) {
            if let Some(x) = x {
                *slot = x;
            }
        }
        if let Some(g) = gravity {
            n.gravity = g;
        }
        true
    }

    /// Sets a node's spin about x, y and z, radians a second (`None` leaves
    /// one as it is). As [`Scene3::set_motion`], [`Scene3::step`] turns it.
    pub fn set_spin(&mut self, id: Node3Id, v: [Option<f32>; 3]) -> bool {
        let Some(n) = self.slot_mut(id) else { return false };
        for (slot, x) in [&mut n.vrx, &mut n.vry, &mut n.vrz].into_iter().zip(v) {
            if let Some(x) = x {
                *slot = x;
            }
        }
        true
    }

    /// The node, unmarked.
    fn slot_mut(&mut self, id: Node3Id) -> Option<&mut Node3> {
        let s = self.slots.get_mut(id.slot())?;
        if Node3Id::new(id.slot(), s.gen_) != id {
            return None;
        }
        s.node.as_mut()
    }

    pub fn alive(&self, id: Node3Id) -> bool {
        self.get(id).is_some()
    }

    /// Moves a node under `parent` (None: the root), keeping its own place,
    /// turn and size, now in the new parent's space (engine 1.0's
    /// `n:parent(p)`). Refused (false) if `parent` is the node or under it.
    pub fn set_parent(&mut self, id: Node3Id, parent: Option<Node3Id>) -> bool {
        if !self.alive(id) || parent.is_some_and(|p| !self.alive(p)) {
            return false;
        }
        let mut up = parent;
        while let Some(p) = up {
            if p == id {
                return false;
            }
            up = self.get(p).and_then(|n| n.parent);
        }
        if let Some(old) = self.get(id).and_then(|n| n.parent)
            && let Some(o) = self.slots[old.slot()].node.as_mut()
        {
            o.children.retain(|c| *c != id);
        }
        if let Some(p) = parent {
            self.slots[p.slot()].node.as_mut().unwrap().children.push(id);
        }
        let host_parent = parent.and_then(|p| self.get(p)).and_then(|p| p.host);
        let n = self.slots[id.slot()].node.as_mut().unwrap();
        n.parent = parent;
        if let Some(h) = n.host {
            h.set_parent(host_parent);
        }
        true
    }

    /// Destroys a node and everything under it (mb3d's own children go with
    /// an owned host node); returns the ids that went, the node first.
    pub fn destroy(&mut self, id: Node3Id) -> Vec<Node3Id> {
        let mut gone = Vec::new();
        let Some(n) = self.get(id) else { return gone };
        if let Some(p) = n.parent
            && let Some(pn) = self.slots[p.slot()].node.as_mut()
        {
            pn.children.retain(|c| *c != id);
        }
        let mut stack = vec![id];
        let mut first = true;
        while let Some(n) = stack.pop() {
            let slot = n.slot();
            if let Some(node) = self.slots[slot].node.take() {
                // Only the top host node: mb3d takes its children with it.
                if first
                    && node.owned
                    && let Some(h) = node.host
                {
                    h.destroy();
                }
                first = false;
                stack.extend(node.children.iter().rev());
                self.free.push(slot);
                self.live -= 1;
                gone.push(n);
            }
        }
        gone
    }

    /// Every live node's id, by slot.
    pub fn ids(&self) -> Vec<Node3Id> {
        self.slots.iter().enumerate().filter(|(_, s)| s.node.is_some()).map(|(i, s)| Node3Id::new(i, s.gen_)).collect()
    }

    /// Every live node's id, in the order they were made (a node made into
    /// a freed slot comes after the older ones), as the 2D scene's are.
    pub fn ids_made(&self) -> Vec<Node3Id> {
        let mut live: Vec<(u64, Node3Id)> = self.slots.iter().enumerate().filter(|(_, s)| s.node.is_some()).map(|(i, s)| (s.seq, Node3Id::new(i, s.gen_))).collect();
        live.sort_unstable_by_key(|(seq, _)| *seq);
        live.into_iter().map(|(_, id)| id).collect()
    }

    /// One update's motion: velocities, gravity and spins, by slot. A spun
    /// angle stays in −π..π ([`super::wrap_angle`]).
    pub fn step(&mut self, dt: f32) {
        for s in self.slots.iter_mut() {
            let Some(n) = s.node.as_mut() else { continue };
            if n.vx != 0.0 || n.vy != 0.0 || n.vz != 0.0 || n.gravity != 0.0 {
                n.vy -= n.gravity * dt;
                n.x += n.vx * dt;
                n.y += n.vy * dt;
                n.z += n.vz * dt;
                n.dirty |= MOVED;
            }
            if n.vrx != 0.0 || n.vry != 0.0 || n.vrz != 0.0 {
                for (a, v) in [(&mut n.rx, n.vrx), (&mut n.ry, n.vry), (&mut n.rz, n.vrz)] {
                    if v != 0.0 {
                        *a = super::wrap_angle(*a + v * dt);
                    }
                }
                n.dirty |= MOVED;
            }
        }
    }

    /// Sends what changed to mb3d: a transform for each node moved since
    /// the last flush, and its visibility when that changed. Returns how
    /// many host calls it made.
    pub fn flush(&mut self) -> usize {
        let mut calls = 0;
        for s in self.slots.iter_mut() {
            let Some(n) = s.node.as_mut() else { continue };
            let Some(h) = n.host else {
                n.dirty = 0;
                continue;
            };
            if n.dirty & MOVED != 0 {
                h.set_transform(&n.transform());
                calls += 1;
            }
            if n.visible != n.sent_visible {
                h.set_visible(n.visible);
                n.sent_visible = n.visible;
                calls += 1;
            }
            n.dirty = 0;
        }
        calls
    }

    /// [`Scene3::flush`] with the transforms in one host call
    /// (`mb3d_node_transforms`, mb3d 3: the game declares `mb3d = 3`).
    /// Visibility changes, which are rare, are still one call each, after
    /// it. Returns how many nodes' transforms and visibilities it sent, as
    /// `flush` does.
    pub fn flush_batched(&mut self) -> usize {
        let mut batch = std::mem::take(&mut self.batch);
        batch.clear();
        let mut shown = Vec::new();
        for s in self.slots.iter_mut() {
            let Some(n) = s.node.as_mut() else { continue };
            let Some(h) = n.host else {
                n.dirty = 0;
                continue;
            };
            if n.dirty & MOVED != 0 {
                batch.push(gfx3d::NodeTransform::new(h, n.transform()));
            }
            if n.visible != n.sent_visible {
                shown.push((h, n.visible));
                n.sent_visible = n.visible;
            }
            n.dirty = 0;
        }
        gfx3d::Node::set_transforms(&batch);
        for (h, on) in &shown {
            h.set_visible(*on);
        }
        let sent = batch.len() + shown.len();
        self.batch = batch;
        sent
    }

    /// Nodes waiting for the next flush.
    pub fn pending(&self) -> usize {
        self.slots.iter().filter_map(|s| s.node.as_ref()).filter(|n| n.host.is_some() && (n.dirty != 0 || n.visible != n.sent_visible)).count()
    }

    /// The node's transform to the world, when the scene knows it: the node
    /// and everything above it are its own (a model's part moves with the
    /// model's animation, which only mb3d knows).
    pub fn world(&self, id: Node3Id) -> Option<Mat4> {
        let mut m = Mat4::IDENTITY;
        let mut at = Some(id);
        while let Some(i) = at {
            let n = self.get(i)?;
            if !n.owned {
                return None;
            }
            m = n.transform().matrix() * m;
            at = n.parent;
        }
        Some(m)
    }

    /// Where the node is in the world: from the scene when it knows (see
    /// [`Scene3::world`]), else mb3d's answer after a flush.
    pub fn world_position(&mut self, id: Node3Id) -> Option<Vec3> {
        if let Some(m) = self.world(id) {
            return Some(vec3(m.0[12], m.0[13], m.0[14]));
        }
        let h = self.get(id)?.host?;
        self.flush();
        let m = h.world()?;
        Some(vec3(m.0[12], m.0[13], m.0[14]))
    }
}

impl Drop for Scene3 {
    /// Its own host nodes go with it, top ones only (mb3d takes their
    /// children), newest first.
    fn drop(&mut self) {
        for s in self.slots.iter().rev() {
            if let Some(n) = &s.node
                && n.owned
                && n.parent.is_none_or(|p| !self.alive(p))
                && let Some(h) = n.host
            {
                h.destroy();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_mark_and_a_flush_sends_each_changed_node_once() {
        let mut s = Scene3::new();
        let a = s.add(gfx3d::Node::new(), true, None).unwrap();
        let b = s.add(gfx3d::Node::new(), true, Some(a)).unwrap();
        s.flush();
        assert_eq!(s.pending(), 0);
        for _ in 0..5 {
            s.node_mut(a).unwrap().x += 1.0;
        }
        assert_eq!(s.pending(), 1);
        assert_eq!(s.flush(), 1, "one transform for five writes");
        s.node_mut(b).unwrap().visible = false;
        assert_eq!(s.flush(), 2, "b's transform and its visibility");
        assert_eq!(s.flush(), 0);
    }

    /// mb3d 3: the moved nodes go in one call, recorded as one transform
    /// each, so a batched frame draws exactly what per-node calls drew.
    #[test]
    fn a_batched_flush_sends_the_same_transforms() {
        use crate::sys::test::Host;
        struct G(Scene3, bool);
        impl crate::Game for G {
            fn init() -> G {
                G(Scene3::new(), false)
            }
            fn update(&mut self, _: f32) {
                let s = &mut self.0;
                let a = s.add(gfx3d::Node::new(), true, None).unwrap();
                let b = s.add(gfx3d::Node::new(), true, Some(a)).unwrap();
                s.node_mut(a).unwrap().x = 2.0;
                s.node_mut(b).unwrap().ry = 0.5;
                s.set_visible(b, false);
                let sent = if self.1 { s.flush_batched() } else { s.flush() };
                assert_eq!((sent, s.pending()), (3, 0));
            }
            fn render(&self) {}
        }
        let frame = |batched: bool| {
            let host = Host::start(1);
            let mut g: G = host.init();
            g.1 = batched;
            host.frame(&mut g, 1.0 / 60.0);
            host.drawn()
        };
        let one = frame(true);
        assert_eq!(one.len(), 1);
        assert_eq!(one, frame(false));
    }

    /// Hiding, showing or setting the velocity of a model's part sends no
    /// transform: its pose is mb3d's until a place, turn or size is written.
    #[test]
    fn a_parts_visibility_and_velocity_send_no_transform() {
        let mut s = Scene3::new();
        let root = s.add(gfx3d::Node::new(), true, None).unwrap();
        let part = s.add(gfx3d::Node::new(), false, Some(root)).unwrap();
        assert!(s.set_visible(part, false));
        assert!(s.set_motion(part, [Some(0.0), None, Some(0.0)], Some(0.0)));
        assert_eq!(s.pending(), 1);
        assert_eq!(s.flush(), 1, "the visibility only");
        s.step(0.1);
        assert_eq!(s.flush(), 0, "still: nothing moved it");
        s.set_motion(part, [Some(1.0), None, None], None);
        s.step(0.1);
        assert_eq!(s.flush(), 1, "moving, it's sent");
    }

    #[test]
    fn velocity_and_gravity_move_and_world_composes_parents() {
        let mut s = Scene3::new();
        let a = s.add(None, true, None).unwrap();
        let b = s.add(None, true, Some(a)).unwrap();
        {
            let n = s.node_mut(a).unwrap();
            (n.x, n.ry, n.vz, n.gravity) = (2.0, std::f32::consts::FRAC_PI_2, -1.0, 10.0);
        }
        s.node_mut(b).unwrap().z = -1.0;
        s.step(0.5);
        let n = s.get(a).unwrap();
        assert_eq!((n.z, n.vy, n.y), (-0.5, -5.0, -2.5));
        // b is 1 m along a's −z, and a faces −x after a quarter turn left.
        let w = s.world(b).unwrap();
        assert!((w.0[12] - 1.0).abs() < 1e-5 && (w.0[13] + 2.5).abs() < 1e-5 && (w.0[14] + 0.5).abs() < 1e-5, "{:?}", &w.0[12..15]);
    }

    /// A spin turns the node each step, sends its transform like a move,
    /// keeps the angles in −π..π, and a part's spin of 0 sends nothing.
    #[test]
    fn spin_turns_and_sends_like_a_move() {
        use std::f32::consts::PI;
        let mut s = Scene3::new();
        let a = s.add(gfx3d::Node::new(), true, None).unwrap();
        let part = s.add(gfx3d::Node::new(), false, Some(a)).unwrap();
        s.flush();
        assert!(s.set_spin(part, [Some(0.0), Some(0.0), None]));
        assert!(s.set_spin(a, [None, Some(PI), Some(-2.0)]));
        assert_eq!(s.pending(), 0, "setting a spin sends nothing");
        s.step(0.5);
        let n = s.get(a).unwrap();
        assert_eq!((n.rx, n.ry, n.rz), (0.0, PI / 2.0, -1.0));
        assert_eq!(s.flush(), 1, "the spinning node only");
        s.step(1.0);
        let n = s.get(a).unwrap();
        assert!((n.ry + PI / 2.0).abs() < 1e-5 && n.rz == -3.0, "ry wrapped past π: {} {}", n.ry, n.rz);
        s.step(1.0);
        let n = s.get(a).unwrap();
        assert!((n.ry - PI / 2.0).abs() < 1e-5 && (n.rz - (2.0 * PI - 5.0)).abs() < 1e-5, "rz wrapped past −π: {} {}", n.ry, n.rz);
    }

    #[test]
    fn destroying_takes_the_children_and_old_ids_stay_dead() {
        let mut s = Scene3::new();
        let a = s.add(None, true, None).unwrap();
        let b = s.add(None, true, Some(a)).unwrap();
        let part = s.add(None, false, Some(b)).unwrap();
        assert!(s.world(part).is_none(), "a model's part is mb3d's to place");
        assert!(!s.set_parent(a, Some(b)), "not under its own child");
        assert_eq!(s.destroy(a), [a, b, part]);
        assert!(s.is_empty() && !s.alive(b));
        let c = s.add(None, true, None).unwrap();
        assert_ne!(c, a);
        assert!(s.get(a).is_none());
    }
}
