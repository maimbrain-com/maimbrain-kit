//! A between-rounds stats card: "FIGHT 2 CLEAR", the round's stats rolling
//! up one row at a time, a rank letter stamped on, the next opponent
//! teased, then a wipe into the next scene. Driven by `update(dt)` (game
//! time) and skippable by tap, like the other cards.
//!
//! ```
//! use maimbrain::ui::{Layout, StatsCard, StatsEvent, Theme, Wipe};
//! let mut card = StatsCard::new("FIGHT 2 CLEAR", Theme::arcade(), &Layout::default())
//!     .percent("ACCURACY", 0.87)
//!     .int("PARTS BROKEN", 6)
//!     .time("TIME", 42.3)
//!     .rank("A")
//!     .next("GRANNY", "she's never lost")
//!     .wipe(Wipe::Iris);
//! let mut covered = false;
//! for _ in 0..60 * 12 {
//!     for e in card.update(1.0 / 60.0) {
//!         // Row ticks, a row landing, the rank's stamp: sounds and haptics.
//!         covered |= e == StatsEvent::Covered; // swap to the next scene here
//!     }
//! }
//! assert!(covered && card.finished());
//! ```

use super::color::{fade, over, readable, with_alpha};
use super::color::rgba;
use super::layout::{Layout, PILL_ZONE, Rect};
use super::shape::{disc, ring, rrect};
use super::text::{Align, fmt_int, text};
use super::theme::Theme;
use crate::audio::Sound;
use crate::gfx2d;
use crate::input::Event;
use crate::sys::{Asset, AssetState};
use crate::motion::{Ease, Spring};

/// How a stat's number is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatFormat {
    /// A whole number (1,234).
    Int,
    /// A fraction 0–1 shown as a percentage (87%).
    Percent,
    /// Seconds shown as m:ss.t (0:42.3).
    Time,
}

#[derive(Clone, Debug)]
struct Row {
    label: String,
    value: f32,
    format: StatFormat,
    /// Shown in the "bad" color (damage taken, deaths).
    bad: bool,
}

/// The transition out into the next scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wipe {
    /// A circle closes on the middle, then opens on the next scene.
    Iris,
    /// A slanted band sweeps across, right to left.
    Slide,
    /// Two halves close from top and bottom, then part.
    Shutter,
}

/// Moments to play a sound or haptic for (and `Covered`, to swap scenes).
#[derive(Clone, Debug, PartialEq)]
pub enum StatsEvent {
    /// A row began rolling (index).
    Row(usize),
    /// A rolling number changed: a tick.
    Tick,
    /// A row's number landed (index).
    Landed(usize),
    /// The rank letter stamped on.
    Rank(String),
    /// The next opponent slid in.
    Teaser,
    /// The wipe began.
    Out,
    /// The screen is fully covered: switch to the next scene now, and draw
    /// it under the card from here on.
    Covered,
    /// The wipe has opened on the next scene: drop the card.
    Done,
}

/// Seconds: the title lands, the first row starts, one row after another,
/// a row's roll, the rank after the last row, the teaser after the rank.
const TITLE: f32 = 0.35;
const FIRST_ROW: f32 = 0.5;
const ROW_GAP: f32 = 0.5;
const ROLL: f32 = 0.4;
const RANK_AFTER: f32 = 0.35;
const TEASER_AFTER: f32 = 0.7;
/// The wipe: closing, then opening.
const CLOSE: f32 = 0.45;
const OPEN: f32 = 0.4;
/// Dims the scene behind the card, whatever the theme.
const SCRIM: u32 = rgba(6, 6, 14, 165);
/// Taps are ignored this long after the card appears (a shot fired as the
/// fight ended shouldn't skip it).
const GUARD: f32 = 0.35;

/// The card's sounds, played by [`StatsCard::update`] at its events. Any
/// can be left out. [`CardSounds::load`] takes the set the skill's
/// `scripts/card_sfx.py` writes; or fill the fields with your own [`Sound`]s.
#[derive(Clone, Debug, Default)]
pub struct CardSounds {
    /// A short jingle as the card comes in.
    pub intro: Option<Sound>,
    /// A row sliding in.
    pub row: Option<Sound>,
    /// One step of a rolling number (played at most every 45 ms, rising in pitch as it rolls).
    pub tick: Option<Sound>,
    /// A number landing.
    pub land: Option<Sound>,
    /// The rank stamped on.
    pub rank: Option<Sound>,
    /// The next opponent sliding in.
    pub teaser: Option<Sound>,
    /// The wipe closing.
    pub wipe: Option<Sound>,
    /// A sting as the wipe goes, into the next scene.
    pub outro: Option<Sound>,
    /// Volume for all of them (default 0.8).
    pub vol: f32,
    pending: Vec<(usize, Asset)>,
}

