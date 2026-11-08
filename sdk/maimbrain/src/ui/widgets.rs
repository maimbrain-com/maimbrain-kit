//! Widgets: panels, buttons, pills, progress bars and meters, ribbons.

use std::f32::consts::TAU;

use super::color::{darken, fade, lighten, with_alpha};
use super::icon::Icon;
use super::layout::{MIN_TOUCH, Rect};
use super::shape::{arc, poly, ring, rrect, rrect_gradient, rrect_stroke, shadow};
use super::text::{Align, VAlign, text};
use super::style::{Border, Shadow, Shape, fill, stroke};
use super::theme::Theme;
use crate::gfx2d::{self, Font};
use crate::input::{Event, Kind};
use crate::motion::{Punch, Spring};

/// A card in the theme's style: its shape, shadow, body, texture and border.
pub fn panel(r: Rect, theme: &Theme) {
    super::style::frame(&theme.style, r, theme.radius, theme.panel_top, theme.panel_bottom, theme.panel_border, theme.shadow, theme.accent);
}

/// A panel in any colors (`border` alpha 0 = none, `shadow_color` alpha 0 = flat).
pub fn panel_colored(r: Rect, radius: f32, top: u32, bottom: u32, border: u32, shadow_color: u32) {
    if shadow_color & 0xff != 0 {
        shadow(r, radius, 10.0, 28.0, shadow_color);
    }
    rrect_gradient(r, radius, top, bottom);
    // A light rim along the top inside edge reads as a lit bevel.
    gfx2d::rrect(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h * 0.5, (radius - 1.0).max(0.0), 1.5, 0.0, 0xffffff22, 0xffffff00);
    if border & 0xff != 0 {
        rrect_stroke(r, radius, 2.0, border);
    }
}

/// A rounded label: `text` on a pill `h` tall centred at (cx, cy), with an
/// optional icon before it. Returns its rect.
#[allow(clippy::too_many_arguments)]
pub fn pill(s: &str, cx: f32, cy: f32, h: f32, bg: u32, fg: u32, font: Font, icon: Option<Icon>) -> Rect {
    let t = text(s).font(font).size(pill_text_size(h, font)).color(fg);
    let icon_w = if icon.is_some() { h * 0.62 } else { 0.0 };
    let pad = h * 0.5;
    let r = Rect::centered(cx, cy, pill_width(s, h, font, icon.is_some()), h);
    rrect(r, h / 2.0, bg);
    let mut x = r.x + pad;
    if let Some(i) = icon {
        i.draw(x + icon_w * 0.4, cy, h * 0.55, fg);
        x += icon_w;
    }
    t.valign(VAlign::Middle).draw(x, cy);
    r
}

/// [`pill`] in a theme's shape and fonts (square for sharp themes, stepped
/// for pixel ones, and so on).
#[allow(clippy::too_many_arguments)]
pub fn pill_in(theme: &Theme, s: &str, cx: f32, cy: f32, h: f32, bg: u32, fg: u32, icon: Option<Icon>) -> Rect {
    let font = theme.body_font;
    let r = Rect::centered(cx, cy, pill_width(s, h, font, icon.is_some()), h);
    let shape = match theme.style.shape {
        super::style::Shape::Rounded => super::style::Shape::Pill,
        other => other,
    };
    super::style::fill(shape, r, h * 0.3, bg, bg);
    let icon_w = if icon.is_some() { h * 0.62 } else { 0.0 };
    let mut x = r.x + h * 0.5;
    if let Some(i) = icon {
        i.draw(x + icon_w * 0.4, cy, h * 0.55, fg);
        x += icon_w;
    }
    text(s).font(font).size(pill_text_size(h, font)).color(fg).valign(VAlign::Middle).draw(x, cy);
    r
}

fn pill_text_size(h: f32, font: Font) -> f32 {
    if font.is_bitmap() { super::theme::fit(font, h * 0.5) } else { h * 0.5 }
}

/// How wide [`pill`] draws `s` (to lay several out with `stack_h`).
pub fn pill_width(s: &str, h: f32, font: Font, icon: bool) -> f32 {
    let tw = text(s).font(font).size(pill_text_size(h, font)).measure().0;
    tw + if icon { h * 0.62 } else { 0.0 } + h
}

