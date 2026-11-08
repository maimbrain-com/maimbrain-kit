//! On-screen hints that can't hide each other. A game says each frame which
//! hints apply now ([`Hints::want`]); `Hints` decides which show, by
//! [`Priority`]: a hint about a threat that's about to land ("TAP HER" as the
//! witch lunges) always beats a teaching hint, and a teaching hint beats an
//! ambient one ("TAP TAP TAP" on a rattling chair). A hint that loses its
//! slot waits and comes back when the slot frees, instead of vanishing.
//! Equal priorities don't flicker: the one showing keeps its slot, and they
//! take turns every [`Hints::TURN`] seconds while both are wanted.
//!
//! Each hint says what it asks for ([`Ask`]: tap a named thing, hold, let
//! go), so the bot can do exactly what the screen says
//! (`maimbrain::bot::Literal`) and catch a hint that would hurt the player.
//! Decide the hints in the sim (pure code), not in drawing, so the bot sees
//! the same ones.
//!
//! ```
//! use maimbrain::ui::{Hint, Hints};
//! let mut hints = Hints::new();
//! for frame in 0..120 {
//!     let lunging = frame >= 30 && frame < 90;
//!     hints.want(Hint::tap("TAP TAP TAP", "chair").ambient()); // always applies
//!     if lunging {
//!         hints.want(Hint::tap("TAP HER", "witch").threat());
//!     }
//!     hints.update(1.0 / 60.0);
//!     if frame == 60 {
//!         assert_eq!(hints.showing().next().unwrap().text, "TAP HER");
//!     }
//! }
//! assert_eq!(hints.showing().next().unwrap().text, "TAP TAP TAP", "back once the lunge is over");
//! // render: hints.draw(&theme, x, y)
//! ```

use super::color::with_alpha;
use super::theme::Theme;
use super::widgets::pill_in;

/// How much a hint matters. A higher one takes a lower one's slot at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// Something to do when nothing else is going on (a prop to poke).
    Ambient,
    /// Teaching a move (the default).
    Teach,
    /// What to do about a threat that's about to land: never hidden by
    /// anything but another threat.
    Threat,
}

/// What a hint asks the player to do, for the bot to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ask {
    /// Nothing to do with a finger ("WATCH THE WIND").
    Look,
    /// Tap the hint's target (again and again while it shows).
    Tap,
    /// Put a finger down (on the target, if it names one) and keep it there.
    Hold,
    /// Lift the held finger.
    LetGo,
}

/// One hint: its words (≤ 5, a verb), what it asks for, where, and how much
/// it matters.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub text: String,
    pub ask: Ask,
    /// What to touch, by a name the game gives the bot a position for.
    pub target: Option<&'static str>,
    pub priority: Priority,
    /// Seconds it stays wanted after the last frame that wanted it (for a
    /// one-off moment).
    pub linger: f32,
    /// Where its target is on the screen now, when the game knows (a node
    /// the hint follows): the pill draws by it and the bot touches it.
    pub at: Option<(f32, f32)>,
}

impl Hint {
    fn new(text: &str, ask: Ask) -> Hint {
        Hint { text: text.to_string(), ask, target: None, priority: Priority::Teach, linger: 0.0, at: None }
    }
    /// "TAP HER": tap `target`.
    pub fn tap(text: &str, target: &'static str) -> Hint {
        Hint::new(text, Ask::Tap).on(target)
    }
    /// "HOLD", "KEEP HOLDING": put a finger down and keep it there.
    pub fn hold(text: &str) -> Hint {
        Hint::new(text, Ask::Hold)
    }
    /// "LET GO".
    pub fn let_go(text: &str) -> Hint {
        Hint::new(text, Ask::LetGo)
    }
    /// Words with nothing to do ("WATCH THE WIND").
    pub fn look(text: &str) -> Hint {
        Hint::new(text, Ask::Look)
    }
    /// Where the move happens (a hold on the candle).
    pub fn on(mut self, target: &'static str) -> Hint {
        self.target = Some(target);
        self
    }
    /// About a threat that's about to land.
    pub fn threat(mut self) -> Hint {
        self.priority = Priority::Threat;
        self
    }
    /// Only when nothing else is showing.
    pub fn ambient(mut self) -> Hint {
        self.priority = Priority::Ambient;
        self
    }
    /// Stays wanted `secs` after the last `want` (a one-off moment).
    pub fn linger(mut self, secs: f32) -> Hint {
        self.linger = secs.max(0.0);
        self
    }
    /// Its target is at (x, y) on the screen now.
    pub fn at(mut self, x: f32, y: f32) -> Hint {
        self.at = Some((x, y));
        self
    }
    /// Reads "TAP", "HOLD", "KEEP HOLDING", "LET GO", "RELEASE" off the
    /// front of a hint's words (any case): what a player takes it to ask.
    /// Anything else asks nothing of a finger ([`Ask::Look`]).
    pub fn ask_in(text: &str) -> Ask {
        let t = text.trim().to_ascii_lowercase();
        let starts = |w: &str| t == w || t.starts_with(&format!("{w} ")) || t.starts_with(&format!("{w}!"));
        if starts("let go") || starts("release") || starts("lift") {
            Ask::LetGo
        } else if starts("hold") || starts("keep holding") || starts("press and hold") || starts("tap and hold") {
            Ask::Hold
        } else if starts("tap") || starts("press") || starts("click") || starts("touch") || starts("hit") {
            Ask::Tap
        } else {
            Ask::Look
        }
    }
}

