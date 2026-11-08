//! Games' shaders (docs/STUDIO-2-API.md [shaders]; docs/STUDIO-2.md §4.15):
//! `shaders/<name>.wgsl` in the project, checked by `mb build` with the
//! checks the phone runs again (crate `mb-shader`).
//!
//! - **3D**: `material{ shader = "dissolve", uniforms = u, textures = { … } }`.
//!   The engine keeps the `uniforms` table itself and reads it each frame,
//!   so `u.t = 0.5`, or `flux.to(u, 1, { t = 1 })`, changes the shader.
//! - **2D**: `love.graphics.newShader("glow")` (or WGSL with `fn sprite`),
//!   `love.graphics.setShader(s)` for the images drawn after it, and
//!   `s:send("name", value)` (LÖVE's, with WGSL in place of GLSL).
//!
//! Named uniforms are the shader's `struct Uniforms` (its layout comes from
//! packed.toml, which `mb build` writes from the checks); a shader without
//! one reads `mb_u[0..16]`, and its uniforms are a list of numbers or
//! `{ x, y, z, w }` tables. Only the shader builds (feature `shaders`)
//! call mb3d 5 and mb2d 4; the others have no shaders at all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CString, c_int, c_void};

use maimbrain::scene::Cmd;
use maimbrain::shaders::Params;

use super::*;
use crate::project::{Project, ShaderInfo};

thread_local! {
    /// The session's shader handles, by kind and source (made once a session).
    static HANDLES: RefCell<HashMap<(bool, String), u32>> = RefCell::new(HashMap::new());
}

/// A new session: the last one's handles were that session's.
pub fn new_session() {
    HANDLES.with(|h| h.borrow_mut().clear());
}

/// The project's shader by name: `"dissolve"`, `"shaders/dissolve"`, `"shaders/dissolve.wgsl"`.
pub fn find(p: &Project, name: &str) -> Option<usize> {
    let n = name.strip_prefix("shaders/").unwrap_or(name);
    let n = n.strip_suffix(".wgsl").unwrap_or(n);
    p.shaders.iter().position(|s| s.name == n)
}

/// Whether WGSL declares `struct Uniforms`: crate mb-shader's
/// `declares_uniforms`, the same rule (the engine doesn't link naga).
pub fn declares_uniforms(src: &str) -> bool {
    let words: Vec<&str> = src.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
    words.windows(2).any(|w| w[0] == "struct" && w[1] == "Uniforms")
}

/// The session's handle for a shader (made the first time).
fn handle_for(info: &ShaderInfo) -> Result<u32, String> {
    let key = (info.sprite, info.source.clone());
    if let Some(h) = HANDLES.with(|m| m.borrow().get(&key).copied()) {
        return Ok(h);
    }
    let h = make(info)?.map_err(|code| match code {
        -4 => format!("shaders/{}.wgsl: a game makes 64 shaders at most", info.name),
        c => format!("shaders/{}.wgsl couldn't be made (error {c}: empty, or over 64 KB)", info.name),
    })?;
    HANDLES.with(|m| m.borrow_mut().insert(key, h));
    Ok(h)
}

#[cfg(shaders)]
fn make(info: &ShaderInfo) -> Result<Result<u32, i32>, String> {
    Ok(if info.sprite { maimbrain::shaders::sprite_shader(&info.source) } else { maimbrain::shaders::material_shader(&info.source) })
}

#[cfg(not(shaders))]
fn make(_: &ShaderInfo) -> Result<Result<u32, i32>, String> {
    Err("this game was built without shaders: rebuild it (mb build packs a shader build when there's a shaders/*.wgsl)".into())
}

/// A 3D material with a shader whose uniforms follow a Lua table.
pub struct Shaded3 {
    pub material: maimbrain::gfx3d::Material,
    pub shader: u32,
    pub info: usize,
    /// The `uniforms` table, in the registry.
    pub table: Option<c_int>,
    pub params: Params,
}

/// A `love.graphics.newShader`.
#[derive(Clone)]
pub struct Shader2 {
    pub handle: u32,
    pub info: ShaderInfo,
    pub params: Params,
}

/// A round's shaders.
#[derive(Default)]
pub struct State {
    pub materials: Vec<Shaded3>,
    pub sprites: Vec<Shader2>,
    /// The `setShader` shader, this frame.
    pub current: Option<usize>,
}

unsafe fn number(L: *mut lua_State, i: c_int) -> Option<f32> {
    unsafe { (lua_type(L, i) == LUA_TNUMBER).then(|| lua_tonumber(L, i) as f32).filter(|v| v.is_finite()) }
}

