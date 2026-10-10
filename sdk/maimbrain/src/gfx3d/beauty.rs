//! The beauty pass (mb3d 2): the last layer of polish over a well-lit
//! scene. Each call sets its look until changed, and each struct's zeroed
//! form ([`Grade::none`], `LightShafts::default()`, …) is off, so a game
//! that never calls these looks exactly as before.
//!
//! - [`grade`]: color grading of the final image (lift/gamma/gain, split
//!   toning, contrast with a filmic toe and shoulder, a baked 3D LUT).
//! - [`shafts`]: light shafts (god rays) from the sun or moon, through the
//!   gaps in whatever stands in front of it, thickest in the fog.
//! - [`halos`]: point lights glowing in the air around them (lanterns in
//!   mist), cut off by whatever is in front of them.
//! - [`film`]: film grain and anamorphic streaks from the brightest points.
//!
//! Tune them in this order, each against `mb shot` sheets of mid-play: the
//! lighting and sky, then fog, then the grade, then shafts and halos, then
//! bloom, then grain. Readability first: the course, the player and the
//! hazards must stay the clearest things on screen.
//!
//! ```ignore
//! gfx3d::grade(&Grade::moonlit());
//! gfx3d::shafts(&LightShafts::moonbeams());
//! gfx3d::halos(&Halos::mist());
//! gfx3d::film(&Film::subtle());
//! ```
//!
//! Cost on an iPhone 13 class GPU (MB3D.md "The beauty pass"): the grade and
//! grain are a few dozen ALU per pixel in the composite; halos and the
//! shafts' source are one half-resolution pass (plus keeping the scene's
//! depth, as depth of field does), the shafts and streaks quarter-resolution
//! passes of 40 and 3 × 17 taps.

use super::{Texture, ptr};
use crate::ffi;

/// Color grading (96 bytes, mb3d 2), applied to the tonemapped image in
/// display values (0–1, sRGB-encoded), after `Post`'s exposure, saturation
/// and contrast and before the vignette. In order: lift (`shadows`), gain
/// (`highlights`) and gamma (`midtones`) per channel, split toning,
/// vibrance, an S-curve `contrast` with a filmic `toe` and `shoulder`, then
/// the LUT; `amount` mixes the result with the ungraded image.
///
/// `Grade::default()` is a neutral grade at full `amount` (set the fields
/// you want); [`Grade::none`] (all zero) turns grading off. Presets:
/// [`Grade::moonlit`], [`Grade::golden_hour`], [`Grade::neon`]. Blend two
/// with [`Grade::lerp`] (a dusk that turns to night).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Grade {
    /// Lift per channel (−0.5–0.5; 0 = none): raises (or lowers) the blacks
    /// toward this color, leaving white alone. `[0.0, 0.01, 0.03]` gives
    /// blue-teal shadows.
    pub shadows: [f32; 3],
    /// −1–1: an S-curve around mid-grey (0.3 is punchy, −0.3 flat). Never clips.
    pub contrast: f32,
    /// Gamma per channel (−1–1; 0 = none): brightens (or darkens) the
    /// midtones in that channel, leaving black and white alone.
    pub midtones: [f32; 3],
    /// −1–1: how the shadows roll into black. Positive deepens them (rich,
    /// inky blacks), negative lifts and flattens them (a faded print).
    pub toe: f32,
    /// Gain per channel (−1–2; 0 = none): scales toward white, leaving black
    /// alone. `[0.06, 0.02, -0.03]` warms the highlights.
    pub highlights: [f32; 3],
    /// −1–1: how the highlights roll into white. Positive lifts them into
    /// a soft glow, negative holds them down (more detail at the top).
    pub shoulder: f32,
    /// A color whose hue tints the shadows; how saturated it is is how much
    /// (it doesn't change brightness). Linear 0–1; zero = none.
    pub shadow_tint: [f32; 3],
    /// −1–1: where the shadows end and the highlights begin. Positive gives
    /// more of the image to the highlight tint.
    pub balance: f32,
    /// A color whose hue tints the highlights (as `shadow_tint`).
    pub highlight_tint: [f32; 3],
    /// −1–1: saturation that favors dull colors (skin and already-vivid
    /// colors change least).
    pub vibrance: f32,
    /// A 3D LUT baked by hand (the last step of the grade): an N²×N strip
    /// texture (N = 2–64, e.g. 1024×32: blue slices left to right, red across
    /// each slice, green down). Make the neutral strip with `tools/lut.py`,
    /// grade it in any photo editor, ship the PNG. None = no LUT.
    pub lut: Option<Texture>,
    /// 0–1: how much of the LUT (0 = all of it).
    pub lut_amount: f32,
    /// 0–1: how much of the whole grade (0 = off).
    pub amount: f32,
    pub _reserved: u32,
}

