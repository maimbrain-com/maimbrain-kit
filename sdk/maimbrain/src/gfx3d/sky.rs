//! Sky v2 (mb3d 2): layers over the procedural space sky ([`super::Sky`],
//! whose nebula and stars stay and compose with them). Each layer is off
//! while its struct is zeroed (`Default`), and each is set until changed.

use super::{Texture, Vec3, ptr};
use crate::ffi;

/// The atmosphere (80 bytes, mb3d 2): a gradient from the zenith down to
/// the horizon and the ground, a glow band along the horizon and a halo
/// around the sun (or moon, whichever [`super::sun`] is). It replaces the
/// [`super::Sky`]'s background color; the nebula (if any) thins toward the
/// horizon over it, and stars fade out near the horizon and over anything
/// bright (the band, the halo, a day sky).
///
/// Like the sky, it's the source of the scene's ambient and reflected
/// light: changing the gradient or the band re-bakes the lighting over ~6
/// frames, so set them once (or rarely). The halo is drawn per pixel and
/// follows the sun as it moves (the lighting catches up every ~2° of travel).
///
/// ```ignore
/// gfx3d::atmosphere(&Atmosphere::night());
/// gfx3d::atmosphere(&Atmosphere { glow: [0.9, 0.4, 0.15], ..Atmosphere::dusk() });
/// ```
///
/// Colors are linear and may exceed 1. Cost: a few ALU per sky pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Atmosphere {
    /// The sky straight up.
    pub zenith: [f32; 3],
    /// 0–1, the sine of the elevation where the sky is mostly zenith-colored
    /// (0 = 0.2, about 12°): small for a thin horizon haze, large for a sky
    /// that's horizon-colored most of the way up.
    pub horizon_height: f32,
    /// The sky at the horizon.
    pub horizon: [f32; 3],
    /// 0–1: the band's edge, from a soft exponential tail (0) to a
    /// flat-topped band with a crisp edge (1).
    pub band_sharpness: f32,
    /// Below the horizon (what shows where nothing is drawn).
    pub ground: [f32; 3],
    /// 0–1, the sine of the elevation the band reaches (0 = 0.06, about 3.5°).
    pub band_height: f32,
    /// The glow band along the horizon, added (city light, the last of a sunset; 0 = none).
    pub band: [f32; 3],
    /// Radius of the halo around the sun or moon, radians (0 = 0.25).
    pub glow_size: f32,
    /// The halo around the sun or moon, added (0 = none).
    pub glow: [f32; 3],
    pub _reserved: u32,
}

impl Atmosphere {
    /// A clear night: near-black blue overhead, a faint blue haze at the
    /// horizon, a pale halo around the moon.
    pub fn night() -> Atmosphere {
        Atmosphere {
            zenith: [0.002, 0.004, 0.016],
            horizon_height: 0.25,
            horizon: [0.02, 0.035, 0.08],
            band_sharpness: 0.3,
            ground: [0.004, 0.006, 0.012],
            band_height: 0.05,
            band: [0.012, 0.02, 0.045],
            glow_size: 0.3,
            glow: [0.04, 0.05, 0.075],
            _reserved: 0,
        }
    }

    /// Dusk: deep blue overhead, orange at the horizon with a bright band,
    /// a warm halo around a low sun.
    pub fn dusk() -> Atmosphere {
        Atmosphere {
            zenith: [0.01, 0.022, 0.11],
            horizon_height: 0.14,
            horizon: [0.5, 0.17, 0.07],
            band_sharpness: 0.35,
            ground: [0.03, 0.02, 0.03],
            band_height: 0.045,
            band: [0.45, 0.15, 0.035],
            glow_size: 0.3,
            glow: [0.8, 0.35, 0.1],
            _reserved: 0,
        }
    }

    /// A clear day: blue overhead, pale at the horizon, a white-gold halo around the sun.
    pub fn day() -> Atmosphere {
        Atmosphere {
            zenith: [0.08, 0.22, 0.65],
            horizon_height: 0.3,
            horizon: [0.55, 0.7, 0.92],
            band_sharpness: 0.0,
            ground: [0.22, 0.22, 0.24],
            band_height: 0.08,
            band: [0.15, 0.15, 0.15],
            glow_size: 0.2,
            glow: [1.2, 1.05, 0.8],
            _reserved: 0,
        }
    }
}

/// Sets the atmosphere until changed (mb3d 2); `&Atmosphere::default()` turns it off.
pub fn atmosphere(a: &Atmosphere) {
    unsafe { ffi::mb3d_atmosphere(ptr(a)) }
}

