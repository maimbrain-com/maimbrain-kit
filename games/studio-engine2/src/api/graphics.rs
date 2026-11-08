//! `love.graphics` (docs/STUDIO-2.md §5 [love]): LÖVE's immediate mode,
//! called in `love.draw` and recorded as the frame's draw list, which the
//! engine draws in `mb_render` between the nodes at order 0 or less and
//! those above (`maimbrain::scene::DrawList`). Lua never runs in
//! `mb_render`. Coordinates are the canvas's, as LÖVE's are the window's;
//! `camera:attach()` puts them in the world.
//!
//! Images are the project's (`newImage("hero")`, any size, maybe packed in
//! an atlas), quads are LÖVE's, fonts come from the library by family name
//! (the packer bakes what the Lua names), and `setDefaultFilter("nearest")`
//! keeps pixel art crisp (mb2d 3's nearest filter). Particle systems are
//! `Emitter`'s (node.rs).

use std::ffi::{c_int, c_void};

use maimbrain::gfx2d::Blend;
use maimbrain::scene::{Align, Cmd};

use super::*;
use crate::api::{Batch, TextObject};

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(
            L,
            ud,
            "love.graphics",
            &[
                (c"setColor", setColor),
                (c"getColor", getColor),
                (c"setBackgroundColor", setBackgroundColor),
                (c"getBackgroundColor", getBackgroundColor),
                (c"setLineWidth", setLineWidth),
                (c"getLineWidth", getLineWidth),
                (c"setBlendMode", setBlendMode),
                (c"rectangle", rectangle),
                (c"circle", circle),
                (c"line", line),
                (c"polygon", polygon),
                (c"print", print),
                (c"printf", printf),
                (c"push", push),
                (c"pop", pop),
                (c"translate", translate),
                (c"rotate", rotate),
                (c"scale", scale),
                (c"origin", origin),
                (c"getWidth", getWidth),
                (c"getHeight", getHeight),
                (c"getDimensions", getDimensions),
                (c"ellipse", ellipse),
                (c"arc", arc),
                (c"points", points_),
                (c"setPointSize", setPointSize),
                (c"newImage", newImage),
                (c"newQuad", newQuad),
                (c"draw", draw),
                (c"setDefaultFilter", setDefaultFilter),
                (c"getDefaultFilter", getDefaultFilter),
                (c"newFont", newFont),
                (c"setNewFont", setNewFont),
                (c"setFont", setFont),
                (c"getFont", getFont),
                (c"newText", newText),
                (c"newSpriteBatch", newSpriteBatch),
                (c"newCanvas", newCanvas),
                (c"newShader", super::shaders::newShader),
                (c"setShader", super::shaders::setShader),
                (c"getShader", super::shaders::getShader),
                (c"setCanvas", setCanvas),
                (c"getCanvas", getCanvas),
                (c"clear", clear),
            ],
        );
        register_methods(
            L,
            ud,
            tag::CANVAS,
            c"Canvas",
            &[
                (c"getWidth", canvas_w),
                (c"getHeight", canvas_h),
                (c"getDimensions", canvas_dims),
                (c"renderTo", canvas_renderTo),
                (c"setFilter", noop),
                (c"newImageData", canvas_imageData),
                (c"typeOf", type_of),
            ],
        );
        register_methods(L, ud, tag::IMAGE, c"Image", &[(c"getWidth", img_w), (c"getHeight", img_h), (c"getDimensions", img_dims), (c"setFilter", img_setFilter), (c"getFilter", img_getFilter), (c"typeOf", type_of)]);
        register_methods(L, ud, tag::QUAD, c"Quad", &[(c"getViewport", quad_viewport), (c"setViewport", quad_setViewport), (c"typeOf", type_of)]);
        register_methods(
            L,
            ud,
            tag::FONT,
            c"Font",
            &[(c"getWidth", font_w), (c"getHeight", font_h), (c"getLineHeight", font_line), (c"getWrap", font_wrap), (c"getAscent", font_ascent), (c"getDescent", font_descent), (c"getBaseline", font_ascent), (c"setFilter", noop), (c"typeOf", type_of)],
        );
        register_methods(L, ud, tag::TEXT, c"Text", &[(c"set", text_set), (c"setf", text_setf), (c"clear", text_clear), (c"getWidth", text_w), (c"getHeight", text_h), (c"getDimensions", text_dims), (c"typeOf", type_of)]);
        register_methods(
            L,
            ud,
            tag::BATCH,
            c"SpriteBatch",
            &[(c"add", batch_add), (c"set", batch_set), (c"clear", batch_clear), (c"getCount", batch_count), (c"setColor", batch_color), (c"getImage", batch_image), (c"typeOf", type_of)],
        );
    }
}

b!(noop, |L, h| 0);

// `obj:typeOf(name)`, as LÖVE's objects have it.
b!(type_of, |L, h| {
    let ty = type_name(L, 1);
    retb(L, st(L, 2).is_some_and(|n| n == ty || n == "Object" || (n == "Drawable" && matches!(ty, "Image" | "Text" | "SpriteBatch" | "ParticleSystem" | "Canvas")) || (n == "Texture" && matches!(ty, "Image" | "Canvas"))))
});

// ---------------------------------------------------------------- shapes

