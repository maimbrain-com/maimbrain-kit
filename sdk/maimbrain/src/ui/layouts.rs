//! The title, results and HUD layouts behind `Style::title`, `results` and
//! `hud` (ui/style.rs). Screens (ui/screens.rs) call these; each is a pure
//! function of its inputs and the time since the screen appeared, so they
//! replay exactly.

use super::color::{darken, fade, lighten, mix, with_alpha};
use super::icon::Icon;
use super::layout::{Layout, Rect};
use super::shape::{disc, poly, soft_disc};
use super::style::{self, Shadow, Texture};
use super::text::{Align, Text, VAlign, fmt_int, metrics, text};
use super::theme::Theme;
use super::widgets::pill_in;
use crate::gfx2d::{self, Font};
use crate::motion::noise1;

/// Display text in a theme's title style: its tracking, outline (if the
/// style outlines titles) and shadow treatment.
pub fn styled<'a>(th: &Theme, s: &'a str, font: Font, size: f32, color: u32) -> Text<'a> {
    let mut t = text(s).font(font).size(size).color(color).tracking(th.style.tracking);
    if th.style.outlined {
        t = t.outline(th.outline_em, th.outline);
    }
    match th.style.shadow {
        Shadow::Soft => t.soft_shadow(0.0, size * 0.07, 0.06, th.shadow),
        Shadow::Hard(dx, dy) => t.shadow((dx * 0.6).clamp(-4.0, 4.0), (dy * 0.6).clamp(-4.0, 4.0), th.shadow),
        Shadow::Glow => t.glow(0.1, with_alpha(th.accent, 0.8)),
        Shadow::None => t,
    }
}

/// A size at which `s` fits `max_w` (never bigger than `size`).
fn fit_width(th: &Theme, s: &str, font: Font, size: f32, max_w: f32) -> f32 {
    let w = text(s).font(font).size(size).tracking(th.style.tracking).measure().0;
    let size = if w > max_w && w > 0.0 { size * max_w / w } else { size };
    th.fit(font, size)
}

/// Everything a title layout needs.
pub struct TitleArgs<'a> {
    pub title: &'a str,
    pub tagline: Option<&'a str>,
    pub best: Option<(&'a str, i64)>,
    pub size: f32,
    /// The title's centre line (from the top for left-aligned layouts).
    pub y: f32,
    pub color: u32,
    pub t: f32,
}

/// Draws a title card in `th.style.title`'s layout.
pub fn title(th: &Theme, l: &Layout, a: &TitleArgs) {
    use super::style::TitleLayout as T;
    let s = th.case(a.title);
    let below = match th.style.title {
        T::Drop => {
            let r = super::screens::bouncy_title(&s, l.screen.cx(), a.y, a.size, a.t, th, a.color, l.safe.w - 32.0);
            if matches!(th.style.motion, super::style::Motion::Bouncy | super::style::Motion::Elastic) {
                super::screens::twinkles(r.inset(-8.0), a.t, lighten(th.gold, 0.4));
            }
            Some((l.screen.cx(), r.bottom() + a.size * 0.42, Align::Center))
        }
        T::Stack => stack(th, l, a, &s),
        T::Banner => banner(th, l, a, &s),
        T::Stamp => stamp_title(th, l, a, &s),
        T::Typewriter => {
            typewriter(th, l, a, &s);
            None
        }
        T::Marquee => marquee(th, l, a, &s),
        T::Arc => arc(th, l, a, &s),
        T::Neon => neon(th, l, a, &s),
        T::Plate => plate(th, l, a, &s),
    };
    if let Some((x, y, align)) = below {
        tagline_and_best(th, l, a, x, y, align);
    }
}

/// The tagline and best pill, under the title, fading in after it.
fn tagline_and_best(th: &Theme, l: &Layout, a: &TitleArgs, x: f32, mut y: f32, align: Align) {
    let m = th.style.motion;
    if let Some(tag) = a.tagline {
        let k = m.enter(((a.t - 0.45) / m.duration().max(0.3)).clamp(0.0, 1.0)).clamp(0.0, 1.0);
        let size = th.fit(th.body_font, 18.0);
        let mut t = text(tag).font(th.body_font).size(size).color(th.text).alpha(k).align(align).wrap(l.card.w);
        if th.style.outlined || th.style.backdrop_texture != Texture::None {
            t = t.outline(0.05, with_alpha(th.outline, 0.6));
        }
        if th.style.shadow == Shadow::Soft {
            t = t.soft_shadow(0.0, 2.0, 0.08, th.shadow);
        }
        t.draw(x, y + (1.0 - k) * 8.0);
        y += size * 1.6 * t.lines().len().max(1) as f32;
    }
    if let Some((label, b)) = a.best {
        let k = ((a.t - 0.7) / 0.5).clamp(0.0, 1.0);
        if k > 0.0 {
            let s = m.enter(k);
            let label = format!("{} {}", th.case(label), fmt_int(b));
            let w = super::widgets::pill_width(&label, 34.0, th.body_font, true);
            let px = match align {
                Align::Left => x + w / 2.0,
                Align::Center => x,
                Align::Right => x - w / 2.0,
            };
            gfx2d::push();
            gfx2d::translate(px, y + 18.0);
            gfx2d::scale(s, s);
            pill_in(th, &label, 0.0, 0.0, 34.0, with_alpha(th.outline, 0.55), th.gold, Some(Icon::TROPHY));
            gfx2d::pop();
        }
    }
}

