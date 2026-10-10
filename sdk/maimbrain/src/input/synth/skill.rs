//! How precisely real people use each kind of input, for bots: reaction
//! times, Fitts'-law movement times with the scatter that comes with them,
//! timing noise, tremor and swipes, in four presets from a first-timer to
//! an expert. A bot built on these misses about as often as a person would,
//! so its win rates mean something. They're starting points from the
//! literature, not measurements of your game: check them against a recorded
//! human run (SKILL.md, "Difficulty sanity check").
//!
//! Sources (the numbers below are rounded from these and placed between
//! them by skill):
//! - Reaction: simple visual reaction time in young adults averages about
//!   0.21–0.23 s (Woods et al. 2015, "Factors influencing the latency of
//!   simple reaction time", Front. Hum. Neurosci.); choosing between
//!   responses adds time with the number of choices (Hick 1952; Hyman
//!   1953). A game's "something new happened, act on it" sits between the
//!   two: 0.23 s for an expert who expects it, 0.4+ s for a first-timer.
//! - Pointing: movement time MT = a + log2(D/W + 1) / TP (Fitts 1954, in
//!   MacKenzie's Shannon form), with the endpoint spread that goes with it:
//!   a target of effective width We = 4.133 σ is hit about 96% of the time
//!   (ISO 9241-9; Soukoreff & MacKenzie 2004).
//! - Touch: finger taps on a touchscreen reach about 6–7 bits/s (MacKenzie
//!   2015, "Fitts' throughput and the remarkable case of touch-based target
//!   selection"), and a finger has an absolute precision σa of about 1.5 mm
//!   however slowly it moves (Bi, Li & Zhai 2013, "FFitts law"; Holz &
//!   Baudisch 2010 measured similar offsets): about 8–10 points on an
//!   iPhone. Apple's 44 pt minimum target follows from it.
//! - Pointing the phone (a light gun, `sensors::Scope`): pointing with a
//!   held remote measures about 2–3 bits/s in ISO 9241-9 studies (Natapov,
//!   Castellucci & MacKenzie 2009, "ISO 9241-9 evaluation of video game
//!   controllers"; McArthur, Castellucci & MacKenzie 2009 on Wiimote gun
//!   attachments), well under a finger, plus the hand's physiological tremor
//!   (8–12 Hz; a few hundredths to a tenth of a degree on a held device).
//! - Tilt (roll to steer): position-control tilt measures roughly 1.5–2.5
//!   bits/s (MacKenzie & Teather 2012, "FittsTilt"; Teather & MacKenzie
//!   2014), and holding an angle wanders by about a degree.
//! - Timing: tapping in time with a beat or a predictable event varies by
//!   about 20–50 ms (SD) in non-musicians, and 10–20 ms in trained players
//!   (Repp 2005, "Sensorimotor synchronization: a review of the tapping
//!   literature").
//! - Swipes: a thumb flick lasts about 0.08–0.25 s; its direction varies
//!   by several degrees (an estimate: measure your own players).

use super::super::Event;
use super::{AimPath, flick, tap};
use crate::Rng;

/// A Fitts'-law model of one way of pointing (a finger on glass, the phone
/// turned to aim, the phone tilted): how long a move takes and where it
/// lands. Units are the input's own: logical units (≈ points) for touch,
/// degrees for pointing and tilt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointing {
    /// The part of every move that doesn't depend on its difficulty (s).
    pub a: f32,
    /// Throughput (bits/s): how much difficulty a second of movement buys.
    pub throughput: f32,
    /// The scatter (1 SD) that remains however slowly the move is made
    /// (FFitts' σa for a finger; tremor and smoothing for a held phone).
    pub floor: f32,
    /// How far past the target a quick first move goes, as a fraction of
    /// its distance (typical; draw around it).
    pub overshoot: f32,
}

