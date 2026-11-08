//! One Luau VM, which is one round (docs/STUDIO-2.md §4.7, §4.13): the
//! sandbox and budget from spike S0, the API installed, the prelude, the
//! game's main.lua and love.load, then a step a frame. Lua runs only here,
//! in `mb_update`; the round's scene and draw list live in its [`Host`].
//!
//! Every chunk arrives as bytecode: the engine's prelude compiled when the
//! engine was built, the game's when the project was packed. Chunks are
//! named by their paths (`game/main.lua`), so errors name the source line.

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void};
use std::rc::Rc;

use luau::*;
use maimbrain::Rng;
use maimbrain::gfx2d::Font;
use maimbrain::scene::{DrawList, Scene, SceneImage};

use crate::api::{self, Host, Over};
use crate::input::{Held, LoveEvent};
use crate::project::Project;

/// Interrupts a frame (loop back-edges, calls and returns): a runaway loop
/// trips it at about the time 1.0's 2M omnilua instructions did (§4.6).
pub const FRAME_BUDGET: i64 = 1_000_000;
/// For main.lua's top level and love.load.
pub const LOAD_BUDGET: i64 = 2_000_000;
/// The Lua heap (§4.6).
pub const MEMORY_LIMIT: usize = 32 << 20;

/// The prelude and classic, compiled by build.rs (and the 3D build's prelude).
const PRELUDE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/prelude.luac"));
#[cfg(three)]
const PRELUDE3D: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/prelude3d.luac"));
const CLASSIC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classic.luac"));

/// Which VM: the title card's attract mode, or a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Title,
    Round,
}

/// The longest `round.endDelay` (s): a game's ending, not a second round.
pub const MAX_END_DELAY: f64 = 10.0;

/// What a frame's step says about the round.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Step {
    pub score: f64,
    pub lives: f64,
    /// At 0 lives, the round's end waits out `round.endDelay`.
    pub hold: bool,
    /// `round.over` was called (this frame or before).
    pub over: Option<Over>,
}

pub struct Vm {
    l: *mut lua_State,
    t: *mut lua_State,
    host: *mut Host,
    step_ref: c_int,
    pub mode: Mode,
}

impl Drop for Vm {
    fn drop(&mut self) {
        unsafe {
            sv_close(self.l);
            drop(Box::from_raw(self.host));
        }
    }
}

/// The message on top of the stack (or the budget's, which says the line
/// of the loop), popped.
unsafe fn take_error(l: *mut lua_State, t: *mut lua_State) -> String {
    unsafe {
        let msg = String::from_utf8_lossy(lua_tostr(t, -1).unwrap_or(b"(an error that isn't a string)")).into_owned();
        lua_pop(t, 1);
        cstr(sv_trip_message(l)).unwrap_or(msg)
    }
}

