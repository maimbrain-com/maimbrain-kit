//! `mb art sky`: a model's panorama → an equirectangular sky for mb3d's
//! `mb3d_sky_texture` (2:1, the horizon across the middle, the top row
//! straight up, the left and right edges one direction).
//!
//! Image models don't reliably draw true equirectangular projections. What
//! they do draw well, asked for "a 360° equirectangular panorama", is a wide
//! landscape with the horizon in the middle and the sky above it. So:
//!
//! 1. The provider is asked for 21:9 (no provider offers 2:1); the image is
//!    cropped at its sides to 2:1 plus a seam band, keeping its pixels'
//!    aspect, and the band is joined onto the opposite edge where the two
//!    match best (`img::seamless`), so the left and right edges meet.
//! 2. The horizon goes on the middle row. `--horizon` says where it is in
//!    the image (0–1 from the top; by default it's found, since models put
//!    it a little off the middle, e.g. 0.55 when the sky got more room).
//!    `--span` says how many degrees of elevation the image covers. At 180
//!    (the default) its top row is straight up and its bottom row straight
//!    down, and each half is stretched to its 90°. Below 180 it covers that
//!    many degrees at an even scale, and the rows it doesn't reach are
//!    filled by stretching its top and bottom rows' colors toward the poles,
//!    blurring as they go: color extension, not painting, and the facts and
//!    a warning say how many rows were filled that way.
//! 3. The poles: an equirect squeezes a whole row into one point there, so
//!    any detail in the top and bottom rows pinches into a swirl. Each row
//!    is blurred around the circle by how much it's squeezed
//!    (`(1/cos φ − 1)/2` pixels), and within `--poles` degrees of each pole
//!    blended into one even color (the mean of the cap's edge).
//! 4. JPEG (baseline, 4:2:0): a 2048×1024 sky is a few hundred KB, where a
//!    PNG would be several MB. Quality steps down until it fits `--max-kb`.

use super::img::Image;

/// What `equirect` did, for the sidecar.
pub struct Report {
    /// Rows filled by color extension above and below the image (span < 180).
    pub extended: [u32; 2],
    /// Fraction of the source's width cropped away to reach 2:1 (+ the seam band).
    pub cropped: f32,
}

/// A horizon found in a panorama: its height (0–1 from the top) and how sure
/// the finder is (0–1: how far the best row stands out from the next best
/// one elsewhere; below 0.25, look at the preview).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Horizon {
    pub at: f32,
    pub confidence: f32,
}

/// Below this, `mb art sky` warns that the horizon is a guess.
pub const SURE: f32 = 0.25;

/// The horizon's height in a panorama: the row where the picture changes
/// most from above to below, over a prior around 55% (a painted horizon sits
/// a little below the middle). Each row's change is measured at 512 pixels
/// wide, a few rows apart: the mean brightness falling (a glowing sky over
/// darker land or water), the mean absolute change of each pixel's
/// brightness (a sharp line even where the averages match: a water line
/// under distant hills) and the change in saturation (a hazy sky over
/// colored ground), lightly smoothed. Searched between 25% and 85%. None if
/// no row changes at all (an all-sky painting).
pub fn find_horizon(im: &Image) -> Option<Horizon> {
    let w = 512u32.min(im.w.max(1));
    let h = ((im.h as f32 * w as f32 / im.w.max(1) as f32).round() as u32).max(1);
    if h < 30 {
        return None;
    }
    let small = if (w, h) == (im.w, im.h) { im.clone() } else { im.resize(w, h) };
    let (wu, hu) = (w as usize, h as usize);
    let mut luma = vec![0f32; wu * hu];
    let mut sat_row = vec![0f32; hu];
    for y in 0..hu {
        for x in 0..wu {
            let p = small.get(x as u32, y as u32);
            let c = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
            luma[y * wu + x] = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            sat_row[y] += if mx > 0.0 { (mx - mn) / mx } else { 0.0 } / wu as f32;
        }
    }
    let d = (hu / 128).max(2);
    let mut change = vec![0f32; hu];
    for y in d..hu - d {
        let (above, below) = (&luma[(y - d) * wu..(y - d + 1) * wu], &luma[(y + d) * wu..(y + d + 1) * wu]);
        let fall = above.iter().zip(below).map(|(a, b)| a - b).sum::<f32>() / wu as f32;
        let abs = above.iter().zip(below).map(|(a, b)| (a - b).abs()).sum::<f32>() / wu as f32;
        change[y] = fall.max(0.0) + 0.5 * abs + 0.5 * (sat_row[y - d] - sat_row[y + d]).abs();
    }
    let change: Vec<f32> = (0..hu).map(|y| (y.saturating_sub(1)..(y + 2).min(hu)).map(|i| change[i]).sum::<f32>() / 3.0).collect();
    let (lo, hi) = (hu / 4, hu * 17 / 20);
    let score = |y: usize| {
        let t = (y as f32 / hu as f32 - 0.55) / 0.15;
        change[y] * (-0.5 * t * t).exp()
    };
    let best = (lo..hi).max_by(|a, b| score(*a).total_cmp(&score(*b)))?;
    if change[lo..hi].iter().cloned().fold(0.0, f32::max) < 0.01 {
        return None;
    }
    let apart = (hu / 20).max(2);
    let second = (lo..hi).filter(|y| y.abs_diff(best) > apart).map(score).fold(0.0, f32::max);
    let ratio = score(best) / second.max(1e-6);
    Some(Horizon { at: best as f32 / hu as f32, confidence: (ratio - 1.0).clamp(0.0, 1.0) })
}