impl CardSounds {
    pub fn new() -> CardSounds {
        CardSounds { vol: 0.8, ..Default::default() }
    }

    /// Starts loading `<dir>/card_intro.ogg`, `card_row`, `card_tick`,
    /// `card_land`, `card_rank`, `card_teaser`, `card_wipe` and
    /// `card_outro` (what `card_sfx.py <dir>` writes); each plays once it has
    /// decoded, and a missing one is left out.
    pub fn load(dir: &str) -> CardSounds {
        let pending = Self::NAMES.iter().enumerate().map(|(i, n)| (i, Asset::load(&format!("{dir}/card_{n}.ogg")))).collect();
        CardSounds { pending, ..CardSounds::new() }
    }

    const NAMES: [&'static str; 8] = ["intro", "row", "tick", "land", "rank", "teaser", "wipe", "outro"];

    fn slot(&mut self, i: usize) -> &mut Option<Sound> {
        match i {
            0 => &mut self.intro,
            1 => &mut self.row,
            2 => &mut self.tick,
            3 => &mut self.land,
            4 => &mut self.rank,
            5 => &mut self.teaser,
            6 => &mut self.wipe,
            _ => &mut self.outro,
        }
    }

    /// Takes in the sounds that have decoded (the card does this itself;
    /// call it on a set you keep, to clone into each card, so each doesn't load its own).
    pub fn poll(&mut self) {
        let mut i = 0;
        while i < self.pending.len() {
            let (slot, a) = self.pending[i];
            match a.state() {
                AssetState::Pending => i += 1,
                AssetState::Failed => {
                    self.pending.remove(i);
                }
                AssetState::Ready => {
                    if let Some(snd) = Sound::new(a) {
                        *self.slot(slot) = Some(snd);
                    }
                    self.pending.remove(i);
                }
            }
        }
    }

    fn play(&self, s: Option<Sound>, vol: f32, pitch: f32) {
        if let Some(s) = s {
            s.play(self.vol * vol, 0.0, pitch);
        }
    }
}

/// The between-rounds card. Build it with the stats, feed it every event
/// (`input`: the first tap reveals everything, the next starts the wipe),
/// call `update(dt)` each frame and play sounds for what it returns, and
/// `draw` (or `draw_with`, to put a portrait in the teaser) last, over the
/// scene. On `StatsEvent::Covered` switch to the next scene and keep
/// drawing the card over it until `finished()`.
///
/// Its text keeps inside `Layout::card` (clear of the feed's overlays);
/// the wipe covers the whole screen.
#[derive(Clone, Debug)]
pub struct StatsCard {
    pub title: String,
    pub theme: Theme,
    pub wipe: Wipe,
    rows: Vec<Row>,
    rank: Option<String>,
    next: Option<(String, String)>,
    /// Seconds after everything is shown before the wipe starts by itself
    /// (None: wait for a tap).
    hold: Option<f32>,
    area: Rect,
    screen: Rect,
    t: f32,
    /// The card's clock jumps here on a skip: every reveal happens by it.
    out_at: Option<f32>,
    stamp: Spring,
    shown_rank: bool,
    shown_teaser: bool,
    landed: Vec<bool>,
    started: Vec<bool>,
    ticks: Vec<i64>,
    out_sent: bool,
    covered: bool,
    done: bool,
    ink: Option<u32>,
    sounds: Option<CardSounds>,
    last_tick: f32,
}

impl StatsCard {
    pub fn new(title: &str, theme: Theme, layout: &Layout) -> StatsCard {
        StatsCard {
            title: title.to_string(),
            theme,
            wipe: Wipe::Iris,
            rows: Vec::new(),
            rank: None,
            next: None,
            hold: Some(2.5),
            area: layout.card,
            screen: layout.screen,
            t: 0.0,
            out_at: None,
            stamp: Spring::new(0.0, 4.0, 0.35),
            shown_rank: false,
            shown_teaser: false,
            landed: Vec::new(),
            started: Vec::new(),
            ticks: Vec::new(),
            out_sent: false,
            covered: false,
            done: false,
            ink: None,
            sounds: None,
            last_tick: -1.0,
        }
    }

