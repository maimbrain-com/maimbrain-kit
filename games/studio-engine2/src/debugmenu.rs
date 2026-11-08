//! The debug menu every Studio game gets without asking (docs/STUDIO-2.md
//! §5 [debug], as engine 1.0's), on the platform's menu (`maimbrain::debug`): the
//! platform draws it, picks are replay events, debug mode plays on a copy of
//! the save, and nothing played in it counts.
//!
//! Tabs, in order:
//! - **Scenarios**: `debug/scenarios.toml`'s named sets of picks;
//! - **Studio**: jump to any level (a map, through `nextLevel`), invincible,
//!   score, lives, hitboxes, and Restart round during one;
//! - **Spawn**: any kind of thing, next to the player;
//! - **Knobs**: each behaviour's knobs as live values;
//! - **Physics** (a game with bodies): every collider drawn over the game
//!   (2D and 3D, the ground's tiles too), and physics paused;
//! - the game's own tab (named for it), from `debug.toggle`, `debug.action`,
//!   `debug.number` and `debug.choice` in its Lua.
//!
//! A pick is the new state ("invincible is on", "score is 38"). Between
//! rounds it waits for the next round; during one it applies at once,
//! except what a round only reads as it starts: a scenario, a level jump and
//! Restart round start the round again, in place, with every pick so far
//! (the platform's Restart is a new session, which forgets them all). A
//! scenario's `set` globals stay for the rounds after it, like knobs, and a
//! round gets them once `love.load` has run. Picking another scenario
//! replaces what the last one set.

use std::collections::BTreeSet;

use maimbrain::debug::{self, Menu, Pick};

use crate::project::{Project, Scalar};

/// A pick, as the round's VM applies it (`Vm::debug`).
#[derive(Clone, Debug, PartialEq)]
pub enum DebugCmd {
    Score(f64),
    Lives(f64),
    Invincible(bool),
    Hitboxes(bool),
    Spawn(String),
    /// kind, knob, value
    Knob(String, String, f64),
    /// The game's item (1-based) and its value.
    Item(usize, f64),
    Set(String, Scalar),
    /// Start on this map (`nextLevel(name)`).
    Level(String),
    /// The Physics tab: draw the colliders; hold the bodies still.
    Colliders(bool),
    PausePhysics(bool),
}

/// A knob the menu shows: kind, knob, value, min, max, step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnobRow {
    pub kind: String,
    pub knob: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

/// One of the game's `debug.*` items.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemRow {
    pub kind: String,
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub options: Vec<String>,
}

/// What the game has, for the menu (read from the title card's VM).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameMenu {
    pub kinds: Vec<String>,
    pub knobs: Vec<KnobRow>,
    pub items: Vec<ItemRow>,
    /// The game has physics (love.physics worlds or 3D bodies).
    pub physics: bool,
}

const SCENARIO: u32 = 100;
const LEVEL: u32 = 1000;
const INVINCIBLE: u32 = 2001;
const SCORE: u32 = 2002;
const LIVES: u32 = 2003;
const HITBOXES: u32 = 2004;
const RESTART: u32 = 2005;
const COLLIDERS: u32 = 2006;
const PAUSE_PHYSICS: u32 = 2007;
const SPAWN: u32 = 3000;
const KNOB: u32 = 4000;
const ITEM: u32 = 10000;
/// Knobs shown, at most (the platform's menu holds 200 items in all).
const MAX_KNOBS: usize = 120;

#[derive(Default)]
pub struct State {
    /// Where the next round starts (a level jump made between rounds).
    pub start_level: Option<String>,
    pub invincible: bool,
    pub hitboxes: bool,
    pub colliders: bool,
    pub pause_physics: bool,
    /// Knob values picked: (kind, knob, value).
    pub knobs: Vec<(String, String, f64)>,
    /// The game's items' values picked (1-based index, value).
    pub items: Vec<(usize, f64)>,
    /// Globals the scenario set: (name, value).
    pub set: Vec<(String, Scalar)>,
    /// The last scenario picked (its index), which the next one replaces.
    scenario: Option<usize>,
    /// A pick made during a round wants it started again (`take_restart`).
    restart: bool,
    /// For the next frame of a round.
    pending: Vec<DebugCmd>,
    /// One-shots picked between rounds: score and lives.
    waiting: Vec<DebugCmd>,
    /// The last scenario's score, lives and spawns, for the round it starts
    /// (the next scenario replaces them).
    scenario_once: Vec<DebugCmd>,
    pub warned: BTreeSet<String>,
}

fn label(s: &str) -> String {
    s.chars().take(debug::MAX_LABEL).collect()
}

