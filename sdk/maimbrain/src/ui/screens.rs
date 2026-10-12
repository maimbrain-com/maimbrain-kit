//! Ready-made screens: a title card, a results (game-over) card, a play HUD
//! and a resume countdown. All themeable, all driven by `update(dt)`.
//!
//! They follow the feed's rules (SPEC §5.2, the maimbrain-game skill):
//! - A card takes no input, so the title card shows a hook and the best,
//!   never "tap to play"; the game starts on any tap.
//! - Key text and buttons stay inside `Layout::card`, clear of the
//!   platform's overlays (the title at the bottom, the buttons down the
//!   right); on the game-over card, inside `Layout::results`, which also
//!   keeps the band above the title clear for the platform's challenge
//!   strip after a round.
//! - The platform draws its own pause pill and leaderboards: nothing here
//!   duplicates them. Open the leaderboard with `store::show_scores` from a
//!   button of your own if you want one.

use super::color::{fade, lighten, with_alpha};
use super::icon::Icon;
use super::layout::{Layout, Rect};
use super::shape::{disc, sparkle};
use super::text::{VAlign, fmt_int, text};
use super::theme::Theme;
use super::widgets::{Button, ButtonKind, Tap, ribbon};
use crate::gfx2d;
use crate::input::Event;
use crate::juice::Particles;
use crate::motion::{Counter, Ease, Pulse, Spring};

/// Draws `s` letter by letter around centre (cx, cy): each letter drops in
/// (staggered) and then bobs. `t` is seconds since the entrance began.
#[allow(clippy::too_many_arguments)]
pub fn bouncy_title(s: &str, cx: f32, cy: f32, size: f32, t: f32, theme: &Theme, color: u32, max_w: f32) -> Rect {
    let font = theme.title_font;
    let mut size = theme.fit(font, size);
    let mut buf = [0u8; 4];
    let measure = |size: f32| -> (Vec<f32>, f32) {
        let mut b = [0u8; 4];
        let ws: Vec<f32> = s.chars().map(|c| gfx2d::measure(font, size, c.encode_utf8(&mut b))).collect();
        let total = ws.iter().sum::<f32>() + size * (0.03 + theme.style.tracking) * ws.len().saturating_sub(1) as f32;
        (ws, total)
    };
    let (mut ws, mut total) = measure(size);
    if total > max_w && total > 0.0 {
        size = theme.fit(font, size * max_w / total);
        if font.is_bitmap() && size * total / theme.fit(font, size) > max_w {
            size = (size - 8.0).max(8.0);
        }
        (ws, total) = measure(size);
    }
    let m = super::text::metrics(font);
    let motion = theme.style.motion;
    let mut x = cx - total / 2.0;
    for (i, c) in s.chars().enumerate() {
        let w = ws[i];
        let k = motion.progress(t, i);
        if k > 0.0 {
            let drop = motion.enter(k);
            let (dx, dy, tilt) = motion.idle(t, i);
            let lx = x + w / 2.0 + dx * size * k;
            let ly = cy - (1.0 - drop) * size * 0.9 + dy * size * k;
            gfx2d::push();
            gfx2d::translate(lx, ly);
            gfx2d::rotate(tilt);
            let sc = 0.4 + 0.6 * drop;
            gfx2d::scale(sc, sc);
            let a = (k * 3.0).min(1.0);
            super::layouts::styled(theme, c.encode_utf8(&mut buf), font, size, color).tracking(0.0).alpha(a).middle().draw(0.0, 0.0);
            gfx2d::pop();
        }
        x += w + size * (0.03 + theme.style.tracking);
    }
    Rect::centered(cx, cy, total, m.cap * size)
}

/// Twinkles that come and go around a rect (no state: a function of `t`).
pub fn twinkles(r: Rect, t: f32, color: u32) {
    let spots = [(-0.06, 0.1, 0.0), (1.04, -0.15, 1.7), (0.9, 1.25, 3.1), (0.12, 1.3, 4.4)];
    for (i, (fx, fy, ph)) in spots.iter().enumerate() {
        let k = ((t * 1.6 + ph).sin() * 0.5 + 0.5).powi(6);
        if k > 0.02 {
            sparkle(r.x + r.w * fx, r.y + r.h * fy, 4.0 + 9.0 * k, t * 0.6 + i as f32, fade(color, k));
        }
    }
}

/// The title card: the game's name (letters drop in, then bob), a tagline,
/// and the player's best. Draw it over your live scene; it leaves the
/// middle of the screen to the game.
/// ```ignore
/// let mut title = TitleCard::new("STACK", Theme::candy()).tagline("how high can you go?").best(best);
/// title.update(dt);          // in update
/// title.draw(&layout);       // in render, over the scene
/// ```
#[derive(Clone, Debug)]
pub struct TitleCard {
    pub title: String,
    pub tagline: Option<String>,
    pub best: Option<i64>,
    pub best_label: String,
    pub theme: Theme,
    /// The title's centre line; default 18% down the screen (below the safe top).
    pub y: Option<f32>,
    pub size: f32,
    pub color: Option<u32>,
    t: f32,
}