    /// The color of the card's numbers and names. By default it's the
    /// theme's `text`, swapped for near-white or near-black when that
    /// wouldn't read on the card's panel over the scrim (a theme with dark
    /// ink on a dark panel); the labels, the bad rows and the accents are
    /// checked the same way.
    pub fn ink(mut self, color: u32) -> Self {
        self.ink = Some(color);
        self
    }

    /// Plays these at the card's events ([`CardSounds`]): the intro as it
    /// comes in, a row's slide, the roll's ticks, the landing, the rank's
    /// stamp, the teaser's whoosh, the wipe and the outro sting.
    pub fn sounds(mut self, sounds: CardSounds) -> Self {
        self.sounds = Some(sounds);
        self
    }

    /// A stat row (up to 6 fit).
    pub fn stat(mut self, label: &str, value: f32, format: StatFormat) -> Self {
        self.rows.push(Row { label: label.to_string(), value, format, bad: false });
        self.landed.push(false);
        self.started.push(false);
        self.ticks.push(0);
        self
    }
    pub fn int(self, label: &str, value: i64) -> Self {
        self.stat(label, value as f32, StatFormat::Int)
    }
    /// `fraction` 0–1, shown as a percentage.
    pub fn percent(self, label: &str, fraction: f32) -> Self {
        self.stat(label, fraction, StatFormat::Percent)
    }
    /// Seconds, shown as m:ss.t.
    pub fn time(self, label: &str, seconds: f32) -> Self {
        self.stat(label, seconds, StatFormat::Time)
    }
    /// Marks the last row as a bad one (damage taken): drawn in `theme.bad`.
    pub fn bad(mut self) -> Self {
        if let Some(r) = self.rows.last_mut() {
            r.bad = true;
        }
        self
    }
    /// The rank letter stamped on after the rows ("S", "A", "B", "C").
    /// `StatsCard::rank_for` picks one from a 0–1 score.
    pub fn rank(mut self, letter: &str) -> Self {
        self.rank = Some(letter.to_string());
        self
    }
    /// The next opponent's name and a line under it.
    pub fn next(mut self, name: &str, line: &str) -> Self {
        self.next = Some((name.to_string(), line.to_string()));
        self
    }
    pub fn wipe(mut self, w: Wipe) -> Self {
        self.wipe = w;
        self
    }
    /// Seconds to hold once everything is shown before wiping out by
    /// itself (default 2.5).
    pub fn hold(mut self, secs: f32) -> Self {
        self.hold = Some(secs);
        self
    }
    /// Wait for a tap before wiping out.
    pub fn manual(mut self) -> Self {
        self.hold = None;
        self
    }

    /// A rank from a 0–1 score: S at 0.9 and up, A 0.75, B 0.5, else C.
    pub fn rank_for(score: f32) -> &'static str {
        match score {
            s if s >= 0.9 => "S",
            s if s >= 0.75 => "A",
            s if s >= 0.5 => "B",
            _ => "C",
        }
    }

    fn row_start(&self, i: usize) -> f32 {
        FIRST_ROW + i as f32 * ROW_GAP
    }
    fn rank_at(&self) -> f32 {
        self.row_start(self.rows.len().max(1) - 1) + ROLL + RANK_AFTER
    }
    fn teaser_at(&self) -> f32 {
        self.rank_at() + if self.rank.is_some() { TEASER_AFTER } else { 0.0 }
    }
    /// When everything has been shown.
    fn shown_at(&self) -> f32 {
        self.teaser_at() + if self.next.is_some() { 0.4 } else { 0.0 }
    }

    /// Seconds since the card appeared.
    pub fn age(&self) -> f32 {
        self.t
    }
    /// Every row, the rank and the teaser are showing.
    pub fn all_shown(&self) -> bool {
        self.t >= self.shown_at()
    }
    /// The wipe has started.
    pub fn leaving(&self) -> bool {
        self.out_at.is_some()
    }
    /// The screen is (or has been) fully covered: the next scene is under it.
    pub fn covered(&self) -> bool {
        self.covered
    }
    pub fn finished(&self) -> bool {
        self.done
    }

