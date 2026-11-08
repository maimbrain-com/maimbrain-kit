//! Nodes and the camera (docs/STUDIO-2.md §4.3, §5 [scene], [2d],
//! [camera]): each node is a tagged userdata holding its id, with C
//! `__index`, `__newindex` and `__namecall` (Luau's fast path for a
//! userdata's metamethods). A read gets the scene's current value; a write
//! checks the value and raises a teaching error from the line that wrote it,
//! then changes the scene. Unknown names are errors, not new fields: a
//! game's own data goes in `node.data`.

use std::ffi::{CStr, c_int, c_void};

use maimbrain::scene::{Align, Cmd, Kind, Node, NodeId, Preset, ShapeKind, TileMap, emitter::PRESETS};

use super::atoms::{A, NAMES};
use super::*;

/// The constructors: `Sprite{ image = "hero", x = 40 }`.
pub(crate) const KINDS: &[(&CStr, Kind)] = &[
    (c"Sprite", Kind::Sprite),
    (c"Shape", Kind::Shape),
    (c"Text", Kind::Text),
    (c"Layer", Kind::Layer),
    (c"Group", Kind::Group),
    (c"Tilemap", Kind::Tilemap),
    (c"Walker", Kind::Walker),
    (c"Emitter", Kind::Emitter),
];

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register_kind(L, ud, tag::NODE, c"Node", &[(c"__index", n_index), (c"__newindex", n_newindex), (c"__namecall", n_namecall), (c"__tostring", n_tostring)]);
        register_kind(L, ud, tag::CAMERA, c"Camera", &[(c"__index", c_index), (c"__newindex", c_newindex), (c"__namecall", c_namecall), (c"__call", c_call)]);
        let make: [RustFn; 8] = [mk_sprite, mk_shape, mk_text, mk_layer, mk_group, mk_tilemap, mk_walker, mk_emitter];
        for ((name, _), f) in KINDS.iter().zip(make) {
            sv_push_rfn(L, f, ud, name.as_ptr());
            lua_setfield(L, LUA_GLOBALSINDEX, name.as_ptr());
        }
        register(L, ud, "scene", &[(c"find", s_find), (c"all", s_all)]);
        // The one camera.
        let p = lua_newuserdatataggedwithmetatable(L, 4, tag::CAMERA) as *mut u32;
        *p = 0;
        lua_setfield(L, LUA_GLOBALSINDEX, c"camera".as_ptr());
    }
}

/// What each kind has, besides what every node has.
pub(crate) fn has(kind: Kind, a: A) -> bool {
    use A::*;
    match a {
        X | Y | Rotation | Scale | Sx | Sy | Vx | Vy | Gravity | Vr | Alpha | Visible | Color | Order | Fixed | Parent | Children | Name | Tag | Data | Alive => true,
        Behaviour | Behaviours | Knobs => !matches!(kind, Kind::Tilemap | Kind::Widget),
        Destroy | Find | WorldPosition => true,
        Body => cfg!(phys2),
        Play | AnimationFinished => kind == Kind::Sprite,
        W | H => matches!(kind, Kind::Sprite | Kind::Shape | Kind::Text | Kind::Tilemap | Kind::Walker | Kind::Widget),
        Radius | Shape | Points | Corner | Line => kind == Kind::Shape,
        Image => matches!(kind, Kind::Sprite | Kind::Layer | Kind::Walker),
        FlipX => matches!(kind, Kind::Sprite | Kind::Walker),
        Frame | AnchorX | AnchorY | FlipY => kind == Kind::Sprite,
        Size => matches!(kind, Kind::Text | Kind::Emitter),
        Text => matches!(kind, Kind::Text | Kind::Widget),
        Font | Align | Width => kind == Kind::Text,
        Value | Icon | Enabled | Tap | Tone | ButtonKind | Focus => kind == Kind::Widget,
        Parallax | RepeatX | RepeatY | Fill => kind == Kind::Layer,
        Map | Tiles | SolidAt | TileAt | Marker | Path => kind == Kind::Tilemap,
        Height | Stepped | Footsteps => kind == Kind::Walker,
        Hum => true,
        Speed => matches!(kind, Kind::Walker | Kind::Emitter),
        Preset | Rate | Colors | Life | Spread | Additive | Emitting | Emit => kind == Kind::Emitter,
        _ => false,
    }
}

pub(crate) fn is_method(a: A) -> bool {
    matches!(a, A::Destroy | A::Find | A::WorldPosition | A::Play | A::Emit | A::SolidAt | A::TileAt | A::Marker | A::Path)
}

/// A node's signals (Godot's idea, §5 [scene]): read as `node.name`.
pub(crate) fn is_signal(a: A) -> bool {
    matches!(a, A::AnimationFinished | A::Stepped)
}

/// `node.animationFinished` and the rest: the prelude's Signal object for
/// this node and name (made once, then the same one).
pub(crate) unsafe fn push_signal(L: *mut lua_State, h: &Host, a: A) -> c_int {
    unsafe {
        if h.signal_ref == 0 {
            return err(L, "signals aren't ready yet");
        }
        lua_rawgeti(L, LUA_REGISTRYINDEX, h.signal_ref);
        lua_pushvalue(L, 1);
        push_str(L, a.name());
        if lua_pcall(L, 2, 1, 0) != LUA_OK {
            return -2;
        }
        1
    }
}

/// Levenshtein distance, for "did you mean".
pub(crate) fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + (ca != *cb) as usize).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// "no property `colour` on Sprite; did you mean `color`?"
pub(crate) fn unknown(kind: Kind, key: &str, method: bool) -> String {
    let what = if method { "method" } else { "property" };
    let mut best: Option<(usize, &str)> = None;
    for (i, n) in NAMES.iter().enumerate() {
        let Some(a) = A::from(i as i32) else { continue };
        if !has(kind, a) || is_method(a) != method {
            continue;
        }
        let d = if n.eq_ignore_ascii_case(key) { 0 } else { distance(&key.to_ascii_lowercase(), &n.to_ascii_lowercase()) };
        // One letter off, or two in a long name ("score" isn't "scale").
        if (d <= 1 || (d == 2 && key.len() >= 8)) && best.is_none_or(|b| d < b.0) {
            best = Some((d, n));
        }
    }
    // Names a node of another kind has.
    let other = NAMES.iter().position(|n| *n == key).and_then(|i| A::from(i as i32)).filter(|a| !has(kind, *a));
    // Engine 1.0's things' own names (upgrade_engine leaves them to the game).
    match key {
        "anim" if !method => return format!("no property `anim` on {}: engine 1.0's `thing.anim = \"run\"` is `node:play(\"run\")` in 2.0", kind.name()),
        "kind" if !method => return format!("no property `kind` on {}: engine 1.0's `thing.kind` is the node's `tag` in 2.0", kind.name()),
        _ => {}
    }
    match (best, other) {
        (_, Some(_)) => format!("{} has no {what} `{key}` (another kind of node has it)", kind.name()),
        (Some((_, n)), _) => format!("no {what} `{key}` on {}; did you mean `{n}`?", kind.name()),
        (None, _) if method => format!("no method `{key}` on {}", kind.name()),
        (None, _) => format!("no property `{key}` on {}: a game's own values go in `node.data` (node.data.{key} = …)", kind.name()),
    }
}

/// The node at stack index `i`, alive or not.
pub unsafe fn node_at(L: *mut lua_State, i: c_int) -> Option<NodeId> {
    unsafe {
        if lua_type(L, i) != LUA_TUSERDATA || lua_userdatatag(L, i) != tag::NODE {
            return None;
        }
        Some(NodeId::from_bits(*(lua_touserdata(L, i) as *const u32)))
    }
}

/// Pushes the node's handle: the same userdata every time.
pub unsafe fn push_node(L: *mut lua_State, h: &Host, id: NodeId) {
    unsafe {
        lua_rawgeti(L, LUA_REGISTRYINDEX, h.handles_ref);
        lua_rawgeti(L, -1, id.to_bits() as c_int);
        if lua_type(L, -1) == LUA_TUSERDATA {
            lua_remove(L, -2);
            return;
        }
        lua_pop(L, 1);
        let p = lua_newuserdatataggedwithmetatable(L, 4, tag::NODE) as *mut u32;
        *p = id.to_bits();
        lua_pushvalue(L, -1);
        lua_rawseti(L, -3, id.to_bits() as c_int);
        lua_remove(L, -2);
    }
}

/// A live node, or a destroyed one as it was last (reads only).
fn read_node(h: &Host, id: NodeId) -> Option<&Node> {
    h.scene.get(id).or_else(|| h.graveyard.iter().rev().find(|(g, _)| *g == id).map(|(_, n)| n))
}

