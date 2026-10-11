//! The rules and the physics: a bird flies back and forth over a plate on a
//! pole, carrying a snack; a tap lets go. Whatever comes to rest on the
//! tower counts, whatever falls off costs a heart. Later the wind gusts and
//! the plate tilts. Pure game state: input arrives as calls (`drop_piece`),
//! sounds, haptics and juice leave as `cues`; nothing here draws or plays.
//!
//! Everything is a real rigid body (`maimbrain::physics2d`, docs/PHYSICS.md):
//! how a snack lands, wobbles, wedges or slides is never scripted.

use maimbrain::Rng;
use maimbrain::motion::Follow;
use maimbrain::physics2d::{Body, BodyId, Event, Joint, JointId, Shape, Vec2, World, vec2};

// ---- Tuning knobs -----------------------------------------------------
// The numbers that shape the game, roughly in order of how much they matter.
// `cargo test -p kit_stacker --profile bot -- --ignored --nocapture difficulty`
// measures what a change does to round lengths.

/// Width of the plate the tower stands on, px. Narrower is harder at once.
pub const PLATE_W: f32 = 156.0;
/// The bird's sweep: half-width in px (start, added per snack stacked, max).
/// Wider sweeps mean more drift to judge.
pub const SWEEP_AMP: (f32, f32, f32) = (14.0, 4.0, 88.0);
/// The bird's sweep speed in rad/s of its sine (start, per snack, max).
/// Faster is harder to time; past ~2.4 it feels frantic.
pub const SWEEP_SPEED: (f32, f32, f32) = (1.3, 0.035, 1.8);
/// How much of the bird's sideways speed a dropped snack keeps (0 = drops
/// straight down, 1 = all of it). The landing ghost always shows the result.
pub const CARRY: f32 = 0.2;
/// Seconds from a drop until the bird has the next snack. Lower is a faster
/// tempo (and taller towers in the same time).
pub const RESPAWN: f32 = 0.6;
/// Hearts: tumbles before it's over.
pub const LIVES: u32 = 3;
/// Snacks that fall within this many seconds of a lost heart belong to the
/// same tumble: a toppling tower costs one heart, not one per snack.
pub const TUMBLE_GRACE: f32 = 2.5;
/// This many snacks in a row without a fall wins a lost heart back (0 = never).
/// Rewards steady play: skilled players go on much longer.
pub const HEART_BACK: u32 = 10;
/// When each snack joins the mix: (snacks stacked, kind). A newly unlocked
/// kind always comes next, so the player meets it at once.
pub const UNLOCKS: [(u32, Kind); 6] = [(0, Kind::Box), (3, Kind::Cube), (6, Kind::Plank), (10, Kind::Wedge), (15, Kind::Bun), (21, Kind::Ball)];
/// Twists (wind gusts, then a tilting plate, alternating): seconds into the
/// round of the first one, and the random gap between later ones.
pub const TWIST_FIRST: f32 = 16.0;
pub const TWIST_GAP: (f32, f32) = (11.0, 16.0);
/// Each twist is announced this long before it starts.
pub const WARN_TIME: f32 = 1.5;
/// Gust: peak sideways push (px/s², gravity is 981) and how long it blows.
/// Every twist is a bit stronger than the last (`TWIST_GROWTH`).
pub const GUST_ACCEL: f32 = 70.0;
pub const GUST_TIME: f32 = 2.4;
/// Tilt: peak plate angle (radians) and how long one tilt-and-back takes.
pub const TILT_MAX: f32 = 0.07;
pub const TILT_TIME: f32 = 4.0;
/// How much stronger each twist is than the one before (0.2 = +20 %), up to ×`TWIST_CAP`.
pub const TWIST_GROWTH: f32 = 0.2;
pub const TWIST_CAP: f32 = 2.0;
/// Gravity, px/s² (real is 981). Lower is floatier: softer landings and
/// slower, more readable topples.
pub const GRAVITY: f32 = 700.0;
/// Grip of every snack (Rapier friction). Lower and towers slide apart.
pub const FRICTION: f32 = 0.9;
/// Snacks this far (px) below the top of the tower, once still, settle into
/// a solid base (held to the plate): only the top of the tower can topple,
/// so skilled players can go on and the physics stays cheap. Lower it and
/// the game gets more forgiving; `f32::MAX` turns it off.
pub const SOLID_BELOW: f32 = 130.0;
/// A landing snack grips for a moment: on first touch its motion is damped
/// this hard (per second) for `GRIP_TIME`, so it settles instead of
/// skidding off. 0 is pure physics (bouncier, harder).
pub const GRIP: f32 = 10.0;
pub const GRIP_TIME: f32 = 0.25;