    /// A tap: the first reveals everything at once, the next (once all is
    /// shown) starts the wipe. True if the card took the event.
    pub fn input(&mut self, e: &Event) -> bool {
        if !e.is_press() || self.t < GUARD {
            return false;
        }
        if !self.all_shown() {
            self.t = self.shown_at();
        } else if self.out_at.is_none() {
            self.out_at = Some(self.t);
        }
        true
    }

    /// Advances the card; returns what happened this frame, in order.
    pub fn update(&mut self, dt: f32) -> Vec<StatsEvent> {
        let mut ev = Vec::new();
        if self.done {
            return ev;
        }
        let first = self.t == 0.0;
        self.t += dt;
        self.stamp.update(dt);
        let t = self.t;
        if let Some(snd) = &mut self.sounds {
            snd.poll();
        }
        for i in 0..self.rows.len() {
            let s = self.row_start(i);
            if t >= s && !self.started[i] {
                self.started[i] = true;
                ev.push(StatsEvent::Row(i));
            }
            if self.started[i] && !self.landed[i] {
                let v = self.shown_value(i);
                let tick = (v * if self.rows[i].format == StatFormat::Percent { 100.0 } else { 1.0 }) as i64;
                if tick != self.ticks[i] {
                    self.ticks[i] = tick;
                    ev.push(StatsEvent::Tick);
                }
                if t >= s + ROLL {
                    self.landed[i] = true;
                    ev.push(StatsEvent::Landed(i));
                }
            }
        }
        if let Some(r) = &self.rank
            && t >= self.rank_at()
            && !self.shown_rank
        {
            self.shown_rank = true;
            self.stamp.snap(2.4);
            self.stamp.target = 1.0;
            ev.push(StatsEvent::Rank(r.clone()));
        }
        if self.next.is_some() && t >= self.teaser_at() && !self.shown_teaser {
            self.shown_teaser = true;
            ev.push(StatsEvent::Teaser);
        }
        if self.out_at.is_none()
            && let Some(h) = self.hold
            && t >= self.shown_at() + h
        {
            self.out_at = Some(t);
        }
        if let Some(o) = self.out_at {
            if !self.out_sent {
                self.out_sent = true;
                ev.push(StatsEvent::Out);
            }
            if !self.covered && t - o >= CLOSE {
                self.covered = true;
                ev.push(StatsEvent::Covered);
            }
            if t - o >= CLOSE + OPEN {
                self.done = true;
                ev.push(StatsEvent::Done);
            }
        }
        self.play_sounds(first, &ev);
        ev
    }

    fn play_sounds(&mut self, first: bool, ev: &[StatsEvent]) {
        let Some(snd) = &self.sounds else { return };
        if first {
            snd.play(snd.intro, 1.0, 1.0);
        }
        for e in ev {
            match e {
                StatsEvent::Row(_) => snd.play(snd.row, 0.6, 1.0),
                StatsEvent::Tick => {
                    if self.t - self.last_tick >= 0.045 {
                        // Rising with the roll of the row that's rolling.
                        let k = (0..self.rows.len()).rev().find(|&i| self.started[i]).map_or(0.0, |i| ((self.t - self.row_start(i)) / ROLL).clamp(0.0, 1.0));
                        snd.play(snd.tick, 0.45, 1.0 + 0.35 * k);
                        self.last_tick = self.t;
                    }
                }
                StatsEvent::Landed(i) => snd.play(snd.land, 0.9, 1.0 + 0.04 * *i as f32),
                StatsEvent::Rank(_) => snd.play(snd.rank, 1.0, 1.0),
                StatsEvent::Teaser => snd.play(snd.teaser, 0.8, 1.0),
                StatsEvent::Out => {
                    snd.play(snd.wipe, 0.9, 1.0);
                    snd.play(snd.outro, 1.0, 1.0);
                }
                _ => {}
            }
        }
    }

    /// Row `i`'s number as shown now (rolling up, eased).
    pub fn shown_value(&self, i: usize) -> f32 {
        let k = ((self.t - self.row_start(i)) / ROLL).clamp(0.0, 1.0);
        self.rows[i].value * Ease::CubicOut.at(k)
    }