/// A cloud layer (64 bytes, mb3d 2): soft 2D clouds on a dome over the
/// camera, drifting with game time (so replays match), lit from the sun's
/// (or moon's) direction: `color` on the side facing it, `shade` on the
/// side away, and a silver lining on thin edges when you look toward it.
/// They hide the stars and the sun disc behind them, fade into the sky
/// toward the horizon, and the reflector's water mirrors them.
///
/// ```ignore
/// gfx3d::clouds(&Clouds { coverage: 0.45, ..Clouds::night() });
/// ```
///
/// Changing anything but `speed` and `direction` re-bakes the scene's
/// lighting (which sees the clouds as an even haze of their average color),
/// so set them once or rarely; drifting is free. Cost: four texture
/// samples and a few dozen ALU per sky pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Clouds {
    /// 0–1: how much of the sky is cloud (0 = none, 1 = overcast).
    pub coverage: f32,
    /// 0–1: crisp edges (0) to soft, wispy ones (1).
    pub softness: f32,
    /// How big a cloud is: 1 ≈ 10° across overhead (0 = 1).
    pub size: f32,
    /// 0–1: how much the thickest parts hide behind them (0 = 1).
    pub opacity: f32,
    /// Linear RGB of the side facing the sun or moon.
    pub color: [f32; 3],
    /// 0–2: how bright thin edges glow when you look toward the light.
    pub silver: f32,
    /// Linear RGB of the side away from the light.
    pub shade: [f32; 3],
    /// How fast they drift, in cloud sizes (at size 1) per second; ~0.02 is a breeze.
    pub speed: f32,
    /// Which way they drift, (x, z) (normalized by the host; zero = +x).
    pub direction: [f32; 2],
    /// Picks a different sky of clouds.
    pub seed: u32,
    pub _reserved: u32,
}

impl Clouds {
    /// Scattered clouds lit by a moon: dark blue-grey, silver at the edges near it.
    pub fn night() -> Clouds {
        Clouds {
            coverage: 0.45,
            softness: 0.6,
            size: 1.8,
            opacity: 0.9,
            color: [0.06, 0.07, 0.1],
            silver: 1.0,
            shade: [0.012, 0.016, 0.03],
            speed: 0.03,
            direction: [1.0, -0.4],
            seed: 0,
            _reserved: 0,
        }
    }

    /// Streaks lit orange from underneath by a low sun.
    pub fn dusk() -> Clouds {
        Clouds {
            coverage: 0.45,
            softness: 0.45,
            size: 1.8,
            opacity: 0.95,
            color: [0.9, 0.36, 0.12],
            silver: 1.2,
            shade: [0.09, 0.05, 0.09],
            speed: 0.02,
            direction: [1.0, 0.2],
            seed: 0,
            _reserved: 0,
        }
    }

    /// Fair-weather clouds on a blue sky.
    pub fn day() -> Clouds {
        Clouds {
            coverage: 0.35,
            softness: 0.35,
            size: 1.2,
            opacity: 1.0,
            color: [1.2, 1.18, 1.12],
            silver: 0.6,
            shade: [0.5, 0.55, 0.65],
            speed: 0.03,
            direction: [1.0, 0.0],
            seed: 0,
            _reserved: 0,
        }
    }
}

/// Sets the cloud layer until changed (mb3d 2); `&Clouds::default()` clears the sky.
pub fn clouds(c: &Clouds) {
    unsafe { ffi::mb3d_clouds(ptr(c)) }
}

/// The sun or moon as a disc in the sky (64 bytes, mb3d 2): limb darkening
/// (dimmer toward the edge, as a sphere is), optional dark seas (maria)
/// for a moon, and a halo around it, all procedural. It replaces the
/// [`super::Sky`]'s `sun_disc`. Clouds pass in front of it, and the
/// reflector's water mirrors it (a moon path on rippling water).
///
/// Leave `direction` zero to put it where [`super::sun`]'s light comes from,
/// so the disc, its highlights on shiny things and the halo and cloud
/// lighting agree. A direction of its own moves the halo and the cloud
/// lighting with it, but not the scene's light.
///
/// ```ignore
/// gfx3d::sky_disc(&SkyDisc::moon());
/// ```
///
/// Not part of the scene's ambient lighting (the sun's light is); changing
/// it is free. Cost: a few ALU per sky pixel, plus noise for the few pixels
/// on the disc.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct SkyDisc {
    /// Toward the disc (zero = toward the sun, `−Sun::direction`).
    pub direction: Vec3,
    /// Angular radius, radians (0 = no disc; up to 0.5). The real moon is 0.0045; 0.03–0.05 reads on a phone.
    pub radius: f32,
    /// Linear RGB at the disc's center; above the bloom threshold it glows, and
    /// far above 1 it tonemaps to flat white (a moon ~1–1.5 keeps its seas, a sun 20+).
    pub color: [f32; 3],
    /// 0–1: how much dimmer the edge is than the center.
    pub limb: f32,
    /// Linear RGB of a halo around the disc, added (0 = none).
    pub halo: [f32; 3],
    /// How far past the edge the halo reaches, radians (0 = the radius).
    pub halo_size: f32,
    /// 0–1: dark seas on the disc (a moon's maria; 0 for a sun).
    pub maria: f32,
    /// Picks a different pattern of seas.
    pub seed: u32,
    pub _reserved: [u32; 2],
}

