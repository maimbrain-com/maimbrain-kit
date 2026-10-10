//! Bones for meshes built in code (mb3d 2): a [`Skeleton`] of ordinary
//! nodes that a skinned mesh (one with `joints` and `weights`, e.g. from
//! [`MeshData::with_chain_weights`](super::MeshData::with_chain_weights))
//! follows, and procedural motion on top: a [`Wave`] traveling down a chain
//! of bones (a banner, a tassel, a tentacle) and [`Swim`], a wave whose beat
//! follows the swimmer's speed.
//!
//! Bones are nodes, so posing is `set_transform` and everything replays: the
//! host only ever sees the transforms you set. The vertex shader does the
//! bending, so a skinned mesh costs about what a plain one does (≤ 64
//! joints per skin, ≤ 128 skinned draws per frame).

use std::f32::consts::TAU;

use super::math::{Mat4, Quat, Transform, Vec3};
use super::Node;
use crate::ffi;

/// One joint of a skin as `mb3d_skin` takes it (68 bytes): the node and its
/// inverse bind matrix (column-major).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Joint {
    pub node: u32,
    pub inverse_bind: [f32; 16],
}

impl Node {
    /// Binds the skinned mesh this node draws (mb3d 2) to `joints`: each a
    /// node and the inverse of where that joint sits, in the mesh's space,
    /// in the mesh's rest pose (its inverse bind matrix). A vertex with
    /// joint `j` then follows `joints[j]`. Make the joints descendants of
    /// this node so moving the node moves the mesh: the node's own
    /// transform is ignored while it is skinned (glTF does the same).
    /// Replaces any skin the node had; an empty list unbinds it, and an
    /// unbound skinned mesh draws rigidly in its rest pose. Most games use
    /// [`Skeleton`] instead. False past 64 joints or with a stale node.
    pub fn set_skin(self, joints: &[(Node, Mat4)]) -> bool {
        let packed: Vec<Joint> = joints.iter().map(|(n, m)| Joint { node: n.raw(), inverse_bind: m.0 }).collect();
        unsafe { ffi::mb3d_skin(self.raw(), packed.as_ptr().cast(), packed.len() as u32) >= 0 }
    }

    /// Hangs this node (a generated part: a shield, a blade, a claw) on
    /// `bone`, keeping it exactly where it is now: its transform becomes its
    /// offset from the bone (bone world⁻¹ × its world), so a part placed by
    /// hand with the model at rest stays put on the arm as the arm moves.
    /// Place it (in the world, or under the model) first, then attach, right
    /// after spawning or with the model posed as you placed it. False for a
    /// dead node.
    /// ```ignore
    /// shield.set_transform(&Transform::at(vec3(0.6, 2.2, 0.4)).with_rot(face_out));   // fitted at rest
    /// shield.attach_to(rig.node("forearm_l")?);                                        // rides the forearm
    /// ```
    pub fn attach_to(self, bone: Node) -> bool {
        let (Some(me), Some(b)) = (self.world(), bone.world()) else { return false };
        self.set_parent(Some(bone));
        self.set_transform(&attach_offset(&b, &me));
        true
    }

    /// Hangs this node on `bone` placed at `at` (a world transform), whatever
    /// its transform was: `attach_at(hand, Transform::at(grip).with_rot(point_down))`.
    pub fn attach_at(self, bone: Node, at: &Transform) -> bool {
        let Some(b) = bone.world() else { return false };
        self.set_parent(Some(bone));
        self.set_transform(&attach_offset(&b, &at.matrix()));
        true
    }

    /// Unbinds the node's skin: its mesh draws rigidly in its rest pose.
    pub fn clear_skin(self) {
        unsafe { ffi::mb3d_skin(self.raw(), std::ptr::null(), 0) };
    }
}

