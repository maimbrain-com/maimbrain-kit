//! Cutscenes for intros and endings: a timeline of steps (camera shots and
//! moves, voiced lines with portraits and subtitles, waits, fades,
//! letterbox bars, title cards, credits, sounds and cues) that plays on game
//! time, the same at any frame rate, and that the player can hurry with a tap
//! or skip whole by holding a finger down.
//!
//! ```
//! use maimbrain::gfx3d::vec3;
//! use maimbrain::motion::Ease;
//! use maimbrain::ui::{SeqEvent, Sequence, Shot, Theme, VoiceLine};
//! let wide = Shot::new(vec3(0.0, 3.0, 12.0), vec3(0.0, 1.0, 0.0));
//! let close = Shot::new(vec3(-1.0, 1.6, 3.0), vec3(-1.5, 1.2, 0.0)).fov(0.7);
//! let mut ending = Sequence::new(Theme::default())
//!     .letterbox(0.25)
//!     .shot(wide, 0.0)
//!     .fade_in(1.0).meanwhile()            // fades in while the camera moves
//!     .move_to(close, 3.0, Ease::SineInOut)
//!     .line("VARN", &VoiceLine::subtitle("So this is how it ends."), "battered")
//!     .cue("varn_falls")                   // the game knocks the mech over
//!     .wait(0.5)
//!     .title("ONE YEAR LATER", "", 2.0)
//!     .credits(&[("VARN", "the champion"), ("KIT", "the challenger")], 5.0)
//!     .fade_out(1.0);
//! let mut cues = Vec::new();
//! for _ in 0..60 * 30 {
//!     maimbrain::sys::test::advance(1.0 / 60.0);
//!     for e in ending.update(1.0 / 60.0) {
//!         if let SeqEvent::Cue(name) = e { cues.push(name) }
//!     }
//!     // render: gfx3d::camera(&ending.camera().unwrap()); the scene; ending.draw(&layout);
//! }
//! assert!(ending.finished() && cues == ["varn_falls"]);
//! ```
//!
//! Each step starts when the one before it ends (a line ends when its voice
//! and the dialogue's auto-advance pause are over), at the exact moment it
//! ends, not at the next frame, so a sequence lands on the same frames at 30
//! or 120 Hz. `.meanwhile()` after a step lets the next start with it (a
//! camera move under a line). A tap hurries a line (as in [`Dialogue`]) or
//! cuts a camera move, wait or title short; holding a finger down for 0.7 s
//! skips the rest (the cues still arrive, so the game ends up where the scene
//! would have left it).

use super::color::{rgba, with_alpha};
use super::dialogue::{Dialogue, DialogueEvent, LineBox, Portrait, VoiceLine};
use super::layout::{Layout, Rect};
use super::shape::{arc, ring};
use super::text::text;
use super::theme::Theme;
use crate::audio::Sound;
use crate::gfx2d;
use crate::gfx3d::{Camera, Vec3};
use crate::input::{Event, Kind};
use crate::motion::Ease;

/// Seconds a finger must stay down to skip the whole sequence.
pub const HOLD_TO_SKIP: f32 = 0.7;
/// Taps this soon after the sequence starts don't count (the tap that ended the fight).
const GUARD: f64 = 0.3;
/// Letterbox bars slide in or out over this long.
const BARS_SECS: f32 = 0.6;

/// A camera position: where it is, what it looks at, its vertical field of
/// view in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Shot {
    pub fn new(pos: Vec3, target: Vec3) -> Shot {
        Shot { pos, target, fov_y: 1.0 }
    }

    pub fn fov(mut self, fov_y: f32) -> Shot {
        self.fov_y = fov_y;
        self
    }

    /// A [`Camera`] looking from `pos` at `target` (`gfx3d::camera(&shot.camera())`).
    pub fn camera(&self) -> Camera {
        Camera { fov_y: self.fov_y, ..Camera::looking_at(self.pos, self.target) }
    }

    /// Between `a` and `b`: position, target and field of view each eased
    /// straight across (the camera turns to keep the target in view).
    pub fn lerp(a: Shot, b: Shot, k: f32) -> Shot {
        Shot { pos: a.pos.lerp(b.pos, k), target: a.target.lerp(b.target, k), fov_y: a.fov_y + (b.fov_y - a.fov_y) * k }
    }
}