/// Points around an ellipse (LÖVE's segment count: about one per 2 units of edge, 8 to 128).
fn ellipse_points(x: f32, y: f32, rx: f32, ry: f32, a1: f32, a2: f32, segs: Option<f64>) -> Vec<f32> {
    let span = (a2 - a1).abs();
    let n = segs.map_or_else(|| ((rx.max(ry) * span / 2.0).ceil() as usize).clamp(8, 128), |s| (s as usize).clamp(3, 256));
    (0..=n)
        .flat_map(|i| {
            let a = a1 + (a2 - a1) * i as f32 / n as f32;
            [x + rx * a.cos(), y + ry * a.sin()]
        })
        .collect()
}

b!(ellipse, |L, h| {
    let line = match mode(L, "ellipse") {
        Ok(l) => l,
        Err(r) => return r,
    };
    let v = match numbers(L, "ellipse", 2, 5) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut p = ellipse_points(v[0], v[1], v[2], v[3], 0.0, std::f32::consts::TAU, num(L, 6));
    p.truncate(p.len() - 2);
    let c = if line { Cmd::Poly { points: p, line: h.line_width, color: h.color } } else { Cmd::Poly { points: p, line: 0.0, color: h.color } };
    rec(L, h, c)
});

b!(arc, |L, h| {
    let line = match mode(L, "arc") {
        Ok(l) => l,
        Err(r) => return r,
    };
    // arc(mode, [arctype,] x, y, radius, angle1, angle2, [segments])
    let (kind, at) = match st(L, 2) {
        Some(k @ ("pie" | "open" | "closed")) => (k, 3),
        Some(k) => return err(L, &format!("arc: arc type is \"pie\", \"open\" or \"closed\", got \"{k}\"")),
        None => ("pie", 2),
    };
    let v = match numbers(L, "arc", at, at + 4) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut p = ellipse_points(v[0], v[1], v[2], v[2], v[3], v[4], num(L, at + 5));
    if kind == "pie" {
        p.splice(0..0, [v[0], v[1]]);
    }
    let c = match (line, kind) {
        (true, "open") => Cmd::Line { points: p, width: h.line_width, color: h.color },
        (true, _) => Cmd::Poly { points: p, line: h.line_width, color: h.color },
        (false, _) => Cmd::Poly { points: p, line: 0.0, color: h.color },
    };
    rec(L, h, c)
});

b!(points_, |L, h| {
    let p = match points(L, "points", 1) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let s = h.point_size;
    for c in p.as_chunks::<2>().0 {
        if rec(L, h, Cmd::Rect { x: c[0] - s / 2.0, y: c[1] - s / 2.0, w: s, h: s, r: 0.0, line: 0.0, color: h.color }) < 0 {
            return -1;
        }
    }
    0
});

b!(setPointSize, |L, h| match num(L, 1) {
    Some(s) if s > 0.0 => {
        h.point_size = s as f32;
        0
    }
    _ => bad(L, 1, "setPointSize", "positive number"),
});

// ---------------------------------------------------------------- images

/// An image by name (`"hero"`), or LÖVE's path (`"images/hero.png"`). A
/// path from a LÖVE game's own folders (`"assets/ship.png"`, its pictures
/// moved to images/ when it came into Studio) finds images/ship.png.
fn image_named(h: &Host, name: &str) -> Result<u16, String> {
    let n = name.strip_prefix("images/").unwrap_or(name);
    let n = n.rsplit_once('.').filter(|(_, e)| matches!(*e, "png" | "jpg" | "jpeg")).map_or(n, |(a, _)| a);
    let file = n.rsplit_once('/').map_or(n, |(_, f)| f);
    h.project.image(n).or_else(|| h.project.image(file)).map(|i| i as u16).ok_or_else(|| format!("no image \"{n}\": add images/{file}.png"))
}

b!(newImage, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "newImage", "image name") };
    match image_named(h, name) {
        Ok(i) => {
            if h.default_nearest {
                h.nearest[i as usize] = true;
            }
            push_handle(L, tag::IMAGE, i as u32)
        }
        Err(m) => err(L, &m),
    }
});

unsafe fn image_size(L: *mut lua_State, h: &Host) -> Option<(f32, f32)> {
    unsafe { handle(L, 1, tag::IMAGE).and_then(|i| h.scene.image(i as u16)).map(|im| (im.w, im.h)) }
}
b!(img_w, |L, h| ret(L, image_size(L, h).map_or(0.0, |s| s.0) as f64));
b!(img_h, |L, h| ret(L, image_size(L, h).map_or(0.0, |s| s.1) as f64));
b!(img_dims, |L, h| {
    let (w, hh) = image_size(L, h).unwrap_or((0.0, 0.0));
    ret2(L, w as f64, hh as f64)
});

/// "nearest" or "linear" (LÖVE's filter modes).
unsafe fn filter(L: *mut lua_State, i: c_int, f: &str) -> Result<bool, c_int> {
    unsafe {
        match st(L, i) {
            Some("nearest") => Ok(true),
            Some("linear") => Ok(false),
            _ => Err(bad(L, i, f, "\"nearest\" or \"linear\"")),
        }
    }
}

b!(img_setFilter, |L, h| {
    let Some(i) = handle(L, 1, tag::IMAGE) else { return bad(L, 1, "setFilter", "Image") };
    // min and mag are one filter in mb2d: mag decides, as pixel art wants.
    let n = match filter(L, if lua_type(L, 3) == LUA_TSTRING { 3 } else { 2 }, "setFilter") {
        Ok(n) => n,
        Err(r) => return r,
    };
    h.nearest[i as usize] = n;
    0
});