/// Bones for meshes built in code (mb3d 2): nodes under a parent node, each
/// with a rest transform, and the inverse bind matrices that make a mesh
/// built around them follow them.
///
/// ```ignore
/// let koi = MeshData::fish(&desc);                    // skinned to desc.spine()
/// let fish = Node::with_mesh(koi.body.upload()?, skin)?;
/// let bones = Skeleton::chain(fish, &desc.spine())?;  // bones under the fish's node
/// bones.bind(fish);
/// let fins = Node::with_mesh(koi.fins.upload()?, fin_mat)?;
/// fins.set_parent(Some(fish));
/// bones.bind(fins);                                   // several meshes can share bones
/// // eyes, a lantern, a light or an emitter's anchor ride a bone:
/// let head = bones.socket(0, desc.spine()[0] + vec3(0.0, 0.05, 0.0))?;
/// eyes.set_parent(Some(head));
/// // every frame:
/// swim.update(dt, speed);
/// swim.apply(&bones, turn_rate * 0.05);
/// fish.set_transform(&Transform::at(pos).with_rot(Quat::look_rotation(vel, Vec3::Y)));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    /// The bone nodes, in order. Parent things to them (a lantern on a tail).
    pub bones: Vec<Node>,
    /// Each bone's rest transform, relative to its parent bone (or, for a
    /// root bone, to the node the skeleton hangs from).
    pub rest: Vec<Transform>,
    /// Each bone's parent bone, if any.
    pub parents: Vec<Option<usize>>,
    /// Where each bone sits at rest in the skeleton's (the mesh's) space, inverted.
    pub inverse_bind: Vec<Mat4>,
    /// Each bone's name, when given ([`Skeleton::with_names`]); empty otherwise.
    pub names: Vec<String>,
    /// The node the skeleton hangs from: the mesh's node, or the spawned
    /// model's root. Its frame is "the model's frame".
    root: Option<Node>,
    /// The node each bone hangs from (its parent bone, or another node).
    hang: Vec<Node>,
}

impl Skeleton {
    /// Bones with rest transforms (relative to their parent bone, or to
    /// `parent` for bones without one). A bone's parent must come before it.
    /// None past 64 bones, with a bad parent index, or if a node can't be made.
    pub fn new(parent: Node, bones: &[(Option<usize>, Transform)]) -> Option<Skeleton> {
        if bones.is_empty() || bones.len() > 64 {
            return None;
        }
        let mut sk = Skeleton { bones: Vec::new(), rest: Vec::new(), parents: Vec::new(), inverse_bind: Vec::new(), names: Vec::new(), root: Some(parent), hang: Vec::new() };
        let mut model: Vec<Mat4> = Vec::new();
        for (i, &(p, t)) in bones.iter().enumerate() {
            if p.is_some_and(|p| p >= i) {
                return None;
            }
            let node = Node::new()?;
            let hang = p.map_or(parent, |p| sk.bones[p]);
            node.set_parent(Some(hang));
            sk.hang.push(hang);
            node.set_transform(&t);
            let m = match p {
                Some(p) => model[p] * t.matrix(),
                None => t.matrix(),
            };
            sk.inverse_bind.push(m.inverse());
            model.push(m);
            sk.bones.push(node);
            sk.rest.push(t);
            sk.parents.push(p);
        }
        Some(sk)
    }

    /// A chain through `points` (in the space of `parent`, the node drawing
    /// the mesh): bone 0 at `points[0]`, each next bone its predecessor's
    /// child at the next point. Pair it with a mesh skinned by
    /// [`MeshData::with_chain_weights`](super::MeshData::with_chain_weights)
    /// over the same points. 2–64 points.
    pub fn chain(parent: Node, points: &[Vec3]) -> Option<Skeleton> {
        if points.len() < 2 {
            return None;
        }
        let bones: Vec<(Option<usize>, Transform)> =
            points.iter().enumerate().map(|(i, &p)| if i == 0 { (None, Transform::at(p)) } else { (Some(i - 1), Transform::at(p - points[i - 1])) }).collect();
        Skeleton::new(parent, &bones)
    }

    /// Adopts bones a spawned model already has (a glTF skin's joints), by
    /// name, each with the name of the node it hangs from (another bone, a
    /// plain node like `wings`, or None for one hanging straight from
    /// `root`, the node [`Model::spawn`](super::Model::spawn) returned).
    /// Their rest transforms are read from where they are now, so call it
    /// right after spawning, before anything poses them. Then `Wave`,
    /// `Swim`, `Flight`, `turn` and `socket` work on the model like on a
    /// skeleton built in code. None if a name isn't found or past 64 bones.
    pub fn adopt(root: Node, bones: &[(&str, Option<&str>)]) -> Option<Skeleton> {
        if bones.is_empty() || bones.len() > 64 {
            return None;
        }
        let root_w = root.world()?;
        let root_inv = root_w.inverse();
        let mut sk = Skeleton { bones: Vec::new(), rest: Vec::new(), parents: Vec::new(), inverse_bind: Vec::new(), names: Vec::new(), root: Some(root), hang: Vec::new() };
        for &(name, parent) in bones {
            let node = root.find(name)?;
            let pnode = match parent {
                Some(p) => root.find(p)?,
                None => root,
            };
            let w = node.world()?;
            let local = pnode.world()?.inverse() * w;
            sk.rest.push(decompose(&local));
            sk.parents.push(parent.and_then(|p| bones.iter().position(|b| b.0 == p)));
            sk.inverse_bind.push((root_inv * w).inverse());
            sk.bones.push(node);
            sk.hang.push(pnode);
            sk.names.push(name.to_string());
        }
        Some(sk)
    }

