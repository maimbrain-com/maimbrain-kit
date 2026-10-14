//! The smoothness check (`mb shot --smoothness`, SPEC §9): whether the
//! camera (and a node the game names) moves smoothly, from the same scripted
//! run stepped at a frame rate and at twice it.
//!
//! Screenshots and `mb.verify()` can't see a kink: a camera that turns 90°
//! within a frame, a follow that snaps, smoothing that restarts. In a run
//! stepped frame by frame they show up in the second difference of position
//! (its acceleration, m/s²) and of the look direction (rad/s²): smooth
//! motion has an acceleration that changes gradually, whatever the frame
//! rate; a discontinuity is a spike one or two frames wide. Doubling the
//! frame rate tells the kinds apart, since a spike scales with how much
//! changed within one frame:
//!
//! - the same at both rates: a sharp but continuous change (an ease, a hard
//!   but real acceleration);
//! - about 2× at twice the rate: the velocity jumps within a frame (a kink:
//!   an instant change of speed or direction, or first-order smoothing such
//!   as `x += (target - x) * (1 - exp(-dt / tau))`, which starts moving at
//!   full speed the frame its target jumps);
//! - about 4×: the position jumps (a snap or a cut), or smoothing by a fixed
//!   fraction per frame (`x += (target - x) * 0.2`), which follows twice as
//!   fast at twice the rate.
//!
//! Where the runs should play the same, how far apart the camera is at the
//! same game time also shows motion that depends on the frame rate.
//!
//! The camera's field of view is checked the same way (a zoom that pops the
//! frame a boost starts is as jarring as a snap), in rad/s²; a near or far
//! plane that steps by more than a quarter within a frame is listed too.
//!
//! Each spike comes with what the game logged (`sys::log`) within two frames
//! of it, to say what was going on. A game marks the frames where motion
//! changes on purpose (`sys::mark("jump")`, SPEC §5.1): a spike within two
//! frames of a mark is listed as "marked: jump" and isn't counted.

use serde_json::Value;

/// A spike stands out from the motion around it by this factor: its
/// acceleration is at least `SPIKE` times the median of the frames before
/// it and of those after it (whichever is higher), within `AROUND`.
const SPIKE: f64 = 4.0;
/// Seconds on each side of a frame that make up its surroundings.
const AROUND: f64 = 0.15;
/// Smallest spike worth reporting: a camera position kink of 0.1 m/s (or
/// a look-direction kink of 3°/s, 0.05 rad/s) in one 60 Hz frame, below
/// what anyone sees and well above f32 rounding a kilometre from the origin.
const FLOOR_POSITION: f64 = 6.0; // m/s²
const FLOOR_DIRECTION: f64 = 3.0; // rad/s²
/// Spikes this close (s) are one event; its peak at the other rate is looked for this near.
const SAME_EVENT: f64 = 0.03;
/// The camera this far apart (m, median over the run) at the two rates is reported as a difference.
const APART: f64 = 0.05;
/// Events listed per channel, worst first.
const LIST: usize = 5;
/// Frames on each side of a spike whose marks and log lines go with it.
const NOTES_AROUND: u32 = 2;
/// Log lines listed per spike.
const LOG_LINES: usize = 4;

/// A near or far plane that changes by more than this fraction within a frame is a step.
const CLIP_STEP: f64 = 0.25;

/// One frame's pose from `mb.step()`'s timings: `camera` [x, y, z, fx, fy, fz,
/// fov_y, near, far] and the tracked node's position, when the frame
/// rendered mb3d; and what the game marked and logged in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pose {
    pub frame: u32,
    pub camera: Option<[f64; 6]>,
    /// The projection: vertical field of view (radians), near and far (m).
    pub projection: Option<[f64; 3]>,
    pub track: Option<[f64; 3]>,
    pub marks: Vec<String>,
    pub logs: Vec<String>,
}

impl Pose {
    /// From a `mb.step()` timing.
    pub fn from_timing(t: &Value) -> Option<Pose> {
        let nums = |k: &str| t[k].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<f64>>());
        let strings = |k: &str| t[k].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect());
        let camera = nums("camera").filter(|v| v.len() >= 6);
        Some(Pose {
            frame: t["frame"].as_u64()? as u32,
            camera: camera.as_ref().and_then(|v| v[..6].try_into().ok()),
            projection: camera.as_ref().and_then(|v| v.get(6..9)?.try_into().ok()),
            track: nums("track").and_then(|v| v.try_into().ok()),
            // Not `Node::mark_subject`'s, which names a node rather than an impulse.
            // Nor ground::Figure's reports of where things stand (grounding.rs).
            marks: strings("marks").into_iter().filter(|m| !m.starts_with(crate::readability::SUBJECT_MARK) && !crate::grounding::is_ground(m)).collect(),
            logs: strings("logs"),
        })
    }
}

