//! A terrain's ground surfaces (mb3d 4; docs/STUDIO-2.md Decided 70): which
//! photographed surface the ground is where, per pixel, as mb-host's ground
//! shader draws it ([`crate::gfx3d::surfaces`]).
//!
//! Five built-in surfaces ([`Surface`]): grass, a dry meadow, dirt, rock
//! (laid from the side on cliffs) and snow. With no `paint`, the natural
//! ground ([`natural`]) lays them as an experienced maker would paint a
//! mountain landscape. A game's `paint` rules name a surface, or a colour,
//! or both (a colour recolours the surface); a rule with only a colour takes
//! the game's own `texture` if it has one, else the surface nearest its
//! colour. Without images (`texture = false`), every surface is its colour.

use crate::gfx3d::{SurfaceLayer, SurfaceRule, SurfacesDesc, Texture};

use super::tile::PaintRule;

/// A built-in ground surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    Grass,
    Meadow,
    Dirt,
    Rock,
    Snow,
}

impl Surface {
    pub const ALL: [Surface; 5] = [Surface::Grass, Surface::Meadow, Surface::Dirt, Surface::Rock, Surface::Snow];

    /// The name `paint` and `surfaces` use, and the bundle's images
    /// (`terrain/<name>.jpg`, `terrain/<name>_n.jpg`).
    pub fn name(self) -> &'static str {
        ["grass", "meadow", "dirt", "rock", "snow"][self as usize]
    }

    pub fn from_name(s: &str) -> Option<Surface> {
        Surface::ALL.into_iter().find(|k| k.name() == s)
    }

    /// The built-in image's mean colour (sRGB), printed by tools/terrain/ground.py:
    /// what it is from afar, and its colour without images.
    pub fn mean(self) -> u32 {
        [0x567336, 0x807d3c, 0x6f5e49, 0x7c7774, 0xe0e1e4][self as usize]
    }

    /// Meters of ground one repeat of the built-in image covers.
    pub fn size(self) -> f32 {
        [3.5, 4.0, 4.0, 7.0, 6.0][self as usize]
    }

    /// How the built-in surface is laid: (normal strength, roughness, triplanar, meters its detail lasts).
    fn look(self) -> (f32, f32, bool, f32) {
        match self {
            Surface::Grass => (1.0, 0.95, false, 70.0),
            Surface::Meadow => (1.0, 0.95, false, 80.0),
            Surface::Dirt => (1.0, 0.92, false, 80.0),
            Surface::Rock => (1.2, 0.85, true, 120.0),
            Surface::Snow => (0.8, 0.7, false, 100.0),
        }
    }
}

/// A surface's images: the colour image and its normal map, and the meters one repeat covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceImage {
    pub color: Option<Texture>,
    pub normal: Option<Texture>,
    pub size: f32,
}

/// The ground's broad noise patches (meters across), edge breakup, and brightness variation.
const PATCH_SIZE: f32 = 60.0;
const EDGE_SIZE: f32 = 4.0;
const MACRO: f32 = 0.1;

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

/// The built-in surface nearest a colour (sRGB): by its hue and
/// saturation (its colour ÷ its brightness), then a little by brightness.
/// What a `paint` rule with only a colour lays.
pub fn nearest(c: u32) -> Surface {
    let key = |c: u32| {
        let [r, g, b] = rgb(c);
        let l = (0.3 * r + 0.59 * g + 0.11 * b).max(1e-3);
        ([(r - l) / l, (g - l) / l, (b - l) / l], l)
    };
    let (a, la) = key(c);
    let d = |s: Surface| {
        let (b, lb) = key(s.mean());
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2) + 0.5 * ((la - lb) / la.max(lb)).powi(2)
    };
    Surface::ALL.into_iter().min_by(|a, b| d(*a).total_cmp(&d(*b))).unwrap()
}

/// Layers as they're made: each built-in surface with images, the game's
/// own `texture`, and a flat one for surfaces without images.
struct Layers {
    out: Vec<SurfaceLayer>,
    by_surface: [Option<u32>; 5],
    texture: Option<u32>,
    flat: Option<u32>,
}

impl Layers {
    fn surface(&mut self, images: &[Option<SurfaceImage>; 5], s: Surface) -> (u32, Option<[f32; 3]>) {
        let Some(img) = images[s as usize] else { return (self.flat(), Some(rgb(s.mean()))) };
        if let Some(i) = self.by_surface[s as usize] {
            return (i, None);
        }
        let (normal_strength, roughness, triplanar, detail_far) = s.look();
        let i = self.push(SurfaceLayer { color: img.color, normal: img.normal, size: img.size, normal_strength, roughness, triplanar, detail_far });
        self.by_surface[s as usize] = Some(i);
        (i, None)
    }