/// What [`Sequence::update`] reports.
#[derive(Clone, Debug, PartialEq)]
pub enum SeqEvent {
    /// Step `i` began.
    Step(usize),
    /// A `.cue(name)` step: do the game's part now (knock a mech over, swap the music).
    Cue(String),
    /// A line began (its voice is heard from now).
    LineStarted,
    /// The player held to skip; the cues not yet reached came with it.
    Skipped,
    /// The last step is over: carry on.
    Finished,
}

#[derive(Clone, Debug)]
enum Step {
    Shot(Shot, f32),
    Move(Shot, f32, Ease),
    Line { speaker: String, line: VoiceLine, expression: String },
    Wait(f32),
    Letterbox(f32),
    Fade(f32, f32),
    Title(String, String, f32),
    Credits(Vec<(String, String)>, f32),
    Sound(Sound, f32),
    Cue(String),
}

impl Step {
    /// How long it holds the timeline (a line: until it's done).
    fn secs(&self) -> f32 {
        match self {
            Step::Shot(_, s) | Step::Move(_, s, _) | Step::Wait(s) | Step::Fade(_, s) | Step::Title(_, _, s) | Step::Credits(_, s) => *s,
            _ => 0.0,
        }
    }
}

/// A value easing from `from` to `to` over `secs` from `start`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Track {
    from: f32,
    to: f32,
    start: f64,
    secs: f32,
    ease: Ease,
}

impl Track {
    fn still(v: f32) -> Track {
        Track { from: v, to: v, start: 0.0, secs: 0.0, ease: Ease::Linear }
    }

    fn at(&self, t: f64) -> f32 {
        let k = if self.secs <= 0.0 { 1.0 } else { ((t - self.start) as f32 / self.secs).clamp(0.0, 1.0) };
        self.from + (self.to - self.from) * self.ease.at(k)
    }

    /// Heads for `to` from wherever it is at `t`.
    fn retarget(&mut self, to: f32, t: f64, secs: f32, ease: Ease) {
        *self = Track { from: self.at(t), to, start: t, secs, ease };
    }
}

type Roll = (Vec<(String, String)>, f64, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    Until(f64),
    Line,
}

/// A cutscene: build it from steps, then `update`, `input` and `draw` it
/// each frame until [`Sequence::finished`]. See the [module docs](self).
#[derive(Clone, Debug)]
pub struct Sequence {
    pub theme: Theme,
    /// The lines' dialogue box (portraits, subtitles, music ducking). It
    /// moves on by itself 0.5 s after each voice (`talk.advance`).
    pub talk: Dialogue,
    /// The color fades go to.
    pub fade_color: u32,
    steps: Vec<(Step, bool)>,
    next: usize,
    t: f64,
    /// When the next step starts (the moment the one before ended).
    cursor: f64,
    hold: Option<Hold>,
    cam: Option<(Shot, Shot, f64, f32, Ease)>,
    bars: Track,
    fade: Track,
    title: Option<(String, String, f64, f32)>,
    /// The credits rolling: (role, name) lines, when they started, for how long.
    credits: Option<Roll>,
    down: Option<f32>,
    finished: bool,
    skipped: bool,
}

impl Sequence {
    pub fn new(theme: Theme) -> Sequence {
        Sequence {
            theme,
            talk: Dialogue::new(theme).auto_advance(0.5),
            fade_color: rgba(0, 0, 0, 255),
            steps: Vec::new(),
            next: 0,
            t: 0.0,
            cursor: 0.0,
            hold: None,
            cam: None,
            bars: Track::still(0.0),
            fade: Track::still(0.0),
            title: None,
            credits: None,
            down: None,
            finished: false,
            skipped: false,
        }
    }

    fn push(mut self, s: Step) -> Sequence {
        self.steps.push((s, false));
        self
    }

    /// Gives a speaker a portrait ([`Dialogue::cast`]).
    pub fn cast(mut self, name: &str, portrait: Portrait) -> Sequence {
        self.talk = self.talk.cast(name, portrait);
        self
    }

