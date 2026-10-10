//! Audio post-processing for `mb music` and `mb sfx`: decoding what a
//! provider returns (WAV, MP3, Ogg, raw PCM), mono at one sample rate,
//! tempo estimation, cutting a seamless whole-bar loop, splitting base/high
//! layers, loudness normalization with a look-ahead limiter (the same rules
//! as the skill's sfx.py: sfx ≈ −14 dBFS, music ≈ −20 dBFS RMS of the loud
//! part, peaks ≤ −3 dBFS before encoding, ≤ −1 dBFS decoded), and Ogg Vorbis
//! encoding (libvorbis, built into mb), checked by decoding the result. Also
//! `mb ogg`, which converts files to Ogg (and back to WAV) for the skill's
//! scripts, so no platform needs vorbis-tools.

use std::path::Path;

pub const CEILING_DB: f32 = -3.0;
/// The limiter aims this far under CEILING_DB: Vorbis encoding adds a few
/// tenths of a dB to peaks, and the file should still read under −3.
pub const ENCODE_HEADROOM_DB: f32 = 0.5;
pub const DECODED_MAX_DB: f32 = -1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Sfx,
    Music,
}

impl Kind {
    pub fn target_db(self) -> f32 {
        match self {
            Kind::Sfx => -14.0,
            Kind::Music => -20.0,
        }
    }
}

pub fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

pub fn undb(d: f32) -> f32 {
    10f32.powf(d / 20.0)
}

// --- decoding -------------------------------------------------------------------

/// Decodes audio bytes to (channels, sample rate). `mime` may be
/// "audio/pcm;rate=24000" for raw signed 16-bit little-endian mono.
pub fn decode(bytes: &[u8], mime: &str) -> Result<(Vec<Vec<f32>>, u32), String> {
    if let Some(rate) = mime.strip_prefix("audio/pcm;rate=") {
        let rate: u32 = rate.parse().map_err(|_| format!("bad PCM rate in {mime}"))?;
        let s = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        return Ok((vec![s], rate));
    }
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    let mut hint = Hint::new();
    match mime {
        "audio/mpeg" | "audio/mp3" => {
            hint.with_extension("mp3");
        }
        "audio/wav" | "audio/x-wav" | "audio/wave" => {
            hint.with_extension("wav");
        }
        "audio/ogg" => {
            hint.with_extension("ogg");
        }
        _ => {}
    }
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes.to_vec())), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("can't read the audio ({mime}): {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or("no audio track")?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or("no audio codec parameters")?.clone();
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|e| format!("can't decode the audio: {e}"))?;
    let mut chans: Vec<Vec<f32>> = Vec::new();
    let mut rate = params.sample_rate.unwrap_or(0);
    let mut inter: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(_) => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = buf.spec();
                let n = spec.channels().count().max(1);
                rate = spec.rate();
                inter.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut inter);
                if chans.is_empty() {
                    chans = vec![Vec::new(); n];
                }
                for frame in inter.chunks_exact(n) {
                    for (c, v) in frame.iter().enumerate() {
                        if c < chans.len() {
                            chans[c].push(*v);
                        }
                    }
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("decoding: {e}")),
        }
    }
    if chans.is_empty() || chans[0].is_empty() || rate == 0 {
        return Err("the audio decoded to nothing".into());
    }
    Ok((chans, rate))
}

pub fn to_mono(chans: &[Vec<f32>]) -> Vec<f32> {
    let n = chans.iter().map(Vec::len).min().unwrap_or(0);
    let k = 1.0 / chans.len().max(1) as f32;
    (0..n).map(|i| chans.iter().map(|c| c[i]).sum::<f32>() * k).collect()
}

/// Windowed-sinc resampling (Blackman window, 32 zero crossings).
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    resample_ratio(x, to as f64 / from as f64)
}

/// `x` rotated to start at sample `at` (a loop stays seamless).
pub fn rotate(x: &[f32], at: usize) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let at = at % x.len();
    x[at..].iter().chain(&x[..at]).copied().collect()
}

/// Time-stretch without changing pitch (WSOLA: 40 ms Hann grains at 50 %
/// overlap, each placed within ±10 ms where it best continues the last), to
/// `factor` × the length. Clean for the few percent a tempo correction needs.
pub fn stretch(x: &[f32], rate: u32, factor: f64) -> Vec<f32> {
    if (factor - 1.0).abs() < 1e-6 || x.is_empty() {
        return x.to_vec();
    }
    let n = ((0.04 * rate as f64) as usize).max(16) & !1;
    let hop = n / 2;
    let tol = (0.01 * rate as f64) as usize;
    let out_len = (x.len() as f64 * factor).round() as usize;
    let win: Vec<f32> = (0..n).map(|k| (0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos()) as f32).collect();
    let mut out = vec![0.0f32; out_len + n];
    let at = |i: isize| -> f32 { if i >= 0 && (i as usize) < x.len() { x[i as usize] } else { 0.0 } };
    let mut prev: isize = 0;
    let mut k = 0usize;
    while k * hop < out_len {
        let nominal = (k * hop) as f64 / factor;
        let pos = if k == 0 {
            0
        } else {
            // Where the previous grain would naturally continue, and the best match near the nominal position.
            let natural = prev + hop as isize;
            let center = nominal.round() as isize;
            let mut best = (center, f32::MIN);
            let mut d = -(tol as isize);
            while d <= tol as isize {
                let c = center + d;
                let mut s = 0.0f32;
                for j in (0..n).step_by(4) {
                    s += at(natural + j as isize) * at(c + j as isize);
                }
                if s > best.1 {
                    best = (c, s);
                }
                d += 2;
            }
            best.0
        };
        let o = k * hop;
        for j in 0..n {
            out[o + j] += at(pos + j as isize) * win[j];
        }
        prev = pos;
        k += 1;
    }
    out.truncate(out_len);
    out
}

/// Shift pitch by `semis` semitones keeping the length: stretch, then resample back.
pub fn pitch_shift(x: &[f32], rate: u32, semis: f32) -> Vec<f32> {
    if semis.abs() < 1e-4 {
        return x.to_vec();
    }
    let r = 2f64.powf(semis as f64 / 12.0);
    let y = stretch(x, rate, r);
    let mut z = resample_ratio(&y, x.len() as f64 / y.len().max(1) as f64);
    z.resize(x.len(), 0.0);
    z
}

/// How long the music in `x` takes to repeat near `len` samples (±`dev` as a
/// fraction): the lag around `len` where the onset envelope best matches
/// itself, refined to a sample or so. Gives a section's own tempo against a
/// loop length to within a few hundredths of a percent.
pub fn local_period(x: &[f32], rate: u32, len: usize, dev: f64) -> Option<(usize, f32)> {
    let hop = (rate / 200).max(1) as usize; // 5 ms
    let env: Vec<f32> = {
        let frames = x.len() / hop;
        let e: Vec<f32> = (0..frames)
            .map(|k| {
                let s = k * hop;
                let sum: f32 = (s.max(1)..s + hop).map(|j| (x[j] - 0.97 * x[j - 1]).powi(2)).sum();
                (sum / hop as f32 + 1e-10).ln()
            })
            .collect();
        (0..frames).map(|k| if k == 0 { 0.0 } else { (e[k] - e[k - 1]).max(0.0) }).collect()
    };
    let lag0 = len / hop;
    let span = ((len as f64 * dev) / hop as f64).ceil() as usize;
    if lag0 + span + 20 >= env.len() {
        return None;
    }
    let ac = |lag: usize| -> f32 {
        let n = env.len() - lag;
        let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let (a, b) = (env[i] as f64, env[i + lag] as f64);
            ab += a * b;
            aa += a * a;
            bb += b * b;
        }
        if aa < 1e-12 || bb < 1e-12 { 0.0 } else { (ab / (aa * bb).sqrt()) as f32 }
    };
    let lo = lag0.saturating_sub(span).max(1);
    let vals: Vec<(usize, f32)> = (lo - 1..=lag0 + span + 1).map(|l| (l, ac(l))).collect();
    let (bi, &(lag, v)) = vals.iter().enumerate().skip(1).take(vals.len() - 2).max_by(|a, b| a.1.1.total_cmp(&b.1.1))?;
    // A maximum at the edge of the range isn't a period, just a slope.
    if bi == 1 || bi == vals.len() - 2 {
        return None;
    }
    let (a, c) = (vals[bi - 1].1, vals[bi + 1].1);
    let denom = a - 2.0 * v + c;
    let off = if denom.abs() > 1e-9 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    Some((((lag as f32 + off) * hop as f32).round() as usize, v))
}

