//! The rules, with no host calls: `cargo test -p NAME` runs them natively.
//! The camera and the tap test live here too: `Camera::project` is the same
//! projection the host uses (SPEC §5.4), in plain Rust.

use maimbrain::Rng;
use maimbrain::gfx3d::{Camera, Vec3, look_at, vec3};
use maimbrain::motion::Follow;

/// The logical screen (manifest.toml's `logical_size`).
pub const W: f32 = 360.0;
pub const H: f32 = 640.0;
/// Seconds on the clock at the start; each catch adds some back.
pub const START_CLOCK: f32 = 6.0;
pub const CATCH_BONUS: f32 = 1.5;
/// How close to the drone (logical units) a tap must land.
pub const TAP_RADIUS: f32 = 46.0;
/// The difficulty curve, tuned against the bot (bot.rs): the drone flies
/// `START_SPEED + SPEED_UP × √catches` radians per second along its loop, so
/// it speeds up fast at first and ever more slowly after (1.4 after 4
/// catches, 2.15 after 25, 3.4 after 100). A fixed step per catch ends every
/// player's round at the same place: a good player then lasts barely longer
/// than a first-timer.
const START_SPEED: f32 = 0.9;
const SPEED_UP: f32 = 0.25;
/// The camera looks between the platform's centre and the drone, this far
/// toward the drone.
const LOOK_TOWARD_DRONE: f32 = 0.3;

/// A drone loops over the platform; tap it before the clock runs out.
pub struct Sim {
    /// Seconds since the round started.
    pub t: f32,
    /// How far along its loop the drone is (radians).
    pub phase: f32,
    pub speed: f32,
    pub clock: f32,
    pub score: u32,
    pub over: bool,
    /// Where the camera looks: a critically damped spring following a point
    /// near the drone, so the view drifts after it and glides (never snaps)
    /// when it jumps away.
    look: Follow<Vec3>,
    rng: Rng,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut s = Sim { t: 0.0, phase: 0.0, speed: START_SPEED, clock: START_CLOCK, score: 0, over: false, look: Follow::new(Vec3::ZERO, 0.25), rng: Rng::new(seed) };
        s.look.snap(s.look_target());
        s
    }

    fn look_target(&self) -> Vec3 {
        vec3(0.0, 1.0, 0.0).lerp(self.drone(), LOOK_TOWARD_DRONE)
    }

    /// Where the drone is: a figure-eight over the platform.
    pub fn drone(&self) -> Vec3 {
        let a = self.phase;
        vec3(1.7 * a.sin(), 1.5 + 0.5 * (a * 2.0).sin(), 1.2 * (a * 2.0).sin() - 0.4)
    }

    /// The drone's velocity (m/s), e.g. for `Emitter::emit_moving`.
    pub fn drone_velocity(&self) -> Vec3 {
        let a = self.phase;
        vec3(1.7 * a.cos(), 1.0 * (a * 2.0).cos(), 2.4 * (a * 2.0).cos()) * self.speed
    }

    /// The camera: a slow sway, looking where `look` has got to.
    pub fn camera(&self) -> Camera {
        let pos = vec3(0.6 * (self.t * 0.2).sin(), 4.0, 8.5);
        Camera { pos, rot: look_at(pos, self.look.value, Vec3::Y), fov_y: 0.9, near: 0.1, far: 100.0 }
    }

    pub fn step(&mut self, dt: f32) {
        if self.over {
            return;
        }
        self.t += dt;
        self.phase += self.speed * dt;
        self.look.update(self.look_target(), dt);
        self.clock -= dt;
        if self.clock <= 0.0 {
            self.clock = 0.0;
            self.over = true;
        }
    }

    /// A tap at logical (x, y). On a catch the drone jumps to a new spot on
    /// its loop and speeds up; returns where it was caught.
    pub fn tap(&mut self, x: f32, y: f32) -> Option<Vec3> {
        let at = self.drone();
        let p = self.camera().project(at, W, H);
        if self.over || !p.on_screen || (p.x - x).hypot(p.y - y) > TAP_RADIUS {
            return None;
        }
        self.score += 1;
        self.speed = START_SPEED + SPEED_UP * (self.score as f32).sqrt();
        self.clock = (self.clock + CATCH_BONUS).min(START_CLOCK);
        self.phase += self.rng.range(1.5, 3.5);
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_pos(s: &Sim) -> (f32, f32) {
        let p = s.camera().project(s.drone(), W, H);
        assert!(p.on_screen, "the drone is always in view");
        (p.x, p.y)
    }

    #[test]
    fn first_seconds_cannot_fail() {
        let mut s = Sim::new(1);
        for _ in 0..(60 * 3) {
            s.step(1.0 / 60.0);
        }
        assert!(!s.over);
    }

    #[test]
    fn tapping_the_drone_scores_and_missing_does_not() {
        let mut s = Sim::new(1);
        s.step(0.5);
        let (x, y) = screen_pos(&s);
        assert!(s.tap(x + 300.0, y).is_none());
        let before = s.drone();
        assert_eq!(s.tap(x + 10.0, y - 10.0), Some(before));
        assert_eq!(s.score, 1);
        assert!(s.drone().distance(before) > 0.5, "it jumps away");
    }

    #[test]
    fn the_drone_stays_in_view_and_the_clock_ends_the_round() {
        let mut s = Sim::new(7);
        while !s.over {
            screen_pos(&s);
            s.step(1.0 / 60.0);
        }
        assert!((s.t - START_CLOCK).abs() < 0.05);
    }

    /// Anything the camera follows must move in continuous curves. The
    /// camera's peak acceleration (the second difference of where it looks)
    /// stays about the same at 60 and 120 Hz; a snap or a kink in what it
    /// follows shows up as a peak that doubles with the frame rate.
    #[test]
    fn the_camera_moves_smoothly_at_60_and_120_hz() {
        let peak = |hz: f32| {
            let (mut s, dt) = (Sim::new(3), 1.0 / hz);
            let (mut seen, mut peak, mut catches) = (Vec::new(), 0.0f32, 0);
            while s.t < 4.0 {
                // A catch every second: the drone jumps away.
                if s.t >= 1.0 + catches as f32 {
                    let (x, y) = screen_pos(&s);
                    assert!(s.tap(x, y).is_some());
                    catches += 1;
                }
                s.step(dt);
                let c = s.camera();
                seen.push(c.pos + c.rot.forward() * 8.0);
                if let [.., a, b, c] = seen[..] {
                    peak = peak.max((c - b * 2.0 + a).length() / (dt * dt));
                }
            }
            peak
        };
        let (a60, a120) = (peak(60.0), peak(120.0));
        assert!(a120 < a60 * 1.3 && a120 < 40.0, "peak camera acceleration (m/s²): {a60:.1} at 60 Hz, {a120:.1} at 120 Hz");
    }

    #[test]
    fn same_seed_same_game() {
        let play = |seed| {
            let mut s = Sim::new(seed);
            for i in 0..600 {
                s.step(1.0 / 60.0);
                if i % 50 == 0 {
                    let (x, y) = screen_pos(&s);
                    s.tap(x, y);
                }
            }
            (s.score, s.phase.to_bits())
        };
        assert_eq!(play(3), play(3));
    }
}
