//! Drawing: the generated clay sheets (src/art.rs) for the map, pads,
//! towers, jellies and the cake; gfx2d and the UI kit's shapes for what code
//! does better (shadows, shots, rings, particles, the picker and the HUD).
//! Reads the game, changes nothing.

use std::f32::consts::FRAC_PI_2;

use maimbrain::gfx2d;
use maimbrain::motion::Ease;
use maimbrain::rounds::Stage;
use maimbrain::ui::{self, Icon, Rect, fade, shape, text, with_alpha};

use crate::art::{self, Art};
use crate::look::*;
use crate::sim::{CAKE, CAKE_R, Creep, CreepKind, ENDING, HEARTS, PAD, PADS, PATH, ROAD_W, Shot, Sim, Tower, TowerKind, WAVE_GAP};
use crate::{Keep, SLOT_R, Sel, Splat, picker_slots, upgrade_slot};

/// A cheap deterministic hash in 0…1 (scenery placement; no RNG in render).
fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

pub fn frame(g: &Keep) {
    let s = &g.sim;
    let l = &g.rounds.layout;
    let a = &g.art;
    gfx2d::push();
    g.shake.apply(l.screen.cx(), l.screen.cy());
    // The clay map (with the road baked in), and lawn past its edges on tall screens.
    gfx2d::rect(-20.0, -20.0, l.screen.w + 40.0, l.screen.h + 40.0, GROUND);
    a.map();
    road_flow(g.time);
    // The selected tower's reach.
    let reach = match g.sel {
        Sel::Tower(p) => s.towers[p].as_ref().map(|t| (p, t.range())),
        Sel::Pad(p) => Some((p, TowerKind::Pop.spec().range[0])),
        Sel::None => None,
    };
    if let Some((p, r)) = reach {
        let (x, y) = PADS[p];
        let k = Ease::BackOut.at((g.sel_t / 0.25).min(1.0));
        shape::disc(x, y, r * k, 0xffffff30);
        shape::ring(x, y, r * k, 2.5, 0xffffffb0);
    }
    for (p, &(x, y)) in PADS.iter().enumerate() {
        pad(a, x, y, s.towers[p].is_none(), g.sel == Sel::Pad(p) || g.sel == Sel::Tower(p));
    }
    cake(g);
    // Jellies, back to front, then towers over them where they overlap.
    let mut order: Vec<&Creep> = s.creeps.iter().collect();
    order.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.id.cmp(&b.id)));
    for c in &order {
        jelly_shadow(c);
    }
    for c in &order {
        jelly(a, s, c, g.time);
    }
    for sp in &g.splats {
        splat(a, sp);
    }
    for (p, t) in s.towers.iter().enumerate() {
        if let Some(t) = t {
            let (x, y) = PADS[p];
            tower(a, x, y, t, 1.0, g.time);
        }
    }
    for sh in &s.shots {
        shot(a, sh);
    }
    g.fx.draw();
    gfx2d::pop();
    g.popups.draw();
    g.hurt.draw(l.screen);

    if g.rounds.playing() {
        picker(g);
        hud(g);
        hint(g);
        banner(g);
    }
}

/// Little pressed dimples that drift down the road toward the cake: which way
/// the jellies come.
fn road_flow(t: f32) {
    let mut d = (t * 14.0) % 22.0;
    let mut acc = 0.0;
    for w in PATH.windows(2) {
        let len = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        while d < acc + len {
            let k = (d - acc) / len;
            let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * k, w[0].1 + (w[1].1 - w[0].1) * k);
            let r = ROAD_W * 0.07;
            shape::disc(x, y + 0.6, r + 0.3, ROAD_DOT_SHADE);
            shape::disc(x, y, r, ROAD_DOT);
            d += 22.0;
        }
        acc += len;
    }
}

fn pad(a: &Art, x: f32, y: f32, empty: bool, selected: bool) {
    let r = Rect::centered(x, y + 3.0, PAD - 2.0, PAD - 6.0);
    shape::shadow(r, 14.0, 4.0, 5.0, PAD_SHADOW);
    a.draw(art::PAD, 0, x, y + PAD / 2.0 + 4.0, PAD + 8.0, false, 0xffffffff);
    if selected {
        shape::rrect_stroke(Rect::centered(x, y, PAD + 2.0, PAD + 2.0), 14.0, 3.0, WHITE);
    }
    if empty {
        Icon::PLUS.draw(x, y + 1.0, 18.0, with_alpha(PAD_PLUS, 0.75));
    }
}

