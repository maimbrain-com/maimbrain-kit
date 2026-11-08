//! `mb play --check readability` (docs/STUDIO-2.md §8.2): does the subject
//! read against the scene, on the frames the server draws?
//!
//! The measuring is readability.rs's, as `mb shot --readability` does it:
//! the subject's share of the screen, its outline's lightness against
//! what's just outside it, its own tonal range, the HUD over it and the
//! frame's lightness. What differs is where the pictures come from. Studio
//! engine 2.0 says which node the subject is (`round.subject`, else the
//! player, else what the camera follows, else a node called "player") in
//! its frame description, and the headless runner has it draw each frame
//! again: without the HUD, and its 2D subject alone on a key colour (the
//! silhouette is where that differs from the key). A 3D subject is masked
//! in mb-host, the world drawn plain with it and without it, as `mb shot`
//! does. So the subject is never guessed from pixels.
//!
//! The advice is in Studio's words, one line a problem, the worst frame's,
//! with the frames it also showed in; and the worst frame's picture in
//! greyscale with the subject outlined in magenta (`<frame>.readability.png`,
//! at one pixel a unit).

use std::path::Path;

use serde_json::{Value, json};

use crate::generate::img::Image;
use crate::readability::{self, COVERAGE_LOW, EDGE_LOW, EDGE_OK, EDGE_SHARE_LOW, FRAME_DARK, HUD_OVER, INSIDE_FLAT, Report};

/// One frame drawn, with the runner's variants of it.
pub struct Frame {
    /// Seconds into the run.
    pub at: f64,
    /// The frame's description (the engine's `studio-frame`).
    pub described: Value,
    /// What the player sees.
    pub shown: Image,
    /// Without the HUD.
    pub clean: Option<Image>,
    /// A 2D subject alone on the key colour.
    pub alone: Option<Image>,
    /// A 3D subject: the world plain with it and without it.
    pub plain: Option<(Image, Image)>,
    /// The PNG's file name (in the frames' folder).
    pub file: String,
}

/// The key colour a 2D subject is drawn alone on (studio-engine2's
/// headless.rs `SUBJECT_KEY`).
const KEY: [u8; 4] = [255, 0, 255, 255];

/// A problem found, for grouping across frames.
struct Problem {
    kind: &'static str,
    /// Higher is worse (to pick the frame that shows it best).
    badness: f32,
    at: f64,
    text: String,
}

