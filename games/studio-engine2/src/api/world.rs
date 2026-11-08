//! Engine 2.0's 3D building blocks and the world's look (docs/STUDIO-2.md
//! §5 [3d], §9.3 row 2a): `geometry.*` (three.js's names, with lofts and
//! fish from the SDK's `organic`), `material{}` with three.js's keys (1.0's
//! keys still work: the table says which), `world.*` (sky v2, the sun,
//! lighting presets, fog, post, depth of field, speed blur, the beauty pass,
//! water, the retro look and its backdrop), `splash`, `shockwave` and
//! `scatter`. Colours are 2.0's: `0xRRGGBB` or `{ r, g, b }` from 0 to 1.

use std::ffi::{CStr, c_int, c_void};

use maimbrain::gfx3d::{
    self, Atmosphere, Clouds, DepthOfField, Film, FishDesc, Fog, FogMode, Grade, Halos, Instance, LightShafts, Lighting, Loft, Material, MaterialDesc, MaterialStyle, MeshData, Post, Quat, Reflector,
    Retro, Sky, SkyDisc, SkyTexture, SpeedBlur, Splash, Sun, Texture, Transform, Vec3, Water, vec3,
};

use super::nodes3d::{col, lin};
use super::three::{abs, fflag, flat, fnum, fnums, node_at, with_field};
use super::*;
use crate::three::World3;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(
            L,
            ud,
            "geometry",
            &[
                (c"box", g_box),
                (c"sphere", g_sphere),
                (c"ico", g_ico),
                (c"rock", g_rock),
                (c"cylinder", g_cylinder),
                (c"cone", g_cone),
                (c"plane", g_plane),
                (c"torus", g_torus),
                (c"lathe", g_lathe),
                (c"extrude", g_extrude),
                (c"tris", g_tris),
                (c"loft", g_loft),
                (c"fish", g_fish),
            ],
        );
        register(
            L,
            ud,
            "world",
            &[
                (c"sky", w_sky),
                (c"sun", w_sun),
                (c"lighting", w_lighting),
                (c"fog", w_fog),
                (c"post", w_post),
                (c"dof", w_dof),
                (c"speedBlur", w_speed_blur),
                (c"grade", w_grade),
                (c"shafts", w_shafts),
                (c"halos", w_halos),
                (c"film", w_film),
                (c"water", w_water),
                (c"retro", w_retro),
                (c"backdrop", w_backdrop),
            ],
        );
        for (name, f) in [(c"material", material as RustFn), (c"scatter", scatter), (c"shockwave", shockwave), (c"splash", splash)] {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
    }
}

macro_rules! w {
    ($L:ident, $h:ident) => {
        match $h.three.as_deref_mut() {
            Some(w) => w,
            None => return err($L, super::three::NOT_3D),
        }
    };
}

/// A 2.0 colour field of the table at `t` (`0xRRGGBB`, `{ r, g, b }`), linear, or `d`.
unsafe fn fcol(L: *mut lua_State, t: c_int, k: &CStr, d: [f32; 3]) -> Result<[f32; 3], String> {
    unsafe {
        with_field(L, t, k, || {
            if lua_type(L, -1) == LUA_TNIL {
                return Ok(d);
            }
            let c = lin(col(L, -1).map_err(|m| format!("{}: {m}", k.to_str().unwrap_or("color")))?);
            Ok([c[0], c[1], c[2]])
        })
    }
}

thread_local! {
    /// The bundle's files, loaded before anything runs. A new `Asset::load` of
    /// one wouldn't be ready until the next frame, and a texture is asked for now.
    static LOADED: std::cell::RefCell<std::collections::HashMap<String, maimbrain::sys::Asset>> = Default::default();
}

/// The files a project's textures come from (`assets/files.txt`'s, once they're here).
pub fn loaded_files(files: &[(String, maimbrain::sys::Asset)]) {
    LOADED.with(|l| *l.borrow_mut() = files.iter().cloned().collect());
}

/// A bundle file's asset (`assets/files.txt`'s), loaded before anything ran.
/// Natively nothing loads (SPEC §9.8), so any file answers.
pub fn loaded(path: &str) -> Option<maimbrain::sys::Asset> {
    let a = LOADED.with(|l| l.borrow().get(path).copied());
    #[cfg(not(target_arch = "wasm32"))]
    let a = a.or(Some(maimbrain::sys::Asset(1)));
    a
}

/// A project image as a texture (made once a round). Images the packer
/// put in an atlas can't be: they share their picture.
pub fn texture(h: &mut Host, name: &str) -> Result<Texture, String> {
    let key = name.strip_prefix("images/").unwrap_or(name).rsplit_once('.').map_or(name, |(a, _)| a);
    let key = key.strip_prefix("images/").unwrap_or(key);
    let Some(i) = h.project.images.iter().position(|im| im.name == key) else {
        return Err(format!("there's no images/{key}.png (or .jpg)"));
    };
    let path = h.project.images[i].path.clone();
    if path.starts_with("atlas/") {
        return Err(format!("images/{key} is small enough that the packer put it in an atlas, so it can't be a texture: textures are 512 px or more a side"));
    }
    let w = h.three.as_deref_mut().ok_or(super::three::NOT_3D)?;
    if let Some((_, t)) = w.textures.iter().find(|t| t.0 == i) {
        return Ok(*t);
    }
    let asset = LOADED.with(|l| l.borrow().get(&path).copied()).unwrap_or_else(|| maimbrain::sys::Asset::load(&format!("assets/{path}")));
    let t = Texture::try_new(asset).map_err(|code| match code {
        -6 => format!("images/{key} hasn't loaded yet"),
        -4 => format!("images/{key} doesn't fit: 64 textures at most, in 96 MB"),
        _ => format!("images/{key} can't be a texture: it has to be a PNG or JPEG of at most 2048×2048"),
    })?;
    w.textures.push((i, t));
    Ok(t)
}

