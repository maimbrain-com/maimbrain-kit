//! Music that follows the game (docs/STUDIO-2.md §5 [sound], Phase 3d):
//! `music.play` of a music graph (`music/<name>.music.json`) or a setlist
//! (`music/<name>.songs.json`) on the SDK's `music::Director` and `Songs`,
//! `music.mood`, `music.go`, `music.nearMiss`, `music.stop`, what the music
//! is doing (`music.time`, `bar`, `beat`, `bpm`, `zone`, `playing`), and
//! `on("beat")` / `on("bar")`.
//!
//! **The grid is game time.** The Director belongs to the round's VM and
//! runs on the round's clock (`round.time`), seeded by the round's seed, so
//! where every bar line falls, every jump and every zone change is a
//! function of the round alone. Beats are the graph's tempo map in that
//! time (`Grid`), never the audio clock: a beat at 12.62 s fires on the
//! first frame at or past 12.62 s, in a replay too, and a hitch never moves
//! it. The sound follows: each voice is scheduled at its game time plus
//! the frame's offset to the session's clock (`Speaker`), which the platform
//! plays on time (SPEC §5.6); after lost time (`sys::time_lost`) the take
//! is started again where the grid says it is (`Director::resync`).
//!
//! **Each round starts the music on its own clock.** What a round plays
//! carries into the next round (the engine hands it over, `carried`), but
//! it starts again there, from its zone's top, on that round's clock, so a
//! round's beats never depend on how long the title card or the last round
//! ran. A plain `.ogg` loop has no grid: it carries on playing across rounds
//! as in Phase 1 (lib.rs), and `music.bar` and `music.beat` stay 0.

use std::cell::RefCell;
use std::ffi::{c_int, c_void};
use std::rc::Rc;

use maimbrain::audio::{Bus, Sound, Voice};
use maimbrain::music::{Change, Director, Grid, Mood, Out, RESTART_FADE, Songs};

use super::*;

/// What the round's music sounds through, shared by its Directors.
#[derive(Debug, Default)]
pub struct Shared {
    /// The session's clock minus the round's (`sys::time() - round.time`), this frame.
    pub offset: f64,
    /// `music.play`'s volume, on every voice.
    pub volume: f32,
    /// The project's music files as they decode, by index (a Director's
    /// sounds are `Sound(index + 1)`, mapped here).
    pub sounds: Vec<Option<Sound>>,
}

/// The platform, from the round's clock: a Director's game times become
/// the session's, its sounds the decoded ones. A file that hasn't decoded
/// plays nothing (the music is output only).
#[derive(Clone)]
pub struct Speaker(pub Rc<RefCell<Shared>>);

impl Speaker {
    fn real(&self, s: Sound) -> Option<(Sound, f64, f32)> {
        let sh = self.0.borrow();
        let r = sh.sounds.get((s.0 as usize).wrapping_sub(1)).copied().flatten()?;
        Some((r, sh.offset, sh.volume))
    }
}

impl Out for Speaker {
    fn play_at(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, looped: bool, bus: Bus) -> Voice {
        let Some((s, off, v)) = self.real(sound) else { return Voice(0) };
        let x = if looped { s.play_looped_at(vol * v, 0.0, pitch, at + off) } else { s.play_at(vol * v, 0.0, pitch, at + off) };
        x.bus(bus)
    }
    fn play_from(&mut self, sound: Sound, vol: f32, at: f64, from: f64, fade: f32, looped: bool, bus: Bus) -> Voice {
        let Some((s, off, v)) = self.real(sound) else { return Voice(0) };
        let x = if looped { s.play_looped_from(vol * v, 0.0, 1.0, at + off, from, fade) } else { s.play_from(vol * v, 0.0, 1.0, at + off, from, fade) };
        x.bus(bus)
    }
    fn set(&mut self, voice: Voice, vol: f32, _now: f64) {
        if voice.0 != 0 {
            voice.set(vol * self.0.borrow().volume, 0.0, 1.0);
        }
    }
    fn stop(&mut self, voice: Voice, _now: f64) {
        if voice.0 != 0 {
            voice.stop();
        }
    }
    fn stop_at(&mut self, voice: Voice, at: f64, fade: f32) {
        if voice.0 != 0 {
            voice.stop_at(at + self.0.borrow().offset, fade);
        }
    }
    fn lowpass(&mut self, cutoff: f32, ramp: f32, _now: f64) {
        Bus::Music.lowpass(cutoff, 0.707, ramp);
    }
    fn music_volume(&mut self, gain: f32, ramp: f32, _now: f64) {
        // The Director's part of the music bus (its dip under a sting), times the game's and the voices'.
        super::sound::music_mix::set(super::sound::music_mix::DIRECTOR, gain, ramp);
    }
}