fn stack(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let words: Vec<&str> = s.split_whitespace().collect();
    let x = l.card.x + 6.0;
    let max_w = l.card.w - 12.0;
    let widest = words.iter().copied().max_by(|p, q| {
        let w = |w: &str| text(w).font(th.title_font).size(100.0).tracking(th.style.tracking).measure().0;
        w(p).total_cmp(&w(q))
    })?;
    let size = fit_width(th, widest, th.title_font, a.size * 1.3, max_w);
    let mt = metrics(th.title_font);
    let line = mt.cap * size * 1.12;
    let top = a.y - a.size * 0.55;
    for (i, w) in words.iter().enumerate() {
        let k = m.progress(a.t, i);
        if k <= 0.0 {
            continue;
        }
        let e = m.enter(k);
        let (dx, dy, _) = m.idle(a.t, i);
        styled(th, w, th.title_font, size, a.color)
            .alpha((k * 3.0).min(1.0))
            .valign(VAlign::Baseline)
            .draw(x - (1.0 - e) * 60.0 + dx * size, top + mt.cap * size + i as f32 * line + dy * size);
    }
    let rule_y = top + mt.cap * size + (words.len() as f32 - 1.0) * line + size * 0.22;
    let k = m.enter(m.progress(a.t, words.len()));
    gfx2d::rect(x, rule_y, max_w * 0.42 * k.clamp(0.0, 1.2), (size * 0.07).max(4.0), th.accent);
    Some((x, rule_y + size * 0.3, Align::Left))
}

fn banner(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let size = fit_width(th, s, th.title_font, a.size, l.safe.w - 56.0);
    let mt = metrics(th.title_font);
    let h = mt.cap * size + size * 0.55;
    let cx = l.screen.cx();
    let k = m.enter(m.progress(a.t, 0));
    let angle = -0.07;
    gfx2d::push();
    gfx2d::translate(cx, a.y);
    gfx2d::rotate(angle);
    let w = l.screen.w + 80.0;
    gfx2d::scale(k.max(0.0), 1.0);
    if let Shadow::Hard(dx, dy) = th.style.shadow {
        gfx2d::rect(-w / 2.0 + dx, -h / 2.0 + dy, w, h, th.shadow);
    }
    gfx2d::rect(-w / 2.0, -h / 2.0, w, h, th.accent);
    gfx2d::rect(-w / 2.0, -h / 2.0 + 4.0, w, 2.0, with_alpha(th.on_accent, 0.35));
    gfx2d::rect(-w / 2.0, h / 2.0 - 6.0, w, 2.0, with_alpha(th.on_accent, 0.35));
    gfx2d::pop();
    let tk = m.progress(a.t - 0.12, 0);
    if tk > 0.0 {
        let (dx, dy, tilt) = m.idle(a.t, 0);
        gfx2d::push();
        gfx2d::translate(cx + dx * size, a.y + dy * size);
        gfx2d::rotate(angle + tilt * 0.5);
        let e = m.enter(tk);
        gfx2d::scale(e, e);
        text(s).font(th.title_font).size(size).color(th.on_accent).tracking(th.style.tracking).middle().draw(0.0, 0.0);
        gfx2d::pop();
    }
    Some((cx, a.y + h * 0.5 + 26.0, Align::Center))
}

