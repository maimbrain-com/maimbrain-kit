//! Walk cycles: the prompt for a walk/run sheet's poses, a check that a
//! generated sheet's feet really step (generated walk cycles often keep the
//! feet nearly still while the body bobs, and a character drawn from them
//! slides), an onion-skin preview to see it, and the facts for a `--walker`
//! sheet (a feetless body and a foot for `maimbrain::walk`).

use serde_json::{Value, json};

use super::img::Image;

/// The gait a frames asset is about, from its name and description: "walk",
/// "run" or "creep" (None for anything else: a flap, a hop, a blink).
pub fn gait_word(name: &str, subject: &str) -> Option<&'static str> {
    let text = format!("{name} {subject}").to_lowercase();
    let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |stems: &[&str]| words.iter().any(|w| stems.iter().any(|s| w.starts_with(s)));
    if has(&["creep", "stalk", "sneak", "tiptoe", "prowl", "lurk", "shuffl", "limp", "hobbl"]) {
        Some("creep")
    } else if has(&["run", "sprint", "jog", "dash", "gallop", "charg", "flee"]) {
        Some("run")
    } else if has(&["walk", "stride", "strid", "march", "step", "pace", "trot", "stroll", "wander", "amble", "gait"]) {
        Some("walk")
    } else {
        None
    }
}

/// The poses of a walk/run sheet of `n` frames, for the prompt: one full
/// cycle of two steps, so it loops. None when the description already lays
/// the frames out itself ("frame 1 …", "1 front leg …").
pub fn cycle_prompt(gait: &str, n: u32, subject: &str) -> Option<String> {
    let s = subject.to_lowercase();
    if s.contains("frame 1") || s.contains("1 ") && s.contains("2 ") && s.contains("3 ") {
        return None;
    }
    if n < 2 || !n.is_multiple_of(2) {
        return Some(format!(
            "The {n} frames are one full {gait} cycle of two steps in side view, each frame a clearly different leg position, the feet planted on one flat ground line and the legs wide apart when a foot touches down."
        ));
    }
    let per = (n / 2) as usize;
    let poses: &[&str] = match (gait, per) {
        ("run", 1) => &["the front leg reaching far forward, the back leg pushing off far behind"],
        ("run", 2) => &["the front leg reaching far forward to land, the back leg pushing off far behind", "both feet off the ground, knees tucked, the body at its highest"],
        ("run", 3) => &["contact: the front leg reaching far forward to land, the back leg far behind", "down: the landed leg bent under the body, the other knee swinging forward", "flight: both feet off the ground, the body at its highest"],
        ("run", _) => &[
            "contact: the front leg reaching far forward to land, the back leg far behind",
            "down: the landed leg bent under the body, the body at its lowest",
            "push-off: the landed leg straightening behind, the other knee driving up in front",
            "flight: both feet off the ground, the body at its highest",
        ],
        (_, 1) => &["contact: the legs wide apart, the front heel down, the back toe on the ground"],
        (_, 2) => &["contact: the legs wide apart, the front heel down, the back foot on its toe", "passing: the back leg swinging past the planted one, its foot lifted, the body at its highest"],
        (_, 3) => &["contact: the legs wide apart, the front heel down, the back foot on its toe", "down: the front foot flat and the knee bent, the body at its lowest", "passing: the back leg swinging past the planted one, its foot lifted, the body at its highest"],
        (_, _) => &[
            "contact: the legs wide apart, the front heel down, the back foot on its toe",
            "down: the front foot flat and the knee bent, the body at its lowest",
            "passing: the back leg swinging past the planted one, its foot lifted",
            "up: the planted leg straight and pushing off, the swinging foot reaching forward, the body at its highest",
        ],
    };
    let list: Vec<String> = (0..per)
        .map(|i| match poses.get(i) {
            Some(p) => format!("{} {p}", i + 1),
            None => format!("{} an in-between leading into the next step", i + 1),
        })
        .collect();
    let rest = if per == 1 { format!("{n} the same with the legs swapped") } else { format!("{}–{n} the same poses with the legs swapped (the other foot steps)", per + 1) };
    let list = [list.join("; "), rest];
    let gait_name = if gait == "creep" { "slow creeping walk" } else { gait };
    Some(format!(
        "The {n} frames are one full {gait_name} cycle of two steps in side view, so the last frame leads back into the first: {}. The feet are clearly drawn and clearly apart in every contact frame, each frame's legs are visibly different from the next, and every planted foot stands on the same flat ground line.",
        list.join("; ")
    ))
}