/// What plays.
pub enum Track {
    None,
    /// A plain loop (the engine plays it, across rounds): when it was asked for.
    Loop { since: f64 },
    Graph(Box<Director<Speaker>>),
    Songs(Box<Songs<Speaker>>),
}

/// What a round asked to play, handed to the next round.
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    pub name: String,
    pub zone: Option<String>,
    pub volume: f32,
}

/// The round's music: its Director, its mood and its beat clock.
pub struct MusicState {
    pub shared: Rc<RefCell<Shared>>,
    pub track: Track,
    pub mood: Mood,
    pub asked: Option<Asked>,
    /// The last beat fired (round time), bars begun since the music started,
    /// and the first downbeat's time.
    last_beat: f64,
    bars: i64,
    start: f64,
    /// Round time of the last frame (for a stop when the VM goes).
    now: f64,
}

thread_local! {
    /// The session's music files as they've decoded (lib.rs keeps it): a new
    /// VM's Directors have them from love.load on.
    pub static SESSION_SOUNDS: RefCell<Vec<Option<Sound>>> = const { RefCell::new(Vec::new()) };
}

/// The most beats a frame fires (a long hitch's catch-up).
const MAX_BEATS: usize = 64;

impl MusicState {
    pub fn new(music_files: usize) -> MusicState {
        let mut sounds = vec![None; music_files];
        SESSION_SOUNDS.with(|s| {
            for (d, x) in sounds.iter_mut().zip(s.borrow().iter()) {
                *d = *x;
            }
        });
        MusicState {
            shared: Rc::new(RefCell::new(Shared { offset: 0.0, volume: 1.0, sounds })),
            track: Track::None,
            mood: Mood::default(),
            asked: None,
            last_beat: f64::NEG_INFINITY,
            bars: 0,
            start: 0.0,
            now: 0.0,
        }
    }

    fn set_offset(&self, now: f64) {
        self.shared.borrow_mut().offset = maimbrain::sys::time() - now;
    }

    /// The grid heard at round time `t`.
    pub fn grid_at(&self, t: f64) -> Option<Grid> {
        match &self.track {
            Track::Graph(d) => d.grid(),
            Track::Songs(s) => s.grid_at(t),
            _ => None,
        }
    }

    /// Stops what plays, over `fade` seconds (and forgets it).
    pub fn stop(&mut self, now: f64, fade: f32) {
        self.set_offset(now);
        match &mut self.track {
            Track::Graph(d) => d.stop(now, fade),
            Track::Songs(s) => s.stop(now, fade),
            _ => {}
        }
        self.track = Track::None;
        self.asked = None;
    }

    /// What it plays, for the next round; it lets go of the sound itself (the round is over).
    pub fn hand_over(&mut self) -> Option<Asked> {
        let asked = match self.track {
            Track::Graph(_) | Track::Songs(_) => self.asked.clone(),
            _ => None,
        };
        let now = self.now;
        self.stop(now, RESTART_FADE);
        asked
    }

