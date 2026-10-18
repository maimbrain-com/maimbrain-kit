//! The Lua side: one omnilua VM, its sandbox and budget, and the small Rust
//! core the prelude calls (tile collision, maps, drawing, sprite animation,
//! sound and effects). docs/spike/studio-lua-vm.md has the measurements
//! behind the split: Rust calls Lua once a frame (`step`), and Lua calls
//! Rust once per thing drawn and per thing that collides with tiles.
//!
//! Everything a game can observe is deterministic: the RNG is the SDK's
//! PCG32 seeded by the round, omnilua hashes strings with a fixed seed (so
//! `pairs()` order is the same everywhere), there's no clock, no files and no
//! `load` for the game, and Lua never runs in `mb_render` (an allocation
//! there would move later tables' addresses, which `pairs()` over table keys
//! and `tostring(t)` see).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use maimbrain::Rng;
use maimbrain::store;
use maimbrain::sys::{self, Level};
use omnilua::{Function, Lua, Sandbox, SandboxConfig, Table, Value, Variadic};

use crate::project::{Project, Scalar, TILE};

/// Lua instructions one frame may run, all handlers together. A Lua
/// instruction costs about one wasm loop iteration of the platform's
/// 300M-iteration fuel, so that's never the limit; time is: 2M take
/// ~35–45 ms on an M1 Pro, so a runaway loop stops well inside the 250 ms
/// watchdog on the floor device too.
pub const FRAME_BUDGET: u64 = 2_000_000;
/// Loading the scripts and `start()`.
pub const LOAD_BUDGET: u64 = 4_000_000;
/// GC-tracked bytes the VM may hold.
pub const MEMORY_LIMIT: usize = 32 << 20;

const PRELUDE: &str = include_str!("prelude.lua");

/// A sprite to draw: its top-left in world pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDraw {
    pub sprite: usize,
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub flip: bool,
}

/// A thing without a sprite, centred at x, y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeDraw {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: u8,
    pub circle: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextDraw {
    pub s: String,
    /// Screen (canvas) pixels.
    pub x: f32,
    pub y: f32,
    pub color: u8,
}

/// Feel the game asked for this frame; the engine plays it after `step`.
#[derive(Clone, Debug, PartialEq)]
pub enum Fx {
    Sfx(String),
    Music(Option<String>),
    Shake(f32),
    Flash(u8),
    Particles(String, f32, f32),
    Popup(String, f32, f32),
}

/// What a frame of Lua left for the renderer and the engine.
#[derive(Default)]
pub struct Out {
    /// Things in draw order: Ok(sprite) or Err(shape).
    pub things: Vec<Result<SpriteDraw, ShapeDraw>>,
    /// Hitboxes (the debug menu's "Show hitboxes"), centred.
    pub boxes: Vec<[f32; 4]>,
    pub texts: Vec<TextDraw>,
    pub fx: Vec<Fx>,
}

/// One frame's input, in canvas pixels (the prelude adds the camera).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameInput {
    pub tap: bool,
    pub tx: f32,
    pub ty: f32,
    pub down: bool,
    pub x: f32,
    pub y: f32,
    pub release: bool,
    pub rx: f32,
    pub ry: f32,
    pub swipe: Option<(f32, f32)>,
    /// Arrows, WASD, the d-pad or the left stick: −1 … 1.
    pub ax: f32,
    pub ay: f32,
    /// Jump pressed this frame (Space, Up, W, Z, or A, B, X, Y and the shoulders).
    pub jump: bool,
    /// Jump let go this frame.
    pub jumpup: bool,
    /// Buttons pressed this frame, for `on("press", name, fn)`.
    pub press: Vec<&'static str>,
    /// Tilt sideways, −1 … 1 (the sensors build only).
    pub tilt: Option<f32>,
    pub debug: Vec<DebugCmd>,
}

/// A debug-menu pick, as the prelude applies it.
#[derive(Clone, Debug, PartialEq)]
pub enum DebugCmd {
    Score(f64),
    Lives(f64),
    Invincible(bool),
    Hitboxes(bool),
    Level(String),
    Spawn(String),
    Knob(String, String, f64),
    /// A game item (1-based, as `debug.*` declared them) and its new value.
    Item(usize, f64),
    Set(String, Scalar),
}

/// The round's state after a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub over: bool,
    pub heading: Option<String>,
    pub camx: f32,
    pub camy: f32,
}

