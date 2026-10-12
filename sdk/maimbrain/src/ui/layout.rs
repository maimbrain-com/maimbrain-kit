//! Rectangles, safe areas and simple stacks.

use crate::sys::Screen;

/// Where a child sits inside a parent rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// (0, 0.5 or 1) fractions along x and y.
    fn frac(self) -> (f32, f32) {
        use Anchor::*;
        match self {
            TopLeft => (0.0, 0.0),
            Top => (0.5, 0.0),
            TopRight => (1.0, 0.0),
            Left => (0.0, 0.5),
            Center => (0.5, 0.5),
            Right => (1.0, 0.5),
            BottomLeft => (0.0, 1.0),
            Bottom => (0.5, 1.0),
            BottomRight => (1.0, 1.0),
        }
    }
}

/// An axis-aligned rectangle in logical units (x, y = top-left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn centered(cx: f32, cy: f32, w: f32, h: f32) -> Rect {
        Rect::new(cx - w / 2.0, cy - h / 2.0, w, h)
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    /// Shrunk by `d` on every side (negative grows).
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_xy(d, d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    /// Shrunk by different amounts per side.
    pub fn inset_sides(&self, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        Rect::new(self.x + left, self.y + top, (self.w - left - right).max(0.0), (self.h - top - bottom).max(0.0))
    }
    pub fn offset(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    /// Scaled about its centre.
    pub fn scaled(&self, s: f32) -> Rect {
        Rect::centered(self.cx(), self.cy(), self.w * s, self.h * s)
    }
    /// At least `w` × `h`, grown about its centre (e.g. a 44 pt touch target).
    pub fn at_least(&self, w: f32, h: f32) -> Rect {
        Rect::centered(self.cx(), self.cy(), self.w.max(w), self.h.max(h))
    }
    /// A `w` × `h` child placed at `anchor`, `margin` in from the edges.
    pub fn place(&self, anchor: Anchor, w: f32, h: f32, margin: f32) -> Rect {
        let (fx, fy) = anchor.frac();
        let x = self.x + margin + (self.w - 2.0 * margin - w) * fx;
        let y = self.y + margin + (self.h - 2.0 * margin - h) * fy;
        Rect::new(x, y, w, h)
    }
    /// The top `h` and the rest.
    pub fn split_top(&self, h: f32) -> (Rect, Rect) {
        let h = h.min(self.h);
        (Rect::new(self.x, self.y, self.w, h), Rect::new(self.x, self.y + h, self.w, self.h - h))
    }
    /// The bottom `h` and the rest.
    pub fn split_bottom(&self, h: f32) -> (Rect, Rect) {
        let h = h.min(self.h);
        (Rect::new(self.x, self.bottom() - h, self.w, h), Rect::new(self.x, self.y, self.w, self.h - h))
    }
    /// The left `w` and the rest.
    pub fn split_left(&self, w: f32) -> (Rect, Rect) {
        let w = w.min(self.w);
        (Rect::new(self.x, self.y, w, self.h), Rect::new(self.x + w, self.y, self.w - w, self.h))
    }
    /// `n` equal columns with `gap` between them.
    pub fn columns(&self, n: usize, gap: f32) -> Vec<Rect> {
        let n = n.max(1);
        let w = (self.w - gap * (n - 1) as f32) / n as f32;
        (0..n).map(|i| Rect::new(self.x + i as f32 * (w + gap), self.y, w, self.h)).collect()
    }
    /// `n` equal rows with `gap` between them.
    pub fn rows(&self, n: usize, gap: f32) -> Vec<Rect> {
        let n = n.max(1);
        let h = (self.h - gap * (n - 1) as f32) / n as f32;
        (0..n).map(|i| Rect::new(self.x, self.y + i as f32 * (h + gap), self.w, h)).collect()
    }
    pub fn lerp(&self, o: &Rect, t: f32) -> Rect {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Rect::new(l(self.x, o.x), l(self.y, o.y), l(self.w, o.w), l(self.h, o.h))
    }
    /// The part inside `o` too (`None` if they don't overlap).
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let (x0, y0) = (self.x.max(o.x), self.y.max(o.y));
        let (x1, y1) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

/// The platform's feed overlays on a card (SPEC §5.2): the title over the
/// bottom edge and the buttons down the right edge.
pub const CARD_BOTTOM: f32 = 70.0;
pub const CARD_RIGHT: f32 = 56.0;
/// The platform's after-round challenge strip (who you played against and
/// how it went), over a game-over card just above the title: the bottom of
/// `Layout::card` that `Layout::results` leaves out.
pub const CARD_STRIP: f32 = 96.0;
/// The pause pill plus the mic badge beside it, top-left in play.
pub const PILL_ZONE: (f32, f32) = (110.0, 50.0);
/// Smallest comfortable touch target (Apple HIG), in logical units.
pub const MIN_TOUCH: f32 = 44.0;

/// Screen regions every game needs, from `sys::screen()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The whole logical canvas.
    pub screen: Rect,
    /// Inside the safe-area insets.
    pub safe: Rect,
    /// Where title cards keep buttons and key text (game-over cards use
    /// `results`): the safe area minus the card overlays, symmetric so
    /// centred things stay centred (CARD_RIGHT off both sides, CARD_BOTTOM
    /// off the bottom).
    pub card: Rect,
    /// The same on a game-over card, which also keeps the challenge strip's
    /// band clear: `card` less its bottom CARD_STRIP (`strip()`).
    pub results: Rect,
    /// The play HUD's area: the safe area (the pause pill lives in the top
    /// inset; keep the top-left `pill_zone()` clear of anything tappable).
    pub hud: Rect,
}

impl Layout {
    /// Reads `sys::screen()`: once in init, and again on an
    /// `input::Kind::Resize` (an `any`, `landscape` or `fill` game's screen
    /// changed; a portrait letterboxed game's never does).
    pub fn new() -> Layout {
        Layout::from_screen(&crate::sys::screen())
    }

    /// For tests and tools: any screen. A zero-size screen (the native SDK
    /// stubs) is treated as 360 × 640 with no insets.
    pub fn from_screen(s: &Screen) -> Layout {
        let (w, h) = if s.width > 0.0 && s.height > 0.0 { (s.width, s.height) } else { (360.0, 640.0) };
        let screen = Rect::new(0.0, 0.0, w, h);
        let safe = screen.inset_sides(s.inset_left, s.inset_top, s.inset_right, s.inset_bottom);
        let side = CARD_RIGHT.max(s.inset_left).max(s.inset_right);
        let card = Rect::new(side, safe.y, (w - 2.0 * side).max(0.0), (h - s.inset_bottom.max(CARD_BOTTOM) - 8.0 - safe.y).max(0.0));
        let results = card.split_bottom(CARD_STRIP).1;
        Layout { screen, safe, card, results, hud: safe }
    }

    /// Where the platform draws its challenge strip after a round, over a
    /// game-over card: the band between `results` and the title.
    pub fn strip(&self) -> Rect {
        self.card.split_bottom(CARD_STRIP).0
    }

    /// The top-left corner the platform's pause pill and mic badge use in play.
    pub fn pill_zone(&self) -> Rect {
        Rect::new(0.0, 0.0, PILL_ZONE.0, self.safe.y.max(PILL_ZONE.1))
    }

    pub fn w(&self) -> f32 {
        self.screen.w
    }
    pub fn h(&self) -> f32 {
        self.screen.h
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout::from_screen(&Screen::default())
    }
}

/// Hands out rects top to bottom inside an area: a vertical stack.
/// ```
/// use maimbrain::ui::{Column, Rect};
/// let mut col = Column::new(Rect::new(0.0, 100.0, 360.0, 400.0), 12.0);
/// let title = col.next(60.0);
/// let button = col.next_centered(200.0, 56.0);
/// assert_eq!(button.y, 172.0);
/// assert_eq!(button.x, 80.0);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub area: Rect,
    pub gap: f32,
    pub y: f32,
}