/// A rubber stamp: text in a double-ruled box, turned, thumped down at `k` (entrance 0…1).
pub fn stamp(th: &Theme, s: &str, font: Font, cx: f32, cy: f32, size: f32, angle: f32, color: u32, e: f32) -> Rect {
    let tw = text(s).font(font).size(size).tracking(th.style.tracking).measure().0;
    let mt = metrics(font);
    let (w, h) = (tw + size * 0.7, mt.cap * size + size * 0.62);
    let sc = 1.0 + 1.4 * (1.0 - e.clamp(0.0, 1.0));
    let alpha = e.clamp(0.0, 1.0);
    gfx2d::push();
    gfx2d::translate(cx, cy);
    gfx2d::rotate(angle);
    gfx2d::scale(sc, sc);
    let r = Rect::centered(0.0, 0.0, w, h);
    let c = fade(color, alpha);
    let lw = (size * 0.07).max(2.5);
    gfx2d::rrect(r.x, r.y, r.w, r.h, size * 0.08, lw, 0.0, c, c);
    let inner = r.inset(lw * 1.8);
    gfx2d::rrect(inner.x, inner.y, inner.w, inner.h, size * 0.05, (lw * 0.45).max(1.2), 0.0, c, c);
    text(s).font(font).size(size).color(c).tracking(th.style.tracking).middle().draw(0.0, 0.0);
    // Worn ink: specks of the backdrop color across it.
    style::texture(Texture::Grain, r, with_alpha(th.bg_top, 0.5 * alpha), 0x57a3);
    gfx2d::pop();
    Rect::centered(cx, cy, w, h)
}

fn stamp_title(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let size = fit_width(th, s, th.title_font, a.size, l.safe.w - 90.0);
    let e = m.enter(m.progress(a.t - 0.1, 0));
    let col = if super::color::luma(th.bg_top) > 0.5 { th.accent } else { lighten(th.accent, 0.1) };
    let r = stamp(th, s, th.title_font, l.screen.cx(), a.y, size, -0.09, col, e);
    Some((l.screen.cx(), r.bottom() + 22.0, Align::Center))
}

fn typewriter(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) {
    let x = l.card.x + 6.0;
    let size = fit_width(th, s, th.title_font, a.size * 0.9, l.card.w - 30.0);
    let mt = metrics(th.title_font);
    let cps = 14.0;
    let n = ((a.t - 0.15) * cps).max(0.0) as usize;
    let shown: String = s.chars().take(n).collect();
    let top = a.y - a.size * 0.4;
    let r = styled(th, &shown, th.title_font, size, a.color).draw(x, top);
    let title_done = n >= s.chars().count();
    let blink = (a.t * 2.2).fract() < 0.55 || !title_done;
    let mut y = top + mt.line * size + 8.0;
    let body = th.fit(th.body_font, 17.0);
    let bm = metrics(th.body_font);
    let mut cursor = (r.right() + size * 0.08, top + (mt.ascent - mt.cap) * size, size * 0.5, mt.cap * size);
    if title_done {
        let mut t0 = 0.15 + s.chars().count() as f32 / cps + 0.25;
        let mut lines: Vec<String> = Vec::new();
        if let Some(tag) = a.tagline {
            lines.push(format!("> {tag}"));
        }
        if let Some((label, b)) = a.best {
            lines.push(format!("> {} {}", label.to_lowercase(), fmt_int(b)));
        }
        for line in lines {
            let k = ((a.t - t0) * cps * 1.8).max(0.0) as usize;
            if k == 0 {
                break;
            }
            let part: String = line.chars().take(k).collect();
            let rr = text(&part).font(th.body_font).size(body).color(th.text_dim).draw(x, y);
            cursor = (rr.right() + 3.0, y + (bm.ascent - bm.cap) * body, body * 0.5, bm.cap * body);
            t0 += line.chars().count() as f32 / (cps * 1.8) + 0.3;
            y += bm.line * body * 1.3;
        }
    }
    if blink {
        gfx2d::rect(cursor.0, cursor.1, cursor.2, cursor.3, th.accent);
    }
}

