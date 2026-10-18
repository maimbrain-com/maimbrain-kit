//! Touch, mouse, keyboard and controller input, turned into one frame's
//! [`FrameInput`] for the behaviours and the game's handlers.
//!
//! Every way to play maps onto the same few actions, so nothing exists only
//! on a controller (games-touch-parity): a finger (or the mouse) points,
//! drags and taps; the arrows, WASD, the d-pad and the left stick are one
//! direction; Space, Up, W, Z, J and the face and shoulder buttons are one
//! "jump" (what a tap does for flappy and runner games). The title and
//! game-over cards start on Enter, Space or A through the SDK's `Rounds`.

use maimbrain::input::{Event, Kind, key, pad};

use crate::vm::FrameInput;

/// Seconds and art pixels a quick drag must take to count as a swipe.
const SWIPE_SECS: f64 = 0.35;
const SWIPE_MIN: f32 = 16.0;

#[derive(Default)]
pub struct Mapper {
    frame: FrameInput,
    /// The pointer being followed (touch id, or 255 for the mouse).
    pointer: Option<u8>,
    down_at: (f32, f32, f64),
    keys: Keys,
    stick: (f32, f32),
    tilt: Option<f32>,
}

#[derive(Default)]
struct Keys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    jump: u32,
}

/// The action a key or a button stands for.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Left,
    Right,
    Up,
    Down,
    /// Jump, and also "a" for `on("press", "a", fn)`.
    Jump(&'static str),
}

fn key_action(code: u32) -> Option<Action> {
    Some(match code {
        key::LEFT | key::A => Action::Left,
        key::RIGHT | key::D => Action::Right,
        key::DOWN | key::S => Action::Down,
        key::UP | key::W => Action::Up,
        key::SPACE | key::Z | key::J => Action::Jump("a"),
        key::X | key::K => Action::Jump("b"),
        _ => return None,
    })
}

fn pad_action(button: u32) -> Option<Action> {
    Some(match button {
        pad::LEFT => Action::Left,
        pad::RIGHT => Action::Right,
        pad::UP => Action::Up,
        pad::DOWN => Action::Down,
        pad::A => Action::Jump("a"),
        pad::B => Action::Jump("b"),
        pad::X => Action::Jump("x"),
        pad::Y => Action::Jump("y"),
        pad::LB => Action::Jump("lb"),
        pad::RB => Action::Jump("rb"),
        _ => return None,
    })
}

fn name(a: Action) -> &'static str {
    match a {
        Action::Left => "left",
        Action::Right => "right",
        Action::Up => "up",
        Action::Down => "down",
        Action::Jump(n) => n,
    }
}

impl Mapper {
    /// A new round: nothing held.
    pub fn reset(&mut self) {
        *self = Mapper { tilt: self.tilt, ..Mapper::default() };
    }

    /// The phone's sideways tilt this frame (the sensors build).
    pub fn tilt(&mut self, t: Option<f32>) {
        self.tilt = t;
    }

    /// One event the round flow passed on; `(ox, oy, scale)` maps the
    /// screen to the canvas.
    pub fn feed(&mut self, e: &Event, (ox, oy, s): (f32, f32, f32)) {
        let (x, y) = ((e.x - ox) / s, (e.y - oy) / s);
        let held = self.held(e);
        let f = &mut self.frame;
        match e.kind() {
            Kind::TouchDown | Kind::MouseDown if self.pointer.is_none() => {
                if e.kind() == Kind::MouseDown && e.id != 0 {
                    return;
                }
                self.pointer = Some(if e.kind() == Kind::MouseDown { 255 } else { e.id });
                self.down_at = (x, y, e.time);
                if !f.tap {
                    (f.tap, f.tx, f.ty) = (true, x, y);
                }
                (f.down, f.x, f.y) = (true, x, y);
            }
            Kind::TouchMove | Kind::MouseMove if held => (f.x, f.y) = (x, y),
            Kind::TouchUp | Kind::TouchCancel | Kind::MouseUp if held => {
                self.pointer = None;
                (f.release, f.rx, f.ry, f.x, f.y) = (true, x, y, x, y);
                let (dx, dy) = (x - self.down_at.0, y - self.down_at.1);
                if e.time - self.down_at.2 <= SWIPE_SECS && dx.hypot(dy) >= SWIPE_MIN {
                    f.swipe = Some((dx, dy));
                }
            }
            Kind::KeyDown => {
                if let Some(a) = key_action(e.code) {
                    self.act(a, true);
                }
            }
            Kind::KeyUp => {
                if let Some(a) = key_action(e.code) {
                    self.act(a, false);
                }
            }
            Kind::PadButton => {
                if let Some(a) = pad_action(e.code) {
                    self.act(a, e.a > 0.5);
                }
            }
            Kind::PadAxis if e.code == pad::LX => self.stick.0 = e.a,
            Kind::PadAxis if e.code == pad::LY => self.stick.1 = e.a,
            _ => {}
        }
    }

