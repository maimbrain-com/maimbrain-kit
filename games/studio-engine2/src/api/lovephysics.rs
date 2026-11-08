//! `love.physics` (docs/STUDIO-2.md §5 [physics], §9.4 row 3a): Box2D's API
//! as LÖVE has it, on the SDK's Rapier 2D (`maimbrain::physics2d`). Worlds,
//! bodies, fixtures, shapes, joints and contacts are LÖVE's userdata, with
//! LÖVE's names, arguments, units (`setMeter`) and callbacks; what differs
//! is in STUDIO-2-API.md's table.
//!
//! - **Units.** LÖVE's pixels, scaled by `setMeter` (30 px a meter unless
//!   set) as LÖVE scales them: positions, sizes and velocities by the meter,
//!   forces and impulses too, torques by its square. The SDK works at 100 px
//!   a meter, so a world keeps `k` = 100 ÷ the meter it was made at.
//! - **Steps.** `World:update(dt)` runs the SDK's fixed 60 Hz steps from
//!   the frame's `dt` (deterministic, replayed exactly), and reads in
//!   `love.draw` see bodies where they're drawn: between the last two steps.
//! - **Callbacks** run after the step that caused them, before `update`
//!   returns, as Lua calls (`beginContact`, `endContact`, then `preSolve`
//!   and `postSolve` for every pair still touching). Bodies and joints
//!   can't be made or destroyed in them, as in Box2D.
//! - **`node.body = body`** (ours): a 2D node follows a body, drawn
//!   between steps, so a sprite needs no `love.draw`.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{c_int, c_void};

use maimbrain::physics2d::{self as p2, BodyId, BodyKind, Event, JointId, ShapeId, Vec2, World, vec2};
use maimbrain::scene::NodeId;

use super::*;

/// The SDK's pixels a meter.
const SDK_PPM: f32 = p2::PIXELS_PER_METER;

#[derive(Clone, Debug)]
pub enum Geom {
    Circle {
        x: f32,
        y: f32,
        r: f32,
    },
    /// A rectangle: centre, size and angle (a polygon to LÖVE).
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        a: f32,
    },
    Polygon(Vec<(f32, f32)>),
    Edge(f32, f32, f32, f32),
    Chain {
        pts: Vec<(f32, f32)>,
        looped: bool,
    },
}

impl Geom {
    fn type_name(&self) -> &'static str {
        match self {
            Geom::Circle { .. } => "circle",
            Geom::Rect { .. } | Geom::Polygon(_) => "polygon",
            Geom::Edge(..) => "edge",
            Geom::Chain { .. } => "chain",
        }
    }
    /// Its points (polygon, edge, chain), as LÖVE's getPoints gives them.
    fn points(&self) -> Vec<(f32, f32)> {
        match self {
            Geom::Circle { .. } => Vec::new(),
            Geom::Rect { x, y, w, h, a } => {
                let (s, c) = (a.sin(), a.cos());
                [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)].iter().map(|(px, py)| (x + px * c - py * s, y + px * s + py * c)).collect()
            }
            Geom::Polygon(p) => p.clone(),
            Geom::Edge(a, b, c, d) => vec![(*a, *b), (*c, *d)],
            Geom::Chain { pts, .. } => pts.clone(),
        }
    }
    /// The SDK's shape at `k` SDK px a LÖVE px.
    fn sdk(&self, k: f32) -> p2::Shape {
        let v = |(x, y): (f32, f32)| vec2(x * k, y * k);
        match self {
            Geom::Circle { x, y, r } => p2::Shape::circle(r * k).at(x * k, y * k),
            Geom::Rect { x, y, w, h, a } => p2::Shape::rect(w * k, h * k).at(x * k, y * k).rotated(*a),
            Geom::Polygon(p) => p2::Shape::convex(&p.iter().map(|q| v(*q)).collect::<Vec<_>>()),
            Geom::Edge(a, b, c, d) => p2::Shape::segment(v((*a, *b)), v((*c, *d))),
            Geom::Chain { pts, looped } => {
                let mut list: Vec<Vec2> = pts.iter().map(|q| v(*q)).collect();
                if *looped && let Some(f) = list.first().copied() {
                    list.push(f);
                }
                p2::Shape::polyline(&list)
            }
        }
    }
}

pub struct LWorld {
    pub w: World,
    /// SDK px a LÖVE px (100 ÷ the meter when the world was made).
    pub k: f32,
    /// beginContact, endContact, preSolve, postSolve (registry refs, 0: none).
    pub cb: [c_int; 4],
    pub sleep: bool,
    pub alive: bool,
}

pub struct LBody {
    pub world: u32,
    pub id: BodyId,
    pub user: c_int,
    pub alive: bool,
    pub fixtures: Vec<u32>,
}

pub struct LFixture {
    pub body: u32,
    pub shape: u32,
    pub sid: ShapeId,
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub sensor: bool,
    pub category: u16,
    pub mask: u16,
    pub group: i16,
    pub user: c_int,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JKind {
    Weld,
    Revolute,
    Distance,
    Rope,
    Prismatic,
    Wheel,
    Mouse,
}

impl JKind {
    fn name(self) -> &'static str {
        match self {
            JKind::Weld => "weld",
            JKind::Revolute => "revolute",
            JKind::Distance => "distance",
            JKind::Rope => "rope",
            JKind::Prismatic => "prismatic",
            JKind::Wheel => "wheel",
            JKind::Mouse => "mouse",
        }
    }
}

pub struct LJoint {
    pub world: u32,
    pub kind: JKind,
    pub id: JointId,
    pub a: u32,
    pub b: u32,
    pub collide: bool,
    pub user: c_int,
    pub alive: bool,
    /// The anchors where they were made (LÖVE px, world).
    pub anchors: [f32; 4],
    /// Each anchor in its body's frame (SDK px), so it's read where the
    /// bodies are now. A mouse joint's first is its target instead.
    pub local: [Vec2; 2],
    /// Body A's angle when made (a prismatic's axis turns with it).
    pub ref_angle_a: f32,
    pub axis: (f32, f32),
    pub limits: (f32, f32),
    pub limits_on: bool,
    pub motor_on: bool,
    pub speed: f32,
    pub max_motor: f32,
    pub length: f32,
    pub freq: f32,
    pub damping: f32,
    /// The bodies' angle apart when made (a revolute's 0).
    pub ref_angle: f32,
    /// A mouse joint's own body, which its target moves.
    pub ground: Option<BodyId>,
    pub target: (f32, f32),
    pub max_force: f32,
}

/// A contact, valid while its callback runs.
pub struct LContact {
    pub world: u32,
    pub fa: u32,
    pub fb: u32,
    pub normal: (f32, f32),
    pub points: Vec<(f32, f32)>,
    pub touching: bool,
    pub enabled: bool,
    pub friction: f32,
    pub restitution: f32,
}

/// A round's love.physics.
pub struct Phys2 {
    pub meter: f32,
    pub worlds: Vec<LWorld>,
    pub bodies: Vec<LBody>,
    pub fixtures: Vec<LFixture>,
    pub shapes: Vec<Geom>,
    pub joints: Vec<LJoint>,
    pub contacts: Vec<LContact>,
    /// Contacts' generation: a contact kept past its callback is stale.
    pub epoch: u32,
    /// Registry table: one userdata per object, so `==` and table keys work.
    pub reg: c_int,
    pub in_callback: bool,
    /// love.draw is running: bodies read where they're drawn.
    pub drawing: bool,
    /// `node.body = body`: nodes that follow bodies.
    pub links: Vec<(NodeId, u32)>,
    /// `CharacterBody2D`s (character2d.rs).
    pub chars: Vec<super::character2d::LChar>,
    /// The fixture of each SDK shape.
    pub fix_of: BTreeMap<u64, u32>,
    pub warned: BTreeSet<&'static str>,
    /// The debug menu's Show colliders, and Pause physics.
    pub show: bool,
    pub paused: bool,
}

impl Phys2 {
    pub fn new() -> Phys2 {
        Phys2 {
            meter: 30.0,
            worlds: Vec::new(),
            bodies: Vec::new(),
            fixtures: Vec::new(),
            shapes: Vec::new(),
            joints: Vec::new(),
            contacts: Vec::new(),
            epoch: 1,
            reg: 0,
            in_callback: false,
            drawing: false,
            links: Vec::new(),
            chars: Vec::new(),
            fix_of: BTreeMap::new(),
            warned: BTreeSet::new(),
            show: false,
            paused: false,
        }
    }

    fn warn(&mut self, key: &'static str, msg: &str) {
        if self.warned.insert(key) {
            maimbrain::sys::log(maimbrain::sys::Level::Warn, msg);
        }
    }

    /// Bodies awake and in all, over every world (the debug menu).
    pub fn counts(&self) -> (usize, usize) {
        let mut out = (0, 0);
        for w in self.worlds.iter().filter(|w| w.alive) {
            out.0 += w.w.awake_count();
            out.1 += w.w.len();
        }
        out
    }
}

const TAGS: [i32; 6] = [tag::PWORLD, tag::PBODY, tag::PFIXTURE, tag::PSHAPE, tag::PJOINT, tag::PCONTACT];

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        let h = host(ud);
        if h.project.three {
            // A 3D game's bodies are RigidBody3D and the rest (prelude: love.physics says so).
            return;
        }
        register(
            L,
            ud,
            "love.physics",
            &[
                (c"setMeter", setMeter),
                (c"getMeter", getMeter),
                (c"newWorld", newWorld),
                (c"newBody", newBody),
                (c"newFixture", newFixture),
                (c"newRectangleShape", newRectangleShape),
                (c"newCircleShape", newCircleShape),
                (c"newPolygonShape", newPolygonShape),
                (c"newEdgeShape", newEdgeShape),
                (c"newChainShape", newChainShape),
                (c"newWeldJoint", newWeldJoint),
                (c"newRevoluteJoint", newRevoluteJoint),
                (c"newDistanceJoint", newDistanceJoint),
                (c"newRopeJoint", newRopeJoint),
                (c"newPrismaticJoint", newPrismaticJoint),
                (c"newWheelJoint", newWheelJoint),
                (c"newMouseJoint", newMouseJoint),
            ],
        );
        for (t, name) in TAGS.iter().zip([c"World", c"Body", c"Fixture", c"Shape", c"Joint", c"Contact"]) {
            register_kind(L, ud, *t, name, &[(c"__namecall", namecall), (c"__index", index), (c"__tostring", tostring)]);
        }
        lua_newtable(L);
        let p = h.phys2.get_or_insert_with(|| Box::new(Phys2::new()));
        p.reg = lua_ref(L, -1);
        lua_pop(L, 1);
    }
}

fn phys(h: &mut Host) -> &mut Phys2 {
    h.phys2.get_or_insert_with(|| Box::new(Phys2::new()))
}

/// Pushes an object's userdata: the same one every time.
unsafe fn push_obj(L: *mut lua_State, p: &Phys2, t: i32, i: u32) -> c_int {
    unsafe {
        let key = ((t - tag::PWORLD) << 24) | (i as i32 + 1);
        lua_rawgeti(L, LUA_REGISTRYINDEX, p.reg);
        lua_rawgeti(L, -1, key);
        if lua_type(L, -1) == LUA_TUSERDATA {
            lua_remove(L, -2);
            return 1;
        }
        lua_pop(L, 1);
        push_handle(L, t, i);
        lua_pushvalue(L, -1);
        lua_rawseti(L, -3, key);
        lua_remove(L, -2);
        1
    }
}

unsafe fn obj(L: *mut lua_State, i: c_int, t: i32) -> Option<u32> {
    unsafe { handle(L, i, t) }
}

