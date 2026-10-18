//! The Studio engine: a Maimbrain game, built on the SDK like any other,
//! that plays a Studio project (docs/STUDIO.md "The Studio engine",
//! docs/STUDIO-API.md for the project format and the Lua API).
//!
//! The project ships in the bundle under `assets/` (docs/SPEC.md "Studio
//! bundles"): `mb build` writes `assets/files.txt` listing it, scales the
//! sprites and tiles up for crisp drawing and renders the synth sounds and
//! songs to Ogg. The engine loads them all, parses the project
//! (`project.rs`), and plays it with the SDK's `Rounds`, so the platform's
//! rules (the title card, a press starts the round, `Over`, the game-over
//! card, bests, a controller's A or Enter to start) are the engine's job and
//! a Studio game can't break them.
//!
//! Each round gets a fresh Lua VM (`vm.rs`), so a round depends only on the
//! project, its seed and the input; a title-screen VM, which never starts,
//! tells the debug menu what the game has. Lua runs in `mb_update` only.

pub mod debugmenu;
mod headless;
pub mod input;
pub mod project;
pub mod vm;

use std::collections::BTreeMap;
use std::rc::Rc;

use maimbrain::audio::{Bus, Sound, Voice};
use maimbrain::gfx2d::{self, Font, Image};
use maimbrain::juice::{Particles, Popups};
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::sys::{self, Asset, AssetState, Level, Screen};
use maimbrain::ui::{Theme, TitleCard};
use maimbrain::{Game, debug, export_game};

use project::{Orientation, Project, TILE};
use vm::{Fx, Mode, Step, Vm};

/// The engine's version, as a project pins it (`engine = "1.0"`) and the
/// manifest names it (`engine = "studio@1.0"`).
pub const ENGINE_VERSION: &str = "1.0";
/// Texels per art pixel in a bundle's sprites and tiles: `mb build` scales
/// them up 5× (nearest), so an art pixel drawn 2.5 logical units wide covers
/// whole canvas pixels (SPEC §5.3: images are sampled linearly, and the canvas
/// is drawn at 2 device pixels a unit).
pub const TEXELS: u32 = 5;
/// The generated list of the project's files in a bundle.
pub const INDEX: &str = "assets/files.txt";

/// Natively (`cargo test`) no asset loads (SPEC §9.8): tests hand the engine
/// a project's files instead; without one it plays the catch starter.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::RefCell;

    use super::project::Files;

    thread_local! {
        static FILES: RefCell<Option<Files>> = const { RefCell::new(None) };
    }

    /// The project `Studio::init` plays on this thread.
    pub fn use_project(files: Files) {
        FILES.with(|f| *f.borrow_mut() = Some(files));
    }

    pub fn files() -> Files {
        FILES.with(|f| f.borrow().clone()).unwrap_or_else(|| read_dir(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("starters/catch")))
    }

    /// A project directory's files, as `mb build` would ship them (without
    /// the rendered sounds or scaled art).
    pub fn read_dir(root: &std::path::Path) -> Files {
        let mut out = Files::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(b) = std::fs::read(&p) {
                    let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                    out.insert(rel, b);
                }
            }
        }
        out
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::{read_dir, use_project};

/// Loading the project's files from the bundle.
enum Loading {
    /// Waiting for `assets/files.txt`.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Index(Asset),
    /// Waiting for every file it lists.
    Files(Vec<(String, Asset)>),
    Done,
}

/// Shake, flash, particles and popups: output only, drawn over the world.
struct Feel {
    shake: f32,
    flash: Option<(u8, f32)>,
    particles: Particles,
    popups: Popups,
}

