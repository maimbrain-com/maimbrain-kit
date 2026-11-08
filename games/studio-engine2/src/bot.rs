//! The difficulty bot (docs/STUDIO-2.md §8.1). A debug parameter turns it
//! on (`studio_bot=good`, or `first-timer`, `casual`, `expert`): the engine
//! taps to start a round, and after each frame's `love.update` asks a
//! policy what a player would do. The policy is the project's
//! `game/bot.lua` (`function bot(act)`, loaded into the round's VM after
//! `main.lua`, bot runs only), or, without one, the quick path's default
//! for its player's behaviour (`autoplay.rs`). It never moves anything
//! itself: it says what to press, where and when (`act.tap`, `act.hold`,
//! …), and this module adds the person from the preset's `synth::Skill`:
//!
//! - a reaction delay (lognormal, at least 0.15 s) and lapses (looking away
//!   for a while), only on a `new` situation, as S5 found: the next beat of
//!   a rhythm (`new = false`) gets timing noise only;
//! - Fitts' law time and scatter for where a press lands, from where the
//!   finger is, by the target's `size`;
//! - timing noise around when the press is due (`at`);
//! - one finger: decisions wait while it's busy, and a fresh press lifts it
//!   first;
//! - a held finger (`act.hold`) steers as a thumb does: a big change of
//!   where it's wanted is a ballistic Fitts move, and under that it follows
//!   continuously, closing the gap at the preset's `track.gain` and
//!   wandering by `track.wander` (`Skill::steer`); a new decision waits for
//!   one reaction (later ones while it's pending replace its target), so
//!   fine steering is quick and skill shows in it.
//!
//! The touches it makes go in with the frame's input (`maimbrain::input`),
//! through `Rounds` and the input mapper, as a person's do, so touch parity
//! holds. A round ends at `studio_bot_max` seconds (the preset's cap) if the
//! game hasn't ended it, and the bot logs one line, which the runners stop
//! at (`mb difficulty`'s Deno worker and its native runner):
//!
//! `studio-bot {"preset":…,"seed":…,"seconds":…,"score":…,"cause":…,"won":…,"capped":…,"presses":…,"reads":…,"events":[[kind, t], …]}`
//!
//! `cause` is `round.over`'s (or the hazard that took the last life,
//! "lives" without one, "time" at the cap); `events` are what the game's
//! `on` handlers heard (pickups, hurts, overlaps starting, things leaving),
//! for the report's pacing; `reads` says the game's logic read something
//! back from the host (text metrics, a sound's position): the native
//! runner, whose host is stubbed, can't play such a game as the phone does.

use std::collections::VecDeque;
use std::fmt::Write;

use maimbrain::Rng;
use maimbrain::input::{Event, Kind, pad, synth};
use maimbrain::sys::{self, Level};
use maimbrain::ui::hints::Follow;

pub use synth::Skill;

/// What a policy asked for in a frame. Positions are canvas units (as
/// `love.mouse` reports them), times seconds from now.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// A fresh press at (x, y) on a target `size` across, ideally `at`
    /// from now, held `hold`.
    Tap { x: f32, y: f32, size: f32, at: f32, hold: f32, new: bool },
    /// The finger down at (x, y), within `size / 2`, until a release (or
    /// another hold): it presses if it's up and drags there if it's down.
    Hold { x: f32, y: f32, size: f32, new: Option<bool> },
    /// The finger up.
    Release,
    /// A flick at `angle` (radians, 0 right, y down) for `length` units,
    /// from `from` (the finger's place, else where it is).
    Swipe { angle: f32, length: f32, from: Option<(f32, f32)>, at: f32, new: bool },
    /// A controller button (`pad::A`, …), held `hold`.
    Press { button: u32, at: f32, hold: f32, new: bool },
    /// The left stick (-1–1 each; y down).
    Stick { ax: f32, ay: f32, new: bool },
}

/// The press a policy didn't say how long to hold.
pub const TAP_HOLD: f32 = 0.06;
/// The default target size (units): a thumb's pad, Apple's 44 pt and a bit.
pub const TAP_SIZE: f32 = 60.0;
/// How far a held finger may be from where the policy wants it before it
/// drags there (`size` 40: within 20 units).
pub const HOLD_SIZE: f32 = 40.0;
/// Between a finger's lift and its next press.
const GAP: f64 = 0.03;
/// A tap due within this of one already planned, at about the same place,
/// is the same decision (a policy asks every frame while its condition
/// holds); a second tap elsewhere is a new one.
const SAME_TAP: f64 = 0.25;
/// "About the same place": within half the target, at least this (screen units).
const SAME_PLACE: f32 = 12.0;
/// A press still waiting for the finger this long after it should have set
/// off is dropped.
const STALE: f64 = 0.3;