// ---- Rules that rarely need changing ------------------------------------

pub const W: f32 = 360.0;
/// World y of the plate's top surface (the camera starts at 0).
pub const PLATE_TOP: f32 = 404.0;
pub const PLATE_H: f32 = 14.0;
/// The picnic blanket under the pole: where fallen snacks end up.
pub const GROUND_Y: f32 = 560.0;
/// A snack whose centre falls this far below the plate top is lost.
pub const LOST_BELOW: f32 = 70.0;
/// Where the bird flies, on screen, and the least room it keeps above the tower.
pub const BIRD_SCREEN_Y: f32 = 222.0;
pub const BIRD_CLEARANCE: f32 = 70.0;
/// From the bird's centre to the top of the snack it carries.
pub const HOLD_GAP: f32 = 20.0;
/// Where the camera keeps the top of the tower, on screen.
pub const TOWER_TOP_ON_SCREEN: f32 = 372.0;
/// A dropped snack counts once it has rested on the tower this long, this still.
pub const SETTLE_TIME: f32 = 0.3;
pub const SETTLE_SPEED: f32 = 22.0;
/// Landing within this many px of the centre of what it sits on is "neat";
/// further out than this fraction of its half-width is a close one.
pub const NEAT_PX: f32 = 7.0;
pub const CLOSE_FRACTION: f32 = 0.6;
/// Impacts slower than this (px/s) are silent; above `HEAVY_SPEED` they thump.
pub const THUD_SPEED: f32 = 70.0;
pub const HEAVY_SPEED: f32 = 420.0;
/// After the last heart: the plate tips and the collapse plays out this long.
pub const ENDING: f32 = 2.0;
/// Fallen snacks lie dizzy on the blanket this long, then go (not after the last heart:
/// the pile is the game-over card's aftermath).
pub const LOST_LINGER: f32 = 3.0;

// ---- Snacks ----------------------------------------------------------------

/// Piece shapes. The names are physical; what they look like (toast, jelly,
/// a baguette, cheese…) lives in `look.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A wide box: the easy one.
    Box,
    /// A square, a little bouncy.
    Cube,
    /// Long and thin: a great platform, easy to tip.
    Plank,
    /// Wide at the bottom, narrow on top: steady, but a small target.
    Wedge,
    /// A box with very round corners: slides and rocks.
    Bun,
    /// Round: rolls (damped so it settles).
    Ball,
}

impl Kind {
    /// Full size in px (a ball's is its diameter).
    pub fn size(self) -> Vec2 {
        match self {
            Kind::Box => vec2(66.0, 42.0),
            Kind::Cube => vec2(44.0, 44.0),
            Kind::Plank => vec2(116.0, 22.0),
            Kind::Wedge => vec2(84.0, 42.0),
            Kind::Bun => vec2(64.0, 36.0),
            Kind::Ball => vec2(46.0, 46.0),
        }
    }

    /// Corner radius of the drawn and simulated shape (matched to the art:
    /// the baguette's ends are round).
    pub fn radius(self) -> f32 {
        match self {
            Kind::Plank => 9.0,
            Kind::Bun => 16.0,
            Kind::Ball => 23.0,
            _ => 0.0,
        }
    }

    /// The wedge's corners in its own frame: bottom-left, bottom-right,
    /// top-right, top-left (the top is 46 % of the bottom, as in the art).
    pub fn wedge_points() -> [Vec2; 4] {
        let s = Kind::Wedge.size();
        let (hw, hh, top) = (s.x / 2.0, s.y / 2.0, s.x * 0.23);
        [vec2(-hw, hh), vec2(hw, hh), vec2(top, -hh), vec2(-top, -hh)]
    }