    /// Draws the lines' box your own way, to match the game's own panel
    /// ([`Dialogue::draw_line`]): `f` gets each frame's [`LineBox`] (where
    /// the box goes, above the letterbox bar; the speaker, the text typed so
    /// far, the portrait). Without it the box is the theme's panel; for
    /// another theme's panel, set `seq.talk.theme`.
    ///
    /// ```
    /// use maimbrain::gfx2d::{self, Font};
    /// use maimbrain::ui::{Rect, Sequence, Theme, VoiceLine, text};
    /// // The game's comm panel: a dark plate with a green rule, the face
    /// // left, the name in the pixel font, the line typing out beside it.
    /// let intro = Sequence::new(Theme::default())
    ///     .draw_line(|l| {
    ///         let r = l.rect;
    ///         gfx2d::rect(r.x, r.y, r.w, r.h, 0x101820f0);
    ///         gfx2d::rect(r.x, r.y, r.w, 2.0, 0x40ff80ff);
    ///         let mut x = r.x + 12.0;
    ///         if let Some(p) = l.portrait {
    ///             p.draw(Rect::new(x, r.y + 12.0, 96.0, r.h - 24.0), &l.line.expression, l.line.mouth, l.t);
    ///             x += 108.0;
    ///         }
    ///         text(&l.line.speaker).font(Font::Pixel).size(16.0).color(0x40ff80ff).draw(x, r.y + 12.0);
    ///         l.draw_text(x, r.y + 36.0, r.right() - 12.0 - x, 16.0);
    ///     })
    ///     .line("KIT", &VoiceLine::subtitle("Signal's clear. Let's go."), "");
    /// # let _ = intro;
    /// ```
    pub fn draw_line(mut self, f: impl Fn(&LineBox) + 'static) -> Sequence {
        self.talk = self.talk.draw_line(f);
        self
    }

    /// The step before doesn't hold the timeline: the next one starts with it
    /// (a camera move under a line, a fade over a move).
    pub fn meanwhile(mut self) -> Sequence {
        if let Some(s) = self.steps.last_mut() {
            s.1 = true;
        }
        self
    }

    /// Cuts to `shot` and holds it `secs`.
    pub fn shot(self, shot: Shot, secs: f32) -> Sequence {
        self.push(Step::Shot(shot, secs))
    }

    /// Moves the camera from where it is to `shot` over `secs`.
    pub fn move_to(self, shot: Shot, secs: f32, ease: Ease) -> Sequence {
        self.push(Step::Move(shot, secs, ease))
    }

    /// `speaker` says `line` with `expression`, in the dialogue box; the
    /// timeline waits for it (its voice, then the auto-advance pause).
    pub fn line(self, speaker: &str, line: &VoiceLine, expression: &str) -> Sequence {
        self.push(Step::Line { speaker: speaker.to_string(), line: line.clone(), expression: expression.to_string() })
    }

    pub fn wait(self, secs: f32) -> Sequence {
        self.push(Step::Wait(secs))
    }

    /// Black bars covering `amount` of the screen's height between them (0.2–0.3 is cinematic; 0 takes them away), sliding in over 0.6 s without holding the timeline.
    pub fn letterbox(self, amount: f32) -> Sequence {
        self.push(Step::Letterbox(amount.clamp(0.0, 0.9)))
    }

    /// From `fade_color` to the scene over `secs` (the screen starts covered).
    pub fn fade_in(mut self, secs: f32) -> Sequence {
        if self.steps.iter().all(|s| !matches!(s.0, Step::Fade(..))) {
            self.fade = Track::still(1.0);
        }
        self.push(Step::Fade(0.0, secs))
    }

    /// To `fade_color` over `secs`.
    pub fn fade_out(self, secs: f32) -> Sequence {
        self.push(Step::Fade(1.0, secs))
    }

    /// A title card in the middle ("ONE YEAR LATER"), with an optional line under it, for `secs` (fading in and out).
    pub fn title(self, title: &str, sub: &str, secs: f32) -> Sequence {
        self.push(Step::Title(title.to_string(), sub.to_string(), secs))
    }

    /// Credits rolling up over `secs`: each (role, name), the cast first.
    pub fn credits(self, lines: &[(&str, &str)], secs: f32) -> Sequence {
        self.push(Step::Credits(lines.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(), secs))
    }

    /// Plays `sound` as the step is reached (a sting, a crash, the music's last chord).
    pub fn sound(self, sound: Sound, vol: f32) -> Sequence {
        self.push(Step::Sound(sound, vol))
    }

    /// Reports [`SeqEvent::Cue`] as the step is reached, for the game's part.
    pub fn cue(self, name: &str) -> Sequence {
        self.push(Step::Cue(name.to_string()))
    }

