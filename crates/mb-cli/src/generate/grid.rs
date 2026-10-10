//! The bar grid of a generated piece, and how well a loop cut from it joins
//! end to start (`mb music --zones/--sections/--variations`; score.rs).
//!
//! **One grid per piece.** A take is one performance at one tempo, so it has
//! one bar grid: bar lines at `downbeat + k·bar`. Every loop cut from it
//! starts on one of those bar lines, so all of them put the same beat of the
//! bar at the start of the file, and a switch on a bar line keeps the bar
//! (even if the grid's 1 were a beat off, it would be off alike everywhere).
//!
//! **Which beat is the 1.** Onsets give the beat well; the bar is harder.
//! Taiko, plucked strings and hand drums don't put a kick on every 1, and
//! the strongest low onsets alone picked a different beat in five of eight
//! sections of one take. Chords and melodies change on the 1 far more often
//! than between beats, so the harmonic change across each beat (pitch-class
//! profiles over two beats and over a bar on either side) votes too, with
//! the low onsets as a lighter third. On four Lyria 3.5 takes of eight
//! sections, a section's own vote agreed with the whole piece's in 28 of 32
//! sections this way, against 13 of 32 for the low onsets alone.
//!
//! **How well a loop joins.** A loop of P bars from bar line s plays bar
//! P−1 into bar 0 where the music played bar P−1 into bar P. The join can't
//! be heard when bar P sounds like bar 0 (what the baked crossfade blends)
//! and bar P−1 like bar −1. Each pair of bars is compared beat by beat:
//! pitch-class profiles (harmony and melody), band energies per half beat
//! (timbre and level) and the onset envelope (rhythm; the waveform's attack
//! pattern, which lines up because both bars sit on the same grid).

use super::audio::{self, Biquad};

/// A piece's bar grid in samples: bar lines at `downbeat + k·bar` (4/4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub downbeat: f64,
    pub bar: f64,
}

impl Grid {
    pub fn beat(&self) -> f64 {
        self.bar / 4.0
    }

    /// The bar lines in [lo, hi].
    pub fn bars_in(&self, lo: f64, hi: f64) -> Vec<f64> {
        let k0 = ((lo - self.downbeat) / self.bar).ceil() as i64;
        (k0..).map(|k| self.downbeat + k as f64 * self.bar).take_while(|&p| p <= hi).collect()
    }

    /// The bar number of the bar line at (or nearest) `pos`.
    pub fn bar_number(&self, pos: f64) -> i64 {
        ((pos - self.downbeat) / self.bar).round() as i64
    }
}

const HOP_HZ: u32 = 200; // 5 ms onset frames

/// Onset strength per 5 ms frame: rises in log energy, as a share of the mean.
fn onset_env(x: &[f32], hop: usize, emph: f32) -> Vec<f32> {
    let frames = x.len() / hop;
    let e: Vec<f32> = (0..frames)
        .map(|k| {
            let s = k * hop;
            let sum: f32 = (s.max(1)..s + hop).map(|j| (x[j] - emph * x[j - 1]).powi(2)).sum();
            (sum / hop as f32 + 1e-10).ln()
        })
        .collect();
    let d: Vec<f32> = (0..frames).map(|k| if k == 0 { 0.0 } else { (e[k] - e[k - 1]).max(0.0) }).collect();
    let m = d.iter().sum::<f32>() / d.len().max(1) as f32;
    d.iter().map(|v| v / m.max(1e-9)).collect()
}

fn lows(x: &[f32], rate: u32) -> Vec<f32> {
    audio::filter(x, &[Biquad::lowpass(rate, 150.0, std::f32::consts::FRAC_1_SQRT_2)])
}

/// The strongest onset within a frame of `pos` (samples).
fn at(env: &[f32], hop: usize, pos: f64) -> f32 {
    if env.is_empty() || pos < 0.0 {
        return 0.0;
    }
    let k = (pos / hop as f64).round() as usize;
    if k >= env.len() {
        return 0.0;
    }
    env[k.saturating_sub(1)].max(env[k]).max(env[(k + 1).min(env.len() - 1)])
}

