//! The quick path's default bots (docs/STUDIO-2.md §8.1): a game without
//! `game/bot.lua` whose player has a behaviour gets a policy for the
//! behaviour's verb (`quick::Beh::verb`), so `difficulty` works with no Lua
//! at all. Each plays the verb, not the game: it reads the scene as a
//! player sees it (the player, the level's tiles, hazards, pickups and what
//! the game's `on("overlap")` handlers watch for) and says what to press.
//! The person (reaction, aim, timing) is added after, by bot.rs.
//!
//! - `player.flappy`: flaps when it would fall below the next gap (or the
//!   middle of the free space), as a beat (`new = false`);
//! - `player.runner`: jumps each hazard, wall and gap so the jump's top is
//!   over it, holding for the full height;
//! - `player.platformer`: holds toward the nearest goal it hasn't reached
//!   (a pickup, or what `on("overlap", player, …)` watches for), else on
//!   along the level, and flicks that thumb up to jump gaps, thorns, walls,
//!   hazards and goals overhead (one thumb, as most first-timers play);
//! - `player.drag`, `player.paddle`, `player.tilt`: drags the player to
//!   where the next pickup will be, away from hazards.
//!
//! When the default plays wrong, the game writes its own `game/bot.lua`.

use maimbrain::scene::NodeId;
use maimbrain::scene::tilemap::TileMap;

use crate::api::Host;
use crate::bot::{HOLD_SIZE, Intent};
use crate::quick::{Beh, Quick, Thing, Verb};

/// A default bot's memory across a round's frames.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// The platformer's way along the level (1 right, -1 left; 0 not yet).
    dir: f32,
    /// Goals reached (or given up on).
    done: Vec<NodeId>,
    /// The goal it's going for, and since when.
    goal: Option<(NodeId, f64)>,
    /// Where it was a while ago, to notice it's stuck.
    stuck: (f32, f64),
    /// The way it was going when it left the ground.
    air: f32,
    /// When the platformer's jump asked for is due (game time).
    jump: f64,
}

/// How long a jump takes to its top for this jump speed and gravity (s).
fn rise(jump: f32, g: f32) -> f32 {
    if g > 0.0 { jump / g } else { 0.4 }
}

/// What a player's box and node say: centre, size, velocity.
struct Body {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    vx: f32,
    vy: f32,
}

impl Body {
    fn top(&self) -> f32 {
        self.y - self.h / 2.0
    }
    fn feet(&self) -> f32 {
        self.y + self.h / 2.0
    }
}

fn body(h: &Host, node: NodeId, kind: &str) -> Option<Body> {
    let b = h.quick.hitbox(&h.scene, node, kind)?;
    let n = h.scene.get(node)?;
    Some(Body { x: b[0] + b[2] / 2.0, y: b[1] + b[3] / 2.0, w: b[2], h: b[3], vx: n.vx, vy: n.vy })
}

/// The level's map and where it is in the world.
fn level(h: &Host) -> Option<((f32, f32), &TileMap)> {
    let l = h.quick.level?;
    let m = h.scene.get(l)?.tilemap.as_ref()?;
    Some((h.scene.world_position(l), &**m))
}

/// A cell's collision at a world point: solid 1, one-way 2, hazard 4.
fn flags(map: Option<((f32, f32), &TileMap)>, x: f32, y: f32) -> u8 {
    map.map_or(0, |((ox, oy), m)| {
        let (cx, cy) = m.cell_of(x - ox, y - oy);
        m.flags(cx, cy)
    })
}

/// The world's visible rectangle: x0, y0, x1, y1.
fn view(h: &Host) -> [f32; 4] {
    let (w, hh) = (h.scene.width, h.scene.height);
    let (x0, y0) = h.scene.camera.to_world(w, hh, 0.0, 0.0);
    let (x1, y1) = h.scene.camera.to_world(w, hh, w, hh);
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}

/// A world point on the canvas.
fn canvas(h: &Host, x: f32, y: f32) -> (f32, f32) {
    h.scene.camera.to_screen(h.scene.width, h.scene.height, x, y)
}

fn knob(q: &Quick, t: &Thing, name: &str) -> f32 {
    q.knob(t, name) as f32
}