impl Column {
    pub fn new(area: Rect, gap: f32) -> Column {
        Column { area, gap, y: area.y }
    }
    /// The next full-width row, `h` tall.
    pub fn next(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.area.x, self.y, self.area.w, h);
        self.y += h + self.gap;
        r
    }
    /// The next row, `w` wide, centred.
    pub fn next_centered(&mut self, w: f32, h: f32) -> Rect {
        let r = self.next(h);
        Rect::centered(r.cx(), r.cy(), w, h)
    }
    pub fn space(&mut self, h: f32) {
        self.y += h;
    }
    /// What's left below.
    pub fn rest(&self) -> Rect {
        Rect::new(self.area.x, self.y, self.area.w, (self.area.bottom() - self.y).max(0.0))
    }
}

/// Rects for items of the given heights stacked with `gap`, the whole
/// stack placed in `area` by `anchor` (Center centres it both ways). Each
/// item is `width` wide (or the area's width if `width` is 0).
pub fn stack_v(area: Rect, heights: &[f32], gap: f32, width: f32, anchor: Anchor) -> Vec<Rect> {
    let total: f32 = heights.iter().sum::<f32>() + gap * heights.len().saturating_sub(1) as f32;
    let w = if width > 0.0 { width } else { area.w };
    let block = area.place(anchor, w, total, 0.0);
    let mut y = block.y;
    heights
        .iter()
        .map(|&h| {
            let r = Rect::new(block.x, y, w, h);
            y += h + gap;
            r
        })
        .collect()
}

