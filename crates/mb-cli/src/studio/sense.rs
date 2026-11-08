//! `mb play`'s scripted sensors (docs/STUDIO-2.md §5 [input], Phase 3e):
//! the script's `tilt`, `aim`, `shake` and `loud` items as the replay log's
//! sensor readings (SPEC §5.7, runtime/src/replay.ts `SensorDelta`), frame
//! by frame, as a phone would send them. The runtime replays them like a
//! recorded session, so the game reads them through the same imports.
//!
//!     { "at": 1, "tilt": [-20, 10], "for": 0.4 }   roll and pitch (tipped back), degrees from upright
//!     { "at": 2, "aim": [15, -5], "for": 0.3 }     yaw right and pitch up, degrees from where it started
//!     { "at": 3, "shake": 2, "for": 0.5 }          a shake of 2 g side to side (8 a second)
//!     { "at": 4, "loud": 0.85, "for": 1 }          the mic's loudness (blowing), back to the room after
//!
//! A turn with `for` goes there as a hand does (minimum jerk, with the
//! rotation rate a gyro would report); without, at once. Until the first,
//! the phone is upright, still and facing ahead, and the mic hears a quiet
//! room with the game's music (0.45, as an iPhone 16 Pro measured: SPEC
//! §5.7). Only the sensors the game declares are sent, quantized as the
//! platform does.

use serde_json::{Value, json};

use mb_format::manifest::Sensor;

use crate::shot::{min_jerk, min_jerk_speed, pose, qmul};

/// What a quiet room with the game's music playing reads (SPEC §5.7).
pub const ROOM: f64 = 0.45;
/// A scripted shake's rate, back and forth a second.
const SHAKE_HZ: f64 = 8.0;

/// One turn of the phone: from where the last left it to `to` (yaw, up,
/// roll in degrees), over `dur` seconds from `at`.
#[derive(Clone, Copy, Debug)]
struct Turn {
    at: f64,
    dur: f64,
    to: [Option<f64>; 3],
}

/// The script's sensor items, ready to sample.
#[derive(Debug, Default)]
pub struct Track {
    turns: Vec<Turn>,
    /// (from, to, g)
    shakes: Vec<(f64, f64, f64)>,
    /// (from, until or None, level)
    louds: Vec<(f64, Option<f64>, f64)>,
}

/// The sensors a 2.0 project's studio.toml lists (unknown names left out).
pub fn listed(t: &toml::Table) -> Vec<Sensor> {
    let names = t.get("sensors").and_then(toml::Value::as_array).cloned().unwrap_or_default();
    names
        .iter()
        .filter_map(|s| match s.as_str()? {
            "tilt" => Some(Sensor::Tilt),
            "motion" => Some(Sensor::Motion),
            "attitude" => Some(Sensor::Attitude),
            "loudness" => Some(Sensor::Loudness),
            "haptics" => Some(Sensor::Haptics),
            _ => None,
        })
        .collect()
}

/// Whether a script item is a sensor's.
pub fn is_sensor(item: &Value) -> bool {
    ["tilt", "aim", "shake", "loud"].iter().any(|k| item.get(*k).is_some())
}

impl Track {
    /// The script's sensor items (`what` names an item for errors). The
    /// game's `sensors` say which may be scripted.
    pub fn parse(script: &Value, sensors: &[Sensor]) -> Result<Track, String> {
        let mut t = Track::default();
        let Value::Array(list) = script else { return Ok(t) };
        let has = |s: Sensor| sensors.contains(&s);
        for (i, s) in list.iter().enumerate().filter(|(_, s)| is_sensor(s)) {
            let what = format!("script item {}", i + 1);
            let at = s["at"].as_f64().filter(|a| *a >= 0.0).ok_or_else(|| format!("{what}: no \"at\" (seconds)"))?;
            let dur = match s.get("for") {
                None => None,
                Some(v) => Some(v.as_f64().filter(|d| *d > 0.0 && *d <= 60.0).ok_or_else(|| format!("{what}: \"for\" is seconds above 0"))?),
            };
            let angles = |k: &str, limit: f64| -> Result<(f64, Option<f64>), String> {
                let p: Vec<f64> = match &s[k] {
                    Value::Number(n) => vec![n.as_f64().unwrap_or(0.0)],
                    Value::Array(a) => a.iter().filter_map(Value::as_f64).collect(),
                    _ => Vec::new(),
                };
                if p.is_empty() || p.len() > 2 || p.iter().any(|v| v.abs() > limit) {
                    return Err(format!("{what}: \"{k}\" is [a, b] in degrees (±{limit})"));
                }
                Ok((p[0], p.get(1).copied()))
            };
            if s.get("tilt").is_some() {
                if !has(Sensor::Tilt) && !has(Sensor::Attitude) {
                    return Err(format!("{what}: the game doesn't read tilt (sensors = [\"tilt\"] in studio.toml)"));
                }
                let (roll, back) = angles("tilt", 90.0)?;
                // Tipped back is pointing the phone's back down: the attitude's pitch, negated.
                t.turns.push(Turn { at, dur: dur.unwrap_or(0.0), to: [None, Some(-back.unwrap_or(0.0)), Some(roll)] });
            } else if s.get("aim").is_some() {
                if !has(Sensor::Attitude) {
                    return Err(format!("{what}: the game doesn't read the attitude (sensors = [\"attitude\"] in studio.toml)"));
                }
                let (yaw, up) = angles("aim", 180.0)?;
                t.turns.push(Turn { at, dur: dur.unwrap_or(0.0), to: [Some(yaw), Some(up.unwrap_or(0.0)), None] });
            } else if s.get("shake").is_some() {
                if !has(Sensor::Motion) {
                    return Err(format!("{what}: the game doesn't read motion (sensors = [\"motion\"] in studio.toml)"));
                }
                let g = s["shake"].as_f64().filter(|g| *g > 0.0 && *g <= 8.0).ok_or_else(|| format!("{what}: \"shake\" is how hard, in g (2 is brisk)"))?;
                t.shakes.push((at, at + dur.unwrap_or(0.5), g));
            } else {
                if !has(Sensor::Loudness) {
                    return Err(format!("{what}: the game doesn't read loudness (sensors = [\"loudness\"] in studio.toml)"));
                }
                let level = s["loud"].as_f64().filter(|l| (0.0..=1.0).contains(l)).ok_or_else(|| format!("{what}: \"loud\" is 0–1 (a hard blow is 0.85)"))?;
                t.louds.push((at, dur.map(|d| at + d), level));
            }
        }
        t.turns.sort_by(|a, b| a.at.total_cmp(&b.at));
        for w in t.turns.windows(2) {
            if w[1].at < w[0].at + w[0].dur - 1e-9 {
                return Err(format!("the phone turns again at {} s before its turn from {} s has finished: one turn at a time", w[1].at, w[0].at));
            }
        }
        t.louds.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(t)
    }