/// A knob the debug menu shows: kind, knob, value, min, max, step.
#[derive(Clone, Debug, PartialEq)]
pub struct Knob {
    pub kind: String,
    pub knob: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

/// A `debug.*` item from the game's Lua.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub options: Vec<String>,
}

/// What the prelude says the game has, for the debug menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub kinds: Vec<String>,
    pub knobs: Vec<Knob>,
    pub items: Vec<Item>,
}

pub struct Vm {
    lua: Lua,
    sandbox: Sandbox,
    step: Option<Function>,
    pub out: Rc<RefCell<Out>>,
    names: Vec<String>,
}

impl Drop for Vm {
    fn drop(&mut self) {
        // The handle first, while the VM is alive.
        self.step = None;
    }
}

fn rt_err(msg: impl std::fmt::Display) -> omnilua::Error {
    omnilua::Error::from(omnilua::LuaError::runtime(format_args!("{msg}")))
}

/// Globals a Studio script never gets (SPEC §3, docs/STUDIO.md "Determinism").
/// Lua's own `load` isn't listed: `install_api` has already replaced it with
/// the saved-data reader, `load(key)`, which removing it would delete.
const REMOVED: &[&str] = &["dofile", "loadfile", "loadstring", "package", "io", "os", "collectgarbage"];

/// How a VM starts.
pub enum Mode<'a> {
    /// Load the scripts only: the title screen's VM, which the debug menu
    /// reads its kinds, knobs and items from.
    Title,
    /// A round: the first level (or `level`), then `start()`.
    Round { level: Option<&'a str> },
}

impl Vm {
    pub fn new(project: &Rc<Project>, seed: u64, mode: Mode) -> Result<Vm, String> {
        let lua = Lua::new();
        // One Rust→Lua call a frame, so the ~0.8 µs a call this costs is
        // nothing, and the instruction budget's error gets its line from it.
        lua.set_capture_tracebacks(true);
        let out = Rc::new(RefCell::new(Out::default()));
        let rng = Rc::new(RefCell::new(Rng::new(seed)));
        let map = Rc::new(Cell::new(None::<usize>));
        let r = core(&lua, project, &out, &rng, &map, &mode).map_err(|e| e.to_string())?;
        install_api(&lua, project, &out, &rng).map_err(|e| e.to_string())?;
        let prelude = lua.load(PRELUDE).set_name("=prelude.lua").into_function().map_err(|e| e.to_string())?;
        let (step, begin): (Function, Function) = prelude.call(r).map_err(|e| describe(&e, &[]))?;
        let sandbox = lua
            .install_sandbox(SandboxConfig {
                instruction_limit: Some(LOAD_BUDGET),
                memory_limit_bytes: Some(MEMORY_LIMIT),
                check_interval: 1000,
                remove_globals: REMOVED.iter().map(|s| s.as_bytes().to_vec()).collect(),
            })
            .map_err(|e| e.to_string())?;
        let names: Vec<String> = project.scripts.keys().cloned().collect();
        let vm = Vm { lua, sandbox, step: Some(step), out, names };
        let main = &project.scripts["main"];
        vm.lua.load(main.as_bytes()).set_name("=main.lua").exec().map_err(|e| describe(&e, &vm.names))?;
        if let Mode::Round { .. } = mode {
            begin.call::<_, ()>(()).map_err(|e| describe(&e, &vm.names))?;
        }
        vm.set_budget(FRAME_BUDGET);
        Ok(vm)
    }

    /// Each frame's instruction budget from now on; 0 is none.
    pub fn set_budget(&self, n: u64) {
        let _ = self.lua.install_sandbox(SandboxConfig {
            instruction_limit: (n > 0).then_some(n),
            memory_limit_bytes: Some(MEMORY_LIMIT),
            check_interval: 1000,
            remove_globals: vec![],
        });
    }

    pub fn instructions_used(&self) -> u64 {
        self.sandbox.instructions_used().unwrap_or(0)
    }

    /// A number global (`score`, `lives`), 0 when it isn't one.
    pub fn number(&self, name: &str) -> f64 {
        match self.lua.globals().get::<_, Value>(name) {
            Ok(Value::Integer(i)) => i as f64,
            Ok(Value::Number(f)) if f.is_finite() => f,
            _ => 0.0,
        }
    }

    /// A global as text (`tostring`), or None when it's nil.
    pub fn global_string(&self, name: &str) -> Option<String> {
        match self.lua.globals().get::<_, Value>(name) {
            Ok(Value::Nil) | Err(_) => None,
            Ok(v) => to_s(&self.lua, v).ok(),
        }
    }