/// Up to four numbers from the value at `i`: a number, `{ a, b, c, d }`,
/// `{ x = …, y = … }` or `{ r = …, g = … }`, or a colour (`0xRRGGBB`, as
/// linear light) where `color` (a vec3 or vec4 field).
unsafe fn value_at(L: *mut lua_State, i: c_int, comps: u32) -> Result<[f32; 4], String> {
    unsafe {
        let i = if i < 0 { lua_gettop(L) + i + 1 } else { i };
        let mut v = [0.0f32; 4];
        match lua_type(L, i) {
            LUA_TNUMBER => {
                let x = number(L, i).unwrap_or(0.0);
                if comps >= 3 && x > 1.0 && x == x.trunc() && x <= 16_777_215.0 {
                    // A colour: 0xRRGGBB in linear light.
                    let c = x as u32;
                    let ch = |s: u32| {
                        let u = ((c >> s) & 255) as f32 / 255.0;
                        if u <= 0.04045 { u / 12.92 } else { ((u + 0.055) / 1.055).powf(2.4) }
                    };
                    v = [ch(16), ch(8), ch(0), 1.0];
                } else {
                    v[0] = x;
                }
            }
            LUA_TTABLE => {
                for k in 0..4 {
                    lua_rawgeti(L, i, k + 1);
                    if let Some(x) = number(L, -1) {
                        v[k as usize] = x;
                    }
                    lua_pop(L, 1);
                }
                for (k, names) in [["x", "r"], ["y", "g"], ["z", "b"], ["w", "a"]].iter().enumerate() {
                    for n in names {
                        let c = CString::new(*n).unwrap();
                        lua_rawgetfield(L, i, c.as_ptr());
                        if let Some(x) = number(L, -1) {
                            v[k] = x;
                        }
                        lua_pop(L, 1);
                    }
                }
            }
            -1 | LUA_TNIL => {}
            _ => return Err(format!("a uniform is a number or a table of up to 4 (got {})", type_name(L, i))),
        }
        Ok(v)
    }
}

/// The uniforms table at `t` packed by the shader's layout: named fields
/// for a `struct Uniforms`, else `mb_u[0..16]` from a list.
unsafe fn pack(L: *mut lua_State, t: c_int, info: &ShaderInfo, out: &mut [f32; 64]) -> Result<(), String> {
    unsafe {
        let t = if t < 0 { lua_gettop(L) + t + 1 } else { t };
        if lua_type(L, t) != LUA_TTABLE {
            return Err("uniforms is a table".into());
        }
        match &info.uniforms {
            Some(fields) => {
                for (name, offset, comps) in fields {
                    let c = CString::new(name.as_str()).map_err(|_| "a uniform's name")?;
                    lua_rawgetfield(L, t, c.as_ptr());
                    let v = value_at(L, -1, *comps);
                    lua_pop(L, 1);
                    let v = v.map_err(|e| format!("uniforms.{name}: {e}"))?;
                    let at = (*offset / 4) as usize;
                    for k in 0..(*comps as usize).min(4) {
                        if at + k < 64 {
                            out[at + k] = v[k];
                        }
                    }
                }
            }
            None => {
                for k in 0..16 {
                    lua_rawgeti(L, t, k as c_int + 1);
                    let v = value_at(L, -1, 4);
                    lua_pop(L, 1);
                    let v = v.map_err(|e| format!("uniforms[{}]: {e}", k + 1))?;
                    out[k * 4..k * 4 + 4].copy_from_slice(&v);
                }
            }
        }
        Ok(())
    }
}

/// What keys the uniforms table may have, for a mistake's message.
fn names(info: &ShaderInfo) -> String {
    match &info.uniforms {
        Some(f) => f.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>().join(", "),
        None => "a list of up to 16 values (mb_u[0] is the first)".into(),
    }
}