/// A press waiting for the finger: when the player has taken it in
/// (`ready`, after reaction and any lapse), when it's aimed at (`due`,
/// with timing noise), where (screen units), on what size, for how long.
#[derive(Clone, Debug)]
enum Plan {
    Tap { ready: f64, due: f64, to: (f32, f32), size: f32, hold: f64 },
    Swipe { ready: f64, due: f64, from: Option<(f32, f32)>, angle: f32, length: f32 },
}

/// Where the policy wants a held finger (screen units), and whether getting
/// there is a new decision.
#[derive(Clone, Copy, Debug)]
struct Desire {
    to: (f32, f32),
    tol: f32,
    new: bool,
}

pub struct Bot {
    pub preset: &'static str,
    skill: Skill,
    rng: Rng,
    /// The input made and not yet due, in time order.
    queue: Vec<Event>,
    plans: VecDeque<Plan>,
    /// Where the finger was at the last event handed out.
    at_now: (f32, f32),
    /// A drag in progress ends then: a press that's due may cut it short.
    drag_until: f64,
    /// The finger: down or up, where, when it's next free, when it last lifted.
    down: bool,
    finger: (f32, f32),
    free_at: f64,
    up_at: f64,
    /// When and where the presses asked for recently were wanted, and how
    /// near counts as the same place (for `SAME_TAP`); a swipe's place is
    /// none, and swipes match swipes by time alone.
    wanted: VecDeque<(f64, Option<((f32, f32), f32)>)>,
    desire: Option<Desire>,
    /// Where a held finger was wanted, from when the player has seen it
    /// (at once, or after a reaction for a new decision): what it steers by.
    seen: VecDeque<(f64, (f32, f32))>,
    /// The steering hand's drift (screen units), and when it last steered.
    wander: (f32, f32),
    track_at: f64,
    /// Canvas to screen, for the steering model's logical units.
    scale: f32,
    /// The other thumb, on a controller.
    thumb_free: f64,
    stick: (f32, f32),
    /// The round's cap (s).
    pub max: f64,
    started: bool,
    pub round_start: Option<f64>,
    pub done: bool,
    pub presses: u32,
    /// `studio_trace`: each press as it's scheduled, as a `studio-event` line.
    trace: bool,
    /// What the game's handlers heard: (kind, seconds into the round).
    pub events: Vec<(String, f64)>,
    /// Reading the hints (`ui.hint`, lessons): the kit's `bot::Literal`'s
    /// rules (`maimbrain::ui::hints::Follow`).
    hints: Follow,
}

/// The preset a bot run plays as (`studio_bot`; `good` for a name it
/// doesn't know), if this is a bot run.
pub fn preset() -> Option<Skill> {
    let name = sys::param("studio_bot")?;
    Some(Skill::ALL.into_iter().find(|s| s.name == name.trim()).unwrap_or(Skill::good()))
}

impl Bot {
    /// The bot the debug parameters ask for, if any.
    pub fn from_params(seed: u64) -> Option<Bot> {
        let skill = preset()?;
        let max = sys::param_as::<f64>("studio_bot_max").filter(|m| *m > 0.0).unwrap_or(45.0);
        Some(Bot::new(skill, seed, max, sys::param("studio_trace").is_some()))
    }

    pub fn new(skill: Skill, seed: u64, max: f64, trace: bool) -> Bot {
        Bot {
            preset: skill.name,
            skill,
            rng: Rng::new(seed ^ 0xb07_b07),
            queue: Vec::new(),
            plans: VecDeque::new(),
            at_now: (0.0, 0.0),
            drag_until: f64::MIN,
            down: false,
            finger: (0.0, 0.0),
            free_at: 0.0,
            up_at: f64::MIN,
            wanted: VecDeque::new(),
            desire: None,
            seen: VecDeque::new(),
            wander: (0.0, 0.0),
            track_at: f64::MIN,
            scale: 1.0,
            thumb_free: 0.0,
            stick: (0.0, 0.0),
            max,
            started: false,
            round_start: None,
            done: false,
            presses: 0,
            trace,
            events: Vec::new(),
            hints: Follow::new(),
        }
    }

    /// A new round: no hint seen yet.
    pub fn new_round(&mut self, now: f64) {
        self.round_start = Some(now);
        self.hints = Follow::new();
    }

