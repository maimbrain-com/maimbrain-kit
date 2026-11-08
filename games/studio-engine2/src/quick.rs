//! The quick path (docs/STUDIO-2.md §5 [quick]): engine 1.0's things and
//! behaviours on 2.0's nodes, run in Rust. A *thing* is a node the quick
//! path moves: a `Sprite` (or `Walker`, or a kind's box) tagged with its
//! kind, made by `spawn`, a map's legend, or by giving any node a
//! `behaviour`. Each frame, before `love.update`, the players' behaviours
//! read the input, every thing's behaviours run, things move (against the
//! level's tiles, with the SDK's `TileMap::move_box`, for those that
//! collide), and what happened comes back as events for the game's `on`
//! handlers: `jump`, `land`, `flap`, `hurt`, `pickup`, `shoot`, `leave`,
//! `tick` and `overlap`.
//!
//! `player.platformer` moves on `CharacterBody2D`'s controller (Phase 3b,
//! the SDK's `physics2d::character`: Rapier's, with Godot's
//! `move_and_slide` rules) in a world of the level's tiles: solid rows as
//! boxes, ledges as one-way platforms it lands on from above and jumps up
//! through, the map's sides as walls. In the 3D build (no 2D physics) it
//! moves as the others do.
//!
//! Knobs are 1.0's, in 1.0's art pixels, times the project's units an art
//! pixel (2.5, or 1 under the Arcade preset), so a 1.0 game's numbers carry
//! over and a 2.0 game's defaults feel the same on a 360-unit canvas.
//!
//! Determinism: things step in the order they were made; nothing reads a
//! clock; randomness is the round's.

use maimbrain::Rng;
use maimbrain::scene::tilemap::hit;
use maimbrain::scene::{Kind as NodeKind, NodeId, Scene};

/// A behaviour (1.0's, docs/STUDIO-API.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beh {
    Drag,
    Paddle,
    Tilt,
    Flappy,
    Runner,
    Platformer,
    Patrol,
    Chase,
    Shooter,
    Bullet,
    Pickup,
    Hazard,
    Solid,
    Spawner,
}

/// What a player behaviour asks of a player: the hook for the default bot
/// (§8.1), which plays the verb rather than the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Tap (or jump) at the right moments: `player.flappy`, `player.runner`.
    Tap,
    /// Hold toward where to go, tap above to jump: `player.platformer`.
    HoldToward,
    /// Drag (or steer) to a place: `player.drag`, `player.paddle`, `player.tilt`.
    Drag,
}

/// A knob: name, 1.0's default, min, max, step, and whether it's a distance
/// (scaled by the project's units an art pixel).
type KnobDef = (&'static str, f64, f64, f64, f64, bool);

pub const BEHAVIOURS: &[(&str, Beh)] = &[
    ("player.drag", Beh::Drag),
    ("player.paddle", Beh::Paddle),
    ("player.tilt", Beh::Tilt),
    ("player.flappy", Beh::Flappy),
    ("player.runner", Beh::Runner),
    ("player.platformer", Beh::Platformer),
    ("enemy.patrol", Beh::Patrol),
    ("enemy.chase", Beh::Chase),
    ("enemy.shooter", Beh::Shooter),
    ("bullet", Beh::Bullet),
    ("pickup", Beh::Pickup),
    ("hazard", Beh::Hazard),
    ("solid", Beh::Solid),
    ("spawner", Beh::Spawner),
];

impl Beh {
    pub fn named(s: &str) -> Option<Beh> {
        BEHAVIOURS.iter().find(|b| b.0 == s).map(|b| b.1)
    }

    pub fn name(self) -> &'static str {
        BEHAVIOURS.iter().find(|b| b.1 == self).map_or("?", |b| b.0)
    }

    /// "no behaviour called 'x' (bullet, enemy.chase, …)".
    pub fn unknown(s: &str) -> String {
        let mut known: Vec<&str> = BEHAVIOURS.iter().map(|b| b.0).collect();
        known.sort();
        format!("no behaviour called '{s}' ({})", known.join(", "))
    }

    pub fn knobs(self) -> &'static [KnobDef] {
        match self {
            Beh::Drag => &[("speed", 120.0, 0.0, 400.0, 5.0, true), ("x", 1.0, 0.0, 1.0, 1.0, false), ("y", 1.0, 0.0, 1.0, 1.0, false)],
            Beh::Paddle | Beh::Tilt => &[("speed", 160.0, 0.0, 400.0, 5.0, true)],
            Beh::Flappy => &[("gravity", 520.0, 0.0, 2000.0, 10.0, true), ("flap", 170.0, 0.0, 600.0, 5.0, true), ("max_fall", 260.0, 0.0, 1000.0, 10.0, true)],
            Beh::Runner => &[("gravity", 900.0, 0.0, 3000.0, 10.0, true), ("jump", 300.0, 0.0, 800.0, 5.0, true), ("speed", 0.0, 0.0, 400.0, 5.0, true), ("jumps", 1.0, 1.0, 3.0, 1.0, false)],
            Beh::Platformer => &[("gravity", 900.0, 0.0, 3000.0, 10.0, true), ("jump", 280.0, 0.0, 800.0, 5.0, true), ("speed", 90.0, 0.0, 300.0, 5.0, true), ("jumps", 1.0, 1.0, 3.0, 1.0, false)],
            Beh::Patrol => &[("speed", 40.0, 0.0, 300.0, 5.0, true), ("gravity", 900.0, 0.0, 3000.0, 10.0, true), ("ledges", 1.0, 0.0, 1.0, 1.0, false)],
            Beh::Chase => &[("speed", 40.0, 0.0, 300.0, 5.0, true), ("sight", 200.0, 0.0, 1000.0, 10.0, true), ("gravity", 0.0, 0.0, 3000.0, 10.0, true)],
            Beh::Shooter => &[("every", 1.5, 0.1, 10.0, 0.1, false), ("speed", 90.0, 0.0, 400.0, 5.0, true), ("aim", 1.0, 0.0, 1.0, 1.0, false)],
            Beh::Bullet => &[("speed", 120.0, 0.0, 600.0, 5.0, true)],
            Beh::Pickup => &[("value", 1.0, -100.0, 100.0, 1.0, false)],
            Beh::Hazard | Beh::Solid => &[],
            Beh::Spawner => &[("every", 1.5, 0.1, 10.0, 0.1, false), ("spread", 0.0, 0.0, 512.0, 4.0, true), ("vx", 0.0, -600.0, 600.0, 5.0, true), ("vy", 0.0, -600.0, 600.0, 5.0, true)],
        }
    }

    pub fn player(self) -> bool {
        matches!(self, Beh::Drag | Beh::Paddle | Beh::Tilt | Beh::Flappy | Beh::Runner | Beh::Platformer)
    }

    pub fn collide(self) -> bool {
        matches!(self, Beh::Flappy | Beh::Runner | Beh::Platformer | Beh::Patrol | Beh::Chase | Beh::Bullet)
    }

    /// The player's verb, for the default bot (§8.1).
    pub fn verb(self) -> Option<Verb> {
        match self {
            Beh::Flappy | Beh::Runner => Some(Verb::Tap),
            Beh::Platformer => Some(Verb::HoldToward),
            Beh::Drag | Beh::Paddle | Beh::Tilt => Some(Verb::Drag),
            _ => None,
        }
    }
}