    /// How covered the screen is by the wipe: 0 → 1 while closing, 1 → 0
    /// while opening on the next scene.
    pub fn cover(&self) -> f32 {
        match self.out_at {
            None => 0.0,
            Some(o) => {
                let u = self.t - o;
                if u < CLOSE { Ease::CubicIn.at(u / CLOSE) } else { 1.0 - Ease::CubicOut.at(((u - CLOSE) / OPEN).min(1.0)) }
            }
        }
    }

    fn format(&self, i: usize, v: f32) -> String {
        match self.rows[i].format {
            StatFormat::Int => fmt_int(v.round() as i64),
            StatFormat::Percent => format!("{}%", (v * 100.0).round() as i64),
            StatFormat::Time => {
                let m = (v / 60.0).floor() as i64;
                format!("{}:{:04.1}", m, v - m as f32 * 60.0)
            }
        }
    }

    /// Draws the card with an empty frame in the teaser.
    pub fn draw(&self) {
        self.draw_with(|_, _| {});
    }

    /// Draws the card; `teaser(rect, reveal)` draws the next opponent's
    /// portrait in its frame (`reveal` 0 → 1 as it slides in), e.g.
    /// `|r, _| portrait.draw(r, "smug", 0, sys::time())`.
    pub fn draw_with(&self, teaser: impl Fn(Rect, f32)) {
        if !self.covered {
            self.draw_card(&teaser);
        }
        self.draw_wipe();
    }

