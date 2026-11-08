//! The immediate layer: drawing commands recorded during an update and
//! drawn in the next render, between the scene's nodes at order 0 or less
//! and those above (docs/STUDIO-2.md §2.2, §5 [love]). Studio engine 2.0
//! records `love.draw()` here, so Lua never runs in `mb_render`.
//!
//! Coordinates are the view's (LÖVE's screen space), unless the commands
//! between [`Cmd::Attach`] and [`Cmd::Detach`] put them in the world through
//! the scene's camera (hump.camera's `attach`/`detach`).

use crate::gfx2d::{self, Blend, Font};

use super::{Align, ImageId, Scene, View, with_alpha};

/// Commands a frame may record, past which [`DrawList::push`] refuses.
pub const MAX_CMDS: usize = 32_768;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// `line` 0 fills; `r` rounds the corners.
    Rect { x: f32, y: f32, w: f32, h: f32, r: f32, line: f32, color: u32 },
    Circle { x: f32, y: f32, r: f32, line: f32, color: u32 },
    /// A line through the points (x1, y1, x2, y2, …).
    Line { points: Vec<f32>, width: f32, color: u32 },
    /// A closed polygon; `line` 0 fills.
    Poly { points: Vec<f32>, line: f32, color: u32 },
    /// Text with its top at `y`; wrapped at `wrap` (0: no wrapping), each
    /// line placed on `x` by `align` within the wrap width.
    Text { s: String, font: Font, size: f32, x: f32, y: f32, color: u32, wrap: f32, align: Align },
    /// A scene image at x, y, turned by r about the origin ox, oy, scaled:
    /// the rectangle `quad` of it (in its pixels, LÖVE's quads), else frame
    /// `frame` of its sheet (from 1), else (0) the whole picture.
    Image { image: ImageId, frame: u32, quad: Option<[f32; 4]>, x: f32, y: f32, r: f32, sx: f32, sy: f32, ox: f32, oy: f32, color: u32 },
    /// One of the scene's particle systems ([`Scene::systems`](super::Scene::systems)), moved by x, y.
    Particles { system: u16, x: f32, y: f32 },
    Push,
    Pop,
    Translate(f32, f32),
    Rotate(f32),
    Scale(f32, f32),
    /// Back to the view's own transform (the pushes before it are undone).
    Origin,
    Blend(Blend),
    /// Images drawn after it go through sprite shader `shader` (mb2d 4; 0:
    /// none), with these parameters (`shaders::Params::pack`). Only the
    /// `shaders` builds draw it; the others ignore it.
    Shader { shader: u32, params: Box<[u8; crate::shaders::Params::SIZE]> },
    /// Into the world, through the camera; until [`Cmd::Detach`].
    Attach,
    Detach,
}

/// One frame's commands.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    cmds: Vec<Cmd>,
}

impl DrawList {
    pub fn new() -> DrawList {
        DrawList::default()
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
    }

    /// Records a command; false (and nothing recorded) past [`MAX_CMDS`].
    pub fn push(&mut self, c: Cmd) -> bool {
        if self.cmds.len() >= MAX_CMDS {
            return false;
        }
        self.cmds.push(c);
        true
    }

    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    pub fn cmds(&self) -> &[Cmd] {
        &self.cmds
    }

