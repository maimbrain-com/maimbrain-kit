//! `Terrain{…}` (docs/STUDIO-2.md §5 [terrain], §9.3 row 2c): an outdoor
//! world on a heightmap, the SDK's `gfx3d::terrain` behind a 3D node of its
//! own kind. The node is the round's (`name`, `tag`, `visible`, `data`,
//! `destroy`, the `built` signal, `progress`); its tiles are the session's,
//! so the round after the title card takes over the ground the attract
//! mode built (three.rs `take_terrain`).
//!
//! ```lua
//! local ground = Terrain{ noise = { seed = 7, ridged = 0.3 }, height = 90,
//!   stamps = { { "hill", 20, -10, radius = 18, height = 6 } },
//!   paint = { { surface = "grass" }, { above = 60, surface = "snow" }, { slope = 0.2, surface = "rock" } } }
//! hero.y = ground:heightAt(hero.x, hero.z)
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, c_int, c_void};

use maimbrain::gfx3d::terrain::{self as ter, NoiseDesc, PaintRule, Stamp, Surface, SurfaceImage, TerrainDesc};
use maimbrain::gfx3d::{self, Fog, FogMode, Texture, Vec3};
use maimbrain::scene::three::Node3Id;

use super::atoms::A;
use super::three::{abs, push_node, with_field};
use super::*;
use crate::three::{Ground, Info, Kind3, World3};

/// The built-in ground surfaces `mb build` packs with a game that makes a
/// terrain (crates/mb-cli's `studio::v2`, made by tools/terrain/ground.py):
/// `terrain/<surface>.jpg` and its normal map `terrain/<surface>_n.jpg`,
/// for grass, meadow, dirt, rock and snow (Decided 70).
pub fn surface_image(s: Surface) -> String {
    format!("terrain/{}.jpg", s.name())
}
pub fn surface_normal(s: Surface) -> String {
    format!("terrain/{}_n.jpg", s.name())
}

thread_local! {
    /// Ground images as textures, made once a session (a terrain kept for
    /// the next round keeps using them, so they outlive rounds).
    static TEXTURES: RefCell<BTreeMap<String, Texture>> = const { RefCell::new(BTreeMap::new()) };
}

/// A new session: the last one's textures are gone.
pub fn new_session() {
    TEXTURES.with(|m| m.borrow_mut().clear());
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        sv_push_rfn(L, mk_terrain, ud, c"Terrain".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"Terrain".as_ptr());
    }
}

/// A write or call a Terrain doesn't take: its ground is the world's.
pub fn fixed(key: &str) -> String {
    format!("a Terrain stays where it is (its ground is the world's): it has no `{key}`. It has heightAt, normalAt, raise, progress, built, visible, name, tag, data and destroy")
}

// `Terrain{ noise, height, stamps, paint, surfaces, reach, detail, fog, size, collision, texture, textureSize }`
b!(mk_terrain, |L, h| match make(L, h) {
    Ok(n) => n,
    Err(m) => err(L, &format!("Terrain{{…}}: {m}")),
});

const KEYS: &[&str] = &["noise", "height", "stamps", "paint", "surfaces", "reach", "detail", "fog", "size", "collision", "texture", "textureSize", "heightmap", "resolution", "name", "tag", "visible"];

const SURFACES: &str = "\"grass\", \"meadow\", \"dirt\", \"rock\" or \"snow\"";

/// A number field, or `d` when it's absent; anything else is an error.
unsafe fn number(L: *mut lua_State, t: c_int, k: &CStr, d: f32) -> Result<f32, String> {
    unsafe {
        with_field(L, t, k, || match lua_type(L, -1) {
            LUA_TNIL => Ok(d),
            LUA_TNUMBER if lua_tonumber(L, -1).is_finite() => Ok(lua_tonumber(L, -1) as f32),
            _ => Err(format!("{} is a number, got {}", k.to_str().unwrap_or("?"), to_s(L, -1))),
        })
    }
}