fn jelly_shadow(c: &Creep) {
    let r = c.kind.spec().radius;
    gfx2d::push();
    gfx2d::translate(c.x, c.y + 1.0);
    gfx2d::scale(1.0, 0.38);
    shape::soft_disc(0.0, 0.0, r * 1.15, 3.0, SHADOW);
    gfx2d::pop();
}

/// One jelly picture: frame `frame` of `kind`'s sheet with its feet at (x, y),
/// sized by radius `r`, squashed by `wobble` (−1…1), mirrored when `flip`.
/// Shared by the road, the HUD's next-wave icons and the cards.
#[allow(clippy::too_many_arguments)]
pub fn jelly_pic(a: &Art, x: f32, y: f32, r: f32, kind: CreepKind, frame: u32, flip: bool, wobble: f32, tint: u32, flash: f32, frost: bool) {
    let sheet = art::jelly_sheet(kind);
    let w = r * art::JELLY_CELL_PER_R[kind as usize];
    let foot = w * 6.0 / art::SHEETS[sheet].cell.0; // the sheet's bottom margin (6 texels)
    gfx2d::push();
    gfx2d::translate(x, y + foot);
    gfx2d::scale(1.0 + 0.08 * wobble, 1.0 - 0.08 * wobble);
    a.draw(sheet, frame, 0.0, 0.0, w, flip, tint);
    if frost {
        a.glow(sheet, frame, 0.0, 0.0, w, flip, FROST_GLOW, 0.45);
    }
    a.glow(sheet, frame, 0.0, 0.0, w, flip, WHITE, flash);
    gfx2d::pop();
}

fn jelly(a: &Art, s: &Sim, c: &Creep, t: f32) {
    let spec = c.kind.spec();
    let r = spec.radius;
    let speed = c.cur_speed() / spec.speed;
    let phase = c.age * 7.0 * speed.max(0.4) + c.id as f32 * 1.7;
    let wobble = (phase * 1.4).sin();
    let hop = (phase * 1.4).sin().abs() * 2.0;
    // Which way it walks: mirror the sheet when heading left.
    let (ax, _) = s.point_at(c.d + 4.0);
    let flip = ax < c.x - 0.5;
    if c.kind == CreepKind::Zipper {
        // Speed lines behind it.
        let (bx, by) = s.point_at(c.d - 1.0);
        let (mx, my) = ((c.x - bx).signum() * (c.x - bx).abs().min(1.0), (c.y - by).signum() * (c.y - by).abs().min(1.0));
        for k in 0..3 {
            let o = (k as f32 - 1.0) * 5.0;
            let (px, py) = (c.x - mx * 12.0 + my * o, c.y - 8.0 - my * 12.0 - mx * o);
            shape::capsule(px, py, px - mx * 8.0, py - my * 8.0, 1.6, 0xffffffa0);
        }
    }
    let frame = if c.kind == CreepKind::Gummy && c.flash > 0.55 { art::GUMMY_HIT } else { (phase / 1.6) as u32 % 2 };
    jelly_pic(a, c.x, c.y - hop, r, c.kind, frame, flip, wobble, WHITE, c.flash * 0.8, c.slow > 0.0);
    if c.slow > 0.0 {
        shape::sparkle(c.x + r * 0.7, c.y - r * 1.8, 3.5, t * 3.0 + c.id as f32, 0xe6f9ffff);
    }
    if c.hp < c.max_hp {
        let w = (r * 2.0).max(18.0);
        let bar = Rect::new(c.x - w / 2.0, c.y - r * 2.1 - 9.0 - hop, w, 4.0);
        shape::rrect(bar.inset(-1.0), 3.0, with_alpha(INK, 0.6));
        shape::rrect(Rect::new(bar.x, bar.y, bar.w * (c.hp / c.max_hp).clamp(0.0, 1.0), bar.h), 2.0, if c.kind == CreepKind::King { GOLD } else { 0x7dff8aff });
    }
}

/// A popped jelly: its sheet's pop frame, bursting outward and fading.
fn splat(a: &Art, sp: &Splat) {
    let k = (sp.age / Splat::LIFE).clamp(0.0, 1.0);
    let r = sp.kind.spec().radius * (1.0 + 0.35 * Ease::QuadOut.at(k));
    let alpha = ((1.0 - Ease::QuadIn.at(k)) * 255.0) as u32;
    jelly_pic(a, sp.x, sp.y, r, sp.kind, art::pop_frame(sp.kind), sp.flip, 0.0, 0xffffff00 | alpha, 0.0, false);
}