impl State {
    /// A round's VM is ready: everything picked so far applies to it.
    pub fn round_started(&mut self) {
        let mut cmds = Vec::new();
        if let Some(l) = &self.start_level {
            cmds.push(DebugCmd::Level(l.clone()));
        }
        if self.invincible {
            cmds.push(DebugCmd::Invincible(true));
        }
        if self.hitboxes {
            cmds.push(DebugCmd::Hitboxes(true));
        }
        if self.colliders {
            cmds.push(DebugCmd::Colliders(true));
        }
        if self.pause_physics {
            cmds.push(DebugCmd::PausePhysics(true));
        }
        for (k, n, v) in &self.knobs {
            cmds.push(DebugCmd::Knob(k.clone(), n.clone(), *v));
        }
        for (i, v) in &self.items {
            cmds.push(DebugCmd::Item(*i, *v));
        }
        for (k, v) in &self.set {
            cmds.push(DebugCmd::Set(k.clone(), v.clone()));
        }
        cmds.append(&mut self.scenario_once);
        cmds.append(&mut self.waiting);
        // One-shots picked in the same go as a pick that starts the round again.
        cmds.extend(self.pending.drain(..).filter(|c| matches!(c, DebugCmd::Score(_) | DebugCmd::Lives(_) | DebugCmd::Spawn(_))));
        self.pending = cmds;
    }

    pub fn take_commands(&mut self) -> Vec<DebugCmd> {
        std::mem::take(&mut self.pending)
    }

    /// Whether the round should start again with the picks (once).
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    fn now_or_later(&mut self, cmd: DebugCmd, playing: bool) {
        if playing { self.pending.push(cmd) } else { self.waiting.push(cmd) }
    }

    fn set_knob(&mut self, kind: &str, knob: &str, v: f64, playing: bool) {
        self.knobs.retain(|(k, n, _)| !(k == kind && n == knob));
        self.knobs.push((kind.to_string(), knob.to_string(), v));
        if playing {
            self.pending.push(DebugCmd::Knob(kind.to_string(), knob.to_string(), v));
        }
    }

    pub fn pick(&mut self, p: &Project, menu: &GameMenu, pick: Pick, playing: bool) {
        let id = pick.id;
        match id {
            SCENARIO.. if id < LEVEL => {
                let i = (id - SCENARIO) as usize;
                let Some(s) = p.scenarios.get(i) else { return };
                // What the last scenario set goes back to the game's own.
                if let Some(old) = self.scenario.and_then(|j| p.scenarios.get(j)) {
                    if old.level.is_some() && self.start_level == old.level {
                        self.start_level = None;
                    }
                    if old.invincible.is_some() {
                        self.invincible = false;
                    }
                    self.knobs.retain(|(k, n, _)| !old.knobs.iter().any(|(ok, on, _)| ok == k && on == n));
                }
                self.scenario = Some(i);
                self.set = s.set.clone();
                if let Some(l) = &s.level {
                    self.start_level = Some(l.clone());
                }
                if let Some(on) = s.invincible {
                    self.invincible = on;
                }
                for (k, n, v) in &s.knobs {
                    self.set_knob(k, n, *v, false);
                }
                // One-shots for the round it starts.
                self.scenario_once = s.score.map(DebugCmd::Score).into_iter().chain(s.lives.map(DebugCmd::Lives)).chain(s.spawn.iter().cloned().map(DebugCmd::Spawn)).collect();
                self.restart = playing;
            }
            LEVEL.. if id < INVINCIBLE => {
                let Some(m) = p.maps.get((id - LEVEL) as usize) else { return };
                self.start_level = Some(m.name.clone());
                self.restart = playing;
            }
            RESTART => self.restart = playing,
            // States: a round starts with them (`round_started`), so only a round on hears them now.
            INVINCIBLE => {
                self.invincible = pick.on();
                if playing {
                    self.pending.push(DebugCmd::Invincible(self.invincible));
                }
            }
            HITBOXES => {
                self.hitboxes = pick.on();
                if playing {
                    self.pending.push(DebugCmd::Hitboxes(self.hitboxes));
                }
            }
            COLLIDERS => {
                self.colliders = pick.on();
                if playing {
                    self.pending.push(DebugCmd::Colliders(self.colliders));
                }
            }
            PAUSE_PHYSICS => {
                self.pause_physics = pick.on();
                if playing {
                    self.pending.push(DebugCmd::PausePhysics(self.pause_physics));
                }
            }
            SCORE => self.now_or_later(DebugCmd::Score(pick.value as f64), playing),
            LIVES => self.now_or_later(DebugCmd::Lives(pick.value as f64), playing),
            SPAWN.. if id < KNOB => {
                if let Some(k) = menu.kinds.get((id - SPAWN) as usize)
                    && playing
                {
                    self.pending.push(DebugCmd::Spawn(k.clone()));
                }
            }
            KNOB.. if id < ITEM => {
                if let Some(k) = menu.knobs.get((id - KNOB) as usize) {
                    let (kind, knob) = (k.kind.clone(), k.knob.clone());
                    self.set_knob(&kind, &knob, pick.value as f64, playing);
                }
            }
            ITEM.. => {
                let i = (id - ITEM) as usize;
                let Some(it) = menu.items.get(i) else { return };
                let n = i + 1;
                if it.kind != "action" {
                    self.items.retain(|(j, _)| *j != n);
                    self.items.push((n, pick.value as f64));
                }
                if playing {
                    self.pending.push(DebugCmd::Item(n, pick.value as f64));
                }
            }
            _ => {}
        }
    }