// ---------------------------------------------------------------- geometry

unsafe fn push_geo(L: *mut lua_State, h: &mut Host, data: MeshData) -> c_int {
    unsafe {
        let w = w!(L, h);
        let i = w.add_mesh(data, true);
        push_handle(L, tag::GEOMETRY, i as u32 + 1)
    }
}

b!(g_box, |L, h| {
    let [x, y, z, _] = super::three::args4(L, [1.0, 1.0, 1.0, 0.0]);
    push_geo(L, h, MeshData::cuboid(vec3(x, y, z)))
});
b!(g_sphere, |L, h| {
    let [r, s, k, _] = super::three::args4(L, [0.5, 24.0, 16.0, 0.0]);
    push_geo(L, h, MeshData::uv_sphere(r, (s as u32).clamp(3, 128), (k as u32).clamp(2, 128)))
});
b!(g_ico, |L, h| {
    let [r, n, _, _] = super::three::args4(L, [0.5, 1.0, 0.0, 0.0]);
    push_geo(L, h, MeshData::icosphere(r, (n as u32).min(5)))
});
// geometry.rock(size, seed): a jagged, flat-shaded boulder `size` meters
// across, the same for the same seed.
b!(g_rock, |L, h| {
    let [size, seed, _, _] = super::three::args4(L, [1.0, 1.0, 0.0, 0.0]);
    push_geo(L, h, MeshData::rock(size.max(0.01), seed.abs() as u32))
});
b!(g_cylinder, |L, h| {
    let [r, hh, s, _] = super::three::args4(L, [0.5, 1.0, 16.0, 0.0]);
    push_geo(L, h, MeshData::cylinder(r, hh, (s as u32).clamp(3, 128)))
});
b!(g_cone, |L, h| {
    let [r, hh, s, _] = super::three::args4(L, [0.5, 1.0, 16.0, 0.0]);
    push_geo(L, h, MeshData::cone(r, hh, (s as u32).clamp(3, 128)))
});
b!(g_plane, |L, h| {
    let [x, z, _, _] = super::three::args4(L, [1.0, 1.0, 0.0, 0.0]);
    push_geo(L, h, MeshData::plane(x, z))
});
b!(g_torus, |L, h| {
    let [r, t, s, k] = super::three::args4(L, [0.5, 0.15, 24.0, 12.0]);
    push_geo(L, h, MeshData::torus(r, t, (s as u32).clamp(3, 128), (k as u32).clamp(3, 64)))
});
b!(g_lathe, |L, h| {
    let p = match flat(L, 1, "geometry.lathe") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
    push_geo(L, h, MeshData::lathe(p.as_chunks::<2>().0, (opt(L, 2, 16.0) as u32).clamp(3, 128)))
});
b!(g_extrude, |L, h| {
    let p = match flat(L, 1, "geometry.extrude") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
    let Some(depth) = num(L, 2) else {
        return bad(L, 2, "geometry.extrude", "number");
    };
    push_geo(L, h, MeshData::extrude(p.as_chunks::<2>().0, depth as f32))
});
b!(g_tris, |L, h| {
    let p = match flat(L, 1, "geometry.tris") {
        Ok(p) => p,
        Err(e) => return err(L, &e),
    };
    let mut m = MeshData::new();
    m.colors = Vec::new();
    for t in p.as_chunks::<9>().0 {
        let (a, b, c) = (vec3(t[0], t[1], t[2]), vec3(t[3], t[4], t[5]), vec3(t[6], t[7], t[8]));
        let n = (b - a).cross(c - a).normalize();
        let base = m.vertex_count() as u32;
        m.push(a, n, [0.0, 1.0]);
        m.push(b, n, [1.0, 1.0]);
        m.push(c, n, [0.5, 0.0]);
        m.tri(base, base + 1, base + 2);
    }
    push_geo(L, h, m)
});