fn marquee(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let size = fit_width(th, s, th.title_font, a.size * 0.9, l.safe.w - 90.0);
    let mt = metrics(th.title_font);
    let tw = text(s).font(th.title_font).size(size).tracking(th.style.tracking).measure().0;
    let (w, h) = ((tw + size * 1.2).min(l.screen.w - 20.0), mt.cap * size + size * 1.25);
    let e = m.enter(m.progress(a.t, 0)).max(0.0);
    let board = Rect::centered(l.screen.cx(), a.y, w, h);
    gfx2d::push();
    gfx2d::translate(board.cx(), board.cy());
    gfx2d::scale(1.0, e);
    gfx2d::translate(-board.cx(), -board.cy());
    style::frame(&th.style, board, th.radius, th.panel_top, th.panel_bottom, th.panel_border, th.shadow, th.accent);
    // Bulbs around the board, chasing.
    let inner = board.inset(10.0);
    let per = 2.0 * (inner.w + inner.h);
    let n = (per / 19.0) as usize;
    let phase = (a.t * 7.0) as usize;
    for i in 0..n {
        let d = i as f32 / n as f32 * per;
        let (bx, by) = if d < inner.w {
            (inner.x + d, inner.y)
        } else if d < inner.w + inner.h {
            (inner.right(), inner.y + d - inner.w)
        } else if d < 2.0 * inner.w + inner.h {
            (inner.right() - (d - inner.w - inner.h), inner.bottom())
        } else {
            (inner.x, inner.bottom() - (d - 2.0 * inner.w - inner.h))
        };
        let on = (i + phase) % 3 != 0 && a.t > 0.3;
        if on {
            soft_disc(bx, by, 6.0, 5.0, with_alpha(th.gold, 0.45));
        }
        disc(bx, by, 3.2, if on { lighten(th.gold, 0.35) } else { darken(th.gold, 0.55) });
    }
    gfx2d::pop();
    if a.t > 0.2 {
        let (dx, dy, _) = m.idle(a.t, 0);
        styled(th, s, th.title_font, size, a.color).middle().draw(board.cx() + dx * size, board.cy() + dy * size);
    }
    Some((l.screen.cx(), board.bottom() + 20.0, Align::Center))
}

fn arc(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let size = fit_width(th, s, th.title_font, a.size, l.safe.w - 50.0);
    let mut buf = [0u8; 4];
    let ws: Vec<f32> = s.chars().map(|c| gfx2d::measure(th.title_font, size, c.encode_utf8(&mut buf)) + th.style.tracking * size).collect();
    let tw: f32 = ws.iter().sum();
    let radius = (tw * 1.05).max(240.0);
    let (cx, cy) = (l.screen.cx(), a.y + radius);
    let mut at = -tw / 2.0;
    for (i, c) in s.chars().enumerate() {
        let w = ws[i];
        let ang = (at + w / 2.0) / radius;
        at += w;
        let k = m.progress(a.t, i);
        if k <= 0.0 || c == ' ' {
            continue;
        }
        let e = m.enter(k);
        let (dx, dy, tilt) = m.idle(a.t, i);
        let (x, y) = (cx + radius * ang.sin(), cy - radius * ang.cos());
        gfx2d::push();
        gfx2d::translate(x + dx * size, y - (1.0 - e) * size * 0.8 + dy * size);
        gfx2d::rotate(ang + tilt * 0.5);
        let sc = 0.5 + 0.5 * e;
        gfx2d::scale(sc, sc);
        styled(th, c.encode_utf8(&mut buf), th.title_font, size, a.color).tracking(0.0).alpha((k * 3.0).min(1.0)).middle().draw(0.0, 0.0);
        gfx2d::pop();
    }
    let drop = radius * (1.0 - (tw / 2.0 / radius).cos());
    Some((cx, a.y + metrics(th.title_font).cap * size * 0.6 + drop * 0.4 + size * 0.35, Align::Center))
}

fn neon(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let size = fit_width(th, s, th.title_font, a.size, l.safe.w - 40.0);
    // Flickers on, then hums.
    let on = if a.t < 1.3 { noise1(41, a.t * 14.0) > -0.1 + (1.3 - a.t) * 0.6 } else { true };
    let hum = 0.92 + 0.08 * (a.t * 9.0).sin().abs();
    let k = if on { hum } else { 0.12 };
    let tube = lighten(th.accent, 0.55);
    let cx = l.screen.cx();
    let t = text(s).font(th.title_font).size(size).tracking(th.style.tracking).middle();
    t.color(fade(th.accent, 0.9 * k)).glow(0.12, fade(th.accent, k)).draw(cx, a.y);
    t.color(fade(tube, k)).draw(cx, a.y);
    Some((cx, a.y + metrics(th.title_font).cap * size * 0.5 + 26.0, Align::Center))
}

fn diamond(cx: f32, cy: f32, r: f32, color: u32) {
    poly(&[cx, cy - r, cx + r, cy, cx, cy + r, cx - r, cy], color);
}

