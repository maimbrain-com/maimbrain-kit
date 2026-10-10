//! UI Gallery: every widget, icon, text style, easing curve and juice
//! effect in `maimbrain::ui`, `maimbrain::motion` and `maimbrain::juice`,
//! one page at a time. Not a feed game: a living reference (docs/UI.md).
//!
//! Arrows at the bottom (or ← →, 1–9 on a keyboard) change pages; the pill
//! top-right (or T) cycles the theme; taps elsewhere poke the page.
//!
//! The AIM page is a target range aimed by pointing the phone
//! (`sensors::Scope` on the attitude sensor): tap to fire, RECENTER (or C)
//! to make where the phone points now the middle, and DEFAULT/PRECISE (or M)
//! to compare the scope's default light-gun mapping with `Scope::precise`.
//!
//! The STATS page is the between-fights card (`ui::StatsCard`): the stats
//! roll up, a rank stamps on, the next opponent slides in and a wipe
//! (iris, slide, shutter in turn) carries on to the next fight. Tap to
//! skip ahead.
//!
//! The DAMAGE page is `Portrait::set_health` and `Portrait::hit`: an
//! opponent taking hits from full health to near death (sweat, soot, cracked
//! glass, static, sparks, the alarm), and the player's own portrait in the
//! corner taking a few too. Tap to land a hit.
//!
//! The ENDING page is a ~20 s epilogue played by `ui::Sequence`: the
//! camera moves over a stand-in arena (drawn in 2D through
//! `gfx3d::Camera::project`), the beaten champion speaks, her mech falls,
//! the winner counts the cost, a title card jumps a year on, a closing line,
//! credits with the cast, a fade. Tap to hurry it, hold to skip.

use maimbrain::gfx2d::{self, Font};
use maimbrain::input::{self, Kind};
use maimbrain::juice::{Combo, Flash, Particles, Popups, VignettePulse};
use maimbrain::motion::{Counter, Ease, Punch, Seq, Shake, Spring, Timer, stagger};
use maimbrain::gfx3d::{Camera, Vec3, vec3};
use maimbrain::sensors::{self, AimSource, Haptic, Scope, Smoothing, Target};
use maimbrain::sys::{self, Round};
use maimbrain::ui::shape::{self, capsule, disc, rrect, soft_disc};
use maimbrain::ui::{
    self, Align, Button, ButtonKind, CardSounds, Countdown, SeqEvent, Sequence, Shot, Dialogue, Hud, Icon, Layout, Portrait, Rect, ResultsCard, StatsCard, StatsEvent, Tap, Theme,
    TitleCard, VAlign, VoiceLine, Wipe, fade, text, with_alpha,
};
use maimbrain::{Game, Rng, export_game};

const PAGES: [&str; 14] = ["TITLE", "WIDGETS", "ICONS", "TEXT", "EASING", "MOTION", "JUICE", "RESULTS", "HUD", "VOICE", "AIM", "STATS", "DAMAGE", "ENDING"];
const AIM: usize = 10;
const STATS: usize = 11;
const DAMAGE: usize = 12;
const ENDING: usize = 13;
/// A target's radius, and how close the crosshair must be to hit it (a little generous).
const TARGET_R: f32 = 22.0;
const HIT_R: f32 = 27.0;

/// Portrait sheet cells (sheet pixels), as `mb art portrait` wrote them: 48 art
/// pixels across, 4 sheet pixels each (`Portrait` takes a multiple of 48 that way).
const PORTRAIT_CELL: u32 = 192;

// HID usage codes (SPEC §5.2).
const KEY_RIGHT: u32 = 79;
const KEY_LEFT: u32 = 80;
const KEY_1: u32 = 30;
const KEY_T: u32 = 23;
const KEY_C: u32 = 6;
const KEY_M: u32 = 16;

struct Gallery {
    layout: Layout,
    theme_i: usize,
    theme: Theme,
    page: usize,
    t: f32,
    page_t: f32,
    rng: Rng,
    prev: Button,
    next: Button,
    theme_btn: Button,
    // widgets page
    buttons: Vec<Button>,
    // title page
    title: TitleCard,
    // motion page
    springs: [Spring; 3],
    spring_timer: Timer,
    counter: Counter,
    count_timer: Timer,
    punch: Punch,
    shake: Shake,
    seq: Seq,
    // juice page
    fx: Particles,
    popups: Popups,
    combo: Combo,
    flash: Flash,
    vignette: VignettePulse,
    juice_timer: Timer,
    juice_step: u32,
    // results / hud pages
    results: ResultsCard,
    results_round: u32,
    hud: Hud,
    hud_score: i64,
    hud_timer: Timer,
    countdown: Countdown,
    // voice page: two pilots trash-talking (mb voice lines, mb art portrait sheets)
    talk: Dialogue,
    rocko: Portrait,
    granny: Portrait,
    /// (speaker, line, expression): the cutscene, then the barks a tap plays.
    script: Vec<(&'static str, VoiceLine, &'static str)>,
    barks: Vec<(&'static str, VoiceLine, &'static str)>,
    bark_i: usize,
    // aim page: a target range aimed by pointing the phone
    scope: Scope,
    /// (x, y, seconds since it appeared)
    targets: Vec<(f32, f32, f32)>,
    recenter: Button,
    /// DEFAULT (`Scope::new`: 32°, fixed 8 ms, flat gain, no friction) or PRECISE (`Scope::precise`).
    aim_mode: Button,
    precise_aim: bool,
    hits: u32,
    shots: u32,
    /// The finger, while it's down (drags aim when there's no gyroscope).
    finger: Option<(f32, f32)>,
    /// How fast the phone is turning (°/s), and the frame rate, for the readout.
    turn_rate: f32,
    fps: f32,
    // stats page: the card between fights, over a stand-in arena
    stats: StatsCard,
    /// The fight on screen (it moves on when the wipe covers it), and how long since the card finished.
    fight: u32,
    stats_idle: f32,
    /// The card's sounds (`card_sfx.py`), shared by every card: a jingle in, ticks, thunks, the stamp, whooshes, a sting out.
    card_sfx: CardSounds,
    // damage page: an opponent beaten from full health to near death, and the player's own face
    foe: Portrait,
    me: Portrait,
    foe_hp: f32,
    me_hp: f32,
    dmg_timer: Timer,
    dmg_hits: u32,
    // ending page: an epilogue on ui::Sequence
    ending: Sequence,
    /// When the champion's mech fell (the sequence's "fall" cue), and how long since the epilogue finished.
    fell_at: Option<f64>,
    ending_idle: f32,
}

impl Gallery {
    fn set_theme(&mut self, i: usize) {
        let all = themes();
        self.theme_i = i % all.len();
        self.theme = all[self.theme_i].1;
        self.title.theme = self.theme;
        self.hud.theme = self.theme;
        self.stats.theme = self.theme;
        self.theme_btn.label = all[self.theme_i].0.to_uppercase();
        // The results layout decides where its buttons go: build it again.
        self.results_round = self.results_round.saturating_sub(1);
        self.new_results();
    }

    fn go(&mut self, page: usize) {
        self.page = page % PAGES.len();
        self.page_t = 0.0;
        self.fx.clear();
        self.popups.list.clear();
        self.combo = Combo::new(3.0);
        match self.page {
            0 => self.title.restart(),
            7 => self.new_results(),
            8 => {
                self.hud_score = 0;
                self.hud.reset(120);
                self.countdown.start(3);
            }
            9 => {
                self.talk.skip_all();
                for (who, line, face) in &self.script {
                    self.talk.say(who, line, face);
                }
            }
            STATS => {
                self.fight = 0;
                self.new_stats();
            }
            DAMAGE => self.reset_damage(),
            ENDING => {
                self.ending = new_ending(self.theme);
                self.fell_at = None;
                self.ending_idle = 0.0;
            }
            AIM => {
                // Wherever the phone points as the page opens is the middle.
                self.scope.recenter();
                self.finger = None;
            }
            _ => {}
        }
        if self.page != 9 {
            self.talk.skip_all();
        }
    }