    /// Adopts bones of a spawned model by node, each under its real parent
    /// (read from the model), named as given: what [`Humanoid::find`](super::Humanoid::find)
    /// builds on. Rest transforms are read from where they are now, so call
    /// it right after spawning. None past 64 bones or for a dead node.
    pub fn adopt_nodes(root: Node, bones: &[(Node, &str)]) -> Option<Skeleton> {
        if bones.is_empty() || bones.len() > 64 {
            return None;
        }
        let root_inv = root.world()?.inverse();
        let mut sk = Skeleton { bones: Vec::new(), rest: Vec::new(), parents: Vec::new(), inverse_bind: Vec::new(), names: Vec::new(), root: Some(root), hang: Vec::new() };
        for &(node, name) in bones {
            let pnode = node.parent().unwrap_or(root);
            let w = node.world()?;
            sk.rest.push(decompose(&(pnode.world()?.inverse() * w)));
            sk.parents.push(bones.iter().position(|b| b.0 == pnode));
            sk.inverse_bind.push((root_inv * w).inverse());
            sk.bones.push(node);
            sk.hang.push(pnode);
            sk.names.push(name.to_string());
        }
        Some(sk)
    }

    /// The node the skeleton hangs from (its frame is the model's frame).
    pub fn root(&self) -> Option<Node> {
        self.root
    }

    /// Bone `i`'s rest rotation in the model's frame (what turns its own
    /// axes into the model's).
    pub fn rest_rot(&self, i: usize) -> Quat {
        self.inverse_bind.get(i).map_or(Quat::IDENTITY, |m| decompose(&m.inverse()).rot)
    }

    /// Turns bone `i` by `rot` given in **the model's frame** (+y up, the
    /// model's own forward), relative to its parent, whatever axes the rig
    /// gave the bone: `turn_model(elbow, Quat::from_rotation_x(-1.2))` bends
    /// an elbow the same way on a Tripo, Mixamo or Blender rig. With the
    /// parents at rest the bone ends up turned by exactly `rot` about its
    /// joint; turns down a chain compose (a chest turn carries the head).
    /// (`turn` is in the bone's own frame, which on a generated rig points
    /// anywhere.)
    pub fn turn_model(&self, i: usize, rot: Quat) {
        let w = self.rest_rot(i);
        self.turn(i, w.conjugate() * rot * w);
    }

    /// Bone `i`'s posed rotation and position in the world, now (after this
    /// frame's animation and what the game set so far). Off wasm, the identity.
    pub fn world(&self, i: usize) -> Option<Transform> {
        Some(decompose(&self.bones.get(i)?.world()?))
    }

    /// Turns bone `i` so that its world rotation is `rot` (keeping its rest
    /// position and scale), whatever its parents are doing: what IK and
    /// planted feet need.
    pub fn set_world_rot(&self, i: usize, rot: Quat) {
        let (Some(h), Some(r)) = (self.hang.get(i), self.rest.get(i)) else { return };
        let Some(hw) = h.world() else { return };
        let hr = decompose(&hw).rot;
        self.bones[i].set_transform(&Transform { rot: (hr.conjugate() * rot).normalize(), ..*r });
    }

    /// Turns bone `i` to `rot` × its rest rotation in the model's frame,
    /// whatever its parents are doing (`rot` is absolute, not relative to the
    /// parent as in [`turn_model`](Self::turn_model)): a head that keeps
    /// looking at something while the body sways.
    pub fn orient(&self, i: usize, rot: Quat) {
        let root = self.root.and_then(|r| r.world()).map_or(Quat::IDENTITY, |m| decompose(&m).rot);
        self.set_world_rot(i, root * rot * self.rest_rot(i));
    }

    /// The model root's world rotation with its pitch and roll taken out
    /// (only its heading): the ground frame flat feet keep.
    pub fn ground_rot(&self) -> Quat {
        let r = self.root.and_then(|r| r.world()).map_or(Quat::IDENTITY, |m| decompose(&m).rot);
        heading(r)
    }
}

/// The heading (yaw about +y) of a rotation: where it turns +z, flattened.
pub fn heading(r: Quat) -> Quat {
    let f = r * Vec3::Z;
    if f.x.abs() + f.z.abs() < 1e-6 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_y(f.x.atan2(f.z))
}

