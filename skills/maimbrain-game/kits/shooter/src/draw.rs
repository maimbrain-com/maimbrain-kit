//! Drawing: the generated art (`assets/sprites.png`, packed by `mb art atlas`
//! with its source rects in `sprites.rs`, and `assets/backdrop.png`) plus
//! what code does better: the scrolling neon grid, faces whose eyes follow
//! you, glows, the queen's tentacles, bullets, particles and the UI kit.
//! Reads the game, changes nothing.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use maimbrain::gfx2d::{self, Blend, Image};
use maimbrain::motion::Ease;
use maimbrain::rounds::Stage;
use maimbrain::sys::{Asset, AssetState};
use maimbrain::ui::{self, Icon, Rect, fade, lighten, mix, shape, text, with_alpha};

use crate::look::{self, *};
use crate::sim::*;
use crate::sprites as spr;
use crate::Shooter;

const WHITE: u32 = 0xffffffff;

// ---- Where things sit in the generated art -----------------------------------
// Sizes are the logical units each sprite is drawn at (its PNG is 2 texels per
// unit). Offsets are measured from each sprite's center to its dark face
// window, where the code draws eyes; `tools/measure_faces.sh` prints them for
// freshly generated sprites (texels / 2 at these sizes).

/// The backdrop's horizon line (texel 528 of 960): the grid starts here.
pub const HORIZON: f32 = 352.0;
const BACKDROP_TEXELS: [f32; 4] = [0.0, 0.0, 540.0, 960.0];
/// Pip: 64 units square; the visor window's center; the engine nozzle.
const PIP_SIZE: f32 = 64.0;
const PIP_VISOR: (f32, f32) = (0.0, -4.4);
const PIP_NOZZLE: f32 = 25.0;
const PIP_WINGTIP: (f32, f32) = (24.0, 8.0);
/// Enemies: drawn size, and where the face window is relative to the body.
const JELLY_SIZE: f32 = 46.0;
const JELLY_FACE_Y: f32 = -4.5;
const SWOOPER_SIZE: f32 = 56.0;
/// The swooper sheet's 4 flap frames: face y in each (the body bobs as it flaps).
const SWOOPER_FACE_Y: [f32; 4] = [15.1, 15.1, 10.9, 15.1];
const SWOOPER_LIFT: f32 = 13.0;
const DARTER_SIZE: f32 = 48.0;
const DARTER_EYE: f32 = 0.8;
const BULB_SIZE: f32 = 64.0;
const SPINNER_SIZE: f32 = 58.0;
/// The queen: her dome sprite is 160 units wide, drawn this far above her
/// center so the hit ellipse sits on the dome; the face plate's center.
const BOSS_SIZE: f32 = 160.0;
const BOSS_LIFT: f32 = 6.0;
const BOSS_FACE: (f32, f32) = (-0.5, 1.5);
const HAND_SIZE: f32 = 30.0;
const GEM_SIZE: f32 = 28.0;
const POWER_SIZE: f32 = 38.0;
const HEART_SIZE: f32 = 34.0;

/// The generated images, decoded once their assets are ready.
pub struct Art {
    sheet: Asset,
    back: Asset,
    pub img: Option<Image>,
    pub bg: Option<Image>,
}

impl Art {
    pub fn new() -> Art {
        Art { sheet: Asset::load("assets/sprites.png"), back: Asset::load("assets/backdrop.png"), img: None, bg: None }
    }

    /// Called from `update` (render changes nothing).
    pub fn poll(&mut self) {
        if self.img.is_none() && self.sheet.state() == AssetState::Ready {
            self.img = Image::new(self.sheet);
        }
        if self.bg.is_none() && self.back.state() == AssetState::Ready {
            self.bg = Image::new(self.back);
        }
    }
}

/// Atlas rect `src` centered at (x, y), `w` × `h` units, tinted (multiplied).
fn sprite_wh(img: Image, src: [f32; 4], x: f32, y: f32, w: f32, h: f32, tint: u32) {
    gfx2d::sprite(img, src, [x - w * 0.5, y - h * 0.5, w, h], tint);
}

/// Atlas rect `src` centered at (x, y), `w` units wide (height keeps the aspect).
fn sprite(img: Image, src: [f32; 4], x: f32, y: f32, w: f32, tint: u32) {
    sprite_wh(img, src, x, y, w, w * src[3] / src[2], tint);
}

/// The same sprite again, added on top: a white hit flash of strength `k`.
fn flash_over(img: Image, src: [f32; 4], x: f32, y: f32, w: f32, h: f32, k: f32) {
    if k > 0.02 {
        gfx2d::blend(Blend::Add);
        sprite_wh(img, src, x, y, w, h, with_alpha(WHITE, k.min(1.0)));
        gfx2d::blend(Blend::Alpha);
    }
}

/// A neon tube through `pts`: a wide additive glow, the tube, a white-hot core.
fn neon(pts: &[f32], width: f32, color: u32, a: f32) {
    gfx2d::blend(Blend::Add);
    shape::polyline(pts, width * 2.6, with_alpha(color, 0.18 * a));
    gfx2d::blend(Blend::Alpha);
    shape::polyline(pts, width, fade(color, a));
    shape::polyline(pts, width * 0.38, fade(lighten(color, 0.7), a));
}

