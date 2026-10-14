//! Standing things on the floor: a generated figure drawn with its feet on a
//! painting's floor line, at the height it should be, with a contact shadow
//! (docs/GENERATE.md "Standing things on the floor").
//!
//! `mb art` measures every generated sprite, frames sheet, layer and
//! background and writes `src/art_meta.rs`: a [`Footing`] for each figure
//! (where its feet are in the image, how tall it's drawn) and a [`Backdrop`]
//! for each painting with a floor line (which row it's on). Drawing with
//! them, the feet land on the floor whatever margin the image has under
//! them, and a figure is as tall as its `mb art … --height` said:
//!
//! ```ignore
//! mod art_meta;
//! use maimbrain::ground::Figure;
//!
//! // The corridor, scrolled, filling the screen's height; its floor's y on screen.
//! let floor = art_meta::HALL_0.draw(self.hall, self.scroll, 0.0, screen.height);
//! Figure::new(self.girl, art_meta::GIRL).frame(self.step).draw(self.x, floor);
//! Figure::new(self.witch, art_meta::WITCH).flip(true).draw(self.witch_x, floor);
//! ```
//!
//! A scene of several: size props against the hero with a [`Scale`] (from
//! each footing's real-world height, `mb art … --real`), draw them in depth
//! order with a [`Scene`] (by their feet, or a declared [`Figure::layer`]),
//! stop a body short of what's in the way ([`stop_short`]), and take taps
//! on what's drawn, head included ([`Figure::hit`]):
//!
//! ```ignore
//! let scale = Scale::of(art_meta::GIRL);                  // the hero's height sets units a meter
//! let mut scene = Scene::new();
//! scene.figure(Figure::new(self.mirror, art_meta::MIRROR).sized(scale), 220.0, floor);
//! scene.figure(Figure::new(self.girl, art_meta::GIRL).layer(1), self.x, floor);
//! scene.draw();                                           // back to front
//! let witch = Figure::new(self.witch, art_meta::WITCH);
//! if ground::hits(witch.hit(self.witch_x, floor), tap) { /* … */ }
//! ```
//!
//! `mb shot` checks what these draw (its grounding check, on by default):
//! a figure floating above the floor it was stood on, its feet below the
//! screen or cropped away, a figure with no contact shadow, one drawn in
//! front of another it stands behind, two standing in each other on one
//! floor, a tap area ([`tap_area`]) that misses part of the figure it's
//! for. While it does, [`Figure::draw`] and the floors report where they
//! drew with `mb_mark`; otherwise (and on a device) nothing is reported.
//! `mb shot --taps` also draws every tap area over the shot (the game's in
//! magenta, each figure's [`Figure::hit`] in cyan).

use crate::gfx2d::{self, Image};

/// Where a figure stands in its image: `mb art` writes one for each
/// generated sprite and frames sheet in `src/art_meta.rs`. Texels are the
/// image's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footing {
    /// The asset's name (in `mb shot`'s reports).
    pub name: &'static str,
    /// The image, or one frame's cell of a sheet, in texels.
    pub cell: [f32; 2],
    /// A sheet's columns (1 for a single image).
    pub cols: u32,
    /// The point on the ground under the figure, in texels from the cell's
    /// top-left: the middle of its feet, at the bottom of its lowest opaque
    /// row (a sheet: the lowest of any frame).
    pub foot: [f32; 2],
    /// The figure's first opaque row (a sheet: the highest of any frame).
    pub top: f32,
    /// How wide its feet (or hem, or base) are, in texels: the contact shadow's width.
    pub base: f32,
    /// Its opaque columns, left and right edge, in texels from the cell's
    /// left (a sheet: of any frame). With `top` and the feet, the box of
    /// what's drawn: [`Figure::body`], [`Figure::hit`].
    pub span: [f32; 2],
    /// How tall it's drawn, top to feet, in logical units: `--height`, or
    /// half its height in texels.
    pub height: f32,
    /// How tall it is in the real world, in meters (`--real`, else a typical
    /// height for what it is); 0 if unknown. [`Scale`] sizes it against the hero.
    pub real: f32,
}

impl Footing {
    /// A footing for art made another way: a `w`×`h` texel image standing
    /// on its bottom edge, its feet in the middle, drawn at 2 texels a unit.
    pub const fn bottom(name: &'static str, w: f32, h: f32) -> Footing {
        Footing { name, cell: [w, h], cols: 1, foot: [w / 2.0, h], top: 0.0, base: w * 0.6, span: [0.0, w], height: h / 2.0, real: 0.0 }
    }

    /// The same, `meters` tall in the real world (for [`Scale`]).
    pub const fn real(mut self, meters: f32) -> Footing {
        self.real = meters;
        self
    }
}

/// A scene's scale: logical units a meter, so every figure and prop is
/// sized against the hero the way they are in the world (a mirror taller
/// than a child, a bell that fits in her hand), not by eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub units_per_m: f32,
}

impl Scale {
    /// From the hero as its footing draws it: its height over its real height
    /// (1.7 m when its footing has none).
    pub fn of(hero: Footing) -> Scale {
        Scale::of_height(hero, hero.height)
    }

