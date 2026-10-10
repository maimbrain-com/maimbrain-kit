//! How a [`super::Portrait`] shows damage: stages driven by a 0–1 health
//! (sweat, soot, cracked glass, sparks, a flickering cockpit light, static,
//! a red alarm border and a WARNING tag), and hit reactions (a shake, a
//! white then red flash, a knock that springs back, a brief pained face).
//!
//! Everything is procedural, so it works on any pixel-art portrait, and
//! drawn on a grid of whole "fx pixels" so it matches the art. Every frame is
//! a closed-form function of game time and the portrait's seed: replays draw
//! the same, at any frame rate.
//!
//! ```
//! use maimbrain::ui::{Portrait, damage};
//! use maimbrain::gfx2d::Image;
//! let pilot = Portrait::from_image(Image(1), 48, 48).expression("hurt", 1).expression("battered", 2);
//! pilot.set_health(0.2);
//! assert_eq!(damage::stage(pilot.health()), 3);
//! pilot.hit(0.8);
//! assert!(pilot.reaction(maimbrain::sys::time()).pained);
//! ```

use super::color::{rgba, with_alpha};
use super::layout::Rect;
use super::text::text;
use crate::gfx2d::{self, Blend, Font};

/// Seconds a hit is felt for (its knock, shake and flash are over by then).
const HIT_LIFE: f64 = 1.2;

/// The damage stage for `health`: 0 fine (above 0.75), 1 scuffed (sweat),
/// 2 damaged (soot, a crack, static, the light flickers; at or below 0.5),
/// 3 critical (more cracks, sparks, the alarm; at or below 0.25).
pub fn stage(health: f32) -> u8 {
    match health {
        h if h <= 0.25 => 3,
        h if h <= 0.5 => 2,
        h if h <= 0.75 => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hit {
    at: f64,
    strength: f32,
    n: u32,
}

/// A portrait's damage, shared by its clones (so a hit on the game's copy
/// shows on the dialogue's).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Condition {
    pub health: f32,
    hits: Vec<Hit>,
    count: u32,
    pub hurt: String,
    pub battered: String,
    /// Draw the overlays (sweat, soot, cracks, static, alarm).
    pub overlays: bool,
}

impl Default for Condition {
    fn default() -> Self {
        Condition { health: 1.0, hits: Vec::new(), count: 0, hurt: "hurt".into(), battered: "battered".into(), overlays: true }
    }
}

/// What a portrait's hits do to it at one moment ([`super::Portrait::reaction`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reaction {
    /// Offset, as a fraction of the portrait's width (knock + shake).
    pub dx: f32,
    pub dy: f32,
    /// White flash 0–1 (the first few hundredths of a second).
    pub white: f32,
    /// Red flash 0–1, fading over ~0.35 s.
    pub red: f32,
    /// The pained face is showing (a sheet row named "hurt", if it has one).
    pub pained: bool,
    /// Signal break-up 0–1 (static and a torn picture).
    pub glitch: f32,
}

impl Condition {
    pub fn hit(&mut self, strength: f32, at: f64) {
        self.hits.retain(|h| at - h.at < HIT_LIFE && at >= h.at);
        self.count = self.count.wrapping_add(1);
        self.hits.push(Hit { at, strength: strength.clamp(0.0, 1.0), n: self.count });
        if self.hits.len() > 6 {
            self.hits.remove(0);
        }
    }

    /// The hits' combined reaction at game time `t`. `dir` is the way the
    /// knock throws the face (−1 left, +1 right).
    pub fn reaction(&self, t: f64, seed: u32, dir: f32) -> Reaction {
        let mut r = Reaction::default();
        for h in &self.hits {
            let a = (t - h.at) as f32;
            if !(0.0..HIT_LIFE as f32).contains(&a) {
                continue;
            }
            let s = h.strength;
            // Knocked away, then an underdamped spring back to rest.
            let knock = s * 0.09 * (-a * 8.0).exp() * (a * 24.0 + 0.0).cos();
            // A shake: a new jitter every 1/40 s, dying fast.
            let amp = s * 0.035 * (-a * 12.0).exp();
            let slot = (a * 40.0) as u64;
            let k = (seed as u64) << 32 ^ (h.n as u64) << 16 ^ slot;
            r.dx += dir * knock + (hash(k) - 0.5) * 2.0 * amp;
            r.dy += (hash(k ^ 0xabcd) - 0.5) * 1.2 * amp + s * 0.02 * (-a * 10.0).exp();
            r.white = r.white.max(s.max(0.4) * (1.0 - a / 0.07).max(0.0));
            r.red = r.red.max(s * 0.75 * (1.0 - a / 0.35).max(0.0));
            r.pained |= a < 0.25 + 0.35 * s;
            r.glitch = r.glitch.max(s * (1.0 - a / 0.22).max(0.0));
        }
        r
    }
}