/// How much a walk sheet's feet move.
#[derive(Clone, Debug, PartialEq)]
pub struct WalkReport {
    pub frames: u32,
    /// Mean change of the feet (the bottom quarter of the figure) from frame
    /// to frame, the body's sway aligned away: 0 still, 1 nothing in common.
    pub feet_change: f32,
    /// The same for the upper body.
    pub body_change: f32,
    /// Widest over narrowest feet-band span: legs apart vs passing.
    pub spread_ratio: f32,
    /// How far the feet move from the last frame back to the first, over
    /// the median of the other frame-to-frame moves (about 1 for a loop).
    pub loop_gap: f32,
}

fn cells(sheet: &Image, n: u32, cols: u32, w: u32, h: u32) -> Vec<Image> {
    (0..n).map(|i| sheet.crop((i % cols.max(1)) * w, (i / cols.max(1)) * h, w, h)).collect()
}

fn solid(im: &Image, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as u32) < im.w && (y as u32) < im.h && im.px[(y as u32 * im.w + x as u32) as usize][3] >= 128
}

/// 1 − IoU of the rows `y0..=y1` of `a` shifted by `dx` against `b`.
fn change(a: &Image, b: &Image, y0: i32, y1: i32, dx: i32) -> f32 {
    let (mut and, mut or) = (0usize, 0usize);
    for y in y0..=y1 {
        for x in 0..b.w as i32 {
            let (p, q) = (solid(a, x - dx, y), solid(b, x, y));
            and += (p && q) as usize;
            or += (p || q) as usize;
        }
    }
    if or == 0 { 0.0 } else { 1.0 - and as f32 / or as f32 }
}

/// Mean horizontal distance (texels) from each solid texel in rows
/// `y0..=y1` of one image to the nearest in the same row of the other, both
/// ways (`a` shifted by `dx`): how far the shapes moved, without saturating.
fn shift_dist(a: &Image, b: &Image, y0: i32, y1: i32, dx: i32) -> f32 {
    let cap = b.w as f32 / 2.0;
    let (mut sum, mut count) = (0.0f32, 0usize);
    for (p, q, dp, dq) in [(a, b, dx, 0), (b, a, 0, dx)] {
        for y in y0..=y1 {
            let other: Vec<i32> = (0..b.w as i32).filter(|&x| solid(q, x - dq, y)).collect();
            for x in (0..b.w as i32).filter(|&x| solid(p, x - dp, y)) {
                let d = other.iter().map(|o| (o - x).abs()).min().map_or(cap, |d| (d as f32).min(cap));
                sum += d;
                count += 1;
            }
        }
    }
    if count == 0 { 0.0 } else { sum / count as f32 }
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() { 0.0 } else { v[v.len() / 2] }
}