impl Vm {
    /// A VM for `project` with its own scene (the session's `images`, and a
    /// 3D game's `models`), its random numbers seeded by `seed`: main.lua
    /// and love.load have run.
    #[allow(clippy::redundant_closure_call, clippy::too_many_arguments)] // closures for `?`
    pub fn new(project: &Rc<Project>, seed: u64, mode: Mode, images: &[SceneImage], sounds: &[Option<maimbrain::audio::Sound>], models: &[Option<maimbrain::gfx3d::Model>], safe: (f32, f32, f32, f32)) -> Result<Vm, String> {
        luau::init();
        if project.three && !cfg!(three) {
            return Err("this is engine 2.0's 2D build: a 3D game (kind = \"3d\") plays on its 3D build, which mb build packs it with".into());
        }
        #[cfg(not(three))]
        let _ = models;
        let (w, h) = (project.size.0 as f32, project.size.1 as f32);
        // The identity's body font (`print`'s, a Text's): the pixel font under the Arcade preset.
        let body = if project.arcade { Font::Pixel } else { crate::theme(project).body_font };
        let mut scene = Scene::new(w, h);
        // Widgets draw in the identity (the Arcade preset's look too).
        scene.theme = crate::theme(project);
        // The Arcade preset: whole pixels, and 0–15 are its palette's colours.
        scene.camera.snap = project.arcade;
        crate::api::PALETTE.with(|p| p.set(project.arcade.then_some(project.palette)));
        for im in images {
            scene.add_image(im.clone());
        }
        let trace = mode == Mode::Round && maimbrain::sys::param("studio_trace").is_some_and(|v| v != "0");
        let host = Box::into_raw(Box::new(Host {
            project: project.clone(),
            scene,
            draw: DrawList::new(),
            rng: Rng::new(seed),
            time: 0.0,
            dt: 0.0,
            dt_wall: 0.0,
            frames: 0,
            color: 0xffffffff,
            background: if project.arcade { project.color(project.background.unwrap_or(0)) } else { 0x000000ff },
            line_width: 1.0,
            point_size: 1.0,
            font: body,
            font_size: if project.arcade { 8.0 } else { 16.0 },
            held: Held::default(),
            nearest: project.images.iter().map(|i| i.pixel).collect(),
            default_nearest: false,
            texts: Vec::new(),
            font_names: Vec::new(),
            batches: Vec::new(),
            sounds: if sounds.len() == project.sounds.len() { sounds.to_vec() } else { vec![None; project.sounds.len()] },
            sources: Vec::new(),
            volume: 1.0,
            mute: mode == Mode::Title,
            music: None,
            tunes: crate::api::music::MusicState::new(project.music.len()),
            rngs: Vec::new(),
            libs: Default::default(),
            grids: Vec::new(),
            shaders: Default::default(),
            worlds: Vec::new(),
            tilemaps: Vec::new(),
            shake: (0.0, 0.0, 0.0),
            hitstop: 0.0,
            slowmo: Default::default(),
            sound: crate::api::sound::Sounds::new(seed, project.sounds.iter().map(|s| s.0.clone()).collect(), mode == Mode::Title, trace),
            ps_pos: Vec::new(),
            quick: crate::quick::Quick::from_project(project)?,
            frame_events: Vec::new(),
            level_name: None,
            camera_touched: false,
            stop_ref: 0,
            debug_items: Vec::new(),
            show_hitboxes: false,
            hud: crate::api::ui::HudSpec::default_for(project.lives, project.maps.len(), project.scored),
            identity_change: None,
            ui: Default::default(),
            body_font: body,
            signal_ref: 0,
            ext_ref: 0,
            safe,
            over: None,
            graveyard: VecDeque::new(),
            handles_ref: 0,
            data_ref: 0,
            last_score: 0.0,
            boards: crate::boards::RoundBoards { title: mode == Mode::Title, daily: mode == Mode::Round && crate::boards::take_next_daily(), ..Default::default() },
            lua_counted: 0,
            last_lives: project.lives as f64,
            screen: [w, h, 0.0, 0.0, 1.0],
            bot: (mode == Mode::Round && crate::bot::wanted()).then(Box::default),
            seed,
            trace,
            canvases: Vec::new(),
            target: None,
            subject: None,
            voice: Box::new(crate::api::voice::Voice::new(project, crate::theme(project), seed, mode == Mode::Title)),
            sensors: Default::default(),
            marks: Vec::new(),
            froze: false,
            rate_was: 1.0,
            in_draw: false,
            zero_lives: 0.0,
            #[cfg(phys2)]
            phys2: None,
            #[cfg(three)]
            three: project.three.then(|| {
                let mut m = models.to_vec();
                m.resize(project.models.len(), None);
                Box::new(crate::three::World3::new(m))
            }),
        }));
        let l = unsafe { sv_new(MEMORY_LIMIT) };
        if l.is_null() {
            unsafe { drop(Box::from_raw(host)) };
            return Err("couldn't make a Lua VM".into());
        }
        let mut vm = Vm { l, t: std::ptr::null_mut(), host, step_ref: 0, mode };
        unsafe {
            (*lua_callbacks(l)).useratom = Some(api::atoms::useratom);
            sv_set_host(l, host as *mut c_void);
            // The API and the sandbox, under protection (they allocate).
            let mut t = std::ptr::null_mut();
            let mut engine_ref = 0;
            let mut globals_ref = 0;
            protect(l, l, &mut |l| {
                api::install(l, host as *mut c_void);
                for r in [&mut (*host).handles_ref, &mut (*host).data_ref] {
                    lua_newtable(l);
                    *r = lua_ref(l, -1);
                    lua_pop(l, 1);
                }
                // The engine's own functions (`__engine`, installed with the
                // API) go to the registry for the prelude: not a game global.
                lua_rawgetfield(l, LUA_GLOBALSINDEX, c"__engine".as_ptr());
                engine_ref = lua_ref(l, -1);
                lua_pop(l, 1);
                lua_pushnil(l);
                lua_rawsetfield(l, LUA_GLOBALSINDEX, c"__engine".as_ptr());
                lua_pushvalue(l, LUA_GLOBALSINDEX);
                globals_ref = lua_ref(l, -1);
                lua_pop(l, 1);
                t = sv_sandbox(l);
                // The game's thread, kept alive (the protected call's stack goes).
                lua_ref(l, -1);
                0
            })?;
            vm.t = t;
            // A round's love.load reads the sensors as they are as it starts
            // (to level tilt against how the phone is held); the title's reads none.
            if mode == Mode::Round {
                vm.host_mut().sensors.frame(crate::api::sensors::Sensed::read(), 0.0);
            }
            sv_set_budget(l, LOAD_BUDGET);
            let main = project.chunks.get("main").ok_or("game/main.lua hasn't been compiled")?;
            let lives = project.lives as f64;
            // The prelude (it returns load and step), classic as the global
            // Object, main.lua, then love.load: under protection, as a frame is.
            let mut out: Result<c_int, String> = Ok(0);
            protect(l, t, &mut |t| {
                // A closure for `?`: each step's error ends the setup.
                let run = || -> Result<c_int, String> {
                    if project.three {
                        // Engine 1.0's `score` and `lives` write round's (api/mod.rs).
                        lua_createtable(t, 0, 2);
                        lua_rawgeti(t, LUA_REGISTRYINDEX, globals_ref);
                        lua_setfield(t, -2, c"__index".as_ptr());
                        sv_push_rfn(t, api::globals_newindex, host as *mut c_void, c"__newindex".as_ptr());
                        lua_setfield(t, -2, c"__newindex".as_ptr());
                        lua_setreadonly(t, -1, 1);
                        lua_setmetatable(t, LUA_GLOBALSINDEX);
                    }
                    load_chunk(l, t, c"=prelude.lua", PRELUDE)?;
                    lua_rawgeti(t, LUA_REGISTRYINDEX, engine_ref);
                    lua_pushnumber(t, lives);
                    // round.title from the start: main.lua and love.load see it too.
                    lua_pushboolean(t, (mode == Mode::Title) as c_int);
                    // round.daily: a round on the daily board's seed.
                    lua_pushboolean(t, (*host).boards.daily as c_int);
                    pcall_on(l, t, 4, 3)?;
                    // The prelude's `extra`, for the 3D build's prelude.
                    let extra = lua_ref(t, -1);
                    lua_pop(t, 1);
                    let step_ref = lua_ref(t, -1);
                    lua_pop(t, 1);
                    let load = lua_ref(t, -1);
                    lua_pop(t, 1);
                    #[cfg(three)]
                    if project.three {
                        load_chunk(l, t, c"=prelude3d.lua", PRELUDE3D)?;
                        lua_rawgeti(t, LUA_REGISTRYINDEX, engine_ref);
                        lua_rawgeti(t, LUA_REGISTRYINDEX, extra);
                        pcall_on(l, t, 2, 0)?;
                    }
                    lua_unref(t, extra);
                    load_chunk(l, t, c"=classic.lua", CLASSIC)?;
                    pcall_on(l, t, 0, 1)?;
                    lua_rawsetfield(t, LUA_GLOBALSINDEX, c"Object".as_ptr());
                    load_chunk(l, t, c"=game/main.lua", main)?;
                    pcall_on(l, t, 0, 0)?;
                    lua_rawgeti(t, LUA_REGISTRYINDEX, load);
                    pcall_on(l, t, 0, 0)?;
                    lua_unref(t, load);
                    // A bot run: the policy's hands, and game/bot.lua after main.lua (§8.1).
                    if (*host).bot.is_some() {
                        let act = api::act::make(t, host as *mut c_void);
                        if let Some(b) = &mut (*host).bot {
                            b.act_ref = act;
                        }
                        if let Some(bc) = project.chunks.get("bot") {
                            load_chunk(l, t, c"=game/bot.lua", bc)?;
                            pcall_on(l, t, 0, 0)?;
                        }
                    }
                    Ok(step_ref)
                };
                out = run();
                0
            })?;
            vm.step_ref = out?;
            sv_set_budget(l, FRAME_BUDGET);
        }
        Ok(vm)
    }

