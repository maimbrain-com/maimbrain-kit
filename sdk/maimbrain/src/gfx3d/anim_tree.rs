//! An animation tree for a spawned model, after Godot's `AnimationTree`
//! with an `AnimationNodeStateMachine` at its root: **states**, each one
//! clip or a **blend space** of clips (1D, Godot's `BlendSpace1D`, or 2D,
//! `BlendSpace2D`), and **transitions** between them that fire when a
//! boolean parameter turns on (Godot's advance conditions), crossfading over
//! their `fade`. A one-shot state can name the state that follows it at its
//! end (Godot's switch mode "at end" with auto-advance).
//!
//! It is bookkeeping only, like [`Animator`](super::Animator): it moves when
//! you call [`AnimTree::update`] with the frame's `dt`, and
//! [`AnimTree::samples`] says which clips to sample at which times and
//! weights ([`Node::pose`](super::Node::pose) each, or
//! [`AnimTree::apply`]). So it replays exactly, and it runs and is tested
//! natively.
//!
//! ```ignore
//! let mut tree = AnimTree::new();
//! let mv = tree.add_state("move", Motion::blend1d("speed", &[(0.0, idle), (2.0, walk), (6.0, run)]));
//! let jump = tree.add_state("jump", Motion::once(jump_clip));
//! tree.add_transition(Transition::new(Some(mv), jump).when("airborne").fade(0.1));
//! tree.add_transition(Transition::new(Some(jump), mv).when("onFloor").fade(0.15));
//! tree.start(mv);
//! // every frame:
//! tree.set("speed", 3.2);
//! tree.set_bool("airborne", !on_floor);
//! tree.update(dt);
//! tree.apply(fox);
//! ```
//!
//! What follows Godot (MIT licence; Copyright (c) 2014-present Godot Engine
//! contributors, (c) 2007-2014 Juan Linietsky, Ariel Manzur; the notice is
//! in the engine's NOTICES): a 1D blend space weighs the two points either
//! side of the value, clamped at the ends; a 2D one triangulates its points
//! (Delaunay) and weighs the triangle the value falls in by barycentric
//! weights, or the nearest point on the triangles when it falls outside; a
//! state's clips play in step (Godot's `sync`), so a walk and a run keep
//! their feet together. Ours: [`AnimTree::travel`] goes straight to the
//! state, crossfading with the fade of the transition into it (Godot walks
//! the shortest path of transitions through the states between), and the
//! transitions are checked in the order they were added, the first whose
//! condition holds winning.

use super::Node;
use super::anim::{Clip, Mask};

/// What a state plays.
#[derive(Clone, Debug, PartialEq)]
pub enum Motion {
    /// One clip, looping, or once (holding its last frame).
    Clip { clip: Clip, once: bool },
    /// Clips at points along one parameter.
    Blend1D { param: String, points: Vec<(f32, Clip)> },
    /// Clips at points of a plane of two parameters.
    Blend2D { params: [String; 2], points: Vec<([f32; 2], Clip)> },
}

impl Motion {
    pub fn looping(clip: Clip) -> Motion {
        Motion::Clip { clip, once: false }
    }
    pub fn once(clip: Clip) -> Motion {
        Motion::Clip { clip, once: true }
    }
    pub fn blend1d(param: &str, points: &[(f32, Clip)]) -> Motion {
        let mut points = points.to_vec();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Motion::Blend1D { param: param.into(), points }
    }
    pub fn blend2d(params: [&str; 2], points: &[([f32; 2], Clip)]) -> Motion {
        Motion::Blend2D { params: params.map(String::from), points: points.to_vec() }
    }
}

/// A state: its name, what it plays, its speed, and (for a one-shot) the
/// state it moves on to at its end.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub name: String,
    pub motion: Motion,
    pub speed: f32,
    pub next: Option<usize>,
    /// The crossfade into `next` (it ends as the clip does).
    pub next_fade: f32,
}

/// A transition: from a state (None: from any), to one, when a boolean
/// parameter is on (None: only by [`AnimTree::travel`]), crossfading.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from: Option<usize>,
    pub to: usize,
    pub when: Option<String>,
    pub fade: f32,
}

