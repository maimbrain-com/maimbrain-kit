//! `mb shot`'s grounding check: do the figures a game stands on its floors
//! (`maimbrain::ground::Figure`) stand on them, on screen, whole, with a
//! contact shadow? The title and game-over cards are the most shared
//! screens, and a character standing in mid-air or cut off at the knees
//! there is what a player sees first.
//!
//! While `mb shot` runs a game with the debug parameter `mb.ground` (by
//! default; `--no-grounding` leaves it off), the SDK reports every figure it
//! draws and every floor line a painting puts on screen with `mb_mark`
//! (`ground:` records, packed several to a mark; sdk/maimbrain/src/ground.rs).
//! After each shot this reads the shot frame's records and:
//! - a figure whose feet are above the floor line under it (a [`Floor`]
//!   the game marked: `Backdrop::draw`/`floor_at`, `ground::floor_line`) is
//!   floating;
//! - a figure whose feet are below the screen's bottom edge, or whose src
//!   rect cuts above its feet, is cut off;
//! - one standing (not lifted) with no contact shadow is flagged;
//! - with no floor marked under a figure, the shot's pixels beside it are
//!   searched for a floor line: one well below its feet and none at them
//!   looks like floating; nothing at all and no shadow is standing on
//!   nothing;
//! - two figures on one layer whose bodies overlap: one drawn in front of
//!   another whose feet are nearer (lower on the screen) is in the wrong
//!   order; two on the same floor line, one standing in the other's
//!   footprint or overlapping it by more than a quarter of a body, look
//!   tangled together (stop half a body short: `ground::stop_short`);
//! - a tap area (`ground::tap_area`) named like a figure that misses part
//!   of it (its head: people tap faces), and one under 44 units across.

use serde_json::Value;

use crate::generate::img::{Image, oklab};

/// What the SDK's records start with (`ground::MARK`).
pub const MARK: &str = "ground:";
/// The debug parameter that turns the reports on (`ground::PARAM`).
pub const PARAM: &str = "mb.ground";
/// The debug parameter that draws tap areas over the frame (`ground::TAPS_PARAM`).
pub const TAPS_PARAM: &str = "mb.taps";

/// A figure as drawn, in logical units on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    pub name: String,
    pub l: f32,
    pub t: f32,
    pub r: f32,
    /// Where its feet are (the floor it was stood on).
    pub b: f32,
    pub shadow: bool,
    /// Its src rect ends above its feet.
    pub cropped: bool,
    /// Drawn above its floor on purpose (`Figure::lift`: a jump, a hover), its shadow on the floor.
    pub lifted: bool,
    /// Its feet's middle, and how wide they are (its footprint).
    pub fx: f32,
    pub base: f32,
    /// Its depth layer (`Figure::layer`; 0 by default).
    pub layer: i8,
}

/// A tap target the game declared (`ground::tap_area`), on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Tap {
    pub name: String,
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

/// A floor line a painting (or the game) put on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floor {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ground {
    /// In the order they were drawn.
    pub figures: Vec<Figure>,
    pub floors: Vec<Floor>,
    pub taps: Vec<Tap>,
    /// Records that didn't fit in the frame's marks.
    pub more: u32,
}