/// Pushes a property's value.
unsafe fn get(L: *mut lua_State, h: &Host, id: NodeId, n: &Node, a: A) -> c_int {
    unsafe {
        let alive = h.scene.alive(id);
        if n.kind == Kind::Widget
            && let Some(r) = super::ui::widget_get(L, h, id, n, a)
        {
            return r;
        }
        match a {
            A::X => ret(L, n.x as f64),
            A::Y => ret(L, n.y as f64),
            A::Rotation => ret(L, n.rotation as f64),
            A::Scale | A::Sx => ret(L, n.sx as f64),
            A::Sy => ret(L, n.sy as f64),
            A::Vx => ret(L, n.vx as f64),
            A::Vy => ret(L, n.vy as f64),
            A::Vr => ret(L, n.vr as f64),
            A::Gravity if n.kind != Kind::Emitter => ret(L, n.gravity as f64),
            A::Alpha => ret(L, n.alpha as f64),
            A::Visible => retb(L, n.visible),
            A::Color => ret(L, (n.color >> 8) as f64),
            A::Order => ret(L, n.order as f64),
            A::Fixed => retb(L, n.fixed),
            A::Name => rets(L, &n.name),
            A::Tag => rets(L, &n.tag),
            A::Alive => retb(L, alive),
            A::Parent => match n.parent() {
                Some(p) if h.scene.alive(p) => {
                    push_node(L, h, p);
                    1
                }
                _ => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Children => {
                lua_createtable(L, n.children().len() as c_int, 0);
                for (i, c) in n.children().iter().enumerate() {
                    push_node(L, h, *c);
                    lua_rawseti(L, -2, i as c_int + 1);
                }
                1
            }
            A::Data => {
                lua_rawgeti(L, LUA_REGISTRYINDEX, h.data_ref);
                lua_rawgeti(L, -1, id.to_bits() as c_int);
                if lua_type(L, -1) != LUA_TTABLE && alive {
                    lua_pop(L, 1);
                    lua_newtable(L);
                    lua_pushvalue(L, -1);
                    lua_rawseti(L, -3, id.to_bits() as c_int);
                }
                lua_remove(L, -2);
                1
            }
            A::W | A::H if matches!(n.kind, Kind::Tilemap | Kind::Walker) => {
                let (lw, lh) = h.scene.local_size(id);
                ret(L, if a == A::W { lw } else { lh } as f64)
            }
            A::Map | A::Tiles => {
                let at = h.tilemaps.iter().find(|t| t.0 == id);
                let name = match a {
                    A::Map => at.and_then(|t| t.1).map(|m| h.project.maps[m].name.as_str()),
                    _ => at.and_then(|t| t.2).map(|s| h.project.tilesets[s].name.as_str()),
                };
                match name {
                    Some(s) => rets(L, s),
                    None => {
                        lua_pushnil(L);
                        1
                    }
                }
            }
            A::Behaviour | A::Behaviours | A::Knobs => {
                let Some(t) = h.quick.thing(id) else {
                    return match a {
                        A::Behaviour => {
                            lua_pushnil(L);
                            1
                        }
                        _ => {
                            lua_newtable(L);
                            1
                        }
                    };
                };
                match a {
                    A::Behaviour => match t.behs.first() {
                        Some(b) => rets(L, b.name()),
                        None => {
                            lua_pushnil(L);
                            1
                        }
                    },
                    A::Behaviours => {
                        lua_createtable(L, t.behs.len() as c_int, 0);
                        for (i, b) in t.behs.iter().enumerate() {
                            push_str(L, b.name());
                            lua_rawseti(L, -2, i as c_int + 1);
                        }
                        1
                    }
                    _ => {
                        // A copy: write node.knobs = { … } to change them.
                        lua_newtable(L);
                        let mut names: Vec<&str> = t.behs.iter().flat_map(|b| b.knobs().iter().map(|k| k.0)).collect();
                        names.dedup();
                        for k in names {
                            let c = std::ffi::CString::new(k).unwrap_or_default();
                            lua_pushnumber(L, h.quick.knob(t, k));
                            lua_setfield(L, -2, c.as_ptr());
                        }
                        1
                    }
                }
            }
            A::Footsteps | A::Hum => match super::sound::get_foley(h, super::sound::FoleyNode::D2(id), a == A::Hum) {
                Some(s) => rets(L, &s),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Height => ret(L, n.walker.as_ref().map_or(0.0, |w| w.height) as f64),
            A::Speed if n.kind == Kind::Walker => ret(L, n.walker.as_ref().map_or(0.0, |w| w.speed) as f64),
            A::Size | A::Speed | A::Life | A::Spread | A::Gravity if n.kind == Kind::Emitter => {
                let Some(e) = &n.emitter else { return 0 };
                match match a {
                    A::Size => e.size,
                    A::Speed => e.speed,
                    A::Life => e.life,
                    A::Spread => e.spread,
                    _ => e.gravity,
                } {
                    Some(v) => ret(L, v as f64),
                    None => {
                        lua_pushnil(L);
                        1
                    }
                }
            }
            A::Preset => rets(L, n.emitter.as_ref().map_or("sparkles", |e| e.preset.name())),
            A::Rate => ret(L, n.emitter.as_ref().map_or(0.0, |e| e.rate) as f64),
            A::Additive => retb(L, n.emitter.as_ref().is_some_and(|e| e.additive)),
            A::Emitting => retb(L, n.emitter.as_ref().is_some_and(|e| e.emitting)),
            A::Colors => {
                let colors = n.emitter.as_ref().map_or(&[][..], |e| &e.colors[..]);
                lua_createtable(L, colors.len() as c_int, 0);
                for (i, c) in colors.iter().enumerate() {
                    lua_pushnumber(L, (c >> 8) as f64);
                    lua_rawseti(L, -2, i as c_int + 1);
                }
                1
            }
            A::W | A::H => {
                let (lw, lh) = match n.kind {
                    Kind::Sprite => n.image.and_then(|i| h.scene.image(i)).map_or((0.0, 0.0), |im| (im.frame_w * n.sx.abs(), im.frame_h * n.sy.abs())),
                    Kind::Text => {
                        h.read_back();
                        let (w, hh) = maimbrain::ui::text(&n.text).font(n.font).size(n.size).wrap(n.width).measure();
                        (w * n.sx.abs(), hh * n.sy.abs())
                    }
                    _ => (n.w, n.h),
                };
                ret(L, if a == A::W { lw } else { lh } as f64)
            }
            A::Radius => ret(L, n.radius as f64),
            A::Image => match n.image.and_then(|i| h.project.images.get(i as usize)) {
                Some(im) => rets(L, &im.name),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Frame => ret(L, n.frame as f64),
            A::AnchorX => ret(L, n.anchor_x as f64),
            A::AnchorY => ret(L, n.anchor_y as f64),
            A::FlipX => retb(L, n.flip_x),
            A::FlipY => retb(L, n.flip_y),
            A::Shape => rets(
                L,
                match n.shape {
                    ShapeKind::Rect => "rect",
                    ShapeKind::Circle => "circle",
                    ShapeKind::Poly => "poly",
                    ShapeKind::Line => "line",
                },
            ),
            A::Points => {
                lua_createtable(L, n.points.len() as c_int, 0);
                for (i, p) in n.points.iter().enumerate() {
                    lua_pushnumber(L, *p as f64);
                    lua_rawseti(L, -2, i as c_int + 1);
                }
                1
            }
            A::Corner => ret(L, n.corner as f64),
            A::Line => ret(L, n.line as f64),
            A::Text => rets(L, &n.text),
            A::Font => rets(L, &super::font_name(h, n.font)),
            A::Size => ret(L, n.size as f64),
            A::Align => rets(
                L,
                match n.align {
                    Align::Left => "left",
                    Align::Center => "center",
                    Align::Right => "right",
                },
            ),
            A::Width => ret(L, n.width as f64),
            A::Parallax => ret(L, n.parallax as f64),
            A::RepeatX => retb(L, n.repeat_x),
            A::RepeatY => retb(L, n.repeat_y),
            A::Fill => match n.fill {
                Some(c) => ret(L, (c >> 8) as f64),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            _ => 0,
        }
    }
}

pub(crate) unsafe fn number(L: *mut lua_State, i: c_int, key: &str) -> Result<f32, String> {
    unsafe {
        match num(L, i) {
            Some(v) if v.is_finite() => Ok(v as f32),
            Some(v) => Err(format!("{key} takes a number, got {}", fmt_num(v))),
            None => Err(format!("{key} takes a number, got {}", type_name(L, i))),
        }
    }
}

pub(crate) unsafe fn boolean(L: *mut lua_State, i: c_int, key: &str) -> Result<bool, String> {
    unsafe {
        match lua_type(L, i) {
            LUA_TBOOLEAN => Ok(lua_toboolean(L, i) != 0),
            _ => Err(format!("{key} takes true or false, got {}", type_name(L, i))),
        }
    }
}

pub(crate) unsafe fn string(L: *mut lua_State, i: c_int, key: &str) -> Result<String, String> {
    unsafe {
        match lua_type(L, i) {
            LUA_TSTRING | LUA_TNUMBER => Ok(to_s(L, i)),
            _ => Err(format!("{key} takes a string, got {}", type_name(L, i))),
        }
    }
}

/// Writes the value at stack index `v` to property `a`, after checking it.
pub(crate) unsafe fn set(L: *mut lua_State, h: &mut Host, id: NodeId, a: A, v: c_int) -> Result<(), String> {
    unsafe {
        // An absolute index: some properties push before they read it (a
        // constructor's `data = { … }` came as -1, and stored the registry's table).
        let v = if v < 0 && v > LUA_REGISTRYINDEX { lua_gettop(L) + v + 1 } else { v };
        let Some(kind) = h.scene.get(id).map(|n| n.kind) else {
            return Err(format!("this node was destroyed: setting {} does nothing (check node.alive)", a.name()));
        };
        let key = a.name();
        if kind == Kind::Widget
            && let Some(r) = super::ui::widget_set(L, h, id, a, v)
        {
            return r;
        }
        // Values that need the scene or the project, checked before the node is borrowed.
        match a {
            A::Parent => return parent_at(L, h, id, v, true),
            A::Footsteps | A::Hum => return super::sound::set_foley(L, h, super::sound::FoleyNode::D2(id), a == A::Hum, v),
            #[cfg(phys2)]
            A::Body => return super::lovephysics::set_node_body(L, h, id, v),
            A::Data => {
                if lua_type(L, v) != LUA_TTABLE {
                    return Err(format!("data takes a table, got {}", type_name(L, v)));
                }
                lua_rawgeti(L, LUA_REGISTRYINDEX, h.data_ref);
                lua_pushvalue(L, v);
                lua_rawseti(L, -2, id.to_bits() as c_int);
                lua_pop(L, 1);
                return Ok(());
            }
            A::Image => {
                let name = string(L, v, key)?;
                let name = name.strip_prefix("images/").unwrap_or(&name);
                let name = name.rsplit_once('.').filter(|(_, e)| matches!(*e, "png" | "jpg" | "jpeg")).map_or(name, |(n, _)| n);
                let Some(i) = h.project.image(name) else {
                    return Err(format!("no image \"{name}\": add images/{name}.png"));
                };
                let rects = h.project.images[i].walker;
                // An image whose .toml gives it behaviours makes a thing of its kind.
                let iname = h.project.images[i].name.clone();
                if h.quick.kind(&iname).is_some_and(|k| !k.behs.is_empty()) && h.quick.thing(id).is_none() {
                    let n = h.scene.node_mut(id).unwrap();
                    if n.tag.is_empty() {
                        n.tag = iname.clone();
                    }
                    if n.order == 0.0 {
                        n.order = h.quick.kind(&iname).map_or(0.0, |k| k.order);
                    }
                    h.quick.adopt(&mut h.scene, id, &iname, None);
                }
                let n = h.scene.node_mut(id).unwrap();
                if let Some(w) = n.walker.as_mut() {
                    // A walker's picture is a body and a foot.
                    let Some((body, foot)) = rects else {
                        return Err(format!("images/{name} isn't a walker's picture: say where its body and foot are in images/{name}.toml (walker = true, which mb build measures, or body = [x, y, w, h] and foot = [x, y, w, h])"));
                    };
                    (w.body, w.foot) = (body, foot);
                }
                n.image = Some(i as u16);
                n.frame = 1;
                return Ok(());
            }
            A::Map | A::Tiles => return Err(format!("a Tilemap's {key} is set when it's made: Tilemap{{ map = \"level2\" }} makes another")),
            A::Behaviour | A::Behaviours => {
                let mut list = Vec::new();
                match lua_type(L, v) {
                    LUA_TNIL => {}
                    LUA_TSTRING if a == A::Behaviour => list.push(to_s(L, v)),
                    LUA_TTABLE if a == A::Behaviours => {
                        for i in 1..=lua_objlen(L, v) {
                            lua_rawgeti(L, v, i);
                            let s = st(L, -1).map(str::to_string);
                            lua_pop(L, 1);
                            list.push(s.ok_or("behaviours is a list of names")?);
                        }
                    }
                    _ if a == A::Behaviour => return Err(format!("behaviour takes a name (\"player.drag\"), got {}", type_name(L, v))),
                    _ => return Err(format!("behaviours takes a list of names, got {}", type_name(L, v))),
                }
                let mut behs = Vec::new();
                for b in &list {
                    behs.push(crate::quick::Beh::named(b).ok_or_else(|| crate::quick::Beh::unknown(b))?);
                }
                let n = h.scene.get(id).unwrap();
                let kind_name = if !n.tag.is_empty() { n.tag.clone() } else { n.image.and_then(|i| h.project.images.get(i as usize)).map_or_else(String::new, |i| i.name.clone()) };
                h.quick.adopt(&mut h.scene, id, &kind_name, Some(behs));
                return Ok(());
            }
            A::Knobs => {
                if lua_type(L, v) != LUA_TTABLE {
                    return Err(format!("knobs takes a table {{ speed = 140 }}, got {}", type_name(L, v)));
                }
                let v = if v < 0 { lua_gettop(L) + v + 1 } else { v };
                if h.quick.thing(id).is_none() {
                    let tag = h.scene.get(id).unwrap().tag.clone();
                    h.quick.adopt(&mut h.scene, id, &tag, None);
                }
                lua_pushnil(L);
                while lua_next(L, v) != 0 {
                    let (k, x) = (st(L, -2).map(str::to_string), num(L, -1));
                    lua_pop(L, 1);
                    match (k, x) {
                        (Some(k), Some(x)) => h.quick.set_knob(id, &k, x),
                        _ => {
                            lua_pop(L, 1);
                            return Err("knobs are names = numbers: { speed = 140, jump = 600 }".into());
                        }
                    }
                }
                return Ok(());
            }
            A::W | A::H if matches!(kind, Kind::Tilemap | Kind::Walker) => {
                return Err(format!("a {}'s {key} is read-only ({})", kind.name(), if kind == Kind::Walker { "its height is height" } else { "its map's size" }));
            }
            A::Height => {
                let x = number(L, v, key)?;
                if !(1.0..=2048.0).contains(&x) {
                    return Err(format!("height takes 1–2048 units, got {}", fmt_num(x as f64)));
                }
                if let Some(w) = h.scene.node_mut(id).unwrap().walker.as_mut() {
                    w.set_height(x);
                }
                return Ok(());
            }
            A::Speed if kind == Kind::Walker => {
                let x = number(L, v, key)?;
                if let Some(w) = h.scene.node_mut(id).unwrap().walker.as_mut() {
                    w.speed = x.max(1.0);
                }
                return Ok(());
            }
            A::Size | A::Speed | A::Life | A::Spread | A::Gravity | A::Preset | A::Rate | A::Colors | A::Additive | A::Emitting if kind == Kind::Emitter => {
                let value = match a {
                    A::Preset => {
                        let s = string(L, v, key)?;
                        Some(Err(preset_named(&s)?))
                    }
                    A::Colors => Some(Ok(colors_at(L, v)?)),
                    A::Additive | A::Emitting => {
                        boolean(L, v, key)?;
                        None
                    }
                    _ => None,
                };
                let x = match a {
                    A::Size | A::Speed | A::Life | A::Spread | A::Gravity | A::Rate if lua_type(L, v) != LUA_TNIL || a == A::Rate => Some(number(L, v, key)?),
                    _ => None,
                };
                let flag = lua_toboolean(L, v) != 0;
                let e = h.scene.node_mut(id).unwrap().emitter.as_mut().unwrap();
                match a {
                    A::Preset => {
                        if let Some(Err(p)) = value {
                            e.preset = p;
                        }
                    }
                    A::Colors => {
                        if let Some(Ok(c)) = value {
                            e.colors = c;
                        }
                    }
                    A::Additive => e.additive = flag,
                    A::Emitting => e.emitting = flag,
                    A::Rate => e.rate = x.unwrap_or(0.0).clamp(0.0, 2000.0),
                    A::Size => e.size = x.map(|x| x.max(0.0)),
                    A::Speed => e.speed = x,
                    A::Life => e.life = x.map(|x| x.max(0.01)),
                    A::Spread => e.spread = x,
                    _ => e.gravity = x,
                }
                return Ok(());
            }
            A::W | A::H if kind == Kind::Sprite => {
                let x = number(L, v, key)?;
                let n = h.scene.get(id).unwrap();
                let Some(im) = n.image.and_then(|i| h.scene.image(i)) else { return Err(format!("{key}: give the Sprite its image first")) };
                let (fw, fh) = (im.frame_w.max(1.0), im.frame_h.max(1.0));
                let n = h.scene.node_mut(id).unwrap();
                if a == A::W {
                    n.sx = x / fw * n.sx.signum();
                } else {
                    n.sy = x / fh * n.sy.signum();
                }
                return Ok(());
            }
            A::Frame => {
                let x = number(L, v, key)?;
                let n = h.scene.get(id).unwrap();
                let frames = n.image.and_then(|i| h.scene.image(i)).map_or(1, |im| im.frames());
                if x < 1.0 || x != x.trunc() || x as u32 > frames {
                    return Err(format!("frame takes 1 to {frames} (frames count from 1), got {}", fmt_num(x as f64)));
                }
                h.scene.node_mut(id).unwrap().frame = x as u32;
                return Ok(());
            }
            A::Font => {
                let name = string(L, v, key)?;
                let f = resolve_font(h, &name).map_err(|e| format!("font: {e}"))?;
                h.scene.node_mut(id).unwrap().font = f;
                return Ok(());
            }
            A::Children | A::Alive => return Err(format!("{key} is read-only")),
            A::W | A::H if kind == Kind::Text => return Err(format!("a Text's {key} is its measured size, read-only (wrap it with width)")),
            A::Stepped => return Err(format!("{key} is a signal")),
            _ => {}
        }
        let n = h.scene.node_mut(id).unwrap();
        match a {
            A::X => n.x = number(L, v, key)?,
            A::Y => n.y = number(L, v, key)?,
            A::Rotation => n.rotation = number(L, v, key)?,
            A::Scale => {
                let s = number(L, v, key)?;
                (n.sx, n.sy) = (s, s);
            }
            A::Sx => n.sx = number(L, v, key)?,
            A::Sy => n.sy = number(L, v, key)?,
            A::Vx => n.vx = number(L, v, key)?,
            A::Vy => n.vy = number(L, v, key)?,
            A::Vr => n.vr = number(L, v, key)?,
            A::Gravity => n.gravity = number(L, v, key)?,
            A::Alpha => {
                let x = number(L, v, key)?;
                if !(0.0..=1.0).contains(&x) {
                    let hint = if x > 1.0 && x <= 100.0 { format!(" Use {} for {}%.", fmt_num(x as f64 / 100.0), fmt_num(x as f64)) } else { String::new() };
                    return Err(format!("alpha takes 0–1, got {}.{hint}", fmt_num(x as f64)));
                }
                n.alpha = x;
            }
            A::Visible => n.visible = boolean(L, v, key)?,
            A::Color => n.color = color_at(L, v, false).map_err(|e| format!("color: {e}"))?,
            A::Order => match lua_type(L, v) {
                LUA_TSTRING if st(L, v) == Some("feet") => return Err("order = \"feet\" comes with footed sprites in a later engine build".into()),
                _ => n.order = number(L, v, key)?,
            },
            A::Fixed => n.fixed = boolean(L, v, key)?,
            A::Name => n.name = string(L, v, key)?,
            A::Tag => n.tag = string(L, v, key)?,
            A::W | A::H | A::Radius => {
                let x = number(L, v, key)?;
                if x < 0.0 {
                    return Err(format!("{key} can't be negative, got {}", fmt_num(x as f64)));
                }
                match a {
                    A::W => n.w = x,
                    A::H => n.h = x,
                    _ => n.radius = x,
                }
            }
            A::AnchorX => n.anchor_x = number(L, v, key)?,
            A::AnchorY => n.anchor_y = number(L, v, key)?,
            A::FlipX => n.flip_x = boolean(L, v, key)?,
            A::FlipY => n.flip_y = boolean(L, v, key)?,
            A::Shape => {
                n.shape = match st(L, v) {
                    Some("rect") => ShapeKind::Rect,
                    Some("circle") => ShapeKind::Circle,
                    Some("poly") => ShapeKind::Poly,
                    Some("line") => ShapeKind::Line,
                    _ => return Err(format!("shape is \"rect\", \"circle\", \"poly\" or \"line\", got {}", to_s(L, v))),
                }
            }
            A::Points => {
                if lua_type(L, v) != LUA_TTABLE {
                    return Err(format!("points takes a flat list {{ x1, y1, x2, y2, … }}, got {}", type_name(L, v)));
                }
                let len = lua_objlen(L, v);
                if len % 2 != 0 || len > 2048 {
                    return Err(format!("points takes pairs, x1, y1, x2, y2, … (up to 1024 points), got {len} numbers"));
                }
                let mut p = Vec::with_capacity(len as usize);
                for i in 1..=len {
                    lua_rawgeti(L, v, i);
                    let x = num(L, -1);
                    lua_pop(L, 1);
                    p.push(x.ok_or_else(|| format!("points[{i}] isn't a number"))? as f32);
                }
                n.points = p;
            }
            A::Corner => n.corner = number(L, v, key)?.max(0.0),
            A::Line => n.line = number(L, v, key)?.max(0.0),
            A::Text => n.text = string(L, v, key)?,
            A::Font => {} // resolved above
            A::Size => {
                let x = number(L, v, key)?;
                if !(1.0..=512.0).contains(&x) {
                    return Err(format!("size takes 1–512 units, got {}", fmt_num(x as f64)));
                }
                n.size = x;
            }
            A::Align => {
                n.align = match st(L, v) {
                    Some("left") => Align::Left,
                    Some("center") => Align::Center,
                    Some("right") => Align::Right,
                    _ => return Err(format!("align is \"left\", \"center\" or \"right\", got {}", to_s(L, v))),
                }
            }
            A::Width => n.width = number(L, v, key)?.max(0.0),
            A::Parallax => n.parallax = number(L, v, key)?,
            A::RepeatX => n.repeat_x = boolean(L, v, key)?,
            A::RepeatY => n.repeat_y = boolean(L, v, key)?,
            A::Fill => {
                n.fill = match lua_type(L, v) {
                    LUA_TNIL => None,
                    _ => Some(color_at(L, v, false).map_err(|e| format!("fill: {e}"))?),
                }
            }
            _ => return Err(unknown(kind, key, false)),
        }
        Ok(())
    }
}

/// Puts the node under the node at stack index `v` (or nil: the top).
/// `keep`: it stays where it is in the world (a write); a constructor's
/// `x` and `y` are under the parent instead.
pub(crate) unsafe fn parent_at(L: *mut lua_State, h: &mut Host, id: NodeId, v: c_int, keep: bool) -> Result<(), String> {
    unsafe {
        let parent = match lua_type(L, v) {
            LUA_TNIL => None,
            _ => match node_at(L, v) {
                Some(p) if h.scene.alive(p) => Some(p),
                Some(_) => return Err("parent: that node was destroyed".into()),
                None => return Err(format!("parent takes a node or nil, got {}", type_name(L, v))),
            },
        };
        if !h.scene.set_parent(id, parent, keep) {
            return Err("parent: a node can't go under itself or its own children".into());
        }
        Ok(())
    }
}

/// The atom of the key at stack index `i` and its text.
pub(crate) unsafe fn key_at(L: *mut lua_State, i: c_int) -> (Option<A>, String) {
    unsafe {
        if lua_type(L, i) != LUA_TSTRING {
            return (None, to_s(L, i));
        }
        let mut atom = -1;
        let mut len = 0;
        let p = lua_tolstringatom(L, i, &mut len, &mut atom);
        let s = String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len)).into_owned();
        (A::from(atom), s)
    }
}

/// Destroys a node (Rust's callers: `nextLevel`).
pub fn destroy_node(L: *mut lua_State, h: &mut Host, id: NodeId) {
    destroy(L, h, id)
}

/// Destroys a node: its handles and data stay readable a while (the graveyard).
fn destroy(L: *mut lua_State, h: &mut Host, id: NodeId) {
    for (gone, node) in h.scene.destroy(id) {
        h.graveyard.push_back((gone, node));
    }
    while h.graveyard.len() > GRAVEYARD {
        let (old, _) = h.graveyard.pop_front().unwrap();
        unsafe {
            for r in [h.handles_ref, h.data_ref] {
                lua_rawgeti(L, LUA_REGISTRYINDEX, r);
                lua_pushnil(L);
                lua_rawseti(L, -2, old.to_bits() as c_int);
                lua_pop(L, 1);
            }
        }
    }
}

b!(n_index, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let (a, key) = key_at(L, 2);
    #[cfg(phys2)]
    if a == Some(A::Body) && h.scene.alive(id) {
        return super::lovephysics::node_body(L, h, id);
    }
    #[cfg(phys2)]
    if let Some(a) = a
        && let Some(r) = super::character2d::index(L, h, id, a)
    {
        return r;
    }
    let Some(n) = read_node(h, id) else { return err(L, &format!("this node was destroyed long ago: reading {key}")) };
    match a {
        Some(a) if has(n.kind, a) && is_signal(a) => push_signal(L, h, a),
        Some(a) if has(n.kind, a) && !is_method(a) => get(L, h, id, n, a),
        _ => err(L, &unknown(n.kind, &key, false)),
    }
});

b!(n_newindex, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let (a, key) = key_at(L, 2);
    let kind = match read_node(h, id) {
        Some(n) => n.kind,
        None => return err(L, &format!("this node was destroyed: setting {key} does nothing (check node.alive)")),
    };
    #[cfg(phys2)]
    if let Some(a) = a
        && let Some(r) = super::character2d::newindex(L, h, id, a, 3)
    {
        return match r {
            Ok(()) => 0,
            Err(m) => err(L, &m),
        };
    }
    let r = match a {
        Some(a) if has(kind, a) && is_signal(a) => Err(format!("{key} is a signal: connect to it, {key}:connect(function() … end)")),
        Some(a) if has(kind, a) && !is_method(a) => set(L, h, id, a, 3),
        _ => Err(unknown(kind, &key, false)),
    };
    // A write beats motion: a tween moving this property stops moving it.
    if r.is_ok() {
        h.libs.written(crate::libs::Target::Node(id), &key);
    }
    match r {
        Ok(()) => 0,
        // "node.alpha takes 0–1, got 3." for a bad value; unknown names as they are.
        Err(m) if m.starts_with(&key) => err(L, &format!("node.{m}")),
        Err(m) => err(L, &m),
    }
});

