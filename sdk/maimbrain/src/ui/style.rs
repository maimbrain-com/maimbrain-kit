//! A theme's look beyond its colors: shape language, borders, shadows,
//! surface texture, backdrop, title / results / HUD layouts and motion
//! personality. Every kit widget and screen reads `theme.style`, so two
//! themes with different styles look different even in the same colors.
//!
//! ```ignore
//! let theme = Theme::preset("riso");                      // 17 identities (docs/UI.md)
//! let theme = Theme::from_identity("80s arcade cabinet, neon green, jittery");
//! let theme = Theme::candy().with_style(Style { shape: Shape::Wobbly, motion: Motion::Floaty, ..Style::default() });
//! ```

use std::f32::consts::TAU;

use super::color::{darken, fade, lighten, with_alpha};
use super::layout::Rect;
use super::shape::{glow, poly, polyline, rrect_gradient, rrect_stroke, shadow};
use crate::gfx2d;
use crate::motion::{Ease, noise1};

/// The outline of panels, buttons, pills and badges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shape {
    /// Rounded corners (`theme.radius`).
    #[default]
    Rounded,
    /// Square corners.
    Sharp,
    /// Fully round ends.
    Pill,
    /// Chamfered corners (sci-fi, military).
    Cut,
    /// Hand-drawn: slightly uneven edges.
    Wobbly,
    /// Stepped corners on a pixel grid.
    Pixel,
    /// Square, with light and dark edges (old desktop UI).
    Bevel,
    /// Square with punched half-circle notches (tickets, coupons).
    Ticket,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Border {
    #[default]
    None,
    /// A line this wide in `theme.panel_border`.
    Line(f32),
    /// Two thin lines with a gap.
    Double(f32),
    /// Dashes.
    Dashed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Shadow {
    #[default]
    Soft,
    None,
    /// A solid copy offset by (dx, dy) in `theme.shadow` (print, neo-brutalism).
    Hard(f32, f32),
    /// A halo in the accent color (neon).
    Glow,
}

/// Surface texture over panels (and backdrops).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Texture {
    #[default]
    None,
    /// Paper or print grain.
    Grain,
    /// CRT scanlines.
    Scanlines,
    /// Halftone dots.
    Halftone,
    /// Diagonal stripes.
    Stripes,
    /// Graph paper.
    Grid,
    /// Ruled notebook lines.
    Lines,
}

/// What `theme.backdrop()` paints behind a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Backdrop {
    /// `bg_top` → `bg_bottom`.
    #[default]
    Gradient,
    /// `bg_top` only.
    Flat,
    /// A slowly turning sunburst.
    Rays,
    /// Flat with dark edges.
    Vignette,
    /// A perspective grid floor (synthwave).
    Horizon,
}

/// How the title card is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TitleLayout {
    /// Centred letters that drop in and bob.
    #[default]
    Drop,
    /// One word per line, left-aligned and huge, with a rule.
    Stack,
    /// On a tilted band across the screen.
    Banner,
    /// Inside a rubber stamp.
    Stamp,
    /// Typed out with a cursor.
    Typewriter,
    /// On a sign ringed with chasing bulbs.
    Marquee,
    /// Letters along an arc.
    Arc,
    /// A flickering neon tube.
    Neon,
    /// An engraved plaque with rules and ornaments.
    Plate,
}

/// How the game-over card is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ResultsLayout {
    /// A panel with the heading on its edge and a ribbon for a new best.
    #[default]
    Panel,
    /// Text floating over the scene.
    Floating,
    /// A printed paper slip with dotted rows.
    Receipt,
    /// Digits in tiles on a board, a lamp for a new best.
    Scoreboard,
    /// The heading as a rubber stamp over a huge number.
    Stamp,
    /// Left-aligned, editorial: label, rule, huge number.
    Editorial,
}

/// Where the play HUD puts the score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HudLayout {
    /// Big, top centre.
    #[default]
    Center,
    /// Right-aligned in the top-right corner.
    Corner,
    /// Inside a badge in the theme's shape, top centre.
    Badge,
}

