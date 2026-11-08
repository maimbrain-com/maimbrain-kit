//! `mb play --check smoothness` (docs/STUDIO-2.md §8.2, Phase 3e): does
//! what the player watches move smoothly? The run is played again at twice
//! the frame rate (120 Hz, a ProMotion iPhone's), and both runs' poses
//! (studio-engine2's `studio_smooth`: the camera, the subject, the things
//! on the view, a frame each) are compared as `mb shot --smoothness` does
//! for kit games (smooth.rs): a spike in a path's second difference that
//! grows about 2× at twice the rate is a speed that changes within a frame,
//! about 4× a jump.
//!
//! What it flags, in Studio's words, each with what to do:
//! - the camera (2D: its place, zoom and turn; 3D: its place, where it
//!   looks and its lens) jumping, or starting and stopping at once;
//! - the subject (`round.subject`, else the player, as readability picks
//!   it) jumping in the world, or on the screen when the camera didn't;
//! - a thing on the view jumping across the screen in one frame (a
//!   teleport while visible), at both rates;
//! - a camera that's somewhere else at 120 Hz than at 60 (it moves by a
//!   fraction a frame, not a rate a second).
//!
//! A shake (`fx.shake`) isn't counted (poses leave it out), nor is a jump
//! within two frames of a mark: a hitstop, or the game's `debug.mark`.

use serde_json::{Value, json};

use crate::smooth::{self, Channel, Event, Measure, Pose, Run};

/// A 2D path's smallest spike worth a line: a speed change of about 8 units
/// a second within a 60 Hz frame (as smooth.rs's 0.1 m/s, seen from ~7 m).
const FLOOR_2D: f64 = 500.0;
/// A thing jumps when it moves at least this far on the screen in a frame...
const JUMP_MIN: f64 = 24.0;
/// ...and this many times its usual step around then.
const JUMP_TIMES: f64 = 6.0;
/// The 2D camera this far apart (units, median) at the two rates is moving by frames.
const APART_2D: f64 = 2.0;
/// Problems listed per kind.
const LIST: usize = 3;

/// One frame's pose, as the engine logs it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub f: u32,
    /// x, y, zoom, turn.
    pub cam: [f64; 4],
    /// place, where it looks, field of view, near, far.
    pub cam3: Option<[f64; 9]>,
    /// name, 3D, in the world, on the screen.
    pub subject: Option<(String, bool, [f64; 3], [f64; 2])>,
    /// id, name, on the screen.
    pub things: Vec<(u32, String, [f64; 2])>,
    pub marks: Vec<String>,
}

/// The poses in a run's log (`[game] studio-pose {…}` lines).
pub fn parse(stderr: &str) -> Vec<Frame> {
    stderr.lines().filter_map(|l| l.strip_prefix("[game] studio-pose ")).filter_map(|j| serde_json::from_str::<Value>(j).ok()).filter_map(|v| frame(&v)).collect()
}

fn frame(v: &Value) -> Option<Frame> {
    let nums = |v: &Value| v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()).unwrap_or_default();
    let cam = nums(&v["cam"]);
    let s = &v["subject"];
    Some(Frame {
        f: v["f"].as_u64()? as u32,
        cam: cam.get(..4)?.try_into().ok()?,
        cam3: nums(&v["cam3"]).get(..9).and_then(|c| c.try_into().ok()),
        subject: s.as_array().filter(|a| a.len() == 7).and_then(|a| Some((a[0].as_str()?.to_string(), a[1] == "3d", [a[2].as_f64()?, a[3].as_f64()?, a[4].as_f64()?], [a[5].as_f64()?, a[6].as_f64()?]))),
        things: v["things"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                let a = t.as_array()?;
                Some((a.first()?.as_u64()? as u32, a.get(1)?.as_str()?.to_string(), [a.get(2)?.as_f64()?, a.get(3)?.as_f64()?]))
            })
            .collect(),
        marks: v["marks"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(str::to_string)).collect(),
    })
}