impl TitleCard {
    pub fn new(title: &str, theme: Theme) -> TitleCard {
        TitleCard { title: title.to_string(), tagline: None, best: None, best_label: "BEST".into(), theme, y: None, size: 64.0, color: None, t: 0.0 }
    }
    pub fn tagline(mut self, s: &str) -> Self {
        self.tagline = Some(s.to_string());
        self
    }
    /// The best to brag about (0 or less shows none).
    pub fn best(mut self, b: i64) -> Self {
        self.set_best(b);
        self
    }
    pub fn set_best(&mut self, b: i64) {
        self.best = (b > 0).then_some(b);
    }
    pub fn at(mut self, y: f32) -> Self {
        self.y = Some(y);
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    /// Plays the entrance again (when coming back to the title).
    pub fn restart(&mut self) {
        self.t = 0.0;
    }
    pub fn update(&mut self, dt: f32) {
        self.t += dt;
    }
    /// Seconds since the entrance began: time your own title's animation by
    /// it (`theme.style.motion.progress(card.age(), i)` for letter i).
    pub fn age(&self) -> f32 {
        self.t
    }
    pub fn draw(&self, layout: &Layout) {
        let th = &self.theme;
        let y = self.y.unwrap_or(layout.safe.y + 40.0 + self.size * 0.6);
        let args = super::layouts::TitleArgs {
            title: &self.title,
            tagline: self.tagline.as_deref(),
            best: self.best.map(|b| (self.best_label.as_str(), b)),
            size: self.size,
            y,
            color: self.color.unwrap_or(th.text),
            t: self.t,
        };
        super::layouts::title(th, layout, &args);
    }
}

/// What a results card wants the game to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultsAction {
    Retry,
    /// The optional extra button (e.g. open the leaderboard, switch mode).
    Extra,
}

/// Moments to play a sound or haptic for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultsEvent {
    /// The rolling score changed (a tick).
    Tick,
    /// The roll-up landed on a new best: confetti is flying.
    NewBest,
    /// The roll-up finished (not a new best).
    Landed,
}

/// The game-over card: a heading, the score rolling up, the best (or a
/// "NEW BEST!" ribbon with confetti), how close the player came, a retry
/// button and an optional extra one.
///
/// It's a feed card too: a brag and a dare. The whole screen retries (after
/// `RETRY_GUARD`): feed every event to `input`, which keeps that rule.
///
/// To draw your own card from your world, build it `.state_only()`: it keeps
/// the roll-up, the new-best moment and the retry rule, and you draw from
/// `shown_score`, `best`, `new_best_shown`, `stamp`, `gap_to_best`…, with
/// your text and buttons inside `Layout::results`.
#[derive(Clone, Debug)]
pub struct ResultsCard {
    pub heading: String,
    /// What the score counts ("HEIGHT", "POINTS"); empty for none.
    pub label: String,
    pub score: i64,
    pub prev_best: i64,
    pub theme: Theme,
    pub retry: Option<Button>,
    pub extra: Option<Button>,
    /// Draw on a panel (default) or float the text over the scene.
    pub panel: bool,
    /// Lower scores are better (times).
    pub lower_is_better: bool,
    area: Rect,
    /// Where the card lays out at full size, and the scale that fits it in
    /// `area`: 1 (and `area`) unless the area is too short for it, as on a
    /// landscape screen, where the whole card shrinks about its top center.
    virt: Rect,
    fit: f32,
    counter: Counter,
    t: f32,
    fx: Particles,
    stamp: Spring,
    flash: Pulse,
    /// The card's age when the roll-up landed.
    landed_at: Option<f32>,
    state_only: bool,
}

impl ResultsCard {
    /// Seconds after the card appears before a tap retries: a frantic tap
    /// during the failure shouldn't start the next round.
    pub const RETRY_GUARD: f32 = 0.35;

