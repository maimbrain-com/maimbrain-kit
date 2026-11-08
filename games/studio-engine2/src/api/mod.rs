//! What Lua sees: functions in library tables (`love.graphics`, `round`,
//! …) and userdata kinds (nodes, the camera), each a Rust function behind
//! the luau crate's trampoline. A binding reads and checks its arguments off
//! the stack, does its work on the [`Host`], pushes its results, and returns
//! -1 with a message to raise an error (the shim raises it from C, with the
//! line of the Lua that called it, so no Lua error unwinds through Rust).
//!
//! - `node.rs`: the retained scene's nodes (§4.3: C `__index`/`__newindex`
//!   on a userdata, properties matched by atom) and the camera;
//! - `graphics.rs`: `love.graphics`, recorded into the frame's draw list;
//! - `love.rs`: the rest of the LÖVE subset Stage A has (`love.timer`,
//!   `love.math`, `love.keyboard`, `love.mouse`, `love.touch`,
//!   `love.joystick`, `love.window`, `love.system`), `round`, `print`, and
//!   the chunk loader behind `require`.
#![allow(non_snake_case, clippy::missing_safety_doc)]

pub mod act;
pub mod atoms;
pub mod audio;
pub mod debug;
pub mod fx;
pub mod graphics;
pub mod libs;
pub mod love;
#[cfg(phys2)]
pub mod character2d;
#[cfg(phys2)]
pub mod lovephysics;
pub mod music;
pub mod node;
pub mod quick;
pub mod save;
pub mod shaders;
pub mod sensors;
pub mod sound;
#[cfg(three)]
pub mod bodies3d;
#[cfg(three)]
pub mod nav3d;
#[cfg(three)]
pub mod nodes3d;
#[cfg(three)]
pub mod three;
#[cfg(three)]
pub mod terrain;
#[cfg(three)]
pub mod world;
pub mod ui;
pub mod voice;

use std::collections::VecDeque;
use std::ffi::{CStr, c_int, c_void};
use std::rc::Rc;

pub use luau::*;
use maimbrain::Rng;
use maimbrain::gfx2d::Font;
use maimbrain::scene::{DrawList, Node, NodeId, Scene};

use crate::input::Held;
use crate::project::Project;

/// Userdata tags (`lua_newuserdatataggedwithmetatable`): one per kind of handle.
pub mod tag {
    pub const NODE: i32 = 1;
    pub const CAMERA: i32 = 2;
    pub const IMAGE: i32 = 3;
    pub const QUAD: i32 = 4;
    pub const FONT: i32 = 5;
    pub const TEXT: i32 = 6;
    pub const BATCH: i32 = 7;
    pub const SOURCE: i32 = 8;
    pub const JOYSTICK: i32 = 9;
    pub const TWEEN: i32 = 10;
    pub const GRID: i32 = 11;
    pub const ANIM: i32 = 12;
    pub const WORLD: i32 = 13;
    pub const TIMER: i32 = 14;
    pub const PARTICLES: i32 = 15;
    pub const RNG: i32 = 16;
    // 3D games (three.rs)
    pub const NODE3: i32 = 17;
    pub const GEOMETRY: i32 = 18;
    pub const MATERIAL: i32 = 19;
    pub const MODEL: i32 = 20;
    pub const EMITTER3: i32 = 21;
    pub const VOICE: i32 = 22;
    pub const ANIMTREE: i32 = 23;
    // LÖVE's canvases and cursors (graphics.rs, love.rs), apart from 3D's.
    pub const CANVAS: i32 = 30;
    pub const CURSOR: i32 = 31;
    // love.physics (lovephysics.rs), in this order.
    pub const PWORLD: i32 = 32;
    pub const PBODY: i32 = 33;
    pub const PFIXTURE: i32 = 34;
    pub const PSHAPE: i32 = 35;
    pub const PJOINT: i32 = 36;
    pub const PCONTACT: i32 = 37;
    // 3D physics' shapes (bodies3d.rs).
    pub const SHAPE3: i32 = 38;
    // The UI kit's handles (ui.rs): ui.hint, ui.lesson, ui.combo.
    pub const HINT: i32 = 39;
    pub const LESSON: i32 = 40;
    pub const COMBO: i32 = 41;
    // `sensors.scope` (sensors.rs), above physics (32–38) and the UI kit (39–41).
    pub const SCOPE: i32 = 47;
    // `audio.bus`'s handles (sound.rs), clear of the tracks building beside it.
    pub const BUS: i32 = 48;
    // love.graphics.newShader's (shaders.rs).
    pub const SHADER: i32 = 49;
}