/// How things move: entrances, idles, presses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Motion {
    /// Overshoot and wobble (`BackOut`, bobbing).
    #[default]
    Bouncy,
    /// Fast and exact, no overshoot.
    Snappy,
    /// Slow, eased, drifting.
    Floaty,
    /// Stepped, ticking, no easing.
    Mechanical,
    /// Twitchy: small random jolts.
    Jittery,
    /// Springy rubber-band overshoot.
    Elastic,
}

/// Letter case for titles and headings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Case {
    #[default]
    AsWritten,
    Upper,
    Lower,
}

/// A theme's look beyond its colors (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub shape: Shape,
    pub border: Border,
    pub shadow: Shadow,
    pub texture: Texture,
    pub backdrop: Backdrop,
    /// Texture over the whole backdrop.
    pub backdrop_texture: Texture,
    pub title: TitleLayout,
    pub results: ResultsLayout,
    pub hud: HudLayout,
    pub motion: Motion,
    pub case: Case,
    /// Extra letter spacing for titles and headings, in ems.
    pub tracking: f32,
    /// Titles and big numbers get `theme.outline` around them.
    pub outlined: bool,
}

impl Default for Style {
    /// The kit's original look: rounded, soft shadows, letters that drop in.
    fn default() -> Style {
        Style {
            shape: Shape::Rounded,
            border: Border::Line(2.0),
            shadow: Shadow::Soft,
            texture: Texture::None,
            backdrop: Backdrop::Gradient,
            backdrop_texture: Texture::None,
            title: TitleLayout::Drop,
            results: ResultsLayout::Panel,
            hud: HudLayout::Center,
            motion: Motion::Bouncy,
            case: Case::AsWritten,
            tracking: 0.0,
            outlined: true,
        }
    }
}

impl Case {
    pub fn apply(self, s: &str) -> String {
        match self {
            Case::AsWritten => s.to_string(),
            Case::Upper => s.to_uppercase(),
            Case::Lower => s.to_lowercase(),
        }
    }
}

impl Motion {
    /// Seconds an entrance takes.
    pub fn duration(self) -> f32 {
        match self {
            Motion::Bouncy => 0.55,
            Motion::Snappy => 0.22,
            Motion::Floaty => 1.1,
            Motion::Mechanical => 0.4,
            Motion::Jittery => 0.3,
            Motion::Elastic => 0.9,
        }
    }

    /// Delay between staggered items (letters, rows).
    pub fn stagger(self) -> f32 {
        match self {
            Motion::Bouncy => 0.055,
            Motion::Snappy => 0.025,
            Motion::Floaty => 0.12,
            Motion::Mechanical => 0.08,
            Motion::Jittery => 0.03,
            Motion::Elastic => 0.06,
        }
    }

    /// An entrance curve: `k` 0…1 → eased value (may overshoot 1).
    pub fn enter(self, k: f32) -> f32 {
        let k = k.clamp(0.0, 1.0);
        match self {
            Motion::Bouncy => Ease::BackOut.at(k),
            Motion::Snappy => Ease::ExpoOut.at(k),
            Motion::Floaty => Ease::SineInOut.at(k),
            Motion::Mechanical => (k * 4.0).floor() / 4.0,
            Motion::Jittery => Ease::QuadOut.at(k),
            Motion::Elastic => Ease::ElasticOut.at(k),
        }
    }

    /// Entrance progress of item `i` at time `t` (0…1), staggered.
    pub fn progress(self, t: f32, i: usize) -> f32 {
        ((t - i as f32 * self.stagger()) / self.duration()).clamp(0.0, 1.0)
    }

    /// An idle offset for item `i` at time `t`: (dx, dy) in ems and a tilt in radians.
    pub fn idle(self, t: f32, i: usize) -> (f32, f32, f32) {
        let fi = i as f32;
        match self {
            Motion::Bouncy => (0.0, (t * 2.3 + fi * 0.55).sin() * 0.04, (t * 1.7 + fi * 0.8).sin() * 0.035),
            Motion::Elastic => (0.0, (t * 3.1 + fi * 0.7).sin() * 0.03, (t * 2.2 + fi).sin() * 0.05),
            Motion::Floaty => ((t * 0.7 + fi * 0.3).sin() * 0.02, (t * 0.9 + fi * 0.45).sin() * 0.07, 0.0),
            Motion::Snappy | Motion::Mechanical => (0.0, 0.0, 0.0),
            Motion::Jittery => {
                // Short jolts a few times a second, still between them.
                let step = (t * 9.0).floor();
                let on = noise1(77, step * 1.37 + fi * 0.21) > 0.55;
                if on { (noise1(3, step + fi) * 0.03, noise1(5, step - fi) * 0.03, noise1(7, step) * 0.03) } else { (0.0, 0.0, 0.0) }
            }
        }
    }