// `geometry.loft{ points = { x, y, z, … }, sides, rings, radius = r or fn(t, u), open, caps }`:
// a section swept along a smooth spine (a tube when `radius` is a number).
b!(g_loft, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return err(L, "geometry.loft{ points = { x1, y1, z1, x2, y2, z2, … }, sides = 12, radius = 0.2 } (radius may be a function(t, u))");
    }
    lua_rawgetfield(L, 1, c"points".as_ptr());
    let pts = flat(L, -1, "geometry.loft: points");
    lua_pop(L, 1);
    let pts = match pts {
        Ok(p) if p.len() >= 6 => p.as_chunks::<3>().0.iter().map(|p| vec3(p[0], p[1], p[2])).collect::<Vec<_>>(),
        Ok(_) => {
            return err(L, "geometry.loft: points needs two points at least ({ x1, y1, z1, x2, y2, z2 })");
        }
        Err(e) => return err(L, &e),
    };
    let open = fflag(L, 1, c"open");
    let desc = Loft {
        spine: pts,
        sides: (fnum(L, 1, c"sides", 12.0) as u32).clamp(1, 64),
        rings: (fnum(L, 1, c"rings", 24.0) as u32).clamp(2, 256),
        closed: !open,
        caps: !with_field(L, 1, c"caps", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0),
        ends: fnum(L, 1, c"ends", 0.0),
        ..Default::default()
    };
    lua_rawgetfield(L, 1, c"radius".as_ptr());
    let rf = lua_gettop(L);
    let fixed = num(L, rf).map(|r| r as f32);
    if fixed.is_none() && lua_type(L, rf) != LUA_TFUNCTION {
        lua_pop(L, 1);
        return err(L, "geometry.loft: radius is a number or a function(t, u) returning one (t along the spine, u around it, both 0–1)");
    }
    // The radius function runs per point, under protection; the first error ends it.
    let fail: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    let m = MeshData::loft(&desc, |t, u| {
        let r = match fixed {
            Some(r) => r,
            None if fail.borrow().is_some() => 0.0,
            None => {
                lua_pushvalue(L, rf);
                lua_pushnumber(L, t as f64);
                lua_pushnumber(L, u as f64);
                if lua_pcall(L, 2, 1, 0) != LUA_OK {
                    *fail.borrow_mut() = Some(to_s(L, -1));
                    lua_pop(L, 1);
                    0.0
                } else {
                    let r = num(L, -1).unwrap_or(0.0) as f32;
                    lua_pop(L, 1);
                    r
                }
            }
        };
        let a = u * std::f32::consts::TAU;
        if open { vec3((u - 0.5) * 2.0 * r, 0.0, 0.0) } else { vec3(a.cos() * r, a.sin() * r, 0.0) }
    });
    lua_pop(L, 1);
    if let Some(e) = fail.into_inner() {
        return err(L, &format!("geometry.loft: radius: {e}"));
    }
    push_geo(L, h, if open { m.double_sided() } else { m })
});

// `geometry.fish{ length, height, fins }`: a fish's body, fins and eyes in one geometry.
b!(g_fish, |L, h| {
    let d = FishDesc::default();
    let desc = FishDesc { length: fnum(L, 1, c"length", d.length).max(0.05), height: fnum(L, 1, c"height", d.height).max(0.01), fins: fnum(L, 1, c"fins", d.fins), ..d };
    let f = MeshData::fish(&desc);
    let mut m = f.body;
    m.merge(&f.fins.double_sided());
    m.merge(&f.eyes.with_vertex_colors(0x0a0a0cff));
    push_geo(L, h, m)
});

/// A geometry's edit, returning it so edits chain.
unsafe fn edit(L: *mut lua_State, h: &mut Host, f: &str, e: impl FnOnce(&mut MeshData, &World3) -> Result<(), String>) -> c_int {
    unsafe { super::three::edit(L, h, f, e) }
}

// g:merge(other, x, y, z, rx, ry, rz, s): another geometry placed into this one (one Mesh is one draw).
b!(g_merge, |L, h| {
    let other = handle(L, 2, tag::GEOMETRY);
    let itself = other.is_some() && other == handle(L, 1, tag::GEOMETRY);
    let g = |i: c_int, d: f64| opt(L, i, d) as f32;
    let t = Transform { pos: vec3(g(3, 0.0), g(4, 0.0), g(5, 0.0)), rot: Quat::from_euler(g(7, 0.0), g(6, 0.0), g(8, 0.0)), scale: Vec3::splat(g(9, 1.0)) };
    edit(L, h, "merge", |m, w| {
        let o = other.ok_or("merge(other, x, y, z, rx, ry, rz, s) takes a geometry")? as usize - 1;
        let src = if itself { m.clone() } else { w.meshes[o].data.clone() };
        m.merge(&src.transformed(&t.matrix()));
        Ok(())
    })
});
// g:paint(color): every vertex this colour (0xRRGGBB).
b!(g_paint, |L, h| {
    let c = col(L, 2);
    edit(L, h, "paint", |m, _| {
        let c = c?;
        *m = std::mem::take(m).with_vertex_colors(c);
        Ok(())
    })
});
// g:smooth(crease): smooth normals, kept hard across edges sharper than `crease` radians.
b!(g_smooth, |L, h| {
    let crease = opt(L, 2, 1.0) as f32;
    edit(L, h, "smooth", |m, _| {
        *m = m.smooth_normals(crease);
        Ok(())
    })
});
// g:subdivide(n): Loop subdivision n times (each quadruples the triangles; 3 at most).
b!(g_subdivide, |L, h| {
    let n = opt(L, 2, 1.0).clamp(0.0, 3.0) as u32;
    edit(L, h, "subdivide", |m, _| {
        if m.indices.len() * 4usize.pow(n) > 600_000 {
            return Err("that many triangles is past a mesh's budget: subdivide less".into());
        }
        *m = m.subdivide(n, 1.0);
        Ok(())
    })
});

// --------------------------------------------------------------- materials

