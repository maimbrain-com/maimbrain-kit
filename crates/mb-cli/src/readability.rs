//! `mb shot --readability`: does the subject read against the scene?
//!
//! The page draws the shot's frame again without the HUD (`mb.isolate`),
//! and twice more "plain" (no retro look, fog, grade or film, brighter, so a
//! dark subject on dark ground still differs from it): with the subject's
//! node and without it in the camera's pass (it keeps its shadow and
//! reflection). Where those two differ is the subject's silhouette. From the
//! silhouette, the frame without the HUD (what the player sees, retro look
//! and all) and the shot itself:
//!
//! - **coverage**: the share of the screen the subject covers;
//! - **edge contrast**: at each point of its outline, the lightness (CIE L*,
//!   what's left in greyscale) of the subject just inside against the scene
//!   just outside, within `EDGE_R` logical pixels; the median, and the share
//!   of the outline that differs by at least `EDGE_OK`. The same in color
//!   (ΔE, CIE Lab) says whether it separates only by hue;
//! - **inside**: the subject's own tonal range (L* 10th–90th percentile),
//!   which is flat when banding or darkness hides its texture and shape;
//! - **HUD**: the share of the subject that the mb2d HUD draws over;
//! - **frame**: the whole frame's lightness range (5th–95th percentile),
//!   low and narrow in a dark, muddy scene.
//!
//! Thresholds were set on scenes that read and ones that don't (CHANGELOG,
//! 0.7.0): mb3d-demo's hero scene against its muddy variant, the retro
//! mechs, the koi pond and the PBR demo.

use crate::generate::img::Image;

/// Logical pixels on each side of the outline that count as "just inside" and "just outside".
const EDGE_R: i32 = 3;
/// A max-channel difference (0–255) between the two renders that counts as the subject.
const MASK_DIFF: u8 = 12;
/// L* difference at the outline that a glance separates (greyscale).
pub const EDGE_OK: f32 = 15.0;
/// Below this median edge L* difference the subject blends into its surroundings.
pub const EDGE_LOW: f32 = 12.0;
/// Less than this share of the outline at `EDGE_OK` or more: the silhouette is mostly lost.
pub const EDGE_SHARE_LOW: f32 = 0.35;
/// Coverage below which the subject is too small to read on a phone.
pub const COVERAGE_LOW: f32 = 0.006;
/// An inside L* range (p90 − p10) below this: one flat tone, no texture or form showing.
pub const INSIDE_FLAT: f32 = 12.0;
/// The HUD covering more than this share of the subject.
pub const HUD_OVER: f32 = 0.08;
/// A frame whose 95th-percentile L* is below this is dark.
pub const FRAME_DARK: f32 = 30.0;

/// What `measure` found.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// None when no subject was named (or it drew nothing): only the frame is measured.
    pub subject: Option<Subject>,
    /// The frame's L* at the 5th and 95th percentiles (without the HUD).
    pub frame: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    pub coverage: f32,
    pub edge_dl: f32,
    pub edge_share: f32,
    pub edge_de: f32,
    pub inside: [f32; 2],
    pub hud: f32,
}