impl Default for Grade {
    /// Neutral, at full amount: set what you need.
    fn default() -> Grade {
        Grade { amount: 1.0, ..Grade::none() }
    }
}

impl Grade {
    /// No grading (all zero): what a game that never calls [`grade`] gets.
    pub const fn none() -> Grade {
        Grade {
            shadows: [0.0; 3],
            contrast: 0.0,
            midtones: [0.0; 3],
            toe: 0.0,
            highlights: [0.0; 3],
            shoulder: 0.0,
            shadow_tint: [0.0; 3],
            balance: 0.0,
            highlight_tint: [0.0; 3],
            vibrance: 0.0,
            lut: None,
            lut_amount: 0.0,
            amount: 0.0,
            _reserved: 0,
        }
    }

    /// Night lit by the moon and lanterns: blue-teal shadows, warm lantern
    /// highlights, inky blacks, a gentle S-curve.
    pub fn moonlit() -> Grade {
        Grade {
            shadows: [0.0, 0.008, 0.022],
            contrast: 0.12,
            midtones: [-0.03, 0.0, 0.04],
            toe: 0.3,
            highlights: [0.03, 0.01, -0.015],
            shoulder: 0.2,
            shadow_tint: [0.08, 0.3, 0.36],
            balance: -0.5,
            highlight_tint: [0.4, 0.28, 0.14],
            vibrance: 0.2,
            ..Grade::default()
        }
    }

    /// Late sun: warm gold highlights, violet shadows, soft rolled-off whites.
    pub fn golden_hour() -> Grade {
        Grade {
            shadows: [0.012, 0.0, 0.025],
            contrast: 0.1,
            midtones: [0.06, 0.02, -0.04],
            toe: 0.15,
            highlights: [0.08, 0.03, -0.05],
            shoulder: 0.35,
            shadow_tint: [0.24, 0.14, 0.34],
            balance: 0.1,
            highlight_tint: [0.45, 0.3, 0.1],
            vibrance: 0.25,
            ..Grade::default()
        }
    }

    /// Neon nights: deep blue shadows, magenta highlights, hard contrast.
    pub fn neon() -> Grade {
        Grade {
            shadows: [0.0, 0.0, 0.03],
            contrast: 0.3,
            midtones: [0.02, -0.04, 0.05],
            toe: 0.45,
            highlights: [0.06, -0.02, 0.06],
            shoulder: 0.2,
            shadow_tint: [0.08, 0.15, 0.42],
            balance: -0.1,
            highlight_tint: [0.45, 0.12, 0.4],
            vibrance: 0.4,
            ..Grade::default()
        }
    }

    /// The grade between `a` (t = 0) and `b` (t = 1): every number mixed
    /// (the LUT is `b`'s past halfway).
    pub fn lerp(a: &Grade, b: &Grade, t: f32) -> Grade {
        let t = t.clamp(0.0, 1.0);
        let m = |x: f32, y: f32| x * (1.0 - t) + y * t;
        let m3 = |x: [f32; 3], y: [f32; 3]| [m(x[0], y[0]), m(x[1], y[1]), m(x[2], y[2])];
        Grade {
            shadows: m3(a.shadows, b.shadows),
            contrast: m(a.contrast, b.contrast),
            midtones: m3(a.midtones, b.midtones),
            toe: m(a.toe, b.toe),
            highlights: m3(a.highlights, b.highlights),
            shoulder: m(a.shoulder, b.shoulder),
            shadow_tint: m3(a.shadow_tint, b.shadow_tint),
            balance: m(a.balance, b.balance),
            highlight_tint: m3(a.highlight_tint, b.highlight_tint),
            vibrance: m(a.vibrance, b.vibrance),
            lut: if t < 0.5 { a.lut } else { b.lut },
            lut_amount: m(a.lut_amount, b.lut_amount),
            amount: m(a.amount, b.amount),
            _reserved: 0,
        }
    }
}