/// The check over the frames: `{ lines, warnings, image }`, where `image`
/// is the worst frame's picture, written to `out` (None when no frame had a
/// subject to measure). `scale`: frame pixels a logical unit.
pub fn check(frames: &[Frame], scale: u32, out: &Path) -> Value {
    let playing: Vec<&Frame> = frames.iter().filter(|f| f.described["mode"] == "play").collect();
    if playing.is_empty() {
        return json!({ "lines": ["readability: no frame was drawn while the round played (only the cards), so nothing was checked"], "warnings": [] });
    }
    // An engine from before the check neither names a subject (not even
    // `null`) nor draws the variants it's asked for.
    if playing.iter().all(|f| f.clean.is_none() || f.described.get("subject").is_none()) {
        return json!({ "lines": ["readability: this engine build doesn't name its subject or draw the check's variants, so nothing was checked"], "warnings": [] });
    }
    let mut lines = Vec::new();
    let mut problems: Vec<Problem> = Vec::new();
    // The frame to picture: the one with the worst problem, else the last.
    let mut worst: Option<(f32, usize)> = None;
    let mut measured: Vec<(usize, Option<(Image, Image)>)> = Vec::new();
    for (i, f) in playing.iter().enumerate() {
        let (Some(clean), Some(_)) = (&f.clean, f.described.get("subject")) else { continue };
        let subject = &f.described["subject"];
        let name = subject["name"].as_str().filter(|n| !n.is_empty()).unwrap_or("the subject").to_string();
        let three = subject["kind"] == "3d";
        let mask = if three {
            f.plain.clone()
        } else if subject.is_object() {
            f.alone.clone().map(|a| (a, Image::new(clean.w, clean.h, KEY)))
        } else {
            None
        };
        let r = readability::measure(&f.shown, clean, mask.as_ref().map(|(a, b)| (a, b)), scale);
        let at = format!("{:.1} s", f.at);
        let line = match (&r.subject, subject.is_object()) {
            (Some(_), _) => format!("{name} ({}) at {at}: {}", subject["why"].as_str().unwrap_or("the subject"), r.line().trim_start_matches("readability: subject ")),
            (None, true) => format!("{name} at {at} wasn't drawn (off screen, hidden or blinking): {}", frame_part(&r)),
            (None, false) => format!("no subject at {at} (name one: round.subject = node): {}", frame_part(&r)),
        };
        lines.push(line);
        let behind: Vec<&str> = subject["behind"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let found = problems_in(&r, &name, &behind, three, f.at);
        let bad = found.iter().map(|p| p.badness).fold(0.0, f32::max);
        if r.subject.is_some() && worst.is_none_or(|(b, _)| bad > b || (bad == b && bad == 0.0)) {
            worst = Some((bad, i));
        }
        problems.extend(found);
        measured.push((i, mask));
    }
    // One warning a kind of problem: the worst frame's, and when else it showed.
    let mut warnings = Vec::new();
    let mut kinds: Vec<&str> = problems.iter().map(|p| p.kind).collect();
    kinds.dedup();
    let mut seen = Vec::new();
    for k in kinds {
        if seen.contains(&k) {
            continue;
        }
        seen.push(k);
        let of: Vec<&Problem> = problems.iter().filter(|p| p.kind == k).collect();
        let w = of.iter().max_by(|a, b| a.badness.total_cmp(&b.badness)).unwrap();
        let mut also: Vec<String> = of.iter().filter(|p| p.at != w.at).map(|p| format!("{:.1}", p.at)).collect();
        also.dedup();
        warnings.push(if also.is_empty() { w.text.clone() } else { format!("{} (also at {} s)", w.text, also.join(", ")) });
    }
    let mut report = json!({ "lines": lines, "warnings": warnings });
    if let Some((_, i)) = worst {
        let f = playing[i];
        let mask = measured.iter().find(|(j, _)| *j == i).and_then(|(_, m)| m.as_ref());
        let clean = f.clean.as_ref().unwrap();
        // Shrink first and draw the 1-pixel outline at 1×: a resize after
        // would blur the magenta into the grey.
        let small = |i: &Image| if scale > 1 { i.resize(i.w / scale, i.h / scale) } else { i.clone() };
        let mask = mask.map(|(a, b)| (small(a), small(b)));
        let pic = readability::picture(&small(clean), mask.as_ref().map(|(a, b)| (a, b)));
        let file = Path::new(&f.file).with_extension("readability.png").to_string_lossy().into_owned();
        if std::fs::write(out.join(&file), pic.encode_png()).is_ok() {
            report["image"] = json!({ "at": f.at, "file": file });
        }
    }
    report
}

fn frame_part(r: &Report) -> String {
    r.line().rsplit(" · ").next().unwrap_or_default().trim_start_matches("readability: ").split(" (no subject").next().unwrap_or_default().to_string()
}

/// What to fix in one frame, in Studio's words, as readability.rs's
/// `Report::warnings` judges it (the same thresholds).
fn problems_in(r: &Report, name: &str, behind: &[&str], three: bool, at: f64) -> Vec<Problem> {
    let mut out = Vec::new();
    let when = format!("at {at:.1} s");
    let against = if behind.is_empty() { String::new() } else { format!(" against {}", behind.join(", ")) };
    if let Some(s) = &r.subject {
        if s.edge_dl < EDGE_LOW || s.edge_share < EDGE_SHARE_LOW {
            let hue = if s.edge_de >= 2.0 * EDGE_OK { " It differs mostly in colour, which a glance (and greyscale) loses." } else { "" };
            let fix = if three {
                "Make it lighter or darker than what's behind it: world.lighting(\"hero\"), a rim on its material (rim = 0.5), a brighter or darker sky, or a lighter/darker colour."
            } else {
                "Make it lighter or darker than what's behind it: a darker or lighter background layer, an outline in its art, or a lighter/darker sprite."
            };
            out.push(Problem {
                kind: "edge",
                badness: 2.0 + (EDGE_LOW - s.edge_dl).max(0.0) / EDGE_LOW + (EDGE_SHARE_LOW - s.edge_share).max(0.0),
                at,
                text: format!(
                    "{name} doesn't stand out{against} {when}: only {:.0}% of its outline differs in lightness by {EDGE_OK:.0}+ (L*, 0–100), median {:.0}; aim for {:.0}%+ and a median of {EDGE_LOW:.0}+.{hue} {fix}",
                    s.edge_share * 100.0,
                    s.edge_dl,
                    EDGE_SHARE_LOW * 100.0,
                ),
            });
        }
        if s.coverage < COVERAGE_LOW {
            let fix = if three { "Bring the camera closer (camera:follow's behind and above) or scale it up." } else { "Scale it up (scale) or zoom the camera in (camera.scale)." };
            out.push(Problem {
                kind: "small",
                badness: 2.0 + (COVERAGE_LOW - s.coverage) / COVERAGE_LOW,
                at,
                text: format!("{name} covers {:.1}% of the screen {when}: too small to read on a phone (aim for {:.1}%+). {fix}", s.coverage * 100.0, COVERAGE_LOW * 100.0),
            });
        }
        // A flat tone inside is how lighting hides a model's texture and
        // form; a 2D subject of one colour (a shape, flat art) is a choice.
        if three && s.inside[1] - s.inside[0] < INSIDE_FLAT {
            let fix = "Light its front (world.lighting(\"hero\"), the sun or a Light), and drop toon bands on a textured model.";
            out.push(Problem {
                kind: "flat",
                badness: 1.0 + (INSIDE_FLAT - (s.inside[1] - s.inside[0])) / INSIDE_FLAT,
                at,
                text: format!("{name} is nearly one tone inside {when} (L* {:.0}–{:.0}): its shape and detail don't show. {fix}", s.inside[0], s.inside[1]),
            });
        }
        if s.hud > HUD_OVER {
            out.push(Problem {
                kind: "hud",
                badness: 1.0 + s.hud,
                at,
                text: format!("the HUD covers {:.0}% of {name} {when}: keep it clear of the HUD (move {name}, or the HUD with ui.hud).", s.hud * 100.0),
            });
        }
    }
    if r.frame[1] < FRAME_DARK {
        let fix = if three { "Brighten it: world.post{ exposure }, a brighter sky or sun, less fog." } else { "Brighten the background and the art, or the colours it's tinted." };
        out.push(Problem {
            kind: "dark",
            badness: 2.0 + (FRAME_DARK - r.frame[1]) / FRAME_DARK,
            at,
            text: format!("the frame is dark {when}: 95% of it is below L* {:.0} (of 100), so on a phone in daylight it reads as mud. {fix}", r.frame[1]),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A w×h frame of `bg` with a `fg` square of side `s` at (x, y).
    fn scene(w: u32, h: u32, bg: [u8; 4], fg: Option<[u8; 4]>, x: u32, y: u32, s: u32) -> Image {
        let mut img = Image::new(w, h, bg);
        if let Some(fg) = fg {
            for v in y..y + s {
                for u in x..x + s {
                    img.set(u, v, fg);
                }
            }
        }
        img
    }

    fn frame(at: f64, bg: [u8; 4], fg: [u8; 4]) -> Frame {
        let shown = scene(180, 320, bg, Some(fg), 70, 200, 40);
        Frame {
            at,
            described: json!({ "mode": "play", "subject": { "name": "fox", "why": "round.subject", "kind": "2d", "behind": ["dusk_2"] } }),
            clean: Some(shown.clone()),
            alone: Some(scene(180, 320, KEY, Some(fg), 70, 200, 40)),
            shown,
            plain: None,
            file: format!("play-{at:.2}s.png"),
        }
    }

    fn out() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mb-readable-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_low_contrast_subject_on_dusk_is_named_with_what_is_behind_it() {
        let dusk = [70, 60, 90, 255];
        let frames = [frame(1.0, dusk, [80, 66, 96, 255]), frame(2.0, dusk, [84, 70, 98, 255])];
        let dir = out();
        let r = check(&frames, 1, &dir);
        let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let edge = w.iter().find(|w| w.contains("doesn't stand out")).expect("flagged");
        assert!(edge.starts_with("fox doesn't stand out against dusk_2 at "), "{edge}");
        assert!(edge.contains("(also at"), "one warning for both frames: {edge}");
        assert_eq!(w.iter().filter(|w| w.contains("doesn't stand out")).count(), 1);
        assert!(r["lines"][0].as_str().unwrap().starts_with("fox (round.subject) at 1.0 s: "), "{}", r["lines"]);
        let pic = Image::decode(&std::fs::read(dir.join(r["image"]["file"].as_str().unwrap())).unwrap()).unwrap();
        assert!(pic.px.contains(&[255, 0, 255, 255]), "the outline is drawn");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_outline_stays_pure_magenta_at_2x() {
        let dir = out();
        let r = check(&[frame(1.0, [70, 60, 90, 255], [80, 66, 96, 255])], 2, &dir);
        let pic = Image::decode(&std::fs::read(dir.join(r["image"]["file"].as_str().unwrap())).unwrap()).unwrap();
        assert_eq!((pic.w, pic.h), (90, 160), "the picture is at 1×");
        let magenta = pic.px.iter().filter(|p| **p == [255, 0, 255, 255]).count();
        assert!(magenta >= 60, "a solid outline round the 20 px square, not a blurred one: {magenta} pure magenta pixels");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_clear_subject_has_nothing_to_fix() {
        let r = check(&[frame(1.0, [40, 70, 140, 255], [250, 230, 200, 255])], 1, &out());
        assert_eq!(r["warnings"], json!([]), "{r}");
        assert!(r["image"]["file"].is_string());
    }

    #[test]
    fn cards_only_and_no_subject_say_so() {
        let mut f = frame(1.0, [40, 70, 140, 255], [250, 230, 200, 255]);
        f.described["mode"] = "title".into();
        let r = check(std::slice::from_ref(&f), 1, &out());
        assert!(r["lines"][0].as_str().unwrap().contains("only the cards"));
        f.described = json!({ "mode": "play", "subject": null });
        let r = check(std::slice::from_ref(&f), 1, &out());
        assert!(r["lines"][0].as_str().unwrap().starts_with("no subject at 1.0 s (name one: round.subject = node)"), "{r}");
        // An engine from before the check says nothing of a subject.
        f.described = json!({ "mode": "play" });
        let r = check(&[f], 1, &out());
        assert!(r["lines"][0].as_str().unwrap().contains("this engine build doesn't name its subject"), "{r}");
        assert_eq!(r["warnings"], json!([]));
    }
}
