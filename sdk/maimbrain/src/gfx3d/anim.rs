//! Playing a glTF model's animation clips (mb3d 2): [`Clip`] names one,
//! [`Node::pose`] adds one weighted sample of it to a spawned model's pose
//! this frame, and [`Animator`] does the bookkeeping most games want: play,
//! loop or play once, speed, and crossfades between clips (idle → walk →
//! run), with walk and run kept in step.
//!
//! Animation time is yours: the animator only moves when you call
//! [`Animator::update`] with your frame's `dt`, and the host samples clips at
//! exactly the times you pass, so replays pose the model identically.

use super::math::{Quat, Transform, Vec3};
use super::skin::Skeleton;
use super::{Model, Node};
use crate::ffi;

/// One of a model's animation clips, with its length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub index: u32,
    /// Seconds.
    pub duration: f32,
}

impl Model {
    /// How many animation clips the model has.
    pub fn clip_count(self) -> u32 {
        unsafe { ffi::mb3d_clip_count(self.0.get()) }.max(0) as u32
    }
    /// The clip named `name` (its glTF animation name), if the model has one.
    pub fn clip(self, name: &str) -> Option<Clip> {
        let i = unsafe { ffi::mb3d_clip_find(self.0.get(), name.as_ptr(), name.len() as u32) };
        (i >= 0).then(|| self.clip_at(i as u32)).flatten()
    }
    /// Clip number `index` (0 ≤ index < [`clip_count`](Self::clip_count)).
    pub fn clip_at(self, index: u32) -> Option<Clip> {
        let duration = unsafe { ffi::mb3d_clip_duration(self.0.get(), index) };
        (duration >= 0.0).then_some(Clip { index, duration })
    }
}

impl Node {
    /// Adds `clip` sampled at `time` seconds (clamped to the clip) with
    /// `weight` to this frame's pose of the model spawned at this node (the
    /// root [`Model::spawn`] returned). Samples blend by weight; weights short
    /// of 1 are made up by the model's rest pose. Call it every frame you want
    /// the pose to change (in `update` or `render`, before `render()`); with
    /// no samples a frame keeps the last pose. False if this isn't a spawned
    /// model's root or the clip doesn't exist.
    pub fn pose(self, clip: Clip, time: f32, weight: f32) -> bool {
        unsafe { ffi::mb3d_anim(self.0.get(), clip.index, time, weight) >= 0 }
    }

    /// [`pose`](Self::pose) on only the bones in `mask`, each at `weight` ×
    /// its weight in the mask: a punch on the upper body while the legs walk,
    /// an idle at 40 % on the spine under code-driven legs. The clip's other
    /// bones get nothing from this sample (they keep what other samples, or
    /// the game, give them).
    pub fn pose_masked(self, clip: Clip, time: f32, weight: f32, mask: &Mask) -> bool {
        let packed: Vec<u8> = mask.entries.iter().flat_map(|&(n, w)| n.raw().to_le_bytes().into_iter().chain(w.to_le_bytes())).collect();
        unsafe { ffi::mb3d_anim_mask(self.0.get(), clip.index, time, weight, packed.as_ptr(), mask.entries.len() as u32) >= 0 }
    }

    /// Takes over this node's pose from this frame's clip samples: returns
    /// the local transform they blend to (Some) and drops them, so a
    /// `set_transform` after this wins (without it the samples would
    /// override it when the frame draws). None if no clip moved the node
    /// this frame. The base of every procedural layer on top of a clip
    /// ([`Skeleton::add_turn`]).
    pub fn take_pose(self) -> Option<Transform> {
        let mut out = [0.0f32; 10];
        let r = unsafe { ffi::mb3d_anim_take(self.0.get(), out.as_mut_ptr().cast()) };
        (r == 1).then(|| transform_of(&out))
    }