    /// Every live thing: kind, x, y (for `mb play`'s summary).
    pub fn things(&self) -> Vec<(String, f32, f32)> {
        let Ok(f) = self.lua.globals().get::<_, Function>("__things") else { return Vec::new() };
        let Ok(t) = f.call::<_, Table>(()) else { return Vec::new() };
        (1..=t.len().unwrap_or(0))
            .filter_map(|i| {
                let e: Table = t.get(i).ok()?;
                Some((e.get(1).ok()?, e.get::<_, f64>(2).ok()? as f32, e.get::<_, f64>(3).ok()? as f32))
            })
            .collect()
    }

    /// The level being played (its map's name).
    pub fn level(&self) -> Option<String> {
        self.lua.globals().get::<_, Option<String>>("level").ok().flatten()
    }

    /// The kinds, knobs and `debug.*` items, for the debug menu.
    pub fn menu(&self) -> Menu {
        let f: Option<Function> = self.lua.globals().get("__menu").ok();
        let Some(f) = f else { return Menu::default() };
        let Ok((names, knobs, items)) = f.call::<_, (Vec<String>, Table, Table)>(()) else { return Menu::default() };
        let mut m = Menu { kinds: names, ..Menu::default() };
        for i in 1..=knobs.len().unwrap_or(0) {
            let Ok(k) = knobs.get::<_, Table>(i) else { continue };
            m.knobs.push(Knob {
                kind: k.get(1).unwrap_or_default(),
                knob: k.get(2).unwrap_or_default(),
                value: k.get(3).unwrap_or(0.0),
                min: k.get(4).unwrap_or(0.0),
                max: k.get(5).unwrap_or(0.0),
                step: k.get(6).unwrap_or(1.0),
            });
        }
        for i in 1..=items.len().unwrap_or(0) {
            let Ok(t) = items.get::<_, Table>(i) else { continue };
            let options: Vec<String> = t.get::<_, Option<Vec<String>>>(7).ok().flatten().unwrap_or_default();
            m.items.push(Item {
                kind: t.get(1).unwrap_or_default(),
                label: t.get(2).unwrap_or_default(),
                value: t.get(3).unwrap_or(0.0),
                min: t.get(4).unwrap_or(0.0),
                max: t.get(5).unwrap_or(0.0),
                step: t.get(6).unwrap_or(1.0),
                options,
            });
        }
        m
    }

