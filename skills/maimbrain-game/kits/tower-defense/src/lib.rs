//! Cake Keep: jellies march down a winding road toward your cake. Tap a pad
//! beside the road to build a tower (POP, CHILL or BOOM), tap a tower to
//! upgrade it, and pop jellies for coins. Every jelly that reaches the cake
//! takes a bite; five bites and it's devoured.
//!
//! Layout: `sim` (rules, tested), `autopilot` (a building heuristic: the
//! title card's attract mode and the test bot), `draw` (gfx2d), `sound`
//! (sounds and haptics from the sim's cues), `art` (the generated clay
//! sheets), `look` (words, theme, palette), this file (input, the build picker, juice).
//! The round itself (title card, a tap starting play, the score, the best,
//! the game-over card, retrying) is `maimbrain::rounds::Rounds`;
//! `tests::keeps_the_platform_rules` checks it.

mod art;
mod autopilot;
mod draw;
mod look;
mod sim;
mod sound;
#[cfg(test)]
mod tests;

use maimbrain::input;
use maimbrain::juice::{Particle, Particles, Popups, VignettePulse};
use maimbrain::motion::{HitStop, Pulse, Punch, Shake};
use maimbrain::rounds::{Input, Rounds, Stage};
use maimbrain::sys::{self, Level};
use maimbrain::ui::{Hud, Rect, ResultsEvent, Theme, TitleCard};
use maimbrain::{Game, Rng, export_game};

use autopilot::{Action, Planner, Style};
use sim::{CAKE, CreepKind, Cue, HEARTS, HINT_PAD, PADS, Sim, TowerKind};

const BOARD: u32 = 0;
/// Hints come back after this long without a tap.
const HINT_IDLE: f32 = 3.0;
/// The picker's round buttons: radius, and distance from the pad.
pub const SLOT_R: f32 = 27.0;
const SLOT_GAP: f32 = 64.0;
const SLOT_OFFSET: f32 = 70.0;
/// The title card's autopilot: seconds between its moves, and how many
/// towers it builds (few enough that the cake falls now and then).
const ATTRACT_THINK: f32 = 1.6;
const ATTRACT_STYLE: Style = Style { max_towers: 5, pads_below: 280.0, upgrades: true, call_early: false };

/// A popped jelly's burst (its sheet's pop frame), drawn for `LIFE` seconds.
pub struct Splat {
    pub x: f32,
    pub y: f32,
    pub kind: CreepKind,
    pub flip: bool,
    pub age: f32,
}

impl Splat {
    pub const LIFE: f32 = 0.32;
}

/// What the last tap selected.
#[derive(Clone, Copy, PartialEq)]
pub enum Sel {
    None,
    /// An empty pad: the tower picker is open.
    Pad(usize),
    /// A built tower: its upgrade bubble is open.
    Tower(usize),
}

pub struct Keep {
    /// Title (the game plays itself), round, game-over card.
    pub rounds: Rounds,
    pub sim: Sim,
    /// Seeds the title card's games.
    rng: Rng,
    pub time: f32,
    /// Seconds since the player last tapped.
    pub idle: f32,
    pub sel: Sel,
    /// Seconds since the selection changed (pop-in).
    pub sel_t: f32,
    /// A picker button that refused (too poor): which and how long ago.
    pub denied: Option<(usize, f32)>,
    pub taught_build: bool,
    pub taught_upgrade: bool,
    pub theme: Theme,
    pub hud: Hud,
    pub fx: Particles,
    pub popups: Popups,
    pub shake: Shake,
    hitstop: HitStop,
    pub hurt: VignettePulse,
    pub coin_punch: Punch,
    /// The cake's wince when bitten.
    pub wince: Pulse,
    /// A big centred line ("WAVE 4", "HELMETS!") and its age.
    pub banner: Option<(String, f32, bool)>,
    pub planner: Planner,
    attract_t: f32,
    pub art: art::Art,
    pub splats: Vec<Splat>,
    sound: sound::Sound,
}

/// The picker's three buttons around an empty pad: above it on the lower
/// half of the screen, below it near the top, kept on screen.
pub fn picker_slots(pad: usize) -> [(f32, f32); 3] {
    let (px, py) = PADS[pad];
    let y = if py > 200.0 { py - SLOT_OFFSET } else { py + SLOT_OFFSET };
    let cx = px.clamp(16.0 + SLOT_GAP + SLOT_R, 344.0 - SLOT_GAP - SLOT_R);
    [(cx - SLOT_GAP, y), (cx, y), (cx + SLOT_GAP, y)]
}

