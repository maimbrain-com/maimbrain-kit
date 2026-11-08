//! `love.audio` sources (docs/STUDIO-2.md §5 [love], [sound]): the
//! project's sounds by name (`sounds/<name>.ogg`, or a `.sfx.toml` the
//! packer renders), played through the host's audio. Sound is output only,
//! so nothing here changes how a round plays or replays: `isPlaying` and
//! `tell` are estimates from game time and the sound's length (the packer
//! records it), and a sound that hasn't loaded yet plays silently.
//!
//! Also the plain music loop (`music/<name>.ogg` or a
//! `.song.toml`) that `music.play` asks the engine for (music.rs), which it carries from round to round, and the
//! haptics: `love.system.vibrate` and `haptic(kind)`.

use std::ffi::{c_int, c_void};

use maimbrain::audio::Voice;
use maimbrain::sensors::Haptic;
use maimbrain::sys;

use super::*;

/// One `love.audio.newSource`.
#[derive(Clone, Debug)]
pub struct Source {
    pub sound: usize,
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
    pub voice: Option<Voice>,
    /// Game time it (re)started, and where in the sound it started from.
    pub started: f64,
    pub from: f64,
    /// Paused at this point in the sound.
    pub paused: Option<f64>,
    pub playing: bool,
    /// `setPosition`: where it is in the world (sound.rs places it).
    pub pos: Option<[f32; 3]>,
}

/// What the round asked of the music (the engine plays it, across rounds).
#[derive(Clone, Debug, PartialEq)]
pub enum Music {
    Play { name: String, fade: f32, volume: f32 },
    Stop { fade: f32 },
}

/// Sources a round may make.
const MAX_SOURCES: usize = 512;

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(
            L,
            ud,
            "love.audio",
            &[(c"newSource", newSource), (c"play", a_play), (c"stop", a_stop), (c"pause", a_pause), (c"setVolume", a_setVolume), (c"getVolume", a_getVolume), (c"setPosition", noop), (c"getActiveSourceCount", a_active)],
        );
        register_methods(
            L,
            ud,
            tag::SOURCE,
            c"Source",
            &[
                (c"play", s_play),
                (c"stop", s_stop),
                (c"pause", s_pause),
                (c"resume", s_play),
                (c"clone", s_clone),
                (c"isPlaying", s_isPlaying),
                (c"isLooping", s_isLooping),
                (c"setLooping", s_setLooping),
                (c"setVolume", s_setVolume),
                (c"getVolume", s_getVolume),
                (c"setPitch", s_setPitch),
                (c"getPitch", s_getPitch),
                (c"setPosition", super::sound::s_setPosition),
                (c"getPosition", super::sound::s_getPosition),
                (c"tell", s_tell),
                (c"seek", s_seek),
                (c"getDuration", s_getDuration),
                (c"typeOf", super::graphics::type_of),
            ],
        );
        register(L, ud, "love.system", &[(c"vibrate", vibrate)]);
        sv_push_rfn(L, haptic, ud, c"haptic".as_ptr());
        lua_setfield(L, LUA_GLOBALSINDEX, c"haptic".as_ptr());
    }
}

b!(noop, |L, h| 0);

/// Where a source is in its sound now (seconds of sound).
fn position(h: &Host, s: &Source) -> f64 {
    if let Some(p) = s.paused {
        return p;
    }
    if !s.playing {
        return 0.0;
    }
    let len = h.project.sounds.get(s.sound).map_or(0.0, |x| x.1 as f64);
    let at = s.from + (h.time - s.started) * s.pitch as f64;
    if s.looping && len > 0.0 { at % len } else { at }
}

fn still_playing(h: &Host, s: &Source) -> bool {
    if !s.playing || s.paused.is_some() {
        return false;
    }
    let len = h.project.sounds.get(s.sound).map_or(0.0, |x| x.1 as f64);
    s.looping || len <= 0.0 || position(h, s) < len
}

fn start(h: &mut Host, i: usize, from: f64) {
    let (sound, volume, pitch, looping) = {
        let s = &h.sources[i];
        (s.sound, s.volume, s.pitch, s.looping)
    };
    if let Some(v) = h.sources[i].voice.take() {
        v.stop();
    }
    // A placed source is as loud and as far to a side as it is from the listener.
    let (gain, pan) = super::sound::placed(h, h.sources[i].pos);
    let volume = volume * gain;
    if !h.mute {
        super::sound::log_source(h, i, volume, pitch, pan, looping);
    }
    let voice = match h.sounds.get(sound).copied().flatten() {
        Some(snd) if !h.mute => {
            let now = sys::time();
            Some(match (looping, from > 0.0) {
                (false, false) => snd.play(volume, pan, pitch),
                (true, false) => snd.play_looped(volume, pan, pitch),
                (false, true) => snd.play_from(volume, pan, pitch, now, from, 0.0),
                (true, true) => snd.play_looped_from(volume, pan, pitch, now, from, 0.0),
            })
        }
        _ => None,
    };
    let time = h.time;
    let s = &mut h.sources[i];
    (s.voice, s.started, s.from, s.paused, s.playing) = (voice, time, from, None, true);
}