    /// `prev_best` is the best before this round (0 = none yet). Seed the
    /// confetti from your game's `Rng`.
    pub fn new(score: i64, prev_best: i64, theme: Theme, layout: &Layout, seed: u64) -> ResultsCard {
        let mut c = ResultsCard {
            heading: "GAME OVER".into(),
            label: String::new(),
            score,
            prev_best,
            theme,
            retry: None,
            extra: None,
            panel: true,
            lower_is_better: false,
            area: layout.results,
            virt: layout.results,
            fit: 1.0,
            counter: Counter::new(0),
            t: 0.0,
            fx: Particles::new(seed),
            stamp: Spring::new(0.0, 3.2, 0.32),
            flash: Pulse::new(0.6),
            landed_at: None,
            state_only: false,
        };
        let dur = (0.5 + 0.25 * (score.max(1) as f32).log10()).min(1.4);
        c.counter.roll_to(score, dur);
        c.place();
        c
    }
    pub fn heading(mut self, s: &str) -> Self {
        self.heading = s.to_string();
        self
    }
    pub fn label(mut self, s: &str) -> Self {
        self.label = s.to_string();
        self
    }
    /// An extra button under retry, e.g. `Button::new(r, "SCORES").kind(ButtonKind::Secondary)`
    /// (its rect is replaced to fit the card).
    pub fn extra(mut self, b: Button) -> Self {
        self.extra = Some(b);
        self.place();
        self
    }
    /// No panel: big outlined text over the scene, near the top.
    pub fn floating(mut self) -> Self {
        self.panel = false;
        self.place();
        self
    }
    pub fn lower_is_better(mut self) -> Self {
        self.lower_is_better = true;
        self
    }
    pub fn no_retry_button(mut self) -> Self {
        self.retry = None;
        self
    }
    /// State and input only: `draw` draws nothing but the new-best confetti,
    /// and there's no retry button (any tap retries after the guard). Draw
    /// the card yourself from its state; give it buttons of your own with
    /// `.extra(..)` or by setting `retry`, at the rects you draw them.
    pub fn state_only(mut self) -> Self {
        self.state_only = true;
        self.retry = None;
        self
    }

    /// The panel's rect (or the floating block's), on the screen.
    pub fn panel_rect(&self) -> Rect {
        let p = self.full_panel();
        let (x, y) = self.to_screen(p.x, p.y);
        Rect::new(x, y, p.w * self.fit, p.h * self.fit)
    }

    /// The screen changed (`input::Kind::Resize`): lay the card out again in it.
    pub fn relayout(&mut self, layout: &Layout) {
        self.area = layout.results;
        self.place();
    }

    /// The panel's rect at full size, in `virt`.
    fn full_panel(&self) -> Rect {
        super::layouts::results_rect(&self.layout_theme(), self.virt)
    }