/// A run's poses at its frame rate.
pub struct Run {
    pub hz: f64,
    pub poses: Vec<Pose>,
}

/// |second difference| / dt² at each frame that has both neighbours (in
/// consecutive frames), as (game time, magnitude).
fn accel(points: &[(u32, [f64; 3])], hz: f64) -> Vec<(f64, f64)> {
    points
        .windows(3)
        .filter(|w| w[1].0 == w[0].0 + 1 && w[2].0 == w[1].0 + 1)
        .map(|w| {
            let d: Vec<f64> = (0..3).map(|k| w[2].1[k] - 2.0 * w[1].1[k] + w[0].1[k]).collect();
            (w[1].0 as f64 / hz, d.iter().map(|x| x * x).sum::<f64>().sqrt() * hz * hz)
        })
        .collect()
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The frames whose acceleration stands out from the frames around it,
/// as (time, magnitude), one per cluster.
fn spikes(a: &[(f64, f64)], hz: f64, floor: f64) -> Vec<(f64, f64)> {
    let w = (AROUND * hz).round() as usize;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for i in 0..a.len() {
        if a[i].1 < floor {
            continue;
        }
        // Skip the next-door frames: a jump in position is two frames wide.
        let mut before: Vec<f64> = a[i.saturating_sub(w)..i.saturating_sub(1)].iter().map(|x| x.1).collect();
        let mut after: Vec<f64> = a[(i + 2).min(a.len())..(i + w + 1).min(a.len())].iter().map(|x| x.1).collect();
        if a[i].1 < SPIKE * median(&mut before).max(median(&mut after)) {
            continue;
        }
        match out.last_mut() {
            Some(last) if a[i].0 - last.0 <= SAME_EVENT => {
                if a[i].1 > last.1 {
                    *last = a[i];
                }
            }
            _ => out.push(a[i]),
        }
    }
    out
}

/// The largest acceleration within `SAME_EVENT` of time `t`.
fn peak_near(a: &[(f64, f64)], t: f64) -> f64 {
    a.iter().filter(|x| (x.0 - t).abs() <= SAME_EVENT + 1e-9).map(|x| x.1).fold(0.0, f64::max)
}

/// A spike, with its peak at each rate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Event {
    pub t: f64,
    pub lo: f64,
    pub hi: f64,
    /// When the disturbance it's the worst of began (≤ `t`).
    pub start: f64,
    /// The game's `sys::mark` label near it: an impulse it meant.
    pub marked: Option<String>,
    /// What the game logged near it, as "f<frame> <line>" (frames at the lower rate).
    pub logs: Vec<String>,
}

/// What a channel measures, for its verdicts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    Position,
    Direction,
    Fov,
}

impl Event {
    /// What the growth from the lower rate to twice it says.
    pub fn verdict(&self, floor: f64, lo_hz: f64, hi_hz: f64, m: Measure) -> String {
        if self.lo < floor {
            return format!("only at {hi_hz} Hz");
        }
        if self.hi < floor {
            return format!("only at {lo_hz} Hz");
        }
        let (rate, change, jump) = match m {
            Measure::Position => ("velocity", "an instant turn or speed change", "position jumps (a snap or cut)"),
            Measure::Direction => ("turn rate", "a turn that starts or stops at once", "direction jumps (a snap or cut)"),
            Measure::Fov => ("zoom speed", "a zoom that starts or stops at once", "field of view jumps (a zoom pop)"),
        };
        match self.hi / self.lo {
            r if r < 1.4 => "the same at twice the rate: a sharp change, but continuous".into(),
            r if r < 2.8 => format!("~2× at twice the rate: the {rate} jumps within a frame ({change}, or smoothing that starts at full speed)"),
            _ => format!("~4× at twice the rate: the {jump}, or smoothing by a fixed fraction per frame"),
        }
    }
}

