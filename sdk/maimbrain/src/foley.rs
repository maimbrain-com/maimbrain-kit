//! Foley: sound tied to motion, so everything that moves makes sound without
//! a line of mixing per event. Footsteps fire from a stride's phase
//! ([`Gait`]) or a foot touching the ground ([`Contact`]); a servo whine or
//! a thruster loop follows a speed or a thrust ([`Drive`]); a landing or a
//! hit is as loud as it was hard ([`Foley::impact`]). Every sound is placed
//! in 3D: quieter with distance and panned from where it is relative to the
//! camera ([`Listener`], [`Space`]), and one-shots share a small voice budget
//! ([`Foley::max_voices`]) where the loudest heard win, so a brawl of four
//! mechs doesn't pile 40 stomps onto the mixer.
//!
//! It advances only by `dt` and draws its variation (±1.5 dB, ±4 % pitch per
//! play) from its own seeded generator, so a replay hears the same sounds,
//! and its smoothing is frame-rate independent. Audio is output only: none
//! of this touches the game's state.
//!
//! ```ignore
//! use maimbrain::foley::{Drive, Foley, Gait, Listener};
//! let mut foley = Foley::new(sys::rand_seed());
//! let mut gait = Gait::biped();
//! let mut servo = Drive::servo(sounds.servo);
//! let mut jets = Drive::thruster(sounds.thruster);
//! // Each frame, after moving the mech:
//! foley.update(dt, Listener::from_camera(&cam));
//! for _foot in gait.update(mech.stride_phase) {
//!     foley.step(sounds.stomp, mech.pos, mech.speed / mech.top_speed);
//! }
//! servo.update(&mut foley, mech.turn_rate.abs() + mech.arm_speed, mech.pos, dt);
//! jets.update(&mut foley, mech.thrust, mech.pos, dt);
//! if mech.just_landed { foley.impact(sounds.stomp, mech.pos, mech.fall_speed, 12.0); }
//! if hit { foley.impact(sounds.clang, at, hit_speed, 20.0); foley.hit(sounds.hiss, at, 0.5, 1.0, 0.6); }
//! ```

use crate::audio::{Sound, Voice};
use crate::gfx3d::{Camera, Vec3};

/// Where foley's sound goes: [`Live`] plays it; [`Log`] writes it down
/// (tests, and `mix_check.py` renders).
pub trait Mixer {
    /// Starts `sound` now, looped or not.
    fn play(&mut self, sound: Sound, vol: f32, pan: f32, pitch: f32, looped: bool, now: f64) -> Voice;
    fn set(&mut self, voice: Voice, vol: f32, pan: f32, pitch: f32, now: f64);
    /// Stops `voice` with a short release (no click).
    fn stop(&mut self, voice: Voice, now: f64);
}

/// Plays through the platform.
#[derive(Default)]
pub struct Live;

impl Mixer for Live {
    fn play(&mut self, sound: Sound, vol: f32, pan: f32, pitch: f32, looped: bool, _now: f64) -> Voice {
        if looped { sound.play_looped(vol, pan, pitch) } else { sound.play(vol, pan, pitch) }
    }
    fn set(&mut self, voice: Voice, vol: f32, pan: f32, pitch: f32, _now: f64) {
        voice.set(vol, pan, pitch);
    }
    fn stop(&mut self, voice: Voice, _now: f64) {
        voice.stop_at(0.0, RELEASE);
    }
}

/// How long a stolen or stopped voice takes to die away, seconds.
pub const RELEASE: f32 = 0.04;

/// Meters within which two plays of one sound count as one source ([`Foley::min_gap`]).
pub const SAME_PLACE: f32 = 1.5;

/// Writes what would be played as `mix_check.py` event lines (`<seconds>
/// <sound> <vol> <pitch> [loop|set|stop] [fade=<s>] id=<voice> pan=<p>`),
/// naming each sound with [`Log::name`]. mix_check mixes in mono, like the
/// platform's speaker; `pan` is there for a reader.
#[derive(Default)]
pub struct Log {
    pub lines: Vec<String>,
    names: Vec<(u32, String)>,
    voices: Vec<(u32, u32)>,
    next: u32,
}

impl Log {
    pub fn name(&mut self, sound: Sound, name: &str) {
        self.names.push((sound.0, name.to_string()));
    }

    fn sound_name(&self, s: u32) -> String {
        self.names.iter().find(|n| n.0 == s).map_or_else(|| format!("sound{s}"), |n| n.1.clone())
    }

