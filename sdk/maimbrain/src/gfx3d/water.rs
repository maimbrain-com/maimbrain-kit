//! Splashes for water that reacts (mb3d 2): particle looks for water
//! ([`EmitterDesc::water_crown`], [`water_droplets`](EmitterDesc::water_droplets),
//! [`water_mist`](EmitterDesc::water_mist), [`water_spray`](EmitterDesc::water_spray))
//! and [`Splash`], which fires them together with [`ripple`] rings.
//!
//! Particles aren't lit, so each look takes `light`: the color and
//! brightness of what lights the water there (the moon, the lanterns, the
//! sun), linear. Water is white only as far as it catches light; at night
//! `[0.5, 0.6, 0.8]` is a moonlit splash, by a lantern `[1.0, 0.6, 0.3]`.
//! The crown, mist and spray blend over what's behind them, so fog fades
//! them to its color with distance; the droplets add light (glints) and fade
//! out in fog. None of it is reflected (particles aren't), and nothing
//! collides: drops falling back simply go under the water's surface.
//!
//! ```ignore
//! // Once, in init: a splash look for a koi-sized thing in moonlight.
//! let splash = Splash::new([0.55, 0.65, 0.9], 0.3).unwrap();
//! // When the koi leaves the water, and when it lands back in:
//! splash.splash(koi_pos, 0.3, 2.5);
//! let rings = splash.splash(koi_pos, 0.3, 4.5);   // keep them to bob things on
//! // Every few frames at its bow while it swims fast:
//! splash.spray(bow_pos, koi_vel, 2);
//! // A petal floating nearby rides the rings as they pass:
//! let lift: f32 = rings.iter().map(|r| r.height(petal_pos, now - landed_at)).sum();
//! ```
//!
//! [`Water`] (water v2, [`water`]) is how the reflector's water looks
//! beyond its mirror: small ripples carried by a current, the water's body
//! (its color, what's seen through it, light scattered inside it), foam
//! where it meets things, glints and caustics.

use std::f32::consts::TAU;

use super::{Emitter, EmitterDesc, FxBlend, Vec3, ffi, ptr, ripple, vec3};

/// How the reflector's water looks beyond its mirror (water v2, mb3d 2):
/// set it with [`water`]. It shows on [`Material::REFLECTIVE`](super::Material::REFLECTIVE)
/// materials while the reflector is on, and changes nothing else about
/// them: the reflection, ring ripples and wakes stay as they were.
/// `Water::default()` (all zero) is off. Start from a preset:
///
/// ```ignore
/// // A gentle swell: the look's own ripples carry the detail.
/// gfx3d::reflector(&Reflector { max_distance: 70.0, ripple: 0.15, ..Reflector::water(0.0) });
/// // A canal flowing toward the camera (+z), murky green, foaming at the walls.
/// gfx3d::water(&Water::canal().flowing(Vec3::Z, 0.35));
/// // A clear koi pond: see the fish, caustics on the bed.
/// gfx3d::water(&Water::pond());
/// ```
///
/// What each part costs and needs:
/// - **Ripples** (`detail`, `flow`): three layers of a tiling ripple texture
///   drifting downstream at their own speeds, fading into roughness with
///   distance (no shimmer): a few texture reads per water pixel.
/// - **Seeing in and contact foam** (`depth_fade`, `foam`): the host draws
///   what's below the plane a second time, from the camera, at the
///   reflection's resolution (one more draw for each mesh reaching below the
///   water, `mb.stats()` `under`), and the water reads how much water lies
///   in front of it. Things only foam where they go *under* the surface:
///   walls, posts, rocks and boats should reach below it. A fish swimming
///   just under the surface foams over its back; keep it a little deeper.
/// - **Glints** (`sparkle`) and the **body** (`color`, `tint`, `scatter`):
///   per water pixel, per light (the sun and up to 8 point lights).
/// - **Caustics**: on everything within ~0.6 m above the water (its sides
///   facing the water) and under it, from the sun and point lights.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Water {
    /// The deep water's own color (linear): how much of the light around it
    /// it scatters back. Dark: 0.01–0.06.
    pub color: [f32; 3],
    /// How deep you see into it, meters: what's under the surface fades
    /// into `color` over this depth (clear pond 1–3, murky canal 0.3–1).
    /// 0 = you don't.
    pub depth_fade: f32,
    /// Light scattered inside the water toward the camera (linear): the
    /// glow of a wave lit from behind, of water under a lantern.
    pub tint: [f32; 3],
    /// 0–4: how much it scatters (`tint`).
    pub scatter: f32,
    /// Foam's color (linear), lit like the water.
    pub foam_color: [f32; 3],
    /// 0–1: foam where the water meets things (and how it breaks up wake and crest foam).
    pub foam: f32,
    /// Which way the current runs (x, z); any length, zero = still water.
    /// It and `flow_speed` can change every frame: the ripples carry on from where they are.
    pub flow: [f32; 2],
    /// The current's speed, m/s (a canal 0.2–0.5, a stream 1–2).
    pub flow_speed: f32,
    /// 0–1: small ripples breaking up the surface (0.5 is a light breeze).
    pub detail: f32,
    /// Meters across the coarsest ripples' tile (0 = 1.5); the finer layers are 2.4× and 5.3× smaller.
    pub detail_scale: f32,
    /// 0–1: sharp glints of the sun, moon and point lights on the ripples.
    pub sparkle: f32,
    /// 0–1: light patterns the ripples throw on things under the water and on the sides of things just above it.
    pub caustics: f32,
    /// 0–1: how far the ripples bend what's seen under the water.
    pub refraction: f32,
    /// Meters of water in front of something that foam (0 = 0.12).
    pub foam_width: f32,
    pub _reserved: [u32; 3],
}