b!(n_namecall, |L, h| {
    let Some(id) = node_at(L, 1) else { return err(L, "not a node") };
    let mut atom = -1;
    let name = cstr(lua_namecallatom(L, &mut atom)).unwrap_or_default();
    let Some(kind) = read_node(h, id).map(|n| n.kind) else { return err(L, &format!("this node was destroyed long ago: calling {name}")) };
    #[cfg(phys2)]
    if let Some(r) = super::character2d::call(L, h, id, A::from(atom)) {
        return r;
    }
    match A::from(atom) {
        Some(A::Destroy) => {
            destroy(L, h, id);
            0
        }
        Some(A::Find) => {
            let Some(n) = st(L, 2) else { return bad(L, 2, "find", "string") };
            match h.scene.find_under(id, n) {
                Some(c) => {
                    push_node(L, h, c);
                    1
                }
                None => {
                    lua_pushnil(L);
                    1
                }
            }
        }
        Some(A::WorldPosition) => {
            if !h.scene.alive(id) {
                return err(L, "worldPosition: this node was destroyed (check node.alive)");
            }
            let (x, y) = h.scene.world_position(id);
            ret2(L, x as f64, y as f64)
        }
        Some(A::Play) if kind == Kind::Sprite => play(L, h, id),
        Some(A::Emit) if kind == Kind::Emitter => {
            if !h.scene.alive(id) {
                return 0;
            }
            let n = opt(L, 2, 1.0).clamp(0.0, 2000.0) as usize;
            let (x, y) = h.scene.world_position(id);
            if let Some(e) = h.scene.node_mut(id).unwrap().emitter.as_mut() {
                e.emit(x, y, n);
            }
            0
        }
        Some(a @ (A::SolidAt | A::TileAt | A::Marker | A::Path)) if kind == Kind::Tilemap => {
            let Some(n) = read_node(h, id) else { return 0 };
            let Some(m) = n.tilemap.as_ref() else { return 0 };
            let (ox, oy) = if h.scene.alive(id) { h.scene.world_position(id) } else { (n.x, n.y) };
            // path(x1, y1, x2, y2, { diagonal }): A* over the cells that
            // aren't solid (the `pathfinding` crate, Decided 48), as the
            // cells' centres, in the world: { x1, y1, x2, y2, … } or nil.
            if a == A::Path {
                let mut at = [0f32; 4];
                for (k, v) in at.iter_mut().enumerate() {
                    match num(L, 2 + k as c_int) {
                        Some(x) if x.is_finite() => *v = x as f32,
                        _ => return bad(L, 2 + k as c_int, "path", "number"),
                    }
                }
                let diagonal = lua_type(L, 6) == LUA_TTABLE && {
                    lua_rawgetfield(L, 6, c"diagonal".as_ptr());
                    let d = lua_toboolean(L, -1) != 0;
                    lua_pop(L, 1);
                    d
                };
                let from = m.cell_of(at[0] - ox, at[1] - oy);
                let to = m.cell_of(at[2] - ox, at[3] - oy);
                let (cols, rows) = (m.cols as i32, m.rows as i32);
                let cells = maimbrain::nav::grid_path(cols, rows, (from.0 as i32, from.1 as i32), (to.0 as i32, to.1 as i32), diagonal, |x, y| m.flags(x as i64, y as i64) & 1 == 0);
                let Some(cells) = cells else {
                    lua_pushnil(L);
                    return 1;
                };
                lua_createtable(L, (cells.len() * 2) as c_int, 0);
                for (i, (cx, cy)) in cells.iter().enumerate() {
                    lua_pushnumber(L, (ox + (*cx as f32 + 0.5) * m.tile_w) as f64);
                    lua_rawseti(L, -2, (i * 2 + 1) as c_int);
                    lua_pushnumber(L, (oy + (*cy as f32 + 0.5) * m.tile_h) as f64);
                    lua_rawseti(L, -2, (i * 2 + 2) as c_int);
                }
                return 1;
            }
            if a == A::Marker {
                let Some(want) = st(L, 2) else { return bad(L, 2, "marker", "string") };
                return match m.markers.iter().find(|k| k.0 == want) {
                    Some((_, x, y)) => ret2(L, (ox + x) as f64, (oy + y) as f64),
                    None if m.markers.is_empty() => err(L, &format!("marker: no marker \"{want}\"; mark one in the map's legend: \"@\" = {{ marker = \"{want}\" }}")),
                    None => err(L, &format!("marker: no marker \"{want}\" ({})", m.markers.iter().map(|k| k.0.as_str()).collect::<Vec<_>>().join(", "))),
                };
            }
            let (Some(x), Some(y)) = (num(L, 2), num(L, 3)) else { return bad(L, if num(L, 2).is_none() { 2 } else { 3 }, &name, "number") };
            let (x, y) = (x as f32 - ox, y as f32 - oy);
            if a == A::SolidAt {
                return retb(L, m.solid_at(x, y));
            }
            match m.tile_at(x, y) {
                Some(t) => rets(L, &t.name),
                None => {
                    lua_pushnil(L);
                    1
                }
            }
        }
        _ => node_ext(L, h, &name).unwrap_or_else(|| err(L, &unknown(kind, &name, true))),
    }
});