    /// A point of the full-size card, on the screen.
    fn to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (self.area.cx() + (x - self.virt.cx()) * self.fit, self.area.y + (y - self.virt.y) * self.fit)
    }

    /// Fits the card to `area`: the full-size card reaches down to its retry
    /// button (and an extra button under it); an area too short for that
    /// gets the card scaled down to fit.
    fn fit_area(&mut self) {
        let bottom = |c: &Self, area: Rect| {
            let lt = c.layout_theme();
            let p = super::layouts::results_rect(&lt, area);
            let (_, cy) = super::layouts::retry_center(&lt, p);
            p.bottom().max(cy + 34.0).max(if c.extra.is_some() { cy + 100.0 } else { 0.0 })
        };
        self.virt = self.area;
        self.fit = 1.0;
        let need = bottom(self, self.area) - self.area.y;
        if need > self.area.h && self.area.h > 0.0 {
            let k = (self.area.h / need).max(0.4);
            self.virt = Rect::new(self.area.x, self.area.y, self.area.w, self.area.h / k);
            self.fit = k;
        }
    }

    /// The theme with `floating()` applied to its results layout.
    fn layout_theme(&self) -> Theme {
        let mut t = self.theme;
        if !self.panel {
            t.style.results = super::style::ResultsLayout::Floating;
        }
        t
    }

    fn place(&mut self) {
        self.fit_area();
        if self.state_only {
            return;
        }
        let k = self.fit;
        let p = self.full_panel();
        let (cx, cy) = super::layouts::retry_center(&self.layout_theme(), p);
        let (rx, ry) = self.to_screen(cx, cy);
        let mut retry = Button::icon(rx, ry, 68.0 * k, Icon::RESTART);
        if let Some(old) = &self.retry {
            retry.haptic = old.haptic;
        }
        self.retry = Some(retry);
        let w = 150.0;
        let (ex, ey) = self.to_screen(cx.max(p.x + w / 2.0), cy + 34.0 + 18.0 + 26.0);
        if let Some(e) = &mut self.extra {
            e.rect = Rect::centered(ex, ey, w * k, 44.0 * k);
        }
    }

    /// Beat a previous best (a first-ever score shows as the best, without
    /// the celebration: there was nothing to beat).
    pub fn is_new_best(&self) -> bool {
        if self.prev_best <= 0 || self.score <= 0 {
            false
        } else if self.lower_is_better {
            self.score < self.prev_best
        } else {
            self.score > self.prev_best
        }
    }

    /// The roll-up and its celebration are over.
    pub fn finished(&self) -> bool {
        self.landed_at.is_some()
    }

    /// Seconds since the card appeared.
    pub fn age(&self) -> f32 {
        self.t
    }

    /// The score as shown now: it rolls up from 0 once the card has landed.
    pub fn shown_score(&self) -> i64 {
        self.counter.value()
    }

    /// The shown score's scale: a punch when the roll-up lands (1 at rest).
    pub fn score_scale(&self) -> f32 {
        self.counter.scale()
    }

    /// The best after this round (what a BEST line shows).
    pub fn best(&self) -> i64 {
        if !self.lower_is_better {
            self.prev_best.max(self.score)
        } else if self.prev_best > 0 {
            self.prev_best.min(self.score.max(1))
        } else {
            self.score
        }
    }

    /// The card's age when the roll-up landed (`ResultsEvent::Landed` or `NewBest`).
    pub fn landed_at(&self) -> Option<f32> {
        self.landed_at
    }

    /// The card's age when the new-best celebration began, if there is one.
    pub fn new_best_at(&self) -> Option<f32> {
        self.landed_at.filter(|_| self.is_new_best())
    }

    /// True from the moment the roll-up lands on a new best: show the
    /// NEW BEST ribbon (scaled by `stamp`) instead of the best.
    pub fn new_best_shown(&self) -> bool {
        self.new_best_at().is_some()
    }

    /// The NEW BEST ribbon's scale: springs 0 → 1 (overshooting) once it's shown.
    pub fn stamp(&self) -> f32 {
        self.stamp.value
    }

    /// The gold rim's flash as the new best lands (1 → 0).
    pub fn flash(&self) -> f32 {
        if self.flash.active() { self.flash.value() } else { 0.0 }
    }

    /// How many more would have beaten the best, once landed, when it was
    /// close enough to dare ("3 MORE TO BEAT IT").
    pub fn gap_to_best(&self) -> Option<i64> {
        let gap = self.prev_best - self.score;
        let close = !self.lower_is_better && gap > 0 && (gap as f32) <= (self.prev_best as f32 * 0.25).max(3.0);
        (self.finished() && close).then_some(gap)
    }

    /// True once a tap would retry (after `RETRY_GUARD`).
    pub fn retry_ready(&self) -> bool {
        self.t > Self::RETRY_GUARD
    }

    /// The card's whole input rule: its buttons, and once `retry_ready`, a
    /// press anywhere else retries, as does a controller's A, Enter or Space
    /// (`Event::is_confirm`). Feed it every event while it's up.
    pub fn input(&mut self, e: &Event) -> Option<ResultsAction> {
        if !self.retry_ready() {
            return None;
        }
        if let Some(a) = self.handle(e) {
            return Some(a);
        }
        ((e.is_press() && !self.claims(e.x, e.y)) || e.is_confirm()).then_some(ResultsAction::Retry)
    }

    /// Feeds an input event to the buttons (only: `input` adds the
    /// whole-screen retry and the guard).
    pub fn handle(&mut self, e: &Event) -> Option<ResultsAction> {
        if self.t < 0.3 {
            return None;
        }
        if let Some(b) = &mut self.retry
            && b.handle(e) == Some(Tap::Clicked)
        {
            return Some(ResultsAction::Retry);
        }
        if let Some(b) = &mut self.extra
            && b.handle(e) == Some(Tap::Clicked)
        {
            return Some(ResultsAction::Extra);
        }
        None
    }

    /// True if `e` landed on one of the card's buttons (so it isn't a
    /// whole-screen retry tap).
    pub fn claims(&self, x: f32, y: f32) -> bool {
        self.retry.as_ref().is_some_and(|b| b.hit_rect().contains(x, y)) || self.extra.as_ref().is_some_and(|b| b.hit_rect().contains(x, y))
    }

    pub fn update(&mut self, dt: f32) -> Option<ResultsEvent> {
        self.t += dt;
        self.fx.update(dt);
        self.stamp.update(dt);
        self.flash.update(dt);
        for b in self.retry.iter_mut().chain(self.extra.iter_mut()) {
            b.update(dt);
        }
        // The number starts rolling once the panel has landed.
        let mut ev = None;
        if self.t > 0.35 && self.counter.update(dt) {
            ev = Some(ResultsEvent::Tick);
        }
        if self.landed_at.is_none() && self.t > 0.35 && self.counter.done() {
            self.landed_at = Some(self.t);
            self.counter.punch.kick(0.35);
            if self.is_new_best() {
                self.stamp.target = 1.0;
                self.flash.fire();
                let p = self.full_panel();
                let (fx, fy) = self.to_screen(p.cx(), p.y + 100.0);
                self.fx.celebrate(fx, fy, &self.theme);
                self.fx.confetti_cannons(Rect::new(self.area.x - 40.0, self.area.y, self.area.w + 80.0, self.area.h + 40.0), 60, &self.theme.confetti);
                ev = Some(ResultsEvent::NewBest);
            } else {
                ev = Some(ResultsEvent::Landed);
            }
        }
        ev
    }

    /// Draws the card in the theme's results layout (only the confetti when
    /// `state_only`).
    pub fn draw(&self) {
        if self.state_only {
            self.fx.draw();
            return;
        }
        let th = &self.theme;
        let lt = self.layout_theme();
        if !matches!(lt.style.results, super::style::ResultsLayout::Panel | super::style::ResultsLayout::Floating) {
            let best = if self.lower_is_better || self.finished() { self.best() } else { self.prev_best };
            let a = super::layouts::ResultsArgs {
                heading: &self.heading,
                label: &self.label,
                shown: self.counter.value(),
                punch: self.counter.scale(),
                best,
                new_best: self.is_new_best(),
                landed: self.finished(),
                stamp: self.stamp.value,
                t: self.t,
                area: self.virt,
            };
            self.push_fit();
            super::layouts::results(&lt, &a);
            gfx2d::pop();
            let ba = ((self.t - 0.6) / 0.3).clamp(0.0, 1.0);
            for b in self.retry.iter().chain(self.extra.iter()) {
                if ba > 0.0 {
                    b.draw_alpha(th, ba);
                }
            }
            self.fx.draw();
            return;
        }
        let p = self.full_panel();
        let panel = lt.style.results == super::style::ResultsLayout::Panel;
        self.push_fit();
        let enter = th.style.motion.enter((self.t / th.style.motion.duration().max(0.3)).min(1.0));
        let a = (self.t / 0.2).min(1.0);
        gfx2d::push();
        gfx2d::translate(p.cx(), p.cy());
        let s = 0.82 + 0.18 * enter;
        gfx2d::scale(s, s);
        gfx2d::translate(-p.cx(), -p.cy() + (1.0 - enter) * 30.0);
        if panel {
            super::widgets::panel(p, &with_alpha_theme(th, a));
            if self.flash.active() {
                // A bright rim when a new best lands.
                gfx2d::rrect(p.x - 3.0, p.y - 3.0, p.w + 6.0, p.h + 6.0, th.radius + 3.0, 4.0, 6.0, fade(th.gold, self.flash.value()), fade(th.gold, self.flash.value()));
            }
        }
        let cx = p.cx();
        // Heading straddling the panel's top edge.
        let hsize = th.fit(th.title_font, 38.0);
        let head_y = if panel { p.y } else { p.y + 10.0 };
        let heading = th.case(&self.heading);
        let ht = super::layouts::styled(th, &heading, th.title_font, hsize, th.text);
        let ht = if panel || th.style.outlined { ht.outline(th.outline_em.max(0.04), th.outline) } else { ht };
        ht.alpha(a).middle().draw(cx, head_y);
        let mut y = head_y + hsize * 0.9;
        if !self.label.is_empty() {
            let ls = th.fit(th.body_font, 15.0);
            let l = text(&self.label).font(th.body_font).size(ls).color(th.text_dim).tracking(0.12);
            // Over a busy scene the label needs an edge.
            let l = if panel { l } else { l.outline(0.1, with_alpha(th.outline, 0.7)) };
            l.alpha(a).middle().draw(cx, y);
            y += ls * 0.9;
        }
        // The score, rolling up, punched when it lands.
        let nsize = th.fit(th.number_font, 76.0);
        let ny = y + nsize * 0.62;
        gfx2d::push();
        gfx2d::translate(cx, ny);
        let k = self.counter.scale();
        gfx2d::scale(k, k);
        let col = if self.new_best_shown() { th.gold } else { th.text };
        super::layouts::styled(th, &fmt_int(self.counter.value()), th.number_font, nsize, col).tracking(0.0).alpha(a).middle().draw(0.0, 0.0);
        gfx2d::pop();
        let row = ny + nsize * 0.62;
        // Best, or the new-best ribbon.
        if self.new_best_shown() {
            let st = self.stamp.value;
            if st > 0.01 {
                gfx2d::push();
                gfx2d::translate(cx, row + 16.0);
                gfx2d::scale(st, st);
                ribbon("NEW BEST!", 0.0, 0.0, 40.0, th.gold, th.outline, th.title_font, -0.05);
                gfx2d::pop();
            }
        } else {
            let best = self.best();
            let k = ((self.t - 0.5) / 0.3).clamp(0.0, 1.0);
            if best > 0 && k > 0.0 {
                let bg = with_alpha(th.outline, 0.45 * k);
                super::widgets::pill_in(th, &format!("{} {}", th.case("BEST"), fmt_int(best)), cx, row + 6.0, 32.0, bg, fade(th.gold, k), Some(Icon::TROPHY));
                // A dare when it was close.
                if let Some(gap) = self.gap_to_best() {
                    let ds = th.fit(th.body_font, 15.0);
                    text(&format!("{} MORE TO BEAT IT", fmt_int(gap)))
                        .font(th.body_font)
                        .size(ds)
                        .color(th.text)
                        .alpha(k)
                        .outline(0.06, with_alpha(th.outline, 0.7))
                        .middle()
                        .draw(cx, row + 40.0);
                }
            }
        }
        gfx2d::pop();
        gfx2d::pop();
        let ba = ((self.t - 0.6) / 0.3).clamp(0.0, 1.0);
        for b in self.retry.iter().chain(self.extra.iter()) {
            if ba > 0.0 {
                b.draw_alpha(th, ba);
            }
        }
        self.fx.draw();
    }
}