/// A `love.graphics.newText` object.
#[derive(Clone, Debug)]
pub struct TextObject {
    pub font: maimbrain::gfx2d::Font,
    pub size: f32,
    pub text: String,
    pub wrap: f32,
    pub align: maimbrain::scene::Align,
}

/// A `love.graphics.newSpriteBatch`: its image and its sprites (quad, x, y,
/// r, sx, sy, ox, oy, colour).
#[derive(Clone, Debug)]
pub struct Batch {
    pub image: u16,
    pub max: usize,
    pub sprites: Vec<(Option<[f32; 4]>, [f32; 7], u32)>,
    pub color: u32,
}

/// How a round ended, from `round.over` (or its lives running out).
#[derive(Clone, Debug, PartialEq)]
pub struct Over {
    pub heading: Option<String>,
    pub cause: Option<String>,
    pub won: bool,
}

/// Everything the bindings reach, behind the VM's host pointer. It belongs
/// to one VM (one round): its scene, its random numbers, its frame.
pub struct Host {
    pub project: Rc<Project>,
    pub scene: Scene,
    /// `love.draw`'s commands, this frame.
    pub draw: DrawList,
    pub rng: Rng,
    /// Game time since the round started, and this frame's step.
    pub time: f64,
    pub dt: f32,
    /// This frame's real step (the world's `dt` is slower in a lesson's
    /// slow motion, 0 in a hitstop): what the UI moves by.
    pub dt_wall: f32,
    pub frames: u64,
    // love.graphics state
    pub color: u32,
    pub background: u32,
    pub line_width: f32,
    pub point_size: f32,
    pub font: Font,
    pub font_size: f32,
    pub held: Held,
    /// Per scene image: whether it's sampled nearest (`pixel = true`,
    /// `setDefaultFilter`, `Image:setFilter`); the engine applies it.
    pub nearest: Vec<bool>,
    /// `setDefaultFilter`'s, for what's made after it (in this build: every image).
    pub default_nearest: bool,
    pub texts: Vec<TextObject>,
    /// Fonts resolved from names, so a Text's `font` reads back as written.
    pub font_names: Vec<(maimbrain::gfx2d::Font, String)>,
    pub batches: Vec<Batch>,
    /// The session's sounds (by project sound), as they decode.
    pub sounds: Vec<Option<maimbrain::audio::Sound>>,
    pub sources: Vec<audio::Source>,
    /// `love.audio.setVolume`.
    pub volume: f32,
    /// The title card's attract mode plays no sound effects or haptics.
    pub mute: bool,
    /// What the round asked of the music this frame.
    pub music: Option<audio::Music>,
    /// The round's music graph or setlist, its mood and its beats (music.rs).
    pub tunes: music::MusicState,
    /// `love.math.newRandomGenerator`'s generators.
    pub rngs: Vec<Rng>,
    /// flux, hump.timer, anim8, Sprites' animations, the camera's follow.
    pub libs: crate::libs::Libs,
    /// anim8 grids: frame w, h, image w, h, left, top, border.
    pub grids: Vec<[f32; 7]>,
    /// The round's shaders: shader materials and love.graphics's (shaders.rs).
    pub shaders: crate::api::shaders::State,
    pub worlds: Vec<libs::BumpWorld>,
    /// Each Tilemap's map and tile set (project indices), to read back.
    pub tilemaps: Vec<(NodeId, Option<usize>, Option<usize>)>,
    /// `fx.shake`: largest offset, its length, time left.
    pub shake: (f32, f32, f32),
    /// `fx.hitstop`: time left frozen.
    pub hitstop: f32,
    /// `fx.slowmo`: the world's speed, easing back on real time (vm.rs).
    pub slowmo: maimbrain::ui::SlowMo,
    /// Buses, placed sound and foley (sound.rs).
    pub sound: sound::Sounds,
    /// Each `newParticleSystem`'s position (`setPosition`).
    pub ps_pos: Vec<(f32, f32)>,
    /// The quick path: kinds, things, behaviours, spawners (quick.rs).
    pub quick: crate::quick::Quick,
    /// This frame's input as LÖVE gets it, for the behaviours.
    pub frame_events: Vec<crate::input::LoveEvent>,
    /// The level's map name (`round.level`).
    pub level_name: Option<String>,
    /// The game moved the camera itself (no following the player by default).
    pub camera_touched: bool,
    /// A spawner handle's `stop` (registry ref).
    pub stop_ref: c_int,
    /// The game's debug-menu items (`debug.*`), and the menu's Show hitboxes.
    pub debug_items: Vec<debug::DebugItem>,
    pub show_hitboxes: bool,
    /// What the HUD shows (`ui.hud`), and an identity `ui.identity` asked for.
    pub hud: ui::HudSpec,
    pub identity_change: Option<String>,
    /// Hints, lessons, buttons' taps and focus, the countdown, the stats card, combos (api/ui.rs).
    pub ui: ui::UiState,
    /// The identity's body font: `print` and a sized `newFont` without a name use it.
    pub body_font: Font,
    /// The prelude's signal maker and its node methods (`__engine.hooks`).
    pub signal_ref: c_int,
    pub ext_ref: c_int,
    /// The safe area in canvas units: x, y, w, h.
    pub safe: (f32, f32, f32, f32),
    pub over: Option<Over>,
    /// Destroyed nodes as they last were, so a handler can still read the
    /// position of what it just destroyed (newest last, at most `GRAVEYARD`).
    pub graveyard: VecDeque<(NodeId, Node)>,
    /// Registry refs: node handles by id (one userdata per node, so `==`
    /// and table keys work), and each node's `data` table.
    pub handles_ref: c_int,
    pub data_ref: c_int,
    /// `round.score` after the last frame that finished.
    pub last_score: f64,
    /// `round.post`'s bests this round, and whether it's a daily round (boards.rs).
    pub boards: crate::boards::RoundBoards,
    pub last_lives: f64,
    /// Luau's interrupts in the last frame's Lua (its budget is 1M), for
    /// `mb play`'s budgets (headless.rs).
    pub lua_counted: i64,
    /// Where the canvas is on the screen, for 3D's `project`: the screen's
    /// size in logical units, then the canvas's offset and scale on it.
    pub screen: [f32; 5],
    /// A 3D game's world (three.rs): its nodes, meshes and the rest in mb3d.
    #[cfg(three)]
    pub three: Option<Box<crate::three::World3>>,
    /// Bot runs only (bot.rs): what the policy asks this frame, and what the game's handlers heard.
    pub bot: Option<Box<crate::bot::BotFrame>>,
    /// The round's seed, for `os.time` (love.rs).
    pub seed: u64,
    /// `mb play`'s `studio_trace`: log the quick path's events (quick.rs).
    pub trace: bool,
    /// `love.graphics.newCanvas`'s canvases, and the one being drawn to
    /// (`setCanvas`; none is the screen, as at the start of every frame).
    pub canvases: Vec<graphics::Canvas>,
    pub target: Option<usize>,
    /// love.physics: its worlds, bodies and the rest (lovephysics.rs).
    #[cfg(phys2)]
    pub phys2: Option<Box<lovephysics::Phys2>>,
    /// `round.subject`: the node the readability check measures (`mb play
    /// --check readability`, headless.rs), as the prelude last handed it over.
    pub subject: Option<Subject>,
    /// Voice and dialogue (voice.rs): the box, the cast, a cutscene.
    pub voice: Box<voice::Voice>,
    /// The phone's sensors this frame, and `sensors.scope`'s scopes (sensors.rs).
    pub sensors: sensors::Sensors,
    /// This frame's `debug.mark`s and the engine's own (a hitstop), for the
    /// smoothness check (`studio_smooth`, headless.rs).
    pub marks: Vec<String>,
    /// The world was frozen last frame (a hitstop): its end is marked too.
    pub froze: bool,
    /// The world's speed last frame (`ui`'s slow motion): a change is marked too.
    pub rate_was: f32,
    /// `love.draw` is running (`__engine.drawing`): text measured there only
    /// places drawing, so it isn't a read-back the native runner can't play.
    pub in_draw: bool,
    /// Seconds the round has been at 0 lives while `round.endDelay` holds
    /// its end (Decided 28's delayed form).
    pub zero_lives: f32,
}