/// Sets the grade until changed (mb3d 2). A few dozen ALU per pixel in the
/// final composite; a LUT adds two texture samples.
pub fn grade(g: &Grade) {
    unsafe { ffi::mb3d_grade(ptr(g)) }
}

/// Light shafts, or god rays (48 bytes, mb3d 2): light from the sun or moon
/// streaming past whatever stands in front of it — moonbeams through a torii
/// and the mist, sunbeams through trees. The host takes a glow around the
/// light where the sky shows (or fog hides what's behind), blurs it toward
/// the light in screen space, and adds it to the image; anything in front
/// of the glow casts a dark ray, the gaps between bright ones.
///
/// They need the light near the view (on screen or just off it) and
/// something with gaps in front of it; they fade out as the camera turns
/// away. `LightShafts::default()` (intensity 0) is off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct LightShafts {
    /// Linear color (0 = the sun's color).
    pub color: [f32; 3],
    /// How bright (0 = off). 0.2–0.6 for moonlight, more for a low sun.
    pub intensity: f32,
    /// 0.5–1: how fast the rays fade away from the light (0 = 0.95; nearer 1 is longer).
    pub decay: f32,
    /// 0–1: how far toward the light each pixel looks (0 = 0.7): longer rays, softer.
    pub length: f32,
    /// Radians: the size of the glow around the light the rays come from (0 = 0.3).
    pub size: f32,
    /// 0–1: rays only as thick as the fog in front of what each pixel shows,
    /// so they hang in the mist over the distance and thin out up close (0 = even).
    pub fog: f32,
    /// Toward the light (zero = the sky's light: the sun, or the sky disc's direction).
    pub direction: [f32; 3],
    pub _reserved: u32,
}

impl LightShafts {
    /// Pale moonbeams in the mist.
    pub fn moonbeams() -> LightShafts {
        LightShafts { intensity: 0.6, decay: 0.97, length: 0.85, size: 0.18, fog: 0.5, ..Default::default() }
    }
    /// Strong warm sunbeams for a low sun.
    pub fn sunbeams() -> LightShafts {
        LightShafts { intensity: 0.8, decay: 0.95, length: 0.8, size: 0.3, fog: 0.4, ..Default::default() }
    }
}

/// Sets the light shafts until changed (mb3d 2). One half-resolution pass
/// (shared with [`halos`]) and a quarter-resolution pass of 32 taps; the
/// scene's depth is kept for them (as for depth of field).
pub fn shafts(s: &LightShafts) {
    unsafe { ffi::mb3d_shafts(ptr(s)) }
}

/// Halos around point lights (32 bytes, mb3d 2): the light each point light
/// scatters in the air toward the camera, worked out along every pixel's
/// view ray up to what the pixel shows — lanterns glowing in mist. Unlike
/// bloom (a blur of bright pixels), a halo hangs in the air around the
/// light: it grows with the fog's density, is cut off by anything in front
/// of the light, and spreads over what's behind it. Every point light with
/// intensity above 0 gets one (≤ 8), as bright as its intensity.
/// `Halos::default()` (intensity 0) is off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Halos {
    /// How bright (0 = off; 1 = what the air would scatter; 2–6 reads on a phone).
    pub intensity: f32,
    /// Meters: the bright core's radius, about the size of the lamp (0 = 0.15).
    pub size: f32,
    /// Scattering per meter (0 = the fog's density, or 0.04 without fog).
    pub density: f32,
    /// Multiplies each light's color (0 = white).
    pub tint: [f32; 3],
    /// Meters from the light where its halo has faded (0 = 2). Mist
    /// scatters light mostly forward, so a halo hangs close around its
    /// lamp: 1–3 m for lanterns, more for a floodlight in thick fog.
    pub reach: f32,
    pub _reserved: u32,
}

