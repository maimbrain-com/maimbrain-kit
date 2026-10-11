//! The round every game plays, kept the platform's way (SPEC §5.1–§5.2,
//! SKILL.md "Platform facts that bite"): the title card until a press, which
//! starts a round and is also its first move; the round; its ending playing
//! out while still `Playing`; then the score submitted, the best saved, the
//! game-over card shown and `Over` reported, in that order; and after a
//! guard against a frantic tap, any tap (or the card's retry button) plays
//! again. [`Rounds`] does all of it, so a game only says when its round ends
//! and with what score.
//!
//! ```ignore
//! // init
//! let theme = Theme::from_identity(IDENTITY).load_fonts();
//! let rounds = Rounds::new(TitleCard::new("BUBBLE", theme).tagline("don't let it pop")).heading("POP!").label("SCORE");
//! // update: input first
//! for e in input::poll() {
//!     match self.rounds.input(&e) {
//!         Input::Start => {
//!             self.sim = Sim::new(self.rounds.seed());   // a fresh round…
//!             self.act(&e);                              // …and the press is its first move
//!         }
//!         Input::Play => self.act(&e),
//!         Input::Taken => {}
//!     }
//! }
//! if self.rounds.playing() { self.sim.step(dt) }
//! if self.sim.over { self.rounds.end(self.sim.score as i64); }   // once is enough; more are ignored
//! self.rounds.update(dt);
//! // render: your scene, your HUD while `playing()`, then
//! self.rounds.draw();
//! ```

use crate::input::Event;
use crate::sys::{self, Round};
use crate::ui::{Button, ButtonKind, Icon, Layout, Rect, ResultsAction, ResultsCard, ResultsEvent, TitleCard};
use crate::{Rng, store};

/// Where a round is (named so it doesn't clash with a sim's own `Phase`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The title card: waiting for a press (reported `Idle`).
    Title,
    /// A round is being played (`Playing`).
    Playing,
    /// The round is lost (or won), and its ending plays out: still `Playing`,
    /// so the feed's countdown back to browse doesn't start mid-animation.
    Ending,
    /// The game-over card is up (`Over`).
    Over,
}

/// What an input event means for the game, from [`Rounds::input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A round starts: set up a new one now (with [`Rounds::seed`]). The
    /// event is also the round's first move (SPEC §5.2: the tap that enters
    /// play reaches the game), so handle it as you would in `Play`. (It's a
    /// release when the game-over card's retry button started it.)
    Start,
    /// Input for the round. Moves and releases always come back as `Play`,
    /// in every phase, so a finger held when a round ends is let go.
    Play,
    /// The round flow's own (a press during the ending or the card's guard,
    /// the card's buttons): ignore it.
    Taken,
    /// The game-over card's extra button (one you gave it with
    /// [`Rounds::extra`]).
    Extra,
}

/// The round flow: title card, round, ending, game-over card, retry. See
/// the [module docs](self).
///
/// It saves the best under `"best"` (and the daily best under
/// `"daily:<today's seed>"`) with `store::set_u64`, as games always have, so
/// an update keeps a player's best.
#[derive(Clone, Debug)]
pub struct Rounds {
    /// The title card (drawn in [`Stage::Title`]). Change it freely.
    pub title: TitleCard,
    /// The game-over card, once a round has ended (drawn in [`Stage::Over`]).
    pub results: Option<ResultsCard>,
    pub layout: Layout,
    phase: Stage,
    /// Seconds in the current phase.
    age: f32,
    board: Option<u32>,
    daily_board: Option<u32>,
    daily: bool,
    best: i64,
    lower_is_better: bool,
    ending: f32,
    card: Card,
    /// This round's card, set by `end`.
    this: Card,
    score: i64,
    seed: u64,
    rng: Rng,
    played: u32,
    /// The game-over card went up in the last `update`.
    shown: bool,
}

/// How the game-over card looks.
#[derive(Clone, Debug, Default)]
struct Card {
    heading: Option<String>,
    label: Option<String>,
    floating: bool,
    state_only: bool,
    scores: bool,
    extra: Option<Button>,
}

impl Rounds {
    /// The ending's length unless [`ending`](Rounds::ending) says otherwise (s).
    pub const ENDING: f32 = 0.7;