pub struct Studio {
    loading: Loading,
    project: Option<Rc<Project>>,
    /// Per sprite, then per tile set: the image, once loaded.
    sprite_images: Vec<Option<Image>>,
    tile_images: Vec<Option<Image>>,
    assets: BTreeMap<String, Asset>,
    sounds: BTreeMap<String, Sound>,
    music: Option<(String, Voice)>,
    rounds: Rounds,
    /// The title screen's VM: what the debug menu shows.
    title: Option<Vm>,
    menu: vm::Menu,
    round: Option<Vm>,
    last: Option<Step>,
    /// The round's score, lives and level after its last frame, read in
    /// `update`: `render` never touches the VM (a global lookup there would
    /// allocate in the Lua heap, and a replay may not render).
    hud: (f64, f64, Option<String>),
    error: Option<String>,
    input: input::Mapper,
    debug: debugmenu::State,
    feel: Feel,
    screen: Screen,
    frames: u64,
    /// `mb play`'s debug parameters (headless.rs).
    hooks: headless::Hooks,
}

impl Studio {
    fn project(&self) -> Option<&Rc<Project>> {
        self.project.as_ref()
    }

    /// The project's files are all here: parse it, set up the title.
    fn loaded(&mut self, files: project::Files) {
        match Project::parse(&files) {
            Ok(p) => {
                let p = Rc::new(p);
                let theme = theme(&p);
                let mut title = TitleCard::new(&p.title.to_uppercase(), theme);
                if let Some(t) = &p.tagline {
                    title = title.tagline(t);
                }
                let mut rounds = Rounds::new(title).heading(&p.over.to_uppercase()).label(&p.label.to_uppercase());
                if p.lower_is_better {
                    rounds = rounds.lower_is_better();
                }
                // Keep where the title card is (a session's rounds share its age).
                self.rounds = rounds;
                self.sprite_images = vec![None; p.sprites.len()];
                self.tile_images = vec![None; p.tilesets.len()];
                self.project = Some(p.clone());
                match Vm::new(&p, sys::rand_seed(), Mode::Title) {
                    Ok(v) => {
                        self.menu = v.menu();
                        self.title = Some(v);
                    }
                    Err(e) => self.fail(e),
                }
            }
            Err(e) => self.fail(e),
        }
        self.loading = Loading::Done;
    }

    fn fail(&mut self, msg: String) {
        sys::log(Level::Error, &format!("studio: {msg}"));
        self.error = Some(msg);
    }

    fn start_round(&mut self) {
        self.round = None;
        self.last = None;
        self.hud = (0.0, 0.0, None);
        self.input.reset();
        self.feel.particles.clear();
        self.feel.popups.list.clear();
        let Some(p) = self.project.clone() else {
            if self.error.is_none() {
                self.error = Some("the project hasn't loaded yet".into());
            }
            return;
        };
        if self.title.is_none() {
            return; // the error from loading stays up
        }
        self.error = None;
        match Vm::new(&p, self.rounds.seed(), Mode::Round { level: self.debug.start_level.as_deref() }) {
            Ok(v) => {
                if let Some(n) = sys::param_as::<u64>("lua_budget") {
                    v.set_budget(n);
                }
                self.debug.round_started();
                self.round = Some(v);
            }
            Err(msg) => self.fail(msg),
        }
    }