    /// One frame, after the round's Lua: the Director decides what's next.
    pub fn update(&mut self, now: f64, dt: f32) {
        self.now = now;
        self.set_offset(now);
        let lost = maimbrain::sys::time_lost() > 0.0;
        let mood = self.mood;
        match &mut self.track {
            Track::Graph(d) => {
                if lost {
                    d.resync(now);
                }
                d.update(now, dt, mood);
            }
            Track::Songs(s) => {
                if lost && let Some(d) = s.current().and_then(|c| s.director_mut(c)) {
                    d.resync(now);
                }
                s.update(now, dt, mood);
            }
            _ => {}
        }
    }

    /// Beats due by round time `now`, in order: (beat in its bar from 1, bar from 1, round time).
    pub fn due(&mut self, now: f64) -> Vec<(u32, i64, f64)> {
        let mut out = Vec::new();
        while out.len() < MAX_BEATS {
            let Some(g) = self.grid_at(self.last_beat + 1e-6) else { break };
            let (mut n, mut t) = g.next_after(self.last_beat);
            // A setlist's next song starts on its cue, its own beat 0.
            if let Track::Songs(s) = &self.track
                && let Some(c) = s.change_at()
                && c > self.last_beat + 1e-6
                && c < t - 1e-6
            {
                (n, t) = (0, c);
            }
            if t > now + 1e-9 {
                break;
            }
            self.last_beat = t;
            if n < 0 {
                continue; // the pickup before the first downbeat
            }
            let k = n.rem_euclid(4) as u32;
            if k == 0 {
                self.bars += 1;
            }
            out.push((k + 1, self.bars.max(1), t));
        }
        out
    }

    fn started(&mut self, now: f64) {
        self.last_beat = now;
        self.bars = 0;
        self.start = self.grid_at(now).map_or(now, |g| g.origin);
    }
}

impl Drop for MusicState {
    fn drop(&mut self) {
        // The VM is going (a new project, a new round): its voices go with it.
        let now = self.now;
        if !matches!(self.track, Track::None | Track::Loop { .. }) {
            self.stop(now, 0.1);
        }
    }
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "music", &[(c"play", m_play), (c"stop", m_stop), (c"mood", m_mood), (c"go", m_go), (c"nearMiss", m_near_miss)]);
        // music.time, .bar, .beat, .bpm, .zone, .playing: read when asked.
        lua_rawgetfield(L, LUA_GLOBALSINDEX, c"music".as_ptr());
        lua_createtable(L, 0, 1);
        set_fn(L, ud, c"__index", m_index);
        lua_setmetatable(L, -2);
        lua_pop(L, 1);
        register(L, ud, "__engine", &[(c"beats", beats)]);
    }
}

/// `music.play(name, opts)` of a graph or a setlist (a plain loop is audio.rs's).
/// Ok(false) when `name` is neither.
pub fn play(h: &mut Host, name: &str, zone: Option<&str>, fade: f32, volume: f32) -> Result<bool, String> {
    let p = h.project.clone();
    let graph = p.graphs.iter().position(|g| g.0 == name);
    let list = p.setlists.iter().position(|s| s.0 == name);
    if graph.is_none() && list.is_none() {
        return Ok(false);
    }
    let now = h.time;
    let asked = Asked { name: name.to_string(), zone: zone.map(str::to_string), volume };
    // The same music again (love.load in each round): it plays on.
    if h.tunes.asked.as_ref() == Some(&asked) && !matches!(h.tunes.track, Track::None) {
        return Ok(true);
    }
    h.tunes.stop(now, fade);
    h.tunes.shared.borrow_mut().volume = volume;
    let seed = h.seed ^ name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |x, b| (x ^ b as u64).wrapping_mul(0x100_0000_01b3));
    let speaker = Speaker(h.tunes.shared.clone());
    let director = |i: usize, s: u64| -> Result<Director<Speaker>, String> {
        let (n, g) = &p.graphs[i];
        Director::with_graph(g.clone(), |path: &str| p.music_file(path).map(|k| Sound(k as u32 + 1)), s, speaker.clone()).ok_or_else(|| format!("music/{n}.music.json has no take"))
    };
    h.tunes.set_offset(now);
    if let Some(i) = graph {
        let mut d = director(i, seed)?;
        let z = match zone {
            Some(z) => d.zone(z).ok_or_else(|| format!("music.play: \"{name}\" has no zone \"{z}\" (its zones: {})", zones(&p.graphs[i].1).join(", ")))?,
            None => 0,
        };
        d.start(z, now);
        h.tunes.track = Track::Graph(Box::new(d));
    } else if let Some(i) = list {
        let mut s = Songs::empty();
        for (k, (z, gi)) in p.setlists[i].1.iter().enumerate() {
            s.add(z, director(*gi, seed.wrapping_add(k as u64))?);
        }
        let first = match zone {
            Some(z) => s.song(z).ok_or_else(|| format!("music.play: \"{name}\" has no zone \"{z}\" (its zones: {})", s.zones().collect::<Vec<_>>().join(", ")))?,
            None => 0,
        };
        s.start(first, now);
        h.tunes.track = Track::Songs(Box::new(s));
    }
    h.tunes.asked = Some(asked);
    h.tunes.started(now);
    Ok(true)
}