fn plate(th: &Theme, l: &Layout, a: &TitleArgs, s: &str) -> Option<(f32, f32, Align)> {
    let m = th.style.motion;
    let size = fit_width(th, s, th.title_font, a.size * 0.85, l.card.w - 48.0);
    let mt = metrics(th.title_font);
    let e = m.enter(m.progress(a.t, 0)).clamp(0.0, 1.2);
    let h = mt.cap * size + size * 1.1;
    let r = Rect::new(l.card.x, a.y - h / 2.0 + (1.0 - e.min(1.0)) * 16.0, l.card.w, h);
    let al = e.clamp(0.0, 1.0);
    style::frame(&th.style, r, th.radius, fade(th.panel_top, al), fade(th.panel_bottom, al), fade(th.panel_border, al), fade(th.shadow, al), th.accent);
    let orn = fade(th.panel_border, al);
    for (x, y) in [(r.x, r.y), (r.right(), r.y), (r.x, r.bottom()), (r.right(), r.bottom())] {
        diamond(x, y, 6.0, orn);
    }
    styled(th, s, th.title_font, size, th.text).alpha(al).middle().draw(r.cx(), r.cy());
    // A rule with a diamond under the plaque.
    let y = r.bottom() + 16.0;
    let half = r.w * 0.3 * al;
    gfx2d::rect(r.cx() - half, y, half * 2.0, 1.5, orn);
    diamond(r.cx(), y + 0.75, 4.5, orn);
    Some((r.cx(), y + 14.0, Align::Center))
}

// --- Results.

/// What a results layout draws from.
pub struct ResultsArgs<'a> {
    pub heading: &'a str,
    pub label: &'a str,
    /// The rolling value and its punch scale.
    pub shown: i64,
    pub punch: f32,
    pub best: i64,
    pub new_best: bool,
    pub landed: bool,
    /// The new-best stamp's spring (0 → 1).
    pub stamp: f32,
    pub t: f32,
    pub area: Rect,
}

/// The block a results layout occupies (its buttons go under it).
pub fn results_rect(th: &Theme, area: Rect) -> Rect {
    use super::style::ResultsLayout as R;
    match th.style.results {
        R::Panel => {
            let h = 236.0;
            Rect::new(area.x, (area.y + 48.0).max(area.cy() - h / 2.0 - 40.0), area.w, h)
        }
        R::Floating => Rect::new(area.x, area.y + 24.0, area.w, 200.0),
        R::Receipt => {
            let w = area.w.min(250.0);
            Rect::new(area.cx() - w / 2.0, area.y + 20.0, w, 250.0)
        }
        R::Scoreboard => Rect::new(area.x, area.y + 44.0, area.w, 220.0),
        R::Stamp => Rect::new(area.x, area.y + 30.0, area.w, 230.0),
        R::Editorial => Rect::new(area.x, area.y + 24.0, area.w, 250.0),
    }
}

/// Where a results layout puts its round retry button.
pub fn retry_center(th: &Theme, p: Rect) -> (f32, f32) {
    use super::style::ResultsLayout as R;
    match th.style.results {
        R::Panel => (p.cx(), p.bottom()),
        R::Floating => (p.cx(), p.bottom() + 20.0),
        R::Editorial => (p.x + 40.0, p.bottom() + 42.0),
        _ => (p.cx(), p.bottom() + 46.0),
    }
}

/// Draws the receipt, scoreboard, stamp and editorial layouts (the panel
/// and floating ones are drawn by `ResultsCard` itself).
pub fn results(th: &Theme, a: &ResultsArgs) {
    use super::style::ResultsLayout as R;
    match th.style.results {
        R::Receipt => receipt(th, a),
        R::Scoreboard => scoreboard(th, a),
        R::Stamp => stamp_results(th, a),
        R::Editorial => editorial(th, a),
        R::Panel | R::Floating => {}
    }
}