    /// Button press spring: (frequency, damping).
    pub fn spring(self) -> (f32, f32) {
        match self {
            Motion::Bouncy => (7.0, 0.55),
            Motion::Snappy => (14.0, 1.0),
            Motion::Floaty => (4.0, 0.8),
            Motion::Mechanical => (20.0, 1.0),
            Motion::Jittery => (16.0, 0.45),
            Motion::Elastic => (6.0, 0.3),
        }
    }
}

/// A 32-bit seed from a rect, so hand-drawn edges stay put while it does.
fn rect_seed(r: Rect) -> u32 {
    (r.x.to_bits() ^ r.y.to_bits().rotate_left(7) ^ r.w.to_bits().rotate_left(13) ^ r.h.to_bits().rotate_left(21)).wrapping_mul(0x9E37_79B1)
}

/// The outline of `shape` around `r` as polygon points, for the shapes that
/// aren't rounded rects (None for those: draw them with `rrect`).
pub fn outline_points(shape: Shape, r: Rect, radius: f32) -> Option<Vec<f32>> {
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    match shape {
        Shape::Rounded | Shape::Sharp | Shape::Pill | Shape::Bevel => None,
        Shape::Cut => {
            let c = radius.clamp(2.0, r.w.min(r.h) * 0.4);
            Some(vec![x0 + c, y0, x1 - c, y0, x1, y0 + c, x1, y1 - c, x1 - c, y1, x0 + c, y1, x0, y1 - c, x0, y0 + c])
        }
        Shape::Pixel => {
            let s = (radius / 3.0).clamp(2.0, r.w.min(r.h) * 0.2).round();
            Some(vec![
                x0 + 2.0 * s, y0, x1 - 2.0 * s, y0, x1 - 2.0 * s, y0 + s, x1 - s, y0 + s, x1 - s, y0 + 2.0 * s, x1, y0 + 2.0 * s,
                x1, y1 - 2.0 * s, x1 - s, y1 - 2.0 * s, x1 - s, y1 - s, x1 - 2.0 * s, y1 - s, x1 - 2.0 * s, y1,
                x0 + 2.0 * s, y1, x0 + 2.0 * s, y1 - s, x0 + s, y1 - s, x0 + s, y1 - 2.0 * s, x0, y1 - 2.0 * s,
                x0, y0 + 2.0 * s, x0 + s, y0 + 2.0 * s, x0 + s, y0 + s, x0 + 2.0 * s, y0 + s, x0 + 2.0 * s, y0,
            ])
        }
        Shape::Ticket => {
            let n = (r.h * 0.14).clamp(4.0, 18.0);
            let mut p = vec![x0, y0, x1, y0];
            let cy = r.cy();
            for i in 0..=10 {
                let a = -TAU / 4.0 + i as f32 / 10.0 * TAU / 2.0;
                p.extend([x1 - n * a.cos(), cy + n * a.sin()]);
            }
            p.extend([x1, y1, x0, y1]);
            for i in 0..=10 {
                let a = TAU / 4.0 - i as f32 / 10.0 * TAU / 2.0;
                p.extend([x0 + n * a.cos(), cy + n * a.sin()]);
            }
            Some(p)
        }
        Shape::Wobbly => {
            let seed = rect_seed(r);
            let amp = (r.w.min(r.h) * 0.03).clamp(0.8, 2.6);
            let step = 16.0;
            let mut p = Vec::new();
            let corners = [(x0, y0, x1, y0), (x1, y0, x1, y1), (x1, y1, x0, y1), (x0, y1, x0, y0)];
            for (side, (ax, ay, bx, by)) in corners.into_iter().enumerate() {
                let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                let n = (len / step).ceil().max(2.0) as usize;
                let (nx, ny) = ((by - ay) / len, -(bx - ax) / len);
                for i in 0..n {
                    let t = i as f32 / n as f32;
                    let w = noise1(seed.wrapping_add(side as u32 * 101), i as f32 * 0.9) * amp;
                    p.extend([ax + (bx - ax) * t + nx * w, ay + (by - ay) * t + ny * w]);
                }
            }
            Some(p)
        }
    }
}