fn zones(g: &maimbrain::music::Graph) -> Vec<&str> {
    g.zones.iter().map(|z| z.name.as_str()).collect()
}

b!(m_play, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "music.play", "music name") };
    let name = name.strip_prefix("music/").unwrap_or(name);
    let name = name.trim_end_matches(".ogg").trim_end_matches(".music.json").trim_end_matches(".songs.json").to_string();
    let fade = super::audio::opt_field(L, 2, c"fade", 0.5).max(0.0);
    let volume = super::audio::opt_field(L, 2, c"volume", 0.7).clamp(0.0, 1.0);
    let zone = str_field(L, 2, c"zone");
    match play(h, &name, zone.as_deref(), fade, volume) {
        Ok(true) => {
            // A plain loop the engine was playing makes way.
            h.music = Some(super::audio::Music::Stop { fade });
            0
        }
        Ok(false) => {
            if !h.project.music.iter().any(|m| m.0 == name) {
                return err(L, &format!("no music \"{name}\": add music/{name}.ogg, a song music/{name}.song.toml, or generate a music graph (music/{name}.music.json)"));
            }
            h.tunes.stop(h.time, fade);
            h.tunes.track = Track::Loop { since: h.time };
            h.music = Some(super::audio::Music::Play { name, fade, volume });
            0
        }
        Err(e) => err(L, &e),
    }
});

b!(m_stop, |L, h| {
    let fade = opt(L, 1, 0.5).max(0.0) as f32;
    h.tunes.stop(h.time, fade);
    h.music = Some(super::audio::Music::Stop { fade });
    0
});

b!(m_mood, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "music.mood", "table ({ intensity = 0–1, danger = 0–1 })");
    }
    for (k, slot) in [(c"intensity", 0), (c"danger", 1)] {
        lua_rawgetfield(L, 1, k.as_ptr());
        let v = num(L, -1);
        let present = lua_type(L, -1) != LUA_TNIL;
        lua_pop(L, 1);
        match v {
            Some(v) if v.is_finite() => {
                let v = v.clamp(0.0, 1.0) as f32;
                if slot == 0 { h.tunes.mood.intensity = v } else { h.tunes.mood.danger = v }
            }
            _ if present => return err(L, "music.mood: intensity and danger are numbers from 0 to 1"),
            _ => {}
        }
    }
    0
});

