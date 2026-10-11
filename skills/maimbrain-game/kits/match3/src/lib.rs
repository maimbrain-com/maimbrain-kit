//! Poppets: a swap-to-match puzzle for one thumb. Swipe a poppet into its
//! neighbour's place to line up three of a kind; they pop, the rest fall,
//! cascades multiply. Popping keeps everyone awake: the wake meter drains
//! faster and faster, and when it's empty the whole board nods off.
//!
//! Layout: `sim` (the rules, pure and tested, with a bot in `tests`), `look`
//! (names, words, colors, sprites: reskin here), `cast` (the atlas's source
//! rects, written by `mb art atlas`), `draw` (poppets, board, meter, effects), `sound` (sounds, music, haptics from the sim's cues), this file
//! (input, buttons, juice). The round itself (title card, a tap starting
//! play, the score, the best, the game-over card, retrying) is
//! `maimbrain::rounds::Rounds`; `tests::keeps_the_platform_rules` checks it.
//!
//! Daily mode deals the same board (and the same refills, given the same
//! swaps) to everyone today (`sys::daily_seed`), on its own leaderboard and
//! best (`Rounds::daily`).

mod cast;
mod draw;
mod look;
mod sim;
mod sound;
#[cfg(test)]
mod tests;

use draw::{Mood, Shapes, cell_center};
use look::*;
use maimbrain::input::{self, Event, Kind as Ev};
use maimbrain::juice::{Flash, Particles, Popups};
use maimbrain::motion::{Ease, HitStop, Shake};
use maimbrain::rounds::{self, Input, Rounds};
use maimbrain::ui::{self, Button, ButtonKind, Hud, Icon, Layout, Rect, ResultsEvent, Tap, Theme, TitleCard, fade, text};
use maimbrain::{Game, Rng, export_game, store, sys};
use sim::{BlastKind, COLS, Cell, Cue, Kind, News, Phase, Piece, ROWS, Sim, Special};

const BOARD_ENDLESS: u32 = 0;
const BOARD_DAILY: u32 = 1;
/// The swipe hint comes back after this long without a touch.
const HINT_IDLE: f32 = 3.0;
/// A drag this far (logical units) is a swipe.
const SWIPE_MIN: f32 = 14.0;
/// Title-card autopilot: how long the poppets doze before it makes a move.
const DEMO_WAIT: f32 = 2.4;
/// Debug: in play, make the hinted move after this many seconds (0 = off).
const DEBUG_AUTOPLAY: f32 = 0.0;

struct Touch {
    cell: Cell,
    x0: f32,
    y0: f32,
    done: bool,
}

struct Banner {
    text: &'static str,
    t: f32,
    piece: Option<Piece>,
}

/// A puff of particles waiting for its piece to pop (blasts pop in a wave).
struct LaterFx {
    t: f32,
    x: f32,
    y: f32,
    color: u32,
    big: bool,
}

struct Poppets {
    /// Title (the board plays itself), round, game-over card; daily mode.
    rounds: Rounds,
    sim: Sim,
    /// Seeds the title card's demo boards.
    rng: Rng,
    time: f32,
    play_t: f32,
    idle: f32,
    /// The player has made a match (the swipe hint stops showing on its own).
    taught: bool,
    theme: Theme,
    hud: Hud,
    daily_btn: Button,
    scores_btn: Button,
    fx: Particles,
    popups: Popups,
    shake: Shake,
    hitstop: HitStop,
    flash: Flash,
    sfx: sound::Sfx,
    shapes: Shapes,
    touch: Option<Touch>,
    selected: Option<Cell>,
    /// A rock poked: it wobbles.
    nudge: Option<(Cell, f32)>,
    banner: Option<Banner>,
    later: Vec<LaterFx>,
    demo_wait: f32,
    /// What the faces and sky show (eased): 0 awake … 1 asleep.
    sleep: f32,
    meter_shown: f32,
    meter_flash: f32,
}