/// Windowed-sinc resampling by `ratio` (output length = input × ratio).
pub fn resample_ratio(x: &[f32], ratio: f64) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let cutoff = ratio.min(1.0) * 0.95;
    let half = 16.0 / cutoff.min(1.0);
    let n_out = (x.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(n_out);
    for o in 0..n_out {
        let t = o as f64 / ratio;
        let lo = (t - half).ceil().max(0.0) as usize;
        let hi = ((t + half).floor() as usize).min(x.len() - 1);
        let mut acc = 0.0;
        let mut wsum = 0.0;
        for (i, &xi) in x.iter().enumerate().take(hi + 1).skip(lo) {
            let d = i as f64 - t;
            let s = if d.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * d * cutoff).sin() / (std::f64::consts::PI * d * cutoff) };
            let wpos = (d / half + 1.0) / 2.0; // 0..1 across the window
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * wpos).cos() + 0.08 * (4.0 * std::f64::consts::PI * wpos).cos();
            let k = s * w;
            acc += xi as f64 * k;
            wsum += k;
        }
        out.push(if wsum.abs() > 1e-9 { (acc / wsum) as f32 } else { 0.0 });
    }
    out
}

// --- levels ---------------------------------------------------------------------

/// RMS of the loudest two thirds of the samples (ignores near-silence), as sfx.py does.
pub fn loud_rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    let mut sq: Vec<f32> = x.iter().map(|v| v * v).collect();
    sq.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let top = &sq[sq.len() / 3..];
    (top.iter().map(|&v| v as f64).sum::<f64>() / top.len().max(1) as f64).sqrt() as f32
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

pub fn gain_for(x: &[f32], kind: Kind) -> f32 {
    undb(kind.target_db()) / loud_rms(x).max(1e-9)
}

/// Applies gain g, then a look-ahead peak limiter (a smooth gain envelope that
/// is already down when a peak arrives and recovers over ~80 ms), so no sample
/// exceeds CEILING_DB and no distortion is added. Port of sfx.py's `_limit`.
pub fn limit(x: &[f32], g: f32, rate: u32) -> Vec<f32> {
    let ceiling = undb(CEILING_DB - ENCODE_HEADROOM_DB);
    let y: Vec<f32> = x.iter().map(|v| v * g).collect();
    let n = y.len();
    let need: Vec<f32> = y.iter().map(|v| if v.abs() > ceiling { ceiling / v.abs() } else { 1.0 }).collect();
    let la = ((0.0015 * rate as f32) as usize).max(1);
    // m[i] = min(need[i-la ..= i+la]) via a monotonic deque.
    let mut m = vec![1.0f32; n];
    let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for j in 0..n + la {
        if j < n {
            while q.back().is_some_and(|&b| need[b] >= need[j]) {
                q.pop_back();
            }
            q.push_back(j);
        }
        if j >= la {
            let i = j - la;
            while q.front().is_some_and(|&f| f + la < i) {
                q.pop_front();
            }
            m[i] = need[*q.front().unwrap()];
        }
    }
    let mut acc = vec![0.0f64; n + 1];
    for i in 0..n {
        acc[i + 1] = acc[i] + m[i] as f64;
    }
    let k = 1.0 - (-1.0 / (0.08 * rate as f32)).exp();
    let mut env = 1.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let lo = i.saturating_sub(la);
        let hi = (i + la + 1).min(n);
        let a = ((acc[hi] - acc[lo]) / (hi - lo) as f64) as f32;
        env = a.min(env + (1.0 - env) * k);
        out.push(y[i] * env);
    }
    out
}

pub fn normalize(x: &[f32], kind: Kind, rate: u32) -> Vec<f32> {
    limit(x, gain_for(x, kind), rate)
}

/// `limit` for a loop: limited as if it were playing on repeat, so the
/// gain at its end and its start agree (no step at the seam).
pub fn limit_loop(x: &[f32], g: f32, rate: u32) -> Vec<f32> {
    let n = x.len();
    let three: Vec<f32> = x.iter().chain(x).chain(x).copied().collect();
    limit(&three, g, rate)[n..2 * n].to_vec()
}

// --- trimming (sfx) -------------------------------------------------------------

/// Trims leading and trailing near-silence (below `floor_db` relative to the
/// peak), keeping 3 ms before the onset, then fades the last 8 ms.
/// Cuts `x` to at most `secs` seconds with a short fade-out (models often
/// return more than was asked for, e.g. 1.2 s for a 0.6 s effect).
pub fn cut_to(x: &[f32], rate: u32, secs: f32) -> Vec<f32> {
    let n = ((secs.max(0.05) * rate as f32) as usize).min(x.len());
    let mut y = x[..n].to_vec();
    let fade = ((0.015 * rate as f32) as usize).min(n / 2).max(1);
    for i in 0..fade {
        y[n - 1 - i] *= i as f32 / fade as f32;
    }
    y
}

pub fn trim_silence(x: &[f32], rate: u32, floor_db: f32) -> Vec<f32> {
    let p = peak(x);
    if p <= 0.0 {
        return x.to_vec();
    }
    let thr = p * undb(floor_db);
    let first = x.iter().position(|v| v.abs() > thr).unwrap_or(0);
    let last = x.iter().rposition(|v| v.abs() > thr).unwrap_or(x.len() - 1);
    let pre = (0.003 * rate as f32) as usize;
    let post = (0.03 * rate as f32) as usize;
    let mut y = x[first.saturating_sub(pre)..(last + post).min(x.len())].to_vec();
    let f = ((0.008 * rate as f32) as usize).min(y.len() / 2);
    let n = y.len();
    for i in 0..f {
        y[n - 1 - i] *= i as f32 / f as f32;
    }
    y
}

// --- tempo and loops -------------------------------------------------------------