impl ResultsCard {
    /// Draws what follows as the full-size card (`virt`), fitted to `area`.
    fn push_fit(&self) {
        gfx2d::push();
        gfx2d::translate(self.area.cx(), self.area.y);
        gfx2d::scale(self.fit, self.fit);
        gfx2d::translate(-self.virt.cx(), -self.virt.y);
    }
}

fn with_alpha_theme(th: &Theme, a: f32) -> Theme {
    let mut t = *th;
    t.panel_top = fade(t.panel_top, a);
    t.panel_bottom = fade(t.panel_bottom, a);
    t.panel_border = fade(t.panel_border, a);
    t.shadow = fade(t.shadow, a);
    t
}

/// The play HUD: the score big at the top (rolling and punching when it
/// changes), the best under it until it's beaten, then a gold "NEW BEST"
/// marker. Optional lives (hearts) and a progress bar. Its animations run on
/// `update(dt)` only, so they freeze with the game when the platform pauses
/// it, and it stays clear of the pause pill (top-left).
#[derive(Clone, Debug)]
pub struct Hud {
    pub score: Counter,
    pub best: i64,
    pub theme: Theme,
    /// (lives left, max), drawn as hearts top-right.
    pub lives: Option<(u32, u32)>,
    /// A progress bar under the score (level progress, time left).
    pub progress: Option<f32>,
    pub size: f32,
    beaten: Pulse,
    beat: bool,
}

