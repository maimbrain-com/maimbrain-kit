//! Sound effects, haptics and two-stem music, from the sim's cues.
//!
//! Output only: nothing here is read back by the game, and pitch jitter
//! comes from this module's own fixed-seed generator (never the game's), so
//! playing sound can't change a replay. The files are generated
//! (`tools/regen.sh`: `mb music --layers` for the two stems, `mb sfx` for
//! the effects); each has a `.gen.json` sidecar with its prompt.

use maimbrain::Rng;
use maimbrain::audio::{Sound as Snd, Voice};
use maimbrain::sensors::{self, Haptic};
use maimbrain::sys::{Asset, AssetState};

use crate::sim::Cue;

const FILES: [&str; 19] = [
    "jump", "jump2", "land", "coin", "arc", "close", "faster", "new", "milestone", "bonk", "fall", "bounce", "stuck", "over", "best", "start", "ui",
    "music_base", "music_hi",
];
const JUMP: usize = 0;
const JUMP2: usize = 1;
const LAND: usize = 2;
const COIN: usize = 3;
const ARC: usize = 4;
const CLOSE: usize = 5;
const FASTER: usize = 6;
const NEW: usize = 7;
const MILESTONE: usize = 8;
const BONK: usize = 9;
const FALL: usize = 10;
const BOUNCE: usize = 11;
const STUCK: usize = 12;
const OVER: usize = 13;
const BEST: usize = 14;
const START: usize = 15;
const UI: usize = 16;
const MUSIC_BASE: usize = 17;
const MUSIC_HI: usize = 18;

/// The mix. The music is the reference: both stems at `MUSIC_VOL` come to
/// about -23 LUFS, and every effect's level below is set against it
/// (measured with the skill's `mix_check.py` over bot-played rounds).
/// Frequent sounds carry the least energy: coins (2–3 a second in bursts)
/// sit ~9 dB under the music over 3 s, yet each one, short and bright,
/// peaks over half as loud as the music on a phone speaker, so it's heard
/// (the streak sparkle too); jumps ~10 dB under; rare moments (CLOSE!, a new family) get close to
/// it, and only the end of a run (the bonk, the slide whistle) goes over,
/// while the music fades out.
const MUSIC_VOL: f32 = 1.0;
/// The high stem (lead and arpeggios) never drops below this share: the
/// hook is always there, and speed and danger bring it up to full.
const HI_FLOOR: f32 = 0.6;
const VOL_COIN: f32 = 0.24;
const VOL_ARC: f32 = 0.25;
const VOL_JUMP: f32 = 0.22;
const VOL_JUMP2: f32 = 0.36;
const VOL_CLOSE: f32 = 0.47;
const VOL_FASTER: f32 = 0.22;
const VOL_NEW: f32 = 0.4;
const VOL_MILESTONE: f32 = 0.3;
const VOL_BONK: f32 = 0.85;
const VOL_FALL: f32 = 0.46;
const VOL_STUCK: f32 = 0.53;
const VOL_BOUNCE: f32 = 0.35;
/// A coin streak climbs these semitones, then stays at the top: coin.ogg is
/// B5 -> E6, so E, A, B with grace notes B, E, F#, notes the music (in E,
/// part major, part minor) always has. It starts again with the next
/// streak (`Cue::Coin::streak`, which the sim resets after 0.45 s without
/// a coin).
const LADDER: [f32; 3] = [0.0, 5.0, 7.0];
/// Coins on an arc come ~0.1 s apart (some closer): one this soon after the
/// last coin sound is folded into it, and at most two coin voices ring at
/// once (a third steals the oldest). The arc's last coin also plays the
/// soft "collect streak" sparkle (`arc.ogg`).
const COIN_GAP: f32 = 0.085;
/// Simultaneous voices per sound, the oldest stopped beyond it (the
/// platform's limit is 32 in all).
const CAP: [usize; 17] = [2, 1, 2, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 2];
/// A sound that hasn't finished decoding plays silently, and a silent loop
/// would stay silent: music waits this long after its sounds exist.
const DECODE_GRACE: f32 = 0.6;

fn semis(s: f32) -> f32 {
    (s / 12.0).exp2()
}