    /// A round flow with this title card (its theme styles the game-over
    /// card too), on board 0. Reports `Idle` and reads the saved best into
    /// the title card. Call it in `init`.
    pub fn new(title: TitleCard) -> Rounds {
        sys::round(Round::Idle);
        let mut r = Rounds {
            title,
            results: None,
            layout: Layout::new(),
            phase: Stage::Title,
            age: 0.0,
            board: Some(0),
            daily_board: None,
            daily: false,
            best: 0,
            lower_is_better: false,
            ending: Rounds::ENDING,
            card: Card::default(),
            this: Card::default(),
            score: 0,
            seed: 0,
            rng: Rng::new(sys::rand_seed() ^ 0x726f_756e_6473),
            played: 0,
            shown: false,
        };
        r.load_best();
        r
    }

    /// The leaderboard rounds are submitted to (a `[[scores]]` board in the
    /// manifest; default 0).
    pub fn board(mut self, board: u32) -> Self {
        self.board = Some(board);
        self
    }

    /// No leaderboard: no score is submitted (the best is still saved).
    pub fn no_board(mut self) -> Self {
        self.board = None;
        self
    }

    /// A daily mode on this board: with [`set_daily`](Rounds::set_daily) on,
    /// every round plays today's seed (the same course for everyone), is
    /// submitted to this board and keeps its own best.
    pub fn daily(mut self, board: u32) -> Self {
        self.daily_board = Some(board);
        self
    }

    /// Lower scores are better (a time). The card says so too.
    pub fn lower_is_better(mut self) -> Self {
        self.lower_is_better = true;
        self.load_best();
        self
    }

    /// How long the ending plays (s; default [`ENDING`](Rounds::ENDING)).
    /// 0 when the game plays its own ending before calling `end` (a sim
    /// that tumbles, then reports over).
    pub fn ending(mut self, seconds: f32) -> Self {
        self.ending = seconds.max(0.0);
        self
    }

    /// The game-over card's heading, in the game's voice ("SPLAT!" is the
    /// one everyone copies). Default "GAME OVER".
    pub fn heading(mut self, s: &str) -> Self {
        self.card.heading = Some(s.into());
        self
    }

    /// What the score counts ("HEIGHT", "POINTS"); default none.
    pub fn label(mut self, s: &str) -> Self {
        self.card.label = Some(s.into());
        self
    }

    /// The card's text floats over the scene, no panel.
    pub fn floating(mut self) -> Self {
        self.card.floating = true;
        self
    }

    /// You draw the game-over card yourself, from `results`'s state
    /// (`ResultsCard::state_only`, docs/UI.md "Your own card").
    pub fn state_only(mut self) -> Self {
        self.card.state_only = true;
        self
    }

    /// A SCORES button on the game-over card that opens the round's
    /// leaderboard (`store::show_scores`). The card has room for one extra
    /// button: this or [`extra`](Rounds::extra), not both (more go in a row
    /// of your own, behind [`card_ready`](Rounds::card_ready)).
    pub fn scores_button(mut self) -> Self {
        self.card.scores = true;
        self
    }

    /// Another button on the game-over card; a tap on it is [`Input::Extra`].
    pub fn extra(mut self, b: Button) -> Self {
        self.card.extra = Some(b);
        self
    }

    /// Where the round is.
    pub fn stage(&self) -> Stage {
        self.phase
    }

    /// A round is on: `Playing` or `Ending` (the platform has it as `Playing`).
    pub fn playing(&self) -> bool {
        matches!(self.phase, Stage::Playing | Stage::Ending)
    }

    /// Seconds into the ending, while it plays.
    pub fn ending_for(&self) -> Option<f32> {
        (self.phase == Stage::Ending).then_some(self.age)
    }

    /// Seconds in the current phase (on the title, in the round, …).
    pub fn age(&self) -> f32 {
        self.age
    }

    /// The seed for the round that just started: today's in daily mode,
    /// else a new one from the session's.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The best so far (the daily best in daily mode).
    pub fn best(&self) -> i64 {
        self.best
    }

    /// The last ended round's score.
    pub fn score(&self) -> i64 {
        self.score
    }

    /// Rounds started this session.
    pub fn played(&self) -> u32 {
        self.played
    }

    /// Daily mode is on.
    pub fn is_daily(&self) -> bool {
        self.daily
    }

    /// The game-over card went up in this frame's `update` (and `Over` was
    /// reported): the moment for a verdict sound, a log line, resetting your
    /// own buttons. [`new_best`](Rounds::new_best) says how it went.
    pub fn card_shown(&self) -> bool {
        self.shown
    }

    /// The round on the game-over card beat a previous best.
    pub fn new_best(&self) -> bool {
        self.results.as_ref().is_some_and(|c| c.is_new_best())
    }