/// The things with a behaviour, with their boxes (x, y, w, h) and velocities.
fn things_with(h: &Host, beh: Beh) -> Vec<(NodeId, [f32; 4], (f32, f32))> {
    h.quick
        .things
        .iter()
        .filter(|t| t.behs.contains(&beh) && h.scene.get(t.node).is_some_and(|n| n.visible))
        .filter_map(|t| {
            let b = h.quick.hitbox(&h.scene, t.node, &t.kind)?;
            let n = h.scene.get(t.node)?;
            // As they move: speedUp runs the world (not the players) faster than its vx says.
            let s = h.quick.speed as f32;
            Some((t.node, b, (n.vx * s, n.vy * s)))
        })
        .collect()
}

/// What the player goes for: pickups, and the tags `on("overlap", player, tag)` watches.
fn goals(h: &Host, player: &str) -> Vec<(NodeId, [f32; 4])> {
    let mut tags: Vec<&str> = Vec::new();
    for (a, b) in &h.quick.overlaps {
        let other = if a == player {
            b
        } else if b == player {
            a
        } else {
            continue;
        };
        if other != "*" && !tags.contains(&other.as_str()) {
            tags.push(other);
        }
    }
    let hazard = |id: NodeId| h.quick.thing(id).is_some_and(|t| t.behs.contains(&Beh::Hazard));
    let mut out: Vec<(NodeId, [f32; 4])> = things_with(h, Beh::Pickup).into_iter().map(|(id, b, _)| (id, b)).collect();
    for tag in tags {
        for id in h.scene.all(tag) {
            if hazard(id) || out.iter().any(|o| o.0 == id) || !h.scene.get(id).is_some_and(|n| n.visible) {
                continue;
            }
            let kind = h.quick.thing(id).map_or(tag.to_string(), |t| t.kind.clone());
            if let Some(b) = h.quick.hitbox(&h.scene, id, &kind) {
                out.push((id, b));
            }
        }
    }
    out
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// The quick path's frame as the bot's report hears it (bot.rs `events`):
/// a pickup by its kind, an `on("overlap")` pair as it starts touching, a
/// thing leaving; and what hurt the player this frame, for `hurt` events
/// and the round's cause.
pub fn heard(h: &mut Host, events: &[crate::quick::Event]) {
    let tag = |id: NodeId| {
        h.scene.get(id).map_or_else(
            || "thing".to_string(),
            |n| {
                if n.tag.is_empty() { "thing".to_string() } else { n.tag.clone() }
            },
        )
    };
    let was = h.bot.as_ref().map(|b| b.overlapping.clone()).unwrap_or_default();
    let mut out = Vec::new();
    let mut touching = Vec::new();
    let mut hurt_by = None;
    for e in events {
        match (e.name, e.b) {
            ("pickup", b) => out.push(format!("pickup {}", b.map_or_else(|| "thing".to_string(), tag))),
            ("hazard", Some(b)) => hurt_by = Some(tag(b)),
            ("hazard", None) => hurt_by = Some(hazard_tile(h, e.a).unwrap_or_else(|| "hazard tiles".into())),
            ("overlap", Some(b)) => {
                touching.push((e.a, b));
                if !was.contains(&(e.a, b))
                    && let Some((x, y)) = h.quick.overlaps.get((e.value as usize).wrapping_sub(1))
                {
                    out.push(format!("overlap {x} {y}"));
                }
            }
            ("leave", _) => out.push(format!("leave {}", tag(e.a))),
            _ => {}
        }
    }
    let t = h.time;
    if let Some(b) = &mut h.bot {
        b.events.extend(out.into_iter().map(|k| (k, t)));
        b.overlapping = touching;
        b.hurt_by = hurt_by;
    }
}

/// The hazard tile a thing is touching, by name.
fn hazard_tile(h: &Host, node: NodeId) -> Option<String> {
    let kind = h.quick.thing(node)?.kind.clone();
    let b = h.quick.hitbox(&h.scene, node, &kind)?;
    let ((ox, oy), m) = level(h)?;
    let (c0, r0) = m.cell_of(b[0] - ox - 1.0, b[1] - oy - 1.0);
    let (c1, r1) = m.cell_of(b[0] + b[2] - ox + 1.0, b[1] + b[3] - oy + 1.0);
    (r0..=r1).flat_map(|r| (c0..=c1).map(move |c| (c, r))).find_map(|(c, r)| m.cell(c, r).filter(|t| t.hazard).map(|t| t.name.clone()))
}

/// The default bot's decisions this frame, for the first player. `finger`
/// is where the bot's finger is (canvas units, down).
pub fn policy(h: &Host, st: &mut State, finger: (f32, f32, bool)) -> Vec<Intent> {
    let Some(p) = h.quick.players().next().cloned() else {
        return Vec::new();
    };
    let Some(me) = body(h, p.node, &p.kind) else {
        return Vec::new();
    };
    let Some(verb) = p.behs.iter().find_map(|b| b.verb()) else {
        return Vec::new();
    };
    match verb {
        Verb::Tap if p.behs.contains(&Beh::Flappy) => flappy(h, &p, &me),
        Verb::Tap => runner(h, &p, &me),
        Verb::HoldToward => platformer(h, &p, &me, st),
        Verb::Drag => drag(h, &p, &me, finger),
    }
}

/// The free stretches of a column of the world from `y0` to `y1` at `x0..x1`
/// (hazard things and the map's solid and hazard cells block it).
fn free_runs(h: &Host, x0: f32, x1: f32, y0: f32, y1: f32, hazards: &[(NodeId, [f32; 4], (f32, f32))]) -> Vec<(f32, f32)> {
    let map = level(h);
    let step = 4.0;
    let mut runs = Vec::new();
    let mut start: Option<f32> = None;
    let mut y = y0;
    while y <= y1 {
        let blocked = [x0, (x0 + x1) / 2.0, x1].iter().any(|x| flags(map, *x, y) & 5 != 0) || hazards.iter().any(|(_, b, _)| x1 > b[0] && x0 < b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]);
        match (blocked, start) {
            (false, None) => start = Some(y),
            (true, Some(s)) => {
                runs.push((s, y - step));
                start = None;
            }
            _ => {}
        }
        y += step;
    }
    if let Some(s) = start {
        runs.push((s, y1));
    }
    runs
}