impl Hud {
    pub fn new(theme: Theme, best: i64) -> Hud {
        Hud { score: Counter::new(0), best, theme, lives: None, progress: None, size: 56.0, beaten: Pulse::new(0.8), beat: false }
    }
    /// A new round: score back to 0.
    pub fn reset(&mut self, best: i64) {
        self.score.snap(0);
        self.best = best;
        self.beat = false;
    }
    pub fn set_score(&mut self, v: i64) {
        self.score.set(v);
        if !self.beat && self.best > 0 && v > self.best {
            self.beat = true;
            self.beaten.fire();
            self.score.punch.kick(0.5);
        }
    }
    /// Advances; true if the shown score changed (tick).
    pub fn update(&mut self, dt: f32) -> bool {
        self.beaten.update(dt);
        self.score.update(dt)
    }
    pub fn beat_best(&self) -> bool {
        self.beat
    }
    pub fn draw(&self, layout: &Layout) {
        let th = &self.theme;
        let col = if self.beat { lighten(th.gold, 0.15 * self.beaten.value()) } else { th.text };
        let (cx, mut below, align) = super::layouts::hud_score(th, layout, &fmt_int(self.score.value()), self.size, self.score.scale(), col);
        let small = th.fit(th.body_font, 14.0);
        if self.beat {
            let s = 1.0 + 0.25 * self.beaten.value();
            let label = th.case("NEW BEST");
            let w = super::widgets::pill_width(&label, 24.0, th.body_font, true);
            let px = match align {
                super::text::Align::Right => cx - w / 2.0,
                _ => cx,
            };
            gfx2d::push();
            gfx2d::translate(px, below + 10.0);
            gfx2d::scale(s, s);
            super::widgets::pill_in(th, &label, 0.0, 0.0, 24.0, with_alpha(th.outline, 0.5), th.gold, Some(Icon::CROWN));
            gfx2d::pop();
            below += 26.0;
        } else if self.best > 0 {
            text(&format!("{} {}", th.case("BEST"), fmt_int(self.best))).font(th.body_font).size(small).color(th.text_dim).outline(0.08, with_alpha(th.outline, 0.6)).align(align).valign(VAlign::Middle).draw(cx, below + 6.0);
            below += 20.0;
        }
        if let Some(p) = self.progress {
            let bx = if align == super::text::Align::Right { cx - 80.0 } else { cx };
            super::widgets::progress_bar(Rect::centered(bx, below + 8.0, 160.0, 10.0), p, th.accent, with_alpha(th.outline, 0.45));
        }
        if let Some((n, max)) = self.lives {
            let hs = 22.0;
            // Top-right, or (when the score sits there) just right of the pause pill.
            let corner = th.style.hud == super::style::HudLayout::Corner;
            let right = if corner { layout.pill_zone().right() + 16.0 + max as f32 * (hs + 4.0) } else { layout.hud.right() - 16.0 };
            for i in 0..max {
                let x = right - (max - 1 - i) as f32 * (hs + 4.0) - hs / 2.0;
                let on = i < n;
                Icon::HEART.draw(x, layout.hud.y + 26.0 + 1.5, hs + 3.0, with_alpha(th.outline, 0.7));
                Icon::HEART.draw(x, layout.hud.y + 26.0, hs, if on { th.bad } else { with_alpha(th.text, 0.25) });
            }
        }
    }
}