    fn draw_card(&self, teaser: &dyn Fn(Rect, f32)) {
        let th = &self.theme;
        let a = self.area;
        let t = self.t;
        // A scrim over the scene, so the stats read over anything.
        let dim = (t / 0.25).min(1.0);
        gfx2d::rect(self.screen.x, self.screen.y, self.screen.w, self.screen.h, fade(SCRIM, dim));
        let cx = a.cx();
        // The title slams in from big.
        let k = Ease::BackOut.at((t / TITLE).min(1.0));
        let tsize = th.fit(th.title_font, 34.0);
        let ty = a.y.max(PILL_ZONE.1) + 40.0; // below the pause pill
        gfx2d::push();
        gfx2d::translate(cx, ty);
        let s = 1.6 - 0.6 * k;
        gfx2d::scale(s, s);
        super::layouts::styled(th, &th.case(&self.title), th.title_font, tsize, th.gold)
            .outline(th.outline_em.max(0.06), th.outline)
            .alpha(k.min(1.0))
            .middle()
            .draw(0.0, 0.0);
        gfx2d::pop();
        // The rows.
        let body = th.fit(th.body_font, 16.0);
        let num = th.fit(th.number_font, 24.0);
        let top = ty + 40.0;
        let row_h = 38.0;
        let rows_h = self.rows.len() as f32 * row_h + 16.0;
        // One panel under the rows, the rank and the teaser, so the theme's text reads on it.
        // Room for the teaser's line when it wraps.
        let more = self.teaser_line().map_or(0.0, |(size, _, h)| (h - size * 1.2).max(0.0));
        let extra = if self.rank.is_some() || self.next.is_some() { 186.0 + more } else { 0.0 };
        let panel = Rect::new(a.x + 4.0, top - 8.0, a.w - 8.0, rows_h + extra);
        // Ink that reads on the panel over the scrim (over a mid-dark scene), whatever the theme.
        let bg = over(th.panel_bottom, over(SCRIM, 0x404048ff));
        let ink = self.ink.unwrap_or_else(|| readable(th.text, bg, 4.5));
        let dim = readable(th.text_dim, bg, 3.0);
        let bad = readable(th.bad, bg, 3.0);
        let accent = readable(th.accent, bg, 3.0);
        if !self.rows.is_empty() {
            let pa = ((t - FIRST_ROW + 0.2) / 0.2).clamp(0.0, 1.0);
            rrect(panel, th.radius.min(14.0), fade(th.panel_bottom, pa));
        }
        for i in 0..self.rows.len() {
            let r = &self.rows[i];
            let st = self.row_start(i);
            if t < st {
                continue;
            }
            let e = Ease::CubicOut.at(((t - st) / 0.2).min(1.0));
            let y = top + i as f32 * row_h + row_h * 0.5;
            let dx = (1.0 - e) * 24.0;
            let col = if r.bad { bad } else { ink };
            text(&th.case(&r.label)).font(th.body_font).size(body).color(dim).tracking(0.08).alpha(e).valign(super::text::VAlign::Middle).draw(panel.x + 14.0 + dx, y);
            // A punch when the number lands.
            let since = t - st - ROLL;
            let punch = if (0.0..0.2).contains(&since) { 1.0 + 0.25 * (1.0 - since / 0.2) } else { 1.0 };
            gfx2d::push();
            gfx2d::translate(panel.right() - 14.0, y);
            gfx2d::scale(punch, punch);
            super::layouts::styled(th, &self.format(i, self.shown_value(i)), th.number_font, num, col)
                .align(Align::Right)
                .valign(super::text::VAlign::Middle)
                .alpha(e)
                .draw(0.0, 0.0);
            gfx2d::pop();
        }
        let below = panel.y + rows_h + 22.0;
        // The rank: stamped on, big, slightly turned, with a flash.
        if let Some(r) = &self.rank
            && self.shown_rank
        {
            let since = t - self.rank_at();
            let size = th.fit(th.title_font, 76.0);
            let (rx, ry) = (if self.next.is_some() { a.x + a.w * 0.28 } else { cx }, below + 54.0);
            let col = match r.as_str() {
                "S" => th.gold,
                "A" => th.good,
                "B" => th.accent,
                _ => dim,
            };
            let flash = (1.0 - since / 0.3).clamp(0.0, 1.0);
            if flash > 0.0 {
                disc(rx, ry, 30.0 + 50.0 * (1.0 - flash), fade(with_alpha(col, 0.5), flash));
            }
            ring(rx, ry, 44.0, 5.0, fade(col, (since / 0.15).min(1.0)));
            gfx2d::push();
            gfx2d::translate(rx, ry);
            gfx2d::rotate(-0.12);
            let s = self.stamp.value.max(0.0);
            gfx2d::scale(s, s);
            super::layouts::styled(th, r, th.title_font, size, col).outline(0.08, th.outline).middle().draw(0.0, 0.0);
            gfx2d::pop();
            text(&th.case("RANK")).font(th.body_font).size(th.fit(th.body_font, 13.0)).color(dim).tracking(0.2).align(Align::Center).draw(rx, ry + 56.0);
        }
        // The next opponent: slides in from the right edge of the card.
        let mut bottom = below;
        if let Some((name, line)) = &self.next
            && t >= self.teaser_at()
        {
            let k = Ease::BackOut.at(((t - self.teaser_at()) / 0.4).min(1.0));
            let w = if self.rank.is_some() { a.w * 0.5 } else { a.w * 0.7 };
            let x = a.right() - w + (1.0 - k) * (w + 40.0);
            let frame = Rect::new(x + (w - 96.0) / 2.0, below + 10.0, 96.0, 96.0);
            text(&th.case("NEXT")).font(th.body_font).size(th.fit(th.body_font, 13.0)).color(accent).tracking(0.2).align(Align::Center).draw(frame.cx(), below - 14.0);
            rrect(frame.inset(-3.0), 12.0, th.outline);
            rrect(frame, 10.0, with_alpha(th.panel_top, 0.95));
            teaser(frame, k);
            let name_size = th.fit(th.title_font, 22.0);
            // An outline that stands apart from the ink.
            let edge = if super::color::contrast(ink, th.outline) < 2.0 { readable(th.outline, ink, 3.0) } else { th.outline };
            text(name).font(th.title_font).size(name_size).color(ink).outline(0.08, edge).align(Align::Center).draw(frame.cx(), frame.bottom() + 8.0);
            bottom = frame.bottom() + 8.0 + name_size * 1.2;
            if let Some((size, room, lh)) = self.teaser_line() {
                let y = frame.bottom() + 34.0;
                text(line).font(th.body_font).size(size).color(dim).align(Align::Center).wrap(room).draw(frame.cx(), y);
                bottom = y + lh;
            }
        }
        // Once everything is in: the way on, under whatever the card shows.
        if self.all_shown() && self.out_at.is_none() {
            let pulse = 0.55 + 0.45 * ((t - self.shown_at()) * 4.0).sin().abs();
            text(&th.case("TAP TO CONTINUE")).font(th.body_font).size(th.fit(th.body_font, 14.0)).color(rgba(255, 255, 255, 235)).outline(0.1, SCRIM).alpha(pulse).align(Align::Center).draw(cx, prompt_y(a, self.screen, bottom));
        }
    }