    fn play_fx(&mut self, fx: Vec<Fx>, camx: f32, camy: f32) {
        let Some(p) = self.project.clone() else { return };
        let (ox, oy, s) = self.transform(&p);
        let to_screen = |x: f32, y: f32| (ox + (x - camx) * s, oy + (y - camy) * s);
        for f in fx {
            match f {
                Fx::Sfx(name) => match self.sounds.get(&name) {
                    Some(snd) => {
                        snd.play(1.0, 0.0, 1.0);
                    }
                    None if !p.sounds.contains(&name) => self.warn_once(format!("sfx(\"{name}\"): there's no sounds/{name}.sfx.toml or .ogg")),
                    None => {}
                },
                Fx::Music(name) => {
                    if self.music.as_ref().map(|m| &m.0) == name.as_ref() {
                        continue;
                    }
                    if let Some((_, v)) = self.music.take() {
                        v.stop_at(sys::time(), 0.3);
                    }
                    if let Some(n) = name {
                        match self.sounds.get(&n) {
                            Some(snd) => self.music = Some((n, snd.play_looped(0.7, 0.0, 1.0).bus(Bus::Music))),
                            None if !p.music.contains(&n) => self.warn_once(format!("music(\"{n}\"): there's no music/{n}.song.toml or .ogg")),
                            None => {}
                        }
                    }
                }
                Fx::Shake(n) => self.feel.shake = self.feel.shake.max(n.clamp(0.0, 16.0)),
                Fx::Flash(c) => self.feel.flash = Some((c, 0.25)),
                Fx::Particles(kind, x, y) => {
                    let (sx, sy) = to_screen(x, y);
                    let c = p.color(7);
                    let parts = &mut self.feel.particles;
                    match kind.as_str() {
                        "sparkles" => parts.sparkles(sx, sy, 20.0, 12, c),
                        "confetti" => parts.confetti(sx, sy, 24, &[p.color(3), p.color(5), p.color(8), p.color(11), p.color(14)]),
                        "stars" => parts.stars(sx, sy, 8, p.color(5)),
                        "ring" => parts.ring(sx, sy, 24.0, c),
                        _ => parts.burst(sx, sy, 14, c),
                    }
                }
                Fx::Popup(text, x, y) => {
                    let (sx, sy) = to_screen(x, y);
                    self.feel.popups.spawn(sx, sy, &text, p.color(7), 16.0);
                }
            }
        }
    }

    fn warn_once(&mut self, msg: String) {
        if self.debug.warned.insert(msg.clone()) {
            sys::log(Level::Warn, &msg);
        }
    }

    /// Where the canvas sits on the screen: offset and logical units per art pixel.
    fn transform(&self, p: &Project) -> (f32, f32, f32) {
        let (w, h) = (p.canvas.0 as f32, p.canvas.1 as f32);
        let (sw, sh) = screen_size(&self.screen, p);
        let s = (sw / w).min(sh / h);
        ((sw - w * s) / 2.0, (sh - h * s) / 2.0, s)
    }

    /// A sideways game as an upright feed card (not in a round).
    fn card_mode(&self, p: &Project) -> bool {
        let (sw, sh) = screen_size(&self.screen, p);
        p.orientation == Orientation::Sideways && sh > sw && !self.rounds.playing()
    }
}

/// The screen's logical size (natively, where there's no screen, the manifest's).
fn screen_size(s: &Screen, p: &Project) -> (f32, f32) {
    if s.width > 0.0 && s.height > 0.0 {
        (s.width, s.height)
    } else {
        let (w, h) = logical_size(p.orientation);
        (w as f32, h as f32)
    }
}

/// The manifest's `logical_size`: the canvas at 2.5 units an art pixel.
pub fn logical_size(o: Orientation) -> (u32, u32) {
    match o {
        Orientation::Portrait => (360, 640),
        Orientation::Sideways => (640, 360),
    }
}

/// The SDK's arcade theme, in the project's colours.
fn theme(p: &Project) -> Theme {
    let mut t = Theme::arcade();
    t.bg_top = p.color(p.background);
    t.bg_bottom = p.color(0);
    t
}

impl Game for Studio {
    fn init() -> Self {
        let rounds = Rounds::new(TitleCard::new("", Theme::arcade()));
        let mut s = Studio {
            loading: Loading::Done,
            project: None,
            sprite_images: Vec::new(),
            tile_images: Vec::new(),
            assets: BTreeMap::new(),
            sounds: BTreeMap::new(),
            music: None,
            rounds,
            title: None,
            menu: vm::Menu::default(),
            round: None,
            last: None,
            hud: (0.0, 0.0, None),
            error: None,
            input: input::Mapper::default(),
            debug: debugmenu::State::default(),
            feel: Feel { shake: 0.0, flash: None, particles: Particles::new(sys::rand_seed() ^ 0x5354_5544), popups: Popups::new() },
            screen: sys::screen(),
            frames: 0,
            hooks: headless::Hooks::from_params(),
        };
        #[cfg(target_arch = "wasm32")]
        {
            s.loading = Loading::Index(Asset::load(INDEX));
        }
        #[cfg(not(target_arch = "wasm32"))]
        s.loaded(native::files());
        if let Some(p) = s.project.clone() {
            s.debug.show(&p, &s.menu, None, false);
        }
        s
    }