impl Skeleton {
    /// The bones `mb model --spine` gives a generated model: `spine_0` (the
    /// head) … `spine_N-1` (the tail), then, for a bird, `wing_l_0`,
    /// `wing_l_1`, `wing_r_0`, `wing_r_1` (left is −x on a model facing −z).
    /// Named, so `bones.find("wing_l_0")` works.
    /// ```ignore
    /// let dragon = model.spawn(None)?;
    /// let bones = Skeleton::spine(dragon)?;     // right after spawning
    /// let mut swim = Swim::new(4.5);
    /// swim.wave.axis = Vec3::X;                 // undulate up and down, like a flying dragon
    /// // every frame:
    /// swim.update(dt, speed);
    /// swim.apply(&bones, 0.0);
    /// ```
    pub fn spine(root: Node) -> Option<Skeleton> {
        let names: Vec<String> = (0..64).map(|i| format!("spine_{i}")).take_while(|n| root.find(n).is_some()).collect();
        if names.len() < 2 {
            return None;
        }
        let mut list: Vec<(&str, Option<&str>)> = names.iter().enumerate().map(|(i, n)| (n.as_str(), (i > 0).then(|| names[i - 1].as_str()))).collect();
        // Off wasm `find` finds everything; only count wings a real spawn has.
        if root.find("wings").is_some() && root.find("wing_l_0").is_some() && cfg!(target_arch = "wasm32") {
            list.extend([("wing_l_0", Some("wings")), ("wing_l_1", Some("wing_l_0")), ("wing_r_0", Some("wings")), ("wing_r_1", Some("wing_r_0"))]);
        }
        Skeleton::adopt(root, &list)
    }

    /// How many bones are in the head-to-tail chain (`spine_*`, or all of
    /// them for a skeleton without names): what `Swim`/`Wave` should bend.
    pub fn spine_len(&self) -> usize {
        let n = self.names.iter().take_while(|n| n.starts_with("spine_")).count();
        if n == 0 { self.bones.len() } else { n }
    }