    /// One frame: debug picks, input, spawners and timers, handlers,
    /// behaviours, motion, touches, leaving, the camera, then the things to draw.
    pub fn step(&mut self, dt: f32, input: &FrameInput) -> Result<Step, String> {
        self.sandbox.reset();
        {
            let mut o = self.out.borrow_mut();
            o.things.clear();
            o.boxes.clear();
            o.texts.clear();
            o.fx.clear();
        }
        let r = (|| -> omnilua::Result<(bool, Option<String>, f64, f64)> {
            let i = self.lua.create_table()?;
            i.set("tap", input.tap)?;
            i.set("tx", input.tx as f64)?;
            i.set("ty", input.ty as f64)?;
            i.set("down", input.down)?;
            i.set("x", input.x as f64)?;
            i.set("y", input.y as f64)?;
            i.set("release", input.release)?;
            i.set("rx", input.rx as f64)?;
            i.set("ry", input.ry as f64)?;
            if let Some((dx, dy)) = input.swipe {
                i.set("swipe", true)?;
                i.set("sdx", dx as f64)?;
                i.set("sdy", dy as f64)?;
            }
            i.set("ax", input.ax as f64)?;
            i.set("ay", input.ay as f64)?;
            i.set("jump", input.jump)?;
            i.set("jumpup", input.jumpup)?;
            if !input.press.is_empty() {
                i.set("press", input.press.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
            }
            if let Some(t) = input.tilt {
                i.set("tilt", t as f64)?;
            }
            if !input.debug.is_empty() {
                let d = self.lua.create_table()?;
                for c in &input.debug {
                    let e = self.lua.create_table()?;
                    let s = |e: &Table, k: &str| e.push(k);
                    match c {
                        DebugCmd::Score(v) => (s(&e, "score")?, e.push(*v)?),
                        DebugCmd::Lives(v) => (s(&e, "lives")?, e.push(*v)?),
                        DebugCmd::Invincible(on) => (s(&e, "inv")?, e.push(*on as i64)?),
                        DebugCmd::Hitboxes(on) => (s(&e, "boxes")?, e.push(*on as i64)?),
                        DebugCmd::Level(l) => (s(&e, "level")?, e.push(l.as_str())?),
                        DebugCmd::Spawn(k) => (s(&e, "spawn")?, e.push(k.as_str())?),
                        DebugCmd::Knob(k, n, v) => (s(&e, "knob")?, {
                            e.push(k.as_str())?;
                            e.push(n.as_str())?;
                            e.push(*v)?
                        }),
                        DebugCmd::Item(n, v) => (s(&e, "item")?, {
                            e.push(*n as i64)?;
                            e.push(*v)?
                        }),
                        DebugCmd::Set(k, v) => (s(&e, "set")?, {
                            e.push(k.as_str())?;
                            match v {
                                Scalar::Num(n) => e.push(*n)?,
                                Scalar::Str(t) => e.push(t.as_str())?,
                                Scalar::Bool(b) => e.push(*b)?,
                            }
                        }),
                    };
                    d.push(e)?;
                }
                i.set("dbg", d)?;
            }
            let f = self.step.as_ref().expect("live VM");
            let (over, rest): (bool, Variadic<Value>) = f.call((dt as f64, i))?;
            let rest: Vec<Value> = rest.into_vec();
            let heading = match rest.first() {
                Some(Value::String(s)) => Some(s.to_str()?),
                _ => None,
            };
            let n = |v: Option<&Value>| match v {
                Some(Value::Integer(i)) => *i as f64,
                Some(Value::Number(f)) => *f,
                _ => 0.0,
            };
            Ok((over, heading, n(rest.get(1)), n(rest.get(2))))
        })();
        match r {
            Ok((over, heading, camx, camy)) => Ok(Step { over, heading, camx: camx as f32, camy: camy as f32 }),
            Err(e) => Err(describe(&e, &self.names)),
        }
    }
}

/// A Lua error as the creator (and Claude) should see it: the message with
/// its `main.lua:<line>:` position. The instruction budget's own error has no
/// position (omnilua 0.7.1), so it takes the innermost game line of the
/// traceback (inside a `pcall`, that's the `pcall`'s line).
pub fn describe(e: &omnilua::Error, scripts: &[String]) -> String {
    let msg = e.to_string();
    let tripped = msg.contains("instruction budget") || msg.contains("memory limit");
    let tb = if tripped { e.traceback_lossy() } else { None };
    let ours = |l: &str| scripts.iter().any(|s| l.starts_with(&format!("{s}.lua:")));
    match tb.as_deref().and_then(|tb| tb.lines().map(str::trim).find(|l| ours(l))) {
        Some(line) => {
            let at = line.split(':').take(2).collect::<Vec<_>>().join(":");
            let what = if msg.contains("memory") { "using too much memory (a table that only grows?)" } else { "a loop that never ends?" };
            format!("{at}: {msg} ({what})")
        }
        None => msg,
    }
}

fn num_or(v: Option<&f64>, d: f64) -> f64 {
    v.copied().unwrap_or(d)
}

/// The tile flags at a cell: 1 solid, 2 one-way, 4 hazard. Outside a map's
/// sides (but within its rows) is solid, so nothing walks off the edge;
/// above and below are open.
fn cell_flags(p: &Project, map: Option<usize>, cx: i64, cy: i64) -> i64 {
    let Some(m) = map.and_then(|i| p.maps.get(i)) else { return 0 };
    if cy < 0 || cy >= m.h as i64 {
        return 0;
    }
    if cx < 0 || cx >= m.w as i64 {
        return 1;
    }
    match m.cell(cx, cy) {
        Some((s, t)) => {
            let t = &p.tilesets[s as usize].tiles[t as usize];
            (t.solid as i64) | ((t.oneway as i64) << 1) | ((t.hazard as i64) << 2)
        }
        None => 0,
    }
}

/// Moves a box (centre x, y) by dx, dy against the map's tiles, in steps of
/// at most 8 px so nothing passes through a tile. Flags: 1 hit a wall, 2
/// landed (blocked moving down), 4 hit a ceiling, 8 touching a hazard tile.
/// A one-way tile blocks only from above, unless `drop`.
pub fn move_box(p: &Project, map: Option<usize>, mut x: f64, mut y: f64, w: f64, h: f64, dx: f64, dy: f64, drop: bool) -> (f64, f64, i64) {
    let t = TILE as f64;
    let cells = |lo: f64, hi: f64| ((lo / t).floor() as i64, ((hi - 1e-6) / t).floor() as i64);
    let mut flags = 0;
    let steps = (dx.abs().max(dy.abs()) / 8.0).ceil().max(1.0);
    let (sx, sy) = (dx / steps, dy / steps);
    for _ in 0..steps as usize {
        if sx != 0.0 {
            x += sx;
            let (x0, x1) = cells(x - w / 2.0, x + w / 2.0);
            let (y0, y1) = cells(y - h / 2.0, y + h / 2.0);
            let mut hit: Option<i64> = None;
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    if cell_flags(p, map, cx, cy) & 1 != 0 {
                        hit = Some(match hit {
                            None => cx,
                            Some(c) if sx > 0.0 => c.min(cx),
                            Some(c) => c.max(cx),
                        });
                    }
                }
            }
            if let Some(cx) = hit {
                x = if sx > 0.0 { cx as f64 * t - w / 2.0 } else { (cx + 1) as f64 * t + w / 2.0 };
                flags |= 1;
            }
        }
        if sy != 0.0 {
            let before = y + h / 2.0;
            y += sy;
            let (x0, x1) = cells(x - w / 2.0, x + w / 2.0);
            let (y0, y1) = cells(y - h / 2.0, y + h / 2.0);
            let mut hit: Option<i64> = None;
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    let f = cell_flags(p, map, cx, cy);
                    let blocks = f & 1 != 0 || (f & 2 != 0 && sy > 0.0 && !drop && before <= cy as f64 * t + 1e-6);
                    if blocks {
                        hit = Some(match hit {
                            None => cy,
                            Some(c) if sy > 0.0 => c.min(cy),
                            Some(c) => c.max(cy),
                        });
                    }
                }
            }
            if let Some(cy) = hit {
                if sy > 0.0 {
                    y = cy as f64 * t - h / 2.0;
                    flags |= 2;
                } else {
                    y = (cy + 1) as f64 * t + h / 2.0;
                    flags |= 4;
                }
            }
        }
    }
    // Standing still on the ground still counts as on it.
    if flags & 2 == 0 && dy >= 0.0 {
        let (x0, x1) = cells(x - w / 2.0, x + w / 2.0);
        let below = ((y + h / 2.0 + 0.5) / t).floor() as i64;
        let flush = ((y + h / 2.0) / t - (y + h / 2.0) / t).abs() < 1e-9 && ((y + h / 2.0) % t).abs() < 1e-6;
        if flush && (x0..=x1).any(|cx| cell_flags(p, map, cx, below) & 3 != 0) {
            flags |= 2;
        }
    }
    let (x0, x1) = cells(x - w / 2.0, x + w / 2.0);
    let (y0, y1) = cells(y - h / 2.0, y + h / 2.0);
    if (y0..=y1).any(|cy| (x0..=x1).any(|cx| cell_flags(p, map, cx, cy) & 4 != 0)) {
        flags |= 8;
    }
    (x, y, flags)
}