    /// The card after `self.fight`, with the next opponent and the next wipe.
    fn new_stats(&mut self) {
        let f = self.fight;
        let acc = [0.94, 0.78, 0.61][f as usize % 3];
        let wipe = [Wipe::Iris, Wipe::Slide, Wipe::Shutter][f as usize % 3];
        // Granny's line is long: it wraps under her name, and the prompt keeps below it.
        let next = if f % 2 == 0 { ("GRANNY", "has never lost a duel in forty years of piloting") } else { ("ROCKO", "wants a rematch") };
        self.stats = StatsCard::new(&format!("FIGHT {} CLEAR", f + 1), self.theme, &self.layout)
            .percent("ACCURACY", acc)
            .int("PARTS BROKEN", 6 - f as i64 % 3)
            .int("POWER SHOTS", 3 + f as i64 % 4)
            .time("TIME", 38.4 + 11.0 * (f % 3) as f32)
            .int("DAMAGE TAKEN", 12 + 17 * (f % 3) as i64)
            .bad()
            .rank(StatsCard::rank_for(acc))
            .next(next.0, next.1)
            .wipe(wipe)
            .sounds(self.card_sfx.clone());
        self.stats_idle = 0.0;
    }

    fn reset_damage(&mut self) {
        (self.foe_hp, self.me_hp, self.dmg_hits) = (1.0, 1.0, 0);
        self.foe.set_health(1.0);
        self.me.set_health(1.0);
        self.dmg_timer = Timer::new(1.0);
    }

    /// A hit on the opponent: harder every third, and the health it takes.
    fn hit_foe(&mut self, strength: f32) {
        self.foe.hit(strength);
        self.foe_hp = (self.foe_hp - 0.05 - 0.06 * strength).max(0.02);
        self.foe.set_health(self.foe_hp);
        self.shake.add(0.15 + 0.3 * strength);
    }