    /// From the hero drawn `height` units tall.
    pub fn of_height(hero: Footing, height: f32) -> Scale {
        Scale { units_per_m: if hero.real > 0.0 { height / hero.real } else { height / 1.7 } }
    }

    /// Logical units for `meters`.
    pub fn units(&self, meters: f32) -> f32 {
        meters * self.units_per_m
    }

    /// How tall `f` is drawn at this scale: its real height in units, or its
    /// footing's height when its real one is unknown. A thing smaller than
    /// [`HANDHELD`] is drawn bigger than life (at least `HANDHELD_MIN` units)
    /// so it reads.
    pub fn height(&self, f: Footing) -> f32 {
        if f.real <= 0.0 {
            return f.height;
        }
        let h = f.real * self.units_per_m;
        if f.real < HANDHELD { h.max(HANDHELD_MIN) } else { h }
    }
}

/// Meters below which a thing is carried rather than stood beside.
pub const HANDHELD: f32 = 0.3;
/// Units a handheld thing is drawn at least (a bell, a key: bigger than life so they read).
pub const HANDHELD_MIN: f32 = 16.0;
/// The least a tap target is across, either way, in logical units (Apple's 44 points).
pub const MIN_TAP: f32 = 44.0;

/// Whether (`x`, `y`) is inside `rect` ([x, y, w, h]).
pub fn hits(rect: [f32; 4], x: f32, y: f32) -> bool {
    x >= rect[0] && x <= rect[0] + rect[2] && y >= rect[1] && y <= rect[1] + rect[3]
}

/// Where to stop walking toward an obstacle (its [`Figure::body`]) so a
/// body `width` wide coming from `from` stands clear of it: half a body
/// width short of its drawn edge (stopping on it, or a hair away, looks
/// tangled up in it). Interacting with something within reach still reads.
pub fn stop_short(from: f32, width: f32, obstacle: [f32; 4]) -> f32 {
    if from <= obstacle[0] + obstacle[2] / 2.0 { obstacle[0] - width } else { obstacle[0] + obstacle[2] + width }
}

/// A rect grown to at least [`MIN_TAP`] each way around its middle.
pub fn tap_size(r: [f32; 4]) -> [f32; 4] {
    let (w, h) = (r[2].max(MIN_TAP), r[3].max(MIN_TAP));
    [r[0] + r[2] / 2.0 - w / 2.0, r[1] + r[3] / 2.0 - h / 2.0, w, h]
}

/// Tells `mb shot` that a tap on `rect` ([x, y, w, h], the current
/// transform applies) counts as a tap on `name` this frame: call it where
/// you draw it, with the rect your input code tests. `mb shot` warns when
/// it misses part of the figure of the same name (its head) or is under
/// [`MIN_TAP`] across, and `--taps` draws it over the shot. Prefer
/// [`Figure::hit`]'s rect: it is the drawn figure. Nothing on a device.
pub fn tap_area(name: &'static str, rect: [f32; 4]) {
    if taps_shown() {
        outline(rect, TAP_COLOR, true);
    }
    if checking() {
        let c = corners(rect);
        note(Note::Tap { name, l: c.iter().map(|p| p[0]).fold(f32::MAX, f32::min), t: c.iter().map(|p| p[1]).fold(f32::MAX, f32::min), r: c.iter().map(|p| p[0]).fold(f32::MIN, f32::max), b: c.iter().map(|p| p[1]).fold(f32::MIN, f32::max) });
    }
}

/// A rect's corners on screen.
fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[0] + r[2], r[1]], [r[0], r[1] + r[3]], [r[0] + r[2], r[1] + r[3]]].map(track::apply)
}

const TAP_COLOR: u32 = 0xff2bd6e0;
const HIT_COLOR: u32 = 0x22e5ffd0;

/// `mb shot --taps`: a rect's outline (and a faint fill) over the frame.
fn outline(r: [f32; 4], c: u32, fill: bool) {
    let t = 1.5;
    if fill {
        gfx2d::rect(r[0], r[1], r[2], r[3], (c & 0xffffff00) | 0x30);
    }
    gfx2d::rect(r[0], r[1], r[2], t, c);
    gfx2d::rect(r[0], r[1] + r[3] - t, r[2], t, c);
    gfx2d::rect(r[0], r[1], t, r[3], c);
    gfx2d::rect(r[0] + r[2] - t, r[1], t, r[3], c);
}

/// A painting with a floor line: `mb art` writes one for each generated
/// layer and background where it finds one, in `src/art_meta.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    pub name: &'static str,
    /// The image in texels.
    pub size: [f32; 2],
    /// The floor line's row, in texels from the image's top.
    pub floor: f32,
}

impl Backdrop {
    /// The floor's y when the painting is drawn `height` units tall from
    /// `top`, for a painting the game draws itself. Tells `mb shot` there's
    /// a floor there, across the whole screen.
    pub fn floor_at(&self, top: f32, height: f32) -> f32 {
        let y = top + self.floor * height / self.size[1].max(1.0);
        if checking() {
            let p = track::apply([0.0, y]);
            note(Note::Floor { y: p[1], x0: 0.0, x1: crate::sys::screen().width });
        }
        y
    }