impl Default for Water {
    fn default() -> Water {
        Water {
            color: [0.0; 3],
            depth_fade: 0.0,
            tint: [0.0; 3],
            scatter: 0.0,
            foam_color: [0.0; 3],
            foam: 0.0,
            flow: [0.0; 2],
            flow_speed: 0.0,
            detail: 0.0,
            detail_scale: 0.0,
            sparkle: 0.0,
            caustics: 0.0,
            refraction: 0.0,
            foam_width: 0.0,
            _reserved: [0; 3],
        }
    }
}

impl Water {
    /// A town canal: murky green-blue, a slow current (along +z; turn it
    /// with [`flowing`](Water::flowing)), foam where it laps the walls and
    /// posts, lantern light glowing in it and glinting on it, caustics
    /// dancing on the walls.
    pub fn canal() -> Water {
        Water {
            color: [0.012, 0.035, 0.04],
            depth_fade: 0.8,
            tint: [0.08, 0.26, 0.22],
            scatter: 0.8,
            foam_color: [0.75, 0.8, 0.82],
            foam: 0.6,
            flow: [0.0, 1.0],
            flow_speed: 0.35,
            detail: 0.25,
            detail_scale: 1.6,
            sparkle: 0.35,
            caustics: 0.6,
            refraction: 0.4,
            foam_width: 0.12,
            ..Water::default()
        }
    }

    /// A lake: still, deep blue, wind ripples, a long glitter path under a low sun or moon.
    pub fn lake() -> Water {
        Water {
            color: [0.01, 0.03, 0.06],
            depth_fade: 2.5,
            tint: [0.05, 0.15, 0.25],
            scatter: 0.5,
            foam_color: [0.85, 0.88, 0.9],
            foam: 0.25,
            detail: 0.6,
            detail_scale: 3.0,
            sparkle: 1.0,
            caustics: 0.5,
            refraction: 0.5,
            ..Water::default()
        }
    }

    /// A clear garden pond: still and shallow, you see the fish and the
    /// bed, with caustics on them, darkening toward the deep middle; a thin
    /// ring of foam at the stones.
    pub fn pond() -> Water {
        Water {
            color: [0.015, 0.045, 0.035],
            depth_fade: 2.0,
            tint: [0.1, 0.3, 0.16],
            scatter: 0.4,
            foam_color: [0.85, 0.88, 0.85],
            foam: 0.3,
            detail: 0.3,
            detail_scale: 0.9,
            sparkle: 0.6,
            caustics: 0.6,
            refraction: 0.6,
            foam_width: 0.06,
            ..Water::default()
        }
    }

    /// The same water flowing along `dir` (x, z; y is ignored) at `speed` m/s.
    ///
    /// Fine to change every frame (a current that follows the player down
    /// whichever canal they're in): the ripples carry on from where they
    /// are, turning and speeding up with the current, without jumping.
    pub fn flowing(self, dir: Vec3, speed: f32) -> Water {
        Water { flow: [dir.x, dir.z], flow_speed: speed, ..self }
    }
}

