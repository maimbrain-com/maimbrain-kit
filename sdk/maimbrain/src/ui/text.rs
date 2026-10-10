//! Styled text: alignment, outlines, shadows, glows, wrapping, tracking and
//! measuring, for the host fonts (SPEC §5.3).
//!
//! ```ignore
//! use maimbrain::ui::{text, Align};
//! text("STACK").size(64.0).color(0xffffffff).outline(0.08, 0x1b1030ff)
//!     .shadow(0.0, 4.0, 0x00000080).align(Align::Center).draw(180.0, 120.0);
//! ```
//!
//! Inter (`Font::Sans`, `Font::SansBold`) and game fonts baked as distance
//! fields (`Font::asset`, most library fonts) are crisp at any size, with
//! outlines, glows and soft shadows done by the host. Bitmap fonts (the
//! pixel font, library pixel fonts) get outlines and shadows by drawing
//! offset copies, a whole art pixel (`size / pixel_em`) apart, so keep their
//! size a multiple of `font.pixel_em()` (8 for `Font::Pixel`; `theme.fit`).

use crate::gfx2d::{self, Font, TextStyle};

/// Horizontal alignment of each line relative to the x you draw at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// What the y you draw at means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VAlign {
    /// The top of the line box (what `gfx2d::text` takes).
    #[default]
    Top,
    /// The middle of the capital letters: centres text in buttons and pills.
    Middle,
    /// The baseline of the first line.
    Baseline,
    /// The bottom of the last line box.
    Bottom,
}

/// Vertical metrics of a host font, in ems (from the baked fonts).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Line top to baseline.
    pub ascent: f32,
    /// Height of capital letters above the baseline.
    pub cap: f32,
    /// Distance between baselines.
    pub line: f32,
}

pub fn metrics(font: Font) -> Metrics {
    let m = font.metrics();
    Metrics { ascent: m.ascent, cap: m.cap, line: m.line }
}

/// A piece of text with its look. Build with [`text`], then `draw`.
#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    pub s: &'a str,
    pub font: Font,
    pub size: f32,
    pub color: u32,
    pub align: Align,
    pub valign: VAlign,
    /// Outline width in ems and its color (0 = none).
    pub outline: (f32, u32),
    /// Shadow offset (logical units), blur in ems, color (alpha 0 = none).
    pub shadow: (f32, f32, f32, u32),
    /// Glow blur in ems and color (alpha 0 = none).
    pub glow: (f32, u32),
    /// Thicker (> 0) or thinner (< 0) letters, in ems (SDF fonts only).
    pub weight: f32,
    /// Extra space between letters, in ems.
    pub tracking: f32,
    /// Wrap lines to this width (0 = no wrapping; `\n` always breaks).
    pub wrap: f32,
    /// Line spacing multiplier.
    pub leading: f32,
}

/// Starts styling `s`: Inter Bold, 24 units, white, left/top.
pub fn text(s: &str) -> Text<'_> {
    Text {
        s,
        font: Font::SansBold,
        size: 24.0,
        color: 0xffffffff,
        align: Align::Left,
        valign: VAlign::Top,
        outline: (0.0, 0),
        shadow: (0.0, 0.0, 0.0, 0),
        glow: (0.0, 0),
        weight: 0.0,
        tracking: 0.0,
        wrap: 0.0,
        leading: 1.0,
    }
}