/// A kind of thing: an image's (its `.toml`'s behaviours, knobs, hitbox,
/// spawns) or one `kind(name, props)` made or changed.
#[derive(Clone, Debug, PartialEq)]
pub struct KindDef {
    pub name: String,
    /// The project image it's drawn with; without one, a box `w` × `h`.
    pub image: Option<usize>,
    pub w: f32,
    pub h: f32,
    pub color: u32,
    pub circle: bool,
    /// In the frame's pixels: x, y, w, h.
    pub hitbox: Option<[f32; 4]>,
    pub behs: Vec<Beh>,
    pub knobs: Vec<(String, f64)>,
    pub spawns: Option<String>,
    pub order: f32,
}

impl KindDef {
    pub fn new(name: &str) -> KindDef {
        KindDef { name: name.to_string(), image: None, w: 20.0, h: 20.0, color: 0xb0b0b0ff, circle: false, hitbox: None, behs: Vec::new(), knobs: Vec::new(), spawns: None, order: 0.0 }
    }
}

#[derive(Clone, Debug, Default)]
struct State {
    ground: bool,
    coyote: f32,
    buffer: f32,
    jumps: i32,
    flap: f32,
    /// Blinking safety after a hurt.
    safe: f32,
    next: f32,
    dir: f32,
    seen: bool,
    left: bool,
    grab: Option<(f32, f32)>,
    hitx: bool,
    hity: bool,
    inited: bool,
    /// A platformer walking by touch: when the finger lifts it eases to a stop.
    touch_walk: bool,
}

/// A thing: its node, kind, behaviours and their state.
#[derive(Clone, Debug)]
pub struct Thing {
    pub node: NodeId,
    pub kind: String,
    pub behs: Vec<Beh>,
    /// Knobs set on the node (`node.knobs`), over its kind's.
    pub knobs: Vec<(String, f64)>,
    st: State,
}

/// A spawner: `spawnEvery`, or a map's `{ thing = …, every = … }`.
#[derive(Clone, Debug)]
pub struct Spawner {
    pub id: u32,
    pub kind: String,
    pub every: f64,
    next: f64,
    pub at: Option<(f32, f32)>,
    pub spread: f32,
    pub props: Vec<(String, f64)>,
    pub on: bool,
}

/// What the input did this frame, for the players' behaviours (world units).
#[derive(Clone, Debug, Default)]
pub struct Input {
    /// The mouse's press this frame (on a phone, the first finger's).
    pub tap: Option<(f32, f32)>,
    /// Any finger's press this frame (the mouse's too): a flap or a jump,
    /// so a second thumb jumps while the first keeps walking.
    pub press: Option<(f32, f32)>,
    pub down: Option<(f32, f32)>,
    pub release: bool,
    /// The walking finger flicked up this frame (`Gesture`).
    pub flick: bool,
    /// The walking finger lifted very soon after its press (a graze): a
    /// touch jump is cut short only then.
    pub early_lift: bool,
    pub jump: bool,
    pub jumpup: bool,
    pub ax: f32,
    pub ay: f32,
    /// The phone's sideways tip, −1…1, in a build with tilt (`player.tilt`).
    pub tilt: Option<f32>,
}

/// What a frame did, for the game's `on` handlers.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: &'static str,
    pub a: NodeId,
    pub b: Option<NodeId>,
    /// A pickup's value.
    pub value: f64,
}

/// A thing to make: its kind, where, and its numbers.
#[derive(Clone, Debug)]
pub struct Make {
    pub kind: String,
    pub props: Vec<(String, f64)>,
    /// The shooter that fired it (a `shoot` event once it's made).
    pub shooter: Option<NodeId>,
}

#[derive(Default)]
pub struct Quick {
    pub kinds: Vec<KindDef>,
    pub things: Vec<Thing>,
    pub spawners: Vec<Spawner>,
    next_spawner: u32,
    /// `on("overlap", a, b)` pairs, `on("tick", kind)` and `on("leave", kind)` kinds.
    pub overlaps: Vec<(String, String)>,
    pub ticks: Vec<String>,
    pub leaves: Vec<String>,
    /// `speedUp`: the world (not the players) runs this much faster.
    pub speed: f64,
    pub now: f64,
    /// Units an art pixel: knobs that are distances are 1.0's times this.
    pub unit: f32,
    /// The debug menu's Invincible.
    pub invincible: bool,
    /// The level's map: the last `Tilemap` made (or `nextLevel`'s).
    pub level: Option<NodeId>,
    /// The walking finger's path (a platformer's flick up).
    pub gesture: Gesture,
    /// The platformers' world: the level's tiles as colliders (for the level node it was made from).
    #[cfg(phys2)]
    pworld: Option<(NodeId, maimbrain::physics2d::World)>,
    /// Each platformer's character in it, and where its box was last drawn.
    #[cfg(phys2)]
    chars: Vec<(NodeId, maimbrain::physics2d::Character, (f32, f32))>,
}

/// Whether a node's box (world) overlaps another's.
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

impl Quick {
    pub fn new(unit: f32) -> Quick {
        Quick { speed: 1.0, unit, ..Default::default() }
    }