/// Sets how the reflector's water looks until changed (water v2, mb3d 2);
/// `&Water::default()` returns it to the plain mirror.
pub fn water(w: &Water) {
    unsafe { ffi::mb3d_water(ptr(w)) }
}

/// A ring of ripples ([`ripple`]) as a value: start it, and ask how high it
/// lifts the water anywhere afterwards, so floating things (petals, leaves,
/// boats, lanterns) can bob on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    /// Where it starts (x, z; y is ignored: the rings lie on the reflector's plane).
    pub center: Vec3,
    pub strength: f32,
    pub speed: f32,
    pub seconds: f32,
}

impl Ring {
    /// Makes the ring on the water (`mb3d_ripple`).
    pub fn start(&self) {
        ripple(self.center, self.strength, self.speed, self.seconds);
    }
    /// How high (meters) the ring lifts the water at `p` (x, z), `age`
    /// seconds after it started: the same wave the host draws (SPEC §5.4).
    pub fn height(&self, p: Vec3, age: f32) -> f32 {
        let (dx, dz) = (p.x - self.center.x, p.z - self.center.z);
        ripple_height((dx * dx + dz * dz).sqrt(), self.strength, self.speed, self.seconds, age)
    }
}

/// How high (meters) a ring made by [`ripple`]`(_, strength, speed,
/// seconds)` lifts the water `age` seconds later, `dist` meters from where it
/// started (SPEC §5.4's ring): crests `speed` × 0.35 s apart in a packet
/// whose front is `speed × age` out, their slope starting at `strength` and
/// fading as the ring spreads and ages. 0 outside the packet and after `seconds`.
pub fn ripple_height(dist: f32, strength: f32, speed: f32, seconds: f32, age: f32) -> f32 {
    if !(speed > 0.0 && seconds > 0.0 && strength > 0.0 && age >= 0.0) {
        return 0.0;
    }
    let (strength, speed, seconds) = (strength.min(1.0), speed.min(50.0), seconds.min(30.0));
    if age >= seconds {
        return 0.0;
    }
    let lambda = (speed * 0.35).max(0.06);
    let radius = speed * age;
    let u = (radius - dist) / lambda;
    if !(-0.6..=3.2).contains(&u) {
        return 0.0;
    }
    let left = 1.0 - age / seconds;
    let slope = 0.45 * strength * left * left * (age / 0.06).min(1.0) / (1.0 + radius / (3.0 * lambda)).sqrt();
    let env = (-(u - 1.1) * (u - 1.1) / 0.72).exp();
    slope * lambda / TAU * env * (TAU * (u + radius / lambda)).sin()
}

fn tint(light: [f32; 3], k: f32, a: f32) -> [f32; 4] {
    [light[0] * k, light[1] * k, light[2] * k, a]
}

impl EmitterDesc {
    /// The crown of water thrown up where something `scale` meters across
    /// breaks the surface: streaks of water, stretched by their speed,
    /// falling back under gravity. Fire it straight up (`dir` +y).
    pub fn water_crown(light: [f32; 3], scale: f32) -> EmitterDesc {
        let s = scale.max(0.02).sqrt() / 0.55;
        EmitterDesc {
            colors: [tint(light, 1.0, 0.9), tint(light, 0.9, 0.75), tint(light, 0.8, 0.45), tint(light, 0.7, 0.0)],
            sizes: [0.05 * s, 0.045 * s, 0.035 * s, 0.02 * s],
            lifetime: [0.35, 0.65],
            speed: [1.6 * s, 3.4 * s],
            spread: 0.42,
            drag: 0.4,
            blend: FxBlend::Alpha,
            texture: None,
            gravity: vec3(0.0, -9.8, 0.0),
            stretch: 0.035,
        }
    }

    /// Droplets flung higher and wider than the crown, glinting (additive):
    /// they catch the light, so a few read even at night.
    pub fn water_droplets(light: [f32; 3], scale: f32) -> EmitterDesc {
        let s = scale.max(0.02).sqrt() / 0.55;
        EmitterDesc {
            colors: [tint(light, 2.2, 1.0), tint(light, 1.6, 1.0), tint(light, 1.0, 0.8), tint(light, 0.4, 0.0)],
            sizes: [0.028 * s, 0.025 * s, 0.022 * s, 0.015 * s],
            lifetime: [0.55, 1.0],
            speed: [2.4 * s, 5.0 * s],
            spread: 0.75,
            drag: 0.2,
            blend: FxBlend::Additive,
            texture: None,
            gravity: vec3(0.0, -9.8, 0.0),
            stretch: 0.02,
        }
    }

