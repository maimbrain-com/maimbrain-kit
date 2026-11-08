//! Studio's sound past LÖVE's sources (docs/STUDIO-2.md §5 [sound]): the
//! mixing buses (`audio.bus`), hits as loud as they were hard
//! (`audio.impact`), sound placed in the world (`src:setPosition`,
//! `love.audio.setPosition`), and foley, a node's `footsteps` and `hum`.
//! It's the SDK's (`audio::Bus`, `foley`): the buses' filters, reverb and
//! delay are the platform's (SPEC §5.6), and foley places each sound from
//! the camera, varies it and keeps a voice budget.
//!
//! Sound is output only. Foley varies its sounds from its own seeded
//! generator and moves on real time, and nothing here is read back by the
//! game, so a round plays and replays the same with or without it. With
//! `mb play`'s `studio_trace`, every sound a round starts (and a loop's
//! level as it follows its node) is logged as `studio-sound` lines, which
//! `mb play --json` lists under `sounds`: sound can't be heard headless,
//! but when it plays can be checked.

use std::ffi::{c_int, c_void};

use maimbrain::audio::{Bus, Delay, OPEN, Reverb, Sound, Voice};
use maimbrain::foley::{Contact, Drive, Foley, Listener, Mixer, RELEASE, Space, impact_level};
use maimbrain::gfx3d::{Vec3, vec3};
use maimbrain::scene::NodeId;

use super::*;

/// `audio.bus`'s handles.
pub const BUS_TAG: i32 = tag::BUS;
/// By `maimbrain::audio::Bus`'s numbers.
pub const BUS_NAMES: [&str; 3] = ["master", "music", "sfx"];
const BUSES: [Bus; 3] = [Bus::Master, Bus::Music, Bus::Sfx];
/// How long a bus change glides (a tween or a spring writes it each frame).
const RAMP: f32 = 0.05;
/// Lines of `studio-sound` a round logs, at most.
const MAX_LINES: usize = 4000;
/// Nodes with foley, at most.
const MAX_FOLEY: usize = 256;

/// A bus as the round set it.
#[derive(Clone, Debug, PartialEq)]
pub struct BusState {
    pub volume: f32,
    pub lowpass: f32,
    pub highpass: f32,
    /// Its name (a preset's, or "custom") and the reverb.
    pub reverb: (String, Reverb),
    pub delay: Option<Delay>,
}

impl Default for BusState {
    fn default() -> BusState {
        BusState { volume: 1.0, lowpass: OPEN, highpass: 20.0, reverb: ("off".into(), Reverb::OFF), delay: None }
    }
}

/// What a node with foley is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FoleyNode {
    D2(NodeId),
    #[cfg(three)]
    D3(maimbrain::scene::three::Node3Id),
}

/// `node.hum`: a loop following the node's speed (the SDK's `Drive`).
#[derive(Clone, Debug)]
pub struct Hum {
    pub sound: usize,
    pub drive: Drive,
}

/// A node's foley.
#[derive(Clone, Debug)]
pub struct Noisy {
    pub node: FoleyNode,
    pub footsteps: Option<usize>,
    pub hum: Option<Hum>,
    last: Option<Vec3>,
    speed: f32,
    /// A plantFeet model's feet, watched for touchdowns.
    #[cfg_attr(not(three), allow(dead_code))]
    feet: [Contact; 2],
}

/// Plays foley's sounds through the platform, the project's sounds by
/// index (a foley `Sound` here is `Sound(index)`, and a voice is its own
/// number, so the log reads the same whether the sound has loaded or not).
pub struct Mix {
    /// The project's sounds, as decoded (None until they are).
    pub handles: Vec<Option<Sound>>,
    pub names: Vec<String>,
    voices: Vec<Held>,
    next: u32,
    /// Game time, for the log.
    pub now: f64,
    pub mute: bool,
    pub trace: bool,
    pub lines: Vec<String>,
}

/// A voice foley holds: ours, the platform's, its sound, the level last
/// logged, and (a one-shot) when it's forgotten.
#[derive(Clone, Copy, Debug)]
struct Held {
    id: u32,
    real: Option<Voice>,
    sound: usize,
    logged: [f32; 3],
    until: Option<f64>,
}

impl Mix {
    fn log(&mut self, line: String) {
        if !self.trace || self.lines.len() >= MAX_LINES {
            return;
        }
        maimbrain::sys::log(maimbrain::sys::Level::Debug, &format!("studio-sound {line}"));
        self.lines.push(line);
    }