/// A number in [0, 1) from `x` (splitmix64).
pub(crate) fn hash(x: u64) -> f32 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn h3(seed: u32, a: u64, b: u64) -> f32 {
    hash((seed as u64) << 32 ^ a.wrapping_mul(0x1000_0193) ^ b.wrapping_mul(0x9e37_79b9))
}

/// The fx-pixel grid over a drawn portrait: cells `q` units square from `o`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grid {
    pub o: (f32, f32),
    pub q: f32,
    pub cols: i32,
    pub rows: i32,
    /// Nothing is drawn outside this ([`super::Portrait::clipped`]).
    pub clip: Option<Rect>,
}

impl Grid {
    /// About 40 fx pixels across, each a whole number of half units.
    pub fn over(r: Rect) -> Grid {
        let q = ((r.w / 40.0) * 2.0).round().max(1.0) / 2.0;
        Grid { o: (r.x, r.y), q, cols: (r.w / q).floor() as i32, rows: (r.h / q).floor() as i32, clip: None }
    }

    fn cell(&self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && x < self.cols && y < self.rows {
            self.rect(Rect::new(self.o.0 + x as f32 * self.q, self.o.1 + y as f32 * self.q, self.q, self.q), c);
        }
    }

    /// A rect, cut to the clip.
    fn rect(&self, r: Rect, c: u32) {
        let r = match self.clip {
            Some(k) => match r.intersect(&k) {
                Some(r) => r,
                None => return,
            },
            None => r,
        };
        gfx2d::rect(r.x, r.y, r.w, r.h, c);
    }

    fn fx(&self, f: f32) -> i32 {
        (f * self.cols as f32) as i32
    }
    fn fy(&self, f: f32) -> i32 {
        (f * self.rows as f32) as i32
    }
}

/// How often the picture breaks up by itself at this health (per 0.1 s slot).
fn static_chance(health: f32) -> f32 {
    match stage(health) {
        3 => 0.22,
        2 => 0.07,
        _ => 0.0,
    }
}

/// Signal break-up 0–1 at `t`: hits, plus bursts at low health.
pub(crate) fn glitch(c: &Condition, r: &Reaction, t: f64, seed: u32) -> f32 {
    let slot = (t / 0.1).floor() as i64 as u64;
    let burst = if h3(seed, 11, slot) < static_chance(c.health) { 0.5 + 0.5 * h3(seed, 12, slot) } else { 0.0 };
    r.glitch.max(burst)
}

/// Horizontal tear for band `i` of the picture at `t` (fraction of the width).
pub(crate) fn tear(g: f32, t: f64, seed: u32, i: u32) -> f32 {
    if g <= 0.0 {
        return 0.0;
    }
    let slot = (t / 0.05).floor() as i64 as u64;
    let v = h3(seed, 20 + i as u64, slot);
    if v < 0.45 { 0.0 } else { (h3(seed, 40 + i as u64, slot) - 0.5) * 0.16 * g }
}

