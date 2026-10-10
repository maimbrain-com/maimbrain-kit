//! Mistwood: a fox spirit runs a misty forest trail at night. Tap to jump
//! the fallen logs; coins over them are the reward for jumping on time. The
//! showcase for mb3d 2 (see world.rs): an animated, toon-shaded fox with a
//! lantern on its tail, instanced forest, fog, 3D text and depth of field.
//!
//! Shape as in the 3D template: rules and camera in `sim` (tested), the
//! scene in `world`, and this thin host layer (rounds, input, the HUD).

mod sim;
mod world;

use maimbrain::gfx2d::Font;
use maimbrain::gfx3d;
use maimbrain::sys::{self, Round};
use maimbrain::{Game, Rng, export_game, gfx2d, input, store};

use sim::{H, Sim, W};
use world::World;

const BOARD: u32 = 0;
/// Taps right after a fall don't restart (a frantic tap during the tumble).
const RETRY_GUARD: f32 = 0.5;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Title,
    Play,
    Over(f32),
}

struct Mistwood {
    mode: Mode,
    sim: Sim,
    world: World,
    rng: Rng,
    best: u32,
    time: f32,
}

impl Mistwood {
    fn start(&mut self) {
        self.sim = Sim::new(self.rng.next_u32() as u64);
        self.mode = Mode::Play;
        sys::round(Round::Playing);
        // The title card's orbiting camera cuts to the trail camera: meant, so
        // `mb shot --smoothness` lists it as marked rather than as a snap.
        sys::mark("start: cut to the trail camera");
    }
}

impl Game for Mistwood {
    fn init() -> Self {
        sys::round(Round::Idle);
        let mut rng = Rng::from_host();
        let sim = Sim::new(rng.next_u32() as u64);
        let world = World::build(rng.next_u32() as u64);
        Mistwood { mode: Mode::Title, sim, world, rng, best: store::get_u64("best").unwrap_or(0) as u32, time: 0.0 }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        for e in input::poll() {
            if !e.is_press() {
                continue;
            }
            match self.mode {
                // The tap that enters play is also the first jump (SPEC §5.2).
                Mode::Title => {
                    self.start();
                    self.sim.tap();
                }
                Mode::Play => self.sim.tap(),
                Mode::Over(t) if t > RETRY_GUARD => self.start(),
                Mode::Over(_) => {}
            }
        }
        match &mut self.mode {
            Mode::Play => {
                self.sim.step(dt);
                if self.sim.over {
                    store::submit_score(BOARD, self.sim.dist as i64);
                    if self.sim.dist as u32 > self.best {
                        self.best = self.sim.dist as u32;
                        store::set_u64("best", self.best as u64);
                    }
                    self.mode = Mode::Over(0.0);
                    sys::round(Round::Over);
                }
            }
            Mode::Over(t) => {
                *t += dt;
                self.sim.events.clear();
            }
            // The title card is an attract mode: the fox runs the trail on its own.
            Mode::Title => {
                self.sim.autopilot();
                self.sim.step(dt);
                if self.sim.over {
                    self.sim = Sim::new(self.rng.next_u32() as u64);
                }
            }
        }
        let (title, over) = (self.mode == Mode::Title, matches!(self.mode, Mode::Over(_)));
        self.world.update(&self.sim, title, over, self.time, dt);
    }

    fn render(&self) {
        gfx3d::camera(&self.sim.camera(self.mode == Mode::Title, self.time));
        gfx3d::render();

        let shadow = |size: f32, y: f32, color: u32, s: &str| {
            gfx2d::text_centered(Font::SansBold, size, W / 2.0 + 1.5, y + 1.5, 0x00000090, s);
            gfx2d::text_centered(Font::SansBold, size, W / 2.0, y, color, s);
        };
        match self.mode {
            Mode::Title => {
                // The card is an ad: the hook and a brag. The world sign says MISTWOOD.
                shadow(22.0, 70.0, 0xffe7b8ff, "RUN THE MISTWOOD");
                if self.best > 0 {
                    shadow(15.0, 102.0, 0xffd27aff, &format!("BEST {} m", self.best));
                }
            }
            Mode::Play | Mode::Over(_) => {
                shadow(34.0, 40.0, 0xffffffff, &format!("{} m", self.sim.dist as u32));
                shadow(16.0, 84.0, 0xffd27aff, &format!("{} COINS", self.sim.coins));
                if self.mode == Mode::Play && self.sim.t < 3.0 && self.sim.coins == 0 {
                    shadow(16.0, H * 0.62, 0xffffffd0, "TAP TO JUMP THE LOGS");
                }
            }
        }
        if let Mode::Over(t) = self.mode {
            let a = ((t * 3.0).min(1.0) * 112.0) as u32;
            gfx2d::rect(0.0, 0.0, W, H, a);
            shadow(30.0, 250.0, 0xffffffff, "TRIPPED!");
            shadow(16.0, 296.0, 0xffd27aff, &format!("BEST {} m", self.best));
            if t > RETRY_GUARD {
                shadow(14.0, 330.0, 0xffffffb0, "TAP TO RUN AGAIN");
            }
        }
    }
}

export_game!(Mistwood);