/// "3, 2, 1, GO!" for a run-up, e.g. after `Game::resume` (a recorded
/// lifecycle call, so it replays): skip your simulation while `active()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Countdown {
    left: f32,
    from: u32,
    last: u32,
}

impl Countdown {
    pub fn start(&mut self, from: u32) {
        self.from = from;
        self.left = from as f32 + 0.5;
        self.last = from + 1;
    }
    pub fn active(&self) -> bool {
        self.left > 0.5
    }
    pub fn showing(&self) -> bool {
        self.left > 0.0
    }
    /// Advances; returns the number just shown (0 = "GO").
    pub fn update(&mut self, dt: f32) -> Option<u32> {
        if self.left <= 0.0 {
            return None;
        }
        self.left -= dt;
        let n = (self.left - 0.5).ceil().max(0.0) as u32;
        if n < self.last && self.left > 0.0 {
            self.last = n;
            return Some(n);
        }
        None
    }
    pub fn draw(&self, layout: &Layout, theme: &Theme) {
        if self.left <= 0.0 {
            return;
        }
        let n = (self.left - 0.5).ceil().max(0.0) as u32;
        let frac = (self.left - 0.5) - (n as f32 - 1.0);
        let phase = if n == 0 { 1.0 - self.left / 0.5 } else { 1.0 - frac };
        let s = 1.6 - 0.6 * Ease::BackOut.at((phase * 3.0).min(1.0));
        let a = if phase > 0.75 { 1.0 - (phase - 0.75) / 0.25 } else { 1.0 };
        let (cx, cy) = (layout.screen.cx(), layout.screen.cy());
        disc(cx, cy, 70.0, with_alpha(theme.outline, 0.35 * a));
        super::widgets::ring_meter(cx, cy, 70.0, 6.0, if n == 0 { 1.0 } else { 1.0 - phase }, fade(theme.accent, a), fade(with_alpha(theme.text, 0.15), a));
        gfx2d::push();
        gfx2d::translate(cx, cy);
        gfx2d::scale(s, s);
        let label = if n == 0 { "GO!".to_string() } else { n.to_string() };
        let size = theme.fit(theme.number_font, if n == 0 { 48.0 } else { 72.0 });
        text(&label).font(theme.number_font).size(size).color(fade(theme.text, a)).outline(theme.outline_em, fade(theme.outline, a)).valign(VAlign::Middle).center().draw(0.0, 0.0);
        gfx2d::pop();
    }
}

/// A round restart icon button sized for thumbs, in the kit's style.
pub fn retry_button(cx: f32, cy: f32) -> Button {
    Button::icon(cx, cy, 68.0, Icon::RESTART)
}