unsafe fn need_num(L: *mut lua_State, i: c_int, f: &str) -> Result<f32, String> {
    unsafe {
        match num(L, i) {
            Some(v) if v.is_finite() => Ok(v as f32),
            _ => Err(format!("bad argument #{i} to '{f}' (number expected, got {})", type_name(L, i))),
        }
    }
}

unsafe fn opt_bool(L: *mut lua_State, i: c_int, d: bool) -> bool {
    unsafe {
        match lua_type(L, i) {
            LUA_TBOOLEAN => lua_toboolean(L, i) != 0,
            _ => d,
        }
    }
}

/// Numbers from `i` on, or one table of them at `i`.
unsafe fn coords(L: *mut lua_State, i: c_int, f: &str) -> Result<Vec<(f32, f32)>, String> {
    unsafe {
        let mut v = Vec::new();
        if lua_type(L, i) == LUA_TTABLE {
            for k in 1..=lua_objlen(L, i) {
                lua_rawgeti(L, i, k);
                let x = num(L, -1);
                lua_pop(L, 1);
                v.push(x.ok_or_else(|| format!("{f}: its points are numbers"))? as f32);
            }
        } else {
            let top = lua_gettop(L);
            for k in i..=top {
                v.push(need_num(L, k, f)?);
            }
        }
        if v.len() % 2 != 0 {
            return Err(format!("{f}: the points are x, y pairs (an odd count of numbers)"));
        }
        Ok(v.chunks(2).map(|c| (c[0], c[1])).collect())
    }
}

fn kind_name(k: BodyKind) -> &'static str {
    match k {
        BodyKind::Dynamic => "dynamic",
        BodyKind::Fixed => "static",
        BodyKind::Kinematic => "kinematic",
    }
}

fn kind_of(s: &str) -> Option<BodyKind> {
    match s {
        "dynamic" => Some(BodyKind::Dynamic),
        "static" => Some(BodyKind::Fixed),
        "kinematic" => Some(BodyKind::Kinematic),
        _ => None,
    }
}

/// Wraps a constructor or method body: `Err` raises it.
macro_rules! tri {
    ($L:ident, $e:expr) => {
        match $e {
            Ok(n) => n,
            Err(m) => return err($L, &m),
        }
    };
}

b!(setMeter, |L, h| {
    let m = tri!(L, need_num(L, 1, "setMeter"));
    if m <= 0.0 {
        return err(L, "setMeter: the meter is more than 0 px");
    }
    phys(h).meter = m;
    0
});

b!(getMeter, |L, h| ret(L, phys(h).meter as f64));

b!(newWorld, |L, h| {
    let (gx, gy) = (opt(L, 1, 0.0) as f32, opt(L, 2, 0.0) as f32);
    let sleep = opt_bool(L, 3, true);
    let p = phys(h);
    if p.in_callback {
        return err(L, "newWorld: not inside a world callback");
    }
    let k = SDK_PPM / p.meter;
    p.worlds.push(LWorld { w: World::with_gravity(vec2(gx * k, gy * k)), k, cb: [0; 4], sleep, alive: true });
    let i = p.worlds.len() as u32 - 1;
    push_obj(L, p, tag::PWORLD, i)
});

/// The live world at `i`, if it is one.
pub unsafe fn world_index(L: *mut lua_State, p: &Phys2, i: c_int) -> Option<u32> {
    unsafe { obj(L, i, tag::PWORLD).filter(|w| p.worlds[*w as usize].alive) }
}

/// The live world at `i`.
unsafe fn world_at(L: *mut lua_State, p: &Phys2, i: c_int, f: &str) -> Result<u32, String> {
    unsafe {
        let Some(w) = obj(L, i, tag::PWORLD) else {
            return Err(format!("bad argument #{i} to '{f}' (World expected, got {})", type_name(L, i)));
        };
        if !p.worlds[w as usize].alive {
            return Err(format!("{f}: this World was destroyed"));
        }
        Ok(w)
    }
}

unsafe fn body_at(L: *mut lua_State, p: &Phys2, i: c_int, f: &str) -> Result<u32, String> {
    unsafe {
        let Some(b) = obj(L, i, tag::PBODY) else {
            return Err(format!("bad argument #{i} to '{f}' (Body expected, got {})", type_name(L, i)));
        };
        if !p.bodies[b as usize].alive {
            return Err(format!("{f}: this Body was destroyed"));
        }
        Ok(b)
    }
}

b!(newBody, |L, h| {
    let p = phys(h);
    let wi = tri!(L, world_at(L, p, 1, "newBody"));
    if p.in_callback {
        return err(L, "newBody: bodies can't be made inside a world callback (Box2D's rule): remember it and make it after world:update");
    }
    let (x, y) = (opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    let kind = match st(L, 4) {
        None => BodyKind::Fixed,
        Some(s) => match kind_of(s) {
            Some(k) => k,
            None => {
                return err(L, &format!("newBody: \"{s}\" isn't a body type: \"static\", \"dynamic\" or \"kinematic\""));
            }
        },
    };
    let idx = p.bodies.len() as u32;
    let lw = &mut p.worlds[wi as usize];
    let k = lw.k;
    let mut def = match kind {
        BodyKind::Dynamic => p2::Body::dynamic(x * k, y * k),
        BodyKind::Fixed => p2::Body::fixed(x * k, y * k),
        BodyKind::Kinematic => p2::Body::kinematic(x * k, y * k),
    };
    def = def.tag(idx as u64);
    if !lw.sleep {
        def = def.always_awake();
    }
    let id = lw.w.add_body(def);
    p.bodies.push(LBody { world: wi, id, user: 0, alive: true, fixtures: Vec::new() });
    push_obj(L, p, tag::PBODY, idx)
});

fn shape_push(p: &mut Phys2, g: Geom) -> u32 {
    p.shapes.push(g);
    p.shapes.len() as u32 - 1
}

b!(newRectangleShape, |L, h| {
    let n = lua_gettop(L);
    let g = if n >= 4 {
        Geom::Rect {
            x: tri!(L, need_num(L, 1, "newRectangleShape")),
            y: tri!(L, need_num(L, 2, "newRectangleShape")),
            w: tri!(L, need_num(L, 3, "newRectangleShape")).abs(),
            h: tri!(L, need_num(L, 4, "newRectangleShape")).abs(),
            a: opt(L, 5, 0.0) as f32,
        }
    } else {
        Geom::Rect { x: 0.0, y: 0.0, w: tri!(L, need_num(L, 1, "newRectangleShape")).abs(), h: tri!(L, need_num(L, 2, "newRectangleShape")).abs(), a: 0.0 }
    };
    let p = phys(h);
    let i = shape_push(p, g);
    push_obj(L, p, tag::PSHAPE, i)
});

b!(newCircleShape, |L, h| {
    let g = if lua_gettop(L) >= 3 {
        Geom::Circle { x: tri!(L, need_num(L, 1, "newCircleShape")), y: tri!(L, need_num(L, 2, "newCircleShape")), r: tri!(L, need_num(L, 3, "newCircleShape")).abs() }
    } else {
        Geom::Circle { x: 0.0, y: 0.0, r: tri!(L, need_num(L, 1, "newCircleShape")).abs() }
    };
    let p = phys(h);
    let i = shape_push(p, g);
    push_obj(L, p, tag::PSHAPE, i)
});

b!(newPolygonShape, |L, h| {
    let pts = tri!(L, coords(L, 1, "newPolygonShape"));
    if pts.len() < 3 {
        return err(L, "newPolygonShape: a polygon has 3 points at least");
    }
    let p = phys(h);
    let i = shape_push(p, Geom::Polygon(pts));
    push_obj(L, p, tag::PSHAPE, i)
});

b!(newEdgeShape, |L, h| {
    let f = "newEdgeShape";
    let g = Geom::Edge(tri!(L, need_num(L, 1, f)), tri!(L, need_num(L, 2, f)), tri!(L, need_num(L, 3, f)), tri!(L, need_num(L, 4, f)));
    let p = phys(h);
    let i = shape_push(p, g);
    push_obj(L, p, tag::PSHAPE, i)
});

b!(newChainShape, |L, h| {
    let looped = lua_toboolean(L, 1) != 0;
    let pts = tri!(L, coords(L, 2, "newChainShape"));
    if pts.len() < 2 {
        return err(L, "newChainShape: a chain has 2 points at least");
    }
    let p = phys(h);
    let i = shape_push(p, Geom::Chain { pts, looped });
    push_obj(L, p, tag::PSHAPE, i)
});

/// LÖVE's categories (1–16) as the SDK's groups: on its categories,
/// colliding with what its mask doesn't name.
fn groups_of(category: u16, mask: u16) -> (u32, u32) {
    (category as u32, !(mask as u32) & 0xffff)
}

b!(newFixture, |L, h| {
    let p = phys(h);
    let bi = tri!(L, body_at(L, p, 1, "newFixture"));
    let Some(si) = obj(L, 2, tag::PSHAPE) else {
        return bad(L, 2, "newFixture", "Shape");
    };
    if p.in_callback {
        return err(L, "newFixture: fixtures can't be made inside a world callback: make it after world:update");
    }
    let density = opt(L, 3, 1.0) as f32;
    let geom = p.shapes[si as usize].clone();
    let b = &p.bodies[bi as usize];
    let wi = b.world;
    let id = b.id;
    let lw = &mut p.worlds[wi as usize];
    let (member, collides) = groups_of(1, 0);
    let shape = geom.sdk(lw.k).density(density.max(0.0)).friction(0.2).restitution(0.0).groups(member, collides).box2d_mixing();
    let sid = lw.w.add_shape(id, shape);
    let fi = p.fixtures.len() as u32;
    p.fixtures.push(LFixture { body: bi, shape: si, sid, density, friction: 0.2, restitution: 0.0, sensor: false, category: 1, mask: 0, group: 0, user: 0, alive: true });
    p.bodies[bi as usize].fixtures.push(fi);
    p.fix_of.insert(sid.to_bits(), fi);
    push_obj(L, p, tag::PFIXTURE, fi)
});

// ------------------------------------------------------------ joints

struct JointArgs {
    a: u32,
    b: u32,
    world: u32,
}

unsafe fn two_bodies(L: *mut lua_State, p: &Phys2, f: &str) -> Result<JointArgs, String> {
    unsafe {
        let a = body_at(L, p, 1, f)?;
        let b = body_at(L, p, 2, f)?;
        if a == b {
            return Err(format!("{f}: a joint joins two different bodies"));
        }
        let (wa, wb) = (p.bodies[a as usize].world, p.bodies[b as usize].world);
        if wa != wb {
            return Err(format!("{f}: the two bodies are in different worlds"));
        }
        if p.in_callback {
            return Err(format!("{f}: joints can't be made inside a world callback (Box2D's rule): remember the pair and join them after world:update"));
        }
        Ok(JointArgs { a, b, world: wa })
    }
}

fn new_joint(p: &mut Phys2, ja: &JointArgs, kind: JKind, def: p2::Joint, anchors: [f32; 4], collide: bool) -> u32 {
    let lw = &mut p.worlds[ja.world as usize];
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let ref_angle = lw.w.angle(idb) - lw.w.angle(ida);
    let k = lw.k;
    let to_local = |pose: p2::Pose, x: f32, y: f32| (vec2(x * k, y * k) - pose.pos).rotate(-pose.angle);
    let local = [to_local(lw.w.pose(ida), anchors[0], anchors[1]), to_local(lw.w.pose(idb), anchors[2], anchors[3])];
    let id = lw.w.join(def.collide(collide));
    p.joints.push(LJoint {
        world: ja.world,
        kind,
        id,
        a: ja.a,
        b: ja.b,
        collide,
        user: 0,
        alive: true,
        anchors,
        local,
        ref_angle_a: lw.w.angle(ida),
        axis: (0.0, 0.0),
        limits: (0.0, 0.0),
        limits_on: false,
        motor_on: false,
        speed: 0.0,
        max_motor: 0.0,
        length: 0.0,
        freq: 0.0,
        damping: 0.0,
        ref_angle,
        ground: None,
        target: (0.0, 0.0),
        max_force: 0.0,
    });
    p.joints.len() as u32 - 1
}

/// `(b1, b2, x, y, collide)` or `(b1, b2, x1, y1, x2, y2, collide)`: one
/// anchor or two, and where `collideConnected` is.
unsafe fn anchors(L: *mut lua_State, f: &str) -> Result<([f32; 4], c_int), String> {
    unsafe {
        let (x, y) = (need_num(L, 3, f)?, need_num(L, 4, f)?);
        if lua_type(L, 5) == LUA_TNUMBER && lua_type(L, 6) == LUA_TNUMBER { Ok(([x, y, need_num(L, 5, f)?, need_num(L, 6, f)?], 7)) } else { Ok(([x, y, x, y], 5)) }
    }
}

b!(newWeldJoint, |L, h| {
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, "newWeldJoint"));
    let (an, ci) = tri!(L, anchors(L, "newWeldJoint"));
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let collide = opt_bool(L, ci, false);
    let j = new_joint(p, &ja, JKind::Weld, p2::Joint::fixed(ida, idb), an, collide);
    push_obj(L, p, tag::PJOINT, j)
});