/// An onset-strength envelope at 100 frames per second.
fn onset_envelope(x: &[f32], rate: u32) -> Vec<f32> {
    let hop = (rate / 100) as usize;
    let win = hop * 2;
    let mut energy = Vec::new();
    let mut i = 0;
    // Emphasize transients: first difference before energy.
    while i + win <= x.len() {
        let e: f32 = (i + 1..i + win).map(|j| (x[j] - 0.97 * x[j - 1]).powi(2)).sum::<f32>() / win as f32;
        energy.push((e + 1e-10).ln());
        i += hop;
    }
    let mut flux: Vec<f32> = (0..energy.len()).map(|k| if k == 0 { 0.0 } else { (energy[k] - energy[k - 1]).max(0.0) }).collect();
    // Remove the local mean so sustained parts don't dominate.
    let w = 20;
    let mut out = vec![0.0; flux.len()];
    for k in 0..flux.len() {
        let lo = k.saturating_sub(w);
        let hi = (k + w + 1).min(flux.len());
        let mean = flux[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        out[k] = (flux[k] - mean).max(0.0);
    }
    flux.clear();
    out
}

/// Estimated tempo in BPM (60–200) and a confidence 0..1, from the
/// autocorrelation of the onset envelope.
pub fn detect_bpm(x: &[f32], rate: u32) -> Option<(f32, f32)> {
    let env = onset_envelope(x, rate);
    if env.len() < 400 {
        return None;
    }
    let fps = 100.0f32;
    let ac = |lag: usize| -> f32 { env.iter().zip(&env[lag..]).map(|(a, b)| a * b).sum::<f32>() / (env.len() - lag) as f32 };
    let zero = ac(0).max(1e-12);
    let (lo, hi) = ((fps * 60.0 / 200.0) as usize, (fps * 60.0 / 60.0) as usize);
    let mut best = (0usize, f32::MIN);
    let mut vals = vec![0.0; hi + 2];
    for (lag, v) in vals.iter_mut().enumerate().take(hi + 2).skip(lo.saturating_sub(1)) {
        *v = ac(lag);
    }
    for lag in lo..=hi {
        let bpm = 60.0 * fps / lag as f32;
        // Prefer tempos near 120 (a log-Gaussian), and reward periodicity at 2× the lag too.
        let pref = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        let two = if lag * 2 < env.len() / 2 { ac(lag * 2) } else { 0.0 };
        let score = (vals[lag] + 0.5 * two) * (0.6 + 0.4 * pref);
        if score > best.1 {
            best = (lag, score);
        }
    }
    let lag = best.0;
    if lag == 0 {
        return None;
    }
    // Parabolic refinement.
    let (a, b, c) = (vals[lag - 1], vals[lag], vals[lag + 1]);
    let denom = a - 2.0 * b + c;
    let off = if denom.abs() > 1e-12 { 0.5 * (a - c) / denom } else { 0.0 };
    let period = lag as f32 + off.clamp(-0.5, 0.5);
    let conf = (vals[lag] / zero).clamp(0.0, 1.0);
    Some((60.0 * fps / period, conf))
}

fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        ab += (*x as f64) * (*y as f64);
        aa += (*x as f64).powi(2);
        bb += (*y as f64).powi(2);
    }
    if aa < 1e-12 || bb < 1e-12 { 0.0 } else { (ab / (aa * bb).sqrt()) as f32 }
}

fn env_rms(x: &[f32], hop: usize) -> Vec<f32> {
    x.chunks(hop).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect()
}

/// Finds the best loop of `len` samples (± `slack` samples) in x: where
/// the audio just after the end best matches the audio just after the start,
/// both in loudness contour (~1 s) and waveform (~20 ms). Skips the first
/// `skip` samples (intros fade in) and leaves room for a crossfade.
pub fn find_loop(x: &[f32], rate: u32, len: usize, slack: usize, skip: usize, xfade: usize) -> Option<(usize, usize, f32)> {
    let ctx = (rate as usize).min(len / 4); // contour context
    if x.len() < skip + len + slack + ctx.max(xfade) + 1 {
        return None;
    }
    let hop = (rate / 100) as usize;
    let env = env_rms(x, hop);
    let last_start = x.len() - len - slack - ctx.max(xfade) - 1;
    let mut cands: Vec<(f32, usize)> = Vec::new();
    let mut s = skip;
    let ctx_frames = ctx / hop;
    while s <= last_start {
        let e = s + len;
        let (fs, fe) = (s / hop, e / hop);
        if fe + ctx_frames >= env.len() {
            break;
        }
        let a = &env[fs..fs + ctx_frames];
        let b = &env[fe..fe + ctx_frames];
        let mean_a = a.iter().sum::<f32>() / a.len() as f32;
        let mean_b = b.iter().sum::<f32>() / b.len() as f32;
        let level = (mean_a.min(mean_b) / mean_a.max(mean_b).max(1e-9)).clamp(0.0, 1.0);
        let shape = ncc(a, b).max(0.0);
        cands.push((0.6 * shape + 0.4 * level, s));
        s += hop;
    }
    cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let w = (0.02 * rate as f32) as usize;
    let mut best: Option<(usize, usize, f32)> = None;
    for &(coarse, s) in cands.iter().take(12) {
        // Fine-align the end at sample level within ±slack (at least ±1 period of 50 Hz).
        let span = slack.max((rate / 50) as usize);
        let a = &x[s..s + w];
        let mut bl = len;
        let mut bc = f32::MIN;
        let lo = len.saturating_sub(span);
        for l in lo..=len + span {
            if s + l + w > x.len() {
                break;
            }
            let c = ncc(a, &x[s + l..s + l + w]);
            if c > bc {
                bc = c;
                bl = l;
            }
        }
        let score = 0.5 * coarse + 0.5 * bc.max(0.0);
        if best.is_none_or(|b| score > b.2) {
            best = Some((s, bl, score));
        }
    }
    best
}

/// The loop region with its seam cross-faded: the first `xfade` samples blend
/// from what followed the end in the source into the start, so playing the
/// result on repeat is continuous.
pub fn cut_loop(x: &[f32], start: usize, len: usize, xfade: usize) -> Vec<f32> {
    let mut out = x[start..start + len].to_vec();
    let xf = xfade.min(len / 2).min(x.len().saturating_sub(start + len));
    for i in 0..xf {
        let t = (i as f32 + 0.5) / xf as f32;
        let t = t * t * (3.0 - 2.0 * t);
        out[i] = x[start + i] * t + x[start + len + i] * (1.0 - t);
    }
    out
}

/// A loop of `len` samples from `start` with its tail baked in: the `xfade`
/// samples that followed its end in the source are cross-faded into its
/// head (equal power: sine in, cosine out, corrected for how alike the two
/// are so the level holds whether they're the same groove or not). The
/// file is exactly `len` long and repeats seamlessly: its end runs on into
/// what really came next, which turns into its start over the crossfade.
/// The crossfade is shortened to what the source has after the loop.
/// Returns the loop and the crossfade used.
pub fn bake_loop(x: &[f32], start: usize, len: usize, xfade: usize) -> (Vec<f32>, usize) {
    let mut out = x[start..start + len].to_vec();
    let xf = xfade.min(len).min(x.len().saturating_sub(start + len));
    if xf == 0 {
        return (out, 0);
    }
    let (head, tail) = (&x[start..start + xf], &x[start + len..start + len + xf]);
    // How alike they are: identical material would gain 3 dB mid-fade under a plain equal-power curve.
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for (a, b) in head.iter().zip(tail) {
        ab += (*a as f64) * (*b as f64);
        aa += (*a as f64).powi(2);
        bb += (*b as f64).powi(2);
    }
    let rho = if aa > 1e-12 && bb > 1e-12 { (ab / (aa * bb).sqrt()).clamp(0.0, 1.0) } else { 0.0 };
    for i in 0..xf {
        let t = (i as f64 + 0.5) / xf as f64 * std::f64::consts::FRAC_PI_2;
        let (gi, go) = (t.sin(), t.cos());
        let norm = (1.0 + 2.0 * rho * gi * go).sqrt();
        out[i] = ((head[i] as f64 * gi + tail[i] as f64 * go) / norm) as f32;
    }
    (out, xf)
}

/// `x` repeated to exactly `n` samples (a short phrase written as a whole loop).
pub fn repeat_to(x: &[f32], n: usize) -> Vec<f32> {
    x.iter().copied().cycle().take(if x.is_empty() { 0 } else { n }).collect()
}

/// Jump at the loop seam relative to the typical sample-to-sample change
/// (check_audio.py's measure: above ~4× is an audible click).
pub fn seam_ratio(x: &[f32]) -> f32 {
    if x.len() < 3 {
        return 0.0;
    }
    // Compared with the sample-to-sample change right around the seam (the
    // last and first 256 samples), so a loop that wraps inside a noisy hi-hat
    // isn't mistaken for a click.
    let seam = (x[0] - x[x.len() - 1]).abs();
    let n = (x.len() / 2).min(256);
    let head = (1..n).map(|i| (x[i] - x[i - 1]).abs());
    let tail = (x.len() - n + 1..x.len()).map(|i| (x[i] - x[i - 1]).abs());
    let typical = head.chain(tail).fold(0.0f32, |a, v| a + v) / (2 * (n - 1)).max(1) as f32;
    seam / typical.max(1e-9)
}