unsafe fn maybe(L: *mut lua_State, t: c_int, k: &CStr) -> Result<Option<f32>, String> {
    unsafe {
        let present = with_field(L, t, k, || lua_type(L, -1) != LUA_TNIL);
        if present { number(L, t, k, 0.0).map(Some) } else { Ok(None) }
    }
}

/// A table's string keys that aren't in `known`.
unsafe fn unknown_keys(L: *mut lua_State, t: c_int, known: &[&str]) -> Option<String> {
    unsafe {
        let mut out = None;
        lua_pushnil(L);
        while lua_next(L, t) != 0 {
            if let Some(k) = st(L, -2)
                && !known.contains(&k)
                && out.is_none()
            {
                out = Some(k.to_string());
            }
            lua_pop(L, 1);
        }
        out
    }
}

unsafe fn noise(L: *mut lua_State, t: c_int) -> Result<Option<NoiseDesc>, String> {
    unsafe {
        lua_rawgetfield(L, t, c"noise".as_ptr());
        let n = abs(L, -1);
        let r = match lua_type(L, n) {
            LUA_TNIL => Ok(Some(NoiseDesc::default())),
            LUA_TBOOLEAN if lua_toboolean(L, n) == 0 => Ok(None),
            LUA_TTABLE => (|| {
                if let Some(k) = unknown_keys(L, n, &["seed", "octaves", "frequency", "ridged", "warp"]) {
                    return Err(format!("noise has seed, octaves, frequency, ridged and warp, not {k}"));
                }
                let d = NoiseDesc::default();
                let octaves = number(L, n, c"octaves", d.octaves as f32)?;
                if !(1.0..=8.0).contains(&octaves) {
                    return Err(format!("noise.octaves is 1 to 8, got {}", fmt_num(octaves as f64)));
                }
                let frequency = number(L, n, c"frequency", d.frequency)?;
                if !(frequency > 0.0 && frequency <= 1.0) {
                    return Err(format!("noise.frequency is about 1 ÷ a hill's width in meters (0.004: hills ~250 m apart), got {}", fmt_num(frequency as f64)));
                }
                let ridged = number(L, n, c"ridged", 0.0)?;
                if !(0.0..=1.0).contains(&ridged) {
                    return Err(format!("noise.ridged is 0 to 1, got {}", fmt_num(ridged as f64)));
                }
                Ok(Some(NoiseDesc { seed: number(L, n, c"seed", d.seed as f32)? as i32, octaves: octaves as u32, frequency, ridged, warp: maybe(L, n, c"warp")?.map(|w| w.max(0.0)) }))
            })(),
            _ => Err(format!("noise is a table ({{ seed = 7, octaves = 5, frequency = 0.004, ridged = 0.3, warp = 25 }}) or false for flat ground, got {}", type_name(L, n))),
        };
        lua_pop(L, 1);
        r
    }
}