b!(img_getFilter, |L, h| {
    let n = handle(L, 1, tag::IMAGE).is_some_and(|i| h.nearest[i as usize]);
    let s = if n { "nearest" } else { "linear" };
    push_str(L, s);
    push_str(L, s);
    2
});

b!(setDefaultFilter, |L, h| {
    let n = match filter(L, if lua_type(L, 2) == LUA_TSTRING { 2 } else { 1 }, "setDefaultFilter") {
        Ok(n) => n,
        Err(r) => return r,
    };
    // The project's images all exist from the start, so the default is
    // every image's (LÖVE's applies to images made after the call).
    h.default_nearest = n;
    for v in h.nearest.iter_mut() {
        *v = n;
    }
    0
});

b!(getDefaultFilter, |L, h| {
    let s = if h.default_nearest { "nearest" } else { "linear" };
    push_str(L, s);
    push_str(L, s);
    2
});

/// A quad: x, y, w, h in its image's pixels, and the reference size.
type Quad = [f32; 6];

b!(newQuad, |L, h| {
    let v = match numbers(L, "newQuad", 1, 4) {
        Ok(v) => v,
        Err(r) => return r,
    };
    // LÖVE's sw, sh: the image's size, or an Image in their place.
    let (sw, sh) = match handle(L, 5, tag::IMAGE).and_then(|i| h.scene.image(i as u16)) {
        Some(im) => (im.w, im.h),
        None => (opt(L, 5, v[2] as f64) as f32, opt(L, 6, v[3] as f64) as f32),
    };
    push_value::<Quad>(L, tag::QUAD, [v[0], v[1], v[2], v[3], sw, sh])
});

b!(quad_viewport, |L, h| {
    let Some(q) = value_at::<Quad>(L, 1, tag::QUAD) else { return bad(L, 1, "getViewport", "Quad") };
    for v in &q[..4] {
        lua_pushnumber(L, *v as f64);
    }
    4
});

b!(quad_setViewport, |L, h| {
    if value_at::<Quad>(L, 1, tag::QUAD).is_none() {
        return bad(L, 1, "setViewport", "Quad");
    }
    let v = match numbers(L, "setViewport", 2, 5) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let p = lua_touserdata(L, 1) as *mut Quad;
    let mut q = p.read_unaligned();
    q[..4].copy_from_slice(&v);
    p.write_unaligned(q);
    0
});

/// LÖVE's transform arguments from `at`: x, y, r, sx, sy, ox, oy.
unsafe fn place(L: *mut lua_State, at: c_int) -> [f32; 7] {
    unsafe {
        let sx = opt(L, at + 3, 1.0) as f32;
        [opt(L, at, 0.0) as f32, opt(L, at + 1, 0.0) as f32, opt(L, at + 2, 0.0) as f32, sx, opt(L, at + 4, sx as f64) as f32, opt(L, at + 5, 0.0) as f32, opt(L, at + 6, 0.0) as f32]
    }
}

fn image_cmd(image: u16, quad: Option<[f32; 4]>, p: [f32; 7], color: u32) -> Cmd {
    Cmd::Image { image, frame: 0, quad, x: p[0], y: p[1], r: p[2], sx: p[3], sy: p[4], ox: p[5], oy: p[6], color }
}

b!(draw, |L, h| {
    if let Some(i) = handle(L, 1, tag::IMAGE) {
        let (quad, at) = match value_at::<Quad>(L, 2, tag::QUAD) {
            Some(q) => (Some([q[0], q[1], q[2], q[3]]), 3),
            None => (None, 2),
        };
        let c = image_cmd(i as u16, quad, place(L, at), h.color);
        return rec(L, h, c);
    }
    if let Some(t) = handle(L, 1, tag::TEXT) {
        let Some(o) = h.texts.get(t as usize).cloned() else { return 0 };
        let p = place(L, 2);
        for c in [Cmd::Push, Cmd::Translate(p[0], p[1]), Cmd::Rotate(p[2]), Cmd::Scale(p[3], p[4]), Cmd::Translate(-p[5], -p[6])] {
            if rec(L, h, c) < 0 {
                return -1;
            }
        }
        let color = h.color;
        if rec(L, h, Cmd::Text { s: o.text, font: o.font, size: o.size, x: 0.0, y: 0.0, color, wrap: o.wrap, align: o.align }) < 0 {
            return -1;
        }
        return rec(L, h, Cmd::Pop);
    }
    if let Some(b) = handle(L, 1, tag::BATCH) {
        let Some(batch) = h.batches.get(b as usize).cloned() else { return 0 };
        let p = place(L, 2);
        for c in [Cmd::Push, Cmd::Translate(p[0], p[1]), Cmd::Rotate(p[2]), Cmd::Scale(p[3], p[4]), Cmd::Translate(-p[5], -p[6])] {
            if rec(L, h, c) < 0 {
                return -1;
            }
        }
        for (quad, s, color) in batch.sprites {
            if rec(L, h, image_cmd(batch.image, quad, s, mul_color(color, h.color))) < 0 {
                return -1;
            }
        }
        return rec(L, h, Cmd::Pop);
    }
    if handle(L, 1, tag::PARTICLES).is_some() {
        return crate::api::fx::draw_particles(L, h);
    }
    if let Some(c) = handle(L, 1, tag::CANVAS) {
        return draw_canvas(L, h, c as usize);
    }
    bad(L, 1, "draw", "Image, Text, SpriteBatch, ParticleSystem or Canvas")
});