// --- layers -----------------------------------------------------------------------

/// A Linkwitz-Riley low-pass (two cascaded Butterworth biquads), run over the
/// loop twice so its state at the seam is the steady state (seamless).
pub fn lowpass_loop(x: &[f32], rate: u32, cutoff: f32) -> Vec<f32> {
    let w0 = 2.0 * std::f32::consts::PI * cutoff / rate as f32;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let a0 = 1.0 + alpha;
    let b0 = (1.0 - cos) / 2.0 / a0;
    let b1 = (1.0 - cos) / a0;
    let b2 = b0;
    let a1 = -2.0 * cos / a0;
    let a2 = (1.0 - alpha) / a0;
    let mut st = [[0.0f32; 4]; 2]; // x1 x2 y1 y2 per stage
    let mut out = vec![0.0; x.len()];
    for pass in 0..2 {
        for (i, &v) in x.iter().enumerate() {
            let mut s = v;
            for z in st.iter_mut() {
                let y = b0 * s + b1 * z[0] + b2 * z[1] - a1 * z[2] - a2 * z[3];
                z[1] = z[0];
                z[0] = s;
                z[3] = z[2];
                z[2] = y;
                s = y;
            }
            if pass == 1 {
                out[i] = s;
            }
        }
    }
    out
}

/// Splits a loop into a calm base (low-passed) and a high layer (the rest):
/// base + high = the full mix, so a game plays base alone and fades the high
/// layer in as intensity rises.
pub fn split_layers(x: &[f32], rate: u32, cutoff: f32) -> (Vec<f32>, Vec<f32>) {
    let base = lowpass_loop(x, rate, cutoff);
    let high = x.iter().zip(&base).map(|(a, b)| a - b).collect();
    (base, high)
}

// --- filters ------------------------------------------------------------------------

/// A biquad filter (RBJ's cookbook), transposed direct form II in f64.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    pub fn new(b: [f64; 3], a: [f64; 3]) -> Biquad {
        Biquad { b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]], a: [a[1] / a[0], a[2] / a[0]], z: [0.0; 2] }
    }

    fn cos_alpha(rate: u32, f: f32, q: f32) -> (f64, f64) {
        let w = std::f64::consts::TAU * (f as f64).min(rate as f64 * 0.49) / rate as f64;
        (w.cos(), w.sin() / (2.0 * q as f64))
    }

    pub fn lowpass(rate: u32, f: f32, q: f32) -> Biquad {
        let (c, al) = Self::cos_alpha(rate, f, q);
        Biquad::new([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al])
    }

    pub fn highpass(rate: u32, f: f32, q: f32) -> Biquad {
        let (c, al) = Self::cos_alpha(rate, f, q);
        Biquad::new([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al])
    }

    /// A band-pass with 0 dB at its centre.
    pub fn bandpass(rate: u32, f: f32, q: f32) -> Biquad {
        let (c, al) = Self::cos_alpha(rate, f, q);
        Biquad::new([al, 0.0, -al], [1.0 + al, -2.0 * c, 1.0 - al])
    }

    /// New coefficients, same state (a sweeping filter).
    pub fn retune(&mut self, to: Biquad) {
        self.b = to.b;
        self.a = to.a;
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y as f32
    }
}

/// Butterworth Qs for a 4th-order (24 dB/octave) filter made of two biquads.
pub const BUTTER4_Q: [f32; 2] = [0.541_196_1, 1.306_563];

/// x through the stages in order, from rest.
pub fn filter(x: &[f32], stages: &[Biquad]) -> Vec<f32> {
    let mut st = stages.to_vec();
    x.iter().map(|&v| st.iter_mut().fold(v, |s, f| f.run(s))).collect()
}

/// The same over a loop: run twice and keep the second pass, so the state
/// at the seam is the steady state (seamless).
pub fn filter_loop(x: &[f32], stages: &[Biquad]) -> Vec<f32> {
    let mut st = stages.to_vec();
    for &v in x {
        st.iter_mut().fold(v, |s, f| f.run(s));
    }
    x.iter().map(|&v| st.iter_mut().fold(v, |s, f| f.run(s))).collect()
}

// --- heard on a phone -----------------------------------------------------------------

/// A phone speaker barely plays below this (a 4th-order roll-off, close to
/// an iPhone's): the same model as the skill's mix_check.py.
pub const PHONE_HP: f32 = 300.0;
/// Past this many dB lost on a phone, an effect is mostly bass it can't play.
pub const PHONE_WARN_DB: f32 = -10.0;

/// BS.1770 K-weighting (the ear): the shelf and the high-pass, at any rate.
pub fn k_filters(rate: u32) -> [Biquad; 2] {
    let r = rate as f64;
    let w = std::f64::consts::TAU * 1681.974450955533 / r;
    let (a, c) = (10f64.powf(4.0 / 40.0), w.cos());
    let s = 2.0 * a.sqrt() * w.sin() / (2.0 * 0.7071752369554196);
    let shelf = Biquad::new(
        [a * ((a + 1.0) + (a - 1.0) * c + s), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - s)],
        [(a + 1.0) - (a - 1.0) * c + s, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - s],
    );
    let w = std::f64::consts::TAU * 38.13547087602444 / r;
    let (c, al) = (w.cos(), w.sin() / (2.0 * 0.5003270373238773));
    let hp = Biquad::new([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + al, -2.0 * c, 1.0 - al]);
    [shelf, hp]
}

/// How many dB of `x` a phone speaker loses as the ear hears it (≤ 0):
/// K-weighted, then rolled off under PHONE_HP. With `peak`, over its
/// loudest 50 ms (what mix_check.py measures of each play of an effect);
/// otherwise over all of it (a loop).
pub fn phone_loss_db(x: &[f32], rate: u32, peak: bool) -> f32 {
    let [k1, k2] = k_filters(rate);
    let ear = filter(x, &[k1, k2]);
    let phone = filter(&ear, &[Biquad::highpass(rate, PHONE_HP, BUTTER4_Q[0]), Biquad::highpass(rate, PHONE_HP, BUTTER4_Q[1])]);
    let w = (0.05 * rate as f32) as usize;
    let (e, p) = if peak && x.len() > w {
        let mut ce = vec![0.0f64; x.len() + 1];
        let mut cp = vec![0.0f64; x.len() + 1];
        for i in 0..x.len() {
            ce[i + 1] = ce[i] + (ear[i] as f64).powi(2);
            cp[i + 1] = cp[i] + (phone[i] as f64).powi(2);
        }
        let j = (0..=x.len() - w).step_by((w / 4).max(1)).max_by(|&a, &b| (ce[a + w] - ce[a]).total_cmp(&(ce[b + w] - ce[b]))).unwrap_or(0);
        (ce[j + w] - ce[j], cp[j + w] - cp[j])
    } else {
        (ear.iter().map(|v| (*v as f64).powi(2)).sum(), phone.iter().map(|v| (*v as f64).powi(2)).sum())
    };
    (10.0 * (p.max(1e-30) / e.max(1e-30)).log10()) as f32
}

