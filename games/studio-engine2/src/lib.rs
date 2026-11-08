//! Studio engine 2.0 (docs/STUDIO-2.md): a Maimbrain game, built on the SDK
//! like any other, that plays a Studio project written as LÖVE-style Lua
//! (§5) on Luau (§4.13), over the SDK's retained scene (`maimbrain::scene`).
//!
//! Engine 1.0 (games/studio-engine, omnilua) stays as it is and keeps
//! playing every 1.0 project; this is its own crate so 1.0's published
//! builds and their source never change (§10.2).
//!
//! The project ships in the bundle under `assets/` (SPEC "Studio bundles"):
//! `mb build` writes `assets/files.txt` listing it, compiles each
//! `game/*.lua` to Luau bytecode beside it (the engine has no compiler,
//! Decided 36), and writes `packed.toml` with what it knows about the files
//! (images' sizes). The engine loads them, parses the project
//! (`project.rs`), and plays it with the SDK's `Rounds`, so the platform's
//! rules (the title card, a press starts the round, the game-over card,
//! bests) are the engine's and a game can't break them.
//!
//! Each round is a fresh VM (`vm.rs`), so a round depends only on the
//! project, its seed and the input. The title card plays the game as an
//! attract mode (a VM with `round.title` true and no input), and the
//! round's world keeps stepping under the game-over card (§4.7). Lua runs
//! in `mb_update` only: `love.draw` is recorded there and drawn in
//! `mb_render`.

pub mod api;
pub mod autoplay;
pub mod boards;
pub mod bot;
pub mod debugmenu;
pub mod identity;
mod headless;
pub mod input;
pub mod libs;
pub mod project;
pub mod quick;
#[cfg(three)]
pub mod three;
pub mod vm;

use std::rc::Rc;

use maimbrain::audio::{Bus, Sound, Voice};
use maimbrain::gfx2d::{self, Font, Image};
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::scene::{SceneImage, View};
use maimbrain::sys::{self, Asset, AssetState, Level, Screen};
use maimbrain::ui::{Theme, TitleCard};
use maimbrain::{Game, export_game};

use api::audio::Music;
use project::{Files, Project};
use vm::{Mode, Vm};

/// The engine's version, as a project pins it (`engine = "2.0"`) and the
/// manifest names it (`engine = "studio@2.0"`).
pub const ENGINE_VERSION: &str = "2.0";
/// The generated list of the project's files in a bundle.
pub const INDEX: &str = "assets/files.txt";

