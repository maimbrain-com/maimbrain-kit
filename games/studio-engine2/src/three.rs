//! 3D games (`kind = "3d"`, the engine's 3D build; docs/STUDIO-2.md §9.2
//! row 1b, §9.3 row 2a): what a round's VM has made in mb3d, kept with its
//! 3D nodes. The Lua API is three.js's and Godot's names (api/nodes3d.rs,
//! api/world.rs) with engine 1.0's beside them (api/three.rs,
//! prelude3d.lua).
//!
//! - **Nodes** are the SDK scene's 3D side (`maimbrain::scene::three`):
//!   a Lua write changes the node in Rust, and [`World3::end_frame`] sends
//!   the nodes that changed to mb3d at the end of the update in one call
//!   (mb3d 3's `mb3d_node_transforms`), however many writes they had (§4.1,
//!   §4.3). What each node is (`Mesh`, `Model`, `Light`, …) and what it has
//!   beyond a transform is here, in [`Info`].
//! - **Meshes, materials, emitters, trails and loops** are made in mb3d when
//!   the game asks and freed with the VM, so each round builds its world
//!   afresh.
//! - **Models** are the session's: loaded once, behind the title card, and
//!   spawned by each round (§4.7, assets outlive VMs).
//! - **A frame** (§4.1): Lua; then velocities; the flush; the models'
//!   clips, layers and animation trees; their gaits, flight and swims; their
//!   IK (reach, planted feet, head looks); emitters' rates and popups; and
//!   last the follow camera, which sees where everything ended up.
//!
//! Lua sees its own ids (a node's slot and generation, an index into each
//! list), never host handles, so a round is the same on every run and
//! natively, where every host call answers handle 1.

use std::collections::BTreeMap;
use std::ffi::c_int;

use maimbrain::audio::Voice;
use maimbrain::gfx3d::terrain::{Terrain, TerrainDesc};
use maimbrain::gfx3d::{
    self, AnimTree, Animator, Camera, Clip, Emitter, Flight, Humanoid, LookAt, Mask, Material, MaterialDesc, MechGait, Mesh, MeshData, Model, Quat, Skeleton, SpringCamera, Swim, Trail, Transform,
    TwoBoneIk, Vec3, vec3,
};
use maimbrain::scene::three::{Node3Id, Scene3};

/// A mesh built in Lua, uploaded the first time a node draws it (later
/// edits don't reach nodes already made, as in 1.0). `v2` is a geometry
/// made by `geometry.*` (its colours are `0xRRGGBB`), else by 1.0's
/// `mesh.*` (`0xRRGGBBAA`).
pub struct MeshSlot {
    pub data: MeshData,
    pub uploaded: Option<Mesh>,
    pub v2: bool,
}

/// What a 3D node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind3 {
    /// 1.0's `node()` and the parts of a model (`find`, `bone`).
    Node,
    Group,
    Mesh,
    Model,
    Light,
    Emitter,
    Trail,
    Text,
    /// `Terrain{…}`: the ground (its tiles are the session's, [`Ground`]).
    Terrain,
    /// Godot's physics nodes (api/bodies3d.rs, physics3.rs).
    RigidBody,
    StaticBody,
    Area,
    Vehicle,
    Wheel,
    /// A `HingeJoint3D` or another joint (its type is its `Info::src`).
    Joint,
    /// Godot's `CharacterBody3D` (bodies3d.rs, physics3.rs).
    Character,
    /// `NavigationRegion3D` and `NavigationAgent3D` (nav3d.rs, nav3.rs).
    NavRegion,
    NavAgent,
}

impl Kind3 {
    pub fn name(self) -> &'static str {
        match self {
            Kind3::Node => "Node3D",
            Kind3::Group => "Group",
            Kind3::Mesh => "Mesh",
            Kind3::Model => "Model",
            Kind3::Light => "Light",
            Kind3::Emitter => "Emitter",
            Kind3::Trail => "Trail",
            Kind3::Text => "Text",
            Kind3::Terrain => "Terrain",
            Kind3::RigidBody => "RigidBody3D",
            Kind3::StaticBody => "StaticBody3D",
            Kind3::Area => "Area3D",
            Kind3::Vehicle => "VehicleBody3D",
            Kind3::Wheel => "VehicleWheel3D",
            Kind3::Joint => "Joint3D",
            Kind3::Character => "CharacterBody3D",
            Kind3::NavRegion => "NavigationRegion3D",
            Kind3::NavAgent => "NavigationAgent3D",
        }
    }
}

/// A 3D `Emitter`'s particles: the host's emitter, its look, and its rate.
pub struct Emit3 {
    pub emitter: Emitter,
    pub preset: maimbrain::scene::Preset,
    pub rate: f32,
    pub carry: f32,
    pub emitting: bool,
    /// What the emitter was made with (colours, sizes, …), to read back and remake.
    pub desc: gfx3d::EmitterDesc,
}

/// A 3D `Text`'s words and look.
#[derive(Clone, Debug, PartialEq)]
pub struct Text3 {
    pub text: String,
    pub size: f32,
    pub color: u32,
    pub billboard: bool,
    pub font: maimbrain::gfx2d::Font,
}

/// What a node has beyond its transform (by its id's bits).
pub struct Info {
    pub kind: Kind3,
    pub name: String,
    pub tag: String,
    /// `Mesh`: its geometry and material (indices into the lists).
    pub geometry: Option<usize>,
    pub material: Option<usize>,
    /// `Light`'s and `Text`'s colour (`0xRRGGBB`), a light's intensity and range.
    pub color: u32,
    pub intensity: f32,
    pub range: f32,
    pub emit: Option<Box<Emit3>>,
    /// `Trail`: the host's trail (following this node).
    pub trail: Option<(Trail, gfx3d::TrailDesc)>,
    pub text: Option<Box<Text3>>,
    /// `fade = { from, to }`.
    pub fade: Option<(f32, f32)>,
    /// `wake = true`: its slot in mb3d's wakes (0–7).
    pub wake: Option<u32>,
    /// Copies drawn by `instance` or `scatter`.
    pub instances: usize,
    /// `Model`: the model's name (`"fox"`).
    pub src: String,
    /// Made under a model's part (a bone): the node between them that
    /// cancels the bone's own turn and size (see api/nodes3d.rs `socket`).
    pub socket: Option<gfx3d::Node>,
}