    /// The phone's yaw, up and roll (degrees) and their rates (°/s) at `time`.
    fn phone(&self, time: f64) -> ([f64; 3], [f64; 3]) {
        let mut at = [0.0f64; 3];
        let mut rate = [0.0f64; 3];
        for turn in &self.turns {
            if turn.at > time + 1e-9 {
                break;
            }
            let from = at;
            let u = if turn.dur > 0.0 { ((time - turn.at) / turn.dur).clamp(0.0, 1.0) } else { 1.0 };
            let (e, de) = (min_jerk(u), if turn.dur > 0.0 && u < 1.0 { min_jerk_speed(u) / turn.dur } else { 0.0 });
            rate = [0.0; 3];
            for k in 0..3 {
                if let Some(to) = turn.to[k] {
                    at[k] = from[k] + (to - from[k]) * e;
                    rate[k] = (to - from[k]) * de;
                }
            }
        }
        (at, rate)
    }

    /// The readings at `time`: tilt (gravity), motion, attitude (quaternion
    /// and rate), loudness.
    pub fn sample(&self, time: f64) -> ([f64; 3], [f64; 3], [f64; 7], f64) {
        let ([yaw, up, roll], [dyaw, dup, droll]) = self.phone(time);
        let q = pose(yaw.to_radians(), up.to_radians(), roll.to_radians());
        let [x, y, z, w] = q;
        let g = [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)];
        // The rate a gyro reports: the turn over a tiny step, in the phone's axes.
        let h = 1e-4;
        let q1 = pose((yaw + dyaw * h).to_radians(), (up + dup * h).to_radians(), (roll + droll * h).to_radians());
        let d = qmul([-q[0], -q[1], -q[2], q[3]], q1);
        let r = [2.0 * d[0] / h, 2.0 * d[1] / h, 2.0 * d[2] / h];
        let mut motion = [0.0; 3];
        for &(from, to, gs) in &self.shakes {
            if (from..to).contains(&time) {
                motion[0] += gs * (std::f64::consts::TAU * SHAKE_HZ * (time - from)).sin();
            }
        }
        let mut loud = ROOM;
        for &(from, until, level) in &self.louds {
            if time + 1e-9 >= from && until.is_none_or(|u| time < u) {
                loud = level;
            }
        }
        (g, motion, [q[0], q[1], q[2], q[3], r[0], r[1], r[2]], loud)
    }

    /// Each frame's sensor fields for the log (only what changed, as the
    /// runtime records them), for a game reading `sensors`, at `hz`.
    pub fn frames(&self, frames: usize, hz: f64, sensors: &[Sensor]) -> Vec<serde_json::Map<String, Value>> {
        let has = |s: Sensor| sensors.contains(&s);
        let q = |v: f64, k: f64| (v * k).round() / k;
        let mut last: [Option<Value>; 4] = [None, None, None, None];
        (0..frames)
            .map(|f| {
                let (g, m, a, l) = self.sample(f as f64 / hz);
                let mut out = serde_json::Map::new();
                let now = [
                    has(Sensor::Tilt).then(|| json!(g.map(|v| q(v, 4096.0)))),
                    has(Sensor::Motion).then(|| json!(m.map(|v| q(v, 4096.0)))),
                    has(Sensor::Attitude).then(|| {
                        let mut v: Vec<f64> = a[..4].iter().map(|c| q(*c, 32768.0)).collect();
                        v.extend(a[4..].iter().map(|c| q(*c, 1024.0)));
                        json!(v)
                    }),
                    has(Sensor::Loudness).then(|| json!(q(l, 1024.0))),
                ];
                for (k, (key, v)) in ["tilt", "motion", "att", "loud"].into_iter().zip(now).enumerate() {
                    if let Some(v) = v
                        && last[k].as_ref() != Some(&v)
                    {
                        out.insert(key.into(), v.clone());
                        last[k] = Some(v);
                    }
                }
                out
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[Sensor] = &[Sensor::Tilt, Sensor::Motion, Sensor::Attitude, Sensor::Loudness];

    #[test]
    fn upright_still_and_a_quiet_room_until_the_script_says() {
        let t = Track::parse(&json!([]), ALL).unwrap();
        let f = t.frames(3, 60.0, ALL);
        assert_eq!(f[0]["tilt"], json!([0.0, 1.0, 0.0]));
        assert_eq!(f[0]["motion"], json!([0.0, 0.0, 0.0]));
        assert_eq!(f[0]["att"], json!([0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]));
        assert_eq!(f[0]["loud"], json!(461.0 / 1024.0), "a quiet room, quantized as the platform does");
        assert!(f[1].is_empty() && f[2].is_empty(), "only changes are sent");
        // A game that reads only tilt gets only tilt.
        let f = t.frames(1, 60.0, &[Sensor::Tilt]);
        assert_eq!(f[0].keys().collect::<Vec<_>>(), ["tilt"]);
    }

    #[test]
    fn a_tilt_turns_there_as_a_hand_does() {
        let t = Track::parse(&json!([{ "at": 1, "tilt": [30], "for": 0.5 }, { "at": 2, "tilt": [-90, 0] }]), ALL).unwrap();
        let (g, ..) = t.sample(0.9);
        assert!(g[0].abs() < 1e-9);
        let (g, _, a, _) = t.sample(1.25);
        assert!((g[0] - 15f64.to_radians().sin()).abs() < 1e-6, "halfway: {g:?}");
        assert!(a[6].abs() > 0.5, "turning about the screen's axis: {a:?}");
        let (g, _, a, _) = t.sample(1.6);
        assert!((g[0] - 0.5).abs() < 1e-9 && a[6] == 0.0, "there, and still: {g:?} {a:?}");
        let (g, ..) = t.sample(2.0);
        assert!((g[0] + 1.0).abs() < 1e-9, "at once: {g:?}");
        // Tipped back 30°: as mb shot's --tilt 0,30.
        let t = Track::parse(&json!([{ "at": 0, "tilt": [0, 30] }]), ALL).unwrap();
        let (g, ..) = t.sample(0.0);
        assert!(g[0].abs() < 1e-9 && (g[1] - 0.75f64.sqrt()).abs() < 1e-9 && (g[2] + 0.5).abs() < 1e-9, "{g:?}");
    }

    #[test]
    fn aims_shakes_and_blows() {
        let t = Track::parse(&json!([{ "at": 0, "aim": [20, 10] }, { "at": 1, "shake": 2, "for": 0.5 }, { "at": 2, "loud": 0.85, "for": 1 }]), ALL).unwrap();
        let (_, _, a, _) = t.sample(0.5);
        let att = maimbrain::sensors::Attitude { q: [a[0] as f32, a[1] as f32, a[2] as f32, a[3] as f32], rate: [0.0; 3] };
        assert!((att.yaw().to_degrees() - 20.0).abs() < 0.01 && (att.pitch().to_degrees() - 10.0).abs() < 0.01, "{} {}", att.yaw().to_degrees(), att.pitch().to_degrees());
        let peak = (0..30).map(|k| t.sample(1.0 + k as f64 / 60.0).1[0].abs()).fold(0.0, f64::max);
        assert!((peak - 2.0).abs() < 0.05, "{peak}");
        assert_eq!(t.sample(1.6).1, [0.0; 3]);
        assert_eq!((t.sample(1.9).3, t.sample(2.5).3, t.sample(3.0).3), (ROOM, 0.85, ROOM));
    }

    #[test]
    fn says_what_the_game_must_declare() {
        let e = Track::parse(&json!([{ "at": 1, "loud": 0.8 }]), &[Sensor::Tilt]).unwrap_err();
        assert!(e.contains("sensors = [\"loudness\"]"), "{e}");
        let e = Track::parse(&json!([{ "at": 1, "aim": [10] }]), &[Sensor::Tilt]).unwrap_err();
        assert!(e.contains("attitude"), "{e}");
        let e = Track::parse(&json!([{ "at": 1, "tilt": [10], "for": 1 }, { "at": 1.5, "tilt": [0] }]), ALL).unwrap_err();
        assert!(e.contains("one turn at a time"), "{e}");
        assert!(Track::parse(&json!([{ "at": 1, "tilt": [100] }]), ALL).unwrap_err().contains("±90"));
    }
}
