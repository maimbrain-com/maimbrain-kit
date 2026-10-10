//! The tension kit for `mb music --layers …,tension`: stems that add dread
//! to a generated loop, synthesized on that loop's own beat grid and in its
//! measured key, so they stay sample-aligned and in tune with it.
//!
//! Why synthesized: Lyria returns one finished stereo mix per request. It
//! has no stems, takes no audio to follow, and a second request with a
//! "tense" prompt comes back in another key and at another tempo (the seed
//! doesn't hold them across prompts). Lining such a take up with the loop
//! would mean time-stretching and pitch-shifting it, which smears it. What
//! the loop does give exactly is its length (a whole number of bars), its
//! beats (found in it) and its key (measured), and dread is mostly rhythm
//! and register: a drone, a pulse, a heartbeat, a tremolo that rubs, a
//! riser. Those synthesize well, cost nothing, and can be made to fit the
//! loop to the sample.
//!
//! Every oscillator runs a whole number of cycles per loop and every
//! envelope and filter is periodic in the loop, so the files loop with no
//! seam at all. The rubbing notes are both in the key's scale (the fifth and
//! minor sixth in minor, the seventh and octave in major): they clash with
//! each other a semitone apart, not with the music, so mix_check's key check
//! passes.
//!
//! Files: `_tension` (drone, eighth-note pulse, a heartbeat on each bar's
//! downbeat, string tremolo swelling over each phrase, a riser into each
//! phrase), `_tick` (a clock: tick-tock eighths with ghost sixteenths, for
//! tempo feel), and two one-shots in the same key, `_heartbeat` (lub-dub, to
//! schedule on the beat grid faster as the danger grows) and `_sting` (a
//! hit for a near-catch).

use std::f64::consts::{LN_2, TAU};

use super::audio::{self, Biquad, Key};

/// A loop's beat grid: `beats` beats over `len` samples, the first downbeat at `phase`.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub len: usize,
    pub rate: u32,
    pub beats: usize,
    pub phase: usize,
}

impl Grid {
    fn secs(&self) -> f64 {
        self.len as f64 / self.rate as f64
    }

    /// Seconds per beat.
    fn beat(&self) -> f64 {
        self.secs() / self.beats as f64
    }

    /// Beats since the first downbeat at sample i, in [0, beats).
    fn pos(&self, i: usize) -> f64 {
        ((i + self.len - self.phase % self.len) % self.len) as f64 / self.len as f64 * self.beats as f64
    }

    /// Beats per phrase: 4 bars when the loop holds a whole number of them, else the loop.
    fn phrase(&self) -> usize {
        if self.beats.is_multiple_of(16) { 16 } else { self.beats }
    }

    /// The nearest frequency that runs a whole number of cycles per loop.
    fn fit(&self, f: f64) -> f64 {
        (f * self.secs()).round().max(1.0) / self.secs()
    }
}

/// The key's root in [lo, 2·lo) Hz, `semis` above it.
fn note(key: Key, semis: i32, lo: f64) -> f64 {
    let mut f = 440.0 * 2f64.powf((key.root as i32 + semis - 9) as f64 / 12.0);
    while f >= 2.0 * lo {
        f /= 2.0;
    }
    while f < lo {
        f *= 2.0;
    }
    f
}

/// The two in-key notes a semitone apart that rub: the fifth and minor
/// sixth (minor), the seventh and octave (major).
fn rub(key: Key) -> [i32; 2] {
    if key.minor { [7, 8] } else { [11, 12] }
}

/// A band-limited sawtooth (polyBLEP) at a whole number of cycles per loop.
/// Each starts at its own phase (from its frequency), so they don't all
/// reset together on the file's first sample (an edge at the seam).
fn saw(g: &Grid, f: f64) -> Vec<f32> {
    let cycles = (g.fit(f) * g.secs()).round();
    let dt = cycles / g.len as f64;
    let start = (f * 0.618_034).fract();
    (0..g.len)
        .map(|i| {
            let t = (i as f64 * cycles / g.len as f64 + start).fract();
            let mut v = 2.0 * t - 1.0;
            if t < dt {
                let x = t / dt;
                v -= x + x - x * x - 1.0;
            } else if t > 1.0 - dt {
                let x = (t - 1.0) / dt;
                v -= x * x + x + x + 1.0;
            }
            v as f32
        })
        .collect()
}