    /// What the hints on screen ask, as a player who does exactly that
    /// would (§8.1; the kit's `bot::Literal`): once a hint has been up for
    /// this player's reaction to it, a TAP is tapped every 0.3 s while it
    /// shows, a HOLD held until a LET GO. A hint that names no place is
    /// done at `home` (canvas units). The reaction is drawn here, so the
    /// presses are a rhythm already going (`new = false`) to `decide`.
    pub fn follow_hints(&mut self, now: f64, hints: &maimbrain::ui::Hints, home: (f32, f32)) -> Vec<Intent> {
        use maimbrain::ui::hints::Want;
        let (skill, rng) = (&self.skill, &mut self.rng);
        let wants = self.hints.step(now, hints, || skill.reaction_time(rng));
        wants
            .into_iter()
            .map(|w| match w {
                Want::Tap { at, .. } => {
                    let (x, y) = at.unwrap_or(home);
                    Intent::Tap { x, y, size: TAP_SIZE, at: 0.0, hold: TAP_HOLD, new: false }
                }
                Want::Hold { at, .. } => {
                    let (x, y) = at.unwrap_or(home);
                    Intent::Hold { x, y, size: HOLD_SIZE, new: Some(false) }
                }
                Want::LetGo { .. } => Intent::Release,
            })
            .collect()
    }