b!(m_go, |L, h| {
    let Some(zone) = st(L, 1) else { return bad(L, 1, "music.go", "zone name") };
    let change = match str_field(L, 2, c"change").as_deref() {
        None | Some("auto") => Change::Auto,
        Some("nextBar") => Change::NextBar,
        Some("stinger") => Change::Stinger,
        Some("crossfade") => Change::Crossfade(super::audio::opt_field(L, 2, c"bars", 1.0).clamp(0.0, 8.0)),
        Some(o) => return err(L, &format!("music.go: change is \"auto\", \"nextBar\", \"stinger\" or \"crossfade\", got \"{o}\"")),
    };
    let now = h.time;
    h.tunes.set_offset(now);
    match &mut h.tunes.track {
        Track::Graph(d) => match d.zone(zone) {
            Some(z) => d.go(z, change, now),
            None => {
                let names = d.graph().map_or_else(|| d.zone_name().map(|z| vec![z.to_string()]).unwrap_or_default(), |g| zones(g).into_iter().map(str::to_string).collect());
                return err(L, &format!("music.go: no zone \"{zone}\" (the music's zones: {})", names.join(", ")));
            }
        },
        Track::Songs(s) => match s.song(zone) {
            Some(z) => s.go(z, change, now),
            None => return err(L, &format!("music.go: no zone \"{zone}\" (the setlist's: {})", s.zones().collect::<Vec<_>>().join(", "))),
        },
        Track::Loop { .. } => return err(L, "music.go needs a music graph or a setlist (this music is one loop)"),
        // Nothing yet: what the last round played starts after love.load.
        Track::None => {}
    }
    0
});

b!(m_near_miss, |L, h| {
    let now = h.time;
    h.tunes.set_offset(now);
    match &mut h.tunes.track {
        Track::Graph(d) => d.near_miss(now),
        Track::Songs(s) => s.near_miss(now),
        _ => {}
    }
    0
});

// `music.<key>`: what the music is doing, on the round's clock.
b!(m_index, |L, h| {
    let Some(k) = st(L, 2) else { return 0 };
    let now = h.time;
    let t = &h.tunes;
    let grid = t.grid_at(now);
    match k {
        "time" => ret(L, match t.track {
            Track::None => 0.0,
            Track::Loop { since } => now - since,
            _ => now - t.start,
        }),
        "bar" => ret(L, t.bars as f64),
        "beat" => ret(L, grid.map_or(0.0, |g| {
            let b = g.beat_at(now);
            if b < 0.0 { 0.0 } else { b.rem_euclid(g.beats_per_bar as f64) + 1.0 }
        })),
        "bpm" => ret(L, grid.map_or(0.0, |g| (g.bpm() * 1000.0).round() / 1000.0)),
        "zone" => match &t.track {
            Track::Graph(d) => match d.zone_at(now) {
                Some(z) => rets(L, z),
                None => 0,
            },
            Track::Songs(s) => match s.heard(now).and_then(|c| s.zones().nth(c)) {
                Some(z) => rets(L, z),
                None => 0,
            },
            _ => 0,
        },
        "playing" => retb(L, !matches!(t.track, Track::None)),
        _ => 0,
    }
});

b!(
    /// `__engine.beats()`: the beats due this frame, `{ beat, bar, at, … }`, or nil.
    beats, |L, h| {
    let due = h.tunes.due(h.time);
    if due.is_empty() {
        return 0;
    }
    lua_createtable(L, due.len() as c_int * 3, 0);
    for (i, (b, bar, at)) in due.iter().enumerate() {
        lua_pushnumber(L, *b as f64);
        lua_rawseti(L, -2, i as c_int * 3 + 1);
        lua_pushnumber(L, *bar as f64);
        lua_rawseti(L, -2, i as c_int * 3 + 2);
        lua_pushnumber(L, *at);
        lua_rawseti(L, -2, i as c_int * 3 + 3);
    }
    1
});

/// A string field of an options table at `i`.
unsafe fn str_field(L: *mut lua_State, i: c_int, k: &std::ffi::CStr) -> Option<String> {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return None;
        }
        lua_rawgetfield(L, i, k.as_ptr());
        let v = st(L, -1).map(str::to_string);
        lua_pop(L, 1);
        v
    }
}