/// The style for a card or panel bigger than a button: a pill identity's
/// round ends are for small things, so a card in one is rounded instead
/// (else a square panel draws as a circle).
pub fn for_cards(st: &Style) -> Style {
    let mut s = *st;
    if s.shape == Shape::Pill {
        s.shape = Shape::Rounded;
    }
    s
}

/// The corner radius a shape draws `r` with.
pub fn radius_for(shape: Shape, r: Rect, radius: f32) -> f32 {
    match shape {
        Shape::Rounded => radius.min(r.w.min(r.h) / 2.0),
        Shape::Pill => r.w.min(r.h) / 2.0,
        _ => 0.0,
    }
}

/// Fills `r` in the shape, shaded `top` → `bottom` where the shape allows.
pub fn fill(shape: Shape, r: Rect, radius: f32, top: u32, bottom: u32) {
    match outline_points(shape, r, radius) {
        Some(p) => poly(&p, super::color::mix(top, bottom, 0.5)),
        None => rrect_gradient(r, radius_for(shape, r, radius), top, bottom),
    }
}

/// Strokes the shape's edge, `width` wide, inside `r`.
pub fn stroke(shape: Shape, r: Rect, radius: f32, width: f32, color: u32) {
    match outline_points(shape, r.inset(width / 2.0), radius) {
        Some(mut p) => {
            p.extend([p[0], p[1]]);
            polyline(&p, width, color);
        }
        None => rrect_stroke(r, radius_for(shape, r, radius), width, color),
    }
}

fn dashed(r: Rect, width: f32, color: u32) {
    let dash = 9.0;
    let seg = |ax: f32, ay: f32, bx: f32, by: f32| {
        let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let n = (len / (dash * 2.0)).floor().max(1.0) as usize;
        for i in 0..n {
            let t0 = (i as f32 * 2.0 * dash) / len;
            let t1 = ((i as f32 * 2.0 + 1.0) * dash / len).min(1.0);
            gfx2d::line(ax + (bx - ax) * t0, ay + (by - ay) * t0, ax + (bx - ax) * t1, ay + (by - ay) * t1, width, color);
        }
    };
    let r = r.inset(width / 2.0);
    seg(r.x, r.y, r.right(), r.y);
    seg(r.right(), r.y, r.right(), r.bottom());
    seg(r.right(), r.bottom(), r.x, r.bottom());
    seg(r.x, r.bottom(), r.x, r.y);
}