impl<'a> Text<'a> {
    pub fn font(mut self, f: Font) -> Self {
        self.font = f;
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn color(mut self, c: u32) -> Self {
        self.color = c;
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn center(self) -> Self {
        self.align(Align::Center)
    }
    pub fn right(self) -> Self {
        self.align(Align::Right)
    }
    pub fn valign(mut self, v: VAlign) -> Self {
        self.valign = v;
        self
    }
    /// Centred on (x, y) both ways (capitals centred vertically). It sets
    /// the horizontal alignment too, replacing an earlier `.right()`: for
    /// right-aligned text centred vertically, use `.right().vcenter()`.
    pub fn middle(self) -> Self {
        self.align(Align::Center).valign(VAlign::Middle)
    }
    /// Centred vertically on y (the capitals), keeping the horizontal alignment.
    pub fn vcenter(self) -> Self {
        self.valign(VAlign::Middle)
    }
    /// An outline `width` ems wide (0.05–0.1 reads well).
    pub fn outline(mut self, width: f32, color: u32) -> Self {
        self.outline = (width, color);
        self
    }
    /// A hard drop shadow offset by (dx, dy) units.
    pub fn shadow(mut self, dx: f32, dy: f32, color: u32) -> Self {
        self.shadow = (dx, dy, 0.0, color);
        self
    }
    /// A blurred drop shadow (`blur` in ems, up to ~0.1).
    pub fn soft_shadow(mut self, dx: f32, dy: f32, blur: f32, color: u32) -> Self {
        self.shadow = (dx, dy, blur, color);
        self
    }
    /// A soft halo (`blur` in ems, up to ~0.1).
    pub fn glow(mut self, blur: f32, color: u32) -> Self {
        self.glow = (blur, color);
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = w;
        self
    }
    pub fn tracking(mut self, em: f32) -> Self {
        self.tracking = em;
        self
    }
    pub fn wrap(mut self, width: f32) -> Self {
        self.wrap = width;
        self
    }
    pub fn leading(mut self, k: f32) -> Self {
        self.leading = k;
        self
    }
    /// Multiplies every color's alpha (fading the whole thing).
    pub fn alpha(mut self, a: f32) -> Self {
        let f = |c: u32| super::color::fade(c, a);
        self.color = f(self.color);
        self.outline.1 = f(self.outline.1);
        self.shadow.3 = f(self.shadow.3);
        self.glow.1 = f(self.glow.1);
        self
    }

    fn line_h(&self) -> f32 {
        metrics(self.font).line * self.size * self.leading
    }

    fn line_w(&self, line: &str) -> f32 {
        let w = gfx2d::measure(self.font, self.size, line);
        if self.tracking != 0.0 {
            let n = line.chars().count();
            w + self.tracking * self.size * n.saturating_sub(1) as f32
        } else {
            w
        }
    }

    /// The lines after `\n` and wrapping.
    pub fn lines(&self) -> Vec<&'a str> {
        let mut out = Vec::new();
        for para in self.s.split('\n') {
            if self.wrap <= 0.0 {
                out.push(para);
                continue;
            }
            let mut start = 0;
            let mut last_fit = None;
            let mut i = 0;
            let bytes = para.as_bytes();
            while i <= para.len() {
                let at_break = i == para.len() || bytes[i] == b' ';
                if at_break {
                    if self.line_w(&para[start..i]) <= self.wrap || last_fit.is_none() {
                        last_fit = Some(i);
                    } else {
                        let end = last_fit.unwrap();
                        out.push(&para[start..end]);
                        start = (end + 1).min(para.len());
                        last_fit = None;
                        // Re-test this word on its new line.
                        continue;
                    }
                }
                i += 1;
            }
            out.push(&para[start..]);
        }
        out
    }

    /// Width (the widest line) and height of the block.
    pub fn measure(&self) -> (f32, f32) {
        let lines = self.lines();
        let w = lines.iter().map(|l| self.line_w(l)).fold(0.0, f32::max);
        let m = metrics(self.font);
        let h = self.line_h() * (lines.len().max(1) - 1) as f32 + m.line * self.size;
        (w, h)
    }

    /// Draws with (x, y) interpreted by `align` and `valign`. Returns the
    /// block's bounds.
    pub fn draw(&self, x: f32, y: f32) -> super::Rect {
        let lines = self.lines();
        let m = metrics(self.font);
        let total_h = self.line_h() * (lines.len().max(1) - 1) as f32 + m.line * self.size;
        let top = match self.valign {
            VAlign::Top => y,
            VAlign::Middle => y - (m.ascent - m.cap * 0.5) * self.size,
            VAlign::Baseline => y - m.ascent * self.size,
            VAlign::Bottom => y - total_h,
        };
        let mut bounds = super::Rect::new(x, top, 0.0, total_h);
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for (i, line) in lines.iter().enumerate() {
            let w = self.line_w(line);
            let lx = match self.align {
                Align::Left => x,
                Align::Center => x - w / 2.0,
                Align::Right => x - w,
            };
            min_x = min_x.min(lx);
            max_x = max_x.max(lx + w);
            self.draw_line(line, lx, top + i as f32 * self.line_h());
        }
        if min_x.is_finite() {
            bounds.x = min_x;
            bounds.w = max_x - min_x;
        }
        bounds
    }

    fn draw_line(&self, line: &str, x: f32, y: f32) {
        if line.is_empty() {
            return;
        }
        let pixel_em = self.font.pixel_em();
        let (sdx, sdy, sblur, scol) = self.shadow;
        let (ow, ocol) = self.outline;
        if pixel_em > 0.0 {
            let px = self.size / pixel_em;
            let o = if ow > 0.0 && ocol & 0xff != 0 { (ow * pixel_em).round().max(1.0) * px } else { 0.0 };
            if scol & 0xff != 0 {
                self.raw(line, x + sdx, y + sdy, scol, o, scol);
            }
            self.raw(line, x, y, self.color, o, ocol);
            return;
        }
        let (gblur, gcol) = self.glow;
        if gcol & 0xff != 0 {
            gfx2d::text_style(TextStyle { weight: self.weight + ow, soft: gblur.max(0.02), ..Default::default() });
            self.raw(line, x, y, gcol, 0.0, 0);
        }
        if scol & 0xff != 0 {
            gfx2d::text_style(TextStyle { weight: self.weight + ow, soft: sblur, ..Default::default() });
            self.raw(line, x + sdx, y + sdy, scol, 0.0, 0);
        }
        let styled = ow > 0.0 || self.weight != 0.0;
        if styled {
            gfx2d::text_style(TextStyle { weight: self.weight, outline: ow, outline_rgba: ocol, soft: 0.0 });
        } else if gcol & 0xff != 0 || scol & 0xff != 0 {
            gfx2d::text_style(TextStyle::default());
        }
        self.raw(line, x, y, self.color, 0.0, 0);
        if styled || gcol & 0xff != 0 || scol & 0xff != 0 {
            gfx2d::text_style(TextStyle::default());
        }
    }

    /// One line in one color; for the pixel font, `o` > 0 draws an outline
    /// of offset copies first.
    fn raw(&self, line: &str, x: f32, y: f32, color: u32, o: f32, ocol: u32) {
        let draw = |dx: f32, dy: f32, c: u32| {
            if self.tracking == 0.0 {
                gfx2d::text(self.font, self.size, x + dx, y + dy, c, line);
            } else {
                let mut cx = x + dx;
                let mut buf = [0u8; 4];
                for ch in line.chars() {
                    let s = ch.encode_utf8(&mut buf);
                    gfx2d::text(self.font, self.size, cx, y + dy, c, s);
                    cx += gfx2d::measure(self.font, self.size, s) + self.tracking * self.size;
                }
            }
        };
        if o > 0.0 {
            for (dx, dy) in [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
                draw(dx * o, dy * o, ocol);
            }
        }
        draw(0.0, 0.0, color);
    }
}

/// `12345` → `"12,345"` (negative numbers keep their sign).
pub fn fmt_int(n: i64) -> String {
    fmt_int_sep(n, ',')
}

/// Thousands separated by `sep`.
pub fn fmt_int_sep(n: i64, sep: char) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vcenter_keeps_the_alignment_and_middle_replaces_it() {
        let t = text("9").right().vcenter();
        assert_eq!((t.align, t.valign), (Align::Right, VAlign::Middle));
        let t = text("9").right().middle();
        assert_eq!((t.align, t.valign), (Align::Center, VAlign::Middle));
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(999), "999");
        assert_eq!(fmt_int(1000), "1,000");
        assert_eq!(fmt_int(-1234567), "-1,234,567");
    }

    #[test]
    fn newlines_split_and_unwrapped_text_stays_whole() {
        // Natively, measure() is a stub returning 0, so wrapping never breaks a line.
        let t = text("ONE TWO\nTHREE").wrap(10.0);
        assert_eq!(t.lines(), vec!["ONE TWO", "THREE"]);
        let (_, h) = t.measure();
        assert!((h - 24.0 * 1.2099 * 2.0).abs() < 1e-3);
    }
}
