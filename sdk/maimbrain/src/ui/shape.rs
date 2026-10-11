//! Anti-aliased shapes on top of mb2d: rounded rects, shadows, glows,
//! capsules, rings, arcs, rounded polygons, nine-slice sprites.

use std::f32::consts::{PI, TAU};

use super::color::fade;
use super::layout::Rect;
use crate::gfx2d::{self, Image};

/// A filled rounded rectangle.
pub fn rrect(r: Rect, radius: f32, color: u32) {
    gfx2d::rrect(r.x, r.y, r.w, r.h, radius, 0.0, 0.0, color, color);
}

/// A rounded rectangle shaded from `top` to `bottom`.
pub fn rrect_gradient(r: Rect, radius: f32, top: u32, bottom: u32) {
    gfx2d::rrect(r.x, r.y, r.w, r.h, radius, 0.0, 0.0, top, bottom);
}

/// A rounded border `width` wide, inside `r`'s edge.
pub fn rrect_stroke(r: Rect, radius: f32, width: f32, color: u32) {
    gfx2d::rrect(r.x, r.y, r.w, r.h, radius, width, 0.0, color, color);
}

/// A soft drop shadow under a rounded rect: offset down by `dy`, blurred over `blur`.
pub fn shadow(r: Rect, radius: f32, dy: f32, blur: f32, color: u32) {
    let s = r.offset(0.0, dy);
    gfx2d::rrect(s.x, s.y, s.w, s.h, radius + blur * 0.25, 0.0, blur, color, color);
}

/// A soft glow around a rounded rect (draw it under the shape, or with `Blend::Add`).
pub fn glow(r: Rect, radius: f32, spread: f32, color: u32) {
    let g = r.inset(-spread * 0.5);
    gfx2d::rrect(g.x, g.y, g.w, g.h, radius + spread * 0.5, 0.0, spread, color, color);
}

/// A crisp anti-aliased disc.
pub fn disc(cx: f32, cy: f32, r: f32, color: u32) {
    gfx2d::rrect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, 0.0, 0.0, color, color);
}

/// A disc whose edge fades over `feather` (soft dots, light blobs).
pub fn soft_disc(cx: f32, cy: f32, r: f32, feather: f32, color: u32) {
    gfx2d::rrect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, 0.0, feather, color, color);
}

/// A circle outline `width` wide, its outer edge at radius `r`.
pub fn ring(cx: f32, cy: f32, r: f32, width: f32, color: u32) {
    gfx2d::rrect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, width, 0.0, color, color);
}