fn flappy(h: &Host, p: &Thing, me: &Body) -> Vec<Intent> {
    let q = &h.quick;
    let (g, flap) = (knob(q, p, "gravity"), knob(q, p, "flap"));
    let v = view(h);
    let hazards = things_with(h, Beh::Hazard);
    let map = level(h);
    // The next thing in the way: a hazard ahead, or a column of the map with something in it.
    // (Not a floor or a ceiling: what spans the view is part of every column.)
    let mut next: Option<(f32, f32)> =
        hazards.iter().filter(|(_, b, _)| b[0] + b[2] >= me.x - me.w / 2.0 && b[2] < (v[2] - v[0]) * 0.5).map(|(_, b, _)| (b[0], b[0] + b[2])).min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(((ox, _), m)) = map {
        // A row solid or hazardous right across the view (Puffin Hop's sea)
        // is a floor or a ceiling, not something in the way.
        let spans = |y: f32| [v[0] + 1.0, me.x, v[2] - 1.0].iter().all(|x| flags(map, *x, y) & 5 != 0);
        let mut x = me.x + me.w / 2.0;
        while x < v[2] && next.is_none_or(|n| x < n.0) {
            let col = (v[1]..v[3]).step_by_f32(m.tile_h / 2.0).any(|y| flags(map, x, y) & 5 != 0 && !spans(y));
            if col {
                let cx = ((x - ox) / m.tile_w).floor();
                next = Some((ox + cx * m.tile_w, ox + (cx + 1.0) * m.tile_w));
                break;
            }
            x += m.tile_w / 2.0;
        }
    }
    let pick = |runs: Vec<(f32, f32)>| runs.into_iter().filter(|r| r.1 - r.0 >= me.h).min_by(|a, b| ((a.0 + a.1) / 2.0 - me.y).abs().total_cmp(&((b.0 + b.1) / 2.0 - me.y).abs()));
    let here = pick(free_runs(h, me.x - me.w / 2.0, me.x + me.w / 2.0, v[1], v[3], &hazards)).unwrap_or((v[1], v[3]));
    let (mut lo, mut hi) = (here.0, here.1);
    if let Some((a, b)) = next
        && let Some(gap) = pick(free_runs(h, a, b, v[1], v[3], &hazards))
        && a - (me.x + me.w / 2.0) < (v[2] - v[0]) * 0.6
    {
        (lo, hi) = (lo.max(gap.0), hi.min(gap.1));
        if lo >= hi {
            (lo, hi) = gap;
        }
    }
    // Where it'll be when a flap lands (a press takes about this long), and the top a flap would reach.
    // Flap to stay just under the gap's middle: once it'd be past there by
    // the time a flap lands, unless the flap would take it into the top.
    let tau = 0.15;
    let y_then = me.y + me.vy * tau + 0.5 * g * tau * tau;
    let apex = y_then - if g > 0.0 { flap * flap / (2.0 * g) } else { 0.0 };
    let aim = ((lo + hi) / 2.0 + 10.0).min(hi - me.h / 2.0 - 16.0);
    let room_above = apex - me.h / 2.0 > lo + 2.0;
    let falling_out = y_then + me.h / 2.0 > hi - 2.0;
    if y_then > aim && (room_above || falling_out) {
        let (cx, cy) = (h.scene.width / 2.0, h.scene.height * 0.6);
        return vec![Intent::Tap { x: cx, y: cy, size: 200.0, at: 0.0, hold: crate::bot::TAP_HOLD, new: false }];
    }
    Vec::new()
}

