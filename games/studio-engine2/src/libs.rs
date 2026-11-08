//! LÖVE's libraries, run in Rust (docs/STUDIO-2.md §5 [libraries]): what
//! flux, hump.timer and anim8 do every frame for every tween, timer and
//! animation, and the engine's own frame work beside them (a Sprite's
//! `play`, the camera's `follow`). Their Lua faces are api/libs.rs.
//!
//! Each frame the prelude asks for [`Libs::update`] before `love.update`,
//! as a LÖVE game calls `flux.update(dt)`, `timer.update(dt)` and
//! `anim:update(dt)` first thing. Values are written in Rust; what has to
//! run in Lua (an `oncomplete`, a timer's function, an animation's
//! `onLoop`, a node's signal) comes back as a list of [`Due`] calls, which
//! the prelude makes in order.
//!
//! flux's easing curves and its update rules are flux.lua's (rxi, MIT),
//! hump.timer's are hump's (Matthias Richter, MIT), anim8's are anim8's
//! (Enrique García Cota, MIT); api/libs.rs carries their license notices.

use std::ffi::c_int;

use maimbrain::scene::NodeId;

/// What a tween moves.
#[derive(Clone, Copy, Debug)]
pub enum Target {
    Node(NodeId),
    /// A 3D game's node (api/three.rs).
    Node3(maimbrain::scene::three::Node3Id),
    Camera,
    /// A Lua table: a registry ref to it, and its address, which says which
    /// table it is (each tween holds a ref of its own).
    Table(c_int, usize),
    /// `audio.bus(name)`: its volume and filters (api/sound.rs).
    Bus(u8),
}

impl PartialEq for Target {
    fn eq(&self, o: &Target) -> bool {
        match (self, o) {
            (Target::Node(a), Target::Node(b)) => a == b,
            (Target::Node3(a), Target::Node3(b)) => a == b,
            (Target::Camera, Target::Camera) => true,
            (Target::Table(_, a), Target::Table(_, b)) => a == b,
            (Target::Bus(a), Target::Bus(b)) => a == b,
            _ => false,
        }
    }
}

/// One variable a tween moves: from where it was when the tween started.
#[derive(Clone, Debug, PartialEq)]
pub struct Var {
    pub key: String,
    pub end: f64,
    pub start: f64,
    /// A colour (0xRRGGBB), moved channel by channel.
    pub color: bool,
}

#[derive(Clone, Debug)]
pub struct Tween {
    pub id: u32,
    pub target: Target,
    pub vars: Vec<Var>,
    pub rate: f64,
    pub progress: f64,
    pub delay: f64,
    pub ease: String,
    pub inited: bool,
    pub onstart: Vec<c_int>,
    pub onupdate: Vec<c_int>,
    pub oncomplete: Vec<c_int>,
    /// Tweens to add when this one completes (`:after`).
    pub then: Vec<u32>,
    /// There and back for ever (ours).
    pub yoyo: bool,
}

/// flux's easing by name (flux's own, `"backout"`, or easings.net's,
/// `"outBack"`): the curve, as flux.lua computes it.
pub fn flux_ease(name: &str, p: f64) -> Option<f64> {
    let lower = name.to_ascii_lowercase();
    // easings.net's "outBack" is flux's "backout".
    let (kind, dir) = ["inout", "in", "out"]
        .iter()
        .find_map(|d| lower.strip_prefix(d).map(|k| (k.to_string(), *d)).filter(|(k, _)| !k.is_empty()))
        .or_else(|| ["inout", "in", "out"].iter().find_map(|d| lower.strip_suffix(d).map(|k| (k.to_string(), *d)).filter(|(k, _)| !k.is_empty())))
        .unwrap_or((lower.clone(), ""));
    let f: fn(f64) -> f64 = match kind.as_str() {
        "linear" if dir.is_empty() => return Some(p),
        "quad" => |p| p * p,
        "cubic" => |p| p * p * p,
        "quart" => |p| p * p * p * p,
        "quint" => |p| p * p * p * p * p,
        "expo" => |p| 2f64.powf(10.0 * (p - 1.0)),
        "sine" => |p| -(p * (std::f64::consts::PI * 0.5)).cos() + 1.0,
        "circ" => |p| -((1.0 - p * p).sqrt() - 1.0),
        "back" => |p| p * p * (2.7 * p - 1.7),
        "elastic" => |p| -(2f64.powf(10.0 * (p - 1.0)) * ((p - 1.075) * (std::f64::consts::PI * 2.0) / 0.3).sin()),
        // Not flux's: easings.net's bounce, for its names.
        "bounce" => |p| 1.0 - bounce_out(1.0 - p),
        _ => return None,
    };
    Some(match dir {
        "in" => f(p),
        "out" => 1.0 - f(1.0 - p),
        "inout" => {
            if p < 0.5 {
                f(p * 2.0) * 0.5
            } else {
                1.0 - f((1.0 - p) * 2.0) * 0.5
            }
        }
        _ => return None,
    })
}