    fn update(&mut self, dt: f32) {
        self.frames += 1;
        self.load();
        for e in maimbrain::input::poll() {
            if let Some(pick) = debug::Pick::from(&e) {
                if let Some(p) = self.project.clone() {
                    self.debug.pick(&p, &self.menu, pick, self.rounds.playing());
                }
                continue;
            }
            if e.is_resize() {
                self.screen = sys::screen();
            }
            let tf = self.project.clone().map(|p| self.transform(&p));
            match self.rounds.input(&e) {
                Input::Start => {
                    self.start_round();
                    if let Some(t) = tf {
                        self.input.feed(&e, t);
                    }
                }
                Input::Play => {
                    if let Some(t) = tf {
                        self.input.feed(&e, t);
                    }
                }
                Input::Taken | Input::Extra => {}
            }
        }
        #[cfg(feature = "sensors")]
        self.input.tilt(maimbrain::sensors::tilt().map(|t| (t.x * 2.0).clamp(-1.0, 1.0)));

        // The world runs until the round ends (its ending shows the moment it
        // did, frozen), and never after a Lua error.
        if self.rounds.stage() == Stage::Playing {
            if let Some(v) = &mut self.round {
                let mut frame = self.input.frame();
                frame.debug = self.debug.take_commands();
                let r = v.step(dt, &frame);
                let score = v.number("score").round() as i64;
                match r {
                    Err(msg) => {
                        self.fail(msg);
                        self.rounds.end(score);
                    }
                    Ok(step) => {
                        let fx = std::mem::take(&mut v.out.borrow_mut().fx);
                        let (cx, cy) = (step.camx, step.camy);
                        if step.over {
                            match &step.heading {
                                Some(h) => {
                                    self.rounds.end(score).heading(&h.to_uppercase());
                                }
                                None => {
                                    self.rounds.end(score);
                                }
                            }
                        }
                        self.hud = (v.number("score"), v.number("lives"), v.level());
                        self.last = Some(step);
                        self.play_fx(fx, cx, cy);
                    }
                }
            } else if self.rounds.stage() == Stage::Playing {
                // Nothing to play (the project didn't load): the round ends.
                self.rounds.end(0);
            }
        } else {
            // Picks made between rounds wait for the next one.
            let _ = self.input.frame();
        }
        if let Some(p) = self.project.clone() {
            let live = self.round.as_ref().filter(|_| self.rounds.playing()).map(|v| (v.number("score"), v.number("lives")));
            self.debug.show(&p, &self.menu, live, self.rounds.playing());
        }
        self.feel.shake = (self.feel.shake - dt * 24.0).max(0.0);
        if let Some((c, t)) = self.feel.flash {
            self.feel.flash = (t > dt).then_some((c, t - dt));
        }
        self.feel.particles.update(dt);
        self.feel.popups.update(dt);
        self.rounds.update(dt);
        self.headless();
    }