/// sRGB (0–255) to CIE L*a*b* (D65).
fn lab(c: [u8; 4]) -> [f32; 3] {
    let lin = |u: u8| {
        let v = u as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn differs(a: [u8; 4], b: [u8; 4]) -> bool {
    (0..3).any(|i| a[i].abs_diff(b[i]) > MASK_DIFF)
}

fn percentile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// Measures one shot: `shown` is what the player sees, `clean` the same
/// frame without the HUD, `mask` the plain redraws with and without the
/// subject (None: no subject). All the same size; `scale` device pixels per
/// logical pixel.
pub fn measure(shown: &Image, clean: &Image, mask: Option<(&Image, &Image)>, scale: u32) -> Report {
    let scale = scale.max(1);
    let (w, h) = (clean.w / scale, clean.h / scale);
    let small = |i: &Image| if scale > 1 { i.resize(w, h) } else { i.clone() };
    let c = small(clean);
    let labs: Vec<[f32; 3]> = c.px.iter().map(|&p| lab(p)).collect();
    let mut ls: Vec<f32> = labs.iter().map(|l| l[0]).collect();
    let frame = [percentile(&mut ls, 0.05), percentile(&mut ls, 0.95)];
    let subject = mask.and_then(|(with, without)| {
        let (s, p, q) = (small(shown), small(with), small(without));
        let mask: Vec<bool> = p.px.iter().zip(&q.px).map(|(&x, &y)| differs(x, y)).collect();
        subject(&labs, &mask, w as i32, h as i32, |i| differs(s.px[i], c.px[i]))
    });
    Report { subject, frame }
}

fn subject(labs: &[[f32; 3]], mask: &[bool], w: i32, h: i32, hud_over: impl Fn(usize) -> bool) -> Option<Subject> {
    let n = mask.iter().filter(|&&m| m).count();
    // A few stray pixels (a glint, bloom) aren't a subject.
    if n < 16 {
        return None;
    }
    let at = |x: i32, y: i32| (y * w + x) as usize;
    let (mut dls, mut des, mut inside) = (Vec::new(), Vec::new(), Vec::new());
    let mut hud = 0;
    for y in 0..h {
        for x in 0..w {
            let i = at(x, y);
            if !mask[i] {
                continue;
            }
            inside.push(labs[i][0]);
            if hud_over(i) {
                hud += 1;
            }
            let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                let (u, v) = (x + dx, y + dy);
                u >= 0 && v >= 0 && u < w && v < h && !mask[at(u, v)]
            });
            if !edge {
                continue;
            }
            let (mut a, mut na, mut b, mut nb) = ([0.0f32; 3], 0.0f32, [0.0f32; 3], 0.0f32);
            for v in (y - EDGE_R).max(0)..=(y + EDGE_R).min(h - 1) {
                for u in (x - EDGE_R).max(0)..=(x + EDGE_R).min(w - 1) {
                    let j = at(u, v);
                    let (acc, cnt) = if mask[j] { (&mut a, &mut na) } else { (&mut b, &mut nb) };
                    for k in 0..3 {
                        acc[k] += labs[j][k];
                    }
                    *cnt += 1.0;
                }
            }
            if na == 0.0 || nb == 0.0 {
                continue;
            }
            let d: Vec<f32> = (0..3).map(|k| a[k] / na - b[k] / nb).collect();
            dls.push(d[0].abs());
            des.push((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
        }
    }
    if dls.is_empty() {
        return None;
    }
    let share = dls.iter().filter(|&&d| d >= EDGE_OK).count() as f32 / dls.len() as f32;
    Some(Subject {
        coverage: n as f32 / mask.len() as f32,
        edge_dl: percentile(&mut dls, 0.5),
        edge_share: share,
        edge_de: percentile(&mut des, 0.5),
        inside: [percentile(&mut inside.clone(), 0.1), percentile(&mut inside, 0.9)],
        hud: hud as f32 / n as f32,
    })
}

impl Report {
    /// One line of numbers.
    pub fn line(&self) -> String {
        let frame = format!("frame L* {:.0}–{:.0}", self.frame[0], self.frame[1]);
        match &self.subject {
            Some(s) => format!(
                "readability: subject {:.1}% of the screen · outline ΔL* {:.0} ({:.0}% ≥ {EDGE_OK:.0}) · ΔE {:.0} · inside L* {:.0}–{:.0} · HUD over {:.0}% · {frame}",
                s.coverage * 100.0,
                s.edge_dl,
                s.edge_share * 100.0,
                s.edge_de,
                s.inside[0],
                s.inside[1],
                s.hud * 100.0
            ),
            None => format!("readability: {frame} (no subject: name one with Node::mark_subject() or --subject)"),
        }
    }

    /// What to fix, worst first; `retro` when the retro look was on.
    pub fn warnings(&self, retro: bool) -> Vec<String> {
        let mut out = Vec::new();
        let dark = self.frame[1] < FRAME_DARK;
        let mut failed = false;
        if let Some(s) = &self.subject {
            if s.edge_dl < EDGE_LOW || s.edge_share < EDGE_SHARE_LOW {
                failed = true;
                let hue = if s.edge_de >= 2.0 * EDGE_OK { " It differs mostly in hue, which greyscale (and many players' eyes) loses." } else { "" };
                out.push(format!(
                    "the subject doesn't separate from what's around it: only {:.0}% of its outline differs in lightness by {EDGE_OK:.0}+ L* (median {:.0}).{hue} Light it against its background: a brighter sky or a darker one, a rim light, or a lighter/darker subject.",
                    s.edge_share * 100.0,
                    s.edge_dl
                ));
            }
            if s.coverage < COVERAGE_LOW {
                failed = true;
                out.push(format!("the subject covers {:.1}% of the screen: too small to read on a phone. Bring the camera closer or scale it up.", s.coverage * 100.0));
            }
            if s.inside[1] - s.inside[0] < INSIDE_FLAT {
                failed = true;
                out.push(format!(
                    "the subject is nearly one tone inside (L* {:.0}–{:.0}): its texture and form don't show. Light its front (key light, sky fill), and drop toon bands on a textured model.",
                    s.inside[0], s.inside[1]
                ));
            }
            if s.hud > HUD_OVER {
                out.push(format!("the HUD draws over {:.0}% of the subject.", s.hud * 100.0));
            }
        }
        if dark {
            failed = true;
            out.push(format!("the frame is dark: 95% of it is below L* {:.0}, so on a phone in daylight it reads as mud.", self.frame[1]));
        }
        if failed && retro {
            out.push(
                "the retro look is on, and may be the cause: few colors, dither and fog bands flatten a dark or detailed subject. Try the shot with it off (full resolution, crisp edges) — MB3D.md \"When the retro look hurts\"."
                    .into(),
            );
        }
        out
    }
}

/// The frame in greyscale with the subject's outline in magenta: how it
/// reads without color (`<shot>.readability.png`).
pub fn picture(clean: &Image, mask: Option<(&Image, &Image)>) -> Image {
    let mut out = clean.clone();
    for p in out.px.iter_mut() {
        let l = (lab(*p)[0] / 100.0 * 255.0).clamp(0.0, 255.0) as u8;
        *p = [l, l, l, 255];
    }
    if let Some((with, without)) = mask {
        let (w, h) = (clean.w as i32, clean.h as i32);
        let mask: Vec<bool> = with.px.iter().zip(&without.px).map(|(&x, &y)| differs(x, y)).collect();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let edge = mask[i] && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    let (u, v) = (x + dx, y + dy);
                    u >= 0 && v >= 0 && u < w && v < h && !mask[(v * w + u) as usize]
                });
                if edge {
                    out.px[i] = [255, 0, 255, 255];
                }
            }
        }
    }
    out
}