/// How wide a tower's cell is drawn at level 3 (that model fills the cell's
/// height), and how much bigger levels 1–3 are drawn per `TowerKind`, so a
/// small first-level model still reads on its pad. Retune after regenerating.
const TOWER_W: f32 = 80.0;
const TOWER_LEVEL_GROW: [[f32; 3]; 3] = [[1.45, 1.2, 1.0], [1.35, 1.12, 1.0], [1.0, 1.0, 0.95]];

/// A tower standing on its pad (x, y is the pad's centre). `scale` for the
/// picker's icons. The sheet's frames are the three levels.
pub fn tower(a: &Art, x: f32, y: f32, t: &Tower, scale: f32, time: f32) {
    let pop = if scale < 1.0 { 1.0 } else { Ease::BackOut.at((t.age / 0.35).min(1.0)) };
    let kick = t.recoil * t.recoil;
    gfx2d::push();
    gfx2d::translate(x, y + 22.0 * scale);
    gfx2d::scale(scale * pop, scale * pop);
    if t.kind == TowerKind::Chill {
        // A frosty glow that breathes, brighter on each pulse.
        let glow = 0.5 + 0.5 * (time * 3.0).sin();
        shape::soft_disc(0.0, -30.0 - t.level as f32 * 5.0, 26.0 + t.level as f32 * 3.0, 16.0, with_alpha(FROST, 0.16 + 0.08 * glow + 0.3 * kick));
    }
    // Squash on each shot, and lean toward the target.
    let lean = if scale < 1.0 { 0.0 } else { t.aim.cos() * 0.08 };
    gfx2d::rotate(lean);
    gfx2d::scale(1.0 + 0.1 * kick, 1.0 - 0.14 * kick);
    let flip = scale >= 1.0 && t.kind != TowerKind::Chill && t.aim.cos() < -0.2;
    let frame = art::TOWER_LEVEL_FRAME[t.kind as usize][t.level];
    a.draw(art::tower_sheet(t.kind), frame, 0.0, 6.0, TOWER_W * TOWER_LEVEL_GROW[t.kind as usize][t.level], flip, WHITE);
    gfx2d::pop();
}

fn shot(a: &Art, sh: &Shot) {
    match sh.kind {
        TowerKind::Pop => {
            // A clay gumball: one of the dome's colours, a soft rim and a highlight.
            let col = GUMBALLS[(sh.target.unwrap_or(0) % GUMBALLS.len() as u32) as usize];
            let (dx, dy) = (sh.tx - sh.x, sh.ty - sh.y);
            let n = dx.hypot(dy).max(1.0);
            shape::capsule(sh.x - dx / n * 9.0, sh.y - dy / n * 9.0, sh.x, sh.y, 3.5, 0xffffff60);
            shape::disc(sh.x, sh.y + 0.8, 5.0, ui::darken(col, 0.35));
            shape::disc(sh.x, sh.y, 4.4, col);
            shape::soft_disc(sh.x - 1.4, sh.y - 1.6, 1.6, 1.0, 0xffffffd0);
        }
        TowerKind::Boom => {
            let lift = sh.lift();
            // Its shadow on the ground shows where it lands.
            gfx2d::push();
            gfx2d::translate(sh.x, sh.y);
            gfx2d::scale(1.0, 0.4);
            shape::soft_disc(0.0, 0.0, 7.0, 2.0, SHADOW);
            gfx2d::pop();
            shape::ring(sh.tx, sh.ty, sh.splash * 0.5, 1.5, 0xffffff60);
            a.draw(art::BOMB, 0, sh.x, sh.y - lift + 9.0, 20.0, false, WHITE);
        }
        TowerKind::Chill => {}
    }
}

