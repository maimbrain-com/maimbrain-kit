//! Sound, music and haptics from the sim's cues. The assets are generated
//! (`tools/regen.sh`: Lyria music, ElevenLabs effects; each has a `.gen.json`
//! sidecar with its prompt); `tools/gen_audio.py` is the older code-made set.
//!
//! The mix (DESIGN.md "Look and sound"): the music sits at a fixed level and
//! every effect is set relative to it in `FX`. The sounds heard every second
//! or so (coins, lane swishes, pass-bys) are the quietest, vary a little in
//! level and pitch each time, and have a cap on how many can ring at once.
//! The engine and wind loops stay under the music even at top speed.
//!
//! Output only: nothing here is read back by the game, and the random
//! variation comes from this module's own fixed-seed generator, never the
//! game's RNG, so playing sound can't change a replay.
//!
//! Tests can record everything it plays as a mix log (`Audio::recording`),
//! which the skill's `scripts/mix_check.py` renders with the real assets and
//! measures (the `mix_log` test in `src/tests.rs`).

use std::fmt::Write as _;

use maimbrain::Rng;
use maimbrain::audio::{Sound, Voice};
use maimbrain::sensors::{self, Haptic};
use maimbrain::sys::{Asset, AssetState};

use crate::sim::{self, Cue, Sim};

const FILES: [&str; 20] = [
    "music_base", "music_hi", "engine", "start", "lane", "squeal", "coin", "near", "boost", "smash", "speedup", "alert", "oil", "crash", "over", "best", "ui", "wind", "pass",
    "blinker",
];
const MUSIC_BASE: usize = 0;
const MUSIC_HI: usize = 1;
const ENGINE: usize = 2;
const START: usize = 3;
const LANE: usize = 4;
const SQUEAL: usize = 5;
const COIN: usize = 6;
const NEAR: usize = 7;
const BOOST: usize = 8;
const SMASH: usize = 9;
const SPEEDUP: usize = 10;
const ALERT: usize = 11;
const OIL: usize = 12;
const CRASH: usize = 13;
const OVER: usize = 14;
const BEST: usize = 15;
const UI: usize = 16;
const WIND: usize = 17;
const PASS: usize = 18;
const BLINKER: usize = 19;

/// How a one-shot plays: `vol` (1 = the file's own level, which is about
/// −14 dB RMS for the effects), a random spread per play (± dB, ± pitch
/// fraction), `cap`, how many of it may ring at once (past it the oldest is
/// stopped), and `secs`, the file's length (re-measure after regenerating;
/// it says when a voice has finished and stops counting toward the cap).
#[derive(Clone, Copy)]
struct Fx {
    vol: f32,
    db: f32,
    pitch: f32,
    cap: usize,
    secs: f32,
}

const fn fx(vol: f32, db: f32, pitch: f32, cap: usize, secs: f32) -> Fx {
    Fx { vol, db, pitch, cap, secs }
}

