//! The offline provider: deterministic stand-ins shaped like what real models
//! return (a subject on a slightly noisy flat background, a sprite sheet in
//! a row, a 30 s stereo song with an intro, a sound effect with silence
//! around it), so the whole pipeline runs and is tested without a network
//! or a key. Same prompt and seed, same bytes.

use super::audio::{self, Kind};
use super::img::{Image, over};
use super::provider::{AudioRequest, ImageRequest, ModelOutput, ModelRequest, Output, Provider};

pub struct Mock;

pub fn hash(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in *p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn dims(aspect: &str, resolution: &str) -> (u32, u32) {
    let long = match resolution {
        "0.5K" => 512.0,
        "2K" => 2048.0,
        "4K" => 4096.0,
        _ => 1024.0,
    };
    let (a, b) = aspect.split_once(':').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?))).unwrap_or((1.0, 1.0));
    if a >= b { (long as u32, (long * b / a).round() as u32) } else { ((long * a / b).round() as u32, long as u32) }
}

/// Fills an anti-aliased ellipse.
fn ellipse(im: &mut Image, cx: f32, cy: f32, rx: f32, ry: f32, c: [u8; 3], blocky: f32) {
    let (x0, x1) = ((cx - rx - 2.0).max(0.0) as u32, ((cx + rx + 2.0) as u32).min(im.w));
    let (y0, y1) = ((cy - ry - 2.0).max(0.0) as u32, ((cy + ry + 2.0) as u32).min(im.h));
    for y in y0..y1 {
        for x in x0..x1 {
            let (mut px, mut py) = (x as f32 + 0.5, y as f32 + 0.5);
            if blocky > 1.0 {
                px = (px / blocky).floor() * blocky + blocky / 2.0;
                py = (py / blocky).floor() * blocky + blocky / 2.0;
            }
            let d = (((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2)).sqrt();
            let edge = (1.0 - d) * rx.min(ry);
            let a = if blocky > 1.0 { (edge > 0.0) as u8 as f32 } else { (edge + 0.5).clamp(0.0, 1.0) };
            if a > 0.0 {
                let p = im.get(x, y);
                im.set(x, y, over([c[0], c[1], c[2], (a * 255.0) as u8], p));
            }
        }
    }
}

fn creature(im: &mut Image, cx: f32, cy: f32, r: f32, pal: &[[u8; 3]], phase: f32, blocky: f32) {
    let dark = pal[0];
    let body = pal[1 % pal.len()];
    let belly = pal[2 % pal.len()];
    let leg = (phase * std::f32::consts::TAU).sin() * r * 0.25;
    for (dx, l) in [(-0.4, leg), (0.4, -leg)] {
        ellipse(im, cx + dx * r + l * 0.5, cy + r * 0.85, r * 0.2, r * 0.3, dark, blocky);
    }
    ellipse(im, cx, cy, r * 1.06, r * 0.96, dark, blocky); // outline
    ellipse(im, cx, cy, r, r * 0.9, body, blocky);
    ellipse(im, cx, cy + r * 0.3, r * 0.6, r * 0.45, belly, blocky);
    for dx in [-0.35, 0.35] {
        ellipse(im, cx + dx * r, cy - r * 0.25, r * 0.2, r * 0.24, [250, 250, 250], blocky);
        ellipse(im, cx + dx * r + r * 0.05, cy - r * 0.22, r * 0.09, r * 0.12, dark, blocky);
    }
}

/// A head-and-shoulders bust for a talking-portrait cell: `mouth` 0 (closed) to 3 (wide open),
/// `row` picks the brows (each expression its own), `blink` closes the eyes. The bust is cut off
/// by a straight line at `cut`.
#[allow(clippy::too_many_arguments)]
fn bust(im: &mut Image, cx: f32, cy: f32, r: f32, cut: f32, pal: &[[u8; 3]], mouth: u32, row: u32, blink: bool, blocky: f32) {
    let dark = pal[0];
    let cloth = pal[1 % pal.len()];
    let skin = pal[pal.len() - 1];
    ellipse(im, cx, cy + r * 1.35, r * 1.34, r * 0.79, dark, blocky); // shoulders
    ellipse(im, cx, cy + r * 1.35, r * 1.24, r * 0.69, cloth, blocky);
    ellipse(im, cx, cy, r * 1.12, r * 1.12, dark, blocky); // head
    ellipse(im, cx, cy, r, r, skin, blocky);
    ellipse(im, cx, cy - r * 0.62, r * 0.9, r * 0.42, cloth, blocky); // hair
    for dx in [-0.36, 0.36] {
        let ex = cx + dx * r;
        if blink {
            ellipse(im, ex, cy - r * 0.08, r * 0.17, r * 0.035, dark, blocky);
        } else {
            ellipse(im, ex, cy - r * 0.1, r * 0.16, r * 0.19, [250, 250, 250], blocky);
            ellipse(im, ex, cy - r * 0.08, r * 0.07, r * 0.09, dark, blocky);
        }
        // Brows: higher or lower, nearer or farther apart, by expression.
        let lift = [0.0, 0.08, -0.06, 0.14][row as usize % 4];
        let pinch = [0.0, 0.05, -0.04, 0.02][row as usize % 4] * dx.signum();
        ellipse(im, ex - pinch * r, cy - r * (0.36 + lift), r * 0.17, r * 0.04, dark, blocky);
    }
    let m = mouth.min(3) as f32;
    ellipse(im, cx, cy + r * 0.48, r * (0.22 + 0.05 * m), r * [0.05, 0.1, 0.16, 0.24][mouth.min(3) as usize], dark, blocky);
    // The straight cut under the shoulders.
    let key = im.get(0, 0);
    for y in (cut.max(0.0) as u32)..((cy + r * 2.2) as u32).min(im.h) {
        for x in ((cx - r * 1.4).max(0.0) as u32)..((cx + r * 1.4) as u32).min(im.w) {
            im.set(x, y, key);
        }
    }
}

fn gradient(w: u32, h: u32, top: [u8; 3], bottom: [u8; 3]) -> Image {
    let mut im = Image::new(w, h, [0, 0, 0, 255]);
    for y in 0..h {
        let t = y as f32 / h.max(2) as f32;
        let c = [0, 1, 2].map(|i| (top[i] as f32 * (1.0 - t) + bottom[i] as f32 * t) as u8);
        for x in 0..w {
            im.set(x, y, [c[0], c[1], c[2], 255]);
        }
    }
    im
}

fn hills(im: &mut Image, rng: &mut Rng, c: [u8; 3], base: f32, amp: f32) {
    let (f1, f2, p) = (1.0 + rng.f() * 3.0, 4.0 + rng.f() * 5.0, rng.f() * 6.0);
    for x in 0..im.w {
        let t = x as f32 / im.w as f32;
        let top = im.h as f32 * (base - amp * (0.6 * (t * f1 * 6.283 + p).sin() + 0.4 * (t * f2 * 6.283).sin()));
        for y in (top.max(0.0) as u32)..im.h {
            let q = im.get(x, y);
            im.set(x, y, over([c[0], c[1], c[2], 255], q));
        }
    }
}

fn noisy(im: &mut Image, rng: &mut Rng, amount: i32) {
    for p in &mut im.px {
        if p[3] == 255 {
            let n = (rng.next() % (2 * amount as u64 + 1)) as i32 - amount;
            for c in 0..3 {
                p[c] = (p[c] as i32 + n).clamp(0, 255) as u8;
            }
        }
    }
}

pub fn image(req: &ImageRequest) -> Image {
    let seed = req.seed.unwrap_or(0);
    let mut rng = Rng(hash(&[req.prompt.as_bytes(), &seed.to_le_bytes(), req.hint.kind.as_bytes()]) | 1);
    let (w, h) = dims(&req.aspect, &req.resolution);
    let mut pal: Vec<[u8; 3]> = req.hint.palette.clone();
    if pal.len() < 3 {
        pal = vec![[30, 20, 50], [(rng.next() % 200 + 40) as u8, (rng.next() % 120 + 60) as u8, (rng.next() % 200 + 40) as u8], [240, 200, 120]];
    }
    // Darkest first (outlines).
    pal.sort_by_key(|c| c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32);
    let key = req.hint.key.unwrap_or([0, 255, 0]);
    let flat = |w, h| Image::new(w, h, [key[0], key[1], key[2], 255]);
    let blocky = if req.hint.pixel_art { (w.min(h) as f32 / 48.0).round().max(2.0) } else { 0.0 };
    let s = w.min(h) as f32;
    let mut im = match req.hint.kind.as_str() {
        "frames" => {
            let n = req.hint.frames.max(1);
            let (c, r) = super::style::frame_grid(n);
            let mut im = flat(w, h);
            let (cw, ch) = (w as f32 / c as f32, h as f32 / r as f32);
            for i in 0..n {
                let bob = (i as f32 / n as f32 * std::f32::consts::TAU).sin() * ch * 0.04;
                let (cx, cy) = (cw * ((i % c) as f32 + 0.5), ch * ((i / c) as f32 + 0.5));
                creature(&mut im, cx, cy + bob, cw.min(ch) * 0.28, &pal, i as f32 / n as f32, blocky);
            }
            im
        }
        "portrait" => {
            // 4 columns (mouth closed → wide open) × frames / 4 rows (expressions, then the blink
            // row), each bust a little off in place and size, as a model's would be.
            let rows = (req.hint.frames / 4).max(1);
            let mut im = flat(w, h);
            let (cw, ch) = (w as f32 / 4.0, h as f32 / rows as f32);
            let blocky = if req.hint.pixel_art { (cw.min(ch) / 40.0).round().max(2.0) } else { 0.0 };
            for i in 0..4 * rows {
                let (col, row) = (i % 4, i / 4);
                let (jx, jy, js) = ((rng.f() - 0.5) * 0.06, (rng.f() - 0.5) * 0.05, 1.0 + (rng.f() - 0.5) * 0.06);
                let r = cw.min(ch) * 0.22 * js;
                let (cx, cy) = (cw * (col as f32 + 0.5 + jx), ch * (row as f32 + 0.4 + jy));
                bust(&mut im, cx, cy, r, cy + r * 1.7, &pal, col, row, row + 1 == rows && rows > 1, blocky);
            }
            im
        }
        "background" | "icon" => {
            let top = pal[pal.len() - 1];
            let mut im = gradient(w, h, top, pal[0]);
            for _ in 0..12 {
                let (x, y) = (rng.f() * w as f32, rng.f() * h as f32 * 0.5);
                ellipse(&mut im, x, y, s * 0.006 + 1.0, s * 0.006 + 1.0, [255, 250, 230], 0.0);
            }
            hills(&mut im, &mut rng, pal[1 % pal.len()], 0.75, 0.08);
            hills(&mut im, &mut rng, pal[0], 0.88, 0.05);
            if req.hint.kind == "icon" {
                creature(&mut im, w as f32 * 0.5, h as f32 * 0.55, s * 0.36, &pal, 0.0, blocky);
            }
            im
        }
        "sky" => {
            // Shaped like what models return for an equirect: sky above a horizon at the middle,
            // stars right up to the top row, hills that don't wrap at the edges.
            let mut im = gradient(w, h / 2, [8, 12, 40], [70, 80, 140]);
            im.px.extend(gradient(w, h - h / 2, [30, 26, 40], [12, 10, 16]).px);
            im.h = h;
            for _ in 0..(w / 6) {
                let (x, y) = (rng.f() * w as f32, rng.f() * h as f32 * 0.45);
                ellipse(&mut im, x, y, 1.2, 1.2, [255, 250, 230], 0.0);
            }
            hills(&mut im, &mut rng, pal[1 % pal.len()], 0.5, 0.04);
            im
        }
        "layer" => {
            let mut im = flat(w, h);
            let c = pal[rng.next() as usize % pal.len()];
            let base = 0.6 + rng.f() * 0.2;
            hills(&mut im, &mut rng, c, base, 0.1);
            im
        }
        "tile" => {
            let mut im = Image::new(w, h, [pal[1][0], pal[1][1], pal[1][2], 255]);
            for _ in 0..30 {
                let (x, y) = (rng.f() * w as f32, rng.f() * h as f32);
                let r = s * (0.03 + rng.f() * 0.06);
                ellipse(&mut im, x, y, r, r * 0.8, pal[rng.next() as usize % pal.len()], blocky);
            }
            im
        }
        "ui" => {
            let mut im = flat(w, h);
            ellipse(&mut im, w as f32 / 2.0, h as f32 / 2.0, w as f32 * 0.4, h as f32 * 0.25, pal[0], blocky);
            ellipse(&mut im, w as f32 / 2.0, h as f32 / 2.0, w as f32 * 0.38, h as f32 * 0.22, pal[2 % pal.len()], blocky);
            im
        }
        _ => {
            let mut im = flat(w, h);
            // Seeds vary the pose a little, so variants differ.
            let (dx, r, ph) = ((rng.f() - 0.5) * s * 0.08, s * (0.24 + rng.f() * 0.08), rng.f());
            creature(&mut im, w as f32 * 0.5 + dx, h as f32 * 0.5, r, &pal, ph, blocky);
            im
        }
    };
    noisy(&mut im, &mut rng, 3);
    im
}

/// A 30 s, 44.1 kHz stereo song with a fade-in, at a tempo set by the prompt
/// and seed: a kick on every beat, off-beat hats, a chord change every bar.
#[cfg(test)]
pub fn song(bpm: f32, secs: f32, rate: u32, seed: u64) -> Vec<f32> {
    song_in_sections(bpm, secs, rate, seed, &[])
}

/// The same with sections starting at `starts` (seconds): each moves the
/// progression on and adds a melody, at the same tempo and in the same key.
pub fn song_in_sections(bpm: f32, secs: f32, rate: u32, seed: u64, starts: &[f32]) -> Vec<f32> {
    let n = (secs * rate as f32) as usize;
    let beat = 60.0 / bpm;
    let roots = [220.0, 196.0, 174.61, 196.0, 246.94, 220.0];
    // A minor's notes, for the arpeggios.
    let arp = [[440.0, 523.25, 659.25, 523.25], [392.0, 493.88, 587.33, 783.99], [349.23, 440.0, 523.25, 698.46], [659.25, 587.33, 523.25, 493.88]];
    let off = (seed % 3) as usize;
    let mut rng = Rng(seed | 1);
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let section = starts.iter().rposition(|&s| t >= s);
            let bt = t % beat;
            let kick = (-bt * 30.0).exp() * (std::f32::consts::TAU * (50.0 + 80.0 * (-bt * 40.0).exp()) * bt).sin();
            let ht = (t + beat / 2.0) % beat;
            let noise = rng.f() * 2.0 - 1.0;
            let hat = (-ht * 80.0).exp() * noise * 0.3;
            let bar = ((t / (beat * 4.0)) as usize + off + section.unwrap_or(0)) % 4;
            let r = roots[bar];
            let pad: f32 = [1.0, 1.26, 1.5].iter().map(|k| (std::f32::consts::TAU * r * k * t).sin()).sum::<f32>() * 0.08;
            let lead = match section {
                Some(s) => {
                    let e = t / (beat / 2.0);
                    let et = e.fract() * beat / 2.0;
                    // A melody that doesn't repeat (as a model's doesn't, quite): each eighth's
                    // note from one of the arpeggios, picked by a hash of the eighth.
                    let k = (e as u64 / 2).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 62;
                    let f = arp[(k as usize + s) % 4][(e as usize + s) % 4];
                    (std::f32::consts::TAU * f * t).sin() * (-et * 5.0).exp() * 0.25
                }
                None => 0.0,
            };
            let fade = (t / 1.5).min(1.0) * ((secs - t) / 2.0).clamp(0.0, 1.0);
            (kick * 0.6 + hat + pad + lead) * fade
        })
        .collect()
}