/// A panel, button face or badge in a style: its shadow, body, texture and border.
#[allow(clippy::too_many_arguments)]
pub fn frame(st: &Style, r: Rect, radius: f32, top: u32, bottom: u32, border: u32, shadow_color: u32, glow_color: u32) {
    let rad = radius_for(st.shape, r, radius);
    match st.shadow {
        Shadow::None => {}
        Shadow::Soft => {
            if shadow_color & 0xff != 0 {
                match outline_points(st.shape, r, radius) {
                    Some(p) => {
                        let off: Vec<f32> = p.iter().enumerate().map(|(i, v)| if i % 2 == 1 { v + 6.0 } else { *v }).collect();
                        poly(&off, fade(shadow_color, 0.7));
                    }
                    None => shadow(r, rad, 10.0, 28.0, shadow_color),
                }
            }
        }
        Shadow::Hard(dx, dy) => fill(st.shape, r.offset(dx, dy), radius, shadow_color, shadow_color),
        // (Faded with the frame: the colours' own alpha scales the halo.)
        Shadow::Glow => glow(r, rad.max(4.0), 26.0, fade(with_alpha(glow_color, 0.55), (glow_color & 0xff) as f32 / 255.0)),
    }
    fill(st.shape, r, radius, top, bottom);
    if st.shape == Shape::Bevel {
        let w = 3.0;
        gfx2d::rect(r.x, r.y, r.w, w, lighten(top, 0.45));
        gfx2d::rect(r.x, r.y, w, r.h, lighten(top, 0.35));
        gfx2d::rect(r.x, r.bottom() - w, r.w, w, darken(bottom, 0.45));
        gfx2d::rect(r.right() - w, r.y, w, r.h, darken(bottom, 0.4));
    } else if matches!(st.shape, Shape::Rounded | Shape::Pill) && st.shadow == Shadow::Soft {
        // A light rim along the top inside edge reads as a lit bevel.
        gfx2d::rrect(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h * 0.5, (rad - 1.0).max(0.0), 1.5, 0.0, 0xffffff22, 0xffffff00);
    }
    texture(st.texture, r.inset(rad * 0.3 + 3.0), fade(with_alpha(darken(bottom, 0.6), 0.16), (bottom & 0xff) as f32 / 255.0), rect_seed(r));
    if border & 0xff != 0 {
        match st.border {
            Border::None => {}
            Border::Line(w) => stroke(st.shape, r, radius, w, border),
            Border::Double(w) => {
                stroke(st.shape, r, radius, w, border);
                stroke(st.shape, r.inset(w * 2.5), (radius - w * 2.5).max(0.0), (w * 0.6).max(1.0), border);
            }
            Border::Dashed(w) => dashed(r, w, border),
        }
    }
}

/// Draws a texture over `r` in `ink` (its alpha sets the strength).
pub fn texture(t: Texture, r: Rect, ink: u32, seed: u32) {
    if r.w <= 2.0 || r.h <= 2.0 || ink & 0xff == 0 {
        return;
    }
    match t {
        Texture::None => {}
        Texture::Grain => {
            let n = ((r.w * r.h) / 70.0) as u32;
            let light = with_alpha(0xffffffff, (ink & 0xff) as f32 / 255.0 * 0.8);
            for i in 0..n.min(4000) {
                let h = (i.wrapping_mul(0x9E37_79B1) ^ seed).wrapping_mul(0x85EB_CA6B);
                let (fx, fy) = ((h & 0xffff) as f32 / 65535.0, (h >> 16) as f32 / 65535.0);
                let c = if h & 0x100 != 0 { ink } else { light };
                gfx2d::rect(r.x + fx * (r.w - 1.0), r.y + fy * (r.h - 1.0), 1.0, 1.0, c);
            }
        }
        Texture::Scanlines => {
            let mut y = r.y;
            while y < r.bottom() {
                gfx2d::rect(r.x, y, r.w, 1.0, ink);
                y += 3.0;
            }
        }
        Texture::Halftone => {
            let s = 9.0;
            let (cols, rows) = ((r.w / s) as usize, (r.h / s) as usize);
            for j in 0..rows {
                for i in 0..cols {
                    let k = 1.0 - j as f32 / rows.max(1) as f32;
                    let rad = s * 0.42 * (0.25 + 0.75 * k);
                    let off = if j % 2 == 1 { s * 0.5 } else { 0.0 };
                    let (cx, cy) = (r.x + s * 0.5 + i as f32 * s + off, r.y + s * 0.5 + j as f32 * s);
                    if cx + rad < r.right() {
                        gfx2d::circle(cx, cy, rad, ink);
                    }
                }
            }
        }
        Texture::Stripes => {
            let gap = 14.0;
            let mut c = r.x - r.h;
            while c < r.right() {
                // The line from (c, bottom) up and to the right at 45°, clipped to r.
                let (mut ax, mut ay) = (c, r.bottom());
                let (mut bx, mut by) = (c + r.h, r.y);
                if ax < r.x {
                    ay -= r.x - ax;
                    ax = r.x;
                }
                if bx > r.right() {
                    by += bx - r.right();
                    bx = r.right();
                }
                if bx > ax {
                    gfx2d::line(ax, ay, bx, by, 4.0, ink);
                }
                c += gap;
            }
        }
        Texture::Grid => {
            let s = 16.0;
            let mut x = r.x + s;
            while x < r.right() {
                gfx2d::rect(x, r.y, 1.0, r.h, ink);
                x += s;
            }
            let mut y = r.y + s;
            while y < r.bottom() {
                gfx2d::rect(r.x, y, r.w, 1.0, ink);
                y += s;
            }
        }
        Texture::Lines => {
            let s = 22.0;
            let mut y = r.y + s;
            while y < r.bottom() {
                gfx2d::rect(r.x, y, r.w, 1.0, ink);
                y += s;
            }
            gfx2d::rect(r.x + 28.0, r.y, 1.5, r.h, with_alpha(0xe0505aff, (ink & 0xff) as f32 / 255.0 * 1.6));
        }
    }
}