pub fn frame(g: &Shooter) {
    let s = &g.sim;
    let l = &g.rounds.layout;
    background(g);
    gfx2d::push();
    g.shake.apply(W / 2.0, H / 2.0);
    for d in &s.drops {
        drop(g, d);
    }
    for e in &s.enemies {
        enemy(g, e);
    }
    if let Some(b) = &s.boss {
        boss(g, b);
    }
    shots(s);
    ship(g);
    bullets(s);
    if let (Some(_), Some((x, y)), false) = (s.dying, s.killer, s.over) {
        // Why you died: a ring where the fatal hit landed.
        let k = (g.time * 10.0).sin() * 0.5 + 0.5;
        shape::ring(x, y, 16.0 + 5.0 * k, 3.0, DANGER);
    }
    g.fx.draw();
    gfx2d::pop();
    g.popups.draw();
    g.flash.draw(l.screen);
    g.hurt.draw(l.screen);
    match g.rounds.stage() {
        Stage::Title => {
            g.rounds.draw();
            g.daily_btn.draw(&g.theme);
        }
        Stage::Playing | Stage::Ending => {
            g.hud.draw(l);
            combo(g);
            powers(g);
            boss_bar(g);
            banner(g);
            if g.show_hint() {
                hint(g);
            }
            if g.rounds.is_daily() {
                // Under the pause pill's corner, clear of the score.
                ui::pill("DAILY", 50.0, l.pill_zone().bottom() + 14.0, 24.0, with_alpha(INK, 0.6), WHITE, g.theme.body_font, Some(Icon::STAR));
            }
        }
        Stage::Over => {
            g.rounds.draw();
            g.scores_btn.draw(&g.theme);
            g.daily_btn.draw(&g.theme);
        }
    }
}

/// A cheap integer hash → 0…1 (star positions; no state, no RNG).
fn hash(i: u32, salt: u32) -> f32 {
    let mut v = i.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^= v >> 12;
    (v & 0xffff) as f32 / 65536.0
}

/// The generated synthwave backdrop (sky, sun, wireframe mountains), dimmed a
/// little so bullets pop and warming with the heat; over it, in code, a neon
/// floor grid rushing toward you, twinkling stars and speed streaks.
fn background(g: &Shooter) {
    let s = &g.sim;
    let hot = ((s.heat() - 1.0) / 1.0).clamp(0.0, 1.0);
    shape::gradient(g.rounds.layout.screen, BG_TOP, BG_BOTTOM);
    if let Some(bg) = g.art.bg {
        gfx2d::sprite(bg, BACKDROP_TEXELS, [0.0, 0.0, W, H], mix(BG_TINT, BG_HOT_TINT, hot));
    }
    let t = g.time;
    let speed = 1.0 + 0.6 * hot;
    let gc = mix(GRID, GRID_HOT, hot);
    gfx2d::blend(Blend::Add);
    // The horizon's glow.
    gfx2d::rect_gradient(0.0, HORIZON - 14.0, W, 14.0, with_alpha(HORIZON_GLOW, 0.0), with_alpha(HORIZON_GLOW, 0.3));
    gfx2d::rect_gradient(0.0, HORIZON, W, 40.0, with_alpha(HORIZON_GLOW, 0.28), with_alpha(HORIZON_GLOW, 0.0));
    // Cross lines, spaced by perspective, scrolling toward you.
    let depth = H - HORIZON;
    let n = 10;
    let phase = (t * 0.6 * speed).fract();
    for i in 0..n {
        let d = (i as f32 + phase) / n as f32;
        let y = HORIZON + depth * d * d;
        let w = 0.8 + 1.6 * d;
        gfx2d::rect(0.0, y - w * 0.5, W, w, with_alpha(gc, 0.08 + 0.3 * d));
    }
    // Rails converging on the vanishing point, brighter toward you.
    for i in -8i32..=8 {
        let k = i as f32;
        let (x0, x1) = (W / 2.0 + k * 5.0, W / 2.0 + k * 62.0);
        for (f0, f1, a) in [(0.0f32, 0.35f32, 0.1f32), (0.35, 0.7, 0.2), (0.7, 1.0, 0.3)] {
            let (ya, yb) = (HORIZON + depth * f0, HORIZON + depth * f1);
            let (xa, xb) = (x0 + (x1 - x0) * f0, x0 + (x1 - x0) * f1);
            gfx2d::line(xa, ya, xb, yb, 0.8 + 1.2 * f1, with_alpha(gc, a));
        }
    }
    // Stars twinkling in the sky.
    for i in 0..28u32 {
        let x = hash(i, 101) * W;
        let y = hash(i, 202) * (HORIZON - 30.0);
        let twinkle = 0.5 + 0.5 * (t * (1.2 + hash(i, 303) * 3.0) + i as f32).sin();
        let size = 1.0 + hash(i, 404);
        gfx2d::rect(x, y, size, size, fade(STAR, 0.8 * twinkle));
    }
    // Speed streaks falling past, faster as the heat rises.
    for i in 0..12u32 {
        let x = hash(i, 505) * W;
        let y = (hash(i, 606) * H + t * (150.0 + 90.0 * hash(i, 707)) * speed) % (H + 40.0) - 20.0;
        gfx2d::rect(x, y, 1.5, 8.0 + 10.0 * speed, fade(STREAK, 0.22));
    }
    gfx2d::blend(Blend::Alpha);
}

// ---- Pip --------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mood {
    Calm,
    Worried,
    Happy,
    Ouch,
    Dead,
}