fn bounce_out(p: f64) -> f64 {
    let (n, d) = (7.5625, 2.75);
    if p < 1.0 / d {
        n * p * p
    } else if p < 2.0 / d {
        let p = p - 1.5 / d;
        n * p * p + 0.75
    } else if p < 2.5 / d {
        let p = p - 2.25 / d;
        n * p * p + 0.9375
    } else {
        let p = p - 2.625 / d;
        n * p * p + 0.984375
    }
}

/// A call the prelude makes after the libraries' frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Due {
    /// A function (registry ref) with numbers; `pass_self` passes the
    /// function itself first (hump.timer's `after(after)`). A timer's call
    /// names its timer, which stops if the function returns false.
    Call { f: c_int, args: Vec<f64>, pass_self: bool, timer: Option<u32> },
    /// anim8's onLoop(animation, loops).
    Loop { f: c_int, anim: u32, loops: f64 },
    /// A node's signal, with numbers.
    Signal { node: NodeId, name: &'static str, args: Vec<f64> },
    /// A 3D node's signal (a model's `animationFinished`, a gait's `stepped`).
    Signal3 { node: maimbrain::scene::three::Node3Id, name: &'static str, args: Vec<f64> },
    /// A 3D node's signal with another node (a body's `bodyEntered`).
    Signal3Node { node: maimbrain::scene::three::Node3Id, name: &'static str, other: maimbrain::scene::three::Node3Id },
}

/// A hump.timer handle.
#[derive(Clone, Debug)]
pub struct Timer {
    pub id: u32,
    pub time: f64,
    pub limit: f64,
    pub count: f64,
    pub during: Option<c_int>,
    pub after: Option<c_int>,
}

/// An anim8 animation.
#[derive(Clone, Debug)]
pub struct Anim {
    /// Its frames as quads (x, y, w, h).
    pub frames: Vec<[f32; 4]>,
    pub durations: Vec<f64>,
    /// When each frame starts.
    pub intervals: Vec<f64>,
    pub total: f64,
    pub timer: f64,
    /// From 1.
    pub position: usize,
    pub playing: bool,
    pub flipped_h: bool,
    pub flipped_v: bool,
    pub on_loop: OnLoop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OnLoop {
    Nothing,
    Function(c_int),
    PauseAtEnd,
    PauseAtStart,
}

impl Anim {
    pub fn new(frames: Vec<[f32; 4]>, durations: Vec<f64>, on_loop: OnLoop) -> Anim {
        let mut intervals = Vec::with_capacity(durations.len() + 1);
        let mut t = 0.0;
        for d in &durations {
            intervals.push(t);
            t += d;
        }
        Anim { frames, durations, intervals, total: t, timer: 0.0, position: 1, playing: true, flipped_h: false, flipped_v: false, on_loop }
    }

    /// anim8's seekFrameIndex: the frame whose interval holds the timer.
    fn seek(&self) -> usize {
        let (mut lo, mut hi) = (0usize, self.intervals.len() - 1);
        let mut i = 0;
        while lo <= hi {
            i = (lo + hi) / 2;
            if self.timer >= self.intervals[i] && (i + 1 >= self.intervals.len() || self.timer < self.intervals[i + 1]) {
                return i + 1;
            }
            if self.timer < self.intervals[i] {
                if i == 0 {
                    break;
                }
                hi = i - 1;
            } else {
                lo = i + 1;
            }
        }
        i + 1
    }

    pub fn goto(&mut self, n: usize) {
        self.position = n.clamp(1, self.frames.len());
        self.timer = self.intervals[self.position - 1];
    }

    pub fn pause_at_end(&mut self) {
        self.position = self.frames.len();
        self.timer = self.total;
        self.playing = false;
    }

    pub fn pause_at_start(&mut self) {
        self.position = 1;
        self.timer = 0.0;
        self.playing = false;
    }
}

/// A Sprite's `play`: its sheet's named animation.
#[derive(Clone, Debug)]
pub struct SpriteAnim {
    pub node: NodeId,
    pub name: String,
    pub frames: Vec<u32>,
    pub fps: f32,
    pub speed: f32,
    pub once: bool,
    pub t: f32,
    pub done: bool,
}

/// `camera:follow`.
#[derive(Clone, Debug)]
pub struct Follow {
    pub node: NodeId,
    pub lag: f32,
    pub lead: f32,
    pub deadzone: (f32, f32),
    pub bounds: Option<[f32; 4]>,
    /// The lead now (it eases toward the side the node is heading).
    pub lead_x: f32,
    /// Held at the bounds' edge (bit 0 across, bit 1 down), and whether it
    /// got to one this frame (it stops there at once: the smoothness check's
    /// `camera edge` mark).
    pub at_edge: u8,
    pub reached_edge: bool,
}

impl Follow {
    /// One frame of the camera following its node (false once the node is gone).
    pub fn step(&mut self, scene: &mut maimbrain::scene::Scene, dt: f32) -> bool {
        let Some(n) = scene.get(self.node) else { return false };
        let vx = n.vx;
        let (tx, ty) = scene.world_position(self.node);
        // The lead eases toward the side the node is heading.
        let want = if vx > 1.0 { self.lead } else if vx < -1.0 { -self.lead } else { self.lead_x };
        let k = |lag: f32| if lag <= 0.0 { 1.0 } else { 1.0 - (-dt / lag).exp() };
        self.lead_x += (want - self.lead_x) * k(0.5);
        let (tx, ty) = (tx + self.lead_x, ty);
        let cam = &mut scene.camera;
        let toward = |c: f32, t: f32, dz: f32| {
            let half = dz / 2.0;
            if t > c + half {
                t - half
            } else if t < c - half {
                t + half
            } else {
                c
            }
        };
        let (dx, dy) = (toward(cam.x, tx, self.deadzone.0), toward(cam.y, ty, self.deadzone.1));
        cam.x += (dx - cam.x) * k(self.lag);
        cam.y += (dy - cam.y) * k(self.lag);
        let mut held = 0;
        if let Some([bx, by, bw, bh]) = self.bounds {
            let (hw, hh) = (scene.width / 2.0 / cam.scale, scene.height / 2.0 / cam.scale);
            let clamp = |v: f32, lo: f32, hi: f32| if lo > hi { (lo + hi) / 2.0 } else { v.clamp(lo, hi) };
            let (x, y) = (clamp(cam.x, bx + hw, bx + bw - hw), clamp(cam.y, by + hh, by + bh - hh));
            held = u8::from((x - cam.x).abs() > 1e-3) | u8::from((y - cam.y).abs() > 1e-3) << 1;
            (cam.x, cam.y) = (x, y);
        }
        self.reached_edge = held & !self.at_edge != 0;
        self.at_edge = held;
        true
    }
}

/// `spring(target, { key = value }, { frequency, damping })`: one value
/// heading for its goal on the SDK's spring (docs/STUDIO-2.md §5 [juice]).
#[derive(Clone, Debug)]
pub struct SpringVar {
    pub target: Target,
    pub key: String,
    pub spring: maimbrain::motion::Spring,
}

/// What an `fx.punch` kicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PunchKind {
    /// The node's size: `sx`, `sy` (and `sz`) times 1 + the spring.
    Scale,
    /// Its place: `x`, `y` (and `z`) plus the springs.
    Move,
}

/// `fx.punch(node, amount)`: a kick that springs back to rest, applied on
/// top of whatever else moves the node. `applied` is what it added last
/// frame (a factor for Scale, offsets for Move), taken off before the new
/// one goes on; a write to the property sets it back to none (the written
/// value has no kick in it).
#[derive(Clone, Debug)]
pub struct PunchVar {
    pub target: Target,
    pub kind: PunchKind,
    pub springs: [maimbrain::motion::Spring; 3],
    pub applied: [f32; 3],
}

impl PunchVar {
    /// The springs' rest and give: the SDK's `motion::Punch`.
    pub fn new(target: Target, kind: PunchKind) -> PunchVar {
        let s = maimbrain::motion::Punch::new().spring;
        let rest = if kind == PunchKind::Scale { 1.0 } else { 0.0 };
        PunchVar { target, kind, springs: [s; 3], applied: [rest; 3] }
    }

    /// The keys it moves: a 3D node has a third.
    pub fn keys(&self, three: bool) -> &'static [&'static str] {
        match (self.kind, three) {
            (PunchKind::Scale, false) => &["sx", "sy"],
            (PunchKind::Scale, true) => &["sx", "sy", "sz"],
            (PunchKind::Move, false) => &["x", "y"],
            (PunchKind::Move, true) => &["x", "y", "z"],
        }
    }
}

/// The punches of `target` moving `key` go on from its value now (it was
/// just set without their kick in it): their last kick is forgotten, not
/// taken off.
fn rebase_punches(punches: &mut [PunchVar], target: Target, key: &str) {
    for p in punches.iter_mut().filter(|p| p.target == target) {
        let axes: &[usize] = match (p.kind, key) {
            (PunchKind::Scale, "sx") | (PunchKind::Move, "x") => &[0],
            (PunchKind::Scale, "sy") | (PunchKind::Move, "y") => &[1],
            (PunchKind::Scale, "sz") | (PunchKind::Move, "z") => &[2],
            (PunchKind::Scale, "scale") => &[0, 1, 2],
            _ => &[],
        };
        for &i in axes {
            p.applied[i] = if p.kind == PunchKind::Scale { 1.0 } else { 0.0 };
        }
    }
}

#[derive(Default)]
pub struct Libs {
    /// flux's group: active tweens, oldest first, and ones made by `after`
    /// waiting for theirs to end.
    pub tweens: Vec<Tween>,
    pub waiting: Vec<Tween>,
    pub timers: Vec<Timer>,
    pub anims: Vec<Anim>,
    pub sprites: Vec<SpriteAnim>,
    pub follow: Option<Follow>,
    /// Signals from the scene's last step (a walker's footfalls), heard first.
    pub pending: Vec<Due>,
    pub next_id: u32,
    /// Registry refs to free once the prelude has made this frame's calls.
    pub release: Vec<c_int>,
    /// `spring`'s values, oldest first (moved after flux, by the world's dt).
    pub springs: Vec<SpringVar>,
    /// `fx.punch`'s kicks (moved after Lua's frame, by real time, as the shake).
    pub punches: Vec<PunchVar>,
}

/// Reads and writes what tweens move (the scene, the camera, Lua tables).
pub trait Values {
    fn get(&mut self, t: Target, key: &str) -> Option<f64>;
    fn set(&mut self, t: Target, key: &str, v: f64);
    fn is_color(&self, t: Target, key: &str) -> bool;
}

fn mix_color(a: f64, b: f64, x: f64) -> f64 {
    let (a, b) = (a as u32, b as u32);
    let ch = |s: u32| {
        let (ca, cb) = (((a >> s) & 0xff) as f64, ((b >> s) & 0xff) as f64);
        ((ca + (cb - ca) * x).round().clamp(0.0, 255.0) as u32) << s
    };
    (ch(16) | ch(8) | ch(0)) as f64
}

impl Libs {
    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    pub fn tween_mut(&mut self, id: u32) -> Option<&mut Tween> {
        self.tweens.iter_mut().chain(self.waiting.iter_mut()).find(|t| t.id == id)
    }

    /// flux.clear: other started tweens of this target stop moving these
    /// keys, and so do springs (the newest motion wins).
    fn clear(&mut self, target: Target, keys: &[String], except: u32) {
        for t in &mut self.tweens {
            if t.target == target && t.inited && t.id != except {
                t.vars.retain(|v| !keys.contains(&v.key));
            }
        }
        self.drop_springs(|s| s.target == target && keys.contains(&s.key));
    }

    fn drop_springs(&mut self, gone: impl Fn(&SpringVar) -> bool) {
        let release = &mut self.release;
        self.springs.retain(|s| {
            if !gone(s) {
                return true;
            }
            if let Target::Table(r, _) = s.target {
                release.push(r);
            }
            false
        });
    }

    /// `spring(target, { key = goal }, opts)`: a spring already moving the
    /// key heads for the new goal from where it is, at its speed (with the
    /// new frequency and damping); else one starts from the value now, at
    /// rest. Tweens of the key stop. False if the key isn't a number there.
    /// A table target's ref is the spring's (or freed, when one has it).
    pub fn spring(&mut self, values: &mut dyn Values, target: Target, key: &str, goal: f64, frequency: f32, damping: f32) -> bool {
        for t in &mut self.tweens {
            if t.target == target && t.inited {
                t.vars.retain(|v| v.key != key);
            }
        }
        if let Some(s) = self.springs.iter_mut().find(|s| s.target == target && s.key == key) {
            (s.spring.target, s.spring.frequency, s.spring.damping) = (goal as f32, frequency, damping);
            if let Target::Table(r, _) = target {
                self.release.push(r);
            }
            return true;
        }
        let Some(now) = values.get(target, key) else {
            if let Target::Table(r, _) = target {
                self.release.push(r);
            }
            return false;
        };
        let spring = maimbrain::motion::Spring::new(now as f32, frequency, damping).to(goal as f32);
        self.springs.push(SpringVar { target, key: key.to_string(), spring });
        true
    }

    /// One frame of the springs: each value written; a settled one lands
    /// on its goal and stops.
    fn step_springs(&mut self, dt: f64, values: &mut dyn Values) {
        let mut done = Vec::new();
        for (i, s) in self.springs.iter_mut().enumerate() {
            if values.get(s.target, &s.key).is_none() {
                done.push(i); // its node was destroyed
                continue;
            }
            s.spring.update(dt as f32);
            // Settled: exactly on its goal (the spring snaps only on its next step).
            if s.spring.settled() {
                s.spring.snap(s.spring.target);
            }
            values.set(s.target, &s.key, s.spring.value as f64);
            rebase_punches(&mut self.punches, s.target, &s.key);
            if s.spring.settled() {
                done.push(i);
            }
        }
        for i in done.into_iter().rev() {
            let s = self.springs.remove(i);
            if let Target::Table(r, _) = s.target {
                self.release.push(r);
            }
        }
    }

    /// `fx.punch`: kicks the node's size (`amounts[0]` 0.2 is 20% bigger at
    /// once, springing back) or its place (moving out about that far on
    /// each axis and back). Another kick before it settles adds to it.
    pub fn punch(&mut self, target: Target, kind: PunchKind, amounts: [f32; 3]) {
        let i = match self.punches.iter().position(|p| p.target == target && p.kind == kind) {
            Some(i) => i,
            None => {
                self.punches.push(PunchVar::new(target, kind));
                self.punches.len() - 1
            }
        };
        let p = &mut self.punches[i];
        for (s, a) in p.springs.iter_mut().zip(amounts) {
            match kind {
                // The SDK's Punch::kick: up at once, springing back.
                PunchKind::Scale => {
                    let cap = a.abs() * 2.0;
                    s.value = (s.value + a).clamp(-cap, cap);
                    s.velocity = 0.0;
                }
                // Out from where it is: a velocity whose first swing peaks
                // near `a` (an underdamped spring's is ~0.62 v/ω at ζ 0.32).
                PunchKind::Move => s.velocity += a * s.frequency * std::f32::consts::TAU / 0.62,
            }
        }
    }

    /// One frame of the punches (after Lua's frame and the scene's motion):
    /// last frame's kick off, this frame's on. `three` says a target has a z.
    pub fn step_punches(&mut self, dt: f32, values: &mut dyn Values, three: impl Fn(Target) -> bool) {
        self.punches.retain_mut(|p| {
            let keys = p.keys(three(p.target));
            if values.get(p.target, keys[0]).is_none() {
                return false;
            }
            let mut rest = true;
            for (k, key) in keys.iter().enumerate() {
                let s = &mut p.springs[k];
                s.update(dt);
                if s.settled() {
                    // Exactly at rest, so a settled punch leaves nothing behind.
                    s.snap(s.target);
                } else {
                    rest = false;
                }
                let Some(v) = values.get(p.target, key) else { continue };
                let (v, was) = (v as f32, p.applied[k]);
                let (now, to) = match p.kind {
                    PunchKind::Scale => (1.0 + s.value, if was.abs() > 1e-4 { v / was * (1.0 + s.value) } else { v }),
                    PunchKind::Move => (s.value, v - was + s.value),
                };
                values.set(p.target, key, to as f64);
                p.applied[k] = now;
            }
            !rest
        });
    }

    /// A write beats motion (§4.3): tweens of this target stop moving the key.
    pub fn written(&mut self, target: Target, key: &str) {
        for t in &mut self.tweens {
            // A tween still waiting out its delay starts from the new value.
            if t.target == target && t.inited {
                t.vars.retain(|v| v.key != key);
            }
        }
        self.drop_springs(|s| s.target == target && s.key == key);
        self.rebase(target, key);
        if target == Target::Camera && matches!(key, "x" | "y") {
            self.follow = None;
        }
    }

    /// A punch goes on from a value set from outside (a write, a tween or a
    /// spring, a body the node follows): it has no kick in it.
    pub fn rebase(&mut self, target: Target, key: &str) {
        rebase_punches(&mut self.punches, target, key);
    }

    /// Stops a tween (flux's `:stop()`), started or waiting.
    pub fn stop(&mut self, id: u32) {
        for list in [&mut self.tweens, &mut self.waiting] {
            for t in list.iter().filter(|t| t.id == id) {
                self.release.extend(t.onstart.iter().chain(&t.onupdate).chain(&t.oncomplete));
                if let Target::Table(r, _) = t.target {
                    self.release.push(r);
                }
            }
            list.retain(|t| t.id != id);
        }
    }

    /// One frame of flux, hump.timer and anim8, and the Sprites' animations.
    pub fn update(&mut self, dt: f64, values: &mut dyn Values, sprite_frames: &mut dyn FnMut(NodeId, u32) -> bool) -> Vec<Due> {
        let mut due = std::mem::take(&mut self.pending);
        self.flux(dt, values, &mut due);
        self.step_springs(dt, values);
        self.timers(dt, &mut due);
        self.anims(dt, &mut due);
        self.sprites(dt, sprite_frames, &mut due);
        due
    }

    /// flux.lua's `update`: newest first.
    fn flux(&mut self, dt: f64, values: &mut dyn Values, due: &mut Vec<Due>) {
        let mut i = self.tweens.len();
        while i > 0 {
            i -= 1;
            if i >= self.tweens.len() {
                continue;
            }
            if self.tweens[i].delay > 0.0 {
                self.tweens[i].delay -= dt;
                continue;
            }
            if !self.tweens[i].inited {
                let (target, id) = (self.tweens[i].target, self.tweens[i].id);
                let keys: Vec<String> = self.tweens[i].vars.iter().map(|v| v.key.clone()).collect();
                self.clear(target, &keys, id);
                let t = &mut self.tweens[i];
                for v in &mut t.vars {
                    v.color = values.is_color(target, &v.key);
                    v.start = values.get(target, &v.key).unwrap_or(v.end);
                }
                t.inited = true;
            }
            let t = &mut self.tweens[i];
            for f in t.onstart.drain(..) {
                due.push(Due::Call { f, args: vec![], pass_self: false, timer: None });
                self.release.push(f);
            }
            t.progress += t.rate * dt;
            let p = t.progress;
            let x = if p >= 1.0 { 1.0 } else { flux_ease(&t.ease, p).unwrap_or(p) };
            for v in &t.vars {
                let val = if v.color { mix_color(v.start, v.end, x) } else { v.start + x * (v.end - v.start) };
                values.set(t.target, &v.key, val);
                rebase_punches(&mut self.punches, t.target, &v.key);
            }
            for f in &t.onupdate {
                due.push(Due::Call { f: *f, args: vec![], pass_self: false, timer: None });
            }
            if p >= 1.0 {
                if t.yoyo {
                    // There and back again, for ever.
                    t.progress = (p - 1.0).min(1.0);
                    for v in &mut t.vars {
                        (v.start, v.end) = (v.end, v.start);
                    }
                    continue;
                }
                let t = self.tweens.remove(i);
                if let Target::Table(r, _) = t.target {
                    self.release.push(r);
                }
                for f in &t.oncomplete {
                    due.push(Due::Call { f: *f, args: vec![], pass_self: false, timer: None });
                }
                self.release.extend(t.onupdate.iter().chain(&t.oncomplete));
                for next in t.then {
                    if let Some(k) = self.waiting.iter().position(|w| w.id == next) {
                        let w = self.waiting.remove(k);
                        self.tweens.push(w);
                    }
                }
            }
        }
    }

    /// hump.timer's `update`, in the order the timers were made.
    fn timers(&mut self, dt: f64, due: &mut Vec<Due>) {
        let mut done = Vec::new();
        for h in &mut self.timers {
            h.time += dt;
            if let Some(f) = h.during {
                due.push(Due::Call { f, args: vec![dt, (h.limit - h.time).max(0.0)], pass_self: false, timer: Some(h.id) });
            }
            while h.time >= h.limit && h.count > 0.0 {
                if let Some(f) = h.after {
                    due.push(Due::Call { f, args: vec![], pass_self: true, timer: Some(h.id) });
                }
                h.time -= h.limit;
                h.count -= 1.0;
                // An every(0, …) would loop for ever: once a frame.
                if h.limit <= 0.0 {
                    break;
                }
            }
            if h.count <= 0.0 {
                done.push(h.id);
            }
        }
        for id in done {
            self.cancel_timer(id);
        }
    }

    pub fn cancel_timer(&mut self, id: u32) {
        if let Some(i) = self.timers.iter().position(|t| t.id == id) {
            let t = self.timers.remove(i);
            self.release.extend(t.during.into_iter().chain(t.after));
        }
    }

    /// anim8's `Animation:update`.
    fn anims(&mut self, dt: f64, due: &mut Vec<Due>) {
        for (i, a) in self.anims.iter_mut().enumerate() {
            if !a.playing || a.total <= 0.0 {
                continue;
            }
            a.timer += dt;
            let loops = (a.timer / a.total).floor();
            if loops != 0.0 {
                a.timer -= a.total * loops;
                match a.on_loop {
                    OnLoop::Nothing => {}
                    OnLoop::Function(f) => due.push(Due::Loop { f, anim: i as u32, loops }),
                    OnLoop::PauseAtEnd => a.pause_at_end(),
                    OnLoop::PauseAtStart => a.pause_at_start(),
                }
            }
            if a.playing {
                a.position = a.seek();
            }
        }
    }

    /// Sprites playing their sheets' animations; `set_frame` shows a frame
    /// (false once the node is gone).
    fn sprites(&mut self, dt: f64, set_frame: &mut dyn FnMut(NodeId, u32) -> bool, due: &mut Vec<Due>) {
        self.sprites.retain_mut(|s| {
            if s.done || s.frames.is_empty() {
                return set_frame(s.node, *s.frames.last().unwrap_or(&1));
            }
            s.t += dt as f32 * s.speed;
            let n = s.frames.len();
            let mut idx = (s.t * s.fps).floor() as usize;
            if idx >= n {
                if s.once {
                    idx = n - 1;
                    s.done = true;
                    due.push(Due::Signal { node: s.node, name: "animationFinished", args: vec![] });
                } else {
                    idx %= n;
                }
            }
            set_frame(s.node, s.frames[idx])
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// flux.lua's curves at a few points (its own formulas).
    #[test]
    fn flux_easings_are_fluxs() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(flux_ease("linear", 0.3).unwrap(), 0.3));
        assert!(close(flux_ease("quadin", 0.5).unwrap(), 0.25));
        assert!(close(flux_ease("quadout", 0.5).unwrap(), 0.75));
        assert!(close(flux_ease("quadinout", 0.25).unwrap(), 0.125));
        // flux's back: p * p * (2.7 * p - 1.7).
        assert!(close(flux_ease("backin", 0.5).unwrap(), 0.25 * (1.35 - 1.7)));
        assert!(close(flux_ease("backout", 0.5).unwrap(), 1.0 - 0.25 * (1.35 - 1.7)));
        // easings.net's names reach the same curves.
        assert!(close(flux_ease("outBack", 0.37).unwrap(), flux_ease("backout", 0.37).unwrap()));
        assert!(close(flux_ease("inOutSine", 0.8).unwrap(), flux_ease("sineinout", 0.8).unwrap()));
        assert!(flux_ease("wobble", 0.5).is_none() && flux_ease("quad", 0.5).is_none());
        for e in ["quadin", "cubicout", "expoinout", "circin", "elasticout", "bounceout", "sineinout"] {
            // flux's expo and elastic start a hair off 0, as flux.lua's do.
            assert!(flux_ease(e, 0.0).unwrap().abs() < 2e-3, "{e}");
            assert!((flux_ease(e, 1.0).unwrap() - 1.0).abs() < 2e-3, "{e}");
        }
    }

    /// anim8's frame seeking and its looping.
    #[test]
    fn anim8_seeks_and_loops() {
        let mut a = Anim::new(vec![[0.0; 4]; 3], vec![0.1, 0.2, 0.1], OnLoop::Nothing);
        assert_eq!(a.intervals, [0.0, 0.1, 0.30000000000000004]);
        let mut libs = Libs { anims: vec![a.clone()], ..Libs::default() };
        let mut no = |_: NodeId, _: u32| true;
        struct None_;
        impl Values for None_ {
            fn get(&mut self, _: Target, _: &str) -> Option<f64> {
                None
            }
            fn set(&mut self, _: Target, _: &str, _: f64) {}
            fn is_color(&self, _: Target, _: &str) -> bool {
                false
            }
        }
        libs.update(0.15, &mut None_, &mut no);
        assert_eq!(libs.anims[0].position, 2);
        libs.update(0.2, &mut None_, &mut no);
        assert_eq!(libs.anims[0].position, 3);
        libs.update(0.1, &mut None_, &mut no);
        assert_eq!(libs.anims[0].position, 1, "looped: 0.45 is 0.05 into the next loop");
        a.on_loop = OnLoop::PauseAtEnd;
        libs.anims[0] = a;
        libs.update(0.5, &mut None_, &mut no);
        assert_eq!((libs.anims[0].position, libs.anims[0].playing), (3, false));
    }
}
