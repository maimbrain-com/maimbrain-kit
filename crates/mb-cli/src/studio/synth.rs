//! Studio's sounds as data (docs/STUDIO-API.md "Sound" and "Songs"): a
//! `.sfx.toml` is an sfxr-style parameter block, a `.song.toml` a small
//! four-channel tracker (two pulse voices, a triangle and noise). `mb build`
//! renders each to a mono Ogg next to it in the bundle, which the engine
//! plays like any other sound.
//!
//! SPEC §5.6 names `mb_synth(params) -> sound` for this, but no host has it
//! yet; rendering here means a Studio game needs no new host import. When a
//! host does, the same parameters can go to it instead. The output only has
//! to sound right: audio never affects a game's state or its replays.

use toml::{Table, Value};

/// Samples a second.
pub const RATE: u32 = 22_050;
/// The longest sound and the longest song (seconds).
const MAX_SFX: f64 = 4.0;
const MAX_SONG: f64 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Wave {
    Square,
    Triangle,
    Saw,
    Sine,
    Noise,
}

/// One voice's sound: a wave, its envelope and colour.
#[derive(Clone, Debug)]
struct Voice {
    wave: Wave,
    duty: f64,
    attack: f64,
    decay: f64,
    volume: f64,
    vibrato: (f64, f64),
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
}

fn wave(t: &Table, default: Wave, path: &str) -> Result<Wave, String> {
    Ok(match t.get("wave").and_then(Value::as_str) {
        None => default,
        Some("square" | "pulse") => Wave::Square,
        Some("triangle") => Wave::Triangle,
        Some("saw") => Wave::Saw,
        Some("sine") => Wave::Sine,
        Some("noise") => Wave::Noise,
        Some(w) => return Err(format!("{path}: wave = {w:?} (square, pulse, triangle, saw, sine or noise)")),
    })
}

fn voice(t: &Table, d: &Voice, path: &str) -> Result<Voice, String> {
    let vib = match t.get("vibrato").and_then(Value::as_array) {
        Some(a) => (num(a.first()).unwrap_or(0.0), num(a.get(1)).unwrap_or(6.0)),
        None => d.vibrato,
    };
    Ok(Voice {
        wave: wave(t, d.wave, path)?,
        duty: num(t.get("duty")).unwrap_or(d.duty).clamp(0.05, 0.95),
        attack: num(t.get("attack")).unwrap_or(d.attack).clamp(0.0, 2.0),
        decay: num(t.get("decay")).unwrap_or(d.decay).clamp(0.0, 4.0),
        volume: num(t.get("volume")).unwrap_or(d.volume).clamp(0.0, 1.0),
        vibrato: vib,
    })
}

/// A deterministic noise source (xorshift), held for `1 / freq` like a chip's.
struct Noise {
    s: u32,
    held: f64,
    phase: f64,
}

impl Noise {
    fn new(seed: u32) -> Noise {
        Noise { s: seed | 1, held: 0.0, phase: 1.0 }
    }
    fn next(&mut self, step: f64) -> f64 {
        self.phase += step;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.s ^= self.s << 13;
            self.s ^= self.s >> 17;
            self.s ^= self.s << 5;
            self.held = (self.s as f64 / u32::MAX as f64) * 2.0 - 1.0;
        }
        self.held
    }
}