/// The `ground:` records among a frame's marks: the SDK cuts one stream of
/// records into marks, so they're joined in order before they're split.
pub fn parse<'a>(marks: impl IntoIterator<Item = &'a str>) -> Ground {
    let mut g = Ground::default();
    let stream: String = marks.into_iter().filter_map(|m| m.strip_prefix(MARK)).collect();
    for rec in stream.split(';').filter(|r| !r.is_empty()) {
        if let Some(n) = rec.strip_prefix('+') {
            g.more += n.parse().unwrap_or(0);
        } else if let Some(f) = rec.strip_prefix('F') {
            let v: Vec<f32> = f.split(',').filter_map(|x| x.parse().ok()).collect();
            if let [y, x0, x1] = v[..] {
                g.floors.push(Floor { y, x0, x1 });
            }
        } else if let Some(f) = rec.strip_prefix('f') {
            let parts: Vec<&str> = f.split(',').collect();
            if parts.len() != 6 && parts.len() != 8 {
                continue;
            }
            let v: Vec<f32> = parts[1..].iter().filter_map(|x| x.parse().ok()).collect();
            if v.len() != parts.len() - 1 {
                continue;
            }
            let (l, t, r, b, flags) = (v[0], v[1], v[2], v[3], v[4] as u32);
            let (fx, base) = if v.len() == 7 { (v[5], v[6]) } else { ((l + r) / 2.0, 0.0) };
            let layer = match (flags >> 3) & 7 {
                0 => 0,
                n => n as i8 - 4,
            };
            g.figures.push(Figure { name: parts[0].to_string(), l, t, r, b, shadow: flags & 1 != 0, cropped: flags & 2 != 0, lifted: flags & 4 != 0, fx, base, layer });
        } else if let Some(f) = rec.strip_prefix('t') {
            let parts: Vec<&str> = f.split(',').collect();
            let v: Vec<f32> = parts.iter().skip(1).filter_map(|x| x.parse().ok()).collect();
            if let [l, t, r, b] = v[..] {
                g.taps.push(Tap { name: parts[0].to_string(), l, t, r, b });
            }
        }
    }
    g
}

/// The `ground:` records of the last frame in `mb.step` timings (the shot's).
pub fn of_last_frame(timings: &[Value]) -> Ground {
    let marks = timings.last().and_then(|t| t["marks"].as_array()).cloned().unwrap_or_default();
    parse(marks.iter().filter_map(Value::as_str))
}

/// Whether a mark is one of these reports (other tools skip them).
pub fn is_ground(mark: &str) -> bool {
    mark.starts_with(MARK)
}

/// Feet this far above the floor line (units) float.
const FLOAT: f32 = 2.0;

/// The check for one shot: summary lines (for --grounding) and warnings.
/// `shot` is the screenshot, `size` the game's logical size.
pub fn check(g: &Ground, shot: &Image, size: [u32; 2]) -> (Vec<String>, Vec<String>) {
    let (w, h) = (size[0] as f32, size[1] as f32);
    let mut warnings = Vec::new();
    let mut seen = Vec::new();
    for f in &g.figures {
        if f.r < 0.0 || f.l > w || f.b < 0.0 || f.t > h {
            continue;
        }
        seen.push(f.name.clone());
        let n = &f.name;
        if f.cropped {
            warnings.push(format!("{n} is drawn from a src rect that ends above its feet: on screen it's cut off (draw the whole cell)"));
        }
        if f.b > h + 1.0 {
            warnings.push(format!("{n}'s feet are {:.0} units below the bottom of the screen: it's cut off (stand it on a floor inside the screen)", f.b - h));
            continue;
        }
        if f.t < -1.0 {
            warnings.push(format!("{n}'s top is {:.0} units above the top of the screen: it's cut off", -f.t));
        }
        let cx = (f.l + f.r) / 2.0;
        let under = g.floors.iter().filter(|fl| cx >= fl.x0 - 1.0 && cx <= fl.x1 + 1.0).min_by(|a, b| (a.y - f.b).abs().total_cmp(&(b.y - f.b).abs()));
        let mut floating = false;
        match under {
            _ if f.lifted => {}
            Some(fl) => {
                if fl.y - f.b > FLOAT {
                    floating = true;
                    warnings.push(format!(
                        "{n} floats {:.0} units above the floor (its feet at y {:.0}, the painting's floor line at {:.0}): stand it on the floor `Backdrop::draw` returns",
                        fl.y - f.b,
                        f.b,
                        fl.y
                    ));
                }
            }
            None => match look_for_floor(shot, size, f) {
                Look::Below(d, y) => {
                    floating = true;
                    warnings.push(format!(
                        "{n} looks like it floats {d:.0} units above a floor line (its feet at y {:.0}, a level edge beside it at {y:.0}); no floor is marked there: draw the painting with ground::Backdrop (or mark it with ground::floor_line) and stand {n} on its floor",
                        f.b
                    ));
                }
                Look::Nothing if !f.shadow => {
                    floating = true;
                    warnings.push(format!("{n} stands on nothing visible: no floor line at its feet (y {:.0}) and no contact shadow", f.b));
                }
                _ => {}
            },
        }
        if !f.shadow && !floating {
            warnings.push(format!("{n} has no contact shadow, so it doesn't look like it stands on the floor (Figure::shadow > 0)"));
        }
    }
    let on_screen: Vec<&Figure> = g.figures.iter().filter(|f| !(f.r < 0.0 || f.l > w || f.b < 0.0 || f.t > h)).collect();
    warnings.extend(depth(&on_screen));
    warnings.extend(taps(&g.taps, &on_screen));
    let mut lines = vec![match (seen.len(), g.floors.len()) {
        (0, _) => "grounding: nothing stood on a floor in this frame (stand characters and props with maimbrain::ground::Figure so this can check them)".to_string(),
        (n, fl) => format!(
            "grounding: {n} figure{} ({}) on {fl} marked floor{}{}",
            if n == 1 { "" } else { "s" },
            seen.join(", "),
            if fl == 1 { "" } else { "s" },
            if warnings.is_empty() { ": all standing" } else { "" }
        ),
    }];
    for t in &g.taps {
        lines.push(format!("tap area: {} {:.0}×{:.0} at ({:.0}, {:.0})–({:.0}, {:.0})", t.name, t.r - t.l, t.b - t.t, t.l, t.t, t.r, t.b));
    }
    if g.more > 0 {
        lines.push(format!("grounding: {} more not checked (more than a frame's marks hold)", g.more));
    }
    (lines, warnings)
}

