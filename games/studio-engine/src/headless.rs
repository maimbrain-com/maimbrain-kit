//! What `mb play` (the headless player behind Studio's `play_test` and
//! `screenshot`) reads from the engine, through debug parameters (SPEC §5.1
//! `mb_param`: preview tools only, always absent on a device):
//!
//! - `studio_dump=60,120,…`: at the end of those frames' updates the engine
//!   logs `studio-frame {…}`, a JSON description of the canvas (what to draw,
//!   in art pixels, with the bundle's image paths) and the things in it. mb
//!   draws the PNG from that on the CPU, so frames need no GPU.
//! - `studio_trace`: events (`pickup`, `hurt`, `jump`, …) are logged as
//!   `studio-event <time> <event> <kind> …`.
//! - `studio_debug=Invincible=1;Score=38;Gold rush`: debug-menu picks by
//!   label (a scenario's name, a Studio item, a knob as "cat · speed=90", a
//!   game item), made before the first round as a player would.
//!
//! None of it changes how the game plays except the picks, which are what a
//! debug session would make.

use std::collections::BTreeSet;
use std::fmt::Write;

use maimbrain::debug::{Item, Pick};
use maimbrain::sys::{self, Level};

use crate::project::TILE;
use crate::{Studio, logical_size};

#[derive(Default)]
pub struct Hooks {
    dump: BTreeSet<u64>,
    picks: Option<String>,
}

impl Hooks {
    pub fn from_params() -> Hooks {
        let dump = sys::param("studio_dump").map(|s| s.split(',').filter_map(|f| f.trim().parse().ok()).collect()).unwrap_or_default();
        Hooks { dump, picks: sys::param("studio_debug") }
    }
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Studio {
    /// The hooks' work at the end of a frame's update.
    pub(crate) fn headless(&mut self) {
        if let Some(spec) = self.hooks.picks.take() {
            if self.project.is_some() && !self.rounds.playing() {
                self.pick_by_label(&spec);
            } else {
                self.hooks.picks = Some(spec); // not loaded yet
            }
        }
        if self.hooks.dump.contains(&self.frames) {
            let d = self.dump();
            sys::log(Level::Debug, &format!("studio-frame {d}"));
        }
    }

    fn pick_by_label(&mut self, spec: &str) {
        let Some(p) = self.project.clone() else { return };
        let menu = self.debug.menu(&p, &self.menu, None);
        for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (label, value) = match part.rsplit_once('=') {
                Some((l, v)) => (l.trim(), v.trim()),
                None => (part, ""),
            };
            let Some(e) = menu.find_label(label).or_else(|| menu.entries().find(|e| e.label.eq_ignore_ascii_case(label))) else {
                sys::log(Level::Warn, &format!("studio: no debug item called \"{label}\""));
                continue;
            };
            let v: f32 = match (&e.item, value) {
                (Item::Toggle(_), "" | "1" | "on" | "true") => 1.0,
                (Item::Toggle(_), _) => 0.0,
                (Item::Choice { options, .. }, v) => options.iter().position(|o| o == v).map_or_else(|| v.parse().unwrap_or(0.0), |i| i as f32),
                (_, v) => v.parse().unwrap_or(0.0),
            };
            let id = e.id;
            self.debug.pick(&p, &self.menu, Pick { id, value: v }, false);
        }
    }