fn ship(g: &Shooter) {
    let s = &g.sim;
    let sh = &s.ship;
    // After the round, Pip's sooty wreck floats from where it fell into the
    // gap between the results panel and the buttons, tumbling and smoking.
    let wreck = s.over;
    let dying = s.dying.is_some();
    let (mut x, mut y) = (sh.x, sh.y);
    if let (true, Stage::Over, Some(card)) = (wreck, g.rounds.stage(), &g.rounds.results) {
        // Seconds the card has been up.
        let t = g.rounds.age();
        let spot_y = (card.panel_rect().bottom() + 40.0 + g.rounds.layout.card.bottom() - 46.0) / 2.0;
        let k = Ease::CubicInOut.at((t / 1.6).min(1.0));
        x += (x.clamp(80.0, W - 110.0) - x) * k;
        y += (spot_y + 4.0 * (t * 1.3).sin() - y) * k;
        for i in 0..3 {
            let k = (g.time * 0.7 + i as f32 / 3.0).fract();
            let px = x + (g.time * 2.0 + i as f32 * 2.1).sin() * 7.0 * k;
            shape::soft_disc(px, y - 12.0 - k * 40.0, 5.0 + k * 10.0, 8.0, with_alpha(0x8a80a0ff, 0.45 * (1.0 - k)));
        }
    }
    let blink = sh.invuln > 0.0 && !dying && ((g.time * 14.0) as i32) % 2 == 0;
    let a = if blink { 0.28 } else { 1.0 };
    let tilt = if dying { sh.spin } else { (sh.vx * 0.0016).clamp(-0.35, 0.35) };
    gfx2d::push();
    gfx2d::translate(x, y);
    gfx2d::rotate(tilt);
    let k = g.ship_punch.scale();
    gfx2d::scale(k, k);
    if !dying && !wreck {
        // A cyan under-glow, and the exhaust: three nested flames, additive.
        let flick = (g.time * 43.0).sin() * 3.0 + (g.time * 71.0).sin() * 2.0;
        let n = PIP_NOZZLE;
        gfx2d::blend(Blend::Add);
        shape::soft_disc(0.0, 0.0, 34.0, 24.0, fade(with_alpha(SHIP_TRIM, 0.13), a));
        shape::soft_disc(0.0, n + 4.0, 11.0, 9.0, fade(with_alpha(FLAME_OUT, 0.4), a));
        // A longer burn while the thumb is on the glass: you're steering.
        let len = 15.0 + flick + if s.grab.is_some() { 7.0 } else { 0.0 };
        shape::poly(&[-5.5, n, 5.5, n, 0.0, n + len], fade(with_alpha(FLAME_EDGE, 0.75), a));
        shape::poly(&[-3.6, n, 3.6, n, 0.0, n + len * 0.78], fade(FLAME_OUT, a));
        shape::poly(&[-1.8, n, 1.8, n, 0.0, n + len * 0.45], fade(FLAME_IN, a));
        gfx2d::blend(Blend::Alpha);
    }
    let tint = if wreck { WRECK_TINT } else { fade(WHITE, a) };
    match g.art.img {
        Some(img) => sprite(img, spr::PIP, 0.0, 0.0, PIP_SIZE, tint),
        None => shape::disc(0.0, 0.0, 18.0, fade(SHIP_TRIM_DARK, a)),
    }
    if sh.spread > 0 && !wreck {
        // Little cannons glowing on the wing tips while Spread is on.
        for side in [-1.0f32, 1.0] {
            let (wx, wy) = (side * PIP_WINGTIP.0, PIP_WINGTIP.1);
            shape::capsule(wx, wy, wx, wy - 10.0, 4.0, fade(SHOT, a));
        }
    }
    let mood = if dying || wreck {
        Mood::Dead
    } else if sh.invuln > INVULN_TIME - 0.5 {
        Mood::Ouch
    } else if sh.happy > 0.0 {
        Mood::Happy
    } else if sh.danger > 0.45 {
        Mood::Worried
    } else {
        Mood::Calm
    };
    // Eyes look at the nearest bullet (or up the screen).
    let look = s
        .bullets
        .iter()
        .map(|b| (b.x - sh.x, b.y - sh.y))
        .min_by(|p, q| (p.0 * p.0 + p.1 * p.1).total_cmp(&(q.0 * q.0 + q.1 * q.1)))
        .filter(|p| p.0 * p.0 + p.1 * p.1 < 140.0 * 140.0)
        .unwrap_or((0.0, -1.0));
    face(PIP_VISOR.0, PIP_VISOR.1, mood, look, g.time, if wreck { 0.6 } else { a });
    gfx2d::pop();
    if dying || wreck {
        return;
    }
    if sh.shield {
        let k = (g.time * 5.0).sin() * 0.5 + 0.5;
        gfx2d::blend(Blend::Add);
        shape::soft_disc(sh.x, sh.y, 36.0, 8.0, with_alpha(SHIELD, 0.14 + 0.06 * k));
        gfx2d::blend(Blend::Alpha);
        shape::ring(sh.x, sh.y, 36.0 + k, 2.5, with_alpha(SHIELD, 0.8));
        shape::arc(sh.x, sh.y, 33.0, 3.0, g.time * 2.0, g.time * 2.0 + 0.9, with_alpha(WHITE, 0.7));
        shape::arc(sh.x, sh.y, 33.0, 3.0, g.time * 2.0 + PI, g.time * 2.0 + PI + 0.9, with_alpha(SHIP_TRIM, 0.7));
    }
    if sh.magnet_t > 0.0 {
        let k = (g.time * 1.6).fract();
        shape::ring(sh.x, sh.y, 40.0 + 60.0 * (1.0 - k), 2.0, with_alpha(power_color(Power::Magnet), 0.35 * k));
    }
}

