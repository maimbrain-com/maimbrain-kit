//! Juice presets: confetti, sparkles and star bursts, floating "+100"
//! popups, a combo meter, screen flashes and vignette pulses. Cheap (a
//! fixed pool, a few draw calls per particle) and composable.
//!
//! Like everything in [`crate::motion`], they advance only by `dt` and draw
//! from their own seeded [`Rng`], so they replay exactly: seed them from
//! your game's `Rng` (itself from `sys::rand_seed`), `update` them in
//! `Game::update` and `draw` them in `Game::render`.
//!
//! ```ignore
//! let mut fx = juice::Particles::new(rng.next_u32() as u64);
//! fx.confetti(180.0, 320.0, 80, &theme.confetti);   // on a new best
//! fx.update(dt);                                     // in update
//! fx.draw();                                         // in render
//! ```

use std::f32::consts::TAU;

use crate::Rng;
use crate::gfx2d::{self, Blend, Font};
use crate::motion::{Ease, Pulse, Punch};
use crate::ui::color::{darken, fade, lighten, mix, with_alpha};
use crate::ui::layout::Rect;
use crate::ui::shape::{disc, ring, soft_disc, sparkle, star_points, vignette};
use crate::ui::text::{VAlign, text};
use crate::ui::{Theme, widgets};

/// What a particle looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A soft round dot (puffs, glows; additive).
    Dot,
    /// A paper rectangle that tumbles (flips) as it falls.
    Confetti,
    /// A five-pointed star that spins.
    Star,
    /// A four-pointed twinkle.
    Sparkle,
    /// A streak along its velocity (sparks; additive).
    Spark,
    /// An expanding ring (shockwaves).
    Ring,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// Downward acceleration (units/s²) and velocity damping per second.
    pub gravity: f32,
    pub drag: f32,
    pub age: f32,
    pub life: f32,
    /// Size (radius or half-length) at birth and at death.
    pub size: f32,
    pub size_end: f32,
    pub color: u32,
    pub rot: f32,
    pub spin: f32,
    pub look: Look,
    /// Confetti's tumble phase and rate.
    pub flip: f32,
    pub flip_rate: f32,
}

impl Particle {
    pub fn new(look: Look, x: f32, y: f32, color: u32) -> Particle {
        Particle { x, y, vx: 0.0, vy: 0.0, gravity: 0.0, drag: 0.0, age: 0.0, life: 1.0, size: 6.0, size_end: 0.0, color, rot: 0.0, spin: 0.0, look, flip: 0.0, flip_rate: 0.0 }
    }
}

/// A pool of particles (oldest replaced when full).
#[derive(Clone, Debug)]
pub struct Particles {
    pub list: Vec<Particle>,
    pub cap: usize,
    pub rng: Rng,
}