/// `material{ shader, uniforms, textures }` (3D): gives `material` its
/// shader. The table is at stack index 1.
#[cfg(three)]
pub unsafe fn material(L: *mut lua_State, h: &mut Host, material: maimbrain::gfx3d::Material) -> Result<(), String> {
    unsafe {
        let Some(name) = super::three::with_field(L, 1, c"shader", || st(L, -1).map(str::to_string)) else { return Err("shader is a shader's name (shaders/<name>.wgsl)".into()) };
        let Some(i) = find(&h.project, &name) else {
            let have = h.project.shaders.iter().filter(|s| !s.sprite).map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
            return Err(format!("shader = \"{name}\": there's no shaders/{name}.wgsl{}", if have.is_empty() { String::new() } else { format!(" (there's {have})") }));
        };
        let info = h.project.shaders[i].clone();
        if info.sprite {
            return Err(format!("shaders/{}.wgsl is a sprite's shader (fn sprite): a material's has fn surface", info.name));
        }
        let shader = handle_for(&info)?;
        let mut params = Params::default();
        // textures = { "noise", "images/sand" }: mb_tex0–3.
        let n = super::three::with_field(L, 1, c"textures", || lua_type(L, -1));
        if n == LUA_TTABLE {
            lua_rawgetfield(L, 1, c"textures".as_ptr());
            for k in 0..4 {
                lua_rawgeti(L, -1, k + 1);
                let t = st(L, -1).map(str::to_string);
                lua_pop(L, 1);
                if let Some(t) = t {
                    match super::world::texture(h, &t) {
                        Ok(tex) => params.textures[k as usize] = tex.raw(),
                        Err(e) => {
                            lua_pop(L, 1);
                            return Err(format!("textures[{}]: {e}", k + 1));
                        }
                    }
                }
            }
            lua_pop(L, 1);
        } else if n != LUA_TNIL && n != -1 {
            return Err("textures is a list of images' names: { \"noise\" }".into());
        }
        let mut table = None;
        if super::three::with_field(L, 1, c"uniforms", || lua_type(L, -1)) == LUA_TTABLE {
            lua_rawgetfield(L, 1, c"uniforms".as_ptr());
            if let Some(fields) = &info.uniforms {
                // A key the shader doesn't have is a mistake: say which it has.
                lua_pushnil(L);
                while lua_next(L, -2) != 0 {
                    lua_pop(L, 1);
                    let k = st(L, -1).map(str::to_string);
                    if let Some(k) = k
                        && !fields.iter().any(|f| f.0 == k)
                    {
                        lua_pop(L, 2);
                        return Err(format!("uniforms.{k}: shaders/{}.wgsl's struct Uniforms has {}", info.name, names(&info)));
                    }
                }
            }
            let r = pack(L, -1, &info, &mut params.uniforms);
            if let Err(e) = r {
                lua_pop(L, 1);
                return Err(e);
            }
            table = Some(lua_ref(L, -1));
            lua_pop(L, 1);
        }
        #[cfg(shaders)]
        maimbrain::shaders::set_material_shader(material, shader, &params);
        h.shaders.materials.push(Shaded3 { material, shader, info: i, table, params });
        Ok(())
    }
}

/// Each frame, before the world goes to mb3d: every shader material's
/// uniforms table read again, and sent where it changed.
#[cfg(three)]
pub unsafe fn sync(L: *mut lua_State, h: &mut Host) {
    unsafe {
        let project = h.project.clone();
        for m in &mut h.shaders.materials {
            let Some(r) = m.table else { continue };
            lua_rawgeti(L, LUA_REGISTRYINDEX, r);
            let mut u = m.params.uniforms;
            let ok = pack(L, -1, &project.shaders[m.info], &mut u).is_ok();
            lua_pop(L, 1);
            if ok && u != m.params.uniforms {
                m.params.uniforms = u;
                #[cfg(shaders)]
                maimbrain::shaders::set_material_shader(m.material, m.shader, &m.params);
            }
        }
    }
}

// ------------------------------------------------------------------- 2D

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register_methods(L, ud, tag::SHADER, c"Shader", &[(c"send", send), (c"hasUniform", has_uniform), (c"typeOf", type_of)]);
    }
}

// love.graphics.newShader("glow") (shaders/glow.wgsl), or WGSL with `fn sprite`.
b!(newShader, |L, h| {
    let Some(arg) = st(L, 1).map(str::to_string) else { return bad(L, 1, "newShader", "a shader's name (shaders/<name>.wgsl) or WGSL") };
    let info = match find(&h.project, &arg) {
        Some(i) => h.project.shaders[i].clone(),
        None if arg.contains("fn sprite") => ShaderInfo { name: "(code)".into(), source: arg.clone(), sprite: true, uniforms: None, cost: 0 },
        None => return err(L, &format!("newShader(\"{arg}\"): there's no shaders/{arg}.wgsl (or pass WGSL with fn sprite; Studio's shaders are WGSL, not LÖVE's GLSL)")),
    };
    if !info.sprite {
        return err(L, &format!("newShader: shaders/{}.wgsl is a 3D material's shader (fn surface): a sprite's has fn sprite(in: Sprite) -> vec4<f32>", info.name));
    }
    if info.name == "(code)" && declares_uniforms(&info.source) {
        return err(L, "newShader: a shader with struct Uniforms goes in shaders/<name>.wgsl (mb build lays its uniforms out)");
    }
    let handle = match handle_for(&info) {
        Ok(x) => x,
        Err(e) => return err(L, &format!("newShader: {e}")),
    };
    if h.shaders.sprites.len() >= 64 {
        return err(L, "newShader: 64 shaders a round at most: make them once, in love.load");
    }
    h.shaders.sprites.push(Shader2 { handle, info, params: Params::default() });
    push_handle(L, tag::SHADER, h.shaders.sprites.len() as u32 - 1)
});