/// A rounded progress bar filled to `frac` (0…1), with a gloss.
pub fn progress_bar(r: Rect, frac: f32, fill: u32, track: u32) {
    let rad = r.h / 2.0;
    rrect(r, rad, track);
    let f = frac.clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    let w = r.w * f;
    // Narrower than its own height, a bar fades in rather than squashing its ends.
    let (w, c) = if w < r.h { (r.h, fade(fill, w / r.h)) } else { (w, fill) };
    let bar = Rect::new(r.x, r.y, w, r.h);
    rrect_gradient(bar, rad, lighten(c, 0.18), darken(c, 0.12));
    gfx2d::rrect(bar.x + rad * 0.5, bar.y + r.h * 0.14, (bar.w - rad).max(0.0), r.h * 0.3, r.h * 0.15, 0.0, 0.0, fade(0xffffff60, c as u8 as f32 / 255.0), 0xffffff00);
}

/// `n` segments, the first `filled` lit (fractions light a segment partway).
pub fn segments(r: Rect, n: usize, filled: f32, on: u32, off: u32) {
    for (i, s) in r.columns(n, 4.0).into_iter().enumerate() {
        let k = (filled - i as f32).clamp(0.0, 1.0);
        rrect(s, s.h.min(s.w) * 0.3, off);
        if k > 0.0 {
            rrect(s, s.h.min(s.w) * 0.3, fade(on, k));
        }
    }
}

/// A circular gauge: a track ring and an arc from the top, clockwise, to `frac`.
pub fn ring_meter(cx: f32, cy: f32, r: f32, width: f32, frac: f32, color: u32, track: u32) {
    ring(cx, cy, r, width, track);
    arc(cx, cy, r, width, 0.0, frac.clamp(0.0, 1.0) * TAU, color);
}

/// A banner with folded tails ("NEW BEST!"), turned by `angle`.
#[allow(clippy::too_many_arguments)]
pub fn ribbon(s: &str, cx: f32, cy: f32, h: f32, color: u32, fg: u32, font: Font, angle: f32) {
    let size = if font.is_bitmap() { super::theme::fit(font, h * 0.5) } else { h * 0.52 };
    let t = text(s).font(font).size(size).color(fg).align(Align::Center).valign(VAlign::Middle);
    let w = t.measure().0 + h * 1.2;
    gfx2d::push();
    gfx2d::translate(cx, cy);
    gfx2d::rotate(angle);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let tail = darken(color, 0.3);
    let fold = darken(color, 0.55);
    // Tails behind, notched, sitting lower; then the folds; then the band.
    for s in [-1.0f32, 1.0] {
        let x0 = s * (hw - h * 0.2);
        let x1 = s * (hw + h * 0.55);
        poly(&[x0, -hh + h * 0.28, x1, -hh + h * 0.28, x1 - s * h * 0.28, hh * 0.3 + h * 0.28, x1, hh + h * 0.28, x0, hh + h * 0.28], tail);
        poly(&[s * (hw - h * 0.2), hh, s * (hw - h * 0.2), hh + h * 0.28, s * (hw - h * 0.45), hh], fold);
    }
    gfx2d::rrect(-hw, -hh, w, h, h * 0.12, 0.0, 0.0, lighten(color, 0.15), darken(color, 0.1));
    gfx2d::rrect(-hw + 3.0, -hh + 3.0, w - 6.0, h - 6.0, h * 0.08, 1.5, 0.0, 0xffffff40, 0xffffff20);
    t.shadow(0.0, h * 0.05, with_alpha(darken(color, 0.6), 0.6)).draw(0.0, 0.0);
    gfx2d::pop();
}

/// A burst of rays behind something (a sunburst), turning with `rot`.
pub fn rays(cx: f32, cy: f32, r: f32, n: usize, rot: f32, color: u32) {
    let half = TAU / n as f32 * 0.25;
    for i in 0..n {
        let a = rot + i as f32 * TAU / n as f32;
        let p = [cx, cy, cx + r * (a - half).sin(), cy - r * (a - half).cos(), cx + r * (a + half).sin(), cy - r * (a + half).cos()];
        poly(&p, color);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Filled with the accent, with a raised edge.
    Primary,
    /// Translucent fill with a border.
    Secondary,
    /// Just the label (or icon).
    Ghost,
}

/// What a press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    /// Went down on the button (play a tick, buzz a haptic).
    Pressed,
    /// Released on it: do the thing.
    Clicked,
    /// Dragged off and let go.
    Cancelled,
}