    fn shape(self) -> Shape {
        let s = self.size();
        let shape = match self {
            Kind::Wedge => Shape::convex(&Kind::wedge_points()),
            Kind::Bun | Kind::Plank => Shape::round_rect(s.x, s.y, self.radius()),
            Kind::Ball => Shape::circle(s.x / 2.0),
            _ => Shape::rect(s.x, s.y),
        };
        let bounce = if self == Kind::Cube { 0.15 } else { 0.04 };
        shape.friction(FRICTION).restitution(bounce)
    }

    /// Linear and angular damping (balls roll, then stop).
    fn damping(self) -> (f32, f32) {
        if self == Kind::Ball { (0.3, 4.0) } else { (0.02, 0.3) }
    }

    /// Wedges and balls are awkward: never two in a row.
    pub fn awkward(self) -> bool {
        matches!(self, Kind::Wedge | Kind::Ball)
    }

    /// Half its height when turned by `angle` (for measuring the tower).
    fn half_height(self, angle: f32) -> f32 {
        if self == Kind::Ball {
            return self.size().x / 2.0;
        }
        let s = self.size();
        0.5 * (s.x * angle.sin().abs() + s.y * angle.cos().abs())
    }
}

/// What a snack is doing; its face shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// Let go; `rest` is how long it has been still on the tower.
    Falling { rest: f32 },
    /// Counted: part of the tower.
    Landed,
    /// Fell off, `t` seconds ago (removed once off screen).
    Lost { t: f32 },
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub id: BodyId,
    pub kind: Kind,
    /// Which of the kind's colors (0–2).
    pub variant: u32,
    pub state: State,
    /// Seconds since it was dropped.
    pub age: f32,
    /// How rattled it is (0–1): hits, speed, the tower wobbling. Drives the face.
    pub fear: f32,
    /// Settled into the solid base (`SOLID_BELOW`): held to the plate by this joint.
    pub solid: Option<JointId>,
    /// Seconds of landing grip left (`GRIP`); `None` until it first touches something.
    grip: Option<f32>,
}

/// The snack the bird is carrying (not a body until it's let go).
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub kind: Kind,
    pub variant: u32,
    /// Seconds since the bird got it (it swoops in).
    pub age: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwistKind {
    Gust,
    Tilt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Twist {
    /// Nothing happening; the next twist is `left` seconds away.
    Calm { left: f32 },
    /// Announced: starts when `t` reaches `WARN_TIME`. `dir` is ±1 (right/left).
    Warn { kind: TwistKind, dir: f32, t: f32 },
    On { kind: TwistKind, dir: f32, t: f32 },
}

/// Things that happened this step, for the host's sounds, haptics and juice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// The player let go.
    Drop,
    /// Something hit something: `vol` 0–1 from the closing speed.
    Thud { x: f32, y: f32, vol: f32, heavy: bool },
    /// A snack came to rest on the tower: +1. `streak` landings in a row
    /// without losing one; `neat` if centred on what it sits on, `close` if
    /// it only just stayed on.
    Land { x: f32, y: f32, streak: u32, neat: bool, close: bool },
    /// A snack fell off. `counted`: it cost a heart (false for the rest of
    /// a tumble and during the final collapse).
    /// `landed`: it was part of the tower (a tumble) rather than a miss.
    Lost { x: f32, y: f32, kind: Kind, counted: bool, landed: bool },
    /// A streak of `HEART_BACK` won a heart back.
    HeartBack,
    /// A new kind of snack just joined the mix (the bird is carrying it).
    NewKind(Kind),
    /// A twist is coming (and which way).
    Warn(TwistKind, f32),
    Start(TwistKind),
    /// The last heart is gone: the plate tips.
    Collapse,
    Over,
}

/// Where the carried snack would go if let go now.
#[derive(Clone, Debug, Default)]
pub struct Prediction {
    /// Its centre along the way, every 1/15 s.
    pub arc: Vec<Vec2>,
    /// Its centre when it first touches something.
    pub land: Vec2,
    /// Whether it touches anything at all (false: it would miss everything).
    pub hits: bool,
}

// ---- The sim ------------------------------------------------------------------