/// The seam band, as a fraction of the output width.
const BAND: f32 = 0.06;

/// Makes a `w`×`h` (2:1) equirectangular sky from a provider's panorama.
pub fn equirect(src: &Image, w: u32, h: u32, horizon: f32, span: f32, poles: f32) -> (Image, Report) {
    let span = span.clamp(10.0, 180.0);
    let horizon = horizon.clamp(0.01, 0.99);
    // Rows of the output the source covers, keeping its pixels square in angle.
    let rows = ((h as f32 * span / 180.0).round() as u32).clamp(2, h);
    let wide = (w as f32 * (1.0 + BAND)).round() as u32;
    let cropped = 1.0 - (src.h as f32 * wide as f32 / rows as f32 / src.w as f32).min(1.0);
    let strip = super::img::seamless(&src.cover(wide, rows), true, false, BAND / (1.0 + BAND));
    let strip = if strip.w != w { strip.resize(w, rows) } else { strip };
    // The strip's row at each elevation, its horizon on the middle row.
    let at = horizon * rows as f32;
    let source_row = |y: u32| {
        let e = 90.0 - 180.0 * (y as f32 + 0.5) / h as f32;
        if span >= 180.0 {
            if e >= 0.0 { at * (1.0 - e / 90.0) } else { at + (rows as f32 - at) * (-e / 90.0) }
        } else {
            at - e * rows as f32 / span
        }
    };
    let mut out = Image::new(w, h, [0, 0, 0, 255]);
    let mut extended = [0u32; 2];
    let top = row_of(&strip, 0);
    let bottom = row_of(&strip, rows - 1);
    for y in 0..h {
        let sy = source_row(y) - 0.5;
        // A full span reaches both poles: its first and last rows are the edge.
        let sy = if span >= 180.0 { sy.clamp(0.0, (rows - 1) as f32) } else { sy };
        let row: Vec<[f32; 3]> = if sy < 0.0 {
            extended[0] += 1;
            blur_row(&top, (-sy * 3.0) as usize)
        } else if sy > (rows - 1) as f32 {
            extended[1] += 1;
            blur_row(&bottom, ((sy - (rows - 1) as f32) * 3.0) as usize)
        } else {
            let (y0, t) = (sy.floor() as u32, sy.fract());
            let (a, b) = (row_of(&strip, y0), row_of(&strip, (y0 + 1).min(rows - 1)));
            a.iter().zip(&b).map(|(p, q)| [0, 1, 2].map(|c| p[c] + (q[c] - p[c]) * t)).collect()
        };
        put_row(&mut out, y, &row);
    }
    calm_poles(&mut out, poles);
    (out, Report { extended, cropped })
}

fn row_of(im: &Image, y: u32) -> Vec<[f32; 3]> {
    (0..im.w).map(|x| im.get(x, y)).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect()
}

fn put_row(im: &mut Image, y: u32, row: &[[f32; 3]]) {
    for (x, p) in row.iter().enumerate() {
        im.set(x as u32, y, [p[0].round() as u8, p[1].round() as u8, p[2].round() as u8, 255]);
    }
}

