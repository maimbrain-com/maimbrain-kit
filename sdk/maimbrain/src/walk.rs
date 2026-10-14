//! Walking characters in 2D (side view): a body and two feet placed by a
//! step cycle that follows the **distance walked**, not time. A planted foot
//! stays where it landed on the ground (it never slides, at any speed), the
//! foot in the air swings to where it will land, and the body bobs and leans
//! with the gait. Speed changes are eased and the cycle's phase never jumps,
//! so a slow creep is one continuous stalk, never stop-start.
//!
//! The easy path is a feetless body and one foot sprite, `mb art frames <game>
//! <name> "<character>" --walker` (cell 0 the body, cell 1 a foot; the run
//! prints the [`WalkSprite`] line to paste). A walk-cycle sheet works too:
//! [`Walker::frame`] picks its frame from the distance walked.
//!
//! ```
//! use maimbrain::walk::Walker;
//! let mut girl = Walker::new(60.0).at(100.0);   // 60 units tall, standing at x = 100
//! let mut slide = 0.0f32;
//! for _ in 0..240 {
//!     let before = [girl.foot(0), girl.foot(1)];
//!     girl.drive(1.0 / 60.0, 45.0);               // eases to 45 units/s and moves her
//!     for i in 0..2 {
//!         if before[i].1 == 0.0 && girl.foot(i).1 == 0.0 { slide += (girl.foot(i).0 - before[i].0).abs(); }
//!     }
//! }
//! assert!(girl.x() > 220.0 && slide < 1e-3);    // she walked, and no planted foot moved
//! ```
//!
//! Everything advances only by the `dt` you pass (SPEC §3): update in
//! `Game::update`, draw in `Game::render`.

use std::f32::consts::PI;

use crate::gfx2d::{self, Image};
use crate::motion::Spring;

/// A foot landing: which foot, where (x on the ground), and how hard (0–1:
/// about 0.3 for a walk, 1 for a run): `foley.step(sound, pos, strength)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub foot: usize,
    pub x: f32,
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Foot {
    /// Where it stands (or last stood) on the ground.
    planted: f32,
    /// Where its step started.
    from: f32,
    x: f32,
    h: f32,
    air: bool,
    /// The point of the cycle (0–1) it lifted at.
    lift_q: f32,
}