impl Transition {
    pub fn new(from: Option<usize>, to: usize) -> Transition {
        Transition { from, to, when: None, fade: 0.2 }
    }
    pub fn when(mut self, param: &str) -> Transition {
        self.when = Some(param.into());
        self
    }
    pub fn fade(mut self, secs: f32) -> Transition {
        self.fade = secs.max(0.0);
        self
    }
}

/// A state playing, or fading out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Playing {
    state: usize,
    /// Seconds into a one-shot or looping clip; for a blend space, its
    /// phase (0–1) times 1 s, so its clips stay in step.
    time: f32,
    weight: f32,
    target: f32,
    rate: f32,
}

/// The state machine. Build it with [`AnimTree::add_state`] and
/// [`AnimTree::add_transition`], [`AnimTree::start`] it, then each frame
/// set parameters, [`update`](AnimTree::update) and
/// [`apply`](AnimTree::apply).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimTree {
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    params: Vec<(String, f32)>,
    playing: Vec<Playing>,
    /// Set when a state was entered this update (for the engine's signal).
    entered: Option<usize>,
}

/// States kept at once: the current one and those fading out.
const MAX_PLAYING: usize = 4;

impl AnimTree {
    pub fn new() -> AnimTree {
        AnimTree::default()
    }

    pub fn add_state(&mut self, name: &str, motion: Motion) -> usize {
        self.states.push(State { name: name.into(), motion, speed: 1.0, next: None, next_fade: 0.1 });
        self.states.len() - 1
    }

    pub fn add_transition(&mut self, t: Transition) {
        self.transitions.push(t);
    }