/// White noise that repeats every loop.
fn noise(len: usize, seed: u64) -> Vec<f32> {
    let mut s = seed | 1;
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        })
        .collect()
}

fn mix(parts: &[(&[f32], f32)], len: usize) -> Vec<f32> {
    (0..len).map(|i| parts.iter().map(|(x, g)| x[i] * g).sum()).collect()
}

/// A felt low thump t seconds after it starts (0 outside it): a sine
/// falling from 2·f to f, saturated so it has harmonics a phone can play,
/// with a soft click on the attack.
fn thump(t: f64, f: f64, decay: f64) -> f32 {
    if !(0.0..decay * 6.0).contains(&t) {
        return 0.0;
    }
    let tau = 0.025;
    let ph = f * (t + tau * (1.0 - (-t / tau).exp()));
    let amp = (1.0 - (-t / 0.002).exp()) * (-t / decay).exp();
    let body = ((TAU * ph).sin() * amp * 2.5).tanh() / 2.5f64.tanh();
    let click = (TAU * 1300.0 * t).sin() * (-t / 0.004).exp() * 0.12;
    (body + click) as f32
}

/// Lub-dub at t seconds after the lub.
fn heartbeat_at(t: f64, f: f64, gap: f64) -> f32 {
    thump(t, f, 0.07) + 0.7 * thump(t - gap, f * 0.94, 0.08)
}

pub struct Kit {
    pub tension: Vec<f32>,
    pub tick: Vec<f32>,
    pub heartbeat: Vec<f32>,
    pub sting: Vec<f32>,
}