impl Particles {
    /// Seed from your game's `Rng` so bursts replay.
    pub fn new(seed: u64) -> Particles {
        Particles { list: Vec::new(), cap: 600, rng: Rng::new(seed) }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn add(&mut self, p: Particle) {
        if self.list.len() >= self.cap {
            self.list.remove(0);
        }
        self.list.push(p);
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.list {
            p.age += dt;
            let k = (-p.drag * dt).exp();
            p.vx *= k;
            p.vy = p.vy * k + p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.rot += p.spin * dt;
            p.flip += p.flip_rate * dt;
        }
        self.list.retain(|p| p.age < p.life);
    }

    pub fn draw(&self) {
        gfx2d::antialias(true);
        for p in &self.list {
            let t = (p.age / p.life).clamp(0.0, 1.0);
            let size = p.size + (p.size_end - p.size) * Ease::QuadOut.at(t);
            // Fade over the last 30% of life.
            let a = if t > 0.7 { 1.0 - (t - 0.7) / 0.3 } else { 1.0 };
            let c = fade(p.color, a);
            match p.look {
                Look::Dot => {
                    gfx2d::blend(Blend::Add);
                    soft_disc(p.x, p.y, size, size * 0.8, c);
                    gfx2d::blend(Blend::Alpha);
                }
                Look::Confetti => {
                    // A tumbling paper strip: its height shrinks and grows as it flips over.
                    let (s, co) = p.rot.sin_cos();
                    let (hw, hh) = (size, size * 0.55 * p.flip.cos());
                    let shade = if p.flip.cos() < 0.0 { darken(c, 0.25) } else { c };
                    let corner = |x: f32, y: f32| [p.x + x * co - y * s, p.y + x * s + y * co];
                    let q = [corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)];
                    gfx2d::poly(&[q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1], q[3][0], q[3][1]], shade);
                }
                Look::Star => gfx2d::poly(&star_points(p.x, p.y, size, 0.48, 5, p.rot), c),
                Look::Sparkle => {
                    gfx2d::blend(Blend::Add);
                    soft_disc(p.x, p.y, size * 0.6, size * 0.8, fade(c, 0.5));
                    sparkle(p.x, p.y, size, p.rot, c);
                    gfx2d::blend(Blend::Alpha);
                }
                Look::Spark => {
                    let sp = (p.vx * p.vx + p.vy * p.vy).sqrt().max(1e-3);
                    let len = (size * 0.06 * sp).min(size * 6.0).max(size);
                    let (ux, uy) = (p.vx / sp, p.vy / sp);
                    gfx2d::blend(Blend::Add);
                    gfx2d::line(p.x - ux * len, p.y - uy * len, p.x, p.y, (size * 0.5).max(1.0), c);
                    gfx2d::blend(Blend::Alpha);
                }
                Look::Ring => {
                    let w = (p.size_end.max(p.size) * 0.12 * (1.0 - t)).max(1.0);
                    ring(p.x, p.y, size, w, c);
                }
            }
        }
        gfx2d::antialias(false);
    }

    /// Paper confetti bursting up from (x, y) and fluttering down.
    pub fn confetti(&mut self, x: f32, y: f32, count: usize, colors: &[u32]) {
        for i in 0..count {
            let r = &mut self.rng;
            let a = -std::f32::consts::FRAC_PI_2 + r.range(-0.9, 0.9);
            let sp = r.range(260.0, 620.0);
            let mut p = Particle::new(Look::Confetti, x, y, colors[i % colors.len().max(1)]);
            p.vx = a.cos() * sp;
            p.vy = a.sin() * sp;
            p.gravity = 520.0;
            p.drag = 2.2;
            p.life = r.range(1.6, 2.6);
            p.size = r.range(4.0, 7.0);
            p.size_end = p.size;
            p.rot = r.range(0.0, TAU);
            p.spin = r.range(-9.0, 9.0);
            p.flip = r.range(0.0, TAU);
            p.flip_rate = r.range(6.0, 14.0);
            self.add(p);
        }
    }

    /// Two confetti cannons from the bottom corners of `area`, aimed inward.
    pub fn confetti_cannons(&mut self, area: Rect, count: usize, colors: &[u32]) {
        for side in [0.0f32, 1.0] {
            for i in 0..count / 2 {
                let r = &mut self.rng;
                let base = if side == 0.0 { -1.05 } else { -std::f32::consts::PI + 1.05 };
                let a = base + r.range(-0.3, 0.3);
                let sp = r.range(500.0, 900.0);
                let x = area.x + area.w * side;
                let mut p = Particle::new(Look::Confetti, x, area.bottom(), colors[(i + side as usize) % colors.len().max(1)]);
                p.vx = a.cos() * sp;
                p.vy = a.sin() * sp;
                p.gravity = 480.0;
                p.drag = 1.9;
                p.life = r.range(2.0, 3.2);
                p.size = r.range(4.0, 7.0);
                p.size_end = p.size;
                p.rot = r.range(0.0, TAU);
                p.spin = r.range(-8.0, 8.0);
                p.flip = r.range(0.0, TAU);
                p.flip_rate = r.range(6.0, 13.0);
                self.add(p);
            }
        }
    }

    /// Twinkles scattered around (x, y) within `radius`.
    pub fn sparkles(&mut self, x: f32, y: f32, radius: f32, count: usize, color: u32) {
        for _ in 0..count {
            let r = &mut self.rng;
            let a = r.range(0.0, TAU);
            let d = radius * r.f32().sqrt();
            let mut p = Particle::new(Look::Sparkle, x + a.cos() * d, y + a.sin() * d, color);
            p.vx = a.cos() * r.range(10.0, 50.0);
            p.vy = a.sin() * r.range(10.0, 50.0) - 20.0;
            p.drag = 2.0;
            p.life = r.range(0.45, 0.9);
            p.size = r.range(5.0, 11.0);
            p.size_end = 0.0;
            p.rot = r.range(0.0, 0.8);
            p.spin = r.range(-2.0, 2.0);
            self.add(p);
        }
    }

    /// Spinning stars bursting out of (x, y).
    pub fn stars(&mut self, x: f32, y: f32, count: usize, color: u32) {
        for i in 0..count {
            let r = &mut self.rng;
            let a = i as f32 / count.max(1) as f32 * TAU + r.range(-0.25, 0.25);
            let sp = r.range(160.0, 340.0);
            let mut p = Particle::new(Look::Star, x, y, color);
            p.vx = a.cos() * sp;
            p.vy = a.sin() * sp;
            p.gravity = 260.0;
            p.drag = 2.6;
            p.life = r.range(0.7, 1.1);
            p.size = r.range(7.0, 12.0);
            p.size_end = 2.0;
            p.spin = r.range(-7.0, 7.0);
            self.add(p);
        }
    }

    /// A round puff of soft dots (hits, pickups, landings).
    pub fn burst(&mut self, x: f32, y: f32, count: usize, color: u32) {
        for _ in 0..count {
            let r = &mut self.rng;
            let a = r.range(0.0, TAU);
            let sp = r.range(60.0, 260.0);
            let mut p = Particle::new(Look::Dot, x, y, color);
            p.vx = a.cos() * sp;
            p.vy = a.sin() * sp;
            p.drag = 4.0;
            p.life = r.range(0.35, 0.7);
            p.size = r.range(4.0, 9.0);
            p.size_end = 0.0;
            self.add(p);
        }
    }

    /// Sparks flying from (x, y) in direction `angle` (radians, 0 = right,
    /// clockwise) within ± `spread`.
    #[allow(clippy::too_many_arguments)]
    pub fn sparks(&mut self, x: f32, y: f32, angle: f32, spread: f32, count: usize, color: u32) {
        for _ in 0..count {
            let r = &mut self.rng;
            let a = angle + r.range(-spread, spread);
            let sp = r.range(240.0, 560.0);
            let mut p = Particle::new(Look::Spark, x, y, color);
            p.vx = a.cos() * sp;
            p.vy = a.sin() * sp;
            p.gravity = 600.0;
            p.drag = 3.0;
            p.life = r.range(0.3, 0.6);
            p.size = r.range(2.0, 3.5);
            p.size_end = p.size;
            self.add(p);
        }
    }

    /// A shockwave ring growing to `radius`.
    pub fn ring(&mut self, x: f32, y: f32, radius: f32, color: u32) {
        let mut p = Particle::new(Look::Ring, x, y, color);
        p.life = 0.45;
        p.size = radius * 0.15;
        p.size_end = radius;
        self.add(p);
    }

    /// The works, for a celebration: a ring, stars, sparkles and confetti.
    pub fn celebrate(&mut self, x: f32, y: f32, theme: &Theme) {
        self.ring(x, y, 120.0, with_alpha(theme.gold, 0.9));
        self.stars(x, y, 10, theme.gold);
        self.sparkles(x, y, 90.0, 14, lighten(theme.gold, 0.5));
        self.confetti(x, y, 70, &theme.confetti);
    }
}