/// Draws the damage overlays over a portrait drawn at `r` (its fx grid `g`).
pub(crate) fn draw_overlays(c: &Condition, re: &Reaction, g: &Grid, r: Rect, t: f64, seed: u32) {
    let h = c.health.clamp(0.0, 1.0);
    let st = stage(h);
    let wear = 1.0 - h;
    if c.overlays {
        if wear > 0.4 {
            soot(g, seed, ((wear - 0.4) * 10.0).ceil() as u32);
        }
        if st >= 1 {
            sweat(g, seed, 1 + st as u32, t);
        }
        let cracks = ((wear * 4.0 - 1.0).floor() as i32).clamp(0, 3) as u32;
        for i in 0..cracks {
            crack(g, seed, i);
        }
        if st >= 3 {
            sparks(g, seed, t);
        }
        if st >= 2 {
            // Scanlines.
            let a = if st >= 3 { 0.14 } else { 0.09 };
            for y in (0..g.rows).step_by(2) {
                g.rect(Rect::new(r.x, g.o.1 + y as f32 * g.q, g.cols as f32 * g.q, g.q * 0.5), with_alpha(0x000000ff, a));
            }
        }
    }
    // Static from hits and low-health bursts.
    let gl = glitch(c, re, t, seed);
    if gl > 0.0 {
        let slot = (t / 0.05).floor() as i64 as u64;
        for y in 0..g.rows {
            if h3(seed, 50 + y as u64, slot) > 0.35 + 0.45 * (1.0 - gl) {
                continue;
            }
            for x in 0..g.cols {
                let v = h3(seed, (y as u64) << 16 | x as u64, slot);
                if v < 0.22 * gl {
                    let lum = (140.0 + 115.0 * h3(seed, 7 + x as u64, slot ^ y as u64)) as u8;
                    g.cell(x, y, rgba(lum, lum, lum, 150));
                }
            }
        }
    }
    if c.overlays && st >= 3 {
        alarm(g, r, t);
    }
}

/// Dark, uneven smudges on the face and shoulders, in fixed places.
fn soot(g: &Grid, seed: u32, n: u32) {
    for i in 0..n.min(6) as u64 {
        let cx = g.fx(0.22 + 0.56 * h3(seed, 100 + i, 0));
        let cy = g.fy(0.42 + 0.48 * h3(seed, 100 + i, 1));
        let rad = 1.5 + 1.5 * h3(seed, 100 + i, 2);
        let ri = rad.ceil() as i32;
        for dy in -ri..=ri {
            for dx in -ri - 1..=ri + 1 {
                let d = ((dx as f32 / 1.4).powi(2) + (dy as f32).powi(2)).sqrt();
                let keep = h3(seed, 200 + i, ((dx + 8) * 32 + dy + 8) as u64);
                if d <= rad && keep > d / rad * 0.7 {
                    g.cell(cx + dx, cy + dy, rgba(24, 18, 14, (90.0 + 70.0 * (1.0 - d / rad)) as u8));
                }
            }
        }
    }
}

/// Beads of sweat running down the brow, each on its own loop.
fn sweat(g: &Grid, seed: u32, n: u32, t: f64) {
    for i in 0..n as u64 {
        let x = g.fx(0.32 + 0.36 * h3(seed, 300 + i, 0));
        let y0 = g.fy(0.3 + 0.1 * h3(seed, 300 + i, 1));
        let period = 1.4 + 0.8 * h3(seed, 300 + i, 2) as f64;
        let k = ((t / period + h3(seed, 300 + i, 3) as f64) % 1.0) as f32;
        let fall = (k * k * 7.0) as i32;
        let a = (k / 0.1).min((1.0 - k) / 0.15).min(1.0);
        let y = y0 + fall;
        g.cell(x, y, with_alpha(0xffffffff, 0.95 * a));
        g.cell(x, y + 1, with_alpha(0x6cc4ffff, 0.95 * a));
        g.cell(x, y + 2, with_alpha(0x6cc4ffff, 0.9 * a));
        g.cell(x + 1, y + 1, with_alpha(0x2f7fc0ff, 0.85 * a));
        g.cell(x + 1, y + 2, with_alpha(0x2f7fc0ff, 0.85 * a));
    }
}