/// A secondary text button.
pub fn secondary_button(r: Rect, label: &str) -> Button {
    Button::new(r, label).kind(ButtonKind::Secondary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_roll_up_then_celebrate_a_new_best() {
        let layout = Layout::default();
        let mut r = ResultsCard::new(120, 100, Theme::candy(), &layout, 1);
        assert!(r.is_new_best());
        let mut events = Vec::new();
        for _ in 0..240 {
            if let Some(e) = r.update(1.0 / 60.0) {
                events.push(e);
            }
        }
        assert!(events.contains(&ResultsEvent::Tick));
        assert_eq!(events.iter().filter(|e| **e == ResultsEvent::NewBest).count(), 1);
        assert!(r.finished());
        // The retry button sits inside the card area, clear of the overlays
        // and the challenge strip, and so does an extra button under it.
        let b = r.retry.as_ref().unwrap().rect;
        assert!(b.bottom() <= layout.results.bottom() && b.x >= layout.results.x);
        let r3 = ResultsCard::new(50, 100, Theme::night(), &layout, 1).extra(Button::new(Rect::default(), "SCORES"));
        let e = r3.extra.as_ref().unwrap().rect;
        assert!(e.bottom() <= layout.results.bottom() && e.x >= layout.results.x, "{e:?}");
        let r2 = ResultsCard::new(50, 100, Theme::night(), &layout, 1);
        assert!(!r2.is_new_best());
    }

    #[test]
    fn a_turned_results_card_still_keeps_the_strip_clear() {
        // A landscape game's game-over card turns from the vertical feed card to
        // landscape and back (a Resize): its buttons stay out of the strip's band.
        let screen = |width, height| crate::sys::Screen { width, height, inset_bottom: 20.0, dpr: 2.0, ..Default::default() };
        let mut r = ResultsCard::new(50, 100, Theme::night(), &Layout::from_screen(&screen(360.0, 640.0)), 1);
        for (w, h) in [(640.0, 360.0), (360.0, 640.0)] {
            let layout = Layout::from_screen(&screen(w, h));
            r.relayout(&layout);
            let (b, p) = (r.retry.as_ref().unwrap().rect, r.panel_rect());
            assert!(b.bottom() <= layout.results.bottom() + 0.01 && b.right() <= layout.results.right(), "{w}×{h}: {b:?}");
            assert!(p.y >= layout.results.y && p.bottom() <= layout.results.bottom() + 0.01 && p.x >= layout.results.x, "{w}×{h}: {p:?}");
            // Upright, the card has room: full size, as before.
            assert_eq!(r.fit == 1.0, h > w, "{w}×{h}: scaled {}", r.fit);
        }
    }

    #[test]
    fn a_state_only_card_keeps_the_rules_for_a_card_you_draw() {
        use crate::input::synth;
        let layout = Layout::default();
        let mut r = ResultsCard::new(120, 100, Theme::candy(), &layout, 1).state_only();
        assert!(r.retry.is_none() && r.shown_score() == 0 && !r.new_best_shown());
        // A frantic tap during the failure doesn't retry; one after the guard does, anywhere.
        r.update(0.2);
        assert_eq!(synth::tap(180.0, 300.0, 0.2).iter().find_map(|e| r.input(e)), None);
        let mut landed = None;
        for _ in 0..240 {
            if r.update(1.0 / 60.0) == Some(ResultsEvent::NewBest) {
                landed = Some(r.age());
            }
        }
        assert!(r.retry_ready() && r.shown_score() == 120 && r.best() == 120);
        assert_eq!(r.new_best_at(), landed);
        assert!(r.new_best_shown() && r.stamp() > 0.9);
        assert_eq!(synth::tap(180.0, 300.0, 4.0).iter().find_map(|e| r.input(e)), Some(ResultsAction::Retry));
        // Close to the best: the dare's gap. Far off, none.
        let mut close = ResultsCard::new(97, 100, Theme::candy(), &layout, 1).state_only();
        let mut far = ResultsCard::new(20, 100, Theme::candy(), &layout, 1).state_only();
        for _ in 0..240 {
            close.update(1.0 / 60.0);
            far.update(1.0 / 60.0);
        }
        assert_eq!((close.gap_to_best(), far.gap_to_best(), close.new_best_at()), (Some(3), None, None));
        assert!(close.landed_at().is_some() && close.best() == 100);
    }

    #[test]
    fn the_drawn_card_keeps_its_buttons_out_of_the_whole_screen_retry() {
        let layout = Layout::default();
        let mut r = ResultsCard::new(5, 10, Theme::candy(), &layout, 1);
        r.update(0.5);
        let b = r.retry.as_ref().unwrap().rect;
        let press = |x, y| crate::input::synth::touch(crate::input::Kind::TouchDown, x, y, 0.5);
        // A press on the retry button is the button's (it retries on release), not a screen tap.
        assert_eq!(r.input(&press(b.cx(), b.cy())), None);
        assert_eq!(r.input(&press(b.cx(), b.y - 120.0)), Some(ResultsAction::Retry));
    }

    #[test]
    fn countdown_counts_down_to_go() {
        let mut c = Countdown::default();
        c.start(3);
        let mut seen = Vec::new();
        for _ in 0..300 {
            if let Some(n) = c.update(1.0 / 60.0) {
                seen.push(n);
            }
        }
        assert_eq!(seen, vec![3, 2, 1, 0]);
        assert!(!c.active() && !c.showing());
    }

    #[test]
    fn hud_marks_a_beaten_best_once() {
        let mut h = Hud::new(Theme::candy(), 10);
        h.set_score(5);
        assert!(!h.beat_best());
        h.set_score(11);
        assert!(h.beat_best());
        h.reset(11);
        assert!(!h.beat_best());
    }

    #[test]
    fn twinkle_positions_are_finite() {
        twinkles(Rect::new(0.0, 0.0, 10.0, 10.0), 1.0, 0xffffffff);
    }
}