    /// Seconds since it started (its own clock: game time, jumped forward by taps that cut a step short).
    pub fn age(&self) -> f64 {
        self.t
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    /// The player held to skip it.
    pub fn skipped(&self) -> bool {
        self.skipped
    }

    /// Seconds the finger has been held toward skipping (0 when none is down).
    pub fn held(&self) -> f32 {
        self.down.unwrap_or(0.0)
    }

    /// The camera now: the last shot, or the move under way (None before the first shot).
    pub fn shot_now(&self) -> Option<Shot> {
        self.cam.map(|(a, b, start, secs, ease)| {
            let k = if secs <= 0.0 { 1.0 } else { ((self.t - start) as f32 / secs).clamp(0.0, 1.0) };
            Shot::lerp(a, b, ease.at(k))
        })
    }

    /// [`Sequence::shot_now`] as a [`Camera`], for `gfx3d::camera`.
    pub fn camera(&self) -> Option<Camera> {
        self.shot_now().map(|s| s.camera())
    }

    /// The letterbox bars: the fraction of the screen's height they cover between them.
    pub fn bars(&self) -> f32 {
        self.bars.at(self.t)
    }

    /// How covered the screen is by the fade, 0–1.
    pub fn fade(&self) -> f32 {
        self.fade.at(self.t)
    }

    /// Feeds one input event. True while the sequence runs (it takes every
    /// touch, click and key): a press hurries a line or cuts the step showing
    /// short; holding it [`HOLD_TO_SKIP`] seconds skips the rest.
    pub fn input(&mut self, e: &Event) -> bool {
        if self.finished {
            return false;
        }
        match e.kind() {
            Kind::TouchUp | Kind::TouchCancel | Kind::MouseUp => self.down = None,
            _ if (e.is_press() || e.kind() == Kind::KeyDown || e.pad_pressed().is_some()) && self.t >= GUARD => {
                if e.is_press() {
                    self.down = Some(0.0);
                }
                if self.talk.active() {
                    self.talk.input(e);
                } else if let Some(Hold::Until(end)) = self.hold {
                    // Cut short: the clock jumps to the step's end, so everything lands where it would have.
                    self.t = self.t.max(end);
                }
            }
            _ => {}
        }
        true
    }

    /// Advances: starts the steps that are due, plays the lines, reports
    /// what happened. Call every frame while it runs.
    pub fn update(&mut self, dt: f32) -> Vec<SeqEvent> {
        let mut ev = Vec::new();
        if self.finished {
            return ev;
        }
        self.t += dt as f64;
        if let Some(d) = &mut self.down {
            *d += dt;
            if *d >= HOLD_TO_SKIP {
                self.skip_all(&mut ev);
                return ev;
            }
        }
        if self.talk.update(dt) == Some(DialogueEvent::LineStarted) {
            ev.push(SeqEvent::LineStarted);
        }
        loop {
            match self.hold {
                Some(Hold::Until(end)) if self.t >= end => {
                    self.cursor = end;
                    self.hold = None;
                }
                Some(Hold::Line) if !self.talk.active() => {
                    self.cursor = self.t;
                    self.hold = None;
                }
                Some(_) => break,
                None => {}
            }
            if self.next >= self.steps.len() {
                if !self.talk.active() && self.t >= self.cursor {
                    self.finished = true;
                    ev.push(SeqEvent::Finished);
                }
                break;
            }
            let i = self.next;
            self.next += 1;
            ev.push(SeqEvent::Step(i));
            let (step, meanwhile) = self.steps[i].clone();
            self.start(step.clone(), self.cursor, &mut ev);
            if !meanwhile {
                if matches!(step, Step::Line { .. }) {
                    self.hold = Some(Hold::Line);
                } else if step.secs() > 0.0 {
                    self.hold = Some(Hold::Until(self.cursor + step.secs() as f64));
                }
            }
        }
        ev
    }

    fn start(&mut self, step: Step, at: f64, ev: &mut Vec<SeqEvent>) {
        match step {
            Step::Shot(s, _) => self.cam = Some((s, s, at, 0.0, Ease::Linear)),
            Step::Move(s, secs, ease) => {
                let from = self.shot_now().unwrap_or(s);
                self.cam = Some((from, s, at, secs, ease));
            }
            Step::Line { speaker, line, expression } => self.talk.say(&speaker, &line, &expression),
            Step::Wait(_) => {}
            Step::Letterbox(a) => self.bars.retarget(a, at, BARS_SECS, Ease::CubicOut),
            Step::Fade(to, secs) => self.fade.retarget(to, at, secs, Ease::QuadInOut),
            Step::Title(a, b, secs) => self.title = Some((a, b, at, secs)),
            Step::Credits(lines, secs) => self.credits = Some((lines, at, secs)),
            Step::Sound(s, vol) => {
                s.play(vol, 0.0, 1.0);
            }
            Step::Cue(name) => ev.push(SeqEvent::Cue(name)),
        }
    }

    /// Skips the rest: the camera, bars and fade go where the steps would
    /// have left them, the cues not yet reached are reported, and nothing
    /// more is said or played.
    fn skip_all(&mut self, ev: &mut Vec<SeqEvent>) {
        self.talk.skip_all();
        ev.push(SeqEvent::Skipped);
        let far = self.t + 1e6;
        while self.next < self.steps.len() {
            let (step, _) = self.steps[self.next].clone();
            self.next += 1;
            match step {
                Step::Line { .. } | Step::Sound(..) | Step::Title(..) | Step::Credits(..) => {}
                s => self.start(s, self.t, ev),
            }
        }
        // Everything under way lands now.
        self.t = far;
        self.title = None;
        self.credits = None;
        self.hold = None;
        self.down = None;
        self.skipped = true;
        self.finished = true;
        ev.push(SeqEvent::Finished);
    }

    /// Draws the bars, a title or the credits, the dialogue box, the fade
    /// and the hold-to-skip ring, over the scene.
    pub fn draw(&self, layout: &Layout) {
        let th = &self.theme;
        let s = layout.screen;
        let bar = (self.bars() * s.h * 0.5).round();
        if bar > 0.0 {
            gfx2d::rect(s.x, s.y, s.w, bar, rgba(0, 0, 0, 255));
            gfx2d::rect(s.x, s.bottom() - bar, s.w, bar, rgba(0, 0, 0, 255));
        }
        if let Some((title, sub, at, secs)) = &self.title {
            let u = (self.t - at) as f32;
            if (0.0..*secs).contains(&u) {
                let a = (u / 0.5).min((secs - u) / 0.5).clamp(0.0, 1.0);
                let size = th.fit(th.title_font, 30.0);
                text(&th.case(title)).font(th.title_font).size(size).color(0xffffffff).outline(0.08, rgba(0, 0, 0, 200)).alpha(a).middle().draw(s.cx(), s.cy() - 8.0);
                if !sub.is_empty() {
                    text(sub).font(th.body_font).size(th.fit(th.body_font, 15.0)).color(0xffffffdd).alpha(a).center().draw(s.cx(), s.cy() + size * 0.6);
                }
            }
        }
        if let Some((lines, at, secs)) = &self.credits {
            self.draw_credits(lines, (self.t - at) as f32, *secs, layout, s.y + bar);
        }
        // The box sits above the bottom bar.
        let mut dl = *layout;
        dl.safe.h = (s.bottom() - bar.max(0.0) + 6.0 - dl.safe.y).min(dl.safe.h);
        self.talk.draw(&dl);
        let f = self.fade();
        if f > 0.0 {
            gfx2d::rect(s.x, s.y, s.w, s.h, with_alpha(self.fade_color, f * ((self.fade_color & 0xff) as f32 / 255.0)));
        }
        if !self.finished {
            // Top right, under the bar (the dialogue box has the bottom).
            let r = Rect::new(s.right() - 58.0, bar.max(layout.safe.y) + 8.0, 44.0, 44.0);
            let (cx, cy) = (r.cx(), r.cy());
            let held = self.held() / HOLD_TO_SKIP;
            let hint = if self.down.is_some() { 1.0 } else { (1.0 - (self.t as f32 - 2.5) / 0.5).clamp(0.0, 0.6) };
            if hint > 0.0 {
                // A dark backing so it reads over a bright sky.
                super::shape::rrect(Rect::new(cx - 98.0, cy - 16.0, 116.0, 32.0), 16.0, rgba(0, 0, 0, (140.0 * hint) as u8));
                ring(cx, cy, 14.0, 3.0, with_alpha(0xffffffff, 0.55 * hint));
                if held > 0.0 {
                    arc(cx, cy, 14.0, 3.0, -std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2 + held.min(1.0) * std::f32::consts::TAU, with_alpha(0xffffffff, 0.95));
                }
                text("HOLD TO SKIP").size(11.0).color(with_alpha(0xffffffff, hint)).outline(0.15, rgba(0, 0, 0, 200)).align(super::text::Align::Right).valign(super::text::VAlign::Middle).draw(cx - 22.0, cy);
            }
        }
    }

    fn draw_credits(&self, lines: &[(String, String)], u: f32, secs: f32, layout: &Layout, bar_bottom: f32) {
        if !(0.0..secs).contains(&u) {
            return;
        }
        let th = &self.theme;
        let s = layout.screen;
        let gap = 58.0;
        let total = lines.len() as f32 * gap;
        // From just below the screen to just above it.
        let y0 = s.bottom() - (s.h + total) * (u / secs);
        let a = (u / 0.4).min((secs - u) / 0.4).clamp(0.0, 1.0);
        // Lines fade out under the top letterbox bar or the safe area (the
        // status bar, the Dynamic Island), never at the screen's edge.
        let top = bar_bottom.max(layout.safe.y) + 12.0;
        for (i, (role, name)) in lines.iter().enumerate() {
            let y = y0 + i as f32 * gap;
            if y < top - 30.0 || y > s.bottom() {
                continue;
            }
            let a = a * ((y - top + 30.0) / 30.0).clamp(0.0, 1.0);
            text(&th.case(role)).font(th.body_font).size(th.fit(th.body_font, 13.0)).color(th.accent).tracking(0.2).alpha(a).center().draw(s.cx(), y);
            text(name).font(th.title_font).size(th.fit(th.title_font, 22.0)).color(0xffffffff).outline(0.08, rgba(0, 0, 0, 200)).alpha(a).center().draw(s.cx(), y + 18.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx3d::vec3;
    use crate::input::synth;
    use crate::sys;

    fn a() -> Shot {
        Shot::new(vec3(0.0, 2.0, 10.0), Vec3::ZERO)
    }
    fn b() -> Shot {
        Shot::new(vec3(4.0, 1.0, 2.0), vec3(1.0, 1.0, 0.0)).fov(0.6)
    }

    fn run(s: &mut Sequence, secs: f32, hz: f32) -> Vec<(f64, SeqEvent)> {
        let mut out = Vec::new();
        for _ in 0..(secs * hz).round() as usize {
            sys::test::advance(1.0 / hz);
            for e in s.update(1.0 / hz) {
                out.push((s.age(), e));
            }
        }
        out
    }

    fn scene() -> Sequence {
        Sequence::new(Theme::default())
            .letterbox(0.25)
            .shot(a(), 1.0)
            .fade_in(1.0)
            .meanwhile()
            .move_to(b(), 2.0, Ease::SineInOut)
            .cue("boom")
            .wait(0.5)
            .title("LATER", "", 1.0)
            .fade_out(0.5)
    }

    #[test]
    fn steps_run_in_order_and_land_on_time() {
        sys::test::reset();
        let mut s = scene();
        let ev = run(&mut s, 8.0, 60.0);
        let at = |want: &SeqEvent| ev.iter().find(|(_, e)| e == want).map(|(t, _)| *t).unwrap();
        assert!((at(&SeqEvent::Cue("boom".into())) - 3.0).abs() < 0.02, "shot 1 s, then the move 2 s under the fade");
        assert!((at(&SeqEvent::Finished) - 5.0).abs() < 0.02);
        assert!(s.finished() && !s.skipped());
        assert_eq!(s.shot_now(), Some(b()));
        assert!((s.fade() - 1.0).abs() < 1e-6 && (s.bars() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn the_same_moment_looks_the_same_at_any_frame_rate() {
        for t in [0.5f32, 1.7, 2.3, 3.9] {
            let mut x = scene();
            let mut y = scene();
            sys::test::reset();
            run(&mut x, t, 30.0);
            sys::test::reset();
            run(&mut y, t, 120.0);
            let (px, py) = (x.shot_now().unwrap(), y.shot_now().unwrap());
            assert!((px.pos - py.pos).length() < 1e-3, "{t}: {px:?} {py:?}");
            assert!((x.fade() - y.fade()).abs() < 1e-3 && (x.bars() - y.bars()).abs() < 1e-3, "{t}");
        }
        // Halfway through the eased move: halfway there.
        let mut s = scene();
        sys::test::reset();
        run(&mut s, 2.0, 60.0);
        assert!((s.shot_now().unwrap().pos - a().pos.lerp(b().pos, 0.5)).length() < 0.02);
    }

    #[test]
    fn lines_hold_the_timeline_and_a_tap_cuts_a_step_short() {
        sys::test::reset();
        let mut s = Sequence::new(Theme::default()).line("A", &VoiceLine::subtitle("Hello there."), "").cue("after").wait(5.0).cue("end");
        let ev = run(&mut s, 0.5, 60.0);
        assert!(ev.iter().any(|e| e.1 == SeqEvent::LineStarted) && !ev.iter().any(|e| e.1 == SeqEvent::Cue("after".into())));
        let ev = run(&mut s, 2.0, 60.0);
        let after = ev.iter().find(|e| e.1 == SeqEvent::Cue("after".into())).expect("after the line and its pause").0;
        // A tap during the long wait ends it.
        let tap = synth::tap(180.0, 300.0, sys::time());
        assert!(s.input(&tap[0]));
        assert!(s.input(&tap[tap.len() - 1]));
        let ev = run(&mut s, 0.1, 60.0);
        assert!(ev.iter().any(|e| e.1 == SeqEvent::Cue("end".into())), "{ev:?}");
        assert!(s.finished() && s.age() - after >= 5.0 - 1e-6, "the clock jumped to the wait's end");
    }

    #[test]
    fn holding_skips_everything_but_the_cues() {
        sys::test::reset();
        let mut s = scene();
        run(&mut s, 0.5, 60.0);
        let down = synth::tap(180.0, 300.0, sys::time())[0];
        assert!(s.input(&down));
        let ev = run(&mut s, HOLD_TO_SKIP + 0.05, 60.0);
        let names: Vec<_> = ev.iter().map(|e| e.1.clone()).filter(|e| !matches!(e, SeqEvent::Step(_))).collect();
        assert_eq!(names, [SeqEvent::Skipped, SeqEvent::Cue("boom".into()), SeqEvent::Finished]);
        assert!(s.skipped() && s.shot_now() == Some(b()) && s.fade() == 1.0);
        assert!(!s.input(&down), "done: the game gets its input back");
    }

    #[test]
    fn a_game_draws_the_lines_box_its_own_way() {
        use crate::gfx2d::Image;
        use std::cell::RefCell;
        use std::rc::Rc;
        sys::test::reset();
        type Seen = Rc<RefCell<Vec<(String, String, bool, Rect)>>>;
        let seen: Seen = Rc::default();
        let log = seen.clone();
        let mut s = Sequence::new(Theme::default())
            .cast("KIT", Portrait::from_image(Image(3), 48, 48))
            .letterbox(0.25)
            .draw_line(move |l| log.borrow_mut().push((l.line.speaker.clone(), l.shown().to_string(), l.portrait.is_some(), l.rect)))
            .line("KIT", &VoiceLine::subtitle("Signal's clear. Let's go."), "");
        let layout = Layout::from_screen(&Default::default());
        let mut shown = Vec::new();
        for _ in 0..90 {
            run(&mut s, 1.0 / 30.0, 30.0);
            s.draw(&layout);
            if let Some(last) = seen.borrow().last() {
                shown.push(last.1.clone());
            }
        }
        let seen = seen.borrow();
        assert!(!seen.is_empty(), "the hook draws the line");
        let (speaker, text, portrait, r) = seen.last().unwrap();
        assert_eq!((speaker.as_str(), text.as_str(), *portrait), ("KIT", "Signal's clear. Let's go.", true));
        assert!(shown.windows(2).all(|w| w[1].len() >= w[0].len()) && shown[0].len() < text.len(), "it types out: {shown:?}");
        // Above the letterbox bar, across the screen.
        assert!(r.bottom() <= 640.0 - 0.25 * 320.0 + 6.0 && r.w > 300.0, "{r:?}");
        // Without the hook, the default box (no panic, nothing recorded).
        let mut plain = Sequence::new(Theme::default()).line("KIT", &VoiceLine::subtitle("Hi."), "");
        run(&mut plain, 0.5, 30.0);
        plain.draw(&layout);
    }

    #[test]
    fn taps_at_the_start_dont_count() {
        sys::test::reset();
        let mut s = Sequence::new(Theme::default()).wait(2.0);
        s.update(0.016);
        let tap = synth::tap(10.0, 10.0, 0.0)[0];
        assert!(s.input(&tap));
        run(&mut s, 0.1, 60.0);
        assert!(!s.finished());
    }
}