/// A node the game named as what the player must see at a glance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Subject {
    D2(maimbrain::scene::NodeId),
    #[cfg(three)]
    D3(maimbrain::scene::three::Node3Id),
}

impl Host {
    /// The game's logic read something back from the host (text metrics):
    /// a bot run says so, since the native runner's host is stubbed. Not
    /// from `love.draw`, where a measure only lays out what's drawn (it
    /// can't change the round's outcome; auto runs still check with Deno).
    pub fn read_back(&self) {
        if self.in_draw {
            return;
        }
        if let Some(b) = &self.bot {
            b.reads.set(true);
        }
    }
}

impl Drop for Host {
    /// A round's sounds end with it (its world is gone).
    fn drop(&mut self) {
        for s in &mut self.sources {
            if let Some(v) = s.voice.take() {
                v.stop();
            }
        }
    }
}

thread_local! {
    /// The Arcade preset's palette (set as each VM is made; None otherwise).
    pub static PALETTE: std::cell::Cell<Option<[u32; 16]>> = const { std::cell::Cell::new(None) };
}

/// Destroyed nodes kept readable.
pub const GRAVEYARD: usize = 1024;

pub unsafe fn host<'a>(ud: *mut c_void) -> &'a mut Host {
    unsafe { &mut *(ud as *mut Host) }
}