/// The node handle the game last named its subject with
/// (`Node::mark_subject`, a `subject:<handle>` mark) in these `mb.step` timings.
pub fn marked_subject(timings: &[serde_json::Value]) -> Option<u32> {
    timings.iter().rev().find_map(|t| {
        t["marks"].as_array()?.iter().rev().filter_map(|m| m.as_str()?.strip_prefix(SUBJECT_MARK)?.parse().ok()).next()
    })
}

/// The mark prefix `Node::mark_subject` uses.
pub const SUBJECT_MARK: &str = "subject:";

#[cfg(test)]
mod tests {
    use super::*;

    /// A w×h image of `bg` with a `fg` square of side `s` in the middle.
    fn scene(w: u32, h: u32, bg: [u8; 4], fg: Option<[u8; 4]>, s: u32) -> Image {
        let mut img = Image::new(w, h, bg);
        if let Some(fg) = fg {
            for y in (h - s) / 2..(h + s) / 2 {
                for x in (w - s) / 2..(w + s) / 2 {
                    img.set(x, y, fg);
                }
            }
        }
        img
    }

    #[test]
    fn lightness_is_cie_l() {
        assert!((lab([255, 255, 255, 255])[0] - 100.0).abs() < 0.1);
        assert!(lab([0, 0, 0, 255])[0].abs() < 0.1);
        assert!((lab([119, 119, 119, 255])[0] - 50.0).abs() < 1.0);
    }