b!(n_tostring, |L, h| {
    let Some(id) = node_at(L, 1) else { return rets(L, "node") };
    match read_node(h, id) {
        Some(n) if n.kind == Kind::Widget => rets(L, &format!("ui.{}{}", n.widget.as_ref().map_or("widget", |w| w.kind.name()), if n.name.is_empty() { String::new() } else { format!(" \"{}\"", n.name) })),
        Some(n) if !n.name.is_empty() => rets(L, &format!("{} \"{}\"", n.kind.name(), n.name)),
        Some(n) if !n.tag.is_empty() => rets(L, &format!("{} ({})", n.kind.name(), n.tag)),
        Some(n) => rets(L, n.kind.name()),
        None => rets(L, "destroyed node"),
    }
});

/// What a constructor sets first, so the rest can depend on it (a Sprite's
/// `w` needs its image's size).
const FIRST: &[A] = &[A::Image, A::Shape, A::Text, A::Font, A::Size, A::Parent];

unsafe fn make(L: *mut lua_State, h: &mut Host, kind: Kind) -> c_int {
    unsafe {
        // In a 3D game, a Group or Emitter (and a billboard Text) is a 3D node.
        #[cfg(three)]
        if super::nodes3d::wants_3d(L, h, kind) {
            return super::nodes3d::make_from_2d(L, h, kind);
        }
        let ctor = kind.name();
        let t = lua_type(L, 1);
        if t != LUA_TTABLE && t != LUA_TNIL && t != -1 {
            return err(L, &format!("{ctor} takes one table: {ctor}{{ x = 10, y = 20 }} (got {})", type_name(L, 1)));
        }
        let id = match h.scene.add(kind, None) {
            Ok(id) => id,
            Err(_) => return err(L, &format!("too many nodes ({} at once): destroy what's gone, or reuse nodes", maimbrain::scene::MAX_NODES)),
        };
        if kind == Kind::Text
            && let Some(n) = h.scene.node_mut(id)
        {
            // The identity's body font (the pixel font under the Arcade preset).
            n.font = h.body_font;
            if h.project.arcade {
                n.size = 8.0;
            }
        }
        if kind == Kind::Tilemap
            && let Err(m) = tilemap(L, h, id, t == LUA_TTABLE)
        {
            destroy(L, h, id);
            return err(L, &format!("{ctor}{{…}}: {m}"));
        }
        if t == LUA_TTABLE {
            // Two passes: FIRST's keys, then the rest in the table's order.
            for pass in 0..2 {
                lua_pushnil(L);
                while lua_next(L, 1) != 0 {
                    let (a, key) = key_at(L, -2);
                    let first = a.is_some_and(|a| FIRST.contains(&a));
                    if kind == Kind::Tilemap && matches!(a, Some(A::Map | A::Tiles)) {
                        // Read already.
                    } else if (pass == 0) == first {
                        let r = match a {
                            Some(A::Parent) => parent_at(L, h, id, -1, false),
                            Some(a) if has(kind, a) && !is_method(a) => set(L, h, id, a, -1),
                            _ if lua_type(L, -2) != LUA_TSTRING => Err(format!("{ctor}{{…}} takes names = values, got a {} key", type_name(L, -2))),
                            _ => Err(unknown(kind, &key, false)),
                        };
                        if let Err(m) = r {
                            lua_pop(L, 2);
                            destroy(L, h, id);
                            return err(L, &format!("{ctor}{{…}}: {m}"));
                        }
                    }
                    lua_pop(L, 1);
                }
            }
        }
        if kind == Kind::Tilemap
            && let Err(m) = things(L, h, id)
        {
            return err(L, &format!("{ctor}{{…}}: {m}"));
        }
        push_node(L, h, id);
        1
    }
}