    /// The teaser's line: its size (shrunk to fit its column, down to 11),
    /// the width it wraps at and its height.
    fn teaser_line(&self) -> Option<(f32, f32, f32)> {
        let (_, line) = self.next.as_ref().filter(|n| !n.1.is_empty())?;
        let th = &self.theme;
        let w = if self.rank.is_some() { self.area.w * 0.5 } else { self.area.w * 0.7 };
        let room = w - 12.0;
        let mut size = th.fit(th.body_font, 13.0);
        while size > 11.0 && text(line).font(th.body_font).size(size).measure().0 > room {
            size -= 1.0;
        }
        let (_, h) = text(line).font(th.body_font).size(size).wrap(room).measure();
        Some((size, room, h.max(size * 1.2)))
    }

    fn draw_wipe(&self) {
        let c = self.cover();
        if c <= 0.0 {
            return;
        }
        let th = &self.theme;
        let s = self.screen;
        let col = th.outline;
        match self.wipe {
            Wipe::Iris => {
                let far = s.w.hypot(s.h) * 0.5 + 4.0;
                let r = far * (1.0 - c);
                // A ring from the iris's edge out past the corners.
                let width = far - r + 4.0;
                ring(s.cx(), s.cy(), r + width / 2.0, width, col);
                if r > 2.0 {
                    ring(s.cx(), s.cy(), r, 5.0, th.accent);
                }
            }
            Wipe::Slide => {
                let slant = 80.0;
                let w = (s.w + slant) * c;
                // Closing from the right; opening off to the left.
                let (x0, x1) = if self.covered { (s.x, s.x + w) } else { (s.right() - w, s.right()) };
                let pts = [x0, s.y, x1 + slant, s.y, x1, s.bottom(), x0 - slant, s.bottom()];
                gfx2d::poly(&pts, col);
                let edge = if self.covered { x1 } else { x0 };
                gfx2d::line(edge, s.bottom(), edge + slant, s.y, 6.0, th.accent);
            }
            Wipe::Shutter => {
                let h = s.h * 0.5 * c;
                gfx2d::rect(s.x, s.y, s.w, h, col);
                gfx2d::rect(s.x, s.bottom() - h, s.w, h, col);
                gfx2d::rect(s.x, s.y + h - 3.0, s.w, 3.0, th.accent);
                gfx2d::rect(s.x, s.bottom() - h, s.w, 3.0, th.accent);
            }
        }
    }
}