fn osc(w: Wave, phase: f64, duty: f64) -> f64 {
    let p = phase - phase.floor();
    match w {
        Wave::Square => {
            if p < duty {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        Wave::Saw => 2.0 * p - 1.0,
        Wave::Sine => (p * std::f64::consts::TAU).sin(),
        Wave::Noise => 0.0,
    }
}

/// One note into `out` from sample `at`: `freq` sliding to `freq_end`,
/// held for `len` seconds, then its decay.
#[allow(clippy::too_many_arguments)]
fn note(out: &mut Vec<f32>, at: usize, v: &Voice, freq: f64, freq_end: f64, len: f64, arp: &[f64], arp_speed: f64, gain: f64, seed: u32) {
    let rate = RATE as f64;
    let total = ((len + v.decay) * rate) as usize;
    if out.len() < at + total {
        out.resize(at + total, 0.0);
    }
    let mut phase = 0.0;
    let mut noise = Noise::new(seed);
    for i in 0..total {
        let t = i as f64 / rate;
        let k = (t / len.max(1e-6)).min(1.0);
        let mut f = freq * (freq_end / freq).powf(k);
        if !arp.is_empty() {
            let step = ((t / arp_speed.max(0.01)) as usize) % arp.len();
            f *= 2f64.powf(arp[step] / 12.0);
        }
        if v.vibrato.0 > 0.0 {
            f *= 2f64.powf(v.vibrato.0 / 12.0 * (t * v.vibrato.1 * std::f64::consts::TAU).sin());
        }
        let s = if v.wave == Wave::Noise {
            noise.next(f / rate * 8.0)
        } else {
            phase += f / rate;
            osc(v.wave, phase, v.duty)
        };
        let env = if t < v.attack {
            t / v.attack.max(1e-6)
        } else if t < len {
            1.0
        } else {
            (1.0 - (t - len) / v.decay.max(1e-6)).max(0.0).powi(2)
        };
        out[at + i] += (s * env * v.volume * gain) as f32;
    }
}

/// A `.sfx.toml` as samples at [`RATE`].
pub fn sfx(text: &str, path: &str) -> Result<Vec<f32>, String> {
    let t: Table = text.parse().map_err(|e: toml::de::Error| format!("{path}: {}", e.message()))?;
    let d = Voice { wave: Wave::Square, duty: 0.5, attack: 0.005, decay: 0.08, volume: 0.8, vibrato: (0.0, 6.0) };
    let v = voice(&t, &d, path)?;
    let freq = num(t.get("freq")).unwrap_or(440.0).clamp(20.0, 12_000.0);
    let freq_end = num(t.get("freq_end")).unwrap_or(freq).clamp(20.0, 12_000.0);
    let len = num(t.get("duration")).unwrap_or(0.15).clamp(0.01, MAX_SFX);
    let arp: Vec<f64> = t.get("arp").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| num(Some(x))).collect()).unwrap_or_default();
    let arp_speed = num(t.get("arp_speed")).unwrap_or(0.06);
    let mut out = Vec::new();
    note(&mut out, 0, &v, freq, freq_end, len, &arp, arp_speed, 1.0, 0x5eed);
    Ok(out)
}

/// `C5`, `F#3`, `Bb2` → Hz.
fn pitch(s: &str) -> Option<f64> {
    let mut c = s.chars();
    let semi = match c.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = c.collect();
    let (shift, oct) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    let oct: i32 = oct.parse().ok().filter(|o| (0..=8).contains(o))?;
    let midi = (oct + 1) * 12 + semi + shift;
    Some(440.0 * 2f64.powf((midi as f64 - 69.0) / 12.0))
}

/// One step of a channel's pattern.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Rest,
    Hold,
    /// Hz (0 for a noise hit at the channel's own pitch) and loudness.
    Note(f64, f64),
}

fn steps(s: &str, path: &str, what: &str) -> Result<Vec<Step>, String> {
    s.split_whitespace()
        .map(|w| {
            Ok(match w {
                "." => Step::Rest,
                "-" => Step::Hold,
                "x" => Step::Note(0.0, 0.8),
                "X" => Step::Note(0.0, 1.0),
                "?" => Step::Note(0.0, 0.45),
                _ => {
                    let (w, accent) = match w.strip_suffix('!') {
                        Some(n) => (n, 1.0),
                        None => match w.strip_suffix('?') {
                            Some(n) => (n, 0.45),
                            None => (w, 0.8),
                        },
                    };
                    Step::Note(pitch(w).ok_or_else(|| format!("{path}: {what}: {w:?} isn't a note (C4, F#3, Bb2, . rest, - hold, x hit)"))?, accent)
                }
            })
        })
        .collect()
}

const CHANNELS: [&str; 4] = ["pulse1", "pulse2", "triangle", "noise"];