/// Rects for items of the given widths side by side with `gap`, centred in `area`.
pub fn stack_h(area: Rect, widths: &[f32], gap: f32) -> Vec<Rect> {
    let total: f32 = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    let mut x = area.cx() - total / 2.0;
    widths
        .iter()
        .map(|&w| {
            let r = Rect::new(x, area.y, w, area.h);
            x += w + gap;
            r
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_helpers() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!((r.cx(), r.cy(), r.right(), r.bottom()), (60.0, 45.0, 110.0, 70.0));
        assert!(r.contains(10.0, 70.0) && !r.contains(9.0, 30.0));
        assert_eq!(r.inset(5.0), Rect::new(15.0, 25.0, 90.0, 40.0));
        assert_eq!(Rect::new(0.0, 0.0, 20.0, 20.0).at_least(44.0, 44.0), Rect::new(-12.0, -12.0, 44.0, 44.0));
        assert_eq!(r.place(Anchor::BottomRight, 10.0, 10.0, 5.0), Rect::new(95.0, 55.0, 10.0, 10.0));
        assert_eq!(r.place(Anchor::Center, 20.0, 10.0, 0.0), Rect::new(50.0, 40.0, 20.0, 10.0));
        let (top, rest) = r.split_top(10.0);
        assert_eq!((top.h, rest.y, rest.h), (10.0, 30.0, 40.0));
        let cols = r.columns(2, 10.0);
        assert_eq!((cols[0].w, cols[1].x), (45.0, 65.0));
    }

    #[test]
    fn layout_keeps_cards_clear_of_overlays() {
        let s = Screen { width: 360.0, height: 640.0, inset_top: 59.0, inset_bottom: 34.0, ..Default::default() };
        let l = Layout::from_screen(&s);
        assert_eq!(l.safe, Rect::new(0.0, 59.0, 360.0, 547.0));
        assert_eq!(l.card.x, 56.0);
        assert_eq!(l.card.right(), 304.0);
        assert!(l.card.bottom() <= 640.0 - 70.0);
        // A game-over card stops above the challenge strip, which ends where the card does.
        assert_eq!((l.results.x, l.results.y, l.results.w), (l.card.x, l.card.y, l.card.w));
        assert_eq!(l.results.bottom(), l.card.bottom() - CARD_STRIP);
        assert_eq!(l.strip(), Rect::new(56.0, l.results.bottom(), 248.0, CARD_STRIP));
        assert_eq!(l.pill_zone().h, 59.0);
        let stub = Layout::default();
        assert_eq!(stub.screen.w, 360.0);
    }

    #[test]
    fn stacks_center() {
        let v = stack_v(Rect::new(0.0, 0.0, 100.0, 100.0), &[20.0, 20.0], 10.0, 50.0, Anchor::Center);
        assert_eq!(v[0], Rect::new(25.0, 25.0, 50.0, 20.0));
        assert_eq!(v[1].y, 55.0);
        let h = stack_h(Rect::new(0.0, 0.0, 100.0, 10.0), &[20.0, 20.0], 10.0);
        assert_eq!((h[0].x, h[1].x), (25.0, 55.0));
    }
}