/// A side-view walker: see the module docs. Positions are game units along
/// the ground (+x right); heights are units above the ground (up).
#[derive(Clone, Debug, PartialEq)]
pub struct Walker {
    /// Distance the body travels in one full cycle (each foot steps once).
    /// Slower things take shorter strides: a creep ~0.5× the height, a walk
    /// ~0.7×, a run ~1.4×.
    pub stride: f32,
    /// How high a foot lifts mid-step.
    pub lift: f32,
    /// The share of the cycle each foot is on the ground: over 0.5 walks (both
    /// feet down at each contact), under 0.5 runs (both feet in the air between steps).
    pub duty: f32,
    /// How far the body rises and falls each step: low at contact, high
    /// passing over the planted foot (a run: low mid-stance, high in flight).
    pub bob: f32,
    /// Lean toward the way it's moving (radians), once it moves.
    pub lean: f32,
    /// The feet's distance apart at rest (foot 0 behind, foot 1 ahead).
    pub spread: f32,
    /// [`Walker::drive`]: how quickly speed follows what you ask for (Hz; 1.5
    /// brisk, 0.6 a slow, deliberate creep). Never a step change.
    pub ease_hz: f32,
    /// Steps per second when it stops: a foot in the air finishes its step, a
    /// foot left behind steps back under the body.
    pub settle_hz: f32,
    /// Slower than this (units/s) is standing.
    pub min_speed: f32,
    x: f32,
    last_x: Option<f32>,
    pace: Spring,
    vel: f32,
    dir: f32,
    moving: bool,
    phase: f32,
    feet: [Foot; 2],
    amp: f32,
    lean_now: f32,
    lead: f32,
    steps: Vec<Step>,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Walker {
    /// A walk for a character `height` units tall (feet to the top of the head).
    pub fn new(height: f32) -> Walker {
        let h = height.max(1.0);
        let foot = Foot { planted: 0.0, from: 0.0, x: 0.0, h: 0.0, air: false, lift_q: 0.0 };
        Walker {
            stride: 0.7 * h,
            lift: 0.09 * h,
            duty: 0.6,
            bob: 0.025 * h,
            lean: 0.05,
            spread: 0.06 * h,
            ease_hz: 1.5,
            settle_hz: 1.6,
            min_speed: 0.02 * h,
            x: 0.0,
            last_x: None,
            pace: Spring::critical(0.0, 1.5),
            vel: 0.0,
            dir: 1.0,
            moving: false,
            phase: 0.0,
            feet: [foot; 2],
            amp: 0.0,
            lean_now: 0.0,
            lead: 0.0,
            steps: Vec::new(),
        }
    }

    /// A run: long strides, high feet, both feet off the ground between steps, a forward lean.
    pub fn running(height: f32) -> Walker {
        let h = height.max(1.0);
        Walker { stride: 1.4 * h, lift: 0.18 * h, duty: 0.35, bob: 0.04 * h, lean: 0.16, ease_hz: 2.0, ..Walker::new(h) }
    }

    /// A creep (a stalker closing in): short, flat, deliberate steps, both feet
    /// down most of the time, hunched forward, and slow to change speed, so
    /// it reads as one continuous stalk.
    pub fn creeping(height: f32) -> Walker {
        let h = height.max(1.0);
        Walker { stride: 0.5 * h, lift: 0.05 * h, duty: 0.75, bob: 0.015 * h, lean: 0.22, ease_hz: 0.6, settle_hz: 1.0, ..Walker::new(h) }
    }

    /// Puts it at `x`, standing, feet under the body (a spawn or a cut).
    pub fn at(mut self, x: f32) -> Walker {
        self.snap(x);
        self
    }

    /// Jumps to `x` standing still (a spawn, a respawn, a cut).
    pub fn snap(&mut self, x: f32) {
        self.x = x;
        self.last_x = Some(x);
        self.pace.snap(0.0);
        self.vel = 0.0;
        self.moving = false;
        self.amp = 0.0;
        self.lean_now = 0.0;
        self.lead = 0.0;
        for i in 0..2 {
            let h = self.home(i, x);
            self.feet[i] = Foot { planted: h, from: h, x: h, h: 0.0, air: false, lift_q: 0.0 };
        }
    }

    fn home(&self, i: usize, x: f32) -> f32 {
        x + if i == 0 { -0.5 } else { 0.5 } * self.spread * self.dir
    }

    /// Moves the body itself: speed eases toward `want` (units/s, signed:
    /// negative walks left), the body moves, the feet follow. Returns the new x.
    /// Use this for anything that walks under its own power (an enemy closing
    /// in, a character steered by the player); ask for a new speed whenever
    /// you like: it never changes at once.
    pub fn drive(&mut self, dt: f32, want: f32) -> f32 {
        if self.last_x.is_none() {
            self.snap(self.x);
        }
        self.pace.frequency = self.ease_hz.max(0.05);
        self.pace.target = want;
        let v0 = self.pace.value;
        self.pace.update(dt.max(0.0));
        let x = self.x + 0.5 * (v0 + self.pace.value) * dt.max(0.0);
        self.update(dt, x);
        x
    }

    /// Follows a body moved elsewhere (your sim, physics): `x` is where it is
    /// now. Returns this frame's footfalls. A jump of more than two strides
    /// in one frame is a teleport: the feet snap under it.
    pub fn update(&mut self, dt: f32, x: f32) -> &[Step] {
        self.steps.clear();
        let Some(prev) = self.last_x else {
            self.snap(x);
            return &self.steps;
        };
        let stride = self.stride.max(1e-3);
        if (x - prev).abs() > 2.0 * stride {
            self.snap(x);
            return &self.steps;
        }
        self.last_x = Some(x);
        self.x = x;
        if dt.is_nan() || dt <= 0.0 {
            return &self.steps;
        }
        let dx = x - prev;
        let v = dx / dt;
        self.vel += (v - self.vel) * (1.0 - (-dt * 10.0).exp());
        let was_moving = self.moving;
        self.moving = v.abs() > self.min_speed;
        if self.moving {
            self.dir = v.signum();
        }
        let duty = self.duty.clamp(0.2, 0.9);
        let off_home = |w: &Walker, i: usize, at: f32| !w.feet[i].air && (w.feet[i].planted - w.home(i, at)).abs() > 0.12 * stride;
        let air = self.feet.iter().any(|f| f.air);
        // Setting off from a stand: the foot behind steps first, now.
        if self.moving && !was_moving && !air {
            let back = if (self.feet[0].planted - self.feet[1].planted) * self.dir <= 0.0 { 0 } else { 1 };
            self.phase = (duty - 1e-4 - 0.5 * back as f32).rem_euclid(1.0);
        }
        let dphase = if self.moving {
            dx.abs() / stride
        } else if air || off_home(self, 0, x) || off_home(self, 1, x) {
            self.settle_hz * dt
        } else {
            0.0
        };
        // Where a foot lands: ahead of its home by half the distance the body
        // covers while it's planted, so it's planted centred under the body.
        let want_lead = if self.moving { self.dir * 0.5 * duty * stride * smooth(v.abs() / (3.0 * self.min_speed.max(1e-3))) } else { 0.0 };
        self.lead += (want_lead - self.lead) * (1.0 - (-dt * 12.0).exp());
        let old = self.phase;
        self.phase = (old + dphase).rem_euclid(64.0);
        let run_k = ((0.6 - duty) / 0.25).clamp(0.0, 1.0);
        for i in 0..2 {
            let off = 0.5 * i as f32;
            let (qa, qb) = (old + off, old + off + dphase);
            // Lift-offs and landings crossed this frame, in order.
            let mut k = qa.floor();
            while k <= qb {
                for (edge, landing) in [(k + duty, false), (k + 1.0, true)] {
                    if !(edge > qa && edge <= qb) {
                        continue;
                    }
                    let p = if dphase > 0.0 { prev + dx * ((edge - qa) / dphase) } else { x };
                    if landing {
                        if self.feet[i].air {
                            let at = self.home(i, p) + self.lead;
                            let len = (at - self.feet[i].from).abs();
                            let strength = ((len / stride).clamp(0.0, 1.0) * (0.35 + 0.65 * run_k)).clamp(0.05, 1.0);
                            let f = &mut self.feet[i];
                            f.planted = at;
                            f.air = false;
                            self.steps.push(Step { foot: i, x: at, strength });
                        }
                    } else if !self.feet[i].air && (self.moving || off_home(self, i, p)) {
                        let f = &mut self.feet[i];
                        f.air = true;
                        f.from = f.planted;
                        f.lift_q = duty;
                    }
                }
                k += 1.0;
            }
            let q = (self.phase + off).fract();
            // Starting off mid-cycle with this foot trailing: it steps now (a quicker step).
            if self.moving && !self.feet[i].air && q >= duty && q < 0.95 && (self.home(i, x) - self.feet[i].planted) * self.dir > 0.5 * duty * stride + 0.1 * stride {
                let f = &mut self.feet[i];
                f.air = true;
                f.from = f.planted;
                f.lift_q = q;
            }
            let to = self.home(i, x) + self.lead;
            let f = &mut self.feet[i];
            if f.air {
                let s = ((q - f.lift_q) / (1.0 - f.lift_q).max(1e-3)).clamp(0.0, 1.0);
                let len = (to - f.from).abs();
                f.x = f.from + (to - f.from) * smooth(s);
                // Up fast, down onto the ground; a short step lifts less.
                f.h = self.lift * (PI * s.powf(0.8)).sin() * (len / (0.5 * stride)).clamp(0.3, 1.0);
            } else {
                f.x = f.planted;
                f.h = 0.0;
            }
        }
        let k = 1.0 - (-dt * 6.0).exp();
        self.amp += (if self.moving { 1.0 } else { 0.0 } - self.amp) * k;
        let lean_to = if self.moving { self.dir * self.lean * smooth(v.abs() / (3.0 * self.min_speed.max(1e-3))) } else { 0.0 };
        self.lean_now += (lean_to - self.lean_now) * (1.0 - (-dt * 4.0).exp());
        &self.steps
    }

    /// The footfalls of the last update.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The body's x.
    pub fn x(&self) -> f32 {
        self.x
    }

    /// Its speed (units/s, signed), smoothed.
    pub fn speed(&self) -> f32 {
        self.vel
    }

    /// Moving faster than `min_speed` this frame (show an idle frame when not).
    pub fn moving(&self) -> bool {
        self.moving
    }

    /// 1 facing right, −1 facing left: the way it last moved.
    pub fn facing(&self) -> f32 {
        self.dir
    }

    /// The cycle's phase, 0–1: 0 is foot 0 landing, 0.5 foot 1 landing.
    pub fn phase(&self) -> f32 {
        self.phase.fract()
    }

    /// Foot `i` (0 the far foot, 1 the near one): (x on the ground, height above it).
    /// A planted foot's height is exactly 0.
    pub fn foot(&self, i: usize) -> (f32, f32) {
        let f = &self.feet[i.min(1)];
        (f.x, f.h)
    }

    /// Whether foot `i` is in the air.
    pub fn in_air(&self, i: usize) -> bool {
        self.feet[i.min(1)].air
    }

    /// Foot `i`'s pitch (radians, clockwise, with the facing): heel up as it
    /// pushes off, toes up before it lands, flat on the ground.
    pub fn foot_tilt(&self, i: usize) -> f32 {
        let f = &self.feet[i.min(1)];
        if !f.air {
            return 0.0;
        }
        let q = (self.phase + 0.5 * i.min(1) as f32).fract();
        let s = ((q - f.lift_q) / (1.0 - f.lift_q).max(1e-3)).clamp(0.0, 1.0);
        0.35 * (2.0 * PI * s).sin() * self.dir
    }

    /// Add to the body's height: the bob of the gait (units, up).
    pub fn bob(&self) -> f32 {
        let duty = self.duty.clamp(0.2, 0.9);
        let sign = if duty >= 0.5 { 1.0 } else { -1.0 };
        sign * self.amp * self.bob * (4.0 * PI * (self.phase - 0.5 * duty)).cos()
    }

    /// The body's lean (radians, clockwise in gfx2d's y-down space: toward
    /// the way it's moving). Rotate the body about its hips.
    pub fn lean_angle(&self) -> f32 {
        self.lean_now
    }

    /// Frame of an `n`-frame walk-cycle sheet (the whole cycle, two steps,
    /// frame 0 = foot 0 landing): advances with the distance walked, so set
    /// `stride` to the distance the sheet's cycle covers at the size you draw
    /// it. Show your idle frame when `!moving()`.
    pub fn frame(&self, n: usize) -> usize {
        let n = n.max(1);
        ((self.phase.fract() * n as f32) as usize).min(n - 1)
    }

    /// Debug overlay (draw it over the character in a debug build, and look
    /// at it with `mb shot`): a green tick under each planted foot, an orange
    /// one under a foot in the air, a line at the body. In world units with the
    /// ground at `ground_y` (translate for your camera first). Over drawn feet,
    /// a planted foot that drifts off its green tick is sliding.
    pub fn debug_draw(&self, ground_y: f32) {
        let u = (self.stride * 0.04).max(1.0);
        for i in 0..2 {
            let f = &self.feet[i];
            let c = if f.air { 0xff9a3cff } else { 0x3cff6aff };
            gfx2d::rect(f.x - 0.5 * u, ground_y - f.h - 3.0 * u, u, 3.0 * u, c);
            if f.air {
                gfx2d::rect(f.planted - 0.5 * u, ground_y, u, u, 0xff9a3c80);
            }
        }
        gfx2d::rect(self.x - 0.25 * u, ground_y - 12.0 * u - self.bob(), 0.5 * u, 10.0 * u, 0x6ad0ffff);
    }
}

/// Draws a [`Walker`] as a feetless body and a foot sprite (`mb art frames …
/// --walker` prints the line that makes one): the far foot, the body (bobbing,
/// leaning about its bottom), then the near foot, facing the way it walks.
/// `body` and `foot` are source rects trimmed to the drawing (texels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkSprite {
    pub img: Image,
    pub body: [f32; 4],
    pub foot: [f32; 4],
    /// Game units per texel (0.5: 2 texels per unit, the canvas's density).
    pub scale: f32,
    /// How high above the ground the body's bottom sits (units): where it
    /// meets the tops of the feet.
    pub ankle: f32,
    /// The far foot's tint (a shade darker reads as behind).
    pub far_tint: u32,
    /// Legs as lines from the body's bottom to each foot: (width, color), or none.
    pub legs: Option<(f32, u32)>,
    /// The name `mb shot`'s checks report it by ("" for none: not reported).
    pub name: &'static str,
    /// Its depth layer, as `ground::Figure::layer`.
    pub layer: i8,
}