    /// A soft puff of mist hanging over the splash and drifting up a little.
    pub fn water_mist(light: [f32; 3], scale: f32) -> EmitterDesc {
        let s = scale.max(0.02) / 0.3;
        EmitterDesc {
            colors: [tint(light, 0.5, 0.0), tint(light, 0.45, 0.28), tint(light, 0.4, 0.14), tint(light, 0.35, 0.0)],
            sizes: [0.25 * s, 0.6 * s, 0.85 * s, 1.0 * s],
            lifetime: [0.8, 1.4],
            speed: [0.3, 1.0],
            spread: 1.2,
            drag: 2.0,
            blend: FxBlend::Alpha,
            texture: None,
            gravity: vec3(0.0, 0.15, 0.0),
            stretch: 0.0,
        }
    }

    /// Low spray and foam flecks thrown off a bow or a churning tail: short,
    /// close to the water, settling fast. Fire it along the surface (no
    /// gravity: it slows to a stop and fades).
    pub fn water_spray(light: [f32; 3], scale: f32) -> EmitterDesc {
        let s = scale.max(0.02).sqrt() / 0.55;
        EmitterDesc {
            colors: [tint(light, 0.9, 0.7), tint(light, 0.8, 0.5), tint(light, 0.7, 0.25), tint(light, 0.6, 0.0)],
            sizes: [0.04 * s, 0.06 * s, 0.07 * s, 0.07 * s],
            lifetime: [0.3, 0.6],
            speed: [0.4 * s, 1.2 * s],
            spread: 0.6,
            drag: 4.0,
            blend: FxBlend::Alpha,
            texture: None,
            gravity: vec3(0.0, -1.5, 0.0),
            stretch: 0.0,
        }
    }
}

/// A splash look: crown, droplets, mist and spray emitters for one size of
/// thing and one light, built once and fired anywhere (four of the 256
/// emitters). [`splash`](Splash::splash) also starts [`ripple`] rings, so
/// the water shows where it was broken long after the drops are gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splash {
    pub crown: Emitter,
    pub droplets: Emitter,
    pub mist: Emitter,
    pub spray: Emitter,
}

impl Splash {
    /// `light`: what lights the water (linear rgb); `scale`: the size of the
    /// things that will splash, meters (a koi 0.3, a person 0.6, a crate 1).
    pub fn new(light: [f32; 3], scale: f32) -> Option<Splash> {
        Some(Splash {
            crown: Emitter::new(&EmitterDesc::water_crown(light, scale))?,
            droplets: Emitter::new(&EmitterDesc::water_droplets(light, scale))?,
            mist: Emitter::new(&EmitterDesc::water_mist(light, scale))?,
            spray: Emitter::new(&EmitterDesc::water_spray(light, scale))?,
        })
    }

    /// Something `size` meters across breaks the surface at `at` (on the
    /// water) at `speed` m/s, going in or coming out: a crown, droplets and
    /// mist, more for bigger and faster, and two rings (a fast strong one,
    /// a slower one inside it), which it returns (for things to bob on).
    pub fn splash(&self, at: Vec3, size: f32, speed: f32) -> [Ring; 2] {
        let k = (size.max(0.02) / 0.3) * (speed.max(0.0) / 4.0).clamp(0.2, 2.0);
        let n = |base: f32| (base * k).round().clamp(1.0, 400.0) as u32;
        self.crown.emit(at, Vec3::Y, n(46.0));
        self.droplets.emit(at, Vec3::Y, n(26.0));
        self.mist.emit(at + vec3(0.0, 0.1 * size, 0.0), Vec3::Y, n(5.0));
        let strength = (0.45 + 0.15 * speed).min(1.0);
        let fast = (0.9 + 0.12 * speed + size).min(3.0);
        let rings = [Ring { center: at, strength, speed: fast, seconds: 2.6 }, Ring { center: at, strength: strength * 0.6, speed: fast * 0.55, seconds: 3.0 }];
        for r in &rings {
            r.start();
        }
        rings
    }