/// The tension kit for a loop with grid `g` in `key`, levelled for a music
/// bus whose full mix sits at −20 dBFS: the tension stem about 3 dB under
/// it, the clock well under that, the one-shots at the effects' level.
pub fn kit(g: &Grid, key: Key, seed: u64) -> Kit {
    let rate = g.rate;
    let n = g.len;
    let beat = g.beat();
    let phrase = g.phrase() as f64;

    // A drone on the root and fifth, a slow beating pair, breathing over two bars.
    let root = note(key, 0, 55.0);
    let drone_src = mix(&[(&saw(g, root), 0.5), (&saw(g, root * 1.004), 0.5), (&saw(g, root * 1.5), 0.35), (&saw(g, root * 2.0), 0.2)], n);
    let drone = audio::filter_loop(&drone_src, &[Biquad::lowpass(rate, 900.0, 0.8), Biquad::lowpass(rate, 1800.0, 0.6)]);
    let breathe = if g.beats.is_multiple_of(8) { 8.0 } else { g.beats as f64 };

    // An eighth-note pulse on the root an octave up: accents on the beat.
    let pulse_src = mix(&[(&saw(g, root * 2.0), 0.6), (&saw(g, root * 2.0 * 1.007), 0.4)], n);
    let pulse_osc = audio::filter_loop(&pulse_src, &[Biquad::lowpass(rate, 1400.0, 1.4)]);

    // String tremolo on the rubbing pair, two voices each.
    let [a, b] = rub(key);
    let fa = note(key, a, 300.0);
    let fb = fa * 2f64.powf((b - a) as f64 / 12.0);
    let str_src = mix(&[(&saw(g, fa), 0.5), (&saw(g, fa * 1.003), 0.5), (&saw(g, fb), 0.5), (&saw(g, fb * 0.997), 0.5)], n);
    let strings = audio::filter_loop(&str_src, &[Biquad::lowpass(rate, 3000.0, 0.7), Biquad::highpass(rate, 200.0, 0.7)]);

    // A riser through the last bar of each phrase: noise swept up a band-pass, and a gliss.
    let riser_beats = if phrase >= 8.0 { 4.0 } else { (phrase / 2.0).max(1.0) };
    let rise_t = riser_beats * beat;
    let nz = noise(n, seed ^ 0x5eed);
    let riser_u = |i: usize| -> Option<f64> {
        let p = g.pos(i) % phrase;
        (p >= phrase - riser_beats).then(|| (p - (phrase - riser_beats)) / riser_beats)
    };
    let mut bp = Biquad::bandpass(rate, 400.0, 1.6);
    let mut swept = vec![0.0f32; n];
    for pass in 0..2 {
        for i in 0..n {
            if i % 16 == 0 {
                let u = riser_u(i).unwrap_or(0.0);
                bp.retune(Biquad::bandpass(rate, (400.0 * (5000.0f64 / 400.0).powf(u)) as f32, 1.6));
            }
            let y = bp.run(nz[i]);
            if pass == 1 {
                swept[i] = y;
            }
        }
    }
    let glide_lo = note(key, 0, 220.0);
    let gap = (0.45 * beat).min(0.26);
    let hb_f = note(key, 0, 45.0);

    let mut tension = vec![0.0f32; n];
    for (i, out) in tension.iter_mut().enumerate() {
        let p = g.pos(i);
        // Drone breathing.
        let br = 0.7 + 0.3 * (TAU * p / breathe).sin().powi(2);
        // Pulse: eighths, a fast attack, released before the next.
        let e8 = p * 2.0;
        let f8 = e8.fract();
        let t8 = f8 * beat / 2.0;
        let gate = (1.0 - (-t8 / 0.003).exp()) * (-t8 / (0.3 * beat / 2.0)).exp() * (1.0 - smoothstep(0.85, 1.0, f8));
        let accent = if (e8 as usize).is_multiple_of(2) { 1.0 } else { 0.6 };
        // Heartbeat on each bar's downbeat.
        let bar_t = (p % 4.0) * beat;
        let heart = heartbeat_at(bar_t, hb_f, gap);
        // Tremolo in sixteenths, silent at each sixteenth's edge; the swell restarts on a phrase.
        let trem = (0.5 - 0.5 * (TAU * p * 4.0).cos()).powf(1.5);
        let swell = 0.25 + 0.75 * ((p % phrase) / phrase).powf(1.5);
        // Riser.
        let (rn, rg) = match riser_u(i) {
            Some(u) => {
                let amp = u.powf(2.5) * ((1.0 - u) * rise_t / 0.008).min(1.0);
                let ph = glide_lo * rise_t * (2f64.powf(u) - 1.0) / LN_2;
                (swept[i] as f64 * amp, (TAU * ph).sin() * amp)
            }
            None => (0.0, 0.0),
        };
        *out = (drone[i] as f64 * 0.45 * br
            + pulse_osc[i] as f64 * 0.55 * gate * accent
            + heart as f64 * 0.75
            + strings[i] as f64 * 0.22 * trem * swell
            + rn * 1.4
            + rg * 0.12) as f32;
    }
    let tension = audio::limit_loop(&tension, audio::undb(-23.0) / audio::loud_rms(&tension).max(1e-9), rate);

    // The clock: band-passed noise clicks, tick on even eighths, tock on odd, ghost sixteenths.
    let nz = noise(n, seed ^ 0x71c);
    let (mut hi, mut lo) = (vec![0.0f32; n], vec![0.0f32; n]);
    for i in 0..n {
        let p = g.pos(i);
        let s16 = p * 4.0;
        let k = s16 as usize;
        let t = s16.fract() * beat / 4.0;
        let env = ((1.0 - (-t / 0.0003).exp()) * (-t / 0.005).exp()) as f32;
        let level = match k % 4 {
            0 if k.is_multiple_of(16) => 1.0,
            0 => 0.8,
            2 => 0.65,
            _ => 0.22,
        };
        if (k / 2).is_multiple_of(2) {
            hi[i] = nz[i] * env * level;
        } else {
            lo[i] = nz[i] * env * level;
        }
    }
    let hi = audio::filter_loop(&hi, &[Biquad::bandpass(rate, 3200.0, 3.0)]);
    let lo = audio::filter_loop(&lo, &[Biquad::bandpass(rate, 2300.0, 3.0)]);
    let tick: Vec<f32> = hi.iter().zip(&lo).map(|(a, b)| a + b).collect();
    // Clicks are mostly silence between them: level by the whole file's energy, 10 dB under the stem's.
    let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32;
    let tick = audio::limit_loop(&tick, rms(&tension) * audio::undb(-10.0) / rms(&tick).max(1e-9), rate);

    // One-shots.
    let hb_len = (gap + 0.6) * rate as f64;
    let heartbeat: Vec<f32> = (0..hb_len as usize).map(|i| heartbeat_at(i as f64 / rate as f64, hb_f, gap)).collect();
    // A heartbeat is all sub-bass: give it the harmonics a phone speaker can play.
    let heartbeat = audio::brighten(&heartbeat, rate, 1.5);
    let heartbeat = audio::normalize(&audio::trim_silence(&heartbeat, rate, -60.0), audio::Kind::Sfx, rate);
    let sting = sting(rate, key, seed);
    Kit { tension, tick, heartbeat, sting }
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A near-catch hit: a low boom, a string stab on the root and the rubbing
/// pair whose filter closes as it dies, and a burst of noise.
fn sting(rate: u32, key: Key, seed: u64) -> Vec<f32> {
    let secs = 1.6;
    let n = (secs * rate as f64) as usize;
    let r = rate as f64;
    let [a, b] = rub(key);
    let notes = [note(key, 0, 110.0), note(key, 0, 220.0), note(key, a, 220.0), note(key, b, 220.0) * if b > a { 1.0 } else { 2.0 }];
    let nz = noise(n, seed ^ 0x57);
    let mut stab = vec![0.0f32; n];
    let mut phases = [0.0f64; 8];
    for (i, v) in stab.iter_mut().enumerate() {
        let t = i as f64 / r;
        let env = (1.0 - (-t / 0.002).exp()) * (-t / 0.45).exp();
        let mut s = 0.0;
        for (k, f) in notes.iter().enumerate() {
            for (d, det) in [1.0, 1.004].iter().enumerate() {
                let ph = &mut phases[k * 2 + d];
                *ph = (*ph + f * det / r).fract();
                s += 2.0 * *ph - 1.0;
            }
        }
        *v = (s / 8.0 * env) as f32;
    }
    // The stab's low-pass closes from 5 kHz to 600 Hz over 0.8 s.
    let mut lp = Biquad::lowpass(rate, 5000.0, 0.9);
    for (i, v) in stab.iter_mut().enumerate() {
        if i % 16 == 0 {
            let u = (i as f64 / r / 0.8).min(1.0);
            lp.retune(Biquad::lowpass(rate, (5000.0 * (600.0f64 / 5000.0).powf(u)) as f32, 0.9));
        }
        *v = lp.run(*v);
    }
    let burst = audio::filter(&nz, &[Biquad::bandpass(rate, 1500.0, 0.8)]);
    let hb_f = note(key, 0, 45.0);
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / r;
            let fade = ((secs - t) / 0.05).min(1.0) as f32;
            (thump(t, hb_f, 0.35) * 0.9 + stab[i] * 1.1 + burst[i] * ((-t / 0.12).exp() as f32) * 0.5) * fade
        })
        .collect();
    // The boom is sub-bass: give it the harmonics a phone speaker can play.
    audio::normalize(&audio::brighten(&x, rate, 1.0), audio::Kind::Sfx, rate)
}

