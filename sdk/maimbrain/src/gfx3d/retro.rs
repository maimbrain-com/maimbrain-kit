//! The retro look (mb3d 2): 16- and 32-bit console rendering for the 3D
//! scene. The scene is drawn at a low resolution and blown up with square
//! nearest-neighbour pixels; colors are cut to a few bits a channel with an
//! ordered (Bayer) dither; vertices snap to the low-resolution pixel grid
//! (the PS1/Saturn wobble); fog comes in flat bands. The mb2d HUD is drawn
//! afterwards at full resolution, so pixel-art HUDs and portraits stay
//! crisp over it.
//!
//! ```ignore
//! gfx3d::retro(&Retro::saturn());
//! // or tuned:
//! gfx3d::retro(&Retro { vertex_snap: 0.0, ..Retro::arcade() });
//! gfx3d::retro(&Retro::default()); // off
//! ```
//!
//! Pair it with nearest-sampled textures ([`Texture::nearest`](super::Texture::nearest),
//! [`Model::pixelated`](super::Model::pixelated)) so texels read as pixels.

use super::{Texture, Vec3, ptr, vec3};
use crate::ffi;

/// The retro look (32 bytes, mb3d 2). `Retro::default()` is off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Retro {
    /// The pixel budget `(w, h)`, e.g. `(320.0, 240.0)` or `(256.0, 224.0)`:
    /// the short side of the screen gets `min(w, h)` square pixels (on a
    /// portrait phone, 240 across), each a whole number of device pixels,
    /// and the long side as many as fit. Below 16 is off.
    pub resolution: (f32, f32),
    /// Bits per color channel after the composite: 5 is 15-bit color
    /// (5-5-5, the Saturn's and the SNES's), 0 keeps full color.
    pub color_bits: f32,
    /// 0–1: the ordered dither between color levels (0 = flat bands).
    pub dither: f32,
    /// Vertex snapping to a grid of this many low-res pixels (0 = off,
    /// 1 = the PS1/Saturn wobble, 2 = a lot).
    pub vertex_snap: f32,
    /// Fog in this many flat bands (0 = smooth; 6–10 reads as retro).
    pub fog_bands: f32,
    pub _reserved: [f32; 2],
}

impl Retro {
    /// Sega Saturn: 320×224, 15-bit color with a light dither, a little
    /// vertex wobble, banded fog.
    pub fn saturn() -> Retro {
        Retro { resolution: (320.0, 224.0), color_bits: 5.0, dither: 0.6, vertex_snap: 1.0, fog_bands: 8.0, _reserved: [0.0; 2] }
    }

    /// PlayStation: 320×240, 15-bit color with a strong dither, a lot of
    /// wobble, coarse fog bands.
    pub fn ps1() -> Retro {
        Retro { resolution: (320.0, 240.0), color_bits: 5.0, dither: 1.0, vertex_snap: 1.5, fog_bands: 6.0, _reserved: [0.0; 2] }
    }

    /// A 90s arcade board: 384×224, 15-bit color, no wobble, smooth fog.
    pub fn arcade() -> Retro {
        Retro { resolution: (384.0, 224.0), color_bits: 5.0, dither: 0.4, vertex_snap: 0.0, fog_bands: 0.0, _reserved: [0.0; 2] }
    }

    /// SNES-like: 256×224 and 15-bit color with no dither (flat pixel-art colors).
    pub fn snes() -> Retro {
        Retro { resolution: (256.0, 224.0), color_bits: 5.0, dither: 0.0, vertex_snap: 0.0, fog_bands: 8.0, _reserved: [0.0; 2] }
    }
}

/// One parallax backdrop layer (40 bytes, mb3d 2): a texture wrapped
/// around a cylinder about the camera, drawn behind everything (right
/// after the sky, where nothing opaque is), its texels read nearest so
/// pixel art stays crisp. See [`backdrop`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Layer {
    pub texture: Option<Texture>,
    /// [`Layer::FILL_BELOW`]: the bottom row carries on down past the bottom edge.
    pub flags: u32,
    /// How much the layer turns with the world as the camera yaws and
    /// pitches: 1 = fixed in the world (like the sky), 0.3 = slower (far
    /// mountains), 1.5 = faster (near hills), 0 = stuck to the screen.
    pub scroll: (f32, f32),
    /// Texture offset: across in repeats (animate it for drifting clouds),
    /// up in texture heights.
    pub offset: (f32, f32),
    /// How many times the texture repeats around 360° (a whole number; 0 = 1).
    pub repeat_x: f32,
    /// Height against the texture's own aspect (0 = 1).
    pub scale: f32,
    /// The bottom edge as tan(elevation): 0 = on the horizon, −0.05 just
    /// below it (hidden by a floor), 0.2 well up the sky.
    pub y: f32,
    /// sRGB RGBA multiplier, [`srgba`](super::srgba)-style packed `0xAABBGGRR`
    /// (0 = white); tint far layers toward the fog/sky color for depth.
    pub tint: u32,
}