trait StepF32 {
    fn step_by_f32(self, step: f32) -> impl Iterator<Item = f32>;
}

impl StepF32 for std::ops::Range<f32> {
    fn step_by_f32(self, step: f32) -> impl Iterator<Item = f32> {
        let n = if step > 0.0 { ((self.end - self.start) / step).ceil().max(0.0) as usize } else { 0 };
        (0..n).map(move |i| self.start + i as f32 * step)
    }
}

/// What's in the way ahead along `dir`, from `x` to `reach` units on: a
/// hazard thing, a hazard or solid tile at body height, or a gap in the
/// floor. Its near and far x.
fn ahead(h: &Host, me: &Body, dir: f32, reach: f32) -> Option<(f32, f32, &'static str)> {
    let map = level(h);
    let hazards = things_with(h, Beh::Hazard);
    let front = me.x + dir * me.w / 2.0;
    let mut found: Option<(f32, f32, &'static str)> = None;
    for (_, b, (vx, _)) in &hazards {
        let (near, far) = if dir > 0.0 { (b[0], b[0] + b[2]) } else { (b[0] + b[2], b[0]) };
        let d = (near - front) * dir;
        // In the way: ahead (or level with it), and overlapping the body's height.
        if d >= -me.w && d <= reach && b[1] < me.feet() && b[1] + b[3] > me.top() && (*vx * dir <= 0.0 || d < reach * 0.5) && found.is_none_or(|f| (f.0 - front) * dir > d) {
            found = Some((near, far, "hazard"));
        }
    }
    let Some(((_, _), m)) = map else { return found };
    // What's at x for a body walking on: a wall, a hazard (in the body's
    // way, or on the floor it would come down on), a gap (no floor within
    // four rows), or nothing.
    let rows = [me.feet() - 2.0, me.y, me.top() + 2.0];
    let at = |x: f32| -> Option<&'static str> {
        if rows.iter().any(|y| flags(map, x, *y) & 1 != 0) {
            return Some("wall");
        }
        if rows.iter().any(|y| flags(map, x, *y) & 4 != 0) {
            return Some("hazard");
        }
        let floor = (0..4).map(|r| me.feet() + 2.0 + r as f32 * m.tile_h).find(|y| flags(map, x, *y) & 3 != 0);
        match floor {
            None => Some("gap"),
            Some(y) if flags(map, x, y) & 4 != 0 || flags(map, x, y - m.tile_h) & 4 != 0 => Some("hazard"),
            Some(_) => None,
        }
    };
    let step = m.tile_w / 4.0;
    let mut d = 0.0;
    while d <= reach && found.is_none_or(|f| (f.0 - front) * dir > d) {
        let x = front + dir * d;
        if let Some(w) = at(x) {
            // How far it goes on.
            let mut e = d;
            while e <= d + reach && at(front + dir * e) == Some(w) {
                e += step;
            }
            return Some((x, front + dir * e, w));
        }
        d += step;
    }
    found
}