/// The title card's game: already under way (three towers, a couple of
/// waves on the road) so the card shows the action from its first frame.
fn attract_sim(seed: u64) -> Sim {
    let mut sim = Sim::new(seed);
    sim.coins += 150;
    sim.build(6, TowerKind::Pop);
    sim.build(10, TowerKind::Boom);
    sim.build(7, TowerKind::Chill);
    sim.coins = 0;
    for _ in 0..(60 * 9) {
        sim.step(1.0 / 60.0);
    }
    sim.call_wave();
    for _ in 0..(60 * 3) {
        sim.step(1.0 / 60.0);
    }
    sim.cues.clear();
    sim
}

/// The upgrade bubble over (or under) a tower.
pub fn upgrade_slot(pad: usize) -> (f32, f32) {
    let (px, py) = PADS[pad];
    (px.clamp(52.0, 308.0), if py > 200.0 { py - 64.0 } else { py + 64.0 })
}

impl Keep {
    /// The next-wave button (top right, under the hearts).
    pub fn wave_button(&self) -> Rect {
        Rect::new(236.0, self.rounds.layout.hud.y + 50.0, 114.0, 46.0)
    }

    /// The coin counter (top left, clear of the pause pill).
    pub fn coin_pill(&self) -> Rect {
        Rect::new(12.0, self.rounds.layout.pill_zone().bottom() + 6.0, 88.0, 30.0)
    }

    /// A round starts (on `Rounds`' seed).
    fn start(&mut self) {
        self.sim = Sim::new(self.rounds.seed());
        self.idle = 0.0;
        // The build and upgrade hints show every round, gone once done.
        self.taught_build = false;
        self.taught_upgrade = false;
        self.sel = Sel::None;
        self.banner = None;
        self.fx.clear();
        self.splats.clear();
        self.popups.list.clear();
        self.hud.reset(self.rounds.best());
        self.hud.lives = Some((HEARTS, HEARTS));
        self.sound.start();
    }

    fn select(&mut self, s: Sel) {
        if s != self.sel {
            self.sel = s;
            self.sel_t = 0.0;
            self.denied = None;
        }
    }

    /// A tap in play. Every tap gets a visible answer.
    fn tap(&mut self, x: f32, y: f32) {
        self.idle = 0.0;
        if !self.sim.playing() {
            return;
        }
        // A soft ripple wherever the finger lands.
        self.fx.ring(x, y, 22.0, 0xffffff90);
        if self.wave_button().inset(-4.0).contains(x, y) {
            let (bx, by) = (self.wave_button().cx(), self.wave_button().bottom());
            if let Some(bonus) = self.sim.call_wave() {
                if bonus > 0 {
                    self.popups.spawn(bx, by + 6.0, &format!("+{bonus}"), look::GOLD, 22.0);
                    self.coin_punch.kick(0.3);
                }
            }
            self.select(Sel::None);
            return;
        }
        match self.sel {
            Sel::Pad(p) => {
                for (k, &(sx, sy)) in picker_slots(p).iter().enumerate() {
                    if (x - sx).hypot(y - sy) <= SLOT_R + 6.0 {
                        if self.sim.build(p, TowerKind::ALL[k]) {
                            self.taught_build = true;
                            self.select(Sel::None);
                        } else {
                            self.denied = Some((k, 0.0));
                        }
                        return;
                    }
                }
            }
            Sel::Tower(p) => {
                let (ux, uy) = upgrade_slot(p);
                if (x - ux).hypot(y - uy) <= SLOT_R + 8.0 {
                    if self.sim.upgrade(p) {
                        self.taught_upgrade = true;
                        self.select(Sel::None);
                    } else if self.sim.upgrade_cost(p).is_some() {
                        self.denied = Some((0, 0.0));
                    }
                    return;
                }
            }
            Sel::None => {}
        }
        match Sim::pad_at(x, y) {
            Some(p) => {
                let want = if self.sim.towers[p].is_some() { Sel::Tower(p) } else { Sel::Pad(p) };
                if want == self.sel {
                    self.select(Sel::None);
                    self.sound.ui(false);
                } else {
                    self.select(want);
                    self.sound.ui(true);
                }
            }
            None => {
                if self.sel != Sel::None {
                    self.sound.ui(false);
                }
                self.select(Sel::None);
            }
        }
    }