impl Info {
    pub fn new(kind: Kind3) -> Info {
        Info {
            kind,
            name: String::new(),
            tag: String::new(),
            geometry: None,
            material: None,
            color: 0xffffff,
            intensity: 1.0,
            range: 6.0,
            emit: None,
            trail: None,
            text: None,
            fade: None,
            wake: None,
            instances: 0,
            src: String::new(),
            socket: None,
        }
    }
}

/// A layer of clips over a model's own (`play(clip, { layer, mask })`):
/// its own animator on the bones of its mask, fading in and out.
pub struct Layer {
    pub name: String,
    pub animator: Animator,
    pub mask: Mask,
    pub weight: f32,
    pub target: f32,
    pub rate: f32,
}

/// A model's rig, found once by the standard bone names (`rig::Humanoid`),
/// and what poses it after the clips.
#[derive(Default)]
pub struct Rig {
    /// None until looked for; Some(None) when the model has no humanoid rig.
    pub humanoid: Option<Option<Humanoid>>,
    pub spine: Option<Option<Skeleton>>,
    /// `reach(bone, x, y, z)`: the limb and its target (world).
    pub reach: Vec<(String, TwoBoneIk, Vec3)>,
    /// `headLook`: the head's turn and where it looks.
    pub head: Option<(LookAt, Option<Vec3>)>,
    pub plant: bool,
    pub legs: Option<[TwoBoneIk; 2]>,
    /// The feet's height above the model's floor at rest (ankles), per side.
    pub ankle: [f32; 2],
    pub gait: Option<MechGait>,
    pub flight: Option<Flight>,
    pub swim: Option<Swim>,
    /// Where the node was last frame, for the velocity gaits and flight need.
    pub last: Option<Vec3>,
    pub heading: f32,
    /// How far planted feet on a terrain have lowered the body (meters, ≤ 0).
    pub sunk: f32,
    /// The rig's hips (its root-most hips, hip or pelvis bone): None until
    /// looked for, Some(None) when it has none.
    pub hips: Option<Option<gfx3d::Node>>,
    /// Each looping clip's hips drift over one cycle (clip index, the hips'
    /// local position at its end less at its start), measured once.
    pub drift: Vec<(u32, Vec3)>,
}

/// The rig's hips bone, by the same names `Humanoid` reads (the one nearest
/// the root), without reading rest poses: safe to look for any time.
fn find_hips(model: Model, root: gfx3d::Node) -> Option<gfx3d::Node> {
    let names = model.node_names();
    let nodes: Vec<(String, gfx3d::Node)> = names.iter().filter_map(|n| Some((n.clone(), root.find(n)?))).collect();
    let pairs: Vec<(String, Option<String>)> =
        nodes.iter().map(|(n, node)| (n.clone(), node.parent().and_then(|p| nodes.iter().find(|x| x.1 == p).map(|x| x.0.clone())))).collect();
    let map = gfx3d::humanoid_map(&pairs);
    let hips = map.iter().find(|m| m.0 == "hips")?;
    nodes.iter().find(|x| x.0 == hips.1).map(|x| x.1)
}

/// How far `clip` moves the hips over one cycle (end less start, in the
/// hips' parent's frame): posed on the hips alone and taken back, so
/// nothing of it is drawn.
fn hips_drift(root: gfx3d::Node, hips: gfx3d::Node, clip: Clip) -> Vec3 {
    let mask = Mask::new().with(hips, 1.0);
    let at = |t: f32| {
        root.pose_masked(clip, t, 1.0, &mask);
        hips.take_pose().map(|p| p.pos)
    };
    match (at(0.0), at(clip.duration)) {
        (Some(a), Some(b)) => b - a,
        _ => Vec3::ZERO,
    }
}

/// A spawned model: the node it's under (a `Model`'s own, or 1.0's spawn
/// root), the model's root in mb3d (the node clips pose), and what plays.
pub struct Anim {
    pub node: Node3Id,
    pub root: gfx3d::Node,
    pub model: Model,
    pub animator: Animator,
    /// Clips asked for by name, as the model has them.
    pub clips: Vec<(String, Clip)>,
    pub layers: Vec<Layer>,
    /// An `AnimationTree` running it (an index into `World3::trees`).
    pub tree: Option<usize>,
    /// The once clip that played to its end was told (`animationFinished`).
    pub told: bool,
    pub rig: Rig,
}

/// An `AnimationTree`: the SDK's state machine, the model it runs, and its
/// state names by index.
pub struct TreeInst {
    pub tree: AnimTree,
    pub model: Node3Id,
}

/// The 3D camera (§5 [camera]): where it is, which way it looks before its
/// roll, the roll, and the lens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cam3 {
    pub pos: Vec3,
    pub rot: Quat,
    pub roll: f32,
    /// Vertical, radians (Lua sees degrees).
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Cam3 {
    /// mb3d's own default (SPEC §5.4): at (0, 0, 5) looking down −z, 1 rad
    /// high, 0.1–500 m, so a game that never sets the camera sees what 1b's did.
    fn default() -> Cam3 {
        Cam3 { pos: vec3(0.0, 0.0, 5.0), rot: Quat::IDENTITY, roll: 0.0, fov: 1.0, near: 0.1, far: 500.0 }
    }
}

