//! The host 2D engine, `mb2d` (SPEC §5.3). Declare `stdlib = { mb2d = 1 }`
//! in manifest.toml (`mb2d = 2` for game fonts, [`Font::asset`]). Colors are `0xRRGGBBAA`; coordinates are logical units,
//! origin top-left, +y down.

use crate::ffi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Blend {
    Alpha = 0,
    Add = 1,
    Multiply = 2,
    Screen = 3,
}

/// A font: one of the host fonts (`Font::Sans`, `Font::SansBold`,
/// `Font::Pixel`), or a game font baked by `mb font add` into
/// `assets/fonts/<name>.mbf` and named with [`Font::asset`]. Game fonts need
/// `stdlib = { mb2d = 2 }`. A `Font` is a small `Copy` value that works
/// anywhere a font goes (`gfx2d::text`, `ui::text`, a `Theme`, 3D text):
/// until its asset is ready it draws in its fallback, so it can be made in
/// `init` and used from the first frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Font(u32);

/// Vertical metrics of a font, in ems (multiples of the text size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    /// Line top to baseline.
    pub ascent: f32,
    /// Baseline to the bottom of the line box.
    pub descent: f32,
    /// Baseline to baseline.
    pub line: f32,
    /// Height of capital letters above the baseline.
    pub cap: f32,
    pub x_height: f32,
    /// Bitmap (pixel) fonts: pixels per em of their grid; they're crisp at
    /// whole multiples of it. 0 for distance-field fonts.
    pub pixel_em: f32,
}

const INTER: FontMetrics = FontMetrics { ascent: 0.96875, descent: 0.241, line: 1.2099, cap: 0.734, x_height: 0.55, pixel_em: 0.0 };
const PIXEL: FontMetrics = FontMetrics { ascent: 0.875, descent: 0.25, line: 1.25, cap: 0.875, x_height: 0.625, pixel_em: 8.0 };
/// Asset-backed fonts are `LAZY + slot`.
const LAZY: u32 = 0x1000;
const MAX_SLOTS: usize = 16;

#[derive(Clone, Copy)]
enum SlotState {
    Pending,
    Loaded(u32, FontMetrics),
    Failed,
}

#[derive(Clone)]
struct Slot {
    path: String,
    asset: crate::sys::Asset,
    fallback: Font,
    state: SlotState,
}

thread_local! {
    static SLOTS: std::cell::RefCell<Vec<Slot>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Set by `Font::asset`, so `mb2d_font_load`/`_metrics` (mb2d 2) are only
    /// imported by games that load a font; others stay mb2d 1 bundles.
    static LOADER: std::cell::Cell<Option<fn(u32) -> Option<(u32, FontMetrics)>>> = const { std::cell::Cell::new(None) };
}

/// Hands a ready `.mbf` asset to the host: its font id and metrics, or None.
fn load_font(asset: u32) -> Option<(u32, FontMetrics)> {
    let id = unsafe { ffi::mb2d_font_load(asset) };
    let mut m = [0f32; 8];
    (id >= 16 && unsafe { ffi::mb2d_font_metrics(id as u32, m.as_mut_ptr() as *mut u8) } == 0)
        .then(|| (id as u32, FontMetrics { ascent: m[0], descent: m[1], line: m[2], cap: m[3], x_height: m[4], pixel_em: m[5] }))
}

#[allow(non_upper_case_globals)]
impl Font {
    /// Inter Regular (signed distance field: crisp at any size).
    pub const Sans: Font = Font(0);
    /// Inter Bold.
    pub const SansBold: Font = Font(1);
    /// 5×7 pixel font; crisp at sizes that are multiples of 8.
    pub const Pixel: Font = Font(2);