pub unsafe fn err(L: *mut lua_State, msg: &str) -> c_int {
    unsafe { push_str(L, msg) };
    -1
}

pub unsafe fn type_name(L: *mut lua_State, i: c_int) -> &'static str {
    match unsafe { lua_type(L, i) } {
        -1 | LUA_TNIL => "nil",
        LUA_TBOOLEAN => "boolean",
        LUA_TNUMBER => "number",
        LUA_TSTRING => "string",
        LUA_TTABLE => "table",
        LUA_TFUNCTION => "function",
        LUA_TUSERDATA => match unsafe { lua_userdatatag(L, i) } {
            tag::NODE => "node",
            tag::CAMERA => "camera",
            tag::IMAGE => "Image",
            tag::QUAD => "Quad",
            tag::FONT => "Font",
            tag::TEXT => "Text",
            tag::BATCH => "SpriteBatch",
            tag::SOURCE => "Source",
            tag::JOYSTICK => "Joystick",
            tag::TWEEN => "tween",
            tag::GRID => "grid",
            tag::ANIM => "animation",
            tag::WORLD => "World",
            tag::TIMER => "timer handle",
            tag::PARTICLES => "ParticleSystem",
            tag::NODE3 => "3D node",
            tag::GEOMETRY => "mesh",
            tag::MATERIAL => "material",
            tag::MODEL => "model",
            tag::EMITTER3 => "emitter",
            tag::VOICE => "loop",
            tag::ANIMTREE => "AnimationTree",
            tag::BUS => "Bus",
            tag::SHADER => "Shader",
            tag::PWORLD => "World",
            tag::PBODY => "Body",
            tag::PFIXTURE => "Fixture",
            tag::PSHAPE => "Shape",
            tag::PJOINT => "Joint",
            tag::PCONTACT => "Contact",
            tag::SHAPE3 => "Shape3D",
            tag::HINT => "Hint",
            tag::LESSON => "Lesson",
            tag::COMBO => "combo",
            tag::SCOPE => "Scope",
            _ => "userdata",
        },
        _ => "value",
    }
}

/// An argument error in LÖVE's words: "bad argument #2 to 'rectangle' (number expected, got nil)".
pub unsafe fn bad(L: *mut lua_State, i: c_int, f: &str, want: &str) -> c_int {
    unsafe { err(L, &format!("bad argument #{i} to '{f}' ({want} expected, got {})", type_name(L, i))) }
}

pub unsafe fn num(L: *mut lua_State, i: c_int) -> Option<f64> {
    if unsafe { lua_type(L, i) } != LUA_TNUMBER {
        return None;
    }
    Some(unsafe { lua_tonumber(L, i) })
}

/// An optional number: the default when nil or missing.
pub unsafe fn opt(L: *mut lua_State, i: c_int, d: f64) -> f64 {
    unsafe { num(L, i).unwrap_or(d) }
}

pub unsafe fn st<'a>(L: *mut lua_State, i: c_int) -> Option<&'a str> {
    unsafe {
        if lua_type(L, i) != LUA_TSTRING {
            return None;
        }
        lua_tostr(L, i).and_then(|b| std::str::from_utf8(b).ok())
    }
}

pub unsafe fn ret(L: *mut lua_State, v: f64) -> c_int {
    unsafe { lua_pushnumber(L, v) };
    1
}

pub unsafe fn ret2(L: *mut lua_State, a: f64, b: f64) -> c_int {
    unsafe {
        lua_pushnumber(L, a);
        lua_pushnumber(L, b);
    }
    2
}