    fn name(&self, s: usize) -> &str {
        self.names.get(s).map_or("?", String::as_str)
    }

    /// Stops every voice (the round is over).
    pub fn stop_all(&mut self) {
        for v in self.voices.drain(..) {
            if let Some(r) = v.real {
                r.stop();
            }
        }
    }
}

impl Mixer for Mix {
    fn play(&mut self, sound: Sound, vol: f32, pan: f32, pitch: f32, looped: bool, _now: f64) -> Voice {
        self.next += 1;
        let s = sound.0 as usize;
        if self.mute {
            return Voice(self.next);
        }
        let real = self.handles.get(s).copied().flatten().map(|h| if looped { h.play_looped(vol, pan, pitch) } else { h.play(vol, pan, pitch) });
        // One-shots are forgotten after foley lets them go (it holds them a second at most).
        let now = self.now;
        self.voices.retain(|v| v.until.is_none_or(|u| u > now));
        self.voices.push(Held { id: self.next, real, sound: s, logged: [vol, pitch, pan], until: (!looped).then_some(now + 2.0) });
        let line = format!("{:.3} {} {vol:.3} {pitch:.3} {} id=f{} pan={pan:.2}", self.now, self.name(s), if looped { "loop" } else { "play" }, self.next);
        self.log(line);
        Voice(self.next)
    }

    fn set(&mut self, voice: Voice, vol: f32, pan: f32, pitch: f32, _now: f64) {
        let Some(i) = self.voices.iter().position(|v| v.id == voice.0) else { return };
        let Held { real, sound: s, logged: last, .. } = self.voices[i];
        if let Some(r) = real {
            r.set(vol, pan, pitch);
        }
        // Logged when it has changed by a step a listener would hear.
        let moved = (vol - last[0]).abs() > 0.03 + 0.1 * last[0] || (pitch - last[1]).abs() > 0.03 || (pan - last[2]).abs() > 0.1;
        if moved {
            self.voices[i].logged = [vol, pitch, pan];
            let line = format!("{:.3} {} {vol:.3} {pitch:.3} set id=f{} pan={pan:.2}", self.now, self.name(s), voice.0);
            self.log(line);
        }
    }

    fn stop(&mut self, voice: Voice, _now: f64) {
        let Some(i) = self.voices.iter().position(|v| v.id == voice.0) else { return };
        let Held { real, sound: s, .. } = self.voices.remove(i);
        if let Some(r) = real {
            r.stop_at(0.0, RELEASE);
        }
        let line = format!("{:.3} {} 0 1 stop id=f{}", self.now, self.name(s), voice.0);
        self.log(line);
    }
}

/// A round's sound: its buses, foley and where it's heard from.
pub struct Sounds {
    pub buses: [BusState; 3],
    /// The round changed a bus: they're put back when it ends.
    pub touched: bool,
    pub foley: Foley<Mix>,
    /// `love.audio.setPosition`: where it's heard from, if not the camera.
    pub listener: Option<[f32; 3]>,
    pub noisy: Vec<Noisy>,
}

impl Sounds {
    pub fn new(seed: u64, names: Vec<String>, mute: bool, trace: bool) -> Sounds {
        let mix = Mix { handles: Vec::new(), names, voices: Vec::new(), next: 0, now: 0.0, mute, trace: trace && !mute, lines: Vec::new() };
        Sounds { buses: Default::default(), touched: false, foley: Foley::with_mixer(seed ^ 0x5f0_1e7, mix), listener: None, noisy: Vec::new() }
    }

    /// The `studio-sound` lines so far (tests).
    pub fn lines(&self) -> &[String] {
        &self.foley.mixer.lines
    }
}

impl Drop for Sounds {
    /// A round's loops stop with it, and what it did to the buses is undone.
    fn drop(&mut self) {
        self.foley.mixer.stop_all();
        if self.touched && !self.foley.mixer.mute {
            for b in BUSES {
                reset(b);
            }
        }
    }
}

/// The music bus's volume has three owners, multiplied, so none undoes
/// another: the game's (`audio.bus("music").volume`), the Director's dip
/// under a near-miss sting (music.rs), and the dip under voices
/// (voice.rs). Each sets its own part here and the bus gets the product.
pub mod music_mix {
    use std::cell::Cell;

    use maimbrain::audio::Bus;