/// The cake at the end of the road: a clay cake on a plate whose face reacts
/// (happy, nervous, panicking, wincing at a bite), and the empty plate once
/// it's devoured.
fn cake(g: &Keep) {
    let s = &g.sim;
    let a = &g.art;
    let (cx, cy) = CAKE;
    let end = s.ending.map_or(0.0, |e| (e / ENDING).clamp(0.0, 1.0));
    let gone = s.over || g.rounds.stage() == Stage::Over;
    // The plate's foot: where the sheet's bottom sits.
    let base = cy + 38.0;
    // The sheet's cell is wider than the cake (plate and margins).
    const W: f32 = CAKE_R * 3.05;
    gfx2d::push();
    gfx2d::translate(cx, base - 6.0);
    gfx2d::scale(1.0, 0.3);
    shape::soft_disc(0.0, 0.0, 50.0, 6.0, SHADOW);
    gfx2d::pop();
    if gone {
        a.draw(art::CAKE, art::CAKE_GONE, cx, base, W, false, WHITE);
        // The culprits, full and smug.
        let fat = [(-48.0, 34.0, 1.25, false), (50.0, 30.0, 1.2, true), (-22.0, 50.0, 1.1, false), (26.0, 52.0, 1.15, true)];
        for (i, &(dx, dy, k, flip)) in fat.iter().enumerate() {
            let w = (g.time * 2.5 + i as f32 * 1.3).sin();
            jelly_pic(a, cx + dx, cy + dy, 13.0 * k, CreepKind::Gummy, art::GUMMY_SMUG, flip, w, WHITE, 0.0, false);
        }
        return;
    }
    let jit = if end > 0.0 { ((g.time * 50.0).sin() * 2.5 * end, (g.time * 43.0).cos() * 1.5 * end) } else { (0.0, 0.0) };
    let k = (1.0 - 0.45 * Ease::QuadIn.at(end)) * (1.0 - 0.04 * (HEARTS - s.hearts) as f32);
    let danger = s.danger();
    let breathe = 1.0 + 0.02 * (g.time * 2.0).sin() + 0.06 * g.wince.value();
    let mood = if g.wince.value() > 0.3 {
        art::CAKE_WINCE
    } else if danger > 0.6 || end > 0.0 {
        art::CAKE_PANIC
    } else if danger > 0.25 {
        art::CAKE_NERVOUS
    } else {
        art::CAKE_HAPPY
    };
    gfx2d::push();
    gfx2d::translate(cx + jit.0, base + jit.1);
    gfx2d::scale(k * breathe, k / breathe.sqrt());
    a.draw(art::CAKE, mood, 0.0, 0.0, W, false, WHITE);
    // Bites: one handful of crumbs on the plate per heart lost (and the cake
    // shrinks a little, above).
    for i in 0..(HEARTS - s.hearts) * 5 {
        let bx = (hash(i * 3 + 300) - 0.5) * 68.0;
        let by = -7.0 - hash(i * 3 + 301) * 6.0;
        let r = 1.4 + hash(i * 3 + 302) * 1.6;
        shape::disc(bx, by + 0.8, r, SHADOW);
        shape::disc(bx, by, r, if i % 4 == 3 { CAKE_FROSTING } else { CAKE_SPONGE });
    }
    gfx2d::pop();
    if end > 0.0 {
        // The devouring: jellies pile on and chomp.
        let pile = [(-22.0, -40.0, CreepKind::Gummy), (20.0, -44.0, CreepKind::Helmet), (-36.0, -14.0, CreepKind::Zipper), (36.0, -18.0, CreepKind::Gummy), (0.0, -58.0, CreepKind::Splitter), (-8.0, -30.0, CreepKind::Zipper)];
        let n = ((end * 1.6 * pile.len() as f32) as usize).min(pile.len());
        for (i, &(dx, dy, kind)) in pile.iter().take(n).enumerate() {
            let w = (g.time * 14.0 + i as f32 * 2.0).sin();
            let hop = (g.time * 12.0 + i as f32).sin().abs() * 4.0;
            let frame = ((g.time * 8.0) as u32 + i as u32) % 2;
            jelly_pic(a, cx + dx + jit.0, cy + dy * k + 8.0 - hop, kind.spec().radius, kind, frame, dx > 0.0, w, WHITE, 0.0, false);
        }
    }
}