    /// The game-over card takes taps now (past its guard against a frantic
    /// tap during the failure). Buttons of your own on the card (DAILY,
    /// SCORES in a row of your own) should wait for this, as its own do.
    pub fn card_ready(&self) -> bool {
        self.phase == Stage::Over && self.results.as_ref().is_some_and(|c| c.retry_ready())
    }

    /// Daily mode on or off (needs [`daily`](Rounds::daily)). Takes effect
    /// from the next round; on the game-over card it goes back to the title,
    /// and either way the title card plays its entrance again.
    pub fn set_daily(&mut self, on: bool) {
        let on = on && self.daily_board.is_some();
        if on == self.daily {
            return;
        }
        self.daily = on;
        self.load_best();
        match self.phase {
            Stage::Over => self.to_title(),
            // The title card plays its entrance again, for the other mode.
            Stage::Title => self.title.restart(),
            Stage::Playing | Stage::Ending => {}
        }
    }

    /// Back to the title card (`Idle`), from the game-over card.
    pub fn to_title(&mut self) {
        if self.phase != Stage::Over {
            return;
        }
        self.phase = Stage::Title;
        self.age = 0.0;
        self.results = None;
        self.title.restart();
        sys::round(Round::Idle);
    }

    /// Feeds one input event to the round flow; what it means for the game.
    /// Call it for every event in `update`, before anything else sees it.
    pub fn input(&mut self, e: &Event) -> Input {
        if self.phase == Stage::Over {
            // The card sees everything: its buttons click on release.
            return match self.results.as_mut().and_then(|c| c.input(e)) {
                Some(ResultsAction::Retry) => {
                    self.start();
                    Input::Start
                }
                Some(ResultsAction::Extra) if self.this.scores => {
                    if let Some(b) = self.board_now() {
                        store::show_scores(b);
                    }
                    Input::Taken
                }
                Some(ResultsAction::Extra) => Input::Extra,
                None if e.is_press() => Input::Taken,
                None => Input::Play,
            };
        }
        if !e.is_press() {
            return Input::Play;
        }
        match self.phase {
            Stage::Title => {
                self.start();
                Input::Start
            }
            Stage::Playing => Input::Play,
            Stage::Ending | Stage::Over => Input::Taken,
        }
    }

    /// The round is over, with this score: the ending plays (still
    /// `Playing`), then [`update`](Rounds::update) submits the score, saves
    /// the best, shows the game-over card and reports `Over`. Call it once,
    /// at the moment of failure (or the finish line); calls when no round is
    /// being played are ignored. Change this round's card with the returned
    /// value: `rounds.end(score).heading("PIT!")`.
    pub fn end(&mut self, score: i64) -> RoundEnd<'_> {
        if self.phase == Stage::Playing {
            self.phase = Stage::Ending;
            self.age = 0.0;
            self.score = score;
            self.this = self.card.clone();
        }
        RoundEnd(self)
    }

    /// Moves the flow on by `dt`: the cards' animations, and the ending into
    /// the game-over card. Call it every frame, after your sim (so a round
    /// that ended this frame goes on from here). Returns the game-over card's
    /// moments (a tick of the roll-up, a new best, landing) for sound and haptics.
    pub fn update(&mut self, dt: f32) -> Option<ResultsEvent> {
        self.shown = false;
        self.age += dt;
        self.title.update(dt);
        if self.phase == Stage::Ending && self.age >= self.ending {
            self.finish();
        }
        self.results.as_mut().and_then(|c| c.update(dt))
    }

    /// Draws the title card on the title and the game-over card after a
    /// round, over your scene and HUD. Nothing during a round.
    pub fn draw(&self) {
        if self.phase == Stage::Title {
            self.title.draw(&self.layout);
        }
        self.draw_card();
    }

    /// Only the game-over card, for a game that draws its own title (then
    /// the title card's drawing isn't linked into its wasm at all).
    pub fn draw_card(&self) {
        if self.phase == Stage::Over
            && let Some(c) = &self.results
        {
            c.draw();
        }
    }

    fn start(&mut self) {
        self.seed = if self.daily { sys::daily_seed() } else { self.rng.next_u32() as u64 | (self.rng.next_u32() as u64) << 32 };
        self.phase = Stage::Playing;
        self.age = 0.0;
        self.results = None;
        self.played += 1;
        sys::round(Round::Playing);
    }

    /// Submit, save, the card, then `Over`: the platform saves the replay on Over.
    fn finish(&mut self) {
        let score = self.score;
        if let Some(b) = self.board_now() {
            store::submit_score(b, score);
        }
        let prev = self.best;
        let better = if self.lower_is_better { score > 0 && (prev <= 0 || score < prev) } else { score > prev };
        if better {
            self.best = score;
            store::set_u64(&self.best_key(), score.max(0) as u64);
        }
        self.title.set_best(self.best);
        let c = &self.this;
        let mut card = ResultsCard::new(score, prev, self.title.theme, &self.layout, self.rng.next_u32() as u64);
        if let Some(h) = &c.heading {
            card = card.heading(h);
        }
        if let Some(l) = &c.label {
            card = card.label(l);
        }
        if c.floating {
            card = card.floating();
        }
        if self.lower_is_better {
            card = card.lower_is_better();
        }
        if c.state_only {
            card = card.state_only();
        }
        // The card has room for one extra button: SCORES, or yours.
        if c.scores {
            card = card.extra(Button::new(Rect::new(0.0, 0.0, 150.0, 44.0), "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY));
        } else if let Some(b) = &c.extra {
            card = card.extra(b.clone());
        }
        self.results = Some(card);
        self.phase = Stage::Over;
        self.age = 0.0;
        self.shown = true;
        sys::round(Round::Over);
    }

    fn board_now(&self) -> Option<u32> {
        if self.daily { self.daily_board } else { self.board }
    }

    fn best_key(&self) -> String {
        if self.daily { format!("daily:{:016x}", sys::daily_seed()) } else { "best".into() }
    }

    fn load_best(&mut self) {
        self.best = store::get_u64(&self.best_key()).unwrap_or(0) as i64;
        self.title.set_best(self.best);
    }
}

/// From [`Rounds::end`]: this round's game-over card, if it should differ
/// from the usual (a heading that names how it was lost).
pub struct RoundEnd<'a>(&'a mut Rounds);