/// Pip's face in its dark visor (about 16 × 14 units): two tall white eyes.
fn face(cx: f32, cy: f32, mood: Mood, look: (f32, f32), t: f32, a: f32) {
    let ex = 3.7;
    let d = (look.0 * look.0 + look.1 * look.1).sqrt().max(1e-3);
    let (lx, ly) = (look.0 / d * 1.0, look.1 / d * 1.1);
    let white = fade(EYE, a);
    let ink = fade(INK, a);
    match mood {
        Mood::Calm | Mood::Worried => {
            let worried = mood == Mood::Worried;
            let blinking = !worried && (t % 3.3) < 0.12;
            for side in [-1.0f32, 1.0] {
                let x = cx + side * ex;
                if blinking {
                    shape::capsule(x - 2.0, cy, x + 2.0, cy, 1.5, white);
                } else {
                    let (h, w) = if worried { (1.9, 4.8) } else { (1.4, 4.2) };
                    shape::capsule(x, cy - h, x, cy + h, w, white);
                    shape::disc(x + lx, cy + ly, if worried { 1.0 } else { 1.35 }, ink);
                }
                if worried {
                    // Brows up in the middle.
                    shape::capsule(x - side * 0.6, cy - 5.6, x + side * 2.6, cy - 4.2, 1.2, fade(SHIP_TRIM, a));
                }
            }
            if worried {
                let k = (t * 3.0).fract();
                shape::disc(cx + 10.5, cy - 3.0 + k * 6.0, 1.8, fade(0x8fd8ffff, a * (1.0 - k)));
            }
        }
        Mood::Happy => {
            for side in [-1.0f32, 1.0] {
                let x = cx + side * ex;
                shape::polyline(&[x - 2.4, cy + 1.0, x, cy - 1.8, x + 2.4, cy + 1.0], 1.5, white);
            }
        }
        Mood::Ouch => {
            for side in [-1.0f32, 1.0] {
                let x = cx + side * ex;
                shape::polyline(&[x - side * 2.4, cy - 2.4, x + side * 1.6, cy, x - side * 2.4, cy + 2.4], 1.4, white);
            }
        }
        Mood::Dead => {
            for side in [-1.0f32, 1.0] {
                let x = cx + side * ex;
                shape::capsule(x - 2.0, cy - 2.0, x + 2.0, cy + 2.0, 1.4, white);
                shape::capsule(x - 2.0, cy + 2.0, x + 2.0, cy - 2.0, 1.4, white);
            }
        }
    }
}

// ---- Enemies ----------------------------------------------------------------

/// Two eyes at (x ± dx, y) looking at the ship, with angry neon brows.
fn eyes(x: f32, y: f32, dx: f32, r: f32, target: (f32, f32), brow: u32) {
    for side in [-1.0f32, 1.0] {
        let ex = x + side * dx;
        let (vx, vy) = (target.0 - ex, target.1 - y);
        let d = (vx * vx + vy * vy).sqrt().max(1e-3);
        shape::disc(ex, y, r, EYE);
        shape::disc(ex + vx / d * r * 0.4, y + vy / d * r * 0.4, r * 0.58, INK);
        // The brow cuts down across the top of the eye: a glare.
        shape::capsule(ex - side * r * 1.25, y - r * 1.45, ex + side * r * 0.9, y - r * 0.55, r * 0.75, brow);
    }
}