    pub const GAME: usize = 0;
    pub const DIRECTOR: usize = 1;
    pub const VOICE: usize = 2;

    thread_local! {
        static PARTS: Cell<[f32; 3]> = const { Cell::new([1.0; 3]) };
    }

    /// One part of the music bus's volume, gliding over `ramp` seconds.
    pub fn set(part: usize, gain: f32, ramp: f32) {
        let mut p = PARTS.with(Cell::get);
        p[part] = gain;
        PARTS.with(|c| c.set(p));
        Bus::Music.volume(p.iter().product(), ramp);
    }

    /// One part, as last set.
    pub fn get(part: usize) -> f32 {
        PARTS.with(Cell::get)[part]
    }
}

/// A bus as it was before any round touched it.
fn reset(b: Bus) {
    let d = BusState::default();
    if b == Bus::Music {
        music_mix::set(music_mix::GAME, d.volume, 0.15);
    } else {
        b.volume(d.volume, 0.15);
    }
    b.lowpass(d.lowpass, 0.707, 0.15);
    b.highpass(d.highpass, 0.707, 0.15);
    b.reverb(d.reverb.1, 0.15);
    b.delay(Delay::OFF, 0.15);
}

/// The log line of a source starting (audio.rs): sources are numbered `s1`, `s2`…
pub fn log_source(h: &mut Host, i: usize, vol: f32, pitch: f32, pan: f32, looped: bool) {
    let s = h.sources[i].sound;
    let m = &mut h.sound.foley.mixer;
    m.now = h.time;
    let line = format!("{:.3} {} {vol:.3} {pitch:.3} {} id=s{} pan={pan:.2}", h.time, m.name(s), if looped { "loop" } else { "play" }, i + 1);
    m.log(line);
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(L, ud, "audio", &[(c"bus", bus), (c"impact", impact)]);
        register_kind(L, ud, BUS_TAG, c"Bus", &[(c"__index", bus_index), (c"__newindex", bus_newindex)]);
        register(L, ud, "love.audio", &[(c"setPosition", a_setPosition), (c"getPosition", a_getPosition)]);
    }
}

// ---------------------------------------------------------------- buses

b!(bus, |L, h| {
    let name = st(L, 1).unwrap_or("");
    match BUS_NAMES.iter().position(|n| *n == name) {
        Some(i) => push_handle(L, BUS_TAG, i as u32),
        None => err(L, &format!("audio.bus(name): \"master\", \"music\" or \"sfx\", got {}", to_s(L, 1))),
    }
});

/// The bus at stack index `i`.
pub unsafe fn bus_at(L: *mut lua_State, i: c_int) -> Option<usize> {
    unsafe { handle(L, i, BUS_TAG).map(|b| b as usize).filter(|b| *b < 3) }
}

/// A bus's number by name (tweens and springs move these).
pub fn bus_number(h: &Host, b: usize, key: &str) -> Option<f64> {
    let s = &h.sound.buses[b];
    Some(match key {
        "volume" => s.volume,
        "lowpass" => s.lowpass,
        "highpass" => s.highpass,
        _ => return None,
    } as f64)
}

pub fn set_bus_number(h: &mut Host, b: usize, key: &str, v: f64) {
    let v = v as f32;
    let (mute, bus) = (h.mute, BUSES[b]);
    let s = &mut h.sound.buses[b];
    match key {
        "volume" => {
            s.volume = v.clamp(0.0, 4.0);
            if !mute && bus == Bus::Music {
                music_mix::set(music_mix::GAME, s.volume, RAMP);
            } else if !mute {
                bus.volume(s.volume, RAMP);
            }
        }
        "lowpass" => {
            s.lowpass = v.clamp(20.0, OPEN);
            if !mute {
                bus.lowpass(s.lowpass, 0.707, RAMP);
            }
        }
        "highpass" => {
            s.highpass = v.clamp(20.0, OPEN);
            if !mute {
                bus.highpass(s.highpass, 0.707, RAMP);
            }
        }
        _ => return,
    }
    h.sound.touched = true;
}