impl Poppets {
    /// The title card's demo board: today's in daily mode.
    fn demo_seed(&mut self) -> u64 {
        if self.rounds.is_daily() { sys::daily_seed() } else { self.rng.next_u32() as u64 }
    }

    /// A round starts (`Rounds` picked its seed: today's in daily mode).
    fn start(&mut self) {
        self.sim = Sim::new(self.rounds.seed(), false);
        self.hud.reset(self.rounds.best());
        self.play_t = 0.0;
        self.idle = 0.0;
        self.selected = None;
        self.touch = None;
        self.banner = None;
        self.later.clear();
        self.popups.list.clear();
        self.meter_shown = 1.0;
        self.sleep = 0.0;
        self.sfx.start();
    }

    /// The board is asleep (the sim played the falling asleep, still
    /// `Playing`): `Rounds` submits the score, keeps the best and shows the card.
    fn game_over(&mut self) {
        let label = if self.rounds.is_daily() { DAILY_LABEL } else { SCORE_LABEL };
        self.rounds.end(self.sim.score as i64).label(label);
        self.daily_btn.reset();
        self.scores_btn.reset();
        self.selected = None;
        self.touch = None;
    }

    /// Daily on or off: back to the title (from the card too), with that
    /// mode's best and a demo board.
    fn toggle_daily(&mut self) {
        self.rounds.set_daily(!self.rounds.is_daily());
        self.sfx.ui();
        self.daily_btn.kind = if self.rounds.is_daily() { ButtonKind::Primary } else { ButtonKind::Secondary };
        let seed = self.demo_seed();
        self.sim = Sim::new(seed, true);
    }

