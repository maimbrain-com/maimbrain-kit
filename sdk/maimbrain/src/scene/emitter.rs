//! A [`Kind::Emitter`](super::Kind)'s particles: the SDK's juice particles
//! (`juice::Particles`) by preset, emitted at the node's place in the world
//! (`emit(n)`, or `rate` a second while `emitting`), and living in the
//! world from then on. The scene's own one is `fx.burst`'s.

use crate::juice::{Look, Particle, Particles};

/// The presets (§5 [2d], [juice]): the juice kit's, plus dust and smoke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Sparkles,
    Confetti,
    Stars,
    Burst,
    Sparks,
    Ring,
    Dust,
    Smoke,
}

pub const PRESETS: &[(&str, Preset)] = &[
    ("sparkles", Preset::Sparkles),
    ("confetti", Preset::Confetti),
    ("stars", Preset::Stars),
    ("burst", Preset::Burst),
    ("sparks", Preset::Sparks),
    ("ring", Preset::Ring),
    ("dust", Preset::Dust),
    ("smoke", Preset::Smoke),
];

impl Preset {
    /// A preset by name, its singular too (`"sparkle"` is sparkles).
    pub fn named(s: &str) -> Option<Preset> {
        PRESETS.iter().find(|(n, _)| *n == s || n.strip_suffix('s') == Some(s)).map(|p| p.1)
    }

    pub fn name(self) -> &'static str {
        PRESETS.iter().find(|p| p.1 == self).map_or("sparkles", |p| p.0)
    }
}

#[derive(Clone, Debug)]
pub struct Emitter {
    pub particles: Particles,
    pub preset: Preset,
    /// Particles a second while `emitting`.
    pub rate: f32,
    pub emitting: bool,
    acc: f32,
    /// The colours they come in (empty: the preset's white or warm).
    pub colors: Vec<u32>,
    /// Overrides for the preset's own: size (units), life (s), speed (units/s),
    /// spread (radians either side of up), gravity (units/s²).
    pub size: Option<f32>,
    pub life: Option<f32>,
    pub speed: Option<f32>,
    pub spread: Option<f32>,
    pub gravity: Option<f32>,
    pub additive: bool,
}

impl Emitter {
    pub fn new(seed: u64) -> Emitter {
        Emitter { particles: Particles::new(seed), preset: Preset::Sparkles, rate: 0.0, emitting: true, acc: 0.0, colors: Vec::new(), size: None, life: None, speed: None, spread: None, gravity: None, additive: false }
    }

    fn color(&self, i: usize) -> u32 {
        if self.colors.is_empty() {
            match self.preset {
                Preset::Dust => 0xc8b8a0ff,
                Preset::Smoke => 0x9a9aa4c0,
                Preset::Confetti => [0xff5a6eff, 0xffd24aff, 0x4ad0ffff, 0x7cff6aff, 0xc77dffff][i % 5],
                _ => 0xfff2c4ff,
            }
        } else {
            self.colors[i % self.colors.len()]
        }
    }

    /// `n` particles from (x, y), in world units.
    pub fn emit(&mut self, x: f32, y: f32, n: usize) {
        let before = self.particles.len();
        let c = self.color(0);
        let p = &mut self.particles;
        match self.preset {
            Preset::Sparkles => p.sparkles(x, y, 18.0, n, c),
            Preset::Confetti => {
                let colors: Vec<u32> = (0..5).map(|i| self.color(i)).collect();
                self.particles.confetti(x, y, n, &colors);
            }
            Preset::Stars => p.stars(x, y, n, c),
            Preset::Burst => p.burst(x, y, n, c),
            Preset::Sparks => p.sparks(x, y, -std::f32::consts::FRAC_PI_2, 1.2, n, c),
            Preset::Ring => {
                for _ in 0..n.max(1) {
                    p.ring(x, y, 40.0, c);
                }
            }
            Preset::Dust | Preset::Smoke => {
                let smoke = self.preset == Preset::Smoke;
                for i in 0..n {
                    let r = &mut p.rng;
                    let a = -std::f32::consts::FRAC_PI_2 + r.range(-1.2, 1.2);
                    let sp = if smoke { r.range(15.0, 45.0) } else { r.range(30.0, 90.0) };
                    let mut q = Particle::new(Look::Dot, x + r.range(-4.0, 4.0), y, c);
                    let _ = i;
                    q.vx = a.cos() * sp;
                    q.vy = a.sin() * sp;
                    q.drag = if smoke { 0.8 } else { 3.0 };
                    q.gravity = if smoke { -20.0 } else { 60.0 };
                    q.life = if smoke { r.range(1.0, 1.8) } else { r.range(0.4, 0.8) };
                    q.size = if smoke { r.range(6.0, 10.0) } else { r.range(2.0, 4.0) };
                    q.size_end = if smoke { q.size * 2.5 } else { 0.0 };
                    p.add(q);
                }
            }
        }
        // The node's overrides, on what was just made. (The pool may have
        // dropped its oldest to make room, so count from the end.)
        let _ = before;
        let made = if self.preset == Preset::Ring { n.max(1) } else { n }.min(self.particles.len());
        let start = self.particles.len() - made;
        let colors: Vec<u32> = (0..made).map(|i| self.color(i)).collect();
        for (k, q) in self.particles.list[start..].iter_mut().enumerate() {
            if !self.colors.is_empty() && self.preset != Preset::Confetti {
                q.color = colors[k];
            }
            if let Some(s) = self.size {
                let k = if q.size > 0.0 { q.size_end / q.size } else { 0.0 };
                q.size = s;
                q.size_end = s * k;
            }
            if let Some(l) = self.life {
                q.life = l.max(0.01);
            }
            if self.speed.is_some() || self.spread.is_some() {
                let sp = (q.vx * q.vx + q.vy * q.vy).sqrt();
                let sp = self.speed.unwrap_or(sp);
                let a = match self.spread {
                    Some(s) => -std::f32::consts::FRAC_PI_2 + self.particles.rng.range(-s, s),
                    None => q.vy.atan2(q.vx),
                };
                (q.vx, q.vy) = (a.cos() * sp, a.sin() * sp);
            }
            if let Some(g) = self.gravity {
                q.gravity = g;
            }
            if self.additive {
                q.look = Look::Dot;
            }
        }
    }

    /// One update: a stream at `rate` from (x, y), and the particles move.
    pub fn step(&mut self, dt: f32, x: f32, y: f32) {
        if self.emitting && self.rate > 0.0 {
            self.acc += self.rate * dt;
            let n = self.acc.floor();
            if n >= 1.0 {
                self.acc -= n;
                self.emit(x, y, n as usize);
            }
        }
        self.particles.update(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_by_name_and_overrides() {
        assert_eq!(Preset::named("sparkle"), Some(Preset::Sparkles));
        assert_eq!(Preset::named("smoke"), Some(Preset::Smoke));
        assert_eq!(Preset::named("glitter"), None);
        let mut e = Emitter::new(1);
        e.preset = Preset::Burst;
        (e.life, e.size, e.colors) = (Some(2.0), Some(3.0), vec![0xff0000ff]);
        e.emit(10.0, 20.0, 5);
        assert_eq!(e.particles.len(), 5);
        assert!(e.particles.list.iter().all(|p| p.life == 2.0 && p.size == 3.0 && p.color == 0xff0000ff));
        // A stream: 30 a second for half a second.
        let mut s = Emitter::new(2);
        s.rate = 30.0;
        for _ in 0..30 {
            s.step(1.0 / 60.0, 0.0, 0.0);
        }
        assert_eq!(s.particles.len(), 15);
    }
}