impl WalkSprite {
    pub fn new(img: Image, body: [f32; 4], foot: [f32; 4]) -> WalkSprite {
        WalkSprite { img, body, foot, scale: 0.5, ankle: 0.5 * foot[3] * 0.5, far_tint: 0xb4b4bcff, legs: None, name: "", layer: 0 }
    }

    /// Reported to `mb shot`'s grounding check by this name, like a
    /// `ground::Figure`: overlaps with props, draw order, tap areas of the
    /// same name. (Its planted feet are its contact with the floor.)
    pub fn named(mut self, name: &'static str) -> WalkSprite {
        self.name = name;
        self
    }

    /// Its depth layer (`ground::Figure::layer`): what `mb shot` compares it
    /// with; a `ground::Scene` takes it in `Scene::other`.
    pub fn layer(mut self, layer: i8) -> WalkSprite {
        self.layer = layer.clamp(-3, 3);
        self
    }

    /// The box of what's drawn at the walker's place, the ground at
    /// `ground_y`: the body, bob included, down to the ground: [x, y, w, h].
    pub fn body_rect(&self, w: &Walker, ground_y: f32) -> [f32; 4] {
        let k = self.scale;
        let (bw, bh) = (self.body[2] * k, self.body[3] * k);
        let top = ground_y - self.ankle - w.bob() - bh;
        [w.x() - bw / 2.0, top, bw, ground_y - top]
    }

