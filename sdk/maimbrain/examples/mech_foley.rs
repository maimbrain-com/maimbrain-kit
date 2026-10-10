//! Two mechs' foley with `maimbrain::foley`, written down as a `mix_check.py`
//! event log instead of played (swap `Log` for `Live` in a game, as
//! mb3d-demo's retro scene does).
//!
//!     python3 <skill>/scripts/sfx.py --pack mech /tmp/mech     # or: mb sfx <game> --pack mech
//!     cargo run -p maimbrain --example mech_foley > /tmp/mech/foley.log
//!     python3 <skill>/scripts/mix_check.py /tmp/mech /tmp/mech/foley.log --out foley.wav --fx-loop mech_servo,mech_thruster
//!
//! 14 s at 60 fps: the blue mech walks in from the left past the camera
//! (stomps by stride phase, panning left to right and louder as it nears),
//! turns to face the red one (the servo whines with the turn), jumps on its
//! thrusters and lands (the landing as loud as the fall was fast), trades
//! punches (an impact and a hiss on each) and loses an arm (a tear); the red
//! mech stomps in place far off the whole time, quieter, never crowding out
//! the near one (six voices at most).

use maimbrain::audio::Sound;
use maimbrain::foley::{Contact, Drive, Foley, Gait, Listener, Log};
use maimbrain::gfx3d::{Vec3, vec3};

const STOMP: Sound = Sound(1);
const SERVO: Sound = Sound(2);
const THRUSTER: Sound = Sound(3);
const HISS: Sound = Sound(4);
const IMPACT: Sound = Sound(5);
const TEAR: Sound = Sound(6);

fn main() {
    let mut log = Log::default();
    for (s, n) in [(STOMP, "stomp"), (SERVO, "servo"), (THRUSTER, "thruster"), (HISS, "hiss"), (IMPACT, "impact"), (TEAR, "tear")] {
        log.name(s, &format!("mech_{n}"));
    }
    let mut foley = Foley::with_mixer(7, log);
    let ear = Listener { pos: vec3(0.0, 2.0, 8.0), right: Vec3::X };
    let (mut blue_gait, mut red_gait) = (Gait::biped(), Gait::biped());
    let mut servo = Drive::servo(SERVO);
    let mut jets = Drive::thruster(THRUSTER);
    let mut feet = Contact::new();
    let dt = 1.0 / 60.0;
    let (mut blue, mut stride, mut yaw, mut h, mut vy) = (vec3(-14.0, 0.0, 0.0), 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let red = vec3(10.0, 0.0, -30.0);
    let (mut red_stride, mut punched, mut torn) = (0.0f32, [false; 3], false);
    for i in 0..(14.0 / dt) as usize {
        let t = i as f32 * dt;
        foley.update(dt, ear);
        // Walk in for 5 s at 3 m/s, 1.4 strides a second.
        let walking = t < 5.0;
        if walking {
            blue.x += 3.0 * dt;
            stride += 1.4 * dt;
            for _ in blue_gait.update(stride) {
                foley.step(STOMP, blue, 0.7);
            }
        } else {
            blue_gait.reset();
        }
        // Turn a quarter round toward the red mech over 5.5–6.5 s.
        let turn_rate = if (5.5..6.5).contains(&t) { std::f32::consts::FRAC_PI_2 } else { 0.0 };
        yaw += turn_rate * dt;
        servo.update(&mut foley, turn_rate + if walking { 0.4 } else { 0.0 }, blue + vec3(0.0, 2.5, 0.0), dt);
        // Thrusters from 7.5 s for 0.9 s: up, a hang, then a fall onto the ground.
        let thrust = if (7.5..8.4).contains(&t) { 1.0 } else { 0.0 };
        vy += (thrust * 16.0 - 9.8) * dt;
        h += vy * dt;
        if h <= 0.0 {
            (h, vy) = (0.0, 0.0);
        }
        jets.update(&mut foley, thrust, blue + vec3(0.0, h, 0.0), dt);
        if let Some(speed) = feet.update(h, dt) {
            foley.impact(STOMP, blue, speed, 12.0);
            foley.hit(HISS, blue + vec3(0.0, 1.0, 0.0), 0.4, 1.0, 0.6);
        }
        // Punches at 10.5, 11.3 and 12.0 s; the third takes an arm off.
        for (k, at) in [10.5f32, 11.3, 12.0].into_iter().enumerate() {
            if !punched[k] && t >= at {
                punched[k] = true;
                foley.impact(IMPACT, blue + vec3(0.0, 2.8, 0.0), 14.0 + 3.0 * k as f32, 20.0);
                foley.hit(HISS, blue + vec3(0.5, 2.5, 0.0), 0.35, 1.1, 0.6);
            }
        }
        if !torn && t >= 12.05 {
            torn = true;
            foley.hit(TEAR, blue + vec3(1.0, 2.6, 0.0), 0.8, 1.0, 1.0);
        }
        // The red mech stomps in place far off, all the time.
        red_stride += 1.1 * dt;
        for _ in red_gait.update(red_stride) {
            foley.step(STOMP, red, 0.9);
        }
        let _ = yaw;
    }
    servo.stop(&mut foley);
    jets.stop(&mut foley);
    print!("{}", foley.mixer.text());
}