/// smooth.rs's run, for its marks (and the 3D camera's distance apart).
fn run(frames: &[Frame], hz: f64) -> Run {
    let poses = frames
        .iter()
        .map(|p| Pose {
            frame: p.f,
            camera: p.cam3.map(|c| [c[0], c[1], c[2], c[3], c[4], c[5]]),
            projection: p.cam3.map(|c| [c[6], c[7], c[8]]),
            track: None,
            marks: p.marks.clone(),
            logs: Vec::new(),
        })
        .collect();
    Run { hz, poses }
}

type Path = Vec<(u32, [f64; 3])>;

fn path(frames: &[Frame], f: impl Fn(&Frame) -> Option<[f64; 3]>) -> Path {
    frames.iter().filter_map(|p| Some((p.f, f(p)?))).collect()
}

/// Whether a path moves at all (a still camera isn't worth a line).
fn moves(p: &Path) -> bool {
    p.windows(2).any(|w| (0..3).any(|k| (w[1].1[k] - w[0].1[k]).abs() > 1e-6))
}

/// What a channel is about, for its words.
#[derive(Clone, Copy, PartialEq)]
enum About {
    Camera,
    Subject,
    OnScreen,
}

/// What an event is, from how it grows at twice the rate (smooth.rs's verdicts).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// The same at both rates: sharp but continuous.
    Sharp,
    /// About 2×: the speed changes within a frame.
    Kink,
    /// About 4×: a jump.
    Jump,
    /// At one rate only: it moves differently at 120 Hz.
    OneRate,
}

fn kind(e: &Event, floor: f64) -> Kind {
    if e.lo < floor || e.hi < floor {
        return Kind::OneRate;
    }
    match e.hi / e.lo {
        r if r < 1.4 => Kind::Sharp,
        r if r < 2.8 => Kind::Kink,
        _ => Kind::Jump,
    }
}

struct Found {
    channel: Channel,
    about: About,
    /// The words for what it is: "the camera", "the 3D camera's lens", "fox".
    what: String,
}