/// A crack across the glass: rays from an impact point, a pixel wide, with a shadow.
fn crack(g: &Grid, seed: u32, i: u32) {
    let i = i as u64;
    let ix = g.fx(0.2 + 0.6 * h3(seed, 400 + i, 0));
    let iy = g.fy(0.15 + 0.6 * h3(seed, 400 + i, 1));
    let rays = 3 + (h3(seed, 400 + i, 2) * 3.0) as u64;
    let mut cells = Vec::new();
    for ray in 0..rays {
        let mut ang = (ray as f32 + h3(seed, 500 + i, ray) * 0.8) / rays as f32 * std::f32::consts::TAU;
        let (mut x, mut y) = (ix as f32 + 0.5, iy as f32 + 0.5);
        let segs = 2 + (h3(seed, 600 + i, ray) * 2.0) as u64;
        for s in 0..segs {
            let len = 3.0 + 5.0 * h3(seed, 700 + i * 8 + ray, s);
            let steps = len.ceil() as i32;
            for _ in 0..steps {
                x += ang.cos();
                y += ang.sin();
                cells.push((x.floor() as i32, y.floor() as i32));
            }
            ang += (h3(seed, 800 + i * 8 + ray, s) - 0.5) * 1.2;
        }
    }
    for &(x, y) in &cells {
        g.cell(x + 1, y + 1, rgba(0, 0, 0, 80));
    }
    for &(x, y) in &cells {
        g.cell(x, y, rgba(235, 245, 255, 190));
    }
    g.cell(ix, iy, rgba(255, 255, 255, 230));
}

/// Bursts of sparks from the frame's edge, now and then.
fn sparks(g: &Grid, seed: u32, t: f64) {
    const SLOT: f64 = 0.7;
    const LIFE: f32 = 0.4;
    let slot = (t / SLOT).floor();
    let age = (t - slot * SLOT) as f32;
    let k = slot as i64 as u64;
    if age > LIFE || h3(seed, 900, k) > 0.65 {
        return;
    }
    let side = h3(seed, 901, k) < 0.5;
    let ox = if side { 1.0 } else { g.cols as f32 - 2.0 };
    let oy = g.rows as f32 * (0.2 + 0.5 * h3(seed, 902, k));
    for p in 0..7u64 {
        let vx = (if side { 1.0 } else { -1.0 }) * (6.0 + 22.0 * h3(seed, 903 + p, k));
        let vy = -14.0 + 20.0 * h3(seed, 920 + p, k);
        let x = ox + vx * age;
        let y = oy + vy * age + 40.0 * age * age;
        let life = 1.0 - age / LIFE;
        let col = if life > 0.6 { rgba(255, 250, 210, 255) } else { rgba(255, 170, 40, (255.0 * life / 0.6) as u8) };
        g.cell(x as i32, y as i32, col);
    }
}

/// A red border pulsing at 2 Hz, and a blinking WARNING tag. A clipped
/// portrait's border is its frame, which holds still while the face shakes.
fn alarm(g: &Grid, r: Rect, t: f64) {
    let p = 0.5 + 0.5 * (t as f32 * std::f32::consts::TAU * 2.0).sin();
    let c = with_alpha(0xff2a2aff, 0.45 + 0.55 * p);
    let w = (g.q * 2.0).max(2.0);
    let (x, y, rw, rh) = match g.clip {
        Some(k) => (k.x, k.y, k.w, k.h),
        None => (r.x, r.y, g.cols as f32 * g.q, g.rows as f32 * g.q),
    };
    gfx2d::rect(x, y, rw, w, c);
    gfx2d::rect(x, y + rh - w, rw, w, c);
    gfx2d::rect(x, y, w, rh, c);
    gfx2d::rect(x + rw - w, y, w, rh, c);
    if (t * 2.5).fract() < 0.62 {
        let size = if rw >= 120.0 { 16.0 } else { 8.0 };
        let label = text("WARNING").font(Font::Pixel).size(size);
        let (tw, _) = label.measure();
        let tw = if tw > 0.0 { tw } else { size * 7.0 };
        let (bx, by) = ((x + rw / 2.0 - tw / 2.0 - 4.0).round(), (y + w + 4.0).round());
        gfx2d::rect(bx, by, tw + 8.0, size + 6.0, rgba(200, 16, 24, 235));
        label.color(0xffffffff).draw(bx + 4.0, by + 3.0);
    }
}

