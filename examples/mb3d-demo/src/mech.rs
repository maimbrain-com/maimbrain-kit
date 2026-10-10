//! A rigged mech posed in code (`--param scene=mech`): the HAULER, a
//! generated model rigged by Tripo into a 41-joint biped, through a 10 s
//! beat sheet, all a pure function of the scene's time so replays match:
//!
//! - **0.5–3 s** a sideways strafe and **8.6–9.8 s** back: `MechGait` steps
//!   the planted feet (IK, flat) toward where the body goes, dips the body
//!   after each stomp and rolls it over the planted leg; each `Stomp` kicks
//!   up dust and shakes the camera.
//! - **3.5–6.6 s** a keyed strike (`PoseKeys`): ease into the windup, hold it
//!   cocked with a tremble, a fast ease-in punch with a lunge, a spring stop
//!   (`Kick`), the held follow-through and a slow recovery.
//! - **7.0 s** a hit: a snap to a reel pose, a recoil spring on the chest
//!   and head, pushed back a meter (the gait steps backward), recovery.
//! - Throughout: the rig found by `Humanoid::find` (Tripo's bone names),
//!   poses keyed in the mech's own frame (`turn_model`), its `Idle` clip
//!   added at 30 % on the upper body over the code pose (`add_clip`), the
//!   head tracking the camera (`LookAt`), and a briefcase fitted in its
//!   right hand at rest and attached there (`Humanoid::attach`).

use std::f32::consts::PI;

use maimbrain::gfx3d::{
    self, Camera, Clip, Emitter, EmitterDesc, FxBlend, Humanoid, Lighting, LookAt, Material, MaterialDesc, MechGait, MeshData, Model, Node, Quat, Sky,
    Transform, TwoBoneIk, Vec3, look_at, srgba, vec3,
};
use maimbrain::motion::{Ease, Kick, Lerp, PoseKeys, kick};
use maimbrain::sys::{self, Asset, AssetState};

/// The beat sheet's length (it loops).
const LOOP: f32 = 10.0;
/// Where the mech stands at the start, and the camera.
const HOME: Vec3 = vec3(0.0, 0.0, -9.0);
const EYE: Vec3 = vec3(1.2, 3.4, 3.0);

/// A pose in the mech's frame (+z its front, +y up, its right arm on −x):
/// one turn per bone, relative to the parent, and how low the body sits.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
    low: f32,
    spine: Quat,
    chest: Quat,
    arm_r: Quat,
    fore_r: Quat,
    arm_l: Quat,
    fore_l: Quat,
}

impl Lerp for Pose {
    fn lerp(a: Pose, b: Pose, t: f32) -> Pose {
        Pose {
            low: a.low + (b.low - a.low) * t,
            spine: a.spine.slerp(b.spine, t),
            chest: a.chest.slerp(b.chest, t),
            arm_r: a.arm_r.slerp(b.arm_r, t),
            fore_r: a.fore_r.slerp(b.fore_r, t),
            arm_l: a.arm_l.slerp(b.arm_l, t),
            fore_l: a.fore_l.slerp(b.fore_l, t),
        }
    }
}