    /// Draws the painting `height` units tall from `top`, repeated across
    /// the screen and moved left by `scroll` units (a layer that wraps: draw
    /// it outside any camera transform, `scroll` is the camera), and returns
    /// its floor's y.
    pub fn draw(&self, img: Image, scroll: f32, top: f32, height: f32) -> f32 {
        let w = self.size[0] * height / self.size[1].max(1.0);
        let screen = crate::sys::screen().width;
        let y = top + self.floor * height / self.size[1].max(1.0);
        if w > 0.5 {
            let first = -scroll.rem_euclid(w);
            let mut x = first;
            while x < screen {
                gfx2d::sprite(img, [0.0, 0.0, self.size[0], self.size[1]], [x, top, w + 0.01, height], 0xffffffff);
                x += w;
            }
            floor_line(y, first, x);
        }
        y
    }
}

/// Tells `mb shot` that things stand on a floor at `y` from `x0` to `x1`
/// (the current transform applies), for a floor drawn some other way than
/// [`Backdrop`]: a ground the game draws in code, a stage.
pub fn floor_line(y: f32, x0: f32, x1: f32) {
    if checking() {
        let (a, b) = (track::apply([x0, y]), track::apply([x1, y]));
        note(Note::Floor { y: (a[1] + b[1]) / 2.0, x0: a[0].min(b[0]), x1: a[0].max(b[0]) });
    }
}

/// A figure or prop to stand on a floor: a generated image and its
/// [`Footing`], drawn with its feet on the floor.
#[derive(Clone, Copy, Debug)]
pub struct Figure {
    img: Image,
    footing: Footing,
    src: Option<[f32; 4]>,
    height: f32,
    flip: bool,
    shadow: f32,
    pixel_shadow: bool,
    tint: u32,
    lift: f32,
    layer: i8,
}

impl Figure {
    pub fn new(img: Image, footing: Footing) -> Figure {
        Figure { img, footing, src: None, height: footing.height, flip: false, shadow: 0.3, pixel_shadow: false, tint: 0xffffffff, lift: 0.0, layer: 0 }
    }

    /// As tall as its real height at the scene's [`Scale`] (its footing's
    /// height if its real one is unknown).
    pub fn sized(mut self, scale: Scale) -> Figure {
        self.height = scale.height(self.footing);
        self
    }

    /// Its depth layer in a [`Scene`] (default 0): higher layers draw in
    /// front of lower ones whatever their feet. Put props the hero walks
    /// past along the back wall at -1, a foreground pillar at 2, the hero on
    /// 1 if it should pass in front of everything on the floor. Within a
    /// layer, a [`Scene`] draws figures by their feet (lower on screen, nearer).
    /// `mb shot` doesn't warn about overlaps between different layers.
    pub fn layer(mut self, layer: i8) -> Figure {
        self.layer = layer.clamp(-3, 3);
        self
    }

    /// The box of what's drawn with its feet at (`x`, `ground`), less any
    /// lift: its opaque columns, head to feet: [x, y, w, h].
    pub fn body(&self, x: f32, ground: f32) -> [f32; 4] {
        let s = self.units_per_texel();
        let floor = ground - self.lift;
        let w = self.footing.cell[0];
        let [a, b] = self.footing.span;
        let (a, b) = if self.flip { (w - b, w - a) } else { (a, b) };
        let fx = if self.flip { w - self.footing.foot[0] } else { self.footing.foot[0] };
        [x + (a - fx) * s, floor - self.height, (b - a).max(0.0) * s, self.height]
    }

    /// What a tap on it should hit: its [`body`](Self::body), head to feet,
    /// grown to at least [`MIN_TAP`] each way. Test taps with this (and
    /// [`hits`]), never a rect of your own: people tap faces.
    pub fn hit(&self, x: f32, ground: f32) -> [f32; 4] {
        tap_size(self.body(x, ground))
    }

    /// Frame `i` of a sheet (cells in rows of `footing.cols`, from the
    /// [`src`](Self::src) rect's corner in an atlas).
    pub fn frame(mut self, i: usize) -> Figure {
        let cols = self.footing.cols.max(1) as usize;
        let [w, h] = self.footing.cell;
        let [x0, y0] = self.src.map_or([0.0, 0.0], |r| [r[0], r[1]]);
        self.src = Some([x0 + (i % cols) as f32 * w, y0 + (i / cols) as f32 * h, w, h]);
        self
    }

    /// Where the image (or the sheet) is in a bigger one: an atlas rect
    /// (`mb art atlas` writes `<NAME>` and `<NAME>_FOOTING`). Call it before
    /// [`frame`](Self::frame). A rect shorter than the cell crops the figure
    /// (`mb shot` reports it when it crops the feet).
    pub fn src(mut self, rect: [f32; 4]) -> Figure {
        let [w, h] = self.footing.cell;
        // A whole sheet's rect: its first cell until frame() picks one.
        self.src = Some(if self.footing.cols > 1 && rect[2] > w + 0.5 { [rect[0], rect[1], w, h.min(rect[3])] } else { rect });
        self
    }