/// Pitch class `pc` in [lo, 2·lo) Hz.
fn pitch(pc: u8, lo: f64) -> f64 {
    note(Key { root: pc % 12, minor: false, fit: 1.0 }, 0, lo)
}

/// A transition stinger into a section (`mb music --zones`): a riser over
/// `rise_beats` beats (swept noise, a gliding tone and the chord swelling in)
/// that lands exactly `land` samples in on a hit: a low boom and a stab on
/// `chord`, the pitch classes the next section opens with, whose filter
/// closes as it rings. Returns (samples, land).
pub fn stinger(rate: u32, key: Key, beat: f64, rise_beats: f64, chord: &[u8], seed: u64) -> (Vec<f32>, usize) {
    let r = rate as f64;
    let rise = beat * rise_beats;
    let tail = 1.4;
    let n = ((rise + tail) * r) as usize;
    let land = (rise * r).round() as usize;
    let nz = noise(n, seed ^ 0x57196e);
    let root = chord.first().copied().unwrap_or(key.root);
    let mut freqs = vec![pitch(root, 110.0)];
    freqs.extend(chord.iter().map(|&pc| pitch(pc, 220.0)));
    freqs.push(pitch(root, 440.0));
    // The swept noise.
    let mut bp = Biquad::bandpass(rate, 400.0, 1.4);
    let mut swept = vec![0.0f32; n];
    for (i, v) in swept.iter_mut().enumerate().take(land) {
        if i % 16 == 0 {
            let u = i as f64 / land as f64;
            bp.retune(Biquad::bandpass(rate, (400.0 * (6000.0f64 / 400.0).powf(u)) as f32, 1.4));
        }
        *v = bp.run(nz[i]);
    }
    let burst = audio::filter(&nz, &[Biquad::bandpass(rate, 1800.0, 0.7)]);
    let glide_lo = pitch(key.root, 220.0);
    let hb_f = pitch(root, 45.0);
    let mut phases = vec![0.0f64; freqs.len() * 2];
    let mut lp = Biquad::lowpass(rate, 6000.0, 0.9);
    let mut out = vec![0.0f32; n];
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f64 / r;
        // The chord's saws, two detuned voices each.
        let mut saw = 0.0;
        for (k, f) in freqs.iter().enumerate() {
            for (d, det) in [1.0, 1.005].iter().enumerate() {
                let ph = &mut phases[k * 2 + d];
                *ph = (*ph + f * det / r).fract();
                saw += 2.0 * *ph - 1.0;
            }
        }
        let saw = saw / (freqs.len() * 2) as f64;
        let v = if i < land {
            let u = t / rise;
            let ph = glide_lo * rise * (2f64.powf(u) - 1.0) / std::f64::consts::LN_2;
            let swell = u.powi(3);
            swept[i] as f64 * u.powf(2.0) * 1.6 + (TAU * ph).sin() * u.powf(2.5) * 0.15 + saw * swell * 0.5
        } else {
            let th = t - rise;
            if i % 16 == 0 {
                let u = (th / 0.9).min(1.0);
                lp.retune(Biquad::lowpass(rate, (6000.0 * (500.0f64 / 6000.0).powf(u)) as f32, 0.9));
            }
            let env = (1.0 - (-th / 0.002).exp()) * (-th / 0.5).exp();
            let stab = lp.run((saw * env) as f32) as f64;
            thump(th, hb_f, 0.3) as f64 * 0.8 + stab * 1.4 + burst[i] as f64 * (-th / 0.1).exp() * 0.5
        };
        let fade = ((rise + tail - t) / 0.08).min(1.0);
        *o = (v * fade) as f32;
    }
    (audio::normalize(&out, audio::Kind::Sfx, rate), land)
}