pub struct Sim {
    pub world: World,
    pub plate: BodyId,
    pub pieces: Vec<Piece>,
    pub held: Option<Held>,
    rng: Rng,
    /// Seconds since the round began.
    pub t: f32,
    phase: f32,
    amp: f32,
    speed: f32,
    /// The bird waits at the centre until the first drop.
    moving: bool,
    pub score: u32,
    pub lives: u32,
    pub streak: u32,
    pub drops: u32,
    respawn: Option<f32>,
    /// Seconds left of the current tumble (further falls are free).
    grace: f32,
    /// Kinds met so far (indices of `UNLOCKS`).
    unlocked: usize,
    last_kind: Option<Kind>,
    pub twist: Twist,
    twists: u32,
    /// The wind's push right now (px/s², + is right).
    pub wind: f32,
    /// The plate's angle right now (radians, + is clockwise).
    pub plate_angle: f32,
    /// Since the last heart was lost (the collapse plays out first).
    pub ending: Option<f32>,
    tip_dir: f32,
    pub over: bool,
    /// World y at the top of the screen (negative as the tower grows).
    pub cam_y: f32,
    /// What moves `cam_y`: a critically damped spring, so the camera glides
    /// up when a block lands rather than starting with a jolt.
    cam: Follow,
    /// The tower's height above the plate, px.
    pub height: f32,
    pub best_height: f32,
    /// How much the tower is moving (0 calm … 1 swaying), eased.
    pub wobble: f32,
    pub cues: Vec<Cue>,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut world = World::with_gravity(vec2(0.0, GRAVITY));
        world.set_solver_iterations(8);
        // The plate is kinematic so it can tilt (twists, the final tip).
        let plate = world.add(Body::kinematic(W / 2.0, PLATE_TOP + PLATE_H / 2.0), Shape::round_rect(PLATE_W, PLATE_H, 6.0).friction(1.0));
        // The ground catches what falls (it's far below the tower; nothing counts there).
        world.add(Body::fixed(W / 2.0, GROUND_Y + 30.0), Shape::rect(1200.0, 60.0).friction(0.9));
        let mut sim = Sim {
            world,
            plate,
            pieces: Vec::new(),
            held: None,
            rng: Rng::new(seed),
            t: 0.0,
            phase: 0.0,
            // The sweep eases in from nothing, so the first seconds can't go wrong.
            amp: 0.0,
            speed: SWEEP_SPEED.0,
            moving: false,
            score: 0,
            lives: LIVES,
            streak: 0,
            drops: 0,
            respawn: None,
            grace: 0.0,
            unlocked: 1,
            last_kind: None,
            twist: Twist::Calm { left: TWIST_FIRST },
            twists: 0,
            wind: 0.0,
            plate_angle: 0.0,
            ending: None,
            tip_dir: 1.0,
            over: false,
            cam_y: 0.0,
            cam: Follow::new(0.0, 0.3),
            height: 0.0,
            best_height: 0.0,
            wobble: 0.0,
            cues: Vec::new(),
        };
        sim.spawn();
        sim
    }

    // ---- where things are ---------------------------------------------------

    /// The bird's centre (world).
    pub fn bird(&self) -> Vec2 {
        let top = PLATE_TOP - self.height;
        // Room for the tallest snack under it, whatever it carries now.
        let y = (self.cam_y + BIRD_SCREEN_Y).min(top - BIRD_CLEARANCE - 46.0 - HOLD_GAP);
        vec2(W / 2.0 + self.amp * self.phase.sin(), y)
    }

    /// The bird's sideways speed, px/s.
    pub fn bird_vx(&self) -> f32 {
        if self.moving { self.amp * self.speed * self.phase.cos() } else { 0.0 }
    }

    /// Centre of the carried snack (world).
    pub fn held_pos(&self) -> Option<Vec2> {
        let h = self.held?;
        Some(self.bird() + vec2(0.0, HOLD_GAP + h.kind.size().y / 2.0))
    }

    /// The top of the tower: the centre x of the highest landed snack (or
    /// the plate's) and the y of its top.
    pub fn tower_top(&self) -> Vec2 {
        let mut best = vec2(W / 2.0, PLATE_TOP);
        for p in self.pieces.iter().filter(|p| p.state == State::Landed) {
            let pose = self.world.pose(p.id);
            let top = pose.pos.y - p.kind.half_height(pose.angle);
            if top < best.y {
                best = vec2(pose.pos.x, top);
            }
        }
        best
    }

    /// Where the carried snack would land if let go now: its free fall
    /// (keeping `CARRY` of the bird's speed, pushed by the wind blowing now),
    /// raycast from its bottom corners and centre against everything.
    pub fn predict(&self) -> Option<Prediction> {
        let held = self.held?;
        let start = self.held_pos()?;
        let size = held.kind.size();
        let g = self.world.gravity().y;
        let (vx, ax) = (self.bird_vx() * CARRY, self.wind);
        let feet = [-0.42 * size.x, 0.0, 0.42 * size.x];
        let h = 1.0 / 30.0;
        let mut arc = Vec::new();
        let at = |t: f32| vec2(start.x + vx * t + 0.5 * ax * t * t, start.y + 0.5 * g * t * t);
        for k in 0..90 {
            let (t0, t1) = (k as f32 * h, (k + 1) as f32 * h);
            let (p0, p1) = (at(t0), at(t1));
            if k % 2 == 0 {
                arc.push(p0);
            }
            let mut first: Option<f32> = None;
            for dx in feet {
                let from = vec2(p0.x + dx, p0.y + size.y / 2.0);
                let to = vec2(p1.x + dx, p1.y + size.y / 2.0);
                if let Some(hit) = self.world.raycast(from, to) {
                    let f = hit.distance / from.distance(to).max(1e-3);
                    first = Some(first.map_or(f, |x: f32| x.min(f)));
                }
            }
            if let Some(f) = first {
                let land = p0.lerp(p1, f);
                arc.push(land);
                return Some(Prediction { arc, land, hits: true });
            }
            if p1.y > PLATE_TOP + LOST_BELOW + 60.0 {
                return Some(Prediction { arc, land: p1, hits: false });
            }
        }
        let land = *arc.last()?;
        Some(Prediction { arc, land, hits: false })
    }

    /// How strong twists are right now (1 = the first one).
    fn strength(&self) -> f32 {
        (1.0 + self.twists.saturating_sub(1) as f32 * TWIST_GROWTH).min(TWIST_CAP)
    }

    // ---- input ------------------------------------------------------------

    /// Lets go of the carried snack (the player's tap). False if the bird
    /// has none right now.
    pub fn drop_piece(&mut self) -> bool {
        if self.ending.is_some() || self.held.is_none() {
            return false;
        }
        self.release();
        self.drops += 1;
        self.moving = true;
        self.respawn = Some(RESPAWN);
        self.cues.push(Cue::Drop);
        true
    }

    /// Turns the carried snack into a falling body.
    fn release(&mut self) {
        let Some(pos) = self.held_pos() else { return };
        let Some(h) = self.held.take() else { return };
        let damping = h.kind.damping();
        let body = Body::dynamic(pos.x, pos.y).velocity(self.bird_vx() * CARRY, 0.0).damping(damping.0, damping.1);
        let id = self.world.add(body, h.kind.shape());
        self.pieces.push(Piece { id, kind: h.kind, variant: h.variant, state: State::Falling { rest: 0.0 }, age: 0.0, fear: 0.0, solid: None, grip: None });
    }

    // ---- what comes next -------------------------------------------------------

    /// The kind of the next snack: a newly unlocked kind first, else random
    /// among the unlocked ones (never two awkward ones in a row).
    fn next_kind(&mut self) -> Kind {
        if self.unlocked < UNLOCKS.len() && self.score >= UNLOCKS[self.unlocked].0 {
            let k = UNLOCKS[self.unlocked].1;
            self.unlocked += 1;
            self.cues.push(Cue::NewKind(k));
            return k;
        }
        loop {
            let k = UNLOCKS[(self.rng.next_u32() as usize) % self.unlocked].1;
            if !(k.awkward() && self.last_kind.is_some_and(|l| l.awkward())) {
                return k;
            }
        }
    }

    fn spawn(&mut self) {
        let kind = self.next_kind();
        self.last_kind = Some(kind);
        let variant = self.rng.next_u32() % 3;
        self.held = Some(Held { kind, variant, age: 0.0 });
    }

    // ---- the step ---------------------------------------------------------

    pub fn step(&mut self, dt: f32) {
        self.t += dt;
        // The bird: sweeps wider and faster as the tower grows (eased).
        let s = self.score as f32;
        let amp = (SWEEP_AMP.0 + SWEEP_AMP.1 * s).min(SWEEP_AMP.2);
        let speed = (SWEEP_SPEED.0 + SWEEP_SPEED.1 * s).min(SWEEP_SPEED.2);
        let k = 1.0 - (-dt * 1.5).exp();
        self.amp += (amp - self.amp) * k;
        self.speed += (speed - self.speed) * k;
        if self.moving {
            self.phase += self.speed * dt;
        }
        if let Some(h) = &mut self.held {
            h.age += dt;
        }

        self.twists(dt);
        // Wind pushes every snack in the air or on the tower.
        if self.wind != 0.0 {
            for p in &self.pieces {
                if !matches!(p.state, State::Lost { .. }) {
                    let m = self.world.mass(p.id);
                    self.world.apply_force(p.id, vec2(self.wind * m, 0.0));
                }
            }
        }
        // The plate only moves while it tilts (a resting tower can sleep).
        let pose = self.world.pose(self.plate);
        if (pose.angle - self.plate_angle).abs() > 1e-5 {
            self.world.move_kinematic(self.plate, pose.pos, self.plate_angle);
        }
        self.world.step(dt);
        self.contacts();
        self.settle(dt);
        self.fall_off(dt);

        if let Some(r) = &mut self.respawn {
            *r -= dt;
            if *r <= 0.0 && self.ending.is_none() {
                self.respawn = None;
                self.spawn();
            }
        }

        // Camera: keep the top of the tower in view, easing.
        self.height = self.measure_height();
        self.best_height = self.best_height.max(self.height);
        let want = (PLATE_TOP - self.height - TOWER_TOP_ON_SCREEN).min(0.0);
        // After the last heart the camera drops to the pile on the blanket.
        let (want, halflife) = if self.ending.is_some() { (0.0, 0.28) } else { (want, 0.3) };
        self.cam.halflife = halflife;
        self.cam_y = self.cam.update(want, dt);

        if let Some(e) = &mut self.ending {
            *e += dt;
            if *e >= ENDING && !self.over {
                self.over = true;
                self.cues.push(Cue::Over);
            }
        }
    }

    /// Wind gusts and plate tilts: announced, then on, then calm again.
    fn twists(&mut self, dt: f32) {
        if let Some(e) = self.ending {
            // The plate tips everything off (the fail animation).
            self.wind = 0.0;
            let want = self.tip_dir * 0.7 * (e / 1.0).min(1.0);
            self.plate_angle = want;
            return;
        }
        self.twist = match self.twist {
            Twist::Calm { left } if left - dt <= 0.0 => {
                let kind = if self.twists.is_multiple_of(2) { TwistKind::Gust } else { TwistKind::Tilt };
                let dir = if self.rng.next_u32().is_multiple_of(2) { 1.0 } else { -1.0 };
                self.twists += 1;
                self.cues.push(Cue::Warn(kind, dir));
                Twist::Warn { kind, dir, t: 0.0 }
            }
            Twist::Calm { left } => Twist::Calm { left: left - dt },
            Twist::Warn { kind, dir, t } if t + dt >= WARN_TIME => {
                self.cues.push(Cue::Start(kind));
                Twist::On { kind, dir, t: 0.0 }
            }
            Twist::Warn { kind, dir, t } => Twist::Warn { kind, dir, t: t + dt },
            Twist::On { kind, dir, t } => {
                let t = t + dt;
                let len = if kind == TwistKind::Gust { GUST_TIME } else { TILT_TIME };
                if t >= len {
                    Twist::Calm { left: self.rng.range(TWIST_GAP.0, TWIST_GAP.1) }
                } else {
                    Twist::On { kind, dir, t }
                }
            }
        };
        let strength = self.strength();
        (self.wind, self.plate_angle) = match self.twist {
            Twist::On { kind: TwistKind::Gust, dir, t } => (dir * GUST_ACCEL * strength * (std::f32::consts::PI * t / GUST_TIME).sin(), 0.0),
            Twist::On { kind: TwistKind::Tilt, dir, t } => (0.0, dir * TILT_MAX * strength.min(1.5) * (std::f32::consts::PI * t / TILT_TIME).sin()),
            _ => (0.0, 0.0),
        };
    }

    /// Impacts: thuds (at most three a step) and scared faces.
    fn contacts(&mut self) {
        let mut thuds = 0;
        let events: Vec<Event> = self.world.events().to_vec();
        for e in events {
            let Event::ContactBegin(c) = e else { continue };
            // A falling snack's first touch: it grips.
            for id in [c.a, c.b] {
                if let Some(p) = self.pieces.iter_mut().find(|p| p.id == id && p.grip.is_none() && matches!(p.state, State::Falling { .. })) {
                    p.grip = Some(GRIP_TIME);
                    self.world.set_damping(id, GRIP, GRIP);
                }
            }
            if c.speed < THUD_SPEED {
                continue;
            }
            if thuds < 3 {
                thuds += 1;
                let vol = ((c.speed - 50.0) / 500.0).clamp(0.1, 1.0);
                self.cues.push(Cue::Thud { x: c.point.x, y: c.point.y, vol, heavy: c.speed > HEAVY_SPEED });
            }
            for id in [c.a, c.b] {
                if let Some(p) = self.pieces.iter_mut().find(|p| p.id == id) {
                    p.fear = (p.fear + c.speed / 600.0).min(1.0);
                }
            }
        }
    }

    /// A dropped snack counts once it rests on the tower; faces calm down
    /// or get worried with the tower's sway.
    fn settle(&mut self, dt: f32) {
        let mut sway: f32 = 0.0;
        for i in 0..self.pieces.len() {
            let id = self.pieces[i].id;
            let speed = self.world.velocity(id).length();
            let spin = self.world.spin(id).abs();
            let p = &mut self.pieces[i];
            p.age += dt;
            if let Some(g) = &mut p.grip
                && *g > 0.0
            {
                *g -= dt;
                if *g <= 0.0 {
                    let (lin, ang) = p.kind.damping();
                    self.world.set_damping(id, lin, ang);
                }
            }
            let p = &mut self.pieces[i];
            let calm = (1.0 - dt * 1.2).max(0.0);
            p.fear = (p.fear * calm).max(((speed - 30.0) / 260.0).clamp(0.0, 1.0)).max((spin / 2.5).min(1.0));
            match p.state {
                State::Landed => sway = sway.max(speed / 120.0 + spin / 1.5),
                State::Falling { rest } => {
                    let still = speed < SETTLE_SPEED && spin < 0.5;
                    let support = self.support(id);
                    let rest = if still && support.is_some() { rest + dt } else { 0.0 };
                    self.pieces[i].state = State::Falling { rest };
                    if rest >= SETTLE_TIME && self.ending.is_none() {
                        self.pieces[i].state = State::Landed;
                        self.land(i, support.unwrap_or((W / 2.0, PLATE_W / 2.0)));
                    }
                }
                State::Lost { .. } => {}
            }
        }
        // The tower's sway (rises fast, settles slowly) worries every face on it.
        let target = sway.min(1.0).max(if self.wind != 0.0 || self.plate_angle != 0.0 { 0.5 } else { 0.0 });
        let rate = if target > self.wobble { 8.0 } else { 1.5 };
        self.wobble += (target - self.wobble) * (1.0 - (-dt * rate).exp());
        let wobble = self.wobble;
        for p in self.pieces.iter_mut().filter(|p| p.state == State::Landed && p.solid.is_none()) {
            p.fear = p.fear.max(wobble * 0.9);
        }
        // Deep, still snacks settle into the solid base.
        let top = self.tower_top().y;
        for i in 0..self.pieces.len() {
            let p = &self.pieces[i];
            if p.state == State::Landed && p.solid.is_none() && self.ending.is_none() {
                let (pos, v) = (self.world.position(p.id), self.world.velocity(p.id));
                if pos.y > top + SOLID_BELOW && v.length() < 8.0 {
                    let j = self.world.join(Joint::fixed(self.plate, p.id));
                    self.pieces[i].solid = Some(j);
                }
            }
        }
    }

    /// If `id` rests on the tower (the plate or a landed snack),
    /// what it rests on: its centre x and half its width.
    fn support(&self, id: BodyId) -> Option<(f32, f32)> {
        let me = self.world.position(id).y;
        let mut best: Option<(f32, f32, f32)> = None;
        for other in self.world.contacts(id) {
            let half = if other == self.plate {
                PLATE_W / 2.0
            } else {
                match self.pieces.iter().find(|p| p.id == other && p.state == State::Landed) {
                    Some(p) => p.kind.size().x / 2.0,
                    None => continue,
                }
            };
            let pos = self.world.position(other);
            if pos.y > me && best.is_none_or(|b| pos.y < b.1) {
                best = Some((pos.x, pos.y, half));
            }
        }
        best.map(|b| (b.0, b.2))
    }

    fn land(&mut self, i: usize, (support_x, half): (f32, f32)) {
        self.score += 1;
        self.streak += 1;
        let pos = self.world.position(self.pieces[i].id);
        let off = (pos.x - support_x).abs();
        let (neat, close) = (off < NEAT_PX, off > half * CLOSE_FRACTION);
        self.cues.push(Cue::Land { x: pos.x, y: pos.y, streak: self.streak, neat, close });
        if HEART_BACK > 0 && self.streak.is_multiple_of(HEART_BACK) && self.lives < LIVES {
            self.lives += 1;
            self.cues.push(Cue::HeartBack);
        }
    }

    /// Snacks that fall off cost a heart; the last one starts the collapse.
    /// Fallen snacks leave the world once out of sight.
    fn fall_off(&mut self, dt: f32) {
        self.grace = (self.grace - dt).max(0.0);
        for i in 0..self.pieces.len() {
            let pos = self.world.position(self.pieces[i].id);
            match &mut self.pieces[i].state {
                State::Lost { t } => *t += dt,
                _ if pos.y > PLATE_TOP + LOST_BELOW => {
                    let landed = self.pieces[i].state == State::Landed;
                    self.pieces[i].state = State::Lost { t: 0.0 };
                    self.pieces[i].fear = 1.0;
                    let counted = self.ending.is_none() && self.grace <= 0.0;
                    let kind = self.pieces[i].kind;
                    self.cues.push(Cue::Lost { x: pos.x, y: pos.y, kind, counted, landed });
                    if counted {
                        self.grace = TUMBLE_GRACE;
                        self.lose_heart(pos.x);
                    }
                }
                _ => {}
            }
        }
        let (world, linger) = (&mut self.world, if self.ending.is_some() { f32::MAX } else { LOST_LINGER });
        self.pieces.retain(|p| {
            let gone = matches!(p.state, State::Lost { t } if t > linger || world.position(p.id).y > GROUND_Y + 200.0);
            if gone {
                world.remove(p.id);
            }
            !gone
        });
    }

    fn lose_heart(&mut self, x: f32) {
        self.streak = 0;
        self.lives = self.lives.saturating_sub(1);
        if self.lives == 0 {
            self.ending = Some(0.0);
            self.twist = Twist::Calm { left: f32::MAX };
            // Tip away from the side it fell on: whatever's left goes the other way.
            let lean = self.tower_top().x - W / 2.0;
            self.tip_dir = if lean.abs() > 4.0 { lean.signum() } else if x < W / 2.0 { 1.0 } else { -1.0 };
            self.cues.push(Cue::Collapse);
            // Everything comes loose for the fall.
            for p in &mut self.pieces {
                if let Some(j) = p.solid.take() {
                    self.world.unjoin(j);
                }
            }
            // The bird lets go of what it carries too.
            if self.held.is_some() {
                self.release();
            }
            self.respawn = None;
        }
    }

    /// The highest point of anything resting on the tower, above the plate.
    fn measure_height(&self) -> f32 {
        (PLATE_TOP - self.tower_top().y).max(0.0)
    }
}