/// A reverb by name, or a table of its fields.
unsafe fn reverb_at(L: *mut lua_State, v: c_int) -> Result<(String, Reverb), String> {
    unsafe {
        let preset = |n: &str| match n {
            "off" => Some(Reverb::OFF),
            "room" => Some(Reverb::ROOM),
            "hall" => Some(Reverb::HALL),
            "cave" => Some(Reverb::CAVE),
            _ => None,
        };
        match lua_type(L, v) {
            LUA_TNIL => Ok(("off".into(), Reverb::OFF)),
            LUA_TBOOLEAN if lua_toboolean(L, v) == 0 => Ok(("off".into(), Reverb::OFF)),
            LUA_TSTRING => {
                let n = to_s(L, v);
                preset(&n).map(|r| (n.clone(), r)).ok_or_else(|| format!("reverb is \"room\", \"hall\", \"cave\" or \"off\" (or {{ wet, decay, damping, predelay }}), got \"{n}\""))
            }
            LUA_TTABLE => {
                let d = Reverb::HALL;
                let f = |k: &std::ffi::CStr, d: f32| field_num(L, v, k).unwrap_or(d);
                let r = Reverb {
                    wet: f(c"wet", d.wet).clamp(0.0, 1.0),
                    decay: f(c"decay", d.decay).clamp(0.1, 10.0),
                    damping: f(c"damping", d.damping).clamp(0.0, 1.0),
                    predelay: f(c"predelay", d.predelay).clamp(0.0, 0.25),
                };
                Ok(("custom".into(), r))
            }
            _ => Err(format!("reverb is a name (\"hall\") or {{ wet, decay, damping, predelay }}, got {}", type_name(L, v))),
        }
    }
}

/// A delay from `{ time, feedback, wet, damping }` (nil or false: off).
unsafe fn delay_at(L: *mut lua_State, v: c_int) -> Result<Option<Delay>, String> {
    unsafe {
        match lua_type(L, v) {
            LUA_TNIL => Ok(None),
            LUA_TBOOLEAN if lua_toboolean(L, v) == 0 => Ok(None),
            LUA_TTABLE => {
                let f = |k: &std::ffi::CStr, d: f32| field_num(L, v, k).unwrap_or(d);
                Ok(Some(Delay {
                    wet: f(c"wet", 0.35).clamp(0.0, 1.0),
                    time: f(c"time", 0.3).clamp(0.01, 2.0),
                    feedback: f(c"feedback", 0.35).clamp(0.0, 0.9),
                    damping: f(c"damping", 0.3).clamp(0.0, 1.0),
                }))
            }
            _ => Err(format!("delay is {{ time, feedback }} (seconds between echoes, 0–0.9 each echo's level), or nil, got {}", type_name(L, v))),
        }
    }
}

unsafe fn field_num(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<f32> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let v = num(L, -1).filter(|v| v.is_finite()).map(|v| v as f32);
        lua_pop(L, 1);
        v
    }
}

b!(bus_index, |L, h| {
    let Some(b) = bus_at(L, 1) else { return err(L, "not a bus") };
    let key = to_s(L, 2);
    if let Some(v) = bus_number(h, b, &key) {
        return ret(L, v);
    }
    let s = &h.sound.buses[b];
    match key.as_str() {
        "name" => rets(L, BUS_NAMES[b]),
        "reverb" => rets(L, &s.reverb.0),
        "delay" => match s.delay {
            Some(d) => {
                lua_createtable(L, 0, 4);
                for (k, v) in [(c"time", d.time), (c"feedback", d.feedback), (c"wet", d.wet), (c"damping", d.damping)] {
                    lua_pushnumber(L, v as f64);
                    lua_setfield(L, -2, k.as_ptr());
                }
                1
            }
            None => {
                lua_pushnil(L);
                1
            }
        },
        _ => err(L, &unknown_bus(&key)),
    }
});

fn unknown_bus(key: &str) -> String {
    const P: &[&str] = &["volume", "lowpass", "highpass", "reverb", "delay", "name"];
    match P.iter().find(|n| n.eq_ignore_ascii_case(key) || (key.len() > 3 && key.is_char_boundary(3) && n.starts_with(&key[..3]))) {
        Some(n) => format!("no property `{key}` on a bus; did you mean `{n}`?"),
        None => format!("no property `{key}` on a bus: it has volume, lowpass, highpass (Hz), reverb and delay"),
    }
}