/// The tower picker (an empty pad) or the upgrade bubble (a tower).
fn picker(g: &Keep) {
    let s = &g.sim;
    let pop = |i: usize| Ease::BackOut.at(((g.sel_t - i as f32 * 0.04) / 0.22).clamp(0.0, 1.0));
    let shake_x = |k: usize| match g.denied {
        Some((d, t)) if d == k => (t * 70.0).sin() * 5.0 * (1.0 - t / 0.4),
        _ => 0.0,
    };
    match g.sel {
        Sel::Pad(p) => {
            let slots = picker_slots(p);
            let (px, py) = PADS[p];
            let panel = Rect::new(slots[0].0 - SLOT_R - 8.0, slots[0].1 - SLOT_R - 8.0, slots[2].0 - slots[0].0 + 2.0 * SLOT_R + 16.0, 2.0 * SLOT_R + 30.0);
            let a = pop(0);
            // A tail pointing at the pad.
            let tail_y = if slots[0].1 < py { panel.bottom() } else { panel.y };
            let dir = if slots[0].1 < py { 1.0 } else { -1.0 };
            let tx = px.clamp(panel.x + 20.0, panel.right() - 20.0);
            shape::poly(&[tx - 10.0, tail_y, tx + 10.0, tail_y, px, tail_y + dir * 12.0], fade(0x3a1f35d8, a));
            shape::rrect(panel, 18.0, fade(0x3a1f35d8, a));
            for (k, &(x, y)) in slots.iter().enumerate() {
                let kind = TowerKind::ALL[k];
                let cost = kind.spec().cost[0];
                let ok = s.coins >= cost;
                let sc = pop(k);
                let x = x + shake_x(k);
                gfx2d::push();
                gfx2d::translate(x, y);
                gfx2d::scale(sc, sc);
                shape::disc(0.0, 0.0, SLOT_R, if ok { 0xfff4dcff } else { 0x8a7a8aff });
                shape::ring(0.0, 0.0, SLOT_R, 2.5, if ok { TOWER[k] } else { 0x5a4a5aff });
                let t = Tower { kind, level: 0, cd: 0.0, aim: -FRAC_PI_2, recoil: 0.0, age: 1.0 };
                tower(&g.art, 0.0, 2.0, &t, 0.7, g.time);
                gfx2d::pop();
                let cost_s = cost.to_string();
                text(&cost_s).size(15.0).color(if ok { GOLD } else { 0xff8a9aff }).outline(0.14, INK).middle().draw(x + 6.0, y + SLOT_R + 9.0);
                Icon::COIN.draw(x - 13.0, y + SLOT_R + 9.0, 13.0, if ok { GOLD } else { 0xb0a0a0ff });
            }
        }
        Sel::Tower(p) => {
            let (x, y) = upgrade_slot(p);
            let x = x + shake_x(0);
            let sc = pop(0);
            gfx2d::push();
            gfx2d::translate(x, y);
            gfx2d::scale(sc, sc);
            match s.upgrade_cost(p) {
                Some(cost) => {
                    let ok = s.coins >= cost;
                    shape::disc(0.0, 0.0, SLOT_R + 2.0, INK);
                    shape::disc(0.0, 0.0, SLOT_R, if ok { 0x7be38aff } else { 0x8a7a8aff });
                    Icon::ARROW_UP.draw(0.0, -6.0, 22.0, WHITE);
                    text(&cost.to_string()).size(14.0).color(if ok { GOLD } else { 0xffb0b8ff }).outline(0.16, INK).middle().draw(0.0, 13.0);
                }
                None => {
                    shape::rrect(Rect::centered(0.0, 0.0, 64.0, 30.0), 15.0, INK);
                    text("MAX").size(16.0).color(GOLD).middle().draw(0.0, 0.0);
                }
            }
            gfx2d::pop();
        }
        Sel::None => {}
    }
}

fn hud(g: &Keep) {
    let s = &g.sim;
    g.hud.draw(&g.rounds.layout);
    // Coins, top left (clear of the pause pill).
    let r = g.coin_pill();
    let k = g.coin_punch.scale();
    gfx2d::push();
    gfx2d::translate(r.cx(), r.cy());
    gfx2d::scale(k, k);
    ui::pill(&s.coins.to_string(), 0.0, 0.0, r.h, with_alpha(INK, 0.7), GOLD, g.theme.body_font, Some(Icon::COIN));
    gfx2d::pop();
    // Next wave: what's coming, a countdown ring, tap to call it early.
    let b = g.wave_button();
    shape::rrect(b, 16.0, with_alpha(INK, 0.68));
    let label = if s.wave == 0 { "WAVE 1".to_string() } else { format!("WAVE {}", s.next.n) };
    text(&label).size(11.0).color(0xffffffc0).middle().draw(b.x + 34.0, b.y + 10.0);
    let groups = &s.next.groups;
    let gw = 26.0;
    for (i, &(kind, n)) in groups.iter().take(3).enumerate() {
        let x = b.x + 12.0 + i as f32 * gw;
        let r = (kind.spec().radius * 0.6).clamp(6.0, 9.0);
        jelly_pic(&g.art, x, b.y + 39.0, r, kind, 0, false, 0.0, WHITE, 0.0, false);
        text(&format!("{n}")).size(10.0).color(WHITE).outline(0.2, INK).middle().draw(x + 8.0, b.y + 36.0);
    }
    if s.next.new_kind.is_some() {
        let pulse = 0.7 + 0.3 * (g.time * 6.0).sin();
        text("NEW").size(10.0).color(fade(GOLD, pulse)).outline(0.2, INK).middle().draw(b.x + 70.0, b.y + 10.0);
    }
    let (cx, cy) = (b.right() - 22.0, b.cy());
    let frac = if s.wave == 0 { 0.0 } else { 1.0 - (s.next_in / WAVE_GAP).clamp(0.0, 1.0) };
    shape::disc(cx, cy, 18.0, 0xfff4dcff);
    ui::ring_meter(cx, cy, 18.0, 4.0, frac, DANGER, 0x00000030);
    for o in [-4.0f32, 4.0] {
        shape::poly(&[cx + o - 4.0, cy - 6.0, cx + o + 4.0, cy, cx + o - 4.0, cy + 6.0], INK);
    }
    let bonus = s.early_bonus();
    if bonus > 0 {
        text(&format!("+{bonus}")).size(12.0).color(GOLD).outline(0.2, INK).middle().draw(cx, b.bottom() + 8.0);
    }
}