/// The table the prelude gets: the Rust core and what it needs to know.
fn core(lua: &Lua, project: &Rc<Project>, out: &Rc<RefCell<Out>>, rng: &Rc<RefCell<Rng>>, map: &Rc<Cell<Option<usize>>>, mode: &Mode) -> omnilua::Result<Table> {
    let r = lua.create_table()?;
    r.set("W", project.canvas.0 as f64)?;
    r.set("H", project.canvas.1 as f64)?;
    r.set("lives", project.lives)?;
    r.set("win", "YOU WIN!")?;
    // `mb play --json`: events (pickup, hurt, jump, …) go to the log.
    if sys::param("studio_trace").is_some() {
        r.set(
            "trace",
            // trace(round time, event, kind, other kind or "-", x, y)
            lua.create_function(|_, v: Variadic<Value>| {
                let words: Vec<String> = v
                    .into_vec()
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => s.to_str().unwrap_or_default(),
                        Value::Integer(i) => i.to_string(),
                        Value::Number(n) => format!("{n:.2}"),
                        _ => String::new(),
                    })
                    .collect();
                // The session's game time, as the runner's rounds and scores have it.
                sys::log(Level::Debug, &format!("studio-event {:.3} {}", sys::time(), words.get(1..).unwrap_or_default().join(" ")));
                Ok(())
            })?,
        )?;
    }
    if let Mode::Round { level: Some(l) } = mode {
        r.set("start_level", *l)?;
    }
    r.set("levels", project.maps.iter().map(|m| m.name.clone()).collect::<Vec<_>>())?;
    let kinds = lua.create_table()?;
    for (si, s) in project.sprites.iter().enumerate() {
        let k = lua.create_table()?;
        let (hx, hy, hw, hh) = s.hitbox;
        k.set("name", s.name.as_str())?;
        k.set("w", hw as f64)?;
        k.set("h", hh as f64)?;
        k.set("ox", (hx + hw / 2.0 - s.frame.0 as f32 / 2.0) as f64)?;
        k.set("oy", (hy + hh / 2.0 - s.frame.1 as f32 / 2.0) as f64)?;
        k.set("spr", if s.shape.is_some() { -1 } else { si as i64 })?;
        k.set("layer", s.layer as i64)?;
        if let Some(sh) = s.shape {
            k.set("color", sh.color as i64)?;
            k.set("circle", sh.circle)?;
        }
        let anims = lua.create_table()?;
        for (ai, a) in s.anims.iter().enumerate() {
            anims.set(a.0.as_str(), ai as i64)?;
        }
        k.set("anims", anims)?;
        if !s.behaviours.is_empty() {
            k.set("behaviours", s.behaviours.clone())?;
        }
        let knobs = lua.create_table()?;
        for (n, v) in &s.knobs {
            knobs.set(n.as_str(), *v)?;
        }
        k.set("knobs", knobs)?;
        if let Some(sp) = &s.spawns {
            k.set("spawns", sp.as_str())?;
        }
        kinds.push(k)?;
    }
    r.set("kinds", kinds)?;

    // sprite(spr, anim, t, cx, cy, flip): a sprite's frame, centred at cx, cy.
    let (o, p) = (out.clone(), project.clone());
    r.set(
        "sprite",
        lua.create_function(move |_, v: Variadic<f64>| {
            let v: Vec<f64> = v.into_vec();
            if let [spr, anim, t, cx, cy, flip] = v[..]
                && let Some(s) = p.sprites.get(spr as usize)
            {
                let (_, frames, fps) = &s.anims[(anim as usize).min(s.anims.len() - 1)];
                let i = ((t.max(0.0) * *fps as f64).floor() as usize) % frames.len();
                o.borrow_mut().things.push(Ok(SpriteDraw {
                    sprite: spr as usize,
                    frame: frames[i],
                    x: (cx - s.frame.0 as f64 / 2.0) as f32,
                    y: (cy - s.frame.1 as f64 / 2.0) as f32,
                    flip: flip != 0.0,
                }));
            }
            Ok(())
        })?,
    )?;
    let o = out.clone();
    r.set(
        "shape",
        lua.create_function(move |_, v: Variadic<f64>| {
            let v: Vec<f64> = v.into_vec();
            if let [x, y, w, h, c, circle] = v[..] {
                o.borrow_mut().things.push(Err(ShapeDraw { x: x as f32, y: y as f32, w: w as f32, h: h as f32, color: (c as i64 & 15) as u8, circle: circle != 0.0 }));
            }
            Ok(())
        })?,
    )?;
    let o = out.clone();
    r.set(
        "box",
        lua.create_function(move |_, v: Variadic<f64>| {
            let v: Vec<f64> = v.into_vec();
            if let [x, y, w, h] = v[..] {
                o.borrow_mut().boxes.push([x as f32, y as f32, w as f32, h as f32]);
            }
            Ok(())
        })?,
    )?;
    let (p, m) = (project.clone(), map.clone());
    r.set(
        "move",
        lua.create_function(move |_, v: Variadic<f64>| {
            let v: Vec<f64> = v.into_vec();
            let a = |i: usize| num_or(v.get(i), 0.0);
            let (x, y, f) = move_box(&p, m.get(), a(0), a(1), a(2), a(3), a(4), a(5), a(6) != 0.0);
            Ok((x, y, f))
        })?,
    )?;
    let (p, m) = (project.clone(), map.clone());
    r.set(
        "tile",
        lua.create_function(move |_, (x, y): (f64, f64)| {
            let t = TILE as f64;
            Ok(cell_flags(&p, m.get(), (x / t).floor() as i64, (y / t).floor() as i64))
        })?,
    )?;
    let (p, m) = (project.clone(), map.clone());
    r.set(
        "tile_name",
        lua.create_function(move |_, (x, y): (f64, f64)| {
            let t = TILE as f64;
            let Some(mp) = m.get().and_then(|i| p.maps.get(i)) else { return Ok(None) };
            Ok(mp.cell((x / t).floor() as i64, (y / t).floor() as i64).map(|(s, ti)| {
                let set = &p.tilesets[s as usize];
                let tile = &set.tiles[ti as usize];
                if ti == 0 { set.name.clone() } else { format!("{}.{}", set.name, tile.name) }
            }))
        })?,
    )?;
    let (p, m) = (project.clone(), map.clone());
    r.set(
        "level",
        lua.create_function(move |lua, name: String| {
            let Some(i) = p.maps.iter().position(|mp| mp.name == name) else {
                let known: Vec<&str> = p.maps.iter().map(|m| m.name.as_str()).collect();
                return Ok((Value::Nil, Some(format!("no level called '{name}' (maps/: {})", known.join(", ")))));
            };
            m.set(Some(i));
            let mp = &p.maps[i];
            let t = lua.create_table()?;
            t.set("name", mp.name.as_str())?;
            t.set("w", (mp.w * TILE) as f64)?;
            t.set("h", (mp.h * TILE) as f64)?;
            if let Some(mu) = &mp.music {
                t.set("music", mu.as_str())?;
            }
            let spawns = lua.create_table()?;
            for s in &mp.spawns {
                let e = lua.create_table()?;
                e.set("kind", s.kind.as_str())?;
                e.set("x", s.x as f64)?;
                e.set("y", s.y as f64)?;
                if let Some(every) = s.every {
                    e.set("every", every as f64)?;
                }
                e.set("spread", s.spread as f64)?;
                let props = lua.create_table()?;
                for (k, v) in &s.props {
                    props.set(k.as_str(), *v)?;
                }
                e.set("props", props)?;
                spawns.push(e)?;
            }
            t.set("spawns", spawns)?;
            Ok((Value::Table(t), None))
        })?,
    )?;
    let o = out.clone();
    r.set(
        "sfx",
        lua.create_function(move |_, name: String| {
            o.borrow_mut().fx.push(Fx::Sfx(name));
            Ok(())
        })?,
    )?;
    let p = project.clone();
    r.set("has_sound", lua.create_function(move |_, name: String| Ok(p.sounds.contains(&name)))?)?;
    let p = project.clone();
    r.set("source", lua.create_function(move |_, name: String| Ok(p.scripts.get(&name).cloned()))?)?;
    let _ = rng;
    Ok(r)
}