b!(newRevoluteJoint, |L, h| {
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, "newRevoluteJoint"));
    let (an, ci) = tri!(L, anchors(L, "newRevoluteJoint"));
    let k = p.worlds[ja.world as usize].k;
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let collide = opt_bool(L, ci, false);
    let j = new_joint(p, &ja, JKind::Revolute, p2::Joint::revolute(ida, idb, vec2(an[0] * k, an[1] * k)), an, collide);
    push_obj(L, p, tag::PJOINT, j)
});

b!(newDistanceJoint, |L, h| {
    let f = "newDistanceJoint";
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, f));
    let an = [tri!(L, need_num(L, 3, f)), tri!(L, need_num(L, 4, f)), tri!(L, need_num(L, 5, f)), tri!(L, need_num(L, 6, f))];
    let collide = opt_bool(L, 7, false);
    let k = p.worlds[ja.world as usize].k;
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let def = p2::Joint::distance(ida, idb, vec2(an[0] * k, an[1] * k), vec2(an[2] * k, an[3] * k));
    let j = new_joint(p, &ja, JKind::Distance, def, an, collide);
    p.joints[j as usize].length = ((an[2] - an[0]).powi(2) + (an[3] - an[1]).powi(2)).sqrt();
    push_obj(L, p, tag::PJOINT, j)
});

b!(newRopeJoint, |L, h| {
    let f = "newRopeJoint";
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, f));
    let an = [tri!(L, need_num(L, 3, f)), tri!(L, need_num(L, 4, f)), tri!(L, need_num(L, 5, f)), tri!(L, need_num(L, 6, f))];
    let max = tri!(L, need_num(L, 7, f)).max(0.0);
    let collide = opt_bool(L, 8, false);
    let k = p.worlds[ja.world as usize].k;
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let def = p2::Joint::rope(ida, idb, vec2(an[0] * k, an[1] * k), vec2(an[2] * k, an[3] * k)).length(max * k);
    let j = new_joint(p, &ja, JKind::Rope, def, an, collide);
    p.joints[j as usize].length = max;
    push_obj(L, p, tag::PJOINT, j)
});

/// `(b1, b2, x, y, ax, ay, collide)` or `(b1, b2, x1, y1, x2, y2, ax, ay, collide)`.
unsafe fn axis_args(L: *mut lua_State, f: &str) -> Result<([f32; 4], (f32, f32), c_int), String> {
    unsafe {
        let n = lua_gettop(L);
        let numbers = (3..=n).take_while(|&i| lua_type(L, i) == LUA_TNUMBER).count();
        if numbers >= 6 {
            let an = [need_num(L, 3, f)?, need_num(L, 4, f)?, need_num(L, 5, f)?, need_num(L, 6, f)?];
            Ok((an, (need_num(L, 7, f)?, need_num(L, 8, f)?), 9))
        } else {
            let (x, y) = (need_num(L, 3, f)?, need_num(L, 4, f)?);
            Ok(([x, y, x, y], (need_num(L, 5, f)?, need_num(L, 6, f)?), 7))
        }
    }
}

b!(newPrismaticJoint, |L, h| {
    let f = "newPrismaticJoint";
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, f));
    let (an, axis, ci) = tri!(L, axis_args(L, f));
    let collide = opt_bool(L, ci, false);
    let k = p.worlds[ja.world as usize].k;
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    let j = new_joint(p, &ja, JKind::Prismatic, p2::Joint::prismatic(ida, idb, vec2(an[0] * k, an[1] * k), vec2(axis.0, axis.1)), an, collide);
    p.joints[j as usize].axis = axis;
    push_obj(L, p, tag::PJOINT, j)
});

b!(newWheelJoint, |L, h| {
    let f = "newWheelJoint";
    let p = phys(h);
    let ja = tri!(L, two_bodies(L, p, f));
    let (an, axis, ci) = tri!(L, axis_args(L, f));
    let collide = opt_bool(L, ci, false);
    let k = p.worlds[ja.world as usize].k;
    let (ida, idb) = (p.bodies[ja.a as usize].id, p.bodies[ja.b as usize].id);
    // Box2D's wheel: a 2 Hz spring at 0.7 of critical damping.
    let def = p2::Joint::wheel(ida, idb, vec2(an[2] * k, an[3] * k), vec2(axis.0, axis.1)).stiffness(2.0, 0.7);
    let j = new_joint(p, &ja, JKind::Wheel, def, an, collide);
    let jj = &mut p.joints[j as usize];
    jj.axis = axis;
    jj.freq = 2.0;
    jj.damping = 0.7;
    push_obj(L, p, tag::PJOINT, j)
});

b!(newMouseJoint, |L, h| {
    let f = "newMouseJoint";
    let p = phys(h);
    let bi = tri!(L, body_at(L, p, 1, f));
    if p.in_callback {
        return err(L, "newMouseJoint: not inside a world callback");
    }
    let (x, y) = (tri!(L, need_num(L, 2, f)), tri!(L, need_num(L, 3, f)));
    let wi = p.bodies[bi as usize].world;
    let id = p.bodies[bi as usize].id;
    let lw = &mut p.worlds[wi as usize];
    let k = lw.k;
    let mass = lw.w.mass(id);
    // LÖVE's: 1000 × the body's mass in Box2D's newtons (LÖVE's units are
    // those × the meter), a 5 Hz spring at 0.7 of critical damping.
    let max_force = 1000.0 * mass.max(1e-3) * (SDK_PPM / k);
    let ground = lw.w.add_body(p2::Body::kinematic(x * k, y * k).tag(u64::MAX));
    let def = p2::Joint::mouse(ground, id, vec2(x * k, y * k)).stiffness(5.0, 0.7).motor(0.0, max_force * k);
    let jid = lw.w.join(def);
    p.joints.push(LJoint {
        world: wi,
        kind: JKind::Mouse,
        id: jid,
        a: bi,
        b: bi,
        collide: false,
        user: 0,
        alive: true,
        anchors: [x, y, x, y],
        local: [Vec2::ZERO, (vec2(x * k, y * k) - lw.w.pose(id).pos).rotate(-lw.w.pose(id).angle)],
        ref_angle_a: 0.0,
        axis: (0.0, 0.0),
        limits: (0.0, 0.0),
        limits_on: false,
        motor_on: false,
        speed: 0.0,
        max_motor: 0.0,
        length: 0.0,
        freq: 5.0,
        damping: 0.7,
        ref_angle: 0.0,
        ground: Some(ground),
        target: (x, y),
        max_force,
    });
    let j = p.joints.len() as u32 - 1;
    push_obj(L, p, tag::PJOINT, j)
});

// ------------------------------------------------------------ methods

b!(index, |L, h| {
    let k = st(L, 2).unwrap_or("?");
    err(L, &format!("love.physics objects' methods are called with a colon: obj:{k}(…)"))
});

b!(tostring, |L, h| {
    for (t, name) in TAGS.iter().zip(["World", "Body", "Fixture", "Shape", "Joint", "Contact"]) {
        if obj(L, 1, *t).is_some() {
            return rets(L, name);
        }
    }
    rets(L, "object")
});

b!(namecall, |L, h| {
    let name = cstr(lua_namecallatom(L, std::ptr::null_mut())).unwrap_or_default();
    let p = phys(h);
    let r = if let Some(i) = obj(L, 1, tag::PWORLD) {
        world_method(L, p, i, &name)
    } else if let Some(i) = obj(L, 1, tag::PBODY) {
        body_method(L, p, i, &name)
    } else if let Some(i) = obj(L, 1, tag::PFIXTURE) {
        fixture_method(L, p, i, &name)
    } else if let Some(i) = obj(L, 1, tag::PSHAPE) {
        shape_method(L, p, i, &name)
    } else if let Some(i) = obj(L, 1, tag::PJOINT) {
        joint_method(L, p, i, &name)
    } else if let Some(i) = obj(L, 1, tag::PCONTACT) {
        contact_method(L, p, i, &name)
    } else {
        Err("not a love.physics object".into())
    };
    match r {
        Ok(n) => n,
        Err(m) if m == "\u{1}" => -2,
        Err(m) => err(L, &m),
    }
});

/// Common to every object: type, typeOf, release.
unsafe fn common(L: *mut lua_State, name: &str, ty: &str) -> Option<c_int> {
    unsafe {
        match name {
            "type" => Some(rets(L, ty)),
            "typeOf" => Some(retb(L, st(L, 2).is_some_and(|s| s == ty || s == "Object" || (s == "Joint" && ty.ends_with("Joint"))))),
            "release" => Some(0),
            _ => None,
        }
    }
}

unsafe fn user_get(L: *mut lua_State, r: c_int) -> c_int {
    unsafe {
        if r == 0 {
            lua_pushnil(L);
        } else {
            lua_rawgeti(L, LUA_REGISTRYINDEX, r);
        }
        1
    }
}

unsafe fn user_set(L: *mut lua_State, r: &mut c_int) {
    unsafe {
        if *r != 0 {
            lua_unref(L, *r);
            *r = 0;
        }
        if lua_type(L, 2) != LUA_TNIL && lua_type(L, 2) != -1 {
            lua_pushvalue(L, 2);
            *r = lua_ref(L, -1);
            lua_pop(L, 1);
        }
    }
}

fn unknown_method(kind: &str, name: &str, list: &[&str]) -> String {
    let near = list.iter().map(|n| (super::node::distance(&n.to_ascii_lowercase(), &name.to_ascii_lowercase()), n)).filter(|(d, _)| *d <= 2).min_by_key(|(d, _)| *d).map(|(_, n)| n);
    match near {
        Some(n) => format!("{kind} has no method `{name}`; did you mean `{n}`?"),
        None => format!("{kind} has no method `{name}` in 2.0's love.physics"),
    }
}

pub const WORLD_METHODS: &[&str] = &[
    "update",
    "setGravity",
    "getGravity",
    "setCallbacks",
    "getCallbacks",
    "rayCast",
    "queryBoundingBox",
    "getBodies",
    "getBodyCount",
    "getJoints",
    "getJointCount",
    "getContacts",
    "getContactCount",
    "setSleepingAllowed",
    "isSleepingAllowed",
    "isLocked",
    "destroy",
    "isDestroyed",
];