pub unsafe fn retb(L: *mut lua_State, v: bool) -> c_int {
    unsafe { lua_pushboolean(L, v as c_int) };
    1
}

pub unsafe fn rets(L: *mut lua_State, s: &str) -> c_int {
    unsafe { push_str(L, s) };
    1
}

/// A value as `tostring` would print it, for strings and numbers (and
/// `print`'s other values by type).
pub unsafe fn to_s(L: *mut lua_State, i: c_int) -> String {
    unsafe {
        match lua_type(L, i) {
            LUA_TNIL | -1 => "nil".into(),
            LUA_TBOOLEAN => (if lua_toboolean(L, i) != 0 { "true" } else { "false" }).into(),
            LUA_TSTRING => String::from_utf8_lossy(lua_tostr(L, i).unwrap_or_default()).into_owned(),
            LUA_TNUMBER => fmt_num(lua_tonumber(L, i)),
            _ => type_name(L, i).into(),
        }
    }
}

/// A number as Luau's `tostring` prints it: whole numbers without a
/// fraction, NaN as `nan`, else the shortest round trip.
pub fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        "nan".into()
    } else if v.is_infinite() {
        (if v > 0.0 { "inf" } else { "-inf" }).into()
    } else if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A colour as 2.0 takes it (§5 conventions): LÖVE's r, g, b[, a] from 0
/// to 1 (as numbers from `i`, or a table `{ r, g, b, a }`), or a
/// `0xRRGGBB` number (ours). `Err` names what was wrong.
pub unsafe fn color_at(L: *mut lua_State, i: c_int, spread: bool) -> Result<u32, String> {
    unsafe {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
        match lua_type(L, i) {
            LUA_TTABLE => {
                let mut v = [1.0f64; 4];
                for (k, slot) in v.iter_mut().enumerate() {
                    lua_rawgeti(L, i, k as c_int + 1);
                    if let Some(x) = num(L, -1) {
                        *slot = x;
                    } else if k < 3 {
                        lua_pop(L, 1);
                        return Err("a colour table is { r, g, b } or { r, g, b, a }, each 0–1".into());
                    }
                    lua_pop(L, 1);
                }
                Ok(c(v[0]) << 24 | c(v[1]) << 16 | c(v[2]) << 8 | c(v[3]))
            }
            LUA_TNUMBER => {
                let r = lua_tonumber(L, i);
                // r, g, b[, a] as numbers (LÖVE's setColor), when they're given.
                if spread && num(L, i + 1).is_some() {
                    let (g, b, a) = (opt(L, i + 1, 1.0), opt(L, i + 2, 1.0), opt(L, i + 3, 1.0));
                    return Ok(c(r) << 24 | c(g) << 16 | c(b) << 8 | c(a));
                }
                // Under the Arcade preset, 0–15 are the palette's colours.
                if let Some(p) = PALETTE.with(|p| p.get())
                    && (0.0..16.0).contains(&r)
                    && r == r.trunc()
                {
                    return Ok(p[r as usize]);
                }
                if (0.0..=16_777_215.0).contains(&r) && r == r.trunc() {
                    return Ok((r as u32) << 8 | 0xff);
                }
                if (0.0..1.0).contains(&r) {
                    return Err(format!("a colour is 0xRRGGBB or {{ r, g, b }}, got {}: for grey use {{ {r}, {r}, {r} }}", fmt_num(r)));
                }
                Err(format!("a colour is 0xRRGGBB (0 to 0xffffff) or {{ r, g, b }} from 0 to 1, got {}", fmt_num(r)))
            }
            _ => Err(format!("a colour is 0xRRGGBB or {{ r, g, b }}, got {}", type_name(L, i))),
        }
    }
}

/// Registers `name = f` in the table on top of the stack.
pub unsafe fn set_fn(L: *mut lua_State, ud: *mut c_void, name: &CStr, f: RustFn) {
    unsafe {
        sv_push_rfn(L, f, ud, name.as_ptr());
        lua_setfield(L, -2, name.as_ptr());
    }
}

