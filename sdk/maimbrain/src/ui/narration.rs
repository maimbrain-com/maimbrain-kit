//! Story lines told in the quiet stretches of play, never over the action
//! and never cut off by a level change.
//!
//! A line told at a fixed point in a level lands in the middle of a fight,
//! on top of a bark, or just before the stairs, where the next floor cuts
//! it off. [`Narration`] (`dialogue.narration`, played by its
//! [`super::Dialogue`]) holds story lines until the game is calm: no threat
//! (the game says so with [`Narration::set_busy`]), no cutscene, no bark or
//! other line being spoken, for [`Narration::settle`] seconds. Then the next
//! line is told, in the bark bubble, without holding the game.
//!
//! Its timing comes from each line's recorded length in the table `mb voice`
//! writes (`src/vo_lengths.rs`, compile-time constants;
//! [`Narration::lengths`]), so a game can clear a quiet stretch for the tale
//! ([`Narration::left`]: hold the spawner that long) and every replay times
//! it the same, whenever the audio happened to load.
//!
//! - **Never during a threat.** No line starts while the game is busy; a
//!   line already being told plays on (it's never cut).
//! - **Never cut by a transition.** Keep the same `Dialogue` across levels
//!   and keep calling its `update`: the line being told carries over the
//!   transition. Or wait for it: hold the stairs for
//!   [`Narration::remaining`] (or [`Narration::left`], for every line
//!   queued). [`Narration::clear_queue`] drops the old level's lines not yet
//!   told, never the one being told.
//! - **Reactions cut in, the story waits.** A [`super::Barks::react`] line
//!   stops a narration line being told; it's told again, from the top, in
//!   the next quiet stretch. `Dialogue::skip_all` does the same.
//!
//! ```
//! use maimbrain::ui::{Dialogue, Theme, VoiceLine};
//! // src/vo_lengths.rs, written by `mb voice`:
//! const LENGTHS: &[(&str, f32)] = &[("narrator/f1_end", 3.2)];
//! let mut talk = Dialogue::new(Theme::default());
//! talk.narration.lengths = LENGTHS;
//! // the floor's last line, on the stairs
//! talk.narration.tell("narrator", &VoiceLine::subtitle("And the candle burned low."), "");
//! let stairs = 2.0f32.max(talk.narration.left());   // hold the stairs until it's told
//! // update, every frame
//! talk.narration.set_busy(false);                    // true while a threat is on
//! talk.update(1.0 / 60.0);
//! # let _ = stairs;
//! ```

use std::collections::VecDeque;

use super::dialogue::VoiceLine;
use crate::sys;

/// A story line waiting to be told, or being told.
#[derive(Clone, Debug)]
pub struct Told {
    pub speaker: String,
    pub line: VoiceLine,
    pub expression: String,
    /// Which line this is (for the dialogue that tells it).
    pub(crate) id: u64,
}

/// Story lines told in quiet stretches (module docs). A
/// [`super::Dialogue`] owns one as `dialogue.narration` and tells what it
/// releases.
#[derive(Clone, Debug)]
pub struct Narration {
    /// Seconds of calm (no threat, nothing else speaking) before a line
    /// starts, and so the gap between two lines. Default 0.75.
    pub settle: f32,
    /// Each recorded line's length, by `"<character>/<line>"`: the table
    /// `mb voice` writes to `src/vo_lengths.rs` (`vo_lengths::ALL`). A line
    /// not in it is timed from its loaded file (a warning says so, once).
    pub lengths: &'static [(&'static str, f32)],
    queue: VecDeque<Told>,
    /// The line being told: it, when it started, how long it lasts.
    current: Option<(Told, f64, f32)>,
    busy: bool,
    calm_since: Option<f64>,
    /// The line being told when [`Narration::clear`] was called, for the dialogue to stop.
    cleared: Option<u64>,
    next_id: u64,
    now: f64,
    warned: Vec<String>,
}

impl Default for Narration {
    fn default() -> Self {
        Narration::new()
    }
}

impl Narration {
    pub fn new() -> Narration {
        Narration { settle: 0.75, lengths: &[], queue: VecDeque::new(), current: None, busy: false, calm_since: None, cleared: None, next_id: 1, now: 0.0, warned: Vec::new() }
    }

    /// Queues a story line: `speaker` says `line` wearing `expression`, in
    /// the next quiet stretch, after the lines queued before it.
    pub fn tell(&mut self, speaker: &str, line: &VoiceLine, expression: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.queue.push_back(Told { speaker: speaker.to_string(), line: line.clone(), expression: expression.to_string(), id });
    }