pub struct Sound {
    assets: Vec<Asset>,
    sounds: Vec<Option<Snd>>,
    age: Vec<f32>,
    rng: Rng,
    /// Delayed one-shots: (seconds left, sound, vol, pitch).
    queue: Vec<(f32, usize, f32, f32)>,
    music: Option<(Voice, Voice)>,
    want_music: bool,
    /// The tension stem's level (eased) and the overall fade.
    drive: f32,
    fade: f32,
    bounces: u32,
    /// The effects' voices still (maybe) ringing, per sound: (voice, started).
    voices: Vec<Vec<(Voice, f32)>>,
    /// When the last coin sound started.
    coin_at: f32,
    /// Seconds since this was made (for the test log).
    clock: f32,
    /// Tests: every play, stop and loop level, in the skill's
    /// `scripts/mix_check.py` format, to render and measure the mix offline.
    #[cfg(test)]
    pub log: Vec<String>,
    #[cfg(test)]
    logged_music: Option<(f32, f32)>,
}

impl Sound {
    pub fn new() -> Sound {
        Sound {
            assets: FILES.iter().map(|f| Asset::load(&format!("assets/{f}.ogg"))).collect(),
            sounds: vec![None; FILES.len()],
            age: vec![0.0; FILES.len()],
            rng: Rng::new(0x0b0b_5eed),
            queue: Vec::new(),
            music: None,
            want_music: false,
            drive: 0.0,
            fade: 1.0,
            bounces: 0,
            voices: vec![Vec::new(); FILES.len()],
            coin_at: -1.0,
            clock: 0.0,
            #[cfg(test)]
            log: Vec::new(),
            #[cfg(test)]
            logged_music: None,
        }
    }

    fn play(&mut self, i: usize, vol: f32, pitch: f32) {
        #[cfg(test)]
        self.log.push(format!("{:.4} {} {vol:.4} {pitch:.4}", self.clock, FILES[i]));
        let Some(s) = self.sounds[i] else { return };
        let v = s.play(vol, 0.0, pitch);
        if let Some(&cap) = CAP.get(i) {
            let now = self.clock;
            let vs = &mut self.voices[i];
            vs.retain(|&(_, at)| now - at < 2.0);
            if vs.len() >= cap {
                let (old, _) = vs.remove(0);
                old.stop();
                #[cfg(test)]
                self.log.push(format!("{now:.4} {} 0 1 stop", FILES[i]));
            }
            self.voices[i].push((v, now));
        }
    }

    fn jitter(&mut self, amount: f32) -> f32 {
        1.0 + self.rng.range(-amount, amount)
    }

    /// A volume varied by up to ±`db` decibels.
    fn vary(&mut self, vol: f32, db: f32) -> f32 {
        vol * (self.rng.range(-db, db) / 20.0 * std::f32::consts::LN_10).exp()
    }

    /// A round starts: a little "go" and the music from the top.
    pub fn start(&mut self) {
        self.stop_music();
        self.queue.clear();
        self.play(START, 0.5, 1.0);
        self.want_music = true;
        self.fade = 1.0;
        self.drive = 0.0;
        self.bounces = 0;
    }

    fn stop_music(&mut self) {
        #[cfg(test)]
        if self.logged_music.take().is_some() {
            self.log.push(format!("{:.4} music_base 0 1 stop\n{:.4} music_hi 0 1 stop", self.clock, self.clock));
        }
        if let Some((a, b)) = self.music.take() {
            a.stop();
            b.stop();
        }
        self.want_music = false;
    }

    /// The game-over card is up: a beat after the tumble, the verdict.
    pub fn over(&mut self, new_best: bool) {
        self.stop_music();
        self.queue.push((0.15, if new_best { BEST } else { OVER }, if new_best { 0.6 } else { 0.67 }, 1.0));
        if new_best {
            sensors::haptic(Haptic::Success);
        }
    }

    /// A card button.
    pub fn ui(&mut self, up: bool) {
        self.play(UI, 0.48, if up { 1.12 } else { 1.0 });
    }