    fn new_results(&mut self) {
        // Alternate a new best with a near miss.
        let (score, best) = if self.results_round.is_multiple_of(2) { (1280, 940) } else { (88, 92) };
        self.results_round += 1;
        let extra = Button::new(Rect::default(), "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY);
        self.results = ResultsCard::new(score, best, self.theme, &self.layout, self.rng.next_u32() as u64).heading("SPLAT!").label("POINTS").extra(extra);
    }

    /// Where targets appear: the middle of the screen, clear of the readout and the buttons.
    fn range(&self) -> Rect {
        let l = &self.layout;
        Rect::new(40.0, l.safe.y + 130.0, 280.0, (self.recenter.rect.y - 30.0) - (l.safe.y + 130.0))
    }

    fn new_target(&mut self) -> (f32, f32, f32) {
        let r = self.range();
        // Not on top of another one.
        for _ in 0..20 {
            let p = (r.x + self.rng.f32() * r.w, r.y + self.rng.f32() * r.h);
            if self.targets.iter().all(|t| (t.0 - p.0).hypot(t.1 - p.1) > 3.0 * TARGET_R) {
                return (p.0, p.1, 0.0);
            }
        }
        (r.cx(), r.cy(), 0.0)
    }

    /// A shot at the crosshair.
    fn fire(&mut self) {
        let (x, y) = self.scope.position();
        self.shots += 1;
        self.flash.fire(with_alpha(0xffffffff, 0.1));
        let th = self.theme;
        if let Some(i) = self.targets.iter().position(|t| (t.0 - x).hypot(t.1 - y) < HIT_R) {
            let (tx, ty, _) = self.targets[i];
            self.hits += 1;
            self.fx.burst(tx, ty, 26, th.accent);
            self.fx.ring(tx, ty, 60.0, th.gold);
            self.popups.score(tx, ty - 24.0, 100, th.gold);
            sensors::haptic(Haptic::Tap);
            self.targets[i] = self.new_target();
        } else {
            self.fx.sparks(x, y, -std::f32::consts::FRAC_PI_2, 2.4, 8, with_alpha(th.text, 0.7));
        }
    }

    /// DEFAULT: the scope's defaults, the simple light-gun mapping (32° across, a fixed
    /// 8 ms of smoothing, the same gain at every speed, no friction). PRECISE: what
    /// `Scope::precise` turns on (40° across, One Euro smoothing, a slower gain for slow
    /// turns, friction on the targets), to feel the difference. The crosshair stays where it is.
    fn set_precise_aim(&mut self, precise: bool) {
        self.precise_aim = precise;
        let d = if precise { Scope::precise(1.0, 1.0) } else { Scope::new(1.0, 1.0) };
        let s = &mut self.scope;
        (s.gain, s.smoothing, s.curve, s.friction, s.magnetism) = (d.gain, d.smoothing, d.curve, d.friction, d.magnetism);
        self.aim_mode.label = if precise { "PRECISE" } else { "DEFAULT" }.to_string();
        self.aim_mode.kind = if precise { ButtonKind::Primary } else { ButtonKind::Secondary };
    }

    /// The aim page's input: RECENTER, DEFAULT/PRECISE, C, M, a tap fires, and with no gyroscope a drag aims.
    fn aim_input(&mut self, e: &input::Event) {
        if self.recenter.handle(e) == Some(Tap::Clicked) {
            self.scope.recenter();
            return;
        }
        if self.aim_mode.handle(e) == Some(Tap::Clicked) {
            self.set_precise_aim(!self.precise_aim);
            return;
        }
        if self.recenter.is_held() || self.aim_mode.is_held() {
            return;
        }
        match e.kind() {
            Kind::KeyDown if e.code == KEY_C => self.scope.recenter(),
            Kind::KeyDown if e.code == KEY_M => self.set_precise_aim(!self.precise_aim),
            Kind::TouchDown | Kind::MouseDown => {
                self.finger = Some((e.x, e.y));
                self.fire();
            }
            Kind::TouchMove | Kind::MouseMove => {
                if let Some(f) = self.finger {
                    if self.scope.source() == Some(AimSource::Drag) {
                        self.scope.drag(e.x - f.0, e.y - f.1);
                    }
                    self.finger = Some((e.x, e.y));
                }
            }
            Kind::TouchUp | Kind::TouchCancel | Kind::MouseUp => self.finger = None,
            _ => {}
        }
    }

    fn poke(&mut self, x: f32, y: f32) {
        match self.page {
            0 => self.title.restart(),
            5 => {
                self.punch.kick(0.4);
                self.shake.add(0.6);
            }
            6 => {
                self.fx.confetti(x, y, 60, &self.theme.confetti);
                self.popups.score(x, y - 20.0, 100 * (1 + self.combo.count as i64), self.theme.gold);
                self.combo.hit();
            }
            8 => {
                self.hud_score += 25;
                self.hud.set_score(self.hud_score);
            }
            DAMAGE => self.hit_foe(0.9),
            9 => {
                // A hit: the one who got hit barks (dropped if a bark started less than 5 s ago or someone is still speaking).
                let (who, line, face) = &self.barks[self.bark_i % self.barks.len()];
                if self.talk.bark(who, line, face) {
                    self.bark_i += 1;
                }
            }
            _ => {}
        }
    }

    fn update_page(&mut self, dt: f32) {
        match self.page {
            0 => self.title.update(dt),
            1 => {
                for b in &mut self.buttons {
                    b.update(dt);
                }
            }
            5 => {
                if self.spring_timer.update(dt) {
                    let to = if self.springs[0].target > 0.5 { 0.0 } else { 1.0 };
                    for s in &mut self.springs {
                        s.target = to;
                    }
                }
                for s in &mut self.springs {
                    s.update(dt);
                }
                if self.count_timer.update(dt) {
                    let next = self.counter.target() + 250 + (self.rng.next_u32() % 2000) as i64;
                    self.counter.set(next);
                    self.punch.kick(0.3);
                    self.shake.add(0.35);
                }
                self.counter.update(dt);
                self.punch.update(dt);
                self.shake.update(dt);
                self.seq.update(dt);
            }
            6 => {
                if self.juice_timer.update(dt) {
                    self.juice_step += 1;
                    let (cx, cy) = (180.0, 300.0);
                    let th = self.theme;
                    match self.juice_step % 6 {
                        0 => {
                            self.fx.confetti(cx, cy, 90, &th.confetti);
                            self.flash.fire(with_alpha(th.gold, 0.35));
                        }
                        1 => self.fx.sparkles(cx, cy, 70.0, 18, ui::lighten(th.gold, 0.4)),
                        2 => self.fx.stars(cx, cy, 12, th.gold),
                        3 => {
                            self.fx.burst(cx, cy, 30, th.accent);
                            self.fx.ring(cx, cy, 110.0, th.text);
                            self.vignette.fire();
                        }
                        4 => self.fx.sparks(cx, cy, -std::f32::consts::FRAC_PI_2, 1.2, 40, th.gold),
                        _ => self.fx.confetti_cannons(self.layout.screen, 80, &th.confetti),
                    }
                    let x = 70.0 + self.rng.f32() * 220.0;
                    self.popups.score(x, 470.0, 50 * (1 + self.juice_step as i64 % 5), th.text);
                    if self.combo.count < 12 {
                        self.combo.hit();
                    } else {
                        self.combo.break_combo();
                    }
                }
                self.fx.update(dt);
                self.popups.update(dt);
                self.combo.window = 3.0;
                self.combo.update(dt);
                self.flash.update(dt);
                self.vignette.update(dt);
            }
            7 => {
                self.results.update(dt);
                if self.page_t > 7.0 {
                    self.page_t = 0.0;
                    self.new_results();
                }
            }
            8 => {
                if self.countdown.update(dt).is_some() {}
                if !self.countdown.active() && self.hud_timer.update(dt) {
                    self.hud_score += 5 + (self.rng.next_u32() % 30) as i64;
                    self.hud.set_score(self.hud_score);
                    let lives = 3 - ((self.hud_score / 120) % 4) as u32;
                    self.hud.lives = Some((lives.min(3), 3));
                    self.hud.progress = Some((self.hud_score % 120) as f32 / 120.0);
                    self.combo.hit();
                }
                self.hud.update(dt);
                self.combo.window = 1.4;
                self.combo.update(dt);
                if self.page_t > 9.0 {
                    self.go(8);
                }
            }
            STATS => {
                self.card_sfx.poll();
                for e in self.stats.update(dt) {
                    // A game plays its sounds here (Tick, Landed, Rank…); on Covered it swaps scenes.
                    if e == StatsEvent::Covered {
                        self.fight += 1;
                    }
                }
                if self.stats.finished() {
                    // The next fight "plays" for a moment, then its card comes up.
                    self.stats_idle += dt;
                    if self.stats_idle > 1.5 {
                        self.new_stats();
                    }
                }
            }
            ENDING => {
                for e in self.ending.update(dt) {
                    if e == SeqEvent::Cue("fall".into()) {
                        self.fell_at = Some(self.ending.age());
                    }
                }
                if self.ending.finished() {
                    self.ending_idle += dt;
                    if self.ending_idle > 2.0 {
                        self.go(ENDING);
                    }
                }
            }
            DAMAGE => {
                // A fight in fast-forward: a hit a second (every third one hard), the player hit back now and then.
                if self.dmg_timer.update(dt) {
                    self.dmg_hits += 1;
                    if self.foe_hp <= 0.02 {
                        self.reset_damage();
                    } else {
                        let strength = if self.dmg_hits.is_multiple_of(3) { 1.0 } else { 0.45 };
                        self.hit_foe(strength);
                        if self.dmg_hits % 4 == 2 {
                            self.me.hit(0.7);
                            self.me_hp = (self.me_hp - 0.18).max(0.1);
                            self.me.set_health(self.me_hp);
                        }
                    }
                }
                self.shake.update(dt);
            }
            AIM => {
                // Friction: the crosshair slows over a target (it never pulls toward one).
                let targets: Vec<Target> = self.targets.iter().map(|t| Target::new(t.0, t.1, TARGET_R)).collect();
                self.scope.set_targets(&targets);
                self.scope.update(dt);
                let r = sensors::attitude().map_or([0.0; 3], |a| a.rate);
                self.turn_rate = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().to_degrees();
                if dt > 0.0 {
                    self.fps += (1.0 / dt - self.fps) * 0.05;
                }
                while self.targets.len() < 3 {
                    let t = self.new_target();
                    self.targets.push(t);
                }
                for t in &mut self.targets {
                    t.2 += dt;
                }
                self.recenter.update(dt);
                self.aim_mode.update(dt);
                self.fx.update(dt);
                self.popups.update(dt);
                self.flash.update(dt);
            }
            _ => {}
        }
    }
}

/// The gallery's epilogue: the five beats of an ending (the defeat, the
/// cost, the hero's reaction, the world after with a closing line, credits
/// with the cast) on `ui::Sequence`, about 20 s. The lines are text-only
/// (`VoiceLine::subtitle`); a game would `VoiceLine::load` its `mb voice` lines.
fn new_ending(theme: Theme) -> Sequence {
    let granny = Portrait::load("assets/portraits/granny.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3).on_right();
    granny.set_health(0.15);
    let rocko = Portrait::load("assets/portraits/rocko.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3);
    rocko.set_health(0.6);
    let wide = Shot::new(vec3(0.0, 2.4, 9.5), vec3(0.0, 1.0, 0.0)).fov(0.9);
    let on_granny = Shot::new(vec3(4.2, 1.9, 5.2), vec3(1.2, 0.8, 0.4)).fov(0.75);
    let on_rocko = Shot::new(vec3(-3.0, 1.8, 3.8), vec3(-1.5, 1.4, 0.0)).fov(0.75);
    let sunset = Shot::new(vec3(0.0, 1.2, 7.0), vec3(0.0, 3.5, -30.0)).fov(0.95);
    let crane = Shot::new(vec3(0.5, 8.0, 8.0), vec3(0.0, 0.0, 0.0)).fov(1.0);
    let say = VoiceLine::subtitle;
    Sequence::new(theme)
        .cast("GRANNY", granny)
        .cast("ROCKO", rocko)
        .letterbox(0.22)
        .shot(wide, 0.0)
        .fade_in(1.2)
        .meanwhile()
        // The defeat.
        .move_to(on_granny, 3.0, Ease::SineInOut)
        .meanwhile()
        .line("GRANNY", &say("Forty years undefeated. Beaten by a kid in a dented helmet."), "")
        .cue("fall")
        .wait(1.2)
        // The cost, and the hero's reaction.
        .move_to(on_rocko, 2.0, Ease::SineInOut)
        .meanwhile()
        .line("ROCKO", &say("Cost me an arm. The mech's, I mean."), "")
        .line("ROCKO", &say("Good fight, Granny."), "smug")
        // The world after, and a closing line.
        .move_to(sunset, 3.0, Ease::SineInOut)
        .meanwhile()
        .title("ONE YEAR LATER", "the arena, rebuilt", 3.0)
        .line("GRANNY", &say("Same time next year. Bring a better arm."), "smug")
        // Credits with the cast.
        .move_to(crane, 6.5, Ease::QuadInOut)
        .meanwhile()
        .credits(&[("THE CHAMPION", "GRANNY"), ("THE CHALLENGER", "ROCKO"), ("THANKS FOR PLAYING", "UI GALLERY")], 6.0)
        .fade_out(1.2)
}

/// The 17 identities first, then the five original presets (fonts load
/// only where this gallery bakes them; the rest draw in Inter).
fn themes() -> Vec<(&'static str, Theme)> {
    let mut v = Theme::identities();
    v.extend(Theme::presets());
    v
}

impl Game for Gallery {
    fn init() -> Self {
        sys::round(Round::Idle);
        let layout = Layout::new();
        let mut rng = Rng::from_host();
        let theme = themes()[0].1;
        let foot = layout.screen.bottom() - 46.0;
        let mut buttons = Vec::new();
        buttons.push(Button::new(Rect::centered(180.0, 132.0, 220.0, 58.0), "PLAY").with_icon(Icon::PLAY));
        buttons.push(Button::new(Rect::new(40.0, 186.0, 136.0, 48.0), "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY));
        buttons.push(Button::new(Rect::new(184.0, 186.0, 136.0, 48.0), "SKIP").kind(ButtonKind::Ghost).with_icon(Icon::ARROW_RIGHT));
        for (i, icon) in [Icon::RESTART, Icon::HOME, Icon::GEAR, Icon::SOUND, Icon::PAUSE].into_iter().enumerate() {
            buttons.push(Button::icon(64.0 + i as f32 * 58.0, 270.0, 50.0, icon));
        }
        let seed = rng.next_u32() as u64;
        let rocko = Portrait::load("assets/portraits/rocko.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3);
        let granny = Portrait::load("assets/portraits/granny.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3).on_right();
        let vo = |p: &str| VoiceLine::load(&format!("assets/vo/{p}"));
        let script = vec![("ROCKO", vo("rocko/intro"), "smug"), ("GRANNY", vo("granny/intro"), ""), ("ROCKO", vo("rocko/taunt"), "smug"), ("GRANNY", vo("granny/taunt"), "smug")];
        let barks = vec![("ROCKO", vo("rocko/ouch"), "angry"), ("GRANNY", vo("granny/ouch"), "angry")];
        let results = ResultsCard::new(1280, 940, theme, &layout, seed);
        let mut g = Gallery {
            layout,
            theme_i: 0,
            theme,
            page: 0,
            t: 0.0,
            page_t: 0.0,
            prev: Button::icon(54.0, foot, 48.0, Icon::ARROW_LEFT).kind(ButtonKind::Secondary),
            next: Button::icon(306.0, foot, 48.0, Icon::ARROW_RIGHT).kind(ButtonKind::Secondary),
            theme_btn: Button::new(Rect::new(236.0, layout.safe.y + 10.0, 110.0, 32.0), themes()[0].0).kind(ButtonKind::Secondary),
            buttons,
            title: TitleCard::new("BUBBLE POP", theme).tagline("don't let it pop").best(1240),
            springs: [Spring::critical(0.0, 1.6), Spring::bouncy(0.0, 1.6), Spring::wobbly(0.0, 1.6)],
            spring_timer: Timer::new(1.6),
            counter: Counter::new(0),
            count_timer: Timer::new(1.5),
            punch: Punch::new(),
            shake: Shake::new(),
            seq: Seq::new(0.0).to(1.0, 0.5, Ease::BackOut).wait(0.4).to(0.3, 0.3, Ease::QuadIn).wait(0.2).to(0.0, 0.6, Ease::BounceOut).wait(0.3).looped(),
            fx: Particles::new(rng.next_u32() as u64),
            popups: Popups::new(),
            combo: Combo::new(3.0),
            flash: Flash::new(0.4),
            vignette: VignettePulse::new(0xff2040ff),
            juice_timer: Timer::new(1.1),
            juice_step: 0,
            results,
            results_round: 0,
            stats: StatsCard::new("", theme, &layout),
            fight: 0,
            stats_idle: 0.0,
            card_sfx: CardSounds::load("assets/sfx"),
            // Clipped: the hits knock the faces about inside their frames.
            foe: Portrait::load("assets/portraits/granny.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3).on_right().clipped(),
            me: Portrait::load("assets/portraits/rocko.png", PORTRAIT_CELL, PORTRAIT_CELL).expression("smug", 1).expression("angry", 2).blinks(3).clipped(),
            foe_hp: 1.0,
            me_hp: 1.0,
            dmg_timer: Timer::new(1.0),
            dmg_hits: 0,
            ending: new_ending(theme),
            fell_at: None,
            ending_idle: 0.0,
            hud: Hud::new(theme, 120),
            hud_score: 0,
            hud_timer: Timer::new(0.45),
            countdown: Countdown::default(),
            talk: Dialogue::new(theme).cast("ROCKO", rocko.clone()).cast("GRANNY", granny.clone()),
            rocko,
            granny,
            script,
            barks,
            bark_i: 0,
            scope: Scope::new(layout.screen.w, layout.screen.h),
            targets: Vec::new(),
            recenter: Button::new(Rect::centered(132.0, foot - 64.0, 160.0, 44.0), "RECENTER").kind(ButtonKind::Secondary).with_icon(Icon::TARGET),
            aim_mode: Button::new(Rect::centered(282.0, foot - 64.0, 116.0, 44.0), "DEFAULT").kind(ButtonKind::Secondary),
            precise_aim: false,
            hits: 0,
            shots: 0,
            finger: None,
            turn_rate: 0.0,
            fps: 60.0,
            rng,
        };
        g.set_theme(0);
        // Barks pop up under the pilots.
        g.talk.bark_y = Some(g.layout.safe.y + 300.0);
        // `--param theme=N` starts on theme N (0 riso … 16 cozy-pixel, then candy, night, arcade, paper, jungle).
        if let Some(t) = sys::param_as::<usize>("theme") {
            g.set_theme(t);
        }
        // `--param ink=navy`: dark ink on a dark panel (a game's own theme the STATS card must still read on).
        if sys::param("ink").as_deref() == Some("navy") {
            (g.theme.text, g.theme.text_dim, g.theme.panel_top, g.theme.panel_bottom) = (0x1b2a4aff, 0x2c3a5cff, 0x2a3866f0, 0x22305af0);
        }
        // `--param page=9` (mb shot) opens a page directly (`page=10`: AIM, `page=11`: STATS, `page=12`: DAMAGE).
        if let Some(p) = sys::param_as::<usize>("page") {
            g.go(p);
        }
        // `--param aim=precise` starts the AIM page on `Scope::precise`.
        if sys::param("aim").as_deref() == Some("precise") {
            g.set_precise_aim(true);
        }
        g
    }

    fn update(&mut self, dt: f32) {
        self.t += dt;
        self.page_t += dt;
        for e in input::poll() {
            // A cutscene takes the taps (the first finishes the line, the next moves on); the arrows still work.
            if self.page == 9 && !self.prev.rect.contains(e.x, e.y) && !self.next.rect.contains(e.x, e.y) && self.talk.input(&e) {
                continue;
            }
            // The epilogue takes every touch but the arrows: a tap hurries it, holding skips it.
            if self.page == ENDING && !self.prev.rect.contains(e.x, e.y) && !self.next.rect.contains(e.x, e.y) && !matches!(e.kind(), Kind::KeyDown if e.code == KEY_LEFT || e.code == KEY_RIGHT) && self.ending.input(&e) {
                continue;
            }
            // The range takes every touch but the arrows and the theme switch: a tap anywhere fires.
            let chrome = |b: &Button| b.rect.contains(e.x, e.y) || b.is_held();
            if self.page == AIM && !chrome(&self.prev) && !chrome(&self.next) && !chrome(&self.theme_btn) && !matches!(e.kind(), Kind::KeyDown if e.code != KEY_C && e.code != KEY_M) {
                self.aim_input(&e);
                continue;
            }
            match e.kind() {
                Kind::KeyDown if e.code == KEY_RIGHT => self.go(self.page + 1),
                Kind::KeyDown if e.code == KEY_LEFT => self.go(self.page + PAGES.len() - 1),
                Kind::KeyDown if (KEY_1..KEY_1 + 9).contains(&e.code) => self.go((e.code - KEY_1) as usize),
                Kind::KeyDown if e.code == KEY_T => self.set_theme(self.theme_i + 1),
                _ => {}
            }
            if self.prev.handle(&e) == Some(Tap::Clicked) {
                self.go(self.page + PAGES.len() - 1);
                continue;
            }
            if self.next.handle(&e) == Some(Tap::Clicked) {
                self.go(self.page + 1);
                continue;
            }
            if self.theme_btn.handle(&e) == Some(Tap::Clicked) {
                self.set_theme(self.theme_i + 1);
                continue;
            }
            if self.prev.is_held() || self.next.is_held() || self.theme_btn.is_held() {
                continue;
            }
            let mut claimed = false;
            if self.page == 1 {
                for b in &mut self.buttons {
                    claimed |= b.handle(&e).is_some();
                }
            }
            if self.page == STATS && !self.stats.finished() {
                claimed |= self.stats.input(&e);
            }
            if self.page == 7 {
                if let Some(a) = self.results.handle(&e) {
                    if a == ui::ResultsAction::Retry {
                        self.new_results();
                    }
                    claimed = true;
                }
                claimed |= self.results.claims(e.x, e.y);
            }
            if !claimed && e.is_press() {
                self.poke(e.x, e.y);
            }
        }
        self.prev.update(dt);
        self.next.update(dt);
        self.theme_btn.update(dt);
        self.talk.theme = self.theme;
        self.talk.update(dt);
        self.update_page(dt);
    }

    fn render(&self) {
        let th = &self.theme;
        let l = &self.layout;
        th.backdrop(l.screen, self.t);
        match self.page {
            0 => self.draw_title(),
            1 => self.draw_widgets(),
            2 => self.draw_icons(),
            3 => self.draw_text(),
            4 => self.draw_easing(),
            5 => self.draw_motion(),
            6 => self.draw_juice(),
            7 => self.results.draw(),
            8 => self.draw_hud(),
            9 => self.draw_voice(),
            STATS => self.draw_stats(),
            DAMAGE => self.draw_damage(),
            ENDING => self.draw_ending(),
            _ => self.draw_aim(),
        }
        // Chrome: page name, theme switch, arrows and dots.
        if self.page != 0 && self.page != 8 {
            text(PAGES[self.page]).font(th.title_font).size(th.fit(th.title_font, 22.0)).color(th.text).outline(0.08, th.outline).valign(VAlign::Middle).draw(16.0, l.safe.y + 26.0);
        }
        if self.page != 8 {
            self.theme_btn.draw(th);
        }
        self.prev.draw(th);
        self.next.draw(th);
        let n = PAGES.len();
        for i in 0..n {
            let x = 180.0 + (i as f32 - (n - 1) as f32 / 2.0) * (190.0 / n as f32).min(16.0);
            let on = i == self.page;
            disc(x, self.prev.rect.cy(), if on { 5.0 } else { 3.5 }, if on { th.accent } else { with_alpha(th.text, 0.35) });
        }
    }
}

impl Gallery {
    /// Two pilots face off; whoever speaks moves their mouth with the line and leans in.
    fn draw_voice(&self) {
        let th = &self.theme;
        let l = &self.layout;
        let now = sys::time();
        let speaking = self.talk.current().or_else(|| self.talk.current_bark());
        let top = l.safe.y + 64.0;
        for (i, (name, p)) in [("ROCKO", &self.rocko), ("GRANNY", &self.granny)].into_iter().enumerate() {
            let x = if i == 0 { 22.0 } else { 202.0 };
            let (face, mouth, on) = match &speaking {
                Some(s) if s.speaker == name => (s.expression.as_str(), s.mouth, true),
                _ => ("", 0, false),
            };
            rrect(Rect::new(x - 4.0, top - 4.0, 144.0, 152.0), 14.0, with_alpha(if on { th.accent } else { th.text }, if on { 0.22 } else { 0.08 }));
            p.draw(Rect::new(x, top + if on { 0.0 } else { 6.0 }, 136.0, 144.0), face, mouth, now);
            text(name).font(th.title_font).size(18.0).color(th.text).outline(0.08, th.outline).align(Align::Center).draw(x + 68.0, top + 166.0);
        }
        text("VS").font(th.title_font).size(30.0).color(th.accent).outline(0.1, th.outline).align(Align::Center).valign(VAlign::Middle).draw(180.0, top + 72.0);
        if !self.talk.active() {
            text("tap: a hit (they bark)").size(14.0).color(with_alpha(th.text, 0.7)).align(Align::Center).draw(180.0, top + 330.0);
        }
        // The box sits above the page arrows.
        let mut dl = *l;
        dl.safe.h = self.prev.rect.y - 8.0 - dl.safe.y;
        self.talk.draw(&dl);
    }

    /// The opponent's comm screen, big, and the player's own in the corner, each with a health bar.
    fn draw_damage(&self) {
        let th = &self.theme;
        let l = &self.layout;
        let now = sys::time();
        let bar = |r: Rect, hp: f32| {
            ui::progress_bar(r, hp, if hp > 0.5 { th.good } else if hp > 0.25 { th.gold } else { th.bad }, with_alpha(th.text, 0.15));
        };
        // The player, top left (a HUD portrait).
        let me = Rect::new(16.0, l.safe.y + 52.0, 84.0, 84.0);
        rrect(me.inset(-4.0), 10.0, with_alpha(th.outline, 0.85));
        self.me.draw(me, "", 0, now);
        bar(Rect::new(me.right() + 10.0, me.y + 30.0, 120.0, 12.0), self.me_hp);
        text("YOU").font(th.title_font).size(16.0).color(th.text).outline(0.08, th.outline).draw(me.right() + 10.0, me.y + 6.0);
        // The opponent, big, shaken by the hits.
        gfx2d::push();
        self.shake.apply(180.0, 330.0);
        let foe = Rect::new(60.0, l.safe.y + 170.0, 240.0, 240.0);
        rrect(foe.inset(-6.0), 14.0, with_alpha(th.outline, 0.9));
        self.foe.draw(foe, "", 0, now);
        gfx2d::pop();
        bar(Rect::new(60.0, foe.bottom() + 18.0, 240.0, 16.0), self.foe_hp);
        let stage = ["FINE", "SCUFFED", "DAMAGED", "CRITICAL"][ui::damage::stage(self.foe_hp) as usize];
        text(&format!("GRANNY  {}%  {stage}", (self.foe_hp * 100.0).round()))
            .font(th.body_font)
            .size(15.0)
            .color(th.text)
            .align(Align::Center)
            .draw(180.0, foe.bottom() + 44.0);
        text("tap: a hard hit").size(13.0).color(th.text_dim).align(Align::Center).draw(180.0, foe.bottom() + 70.0);
    }

    /// The arena at dusk seen through the sequence's camera (projected in 2D), the epilogue over it.
    fn draw_ending(&self) {
        let l = &self.layout;
        let (w, h) = (l.screen.w, l.screen.h);
        let cam = self.ending.camera().unwrap_or_else(|| Camera::looking_at(vec3(0.0, 2.0, 9.0), vec3(0.0, 1.0, 0.0)));
        let pr = |p: Vec3| cam.project(p, w, h);
        // Sky and ground: the horizon is where a far point on the ground lands.
        let fwd = (cam.rot * vec3(0.0, 0.0, -1.0)) * 1000.0;
        let horizon = pr(vec3(cam.pos.x + fwd.x, 0.0, cam.pos.z + fwd.z)).y.clamp(-h, 2.0 * h);
        ui::shape::gradient(Rect::new(0.0, 0.0, w, horizon.max(0.0)), 0x1b1236ff, 0xe0805aff);
        let sun = pr(vec3(0.0, 30.0, -400.0));
        if sun.depth > 0.0 {
            soft_disc(sun.x, sun.y, 60.0, 50.0, 0xffd08a70);
            disc(sun.x, sun.y, 26.0, 0xffe2b0ff);
        }
        gfx2d::rect(0.0, horizon.max(0.0), w, (h - horizon).max(0.0), 0x2a1d30ff);
        // The arena floor's grid, in short pieces so the ones behind the camera drop out.
        for i in -6..=6 {
            for j in -6..6 {
                for (a, b) in [(vec3(i as f32 * 2.0, 0.0, j as f32 * 2.0), vec3(i as f32 * 2.0, 0.0, j as f32 * 2.0 + 2.0)), (vec3(j as f32 * 2.0, 0.0, i as f32 * 2.0), vec3(j as f32 * 2.0 + 2.0, 0.0, i as f32 * 2.0))] {
                    let (pa, pb) = (pr(a), pr(b));
                    if pa.depth > 0.3 && pb.depth > 0.3 {
                        gfx2d::line(pa.x, pa.y, pb.x, pb.y, 1.5, 0xff8a6a40);
                    }
                }
            }
        }
        // The two mechs, the farther first.
        let t = self.ending.age();
        let fall = self.fell_at.map_or(0.0, |f| Ease::BounceOut.at(((t - f) / 0.9).clamp(0.0, 1.0) as f32));
        let hero = (vec3(-1.5, 0.0, 0.0), 0xe07a2aff, 0.0f32);
        let champ = (vec3(1.5, 0.0, 0.4), 0x8a6ad0ff, -0.25 - 1.32 * fall);
        let mut both = [hero, champ];
        both.sort_by(|a, b| pr(b.0).depth.total_cmp(&pr(a.0).depth));
        for (base, color, lean) in both {
            let (b0, b1) = (pr(base), pr(base + vec3(0.0, 1.0, 0.0)));
            if b0.depth <= 0.3 {
                continue;
            }
            let u = b0.y - b1.y; // pixels per metre here
            gfx2d::push();
            gfx2d::translate(b0.x, b0.y);
            gfx2d::rotate(lean);
            let part = |x: f32, y: f32, pw: f32, ph: f32, c: u32| rrect(Rect::new(x * u, -(y + ph) * u, pw * u, ph * u), 0.08 * u, c);
            part(-0.45, 0.0, 0.3, 0.8, ui::darken(color, 0.35));
            part(0.15, 0.0, 0.3, 0.8, ui::darken(color, 0.35));
            part(-0.55, 0.75, 1.1, 0.85, color);
            part(-0.25, 1.6, 0.5, 0.4, ui::lighten(color, 0.15));
            let lit = if lean.abs() > 0.5 { 0xff303080 } else { 0x7ff6ffff };
            disc(0.05 * u, -1.8 * u, 0.07 * u, lit);
            gfx2d::pop();
        }
        // Smoke from the champion's mech once it's down.
        if self.fell_at.is_some() {
            for i in 0..7 {
                let k = ((t as f32 * 0.35 + i as f32 / 7.0) % 1.0).max(0.0);
                let p = pr(champ.0 + vec3(0.6 + 0.25 * (k * 9.0 + i as f32).sin(), 0.3 + 3.0 * k, 0.0));
                if p.depth > 0.3 {
                    let r = (0.25 + 0.6 * k) * (h / p.depth) * 0.6;
                    soft_disc(p.x, p.y, r, r * 0.6, with_alpha(0x302838ff, 0.55 * (1.0 - k)));
                }
            }
        }
        self.ending.draw(l);
        if self.ending.finished() {
            text("tap the arrows for another page").size(13.0).color(0xffffffb0).align(Align::Center).draw(180.0, 300.0);
        }
    }

    /// A stand-in arena with the fight's number, the card over it.
    fn draw_stats(&self) {
        let th = &self.theme;
        let l = &self.layout;
        let floor = l.screen.h * 0.62;
        ui::shape::gradient(Rect::new(0.0, floor, l.screen.w, l.screen.h - floor), with_alpha(th.accent, 0.35), with_alpha(th.outline, 0.6));
        for i in 0..9 {
            let x = (i as f32 + 0.5) * 40.0 + (self.t * 20.0) % 40.0 - 20.0;
            gfx2d::line(x, floor, 180.0 + (x - 180.0) * 3.0, l.screen.h, 2.0, with_alpha(th.text, 0.15));
        }
        text(&format!("FIGHT {}", self.fight + 1))
            .font(th.title_font)
            .size(th.fit(th.title_font, 40.0))
            .color(th.text)
            .outline(0.08, th.outline)
            .align(Align::Center)
            .valign(VAlign::Middle)
            .draw(180.0, floor - 60.0);
        let next_is_granny = self.fight % 2 == 0;
        self.stats.draw_with(|r, _| {
            let p = if next_is_granny { &self.granny } else { &self.rocko };
            p.draw(r, "smug", 0, sys::time());
        });
    }

    fn draw_title(&self) {
        // A stand-in scene: bubbles drifting up behind the title.
        let th = &self.theme;
        for i in 0..14 {
            let k = i as f32;
            let x = 30.0 + (k * 97.0) % 300.0 + (self.t * 0.7 + k).sin() * 12.0;
            let y = 640.0 - ((self.t * (18.0 + k * 3.0) + k * 61.0) % 720.0);
            let r = 10.0 + (k * 7.0) % 26.0;
            shape::ring(x, y, r, 2.5, with_alpha(th.text, 0.25));
            soft_disc(x - r * 0.35, y - r * 0.35, r * 0.18, r * 0.2, with_alpha(0xffffffff, 0.35));
        }
        let pulse = 1.0 + 0.05 * (self.t * 4.0).sin();
        soft_disc(180.0, 380.0, 92.0 * pulse, 30.0, with_alpha(th.accent, 0.35));
        disc(180.0, 380.0, 78.0 * pulse, with_alpha(th.accent, 0.85));
        soft_disc(152.0, 352.0, 16.0, 10.0, 0xffffffb0);
        self.title.draw(&self.layout);
    }

    fn draw_widgets(&self) {
        let th = &self.theme;
        for b in &self.buttons {
            b.draw(th);
        }
        let y = 322.0;
        let f = th.body_font;
        let pills: [(&str, Option<Icon>, u32, u32); 3] = [
            ("1,240", Some(Icon::TROPHY), with_alpha(th.outline, 0.5), th.gold),
            ("×3", None, th.accent, th.on_accent),
            ("LOCKED", Some(Icon::LOCK), with_alpha(th.text, 0.15), th.text_dim),
        ];
        let widths: Vec<f32> = pills.iter().map(|p| ui::pill_width(p.0, 32.0, f, p.1.is_some())).collect();
        for (r, p) in ui::stack_h(Rect::new(0.0, y - 16.0, 360.0, 32.0), &widths, 8.0).iter().zip(pills) {
            ui::pill(p.0, r.cx(), y, 32.0, p.2, p.3, f, p.1);
        }
        let p = Rect::new(24.0, 352.0, 312.0, 196.0);
        ui::panel(p, th);
        let k = (self.t * 0.35).fract();
        ui::progress_bar(Rect::new(44.0, 372.0, 272.0, 18.0), k, th.accent, with_alpha(th.outline, 0.45));
        ui::progress_bar(Rect::new(44.0, 400.0, 180.0, 10.0), 1.0 - k, th.good, with_alpha(th.outline, 0.45));
        ui::segments(Rect::new(236.0, 398.0, 80.0, 14.0), 5, 5.0 * (1.0 - k), th.gold, with_alpha(th.outline, 0.45));
        for (i, frac) in [0.25, 0.6, k].into_iter().enumerate() {
            let cx = 70.0 + i as f32 * 62.0;
            ui::ring_meter(cx, 448.0, 24.0, 7.0, frac, [th.accent, th.good, th.gold][i], with_alpha(th.outline, 0.45));
            text(&format!("{}%", (frac * 100.0) as u32)).font(Font::Sans).size(12.0).color(th.text).middle().draw(cx, 448.0);
        }
        // Shadows, glows, strokes.
        let r1 = Rect::new(250.0, 428.0, 66.0, 40.0);
        shape::glow(r1, 12.0, 16.0, with_alpha(th.accent, 0.7));
        rrect(r1, 12.0, th.accent);
        ui::ribbon("NEW BEST!", 180.0, 505.0, 32.0, th.gold, th.outline, th.title_font, -0.04);
        text("panel · bars · segments · rings · glow · ribbon").font(Font::Sans).size(12.0).color(th.text_dim).center().draw(180.0, 556.0);
    }

    fn draw_icons(&self) {
        let th = &self.theme;
        let cols = 6;
        let (cw, ch) = (54.0, 64.0);
        let x0 = 180.0 - cw * cols as f32 / 2.0 + cw / 2.0;
        let y0 = self.layout.safe.y + 92.0;
        for (i, (name, icon)) in Icon::ALL.iter().enumerate() {
            let (cx, cy) = (x0 + (i % cols) as f32 * cw, y0 + (i / cols) as f32 * ch);
            let k = Ease::BackOut.at(stagger(self.page_t, i, 0.02, 0.35));
            if k <= 0.0 {
                continue;
            }
            rrect(Rect::centered(cx, cy, 46.0 * k, 46.0 * k), 12.0, with_alpha(th.outline, 0.35));
            let col = match i % 4 {
                0 => th.text,
                1 => th.accent,
                2 => th.gold,
                _ => th.good,
            };
            icon.draw(cx, cy, 30.0 * k, col);
            text(name).font(Font::Sans).size(10.0).color(th.text_dim).center().draw(cx, cy + 25.0);
        }
    }

    fn draw_text(&self) {
        let th = &self.theme;
        let mut y = self.layout.safe.y + 70.0;
        text("Outline").size(46.0).color(th.text).outline(0.08, th.outline).center().draw(180.0, y);
        y += 56.0;
        text("Soft shadow").size(34.0).color(th.text).soft_shadow(0.0, 5.0, 0.08, th.shadow).center().draw(180.0, y);
        y += 46.0;
        gfx2d::blend(gfx2d::Blend::Add);
        text("Glow").size(36.0).color(th.text).glow(0.1, with_alpha(th.accent, 0.9)).center().draw(110.0, y);
        gfx2d::blend(gfx2d::Blend::Alpha);
        text("Heavy").size(36.0).color(th.gold).weight(0.035).outline(0.04, th.outline).center().draw(250.0, y);
        y += 48.0;
        text("Light weight").font(Font::Sans).size(28.0).weight(-0.02).color(th.text).center().draw(180.0, y);
        y += 40.0;
        text("PIXEL OUTLINE").font(Font::Pixel).size(24.0).color(th.gold).outline(0.125, th.outline).shadow(0.0, 3.0, with_alpha(th.outline, 0.6)).center().draw(180.0, y);
        y += 34.0;
        text("S P A C E D").size(20.0).tracking(0.1).color(th.text_dim).center().draw(180.0, y);
        y += 32.0;
        // Alignment against a guide.
        capsule(180.0, y - 4.0, 180.0, y + 70.0, 2.0, with_alpha(th.accent, 0.6));
        for (i, a) in [Align::Left, Align::Center, Align::Right].into_iter().enumerate() {
            text(["left", "center", "right"][i]).font(Font::Sans).size(18.0).color(th.text).align(a).draw(180.0, y + i as f32 * 24.0);
        }
        y += 80.0;
        let p = Rect::new(36.0, y, 288.0, 92.0);
        ui::panel(p, th);
        text("Wrapped text flows onto new lines to fit the width you give it, centred or not.").font(Font::Sans).size(16.0).color(th.text).wrap(p.w - 32.0).center().valign(VAlign::Middle).draw(p.cx(), p.cy() - 18.0);
        // Every host font covers Latin-1, the pixel font included.
        y += 112.0;
        text("Touché! «Ça va, señor?» — Ünïcødé").font(Font::Sans).size(18.0).color(th.text).center().draw(180.0, y);
        text("TOUCHÉ! ¿Ça va, señor?").font(Font::Pixel).size(16.0).color(th.gold).outline(0.125, th.outline).center().draw(180.0, y + 28.0);
    }

    fn draw_easing(&self) {
        let th = &self.theme;
        let cols = 4;
        let (cw, ch) = (82.0, 60.0);
        let x0 = 180.0 - cw * cols as f32 / 2.0;
        let y0 = self.layout.safe.y + 54.0;
        let k = ((self.t * 0.6).fract() * 1.3).min(1.0);
        for (i, e) in Ease::ALL.iter().enumerate() {
            let (x, y) = (x0 + (i % cols) as f32 * cw, y0 + (i / cols) as f32 * ch);
            let cell = Rect::new(x + 4.0, y + 4.0, cw - 8.0, ch - 8.0);
            rrect(cell, 8.0, with_alpha(th.outline, 0.35));
            let g = cell.inset_sides(8.0, 14.0, 8.0, 10.0);
            let mut pts = Vec::with_capacity(48);
            for j in 0..=40 {
                let u = j as f32 / 40.0;
                pts.extend([g.x + u * g.w, g.bottom() - e.at(u) * g.h]);
            }
            shape::polyline(&pts, 1.6, ui::mix(th.text, th.panel_bottom, 0.2));
            disc(g.x + k * g.w, g.bottom() - e.at(k) * g.h, 3.5, th.accent);
            text(&format!("{e:?}")).font(Font::Sans).size(9.0).color(th.text_dim).draw(cell.x + 5.0, cell.y + 2.0);
        }
    }

    fn draw_motion(&self) {
        let th = &self.theme;
        let top = self.layout.safe.y + 64.0;
        let names = ["critical", "bouncy", "wobbly"];
        for (i, s) in self.springs.iter().enumerate() {
            let y = top + i as f32 * 46.0;
            text(names[i]).font(Font::Sans).size(13.0).color(th.text_dim).valign(VAlign::Middle).draw(20.0, y);
            capsule(100.0, y, 320.0, y, 4.0, with_alpha(th.outline, 0.4));
            let x = 110.0 + s.value * 200.0;
            disc(x, y, 14.0, [th.accent, th.gold, th.good][i]);
        }
        // A sequence: up, hang, dip, bounce down.
        let sy = top + 150.0;
        text("sequence").font(Font::Sans).size(13.0).color(th.text_dim).draw(20.0, sy - 6.0);
        let v = self.seq.value();
        Icon::STAR.draw(150.0, sy + 50.0 - v * 50.0, 30.0 + v * 10.0, th.gold);
        // Stagger.
        text("stagger").font(Font::Sans).size(13.0).color(th.text_dim).draw(200.0, sy - 6.0);
        let lt = (self.t * 0.5).fract() * 2.0;
        for i in 0..6 {
            let k = Ease::BackOut.at(stagger(lt, i, 0.08, 0.4));
            rrect(Rect::centered(215.0 + i as f32 * 20.0, sy + 40.0 - k * 26.0, 14.0, 14.0), 4.0, fade(th.accent, k.clamp(0.0, 1.0)));
        }
        // Counter with punch inside a shaking panel.
        let p = Rect::new(40.0, top + 240.0, 280.0, 150.0);
        gfx2d::push();
        self.shake.apply(p.cx(), p.cy());
        ui::panel(p, th);
        text("counter + punch + shake").font(Font::Sans).size(13.0).color(th.text_dim).center().draw(p.cx(), p.y + 14.0);
        gfx2d::push();
        gfx2d::translate(p.cx(), p.cy() + 10.0);
        let s = self.counter.scale() * self.punch.scale();
        gfx2d::scale(s, s);
        text(&ui::fmt_int(self.counter.value())).font(th.number_font).size(th.fit(th.number_font, 56.0)).color(th.gold).outline(th.outline_em, th.outline).middle().draw(0.0, 0.0);
        gfx2d::pop();
        gfx2d::pop();
        text("tap to punch and shake").font(Font::Sans).size(13.0).color(th.text_dim).center().draw(180.0, p.bottom() + 16.0);
    }

    fn draw_juice(&self) {
        let th = &self.theme;
        self.fx.draw();
        self.popups.draw();
        self.combo.draw(300.0, self.layout.safe.y + 100.0, th);
        text("confetti · sparkles · stars · burst · sparks · cannons").font(Font::Sans).size(12.0).color(th.text_dim).center().draw(180.0, 530.0);
        text("tap for more").font(Font::Sans).size(12.0).color(th.text_dim).center().draw(180.0, 548.0);
        self.vignette.draw(self.layout.screen);
        self.flash.draw(self.layout.screen);
    }

    /// The target range: targets, the crosshair where the phone points, and what the sensor says.
    fn draw_aim(&self) {
        let th = &self.theme;
        let l = &self.layout;
        let field = self.range();
        rrect(field.inset(-24.0), 18.0, with_alpha(th.text, 0.05));
        // The middle: where the crosshair rests after RECENTER.
        let (cx, cy) = (l.screen.w * 0.5, l.screen.h * 0.5);
        rrect(Rect::centered(cx, cy, 14.0, 2.0), 1.0, with_alpha(th.text, 0.25));
        rrect(Rect::centered(cx, cy, 2.0, 14.0), 1.0, with_alpha(th.text, 0.25));
        let (x, y) = self.scope.position();
        let over = self.targets.iter().any(|t| (t.0 - x).hypot(t.1 - y) < HIT_R);
        for &(tx, ty, age) in &self.targets {
            let k = Ease::BackOut.at((age / 0.35).min(1.0));
            let r = TARGET_R * k;
            soft_disc(tx, ty + 4.0, r * 1.05, 8.0, with_alpha(0x000000ff, 0.25));
            disc(tx, ty, r, th.accent);
            disc(tx, ty, r * 0.68, 0xf4f4f4ff);
            disc(tx, ty, r * 0.4, th.accent);
            disc(tx, ty, r * 0.14, 0xf4f4f4ff);
        }
        self.fx.draw();
        self.popups.draw();
        // The crosshair: gold over a target.
        let c = if over { th.gold } else { th.text };
        shape::ring(x, y, 17.0, 2.5, with_alpha(c, 0.95));
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let (w, h) = if dx != 0.0 { (10.0, 2.5) } else { (2.5, 10.0) };
            rrect(Rect::centered(x + dx * 24.0, y + dy * 24.0, w, h), 1.2, c);
        }
        disc(x, y, 2.2, c);
        // What the sensor says.
        let (yaw, pitch) = self.scope.angles();
        let (name, hint) = match self.scope.source() {
            Some(AimSource::Attitude) => ("GYRO", "point the phone · tap to fire"),
            Some(AimSource::Tilt) => ("TILT ONLY", "no gyroscope: tip to aim · tap to fire"),
            _ => ("NO SENSOR", "drag to aim · tap to fire"),
        };
        let top = l.safe.y + 58.0;
        let dim = with_alpha(th.text, 0.75);
        let pw = ui::pill_width(name, 26.0, Font::Sans, false);
        ui::pill(name, 16.0 + pw * 0.5, top, 26.0, if name == "GYRO" { th.good } else { th.accent }, th.on_accent, Font::Sans, None);
        text(&format!("yaw {yaw:+.1}°   pitch {pitch:+.1}°")).font(Font::Sans).size(14.0).color(th.text).valign(VAlign::Middle).draw(26.0 + pw, top);
        text(&format!("turning {:.0}°/s · {:.0} fps · {:.0}° across", self.turn_rate, self.fps, self.scope.gain)).font(Font::Sans).size(11.0).color(dim).draw(16.0, top + 20.0);
        // The filter, its smoothing right now (the one-euro filter's shrinks as the phone turns
        // faster) and the gain's multiplier right now (slow turns and friction lower it).
        let filter = match self.scope.smoothing {
            Smoothing::Fixed(_) => "FIXED",
            Smoothing::Adaptive { .. } => "ONE-EURO",
        };
        let mode = if self.precise_aim { "PRECISE" } else { "DEFAULT" };
        let (ms, k) = (self.scope.time_constant() * 1000.0, self.scope.gain_scale());
        text(&format!("{mode} · {filter} smoothing {ms:.0} ms · gain ×{k:.2}")).font(Font::Sans).size(11.0).color(th.text).draw(16.0, top + 36.0);
        let acc = if self.shots > 0 { 100 * self.hits / self.shots } else { 0 };
        text(&format!("hits {} / {}  ({acc}%)", self.hits, self.shots)).font(Font::Sans).size(11.0).color(dim).right().draw(344.0, top + 20.0);
        text(hint).font(Font::Sans).size(12.0).color(th.text_dim).center().draw(180.0, self.recenter.rect.y - 17.0);
        self.recenter.draw(th);
        self.aim_mode.draw(th);
        self.flash.draw(l.screen);
    }

    fn draw_hud(&self) {
        let th = &self.theme;
        let l = &self.layout;
        // Shade the pause-pill zone the HUD keeps clear.
        let z = l.pill_zone();
        rrect(z.inset(4.0), 10.0, with_alpha(th.text, 0.08));
        text("pause pill").font(Font::Sans).size(10.0).color(th.text_dim).middle().draw(z.cx(), z.cy());
        self.hud.draw(l);
        self.combo.draw(64.0, l.safe.y + 130.0, th);
        self.countdown.draw(l, th);
    }
}

export_game!(Gallery);