/// A floating label ("+100", "PERFECT!") that pops in, rises and fades.
#[derive(Clone, Debug)]
pub struct Popup {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub age: f32,
    pub life: f32,
    pub color: u32,
    pub size: f32,
    /// How far it rises over its life.
    pub rise: f32,
}

/// Floating text popups.
#[derive(Clone, Debug, Default)]
pub struct Popups {
    pub list: Vec<Popup>,
    pub font: Option<Font>,
    /// Outline color (default: a dark ink).
    pub outline: Option<u32>,
}

impl Popups {
    pub fn new() -> Popups {
        Popups::default()
    }

    /// Text at (x, y), in `color`, `size` units tall.
    pub fn spawn(&mut self, x: f32, y: f32, s: &str, color: u32, size: f32) {
        if self.list.len() >= 32 {
            self.list.remove(0);
        }
        self.list.push(Popup { text: s.to_string(), x, y, age: 0.0, life: 0.9, color, size, rise: 46.0 });
    }

    /// "+points" sized by how many.
    pub fn score(&mut self, x: f32, y: f32, points: i64, color: u32) {
        let size = (20.0 + (points.max(1) as f32).log10() * 6.0).min(40.0);
        let sign = if points >= 0 { "+" } else { "" };
        self.spawn(x, y, &format!("{sign}{}", crate::ui::fmt_int(points)), color, size);
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.list {
            p.age += dt;
        }
        self.list.retain(|p| p.age < p.life);
    }