impl SkyDisc {
    /// A full moon, larger than life: pale, limb-darkened, with seas and a soft corona.
    pub fn moon() -> SkyDisc {
        SkyDisc {
            direction: Vec3::ZERO,
            radius: 0.04,
            color: [1.15, 1.17, 1.25],
            limb: 0.4,
            halo: [0.2, 0.24, 0.34],
            halo_size: 0.08,
            maria: 0.7,
            seed: 0,
            _reserved: [0; 2],
        }
    }

    /// A sun: blinding, limb-darkened, with a warm glare.
    pub fn sun() -> SkyDisc {
        SkyDisc {
            direction: Vec3::ZERO,
            radius: 0.025,
            color: [30.0, 26.0, 20.0],
            limb: 0.6,
            halo: [1.6, 1.1, 0.6],
            halo_size: 0.05,
            maria: 0.0,
            seed: 0,
            _reserved: [0; 2],
        }
    }
}

/// Sets the sky disc until changed (mb3d 2); `&SkyDisc::default()` removes it
/// (and brings back the `Sky`'s `sun_disc`, if it has one).
pub fn sky_disc(d: &SkyDisc) {
    unsafe { ffi::mb3d_sky_disc(ptr(d)) }
}

/// How a painted sky is shown (32 bytes, mb3d 2; see [`sky_texture`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct SkyTexture {
    /// Multiplies the painting's (linear) color (0 = 1). Above 1 its bright parts bloom.
    pub intensity: f32,
    /// Turns the painting about +y, radians (its middle faces −z at 0).
    pub yaw: f32,
    /// 0–1: how much of the procedural sky (atmosphere, nebula) shows
    /// instead of the painting everywhere (0 = the painting only).
    pub procedural: f32,
    /// 0–1, the sine of the elevation above which the painting gives way to
    /// the procedural sky over the next ~15° (0 = no fade): a painted
    /// horizon under a procedural night (0.25 ≈ 15° up).
    pub fade: f32,
    pub _reserved: [u32; 4],
}

/// Paints the sky with an equirectangular panorama (mb3d 2): a 2:1 texture
/// whose left and right edges meet behind you, top row straight up, bottom
/// row straight down, middle column straight ahead (−z). `mb art sky`
/// makes one (2048×1024, seamless, calm poles). `None` removes it.
///
/// The painting replaces the background (gradient, nebula) as `procedural`
/// and `fade` say; stars show over its dark parts above ~10°, and clouds,
/// the sky disc and the atmosphere's halo draw over it. It lights the scene
/// like the rest of the sky: setting or changing it re-bakes the lighting
/// over ~6 frames, so set it once. Returns false if `texture` isn't live.
///
/// ```ignore
/// let pano = Texture::new(Asset::load("assets/sky.jpg"))?;   // once it's ready
/// gfx3d::sky_texture(Some(pano), &SkyTexture { fade: 0.3, ..Default::default() });
/// ```
///
/// Cost: baked into the sky cube, so nothing per frame beyond the usual
/// sky; the texture counts toward the 64 textures and 96 MB (a 2048×1024
/// painting is 11 MB with mips).
pub fn sky_texture(texture: Option<Texture>, s: &SkyTexture) -> bool {
    unsafe { ffi::mb3d_sky_texture(texture.map_or(0, |t| t.0.get()), ptr(s)) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_struct_sizes_match_the_spec() {
        assert_eq!(size_of::<Atmosphere>(), 80);
        assert_eq!(size_of::<Clouds>(), 64);
        assert_eq!(size_of::<SkyDisc>(), 64);
        assert_eq!(size_of::<SkyTexture>(), 32);
    }
}