/// Measures a frames sheet (`n` cells of `w`×`h`, `cols` across). None if
/// it has under 3 frames or nothing drawn.
pub fn measure(sheet: &Image, n: u32, cols: u32, w: u32, h: u32) -> Option<WalkReport> {
    if n < 3 {
        return None;
    }
    let cs = cells(sheet, n, cols, w, h);
    let (mut top, mut bottom) = (i32::MAX, i32::MIN);
    for c in &cs {
        if let Some((_, y, _, bh)) = c.content_bbox(128) {
            top = top.min(y as i32);
            bottom = bottom.max((y + bh) as i32 - 1);
        }
    }
    if bottom - top < 8 {
        return None;
    }
    let tall = bottom - top + 1;
    let feet = bottom - (tall as f32 * 0.25).ceil() as i32;
    let body_end = top + (tall as f32 * 0.55) as i32;
    let reach = (w as i32 / 6).max(1);
    let pair = |a: &Image, b: &Image| -> (f32, f32, f32) {
        // Align the upper bodies (the art's horizontal sway), then compare.
        let dx = (-reach..=reach).min_by(|p, q| change(a, b, top, body_end, *p).partial_cmp(&change(a, b, top, body_end, *q)).unwrap().then(p.abs().cmp(&q.abs()))).unwrap_or(0);
        (change(a, b, feet, bottom, dx), change(a, b, top, body_end, dx), shift_dist(a, b, feet, bottom, dx))
    };
    let pairs: Vec<(f32, f32, f32)> = (0..n as usize).map(|i| pair(&cs[i], &cs[(i + 1) % n as usize])).collect();
    let mean = |f: fn(&(f32, f32, f32)) -> f32| pairs.iter().map(f).sum::<f32>() / pairs.len() as f32;
    let spans: Vec<f32> = cs
        .iter()
        .map(|c| {
            let cols: Vec<i32> = (0..c.w as i32).filter(|&x| (feet..=bottom).any(|y| solid(c, x, y))).collect();
            match (cols.first(), cols.last()) {
                (Some(a), Some(b)) => (b - a + 1) as f32,
                _ => 0.0,
            }
        })
        .collect();
    let lo = spans.iter().cloned().fold(f32::MAX, f32::min).max(1.0);
    let hi = spans.iter().cloned().fold(0.0, f32::max);
    let inner = median(pairs[..pairs.len() - 1].iter().map(|p| p.2).collect());
    let floor = (tall as f32 * 0.01).max(0.5);
    Some(WalkReport {
        frames: n,
        feet_change: mean(|p| p.0),
        body_change: mean(|p| p.1),
        spread_ratio: hi / lo,
        loop_gap: pairs[pairs.len() - 1].2 / inner.max(floor),
    })
}

/// Past this, the last frame doesn't lead back into the first.
pub const LOOP_GAP: f32 = 2.5;

/// Below this mean feet change, a walk sheet's feet barely move.
pub const STILL_FEET: f32 = 0.3;

impl WalkReport {
    /// Warnings for a sheet meant as a `gait` cycle.
    pub fn warnings(&self, gait: &str, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if self.feet_change < STILL_FEET || self.spread_ratio < 1.15 {
            out.push(format!(
                "this {gait} cycle barely moves its feet (they change {:.0}% frame to frame, the legs' spread {:.2}×): drawn in a game it slides. Use a feetless body and a foot stepped by distance walked instead: `mb art frames <game> {name}_walker \"<the character>\" --walker` and `maimbrain::walk::Walker` (assets.md \"Walking characters\"), or regenerate",
                self.feet_change * 100.0,
                self.spread_ratio
            ));
        }
        if self.frames >= 3 && self.loop_gap > LOOP_GAP {
            out.push(format!(
                "the cycle doesn't loop: the last frame jumps back to the first ({:.1}× the other frame-to-frame changes); regenerate, or drop the odd frame",
                self.loop_gap
            ));
        }
        out
    }

    pub fn facts(&self) -> Value {
        let r = |v: f32| (v * 100.0).round() / 100.0;
        json!({
            "feet_change": r(self.feet_change),
            "body_change": r(self.body_change),
            "spread_ratio": r(self.spread_ratio),
            "loop_gap": r(self.loop_gap),
        })
    }
}