/// A tappable button with a pressed state and a springy release. Feed it
/// every input event (`handle`), `update` it, `draw` it. Its touch area is
/// at least 44 × 44 even when it looks smaller.
#[derive(Clone, Debug)]
pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub icon: Option<Icon>,
    pub kind: ButtonKind,
    /// A circle (for icon buttons) instead of a rounded rect.
    pub round: bool,
    pub enabled: bool,
    /// Called on `Pressed` (e.g. `|| sensors::haptic(Haptic::Tap)`; needs
    /// `sensors = ["haptics"]`).
    pub haptic: Option<fn()>,
    held: Option<(bool, u8)>,
    inside: bool,
    press: Spring,
    pop: Punch,
}

impl Button {
    pub fn new(rect: Rect, label: &str) -> Button {
        Button {
            rect,
            label: label.to_string(),
            icon: None,
            kind: ButtonKind::Primary,
            round: false,
            enabled: true,
            haptic: None,
            held: None,
            inside: false,
            press: Spring::new(0.0, 7.0, 0.55),
            pop: Punch::new(),
        }
    }

    /// A round icon button `size` across, centred at (cx, cy).
    pub fn icon(cx: f32, cy: f32, size: f32, icon: Icon) -> Button {
        let mut b = Button::new(Rect::centered(cx, cy, size, size), "");
        b.icon = Some(icon);
        b.round = true;
        b
    }

    pub fn kind(mut self, k: ButtonKind) -> Button {
        self.kind = k;
        self
    }

    pub fn with_icon(mut self, i: Icon) -> Button {
        self.icon = Some(i);
        self
    }

    pub fn with_haptic(mut self, f: fn()) -> Button {
        self.haptic = Some(f);
        self
    }

    /// The touch area: the rect grown to at least 44 × 44.
    pub fn hit_rect(&self) -> Rect {
        self.rect.at_least(MIN_TOUCH, MIN_TOUCH)
    }

    pub fn is_held(&self) -> bool {
        self.held.is_some() && self.inside
    }

    /// Feeds one input event. Returns what it did to this button, if
    /// anything; a `Some` means the event was this button's (don't also
    /// treat it as a tap on the game).
    pub fn handle(&mut self, e: &Event) -> Option<Tap> {
        if !self.enabled {
            return None;
        }
        let mouse = matches!(e.kind(), Kind::MouseDown | Kind::MouseMove | Kind::MouseUp);
        let who = (mouse, e.id);
        let hit = self.hit_rect().contains(e.x, e.y);
        match e.kind() {
            Kind::TouchDown | Kind::MouseDown if hit && self.held.is_none() => {
                self.held = Some(who);
                self.inside = true;
                if let Some(h) = self.haptic {
                    h();
                }
                Some(Tap::Pressed)
            }
            Kind::TouchMove | Kind::MouseMove if self.held == Some(who) => {
                self.inside = hit;
                None
            }
            Kind::TouchUp | Kind::MouseUp if self.held == Some(who) => {
                self.held = None;
                if hit {
                    self.pop.kick(0.12);
                    Some(Tap::Clicked)
                } else {
                    Some(Tap::Cancelled)
                }
            }
            Kind::TouchCancel if self.held == Some(who) => {
                self.held = None;
                Some(Tap::Cancelled)
            }
            _ => None,
        }
    }

    /// [`Button::handle`] with the button's box on the screen given (a
    /// button drawn somewhere other than its `rect`: a scene node's).
    pub fn handle_at(&mut self, e: &Event, on_screen: Rect) -> Option<Tap> {
        let rect = self.rect;
        self.rect = on_screen;
        let t = self.handle(e);
        self.rect = rect;
        t
    }

    /// The pop a click gives it (Enter or a pad's A on a focused button).
    pub fn pop(&mut self) {
        self.pop.kick(0.12);
    }

    /// Forgets a press in progress (e.g. when the screen changes).
    pub fn reset(&mut self) {
        self.held = None;
        self.press.snap(0.0);
    }