/// A swell into a bar (`<name>_swell`, the Director's cover for a switch
/// into a loop without a stinger of its own): over one beat, noise opening
/// upward like a reversed cymbal and the key's root and fifth swelling in,
/// landing `land` samples in on a soft hit (a low thump on the root, a
/// short airy burst) that rings for under a second. No chord, so it fits
/// whichever loop of the piece comes next. Returns (samples, land).
pub fn swell(rate: u32, key: Key, beat: f64, seed: u64) -> (Vec<f32>, usize) {
    let r = rate as f64;
    let rise = beat;
    let tail = 0.9;
    let n = ((rise + tail) * r) as usize;
    let land = (rise * r).round() as usize;
    let nz = noise(n, seed ^ 0x5e11);
    let air = audio::filter(&nz, &[Biquad::highpass(rate, 3500.0, 0.7)]);
    let mut bp = Biquad::bandpass(rate, 1500.0, 0.9);
    let (f1, f5) = (note(key, 0, 110.0), note(key, 7, 110.0));
    let hb_f = note(key, 0, 45.0);
    let out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / r;
            if i % 16 == 0 && i < land {
                let u = t / rise;
                bp.retune(Biquad::bandpass(rate, (1500.0 * (9000.0f64 / 1500.0).powf(u)) as f32, 0.9));
            }
            let v = if i < land {
                let u = t / rise;
                let swept = bp.run(nz[i]) as f64;
                let tone = ((TAU * f1 * t).sin() + 0.7 * (TAU * f5 * t).sin() + 0.4 * (TAU * 2.0 * f1 * t).sin()) / 2.1;
                swept * u.powi(3) * 1.4 + air[i] as f64 * u.powi(4) * 0.5 + tone * u.powi(2) * 0.25
            } else {
                let th = t - rise;
                thump(th, hb_f, 0.22) as f64 * 0.6 + air[i] as f64 * (-th / 0.18).exp() * 0.45
            };
            let fade = ((rise + tail - t) / 0.1).min(1.0);
            (v * fade) as f32
        })
        .collect();
    (audio::normalize(&audio::brighten(&out, rate, 0.5), audio::Kind::Sfx, rate), land)
}

// --- how loud the Director plays the kit over this take ---------------------------