    fn held(&self, e: &Event) -> bool {
        match (self.pointer, e.kind()) {
            (Some(255), Kind::MouseMove | Kind::MouseUp) => true,
            (Some(id), Kind::TouchMove | Kind::TouchUp | Kind::TouchCancel) => id == e.id && id != 255,
            _ => false,
        }
    }

    fn act(&mut self, a: Action, down: bool) {
        let k = &mut self.keys;
        let was = match a {
            Action::Left => std::mem::replace(&mut k.left, down),
            Action::Right => std::mem::replace(&mut k.right, down),
            Action::Up => std::mem::replace(&mut k.up, down),
            Action::Down => std::mem::replace(&mut k.down, down),
            Action::Jump(_) => {
                let was = k.jump > 0;
                k.jump = if down { k.jump + 1 } else { k.jump.saturating_sub(1) };
                was && down
            }
        };
        if down && !was {
            self.frame.press.push(name(a));
            // Up is a jump too, for the games that jump.
            if matches!(a, Action::Jump(_) | Action::Up) {
                self.frame.jump = true;
            }
        }
        if !down && (matches!(a, Action::Jump(_)) && self.keys.jump == 0 || a == Action::Up) {
            self.frame.jumpup = true;
        }
    }

    /// This frame's input; the next frame starts empty but for what's held.
    pub fn frame(&mut self) -> FrameInput {
        let k = &self.keys;
        let axis = |neg: bool, pos: bool| (pos as i32 - neg as i32) as f32;
        let (sx, sy) = self.stick;
        self.frame.ax = if sx.abs() > 0.25 { sx } else { axis(k.left, k.right) };
        self.frame.ay = if sy.abs() > 0.25 { sy } else { axis(k.up, k.down) };
        self.frame.tilt = self.tilt;
        let held = self.pointer.is_some();
        let (x, y) = (self.frame.x, self.frame.y);
        let out = std::mem::take(&mut self.frame);
        self.frame.down = held;
        (self.frame.x, self.frame.y) = (x, y);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: Kind, code: u32, a: f32) -> Event {
        Event { kind: kind as u8, code, a, ..Event::default() }
    }

    #[test]
    fn keys_and_pads_move_and_jump_like_a_finger() {
        let mut m = Mapper::default();
        let tf = (0.0, 0.0, 2.5);
        m.feed(&ev(Kind::KeyDown, key::RIGHT, 0.0), tf);
        m.feed(&ev(Kind::KeyDown, key::SPACE, 0.0), tf);
        let f = m.frame();
        assert_eq!((f.ax, f.jump, &f.press[..]), (1.0, true, &["right", "a"][..]));
        // Held: still moving, no new jump.
        let f = m.frame();
        assert_eq!((f.ax, f.jump), (1.0, false));
        m.feed(&ev(Kind::KeyUp, key::SPACE, 0.0), tf);
        assert!(m.frame().jumpup);
        // The left stick beats the d-pad; A jumps; B is "b" and jumps too.
        m.feed(&ev(Kind::PadAxis, pad::LX, -0.8), tf);
        m.feed(&ev(Kind::PadButton, pad::B, 1.0), tf);
        let f = m.frame();
        assert_eq!((f.ax, f.jump, &f.press[..]), (-0.8, true, &["b"][..]));
    }

    #[test]
    fn a_quick_drag_is_a_swipe_in_canvas_pixels() {
        let mut m = Mapper::default();
        let tf = (0.0, 0.0, 2.5);
        let t = |k: Kind, x: f32, y: f32, time: f64| Event { kind: k as u8, x, y, time, ..Event::default() };
        m.feed(&t(Kind::TouchDown, 100.0, 300.0, 1.0), tf);
        let f = m.frame();
        assert!(f.tap && f.down && (f.tx, f.ty) == (40.0, 120.0));
        m.feed(&t(Kind::TouchMove, 200.0, 300.0, 1.1), tf);
        m.feed(&t(Kind::TouchUp, 200.0, 300.0, 1.15), tf);
        let f = m.frame();
        assert!(f.release && !m.frame().down);
        assert_eq!(f.swipe, Some((40.0, 0.0)));
    }
}