    /// The project's kinds: every image (not a tile set's sheet), with what
    /// its `.toml` says.
    pub fn from_project(p: &crate::project::Project) -> Result<Quick, String> {
        let mut q = Quick::new(p.unit());
        for (i, im) in p.images.iter().enumerate() {
            if im.name.starts_with("tiles/") {
                continue;
            }
            let mut k = KindDef::new(&im.name);
            k.image = Some(i);
            k.hitbox = im.hitbox;
            k.knobs = im.knobs.clone();
            k.spawns = im.spawns.clone();
            for b in &im.behaviours {
                k.behs.push(Beh::named(b).ok_or_else(|| format!("images/{}.toml: {}", im.name, Beh::unknown(b)))?);
            }
            // 1.0's players were drawn over the rest.
            if k.behs.iter().any(|b| b.player()) {
                k.order = 1.0;
            }
            q.kinds.push(k);
        }
        Ok(q)
    }

    pub fn kind(&self, name: &str) -> Option<&KindDef> {
        self.kinds.iter().find(|k| k.name == name)
    }

    pub fn kind_mut(&mut self, name: &str) -> &mut KindDef {
        if let Some(i) = self.kinds.iter().position(|k| k.name == name) {
            return &mut self.kinds[i];
        }
        self.kinds.push(KindDef::new(name));
        self.kinds.last_mut().unwrap()
    }

    pub fn thing(&self, node: NodeId) -> Option<&Thing> {
        self.things.iter().find(|t| t.node == node)
    }

    /// Makes `node` a thing of `kind` with these behaviours (its kind's when
    /// `behs` is None), or changes it.
    pub fn adopt(&mut self, scene: &mut Scene, node: NodeId, kind: &str, behs: Option<Vec<Beh>>) {
        let behs = behs.unwrap_or_else(|| self.kind(kind).map_or_else(Vec::new, |k| k.behs.clone()));
        match self.things.iter_mut().find(|t| t.node == node) {
            Some(t) => {
                t.behs = behs;
                t.kind = kind.to_string();
                t.st.inited = false;
            }
            None => self.things.push(Thing { node, kind: kind.to_string(), behs, knobs: Vec::new(), st: State::default() }),
        }
        if let Some(n) = scene.node_mut(node) {
            n.driven = true;
        }
    }

    pub fn set_knob(&mut self, node: NodeId, name: &str, v: f64) {
        if let Some(t) = self.things.iter_mut().find(|t| t.node == node) {
            t.knobs.retain(|k| k.0 != name);
            t.knobs.push((name.to_string(), v));
        }
    }

    /// A knob's value: the node's, its kind's, or its behaviour's default.
    pub fn knob(&self, t: &Thing, name: &str) -> f64 {
        if let Some(v) = t.knobs.iter().find(|k| k.0 == name) {
            return v.1;
        }
        if let Some(v) = self.kind(&t.kind).and_then(|k| k.knobs.iter().find(|k| k.0 == name)) {
            return v.1;
        }
        for b in &t.behs {
            if let Some(d) = b.knobs().iter().find(|d| d.0 == name) {
                return if d.5 { d.1 * self.unit as f64 } else { d.1 };
            }
        }
        0.0
    }

    /// Every knob of a kind's behaviours, as the debug menu shows them:
    /// name, value, min, max, step (in this project's units).
    pub fn kind_knobs(&self, k: &KindDef) -> Vec<(String, f64, f64, f64, f64)> {
        let mut out: Vec<(String, f64, f64, f64, f64)> = Vec::new();
        for b in &k.behs {
            for d in b.knobs() {
                if out.iter().any(|o| o.0 == d.0) {
                    continue;
                }
                let s = if d.5 { self.unit as f64 } else { 1.0 };
                let v = k.knobs.iter().find(|x| x.0 == d.0).map_or(d.1 * s, |x| x.1);
                out.push((d.0.to_string(), v, d.2 * s, d.3 * s, d.4 * s));
            }
        }
        out
    }

    pub fn is_player(t: &Thing) -> bool {
        t.behs.iter().any(|b| b.player())
    }

    /// The players, in made order (the bot's subjects).
    pub fn players(&self) -> impl Iterator<Item = &Thing> {
        self.things.iter().filter(|t| Quick::is_player(t))
    }

    pub fn spawn_every(&mut self, kind: &str, every: f64, at: Option<(f32, f32)>, spread: f32, props: Vec<(String, f64)>) -> u32 {
        self.next_spawner += 1;
        self.spawners.push(Spawner { id: self.next_spawner, kind: kind.to_string(), every, next: self.now + every, at, spread, props, on: true });
        self.next_spawner
    }

    /// A hurt from `hurt(node)`: false (no life taken) while it's safe or
    /// the menu's Invincible is on; else it blinks a while.
    pub fn hurt(&mut self, node: NodeId) -> bool {
        if self.invincible {
            return false;
        }
        match self.things.iter_mut().find(|t| t.node == node) {
            Some(t) if t.st.safe > 0.0 => false,
            Some(t) => {
                t.st.safe = 1.2;
                true
            }
            None => true,
        }
    }