b!(bus_newindex, |L, h| {
    let Some(b) = bus_at(L, 1) else { return err(L, "not a bus") };
    let key = to_s(L, 2);
    match key.as_str() {
        "volume" | "lowpass" | "highpass" => {
            let Some(v) = num(L, 3).filter(|v| v.is_finite()) else {
                return err(L, &format!("bus.{key} takes a number{}, got {}", if key == "volume" { " (0–4, 1 is as is)" } else { " of Hz (20–20000)" }, to_s(L, 3)));
            };
            set_bus_number(h, b, &key, v);
            h.libs.written(crate::libs::Target::Bus(b as u8), &key);
        }
        "reverb" => match reverb_at(L, 3) {
            Ok(r) => {
                if !h.mute {
                    BUSES[b].reverb(r.1, 0.3);
                }
                h.sound.buses[b].reverb = r;
                h.sound.touched = true;
            }
            Err(m) => return err(L, &format!("bus.{m}")),
        },
        "delay" => match delay_at(L, 3) {
            Ok(d) => {
                if !h.mute {
                    BUSES[b].delay(d.unwrap_or(Delay::OFF), 0.1);
                }
                h.sound.buses[b].delay = d;
                h.sound.touched = true;
            }
            Err(m) => return err(L, &format!("bus.{m}")),
        },
        "name" => return err(L, "a bus's name is read-only: audio.bus(\"music\") is another bus"),
        _ => return err(L, &unknown_bus(&key)),
    }
    0
});

// ---------------------------------------------------------------- placed sound

/// Where the round is heard from: `love.audio.setPosition`'s point, else
/// the camera (the 3D camera, or the middle of a 2D game's view).
fn listener(h: &Host) -> Listener {
    #[cfg(three)]
    if let Some(w) = &h.three {
        let mut l = Listener::from_camera(&w.camera());
        if let Some([x, y, z]) = h.sound.listener {
            l.pos = vec3(x, y, z);
        }
        return l;
    }
    let c = &h.scene.camera;
    let pos = h.sound.listener.map_or(vec3(c.x, c.y, 0.0), |[x, y, z]| vec3(x, y, z));
    Listener { pos, right: vec3(c.rot.cos(), c.rot.sin(), 0.0) }
}

/// How far sound carries: the SDK's meters in 3D; in 2D, full within
/// about half the canvas's width, fading out over three.
fn space(h: &Host) -> Space {
    #[cfg(three)]
    if h.three.is_some() {
        return Space::default();
    }
    let w = h.project.size.0 as f32;
    Space { near: w * 0.5, far: w * 3.0, pan_width: 0.6 }
}

/// A source's level and pan, from where it is (1 and the middle when it isn't placed).
pub fn placed(h: &Host, pos: Option<[f32; 3]>) -> (f32, f32) {
    match pos {
        Some([x, y, z]) => space(h).place(&listener(h), vec3(x, y, z)),
        None => (1.0, 0.0),
    }
}

b!(s_setPosition, |L, h| {
    let Some(i) = handle(L, 1, tag::SOURCE).map(|s| s as usize).filter(|s| *s < h.sources.len()) else { return bad(L, 1, "setPosition", "Source") };
    let (Some(x), Some(y)) = (num(L, 2), num(L, 3)) else { return err(L, "setPosition(x, y, z): where the sound is, in the world (z is 0 if left out)") };
    h.sources[i].pos = Some([x as f32, y as f32, opt(L, 4, 0.0) as f32]);
    0
});

b!(s_getPosition, |L, h| {
    let Some(i) = handle(L, 1, tag::SOURCE).map(|s| s as usize).filter(|s| *s < h.sources.len()) else { return bad(L, 1, "getPosition", "Source") };
    let p = h.sources[i].pos.unwrap_or([0.0; 3]);
    for v in p {
        lua_pushnumber(L, v as f64);
    }
    3
});

b!(a_setPosition, |L, h| {
    h.sound.listener = match (num(L, 1), num(L, 2)) {
        (Some(x), Some(y)) => Some([x as f32, y as f32, opt(L, 3, 0.0) as f32]),
        _ if lua_gettop(L) == 0 || lua_type(L, 1) == LUA_TNIL => None,
        _ => return err(L, "love.audio.setPosition(x, y, z): where the round is heard from (nil: the camera again)"),
    };
    0
});

b!(a_getPosition, |L, h| {
    let p = listener(h).pos;
    for v in [p.x, p.y, p.z] {
        lua_pushnumber(L, v as f64);
    }
    3
});

