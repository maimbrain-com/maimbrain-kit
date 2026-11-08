//! Navigation in a round (docs/STUDIO-2.md §4.14, §5 [godot]; §9.4 row 3b;
//! Decided 45–47): Godot's `NavigationRegion3D` and `NavigationAgent3D`
//! over the SDK's `nav` (Recast, Detour and DetourCrowd). The Lua API is
//! api/nav3d.rs; this is what a round keeps and what runs each frame, in
//! [`World3::nav_step`], after the velocities and before physics:
//!
//! 1. **The bake.** A region gathers its level the first frame after it's
//!    made (so after `love.load` returns): the terrain's true heights inside
//!    its square and the static bodies (its children, or every
//!    `StaticBody3D` if it has none), and queues its 16 m tiles. Then a few
//!    tiles a frame ([`FIRST_BUDGET`]; a rebake after `region:bake()`,
//!    [`REBAKE_BUDGET`]), never all in one callback. The title card waits
//!    for it, as it waits for the terrain (`Vm::ground_ready`). A region
//!    the title card's attract mode baked is kept for the round that makes
//!    the same one ([`take_region`]), as terrains are.
//! 2. **The crowd.** Each agent is a DetourCrowd agent at its body's feet;
//!    one `dtCrowd::update` a frame by the recorded `dt`.
//! 3. **`autoMove`.** The crowd's velocity (across) and gravity (down) go
//!    to the parent `CharacterBody3D`'s `moveAndSlide()`; the agent is put
//!    back where the body ended, and the body turns to face its way. A
//!    crowd of enemies costs no Lua.
//! 4. **Godot's rules** for the agent (ported from Godot 4.3's
//!    scene/3d/navigation_agent_3d.cpp, MIT, Copyright (c) 2014-present
//!    Godot Engine contributors, (c) 2007-2014 Juan Linietsky, Ariel
//!    Manzur): the target is reached within `targetDesiredDistance` of
//!    the body's origin, then `targetReached` and `navigationFinished` fire
//!    once and it stops; `getNextPathPosition` is the next corner more than
//!    `pathDesiredDistance` away.

use std::collections::BTreeMap;

use maimbrain::gfx3d::{Vec3, vec3};
use maimbrain::nav::{self, AgentId, AgentSettings, Geometry, Region, Settings, Solid};
use maimbrain::scene::three::Node3Id;

use crate::physics3::{Geom3, trs_of};
use crate::three::{Kind3, World3};

/// Tiles baked a frame the first time (~1 ms each on a 16 Pro, S10).
pub const FIRST_BUDGET: usize = 4;
/// Tiles a frame when `region:bake()` builds again during play.
pub const REBAKE_BUDGET: usize = 1;
/// A region's side when it says none and the terrain has no `size` (m).
pub const DEFAULT_SIZE: f32 = 64.0;

/// A `NavigationRegion3D`.
pub struct Region3 {
    pub settings: Settings,
    /// Its square's side (m), around its node; None: the terrain's size, or [`DEFAULT_SIZE`].
    pub size: Option<f32>,
    pub region: Option<Region>,
    /// The session key of what it baked (settings, square and level).
    pub key: u64,
    /// Asked to bake (made, or `region:bake()`): gathers the level next frame.
    pub wanted: bool,
    /// Its first bake is done (so later ones go at the rebake budget).
    pub baked_once: bool,
    pub was_baked: bool,
}

impl Region3 {
    pub fn new(settings: Settings, size: Option<f32>) -> Region3 {
        Region3 { settings, size, region: None, key: 0, wanted: true, baked_once: false, was_baked: false }
    }

    pub fn is_baked(&self) -> bool {
        !self.wanted && self.region.as_ref().is_some_and(|r| r.is_baked())
    }
}

/// A `NavigationAgent3D`.
pub struct Agent3 {
    pub settings: AgentSettings,
    pub path_desired: f32,
    pub target_desired: f32,
    pub auto_move: bool,
    pub id: Option<(u32, AgentId)>,
    pub target: Option<Vec3>,
    /// The target changed since the crowd heard it.
    pub fresh: bool,
    pub reached: bool,
    pub finished: bool,
    pub state: nav::Agent,
}