    /// The title is up and the project loaded: tap the middle once to
    /// start, at 0.25 s of game time (or as soon as it's loaded after
    /// that), so a native round and a runner's start on the same frame.
    pub fn start(&mut self, now: f64, w: f32, h: f32) {
        if !self.started {
            self.started = true;
            let at = (now + 0.05).max(0.25);
            self.finger = (w / 2.0, h * 0.6);
            self.push(synth::tap(w / 2.0, h * 0.6, at));
            self.free_at = at + synth::TAP_HOLD;
            self.up_at = self.free_at;
        }
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// The bot's input due by `now`.
    pub fn due(&mut self, now: f64) -> Vec<Event> {
        let n = self.queue.partition_point(|e| e.time <= now);
        let out: Vec<Event> = self.queue.drain(..n).collect();
        if let Some(e) = out.iter().rfind(|e| matches!(e.kind(), Kind::TouchDown | Kind::TouchMove | Kind::TouchUp)) {
            self.at_now = (e.x, e.y);
        }
        out
    }

    /// Adds input (any order; kept in time order).
    fn push(&mut self, events: impl IntoIterator<Item = Event>) {
        for e in events {
            let i = self.queue.partition_point(|q| q.time <= e.time);
            self.queue.insert(i, e);
        }
    }

    /// Where the finger is, in canvas units through `tf` (offset x, y and
    /// scale), and whether it's down: `act.finger()`.
    pub fn finger(&self, tf: (f32, f32, f32)) -> (f32, f32, bool) {
        let (ox, oy, s) = tf;
        ((self.finger.0 - ox) / s, (self.finger.1 - oy) / s, self.down)
    }

    /// A reaction to something new: the delay, and a lapse now and then.
    fn react(&mut self, new: bool) -> f64 {
        if !new {
            return 0.0;
        }
        let mut r = self.skill.reaction_time(&mut self.rng) as f64;
        if self.rng.f32() < self.skill.lapse.0 {
            r += (self.skill.lapse.1 * self.rng.range(0.5, 1.5)) as f64;
        }
        r
    }

    /// Whether a press wanted at `t` (at `at`, for a tap) is one already asked for.
    fn asked(&self, t: f64, at: Option<((f32, f32), f32)>) -> bool {
        self.wanted.iter().any(|(w, place)| {
            (w - t).abs() < SAME_TAP
                && match (place, at) {
                    (Some((p, tol)), Some((q, _))) => dist(*p, q) <= *tol,
                    (None, None) => true,
                    _ => false,
                }
        })
    }

    /// The policy's decisions this frame (`now`); `tf` maps the canvas to
    /// the screen (offset x, y and scale).
    pub fn decide(&mut self, now: f64, intents: &[Intent], tf: (f32, f32, f32)) {
        let (ox, oy, s) = tf;
        self.scale = s.max(1e-3);
        let screen = |x: f32, y: f32| (ox + x * s, oy + y * s);
        while self.wanted.front().is_some_and(|(t, _)| *t < now - SAME_TAP) {
            self.wanted.pop_front();
        }
        for it in intents {
            match *it {
                Intent::Tap { x, y, size, at, hold, new } => {
                    let wanted = now + at.max(0.0) as f64;
                    let place = Some((screen(x, y), (size * s / 2.0).max(SAME_PLACE)));
                    // The same press again (the policy's condition still holds): one decision.
                    if self.asked(wanted, place) {
                        continue;
                    }
                    let ready = now + self.react(new);
                    let due = self.skill.timed(wanted, &mut self.rng);
                    let hold = (hold.max(TAP_HOLD) as f64 * (1.0 + synth::gauss(&mut self.rng) as f64 * self.skill.timing as f64 * 2.0)).max(synth::TAP_HOLD);
                    self.plans.push_back(Plan::Tap { ready, due, to: screen(x, y), size: (size * s).max(1.0), hold });
                    self.wanted.push_back((wanted, place));
                }
                Intent::Swipe { angle, length, from, at, new } => {
                    let wanted = now + at.max(0.0) as f64;
                    if self.asked(wanted, None) {
                        continue;
                    }
                    let ready = now + self.react(new);
                    let due = self.skill.timed(wanted, &mut self.rng);
                    self.plans.push_back(Plan::Swipe { ready, due, from: from.map(|(x, y)| screen(x, y)), angle, length: length * s });
                    self.wanted.push_back((wanted, None));
                }
                Intent::Hold { x, y, size, new } => {
                    let to = screen(x, y);
                    let tol = (size * s / 2.0).max(1.0);
                    if self.desire.is_none() {
                        self.seen.clear();
                    }
                    let d = match self.desire {
                        // About where it was: the same hold, not a new decision.
                        Some(d) if dist(d.to, to) <= tol => Desire { to, tol, new: new.unwrap_or(d.new) },
                        Some(_) => Desire { to, tol, new: new.unwrap_or(false) },
                        None => Desire { to, tol, new: new.unwrap_or(true) },
                    };
                    self.desire = Some(d);
                    // A finger that's down steers by what the player has taken in: a
                    // new decision after a reaction, a correction (`new = false`, what
                    // the player expects) at once, as taps are.
                    if self.down {
                        match self.seen.back_mut() {
                            // Still reacting: it acts on the latest when it does
                            // (one reaction, as a drag that's under way had).
                            Some(last) if last.0 > now => last.1 = to,
                            last => {
                                // Asked again every frame about where it was: the same decision.
                                let fresh = d.new && last.is_none_or(|e| dist(e.1, to) > tol);
                                let at = now + if fresh { self.react(true) } else { 0.0 };
                                self.seen.push_back((at, to));
                            }
                        }
                        self.desire = Some(Desire { new: false, ..d });
                    }
                }
                Intent::Release => {
                    self.desire = None;
                    self.seen.clear();
                }
                Intent::Press { button, at, hold, new } => {
                    let ready = now + self.react(new);
                    let due = self.skill.timed(now + at.max(0.0) as f64, &mut self.rng);
                    let t = ready.max(due).max(self.thumb_free + GAP).max(now);
                    let hold = (hold.max(TAP_HOLD) as f64).max(synth::TAP_HOLD);
                    self.push([pad_event(Kind::PadButton, button, 1.0, t), pad_event(Kind::PadButton, button, 0.0, t + hold)]);
                    self.thumb_free = t + hold;
                    self.presses += 1;
                }
                Intent::Stick { ax, ay, new } => {
                    let (ax, ay) = (ax.clamp(-1.0, 1.0), ay.clamp(-1.0, 1.0));
                    if (ax - self.stick.0).abs() < 0.05 && (ay - self.stick.1).abs() < 0.05 {
                        continue;
                    }
                    let t = now + self.react(new) + (synth::gauss(&mut self.rng) * self.skill.timing).abs() as f64;
                    self.push([pad_event(Kind::PadAxis, pad::LX, ax, t), pad_event(Kind::PadAxis, pad::LY, ay, t)]);
                    self.stick = (ax, ay);
                }
            }
        }
        self.pump(now);
    }

    /// When the finger has to set off for a plan: once the player has
    /// taken it in, and soon enough to land when it's due.
    fn sets_off(&self, p: &Plan) -> f64 {
        match p {
            Plan::Tap { ready, due, to, size, .. } => ready.max(due - self.skill.touch.time(dist(self.finger, *to), *size) as f64 - GAP),
            Plan::Swipe { ready, due, .. } => ready.max(due - GAP),
        }
    }

    /// Hands the finger its next job when it's free: the oldest plan once
    /// it's time to set off for it, else where the policy wants it held
    /// (or up). Until then a held finger stays where it is.
    fn pump(&mut self, now: f64) {
        // A press that's due cuts a drag short: the thumb leaves from where it is.
        if self.free_at > now && self.drag_until == self.free_at && self.plans.front().is_some_and(|p| self.sets_off(p) <= now) {
            self.queue.retain(|e| !(e.kind() == Kind::TouchMove && e.time > now));
            self.finger = self.at_now;
            (self.free_at, self.drag_until) = (now, f64::MIN);
        }
        while self.free_at <= now {
            let start = now.max(self.free_at);
            if self.plans.front().is_none_or(|p| self.sets_off(p) > now + 1.0 / 60.0) {
                self.follow(start);
                return;
            }
            let Some(p) = self.plans.pop_front() else {
                return;
            };
            // A press the finger couldn't get to in time is let go: by now the
            // player would see it again, and the policy asks again if it still holds.
            if start - self.sets_off(&p) > STALE {
                continue;
            }
            match p {
                Plan::Tap { ready, due, to, size, hold } => {
                    let was_down = self.down;
                    let (touches, at) = self.skill.tap(self.finger, to, size, 0.0, &mut self.rng);
                    let movement = touches[0].time;
                    // A finger that's down lifts as it sets off.
                    let off = ready.max(start).max(self.up_at + GAP);
                    let down = (off + movement + if was_down { GAP } else { 0.0 }).max(due);
                    self.lift((down - movement - GAP).max(start));
                    self.trace_press(now, down, ready, due);
                    self.push([synth::touch(Kind::TouchDown, at.0, at.1, down)]);
                    self.presses += 1;
                    self.finger = at;
                    self.down = true;
                    self.free_at = down + hold;
                    // Let go after it, unless the policy wants a finger held: then it
                    // stays down and drags there (a tap, then hold where it landed).
                    if self.desire.is_none() {
                        self.push([synth::touch(Kind::TouchUp, at.0, at.1, down + hold)]);
                        self.down = false;
                        self.up_at = down + hold;
                    }
                }
                Plan::Swipe { ready, due, from, angle, length } => {
                    let from = from.unwrap_or(self.finger);
                    let movement = self.skill.touch.time(dist(self.finger, from), TAP_SIZE) as f64;
                    let t0 = (ready.max(start).max(self.up_at + GAP) + movement + GAP).max(due);
                    self.lift((t0 - movement - GAP).max(start));
                    self.trace_press(now, t0, ready, due);
                    let flick = self.skill.flick(from, angle, length, t0, &mut self.rng);
                    let end = flick.last().map_or((from, t0), |e| ((e.x, e.y), e.time));
                    self.push(flick);
                    self.presses += 1;
                    (self.finger, self.free_at, self.up_at, self.down) = (end.0, end.1, end.1, false);
                }
            }
        }
    }

    /// The finger up at `t` if it's down; when it's (been) up.
    fn lift(&mut self, t: f64) -> f64 {
        if self.down {
            self.push([synth::touch(Kind::TouchUp, self.finger.0, self.finger.1, t)]);
            self.down = false;
            self.up_at = t;
        }
        self.up_at
    }

    /// No press waiting: the finger goes where the policy wants it held, or up.
    fn follow(&mut self, start: f64) {
        match self.desire {
            None => {
                if self.down {
                    self.lift(start);
                    self.free_at = start;
                }
            }
            Some(d) if !self.down => {
                let react = self.react(d.new);
                let (touches, at) = self.skill.tap(self.finger, d.to, d.tol * 2.0, 0.0, &mut self.rng);
                let down = (start + react).max(self.up_at + GAP) + touches[0].time;
                self.trace_press(start, down, start + react, start);
                self.push([synth::touch(Kind::TouchDown, at.0, at.1, down)]);
                self.presses += 1;
                (self.finger, self.down, self.free_at) = (at, true, down);
                self.desire = Some(Desire { new: false, ..d });
            }
            Some(d) => {
                // Where the player has seen it wanted; nothing yet, it stays.
                let Some(to) = self.seen_at(start) else { return };
                if dist(self.finger, to) <= d.tol * 2.0 {
                    self.steer(start, to);
                    return;
                }
                // A big change: a ballistic drag there, in this player's Fitts
                // time, landing scattered; corrections follow it (steer).
                let (touches, at) = self.skill.tap(self.finger, to, d.tol * 2.0, 0.0, &mut self.rng);
                let (t0, secs) = (start, touches[0].time.max(1.0 / 60.0));
                let from = self.finger;
                let n = ((secs * synth::PANEL_HZ as f64).ceil() as usize).max(1);
                let path: Vec<Event> = (1..=n)
                    .map(|i| {
                        let k = i as f32 / n as f32;
                        // Minimum jerk: quick in the middle, slow at the ends.
                        let e = k * k * k * (10.0 - 15.0 * k + 6.0 * k * k);
                        synth::touch(Kind::TouchMove, from.0 + (at.0 - from.0) * e, from.1 + (at.1 - from.1) * e, t0 + secs * i as f64 / n as f64)
                    })
                    .collect();
                self.push(path);
                (self.finger, self.free_at, self.drag_until, self.track_at) = (at, t0 + secs, t0 + secs, t0 + secs);
            }
        }
    }

    /// Where the player has seen the held finger wanted by `t`.
    fn seen_at(&mut self, t: f64) -> Option<(f32, f32)> {
        while self.seen.len() >= 2 && self.seen[1].0 <= t {
            self.seen.pop_front();
        }
        self.seen.front().filter(|e| e.0 <= t).map(|e| e.1)
    }

    /// A frame of a held finger's corrections toward `to` (screen units),
    /// at the touch panel's rate; the move goes in for the next frame.
    fn steer(&mut self, start: f64, to: (f32, f32)) {
        let dt = (start - self.track_at).clamp(0.0, 0.1);
        self.track_at = start;
        let n = ((dt * synth::PANEL_HZ as f64).ceil() as usize).max(1);
        let k = self.scale;
        let (mut at, target) = ((self.finger.0 / k, self.finger.1 / k), (to.0 / k, to.1 / k));
        let mut wander = (self.wander.0 / k, self.wander.1 / k);
        for _ in 0..n {
            at = self.skill.steer(at, target, &mut wander, (dt / n as f64) as f32, &mut self.rng);
        }
        self.wander = (wander.0 * k, wander.1 * k);
        let at = (at.0 * k, at.1 * k);
        if dist(at, self.finger) > 0.25 {
            let t = start + 1e-3;
            self.push([synth::touch(Kind::TouchMove, at.0, at.1, t)]);
            (self.finger, self.free_at, self.drag_until) = (at, t, t);
        }
    }

    fn trace_press(&self, now: f64, down: f64, ready: f64, due: f64) {
        if self.trace {
            sys::log(Level::Debug, &format!("studio-event {now:.3} press - - {down:.3} {ready:.3} {due:.3}"));
        }
    }

    /// Something the game's handlers heard, `t` s into the round.
    pub fn event(&mut self, kind: String, t: f64) {
        if !self.done && self.events.len() < 4096 {
            self.events.push((kind, t));
        }
    }

    /// The one line the runners stop at.
    pub fn report(&mut self, now: f64, outcome: &Outcome) {
        if self.done {
            return;
        }
        self.done = true;
        let secs = now - self.round_start.unwrap_or(now);
        let mut j = String::from("{");
        let _ = write!(j, "\"preset\":\"{}\",\"seed\":\"{}\",\"seconds\":{secs:.4},\"score\":{},\"cause\":", self.preset, sys::rand_seed(), crate::api::fmt_num(outcome.score));
        quote(&mut j, &outcome.cause);
        let _ = write!(j, ",\"won\":{},\"capped\":{},\"presses\":{},\"reads\":{},\"events\":[", outcome.won, outcome.capped, self.presses, outcome.reads);
        for (i, (k, t)) in self.events.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            j.push('[');
            quote(&mut j, k);
            let _ = write!(j, ",{t:.3}]");
        }
        j.push(']');
        if let Some(e) = &outcome.error {
            j.push_str(",\"error\":");
            quote(&mut j, e);
        }
        j.push('}');
        sys::log(Level::Info, &format!("studio-bot {j}"));
    }
}