    /// How tall it stands, top to feet, in logical units (default: the
    /// footing's height).
    pub fn height(mut self, units: f32) -> Figure {
        self.height = units;
        self
    }

    /// `k` times its footing's height (a squash, a growing boss).
    pub fn scale(mut self, k: f32) -> Figure {
        self.height = self.footing.height * k;
        self
    }

    /// Mirrored around its feet (facing the other way).
    pub fn flip(mut self, on: bool) -> Figure {
        self.flip = on;
        self
    }

    /// The contact shadow's darkness, 0–1 (default 0.3; 0 for none, for
    /// something that hovers).
    pub fn shadow(mut self, alpha: f32) -> Figure {
        self.shadow = alpha.clamp(0.0, 1.0);
        self
    }

    /// A stepped shadow of whole art pixels (2 units), for pixel art,
    /// instead of a soft one.
    pub fn pixel_shadow(mut self) -> Figure {
        self.pixel_shadow = true;
        self
    }

    /// Drawn this many units above its floor on purpose (a jump, a bob, a
    /// hover), its shadow staying on the floor and shrinking as it rises.
    /// `draw` still takes the floor it's over.
    pub fn lift(mut self, units: f32) -> Figure {
        self.lift = units.max(0.0);
        self
    }

    pub fn tint(mut self, rgba: u32) -> Figure {
        self.tint = rgba;
        self
    }

    /// Logical units per texel at this height.
    fn units_per_texel(&self) -> f32 {
        let tall = self.footing.foot[1] - self.footing.top;
        if tall > 0.5 { self.height / tall } else { 0.5 }
    }

    /// Where the cell draws with its feet at (`x`, `floor`) (less any
    /// [`lift`](Self::lift)): [x, y, w, h].
    pub fn rect(&self, x: f32, floor: f32) -> [f32; 4] {
        let floor = floor - self.lift;
        let s = self.units_per_texel();
        let [w, h] = self.footing.cell;
        let fx = if self.flip { w - self.footing.foot[0] } else { self.footing.foot[0] };
        [x - fx * s, floor - self.footing.foot[1] * s, w * s, h * s]
    }

    /// Draws it with its feet at (`x`, `floor`), its contact shadow first.
    /// Returns where the figure is, top to feet: [x, y, w, h].
    pub fn draw(&self, x: f32, ground: f32) -> [f32; 4] {
        let s = self.units_per_texel();
        let [cw, ch] = self.footing.cell;
        let src = self.src.unwrap_or([0.0, 0.0, cw, ch]);
        if self.shadow > 0.0 {
            let k = 1.0 - (self.lift / self.height.max(1.0)).min(0.6);
            shadow(x, ground, (self.footing.base * s * 1.15).max(self.height * 0.2) * k, self.shadow * k, self.pixel_shadow);
        }
        let floor = ground - self.lift;
        // The cell's rect as given: an atlas cell the size of the footing's.
        let dst_w = src[2] * s;
        let dst_h = src[3] * s;
        let top = floor - self.footing.foot[1] * s;
        if self.flip {
            gfx2d::push();
            gfx2d::translate(x, 0.0);
            gfx2d::scale(-1.0, 1.0);
            gfx2d::sprite(self.img, src, [-self.footing.foot[0] * s, top, dst_w, dst_h], self.tint);
            gfx2d::pop();
        } else {
            gfx2d::sprite(self.img, src, [x - self.footing.foot[0] * s, top, dst_w, dst_h], self.tint);
        }
        if taps_shown() {
            outline(self.hit(x, ground), HIT_COLOR, false);
        }
        if checking() {
            // A src shorter than the cell (or cut above the feet row) loses the feet.
            let cropped = src[3] + 0.5 < self.footing.foot[1];
            report(self.footing.name, self.body(x, ground), x, ground, self.footing.base * s, self.layer, self.shadow > 0.05, cropped, self.lift > 0.5);
        }
        [x - self.footing.base * s / 2.0, floor - self.height, self.footing.base * s, self.height]
    }
}

/// Figures (and anything else standing on the floor) drawn back to front:
/// by [`Figure::layer`], then by their feet (higher up the screen is
/// farther: drawn first), then in the order added. Add them in any order
/// during `render`, then [`draw`](Self::draw).
#[derive(Default)]
pub struct Scene<'a> {
    items: Vec<(i8, f32, Item<'a>)>,
}

enum Item<'a> {
    Figure(Figure, f32, f32),
    Other(Box<dyn FnOnce() + 'a>),
}