fn receipt(th: &Theme, a: &ResultsArgs) {
    let m = th.style.motion;
    let p = results_rect(th, a.area);
    let e = m.enter((a.t / m.duration().max(0.35)).min(1.0));
    let y0 = p.y - (1.0 - e.min(1.0)) * p.h * 0.5;
    let al = (a.t / 0.2).min(1.0);
    // The slip, with a torn zigzag bottom.
    let mut pts = vec![p.x, y0, p.right(), y0];
    let teeth = (p.w / 12.0) as usize;
    for i in 0..=teeth {
        let x = p.right() - i as f32 * p.w / teeth as f32;
        pts.extend([x, y0 + p.h - if i % 2 == 0 { 0.0 } else { 7.0 }]);
    }
    let paper = fade(th.panel_top, al);
    if let Shadow::Hard(dx, dy) = th.style.shadow {
        let off: Vec<f32> = pts.chunks(2).flat_map(|c| [c[0] + dx, c[1] + dy]).collect();
        poly(&off, fade(th.shadow, al));
    } else if th.style.shadow == Shadow::Soft {
        super::shape::shadow(Rect::new(p.x, y0, p.w, p.h), 2.0, 8.0, 22.0, fade(th.shadow, al));
    }
    poly(&pts, paper);
    style::texture(th.style.texture, Rect::new(p.x + 4.0, y0 + 4.0, p.w - 8.0, p.h - 16.0), with_alpha(darken(th.panel_bottom, 0.6), 0.14 * al), 0x4ec1);
    let ink = fade(th.text, al);
    let dim = fade(th.text_dim, al);
    let (x0, x1) = (p.x + 16.0, p.right() - 16.0);
    let mut y = y0 + 18.0;
    let hs = th.fit(th.title_font, 30.0);
    text(&th.case(a.heading)).font(th.title_font).size(hs).color(ink).tracking(th.style.tracking).center().draw(p.cx(), y);
    y += metrics(th.title_font).line * hs + 2.0;
    let body = th.fit(th.body_font, 16.0);
    let dashes = |y: f32| {
        let mut x = x0;
        while x < x1 {
            gfx2d::rect(x, y, 5.0, 1.5, dim);
            x += 9.0;
        }
    };
    dashes(y);
    y += 12.0;
    let row = |label: &str, value: &str, y: f32| {
        text(label).font(th.body_font).size(body).color(ink).draw(x0, y);
        let r = text(value).font(th.body_font).size(body).color(ink).right().draw(x1, y);
        let lw = text(label).font(th.body_font).size(body).measure().0;
        let dots_y = y + metrics(th.body_font).ascent * body - 2.0;
        let mut x = x0 + lw + 6.0;
        while x < r.x - 6.0 {
            gfx2d::rect(x, dots_y, 2.0, 2.0, dim);
            x += 6.0;
        }
    };
    let label = if a.label.is_empty() { "SCORE".to_string() } else { a.label.to_string() };
    // The score, big, then the itemised rows.
    let ns = th.fit(th.number_font, 54.0);
    gfx2d::push();
    gfx2d::translate(p.cx(), y + ns * 0.55);
    gfx2d::scale(a.punch, a.punch);
    text(&fmt_int(a.shown)).font(th.number_font).size(ns).color(if a.landed && a.new_best { fade(th.accent, al) } else { ink }).middle().draw(0.0, 0.0);
    gfx2d::pop();
    y += ns * 1.15;
    row(&th.case(&label), &fmt_int(a.shown), y);
    y += body * 1.5;
    row(&th.case("best"), &fmt_int(a.best.max(if a.landed { a.shown } else { 0 })), y);
    y += body * 1.6;
    dashes(y);
    y += 12.0;
    if a.landed && a.new_best {
        if (a.t * 3.0).fract() < 0.7 {
            text(&th.case("** new best **")).font(th.body_font).size(body).color(fade(th.accent, al)).center().draw(p.cx(), y);
        }
    } else {
        text(&th.case("thank you, come again")).font(th.body_font).size(body * 0.85).color(dim).center().draw(p.cx(), y);
    }
}

fn scoreboard(th: &Theme, a: &ResultsArgs) {
    let m = th.style.motion;
    let p = results_rect(th, a.area);
    let e = m.enter((a.t / m.duration().max(0.3)).min(1.0)).max(0.0);
    let al = (a.t / 0.2).min(1.0);
    gfx2d::push();
    gfx2d::translate(p.cx(), p.cy());
    gfx2d::scale(1.0, e);
    gfx2d::translate(-p.cx(), -p.cy());
    style::frame(&th.style, p, th.radius, fade(th.panel_top, al), fade(th.panel_bottom, al), fade(th.panel_border, al), fade(th.shadow, al), th.accent);
    gfx2d::pop();
    let hs = th.fit(th.body_font, 16.0);
    text(&th.case(a.heading)).font(th.body_font).size(hs).color(fade(th.text, al)).tracking(0.2).center().draw(p.cx(), p.y + 16.0);
    // Digit tiles.
    let digits = fmt_int(a.shown);
    let n = digits.chars().count();
    let tile_w = ((p.w - 32.0) / n as f32 - 6.0).min(48.0);
    let tile_h = tile_w * 1.4;
    let total = n as f32 * (tile_w + 6.0) - 6.0;
    let mut x = p.cx() - total / 2.0;
    let ty = p.y + 46.0;
    let dark = darken(th.panel_bottom, 0.55);
    let col = if a.landed && a.new_best { th.gold } else { th.text };
    let mut buf = [0u8; 4];
    for (i, c) in digits.chars().enumerate() {
        let k = m.enter(m.progress(a.t - 0.2, i)).clamp(0.0, 1.0);
        if c == ',' || c == '.' {
            text(c.encode_utf8(&mut buf)).font(th.number_font).size(tile_h * 0.6).color(fade(col, k)).center().draw(x + tile_w / 2.0, ty + tile_h * 0.3);
        } else {
            style::fill(th.style.shape, Rect::new(x, ty, tile_w, tile_h), 6.0, fade(dark, k * al), fade(darken(dark, 0.3), k * al));
            let ns = th.fit(th.number_font, tile_h * 0.72);
            text(c.encode_utf8(&mut buf)).font(th.number_font).size(ns).color(fade(col, k)).middle().draw(x + tile_w / 2.0, ty + tile_h / 2.0);
            gfx2d::rect(x, ty + tile_h / 2.0 - 0.75, tile_w, 1.5, fade(darken(dark, 0.5), k));
        }
        x += tile_w + 6.0;
    }
    let by = ty + tile_h + 26.0;
    let bs = th.fit(th.body_font, 15.0);
    if a.landed && a.new_best {
        let lit = (a.t * 2.5).fract() < 0.6;
        let lx = p.cx() - 58.0;
        if lit {
            soft_disc(lx, by, 13.0, 10.0, with_alpha(th.accent, 0.6));
        }
        disc(lx, by, 7.0, if lit { lighten(th.accent, 0.3) } else { darken(th.accent, 0.5) });
        text(&th.case("new best")).font(th.body_font).size(bs).color(th.accent).tracking(0.15).valign(VAlign::Middle).draw(lx + 16.0, by);
    } else if a.best > 0 {
        text(&format!("{}  {}", th.case("best"), fmt_int(a.best))).font(th.body_font).size(bs).color(fade(th.text_dim, al)).tracking(0.15).middle().draw(p.cx(), by);
    }
}