    pub fn draw(&self) {
        let font = self.font.unwrap_or(Font::SansBold);
        let ink = self.outline.unwrap_or(0x1a0f2eff);
        for p in &self.list {
            let t = p.age / p.life;
            let pop = Ease::BackOut.at((p.age / 0.22).min(1.0));
            let y = p.y - p.rise * Ease::ExpoOut.at(t);
            let a = if t > 0.6 { 1.0 - (t - 0.6) / 0.4 } else { 1.0 };
            let size = crate::ui::theme::fit(font, p.size);
            gfx2d::push();
            gfx2d::translate(p.x, y);
            gfx2d::scale(pop.max(0.01), pop.max(0.01));
            text(&p.text).font(font).size(size).color(p.color).outline(0.09, ink).soft_shadow(0.0, size * 0.08, 0.05, 0x00000060).alpha(a).middle().draw(0.0, 0.0);
            gfx2d::pop();
        }
    }
}

/// A combo counter with a draining timer: `hit` on every success, it
/// breaks if the next one doesn't come within `window` seconds.
#[derive(Clone, Copy, Debug)]
pub struct Combo {
    pub count: u32,
    pub best: u32,
    /// Seconds a combo survives without a hit.
    pub window: f32,
    left: f32,
    pub punch: Punch,
    /// Fires when a combo of 2 or more breaks.
    pub broke: Pulse,
    broke_count: u32,
}

impl Combo {
    pub fn new(window: f32) -> Combo {
        Combo { count: 0, best: 0, window, left: 0.0, punch: Punch::new(), broke: Pulse::new(0.5), broke_count: 0 }
    }

    /// A success: returns the new count.
    pub fn hit(&mut self) -> u32 {
        self.count += 1;
        self.best = self.best.max(self.count);
        self.left = self.window;
        self.punch.kick(0.22 + 0.02 * self.count.min(10) as f32);
        self.count
    }

    /// Ends the combo now (a miss). Returns the count it had.
    pub fn break_combo(&mut self) -> u32 {
        let c = self.count;
        if c >= 2 {
            self.broke.fire();
            self.broke_count = c;
        }
        self.count = 0;
        self.left = 0.0;
        c
    }

    /// Advances the timer; returns the count of a combo that just timed out.
    pub fn update(&mut self, dt: f32) -> Option<u32> {
        self.punch.update(dt);
        self.broke.update(dt);
        if self.count > 0 {
            self.left -= dt;
            if self.left <= 0.0 {
                return Some(self.break_combo());
            }
        }
        None
    }

    /// Score multiplier: 1 + count / 5, rounded down (×1, ×2 at 5, ×3 at 10…).
    pub fn multiplier(&self) -> u32 {
        1 + self.count / 5
    }