/// `{ "hill", x, z, radius, height }`, `{ "crater", x, z, radius, depth, rim }`,
/// `{ "river", { x, z, x, z, … }, width, bank, bed }`.
unsafe fn stamps(L: *mut lua_State, t: c_int) -> Result<Vec<Stamp>, String> {
    unsafe {
        lua_rawgetfield(L, t, c"stamps".as_ptr());
        let list = abs(L, -1);
        let mut out = Vec::new();
        let r = (|| {
            match lua_type(L, list) {
                LUA_TNIL => return Ok(()),
                LUA_TTABLE => {}
                _ => {
                    return Err(format!("stamps is a list of stamps ({{ {{ \"hill\", 20, -10, radius = 18, height = 6 }} }}), got {}", type_name(L, list)));
                }
            }
            for k in 1..=lua_objlen(L, list) {
                lua_rawgeti(L, list, k);
                let s = abs(L, -1);
                let one = (|| {
                    if lua_type(L, s) != LUA_TTABLE {
                        return Err(format!("stamps[{k}] is a table like {{ \"hill\", x, z, radius = 18, height = 6 }}"));
                    }
                    lua_rawgeti(L, s, 1);
                    let kind = st(L, -1).map(str::to_string);
                    lua_pop(L, 1);
                    let at = |i: c_int| -> Result<f32, String> {
                        lua_rawgeti(L, s, i);
                        let v = num(L, -1).filter(|v| v.is_finite());
                        lua_pop(L, 1);
                        v.map(|v| v as f32).ok_or_else(|| format!("stamps[{k}]: its x and z come after its kind: {{ \"{}\", x, z, … }}", kind.as_deref().unwrap_or("hill")))
                    };
                    let need = |key: &CStr, what: &str| -> Result<f32, String> { maybe(L, s, key)?.ok_or_else(|| format!("stamps[{k}] needs {} = … ({what})", key.to_str().unwrap_or("?"))) };
                    let positive = |v: f32, key: &str| {
                        if v > 0.0 { Ok(v) } else { Err(format!("stamps[{k}].{key} is more than 0, got {}", fmt_num(v as f64))) }
                    };
                    match kind.as_deref() {
                        Some("hill") => {
                            if let Some(x) = unknown_keys(L, s, &["radius", "height"]) {
                                return Err(format!("stamps[{k}]: a hill has radius and height, not {x}"));
                            }
                            Ok(Stamp::Hill { x: at(2)?, z: at(3)?, radius: positive(need(c"radius", "meters")?, "radius")?, height: need(c"height", "meters at its middle; negative for a dip")? })
                        }
                        Some("crater") => {
                            if let Some(x) = unknown_keys(L, s, &["radius", "depth", "rim"]) {
                                return Err(format!("stamps[{k}]: a crater has radius, depth and rim, not {x}"));
                            }
                            Ok(Stamp::Crater { x: at(2)?, z: at(3)?, radius: positive(need(c"radius", "meters")?, "radius")?, depth: need(c"depth", "meters")?, rim: number(L, s, c"rim", 0.0)? })
                        }
                        Some("river") => {
                            if let Some(x) = unknown_keys(L, s, &["width", "bank", "bed"]) {
                                return Err(format!("stamps[{k}]: a river has width, bank and bed, not {x}"));
                            }
                            lua_rawgeti(L, s, 2);
                            let pts = super::three::flat(L, -1, "a river's points");
                            lua_pop(L, 1);
                            let pts = pts.map_err(|_| format!("stamps[{k}]: a river's points are one list: {{ \"river\", {{ x1, z1, x2, z2, … }}, width = 6 }}"))?;
                            if pts.len() < 4 || pts.len() % 2 != 0 {
                                return Err(format!("stamps[{k}]: a river needs two points at least, x and z each: {{ x1, z1, x2, z2 }}"));
                            }
                            let width = positive(number(L, s, c"width", 6.0)?, "width")?;
                            Ok(Stamp::River { points: pts.as_chunks::<2>().0.to_vec(), width, bank: number(L, s, c"bank", width)?.max(0.0), bed: maybe(L, s, c"bed")? })
                        }
                        Some(o) => Err(format!("stamps[{k}]: \"{o}\" isn't a stamp: \"hill\", \"crater\" or \"river\"")),
                        None => Err(format!("stamps[{k}] starts with its kind: {{ \"hill\", x, z, radius = 18, height = 6 }}")),
                    }
                })();
                lua_pop(L, 1);
                out.push(one?);
            }
            Ok(())
        })();
        lua_pop(L, 1);
        r.map(|_| out)
    }
}