fn install_api(lua: &Lua, project: &Rc<Project>, out: &Rc<RefCell<Out>>, rng: &Rc<RefCell<Rng>>) -> omnilua::Result<()> {
    let g = lua.globals();

    // text("hi", x, y [, color]): on the screen this frame, in canvas pixels.
    let o = out.clone();
    g.set(
        "text",
        lua.create_function(move |lua, (v, x, rest): (Value, f64, Variadic<f64>)| {
            let rest: Vec<f64> = rest.into_vec();
            let s = to_s(lua, v)?;
            o.borrow_mut().texts.push(TextDraw { s, x: x as f32, y: num_or(rest.first(), 0.0) as f32, color: (num_or(rest.get(1), 7.0) as i64 & 15) as u8 });
            Ok(())
        })?,
    )?;
    let fx = |f: fn(&[Value]) -> Option<Fx>| {
        let o = out.clone();
        lua.create_function(move |_, v: Variadic<Value>| {
            if let Some(e) = f(&v.into_vec()) {
                o.borrow_mut().fx.push(e);
            }
            Ok(())
        })
    };
    fn n(v: Option<&Value>, d: f64) -> f64 {
        match v {
            Some(Value::Integer(i)) => *i as f64,
            Some(Value::Number(f)) if f.is_finite() => *f,
            _ => d,
        }
    }
    fn s(v: Option<&Value>) -> Option<String> {
        match v {
            Some(Value::String(s)) => s.to_str().ok(),
            Some(Value::Integer(i)) => Some(i.to_string()),
            _ => None,
        }
    }
    g.set("sfx", fx(|v| s(v.first()).map(Fx::Sfx))?)?;
    g.set("music", fx(|v| Some(Fx::Music(s(v.first()))))?)?;
    g.set("shake", fx(|v| Some(Fx::Shake(n(v.first(), 2.0) as f32)))?)?;
    g.set("flash", fx(|v| Some(Fx::Flash((n(v.first(), 7.0) as i64 & 15) as u8)))?)?;
    g.set("particles", fx(|v| Some(Fx::Particles(s(v.first()).unwrap_or_else(|| "burst".into()), n(v.get(1), 0.0) as f32, n(v.get(2), 0.0) as f32)))?)?;
    g.set(
        "popup",
        lua.create_function({
            let o = out.clone();
            move |lua, (v, x, y): (Value, f64, f64)| {
                let t = to_s(lua, v)?;
                o.borrow_mut().fx.push(Fx::Popup(t, x as f32, y as f32));
                Ok(())
            }
        })?,
    )?;

    // Saved data (the SDK's store; a debug session plays on a copy).
    g.set(
        "save",
        lua.create_function(|_, (key, v): (String, Value)| {
            let text = match v {
                Value::Nil => String::new(),
                Value::Boolean(b) => format!("b:{}", b as u8),
                Value::Integer(i) => format!("n:{i}"),
                Value::Number(f) => format!("n:{f}"),
                Value::String(s) => format!("s:{}", s.to_str()?),
                _ => return Err(rt_err("save keeps numbers, strings and true/false")),
            };
            if key.len() > 64 {
                return Err(rt_err("save: keys are at most 64 characters"));
            }
            store::set(&format!("studio:{key}"), text.as_bytes());
            Ok(())
        })?,
    )?;
    g.set(
        "load",
        lua.create_function(|lua, key: String| {
            let Some(b) = store::get(&format!("studio:{key}")) else { return Ok(Value::Nil) };
            let t = String::from_utf8_lossy(&b).into_owned();
            Ok(match t.split_once(':') {
                Some(("b", v)) => Value::Boolean(v == "1"),
                Some(("n", v)) => v.parse::<i64>().map(Value::Integer).or_else(|_| v.parse::<f64>().map(Value::Number)).unwrap_or(Value::Nil),
                Some(("s", v)) => Value::String(lua.create_string(v)?),
                _ => Value::Nil,
            })
        })?,
    )?;

    // Randomness: the SDK's PCG32, seeded by the round (SPEC §3).
    let r = rng.clone();
    g.set("rnd", lua.create_function(move |_, n: Option<f64>| Ok(r.borrow_mut().f32() as f64 * n.unwrap_or(1.0)))?)?;
    let r = rng.clone();
    g.set("chance", lua.create_function(move |_, p: f64| Ok((r.borrow_mut().f32() as f64) < p))?)?;
    let math: Table = g.get("math")?;
    let r = rng.clone();
    math.set(
        "random",
        lua.create_function(move |_, (m, n): (Option<i64>, Option<i64>)| {
            let mut r = r.borrow_mut();
            Ok(match (m, n) {
                (None, _) => Value::Number(r.next_u32() as f64 / 4294967296.0),
                (Some(m), None) if m >= 1 => Value::Integer(1 + (r.below(m.min(u32::MAX as i64) as u32) as i64)),
                // The width in i128: math.random(math.mininteger, math.maxinteger) is an
                // error, not an overflow (and never rng.below(0)).
                (Some(lo), Some(hi)) if hi >= lo && (hi as i128 - lo as i128) < u32::MAX as i128 => Value::Integer(lo + r.below((hi as i128 - lo as i128 + 1) as u32) as i64),
                _ => return Err(rt_err("bad argument to 'random' (interval is empty)")),
            })
        })?,
    )?;
    let r = rng.clone();
    math.set(
        "randomseed",
        lua.create_function(move |_, n: Option<i64>| {
            if let Some(n) = n {
                *r.borrow_mut() = Rng::new(n as u64);
            }
            Ok(())
        })?,
    )?;

    g.set(
        "print",
        lua.create_function(|lua, args: Variadic<Value>| {
            let parts = args.into_iter().map(|v| to_s(lua, v)).collect::<omnilua::Result<Vec<String>>>()?;
            sys::log(Level::Info, &parts.join("\t"));
            Ok(())
        })?,
    )?;

    // require("enemies") runs game/enemies.lua once and returns what it returns.
    let p = project.clone();
    let loaded: Rc<RefCell<Vec<(String, Value)>>> = Rc::default();
    g.set(
        "require",
        lua.create_function(move |lua, name: String| {
            if let Some((_, v)) = loaded.borrow().iter().find(|(n, _)| *n == name) {
                return Ok(v.clone());
            }
            let src = p.scripts.get(&name).ok_or_else(|| rt_err(format!("module '{name}' not found (game/{name}.lua)")))?;
            let v: Value = lua.load(src.as_bytes()).set_name(format!("={name}.lua")).eval()?;
            let v = if matches!(v, Value::Nil) { Value::Boolean(true) } else { v };
            loaded.borrow_mut().push((name, v.clone()));
            Ok(v)
        })?,
    )?;
    Ok(())
}

/// `tostring(v)`, Lua's own: numbers as `%.14g`, NaN always `nan`.
fn to_s(lua: &Lua, v: Value) -> omnilua::Result<String> {
    if let Value::String(s) = &v {
        return s.to_str();
    }
    let f: Function = lua.globals().get("tostring")?;
    f.call::<_, String>(v)
}