/// A Tilemap's map and tile set (`map` and `tiles` in its table).
unsafe fn tilemap(L: *mut lua_State, h: &mut Host, id: NodeId, table: bool) -> Result<(), String> {
    unsafe {
        let field = |k: &CStr| -> Option<String> {
            if !table {
                return None;
            }
            lua_rawgetfield(L, 1, k.as_ptr());
            let s = st(L, -1).map(str::to_string);
            lua_pop(L, 1);
            s
        };
        let p = h.project.clone();
        let map = match field(c"map") {
            Some(m) => Some(p.map(&m).ok_or_else(|| {
                let names: Vec<&str> = p.maps.iter().map(|m| m.name.as_str()).collect();
                if names.is_empty() { format!("no map \"{m}\": add maps/{m}.map") } else { format!("no map \"{m}\" (maps: {})", names.join(", ")) }
            })?),
            None => return Err("a Tilemap needs its map: Tilemap{ map = \"level1\" } (maps/level1.map)".into()),
        };
        let m = &p.maps[map.unwrap()];
        let set = match (field(c"tiles"), m.set) {
            (Some(t), from) => {
                let s = p.tileset(&t).ok_or_else(|| format!("no tile set \"{t}\": add tiles/{t}.png (and tiles/{t}.toml)"))?;
                if from.is_some_and(|f| f != s) {
                    return Err(format!("maps/{}.map's tiles are from tiles/{}, not tiles/{t}", m.name, p.tilesets[from.unwrap()].name));
                }
                Some(s)
            }
            (None, from) => from,
        };
        build_map(h, id, map.unwrap(), set);
        Ok(())
    }
}