unsafe fn paint(L: *mut lua_State, t: c_int) -> Result<Vec<PaintRule>, String> {
    unsafe {
        lua_rawgetfield(L, t, c"paint".as_ptr());
        let list = abs(L, -1);
        let mut out = Vec::new();
        let r = (|| {
            match lua_type(L, list) {
                LUA_TNIL => return Ok(()),
                LUA_TTABLE => {}
                _ => {
                    return Err(format!("paint is a list: {{ {{ surface = \"grass\" }}, {{ above = 60, surface = \"snow\" }}, {{ slope = 0.2, surface = \"rock\" }} }}, got {}", type_name(L, list)));
                }
            }
            for k in 1..=lua_objlen(L, list) {
                lua_rawgeti(L, list, k);
                let p = abs(L, -1);
                let one = (|| {
                    if lua_type(L, p) != LUA_TTABLE {
                        return Err(format!("paint[{k}] is a table: {{ surface = \"rock\", above = 20 }}"));
                    }
                    if let Some(x) = unknown_keys(L, p, &["surface", "color", "above", "below", "slope", "blend"]) {
                        return Err(format!("paint[{k}] has surface, color, above, below, slope and blend, not {x}"));
                    }
                    let c = with_field(L, p, c"color", || if lua_type(L, -1) == LUA_TNIL { Ok(None) } else { super::nodes3d::col(L, -1).map(Some) })
                        .map_err(|m| format!("paint[{k}]: {m}"))?;
                    let surface = with_field(L, p, c"surface", || match lua_type(L, -1) {
                        LUA_TNIL => Ok(None),
                        LUA_TSTRING => {
                            let n = st(L, -1).unwrap_or("");
                            Surface::from_name(n).map(Some).ok_or_else(|| format!("paint[{k}].surface \"{n}\" isn't a ground surface: {SURFACES}"))
                        }
                        _ => Err(format!("paint[{k}].surface is a name: {SURFACES}")),
                    })?;
                    if c.is_none() && surface.is_none() {
                        return Err(format!("paint[{k}] needs a surface ({SURFACES}) or a color, or both (the surface in that colour)"));
                    }
                    let slope = maybe(L, p, c"slope")?;
                    if slope.is_some_and(|s| !(0.0..=1.0).contains(&s)) {
                        return Err(format!("paint[{k}].slope is 1 − the ground's normal.y, 0 (flat) to 1 (a wall): 0.13 is 30°, 0.29 is 45°, 0.5 is 60°"));
                    }
                    Ok(PaintRule { color: c.map(|c| c >> 8), surface, above: maybe(L, p, c"above")?, below: maybe(L, p, c"below")?, slope, blend: maybe(L, p, c"blend")?.map(|b| b.max(0.0)) })
                })();
                lua_pop(L, 1);
                out.push(one?);
            }
            Ok(())
        })();
        lua_pop(L, 1);
        r.map(|_| out)
    }
}

/// A bundle file as a texture, made once a session.
fn session_texture(path: &str) -> Result<Option<Texture>, String> {
    if let Some(t) = TEXTURES.with(|m| m.borrow().get(path).copied()) {
        return Ok(Some(t));
    }
    let Some(asset) = super::world::loaded(path) else { return Ok(None) };
    let tex = Texture::try_new(asset).map_err(|code| match code {
        -6 => format!("texture: {path} hasn't loaded yet"),
        -4 => format!("texture: {path} doesn't fit: 64 textures at most, in 96 MB"),
        _ => format!("texture: {path} can't be a texture: it has to be a PNG or JPEG of at most 2048×2048"),
    })?;
    TEXTURES.with(|m| m.borrow_mut().insert(path.to_string(), tex));
    Ok(Some(tex))
}

/// A project image's bundle path, by its name (`"basalt"`, `"images/basalt.png"`).
fn project_image(h: &Host, what: &str, n: &str) -> Result<String, String> {
    let key = n.strip_prefix("images/").unwrap_or(n).rsplit_once('.').map_or(n, |(a, _)| a).to_string();
    let Some(im) = h.project.images.iter().find(|im| im.name == key) else {
        return Err(format!("{what}: there's no images/{key}.png (or .jpg)"));
    };
    if im.path.starts_with("atlas/") {
        return Err(format!("{what}: images/{key} is small enough that the packer put it in an atlas, so it can't be a texture: textures are 512 px or more a side"));
    }
    Ok(im.path.clone())
}

/// A project image as a texture, by its name.
fn own_texture(h: &Host, what: &str, n: &str) -> Result<Texture, String> {
    let path = project_image(h, what, n)?;
    session_texture(&path)?.ok_or_else(|| format!("{what}: {path} hasn't loaded"))
}