/// Where the beats fall in `x` given the beat length (samples, fractional):
/// the position in [0, beat) of the comb of beats the onsets fit best
/// (low onsets weighted twice: kicks sit on beats, hats between them), and
/// how much better than the average comb (0–1).
pub fn beat_phase(x: &[f32], rate: u32, beat: f64) -> (f64, f32) {
    let hop = (rate / HOP_HZ).max(1) as usize;
    if x.len() < 4 * hop || beat < 2.0 * hop as f64 {
        return (0.0, 0.0);
    }
    let all = onset_env(x, hop, 0.97);
    let lo = onset_env(&lows(x, rate), hop, 0.0);
    let env: Vec<f32> = all.iter().zip(&lo).map(|(a, l)| a + 2.0 * l).collect();
    let beats = ((x.len() as f64 - beat) / beat).floor().max(1.0) as usize;
    let steps = ((beat / hop as f64) as usize).max(1);
    let scores: Vec<f32> = (0..steps).map(|p| (0..beats).map(|k| at(&env, hop, p as f64 * hop as f64 + k as f64 * beat)).sum()).collect();
    let (best, top) = scores.iter().enumerate().fold((0, f32::MIN), |b, (i, &s)| if s > b.1 { (i, s) } else { b });
    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let conf = ((top - mean) / top.max(1e-9)).clamp(0.0, 1.0);
    // Parabolic refinement between frames (the comb wraps at the beat).
    let n = scores.len();
    let (a, c) = (scores[(best + n - 1) % n], scores[(best + 1) % n]);
    let denom = a - 2.0 * top + c;
    let off = if denom.abs() > 1e-9 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    (((best as f64 + off as f64) * hop as f64).rem_euclid(beat), conf)
}

/// A pitch-class profile of `x[from..from + len]`, scaled to unit length (zero for silence).
fn profile(x: &[f32], rate: u32, from: usize, len: usize) -> [f32; 12] {
    let to = (from + len).min(x.len());
    if to <= from {
        return [0.0; 12];
    }
    let ch = audio::chroma(&x[from..to], rate);
    let n = ch.iter().map(|v| v * v).sum::<f64>().sqrt();
    if n < 1e-12 {
        return [0.0; 12];
    }
    ch.map(|v| (v / n) as f32)
}

