//! The debug menu every Studio game gets without asking (docs/STUDIO.md
//! "Debug"), on the platform's menu (`maimbrain::debug`, kit 0.8.3): the
//! platform draws it, picks are replay events, debug mode plays on a copy of
//! the save, and nothing played in it counts.
//!
//! Tabs, in order:
//! - **Scenarios**: `debug/scenarios.toml`'s named sets of picks;
//! - **Studio**: jump to any level, invincible, score, lives, hitboxes;
//! - **Spawn**: any kind of thing, next to the player;
//! - **Knobs**: each behaviour's knobs as live values;
//! - the game's own tab (named for it), from `debug.toggle`, `debug.action`,
//!   `debug.number` and `debug.choice` in its Lua.
//!
//! A pick is the new state ("invincible is on", "score is 38"). Between
//! rounds it waits for the next round; during one it applies at once.

use std::collections::BTreeSet;

use maimbrain::debug::{self, Menu, Pick};

use crate::project::Project;
use crate::vm::{self, DebugCmd};

const SCENARIO: u32 = 100;
const LEVEL: u32 = 1000;
const INVINCIBLE: u32 = 2001;
const SCORE: u32 = 2002;
const LIVES: u32 = 2003;
const HITBOXES: u32 = 2004;
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
    /// Knob values picked: (kind, knob, value).
    pub knobs: Vec<(String, String, f64)>,
    /// The game's items' values picked (1-based index, value).
    pub items: Vec<(usize, f64)>,
    /// For the next frame of a round.
    pending: Vec<DebugCmd>,
    /// One-shots picked between rounds: score, lives, spawns, globals.
    waiting: Vec<DebugCmd>,
    pub warned: BTreeSet<String>,
}

fn label(s: &str) -> String {
    s.chars().take(debug::MAX_LABEL).collect()
}

impl State {
    /// A round's VM is ready: everything picked so far applies to it.
    pub fn round_started(&mut self) {
        let mut cmds = Vec::new();
        if self.invincible {
            cmds.push(DebugCmd::Invincible(true));
        }
        if self.hitboxes {
            cmds.push(DebugCmd::Hitboxes(true));
        }
        for (k, n, v) in &self.knobs {
            cmds.push(DebugCmd::Knob(k.clone(), n.clone(), *v));
        }
        for (i, v) in &self.items {
            cmds.push(DebugCmd::Item(*i, *v));
        }
        cmds.append(&mut self.waiting);
        self.pending = cmds;
    }

    pub fn take_commands(&mut self) -> Vec<DebugCmd> {
        std::mem::take(&mut self.pending)
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

    pub fn pick(&mut self, p: &Project, menu: &vm::Menu, pick: Pick, playing: bool) {
        let id = pick.id;
        match id {
            SCENARIO.. if id < LEVEL => {
                let Some(s) = p.scenarios.get((id - SCENARIO) as usize) else { return };
                if let Some(l) = &s.level {
                    self.start_level = Some(l.clone());
                    if playing {
                        self.pending.push(DebugCmd::Level(l.clone()));
                    }
                }
                if let Some(on) = s.invincible {
                    self.invincible = on;
                    self.now_or_later(DebugCmd::Invincible(on), playing);
                }
                for (k, n, v) in &s.knobs {
                    self.set_knob(k, n, *v, playing);
                }
                if let Some(v) = s.score {
                    self.now_or_later(DebugCmd::Score(v), playing);
                }
                if let Some(v) = s.lives {
                    self.now_or_later(DebugCmd::Lives(v), playing);
                }
                for (k, v) in &s.set {
                    self.now_or_later(DebugCmd::Set(k.clone(), v.clone()), playing);
                }
                for k in &s.spawn {
                    self.now_or_later(DebugCmd::Spawn(k.clone()), playing);
                }
            }
            LEVEL.. if id < INVINCIBLE => {
                let Some(m) = p.maps.get((id - LEVEL) as usize) else { return };
                self.start_level = Some(m.name.clone());
                if playing {
                    self.pending.push(DebugCmd::Level(m.name.clone()));
                }
            }
            INVINCIBLE => {
                self.invincible = pick.on();
                self.now_or_later(DebugCmd::Invincible(self.invincible), playing);
            }
            HITBOXES => {
                self.hitboxes = pick.on();
                self.now_or_later(DebugCmd::Hitboxes(self.hitboxes), playing);
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

    /// The menu as it stands (`live`: this round's score and lives).
    pub fn menu(&self, p: &Project, menu: &vm::Menu, live: Option<(f64, f64)>) -> Menu {
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
        m.toggle(INVINCIBLE, "Invincible", self.invincible).hint("never hurt, and the round never ends");
        let (score, lives) = live.unwrap_or((0.0, p.lives as f64));
        m.number(SCORE, "Score", score as f32).range(-99_999.0, 99_999.0).step(1.0);
        m.number(LIVES, "Lives", lives as f32).range(0.0, 99.0).step(1.0);
        m.toggle(HITBOXES, "Show hitboxes", self.hitboxes);
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
    pub fn show(&self, p: &Project, menu: &vm::Menu, live: Option<(f64, f64)>, _playing: bool) {
        debug::show(&self.menu(p, menu, live));
    }
}