/// Two colours multiplied channel by channel.
pub fn mul_color(a: u32, b: u32) -> u32 {
    let ch = |s: u32| (((a >> s) & 0xff) * ((b >> s) & 0xff) + 127) / 255;
    ch(24) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

// ---------------------------------------------------------------- fonts

/// A font userdata's value: the font and its size in units.
type FontValue = (maimbrain::gfx2d::Font, f32);

/// newFont(size), newFont(name), newFont(name, size).
unsafe fn font_args(L: *mut lua_State, h: &mut Host) -> Result<FontValue, String> {
    unsafe {
        let (font, size) = match (lua_type(L, 1), num(L, 1)) {
            (LUA_TNUMBER, Some(s)) => (h.body_font, s),
            (LUA_TSTRING, _) => (resolve_font(h, st(L, 1).unwrap_or(""))?, opt(L, 2, 16.0)),
            (LUA_TNIL, _) | (-1, _) => (h.body_font, 16.0),
            _ => return Err(format!("newFont takes a size, or a font name and a size, got {}", type_name(L, 1))),
        };
        if !(1.0..=512.0).contains(&size) {
            return Err(format!("a font's size is 1–512 units, got {}", fmt_num(size)));
        }
        Ok((font, size as f32))
    }
}

b!(newFont, |L, h| match font_args(L, h) {
    Ok(f) => push_value::<FontValue>(L, tag::FONT, f),
    Err(m) => err(L, &m),
});

b!(setNewFont, |L, h| match font_args(L, h) {
    Ok(f) => {
        (h.font, h.font_size) = f;
        push_value::<FontValue>(L, tag::FONT, f)
    }
    Err(m) => err(L, &m),
});

b!(setFont, |L, h| match value_at::<FontValue>(L, 1, tag::FONT) {
    Some(f) => {
        (h.font, h.font_size) = f;
        0
    }
    None => bad(L, 1, "setFont", "Font"),
});

b!(getFont, |L, h| push_value::<FontValue>(L, tag::FONT, (h.font, h.font_size)));

fn measure(f: FontValue, s: &str, wrap: f32) -> (f32, f32) {
    maimbrain::ui::text(s).font(f.0).size(f.1).wrap(wrap).measure()
}

unsafe fn font_self(L: *mut lua_State) -> Option<FontValue> {
    unsafe { value_at::<FontValue>(L, 1, tag::FONT) }
}

b!(font_w, |L, h| {
    let Some(f) = font_self(L) else { return bad(L, 1, "getWidth", "Font") };
    h.read_back();
    ret(L, measure(f, &to_s(L, 2), 0.0).0 as f64)
});
b!(font_h, |L, h| {
    let Some(f) = font_self(L) else { return bad(L, 1, "getHeight", "Font") };
    h.read_back();
    ret(L, (f.0.metrics().line * f.1) as f64)
});
b!(font_line, |L, h| ret(L, 1.0));
b!(font_ascent, |L, h| {
    let Some(f) = font_self(L) else { return bad(L, 1, "getAscent", "Font") };
    h.read_back();
    ret(L, (f.0.metrics().ascent * f.1) as f64)
});
b!(font_descent, |L, h| {
    let Some(f) = font_self(L) else { return bad(L, 1, "getDescent", "Font") };
    h.read_back();
    ret(L, -(f.0.metrics().descent * f.1) as f64)
});
b!(font_wrap, |L, h| {
    let Some(f) = font_self(L) else { return bad(L, 1, "getWrap", "Font") };
    h.read_back();
    let s = to_s(L, 2);
    let limit = opt(L, 3, 1e9) as f32;
    let t = maimbrain::ui::text(&s).font(f.0).size(f.1).wrap(limit);
    let lines = t.lines();
    lua_pushnumber(L, t.measure().0 as f64);
    lua_createtable(L, lines.len() as c_int, 0);
    for (i, l) in lines.iter().enumerate() {
        push_str(L, l);
        lua_rawseti(L, -2, i as c_int + 1);
    }
    2
});

// ---------------------------------------------------------------- text objects

b!(newText, |L, h| {
    let Some(f) = value_at::<FontValue>(L, 1, tag::FONT) else { return bad(L, 1, "newText", "Font") };
    let text = if lua_gettop(L) >= 2 && lua_type(L, 2) != LUA_TNIL { to_s(L, 2) } else { String::new() };
    if h.texts.len() >= 1024 {
        return err(L, "newText: more than 1024 text objects in a round: reuse them with text:set");
    }
    h.texts.push(TextObject { font: f.0, size: f.1, text, wrap: 0.0, align: Align::Left });
    push_handle(L, tag::TEXT, h.texts.len() as u32 - 1)
});

unsafe fn text_self(L: *mut lua_State, h: &mut Host) -> Option<&mut TextObject> {
    unsafe { handle(L, 1, tag::TEXT).and_then(|i| h.texts.get_mut(i as usize)) }
}

b!(text_set, |L, h| {
    let s = to_s(L, 2);
    match text_self(L, h) {
        Some(t) => {
            (t.text, t.wrap, t.align) = (s, 0.0, Align::Left);
            0
        }
        None => bad(L, 1, "set", "Text"),
    }
});
b!(text_setf, |L, h| {
    let s = to_s(L, 2);
    let wrap = opt(L, 3, 0.0) as f32;
    let align = match st(L, 4) {
        Some("center") => Align::Center,
        Some("right") => Align::Right,
        _ => Align::Left,
    };
    match text_self(L, h) {
        Some(t) => {
            (t.text, t.wrap, t.align) = (s, wrap, align);
            0
        }
        None => bad(L, 1, "setf", "Text"),
    }
});
b!(text_clear, |L, h| {
    if let Some(t) = text_self(L, h) {
        t.text.clear();
    }
    0
});
unsafe fn text_size(L: *mut lua_State, h: &mut Host) -> (f32, f32) {
    h.read_back();
    unsafe { text_self(L, h).map_or((0.0, 0.0), |t| measure((t.font, t.size), &t.text, t.wrap)) }
}
b!(text_w, |L, h| ret(L, text_size(L, h).0 as f64));
b!(text_h, |L, h| ret(L, text_size(L, h).1 as f64));
b!(text_dims, |L, h| {
    let (w, hh) = text_size(L, h);
    ret2(L, w as f64, hh as f64)
});

// ---------------------------------------------------------------- sprite batches

b!(newSpriteBatch, |L, h| {
    let Some(i) = handle(L, 1, tag::IMAGE) else { return bad(L, 1, "newSpriteBatch", "Image") };
    let max = opt(L, 2, 1000.0).clamp(1.0, 8192.0) as usize;
    if h.batches.len() >= 256 {
        return err(L, "newSpriteBatch: more than 256 batches in a round: reuse them with batch:clear");
    }
    h.batches.push(Batch { image: i as u16, max, sprites: Vec::new(), color: 0xffffffff });
    push_handle(L, tag::BATCH, h.batches.len() as u32 - 1)
});

/// add/set's arguments from `at`: an optional quad, then LÖVE's transform.
unsafe fn batch_sprite(L: *mut lua_State, at: c_int) -> (Option<[f32; 4]>, [f32; 7]) {
    unsafe {
        match value_at::<Quad>(L, at, tag::QUAD) {
            Some(q) => (Some([q[0], q[1], q[2], q[3]]), place(L, at + 1)),
            None => (None, place(L, at)),
        }
    }
}

b!(batch_add, |L, h| {
    let Some(b) = handle(L, 1, tag::BATCH) else { return bad(L, 1, "add", "SpriteBatch") };
    let (q, p) = batch_sprite(L, 2);
    let batch = &mut h.batches[b as usize];
    if batch.sprites.len() >= batch.max {
        return err(L, &format!("SpriteBatch:add: the batch holds {} (newSpriteBatch's size)", batch.max));
    }
    batch.sprites.push((q, p, batch.color));
    ret(L, batch.sprites.len() as f64)
});
b!(batch_set, |L, h| {
    let Some(b) = handle(L, 1, tag::BATCH) else { return bad(L, 1, "set", "SpriteBatch") };
    let id = opt(L, 2, 0.0) as usize;
    let (q, p) = batch_sprite(L, 3);
    let batch = &mut h.batches[b as usize];
    match batch.sprites.get_mut(id.wrapping_sub(1)) {
        Some(s) => {
            *s = (q, p, batch.color);
            0
        }
        None => err(L, &format!("SpriteBatch:set: no sprite {id} (ids are what add returned)")),
    }
});
b!(batch_clear, |L, h| {
    if let Some(b) = handle(L, 1, tag::BATCH) {
        h.batches[b as usize].sprites.clear();
    }
    0
});
b!(batch_count, |L, h| ret(L, handle(L, 1, tag::BATCH).map_or(0, |b| h.batches[b as usize].sprites.len()) as f64));
b!(batch_color, |L, h| {
    let Some(b) = handle(L, 1, tag::BATCH) else { return bad(L, 1, "setColor", "SpriteBatch") };
    let c = if lua_gettop(L) < 2 { Ok(0xffffffff) } else { color_at(L, 2, true) };
    match c {
        Ok(c) => {
            h.batches[b as usize].color = c;
            0
        }
        Err(m) => err(L, &format!("setColor: {m}")),
    }
});
b!(batch_image, |L, h| match handle(L, 1, tag::BATCH) {
    Some(b) => push_handle(L, tag::IMAGE, h.batches[b as usize].image as u32),
    None => bad(L, 1, "getImage", "SpriteBatch"),
});

/// Records a command, or raises past the frame's cap.
pub unsafe fn rec(L: *mut lua_State, h: &mut Host, c: Cmd) -> c_int {
    if let Some(t) = h.target {
        return unsafe { rec_canvas(L, h, t, c) };
    }
    if h.draw.push(c) {
        0
    } else {
        unsafe { err(L, &format!("love.draw drew more than {} things this frame: use nodes for what stays (Sprite, Shape, Text)", maimbrain::scene::draw::MAX_CMDS)) }
    }
}

// ---------------------------------------------------------------- canvases

/// A `love.graphics.newCanvas`: what's been drawn into it, kept from frame to
/// frame until `clear`. mb2d has no render targets, so a canvas is its
/// commands, drawn again wherever it's drawn (docs/STUDIO-2-API.md [love]):
/// the same picture for what LÖVE games draw into one (trails, a level
/// drawn once), without its pixels (no read-back, no clipping to its size).
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    pub w: f32,
    pub h: f32,
    pub cmds: Vec<Cmd>,
}