    pub fn text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
}

impl Mixer for Log {
    fn play(&mut self, sound: Sound, vol: f32, pan: f32, pitch: f32, looped: bool, now: f64) -> Voice {
        self.next += 1;
        self.voices.push((self.next, sound.0));
        let kind = if looped { " loop" } else { "" };
        self.lines.push(format!("{now:.3} {} {vol:.3} {pitch:.4}{kind} id={} pan={pan:.2}", self.sound_name(sound.0), self.next));
        Voice(self.next)
    }
    fn set(&mut self, voice: Voice, vol: f32, pan: f32, pitch: f32, now: f64) {
        let Some(&(_, s)) = self.voices.iter().find(|v| v.0 == voice.0) else { return };
        self.lines.push(format!("{now:.3} {} {vol:.3} {pitch:.4} set id={} pan={pan:.2}", self.sound_name(s), voice.0));
    }
    fn stop(&mut self, voice: Voice, now: f64) {
        let Some(i) = self.voices.iter().position(|v| v.0 == voice.0) else { return };
        let (_, s) = self.voices.remove(i);
        self.lines.push(format!("{now:.3} {} 0 1 stop fade={RELEASE:.3} id={}", self.sound_name(s), voice.0));
    }
}

/// Where the player hears from: the camera's position and its right-hand side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: Vec3,
    /// Unit vector to the listener's right (sounds there pan right).
    pub right: Vec3,
}

impl Listener {
    pub fn from_camera(cam: &Camera) -> Listener {
        Listener { pos: cam.pos, right: cam.rot.right() }
    }
}

impl Default for Listener {
    fn default() -> Listener {
        Listener { pos: Vec3::ZERO, right: Vec3::X }
    }
}

/// How distance and direction change a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    /// Full level within this many meters.
    pub near: f32,
    /// Silent from this far; between `near` and here the level falls as
    /// `near / distance` (−6 dB per doubling), fading to nothing over the last third.
    pub far: f32,
    /// The widest pan, 0–1: a sound straight to the side pans this far.
    /// A phone held in portrait is a narrow stereo image; 0.6 keeps both sides heard.
    pub pan_width: f32,
}

impl Default for Space {
    fn default() -> Space {
        Space { near: 4.0, far: 60.0, pan_width: 0.6 }
    }
}