impl<'a> Scene<'a> {
    pub fn new() -> Scene<'a> {
        Scene { items: Vec::new() }
    }

    /// A figure with its feet at (`x`, `ground`). Returns its [`Figure::body`].
    pub fn figure(&mut self, f: Figure, x: f32, ground: f32) -> [f32; 4] {
        self.items.push((f.layer, ground, Item::Figure(f, x, ground)));
        f.body(x, ground)
    }

    /// Something drawn another way (a `walk::WalkSprite`, a shape) whose
    /// feet are at `feet_y`, on `layer`.
    pub fn other(&mut self, layer: i8, feet_y: f32, draw: impl FnOnce() + 'a) {
        self.items.push((layer, feet_y, Item::Other(Box::new(draw))));
    }

    /// Draws everything, back to front.
    pub fn draw(mut self) {
        self.items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for (_, _, item) in self.items {
            match item {
                Item::Figure(f, x, g) => {
                    f.draw(x, g);
                }
                Item::Other(d) => d(),
            }
        }
    }
}

/// Reports a figure to `mb shot`: its `body` ([x, y, w, h]), its feet at
/// (`x`, `ground`) `base` wide, all under the current transform.
#[allow(clippy::too_many_arguments)]
pub(crate) fn report(name: &'static str, body: [f32; 4], x: f32, ground: f32, base: f32, layer: i8, shadow: bool, cropped: bool, lifted: bool) {
    if !checking() {
        return;
    }
    let c = corners(body);
    let feet = track::apply([x, ground]);
    let edge = track::apply([x + base / 2.0, ground]);
    let head = track::apply([x, body[1]]);
    note(Note::Figure {
        name,
        l: c.iter().map(|p| p[0]).fold(f32::MAX, f32::min),
        r: c.iter().map(|p| p[0]).fold(f32::MIN, f32::max),
        t: head[1],
        b: feet[1],
        fx: feet[0],
        base: 2.0 * (edge[0] - feet[0]).abs(),
        layer,
        shadow,
        cropped,
        lifted,
    });
}

/// A contact shadow centred on (`x`, `y`), `w` units wide.
fn shadow(x: f32, y: f32, w: f32, alpha: f32, pixel: bool) {
    let c = (alpha * 255.0).round() as u32;
    if pixel {
        // Whole art pixels: a 1-pixel-tall bar with a narrower one under it.
        let px = 2.0;
        let w = (w / px).round().max(2.0) * px;
        let n = ((w * 0.7) / px).round().max(1.0) * px;
        gfx2d::rect((x - w / 2.0).round(), y - px, w, px, c);
        gfx2d::rect((x - n / 2.0).round(), y, n, px, c);
    } else {
        let h = (w * 0.22).max(2.0);
        gfx2d::rrect(x - w / 2.0, y - h / 2.0, w, h, h / 2.0, 0.0, h * 0.5, c, c);
    }
}

// --- what mb shot sees ---------------------------------------------------------