    pub fn state_named(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s.name == name)
    }

    /// Starts in `state` at once (no fade), from its beginning.
    pub fn start(&mut self, state: usize) {
        if state < self.states.len() {
            self.playing.clear();
            self.playing.push(Playing { state, time: 0.0, weight: 1.0, target: 1.0, rate: f32::INFINITY });
            self.entered = Some(state);
        }
    }

    /// Goes to `state`, crossfading with the fade of a transition into it
    /// (from the current state if there is one, else any), or 0.2 s.
    /// Travelling to the current state does nothing.
    pub fn travel(&mut self, state: usize) {
        if state >= self.states.len() || self.current() == Some(state) {
            return;
        }
        let cur = self.current();
        let fade = self
            .transitions
            .iter()
            .find(|t| t.to == state && t.from.is_some() && t.from == cur)
            .or_else(|| self.transitions.iter().find(|t| t.to == state && t.from.is_none()))
            .or_else(|| self.transitions.iter().find(|t| t.to == state))
            .map_or(0.2, |t| t.fade);
        self.go(state, fade);
    }

    fn go(&mut self, state: usize, fade: f32) {
        let rate = if fade > 1e-4 { 1.0 / fade } else { f32::INFINITY };
        for p in &mut self.playing {
            p.target = 0.0;
            p.rate = rate;
        }
        // A blend space joins in step with what it replaces (a walk into a run).
        let phase = self.playing.last().map_or(0.0, |p| self.phase_of(p));
        let time = match &self.states[state].motion {
            Motion::Clip { .. } => 0.0,
            _ => phase,
        };
        let first = self.playing.is_empty();
        self.playing.push(Playing { state, time, weight: if first || rate.is_infinite() { 1.0 } else { 0.0 }, target: 1.0, rate });
        if rate.is_infinite() {
            self.playing.retain(|p| p.target > 0.0);
        }
        while self.playing.len() > MAX_PLAYING {
            self.playing.remove(0);
        }
        self.entered = Some(state);
    }

    /// A number parameter (`by` of a blend space).
    pub fn set(&mut self, name: &str, value: f32) {
        let value = if value.is_finite() { value } else { 0.0 };
        match self.params.iter_mut().find(|p| p.0 == name) {
            Some(p) => p.1 = value,
            None => self.params.push((name.into(), value)),
        }
    }

    /// A boolean parameter (a transition's `when`).
    pub fn set_bool(&mut self, name: &str, on: bool) {
        self.set(name, on as u32 as f32);
    }

    pub fn get(&self, name: &str) -> f32 {
        self.params.iter().find(|p| p.0 == name).map_or(0.0, |p| p.1)
    }

    /// The state playing (or fading in) now.
    pub fn current(&self) -> Option<usize> {
        self.playing.last().map(|p| p.state)
    }

    /// The state entered since the last call, once (the engine's signal).
    pub fn take_entered(&mut self) -> Option<usize> {
        self.entered.take()
    }

    /// Seconds into the current state's clip (a blend space's: into its
    /// heaviest clip).
    pub fn time(&self) -> f32 {
        let Some(p) = self.playing.last() else { return 0.0 };
        match &self.states[p.state].motion {
            Motion::Clip { .. } => p.time,
            _ => self.state_samples(p).into_iter().max_by(|a, b| a.2.total_cmp(&b.2)).map_or(0.0, |s| s.1),
        }
    }

    /// The current state is a one-shot at its end.
    pub fn finished(&self) -> bool {
        self.playing.last().is_some_and(|p| matches!(self.states[p.state].motion, Motion::Clip { clip, once: true } if p.time >= clip.duration))
    }

    fn phase_of(&self, p: &Playing) -> f32 {
        match &self.states[p.state].motion {
            Motion::Clip { clip, .. } if clip.duration > 0.0 => (p.time / clip.duration).fract(),
            Motion::Clip { .. } => 0.0,
            _ => p.time.fract(),
        }
    }

    /// The cycle length (seconds) a blend space's phase moves by: its
    /// clips' lengths, weighted (Godot's sync).
    fn blend_length(&self, p: &Playing) -> f32 {
        let w: Vec<(Clip, f32)> = self.weights(p.state);
        let total: f32 = w.iter().map(|x| x.1).sum();
        if total <= 0.0 {
            return 1.0;
        }
        (w.iter().map(|(c, k)| c.duration.max(0.01) * k).sum::<f32>() / total).max(0.01)
    }

    /// Each clip of a state and its weight in it (summing to 1).
    fn weights(&self, state: usize) -> Vec<(Clip, f32)> {
        match &self.states[state].motion {
            Motion::Clip { clip, .. } => vec![(*clip, 1.0)],
            Motion::Blend1D { param, points } => blend1d(points, self.get(param)),
            Motion::Blend2D { params, points } => blend2d(points, [self.get(&params[0]), self.get(&params[1])]),
        }
    }

    /// Moves the tree on by `dt`: first the transitions whose condition
    /// holds (the first that does, from the current state or any), and a
    /// one-shot whose `next` is due; then every playing state and fade.
    pub fn update(&mut self, dt: f32) {
        if let Some(cur) = self.current() {
            let fire = self.transitions.iter().find(|t| t.to != cur && (t.from.is_none() || t.from == Some(cur)) && t.when.as_ref().is_some_and(|w| self.get(w) != 0.0)).map(|t| (t.to, t.fade));
            if let Some((to, fade)) = fire {
                self.go(to, fade);
            } else if let (Some(p), Some(next)) = (self.playing.last(), self.states[cur].next)
                && let Motion::Clip { clip, once: true } = self.states[cur].motion
            {
                let fade = self.states[cur].next_fade;
                // The crossfade into `next` ends as the clip does.
                if p.target > 0.0 && p.time + 1e-6 >= (clip.duration - fade).max(0.0) {
                    self.go(next, fade.min(clip.duration - p.time.min(clip.duration)).max(0.0));
                }
            }
        }
        if !(dt > 0.0) {
            return;
        }
        for i in 0..self.playing.len() {
            let p = self.playing[i];
            let st = &self.states[p.state];
            let time = match st.motion {
                Motion::Clip { clip, once } => {
                    let t = p.time + dt * st.speed;
                    if clip.duration <= 0.0 {
                        0.0
                    } else if once {
                        t.min(clip.duration)
                    } else {
                        t.rem_euclid(clip.duration)
                    }
                }
                _ => (p.time + dt * st.speed / self.blend_length(&p)).rem_euclid(1.0),
            };
            let step = if p.rate.is_infinite() { 1.0 } else { p.rate * dt };
            let weight = if p.weight < p.target { (p.weight + step).min(p.target) } else { (p.weight - step).max(p.target) };
            self.playing[i].time = time;
            self.playing[i].weight = weight;
        }
        self.playing.retain(|p| p.target > 0.0 || p.weight > 0.0);
    }

    fn state_samples(&self, p: &Playing) -> Vec<(Clip, f32, f32)> {
        match &self.states[p.state].motion {
            Motion::Clip { clip, .. } => vec![(*clip, p.time, 1.0)],
            _ => self.weights(p.state).into_iter().filter(|w| w.1 > 0.0).map(|(c, w)| (c, p.time.fract() * c.duration, w)).collect(),
        }
    }

    /// What to pose: (clip, time, weight) for every clip of every audible
    /// state, crossfades eased as [`Animator`](super::Animator)'s are.
    pub fn samples(&self) -> Vec<(Clip, f32, f32)> {
        let mut out = Vec::new();
        for p in self.playing.iter().filter(|p| p.weight > 0.0) {
            let w = p.weight * p.weight * (3.0 - 2.0 * p.weight);
            for (c, t, k) in self.state_samples(p) {
                out.push((c, t, w * k));
            }
        }
        out
    }

    /// [`samples`](Self::samples), each with whether its clip loops (all
    /// but a `Motion::once` state's).
    pub fn samples_looping(&self) -> Vec<(Clip, f32, f32, bool)> {
        let mut out = Vec::new();
        for p in self.playing.iter().filter(|p| p.weight > 0.0) {
            let w = p.weight * p.weight * (3.0 - 2.0 * p.weight);
            let looping = !matches!(self.states[p.state].motion, Motion::Clip { once: true, .. });
            for (c, t, k) in self.state_samples(p) {
                out.push((c, t, w * k, looping));
            }
        }
        out
    }

    /// Poses `node` (a spawned model's root) with [`samples`](Self::samples).
    pub fn apply(&self, node: Node) {
        for (clip, time, w) in self.samples() {
            node.pose(clip, time, w);
        }
    }

    /// [`apply`](Self::apply) on the bones of `mask` only.
    pub fn apply_masked(&self, node: Node, mask: &Mask) {
        for (clip, time, w) in self.samples() {
            node.pose_masked(clip, time, w, mask);
        }
    }
}