fn stop(h: &mut Host, i: usize) {
    let s = &mut h.sources[i];
    if let Some(v) = s.voice.take() {
        v.stop();
    }
    (s.playing, s.paused) = (false, None);
}

unsafe fn source(L: *mut lua_State, h: &Host, i: c_int, f: &str) -> Result<usize, c_int> {
    unsafe {
        match handle(L, i, tag::SOURCE).map(|s| s as usize).filter(|s| *s < h.sources.len()) {
            Some(s) => Ok(s),
            None => Err(bad(L, i, f, "Source")),
        }
    }
}

macro_rules! src {
    ($L:ident, $h:ident, $f:literal) => {
        match source($L, $h, 1, $f) {
            Ok(s) => s,
            Err(r) => return r,
        }
    };
}

b!(newSource, |L, h| {
    let Some(name) = st(L, 1) else { return bad(L, 1, "newSource", "sound name") };
    match st(L, 2) {
        None | Some("static" | "stream") => {}
        Some("queue") => return err(L, "newSource: \"queue\" sources aren't in Studio (no sample access)"),
        Some(t) => return err(L, &format!("newSource: the type is \"static\" or \"stream\", got \"{t}\"")),
    }
    let Some(sound) = h.project.sound(name) else {
        let n = name.strip_prefix("sounds/").unwrap_or(name).trim_end_matches(".ogg");
        return err(L, &format!("no sound \"{n}\": add sounds/{n}.ogg, or a synth sound sounds/{n}.sfx.toml"));
    };
    if h.sources.len() >= MAX_SOURCES {
        return err(L, &format!("more than {MAX_SOURCES} sources in a round: make each once (in love.load) and clone it to overlap"));
    }
    h.sources.push(Source { sound, volume: 1.0, pitch: 1.0, looping: false, voice: None, started: 0.0, from: 0.0, paused: None, playing: false, pos: None });
    push_handle(L, tag::SOURCE, h.sources.len() as u32 - 1)
});

b!(s_play, |L, h| {
    let i = src!(L, h, "play");
    let s = &h.sources[i];
    if let Some(p) = s.paused {
        start(h, i, p);
    } else if !still_playing(h, s) {
        start(h, i, 0.0);
    }
    retb(L, true)
});
b!(s_stop, |L, h| {
    let i = src!(L, h, "stop");
    stop(h, i);
    0
});
b!(s_pause, |L, h| {
    let i = src!(L, h, "pause");
    if still_playing(h, &h.sources[i]) {
        let p = position(h, &h.sources[i]);
        if let Some(v) = h.sources[i].voice.take() {
            v.stop();
        }
        h.sources[i].paused = Some(p);
    }
    0
});
b!(s_clone, |L, h| {
    let i = src!(L, h, "clone");
    if h.sources.len() >= MAX_SOURCES {
        return err(L, &format!("more than {MAX_SOURCES} sources in a round: clone fewer, or reuse the clones"));
    }
    let s = Source { voice: None, playing: false, paused: None, ..h.sources[i].clone() };
    h.sources.push(s);
    push_handle(L, tag::SOURCE, h.sources.len() as u32 - 1)
});
b!(s_isPlaying, |L, h| {
    let i = src!(L, h, "isPlaying");
    retb(L, still_playing(h, &h.sources[i]))
});
b!(s_isLooping, |L, h| {
    let i = src!(L, h, "isLooping");
    retb(L, h.sources[i].looping)
});
b!(s_setLooping, |L, h| {
    let i = src!(L, h, "setLooping");
    let on = lua_toboolean(L, 2) != 0;
    if h.sources[i].looping != on {
        h.sources[i].looping = on;
        if still_playing(h, &h.sources[i]) {
            let p = position(h, &h.sources[i]);
            start(h, i, p);
        }
    }
    0
});

/// Volume or pitch changes reach a playing voice.
fn apply(h: &mut Host, i: usize) {
    let s = &h.sources[i];
    if let Some(v) = s.voice {
        let (gain, pan) = super::sound::placed(h, s.pos);
        v.set(s.volume * gain, pan, s.pitch);
    }
}