fn enemy(g: &Shooter, e: &Enemy) {
    let s = &g.sim;
    let target = (s.ship.x, s.ship.y);
    let base = ENEMY[e.kind.index()];
    let brow = lighten(base, 0.25);
    let r = e.kind.radius();
    let (x, y) = (e.x, e.y);
    let fl = e.flash * 0.85;
    // The tell: a glow that swells before it fires.
    let tell = if e.telegraph() && e.y < 430.0 && s.alive() && s.t > GRACE { 1.0 - e.fire_cd / TELL } else { 0.0 };
    gfx2d::blend(Blend::Add);
    shape::soft_disc(x, y, r * 1.7, r * 1.1, with_alpha(base, 0.2 + 0.25 * e.flash));
    gfx2d::blend(Blend::Alpha);
    let Some(img) = g.art.img else {
        shape::disc(x, y, r, base);
        return;
    };
    let mut mouth_y = y + r * 0.7;
    match e.kind {
        Kind::Jelly => {
            // Squash and stretch on a bob; the bell sits on the body's center.
            let bob = (e.age * 5.0 + e.x0).sin();
            let (w, h) = (JELLY_SIZE * (1.0 + 0.06 * bob), JELLY_SIZE * (1.0 - 0.06 * bob));
            let cy = y - JELLY_FACE_Y * h / JELLY_SIZE;
            sprite_wh(img, spr::JELLY, x, cy, w, h, WHITE);
            flash_over(img, spr::JELLY, x, cy, w, h, fl);
            eyes(x, y, 3.8, 3.1, target, brow);
        }
        Kind::Swooper => {
            let f = ((e.age * 12.0) as usize) % 4;
            let cell = spr::SWOOPER[3];
            let src = [spr::SWOOPER[0] + f as f32 * cell, spr::SWOOPER[1], cell, cell];
            let cy = y - SWOOPER_LIFT;
            sprite(img, src, x, cy, SWOOPER_SIZE, WHITE);
            flash_over(img, src, x, cy, SWOOPER_SIZE, SWOOPER_SIZE, fl);
            eyes(x, cy + SWOOPER_FACE_Y[f], 2.9, 2.3, target, brow);
        }
        Kind::Darter => {
            // Points where it's going; a line shows where it will dash.
            let ang = if e.stage == 0 { FRAC_PI_2 } else { (e.aim.1 - y).atan2(e.aim.0 - x) };
            if e.stage == 1 || e.stage == 2 {
                let locked = e.stage == 2;
                let on = !locked || ((g.time * 16.0) as i32) % 2 == 0;
                let col = if locked { with_alpha(DANGER, if on { 0.85 } else { 0.35 }) } else { with_alpha(DANGER, 0.22) };
                let (ux, uy) = (ang.cos(), ang.sin());
                let mut d = 24.0;
                while d < 700.0 {
                    shape::capsule(x + ux * d, y + uy * d, x + ux * (d + 12.0), y + uy * (d + 12.0), if locked { 3.5 } else { 2.0 }, col);
                    d += 26.0;
                }
            }
            if e.stage == 3 {
                // Afterimages behind the dash.
                gfx2d::blend(Blend::Add);
                for k in 1..4 {
                    let f = k as f32 * 0.025;
                    gfx2d::push();
                    gfx2d::translate(x - e.vx * f, y - e.vy * f);
                    gfx2d::rotate(ang);
                    sprite(img, spr::DARTER, 0.0, 0.0, DARTER_SIZE, with_alpha(base, 0.35 / k as f32));
                    gfx2d::pop();
                }
                gfx2d::blend(Blend::Alpha);
            }
            gfx2d::push();
            gfx2d::translate(x, y);
            gfx2d::rotate(ang);
            sprite(img, spr::DARTER, 0.0, 0.0, DARTER_SIZE, WHITE);
            flash_over(img, spr::DARTER, 0.0, 0.0, DARTER_SIZE, DARTER_SIZE, fl);
            // One eye, looking where it's headed.
            shape::disc(DARTER_EYE, 0.0, 3.6, EYE);
            shape::disc(DARTER_EYE + 1.3, 0.0, 1.9, INK);
            gfx2d::pop();
            if e.stage == 2 {
                text("!").size(22.0).color(DANGER).outline(0.12, INK).middle().draw(x, y - r - 14.0);
            }
        }
        Kind::Bulb => {
            sprite(img, spr::BULB, x, y, BULB_SIZE, WHITE);
            flash_over(img, spr::BULB, x, y, BULB_SIZE, BULB_SIZE, fl);
            // One big eye on the ship, a brow, and a mouth that opens as it charges.
            let (vx, vy) = (target.0 - x, target.1 - (y - 3.0));
            let d = (vx * vx + vy * vy).sqrt().max(1e-3);
            shape::disc(x, y - 3.0, 7.0, EYE);
            shape::disc(x + vx / d * 2.6, y - 3.0 + vy / d * 2.6, 4.0, hex_ui(0x2a4fd0));
            shape::disc(x + vx / d * 3.2, y - 3.0 + vy / d * 3.2, 2.0, INK);
            shape::capsule(x - 8.0, y - 11.5, x + 8.0, y - 10.0, 2.8, brow);
            let open = 1.6 + 4.4 * tell;
            gfx2d::rrect(x - 5.0, y + 7.0 - open * 0.3, 10.0, open, open * 0.5, 0.0, 0.0, mix(brow, BULLET_BIG_RIM, tell), brow);
            mouth_y = y + 7.0;
            hp_arc(x, y, r + 7.0, e.hp / e.max_hp, base);
        }
        Kind::Spinner => {
            gfx2d::push();
            gfx2d::translate(x, y);
            gfx2d::rotate(e.spin);
            sprite(img, spr::SPINNER, 0.0, 0.0, SPINNER_SIZE, WHITE);
            flash_over(img, spr::SPINNER, 0.0, 0.0, SPINNER_SIZE, SPINNER_SIZE, fl);
            gfx2d::pop();
            eyes(x, y + 0.5, 3.8, 2.9, target, brow);
            hp_arc(x, y, r + 8.0, e.hp / e.max_hp, base);
        }
    }
    if tell > 0.0 {
        gfx2d::blend(Blend::Add);
        shape::soft_disc(x, mouth_y, 6.0 + 10.0 * tell, 8.0, with_alpha(BULLET_RIM, 0.55 * tell));
        gfx2d::blend(Blend::Alpha);
        shape::disc(x, mouth_y, 2.0 + 3.0 * tell, with_alpha(WHITE, tell));
    }
}

fn hex_ui(rgb: u32) -> u32 {
    ui::hex(rgb)
}

fn hp_arc(x: f32, y: f32, r: f32, frac: f32, color: u32) {
    if frac < 0.999 {
        shape::arc(x, y, r, 3.0, 0.0, TAU, with_alpha(INK, 0.6));
        shape::arc(x, y, r, 3.0, 0.0, TAU * frac.max(0.0), lighten(color, 0.3));
    }
}