#[derive(Clone, Debug)]
struct Wanted {
    hint: Hint,
    since: f32,
    last: f32,
    shown: bool,
}

#[derive(Clone, Debug)]
struct Slot {
    hint: Hint,
    /// Seconds up (since it took the slot).
    age: f32,
    alpha: f32,
    on: bool,
    row: usize,
}

/// The hints a game shows: see the [module docs](self).
#[derive(Clone, Debug)]
pub struct Hints {
    slots: usize,
    clock: f32,
    wanted: Vec<Wanted>,
    shown: Vec<Slot>,
    unseen: Vec<String>,
}

impl Default for Hints {
    fn default() -> Self {
        Hints::new()
    }
}

impl Hints {
    /// Seconds one hint keeps its slot before an equal one waiting gets a turn.
    pub const TURN: f32 = 2.5;
    /// Seconds a hint fades in or out.
    pub const FADE: f32 = 0.2;

    /// One hint on screen at a time (≤ 5 words at a time).
    pub fn new() -> Hints {
        Hints { slots: 1, clock: 0.0, wanted: Vec::new(), shown: Vec::new(), unseen: Vec::new() }
    }

    /// Up to `n` at once, stacked downward from where they're drawn, the
    /// most important on top.
    pub fn slots(mut self, n: usize) -> Hints {
        self.slots = n.max(1);
        self
    }

    /// This hint applies this frame (call before [`update`](Hints::update),
    /// every frame it applies). Hints are told apart by their text.
    pub fn want(&mut self, hint: Hint) {
        let now = self.clock;
        match self.wanted.iter_mut().find(|w| w.hint.text == hint.text) {
            Some(w) => {
                w.hint = hint;
                w.last = now;
            }
            None => self.wanted.push(Wanted { hint, since: now, last: now, shown: false }),
        }
    }