impl Pointing {
    /// How long (s) a move of `distance` takes to land on a target `width`
    /// across about 96% of the time. A target narrower than the floor
    /// allows (4.133 × `floor`) can't be hit that often at any speed: the
    /// time is then capped at 3 bits past where it stops paying.
    /// ```
    /// use maimbrain::input::synth::Skill;
    /// let finger = Skill::good().touch;
    /// assert!(finger.time(200.0, 44.0) > finger.time(100.0, 44.0));
    /// assert!(finger.time(100.0, 22.0) > finger.time(100.0, 44.0));
    /// ```
    pub fn time(&self, distance: f32, width: f32) -> f32 {
        // FFitts: only the part of the width the floor doesn't use up buys speed.
        let w = width.max(1e-6);
        let usable = (w * w - (4.133 * self.floor).powi(2)).max(0.0).sqrt().max(w / 8.0);
        self.a + (distance.abs() / usable + 1.0).log2() / self.throughput
    }

    /// Where a move of `distance` made in `time` seconds lands: the SD of
    /// its endpoint along the move (and about the same across it). Faster
    /// is wider; slow moves come down to `floor`.
    /// ```
    /// use maimbrain::input::synth::Skill;
    /// let gun = Skill::casual().point;
    /// assert!(gun.scatter(15.0, 0.4) > gun.scatter(15.0, 1.0));
    /// assert!((gun.scatter(15.0, 5.0) - gun.floor).abs() < 0.01);
    /// ```
    pub fn scatter(&self, distance: f32, time: f32) -> f32 {
        let bits = ((time - self.a).max(0.0) * self.throughput).min(30.0);
        let we = distance.abs() / (2f32.powf(bits) - 1.0).max(1e-3);
        (we / 4.133).hypot(self.floor)
    }
}

/// A swipe as a kind of player makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swipe {
    /// How long the finger is down (s): a range to draw from.
    pub duration: (f32, f32),
    /// How far off the intended direction it goes (degrees, 1 SD).
    pub angle: f32,
}

/// How a kind of player performs with each input. Use the presets
/// (`first_timer`, `casual`, `good`, `expert`, or `Skill::ALL`) and let a
/// bot take its reaction, aim and timing from them, so its numbers are tied
/// to how precise people really are.
/// ```
/// use maimbrain::input::synth::Skill;
/// for s in Skill::ALL {
///     println!("{}: reacts in {:.2} s, taps within {:.0} pt", s.name, s.reaction.0, s.touch.floor);
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    pub name: &'static str,
    /// Reacting to something new on screen (s): mean and SD.
    pub reaction: (f32, f32),
    /// Looking away: the chance per new situation, and for how long (s, typical).
    pub lapse: (f32, f32),
    /// Timing a press to a predictable moment (a beat, a gap, contact): SD (s).
    pub timing: f32,
    /// A finger tapping the glass (logical units).
    pub touch: Pointing,
    /// Pointing the phone to aim (degrees of turn, `sensors::Scope`).
    pub point: Pointing,
    /// The hand's tremor while holding the phone on target (degrees, RMS).
    pub tremor: f32,
    /// Tilting the phone to steer (degrees of roll).
    pub tilt: Pointing,
    pub swipe: Swipe,
}

impl Skill {
    /// The four presets, least skilled first.
    pub const ALL: [Skill; 4] = [Skill::first_timer(), Skill::casual(), Skill::good(), Skill::expert()];

    /// Playing it for the first time: reacting late, moving carefully,
    /// sometimes looking away. A game's first 20–45 s are for them.
    pub const fn first_timer() -> Skill {
        Skill {
            name: "first-timer",
            reaction: (0.42, 0.08),
            lapse: (0.08, 0.7),
            timing: 0.07,
            touch: Pointing { a: 0.18, throughput: 4.5, floor: 11.0, overshoot: 0.15 },
            point: Pointing { a: 0.25, throughput: 1.8, floor: 0.35, overshoot: 0.12 },
            tremor: 0.1,
            tilt: Pointing { a: 0.3, throughput: 1.4, floor: 1.5, overshoot: 0.2 },
            swipe: Swipe { duration: (0.16, 0.26), angle: 12.0 },
        }
    }