/// The global table `path` (made if missing, `love.graphics` inside
/// `love`), left on top of the stack with its parents below it; returns how
/// many to pop.
pub unsafe fn global_table(L: *mut lua_State, path: &str) -> c_int {
    unsafe {
        let mut depth = 0;
        for part in path.split('.') {
            let k = std::ffi::CString::new(part).unwrap();
            let at = if depth == 0 { LUA_GLOBALSINDEX } else { -1 };
            if lua_getfield(L, at, k.as_ptr()) != LUA_TTABLE {
                lua_pop(L, 1);
                lua_newtable(L);
                lua_pushvalue(L, -1);
                lua_setfield(L, if depth == 0 { LUA_GLOBALSINDEX } else { -3 }, k.as_ptr());
            }
            depth += 1;
        }
        depth
    }
}

/// One function: its Lua name and the Rust behind it.
pub type Reg = (&'static CStr, RustFn);

/// A metatable for a userdata tag whose methods are its functions
/// (`image:getWidth()`): `__index` is the metatable itself.
pub unsafe fn register_methods(L: *mut lua_State, ud: *mut c_void, tag: c_int, name: &CStr, regs: &[Reg]) {
    unsafe {
        lua_createtable(L, 0, regs.len() as c_int + 2);
        for (n, f) in regs {
            set_fn(L, ud, n, *f);
        }
        lua_pushvalue(L, -1);
        lua_setfield(L, -2, c"__index".as_ptr());
        lua_pushstring(L, name.as_ptr());
        lua_setfield(L, -2, c"__type".as_ptr());
        lua_setreadonly(L, -1, 1);
        lua_setuserdatametatable(L, tag);
    }
}

/// A handle: a tagged userdata holding a number (an index into the host's
/// tables, or a node's id).
pub unsafe fn push_handle(L: *mut lua_State, tag: c_int, v: u32) -> c_int {
    unsafe {
        let p = lua_newuserdatataggedwithmetatable(L, 4, tag) as *mut u32;
        *p = v;
    }
    1
}

/// The handle at stack index `i`, if it's one of this tag.
pub unsafe fn handle(L: *mut lua_State, i: c_int, tag: c_int) -> Option<u32> {
    unsafe {
        if lua_type(L, i) != LUA_TUSERDATA || lua_userdatatag(L, i) != tag {
            return None;
        }
        Some(*(lua_touserdata(L, i) as *const u32))
    }
}

/// A userdata of `tag` holding a plain value (a quad's rectangle, a font
/// and its size).
pub unsafe fn push_value<T: Copy>(L: *mut lua_State, tag: c_int, v: T) -> c_int {
    unsafe {
        let p = lua_newuserdatataggedwithmetatable(L, std::mem::size_of::<T>(), tag) as *mut T;
        p.write_unaligned(v);
    }
    1
}

pub unsafe fn value_at<T: Copy>(L: *mut lua_State, i: c_int, tag: c_int) -> Option<T> {
    unsafe {
        if lua_type(L, i) != LUA_TUSERDATA || lua_userdatatag(L, i) != tag {
            return None;
        }
        Some((lua_touserdata(L, i) as *const T).read_unaligned())
    }
}

/// A font by name (§5 [text]): `"pixel"`, `"sans"` (Inter, the default),
/// `"bold"`, a library family the packer baked (`"Press Start 2P"`), or a
/// project font (`"fonts/mine.ttf"` or its name).
pub fn resolve_font(h: &mut Host, name: &str) -> Result<maimbrain::gfx2d::Font, String> {
    use maimbrain::gfx2d::Font;
    if h.project.arcade && name != "pixel" {
        return Err(format!("Arcade games use the pixel font only (\"pixel\"), not \"{name}\". Without preset = \"arcade\" any font works"));
    }
    let f = match name {
        "pixel" => Font::Pixel,
        "sans" | "Inter" | "inter" => Font::Sans,
        "bold" => Font::SansBold,
        _ => {
            let key = name.strip_prefix("fonts/").map(|n| n.rsplit_once('.').map_or(n, |(a, _)| a)).unwrap_or(name);
            // A LÖVE game's path to its own font file ("assets/basis33.ttf",
            // moved to fonts/ when it came into Studio): the file's name.
            let file = name.rsplit_once('/').map(|(_, f)| f).filter(|f| f.ends_with(".ttf") || f.ends_with(".otf")).map(|f| f.rsplit_once('.').map_or(f, |(a, _)| a));
            let Some(info) = h.project.font(key).or_else(|| file.and_then(|f| h.project.font(f))) else {
                return Err(format!("no font \"{name}\": a library font by its family name (mb fonts lists them), fonts/<name>.ttf, \"pixel\" or \"sans\""));
            };
            Font::asset(&format!("assets/{}", info.path), Font::Sans)
        }
    };
    if !h.font_names.iter().any(|(x, _)| *x == f) {
        h.font_names.push((f, name.to_string()));
    }
    Ok(f)
}

/// The name a font was asked for by.
pub fn font_name(h: &Host, f: maimbrain::gfx2d::Font) -> String {
    use maimbrain::gfx2d::Font;
    h.font_names.iter().find(|(x, _)| *x == f).map(|(_, n)| n.clone()).unwrap_or_else(|| {
        match f {
            Font::Pixel => "pixel",
            Font::SansBold => "bold",
            _ => "sans",
        }
        .into()
    })
}

/// Registers functions into a global table.
pub unsafe fn register(L: *mut lua_State, ud: *mut c_void, path: &str, regs: &[Reg]) {
    unsafe {
        let depth = global_table(L, path);
        for (name, f) in regs {
            set_fn(L, ud, name, *f);
        }
        lua_pop(L, depth);
    }
}

/// A metatable for a userdata tag, with its functions.
pub unsafe fn register_kind(L: *mut lua_State, ud: *mut c_void, tag: c_int, name: &CStr, regs: &[Reg]) {
    unsafe {
        lua_createtable(L, 0, regs.len() as c_int + 1);
        for (n, f) in regs {
            set_fn(L, ud, n, *f);
        }
        lua_pushstring(L, name.as_ptr());
        lua_setfield(L, -2, c"__type".as_ptr());
        lua_setreadonly(L, -1, 1);
        lua_setuserdatametatable(L, tag);
    }
}

/// The whole API into a fresh VM, before it's sandboxed (library tables
/// become read-only then).
pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        node::install(L, ud);
        graphics::install(L, ud);
        shaders::install(L, ud);
        love::install(L, ud);
        audio::install(L, ud);
        music::install(L, ud);
        sound::install(L, ud);
        libs::install(L, ud);
        fx::install(L, ud);
        quick::install(L, ud);
        save::install(L, ud);
        debug::install(L, ud);
        ui::install(L, ud);
        voice::install(L, ud);
        #[cfg(phys2)]
        lovephysics::install(L, ud);
        #[cfg(phys2)]
        character2d::install(L, ud);
        sensors::install(L, ud);
        // A 3D game's names (engine 1.0's 3D API), over the quick path's `spawn`.
        #[cfg(three)]
        if host(ud).three.is_some() {
            three::install(L, ud);
            nodes3d::install(L, ud);
            world::install(L, ud);
            terrain::install(L, ud);
            bodies3d::install(L, ud);
        }
        // A name from 1.0, Roblox or LÖVE that 2.0 calls something else,
        // read while undefined, says what to use (§5, S9's renames).
        lua_createtable(L, 0, 1);
        set_fn(L, ud, c"__index", foreign_global);
        lua_setmetatable(L, LUA_GLOBALSINDEX);
        // The sandbox makes the globals' tables read-only; love's own
        // (love.graphics, …) are a level down, so they're done here.
        lua_getfield(L, LUA_GLOBALSINDEX, c"love".as_ptr());
        lua_pushnil(L);
        while lua_next(L, -2) != 0 {
            if lua_type(L, -1) == LUA_TTABLE {
                lua_setreadonly(L, -1, 1);
            }
            lua_pop(L, 1);
        }
        lua_pop(L, 1);
    }
}

