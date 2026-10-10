//! A small RGBA image type and the post-processing `mb art` applies to what a
//! model returns: decoding (PNG, JPEG), resampling, background keying,
//! pixel-art reduction, palette locking and quantizing, seamless wrapping, and
//! a PNG encoder that picks the smallest of indexed, RGB and RGBA.
//!
//! Pixels are straight (non-premultiplied) sRGB RGBA8. Resampling works on
//! premultiplied values so transparent pixels don't bleed their color.

use std::io::Cursor;

pub type Rgba = [u8; 4];

#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<Rgba>,
}

impl Image {
    pub fn new(w: u32, h: u32, fill: Rgba) -> Image {
        Image { w, h, px: vec![fill; (w * h) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> Rgba {
        self.px[(y * self.w + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, c: Rgba) {
        let i = (y * self.w + x) as usize;
        self.px[i] = c;
    }

    /// Decodes a PNG or JPEG (by magic bytes).
    pub fn decode(bytes: &[u8]) -> Result<Image, String> {
        if bytes.starts_with(b"\x89PNG") {
            decode_png(bytes)
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            decode_jpeg(bytes)
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Err("the provider returned WebP, which mb can't decode; ask for PNG".into())
        } else {
            Err("not a PNG or JPEG image".into())
        }
    }

    pub fn is_opaque(&self) -> bool {
        self.px.iter().all(|p| p[3] == 255)
    }

    /// Whether the image has real transparency (some clear pixels on its border),
    /// as a provider's native transparent output does.
    pub fn has_transparent_border(&self) -> bool {
        let mut clear = 0;
        let mut n = 0;
        for (x, y) in border(self.w, self.h) {
            n += 1;
            clear += (self.get(x, y)[3] < 16) as u32;
        }
        n > 0 && clear * 2 > n
    }

    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Image {
        let mut out = Image::new(w, h, [0; 4]);
        for j in 0..h {
            for i in 0..w {
                if x + i < self.w && y + j < self.h {
                    out.set(i, j, self.get(x + i, y + j));
                }
            }
        }
        out
    }

    /// Copies `src` into this image at (x, y), replacing pixels (no blending).
    pub fn blit(&mut self, src: &Image, x: u32, y: u32) {
        for j in 0..src.h {
            for i in 0..src.w {
                if x + i < self.w && y + j < self.h {
                    self.set(x + i, y + j, src.get(i, j));
                }
            }
        }
    }

    /// The bounding box (x, y, w, h) of pixels with alpha above `min_alpha`.
    pub fn content_bbox(&self, min_alpha: u8) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.get(x, y)[3] > min_alpha {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    pub fn trimmed(&self) -> Image {
        match self.content_bbox(0) {
            Some((x, y, w, h)) => self.crop(x, y, w, h),
            None => Image::new(1, 1, [0; 4]),
        }
    }

    /// Composites onto an opaque color.
    pub fn flatten(&self, bg: [u8; 3]) -> Image {
        let mut out = self.clone();
        for p in &mut out.px {
            *p = over(*p, [bg[0], bg[1], bg[2], 255]);
        }
        out
    }

    /// Nearest-neighbour scale by an integer factor (pixel art).
    pub fn scale_nearest(&self, k: u32) -> Image {
        self.resize_nearest(self.w * k, self.h * k)
    }

    pub fn resize_nearest(&self, w: u32, h: u32) -> Image {
        let mut out = Image::new(w, h, [0; 4]);
        for y in 0..h {
            for x in 0..w {
                let sx = ((x as u64 * self.w as u64) / w as u64) as u32;
                let sy = ((y as u64 * self.h as u64) / h as u64) as u32;
                out.set(x, y, self.get(sx, sy));
            }
        }
        out
    }

    /// High-quality resize (Lanczos-3, widened when shrinking so it
    /// averages every source pixel), in premultiplied alpha.
    pub fn resize(&self, w: u32, h: u32) -> Image {
        if w == self.w && h == self.h {
            return self.clone();
        }
        let src: Vec<[f32; 4]> = self.px.iter().map(|p| premul(*p)).collect();
        let tmp = resample_axis(&src, self.w as usize, self.h as usize, w as usize, true);
        let out = resample_axis(&tmp, w as usize, self.h as usize, h as usize, false);
        Image { w, h, px: out.into_iter().map(unpremul).collect() }
    }

    /// Crops to the aspect ratio of w×h around the centre (or `focus`, 0..1),
    /// then resizes to exactly w×h.
    pub fn cover(&self, w: u32, h: u32) -> Image {
        let (sw, sh) = (self.w as f64, self.h as f64);
        let target = w as f64 / h as f64;
        let (cw, ch) = if sw / sh > target { ((sh * target).round(), sh) } else { (sw, (sw / target).round()) };
        let (cw, ch) = (cw.max(1.0) as u32, ch.max(1.0) as u32);
        let x = (self.w - cw) / 2;
        let y = (self.h - ch) / 2;
        self.crop(x, y, cw, ch).resize(w, h)
    }

    /// Scales the content (already trimmed) to fit inside w×h minus `margin`
    /// on every side, keeping its aspect, placed per `anchor`.
    pub fn contain(&self, w: u32, h: u32, margin: u32, anchor: Anchor) -> Image {
        let iw = w.saturating_sub(2 * margin).max(1) as f64;
        let ih = h.saturating_sub(2 * margin).max(1) as f64;
        let k = (iw / self.w as f64).min(ih / self.h as f64);
        let nw = ((self.w as f64 * k).round() as u32).clamp(1, w);
        let nh = ((self.h as f64 * k).round() as u32).clamp(1, h);
        let scaled = self.resize(nw, nh);
        let mut out = Image::new(w, h, [0; 4]);
        let x = (w - nw) / 2;
        let y = match anchor {
            Anchor::Center => (h - nh) / 2,
            Anchor::Bottom => h - margin.min(h - nh) - nh,
        };
        out.blit(&scaled, x, y);
        out
    }

    /// Distinct RGBA colors, up to `limit + 1` (so callers can tell "more than limit").
    pub fn distinct_colors(&self, limit: usize) -> Vec<Rgba> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for p in &self.px {
            let p = if p[3] == 0 { [0; 4] } else { *p };
            if seen.insert(p) {
                out.push(p);
                if out.len() > limit {
                    break;
                }
            }
        }
        out
    }

    /// The image over a checkerboard and over dark and light grounds, side by
    /// side and enlarged so small sprites are easy to inspect.
    pub fn preview(&self) -> Image {
        let k = (256 / self.w.max(self.h)).clamp(1, 8);
        let big = if k > 1 { self.scale_nearest(k) } else { self.clone() };
        let (w, h) = (big.w, big.h);
        let pad = 8;
        let mut out = Image::new(3 * w + 4 * pad, h + 2 * pad, [40, 40, 48, 255]);
        let grounds: [Box<dyn Fn(u32, u32) -> Rgba>; 3] = [
            Box::new(|x, y| if ((x / 8) + (y / 8)) % 2 == 0 { [200, 200, 200, 255] } else { [150, 150, 150, 255] }),
            Box::new(|_, _| [18, 18, 26, 255]),
            Box::new(|_, _| [236, 232, 220, 255]),
        ];
        for (n, g) in grounds.iter().enumerate() {
            let ox = pad + n as u32 * (w + pad);
            for y in 0..h {
                for x in 0..w {
                    out.set(ox + x, pad + y, over(big.get(x, y), g(x, y)));
                }
            }
        }
        out
    }

    pub fn encode_png(&self) -> Vec<u8> {
        encode_png(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    #[default]
    Center,
    /// Feet on the bottom edge (characters that stand on the ground).
    Bottom,
}

fn border(w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    let top = (0..w).map(|x| (x, 0));
    let bottom = (0..w).map(move |x| (x, h - 1));
    let left = (1..h.saturating_sub(1)).map(|y| (0, y));
    let right = (1..h.saturating_sub(1)).map(move |y| (w - 1, y));
    top.chain(bottom).chain(left).chain(right)
}

pub fn over(s: Rgba, d: Rgba) -> Rgba {
    let sa = s[3] as f32 / 255.0;
    let da = d[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        return [0; 4];
    }
    let mut o = [0u8; 4];
    for c in 0..3 {
        let v = (s[c] as f32 * sa + d[c] as f32 * da * (1.0 - sa)) / oa;
        o[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    o[3] = (oa * 255.0).round() as u8;
    o
}

fn premul(p: Rgba) -> [f32; 4] {
    let a = p[3] as f32 / 255.0;
    [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, p[3] as f32]
}

fn unpremul(p: [f32; 4]) -> Rgba {
    let a = p[3].clamp(0.0, 255.0);
    if a < 0.5 {
        return [0; 4];
    }
    let k = 255.0 / a;
    [(p[0] * k).round().clamp(0.0, 255.0) as u8, (p[1] * k).round().clamp(0.0, 255.0) as u8, (p[2] * k).round().clamp(0.0, 255.0) as u8, a.round() as u8]
}

fn lanczos3(x: f64) -> f64 {
    let x = x.abs();
    if x < 1e-9 {
        1.0
    } else if x < 3.0 {
        let px = std::f64::consts::PI * x;
        3.0 * px.sin() * (px / 3.0).sin() / (px * px)
    } else {
        0.0
    }
}

/// Resamples along x (`horizontal`) or y, from `n` to `m` samples.
fn resample_axis(src: &[[f32; 4]], w: usize, h: usize, m: usize, horizontal: bool) -> Vec<[f32; 4]> {
    let n = if horizontal { w } else { h };
    let scale = m as f64 / n as f64;
    let support = if scale < 1.0 { 3.0 / scale } else { 3.0 };
    let fscale = if scale < 1.0 { scale } else { 1.0 };
    // Weights for every output sample.
    let mut taps: Vec<(usize, Vec<f32>)> = Vec::with_capacity(m);
    for o in 0..m {
        let center = (o as f64 + 0.5) / scale - 0.5;
        let lo = ((center - support).floor().max(0.0)) as usize;
        let hi = ((center + support).ceil() as usize).min(n - 1);
        let mut ws: Vec<f64> = (lo..=hi).map(|i| lanczos3((i as f64 - center) * fscale)).collect();
        let sum: f64 = ws.iter().sum();
        if sum.abs() > 1e-12 {
            ws.iter_mut().for_each(|v| *v /= sum);
        }
        taps.push((lo, ws.into_iter().map(|v| v as f32).collect()));
    }
    let (ow, oh) = if horizontal { (m, h) } else { (w, m) };
    let mut out = vec![[0f32; 4]; ow * oh];
    for line in 0..(if horizontal { h } else { w }) {
        for (o, (lo, ws)) in taps.iter().enumerate() {
            let mut acc = [0f32; 4];
            for (k, wt) in ws.iter().enumerate() {
                let i = lo + k;
                let s = if horizontal { src[line * w + i] } else { src[i * w + line] };
                for c in 0..4 {
                    acc[c] += s[c] * wt;
                }
            }
            for v in &mut acc {
                *v = v.max(0.0);
            }
            acc[3] = acc[3].min(255.0);
            for c in 0..3 {
                acc[c] = acc[c].min(acc[3]); // premultiplied color can't exceed alpha
            }
            let idx = if horizontal { line * ow + o } else { o * ow + line };
            out[idx] = acc;
        }
    }
    out
}

// --- color --------------------------------------------------------------------

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Oklab (L, a, b) of an sRGB color.
pub fn oklab(c: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

pub fn oklab_dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub fn parse_hex(s: &str) -> Result<[u8; 3], String> {
    let t = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(t, 16).map_err(|_| format!("{s:?} is not a hex color like #1b1033"))?;
    match t.len() {
        6 => Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8]),
        3 => {
            let e = |n: u32| ((n & 0xf) * 17) as u8;
            Ok([e(v >> 8), e(v >> 4), e(v)])
        }
        _ => Err(format!("{s:?} is not a hex color like #1b1033")),
    }
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// --- background removal -------------------------------------------------------

/// How a keyed background came out, for the log.
#[derive(Debug)]
pub struct KeyReport {
    pub background: [u8; 3],
    /// Every color keyed out (the first is `background`): a model sometimes
    /// paints the flat background in two tones.
    pub backgrounds: Vec<[u8; 3]>,
    /// Share of the border that is background (low: the subject touches the edges).
    pub border_bg: f32,
    /// How much the background varies (90th percentile Oklab distance of its
    /// border pixels from its color): high means a gradient or texture.
    pub noise: f32,
    pub cleared: f32,
}

/// Where [`key_out_with`] looks for the background's color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyHint {
    /// The color the model was asked to paint behind the subject (`key` in
    /// art/style.toml): the border colors near it are the background, even
    /// when the subject covers most of the border.
    pub expected: Option<[u8; 3]>,
    /// Sample the top rows instead of the whole border: a parallax layer's
    /// scenery fills the bottom and sides, its sky is at the top.
    pub top: bool,
}

/// How far (Oklab) a second background tone may be from the first, or from
/// the expected key color, and still count as background.
const SECOND_TONE: f32 = 0.3;

/// The background's colors from a sample of edge pixels: their median, or,
/// when the expected key color is there, the median of the pixels near it;
/// then a second tone when a good share of the rest is flat and close to the
/// first or to the expected color (a two-tone magenta).
fn background_colors(sample: &[[u8; 3]], tolerance: f32, expected: Option<[u8; 3]>) -> Vec<[u8; 3]> {
    let near_expected: Vec<[u8; 3]> = expected
        .map(|e| sample.iter().copied().filter(|c| oklab_dist(oklab(*c), oklab(e)) < SECOND_TONE).collect())
        .unwrap_or_default();
    let first = if near_expected.len() * 10 >= sample.len() { median_color(&near_expected) } else { median_color(sample) };
    let mut out = vec![first];
    let rest: Vec<[u8; 3]> = sample.iter().copied().filter(|c| oklab_dist(oklab(*c), oklab(first)) >= tolerance).collect();
    if rest.len() * 100 >= sample.len() * 15 {
        let second = median_color(&rest);
        let s_lab = oklab(second);
        let mut spread: Vec<f32> = rest.iter().map(|c| oklab_dist(oklab(*c), s_lab)).collect();
        spread.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let flat = spread[spread.len() * 3 / 4] < tolerance;
        let close = match expected {
            Some(e) => oklab_dist(s_lab, oklab(e)) < SECOND_TONE,
            None => oklab_dist(s_lab, oklab(first)) < SECOND_TONE,
        };
        if flat && close {
            out.push(second);
        }
    }
    out
}

/// Removes a flat background: estimates its color from the image border
/// (the median of the border pixels), clears pixels close to it with a soft
/// edge, removes the background's color from the semi-transparent edge
/// pixels ("despill"), and drops specks.
#[cfg(test)]
pub fn key_out(img: &Image, tolerance: f32) -> (Image, KeyReport) {
    key_out_with(img, tolerance, KeyHint::default())
}

/// [`key_out`] with a hint: the expected key color, and whether to sample
/// the top rows rather than the border. Two background tones are both keyed.
pub fn key_out_with(img: &Image, tolerance: f32, hint: KeyHint) -> (Image, KeyReport) {
    let border_px: Vec<[u8; 3]> = border(img.w, img.h).map(|(x, y)| img.get(x, y)).map(|p| [p[0], p[1], p[2]]).collect();
    let sample: Vec<[u8; 3]> = if hint.top {
        let rows = (img.h / 32).max(2).min(img.h);
        (0..rows).flat_map(|y| (0..img.w).map(move |x| (x, y))).map(|(x, y)| img.get(x, y)).map(|p| [p[0], p[1], p[2]]).collect()
    } else {
        border_px.clone()
    };
    let bgs = background_colors(&sample, tolerance, hint.expected);
    let bg = bgs[0];
    let labs: Vec<[f32; 3]> = bgs.iter().map(|c| oklab(*c)).collect();
    let nearest = |c: [u8; 3]| -> (usize, f32) {
        let l = oklab(c);
        labs.iter().enumerate().map(|(i, b)| (i, oklab_dist(l, *b))).fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
    };
    let mut d: Vec<f32> = border_px.iter().map(|c| nearest(*c).1).filter(|d| *d < tolerance * 2.0).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let border_bg = d.len() as f32 / border_px.len().max(1) as f32;
    let noise = d.get(d.len() * 9 / 10).copied().unwrap_or(0.0);
    // Pixels this close to a background tone are background.
    let (w, h) = (img.w as usize, img.h as usize);
    let is_bg: Vec<bool> = img.px.iter().map(|p| p[3] < 8 || nearest([p[0], p[1], p[2]]).1 < tolerance).collect();
    // The edge band: pixels within 2 px of the background, where the model
    // blended subject and background. Their alpha comes from projecting the
    // color onto the line from the background to the nearest solid subject
    // color (observed = a·fg + (1−a)·bg), which also gives the despilled fg.
    let near = dilate(&dilate(&is_bg, w, h), w, h);
    let mut out = img.clone();
    let mut cleared = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let p = img.px[i];
            if is_bg[i] {
                out.px[i] = [0; 4];
                cleared += 1;
                continue;
            }
            if !near[i] {
                continue;
            }
            // Unmixed from the background tone closest to it.
            let tone = bgs[nearest([p[0], p[1], p[2]]).0];
            let bgf = [tone[0] as f32, tone[1] as f32, tone[2] as f32];
            let dist2 = |p: Rgba| (0..3).map(|c| (p[c] as f32 - bgf[c]).powi(2)).sum::<f32>();
            // The most subject-like pixel nearby, preferring ones outside the band.
            let mut fg = p;
            let mut best = (false, dist2(p));
            for yy in y.saturating_sub(3)..(y + 4).min(h) {
                for xx in x.saturating_sub(3)..(x + 4).min(w) {
                    let j = yy * w + xx;
                    if is_bg[j] {
                        continue;
                    }
                    let q = img.px[j];
                    let key = (!near[j], dist2(q));
                    if key.0 > best.0 || (key.0 == best.0 && key.1 > best.1) {
                        best = key;
                        fg = q;
                    }
                }
            }
            let dir: Vec<f32> = (0..3).map(|c| fg[c] as f32 - bgf[c]).collect();
            let len2: f32 = dir.iter().map(|v| v * v).sum();
            let a =
                if len2 < 1.0 { 1.0 } else { ((0..3).map(|c| (p[c] as f32 - bgf[c]) * dir[c]).sum::<f32>() / len2).clamp(0.0, 1.0) } * (p[3] as f32 / 255.0);
            if a < 0.04 {
                out.px[i] = [0; 4];
                cleared += 1;
                continue;
            }
            let mut q = [0u8; 4];
            for c in 0..3 {
                q[c] = ((p[c] as f32 - (1.0 - a) * bgf[c]) / a).round().clamp(0.0, 255.0) as u8;
            }
            q[3] = (a * 255.0).round() as u8;
            out.px[i] = q;
        }
    }
    let min_area = ((img.w * img.h) as f32 * 0.0002).max(12.0) as usize;
    remove_specks(&mut out, min_area);
    let frac = cleared as f32 / img.px.len().max(1) as f32;
    (out, KeyReport { background: bg, backgrounds: bgs, border_bg, noise, cleared: frac })
}

/// 8-neighbour dilation of a mask.
fn dilate(m: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut out = m.to_vec();
    for y in 0..h {
        for x in 0..w {
            if m[y * w + x] {
                continue;
            }
            'n: for yy in y.saturating_sub(1)..(y + 2).min(h) {
                for xx in x.saturating_sub(1)..(x + 2).min(w) {
                    if m[yy * w + xx] {
                        out[y * w + x] = true;
                        break 'n;
                    }
                }
            }
        }
    }
    out
}

fn median_color(px: &[[u8; 3]]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let mut v: Vec<u8> = px.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *o = v[v.len() / 2];
    }
    out
}

/// Clears connected groups of visible pixels smaller than `min_area`.
pub fn remove_specks(img: &mut Image, min_area: usize) {
    let (w, h) = (img.w as usize, img.h as usize);
    let mut label = vec![u32::MAX; w * h];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if img.px[start][3] < 24 || label[start] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let mut n = 0;
        stack.push(start);
        label[start] = id;
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if img.px[j][3] >= 24 && label[j] == u32::MAX {
                    label[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        sizes.push(n);
    }
    for i in 0..w * h {
        let l = label[i];
        if l == u32::MAX {
            if img.px[i][3] < 24 {
                img.px[i] = [0; 4];
            }
        } else if sizes[l as usize] < min_area {
            img.px[i] = [0; 4];
        }
    }
}

// --- pixel art ------------------------------------------------------------------

/// Reduces an image to a gw×gh grid of art pixels: a cell is opaque when at
/// least half covered, and takes the median color of the middle of the cell
/// (so blended edges between a model's "pixels" don't muddy it).
pub fn pixelate(img: &Image, gw: u32, gh: u32) -> Image {
    let mut out = Image::new(gw, gh, [0; 4]);
    for gy in 0..gh {
        for gx in 0..gw {
            let x0 = gx as f64 * img.w as f64 / gw as f64;
            let x1 = (gx + 1) as f64 * img.w as f64 / gw as f64;
            let y0 = gy as f64 * img.h as f64 / gh as f64;
            let y1 = (gy + 1) as f64 * img.h as f64 / gh as f64;
            let (cx0, cx1) = (x0.floor() as u32, (x1.ceil() as u32).min(img.w).max(x0.floor() as u32 + 1));
            let (cy0, cy1) = (y0.floor() as u32, (y1.ceil() as u32).min(img.h).max(y0.floor() as u32 + 1));
            let mut cover = 0f64;
            let mut n = 0f64;
            for y in cy0..cy1 {
                for x in cx0..cx1 {
                    cover += img.get(x, y)[3] as f64 / 255.0;
                    n += 1.0;
                }
            }
            if n == 0.0 || cover / n < 0.5 {
                continue;
            }
            // Inner 60% of the cell.
            let ix0 = (x0 + (x1 - x0) * 0.2).floor() as u32;
            let ix1 = ((x1 - (x1 - x0) * 0.2).ceil() as u32).max(ix0 + 1).min(img.w);
            let iy0 = (y0 + (y1 - y0) * 0.2).floor() as u32;
            let iy1 = ((y1 - (y1 - y0) * 0.2).ceil() as u32).max(iy0 + 1).min(img.h);
            let mut cols = Vec::new();
            for y in iy0..iy1 {
                for x in ix0..ix1 {
                    let p = img.get(x, y);
                    if p[3] >= 128 {
                        cols.push([p[0], p[1], p[2]]);
                    }
                }
            }
            if cols.is_empty() {
                for y in cy0..cy1 {
                    for x in cx0..cx1 {
                        let p = img.get(x, y);
                        if p[3] > 0 {
                            cols.push([p[0], p[1], p[2]]);
                        }
                    }
                }
            }
            if cols.is_empty() {
                continue;
            }
            let c = median_color(&cols);
            out.set(gx, gy, [c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// Snaps every visible pixel to the nearest palette color (in Oklab).
pub fn lock_palette(img: &mut Image, palette: &[[u8; 3]]) {
    if palette.is_empty() {
        return;
    }
    let labs: Vec<[f32; 3]> = palette.iter().map(|c| oklab(*c)).collect();
    let mut cache = std::collections::HashMap::new();
    for p in &mut img.px {
        if p[3] == 0 {
            continue;
        }
        let key = [p[0], p[1], p[2]];
        let best = *cache.entry(key).or_insert_with(|| {
            let l = oklab(key);
            let mut best = 0;
            let mut bd = f32::MAX;
            for (i, pl) in labs.iter().enumerate() {
                let d = oklab_dist(l, *pl);
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
            best
        });
        let c = palette[best];
        *p = [c[0], c[1], c[2], p[3]];
    }
}

/// Median-cut quantization to at most `n` RGBA colors (alpha is quantized
/// too, so soft edges survive in an indexed PNG).
pub fn quantize(img: &mut Image, n: usize) {
    let n = n.clamp(2, 256);
    if img.distinct_colors(n).len() <= n {
        return;
    }
    let mut counts: std::collections::HashMap<Rgba, u32> = std::collections::HashMap::new();
    for p in &img.px {
        let p = if p[3] == 0 { [0; 4] } else { *p };
        *counts.entry(p).or_default() += 1;
    }
    let has_clear = counts.contains_key(&[0, 0, 0, 0]);
    counts.remove(&[0, 0, 0, 0]);
    let mut boxes: Vec<Vec<(Rgba, u32)>> = vec![counts.into_iter().collect()];
    let target = if has_clear { n - 1 } else { n };
    while boxes.len() < target {
        // Split the box with the widest weighted channel range.
        let mut best = None;
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for c in 0..4 {
                let lo = b.iter().map(|e| e.0[c]).min().unwrap();
                let hi = b.iter().map(|e| e.0[c]).max().unwrap();
                let weight: u64 = b.iter().map(|e| e.1 as u64).sum();
                let score = (hi - lo) as u64 * (weight as f64).sqrt() as u64;
                if best.is_none_or(|(s, _, _)| score > s) {
                    best = Some((score, i, c));
                }
            }
        }
        let Some((score, i, c)) = best else { break };
        if score == 0 {
            break;
        }
        let mut b = boxes.swap_remove(i);
        b.sort_by_key(|e| e.0[c]);
        let total: u64 = b.iter().map(|e| e.1 as u64).sum();
        let mut acc = 0;
        let mut cut = 1;
        for (k, e) in b.iter().enumerate() {
            acc += e.1 as u64;
            if acc * 2 >= total {
                cut = (k + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(cut);
        boxes.push(b);
        boxes.push(rest);
    }
    let palette: Vec<Rgba> = boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let w: f64 = b.iter().map(|e| e.1 as f64).sum();
            let mut m = [0f64; 4];
            for e in b {
                for c in 0..4 {
                    m[c] += e.0[c] as f64 * e.1 as f64;
                }
            }
            [(m[0] / w).round() as u8, (m[1] / w).round() as u8, (m[2] / w).round() as u8, (m[3] / w).round() as u8]
        })
        .collect();
    let mut cache: std::collections::HashMap<Rgba, Rgba> = std::collections::HashMap::new();
    for p in &mut img.px {
        if p[3] == 0 {
            *p = [0; 4];
            continue;
        }
        let key = *p;
        *p = *cache.entry(key).or_insert_with(|| {
            *palette
                .iter()
                .min_by_key(|q| {
                    let d: i32 = (0..4).map(|c| (q[c] as i32 - key[c] as i32).pow(2) * if c == 3 { 2 } else { 1 }).sum();
                    d
                })
                .unwrap()
        });
    }
}

// --- seamless -------------------------------------------------------------------

/// Makes the image wrap seamlessly along x and/or y by cross-fading a band
/// from the far edge into the near one. The result is smaller by the band.
pub fn seamless(img: &Image, x: bool, y: bool, band_frac: f32) -> Image {
    let mut cur = img.clone();
    if x {
        cur = seam_x(&cur, band_frac);
    }
    if y {
        cur = transpose(&seam_x(&transpose(&cur), band_frac));
    }
    cur
}

fn transpose(img: &Image) -> Image {
    let mut out = Image::new(img.h, img.w, [0; 4]);
    for y in 0..img.h {
        for x in 0..img.w {
            out.set(y, x, img.get(x, y));
        }
    }
    out
}

/// Joins the far band onto the near edge: within the band, the column where
/// the original and its wrap partner match best is found and the switch
/// happens there with a short blend, so silhouettes (parallax hills) meet
/// instead of ghosting over each other.
fn seam_x(cur: &Image, band_frac: f32) -> Image {
    let b = ((cur.w as f32 * band_frac).round() as u32).clamp(2, cur.w / 2);
    let nw = cur.w - b;
    let half = (b / 6).max(1);
    let diff = |i: u32| -> f32 {
        (0..cur.h)
            .map(|yy| {
                let (a, c) = (premul(cur.get(nw + i, yy)), premul(cur.get(i, yy)));
                (0..4).map(|k| (a[k] - c[k]).abs()).sum::<f32>()
            })
            .sum()
    };
    let lo = (2 * half).min(b - 1);
    let hi = b.saturating_sub(half).max(lo + 1);
    let cut = (lo..hi).min_by(|a, c| diff(*a).partial_cmp(&diff(*c)).unwrap()).unwrap_or(b / 2);
    let mut out = cur.crop(0, 0, nw, cur.h);
    for i in 0..b {
        // 0 → the far edge's pixels, 1 → the original's.
        let t = ((i as f32 + 0.5 - (cut as f32 - half as f32)) / (2.0 * half as f32)).clamp(0.0, 1.0);
        for yy in 0..cur.h {
            out.set(i, yy, mix(cur.get(nw + i, yy), cur.get(i, yy), smooth(t)));
        }
    }
    out
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let pa = premul(a);
    let pb = premul(b);
    let mut o = [0f32; 4];
    for c in 0..4 {
        o[c] = pa[c] * (1.0 - t) + pb[c] * t;
    }
    unpremul(o)
}

/// How far apart the opposite edges are (mean Oklab distance).
#[cfg(test)]
pub fn wrap_error(img: &Image, horizontal: bool) -> f32 {
    let (n, m) = if horizontal { (img.h, img.w) } else { (img.w, img.h) };
    let mut sum = 0.0;
    for i in 0..n {
        let (a, b) = if horizontal { (img.get(m - 1, i), img.get(0, i)) } else { (img.get(i, m - 1), img.get(i, 0)) };
        sum += oklab_dist(oklab([a[0], a[1], a[2]]), oklab([b[0], b[1], b[2]]));
    }
    sum / n as f32
}

// --- PNG ------------------------------------------------------------------------

fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (info.width, info.height);
    let data = &buf[..info.buffer_size()];
    let px = match info.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("PNG: unexpanded palette".into()),
    };
    Ok(Image { w, h, px })
}

fn decode_jpeg(bytes: &[u8]) -> Result<Image, String> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(Cursor::new(bytes), opts);
    let data = dec.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let info = dec.info().ok_or("JPEG: no header")?;
    let (w, h) = (info.width as u32, info.height as u32);
    let px: Vec<Rgba> = data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 255]).collect();
    if px.len() != (w * h) as usize {
        return Err("JPEG: unexpected pixel count".into());
    }
    Ok(Image { w, h, px })
}

/// The smallest PNG among: indexed (when ≤ 256 colors, at the smallest bit
/// depth), RGB (when opaque) or RGBA, each with a few filter strategies.
pub fn encode_png(img: &Image) -> Vec<u8> {
    let mut best: Option<Vec<u8>> = None;
    let mut keep = |v: Vec<u8>| {
        if best.as_ref().is_none_or(|b| v.len() < b.len()) {
            best = Some(v);
        }
    };
    let colors = img.distinct_colors(256);
    if colors.len() <= 256 {
        let index: std::collections::HashMap<Rgba, u8> = colors.iter().enumerate().map(|(i, c)| (*c, i as u8)).collect();
        let depth = match colors.len() {
            0..=2 => 1u8,
            3..=4 => 2,
            5..=16 => 4,
            _ => 8,
        };
        let idx: Vec<u8> = img.px.iter().map(|p| index[&if p[3] == 0 { [0; 4] } else { *p }]).collect();
        let packed = pack_bits(&idx, img.w as usize, img.h as usize, depth);
        let palette: Vec<u8> = colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        let trns: Vec<u8> = colors.iter().map(|c| c[3]).collect();
        let last_opaque = trns.iter().rposition(|&a| a != 255);
        for filter in [png::Filter::NoFilter, png::Filter::Adaptive] {
            keep(write_png(img.w, img.h, png::ColorType::Indexed, depth, &packed, Some((&palette, last_opaque.map(|i| &trns[..=i]))), filter));
        }
    }
    let (ct, data): (png::ColorType, Vec<u8>) = if img.is_opaque() {
        (png::ColorType::Rgb, img.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect())
    } else {
        // Zero the color of clear pixels: compresses better and hides nothing.
        (png::ColorType::Rgba, img.px.iter().flat_map(|p| if p[3] == 0 { [0; 4] } else { *p }).collect())
    };
    for filter in [png::Filter::Adaptive, png::Filter::Paeth, png::Filter::NoFilter] {
        keep(write_png(img.w, img.h, ct, 8, &data, None, filter));
    }
    best.unwrap()
}

fn pack_bits(idx: &[u8], w: usize, h: usize, depth: u8) -> Vec<u8> {
    if depth == 8 {
        return idx.to_vec();
    }
    let per = 8 / depth as usize;
    let row = w.div_ceil(per);
    let mut out = vec![0u8; row * h];
    for y in 0..h {
        for x in 0..w {
            let v = idx[y * w + x];
            let shift = 8 - depth as usize * (x % per + 1);
            out[y * row + x / per] |= v << shift;
        }
    }
    out
}

fn write_png(w: u32, h: u32, ct: png::ColorType, depth: u8, data: &[u8], pal: Option<(&[u8], Option<&[u8]>)>, filter: png::Filter) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(ct);
        enc.set_depth(match depth {
            1 => png::BitDepth::One,
            2 => png::BitDepth::Two,
            4 => png::BitDepth::Four,
            _ => png::BitDepth::Eight,
        });
        enc.set_deflate_compression(png::DeflateCompression::Level(9));
        enc.set_filter(filter);
        if let Some((p, t)) = pal {
            enc.set_palette(p.to_vec());
            if let Some(t) = t {
                enc.set_trns(t.to_vec());
            }
        }
        let mut wr = enc.write_header().expect("png header");
        wr.write_image_data(data).expect("png data");
    }
    out
}

/// PNG width and height from the header, without decoding.
pub fn png_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG") {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// A JPEG's width and height, from its first start-of-frame marker.
pub fn jpeg_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xff {
            return None;
        }
        let marker = bytes[i + 1];
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// Several images in a grid with gaps, for looking at variants at once.
pub fn contact_sheet(images: &[Image]) -> Image {
    let cell = images.iter().map(|i| i.w.max(i.h)).max().unwrap_or(1);
    let cols = (images.len() as f64).sqrt().ceil().max(1.0) as u32;
    let rows = (images.len() as u32).div_ceil(cols);
    let pad = 12;
    let mut out = Image::new(cols * (cell + pad) + pad, rows * (cell + pad) + pad, [40, 40, 48, 255]);
    for (n, im) in images.iter().enumerate() {
        let (c, r) = (n as u32 % cols, n as u32 / cols);
        let x = pad + c * (cell + pad) + (cell - im.w) / 2;
        let y = pad + r * (cell + pad) + (cell - im.h) / 2;
        for yy in 0..im.h {
            for xx in 0..im.w {
                let g = if ((xx / 8) + (yy / 8)) % 2 == 0 { [200, 200, 200, 255] } else { [150, 150, 150, 255] };
                out.set(x + xx, y + yy, over(im.get(xx, yy), g));
            }
        }
        // Variant number as dots under the cell (1 dot = variant 1).
        for d in 0..=n as u32 {
            let dx = pad + c * (cell + pad) + d * 6;
            let dy = pad + r * (cell + pad) + cell + 3;
            for yy in 0..4 {
                for xx in 0..4 {
                    if dx + xx < out.w && dy + yy < out.h {
                        out.set(dx + xx, dy + yy, [255, 210, 80, 255]);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc of `fg` with a soft edge on a flat `bg`, like a model's sprite.
    pub fn disc(size: u32, fg: [u8; 3], bg: [u8; 3]) -> Image {
        let mut im = Image::new(size, size, [bg[0], bg[1], bg[2], 255]);
        let c = size as f32 / 2.0;
        let r = size as f32 * 0.3;
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
                let t = (r + 1.0 - d).clamp(0.0, 1.0);
                if t > 0.0 {
                    let p = [bg[0], bg[1], bg[2], 255];
                    let f = [fg[0], fg[1], fg[2], (t * 255.0) as u8];
                    im.set(x, y, over(f, p));
                }
            }
        }
        im
    }

    #[test]
    fn png_roundtrip_all_encodings() {
        let mut a = disc(40, [200, 30, 30], [0, 255, 0]);
        let b = Image::decode(&a.encode_png()).unwrap();
        assert_eq!(a, b);
        a.px[5][3] = 100; // now RGBA, still few colors → indexed with tRNS
        let b = Image::decode(&a.encode_png()).unwrap();
        assert_eq!(a.px[5], b.px[5]);
        assert_eq!(a, b);
        // Many colors → RGBA path.
        let mut g = Image::new(64, 64, [0; 4]);
        for (i, p) in g.px.iter_mut().enumerate() {
            *p = [(i % 256) as u8, (i / 16) as u8, (i * 7 % 256) as u8, 200];
        }
        assert_eq!(Image::decode(&g.encode_png()).unwrap(), g);
        assert_eq!(png_dims(&g.encode_png()), Some((64, 64)));
    }

    #[test]
    fn indexed_is_much_smaller_for_pixel_art() {
        let small = disc(32, [200, 30, 30], [10, 10, 40]).scale_nearest(4);
        let rgba_size =
            write_png(small.w, small.h, png::ColorType::Rgba, 8, &small.px.iter().flatten().copied().collect::<Vec<_>>(), None, png::Filter::Adaptive).len();
        assert!(small.encode_png().len() < rgba_size);
    }

    #[test]
    fn resize_keeps_flat_color_and_size() {
        let im = Image::new(100, 50, [10, 200, 30, 255]);
        let r = im.resize(33, 17);
        assert_eq!((r.w, r.h), (33, 17));
        assert!(r.px.iter().all(|p| (p[1] as i32 - 200).abs() <= 1 && p[3] == 255));
        // Transparent pixels don't bleed black into the edge.
        let mut half = Image::new(10, 10, [0; 4]);
        for y in 0..10 {
            for x in 5..10 {
                half.set(x, y, [255, 255, 255, 255]);
            }
        }
        let r = half.resize(5, 5);
        assert!(r.px.iter().filter(|p| p[3] > 0).all(|p| p[0] > 240));
    }

    #[test]
    fn key_out_clears_background_and_despills() {
        let im = disc(200, [220, 40, 40], [0, 255, 0]);
        let (k, rep) = key_out(&im, 0.08);
        assert_eq!(rep.background, [0, 255, 0]);
        assert_eq!(k.get(0, 0)[3], 0);
        assert_eq!(k.get(100, 100), [220, 40, 40, 255]);
        // Edge pixels carry no green spill.
        for p in k.px.iter().filter(|p| p[3] > 0 && p[3] < 255) {
            assert!(p[1] < 90, "spill left in {p:?}");
        }
    }

    #[test]
    fn a_two_tone_background_is_keyed_whole() {
        // Magenta with a darker magenta band (as a model sometimes paints it).
        let mut im = disc(200, [40, 160, 220], [255, 0, 255]);
        for y in 0..200 {
            for x in 0..200 {
                if (x / 25 + y / 25) % 2 == 1 && im.get(x, y) == [255, 0, 255, 255] {
                    im.set(x, y, [200, 0, 200, 255]);
                }
            }
        }
        let (k, rep) = key_out(&im, 0.09);
        assert_eq!(rep.backgrounds.len(), 2, "{:?}", rep.backgrounds);
        assert_eq!(k.get(0, 0)[3], 0);
        assert_eq!(k.get(30, 2)[3], 0);
        assert_eq!(k.get(100, 100), [40, 160, 220, 255]);
        // The subject's own color is never taken for a second tone.
        let (_, rep) = key_out(&disc(200, [40, 160, 220], [255, 0, 255]), 0.09);
        assert_eq!(rep.backgrounds, vec![[255, 0, 255]]);
    }

    #[test]
    fn a_layer_is_keyed_from_its_sky_and_the_expected_key() {
        // Scenery fills the bottom 60% and both sides; sky (pure blue) only at the top.
        let mut im = Image::new(200, 100, [0, 0, 255, 255]);
        for y in 40..100 {
            for x in 0..200 {
                im.set(x, y, [60, 40, 30, 255]);
            }
        }
        for y in 0..100 {
            for x in (0..12).chain(188..200) {
                im.set(x, y, [60, 40, 30, 255]);
            }
        }
        let (_, by_border) = key_out(&im, 0.09);
        assert_eq!(by_border.background, [60, 40, 30], "the old estimate keys the scenery");
        let (k, rep) = key_out_with(&im, 0.09, KeyHint { top: true, ..KeyHint::default() });
        assert_eq!(rep.background, [0, 0, 255]);
        assert_eq!(k.get(100, 10)[3], 0);
        assert_eq!(k.get(100, 80)[3], 255);
        let (_, rep) = key_out_with(&im, 0.09, KeyHint { expected: Some([0, 0, 255]), top: false });
        assert_eq!(rep.background, [0, 0, 255]);
    }

    #[test]
    fn specks_are_removed() {
        let mut im = Image::new(100, 100, [0; 4]);
        im.set(3, 3, [255, 0, 0, 255]);
        for y in 40..60 {
            for x in 40..60 {
                im.set(x, y, [0, 0, 255, 255]);
            }
        }
        remove_specks(&mut im, 10);
        assert_eq!(im.get(3, 3)[3], 0);
        assert_eq!(im.get(50, 50)[3], 255);
    }

    #[test]
    fn pixelate_then_palette_lock() {
        let im = disc(256, [230, 50, 40], [0, 255, 0]);
        let (k, _) = key_out(&im, 0.08);
        let mut p = pixelate(&k.trimmed(), 16, 16);
        assert_eq!((p.w, p.h), (16, 16));
        assert!(p.px.iter().all(|c| c[3] == 0 || c[3] == 255));
        let pal = [[255, 0, 0], [20, 20, 20]];
        lock_palette(&mut p, &pal);
        assert!(p.px.iter().filter(|c| c[3] > 0).all(|c| [c[0], c[1], c[2]] == [255, 0, 0]));
        assert_eq!(p.get(0, 0)[3], 0, "corner of a disc is clear");
    }

    #[test]
    fn quantize_limits_colors() {
        let mut g = Image::new(64, 64, [0; 4]);
        for (i, p) in g.px.iter_mut().enumerate() {
            *p = [(i % 64 * 4) as u8, (i / 64 * 4) as u8, 128, 255];
        }
        quantize(&mut g, 16);
        assert!(g.distinct_colors(64).len() <= 16);
    }

    #[test]
    fn seamless_wraps() {
        let mut g = Image::new(80, 40, [0; 4]);
        for y in 0..40 {
            for x in 0..80 {
                g.set(x, y, [(x * 3) as u8, (y * 6) as u8, 100, 255]);
            }
        }
        assert!(wrap_error(&g, true) > 0.2);
        let s = seamless(&g, true, true, 0.125);
        assert_eq!((s.w, s.h), (70, 35));
        assert!(wrap_error(&s, true) < 0.03, "{}", wrap_error(&s, true));
        assert!(wrap_error(&s, false) < 0.03);
    }

    #[test]
    fn contain_and_cover_are_exact() {
        let im = disc(100, [1, 2, 3], [9, 9, 9]).crop(0, 0, 100, 60);
        let c = im.contain(64, 64, 4, Anchor::Bottom);
        assert_eq!((c.w, c.h), (64, 64));
        let (_, y, _, h) = c.content_bbox(0).unwrap();
        assert_eq!(y + h, 60, "bottom-anchored content ends at the margin");
        let v = im.cover(30, 90);
        assert_eq!((v.w, v.h), (30, 90));
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#1b1033").unwrap(), [0x1b, 0x10, 0x33]);
        assert_eq!(parse_hex("fff").unwrap(), [255, 255, 255]);
        assert!(parse_hex("#12").is_err());
        assert_eq!(hex([1, 2, 255]), "#0102ff");
    }
}
