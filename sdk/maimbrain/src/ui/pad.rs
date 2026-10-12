//! Controller button prompts: a button drawn the way the player's controller
//! prints it. Library fonts don't have ✕ ○ □ △, so PlayStation's face buttons
//! are drawn as shapes; every other label is text.

use super::shape;
use crate::input::{PadStyle, pad};

/// The colors PlayStation prints its face buttons in, and Xbox its letters in.
fn face_color(style: PadStyle, button: u32) -> Option<u32> {
    match (style, button) {
        (PadStyle::PlayStation, pad::A) => Some(0x7fa8f0ff),
        (PadStyle::PlayStation, pad::B) => Some(0xf0727aff),
        (PadStyle::PlayStation, pad::X) => Some(0xe79ad0ff),
        (PadStyle::PlayStation, pad::Y) => Some(0x4ccfb0ff),
        (PadStyle::Xbox, pad::A) => Some(0x6fd24aff),
        (PadStyle::Xbox, pad::B) => Some(0xe8394dff),
        (PadStyle::Xbox, pad::X) => Some(0x3a8cffff),
        (PadStyle::Xbox, pad::Y) => Some(0xf2c230ff),
        _ => None,
    }
}

/// How wide [`pad_button`] draws `button`'s badge at `size`: `size` for a
/// one-mark button (✕, A, a d-pad arrow), wider for a longer label ("RB",
/// "R1", "MENU"). Lay out the words beside a badge from this, not from `size`.
pub fn pad_button_width(style: PadStyle, button: u32, size: f32) -> f32 {
    let chars = style.label(button).chars().count();
    if chars > 1 && !matches!(button, pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT) { size * (0.5 + 0.32 * chars as f32) } else { size }
}

/// A button prompt centred at (cx, cy), `size` tall: a dark round badge
/// with the button's mark as `style` prints it (✕ ○ □ △ drawn as shapes on a
/// PlayStation controller, "A" / "B" / "LB" / "MENU" … as text otherwise, in
/// the controller's own colors for face buttons). One-mark buttons are `size`
/// across; shoulder and menu buttons get a wider pill ([`pad_button_width`]).
/// Uses the default font for text; draw your own from
/// [`PadStyle::label`] or [`PadStyle::word`] to match a game font.
/// ```ignore
/// ui::pad_button(self.style, pad::A, x, y, 18.0); // "press ✕ to jump"
/// ```
pub fn pad_button(style: PadStyle, button: u32, cx: f32, cy: f32, size: f32) {
    let r = size * 0.5;
    let label = style.label(button);
    let w = pad_button_width(style, button, size);
    let wide = w > size;
    let col = face_color(style, button).unwrap_or(0xf2f2f2ff);
    crate::gfx2d::rrect(cx - w * 0.5, cy - r, w, size, r, 0.0, 0.0, 0x15151cff, 0x15151cff);
    crate::gfx2d::rrect(cx - w * 0.5, cy - r, w, size, r, (size * 0.08).max(1.0), 0.0, 0x3a3a48ff, 0x3a3a48ff);
    let s = size * 0.24; // the mark's half-size
    let lw = (size * 0.11).max(1.0);
    match (style, button) {
        (PadStyle::PlayStation, pad::A) => {
            shape::capsule(cx - s, cy - s, cx + s, cy + s, lw, col);
            shape::capsule(cx + s, cy - s, cx - s, cy + s, lw, col);
        }
        (PadStyle::PlayStation, pad::B) => shape::ring(cx, cy, s * 1.1, lw, col),
        (PadStyle::PlayStation, pad::X) => {
            crate::gfx2d::rrect(cx - s, cy - s, 2.0 * s, 2.0 * s, lw * 0.3, lw, 0.0, col, col);
        }
        (PadStyle::PlayStation, pad::Y) => {
            let h = s * 1.15;
            shape::polyline(&[cx, cy - h, cx + h, cy + h * 0.75, cx - h, cy + h * 0.75, cx, cy - h], lw, col);
        }
        _ => {
            super::text(label).size(size * if wide { 0.45 } else { 0.58 }).color(col).middle().draw(cx, cy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_badge_says_how_wide_it_draws() {
        assert_eq!(pad_button_width(PadStyle::PlayStation, pad::A, 20.0), 20.0);
        assert_eq!(pad_button_width(PadStyle::Xbox, pad::DOWN, 20.0), 20.0);
        // "R1" and "RB": two characters, a pill wider than `size`.
        let w = pad_button_width(PadStyle::PlayStation, pad::RB, 20.0);
        assert!((w - 22.8).abs() < 1e-4, "{w}");
        assert_eq!(pad_button_width(PadStyle::Xbox, pad::RB, 20.0), w);
    }
}