/// The cockpit light on the face at `t`: how much red it adds (pulsing with
/// the alarm) and how dark it goes when the light cuts out for a moment.
pub(crate) fn light(c: &Condition, t: f64, seed: u32) -> (f32, f32) {
    let st = stage(c.health);
    if !c.overlays || st < 2 {
        return (0.0, 0.0);
    }
    let pulse = if st >= 3 { 0.5 + 0.5 * (t as f32 * std::f32::consts::TAU * 2.0).sin() } else { 0.4 };
    let slot = (t / 0.08).floor() as i64 as u64;
    let off = if st >= 3 { 0.12 } else { 0.05 };
    let dark = if h3(seed, 30, slot) < off { 0.45 } else { 0.0 };
    (0.08 + 0.14 * pulse * (st as f32 - 1.0), dark)
}

/// Draws the light and the flashes over the sprite just drawn: the sprite
/// again, added (or darkened), so only the face lights up, not its background.
pub(crate) fn flash(re: &Reaction, light: (f32, f32), draw: impl Fn(u32)) {
    if light.1 > 0.0 {
        draw(with_alpha(0x000000ff, light.1));
    }
    if re.white <= 0.0 && re.red <= 0.0 && light.0 <= 0.0 {
        return;
    }
    gfx2d::blend(Blend::Add);
    if light.0 > 0.0 {
        draw(with_alpha(0xff3018ff, light.0));
    }
    if re.white > 0.0 {
        draw(with_alpha(0xffffffff, re.white * 0.7));
    }
    if re.red > 0.0 {
        draw(with_alpha(0xff2020ff, re.red * 0.8));
    }
    gfx2d::blend(Blend::Alpha);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_follow_health() {
        assert_eq!([1.0, 0.8, 0.75, 0.6, 0.5, 0.3, 0.25, 0.0].map(stage), [0, 0, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn a_hit_knocks_flashes_and_settles() {
        let mut c = Condition::default();
        c.hit(1.0, 10.0);
        let at = |t: f64| c.reaction(10.0 + t, 7, -1.0);
        assert!(at(0.0).white > 0.9 && at(0.0).red > 0.7 && at(0.0).pained);
        assert!(at(0.1).white == 0.0 && at(0.1).red > 0.0, "white first, then red");
        assert!(!at(0.7).pained && at(0.4).red == 0.0);
        let early = (0..10).map(|i| at(i as f64 / 120.0).dx.abs()).fold(0.0f32, f32::max);
        assert!(early > 0.03, "knocked: {early}");
        assert!(at(0.6).dx.abs() < 0.01 && at(1.3) == Reaction::default(), "springs back to rest");
        // Closed form: the same moment gives the same pose whatever the frame rate, and replays.
        assert_eq!(at(0.05), at(0.05));
        assert_ne!(c.reaction(10.05, 8, -1.0).dy, at(0.05).dy, "another face shakes its own way");
        // A light hit does less.
        let mut l = Condition::default();
        l.hit(0.2, 10.0);
        assert!(l.reaction(10.02, 7, -1.0).red < at(0.02).red);
        assert!(!l.reaction(10.4, 7, -1.0).pained);
    }

    #[test]
    fn old_hits_are_forgotten() {
        let mut c = Condition::default();
        for i in 0..20 {
            c.hit(0.5, i as f64 * 0.1);
        }
        assert!(c.hits.len() <= 6);
        c.hit(0.5, 100.0);
        assert_eq!(c.hits.len(), 1);
    }

    #[test]
    fn static_bursts_come_at_low_health_only() {
        let mut c = Condition::default();
        let quiet = Reaction::default();
        let bursts = |c: &Condition| (0..600).filter(|i| glitch(c, &quiet, *i as f64 / 60.0, 3) > 0.0).count();
        assert_eq!(bursts(&c), 0);
        c.health = 0.4;
        let some = bursts(&c);
        c.health = 0.1;
        let more = bursts(&c);
        assert!(some > 0 && more > some, "{some} {more}");
    }

    #[test]
    fn the_grid_is_whole_half_units() {
        let g = Grid::over(Rect::new(10.0, 20.0, 136.0, 136.0));
        assert_eq!(g.q * 2.0, (g.q * 2.0).round());
        assert!((38..=42).contains(&g.cols));
    }
}