/// Canvases a round may make.
pub const MAX_CANVASES: usize = 16;

/// A command into canvas `t`. A line that carries on from the last one (the
/// same width and colour, starting where it ended) extends it, so a trail
/// drawn a segment a frame stays one command.
unsafe fn rec_canvas(L: *mut lua_State, h: &mut Host, t: usize, c: Cmd) -> c_int {
    let canvas = &mut h.canvases[t];
    if let (Cmd::Line { points, width, color }, Some(Cmd::Line { points: last, width: lw, color: lc })) = (&c, canvas.cmds.last_mut())
        && width == lw
        && color == lc
        && points.len() >= 4
        && last.len() >= 2
        && last[last.len() - 2..] == points[..2]
    {
        last.extend_from_slice(&points[2..]);
        return 0;
    }
    if canvas.cmds.len() >= maimbrain::scene::draw::MAX_CMDS {
        return unsafe { err(L, &format!("a canvas holds at most {} things: love.graphics.clear() it now and then, or draw what stays as nodes", maimbrain::scene::draw::MAX_CMDS)) };
    }
    canvas.cmds.push(c);
    0
}

/// A command drawn with `color` multiplied in, as LÖVE tints a canvas it draws.
fn tinted(c: &Cmd, tint: u32) -> Cmd {
    if tint == 0xffffffff {
        return c.clone();
    }
    let m = |color: &u32| mul_color(*color, tint);
    match c {
        Cmd::Rect { x, y, w, h, r, line, color } => Cmd::Rect { x: *x, y: *y, w: *w, h: *h, r: *r, line: *line, color: m(color) },
        Cmd::Circle { x, y, r, line, color } => Cmd::Circle { x: *x, y: *y, r: *r, line: *line, color: m(color) },
        Cmd::Line { points, width, color } => Cmd::Line { points: points.clone(), width: *width, color: m(color) },
        Cmd::Poly { points, line, color } => Cmd::Poly { points: points.clone(), line: *line, color: m(color) },
        Cmd::Text { s, font, size, x, y, color, wrap, align } => Cmd::Text { s: s.clone(), font: *font, size: *size, x: *x, y: *y, color: m(color), wrap: *wrap, align: *align },
        Cmd::Image { image, frame, quad, x, y, r, sx, sy, ox, oy, color } => {
            Cmd::Image { image: *image, frame: *frame, quad: *quad, x: *x, y: *y, r: *r, sx: *sx, sy: *sy, ox: *ox, oy: *oy, color: m(color) }
        }
        other => other.clone(),
    }
}