/// Effect levels, indexed like `FILES` (the loops' rows are unused). Set
/// against the music (`MUSIC_VOL`), measured with `mix_check.py` on a bot
/// round: coins (~70 a minute, in runs) sit ~7 dB under the music over 3 s
/// and each peaks over half as loud as it on a phone speaker, lane
/// swishes and pass-bys (~25 a minute each) ~8–10 dB under; the rare events
/// (near miss, boost, speed-up, crash) may poke above it.
const FX: [Fx; 20] = [
    fx(0.0, 0.0, 0.0, 1, 16.0),   // music_base (MUSIC_VOL)
    fx(0.0, 0.0, 0.0, 1, 16.0),   // music_hi
    fx(0.0, 0.0, 0.0, 1, 4.45),   // engine (ENGINE_VOL)
    fx(0.6, 0.0, 0.0, 1, 2.0),    // start
    fx(0.5, 1.5, 0.06, 2, 0.5),   // lane
    fx(0.32, 1.0, 0.01, 1, 0.6),  // squeal: tonal (a G), played on E (SQUEAL_PITCH)
    fx(0.19, 1.5, 0.0, 3, 0.5),   // coin: tonal, so no pitch spread (it's on the scale)
    fx(0.55, 1.0, 0.0, 2, 0.9),   // near
    fx(0.75, 0.0, 0.0, 1, 1.6),   // boost
    fx(0.7, 1.0, 0.08, 2, 0.8),   // smash
    fx(0.5, 0.0, 0.0, 1, 0.6),    // speedup
    fx(0.5, 0.0, 0.0, 1, 0.6),    // alert
    fx(0.6, 0.0, 0.0, 1, 0.9),    // oil
    fx(0.72, 0.0, 0.0, 1, 2.6),   // crash
    fx(0.7, 0.0, 0.0, 1, 2.0),    // over
    fx(0.8, 0.0, 0.0, 1, 2.0),    // best
    fx(0.45, 0.0, 0.0, 2, 0.5),   // ui
    fx(0.0, 0.0, 0.0, 1, 5.95),   // wind (WIND_VOL)
    fx(0.36, 1.5, 0.05, 2, 1.0),  // pass
    fx(0.35, 1.0, 0.02, 2, 0.5),  // blinker
];
/// Music level: the reference everything else is mixed against (the stems
/// are about −21.6 dB RMS each, the effects about −14).
const MUSIC_VOL: f32 = 1.0;
/// The engine loop at the start speed; it climbs with speed and boost.
const ENGINE_VOL: f32 = 0.22;
/// The wind roar at full speed feel (near silent at the start), and the most
/// a boost can push it past that.
const WIND_VOL: f32 = 0.25;
const WIND_MAX: f32 = 1.15;
/// How far a near miss ducks the music (and how fast it comes back, s).
const NEAR_DUCK: f32 = 0.7;
const DUCK_BACK: f32 = 0.35;
/// The music's bright layer never goes below this in play (the low layer
/// alone is muffled); speed-ups, boosts and near-miss chains open it fully.
const HI_FLOOR: f32 = 0.35;
/// A sound that hasn't finished decoding plays silently, and a silent loop
/// stays silent: loops wait this long after their sound exists.
const DECODE_GRACE: f32 = 0.6;
/// The music's key note relative to coin.ogg's own pitch (the coin is a G5
/// pluck, the generated track sits on F#/B major): re-measure both after
/// regenerating either.
const MUSIC_ROOT: f32 = -1.0;
/// Coins in a row climb this ladder (semitones from the key note: the 5th
/// below, root, 2nd, 4th, 5th), then start again: only notes of the key, no
/// 3rd (so it fits major or minor), and never more than half an octave from
/// the sample's own pitch.
const LADDER: [f32; 5] = [-5.0, 0.0, 2.0, 5.0, 7.0];
/// The near-miss ting is an A#; a chain climbs from it in the key's notes
/// (A#, B, C#, D#, F#).
const NEAR_STEPS: [f32; 5] = [0.0, 1.0, 3.0, 5.0, 8.0];
/// The tyre squeal whines on a G; three semitones down it's an E, in key.
const SQUEAL_PITCH: f32 = 0.8409;
/// A semitone down: the dashboard alert rings a C, the game-over sting sits
/// on D and the new-best jingle on G; down a semitone all three fall in the
/// music's key (B major).
const DOWN_SEMI: f32 = 0.9439;

fn semis(s: f32) -> f32 {
    (s / 12.0).exp2()
}

pub struct Audio {
    assets: Vec<Asset>,
    sounds: Vec<Option<Sound>>,
    age: Vec<f32>,
    rng: Rng,
    /// Delayed one-shots: (seconds left, sound, vol scale, pitch).
    queue: Vec<(f32, usize, f32, f32)>,
    /// One-shot voices per sound that may still be ringing (voice, when it
    /// ends on `clock`), oldest first, for the caps.
    live: Vec<Vec<(Voice, f32)>>,
    music: Option<(Voice, Voice)>,
    wind: Option<Voice>,
    music_wanted: bool,
    /// The tension stem's level, eased, and a duck that recovers.
    hi: f32,
    duck: f32,
    fade: f32,
    engine: Option<Voice>,
    engine_wanted: bool,
    /// Seconds since this was made (the mix log's clock).
    clock: f32,
    /// Tests: every play, loop, level change and stop, as `mix_check.py`
    /// log lines, and the last level logged per loop (to skip tiny changes).
    pub log: Option<String>,
    logged: Vec<(f32, f32)>,
}