    fn render(&self) {
        let Some(p) = self.project() else {
            gfx2d::clear(0x0b0d1fff);
            if let Some(e) = &self.error {
                error_box(e, 360.0);
            }
            self.rounds.draw();
            return;
        };
        let (sw, sh) = screen_size(&self.screen, p);
        gfx2d::rect(0.0, 0.0, sw, sh, darken(p.color(0)));
        if self.card_mode(p) {
            self.draw_card(p, sw, sh);
            self.rounds.draw();
            return;
        }
        let (ox, oy, s) = self.transform(p);
        let (w, h) = (p.canvas.0 as f32, p.canvas.1 as f32);
        let shake = if self.feel.shake > 0.0 {
            let f = self.frames as f32;
            ((f * 1.7).sin() * self.feel.shake).round() * s
        } else {
            0.0
        };
        gfx2d::push();
        gfx2d::translate(ox + shake, oy);
        gfx2d::scale(s, s);
        let live = self.round.as_ref().zip(self.last.as_ref());
        let level = live.and_then(|_| self.hud.2.clone()).or_else(|| p.maps.first().map(|m| m.name.clone()));
        let map = level.as_deref().and_then(|l| p.map(l));
        let bg = map.and_then(|m| m.background).unwrap_or(p.background);
        gfx2d::rect(0.0, 0.0, w, h, p.color(bg));
        let (camx, camy) = live.map_or((0.0, 0.0), |(_, st)| (st.camx, st.camy));
        if let Some(m) = map {
            self.draw_map(p, m, camx, camy);
        }
        match live {
            Some((v, _)) => {
                let out = v.out.borrow();
                for t in &out.things {
                    match t {
                        Ok(spr) => self.draw_sprite(p, spr.sprite, spr.frame, spr.x - camx, spr.y - camy, spr.flip),
                        Err(sh) => {
                            let (x, y) = ((sh.x - sh.w / 2.0 - camx).round(), (sh.y - sh.h / 2.0 - camy).round());
                            if sh.circle {
                                gfx2d::circle(x + sh.w / 2.0, y + sh.h / 2.0, sh.w / 2.0, p.color(sh.color));
                            } else {
                                gfx2d::rect(x, y, sh.w, sh.h, p.color(sh.color));
                            }
                        }
                    }
                }
                for b in &out.boxes {
                    outline(b[0] - b[2] / 2.0 - camx, b[1] - b[3] / 2.0 - camy, b[2], b[3], 0xff3b6bff);
                }
            }
            None => {
                // The title: the first level as it starts, things where the map puts them.
                if let Some(m) = map {
                    for sp in m.spawns.iter().filter(|s| s.every.is_none()) {
                        if let Some(si) = p.sprites.iter().position(|s| s.name == sp.kind) {
                            let s = &p.sprites[si];
                            let (hx, hy, hw, hh) = s.hitbox;
                            let x = sp.x - (hx + hw / 2.0);
                            let y = sp.y - (hy + hh / 2.0);
                            self.draw_sprite(p, si, s.anims[0].1[0], x, y, false);
                        }
                    }
                }
            }
        }
        if let Some((c, t)) = self.feel.flash {
            gfx2d::rect(0.0, 0.0, w, h, (p.color(c) & 0xffffff00) | ((t / 0.25 * 160.0) as u32).min(255));
        }
        gfx2d::pop();
        self.feel.particles.draw();
        self.feel.popups.draw();
        if let Some((v, _)) = live {
            for t in &v.out.borrow().texts {
                gfx2d::text(Font::Pixel, 16.0, (ox + t.x * s).round(), (oy + t.y * s).round(), p.color(t.color), &t.s);
            }
            if self.rounds.playing() {
                let top = oy + self.screen.inset_top.max(0.0) + 16.0;
                let score = self.hud.0.round() as i64;
                gfx2d::text_centered(Font::Pixel, 32.0, sw / 2.0, top, 0xffffffff, &score.to_string());
                let lives = self.hud.1.max(0.0) as usize;
                if p.lives > 1 && lives > 0 {
                    gfx2d::text(Font::Pixel, 16.0, ox + 12.0, top + 8.0, p.color(3) | 0xff, &"♥".repeat(lives.min(9)));
                }
            }
        }
        if let Some(e) = &self.error {
            error_box(e, sw);
        }
        self.rounds.draw();
    }
}