// `audio.impact(src, speed, fullSpeed)`: a clone of the source, louder and
// heavier the harder the hit, where the source is placed.
b!(impact, |L, h| {
    let Some(i) = handle(L, 1, tag::SOURCE).map(|s| s as usize).filter(|s| *s < h.sources.len()) else { return bad(L, 1, "audio.impact", "Source") };
    let Some(speed) = num(L, 2) else { return err(L, "audio.impact(src, speed, fullSpeed): speed is how fast it hit (units or meters a second)") };
    let full = opt(L, 3, if cfg!(three) && h_three(h) { 12.0 } else { 600.0 });
    let s = h.sources[i].clone();
    let Some((vol, pitch)) = impact_level(speed as f32, full as f32) else { return retb(L, false) };
    let at = match s.pos {
        Some([x, y, z]) => vec3(x, y, z),
        None => listener(h).pos,
    };
    frame_clock(h);
    let played = h.sound.foley.hit(Sound(s.sound as u32), at, vol * s.volume, pitch * s.pitch, 1.0).is_some();
    retb(L, played)
});

fn h_three(h: &Host) -> bool {
    #[cfg(three)]
    return h.three.is_some();
    #[cfg(not(three))]
    {
        let _ = h;
        false
    }
}

/// Foley's state as of now: the decoded sounds, the listener and the space.
fn frame_clock(h: &mut Host) {
    let (l, sp) = (listener(h), space(h));
    let s = &mut h.sound;
    s.foley.mixer.now = h.time;
    s.foley.mixer.handles.clone_from(&h.sounds);
    s.foley.space = sp;
    // The listener moves with the camera; the clock is kept by `frame`.
    s.foley.update(0.0, l);
}

// ---------------------------------------------------------------- foley on nodes

/// `node.footsteps` and `node.hum` (node.rs, three.rs): the sound's name, or nil.
pub fn get_foley(h: &Host, node: FoleyNode, hum: bool) -> Option<String> {
    let n = h.sound.noisy.iter().find(|n| n.node == node)?;
    let s = if hum { n.hum.as_ref().map(|h| h.sound) } else { n.footsteps }?;
    h.project.sounds.get(s).map(|x| x.0.clone())
}

/// Sets `node.footsteps` or `node.hum` from the value at `v`: a sound's
/// name, or nil to stop. A hum also takes `{ sound, idle, full }` (speeds:
/// silent at `idle` and under, loudest from `full`).
pub unsafe fn set_foley(L: *mut lua_State, h: &mut Host, node: FoleyNode, hum: bool, v: c_int) -> Result<(), String> {
    unsafe {
        let key = if hum { "hum" } else { "footsteps" };
        let three = h_three(h);
        let (mut idle, mut full) = if three { (0.3, 8.0) } else { (10.0, 300.0) };
        let name = match lua_type(L, v) {
            LUA_TNIL => None,
            LUA_TBOOLEAN if lua_toboolean(L, v) == 0 => None,
            LUA_TSTRING => Some(to_s(L, v)),
            LUA_TTABLE if hum => {
                lua_rawgetfield(L, v, c"sound".as_ptr());
                let n = st(L, -1).map(str::to_string);
                lua_pop(L, 1);
                idle = field_num(L, v, c"idle").unwrap_or(idle);
                full = field_num(L, v, c"full").unwrap_or(full).max(idle + 1e-3);
                Some(n.ok_or("hum's table needs sound = \"name\"")?)
            }
            _ if hum => return Err(format!("hum takes a sound's name (\"servo\") or {{ sound, idle, full }}, or nil, got {}", type_name(L, v))),
            _ => return Err(format!("footsteps takes a sound's name (\"step_moss\"), or nil, got {}", type_name(L, v))),
        };
        let sound = match &name {
            Some(n) => {
                let n = n.strip_prefix("sounds/").unwrap_or(n).trim_end_matches(".ogg");
                Some(h.project.sound(n).ok_or_else(|| format!("{key}: no sound \"{n}\": add sounds/{n}.ogg, or a synth sound sounds/{n}.sfx.toml"))?)
            }
            None => None,
        };
        let i = match h.sound.noisy.iter().position(|n| n.node == node) {
            Some(i) => i,
            None if sound.is_none() => return Ok(()),
            None => {
                if h.sound.noisy.len() >= MAX_FOLEY {
                    return Err(format!("{key}: more than {MAX_FOLEY} nodes with foley in a round"));
                }
                h.sound.noisy.push(Noisy { node, footsteps: None, hum: None, last: None, speed: 0.0, feet: [Contact::with(0.03, 0.08); 2] });
                h.sound.noisy.len() - 1
            }
        };
        let s = &mut h.sound;
        if hum {
            if let Some(mut old) = s.noisy[i].hum.take() {
                old.drive.stop(&mut s.foley);
            }
            s.noisy[i].hum = sound.map(|snd| {
                let mut drive = Drive::thruster(Sound(snd as u32));
                (drive.idle, drive.full) = (idle, full);
                Hum { sound: snd, drive }
            });
        } else {
            s.noisy[i].footsteps = sound;
        }
        if s.noisy[i].hum.is_none() && s.noisy[i].footsteps.is_none() {
            s.noisy.remove(i);
        }
        Ok(())
    }
}