/// Fills a Tilemap node with map `map` drawn from tile set `set`; it
/// becomes the level (the quick path's tiles, `round.level`).
pub fn build_map(h: &mut Host, id: NodeId, map: usize, set: Option<usize>) {
    let p = h.project.clone();
    let m = &p.maps[map];
    let mut tm = TileMap { cols: m.cols, rows: m.rows, tile_w: 32.0, tile_h: 32.0, cells: m.cells.clone(), ..Default::default() };
    if let Some(s) = set {
        let ts = &p.tilesets[s];
        let im = &p.images[ts.image];
        (tm.tile_w, tm.tile_h) = (im.frame.0 as f32, im.frame.1 as f32);
        tm.tiles = ts.tiles.clone();
        tm.image = Some(ts.image as u16);
    }
    let (tw, th) = (tm.tile_w, tm.tile_h);
    tm.markers = m.markers.iter().map(|(n, (cx, cy))| (n.clone(), (*cx as f32 + 0.5) * tw, (*cy as f32 + 0.5) * th)).collect();
    h.scene.node_mut(id).unwrap().tilemap = Some(Box::new(tm));
    h.tilemaps.retain(|t| h.scene.alive(t.0));
    h.tilemaps.push((id, Some(map), set));
    // The level: the quick path's tiles, `round.level`, and `scene.find(map)`.
    let n = h.scene.node_mut(id).unwrap();
    if n.name.is_empty() {
        n.name = m.name.clone();
    }
    h.quick.level = Some(id);
    h.level_name = Some(m.name.clone());
}


/// The map's things: each a thing of its kind at its cell's middle
/// (`make_thing`), or a spawner of them (`every`, `spread`).
pub unsafe fn things(L: *mut lua_State, h: &mut Host, map: NodeId) -> Result<(), String> {
    unsafe {
        let Some(&(_, Some(m), _)) = h.tilemaps.iter().find(|t| t.0 == map) else { return Ok(()) };
        let p = h.project.clone();
        let (tw, th) = h.scene.get(map).and_then(|n| n.tilemap.as_ref()).map_or((32.0, 32.0), |t| (t.tile_w, t.tile_h));
        let (ox, oy) = h.scene.world_position(map);
        for thing in &p.maps[m].things {
            let (x, y) = (ox + (thing.cell.0 as f32 + 0.5) * tw, oy + (thing.cell.1 as f32 + 0.5) * th);
            let at = |e: String| format!("maps/{}.map's {}: {e}", p.maps[m].name, thing.kind);
            if h.quick.kind(&thing.kind).is_none() {
                return Err(at(format!("no kind named '{}' (add images/{}.png, or kind(\"{}\", {{ … }}) before the map is made)", thing.kind, thing.kind, thing.kind)));
            }
            match thing.every {
                Some(every) => {
                    h.quick.spawn_every(&thing.kind, every, Some((x, y)), thing.spread as f32 * p.unit(), thing.props.clone());
                }
                None => {
                    let mut nums = vec![("x".to_string(), x as f64), ("y".to_string(), y as f64)];
                    nums.extend(thing.props.iter().cloned());
                    make_thing(L, h, &thing.kind, &nums).map_err(at)?;
                }
            }
        }
        Ok(())
    }
}

/// Makes a thing of `kind` (§5 [quick]): a Sprite of its image, or its
/// box (`w` × `h` units); tagged with its kind; moved by the quick path.
/// `nums` are set as properties, or into its `data` when no node has them.
pub unsafe fn make_thing(L: *mut lua_State, h: &mut Host, kind: &str, nums: &[(String, f64)]) -> Result<NodeId, String> {
    unsafe {
        let Some(k) = h.quick.kind(kind).cloned() else {
            return Err(format!("no kind named '{kind}' (add images/{kind}.png, or kind(\"{kind}\", {{ … }}))"));
        };
        let nk = if k.image.is_some() { Kind::Sprite } else { Kind::Shape };
        let id = h.scene.add(nk, None).map_err(|_| format!("too many nodes ({} at once): destroy what's gone", maimbrain::scene::MAX_NODES))?;
        let n = h.scene.node_mut(id).unwrap();
        n.tag = kind.to_string();
        n.order = k.order;
        match k.image {
            Some(i) => {
                n.image = Some(i as u16);
                n.frame = 1;
            }
            None => {
                (n.w, n.h, n.color) = (k.w, k.h, k.color);
                if k.circle {
                    n.shape = ShapeKind::Circle;
                    n.radius = k.w / 2.0;
                }
            }
        }
        h.quick.adopt(&mut h.scene, id, kind, None);
        for (key, v) in nums {
            let a = NAMES.iter().position(|n| n == key).and_then(|i| A::from(i as i32)).filter(|a| has(nk, *a) && !is_method(*a));
            match a {
                Some(a) => {
                    lua_pushnumber(L, *v);
                    let r = set(L, h, id, a, -1);
                    lua_pop(L, 1);
                    r?;
                }
                None => {
                    // Into its data table (made here: it's new).
                    lua_rawgeti(L, LUA_REGISTRYINDEX, h.data_ref);
                    lua_rawgeti(L, -1, id.to_bits() as c_int);
                    if lua_type(L, -1) != LUA_TTABLE {
                        lua_pop(L, 1);
                        lua_newtable(L);
                        lua_pushvalue(L, -1);
                        lua_rawseti(L, -3, id.to_bits() as c_int);
                    }
                    let key = std::ffi::CString::new(key.as_str()).unwrap_or_default();
                    lua_pushnumber(L, *v);
                    lua_setfield(L, -2, key.as_ptr());
                    lua_pop(L, 2);
                }
            }
        }
        Ok(id)
    }
}

