//! Widgets as retained nodes: what a scene's `Kind::Widget` holds and draws
//! (Studio engine 2.0's `ui.button`, `ui.panel`, `ui.pill`, `ui.bar`,
//! `ui.ring` and `ui.icon`, docs/STUDIO-2.md §5 [ui]). Each draws in the
//! scene's theme, so a game's buttons and meters are in its identity: a
//! pixel identity's bar is stepped blocks, a brutalist one's a hard-edged
//! slab with a hard shadow, a storybook one's a soft rounded pill.
//!
//! A widget is drawn centred on its node, `w` × `h` (a pill and an icon
//! size themselves). A button's press state, springs and focus live here;
//! who pressed it is the owner's business ([`Widget::press`] with the
//! input event and where the button is on the screen; [`Focus`] for Enter
//! and a pad's A). Touch parity: a button any key or pad can press is a
//! button a finger can tap, at least 44 units square
//! ([`MIN_TOUCH`](super::layout::MIN_TOUCH)).
//!
//! ```
//! use maimbrain::ui::{Theme, Widget};
//! let mut b = Widget::button("PLAY");
//! b.update(1.0 / 60.0, &Theme::preset("riso"));
//! assert_eq!(b.size(&Theme::preset("riso"), 0.0, 0.0), (160.0, 56.0));
//! // render, at the node: b.draw(&theme, w, h, alpha)
//! ```

use super::color::{darken, fade, lighten, with_alpha};
use super::icon::Icon;
use super::layout::Rect;
use super::shape::{arc, ring};
use super::style::{Border, Motion, Shadow, Shape, fill, frame, stroke};
use super::text::{VAlign, text};
use super::theme::Theme;
use super::widgets::{Button, ButtonKind, Tap, pill_width};
use crate::gfx2d;
use crate::input::Event;

/// What a widget is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    /// A tappable button (its label, an icon, or both).
    Button,
    /// A card in the identity's frame.
    Panel,
    /// A label in a pill (or the identity's shape for one).
    Pill,
    /// A meter filled to `value` (0–1).
    Bar,
    /// A circular meter filled to `value` (0–1), with its text in the middle.
    Ring,
    /// One of [`Icon::ALL`].
    Icon,
}

impl WidgetKind {
    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Button => "button",
            WidgetKind::Panel => "panel",
            WidgetKind::Pill => "pill",
            WidgetKind::Bar => "bar",
            WidgetKind::Ring => "ring",
            WidgetKind::Icon => "icon",
        }
    }
}

/// An icon by its name in [`Icon::ALL`] ("star", "coin", "up", …).
pub fn icon_named(name: &str) -> Option<Icon> {
    Icon::ALL.iter().find(|(n, _)| *n == name).map(|(_, i)| *i)
}

/// The name an icon has in [`Icon::ALL`].
pub fn icon_name(icon: Icon) -> Option<&'static str> {
    Icon::ALL.iter().find(|(_, i)| std::ptr::fn_addr_eq(i.0, icon.0)).map(|(n, _)| *n)
}

/// One widget: see the [module docs](self).
#[derive(Clone, Debug)]
pub struct Widget {
    pub kind: WidgetKind,
    /// The label, icon, look and press state (a button's; the label and
    /// icon of the other kinds too).
    pub button: Button,
    /// A bar's or ring's fill, 0–1.
    pub value: f32,
    /// The fill drawn (it eases to `value` in the identity's motion).
    shown: f32,
    /// A pill or panel in the accent instead of the panel colour; a bar or
    /// ring in the identity's `good`, `bad` or `gold` (`tone`).
    pub tone: Tone,
    /// The button Enter and a pad's A press, and whether to draw its ring
    /// (once a key or pad has been used: a finger never sees one).
    pub focused: bool,
    pub ring: bool,
    age: f32,
}

/// Which of the identity's colours a widget is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    /// Panels and pills in the panel colour; meters in the accent.
    #[default]
    Plain,
    Accent,
    Good,
    Bad,
    Gold,
}