/// Defines a binding: `b!(name, |L, h| body)` with `h` the [`Host`].
macro_rules! b {
    ($(#[$m:meta])* $name:ident, |$L:ident, $h:ident| $body:expr) => {
        $(#[$m])*
        #[allow(unused_variables)]
        pub unsafe extern "C-unwind" fn $name($L: *mut luau::lua_State, ud: *mut std::ffi::c_void) -> std::ffi::c_int {
            #[allow(unused_variables, unused_unsafe)]
            let $h = unsafe { $crate::api::host(ud) };
            #[allow(unused_unsafe)]
            unsafe {
                $body
            }
        }
    };
}
pub(crate) use b;

/// Names a game may reach for that 2.0 calls something else: engine 1.0's
/// (docs/STUDIO-API.md), Roblox's and LÖVE's.
const FOREIGN: &[(&str, &str)] = &[
    // Engine 1.0.
    ("score", "round.score"),
    ("lives", "round.lives"),
    ("level", "round.level"),
    ("game_over", "round.over(heading, cause)"),
    ("spawn_every", "spawnEvery(kind, secs, props)"),
    ("next_level", "nextLevel(name)"),
    ("speed_up", "speedUp(f)"),
    ("rnd", "love.math.random() * n"),
    ("sfx", "love.audio.newSource(name, \"static\"):play()"),
    ("shake", "fx.shake(amount, secs)"),
    ("flash", "fx.flash(color, secs)"),
    ("hitstop", "fx.hitstop(secs)"),
    ("particles", "fx.burst(preset, x, y)"),
    ("popup", "fx.popup(text, x, y)"),
    ("text", "a Text node, Text{ text = \"…\" }, or love.graphics.print in love.draw"),
    ("solid_at", "map:solidAt(x, y) on the Tilemap"),
    ("tile_at", "map:tileAt(x, y) on the Tilemap"),
    ("after", "timer.after(secs, fn)"),
    ("every", "timer.every(secs, fn)"),
    ("W", "love.graphics.getWidth()"),
    ("H", "love.graphics.getHeight()"),
    ("time", "round.time"),
    ("load", "save.get(key) (Lua's own load is off in the sandbox)"),
    // Roblox.
    ("game", "the scene: scene.find(name), scene.all(tag)"),
    ("workspace", "the scene: scene.find(name), scene.all(tag)"),
    ("Instance", "a node constructor with one table: Sprite{ image = \"hero\", x = 40 }"),
    ("wait", "timer.after(secs, fn) (nothing blocks a frame)"),
    ("task", "timer.after(secs, fn) (nothing blocks a frame)"),
    ("tick", "love.timer.getTime()"),
    ("Vector2", "plain numbers: node.x, node.y"),
    ("Vector3", "plain numbers: node.x, node.y, node.z"),
    ("Color3", "a colour as 0xRRGGBB or { r, g, b } from 0 to 1"),
    ("Enum", "a string (\"left\", \"circle\")"),
    // LÖVE and Lua.
    ("conf", "studio.toml"),
    ("io", "save.get and save.set"),
];

/// Engine 1.0's 3D names: a 3D game has them (api/three.rs, prelude3d.lua).
const THREE_D: &[&str] = &[
    "mesh", "material", "node", "model", "emitter", "project", "sky", "sun", "fog", "post", "dof", "grade", "shafts", "halos", "film", "rect", "loop", "lowpass",
];

/// Engine 1.0's round globals, which a 3D game reads (and `score` and
/// `lives` writes) as `round`'s: (the global, its field of `round`).
const ROUND_ALIASES: &[(&str, &CStr)] = &[("score", c"score"), ("lives", c"lives"), ("title", c"title"), ("over", c"ended")];

// The globals' `__index`: nil, except for a name 2.0 calls something else,
// or, in a 3D game, engine 1.0's round globals.
b!(foreign_global, |L, h| {
    if let Some(k) = st(L, 2) {
        if h.project.three
            && let Some((_, field)) = ROUND_ALIASES.iter().find(|(n, _)| *n == k)
        {
            lua_rawgetfield(L, LUA_GLOBALSINDEX, c"round".as_ptr());
            if lua_type(L, -1) != LUA_TTABLE {
                lua_pushnil(L);
                return 1;
            }
            lua_rawgetfield(L, -1, field.as_ptr());
            return 1;
        }
        if !h.project.three && THREE_D.contains(&k) {
            return err(L, &format!("`{k}` is 3D's, engine 1.0's 3D name: a 3D game says kind = \"3d\" in studio.toml"));
        }
        if let Some((_, use_)) = FOREIGN.iter().find(|(n, _)| *n == k) {
            return err(L, &format!("`{k}` isn't in engine 2.0: use {use_}"));
        }
    }
    lua_pushnil(L);
    1
});

// A 3D game's globals' `__newindex` (vm.rs): writing engine 1.0's `score`
// or `lives` writes `round`'s; any other new global is the game's own.
b!(globals_newindex, |L, h| {
    if h.project.three
        && let Some(k) = st(L, 2)
        && let Some((_, field)) = ROUND_ALIASES.iter().take(2).find(|(n, _)| *n == k)
    {
        lua_rawgetfield(L, 1, c"round".as_ptr());
        if lua_type(L, -1) == LUA_TTABLE {
            lua_pushvalue(L, 3);
            lua_rawsetfield(L, -2, field.as_ptr());
        }
        lua_pop(L, 1);
        return 0;
    }
    lua_pushvalue(L, 2);
    lua_pushvalue(L, 3);
    lua_rawset(L, 1);
    0
});