/// Sets the properties in the table at stack index `t` on a node made in
/// Rust (`spawn(kind, props)`), as a constructor would.
pub unsafe fn apply_props(L: *mut lua_State, h: &mut Host, id: NodeId, t: c_int) -> Result<(), String> {
    unsafe {
        let Some(kind) = h.scene.get(id).map(|n| n.kind) else { return Ok(()) };
        lua_pushnil(L);
        while lua_next(L, t) != 0 {
            let (a, key) = key_at(L, -2);
            let r = match a {
                Some(A::Parent) => parent_at(L, h, id, -1, false),
                Some(a) if has(kind, a) && !is_method(a) => set(L, h, id, a, -1),
                _ if lua_type(L, -2) != LUA_TSTRING => Err(format!("props take names = values, got a {} key", type_name(L, -2))),
                _ => Err(unknown(kind, &key, false)),
            };
            if let Err(m) = r {
                lua_pop(L, 2);
                return Err(m);
            }
            lua_pop(L, 1);
        }
        Ok(())
    }
}

/// A preset by name (or its singular); else the nearest name.
pub fn preset_named(s: &str) -> Result<Preset, String> {
    Preset::named(s).ok_or_else(|| {
        let near = PRESETS.iter().map(|(n, _)| (distance(s, n), *n)).min().map(|x| x.1).unwrap_or("sparkles");
        format!("preset \"{s}\" isn't one; did you mean \"{near}\"? ({})", PRESETS.iter().map(|p| p.0).collect::<Vec<_>>().join(", "))
    })
}

/// One colour or a list of them.
pub unsafe fn colors_at(L: *mut lua_State, v: c_int) -> Result<Vec<u32>, String> {
    unsafe {
        if lua_type(L, v) == LUA_TTABLE {
            lua_rawgeti(L, v, 1);
            let list = lua_type(L, -1) == LUA_TTABLE || (lua_type(L, -1) == LUA_TNUMBER && lua_tonumber(L, -1) > 1.0);
            lua_pop(L, 1);
            if list {
                let mut out = Vec::new();
                for i in 1..=lua_objlen(L, v).min(16) {
                    lua_rawgeti(L, v, i);
                    let c = color_at(L, -1, false);
                    lua_pop(L, 1);
                    out.push(c.map_err(|e| format!("colors[{i}]: {e}"))?);
                }
                return Ok(out);
            }
        }
        if lua_type(L, v) == LUA_TNIL {
            return Ok(Vec::new());
        }
        Ok(vec![color_at(L, v, false).map_err(|e| format!("colors: {e}"))?])
    }
}

b!(mk_tilemap, |L, h| make(L, h, Kind::Tilemap));
b!(mk_walker, |L, h| make(L, h, Kind::Walker));
b!(mk_emitter, |L, h| make(L, h, Kind::Emitter));

b!(mk_sprite, |L, h| make(L, h, Kind::Sprite));
b!(mk_shape, |L, h| make(L, h, Kind::Shape));
b!(mk_text, |L, h| make(L, h, Kind::Text));
b!(mk_layer, |L, h| make(L, h, Kind::Layer));
b!(mk_group, |L, h| make(L, h, Kind::Group));

b!(s_find, |L, h| {
    let Some(n) = st(L, 1) else { return bad(L, 1, "scene.find", "string") };
    match h.scene.find(n) {
        Some(id) => {
            push_node(L, h, id);
            1
        }
        None => {
            // Then the 3D nodes, in the order made (as the 2D ones are).
            #[cfg(three)]
            if let Some(w) = h.three.as_deref()
                && let Some(id) = w.scene.ids_made().into_iter().find(|id| w.info(*id).is_some_and(|i| i.name == n))
            {
                super::three::push_node(L, w, id);
                return 1;
            }
            lua_pushnil(L);
            1
        }
    }
});

b!(s_all, |L, h| {
    let Some(tag) = st(L, 1) else { return bad(L, 1, "scene.all", "string") };
    let ids = h.scene.all(tag);
    lua_createtable(L, ids.len() as c_int, 0);
    #[cfg_attr(not(three), allow(unused_mut))]
    let mut n = ids.len() as c_int;
    for (i, id) in ids.into_iter().enumerate() {
        push_node(L, h, id);
        lua_rawseti(L, -2, i as c_int + 1);
    }
    // Then the 3D nodes with it.
    #[cfg(three)]
    if let Some(w) = h.three.as_deref() {
        for id in w.scene.ids_made() {
            if w.info(id).is_some_and(|i| tag == "*" || i.tag == tag) {
                super::three::push_node(L, w, id);
                n += 1;
                lua_rawseti(L, -2, n);
            }
        }
    }
    let _ = n;
    1
});

/// A node method written in Lua (the prelude's `createTween`), if there's
/// one by this name: called with the node and the arguments.
pub(crate) unsafe fn node_ext(L: *mut lua_State, h: &Host, name: &str) -> Option<c_int> {
    unsafe {
        if h.ext_ref == 0 {
            return None;
        }
        let k = std::ffi::CString::new(name).ok()?;
        lua_rawgeti(L, LUA_REGISTRYINDEX, h.ext_ref);
        lua_rawgetfield(L, -1, k.as_ptr());
        lua_remove(L, -2);
        if lua_type(L, -1) != LUA_TFUNCTION {
            lua_pop(L, 1);
            return None;
        }
        let n = lua_gettop(L) - 1;
        lua_insert(L, 1);
        if lua_pcall(L, n, -1, 0) != LUA_OK {
            return Some(-2);
        }
        Some(lua_gettop(L))
    }
}

/// `sprite:play(name, { once, speed, restart })`: one of its sheet's
/// animations (`[anims]` in `images/<name>.toml`), run in Rust.
unsafe fn play(L: *mut lua_State, h: &mut Host, id: NodeId) -> c_int {
    unsafe {
        let Some(anim) = st(L, 2) else { return bad(L, 2, "play", "animation name") };
        let Some(img) = h.scene.get(id).and_then(|n| n.image) else { return err(L, "play: give the Sprite its image first") };
        let info = &h.project.images[img as usize];
        let Some((name, frames, fps)) = info.anims.iter().find(|a| a.0 == anim).cloned() else {
            let names: Vec<&str> = info.anims.iter().map(|a| a.0.as_str()).collect();
            return err(L, &if names.is_empty() {
                format!("play: images/{}.toml has no [anims] (run = {{ frames = \"1-8\", fps = 12 }})", info.name)
            } else {
                format!("play: no animation \"{anim}\" in images/{}.toml ({})", info.name, names.join(", "))
            });
        };
        let field = |k: &std::ffi::CStr| -> Option<c_int> {
            (lua_type(L, 3) == LUA_TTABLE).then(|| {
                lua_rawgetfield(L, 3, k.as_ptr());
                -1
            })
        };
        let mut once = false;
        let mut speed = 1.0f32;
        let mut restart = false;
        if field(c"once").is_some() {
            once = lua_toboolean(L, -1) != 0;
            lua_pop(L, 1);
        }
        if field(c"speed").is_some() {
            speed = num(L, -1).unwrap_or(1.0) as f32;
            lua_pop(L, 1);
        }
        if field(c"restart").is_some() {
            restart = lua_toboolean(L, -1) != 0;
            lua_pop(L, 1);
        }
        let sprites = &mut h.libs.sprites;
        match sprites.iter_mut().find(|s| s.node == id) {
            Some(s) if s.name == name && !restart && !s.done => s.speed = speed,
            Some(s) => *s = crate::libs::SpriteAnim { node: id, name, frames, fps, speed, once, t: 0.0, done: false },
            None => sprites.push(crate::libs::SpriteAnim { node: id, name, frames, fps, speed, once, t: 0.0, done: false }),
        }
        if let Some(f) = h.libs.sprites.iter().find(|s| s.node == id).and_then(|s| s.frames.first().copied())
            && let Some(n) = h.scene.node_mut(id)
            && restart
        {
            n.frame = f;
        }
        0
    }
}