/// The command that puts shader `i` (or none) in use from here.
fn use_cmd(s: Option<&Shader2>) -> Cmd {
    match s {
        Some(s) => Cmd::Shader { shader: s.handle, params: Box::new(s.params.pack()) },
        None => Cmd::Shader { shader: 0, params: Box::new([0; Params::SIZE]) },
    }
}

// love.graphics.setShader(shader) or setShader() for none: the images drawn after it (shapes and text never).
b!(setShader, |L, h| {
    let i = match lua_type(L, 1) {
        -1 | LUA_TNIL => None,
        _ => match handle(L, 1, tag::SHADER) {
            Some(i) => Some(i as usize),
            None => return bad(L, 1, "setShader", "Shader"),
        },
    };
    h.shaders.current = i;
    let c = use_cmd(i.and_then(|i| h.shaders.sprites.get(i)));
    super::graphics::rec(L, h, c)
});

b!(getShader, |L, h| {
    match h.shaders.current {
        Some(i) => push_handle(L, tag::SHADER, i as u32),
        None => {
            lua_pushnil(L);
            1
        }
    }
});

// shader:send(name, value) — a `struct Uniforms` field, `"mb_u"` with a list, or an image as "tex1"–"tex3".
b!(send, |L, h| {
    let Some(i) = handle(L, 1, tag::SHADER) else { return bad(L, 1, "send", "Shader") };
    let Some(name) = st(L, 2).map(str::to_string) else { return bad(L, 2, "send", "a uniform's name") };
    let i = i as usize;
    let Some(s) = h.shaders.sprites.get(i).cloned() else { return 0 };
    let mut s = s;
    if let Some(k) = name.strip_prefix("tex").and_then(|k| k.parse::<usize>().ok()).filter(|k| (1..=3).contains(k)) {
        let Some(img) = handle(L, 3, tag::IMAGE) else { return bad(L, 3, "send", "Image") };
        s.params.textures[k] = h.scene.image(img as maimbrain::scene::ImageId).and_then(|im| im.image).map_or(0, |im| im.0);
    } else {
        match (&s.info.uniforms, name.as_str()) {
            (Some(fields), _) => {
                let Some((_, offset, comps)) = fields.iter().find(|f| f.0 == name).cloned() else {
                    return err(L, &format!("send(\"{name}\"): shaders/{}.wgsl's struct Uniforms has {}", s.info.name, names(&s.info)));
                };
                let v = match value_at(L, 3, comps) {
                    Ok(v) => v,
                    Err(e) => return err(L, &format!("send(\"{name}\"): {e}")),
                };
                let at = (offset / 4) as usize;
                for k in 0..(comps as usize).min(4) {
                    if at + k < 64 {
                        s.params.uniforms[at + k] = v[k];
                    }
                }
            }
            (None, "mb_u") => {
                if let Err(e) = pack(L, 3, &s.info, &mut s.params.uniforms) {
                    return err(L, &format!("send(\"mb_u\"): {e}"));
                }
            }
            (None, _) => return err(L, &format!("send(\"{name}\"): the shader has no struct Uniforms; send(\"mb_u\", {{ … }}) sets mb_u, or declare struct Uniforms {{ {name}: f32 }}")),
        }
    }
    h.shaders.sprites[i] = s;
    // The shader in use now: what's drawn after this send sees it.
    if h.shaders.current == Some(i) {
        let c = use_cmd(h.shaders.sprites.get(i));
        return super::graphics::rec(L, h, c);
    }
    0
});

b!(has_uniform, |L, h| {
    let Some(i) = handle(L, 1, tag::SHADER) else { return bad(L, 1, "hasUniform", "Shader") };
    let name = st(L, 2).unwrap_or("");
    let has = h.shaders.sprites.get(i as usize).is_some_and(|s| s.info.uniforms.as_ref().is_some_and(|f| f.iter().any(|f| f.0 == name)));
    lua_pushboolean(L, has as c_int);
    1
});

b!(type_of, |L, h| {
    let name = st(L, 2).unwrap_or("");
    lua_pushboolean(L, matches!(name, "Shader" | "Object") as c_int);
    1
});

/// A new frame: no shader in use (the colour and the transform start fresh too).
pub fn frame_start(s: &mut State) {
    s.current = None;
}
