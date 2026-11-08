//! Games' shaders (mb3d 5's materials, mb2d 4's sprite shaders;
//! docs/STUDIO-2.md §4.15): WGSL the host checks (crate `mb-shader`, the
//! same as `mb`'s) and plugs into its own pipelines.
//!
//! Only with the `shaders` feature (engine 2.0's shader builds), or
//! natively: a game built without it imports neither mb3d 5 nor mb2d 4, so
//! it never needs the app release that brings them.

#[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
use crate::ffi;

/// `mb3d_material_shader`'s and `mb2d_shader_use`'s parameters: four
/// texture handles, then 16 `vec4` uniforms (272 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// mb3d texture handles for a material's shader (`mb_tex0`–`mb_tex3`);
    /// for a sprite shader, mb2d image handles, the first unused (`mb_tex0`
    /// is the drawn image). 0 = none (white).
    pub textures: [u32; 4],
    pub uniforms: [f32; 64],
}

impl Default for Params {
    fn default() -> Self {
        Params { textures: [0; 4], uniforms: [0.0; 64] }
    }
}

impl Params {
    pub const SIZE: usize = 16 + 256;

    pub fn pack(&self) -> [u8; Params::SIZE] {
        let mut b = [0u8; Params::SIZE];
        for (k, t) in self.textures.iter().enumerate() {
            b[k * 4..k * 4 + 4].copy_from_slice(&t.to_le_bytes());
        }
        for (k, u) in self.uniforms.iter().enumerate() {
            b[16 + k * 4..20 + k * 4].copy_from_slice(&u.to_le_bytes());
        }
        b
    }
}

/// A 3D material's shader (`fn surface`, optionally `fn vertex`): a handle
/// whatever the checks say (a refused one draws its material plain), or
/// the host's error (-2 an empty or oversized source, -4 past 64 shaders).
#[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
pub fn material_shader(src: &str) -> Result<u32, i32> {
    let h = unsafe { ffi::mb3d_shader(src.as_ptr(), src.len() as u32) };
    if h > 0 { Ok(h as u32) } else { Err(h) }
}

/// Draws `material` with `shader` (0: no shader again), its textures and uniforms.
#[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
pub fn set_material_shader(material: crate::gfx3d::Material, shader: u32, p: &Params) -> bool {
    let b = p.pack();
    unsafe { ffi::mb3d_material_shader(material.raw(), shader, b.as_ptr()) == 0 }
}

/// A 2D sprite shader (`fn sprite`), as [`material_shader`].
#[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
pub fn sprite_shader(src: &str) -> Result<u32, i32> {
    let h = unsafe { ffi::mb2d_shader(src.as_ptr(), src.len() as u32) };
    if h > 0 { Ok(h as u32) } else { Err(h) }
}

/// Images drawn from now on go through sprite shader `shader` (0: none).
#[cfg(any(feature = "shaders", not(target_arch = "wasm32")))]
pub fn use_sprite_shader(shader: u32, p: &Params) {
    let b = p.pack();
    unsafe { ffi::mb2d_shader_use(shader, b.as_ptr()) };
}