/// A footfall a node made this frame: a Walker's (scene events), a gait's
/// (its `stepped`), with how hard (0–1).
#[derive(Clone, Copy, Debug)]
pub struct Footfall {
    pub node: FoleyNode,
    pub at: Vec3,
    pub weight: f32,
}

/// One frame of the round's sound, after the world and the camera moved:
/// the footfalls, the feet of plantFeet models (whose touchdowns are also
/// their `stepped` signal), the hums, and placed sources following the
/// listener. `dt` is real time, `world_dt` the world's (slowed, held).
pub fn frame(h: &mut Host, dt: f32, world_dt: f32, steps: &[Footfall]) {
    frame_clock(h);
    h.sound.foley.update(dt, listener(h));
    // Placed sources that are playing follow the listener.
    if !h.mute {
        for i in 0..h.sources.len() {
            let s = &h.sources[i];
            if let (Some(v), Some(_)) = (s.voice, s.pos) {
                let (g, pan) = placed(h, s.pos);
                v.set(s.volume * g, pan, s.pitch);
            }
        }
    }
    if h.sound.noisy.is_empty() {
        return;
    }
    for f in steps {
        if let Some(snd) = h.sound.noisy.iter().find(|n| n.node == f.node).and_then(|n| n.footsteps) {
            h.sound.foley.step(Sound(snd as u32), f.at, f.weight);
        }
    }
    let mut gone = Vec::new();
    for i in 0..h.sound.noisy.len() {
        let node = h.sound.noisy[i].node;
        let Some(pos) = position(h, node) else {
            gone.push(i);
            continue;
        };
        let n = &mut h.sound.noisy[i];
        if world_dt > 0.0 {
            n.speed = n.last.map_or(0.0, |l| (pos - l).length() / world_dt);
            n.last = Some(pos);
        }
        let speed = n.speed;
        #[cfg(three)]
        if let (FoleyNode::D3(id), Some(snd)) = (node, n.footsteps)
            && world_dt > 0.0
            && let Some(feet) = h.three.as_mut().and_then(|w| w.feet(id))
        {
            for (k, (at, height)) in feet.into_iter().enumerate() {
                if let Some(down) = h.sound.noisy[i].feet[k].update(height, world_dt) {
                    // A walk lands at about 1 m/s, a run at 2–3.
                    h.sound.foley.step(Sound(snd as u32), at, (down / 2.5).clamp(0.2, 1.0));
                    h.libs.pending.push(crate::libs::Due::Signal3 { node: id, name: "stepped", args: vec![k as f64 + 1.0, at.x as f64, at.y as f64, at.z as f64, (down / 2.5).clamp(0.0, 1.0) as f64] });
                }
            }
        }
        let s = &mut h.sound;
        if let Some(hm) = &mut s.noisy[i].hum {
            hm.drive.update(&mut s.foley, speed, pos, dt);
        }
    }
    for i in gone.into_iter().rev() {
        let mut n = h.sound.noisy.remove(i);
        if let Some(hm) = &mut n.hum {
            hm.drive.stop(&mut h.sound.foley);
        }
    }
}

/// Where a node with foley is in the world (None once it's gone).
fn position(h: &mut Host, node: FoleyNode) -> Option<Vec3> {
    match node {
        FoleyNode::D2(id) => {
            if !h.scene.alive(id) {
                return None;
            }
            let (x, y) = h.scene.world_position(id);
            Some(vec3(x, y, 0.0))
        }
        #[cfg(three)]
        FoleyNode::D3(id) => {
            let w = h.three.as_mut()?;
            if !w.scene.alive(id) {
                return None;
            }
            w.world_of(id).map(|m| vec3(m.0[12], m.0[13], m.0[14]))
        }
    }
}