    /// Whether a threat is on (an enemy close, a jump to make, a boss): no
    /// line starts while it is. Set it every frame, or whenever it changes.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// Seconds `line` lasts: its length in [`Narration::lengths`] (a
    /// constant, the same in every replay), else its loaded file's (2 s
    /// until it loads), else, for a [`VoiceLine::subtitle`], its text's.
    pub fn length_of(&mut self, line: &VoiceLine) -> f32 {
        let path = line.path();
        let key = path.strip_prefix("assets/vo/").unwrap_or(&path);
        if let Some((_, secs)) = self.lengths.iter().find(|(k, _)| *k == key) {
            return *secs;
        }
        if !path.is_empty() && !self.warned.contains(&path) {
            sys::log(
                sys::Level::Warn,
                &format!("Narration: {path} isn't in the length table (narration.lengths = vo_lengths::ALL; `mb voice lengths` writes src/vo_lengths.rs), so it's timed from its file, which replays may load at another moment"),
            );
            self.warned.push(path.clone());
        }
        line.dur()
    }

    /// A line is being told now.
    pub fn telling(&self) -> bool {
        self.remaining() > 0.0
    }

    /// Seconds left of the line being told (0 when none): hold a level
    /// transition at least this long so it isn't cut off.
    pub fn remaining(&self) -> f32 {
        self.current.as_ref().map_or(0.0, |(_, start, len)| (start + *len as f64 - self.now.max(sys::time())).max(0.0) as f32)
    }

    /// Lines waiting to be told.
    pub fn waiting(&self) -> usize {
        self.queue.len()
    }

    /// How long the next waiting line lasts: the quiet stretch it needs.
    pub fn next_len(&mut self) -> Option<f32> {
        let line = self.queue.front()?.line.clone();
        Some(self.length_of(&line))
    }

    /// Seconds until everything queued has been told, if the game stays
    /// calm from now: the line being told, then each waiting line after
    /// [`Narration::settle`]. Clear a quiet stretch this long for the tale
    /// (spawn nothing), or hold the stairs this long.
    pub fn left(&mut self) -> f32 {
        let lines: Vec<VoiceLine> = self.queue.iter().map(|t| t.line.clone()).collect();
        let mut secs = self.remaining();
        for l in &lines {
            secs += self.settle + self.length_of(l);
        }
        secs
    }