    /// Plays phone games, has played this one a few times.
    pub const fn casual() -> Skill {
        Skill {
            name: "casual",
            reaction: (0.34, 0.06),
            lapse: (0.05, 0.6),
            timing: 0.05,
            touch: Pointing { a: 0.14, throughput: 5.5, floor: 9.5, overshoot: 0.1 },
            point: Pointing { a: 0.2, throughput: 2.3, floor: 0.25, overshoot: 0.09 },
            tremor: 0.07,
            tilt: Pointing { a: 0.25, throughput: 1.8, floor: 1.1, overshoot: 0.15 },
            swipe: Swipe { duration: (0.13, 0.2), angle: 9.0 },
        }
    }

    /// Knows the game and plays it well: expects what's coming.
    pub const fn good() -> Skill {
        Skill {
            name: "good",
            reaction: (0.28, 0.045),
            lapse: (0.02, 0.5),
            timing: 0.035,
            touch: Pointing { a: 0.12, throughput: 6.5, floor: 8.5, overshoot: 0.07 },
            point: Pointing { a: 0.15, throughput: 2.8, floor: 0.18, overshoot: 0.06 },
            tremor: 0.05,
            tilt: Pointing { a: 0.2, throughput: 2.2, floor: 0.8, overshoot: 0.1 },
            swipe: Swipe { duration: (0.1, 0.16), angle: 7.0 },
        }
    }

    /// The best players a game will meet: near the human limits.
    pub const fn expert() -> Skill {
        Skill {
            name: "expert",
            reaction: (0.23, 0.035),
            lapse: (0.01, 0.4),
            timing: 0.022,
            touch: Pointing { a: 0.1, throughput: 7.5, floor: 7.5, overshoot: 0.05 },
            point: Pointing { a: 0.12, throughput: 3.3, floor: 0.12, overshoot: 0.04 },
            tremor: 0.04,
            tilt: Pointing { a: 0.15, throughput: 2.6, floor: 0.6, overshoot: 0.07 },
            swipe: Swipe { duration: (0.08, 0.13), angle: 5.0 },
        }
    }

    /// A reaction time (s), drawn: right-skewed as real ones are, never
    /// under 0.15 s.
    pub fn reaction_time(&self, rng: &mut Rng) -> f32 {
        let g = gauss(rng);
        // A lognormal with this mean and SD.
        let (m, s) = self.reaction;
        let v = (1.0 + (s / m).powi(2)).ln();
        (m * (-0.5 * v + v.sqrt() * g).exp()).max(0.15)
    }

    /// A hand pointing the phone from `from` to a target at `to` that is
    /// `size` degrees across (yaw right, pitch up), starting at `t0`: the
    /// swing takes the Fitts time for this player, overshoots by about
    /// `point.overshoot`, settles where its scatter puts it (not exactly
    /// on `to`) and trembles by `tremor`. Seeded. Feed `at(t)` to
    /// `Scope::update_with`; `landed()` is when it settles.
    /// ```
    /// use maimbrain::input::synth::Skill;
    /// let swing = Skill::good().aim((0.0, 0.0), (10.0, 0.0), 4.0, 1.0, 7);
    /// let (yaw, _) = swing.angles(swing.landed() + 0.5);
    /// assert!((yaw - 10.0).abs() < 2.0 && swing.landed() > 1.3);
    /// ```
    pub fn aim(&self, from: (f32, f32), to: (f32, f32), size: f32, t0: impl Into<f64>, seed: u64) -> AimPath {
        let mut rng = Rng::new(seed);
        let d = (to.0 - from.0).hypot(to.1 - from.1);
        let time = self.point.time(d, size);
        let sd = self.point.scatter(d, time);
        let err = (gauss(&mut rng) * sd, gauss(&mut rng) * sd);
        let overshoot = (self.point.overshoot * rng.range(0.5, 1.5)).max(0.0);
        AimPath::new(from, (to.0 + err.0, to.1 + err.1), t0.into(), time as f64, overshoot, self.tremor / std::f32::consts::SQRT_2, &mut rng)
    }