    /// The node's local transform now (after this frame's clip samples so
    /// far, which it takes over like [`take_pose`](Self::take_pose)). None
    /// for a destroyed node; off wasm, the identity.
    pub fn local(self) -> Option<Transform> {
        let mut out = [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let r = unsafe { ffi::mb3d_anim_take(self.0.get(), out.as_mut_ptr().cast()) };
        (r >= 0).then(|| transform_of(&out))
    }

    /// The node this one hangs from (None at the root): a bone's real parent
    /// in a loaded model.
    pub fn parent(self) -> Option<Node> {
        super::handle(unsafe { ffi::mb3d_node_get_parent(self.0.get()) }).map(Node)
    }
}

fn transform_of(v: &[f32; 10]) -> Transform {
    Transform { pos: Vec3::new(v[0], v[1], v[2]), rot: Quat { x: v[3], y: v[4], z: v[5], w: v[6] }, scale: Vec3::new(v[7], v[8], v[9]) }
}

/// Which bones a clip sample moves, and how much of it each takes (0–1):
/// for [`Node::pose_masked`], [`Animator::apply_masked`] and
/// [`Skeleton::add_clip`]. Build one from bones ([`Mask::with`]), a
/// skeleton's ([`Skeleton::mask`]) or a whole limb of a loaded model
/// ([`Mask::below`]: the bone and everything under it, twist bones included).
///
/// ```ignore
/// let upper = Mask::below(model, mech, mech.find("Spine01")?, 1.0);
/// let legs = Mask::below(model, mech, mech.find("Pelvis")?, 1.0);
/// walk.apply_masked(mech, &legs);            // the walk on the legs
/// attack.apply_masked(mech, &upper);         // the swing on the upper body, in the same frame
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mask {
    entries: Vec<(Node, f32)>,
}

impl Mask {
    pub fn new() -> Mask {
        Mask::default()
    }

    /// Adds `bone` at `weight` (or changes its weight).
    pub fn with(mut self, bone: Node, weight: f32) -> Mask {
        self.set(bone, weight);
        self
    }

    /// Sets `bone`'s weight (adding it if it isn't in the mask).
    pub fn set(&mut self, bone: Node, weight: f32) {
        let w = if weight.is_finite() { weight.max(0.0) } else { 0.0 };
        match self.entries.iter_mut().find(|e| e.0 == bone) {
            Some(e) => e.1 = w,
            None => self.entries.push((bone, w)),
        }
    }

    /// `bone`'s weight (0 if it isn't in the mask).
    pub fn weight(&self, bone: Node) -> f32 {
        self.entries.iter().find(|e| e.0 == bone).map_or(0.0, |e| e.1)
    }

    /// The bones and their weights.
    pub fn entries(&self) -> &[(Node, f32)] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every weight times `k` (fade a whole layer in or out).
    pub fn scaled(mut self, k: f32) -> Mask {
        for e in &mut self.entries {
            e.1 *= k.max(0.0);
        }
        self
    }

    /// `bone` and every node of the loaded model under it (by walking each
    /// named node's parents), at `weight`: a whole limb or the upper body.
    /// `model` is the loaded model and `root` the node its spawn returned.
    pub fn below(model: Model, root: Node, bone: Node, weight: f32) -> Mask {
        let mut m = Mask::new().with(bone, weight);
        for name in model.node_names() {
            let Some(n) = root.find(&name) else { continue };
            let mut p = n.parent();
            let mut guard = 0;
            while let Some(x) = p {
                if x == bone {
                    m.set(n, weight);
                    break;
                }
                if x == root || guard > 64 {
                    break;
                }
                p = x.parent();
                guard += 1;
            }
        }
        m
    }
}

impl Skeleton {
    /// The bones `bones` (indices) at `weight`.
    pub fn mask(&self, bones: &[usize], weight: f32) -> Mask {
        let mut m = Mask::new();
        for &i in bones {
            if let Some(b) = self.bone(i) {
                m.set(b, weight);
            }
        }
        m
    }

    /// Bone `i` and the skeleton's bones under it, at `weight`.
    pub fn mask_below(&self, i: usize, weight: f32) -> Mask {
        self.mask(&self.below(i), weight)
    }

    /// Bone `i` and the indices of the skeleton's bones under it.
    pub fn below(&self, i: usize) -> Vec<usize> {
        (0..self.len())
            .filter(|&j| {
                let mut a = Some(j);
                while let Some(x) = a {
                    if x == i {
                        return true;
                    }
                    a = self.parents[x];
                }
                false
            })
            .collect()
    }

    /// A procedural turn on top of this frame's clip pose: bone `i` takes
    /// what the clips gave it (its rest pose if none moved it), turned by
    /// `rot` in its own frame. Call it after posing the model (`pose`,
    /// `Animator::apply`), every frame: a recoil on the chest over an idle,
    /// a head tracking the player during a walk clip.
    pub fn add_turn(&self, i: usize, rot: Quat) {
        let (Some(b), Some(rest)) = (self.bones.get(i), self.rest.get(i)) else { return };
        let base = b.take_pose().unwrap_or(*rest);
        b.set_transform(&Transform { rot: base.rot * rot, ..base });
    }