impl Agent3 {
    pub fn new() -> Agent3 {
        Agent3 {
            // Godot's defaults, but avoidance on (Decided 47) and a slower walk.
            settings: AgentSettings { radius: 0.5, height: 1.0, max_speed: 5.0, max_accel: 20.0, avoidance: true },
            path_desired: 1.0,
            target_desired: 1.0,
            auto_move: false,
            id: None,
            target: None,
            fresh: false,
            reached: false,
            finished: true,
            state: nav::Agent::default(),
        }
    }
}

impl Default for Agent3 {
    fn default() -> Agent3 {
        Agent3::new()
    }
}

#[derive(Default)]
pub struct Nav3 {
    pub regions: BTreeMap<u32, Region3>,
    pub agents: BTreeMap<u32, Agent3>,
    /// Region nodes destroyed, whose bakes go back to the session.
    pub kept: Vec<Region3>,
}

thread_local! {
    /// Bakes a round left, by key, for the next round that makes the same
    /// region (the title card's attract mode bakes it; the round takes it).
    static NAVS: std::cell::RefCell<Vec<(u64, Region)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn new_session() {
    NAVS.with(|n| n.borrow_mut().clear());
}

/// A bake kept for `key`, if a round left one.
fn take_region(key: u64) -> Option<Region> {
    NAVS.with(|n| {
        let mut n = n.borrow_mut();
        let i = n.iter().position(|(k, _)| *k == key)?;
        Some(n.remove(i).1)
    })
}

/// A round's baked region, kept (without its agents) for the next round.
pub fn keep_region(r: &mut Region3) {
    if let Some(mut reg) = r.region.take()
        && reg.is_baked()
    {
        reg.clear_agents();
        NAVS.with(|n| {
            let mut n = n.borrow_mut();
            n.retain(|(k, _)| *k != r.key);
            n.push((r.key, reg));
            // A few at most: a live update that changes the level leaves old ones.
            while n.len() > 4 {
                n.remove(0);
            }
        });
    }
}

fn mix(h: u64, x: u32) -> u64 {
    (h ^ x as u64).wrapping_mul(0x100000001b3)
}

/// A convex outline (from above) of points, for a solid.
fn hull(points: &[Vec3]) -> Vec<[f32; 2]> {
    let mut p: Vec<[f32; 2]> = points.iter().map(|v| [v.x, v.z]).collect();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4);
    if p.len() < 3 {
        return Vec::new();
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *q) <= 0.0 {
            lower.pop();
        }
        lower.push(*q);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *q) <= 0.0 {
            upper.pop();
        }
        upper.push(*q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    // Recast takes 64 at most: keep every other until it fits.
    while lower.len() > 64 {
        lower = lower.into_iter().step_by(2).collect();
    }
    lower
}

impl World3 {
    pub fn nav(&mut self) -> &mut Nav3 {
        self.nav.get_or_insert_with(Default::default)
    }

    /// Whether every region has baked (the title card waits for them).
    pub fn nav_ready(&self) -> bool {
        self.nav.as_ref().is_none_or(|n| n.regions.values().all(|r| r.is_baked()))
    }