    /// Whether its terrain, if it has one, has built its first view.
    pub fn ground_ready(&self) -> bool {
        #[cfg(three)]
        if let Some(w) = self.host().three.as_ref() {
            // And its navmeshes have baked (nav3.rs).
            return w.ground.as_ref().is_none_or(|g| g.terrain.built()) && w.nav_ready();
        }
        true
    }

    /// The round's world: its scene, draw list and state (never while Lua runs).
    pub fn host(&self) -> &Host {
        unsafe { &*self.host }
    }

    pub fn host_mut(&mut self) -> &mut Host {
        unsafe { &mut *self.host }
    }

    /// The session's music files as they've decoded (by project index), for its Directors.
    pub fn music_sounds(&mut self, sounds: &[Option<maimbrain::audio::Sound>]) {
        let h = self.host_mut();
        let mut sh = h.tunes.shared.borrow_mut();
        // In place: nothing allocated on the frame a file happens to decode.
        for (dst, s) in sh.sounds.iter_mut().zip(sounds) {
            *dst = *s;
        }
    }

    /// What the last round played, if love.load asked for nothing: it
    /// starts again on this round's clock (music.rs).
    pub fn carry_music(&mut self, asked: Option<crate::api::music::Asked>) {
        let h = self.host_mut();
        if let Some(a) = asked
            && matches!(h.tunes.track, crate::api::music::Track::None)
            && let Err(e) = crate::api::music::play(h, &a.name, a.zone.as_deref(), 0.0, a.volume)
        {
            maimbrain::sys::log(maimbrain::sys::Level::Warn, &e);
        }
    }