/// Feet this close (units) are on the same floor line.
const SAME_FLOOR: f32 = 4.0;

/// How much two spans overlap.
fn overlap(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0.0)
}

/// Figures on one layer drawn in the wrong order, or standing in each other.
fn depth(figs: &[&Figure]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, back) in figs.iter().enumerate() {
        for front in &figs[i + 1..] {
            if back.layer != front.layer || back.name == front.name {
                continue;
            }
            let ox = overlap((back.l, back.r), (front.l, front.r));
            let oy = overlap((back.t, back.b), (front.t, front.b));
            let narrow = (back.r - back.l).min(front.r - front.l).max(1.0);
            let short = (back.b - back.t).min(front.b - front.t).max(1.0);
            if ox < 0.1 * narrow || oy < 0.25 * short {
                continue;
            }
            let (a, b) = (&back.name, &front.name);
            if front.b < back.b - SAME_FLOOR && !front.lifted && !back.lifted {
                out.push(format!(
                    "{b} is drawn in front of {a} but stands behind it (its feet {:.0} units farther up the screen): draw by the feet, back to front (ground::Scene), or give one a layer (Figure::layer)",
                    back.b - front.b
                ));
                continue;
            }
            if (front.b - back.b).abs() > SAME_FLOOR || front.lifted || back.lifted {
                continue;
            }
            let inside = front.base > 0.0 && back.base > 0.0 && overlap((back.fx - back.base / 2.0, back.fx + back.base / 2.0), (front.fx - front.base / 2.0, front.fx + front.base / 2.0)) > 0.0;
            if inside || ox > 0.25 * narrow {
                out.push(format!(
                    "{b} and {a} {} on one floor line ({b} drawn in front, bodies overlapping {ox:.0} units): they look tangled together. Stop {:.0} units short (ground::stop_short: half a body clear), or put the one passing behind on a lower layer (Figure::layer)",
                    if inside { "stand in each other's footprint" } else { "overlap" },
                    ox + 0.5 * narrow
                ));
            }
        }
    }
    out
}