fn stamp_results(th: &Theme, a: &ResultsArgs) {
    let m = th.style.motion;
    let p = results_rect(th, a.area);
    let al = (a.t / 0.2).min(1.0);
    let ns = th.fit(th.number_font, 96.0);
    let ny = p.y + 116.0;
    gfx2d::push();
    gfx2d::translate(p.cx(), ny);
    gfx2d::scale(a.punch, a.punch);
    styled(th, &fmt_int(a.shown), th.number_font, ns, if a.landed && a.new_best { th.gold } else { th.text }).alpha(al).middle().draw(0.0, 0.0);
    gfx2d::pop();
    let e = m.enter(((a.t - 0.15) / 0.35).clamp(0.0, 1.0));
    if a.t > 0.15 {
        let hs = th.fit(th.title_font, 40.0);
        stamp(th, &th.case(a.heading), th.title_font, p.cx(), p.y + 30.0, hs, -0.08, th.bad, e);
    }
    let by = ny + ns * 0.62 + 18.0;
    if a.landed && a.new_best {
        if a.stamp > 0.01 {
            stamp(th, &th.case("new best"), th.title_font, p.cx() + 30.0, by + 8.0, th.fit(th.title_font, 26.0), 0.1, th.gold, a.stamp.min(1.2));
        }
    } else if a.best > 0 {
        let bs = th.fit(th.body_font, 18.0);
        text(&format!("{} {}", th.case("best"), fmt_int(a.best))).font(th.body_font).size(bs).color(fade(th.text, al)).outline(0.06, with_alpha(th.outline, 0.6)).middle().draw(p.cx(), by);
    }
}

fn editorial(th: &Theme, a: &ResultsArgs) {
    let m = th.style.motion;
    let p = results_rect(th, a.area);
    let al = (a.t / 0.25).min(1.0);
    let x = p.x + 4.0;
    let hs = th.fit(th.title_font, 44.0);
    let k0 = m.enter(m.progress(a.t, 0));
    styled(th, &th.case(a.heading), th.title_font, hs, th.text).alpha(al).draw(x - (1.0 - k0.min(1.0)) * 30.0, p.y);
    let mut y = p.y + metrics(th.title_font).line * hs + 4.0;
    let k1 = m.enter(m.progress(a.t, 1)).clamp(0.0, 1.0);
    gfx2d::rect(x, y, (p.w - 8.0) * k1, 2.0, th.text);
    y += 12.0;
    let ls = th.fit(th.body_font, 14.0);
    let label = if a.label.is_empty() { "score" } else { a.label };
    text(&th.case(label)).font(th.body_font).size(ls).color(fade(th.text_dim, al)).tracking(0.18).draw(x, y);
    y += ls * 1.4;
    let ns = th.fit(th.number_font, 92.0);
    let col = if a.landed && a.new_best { th.accent } else { th.text };
    gfx2d::push();
    gfx2d::translate(x, y + ns * 0.5);
    gfx2d::scale(a.punch, a.punch);
    text(&fmt_int(a.shown)).font(th.number_font).size(ns).color(fade(col, al)).tracking(th.style.tracking).valign(VAlign::Middle).draw(0.0, 0.0);
    gfx2d::pop();
    y += ns * 1.05;
    gfx2d::rect(x, y, (p.w - 8.0) * k1 * 0.35, 1.0, fade(th.text_dim, al));
    y += 10.0;
    let bs = th.fit(th.body_font, 17.0);
    if a.landed && a.new_best {
        let e = a.stamp.clamp(0.0, 1.0);
        text(&th.case("a new best.")).font(th.title_font).size(th.fit(th.title_font, 24.0)).color(fade(th.accent, e)).draw(x, y);
    } else if a.best > 0 {
        text(&format!("{} {}", th.case("best"), fmt_int(a.best))).font(th.body_font).size(bs).color(fade(th.text_dim, al)).draw(x, y);
    }
}