impl Halos {
    /// Lanterns in mist.
    pub fn mist() -> Halos {
        Halos { intensity: 3.0, size: 0.2, ..Default::default() }
    }
}

/// Sets the halos until changed (mb3d 2). A few dozen ALU per light per
/// pixel at half resolution.
pub fn halos(h: &Halos) {
    unsafe { ffi::mb3d_halos(ptr(h)) }
}

/// Film touches (32 bytes, mb3d 2): grain, and anamorphic streaks from the
/// brightest points. `Film::default()` is off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Film {
    /// 0–1: grain strength (0 = none; 0.03–0.06 is film, more is a style).
    /// It moves 24 times a second of game time, so replays and screenshots repeat.
    pub grain: f32,
    /// Grain size in logical pixels (0 = 1.5).
    pub grain_size: f32,
    /// 0–4: anamorphic streaks from the brightest points (0 = none; 0.2–0.6 is subtle).
    pub streaks: f32,
    /// How bright a point must be to streak, in scene units before exposure
    /// (0 = 3: lamps, the sun's glint; not lit surfaces).
    pub streak_threshold: f32,
    /// Linear color (0 = an anamorphic blue).
    pub streak_color: [f32; 3],
    /// 0–1 of the screen's width (0 = 0.3).
    pub streak_length: f32,
}

impl Film {
    /// A little grain and faint streaks.
    pub fn subtle() -> Film {
        Film { grain: 0.04, streaks: 0.15, ..Default::default() }
    }
}

/// Sets grain and streaks until changed (mb3d 2). Grain is part of the
/// composite (a few ALU); streaks are three quarter-resolution passes.
pub fn film(f: &Film) {
    unsafe { ffi::mb3d_film(ptr(f)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_struct_sizes_match_the_spec() {
        assert_eq!(size_of::<Grade>(), 96);
        assert_eq!(size_of::<LightShafts>(), 48);
        assert_eq!(size_of::<Halos>(), 32);
        assert_eq!(size_of::<Film>(), 32);
        assert_eq!(std::mem::offset_of!(Grade, lut), 80);
        assert_eq!(std::mem::offset_of!(Grade, amount), 88);
        assert_eq!(std::mem::offset_of!(LightShafts, direction), 32);
        assert_eq!(std::mem::offset_of!(Film, streak_color), 16);
    }

    #[test]
    fn none_is_all_zero_and_default_is_neutral() {
        let none = Grade::none();
        // SAFETY: a padding-free #[repr(C)] struct of f32/u32 (and a niche-optimized Option).
        let bytes = unsafe { std::slice::from_raw_parts(ptr(&none), size_of::<Grade>()) };
        assert!(bytes.iter().all(|&b| b == 0));
        assert_eq!(Grade::default(), Grade { amount: 1.0, ..none });
    }

    #[test]
    fn grades_blend() {
        let (a, b) = (Grade::moonlit(), Grade::golden_hour());
        assert_eq!(Grade::lerp(&a, &b, 0.0), a);
        assert_eq!(Grade::lerp(&a, &b, 1.0), b);
        let mid = Grade::lerp(&a, &b, 0.5);
        assert!((mid.toe - (a.toe + b.toe) / 2.0).abs() < 1e-6);
    }

    /// The skill's beauty pass example compiles and runs off-wasm.
    #[test]
    fn skill_beauty_example() {
        let lut_asset = crate::sys::Asset::load("assets/grade.png");
        grade(&Grade::moonlit());
        shafts(&LightShafts::moonbeams());
        halos(&Halos { reach: 1.5, ..Halos::mist() });
        film(&Film::subtle());
        grade(&Grade { lut: Texture::new(lut_asset), ..Grade::default() });
        grade(&Grade { amount: 0.8, ..Grade::lerp(&Grade::golden_hour(), &Grade::moonlit(), 0.5) });
        grade(&Grade::none());
    }
}