    /// The canvas as JSON: its size and mode, the draw list in art pixels,
    /// the things in it, the score.
    fn dump(&self) -> String {
        let mut j = String::from("{");
        let _ = write!(j, "\"frame\":{},\"time\":{:.3}", self.frames, sys::time());
        let Some(p) = self.project.clone() else {
            j.push_str(",\"mode\":\"loading\"");
            if let Some(e) = &self.error {
                j.push_str(",\"error\":");
                quote(&mut j, e);
            }
            j.push('}');
            return j;
        };
        let card = self.card_mode(&p);
        let (w, h) = if card { (p.canvas.1.min(p.canvas.0), p.canvas.0.max(p.canvas.1)) } else { p.canvas };
        let mode = if card {
            "card"
        } else {
            match self.rounds.stage() {
                maimbrain::rounds::Stage::Title => "title",
                maimbrain::rounds::Stage::Over => "over",
                _ => "play",
            }
        };
        let _ = write!(j, ",\"mode\":\"{mode}\",\"w\":{w},\"h\":{h}");
        let (lw, lh) = logical_size(p.orientation);
        let _ = write!(j, ",\"logical\":[{lw},{lh}]");
        let live = self.round.as_ref().zip(self.last.as_ref());
        let level = live.and_then(|(v, _)| v.level()).or_else(|| p.maps.first().map(|m| m.name.clone()));
        if let Some(l) = &level {
            j.push_str(",\"level\":");
            quote(&mut j, l);
        }
        let (camx, camy) = live.map_or((0.0, 0.0), |(_, s)| (s.camx, s.camy));
        let _ = write!(j, ",\"camera\":[{camx},{camy}]");
        if let Some((v, _)) = live {
            let _ = write!(j, ",\"score\":{},\"lives\":{}", v.number("score"), v.number("lives"));
        }
        if let Some(e) = &self.error {
            j.push_str(",\"error\":");
            quote(&mut j, e);
        }
        // The draw list: ["rect", x, y, w, h, rgba] | ["circle", cx, cy, r, rgba]
        // | ["img", path, sx, sy, sw, sh, x, y, w, h, flip] (s* in the PNG's pixels).
        let mut d: Vec<String> = Vec::new();
        let rect = |d: &mut Vec<String>, x: f32, y: f32, w: f32, h: f32, c: u32| d.push(format!("[\"rect\",{x},{y},{w},{h},{c}]"));
        let img = |d: &mut Vec<String>, path: &str, src: [f32; 4], dst: [f32; 4], flip: bool| {
            let mut s = String::from("[\"img\",");
            quote(&mut s, &format!("assets/{path}"));
            let _ = write!(s, ",{},{},{},{},{},{},{},{},{}]", src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3], flip as u8);
            d.push(s);
        };
        let sprite = |d: &mut Vec<String>, si: usize, frame: u32, x: f32, y: f32, flip: bool, k: f32| {
            let s = &p.sprites[si];
            let (fw, fh) = (s.frame.0 as f32, s.frame.1 as f32);
            match &s.png {
                Some(png) => {
                    let t = s.scale as f32;
                    img(d, png, [frame as f32 * fw * t, 0.0, fw * t, fh * t], [x.round(), y.round(), fw * k, fh * k], flip);
                }
                None => {
                    let (hx, hy, hw, hh) = s.hitbox;
                    rect(d, x.round() + hx * k, y.round() + hy * k, hw * k, hh * k, p.color(s.shape.map_or(7, |sh| sh.color)));
                }
            }
        };
        let map = level.as_deref().and_then(|l| p.map(l));
        let bg = map.and_then(|m| m.background).unwrap_or(p.background);
        rect(&mut d, 0.0, 0.0, w as f32, h as f32, p.color(bg));
        if card {
            // The upright card: the hero, big, over the ground (the title card's text isn't drawn here).
            if let Some(hero) = p.hero() {
                let si = p.sprites.iter().position(|s| s.name == hero.name).unwrap_or(0);
                let k = (w as f32 * 0.45 / hero.frame.0 as f32).floor().max(1.0);
                sprite(&mut d, si, hero.anims[0].1[0], (w as f32 - hero.frame.0 as f32 * k) / 2.0, h as f32 * 0.52 - hero.frame.1 as f32 * k / 2.0, false, k);
            }
        } else {
            if let Some(m) = map {
                let t = TILE as f32;
                let (cx0, cy0) = ((camx / t).floor() as i64, (camy / t).floor() as i64);
                for cy in cy0..cy0 + (h as i64 / TILE as i64) + 2 {
                    for cx in cx0..cx0 + (w as i64 / TILE as i64) + 2 {
                        let Some((si, ti)) = m.cell(cx, cy) else { continue };
                        let set = &p.tilesets[si as usize];
                        let tile = &set.tiles[ti as usize];
                        let (x, y) = ((cx as f32 * t - camx).round(), (cy as f32 * t - camy).round());
                        match &set.png {
                            Some(png) => {
                                let k = set.scale as f32 * t;
                                img(&mut d, png, [tile.frame as f32 * k, 0.0, k, k], [x, y, t, t], false);
                            }
                            None => rect(&mut d, x, y, t, t, p.color(set.color)),
                        }
                    }
                }
            }
            match live {
                Some((v, _)) => {
                    for t in &v.out.borrow().things {
                        match t {
                            Ok(s) => sprite(&mut d, s.sprite, s.frame, s.x - camx, s.y - camy, s.flip, 1.0),
                            Err(sh) => {
                                let (x, y) = ((sh.x - sh.w / 2.0 - camx).round(), (sh.y - sh.h / 2.0 - camy).round());
                                if sh.circle {
                                    d.push(format!("[\"circle\",{},{},{},{}]", x + sh.w / 2.0, y + sh.h / 2.0, sh.w / 2.0, p.color(sh.color)));
                                } else {
                                    rect(&mut d, x, y, sh.w, sh.h, p.color(sh.color));
                                }
                            }
                        }
                    }
                }
                None => {
                    if let Some(m) = map {
                        for sp in m.spawns.iter().filter(|s| s.every.is_none()) {
                            if let Some(si) = p.sprites.iter().position(|s| s.name == sp.kind) {
                                let s = &p.sprites[si];
                                let (hx, hy, hw, hh) = s.hitbox;
                                sprite(&mut d, si, s.anims[0].1[0], sp.x - (hx + hw / 2.0), sp.y - (hy + hh / 2.0), false, 1.0);
                            }
                        }
                    }
                }
            }
        }
        let _ = write!(j, ",\"draw\":[{}]", d.join(","));
        let things: Vec<String> = live
            .map(|(v, _)| v.things())
            .unwrap_or_default()
            .into_iter()
            .map(|(k, x, y)| {
                let mut s = String::from("[");
                quote(&mut s, &k);
                let _ = write!(s, ",{x:.1},{y:.1}]");
                s
            })
            .collect();
        let _ = write!(j, ",\"things\":[{}]", things.join(","));
        let texts: Vec<String> = live
            .map(|(v, _)| {
                v.out
                    .borrow()
                    .texts
                    .iter()
                    .map(|t| {
                        let mut s = String::new();
                        quote(&mut s, &t.s);
                        s
                    })
                    .collect()
            })
            .unwrap_or_default();
        let _ = write!(j, ",\"texts\":[{}]}}", texts.join(","));
        j
    }
}