/// The lift `mb music` aims the tension at, at full danger, over a typical
/// (median) 3 s of the take as a phone plays it. mix_check.py wants at least
/// +3 dB at the tension's peak; aiming the typical stretch higher keeps the
/// loudest stretches of the take over +3 too.
pub const LIFT_DB: f32 = 4.5;
/// `music::Levels::default()` at full danger: the tension layer's vol, the
/// clock's and the heartbeat's (a test holds them to the SDK's).
pub const DEFAULT_LEVELS: (f32, f32, f32) = (1.4, 1.0, 0.3);
/// The platform plays a voice at vol 4 at most, so the gain stops where the
/// tension layer gets there.
pub const MAX_GAIN: f32 = 4.0 / DEFAULT_LEVELS.0;
/// mix_check.py's loudness block, and its 3 s window of them.
const BLOCK_S: f64 = 0.1;
const WINDOW_BLOCKS: usize = 30;

/// How loud the tension kit should play over a take (the graph's `levels`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lift {
    /// What `music::Levels::for_graph` multiplies the kit's default levels by.
    pub gain: f32,
    /// What the kit lifts a typical 3 s of the take by on a phone at full
    /// danger: at the default levels, and with the gain.
    pub default_db: f32,
    pub db: f32,
}

/// `x` as the ear hears it from a phone speaker (mix_check.py's model):
/// K-weighted, then rolled off under [`audio::PHONE_HP`] (4th order).
pub fn on_phone(x: &[f32], rate: u32) -> Vec<f32> {
    let [k1, k2] = audio::k_filters(rate);
    let hp = |q| Biquad::highpass(rate, audio::PHONE_HP, q);
    audio::filter(x, &[k1, k2, hp(audio::BUTTER4_Q[0]), hp(audio::BUTTER4_Q[1])])
}

/// Mean power per 0.1 s block of an already-weighted signal.
pub fn block_power(y: &[f32], rate: u32) -> Vec<f64> {
    let h = (rate as f64 * BLOCK_S) as usize;
    y.chunks_exact(h.max(1)).map(|c| c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / h as f64).collect()
}