b!(newCanvas, |L, h| {
    if h.canvases.len() >= MAX_CANVASES {
        return err(L, &format!("newCanvas: a round has at most {MAX_CANVASES} canvases: make them once, in love.load, and clear them to reuse"));
    }
    // newCanvas([w, h,] [settings]): settings (msaa, format, …) don't apply to a list of commands.
    let (w, hh) = (opt(L, 1, h.scene.width as f64) as f32, opt(L, 2, h.scene.height as f64) as f32);
    h.canvases.push(Canvas { w, h: hh, cmds: Vec::new() });
    push_handle(L, tag::CANVAS, h.canvases.len() as u32 - 1)
});

// setCanvas(canvas), setCanvas({ canvas, … }), or setCanvas() for the screen.
b!(setCanvas, |L, h| {
    if lua_type(L, 1) == LUA_TNIL || lua_gettop(L) == 0 {
        h.target = None;
        return 0;
    }
    if lua_type(L, 1) == LUA_TTABLE {
        lua_rawgeti(L, 1, 1);
        lua_replace(L, 1);
    }
    match handle(L, 1, tag::CANVAS) {
        Some(c) => {
            h.target = Some(c as usize);
            0
        }
        None => bad(L, 1, "setCanvas", "Canvas, or nothing for the screen"),
    }
});

b!(getCanvas, |L, h| match h.target {
    Some(c) => push_handle(L, tag::CANVAS, c as u32),
    None => {
        lua_pushnil(L);
        1
    }
});

// clear([r, g, b, a]): the canvas being drawn to (or this frame's drawing
// so far), then that colour over it if it isn't clear.
b!(clear, |L, h| {
    let color = if lua_gettop(L) == 0 || lua_type(L, 1) == LUA_TNIL {
        None
    } else {
        match color_at(L, 1, true) {
            Ok(c) => Some(c),
            Err(m) => return err(L, &format!("clear: {m}")),
        }
    };
    let (w, hh) = match h.target {
        Some(t) => {
            h.canvases[t].cmds.clear();
            (h.canvases[t].w, h.canvases[t].h)
        }
        None => {
            h.draw.clear();
            (h.scene.width, h.scene.height)
        }
    };
    match color.filter(|c| c & 0xff != 0) {
        Some(c) => rec(L, h, Cmd::Rect { x: 0.0, y: 0.0, w, h: hh, r: 0.0, line: 0.0, color: c }),
        None => 0,
    }
});