    /// The menu as it stands (`live`: this round's score and lives;
    /// `playing`: a round is on, to start again).
    pub fn menu(&self, p: &Project, menu: &GameMenu, live: Option<(f64, f64)>, playing: bool) -> Menu {
        let mut m = Menu::new();
        if !p.scenarios.is_empty() {
            m.section("Scenarios");
            for (i, s) in p.scenarios.iter().enumerate() {
                let row = m.action(SCENARIO + i as u32, &label(&s.name));
                if let Some(n) = &s.note {
                    row.hint(&n.chars().take(debug::MAX_HINT).collect::<String>());
                }
            }
        }
        m.section("Studio");
        for (i, l) in p.maps.iter().enumerate() {
            m.action(LEVEL + i as u32, &label(&format!("Jump to {}", l.name)));
        }
        m.toggle(INVINCIBLE, "Invincible", self.invincible).hint("hurt() takes no lives");
        let (score, lives) = live.unwrap_or((0.0, p.lives as f64));
        m.number(SCORE, "Score", score as f32).range(-99_999.0, 99_999.0).step(1.0);
        m.number(LIVES, "Lives", lives as f32).range(0.0, 99.0).step(1.0);
        m.toggle(HITBOXES, "Show hitboxes", self.hitboxes);
        if playing {
            m.action(RESTART, "Restart round").hint("a fresh round, keeping your picks");
        }
        if menu.physics {
            m.section("Physics");
            m.toggle(COLLIDERS, "Show colliders", self.colliders).hint("every body's shapes; asleep grey, fixed blue");
            m.toggle(PAUSE_PHYSICS, "Pause physics", self.pause_physics).hint("bodies hold still; the game goes on");
        }
        if !menu.kinds.is_empty() {
            m.section("Spawn");
            for (i, k) in menu.kinds.iter().take(40).enumerate() {
                m.action(SPAWN + i as u32, &label(&format!("Spawn {k}"))).stay();
            }
        }
        if !menu.knobs.is_empty() {
            m.section("Knobs");
            for (i, k) in menu.knobs.iter().take(MAX_KNOBS).enumerate() {
                let v = self.knobs.iter().find(|(kk, n, _)| *kk == k.kind && *n == k.knob).map_or(k.value, |x| x.2);
                m.number(KNOB + i as u32, &label(&format!("{} · {}", k.kind, k.knob)), v as f32).range(k.min as f32, k.max as f32).step(k.step as f32);
            }
        }
        if !menu.items.is_empty() {
            let title: String = p.title.chars().take(debug::MAX_TITLE).collect();
            m.section(if title.trim().is_empty() { "Game" } else { &title });
            for (i, it) in menu.items.iter().enumerate() {
                let id = ITEM + i as u32;
                let v = self.items.iter().find(|(j, _)| *j == i + 1).map_or(it.value, |x| x.1);
                let l = label(&it.label);
                match it.kind.as_str() {
                    "toggle" => {
                        m.toggle(id, &l, v != 0.0);
                    }
                    "number" => {
                        m.number(id, &l, v as f32).range(it.min as f32, it.max as f32).step(it.step.max(1e-6) as f32);
                    }
                    "choice" => {
                        let opts: Vec<&str> = it.options.iter().map(String::as_str).collect();
                        m.choice(id, &l, (v.max(0.0) as usize).min(opts.len().saturating_sub(1)), &opts);
                    }
                    _ => {
                        m.action(id, &l);
                    }
                }
            }
        }
        m
    }

    /// Declares the menu (sent only when it changed).
    pub fn show(&self, p: &Project, menu: &GameMenu, live: Option<(f64, f64)>, playing: bool) {
        debug::show(&self.menu(p, menu, live, playing));
    }
}