    /// What it plays for the next round, letting go of the sound (the round is over).
    pub fn hand_over_music(&mut self) -> Option<crate::api::music::Asked> {
        self.host_mut().tunes.hand_over()
    }

    /// One frame: the events, then love.update and love.draw (the draw list
    /// starts empty), then motion in Rust. `title` and `ended` are what
    /// `round.title` and `round.ended` say.
    pub fn step(&mut self, dt: f32, events: &[LoveEvent], held: &Held, title: bool, ended: bool) -> Result<Step, String> {
        // The sensors, read once for the frame (none on the title card or under the game-over card).
        let sensed = if title || ended { crate::api::sensors::Sensed::default() } else { crate::api::sensors::Sensed::read() };
        self.step_sensed(dt, events, held, title, ended, sensed)
    }

    /// [`step`](Self::step) with the frame's sensor readings given (tests).
    pub fn step_sensed(&mut self, dt: f32, events: &[LoveEvent], held: &Held, title: bool, ended: bool, sensed: crate::api::sensors::Sensed) -> Result<Step, String> {
        let h = self.host_mut();
        h.sensors.frame(sensed, dt);
        // A hitstop stops everything at once, on purpose: marked for the
        // smoothness check, as is the frame it lets go.
        h.marks.clear();
        let frozen = h.hitstop > 0.0;
        if frozen || h.froze {
            h.marks.push("hitstop".into());
        }
        h.froze = frozen;
        // Slow motion (fx.slowmo) slows the world, easing back on real time;
        // a lesson's slow motion and a stats card scale it too (ui.rate); a
        // hitstop holds it (love.update gets 0). Time and drawing go on.
        let slowed = h.slowmo.update(dt) * h.ui.rate();
        // A sudden change of speed (fx.slowmo's start, a lesson's slow motion, a
        // stats card) is on purpose too; easing back isn't sudden.
        if (slowed - h.rate_was).abs() > 0.05 {
            h.marks.push("slow motion".into());
        }
        h.rate_was = slowed;
        let world_dt = if h.hitstop > 0.0 {
            h.hitstop = (h.hitstop - dt).max(0.0);
            0.0
        } else {
            slowed
        };
        let dt_wall = dt;
        let dt = world_dt;
        h.dt = dt;
        h.dt_wall = dt_wall;
        h.time += dt_wall as f64;
        h.voice.dt = dt_wall;
        h.frames += 1;
        h.held = held.clone();
        // The buttons take their presses first (api/ui.rs).
        let routed;
        let events = if h.ui.wants_input(&h.scene) {
            routed = crate::api::ui::route_input(h, events);
            // A finger on a button is the button's (Decided 67): it isn't
            // held for love.mouse.isDown or love.touch.getTouches either.
            if let Some(touch) = h.ui.button_touch() {
                h.held.mouse[0] = false;
                if let Some(t) = touch {
                    h.held.touches.retain(|x| x.0 != t);
                }
            }
            &routed[..]
        } else {
            events
        };
        h.frame_events = events.to_vec();
        h.draw.clear();
        h.color = 0xffffffff;
        crate::api::shaders::frame_start(&mut h.shaders);
        h.target = None;
        let time = h.time;
        unsafe {
            let (l, t, step_ref) = (self.l, self.t, self.step_ref);
            sv_reset_budget(l);
            // The frame's arguments are pushed under protection too: a push
            // can raise (§ the luau crate's notes), and there'd be no handler.
            let mut out = (0.0, 0.0, 0.0, None);
            protect(l, t, &mut |t| {
                lua_rawgeti(t, LUA_REGISTRYINDEX, step_ref);
                lua_pushnumber(t, dt as f64);
                push_events(t, events);
                lua_pushnumber(t, time);
                lua_pushboolean(t, title as c_int);
                lua_pushboolean(t, ended as c_int);
                if lua_pcall(t, 5, 3, 0) != LUA_OK {
                    out.3 = Some(take_error(l, t));
                    return 0;
                }
                out = (lua_tonumber(t, -3), lua_tonumber(t, -2), lua_tonumber(t, -1), None);
                lua_pop(t, 3);
                0
            })?;
            let (score, lives, end_delay, failed) = out;
            if let Some(e) = failed {
                return Err(e);
            }
            let counted = sv_counted(l);
            let h = self.host_mut();
            h.lua_counted = counted;
            h.last_score = if score.is_finite() { score } else { 0.0 };
            h.last_lives = lives;
            // At 0 lives the round ends (Decided 28), after `round.endDelay`
            // seconds of real time when the game gives one, so its own ending
            // plays first; `round.over` ends it sooner.
            let hold = if lives <= 0.0 && h.over.is_none() && end_delay > 0.0 {
                h.zero_lives += dt_wall;
                h.zero_lives < end_delay.min(MAX_END_DELAY) as f32
            } else {
                h.zero_lives = 0.0;
                false
            };
            // The music decides its next bar lines on the round's clock (music.rs).
            h.tunes.update(time, dt_wall);
            // After Lua's frame: velocities and gravity, then the nodes that
            // follow love.physics bodies (stepped in love.update, as LÖVE does).
            use crate::api::sound::{FoleyNode, Footfall};
            let mut steps = Vec::new();
            for e in h.scene.step(dt) {
                // Heard next frame, before love.update.
                let maimbrain::scene::Event::Stepped { node, foot, x, y } = e;
                h.libs.pending.push(crate::libs::Due::Signal { node, name: "stepped", args: vec![foot as f64 + 1.0, x as f64, y as f64] });
                if !h.sound.noisy.is_empty() {
                    // As hard as the walker is going, of its full stride.
                    let weight = h.scene.get(node).and_then(|n| Some((n.vx.abs() / n.walker.as_ref()?.speed.max(1.0)).clamp(0.2, 1.0))).unwrap_or(0.5);
                    steps.push(Footfall { node: FoleyNode::D2(node), at: maimbrain::gfx3d::vec3(x, y, 0.0), weight });
                }
            }
            #[cfg(phys2)]
            crate::api::lovephysics::sync(h);
            // A node that follows a body was just put where the body is, as a
            // write would: a punch goes on from there (the body never sees it).
            #[cfg(phys2)]
            if !h.libs.punches.is_empty()
                && let Some(p) = h.phys2.as_deref()
            {
                for &(n, _) in &p.links {
                    h.libs.rebase(crate::libs::Target::Node(n), "x");
                    h.libs.rebase(crate::libs::Target::Node(n), "y");
                }
            }
            // Punches on top of whatever moved the nodes, on real time (as
            // the shake), before a 3D game's nodes go to mb3d.
            crate::api::libs::punch_step(l, h, dt_wall);
            // A 3D game's nodes move, and the ones that changed go to mb3d,
            // in one pass (§4.3); then its models' animation.
            // Shader materials' uniforms tables, read again (a tween may have moved them).
            #[cfg(three)]
            if !h.shaders.materials.is_empty() {
                crate::api::shaders::sync(l, h);
            }
            #[cfg(three)]
            if let Some(w) = &mut h.three {
                w.end_frame(dt);
                // Its models' signals, heard next frame before love.update.
                for (node, name, args) in w.signals.drain(..) {
                    if name == "stepped" && args.len() >= 5 && !h.sound.noisy.is_empty() {
                        let at = maimbrain::gfx3d::vec3(args[1] as f32, args[2] as f32, args[3] as f32);
                        steps.push(Footfall { node: FoleyNode::D3(node), at, weight: (args[4] as f32).clamp(0.2, 1.0) });
                    }
                    h.libs.pending.push(crate::libs::Due::Signal3 { node, name, args });
                }
                for (node, name, other) in w.node_signals.drain(..) {
                    h.libs.pending.push(crate::libs::Due::Signal3Node { node, name, other });
                }
            }
            crate::api::fx::shake_step(h, dt_wall);
            h.scene.step_widgets(dt_wall);
            // Then the camera, after everything has moved.
            if let Some(mut f) = h.libs.follow.take()
                && f.step(&mut h.scene, dt)
            {
                // It stops at the bounds' edge at once, as it always has: the
                // engine's own, so the smoothness check doesn't count it.
                if f.reached_edge {
                    h.marks.push("camera edge".into());
                }
                h.libs.follow = Some(f);
            }
            // Sound, from where everything ended up.
            crate::api::sound::frame(h, dt_wall, dt, &steps);
            Ok(Step { score: if score.is_finite() { score } else { 0.0 }, lives, hold, over: h.over.clone() })
        }
    }