    /// A clip on top of a procedural pose: adds `clip`'s motion at `time`
    /// (each bone's difference from its rest pose, times `weight` and its
    /// weight in `mask`; all the skeleton's bones with `None`) to the pose
    /// the game has set on the bones this frame. `model` is the node the
    /// model's spawn returned. Code walks the legs while an idle clip
    /// breathes in the spine at 40 %:
    /// ```ignore
    /// bones.turn(chest, lean);                                // procedural first
    /// bones.add_clip(mech, idle, t, 0.4, Some(&upper));       // then the clip over it
    /// ```
    pub fn add_clip(&self, model: Node, clip: Clip, time: f32, weight: f32, mask: Option<&Mask>) {
        let current: Vec<Option<Transform>> = self.bones.iter().map(|b| b.local()).collect();
        let mut m = Mask::new();
        for &b in &self.bones {
            let w = mask.map_or(1.0, |m| m.weight(b));
            if w > 0.0 {
                m.set(b, w);
            }
        }
        if !model.pose_masked(clip, time, weight, &m) {
            return;
        }
        for (i, b) in self.bones.iter().enumerate() {
            if let (Some(s), Some(cur)) = (b.take_pose(), current[i]) {
                b.set_transform(&additive(cur, self.rest[i], s));
            }
        }
    }
}

/// `sample`'s difference from `rest`, added to `base`: rotations compose
/// (base · rest⁻¹ · sample, in the bone's frame), positions add.
pub fn additive(base: Transform, rest: Transform, sample: Transform) -> Transform {
    Transform { pos: base.pos + (sample.pos - rest.pos), rot: (base.rot * (rest.rot.conjugate() * sample.rot)).normalize(), scale: base.scale }
}

/// How a clip plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    /// Repeats forever.
    Loop,
    /// Plays to the end and holds the last frame ([`Animator::finished`] turns true).
    Once,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    clip: Clip,
    mode: Play,
    time: f32,
    speed: f32,
    /// 0–1 and how fast it moves toward `target` (per second).
    weight: f32,
    target: f32,
    rate: f32,
}

/// Plays clips on one spawned model with crossfades. Keep one per model
/// instance in your game state, `update` it with `dt` and `apply` it each frame.
///
/// ```ignore
/// let idle = model.clip("Idle").unwrap();
/// let run = model.clip("Run").unwrap();
/// let mut anim = Animator::new();
/// anim.play(idle, Play::Loop, 0.0);
/// // later, when the player starts running:
/// anim.play_synced(run, 0.25);
/// // every frame:
/// anim.update(dt);
/// anim.apply(fox);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animator {
    layers: Vec<Layer>,
}

/// Layers kept at once: the current clip and the ones fading out.
const MAX_LAYERS: usize = 4;

impl Animator {
    pub fn new() -> Animator {
        Animator::default()
    }

    /// Crossfades to `clip` over `fade` seconds (0 = cut), from its start.
    /// Playing the clip that's already current changes nothing, except that
    /// a finished [`Play::Once`] clip starts over.
    pub fn play(&mut self, clip: Clip, mode: Play, fade: f32) {
        if let Some(cur) = self.layers.last()
            && cur.clip == clip
            && cur.target == 1.0
            && !(cur.mode == Play::Once && self.finished())
        {
            self.layers.last_mut().unwrap().mode = mode;
            return;
        }
        self.start(clip, mode, fade, 0.0);
    }

    /// Like [`play`](Self::play) with [`Play::Loop`], but starting at the same
    /// point of its cycle as the current clip (its time as a fraction of its
    /// length), so a walk and a run cycle stay in step: feet don't skip when
    /// one fades into the other.
    pub fn play_synced(&mut self, clip: Clip, fade: f32) {
        if self.layers.last().is_some_and(|c| c.clip == clip && c.target == 1.0) {
            return;
        }
        let phase = self.phase();
        self.start(clip, Play::Loop, fade, phase * clip.duration);
    }

    /// Plays `clip` from the start even if it's current (a second jump).
    pub fn restart(&mut self, clip: Clip, mode: Play, fade: f32) {
        self.start(clip, mode, fade, 0.0);
    }

    fn start(&mut self, clip: Clip, mode: Play, fade: f32, time: f32) {
        let rate = if fade > 1e-4 { 1.0 / fade } else { f32::INFINITY };
        for l in &mut self.layers {
            l.target = 0.0;
            l.rate = rate;
        }
        let first = self.layers.is_empty();
        let speed = self.layers.last().map_or(1.0, |l| if l.clip == clip { l.speed } else { 1.0 });
        self.layers.push(Layer { clip, mode, time, speed, weight: if first || rate.is_infinite() { 1.0 } else { 0.0 }, target: 1.0, rate });
        if rate.is_infinite() {
            self.layers.retain(|l| l.target > 0.0);
        }
        while self.layers.len() > MAX_LAYERS {
            self.layers.remove(0);
        }
    }