/// Tap areas that miss part of the figure they're named for, or are too small.
fn taps(taps: &[Tap], figs: &[&Figure]) -> Vec<String> {
    let mut out = Vec::new();
    for t in taps {
        let (tw, th) = (t.r - t.l, t.b - t.t);
        if tw < 43.5 || th < 43.5 {
            out.push(format!("{}'s tap area is {tw:.0}×{th:.0} units: under 44 across, fingers miss it (ground::tap_size, Figure::hit)", t.name));
        }
        let Some(f) = figs.iter().find(|f| f.name == t.name) else { continue };
        let (fh, fw) = ((f.b - f.t).max(1.0), (f.r - f.l).max(1.0));
        let head = f.t + fh * 0.2;
        let mut missed = Vec::new();
        if t.t > f.t + 0.05 * fh {
            missed.push(if t.t >= head { format!("its head (the top {:.0} units)", t.t - f.t) } else { format!("the top {:.0} units", t.t - f.t) });
        }
        if t.b < f.b - 0.1 * fh {
            missed.push(format!("the bottom {:.0} units", f.b - t.b));
        }
        if overlap((t.l, t.r), (f.l, f.r)) < 0.8 * fw {
            missed.push(format!("{:.0} of its {fw:.0} units across", fw - overlap((t.l, t.r), (f.l, f.r))));
        }
        if !missed.is_empty() {
            out.push(format!(
                "{}'s tap area misses {} of the drawn {}: people tap what they see, faces first. Use Figure::hit (the opaque box, head to feet)",
                t.name,
                missed.join(" and "),
                t.name
            ));
        }
    }
    out
}

#[derive(Debug, PartialEq)]
enum Look {
    /// A level line at the feet.
    AtFeet,
    /// None at the feet, a clear one this far below them (units, and its y).
    Below(f32, f32),
    Nothing,
    /// Nothing beside the figure to look at (it fills the width).
    Unknown,
}