    /// A tap aimed at a target `size` units across at `to`, the finger
    /// starting from `from` at `t0`: it lands after this player's Fitts
    /// time, scattered as a finger is. Returns the touches and where it
    /// landed.
    /// ```
    /// use maimbrain::input::synth::Skill;
    /// use maimbrain::Rng;
    /// let (touches, at) = Skill::casual().tap((180.0, 560.0), (120.0, 300.0), 60.0, 2.0, &mut Rng::new(1));
    /// assert!(touches[0].is_press() && touches[0].time > 2.3);
    /// assert!((at.0 - 120.0).hypot(at.1 - 300.0) < 60.0);
    /// ```
    pub fn tap(&self, from: (f32, f32), to: (f32, f32), size: f32, t0: impl Into<f64>, rng: &mut Rng) -> (Vec<Event>, (f32, f32)) {
        let d = (to.0 - from.0).hypot(to.1 - from.1);
        let time = self.touch.time(d, size);
        let sd = self.touch.scatter(d, time);
        let at = (to.0 + gauss(rng) * sd, to.1 + gauss(rng) * sd);
        (tap(at.0, at.1, t0.into() + time as f64), at)
    }

    /// A swipe from `from` meant to go `length` units at `angle` (radians,
    /// 0 = right, y down), starting at `t0`: its duration and direction drawn
    /// for this player, made with [`flick`](super::flick).
    pub fn flick(&self, from: (f32, f32), angle: f32, length: f32, t0: impl Into<f64>, rng: &mut Rng) -> Vec<Event> {
        let dur = rng.range(self.swipe.duration.0, self.swipe.duration.1);
        let a = angle + (gauss(rng) * self.swipe.angle).to_radians();
        let to = (from.0 + a.cos() * length, from.1 + a.sin() * length);
        flick(from, to, t0, dur as f64, rng.next_u32() as u64)
    }

    /// A press timed for a moment the player can see coming (a beat, a gap,
    /// contact) at `due`: early or late by this player's timing noise.
    pub fn timed(&self, due: f64, rng: &mut Rng) -> f64 {
        due + (gauss(rng) * self.timing) as f64
    }
}