/// The debug parameter `mb shot` sets to check grounding.
pub const PARAM: &str = "mb.ground";
/// The mark prefix of what's reported.
pub const MARK: &str = "ground:";
/// The debug parameter `mb shot --taps` sets to draw tap areas over the frame.
pub const TAPS_PARAM: &str = "mb.taps";
/// Marks a frame may use for it (the runtime keeps 8 a frame; the game's own come first).
const MARKS: usize = 6;
/// The runtime keeps a mark's first 64 bytes.
const MARK_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Note {
    Figure { name: &'static str, l: f32, t: f32, r: f32, b: f32, fx: f32, base: f32, layer: i8, shadow: bool, cropped: bool, lifted: bool },
    Floor { y: f32, x0: f32, x1: f32 },
    Tap { name: &'static str, l: f32, t: f32, r: f32, b: f32 },
}

/// A name as a record carries it: letters, digits and '_', 12 at most.
fn short(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(12).collect()
}

impl Note {
    fn text(&self) -> String {
        match *self {
            Note::Figure { name, l, t, r, b, fx, base, layer, shadow, cropped, lifted } => {
                // Flags: shadow, cropped, lifted, then the layer + 4 in 3 bits (0: none given, as layer 0).
                let flags = shadow as u8 | (cropped as u8) << 1 | (lifted as u8) << 2 | ((layer.clamp(-3, 3) + 4) as u8) << 3;
                format!("f{},{},{},{},{},{flags},{},{}", short(name), l.round(), t.round(), r.round(), b.round(), fx.round(), base.round())
            }
            Note::Floor { y, x0, x1 } => format!("F{},{},{}", y.round(), x0.round(), x1.round()),
            Note::Tap { name, l, t, r, b } => format!("t{},{},{},{},{}", short(name), l.round(), t.round(), r.round(), b.round()),
        }
    }
}

/// Notes packed into marks of at most 64 bytes, at most `marks` of them:
/// the records joined by ';' as one stream, cut into pieces that each start
/// with "ground:" (a record may run on into the next mark: `mb shot` joins
/// them in order). A last "+N" counts what didn't fit.
fn pack(notes: &[String], marks: usize) -> Vec<String> {
    let piece = MARK_BYTES - MARK.len();
    let room = piece * marks;
    let mut stream = String::new();
    let mut i = 0;
    while i < notes.len() {
        let sep = !stream.is_empty() as usize;
        // Room for a last ";+N" if anything is left out.
        let rest = if i + 1 < notes.len() { 5 } else { 0 };
        if stream.len() + sep + notes[i].len() + rest > room {
            break;
        }
        if sep == 1 {
            stream.push(';');
        }
        stream.push_str(&notes[i]);
        i += 1;
    }
    if i < notes.len() {
        stream.push_str(&format!("{}+{}", if stream.is_empty() { "" } else { ";" }, notes.len() - i));
    }
    // Records are ASCII: any byte is a cut point.
    stream.as_bytes().chunks(piece).map(|c| format!("{MARK}{}", String::from_utf8_lossy(c))).collect()
}

thread_local! {
    static ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TAPS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NOTES: std::cell::RefCell<Vec<Note>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn checking() -> bool {
    ON.with(std::cell::Cell::get)
}

fn taps_shown() -> bool {
    TAPS.with(std::cell::Cell::get)
}

fn note(n: Note) {
    NOTES.with_borrow_mut(|v| {
        if v.len() < 32 {
            v.push(n);
        }
    });
}

#[doc(hidden)]
/// Before `render` (export_game!): whether `mb shot` is checking, and a
/// fresh transform (the host resets its own every frame).
pub fn __begin_frame() {
    let on = crate::sys::param(PARAM).is_some();
    ON.with(|c| c.set(on));
    TAPS.with(|c| c.set(crate::sys::param(TAPS_PARAM).is_some()));
    track::reset();
    NOTES.with_borrow_mut(Vec::clear);
}

#[doc(hidden)]
/// After `render`: reports what stood where.
pub fn __end_frame() {
    if !checking() {
        return;
    }
    let mut notes = NOTES.with_borrow_mut(std::mem::take);
    // Floors and tap areas first (a figure is checked against them), figures in the order drawn.
    notes.sort_by_key(|n| match n {
        Note::Floor { .. } => 0,
        Note::Tap { .. } => 1,
        Note::Figure { .. } => 2,
    });
    let notes: Vec<String> = notes.iter().map(Note::text).collect();
    for m in pack(&notes, MARKS) {
        crate::sys::mark(&m);
    }
}

/// The mb2d transform, followed while `mb shot` checks grounding, so what's
/// reported is where it is on screen.
pub(crate) mod track {
    use std::cell::RefCell;

    /// [a, b, c, d, e, f]: x' = a·x + c·y + e, y' = b·x + d·y + f.
    type M = [f32; 6];
    const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    thread_local! {
        static STACK: RefCell<(M, Vec<M>)> = const { RefCell::new((ID, Vec::new())) };
    }

    fn on() -> bool {
        super::checking()
    }

    fn then(m: M, n: M) -> M {
        // m · n: n applied first.
        [
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ]
    }

    fn with(n: M) {
        if on() {
            STACK.with_borrow_mut(|(m, _)| *m = then(*m, n));
        }
    }

    pub(crate) fn reset() {
        STACK.with_borrow_mut(|s| *s = (ID, Vec::new()));
    }

    pub(crate) fn push() {
        if on() {
            STACK.with_borrow_mut(|(m, st)| {
                if st.len() < 64 {
                    st.push(*m);
                }
            });
        }
    }

    pub(crate) fn pop() {
        if on() {
            STACK.with_borrow_mut(|(m, st)| {
                if let Some(p) = st.pop() {
                    *m = p;
                }
            });
        }
    }

    pub(crate) fn translate(x: f32, y: f32) {
        with([1.0, 0.0, 0.0, 1.0, x, y]);
    }

    pub(crate) fn rotate(r: f32) {
        let (s, c) = r.sin_cos();
        with([c, s, -s, c, 0.0, 0.0]);
    }

    pub(crate) fn scale(sx: f32, sy: f32) {
        with([sx, 0.0, 0.0, sy, 0.0, 0.0]);
    }

    pub(crate) fn apply(p: [f32; 2]) -> [f32; 2] {
        STACK.with_borrow(|(m, _)| [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIRL: Footing = Footing { name: "girl", cell: [128.0, 128.0], cols: 4, foot: [64.0, 120.0], top: 20.0, base: 30.0, span: [44.0, 90.0], height: 50.0, real: 1.25 };

    #[test]
    fn a_figure_stands_on_its_feet_not_its_images_bottom() {
        let f = Figure::new(Image(1), GIRL);
        // 100 texels top to feet drawn 50 units tall: half a unit a texel.
        let r = f.rect(100.0, 400.0);
        assert_eq!(r, [100.0 - 32.0, 400.0 - 60.0, 64.0, 64.0]);
        // The image's bottom 8 texels (4 units) hang below the floor: the feet are on it.
        assert_eq!(r[1] + r[3], 404.0);
        // Twice as tall, and mirrored around the feet.
        let r = Figure::new(Image(1), Footing { foot: [40.0, 120.0], ..GIRL }).height(100.0).flip(true).rect(100.0, 400.0);
        assert_eq!(r, [100.0 - 88.0 * 1.0, 400.0 - 120.0, 128.0, 128.0]);
        assert_eq!(Figure::new(Image(1), GIRL).scale(1.5).height, 75.0);
        // The bounds it returns are the figure, top to feet.
        assert_eq!(Figure::new(Image(1), GIRL).draw(100.0, 400.0), [92.5, 350.0, 15.0, 50.0]);
    }

    #[test]
    fn frames_are_cells_in_rows() {
        let f = Figure::new(Image(1), GIRL).frame(5);
        assert_eq!(f.src, Some([128.0, 128.0, 128.0, 128.0]));
        // In an atlas: from the sheet's corner.
        let f = Figure::new(Image(1), GIRL).src([2.0, 2.0, 512.0, 128.0]).frame(2);
        assert_eq!(f.src, Some([258.0, 2.0, 128.0, 128.0]));
        assert_eq!(Figure::new(Image(1), GIRL).src([2.0, 2.0, 512.0, 128.0]).src, Some([2.0, 2.0, 128.0, 128.0]));
        assert_eq!(Footing::bottom("crate", 64.0, 48.0).foot, [32.0, 48.0]);
        assert_eq!(Footing::bottom("crate", 64.0, 48.0).height, 24.0);
    }

    #[test]
    fn the_body_and_tap_target_are_whats_drawn_head_included() {
        let f = Figure::new(Image(1), GIRL);
        // Columns 44–90 around feet at 64, rows 20–120, half a unit a texel.
        assert_eq!(f.body(100.0, 400.0), [90.0, 350.0, 23.0, 50.0]);
        // Mirrored: the same columns on the other side of the feet.
        assert_eq!(f.flip(true).body(100.0, 400.0), [87.0, 350.0, 23.0, 50.0]);
        // Lifted: the box goes up with it.
        assert_eq!(f.lift(10.0).body(100.0, 400.0)[1], 340.0);
        // A tap target is at least 44 across, around the body, from the head to the feet and beyond.
        let hit = f.hit(100.0, 400.0);
        assert_eq!(hit, [79.5, 350.0, 44.0, 50.0]);
        assert!(hits(hit, 101.0, 352.0), "a tap on her face counts");
        assert!(!hits(hit, 101.0, 345.0));
        // A small prop's target grows to 44 each way around its middle.
        let bell = Figure::new(Image(1), Footing::bottom("bell", 20.0, 20.0));
        assert_eq!(bell.hit(50.0, 100.0), [28.0, 73.0, 44.0, 44.0]);
    }

    #[test]
    fn props_are_sized_against_the_hero_in_meters() {
        // The girl: 50 units for 1.25 m, 40 units a meter.
        let scale = Scale::of(GIRL);
        assert_eq!(scale.units_per_m, 40.0);
        let mirror = Footing::bottom("mirror", 60.0, 100.0).real(1.6);
        assert_eq!(Figure::new(Image(1), mirror).sized(scale).height, 64.0);
        // A bell is drawn bigger than life (at least HANDHELD_MIN), not 6 units.
        assert_eq!(scale.height(Footing::bottom("bell", 20.0, 20.0).real(0.15)), HANDHELD_MIN);
        // Unknown: its own height.
        assert_eq!(scale.height(Footing::bottom("orb", 20.0, 30.0)), 15.0);
        assert_eq!(Scale::of_height(GIRL, 75.0).units(2.0), 120.0);
    }

    #[test]
    fn walking_up_to_something_stops_half_a_body_short() {
        let chair = [200.0, 360.0, 40.0, 40.0];
        assert_eq!(stop_short(50.0, 24.0, chair), 176.0);
        assert_eq!(stop_short(400.0, 24.0, chair), 264.0);
    }

    #[test]
    fn a_scene_draws_back_to_front() {
        let order = std::cell::RefCell::new(Vec::new());
        let mut scene = Scene::new();
        scene.other(0, 420.0, || order.borrow_mut().push("near chair"));
        scene.other(1, 400.0, || order.borrow_mut().push("girl"));
        scene.other(0, 380.0, || order.borrow_mut().push("far mirror"));
        scene.other(-1, 500.0, || order.borrow_mut().push("back wall"));
        scene.other(0, 380.0, || order.borrow_mut().push("far clock"));
        scene.draw();
        assert_eq!(*order.borrow(), ["back wall", "far mirror", "far clock", "near chair", "girl"]);
        let mut scene = Scene::new();
        assert_eq!(scene.figure(Figure::new(Image(1), GIRL).layer(9), 100.0, 400.0), [90.0, 350.0, 23.0, 50.0]);
        assert_eq!(scene.items[0].0, 3, "layers are -3 to 3");
    }

    #[test]
    fn tap_areas_are_reported_where_they_are() {
        ON.with(|c| c.set(true));
        track::reset();
        NOTES.with_borrow_mut(Vec::clear);
        track::translate(-100.0, 0.0);
        tap_area("witch", [150.0, 380.0, 60.0, 140.0]);
        assert_eq!(NOTES.with_borrow(|n| n[0]), Note::Tap { name: "witch", l: 50.0, t: 380.0, r: 110.0, b: 520.0 });
        assert_eq!(tap_size([0.0, 0.0, 10.0, 100.0]), [-17.0, 0.0, 44.0, 100.0]);
        ON.with(|c| c.set(false));
        track::reset();
        NOTES.with_borrow_mut(Vec::clear);
    }

    #[test]
    fn a_backdrops_floor_scales_with_it() {
        let hall = Backdrop { name: "hall", size: [2880.0, 1280.0], floor: 1088.0 };
        assert_eq!(hall.floor_at(0.0, 640.0), 544.0);
        assert_eq!(hall.floor_at(40.0, 320.0), 312.0);
    }

    #[test]
    fn notes_pack_into_few_short_marks() {
        let fig = Note::Figure { name: "witch-grandma!", l: 10.4, t: 99.6, r: 80.0, b: 300.0, fx: 45.2, base: 30.0, layer: -1, shadow: true, cropped: true, lifted: false }.text();
        assert_eq!(fig, "fwitchgrandma,10,100,80,300,27,45,30");
        assert_eq!(Note::Tap { name: "witch", l: 1.0, t: 2.0, r: 3.0, b: 4.0 }.text(), "twitch,1,2,3,4");
        assert_eq!(Note::Floor { y: 544.2, x0: 0.0, x1: 360.0 }.text(), "F544,0,360");
        let notes: Vec<String> = (0..12).map(|i| format!("fdoll{i},100,200,140,{},1,120,20", 300 + i)).collect();
        let marks = pack(&notes, 3);
        assert_eq!(marks.len(), 3);
        assert!(marks.iter().all(|m| m.starts_with(MARK) && m.len() <= MARK_BYTES), "{marks:?}");
        // Joined in order, the records are whole again; the last counts what didn't fit.
        let joined: String = marks.iter().map(|m| &m[MARK.len()..]).collect();
        let recs: Vec<&str> = joined.split(';').collect();
        let shown = recs.iter().filter(|r| r.starts_with('f')).count();
        assert_eq!(shown, 5, "{marks:?}");
        assert_eq!(recs[..shown], notes[..shown].iter().map(String::as_str).collect::<Vec<_>>()[..]);
        assert_eq!(*recs.last().unwrap(), format!("+{}", 12 - shown));
        assert_eq!(pack(&notes[..2], 3).len(), 2, "two records run into a second mark");
        assert_eq!(pack(&["F1,0,9".into()], 3), vec!["ground:F1,0,9".to_string()]);
        assert!(pack(&[], 3).is_empty());
    }

    #[test]
    fn the_transform_is_followed_while_checking() {
        ON.with(|c| c.set(true));
        track::reset();
        track::translate(-500.0, 10.0);
        track::push();
        track::scale(2.0, 2.0);
        assert_eq!(track::apply([10.0, 5.0]), [-480.0, 20.0]);
        track::pop();
        assert_eq!(track::apply([10.0, 5.0]), [-490.0, 15.0]);
        track::rotate(std::f32::consts::FRAC_PI_2);
        let p = track::apply([1.0, 0.0]);
        assert!((p[0] + 500.0).abs() < 1e-4 && (p[1] - 11.0).abs() < 1e-4, "{p:?}");
        // A figure drawn under it is reported where it is on screen.
        track::reset();
        track::translate(-500.0, 0.0);
        NOTES.with_borrow_mut(Vec::clear);
        Figure::new(Image(1), GIRL).draw(600.0, 400.0);
        floor_line(400.0, 500.0, 860.0);
        let notes = NOTES.with_borrow(|n| n.clone());
        // Its body: columns 44–90 of the cell, its feet at column 64, half a unit a texel.
        assert_eq!(notes[0], Note::Figure { name: "girl", l: 90.0, t: 350.0, r: 113.0, b: 400.0, fx: 100.0, base: 15.0, layer: 0, shadow: true, cropped: false, lifted: false });
        assert_eq!(notes[1], Note::Floor { y: 400.0, x0: 0.0, x1: 360.0 });
        // A jump: lifted, reported over the floor it's above.
        NOTES.with_borrow_mut(Vec::clear);
        let f = Figure::new(Image(1), GIRL).lift(30.0);
        assert_eq!(f.rect(600.0, 400.0)[1], 340.0 - 30.0);
        f.draw(600.0, 400.0);
        assert!(matches!(NOTES.with_borrow(|n| n[0]), Note::Figure { lifted: true, b: 400.0, t: 320.0, .. }), "{:?}", NOTES.with_borrow(|n| n[0]));
        // A src cut above the feet is reported.
        NOTES.with_borrow_mut(Vec::clear);
        Figure::new(Image(1), GIRL).src([0.0, 0.0, 128.0, 70.0]).draw(600.0, 400.0);
        assert!(matches!(NOTES.with_borrow(|n| n[0]), Note::Figure { cropped: true, .. }));
        ON.with(|c| c.set(false));
        track::reset();
        NOTES.with_borrow_mut(Vec::clear);
        Figure::new(Image(1), GIRL).draw(600.0, 400.0);
        assert!(NOTES.with_borrow(Vec::is_empty), "nothing reported when not checking");
    }
}