fn boss(g: &Shooter, b: &Boss) {
    let s = &g.sim;
    let t = g.time;
    let dying = b.dying.unwrap_or(0.0);
    // The sprite is multiplied by `tint`: red when enraged, flickering as she dies.
    let mut tint = WHITE;
    if b.enraged() {
        tint = mix(WHITE, 0xff8a9aff, 0.35 + 0.2 * (t * 9.0).sin());
    }
    if b.dying.is_some() {
        tint = mix(tint, if (t * 20.0) as i32 % 2 == 0 { WHITE } else { 0x7050b0ff }, (dying / BOSS_DEATH).min(1.0) * 0.7);
    }
    let neon_a = if b.dying.is_some() && (t * 20.0) as i32 % 2 == 1 { 0.5 } else { 1.0 };
    let (x, y) = (b.x, b.y);
    gfx2d::blend(Blend::Add);
    shape::soft_disc(x, y, 125.0, 80.0, with_alpha(BOSS, 0.22));
    gfx2d::blend(Blend::Alpha);
    // Tentacles: neon tubes swaying under the frill.
    for i in 0..7 {
        let k = i as f32 - 3.0;
        let tx = x + k * 17.0;
        let w = (t * 4.0 + i as f32 * 0.9).sin() * 9.0;
        let col = if i % 2 == 0 { BOSS_NEON } else { BOSS_NEON_2 };
        neon(&[tx, y + 34.0, tx + w * 0.5, y + 58.0, tx - w * 0.4, y + 84.0], 5.5 - k.abs() * 0.6, col, neon_a);
    }
    // Arms out to the hands (they fire the Rain).
    let rain = b.attack == Attack::Rain && b.attack_t > CHARGE * 0.5;
    let mut hands = [(0.0, 0.0); 2];
    for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let hx = x + side * BOSS_HAND.0;
        let hy = y + BOSS_HAND.1 + (t * 3.0 + side).sin() * 3.0;
        let (mx, my) = (x + side * 74.0, y + 2.0 + (t * 3.0 + side).cos() * 2.0);
        neon(&[x + side * 56.0, y + 14.0, mx, my, hx, hy], 5.0, BOSS_NEON, neon_a);
        hands[i] = (hx, hy);
    }
    let Some(img) = g.art.img else {
        shape::disc(x, y, BOSS_RX, BOSS);
        return;
    };
    for (hx, hy) in hands {
        if rain {
            gfx2d::blend(Blend::Add);
            shape::soft_disc(hx, hy, 24.0, 12.0, with_alpha(BULLET_RIM, 0.6));
            gfx2d::blend(Blend::Alpha);
        }
        sprite(img, spr::HAND, hx, hy, HAND_SIZE, tint);
    }
    // The crowned dome.
    let (dx, dy) = (x, y - BOSS_LIFT);
    let dh = BOSS_SIZE * spr::BOSS[3] / spr::BOSS[2];
    sprite(img, spr::BOSS, dx, dy, BOSS_SIZE, tint);
    flash_over(img, spr::BOSS, dx, dy, BOSS_SIZE, dh, b.flash * 0.7);
    // Eyes, brows and the mouth that opens to charge, in the face plate.
    let (fx, fy) = (x + BOSS_FACE.0, y + BOSS_FACE.1);
    let target = (s.ship.x, s.ship.y);
    for side in [-1.0f32, 1.0] {
        let ex = fx + side * 8.5;
        let ey = fy - 5.0;
        let (vx, vy) = (target.0 - ex, target.1 - ey);
        let d = (vx * vx + vy * vy).sqrt().max(1e-3);
        shape::disc(ex, ey, 6.0, EYE);
        let pupil = if b.enraged() { DANGER } else { INK };
        shape::disc(ex + vx / d * 2.4, ey + vy / d * 2.4, 3.1, pupil);
        shape::capsule(ex - side * 7.5, ey - 10.5, ex + side * 3.5, ey - 6.5, 3.4, BOSS_NEON);
    }
    let charge = b.charge();
    let firing = !b.entering() && b.dying.is_none() && b.attack_t > CHARGE && b.attack_t < CHARGE + ATTACK_FIRE;
    let open = if charge > 0.0 { 2.5 + 9.0 * charge } else if firing { 7.0 } else { 2.5 };
    let my = fy + 10.0;
    gfx2d::rrect(fx - 9.0, my - open * 0.4, 18.0, open, open * 0.5, 0.0, 0.0, mix(BOSS_NEON, WHITE, charge.max(firing as u8 as f32 * 0.6)), BOSS_NEON);
    if charge > 0.0 {
        // Rings closing in on the mouth: the attack is coming.
        let rr = 10.0 + 50.0 * (1.0 - charge);
        shape::ring(fx, my, rr, 3.0, with_alpha(BULLET_RIM, charge));
    }
}

// ---- Bullets, shots, pickups ------------------------------------------------