    /// Playback speed of the current clip (1 = as authored; e.g. match a run to the ground speed).
    pub fn set_speed(&mut self, speed: f32) {
        if let Some(l) = self.layers.last_mut() {
            l.speed = speed.max(0.0);
        }
    }

    /// Advances every playing clip and crossfade by `dt` seconds (your frame's dt).
    pub fn update(&mut self, dt: f32) {
        if !(dt > 0.0) {
            return;
        }
        for l in &mut self.layers {
            l.time += dt * l.speed;
            if l.clip.duration > 0.0 {
                match l.mode {
                    Play::Loop => l.time = l.time.rem_euclid(l.clip.duration),
                    Play::Once => l.time = l.time.min(l.clip.duration),
                }
            }
            let step = if l.rate.is_infinite() { 1.0 } else { l.rate * dt };
            l.weight = if l.weight < l.target { (l.weight + step).min(l.target) } else { (l.weight - step).max(l.target) };
        }
        self.layers.retain(|l| l.target > 0.0 || l.weight > 0.0);
    }

    /// Poses `node` (a spawned model's root) with every audible layer.
    pub fn apply(&self, node: Node) {
        for (clip, time, w) in self.samples() {
            node.pose(clip, time, w);
        }
    }

    /// [`apply`](Self::apply) on only the bones in `mask`, at their weights:
    /// several animators on one model, each on its own bones (legs walking,
    /// arms attacking), or one at a fraction (`mask.scaled(0.4)`).
    pub fn apply_masked(&self, node: Node, mask: &Mask) {
        for (clip, time, w) in self.samples() {
            node.pose_masked(clip, time, w, mask);
        }
    }