unsafe fn world_method(L: *mut lua_State, p: &mut Phys2, wi: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        if let Some(n) = common(L, name, "World") {
            return Ok(n);
        }
        let alive = p.worlds[wi as usize].alive;
        match name {
            "isDestroyed" => return Ok(retb(L, !alive)),
            "destroy" if !alive => return Ok(0),
            _ if !alive => return Err(format!("World:{name}: this World was destroyed")),
            _ => {}
        }
        let k = p.worlds[wi as usize].k;
        Ok(match name {
            "update" => return world_update(L, p, wi),
            "setGravity" => {
                let (x, y) = (need_num(L, 2, name)?, need_num(L, 3, name)?);
                p.worlds[wi as usize].w.set_gravity(vec2(x * k, y * k));
                0
            }
            "getGravity" => {
                let g = p.worlds[wi as usize].w.gravity();
                ret2(L, (g.x / k) as f64, (g.y / k) as f64)
            }
            "setCallbacks" => {
                for i in 0..4 {
                    let at = i as c_int + 2;
                    let old = p.worlds[wi as usize].cb[i];
                    if old != 0 {
                        lua_unref(L, old);
                    }
                    p.worlds[wi as usize].cb[i] = match lua_type(L, at) {
                        LUA_TFUNCTION => {
                            lua_pushvalue(L, at);
                            let r = lua_ref(L, -1);
                            lua_pop(L, 1);
                            r
                        }
                        LUA_TNIL | -1 => 0,
                        _ => return Err(format!("setCallbacks: argument #{} is a function or nil, got {}", at - 1, type_name(L, at))),
                    };
                }
                0
            }
            "getCallbacks" => {
                for r in p.worlds[wi as usize].cb {
                    user_get(L, r);
                }
                4
            }
            "setSleepingAllowed" => {
                let on = opt_bool(L, 2, true);
                p.worlds[wi as usize].sleep = on;
                let ids: Vec<BodyId> = p.bodies.iter().filter(|b| b.alive && b.world == wi).map(|b| b.id).collect();
                for id in ids {
                    p.worlds[wi as usize].w.set_can_sleep(id, on);
                }
                0
            }
            "isSleepingAllowed" => retb(L, p.worlds[wi as usize].sleep),
            "isLocked" => retb(L, p.in_callback),
            "getBodies" | "getBodyList" => {
                let list: Vec<u32> = (0..p.bodies.len() as u32).filter(|&i| p.bodies[i as usize].alive && p.bodies[i as usize].world == wi).collect();
                lua_createtable(L, list.len() as c_int, 0);
                for (n, i) in list.into_iter().enumerate() {
                    push_obj(L, p, tag::PBODY, i);
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "getBodyCount" => ret(L, p.bodies.iter().filter(|b| b.alive && b.world == wi).count() as f64),
            "getJoints" | "getJointList" => {
                let list: Vec<u32> = (0..p.joints.len() as u32).filter(|&i| p.joints[i as usize].alive && p.joints[i as usize].world == wi).collect();
                lua_createtable(L, list.len() as c_int, 0);
                for (n, i) in list.into_iter().enumerate() {
                    push_obj(L, p, tag::PJOINT, i);
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "getJointCount" => ret(L, p.joints.iter().filter(|j| j.alive && j.world == wi).count() as f64),
            "getContactCount" => ret(L, p.worlds[wi as usize].w.touches().len() as f64),
            "getContacts" | "getContactList" => {
                // Contacts outside a callback last until the next update.
                let touches = p.worlds[wi as usize].w.touches();
                lua_createtable(L, touches.len() as c_int, 0);
                for (n, t) in touches.into_iter().enumerate() {
                    let c = contact_of_touch(p, wi, &t);
                    let ci = p.contacts.len() as u32;
                    p.contacts.push(c);
                    push_handle(L, tag::PCONTACT, (p.epoch << 16) | ci);
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "rayCast" => return world_raycast(L, p, wi),
            "queryBoundingBox" => return world_query(L, p, wi),
            "destroy" => {
                if p.in_callback {
                    return Err("World:destroy: not inside one of its callbacks".into());
                }
                for b in 0..p.bodies.len() {
                    if p.bodies[b].alive && p.bodies[b].world == wi {
                        destroy_body(L, p, b as u32);
                    }
                }
                let lw = &mut p.worlds[wi as usize];
                lw.alive = false;
                lw.w = World::new();
                for r in std::mem::take(&mut lw.cb) {
                    if r != 0 {
                        lua_unref(L, r);
                    }
                }
                0
            }
            "translateOrigin" => return Err("World:translateOrigin isn't in 2.0's love.physics: move the camera instead".into()),
            "setContactFilter" | "getContactFilter" => return Err("World:setContactFilter isn't in 2.0's love.physics: use Fixture:setCategory and setMask".into()),
            _ => return Err(unknown_method("World", name, WORLD_METHODS)),
        })
    }
}

fn contact_of_touch(p: &Phys2, wi: u32, t: &p2::Touch) -> LContact {
    let k = p.worlds[wi as usize].k;
    let fa = p.fix_of.get(&t.shape_a.to_bits()).copied().unwrap_or(u32::MAX);
    let fb = p.fix_of.get(&t.shape_b.to_bits()).copied().unwrap_or(u32::MAX);
    let mix = |f: &dyn Fn(&LFixture) -> f32| -> (f32, f32) {
        let a = p.fixtures.get(fa as usize).map_or(0.0, f);
        let b = p.fixtures.get(fb as usize).map_or(0.0, f);
        (a, b)
    };
    let (fr_a, fr_b) = mix(&|x| x.friction);
    let (re_a, re_b) = mix(&|x| x.restitution);
    LContact {
        world: wi,
        fa,
        fb,
        normal: (t.normal.x, t.normal.y),
        points: t.points.iter().flatten().map(|q| (q.x / k, q.y / k)).collect(),
        touching: true,
        enabled: true,
        // What the solver uses (Box2D's √(a·b) as near as Rapier has it).
        friction: fr_a.min(fr_b),
        restitution: re_a.max(re_b),
    }
}

/// A callback to run after the step: which (0 begin, 1 end, 2 pre, 3 post),
/// its fixtures, its contact and postSolve's impulses.
struct Call {
    which: usize,
    fa: u32,
    fb: u32,
    contact: u32,
    impulse: Option<(f32, f32)>,
}

/// `World:update(dt)`: the fixed steps, then the callbacks.
unsafe fn world_update(L: *mut lua_State, p: &mut Phys2, wi: u32) -> Result<c_int, String> {
    unsafe {
        if p.in_callback {
            return Err("World:update can't run inside one of its own callbacks".into());
        }
        let dt = num(L, 2).ok_or_else(|| format!("bad argument #1 to 'update' (number expected, got {})", type_name(L, 2)))? as f32;
        // Mouse joints' targets move their own bodies to where they point.
        for j in p.joints.iter().filter(|j| j.alive && j.world == wi) {
            if let Some(g) = j.ground {
                let k = p.worlds[wi as usize].k;
                p.worlds[wi as usize].w.move_kinematic(g, vec2(j.target.0 * k, j.target.1 * k), 0.0);
            }
        }
        let paused = p.paused;
        let lw = &mut p.worlds[wi as usize];
        // LÖVE's `update(dt, velocityiterations, positioniterations)`: the
        // first is Rapier's solver iterations (a stiffer stack of welds, as
        // Box2D's 8 make it); Rapier has no position iterations.
        if let Some(n) = num(L, 3).filter(|n| n.is_finite()) {
            lw.w.set_solver_iterations(n.max(1.0) as u32);
        }
        lw.w.step(if dt.is_finite() && !paused { dt.max(0.0) } else { 0.0 });
        let cb = lw.cb;
        let k = lw.k;
        let events: Vec<Event> = lw.w.events().to_vec();
        p.contacts.clear();
        p.epoch = p.epoch.wrapping_add(1) & 0xffff;
        let mut calls = Vec::new();
        let fix = |p: &Phys2, s: ShapeId| p.fix_of.get(&s.to_bits()).copied();
        for e in &events {
            let (which, sa, sb, point, normal) = match *e {
                Event::ContactBegin(c) => (0, c.shape_a, c.shape_b, Some(c.point), c.normal),
                Event::ContactEnd { shape_a, shape_b, .. } => (1, shape_a, shape_b, None, Vec2::ZERO),
                Event::SensorEnter { sensor_shape, other_shape, .. } => (0, sensor_shape, other_shape, None, Vec2::ZERO),
                Event::SensorExit { sensor_shape, other_shape, .. } => (1, sensor_shape, other_shape, None, Vec2::ZERO),
            };
            if cb[which] == 0 {
                continue;
            }
            let (Some(fa), Some(fb)) = (fix(p, sa), fix(p, sb)) else {
                continue;
            };
            let mut c = match p.worlds[wi as usize].w.touch(sa, sb) {
                Some(t) if which == 0 => contact_of_touch(p, wi, &t),
                _ => LContact {
                    world: wi,
                    fa,
                    fb,
                    normal: (normal.x, normal.y),
                    points: point.map(|q| vec![(q.x / k, q.y / k)]).unwrap_or_default(),
                    touching: which == 0,
                    enabled: true,
                    friction: 0.0,
                    restitution: 0.0,
                },
            };
            c.fa = fa;
            c.fb = fb;
            p.contacts.push(c);
            calls.push(Call { which, fa, fb, contact: p.contacts.len() as u32 - 1, impulse: None });
        }
        if cb[2] != 0 || cb[3] != 0 {
            for t in p.worlds[wi as usize].w.touches() {
                let (Some(fa), Some(fb)) = (fix(p, t.shape_a), fix(p, t.shape_b)) else {
                    continue;
                };
                let c = contact_of_touch(p, wi, &t);
                p.contacts.push(c);
                let ci = p.contacts.len() as u32 - 1;
                if cb[2] != 0 {
                    calls.push(Call { which: 2, fa, fb, contact: ci, impulse: None });
                }
                if cb[3] != 0 {
                    calls.push(Call { which: 3, fa, fb, contact: ci, impulse: Some((t.normal_impulse / k, t.tangent_impulse / k)) });
                }
            }
        }
        if calls.is_empty() {
            return Ok(0);
        }
        p.in_callback = true;
        let epoch = p.epoch;
        let mut failed = None;
        for c in calls {
            let r = p.worlds[wi as usize].cb[c.which];
            if r == 0 || !p.fixtures[c.fa as usize].alive || !p.fixtures[c.fb as usize].alive {
                continue;
            }
            lua_rawgeti(L, LUA_REGISTRYINDEX, r);
            push_obj(L, p, tag::PFIXTURE, c.fa);
            push_obj(L, p, tag::PFIXTURE, c.fb);
            push_handle(L, tag::PCONTACT, (epoch << 16) | c.contact);
            let mut n = 3;
            if let Some((ni, ti)) = c.impulse {
                lua_pushnumber(L, ni as f64);
                lua_pushnumber(L, ti as f64);
                n += 2;
            }
            if lua_pcall(L, n, 0, 0) != LUA_OK {
                failed = Some(to_s(L, -1));
                lua_pop(L, 1);
                break;
            }
            // A callback may have destroyed the world (an error then) or
            // swapped its callbacks; carry on with what's there.
            if !p.worlds[wi as usize].alive {
                break;
            }
        }
        p.in_callback = false;
        if let Some(m) = failed {
            push_str(L, &m);
            return Err("\u{1}".into());
        }
        Ok(0)
    }
}

/// `World:rayCast(x1, y1, x2, y2, callback)`: `callback(fixture, x, y, xn,
/// yn, fraction)` for each fixture the ray crosses, nearest first; it
/// returns -1 to ignore that one, 0 to stop, a fraction to clip the ray
/// there, or 1 to go on.
unsafe fn world_raycast(L: *mut lua_State, p: &mut Phys2, wi: u32) -> Result<c_int, String> {
    unsafe {
        let f = "rayCast";
        let (x1, y1, x2, y2) = (need_num(L, 2, f)?, need_num(L, 3, f)?, need_num(L, 4, f)?, need_num(L, 5, f)?);
        if lua_type(L, 6) != LUA_TFUNCTION {
            return Err(format!("bad argument #5 to 'rayCast' (function expected, got {})", type_name(L, 6)));
        }
        let k = p.worlds[wi as usize].k;
        let hits = p.worlds[wi as usize].w.raycast_all(vec2(x1 * k, y1 * k), vec2(x2 * k, y2 * k));
        let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt() * k;
        let mut clip = 1.0f32;
        let was = p.in_callback;
        p.in_callback = true;
        let mut failed = None;
        for hit in hits {
            let Some(&fi) = p.fix_of.get(&hit.shape.to_bits()) else {
                continue;
            };
            let fraction = if len > 0.0 { hit.distance / len } else { 0.0 };
            if fraction > clip {
                break;
            }
            lua_pushvalue(L, 6);
            push_obj(L, p, tag::PFIXTURE, fi);
            for v in [hit.point.x / k, hit.point.y / k, hit.normal.x, hit.normal.y, fraction] {
                lua_pushnumber(L, v as f64);
            }
            if lua_pcall(L, 6, 1, 0) != LUA_OK {
                failed = Some(to_s(L, -1));
                lua_pop(L, 1);
                break;
            }
            let r = num(L, -1).unwrap_or(1.0) as f32;
            lua_pop(L, 1);
            if r == 0.0 {
                break;
            }
            if r > 0.0 {
                clip = clip.min(r);
            }
        }
        p.in_callback = was;
        if let Some(m) = failed {
            push_str(L, &m);
            return Err("\u{1}".into());
        }
        Ok(0)
    }
}

/// `World:queryBoundingBox(x1, y1, x2, y2, callback)`: `callback(fixture)`
/// for each fixture whose bounding box overlaps, until it returns false.
unsafe fn world_query(L: *mut lua_State, p: &mut Phys2, wi: u32) -> Result<c_int, String> {
    unsafe {
        let f = "queryBoundingBox";
        let (x1, y1, x2, y2) = (need_num(L, 2, f)?, need_num(L, 3, f)?, need_num(L, 4, f)?, need_num(L, 5, f)?);
        if lua_type(L, 6) != LUA_TFUNCTION {
            return Err(format!("bad argument #5 to 'queryBoundingBox' (function expected, got {})", type_name(L, 6)));
        }
        let k = p.worlds[wi as usize].k;
        let shapes = p.worlds[wi as usize].w.shapes_in_rect(vec2(x1.min(x2) * k, y1.min(y2) * k), vec2(x1.max(x2) * k, y1.max(y2) * k));
        let was = p.in_callback;
        p.in_callback = true;
        let mut failed = None;
        for s in shapes {
            let Some(&fi) = p.fix_of.get(&s.to_bits()) else {
                continue;
            };
            lua_pushvalue(L, 6);
            push_obj(L, p, tag::PFIXTURE, fi);
            if lua_pcall(L, 1, 1, 0) != LUA_OK {
                failed = Some(to_s(L, -1));
                lua_pop(L, 1);
                break;
            }
            let go = lua_toboolean(L, -1) != 0;
            lua_pop(L, 1);
            if !go {
                break;
            }
        }
        p.in_callback = was;
        if let Some(m) = failed {
            push_str(L, &m);
            return Err("\u{1}".into());
        }
        Ok(0)
    }
}

/// Destroys a body: its fixtures and joints go with it.
unsafe fn destroy_body(L: *mut lua_State, p: &mut Phys2, bi: u32) {
    unsafe {
        let b = &mut p.bodies[bi as usize];
        if !b.alive {
            return;
        }
        b.alive = false;
        let (wi, id) = (b.world, b.id);
        if b.user != 0 {
            lua_unref(L, b.user);
            b.user = 0;
        }
        for fi in std::mem::take(&mut b.fixtures) {
            let f = &mut p.fixtures[fi as usize];
            f.alive = false;
            if f.user != 0 {
                lua_unref(L, f.user);
                f.user = 0;
            }
            p.fix_of.remove(&f.sid.to_bits());
        }
        p.worlds[wi as usize].w.remove(id);
        for j in 0..p.joints.len() {
            if p.joints[j].alive && (p.joints[j].a == bi || p.joints[j].b == bi) {
                destroy_joint(L, p, j as u32);
            }
        }
        p.links.retain(|l| l.1 != bi);
    }
}

unsafe fn destroy_joint(L: *mut lua_State, p: &mut Phys2, ji: u32) {
    unsafe {
        let j = &mut p.joints[ji as usize];
        if !j.alive {
            return;
        }
        j.alive = false;
        if j.user != 0 {
            lua_unref(L, j.user);
            j.user = 0;
        }
        let (wi, id, ground) = (j.world, j.id, j.ground);
        let w = &mut p.worlds[wi as usize].w;
        if w.has_joint(id) {
            w.unjoin(id);
        }
        if let Some(g) = ground {
            w.remove(g);
        }
    }
}

pub const BODY_METHODS: &[&str] = &[
    "getPosition",
    "getX",
    "getY",
    "getAngle",
    "setPosition",
    "setX",
    "setY",
    "setAngle",
    "getTransform",
    "setTransform",
    "getLinearVelocity",
    "setLinearVelocity",
    "getAngularVelocity",
    "setAngularVelocity",
    "applyForce",
    "applyLinearImpulse",
    "applyTorque",
    "applyAngularImpulse",
    "getMass",
    "setMass",
    "getInertia",
    "resetMassData",
    "getMassData",
    "getType",
    "setType",
    "isAwake",
    "setAwake",
    "isSleepingAllowed",
    "setSleepingAllowed",
    "isFixedRotation",
    "setFixedRotation",
    "isBullet",
    "setBullet",
    "getGravityScale",
    "setGravityScale",
    "getLinearDamping",
    "setLinearDamping",
    "getAngularDamping",
    "setAngularDamping",
    "getWorldPoint",
    "getWorldPoints",
    "getLocalPoint",
    "getWorldVector",
    "getLocalVector",
    "getWorldCenter",
    "getLocalCenter",
    "getLinearVelocityFromWorldPoint",
    "getLinearVelocityFromLocalPoint",
    "getFixtures",
    "getJoints",
    "getContacts",
    "isTouching",
    "getUserData",
    "setUserData",
    "getWorld",
    "destroy",
    "isDestroyed",
    "isActive",
    "setActive",
];

unsafe fn body_method(L: *mut lua_State, p: &mut Phys2, bi: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        if let Some(n) = common(L, name, "Body") {
            return Ok(n);
        }
        let alive = p.bodies[bi as usize].alive;
        match name {
            "isDestroyed" => return Ok(retb(L, !alive)),
            "destroy" if !alive => return Ok(0),
            _ if !alive => return Err(format!("Body:{name}: this Body was destroyed")),
            _ => {}
        }
        let (wi, id) = (p.bodies[bi as usize].world, p.bodies[bi as usize].id);
        let drawing = p.drawing;
        let lw = &mut p.worlds[wi as usize];
        let k = lw.k;
        let w = &mut lw.w;
        let pose = if drawing { w.draw_pose(id) } else { w.pose(id) };
        let n2 = |i: c_int| need_num(L, i, name);
        let world_pt = |x: f32, y: f32| {
            let q = pose.apply(vec2(x * k, y * k));
            (q.x / k, q.y / k)
        };
        let local_pt = |x: f32, y: f32| {
            let d = vec2(x * k, y * k) - pose.pos;
            let q = d.rotate(-pose.angle);
            (q.x / k, q.y / k)
        };
        Ok(match name {
            "getPosition" => ret2(L, (pose.pos.x / k) as f64, (pose.pos.y / k) as f64),
            "getX" => ret(L, (pose.pos.x / k) as f64),
            "getY" => ret(L, (pose.pos.y / k) as f64),
            "getAngle" => ret(L, pose.angle as f64),
            "getTransform" => {
                lua_pushnumber(L, (pose.pos.x / k) as f64);
                lua_pushnumber(L, (pose.pos.y / k) as f64);
                lua_pushnumber(L, pose.angle as f64);
                3
            }
            "setPosition" => {
                let (x, y) = (n2(2)?, n2(3)?);
                w.set_position(id, vec2(x * k, y * k));
                0
            }
            "setX" | "setY" => {
                let v = n2(2)? * k;
                let cur = w.position(id);
                w.set_position(id, if name == "setX" { vec2(v, cur.y) } else { vec2(cur.x, v) });
                0
            }
            "setAngle" => {
                w.set_angle(id, n2(2)?);
                0
            }
            "setTransform" => {
                let (x, y) = (n2(2)?, n2(3)?);
                w.set_position(id, vec2(x * k, y * k));
                w.set_angle(id, opt(L, 4, 0.0) as f32);
                0
            }
            "getLinearVelocity" => {
                let v = w.velocity(id);
                ret2(L, (v.x / k) as f64, (v.y / k) as f64)
            }
            "setLinearVelocity" => {
                let (x, y) = (n2(2)?, n2(3)?);
                w.set_velocity(id, vec2(x * k, y * k));
                0
            }
            "getAngularVelocity" => ret(L, w.spin(id) as f64),
            "setAngularVelocity" => {
                w.set_spin(id, n2(2)?);
                0
            }
            "applyForce" => {
                let f = vec2(n2(2)? * k, n2(3)? * k);
                if lua_type(L, 4) == LUA_TNUMBER {
                    w.apply_force_at(id, f, vec2(n2(4)? * k, n2(5)? * k));
                } else {
                    w.apply_force(id, f);
                }
                0
            }
            "applyLinearImpulse" => {
                let i = vec2(n2(2)? * k, n2(3)? * k);
                if lua_type(L, 4) == LUA_TNUMBER {
                    w.apply_impulse_at(id, i, vec2(n2(4)? * k, n2(5)? * k));
                } else {
                    w.apply_impulse(id, i);
                }
                0
            }
            "applyTorque" => {
                w.apply_torque(id, n2(2)? * k * k);
                0
            }
            "applyAngularImpulse" => {
                w.apply_angular_impulse(id, n2(2)? * k * k);
                0
            }
            "getMass" => ret(L, w.mass(id) as f64),
            "setMass" => {
                let m = n2(2)?.max(0.0);
                let total = w.mass(id);
                let fixtures = p.bodies[bi as usize].fixtures.clone();
                let w = &mut p.worlds[wi as usize].w;
                if total > 0.0 {
                    // Every fixture's mass scaled alike, as LÖVE scales the
                    // body's: the centre of mass and how mass lies stay.
                    let shapes: Vec<ShapeId> = fixtures.iter().map(|f| p.fixtures[*f as usize].sid).collect();
                    let masses: Vec<f32> = shapes.iter().map(|s| w.shape_mass(*s)).collect();
                    for (s, sm) in shapes.iter().zip(masses) {
                        w.set_shape_mass(*s, m * sm / total);
                    }
                }
                0
            }
            "getInertia" => ret(L, (w.inertia(id) / (k * k)) as f64),
            "resetMassData" => {
                let fixtures = p.bodies[bi as usize].fixtures.clone();
                for f in fixtures {
                    let fx = &p.fixtures[f as usize];
                    let (s, d) = (fx.sid, fx.density);
                    p.worlds[wi as usize].w.set_density(s, d);
                }
                0
            }
            "getMassData" => {
                let c = w.center_of_mass(id);
                let (lx, ly) = local_pt(c.x / k, c.y / k);
                lua_pushnumber(L, lx as f64);
                lua_pushnumber(L, ly as f64);
                lua_pushnumber(L, w.mass(id) as f64);
                lua_pushnumber(L, (w.inertia(id) / (k * k)) as f64);
                4
            }
            "getType" => rets(L, kind_name(w.kind(id))),
            "setType" => {
                let s = st(L, 2).unwrap_or("");
                let Some(kind) = kind_of(s) else {
                    return Err(format!("setType: \"{s}\" isn't a body type: \"static\", \"dynamic\" or \"kinematic\""));
                };
                w.set_kind(id, kind);
                0
            }
            "isAwake" => retb(L, !w.is_sleeping(id)),
            "setAwake" => {
                if opt_bool(L, 2, true) {
                    w.wake(id);
                } else {
                    w.sleep(id);
                }
                0
            }
            "isSleepingAllowed" => retb(L, lw_sleep(p, wi)),
            "setSleepingAllowed" => {
                p.worlds[wi as usize].w.set_can_sleep(id, opt_bool(L, 2, true));
                0
            }
            "isFixedRotation" => retb(L, p.worlds[wi as usize].w.is_fixed_rotation(id)),
            "setFixedRotation" => {
                w.set_fixed_rotation(id, opt_bool(L, 2, true));
                0
            }
            "isBullet" => retb(L, w.is_bullet(id)),
            "setBullet" => {
                w.set_bullet(id, opt_bool(L, 2, true));
                0
            }
            "getGravityScale" => ret(L, w.gravity_scale(id) as f64),
            "setGravityScale" => {
                w.set_gravity_scale(id, n2(2)?);
                0
            }
            "getLinearDamping" => ret(L, w.damping(id).0 as f64),
            "getAngularDamping" => ret(L, w.damping(id).1 as f64),
            "setLinearDamping" => {
                let d = w.damping(id);
                w.set_damping(id, n2(2)?.max(0.0), d.1);
                0
            }
            "setAngularDamping" => {
                let d = w.damping(id);
                w.set_damping(id, d.0, n2(2)?.max(0.0));
                0
            }
            "getWorldPoint" => {
                let (x, y) = world_pt(n2(2)?, n2(3)?);
                ret2(L, x as f64, y as f64)
            }
            "getWorldPoints" => {
                let pts = coords(L, 2, name)?;
                for (x, y) in &pts {
                    let (a, b) = world_pt(*x, *y);
                    lua_pushnumber(L, a as f64);
                    lua_pushnumber(L, b as f64);
                }
                pts.len() as c_int * 2
            }
            "getLocalPoint" => {
                let (x, y) = local_pt(n2(2)?, n2(3)?);
                ret2(L, x as f64, y as f64)
            }
            "getWorldVector" => {
                let v = vec2(n2(2)?, n2(3)?).rotate(pose.angle);
                ret2(L, v.x as f64, v.y as f64)
            }
            "getLocalVector" => {
                let v = vec2(n2(2)?, n2(3)?).rotate(-pose.angle);
                ret2(L, v.x as f64, v.y as f64)
            }
            "getWorldCenter" => {
                let c = w.center_of_mass(id);
                ret2(L, (c.x / k) as f64, (c.y / k) as f64)
            }
            "getLocalCenter" => {
                let c = w.center_of_mass(id);
                let (x, y) = local_pt(c.x / k, c.y / k);
                ret2(L, x as f64, y as f64)
            }
            "getLinearVelocityFromWorldPoint" | "getLinearVelocityFromLocalPoint" => {
                let (mut x, mut y) = (n2(2)?, n2(3)?);
                if name.ends_with("LocalPoint") {
                    (x, y) = world_pt(x, y);
                }
                let v = w.velocity_at(id, vec2(x * k, y * k));
                ret2(L, (v.x / k) as f64, (v.y / k) as f64)
            }
            "getFixtures" | "getFixtureList" => {
                let list = p.bodies[bi as usize].fixtures.clone();
                lua_createtable(L, list.len() as c_int, 0);
                for (n, f) in list.into_iter().enumerate() {
                    push_obj(L, p, tag::PFIXTURE, f);
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "getJoints" | "getJointList" => {
                let list: Vec<u32> = (0..p.joints.len() as u32).filter(|&j| p.joints[j as usize].alive && (p.joints[j as usize].a == bi || p.joints[j as usize].b == bi)).collect();
                lua_createtable(L, list.len() as c_int, 0);
                for (n, j) in list.into_iter().enumerate() {
                    push_obj(L, p, tag::PJOINT, j);
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "getContacts" | "getContactList" => {
                let mine: Vec<u64> = p.bodies[bi as usize].fixtures.iter().map(|f| p.fixtures[*f as usize].sid.to_bits()).collect();
                let touches: Vec<p2::Touch> = p.worlds[wi as usize].w.touches().into_iter().filter(|t| mine.contains(&t.shape_a.to_bits()) || mine.contains(&t.shape_b.to_bits())).collect();
                lua_createtable(L, touches.len() as c_int, 0);
                for (n, t) in touches.iter().enumerate() {
                    let c = contact_of_touch(p, wi, t);
                    p.contacts.push(c);
                    push_handle(L, tag::PCONTACT, (p.epoch << 16) | (p.contacts.len() as u32 - 1));
                    lua_rawseti(L, -2, n as c_int + 1);
                }
                1
            }
            "isTouching" => {
                let other = body_at(L, p, 2, name)?;
                let oid = p.bodies[other as usize].id;
                retb(L, p.worlds[wi as usize].w.touching(id, oid))
            }
            "getUserData" => user_get(L, p.bodies[bi as usize].user),
            "setUserData" => {
                user_set(L, &mut p.bodies[bi as usize].user);
                0
            }
            "getWorld" => push_obj(L, p, tag::PWORLD, wi),
            "destroy" => {
                if p.in_callback {
                    return Err("Body:destroy can't run inside a world callback (Box2D's rule): remember it and destroy it after world:update".into());
                }
                destroy_body(L, p, bi);
                0
            }
            "isActive" => retb(L, true),
            "setActive" => {
                p.warn("setActive", "Body:setActive isn't in 2.0's love.physics: destroy the body, or make its fixtures sensors");
                0
            }
            _ => return Err(unknown_method("Body", name, BODY_METHODS)),
        })
    }
}

/// A joint's anchors in the world now (LÖVE px), where the bodies are
/// drawn while love.draw runs: a mouse joint's target, then its body's point.
fn world_anchors(p: &Phys2, ji: u32) -> [Vec2; 2] {
    let j = &p.joints[ji as usize];
    let lw = &p.worlds[j.world as usize];
    let pose = |b: u32| {
        let id = p.bodies[b as usize].id;
        if p.drawing { lw.w.draw_pose(id) } else { lw.w.pose(id) }
    };
    let at = |b: u32, l: Vec2| pose(b).apply(l) / lw.k;
    let first = if j.kind == JKind::Mouse { vec2(j.target.0, j.target.1) } else { at(j.a, j.local[0]) };
    [first, at(j.b, j.local[1])]
}

fn lw_sleep(p: &Phys2, wi: u32) -> bool {
    p.worlds[wi as usize].sleep
}

pub const FIXTURE_METHODS: &[&str] = &[
    "setOneWay",
    "isOneWay",
    "getBody",
    "getShape",
    "getType",
    "setFriction",
    "getFriction",
    "setRestitution",
    "getRestitution",
    "setDensity",
    "getDensity",
    "setSensor",
    "isSensor",
    "setUserData",
    "getUserData",
    "setCategory",
    "getCategory",
    "setMask",
    "getMask",
    "setGroupIndex",
    "getGroupIndex",
    "setFilterData",
    "getFilterData",
    "testPoint",
    "getBoundingBox",
    "getMassData",
    "destroy",
    "isDestroyed",
];

/// Categories as LÖVE takes them: numbers 1–16.
unsafe fn categories(L: *mut lua_State, from: c_int, f: &str) -> Result<u16, String> {
    unsafe {
        let mut bits = 0u16;
        for i in from..=lua_gettop(L) {
            let c = need_num(L, i, f)? as i32;
            if !(1..=16).contains(&c) {
                return Err(format!("{f}: categories are 1 to 16, got {c}"));
            }
            bits |= 1 << (c - 1);
        }
        Ok(bits)
    }
}

unsafe fn push_categories(L: *mut lua_State, bits: u16) -> c_int {
    unsafe {
        let mut n = 0;
        for c in 0..16 {
            if bits & (1 << c) != 0 {
                lua_pushnumber(L, (c + 1) as f64);
                n += 1;
            }
        }
        n
    }
}

unsafe fn fixture_method(L: *mut lua_State, p: &mut Phys2, fi: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        if let Some(n) = common(L, name, "Fixture") {
            return Ok(n);
        }
        let alive = p.fixtures[fi as usize].alive;
        match name {
            "isDestroyed" => return Ok(retb(L, !alive)),
            "destroy" if !alive => return Ok(0),
            _ if !alive => return Err(format!("Fixture:{name}: this Fixture was destroyed")),
            _ => {}
        }
        let bi = p.fixtures[fi as usize].body;
        let wi = p.bodies[bi as usize].world;
        let k = p.worlds[wi as usize].k;
        let sid = p.fixtures[fi as usize].sid;
        let n2 = |i: c_int| need_num(L, i, name);
        let regroup = |p: &mut Phys2| {
            let f = &p.fixtures[fi as usize];
            let (m, c) = groups_of(f.category, f.mask);
            p.worlds[wi as usize].w.set_groups(sid, m, c);
        };
        Ok(match name {
            "getBody" => push_obj(L, p, tag::PBODY, bi),
            "getShape" => push_obj(L, p, tag::PSHAPE, p.fixtures[fi as usize].shape),
            "getType" => rets(L, p.shapes[p.fixtures[fi as usize].shape as usize].type_name()),
            "getFriction" => ret(L, p.fixtures[fi as usize].friction as f64),
            "setFriction" => {
                let v = n2(2)?.max(0.0);
                p.fixtures[fi as usize].friction = v;
                p.worlds[wi as usize].w.set_friction(sid, v);
                0
            }
            "getRestitution" => ret(L, p.fixtures[fi as usize].restitution as f64),
            "setRestitution" => {
                let v = n2(2)?.max(0.0);
                p.fixtures[fi as usize].restitution = v;
                p.worlds[wi as usize].w.set_restitution(sid, v);
                0
            }
            "getDensity" => ret(L, p.fixtures[fi as usize].density as f64),
            "setDensity" => {
                // As in Box2D, the body's mass changes at resetMassData; here at once.
                let v = n2(2)?.max(0.0);
                p.fixtures[fi as usize].density = v;
                p.worlds[wi as usize].w.set_density(sid, v);
                0
            }
            "isSensor" => retb(L, p.fixtures[fi as usize].sensor),
            "setSensor" => {
                let on = opt_bool(L, 2, true);
                p.fixtures[fi as usize].sensor = on;
                p.worlds[wi as usize].w.set_sensor(sid, on);
                0
            }
            // Ours: a one-way platform for CharacterBody2Ds (they land on it
            // from above and pass up through it), as LÖVE's preSolve can't make one here.
            "setOneWay" => {
                p.worlds[wi as usize].w.set_one_way(sid, opt_bool(L, 2, true));
                0
            }
            "isOneWay" => retb(L, p.worlds[wi as usize].w.is_one_way(sid)),
            "getUserData" => user_get(L, p.fixtures[fi as usize].user),
            "setUserData" => {
                user_set(L, &mut p.fixtures[fi as usize].user);
                0
            }
            "setCategory" => {
                p.fixtures[fi as usize].category = categories(L, 2, name)?;
                regroup(p);
                0
            }
            "getCategory" => push_categories(L, p.fixtures[fi as usize].category),
            "setMask" => {
                p.fixtures[fi as usize].mask = categories(L, 2, name)?;
                regroup(p);
                0
            }
            "getMask" => push_categories(L, p.fixtures[fi as usize].mask),
            "setGroupIndex" => {
                let g = n2(2)? as i16;
                p.fixtures[fi as usize].group = g;
                if g != 0 {
                    p.warn("group", "Fixture:setGroupIndex is kept but doesn't filter in 2.0's love.physics: use setCategory and setMask (joined bodies don't collide unless collideConnected)");
                }
                0
            }
            "getGroupIndex" => ret(L, p.fixtures[fi as usize].group as f64),
            "setFilterData" => {
                let (c, m, g) = (n2(2)? as i64, n2(3)? as i64, opt(L, 4, 0.0) as i16);
                let f = &mut p.fixtures[fi as usize];
                f.category = c as u16;
                f.mask = !(m as u16);
                f.group = g;
                regroup(p);
                0
            }
            "getFilterData" => {
                let f = &p.fixtures[fi as usize];
                lua_pushnumber(L, f.category as f64);
                lua_pushnumber(L, (!f.mask) as f64);
                lua_pushnumber(L, f.group as f64);
                3
            }
            "testPoint" => {
                let (x, y) = (n2(2)?, n2(3)?);
                retb(L, p.worlds[wi as usize].w.shape_contains(sid, vec2(x * k, y * k)))
            }
            "getBoundingBox" => {
                let (a, b) = p.worlds[wi as usize].w.shape_bounds(sid);
                for v in [a.x / k, a.y / k, b.x / k, b.y / k] {
                    lua_pushnumber(L, v as f64);
                }
                4
            }
            "getMassData" => {
                let id = p.bodies[bi as usize].id;
                let w = &p.worlds[wi as usize].w;
                let c = w.center_of_mass(id);
                lua_pushnumber(L, (c.x / k) as f64);
                lua_pushnumber(L, (c.y / k) as f64);
                lua_pushnumber(L, w.mass(id) as f64);
                lua_pushnumber(L, (w.inertia(id) / (k * k)) as f64);
                4
            }
            "destroy" => {
                if p.in_callback {
                    return Err("Fixture:destroy can't run inside a world callback: destroy it after world:update".into());
                }
                let f = &mut p.fixtures[fi as usize];
                f.alive = false;
                if f.user != 0 {
                    lua_unref(L, f.user);
                    f.user = 0;
                }
                p.fix_of.remove(&sid.to_bits());
                p.bodies[bi as usize].fixtures.retain(|x| *x != fi);
                p.worlds[wi as usize].w.remove_shape(sid);
                0
            }
            "rayCast" => {
                return Err("Fixture:rayCast isn't in 2.0's love.physics: use World:rayCast".into());
            }
            _ => return Err(unknown_method("Fixture", name, FIXTURE_METHODS)),
        })
    }
}

pub const SHAPE_METHODS: &[&str] = &["getType", "getRadius", "getPoint", "getPoints", "getVertexCount", "getChildCount", "testPoint", "setRadius", "setPoint"];

unsafe fn shape_method(L: *mut lua_State, p: &mut Phys2, si: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        let g = p.shapes[si as usize].clone();
        let ty = match g {
            Geom::Circle { .. } => "CircleShape",
            Geom::Rect { .. } | Geom::Polygon(_) => "PolygonShape",
            Geom::Edge(..) => "EdgeShape",
            Geom::Chain { .. } => "ChainShape",
        };
        if let Some(n) = common(L, name, ty) {
            return Ok(n);
        }
        Ok(match name {
            "typeOf" => retb(L, st(L, 2).is_some_and(|s| s == ty || s == "Shape" || s == "Object")),
            "getType" => rets(L, g.type_name()),
            "getRadius" => ret(L, if let Geom::Circle { r, .. } = g { r as f64 } else { 0.0 }),
            "getPoint" => match g {
                Geom::Circle { x, y, .. } => ret2(L, x as f64, y as f64),
                _ => return Err("Shape:getPoint is a circle's".into()),
            },
            "getPoints" => {
                let pts = g.points();
                for (x, y) in &pts {
                    lua_pushnumber(L, *x as f64);
                    lua_pushnumber(L, *y as f64);
                }
                pts.len() as c_int * 2
            }
            "getVertexCount" => ret(L, g.points().len() as f64),
            "getChildCount" => ret(
                L,
                match &g {
                    Geom::Chain { pts, looped } => (pts.len() - 1 + *looped as usize) as f64,
                    _ => 1.0,
                },
            ),
            "testPoint" => {
                // (x, y, angle, px, py): the shape placed at x, y, turned.
                let f = name;
                let (x, y, a, px, py) = (need_num(L, 2, f)?, need_num(L, 3, f)?, need_num(L, 4, f)?, need_num(L, 5, f)?, need_num(L, 6, f)?);
                let local = vec2(px - x, py - y).rotate(-a);
                retb(
                    L,
                    match g {
                        Geom::Circle { x: cx, y: cy, r } => (local - vec2(cx, cy)).length() <= r,
                        Geom::Rect { .. } | Geom::Polygon(_) => {
                            let pts = g.points();
                            let n = pts.len();
                            let mut sign = 0.0f32;
                            let mut inside = true;
                            for i in 0..n {
                                let (a, b) = (pts[i], pts[(i + 1) % n]);
                                let c = (b.0 - a.0) * (local.y - a.1) - (b.1 - a.1) * (local.x - a.0);
                                if c != 0.0 {
                                    if sign == 0.0 {
                                        sign = c.signum();
                                    } else if c.signum() != sign {
                                        inside = false;
                                    }
                                }
                            }
                            inside
                        }
                        _ => false,
                    },
                )
            }
            "setRadius" => {
                if let Geom::Circle { r, .. } = &mut p.shapes[si as usize] {
                    *r = need_num(L, 2, name)?.abs();
                }
                0
            }
            "setPoint" => {
                if let Geom::Circle { x, y, .. } = &mut p.shapes[si as usize] {
                    *x = need_num(L, 2, name)?;
                    *y = need_num(L, 3, name)?;
                }
                0
            }
            "computeAABB" | "computeMass" | "rayCast" => {
                return Err(format!("Shape:{name} isn't in 2.0's love.physics"));
            }
            _ => return Err(unknown_method("Shape", name, SHAPE_METHODS)),
        })
    }
}

pub const JOINT_METHODS: &[&str] = &[
    "getType",
    "getBodies",
    "getAnchors",
    "getReactionForce",
    "getReactionTorque",
    "getCollideConnected",
    "getUserData",
    "setUserData",
    "destroy",
    "isDestroyed",
    "setLimits",
    "getLimits",
    "setLimitsEnabled",
    "areLimitsEnabled",
    "setLowerLimit",
    "setUpperLimit",
    "getLowerLimit",
    "getUpperLimit",
    "setMotorEnabled",
    "isMotorEnabled",
    "setMotorSpeed",
    "getMotorSpeed",
    "setMaxMotorTorque",
    "getMaxMotorTorque",
    "setMaxMotorForce",
    "getMaxMotorForce",
    "getJointAngle",
    "getJointTranslation",
    "getJointSpeed",
    "setLength",
    "getLength",
    "setMaxLength",
    "getMaxLength",
    "setFrequency",
    "getFrequency",
    "setDampingRatio",
    "getDampingRatio",
    "setSpringFrequency",
    "getSpringFrequency",
    "setSpringDampingRatio",
    "getSpringDampingRatio",
    "setTarget",
    "getTarget",
    "setMaxForce",
    "getMaxForce",
    "getAxis",
];

unsafe fn joint_method(L: *mut lua_State, p: &mut Phys2, ji: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        let kind = p.joints[ji as usize].kind;
        let ty = match kind {
            JKind::Weld => "WeldJoint",
            JKind::Revolute => "RevoluteJoint",
            JKind::Distance => "DistanceJoint",
            JKind::Rope => "RopeJoint",
            JKind::Prismatic => "PrismaticJoint",
            JKind::Wheel => "WheelJoint",
            JKind::Mouse => "MouseJoint",
        };
        if let Some(n) = common(L, name, ty) {
            return Ok(n);
        }
        let alive = p.joints[ji as usize].alive;
        match name {
            "isDestroyed" => return Ok(retb(L, !alive)),
            "destroy" if !alive => return Ok(0),
            _ if !alive => return Err(format!("Joint:{name}: this Joint was destroyed")),
            _ => {}
        }
        let j = &p.joints[ji as usize];
        let (wi, id, a, b) = (j.world, j.id, j.a, j.b);
        let k = p.worlds[wi as usize].k;
        let n2 = |i: c_int| need_num(L, i, name);
        let (ida, idb) = (p.bodies[a as usize].id, p.bodies[b as usize].id);
        let motor_like = matches!(kind, JKind::Revolute | JKind::Prismatic | JKind::Wheel);
        let apply_motor = |p: &mut Phys2| {
            let j = &p.joints[ji as usize];
            let lin = j.kind == JKind::Prismatic;
            let (s, m) = if j.motor_on { (j.speed, j.max_motor) } else { (0.0, 0.0) };
            let (s, m) = if lin { (s * k, m * k) } else { (s, m * k * k) };
            p.worlds[wi as usize].w.set_motor(j.id, s, m);
        };
        let apply_limits = |p: &mut Phys2| {
            let j = &p.joints[ji as usize];
            let lin = matches!(j.kind, JKind::Prismatic | JKind::Wheel);
            let lim = j.limits_on.then_some(if lin { (j.limits.0 * k, j.limits.1 * k) } else { j.limits });
            p.worlds[wi as usize].w.set_limits(j.id, lim);
        };
        let need = |ok: bool| -> Result<(), String> { if ok { Ok(()) } else { Err(format!("{ty} has no method `{name}`")) } };
        Ok(match name {
            "getType" => rets(L, kind.name()),
            "getBodies" => {
                push_obj(L, p, tag::PBODY, a);
                push_obj(L, p, tag::PBODY, b);
                2
            }
            "getAnchors" => {
                let [a, b] = world_anchors(p, ji);
                for v in [a.x, a.y, b.x, b.y] {
                    lua_pushnumber(L, v as f64);
                }
                4
            }
            "getReactionForce" => {
                let inv = opt(L, 2, 60.0) as f32;
                let w = &p.worlds[wi as usize].w;
                let f = w.joint_force(id) * w.solver_iterations() as f32 / k * (inv / 60.0);
                ret2(L, f as f64, 0.0)
            }
            "getReactionTorque" => {
                let inv = opt(L, 2, 60.0) as f32;
                let w = &p.worlds[wi as usize].w;
                ret(L, (w.joint_torque(id) * w.solver_iterations() as f32 / (k * k) * (inv / 60.0)) as f64)
            }
            "getCollideConnected" => retb(L, p.joints[ji as usize].collide),
            "getUserData" => user_get(L, p.joints[ji as usize].user),
            "setUserData" => {
                user_set(L, &mut p.joints[ji as usize].user);
                0
            }
            "destroy" => {
                if p.in_callback {
                    return Err("Joint:destroy can't run inside a world callback (Box2D's rule): destroy it after world:update".into());
                }
                destroy_joint(L, p, ji);
                0
            }
            "setLimits" => {
                need(motor_like)?;
                let (lo, hi) = (n2(2)?, n2(3)?);
                let j = &mut p.joints[ji as usize];
                j.limits = (lo.min(hi), lo.max(hi));
                j.limits_on = true;
                apply_limits(p);
                0
            }
            "setLowerLimit" | "setUpperLimit" => {
                need(motor_like)?;
                let v = n2(2)?;
                let j = &mut p.joints[ji as usize];
                if name == "setLowerLimit" {
                    j.limits.0 = v;
                } else {
                    j.limits.1 = v;
                }
                apply_limits(p);
                0
            }
            "getLimits" => {
                let l = p.joints[ji as usize].limits;
                ret2(L, l.0 as f64, l.1 as f64)
            }
            "getLowerLimit" => ret(L, p.joints[ji as usize].limits.0 as f64),
            "getUpperLimit" => ret(L, p.joints[ji as usize].limits.1 as f64),
            "setLimitsEnabled" | "setLimitEnabled" => {
                need(motor_like)?;
                p.joints[ji as usize].limits_on = opt_bool(L, 2, true);
                apply_limits(p);
                0
            }
            "areLimitsEnabled" | "isLimitEnabled" | "hasLimitsEnabled" => retb(L, p.joints[ji as usize].limits_on),
            "setMotorEnabled" => {
                need(motor_like)?;
                p.joints[ji as usize].motor_on = opt_bool(L, 2, true);
                apply_motor(p);
                0
            }
            "isMotorEnabled" => retb(L, p.joints[ji as usize].motor_on),
            "setMotorSpeed" => {
                need(motor_like)?;
                p.joints[ji as usize].speed = n2(2)?;
                apply_motor(p);
                0
            }
            "getMotorSpeed" => ret(L, p.joints[ji as usize].speed as f64),
            "setMaxMotorTorque" | "setMaxMotorForce" => {
                need(motor_like)?;
                p.joints[ji as usize].max_motor = n2(2)?.max(0.0);
                apply_motor(p);
                0
            }
            "getMaxMotorTorque" | "getMaxMotorForce" => ret(L, p.joints[ji as usize].max_motor as f64),
            "getJointAngle" => {
                let w = &p.worlds[wi as usize].w;
                ret(L, (w.angle(idb) - w.angle(ida) - p.joints[ji as usize].ref_angle) as f64)
            }
            "getJointSpeed" => {
                let w = &p.worlds[wi as usize].w;
                if kind == JKind::Prismatic {
                    let ax = vec2(p.joints[ji as usize].axis.0, p.joints[ji as usize].axis.1).normalize();
                    ret(L, ((w.velocity(idb) - w.velocity(ida)).dot(ax) / k) as f64)
                } else {
                    ret(L, (w.spin(idb) - w.spin(ida)) as f64)
                }
            }
            "getJointTranslation" => {
                let w = &p.worlds[wi as usize].w;
                let j = &p.joints[ji as usize];
                // Along the axis as body A carries it, from A's anchor to B's.
                let axis = vec2(j.axis.0, j.axis.1).normalize().rotate(w.angle(ida) - j.ref_angle_a);
                let [a, b] = world_anchors(p, ji);
                ret(L, (b - a).dot(axis) as f64)
            }
            "getAxis" => {
                let ax = p.joints[ji as usize].axis;
                ret2(L, ax.0 as f64, ax.1 as f64)
            }
            "setLength" | "setMaxLength" => {
                need(matches!(kind, JKind::Distance | JKind::Rope))?;
                let v = n2(2)?.max(0.0);
                p.joints[ji as usize].length = v;
                p.worlds[wi as usize].w.set_joint_length(id, v * k);
                0
            }
            "getLength" | "getMaxLength" => ret(L, p.joints[ji as usize].length as f64),
            "setFrequency" | "setSpringFrequency" => {
                let v = n2(2)?.max(0.0);
                p.joints[ji as usize].freq = v;
                if matches!(kind, JKind::Wheel | JKind::Mouse) {
                    let d = p.joints[ji as usize].damping;
                    p.worlds[wi as usize].w.set_joint_spring(id, v, d);
                } else {
                    p.warn("frequency", "Joint:setFrequency only softens wheel and mouse joints in 2.0's love.physics; weld and distance joints stay rigid");
                }
                0
            }
            "getFrequency" | "getSpringFrequency" => ret(L, p.joints[ji as usize].freq as f64),
            "setDampingRatio" | "setSpringDampingRatio" => {
                let v = n2(2)?.max(0.0);
                p.joints[ji as usize].damping = v;
                if matches!(kind, JKind::Wheel | JKind::Mouse) {
                    let f = p.joints[ji as usize].freq;
                    p.worlds[wi as usize].w.set_joint_spring(id, f, v);
                }
                0
            }
            "getDampingRatio" | "getSpringDampingRatio" => ret(L, p.joints[ji as usize].damping as f64),
            "setTarget" => {
                need(kind == JKind::Mouse)?;
                p.joints[ji as usize].target = (n2(2)?, n2(3)?);
                0
            }
            "getTarget" => {
                let t = p.joints[ji as usize].target;
                ret2(L, t.0 as f64, t.1 as f64)
            }
            "setMaxForce" => {
                need(kind == JKind::Mouse)?;
                let v = n2(2)?.max(0.0);
                p.joints[ji as usize].max_force = v;
                p.worlds[wi as usize].w.set_joint_max_force(id, v * k);
                0
            }
            "getMaxForce" => ret(L, p.joints[ji as usize].max_force as f64),
            _ => return Err(unknown_method(ty, name, JOINT_METHODS)),
        })
    }
}

pub const CONTACT_METHODS: &[&str] = &[
    "getPositions",
    "getNormal",
    "getFixtures",
    "isTouching",
    "setEnabled",
    "isEnabled",
    "getFriction",
    "setFriction",
    "resetFriction",
    "getRestitution",
    "setRestitution",
    "resetRestitution",
    "getChildren",
];

unsafe fn contact_method(L: *mut lua_State, p: &mut Phys2, h: u32, name: &str) -> Result<c_int, String> {
    unsafe {
        if let Some(n) = common(L, name, "Contact") {
            return Ok(n);
        }
        let (epoch, ci) = (h >> 16, (h & 0xffff) as usize);
        if name == "isDestroyed" {
            return Ok(retb(L, epoch != p.epoch || ci >= p.contacts.len()));
        }
        if epoch != p.epoch || ci >= p.contacts.len() {
            return Err(format!("Contact:{name}: this contact is gone (a contact lasts as long as its callback)"));
        }
        let c = &mut p.contacts[ci];
        Ok(match name {
            "getPositions" => {
                for (x, y) in &c.points {
                    lua_pushnumber(L, *x as f64);
                    lua_pushnumber(L, *y as f64);
                }
                c.points.len() as c_int * 2
            }
            "getNormal" => ret2(L, c.normal.0 as f64, c.normal.1 as f64),
            "getFixtures" => {
                let (a, b) = (c.fa, c.fb);
                push_obj(L, p, tag::PFIXTURE, a);
                push_obj(L, p, tag::PFIXTURE, b);
                2
            }
            "isTouching" => retb(L, c.touching),
            "isEnabled" => retb(L, c.enabled),
            "setEnabled" => {
                c.enabled = opt_bool(L, 2, true);
                if !c.enabled {
                    p.warn("enabled", "Contact:setEnabled(false) doesn't stop a collision in 2.0's love.physics (it runs after the step); for one-way platforms use a CharacterBody2D and fixture:setOneWay(true)");
                }
                0
            }
            "getFriction" => ret(L, c.friction as f64),
            "getRestitution" => ret(L, c.restitution as f64),
            "setFriction" | "setRestitution" | "resetFriction" | "resetRestitution" => {
                p.warn("mix", "Contact:setFriction and setRestitution don't change a collision in 2.0's love.physics: set them on the fixtures");
                0
            }
            "getChildren" => ret2(L, 1.0, 1.0),
            _ => return Err(unknown_method("Contact", name, CONTACT_METHODS)),
        })
    }
}

// ------------------------------------------------------------ nodes

/// `node.body`: the body a 2D node follows, or nil.
pub unsafe fn node_body(L: *mut lua_State, h: &mut Host, node: NodeId) -> c_int {
    unsafe {
        let p = phys(h);
        match p.links.iter().find(|l| l.0 == node).map(|l| l.1) {
            Some(b) if p.bodies[b as usize].alive => push_obj(L, p, tag::PBODY, b),
            _ => {
                lua_pushnil(L);
                1
            }
        }
    }
}

/// `node.body = body` (or nil): the node follows the body from now on.
pub unsafe fn set_node_body(L: *mut lua_State, h: &mut Host, node: NodeId, v: c_int) -> Result<(), String> {
    unsafe {
        if h.project.three {
            return Err("node.body is 2D's (love.physics): a 3D body is a RigidBody3D, with the node as its child".into());
        }
        let has_parent = h.scene.get(node).is_some_and(|n| n.parent().is_some());
        let p = phys(h);
        p.links.retain(|l| l.0 != node);
        if lua_type(L, v) == LUA_TNIL {
            return Ok(());
        }
        let Some(b) = obj(L, v, tag::PBODY) else {
            return Err(format!("body takes a love.physics Body, got {}", type_name(L, v)));
        };
        if !p.bodies[b as usize].alive {
            return Err("body: that Body was destroyed".into());
        }
        if has_parent {
            return Err("body: a node that follows a body is in the world, so it can't have a parent (set parent = nil first)".into());
        }
        p.links.push((node, b));
        Ok(())
    }
}

/// After Lua's frame: each node that follows a body goes where the body is
/// drawn (between its last two steps).
pub fn sync(h: &mut Host) {
    super::character2d::forget(h);
    let Some(p) = h.phys2.as_deref_mut() else {
        return;
    };
    p.drawing = false;
    p.links.retain(|(n, b)| h.scene.alive(*n) && p.bodies[*b as usize].alive);
    for &(n, b) in &p.links {
        let body = &p.bodies[b as usize];
        let lw = &p.worlds[body.world as usize];
        let pose = lw.w.draw_pose(body.id);
        if let Some(node) = h.scene.node_mut(n) {
            node.x = pose.pos.x / lw.k;
            node.y = pose.pos.y / lw.k;
            node.rotation = pose.angle;
            node.vx = 0.0;
            node.vy = 0.0;
            node.vr = 0.0;
        }
    }
}

/// The debug menu's Show colliders: every world's bodies, through the
/// scene's camera (`w`, `h`: the canvas).
pub fn draw_debug(h: &Host, cw: f32, ch: f32) {
    let Some(p) = h.phys2.as_deref() else { return };
    if !p.show {
        return;
    }
    use maimbrain::gfx2d;
    let cam = &h.scene.camera;
    for lw in p.worlds.iter().filter(|w| w.alive) {
        gfx2d::push();
        gfx2d::translate(cw / 2.0, ch / 2.0);
        gfx2d::scale(cam.scale, cam.scale);
        gfx2d::rotate(-cam.rot);
        gfx2d::translate(-cam.x - cam.shake_x, -cam.y - cam.shake_y);
        gfx2d::scale(1.0 / lw.k, 1.0 / lw.k);
        lw.w.draw_debug();
        gfx2d::pop();
    }
}

/// `__engine.drawing(on)`: love.draw is running.
pub fn set_drawing(h: &mut Host, on: bool) {
    if let Some(p) = h.phys2.as_deref_mut() {
        p.drawing = on;
    }
}