/// A `.song.toml` as samples at [`RATE`], one loop of it (the end's tail
/// folded onto the start, so it loops without a gap).
pub fn song(text: &str, path: &str) -> Result<Vec<f32>, String> {
    let t: Table = text.parse().map_err(|e: toml::de::Error| format!("{path}: {}", e.message()))?;
    let bpm = num(t.get("bpm")).unwrap_or(120.0).clamp(30.0, 300.0);
    let per_beat = num(t.get("steps")).unwrap_or(4.0).clamp(1.0, 8.0);
    let step = 60.0 / bpm / per_beat;
    let defaults = [
        Voice { wave: Wave::Square, duty: 0.5, attack: 0.004, decay: 0.08, volume: 0.28, vibrato: (0.0, 6.0) },
        Voice { wave: Wave::Square, duty: 0.25, attack: 0.004, decay: 0.08, volume: 0.22, vibrato: (0.0, 6.0) },
        Voice { wave: Wave::Triangle, duty: 0.5, attack: 0.004, decay: 0.05, volume: 0.45, vibrato: (0.0, 6.0) },
        Voice { wave: Wave::Noise, duty: 0.5, attack: 0.001, decay: 0.06, volume: 0.25, vibrato: (0.0, 6.0) },
    ];
    let vt = t.get("voices").and_then(Value::as_table).cloned().unwrap_or_default();
    let mut voices = Vec::new();
    for (i, name) in CHANNELS.iter().enumerate() {
        let v = match vt.get(*name).and_then(Value::as_table) {
            Some(x) => voice(x, &defaults[i], path)?,
            None => defaults[i].clone(),
        };
        voices.push(v);
    }
    let patterns = t.get("patterns").and_then(Value::as_table).ok_or_else(|| format!("{path}: no [patterns.<name>] (each a line of notes per channel)"))?;
    let order: Vec<String> = match t.get("order").and_then(Value::as_array) {
        Some(o) => o.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        None => patterns.keys().cloned().collect(),
    };
    if order.is_empty() {
        return Err(format!("{path}: the song has no patterns to play"));
    }
    let mut out: Vec<f32> = Vec::new();
    let mut at_step = 0usize;
    for (pi, name) in order.iter().enumerate() {
        let p = patterns.get(name).and_then(Value::as_table).ok_or_else(|| format!("{path}: order names \"{name}\", but there's no [patterns.{name}]"))?;
        for k in p.keys() {
            if !CHANNELS.contains(&k.as_str()) {
                return Err(format!("{path}: patterns.{name}.{k}: the channels are {}", CHANNELS.join(", ")));
            }
        }
        let lines: Vec<Vec<Step>> = CHANNELS.iter().map(|c| p.get(*c).and_then(Value::as_str).map_or(Ok(Vec::new()), |s| steps(s, path, &format!("patterns.{name}.{c}")))).collect::<Result<_, _>>()?;
        let len = lines.iter().map(Vec::len).max().unwrap_or(0);
        for (ci, line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let v = &voices[ci];
            let mut i = 0;
            while i < len {
                if let Step::Note(f, accent) = line[i % line.len()] {
                    let mut held = 1;
                    while i + held < len && line[(i + held) % line.len()] == Step::Hold {
                        held += 1;
                    }
                    let f = if f == 0.0 { if ci == 3 { 3000.0 } else { 440.0 } } else { f };
                    let at = ((at_step + i) as f64 * step * RATE as f64) as usize;
                    let seed = (pi * 1000 + i * 4 + ci) as u32 + 1;
                    note(&mut out, at, v, f, f, held as f64 * step * if ci == 3 { 0.3 } else { 0.9 }, &[], 0.0, accent, seed);
                    i += held;
                } else {
                    i += 1;
                }
            }
        }
        at_step += len;
        if at_step as f64 * step > MAX_SONG {
            return Err(format!("{path}: the song is longer than {MAX_SONG} s"));
        }
    }
    let n = (at_step as f64 * step * RATE as f64) as usize;
    if out.len() > n {
        let tail: Vec<f32> = out.split_off(n);
        for (i, s) in tail.into_iter().enumerate() {
            out[i % n] += s;
        }
    }
    out.resize(n, 0.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes() {
        assert!((pitch("A4").unwrap() - 440.0).abs() < 1e-9);
        assert!((pitch("C4").unwrap() - 261.6256).abs() < 1e-3);
        assert!((pitch("F#3").unwrap() - pitch("Gb3").unwrap()).abs() < 1e-9);
        assert_eq!(pitch("H2"), None);
    }

    #[test]
    fn a_jump_sound_slides_up_and_ends() {
        let x = sfx("wave = \"square\"\nfreq = 330\nfreq_end = 880\nduration = 0.18\n", "jump.sfx.toml").unwrap();
        assert_eq!(x.len(), ((0.18 + 0.08) * RATE as f64) as usize);
        assert!(x.iter().any(|s| s.abs() > 0.5));
        assert!(x.last().unwrap().abs() < 0.01);
        assert!(sfx("wave = \"kazoo\"", "a.sfx.toml").unwrap_err().contains("wave = \"kazoo\""));
        // The same parameters give the same samples.
        assert_eq!(x, sfx("wave = \"square\"\nfreq = 330\nfreq_end = 880\nduration = 0.18\n", "jump.sfx.toml").unwrap());
    }

    #[test]
    fn a_song_is_one_loop_of_its_order() {
        let s = "bpm = 120\norder = [\"a\", \"b\"]\n[patterns.a]\npulse1 = \"C5 - E5 .\"\nnoise = \"x . X .\"\n[patterns.b]\ntriangle = \"C3 . . G2 - - - .\"\n";
        let x = song(s, "theme.song.toml").unwrap();
        // 4 + 8 sixteenths at 120 bpm: 1.5 s.
        assert_eq!(x.len(), (1.5 * RATE as f64) as usize);
        assert!(x.iter().any(|v| v.abs() > 0.1));
        let e = song("[patterns.a]\npulse1 = \"C5 Q5\"\n", "t.song.toml").unwrap_err();
        assert!(e.contains("\"Q5\" isn't a note"), "{e}");
        assert!(song("order = [\"z\"]\n[patterns.a]\npulse1 = \"C5\"\n", "t.song.toml").unwrap_err().contains("no [patterns.z]"));
    }
}