/// Paints a backdrop over `r` at time `t` (seconds; for the moving ones).
pub fn backdrop(st: &Style, r: Rect, top: u32, bottom: u32, accent: u32, t: f32) {
    match st.backdrop {
        Backdrop::Gradient => gfx2d::rect_gradient(r.x, r.y, r.w, r.h, top, bottom),
        Backdrop::Flat => gfx2d::rect(r.x, r.y, r.w, r.h, top),
        Backdrop::Vignette => {
            gfx2d::rect(r.x, r.y, r.w, r.h, top);
            super::shape::vignette(r, r.w * 0.35, 0.55, bottom);
        }
        Backdrop::Rays => {
            gfx2d::rect(r.x, r.y, r.w, r.h, top);
            super::widgets::rays(r.cx(), r.y + r.h * 0.38, r.h, 18, t * 0.08, with_alpha(bottom, 0.5));
        }
        Backdrop::Horizon => {
            let hz = r.y + r.h * 0.58;
            gfx2d::rect_gradient(r.x, r.y, r.w, hz - r.y, top, bottom);
            gfx2d::rect(r.x, hz, r.w, r.bottom() - hz, darken(top, 0.4));
            let line = with_alpha(accent, 0.55);
            gfx2d::rect(r.x, hz - 1.0, r.w, 2.0, accent);
            // Receding lines, scrolling toward the viewer.
            for i in 0..10 {
                let k = ((i as f32 + (t * 0.6).fract()) / 10.0).powi(2);
                gfx2d::rect(r.x, hz + k * (r.bottom() - hz), r.w, 1.0 + k * 2.0, line);
            }
            for i in -8..=8 {
                let x = r.cx() + i as f32 * r.w * 0.09;
                gfx2d::line(r.cx() + i as f32 * 6.0, hz, x + (x - r.cx()) * 3.0, r.bottom(), 1.2, line);
            }
        }
    }
    texture(st.backdrop_texture, r, with_alpha(darken(bottom, 0.5), 0.12), 0x5eed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_have_sane_outlines() {
        let r = Rect::new(10.0, 20.0, 200.0, 80.0);
        for s in [Shape::Cut, Shape::Pixel, Shape::Ticket, Shape::Wobbly] {
            let p = outline_points(s, r, 16.0).unwrap();
            assert!(p.len() >= 16 && p.len() / 2 <= 512 && p.len() % 2 == 0, "{s:?}");
            for c in p.chunks(2) {
                assert!(c[0] >= r.x - 3.0 && c[0] <= r.right() + 3.0 && c[1] >= r.y - 3.0 && c[1] <= r.bottom() + 3.0, "{s:?} {c:?}");
            }
        }
        assert!(outline_points(Shape::Rounded, r, 16.0).is_none());
        // Hand-drawn edges are the same every frame for the same rect.
        assert_eq!(outline_points(Shape::Wobbly, r, 16.0), outline_points(Shape::Wobbly, r, 16.0));
    }

    #[test]
    fn motions_enter_from_zero_to_one() {
        for m in [Motion::Bouncy, Motion::Snappy, Motion::Floaty, Motion::Mechanical, Motion::Jittery, Motion::Elastic] {
            assert!(m.enter(0.0).abs() < 1e-4, "{m:?}");
            assert!((m.enter(1.0) - 1.0).abs() < 1e-3, "{m:?}");
            assert_eq!(m.progress(100.0, 3), 1.0);
            let (a, b, c) = m.idle(1.234, 2);
            assert!(a.is_finite() && b.is_finite() && c.is_finite());
        }
    }
}