    /// Moves on by `dt` seconds: drops hints no longer wanted, gives the
    /// slots to the most important ones, fades.
    pub fn update(&mut self, dt: f32) {
        let now = self.clock;
        let mut gone = Vec::new();
        self.wanted.retain(|w| {
            let keep = w.last + w.hint.linger >= now - 1e-6;
            if !keep && !w.shown {
                gone.push(w.hint.text.clone());
            }
            keep
        });
        for t in gone {
            if !self.unseen.contains(&t) {
                self.unseen.push(t);
            }
        }
        // Who gets the slots: priority, then the one showing (until its turn
        // is up while an equal one waits), then the longest waiting.
        let rank = |w: &Wanted| match self.shown.iter().find(|s| s.on && s.hint.text == w.hint.text) {
            Some(s) if s.age < Self::TURN => 0,
            Some(_) => 2,
            None => 1,
        };
        let mut order: Vec<usize> = (0..self.wanted.len()).collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (&self.wanted[a], &self.wanted[b]);
            y.hint.priority.cmp(&x.hint.priority).then(rank(x).cmp(&rank(y))).then(x.since.total_cmp(&y.since))
        });
        order.truncate(self.slots);
        for s in &mut self.shown {
            s.on = false;
        }
        for (row, &i) in order.iter().enumerate() {
            let w = &mut self.wanted[i];
            w.shown = true;
            match self.shown.iter_mut().find(|s| s.hint.text == w.hint.text) {
                Some(s) => {
                    if s.alpha <= 0.0 {
                        s.age = 0.0;
                    }
                    s.hint = w.hint.clone();
                    s.on = true;
                    s.row = row;
                }
                None => self.shown.push(Slot { hint: w.hint.clone(), age: 0.0, alpha: 0.0, on: true, row }),
            }
        }
        let step = dt / Self::FADE;
        for s in &mut self.shown {
            if s.on {
                s.age += dt;
                s.alpha = (s.alpha + step).min(1.0);
            } else {
                s.alpha -= step;
            }
        }
        // A hint that lost its slot starts its next turn afresh.
        for s in &mut self.shown {
            if !s.on && s.alpha <= 0.0 {
                s.age = 0.0;
            }
        }
        self.shown.retain(|s| s.on || s.alpha > 0.0);
        self.shown.sort_by_key(|s| (!s.on, s.row));
        self.clock += dt;
    }

    /// The hints on screen now (not fading out), the most important first.
    pub fn showing(&self) -> impl Iterator<Item = &Hint> {
        self.shown.iter().filter(|s| s.on).map(|s| &s.hint)
    }

    /// The hints on screen now, each with how long it has been up (s): what
    /// a player has had time to read.
    pub fn visible(&self) -> Vec<(&Hint, f32)> {
        self.shown.iter().filter(|s| s.on).map(|s| (&s.hint, s.age)).collect()
    }

    /// Hints that were wanted and stopped being wanted without ever getting
    /// on screen: an instruction the player never saw. Report it from the
    /// bot (`.flag("a hint never showed", !hints.unseen().is_empty())`).
    pub fn unseen(&self) -> &[String] {
        &self.unseen
    }

    /// Each hint in its pill where `place` says (by the hint: over its
    /// target, say), kept inside `bounds`, with a pointer toward the target
    /// when it has one ([`Hint::at`]); hints `place` gives no place go at
    /// (cx, cy), stacked.
    pub fn draw_each(&self, theme: &Theme, cx: f32, cy: f32, bounds: super::layout::Rect, place: impl Fn(&Hint) -> Option<(f32, f32)>) {
        for s in &self.shown {
            let a = s.alpha.clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let pulse = 1.0 + 0.06 * (s.age * std::f32::consts::TAU * 1.2).sin().max(0.0);
            let bg = if s.hint.priority == Priority::Threat { theme.bad } else { theme.accent };
            let h = 34.0 * pulse;
            let (x, y) = place(&s.hint).unwrap_or((cx, cy + s.row as f32 * 44.0));
            let half = super::widgets::pill_width(&s.hint.text, h, theme.body_font, false) / 2.0 + 4.0;
            let x = if bounds.w > 2.0 * half { x.clamp(bounds.x + half, bounds.right() - half) } else { bounds.cx() };
            if let Some((tx, ty)) = s.hint.at
                && ty > y + h * 0.5
            {
                // A pointer from the pill's foot toward what it's about.
                let tip = (ty - 4.0).min(y + h * 0.5 + 12.0);
                let w = 9.0;
                // Its foot under the target, as near as the pill allows.
                let bx = tx.clamp(x - half + 4.0 + w * 1.5, x + half - 4.0 - w * 1.5);
                let dx = (tx - bx).clamp(-w, w) * 0.3;
                super::shape::poly(&[bx - w, y + h * 0.5 - 1.0, bx + w, y + h * 0.5 - 1.0, bx + dx, tip], with_alpha(bg, a));
            }
            pill_in(theme, &s.hint.text, x, y, h, with_alpha(bg, a), with_alpha(theme.on_accent, a), None);
        }
    }

    /// The hints in pills, the first centred at (cx, cy) and any others
    /// below it; a threat's in the theme's `bad` colour, the rest in its
    /// accent. Draw them near where the move happens, clear of the HUD.
    pub fn draw(&self, theme: &Theme, cx: f32, cy: f32) {
        for s in &self.shown {
            let a = s.alpha.clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let pulse = 1.0 + 0.06 * (s.age * std::f32::consts::TAU * 1.2).sin().max(0.0);
            let bg = if s.hint.priority == Priority::Threat { theme.bad } else { theme.accent };
            pill_in(theme, &s.hint.text, cx, cy + s.row as f32 * 44.0, 34.0 * pulse, with_alpha(bg, a), with_alpha(theme.on_accent, a), None);
        }
    }
}