    /// A bot run's policy for this frame (§8.1): `bot(act)` from
    /// game/bot.lua, else the quick path's default for its player
    /// (autoplay.rs); and whether to follow the hints on screen too. `finger` is where the bot's finger is (canvas units,
    /// and whether it's down), for `act.finger()`.
    pub fn bot_step(&mut self, finger: (f32, f32, bool)) -> Result<(Vec<crate::bot::Intent>, bool), String> {
        let (l, t, host) = (self.l, self.t, self.host);
        let act = match unsafe { &mut (*host).bot } {
            Some(b) => {
                b.finger = finger;
                b.intents.clear();
                b.act_ref
            }
            None => return Ok((Vec::new(), false)),
        };
        let mut lua = false;
        let mut out: Result<(), String> = Ok(());
        unsafe {
            // Its own budget: the policy isn't the game's frame.
            sv_reset_budget(l);
            protect(l, t, &mut |t| {
                lua_rawgetfield(t, LUA_GLOBALSINDEX, c"bot".as_ptr());
                if lua_type(t, -1) != LUA_TFUNCTION {
                    lua_pop(t, 1);
                    return 0;
                }
                lua = true;
                lua_rawgeti(t, LUA_REGISTRYINDEX, act);
                out = pcall_on(l, t, 1, 0);
                0
            })?;
        }
        out?;
        let h = self.host_mut();
        let Some(mut b) = h.bot.take() else { return Ok((Vec::new(), false)) };
        let intents = if lua { std::mem::take(&mut b.intents) } else { crate::autoplay::policy(h, &mut b.auto, b.finger) };
        // The hints are followed when the policy says so (`act.followHints()`),
        // or with no policy of the game's and no quick-path player to play.
        let follow = std::mem::take(&mut b.follow_hints) || (!lua && h.quick.players().next().is_none());
        h.bot = Some(b);
        Ok((intents, follow))
    }

