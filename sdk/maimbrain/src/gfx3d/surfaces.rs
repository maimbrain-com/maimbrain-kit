//! Ground surfaces (mb3d 4, `mb3d_material_surfaces`; declare
//! `stdlib = { mb3d = 4 }`): photographed surfaces a material blends per
//! pixel by rules of height, slope, hollowness and broad patches, each laid
//! in meters from above (or from the side too, `triplanar`, for cliffs), its
//! detail fading with distance to its mean colour. mb-host shades a static
//! mesh with such a material with its ground shader, which reads each
//! vertex's UV as the ground's slope (`1 − normal.y`) and hollowness there
//! and, with [`SurfacesDesc::tint`], its colour as a tint (sRGB 188 is none).
//! [`terrain`](super::terrain) uses it for every terrain's tiles.

use super::{Material, Texture, ffi};

/// One surface: its colour image and normal map (None: flat white, flat).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceLayer {
    pub color: Option<Texture>,
    pub normal: Option<Texture>,
    /// Meters of ground one repeat of the images covers.
    pub size: f32,
    /// How strongly the normal map bends the light, 0–2.
    pub normal_strength: f32,
    pub roughness: f32,
    /// Laid from the side too on steep ground (rock on cliffs).
    pub triplanar: bool,
    /// Meters its detail lasts, fading to its mean colour from 30% of that.
    pub detail_far: f32,
}

/// Where a surface lies. The first rule is the base, everywhere; each later
/// one takes `strength` of the ground where all its conditions hold, edges
/// blended and broken by noise, later rules over earlier ones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRule {
    /// Index into [`SurfacesDesc::layers`].
    pub layer: u32,
    /// sRGB 0–1: recolours the layer, its detail grey around this colour.
    pub color: Option<[f32; 3]>,
    /// Meters: higher than `above`, lower than `below`.
    pub above: Option<f32>,
    pub below: Option<f32>,
    /// `1 − normal.y`: steeper than this (or, `gentle`, gentler).
    pub slope: Option<f32>,
    pub gentle: bool,
    pub height_blend: f32,
    pub slope_blend: f32,
    /// How far the edge noise moves its edges, in meters of height and in slope.
    pub height_noise: f32,
    pub slope_noise: f32,
    /// Where the broad noise (−1..1) is past this, blended over the second.
    pub patches: Option<(f32, f32)>,
    /// Where the ground is hollower than this (a gully: + is hollow).
    pub hollow: Option<f32>,
    pub strength: f32,
}

impl SurfaceRule {
    /// Layer `layer` everywhere (the base).
    pub fn base(layer: u32) -> SurfaceRule {
        SurfaceRule {
            layer,
            color: None,
            above: None,
            below: None,
            slope: None,
            gentle: false,
            height_blend: 1.0,
            slope_blend: 0.04,
            height_noise: 0.0,
            slope_noise: 0.0,
            patches: None,
            hollow: None,
            strength: 1.0,
        }
    }
}

/// A material's ground surfaces.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfacesDesc {
    /// At most 8.
    pub layers: Vec<SurfaceLayer>,
    /// At most 16, in order.
    pub rules: Vec<SurfaceRule>,
    /// Meters across the broad noise's patches.
    pub patch_size: f32,
    /// Meters across the noise that breaks edges.
    pub edge_size: f32,
    /// How much a large-scale noise varies the brightness, 0–0.5.
    pub macro_amount: f32,
    /// The vertex colour is a tint (sRGB 188 = none) rather than ignored.
    pub tint: bool,
}

pub const MAX_LAYERS: usize = 8;
pub const MAX_RULES: usize = 16;

impl SurfacesDesc {
    /// As `mb3d_material_surfaces` reads it (mb-host mb3d/surfaces.rs).
    pub fn pack(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(32 + self.layers.len() * 32 + self.rules.len() * 64);
        let mut w = |v: u32| b.extend(v.to_le_bytes());
        let layers = self.layers.len().min(MAX_LAYERS);
        let rules = self.rules.len().min(MAX_RULES);
        for v in [layers as u32, rules as u32, self.patch_size.to_bits(), self.edge_size.to_bits(), self.macro_amount.to_bits(), self.tint as u32, 0, 0] {
            w(v);
        }
        let tex = |t: Option<Texture>| t.map_or(0, |t| t.0.get());
        for l in &self.layers[..layers] {
            for v in [tex(l.color), tex(l.normal), l.size.to_bits(), l.normal_strength.to_bits(), l.roughness.to_bits(), l.triplanar as u32, l.detail_far.to_bits(), 0] {
                w(v);
            }
        }
        for r in &self.rules[..rules] {
            let flags = r.color.is_some() as u32
                | (r.above.is_some() as u32) << 1
                | (r.below.is_some() as u32) << 2
                | (r.slope.is_some() as u32) << 3
                | (r.patches.is_some() as u32) << 4
                | (r.hollow.is_some() as u32) << 5
                | ((r.slope.is_some() && r.gentle) as u32) << 6;
            let c = r.color.unwrap_or([1.0; 3]);
            let (patch, patch_blend) = r.patches.unwrap_or((0.0, 0.1));
            let f = |v: f32| v.to_bits();
            for v in [
                r.layer.min(layers.saturating_sub(1) as u32),
                flags,
                f(c[0]),
                f(c[1]),
                f(c[2]),
                f(r.above.unwrap_or(0.0)),
                f(r.below.unwrap_or(0.0)),
                f(r.slope.unwrap_or(0.0)),
                f(r.height_blend),
                f(r.slope_blend),
                f(r.height_noise),
                f(r.slope_noise),
                f(patch),
                f(patch_blend),
                f(r.strength),
                f(r.hollow.unwrap_or(0.0)),
            ] {
                w(v);
            }
        }
        b
    }
}

impl Material {
    /// Gives the material ground surfaces (mb3d 4): static meshes drawn
    /// with it are shaded per pixel by mb-host's ground shader. An empty
    /// `layers` takes them away.
    pub fn set_surfaces(self, d: &SurfacesDesc) -> bool {
        let b = d.pack();
        unsafe { ffi::mb3d_material_surfaces(self.0.get(), b.as_ptr(), b.len() as u32) == 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_pack_as_the_host_reads_them() {
        let layer = SurfaceLayer { color: None, normal: None, size: 4.0, normal_strength: 1.0, roughness: 0.9, triplanar: true, detail_far: 120.0 };
        let d = SurfacesDesc {
            layers: vec![layer, layer],
            rules: vec![SurfaceRule::base(0), SurfaceRule { above: Some(60.0), slope: Some(0.3), gentle: true, ..SurfaceRule::base(1) }],
            patch_size: 180.0,
            edge_size: 3.0,
            macro_amount: 0.1,
            tint: true,
        };
        let b = d.pack();
        assert_eq!(b.len(), 32 + 2 * 32 + 2 * 64);
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!((u(0), u(4), u(20)), (2, 2, 1));
        assert_eq!((u(32 + 20), f32::from_bits(u(32 + 8))), (1, 4.0), "triplanar, 4 m");
        let r = 32 + 64 + 64;
        assert_eq!((u(r), u(r + 4)), (1, 2 | 8 | 64), "above, slope, gentle");
        assert_eq!(f32::from_bits(u(r + 20)), 60.0);
    }
}