/// A bot run's side of a round's VM (`api::Host::bot`): what the policy
/// asked for this frame, what the game's handlers heard, and what the
/// default bots remember.
#[derive(Debug, Default)]
pub struct BotFrame {
    pub intents: Vec<Intent>,
    pub events: Vec<(String, f64)>,
    /// The game's logic read something back from the host (`Host::read_back`).
    pub reads: std::cell::Cell<bool>,
    /// What last hurt the player (a hazard's tag, a hazard tile's name).
    pub hurt_by: Option<String>,
    /// The finger, as `act.finger()` reports it (canvas units, down).
    pub finger: (f32, f32, bool),
    /// `act` (api/act.rs), in the registry.
    pub act_ref: std::ffi::c_int,
    /// The `on("overlap")` pairs touching last frame, so only a start counts.
    pub overlapping: Vec<(maimbrain::scene::NodeId, maimbrain::scene::NodeId)>,
    pub auto: crate::autoplay::State,
    /// `act.followHints()` this frame.
    pub follow_hints: bool,
}

/// Whether this is a bot run (`studio_bot`).
pub fn wanted() -> bool {
    sys::param("studio_bot").is_some()
}

/// How a bot's round went.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub score: f64,
    pub cause: String,
    pub won: bool,
    pub capped: bool,
    pub reads: bool,
    pub error: Option<String>,
}