    /// What the game has for the debug menu: its kinds, their behaviours'
    /// knobs, and its `debug.*` items.
    pub fn menu(&self) -> crate::debugmenu::GameMenu {
        use crate::debugmenu::{GameMenu, ItemRow, KnobRow};
        let h = self.host();
        let q = &h.quick;
        let mut m = GameMenu { kinds: q.kinds.iter().map(|k| k.name.clone()).collect(), ..Default::default() };
        #[cfg(phys2)]
        if h.phys2.as_deref().is_some_and(|p| !p.worlds.is_empty()) {
            m.physics = true;
        }
        #[cfg(three)]
        if h.three.as_deref().is_some_and(|w| w.phys.is_some()) {
            m.physics = true;
        }
        for k in &q.kinds {
            for (knob, value, min, max, step) in q.kind_knobs(k) {
                m.knobs.push(KnobRow { kind: k.name.clone(), knob, value, min, max, step });
            }
        }
        m.items = h.debug_items.iter().map(|i| ItemRow { kind: i.kind.into(), label: i.label.clone(), value: i.value, min: i.min, max: i.max, step: i.step, options: i.options.clone() }).collect();
        m
    }

    /// Applies debug-menu picks to this round (between frames).
    #[allow(clippy::redundant_closure_call)] // a closure for `?`
    pub fn debug(&mut self, cmds: Vec<crate::debugmenu::DebugCmd>) -> Result<(), String> {
        use crate::debugmenu::DebugCmd as D;
        use crate::project::Scalar;
        let (l, t, host) = (self.l, self.t, self.host);
        for c in cmds {
            // Each pick under protection; the game's tables get raw sets, so
            // no metamethod of the game's runs from here.
            let mut out: Result<(), String> = Ok(());
            unsafe {
                protect(l, t, &mut |t| {
                    let h = &mut *host;
                    let mut run = || -> Result<(), String> {
                        match &c {
                            D::Score(v) | D::Lives(v) => {
                                lua_rawgetfield(t, LUA_GLOBALSINDEX, c"round".as_ptr());
                                if lua_type(t, -1) == LUA_TTABLE {
                                    lua_pushnumber(t, *v);
                                    lua_rawsetfield(t, -2, if matches!(c, D::Score(_)) { c"score".as_ptr() } else { c"lives".as_ptr() });
                                }
                                lua_pop(t, 1);
                            }
                            D::Invincible(on) => h.quick.invincible = *on,
                            D::Hitboxes(on) => h.show_hitboxes = *on,
                            D::Colliders(on) => {
                                #[cfg(phys2)]
                                if let Some(p) = h.phys2.as_deref_mut() {
                                    p.show = *on;
                                }
                                #[cfg(three)]
                                if let Some(w) = h.three.as_deref_mut() {
                                    w.phys().show = *on;
                                }
                            }
                            D::PausePhysics(on) => {
                                #[cfg(phys2)]
                                if let Some(p) = h.phys2.as_deref_mut() {
                                    p.paused = *on;
                                }
                                #[cfg(three)]
                                if let Some(w) = h.three.as_deref_mut() {
                                    w.phys().paused = *on;
                                }
                            }
                            D::Knob(kind, knob, v) => {
                                let k = h.quick.kind_mut(kind);
                                k.knobs.retain(|x| x.0 != *knob);
                                k.knobs.push((knob.clone(), *v));
                            }
                            D::Set(name, v) => {
                                let Ok(k) = std::ffi::CString::new(name.as_str()) else { return Ok(()) };
                                match v {
                                    Scalar::Num(n) => lua_pushnumber(t, *n),
                                    Scalar::Bool(b) => lua_pushboolean(t, *b as c_int),
                                    Scalar::Str(s) => push_str(t, s),
                                }
                                lua_rawsetfield(t, LUA_GLOBALSINDEX, k.as_ptr());
                            }
                            D::Spawn(kind) => {
                                let near = h.quick.players().next().and_then(|p| h.scene.get(p.node)).map(|n| (n.x, n.y - 120.0));
                                let (w, hh) = (h.scene.width, h.scene.height);
                                let (x, y) = near.unwrap_or_else(|| h.scene.camera.to_world(w, hh, w / 2.0, hh / 3.0));
                                crate::api::node::make_thing(t, h, kind, &[("x".into(), x as f64), ("y".into(), y as f64)])?;
                            }
                            D::Item(n, v) => {
                                let Some(it) = h.debug_items.get(n - 1).cloned() else { return Ok(()) };
                                if it.kind != "action" {
                                    h.debug_items[n - 1].value = *v;
                                }
                                lua_rawgeti(t, LUA_REGISTRYINDEX, it.f);
                                let nargs = match it.kind {
                                    "toggle" => {
                                        lua_pushboolean(t, (*v != 0.0) as c_int);
                                        1
                                    }
                                    "number" => {
                                        lua_pushnumber(t, *v);
                                        1
                                    }
                                    "choice" => {
                                        let i = (v.max(0.0) as usize).min(it.options.len().saturating_sub(1));
                                        push_str(t, it.options.get(i).map_or("", |s| s.as_str()));
                                        lua_pushnumber(t, i as f64 + 1.0);
                                        2
                                    }
                                    _ => 0,
                                };
                                pcall_on(l, t, nargs, 0)?;
                            }
                            D::Level(name) => {
                                lua_getfield(t, LUA_GLOBALSINDEX, c"nextLevel".as_ptr());
                                push_str(t, name);
                                pcall_on(l, t, 1, 0)?;
                            }
                        }
                        Ok(())
                    };
                    out = run();
                    0
                })?;
            }
            out?;
        }
        Ok(())
    }