/// 1.0's material keys and 2.0's (three.js's); `color`, `emissive`, `unlit`
/// and `additive` are both's.
const V1_KEYS: &[&CStr] = &[c"metal", c"rough", c"glow", c"double", c"alpha", c"style"];
const V2_KEYS: &[&CStr] =
    &[c"emissiveIntensity", c"metalness", c"roughness", c"texture", c"doubleSided", c"transparent", c"opacity", c"toon", c"rim", c"outline", c"ink", c"fog", c"shader", c"uniforms", c"textures"];

/// Whether `material{…}`'s table is 1.0's: one of 1.0's own keys, or (with
/// only the shared ones) a colour past `0xffffff`, so `0xRRGGBBAA`.
unsafe fn is_v1(L: *mut lua_State) -> bool {
    unsafe {
        let has = |k: &CStr| with_field(L, 1, k, || lua_type(L, -1) != LUA_TNIL);
        if V1_KEYS.iter().any(|k| has(k)) {
            return true;
        }
        if V2_KEYS.iter().any(|k| has(k)) {
            return false;
        }
        with_field(L, 1, c"color", || lua_type(L, -1) == LUA_TNUMBER && lua_tonumber(L, -1) > 16_777_215.0)
    }
}

// material{ color, emissive, emissiveIntensity, metalness, roughness, texture, unlit, doubleSided,
// transparent, opacity, additive, toon, rim, outline, ink, fog } (three.js's), or 1.0's keys.
b!(material, |L, h| {
    if !matches!(lua_type(L, 1), LUA_TTABLE | LUA_TNIL | -1) {
        return bad(L, 1, "material", "table");
    }
    if lua_type(L, 1) == LUA_TTABLE && is_v1(L) {
        return super::three::material(L, h as *mut Host as *mut c_void);
    }
    match material2(L, h) {
        Ok(n) => n,
        Err(m) => err(L, &format!("material{{…}}: {m}")),
    }
});