    pub fn len(&self) -> usize {
        self.bones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// Bone `i`'s node (0 is the first: a chain's head), to parent things to:
    /// `lantern.set_parent(Some(bones.bone(7)?))` and the lantern rides the
    /// tail. Bones are ordinary nodes, so anything a node can carry works
    /// (meshes, point lights, 3D text, a trail, an emitter's anchor via
    /// `world()`). None past the last bone.
    pub fn bone(&self, i: usize) -> Option<Node> {
        self.bones.get(i).copied()
    }

    /// Every bone's node, in order (the same as the `bones` field).
    pub fn bones(&self) -> &[Node] {
        &self.bones
    }

    /// The last bone (a chain's tip: a tail, a tassel's end).
    pub fn tip(&self) -> Option<Node> {
        self.bones.last().copied()
    }

    /// Names the bones, in order, so code can say `bones.find("head")`
    /// instead of an index. Extra names are ignored; missing ones are "".
    /// ```ignore
    /// let bones = Skeleton::chain(koi, &desc.spine())?.with_names(&["head", "neck"]);
    /// eyes.set_parent(bones.find("head").and_then(|i| bones.bone(i)));
    /// ```
    pub fn with_names(mut self, names: &[&str]) -> Skeleton {
        self.names = (0..self.bones.len()).map(|i| names.get(i).copied().unwrap_or("").to_string()).collect();
        self
    }

    /// Bone `i`'s name ("" or None when it has none).
    pub fn name(&self, i: usize) -> Option<&str> {
        self.names.get(i).map(String::as_str)
    }

    /// The index of the first bone named `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Where bone `i` sits in the rest pose, in the skeleton's (the mesh's)
    /// space: for a chain, its point.
    pub fn rest_point(&self, i: usize) -> Option<Vec3> {
        Some(self.inverse_bind.get(i)?.inverse().transform_point(Vec3::ZERO))
    }

    /// `at` (a point in the mesh's space, in its rest pose) in bone `i`'s
    /// own frame: where a child of the bone must sit to be at `at` at rest.
    pub fn local_point(&self, i: usize, at: Vec3) -> Option<Vec3> {
        Some(self.inverse_bind.get(i)?.transform_point(at))
    }

    /// A new node riding bone `i`, placed at `at` (a point in the mesh's
    /// space, in its rest pose: where the eyes are on the model you built):
    /// it follows the bone as it turns, like a vertex skinned only to it.
    /// Parent eyes, a hat, a light or a lantern to it. None past the last
    /// bone or if a node can't be made.
    pub fn socket(&self, i: usize, at: Vec3) -> Option<Node> {
        let local = self.local_point(i, at)?;
        let node = Node::new()?;
        node.set_parent(Some(self.bone(i)?));
        node.set_transform(&Transform::at(local));
        Some(node)
    }

    /// Makes `node`'s skinned mesh follow these bones (see [`Node::set_skin`]).
    /// `node` should be the one the skeleton hangs from, or another node
    /// with the same transform (a fish's fins beside its body).
    pub fn bind(&self, node: Node) -> bool {
        let joints: Vec<(Node, Mat4)> = self.bones.iter().copied().zip(self.inverse_bind.iter().copied()).collect();
        node.set_skin(&joints)
    }

    /// Poses bone `i`: its rest transform, turned by `rot` (in the bone's own frame).
    pub fn turn(&self, i: usize, rot: Quat) {
        if let (Some(b), Some(r)) = (self.bones.get(i), self.rest.get(i)) {
            b.set_transform(&Transform { rot: r.rot * rot, ..*r });
        }
    }

    /// Puts every bone back in its rest pose.
    pub fn reset(&self) {
        for (b, r) in self.bones.iter().zip(&self.rest) {
            b.set_transform(r);
        }
    }
}

/// The local transform that puts a child at world matrix `child` under a
/// parent at world matrix `parent`: decompose(parent⁻¹ × child).
pub fn attach_offset(parent: &Mat4, child: &Mat4) -> Transform {
    decompose(&(parent.inverse() * *child))
}

/// Translation, rotation and scale of an affine matrix (shear is lost).
fn decompose(m: &Mat4) -> Transform {
    let c = |i: usize| Vec3::new(m.0[i * 4], m.0[i * 4 + 1], m.0[i * 4 + 2]);
    let (x, y, z) = (c(0), c(1), c(2));
    let mut s = Vec3::new(x.length(), y.length(), z.length());
    if x.cross(y).dot(z) < 0.0 {
        s.x = -s.x;
    }
    let d = |v: Vec3, k: f32| if k.abs() > 1e-12 { v * (1.0 / k) } else { v };
    Transform { pos: c(3), rot: Quat::from_basis(d(x, s.x), d(y, s.y), d(z, s.z)), scale: s }
}

/// A wave traveling down a chain of bones: each bone turns about `axis` by
/// a sine whose phase lags one bone to the next, so a bend runs from the
/// first bone to the last. A fish's swim, a banner flapping, a tassel or a
/// tentacle swaying, seaweed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    /// The axis each bone turns about, in its own frame: `Vec3::Y` for a fish
    /// (side to side) or a flag on a vertical pole, `Vec3::X` or `Vec3::Z`
    /// for something hanging.
    pub axis: Vec3,
    /// The last bone's angle, in radians (each bone's turn is relative to the
    /// bone before it, so the tip moves farthest).
    pub amplitude: f32,
    /// The first bone's angle as a fraction of `amplitude`; it grows linearly
    /// to 1 at the last bone. Fish: 0.1–0.3 (the head barely moves).
    pub head: f32,
    /// How many wavelengths fit along the chain: 0 sways it as one piece,
    /// ~0.8–1 is a koi's swim, 1.5–3 a ripple running down a banner.
    pub waves: f32,
}

impl Default for Wave {
    fn default() -> Wave {
        Wave { axis: Vec3::Y, amplitude: 0.35, head: 0.2, waves: 0.9 }
    }
}

impl Wave {
    /// Bone `i`'s angle (of `bones`) at `phase` radians (advance the phase by
    /// 2π × frequency × dt each frame; the wave then runs head to tail).
    pub fn angle(&self, i: usize, bones: usize, phase: f32) -> f32 {
        let k = if bones > 1 { i as f32 / (bones - 1) as f32 } else { 1.0 };
        let amp = self.amplitude * (self.head + (1.0 - self.head) * k);
        amp * (phase - TAU * self.waves * k).sin()
    }

    /// Turns every bone of `skeleton` from its rest pose by the wave at
    /// `phase`, plus `bend` radians each (a steady curve: into a turn, or a
    /// banner pushed by the wind).
    pub fn apply(&self, skeleton: &Skeleton, phase: f32, bend: f32) {
        let n = skeleton.spine_len();
        for i in 0..n {
            skeleton.turn(i, Quat::from_axis_angle(self.axis, self.angle(i, n, phase) + bend));
        }
    }
}