fn cos(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (mut ab, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..12 {
        ab += a[i] * b[i];
        aa += a[i] * a[i];
        bb += b[i] * b[i];
    }
    if aa < 1e-12 || bb < 1e-12 { 0.0 } else { ab / (aa * bb).sqrt() }
}

fn sum(v: &[[f32; 12]]) -> [f32; 12] {
    let mut s = [0.0; 12];
    for p in v {
        for i in 0..12 {
            s[i] += p[i];
        }
    }
    s
}

/// The piece's downbeat: which beat of the bar (0–3, counted from `first`,
/// a beat) is the 1, and the vote of each of `ranges` (sample spans: the
/// sections) on its own, for how much they agree.
pub struct Downbeat {
    pub class: usize,
    pub per_range: Vec<Option<usize>>,
}

/// Votes for which beat is the 1, per beat: (harmonic change over 2 beats, over 4 beats, low onset).
fn votes(x: &[f32], rate: u32, first: f64, beat: f64) -> Vec<(f64, [f32; 3])> {
    let hop = (rate / HOP_HZ).max(1) as usize;
    let lo = onset_env(&lows(x, rate), hop, 0.0);
    // A profile per beat, at least 0.4 s of audio (chroma's frame).
    let win = (beat.max(0.4 * rate as f64)) as usize;
    let mut pos = Vec::new();
    let mut p = first;
    while p + win as f64 <= x.len() as f64 {
        pos.push(p);
        p += beat;
    }
    let prof: Vec<[f32; 12]> = pos.iter().map(|&p| profile(x, rate, p.round() as usize, win)).collect();
    let change = |k: usize, w: usize| -> f32 {
        if k < w || k + w > prof.len() {
            return f32::NAN;
        }
        1.0 - cos(&sum(&prof[k - w..k]), &sum(&prof[k..k + w]))
    };
    pos.iter().enumerate().map(|(k, &p)| (p, [change(k, 2), change(k, 4), at(&lo, hop, p)])).collect()
}

/// Which of the 4 beat classes the votes in `v` (index: beat number) pick, and their scores.
fn pick(v: &[(usize, [f32; 3])]) -> Option<(usize, [f32; 4])> {
    const WEIGHT: [f32; 3] = [1.0, 1.0, 0.25];
    let mut total = [0.0f32; 4];
    for (f, w) in WEIGHT.iter().enumerate() {
        let mut m = [0.0f32; 4];
        for (j, mj) in m.iter_mut().enumerate() {
            let vals: Vec<f32> = v.iter().filter(|(k, s)| k % 4 == j && s[f].is_finite()).map(|(_, s)| s[f]).collect();
            if vals.is_empty() {
                return None;
            }
            *mj = vals.iter().sum::<f32>() / vals.len() as f32;
        }
        let mean = m.iter().sum::<f32>() / 4.0;
        let sd = (m.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / 4.0).sqrt();
        for j in 0..4 {
            total[j] += w * (m[j] - mean) / sd.max(1e-9);
        }
    }
    let best = (0..4).fold(0, |b, j| if total[j] > total[b] { j } else { b });
    Some((best, total))
}

pub fn downbeat(x: &[f32], rate: u32, first: f64, beat: f64, ranges: &[(f64, f64)]) -> Downbeat {
    let v: Vec<(usize, [f32; 3])> = votes(x, rate, first, beat).into_iter().enumerate().map(|(k, (_, s))| (k, s)).collect();
    let class = pick(&v).map_or(0, |p| p.0);
    let per_range = ranges
        .iter()
        .map(|&(a, b)| {
            let inside: Vec<(usize, [f32; 3])> = v.iter().filter(|(k, _)| (a..b).contains(&(first + *k as f64 * beat))).copied().collect();
            pick(&inside).map(|p| p.0)
        })
        .collect();
    Downbeat { class, per_range }
}

/// Band centres for the timbre comparison, Hz (about an octave apart).
const BANDS: [f32; 8] = [90.0, 180.0, 360.0, 720.0, 1440.0, 2880.0, 5760.0, 11000.0];

/// What a stretch of audio sounds like, beat by beat on a grid, for comparing bars.
pub struct Feats {
    /// Position of beat 0 in the signal, and the beat length (samples).
    pub first: f64,
    pub beat: f64,
    /// A pitch-class profile per beat.
    prof: Vec<[f32; 12]>,
    /// Band levels per half beat (dB), [half-beat][band].
    bands: Vec<[f32; 8]>,
    onset: Vec<f32>,
    hop: usize,
}

impl Feats {
    /// The features of `x` for each whole beat at `first + k·beat` (first ≥ 0) that fits in it.
    pub fn new(x: &[f32], rate: u32, first: f64, beat: f64) -> Feats {
        let hop = (rate / HOP_HZ).max(1) as usize;
        let mut first = first;
        while first >= beat {
            first -= beat;
        }
        let n = ((x.len() as f64 - first) / beat).floor().max(0.0) as usize;
        let win = (beat.max(0.4 * rate as f64)) as usize;
        let prof = (0..n).map(|k| profile(x, rate, (first + k as f64 * beat).round() as usize, win)).collect();
        let filtered: Vec<Vec<f32>> = BANDS.iter().map(|&f| audio::filter(x, &[Biquad::bandpass(rate, f.min(rate as f32 * 0.45), 1.0)])).collect();
        let half = beat / 2.0;
        let bands = (0..2 * n)
            .map(|h| {
                let a = (first + h as f64 * half).round() as usize;
                let b = ((first + (h + 1) as f64 * half).round() as usize).min(x.len());
                let mut out = [0.0f32; 8];
                for (o, y) in out.iter_mut().zip(&filtered) {
                    let p = y[a..b].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (b - a).max(1) as f64;
                    *o = (10.0 * (p + 1e-10).log10()) as f32;
                }
                out
            })
            .collect();
        // The onset envelope, widened by a frame each way (a few ms of timing between performances).
        let env = onset_env(x, hop, 0.97);
        let onset = (0..env.len()).map(|k| env[k.saturating_sub(1)].max(env[k]).max(env[(k + 1).min(env.len() - 1)])).collect();
        Feats { first, beat, prof, bands, onset, hop }
    }

    /// The beat index at sample `pos` (a beat of the grid).
    pub fn index(&self, pos: f64) -> isize {
        ((pos - self.first) / self.beat).round() as isize
    }

    /// How alike `n` beats from beat `i` and from beat `j` are: (harmony, timbre, rhythm), each 0–1,
    /// or None if either runs off the features.
    pub fn compare(&self, i: isize, j: isize, n: usize) -> Option<Match> {
        let ok = |k: isize| k >= 0 && (k as usize + n) <= self.prof.len();
        if !ok(i) || !ok(j) || n == 0 {
            return None;
        }
        let (i, j) = (i as usize, j as usize);
        let harmony = (0..n).map(|k| cos(&self.prof[i + k], &self.prof[j + k])).sum::<f32>() / n as f32;
        let mut d = 0.0f32;
        for h in 0..2 * n {
            let (a, b) = (&self.bands[2 * i + h], &self.bands[2 * j + h]);
            d += (0..8).map(|q| (a[q] - b[q]).abs().min(30.0)).sum::<f32>() / 8.0;
        }
        let level_db = d / (2 * n) as f32;
        let fa = ((self.first + i as f64 * self.beat) / self.hop as f64).round() as usize;
        let fb = ((self.first + j as f64 * self.beat) / self.hop as f64).round() as usize;
        let len = ((n as f64 * self.beat) / self.hop as f64) as usize;
        let rhythm = if fa + len <= self.onset.len() && fb + len <= self.onset.len() { ncc(&self.onset[fa..fa + len], &self.onset[fb..fb + len]) } else { 0.0 };
        Some(Match::from_parts(harmony, level_db, rhythm))
    }
}

fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let (ma, mb) = (a[..n].iter().sum::<f32>() / n as f32, b[..n].iter().sum::<f32>() / n as f32);
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for k in 0..n {
        let (x, y) = ((a[k] - ma) as f64, (b[k] - mb) as f64);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa < 1e-12 || bb < 1e-12 { 0.0 } else { (ab / (aa * bb).sqrt()) as f32 }
}

/// How alike two bars are, and the parts it's made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Match {
    /// Pitch-class profiles beat by beat (cosine).
    pub harmony: f32,
    /// Mean difference of the band levels per half beat, dB.
    pub level_db: f32,
    /// The onset envelopes' correlation.
    pub rhythm: f32,
    /// All three as one number, 0–1.
    pub score: f32,
}