    /// Time left in the window, 0…1.
    pub fn remaining(&self) -> f32 {
        if self.window > 0.0 { (self.left / self.window).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// [`draw`](Combo::draw) with its ring in the theme's style (stepped
    /// blocks for a pixel identity, a glow for neon: `ui::ring_in`), as
    /// Studio's HUD draws it.
    pub fn draw_in(&self, cx: f32, cy: f32, theme: &Theme) {
        self.draw_with(cx, cy, theme, true);
    }

    /// Draws "×N" with a ring timer at (cx, cy), heating up with the count.
    pub fn draw(&self, cx: f32, cy: f32, theme: &Theme) {
        self.draw_with(cx, cy, theme, false);
    }

    fn draw_with(&self, cx: f32, cy: f32, theme: &Theme, styled: bool) {
        let r = 30.0;
        if self.count < 2 {
            if self.broke.active() {
                // The broken combo drops away.
                let k = self.broke.value();
                let s = format!("×{}", self.broke_count);
                text(&s).font(theme.number_font).size(theme.fit(theme.number_font, 24.0)).color(fade(theme.bad, k)).outline(0.08, fade(theme.outline, k)).middle().draw(cx, cy + (1.0 - k) * 30.0);
            }
            return;
        }
        let heat = ((self.count as f32 - 2.0) / 18.0).clamp(0.0, 1.0);
        let col = mix(theme.accent, theme.gold, heat);
        let s = self.punch.scale();
        gfx2d::push();
        gfx2d::translate(cx, cy);
        gfx2d::scale(s, s);
        if styled {
            crate::ui::ring_in(theme, r, self.remaining(), col, 1.0);
        } else {
            disc(0.0, 0.0, r, with_alpha(theme.outline, 0.55));
            widgets::ring_meter(0.0, 0.0, r, 5.0, self.remaining(), col, with_alpha(col, 0.25));
        }
        let label = format!("×{}", self.count);
        let size = theme.fit(theme.number_font, if self.count >= 100 { 18.0 } else { 24.0 });
        text(&label).font(theme.number_font).size(size).color(theme.text).outline(0.08, theme.outline).valign(VAlign::Middle).center().draw(0.0, 0.0);
        gfx2d::pop();
        if heat > 0.3 {
            // Hot combos shimmer.
            let t = self.left * 9.0;
            for i in 0..3 {
                let a = t + i as f32 * TAU / 3.0;
                sparkle(cx + a.cos() * (r + 8.0), cy + a.sin() * (r + 8.0), 5.0 * heat, a, with_alpha(lighten(col, 0.4), heat));
            }
        }
    }
}

/// A full-screen flash (hits, pickups, a new best): `fire` with a color.
#[derive(Clone, Copy, Debug)]
pub struct Flash {
    pub pulse: Pulse,
    pub color: u32,
    /// Additive (bright flash) or alpha (tint).
    pub additive: bool,
}

impl Flash {
    pub fn new(duration: f32) -> Flash {
        Flash { pulse: Pulse::new(duration), color: 0xffffffff, additive: true }
    }
    pub fn fire(&mut self, color: u32) {
        self.color = color;
        self.pulse.fire();
    }
    pub fn update(&mut self, dt: f32) {
        self.pulse.update(dt);
    }
    pub fn draw(&self, area: Rect) {
        let k = self.pulse.value();
        if k <= 0.0 {
            return;
        }
        if self.additive {
            gfx2d::blend(Blend::Add);
        }
        gfx2d::rect(area.x, area.y, area.w, area.h, fade(self.color, k));
        gfx2d::blend(Blend::Alpha);
    }
}

/// A vignette that pulses (damage, danger, low time): `fire` it, or hold it
/// with `level` (0…1) for a steady warning.
#[derive(Clone, Copy, Debug)]
pub struct VignettePulse {
    pub pulse: Pulse,
    pub color: u32,
    /// A steady base strength under the pulses.
    pub level: f32,
    pub size: f32,
}

impl VignettePulse {
    pub fn new(color: u32) -> VignettePulse {
        VignettePulse { pulse: Pulse::new(0.6), color, level: 0.0, size: 110.0 }
    }
    pub fn fire(&mut self) {
        self.pulse.fire();
    }
    pub fn update(&mut self, dt: f32) {
        self.pulse.update(dt);
    }
    pub fn draw(&self, area: Rect) {
        let k = (self.level + self.pulse.value() * 0.9).min(1.0);
        vignette(area, self.size, k, self.color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_are_deterministic_and_die() {
        let run = || {
            let mut p = Particles::new(42);
            p.confetti(100.0, 100.0, 50, &[0xff0000ff, 0x00ff00ff]);
            p.stars(100.0, 100.0, 8, 0xffff00ff);
            for _ in 0..30 {
                p.update(1.0 / 60.0);
            }
            p.list.iter().map(|q| (q.x.to_bits(), q.y.to_bits())).collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
        let mut p = Particles::new(1);
        p.burst(0.0, 0.0, 20, 0xffffffff);
        for _ in 0..120 {
            p.update(1.0 / 60.0);
        }
        assert!(p.is_empty());
        p.cap = 10;
        p.sparkles(0.0, 0.0, 10.0, 25, 0xffffffff);
        assert_eq!(p.len(), 10);
    }

    #[test]
    fn combos_time_out_and_multiply() {
        let mut c = Combo::new(1.0);
        for _ in 0..5 {
            c.hit();
        }
        assert_eq!(c.multiplier(), 2);
        assert_eq!(c.update(0.5), None);
        assert_eq!(c.update(0.6), Some(5));
        assert_eq!(c.count, 0);
        assert_eq!(c.best, 5);
        assert!(c.broke.active());
    }

    #[test]
    fn popups_expire() {
        let mut p = Popups::new();
        p.score(10.0, 10.0, 1500, 0xffffffff);
        assert_eq!(p.list[0].text, "+1,500");
        p.update(1.0);
        assert!(p.list.is_empty());
    }
}
