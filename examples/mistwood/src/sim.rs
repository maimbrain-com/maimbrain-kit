//! The run, with no host calls (`cargo test -p mistwood`). The fox stays at
//! the origin facing −z and the forest moves past it: something `d` meters
//! along the trail is at world z = −(d − dist).

use maimbrain::Rng;
use maimbrain::gfx3d::{Camera, Vec3, look_at, vec3};

pub const W: f32 = 360.0;
pub const H: f32 = 640.0;
pub const START_SPEED: f32 = 6.5;
pub const MAX_SPEED: f32 = 13.0;
/// Meters per second gained each second.
pub const ACCEL: f32 = 0.12;
/// The Jump clip's length (assets/fox.glb, tools/make_fox.py).
pub const JUMP_TIME: f32 = 0.9;
/// A log's height and the stretch of trail where it can trip the fox.
pub const LOG_HEIGHT: f32 = 0.27;
pub const LOG_REACH: (f32, f32) = (-0.42, 0.32);
/// Logs are only placed this far ahead (fog hides the rest).
pub const AHEAD: f32 = 90.0;

/// How high the jump lifts the fox at `t` seconds into the Jump clip: the
/// same curve tools/make_fox.py animates the hips with, so what you see is
/// what's tested.
pub fn jump_lift(t: f32) -> f32 {
    let smooth = |e0: f32, e1: f32, x: f32| {
        let k = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    };
    if t < 0.15 {
        -0.1 * smooth(0.0, 0.15, t)
    } else if t < 0.75 {
        let u = (t - 0.15) / 0.6;
        -0.1 + 0.1 * smooth(0.0, 0.1, u) + 0.45 * 4.0 * u * (1.0 - u)
    } else if t < JUMP_TIME {
        -0.08 * (std::f32::consts::PI * (t - 0.75) / 0.15).sin()
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coin {
    /// Meters along the trail, sideways offset, height.
    pub d: f32,
    pub x: f32,
    pub y: f32,
    pub taken: bool,
}

/// Something that happened this step, for sound and effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Jumped,
    Coin(Coin),
    Tripped,
    Marker(u32),
}

pub struct Sim {
    pub t: f32,
    /// Meters run.
    pub dist: f32,
    pub speed: f32,
    pub coins: u32,
    pub over: bool,
    /// Seconds into the current jump.
    pub jump: Option<f32>,
    /// Logs, by meters along the trail.
    pub logs: Vec<f32>,
    pub coin_list: Vec<Coin>,
    next: f32,
    rng: Rng,
    pub events: Vec<Event>,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut s = Sim { t: 0.0, dist: 0.0, speed: START_SPEED, coins: 0, over: false, jump: None, logs: Vec::new(), coin_list: Vec::new(), next: 22.0, rng: Rng::new(seed), events: Vec::new() };
        s.spawn();
        s
    }

    /// The fox's lift off the ground now.
    pub fn lift(&self) -> f32 {
        self.jump.map_or(0.0, jump_lift)
    }


    /// A tap: jump, if on the ground (a tap late in a landing queues nothing).
    pub fn tap(&mut self) {
        if !self.over && self.jump.is_none() {
            self.jump = Some(0.0);
            self.events.push(Event::Jumped);
        }
    }

    /// Lays out logs and coins up to `AHEAD` meters in front.
    fn spawn(&mut self) {
        while self.next < self.dist + AHEAD {
            let d = self.next;
            let pattern = self.rng.next_u32() % 3;
            self.logs.push(d);
            match pattern {
                // An arc of coins over the log: the reward for jumping it right.
                0 => {
                    for k in 0..5 {
                        let off = (k as f32 - 2.0) * 0.7;
                        let lift = jump_lift(0.45 + off / self.speed.max(1.0));
                        self.coin_list.push(Coin { d: d + off, x: 0.0, y: 0.55 + lift.max(0.0), taken: false });
                    }
                }
                // A line of coins before the log.
                1 => {
                    for k in 0..4 {
                        self.coin_list.push(Coin { d: d - 6.0 + k as f32 * 1.1, x: 0.0, y: 0.5, taken: false });
                    }
                }
                _ => {}
            }
            // Gaps shrink as the fox speeds up, but always leave room to land and react.
            let min_gap = self.speed * JUMP_TIME + 3.5;
            self.next += min_gap + self.rng.range(1.0, 9.0) * (START_SPEED / self.speed).sqrt();
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.events.clear();
        if self.over || !(dt > 0.0) {
            return;
        }
        self.t += dt;
        let before = self.dist;
        self.speed = (self.speed + ACCEL * dt).min(MAX_SPEED);
        self.dist += self.speed * dt;
        if (self.dist / 100.0) as u32 > (before / 100.0) as u32 {
            self.events.push(Event::Marker((self.dist / 100.0) as u32 * 100));
        }
        if let Some(j) = &mut self.jump {
            *j += dt;
            if *j >= JUMP_TIME {
                self.jump = None;
            }
        }
        let lift = self.lift();
        for &d in &self.logs {
            let rel = d - self.dist;
            if rel > LOG_REACH.0 && rel < LOG_REACH.1 && lift < LOG_HEIGHT {
                self.over = true;
                self.events.push(Event::Tripped);
                break;
            }
        }
        let body = 0.5 + lift;
        for c in &mut self.coin_list {
            if !c.taken && (c.d - self.dist).abs() < 0.55 && (c.y - body).abs() < 0.45 {
                c.taken = true;
                self.coins += 1;
                self.events.push(Event::Coin(*c));
            }
        }
        let behind = self.dist - 12.0;
        self.logs.retain(|&d| d > behind);
        self.coin_list.retain(|c| c.d > behind);
        self.spawn();
    }

    /// The attract mode's pilot (and the tests' bot): jumps when the next log
    /// is about the jump's rise time away, a little early or late by the seed.
    pub fn autopilot(&mut self) {
        let next = self.logs.iter().copied().filter(|&d| d > self.dist).fold(f32::MAX, f32::min);
        let lead = 0.45 + 0.04 * ((next * 12.9898).sin());
        if (next - self.dist) / self.speed < lead {
            self.tap();
        }
    }

    /// The chase camera: behind and above the fox, swaying a little.
    pub fn camera(&self, title: bool, time: f32) -> Camera {
        let sway = 0.25 * (time * 0.6).sin();
        let (pos, target) = if title {
            // The attract mode: off to the side, so the card shows the fox in profile.
            (vec3(1.3 + sway * 0.5, 1.3, 3.0), vec3(-0.2, 0.55, -2.0))
        } else {
            (vec3(0.9 + sway, 1.55 + self.lift() * 0.3, 4.4), vec3(0.0, 0.6, -4.0))
        };
        Camera { pos, rot: look_at(pos, target, Vec3::Y), fov_y: 0.95, near: 0.1, far: 140.0 }
    }

    /// Meters from the camera to the fox, for the depth of field's focus.
    pub fn focus(&self, title: bool, time: f32) -> f32 {
        self.camera(title, time).pos.distance(vec3(0.0, 0.5, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Sim, seconds: f32, mut on_frame: impl FnMut(&mut Sim)) {
        for _ in 0..(seconds * 60.0) as u32 {
            on_frame(s);
            s.step(1.0 / 60.0);
            if s.over {
                break;
            }
        }
    }

    #[test]
    fn the_jump_curve_clears_logs_mid_air_and_lands() {
        assert_eq!(jump_lift(0.0), 0.0);
        assert!(jump_lift(0.45) > 0.43);
        assert!(jump_lift(0.1) < 0.0, "a crouch first");
        assert_eq!(jump_lift(1.0), 0.0);
        let clear = (0..90).filter(|&k| jump_lift(k as f32 / 100.0) > LOG_HEIGHT).count();
        assert!((25..45).contains(&clear), "{clear} hundredths of a second above a log");
    }

    #[test]
    fn standing_still_trips_on_the_first_log() {
        let mut s = Sim::new(1);
        run(&mut s, 30.0, |_| {});
        assert!(s.over);
        assert!(s.dist > 20.0 && s.dist < 25.0, "{}", s.dist);
    }

    #[test]
    fn a_bot_that_jumps_on_time_runs_far_and_collects_coins() {
        let mut s = Sim::new(7);
        run(&mut s, 120.0, Sim::autopilot);
        assert!(!s.over, "tripped at {} m", s.dist);
        assert!(s.dist > 1000.0 && s.coins > 20, "{} m, {} coins", s.dist, s.coins);
        assert_eq!(s.speed, MAX_SPEED);
    }

    #[test]
    fn logs_leave_room_to_land_and_jump_again() {
        let mut s = Sim::new(3);
        s.speed = MAX_SPEED;
        s.dist = 500.0;
        s.next = 520.0;
        s.logs.clear();
        s.step(1.0 / 60.0);
        for w in s.logs.windows(2) {
            assert!(w[1] - w[0] > MAX_SPEED * JUMP_TIME + 3.0, "{w:?}");
        }
    }

    #[test]
    fn same_seed_same_run() {
        let (mut a, mut b) = (Sim::new(11), Sim::new(11));
        for i in 0..2000 {
            if i % 37 == 0 {
                a.tap();
                b.tap();
            }
            a.step(1.0 / 60.0);
            b.step(1.0 / 60.0);
        }
        assert_eq!((a.dist.to_bits(), a.coins, a.over), (b.dist.to_bits(), b.coins, b.over));
    }
}