/// Procedural swimming: a body [`Wave`] whose beat follows the speed (real
/// fish beat their tails about once per 0.6–0.8 body lengths travelled) and
/// grows a little stronger when they go faster, plus a bend into turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swim {
    pub wave: Wave,
    /// Body length (meters).
    pub length: f32,
    /// Body lengths travelled per tail beat (0.6–0.8 for most fish).
    pub stride: f32,
    /// Beats per second when not moving (a gentle idle sway).
    pub idle: f32,
    /// Speed (m/s) at which `wave.amplitude` is reached; slower swims at ~45 % of it.
    pub cruise: f32,
    phase: f32,
    strength: f32,
}

impl Swim {
    /// A koi-like swim for a body `length` meters long.
    pub fn new(length: f32) -> Swim {
        Swim { wave: Wave::default(), length, stride: 0.7, idle: 0.6, cruise: length * 1.5, phase: 0.0, strength: 0.45 }
    }

    /// Starts the beat at `phase` radians (so a school doesn't beat in unison).
    pub fn with_phase(mut self, phase: f32) -> Swim {
        self.phase = phase;
        self
    }

    /// Advances the beat by `dt` seconds at `speed` m/s.
    pub fn update(&mut self, dt: f32, speed: f32) {
        let speed = speed.abs();
        let beat = self.idle + speed / (self.stride * self.length).max(1e-3);
        self.phase = (self.phase + TAU * beat * dt) % (TAU * 64.0);
        // Ease the strength toward the speed's so a sprint swells smoothly.
        let want = 0.45 + 0.55 * (speed / self.cruise.max(1e-3)).min(1.0);
        self.strength += (want - self.strength) * (1.0 - (-dt * 4.0).exp());
    }

