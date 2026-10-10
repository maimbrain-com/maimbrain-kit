//! Sound playback (SPEC §5.6). Audio is output-only: it never feeds back
//! into game state, so it doesn't affect replays.
//!
//! [`Sound::play`] starts a voice as soon as it can. [`Sound::play_at`] makes
//! it *heard* when game time ([`crate::sys::time`]) reaches `at`: the platform
//! accounts for output latency and for when the frame is on screen, and a
//! voice that can only start late (its sound still decoding, `at` already
//! past) starts part-way into the sound so it stays on time. Use it for
//! anything that must land on a beat (music, rhythm cues).
//! [`Sound::play_from`] does the same from any point of the sound, fading in,
//! and [`Voice::stop_at`] lets a voice die away from a given game time: with
//! the two, a player can move through a long take bar by bar
//! ([`crate::music::Director`] with a music graph).
//!
//! Every voice plays into a [`Bus`]: [`Bus::Sfx`] unless [`Voice::bus`] moves
//! it, usually to [`Bus::Music`]. Each bus has a low-pass and a high-pass
//! filter, a reverb, a delay and a volume, and every change glides over the
//! `ramp` you give, so they can follow the game smoothly: muffle the music
//! under water, open its filter as the speed rises, add a long reverb in a
//! tunnel. Like the rest of audio they're output only.
//!
//! ```ignore
//! let music = theme.play_looped_at(1.0, 0.0, 1.0, origin).bus(Bus::Music);
//! Bus::Music.lowpass(600.0, 1.2, 0.4);           // dive: muffled over 0.4 s
//! Bus::Music.lowpass(OPEN, 0.707, 0.8);          // surface: open again
//! Bus::Master.reverb(Reverb::CAVE, 0.5);         // into the tunnel
//! Bus::Master.reverb(Reverb::OFF, 1.0);
//! ```

use crate::ffi;
use crate::sys::Asset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sound(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice(pub u32);

impl Sound {
    /// None until the asset (.ogg) is ready.
    pub fn new(asset: Asset) -> Option<Sound> {
        let h = unsafe { ffi::mb_sound(asset.0) };
        (h > 0).then_some(Sound(h as u32))
    }

    /// Starts now. `vol` 0–4, `pan` −1…1, `pitch` as a playback-rate
    /// multiplier. Skipped if the sound hasn't finished decoding.
    pub fn play(self, vol: f32, pan: f32, pitch: f32) -> Voice {
        Voice(unsafe { ffi::mb_play(self.0, vol, pan, pitch, 0) })
    }

    /// Loops from now (or from when the sound has decoded, if later).
    pub fn play_looped(self, vol: f32, pan: f32, pitch: f32) -> Voice {
        Voice(unsafe { ffi::mb_play(self.0, vol, pan, pitch, 1) })
    }

    /// Heard from game time `at` (seconds, like [`crate::sys::time`]). If it
    /// can only start after `at`, it starts that far into the sound; skipped
    /// if that's more than half of it. Schedule ~0.1 s or more ahead so it
    /// starts from the top.
    pub fn play_at(self, vol: f32, pan: f32, pitch: f32, at: f64) -> Voice {
        Voice(unsafe { ffi::mb_play_at(self.0, vol, pan, pitch, 0, at) })
    }

    /// Loops with its start heard at game time `at`. Started late, it joins
    /// where the loop would be by then, so loops started with the same `at`
    /// (stems) stay sample-aligned with each other and with the game's grid.
    pub fn play_looped_at(self, vol: f32, pan: f32, pitch: f32, at: f64) -> Voice {
        Voice(unsafe { ffi::mb_play_at(self.0, vol, pan, pitch, 1, at) })
    }

    /// Heard from game time `at`, starting `from` seconds into the sound,
    /// its level rising from silence to `vol` over the first `fade` seconds
    /// (a quarter sine; 0 = at once, with a few ms of ramp). Like
    /// [`Sound::play_at`], a voice that can only start late starts further in
    /// (and further along the fade), so it stays on the grid. This is how a
    /// music player enters a long take at any bar: the new voice comes in
    /// under the old one, which [`Voice::stop_at`] lets die away.
    pub fn play_from(self, vol: f32, pan: f32, pitch: f32, at: f64, from: f64, fade: f32) -> Voice {
        Voice(unsafe { ffi::mb_play_from(self.0, vol, pan, pitch, 0, at, from, fade) })
    }

    /// [`Sound::play_from`], looping: past the end it wraps to the start.
    pub fn play_looped_from(self, vol: f32, pan: f32, pitch: f32, at: f64, from: f64, fade: f32) -> Voice {
        Voice(unsafe { ffi::mb_play_from(self.0, vol, pan, pitch, 1, at, from, fade) })
    }
}

impl Voice {
    pub fn set(self, vol: f32, pan: f32, pitch: f32) {
        unsafe { ffi::mb_voice_set(self.0, vol, pan, pitch) }
    }

    pub fn stop(self) {
        unsafe { ffi::mb_voice_stop(self.0) }
    }

    /// Lets the voice die away from game time `at`, like a note let go: its
    /// level falls 60 dB over `fade` seconds (an exponential release: about
    /// −15 dB a quarter of the way), then it stops. `fade` 0 cuts it at `at`
    /// with a few ms of ramp. `at` in the past means now. A later call
    /// replaces one that hasn't begun; [`Voice::stop`] still stops it at once.
    pub fn stop_at(self, at: f64, fade: f32) {
        unsafe { ffi::mb_voice_stop_at(self.0, at, fade) }
    }

    /// Plays this voice into `bus` from now on (call it right after starting
    /// the voice). Returns the voice, so it chains:
    /// `sound.play_looped_at(1.0, 0.0, 1.0, at).bus(Bus::Music)`.
    pub fn bus(self, bus: Bus) -> Voice {
        unsafe { ffi::mb_voice_bus(self.0, bus as u32) };
        self
    }
}

/// A low-pass cutoff (Hz) at or above which the filter is open (no effect).
pub const OPEN: f32 = 20000.0;

/// A mixing bus with its own effects (SPEC §5.6). Music and Sfx both feed
/// Master, so an effect on Master applies to everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Bus {
    Master = 0,
    Music = 1,
    /// Where every voice starts.
    Sfx = 2,
}