impl Audio {
    pub fn new() -> Audio {
        Audio {
            assets: FILES.iter().map(|f| Asset::load(&format!("assets/{f}.ogg"))).collect(),
            sounds: vec![None; FILES.len()],
            age: vec![0.0; FILES.len()],
            rng: Rng::new(0x5e7e_77e5),
            queue: Vec::new(),
            live: vec![Vec::new(); FILES.len()],
            music: None,
            wind: None,
            music_wanted: false,
            hi: 0.0,
            duck: 1.0,
            fade: 1.0,
            engine: None,
            engine_wanted: false,
            clock: 0.0,
            log: None,
            logged: vec![(-1.0, -1.0); FILES.len()],
        }
    }

    /// Tests: every sound counts as decoded and everything played is logged.
    #[cfg(test)]
    pub fn recording() -> Audio {
        let mut a = Audio::new();
        a.sounds = (0..FILES.len()).map(|i| Some(Sound(i as u32 + 1))).collect();
        a.age = vec![DECODE_GRACE; FILES.len()];
        a.log = Some(String::new());
        a
    }

    fn note(&mut self, i: usize, vol: f32, pitch: f32, op: &str) {
        if let Some(log) = &mut self.log {
            let _ = writeln!(log, "{:.3} {} {:.4} {:.4} {op}", self.clock, FILES[i], vol, pitch);
        }
    }

    /// A one-shot at its `FX` level times `scale`, with its random spread.
    fn play(&mut self, i: usize, scale: f32, pitch: f32) {
        self.play_pan(i, scale, 0.0, pitch);
    }

    fn play_pan(&mut self, i: usize, scale: f32, pan: f32, pitch: f32) {
        let Some(s) = self.sounds[i] else { return };
        let f = FX[i];
        let vol = f.vol * scale * if f.db > 0.0 { (self.rng.range(-f.db, f.db) / 20.0 * std::f32::consts::LOG2_10).exp2() } else { 1.0 };
        let pitch = pitch * if f.pitch > 0.0 { self.jitter(f.pitch) } else { 1.0 };
        // Past the cap, the oldest one still ringing stops.
        let now = self.clock;
        self.live[i].retain(|v| v.1 > now);
        if self.live[i].len() >= f.cap {
            let (old, _) = self.live[i].remove(0);
            old.stop();
            self.note(i, 0.0, 1.0, "stop");
        }
        self.live[i].push((s.play(vol, pan, pitch), now + f.secs / pitch.max(0.1)));
        self.note(i, vol, pitch, "play");
    }

    fn looped(&mut self, i: usize, vol: f32, at: Option<f64>) -> Voice {
        let s = self.sounds[i].unwrap();
        self.note(i, vol, 1.0, "loop");
        self.logged[i] = (vol, 1.0);
        match at {
            Some(at) => s.play_looped_at(vol, 0.0, 1.0, at),
            None => s.play_looped(vol, 0.0, 1.0),
        }
    }

    fn set(&mut self, i: usize, v: Voice, vol: f32, pitch: f32) {
        v.set(vol, 0.0, pitch);
        let (lv, lp) = self.logged[i];
        if (vol - lv).abs() > 0.01 * lv.max(0.02) || (pitch - lp).abs() > 0.004 {
            self.logged[i] = (vol, pitch);
            self.note(i, vol, pitch, "set");
        }
    }

    fn stop(&mut self, i: usize, v: Voice) {
        v.stop();
        self.note(i, 0.0, 1.0, "stop");
    }