    #[test]
    fn a_light_subject_on_a_dark_ground_reads() {
        let bg = [30, 30, 40, 255];
        let shown = scene(100, 160, bg, Some([200, 190, 170, 255]), 30);
        let behind = scene(100, 160, bg, None, 30);
        let r = measure(&shown, &shown, Some((&shown, &behind)), 1);
        let s = r.subject.as_ref().unwrap();
        assert!((s.coverage - 900.0 / 16000.0).abs() < 1e-3, "{s:?}");
        assert!(s.edge_dl > 50.0 && s.edge_share > 0.9, "{s:?}");
        assert_eq!(s.hud, 0.0);
        let w = r.warnings(false);
        assert!(w.iter().all(|w| !w.contains("separate")), "{w:?}");
    }

    #[test]
    fn a_dark_subject_on_dark_ground_does_not_and_retro_is_named() {
        let bg = [28, 26, 34, 255];
        let shown = scene(100, 160, bg, Some([50, 42, 34, 255]), 30);
        let behind = scene(100, 160, bg, None, 30);
        let r = measure(&shown, &shown, Some((&shown, &behind)), 1);
        let w = r.warnings(true);
        assert!(w.iter().any(|w| w.contains("doesn't separate")), "{w:?}");
        assert!(w.iter().any(|w| w.contains("frame is dark")), "{w:?}");
        assert!(w.iter().any(|w| w.contains("retro look is on")), "{w:?}");
        assert!(!r.warnings(false).iter().any(|w| w.contains("retro")));
    }

    #[test]
    fn hue_alone_is_called_out() {
        // Red on green of about the same lightness.
        let bg = [60, 130, 60, 255];
        let shown = scene(100, 160, bg, Some([200, 70, 70, 255]), 30);
        let behind = scene(100, 160, bg, None, 30);
        let s = measure(&shown, &shown, Some((&shown, &behind)), 1).subject.unwrap();
        assert!(s.edge_dl < EDGE_LOW && s.edge_de > 2.0 * EDGE_OK, "{s:?}");
        let r = Report { subject: Some(s), frame: [40.0, 60.0] };
        assert!(r.warnings(false).iter().any(|w| w.contains("hue")));
    }

    #[test]
    fn the_hud_over_the_subject_is_counted() {
        let bg = [30, 30, 40, 255];
        let clean = scene(100, 160, bg, Some([220, 220, 220, 255]), 40);
        let behind = scene(100, 160, bg, None, 40);
        let mut shown = clean.clone();
        for y in 60..80 {
            for x in 0..100 {
                shown.set(x, y, [255, 0, 0, 255]);
            }
        }
        let s = measure(&shown, &clean, Some((&clean, &behind)), 1).subject.unwrap();
        assert!((s.hud - 0.5).abs() < 0.01, "{s:?}");
    }

    #[test]
    fn without_a_subject_only_the_frame_is_measured() {
        let img = scene(40, 40, [10, 10, 10, 255], None, 0);
        let r = measure(&img, &img, None, 2);
        assert!(r.subject.is_none() && r.frame[1] < 5.0);
        assert!(r.line().contains("no subject"));
    }

    #[test]
    fn the_subject_comes_from_the_last_mark() {
        let t = vec![serde_json::json!({ "marks": ["subject:65537"] }), serde_json::json!({ "marks": ["boom", "subject:131074"] }), serde_json::json!({})];
        assert_eq!(marked_subject(&t), Some(131074));
        assert_eq!(marked_subject(&t[..1]), Some(65537));
        assert_eq!(marked_subject(&[]), None);
    }
}