/// A 1D blend space's weights at `x`: the two points either side, linearly,
/// clamped to the ends (Godot's `BlendSpace1D`).
pub fn blend1d(points: &[(f32, Clip)], x: f32) -> Vec<(Clip, f32)> {
    match points {
        [] => Vec::new(),
        [only] => vec![(only.1, 1.0)],
        _ => {
            if x <= points[0].0 {
                return vec![(points[0].1, 1.0)];
            }
            let last = points[points.len() - 1];
            if x >= last.0 {
                return vec![(last.1, 1.0)];
            }
            let i = points.windows(2).position(|w| x >= w[0].0 && x <= w[1].0).unwrap_or(0);
            let (a, b) = (points[i], points[i + 1]);
            let span = b.0 - a.0;
            let t = if span > 0.0 { (x - a.0) / span } else { 0.0 };
            vec![(a.1, 1.0 - t), (b.1, t)]
        }
    }
}

/// A 2D blend space's weights at `p` (Godot's `BlendSpace2D`): the
/// Delaunay triangle it's in, barycentric; outside every triangle, the
/// nearest point on them. Fewer than three points, or all in a line: the
/// nearest segment's two ends.
pub fn blend2d(points: &[([f32; 2], Clip)], p: [f32; 2]) -> Vec<(Clip, f32)> {
    match points.len() {
        0 => return Vec::new(),
        1 => return vec![(points[0].1, 1.0)],
        _ => {}
    }
    let pts: Vec<[f32; 2]> = points.iter().map(|x| x.0).collect();
    let tris = delaunay(&pts);
    let mut best: Option<(f32, Vec<(usize, f32)>)> = None;
    for t in &tris {
        let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
        let [u, v, w] = barycentric(a, b, c, p);
        if u >= -1e-6 && v >= -1e-6 && w >= -1e-6 {
            best = Some((0.0, vec![(t[0], u.max(0.0)), (t[1], v.max(0.0)), (t[2], w.max(0.0))]));
            break;
        }
    }
    if best.is_none() {
        // The nearest point on an edge (of a triangle, or of the points in order when there are none).
        let edges: Vec<[usize; 2]> = if tris.is_empty() { (0..pts.len()).flat_map(|i| (i + 1..pts.len()).map(move |j| [i, j])).collect() } else { tris.iter().flat_map(|t| [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]]).collect() };
        for [i, j] in edges {
            let (a, b) = (pts[i], pts[j]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if len2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            let d = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2);
            if best.as_ref().is_none_or(|b| d < b.0 - 1e-9) {
                best = Some((d, vec![(i, 1.0 - t), (j, t)]));
            }
        }
    }
    let mut out: Vec<(Clip, f32)> = Vec::new();
    for (i, w) in best.map(|b| b.1).unwrap_or_default() {
        if w > 0.0 {
            out.push((points[i].1, w));
        }
    }
    out
}