/// Enemy bullets: a glow, then a dark outline, the hot rim and a white core,
/// so they read on the sun, the grid and the swarm alike.
fn bullets(s: &Sim) {
    gfx2d::blend(Blend::Add);
    for b in &s.bullets {
        let rim = if b.big { BULLET_BIG_RIM } else { BULLET_RIM };
        shape::soft_disc(b.x, b.y, b.r * 2.3, b.r * 1.5, with_alpha(rim, 0.34));
    }
    gfx2d::blend(Blend::Alpha);
    for b in &s.bullets {
        let pulse = 1.0 + 0.08 * (b.age * 14.0).sin();
        shape::disc(b.x, b.y, b.r * 1.45 * pulse + 1.8, BULLET_OUTLINE);
    }
    for b in &s.bullets {
        let rim = if b.big { BULLET_BIG_RIM } else { BULLET_RIM };
        let pulse = 1.0 + 0.08 * (b.age * 14.0).sin();
        shape::disc(b.x, b.y, b.r * 1.45 * pulse, rim);
        shape::disc(b.x, b.y, b.r * 0.82, BULLET_CORE);
    }
}

fn shots(s: &Sim) {
    gfx2d::blend(Blend::Add);
    for p in &s.shots {
        let (tx, ty) = (p.x - p.vx * 0.02, p.y - p.vy * 0.02);
        shape::capsule(p.x, p.y, tx, ty, 7.0, with_alpha(SHOT, 0.45));
        shape::capsule(p.x, p.y, tx, ty, 2.6, WHITE);
    }
    gfx2d::blend(Blend::Alpha);
}

fn power_sprite(p: Power) -> [f32; 4] {
    match p {
        Power::Spread => spr::PW_SPREAD,
        Power::Shield => spr::PW_SHIELD,
        Power::Magnet => spr::PW_MAGNET,
    }
}

fn drop(g: &Shooter, d: &Drop) {
    let t = g.time;
    let Some(img) = g.art.img else {
        shape::disc(d.x, d.y, 6.0, GEM);
        return;
    };
    match d.kind {
        DropKind::Gem => {
            // Spins by squeezing its width.
            let w = (d.age * 7.0 + d.x * 0.1).cos();
            gfx2d::blend(Blend::Add);
            shape::soft_disc(d.x, d.y, 13.0, 9.0, with_alpha(GEM, 0.45));
            gfx2d::blend(Blend::Alpha);
            let gw = GEM_SIZE * w.abs().max(0.25);
            sprite_wh(img, spr::GEM, d.x, d.y, gw, GEM_SIZE, if w > 0.0 { WHITE } else { 0xd8c8b0ff });
            // Lit from inside: the art's dark glass would read as a hole.
            gfx2d::blend(Blend::Add);
            sprite_wh(img, spr::GEM, d.x, d.y, gw, GEM_SIZE, with_alpha(GEM, 0.55));
            gfx2d::blend(Blend::Alpha);
        }
        DropKind::Power(p) => {
            let c = power_color(p);
            let bob = (t * 4.0 + d.x).sin() * 3.0;
            let (x, y) = (d.x, d.y + bob);
            let k = (t * 6.0).sin() * 0.5 + 0.5;
            gfx2d::blend(Blend::Add);
            shape::soft_disc(x, y, 24.0 + 4.0 * k, 12.0, with_alpha(c, 0.45));
            gfx2d::blend(Blend::Alpha);
            sprite(img, power_sprite(p), x, y, POWER_SIZE, WHITE);
        }
        DropKind::Heart => {
            let k = 1.0 + 0.12 * (t * 8.0).sin();
            gfx2d::blend(Blend::Add);
            shape::soft_disc(d.x, d.y, 24.0, 12.0, with_alpha(HEART, 0.45));
            gfx2d::blend(Blend::Alpha);
            sprite(img, spr::HEART, d.x, d.y, HEART_SIZE * k, WHITE);
        }
    }
}

// ---- HUD pieces -------------------------------------------------------------

fn combo(g: &Shooter) {
    let s = &g.sim;
    if s.combo < 2 {
        return;
    }
    let th = &g.theme;
    let (cx, cy) = (g.rounds.layout.hud.right() - 40.0, g.rounds.layout.hud.y + 82.0);
    // A chain count until it's worth a multiplier, then ×N with the chain under it.
    let m = s.multiplier();
    let heat = ((m - 1) as f32 / 4.0).min(1.0);
    let col = mix(SHIP_TRIM, th.gold, heat);
    let k = g.mult_punch.scale();
    gfx2d::push();
    gfx2d::translate(cx, cy);
    gfx2d::scale(k, k);
    shape::disc(0.0, 0.0, 25.0, with_alpha(INK, 0.6));
    ui::ring_meter(0.0, 0.0, 25.0, 5.0, (s.combo_left / COMBO_WINDOW).clamp(0.0, 1.0), col, with_alpha(col, 0.25));
    let big = if m >= 2 { format!("×{m}") } else { s.combo.to_string() };
    text(&big).font(th.number_font).size(22.0).color(WHITE).outline(0.1, INK).middle().draw(0.0, -1.0);
    gfx2d::pop();
    let small = if m >= 2 { format!("{} CHAIN", s.combo) } else { "CHAIN".to_string() };
    text(&small).size(13.0).color(col).outline(0.12, INK).middle().draw(cx, cy + 36.0);
}