/// What the hints on screen ask of a player who does exactly what they
/// say ([`Follow::step`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Want {
    /// Tap the hint's target (`at`, or by its `target` name; neither: where
    /// the player's thumb rests). `first`: the hint's first tap (a new
    /// situation); the next ones are a rhythm already going.
    Tap { at: Option<(f32, f32)>, target: Option<&'static str>, text: String, first: bool },
    /// Put a finger down there and keep it down.
    Hold { at: Option<(f32, f32)>, target: Option<&'static str>, text: String },
    /// Lift the held finger.
    LetGo { text: String },
}

/// The rules a literal player follows, for any player to apply: a hint is
/// acted on once it has been up for the player's reaction time to it
/// (drawn once per hint), a TAP every [`TAP_EVERY`](Follow::TAP_EVERY)
/// seconds while it shows, a HOLD once until a LET GO, a LET GO when
/// something is held, and nothing else. The kit's `bot::Literal` turns
/// these into touches with a person's finger; Studio's difficulty bot
/// hands them to its own (docs/STUDIO-2.md §8.1). Pure: it runs anywhere.
#[derive(Clone, Debug, Default)]
pub struct Follow {
    /// The hints up now that it has seen: (text, its reaction time, the
    /// next time it may tap for it).
    seen: Vec<(String, f32, f64)>,
    held: bool,
}

impl Follow {
    /// Seconds between taps while a TAP hint stays up.
    pub const TAP_EVERY: f64 = 0.3;

    pub fn new() -> Follow {
        Follow::default()
    }

    /// At game time `now`: what the hints up now ask, in their order on
    /// screen. `reaction` draws the reaction time to a hint it hasn't seen
    /// (0 for a player that adds its own). A tap returned is taken as done
    /// at `now`; [`tapped`](Follow::tapped) moves when the next may come.
    pub fn step(&mut self, now: f64, hints: &Hints, mut reaction: impl FnMut() -> f32) -> Vec<Want> {
        let visible = hints.visible();
        self.seen.retain(|s| visible.iter().any(|(h, _)| h.text == s.0));
        let mut out = Vec::new();
        for (hint, age) in visible {
            let i = match self.seen.iter().position(|s| s.0 == hint.text) {
                Some(i) => i,
                None => {
                    let r = reaction();
                    self.seen.push((hint.text.clone(), r, f64::NEG_INFINITY));
                    self.seen.len() - 1
                }
            };
            if age < self.seen[i].1 {
                continue;
            }
            match hint.ask {
                Ask::Tap if now >= self.seen[i].2 => {
                    let first = self.seen[i].2 == f64::NEG_INFINITY;
                    self.seen[i].2 = now + Self::TAP_EVERY;
                    out.push(Want::Tap { at: hint.at, target: hint.target, text: hint.text.clone(), first });
                }
                Ask::Hold if !self.held => {
                    self.held = true;
                    out.push(Want::Hold { at: hint.at, target: hint.target, text: hint.text.clone() });
                }
                Ask::LetGo if self.held => {
                    self.held = false;
                    out.push(Want::LetGo { text: hint.text.clone() });
                }
                _ => {}
            }
        }
        out
    }

    /// The tap for `text` landed at `down`: the next is [`TAP_EVERY`](Follow::TAP_EVERY) after it.
    pub fn tapped(&mut self, text: &str, down: f64) {
        if let Some(s) = self.seen.iter_mut().find(|s| s.0 == text) {
            s.2 = down + Self::TAP_EVERY;
        }
    }

    /// A finger is held down (a HOLD was asked and no LET GO since).
    pub fn holding(&self) -> bool {
        self.held
    }

    /// The finger came up some other way (the round ended).
    pub fn released(&mut self) {
        self.held = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn top(h: &Hints) -> Option<&str> {
        h.showing().next().map(|h| h.text.as_str())
    }

    #[test]
    fn a_threat_keeps_its_slot_and_the_ambient_one_waits() {
        let mut h = Hints::new();
        let (mut during, mut chair_back) = (true, false);
        for frame in 0..300 {
            // The chair's hint has applied all along, and came first.
            h.want(Hint::tap("TAP TAP TAP", "chair").ambient());
            let lunge = (60..180).contains(&frame);
            if lunge {
                h.want(Hint::tap("TAP HER", "witch").threat());
            }
            h.update(DT);
            if lunge {
                during &= top(&h) == Some("TAP HER") && h.showing().count() == 1;
            }
            if frame == 200 {
                chair_back = top(&h) == Some("TAP TAP TAP");
            }
        }
        assert!(during, "the lunge's hint is up for the whole lunge");
        assert!(chair_back, "the chair's comes back after, not lost");
        assert!(h.unseen().is_empty());
    }

    #[test]
    fn a_hint_outranked_for_its_whole_life_is_reported_unseen() {
        let mut h = Hints::new();
        for frame in 0..120 {
            // The bug: the lunge's hint at the chair's level, the chair first.
            h.want(Hint::tap("TAP TAP TAP", "chair"));
            if (30..60).contains(&frame) {
                h.want(Hint::tap("TAP HER", "witch"));
            }
            h.update(DT);
            assert_eq!(top(&h), Some("TAP TAP TAP"));
        }
        assert_eq!(h.unseen(), ["TAP HER"]);
    }

    #[test]
    fn equal_hints_take_turns_and_two_slots_show_both() {
        let mut h = Hints::new();
        let mut seen = Vec::new();
        for _ in 0..(8.0 / DT) as usize {
            h.want(Hint::look("LEFT"));
            h.want(Hint::look("RIGHT"));
            h.update(DT);
            let t = top(&h).unwrap().to_string();
            if seen.last() != Some(&t) {
                seen.push(t);
            }
        }
        assert_eq!(seen, ["LEFT", "RIGHT", "LEFT", "RIGHT"], "turns every {} s, no flicker", Hints::TURN);
        let mut h = Hints::new().slots(2);
        h.want(Hint::look("LEFT"));
        h.want(Hint::hold("HOLD").threat());
        h.update(DT);
        assert_eq!(h.showing().map(|h| h.text.as_str()).collect::<Vec<_>>(), ["HOLD", "LEFT"]);
    }

    #[test]
    fn a_follower_reacts_then_taps_in_a_rhythm_holds_once_and_lets_go() {
        let mut h = Hints::new();
        let mut f = Follow::new();
        let mut wants = Vec::new();
        let mut t = 0.0;
        for frame in 0..240 {
            if frame < 60 {
                h.want(Hint::tap("TAP THE BELL", "bell").at(100.0, 200.0));
            } else if frame < 150 {
                h.want(Hint::hold("HOLD"));
            } else {
                h.want(Hint::let_go("LET GO"));
            }
            h.update(DT);
            for w in f.step(t, &h, || 0.4) {
                wants.push((frame, w));
            }
            t += DT as f64;
        }
        let taps: Vec<usize> = wants.iter().filter(|(_, w)| matches!(w, Want::Tap { .. })).map(|(f, _)| *f).collect();
        assert_eq!(taps.len(), 3, "after 0.4 s, then every 0.3 s while it shows: {taps:?}");
        assert!(matches!(&wants[0].1, Want::Tap { at: Some((100.0, 200.0)), first: true, .. }));
        assert!(matches!(&wants[1].1, Want::Tap { first: false, .. }));
        assert_eq!(wants.iter().filter(|(_, w)| matches!(w, Want::Hold { .. })).count(), 1);
        assert!(matches!(wants.last().unwrap().1, Want::LetGo { .. }));
        assert!(!f.holding());
    }

    #[test]
    fn the_ask_is_read_off_the_words() {
        assert_eq!(Hint::ask_in("TAP TO JUMP"), Ask::Tap);
        assert_eq!(Hint::ask_in("tap"), Ask::Tap);
        assert_eq!(Hint::ask_in("HOLD to charge"), Ask::Hold);
        assert_eq!(Hint::ask_in("KEEP HOLDING"), Ask::Hold);
        assert_eq!(Hint::ask_in("LET GO!"), Ask::LetGo);
        assert_eq!(Hint::ask_in("RELEASE"), Ask::LetGo);
        assert_eq!(Hint::ask_in("WATCH THE WIND"), Ask::Look);
        assert_eq!(Hint::ask_in("TAPESTRY"), Ask::Look);
    }

    #[test]
    fn a_lingering_hint_outlives_its_moment_and_fades() {
        let mut h = Hints::new();
        h.want(Hint::tap("TAP HER", "witch").threat().linger(1.0));
        h.update(DT);
        for _ in 0..50 {
            h.update(DT);
        }
        assert_eq!(top(&h), Some("TAP HER"), "still up 0.8 s later");
        assert!(h.visible()[0].1 > 0.8);
        for _ in 0..30 {
            h.update(DT);
        }
        assert_eq!(top(&h), None);
    }
}