    pub fn update(&mut self, dt: f32) {
        self.press.target = if self.is_held() { 1.0 } else { 0.0 };
        self.press.update(dt);
        self.pop.update(dt);
    }

    /// How far down it is, 0…1 (springs past 0 on release).
    pub fn pressed_amount(&self) -> f32 {
        self.press.value
    }

    pub fn draw(&self, theme: &Theme) {
        self.draw_alpha(theme, 1.0);
    }

    /// Draws faded by `a` (screens fading in).
    pub fn draw_alpha(&self, theme: &Theme, a: f32) {
        if a <= 0.0 {
            return;
        }
        let p = self.press.value.clamp(-0.3, 1.0);
        let s = (1.0 - 0.04 * p) * self.pop.scale();
        let r = self.rect;
        let st = &theme.style;
        let classic = matches!(st.shape, Shape::Rounded | Shape::Pill);
        let radius = if self.round && classic {
            r.h.min(r.w) / 2.0
        } else if st.shape == Shape::Pill {
            r.h / 2.0
        } else if classic {
            (theme.radius * 0.6).min(r.h / 2.0)
        } else {
            theme.radius.max(6.0) * 0.6
        };
        let dim = if self.enabled { 1.0 } else { 0.45 };
        let f = |c: u32| fade(c, a * dim);
        gfx2d::push();
        gfx2d::translate(r.cx(), r.cy());
        gfx2d::scale(s, s);
        let local = Rect::centered(0.0, 0.0, r.w, r.h);
        let (fg, lift) = match self.kind {
            ButtonKind::Primary if !classic || matches!(st.shadow, Shadow::Hard(..)) => {
                // Square-ish faces: a solid block under the face (the hard
                // shadow, or a darker edge) that the face sinks into.
                let (dx, dy) = match st.shadow {
                    Shadow::Hard(dx, dy) => (dx.clamp(-5.0, 5.0), dy.clamp(-5.0, 5.0)),
                    _ => (0.0, 4.0),
                };
                let under = if matches!(st.shadow, Shadow::Hard(..)) { theme.shadow } else { darken(theme.accent, 0.4) };
                if st.shadow == Shadow::Glow {
                    super::shape::glow(local, radius.max(6.0), 22.0, f(with_alpha(theme.accent, 0.5)));
                }
                fill(st.shape, local.offset(dx, dy), radius, f(under), f(under));
                let k = p.max(0.0);
                let face = local.offset(dx * k, dy * k);
                fill(st.shape, face, radius, f(lighten(theme.accent, 0.08)), f(darken(theme.accent, 0.06)));
                if st.shape == Shape::Bevel {
                    let w = 2.5;
                    gfx2d::rect(face.x, face.y, face.w, w, f(lighten(theme.accent, 0.5)));
                    gfx2d::rect(face.x, face.y, w, face.h, f(lighten(theme.accent, 0.4)));
                    gfx2d::rect(face.x, face.bottom() - w, face.w, w, f(darken(theme.accent, 0.5)));
                    gfx2d::rect(face.right() - w, face.y, w, face.h, f(darken(theme.accent, 0.45)));
                }
                if let Border::Line(w) | Border::Double(w) | Border::Dashed(w) = st.border {
                    stroke(st.shape, face, radius, w.min(3.0), f(theme.panel_border));
                }
                (theme.on_accent, dy * k)
            }
            ButtonKind::Primary => {
                let edge = 5.0;
                let down = edge * p.max(0.0);
                shadow(local.offset(0.0, edge + 3.0), radius, 3.0, 10.0, f(fade(theme.shadow, 0.8)));
                // The raised edge under the face; the face sinks into it when pressed.
                rrect(local.offset(0.0, edge), radius, f(darken(theme.accent, 0.35)));
                let face = local.offset(0.0, down);
                rrect_gradient(face, radius, f(lighten(theme.accent, 0.16)), f(darken(theme.accent, 0.06)));
                // A gloss across the top half (inset on round buttons so it stays inside the circle).
                let g = if self.round {
                    Rect::new(face.x + face.w * 0.2, face.y + face.h * 0.08, face.w * 0.6, face.h * 0.36)
                } else {
                    Rect::new(face.x + 2.0, face.y + 2.0, face.w - 4.0, face.h * 0.48)
                };
                let gr = if self.round { g.h / 2.0 } else { (radius - 2.0).max(0.0) };
                gfx2d::rrect(g.x, g.y, g.w, g.h, gr, 0.0, 0.0, f(0xffffff38), f(0xffffff08));
                (theme.on_accent, down)
            }
            ButtonKind::Secondary => {
                fill(st.shape, local, radius, f(theme.secondary), f(theme.secondary));
                stroke(st.shape, local, radius, 2.0, f(with_alpha(theme.text, 0.35)));
                if p > 0.0 {
                    fill(st.shape, local, radius, f(with_alpha(0xffffffff, 0.12 * p)), f(with_alpha(0xffffffff, 0.12 * p)));
                }
                (theme.on_secondary, 0.0)
            }
            ButtonKind::Ghost => {
                if p > 0.0 {
                    fill(st.shape, local, radius, f(with_alpha(theme.text, 0.12 * p)), f(with_alpha(theme.text, 0.12 * p)));
                }
                (theme.text, 0.0)
            }
        };
        let font = theme.body_font;
        let size = theme.fit(font, (r.h * 0.4).min(26.0));
        let has_label = !self.label.is_empty();
        let label = text(&self.label).font(font).size(size).color(f(fg));
        let lw = if has_label { label.measure().0 } else { 0.0 };
        let isz = if has_label { r.h * 0.46 } else { r.h.min(r.w) * 0.5 };
        let gap = if has_label && self.icon.is_some() { isz * 0.35 } else { 0.0 };
        let total = lw + gap + if self.icon.is_some() { isz } else { 0.0 };
        let mut x = -total / 2.0;
        let shade = f(with_alpha(darken(theme.accent, 0.6), 0.5));
        if let Some(i) = self.icon {
            if self.kind == ButtonKind::Primary {
                i.draw(x + isz / 2.0, lift + 1.5, isz, shade);
            }
            i.draw(x + isz / 2.0, lift, isz, f(fg));
            x += isz + gap;
        }
        if has_label {
            let l = label.valign(VAlign::Middle);
            let l = if self.kind == ButtonKind::Primary { l.shadow(0.0, 1.5, shade) } else { l };
            l.draw(x, lift);
        }
        gfx2d::pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: Kind, x: f32, y: f32) -> Event {
        Event { kind: kind as u8, x, y, ..Default::default() }
    }