    /// A game font from a `.mbf` asset (`mb font add` writes
    /// `assets/fonts/<name>.mbf`). Draws in `fallback` until the asset is
    /// ready, or if it can't load. Calling it again with the same path gives
    /// the same font. Up to 16 per game.
    ///
    /// Not a `const fn`: the first call starts loading the asset and claims
    /// a slot for it, which needs the host. For a font used everywhere, keep
    /// the path in a `const` and the `Font` in your state (or in the
    /// `Theme`), or call this again where it's needed: it's cheap.
    pub fn asset(path: &str, fallback: Font) -> Font {
        LOADER.with(|l| l.set(Some(load_font)));
        let fallback = Font(fallback.resolve().0);
        SLOTS.with_borrow_mut(|slots| {
            if let Some(i) = slots.iter().position(|s| s.path == path) {
                return Font(LAZY + i as u32);
            }
            if slots.len() >= MAX_SLOTS {
                crate::sys::log(crate::sys::Level::Warn, "Font::asset: more than 16 game fonts; using the fallback");
                return fallback;
            }
            slots.push(Slot { path: path.to_string(), asset: crate::sys::Asset::load(path), fallback, state: SlotState::Pending });
            Font(LAZY + slots.len() as u32 - 1)
        })
    }

    /// Loads the font if its asset just became ready; the id and metrics to draw with now.
    fn resolve(self) -> (u32, FontMetrics) {
        match self.0 {
            0 | 1 => (self.0, INTER),
            2 => (2, PIXEL),
            n if n >= LAZY => SLOTS.with_borrow_mut(|slots| {
                let Some(slot) = slots.get_mut((n - LAZY) as usize) else { return (1, INTER) };
                if let SlotState::Pending = slot.state {
                    match slot.asset.state() {
                        crate::sys::AssetState::Pending => {}
                        crate::sys::AssetState::Failed => slot.state = SlotState::Failed,
                        crate::sys::AssetState::Ready => {
                            // black_box: otherwise the optimizer sees LOADER only ever holds
                            // load_font and calls it directly, importing the mb2d 2 calls everywhere.
                            slot.state = match std::hint::black_box(LOADER.with(std::cell::Cell::get)).and_then(|load| load(slot.asset.0)) {
                                Some((id, m)) => SlotState::Loaded(id, m),
                                None => {
                                    crate::sys::log(crate::sys::Level::Warn, &format!("Font::asset: {} didn't load; using the fallback (does the manifest declare mb2d = 2?)", slot.path));
                                    SlotState::Failed
                                }
                            };
                        }
                    }
                }
                match slot.state {
                    SlotState::Loaded(id, m) => (id, m),
                    _ => slot.fallback.resolve(),
                }
            }),
            n => (n, INTER),
        }
    }

    /// The host font id to draw with right now (the fallback's until a game font is ready).
    pub fn id(self) -> u32 {
        self.resolve().0
    }

    /// Whether this font is drawing as itself (host fonts always are).
    pub fn ready(self) -> bool {
        self.0 < LAZY || self.resolve().0 >= 16
    }

    pub fn metrics(self) -> FontMetrics {
        self.resolve().1
    }

    /// A 1-bit pixel font (outlines and shadows come from offset copies; keep sizes to multiples of `pixel_em`).
    pub fn is_bitmap(self) -> bool {
        self.metrics().pixel_em > 0.0
    }

    /// Pixels per em of a bitmap font's grid (8 for `Font::Pixel`), or 0.
    pub fn pixel_em(self) -> f32 {
        self.metrics().pixel_em
    }

    /// A raw host font id (as `mb2d_text` takes it).
    pub const fn from_id(id: u32) -> Font {
        Font(id)
    }
}

/// An image decoded from a ready PNG asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image(pub u32);

impl Image {
    /// None until the asset is ready, or if it isn't a valid PNG.
    pub fn new(asset: crate::sys::Asset) -> Option<Image> {
        let h = unsafe { ffi::mb2d_image(asset.0) };
        (h > 0).then_some(Image(h as u32))
    }
}

pub fn clear(rgba: u32) {
    unsafe { ffi::mb2d_clear(rgba) }
}

pub fn push() {
    unsafe { ffi::mb2d_push() }
}

pub fn pop() {
    unsafe { ffi::mb2d_pop() }
}

pub fn translate(x: f32, y: f32) {
    unsafe { ffi::mb2d_translate(x, y) }
}