/// A box blur of radius `r` around the row, wrapping (it's a circle of directions).
fn blur_row(row: &[[f32; 3]], r: usize) -> Vec<[f32; 3]> {
    let n = row.len();
    if r == 0 || n == 0 {
        return row.to_vec();
    }
    if 2 * r + 1 >= n {
        let m = mean(row);
        return vec![m; n];
    }
    let mut acc = [0f32; 3];
    for k in 0..=2 * r {
        let p = row[(k + n - r) % n];
        (0..3).for_each(|c| acc[c] += p[c]);
    }
    let k = (2 * r + 1) as f32;
    let mut out = Vec::with_capacity(n);
    for x in 0..n {
        out.push(acc.map(|v| v / k));
        let (add, sub) = (row[(x + r + 1) % n], row[(x + n - r) % n]);
        (0..3).for_each(|c| acc[c] += add[c] - sub[c]);
    }
    out
}

fn mean(row: &[[f32; 3]]) -> [f32; 3] {
    let n = row.len().max(1) as f32;
    let s = row.iter().fold([0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
    s.map(|v| v / n)
}

/// Blurs each row by how much the equirect squeezes it, and blends the
/// caps within `poles` degrees of each pole into one color.
fn calm_poles(im: &mut Image, poles: f32) {
    let (w, h) = (im.w, im.h);
    let lat = |y: u32| 90.0 - 180.0 * (y as f32 + 0.5) / h as f32;
    let poles = poles.clamp(0.0, 45.0);
    // Each cap's color: the mean of the row where the blend starts.
    let edge = |north: bool| {
        let y = ((poles / 180.0 * h as f32) as u32).min(h / 2 - 1);
        mean(&row_of(im, if north { y } else { h - 1 - y }))
    };
    let caps = [edge(true), edge(false)];
    for y in 0..h {
        let phi = lat(y).to_radians();
        let squeeze = 1.0 / phi.cos().max(1e-3) - 1.0;
        let r = ((squeeze * 0.5).round() as usize).min(w as usize / 2);
        let near = 90.0 - lat(y).abs();
        if r == 0 && near >= poles {
            continue;
        }
        let mut row = blur_row(&row_of(im, y), r);
        if poles > 0.0 && near < poles {
            let t = 1.0 - near / poles;
            let t = t * t * (3.0 - 2.0 * t);
            let cap = caps[(lat(y) < 0.0) as usize];
            row.iter_mut().for_each(|p| *p = [0, 1, 2].map(|c| p[c] + (cap[c] - p[c]) * t));
        }
        put_row(im, y, &row);
    }
}

/// Baseline JPEG at `quality` (1–100), 4:2:0.
pub fn encode_jpeg(im: &Image, quality: u8) -> Result<Vec<u8>, String> {
    let rgb: Vec<u8> = im.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, quality.clamp(1, 100));
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
    enc.encode(&rgb, im.w as u16, im.h as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| format!("JPEG: {e}"))?;
    Ok(out)
}

/// The JPEG at the highest quality from 88 down that fits `max_kb`
/// (quality 40 if none does), and the quality used.
pub fn encode_budget(im: &Image, max_kb: u32) -> Result<(Vec<u8>, u8), String> {
    let mut q = 88;
    loop {
        let jpg = encode_jpeg(im, q)?;
        if jpg.len() <= max_kb as usize * 1024 || q <= 40 {
            return Ok((jpg, q));
        }
        q -= 8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panorama that doesn't wrap: a sky gradient, a horizon in the middle,
    /// a bright stripe at the left edge only, and busy stars up to the top row.
    fn source(w: u32, h: u32) -> Image {
        let mut im = Image::new(w, h, [0, 0, 0, 255]);
        for y in 0..h {
            for x in 0..w {
                let v = y as f32 / h as f32;
                let mut c = if v < 0.5 { [20.0 + 120.0 * v, 30.0 + 100.0 * v, 90.0 + 80.0 * v] } else { [25.0, 20.0, 30.0] };
                if x < w / 40 {
                    c = [240.0, 240.0, 240.0];
                }
                if v < 0.5 && (x * 7 + y * 13) % 29 == 0 {
                    c = [255.0, 250.0, 220.0];
                }
                im.set(x, y, [c[0] as u8, c[1] as u8, c[2] as u8, 255]);
            }
        }
        im
    }

    fn row_spread(im: &Image, y: u32) -> f32 {
        let r = row_of(im, y);
        let m = mean(&r);
        r.iter().map(|p| (0..3).map(|c| (p[c] - m[c]).abs()).sum::<f32>()).fold(0.0, f32::max)
    }

    #[test]
    fn a_wide_panorama_becomes_a_seamless_2_to_1_sky_with_calm_poles() {
        let (sky, rep) = equirect(&source(733, 314), 256, 128, 0.5, 180.0, 12.0);
        assert_eq!((sky.w, sky.h), (256, 128));
        assert_eq!(rep.extended, [0, 0]);
        assert!(rep.cropped > 0.0 && rep.cropped < 0.15, "a 21:9 source loses a little width: {}", rep.cropped);
        assert!(super::super::img::wrap_error(&sky, true) < 0.03, "the left and right edges meet");
        // Stars reached the top row of the source; at the poles every row is one color.
        assert!(row_spread(&sky, 0) < 1.0 && row_spread(&sky, 127) < 1.0);
        assert!(row_spread(&sky, 30) > 50.0, "detail stays away from the poles");
        // The horizon (sky above, ground below) stays on the middle row.
        let (above, below) = (sky.get(100, 60), sky.get(100, 68));
        assert!(above[2] > 120 && below[2] < 60, "{above:?} {below:?}");
    }

    #[test]
    fn the_horizon_is_found_where_the_glow_meets_the_ground() {
        let mut im = source(400, 200);
        let at = |im: &Image| find_horizon(im).map(|h| (h.at * 100.0).round() / 100.0);
        assert_eq!(at(&im), Some(0.5));
        assert!(find_horizon(&im).unwrap().confidence > 0.5);
        // Shift everything down 12 rows: the horizon follows.
        let copy = im.clone();
        for y in 0..200 {
            for x in 0..400 {
                im.set(x, y, copy.get(x, y.saturating_sub(12)));
            }
        }
        assert_eq!(at(&im), Some(0.56));
        assert_eq!(find_horizon(&Image::new(400, 200, [40, 50, 90, 255])), None, "no step, no horizon");
        // Placed with that horizon, it lands on the middle row, with nothing to fill.
        let (sky, rep) = equirect(&im, 256, 128, 0.56, 180.0, 12.0);
        assert_eq!(rep.extended, [0, 0]);
        let (above, below) = (sky.get(100, 60), sky.get(100, 68));
        assert!(above[2] > 120 && below[2] < 60, "{above:?} {below:?}");
    }

    #[test]
    fn a_soft_horizon_is_found_by_its_line_not_by_the_glow_falling_off() {
        // A sunset over the sea: the sky brightens toward a glow that peaks above
        // the water line at 54%, the water darkens slowly below it with a faint
        // reflection, and the line itself is a thin, slightly darker band of
        // distant hills. The brightest fall is far below, in the water.
        let (w, h) = (512u32, 256u32);
        let mut im = Image::new(w, h, [0; 4]);
        let line = (0.54 * h as f32) as u32;
        for y in 0..h {
            let t = y as f32 / h as f32;
            let c = if y < line {
                let g = 0.35 + 0.25 * (t / 0.54);
                [g * 1.1, g * 0.8, g * 0.9]
            } else if y < line + 3 {
                [0.42, 0.33, 0.45]
            } else {
                let g = 0.55 - 0.45 * ((t - 0.54) / 0.46).powf(0.7);
                [g * 0.6, g * 0.7, g * 1.0]
            };
            for x in 0..w {
                let ripple = if y > line && (x / 7 + y) % 11 == 0 { 0.04 } else { 0.0 };
                im.set(x, y, [0, 1, 2].map(|i| ((c[i] + ripple).clamp(0.0, 1.0) * 255.0) as u8).into_iter().chain([255]).collect::<Vec<_>>().try_into().unwrap());
            }
        }
        let found = find_horizon(&im).unwrap();
        assert!((found.at - 0.54).abs() < 0.02, "{found:?}");
    }

    #[test]
    fn a_partial_span_is_extended_with_color_not_detail() {
        let (sky, rep) = equirect(&source(733, 314), 256, 128, 0.5, 90.0, 10.0);
        assert_eq!(rep.extended[0] + rep.extended[1], 64, "half the rows are extension");
        assert!(row_spread(&sky, 5) < 12.0, "extended rows are smooth: {}", row_spread(&sky, 5));
        assert!(super::super::img::wrap_error(&sky, true) < 0.03);
    }

    #[test]
    fn the_jpeg_decodes_and_fits_the_budget() {
        let (sky, _) = equirect(&source(733, 314), 512, 256, 0.5, 180.0, 12.0);
        let (jpg, q) = encode_budget(&sky, 12).unwrap();
        assert!(jpg.len() <= 12 * 1024 || q == 40, "{} bytes at q{q}", jpg.len());
        let back = Image::decode(&jpg).unwrap();
        assert_eq!((back.w, back.h), (512, 256));
        assert!(jpg.starts_with(&[0xff, 0xd8]));
    }
}