/// "[0:20 - 0:40]" section starts in a prompt, in seconds.
fn section_starts(prompt: &str) -> Vec<f32> {
    prompt
        .split('[')
        .skip(1)
        .filter_map(|p| {
            let (m, rest) = p.split_once(':')?;
            let s: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            Some(m.trim().parse::<f32>().ok()? * 60.0 + s.parse::<f32>().ok()?)
        })
        .collect()
}

impl Provider for Mock {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn image(&self, req: &ImageRequest) -> Result<Output, String> {
        let im = image(req);
        Ok(Output { bytes: im.encode_png(), mime: "image/png".into(), text: Some("mock image".into()), cost_usd: Some(0.0) })
    }

    fn model(&self, req: &ModelRequest) -> Result<ModelOutput, String> {
        Ok(super::mock3d::model(req))
    }

    /// The stand-in's rigged twin (same prompt and seed, so clips added later share its bones).
    fn rig(&self, req: &super::provider::RigRequest) -> Result<ModelOutput, String> {
        let m = super::mock3d::model(&ModelRequest { prompt: req.prompt.clone(), seed: req.seed, rig: true, animations: req.animations.clone(), ..ModelRequest::default() });
        let mut out = ModelOutput { clips: m.clips, rigged: true, cost_usd: Some(0.0), ..ModelOutput::default() };
        match &req.rig_task {
            Some(id) => out.rig = Some((id.clone(), req.rig_type.clone().unwrap_or_else(|| "mock".into()))),
            None => {
                let id = format!("mock-rig-{:016x}", hash(&[req.prompt.as_bytes(), &req.seed.unwrap_or(0).to_le_bytes()]));
                let t = req.rig_type.clone().unwrap_or_else(|| "mock".into());
                out.tasks.push((format!("rig ({t})"), id.clone()));
                out.rig = Some((id, t));
                out.glb = m.glb;
            }
        }
        for a in &req.animations {
            out.tasks.push((format!("animate {a}"), format!("mock-{a}")));
        }
        Ok(out)
    }