    /// What [`apply`](Self::apply) sends: (clip, time, weight) per layer, crossfades eased.
    pub fn samples(&self) -> impl Iterator<Item = (Clip, f32, f32)> + '_ {
        self.layers.iter().filter(|l| l.weight > 0.0).map(|l| {
            let w = l.weight;
            (l.clip, l.time, w * w * (3.0 - 2.0 * w))
        })
    }

    /// The clip playing (or fading in) now.
    pub fn current(&self) -> Option<Clip> {
        self.layers.last().map(|l| l.clip)
    }

    /// Seconds into the current clip.
    pub fn time(&self) -> f32 {
        self.layers.last().map_or(0.0, |l| l.time)
    }

    /// How far through its cycle the current clip is (0–1).
    pub fn phase(&self) -> f32 {
        self.layers.last().map_or(0.0, |l| if l.clip.duration > 0.0 { l.time / l.clip.duration } else { 0.0 })
    }

    /// The current clip is [`Play::Once`] and has reached its end.
    pub fn finished(&self) -> bool {
        self.layers.last().is_some_and(|l| l.mode == Play::Once && l.time >= l.clip.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: Clip = Clip { index: 0, duration: 2.0 };
    const WALK: Clip = Clip { index: 1, duration: 1.0 };
    const RUN: Clip = Clip { index: 2, duration: 0.5 };
    const JUMP: Clip = Clip { index: 3, duration: 0.8 };

    fn run(a: &mut Animator, seconds: f32) {
        for _ in 0..(seconds * 60.0).round() as u32 {
            a.update(1.0 / 60.0);
        }
    }

    #[test]
    fn loops_and_crossfades_with_weights_summing_to_one() {
        let mut a = Animator::new();
        a.play(IDLE, Play::Loop, 0.3);
        assert_eq!(a.samples().collect::<Vec<_>>(), vec![(IDLE, 0.0, 1.0)], "the first clip starts at full weight");
        run(&mut a, 2.5);
        assert!((a.time() - 0.5).abs() < 1e-3, "looped: {}", a.time());
        a.play(WALK, Play::Loop, 0.5);
        run(&mut a, 0.25);
        let s: Vec<_> = a.samples().collect();
        assert_eq!(s.len(), 2);
        let total: f32 = s.iter().map(|x| x.2).sum();
        assert!((total - 1.0).abs() < 1e-4, "{s:?}");
        run(&mut a, 0.3);
        assert_eq!(a.samples().map(|x| x.0).collect::<Vec<_>>(), vec![WALK], "the old clip is dropped once faded out");
        // Playing the current clip again changes nothing.
        let before = a.clone();
        a.play(WALK, Play::Loop, 0.2);
        assert_eq!(a, before);
    }

    #[test]
    fn synced_crossfades_keep_the_cycle_phase() {
        let mut a = Animator::new();
        a.play(WALK, Play::Loop, 0.0);
        run(&mut a, 0.25);
        a.play_synced(RUN, 0.2);
        assert!((a.phase() - 0.25).abs() < 1e-3 && (a.time() - 0.125).abs() < 1e-3, "{} {}", a.phase(), a.time());
    }

    #[test]
    fn once_holds_its_last_frame_and_can_restart() {
        let mut a = Animator::new();
        a.play(IDLE, Play::Loop, 0.0);
        a.play(JUMP, Play::Once, 0.1);
        run(&mut a, 0.5);
        assert!(!a.finished());
        run(&mut a, 0.5);
        assert!(a.finished() && a.time() == JUMP.duration);
        a.play(JUMP, Play::Once, 0.1);
        assert_eq!(a.time(), 0.0, "a finished one-shot starts over");
        a.set_speed(2.0);
        run(&mut a, 0.2);
        assert!((a.time() - 0.4).abs() < 1e-3);
    }

    #[test]
    fn masks_hold_one_weight_per_bone() {
        let (a, b) = (Node::new().unwrap(), Node(std::num::NonZeroU32::new(2).unwrap()));
        let mut m = Mask::new().with(a, 0.5).with(b, 2.0).with(a, 0.25);
        assert_eq!(m.len(), 2);
        assert_eq!((m.weight(a), m.weight(b)), (0.25, 2.0));
        m.set(b, f32::NAN);
        assert_eq!(m.weight(b), 0.0, "bad weights are 0");
        assert_eq!(m.clone().scaled(2.0).weight(a), 0.5);
        let parent = Node::new().unwrap();
        let sk = Skeleton::chain(parent, &[Vec3::ZERO, Vec3::Y, Vec3::Y * 2.0, Vec3::Y * 3.0]).unwrap();
        assert_eq!(sk.below(2), vec![2, 3], "bone 2 and its child");
        assert_eq!(sk.below(0), vec![0, 1, 2, 3]);
        assert_eq!(sk.mask_below(0, 1.0).len(), 1, "off wasm every bone is the same stub node");
        assert!(Node::new().unwrap().take_pose().is_none(), "no clip moved it");
        let mut a = Animator::new();
        a.play(IDLE, Play::Loop, 0.0);
        a.apply_masked(parent, &m);
        sk.add_turn(1, Quat::from_rotation_x(0.2));
        sk.add_clip(parent, IDLE, 0.5, 0.4, None);
    }

    #[test]
    fn additive_layers_compose_on_the_base() {
        use crate::gfx3d::vec3;
        let rest = Transform { pos: vec3(0.0, 1.0, 0.0), rot: Quat::from_rotation_z(0.3), scale: Vec3::ONE };
        // A clip sample turned 0.5 about the bone's x and raised 0.1 from rest...
        let sample = Transform { pos: vec3(0.0, 1.1, 0.0), rot: rest.rot * Quat::from_rotation_x(0.5), ..rest };
        // ...added to a procedural pose turned 0.2 about y.
        let base = Transform { rot: rest.rot * Quat::from_rotation_y(0.2), ..rest };
        let out = additive(base, rest, sample);
        let want = rest.rot * Quat::from_rotation_y(0.2) * Quat::from_rotation_x(0.5);
        assert!(out.rot.dot(want).abs() > 0.99999, "{out:?}");
        assert!((out.pos - vec3(0.0, 1.1, 0.0)).length() < 1e-6);
        // The rest pose itself adds nothing.
        assert_eq!(additive(base, rest, rest).pos, base.pos);
        assert!(additive(base, rest, rest).rot.dot(base.rot).abs() > 0.99999);
    }

    #[test]
    fn cuts_and_layer_cap() {
        let mut a = Animator::new();
        for c in [IDLE, WALK, RUN, JUMP, IDLE, WALK] {
            a.restart(c, Play::Loop, 0.5);
        }
        assert!(a.layers.len() <= MAX_LAYERS);
        a.play(RUN, Play::Loop, 0.0);
        assert_eq!(a.samples().collect::<Vec<_>>(), vec![(RUN, 0.0, 1.0)], "a cut drops everything else");
    }
}