impl Cam3 {
    pub fn camera(&self, shake: Vec3) -> Camera {
        Camera { pos: self.pos + shake, rot: (self.rot * Quat::from_rotation_z(self.roll)).normalize(), fov_y: self.fov, near: self.near, far: self.far }
    }
}

/// `camera:follow(node, { behind, above, lag, lookAhead, bank })` in 3D.
pub struct Follow3 {
    pub node: Node3Id,
    pub behind: f32,
    pub above: f32,
    pub lag: f32,
    pub look_ahead: f32,
    /// Roll into the node's turns: radians of roll per radian a second of turn.
    pub bank: f32,
    pub spring: Option<SpringCamera>,
    pub last: Option<(Vec3, f32)>,
}

/// `fx.popup` in 3D: a text that rises and fades.
pub struct Popup3 {
    pub node: gfx3d::Node,
    pub text: String,
    pub desc: gfx3d::TextDesc,
    pub pos: Vec3,
    pub age: f32,
}

pub struct World3 {
    pub scene: Scene3,
    pub info: BTreeMap<u32, Info>,
    pub meshes: Vec<MeshSlot>,
    /// Materials, and whether this round made them (one a model brought
    /// isn't freed with the round).
    pub materials: Vec<(Material, bool)>,
    default_material: Option<Material>,
    pub emitters: Vec<Emitter>,
    /// Trails whose node was destroyed: detached, fading out, and freed in
    /// mb3d (`mb3d_free`) once the round's clock passes the time their fade
    /// ends. mb3d frees a trail only when told (64 at most a session), so
    /// a game that makes one per shot would otherwise run out.
    pub fading: Vec<(Trail, f64)>,
    /// Seconds of world time this round (the sum of `end_frame`'s `dt`).
    pub clock: f64,
    /// `loop(…)`'s sounds (none in the title's attract mode, which is silent).
    pub voices: Vec<Option<Voice>>,
    /// The session's models, by project index (`Project::models`).
    pub models: Vec<Option<Model>>,
    /// Textures made from the project's images (`material{ texture }`), by image.
    pub textures: Vec<(usize, gfx3d::Texture)>,
    pub anims: Vec<Anim>,
    pub trees: Vec<TreeInst>,
    /// `n:find(name)`'s and `bone(name)`'s parts: (where it looked, the name, the part).
    pub found: Vec<(Node3Id, String, Node3Id)>,
    /// The 3D camera, sent to mb3d with the frame's nodes.
    pub cam: Cam3,
    /// A camera was set (by 1.0's `camera(…)` or any of 2.0's).
    pub cam_set: bool,
    pub follow: Option<Follow3>,
    /// The camera's shake this frame (fx.shake), in meters.
    pub shake: Vec3,
    pub popups: Vec<Popup3>,
    /// `fx.burst`'s emitters in 3D, by preset and colours (made once a round).
    pub bursts: Vec<(maimbrain::scene::Preset, Vec<u32>, Emitter)>,
    /// `world.water`'s surface, and `splash`'s particles.
    pub water: Option<gfx3d::Node>,
    pub splash: Option<gfx3d::Splash>,
    /// Wake slots in use (mb3d has 8).
    pub wakes: [bool; 8],
    /// Registry refs: node handles by id (one userdata a node, so `==` and
    /// table keys work), and each node's `data` table.
    pub handles_ref: c_int,
    pub data_ref: c_int,
    /// Nodes the last flush sent (transforms and visibilities), for tests.
    pub flushed: usize,
    /// Signals to hand Lua next frame: (node, signal, numbers).
    pub signals: Vec<(Node3Id, &'static str, Vec<f64>)>,
    /// The round's `Terrain` (one at a time).
    pub ground: Option<Ground>,
    /// The game set the fog itself (`world.fog`), so a terrain's doesn't.
    pub fog_set: bool,
    /// The sky's horizon (linear), for a terrain's fog to match.
    pub horizon: Option<[f32; 3]>,
    /// The game set the camera's far plane itself.
    pub far_set: bool,
    /// The game set the sun itself (`world.sun`, `world.lighting`, 1.0's `sun`).
    pub sun_set: bool,
    /// The round's physics (physics3.rs), made with its first body.
    pub phys: Option<Box<crate::physics3::Phys3>>,
    /// The round's navigation (nav3.rs), made with its first region or agent.
    pub nav: Option<crate::nav3::Nav3>,
    /// Signals with a node to hand Lua next frame: (node, signal, the other node).
    pub node_signals: Vec<(Node3Id, &'static str, Node3Id)>,
    /// 3D text drawn in its fallback while its library font loads.
    pub fonts_pending: Vec<Node3Id>,
}

/// The round's terrain: its node in the scene (for `progress`, `built`,
/// `heightAt` and the rest) and the SDK's terrain, whose tiles outlive the
/// round ([`GROUNDS`]).
pub struct Ground {
    pub node: Node3Id,
    pub terrain: Terrain,
    /// Whether this terrain makes the fog (`fog = true`, and the game set none).
    pub fog: bool,
    /// Bodies land on it (`collision = true`, the default).
    pub collision: bool,
}

thread_local! {
    /// Terrains a round left, kept with their tiles for the next round that
    /// makes the same one: the title card's attract mode builds the first
    /// view, and the round (a fresh VM) takes it over built, as models are
    /// the session's (§4.7). Each is hidden until taken. A terrain made
    /// differently (a live update) frees the ones kept.
    static GROUNDS: std::cell::RefCell<Vec<Terrain>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A new session (natively, several run on one thread): what the last one
/// kept is forgotten, its handles being that session's.
pub fn new_session() {
    GROUNDS.with(|g| g.borrow_mut().clear());
    crate::nav3::new_session();
    crate::api::terrain::new_session();
}

/// A terrain for `desc`: one a round left, restarted, or a new one.
pub fn take_terrain(desc: TerrainDesc) -> Terrain {
    GROUNDS.with(|g| {
        let mut g = g.borrow_mut();
        if let Some(i) = g.iter().position(|t| t.desc == desc) {
            let mut t = g.remove(i);
            t.restart();
            t.set_visible(true);
            return t;
        }
        for old in g.drain(..) {
            old.free();
        }
        Terrain::new(desc)
    })
}

/// A round's terrain, kept hidden for the next round.
pub fn keep_terrain(mut t: Terrain) {
    t.set_visible(false);
    GROUNDS.with(|g| g.borrow_mut().push(t));
}

/// How far the drawn ground can be from the true ground where `scatter`
/// stands copies: level 2 starts here, so a copy's fade ends before it
/// (S11: level 1 is within 0.13 m of it, p95; level 2 0.48 m, 3.3 m at most).
pub fn scatter_fade(t: &Terrain) -> (f32, f32) {
    let end = t.layout().split(2);
    (end * 0.75, end)
}

impl World3 {
    pub fn new(models: Vec<Option<Model>>) -> World3 {
        World3 {
            scene: Scene3::new(),
            info: BTreeMap::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            default_material: None,
            emitters: Vec::new(),
            fading: Vec::new(),
            clock: 0.0,
            voices: Vec::new(),
            models,
            textures: Vec::new(),
            anims: Vec::new(),
            trees: Vec::new(),
            found: Vec::new(),
            cam: Cam3::default(),
            cam_set: false,
            follow: None,
            shake: Vec3::ZERO,
            popups: Vec::new(),
            bursts: Vec::new(),
            water: None,
            splash: None,
            wakes: [false; 8],
            handles_ref: 0,
            data_ref: 0,
            flushed: 0,
            signals: Vec::new(),
            ground: None,
            fog_set: false,
            horizon: None,
            far_set: false,
            sun_set: false,
            phys: None,
            nav: None,
            node_signals: Vec::new(),
            fonts_pending: Vec::new(),
        }
    }

    /// The node's info (a 1.0 node's or a model part's is made on demand).
    pub fn info(&self, id: Node3Id) -> Option<&Info> {
        self.info.get(&id.to_bits())
    }

    pub fn info_mut(&mut self, id: Node3Id) -> &mut Info {
        self.info.entry(id.to_bits()).or_insert_with(|| Info::new(Kind3::Node))
    }

    pub fn kind(&self, id: Node3Id) -> Kind3 {
        self.info(id).map_or(Kind3::Node, |i| i.kind)
    }

    pub fn add_mesh(&mut self, data: MeshData, v2: bool) -> usize {
        self.meshes.push(MeshSlot { data, uploaded: None, v2 });
        self.meshes.len() - 1
    }

    /// Mesh `i` in mb3d, uploaded now if it isn't yet.
    pub fn uploaded(&mut self, i: usize) -> Result<Mesh, String> {
        let m = self.meshes.get_mut(i).ok_or("no such mesh")?;
        if m.uploaded.is_none() {
            m.uploaded = Some(m.data.upload().ok_or("the mesh didn't upload (too many meshes or vertices?)")?);
        }
        Ok(m.uploaded.unwrap())
    }

    /// The material a node gets when none is given: one a round, made when
    /// it's first needed.
    pub fn default_material(&mut self) -> Result<Material, String> {
        if let Some(m) = self.default_material {
            return Ok(m);
        }
        let m = Material::new(&MaterialDesc::default()).ok_or("too many materials")?;
        self.materials.push((m, true));
        self.default_material = Some(m);
        Ok(m)
    }

    /// 1.0's `camera(…)`: a camera from a place, a point to look at and a lens.
    pub fn set_camera(&mut self, c: Camera) {
        self.cam = Cam3 { pos: c.pos, rot: c.rot, roll: 0.0, fov: c.fov_y, near: c.near, far: c.far };
        self.cam_set = true;
        self.far_set = true;
    }

    /// The camera as mb3d has it (the last one sent), for `project` and `ray`.
    pub fn camera(&self) -> Camera {
        self.cam.camera(self.shake)
    }

    /// Forgets what pointed at destroyed nodes, and frees what they had.
    pub fn forget(&mut self, gone: &[Node3Id]) {
        for id in gone {
            if let Some(i) = self.info.remove(&id.to_bits()) {
                if let Some(e) = i.emit {
                    e.emitter.free();
                }
                if let Some((t, d)) = i.trail {
                    // What it drew fades over its lifetime; then it's freed (end_frame).
                    t.detach();
                    self.fading.push((t, self.clock + d.lifetime as f64 + 0.1));
                }
                if let Some(w) = i.wake {
                    self.wakes[w as usize] = false;
                    gfx3d::wake(w, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0);
                }
                if let Some(s) = i.socket {
                    s.destroy();
                }
            }
        }
        if let Some(g) = self.ground.take_if(|g| gone.contains(&g.node)) {
            keep_terrain(g.terrain);
        }
        self.anims.retain(|a| !gone.contains(&a.node));
        self.trees.iter_mut().filter(|t| gone.contains(&t.model)).for_each(|t| t.tree = AnimTree::new());
        self.found.retain(|f| !gone.contains(&f.0) && !gone.contains(&f.2));
        if self.follow.as_ref().is_some_and(|f| gone.contains(&f.node)) {
            self.follow = None;
        }
        if let Some(p) = self.phys.as_deref_mut() {
            p.forget(gone);
        }
        self.nav_forget(gone);
    }

    pub fn anim(&mut self, id: Node3Id) -> Option<&mut Anim> {
        self.anims.iter_mut().find(|a| a.node == id)
    }

    /// A model's two feet as posed this frame, for foley's footsteps
    /// (api/sound.rs): each foot's place and its height over the ground
    /// under it (the terrain, else the node's floor) less its ankle, so 0
    /// is standing. None without the standard leg bones (`plantFeet` finds
    /// them), or with a gait, whose footfalls are its `stepped` signal.
    pub fn feet(&mut self, id: Node3Id) -> Option<[(Vec3, f32); 2]> {
        let pos = self.world_of(id).map(|m| vec3(m.0[12], m.0[13], m.0[14]))?;
        let scale = self.scene.world(id).map_or(1.0, |m| vec3(m.0[4], m.0[5], m.0[6]).length().max(1e-3));
        let ground = self.ground.as_ref().map(|g| &g.terrain);
        let a = self.anims.iter().find(|a| a.node == id).filter(|a| a.rig.gait.is_none())?;
        let (Some(Some(rig)), Some(legs)) = (&a.rig.humanoid, &a.rig.legs) else { return None };
        let mut out = [(Vec3::ZERO, 0.0); 2];
        for (k, leg) in legs.iter().enumerate() {
            let f = rig.skeleton.world(leg_end(leg))?.pos;
            let floor = ground.map_or(pos.y, |t| t.height_at(f.x, f.z));
            out[k] = (f, f.y - floor - a.rig.ankle[k] * scale);
        }
        Some(out)
    }

    /// The node's world transform: the scene's when it knows it, else mb3d's.
    pub fn world_of(&mut self, id: Node3Id) -> Option<gfx3d::Mat4> {
        if let Some(m) = self.scene.world(id) {
            return Some(m);
        }
        let h = self.scene.get(id)?.host?;
        self.scene.flush_batched();
        h.world()
    }

    /// The end of the update (§4.1): velocities and gravity, the nodes that
    /// changed to mb3d (one batched call), the models' animation and what
    /// poses them after it, particles at their rates, popups, then the
    /// camera. `dt` is the world's (0 during a hitstop).
    pub fn end_frame(&mut self, dt: f32) {
        self.scene.step(dt);
        crate::api::nodes3d::fonts_arrived(self);
        self.nav_step(dt);
        self.physics_step(dt);
        self.flushed = self.scene.flush_batched();
        self.animate(dt);
        self.emit(dt);
        self.popups(dt);
        self.wakes(dt);
        self.follow(dt);
        gfx3d::camera(&self.camera());
        // The ground around where the camera ended up.
        let eye = self.cam.pos;
        if let Some(g) = &mut self.ground
            && g.terrain.update(eye)
        {
            self.signals.push((g.node, "built", Vec::new()));
        }
        // Destroyed trails whose fade is over. World time never runs ahead of
        // mb3d's (hitstop and slow motion only hold it back), so a trail is
        // never freed before its ribbon has gone; and the frees depend only
        // on the round's dt, so replays make them at the same frames.
        self.clock += dt as f64;
        let clock = self.clock;
        self.fading.retain(|(t, at)| {
            let done = clock >= *at;
            if done {
                t.free();
            }
            !done
        });
    }

    /// Clips, layers and trees, then gaits, flight and swims, then IK.
    fn animate(&mut self, dt: f32) {
        for i in 0..self.anims.len() {
            let pos = self.world_of(self.anims[i].node).map(|m| vec3(m.0[12], m.0[13], m.0[14]));
            let rot = self.world_of(self.anims[i].node).map(|m| heading_of(&m));
            let tree = self.anims[i].tree;
            let a = &mut self.anims[i];
            let root = a.root;
            // The base: a tree's samples, else the animator's.
            let looped: Vec<(Clip, f32, f32, bool)> = match tree.and_then(|t| self.trees.get_mut(t)) {
                Some(t) => {
                    t.tree.update(dt);
                    t.tree.samples_looping()
                }
                None => {
                    a.animator.update(dt);
                    a.animator.samples_looping().collect()
                }
            };
            let mut base: Vec<(Clip, f32, f32)> = looped.iter().map(|&(c, t, w, _)| (c, t, w)).collect();
            // Loops that don't close: a generated walk whose hips sink
            // through the cycle and jump back up at the loop (Tripo's often
            // do, 10–25 cm). Each looping clip's drift is measured once, and
            // taken out of the hips in step with the clip, so the cycle
            // closes: the same pose at its end as at its start, no jump.
            let hips = *a.rig.hips.get_or_insert_with(|| find_hips(a.model, root));
            if let Some(h) = hips {
                for &(c, _, _, l) in &looped {
                    if l && c.duration > 0.0 && !a.rig.drift.iter().any(|d| d.0 == c.index) {
                        a.rig.drift.push((c.index, hips_drift(root, h, c)));
                    }
                }
            }
            if a.animator.finished() && tree.is_none() {
                if !a.told {
                    a.told = true;
                    self.signals.push((a.node, "animationFinished", Vec::new()));
                }
            } else {
                a.told = false;
            }
            // Layers: each on its mask's bones, the base giving way under it.
            for l in &mut a.layers {
                l.animator.update(dt);
                if l.animator.finished() && l.target > 0.0 {
                    l.target = 0.0;
                }
                if dt > 0.0 {
                    let step = if l.rate.is_infinite() { 1.0 } else { l.rate * dt };
                    l.weight = if l.weight < l.target { (l.weight + step).min(l.target) } else { (l.weight - step).max(l.target) };
                }
            }
            a.layers.retain(|l| l.weight > 0.0 || l.target > 0.0);
            if a.layers.is_empty() {
                for (c, t, w) in base.drain(..) {
                    root.pose(c, t, w);
                }
            } else {
                // The base on every bone but at 1 − the layers' weight on theirs.
                let mut under = Mask::below(a.model, root, root, 1.0);
                for l in &a.layers {
                    let k = l.weight * l.weight * (3.0 - 2.0 * l.weight);
                    for &(n, w) in l.mask.entries() {
                        under.set(n, (under.weight(n) - k * w).max(0.0));
                    }
                }
                for (c, t, w) in base.drain(..) {
                    root.pose_masked(c, t, w, &under);
                }
                for l in &a.layers {
                    let k = l.weight * l.weight * (3.0 - 2.0 * l.weight);
                    let mask = l.mask.clone().scaled(k);
                    for (c, t, w) in l.animator.samples() {
                        root.pose_masked(c, t, w, &mask);
                    }
                }
            }
            // The drift out, in step: by the end of a cycle all of it.
            if let Some(h) = hips {
                let mut off = Vec3::ZERO;
                for &(c, t, w, l) in &looped {
                    if let (true, Some(d)) = (l && c.duration > 0.0, a.rig.drift.iter().find(|d| d.0 == c.index)) {
                        off += d.1 * (w * (t / c.duration).clamp(0.0, 1.0));
                    }
                }
                if off.length() > 1e-5
                    && let Some(mut p) = h.take_pose()
                {
                    p.pos -= off;
                    h.set_transform(&p);
                }
            }
            // Motion from where the node went (writes, tweens, velocities alike).
            let (Some(pos), Some(yaw)) = (pos, rot) else {
                continue;
            };
            let vel = match a.rig.last {
                Some(p) if dt > 0.0 => (pos - p) * (1.0 / dt),
                _ => Vec3::ZERO,
            };
            a.rig.last = Some(pos);
            let turn = if dt > 0.0 { wrap(yaw - a.rig.heading) / dt } else { 0.0 };
            a.rig.heading = yaw;
            let mut offset = Transform::default();
            if let (Some(f), Some(Some(sk))) = (&mut a.rig.flight, &a.rig.spine) {
                f.update(dt, vel);
                offset.pos = f.bob();
                offset.rot = Quat::from_rotation_x(f.pitch()) * Quat::from_rotation_z(f.bank()) * f.rest;
                f.apply(sk);
            }
            if let (Some(s), Some(Some(sk))) = (&mut a.rig.swim, &a.rig.spine) {
                s.update(dt, vel.length());
                s.apply(sk, turn * 0.08);
            }
            let gait = a.rig.gait.is_some();
            if let (Some(g), Some(Some(rig)), Some(legs)) = (&mut a.rig.gait, &a.rig.humanoid, &a.rig.legs) {
                for s in g.update(dt, pos, yaw, vel).to_vec() {
                    self.signals.push((a.node, "stepped", vec![s.foot as f64 + 1.0, s.pos.x as f64, s.pos.y as f64, s.pos.z as f64, s.strength as f64]));
                }
                offset.pos = offset.pos + Quat::from_rotation_y(-yaw) * g.body_offset();
                offset.rot = g.body_tilt() * offset.rot;
                root.set_transform(&offset);
                for (k, leg) in legs.iter().enumerate() {
                    take(&rig.skeleton, leg);
                    // On a terrain the planted feet stand on it: the gait's
                    // ground is the node's height, so lift by the difference.
                    let mut f = g.foot(k);
                    if let Some(t) = self.ground.as_ref().map(|g| &g.terrain) {
                        f.y += t.height_at(f.x, f.z) - pos.y;
                    }
                    leg.apply(&rig.skeleton, f, Quat::from_rotation_y(yaw) * vec3(0.0, 0.0, -1.0));
                }
            } else if a.rig.flight.is_some() {
                root.set_transform(&offset);
            }
            // IK last: planted feet, the limbs reaching, the head.
            if let Some(Some(rig)) = &a.rig.humanoid {
                let forward = Quat::from_rotation_y(yaw) * vec3(0.0, 0.0, -1.0);
                let ground = self.ground.as_ref().map(|g| &g.terrain);
                // The node's size: a rig's rest heights times it are meters.
                let scale = self.scene.world(a.node).map_or(1.0, |m| vec3(m.0[4], m.0[5], m.0[6]).length().max(1e-3));
                let on_terrain = a.rig.plant && !gait && a.rig.legs.is_some() && ground.is_some();
                // Not standing on a terrain any more (plantFeet off, a gait,
                // the terrain gone): raise a body the terrain had lowered. A
                // gait or flight sets the root itself each frame.
                if !on_terrain && a.rig.sunk != 0.0 {
                    if !gait && a.rig.flight.is_none() {
                        root.set_transform(&Transform::default());
                    }
                    a.rig.sunk = 0.0;
                }
                if a.rig.plant
                    && !gait
                    && let Some(legs) = &a.rig.legs
                    && let Some(t) = ground
                {
                    // On a terrain: each foot as high over the ground under
                    // it as the clip has it over the model's floor, the body
                    // lowered for the downhill foot (a third of the hips'
                    // height at most), never into the ground, and a foot on
                    // the ground turned to lie along it. The rig's rest
                    // heights are the model's own units: the node's scale
                    // makes them meters.
                    let feet = [0, 1].map(|k| rig.skeleton.world(leg_end(&legs[k])).map(|f| f.pos));
                    let rise = feet.map(|f| f.map_or(0.0, |f| t.height_at(f.x, f.z) - pos.y));
                    let hips = rig.rest_point("hips").map_or(1.0, |p| p.y.abs().max(0.1)) * scale;
                    // Eased toward, not set: the downhill foot changes as the
                    // feet swap and the ground under them turns, and a body
                    // following that at once bobs. The feet below still never
                    // go into the ground; one may land a little late.
                    let target = rise[0].min(rise[1]).min(0.0).max(-hips * 0.35);
                    let sink = if dt > 0.0 && dt < 0.5 { a.rig.sunk + (target - a.rig.sunk) * (1.0 - (-dt * 10.0).exp()) } else { target };
                    let sink = if (sink - target).abs() < 1e-4 { target } else { sink };
                    if a.rig.flight.is_none() && (sink != 0.0 || a.rig.sunk != 0.0) {
                        root.set_transform(&Transform::at(vec3(0.0, sink / scale, 0.0)));
                        a.rig.sunk = sink;
                    }
                    for (k, leg) in legs.iter().enumerate() {
                        let Some(foot) = rig.skeleton.world(leg_end(leg)) else {
                            continue;
                        };
                        let ankle = a.rig.ankle[k] * scale;
                        let ground = t.height_at(foot.pos.x, foot.pos.z);
                        let want = (foot.pos.y + rise[k] - a.rig.sunk).max(ground + ankle);
                        // How much it stands (1 on the ground, 0 a sole's height up).
                        let stands = 1.0 - ((want - ground - ankle) / (ankle * 2.0).max(0.04)).clamp(0.0, 1.0);
                        let n = t.normal_at(foot.pos.x, foot.pos.z);
                        let sloped = stands > 0.0 && n.y < 0.999;
                        if (want - foot.pos.y).abs() > 1e-3 || sloped {
                            take(&rig.skeleton, leg);
                            leg.apply(&rig.skeleton, vec3(foot.pos.x, want, foot.pos.z), forward);
                        }
                        if sloped && let Some(f) = rig.skeleton.world(leg.end) {
                            let tilt = gfx3d::arc(Vec3::Y, (Vec3::Y + (n - Vec3::Y) * stands).normalize());
                            rig.skeleton.set_world_rot(leg.end, tilt * f.rot);
                        }
                    }
                } else if a.rig.plant
                    && !gait
                    && let Some(legs) = &a.rig.legs
                {
                    for (k, leg) in legs.iter().enumerate() {
                        let Some(foot) = rig.skeleton.world(leg_end(leg)) else {
                            continue;
                        };
                        let floor = pos.y + a.rig.ankle[k] * scale;
                        if foot.pos.y < floor {
                            take(&rig.skeleton, leg);
                            leg.apply(&rig.skeleton, vec3(foot.pos.x, floor, foot.pos.z), forward);
                        }
                    }
                }
                for (bone, limb, target) in &a.rig.reach {
                    take(&rig.skeleton, limb);
                    let pole = if bone.starts_with("hand") { -forward - Vec3::Y } else { forward };
                    limb.apply(&rig.skeleton, *target, pole);
                }
                if let Some((look, at)) = &mut a.rig.head {
                    look.update(dt, &rig.skeleton, *at);
                    if let Some(b) = rig.skeleton.bone(look.bone) {
                        b.take_pose();
                    }
                    look.apply(&rig.skeleton);
                }
            }
        }
        // States entered, as the tree's signal... (none yet: `.state` reads it).
        for t in &mut self.trees {
            t.tree.take_entered();
        }
    }

    /// Emitters at their rates, from where they are now.
    fn emit(&mut self, dt: f32) {
        let ids: Vec<u32> = self.info.iter().filter(|(_, i)| i.emit.as_ref().is_some_and(|e| e.emitting && e.rate > 0.0)).map(|(k, _)| *k).collect();
        for bits in ids {
            let id = Node3Id::from_bits(bits);
            let Some(m) = self.world_of(id) else { continue };
            let Some(e) = self.info.get_mut(&bits).and_then(|i| i.emit.as_mut()) else {
                continue;
            };
            e.carry += e.rate * dt;
            let n = e.carry.floor();
            if n >= 1.0 {
                e.carry -= n;
                let up = m.transform_vector(Vec3::Y).normalize();
                e.emitter.emit(vec3(m.0[12], m.0[13], m.0[14]), up, n as u32);
            }
        }
    }

    fn popups(&mut self, dt: f32) {
        for p in &mut self.popups {
            p.age += dt;
            let k = (p.age / 0.9).min(1.0);
            let mut d = p.desc;
            d.color[3] = 1.0 - k * k;
            p.node.set_text(&d, &p.text);
            p.pos.y += 1.2 * dt;
            p.node.set_transform(&Transform::at(p.pos));
        }
        self.popups.retain(|p| {
            let done = p.age >= 0.9;
            if done {
                p.node.destroy();
            }
            !done
        });
    }

    /// `wake = true` nodes push their wakes along the water.
    fn wakes(&mut self, dt: f32) {
        let ids: Vec<(u32, u32)> = self.info.iter().filter_map(|(k, i)| i.wake.map(|w| (*k, w))).collect();
        for (bits, w) in ids {
            let id = Node3Id::from_bits(bits);
            let Some(n) = self.scene.get(id) else {
                continue;
            };
            let v = vec3(n.vx, n.vy, n.vz);
            if let Some(m) = self.world_of(id) {
                let speed = v.length();
                let _ = dt;
                gfx3d::wake(w, vec3(m.0[12], m.0[13], m.0[14]), v, 0.4, (speed / 3.0).min(1.0));
            }
        }
    }

    /// The follow camera: behind the node's front (its −z), above it, on
    /// springs, looking a little ahead, rolling into its turns.
    fn follow(&mut self, dt: f32) {
        let Some(f) = &self.follow else { return };
        let id = f.node;
        let Some(m) = self.world_of(id) else { return };
        let Some(f) = &mut self.follow else { return };
        let pos = vec3(m.0[12], m.0[13], m.0[14]);
        let yaw = heading_of(&m);
        let front = Quat::from_rotation_y(yaw) * vec3(0.0, 0.0, -1.0);
        let mut want = pos - front * f.behind + Vec3::Y * f.above;
        let look = pos + front * f.look_ahead + Vec3::Y * (f.above * 0.25);
        // Over a terrain: the lines from the node's shins and its middle to
        // the camera clear the ground, so a crest or a hill behind it lifts
        // the camera (or brings it in) and the legs stay in view.
        let ground = self.ground.as_ref().map(|g| &g.terrain);
        if let Some(t) = ground {
            want = clear_of(t, pos + Vec3::Y * 0.3, pos + Vec3::Y * (f.above * 0.5).max(0.5), want, f.behind.max(2.0) * 1.5);
        }
        let spring = f.spring.get_or_insert_with(|| SpringCamera::new(want, look, f.lag.max(1e-3), self.cam.fov));
        let mut c = if f.lag <= 0.0 {
            spring.pos.value = want;
            spring.look.value = look;
            spring.camera()
        } else {
            spring.update(want, look, dt)
        };
        // And never under it, however the spring lags: it slides up the
        // ground (continuous, so no pop), looking where it was looking.
        if let Some(t) = ground {
            let floor = t.height_at(c.pos.x, c.pos.z) + CAMERA_CLEARANCE;
            if c.pos.y < floor {
                spring.pos.value.y = floor;
                c = spring.camera();
            }
        }
        let turn = match f.last {
            Some((_, y)) if dt > 0.0 => wrap(yaw - y) / dt,
            _ => 0.0,
        };
        f.last = Some((pos, yaw));
        self.cam.pos = c.pos;
        self.cam.rot = c.rot;
        if f.bank != 0.0 {
            let want = (turn * f.bank).clamp(-0.6, 0.6);
            self.cam.roll += (want - self.cam.roll) * (1.0 - (-dt * 4.0).exp());
        }
        self.cam_set = true;
    }
}

/// How far over a terrain the follow camera stays, in meters.
pub const CAMERA_CLEARANCE: f32 = 0.6;

/// Where a follow camera goes over a terrain: `to`, lifted until the lines
/// from `low` (the node's shins) and `from` (its middle) to it are clear of
/// the true ground (sampled at 16 points each, the clearance growing to
/// [`CAMERA_CLEARANCE`] at the camera), by up to `most` meters; if that isn't
/// enough, brought in toward `from` to short of where the ground still
/// blocks. Checking from the shins keeps a figure's legs in view over a
/// crest or down a bank, not only its middle. A continuous function of where
/// things are (the spring smooths what's left), deterministic, and 32
/// `height_at`s.
pub fn clear_of(t: &Terrain, low: Vec3, from: Vec3, to: Vec3, most: f32) -> Vec3 {
    const N: usize = 16;
    let need = |from: Vec3, to: Vec3| -> (f32, Option<f32>) {
        let (mut lift, mut first) = (0.0f32, None);
        for k in 1..=N {
            let s = k as f32 / N as f32;
            let p = from + (to - from) * s;
            let short = t.height_at(p.x, p.z) + CAMERA_CLEARANCE * s - p.y;
            if short > 0.0 {
                lift = lift.max(short / s);
                first.get_or_insert(s);
            }
        }
        (lift, first)
    };
    let lift = need(low, to).0.max(need(from, to).0);
    if lift <= 0.0 {
        return to;
    }
    let raised = to + Vec3::Y * lift.min(most);
    if lift <= most {
        return raised;
    }
    match need(from, raised).1 {
        // Short of the first blocked sample, but not into the node.
        Some(s) => from + (raised - from) * (s - 1.0 / N as f32).max(0.25),
        None => raised,
    }
}

/// The yaw (about +y) that turns −z to where `m` points its −z.
pub fn heading_of(m: &gfx3d::Mat4) -> f32 {
    let f = m.transform_vector(vec3(0.0, 0.0, -1.0));
    (-f.x).atan2(-f.z)
}

/// An angle into −π..π.
pub fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI
}

fn leg_end(l: &TwoBoneIk) -> usize {
    l.end
}

/// Takes a limb's bones off this frame's clip samples, so the IK's turns
/// have the last word (mb3d's `take_pose`).
fn take(sk: &Skeleton, l: &TwoBoneIk) {
    for i in [l.upper, l.lower, l.end] {
        if let Some(b) = sk.bone(i) {
            b.take_pose();
        }
    }
}

impl Drop for World3 {
    /// A round's 3D goes with it: its loops stop, its nodes go, then what
    /// they drew with.
    fn drop(&mut self) {
        for v in self.voices.iter().flatten() {
            v.stop();
        }
        for p in &self.popups {
            p.node.destroy();
        }
        if let Some(n) = self.water {
            n.destroy();
        }
        if let Some(s) = &self.splash {
            for e in [s.crown, s.droplets, s.mist, s.spray] {
                e.free();
            }
        }
        for i in self.info.values() {
            if let Some(e) = &i.emit {
                e.emitter.free();
            }
            if let Some((t, _)) = &i.trail {
                t.detach();
                t.free();
            }
        }
        for (t, _) in &self.fading {
            t.free();
        }
        for (w, used) in self.wakes.iter().enumerate() {
            if *used {
                gfx3d::wake(w as u32, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0);
            }
        }
        if let Some(g) = self.ground.take() {
            keep_terrain(g.terrain);
        }
        if let Some(nav) = self.nav.as_mut() {
            for mut r in nav.kept.drain(..).chain(std::mem::take(&mut nav.regions).into_values()) {
                crate::nav3::keep_region(&mut r);
            }
        }
        drop(std::mem::take(&mut self.scene));
        for m in self.meshes.iter().filter_map(|m| m.uploaded) {
            m.free();
        }
        for (m, owned) in &self.materials {
            if *owned {
                m.free();
            }
        }
        for e in self.emitters.iter().chain(self.bursts.iter().map(|b| &b.2)) {
            e.free();
        }
        for (_, t) in &self.textures {
            t.free();
        }
    }
}
