//! The platform's input as LÖVE delivers it (docs/STUDIO-2.md §5 [love],
//! [input]): `keypressed` with LÖVE's key names, `mousepressed` and the
//! rest, and what's held for `love.keyboard.isDown`, `love.mouse.isDown`,
//! `love.touch.getTouches` and `love.joystick`.
//!
//! As on LÖVE's phones, a touch is also the mouse: the first finger down
//! sends `touchpressed` and `mousepressed(x, y, 1, true, 1)`, and moves and
//! lifts the same way. Positions are in the canvas's logical units.
//! Gamepad `start` and `back` are the platform's (pause, menu): never sent.

use maimbrain::input::{Event, Kind, pad};

/// One LÖVE callback with its arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum LoveEvent {
    KeyPressed(&'static str),
    KeyReleased(&'static str),
    MousePressed { x: f32, y: f32, button: u8, istouch: bool },
    MouseReleased { x: f32, y: f32, button: u8, istouch: bool },
    MouseMoved { x: f32, y: f32, dx: f32, dy: f32, istouch: bool },
    TouchPressed { id: u8, x: f32, y: f32, pressure: f32 },
    TouchMoved { id: u8, x: f32, y: f32, dx: f32, dy: f32, pressure: f32 },
    TouchReleased { id: u8, x: f32, y: f32 },
    GamepadPressed(&'static str),
    GamepadReleased(&'static str),
    GamepadAxis(&'static str, f32),
    WheelMoved(f32, f32),
}

/// What's held now.
#[derive(Clone, Debug, Default)]
pub struct Held {
    keys: Vec<&'static str>,
    /// Mouse buttons 1–3 (a primary touch is button 1).
    pub mouse: [bool; 3],
    pub mx: f32,
    pub my: f32,
    /// Fingers down: id, x, y.
    pub touches: Vec<(u8, f32, f32)>,
    buttons: Vec<&'static str>,
    axes: [f32; 6],
    /// A controller has been used (love.joystick lists it from then on).
    pub pad: bool,
}

impl Held {
    /// `input.ax`, `input.ay` (§5 [input]): one steering axis from the
    /// arrows, WASD, the d-pad and the left stick (the stick when it's
    /// pushed past a quarter); `ay` is -1 for up.
    pub fn steer(&self) -> (f32, f32) {
        let axis = |neg: bool, pos: bool| (pos as i32 - neg as i32) as f32;
        let (sx, sy) = (self.axes[0], self.axes[1]);
        let kx = axis(self.key("left") || self.key("a") || self.button("dpleft"), self.key("right") || self.key("d") || self.button("dpright"));
        let ky = axis(self.key("up") || self.key("w") || self.button("dpup"), self.key("down") || self.key("s") || self.button("dpdown"));
        (if sx.abs() > 0.25 { sx } else { kx }, if sy.abs() > 0.25 { sy } else { ky })
    }

    pub fn key(&self, name: &str) -> bool {
        self.keys.contains(&name)
    }
    pub fn button(&self, name: &str) -> bool {
        self.buttons.contains(&name)
    }
    pub fn axis(&self, name: &str) -> f32 {
        AXES.iter().position(|a| *a == name).map_or(0.0, |i| self.axes[i])
    }
}

/// LÖVE's name for a key code (the SDK's HID usage codes), or None.
pub fn key_name(code: u32) -> Option<&'static str> {
    const LETTERS: [&str; 26] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z"];
    const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    const F: [&str; 12] = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];
    Some(match code {
        4..=29 => LETTERS[(code - 4) as usize],
        30..=39 => DIGITS[(code - 30) as usize],
        40 => "return",
        41 => "escape",
        42 => "backspace",
        43 => "tab",
        44 => "space",
        45 => "-",
        46 => "=",
        47 => "[",
        48 => "]",
        49 => "\\",
        51 => ";",
        52 => "'",
        53 => "`",
        54 => ",",
        55 => ".",
        56 => "/",
        57 => "capslock",
        58..=69 => F[(code - 58) as usize],
        79 => "right",
        80 => "left",
        81 => "down",
        82 => "up",
        224 => "lctrl",
        225 => "lshift",
        226 => "lalt",
        227 => "lgui",
        228 => "rctrl",
        229 => "rshift",
        230 => "ralt",
        _ => return None,
    })
}

/// Every key name `love.keyboard.isDown` knows (a name outside it is an error there).
pub fn is_key_name(name: &str) -> bool {
    (0..=230).any(|c| key_name(c) == Some(name))
}

/// LÖVE's gamepad button names (`start` and `back` never arrive).
fn button_name(code: u32) -> Option<&'static str> {
    Some(match code {
        pad::A => "a",
        pad::B => "b",
        pad::X => "x",
        pad::Y => "y",
        pad::LB => "leftshoulder",
        pad::RB => "rightshoulder",
        pad::LS => "leftstick",
        pad::RS => "rightstick",
        pad::UP => "dpup",
        pad::DOWN => "dpdown",
        pad::LEFT => "dpleft",
        pad::RIGHT => "dpright",
        _ => return None,
    })
}

pub const BUTTONS: &[&str] = &["a", "b", "x", "y", "leftshoulder", "rightshoulder", "leftstick", "rightstick", "dpup", "dpdown", "dpleft", "dpright", "start", "back", "guide"];
pub const AXES: &[&str] = &["leftx", "lefty", "rightx", "righty", "triggerleft", "triggerright"];

#[derive(Default)]
pub struct Mapper {
    pub held: Held,
    /// The finger that is also the mouse.
    primary: Option<u8>,
    events: Vec<LoveEvent>,
}

impl Mapper {
    /// A new round: nothing held.
    pub fn reset(&mut self) {
        *self = Mapper::default();
    }

    /// One event the round flow passed on; `(ox, oy, scale)` maps the
    /// screen to the canvas.
    pub fn feed(&mut self, e: &Event, (ox, oy, s): (f32, f32, f32)) {
        let (x, y) = ((e.x - ox) / s, (e.y - oy) / s);
        let h = &mut self.held;
        let ev = &mut self.events;
        match e.kind() {
            Kind::TouchDown => {
                h.touches.push((e.id, x, y));
                ev.push(LoveEvent::TouchPressed { id: e.id, x, y, pressure: if e.a > 0.0 { e.a } else { 1.0 } });
                if self.primary.is_none() {
                    self.primary = Some(e.id);
                    (h.mx, h.my, h.mouse[0]) = (x, y, true);
                    ev.push(LoveEvent::MousePressed { x, y, button: 1, istouch: true });
                }
            }
            Kind::TouchMove => {
                if let Some(t) = h.touches.iter_mut().find(|t| t.0 == e.id) {
                    let (dx, dy) = (x - t.1, y - t.2);
                    (t.1, t.2) = (x, y);
                    ev.push(LoveEvent::TouchMoved { id: e.id, x, y, dx, dy, pressure: if e.a > 0.0 { e.a } else { 1.0 } });
                    if self.primary == Some(e.id) {
                        let (dx, dy) = (x - h.mx, y - h.my);
                        (h.mx, h.my) = (x, y);
                        ev.push(LoveEvent::MouseMoved { x, y, dx, dy, istouch: true });
                    }
                }
            }
            Kind::TouchUp | Kind::TouchCancel => {
                if h.touches.iter().any(|t| t.0 == e.id) {
                    h.touches.retain(|t| t.0 != e.id);
                    ev.push(LoveEvent::TouchReleased { id: e.id, x, y });
                    if self.primary == Some(e.id) {
                        self.primary = None;
                        (h.mx, h.my, h.mouse[0]) = (x, y, false);
                        ev.push(LoveEvent::MouseReleased { x, y, button: 1, istouch: true });
                    }
                }
            }
            Kind::MouseDown => {
                let b = e.id.min(2) as usize;
                (h.mx, h.my, h.mouse[b]) = (x, y, true);
                ev.push(LoveEvent::MousePressed { x, y, button: b as u8 + 1, istouch: false });
            }
            Kind::MouseUp => {
                let b = e.id.min(2) as usize;
                (h.mx, h.my, h.mouse[b]) = (x, y, false);
                ev.push(LoveEvent::MouseReleased { x, y, button: b as u8 + 1, istouch: false });
            }
            Kind::MouseMove => {
                let (dx, dy) = (x - h.mx, y - h.my);
                (h.mx, h.my) = (x, y);
                ev.push(LoveEvent::MouseMoved { x, y, dx, dy, istouch: false });
            }
            Kind::MouseWheel => ev.push(LoveEvent::WheelMoved(e.a, e.b)),
            Kind::KeyDown => {
                if let Some(k) = key_name(e.code) {
                    // The host doesn't repeat keys; a second down is ignored.
                    if !h.keys.contains(&k) {
                        h.keys.push(k);
                        ev.push(LoveEvent::KeyPressed(k));
                    }
                }
            }
            Kind::KeyUp => {
                if let Some(k) = key_name(e.code)
                    && h.keys.contains(&k)
                {
                    h.keys.retain(|x| *x != k);
                    ev.push(LoveEvent::KeyReleased(k));
                }
            }
            Kind::PadButton => {
                h.pad = true;
                if let Some(b) = button_name(e.code) {
                    let down = e.a > 0.5;
                    if down && !h.buttons.contains(&b) {
                        h.buttons.push(b);
                        ev.push(LoveEvent::GamepadPressed(b));
                    } else if !down && h.buttons.contains(&b) {
                        h.buttons.retain(|x| *x != b);
                        ev.push(LoveEvent::GamepadReleased(b));
                    }
                }
                // The triggers are axes in LÖVE.
                if e.code == pad::LT || e.code == pad::RT {
                    let i = if e.code == pad::LT { 4 } else { 5 };
                    h.axes[i] = e.a;
                    ev.push(LoveEvent::GamepadAxis(AXES[i], e.a));
                }
            }
            Kind::PadAxis if (e.code as usize) < 4 => {
                h.pad = true;
                h.axes[e.code as usize] = e.a;
                ev.push(LoveEvent::GamepadAxis(AXES[e.code as usize], e.a));
            }
            _ => {}
        }
    }

    /// This frame's events; held state stays.
    pub fn take(&mut self) -> Vec<LoveEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maimbrain::input::key;

    fn ev(kind: Kind, id: u8, x: f32, y: f32, code: u32, a: f32) -> Event {
        Event { kind: kind as u8, id, x, y, code, a, ..Event::default() }
    }

    #[test]
    fn a_finger_is_a_touch_and_the_mouse() {
        let mut m = Mapper::default();
        let tf = (0.0, 0.0, 2.0);
        m.feed(&ev(Kind::TouchDown, 3, 100.0, 200.0, 0, 1.0), tf);
        m.feed(&ev(Kind::TouchDown, 4, 10.0, 20.0, 0, 1.0), tf);
        m.feed(&ev(Kind::TouchMove, 3, 120.0, 200.0, 0, 1.0), tf);
        assert!(m.held.mouse[0] && m.held.touches.len() == 2 && m.held.mx == 60.0);
        let e = m.take();
        assert_eq!(e[1], LoveEvent::MousePressed { x: 50.0, y: 100.0, button: 1, istouch: true });
        assert!(matches!(e[2], LoveEvent::TouchPressed { id: 4, .. }), "the second finger isn't the mouse");
        assert_eq!(e[4], LoveEvent::MouseMoved { x: 60.0, y: 100.0, dx: 10.0, dy: 0.0, istouch: true });
        m.feed(&ev(Kind::TouchUp, 3, 120.0, 200.0, 0, 0.0), tf);
        assert!(!m.held.mouse[0]);
        assert!(m.take().contains(&LoveEvent::MouseReleased { x: 60.0, y: 100.0, button: 1, istouch: true }));
    }

    #[test]
    fn keys_and_buttons_by_loves_names() {
        let mut m = Mapper::default();
        let tf = (0.0, 0.0, 1.0);
        m.feed(&ev(Kind::KeyDown, 0, 0.0, 0.0, key::SPACE, 0.0), tf);
        m.feed(&ev(Kind::KeyDown, 0, 0.0, 0.0, key::SPACE, 0.0), tf);
        m.feed(&ev(Kind::KeyDown, 0, 0.0, 0.0, key::W, 0.0), tf);
        m.feed(&ev(Kind::PadButton, 0, 0.0, 0.0, pad::A, 1.0), tf);
        m.feed(&ev(Kind::PadButton, 0, 0.0, 0.0, pad::START, 1.0), tf);
        m.feed(&ev(Kind::PadAxis, 0, 0.0, 0.0, pad::LX, -0.5), tf);
        assert_eq!(m.take(), [LoveEvent::KeyPressed("space"), LoveEvent::KeyPressed("w"), LoveEvent::GamepadPressed("a"), LoveEvent::GamepadAxis("leftx", -0.5)]);
        assert!(m.held.key("space") && m.held.button("a") && m.held.axis("leftx") == -0.5);
        assert_eq!(key_name(key::ENTER), Some("return"));
        assert!(is_key_name("lshift") && !is_key_name("shift"));
    }
}