fn on_ground(h: &Host, me: &Body) -> bool {
    let map = level(h);
    me.vy.abs() < 1.0 && [me.x - me.w / 2.0 + 1.0, me.x, me.x + me.w / 2.0 - 1.0].iter().any(|x| flags(map, *x, me.feet() + 2.0) & 3 != 0)
        || h.quick.things.iter().any(|t| {
            t.behs.contains(&Beh::Solid) && h.quick.hitbox(&h.scene, t.node, &t.kind).is_some_and(|b| (b[1] - me.feet()).abs() < 3.0 && b[0] < me.x + me.w / 2.0 && b[0] + b[2] > me.x - me.w / 2.0)
        })
}

fn runner(h: &Host, p: &Thing, me: &Body) -> Vec<Intent> {
    let q = &h.quick;
    let (g, jump) = (knob(q, p, "gravity"), knob(q, p, "jump"));
    let up = rise(jump, g);
    // Things come at the player (the world moves) or it runs at them.
    let hazards = things_with(h, Beh::Hazard);
    let incoming = hazards.iter().filter(|(_, b, _)| b[0] + b[2] > me.x - me.w / 2.0).map(|(_, _, v)| me.vx - v.0).fold(me.vx, f32::max);
    let speed = incoming.max(1.0);
    if !on_ground(h, me) {
        return Vec::new();
    }
    let Some((near, far, what)) = ahead(h, me, 1.0, speed * (up * 2.0 + 0.6)) else {
        return Vec::new();
    };
    // A hazard comes at its own speed (Neon Dash's drones fly faster than
    // its crates roll: timed by the fastest, every crate was jumped early).
    let speed = match what {
        "hazard" => hazards.iter().find(|(_, b, _)| (b[0] - near).abs() < 0.01).map_or(speed, |(_, _, v)| (me.vx - v.0).max(1.0)),
        _ => speed,
    };
    let front = me.x + me.w / 2.0;
    // The top of the jump over the middle of it (a gap: off at its edge).
    let t_take = match what {
        "gap" => ((near - front) - 4.0) / speed,
        _ => ((near + far) / 2.0 - me.x) / speed - up,
    };
    // Planned a second or so ahead; not once it's too late.
    if !(-0.15..=1.2).contains(&t_take) {
        return Vec::new();
    }
    let (cx, cy) = (h.scene.width / 2.0, h.scene.height * 0.6);
    vec![Intent::Tap { x: cx, y: cy, size: 200.0, at: t_take.max(0.0), hold: up * 0.9, new: true }]
}