impl Tone {
    pub fn named(s: &str) -> Option<Tone> {
        Some(match s {
            "plain" => Tone::Plain,
            "accent" | "primary" => Tone::Accent,
            "good" => Tone::Good,
            "bad" => Tone::Bad,
            "gold" => Tone::Gold,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Tone::Plain => "plain",
            Tone::Accent => "accent",
            Tone::Good => "good",
            Tone::Bad => "bad",
            Tone::Gold => "gold",
        }
    }
    /// The colour it fills a meter with.
    fn meter(self, th: &Theme) -> u32 {
        match self {
            Tone::Plain | Tone::Accent => th.accent,
            Tone::Good => th.good,
            Tone::Bad => th.bad,
            Tone::Gold => th.gold,
        }
    }
}

impl Widget {
    pub fn new(kind: WidgetKind) -> Widget {
        let mut button = Button::new(Rect::default(), "");
        if kind == WidgetKind::Icon {
            button.icon = Some(Icon::STAR);
        }
        Widget { kind, button, value: 0.0, shown: 0.0, tone: Tone::Plain, focused: false, ring: false, age: 0.0 }
    }

    pub fn button(label: &str) -> Widget {
        let mut w = Widget::new(WidgetKind::Button);
        w.button.label = label.to_string();
        w
    }

    pub fn text(&self) -> &str {
        &self.button.label
    }

    pub fn set_text(&mut self, s: &str) {
        if self.button.label != s {
            self.button.label = s.to_string();
        }
    }

    /// The size it is when the game doesn't say (`w`, `h` 0): a button
    /// 160 × 56 (an icon-only one 60 round), a panel 240 × 160, a bar
    /// 160 × 14, a ring 80 across, an icon 32; a pill's width is always
    /// its text's. A button's touch area is never under 44 × 44.
    pub fn size(&self, th: &Theme, w: f32, h: f32) -> (f32, f32) {
        let or = |v: f32, d: f32| if v > 0.0 { v } else { d };
        match self.kind {
            WidgetKind::Button if self.button.label.is_empty() && self.button.icon.is_some() => {
                let s = or(w.max(h), 60.0);
                (s, s)
            }
            WidgetKind::Button => {
                let h = or(h, 56.0);
                let min = text(&self.button.label).font(th.body_font).size(th.fit(th.body_font, (h * 0.4).min(26.0))).measure().0 + h * 0.9 + if self.button.icon.is_some() { h * 0.6 } else { 0.0 };
                (or(w, 160.0f32.max(min)), h)
            }
            WidgetKind::Panel => (or(w, 240.0), or(h, 160.0)),
            WidgetKind::Pill => {
                let h = or(h, 32.0);
                (pill_width(&self.button.label, h, th.body_font, self.button.icon.is_some()), h)
            }
            WidgetKind::Bar => (or(w, 160.0), or(h, 14.0)),
            WidgetKind::Ring => {
                let d = or(w.max(h), 80.0);
                (d, d)
            }
            WidgetKind::Icon => {
                let s = or(w.max(h), 32.0);
                (s, s)
            }
        }
    }

    /// Feeds one pointer event, with the button's box on the screen (in
    /// the event's units; its touch area is at least 44 × 44 around it).
    /// `Some` means the event was the button's.
    pub fn press(&mut self, e: &Event, on_screen: Rect) -> Option<Tap> {
        if self.kind != WidgetKind::Button {
            return None;
        }
        self.button.handle_at(e, on_screen)
    }

    /// Enter or a pad's A on the focused button: it pops as a tap would.
    pub fn activate(&mut self) -> Option<Tap> {
        (self.kind == WidgetKind::Button && self.button.enabled).then(|| {
            self.button.pop();
            Tap::Clicked
        })
    }

    /// Moves on its springs, its fill and its focus ring.
    pub fn update(&mut self, dt: f32, th: &Theme) {
        self.age += dt;
        self.button.update(dt);
        let v = self.value.clamp(0.0, 1.0);
        self.shown = match th.style.motion {
            // Stepped: ticks over in eighths.
            Motion::Mechanical => (v * 8.0).round() / 8.0,
            m => {
                let rate = match m {
                    Motion::Snappy => 22.0,
                    Motion::Floaty => 5.0,
                    _ => 11.0,
                };
                let k = 1.0 - (-rate * dt).exp();
                let s = self.shown + (v - self.shown) * k;
                if (s - v).abs() < 1e-3 { v } else { s }
            }
        };
    }

    /// Snaps the drawn fill to the value (a widget just made shows its value).
    pub fn settle(&mut self) {
        self.shown = self.value.clamp(0.0, 1.0);
    }

    /// The fill as drawn now.
    pub fn shown(&self) -> f32 {
        self.shown
    }