    /// The Lua heap's limit, from now on (tests meet it with a small one).
    pub fn set_memory_limit(&mut self, bytes: usize) {
        unsafe { sv_set_mem_limit(self.l, bytes) }
    }

    /// Lua heap in use, in bytes.
    pub fn memory(&self) -> usize {
        unsafe { sv_mem_used(self.l) }
    }
}

/// Loads a chunk into thread `t` with its globals' imports left unresolved.
///
/// Luau resolves a chunk's imports (`round.score`, `camera.x`, any global's
/// field the chunk doesn't assign) when it's loaded into a "safe"
/// environment, and keeps those values for good: right for read-only
/// libraries, wrong for LÖVE code, where `round.score` changes and one file
/// reads a table another file changes. So chunks load with the
/// environment unsafe (imports stay unresolved and are looked up each
/// time), and it's safe again for running, so builtins keep their fast
/// paths (FASTCALL). Returns 0, or 1 with the error on the stack.
///
/// # Safety
///
/// `t` is a live thread of a VM made by `sv_new`.
pub unsafe fn load_game_chunk(t: *mut lua_State, name: *const c_char, bc: &[u8]) -> c_int {
    unsafe {
        lua_setsafeenv(t, LUA_GLOBALSINDEX, 0);
        let r = sv_load_bytecode(t, name, bc.as_ptr() as *const c_char, bc.len());
        lua_setsafeenv(t, LUA_GLOBALSINDEX, 1);
        r
    }
}

/// Loads a chunk into thread `t` (a VM's `l`): its error on failure.
unsafe fn load_chunk(l: *mut lua_State, t: *mut lua_State, name: &std::ffi::CStr, bc: &[u8]) -> Result<(), String> {
    unsafe {
        if load_game_chunk(t, name.as_ptr(), bc) != 0 {
            return Err(take_error(l, t));
        }
        Ok(())
    }
}