/// Every frame drawn over the others, each a shade from cool (first) to warm
/// (last), on a light ground with the feet band marked: a walk whose feet
/// step shows a fan of legs; one that slides shows one blurred pair.
pub fn onion(sheet: &Image, n: u32, cols: u32, w: u32, h: u32) -> Image {
    let cs = cells(sheet, n, cols, w, h);
    let mut out = Image::new(w, h, [236, 236, 230, 255]);
    let k = 1.0 / n.max(1) as f32;
    for (i, c) in cs.iter().enumerate() {
        let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
        let tint = [(40.0 + 200.0 * t) as u8, 70, (230.0 - 190.0 * t) as u8];
        for y in 0..h {
            for x in 0..w {
                let p = c.get(x, y);
                if p[3] >= 128 {
                    let l = (p[0] as f32 * 0.3 + p[1] as f32 * 0.55 + p[2] as f32 * 0.15) / 255.0;
                    let o = out.get(x, y);
                    let a = (0.35 + k).min(0.8);
                    let mix = |d: u8, s: u8| (d as f32 * (1.0 - a) + s as f32 * (0.55 + 0.45 * l) * a) as u8;
                    out.set(x, y, [mix(o[0], tint[0]), mix(o[1], tint[1]), mix(o[2], tint[2]), 255]);
                }
            }
        }
    }
    if let Some(bottom) = cs.iter().filter_map(|c| c.content_bbox(128).map(|b| b.1 + b.3)).max() {
        let top = cs.iter().filter_map(|c| c.content_bbox(128).map(|b| b.1)).min().unwrap_or(0);
        let feet = bottom.saturating_sub(((bottom - top) as f32 * 0.25).ceil() as u32);
        for x in (0..w).step_by(2) {
            out.set(x, feet.min(h - 1), [200, 60, 60, 255]);
            out.set(x, bottom.min(h - 1), [60, 60, 60, 255]);
        }
    }
    out.scale_nearest(256u32.div_ceil(w.max(h).max(1)).max(1))
}

/// Next to a walk sheet's preview (`art/preview/<name>.png`), its onion
/// skin (`<name>.walk.png`), if the run measured its feet.
pub fn write_onion(preview: &std::path::Path, im: &Image, frames: u32, cols: u32, size: [u32; 2], facts: &Value) -> Result<(), String> {
    if facts.get("walk").is_none() {
        return Ok(());
    }
    let op = preview.with_extension("walk.png");
    std::fs::write(&op, onion(im, frames, cols.max(1), size[0], size[1]).encode_png()).map_err(|e| format!("{}: {e}", op.display()))?;
    eprintln!("  feet: {} (every frame drawn over the others: legs that step fan out; one blurred pair slides)", op.display());
    Ok(())
}

/// A `--walker` sheet (cell 0 the body without feet, cell 1 a foot): the
/// drawn rects and the Rust line that draws it with `maimbrain::walk`.
pub fn walker_facts(sheet: &Image, cols: u32, w: u32, h: u32, out: &str) -> Option<(Value, String, Vec<String>)> {
    let cs = cells(sheet, 2, cols, w, h);
    let rect = |i: usize| cs[i].content_bbox(128).map(|(x, y, bw, bh)| [(i as u32 % cols) * w + x, (i as u32 / cols) * h + y, bw, bh]);
    let (body, foot) = (rect(0)?, rect(1)?);
    let mut warnings = Vec::new();
    if foot[3] * 2 > body[3] || foot[2] * 2 > body[2] * 3 {
        warnings.push(format!(
            "cell 1 (the foot) is {}×{} next to a {}×{} body: it should be one small foot; regenerate",
            foot[2], foot[3], body[2], body[3]
        ));
    }
    // Height in game units at 2 texels per unit, feet included.
    let tall = (body[3] as f32 + 0.5 * foot[3] as f32) * 0.5;
    // Named, so `mb shot` checks it against props and tap areas like a ground::Figure.
    let name = std::path::Path::new(out).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rust = format!(
        "maimbrain::walk: `let look = WalkSprite::new(Image::new(Asset::load(\"{out}\"))?, [{}.0, {}.0, {}.0, {}.0], [{}.0, {}.0, {}.0, {}.0]).named({name:?});` and `let mut who = Walker::new({tall:.0}.0).at(x);` (Walker::creeping / running for a stalker or a runner); each update `who.drive(dt, speed)` (or `who.update(dt, x)` if your sim moves it), each render `look.draw(&who, ground_y, 0xffffffff)`",
        body[0], body[1], body[2], body[3], foot[0], foot[1], foot[2], foot[3]
    );
    Some((json!({ "body": body, "foot": foot, "height_units": (tall * 10.0).round() / 10.0 }), rust, warnings))
}