/// "Virtual bass" for a phone speaker: the part under ~250 Hz is bent
/// through a soft clipper and a rectifier, which makes its harmonics (2×,
/// 3×, 4×… the frequency); the ones a phone can play (280 Hz–4 kHz) are
/// added back, `amount` times as loud as the bass, and the rumble under
/// 35 Hz is cut. The ear hears the missing fundamental from its harmonics,
/// so a deep thud still reads as deep on a phone, and is as before on
/// headphones.
pub fn brighten(x: &[f32], rate: u32, amount: f32) -> Vec<f32> {
    let low = filter(x, &[Biquad::lowpass(rate, 250.0, BUTTER4_Q[0]), Biquad::lowpass(rate, 250.0, BUTTER4_Q[1])]);
    let p = peak(&low).max(1e-6);
    let drive = 4.0f32;
    let bent: Vec<f32> = low.iter().map(|v| (v / p * drive).tanh() / drive.tanh() * 0.6 + (v / p).abs() * 0.4).collect();
    let h = filter(
        &bent,
        &[
            Biquad::highpass(rate, 280.0, BUTTER4_Q[0]),
            Biquad::highpass(rate, 280.0, BUTTER4_Q[1]),
            Biquad::lowpass(rate, 4000.0, std::f32::consts::FRAC_1_SQRT_2),
        ],
    );
    let g = amount * loud_rms(&low) / loud_rms(&h).max(1e-9);
    let y: Vec<f32> = x.iter().zip(&h).map(|(a, b)| a + g * b).collect();
    filter(&y, &[Biquad::highpass(rate, 35.0, std::f32::consts::FRAC_1_SQRT_2)])
}

// --- key ----------------------------------------------------------------------------

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const MAJOR_PROFILE: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR_PROFILE: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
const MAJOR_SCALE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR_SCALE: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

/// A key: its root pitch class (0 = C), major or minor, and how well the
/// music fits it (the share of its pitched energy on the key's scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub root: u8,
    pub minor: bool,
    pub fit: f32,
}

impl Key {
    pub fn name(&self) -> String {
        format!("{} {}", NOTE_NAMES[self.root as usize % 12], if self.minor { "minor" } else { "major" })
    }

    pub fn scale(&self) -> [u8; 7] {
        if self.minor { MINOR_SCALE } else { MAJOR_SCALE }
    }

    /// "A", "Am", "A minor", "F#m", "Bb major", "c# min".
    pub fn parse(s: &str) -> Option<Key> {
        let s = s.trim();
        let mut chars = s.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        let mut root: i32 = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let mut rest = chars.as_str();
        if let Some(r) = rest.strip_prefix(['#', '♯']) {
            root += 1;
            rest = r;
        } else if let Some(r) = rest.strip_prefix(['b', '♭']) {
            root -= 1;
            rest = r;
        }
        let minor = match rest.trim().to_ascii_lowercase().as_str() {
            "" | "maj" | "major" => false,
            "m" | "min" | "minor" => true,
            _ => return None,
        };
        Some(Key { root: root.rem_euclid(12) as u8, minor, fit: 1.0 })
    }
}

/// Power per pitch class from 55 Hz to 2 kHz over the first 30 s, as the
/// skill's mix_check.py measures it: Hann frames of ~0.37 s at ~5.5 kHz,
/// each semitone sampled at its centre and ±33 cents (Goertzel).
pub fn chroma(x: &[f32], rate: u32) -> [f64; 12] {
    let d = (rate / 5512).max(1) as usize;
    let lp = filter(&x[..x.len().min(30 * rate as usize)], &[Biquad::lowpass(rate, 2400.0, BUTTER4_Q[0]), Biquad::lowpass(rate, 2400.0, BUTTER4_Q[1])]);
    let y: Vec<f32> = lp.iter().step_by(d).copied().collect();
    let r = rate as f64 / d as f64;
    let n = 2048usize;
    let mut out = [0.0f64; 12];
    if y.len() < n {
        return out;
    }
    let win: Vec<f64> = (0..n).map(|k| 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / (n - 1) as f64).cos()).collect();
    // MIDI 33 (A1, 55 Hz) up to 95 (B6, ~1976 Hz).
    let probes: Vec<(f64, usize)> = (33..=95)
        .flat_map(|m| [-1.0 / 3.0, 0.0, 1.0 / 3.0].map(move |c| (440.0 * 2f64.powf((m as f64 + c - 69.0) / 12.0), m as usize % 12)))
        .map(|(f, pc)| (2.0 * (std::f64::consts::TAU * f / r).cos(), pc))
        .collect();
    let mut frame = vec![0.0f64; n];
    for s in (0..=y.len() - n).step_by(n / 2) {
        for k in 0..n {
            frame[k] = y[s + k] as f64 * win[k];
        }
        for &(coeff, pc) in &probes {
            let (mut s1, mut s2) = (0.0f64, 0.0f64);
            for &v in &frame {
                let s0 = v + coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            out[pc] += s1 * s1 + s2 * s2 - coeff * s1 * s2;
        }
    }
    out
}

/// All 24 keys, best first: the share of the chroma's power on each key's
/// scale, ties between relative keys broken by the Krumhansl profiles
/// (mix_check.py's `keys_of`, so both name the same key).
pub fn keys_of(ch: &[f64; 12]) -> Vec<Key> {
    let tot = ch.iter().sum::<f64>().max(1e-12);
    let corr = |a: &[f64], b: &[f64]| {
        let (ma, mb) = (a.iter().sum::<f64>() / 12.0, b.iter().sum::<f64>() / 12.0);
        let num: f64 = (0..12).map(|i| (a[i] - ma) * (b[i] - mb)).sum();
        let den = ((0..12).map(|i| (a[i] - ma).powi(2)).sum::<f64>() * (0..12).map(|i| (b[i] - mb).powi(2)).sum::<f64>()).sqrt();
        num / den.max(1e-9)
    };
    let mut cand: Vec<(Key, f64)> = Vec::new();
    for root in 0..12u8 {
        for minor in [false, true] {
            let k = Key { root, minor, fit: 0.0 };
            let fit = k.scale().iter().map(|d| ch[(root + d) as usize % 12]).sum::<f64>() / tot;
            let rot: Vec<f64> = (0..12).map(|i| ch[(i + root as usize) % 12]).collect();
            let c = corr(&rot, if minor { &MINOR_PROFILE } else { &MAJOR_PROFILE });
            cand.push((Key { fit: fit as f32, ..k }, (fit * 100.0).round() / 100.0 + 0.001 * c));
        }
    }
    cand.sort_by(|a, b| b.1.total_cmp(&a.1));
    cand.into_iter().map(|(k, _)| k).collect()
}

pub fn detect_key(x: &[f32], rate: u32) -> Key {
    keys_of(&chroma(x, rate))[0]
}

/// Share of the chroma's power on `key`'s scale with the music moved `semis` up.
pub fn in_scale(ch: &[f64; 12], key: Key, semis: i32) -> f32 {
    let tot = ch.iter().sum::<f64>().max(1e-12);
    let sc = key.scale();
    let on: f64 = (0..12).filter(|&i| sc.iter().any(|d| (key.root + d) % 12 == ((i as i32 + semis).rem_euclid(12)) as u8)).map(|i| ch[i]).sum();
    (on / tot) as f32
}

/// How a piece fits a key, as mix_check.py judges a layer: its share in the
/// key as it is, the shift (−6…+5 semitones) that fits best and that share,
/// and whether it clashes (under 70 % in key, or 8 points worse than the best shift).
pub fn key_fit(ch: &[f64; 12], key: Key) -> (f32, i32, f32, bool) {
    let here = in_scale(ch, key, 0);
    // Nearest shifts first, so a tie keeps the smaller one.
    let order = [0, 1, -1, 2, -2, 3, -3, 4, -4, 5, -5, -6];
    let (best, share) = order.iter().map(|&d| (d, in_scale(ch, key, d))).fold((0, f32::MIN), |b, c| if c.1 > b.1 + 1e-6 { c } else { b });
    (here, best, share, here < 0.7 || here < share - 0.08)
}