    /// What a tap on it should hit: [`body_rect`](Self::body_rect), head to
    /// feet, at least `ground::MIN_TAP` each way.
    pub fn hit(&self, w: &Walker, ground_y: f32) -> [f32; 4] {
        crate::ground::tap_size(self.body_rect(w, ground_y))
    }

    /// Units per texel (default 0.5); keeps the ankle at the same place on the foot.
    pub fn scale(mut self, k: f32) -> WalkSprite {
        self.ankle *= k / self.scale.max(1e-6);
        self.scale = k;
        self
    }

    pub fn ankle(mut self, units: f32) -> WalkSprite {
        self.ankle = units;
        self
    }

    pub fn legs(mut self, width: f32, rgba: u32) -> WalkSprite {
        self.legs = Some((width, rgba));
        self
    }

    /// Draws it at the walker's place, the ground at `ground_y`, `tint` on
    /// everything (0xffffffff for none).
    pub fn draw(&self, w: &Walker, ground_y: f32, tint: u32) {
        let k = self.scale;
        let face = w.facing();
        let hip_y = ground_y - self.ankle - w.bob();
        let foot = |i: usize, t: u32| {
            let (x, h) = w.foot(i);
            if let Some((lw, c)) = self.legs {
                gfx2d::line(w.x() + (i as f32 - 0.5) * 0.3 * self.body[2] * k * face, hip_y, x, ground_y - h - 0.6 * self.foot[3] * k, lw, c);
            }
            gfx2d::push();
            gfx2d::translate(x, ground_y - h);
            gfx2d::rotate(w.foot_tilt(i));
            gfx2d::scale(face, 1.0);
            let (fw, fh) = (self.foot[2] * k, self.foot[3] * k);
            gfx2d::sprite(self.img, self.foot, [-0.5 * fw, -fh, fw, fh], t);
            gfx2d::pop();
        };
        foot(0, mul(self.far_tint, tint));
        gfx2d::push();
        gfx2d::translate(w.x(), hip_y);
        gfx2d::rotate(w.lean_angle());
        gfx2d::scale(face, 1.0);
        let (bw, bh) = (self.body[2] * k, self.body[3] * k);
        gfx2d::sprite(self.img, self.body, [-0.5 * bw, -bh, bw, bh], tint);
        gfx2d::pop();
        foot(1, tint);
        if !self.name.is_empty() {
            let feet = (w.foot(0).0 - w.foot(1).0).abs() + self.foot[2] * k;
            crate::ground::report(self.name, self.body_rect(w, ground_y), w.x(), ground_y, feet, self.layer, true, false, false);
        }
    }
}