fn qx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn qy(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn qz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

/// Guard: fists up and forward, knees a little bent. (A turn about −x swings an arm forward.)
fn guard() -> Pose {
    Pose { low: 0.15, spine: Quat::IDENTITY, chest: Quat::IDENTITY, arm_r: qx(-0.45) * qz(-0.15), fore_r: qx(-0.9), arm_l: qx(-0.35) * qz(0.15), fore_l: qx(-0.8) }
}

/// The tell: crouched, the torso turned away, the right fist cocked back and up.
fn windup() -> Pose {
    Pose { low: 0.5, spine: qx(0.12), chest: qy(-0.5), arm_r: qx(0.8) * qz(-0.55), fore_r: qx(-1.5), arm_l: qx(-0.7) * qz(0.3), fore_l: qx(-1.0) }
}

/// The strike: all in, torso through, the right arm straight at the camera.
fn punch() -> Pose {
    Pose { low: 0.6, spine: qx(0.25), chest: qy(0.45), arm_r: qx(-1.45) * qz(0.1), fore_r: qx(-0.1), arm_l: qx(0.4) * qz(0.5), fore_l: qx(-0.6) }
}

/// Hit: thrown back, arms flung wide.
fn reel() -> Pose {
    Pose { low: 0.35, spine: qx(-0.15), chest: qx(-0.3) * qy(0.15), arm_r: qz(-1.0) * qx(-0.3), fore_r: qx(-0.4), arm_l: qz(1.0) * qx(-0.3), fore_l: qx(-0.4) }
}

fn beats() -> PoseKeys<Pose> {
    PoseKeys::new(guard())
        .to(3.5, guard(), Ease::Linear)
        .to(4.6, windup(), Ease::CubicInOut)
        .hold(0.4)
        .trembling(1.0)
        .to(5.22, punch(), Ease::QuadIn)
        .hold(0.55)
        .to(6.6, guard(), Ease::CubicInOut)
        .to(7.0, guard(), Ease::Linear)
        .to(7.12, reel(), Ease::QuadOut)
        .hold(0.35)
        .to(8.4, guard(), Ease::CubicInOut)
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn ramp(t: f32, a: f32, b: f32) -> f32 {
    smooth((t - a) / (b - a))
}

/// Where the body is (on the ground) at scene time `t`.
fn path(t: f32) -> Vec3 {
    let t = t.rem_euclid(LOOP);
    let strafe = 2.6 * ramp(t, 0.5, 3.0) * (1.0 - ramp(t, 8.6, 9.8));
    let lunge = 1.3 * ramp(t, 4.95, 5.22) * (1.0 - ramp(t, 5.9, 6.7));
    let pushed = -1.1 * ramp(t, 7.0, 7.35) * (1.0 - ramp(t, 8.0, 8.6));
    HOME + vec3(strafe, 0.0, lunge + pushed)
}

/// Its heading: toward the camera.
fn heading(p: Vec3) -> f32 {
    (EYE.x - p.x).atan2(EYE.z - p.z)
}

struct Rigged {
    root: Node,
    rig: Humanoid,
    /// Turns the mech's frame (+z front) into the model's.
    facing: Quat,
    legs: [TwoBoneIk; 2],
    gait: MechGait,
    head: LookAt,
    idle: Option<Clip>,
    upper: maimbrain::gfx3d::Mask,
}

pub struct MechScene {
    body_asset: Asset,
    case_asset: Asset,
    body: Node,
    holder: Node,
    mech: Option<Rigged>,
    dust: Emitter,
    beats: PoseKeys<Pose>,
    t: f32,
    /// The last stomp (scene time, strength), for the camera shake.
    stomp: (f32, f32),
    marked: [bool; 3],
}

impl MechScene {
    pub fn new() -> MechScene {
        gfx3d::sky(&Sky { nebula_density: 0.0, star_density: 0.0, ..Default::default() });
        let lighting = Lighting::hero().key_from((HOME - EYE).normalize());
        gfx3d::lighting(&lighting);
        let floor = Material::new(&MaterialDesc { base_color: srgba(0x6b6f66ff), roughness: 0.92, ..Default::default() }).expect("material");
        Node::with_mesh(gfx3d::Mesh::new(&MeshData::plane(80.0, 80.0)).expect("mesh"), floor).expect("node");
        let lines = Material::new(&MaterialDesc { base_color: srgba(0x4d524aff), roughness: 0.95, ..Default::default() }).expect("material");
        let mut grid = MeshData::new();
        for i in -10..=10 {
            grid.merge(&MeshData::cuboid(vec3(0.05, 0.01, 40.0)).transformed(&Transform::at(vec3(i as f32 * 2.0, 0.005, -10.0)).matrix()));
            grid.merge(&MeshData::cuboid(vec3(40.0, 0.01, 0.05)).transformed(&Transform::at(vec3(0.0, 0.005, -i as f32 * 2.0)).matrix()));
        }
        Node::with_mesh(gfx3d::Mesh::new(&grid).expect("grid"), lines).expect("node");
        let body = Node::new().expect("node");
        let holder = Node::new().expect("node");
        holder.set_parent(Some(body));
        body.set_transform(&Transform::at(path(0.0)).with_rot(qy(heading(path(0.0)))));
        let dust = Emitter::new(&EmitterDesc {
            colors: [[0.55, 0.5, 0.42, 0.7], [0.5, 0.46, 0.4, 0.45], [0.45, 0.42, 0.38, 0.2], [0.4, 0.4, 0.38, 0.0]],
            sizes: [0.35, 0.8, 1.2, 1.5],
            lifetime: [0.6, 1.1],
            speed: [1.0, 2.6],
            spread: 1.3,
            drag: 3.0,
            blend: FxBlend::Alpha,
            gravity: vec3(0.0, 0.3, 0.0),
            ..Default::default()
        })
        .expect("emitter");
        MechScene {
            body_asset: Asset::load("assets/mech_hauler_rig.glb"),
            case_asset: Asset::load("assets/prop_briefcase.glb"),
            body,
            holder,
            mech: None,
            dust,
            beats: beats(),
            t: 0.0,
            stomp: (-10.0, 0.0),
            marked: [false; 3],
        }
    }

    /// Spawns and rigs the mech once its assets are in.
    fn spawn(&mut self) -> Option<()> {
        let model = Model::load(self.body_asset)?;
        let case_model = Model::load(self.case_asset)?;
        let root = model.spawn(Some(self.holder))?;
        root.mark_subject();
        let rig = Humanoid::find(model, root)?;
        sys::log(sys::Level::Info, &format!("hauler rig: {:?}", rig.skeleton.names.iter().zip(&rig.rig_names).collect::<Vec<_>>()));
        // Which way the model faces: from the heels to the toes.
        let fwd = match (rig.rest_point("foot_l"), rig.rest_point("toe_l"), rig.rest_point("foot_r"), rig.rest_point("toe_r")) {
            (Some(a), Some(b), Some(c), Some(d)) => (b - a) + (d - c),
            _ => Vec3::Z,
        };
        let face = fwd.x.atan2(fwd.z);
        // The holder turns the model so its front is the body's +z.
        self.holder.set_transform(&Transform::default().with_rot(qy(-face)));
        let facing = qy(face);
        let legs = [TwoBoneIk::leg(&rig, 'l')?, TwoBoneIk::leg(&rig, 'r')?];
        let gait = MechGait::from_rig(&rig, self.body)?;
        let mut head = LookAt::new(rig.index("head")?);
        head.forward = facing * Vec3::Z;
        head.speed = 2.5;
        let idle = model.clip("Idle");
        let upper = rig.skeleton.mask_below(rig.index("chest")?, 1.0);
        // The briefcase in the right hand: fitted where the hand is at rest, hanging below it, then attached.
        let hand = rig.node("hand_r")?.world()?.transform_point(Vec3::ZERO);
        let case = Node::new()?;
        case_model.spawn(Some(case))?;
        case.set_transform(&Transform { pos: hand - vec3(0.0, 0.42, 0.0), rot: self.body.world().map_or(Quat::IDENTITY, |_| qy(heading(path(0.0)))), scale: Vec3::splat(0.75) });
        rig.attach(case, "hand_r");
        self.mech = Some(Rigged { root, rig, facing, legs, gait, head, idle, upper });
        Some(())
    }

    pub fn update(&mut self, dt: f32) {
        if self.mech.is_none() {
            if self.body_asset.state() == AssetState::Ready && self.case_asset.state() == AssetState::Ready {
                if self.spawn().is_none() {
                    sys::log(sys::Level::Warn, "mech: rig not recognised");
                }
                sys::mark("go");
            }
            return;
        }
        self.t += dt;
        let t = self.t.rem_euclid(LOOP);
        for (i, (at, name)) in [(4.6, "tell"), (5.22, "strike"), (7.0, "hit")].into_iter().enumerate() {
            if !self.marked[i] && self.t >= at {
                self.marked[i] = true;
                sys::mark(name);
            }
        }
        let p = beats_sample(&self.beats, t);
        let Some(m) = &mut self.mech else { return };
        // The body: the path, the gait's dip and roll, the pose's crouch, and the springs.
        let pos = path(self.t);
        let h = 0.02;
        let vel = (path(self.t + h) - path(self.t - h)) * (0.5 / h);
        let yaw = heading(pos);
        let stomps: Vec<_> = m.gait.update(dt, pos, yaw, vel).to_vec();
        for s in &stomps {
            self.dust.emit(s.pos - vec3(0.0, s.pos.y - 0.05, 0.0), Vec3::Y, (10.0 + 14.0 * s.strength) as u32);
            self.stomp = (self.t, s.strength);
        }
        let stop = Kick { at: 5.22, amount: 0.14, hz: 3.2, damping: 0.4 };
        let recoil = Kick { at: 7.0, amount: 0.3, hz: 2.4, damping: 0.35 };
        let pitch = stop.value(t) - 0.5 * recoil.value(t);
        let lean = qx(pitch);
        self.body.set_transform(&Transform::at(pos + m.gait.body_offset() - vec3(0.0, p.low, 0.0)).with_rot(qy(yaw) * m.gait.body_tilt() * lean));
        // The upper body: the keyed pose in the mech's frame, turned into the model's.
        let r = &m.rig;
        r.reset();
        let to_model = |q: Quat| m.facing * q * m.facing.conjugate();
        let tremble = 0.035 * self.beats.tremble(t, 38.0);
        r.turn("spine", to_model(p.spine));
        r.turn("chest", to_model(p.chest * qx(stop.value(t) - recoil.value(t))));
        r.turn("upper_arm_r", to_model(p.arm_r * qx(tremble)));
        r.turn("forearm_r", to_model(p.fore_r));
        r.turn("upper_arm_l", to_model(p.arm_l));
        r.turn("forearm_l", to_model(p.fore_l));
        // The idle clip breathing on top, at 30 %.
        if let Some(idle) = m.idle {
            r.skeleton.add_clip(m.root, idle, t.rem_euclid(idle.duration.max(0.1)), 0.3, Some(&m.upper));
        }
        // The head keeps an eye on the camera (knocked by the hit).
        m.head.update(dt, &r.skeleton, Some(EYE));
        m.head.apply(&r.skeleton);
        // Last: the feet, planted by IK.
        m.gait.apply(r, &m.legs, qy(yaw) * Vec3::Z);
    }

    fn camera(&self) -> Camera {
        let x = path(self.t).x;
        let (at, s) = self.stomp;
        let shake = 0.06 * s * kick(self.t - at, 7.0, 0.25);
        let eye = EYE + vec3(0.7 * x, shake, 1.5);
        let look = vec3(0.95 * x, 2.1, HOME.z) + vec3(0.0, -shake * 0.5, 0.0);
        Camera { pos: eye, rot: look_at(eye, look, Vec3::Y), fov_y: 0.9, near: 0.1, far: 200.0 }
    }

    pub fn render(&self) {
        gfx3d::camera(&self.camera());
        gfx3d::render();
    }
}

fn beats_sample(k: &PoseKeys<Pose>, t: f32) -> Pose {
    let mut p = k.sample(t);
    // Idle life: a slow weight shift.
    p.spine = p.spine * qz(0.02 * (t * 2.0 * PI / 3.3).sin());
    p
}