/// Searches the screenshot beside a figure (left and right of it, as wide
/// as it, up to 60 units) for a level edge: the rows where most columns
/// step the same way in lightness.
fn look_for_floor(shot: &Image, size: [u32; 2], f: &Figure) -> Look {
    let k = shot.w as f32 / size[0].max(1) as f32;
    let fw = (f.r - f.l).clamp(12.0, 60.0);
    let px = |u: f32| (u * k).round() as i64;
    let mut cols: Vec<u32> = Vec::new();
    for (a, b) in [(f.l - fw, f.l - 2.0), (f.r + 2.0, f.r + fw)] {
        for x in px(a).max(0)..px(b).min(shot.w as i64) {
            cols.push(x as u32);
        }
    }
    if cols.len() < 8 {
        return Look::Unknown;
    }
    let step = cols.len().div_ceil(256);
    let cols: Vec<u32> = cols.into_iter().step_by(step).collect();
    let d = px(1.0).max(1);
    let row = |y: i64| -> f32 {
        if y - d < 0 || y + d >= shot.h as i64 {
            return 0.0;
        }
        let (mut pos, mut neg) = (0, 0);
        for &x in &cols {
            let a = shot.get(x, (y - d) as u32);
            let b = shot.get(x, (y + d) as u32);
            let delta = oklab([b[0], b[1], b[2]])[0] - oklab([a[0], a[1], a[2]])[0];
            if delta > 0.04 {
                pos += 1;
            } else if delta < -0.04 {
                neg += 1;
            }
        }
        pos.max(neg) as f32 / cols.len() as f32
    };
    // The strongest row's score, and the middle of the rows that score as much (a step is seen
    // from the rows on both sides of it).
    let best = |from: f32, to: f32| -> (f32, f32) {
        let rows: Vec<(f32, i64)> = (px(from)..=px(to)).map(|y| (row(y), y)).collect();
        let top = rows.iter().map(|r| r.0).fold(0.0, f32::max);
        let at: Vec<i64> = rows.iter().filter(|r| r.0 >= top - 1e-6).map(|r| r.1).collect();
        (top, at.iter().sum::<i64>() as f32 / at.len().max(1) as f32 / k + 0.5 / k)
    };
    let at = best(f.b - 3.0, f.b + 3.0);
    if at.0 >= 0.5 {
        return Look::AtFeet;
    }
    let below = best(f.b + 3.0, f.b + 60.0);
    if below.0 >= 0.6 && below.0 >= 2.0 * at.0 {
        return Look::Below(below.1 - f.b, below.1);
    }
    Look::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_parse_from_marks() {
        let g = parse(["boss", "ground:fgirl,68,350,132,400,1;F400,0,3", "ground:60;fdoll,10,20,30,40,6;+3"]);
        assert_eq!(g.figures.len(), 2);
        assert_eq!(g.figures[0], Figure { name: "girl".into(), l: 68.0, t: 350.0, r: 132.0, b: 400.0, shadow: true, cropped: false, lifted: false, fx: 100.0, base: 0.0, layer: 0 });
        assert!(g.figures[1].cropped && g.figures[1].lifted && !g.figures[1].shadow);
        assert_eq!(g.floors, vec![Floor { y: 400.0, x0: 0.0, x1: 360.0 }]);
        assert_eq!(g.more, 3);
        assert!(is_ground("ground:F1,2,3") && !is_ground("subject:3"));
    }

    fn fig(name: &str, l: f32, t: f32, r: f32, b: f32, shadow: bool) -> Figure {
        Figure { name: name.into(), l, t, r, b, shadow, cropped: false, lifted: false, fx: (l + r) / 2.0, base: (r - l) * 0.5, layer: 0 }
    }

    #[test]
    fn new_records_carry_feet_layers_and_taps() {
        let g = parse(["ground:fgirl,90,350,113,400,33,100,15;fchair,100,370,150,400,24,125,50;", "ground:twitch,50,380,110,520"]);
        assert_eq!((g.figures[0].fx, g.figures[0].base, g.figures[0].layer), (100.0, 15.0, 0));
        assert_eq!(g.figures[1].layer, -1);
        assert!(g.figures[0].shadow && !g.figures[1].shadow);
        assert_eq!(g.taps, vec![Tap { name: "witch".into(), l: 50.0, t: 380.0, r: 110.0, b: 520.0 }]);
    }

    #[test]
    fn figures_tangled_together_or_drawn_out_of_order_are_flagged() {
        let floor = Floor { y: 500.0, x0: 0.0, x1: 360.0 };
        let shot = room(500.0);
        let run = |figures: Vec<Figure>| check(&Ground { figures, floors: vec![floor], ..Ground::default() }, &shot, [360, 640]).1;
        // The girl stopped in the toppled chair, the chair drawn over her.
        let girl = fig("girl", 100.0, 400.0, 130.0, 500.0, true);
        let chair = Figure { base: 60.0, ..fig("chair", 110.0, 460.0, 175.0, 500.0, true) };
        let w = run(vec![girl.clone(), chair.clone()]);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("chair and girl stand in each other's footprint on one floor line"), "{w:?}");
        // Stopped half a body short: fine.
        assert!(run(vec![fig("girl", 60.0, 400.0, 90.0, 500.0, true), chair.clone()]).is_empty());
        // On different layers (she passes behind it on purpose): fine.
        assert!(run(vec![Figure { layer: -1, ..girl.clone() }, chair.clone()]).is_empty());
        // A mirror farther back (feet higher) drawn over the girl in front of it: wrong order.
        let mut room_floor = vec![girl.clone(), fig("mirror", 95.0, 380.0, 140.0, 470.0, true)];
        let w = check(&Ground { figures: room_floor.clone(), ..Ground::default() }, &shot, [360, 640]).1;
        assert!(w.iter().any(|w| w.starts_with("mirror is drawn in front of girl but stands behind it (its feet 30 units farther up")), "{w:?}");
        // Drawn back to front: fine.
        room_floor.reverse();
        let w = check(&Ground { figures: room_floor, ..Ground::default() }, &shot, [360, 640]).1;
        assert!(!w.iter().any(|w| w.contains("drawn in front")), "{w:?}");
    }

    #[test]
    fn tap_areas_must_cover_the_figure_head_included() {
        let shot = room(500.0);
        let witch = fig("witch", 200.0, 330.0, 260.0, 500.0, true);
        let floor = Floor { y: 500.0, x0: 0.0, x1: 360.0 };
        let run = |taps: Vec<Tap>| check(&Ground { figures: vec![witch.clone()], floors: vec![floor], taps, more: 0 }, &shot, [360, 640]);
        // The witch's target stops at her shoulders.
        let (lines, w) = run(vec![Tap { name: "witch".into(), l: 198.0, t: 380.0, r: 262.0, b: 500.0 }]);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("witch's tap area misses its head (the top 50 units) of the drawn witch"), "{w:?}");
        assert!(lines.iter().any(|l| l == "tap area: witch 64×120 at (198, 380)–(262, 500)"), "{lines:?}");
        // Figure::hit's box: fine.
        assert!(run(vec![Tap { name: "witch".into(), l: 200.0, t: 330.0, r: 260.0, b: 500.0 }]).1.is_empty());
        // A candle's shrinking circle of light: too small to hit.
        let w = run(vec![Tap { name: "candle".into(), l: 50.0, t: 50.0, r: 70.0, b: 70.0 }]).1;
        assert!(w[0].starts_with("candle's tap area is 20×20 units: under 44 across"), "{w:?}");
    }

    /// A 360×640 screen at 2 px a unit: a wall above `floor`, a darker floor below.
    fn room(floor: f32) -> Image {
        let mut im = Image::new(720, 1280, [150, 120, 100, 255]);
        for y in (floor * 2.0) as u32..1280 {
            for x in 0..720 {
                im.set(x, y, [60, 45, 35, 255]);
            }
        }
        im
    }

    #[test]
    fn floating_cut_and_shadowless_figures_are_flagged() {
        let floor = Floor { y: 500.0, x0: 0.0, x1: 360.0 };
        let shot = room(500.0);
        let g = Ground {
            figures: vec![
                fig("girl", 100.0, 400.0, 140.0, 500.0, true),
                fig("doll", 200.0, 456.0, 220.0, 486.0, false),
                fig("witch", 250.0, 520.0, 330.0, 700.0, true),
            ],
            floors: vec![floor],
            ..Ground::default()
        };
        let (lines, w) = check(&g, &shot, [360, 640]);
        assert_eq!(lines[0], "grounding: 3 figures (girl, doll, witch) on 1 marked floor");
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].starts_with("doll floats 14 units above the floor"), "{w:?}");
        assert!(w[1].starts_with("witch's feet are 60 units below the bottom of the screen"), "{w:?}");
        // Standing, no shadow.
        let g = Ground { figures: vec![fig("girl", 100.0, 400.0, 140.0, 500.0, false)], floors: vec![floor], ..Ground::default() };
        let (lines, w) = check(&g, &shot, [360, 640]);
        assert!(w[0].contains("no contact shadow"), "{w:?}");
        assert!(!lines[0].contains("all standing"));
        // A jump, its shadow on the floor: fine.
        let mut jump = fig("girl", 100.0, 300.0, 140.0, 500.0, true);
        jump.lifted = true;
        let (lines, w) = check(&Ground { figures: vec![jump], floors: vec![floor], ..Ground::default() }, &shot, [360, 640]);
        assert!(w.is_empty() && lines[0].ends_with("all standing"), "{w:?}");
    }

    #[test]
    fn with_no_floor_marked_the_pixels_beside_a_figure_are_read() {
        let shot = room(500.0);
        // Feet on the painted floor line: fine.
        let on = fig("girl", 100.0, 400.0, 140.0, 500.0, true);
        assert_eq!(look_for_floor(&shot, [360, 640], &on), Look::AtFeet);
        assert!(check(&Ground { figures: vec![on], ..Ground::default() }, &shot, [360, 640]).1.is_empty());
        // The title card's characters 60 units up: floating over the line below.
        let up = fig("girl", 100.0, 340.0, 140.0, 440.0, true);
        let w = check(&Ground { figures: vec![up], ..Ground::default() }, &shot, [360, 640]).1;
        assert!(w[0].starts_with("girl looks like it floats 60 units above a floor line"), "{w:?}");
        // A plain card: nothing under it, no shadow.
        let plain = Image::new(720, 1280, [30, 30, 50, 255]);
        let w = check(&Ground { figures: vec![fig("witch", 100.0, 300.0, 180.0, 450.0, false)], ..Ground::default() }, &plain, [360, 640]).1;
        assert_eq!(w, vec!["witch stands on nothing visible: no floor line at its feet (y 450) and no contact shadow".to_string()]);
        // With a shadow it stands on that.
        assert!(check(&Ground { figures: vec![fig("witch", 100.0, 300.0, 180.0, 450.0, true)], ..Ground::default() }, &plain, [360, 640]).1.is_empty());
        // Nothing drawn with ground::Figure says so.
        assert!(check(&Ground::default(), &plain, [360, 640]).0[0].contains("nothing stood on a floor"));
    }
}