    /// Offers an event to DAILY and SCORES (on the title, and on the card
    /// once its guard is over); true if one took it.
    fn ui_event(&mut self, e: &Event) -> bool {
        match self.rounds.stage() {
            rounds::Stage::Title => match self.daily_btn.handle(e) {
                Some(Tap::Clicked) => {
                    self.toggle_daily();
                    true
                }
                Some(_) => true,
                None => false,
            },
            rounds::Stage::Over if self.rounds.card_ready() => {
                if let Some(tap) = self.daily_btn.handle(e) {
                    if tap == Tap::Clicked {
                        self.toggle_daily();
                    }
                    return true;
                }
                if let Some(tap) = self.scores_btn.handle(e) {
                    if tap == Tap::Clicked {
                        self.sfx.ui();
                        store::show_scores(if self.rounds.is_daily() { BOARD_DAILY } else { BOARD_ENDLESS });
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn cell_at(x: f32, y: f32) -> Option<Cell> {
        let (fx, fy) = ((x - BOARD_X) / CELL, (y - BOARD_Y) / CELL);
        (fx >= 0.0 && fy >= 0.0 && (fx as usize) < COLS && (fy as usize) < ROWS).then_some((fx as usize, fy as usize))
    }

    /// A press in play.
    fn press(&mut self, x: f32, y: f32) {
        self.idle = 0.0;
        if matches!(self.sim.phase, Phase::Asleep { .. }) {
            return;
        }
        let Some(c) = Self::cell_at(x, y) else {
            self.selected = None;
            return;
        };
        if let Some(s) = self.selected
            && sim::adjacent(s, c)
        {
            self.swap(s, c);
            return;
        }
        if self.sim.board.get(c).is_some_and(|p| p.kind == Kind::Rock) {
            self.nudge = Some((c, 0.0));
            self.selected = None;
            self.sfx.play_rock_poke();
            return;
        }
        self.selected = if self.selected == Some(c) { None } else { Some(c) };
        self.touch = Some(Touch { cell: c, x0: x, y0: y, done: false });
        self.sfx.select();
    }

    fn drag(&mut self, x: f32, y: f32) {
        let Some(t) = &mut self.touch else { return };
        if t.done {
            return;
        }
        let (dx, dy) = (x - t.x0, y - t.y0);
        if dx.abs().max(dy.abs()) < SWIPE_MIN {
            return;
        }
        t.done = true;
        let c = t.cell;
        let to = if dx.abs() > dy.abs() {
            (c.0 as i32 + dx.signum() as i32, c.1 as i32)
        } else {
            (c.0 as i32, c.1 as i32 + dy.signum() as i32)
        };
        if to.0 >= 0 && to.1 >= 0 && (to.0 as usize) < COLS && (to.1 as usize) < ROWS {
            self.swap(c, (to.0 as usize, to.1 as usize));
        } else {
            self.selected = None;
        }
    }

    fn swap(&mut self, a: Cell, b: Cell) {
        self.selected = None;
        self.touch = None;
        self.idle = 0.0;
        if !self.sim.swap(a, b) {
            let rock = if self.sim.board.get(a).is_some_and(|p| p.kind == Kind::Rock) { a } else { b };
            self.nudge = Some((rock, 0.0));
            self.sfx.play_rock_poke();
        }
    }

    /// Juice for the sim's cues; sound only in play.
    fn react(&mut self, cues: &[Cue]) {
        let playing = self.rounds.playing();
        self.sfx.cues(cues, playing);
        let center = cell_center(COLS as f32 / 2.0 - 0.5, ROWS as f32 / 2.0 - 0.5);
        for &cue in cues {
            match cue {
                Cue::Pop { chain, .. } => {
                    let n = self.sim.last.popping.len();
                    for p in &self.sim.last.popping {
                        let (x, y) = cell_center(p.at.0 as f32, p.at.1 as f32);
                        let color = match p.piece.kind {
                            Kind::Color(k) => PIECES[k as usize],
                            Kind::Rainbow => 0xffffffff,
                            Kind::Rock => ROCK,
                        };
                        self.later.push(LaterFx { t: p.delay + 0.08, x, y, color, big: n < 12 || p.piece.is_special() });
                    }
                    for g in &self.sim.last.groups {
                        let (x, y) = cell_center(g.x, g.y);
                        self.popups.score(x, y - 6.0, g.points as i64, GOLD);
                    }
                    if chain >= 2 && playing {
                        let size = 24.0 + 3.0 * chain.min(6) as f32;
                        self.popups.spawn(center.0, BOARD_Y - 4.0 + 30.0, &format!("×{chain} CASCADE!"), GOLD, size);
                        self.shake.add(0.08 * chain.min(5) as f32);
                    }
                    self.hud.set_score(self.sim.score as i64);
                    self.meter_flash = 1.0;
                    self.demo_wait = 0.0;
                    if playing {
                        self.taught = true;
                    }
                }
                Cue::Fire { kind } => {
                    let (trauma, stop) = match kind {
                        BlastKind::Row | BlastKind::Col => (0.3, 0.04),
                        BlastKind::Area(_) | BlastKind::Cross(_) => (0.45, 0.06),
                        BlastKind::Zap => (0.35, 0.05),
                        BlastKind::Board => (0.8, 0.1),
                    };
                    self.shake.add(trauma);
                    self.hitstop.freeze(stop);
                }
                Cue::Combo => {
                    self.flash.fire(0xffffff70);
                    self.popups.spawn(center.0, center.1, "COMBO!", 0xffffffff, 34.0);
                }
                Cue::Made { at, kind, special } => {
                    let (x, y) = cell_center(at.0 as f32, at.1 as f32);
                    self.fx.ring(x, y, 46.0, 0xffffffd0);
                    self.fx.sparkles(x, y, 30.0, 8, 0xfff2b0ff);
                    let y = y + 52.0;
                    let word = match (kind, special) {
                        (Kind::Rainbow, _) => "RAINBOW!",
                        (_, Special::Bomb) => "BOMB!",
                        _ => "BLASTER!",
                    };
                    self.popups.spawn(x, y - 28.0, word, 0xffffffff, 18.0); // under the special, clear of the score
                }
                Cue::Star { at } => {
                    let (x, y) = cell_center(at.0 as f32, at.1 as f32);
                    self.fx.stars(x, y, 10, GOLD);
                    self.popups.spawn(x, y - 30.0, "WIDE AWAKE!", GOLD, 18.0);
                }
                Cue::Level { news, .. } => {
                    let (text, piece) = match news {
                        News::Faster => ("FASTER!", None),
                        News::NewColor => ("NEW POPPET!", Some(Kind::Color(self.sim.colors - 1))),
                        News::Rocks => ("ROCKS INCOMING!", Some(Kind::Rock)),
                        News::Stars => ("STAR POPPETS!", Some(Kind::Color(2))),
                        News::MoreRocks => ("MORE ROCKS!", Some(Kind::Rock)),
                    };
                    let piece = piece.map(|kind| Piece { kind, special: Special::None, star: news == News::Stars, id: 7, from_x: 0.0, from_y: 0.0 });
                    self.banner = Some(Banner { text, t: 0.0, piece });
                }
                Cue::Shuffle => self.banner = Some(Banner { text: "SHUFFLE!", t: 0.0, piece: None }),
                Cue::Phew => self.popups.spawn(center.0, center.1 - 40.0, "PHEW!", 0x8ff7c0ff, 30.0),
                Cue::Asleep => {
                    self.shake.add(0.5);
                    self.selected = None;
                }
                _ => {}
            }
        }
    }

    /// Title card: the poppets nod off, then a move wakes them (and again).
    fn autopilot(&mut self, dt: f32) {
        if !self.sim.ready() {
            return;
        }
        self.demo_wait += dt;
        if self.demo_wait > DEMO_WAIT
            && let Some(m) = self.sim.hint
        {
            self.sim.swap(m.0, m.1);
            self.demo_wait = 0.0;
        }
    }

    // ---- drawing ----

    /// Where a piece is drawn: (x, y, squash x, squash y, alpha).
    fn piece_pos(&self, c: usize, r: usize, p: &Piece) -> (f32, f32, f32, f32, f32) {
        let (bx, by) = cell_center(c as f32, r as f32);
        let lerp_cells = |a: Cell, b: Cell, k: f32| {
            let (ax, ay) = cell_center(a.0 as f32, a.1 as f32);
            let (cx, cy) = cell_center(b.0 as f32, b.1 as f32);
            (ax + (cx - ax) * k, ay + (cy - ay) * k)
        };
        match self.sim.phase {
            Phase::Swap { a, b, t, .. } | Phase::Bounce { a, b, t } => {
                let k = Ease::QuadInOut.at(t / sim::SWAP_TIME);
                let k = if matches!(self.sim.phase, Phase::Bounce { .. }) { 1.0 - k } else { k };
                if (c, r) == a {
                    let (x, y) = lerp_cells(a, b, k);
                    return (x, y, 1.0 + 0.1 * (k * std::f32::consts::PI).sin(), 1.0, 1.0);
                }
                if (c, r) == b {
                    let (x, y) = lerp_cells(b, a, k);
                    return (x, y, 1.0, 1.0, 1.0);
                }
            }
            Phase::Fall { t, .. } => {
                let (yc, landed) = sim::fall_y(p.from_y, r as f32, t);
                let (_, y) = cell_center(c as f32, yc);
                let moved = p.from_y < r as f32 - 0.01;
                let (sx, sy) = if !moved {
                    (1.0, 1.0)
                } else if landed < 0.0 {
                    (0.94, 1.08)
                } else if landed < 0.16 {
                    let s = (landed / 0.16 * std::f32::consts::PI).sin() * 0.16;
                    (1.0 + s, 1.0 - s)
                } else {
                    (1.0, 1.0)
                };
                let alpha = ((yc + 1.0) * 1.5).clamp(0.0, 1.0);
                return (bx, y + (1.0 - sy) * CELL * 0.3, sx, sy, alpha);
            }
            Phase::Shuffle { t } => {
                let k = Ease::BackInOut.at((t / sim::SHUFFLE_TIME).min(1.0));
                let (fx, fy) = cell_center(p.from_x, p.from_y);
                let s = 1.0 - 0.3 * (k * std::f32::consts::PI).sin();
                return (fx + (bx - fx) * k, fy + (by - fy) * k, s, s, 1.0);
            }
            Phase::Asleep { t } => {
                // Slumping: sag a little, one by one.
                let k = ((t * 1.6 - r as f32 * 0.1 - c as f32 * 0.04) / 0.5).clamp(0.0, 1.0);
                return (bx, by + 4.0 * k, 1.0 + 0.06 * k, 1.0 - 0.1 * k, 1.0);
            }
            _ => {}
        }
        if let Some((n, t)) = self.nudge
            && n == (c, r)
        {
            return (bx + 5.0 * (t * 40.0).sin() * (1.0 - t / 0.3).max(0.0), by, 1.0, 1.0, 1.0);
        }
        (bx, by, 1.0, 1.0, 1.0)
    }

    fn mood(&self, c: usize, r: usize, p: &Piece) -> Mood {
        if self.rounds.stage() == rounds::Stage::Over {
            return Mood::Asleep;
        }
        match self.sim.phase {
            Phase::Asleep { t } => {
                let k = t * 1.6 - r as f32 * 0.1 - c as f32 * 0.04;
                return if k > 0.35 { Mood::Asleep } else { Mood::Sleepy(0.7 + k) };
            }
            Phase::Swap { a, b, .. } if (c, r) == a || (c, r) == b => return Mood::Excited,
            Phase::Bounce { a, b, .. } if (c, r) == a || (c, r) == b => return Mood::Huh,
            _ => {}
        }
        if self.selected == Some((c, r)) {
            return Mood::Excited;
        }
        if self.sleep > 0.02 {
            // Each one nods at its own pace.
            let wobble = 0.85 + 0.15 * (self.time * 1.3 + p.id as f32 * 0.9).sin();
            return Mood::Sleepy(self.sleep * wobble);
        }
        Mood::Happy
    }

    fn look(&self, c: usize, r: usize, p: &Piece) -> (f32, f32) {
        if let Some(s) = self.selected {
            let (dx, dy) = (s.0 as f32 - c as f32, s.1 as f32 - r as f32);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 0.0 {
                return (dx / l, dy / l);
            }
        }
        let id = p.id as f32;
        ((self.time * 0.6 + id * 1.7).sin(), (self.time * 0.45 + id * 2.3).cos() * 0.6)
    }

    fn draw_board(&self) {
        let radius = CELL * 0.43;
        let intro = if self.rounds.playing() { self.play_t } else { 10.0 };
        let mut top: Vec<(usize, usize)> = Vec::new();
        for (c, r) in sim::Board::cells() {
            let Some(p) = self.sim.board.get((c, r)) else { continue };
            let lifted = self.selected == Some((c, r)) || matches!(self.sim.phase, Phase::Swap { a, .. } | Phase::Bounce { a, .. } if a == (c, r));
            if lifted {
                top.push((c, r));
                continue;
            }
            self.draw_piece(c, r, &p, radius, intro);
        }
        for (c, r) in top {
            let p = self.sim.board.get((c, r)).unwrap();
            if self.selected == Some((c, r)) {
                let (x, y) = cell_center(c as f32, r as f32);
                let pulse = 0.5 + 0.5 * (self.time * 8.0).sin();
                ui::shape::soft_disc(x, y, CELL * 0.56, 10.0, fade(0xffe6b0ff, 0.5 + 0.3 * pulse));
            }
            self.draw_piece(c, r, &p, radius * if self.selected == Some((c, r)) { 1.12 } else { 1.06 }, intro);
        }
        // Popping pieces: swell, squeeze, vanish (or fly into the special they made).
        if let Phase::Pop { t, .. } = self.sim.phase {
            for pp in &self.sim.last.popping {
                let local = t - pp.delay;
                let (bx, by) = cell_center(pp.at.0 as f32, pp.at.1 as f32);
                if local < 0.0 {
                    let jig = 1.5 * (self.time * 50.0 + pp.at.0 as f32).sin();
                    draw::piece(&self.shapes, &pp.piece, bx + jig, by, radius, 1.0, 1.0, Mood::Excited, (0.0, 0.0), self.time, 1.0);
                    continue;
                }
                let k = (local / sim::POP_TIME).clamp(0.0, 1.0);
                if let Some(to) = pp.to {
                    let (tx, ty) = cell_center(to.0 as f32, to.1 as f32);
                    let e = Ease::QuadIn.at(k);
                    draw::piece(&self.shapes, &pp.piece, bx + (tx - bx) * e, by + (ty - by) * e, radius * (1.0 - 0.5 * e), 1.0, 1.0, Mood::Squeeze, (0.0, 0.0), self.time, 1.0 - 0.6 * e);
                } else {
                    let s = if k < 0.35 { 1.0 + 0.4 * Ease::QuadOut.at(k / 0.35) } else { 1.4 * (1.0 - Ease::QuadIn.at((k - 0.35) / 0.65)) };
                    draw::piece(&self.shapes, &pp.piece, bx, by, radius * s, 1.0, 1.0, Mood::Squeeze, (0.0, 0.0), self.time, 1.0 - k * 0.5);
                }
            }
            for b in &self.sim.last.blasts {
                draw::blast(b, t - b.delay);
            }
        }
    }

    fn draw_piece(&self, c: usize, r: usize, p: &Piece, radius: f32, intro: f32) {
        let (x, y, sx, sy, alpha) = self.piece_pos(c, r, p);
        let k = ((intro - (c + r) as f32 * 0.015) / 0.22).clamp(0.0, 1.0);
        let pop = if k < 1.0 { Ease::BackOut.at(k) } else { 1.0 };
        draw::piece(&self.shapes, p, x, y, radius * pop, sx, sy, self.mood(c, r, p), self.look(c, r, p), self.time, alpha);
    }

    fn draw_banner(&self) {
        let Some(b) = &self.banner else { return };
        let k = Ease::BackOut.at((b.t / 0.3).min(1.0));
        let a = if b.t > 1.5 { 1.0 - (b.t - 1.5) / 0.3 } else { 1.0 };
        if a <= 0.0 {
            return;
        }
        let y = BOARD_Y + CELL * 3.6;
        maimbrain::gfx2d::push();
        maimbrain::gfx2d::translate(180.0, y);
        maimbrain::gfx2d::scale(k * (0.9 + 0.1 * a), k * (0.9 + 0.1 * a));
        ui::ribbon(b.text, 0.0, 0.0, 46.0, self.theme.accent, 0xffffffff, self.theme.title_font, -0.04);
        maimbrain::gfx2d::pop();
        if let Some(p) = &b.piece {
            let bob = (self.time * 6.0).sin() * 4.0;
            draw::piece(&self.shapes, p, 180.0, y - 62.0 + bob, CELL * 0.62 * k, 1.0, 1.0, Mood::Excited, (0.0, 0.3), self.time, a);
        }
    }
}

impl Game for Poppets {
    fn init() -> Self {
        let layout = Layout::new();
        let theme = look::theme();
        let mut rng = Rng::from_host();
        // Endless on board 0; daily on board 1 with its own best. The sim
        // plays the falling asleep itself, so the card comes as soon as it's over.
        let title = TitleCard::new(TITLE, theme).tagline(TAGLINE).size(56.0).at(layout.safe.y + 62.0);
        let rounds = Rounds::new(title).board(BOARD_ENDLESS).daily(BOARD_DAILY).heading(HEADING).ending(0.0);
        // DAILY and SCORES sit low in the card area, clear of the feed's overlays.
        let row = Rect::new(layout.card.x, layout.card.bottom() - 46.0, layout.card.w, 44.0).columns(2, 12.0);
        let sim = Sim::new(rng.next_u32() as u64, true);
        Poppets {
            sim,
            time: 0.0,
            play_t: 0.0,
            idle: 0.0,
            taught: false,
            hud: Hud::new(theme, rounds.best()),
            rounds,
            daily_btn: Button::new(row[1], "DAILY").kind(ButtonKind::Secondary).with_icon(Icon::STAR),
            scores_btn: Button::new(row[0], "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY),
            fx: Particles::new(rng.next_u32() as u64),
            popups: Popups::new(),
            shake: Shake::new(),
            hitstop: HitStop::default(),
            flash: Flash::new(0.35),
            sfx: sound::Sfx::new(),
            shapes: Shapes::new(),
            touch: None,
            selected: None,
            nudge: None,
            banner: None,
            later: Vec::new(),
            demo_wait: 0.0,
            sleep: 0.0,
            meter_shown: 1.0,
            meter_flash: 0.0,
            theme,
            rng,
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.idle += dt;
        self.shapes.art.poll();
        for e in input::poll() {
            if self.ui_event(&e) {
                continue;
            }
            match self.rounds.input(&e) {
                // The tap that enters play (or retries) starts the round at once; it
                // doesn't pick up a poppet (the board under it is a new one), so the
                // swipe hint shows straight away.
                Input::Start => self.start(),
                Input::Play => match e.kind() {
                    Ev::TouchDown | Ev::MouseDown => self.press(e.x, e.y),
                    Ev::TouchMove | Ev::MouseMove => self.drag(e.x, e.y),
                    Ev::TouchUp | Ev::MouseUp | Ev::TouchCancel => self.touch = None,
                    _ => {}
                },
                Input::Taken | Input::Extra => {}
            }
        }
        let sdt = self.hitstop.step(dt);
        match self.rounds.stage() {
            rounds::Stage::Title => self.autopilot(sdt),
            rounds::Stage::Playing => {
                self.play_t += dt;
                if DEBUG_AUTOPLAY > 0.0 && self.sim.ready() && self.idle > DEBUG_AUTOPLAY
                    && let Some(m) = self.sim.hint
                {
                    self.swap(m.0, m.1);
                }
            }
            rounds::Stage::Ending | rounds::Stage::Over => {}
        }
        self.sim.step(sdt);
        let cues = std::mem::take(&mut self.sim.cues);
        self.react(&cues);
        if self.rounds.stage() == rounds::Stage::Playing && self.sim.over {
            self.game_over();
        }

        // What the faces and the sky show.
        let target = match self.rounds.stage() {
            rounds::Stage::Title => {
                let k = (self.demo_wait / DEMO_WAIT).clamp(0.0, 1.0);
                if self.sim.ready() { 0.85 * k * k } else { 0.0 }
            }
            rounds::Stage::Playing | rounds::Stage::Ending => match self.sim.phase {
                Phase::Asleep { t } => (t / 0.8).min(1.0),
                _ => self.sim.drowsiness(),
            },
            rounds::Stage::Over => 1.0,
        };
        let rate = if target < self.sleep { 6.0 } else { 2.5 };
        self.sleep += (target - self.sleep) * (1.0 - (-dt * rate).exp());
        if self.sim.energy > self.meter_shown {
            self.meter_shown += (self.sim.energy - self.meter_shown) * (1.0 - (-dt * 5.0).exp());
        } else {
            self.meter_shown = self.sim.energy;
        }
        self.meter_flash = (self.meter_flash - dt * 3.0).max(0.0);
        // The drone and the music box winding down follow how sleepy they are.
        self.sfx.tension(self.sim.drowsiness() * 1.4);
        let drowsy = self.rounds.playing() && self.sim.energy < sim::DROWSY && !matches!(self.sim.phase, Phase::Asleep { .. });
        self.sfx.update(dt, drowsy);

        let mut i = 0;
        while i < self.later.len() {
            self.later[i].t -= dt;
            if self.later[i].t <= 0.0 {
                let f = self.later.swap_remove(i);
                self.fx.burst(f.x, f.y, if f.big { 10 } else { 4 }, f.color);
                if f.big {
                    self.fx.sparkles(f.x, f.y, 24.0, 3, 0xfff2b0ff);
                }
            } else {
                i += 1;
            }
        }
        if let Some((_, t)) = &mut self.nudge {
            *t += dt;
            if *t > 0.3 {
                self.nudge = None;
            }
        }
        if let Some(b) = &mut self.banner {
            b.t += dt;
            if b.t > 1.8 {
                self.banner = None;
            }
        }
        // The card goes up the frame the board is asleep; its NEW BEST gets a fanfare.
        if self.rounds.update(dt) == Some(ResultsEvent::NewBest) {
            self.sfx.best();
        }
        self.hud.update(dt);
        self.daily_btn.update(dt);
        self.scores_btn.update(dt);
        self.fx.update(dt);
        self.popups.update(dt);
        self.shake.update(dt);
        self.flash.update(dt);
    }

    fn render(&self) {
        let l = &self.rounds.layout;
        maimbrain::gfx2d::push();
        self.shake.apply(l.screen.cx(), l.screen.cy());
        draw::sky(&self.shapes.art, self.sleep, self.time);
        draw::board_bg(&self.shapes.art, self.sleep);
        self.draw_board();
        self.fx.draw();
        maimbrain::gfx2d::pop();
        self.popups.draw();
        let asleep = matches!(self.sim.phase, Phase::Asleep { .. }) || self.rounds.stage() == rounds::Stage::Over;
        if asleep {
            draw::zzz(self.time, self.sleep);
        }
        if self.rounds.playing() {
            self.hud.draw(l);
            draw::meter(180.0, l.hud.y + 112.0, 220.0, self.sim.energy, self.meter_shown, self.meter_flash, self.time);
            ui::pill(&format!("LV {}", self.sim.level), l.hud.right() - 40.0, l.hud.y + 28.0, 26.0, ui::with_alpha(INK, 0.55), 0xffffffff, self.theme.body_font, None);
            if self.rounds.is_daily() {
                ui::pill("DAILY", l.hud.right() - 40.0, l.hud.y + 60.0, 22.0, ui::with_alpha(INK, 0.55), GOLD, self.theme.body_font, Some(Icon::STAR));
            }
            let hint = self.sim.ready() && self.touch.is_none() && self.selected.is_none() && ((!self.taught && self.play_t > 0.5) || self.idle > HINT_IDLE);
            if let (true, Some(m)) = (hint, self.sim.hint) {
                let strength = if self.taught { ((self.idle - HINT_IDLE) / 0.4).clamp(0.0, 1.0) } else { 1.0 };
                let a = cell_center(m.0.0 as f32, m.0.1 as f32);
                let b = cell_center(m.1.0 as f32, m.1.1 as f32);
                draw::swipe_hint(a, b, self.time, strength);
                if !self.taught {
                    let bob = (self.time * 4.0).sin() * 3.0;
                    text(HINT).size(20.0).color(0xffffffff).outline(0.12, INK).middle().draw(180.0, BOARD_Y - 26.0 + bob);
                }
            }
            self.draw_banner();
        }
        // The title card or the game-over card, then the kit's own buttons on them.
        self.rounds.draw();
        match self.rounds.stage() {
            rounds::Stage::Title => self.daily_btn.draw(&self.theme),
            rounds::Stage::Over => {
                self.scores_btn.draw(&self.theme);
                self.daily_btn.draw(&self.theme);
            }
            rounds::Stage::Playing | rounds::Stage::Ending => {}
        }
        self.flash.draw(l.screen);
    }
}

export_game!(Poppets);