fn powers(g: &Shooter) {
    let sh = &g.sim.ship;
    let mut x = 28.0;
    let y = g.rounds.layout.pill_zone().bottom() + if g.rounds.is_daily() { 56.0 } else { 28.0 };
    let mut chip = |p: Power, frac: Option<f32>| {
        let c = power_color(p);
        shape::disc(x, y, 15.0, with_alpha(INK, 0.7));
        match g.art.img {
            Some(img) => sprite(img, power_sprite(p), x, y, 30.0, WHITE),
            None => shape::disc(x, y, 10.0, c),
        }
        if p == Power::Spread {
            text(if sh.spread > 1 { "5" } else { "3" }).font(g.theme.number_font).size(12.0).color(WHITE).outline(0.2, INK).middle().draw(x + 10.0, y + 10.0);
        }
        if let Some(f) = frac {
            shape::arc(x, y, 15.0, 3.0, 0.0, TAU * f.clamp(0.0, 1.0), c);
        }
        x += 34.0;
    };
    if sh.spread > 0 {
        chip(Power::Spread, Some(sh.spread_t / POWER_TIME));
    }
    if sh.shield {
        chip(Power::Shield, None);
    }
    if sh.magnet_t > 0.0 {
        chip(Power::Magnet, Some(sh.magnet_t / POWER_TIME));
    }
}

fn boss_bar(g: &Shooter) {
    let Some(b) = &g.sim.boss else { return };
    let fill = if b.entering() { (b.age / 2.0).min(1.0) } else { 1.0 };
    let frac = (b.hp / b.max_hp).max(0.0) * fill;
    let r = Rect::centered(W / 2.0, g.rounds.layout.hud.y + 104.0, 200.0, 16.0);
    shape::rrect(r.inset(-3.0), 11.0, with_alpha(INK, 0.8));
    let col = if b.enraged() { DANGER } else { BOSS };
    if frac > 0.0 {
        shape::rrect_gradient(Rect::new(r.x, r.y, r.w * frac, r.h), 8.0, lighten(col, 0.35), col);
    }
    text(look::BOSS_NAME).size(12.0).color(WHITE).outline(0.16, INK).tracking(0.08).middle().draw(r.cx(), r.cy());
}

fn banner(g: &Shooter) {
    let Some(b) = &g.sim.banner else { return };
    let (big, small) = look::banner(b.kind);
    let t = b.age;
    let enter = Ease::BackOut.at((t / 0.35).min(1.0));
    let a = if t > 1.7 { (1.0 - (t - 1.7) / 0.5).max(0.0) } else { 1.0 };
    let y = 262.0;
    let th = &g.theme;
    let (color, band) = match b.kind {
        BannerKind::Warning => (WHITE, Some(DANGER)),
        BannerKind::New(k) => (lighten(ENEMY[k.index()], 0.2), None),
        BannerKind::Clear => (th.gold, None),
    };
    if let Some(band) = band {
        let k = 0.5 + 0.5 * (t * 12.0).sin();
        gfx2d::rect(0.0, y - 34.0, W, 68.0, fade(with_alpha(band, 0.35 + 0.25 * k), a));
        gfx2d::rect(0.0, y - 38.0, W, 4.0, fade(band, a));
        gfx2d::rect(0.0, y + 34.0, W, 4.0, fade(band, a));
    }
    gfx2d::push();
    gfx2d::translate(W / 2.0, y);
    gfx2d::scale(0.4 + 0.6 * enter, 0.4 + 0.6 * enter);
    text(big).font(th.title_font).size(40.0).color(color).outline(0.1, INK).soft_shadow(0.0, 3.0, 0.06, th.shadow).alpha(a).middle().draw(0.0, -6.0);
    text(small).size(15.0).color(WHITE).outline(0.14, INK).tracking(0.1).alpha(a).middle().draw(0.0, 24.0);
    gfx2d::pop();
}

fn hint(g: &Shooter) {
    let sh = &g.sim.ship;
    let t = g.time;
    let swing = (t * 2.2).sin();
    let (fx, fy) = (sh.x + swing * 46.0, (sh.y + MIN_LIFT + 14.0).min(H - 24.0));
    // A ghost thumb sliding side to side under the ship, arrows either side.
    let a = 0.8;
    shape::capsule(sh.x, sh.y + 26.0, fx, fy - 18.0, 2.0, with_alpha(WHITE, 0.25 * a));
    let pulse = (t * 1.6).fract();
    shape::ring(fx, fy, 18.0 + 16.0 * pulse, 2.5, with_alpha(SHIP_TRIM, 0.7 * (1.0 - pulse)));
    gfx2d::rrect(fx - 15.0, fy - 19.0, 30.0, 38.0, 15.0, 0.0, 0.0, with_alpha(WHITE, 0.8 * a), with_alpha(WHITE, 0.5 * a));
    gfx2d::rrect(fx - 15.0, fy - 19.0, 30.0, 38.0, 15.0, 2.0, 0.0, with_alpha(INK, 0.6 * a), with_alpha(INK, 0.6 * a));
    gfx2d::rrect(fx - 9.0, fy - 15.0, 18.0, 13.0, 6.0, 0.0, 0.0, with_alpha(SHIP_TRIM, 0.35 * a), with_alpha(SHIP_TRIM, 0.15 * a));
    for side in [-1.0f32, 1.0] {
        let ax = sh.x + side * 82.0;
        let push = if side * swing > 0.0 { swing.abs() * 4.0 } else { 0.0 };
        let icon = if side < 0.0 { Icon::ARROW_LEFT } else { Icon::ARROW_RIGHT };
        icon.draw(ax + side * push, fy, 26.0, with_alpha(WHITE, 0.85));
    }
    let ty = (fy + 40.0).min(H - 18.0);
    text(look::HINT).size(20.0).color(WHITE).outline(0.12, INK).soft_shadow(0.0, 2.0, 0.08, 0x00000080).middle().draw(W / 2.0, ty);
}