/// Two RGBA tints multiplied.
fn mul(a: u32, b: u32) -> u32 {
    let ch = |c: u32, s: u32| ((((a >> s) & 0xff) * ((c >> s) & 0xff) + 127) / 255) << s;
    ch(b, 24) | ch(b, 16) | ch(b, 8) | ch(b, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks with `drive` toward speeds from `want(t)`; returns the planted
    /// feet's total slide, the footfalls and the walker.
    fn run(mut w: Walker, seconds: f32, dt: f32, want: impl Fn(f32) -> f32) -> (f32, Vec<Step>, Walker) {
        let mut slide = 0.0;
        let mut steps = Vec::new();
        let n = (seconds / dt).round() as usize;
        for k in 0..n {
            let before = [w.foot(0), w.foot(1), (w.in_air(0) as u8 as f32, w.in_air(1) as u8 as f32)];
            w.drive(dt, want(k as f32 * dt));
            steps.extend_from_slice(w.steps());
            for i in 0..2 {
                let was_down = if i == 0 { before[2].0 == 0.0 } else { before[2].1 == 0.0 };
                if was_down && !w.in_air(i) && !w.steps().iter().any(|s| s.foot == i) {
                    slide += (w.foot(i).0 - before[i].0).abs();
                }
            }
        }
        (slide, steps, w)
    }

    #[test]
    fn planted_feet_never_slide_at_any_speed() {
        for (make, speeds) in [
            (Walker::new as fn(f32) -> Walker, [8.0, 30.0, 60.0, 120.0]),
            (Walker::running, [60.0, 150.0, 240.0, 400.0]),
            (Walker::creeping, [4.0, 12.0, 25.0, 40.0]),
        ] {
            for v in speeds {
                let (slide, steps, w) = run(make(60.0).at(0.0), 6.0, 1.0 / 60.0, |_| v);
                assert!(slide < 1e-4, "speed {v}: planted feet slid {slide}");
                // The feet really step: two footfalls per stride walked (after the ease-in), alternating.
                let cycles = w.x() / w.stride;
                assert!((steps.len() as f32 - 2.0 * cycles).abs() <= 2.5, "speed {v}: {} steps for {cycles} cycles", steps.len());
                assert!(steps.windows(2).all(|p| p[0].foot != p[1].foot), "feet alternate");
                // Each step carries the foot about a stride (not a shuffle).
                for p in steps.windows(3).skip(2) {
                    let d = (p[2].x - p[0].x).abs();
                    assert!((d - w.stride).abs() < 0.25 * w.stride, "speed {v}: step of {d} for stride {}", w.stride);
                }
            }
        }
    }

    #[test]
    fn feet_move_with_the_floor_while_the_body_moves_on() {
        // Only a planted foot can carry weight: at every moment while walking at
        // least one foot is down (a walk), and the feet go around the body.
        let mut w = Walker::new(60.0).at(0.0);
        let mut lifted = [0.0f32; 2];
        for _ in 0..300 {
            w.drive(1.0 / 60.0, 40.0);
            assert!(!(w.in_air(0) && w.in_air(1)), "a walk always has a foot down");
            for (i, l) in lifted.iter_mut().enumerate() {
                *l = l.max(w.foot(i).1);
                assert!((w.foot(i).0 - w.x()).abs() < 0.75 * w.stride, "foot {i} stays near the body");
            }
        }
        assert!(lifted.iter().all(|&l| l > 0.6 * w.lift), "both feet lift: {lifted:?}");
    }

    #[test]
    fn the_phase_follows_distance_and_stays_continuous_through_speed_changes() {
        // Speed asked for jumps around (a stalker that stops and goes): the
        // body's speed never jumps, and the phase moves exactly with distance.
        let want = |t: f32| if (t * 0.7).fract() < 0.5 { 30.0 } else { 0.0 };
        let mut w = Walker::creeping(80.0).at(0.0);
        let dt = 1.0 / 60.0;
        let (mut last_x, mut last_phase, mut last_v, mut was) = (w.x(), w.phase(), 0.0f32, false);
        for k in 0..600 {
            w.drive(dt, want(k as f32 * dt));
            let dx = w.x() - last_x;
            let v = dx / dt;
            assert!((v - last_v).abs() < 30.0 * 0.6 * 2.0 * PI * dt * 1.2 + 1e-3, "speed eases: {last_v} → {v}");
            let dp = (w.phase() - last_phase).rem_euclid(1.0);
            if w.moving() && was {
                assert!((dp - dx.abs() / w.stride).abs() < 1e-4, "phase follows distance: {dp} vs {}", dx / w.stride);
            } else if !w.moving() {
                assert!(dp <= w.settle_hz * dt + 1e-4, "settling, never jumping: {dp}");
            }
            (last_x, last_phase, last_v, was) = (w.x(), w.phase(), v, w.moving());
        }
        // A creep never stops dead between the asks: no frame at zero speed while it's asked to go.
        let mut w = Walker::creeping(80.0).at(0.0);
        let mut slowest = f32::MAX;
        for k in 0..600 {
            let x0 = w.x();
            w.drive(dt, 12.0);
            if k > 180 {
                slowest = slowest.min((w.x() - x0) / dt);
            }
        }
        assert!(slowest > 11.5, "a steady creep is steady: slowest {slowest}");
    }

    #[test]
    fn sheet_frames_advance_with_distance_not_time() {
        let mut a = Walker::new(60.0).at(0.0);
        let mut b = Walker::new(60.0).at(0.0);
        a.stride = 64.0;
        b.stride = 64.0;
        // A body moved elsewhere (accelerating), followed at 60 and 120 Hz.
        let path = |t: f32| 20.0 + 20.0 * t + 15.0 * t * t;
        a.update(1.0 / 60.0, path(0.0));
        b.update(1.0 / 120.0, path(0.0));
        a.update(1.0 / 60.0, path(1.0 / 60.0));
        b.update(1.0 / 120.0, path(1.0 / 120.0));
        let (p0, q0) = (a.phase(), b.phase());
        for k in 2..=120 {
            a.update(1.0 / 60.0, path(k as f32 / 60.0));
        }
        for k in 2..=240 {
            b.update(1.0 / 120.0, path(k as f32 / 120.0));
        }
        let walked = |p: f32, p0: f32| (p - p0).rem_euclid(1.0);
        let want = ((path(2.0) - path(1.0 / 60.0)) / 64.0).fract();
        assert!((walked(a.phase(), p0) - want).abs() < 1e-3, "{} vs {want}", walked(a.phase(), p0));
        let want = ((path(2.0) - path(1.0 / 120.0)) / 64.0).fract();
        assert!((walked(b.phase(), q0) - want).abs() < 1e-3);
        assert!((a.phase() - b.phase()).abs() < 0.01);
        assert_eq!(a.frame(8), b.frame(8));
        // Standing still: the frame doesn't move.
        let f = a.frame(8);
        for _ in 0..200 {
            a.update(1.0 / 60.0, path(2.0));
        }
        let settled = a.frame(8);
        for _ in 0..60 {
            a.update(1.0 / 60.0, path(2.0));
        }
        assert_eq!(a.frame(8), settled, "stopped: frame {f} settles and holds");
    }

    #[test]
    fn stopping_finishes_the_step_and_settles_even_from_a_run() {
        for make in [Walker::new as fn(f32) -> Walker, Walker::running, Walker::creeping] {
            let (_, _, mut w) = run(make(60.0).at(0.0), 3.0, 1.0 / 60.0, |_| 200.0);
            for _ in 0..400 {
                w.drive(1.0 / 60.0, 0.0);
            }
            for i in 0..2 {
                assert!(!w.in_air(i) && w.foot(i).1 == 0.0, "both feet down");
                assert!((w.foot(i).0 - w.home(i, w.x())).abs() <= 0.12 * w.stride + 1e-3, "under the body");
            }
            assert!(w.bob().abs() < 0.05 && w.lean_angle().abs() < 0.01, "settled");
            let p = w.phase();
            w.drive(1.0 / 60.0, 0.0);
            assert_eq!(w.phase(), p);
        }
    }

    #[test]
    fn it_replays_and_matches_at_120_hz() {
        let want = |t: f32| if t < 2.0 { 50.0 } else { -35.0 };
        let (_, a, wa) = run(Walker::new(60.0).at(0.0), 4.0, 1.0 / 60.0, want);
        let (_, b, _) = run(Walker::new(60.0).at(0.0), 4.0, 1.0 / 60.0, want);
        assert_eq!(a, b);
        let (_, c, wc) = run(Walker::new(60.0).at(0.0), 4.0, 1.0 / 120.0, want);
        assert_eq!(a.len(), c.len());
        for (x, y) in a.iter().zip(&c) {
            assert!(x.foot == y.foot && (x.x - y.x).abs() < 0.03 * wa.stride, "{x:?} {y:?}");
        }
        assert!((wa.x() - wc.x()).abs() < 0.5);
        assert_eq!(wa.facing(), -1.0, "turned around");
    }

    #[test]
    fn bob_and_lean_follow_the_gait() {
        let mut w = Walker::running(60.0).at(0.0);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for k in 0..300 {
            w.drive(1.0 / 60.0, 160.0);
            if k > 120 {
                lo = lo.min(w.bob());
                hi = hi.max(w.bob());
            }
        }
        assert!(lo < -0.8 * w.bob && hi > 0.8 * w.bob, "{lo} {hi}");
        assert!((w.lean_angle() - w.lean).abs() < 0.01, "leans into the run");
        assert!(w.foot_tilt(0) != 0.0 || w.foot_tilt(1) != 0.0, "a foot in the air pitches");
    }
}