    /// The beat's phase in radians.
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Poses `skeleton`. `turn` bends the body into a turn: pass the turn rate
    /// (radians per second, + to the left) times ~0.05–0.1.
    pub fn apply(&self, skeleton: &Skeleton, turn: f32) {
        let wave = Wave { amplitude: self.wave.amplitude * self.strength, ..self.wave };
        wave.apply(skeleton, self.phase, turn / skeleton.spine_len().max(1) as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::vec3;

    /// The skill's koi and banner examples, kept compiling.
    #[test]
    fn skill_koi_example() {
        use crate::gfx3d::{FishDesc, Material, MaterialDesc, MeshData};
        use std::f32::consts::TAU;
        let skin = Material::new(&MaterialDesc { roughness: 0.3, ..Default::default() }).unwrap();
        let fin_mat = Material::new(&MaterialDesc { flags: Material::ALPHA | Material::DOUBLE_SIDED, ..Default::default() }).unwrap();
        let desc = FishDesc { fins: 2.0, ..Default::default() };
        let fish = MeshData::fish(&desc);
        let mut body = fish.body.with_vertex_colors_by(|p, n| if n.y > -0.3 && (p.z * 11.0).sin() + (p.x * 9.0 + p.y * 7.0).cos() > 0.6 { 0xe0401cff } else { 0xf6f2eaff });
        body.merge(&fish.eyes.with_vertex_colors(0x0a0a0cff));
        assert!(body.colors.iter().any(|&c| c == 0xe0401cff) && body.colors.iter().any(|&c| c == 0xf6f2eaff));
        let koi = Node::with_mesh(body.upload().unwrap(), skin).unwrap();
        let fins = Node::with_mesh(fish.fins.upload().unwrap(), fin_mat).unwrap();
        fins.set_parent(Some(koi));
        let bones = Skeleton::chain(koi, &desc.spine()).unwrap();
        assert!(bones.bind(koi) && bones.bind(fins));
        let mut swim = Swim::new(desc.length).with_phase(1.0);
        let (pos, vel, dt, turn_rate) = (Vec3::ZERO, vec3(0.0, 0.0, -1.0), 1.0 / 60.0, 0.3);
        koi.set_transform(&Transform { pos, rot: Quat::look_rotation(vel, Vec3::Y), scale: Vec3::ONE });
        swim.update(dt, vel.length());
        swim.apply(&bones, turn_rate * 0.1);

        let cloth = MeshData::loft(&crate::gfx3d::Loft { spine: vec![Vec3::ZERO, vec3(1.4, 0.0, 0.0)], closed: false, ..Default::default() }, |_, u| vec3(0.0, -u * 0.6, 0.0));
        let pts: Vec<Vec3> = (0..8).map(|i| vec3(i as f32 * 0.2, 0.0, 0.0)).collect();
        let flag = Node::with_mesh(cloth.with_chain_weights(&pts).upload().unwrap(), fin_mat).unwrap();
        let flag_bones = Skeleton::chain(flag, &pts).unwrap();
        flag_bones.bind(flag);
        let mut phase = 0.0;
        phase += TAU * 0.9 * dt;
        Wave { axis: Vec3::Y, amplitude: 0.3, head: 0.35, waves: 1.2 }.apply(&flag_bones, phase, 0.0);
    }

    #[test]
    fn bones_are_reachable_by_index_and_name() {
        let parent = Node::new().unwrap();
        let pts = [vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 0.0), vec3(0.0, 0.5, 1.0)];
        let sk = Skeleton::chain(parent, &pts).unwrap().with_names(&["head", "body"]);
        assert_eq!(sk.bone(1), Some(sk.bones[1]));
        assert_eq!(sk.bones(), &sk.bones[..]);
        assert_eq!(sk.tip(), Some(sk.bones[2]));
        assert_eq!(sk.bone(3), None);
        assert_eq!((sk.find("head"), sk.find("body"), sk.find("tail")), (Some(0), Some(1), None));
        assert_eq!((sk.name(0), sk.name(2), sk.name(3)), (Some("head"), Some(""), None), "unnamed bones are \"\"");
        assert!(Skeleton::chain(parent, &pts).unwrap().names.is_empty(), "no names unless given");
        for (i, p) in pts.iter().enumerate() {
            assert!((sk.rest_point(i).unwrap() - *p).length() < 1e-6, "bone {i} at rest is at its point");
        }
    }

    #[test]
    fn sockets_sit_where_the_mesh_says_in_the_bones_frame() {
        let parent = Node::new().unwrap();
        // A bone turned and moved: the socket's offset undoes its rest transform.
        let a = Transform::at(vec3(1.0, 0.0, 0.0)).with_rot(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let sk = Skeleton::new(parent, &[(None, a), (Some(0), Transform::at(vec3(0.0, 0.0, -2.0)))]).unwrap();
        let eye = vec3(1.2, 0.3, 0.0);
        let local = sk.local_point(0, eye).unwrap();
        assert!((a.matrix().transform_point(local) - eye).length() < 1e-5, "{local:?}");
        // The second bone's rest point is two meters along the first bone's −z, which the turn points along −x.
        assert!((sk.rest_point(1).unwrap() - vec3(-1.0, 0.0, 0.0)).length() < 1e-5, "{:?}", sk.rest_point(1));
        assert!(sk.socket(1, eye).is_some());
        assert!(sk.socket(2, eye).is_none() && sk.local_point(2, eye).is_none());
    }

    #[test]
    fn model_frame_turns_ignore_the_bones_axes() {
        let parent = Node::new().unwrap();
        // A bone whose own axes are turned arbitrarily (as a generated rig's are).
        let a = Quat::from_euler(0.7, -0.4, 1.1);
        let b = Quat::from_euler(-0.2, 0.9, 0.3);
        let sk = Skeleton::new(parent, &[(None, Transform::at(vec3(0.0, 1.0, 0.0)).with_rot(a)), (Some(0), Transform::at(vec3(0.0, 0.5, 0.0)).with_rot(b))]).unwrap();
        assert!(sk.rest_rot(1).dot(a * b).abs() > 0.99999);
        // turn_model's local delta, conjugated back into the model frame, is the turn asked for.
        let want = Quat::from_rotation_x(-1.2);
        let w = sk.rest_rot(1);
        let local = w.conjugate() * want * w;
        let posed = sk.rest_rot(0) * (b * local); // parent at rest × the bone's new local rotation
        assert!(posed.dot(want * w).abs() > 0.99999, "the bone turned by `want` in the model's frame");
        assert!(sk.root() == Some(parent));
        // Headings keep only the yaw.
        let h = heading(Quat::from_rotation_y(0.8) * Quat::from_rotation_x(0.3) * Quat::from_rotation_z(-0.2));
        assert!(h.dot(Quat::from_rotation_y(0.8)).abs() > 0.999, "{h:?}");
        sk.turn_model(1, want);
        sk.orient(1, want);
        sk.set_world_rot(0, want);
        assert_eq!(sk.ground_rot(), Quat::IDENTITY);
    }

    #[test]
    fn attached_parts_keep_where_they_were() {
        // A forearm somewhere, turned and scaled (a model's own scale), and a shield placed by hand.
        let arm = Transform { pos: vec3(0.5, 2.0, 0.1), rot: Quat::from_euler(0.4, -1.1, 0.7), scale: Vec3::splat(0.01) };
        let shield = Transform { pos: vec3(0.8, 2.1, 0.5), rot: Quat::from_rotation_y(0.3), scale: Vec3::splat(1.5) };
        let off = attach_offset(&arm.matrix(), &shield.matrix());
        let back = arm.matrix() * off.matrix();
        assert!(back.0.iter().zip(shield.matrix().0).all(|(a, b)| (a - b).abs() < 1e-3), "at rest it's where it was");
        // The arm swings: the shield swings with it, its offset from the arm kept.
        let swung = Transform { rot: Quat::from_rotation_x(0.9) * arm.rot, ..arm };
        let moved = (swung.matrix() * off.matrix()).transform_point(Vec3::ZERO);
        let rel = |m: Mat4, p: Vec3| m.inverse().transform_point(p);
        assert!((rel(swung.matrix(), moved) - rel(arm.matrix(), shield.pos)).length() < 1e-2);
        let n = Node::new().unwrap();
        assert!(n.attach_to(Node::new().unwrap()) && n.attach_at(n, &shield));
    }

    #[test]
    fn joints_pack_as_the_abi_says() {
        assert_eq!(size_of::<Joint>(), 68);
    }

    #[test]
    fn chains_bind_at_their_points() {
        let parent = Node::new().unwrap();
        let pts = [vec3(0.0, 0.0, -1.0), vec3(0.0, 0.0, 0.0), vec3(0.0, 0.5, 1.0)];
        let sk = Skeleton::chain(parent, &pts).unwrap();
        assert_eq!(sk.len(), 3);
        assert_eq!(sk.parents, vec![None, Some(0), Some(1)]);
        assert_eq!(sk.rest[2].pos, vec3(0.0, 0.5, 1.0));
        // The inverse bind matrix takes each joint's point to its origin.
        for (m, p) in sk.inverse_bind.iter().zip(pts) {
            assert!(m.transform_point(p).length() < 1e-6);
        }
        assert!(sk.bind(parent));
        assert!(Skeleton::chain(parent, &pts[..1]).is_none());
        assert!(Skeleton::new(parent, &[(Some(0), Transform::default())]).is_none(), "a parent must come first");
    }

    #[test]
    fn inverse_bind_matrices_undo_rotated_and_scaled_bones() {
        let parent = Node::new().unwrap();
        let a = Transform { pos: vec3(1.0, 2.0, 3.0), rot: Quat::from_euler(0.3, -0.7, 0.2), scale: vec3(1.0, 2.0, 0.5) };
        let b = Transform::at(vec3(0.0, 1.0, 0.0)).with_rot(Quat::from_rotation_z(0.9));
        let sk = Skeleton::new(parent, &[(None, a), (Some(0), b)]).unwrap();
        let world = a.matrix() * b.matrix();
        let id = world * sk.inverse_bind[1];
        assert!(id.0.iter().zip(Mat4::IDENTITY.0).all(|(x, y)| (x - y).abs() < 1e-5), "{id:?}");
    }

    #[test]
    fn waves_travel_head_to_tail_and_grow_toward_it() {
        let w = Wave { axis: Vec3::Y, amplitude: 0.4, head: 0.25, waves: 1.0 };
        let n = 8;
        // The tail's peak comes later than the head's.
        let peak_at = |i: usize| (0..360).map(|k| k as f32 * TAU / 360.0).max_by(|a, b| w.angle(i, n, *a).total_cmp(&w.angle(i, n, *b))).unwrap();
        assert!(peak_at(0) < peak_at(3));
        let reach = |i: usize| (0..360).map(|k| w.angle(i, n, k as f32 * TAU / 360.0).abs()).fold(0.0, f32::max);
        assert!((reach(n - 1) - 0.4).abs() < 1e-3 && (reach(0) - 0.1).abs() < 1e-3);
    }

    #[test]
    fn swimming_beats_faster_with_speed_and_replays_exactly() {
        let run = |speed: f32| {
            let mut s = Swim::new(1.0);
            for _ in 0..120 {
                s.update(1.0 / 60.0, speed);
            }
            s
        };
        assert!(run(2.0).phase() > run(0.5).phase() && run(0.0).phase() > 0.0);
        assert_eq!(run(1.3), run(1.3));
        // 60 and 120 Hz end up at nearly the same phase (dt-correct).
        let mut a = Swim::new(1.0);
        let mut b = Swim::new(1.0);
        for _ in 0..60 {
            a.update(1.0 / 60.0, 1.0);
        }
        for _ in 0..120 {
            b.update(1.0 / 120.0, 1.0);
        }
        assert!((a.phase() - b.phase()).abs() < 1e-3 && (a.strength - b.strength).abs() < 1e-2);
    }
}