fn platformer(h: &Host, p: &Thing, me: &Body, st: &mut State) -> Vec<Intent> {
    let q = &h.quick;
    let (g, jump, speed) = (knob(q, p, "gravity"), knob(q, p, "jump"), knob(q, p, "speed").max(1.0));
    let up = rise(jump, g);
    let apex = if g > 0.0 { jump * jump / (2.0 * g) } else { 100.0 };
    let now = h.time;
    let map = level(h);
    let (lx0, lx1) = map.map_or((f32::MIN, f32::MAX), |((ox, _), m)| (ox, ox + m.size().0));
    if st.dir == 0.0 {
        st.dir = if me.x < (lx0 + lx1) / 2.0 { 1.0 } else { -1.0 };
        st.stuck = (me.x, now);
    }
    // Goals (pickups, what `on("overlap")` watches for): one passed under is
    // jumped for, once (reached when touched).
    let tag = h.scene.get(p.node).map_or(p.kind.clone(), |n| if n.tag.is_empty() { p.kind.clone() } else { n.tag.clone() });
    let mine = [me.x - me.w / 2.0, me.top(), me.w, me.h];
    let gs = goals(h, &tag);
    for (id, b) in &gs {
        if overlap(mine, *b) && !st.done.contains(id) {
            st.done.push(*id);
        }
    }
    let grounded = on_ground(h, me);
    // Along the level: back at its end, or when stuck against something;
    // in the air it keeps going the way it jumped.
    let edge = if st.dir > 0.0 { lx1 - me.x } else { me.x - lx0 };
    if grounded && (edge < me.w * 1.5 || (now - st.stuck.1 > 2.0 && (me.x - st.stuck.0).abs() < 8.0)) {
        st.dir = -st.dir;
        st.stuck = (me.x, now);
    }
    if now - st.stuck.1 > 2.0 {
        st.stuck = (me.x, now);
    }
    let dir = if grounded || st.air == 0.0 { st.dir } else { st.air };
    st.air = if grounded { dir } else { st.air };
    let overhead = gs.iter().find(|(id, b)| {
        let dx = (b[0] + b[2] / 2.0 - me.x) * dir;
        !st.done.contains(id) && st.goal.is_none_or(|g| g.0 != *id) && (0.0..me.w).contains(&dx) && b[1] + b[3] < me.top() && b[1] + b[3] > me.top() - apex * 0.9
    });
    // Turning round at an edge: let go first (it stops where it is) rather
    // than walk on into it while the thumb comes across.
    let moving = if me.vx.abs() > 1.0 { me.vx.signum() } else { 0.0 };
    if moving != 0.0 && moving != dir && on_ground(h, me) && ahead(h, me, moving, speed * 0.6).is_some() {
        return vec![Intent::Release];
    }
    let (px, py) = canvas(h, me.x, me.y);
    let (cw, ch) = (h.scene.width, h.scene.height);
    let hold = Intent::Hold { x: (px + dir * cw * 0.3).clamp(10.0, cw - 10.0), y: py.clamp(10.0, ch - 10.0), size: HOLD_SIZE * 1.5, new: None };
    let mut out = vec![hold];
    if !grounded {
        st.jump = f64::MIN;
        return out;
    }
    // One jump at a time: until the one asked for is due (and a little
    // after, for a slow thumb), nothing new: asked again, it would be a
    // second flick that lifts the thumb in the air.
    if now < st.jump + 0.35 {
        return out;
    }
    // One thumb: a flick up from where it holds (ahead), which jumps a
    // moment into the flick and carries on the way it walks once the thumb
    // is up. It lifts and presses again where it held first, so the flick
    // is asked for about this much early (more and it takes off short of
    // gaps; the jump comes partway into the flick).
    const LEAD: f32 = 0.08;
    // Where it flicks from is where the thumb holds, so it stays ahead of
    // the player: with the camera stopped at the level's end the player
    // walks across the screen meanwhile, and a press it has caught up with
    // would land on it (a jump on the spot, and a hold that stands).
    let v = view(h);
    let pinned = map.is_some() && (v[0] <= lx0 + 1.0 || v[2] >= lx1 - 1.0);
    let hx = |at: f32| (px + dir * (cw * 0.3 + if pinned { speed * at.max(0.0) } else { 0.0 })).clamp(10.0, cw - 10.0);
    let tap = |at: f32| Intent::Swipe { angle: -std::f32::consts::FRAC_PI_2, length: ch * 0.2, from: Some((hx(at), py.clamp(ch * 0.25, ch - 10.0))), at: (at - LEAD).max(0.0), new: true };
    let due = |at: f32| now + at.max(0.0) as f64;
    // Standing in a hazard: out of it now.
    if [me.x - me.w / 4.0, me.x + me.w / 4.0].iter().any(|x| flags(map, *x, me.feet() - 2.0) & 4 != 0) {
        st.jump = due(0.0);
        out.push(tap(0.0));
        return out;
    }
    // A goal overhead: jump for it (once), unless it would come down in something.
    if let Some((id, _)) = overhead
        && ahead(h, me, dir, speed * up * 2.0 + me.w).is_none_or(|a| a.2 == "wall")
    {
        st.goal = Some((*id, now));
        st.jump = due(0.0);
        out.push(tap(0.0));
        return out;
    }
    let front = me.x + dir * me.w / 2.0;
    if let Some((near, far, what)) = ahead(h, me, dir, speed * (up * 2.0 + 0.8)) {
        let t_take = match what {
            "gap" => ((near - front) * dir - 6.0) / speed,
            "wall" => ((near - front) * dir) / speed - up * 0.6,
            _ => (((near + far) / 2.0 - me.x) * dir) / speed - up,
        };
        // Timed from its walk: standing (the thumb just came back after
        // turning round), it waits until it's walking again. (A wall it's
        // pushed up against stops it, and is jumped from there.)
        if t_take < 1.2 && (me.vx * dir > speed * 0.5 || what == "wall") {
            st.jump = due(t_take);
            out.push(tap(t_take));
        }
    }
    out
}