    /// Draws it centred on (0, 0), `w` × `h`, faded by `a`.
    pub fn draw(&self, th: &Theme, w: f32, h: f32, a: f32) {
        self.draw_tinted(th, w, h, a, 0xffffffff);
    }

    /// The theme a button draws in: a toned one (`good`, `bad`, `gold`) is
    /// the identity's button in that colour (a primary's face, a
    /// secondary's fill, a ghost's label), with text that reads on it.
    pub fn button_theme(&self, th: &Theme) -> Theme {
        match self.tone {
            Tone::Good | Tone::Bad | Tone::Gold => {
                let c = self.tone.meter(th);
                let readable = |fg: u32| super::color::readable(fg, c, 3.0);
                Theme { accent: c, on_accent: readable(th.on_accent), secondary: c, on_secondary: readable(th.on_secondary), text: c, ..*th }
            }
            Tone::Plain | Tone::Accent => *th,
        }
    }

    /// [`draw`](Widget::draw) with `tint` multiplied into its fills (a
    /// node's `color`; white leaves the identity's colours as they are).
    pub fn draw_tinted(&self, th: &Theme, w: f32, h: f32, a: f32, tint: u32) {
        if a <= 0.0 {
            return;
        }
        let th = &tinted(th, tint);
        let r = Rect::centered(0.0, 0.0, w, h);
        let f = |c: u32| fade(c, a);
        match self.kind {
            WidgetKind::Button => {
                if self.focused && self.ring && self.button.enabled {
                    focus_ring(th, r, self.age, a);
                }
                let mut b = self.button.clone();
                b.rect = r;
                b.round = self.button.label.is_empty() && self.button.icon.is_some();
                b.draw_alpha(&self.button_theme(th), a);
            }
            WidgetKind::Panel => {
                let (top, bottom) = match self.tone {
                    Tone::Plain => (th.panel_top, th.panel_bottom),
                    t => {
                        let c = t.meter(th);
                        (lighten(c, 0.06), darken(c, 0.08))
                    }
                };
                frame(&super::style::for_cards(&th.style), r, th.radius, f(top), f(bottom), f(th.panel_border), f(th.shadow), f(th.accent));
            }
            WidgetKind::Pill => {
                let (bg, fg) = match self.tone {
                    Tone::Plain => (with_alpha(th.panel_top, 0.92), th.text),
                    Tone::Accent => (th.accent, th.on_accent),
                    t => (t.meter(th), super::color::readable(th.on_accent, t.meter(th), 3.0)),
                };
                let s = th.style.shape;
                if matches!(th.style.shadow, Shadow::Hard(..)) || matches!(th.style.border, Border::Line(_) | Border::Double(_)) {
                    // A print identity's pill sits on its hard shadow, outlined.
                    let (dx, dy) = match th.style.shadow {
                        Shadow::Hard(dx, dy) => (dx.clamp(-3.0, 3.0), dy.clamp(-3.0, 3.0)),
                        _ => (0.0, 0.0),
                    };
                    if dx != 0.0 || dy != 0.0 {
                        fill(pill_shape(s), r.offset(dx, dy), h * 0.3, f(th.shadow), f(th.shadow));
                    }
                }
                super::widgets::pill_in(th, &self.button.label, 0.0, 0.0, h, f(bg), f(fg), self.button.icon);
                if let Border::Line(bw) | Border::Double(bw) = th.style.border {
                    stroke(pill_shape(s), r, h * 0.3, bw.min(2.5), f(th.panel_border));
                }
            }
            WidgetKind::Bar => bar_in(th, r, self.shown, self.tone.meter(th), a),
            WidgetKind::Ring => {
                let rad = w.min(h) / 2.0;
                ring_in(th, rad, self.shown, self.tone.meter(th), a);
                if !self.button.label.is_empty() {
                    let size = th.fit(th.number_font, rad * 0.62);
                    text(&self.button.label).font(th.number_font).size(size).color(f(th.text)).outline(th.outline_em, f(th.outline)).valign(VAlign::Middle).center().draw(0.0, 0.0);
                } else if let Some(i) = self.button.icon {
                    i.draw(0.0, 0.0, rad * 0.9, f(th.text));
                }
            }
            WidgetKind::Icon => {
                if let Some(i) = self.button.icon {
                    let s = w.min(h);
                    let c = match self.tone {
                        Tone::Plain => th.text,
                        Tone::Accent => th.accent,
                        t => t.meter(th),
                    };
                    // The identity's outline under it, as its numbers have.
                    if th.style.outlined || matches!(th.style.shadow, Shadow::Hard(..)) {
                        i.draw(0.0, s * 0.05, s, f(with_alpha(th.outline, 0.8)));
                    }
                    i.draw(0.0, 0.0, s, f(c));
                }
            }
        }
    }
}

/// A pill's outline in a shape language (rounded corners become a pill).
fn pill_shape(s: Shape) -> Shape {
    if s == Shape::Rounded { Shape::Pill } else { s }
}

/// The theme with `tint` multiplied into its fills (white: unchanged).
fn tinted(th: &Theme, tint: u32) -> Theme {
    if tint == 0xffffffff {
        return *th;
    }
    let mut t = *th;
    let m = |c: u32| {
        let ch = |s: u32| ((((c >> s) & 0xff) * ((tint >> s) & 0xff) + 127) / 255) << s;
        ch(24) | ch(16) | ch(8) | ch(0)
    };
    (t.accent, t.panel_top, t.panel_bottom, t.secondary) = (m(t.accent), m(t.panel_top), m(t.panel_bottom), m(t.secondary));
    t
}

/// The ring around the focused button: the accent, pulsing, in the shape.
fn focus_ring(th: &Theme, r: Rect, t: f32, a: f32) {
    let pulse = 0.65 + 0.35 * (t * std::f32::consts::TAU * 0.8).sin().abs();
    let g = r.inset(-6.0);
    let shape = th.style.shape;
    let radius = if matches!(shape, Shape::Rounded | Shape::Pill) { (th.radius * 0.6).min(r.h / 2.0) + 6.0 } else { th.radius.max(6.0) * 0.6 + 4.0 };
    stroke(shape, g, radius, 3.0, fade(with_alpha(th.accent, pulse), a));
}

/// A meter bar in the identity: a stepped row of blocks for a pixel one,
/// a slab on its hard shadow for a print one, a glowing tube for neon,
/// segments for a mechanical one; else a rounded bar with a gloss.
pub fn bar_in(th: &Theme, r: Rect, frac: f32, color: u32, a: f32) {
    let f = |c: u32| fade(c, a);
    let st = &th.style;
    let frac = frac.clamp(0.0, 1.0);
    let track = with_alpha(super::color::mix(th.panel_bottom, th.outline, 0.5), 0.75);
    match (st.shape, st.motion) {
        (Shape::Pixel, _) | (_, Motion::Mechanical) => {
            // Blocks: whole ones lit.
            let n = ((r.w / (r.h * 1.1)).floor() as usize).clamp(4, 24);
            let gap = (r.h * 0.18).max(1.0).round();
            let bw = (r.w - gap * (n as f32 - 1.0)) / n as f32;
            if let Shadow::Hard(dx, dy) = st.shadow {
                gfx2d::rect(r.x + dx.clamp(-3.0, 3.0), r.y + dy.clamp(-3.0, 3.0), r.w, r.h, f(th.shadow));
            }
            let lit = (frac * n as f32).round() as usize;
            for i in 0..n {
                let x = r.x + i as f32 * (bw + gap);
                let c = if i < lit { color } else { track };
                gfx2d::rect(x, r.y, bw, r.h, f(c));
                if i < lit {
                    gfx2d::rect(x, r.y, bw, (r.h * 0.25).max(1.0), f(lighten(color, 0.3)));
                }
            }
        }
        _ => {
            let shape = pill_shape(st.shape);
            let radius = if shape == Shape::Pill { r.h / 2.0 } else { (th.radius * 0.3).min(r.h / 2.0) };
            if let Shadow::Hard(dx, dy) = st.shadow {
                fill(shape, r.offset(dx.clamp(-4.0, 4.0), dy.clamp(-4.0, 4.0)), radius, f(th.shadow), f(th.shadow));
            }
            if st.shadow == Shadow::Glow && frac > 0.0 {
                super::shape::glow(Rect::new(r.x, r.y, r.w * frac, r.h), r.h / 2.0, r.h * 1.2, f(with_alpha(color, 0.45)));
            }
            fill(shape, r, radius, f(track), f(track));
            if frac > 0.0 {
                let w = (r.w * frac).max(if shape == Shape::Pill { r.h.min(r.w) } else { 2.0 });
                let k = if w > r.w * frac { (r.w * frac / w).clamp(0.0, 1.0) } else { 1.0 };
                let bar = Rect::new(r.x, r.y, w, r.h);
                fill(shape, bar, radius, f(fade(lighten(color, 0.14), k)), f(fade(darken(color, 0.1), k)));
                if matches!(st.shape, Shape::Rounded | Shape::Pill) && st.shadow == Shadow::Soft {
                    let g = Rect::new(bar.x + radius * 0.5, bar.y + r.h * 0.14, (bar.w - radius).max(0.0), r.h * 0.3);
                    gfx2d::rrect(g.x, g.y, g.w, g.h, g.h / 2.0, 0.0, 0.0, f(fade(0xffffff60, k)), 0xffffff00);
                }
            }
            if let Border::Line(bw) | Border::Double(bw) | Border::Dashed(bw) = st.border {
                stroke(shape, r.inset(-bw.min(2.0)), radius + bw.min(2.0), bw.min(2.0), f(th.panel_border));
            }
        }
    }
}

/// A ring meter of radius `rad` centred on (0, 0) in the identity: twelve
/// blocks for a pixel or mechanical one, else an arc from the top,
/// clockwise, over its track (glowing for neon, on a hard shadow for print).
pub fn ring_in(th: &Theme, rad: f32, frac: f32, color: u32, a: f32) {
    let f = |c: u32| fade(c, a);
    let st = &th.style;
    let frac = frac.clamp(0.0, 1.0);
    let width = (rad * 0.2).clamp(4.0, 14.0);
    let mid = rad - width / 2.0;
    let track = with_alpha(super::color::mix(th.panel_bottom, th.outline, 0.5), 0.75);
    // A face to read its text on.
    super::shape::disc(0.0, 0.0, rad - width, f(with_alpha(th.panel_bottom, 0.7)));
    if matches!(st.shape, Shape::Pixel) || st.motion == Motion::Mechanical {
        let n = 12;
        let lit = (frac * n as f32).round() as usize;
        for i in 0..n {
            let ang = i as f32 / n as f32 * std::f32::consts::TAU;
            let (x, y) = (mid * ang.sin(), -mid * ang.cos());
            let s = width * 1.1;
            gfx2d::push();
            gfx2d::translate(x, y);
            gfx2d::rotate(ang);
            gfx2d::rect(-s / 2.0, -width / 2.0, s, width, f(if i < lit { color } else { track }));
            gfx2d::pop();
        }
        return;
    }
    if let Shadow::Hard(dx, dy) = st.shadow {
        gfx2d::push();
        gfx2d::translate(dx.clamp(-3.0, 3.0), dy.clamp(-3.0, 3.0));
        ring(0.0, 0.0, mid, width, f(th.shadow));
        gfx2d::pop();
    }
    ring(0.0, 0.0, mid, width, f(track));
    if frac > 0.0 {
        if st.shadow == Shadow::Glow {
            arc(0.0, 0.0, mid, width * 2.2, 0.0, frac * std::f32::consts::TAU, f(with_alpha(color, 0.25)));
        }
        arc(0.0, 0.0, mid, width, 0.0, frac * std::f32::consts::TAU, f(color));
    }
    if let Border::Line(bw) | Border::Double(bw) = st.border {
        ring(0.0, 0.0, rad, bw.min(2.0), f(th.panel_border));
    }
}

/// Which button Enter and a pad's A press: the one the game said to focus,
/// else the first made; the d-pad and arrows move it to the nearest one in
/// that direction. Keyed by any id the owner uses (a node's).
#[derive(Clone, Debug)]
pub struct Focus<K> {
    pub on: Option<K>,
    /// A key or pad has been used: show the ring.
    pub shown: bool,
}

impl<K> Default for Focus<K> {
    fn default() -> Self {
        Focus { on: None, shown: false }
    }
}

impl<K: Copy + PartialEq> Focus<K> {
    /// Keeps the focus on one of `buttons` (enabled ones, in made order,
    /// with their centres): the one it's on if it's still there, else the
    /// first.
    pub fn keep(&mut self, buttons: &[(K, f32, f32)]) {
        if !self.on.is_some_and(|k| buttons.iter().any(|b| b.0 == k)) {
            self.on = buttons.first().map(|b| b.0);
        }
    }