/// Natively (`cargo test`) no asset loads (SPEC §9.8): tests hand the
/// engine a project's files, with its Lua compiled as `mb build` would.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::RefCell;

    use super::project::Files;

    thread_local! {
        static FILES: RefCell<Option<Files>> = const { RefCell::new(None) };
    }

    /// The project `Studio2::init` plays on this thread.
    pub fn use_project(files: Files) {
        FILES.with(|f| *f.borrow_mut() = Some(files));
    }

    pub fn files() -> Files {
        FILES.with(|f| f.borrow().clone()).unwrap_or_default()
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::use_project;

/// Loading the project's files from the bundle.
enum Loading {
    /// Waiting for `assets/files.txt`.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Index(Asset),
    /// Waiting for every file it lists.
    Files(Vec<(String, Asset)>),
    Done,
}

/// An image file: its asset, its decoded handle, how it's sampled.
struct ImageFile {
    path: String,
    asset: Option<Asset>,
    image: Option<Image>,
    nearest: bool,
}

/// Files the engine reads as bytes; the rest (pictures, sounds) the host
/// decodes from their assets.
fn is_text(path: &str) -> bool {
    [".lua", ".luac", ".toml", ".txt", ".map", ".json", ".csv", ".wgsl"].iter().any(|x| path.ends_with(x))
}

pub struct Studio2 {
    loading: Loading,
    project: Option<Rc<Project>>,
    /// The session's images (assets outlive VMs, §4.7): each VM's scene
    /// starts with these, and gets their handles as they decode.
    images: Vec<SceneImage>,
    /// The files they're drawn from (an image's own, or an atlas page
    /// several share), and which file each image is.
    files: Vec<ImageFile>,
    image_file: Vec<usize>,
    /// The project's sounds and music loops, as they decode (by project index).
    sounds: Vec<(Option<Asset>, Option<Sound>)>,
    music: Vec<(Option<Asset>, Option<Sound>)>,
    /// Each voice line's sound (by `Project::voices`), decoded only once a
    /// round wants it (Decided 62: a line ahead, not the whole script).
    voices: Vec<(Option<Asset>, Option<Sound>)>,
    /// A 3D game's models, loaded once (§4.7): each VM spawns them.
    models: Vec<Option<maimbrain::gfx3d::Model>>,
    /// The loop playing (it carries from round to round), and one asked for
    /// before it had loaded.
    playing: Option<(String, Voice)>,
    wanted_music: Option<(String, f32, f32)>,
    rounds: Rounds,
    /// DAILY and SCORES on the cards, for a project with a daily board (boards.rs).
    daily: Option<boards::DailyUi>,
    /// The title card's attract mode, until a round starts.
    title: Option<Vm>,
    /// The round being played, then the world under its game-over card.
    round: Option<Vm>,
    /// The last game-over card's heading.
    card: Option<String>,
    /// `ui.identity()` has named another identity than studio.toml's (so an
    /// Arcade game's HUD is that identity's, not 1.0's).
    identity_changed: bool,
    error: Option<String>,
    input: input::Mapper,
    screen: Screen,
    frames: u64,
    hooks: headless::Hooks,
    /// The identity's theme, and the HUD drawn in it.
    theme: Theme,
    hud: maimbrain::ui::Hud,
    hud_spec: Option<(api::ui::HudSpec, f32, Option<String>)>,
    /// The debug menu (debugmenu.rs) and what the game has for it.
    debug: debugmenu::State,
    menu: debugmenu::GameMenu,
    /// The difficulty bot, in a bot run (`studio_bot`, bot.rs).
    bot: Option<bot::Bot>,
    /// Input held while the title card waits for a terrain's first view
    /// (it plays in order once built), and how many frames it has waited.
    held_input: Vec<maimbrain::input::Event>,
    held_frames: u32,
}

/// The most frames the title card waits for a terrain (it's built in about
/// ten; a terrain the host refuses meshes for counts as built).
const HOLD_FRAMES: u32 = 90;

impl Studio2 {
    /// The project's files are all here: parse it, start the title.
    fn loaded(&mut self, files: Files, assets: &[(String, Asset)]) {
        #[cfg(three)]
        api::world::loaded_files(assets);
        match Project::parse(&files) {
            Ok(p) => {
                let p = Rc::new(p);
                let theme = theme(&p);
                sys::log(Level::Info, &format!("identity: {}", p.identity.label()));
                self.theme = theme;
                self.identity_changed = false;
                self.hud = maimbrain::ui::Hud::new(theme, 0);
                let mut title = TitleCard::new(&p.title.to_uppercase(), theme);
                if let Some(t) = &p.tagline {
                    title = title.tagline(t);
                }
                let mut rounds = Rounds::new(title).heading(&p.over.to_uppercase()).label(&p.label.to_uppercase());
                if p.lower_is_better {
                    rounds = rounds.lower_is_better();
                }
                // The daily board (boards.rs): DAILY on the cards plays it.
                self.daily = boards::daily(&p.boards).map(boards::DailyUi::new);
                if let Some(d) = &self.daily {
                    rounds = rounds.daily(d.board);
                }
                self.rounds = rounds;
                self.images = p.images.iter().map(scene_image).collect();
                let audio = |dir: &str, list: &[(String, f32)]| -> Vec<(Option<Asset>, Option<Sound>)> {
                    list.iter().map(|(n, _)| (assets.iter().find(|(p, _)| *p == format!("{dir}/{n}.ogg")).map(|a| a.1), None)).collect()
                };
                self.sounds = audio("sounds", &p.sounds);
                self.music = audio("music", &p.music);
                self.voices = p.voices.iter().map(|v| (assets.iter().find(|(path, _)| *path == format!("voices/{}/{}.ogg", v.who, v.line)).map(|a| a.1), None)).collect();
                api::music::SESSION_SOUNDS.with(|s| *s.borrow_mut() = vec![None; p.music.len()]);
                self.models = vec![None; p.models.len()];
                #[cfg(three)]
                for (i, m) in p.models.iter().enumerate() {
                    #[cfg(target_arch = "wasm32")]
                    let asset = assets.iter().find(|(path, _)| *path == format!("models/{m}.glb")).map(|a| a.1);
                    // Natively nothing loads (SPEC §9.8): every host call answers 1.
                    #[cfg(not(target_arch = "wasm32"))]
                    let asset = Some(Asset(1));
                    self.models[i] = asset.and_then(maimbrain::gfx3d::Model::load);
                    if self.models[i].is_none() {
                        sys::log(Level::Warn, &format!("models/{m}.glb didn't load"));
                    }
                }
                self.files.clear();
                self.image_file.clear();
                for i in &p.images {
                    let f = match self.files.iter().position(|f| f.path == i.path) {
                        Some(f) => f,
                        None => {
                            let asset = assets.iter().find(|(path, _)| *path == i.path).map(|a| a.1);
                            self.files.push(ImageFile { path: i.path.clone(), asset, image: None, nearest: false });
                            self.files.len() - 1
                        }
                    };
                    self.image_file.push(f);
                }
                self.project = Some(p.clone());
                self.decode_images();
                self.decode_sounds();
                let sounds: Vec<Option<Sound>> = self.sounds.iter().map(|s| s.1).collect();
                match Vm::new(&p, sys::rand_seed(), Mode::Title, &self.images, &sounds, &self.models, self.safe_area()) {
                    Ok(v) => {
                        self.menu = v.menu();
                        self.title = Some(v);
                    }
                    Err(e) => self.fail(e),
                }
                self.debug.show(&p, &self.menu, None, false);
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
        // The music on stage carries into the round, starting again on its clock (api/music.rs).
        let carried = match (&mut self.round, &mut self.title) {
            (Some(v), _) | (None, Some(v)) => v.hand_over_music(),
            _ => None,
        };
        self.round = None;
        self.title = None;
        self.input.reset();
        let Some(p) = self.project.clone() else { return };
        self.error = None;
        let sounds: Vec<Option<Sound>> = self.sounds.iter().map(|s| s.1).collect();
        // A daily round is on the day's seed (Rounds picked it) and says so (`round.daily`).
        boards::set_next_daily(self.rounds.is_daily());
        match Vm::new(&p, self.rounds.seed(), Mode::Round, &self.images, &sounds, &self.models, self.safe_area()) {
            Ok(mut v) => {
                v.carry_music(carried);
                self.hud.reset(self.rounds.best());
                // Everything picked so far applies to the new round.
                self.debug.round_started();
                if let Err(msg) = v.debug(self.debug.take_commands()) {
                    self.fail(msg);
                }
                if let Some(b) = &mut self.bot {
                    b.new_round(sys::time());
                }
                self.round = Some(v);
            }
            Err(msg) => self.fail(msg),
        }
    }

    /// Where the canvas sits on the screen: offset and units per canvas unit.
    fn transform(&self) -> (f32, f32, f32) {
        let Some(p) = &self.project else { return (0.0, 0.0, 1.0) };
        let (w, h) = (p.size.0 as f32, p.size.1 as f32);
        let (sw, sh) = self.screen_size();
        let s = (sw / w).min(sh / h);
        ((sw - w * s) / 2.0, (sh - h * s) / 2.0, s)
    }

    fn screen_size(&self) -> (f32, f32) {
        if self.screen.width > 0.0 && self.screen.height > 0.0 {
            (self.screen.width, self.screen.height)
        } else {
            self.project.as_ref().map_or((360.0, 640.0), |p| (p.size.0 as f32, p.size.1 as f32))
        }
    }

    /// The screen's safe area in canvas units (`love.window.getSafeArea`).
    fn safe_area(&self) -> (f32, f32, f32, f32) {
        let Some(p) = &self.project else { return (0.0, 0.0, 360.0, 640.0) };
        let (w, h) = (p.size.0 as f32, p.size.1 as f32);
        let (ox, oy, s) = self.transform();
        let (sw, sh) = self.screen_size();
        let sc = &self.screen;
        let x0 = ((sc.inset_left.max(0.0) - ox) / s).clamp(0.0, w);
        let y0 = ((sc.inset_top.max(0.0) - oy) / s).clamp(0.0, h);
        let x1 = ((sw - sc.inset_right.max(0.0) - ox) / s).clamp(0.0, w);
        let y1 = ((sh - sc.inset_bottom.max(0.0) - oy) / s).clamp(0.0, h);
        (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// Images decode once their assets are ready (already, on a device);
    /// live scenes get the handles. Then each file is sampled as the
    /// on-stage VM wants its images (`pixel = true`, `setDefaultFilter`,
    /// `Image:setFilter`): nearest if any image on it is.
    fn decode_images(&mut self) {
        let mut new = false;
        for f in &mut self.files {
            if f.image.is_none()
                && let Some(a) = f.asset
                && a.state() == AssetState::Ready
                && let Some(img) = Image::new(a)
            {
                f.image = Some(img);
                new = true;
            }
        }
        if new {
            for (i, im) in self.images.iter_mut().enumerate() {
                im.image = self.files[self.image_file[i]].image;
            }
            for vm in [&mut self.title, &mut self.round].into_iter().flatten() {
                let scene = &mut vm.host_mut().scene;
                for (i, im) in self.images.iter().enumerate() {
                    if let Some(s) = scene.image_mut(i as u16) {
                        s.image = im.image;
                    }
                }
            }
        }
        let wanted: Vec<bool> = match self.on_stage() {
            Some(v) => v.host().nearest.clone(),
            None => self.project.as_ref().map_or_else(Vec::new, |p| p.images.iter().map(|i| i.pixel).collect()),
        };
        for (fi, f) in self.files.iter_mut().enumerate() {
            let near = self.image_file.iter().enumerate().any(|(i, file)| *file == fi && wanted.get(i).copied().unwrap_or(false));
            if let Some(img) = f.image
                && near != f.nearest
            {
                img.set_nearest(near);
                f.nearest = near;
            }
        }
    }

    /// Sounds decode once their assets are ready; live VMs get them.
    fn decode_sounds(&mut self) {
        let mut new = false;
        for (a, s) in self.sounds.iter_mut().chain(self.music.iter_mut()) {
            if s.is_none()
                && let Some(a) = a
                && a.state() == AssetState::Ready
                && let Some(snd) = Sound::new(*a)
            {
                *s = Some(snd);
                new = true;
            }
        }
        if new {
            // In place: nothing allocated on the frame an asset happens to
            // arrive (the Lua heap shares the wasm's allocator, and Lua's
            // object addresses order `pairs` over object keys).
            for vm in [&mut self.title, &mut self.round].into_iter().flatten() {
                for (dst, src) in vm.host_mut().sounds.iter_mut().zip(&self.sounds) {
                    *dst = src.1;
                }
            }
            // The music files too, for the Directors (api/music.rs), and for VMs made later.
            api::music::SESSION_SOUNDS.with(|s| {
                let mut s = s.borrow_mut();
                s.resize(self.music.len(), None);
                for (dst, src) in s.iter_mut().zip(&self.music) {
                    *dst = src.1;
                }
            });
            for vm in [&mut self.title, &mut self.round].into_iter().flatten() {
                api::music::SESSION_SOUNDS.with(|s| vm.music_sounds(&s.borrow()));
            }
        }
    }

    /// Voice lines a round has queued (and the one after each) are decoded
    /// as their files arrive, and handed to every VM that wants them. A
    /// line plays on its timing either way: one decoded late is heard from
    /// where it should be by then.
    fn decode_voices(&mut self) {
        for vm in [&mut self.title, &mut self.round].into_iter().flatten() {
            let v = &mut vm.host_mut().voice;
            for (i, want) in v.wanted.iter().enumerate() {
                if !*want || v.lines[i].has_sound() {
                    continue;
                }
                let Some((asset, sound)) = self.voices.get_mut(i) else { continue };
                if sound.is_none()
                    && let Some(a) = asset
                    && a.state() == AssetState::Ready
                {
                    *sound = Sound::new(*a);
                }
                if let Some(s) = sound {
                    v.lines[i].set_sound(*s);
                }
            }
        }
    }

    /// The music the on-stage round asked for, played across rounds.
    fn play_music(&mut self) {
        let asked = [&mut self.round, &mut self.title].into_iter().flatten().find_map(|v| v.host_mut().music.take());
        match asked {
            Some(Music::Play { name, fade, volume }) => {
                if self.playing.as_ref().is_some_and(|(n, _)| *n == name) {
                    return;
                }
                if let Some((_, v)) = self.playing.take() {
                    v.stop_at(sys::time(), fade);
                }
                self.wanted_music = Some((name, fade, volume));
            }
            Some(Music::Stop { fade }) => {
                if let Some((_, v)) = self.playing.take() {
                    v.stop_at(sys::time(), fade);
                }
                self.wanted_music = None;
            }
            None => {}
        }
        if let Some((name, _, volume)) = &self.wanted_music
            && let Some(p) = &self.project
            && let Some(i) = p.music.iter().position(|m| m.0 == *name)
            && let Some(snd) = self.music[i].1
        {
            let voice = snd.play_looped(*volume, 0.0, 1.0).bus(Bus::Music);
            // The name moves (no allocation on the frame the music arrives).
            let (name, _, _) = self.wanted_music.take().unwrap();
            self.playing = Some((name, voice));
        }
    }

    /// The project's files, as they arrive.
    fn load(&mut self) {
        match &self.loading {
            Loading::Index(a) => match a.state() {
                AssetState::Ready => {
                    let list = a.read().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                    let files = list
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
                let list = list.clone();
                let mut files = Files::new();
                for (path, a) in &list {
                    match a.state() {
                        AssetState::Ready => {
                            files.insert(path.clone(), if is_text(path) { a.read().unwrap_or_default() } else { Vec::new() });
                        }
                        _ => sys::log(Level::Warn, &format!("assets/{path} didn't load")),
                    }
                }
                self.loaded(files, &list);
            }
            Loading::Done => {}
        }
        if self.project.is_some() {
            self.decode_images();
        }
    }

    /// One frame of the VM on stage (the round's, or the title's attract mode).
    fn step(&mut self, dt: f32) {
        let stage = self.rounds.stage();
        let events = self.input.take();
        let held = self.input.held.clone();
        // Where the canvas is on the screen (3D's `project` answers in canvas units).
        let (sw, sh) = self.screen_size();
        let (ox, oy, s) = self.transform();
        for v in [&mut self.round, &mut self.title].into_iter().flatten() {
            v.host_mut().screen = [sw, sh, ox, oy, s];
        }
        if let Some(v) = &mut self.round {
            if self.error.is_some() {
                return; // a Lua error stops the world where it was
            }
            let playing = stage == Stage::Playing;
            // Under the ending and the game-over card the world steps on with no input.
            let (ev, held) = if playing { (&events[..], held) } else { (&[][..], input::Held::default()) };
            match v.step(dt, ev, &held, false, !playing) {
                Err(msg) => {
                    let score = v.host().last_score.round() as i64;
                    self.fail(msg);
                    if playing {
                        self.rounds.end(score);
                    }
                }
                Ok(step) if playing => {
                    let score = step.score.round() as i64;
                    let heading = match (&step.over, step.lives <= 0.0 && !step.hold) {
                        (Some(o), _) => Some(o.heading.clone()),
                        (None, true) => Some(None),
                        (None, false) => None,
                    };
                    if let Some(h) = heading {
                        // What the card says, for mb play's description (headless.rs).
                        self.card = h.clone().or_else(|| self.project.as_ref().map(|p| p.over.clone())).map(|s| s.to_uppercase());
                        // A daily round's card names the daily board.
                        let label = self.daily.as_ref().filter(|_| self.rounds.is_daily()).map(|d| d.label.clone());
                        let mut end = self.rounds.end(score);
                        if let Some(l) = label {
                            end = end.label(&l);
                        }
                        if let Some(h) = h {
                            end.heading(&h.to_uppercase());
                        }
                    }
                }
                Ok(_) => {}
            }
        } else if let Some(v) = &mut self.title {
            if self.error.is_none()
                && let Err(msg) = v.step(dt, &[], &input::Held::default(), true, false)
            {
                self.fail(msg);
            }
        } else if stage == Stage::Playing {
            // Nothing to play (the project didn't load): the round ends.
            self.rounds.end(0);
        }
    }

    /// The HUD (§4.12) from what the round says: `ui.hud`'s elements, the
    /// score and lives, a new identity; the game's own nodes get their values.
    fn update_hud(&mut self, dt: f32) {
        let best = self.rounds.best();
        let Some(p) = self.project.clone() else { return };
        let Some(v) = &mut self.round else { return };
        let h = v.host_mut();
        if let Some(s) = h.identity_change.take() {
            let mut id = p.identity.clone();
            (id.name, id.describe) = (None, None);
            id.set_name(&s);
            let t = identity::theme(&id, false, None, &|name| p.font(name).map(|f| f.path.clone()));
            self.theme = t;
            self.hud.theme = t;
            self.rounds.title.theme = t;
            h.scene.theme = t;
            h.font = t.body_font;
            h.voice.set_theme(t);
            self.identity_changed = true;
        }
        let spec = h.hud.clone();
        let (score, lives) = (h.last_score, h.last_lives.max(0.0));
        // The game's own nodes: the value goes in their `value` Text (or the node, a Text).
        use api::ui::El;
        let level = h.level_name.clone().unwrap_or_default();
        for (el, s) in [(&spec.score, maimbrain::ui::fmt_int(score.round() as i64)), (&spec.best, maimbrain::ui::fmt_int(best)), (&spec.lives, format!("{}", lives as i64)), (&spec.level, level)] {
            if let El::Node(n) = el {
                let target = h.scene.find_under(*n, "value").or(Some(*n)).filter(|t| h.scene.get(*t).is_some_and(|x| x.kind == maimbrain::scene::Kind::Text));
                if let Some(t) = target.and_then(|t| h.scene.node_mut(t)) {
                    t.text = s;
                }
            }
        }
        self.hud.set_score(score.round() as i64);
        self.hud.best = if spec.best == El::On { best } else { 0 };
        self.hud.lives = (spec.lives == El::On).then(|| (lives as u32, p.lives.max(lives as u32)));
        let time = h.time as f32;
        self.hud.progress = match (&spec.meter, &spec.timer) {
            (El::Num(m), _) => Some((*m as f32).clamp(0.0, 1.0)),
            (_, El::Num(secs)) if *secs > 0.0 => Some((1.0 - time / *secs as f32).clamp(0.0, 1.0)),
            _ => None,
        };
        self.hud_spec = Some((spec, time, h.level_name.clone()));
        self.hud.update(dt);
    }

    /// The round's stats card (`ui.stats`), laid out on the screen once
    /// it's asked for, moved on in real time; its `done` runs once the
    /// screen is covered (the game swaps the scene under it), and it goes
    /// once it has opened again.
    fn update_cards(&mut self, dt: f32) {
        let layout = maimbrain::ui::Layout::from_screen(&self.screen);
        let theme = self.theme;
        let Some(v) = &mut self.round else { return };
        let h = v.host_mut();
        let Some(s) = &mut h.ui.stats else { return };
        if s.card.is_none() {
            let mut c = maimbrain::ui::StatsCard::new(&theme.case(&s.title), theme, &layout).wipe(s.wipe);
            for (label, value, format, bad) in &s.rows {
                c = c.stat(&theme.case(label), *value, *format);
                if *bad {
                    c = c.bad();
                }
            }
            if let Some(r) = &s.rank {
                c = c.rank(r);
            }
            if let Some((n, line)) = &s.next {
                c = c.next(&theme.case(n), line);
            }
            if s.manual {
                c = c.manual();
            }
            s.card = Some(c);
        }
        let mut done = false;
        for e in s.card.as_mut().unwrap().update(dt) {
            match e {
                maimbrain::ui::StatsEvent::Covered => {
                    s.covered = true;
                    if s.done_ref != 0 {
                        h.libs.pending.push(libs::Due::Call { f: s.done_ref, args: Vec::new(), pass_self: false, timer: None });
                    }
                }
                maimbrain::ui::StatsEvent::Done => done = true,
                _ => {}
            }
        }
        if done {
            h.ui.stats = None;
        }
    }

    /// What the UI kit draws over the round in screen space: the
    /// countdown, the HUD's combo and the stats card.
    fn draw_cards(&self) {
        let Some(v) = self.round.as_ref().filter(|_| self.rounds.playing()) else { return };
        let ui = &v.host().ui;
        let layout = maimbrain::ui::Layout::from_screen(&self.screen);
        if let Some(c) = ui.hud_combo.and_then(|i| ui.combos.get(i)).filter(|_| !ui.stats_up()) {
            let corner = self.theme.style.hud == maimbrain::ui::HudLayout::Corner;
            let (x, y) = if corner { (layout.hud.right() - 40.0, layout.hud.y + 120.0) } else { (layout.hud.right() - 40.0, layout.hud.y + 82.0) };
            c.draw_in(x, y, &self.theme);
        }
        if let Some((c, _)) = &ui.countdown {
            c.draw(&layout, &self.theme);
        }
        if let Some(card) = ui.stats.as_ref().and_then(|s| s.card.as_ref()) {
            card.draw();
        }
    }

    /// The engine's HUD over the round, in the identity, clear of the reserved areas.
    /// What it drew, for the tests: "arcade" (1.0's look) or "identity",
    /// then "score", "best", "bar", "lives" and each pill's text.
    fn draw_hud(&self) -> Vec<String> {
        use api::ui::El;
        let mut drawn: Vec<String> = Vec::new();
        let Some((spec, time, level)) = &self.hud_spec else { return drawn };
        if !self.rounds.playing() || self.round.as_ref().is_none_or(|v| v.host().voice.cinematic()) {
            return drawn;
        }
        // A stats card over the round has the screen: no HUD under its title.
        if self.round.as_ref().is_some_and(|v| v.host().ui.stats_up()) {
            return drawn;
        }
        let layout = maimbrain::ui::Layout::from_screen(&self.screen);
        let th = &self.theme;
        // Under the Arcade preset with no identity of its own, a game keeps
        // engine 1.0's look (§4.9), its HUD too: the score small in the pixel
        // font, top centre, and hearts at the left when it has lives; under
        // them the best, the meter or timer bar, and the pills, as below.
        // Not once `ui.identity` has named one: then the identity's HUD.
        let arcade = self.project.as_ref().filter(|p| p.arcade && !self.identity_changed && matches!(p.identity.name.as_deref(), None | Some("arcade")) && p.identity.describe.is_none());
        let mut pills_y = layout.hud.y + if spec.score == El::On { 104.0 } else { 26.0 };
        if let Some(p) = arcade {
            drawn.push("arcade".into());
            let (ox, oy, _) = self.transform();
            let top = oy + self.screen.inset_top.max(0.0) + 16.0;
            let cx = self.screen_size().0 / 2.0;
            let mut below = top + 20.0;
            if spec.score == El::On {
                gfx2d::text_centered(Font::Pixel, 32.0, cx, top, 0xffffffff, &self.hud.score.target().to_string());
                drawn.push("score".into());
                below = top + 40.0;
                if self.hud.beat_best() {
                    gfx2d::text_centered(Font::Pixel, 16.0, cx, below, th.gold, "NEW BEST");
                    drawn.push("best".into());
                    below += 20.0;
                } else if self.hud.best > 0 {
                    gfx2d::text_centered(Font::Pixel, 16.0, cx, below, th.text_dim, &format!("BEST {}", self.hud.best));
                    drawn.push("best".into());
                    below += 20.0;
                }
            }
            if let Some(f) = self.hud.progress {
                maimbrain::ui::progress_bar(maimbrain::ui::Rect::centered(cx, below + 6.0, 160.0, 10.0), f, th.accent, maimbrain::ui::with_alpha(th.outline, 0.45));
                drawn.push("bar".into());
                below += 22.0;
            }
            if let Some((n, _)) = self.hud.lives.filter(|_| p.lives > 1)
                && n > 0
            {
                gfx2d::text(Font::Pixel, 16.0, ox + 12.0, top + 8.0, p.color(3) | 0xff, &"♥".repeat((n as usize).min(9)));
                drawn.push("lives".into());
            }
            pills_y = below + 14.0;
        } else if spec.score == El::On {
            drawn.push("identity".into());
            self.hud.draw(&layout);
            drawn.push("score".into());
            for (on, what) in [(self.hud.best > 0 || self.hud.beat_best(), "best"), (self.hud.progress.is_some(), "bar"), (self.hud.lives.is_some(), "lives")] {
                if on {
                    drawn.push(what.into());
                }
            }
        } else if let Some((n, max)) = self.hud.lives {
            drawn.push("identity".into());
            drawn.push("lives".into());
            // Lives alone: the hearts in the score's place, top centre.
            gfx2d::push();
            for i in 0..max {
                let x = layout.hud.cx() + (i as f32 - (max - 1) as f32 / 2.0) * 26.0;
                maimbrain::ui::Icon::HEART.draw(x, layout.hud.y + 26.0, 22.0, if i < n { th.bad } else { maimbrain::ui::with_alpha(th.text, 0.25) });
            }
            gfx2d::pop();
        }
        // Timer, combo and level: pills in a row under the score.
        let mut pills: Vec<String> = Vec::new();
        if let El::Num(secs) = spec.timer {
            let left = (secs as f32 - time).max(0.0).ceil() as i64;
            pills.push(format!("{}:{:02}", left / 60, left % 60));
        }
        match &spec.combo {
            El::Num(c) if *c > 1.0 => pills.push(format!("×{}", *c as i64)),
            El::Text(s) => pills.push(s.clone()),
            _ => {}
        }
        match (&spec.level, level) {
            (El::On, Some(l)) => pills.push(th.case(l)),
            (El::Text(s), _) => pills.push(s.clone()),
            (El::Num(n), _) => pills.push(format!("{} {}", th.case("level"), *n as i64)),
            _ => {}
        }
        if !pills.is_empty() {
            let y = pills_y;
            let gap = 8.0;
            let widths: Vec<f32> = pills.iter().map(|p| maimbrain::ui::pill_width(p, 28.0, th.body_font, false)).collect();
            let total: f32 = widths.iter().sum::<f32>() + gap * (pills.len() - 1) as f32;
            let mut x = layout.hud.cx() - total / 2.0;
            for (p, w) in pills.iter().zip(widths) {
                maimbrain::ui::pill(p, x + w / 2.0, y, 28.0, maimbrain::ui::with_alpha(th.panel_top, 0.85), th.text, th.body_font, None);
                x += w + gap;
            }
        }
        drawn.extend(pills);
        drawn
    }

    /// A bot run's frame, after the world's (§8.1): what the game's
    /// handlers heard, the round's end (or its cap) reported once, and
    /// otherwise the policy's decisions handed to the bot. `was_playing`:
    /// a round was being played when the frame began.
    fn bot_frame(&mut self, was_playing: bool) {
        let tf = self.transform();
        let Some(b) = &mut self.bot else { return };
        if b.done {
            return;
        }
        let now = sys::time();
        // The project didn't load, or a Lua error stopped the world: the run can't go on.
        if let Some(e) = &self.error
            && (was_playing || self.round.is_none())
        {
            let h = self.round.as_ref().map(|v| v.host());
            let outcome = bot::Outcome { score: h.map_or(0.0, |h| h.last_score), cause: "error".into(), error: Some(e.clone()), reads: h.is_some_and(|h| h.bot.as_ref().is_some_and(|f| f.reads.get())), ..Default::default() };
            b.report(now, &outcome);
            return;
        }
        if !was_playing {
            return;
        }
        let Some(v) = &mut self.round else { return };
        if let Some(f) = &mut v.host_mut().bot {
            for (k, t) in f.events.drain(..) {
                b.event(k, t);
            }
        }
        let h = v.host();
        let reads = h.bot.as_ref().is_some_and(|f| f.reads.get());
        if self.rounds.stage() != Stage::Playing {
            // Its cause: round.over's, else what took the last life.
            let over = h.over.clone();
            let cause = match &over {
                Some(o) => o.cause.clone().unwrap_or_else(|| if o.won { "won".into() } else { "over".into() }),
                None => b.events.iter().rev().find_map(|(k, _)| k.strip_prefix("hurt ").map(str::to_string)).unwrap_or_else(|| "lives".into()),
            };
            let outcome = bot::Outcome { score: h.last_score, cause, won: over.is_some_and(|o| o.won), reads, ..Default::default() };
            b.report(now, &outcome);
            return;
        }
        if now - b.round_start.unwrap_or(now) >= b.max - 1e-9 {
            let score = h.last_score;
            b.report(now, &bot::Outcome { score, cause: "time".into(), capped: true, reads, ..Default::default() });
            self.rounds.end(score.round() as i64);
            return;
        }
        match v.bot_step(b.finger(tf)) {
            Ok((mut intents, follow)) => {
                if follow {
                    let p = &v.host().project;
                    let home = (p.size.0 as f32 / 2.0, p.size.1 as f32 * 0.62);
                    intents.extend(b.follow_hints(now, &v.host().ui.hints, home));
                }
                b.decide(now, &intents, tf)
            }
            Err(msg) => {
                b.report(now, &bot::Outcome { score: v.host().last_score, cause: "error".into(), error: Some(msg.clone()), reads, ..Default::default() });
                self.fail(msg);
            }
        }
    }

    /// Whether the title card's attract mode has built its terrain's first
    /// view (or has none, or has waited long enough).
    fn title_ready(&self) -> bool {
        self.held_frames >= HOLD_FRAMES || self.title.as_ref().is_none_or(Vm::ground_ready)
    }

    /// The VM whose world is on screen.
    fn on_stage(&self) -> Option<&Vm> {
        self.round.as_ref().or(self.title.as_ref())
    }
}

/// A project image as the scene draws it.
pub fn scene_image(i: &project::ImageInfo) -> SceneImage {
    let mut s = SceneImage::new(i.w as f32, i.h as f32);
    (s.frame_w, s.frame_h) = (i.frame.0 as f32, i.frame.1 as f32);
    s.origin = [i.at.0 as f32, i.at.1 as f32];
    s
}

/// The project's identity as a theme (§4.12): its cards, HUD and default font.
pub fn theme(p: &Project) -> Theme {
    let mut t = identity::theme(&p.identity, p.arcade, p.art_colors, &|name| p.font(name).map(|f| f.path.clone()));
    // 1.0's look under the Arcade preset: its arcade theme in the project's colours.
    let plain = p.identity.name.is_none() && p.identity.describe.is_none();
    if p.arcade && (plain || p.identity.name.as_deref() == Some("arcade")) {
        t.bg_top = p.color(p.background.unwrap_or(0));
        t.bg_bottom = p.color(0);
    }
    t
}

impl Game for Studio2 {
    fn init() -> Self {
        #[cfg(three)]
        three::new_session();
        api::shaders::new_session();
        let mut s = Studio2 {
            loading: Loading::Done,
            project: None,
            images: Vec::new(),
            files: Vec::new(),
            image_file: Vec::new(),
            sounds: Vec::new(),
            music: Vec::new(),
            voices: Vec::new(),
            models: Vec::new(),
            playing: None,
            wanted_music: None,
            rounds: Rounds::new(TitleCard::new("", Theme::preset("greybox"))),
            daily: None,
            theme: Theme::preset("greybox"),
            hud: maimbrain::ui::Hud::new(Theme::preset("greybox"), 0),
            hud_spec: None,
            title: None,
            round: None,
            card: None,
            identity_changed: false,
            error: None,
            input: input::Mapper::default(),
            screen: sys::screen(),
            frames: 0,
            hooks: headless::Hooks::from_params(),
            debug: debugmenu::State::default(),
            menu: debugmenu::GameMenu::default(),
            bot: bot::Bot::from_params(sys::rand_seed()),
            held_input: Vec::new(),
            held_frames: 0,
        };
        #[cfg(target_arch = "wasm32")]
        {
            s.loading = Loading::Index(Asset::load(INDEX));
        }
        #[cfg(not(target_arch = "wasm32"))]
        s.loaded(native::files(), &[]);
        s
    }

    fn update(&mut self, dt: f32) {
        self.frames += 1;
        self.load();
        let mut input = maimbrain::input::poll();
        // The title card holds a press until the attract mode's terrain has
        // built its first view, as it does while the project loads, so the
        // round (a fresh VM) takes the ground over built (three.rs).
        let waiting = self.rounds.stage() == Stage::Title && !self.title_ready();
        if waiting {
            self.held_frames += 1;
        } else if !self.held_input.is_empty() {
            let later = std::mem::take(&mut input);
            input = std::mem::take(&mut self.held_input);
            input.extend(later);
        }
        // A bot run: the bot taps to start, and its touches join the frame's input.
        let (w, h) = self.screen_size();
        if let Some(b) = &mut self.bot {
            if self.project.is_some() && self.rounds.stage() == Stage::Title {
                b.start(sys::time(), w, h);
            }
            input.extend(b.due(sys::time()));
        }
        for e in input {
            if let Some(pick) = maimbrain::debug::Pick::from(&e) {
                // A round's ending doesn't take picks: they wait for the next round.
                if let Some(p) = self.project.clone() {
                    self.debug.pick(&p, &self.menu, pick, self.rounds.stage() == Stage::Playing && self.round.is_some());
                }
                continue;
            }
            if e.is_resize() {
                self.screen = sys::screen();
                continue;
            }
            if self.project.is_none() && !matches!(self.loading, Loading::Done) {
                // Still loading: a tap now would start a round with nothing to play.
                continue;
            }
            if waiting {
                self.held_input.push(e);
                continue;
            }
            if let Some(d) = &mut self.daily
                && d.input(&e, &mut self.rounds)
            {
                continue;
            }
            let tf = self.transform();
            // A stats card over the round takes the taps (a first reveals it
            // all, the next moves on), and the world under it hears nothing.
            if self.rounds.stage() == Stage::Playing
                && let Some(card) = self.round.as_mut().and_then(|v| v.host_mut().ui.stats.as_mut()).filter(|s| !s.covered).and_then(|s| s.card.as_mut())
                && matches!(e.kind(), maimbrain::input::Kind::TouchDown | maimbrain::input::Kind::TouchMove | maimbrain::input::Kind::TouchUp | maimbrain::input::Kind::TouchCancel | maimbrain::input::Kind::MouseDown | maimbrain::input::Kind::MouseUp | maimbrain::input::Kind::MouseMove)
            {
                card.input(&e);
                // A finger already down when the card came lifts for the
                // game too, so it isn't held forever (an unseen touch's up
                // is ignored; a mouse's only counts if its button was down).
                use maimbrain::input::Kind as K;
                let lifts = match e.kind() {
                    K::TouchUp | K::TouchCancel => true,
                    K::MouseUp => self.input.held.mouse[(e.id as usize).min(2)],
                    _ => false,
                };
                if lifts {
                    self.input.feed(&e, tf);
                }
                continue;
            }
            // A cutscene or a line in the box takes the round's taps first
            // (a press moves a line on, picks an answer, held skips a scene).
            if self.rounds.stage() == Stage::Playing
                && self.error.is_none()
                && let Some(v) = &mut self.round
            {
                let layout = maimbrain::ui::Layout::from_screen(&self.screen);
                if v.host_mut().voice.input(&e, &layout) {
                    continue;
                }
            }
            match self.rounds.input(&e) {
                Input::Start => {
                    self.start_round();
                    self.input.feed(&e, tf);
                }
                Input::Play => self.input.feed(&e, tf),
                Input::Taken | Input::Extra => {}
            }
        }
        // A scenario, a level jump or Restart round, picked during the round:
        // it starts again, in place, with the picks.
        if self.debug.take_restart() && self.rounds.stage() == Stage::Playing {
            self.start_round();
        }
        if self.rounds.stage() == Stage::Playing
            && let Some(v) = &mut self.round
            && self.error.is_none()
        {
            let cmds = self.debug.take_commands();
            if !cmds.is_empty()
                && let Err(msg) = v.debug(cmds)
            {
                self.fail(msg);
            }
        }
        let was_playing = self.rounds.stage() == Stage::Playing && self.round.is_some();
        self.step(dt);
        self.bot_frame(was_playing);
        self.update_hud(dt);
        self.update_cards(dt);
        if let Some(p) = self.project.clone() {
            let playing = self.round.is_some() && self.rounds.stage() == Stage::Playing;
            let live = self.round.as_ref().filter(|_| playing).map(|v| (v.host().last_score, v.host().last_lives));
            self.debug.show(&p, &self.menu, live, playing);
        }
        self.decode_images();
        self.decode_sounds();
        self.decode_voices();
        self.play_music();
        self.rounds.update(dt);
        // The game-over card went up: the round's bests on the other boards go too.
        if self.rounds.card_shown()
            && let Some(v) = &mut self.round
        {
            v.host_mut().boards.send();
        }
        if let Some(d) = &mut self.daily {
            d.update(dt, &self.rounds);
        }
        self.headless();
    }

    fn render(&self) {
        let (sw, sh) = self.screen_size();
        let Some(p) = &self.project else {
            gfx2d::clear(0x0b0d1fff);
            if let Some(e) = &self.error {
                error_box(e, sw);
            }
            self.rounds.draw();
            return;
        };
        let (w, h) = (p.size.0 as f32, p.size.1 as f32);
        let (ox, oy, s) = self.transform();
        // The readability check's variants of the frame (headless.rs).
        let isolate = headless::Isolate::asked(&self.hooks);
        if isolate == Some(headless::Isolate::Subject) {
            gfx2d::clear(headless::SUBJECT_KEY);
            gfx2d::rect(0.0, 0.0, sw, sh, headless::SUBJECT_KEY);
            if let (Some(v), Some(headless::Subject { node: api::Subject::D2(id), .. })) = (self.on_stage(), self.subject()) {
                let scene = &v.host().scene;
                gfx2d::push();
                gfx2d::translate(ox, oy);
                gfx2d::scale(s, s);
                scene.draw_filtered(&View { w, h }, None, false, &|e| headless::under(scene, e.0, id));
                gfx2d::pop();
            }
            return;
        }
        // A 3D game's world fills the screen, and its 2D (the scene's
        // nodes, love.draw, the HUD and the cards) draws over it.
        let three = p.three && self.on_stage().is_some();
        if three {
            #[cfg(three)]
            {
                maimbrain::gfx3d::render();
                // The debug menu's Show colliders, in 3D: lines on the screen.
                if let Some(w) = self.on_stage().and_then(|v| v.host().three.as_deref()) {
                    let mut lines = Vec::new();
                    w.physics_lines(&mut lines);
                    let c = w.camera();
                    let front = c.rot * maimbrain::gfx3d::vec3(0.0, 0.0, -1.0);
                    for (a, b, col) in lines {
                        if (a - c.pos).dot(front) < c.near || (b - c.pos).dot(front) < c.near {
                            continue;
                        }
                        let (pa, pb) = (c.project(a, sw, sh), c.project(b, sw, sh));
                        gfx2d::line(pa.x, pa.y, pb.x, pb.y, 1.5, col);
                    }
                }
            }
        } else {
            gfx2d::clear(0x000000ff);
        }
        let bg = self.on_stage().map_or(0x000000ff, |v| v.host().background);
        gfx2d::push();
        gfx2d::translate(ox, oy);
        gfx2d::scale(s, s);
        if !three {
            gfx2d::rect(0.0, 0.0, w, h, bg);
        }
        if let Some(v) = self.on_stage() {
            let host = v.host();
            if isolate == Some(headless::Isolate::Clean) {
                // Without the game's own HUD: fixed nodes (but layers) at order 0 or above.
                let hud = |e: &(maimbrain::scene::NodeId, f32, f32, bool)| e.3 && e.1 >= 0.0 && host.scene.get(e.0).is_some_and(|n| n.kind != maimbrain::scene::Kind::Layer);
                host.scene.draw_filtered(&View { w, h }, Some(&host.draw), true, &|e| !hud(e));
            } else {
                host.scene.draw(&View { w, h }, Some(&host.draw));
            }
            // The debug menu's Show colliders, in 2D: through the scene's camera.
            #[cfg(phys2)]
            crate::api::lovephysics::draw_debug(host, w, h);
            // Hints and lessons' prompts by what they point at (api/ui.rs),
            // not under the game-over card (nor in a frame without the HUD).
            let (sx, sy, sw2, _) = host.safe;
            let ended = (self.round.is_some() && !self.rounds.playing()) || isolate == Some(headless::Isolate::Clean);
            let bounds = maimbrain::ui::Rect::new(sx, sy, sw2, h);
            (!ended).then(|| host.ui.hints.draw_each(&self.theme, w / 2.0, h * 0.62, bounds, |hint| {
                hint.at.map(|(x, y)| {
                    // Above it, or below when that's under the pause pill.
                    if y - 52.0 < sy + 64.0 { (x, y + 52.0) } else { (x, y - 52.0) }
                })
            }));
            // The debug menu's Show hitboxes: each thing's box, through the camera.
            if host.show_hitboxes {
                for t in &host.quick.things {
                    if let Some([x, y, bw, bh]) = host.quick.hitbox(&host.scene, t.node, &t.kind) {
                        let (sx, sy) = host.scene.camera.to_screen(w, h, x, y);
                        let k = host.scene.camera.scale;
                        let (bw, bh) = (bw * k, bh * k);
                        for r in [[sx, sy, bw, 1.0], [sx, sy + bh - 1.0, bw, 1.0], [sx, sy, 1.0, bh], [sx + bw - 1.0, sy, 1.0, bh]] {
                            gfx2d::rect(r[0], r[1], r[2], r[3], 0xff3b6bff);
                        }
                    }
                }
            }
        }
        gfx2d::pop();
        // mb2d doesn't clip: the letterbox's bars cover what the world drew
        // past the canvas (a 3D world fills the screen).
        let bar = 0x000000ff;
        if !three && ox > 0.0 {
            gfx2d::rect(0.0, 0.0, ox, sh, bar);
            gfx2d::rect(ox + w * s, 0.0, sw - ox - w * s, sh, bar);
        }
        if !three && oy > 0.0 {
            gfx2d::rect(0.0, 0.0, sw, oy, bar);
            gfx2d::rect(0.0, oy + h * s, sw, sh - oy - h * s, bar);
        }
        if isolate != Some(headless::Isolate::Clean) {
            self.draw_hud();
            self.draw_cards();
        }
        // The box, barks, answers and a cutscene's bars and cards, over the
        // world and the HUD (a cutscene hides the HUD).
        if isolate != Some(headless::Isolate::Clean)
            && let Some(v) = self.on_stage()
        {
            v.host().voice.draw(&maimbrain::ui::Layout::from_screen(&self.screen));
        }
        if let Some(e) = &self.error {
            error_box(e, sw);
        }
        self.rounds.draw();
        if let Some(d) = &self.daily {
            d.draw(&self.rounds, &self.theme);
        }
    }
}

/// A Lua error, or a problem in the project, over the game.
fn error_box(e: &str, sw: f32) {
    let t = maimbrain::ui::text(e).font(Font::Sans).size(14.0).color(0xff8080ff).wrap(sw - 40.0);
    let (_, th) = t.measure();
    gfx2d::rect(8.0, 8.0, sw - 16.0, th + 24.0, 0x000000d8);
    t.draw(20.0, 20.0);
}

export_game!(Studio2);

#[cfg(test)]
mod bot_tests;
#[cfg(test)]
mod music_tests;
#[cfg(test)]
mod tests;
#[cfg(three)]
pub mod nav3;
#[cfg(three)]
pub mod physics3;
#[cfg(test)]
mod boards_tests;
#[cfg(test)]
mod sensor_tests;
#[cfg(test)]
mod three_tests;
#[cfg(test)]
mod voice_tests;
#[cfg(test)]
mod juice_tests;
#[cfg(test)]
mod ui_tests;
#[cfg(test)]
mod physics_tests;
#[cfg(test)]
mod physics3_tests;
#[cfg(test)]
mod nav_tests;
#[cfg(test)]
mod api_reference;