/// One thing that moves (the camera's position, its direction, a node), checked at two rates.
pub struct Channel {
    pub name: String,
    pub measure: Measure,
    pub unit: &'static str,
    pub floor: f64,
    pub events: Vec<Event>,
    /// The largest acceleration in each run.
    pub peak: [f64; 2],
}

fn channel(name: String, measure: Measure, unit: &'static str, floor: f64, lo: (&[(u32, [f64; 3])], f64), hi: (&[(u32, [f64; 3])], f64)) -> Channel {
    let (a_lo, a_hi) = (accel(lo.0, lo.1), accel(hi.0, hi.1));
    let mut times: Vec<f64> = spikes(&a_lo, lo.1, floor).into_iter().chain(spikes(&a_hi, hi.1, floor)).map(|s| s.0).collect();
    times.sort_by(f64::total_cmp);
    // Spikes within `AROUND` of a disturbance's first are that disturbance
    // (smoothing that settles after a jump spikes again as it decays): keep its worst.
    let mut events: Vec<Event> = Vec::new();
    let mut start = f64::NEG_INFINITY;
    for t in times {
        match events.last_mut() {
            Some(last) if t - start <= AROUND => {
                let e = Event { t, lo: peak_near(&a_lo, t), hi: peak_near(&a_hi, t), start, ..Event::default() };
                if e.lo.max(e.hi) > last.lo.max(last.hi) {
                    *last = e;
                }
            }
            _ => {
                start = t;
                events.push(Event { t, lo: peak_near(&a_lo, t), hi: peak_near(&a_hi, t), start, ..Event::default() });
            }
        }
    }
    events.sort_by(|a, b| b.lo.max(b.hi).total_cmp(&a.lo.max(a.hi)));
    let peak = |a: &[(f64, f64)]| a.iter().map(|x| x.1).fold(0.0, f64::max);
    Channel { name, measure, unit, floor, events, peak: [peak(&a_lo), peak(&a_hi)] }
}