impl Match {
    pub fn from_parts(harmony: f32, level_db: f32, rhythm: f32) -> Match {
        // Scaled on Lyria takes: two bars of one groove on the same chord, a
        // few bars apart, measure harmony 0.9–0.99, levels 1–2 dB apart and a
        // rhythm correlation of 0.4–0.65 (two performances of a pattern, not
        // one); the same groove on another chord about 0.2–0.5 in harmony.
        let h = ((harmony - 0.45) / 0.5).clamp(0.0, 1.0);
        let t = (1.0 - (level_db - 1.0) / 5.0).clamp(0.0, 1.0);
        let r = (rhythm / 0.5).clamp(0.0, 1.0);
        Match { harmony, level_db, rhythm, score: 0.4 * h + 0.35 * t + 0.25 * r }
    }

    /// `a` and `b` weighted `1 − w` and `w` (`b` may be missing).
    pub fn blend(a: Match, b: Option<Match>, w: f32) -> Match {
        match b {
            None => a,
            Some(b) => Match {
                harmony: a.harmony * (1.0 - w) + b.harmony * w,
                level_db: a.level_db * (1.0 - w) + b.level_db * w,
                rhythm: a.rhythm * (1.0 - w) + b.rhythm * w,
                score: a.score * (1.0 - w) + b.score * w,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_bars() {
        let g = Grid { downbeat: 100.0, bar: 1000.0 };
        assert_eq!(g.bars_in(0.0, 2500.0), vec![100.0, 1100.0, 2100.0]);
        assert_eq!(g.bars_in(1100.0, 1100.0), vec![1100.0]);
        assert_eq!(g.bar_number(-900.0), -1);
        assert_eq!(g.beat(), 250.0);
    }
}