/// The animated finger that teaches a tap, where it works.
fn hint(g: &Keep) {
    let Some((x, y, words)) = g.hint() else { return };
    let t = g.time;
    let phase = (t * 1.4).fract();
    shape::ring(x, y, 24.0 + 14.0 * phase, 3.5, fade(WHITE, 1.0 - phase));
    let on_pad = PADS.iter().any(|&(px, py)| px == x && py == y);
    let bob = Ease::QuadInOut.at(((t * 2.8).sin() * 0.5 + 0.5).clamp(0.0, 1.0)) * 8.0;
    let tx = x.clamp(80.0, 280.0);
    if on_pad {
        // A bouncing arrow onto the pad, the words under it.
        let ay = y - 44.0 - bob;
        // A chunky arrow: shaft and head, with an ink edge.
        let arrow = |grow: f32, dy: f32, c: u32| {
            let g = grow;
            let (top, neck, tip) = (ay - 20.0 - g + dy, ay + 2.0 + dy, ay + 18.0 + g + dy);
            shape::rrect(Rect::new(x - 6.0 - g, top, 12.0 + 2.0 * g, neck - top + 2.0), 4.0 + g, c);
            shape::rounded_poly(&[x - 15.0 - g * 1.6, neck, x + 15.0 + g * 1.6, neck, x, tip], 3.0, c);
        };
        arrow(3.0, 2.0, with_alpha(INK, 0.35));
        arrow(3.0, 0.0, INK);
        arrow(0.0, 0.0, WHITE);
        text(words).size(16.0).color(WHITE).outline(0.16, INK).soft_shadow(0.0, 2.0, 0.08, 0x00000080).middle().draw(tx, y + 42.0);
    } else {
        text(words).size(16.0).color(WHITE).outline(0.16, INK).soft_shadow(0.0, 2.0, 0.08, 0x00000080).middle().draw(tx, y - 46.0 - bob * 0.4);
    }
}

fn banner(g: &Keep) {
    let Some((words, age, big)) = &g.banner else { return };
    let k = Ease::BackOut.at((age / 0.3).min(1.0));
    let a = if *age > 1.2 { 1.0 - (age - 1.2) / 0.4 } else { 1.0 };
    let size = if *big { 34.0 } else { 26.0 };
    gfx2d::push();
    gfx2d::translate(180.0, 300.0);
    gfx2d::scale(k, k);
    text(words).size(size).color(if *big { GOLD } else { WHITE }).outline(0.12, INK).soft_shadow(0.0, 3.0, 0.08, 0x00000080).alpha(a.max(0.0)).middle().draw(0.0, 0.0);
    gfx2d::pop();
}

/// Under the results card: how far the round got.
pub fn wave_reached(g: &Keep, panel: Rect) {
    if g.rounds.stage() != Stage::Over {
        return;
    }
    let t = g.rounds.age();
    let a = ((t - 0.9) / 0.3).clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    ui::pill(&format!("REACHED WAVE {}", g.sim.wave.max(1)), panel.cx(), panel.bottom() + 62.0, 30.0, fade(with_alpha(INK, 0.7), a), fade(WHITE, a), g.theme.body_font, Some(Icon::FLAG));
}