    fn ready(&self, i: usize) -> bool {
        self.sounds[i].is_some() && self.age[i] >= DECODE_GRACE
    }

    fn jitter(&mut self, amount: f32) -> f32 {
        1.0 + self.rng.range(-amount, amount)
    }

    /// A round starts: the start sting, the engine, the music from the top.
    pub fn start(&mut self) {
        self.queue.clear();
        self.stop_loops();
        self.play(START, 1.0, 1.0);
        self.music_wanted = true;
        self.engine_wanted = true;
        // The bright layer is in from the first beat (the low layer alone is
        // muffled under the opening carpet of coins).
        self.hi = HI_FLOOR;
        self.fade = 1.0;
    }

    fn stop_music(&mut self) {
        if let Some((a, b)) = self.music.take() {
            self.stop(MUSIC_BASE, a);
            self.stop(MUSIC_HI, b);
        }
        self.music_wanted = false;
    }

    fn stop_engine(&mut self) {
        if let Some(e) = self.engine.take() {
            self.stop(ENGINE, e);
        }
        if let Some(w) = self.wind.take() {
            self.stop(WIND, w);
        }
        self.engine_wanted = false;
    }

    fn stop_loops(&mut self) {
        self.stop_music();
        self.stop_engine();
    }

    pub fn ui(&mut self) {
        self.play(UI, 1.0, 1.0);
    }

    /// The results card landed on a new best.
    pub fn new_best(&mut self) {
        self.play(BEST, 1.0, DOWN_SEMI);
        sensors::haptic(Haptic::Success);
    }

    /// Sounds and haptics for one cue. Off the card (the attract mode) it's silent.
    pub fn cue(&mut self, c: &Cue, playing: bool) {
        if !playing {
            return;
        }
        match *c {
            Cue::Lane { quick, .. } => {
                self.play(LANE, 1.0, 1.0);
                if quick {
                    self.play(SQUEAL, 1.0, SQUEAL_PITCH);
                }
                sensors::haptic(Haptic::Tap);
            }
            Cue::Bump { .. } => {
                self.play(LANE, 0.64, 0.7);
            }
            Cue::Coin { n, .. } => {
                // Up the ladder, then from the bottom again.
                let k = (n.max(1) - 1) as usize % LADDER.len();
                self.play(COIN, 1.0, semis(MUSIC_ROOT + LADDER[k]));
            }
            Cue::Pass { side, closing } => {
                // A Doppler whoosh from the side it passes on, sharper the faster you pass.
                let near = (1.0 - side.abs() / 6.0).max(0.2);
                self.play_pan(PASS, near, (side / 3.2).clamp(-0.85, 0.85), 0.82 + 0.013 * closing);
            }
            Cue::NearMiss { chain, .. } => {
                let p = semis(NEAR_STEPS[(chain.clamp(1, 5) - 1) as usize]);
                self.play(NEAR, 1.0, p);
                self.duck = self.duck.min(NEAR_DUCK);
                sensors::haptic(Haptic::Tap);
            }
            Cue::Boost { .. } => {
                self.play(BOOST, 1.0, 1.0);
                sensors::haptic(Haptic::Success);
            }
            Cue::Smash { .. } => {
                self.play(SMASH, 1.0, 1.0);
                sensors::haptic(Haptic::Heavy);
            }
            Cue::Oil { .. } => {
                self.play(OIL, 1.0, 1.0);
                self.play(SQUEAL, 1.0, SQUEAL_PITCH);
                sensors::haptic(Haptic::Heavy);
            }
            Cue::Blinker => {
                // Tick-tock while it signals (the blinker lead is 0.85 s).
                for k in 0..4 {
                    self.queue.push((k as f32 * 0.24, BLINKER, 1.0, if k % 2 == 0 { 1.0 } else { 0.86 }));
                }
            }
            Cue::SpeedUp { tier } => {
                self.play(SPEEDUP, 1.0, semis(tier.min(6) as f32 * 0.5));
            }
            Cue::New(_) => {
                self.play(ALERT, 1.0, DOWN_SEMI);
                sensors::haptic(Haptic::Tap);
            }
            Cue::Crash { .. } => {
                // Music out under the crash, a beat of quiet, then the verdict.
                self.play(CRASH, 1.0, 1.0);
                sensors::haptic(Haptic::Fail);
                self.fade = 0.0;
                self.stop_engine();
                self.queue.retain(|q| q.1 != BLINKER);
                self.queue.push((1.3, OVER, 1.0, DOWN_SEMI));
            }
            Cue::Over => {}
        }
    }