impl Studio {
    /// The project's files, as they arrive.
    fn load(&mut self) {
        match &self.loading {
            Loading::Index(a) => match a.state() {
                AssetState::Ready => {
                    let list = a.read().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                    let files: Vec<(String, Asset)> = list
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(|l| (l.to_string(), Asset::load(&format!("assets/{l}"))))
                        .collect();
                    self.loading = Loading::Files(files);
                }
                AssetState::Failed => {
                    self.loading = Loading::Done;
                    self.fail(format!("{INDEX} didn't load (build Studio projects with `mb build`)"));
                }
                AssetState::Pending => {}
            },
            Loading::Files(list) => {
                if list.iter().any(|(_, a)| a.state() == AssetState::Pending) {
                    return;
                }
                let mut files = project::Files::new();
                let list = list.clone();
                for (path, a) in &list {
                    match a.state() {
                        AssetState::Ready => {
                            files.insert(path.clone(), a.read().unwrap_or_default());
                            self.assets.insert(path.clone(), *a);
                        }
                        _ => self.warn_once(format!("assets/{path} didn't load")),
                    }
                }
                self.loaded(files);
            }
            Loading::Done => {}
        }
        // Images and sounds decode once their assets are ready (already, on a device).
        if let Some(p) = self.project.clone() {
            for (i, s) in p.sprites.iter().enumerate() {
                if self.sprite_images[i].is_none()
                    && let Some(a) = s.png.as_ref().and_then(|png| self.assets.get(png))
                {
                    self.sprite_images[i] = Image::new(*a);
                }
            }
            for (i, t) in p.tilesets.iter().enumerate() {
                if self.tile_images[i].is_none()
                    && let Some(a) = t.png.as_ref().and_then(|png| self.assets.get(png))
                {
                    self.tile_images[i] = Image::new(*a);
                }
            }
            for (dir, names) in [("sounds", &p.sounds), ("music", &p.music)] {
                for n in names {
                    if !self.sounds.contains_key(n)
                        && let Some(a) = self.assets.get(&format!("{dir}/{n}.ogg"))
                        && let Some(snd) = Sound::new(*a)
                    {
                        self.sounds.insert(n.clone(), snd);
                    }
                }
            }
        }
    }

    fn draw_sprite(&self, p: &Project, si: usize, frame: u32, x: f32, y: f32, flip: bool) {
        let s = &p.sprites[si];
        let (fw, fh) = (s.frame.0 as f32, s.frame.1 as f32);
        let (x, y) = (x.round(), y.round());
        match self.sprite_images.get(si).copied().flatten() {
            Some(img) => {
                let k = s.scale as f32;
                let src = [frame as f32 * fw * k, 0.0, fw * k, fh * k];
                if flip {
                    gfx2d::push();
                    gfx2d::translate(x + fw, y);
                    gfx2d::scale(-1.0, 1.0);
                    gfx2d::sprite(img, src, [0.0, 0.0, fw, fh], 0xffffffff);
                    gfx2d::pop();
                } else {
                    gfx2d::sprite(img, src, [x, y, fw, fh], 0xffffffff);
                }
            }
            None => {
                // No image (natively, or still decoding): the hitbox in a colour.
                let (hx, hy, hw, hh) = s.hitbox;
                gfx2d::rect(x + hx, y + hy, hw, hh, p.color(s.shape.map_or(7, |sh| sh.color)));
            }
        }
    }