unsafe fn material2(L: *mut lua_State, h: &mut Host) -> Result<c_int, String> {
    unsafe {
        let tex = match with_field(L, 1, c"texture", || st(L, -1).map(str::to_string)) {
            Some(n) => Some(texture(h, &n).map_err(|e| format!("texture: {e}"))?),
            None => None,
        };
        let w = h.three.as_deref_mut().ok_or(super::three::NOT_3D)?;
        let d = MaterialDesc::default();
        let c = with_field(L, 1, c"color", || if lua_type(L, -1) == LUA_TNIL { Ok(0xffffffff) } else { col(L, -1) }).map_err(|m| format!("color: {m}"))?;
        let mut base = lin(c);
        base[3] = fnum(L, 1, c"opacity", 1.0).clamp(0.0, 1.0);
        let has_emissive = with_field(L, 1, c"emissive", || lua_type(L, -1) != LUA_TNIL);
        let emissive = fcol(L, 1, c"emissive", [0.0; 3])?;
        let mut flags = 0;
        for (k, bit) in [(c"unlit", Material::UNLIT), (c"doubleSided", Material::DOUBLE_SIDED), (c"transparent", Material::ALPHA), (c"additive", Material::ADDITIVE)] {
            if fflag(L, 1, k) {
                flags |= bit;
            }
        }
        if base[3] < 1.0 {
            flags |= Material::ALPHA;
        }
        let desc = MaterialDesc {
            base_color: base,
            emissive,
            emissive_strength: fnum(L, 1, c"emissiveIntensity", if has_emissive { 1.0 } else { d.emissive_strength }).max(0.0),
            metallic: fnum(L, 1, c"metalness", d.metallic).clamp(0.0, 1.0),
            roughness: fnum(L, 1, c"roughness", d.roughness).clamp(0.0, 1.0),
            base_tex: tex,
            flags,
            ..d
        };
        let m = Material::new(&desc).ok_or("too many materials (1024 at most): share them between meshes")?;
        let styled = [c"toon", c"rim", c"outline", c"ink", c"fog"].iter().any(|k| with_field(L, 1, k, || lua_type(L, -1) != LUA_TNIL));
        if styled {
            let ink = fcol(L, 1, c"ink", [0.0; 3])?;
            let nofog = with_field(L, 1, c"fog", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
            m.set_style(&MaterialStyle {
                outline_color: ink,
                outline_width: fnum(L, 1, c"outline", 0.0).clamp(0.0, 8.0),
                toon_bands: fnum(L, 1, c"toon", 0.0).clamp(0.0, 8.0) as u32,
                rim: fnum(L, 1, c"rim", 0.0).clamp(0.0, 1.0),
                flags: if nofog { MaterialStyle::NO_FOG } else { 0 },
                ..Default::default()
            });
        }
        w.materials.push((m, true));
        let n = w.materials.len() as u32;
        // shader = "dissolve" (shaders/dissolve.wgsl), its uniforms and textures (shaders.rs).
        if with_field(L, 1, c"shader", || lua_type(L, -1) != LUA_TNIL) {
            super::shaders::material(L, h, m)?;
        } else if with_field(L, 1, c"uniforms", || lua_type(L, -1) != LUA_TNIL) {
            return Err("uniforms are a shader's: give the material a shader = \"<name>\" (shaders/<name>.wgsl)".into());
        }
        Ok(push_handle(L, tag::MATERIAL, n))
    }
}

// ------------------------------------------------------------------- world.*

/// A preset name at `i`, or None for a table (or nothing).
unsafe fn preset<'a>(L: *mut lua_State, i: c_int) -> Option<&'a str> {
    unsafe { st(L, i) }
}

unsafe fn unknown_preset(L: *mut lua_State, f: &str, got: &str, list: &str) -> c_int {
    unsafe { err(L, &format!("{f}(\"{got}\"): the presets are {list}")) }
}

// world.sky{ atmosphere = "dusk", clouds = { cover = 0.4 }, disc = "moon", panorama = "night", stars = 0.6 }
b!(w_sky, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return err(L, "world.sky{ atmosphere = \"day\" | \"dusk\" | \"night\", clouds = { cover = 0.4 }, disc = \"sun\" | \"moon\", panorama = \"<image>\", stars = 0.6 }");
    }
    let atmo_name = with_field(L, 1, c"atmosphere", || st(L, -1).map(str::to_string));
    let atmo = match atmo_name.as_deref() {
        Some("day") => Atmosphere::day(),
        Some("dusk") => Atmosphere::dusk(),
        Some("night") => Atmosphere::night(),
        None => Atmosphere::default(),
        Some(o) => return unknown_preset(L, "world.sky: atmosphere", o, "day, dusk, night"),
    };
    // Clouds: the atmosphere's own look, with cover (0–1) over it.
    let clouds = match with_field(L, 1, c"clouds", || lua_type(L, -1)) {
        LUA_TNIL => Clouds::default(),
        LUA_TBOOLEAN => {
            if with_field(L, 1, c"clouds", || lua_toboolean(L, -1) != 0) {
                cloud_preset(atmo_name.as_deref())
            } else {
                Clouds::default()
            }
        }
        LUA_TSTRING => match with_field(L, 1, c"clouds", || st(L, -1).map(str::to_string)).as_deref() {
            Some(n @ ("day" | "dusk" | "night")) => cloud_preset(Some(n)),
            Some(o) => return unknown_preset(L, "world.sky: clouds", o, "day, dusk, night"),
            None => Clouds::default(),
        },
        _ => {
            lua_rawgetfield(L, 1, c"clouds".as_ptr());
            let c = cloud_preset(atmo_name.as_deref());
            let c = Clouds { coverage: fnum(L, -1, c"cover", c.coverage).clamp(0.0, 1.0), ..c };
            lua_pop(L, 1);
            c
        }
    };
    let disc = match with_field(L, 1, c"disc", || st(L, -1).map(str::to_string)).as_deref() {
        Some("sun") => SkyDisc::sun(),
        Some("moon") => SkyDisc::moon(),
        None => SkyDisc::default(),
        Some(o) => return unknown_preset(L, "world.sky: disc", o, "sun, moon"),
    };
    let pano = match with_field(L, 1, c"panorama", || st(L, -1).map(str::to_string)) {
        Some(n) => match texture(h, &n) {
            Ok(t) => Some(t),
            Err(e) => return err(L, &format!("world.sky: panorama: {e}")),
        },
        None => None,
    };
    let stars = fnum(L, 1, c"stars", 0.0).clamp(0.0, 4.0);
    let d = Sky::default();
    gfx3d::sky(&Sky { nebula_density: fnum(L, 1, c"nebula", 0.0).clamp(0.0, 4.0), star_density: stars, star_brightness: if stars > 0.0 { 1.0 } else { d.star_brightness }, sun_disc: 0.0, ..d });
    gfx3d::atmosphere(&atmo);
    if let Some(w) = h.three.as_deref_mut() {
        // A terrain's fog takes the horizon's colour.
        w.horizon = atmo_name.is_some().then_some(atmo.horizon);
        super::terrain::ground_fog(w);
    }
    gfx3d::clouds(&clouds);
    gfx3d::sky_disc(&disc);
    gfx3d::sky_texture(pano, &SkyTexture { intensity: fnum(L, 1, c"panoramaIntensity", 1.0), ..Default::default() });
    0
});

fn cloud_preset(name: Option<&str>) -> Clouds {
    match name {
        Some("night") => Clouds::night(),
        Some("dusk") => Clouds::dusk(),
        _ => Clouds::day(),
    }
}

// world.sun{ dir = { x, y, z }, color, intensity, shadows = true, shadowDistance }
b!(w_sun, |L, h| {
    if let Some(w) = h.three.as_deref_mut() {
        w.sun_set = true;
    }
    let d = Sun::default();
    let color = match fcol(L, 1, c"color", d.color) {
        Ok(c) => c,
        Err(m) => return err(L, &format!("world.sun: {m}")),
    };
    gfx3d::sun(&Sun {
        direction: Vec3::from(fnums(L, 1, c"dir", d.direction.to_array())),
        color,
        intensity: fnum(L, 1, c"intensity", d.intensity).max(0.0),
        shadows: fflag(L, 1, c"shadows") as u32,
        shadow_distance: fnum(L, 1, c"shadowDistance", d.shadow_distance),
    });
    0
});

// world.lighting("hero"): the sun, sky light and exposure for a textured hero.
b!(w_lighting, |L, h| {
    if let Some(w) = h.three.as_deref_mut() {
        w.sun_set = true;
    }
    match preset(L, 1) {
        Some("hero") => gfx3d::lighting(&Lighting::hero()),
        Some(o) => return unknown_preset(L, "world.lighting", o, "hero"),
        None => return bad(L, 1, "world.lighting", "preset name (\"hero\")"),
    }
    0
});