    /// A node's box in the world: its hitbox (from its kind's `hitbox`, in
    /// the frame's pixels), else its whole drawn box.
    pub fn hitbox(&self, scene: &Scene, node: NodeId, kind: &str) -> Option<[f32; 4]> {
        let n = scene.get(node)?;
        let view = maimbrain::scene::View { w: scene.width, h: scene.height };
        let (x0, y0, x1, y1) = scene.screen_box(node, &view, true);
        let pad = n.line.max(0.0);
        let (x0, y0, x1, y1) = (x0 + pad, y0 + pad, x1 - pad, y1 - pad);
        if let (Some(hb), NodeKind::Sprite) = (self.kind(kind).and_then(|k| k.hitbox), n.kind) {
            let (sx, sy) = (n.sx.abs(), n.sy.abs());
            return Some([x0 + hb[0] * sx, y0 + hb[1] * sy, hb[2] * sx, hb[3] * sy]);
        }
        Some([x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)])
    }

    /// The level's bounds in the world (its map, at least the view).
    fn bounds(&self, scene: &Scene) -> [f32; 4] {
        let (w, h) = (scene.width, scene.height);
        match self.level.and_then(|l| scene.get(l).map(|n| (l, n))) {
            Some((l, n)) => {
                let (mw, mh) = n.tilemap.as_ref().map_or((w, h), |m| m.size());
                let (x, y) = scene.world_position(l);
                [x, y, mw.max(w), mh.max(h)]
            }
            None => [0.0, 0.0, w, h],
        }
    }

    /// One frame: returns its events in order, and the things spawners and
    /// shooters want made (the caller makes them, after, and `adopt`s them).
    pub fn step(&mut self, scene: &mut Scene, input: &Input, dt: f32, rng: &mut Rng) -> (Vec<Event>, Vec<Make>) {
        let mut made = Vec::new();
        for t in self.things.iter_mut() {
            if let Some(n) = scene.get(t.node)
                && !n.tag.is_empty()
                && n.tag != t.kind
            {
                t.kind = n.tag.clone();
            }
        }
        let mut ev = Vec::new();
        self.things.retain(|t| scene.alive(t.node));
        // A platformer that went takes its character out of the level's world
        // (else it stays there, an invisible box in the next one's way).
        #[cfg(phys2)]
        if let Some((_, w)) = self.pworld.as_mut() {
            let (live, dead): (Vec<_>, Vec<_>) = std::mem::take(&mut self.chars).into_iter().partition(|c| scene.alive(c.0));
            for (_, ch, _) in dead {
                ch.remove(w);
            }
            self.chars = live;
        }
        self.now += dt as f64;
        let wdt = dt * self.speed as f32;

        // Spawners: at the world's pace.
        let mut due = Vec::new();
        for s in self.spawners.iter_mut().filter(|s| s.on) {
            s.next -= (wdt - dt) as f64;
            while s.next <= self.now {
                due.push(s.clone());
                s.next += s.every;
            }
        }
        for s in due {
            let mut props = s.props.clone();
            if let Some((x, y)) = s.at {
                let jitter = if s.spread > 0.0 { (rng.f32() - 0.5) * s.spread } else { 0.0 };
                props.retain(|p| p.0 != "x" && p.0 != "y");
                props.push(("x".into(), (x + jitter) as f64));
                props.push(("y".into(), y as f64));
            }
            made.push(Make { kind: s.kind.clone(), props, shooter: None });
        }

        // Shooters and spawner things.
        let player_pos = self.players().next().and_then(|p| scene.get(p.node).map(|n| (n.x, n.y)));
        let mut shots = Vec::new();
        for i in 0..self.things.len() {
            let t = self.things[i].clone();
            for b in &t.behs {
                let every = self.knob(&t, "every") as f32;
                match b {
                    Beh::Shooter | Beh::Spawner => {
                        let st = &mut self.things[i].st;
                        if !st.inited {
                            st.next = every;
                        }
                        st.next -= wdt;
                        if st.next <= 0.0 {
                            st.next += every.max(0.05);
                            shots.push((i, *b));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (i, b) in shots {
            let t = self.things[i].clone();
            let Some(name) = self.kind(&t.kind).and_then(|k| k.spawns.clone()) else { continue };
            let Some(n) = scene.get(t.node) else { continue };
            let (x, y, flip) = (n.x, n.y, n.flip_x);
            let mut props = vec![("x".to_string(), x as f64), ("y".to_string(), y as f64)];
            if b == Beh::Shooter {
                let sp = self.knob(&t, "speed") as f32;
                let (mut vx, mut vy) = (if flip { -sp } else { sp }, 0.0);
                if self.knob(&t, "aim") != 0.0
                    && let Some((px, py)) = player_pos
                {
                    let (dx, dy) = (px - x, py - y);
                    let d = (dx * dx + dy * dy).sqrt().max(1.0);
                    (vx, vy) = (dx / d * sp, dy / d * sp);
                }
                props.push(("vx".into(), vx as f64));
                props.push(("vy".into(), vy as f64));
            } else {
                let spread = self.knob(&t, "spread") as f32;
                props[0].1 += ((rng.f32() - 0.5) * spread) as f64;
                props.push(("vx".into(), self.knob(&t, "vx")));
                props.push(("vy".into(), self.knob(&t, "vy")));
            }
            made.push(Make { kind: name, props, shooter: (b == Beh::Shooter).then_some(t.node) });
        }

        // The players read the input.
        let view = {
            let (w, h) = (scene.width, scene.height);
            let (x0, y0) = scene.camera.to_world(w, h, 0.0, 0.0);
            let (x1, y1) = scene.camera.to_world(w, h, w, h);
            [x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()]
        };
        for i in 0..self.things.len() {
            let t = self.things[i].clone();
            if !Quick::is_player(&t) {
                continue;
            }
            for b in &t.behs {
                self.input(scene, i, *b, input, dt, view, &mut ev);
            }
        }

        // Behaviours, then motion (players at the round's pace, the rest at the world's).
        for i in 0..self.things.len() {
            let t = self.things[i].clone();
            let tdt = if Quick::is_player(&t) { dt } else { wdt };
            for b in &t.behs {
                self.update(scene, i, *b, tdt, player_pos);
            }
            self.things[i].st.inited = true;
        }
        let solids: Vec<(NodeId, [f32; 4])> = self.things.iter().filter(|t| t.behs.contains(&Beh::Solid)).filter_map(|t| self.hitbox(scene, t.node, &t.kind).map(|b| (t.node, b))).collect();
        for i in 0..self.things.len() {
            let t = self.things[i].clone();
            let tdt = if Quick::is_player(&t) { dt } else { wdt };
            self.motion(scene, i, tdt, &solids, &mut ev);
            let st = &mut self.things[i].st;
            if st.safe > 0.0 {
                st.safe -= dt;
                if let Some(n) = scene.node_mut(t.node) {
                    n.visible = st.safe <= 0.0 || (st.safe * 10.0).floor() as i32 % 2 != 0;
                }
            }
        }

        // Touches: pickups and hazards on players.
        let boxes: Vec<(usize, Option<[f32; 4]>)> = self.things.iter().enumerate().map(|(i, t)| (i, self.hitbox(scene, t.node, &t.kind))).collect();
        for (pi, p) in self.things.iter().enumerate() {
            if !Quick::is_player(p) || !scene.alive(p.node) {
                continue;
            }
            let Some(pb) = boxes[pi].1 else { continue };
            for (ti, t) in self.things.iter().enumerate() {
                if ti == pi || !scene.alive(t.node) {
                    continue;
                }
                let (pickup, hazard) = (t.behs.contains(&Beh::Pickup), t.behs.contains(&Beh::Hazard));
                if !(pickup || hazard) || !boxes[ti].1.is_some_and(|b| overlaps(pb, b)) {
                    continue;
                }
                if pickup {
                    let value = self.knob(t, "value");
                    ev.push(Event { name: "pickup", a: p.node, b: Some(t.node), value });
                } else {
                    ev.push(Event { name: "hazard", a: p.node, b: Some(t.node), value: 0.0 });
                }
            }
        }
        // Picked up: gone (the event still names it).
        for e in &ev {
            if e.name == "pickup"
                && let Some(b) = e.b
                && let Some(n) = scene.node_mut(b)
            {
                n.visible = false;
            }
        }

        // `on("overlap", a, b)`: tagged boxes, in the tags' order.
        for (pair, (a, b)) in self.overlaps.clone().into_iter().enumerate() {
            let list = |tag: &str| -> Vec<NodeId> { if tag == "*" { scene.ids() } else { scene.all(tag) } };
            let (la, lb) = (list(&a), list(&b));
            for x in &la {
                let Some(bx) = self.box_of(scene, *x) else { continue };
                for y in &lb {
                    if x == y {
                        continue;
                    }
                    if let Some(by) = self.box_of(scene, *y)
                        && overlaps(bx, by)
                    {
                        ev.push(Event { name: "overlap", a: *x, b: Some(*y), value: pair as f64 + 1.0 });
                    }
                }
            }
        }

        // `on("tick", kind)`: each thing of the kind.
        for k in self.ticks.clone() {
            for n in scene.all(&k) {
                ev.push(Event { name: "tick", a: n, b: None, value: 0.0 });
            }
        }

        // Leaving: out of the level, having been in it (or far away).
        let [bx, by, bw, bh] = self.bounds(scene);
        let (vw, vh) = (scene.width, scene.height);
        for i in 0..self.things.len() {
            let t = &self.things[i];
            if t.st.left {
                continue;
            }
            let Some([x, y, w, h]) = self.hitbox(scene, t.node, &t.kind) else { continue };
            let out = x + w < bx || x > bx + bw || y + h < by || y > by + bh;
            let far = x + w < bx - vw || x > bx + bw + vw || y + h < by - vh || y > by + bh + vh;
            if !out {
                self.things[i].st.seen = true;
            } else if t.st.seen || far {
                self.things[i].st.left = true;
                let player = Quick::is_player(&self.things[i]) as u8 as f64;
                ev.push(Event { name: "leave", a: self.things[i].node, b: None, value: player });
            }
        }
        (ev, made)
    }

    /// Any node's box: its thing's hitbox, else its drawn box.
    fn box_of(&self, scene: &Scene, node: NodeId) -> Option<[f32; 4]> {
        let kind = self.thing(node).map(|t| t.kind.clone()).or_else(|| scene.get(node).map(|n| n.tag.clone())).unwrap_or_default();
        let b = self.hitbox(scene, node, &kind)?;
        (b[2] > 0.0 || b[3] > 0.0).then_some(b)
    }

    #[allow(clippy::too_many_arguments)]
    fn input(&mut self, scene: &mut Scene, i: usize, b: Beh, inp: &Input, dt: f32, view: [f32; 4], ev: &mut Vec<Event>) {
        let t = self.things[i].clone();
        let node = t.node;
        let k = |name: &str| self.knob(&t, name) as f32;
        let (speed, kx, ky, flap, jump) = (k("speed"), k("x"), k("y"), k("flap"), k("jump"));
        let hb = self.hitbox(scene, node, &t.kind);
        let Some(n) = scene.node_mut(node) else { return };
        let st = &mut self.things[i].st;
        let drag = |n: &mut maimbrain::scene::Node, st: &mut State, xs: bool, ys: bool| {
            if let Some((tx, ty)) = inp.tap {
                st.grab = Some((n.x - tx, n.y - ty));
            }
            match (inp.down, st.grab) {
                (Some((x, y)), Some((ox, oy))) => {
                    if xs {
                        n.x = x + ox;
                    }
                    if ys {
                        n.y = y + oy;
                    }
                }
                _ => {
                    if xs {
                        n.x += inp.ax * speed * dt;
                    }
                    if ys {
                        n.y += inp.ay * speed * dt;
                    }
                }
            }
            if inp.release {
                st.grab = None;
            }
        };
        match b {
            Beh::Drag => drag(n, st, kx != 0.0, ky != 0.0),
            Beh::Paddle => drag(n, st, true, false),
            Beh::Tilt => {
                drag(n, st, true, false);
                // Tipping the phone steers it, while no finger drags it (1.0's rule).
                if let (Some(t), None) = (inp.tilt, inp.down) {
                    n.x += t * speed * dt;
                }
            }
            Beh::Flappy => {
                if inp.press.is_some() || inp.jump {
                    n.vy = -flap;
                    st.flap = 0.25;
                    ev.push(Event { name: "flap", a: node, b: None, value: 0.0 });
                }
            }
            Beh::Runner => {
                if jumper(n, st, inp, dt, inp.press.is_some(), jump, CUT, inp.release || inp.jumpup) {
                    ev.push(Event { name: "jump", a: node, b: None, value: 0.0 });
                }
            }
            Beh::Platformer => {
                // Touch (docs/STUDIO-2-API.md): a finger held beside the
                // player, at any height, walks it that way; held over it,
                // it stands. Jumps, each the moment it happens: a second
                // finger's tap, the walking finger flicked up, or a press
                // on the player itself (one thumb, standing). Keys and pads
                // as ever.
                let (cx, half) = hb.map_or((n.x, 0.0), |b| (b[0] + b[2] / 2.0, b[2] / 2.0));
                let beside = |x: f32| (x - cx).abs() > half + BESIDE;
                let mut dir = inp.ax;
                let mut touch = false;
                if let Some((x, _)) = inp.down
                    && beside(x)
                {
                    (dir, touch) = ((x - cx).signum(), true);
                }
                let on_me = inp.tap.is_some_and(|(x, _)| !beside(x));
                let second = inp.press.is_some() && inp.tap.is_none();
                if dir != 0.0 {
                    n.vx = dir * speed;
                } else if st.touch_walk && inp.down.is_none() {
                    // The walking finger lifted: on the ground it eases to a
                    // stop (a lift still stops it short of an edge); in the
                    // air it keeps its way until it lands.
                    if st.ground {
                        let step = speed / EASE * dt;
                        n.vx = if n.vx.abs() <= step { 0.0 } else { n.vx - step * n.vx.signum() };
                    }
                } else {
                    n.vx = 0.0;
                }
                st.touch_walk = touch || (st.touch_walk && inp.down.is_none() && n.vx != 0.0);
                if jumper(n, st, inp, dt, on_me || second || inp.flick, jump, CUT, inp.early_lift || inp.jumpup) {
                    ev.push(Event { name: "jump", a: node, b: None, value: 0.0 });
                }
            }
            _ => {}
        }
        // Players stay in view.
        if matches!(b, Beh::Drag | Beh::Paddle | Beh::Tilt)
            && let Some(bx) = self.hitbox(scene, node, &t.kind)
            && let Some(n) = scene.node_mut(node)
        {
            let (hw, hh) = (bx[2] / 2.0, bx[3] / 2.0);
            let (cx, cy) = (bx[0] + hw, bx[1] + hh);
            let ncx = cx.clamp(view[0] + hw, (view[0] + view[2] - hw).max(view[0] + hw));
            let ncy = cy.clamp(view[1] + hh, (view[1] + view[3] - hh).max(view[1] + hh));
            n.x += ncx - cx;
            n.y += ncy - cy;
        }
    }

    fn update(&mut self, scene: &mut Scene, i: usize, b: Beh, dt: f32, player: Option<(f32, f32)>) {
        let t = self.things[i].clone();
        let k = |name: &str| self.knob(&t, name) as f32;
        let (gravity, speed, sight, jumps, ledges) = (k("gravity"), k("speed"), k("sight"), k("jumps"), k("ledges"));
        let max_fall = k("max_fall");
        let level = self.level.and_then(|l| scene.get(l).map(|n| (l, n)).and_then(|(l, n)| n.tilemap.as_ref().map(|m| (l, (**m).clone()))));
        let hb = self.hitbox(scene, t.node, &t.kind);
        let origin = level.as_ref().map(|(l, _)| scene.world_position(*l)).unwrap_or_default();
        let Some(n) = scene.node_mut(t.node) else { return };
        let st = &mut self.things[i].st;
        match b {
            Beh::Flappy => {
                n.vy = (n.vy + gravity * dt).min(max_fall);
                if st.flap > 0.0 {
                    st.flap -= dt;
                }
            }
            Beh::Runner => {
                if st.ground {
                    st.jumps = jumps as i32 - 1;
                }
                n.vy += gravity * dt;
                n.vx = speed;
            }
            Beh::Platformer => {
                if st.ground {
                    st.jumps = jumps as i32 - 1;
                }
                n.vy += gravity * dt;
                if n.vx != 0.0 {
                    n.flip_x = n.vx < 0.0;
                }
            }
            Beh::Patrol => {
                if !st.inited {
                    st.dir = if n.flip_x { 1.0 } else { -1.0 };
                }
                if st.hitx {
                    st.dir = -st.dir;
                }
                // Turn at a ledge rather than walk off it.
                if ledges != 0.0
                    && st.ground
                    && let (Some((_, m)), Some(bx)) = (&level, hb)
                {
                    let ax = bx[0] + bx[2] / 2.0 + st.dir * (bx[2] / 2.0 + 1.0) - origin.0;
                    let ay = bx[1] + bx[3] + 2.0 - origin.1;
                    let (cx, cy) = m.cell_of(ax, ay);
                    if m.flags(cx, cy) & 3 == 0 {
                        st.dir = -st.dir;
                    }
                }
                n.vx = st.dir * speed;
                n.vy = if gravity > 0.0 { n.vy + gravity * dt } else { 0.0 };
                n.flip_x = st.dir < 0.0;
            }
            Beh::Chase => {
                let near = player.filter(|(px, py)| ((px - n.x).powi(2) + (py - n.y).powi(2)).sqrt() <= sight);
                match near {
                    Some((px, py)) => {
                        let (dx, dy) = (px - n.x, py - n.y);
                        let d = (dx * dx + dy * dy).sqrt().max(1.0);
                        n.vx = dx / d * speed;
                        if gravity == 0.0 {
                            n.vy = dy / d * speed;
                        }
                        n.flip_x = dx < 0.0;
                    }
                    None => {
                        n.vx = 0.0;
                        if gravity == 0.0 {
                            n.vy = 0.0;
                        }
                    }
                }
                if gravity > 0.0 {
                    n.vy += gravity * dt;
                }
            }
            Beh::Bullet if !st.inited && n.vx == 0.0 && n.vy == 0.0 => {
                n.vx = if n.flip_x { -speed } else { speed };
            }
            _ => {}
        }
    }

    /// The platformers' world for the level now, made (again) when the level changes.
    #[cfg(phys2)]
    fn platform_world(&mut self, scene: &Scene) -> bool {
        use maimbrain::physics2d::{Body, Shape, World, vec2};
        let Some(l) = self.level.filter(|l| scene.alive(*l)) else {
            self.pworld = None;
            self.chars.clear();
            return false;
        };
        if self.pworld.as_ref().is_some_and(|(n, _)| *n == l) {
            return true;
        }
        let Some(m) = scene.get(l).and_then(|n| n.tilemap.as_ref()).map(|m| (**m).clone()) else { return false };
        let (ox, oy) = scene.world_position(l);
        let mut w = World::with_gravity(vec2(0.0, 0.0));
        let ground = w.add_body(Body::fixed(0.0, 0.0));
        let (tw, th) = (m.tile_w, m.tile_h);
        // Each row's runs of solid tiles as one box, and of ledges as one one-way box.
        for cy in 0..m.rows as i64 {
            for want in [1u8, 2] {
                let mut cx = 0i64;
                while cx < m.cols as i64 {
                    let run = |c: i64| m.flags(c, cy) & want != 0 && (want == 1 || m.flags(c, cy) & 1 == 0);
                    if !run(cx) {
                        cx += 1;
                        continue;
                    }
                    let start = cx;
                    while cx < m.cols as i64 && run(cx) {
                        cx += 1;
                    }
                    let (x, y) = (ox + start as f32 * tw, oy + cy as f32 * th);
                    let wdt = (cx - start) as f32 * tw;
                    // A ledge is solid only in its top few units: a body lands on its top.
                    let h = if want == 1 { th } else { th.min(tw * 0.25).max(2.0) };
                    let s = Shape::rect(wdt, h).at(x + wdt / 2.0, y + h / 2.0).friction(0.0);
                    w.add_shape(ground, if want == 2 { s.one_way() } else { s });
                }
            }
        }
        // Beyond the left and right edges is wall (as TileMap::flags says), as tall as the map and more.
        let (mw, mh) = m.size();
        for x in [ox - tw * 2.0, ox + mw + tw * 2.0] {
            w.add_shape(ground, Shape::rect(tw * 4.0, mh * 3.0).at(x, oy + mh / 2.0).friction(0.0));
        }
        self.pworld = Some((l, w));
        self.chars.clear();
        true
    }

    /// A platformer moves on its character (CharacterBody2D's controller).
    #[cfg(phys2)]
    fn platform_motion(&mut self, scene: &mut Scene, i: usize, dt: f32, b: [f32; 4]) -> Option<(bool, bool, bool)> {
        use maimbrain::physics2d::{Character, CharacterSettings, Shape, vec2};
        if !self.platform_world(scene) {
            return None;
        }
        let node = self.things[i].node;
        let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
        let (_, w) = self.pworld.as_mut().unwrap();
        let k = match self.chars.iter().position(|c| c.0 == node) {
            Some(k) => k,
            None => {
                // A box a hair narrower than the hitbox, its corners rounded, so it slides past tile seams.
                let s = Shape::round_rect(b[2] - 1.0, b[3], (b[2] * 0.15).min(4.0));
                let set = CharacterSettings { floor_snap_length: 2.0, ..CharacterSettings::default() };
                self.chars.push((node, Character::new(w, vec2(cx, cy), s, set, 0), (cx, cy)));
                self.chars.len() - 1
            }
        };
        let level = self.level.and_then(|l| scene.get(l).and_then(|n| n.tilemap.as_ref().map(|m| (scene.world_position(l), m.tile_h))));
        let (_, ch, drawn) = &mut self.chars[k];
        // Lua (or a tween, a respawn) moved it: it starts from there.
        if (drawn.0 - cx).abs() > 0.01 || (drawn.1 - cy).abs() > 0.01 {
            ch.set_position(w, vec2(cx, cy));
        }
        let n = scene.node_mut(node)?;
        let v = ch.move_and_slide(w, vec2(n.vx, n.vy), dt, false);
        let p = ch.position();
        // Drawn standing on the tile's top, not the controller's sliver above it.
        // (A move of a thousandth of a unit is float noise from meters and back: none.)
        let mut draw = (if (p.x - cx).abs() < 1e-3 { cx } else { p.x }, p.y);
        if ch.on_floor()
            && let Some(((_, oy), th)) = level
        {
            let bottom = p.y + b[3] / 2.0;
            let top = ((bottom - oy) / th).round() * th + oy;
            if (bottom - top).abs() < 0.5 {
                draw.1 = top - b[3] / 2.0;
            }
        }
        *drawn = draw;
        n.x += draw.0 - cx;
        n.y += draw.1 - cy;
        n.vx = v.x;
        n.vy = v.y;
        Some((ch.on_wall(), ch.on_floor() || ch.on_ceiling(), ch.on_floor()))
    }

    /// Moves a thing by its velocity (and its own `gravity`): against the
    /// level's tiles and the solid things if it collides.
    fn motion(&mut self, scene: &mut Scene, i: usize, dt: f32, solids: &[(NodeId, [f32; 4])], ev: &mut Vec<Event>) {
        let t = self.things[i].clone();
        let collide = t.behs.iter().any(|b| b.collide());
        let player = Quick::is_player(&t);
        let level = self.level.and_then(|l| scene.get(l).and_then(|n| n.tilemap.as_ref().map(|m| (scene.world_position(l), (**m).clone()))));
        let hb = self.hitbox(scene, t.node, &t.kind);
        #[cfg(phys2)]
        if collide
            && t.behs.contains(&Beh::Platformer)
            && let Some(b) = hb
            && level.is_some()
        {
            if let Some(n) = scene.node_mut(t.node) {
                n.vy += n.gravity * dt;
            }
            let x0 = scene.get(t.node).map_or(0.0, |n| n.x);
            let y0 = scene.get(t.node).map_or(0.0, |n| n.y);
            if let Some((hitx, hity, ground)) = self.platform_motion(scene, i, dt, b) {
                // Thorns: a hazard tile it overlaps hurts.
                let ((ox, oy), m) = level.as_ref().unwrap();
                let Some(n) = scene.get(t.node) else { return };
                let (nb_x, nb_y) = (b[0] + n.x - x0, b[1] + n.y - y0);
                let (_, _, f) = m.move_box(nb_x + b[2] / 2.0 - ox, nb_y + b[3] / 2.0 - oy, b[2], b[3], 0.0, 0.0, false);
                if f & hit::HAZARD != 0 && player {
                    ev.push(Event { name: "hazard", a: t.node, b: None, value: 0.0 });
                }
                self.finish_motion(scene, i, solids, b, (x0, y0), (hitx, hity, ground), ev);
                return;
            }
        }
        let Some(n) = scene.node_mut(t.node) else { return };
        n.vy += n.gravity * dt;
        let (dx, dy) = (n.vx * dt, n.vy * dt);
        let (x0, y0) = (n.x, n.y);
        if !collide {
            n.x += dx;
            n.y += dy;
            return;
        }
        let (mut hitx, mut hity, mut ground) = (false, false, false);
        match (level, hb) {
            (Some(((ox, oy), m)), Some(b)) => {
                let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
                let (nx, ny, f) = m.move_box(cx - ox, cy - oy, b[2], b[3], dx, dy, false);
                n.x += nx + ox - cx;
                n.y += ny + oy - cy;
                hitx = f & hit::SIDE != 0;
                hity = f & (hit::GROUND | hit::CEILING) != 0;
                ground = f & hit::GROUND != 0;
                if f & hit::GROUND != 0 && n.vy > 0.0 {
                    n.vy = 0.0;
                }
                if f & hit::CEILING != 0 && n.vy < 0.0 {
                    n.vy = 0.0;
                }
                if f & hit::HAZARD != 0 && player {
                    ev.push(Event { name: "hazard", a: t.node, b: None, value: 0.0 });
                }
            }
            _ => {
                n.x += dx;
                n.y += dy;
            }
        }
        match hb {
            Some(b) => self.finish_motion(scene, i, solids, b, (x0, y0), (hitx, hity, ground), ev),
            None => self.finish_motion(scene, i, solids, [0.0; 4], (x0, y0), (hitx, hity, ground), ev),
        }
    }

    /// After a thing moved: solid things block it, and what it hit and
    /// whether it landed are kept (and a bullet that hit something goes).
    #[allow(clippy::too_many_arguments)]
    fn finish_motion(&mut self, scene: &mut Scene, i: usize, solids: &[(NodeId, [f32; 4])], b: [f32; 4], (x0, y0): (f32, f32), (mut hitx, mut hity, mut ground): (bool, bool, bool), ev: &mut Vec<Event>) {
        let t = self.things[i].clone();
        let hb = self.hitbox(scene, t.node, &t.kind).map(|_| b);
        let Some(n) = scene.node_mut(t.node) else { return };
        let st = &mut self.things[i].st;
        let was = st.ground;
        // Solid things block, as solid tiles do.
        if let Some(b) = hb {
            let (mx, my) = (n.x - x0, n.y - y0);
            let mut tb = [b[0] + mx, b[1] + my, b[2], b[3]];
            for (s, sb) in solids {
                if *s == t.node || !overlaps(tb, *sb) {
                    continue;
                }
                let (tcx, tcy, scx, scy) = (tb[0] + tb[2] / 2.0, tb[1] + tb[3] / 2.0, sb[0] + sb[2] / 2.0, sb[1] + sb[3] / 2.0);
                let ox = (tb[2] + sb[2]) / 2.0 - (tcx - scx).abs();
                let oy = (tb[3] + sb[3]) / 2.0 - (tcy - scy).abs();
                if ox < oy {
                    let d = if tcx < scx { -ox } else { ox };
                    n.x += d;
                    tb[0] += d;
                    hitx = true;
                } else {
                    let d = if tcy < scy { -oy } else { oy };
                    if d < 0.0 {
                        n.vy = n.vy.min(0.0);
                        ground = true;
                    } else {
                        n.vy = n.vy.max(0.0);
                    }
                    n.y += d;
                    tb[1] += d;
                    hity = true;
                }
            }
        }
        st.hitx = hitx;
        st.hity = hity;
        st.ground = ground;
        // A bullet that hits a wall is gone.
        if (hitx || hity) && t.behs.contains(&Beh::Bullet) {
            ev.push(Event { name: "gone", a: t.node, b: None, value: 0.0 });
        }
        if ground && !was {
            ev.push(Event { name: "land", a: t.node, b: None, value: 0.0 });
        }
    }
}

/// The jump cut: letting go early keeps this much of the rise.
const CUT: f32 = 0.45;

/// 1.0's jump: coyote time, a buffered press, extra jumps in the air, and a
/// lower jump when let go early. True when it jumped.
fn jumper(n: &mut maimbrain::scene::Node, st: &mut State, i: &Input, dt: f32, tap: bool, jump: f32, cut: f32, let_go: bool) -> bool {
    st.coyote = if st.ground { 0.1 } else { (st.coyote - dt).max(0.0) };
    if i.jump || tap {
        st.buffer = 0.12;
    }
    let mut jumped = false;
    if st.buffer > 0.0 {
        st.buffer -= dt;
        if st.coyote > 0.0 || st.jumps > 0 {
            if st.coyote <= 0.0 {
                st.jumps -= 1;
            }
            n.vy = -jump;
            st.ground = false;
            st.coyote = 0.0;
            st.buffer = 0.0;
            jumped = true;
        }
    }
    if let_go && n.vy < 0.0 {
        n.vy *= cut;
    }
    jumped
}

/// A platformer's touch (docs/STUDIO-2-API.md): a finger beyond this many
/// units either side of the player's box walks it that way; nearer, a press
/// jumps and a hold stands.
pub const BESIDE: f32 = 16.0;
/// A flick up: the walking finger rises this share of the view's height
/// within `FLICK_SECS`, more up than sideways.
pub const FLICK_RISE: f32 = 0.06;
pub const FLICK_SECS: f32 = 0.15;
/// A touch jump is cut short only when its finger lifts this soon after the
/// press (a graze); a tap's ordinary 80–150 ms gets the whole jump.
pub const EARLY_LIFT: f32 = 0.06;
/// A touch walk eases to a stop over this long once the finger lifts (in the
/// air it keeps its way until it lands).
pub const EASE: f32 = 0.1;

/// The walking finger's recent path, for flicks (canvas units, game time).
#[derive(Clone, Debug, Default)]
pub struct Gesture {
    path: Vec<(f32, f32, f32)>,
    /// A flick can jump (re-armed when the finger comes back down, or a new press).
    armed: bool,
    /// Since the last flick: its highest point (smallest y).
    top: f32,
    /// When the finger went down.
    pressed: f32,
    down: bool,
    clock: f32,
}

impl Gesture {
    /// One frame (`dt` long) of the walking finger (`at`: where it's held, in
    /// canvas units, if it is; `fresh`: it went down this frame), in a view
    /// `view_h` tall. Returns (a flick up this frame, a lift this soon after the press).
    pub fn feed(&mut self, dt: f32, at: Option<(f32, f32)>, fresh: bool, view_h: f32) -> (bool, bool) {
        self.clock += dt;
        let t = self.clock;
        let Some((x, y)) = at else {
            let early = self.down && t - self.pressed < EARLY_LIFT;
            self.down = false;
            self.path.clear();
            return (false, early);
        };
        if fresh || !self.down {
            (self.path, self.armed, self.pressed, self.down) = (Vec::new(), true, t, true);
        }
        self.path.retain(|p| t - p.0 <= FLICK_SECS);
        self.path.push((t, x, y));
        let rise = FLICK_RISE * view_h;
        if !self.armed {
            self.top = self.top.min(y);
            if y - self.top >= rise * 0.5 {
                (self.armed, self.path) = (true, vec![(t, x, y)]);
            }
            return (false, false);
        }
        // From the lowest point of the last moment to here.
        let low = self.path.iter().fold((t, x, y), |a, p| if p.2 > a.2 { *p } else { a });
        let (up, side) = (low.2 - y, (x - low.1).abs());
        if up >= rise && up >= side * 0.7 {
            (self.armed, self.top) = (false, y);
            return (true, false);
        }
        (false, false)
    }
}