    /// The strings drawn, for a description of the frame.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.cmds.iter().filter_map(|c| match c {
            Cmd::Text { s, .. } => Some(s.as_str()),
            _ => None,
        })
    }

    /// Draws the commands into `view`. Pushes left open are closed, and the
    /// blend mode is put back, so nothing leaks into what's drawn after.
    pub fn replay(&self, scene: &Scene, view: &View) {
        gfx2d::push();
        // Pushes since the view's own transform (Origin pops back to it).
        let mut depth = 0usize;
        let mut attached = false;
        let mut blended = false;
        #[allow(unused_mut)]
        let mut shaded = false;
        for c in &self.cmds {
            match c {
                Cmd::Rect { x, y, w, h, r, line, color } => {
                    if *r > 0.0 || *line > 0.0 {
                        gfx2d::rrect(*x, *y, *w, *h, *r, *line, 0.0, *color, *color);
                    } else {
                        gfx2d::rect(*x, *y, *w, *h, *color);
                    }
                }
                Cmd::Circle { x, y, r, line, color } => {
                    if *line > 0.0 {
                        gfx2d::rrect(x - r, y - r, r * 2.0, r * 2.0, *r, *line, 0.0, *color, *color);
                    } else {
                        gfx2d::circle(*x, *y, *r, *color);
                    }
                }
                Cmd::Line { points, width, color } => {
                    for i in (0..points.len().saturating_sub(3)).step_by(2) {
                        gfx2d::line(points[i], points[i + 1], points[i + 2], points[i + 3], *width, *color);
                    }
                }
                Cmd::Poly { points, line, color } => {
                    if *line > 0.0 {
                        let k = points.len();
                        for i in (0..k.saturating_sub(3)).step_by(2) {
                            gfx2d::line(points[i], points[i + 1], points[i + 2], points[i + 3], *line, *color);
                        }
                        if k >= 6 {
                            gfx2d::line(points[k - 2], points[k - 1], points[0], points[1], *line, *color);
                        }
                    } else if points.len() >= 6 {
                        gfx2d::poly(points, *color);
                    }
                }
                Cmd::Text { s, font, size, x, y, color, wrap, align } => {
                    let t = crate::ui::text(s).font(*font).size(*size).color(*color).wrap(*wrap).align(*align);
                    // Within the wrap width, as LÖVE's printf aligns.
                    let ax = match (align, *wrap > 0.0) {
                        (Align::Center, true) => x + wrap / 2.0,
                        (Align::Right, true) => x + wrap,
                        _ => *x,
                    };
                    t.draw(ax, *y);
                }
                Cmd::Image { image, frame, quad, x, y, r, sx, sy, ox, oy, color } => {
                    let Some(im) = scene.image(*image) else { continue };
                    let (src, w, h) = match (quad, *frame) {
                        (Some(q), _) => (im.sub(*q), q[2], q[3]),
                        (None, 0) => (im.src(), im.w, im.h),
                        (None, f) => (im.frame_src(f), im.frame_w, im.frame_h),
                    };
                    gfx2d::push();
                    gfx2d::translate(*x, *y);
                    if *r != 0.0 {
                        gfx2d::rotate(*r);
                    }
                    gfx2d::scale(*sx, *sy);
                    match im.image {
                        Some(handle) => gfx2d::sprite(handle, src, [-ox, -oy, w, h], *color),
                        None => gfx2d::rect(-ox, -oy, w, h, with_alpha(*color & 0xffffff00 | 0x40, 1.0)),
                    }
                    gfx2d::pop();
                }
                Cmd::Particles { system, x, y } => {
                    let Some(e) = scene.systems.get(*system as usize) else { continue };
                    gfx2d::push();
                    gfx2d::translate(*x, *y);
                    e.particles.draw();
                    gfx2d::pop();
                }
                Cmd::Push => {
                    gfx2d::push();
                    depth += 1;
                }
                Cmd::Pop => {
                    if depth > 0 {
                        gfx2d::pop();
                        depth -= 1;
                    }
                }
                Cmd::Translate(x, y) => gfx2d::translate(*x, *y),
                Cmd::Rotate(r) => gfx2d::rotate(*r),
                Cmd::Scale(x, y) => gfx2d::scale(*x, *y),
                Cmd::Origin => {
                    gfx2d::pop();
                    for _ in 0..depth {
                        gfx2d::pop();
                    }
                    depth = 0;
                    attached = false;
                    gfx2d::push();
                }
                Cmd::Blend(b) => {
                    gfx2d::blend(*b);
                    blended = *b != Blend::Alpha;
                }
                #[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
                Cmd::Shader { shader, params } => {
                    unsafe { crate::ffi::mb2d_shader_use(*shader, params.as_ptr()) };
                    shaded = *shader != 0;
                }
                #[cfg(not(any(feature = "shaders", not(target_arch = "wasm32"))))]
                Cmd::Shader { .. } => {}
                Cmd::Attach if !attached => {
                    gfx2d::push();
                    scene.camera.apply(view.w, view.h);
                    depth += 1;
                    attached = true;
                }
                Cmd::Detach if attached => {
                    gfx2d::pop();
                    depth -= 1;
                    attached = false;
                }
                Cmd::Attach | Cmd::Detach => {}
            }
        }
        for _ in 0..depth {
            gfx2d::pop();
        }
        if blended {
            gfx2d::blend(Blend::Alpha);
        }
        #[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
        if shaded {
            unsafe { crate::ffi::mb2d_shader_use(0, [0u8; crate::shaders::Params::SIZE].as_ptr()) };
        }
        let _ = shaded;
        gfx2d::pop();
    }
}