b!(s_setVolume, |L, h| {
    let i = src!(L, h, "setVolume");
    let Some(v) = num(L, 2).filter(|v| (0.0..=1.0).contains(v)) else { return err(L, "setVolume takes 0–1") };
    h.sources[i].volume = v as f32;
    apply(h, i);
    0
});
b!(s_getVolume, |L, h| {
    let i = src!(L, h, "getVolume");
    ret(L, h.sources[i].volume as f64)
});
b!(s_setPitch, |L, h| {
    let i = src!(L, h, "setPitch");
    let Some(p) = num(L, 2).filter(|p| *p > 0.0 && *p <= 8.0) else { return err(L, "setPitch takes a number above 0 (1 is as recorded, 2 an octave up), up to 8") };
    // Keep the position when the rate changes.
    if still_playing(h, &h.sources[i]) {
        let pos = position(h, &h.sources[i]);
        let t = h.time;
        let s = &mut h.sources[i];
        (s.from, s.started) = (pos, t);
    }
    h.sources[i].pitch = p as f32;
    apply(h, i);
    0
});
b!(s_getPitch, |L, h| {
    let i = src!(L, h, "getPitch");
    ret(L, h.sources[i].pitch as f64)
});
b!(s_tell, |L, h| {
    let i = src!(L, h, "tell");
    ret(L, position(h, &h.sources[i]))
});
b!(s_seek, |L, h| {
    let i = src!(L, h, "seek");
    let to = opt(L, 2, 0.0).max(0.0);
    if still_playing(h, &h.sources[i]) {
        start(h, i, to);
    } else if h.sources[i].paused.is_some() {
        h.sources[i].paused = Some(to);
    }
    0
});
b!(s_getDuration, |L, h| {
    let i = src!(L, h, "getDuration");
    ret(L, h.project.sounds.get(h.sources[i].sound).map_or(0.0, |s| s.1 as f64))
});

/// love.audio.play/stop/pause: the sources given, or (stop, pause) all.
unsafe fn each(L: *mut lua_State, h: &mut Host, f: fn(&mut Host, usize)) -> c_int {
    unsafe {
        let n = lua_gettop(L);
        if n == 0 {
            for i in 0..h.sources.len() {
                f(h, i);
            }
            return 0;
        }
        for k in 1..=n {
            match source(L, h, k, "love.audio") {
                Ok(i) => f(h, i),
                Err(r) => return r,
            }
        }
        0
    }
}

b!(a_play, |L, h| {
    if lua_gettop(L) == 0 {
        return bad(L, 1, "love.audio.play", "Source");
    }
    each(L, h, |h, i| {
        if !still_playing(h, &h.sources[i]) {
            start(h, i, h.sources[i].paused.unwrap_or(0.0));
        }
    })
});
b!(a_stop, |L, h| each(L, h, stop));
b!(a_pause, |L, h| each(L, h, |h, i| {
    if still_playing(h, &h.sources[i]) {
        let p = position(h, &h.sources[i]);
        if let Some(v) = h.sources[i].voice.take() {
            v.stop();
        }
        h.sources[i].paused = Some(p);
    }
}));
b!(a_setVolume, |L, h| match num(L, 1).filter(|v| (0.0..=1.0).contains(v)) {
    Some(v) => {
        h.volume = v as f32;
        // The sfx bus's volume (`audio.bus("sfx").volume`).
        super::sound::set_bus_number(h, 2, "volume", v);
        0
    }
    None => err(L, "love.audio.setVolume takes 0–1"),
});
b!(a_getVolume, |L, h| ret(L, h.volume as f64));
b!(a_active, |L, h| {
    let n = (0..h.sources.len()).filter(|i| still_playing(h, &h.sources[*i])).count();
    ret(L, n as f64)
});

/// A number field of an options table at `i` (or the default).
pub(super) unsafe fn opt_field(L: *mut lua_State, i: c_int, k: &std::ffi::CStr, d: f32) -> f32 {
    unsafe {
        if lua_type(L, i) != LUA_TTABLE {
            return d;
        }
        lua_rawgetfield(L, i, k.as_ptr());
        let v = num(L, -1).map_or(d, |v| v as f32);
        lua_pop(L, 1);
        v
    }
}

b!(vibrate, |L, h| {
    if !h.mute {
        let secs = opt(L, 1, 0.5);
        maimbrain::sensors::haptic(if secs >= 0.3 { Haptic::Heavy } else { Haptic::Tap });
    }
    0
});

b!(haptic, |L, h| {
    let k = match st(L, 1).unwrap_or("tap") {
        "tap" => Haptic::Tap,
        "success" => Haptic::Success,
        "fail" => Haptic::Fail,
        "heavy" => Haptic::Heavy,
        o => return err(L, &format!("haptic(kind): \"tap\", \"success\", \"fail\" or \"heavy\", got \"{o}\"")),
    };
    if !h.mute {
        maimbrain::sensors::haptic(k);
    }
    0
});