/// Roughly normal, mean 0, SD 1 (the sum of four uniforms).
pub fn gauss(rng: &mut Rng) -> f32 {
    ((0..4).map(|_| rng.f32()).sum::<f32>() - 2.0) * 1.732
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_sd(v: &[f32]) -> (f32, f32) {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (m, (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt())
    }

    #[test]
    fn the_presets_get_better_in_every_way_from_first_timer_to_expert() {
        for w in Skill::ALL.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(b.reaction.0 < a.reaction.0 && b.timing < a.timing && b.lapse.0 < a.lapse.0, "{} → {}", a.name, b.name);
            for (pa, pb) in [(a.touch, b.touch), (a.point, b.point), (a.tilt, b.tilt)] {
                assert!(pb.throughput > pa.throughput && pb.a < pa.a && pb.floor < pa.floor && pb.overshoot < pa.overshoot, "{} → {}", a.name, b.name);
            }
            assert!(b.tremor < a.tremor && b.swipe.angle < a.swipe.angle && b.swipe.duration.1 < a.swipe.duration.1);
        }
    }

    #[test]
    fn the_presets_stay_inside_the_published_human_ranges() {
        for s in Skill::ALL {
            // Visual reaction: no one averages under ~0.2 s; a game's choices keep it under 0.5.
            assert!((0.2..0.5).contains(&s.reaction.0), "{}", s.name);
            // Touch 4–8 bits/s (MacKenzie 2015: ~7 for practised adults); finger σa ~1.5 mm ≈ 7–11 pt.
            assert!((4.0..=8.0).contains(&s.touch.throughput) && (7.0..=11.0).contains(&s.touch.floor), "{}", s.name);
            // A held pointer: 1.5–3.5 bits/s; under a finger's.
            assert!((1.5..=3.5).contains(&s.point.throughput) && s.point.throughput < s.touch.throughput, "{}", s.name);
            // Timing to a beat: 20–70 ms SD (Repp 2005).
            assert!((0.02..=0.07).contains(&s.timing), "{}", s.name);
        }
        // An average tap at a 44 pt button from 150 pt away takes over half a second (the finger's floor eats most of the width), not a tenth.
        let t = Skill::casual().touch.time(150.0, 44.0);
        assert!((0.3..0.8).contains(&t), "{t}");
    }

    #[test]
    fn a_move_timed_by_fitts_law_hits_its_target_about_96_percent_of_the_time() {
        for s in Skill::ALL {
            // A swing of 12° onto a 5° target (a light gun's mech part).
            let mut hits = 0;
            for seed in 0..2000 {
                let p = s.aim((0.0, 0.0), (12.0, 0.0), 5.0, 0.0, seed);
                let (yaw, _) = p.angles(p.landed() + 0.3);
                hits += ((yaw - 12.0).abs() <= 2.5) as u32;
            }
            let rate = hits as f32 / 2000.0;
            assert!((0.92..=0.995).contains(&rate), "{}: {rate}", s.name);
        }
    }

    #[test]
    fn slower_is_more_accurate_down_to_the_floor_and_better_players_are_faster() {
        let p = Skill::casual().point;
        let mut last = f32::MAX;
        for k in 1..20 {
            let sd = p.scatter(20.0, 0.2 + k as f32 * 0.1);
            assert!(sd < last && sd >= p.floor);
            last = sd;
        }
        // Time to swing onto a target and shoot: an expert well ahead of a first-timer.
        let (slow, fast) = (Skill::first_timer().point.time(15.0, 4.0), Skill::expert().point.time(15.0, 4.0));
        assert!(fast < slow * 0.65, "{fast} vs {slow}");
        // A target narrower than a finger's floor is slow, not impossible.
        let f = Skill::good().touch;
        assert!(f.time(100.0, 20.0).is_finite() && f.time(100.0, 20.0) > f.time(100.0, 44.0));
    }

    #[test]
    fn taps_land_scattered_like_a_finger_and_reactions_have_the_right_mean() {
        let s = Skill::good();
        let mut rng = Rng::new(9);
        let xs: Vec<f32> = (0..4000).map(|_| s.tap((180.0, 500.0), (180.0, 300.0), 44.0, 0.0, &mut rng).1.0 - 180.0).collect();
        let (m, sd) = mean_sd(&xs);
        assert!(m.abs() < 0.6 && (sd - s.touch.scatter(200.0, s.touch.time(200.0, 44.0))).abs() < 0.8, "{m} {sd}");
        let rts: Vec<f32> = (0..4000).map(|_| s.reaction_time(&mut rng)).collect();
        let (m, sd) = mean_sd(&rts);
        assert!((m - s.reaction.0).abs() < 0.01 && (sd - s.reaction.1).abs() < 0.01, "{m} {sd}");
        let late: Vec<f32> = (0..4000).map(|_| (s.timed(1.0, &mut rng) - 1.0) as f32).collect();
        assert!((mean_sd(&late).1 - s.timing).abs() < 0.004);
    }

    #[test]
    fn a_flick_goes_about_where_it_was_meant_to() {
        let mut rng = Rng::new(3);
        for s in Skill::ALL {
            let f = s.flick((180.0, 500.0), -std::f32::consts::FRAC_PI_2, 150.0, 1.0, &mut rng);
            let (down, up) = (f[0], *f.last().unwrap());
            let off = (up.x - down.x).atan2(down.y - up.y).to_degrees();
            assert!(off.abs() < 4.0 * s.swipe.angle + 5.0, "{}: {off}", s.name);
            let dur = (up.time - down.time) as f32;
            assert!(dur >= s.swipe.duration.0 - 1e-3 && dur <= s.swipe.duration.1 + 1e-3);
        }
    }
}