    fn texture(&mut self, img: SurfaceImage) -> u32 {
        if let Some(i) = self.texture {
            return i;
        }
        let i = self.push(SurfaceLayer { color: img.color, normal: img.normal, size: img.size, normal_strength: 1.0, roughness: 0.92, triplanar: false, detail_far: img.size * 25.0 });
        self.texture = Some(i);
        i
    }

    fn flat(&mut self) -> u32 {
        if let Some(i) = self.flat {
            return i;
        }
        let i = self.push(SurfaceLayer { color: None, normal: None, size: 4.0, normal_strength: 0.0, roughness: 0.92, triplanar: false, detail_far: 0.0 });
        self.flat = Some(i);
        i
    }

    fn push(&mut self, l: SurfaceLayer) -> u32 {
        self.out.push(l);
        (self.out.len() - 1) as u32
    }
}

/// A terrain's surfaces: from its `paint` rules, or the natural ground for
/// a terrain `height` meters from lowest to highest.
pub fn surfaces(paint: &[PaintRule], images: &[Option<SurfaceImage>; 5], texture: Option<SurfaceImage>, height: f32) -> SurfacesDesc {
    let mut layers = Layers { out: Vec::new(), by_surface: [None; 5], texture: None, flat: None };
    let rules = if paint.is_empty() { natural(&mut layers, images, height.abs().max(1.0)) } else { rules(&mut layers, images, texture, paint, height) };
    SurfacesDesc { layers: layers.out, rules, patch_size: PATCH_SIZE, edge_size: EDGE_SIZE, macro_amount: MACRO, tint: true }
}

/// The game's `paint`, in order: the first is the base. Heights blend over
/// 3% of the terrain's height (or `blend` meters), slopes over 0.04 (or
/// `blend` for a rule of slope alone), as vertex paint did; the edges are
/// broken by noise about as wide as they blend.
fn rules(layers: &mut Layers, images: &[Option<SurfaceImage>; 5], texture: Option<SurfaceImage>, paint: &[PaintRule], height: f32) -> Vec<SurfaceRule> {
    let hb = (height.abs() * 0.03).max(0.25);
    paint
        .iter()
        .take(crate::gfx3d::surfaces::MAX_RULES)
        .map(|r| {
            let (layer, own) = match (r.surface, texture) {
                (Some(s), _) => layers.surface(images, s),
                (None, Some(t)) => (layers.texture(t), None),
                (None, None) => layers.surface(images, nearest(r.color.unwrap_or(0x808080))),
            };
            let only_slope = r.above.is_none() && r.below.is_none();
            let height_blend = if only_slope { hb } else { r.blend.unwrap_or(hb).max(0.01) };
            let slope_blend = if only_slope { r.blend.unwrap_or(0.04).max(0.001) } else { 0.04 };
            SurfaceRule {
                layer,
                color: r.color.map(rgb).or(own),
                above: r.above,
                below: r.below,
                slope: r.slope,
                gentle: false,
                height_blend,
                slope_blend,
                height_noise: height_blend * 0.8,
                slope_noise: slope_blend * 0.8,
                patches: None,
                hollow: None,
                strength: 1.0,
            }
        })
        .collect()
}