pub fn rotate(radians: f32) {
    unsafe { ffi::mb2d_rotate(radians) }
}

pub fn scale(sx: f32, sy: f32) {
    unsafe { ffi::mb2d_scale(sx, sy) }
}

pub fn blend(mode: Blend) {
    unsafe { ffi::mb2d_blend(mode as u32) }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32, rgba: u32) {
    unsafe { ffi::mb2d_rect(x, y, w, h, rgba) }
}

/// A rectangle shaded from `top` to `bottom`.
pub fn rect_gradient(x: f32, y: f32, w: f32, h: f32, top: u32, bottom: u32) {
    unsafe { ffi::mb2d_rect_gradient(x, y, w, h, top, bottom) }
}

pub fn circle(x: f32, y: f32, r: f32, rgba: u32) {
    unsafe { ffi::mb2d_circle(x, y, r, rgba) }
}

pub fn line(x0: f32, y0: f32, x1: f32, y1: f32, width: f32, rgba: u32) {
    unsafe { ffi::mb2d_line(x0, y0, x1, y1, width, rgba) }
}

/// A filled polygon; `points` is `[x0, y0, x1, y1, ...]`.
pub fn poly(points: &[f32], rgba: u32) {
    unsafe { ffi::mb2d_poly(points.as_ptr(), (points.len() / 2) as u32, rgba) }
}

/// Draws `src` (pixels in the image) into `dst` (logical units), tinted.
pub fn sprite(img: Image, src: [f32; 4], dst: [f32; 4], tint: u32) {
    unsafe { ffi::mb2d_sprite(img.0, src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3], tint) }
}

/// Text with its first line's top at `y`; `size` is the em height in logical
/// units. `\n` starts a new line.
pub fn text(font: Font, size: f32, x: f32, y: f32, rgba: u32, s: &str) {
    unsafe { ffi::mb2d_text(font.id(), size, x, y, rgba, s.as_ptr(), s.len() as u32) }
}

/// Width of the widest line of `s`, in logical units.
pub fn measure(font: Font, size: f32, s: &str) -> f32 {
    unsafe { ffi::mb2d_measure(font.id(), size, s.as_ptr(), s.len() as u32) }
}

/// Text centered horizontally on `cx`.
pub fn text_centered(font: Font, size: f32, cx: f32, y: f32, rgba: u32, s: &str) {
    text(font, size, cx - measure(font, size, s) / 2.0, y, rgba, s);
}

/// A rounded rectangle with signed-distance (anti-aliased) edges, shaded
/// from `top` to `bottom`. `radius` rounds the corners (clamped to half the
/// short side: `radius = h / 2` makes a pill, a square with `radius = w / 2`
/// a circle); `stroke` > 0 draws only a border that wide, inside the edge;
/// `feather` > 0 blurs the edge over that many units (soft shadows, glows).
#[allow(clippy::too_many_arguments)]
pub fn rrect(x: f32, y: f32, w: f32, h: f32, radius: f32, stroke: f32, feather: f32, top: u32, bottom: u32) {
    unsafe { ffi::mb2d_rrect(x, y, w, h, radius, stroke, feather, top, bottom) }
}

/// How SDF text (the Inter fonts; not the pixel font) draws from now until
/// the next call or the end of the frame. All sizes are in ems (multiples of
/// the text size), and together they reach at most ~0.12 em past the glyph:
/// `weight` thickens (> 0) or thins (< 0) the letters, `outline` adds a
/// border that wide in `outline_rgba`, `soft` blurs the outer edge (a glow,
/// or a soft shadow when drawn offset in a dark color).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub weight: f32,
    pub outline: f32,
    pub outline_rgba: u32,
    pub soft: f32,
}

pub fn text_style(s: TextStyle) {
    unsafe { ffi::mb2d_text_style(s.weight, s.outline, s.outline_rgba, s.soft) }
}

/// Anti-aliases `poly` (a pixel-wide soft fringe) and `line` (smooth edges)
/// from now until the next call or the end of the frame. Off by default.
pub fn antialias(on: bool) {
    unsafe { ffi::mb2d_antialias(on as u32) }
}