    /// Forgets the lines not yet told (the level they belonged to is over).
    /// The line being told carries on.
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Forgets every line, and the dialogue stops the one being told.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.cleared = self.current.take().map(|c| c.0.id);
    }

    /// The line to tell at game time `now`, if one is waiting and the game
    /// has been calm (not busy, and `free`: nothing else being spoken, no
    /// cutscene) for [`Narration::settle`] seconds. [`super::Dialogue::update`]
    /// calls it.
    pub(crate) fn take(&mut self, now: f64, free: bool) -> Option<Told> {
        self.now = now;
        if self.current.as_ref().is_some_and(|(_, start, len)| now >= start + *len as f64) {
            self.current = None;
        }
        if !free || self.busy || self.current.is_some() {
            self.calm_since = None;
            return None;
        }
        let since = *self.calm_since.get_or_insert(now);
        if now - since < self.settle as f64 {
            return None;
        }
        let told = self.queue.pop_front()?;
        let len = self.length_of(&told.line);
        self.current = Some((told.clone(), now, len));
        self.calm_since = None;
        Some(told)
    }

    /// The line [`Narration::clear`] took away, once.
    pub(crate) fn take_cleared(&mut self) -> Option<u64> {
        self.cleared.take()
    }

    /// The line being told was stopped before its end (a cut-in, a
    /// `skip_all`): it goes back to the front of the queue, to be told
    /// again from the top in the next quiet stretch.
    pub(crate) fn interrupted(&mut self, id: u64) {
        if let Some((t, _, _)) = self.current.take_if(|(t, _, _)| t.id == id) {
            self.queue.push_front(t);
        }
        self.calm_since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::dialogue::{Dialogue, Lips};
    use super::super::theme::Theme;
    use super::*;
    use crate::audio::Sound;

    fn voiced(text: &str, dur: f32) -> VoiceLine {
        VoiceLine::new(Some(Sound(1)), Lips::parse(format!("mblips 1\ndur {dur}\ntext {text}\n").as_bytes()).unwrap())
    }

    fn frame(d: &mut Dialogue) {
        sys::test::advance(1.0 / 60.0);
        d.update(1.0 / 60.0);
    }

    fn run(d: &mut Dialogue, seconds: f32) {
        for _ in 0..(seconds * 60.0).round() as usize {
            frame(d);
        }
    }

    fn bubble(d: &Dialogue) -> Option<String> {
        d.current_bark().map(|s| s.text)
    }

    #[test]
    fn a_reaction_cuts_in_at_once_and_the_story_is_told_again() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.barks.add("bell", "Mother", &voiced("Stop that ringing!", 1.0), "angry");
        d.barks.add("bell", "Mother", &voiced("Enough with the bell!", 1.0), "angry");
        d.barks.add("creak", "Mother", &voiced("What was that?", 1.0), "");
        d.narration.tell("Narrator", &voiced("The candle burned low that night.", 3.0), "");
        run(&mut d, 1.0);
        assert!(d.narration.telling() && bubble(&d).as_deref() == Some("The candle burned low that night."));
        // An ordinary bark waits behind the story (and goes stale).
        d.barks.trigger("creak");
        run(&mut d, 0.5);
        assert_eq!(bubble(&d).as_deref(), Some("The candle burned low that night."));
        // The player rings the bell: the reaction is up in the same frame.
        d.barks.react("bell");
        frame(&mut d);
        let first = bubble(&d).unwrap();
        assert!(first.contains("ring") || first.contains("bell"), "{first}");
        assert!(!d.narration.telling() && d.narration.waiting() == 1, "the story line went back to be told again");
        // Mashing the bell: no cut-in over a cut-in, none within the gap.
        run(&mut d, 0.3);
        d.barks.react("bell");
        run(&mut d, 1.2);
        assert_eq!(d.barks.pending(), 0, "the second reaction found the first still speaking and went stale");
        assert!(bubble(&d).is_none_or(|t| t == first), "{:?}", bubble(&d));
        // Then quiet: the story line again, from the top.
        run(&mut d, 1.0);
        let s = d.current_bark().unwrap();
        assert_eq!(s.text, "The candle burned low that night.");
        assert!(d.narration.telling() && s.revealed < 10, "from the top: {}", s.revealed);
        // A cut-in never cuts a cutscene line.
        run(&mut d, 4.0);
        d.say("Mother", &voiced("Come here, child.", 2.0), "");
        run(&mut d, 0.3);
        d.barks.react("bell");
        run(&mut d, 0.2);
        assert!(d.current().is_some() && bubble(&d).is_none_or(|t| !t.contains("ring") && !t.contains("bell")));
        // One line by hand, too (the bell still waiting would cut in once the cutscene is gone).
        d.barks.clear();
        d.skip_all();
        run(&mut d, 0.1);
        assert!(d.cut_in("Mother", &voiced("Hush!", 0.5), ""));
        assert_eq!(bubble(&d).as_deref(), Some("Hush!"));
        assert!(!d.cut_in("Mother", &voiced("Hush, I said!", 0.5), ""), "still speaking");
    }

    #[test]
    fn the_story_waits_for_calm_and_a_transition_never_cuts_it() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.narration.tell("Narrator", &voiced("Floor one was quiet.", 2.0), "");
        d.narration.set_busy(true);
        run(&mut d, 5.0);
        assert!(!d.narration.telling() && bubble(&d).is_none(), "never during a threat");
        d.narration.set_busy(false);
        run(&mut d, 0.5);
        assert!(!d.narration.telling(), "it lets the calm settle first");
        run(&mut d, 0.4);
        assert!(d.narration.telling());
        // A threat now doesn't cut the line being told.
        d.narration.set_busy(true);
        run(&mut d, 1.0);
        assert_eq!(bubble(&d).as_deref(), Some("Floor one was quiet."));
        d.narration.set_busy(false);
        run(&mut d, 1.5);
        assert!(!d.narration.telling());
        // The floor's last line, then the stairs: they last as long as it does.
        d.narration.tell("Narrator", &voiced("She climbed.", 2.5), "");
        d.narration.tell("Narrator", &voiced("And climbed.", 1.5), "");
        let left = d.narration.left();
        assert!((left - (0.75 + 2.5 + 0.75 + 1.5)).abs() < 1e-3, "{left}");
        run(&mut d, 1.0);
        assert!(d.narration.telling());
        let held = d.narration.remaining();
        assert!(held > 1.5 && held < 2.5, "{held}");
        // The level changes: lines not yet told are dropped, the one being told carries over.
        d.narration.clear_queue();
        let mut heard = 0.0;
        for _ in 0..(3.0 * 60.0) as usize {
            frame(&mut d);
            if bubble(&d).as_deref() == Some("She climbed.") && d.current_bark().is_some_and(|s| !s.complete) {
                heard += 1.0 / 60.0;
            }
        }
        assert!(heard >= held - 0.05, "told to its end over the transition: {heard} of {held}");
        assert_eq!(d.narration.waiting(), 0);
        // skip_all doesn't lose it either: told again later.
        d.narration.tell("Narrator", &voiced("Floor two.", 2.0), "");
        run(&mut d, 1.0);
        d.skip_all();
        assert_eq!(d.narration.waiting(), 1);
        // clear() forgets the story and stops the line.
        run(&mut d, 1.0);
        assert!(d.narration.telling());
        d.narration.clear();
        frame(&mut d);
        assert!(bubble(&d).is_none() && d.narration.waiting() == 0);
    }

    #[test]
    fn lengths_come_from_the_table() {
        sys::test::reset();
        let mut n = Narration::new();
        const TABLE: &[(&str, f32)] = &[("narrator/f1_end", 3.25)];
        n.lengths = TABLE;
        let line = VoiceLine::load("assets/vo/narrator/f1_end");
        assert_eq!(line.path(), "assets/vo/narrator/f1_end");
        assert_eq!(n.length_of(&line), 3.25, "the constant, loaded or not");
        n.tell("Narrator", &line, "");
        assert_eq!(n.next_len(), Some(3.25));
        // Not in the table: its file's length (a subtitle's from its text).
        assert!((n.length_of(&VoiceLine::subtitle("Hi.")) - Lips::from_text("Hi.").dur()).abs() < 1e-6);
    }
}