// --- beats in a loop ---------------------------------------------------------------

/// Where the beats fall in a loop that is exactly `beats` beats long (so
/// they're len / beats samples apart): the sample of the first downbeat,
/// and how much the onsets favor that grid over others (0–1). The beat comes
/// from the onset envelope folded on the beat; the downbeat is the beat of
/// the bar with the strongest low-end onsets (the kick on 1).
pub fn beat_phase(x: &[f32], rate: u32, beats: usize) -> (usize, f32) {
    let n = x.len();
    let hop = (rate / 200).max(1) as usize; // 5 ms
    let frames = n / hop;
    if beats == 0 || frames < 8 {
        return (0, 0.0);
    }
    let onsets = |sig: &[f32], emphasis: f32| -> Vec<f32> {
        let e: Vec<f32> = (0..frames)
            .map(|k| {
                let s = k * hop;
                let sum: f32 = (s..s + hop).map(|j| (sig[j] - emphasis * sig[(j + n - 1) % n]).powi(2)).sum();
                (sum / hop as f32 + 1e-10).ln()
            })
            .collect();
        (0..frames).map(|k| (e[k] - e[(k + frames - 1) % frames]).max(0.0)).collect()
    };
    let unit = |e: Vec<f32>| -> Vec<f32> {
        let m = e.iter().sum::<f32>() / e.len() as f32;
        e.iter().map(|v| v / m.max(1e-9)).collect()
    };
    let lows = unit(onsets(&filter_loop(x, &[Biquad::lowpass(rate, 150.0, std::f32::consts::FRAC_1_SQRT_2)]), 0.0));
    // Hats and shakers sit between the beats, kicks on them: weight the lows.
    let all: Vec<f32> = unit(onsets(x, 0.97)).iter().zip(&lows).map(|(a, l)| a + 2.0 * l).collect();
    let beat = n as f64 / beats as f64;
    let at = |e: &[f32], s: f64| -> f32 {
        let k = (s.rem_euclid(n as f64) / hop as f64).round() as usize % frames;
        e[(k + frames - 1) % frames].max(e[k]).max(e[(k + 1) % frames])
    };
    let steps = ((beat / hop as f64) as usize).max(1);
    let scores: Vec<f32> = (0..steps).map(|p| (0..beats).map(|k| at(&all, p as f64 * hop as f64 + k as f64 * beat)).sum()).collect();
    let (best, top) = scores.iter().enumerate().fold((0, f32::MIN), |b, (i, &s)| if s > b.1 { (i, s) } else { b });
    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let conf = ((top - mean) / top.max(1e-9)).clamp(0.0, 1.0);
    let ph = best as f64 * hop as f64;
    let per_bar = if beats.is_multiple_of(4) { 4 } else { 1 };
    let down = (0..per_bar)
        .map(|j| (j, (0..beats / per_bar).map(|m| at(&lows, ph + (m * per_bar + j) as f64 * beat)).sum::<f32>()))
        .fold((0, f32::MIN), |b, (j, s)| if s > b.1 { (j, s) } else { b })
        .0;
    (((ph + down as f64 * beat).round() as usize) % n, conf)
}

// --- files --------------------------------------------------------------------------

pub fn wav_bytes(x: &[f32], rate: u32) -> Vec<u8> {
    wav_bytes_ch(&[x], rate)
}