/// `surfaces = { rock = "basalt", grass = { image = "lawn", normal = "lawn_n", size = 3 } }`:
/// the game's images for built-in surfaces.
unsafe fn own_surfaces(L: *mut lua_State, h: &Host, t: c_int) -> Result<[Option<SurfaceImage>; 5], String> {
    unsafe {
        let mut out = [None; 5];
        lua_rawgetfield(L, t, c"surfaces".as_ptr());
        let list = abs(L, -1);
        let r = (|| {
            match lua_type(L, list) {
                LUA_TNIL => return Ok(()),
                LUA_TTABLE => {}
                _ => return Err(format!("surfaces is a table of images by surface: {{ rock = \"basalt\" }}, got {}", type_name(L, list))),
            }
            if let Some(k) = unknown_keys(L, list, &["grass", "meadow", "dirt", "rock", "snow"]) {
                return Err(format!("surfaces: \"{k}\" isn't a ground surface: {SURFACES}"));
            }
            for s in Surface::ALL {
                let name = std::ffi::CString::new(s.name()).unwrap();
                lua_rawgetfield(L, list, name.as_ptr());
                let v = abs(L, -1);
                let what = format!("surfaces.{}", s.name());
                let one = match lua_type(L, v) {
                    LUA_TNIL => Ok(None),
                    LUA_TSTRING => own_texture(h, &what, st(L, v).unwrap_or("")).map(|c| Some(SurfaceImage { color: Some(c), normal: None, size: s.size() })),
                    LUA_TTABLE => (|| {
                        if let Some(x) = unknown_keys(L, v, &["image", "normal", "size"]) {
                            return Err(format!("{what} has image, normal and size, not {x}"));
                        }
                        let image = with_field(L, v, c"image", || st(L, -1).map(str::to_string)).ok_or_else(|| format!("{what} needs an image: {{ image = \"basalt\" }}"))?;
                        let normal = with_field(L, v, c"normal", || st(L, -1).map(str::to_string));
                        let size = number(L, v, c"size", s.size())?;
                        if size <= 0.0 {
                            return Err(format!("{what}.size is the meters one repeat of the image covers, more than 0"));
                        }
                        Ok(Some(SurfaceImage { color: Some(own_texture(h, &what, &image)?), normal: normal.map(|n| own_texture(h, &format!("{what}.normal"), &n)).transpose()?, size }))
                    })(),
                    _ => Err(format!("{what} is an image's name, or {{ image = …, normal = …, size = … }}")),
                };
                lua_pop(L, 1);
                out[s as usize] = one?;
            }
            Ok(())
        })();
        lua_pop(L, 1);
        r.map(|_| out)
    }
}

/// The ground's images: each built-in surface the paint lays (its photo and
/// normal map, or the game's own from `surfaces`), and the game's
/// `texture` for rules that name only a colour. `texture = false` is no
/// images at all: every surface its colour.
unsafe fn images(L: *mut lua_State, h: &mut Host, t: c_int, paint: &[PaintRule]) -> Result<([Option<SurfaceImage>; 5], Option<SurfaceImage>), String> {
    unsafe {
        let (name, off) = with_field(L, t, c"texture", || match lua_type(L, -1) {
            LUA_TNIL => Ok((None, false)),
            LUA_TBOOLEAN => Ok((None, lua_toboolean(L, -1) == 0)),
            LUA_TSTRING => Ok((st(L, -1).map(str::to_string), false)),
            _ => Err(format!("texture is an image's name (under paint that names only colours), or false for no images, got {}", type_name(L, -1))),
        })?;
        let own = own_surfaces(L, h, t)?;
        if off {
            return Ok(([None; 5], None));
        }
        let texture = match &name {
            Some(n) => Some(SurfaceImage { color: Some(own_texture(h, "texture", n)?), normal: None, size: number(L, t, c"textureSize", 4.0)?.max(0.1) }),
            None => None,
        };
        // Only the surfaces the paint lays: all five for the natural ground.
        let used = |s: Surface| paint.is_empty() || paint.iter().any(|r| r.surface == Some(s) || (r.surface.is_none() && texture.is_none() && ter::ground::nearest(r.color.unwrap_or(0)) == s));
        let mut out = own;
        for s in Surface::ALL {
            if out[s as usize].is_some() || !used(s) {
                continue;
            }
            let Some(color) = session_texture(&surface_image(s))? else {
                // Packed by an `mb` from before the ground surfaces: colours alone.
                maimbrain::sys::log(maimbrain::sys::Level::Warn, "Terrain: this bundle has no built-in ground surfaces (pack it with a newer mb): the ground is its colours alone");
                return Ok(([None; 5], texture));
            };
            out[s as usize] = Some(SurfaceImage { color: Some(color), normal: session_texture(&surface_normal(s)).ok().flatten(), size: s.size() });
        }
        Ok((out, texture))
    }
}