/// Where "TAP TO CONTINUE" goes: near the card's bottom, or under the
/// content when that runs lower (a long teaser line), never off the screen.
fn prompt_y(card: Rect, screen: Rect, content_bottom: f32) -> f32 {
    (card.bottom() - 22.0).max(content_bottom + 10.0).min(screen.bottom() - 24.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Kind, synth};

    fn card() -> StatsCard {
        StatsCard::new("FIGHT 1 CLEAR", Theme::default(), &Layout::default()).percent("ACCURACY", 0.8).int("PARTS", 4).time("TIME", 61.5).rank("A").next("BOSS", "")
    }

    fn run(c: &mut StatsCard, secs: f32) -> Vec<(f32, StatsEvent)> {
        let mut out = Vec::new();
        for _ in 0..(secs * 60.0) as usize {
            for e in c.update(1.0 / 60.0) {
                out.push((c.age(), e));
            }
        }
        out
    }

    #[test]
    fn rows_roll_up_in_order_then_the_rank_stamps_then_the_teaser_then_the_wipe() {
        let mut c = card();
        let ev = run(&mut c, 12.0);
        let at = |want: &StatsEvent| ev.iter().find(|(_, e)| e == want).map(|(t, _)| *t).unwrap();
        assert!(at(&StatsEvent::Row(0)) < at(&StatsEvent::Landed(0)));
        assert!(at(&StatsEvent::Landed(0)) < at(&StatsEvent::Row(2)) || at(&StatsEvent::Row(1)) < at(&StatsEvent::Row(2)));
        assert!(at(&StatsEvent::Landed(2)) < at(&StatsEvent::Rank("A".into())));
        assert!(at(&StatsEvent::Rank("A".into())) < at(&StatsEvent::Teaser));
        assert!(at(&StatsEvent::Teaser) < at(&StatsEvent::Out));
        assert!(at(&StatsEvent::Out) < at(&StatsEvent::Covered) && at(&StatsEvent::Covered) < at(&StatsEvent::Done));
        assert!(ev.iter().filter(|(_, e)| *e == StatsEvent::Tick).count() > 20, "numbers tick as they roll");
        assert_eq!(ev.iter().filter(|(_, e)| *e == StatsEvent::Covered).count(), 1);
        assert!(c.finished() && c.covered());
        assert_eq!(c.format(2, 61.5), "1:01.5");
        assert_eq!(c.format(0, 0.8), "80%");
    }

    #[test]
    fn a_tap_reveals_everything_then_a_second_tap_leaves() {
        let mut c = card().manual();
        let tap = synth::tap(100.0, 100.0, 0.0)[0];
        assert_eq!(tap.kind(), Kind::TouchDown);
        assert!(!c.input(&tap), "the guard ignores a tap as the card appears");
        run(&mut c, 0.5);
        assert!(c.input(&tap) && c.all_shown() && !c.leaving());
        let ev = run(&mut c, 0.1);
        assert!(ev.iter().any(|(_, e)| *e == StatsEvent::Rank("A".into())) && ev.iter().any(|(_, e)| *e == StatsEvent::Teaser), "{ev:?}");
        assert!(ev.iter().any(|(_, e)| *e == StatsEvent::Landed(2)));
        // Manual: it waits for the second tap.
        run(&mut c, 10.0);
        assert!(!c.leaving());
        assert!(c.input(&tap));
        let ev = run(&mut c, 1.0);
        assert!(ev.iter().any(|(_, e)| *e == StatsEvent::Out) && c.finished());
    }

    #[test]
    fn the_ink_reads_on_every_theme() {
        use crate::ui::color::contrast;
        let mut swapped = Vec::new();
        for (name, th) in Theme::identities().into_iter().chain(Theme::presets()) {
            let bg = over(th.panel_bottom, over(SCRIM, 0x404048ff));
            let ink = readable(th.text, bg, 4.5);
            assert!(contrast(ink, bg) >= 4.5, "{name}");
            if ink != th.text {
                swapped.push(name);
            }
        }
        assert!(swapped.is_empty(), "the built-in themes read as they are: {swapped:?}");
        // A dark-ink theme on a dark panel (navy on navy) gets light numbers.
        let mut navy = Theme::default();
        (navy.text, navy.panel_bottom) = (0x1b2a4aff, 0x22305aee);
        let bg = over(navy.panel_bottom, over(SCRIM, 0x404048ff));
        assert_eq!(readable(navy.text, bg, 4.5), 0xf4f4f8ff);
    }

    #[test]
    fn the_prompt_keeps_below_the_teaser() {
        let card = Rect::new(0.0, 60.0, 360.0, 500.0);
        let screen = Rect::new(0.0, 0.0, 360.0, 640.0);
        assert_eq!(prompt_y(card, screen, 400.0), 538.0, "room: at the card's bottom");
        assert_eq!(prompt_y(card, screen, 545.0), 555.0, "a long line: under it");
        assert_eq!(prompt_y(card, screen, 700.0), 616.0, "never off the screen");
    }

    #[test]
    fn sounds_follow_the_events() {
        // Natively no sound plays; the hook still runs through every event.
        let mut c = card().sounds(CardSounds::new()).hold(0.0);
        let ev = run(&mut c, 12.0);
        assert!(c.finished() && ev.len() > 10);
        assert_eq!(CardSounds::load("assets/sfx").pending.len(), 8);
    }

    #[test]
    fn the_wipe_covers_then_uncovers_and_ranks_follow_the_score() {
        let mut c = card().hold(0.0);
        let s = c.shown_at() + 0.02;
        run(&mut c, s);
        assert!(c.leaving());
        let mut peak = 0.0f32;
        while !c.finished() {
            c.update(1.0 / 60.0);
            peak = peak.max(c.cover());
        }
        assert!(peak > 0.99 && c.cover() < 0.01);
        assert_eq!([0.95, 0.8, 0.6, 0.2].map(StatsCard::rank_for), ["S", "A", "B", "C"]);
    }
}