/// The reverb on a bus: a generated room added to the dry sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reverb {
    /// 0–1: how loud the reverb is next to the dry sound (1 ≈ as loud); 0 is off.
    pub wet: f32,
    /// Seconds for the tail to fall by 60 dB, 0.1–10.
    pub decay: f32,
    /// 0–1: how fast the highs die away (0 bright tile and metal, 1 dark and soft).
    pub damping: f32,
    /// Seconds before the reverb starts, 0–0.25: bigger spaces have more.
    pub predelay: f32,
}

impl Reverb {
    pub const OFF: Reverb = Reverb { wet: 0.0, decay: 1.5, damping: 0.5, predelay: 0.02 };
    /// A small room: short and a little dull.
    pub const ROOM: Reverb = Reverb { wet: 0.2, decay: 0.8, damping: 0.6, predelay: 0.01 };
    /// A hall or a temple: long and smooth.
    pub const HALL: Reverb = Reverb { wet: 0.25, decay: 2.5, damping: 0.45, predelay: 0.025 };
    /// A tunnel or a cave: long, bright, with a late first reflection.
    pub const CAVE: Reverb = Reverb { wet: 0.35, decay: 4.0, damping: 0.25, predelay: 0.05 };
}

/// The echo on a bus: repeats `time` apart, each `feedback` as loud as the last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delay {
    /// 0–1: how loud the echoes are; 0 is off.
    pub wet: f32,
    /// Seconds between echoes, 0.01–2. On the beat sounds musical: `60.0 / bpm * 0.75` is a dotted eighth.
    pub time: f32,
    /// 0–0.9: each echo's level relative to the one before.
    pub feedback: f32,
    /// 0–1: how much darker each echo gets.
    pub damping: f32,
}

impl Delay {
    pub const OFF: Delay = Delay { wet: 0.0, time: 0.3, feedback: 0.35, damping: 0.3 };
}

/// What `mb_bus_fx` reads (32 bytes): `ramp`, then the kind's fields.
#[repr(C)]
struct Fx {
    ramp: f32,
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    _reserved: [u32; 3],
}

const LOWPASS: u32 = 1;
const HIGHPASS: u32 = 2;
const REVERB: u32 = 3;
const DELAY: u32 = 4;
const VOLUME: u32 = 5;

impl Bus {
    fn fx(self, kind: u32, ramp: f32, a: f32, b: f32, c: f32, d: f32) {
        let fx = Fx { ramp, a, b, c, d, _reserved: [0; 3] };
        unsafe { ffi::mb_bus_fx(self as u32, kind, (&fx as *const Fx).cast()) };
    }

    /// Cuts everything above `cutoff` Hz (20–20 000; [`OPEN`] turns it off),
    /// gliding there over `ramp` seconds at an even rate in octaves.
    /// `resonance` is the filter's Q: 0.707 is flat, 1–3 adds a bump at the
    /// cutoff (a "wah" as it moves), 8 and more rings. Under water is about
    /// 500 Hz; "through a wall" about 1500 Hz.
    pub fn lowpass(self, cutoff: f32, resonance: f32, ramp: f32) {
        self.fx(LOWPASS, ramp, cutoff, resonance, 0.0, 0.0);
    }

    /// Cuts everything below `cutoff` Hz (20 and under turns it off): ~300 Hz
    /// thins music out under a voice line, ~800 Hz is a cheap radio.
    pub fn highpass(self, cutoff: f32, resonance: f32, ramp: f32) {
        self.fx(HIGHPASS, ramp, cutoff, resonance, 0.0, 0.0);
    }

    /// Sets the bus's reverb. `wet` and `predelay` glide over `ramp`; a new
    /// `decay` or `damping` crossfades to a new room over `ramp`, at most ~5
    /// times a second, so animate `wet` rather than `decay`.
    pub fn reverb(self, r: Reverb, ramp: f32) {
        self.fx(REVERB, ramp, r.wet, r.decay, r.damping, r.predelay);
    }

    /// Sets the bus's echo. Every field glides over `ramp`; a gliding `time`
    /// bends the echoes' pitch like tape.
    pub fn delay(self, d: Delay, ramp: f32) {
        self.fx(DELAY, ramp, d.wet, d.time, d.feedback, d.damping);
    }

    /// The bus's volume, 0–4 (1 is as is), effects included: fade all the
    /// music out behind a menu, or duck it under a big moment.
    pub fn volume(self, gain: f32, ramp: f32) {
        self.fx(VOLUME, ramp, gain, 0.0, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_fx_struct_is_32_bytes() {
        assert_eq!(size_of::<Fx>(), 32);
    }
}