/// The check: `{ lines, warnings }`. `size`: the canvas (a thing that
/// leaves by one edge and comes back by the other wraps, on purpose).
pub fn check(lo: &[Frame], lo_hz: f64, hi: &[Frame], hi_hz: f64, size: [f64; 2]) -> Value {
    if lo.len() < 3 || hi.len() < 3 {
        return json!({ "lines": ["smoothness: the round didn't play long enough to check (the run needs a few frames of play)"], "warnings": [] });
    }
    let (run_lo, run_hi) = (run(lo, lo_hz), run(hi, hi_hz));
    let mut found: Vec<Found> = Vec::new();
    let mut add = |what: String, about: About, measure: Measure, unit: &'static str, floor: f64, f: &dyn Fn(&Frame) -> Option<[f64; 3]>| {
        let (a, b) = (path(lo, f), path(hi, f));
        if moves(&a) || moves(&b) {
            let mut channel = smooth::channel(what.clone(), measure, unit, floor, (&a, lo_hz), (&b, hi_hz));
            for e in &mut channel.events {
                smooth::annotate(e, &run_lo, &run_hi);
            }
            found.push(Found { channel, about, what });
        }
    };
    add("the camera".into(), About::Camera, Measure::Position, "units/s²", FLOOR_2D, &|p| Some([p.cam[0], p.cam[1], 0.0]));
    add("the camera's zoom".into(), About::Camera, Measure::Fov, "/s²", smooth::FLOOR_DIRECTION, &|p| Some([p.cam[2], 0.0, 0.0]));
    add("the camera's turn".into(), About::Camera, Measure::Direction, "rad/s²", smooth::FLOOR_DIRECTION, &|p| Some([p.cam[3], 0.0, 0.0]));
    add("the 3D camera".into(), About::Camera, Measure::Position, "m/s²", smooth::FLOOR_POSITION, &|p| p.cam3.map(|c| [c[0], c[1], c[2]]));
    add("where the 3D camera looks".into(), About::Camera, Measure::Direction, "rad/s²", smooth::FLOOR_DIRECTION, &|p| p.cam3.map(|c| [c[3], c[4], c[5]]));
    add("the 3D camera's field of view".into(), About::Camera, Measure::Fov, "rad/s²", smooth::FLOOR_DIRECTION, &|p| p.cam3.map(|c| [c[6], 0.0, 0.0]));
    // The subject, by name (the one most frames have).
    let name = lo.iter().filter_map(|p| p.subject.as_ref()).map(|s| s.0.clone()).next();
    if let Some(name) = name {
        let three = lo.iter().filter_map(|p| p.subject.as_ref()).any(|s| s.1);
        let named = |p: &Frame| p.subject.as_ref().filter(|s| s.0 == name).cloned();
        let (unit, floor) = if three { ("m/s²", smooth::FLOOR_POSITION) } else { ("units/s²", FLOOR_2D) };
        add(name.clone(), About::Subject, Measure::Position, unit, floor, &|p| named(p).map(|s| s.2));
        add(format!("{name} on the screen"), About::OnScreen, Measure::Position, "units/s²", FLOOR_2D, &|p| named(p).map(|s| [s.3[0], s.3[1], 0.0]));
    }

    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    let unmarked = |about: About, kinds: &[Kind]| -> Vec<f64> {
        found.iter().filter(|c| c.about == about).flat_map(|c| c.channel.events.iter().filter(|e| e.marked.is_none() && kinds.contains(&kind(e, c.channel.floor))).map(|e| e.t)).collect()
    };
    let camera_times = unmarked(About::Camera, &[Kind::Kink, Kind::Jump, Kind::OneRate]);
    let near_camera = |t: f64| camera_times.iter().any(|c| (c - t).abs() <= smooth::AROUND);
    // A camera that follows the subject starts at once when the subject jumps: that's the subject's line.
    let subject_jumps = unmarked(About::Subject, &[Kind::Jump]);
    let near_subject_jump = |t: f64| subject_jumps.iter().any(|c| (c - t).abs() <= smooth::AROUND);
    for c in &found {
        let ch = &c.channel;
        let marked: Vec<&Event> = ch.events.iter().filter(|e| e.marked.is_some()).collect();
        // The problems: what the camera does; a jump of the subject (its own
        // moves start and stop at once by design); the subject jumping on the
        // screen when the camera didn't.
        let problems: Vec<(&Event, Kind)> = ch
            .events
            .iter()
            .filter(|e| e.marked.is_none())
            .map(|e| (e, kind(e, ch.floor)))
            .filter(|(e, k)| match c.about {
                About::Camera => *k == Kind::Jump || (*k != Kind::Sharp && !near_subject_jump(e.t)),
                About::Subject => *k == Kind::Jump,
                About::OnScreen => *k == Kind::Jump && !near_camera(e.t),
            })
            .collect();
        if problems.is_empty() && c.about == About::OnScreen && ch.events.iter().any(|e| e.marked.is_none() && kind(e, ch.floor) == Kind::Jump) {
            lines.push(format!("{}: jumps where the camera does", c.what));
            continue;
        }
        if problems.is_empty() {
            let also = if marked.is_empty() {
                String::new()
            } else {
                let mut labels: Vec<String> = marked.iter().filter_map(|e| e.marked.clone()).collect();
                labels.dedup();
                format!(" but where marked ({})", labels.join(", "))
            };
            lines.push(format!("{}: smooth{also}", c.what));
            continue;
        }
        let times: Vec<String> = problems.iter().take(LIST + 1).map(|(e, _)| format!("{:.2} s", e.t)).collect();
        lines.push(format!("{}: {} sudden change{} ({})", c.what, problems.len(), if problems.len() == 1 { "" } else { "s" }, times.join(", ")));
        let (first, rest) = (problems[0], &problems[1..]);
        let others = if rest.is_empty() { String::new() } else { format!(" (and at {})", rest.iter().take(LIST).map(|(e, _)| format!("{:.2} s", e.t)).collect::<Vec<_>>().join(", ")) };
        warnings.push(format!("{}{others}", advice(c, first.0, first.1, lo_hz, hi_hz)));
    }

    // The camera somewhere else at 120 Hz than at 60.
    let at_hi: std::collections::HashMap<u32, [f64; 4]> = hi.iter().map(|p| (p.f, p.cam)).collect();
    let mut d: Vec<(f64, f64)> = lo
        .iter()
        .filter_map(|p| {
            let t = p.f as f64 / lo_hz;
            let c = at_hi.get(&((t * hi_hz).round() as u32))?;
            Some((t, (p.cam[0] - c[0]).hypot(p.cam[1] - c[1])))
        })
        .collect();
    if !d.is_empty() && d.iter().any(|x| x.1 > 0.0) {
        let worst = d.iter().copied().fold((0.0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
        d.sort_by(|a, b| a.1.total_cmp(&b.1));
        let median = d[d.len() / 2].1;
        if median > APART_2D {
            warnings.push(format!(
                "the camera is {median:.1} units from where it is at 120 Hz (median; {:.1} at {:.2} s): it moves by a fraction or a step a frame, so on a 120 Hz iPhone it follows twice as fast. Scale by dt: x = x + (tx - x) * (1 - math.exp(-dt / 0.15)), or camera:follow(node)",
                worst.1, worst.0
            ));
        }
    }
    if let Some((median, max, t)) = smooth::apart(&run_lo, &run_hi)
        && median > 0.05
    {
        warnings.push(format!(
            "the 3D camera is {median:.2} m from where it is at 120 Hz (median; {max:.2} m at {t:.2} s): it moves by a fraction or a step a frame, so on a 120 Hz iPhone it follows twice as fast. Scale by dt, or camera:follow(node)"
        ));
    }

    // Things that jump across the screen in a frame, at both rates.
    let teleports = teleports(lo, lo_hz, hi, hi_hz, size, &near_camera);
    if !teleports.is_empty() {
        // One line a thing, at its biggest jump.
        let mut names: Vec<&str> = Vec::new();
        for (_, name, _) in &teleports {
            if !names.contains(&name.as_str()) {
                names.push(name);
            }
        }
        for name in names.iter().take(LIST) {
            let all: Vec<&(f64, String, f64)> = teleports.iter().filter(|x| x.1 == *name).collect();
            let (t, _, dist) = all[0];
            let more = if all.len() > 1 { format!(" (and {} more time{})", all.len() - 1, if all.len() == 2 { "" } else { "s" }) } else { String::new() };
            warnings.push(format!(
                "{name} jumps {dist:.0} units across the screen in one frame at {t:.2} s{more}, at 60 and 120 Hz alike (moved while it shows): hide it first (visible = false, then show it where it lands), pop it in (fx.burst, a scale tween), or move it over time. If it's on purpose, debug.mark(\"respawn\") that frame"
            ));
        }
        if names.len() > LIST {
            lines.push(format!("{} more things jump across the screen", names.len() - LIST));
        }
    } else {
        lines.push("things on the view: none jumps across the screen".into());
    }
    json!({ "lines": lines, "warnings": warnings })
}

/// A warning for a channel's worst problem: where, what, and what to do.
fn advice(c: &Found, e: &Event, k: Kind, lo_hz: f64, hi_hz: f64) -> String {
    let at = format!("{:.2} s", e.t);
    let growth = if e.lo > 0.0 && e.hi > 0.0 { format!("{:.1}× as sharp at {hi_hz} Hz as at {lo_hz}", e.hi / e.lo) } else { format!("at {} Hz only", if e.lo > 0.0 { lo_hz } else { hi_hz }) };
    let what = &c.what;
    match (c.about, k) {
        (About::Camera, Kind::Jump) => format!(
            "{what} jumps at {at} ({growth}): a snap from one place to another, or following by a fixed fraction a frame. Ease it over time: camera:follow(node, {{ lag = 0.15 }}), or x = x + (tx - x) * (1 - math.exp(-dt / 0.15)) in love.update. If it's a cut on purpose, debug.mark(\"cut\") that frame"
        ),
        (About::Camera, Kind::Kink) => format!(
            "{what} starts or stops all at once at {at} ({growth}): its speed changes within a frame. Let it ease in and out: follow with lag, or tween it (flux) rather than setting a new speed"
        ),
        (About::Camera, _) => format!("{what} moves differently at 120 Hz than at 60 at {at} ({growth}): something moves it by a step a frame. Scale by dt"),
        (About::Subject, _) => format!(
            "{what} jumps in the world at {at} ({growth}): it's moved, not moved over time. If that's a respawn or a teleport on purpose, debug.mark(\"respawn\") that frame (and give it a flash or a burst); otherwise move it with its speed (vx, vy) or a tween"
        ),
        (About::OnScreen, _) => format!(
            "{what} jumps on the screen at {at} ({growth}) though the camera didn't: something sets its place, or the camera's, a frame late. Move them in the same frame (camera:follow does), or ease it"
        ),
    }
}

/// Things that jump at least `JUMP_MIN` units and `JUMP_TIMES` their usual
/// step on the screen in a frame, in both runs (a fast mover's steps halve
/// at twice the rate; a teleport's don't): (time, name, distance), worst
/// first. Not near a mark or a camera jump.
fn teleports(lo: &[Frame], lo_hz: f64, hi: &[Frame], hi_hz: f64, size: [f64; 2], near_camera: &dyn Fn(f64) -> bool) -> Vec<(f64, String, f64)> {
    // From near one edge to near the other: it wrapped round.
    let edge = JUMP_MIN;
    let wraps = |a: [f64; 2], b: [f64; 2]| (0..2).any(|k| (a[k] > size[k] - edge && b[k] < edge) || (a[k] < edge && b[k] > size[k] - edge));
    let jumps = |frames: &[Frame], hz: f64| -> Vec<(f64, String, f64)> {
        // Where each thing is on the screen, and that with the camera's move taken
        // out (a thing that stays put moves across the screen as the camera does).
        let mut by_id: std::collections::BTreeMap<u32, (String, Vec<(u32, [f64; 2], [f64; 2])>)> = Default::default();
        for p in frames {
            let [cx, cy, zoom, _] = p.cam;
            for (id, name, at) in &p.things {
                by_id.entry(*id).or_insert_with(|| (name.clone(), Vec::new())).1.push((p.f, *at, [at[0] + cx * zoom, at[1] + cy * zoom]));
            }
        }
        let marked = |f: u32| frames.iter().any(|p| p.marks.iter().any(|m| m != "hitstop") && p.f + 2 >= f && p.f <= f + 2);
        let w = (smooth::AROUND * hz).round() as usize;
        let mut out = Vec::new();
        for (name, path) in by_id.values() {
            let steps: Vec<(u32, f64)> =
                path.windows(2).filter(|s| s[1].0 == s[0].0 + 1).map(|s| (s[1].0, if wraps(s[0].1, s[1].1) { 0.0 } else { (s[1].2[0] - s[0].2[0]).hypot(s[1].2[1] - s[0].2[1]) })).collect();
            for i in 0..steps.len() {
                let (f, d) = steps[i];
                if d < JUMP_MIN {
                    continue;
                }
                let mut around: Vec<f64> = steps[i.saturating_sub(w)..(i + w + 1).min(steps.len())].iter().enumerate().filter(|(j, _)| *j + i.saturating_sub(w) != i).map(|(_, s)| s.1).collect();
                around.sort_by(f64::total_cmp);
                let usual = around.get(around.len() / 2).copied().unwrap_or(0.0);
                if d >= JUMP_TIMES * usual && !marked(f) {
                    out.push((f as f64 / hz, name.clone(), d));
                }
            }
        }
        out
    };
    let (a, b) = (jumps(lo, lo_hz), jumps(hi, hi_hz));
    let mut out: Vec<(f64, String, f64)> = a
        .into_iter()
        .filter(|(t, name, d)| !near_camera(*t) && b.iter().any(|(t2, n2, d2)| n2 == name && (t2 - t).abs() <= 1.0 / lo_hz + smooth::SAME_EVENT && *d2 >= 0.6 * d))
        .collect();
    out.sort_by(|x, y| y.2.total_cmp(&x.2));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2D run at `hz` for `secs`: the camera at `cam(t, dt, previous)`,
    /// the subject at `subj(t)`, and `things(t)` on the screen.
    fn run2(hz: f64, secs: f64, mut cam: impl FnMut(f64, f64, [f64; 2]) -> [f64; 2], subj: impl Fn(f64) -> [f64; 2], things: impl Fn(f64) -> Vec<(u32, &'static str, [f64; 2])>) -> Vec<Frame> {
        let mut c = [180.0, 320.0];
        (1..=(secs * hz) as u32)
            .map(|f| {
                let t = f as f64 / hz;
                c = cam(t, 1.0 / hz, c);
                let s = subj(t);
                Frame {
                    f,
                    cam: [c[0], c[1], 1.0, 0.0],
                    cam3: None,
                    subject: Some(("fox".into(), false, [s[0], s[1], 0.0], [s[0] - c[0] + 180.0, s[1] - c[1] + 320.0])),
                    things: things(t).into_iter().map(|(i, n, p)| (i, n.to_string(), p)).collect(),
                    marks: Vec::new(),
                }
            })
            .collect()
    }

    /// The fox runs right at 200 units a second, hopping at 1.5 s.
    fn fox(t: f64) -> [f64; 2] {
        [100.0 + 200.0 * t, 400.0 - if (1.5..2.0).contains(&t) { 600.0 * (t - 1.5) * (2.0 - t) * 2.0 } else { 0.0 }]
    }

    fn none(_: f64) -> Vec<(u32, &'static str, [f64; 2])> {
        Vec::new()
    }

    fn check2(cam: impl Fn(f64, f64, [f64; 2]) -> [f64; 2] + Copy, things: impl Fn(f64) -> Vec<(u32, &'static str, [f64; 2])> + Copy) -> Value {
        check(&run2(60.0, 4.0, cam, fox, things), 60.0, &run2(120.0, 4.0, cam, fox, things), 120.0, [360.0, 640.0])
    }

    #[test]
    fn a_follow_camera_is_smooth() {
        // camera:follow's way: first-order, scaled by dt.
        let follow = |t: f64, dt: f64, c: [f64; 2]| {
            let f = fox(t);
            let k = 1.0 - (-dt / 0.15f64).exp();
            [c[0] + (f[0] - c[0]) * k, c[1] + (f[1] - 80.0 - c[1]) * k]
        };
        let r = check2(follow, none);
        assert_eq!(r["warnings"], json!([]), "{r}");
        let lines: Vec<&str> = r["lines"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(lines.contains(&"the camera: smooth"), "{r}");
        assert!(lines.contains(&"things on the view: none jumps across the screen"), "{r}");
    }

    #[test]
    fn a_camera_that_snaps_is_flagged_with_what_to_do() {
        // A screen-by-screen camera: it jumps a screen's width when the fox leaves it.
        let snap = |t: f64, _: f64, _: [f64; 2]| [180.0 + 360.0 * ((fox(t)[0] - 0.0) / 360.0).floor(), 320.0];
        // A tree that stays put in the world jumps on the screen with the camera: that's the camera's.
        let tree = |t: f64| vec![(3, "tree", [400.0 - snap(t, 0.0, [0.0; 2])[0] + 180.0, 500.0])];
        let r = check2(snap, tree);
        let w = r["warnings"].as_array().unwrap();
        assert_eq!(w.len(), 1, "one warning for the camera; the fox on the screen goes with it: {r}");
        assert!(r["lines"].as_array().unwrap().contains(&json!("fox on the screen: jumps where the camera does")), "{r}");
        let w = w[0].as_str().unwrap();
        assert!(w.starts_with("the camera jumps at 1.28 s (4.0× as sharp at 120 Hz as at 60)") && w.contains("camera:follow(node, { lag = 0.15 })") && w.contains("debug.mark(\"cut\")"), "{w}");
        assert!(w.ends_with("(and at 3.08 s)"), "the next screen too: {w}");
    }

    #[test]
    fn a_camera_lerping_a_fraction_a_frame_is_caught_by_the_rates() {
        let lerp = |t: f64, _: f64, c: [f64; 2]| {
            let f = fox(t);
            [c[0] + (f[0] - c[0]) * 0.1, c[1] + (f[1] - 80.0 - c[1]) * 0.1]
        };
        let r = check2(lerp, none);
        let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(w.iter().any(|w| w.contains("from where it is at 120 Hz") && w.contains("math.exp(-dt / 0.15)")), "{r}");
    }

    #[test]
    fn a_teleport_while_visible_is_flagged_and_a_mark_excuses_it() {
        let follow = |t: f64, _: f64, _: [f64; 2]| [fox(t)[0], 320.0];
        // A coin bobbing in place jumps to the other side at 2 s; a bullet flies fast.
        let things = |t: f64| vec![(7, "coin", if t < 2.0 { [60.0, 200.0 + 4.0 * (t * 6.0).sin()] } else { [300.0, 200.0] }), (9, "bullet", [(t * 900.0) % 360.0, 500.0])];
        let r = check2(follow, things);
        let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(w.len(), 1, "the coin (the bullet wraps round the screen's edges, on purpose): {r}");
        assert!(w[0].starts_with("coin jumps 243 units across the screen in one frame at 2.00 s, at 60 and 120 Hz alike"), "{r}");
        // Marked on purpose: no line.
        let mut lo = run2(60.0, 4.0, follow, fox, things);
        let mut hi = run2(120.0, 4.0, follow, fox, things);
        for p in lo.iter_mut().chain(hi.iter_mut()) {
            p.things.retain(|t| t.1 == "coin");
        }
        lo.iter_mut().find(|p| p.f == 120).unwrap().marks.push("respawn".into());
        hi.iter_mut().find(|p| p.f == 240).unwrap().marks.push("respawn".into());
        let r = check(&lo, 60.0, &hi, 120.0, [360.0, 640.0]);
        assert_eq!(r["warnings"], json!([]), "{r}");
    }

    #[test]
    fn the_subject_teleporting_is_a_jump_in_the_world() {
        let follow = |t: f64, dt: f64, c: [f64; 2]| {
            let k = 1.0 - (-dt / 0.15f64).exp();
            let f = if t < 3.0 { fox(t) } else { [100.0, 400.0] };
            [c[0] + (f[0] - c[0]) * k, c[1]]
        };
        let respawn = |t: f64| if t < 3.0 { fox(t) } else { [100.0, 400.0] };
        let r = check(&run2(60.0, 4.0, follow, respawn, none), 60.0, &run2(120.0, 4.0, follow, respawn, none), 120.0, [360.0, 640.0]);
        let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(w.len(), 1, "the camera setting off after it is the same event: {r}");
        assert!(w[0].starts_with("fox jumps in the world at 2.98 s") && w[0].contains("debug.mark(\"respawn\")"), "{r}");
    }

    #[test]
    fn poses_parse_from_the_engines_lines() {
        let log = "[game] studio-pose {\"f\":7,\"t\":0.1,\"cam\":[180,320,1,0],\"subject\":[\"fox\",\"2d\",10,20,0,30,40],\"things\":[[4097,\"coin\",5,6]],\"marks\":[\"hitstop\"]}\n[game] other\n";
        let f = parse(log);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject, Some(("fox".into(), false, [10.0, 20.0, 0.0], [30.0, 40.0])));
        assert_eq!(f[0].things, [(4097, "coin".to_string(), [5.0, 6.0])]);
        assert_eq!(f[0].marks, ["hitstop"]);
        assert!(check(&f, 60.0, &f, 120.0, [360.0, 640.0])["lines"][0].as_str().unwrap().contains("didn't play long enough"));
    }
}