/// A line with round ends.
pub fn capsule(x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: u32) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = (dx * dx + dy * dy).sqrt();
    let w = width.abs();
    gfx2d::push();
    gfx2d::translate((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    if len > 1e-6 {
        gfx2d::rotate(dy.atan2(dx));
    }
    gfx2d::rrect(-len * 0.5 - w * 0.5, -w * 0.5, len + w, w, w * 0.5, 0.0, 0.0, color, color);
    gfx2d::pop();
}

/// Round-ended segments through `pts` ([x0, y0, x1, y1, …]). Use an opaque
/// color: translucent segments double up where they overlap at the joints.
pub fn polyline(pts: &[f32], width: f32, color: u32) {
    for w in pts.as_chunks::<2>().0.windows(2) {
        capsule(w[0][0], w[0][1], w[1][0], w[1][1], width, color);
    }
}

/// An anti-aliased filled polygon ([x0, y0, x1, y1, …], 3–512 points).
pub fn poly(pts: &[f32], color: u32) {
    gfx2d::antialias(true);
    gfx2d::poly(pts, color);
    gfx2d::antialias(false);
}

/// The outline of `pts` with each corner rounded to `radius` (`segs` points
/// per corner): soft, friendly polygons. Works for convex and concave shapes.
pub fn rounded_points(pts: &[f32], radius: f32, segs: usize) -> Vec<f32> {
    let n = pts.len() / 2;
    if n < 3 || radius <= 0.0 {
        return pts.to_vec();
    }
    let p = |i: usize| (pts[(i % n) * 2], pts[(i % n) * 2 + 1]);
    let mut out = Vec::with_capacity(n * (segs + 1) * 2);
    for i in 0..n {
        let (a, b, c) = (p(i + n - 1), p(i), p(i + 1));
        let (d1, l1) = norm(a.0 - b.0, a.1 - b.1);
        let (d2, l2) = norm(c.0 - b.0, c.1 - b.1);
        let cos = (d1.0 * d2.0 + d1.1 * d2.1).clamp(-1.0, 1.0);
        let half = cos.acos() * 0.5;
        if half < 1e-3 || (PI / 2.0 - half) < 1e-3 {
            out.extend([b.0, b.1]);
            continue;
        }
        // Tangent points on both edges, at most half of each edge away.
        let t = (radius / half.tan()).min(l1 * 0.5).min(l2 * 0.5);
        let r = t * half.tan();
        let (bis, _) = norm(d1.0 + d2.0, d1.1 + d2.1);
        let dc = r / half.sin();
        let center = (b.0 + bis.0 * dc, b.1 + bis.1 * dc);
        let t1 = (b.0 + d1.0 * t, b.1 + d1.1 * t);
        let t2 = (b.0 + d2.0 * t, b.1 + d2.1 * t);
        let a1 = (t1.1 - center.1).atan2(t1.0 - center.0);
        let mut a2 = (t2.1 - center.1).atan2(t2.0 - center.0);
        // The short way round.
        while a2 - a1 > PI {
            a2 -= TAU;
        }
        while a1 - a2 > PI {
            a2 += TAU;
        }
        for k in 0..=segs {
            let ang = a1 + (a2 - a1) * k as f32 / segs as f32;
            out.extend([center.0 + r * ang.cos(), center.1 + r * ang.sin()]);
        }
    }
    out
}

fn norm(x: f32, y: f32) -> ((f32, f32), f32) {
    let l = (x * x + y * y).sqrt();
    if l < 1e-9 { ((0.0, 0.0), 0.0) } else { ((x / l, y / l), l) }
}

/// [`rounded_points`], filled and anti-aliased.
pub fn rounded_poly(pts: &[f32], radius: f32, color: u32) {
    poly(&rounded_points(pts, radius, 5), color);
}

/// Points of a regular star: `n` tips at `r`, valleys at `r × inner`,
/// first tip straight up, turned by `rot` radians.
pub fn star_points(cx: f32, cy: f32, r: f32, inner: f32, n: usize, rot: f32) -> Vec<f32> {
    let mut v = Vec::with_capacity(n * 4);
    for i in 0..n * 2 {
        let a = rot + i as f32 * PI / n as f32;
        let rr = if i % 2 == 0 { r } else { r * inner };
        v.extend([cx + rr * a.sin(), cy - rr * a.cos()]);
    }
    v
}

/// A filled star with slightly rounded tips.
pub fn star(cx: f32, cy: f32, r: f32, rot: f32, color: u32) {
    poly(&rounded_points(&star_points(cx, cy, r, 0.48, 5, rot), r * 0.09, 3), color);
}

/// A four-pointed sparkle (a twinkle), `r` to the tips.
pub fn sparkle(cx: f32, cy: f32, r: f32, rot: f32, color: u32) {
    poly(&star_points(cx, cy, r, 0.28, 4, rot), color);
}

/// Points of an arc band (an annulus sector) from angle `a0` to `a1`
/// (radians, 0 = up, clockwise: [`arc`]'s clock face), between radii `r - width` and `r`.
pub fn arc_points(cx: f32, cy: f32, r: f32, width: f32, a0: f32, a1: f32) -> Vec<f32> {
    let segs = (((a1 - a0).abs() / TAU) * 64.0).ceil().clamp(2.0, 120.0) as usize;
    let ri = (r - width).max(0.0);
    let mut v = Vec::with_capacity((segs + 1) * 4);
    for k in 0..=segs {
        let a = a0 + (a1 - a0) * k as f32 / segs as f32;
        v.extend([cx + r * a.sin(), cy - r * a.cos()]);
    }
    for k in (0..=segs).rev() {
        let a = a0 + (a1 - a0) * k as f32 / segs as f32;
        v.extend([cx + ri * a.sin(), cy - ri * a.cos()]);
    }
    v
}

/// An arc band with round ends: progress rings, timers, gauges. Angles are
/// a clock face's, not `atan2`'s: radians from 0 = up (12 o'clock), growing
/// clockwise, so `PI / 2` is right, `PI` down and `3 * PI / 2` (or `-PI / 2`)
/// left. A smile is `arc(cx, cy, r, w, PI / 2 + 0.4, 3.0 * PI / 2 - 0.4, c)`
/// (through the bottom), a frown `arc(cx, cy, r, w, -PI / 2 + 0.4, PI / 2 - 0.4, c)`
/// (over the top). From a standard angle (0 = right, clockwise on screen, as
/// `gfx2d::rotate` and `dy.atan2(dx)` measure), add `PI / 2`.
pub fn arc(cx: f32, cy: f32, r: f32, width: f32, a0: f32, a1: f32, color: u32) {
    if (a1 - a0).abs() < 1e-4 {
        return;
    }
    if (a1 - a0).abs() >= TAU - 1e-3 {
        ring(cx, cy, r, width, color);
        return;
    }
    poly(&arc_points(cx, cy, r, width, a0, a1), color);
    let rm = r - width * 0.5;
    for a in [a0, a1] {
        disc(cx + rm * a.sin(), cy - rm * a.cos(), width * 0.5, color);
    }
}

/// A filled, anti-aliased ellipse with radii `rx` (across) and `ry` (down).
/// Turn it with `gfx2d::push`/`translate`/`rotate` around it.
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, color: u32) {
    let (rx, ry) = (rx.abs(), ry.abs());
    if (rx - ry).abs() < 1e-3 {
        disc(cx, cy, rx, color);
        return;
    }
    poly(&ellipse_points(cx, cy, rx, ry), color);
}