    #[test]
    fn buttons_click_on_release_inside_and_cancel_outside() {
        let mut b = Button::new(Rect::new(100.0, 100.0, 20.0, 20.0), "GO");
        // The touch area is at least 44 × 44 around the 20 × 20 button.
        assert_eq!(b.handle(&ev(Kind::TouchDown, 89.0, 89.0)), Some(Tap::Pressed));
        assert!(b.is_held());
        assert_eq!(b.handle(&ev(Kind::TouchUp, 110.0, 110.0)), Some(Tap::Clicked));
        assert_eq!(b.handle(&ev(Kind::TouchDown, 110.0, 110.0)), Some(Tap::Pressed));
        assert_eq!(b.handle(&ev(Kind::TouchMove, 300.0, 300.0)), None);
        assert!(!b.is_held());
        assert_eq!(b.handle(&ev(Kind::TouchUp, 300.0, 300.0)), Some(Tap::Cancelled));
        // Misses aren't the button's.
        assert_eq!(b.handle(&ev(Kind::TouchDown, 0.0, 0.0)), None);
        b.enabled = false;
        assert_eq!(b.handle(&ev(Kind::TouchDown, 110.0, 110.0)), None);
    }

    #[test]
    fn pressing_springs_down_and_back() {
        let mut b = Button::new(Rect::new(0.0, 0.0, 100.0, 50.0), "GO");
        b.handle(&ev(Kind::MouseDown, 50.0, 25.0));
        for _ in 0..30 {
            b.update(1.0 / 60.0);
        }
        assert!(b.pressed_amount() > 0.9);
        b.handle(&ev(Kind::MouseUp, 50.0, 25.0));
        for _ in 0..90 {
            b.update(1.0 / 60.0);
        }
        assert!(b.pressed_amount().abs() < 0.05);
    }
}