/// The gain on the Director's default tension levels that makes the kit
/// lift `take` (at the level its files are written at) by [`LIFT_DB`] on a
/// phone at full danger, as mix_check.py measures it: the music's power over
/// 3 s, with and without the tension (the layer at its full vol, the clock
/// at its, and the heartbeat on three beats of every bar at its loudest),
/// K-weighted and through a phone speaker. Against the take's median 3 s.
/// At least 1 (the defaults pass a take that needs less), at most [`MAX_GAIN`].
pub fn lift(take: &[f32], kit: &Kit, rate: u32, bar_seconds: f64) -> Lift {
    let mean = |x: &[f32]| -> f64 { on_phone(x, rate).iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64 };
    let blocks = block_power(&on_phone(take, rate), rate);
    let mut win: Vec<f64> = blocks.windows(WINDOW_BLOCKS.min(blocks.len()).max(1)).map(|w| w.iter().sum::<f64>() / w.len() as f64).filter(|p| *p > 1e-12).collect();
    win.sort_by(f64::total_cmp);
    let music = win.get(win.len() / 2).copied().unwrap_or(1e-12);
    // The heartbeat's energy, three plays a bar, as mean power.
    let hb_energy: f64 = on_phone(&kit.heartbeat, rate).iter().map(|v| (*v as f64).powi(2)).sum();
    let (t, k, h) = DEFAULT_LEVELS;
    let kit_power = (t as f64).powi(2) * mean(&kit.tension) + (k as f64).powi(2) * mean(&kit.tick) + (h as f64).powi(2) * 3.0 * hb_energy / (bar_seconds * rate as f64);
    let db = |g: f64| (10.0 * (1.0 + g * g * kit_power / music).log10()) as f32;
    let want = 10f64.powf(LIFT_DB as f64 / 10.0) - 1.0;
    let g = (want * music / kit_power.max(1e-15)).sqrt();
    // Rounded up to a hundredth, so the graph's gain still reaches the aim.
    let gain = ((g * 100.0).ceil() / 100.0).clamp(1.0, MAX_GAIN as f64);
    Lift { gain: gain as f32, default_db: db(1.0), db: db(gain) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swell_rises_into_its_mark() {
        let rate = 22050;
        let (x, land) = swell(rate, Key { root: 5, minor: true, fit: 1.0 }, 0.5, 9);
        assert_eq!(land, rate as usize / 2);
        let w = (rate / 50) as usize;
        let e = |s: usize| -> f32 { x[s..s + w].iter().map(|v| v * v).sum() };
        // It grows toward the bar and is loudest around it, then dies away.
        assert!(e(land - w) > 4.0 * e(land / 4), "a swell");
        assert!(e(land) > 4.0 * e(x.len() - 2 * w), "it rings out");
        assert!(x.len() < rate as usize * 3 / 2 + land);
        assert!(audio::db(audio::peak(&x)) <= audio::CEILING_DB);
    }

    #[test]
    fn stinger_lands_on_its_mark() {
        let rate = 22050;
        let (x, land) = stinger(rate, Key { root: 9, minor: true, fit: 1.0 }, 0.5, 2.0, &[9, 0, 4], 3);
        assert_eq!(land, rate as usize);
        // The loudest 10 ms is just after the land, not in the riser.
        let w = (rate / 100) as usize;
        let e = |s: usize| -> f32 { x[s..s + w].iter().map(|v| v * v).sum() };
        let top = (0..x.len() - w).step_by(w / 2).max_by(|a, b| e(*a).total_cmp(&e(*b))).unwrap();
        assert!(top >= land.saturating_sub(w) && top < land + rate as usize / 5, "{top} vs {land}");
        assert!(e(land) > 4.0 * e(land - rate as usize / 2), "a hit, not a swell");
        assert!(audio::db(audio::peak(&x)) <= audio::CEILING_DB);
    }

    fn grid() -> Grid {
        let rate = 22050;
        let bpm = 120.0;
        let beats = 16;
        Grid { len: (beats as f64 * 60.0 / bpm * rate as f64) as usize, rate, beats, phase: 1234 }
    }

    #[test]
    fn kit_loops_without_a_seam_and_is_in_key() {
        let g = grid();
        let key = Key { root: 9, minor: true, fit: 1.0 };
        let k = kit(&g, key, 7);
        for (name, x) in [("tension", &k.tension), ("tick", &k.tick)] {
            assert_eq!(x.len(), g.len, "{name} is exactly the loop");
            assert!(audio::seam_ratio(x) < 4.0, "{name} seam {}", audio::seam_ratio(x));
        }
        assert!((audio::db(audio::loud_rms(&k.tension)) + 23.0).abs() < 1.5, "{}", audio::db(audio::loud_rms(&k.tension)));
        assert!(audio::db(audio::peak(&k.tension)) <= audio::CEILING_DB);
        // Its pitched energy sits in A minor.
        let fit = audio::keys_of(&audio::chroma(&k.tension, g.rate)).into_iter().find(|c| c.root == 9 && c.minor).unwrap().fit;
        assert!(fit > 0.75, "{fit}");
        assert!(k.heartbeat.len() < g.rate as usize && k.sting.len() > g.rate as usize);
        // Heard on a phone: harmonics carry the low parts.
        assert!(audio::phone_loss_db(&k.tension, g.rate, false) > -10.0);
        let hb = audio::phone_loss_db(&k.heartbeat, g.rate, true);
        assert!(hb > audio::PHONE_WARN_DB, "{hb}");
    }

    #[test]
    fn pulses_land_on_the_grid() {
        let g = grid();
        let k = kit(&g, Key { root: 0, minor: false, fit: 1.0 }, 1);
        // The clock's clicks are on the eighths, nothing between the sixteenths.
        let w = (g.rate / 100) as usize;
        let energy = |start: f64| -> f32 { (0..w).map(|j| k.tick[(start as usize + j) % g.len].powi(2)).sum() };
        let eighth = g.len as f64 / (g.beats * 2) as f64;
        let on: f32 = (0..g.beats * 2).map(|e| energy(g.phase as f64 + e as f64 * eighth)).sum();
        let off: f32 = (0..g.beats * 2).map(|e| energy(g.phase as f64 + (e as f64 + 0.25) * eighth + eighth / 8.0)).sum();
        assert!(on > 20.0 * off, "{on} vs {off}");
        // And beat_phase finds the same grid in the tension stem.
        let (ph, conf) = audio::beat_phase(&k.tension, g.rate, g.beats);
        let beat = g.len / g.beats;
        let off = (ph as i64 - g.phase as i64).rem_euclid(beat as i64);
        assert!(off.min(beat as i64 - off) < (g.rate / 50) as i64, "{ph} vs {} ({conf})", g.phase);
    }
}