// world.fog{ color, mode = "exp" | "exp2" | "linear" | "off", density, start, ["end"], height, falloff, sky }
b!(w_fog, |L, h| {
    let mode = match with_field(L, 1, c"mode", || st(L, -1).map(str::to_string)).as_deref() {
        Some("linear") => FogMode::Linear,
        Some("exp2") => FogMode::ExponentialSquared,
        Some("off") => FogMode::Off,
        Some("exp") | None => FogMode::Exponential,
        Some(o) => {
            return err(L, &format!("world.fog: mode = \"{o}\" (\"exp\", \"exp2\", \"linear\" or \"off\")"));
        }
    };
    let d = Fog::default();
    let color = match fcol(L, 1, c"color", d.color) {
        Ok(c) => c,
        Err(m) => return err(L, &format!("world.fog: {m}")),
    };
    // The game's fog: a terrain's own (`fog = true`) gives way to it.
    if let Some(w) = h.three.as_deref_mut() {
        w.fog_set = true;
    }
    gfx3d::fog(&Fog {
        color,
        mode,
        start: fnum(L, 1, c"start", d.start),
        end: fnum(L, 1, c"end", d.end),
        density: fnum(L, 1, c"density", d.density),
        height: fnum(L, 1, c"height", d.height),
        height_falloff: fnum(L, 1, c"falloff", d.height_falloff),
        sky: fnum(L, 1, c"sky", d.sky),
        ..d
    });
    0
});

// world.post{ exposure, bloom, bloomThreshold, vignette, aberration, saturation, contrast, sharpen }
b!(w_post, |L, h| {
    let d = Post::default();
    gfx3d::post(&Post {
        exposure: fnum(L, 1, c"exposure", d.exposure),
        bloom_strength: fnum(L, 1, c"bloom", d.bloom_strength),
        bloom_threshold: fnum(L, 1, c"bloomThreshold", d.bloom_threshold),
        vignette: fnum(L, 1, c"vignette", d.vignette),
        chromatic_aberration: fnum(L, 1, c"aberration", d.chromatic_aberration),
        saturation: fnum(L, 1, c"saturation", d.saturation),
        contrast: fnum(L, 1, c"contrast", d.contrast),
        sharpen: fnum(L, 1, c"sharpen", d.sharpen),
    });
    0
});

// world.dof{ focus, range, blur, near }, or world.dof() for none.
b!(w_dof, |L, h| {
    let d = if lua_type(L, 1) == LUA_TTABLE {
        DepthOfField { focus: fnum(L, 1, c"focus", 5.0), range: fnum(L, 1, c"range", 1.0), blur: fnum(L, 1, c"blur", 4.0), near: fnum(L, 1, c"near", 0.4) }
    } else {
        DepthOfField { focus: 0.0, range: 0.0, blur: 0.0, near: 0.0 }
    };
    gfx3d::depth_of_field(&d);
    0
});

// world.speedBlur{ amount, x, y, motion = { x, y }, stretch }: a zoom blur toward (x, y) on the canvas.
b!(w_speed_blur, |L, h| {
    let [sw, sh, ox, oy, s] = h.screen;
    let (cx, cy) = (ox + fnum(L, 1, c"x", (sw * 0.5 - ox) / s.max(1e-3)) * s, oy + fnum(L, 1, c"y", (sh * 0.5 - oy) / s.max(1e-3)) * s);
    gfx3d::speed_blur(&SpeedBlur {
        radial: fnum(L, 1, c"amount", 0.0).clamp(0.0, 1.0),
        center: [cx, cy],
        motion: fnums(L, 1, c"motion", [0.0, 0.0]),
        stretch: fnum(L, 1, c"stretch", 0.0).clamp(0.0, 1.0),
        _reserved: [0; 2],
    });
    0
});

// world.grade("moonlit" | "goldenHour" | "neon" | "off", amount)
b!(w_grade, |L, h| {
    let mut g = match preset(L, 1) {
        Some("moonlit") => Grade::moonlit(),
        Some("goldenHour") => Grade::golden_hour(),
        Some("neon") => Grade::neon(),
        Some("off") | None => Grade::none(),
        Some(o) => return unknown_preset(L, "world.grade", o, "moonlit, goldenHour, neon, off"),
    };
    if let Some(a) = num(L, 2) {
        g.amount = (a as f32).clamp(0.0, 1.0);
    }
    gfx3d::grade(&g);
    0
});
b!(w_shafts, |L, h| {
    let s = match preset(L, 1) {
        Some("moonbeams") => LightShafts::moonbeams(),
        Some("sunbeams") => LightShafts::sunbeams(),
        Some("off") | None => LightShafts::default(),
        Some(o) => return unknown_preset(L, "world.shafts", o, "moonbeams, sunbeams, off"),
    };
    gfx3d::shafts(&s);
    0
});
b!(w_halos, |L, h| {
    let s = match preset(L, 1) {
        Some("mist") => Halos::mist(),
        Some("off") | None => Halos::default(),
        Some(o) => return unknown_preset(L, "world.halos", o, "mist, off"),
    };
    gfx3d::halos(&s);
    0
});
b!(w_film, |L, h| {
    let s = match preset(L, 1) {
        Some("subtle") => Film::subtle(),
        Some("off") | None => Film::default(),
        Some(o) => return unknown_preset(L, "world.film", o, "subtle, off"),
    };
    gfx3d::film(&s);
    0
});