/// `lua_pcall` on thread `t`: its error on failure.
unsafe fn pcall_on(l: *mut lua_State, t: *mut lua_State, nargs: c_int, nres: c_int) -> Result<(), String> {
    unsafe {
        if lua_pcall(t, nargs, nres, 0) != LUA_OK {
            return Err(take_error(l, t));
        }
        Ok(())
    }
}

/// Runs `f` on `on` (a thread of `l`) in protected mode (`sv_protect`): a
/// raise from the Lua API it calls ends it with that error rather than
/// with no handler. `f` keeps no owned values across its Lua calls.
///
/// # Safety
///
/// `on` is `l` or a thread of it, with no Lua call running on it.
pub unsafe fn protect(l: *mut lua_State, on: *mut lua_State, f: &mut dyn FnMut(*mut lua_State) -> c_int) -> Result<c_int, String> {
    unsafe extern "C-unwind" fn call(s: *mut lua_State, ud: *mut c_void) -> c_int {
        let f = unsafe { &mut *(ud as *mut &mut dyn FnMut(*mut lua_State) -> c_int) };
        f(s)
    }
    unsafe {
        let mut f = f;
        let mut result = 0;
        if sv_protect(on, call, &mut f as *mut _ as *mut c_void, &mut result) != LUA_OK {
            return Err(take_error(l, on));
        }
        Ok(result)
    }
}

/// The frame's events as `{ name, args… }` tables for the prelude.
unsafe fn push_events(t: *mut lua_State, events: &[LoveEvent]) {
    unsafe {
        lua_createtable(t, events.len() as c_int, 0);
        for (i, e) in events.iter().enumerate() {
            lua_createtable(t, 7, 0);
            let mut k = 0;
            let mut s = |v: &str| {
                k += 1;
                push_str(t, v);
                lua_rawseti(t, -2, k);
            };
            let name = match e {
                LoveEvent::KeyPressed(_) => "keypressed",
                LoveEvent::KeyReleased(_) => "keyreleased",
                LoveEvent::MousePressed { .. } => "mousepressed",
                LoveEvent::MouseReleased { .. } => "mousereleased",
                LoveEvent::MouseMoved { .. } => "mousemoved",
                LoveEvent::TouchPressed { .. } => "touchpressed",
                LoveEvent::TouchMoved { .. } => "touchmoved",
                LoveEvent::TouchReleased { .. } => "touchreleased",
                LoveEvent::GamepadPressed(_) => "gamepadpressed",
                LoveEvent::GamepadReleased(_) => "gamepadreleased",
                LoveEvent::GamepadAxis(..) => "gamepadaxis",
                LoveEvent::WheelMoved(..) => "wheelmoved",
            };
            s(name);
            let mut k = 1;
            let mut n = |v: f64| {
                k += 1;
                lua_pushnumber(t, v);
                lua_rawseti(t, -2, k);
            };
            match *e {
                LoveEvent::KeyPressed(key) | LoveEvent::KeyReleased(key) | LoveEvent::GamepadPressed(key) | LoveEvent::GamepadReleased(key) => {
                    push_str(t, key);
                    lua_rawseti(t, -2, 2);
                }
                LoveEvent::GamepadAxis(axis, v) => {
                    push_str(t, axis);
                    lua_rawseti(t, -2, 2);
                    lua_pushnumber(t, v as f64);
                    lua_rawseti(t, -2, 3);
                }
                LoveEvent::MousePressed { x, y, button, istouch } | LoveEvent::MouseReleased { x, y, button, istouch } => {
                    n(x as f64);
                    n(y as f64);
                    n(button as f64);
                    lua_pushboolean(t, istouch as c_int);
                    lua_rawseti(t, -2, 5);
                }
                LoveEvent::MouseMoved { x, y, dx, dy, istouch } => {
                    for v in [x, y, dx, dy] {
                        n(v as f64);
                    }
                    lua_pushboolean(t, istouch as c_int);
                    lua_rawseti(t, -2, 6);
                }
                LoveEvent::TouchPressed { id, x, y, pressure } => {
                    for v in [id as f32, x, y, 0.0, 0.0, pressure] {
                        n(v as f64);
                    }
                }
                LoveEvent::TouchMoved { id, x, y, dx, dy, pressure } => {
                    for v in [id as f32, x, y, dx, dy, pressure] {
                        n(v as f64);
                    }
                }
                LoveEvent::TouchReleased { id, x, y } => {
                    for v in [id as f32, x, y, 0.0, 0.0, 1.0] {
                        n(v as f64);
                    }
                }
                LoveEvent::WheelMoved(x, y) => {
                    n(x as f64);
                    n(y as f64);
                }
            }
            lua_rawseti(t, -2, i as c_int + 1);
        }
    }
}