/// 16-bit PCM WAV with the channels interleaved (all the same length).
pub fn wav_bytes_ch(chans: &[&[f32]], rate: u32) -> Vec<u8> {
    let (nch, frames) = (chans.len() as u32, chans.first().map_or(0, |c| c.len()));
    let data_len = frames as u32 * nch * 2;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&(nch as u16).to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * nch * 2).to_le_bytes());
    b.extend_from_slice(&(nch as u16 * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        for c in chans {
            b.extend_from_slice(&((c[i].clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
    }
    b
}

/// Mono samples as Ogg Vorbis at `quality` (as oggenc -q: 0–10, lower is
/// smaller). The stream serial is fixed, so the same samples make the same file.
pub fn ogg_bytes(x: &[f32], rate: u32, quality: f32) -> Result<Vec<u8>, String> {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
    let r = NonZeroU32::new(rate).ok_or("the sample rate is 0")?;
    let fail = |e: vorbis_rs::VorbisError| format!("Vorbis encoding at {rate} Hz, quality {quality}: {e}");
    let mut enc = VorbisEncoderBuilder::new_with_serial(r, NonZeroU8::MIN, Vec::new(), 1)
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr { target_quality: (quality / 10.0).clamp(-0.1, 1.0) })
        .build()
        .map_err(fail)?;
    for block in x.chunks(4096) {
        enc.encode_audio_block([block]).map_err(fail)?;
    }
    enc.finish().map_err(fail)
}

/// Encodes mono samples to Ogg Vorbis at `out` with quality `quality`, then
/// decodes the result and, if Vorbis pushed a peak past DECODED_MAX_DB,
/// scales down and encodes again. Encoding happens in memory, with no temp
/// file (so two encodes of same-named files can run at once), and only the
/// passing file is written to `out`. Returns the decoded samples.
pub fn encode_ogg(x: &[f32], rate: u32, quality: f32, out: &Path) -> Result<Vec<f32>, String> {
    if peak(x) < 1e-4 {
        return Err(format!("{}: the sound is silent", out.display()));
    }
    let limit = undb(DECODED_MAX_DB);
    let mut s = x.to_vec();
    for _ in 0..4 {
        let bytes = ogg_bytes(&s, rate, quality)?;
        let (ch, _) = decode(&bytes, "audio/ogg")?;
        let dec = to_mono(&ch);
        let p = peak(&dec);
        if p <= limit {
            std::fs::write(out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
            return Ok(dec);
        }
        let k = limit / p * 0.97;
        s.iter_mut().for_each(|v| *v *= k);
    }
    Err(format!("{}: decoded peak still above {DECODED_MAX_DB} dBFS", out.display()))
}

/// `mb ogg`: WAV, MP3 or Ogg in, mono Ogg Vorbis out (or, to a .wav, the
/// decoded audio as 16-bit WAV, as oggdec writes it). Levels are left alone;
/// it reports what a player will decode.
pub fn convert(input: &Path, output: &Path, quality: f32, quiet: bool) -> Result<(), String> {
    let ext = |p: &Path| p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let mime = match ext(input).as_str() {
        "wav" | "wave" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        e => return Err(format!("{}: can't read .{e} files (WAV, MP3 or Ogg)", input.display())),
    };
    let bytes = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let (ch, rate) = decode(&bytes, mime).map_err(|e| format!("{}: {e}", input.display()))?;
    if ch.is_empty() || ch[0].is_empty() {
        return Err(format!("{}: no audio in it", input.display()));
    }
    let (written, ch, note) = match ext(output).as_str() {
        "ogg" => {
            let mono = to_mono(&ch);
            let ogg = ogg_bytes(&mono, rate, quality)?;
            let (dec, _) = decode(&ogg, "audio/ogg")?;
            let note = if ch.len() > 1 { format!(", mixed down from {} channels", ch.len()) } else { String::new() };
            (ogg, dec, note)
        }
        "wav" => {
            let refs: Vec<&[f32]> = ch.iter().map(|c| c.as_slice()).collect();
            (wav_bytes_ch(&refs, rate), ch, String::new())
        }
        e => return Err(format!("{}: can't write .{e} files (.ogg to encode, .wav to decode)", output.display())),
    };
    std::fs::write(output, &written).map_err(|e| format!("{}: {e}", output.display()))?;
    let p = db(ch.iter().map(|c| peak(c)).fold(0.0, f32::max));
    if !quiet {
        let layout = if ch.len() == 1 { "mono".to_string() } else { format!("{} channels", ch.len()) };
        eprintln!("{}: {:.2} s · {rate} Hz {layout}{note} · peak {p:.1} dBFS · {:.1} KB", output.display(), ch[0].len() as f32 / rate as f32, written.len() as f32 / 1024.0);
    }
    if !(22050..=48000).contains(&rate) {
        eprintln!("warning: games play 22–48 kHz audio; resample this one ({rate} Hz)");
    }
    if p > -0.5 {
        eprintln!("warning: {}: peaks at {p:.1} dBFS decoded, so it may clip; encode it a few dB quieter", output.display());
    }
    Ok(())
}

/// What check_audio.py reports, for the log.
pub struct Check {
    pub secs: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub seam: Option<f32>,
    /// The seam's jump itself, dBFS.
    pub seam_jump_db: Option<f32>,
}

/// A seam jump this much over the typical change near it is a click…
pub const CLICK_RATIO: f32 = 4.0;
/// …if it's also this loud (dBFS): Vorbis leaves a step of −50 to −45 dB at a file's ends
/// on a smooth (low-passed) layer, many times its tiny sample-to-sample change, and
/// not a click anyone hears under the music.
pub const CLICK_DB: f32 = -40.0;

pub fn check(x: &[f32], rate: u32, is_loop: bool) -> Check {
    let jump = (is_loop && x.len() > 1).then(|| db((x[0] - x[x.len() - 1]).abs()));
    Check { secs: x.len() as f32 / rate as f32, peak_db: db(peak(x)), rms_db: db(loud_rms(x)), seam: is_loop.then(|| seam_ratio(x)), seam_jump_db: jump }
}

impl Check {
    pub fn click(&self) -> bool {
        self.seam.is_some_and(|s| s > CLICK_RATIO) && self.seam_jump_db.is_some_and(|d| d > CLICK_DB)
    }
}

impl std::fmt::Display for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:.2} s · peak {:.1} dB · loud RMS {:.1} dB", self.secs, self.peak_db, self.rms_db)?;
        if let Some(s) = self.seam {
            write!(f, " · loop seam {:.1}× ({:.0} dB) {}", s, self.seam_jump_db.unwrap_or(-120.0), if self.click() { "(CLICK)" } else { "(clean)" })?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A test "song": a kick on every beat, a hat on off-beats, a chord
    /// progression changing every bar, after a 1 s fade-in.
    pub fn song(bpm: f32, secs: f32, rate: u32) -> Vec<f32> {
        let n = (secs * rate as f32) as usize;
        let beat = 60.0 / bpm;
        let chords = [[220.0, 277.18, 329.63], [196.0, 246.94, 293.66], [174.61, 220.0, 261.63], [196.0, 246.94, 293.66]];
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let bt = t % beat;
                let kick = (-bt * 30.0).exp() * (2.0 * std::f32::consts::PI * (50.0 + 80.0 * (-bt * 40.0).exp()) * bt).sin();
                let ht = (t + beat / 2.0) % beat;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0;
                let hat = (-ht * 80.0).exp() * noise * 0.3;
                let bar = ((t / (beat * 4.0)) as usize) % 4;
                let pad: f32 = chords[bar].iter().map(|f| (2.0 * std::f32::consts::PI * f * t).sin()).sum::<f32>() * 0.08;
                let fade = (t / 1.0).min(1.0);
                (kick * 0.6 + hat + pad) * fade
            })
            .collect()
    }

    #[test]
    fn wav_roundtrip_through_symphonia() {
        let x: Vec<f32> = (0..4410).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let (ch, rate) = decode(&wav_bytes(&x, 22050), "audio/wav").unwrap();
        assert_eq!(rate, 22050);
        assert_eq!(ch[0].len(), x.len());
        assert!((ch[0][100] - x[100]).abs() < 1e-3);
    }

    #[test]
    fn raw_pcm() {
        let b: Vec<u8> = [0i16, 16384, -16384].iter().flat_map(|v| v.to_le_bytes()).collect();
        let (ch, rate) = decode(&b, "audio/pcm;rate=24000").unwrap();
        assert_eq!(rate, 24000);
        assert_eq!(ch[0], vec![0.0, 0.5, -0.5]);
    }

    #[test]
    fn resample_keeps_pitch_and_length() {
        let rate = 48000;
        let x: Vec<f32> = (0..rate).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin()).collect();
        let y = resample(&x[..12000], 48000, 44100);
        assert_eq!(y.len(), 11025);
        // Zero crossings ≈ 2 × 440 × 0.25 s.
        let zc = y.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((108..=112).contains(&zc), "{zc}");
    }

    #[test]
    fn normalize_hits_targets_without_clipping() {
        let rate = 44100;
        let x: Vec<f32> = (0..rate).map(|i| (i as f32 * 0.03).sin() * 0.05 + if i == 20000 { 0.9 } else { 0.0 }).collect();
        for kind in [Kind::Sfx, Kind::Music] {
            let y = normalize(&x, kind, rate as u32);
            assert!(db(peak(&y)) <= CEILING_DB - ENCODE_HEADROOM_DB + 0.01);
            assert!((db(loud_rms(&y)) - kind.target_db()).abs() < 1.5, "{kind:?}: {}", db(loud_rms(&y)));
        }
    }

    #[test]
    fn trims_silence() {
        let rate = 44100u32;
        let mut x = vec![0.0f32; 4410];
        x.extend((0..4410).map(|i| (i as f32 * 0.1).sin() * 0.5));
        x.extend(vec![0.0; 8820]);
        let y = trim_silence(&x, rate, -50.0);
        assert!(y.len() < 4410 + 1500 && y.len() > 4410, "{}", y.len());
    }

    #[test]
    fn detects_tempo() {
        for bpm in [96.0, 120.0, 140.0] {
            let x = song(bpm, 20.0, 22050);
            let (got, _) = detect_bpm(&x, 22050).unwrap();
            assert!((got - bpm).abs() / bpm < 0.02, "{bpm} → {got}");
        }
    }

    #[test]
    fn finds_whole_bar_loop_with_clean_seam() {
        let rate = 22050;
        let bpm = 120.0;
        let x = song(bpm, 24.0, rate);
        let bars = 4;
        let len = (bars as f32 * 4.0 * 60.0 / bpm * rate as f32) as usize;
        let (s, l, score) = find_loop(&x, rate, len, len / 200, rate as usize, 1000).unwrap();
        assert!(score > 0.8, "{score}");
        assert!((l as i64 - len as i64).abs() <= (len / 200) as i64 + (rate / 50) as i64);
        assert!(s >= rate as usize, "skips the fade-in");
        let lp = cut_loop(&x, s, l, 1000);
        assert!(seam_ratio(&lp) < 4.0, "{}", seam_ratio(&lp));
    }

    #[test]
    fn layers_sum_to_full_mix() {
        let x = song(120.0, 4.0, 22050);
        let (b, h) = split_layers(&x, 22050, 800.0);
        assert!(b.iter().zip(&h).zip(&x).all(|((b, h), x)| (b + h - x).abs() < 1e-5));
        assert!(loud_rms(&b) > 0.01 && loud_rms(&h) > 0.01);
    }

    #[test]
    fn ogg_encode() {
        let dir = std::env::temp_dir().join(format!("mb-ogg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let x = normalize(&song(120.0, 2.0, 44100), Kind::Music, 44100);
        let out = dir.join("t.ogg");
        let dec = encode_ogg(&x, 44100, 4.0, &out).unwrap();
        assert!(db(peak(&dec)) <= DECODED_MAX_DB + 0.01);
        assert!((dec.len() as i64 - x.len() as i64).abs() < 64, "{} vs {}", dec.len(), x.len());
        assert_eq!(ogg_bytes(&x, 44100, 4.0).unwrap(), ogg_bytes(&x, 44100, 4.0).unwrap(), "the same samples make the same file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_round_trips() {
        let dir = std::env::temp_dir().join(format!("mb-ogg-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 22050;
        let l: Vec<f32> = (0..rate).map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin()).collect();
        let stereo = wav_bytes_ch(&[&l, &vec![0.0; l.len()]], rate as u32);
        std::fs::write(dir.join("in.wav"), stereo).unwrap();
        convert(&dir.join("in.wav"), &dir.join("x.ogg"), 5.0, true).unwrap();
        convert(&dir.join("x.ogg"), &dir.join("out.wav"), 5.0, true).unwrap();
        let (ch, r) = decode(&std::fs::read(dir.join("out.wav")).unwrap(), "audio/wav").unwrap();
        assert_eq!((ch.len(), r), (1, rate as u32), "mono at the same rate");
        assert!((ch[0].len() as i64 - l.len() as i64).abs() < 64);
        assert!((db(peak(&ch[0])) - db(0.25)).abs() < 0.5, "two channels averaged: {}", db(peak(&ch[0])));
        assert!(convert(&dir.join("x.ogg"), &dir.join("x.flac"), 5.0, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sine(f: f32, secs: f32, rate: u32) -> Vec<f32> {
        (0..(secs * rate as f32) as usize).map(|i| (std::f32::consts::TAU * f * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    #[test]
    fn phone_loss_and_brighten() {
        let rate = 44100;
        // A 60 Hz thud: most of it is gone on a phone; a 1 kHz tone isn't touched.
        let thud: Vec<f32> = sine(60.0, 0.6, rate).iter().enumerate().map(|(i, v)| v * (-(i as f32) / rate as f32 / 0.15).exp()).collect();
        let low = phone_loss_db(&thud, rate, true);
        assert!(low < -20.0, "{low}");
        assert!(phone_loss_db(&sine(1000.0, 0.5, rate), rate, true).abs() < 1.0);
        let b = brighten(&thud, rate, 1.0);
        let after = phone_loss_db(&b, rate, true);
        assert!(after > low + 12.0, "{low} → {after}");
        // The thud itself is still there.
        let lp = filter(&b, &[Biquad::lowpass(rate, 150.0, 0.7)]);
        assert!(db(loud_rms(&lp)) > db(loud_rms(&thud)) - 1.5);
    }

    #[test]
    fn keys() {
        assert_eq!(Key::parse("A minor").unwrap().name(), "A minor");
        assert_eq!(Key::parse("F#m").unwrap().name(), "F# minor");
        assert_eq!(Key::parse("Eb").unwrap().name(), "D# major");
        assert_eq!(Key::parse("bb min").unwrap().name(), "A# minor");
        assert!(Key::parse("H").is_none() && Key::parse("C lydian").is_none());
        // The test song: Am, G, F, G (A C# E really: A major's triad) → measured A major or its relative.
        let rate = 22050;
        let am: Vec<f32> = (0..rate as usize * 4)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let chord: &[f32] = if t < 2.0 { &[220.0, 261.63, 329.63] } else { &[174.61, 220.0, 261.63] };
                chord.iter().map(|f| (std::f32::consts::TAU * f * t).sin()).sum::<f32>() * 0.2
            })
            .collect();
        let k = detect_key(&am, rate);
        assert!(k.fit > 0.9, "{k:?}");
        assert!(["A minor", "C major", "F major"].contains(&k.name().as_str()), "{}", k.name());
    }

    #[test]
    fn beat_phase_finds_the_downbeat() {
        let rate = 22050;
        let bpm = 120.0;
        let x = song(bpm, 16.0, rate);
        // A 4-bar loop cut 0.3 s after a beat: the beats are at 0.2 s, 0.7 s, …
        let len = (16.0 * 60.0 / bpm * rate as f32) as usize;
        let start = (4.3 * rate as f32) as usize;
        let (ph, conf) = beat_phase(&x[start..start + len], rate, 16);
        let beat = len as i64 / 16;
        let off = (ph as i64 - (0.2 * rate as f32) as i64).rem_euclid(beat);
        assert!(off.min(beat - off) < (rate / 50) as i64, "{ph} ({conf})");
        assert!(conf > 0.2, "{conf}");
    }

    fn zero_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn stretch_keeps_pitch_and_pitch_shift_keeps_length() {
        let rate = 22050;
        let x = sine(440.0, 2.0, rate);
        let y = stretch(&x, rate, 1.04);
        assert_eq!(y.len(), (x.len() as f64 * 1.04).round() as usize);
        // Same pitch over the longer length: 4 % more crossings.
        let (a, b) = (zero_crossings(&x[2000..x.len() - 2000]) as f32, zero_crossings(&y[2000..y.len() - 2000]) as f32);
        assert!((b / a - 1.04 * (y.len() - 4000) as f32 / (1.04 * (x.len() - 4000) as f32)).abs() < 0.01, "{a} {b}");
        let z = pitch_shift(&x, rate, 1.0);
        assert_eq!(z.len(), x.len());
        let up = zero_crossings(&z[2000..z.len() - 2000]) as f32 / zero_crossings(&x[2000..x.len() - 2000]) as f32;
        assert!((up - 2f32.powf(1.0 / 12.0)).abs() < 0.01, "{up}");
        // No zipper: a stretched sine stays smooth (no sample jumps beyond its slope).
        let slope = std::f32::consts::TAU * 440.0 / rate as f32 * 0.5;
        assert!(y[2000..y.len() - 2000].windows(2).all(|w| (w[1] - w[0]).abs() < slope * 1.3));
    }

    #[test]
    fn local_period_measures_tempo_finely() {
        let rate = 22050;
        let x = song(123.0, 30.0, rate);
        // A beat at 120 BPM asked for (±12 %: no other multiple of the beat is that close); it plays at 123.
        let (l, _) = local_period(&x, rate, (60.0 / 120.0 * rate as f64) as usize, 0.12).unwrap();
        let bpm = 60.0 * rate as f64 / l as f64;
        assert!((bpm - 123.0).abs() < 0.6, "{bpm}");
        // Refined over two bars, within ±1 %.
        let (l, conf) = local_period(&x, rate, (8.0 * 60.0 / bpm * rate as f64) as usize, 0.01).unwrap();
        let bpm = 8.0 * 60.0 * rate as f64 / l as f64;
        assert!((bpm - 123.0).abs() < 0.1, "{bpm} ({conf})");
    }

    #[test]
    fn loops_rotate_without_a_seam() {
        let rate = 22050;
        let x = sine(441.0, 1.0, rate); // 441 whole cycles
        let r = rotate(&x, 1000);
        assert_eq!(r[0], x[1000]);
        assert_eq!(r[r.len() - 1000], x[0]);
    }

    #[test]
    fn key_fit_like_mix_check() {
        let am = Key::parse("A minor").unwrap();
        let mut ch = [0.0f64; 12];
        for pc in [9, 0, 4] {
            ch[pc] = 1.0; // an A minor triad
        }
        assert_eq!(key_fit(&ch, am), (1.0, 0, 1.0, false));
        // The same triad a semitone up (A# minor): clashes, and -1 fits.
        let up: [f64; 12] = std::array::from_fn(|i| ch[(i + 11) % 12]);
        let (here, best, share, clash) = key_fit(&up, am);
        assert!(clash && best == -1 && share == 1.0 && here < 0.7, "{here} {best} {share}");
    }

    #[test]
    fn cut_to_trims_long_effects_and_fades_the_end() {
        let rate = 1000;
        let x = vec![0.5f32; 1200];
        let y = cut_to(&x, rate, 0.6);
        assert_eq!(y.len(), 600);
        assert_eq!(y[599], 0.0);
        assert!(y[500] > 0.4);
        assert_eq!(cut_to(&x[..300], rate, 0.6).len(), 300, "shorter than asked: untouched");
    }
}