/// One bot round natively (`mb difficulty`'s native runner, and tests): the
/// engine on the SDK's fake host, its host stubbed (no assets load, mb2d and
/// mb3d answer nothing back), frames updated and never rendered. `files`
/// is the project as `mb build` ships it (each `game/*.lua` with its
/// bytecode beside it). Returns the round's `studio-bot` JSON, and with
/// `studio_trace` in `params` its `studio-event` lines.
#[cfg(not(target_arch = "wasm32"))]
pub fn native_round(files: &crate::project::Files, preset: &str, seed: u64, max: f64, params: &[(String, String)]) -> Result<(String, Vec<String>), String> {
    use maimbrain::sys::test::{Call, Host, set_param};
    let host = Host::start(seed);
    set_param("studio_bot", preset);
    set_param("studio_bot_max", &max.to_string());
    for (k, v) in params {
        set_param(k, v);
    }
    crate::use_project(files.clone());
    let mut game: crate::Studio2 = host.init();
    let limit = ((max + 5.0) * 60.0) as u64;
    let mut events = Vec::new();
    let mut errors = Vec::new();
    while host.frames() <= limit {
        host.update(&mut game, 1.0 / 60.0);
        for c in host.take_calls() {
            let Call::Log { message, .. } = c else {
                continue;
            };
            if let Some(line) = message.strip_prefix("studio-bot ") {
                return Ok((line.to_string(), events));
            } else if let Some(e) = message.strip_prefix("studio-event ") {
                events.push(e.to_string());
            } else if message.starts_with("studio: ") && errors.len() < 5 {
                errors.push(message);
            }
        }
    }
    Err(format!("{preset} seed {seed}: no studio-bot line after {limit} frames {errors:?}"))
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn pad_event(kind: Kind, code: u32, a: f32, t: f64) -> Event {
    Event { kind: kind as u8, pad: 1, code, a, time: t, ..Event::default() }
}

/// A controller button by LÖVE's name (`act.press("a")`).
pub fn button(name: &str) -> Option<u32> {
    Some(match name {
        "a" => pad::A,
        "b" => pad::B,
        "x" => pad::X,
        "y" => pad::Y,
        "leftshoulder" => pad::LB,
        "rightshoulder" => pad::RB,
        "dpup" => pad::UP,
        "dpdown" => pad::DOWN,
        "dpleft" => pad::LEFT,
        "dpright" => pad::RIGHT,
        _ => return None,
    })
}

/// A JSON string.
pub fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(skill: Skill) -> Bot {
        Bot::new(skill, 7, 45.0, false)
    }

    const TF: (f32, f32, f32) = (0.0, 0.0, 1.0);

    /// Frames from `from` to `to` with nothing new asked: what the bot does.
    fn frames(b: &mut Bot, from: f64, to: f64) -> Vec<Event> {
        let mut out = Vec::new();
        let mut now = from;
        while now < to {
            b.decide(now, &[], TF);
            out.extend(b.due(now));
            now += 1.0 / 60.0;
        }
        out
    }

    #[test]
    fn a_tap_lands_after_reaction_and_movement_later_for_a_first_timer() {
        let tap = Intent::Tap { x: 100.0, y: 300.0, size: 60.0, at: 0.0, hold: 0.06, new: true };
        let when = |skill: Skill| -> f64 {
            // Averaged over seeds: one draw of a first-timer can beat one of an expert.
            (0..50)
                .map(|seed| {
                    let mut b = Bot::new(skill, seed, 45.0, false);
                    b.finger = (180.0, 500.0);
                    b.decide(1.0, std::slice::from_ref(&tap), TF);
                    frames(&mut b, 1.0, 4.0).iter().find(|e| e.kind() == Kind::TouchDown).map(|e| e.time).unwrap()
                })
                .sum::<f64>()
                / 50.0
        };
        let (slow, fast) = (when(Skill::first_timer()), when(Skill::expert()));
        assert!(slow > fast + 0.15 && fast > 1.0 + 0.23, "{slow} {fast}");
    }

    #[test]
    fn the_same_tap_asked_every_frame_is_one_press() {
        let mut b = bot(Skill::good());
        for f in 0..20 {
            // The policy keeps asking for the same moment as it comes closer.
            let now = 1.0 + f as f64 / 60.0;
            let at = (1.3 - now) as f32;
            b.decide(now, &[Intent::Tap { x: 100.0, y: 300.0, size: 60.0, at, hold: 0.06, new: true }], TF);
        }
        let downs = frames(&mut b, 1.4, 4.0).iter().filter(|e| e.kind() == Kind::TouchDown).count();
        assert_eq!((downs, b.presses), (1, 1));
    }

    /// Two taps wanted within `SAME_TAP` at different places (two enemies,
    /// two answers) are two presses; at nearly the same place, one.
    #[test]
    fn a_second_tap_elsewhere_is_a_second_press() {
        let taps = |second: (f32, f32)| {
            let mut b = bot(Skill::good());
            b.finger = (180.0, 500.0);
            // Wanted 0.24 s apart (inside SAME_TAP), with time to get there.
            let tap = |(x, y): (f32, f32), at: f32| Intent::Tap { x, y, size: 60.0, at, hold: 0.06, new: false };
            b.decide(1.0, &[tap((100.0, 300.0), 0.5)], TF);
            b.decide(1.0 + 1.0 / 60.0, &[tap(second, 0.74 - 1.0 / 60.0)], TF);
            let downs: Vec<(f32, f32)> = frames(&mut b, 1.0 + 2.0 / 60.0, 4.0).iter().filter(|e| e.kind() == Kind::TouchDown).map(|e| (e.x, e.y)).collect();
            downs
        };
        let two = taps((170.0, 300.0));
        assert_eq!(two.len(), 2, "{two:?}");
        assert!(two[0].0 < 135.0 && two[1].0 > 135.0, "the left one, then the right: {two:?}");
        assert_eq!(taps((104.0, 302.0)).len(), 1, "nearly the same place is the same tap");
    }

    #[test]
    fn a_rhythm_beat_is_timed_not_reacted_to() {
        // A beat due in 0.4 s at the finger's place: no reaction delay
        // (0.28 s or more), just timing noise and the press's own motion.
        let mut b = bot(Skill::good());
        b.finger = (100.0, 300.0);
        b.decide(1.0, &[Intent::Tap { x: 100.0, y: 300.0, size: 60.0, at: 0.4, hold: 0.06, new: false }], TF);
        let down = frames(&mut b, 1.0, 3.0)[0].time;
        assert!((down - 1.4).abs() < 0.15, "{down}");
    }

    #[test]
    fn a_held_finger_presses_once_drags_where_its_wanted_and_lets_go() {
        let mut b = bot(Skill::casual());
        b.finger = (180.0, 500.0);
        let mut now = 1.0;
        let mut events = Vec::new();
        for f in 0..240 {
            let x = if f < 60 { 200.0 } else { 300.0 };
            let ask = if f < 100 { Intent::Hold { x, y: 400.0, size: 40.0, new: None } } else { Intent::Release };
            b.decide(now, &[ask], TF);
            events.extend(b.due(now));
            now += 1.0 / 60.0;
        }
        events.extend(b.due(100.0));
        let kinds: Vec<Kind> = events.iter().map(|e| e.kind()).collect();
        assert_eq!(kinds.iter().filter(|k| **k == Kind::TouchDown).count(), 1, "{kinds:?}");
        assert_eq!(kinds.last(), Some(&Kind::TouchUp));
        let moved = events.iter().rfind(|e| e.kind() == Kind::TouchMove).unwrap();
        assert!((moved.x - 300.0).abs() < 40.0, "dragged to the new place: {}", moved.x);
        assert!(events.windows(2).all(|w| w[1].time >= w[0].time));
    }

    #[test]
    fn a_held_finger_steers_continuously_after_a_moving_target() {
        // A target sweeping side to side (a ship to keep under a lane): the
        // finger follows it every frame, a little behind, not in jumps.
        let run = |skill: Skill| {
            let mut b = bot(skill);
            b.finger = (180.0, 500.0);
            let (mut now, mut errs, mut moves) = (1.0, Vec::new(), 0);
            let mut last = None;
            while now < 7.0 {
                let x = 180.0 + 80.0 * ((now - 1.0) as f32 * 1.5).sin();
                b.decide(now, &[Intent::Hold { x, y: 500.0, size: 40.0, new: Some(false) }], TF);
                for e in b.due(now) {
                    if e.kind() == Kind::TouchMove {
                        moves += 1;
                    }
                    last = Some(e.x);
                }
                if now > 2.0 {
                    errs.push((last.unwrap() - x).abs());
                }
                now += 1.0 / 60.0;
            }
            (errs.iter().sum::<f32>() / errs.len() as f32, moves)
        };
        let ((first, first_moves), (expert, expert_moves)) = (run(Skill::first_timer()), run(Skill::expert()));
        assert!(first_moves > 300 && expert_moves > 300, "a move most frames: {first_moves}, {expert_moves}");
        // 120 units/s at most: about speed × (delay + 1/gain) behind, plus wander.
        assert!(expert < 25.0 && first > expert * 1.4, "first-timer {first:.1}, expert {expert:.1} units off on average");
    }

    #[test]
    fn a_tap_while_holding_lifts_presses_and_keeps_holding() {
        let mut b = bot(Skill::good());
        b.finger = (300.0, 300.0);
        let hold = Intent::Hold { x: 300.0, y: 300.0, size: 40.0, new: Some(false) };
        b.decide(1.0, std::slice::from_ref(&hold), TF);
        let mut events = b.due(1.5);
        b.decide(1.5, &[hold.clone(), Intent::Tap { x: 200.0, y: 100.0, size: 60.0, at: 0.0, hold: 0.4, new: true }], TF);
        for f in 0..120 {
            let now = 1.5 + f as f64 / 60.0;
            b.decide(now, std::slice::from_ref(&hold), TF);
            events.extend(b.due(now));
        }
        let kinds: Vec<Kind> = events.iter().map(|e| e.kind()).filter(|k| *k != Kind::TouchMove).collect();
        assert_eq!(kinds, [Kind::TouchDown, Kind::TouchUp, Kind::TouchDown], "the tap's press stays down, then drags back");
        let last = events.last().unwrap();
        assert!(last.kind() == Kind::TouchMove && (last.x - 300.0).abs() < 25.0, "{last:?}");
    }
}