/// The outline of an ellipse as points, enough of them that it stays smooth
/// at its size (for [`poly`], [`polyline`] or your own shapes).
pub fn ellipse_points(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<f32> {
    // About one point per 4 units of the larger radius's circumference.
    let segs = ((TAU * rx.abs().max(ry.abs())) / 4.0).ceil().clamp(16.0, 128.0) as usize;
    let mut v = Vec::with_capacity(segs * 2);
    for k in 0..segs {
        let a = TAU * k as f32 / segs as f32;
        v.extend([cx + rx * a.cos(), cy + ry * a.sin()]);
    }
    v
}

/// A vertical gradient filling the whole of `r` (backgrounds).
pub fn gradient(r: Rect, top: u32, bottom: u32) {
    gfx2d::rect_gradient(r.x, r.y, r.w, r.h, top, bottom);
}

/// Darkened (or tinted) edges: `strength` 0…1 of `color` at the borders,
/// fading inward over `size` units.
pub fn vignette(r: Rect, size: f32, strength: f32, color: u32) {
    if strength <= 0.0 {
        return;
    }
    // A thick soft border whose outer edge lies off the rect.
    let o = r.inset(-size);
    let c = fade(color, strength);
    gfx2d::rrect(o.x, o.y, o.w, o.h, size, size * 1.6, size * 1.4, c, c);
}

/// Draws an image region stretched into `dst` without stretching its
/// corners: `border` is [left, top, right, bottom] in image pixels, drawn
/// `scale` logical units per pixel (0.5 for art stored at 2×).
pub fn nine_slice(img: Image, src: [f32; 4], border: [f32; 4], dst: Rect, scale: f32, tint: u32) {
    let [sx, sy, sw, sh] = src;
    let [l, t, r, b] = border;
    let (dl, dt, dr, db) = (l * scale, t * scale, r * scale, b * scale);
    let xs = [(sx, l, dst.x, dl), (sx + l, sw - l - r, dst.x + dl, dst.w - dl - dr), (sx + sw - r, r, dst.right() - dr, dr)];
    let ys = [(sy, t, dst.y, dt), (sy + t, sh - t - b, dst.y + dt, dst.h - dt - db), (sy + sh - b, b, dst.bottom() - db, db)];
    for &(u, uw, x, w) in &xs {
        for &(v, vh, y, h) in &ys {
            if uw > 0.0 && vh > 0.0 && w > 0.0 && h > 0.0 {
                gfx2d::sprite(img, [u, v, uw, vh], [x, y, w, h], tint);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_keeps_shapes_inside_their_corners() {
        let square = [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
        let r = rounded_points(&square, 2.0, 4);
        assert_eq!(r.len(), 4 * 5 * 2);
        for p in r.chunks(2) {
            assert!((-1e-3..=10.001).contains(&p[0]) && (-1e-3..=10.001).contains(&p[1]));
        }
        // The first corner's arc starts on the left edge 2 below the corner, ends on the top edge.
        assert!((r[0] - 0.0).abs() < 1e-4 && (r[1] - 2.0).abs() < 1e-4, "{:?}", &r[..2]);
        assert!((r[8] - 2.0).abs() < 1e-4 && r[9].abs() < 1e-4, "{:?}", &r[8..10]);
        // Huge radii are limited by the edges.
        let big = rounded_points(&square, 100.0, 2);
        assert!(big.iter().all(|v| (-1e-3..=10.001).contains(v)));
    }

    #[test]
    fn arcs_and_stars_have_the_right_point_counts() {
        assert_eq!(star_points(0.0, 0.0, 1.0, 0.5, 5, 0.0).len(), 20);
        let a = arc_points(0.0, 0.0, 10.0, 2.0, 0.0, PI);
        assert_eq!(a.len(), 2 * 2 * 33);
        assert!(a.len() / 2 <= 512);
        // 0 = up, clockwise: a quarter turn lands on the right.
        let q = arc_points(0.0, 0.0, 10.0, 2.0, 0.0, PI / 2.0);
        let n = q.len() / 4;
        assert!((q[(n - 1) * 2] - 10.0).abs() < 1e-4);
    }
}