// world.water{ preset = "lake" | "pond" | "canal", height = 0, size = 200, flow = { x, z, speed } }, or world.water() for none.
b!(w_water, |L, h| {
    let w = w!(L, h);
    if lua_type(L, 1) != LUA_TTABLE {
        if let Some(n) = w.water.take() {
            n.destroy();
        }
        gfx3d::water(&Water::default());
        gfx3d::reflector(&Reflector::default());
        return 0;
    }
    let mut water = match with_field(L, 1, c"preset", || st(L, -1).map(str::to_string)).as_deref() {
        Some("lake") | None => Water::lake(),
        Some("pond") => Water::pond(),
        Some("canal") => Water::canal(),
        Some(o) => return unknown_preset(L, "world.water: preset", o, "lake, pond, canal"),
    };
    let height = fnum(L, 1, c"height", 0.0);
    let size = fnum(L, 1, c"size", 200.0).clamp(1.0, 2000.0);
    if with_field(L, 1, c"flow", || lua_type(L, -1) == LUA_TTABLE) {
        lua_rawgetfield(L, 1, c"flow".as_ptr());
        let (x, z, s) = (fnum(L, -1, c"x", 0.0), fnum(L, -1, c"z", 1.0), fnum(L, -1, c"speed", 0.3));
        lua_pop(L, 1);
        water = water.flowing(vec3(x, 0.0, z), s);
    }
    // The surface: a reflective plane at the height, mirrored by the reflector.
    if w.water.is_none() {
        let surface = MaterialDesc { base_color: [0.006, 0.01, 0.018, 1.0], roughness: 0.06, flags: Material::REFLECTIVE, ..Default::default() };
        let (Some(m), Some(mesh)) = (Material::new(&surface), MeshData::plane(size, size).upload()) else {
            return err(L, "world.water: too many meshes or materials");
        };
        w.materials.push((m, true));
        w.meshes.push(crate::three::MeshSlot { data: MeshData::new(), uploaded: Some(mesh), v2: true });
        w.water = gfx3d::Node::with_mesh(mesh, m);
    }
    if let Some(n) = w.water {
        n.set_transform(&Transform::at(vec3(0.0, height, 0.0)));
    }
    gfx3d::reflector(&Reflector { max_distance: size * 0.5, ripple: 0.15, ..Reflector::water(height) });
    gfx3d::water(&water);
    0
});

// world.retro("ps1" | "saturn" | "snes" | "arcade" | "off")
b!(w_retro, |L, h| {
    let r = match preset(L, 1) {
        Some("ps1") => Retro::ps1(),
        Some("saturn") => Retro::saturn(),
        Some("snes") => Retro::snes(),
        Some("arcade") => Retro::arcade(),
        Some("off") | None => Retro::default(),
        Some(o) => return unknown_preset(L, "world.retro", o, "ps1, saturn, snes, arcade, off"),
    };
    gfx3d::retro(&r);
    0
});

// world.backdrop{ { image = "mountains", scroll = 0.3, y = 0.5 }, … } (4 at most), or world.backdrop() for none.
b!(w_backdrop, |L, h| {
    let mut layers = Vec::new();
    if lua_type(L, 1) == LUA_TTABLE {
        for k in 1..=lua_objlen(L, 1).min(4) {
            lua_rawgeti(L, 1, k);
            let l = abs(L, -1);
            let Some(name) = with_field(L, l, c"image", || st(L, -1).map(str::to_string)) else {
                lua_pop(L, 1);
                return err(L, &format!("world.backdrop: layer {k} needs image = \"<name>\""));
            };
            let t = match texture(h, &name) {
                Ok(t) => t,
                Err(e) => {
                    lua_pop(L, 1);
                    return err(L, &format!("world.backdrop: {e}"));
                }
            };
            let mut layer = gfx3d::Layer::new(t).scroll(fnum(L, l, c"scroll", 0.5)).at(fnum(L, l, c"y", 0.5));
            if fflag(L, l, c"fill") {
                layer = layer.fill_below();
            }
            layers.push(layer);
            lua_pop(L, 1);
        }
    }
    gfx3d::backdrop(&layers);
    0
});