    fn audio(&self, req: &AudioRequest) -> Result<Output, String> {
        // "mock-refuse" in a prompt: the refusal a real provider's content filter gives, to try what follows one.
        if req.prompt.contains("mock-refuse") {
            return Err("mock: no audio in the response (status completed). A safety filter may have blocked the prompt: describe the subject differently.".into());
        }
        let seed = hash(&[req.prompt.as_bytes(), &req.seed.unwrap_or(0).to_le_bytes()]);
        match req.kind {
            Kind::Music => {
                // It plays at the tempo asked for ("128 BPM"); otherwise one from the seed.
                let starts = section_starts(&req.prompt);
                let asked = req.prompt.split(" BPM").next().and_then(|p| p.rsplit(' ').next()).and_then(|b| b.parse::<f32>().ok());
                let bpm = asked.unwrap_or(90.0 + (seed % 50) as f32);
                let rate = 44100;
                let secs = req.seconds.unwrap_or(30.0).clamp(4.0, 240.0);
                let l = song_in_sections(bpm, secs, rate, seed, &starts);
                // A little stereo width: the right channel is slightly delayed.
                let mut wav = Vec::new();
                let d = 20;
                let mono = audio::wav_bytes(&l, rate);
                // Interleave L/R into a stereo WAV.
                let n = l.len();
                wav.extend_from_slice(&mono[..22]);
                wav.extend_from_slice(&2u16.to_le_bytes());
                wav.extend_from_slice(&rate.to_le_bytes());
                wav.extend_from_slice(&(rate * 4).to_le_bytes());
                wav.extend_from_slice(&4u16.to_le_bytes());
                wav.extend_from_slice(&16u16.to_le_bytes());
                wav.extend_from_slice(b"data");
                wav.extend_from_slice(&((n * 4) as u32).to_le_bytes());
                for i in 0..n {
                    let r = if i >= d { l[i - d] } else { 0.0 };
                    for v in [l[i], r] {
                        wav.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
                    }
                }
                let riff = (wav.len() - 8) as u32;
                wav[4..8].copy_from_slice(&riff.to_le_bytes());
                Ok(Output { bytes: wav, mime: "audio/wav".into(), text: Some(format!("mock song at {bpm} BPM")), cost_usd: Some(0.0) })
            }
            Kind::Sfx => {
                let rate = 24000u32;
                let secs = req.seconds.unwrap_or(0.4).clamp(0.05, 30.0);
                let mut rng = Rng(seed | 1);
                let (f0, f1) = (200.0 + (seed % 800) as f32, 100.0 + (seed / 7 % 1500) as f32);
                let mut x = vec![0.0f32; (0.06 * rate as f32) as usize];
                let n = (secs * rate as f32) as usize;
                let mut ph = 0.0f32;
                for i in 0..n {
                    let t = i as f32 / n as f32;
                    ph += std::f32::consts::TAU * (f0 + (f1 - f0) * t) / rate as f32;
                    let env = (1.0 - t).powi(2) * (i as f32 / 60.0).min(1.0);
                    x.push((ph.sin() * 0.7 + (rng.f() * 2.0 - 1.0) * 0.1) * env * 0.4);
                }
                x.extend(vec![0.0; (0.3 * rate as f32) as usize]);
                let bytes = x.iter().flat_map(|v| ((v * 32767.0) as i16).to_le_bytes()).collect();
                Ok(Output { bytes, mime: format!("audio/pcm;rate={rate}"), text: Some("mock effect".into()), cost_usd: Some(0.0) })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::MockHint;
    use super::*;

    fn req(kind: &str) -> ImageRequest {
        ImageRequest {
            model: "mock-image".into(),
            prompt: "a moth".into(),
            refs: vec![],
            aspect: "16:9".into(),
            resolution: "0.5K".into(),
            transparent: true,
            seed: Some(7),
            quality: 1,
            hint: MockHint { kind: kind.into(), frames: 4, key: Some([255, 0, 255]), palette: vec![], pixel_art: false },
        }
    }

    #[test]
    fn deterministic_and_shaped() {
        let a = Mock.image(&req("sprite")).unwrap().bytes;
        let b = Mock.image(&req("sprite")).unwrap().bytes;
        assert_eq!(a, b);
        let im = Image::decode(&a).unwrap();
        assert_eq!((im.w, im.h), (512, 288));
        let c = im.get(0, 0);
        assert!(c[0] > 240 && c[1] < 10 && c[2] > 240, "keyed background {c:?}");
        let mut other = req("sprite");
        other.seed = Some(8);
        assert_ne!(Mock.image(&other).unwrap().bytes, a);
    }

    #[test]
    fn audio_decodes() {
        let r = AudioRequest {
            model: "mock-music".into(),
            prompt: "x".into(),
            kind: Kind::Music,
            seconds: Some(6.0),
            looping: true,
            seed: Some(1),
            negative: None,
        };
        let out = Mock.audio(&r).unwrap();
        let (ch, rate) = audio::decode(&out.bytes, &out.mime).unwrap();
        assert_eq!((ch.len(), rate), (2, 44100));
        assert_eq!(ch[0].len(), 6 * 44100);
        let s = Mock.audio(&AudioRequest { kind: Kind::Sfx, seconds: Some(0.3), ..r }).unwrap();
        let (ch, rate) = audio::decode(&s.bytes, &s.mime).unwrap();
        assert_eq!(rate, 24000);
        assert!(ch[0].len() > 7000);
    }
}