fn drag(h: &Host, p: &Thing, me: &Body, finger: (f32, f32, bool)) -> Vec<Intent> {
    // player.drag's x and y knobs (0 locks that axis); a paddle goes across only.
    let drag = p.behs.contains(&Beh::Drag);
    let only_x = !drag || h.quick.knob(p, "y") == 0.0;
    let only_y = drag && h.quick.knob(p, "x") == 0.0;
    let tag = h.scene.get(p.node).map_or(p.kind.clone(), |n| if n.tag.is_empty() { p.kind.clone() } else { n.tag.clone() });
    let hazards = things_with(h, Beh::Hazard);
    let v = view(h);
    // Where each goal will be when it reaches the player's line (falling
    // things), or now; the soonest reachable first.
    let mut best: Option<(f32, f32, f32)> = None;
    for (id, b) in goals(h, &tag) {
        let n = h.scene.get(id);
        let (vx, vy) = n.map_or((0.0, 0.0), |n| (n.vx, n.vy));
        let (gx, gy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
        let t = if only_x && vy.abs() > 1.0 { (me.y - gy) / vy } else { 0.0 };
        if t < 0.0 {
            continue;
        }
        let (tx, ty) = (gx + vx * t, gy + vy * t);
        if tx < v[0] || tx > v[2] {
            continue;
        }
        let d = if only_x { (tx - me.x).abs() } else { (tx - me.x).hypot(ty - me.y) };
        // Soonest first; a near one now before a far one.
        let score = t + d / 900.0;
        if best.is_none_or(|b| score < b.2) {
            best = Some((tx, ty, score));
        }
    }
    let (mut tx, mut ty) = best.map_or(((v[0] + v[2]) / 2.0, me.y), |b| (b.0, b.1));
    if only_x {
        ty = me.y;
    }
    if only_y {
        tx = me.x;
    }
    // Away from hazards near where it's going.
    for (_, b, (hvx, hvy)) in &hazards {
        let (hx, hy) = (b[0] + b[2] / 2.0 + hvx * 0.3, b[1] + b[3] / 2.0 + hvy * 0.3);
        let reach = (b[2].max(b[3]) + me.w.max(me.h)) * 0.75;
        let (dx, dy) = if only_x { (tx - hx, 0.0) } else { (tx - hx, ty - hy) };
        let near_line = !only_x || (hy - me.y).abs() < reach + hvy.abs() * 0.5;
        let d = dx.hypot(dy);
        if near_line && d < reach {
            let k = if d > 0.1 { (reach - d) / d } else { 1.0 };
            let (px, py) = if d > 0.1 { (dx * k, dy * k) } else { (reach, 0.0) };
            tx += px;
            ty += py;
        }
    }
    tx = tx.clamp(v[0] + me.w / 2.0, v[2] - me.w / 2.0);
    ty = ty.clamp(v[1] + me.h / 2.0, v[3] - me.h / 2.0);
    // The finger moves the player by as much as it moves (it keeps where it
    // came down): so it goes where the player should, from where it is.
    let (gx, gy) = canvas(h, tx, ty);
    let (px, py) = canvas(h, me.x, me.y);
    let (fx, fy) = if finger.2 { (finger.0 + gx - px, finger.1 + gy - py) } else { (px, py) };
    let (cw, ch) = (h.scene.width, h.scene.height);
    vec![Intent::Hold { x: fx.clamp(2.0, cw - 2.0), y: fy.clamp(2.0, ch - 2.0), size: 16.0, new: None }]
}