// splash(x, y, z, size): water thrown up and rings on the water, in the light of the sun.
b!(splash, |L, h| {
    let p = vec3(opt(L, 1, 0.0) as f32, opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    let size = opt(L, 4, 0.5).clamp(0.05, 5.0) as f32;
    let w = w!(L, h);
    if w.splash.is_none() {
        w.splash = Splash::new([0.85, 0.9, 1.0], 0.5);
    }
    if let Some(s) = &w.splash {
        s.splash(p, size, 2.0 + 2.0 * size);
    }
    0
});

// shockwave(x, y, z, radius, strength, secs)
b!(shockwave, |L, h| {
    let g = |i: c_int, d: f64| opt(L, i, d) as f32;
    gfx3d::shockwave(vec3(g(1, 0.0), g(2, 0.0), g(3, 0.0)), g(4, 3.0), g(5, 0.5), g(6, 0.5));
    0
});

// `scatter(node, { count, area, seed, scale = { min, max }, tint = { a, b }, turn })`:
// copies of the node laid out in Rust (the same for the same seed), drawn
// in one draw as its instances. `area` is a square's side (meters, around
// the node's origin) or `{ w, d }`; `on = terrain` comes with terrains (2c).
b!(scatter, |L, h| match scatter_(L, h) {
    Ok(n) => n,
    Err(m) => err(L, &format!("scatter: {m}")),
});

unsafe fn scatter_(L: *mut lua_State, h: &mut Host) -> Result<c_int, String> {
    unsafe {
        let Some(w) = h.three.as_deref_mut() else {
            return Err(super::three::NOT_3D.into());
        };
        let Some(id) = node_at(L, 1).filter(|n| w.scene.alive(*n)) else {
            return Err(format!("scatter(node, {{ count, area }}) takes a 3D node first, got {}", type_name(L, 1)));
        };
        if lua_type(L, 2) != LUA_TTABLE {
            return Err("scatter(node, { count = 200, area = 40, seed = 7, scale = { 0.6, 1.4 } })".into());
        }
        // `on = ground`: each copy stands on the Terrain's true ground.
        let on = match with_field(L, 2, c"on", || lua_type(L, -1)) {
            LUA_TNIL => false,
            _ => {
                lua_rawgetfield(L, 2, c"on".as_ptr());
                let ok = super::terrain::ground_at(L, w, lua_gettop(L)).is_some();
                lua_pop(L, 1);
                if !ok {
                    return Err("on is the round's Terrain (on = ground, where ground = Terrain{…})".into());
                }
                true
            }
        };
        let count = fnum(L, 2, c"count", 100.0);
        if !(0.0..=16384.0).contains(&count) {
            return Err(format!("count is 0 to 16384 copies, got {}", fmt_num(count as f64)));
        }
        let (aw, ad) = match with_field(L, 2, c"area", || lua_type(L, -1)) {
            LUA_TTABLE => {
                lua_rawgetfield(L, 2, c"area".as_ptr());
                let a = (fnum(L, -1, c"w", 20.0), fnum(L, -1, c"d", 20.0));
                lua_pop(L, 1);
                a
            }
            _ => {
                let s = fnum(L, 2, c"area", 20.0);
                (s, s)
            }
        };
        let [smin, smax] = fnums(L, 2, c"scale", [1.0, 1.0]);
        let tint = if with_field(L, 2, c"tint", || lua_type(L, -1) == LUA_TTABLE) {
            lua_rawgetfield(L, 2, c"tint".as_ptr());
            let t = abs(L, -1);
            let mut cs = [0xffffffffu32; 2];
            for (k, c) in cs.iter_mut().enumerate() {
                lua_rawgeti(L, t, k as c_int + 1);
                let r = col(L, -1);
                lua_pop(L, 1);
                match r {
                    Ok(x) => *c = x,
                    Err(m) => {
                        lua_pop(L, 1);
                        return Err(format!("tint: {m}"));
                    }
                }
            }
            lua_pop(L, 1);
            Some(cs)
        } else {
            None
        };
        let turn = !with_field(L, 2, c"turn", || lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1) == 0);
        let mut rng = maimbrain::Rng::new(fnum(L, 2, c"seed", 1.0) as u64);
        let mut list = Vec::with_capacity(count as usize);
        for _ in 0..count as usize {
            let (x, z) = ((rng.f32() - 0.5) * aw, (rng.f32() - 0.5) * ad);
            let s = smin + (smax - smin) * rng.f32();
            let yaw = if turn { rng.f32() * std::f32::consts::TAU } else { 0.0 };
            let mut i = Instance::at(vec3(x, 0.0, z)).with_rot(Quat::from_rotation_y(yaw)).with_scale(s);
            if let Some([a, b]) = tint {
                let t = rng.f32();
                let mix = |sh: u32| (((a >> sh) & 0xff) as f32 * (1.0 - t) + ((b >> sh) & 0xff) as f32 * t).round() as u32;
                i = i.with_color(mix(24) << 24 | mix(16) << 16 | mix(8) << 8 | 0xff);
            }
            list.push(i);
        }
        if on {
            // Copies are in the node's space: each lifted (along the node's
            // up) so it stands where the true ground is under it. The drawn
            // ground is within 0.13 m of it on levels 0 and 1 (p95, S11) but
            // up to half a meter (3 m on a crest) on level 2, so the copies
            // fade out before level 2 starts, unless the game set a fade.
            let m = w.world_of(id).unwrap_or(gfx3d::Mat4::IDENTITY);
            let up = m.transform_vector(Vec3::Y);
            if up.y.abs() < 1e-4 {
                return Err("on = ground needs the node upright (its copies stand along its y)".into());
            }
            let g = &w.ground.as_ref().unwrap().terrain;
            for i in &mut list {
                let p = m.transform_point(i.pos);
                i.pos.y += (g.height_at(p.x, p.z) - p.y) / up.y;
            }
            let (from, to) = crate::three::scatter_fade(g);
            if w.info_mut(id).fade.is_none() {
                if let Some(n) = w.scene.get(id).and_then(|n| n.host) {
                    n.set_fade(from, to);
                }
                w.info_mut(id).fade = Some((from, to));
            }
        }
        let ok = w.scene.get(id).and_then(|n| n.host).is_some_and(|n| n.set_instances(&list));
        w.info_mut(id).instances = list.len();
        if !ok {
            return Err("the host refused the copies (16384 a node, 65536 in all)".into());
        }
        Ok(0)
    }
}