    /// Moves to the nearest button in direction (dx, dy) (each -1, 0 or 1).
    pub fn step(&mut self, buttons: &[(K, f32, f32)], dx: f32, dy: f32) {
        self.shown = true;
        let Some(cur) = self.on.and_then(|k| buttons.iter().find(|b| b.0 == k)) else {
            self.keep(buttons);
            return;
        };
        let (cx, cy) = (cur.1, cur.2);
        let best = buttons
            .iter()
            .filter(|b| b.0 != cur.0)
            .filter_map(|b| {
                let (ox, oy) = (b.1 - cx, b.2 - cy);
                let along = ox * dx + oy * dy;
                if along <= 1.0 {
                    return None;
                }
                let across = (ox * dy - oy * dx).abs();
                Some((b.0, along + across * 2.0))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((k, _)) = best {
            self.on = Some(k);
        }
    }
}

impl ButtonKind {
    pub fn named(s: &str) -> Option<ButtonKind> {
        Some(match s {
            "primary" => ButtonKind::Primary,
            "secondary" => ButtonKind::Secondary,
            "ghost" => ButtonKind::Ghost,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            ButtonKind::Primary => "primary",
            ButtonKind::Secondary => "secondary",
            ButtonKind::Ghost => "ghost",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Kind;

    fn ev(kind: Kind, x: f32, y: f32) -> Event {
        Event { kind: kind as u8, x, y, ..Default::default() }
    }

    #[test]
    fn a_small_button_still_takes_a_thumb_and_others_take_nothing() {
        let mut b = Widget::button("GO");
        let at = Rect::centered(100.0, 100.0, 24.0, 24.0);
        assert_eq!(b.press(&ev(Kind::MouseDown, 100.0 + 20.0, 100.0), at), Some(Tap::Pressed), "22 from the centre is inside 44");
        assert_eq!(b.press(&ev(Kind::MouseUp, 100.0, 100.0), at), Some(Tap::Clicked));
        let mut p = Widget::new(WidgetKind::Panel);
        assert_eq!(p.press(&ev(Kind::MouseDown, 100.0, 100.0), at), None);
    }

    #[test]
    fn focus_moves_to_the_nearest_button_that_way() {
        let bs = [(1, 100.0, 100.0), (2, 260.0, 100.0), (3, 100.0, 200.0), (4, 260.0, 210.0)];
        let mut f = Focus::default();
        f.keep(&bs);
        assert_eq!(f.on, Some(1));
        f.step(&bs, 1.0, 0.0);
        assert_eq!(f.on, Some(2));
        f.step(&bs, 0.0, 1.0);
        assert_eq!(f.on, Some(4));
        f.step(&bs, -1.0, 0.0);
        assert_eq!(f.on, Some(3));
        f.step(&bs, -1.0, 0.0);
        assert_eq!(f.on, Some(3), "nothing further left");
        f.keep(&bs[..2]);
        assert_eq!(f.on, Some(1), "its button went: back to the first");
        assert!(f.shown);
    }

    #[test]
    fn a_toned_button_draws_in_its_tone() {
        let th = Theme::preset("riso");
        let mut b = Widget::button("YES");
        assert_eq!(b.button_theme(&th), th, "plain: the identity's button");
        b.tone = Tone::Accent;
        assert_eq!(b.button_theme(&th), th, "a button's accent is already its own");
        for (tone, c) in [(Tone::Good, th.good), (Tone::Bad, th.bad), (Tone::Gold, th.gold)] {
            b.tone = tone;
            let t = b.button_theme(&th);
            assert_eq!((t.accent, t.secondary, t.text), (c, c, c), "{tone:?}");
            assert!(super::super::color::contrast(t.on_accent, c) >= 3.0 && super::super::color::contrast(t.on_secondary, c) >= 3.0, "{tone:?}: its label reads");
        }
    }

    #[test]
    fn meters_ease_to_their_value_and_a_mechanical_one_steps() {
        let th = Theme::preset("riso");
        let mut b = Widget::new(WidgetKind::Bar);
        b.value = 1.0;
        b.update(1.0 / 60.0, &th);
        assert!(b.shown() > 0.0 && b.shown() < 0.5);
        for _ in 0..120 {
            b.update(1.0 / 60.0, &th);
        }
        assert_eq!(b.shown(), 1.0);
        let mut m = Theme::preset("riso");
        m.style.motion = Motion::Mechanical;
        b.value = 0.3;
        b.update(1.0 / 60.0, &m);
        assert_eq!(b.shown(), 0.25);
    }
}