// ---------------------------------------------------------------- camera

b!(c_index, |L, h| {
    let (a, key) = key_at(L, 2);
    // A 3D game's camera is the 3D one (§5 [camera]): x, y, z, fov, roll, near, far.
    #[cfg(three)]
    if let Some(n) = super::nodes3d::cam_index(L, h, a) {
        return n;
    }
    let c = &h.scene.camera;
    match a {
        Some(A::X) => ret(L, c.x as f64),
        Some(A::Y) => ret(L, c.y as f64),
        Some(A::Scale) => ret(L, c.scale as f64),
        Some(A::Rot) => ret(L, c.rot as f64),
        _ => err(L, &camera_unknown(&key)),
    }
});

fn camera_unknown(key: &str) -> String {
    match key {
        "zoom" => "the camera's zoom is camera.scale (or camera:zoomTo(s))".into(),
        "rotation" | "angle" => "the camera's turn is camera.rot (or camera:rotateTo(r))".into(),
        "fov" | "z" | "roll" => format!("camera.{key} is the 3D camera's (kind = \"3d\"); a 2D camera zooms with camera.scale"),
        _ => format!("no property `{key}` on the camera (x, y, scale, rot)"),
    }
}

/// `camera:follow(node, { lag, lead, deadzone = { w, h }, bounds })`: the
/// camera keeps the node in view, in Rust, after each frame's motion
/// (§5 [camera]). `follow(nil)` stops; writing camera.x or y does too.
unsafe fn follow(L: *mut lua_State, h: &mut Host) -> c_int {
    unsafe {
        if lua_type(L, 2) == LUA_TNIL || lua_gettop(L) < 2 {
            h.libs.follow = None;
            return 0;
        }
        let Some(node) = node_at(L, 2).filter(|n| h.scene.alive(*n)) else { return bad(L, 2, "camera:follow", "node") };
        let get = |k: &std::ffi::CStr| -> Option<f64> {
            if lua_type(L, 3) != LUA_TTABLE {
                return None;
            }
            lua_rawgetfield(L, 3, k.as_ptr());
            let v = num(L, -1);
            lua_pop(L, 1);
            v
        };
        let lag = get(c"lag").unwrap_or(0.12).max(0.0) as f32;
        let lead = get(c"lead").unwrap_or(0.0) as f32;
        let mut deadzone = (0.0, 0.0);
        let mut bounds = None;
        if lua_type(L, 3) == LUA_TTABLE {
            lua_rawgetfield(L, 3, c"deadzone".as_ptr());
            if lua_type(L, -1) == LUA_TTABLE {
                lua_rawgetfield(L, -1, c"w".as_ptr());
                lua_rawgetfield(L, -2, c"h".as_ptr());
                deadzone = (opt(L, -2, 0.0) as f32, opt(L, -1, 0.0) as f32);
                lua_pop(L, 2);
            }
            lua_pop(L, 1);
            lua_rawgetfield(L, 3, c"bounds".as_ptr());
            if let Some(b) = node_at(L, -1) {
                // A Tilemap: its whole map.
                if let Some(n) = h.scene.get(b) {
                    let (w, hh) = h.scene.local_size(b);
                    bounds = Some([n.x, n.y, w, hh]);
                }
            } else if lua_type(L, -1) == LUA_TTABLE {
                let mut v = [0.0f32; 4];
                for (i, k) in [c"x", c"y", c"w", c"h"].iter().enumerate() {
                    lua_rawgetfield(L, -1 - i as c_int, k.as_ptr());
                }
                let keyed = (0..4).any(|i| lua_type(L, -4 + i) != LUA_TNIL);
                for (i, slot) in v.iter_mut().enumerate() {
                    *slot = opt(L, -4 + i as c_int, 0.0) as f32;
                }
                lua_pop(L, 4);
                // `{ 0, 0, w, h }`, as the reference writes it, too.
                if !keyed {
                    for (i, slot) in v.iter_mut().enumerate() {
                        lua_rawgeti(L, -1, i as c_int + 1);
                        *slot = opt(L, -1, 0.0) as f32;
                        lua_pop(L, 1);
                    }
                }
                bounds = Some(v);
            } else if lua_type(L, -1) != LUA_TNIL {
                lua_pop(L, 1);
                return err(L, "camera:follow: bounds is a Tilemap or { x, y, w, h }");
            }
            lua_pop(L, 1);
        }
        h.libs.follow = Some(crate::libs::Follow { node, lag, lead, deadzone, bounds, lead_x: 0.0, at_edge: 0, reached_edge: false });
        0
    }
}

// `camera(x, y, z, look_x, look_y, look_z, fov, near, far)`: engine 1.0's 3D camera (api/three.rs).
b!(c_call, |L, h| {
    #[cfg(three)]
    if h.three.is_some() {
        return super::three::camera_call(L, h);
    }
    err(L, "camera(x, y, z, …) is a 3D game's camera (kind = \"3d\"): a 2D game moves it with camera.x and camera.y, or camera:lookAt(x, y)")
});

b!(c_newindex, |L, h| {
    let (a, key) = key_at(L, 2);
    #[cfg(three)]
    if let Some(n) = super::nodes3d::cam_newindex(L, h, a, &key) {
        return n;
    }
    let v = match number(L, 3, &key) {
        Ok(v) => v,
        Err(m) => return err(L, &format!("camera.{m}")),
    };
    h.libs.written(crate::libs::Target::Camera, &key);
    h.camera_touched = true;
    let c = &mut h.scene.camera;
    match a {
        Some(A::X) => c.x = v,
        Some(A::Y) => c.y = v,
        Some(A::Scale) if v > 0.0 => c.scale = v,
        Some(A::Scale) => return err(L, &format!("camera.scale is above 0, got {}", fmt_num(v as f64))),
        Some(A::Rot) => c.rot = v,
        _ => return err(L, &camera_unknown(&key)),
    }
    0
});

b!(c_namecall, |L, h| {
    let mut atom = -1;
    let name = cstr(lua_namecallatom(L, &mut atom)).unwrap_or_default();
    #[cfg(three)]
    if let Some(n) = super::nodes3d::cam_namecall(L, h, A::from(atom)) {
        return n;
    }
    let (w, hh) = (h.scene.width, h.scene.height);
    if matches!(A::from(atom), Some(A::LookAt | A::Move | A::ZoomTo | A::RotateTo)) {
        h.camera_touched = true;
    }
    let c = &mut h.scene.camera;
    match A::from(atom) {
        Some(A::LookAt) => {
            (c.x, c.y) = (opt(L, 2, c.x as f64) as f32, opt(L, 3, c.y as f64) as f32);
            0
        }
        Some(A::Move) => {
            (c.x, c.y) = (c.x + opt(L, 2, 0.0) as f32, c.y + opt(L, 3, 0.0) as f32);
            0
        }
        Some(A::ZoomTo) => match num(L, 2) {
            Some(s) if s > 0.0 => {
                c.scale = s as f32;
                0
            }
            _ => err(L, "camera:zoomTo(scale) takes a number above 0"),
        },
        Some(A::RotateTo) => {
            c.rot = opt(L, 2, 0.0) as f32;
            0
        }
        Some(A::WorldCoords) => {
            let (x, y) = c.to_world(w, hh, opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
            ret2(L, x as f64, y as f64)
        }
        Some(A::CameraCoords) => {
            let (x, y) = c.to_screen(w, hh, opt(L, 2, 0.0) as f32, opt(L, 3, 0.0) as f32);
            ret2(L, x as f64, y as f64)
        }
        Some(A::Attach) => {
            h.draw.push(Cmd::Attach);
            0
        }
        Some(A::Detach) => {
            h.draw.push(Cmd::Detach);
            0
        }
        Some(A::Shake) => err(L, "camera:shake is fx.shake(amount, secs): one shake for the whole game"),
        Some(A::Follow) => {
            h.camera_touched = true;
            follow(L, h)
        }
        _ => err(L, &format!("no method `{name}` on the camera (lookAt, move, zoomTo, rotateTo, attach, detach, worldCoords, cameraCoords)")),
    }
});