    /// The title card's autopilot (deterministic: it reads only the sim).
    fn attract(&mut self, dt: f32) {
        self.attract_t += dt;
        if self.sim.over {
            if self.attract_t > 1.5 {
                self.sim = attract_sim(self.rng.next_u32() as u64);
                self.attract_t = 0.0;
            }
            return;
        }
        if self.attract_t < ATTRACT_THINK {
            return;
        }
        if let Some(a) = self.planner.want(&self.sim, &ATTRACT_STYLE) {
            if a != Action::CallWave && a.cost(&self.sim) <= self.sim.coins {
                a.apply(&mut self.sim);
                self.attract_t = 0.0;
            }
        }
    }

    /// Juice for this frame's cues (sound and haptics are in `sound`).
    fn juice(&mut self, cues: &[Cue]) {
        let playing = self.rounds.stage() != Stage::Title;
        for &cue in cues {
            match cue {
                Cue::Built { pad, kind } => {
                    let (x, y) = PADS[pad];
                    self.fx.burst(x, y, 14, look::TOWER[kind as usize]);
                    self.fx.ring(x, y, 46.0, 0xffffffc0);
                    self.fx.sparkles(x, y - 10.0, 30.0, 6, 0xfff2b0ff);
                    self.shake.add(0.12);
                }
                Cue::Upgraded { pad, level } => {
                    let (x, y) = PADS[pad];
                    self.fx.stars(x, y - 12.0, 6 + 2 * level, look::GOLD);
                    self.fx.ring(x, y, 56.0, look::GOLD);
                    self.popups.spawn(x, y - 34.0, if level == 1 { "LV 2" } else { "MAX!" }, look::GOLD, 20.0);
                }
                Cue::Frost { x, y, r } => {
                    self.fx.ring(x, y, r, 0x8fe3ffd0);
                    self.fx.sparkles(x, y, r * 0.7, 5, 0xe6f9ffff);
                }
                Cue::Boom { x, y, r } => {
                    self.fx.burst(x, y, 16, 0xffa340ff);
                    self.fx.sparks(x, y, -std::f32::consts::FRAC_PI_2, 2.6, 12, look::GOLD);
                    self.fx.ring(x, y, r * 1.2, 0xffd9a0d0);
                    self.shake.add(0.08);
                }
                Cue::Hit { x, y } => self.fx.sparkles(x, y, 8.0, 2, 0xffffffff),
                Cue::Pop { x, y, kind, coins, chain } => {
                    let col = look::JELLY[kind as usize];
                    self.splats.push(Splat { x, y, kind, flip: chain % 2 == 1, age: 0.0 });
                    self.fx.burst(x, y, 10, col);
                    // Jelly droplets that splash out and fall.
                    for i in 0..7 {
                        let a = i as f32 / 7.0 * std::f32::consts::TAU + self.fx.rng.range(-0.3, 0.3);
                        let v = self.fx.rng.range(60.0, 140.0);
                        let mut p = Particle::new(maimbrain::juice::Look::Dot, x, y, col);
                        p.vx = a.cos() * v;
                        p.vy = a.sin() * v - 60.0;
                        p.gravity = 420.0;
                        p.drag = 1.5;
                        p.life = self.fx.rng.range(0.35, 0.6);
                        p.size = self.fx.rng.range(2.5, 4.5);
                        p.size_end = 1.0;
                        self.fx.add(p);
                    }
                    if kind == CreepKind::King {
                        self.fx.celebrate(x, y, &self.theme);
                        self.shake.add(0.4);
                        self.hitstop.freeze(0.08);
                    }
                    if playing {
                        self.popups.spawn(x, y - 14.0, &format!("+{coins}"), look::GOLD, if chain >= 3 { 22.0 } else { 18.0 });
                        self.hud.set_score(self.sim.score as i64);
                        self.coin_punch.kick(0.15);
                    }
                }
                Cue::Split { x, y } => self.fx.ring(x, y, 30.0, 0xffd0a0c0),
                Cue::Bite { x, y, .. } => {
                    self.shake.add(0.45);
                    self.hurt.fire();
                    self.wince.fire();
                    self.hitstop.freeze(0.06);
                    self.fx.burst(x, y, 12, look::CAKE_SPONGE);
                    self.fx.burst(CAKE.0, CAKE.1, 10, look::CAKE_FROSTING);
                    if playing {
                        self.hud.lives = Some((self.sim.hearts, HEARTS));
                        self.popups.spawn(CAKE.0, CAKE.1 - 50.0, "CHOMP!", look::DANGER, 24.0);
                    }
                }
                Cue::Wave { n, boss, rush, new_kind } => {
                    let text = match new_kind {
                        Some(k) => look::NEW_KIND_BANNER[k as usize].to_string(),
                        None if boss => look::BOSS_BANNER.to_string(),
                        None if rush => look::RUSH_BANNER.to_string(),
                        None => format!("WAVE {n}"),
                    };
                    self.banner = Some((text, 0.0, boss || rush || new_kind.is_some()));
                    if boss {
                        self.shake.add(0.3);
                    }
                }
                Cue::Payday { coins } => {
                    if playing && self.sim.wave > 1 {
                        let r = self.coin_pill();
                        self.popups.spawn(r.cx(), r.bottom() + 12.0, &format!("+{coins}"), look::GOLD, 18.0);
                    }
                }
                Cue::Ending => {
                    if playing {
                        sys::log(Level::Info, &format!("cake keep: devoured at wave {} with {} popped", self.sim.wave, self.sim.score));
                    }
                    self.shake.add(0.8);
                    self.hitstop.freeze(0.12);
                    self.sel = Sel::None;
                }
                Cue::Denied => {
                    self.shake.add(0.05);
                }
                Cue::Chomp { x, y } => {
                    self.fx.burst(x, y, 6, look::CAKE_SPONGE);
                    self.fx.burst(CAKE.0, CAKE.1, 5, look::CAKE_FROSTING);
                    self.wince.fire();
                    self.shake.add(0.12);
                }
                Cue::Fire { .. } | Cue::Early { .. } | Cue::Over => {}
            }
        }
    }