    pub fn update(&mut self, sim: &Sim, playing: bool, dt: f32) {
        self.clock += dt;
        for i in 0..FILES.len() {
            if self.sounds[i].is_none() {
                if self.assets[i].state() == AssetState::Ready {
                    self.sounds[i] = Sound::new(self.assets[i]);
                }
            } else {
                self.age[i] += dt;
            }
        }
        let mut due = Vec::new();
        self.queue.retain_mut(|q| {
            q.0 -= dt;
            if q.0 <= 0.0 {
                due.push((q.1, q.2, q.3));
            }
            q.0 > 0.0
        });
        for (i, vol, pitch) in due {
            self.play(i, vol, pitch);
        }
        if !playing && (self.music.is_some() || self.engine.is_some()) && self.fade <= 0.0 {
            self.stop_loops();
        }
        // Start the loops once they're decoded.
        if self.music_wanted && self.music.is_none() && self.ready(MUSIC_BASE) && self.ready(MUSIC_HI) {
            let at = maimbrain::sys::time() + 0.1;
            let a = self.looped(MUSIC_BASE, MUSIC_VOL, Some(at));
            let b = self.looped(MUSIC_HI, 0.0, Some(at));
            self.music = Some((a, b));
        }
        if self.engine_wanted && self.engine.is_none() && self.ready(ENGINE) {
            self.engine = Some(self.looped(ENGINE, 0.0, None));
        }
        if self.engine_wanted && self.wind.is_none() && self.ready(WIND) {
            self.wind = Some(self.looped(WIND, 0.0, None));
        }
        // The tension layer: speed tiers, boosts and near-miss chains bring it in.
        let want = if playing && !sim.crashed() {
            let open = ((sim.tier as f32 - 1.0) / 4.0).clamp(0.0, 1.0).max(sim.boost_k).max((sim.near_chain as f32 / 3.0).min(1.0));
            HI_FLOOR + (1.0 - HI_FLOOR) * open
        } else {
            0.0
        };
        self.hi += (want - self.hi) * (1.0 - (-dt / 0.8).exp());
        self.duck += (1.0 - self.duck) * (1.0 - (-dt / DUCK_BACK).exp());
        if self.fade <= 0.0 {
            self.stop_music();
        }
        if let Some((a, b)) = self.music {
            self.set(MUSIC_BASE, a, MUSIC_VOL * self.duck, 1.0);
            self.set(MUSIC_HI, b, MUSIC_VOL * self.duck * self.hi, 1.0);
        }
        let frac = ((sim.speed - sim::START_SPEED) / (sim::MAX_SPEED - sim::START_SPEED)).clamp(0.0, 1.0);
        if let Some(e) = self.engine {
            // Pitch climbs with every speed-up and never stops climbing (no
            // gear changes to fall back from); boost and kicks rev it on top.
            let pitch = 0.78 + 0.6 * frac + 0.28 * sim.boost_k + 0.06 * sim.feel.kick.min(1.0);
            self.set(ENGINE, e, ENGINE_VOL * (0.75 + 0.3 * frac + 0.4 * sim.boost_k), pitch);
        }
        if let Some(w) = self.wind {
            // The wind roar comes up with the speed feel: near silent at the start, loud on boost.
            let k = sim.feel.k.clamp(0.0, 1.8);
            let vol = WIND_VOL * ((k - 0.25).max(0.0) / 1.2).min(WIND_MAX);
            self.set(WIND, w, vol, 0.85 + 0.25 * k);
        }
    }
}