    fn draw_map(&self, p: &Project, m: &project::Map, camx: f32, camy: f32) {
        let t = TILE as f32;
        let (w, h) = (p.canvas.0 as f32, p.canvas.1 as f32);
        let (cx0, cy0) = ((camx / t).floor() as i64, (camy / t).floor() as i64);
        let (cx1, cy1) = (((camx + w) / t).ceil() as i64, ((camy + h) / t).ceil() as i64);
        for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                let Some((si, ti)) = m.cell(cx, cy) else { continue };
                let set = &p.tilesets[si as usize];
                let tile = &set.tiles[ti as usize];
                let (x, y) = ((cx as f32 * t - camx).round(), (cy as f32 * t - camy).round());
                match self.tile_images.get(si as usize).copied().flatten() {
                    Some(img) => {
                        let k = set.scale as f32 * t;
                        gfx2d::sprite(img, [tile.frame as f32 * k, 0.0, k, k], [x, y, t, t], 0xffffffff);
                    }
                    None => gfx2d::rect(x, y, t, t, p.color(set.color)),
                }
            }
        }
    }

    /// A sideways game's upright feed card: the hero, big, over the
    /// game's colours; the title card (title, best, prompt) goes on top.
    fn draw_card(&self, p: &Project, sw: f32, sh: f32) {
        gfx2d::rect_gradient(0.0, 0.0, sw, sh, p.color(p.background), p.color(0));
        // A strip of the first level's ground along the bottom.
        if let Some(m) = p.maps.first() {
            let t = TILE as f32 * 2.5;
            gfx2d::push();
            gfx2d::translate(0.0, sh - m.h as f32 * t);
            gfx2d::scale(2.5, 2.5);
            self.draw_map(p, m, 0.0, 0.0);
            gfx2d::pop();
        }
        if let Some(hero) = p.hero() {
            let si = p.sprites.iter().position(|s| s.name == hero.name).unwrap_or(0);
            let k = (sw * 0.45 / hero.frame.0 as f32).floor().max(1.0);
            gfx2d::push();
            gfx2d::translate((sw - hero.frame.0 as f32 * k) / 2.0, sh * 0.52 - hero.frame.1 as f32 * k / 2.0);
            gfx2d::scale(k, k);
            self.draw_sprite(p, si, hero.anims[0].1[0], 0.0, 0.0, false);
            gfx2d::pop();
        }
    }
}

fn darken(c: u32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xff) * 3 / 5) << s;
    ch(24) | ch(16) | ch(8) | 0xff
}

fn outline(x: f32, y: f32, w: f32, h: f32, c: u32) {
    let t = 0.5;
    gfx2d::rect(x, y, w, t, c);
    gfx2d::rect(x, y + h - t, w, t, c);
    gfx2d::rect(x, y, t, h, c);
    gfx2d::rect(x + w - t, y, t, h, c);
}

/// A Lua error, or a problem in the project, over the game.
fn error_box(e: &str, sw: f32) {
    let text = wrap(e, 40);
    let lines = text.lines().count() as f32;
    gfx2d::rect(8.0, 8.0, sw - 16.0, 24.0 + lines * 10.0, 0x000000d0);
    gfx2d::text(Font::Pixel, 8.0, 16.0, 16.0, 0xff6060ff, &text);
}

fn wrap(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut col = 0;
    for c in s.chars() {
        if c == '\n' {
            col = 0;
        } else if col == n {
            out.push('\n');
            col = 0;
        }
        out.push(c);
        col += 1;
    }
    out
}

#[cfg(test)]
impl Studio {
    /// Where the round last drew a kind of thing (the centre of its frame).
    fn thing_at(&self, kind: &str) -> (f32, f32) {
        let p = self.project.as_ref().unwrap();
        let si = p.sprites.iter().position(|s| s.name == kind).unwrap();
        let v = self.round.as_ref().expect("a round");
        let out = v.out.borrow();
        let s = out.things.iter().filter_map(|t| t.as_ref().ok()).find(|t| t.sprite == si).expect("drawn");
        (s.x + p.sprites[si].frame.0 as f32 / 2.0, s.y + p.sprites[si].frame.1 as f32 / 2.0)
    }
    fn thing_x(&self, kind: &str) -> f32 {
        self.thing_at(kind).0
    }
    fn thing_y(&self, kind: &str) -> f32 {
        self.thing_at(kind).1
    }
}

export_game!(Studio);

#[cfg(test)]
mod tests;