    /// Hint target: (x, y, text) for the animated finger, or None.
    pub fn hint(&self) -> Option<(f32, f32, &'static str)> {
        if !self.rounds.playing() || !self.sim.playing() {
            return None;
        }
        let stalled = self.idle > HINT_IDLE;
        match self.sel {
            Sel::Pad(p) if !self.taught_build || stalled => {
                let k = if self.taught_build { self.planner.next_kind(&self.sim) as usize } else { 0 };
                let (x, y) = picker_slots(p)[k];
                (self.sim.coins >= TowerKind::ALL[k].spec().cost[0]).then_some((x, y, look::HINT_BUY))
            }
            Sel::Tower(p) if !self.taught_upgrade || stalled => {
                let (x, y) = upgrade_slot(p);
                self.sim.upgrade_cost(p).is_some_and(|c| c <= self.sim.coins).then_some((x, y, look::HINT_UPGRADE))
            }
            Sel::None if !self.taught_build => Some((PADS[HINT_PAD].0, PADS[HINT_PAD].1, look::HINT_BUILD)),
            Sel::None if stalled => {
                let kind = self.planner.next_kind(&self.sim);
                if self.sim.coins >= kind.spec().cost[0]
                    && let Some(&p) = self.planner.pads_for(&self.sim, kind).first()
                {
                    return Some((PADS[p].0, PADS[p].1, look::HINT_BUILD));
                }
                if !self.taught_upgrade {
                    let p = (0..PADS.len()).find(|&p| self.sim.upgrade_cost(p).is_some_and(|c| c <= self.sim.coins))?;
                    return Some((PADS[p].0, PADS[p].1, look::HINT_UPGRADE));
                }
                None
            }
            _ => None,
        }
    }
}