impl RoundEnd<'_> {
    /// This round's score beats the best so far (what the card will say),
    /// known now, before the ending plays: pick the verdict sound with it.
    pub fn is_new_best(&self) -> bool {
        let (r, s) = (&*self.0, self.0.score);
        let beats = if r.lower_is_better { s > 0 && (r.best <= 0 || s < r.best) } else { s > r.best };
        r.best > 0 && beats
    }
    pub fn heading(self, s: &str) -> Self {
        if self.0.phase == Stage::Ending {
            self.0.this.heading = Some(s.into());
        }
        self
    }
    pub fn label(self, s: &str) -> Self {
        if self.0.phase == Stage::Ending {
            self.0.this.label = Some(s.into());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Kind, synth};
    use crate::sys::test::{Conformance, Host};
    use crate::ui::Theme;
    use crate::{Game, input};

    const DT: f32 = 1.0 / 60.0;

    /// A round of "tap 5 times before 3 s run out", on Rounds.
    struct Five {
        rounds: Rounds,
        taps: u32,
        clock: f32,
        starts: u32,
        released: u32,
    }

    impl Game for Five {
        fn init() -> Self {
            let rounds = Rounds::new(TitleCard::new("FIVE", Theme::default())).heading("TIME!").scores_button();
            Five { rounds, taps: 0, clock: 0.0, starts: 0, released: 0 }
        }
        fn update(&mut self, dt: f32) {
            for e in input::poll() {
                match self.rounds.input(&e) {
                    Input::Start => {
                        self.taps = 0;
                        self.clock = 0.0;
                        self.starts += 1;
                        self.taps += 1;
                    }
                    Input::Play if e.is_press() => self.taps += 1,
                    Input::Play if e.is_release() => self.released += 1,
                    _ => {}
                }
            }
            if self.rounds.stage() == Stage::Playing {
                self.clock += dt;
                if self.clock > 3.0 {
                    self.rounds.end(self.taps as i64);
                }
            }
            self.rounds.update(dt);
        }
        fn render(&self) {
            self.rounds.draw();
        }
    }

    #[test]
    fn a_game_on_rounds_keeps_the_platform_rules() {
        Conformance::new().check::<Five>();
    }

    #[test]
    fn the_ending_plays_before_the_score_the_card_and_over() {
        let host = Host::start(3);
        let mut g: Five = host.init();
        host.push(synth::tap(180.0, 320.0, 0.1));
        host.push(synth::tap(100.0, 100.0, 0.5));
        host.run(&mut g, 3.2, DT);
        assert_eq!(g.rounds.stage(), Stage::Ending, "lost, still playing");
        assert_eq!(host.rounds(), [Round::Idle, Round::Playing]);
        assert!(g.rounds.playing());
        host.run(&mut g, 0.7, DT);
        assert_eq!(g.rounds.stage(), Stage::Over);
        assert_eq!(host.scores(), [(0, 2)]);
        assert_eq!(host.saved("best"), Some(2u64.to_le_bytes().to_vec()));
        assert_eq!(g.rounds.results.as_ref().unwrap().heading, "TIME!");
        assert_eq!(g.released, 2, "both taps' releases reached the game");
        // A press during the ending was the flow's, not the game's.
        assert_eq!(g.starts, 1);
    }

    #[test]
    fn a_retry_starts_a_new_round_with_its_own_seed_and_the_new_best() {
        let host = Host::start(9);
        let mut g: Five = host.init();
        host.tap(180.0, 320.0);
        host.run(&mut g, 4.0, DT);
        let first = g.rounds.seed();
        host.run(&mut g, 1.0, DT);
        host.tap(180.0, 320.0);
        host.frame(&mut g, DT);
        assert_eq!((g.starts, g.rounds.played()), (2, 2));
        assert_ne!(g.rounds.seed(), first);
        assert_eq!(g.rounds.best(), 1);
    }

    #[test]
    fn daily_mode_plays_todays_seed_on_its_own_board_and_best() {
        let host = Host::start(4);
        host.set_daily_seed(0xd);
        let mut g: Five = host.init();
        g.rounds = g.rounds.clone().daily(1);
        g.rounds.set_daily(true);
        host.tap(180.0, 320.0);
        host.run(&mut g, 4.0, DT);
        assert_eq!(g.rounds.seed(), 0xd);
        assert_eq!(host.scores(), [(1, 1)]);
        assert!(host.saved("daily:000000000000000d").is_some() && host.saved("best").is_none());
        // Off again on the card: back to the title, with the other best.
        g.rounds.set_daily(false);
        assert_eq!(g.rounds.stage(), Stage::Title);
        assert_eq!(*host.rounds().last().unwrap(), Round::Idle);
        assert_eq!(g.rounds.best(), 0);
    }

    #[test]
    fn the_scores_button_opens_the_board_and_doesnt_retry() {
        let host = Host::start(2);
        let mut g: Five = host.init();
        host.tap(180.0, 320.0);
        host.run(&mut g, 5.0, DT);
        let b = g.rounds.results.as_ref().unwrap().extra.as_ref().unwrap().rect;
        let (x, y) = (b.cx(), b.cy());
        host.push([synth::touch(Kind::TouchDown, x, y, host.now() + 1e-4), synth::touch(Kind::TouchUp, x, y, host.now() + 0.05)]);
        host.run(&mut g, 0.2, DT);
        assert!(host.calls().iter().any(|c| matches!(c, crate::sys::test::Call::ShowScores { board: 0, .. })));
        assert_eq!(g.rounds.stage(), Stage::Over, "the button doesn't retry");
    }

    #[test]
    fn the_card_says_when_it_went_up_and_when_it_takes_taps() {
        let host = Host::start(5);
        host.save("best", &1u64.to_le_bytes());
        let mut g: Five = host.init();
        host.push(synth::tap(180.0, 320.0, 0.1));
        host.push(synth::tap(100.0, 100.0, 0.5));
        let mut shown = Vec::new();
        for _ in 0..300 {
            host.frame(&mut g, DT);
            if g.rounds.card_shown() {
                shown.push(host.frames());
            }
        }
        assert_eq!(shown.len(), 1, "one frame: the one Over was reported in");
        assert!(g.rounds.new_best(), "2 beats the saved 1");
        assert!(g.rounds.card_ready());
        let mut r = Rounds::new(TitleCard::new("T", Theme::default())).ending(0.0);
        r.input(&synth::touch(Kind::TouchDown, 1.0, 1.0, 0.0));
        assert!(r.end(5).is_new_best(), "known at end(), before the card");
        r.update(DT);
        assert!(r.card_shown() && !r.card_ready(), "up, but inside its guard");
    }

    #[test]
    fn lower_is_better_keeps_the_smaller_time() {
        let host = Host::start(1);
        host.save("best", &30u64.to_le_bytes());
        let mut r = Rounds::new(TitleCard::new("T", Theme::default())).lower_is_better().ending(0.0);
        assert_eq!(r.best(), 30);
        r.input(&synth::touch(Kind::TouchDown, 1.0, 1.0, 0.0));
        r.end(25);
        r.update(DT);
        assert_eq!((r.best(), r.stage()), (25, Stage::Over));
    }
}