/// How far apart the camera is at the same game time in the two runs: (median, max, time of the max).
fn apart(lo: &Run, hi: &Run) -> Option<(f64, f64, f64)> {
    let at_hi: std::collections::HashMap<u32, [f64; 6]> = hi.poses.iter().filter_map(|p| Some((p.frame, p.camera?))).collect();
    let mut d: Vec<(f64, f64)> = lo
        .poses
        .iter()
        .filter_map(|p| {
            let t = p.frame as f64 / lo.hz;
            let c = at_hi.get(&((t * hi.hz).round() as u32))?;
            let e = p.camera?;
            Some((t, (0..3).map(|k| (e[k] - c[k]).powi(2)).sum::<f64>().sqrt()))
        })
        .collect();
    if d.is_empty() {
        return None;
    }
    let max = d.iter().copied().fold((0.0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    let mut v: Vec<f64> = d.iter_mut().map(|x| x.1).collect();
    Some((median(&mut v), max.1, max.0))
}

pub struct Report {
    pub hz: [f64; 2],
    pub frames: [usize; 2],
    pub channels: Vec<Channel>,
    pub apart: Option<(f64, f64, f64)>,
    /// Near or far steps in the run at the lower rate: (time, "near" or "far", from, to).
    pub clip_steps: Vec<(f64, &'static str, f64, f64)>,
}

/// Frames where the near or far plane changes by more than `CLIP_STEP` of itself.
fn clip_steps(r: &Run) -> Vec<(f64, &'static str, f64, f64)> {
    let p: Vec<(u32, [f64; 3])> = r.poses.iter().filter_map(|p| Some((p.frame, p.projection?))).collect();
    let mut out = Vec::new();
    for w in p.windows(2).filter(|w| w[1].0 == w[0].0 + 1) {
        for (k, name) in [(1, "near"), (2, "far")] {
            let (a, b) = (w[0].1[k], w[1].1[k]);
            if (b - a).abs() > CLIP_STEP * a.abs().max(b.abs()).max(1e-6) {
                out.push((w[1].0 as f64 / r.hz, name, a, b));
            }
        }
    }
    out
}

/// Compares a run with the same run at twice the frame rate.
pub fn check(lo: &Run, hi: &Run, track: Option<u32>) -> Report {
    let pick = |r: &Run, f: &dyn Fn(&Pose) -> Option<[f64; 3]>| r.poses.iter().filter_map(|p| Some((p.frame, f(p)?))).collect::<Vec<_>>();
    let eye = |p: &Pose| p.camera.map(|c| [c[0], c[1], c[2]]);
    let look = |p: &Pose| p.camera.map(|c| [c[3], c[4], c[5]]);
    let fov = |p: &Pose| p.projection.map(|c| [c[0], 0.0, 0.0]);
    let node = |p: &Pose| p.track;
    let mut channels = Vec::new();
    let mut add = |name: String, measure, unit, floor, f: &dyn Fn(&Pose) -> Option<[f64; 3]>| {
        let (a, b) = (pick(lo, f), pick(hi, f));
        if !a.is_empty() || !b.is_empty() {
            channels.push(channel(name, measure, unit, floor, (&a, lo.hz), (&b, hi.hz)));
        }
    };
    add("camera position".into(), Measure::Position, "m/s²", FLOOR_POSITION, &eye);
    add("camera direction".into(), Measure::Direction, "rad/s²", FLOOR_DIRECTION, &look);
    add("camera fov".into(), Measure::Fov, "rad/s²", FLOOR_DIRECTION, &fov);
    if let Some(n) = track {
        add(format!("node {n}"), Measure::Position, "m/s²", FLOOR_POSITION, &node);
    }
    for e in channels.iter_mut().flat_map(|c| c.events.iter_mut()) {
        annotate(e, lo, hi);
    }
    Report { hz: [lo.hz, hi.hz], frames: [lo.poses.len(), hi.poses.len()], channels, apart: apart(lo, hi), clip_steps: clip_steps(lo) }
}

/// Adds the game's marks and log lines from `NOTES_AROUND` frames (at the
/// lower rate) before the disturbance began to as many after its worst spike.
fn annotate(e: &mut Event, lo: &Run, hi: &Run) {
    let pad = NOTES_AROUND as f64 / lo.hz + 1e-9;
    let near = |p: &Pose, hz: f64| (e.start - pad..=e.t + pad).contains(&(p.frame as f64 / hz));
    e.marked = [lo, hi].iter().find_map(|r| r.poses.iter().filter(|p| near(p, r.hz)).find_map(|p| p.marks.first().cloned()));
    e.logs = lo.poses.iter().filter(|p| near(p, lo.hz)).flat_map(|p| p.logs.iter().map(move |l| format!("f{} {l}", p.frame))).take(LOG_LINES).collect();
}

impl Report {
    pub fn text(&self) -> String {
        let [lo, hi] = self.hz;
        let mut out = format!("smoothness, {lo} Hz vs {hi} Hz ({} and {} frames):\n", self.frames[0], self.frames[1]);
        if self.channels.is_empty() {
            out += "  no mb3d camera in this run (the check is for 3D games)\n";
            return out;
        }
        for c in &self.channels {
            let (marked, spikes): (Vec<&Event>, Vec<&Event>) = c.events.iter().partition(|e| e.marked.is_some());
            let (n, m) = (spikes.len(), marked.len());
            let plural = |k: usize| if k == 1 { "" } else { "s" };
            if n == 0 && m == 0 {
                out += &format!("  {:<17} smooth (peak {:.1} {} at {lo} Hz, {:.1} at {hi} Hz)\n", c.name, c.peak[0], c.unit, c.peak[1]);
                continue;
            }
            out += &if n == 0 {
                format!("  {:<17} smooth but for {m} marked impulse{}:\n", c.name, plural(m))
            } else {
                let marked = if m > 0 { format!(" (and {m} marked)") } else { String::new() };
                format!("  {:<17} {n} spike{}{marked}{}\n", c.name, plural(n), if n > LIST { format!(", the worst {LIST}:") } else { ":".into() })
            };
            for e in spikes.iter().take(LIST).chain(marked.iter().take(LIST)) {
                let what = match &e.marked {
                    Some(label) => format!("marked: {label}"),
                    None => e.verdict(c.floor, lo, hi, c.measure),
                };
                out += &format!("    {:>7.3} s  {lo} Hz {:>7.1}  {hi} Hz {:>7.1} {}  {what}\n", e.t, e.lo, e.hi, c.unit);
                for l in &e.logs {
                    out += &format!("               log {l}\n");
                }
            }
        }
        if !self.clip_steps.is_empty() {
            let n = self.clip_steps.len();
            out += &format!("  camera clip       {n} step{} (things at the edge pop in or out):\n", if n == 1 { "" } else { "s" });
            for (t, which, a, b) in self.clip_steps.iter().take(LIST) {
                out += &format!("    {t:>7.3} s  {which} {a} → {b} m\n");
            }
        }
        if let Some((median, max, t)) = self.apart {
            let note = if median > APART { " (if the game should play the same at both rates, something moves per frame rather than per second)" } else { "" };
            out += &format!("  camera at {lo} vs {hi} Hz: {median:.3} m apart (median), {max:.3} m at most ({t:.2} s){note}\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run at `hz` for `secs`, the camera at `eye(t, dt, previous eye)` looking along `look(t)`.
    fn run(hz: f64, secs: f64, mut eye: impl FnMut(f64, f64, [f64; 3]) -> [f64; 3], look: impl Fn(f64) -> [f64; 3]) -> Run {
        let mut p = [0.0; 3];
        let poses = (1..=(secs * hz) as u32)
            .map(|f| {
                let t = f as f64 / hz;
                p = eye(t, 1.0 / hz, p);
                let l = look(t);
                Pose { frame: f, camera: Some([p[0], p[1], p[2], l[0], l[1], l[2]]), projection: Some([1.0, 0.1, 500.0]), ..Pose::default() }
            })
            .collect();
        Run { hz, poses }
    }

    fn ahead(_: f64) -> [f64; 3] {
        [0.0, 0.0, -1.0]
    }

    fn smoothstep(u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    }

    /// Forward at 12 m/s, swaying, with an eased lane change at 2 s and a turn eased over 1 s.
    fn smooth(t: f64, _: f64, _: [f64; 3]) -> [f64; 3] {
        let x = 0.3 * (t * 2.0).sin() + 2.0 * smoothstep((t - 2.0) / 0.3);
        [x, 1.5 + 0.05 * (t * 9.0).sin(), -12.0 * t]
    }

    fn turning(t: f64) -> [f64; 3] {
        let a = std::f64::consts::FRAC_PI_2 * smoothstep(t - 3.0);
        [a.sin(), 0.0, -a.cos()]
    }

    fn report(f: impl Fn(f64) -> Run) -> Report {
        check(&f(60.0), &f(120.0), None)
    }

    #[test]
    fn smooth_motion_is_quiet() {
        let r = report(|hz| run(hz, 6.0, smooth, turning));
        for c in &r.channels {
            assert!(c.events.is_empty(), "{}: {:?}", c.name, c.events);
        }
        assert!(r.apart.unwrap().1 < 1e-9, "the same path at both rates");
        let text = r.text();
        assert!(text.contains("camera position   smooth") && text.contains("camera direction  smooth") && text.contains("camera fov        smooth"), "{text}");
        assert!(!text.contains("camera clip"), "{text}");
    }

    #[test]
    fn a_velocity_kink_doubles_at_twice_the_rate() {
        // The camera slides sideways at 3 m/s and reverses within one frame at 2 s.
        let r = report(|hz| run(hz, 4.0, |t, _, _| [3.0 * (2.0 - (t - 2.0).abs()), 1.5, -12.0 * t], ahead));
        let e = &r.channels[0].events;
        assert_eq!(e.len(), 1, "{e:?}");
        assert!((e[0].t - 2.0).abs() < 0.02);
        assert!((e[0].lo - 360.0).abs() < 1.0, "6 m/s of change in 1/60 s: {}", e[0].lo);
        assert!((e[0].hi / e[0].lo - 2.0).abs() < 0.05);
        assert!(e[0].verdict(FLOOR_POSITION, 60.0, 120.0, Measure::Position).contains("velocity jumps"));
        assert!(r.channels[1].events.is_empty(), "the look direction is steady");
    }

    #[test]
    fn a_snap_quadruples() {
        let r = report(|hz| run(hz, 3.0, |t, _, _| [if t >= 1.5 { 0.5 } else { 0.0 }, 1.5, -12.0 * t], ahead));
        let e = &r.channels[0].events;
        assert_eq!(e.len(), 1, "one event, though a jump is two frames wide: {e:?}");
        assert!((e[0].hi / e[0].lo - 4.0).abs() < 0.1, "{e:?}");
        assert!(e[0].verdict(FLOOR_POSITION, 60.0, 120.0, Measure::Position).contains("position jumps"));
    }

    #[test]
    fn smoothing_by_a_fraction_per_frame_is_caught_two_ways() {
        // A follow camera lerping 20% of the way to its target each frame (the
        // target moves at 12 m/s and changes lane at 2 s): it spikes when the
        // lane changes and lags less at 120 Hz.
        let follow = |t: f64, _: f64, p: [f64; 3]| {
            let target = [if t >= 2.0 { 2.0 } else { 0.0 }, 1.5, -12.0 * t];
            std::array::from_fn(|k| p[k] + (target[k] - p[k]) * 0.2)
        };
        let r = report(|hz| run(hz, 4.0, follow, ahead));
        let e = &r.channels[0].events;
        assert!(e.iter().any(|e| (e.t - 2.0).abs() < 0.03 && e.hi / e.lo > 3.0), "{e:?}");
        let (median, ..) = r.apart.unwrap();
        assert!(median > APART, "it lags half as far at 120 Hz: {median}");
        assert!(r.text().contains("something moves per frame"));

        // The frame-rate-independent version: no difference between the rates,
        // but first-order smoothing still starts at full speed when the target jumps.
        let follow = |t: f64, dt: f64, p: [f64; 3]| {
            let target = [if t >= 2.0 { 2.0 } else { 0.0 }, 1.5, -12.0 * t];
            let k = 1.0 - (-dt / 0.08).exp();
            std::array::from_fn(|i| p[i] + (target[i] - p[i]) * k)
        };
        let r = report(|hz| run(hz, 4.0, follow, ahead));
        let e = r.channels[0].events.iter().find(|e| (e.t - 2.0).abs() < 0.03).expect("the lane change");
        assert!((e.hi / e.lo - 2.0).abs() < 0.3, "{e:?}");
    }

    /// Notes the game left on the frame ending nearest game time `t`.
    fn note(r: &mut Run, t: f64, mark: Option<&str>, log: Option<&str>) {
        let f = (t * r.hz).round() as u32;
        let p = r.poses.iter_mut().find(|p| p.frame == f).unwrap();
        p.marks.extend(mark.map(str::to_string));
        p.logs.extend(log.map(str::to_string));
    }

    #[test]
    fn a_marked_impulse_is_listed_but_not_counted_and_logs_say_what_happened() {
        // Two sideways velocity kinks: a jump the game marks (1 s) and a bump it doesn't (2 s).
        let kinks = |t: f64, _: f64, _: [f64; 3]| [3.0 * (t - 1.0).max(0.0) - 3.0 * (t - 2.0).max(0.0) * 2.0, 1.5, -12.0 * t];
        let (mut lo, mut hi) = (run(60.0, 3.0, kinks, ahead), run(120.0, 3.0, kinks, ahead));
        for r in [&mut lo, &mut hi] {
            // The game marks the frame that applies the impulse, a frame from the spike's centre.
            note(r, 1.0 + 1.0 / r.hz, Some("jump"), Some("info: jump from lane 1"));
            note(r, 2.0 - 1.0 / r.hz, None, Some("warn: bumped a crate"));
            note(r, 2.5, None, Some("info: far from both"));
        }
        let r = check(&lo, &hi, None);
        let e = &r.channels[0].events;
        assert_eq!(e.len(), 2, "{e:?}");
        let jump = e.iter().find(|e| (e.t - 1.0).abs() < 0.03).unwrap();
        assert_eq!(jump.marked.as_deref(), Some("jump"));
        assert_eq!(jump.logs, ["f61 info: jump from lane 1"], "the 60 Hz run's frames");
        let bump = e.iter().find(|e| (e.t - 2.0).abs() < 0.03).unwrap();
        assert_eq!((bump.marked.as_deref(), &bump.logs[..]), (None, &["f119 warn: bumped a crate".to_string()][..]));
        let text = r.text();
        assert!(text.contains("camera position   1 spike (and 1 marked):"), "{text}");
        assert!(text.contains("marked: jump\n               log f61 info: jump from lane 1\n"), "{text}");
        assert!(text.contains("velocity jumps") && text.contains("log f119 warn: bumped a crate"), "{text}");
        assert!(!text.contains("far from both"), "{text}");

        // Only the marked one: smooth but for it.
        let one = |t: f64, _: f64, _: [f64; 3]| [3.0 * (t - 1.0).max(0.0), 1.5, -12.0 * t];
        let (mut lo, mut hi) = (run(60.0, 2.0, one, ahead), run(120.0, 2.0, one, ahead));
        note(&mut lo, 1.0, Some("dash"), None);
        note(&mut hi, 1.0, Some("dash"), None);
        let text = check(&lo, &hi, None).text();
        assert!(text.contains("camera position   smooth but for 1 marked impulse:"), "{text}");
    }

    #[test]
    fn an_instant_turn_is_a_direction_spike() {
        let r = report(|hz| run(hz, 3.0, smooth, |t| if t >= 1.0 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, -1.0] }));
        let e = &r.channels[1].events;
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(r.text().contains("camera direction  1 spike:"), "{}", r.text());
    }

    /// A run whose field of view is `fov(t)` and near plane `near(t)`, the camera otherwise smooth.
    fn zooming(hz: f64, fov: impl Fn(f64) -> f64, near: impl Fn(f64) -> f64) -> Run {
        let mut r = run(hz, 4.0, smooth, turning);
        for p in &mut r.poses {
            let t = p.frame as f64 / hz;
            p.projection = Some([fov(t), near(t), 500.0]);
        }
        r
    }

    #[test]
    fn a_field_of_view_that_pops_is_a_spike() {
        // A boost's zoom: +0.12 rad the frame it starts at 2 s (as a builder's camera did).
        let pop = |t: f64| if t >= 2.0 { 1.12 } else { 1.0 };
        let r = check(&zooming(60.0, pop, |_| 0.1), &zooming(120.0, pop, |_| 0.1), None);
        let c = r.channels.iter().find(|c| c.name == "camera fov").unwrap();
        assert_eq!(c.events.len(), 1, "{:?}", c.events);
        let e = &c.events[0];
        assert!((e.t - 2.0).abs() < 0.02 && (e.hi / e.lo - 4.0).abs() < 0.2, "{e:?}");
        let text = r.text();
        assert!(text.contains("camera fov        1 spike:") && text.contains("the field of view jumps (a zoom pop)"), "{text}");
        assert!(r.channels.iter().filter(|c| c.name != "camera fov").all(|c| c.events.is_empty()), "only the fov moved: {text}");

        // The same zoom eased over half a second: smooth.
        let eased = |t: f64| 1.0 + 0.12 * smoothstep((t - 2.0) / 0.5);
        let r = check(&zooming(60.0, eased, |_| 0.1), &zooming(120.0, eased, |_| 0.1), None);
        assert!(r.channels.iter().all(|c| c.events.is_empty()), "{}", r.text());
        assert!(r.text().contains("camera fov        smooth"), "{}", r.text());
    }

    #[test]
    fn a_clip_plane_that_steps_is_listed() {
        let near = |t: f64| if t >= 1.5 { 0.5 } else { 0.1 };
        let r = check(&zooming(60.0, |_| 1.0, near), &zooming(120.0, |_| 1.0, near), None);
        assert_eq!(r.clip_steps, vec![(1.5, "near", 0.1, 0.5)]);
        assert!(r.text().contains("camera clip       1 step (things at the edge pop in or out):\n      1.500 s  near 0.1 → 0.5 m\n"), "{}", r.text());
    }

    #[test]
    fn poses_come_from_step_timings() {
        let t = serde_json::json!({ "frame": 7, "camera": [1, 2, 3, 0, 0, -1, 0.9, 0.1, 400], "track": [4, 5, 6], "marks": ["jump"], "logs": ["info: hop"] });
        let want = Pose {
            frame: 7,
            camera: Some([1.0, 2.0, 3.0, 0.0, 0.0, -1.0]),
            projection: Some([0.9, 0.1, 400.0]),
            track: Some([4.0, 5.0, 6.0]),
            marks: vec!["jump".into()],
            logs: vec!["info: hop".into()],
        };
        assert_eq!(Pose::from_timing(&t), Some(want));
        let old = Pose::from_timing(&serde_json::json!({ "frame": 1, "camera": [1, 2, 3, 0, 0, -1] })).unwrap();
        assert_eq!((old.camera.is_some(), old.projection), (true, None), "a pose without the projection still has the camera");
        let flat = Pose::from_timing(&serde_json::json!({ "frame": 1 })).unwrap();
        assert_eq!((flat.camera, flat.track), (None, None), "a frame without mb3d");
        let r = check(&Run { hz: 60.0, poses: vec![flat.clone()] }, &Run { hz: 120.0, poses: vec![flat] }, None);
        assert!(r.text().contains("no mb3d camera"));
    }
}