    /// Spray off a bow (or a tail) at `at`, moving at `velocity`: `count`
    /// flecks thrown up and out to the sides, riding along with it. Call it
    /// every few frames while something cuts through the water fast.
    pub fn spray(&self, at: Vec3, velocity: Vec3, count: u32) {
        let fwd = vec3(velocity.x, 0.0, velocity.z);
        let side = if fwd.length_squared() > 1e-6 { fwd.normalize().cross(Vec3::Y) } else { Vec3::X };
        let up = Vec3::Y * 0.8;
        self.spray.emit_moving(at, (up + side).normalize(), count.div_ceil(2), velocity * 0.4);
        self.spray.emit_moving(at, (up - side).normalize(), count / 2, velocity * 0.4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Water` is SPEC §5.4's 96 bytes, fields where the host reads them;
    /// the presets are on, and `flowing` only changes the current.
    #[test]
    fn water_look_layout_and_presets() {
        assert_eq!(size_of::<Water>(), 96);
        let w = Water { depth_fade: 1.0, foam: 2.0, flow: [3.0, 4.0], detail_scale: 5.0, foam_width: 6.0, ..Water::default() };
        let b = unsafe { std::slice::from_raw_parts(ptr(&w), 96) };
        let at = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!([at(12), at(44), at(48), at(52), at(64), at(80)], [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        for p in [Water::canal(), Water::lake(), Water::pond()] {
            assert!(p.detail > 0.0 && p.depth_fade > 0.0 && p.color.iter().all(|c| *c < 0.1), "{p:?}");
        }
        let c = Water::canal().flowing(vec3(-1.0, 5.0, 0.0), 0.8);
        assert_eq!((c.flow, c.flow_speed), ([-1.0, 0.0], 0.8));
        assert_eq!(Water { flow: [0.0, 1.0], flow_speed: 0.35, ..c }, Water::canal());
        water(&Water::pond());
    }

    /// The presets stay sane: they fade to nothing, splash drops fall, and
    /// bigger things make bigger drops.
    #[test]
    fn water_presets() {
        let light = [0.55, 0.65, 0.9];
        for d in [EmitterDesc::water_crown(light, 0.3), EmitterDesc::water_droplets(light, 0.3), EmitterDesc::water_mist(light, 0.3), EmitterDesc::water_spray(light, 0.3)] {
            assert_eq!(d.colors[3][3], 0.0, "{d:?}");
            assert!(d.lifetime[0] <= d.lifetime[1] && d.speed[0] <= d.speed[1]);
        }
        assert!(EmitterDesc::water_crown(light, 0.3).gravity.y < -9.0);
        assert!(EmitterDesc::water_crown(light, 1.0).sizes[0] > EmitterDesc::water_crown(light, 0.3).sizes[0]);
        assert_eq!(EmitterDesc::water_droplets(light, 0.3).blend, FxBlend::Additive);
    }

    /// The usage example in the module docs (and the skill) runs off-wasm.
    #[test]
    fn splash_example() {
        let splash = Splash::new([0.55, 0.65, 0.9], 0.3).unwrap();
        let koi_pos = vec3(0.5, 0.0, -6.0);
        splash.splash(koi_pos, 0.3, 2.5);
        let rings = splash.splash(koi_pos, 0.3, 4.5);
        splash.spray(koi_pos + vec3(0.0, 0.0, -0.2), vec3(0.0, 0.0, -4.5), 2);
        assert!(super::super::wake(0, koi_pos, vec3(0.0, 0.0, -4.5), 0.3, 0.8));
        let petal = koi_pos + vec3(1.0, 0.0, 0.0);
        let lift: f32 = rings.iter().map(|r| r.height(petal, 0.8)).sum();
        assert!(lift.abs() < 0.2);
    }

    /// A ring's packet passes a point once: still before, bobbing while its
    /// crests go by (as high as their slope × wavelength / 2π), still after.
    #[test]
    fn a_ring_lifts_the_water_as_it_passes() {
        let r = Ring { center: Vec3::ZERO, strength: 0.9, speed: 1.5, seconds: 3.0 };
        let p = vec3(1.5, 0.0, 0.0);
        let at = |age: f32| r.height(p, age);
        assert_eq!(at(0.3), 0.0, "not there yet");
        let peak = (0..300).map(|k| at(k as f32 / 100.0).abs()).fold(0.0, f32::max);
        let lambda = 1.5 * 0.35;
        assert!(peak > 0.004 && peak < 0.45 * 0.9 * lambda / TAU, "{peak}");
        assert_eq!(at(2.9), 0.0, "gone by");
        assert_eq!(ripple_height(1.0, 0.9, 1.5, 3.0, 3.5), 0.0, "past its seconds");
    }
}