/// love.graphics.draw(canvas, x, y, r, sx, sy, ox, oy): its commands again, placed and tinted.
unsafe fn draw_canvas(L: *mut lua_State, h: &mut Host, c: usize) -> c_int {
    unsafe {
        if h.target == Some(c) {
            return err(L, "love.graphics.draw: a canvas can't be drawn into itself (setCanvas() first)");
        }
        let p = place(L, 2);
        for t in [Cmd::Push, Cmd::Translate(p[0], p[1]), Cmd::Rotate(p[2]), Cmd::Scale(p[3], p[4]), Cmd::Translate(-p[5], -p[6])] {
            if rec(L, h, t) < 0 {
                return -1;
            }
        }
        let tint = h.color;
        let cmds = std::mem::take(&mut h.canvases[c].cmds);
        let mut r = 0;
        for cmd in &cmds {
            if rec(L, h, tinted(cmd, tint)) < 0 {
                r = -1;
                break;
            }
        }
        h.canvases[c].cmds = cmds;
        if r < 0 {
            return r;
        }
        rec(L, h, Cmd::Pop)
    }
}

unsafe fn canvas_size(L: *mut lua_State, h: &Host) -> (f32, f32) {
    unsafe { handle(L, 1, tag::CANVAS).and_then(|c| h.canvases.get(c as usize)).map_or((0.0, 0.0), |c| (c.w, c.h)) }
}
b!(canvas_w, |L, h| ret(L, canvas_size(L, h).0 as f64));
b!(canvas_h, |L, h| ret(L, canvas_size(L, h).1 as f64));
b!(canvas_dims, |L, h| {
    let (w, hh) = canvas_size(L, h);
    ret2(L, w as f64, hh as f64)
});

// canvas:renderTo(fn): fn draws into the canvas.
b!(canvas_renderTo, |L, h| {
    let Some(c) = handle(L, 1, tag::CANVAS) else { return bad(L, 1, "renderTo", "Canvas") };
    if lua_type(L, 2) != LUA_TFUNCTION {
        return bad(L, 2, "renderTo", "function");
    }
    let before = h.target.replace(c as usize);
    lua_pushvalue(L, 2);
    let status = lua_pcall(L, 0, 0, 0);
    h.target = before;
    if status != LUA_OK {
        return -1; // the message is on the stack
    }
    0
});

b!(canvas_imageData, |L, h| err(L, "newImageData: there's no pixel access in the sandbox (a canvas is what was drawn into it, not pixels)"));

fn channels(c: u32) -> [f64; 4] {
    [24, 16, 8, 0].map(|s| ((c >> s) & 0xff) as f64 / 255.0)
}

b!(setColor, |L, h| match color_at(L, 1, true) {
    Ok(c) => {
        h.color = c;
        0
    }
    Err(m) => err(L, &format!("setColor: {m}")),
});

b!(getColor, |L, h| {
    for v in channels(h.color) {
        lua_pushnumber(L, v);
    }
    4
});

b!(setBackgroundColor, |L, h| match color_at(L, 1, true) {
    Ok(c) => {
        h.background = c;
        0
    }
    Err(m) => err(L, &format!("setBackgroundColor: {m}")),
});

b!(getBackgroundColor, |L, h| {
    for v in channels(h.background) {
        lua_pushnumber(L, v);
    }
    4
});

b!(setLineWidth, |L, h| match num(L, 1) {
    Some(w) if w > 0.0 => {
        h.line_width = w as f32;
        0
    }
    _ => bad(L, 1, "setLineWidth", "positive number"),
});

b!(getLineWidth, |L, h| ret(L, h.line_width as f64));

b!(setBlendMode, |L, h| {
    let mode = match st(L, 1) {
        Some("alpha") => Blend::Alpha,
        Some("add") => Blend::Add,
        Some("multiply") => Blend::Multiply,
        Some("screen") => Blend::Screen,
        Some(m @ ("subtract" | "lighten" | "darken" | "replace")) => return err(L, &format!("setBlendMode: \"{m}\" isn't in Studio (alpha, add, multiply, screen)")),
        _ => return bad(L, 1, "setBlendMode", "\"alpha\", \"add\", \"multiply\" or \"screen\""),
    };
    rec(L, h, Cmd::Blend(mode))
});

/// "fill" or "line" (LÖVE's draw modes): whether it's an outline.
unsafe fn mode(L: *mut lua_State, f: &str) -> Result<bool, c_int> {
    unsafe {
        match st(L, 1) {
            Some("fill") => Ok(false),
            Some("line") => Ok(true),
            Some(m) => Err(err(L, &format!("bad argument #1 to '{f}' (invalid draw mode \"{m}\", expected \"fill\" or \"line\")"))),
            None => Err(bad(L, 1, f, "draw mode")),
        }
    }
}

/// Numbers from stack index `from` to `to`, all of them numbers.
unsafe fn numbers(L: *mut lua_State, f: &str, from: c_int, to: c_int) -> Result<Vec<f32>, c_int> {
    unsafe {
        let mut v = Vec::with_capacity((to - from + 1).max(0) as usize);
        for i in from..=to {
            match num(L, i) {
                Some(x) => v.push(x as f32),
                None => return Err(bad(L, i, f, "number")),
            }
        }
        Ok(v)
    }
}