    /// Plays one cue. `playing` is false on the title card's attract run,
    /// which stays silent.
    pub fn cue(&mut self, cue: &Cue, playing: bool) {
        if !playing {
            return;
        }
        match *cue {
            Cue::Jump => {
                let (v, p) = (self.vary(VOL_JUMP, 1.5), self.jitter(0.04));
                self.play(JUMP, v, p);
                sensors::haptic(Haptic::Tap);
            }
            Cue::DoubleJump => {
                let (v, p) = (self.vary(VOL_JUMP2, 1.0), self.jitter(0.03));
                self.play(JUMP2, v, p);
                sensors::haptic(Haptic::Tap);
            }
            Cue::Land { speed } => {
                let p = self.jitter(0.06);
                self.play(LAND, (speed / 1300.0).clamp(0.12, 0.45), p);
            }
            Cue::Coin { streak, .. } => {
                if self.clock - self.coin_at < COIN_GAP {
                    return;
                }
                self.coin_at = self.clock;
                let step = ((streak.max(1) - 1) as usize).min(LADDER.len() - 1);
                // Each step a touch quieter, so a long streak doesn't build up.
                let v = self.vary(VOL_COIN * (1.0 - 0.07 * step as f32), 1.5);
                let p = semis(LADDER[step]) * self.jitter(0.006);
                self.play(COIN, v, p);
            }
            Cue::ArcDone { .. } => {
                let v = self.vary(VOL_ARC, 1.0);
                self.queue.push((0.07, ARC, v, 1.0));
                sensors::haptic(Haptic::Tap);
            }
            Cue::Close { .. } => {
                self.play(CLOSE, VOL_CLOSE, 1.0);
                sensors::haptic(Haptic::Success);
            }
            Cue::SpeedUp => self.play(FASTER, VOL_FASTER, 1.0),
            Cue::NewKind(_) => self.play(NEW, VOL_NEW, 1.0),
            Cue::Milestone(_) => self.play(MILESTONE, VOL_MILESTONE, 1.0),
            Cue::Hit { .. } => {
                self.play(BONK, VOL_BONK, 1.0);
                sensors::haptic(Haptic::Heavy);
                self.fade_out();
            }
            Cue::Fall => {
                self.play(FALL, VOL_FALL, 1.0);
                sensors::haptic(Haptic::Fail);
                self.fade_out();
            }
            Cue::Bounce => {
                self.bounces += 1;
                let v = VOL_BOUNCE / self.bounces as f32;
                self.play(BOUNCE, v, semis(2.0 * self.bounces as f32));
                sensors::haptic(Haptic::Tap);
            }
            Cue::Stuck => {
                self.play(STUCK, VOL_STUCK, 1.0);
                sensors::haptic(Haptic::Heavy);
            }
            Cue::Over => {}
        }
    }

    fn fade_out(&mut self) {
        self.want_music = false;
        self.fade = self.fade.min(0.99);
    }

    /// Each frame: loads, delayed sounds, and the music's mix. `intensity`
    /// (0…1) raises the tension stem.
    pub fn update(&mut self, dt: f32, intensity: f32) {
        self.clock += dt;
        for i in 0..FILES.len() {
            if self.sounds[i].is_none() {
                if self.assets[i].state() == AssetState::Ready {
                    self.sounds[i] = Snd::new(self.assets[i]);
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
        // Music: starts once both stems have decoded; fades out after a mistake.
        let ready = |i: usize| self.sounds[i].is_some() && self.age[i] >= DECODE_GRACE;
        if self.want_music && self.music.is_none() && ready(MUSIC_BASE) && ready(MUSIC_HI) {
            let at = maimbrain::sys::time() + 0.05;
            if let (Some(b), Some(d)) = (self.sounds[MUSIC_BASE], self.sounds[MUSIC_HI]) {
                self.music = Some((b.play_looped_at(MUSIC_VOL, 0.0, 1.0, at), d.play_looped_at(0.0, 0.0, 1.0, at)));
            }
        }
        if self.fade < 1.0 {
            self.fade = (self.fade - dt / 0.5).max(0.0);
            if self.fade <= 0.0 {
                self.stop_music();
                self.fade = 1.0;
            }
        }
        // The high layer swells in over ~0.7 s and leaves over ~1.5 s.
        let tau = if intensity > self.drive { 0.7 } else { 1.5 };
        self.drive += (intensity - self.drive) * (1.0 - (-dt / tau).exp());
        let f = if self.fade < 1.0 { self.fade } else { 1.0 };
        let (gb, gh) = (MUSIC_VOL * f, MUSIC_VOL * f * (HI_FLOOR + (1.0 - HI_FLOOR) * self.drive.clamp(0.0, 1.0)));
        // (Natively the stems never decode: log them as if they had started.)
        #[cfg(test)]
        if self.want_music || self.fade < 1.0 {
            let t = self.clock;
            let verb = match self.logged_music {
                None => Some("loop"),
                Some((b, h)) if (b - gb).abs() > 0.005 || (h - gh).abs() > 0.005 => Some("set"),
                Some(_) => None,
            };
            if let Some(verb) = verb {
                self.log.push(format!("{t:.4} music_base {gb:.4} 1 {verb}\n{t:.4} music_hi {gh:.4} 1 {verb}"));
                self.logged_music = Some((gb, gh));
            }
        }
        if let Some((b, d)) = self.music {
            b.set(gb, 0.0, 1.0);
            d.set(gh, 0.0, 1.0);
        }
    }
}
