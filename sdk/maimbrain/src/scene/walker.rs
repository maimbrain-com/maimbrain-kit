//! A [`Kind::Walker`](super::Kind): a side-view figure whose feet plant
//! as it walks (the SDK's [`walk::Walker`](crate::walk::Walker)), drawn as
//! a feetless body and a foot from its image (`generate(kind: "walker")`'s
//! two cells). Its `y` is its feet; however it moves (its `vx`, a tween, a
//! write to `x`, a behaviour), the feet follow the distance walked.

use crate::walk::{Step, Walker};

#[derive(Clone, Debug)]
pub struct WalkerNode {
    pub walk: Walker,
    /// The body and the foot in its image (pixels of the picture).
    pub body: [f32; 4],
    pub foot: [f32; 4],
    /// Its height in units, feet to head.
    pub height: f32,
    /// The gait's full-stride speed: moving near it runs, well under walks.
    pub speed: f32,
}

impl WalkerNode {
    /// Without a picture it's a greybox: a body and feet as boxes.
    pub fn new(height: f32) -> WalkerNode {
        WalkerNode { walk: Walker::new(height), body: [0.0, 0.0, 24.0, 52.0], foot: [0.0, 0.0, 14.0, 16.0], height, speed: 150.0 }
    }

    /// Units a pixel of its picture: the body and half the foot make its height.
    pub fn scale(&self) -> f32 {
        let px = self.body[3] + 0.5 * self.foot[3];
        if px > 0.0 { self.height / px } else { 1.0 }
    }

    /// Body to ground: where the body's bottom sits (units above the feet).
    pub fn ankle(&self) -> f32 {
        0.5 * self.foot[3] * self.scale()
    }

    /// A new height: the gait's proportions follow.
    pub fn set_height(&mut self, h: f32) {
        self.height = h.max(1.0);
        let x = self.walk.x();
        self.walk = Walker::new(self.height).at(x);
    }

    /// One update with the body at `x`: the gait blends from a walk to a run
    /// by speed, and the footfalls come back.
    pub fn step(&mut self, dt: f32, x: f32) -> Vec<Step> {
        let run = if self.speed > 0.0 { (self.walk.speed().abs() / self.speed).clamp(0.0, 1.0) } else { 0.0 };
        let h = self.height;
        let w = &mut self.walk;
        let lerp = |a: f32, b: f32| a + (b - a) * run;
        (w.stride, w.lift, w.duty, w.lean) = (lerp(0.7, 1.4) * h, lerp(0.09, 0.18) * h, lerp(0.6, 0.35), lerp(0.05, 0.16));
        w.update(dt, x).to_vec()
    }
}