/// Points as LÖVE takes them: x1, y1, x2, y2, … from `from`, or one table.
unsafe fn points(L: *mut lua_State, f: &str, from: c_int) -> Result<Vec<f32>, c_int> {
    unsafe {
        if lua_type(L, from) == LUA_TTABLE {
            let n = lua_objlen(L, from);
            let mut v = Vec::with_capacity(n as usize);
            for i in 1..=n {
                lua_rawgeti(L, from, i);
                let x = num(L, -1);
                lua_pop(L, 1);
                match x {
                    Some(x) => v.push(x as f32),
                    None => return Err(err(L, &format!("{f}: point {i} isn't a number"))),
                }
            }
            return Ok(v);
        }
        numbers(L, f, from, lua_gettop(L))
    }
}

b!(rectangle, |L, h| {
    let line = match mode(L, "rectangle") {
        Ok(l) => l,
        Err(r) => return r,
    };
    let v = match numbers(L, "rectangle", 2, 5) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let r = opt(L, 6, 0.0) as f32;
    let c = Cmd::Rect { x: v[0], y: v[1], w: v[2], h: v[3], r, line: if line { h.line_width } else { 0.0 }, color: h.color };
    rec(L, h, c)
});

b!(circle, |L, h| {
    let line = match mode(L, "circle") {
        Ok(l) => l,
        Err(r) => return r,
    };
    let v = match numbers(L, "circle", 2, 4) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let c = Cmd::Circle { x: v[0], y: v[1], r: v[2], line: if line { h.line_width } else { 0.0 }, color: h.color };
    rec(L, h, c)
});

b!(line, |L, h| {
    let p = match points(L, "line", 1) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if p.len() < 4 || p.len() % 2 != 0 {
        return err(L, "line takes two points or more: x1, y1, x2, y2, …");
    }
    let c = Cmd::Line { points: p, width: h.line_width, color: h.color };
    rec(L, h, c)
});

b!(polygon, |L, h| {
    let line = match mode(L, "polygon") {
        Ok(l) => l,
        Err(r) => return r,
    };
    let p = match points(L, "polygon", 2) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if p.len() < 6 || p.len() % 2 != 0 {
        return err(L, "polygon takes three points or more: x1, y1, x2, y2, x3, y3, …");
    }
    let c = Cmd::Poly { points: p, line: if line { h.line_width } else { 0.0 }, color: h.color };
    rec(L, h, c)
});

/// Text, turned and scaled about x, y as LÖVE's `print` does.
#[allow(clippy::too_many_arguments)]
unsafe fn text(L: *mut lua_State, h: &mut Host, s: String, x: f32, y: f32, wrap: f32, align: Align, from: c_int) -> c_int {
    unsafe {
        let (r, sx) = (opt(L, from, 0.0) as f32, opt(L, from + 1, 1.0) as f32);
        let sy = opt(L, from + 2, sx as f64) as f32;
        let turned = r != 0.0 || sx != 1.0 || sy != 1.0;
        let (font, size, color) = (h.font, h.font_size, h.color);
        if turned {
            for c in [Cmd::Push, Cmd::Translate(x, y), Cmd::Rotate(r), Cmd::Scale(sx, sy)] {
                if rec(L, h, c) < 0 {
                    return -1;
                }
            }
        }
        let (tx, ty) = if turned { (0.0, 0.0) } else { (x, y) };
        if rec(L, h, Cmd::Text { s, font, size, x: tx, y: ty, color, wrap, align }) < 0 {
            return -1;
        }
        if turned { rec(L, h, Cmd::Pop) } else { 0 }
    }
}

b!(print, |L, h| {
    if lua_type(L, 1) == LUA_TNIL || lua_gettop(L) == 0 {
        return bad(L, 1, "print", "string");
    }
    let s = to_s(L, 1);
    let (x, y) = (opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    text(L, h, s, x, y, 0.0, Align::Left, 4)
});

b!(printf, |L, h| {
    if lua_type(L, 1) == LUA_TNIL || lua_gettop(L) == 0 {
        return bad(L, 1, "printf", "string");
    }
    let s = to_s(L, 1);
    let (x, y) = (opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
    let Some(limit) = num(L, 4) else { return bad(L, 4, "printf", "number (the wrap limit)") };
    let align = match st(L, 5) {
        None | Some("left") => Align::Left,
        Some("center") => Align::Center,
        Some("right") => Align::Right,
        Some("justify") => Align::Left,
        Some(a) => return err(L, &format!("printf: align is \"left\", \"center\", \"right\" or \"justify\", got \"{a}\"")),
    };
    text(L, h, s, x, y, limit.max(1.0) as f32, align, 6)
});

b!(push, |L, h| rec(L, h, Cmd::Push));
b!(pop, |L, h| rec(L, h, Cmd::Pop));
b!(origin, |L, h| rec(L, h, Cmd::Origin));
b!(translate, |L, h| {
    let c = Cmd::Translate(opt(L, 1, 0.0) as f32, opt(L, 2, 0.0) as f32);
    rec(L, h, c)
});
b!(rotate, |L, h| {
    let c = Cmd::Rotate(opt(L, 1, 0.0) as f32);
    rec(L, h, c)
});
b!(scale, |L, h| {
    let sx = opt(L, 1, 1.0);
    let c = Cmd::Scale(sx as f32, opt(L, 2, sx) as f32);
    rec(L, h, c)
});

b!(getWidth, |L, h| ret(L, h.scene.width as f64));
b!(getHeight, |L, h| ret(L, h.scene.height as f64));
b!(getDimensions, |L, h| ret2(L, h.scene.width as f64, h.scene.height as f64));
