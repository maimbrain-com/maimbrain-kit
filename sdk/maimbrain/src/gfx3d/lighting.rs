//! Lighting presets: the sun, the sky v2 atmosphere and the exposure that
//! go together, set in one call ([`lighting`]).
//!
//! mb3d lights a smooth (non-toon) PBR material with the sun (key light,
//! shadows), point lights, and image-based light baked from the sky: the
//! atmosphere's colors (or a painted [`super::sky_texture`]) become a soft
//! fill from every direction (brighter from the side the sky is brighter),
//! and metals and glossy paint reflect the same sky. A material's `rim`
//! ([`MaterialStyle`]) adds a bright silhouette edge on smooth shading too.
//! So a textured model reads when the sky behind and around it is bright,
//! the sun comes from the camera's side, and its materials are smooth (toon
//! bands flatten a texture to two or three tones): [`Lighting::hero`] and
//! [`MaterialStyle::hero`].

use super::{Atmosphere, MaterialStyle, Post, Sun, Vec3, vec3};

/// A lighting setup: the sun, the atmosphere (the sky's colors: the
/// ambient fill and what metals reflect) and post-processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    pub sun: Sun,
    pub atmosphere: Atmosphere,
    pub post: Post,
}

impl Lighting {
    /// A textured hero against a bright sky (a crisp, modern anime look): a
    /// clear day atmosphere, so the fill is bright and blue from above and
    /// metals reflect sky rather than black; a warm sun from high over the
    /// camera's left shoulder (for a camera looking down −z; [`Lighting::key_from`]
    /// for another), casting shadows; exposure 1, a little bloom only on what's
    /// really bright, no vignette to darken the edges.
    ///
    /// ```
    /// use maimbrain::gfx3d::{Lighting, MaterialStyle};
    /// let l = Lighting::hero();
    /// assert!(l.sun.direction.z < 0.0 && l.sun.direction.y < 0.0, "from behind the camera, above");
    /// assert_eq!(MaterialStyle::hero().toon_bands, 0, "smooth shading keeps textures");
    /// ```
    pub fn hero() -> Lighting {
        Lighting {
            sun: Sun { direction: vec3(0.38, -0.72, -0.58), color: [1.0, 0.95, 0.86], intensity: 3.4, shadows: 1, shadow_distance: 25.0 },
            atmosphere: Atmosphere::day(),
            post: Post { exposure: 1.0, bloom_strength: 0.2, bloom_threshold: 1.6, vignette: 0.08, chromatic_aberration: 0.0, saturation: 1.08, contrast: 1.06, sharpen: 0.0 },
        }
    }

    /// The same key light turned to come over the shoulder of a camera
    /// looking along `forward` (e.g. `cam.rot.forward()`), so the subject's
    /// side facing the camera is the lit one.
    pub fn key_from(mut self, forward: Vec3) -> Lighting {
        let f = vec3(forward.x, 0.0, forward.z);
        let f = if f.length() > 1e-4 { f.normalize() } else { vec3(0.0, 0.0, -1.0) };
        let right = f.cross(Vec3::Y).normalize();
        let d = self.sun.direction;
        // `hero`'s direction is in the frame of a camera looking down −z: x right, −z ahead.
        self.sun.direction = (right * d.x + Vec3::Y * d.y + f * -d.z).normalize();
        self
    }

    /// Sets the sun, the atmosphere and post-processing.
    pub fn apply(&self) {
        super::sun(&self.sun);
        super::atmosphere(&self.atmosphere);
        super::post(&self.post);
    }
}

/// Sets a whole [`Lighting`]: `gfx3d::lighting(&Lighting::hero())`.
pub fn lighting(l: &Lighting) {
    l.apply();
}

impl MaterialStyle {
    /// For a textured hero: smooth PBR shading (no toon bands, which cut a
    /// texture down to a few flat tones), a rim of light on the silhouette
    /// so it separates from the background, and a thin dark outline.
    /// `for m in model.materials() { m.set_style(&MaterialStyle::hero()); }`
    pub fn hero() -> MaterialStyle {
        MaterialStyle { outline_color: [0.02, 0.02, 0.035], outline_width: 1.0, toon_bands: 0, rim: 0.45, flags: 0, _reserved: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_light_turns_with_the_camera() {
        let l = Lighting::hero();
        let same = l.key_from(vec3(0.0, 0.0, -1.0));
        assert!((same.sun.direction - l.sun.direction.normalize()).length() < 1e-5);
        // Looking down +x, the light still comes from behind the camera and from its left.
        let d = l.key_from(vec3(1.0, 0.0, 0.0)).sun.direction;
        assert!(d.x > 0.3 && d.y < -0.5, "{d:?}");
        assert!(d.z > 0.2, "from the camera's left (−z when looking down +x), travelling right: {d:?}");
    }
}