impl Space {
    /// The gain (0–1) and pan (−1…1) of a sound at `at` heard from `l`.
    pub fn place(&self, l: &Listener, at: Vec3) -> (f32, f32) {
        let d = at - l.pos;
        let dist = d.length();
        let mut gain = if dist <= self.near { 1.0 } else { self.near / dist };
        gain *= 1.0 - smoothstep(self.far * 0.67, self.far, dist);
        let pan = if dist > 1e-4 { (d.dot(l.right) / dist).clamp(-1.0, 1.0) * self.pan_width } else { 0.0 };
        (gain, pan)
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A one-shot that's sounding: how loud it's heard, until when.
#[derive(Clone, Copy, Debug)]
struct Shot {
    voice: Voice,
    sound: Sound,
    heard: f32,
    at: f64,
    pos: Vec3,
    until: f64,
}

/// The foley player: places sounds, varies them and keeps the voice budget.
pub struct Foley<M: Mixer = Live> {
    pub mixer: M,
    pub space: Space,
    /// One-shots sounding at once, at most (default 6). Past it, a new sound
    /// louder (as heard) than the quietest playing replaces it; a quieter one is dropped.
    pub max_voices: usize,
    /// The same sound again within this many seconds is folded into the
    /// first (a louder one replaces it) when it comes from the same place
    /// (within [`SAME_PLACE`]), so a burst doesn't flam (default 0.05).
    pub min_gap: f32,
    /// Per play: level ± this many dB, pitch ± this fraction (default 1.5 dB, 0.04).
    pub vary_db: f32,
    pub vary_pitch: f32,
    /// A sound heard quieter than this (vol × distance gain) isn't played (default 0.02).
    pub floor: f32,
    listener: Listener,
    shots: Vec<Shot>,
    t: f64,
    rng: u32,
}

impl Foley<Live> {
    /// `seed`: `sys::rand_seed()`, so a replay varies the same way.
    pub fn new(seed: u64) -> Foley<Live> {
        Foley::with_mixer(seed, Live)
    }
}

impl<M: Mixer> Foley<M> {
    pub fn with_mixer(seed: u64, mixer: M) -> Foley<M> {
        let rng = ((seed ^ (seed >> 32)) as u32) | 1;
        Foley { mixer, space: Space::default(), max_voices: 6, min_gap: 0.05, vary_db: 1.5, vary_pitch: 0.04, floor: 0.02, listener: Listener::default(), shots: Vec::new(), t: 0.0, rng }
    }

    /// Call once a frame, before the frame's sounds: advances foley's clock
    /// by `dt` and moves the listener (usually [`Listener::from_camera`]).
    pub fn update(&mut self, dt: f32, listener: Listener) {
        self.t += dt.max(0.0) as f64;
        self.listener = listener;
        let t = self.t;
        self.shots.retain(|s| s.until > t);
    }

    pub fn listener(&self) -> Listener {
        self.listener
    }

    /// Seconds of foley's clock (the sum of `dt`).
    pub fn time(&self) -> f64 {
        self.t
    }

    /// One-shots sounding now.
    pub fn voices(&self) -> usize {
        self.shots.len()
    }

    /// The gain and pan of a sound at `at`, from the listener.
    pub fn place(&self, at: Vec3) -> (f32, f32) {
        self.space.place(&self.listener, at)
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Plays `sound` once at `at`, `vol` before distance (0–4), `pitch` before
    /// variation, lasting about `seconds` (how long it holds a voice). None
    /// when it's too far to hear, folded into the same sound just played, or
    /// quieter than everything holding the voice budget.
    pub fn hit(&mut self, sound: Sound, at: Vec3, vol: f32, pitch: f32, seconds: f32) -> Option<Voice> {
        let (gain, pan) = self.place(at);
        let r1 = self.rand();
        let r2 = self.rand();
        let heard = vol * gain;
        if heard < self.floor {
            return None;
        }
        let t = self.t;
        // A burst of one sound: keep the loudest.
        if let Some(i) = self.shots.iter().position(|s| s.sound == sound && t - s.at < self.min_gap as f64 && s.pos.distance(at) < SAME_PLACE) {
            if self.shots[i].heard >= heard {
                return None;
            }
            let old = self.shots.remove(i);
            self.mixer.stop(old.voice, t);
        }
        if self.shots.len() >= self.max_voices.max(1) {
            let (i, q) = self.shots.iter().enumerate().min_by(|a, b| a.1.heard.total_cmp(&b.1.heard))?;
            if q.heard >= heard {
                return None;
            }
            let old = self.shots.remove(i);
            self.mixer.stop(old.voice, t);
        }
        let v = heard * 10f32.powf((r1 * 2.0 - 1.0) * self.vary_db / 20.0);
        let p = pitch * (1.0 + (r2 * 2.0 - 1.0) * self.vary_pitch);
        let voice = self.mixer.play(sound, v.min(4.0), pan, p, false, t);
        self.shots.push(Shot { voice, sound, heard, at: t, pos: at, until: t + seconds.max(0.05) as f64 });
        Some(voice)
    }

    /// A footstep or stomp at `at`: `weight` 0–1 is how hard (a walk 0.3, a
    /// run 1); louder and a little lower the harder it lands.
    pub fn step(&mut self, sound: Sound, at: Vec3, weight: f32) -> Option<Voice> {
        let w = weight.clamp(0.0, 1.0);
        self.hit(sound, at, 0.3 + 0.4 * w, 1.06 - 0.12 * w, 0.6)
    }

    /// A landing, a crash or a hit at `speed` (m/s into the surface):
    /// silent under 5% of `full_speed`, then louder and lower up to
    /// `full_speed` (vol 0.2 → 0.8, pitch 1.15 → 0.85), so a hop taps and a
    /// fall from the sky booms.
    pub fn impact(&mut self, sound: Sound, at: Vec3, speed: f32, full_speed: f32) -> Option<Voice> {
        let k = (speed.abs() / full_speed.max(1e-3)).min(1.0);
        if k < 0.05 {
            return None;
        }
        let k = k.sqrt();
        self.hit(sound, at, 0.2 + 0.6 * k, 1.15 - 0.3 * k, 1.0)
    }
}

/// Footsteps from a stride's phase: each foot touches down at its offset in
/// the cycle (a biped's feet half a cycle apart). Feed it the phase the
/// legs are animated with (cycles, wrapping at 1, or counting up), and it
/// returns the feet that touched down since the last call, however long the
/// frame was (up to half a cycle).
#[derive(Clone, Debug)]
pub struct Gait {
    pub feet: Vec<f32>,
    last: Option<f32>,
}

impl Gait {
    /// Two feet, at 0 and ½ of the cycle.
    pub fn biped() -> Gait {
        Gait::new(&[0.0, 0.5])
    }

    /// Four feet: a walk (left hind, left fore, right hind, right fore).
    pub fn quadruped() -> Gait {
        Gait::new(&[0.0, 0.25, 0.5, 0.75])
    }

    pub fn new(offsets: &[f32]) -> Gait {
        Gait { feet: offsets.iter().map(|o| o.rem_euclid(1.0)).collect(), last: None }
    }

    /// The feet (indices into `feet`) that touched down as the phase moved
    /// from the last call's to `phase`. A phase standing still or going
    /// backwards (more than half a cycle at once) steps no foot.
    pub fn update(&mut self, phase: f32) -> Vec<usize> {
        let p = phase.rem_euclid(1.0);
        let Some(last) = self.last.replace(p) else { return Vec::new() };
        let d = (p - last).rem_euclid(1.0);
        if d <= 0.0 || d > 0.5 {
            return Vec::new();
        }
        self.feet
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                let k = (**o - last).rem_euclid(1.0);
                k > 0.0 && k <= d
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Forget the phase (the walk stopped): the next call steps nothing.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// A foot (or a body) touching the ground: from its height above the
/// ground, with a little hysteresis so a foot resting on the ground doesn't
/// chatter. [`Contact::update`] gives the speed it came down at when it lands.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    /// It lands at or under this height, and lifts off above `lift`.
    pub touch: f32,
    pub lift: f32,
    down: bool,
    last: Option<f32>,
    speed: f32,
    /// The frame before's: the landing frame is cut short by the ground.
    prev: f32,
}

impl Contact {
    /// Lands at 2 cm, lifts at 6 cm.
    pub fn new() -> Contact {
        Contact::with(0.02, 0.06)
    }

    pub fn with(touch: f32, lift: f32) -> Contact {
        Contact { touch, lift: lift.max(touch), down: true, last: None, speed: 0.0, prev: 0.0 }
    }

    /// `height` above the ground this frame. Some(speed coming down, m/s) on
    /// the frame it lands.
    pub fn update(&mut self, height: f32, dt: f32) -> Option<f32> {
        if let Some(h0) = self.last
            && dt > 0.0
        {
            self.prev = self.speed;
            self.speed = (h0 - height) / dt;
        }
        self.last = Some(height);
        if self.down {
            if height > self.lift {
                self.down = false;
            }
            None
        } else if height <= self.touch {
            self.down = true;
            Some(self.speed.max(self.prev).max(0.0))
        } else {
            None
        }
    }

    pub fn grounded(&self) -> bool {
        self.down
    }
}

impl Default for Contact {
    fn default() -> Contact {
        Contact::new()
    }
}

/// A loop that follows how hard something is driven: a servo or hydraulic
/// whine from a joint's or a body's speed, a thruster from its thrust, an
/// engine from its revs. Its level and pitch rise together over
/// `idle`…`full` of the drive, with an attack and a release (frame-rate
/// independent), and it starts and stops itself, so a still mech is silent.
#[derive(Clone, Copy, Debug)]
pub struct Drive {
    pub sound: Sound,
    /// Silent at or under `idle`; at its loudest and highest from `full`.
    pub idle: f32,
    pub full: f32,
    /// Vol and pitch at the bottom and the top.
    pub vol: (f32, f32),
    pub pitch: (f32, f32),
    /// Seconds to rise and to fall by ~63%.
    pub attack: f32,
    pub release: f32,
    level: f32,
    voice: Option<Voice>,
}

impl Drive {
    /// A servo whine: from speed (a joint's in rad/s, a body's turn rate, or
    /// both summed), silent under 0.15, full at 3, pitch 0.8 → 1.35, quick.
    pub fn servo(sound: Sound) -> Drive {
        Drive { sound, idle: 0.15, full: 3.0, vol: (0.1, 0.45), pitch: (0.8, 1.35), attack: 0.06, release: 0.15, level: 0.0, voice: None }
    }

    /// A thruster or jet: from thrust 0–1, pitch 0.85 → 1.2, a slower spool.
    pub fn thruster(sound: Sound) -> Drive {
        Drive { sound, idle: 0.02, full: 1.0, vol: (0.2, 0.8), pitch: (0.85, 1.2), attack: 0.12, release: 0.35, level: 0.0, voice: None }
    }

    /// 0–1: how far up it is now.
    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn voice(&self) -> Option<Voice> {
        self.voice
    }

    /// Call every frame with what drives it and where it is.
    pub fn update<M: Mixer>(&mut self, foley: &mut Foley<M>, drive: f32, at: Vec3, dt: f32) {
        let target = smoothstep(self.idle, self.full.max(self.idle + 1e-3), drive);
        let tau = if target > self.level { self.attack } else { self.release };
        self.level += (target - self.level) * (1.0 - (-dt.max(0.0) / tau.max(1e-3)).exp());
        let now = foley.time();
        if self.level < 0.005 && target == 0.0 {
            self.level = 0.0;
            if let Some(v) = self.voice.take() {
                foley.mixer.stop(v, now);
            }
            return;
        }
        let (gain, pan) = foley.place(at);
        let k = self.level;
        let vol = (self.vol.0 + (self.vol.1 - self.vol.0) * k) * k.sqrt() * gain;
        let pitch = self.pitch.0 + (self.pitch.1 - self.pitch.0) * k;
        match self.voice {
            Some(v) => foley.mixer.set(v, vol, pan, pitch, now),
            None => self.voice = Some(foley.mixer.play(self.sound, vol, pan, pitch, true, now)),
        }
    }

    /// Stops it now (the mech was destroyed, the scene ended).
    pub fn stop<M: Mixer>(&mut self, foley: &mut Foley<M>) {
        self.level = 0.0;
        if let Some(v) = self.voice.take() {
            foley.mixer.stop(v, foley.time());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::vec3;

    fn foley() -> Foley<Log> {
        let mut log = Log::default();
        log.name(Sound(1), "stomp");
        log.name(Sound(2), "servo");
        Foley::with_mixer(7, log)
    }

    #[test]
    fn distance_and_side_set_level_and_pan() {
        let s = Space::default();
        let l = Listener::default();
        let (g, p) = s.place(&l, vec3(2.0, 0.0, 0.0));
        assert_eq!(g, 1.0);
        assert!((p - 0.6).abs() < 1e-5);
        let (g8, _) = s.place(&l, vec3(0.0, 0.0, -8.0));
        assert!((g8 - 0.5).abs() < 1e-5, "{g8}");
        let (gl, pl) = s.place(&l, vec3(-16.0, 0.0, 0.0));
        assert!((gl - 0.25).abs() < 1e-5 && pl < -0.59);
        assert_eq!(s.place(&l, vec3(0.0, 0.0, 70.0)).0, 0.0);
    }

    #[test]
    fn gait_steps_each_foot_once_per_cycle_at_any_frame_rate() {
        for fps in [24.0f32, 30.0, 60.0, 120.0, 13.0] {
            let mut g = Gait::biped();
            let (mut steps, mut phase) = (vec![0usize; 2], 0.0f32);
            g.update(0.01);
            // 2.5 strides a second for 10 s.
            for _ in 0..(10.0 * fps) as usize {
                phase += 2.5 / fps;
                for f in g.update(phase) {
                    steps[f] += 1;
                }
            }
            assert!(steps.iter().all(|&n| (24..=25).contains(&n)), "{fps} fps: {steps:?}");
        }
        // Standing still or a big backward jump steps nothing.
        let mut g = Gait::biped();
        g.update(0.4);
        assert!(g.update(0.4).is_empty());
        assert!(g.update(0.2).is_empty());
    }

    #[test]
    fn contact_lands_once_with_its_speed() {
        let mut c = Contact::new();
        let dt = 1.0 / 60.0;
        let (mut h, mut v, mut lands) = (0.0f32, 6.0f32, Vec::new());
        for _ in 0..120 {
            v -= 9.8 * dt;
            h = (h + v * dt).max(0.0);
            if let Some(s) = c.update(h, dt) {
                lands.push(s);
            }
            if h == 0.0 {
                v = v.max(0.0);
            }
        }
        assert_eq!(lands.len(), 1, "{lands:?}");
        assert!(lands[0] > 4.0, "{lands:?}");
        // Resting on the ground doesn't chatter.
        assert!((0..30).all(|_| c.update(0.01, dt).is_none()));
    }

    #[test]
    fn the_voice_budget_keeps_the_loudest() {
        let mut f = foley();
        f.max_voices = 3;
        f.update(0.016, Listener::default());
        // Four mechs stomping at once, near to far, in one frame: the farthest is dropped.
        let mut played = 0;
        for d in [40.0, 2.0, 10.0, 20.0] {
            played += f.hit(Sound(1 + (d as u32) * 10), vec3(0.0, 0.0, -d), 0.8, 1.0, 0.5).is_some() as usize;
        }
        assert_eq!(f.voices(), 3);
        assert_eq!(played, 4, "the far one played first and was stolen");
        assert!(f.mixer.lines.iter().any(|l| l.contains("stop fade=")), "{:?}", f.mixer.lines);
        // A quieter one than all three is dropped.
        assert!(f.hit(Sound(9), vec3(0.0, 0.0, -50.0), 0.5, 1.0, 0.5).is_none());
        // Time frees the voices.
        f.update(0.6, Listener::default());
        assert_eq!(f.voices(), 0);
    }

    #[test]
    fn a_burst_of_one_sound_folds_into_the_loudest() {
        let mut f = foley();
        f.update(0.016, Listener::default());
        assert!(f.step(Sound(1), vec3(0.0, 0.0, -3.0), 0.5).is_some());
        assert!(f.step(Sound(1), vec3(0.0, 0.0, -3.0), 0.2).is_none());
        assert!(f.step(Sound(1), vec3(0.0, 0.0, -3.0), 1.0).is_some());
        assert_eq!(f.voices(), 1);
        f.update(0.06, Listener::default());
        assert!(f.step(Sound(1), vec3(0.0, 0.0, -3.0), 0.2).is_some());
    }

    #[test]
    fn impacts_scale_with_speed() {
        let mut f = foley();
        f.vary_db = 0.0;
        f.vary_pitch = 0.0;
        f.min_gap = 0.0;
        f.update(0.016, Listener::default());
        assert!(f.impact(Sound(1), Vec3::ZERO, 0.2, 12.0).is_none());
        f.impact(Sound(1), Vec3::ZERO, 2.0, 12.0).unwrap();
        f.impact(Sound(1), Vec3::ZERO, 12.0, 12.0).unwrap();
        let vols: Vec<(f32, f32)> = f.mixer.lines.iter().filter(|l| !l.contains("stop")).map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            (w[2].parse().unwrap(), w[3].parse().unwrap())
        }).collect();
        assert!(vols[0].0 < vols[1].0 && vols[0].1 > vols[1].1, "{vols:?}");
        assert!((vols[1].0 - 0.8).abs() < 1e-3 && (vols[1].1 - 0.85).abs() < 1e-3, "{vols:?}");
    }

    #[test]
    fn a_drive_follows_speed_the_same_at_any_frame_rate_and_stops_when_still() {
        let run = |fps: f32| {
            let mut f = foley();
            let mut s = Drive::servo(Sound(2));
            let dt = 1.0 / fps;
            // Turning hard for 0.1 s (partway up), then still for 2 s.
            for _ in 0..(0.1 * fps).round() as usize {
                f.update(dt, Listener::default());
                s.update(&mut f, 3.0, vec3(0.0, 0.0, -3.0), dt);
            }
            let up = s.level();
            for _ in 0..(2.0 * fps) as usize {
                f.update(dt, Listener::default());
                s.update(&mut f, 0.0, vec3(0.0, 0.0, -3.0), dt);
            }
            (up, s.voice().is_none(), f.mixer.lines.last().cloned().unwrap_or_default())
        };
        let (a30, stopped30, last30) = run(30.0);
        let (a120, stopped120, _) = run(120.0);
        let want = 1.0 - (-0.1f32 / 0.06).exp();
        assert!((a30 - want).abs() < 0.01 && (a120 - want).abs() < 0.01, "{a30} {a120} {want}");
        assert!(stopped30 && stopped120 && last30.contains("servo 0 1 stop"), "{last30}");
    }

    #[test]
    fn replays_hear_the_same_variation() {
        let lines = |seed| {
            let mut f = Foley::with_mixer(seed, Log::default());
            for i in 0..20 {
                f.update(0.1, Listener::default());
                f.step(Sound(1), vec3(i as f32 * 0.3, 0.0, -4.0), 0.6);
            }
            f.mixer.text()
        };
        assert_eq!(lines(3), lines(3));
        assert_ne!(lines(3), lines(4));
    }
}