    /// The level a region bakes: the terrain inside its square, and static
    /// bodies (its children, or all of them).
    fn gather(&mut self, node: Node3Id, size: f32, cell_height: f32) -> (Geometry, Vec3, Vec3) {
        let m = self.scene.world(node).unwrap_or(maimbrain::gfx3d::Mat4::IDENTITY);
        let (c, _) = trs_of(&m);
        let half = size / 2.0;
        let (x0, z0) = (c.x - half, c.z - half);
        let mut g = Geometry::default();
        if let Some(ground) = &self.ground {
            let step = ground.terrain.layout().step(0).clamp(0.5, 2.0);
            let n = (size / step).ceil() as usize + 1;
            let mut hs = Vec::new();
            ground.terrain.heights().fill(x0, z0, step, n, &mut hs);
            g.add_heights(x0, z0, step, n, &hs);
        }
        // Static bodies: under the region if any are, else every one.
        let statics: Vec<Node3Id> = match &self.phys {
            Some(p) => p.bodies.keys().map(|k| Node3Id::from_bits(*k)).filter(|id| self.kind(*id) == Kind3::StaticBody && !p.bodies[&id.to_bits()].moving).collect(),
            None => Vec::new(),
        };
        let mine: Vec<Node3Id> = statics.iter().copied().filter(|id| self.is_under(*id, node)).collect();
        let list = if mine.is_empty() { statics } else { mine };
        for id in list {
            let p = self.phys.as_deref().unwrap();
            let b = &p.bodies[&id.to_bits()];
            let t = p.world.transform(b.id);
            for (_, s) in &b.shapes {
                let shape = match s.geom {
                    Geom3::Box { w, h, d } => maimbrain::physics3d::Shape::cuboid(vec3(w, h, d)),
                    Geom3::Sphere { r } => maimbrain::physics3d::Shape::sphere(r),
                    Geom3::Capsule { r, h } => maimbrain::physics3d::Shape::capsule(r, h),
                    Geom3::Cylinder { r, h } => maimbrain::physics3d::Shape::cylinder(r, h),
                    Geom3::Convex(_) | Geom3::Concave(_) => maimbrain::physics3d::Shape::sphere(0.01),
                };
                let mesh = match s.geom {
                    Geom3::Convex(i) | Geom3::Concave(i) => match self.meshes.get(i as usize) {
                        Some(m) => m.data.transformed(&maimbrain::gfx3d::Transform::at(s.pos).with_rot(s.rot).matrix()),
                        None => continue,
                    },
                    _ => match shape.at(s.pos).rotated(s.rot).mesh(12) {
                        Some(m) => m,
                        None => continue,
                    },
                };
                let world: Vec<Vec3> = mesh.positions.iter().map(|p| t.pos + t.rot * *p).collect();
                g.add_mesh(&world, &mesh.indices, |p| p);
                // Its inside isn't walkable (a closed trunk's floor under its
                // cap would be): an upright shape only, and below its top, so
                // its top (a platform) and a tilted one's slope (a ramp) stay
                // walkable. Recast nulls every span whose top is in y0..y1.
                let up = (t.rot * s.rot) * Vec3::Y;
                if !matches!(s.geom, Geom3::Concave(_)) && up.y >= 0.98 {
                    let outline = hull(&world);
                    let (y0, top) = world.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p.y), m.1.max(p.y)));
                    let y1 = top - 2.0 * cell_height;
                    if outline.len() >= 3 && y1 > y0 {
                        g.solids.push(Solid { outline, y0, y1 });
                    }
                }
            }
        }
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for y in g.verts.iter().skip(1).step_by(3) {
            lo = lo.min(*y);
            hi = hi.max(*y);
        }
        if lo > hi {
            (lo, hi) = (c.y - 1.0, c.y + 1.0);
        }
        (g, vec3(x0, lo - 1.0, z0), vec3(x0 + size, hi + 4.0, z0 + size))
    }

    /// Whether `id` is `top` or under it.
    fn is_under(&self, id: Node3Id, top: Node3Id) -> bool {
        let mut at = Some(id);
        while let Some(n) = at {
            if n == top {
                return true;
            }
            at = self.scene.get(n).and_then(|x| x.parent());
        }
        false
    }

    /// Where an agent stands: its parent character's feet, or its node.
    fn agent_feet(&self, id: Node3Id) -> (Option<Node3Id>, Vec3) {
        let parent = self.scene.get(id).and_then(|n| n.parent());
        if let Some(pn) = parent
            && let Some(c) = self.phys.as_deref().and_then(|p| p.chars.get(&pn.to_bits()))
        {
            return (Some(pn), c.ch.position() - Vec3::Y * c.ch.half_height());
        }
        let p = self.scene.world(id).map(|m| trs_of(&m).0).unwrap_or(Vec3::ZERO);
        (None, p)
    }

    /// One frame of navigation (the module's notes, 1–4).
    pub fn nav_step(&mut self, dt: f32) {
        // No character is an agent's until this frame's agents say so (one
        // whose agent went, or whose region went, stands on the ground's tiles again).
        // A character Lua moved (a respawn, a teleport) is put there now,
        // before its agent drives it (else the agent's move starts from where
        // it was, and undoes the write), and its agent goes with it.
        let mut moved: Vec<u32> = Vec::new();
        if let Some(p) = self.phys.as_deref_mut() {
            for (k, c) in p.chars.iter_mut() {
                c.agent = false;
                if self.nav.is_none() {
                    continue;
                }
                let Some(now) = self.scene.world(Node3Id::from_bits(*k)).map(|m| trs_of(&m).0) else { continue };
                if c.last.is_none_or(|l| l.distance(now) > 1e-4) {
                    c.ch.set_position(&mut p.world, now);
                    c.last = Some(now);
                    moved.push(*k);
                }
            }
        }
        let Some(nav) = self.nav.as_mut() else { return };
        for mut r in nav.kept.drain(..) {
            keep_region(&mut r);
        }
        // 1. The bake.
        let wanted: Vec<u32> = nav.regions.iter().filter(|(_, r)| r.wanted).map(|(k, _)| *k).collect();
        for k in wanted {
            let id = Node3Id::from_bits(k);
            let (settings, size) = {
                let r = &self.nav.as_ref().unwrap().regions[&k];
                (r.settings, r.size)
            };
            let size = size.or_else(|| self.ground.as_ref().and_then(|g| g.terrain.heights().half()).map(|h| h * 2.0)).unwrap_or(DEFAULT_SIZE);
            let (geom, lo, hi) = self.gather(id, size, settings.cell_height);
            let mut key = 0xcbf29ce484222325u64;
            for v in [settings.cell_size, settings.cell_height, settings.agent_radius, settings.agent_height, settings.max_climb, settings.max_slope, lo.x, lo.y, lo.z, hi.x, hi.y, hi.z] {
                key = mix(key, v.to_bits());
            }
            key = mix(key, geom.hash());
            let r = self.nav.as_mut().unwrap().regions.get_mut(&k).unwrap();
            r.wanted = false;
            if r.key == key && r.region.is_some() {
                continue;
            }
            r.key = key;
            // The same box as before: rebake the tiles that changed.
            if let Some(reg) = r.region.as_mut()
                && reg.bounds() == (lo, hi)
                && reg.settings() == &settings
            {
                reg.set_geometry(geom);
                continue;
            }
            match take_region(key) {
                Some(reg) => r.region = Some(reg),
                None => {
                    let mut reg = Region::new(&settings, lo, hi);
                    if let Some(reg) = reg.as_mut() {
                        reg.set_geometry(geom);
                    }
                    r.region = reg;
                }
            }
            // Its agents join the new navmesh.
            for a in self.nav.as_mut().unwrap().agents.values_mut() {
                if a.id.is_some_and(|(rk, _)| rk == k) {
                    a.id = None;
                    a.fresh = a.target.is_some();
                }
            }
        }
        let nav = self.nav.as_mut().unwrap();
        let mut finished = Vec::new();
        for (k, r) in nav.regions.iter_mut() {
            let budget = if r.baked_once { REBAKE_BUDGET } else { FIRST_BUDGET };
            if let Some(reg) = r.region.as_mut() {
                reg.bake_step(budget);
            }
            let baked = r.is_baked();
            if baked && !r.was_baked {
                r.baked_once = true;
                finished.push(Node3Id::from_bits(*k));
                if let Some(reg) = &r.region {
                    let (tw, th) = reg.tile_count();
                    maimbrain::sys::log(maimbrain::sys::Level::Info, &format!("navmesh: {} of {} tiles walkable, {:08x}", reg.walkable_tiles(), tw * th, reg.hash()));
                }
            }
            r.was_baked = baked;
        }
        for n in finished {
            self.signals.push((n, "bakeFinished", Vec::new()));
        }
        // 2. The crowd: agents join the first region, at their feet.
        let Some(rk) = self.nav.as_ref().unwrap().regions.iter().find(|(_, r)| r.region.is_some()).map(|(k, _)| *k) else { return };
        let keys: Vec<u32> = self.nav.as_ref().unwrap().agents.keys().copied().collect();
        let mut feet: BTreeMap<u32, (Option<Node3Id>, Vec3)> = BTreeMap::new();
        for k in &keys {
            feet.insert(*k, self.agent_feet(Node3Id::from_bits(*k)));
        }
        let nav = self.nav.as_mut().unwrap();
        let reg = nav.regions.get_mut(&rk).unwrap().region.as_mut().unwrap();
        for k in &keys {
            let a = nav.agents.get_mut(k).unwrap();
            let (_, at) = feet[k];
            let id = match a.id {
                Some((r, id)) if r == rk => id,
                _ => match reg.add_agent(at, &a.settings) {
                    Some(id) => {
                        a.id = Some((rk, id));
                        a.fresh = a.target.is_some();
                        id
                    }
                    None => continue,
                },
            };
            // A body its agent drives jumps there with its agent; one the
            // game moves itself (no autoMove) keeps its agent's corridor
            // and speed (a teleport would restart them every frame).
            if a.auto_move && feet[k].0.is_some_and(|pn| moved.contains(&pn.to_bits())) {
                reg.teleport_agent(id, at);
            } else if !a.auto_move {
                reg.place_agent(id, at);
            }
            if a.fresh && reg.agent(id).is_some() {
                a.fresh = false;
                if let Some(t) = a.target {
                    reg.set_target(id, t);
                }
            }
        }
        if dt > 0.0 {
            reg.update_crowd(dt);
        }
        for k in &keys {
            let a = nav.agents.get_mut(k).unwrap();
            if let Some((_, id)) = a.id
                && let Some(s) = reg.agent(id)
            {
                a.state = s;
            }
        }
        // 3. autoMove: the crowd's way into the body's moveAndSlide.
        let g = self.phys.as_deref().map_or(vec3(0.0, -9.8, 0.0), |p| p.world.gravity());
        // Agents pass through each other's bodies (the crowd keeps them
        // apart) and through the ground's heightfield: they stand on the
        // terrain's true heights instead, which is the same ground at a
        // fraction of the cost (a capsule cast against a heightfield is
        // most of a character's step).
        let terrain = self.ground.as_ref().is_some_and(|g| g.collision);
        let mut past: Vec<u64> = Vec::new();
        if let Some(p) = self.phys.as_deref_mut() {
            for k in &keys {
                let auto = self.nav.as_ref().unwrap().agents[k].auto_move;
                if let Some(pn) = feet[k].0
                    && let Some(c) = p.chars.get_mut(&pn.to_bits())
                {
                    c.agent = auto;
                    if auto {
                        past.push(c.ch.body().to_bits());
                    }
                }
            }
            if terrain {
                past.extend(p.tiles.values().map(|t| t.body.to_bits()));
            }
        }
        past.sort();
        let mut signals = Vec::new();
        for k in &keys {
            let (parent, _) = feet[k];
            let (auto, vel, id, target, desired, reached, has_state) = {
                let a = &self.nav.as_ref().unwrap().agents[k];
                (a.auto_move, a.state.vel, a.id, a.target, a.target_desired, a.reached, a.id.is_some())
            };
            let mut at = feet[k].1;
            // Godot measures from the body's origin (a character's is its shape's centre).
            let lift = parent.and_then(|pn| self.phys.as_deref().and_then(|p| p.chars.get(&pn.to_bits()))).map_or(0.0, |c| c.ch.half_height());
            if auto
                && dt > 0.0
                && let Some(pn) = parent
                && has_state
            {
                let p = self.phys.as_deref_mut().unwrap();
                let c = p.chars.get_mut(&pn.to_bits()).unwrap();
                let mut v = vec3(vel[0], c.vel.y, vel[2]);
                v.y = if c.grounded { 0.0 } else { v.y + g.y * dt };
                let was = c.grounded;
                c.vel = c.ch.move_and_slide_past(&mut p.world, v, dt, &past);
                let mut pos = c.ch.position();
                c.grounded = c.ch.on_floor();
                // The terrain under it: stood on, and held to when it was on
                // it and isn't going up (Godot's floor snap), unless it stands
                // on something else.
                if let Some(gr) = self.ground.as_ref().filter(|g| g.collision) {
                    let floor = gr.terrain.height_at(pos.x, pos.z) + c.ch.half_height() + 0.01;
                    let snap = if was && v.y <= 0.0 { c.ch.settings.floor_snap_length.max(0.3) } else { 0.0 };
                    if pos.y < floor || (!c.grounded && v.y <= 0.0 && pos.y <= floor + snap) {
                        pos.y = floor;
                        c.ch.set_position(&mut p.world, pos);
                        c.grounded = true;
                        c.vel.y = 0.0;
                    }
                }
                c.last = Some(pos);
                at = pos - Vec3::Y * c.ch.half_height();
                if let Some((_, aid)) = id {
                    let reg = self.nav.as_mut().unwrap().regions.get_mut(&rk).unwrap().region.as_mut().unwrap();
                    reg.place_agent(aid, at);
                }
                // Back to the node, turned to face its way.
                self.place_pos(pn, pos);
                if vel[0] * vel[0] + vel[2] * vel[2] > 0.04
                    && let Some(n) = self.scene.node_mut(pn)
                {
                    // libm's, so the turn is the same natively and in wasm.
                    let want = libm::atan2f(-vel[0], -vel[2]);
                    let mut d = want - n.ry;
                    while d > std::f32::consts::PI {
                        d -= std::f32::consts::TAU;
                    }
                    while d < -std::f32::consts::PI {
                        d += std::f32::consts::TAU;
                    }
                    n.ry += d.clamp(-12.0 * dt, 12.0 * dt);
                }
            }
            // 4. Reaching the target.
            if let Some(t) = target
                && !reached
                && (at + Vec3::Y * lift).distance(t) <= desired
            {
                let a = self.nav.as_mut().unwrap().agents.get_mut(k).unwrap();
                a.reached = true;
                a.finished = true;
                if let Some((_, aid)) = a.id {
                    let reg = self.nav.as_mut().unwrap().regions.get_mut(&rk).unwrap().region.as_mut().unwrap();
                    reg.stop(aid);
                }
                signals.push(Node3Id::from_bits(*k));
            }
        }
        for n in signals {
            self.signals.push((n, "targetReached", Vec::new()));
            self.signals.push((n, "navigationFinished", Vec::new()));
        }
    }

    /// The navmeshes' edges, for the debug menu's Show colliders.
    pub fn nav_lines(&self, out: &mut Vec<(Vec3, Vec3, u32)>) {
        let Some(nav) = self.nav.as_ref() else { return };
        let mut segs = Vec::new();
        for r in nav.regions.values() {
            if let Some(reg) = &r.region {
                reg.outlines(20_000, &mut segs);
                let lift = Vec3::Y * 0.05;
                out.extend(segs.iter().filter(|(a, _)| a.distance(self.cam.pos) < 60.0).map(|(a, b)| (*a + lift, *b + lift, 0x40e0a0ff)));
            }
        }
    }

    /// Forgets destroyed regions (their bakes kept) and agents.
    pub fn nav_forget(&mut self, gone: &[Node3Id]) {
        let Some(nav) = self.nav.as_mut() else { return };
        for id in gone {
            let k = id.to_bits();
            if let Some(r) = nav.regions.remove(&k) {
                nav.kept.push(r);
                for a in nav.agents.values_mut() {
                    if a.id.is_some_and(|(rk, _)| rk == k) {
                        a.id = None;
                    }
                }
            }
            if let Some(a) = nav.agents.remove(&k)
                && let Some((rk, aid)) = a.id
                && let Some(reg) = nav.regions.get_mut(&rk).and_then(|r| r.region.as_mut())
            {
                reg.remove_agent(aid);
            }
        }
    }
}