impl Game for Keep {
    fn init() -> Self {
        let mut rng = Rng::from_host();
        let sim = attract_sim(rng.next_u32() as u64);
        let theme = look::theme();
        // A feed card: a hook and the best, never instructions. The sim plays
        // the devouring itself, so the card comes as soon as it's over.
        let title = TitleCard::new(look::TITLE, theme).tagline(look::TAGLINE);
        let rounds = Rounds::new(title).board(BOARD).heading(look::OVER_HEADING).label(look::SCORE_LABEL).ending(0.0);
        let mut hud = Hud::new(theme, rounds.best());
        hud.size = 44.0;
        hud.lives = Some((HEARTS, HEARTS));
        let planner = Planner::new(&sim);
        Keep {
            rounds,
            sim,
            time: 0.0,
            idle: 0.0,
            sel: Sel::None,
            sel_t: 0.0,
            denied: None,
            taught_build: false,
            taught_upgrade: false,
            theme,
            hud,
            fx: Particles::new(rng.next_u32() as u64),
            popups: Popups::new(),
            shake: Shake::new(),
            hitstop: HitStop::default(),
            hurt: VignettePulse::new(look::DANGER),
            coin_punch: Punch::new(),
            wince: Pulse::new(0.5),
            banner: None,
            planner,
            attract_t: 0.0,
            art: art::Art::new(),
            splats: Vec::new(),
            sound: sound::Sound::new(),
            rng,
        }
    }

    fn update(&mut self, dt: f32) {
        self.art.poll();
        self.time += dt;
        self.idle += dt;
        self.sel_t += dt;
        for e in input::poll() {
            match self.rounds.input(&e) {
                // The tap that enters play (or retries) is also the first move
                // (SPEC §5.2): on a pad, it opens the picker. The card's retry
                // button starts a round on its release, which isn't a move.
                Input::Start => {
                    self.start();
                    if e.is_press() {
                        self.tap(e.x, e.y);
                    }
                }
                Input::Play if e.is_press() => self.tap(e.x, e.y),
                _ => {}
            }
        }
        if self.rounds.stage() == Stage::Title {
            self.attract(dt);
        }
        let sim_dt = self.hitstop.step(dt);
        if self.rounds.stage() != Stage::Over {
            self.sim.step(sim_dt);
        }
        let cues = std::mem::take(&mut self.sim.cues);
        let playing = self.rounds.stage() != Stage::Title;
        self.sound.cues(&cues, playing);
        self.juice(&cues);
        // A tower selected for upgrade that just got built over, etc.
        if let Sel::Pad(p) = self.sel
            && self.sim.towers[p].is_some()
        {
            self.sel = Sel::None;
        }
        if self.rounds.stage() == Stage::Playing && self.sim.over {
            // The sim has played the devouring (still Playing): Rounds submits
            // the score, keeps the best, shows the card and reports Over.
            self.rounds.end(self.sim.score as i64);
        }
        if let Some(d) = &mut self.denied {
            d.1 += dt;
            if d.1 > 0.4 {
                self.denied = None;
            }
        }
        if let Some(b) = &mut self.banner {
            b.1 += dt;
            if b.1 > 1.6 {
                self.banner = None;
            }
        }
        // The card's moments: its score ticking up, a new best.
        match self.rounds.update(dt) {
            Some(ResultsEvent::NewBest) => self.sound.new_best(),
            Some(ResultsEvent::Tick) => self.sound.tick(),
            _ => {}
        }
        self.hud.update(dt);
        self.fx.update(dt);
        self.splats.retain_mut(|s| {
            s.age += dt;
            s.age < Splat::LIFE
        });
        self.popups.update(dt);
        self.shake.update(dt);
        self.hurt.update(dt);
        self.hurt.level = if self.rounds.playing() && self.sim.hearts == 1 && self.sim.playing() { 0.3 } else { 0.0 };
        self.coin_punch.update(dt);
        self.wince.update(dt);
        // The music turns urgent with danger, and for as long as a king is on the road.
        let king = self.sim.creeps.iter().any(|c| c.kind == CreepKind::King);
        let danger = if self.rounds.playing() { self.sim.danger().max(if king { 1.0 } else { 0.0 }) } else { 0.0 };
        self.sound.update(dt, danger);
    }

    fn render(&self) {
        draw::frame(self);
        // The title card, or the game-over card with the wave reached under it.
        self.rounds.draw();
        if let Some(card) = &self.rounds.results {
            draw::wave_reached(self, card.panel_rect());
        }
    }

    fn suspend(&mut self) {
        self.sound.stop_music();
    }

    fn resume(&mut self) {
        if self.rounds.playing() && self.sim.playing() {
            self.sound.resume_music();
        }
    }
}

export_game!(Keep);