unsafe fn make(L: *mut lua_State, h: &mut Host) -> Result<c_int, String> {
    unsafe {
        let t = match lua_type(L, 1) {
            LUA_TTABLE => 1,
            LUA_TNIL | -1 => {
                lua_settop(L, 0);
                lua_newtable(L);
                1
            }
            _ => {
                return Err(format!("takes one table: Terrain{{ noise = {{ seed = 7 }}, height = 60 }} (got {})", type_name(L, 1)));
            }
        };
        if h.three.is_none() {
            return Err(super::three::NOT_3D.into());
        }
        if let Some(k) = unknown_keys(L, t, KEYS) {
            let near = KEYS.iter().find(|n| super::node::distance(&k.to_ascii_lowercase(), &n.to_ascii_lowercase()) <= 2).map(|n| format!("; did you mean `{n}`?")).unwrap_or_default();
            return Err(format!("no key `{k}`{near} (noise, height, stamps, paint, surfaces, reach, detail, fog, size, collision, texture, textureSize)"));
        }
        if with_field(L, t, c"heightmap", || lua_type(L, -1) != LUA_TNIL) || with_field(L, t, c"resolution", || lua_type(L, -1) != LUA_TNIL) {
            return Err("heightmap isn't in engine 2.0 yet: make the land with noise and stamps (hill, crater, river)".into());
        }
        if h.three.as_deref().is_some_and(|w| w.ground.as_ref().is_some_and(|g| w.scene.alive(g.node))) {
            return Err("a round has one Terrain: change that one (raise), or destroy it first".into());
        }
        let paint = paint(L, t)?;
        let (surfaces, texture) = images(L, h, t, &paint)?;
        let desc = TerrainDesc {
            noise: noise(L, t)?,
            height: number(L, t, c"height", 150.0)?,
            stamps: stamps(L, t)?,
            paint,
            reach: number(L, t, c"reach", 400.0)?,
            detail: number(L, t, c"detail", 1.0)?,
            size: maybe(L, t, c"size")?,
            surfaces,
            texture,
        };
        if let Some(e) = desc.check() {
            return Err(e);
        }
        let ratio = desc.reach / desc.detail;
        if ratio > ter::WARN_RATIO + 0.5 {
            maimbrain::sys::log(
                maimbrain::sys::Level::Warn,
                &format!(
                    "Terrain: reach {} at detail {} draws about {}k of the phone's 150k triangles, leaving less for the game (reach ÷ detail of 400 is about 70k)",
                    fmt_num(desc.reach as f64),
                    fmt_num(desc.detail as f64),
                    (ter::triangles_at(ratio) / 1000.0).round()
                ),
            );
        }
        // Bodies land on the ground unless it says otherwise (physics3.rs's tiles).
        let collision = !with_field(L, t, c"collision", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
        let fog = !with_field(L, t, c"fog", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
        let visible = !with_field(L, t, c"visible", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
        let w = h.three.as_deref_mut().unwrap();
        let Some(n) = gfx3d::Node::new() else {
            return Err("too many nodes (4096 at most)".into());
        };
        let id = match w.scene.add(Some(n), true, None) {
            Ok(id) => id,
            Err(_) => {
                n.destroy();
                return Err("too many nodes (4096 at most)".into());
            }
        };
        let mut info = Info::new(Kind3::Terrain);
        info.name = with_field(L, t, c"name", || st(L, -1).unwrap_or("").to_string());
        info.tag = with_field(L, t, c"tag", || st(L, -1).unwrap_or("").to_string());
        w.info.insert(id.to_bits(), info);
        let reach = desc.reach.min(desc.size.map_or(f32::MAX, |s| s * 0.75));
        let mut terrain = crate::three::take_terrain(desc);
        terrain.set_visible(visible);
        if !visible {
            w.scene.set_visible(id, false);
        }
        w.ground = Some(Ground { node: id, terrain, fog, collision });
        // The camera sees as far as the ground goes, unless the game said.
        if !w.far_set && w.cam.far < reach * 1.05 {
            w.cam.far = reach * 1.05;
        }
        if fog {
            ground_fog(w);
        }
        ground_sun(w);
        push_node(L, w, id);
        Ok(1)
    }
}

/// The air a terrain is seen through (`fog = true`, the default), unless the
/// game set the fog itself: aerial perspective on a clear day. The near and
/// middle ground keep their full contrast; from 60% of the reach the far
/// ranges go toward the horizon's colour, gone at the reach, so its end is
/// never seen; the sky fades to it toward the horizon.
pub fn ground_fog(w: &World3) {
    let Some(g) = w.ground.as_ref().filter(|g| g.fog) else { return };
    if w.fog_set {
        return;
    }
    let reach = g.terrain.layout().reach;
    let [r, gr, b] = w.horizon.unwrap_or(gfx3d::Atmosphere::day().horizon);
    let color = [r * 0.94, gr * 0.96, b];
    gfx3d::fog(&Fog { color, mode: FogMode::Linear, start: reach * 0.6, end: reach, sky: 1.0, ..Fog::default() });
}

/// The sun a terrain gets when the game hasn't set one: lower than mb3d's
/// default (30° up, from behind the right shoulder of a camera looking down
/// −z), warm, with shadows, so the relief reads.
pub fn ground_sun(w: &World3) {
    if w.sun_set {
        return;
    }
    gfx3d::sun(&gfx3d::Sun { direction: gfx3d::vec3(0.55, -0.5, -0.67).normalize(), color: [1.0, 0.93, 0.82], intensity: 3.2, shadows: 1, shadow_distance: 50.0 });
}

/// `ground:heightAt(x, z)`, `ground:normalAt(x, z)` → nx, ny, nz,
/// `ground:raise(x, z, radius, amount)`.
pub unsafe fn method(L: *mut lua_State, w: &mut World3, id: Node3Id, a: A, name: &str) -> Result<c_int, String> {
    unsafe {
        let need = |i: c_int, what: &str| -> Result<f32, String> {
            match num(L, i) {
                Some(v) if v.is_finite() => Ok(v as f32),
                _ => Err(format!("bad argument #{} to '{name}' ({what} expected, got {})", i - 1, type_name(L, i))),
            }
        };
        let Some(g) = w.ground.as_mut().filter(|g| g.node == id) else {
            return Err(format!("{name}: this Terrain was destroyed"));
        };
        let (x, z) = (need(2, "x")?, need(3, "z")?);
        Ok(match a {
            A::HeightAt => ret(L, g.terrain.height_at(x, z) as f64),
            A::NormalAt => {
                let n: Vec3 = g.terrain.normal_at(x, z);
                for v in [n.x, n.y, n.z] {
                    lua_pushnumber(L, v as f64);
                }
                3
            }
            _ => {
                let (r, amount) = (need(4, "radius")?, need(5, "amount")?);
                if r <= 0.0 {
                    return Err(format!("raise(x, z, radius, amount): radius is more than 0, got {}", fmt_num(r as f64)));
                }
                g.terrain.raise(x, z, r, amount);
                // The bodies' ground there is made again from the new heights.
                let tile = g.terrain.layout().step(0) * crate::physics3::TILE_QUADS as f32;
                if let Some(p) = w.phys.as_deref_mut() {
                    p.ground_changed(x, z, r, tile);
                }
                0
            }
        })
    }
}

/// The round's terrain, for `scatter(node, { on = ground })`: the node at
/// stack index `i` when it's the live Terrain.
pub unsafe fn ground_at(L: *mut lua_State, w: &World3, i: c_int) -> Option<&Ground> {
    unsafe {
        let id = super::three::node_at(L, i)?;
        w.ground.as_ref().filter(|g| g.node == id && w.scene.alive(id))
    }
}