fn barycentric(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> [f32; 3] {
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < 1e-12 {
        return [-1.0; 3];
    }
    let u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / d;
    let v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / d;
    [u, v, 1.0 - u - v]
}

/// Delaunay triangles of a few points (a blend space has a handful): every
/// triangle whose circumcircle holds no other point, by brute force, in a
/// fixed order so it's the same everywhere.
fn delaunay(p: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = p.len().min(24);
    let mut out = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let (a, b, c) = (p[i], p[j], p[k]);
                let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                if cross.abs() < 1e-9 {
                    continue;
                }
                // The circumcircle.
                let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
                let sq = |q: [f32; 2]| q[0] * q[0] + q[1] * q[1];
                let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
                let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
                let r2 = (a[0] - ux).powi(2) + (a[1] - uy).powi(2);
                let empty = (0..n).filter(|&m| m != i && m != j && m != k).all(|m| (p[m][0] - ux).powi(2) + (p[m][1] - uy).powi(2) >= r2 * (1.0 - 1e-5));
                if empty {
                    out.push([i, j, k]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: Clip = Clip { index: 0, duration: 2.0 };
    const WALK: Clip = Clip { index: 1, duration: 1.0 };
    const RUN: Clip = Clip { index: 2, duration: 0.5 };
    const JUMP: Clip = Clip { index: 3, duration: 0.8 };
    const LAND: Clip = Clip { index: 4, duration: 0.4 };

    fn run(t: &mut AnimTree, seconds: f32) {
        for _ in 0..(seconds * 60.0).round() as u32 {
            t.update(1.0 / 60.0);
        }
    }

    fn weight_of(t: &AnimTree, c: Clip) -> f32 {
        t.samples().iter().filter(|s| s.0 == c).map(|s| s.2).sum()
    }

    fn tree() -> (AnimTree, usize, usize, usize) {
        let mut t = AnimTree::new();
        let mv = t.add_state("move", Motion::blend1d("speed", &[(2.0, WALK), (0.0, IDLE), (6.0, RUN)]));
        let jump = t.add_state("jump", Motion::once(JUMP));
        let land = t.add_state("land", Motion::once(LAND));
        t.states[land].next = Some(mv);
        t.add_transition(Transition::new(Some(mv), jump).when("airborne").fade(0.1));
        t.add_transition(Transition::new(Some(jump), land).when("onFloor").fade(0.05));
        t.start(mv);
        (t, mv, jump, land)
    }

    #[test]
    fn a_1d_blend_space_weighs_its_neighbours_and_keeps_them_in_step() {
        let (mut t, mv, ..) = tree();
        assert_eq!(t.samples(), vec![(IDLE, 0.0, 1.0)], "speed 0 is idle alone");
        t.set("speed", 4.0);
        assert!((weight_of(&t, WALK) - 0.5).abs() < 1e-6 && (weight_of(&t, RUN) - 0.5).abs() < 1e-6);
        t.set("speed", 99.0);
        assert_eq!(weight_of(&t, RUN), 1.0, "clamped at the end");
        t.set("speed", 4.0);
        run(&mut t, 0.25);
        // Walk (1 s) and run (0.5 s) at half each: a 0.75 s cycle, a third of the way after 0.25 s.
        let s = t.samples();
        let walk = s.iter().find(|x| x.0 == WALK).unwrap().1;
        let run_ = s.iter().find(|x| x.0 == RUN).unwrap().1;
        assert!((walk / WALK.duration - run_ / RUN.duration).abs() < 1e-5, "in step: {s:?}");
        assert!((walk - 1.0 / 3.0).abs() < 1e-3, "{walk}");
        assert_eq!(t.current(), Some(mv));
    }

    #[test]
    fn conditions_fire_transitions_and_a_one_shot_moves_on_at_its_end() {
        let (mut t, mv, jump, land) = tree();
        t.set("speed", 6.0);
        run(&mut t, 0.1);
        t.set_bool("airborne", true);
        t.update(1.0 / 60.0);
        assert_eq!(t.current(), Some(jump));
        run(&mut t, 0.05);
        let total: f32 = t.samples().iter().map(|s| s.2).sum();
        assert!((total - 1.0).abs() < 0.02, "a crossfade sums to about 1: {:?}", t.samples());
        run(&mut t, 0.1);
        assert_eq!(t.samples(), vec![(JUMP, t.time(), 1.0)], "the fade is done");
        t.set_bool("airborne", false);
        t.set_bool("onFloor", true);
        run(&mut t, 0.1);
        assert_eq!(t.current(), Some(land));
        t.set_bool("onFloor", false);
        run(&mut t, 0.5);
        assert_eq!(t.current(), Some(mv), "land's next");
        assert_eq!(t.take_entered(), Some(mv));
        assert_eq!(t.take_entered(), None);
    }

    #[test]
    fn travel_crossfades_with_the_transitions_fade() {
        let (mut t, mv, jump, _) = tree();
        t.travel(jump);
        assert_eq!(t.current(), Some(jump));
        run(&mut t, 0.05);
        assert!(weight_of(&t, IDLE) > 0.2 && weight_of(&t, JUMP) > 0.2, "{:?}", t.samples());
        run(&mut t, 0.1);
        assert_eq!(weight_of(&t, IDLE), 0.0);
        t.travel(jump);
        run(&mut t, 2.0);
        assert!(t.finished() && t.time() == JUMP.duration, "holds its last frame");
        t.travel(mv);
        assert_eq!(t.current(), Some(mv));
    }

    #[test]
    fn a_2d_blend_space_is_barycentric_in_its_triangles() {
        let pts = [([0.0, 0.0], IDLE), ([1.0, 0.0], WALK), ([0.0, 1.0], RUN), ([1.0, 1.0], JUMP)];
        let w = blend2d(&pts, [0.0, 0.0]);
        assert_eq!(w, vec![(IDLE, 1.0)]);
        let w = blend2d(&pts, [0.25, 0.25]);
        let sum: f32 = w.iter().map(|x| x.1).sum();
        assert!((sum - 1.0).abs() < 1e-5 && w.len() == 3, "{w:?}");
        // Outside: the nearest point on the edge from walk to jump.
        let w = blend2d(&pts, [3.0, 0.5]);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w.iter().all(|x| (x.1 - 0.5).abs() < 1e-5) && w.iter().any(|x| x.0 == WALK) && w.iter().any(|x| x.0 == JUMP), "{w:?}");
        // Two points: a line.
        let w = blend2d(&pts[..2], [0.75, 3.0]);
        assert_eq!(w, vec![(IDLE, 0.25), (WALK, 0.75)]);
    }
}