/// `mb art check`: a walk/run sheet made before this check (its sidecar
/// has no `walk` facts) measured now, and its warnings, short.
pub fn check_notes(recipe: &Value, result: &Value, png: &[u8]) -> Vec<String> {
    if recipe["kind"] != "frames" || recipe["walker"] == true || result.get("walk").is_some() {
        return Vec::new();
    }
    let s = |k: &str| recipe[k].as_str().unwrap_or("");
    let u = |v: &Value| v.as_u64().unwrap_or(0) as u32;
    let Some(gait) = gait_word(s("name"), s("subject")) else { return Vec::new() };
    let (n, cols, w, h) = (u(&recipe["frames"]), u(&recipe["cols"]).max(1), u(&recipe["size"][0]), u(&recipe["size"][1]));
    let Ok(sheet) = Image::decode(png) else { return Vec::new() };
    if w == 0 || h == 0 || sheet.w < cols.min(n) * w || sheet.h < n.div_ceil(cols) * h {
        return Vec::new();
    }
    let Some(r) = measure(&sheet, n, cols, w, h) else { return Vec::new() };
    r.warnings(gait, s("name"))
        .into_iter()
        .map(|w| {
            let head = w.split(':').next().unwrap_or(&w);
            let fix = if w.contains("barely moves") { " (mb art frames --walker + maimbrain::walk)" } else { "" };
            format!("walk check: {head}{fix}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INK: [u8; 4] = [40, 30, 30, 255];

    fn fill(im: &mut Image, x0: i32, y0: i32, w: i32, h: i32) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if x >= 0 && y >= 0 && (x as u32) < im.w && (y as u32) < im.h {
                    im.set(x as u32, y as u32, INK);
                }
            }
        }
    }

    /// A sheet of `n` 64×64 cells: a body bobbing over two legs whose feet sit
    /// `apart(i)` apart (0 = feet together every frame).
    fn sheet(n: u32, apart: impl Fn(u32) -> i32, sway: impl Fn(u32) -> i32) -> Image {
        let mut s = Image::new(64 * n, 64, [0; 4]);
        for i in 0..n {
            let (ox, cx) = (64 * i as i32, 32 + sway(i));
            let bob = (i % 2) as i32;
            fill(&mut s, ox + cx - 10, 8 - bob, 20, 30); // body
            let a = apart(i);
            for (k, fx) in [cx - a / 2, cx + a / 2].into_iter().enumerate() {
                let side = if k == 0 { -1 } else { 1 };
                // A leg from the hip down to the foot, and the foot.
                for y in 38 - bob..56 {
                    let t = (y - 38) as f32 / 18.0;
                    let x = (cx + side * 4) as f32 + (fx - cx - side * 4) as f32 * t;
                    fill(&mut s, ox + x as i32 - 2, y, 4, 1);
                }
                fill(&mut s, ox + fx - 3, 56, 8, 4);
            }
        }
        s
    }

    #[test]
    fn gait_words() {
        assert_eq!(gait_word("girl_walk", "a girl in a nightgown"), Some("walk"));
        assert_eq!(gait_word("witch", "the witch creeping forward, hunched"), Some("creep"));
        assert_eq!(gait_word("hero_run", "the bun hero running fast"), Some("run"));
        assert_eq!(gait_word("flap", "the moth flapping its wings"), None);
        assert_eq!(gait_word("bird", "a bird with a runny nose"), Some("run"), "false positives only add a check");
    }

    #[test]
    fn the_cycle_prompt_lists_poses_for_two_steps() {
        let p = cycle_prompt("walk", 8, "a girl").unwrap();
        assert!(p.contains("1 contact") && p.contains("4 up") && p.contains("5–8 the same poses with the legs swapped") && p.contains("leads back"), "{p}");
        let r = cycle_prompt("run", 4, "a bun").unwrap();
        assert!(r.contains("2 both feet off the ground") && r.contains("3–4 the same"), "{r}");
        assert!(cycle_prompt("walk", 4, "A 4-frame run cycle: frame 1 the front leg forward").is_none());
        assert!(cycle_prompt("walk", 5, "a girl").unwrap().contains("two steps"));
    }

    #[test]
    fn a_sheet_whose_feet_barely_move_is_caught() {
        // Feet together in every frame; only the body bobs and sways: a slide.
        let still = sheet(6, |_| 6, |i| (i % 3) as i32 - 1);
        let r = measure(&still, 6, 6, 64, 64).unwrap();
        assert!(r.feet_change < STILL_FEET, "{r:?}");
        assert!(!r.warnings("walk", "girl").is_empty());
        // Legs opening and closing (contact, passing, contact, passing…): a walk.
        let walk = sheet(6, |i| [36, 18, 4, 36, 18, 4][i as usize], |_| 0);
        let r = measure(&walk, 6, 6, 64, 64).unwrap();
        assert!(r.feet_change > STILL_FEET && r.spread_ratio > 2.0, "{r:?}");
        assert!(r.warnings("walk", "girl").is_empty(), "{:?}", r.warnings("walk", "girl"));
    }

    #[test]
    fn a_cycle_that_doesnt_loop_is_caught() {
        // Feet opening steadily over 6 frames, then snapping back: no loop.
        let open = sheet(6, |i| 4 + 7 * i as i32, |_| 0);
        let r = measure(&open, 6, 6, 64, 64).unwrap();
        assert!(r.loop_gap > LOOP_GAP, "{r:?}");
        assert!(r.warnings("walk", "girl").iter().any(|w| w.contains("doesn't loop")));
    }

    #[test]
    fn the_runner_kits_generated_run_cycle_passes() {
        // The runner kit: under .claude/skills in the monorepo, skills/ in the kit.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = [".claude/skills", "skills"].iter().map(|s| root.join(s).join("maimbrain-game/kits/runner/assets")).find(|d| d.is_dir()).expect("the runner kit");
        let sheet = Image::decode(&std::fs::read(dir.join("hero_run.png")).unwrap()).unwrap();
        let r = measure(&sheet, 4, 4, sheet.w / 4, sheet.h).unwrap();
        assert!(r.warnings("run", "hero_run").is_empty(), "{r:?}");
        // The same bun with its first frame's feet only nudged a texel or three
        // each frame (the body bobbing): what a sliding generated walk looks like.
        let (w, h) = (sheet.w / 4, sheet.h);
        let first = sheet.crop(0, 0, w, h);
        let (_, top, _, bh) = first.content_bbox(128).unwrap();
        let feet = top + bh - bh / 4;
        let mut slid = Image::new(4 * w, h, [0; 4]);
        for i in 0..4u32 {
            let nudge = [0i32, 1, 3, 1][i as usize];
            for y in 0..h {
                for x in 0..w {
                    let sx = if y >= feet { x as i32 - nudge } else { x as i32 };
                    let sy = if y < feet { y + (i % 2) } else { y };
                    if sx >= 0 && sy < h {
                        slid.set(i * w + x, y, first.get(sx as u32, sy));
                    }
                }
            }
        }
        let r = measure(&slid, 4, 4, w, h).unwrap();
        assert!(r.warnings("run", "hero_run").iter().any(|w| w.contains("barely moves its feet")), "{r:?}");
    }

    #[test]
    fn walker_sheet_facts() {
        let mut s = Image::new(128, 64, [0; 4]);
        fill(&mut s, 20, 10, 24, 44);
        fill(&mut s, 64 + 26, 52, 12, 8);
        let (facts, rust, warnings) = walker_facts(&s, 2, 64, 64, "assets/girl.png").unwrap();
        assert_eq!(facts["body"], json!([20, 10, 24, 44]));
        assert_eq!(facts["foot"], json!([90, 52, 12, 8]));
        assert!(rust.contains("WalkSprite::new") && rust.contains("Walker::new(24.0)"), "{rust}");
        assert!(warnings.is_empty());
        let o = onion(&sheet(4, |i| 4 + 10 * (i as i32 % 2), |_| 0), 4, 4, 64, 64);
        assert_eq!(o.w, 256);
    }
}