// --- HUD.

/// Draws the score in `th.style.hud`'s layout; returns the y under it.
pub fn hud_score(th: &Theme, l: &Layout, s: &str, size: f32, punch: f32, color: u32) -> (f32, f32, Align) {
    use super::style::HudLayout as H;
    match th.style.hud {
        H::Center => {
            let size = th.fit(th.number_font, size);
            let (cx, y) = (l.hud.cx(), l.hud.y + 14.0 + size * 0.45);
            gfx2d::push();
            gfx2d::translate(cx, y);
            gfx2d::scale(punch, punch);
            styled(th, s, th.number_font, size, color).tracking(0.0).middle().draw(0.0, 0.0);
            gfx2d::pop();
            (cx, y + size * 0.62, Align::Center)
        }
        H::Corner => {
            let size = th.fit(th.number_font, size * 0.72);
            let (x, y) = (l.hud.right() - 16.0, l.hud.y + 12.0 + size * 0.45);
            gfx2d::push();
            gfx2d::translate(x, y);
            gfx2d::scale(punch, punch);
            styled(th, s, th.number_font, size, color).tracking(0.0).right().valign(VAlign::Middle).draw(0.0, 0.0);
            gfx2d::pop();
            (x, y + size * 0.6, Align::Right)
        }
        H::Badge => {
            let size = th.fit(th.number_font, size * 0.7);
            let tw = text(s).font(th.number_font).size(size).measure().0;
            let r = Rect::centered(l.hud.cx(), l.hud.y + 14.0 + size * 0.55, (tw + 40.0).max(96.0), metrics(th.number_font).cap * size + 26.0);
            gfx2d::push();
            gfx2d::translate(r.cx(), r.cy());
            gfx2d::scale(punch, punch);
            let local = Rect::centered(0.0, 0.0, r.w, r.h);
            style::frame(&th.style, local, th.radius * 0.7, th.panel_top, th.panel_bottom, th.panel_border, th.shadow, th.accent);
            text(s).font(th.number_font).size(size).color(color).middle().draw(0.0, 0.0);
            gfx2d::pop();
            (r.cx(), r.bottom() + 6.0, Align::Center)
        }
    }
}

/// A small mixed color for HUD captions over any backdrop.
pub fn caption(th: &Theme) -> u32 {
    mix(th.text, th.text_dim, 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Theme, identity::IDENTITIES};

    /// Every identity's layouts draw without panicking at any time, and the
    /// results blocks stay inside the game-over card's area.
    #[test]
    fn every_identity_lays_out() {
        let l = Layout::default();
        for name in IDENTITIES {
            let th = Theme::preset(name);
            for t in [0.0, 0.3, 1.0, 5.0] {
                let a = TitleArgs { title: "Mega Stack", tagline: Some("how high?"), best: Some(("BEST", 42)), size: 64.0, y: 120.0, color: th.text, t };
                title(&th, &l, &a);
                let r = ResultsArgs { heading: "SPLAT", label: "", shown: 1234, punch: 1.0, best: 2000, new_best: t > 1.0, landed: t > 0.5, stamp: 1.0, t, area: l.results };
                results(&th, &r);
                hud_score(&th, &l, "1,234", 56.0, 1.0, th.text);
            }
            let p = results_rect(&th, l.results);
            let (_, ry) = retry_center(&th, p);
            assert!(p.x >= l.results.x && p.right() <= l.results.right() + 0.5, "{name}");
            assert!(ry + 34.0 <= l.results.bottom(), "{name}: retry at {ry}, clear of the challenge strip");
        }
    }
}