impl Layer {
    pub const FILL_BELOW: u32 = 1;

    /// A layer fixed in the world, once around, its bottom on the horizon.
    pub fn new(texture: Texture) -> Layer {
        Layer { texture: Some(texture), flags: 0, scroll: (1.0, 1.0), offset: (0.0, 0.0), repeat_x: 1.0, scale: 1.0, y: 0.0, tint: 0 }
    }

    /// Turns with the camera's yaw at `factor` of the world's rate (pitch follows the world).
    pub fn scroll(self, factor: f32) -> Layer {
        Layer { scroll: (factor, 1.0), ..self }
    }

    pub fn repeat(self, times: f32) -> Layer {
        Layer { repeat_x: times, ..self }
    }

    pub fn at(self, y: f32) -> Layer {
        Layer { y, ..self }
    }

    pub fn scale(self, scale: f32) -> Layer {
        Layer { scale, ..self }
    }

    pub fn fill_below(self) -> Layer {
        Layer { flags: self.flags | Layer::FILL_BELOW, ..self }
    }

    /// Multiplies by an sRGB color and opacity (0–255 each).
    pub fn tint(self, r: u8, g: u8, b: u8, a: u8) -> Layer {
        Layer { tint: u32::from_le_bytes([r, g, b, a.max(1)]), ..self }
    }
}

/// Replaces the backdrop with up to 4 layers, farthest first (mb3d 2); an
/// empty slice clears it. The classic SNES look: a sky panorama (or
/// [`atmosphere`](super::atmosphere)), far mountains at `scroll(0.4)`, nearer
/// hills at `scroll(0.7)`, with a [`MeshData::mode7_floor`](super::MeshData::mode7_floor)
/// in front. One fullscreen draw at the scene's resolution (the retro
/// look's when it's on). False if a texture isn't live (that layer is
/// dropped) or there are more than 4.
pub fn backdrop(layers: &[Layer]) -> bool {
    layers.len() <= 4 && unsafe { ffi::mb3d_backdrop(layers.as_ptr() as *const u8, layers.len() as u32) } == 0
}

/// Where to put a [`MeshData::mode7_floor`](super::MeshData::mode7_floor)'s
/// node so it stays under the camera without its texture sliding: the
/// camera's x and z snapped to whole tiles, at height `y`. Call it every
/// frame (`floor.set_pos(mode7_anchor(cam, 4.0, 0.0))`).
pub fn mode7_anchor(camera: Vec3, tile: f32, y: f32) -> Vec3 {
    let t = if tile > 0.0 { tile } else { 1.0 };
    vec3((camera.x / t).round() * t, y, (camera.z / t).round() * t)
}

/// Sets the retro look until changed (mb3d 2). Cheap: the scene draws at a
/// fraction of the pixels (cheaper than full resolution), and the colors
/// and dither are a few ALU in the composite. Bloom, depth of field and the
/// beauty pass run at the low resolution too.
pub fn retro(r: &Retro) {
    unsafe { ffi::mb3d_retro(ptr(r)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_spec() {
        assert_eq!(size_of::<Retro>(), 32);
        assert_eq!(std::mem::offset_of!(Retro, color_bits), 8);
        assert_eq!(std::mem::offset_of!(Retro, vertex_snap), 16);
        assert_eq!(std::mem::offset_of!(Retro, fog_bands), 20);
        assert_eq!(size_of::<Layer>(), 40);
        assert_eq!(std::mem::offset_of!(Layer, scroll), 8);
        assert_eq!(std::mem::offset_of!(Layer, repeat_x), 24);
        assert_eq!(std::mem::offset_of!(Layer, tint), 36);
        assert!(backdrop(&[]));
        assert_eq!(mode7_anchor(vec3(5.1, 9.0, -3.0), 4.0, 0.0), vec3(4.0, 0.0, -4.0));
        let f = super::super::MeshData::mode7_floor(100.0, 4.0, 8);
        assert_eq!((f.positions.len(), f.indices.len()), (81, 8 * 8 * 6));
        // UVs are world units over the tile; every triangle faces up.
        assert_eq!(f.uvs[0], [-25.0, -25.0]);
        for t in f.indices.chunks(3) {
            let [a, b, c] = [f.positions[t[0] as usize], f.positions[t[1] as usize], f.positions[t[2] as usize]];
            assert!((b - a).cross(c - a).y > 0.0);
        }


        assert!(Retro::saturn().resolution.0 > 16.0 && Retro::ps1().color_bits == 5.0 && Retro::arcade().vertex_snap == 0.0);
        retro(&Retro::saturn());
        retro(&Retro::default());
    }
}