/// The natural ground: grass in several greens (the vertex tint,
/// `tile::Tint`), dry meadow in broken patches over about a fifth of it above the valley floors,
/// sand in the lowest hollows, dirt on steeper slopes and in narrow gullies, rock
/// on cliffs and most of the high ground, snow over a wavering line that
/// slides off the steepest faces. `h` is the terrain's height.
fn natural(layers: &mut Layers, images: &[Option<SurfaceImage>; 5], h: f32) -> Vec<SurfaceRule> {
    let mut on = |s: Surface, r: SurfaceRule| {
        let (layer, color) = layers.surface(images, s);
        SurfaceRule { layer, color: r.color.or(color), ..r }
    };
    let base = SurfaceRule::base(0);
    vec![
        on(Surface::Grass, base),
        // Dry meadow in broken patches over about a quarter of the ground
        // (the broad noise past 0.3: mb-host's `broad_noise_patches_cover_what_they_mean_to`).
        // Each rule covers its ground whole, in patches with broken edges,
        // rather than half and half (two photographs at half strength read as a smear).
        // Not on the low valley floors, which stay lush (and where a round starts).
        on(Surface::Meadow, SurfaceRule { above: Some(0.16 * h), height_blend: 0.03 * h, height_noise: 0.04 * h, patches: Some((0.3, 0.05)), strength: 0.95, ..base }),
        // Sand where rivers have been: the lowest, hollowest ground.
        on(Surface::Dirt, SurfaceRule { color: Some(rgb(0xa8987a)), below: Some(0.008 * h), height_blend: 0.003 * h, height_noise: 0.003 * h, hollow: Some(0.08), strength: 0.9, ..base }),
        // Bare earth on steeper slopes and in narrow gullies.
        on(Surface::Dirt, SurfaceRule { slope: Some(0.265), slope_blend: 0.015, slope_noise: 0.05, strength: 0.9, ..base }),
        on(Surface::Dirt, SurfaceRule { hollow: Some(0.3), strength: 0.85, ..base }),
        // Rock over much of the high ground, all of it where it isn't flat, and every cliff.
        on(Surface::Rock, SurfaceRule { above: Some(0.56 * h), height_blend: 0.02 * h, height_noise: 0.06 * h, patches: Some((0.0, 0.05)), ..base }),
        on(Surface::Rock, SurfaceRule { above: Some(0.56 * h), height_blend: 0.02 * h, height_noise: 0.06 * h, slope: Some(0.1), slope_blend: 0.025, slope_noise: 0.05, ..base }),
        on(Surface::Rock, SurfaceRule { slope: Some(0.29), slope_blend: 0.02, slope_noise: 0.07, ..base }),
        // Snow over a wavering line, off the steepest faces.
        on(Surface::Snow, SurfaceRule { above: Some(0.68 * h), height_blend: 0.012 * h, height_noise: 0.07 * h, slope: Some(0.42), gentle: true, slope_blend: 0.04, slope_noise: 0.08, ..base }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images() -> [Option<SurfaceImage>; 5] {
        [Some(SurfaceImage { color: None, normal: None, size: 4.0 }); 5]
    }

    #[test]
    fn a_colour_takes_the_nearest_surface() {
        assert_eq!(nearest(0x3c5a2e), Surface::Grass);
        assert_eq!(nearest(0xd8dee6), Surface::Snow);
        assert_eq!(nearest(0x7a7a7a), Surface::Rock);
        assert_eq!(nearest(0x6b6256), Surface::Dirt, "a brownish grey");
        assert_eq!(nearest(0x8a6a48), Surface::Dirt);
        assert_eq!(nearest(0x9a9460), Surface::Meadow);
    }

    #[test]
    fn the_natural_ground_lays_all_five() {
        let d = surfaces(&[], &images(), None, 150.0);
        assert_eq!(d.layers.len(), 5);
        assert_eq!(d.rules[0].layer, 0, "grass is the base");
        assert!(d.layers[Surface::Rock as usize].triplanar && !d.layers[0].triplanar);
        let snow = d.rules.last().unwrap();
        assert_eq!((snow.layer, snow.above), (Surface::Snow as u32, Some(0.68 * 150.0)));
        // Without images: one flat layer, every rule its surface's colour.
        let flat = surfaces(&[], &[None; 5], None, 150.0);
        assert_eq!(flat.layers.len(), 1);
        assert!(flat.rules.iter().all(|r| r.layer == 0 && r.color.is_some()));
    }

    #[test]
    fn paint_names_surfaces_or_colours() {
        let paint = [
            PaintRule::color(0x3c5a2e),
            PaintRule { above: Some(60.0), ..PaintRule::surface(Surface::Snow) },
            PaintRule { slope: Some(0.2), surface: Some(Surface::Rock), ..PaintRule::color(0x806050) },
        ];
        let d = surfaces(&paint, &images(), None, 90.0);
        assert_eq!(d.layers.len(), 3, "grass (nearest the base's colour), snow, rock");
        assert_eq!(d.rules[0].color, Some(rgb(0x3c5a2e)), "a colour recolours");
        assert_eq!(d.rules[1].color, None, "a surface alone is its photo");
        assert!(d.layers[d.rules[2].layer as usize].triplanar);
        assert_eq!(d.rules[1].height_blend, 90.0 * 0.03);
        // The game's own texture under rules that name only a colour.
        let own = SurfaceImage { color: None, normal: None, size: 2.0 };
        let d = surfaces(&paint, &images(), Some(own), 90.0);
        assert_eq!((d.rules[0].layer, d.layers[0].size), (0, 2.0));
    }
}
