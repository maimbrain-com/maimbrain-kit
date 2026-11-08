//! What `mb play` (the headless player behind Studio's `play_test` and
//! `screenshot`) reads from the engine, through debug parameters (SPEC
//! §5.1 `mb_param`: preview tools only, always absent on a device):
//!
//! - `studio_dump=60,120,…`: at the end of those frames' updates the engine
//!   logs `studio-frame {…}`, what's on screen in words: the mode, the
//!   card's words and the identity it's drawn in, the score (on the
//!   game-over card, the one the round sent) and lives, the nodes the camera sees (by name or tag, with their
//!   boxes in canvas units), the text drawn, a 3D game's node count and
//!   camera, and an error if one shows.
//!   (The frames themselves are drawn by mb-host on the GPU.)
//! - `studio_debug=Invincible=1;Score=38;Gold rush`: debug-menu picks by
//!   label (a scenario by its name), made on the title card once the
//!   project has loaded, as 1.0 does; they take effect as the round starts.
//!   A label the menu doesn't have is a warning in the log.
//! - `mb.ground` (the grounding check's parameter, crates/mb-cli's
//!   grounding.rs): each description adds `ground`, the SDK's `ground:`
//!   records for what stands on a floor: every Walker on the view as a
//!   figure (its feet are its `y`; lifted while it moves up or down), and
//!   the floor under it from the level's Tilemap (the top of the first
//!   solid or one-way cell below its feet, within four cells). 2.0 draws no
//!   contact shadows, so a figure says it has one (the check doesn't ask).
//!
//! - `studio_smooth` (the smoothness check, crates/mb-cli's
//!   studio/smoothness.rs): every frame of the round, `studio-pose {…}`:
//!   the 2D camera (x, y, zoom, rotation, without its shake), the 3D
//!   camera (place, where it looks, the lens), the subject (as the
//!   readability check picks it) in the world and on the screen, the
//!   screen places of the 2D things on the view (not the layers, maps or
//!   fixed HUD), and the frame's marks (`debug.mark`, a hitstop).
//!
//! Each description also has `lua`, Luau's interrupts in that frame's Lua
//! (the budget is 1M a frame, docs/STUDIO-2.md §4.6).
//!
//! None of it changes how the game plays.

use std::collections::BTreeSet;
use std::fmt::Write;

use maimbrain::rounds::Stage;
use maimbrain::scene::{Kind, View};
use maimbrain::sys::{self, Level};

use crate::Studio2;

#[derive(Default)]
pub struct Hooks {
    dump: BTreeSet<u64>,
    picks: Option<String>,
    ground: bool,
    /// `studio_readability`: the readability check's (see `Isolate`).
    pub readability: bool,
    /// `studio_smooth`: a pose a frame for the smoothness check.
    smooth: bool,
}

impl Hooks {
    pub fn from_params() -> Hooks {
        let dump = sys::param("studio_dump").map(|s| s.split(',').filter_map(|f| f.trim().parse().ok()).collect()).unwrap_or_default();
        Hooks {
            dump,
            picks: sys::param("studio_debug"),
            ground: sys::param("mb.ground").is_some_and(|v| v != "0"),
            readability: sys::param("studio_readability").is_some_and(|v| v != "0"),
            smooth: sys::param("studio_smooth").is_some_and(|v| v != "0"),
        }
    }
}

/// The readability check (`mb play --check readability`, crates/mb-cli's
/// readability.rs; docs/STUDIO-2.md §8.2). With `studio_readability` set,
/// each frame description names the subject (`round.subject`, else the
/// first quick-path player, else the node the camera follows, else a node
/// named or tagged "player"): what it is, why it's the subject, its box on
/// the screen and what's drawn behind it. A 3D subject's mb3d node is also
/// marked (`subject:<node>`, as the kit's `Node::mark_subject`), so the
/// runner can draw the world plain with it and without it. The runner then
/// has the engine draw each captured frame again with `studio_isolate` set
/// to one of these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Isolate {
    /// The frame without the HUD: the engine's (score, lives, pills) and the
    /// game's own, `fixed` nodes other than layers at order 0 or above.
    Clean,
    /// A 2D subject alone (with what hangs under it), as the frame draws
    /// it, on `SUBJECT_KEY`: where it differs from that is its silhouette.
    /// Just the key for a 3D subject (the runner masks one in mb-host).
    Subject,
}

/// The colour a subject is drawn alone on: one no game's art is likely to
/// match within the mask's tolerance (readability.rs's `MASK_DIFF`).
pub const SUBJECT_KEY: u32 = 0xff00ffff;

impl Isolate {
    /// What `studio_isolate` asks of this render (only ever set between
    /// frames by the headless runner, and only read with the check on).
    pub fn asked(hooks: &Hooks) -> Option<Isolate> {
        if !hooks.readability {
            return None;
        }
        match sys::param("studio_isolate")?.as_str() {
            "clean" => Some(Isolate::Clean),
            "subject" => Some(Isolate::Subject),
            _ => None,
        }
    }
}

/// Which node the readability check measures, and why it's that one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Subject {
    pub node: crate::api::Subject,
    pub why: &'static str,
}

/// Whether `id` is `root` or hangs under it.
pub fn under(scene: &maimbrain::scene::Scene, id: maimbrain::scene::NodeId, root: maimbrain::scene::NodeId) -> bool {
    let mut at = Some(id);
    while let Some(i) = at {
        if i == root {
            return true;
        }
        at = scene.get(i).and_then(|n| n.parent());
    }
    false
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

impl Studio2 {
    /// The hooks' work at the end of a frame's update.
    pub(crate) fn headless(&mut self) {
        if let Some(spec) = self.hooks.picks.take() {
            if self.project.is_some() && self.rounds.stage() != Stage::Playing {
                self.pick_by_label(&spec);
            } else {
                self.hooks.picks = Some(spec); // not loaded yet
            }
        }
        if self.hooks.smooth
            && self.rounds.stage() == Stage::Playing
            && let Some(p) = self.pose()
        {
            sys::log(Level::Debug, &format!("studio-pose {p}"));
        }
        if self.hooks.dump.contains(&self.frames) {
            let d = self.dump();
            sys::log(Level::Debug, &format!("studio-frame {d}"));
            // A 3D subject's mb3d node, for the runner's plain redraws.
            #[cfg(three)]
            if self.hooks.readability
                && let Some(Subject { node: crate::api::Subject::D3(id), .. }) = self.subject()
                && let Some(n) = self.on_stage().and_then(|v| v.host().three.as_ref()).and_then(|w| w.scene.get(id)).and_then(|n| n.host)
            {
                n.mark_subject();
            }
        }
    }

    /// `studio_debug`'s picks, by label, as a tester's taps in the menu.
    fn pick_by_label(&mut self, spec: &str) {
        use maimbrain::debug::{Item, Pick};
        let Some(p) = self.project.clone() else { return };
        let menu = self.debug.menu(&p, &self.menu, None, false);
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

    /// The frame in words, as JSON.
    pub(crate) fn dump(&self) -> String {
        let mut j = String::from("{");
        let _ = write!(j, "\"frame\":{},\"time\":{:.3}", self.frames, sys::time());
        let Some(p) = &self.project else {
            j.push_str(",\"mode\":\"loading\"");
            if let Some(e) = &self.error {
                j.push_str(",\"error\":");
                quote(&mut j, e);
            }
            j.push('}');
            return j;
        };
        let mode = match self.rounds.stage() {
            Stage::Title => "title",
            Stage::Playing => "play",
            Stage::Ending | Stage::Over => "over",
        };
        let _ = write!(j, ",\"mode\":\"{mode}\",\"w\":{},\"h\":{},\"units\":\"logical\"", p.size.0, p.size.1);
        if let Some(v) = &self.round {
            let h = v.host();
            // On the card: the score the round sent (the world steps on under
            // it), and lives as the HUD shows them.
            let score = if mode == "over" { self.rounds.score() as f64 } else { h.last_score };
            let _ = write!(j, ",\"score\":{},\"lives\":{}", score, crate::api::fmt_num(h.last_lives.max(0.0)));
        }
        // The cards' words (§4.11): the title card's title, the game-over
        // card's heading, and the identity they're drawn in.
        let card = match mode {
            "title" => Some(p.title.to_uppercase()),
            "over" => self.card.clone(),
            _ => None,
        };
        if let Some(c) = card {
            j.push_str(",\"card\":");
            quote(&mut j, &c);
        }
        j.push_str(",\"identity\":");
        quote(&mut j, &p.identity.label());
        if let Some(v) = self.on_stage() {
            let h = v.host();
            let view = View { w: p.size.0 as f32, h: p.size.1 as f32 };
            j.push_str(",\"things\":[");
            let mut first = true;
            for (id, (x0, y0, x1, y1)) in h.scene.on_view(&view) {
                let Some(n) = h.scene.get(id) else { continue };
                let label = if !n.name.is_empty() {
                    n.name.clone()
                } else if !n.tag.is_empty() {
                    n.tag.clone()
                } else if n.kind == Kind::Text {
                    continue; // its words are under texts
                } else {
                    n.kind.name().to_ascii_lowercase()
                };
                if !first {
                    j.push(',');
                }
                first = false;
                j.push('[');
                quote(&mut j, &label);
                let _ = write!(j, ",{:.0},{:.0},{:.0},{:.0}]", (x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0);
            }
            j.push_str("],\"texts\":[");
            let texts: Vec<&str> = h.scene.on_view(&view).into_iter().filter_map(|(id, _)| h.scene.get(id).filter(|n| n.kind == Kind::Text).map(|n| n.text.as_str())).chain(h.draw.texts()).collect();
            for (i, t) in texts.iter().enumerate() {
                if i > 0 {
                    j.push(',');
                }
                quote(&mut j, t);
            }
            let _ = write!(j, "],\"nodes\":{},\"draws\":{},\"lua\":{}", h.scene.len(), h.draw.len(), h.lua_counted);
            if self.hooks.ground {
                j.push_str(",\"ground\":");
                quote(&mut j, &ground(&h.scene, &view));
            }
            if self.hooks.readability {
                j.push_str(",\"subject\":");
                self.subject_json(&mut j, &view);
            }
            // A 3D game's world, which mb-host draws: its nodes, and the camera.
            #[cfg(three)]
            if let Some(w) = &h.three {
                let _ = write!(j, ",\"nodes3d\":{}", w.scene.len());
                // A terrain's reach ÷ detail, for mb play's budgets.
                if let Some(g) = &w.ground {
                    let d = &g.terrain.desc;
                    let _ = write!(j, ",\"terrain\":{}", (g.terrain.layout().reach / d.detail).round());
                }
                if w.cam_set {
                    let c = &w.cam;
                    let _ = write!(j, ",\"camera3d\":[{:.2},{:.2},{:.2}]", c.pos.x, c.pos.y, c.pos.z);
                }
            }
        }
        if let Some(e) = &self.error {
            j.push_str(",\"error\":");
            quote(&mut j, e);
        }
        j.push('}');
        j
    }
}

/// Things on the view a pose lists, at most.
const POSE_THINGS: usize = 32;

impl Studio2 {
    /// The round's frame for the smoothness check (`studio_smooth`), as JSON.
    fn pose(&self) -> Option<String> {
        use crate::api::Subject as S;
        let p = self.project.as_ref()?;
        let v = self.round.as_ref()?;
        let h = v.host();
        let (w, hh) = (p.size.0 as f32, p.size.1 as f32);
        let view = View { w, h: hh };
        let cam = h.scene.camera;
        // Where a world point is on the canvas, without the camera's shake (a shake is on purpose).
        let still = maimbrain::scene::Camera { shake_x: 0.0, shake_y: 0.0, ..cam };
        let mut j = String::new();
        let _ = write!(j, "{{\"f\":{},\"t\":{:.4},\"cam\":[{:.3},{:.3},{:.4},{:.4}]", self.frames, sys::time(), cam.x, cam.y, cam.scale, cam.rot);
        #[cfg(three)]
        let cam3 = h.three.as_ref().filter(|w| w.cam_set).map(|w| w.cam.camera(maimbrain::gfx3d::Vec3::ZERO));
        #[cfg(three)]
        if let Some(c) = cam3 {
            let f = c.rot.normalize() * -maimbrain::gfx3d::Vec3::Z;
            let _ = write!(j, ",\"cam3\":[{:.4},{:.4},{:.4},{:.5},{:.5},{:.5},{:.4},{:.3},{:.1}]", c.pos.x, c.pos.y, c.pos.z, f.x, f.y, f.z, c.fov_y, c.near, c.far);
        }
        if let Some(sub) = self.subject() {
            match sub.node {
                S::D2(id) => {
                    let n = h.scene.get(id)?;
                    let name = if !n.name.is_empty() { n.name.clone() } else if !n.tag.is_empty() { n.tag.clone() } else { n.kind.name().to_ascii_lowercase() };
                    let (wx, wy) = h.scene.world_position(id);
                    let (sx, sy) = if h.scene.is_fixed(id) { (wx, wy) } else { still.to_screen(w, hh, wx, wy) };
                    j.push_str(",\"subject\":[");
                    quote(&mut j, &name);
                    let _ = write!(j, ",\"2d\",{wx:.3},{wy:.3},0,{sx:.3},{sy:.3}]");
                }
                #[cfg(three)]
                S::D3(id) => {
                    if let Some(world) = h.three.as_ref()
                        && let Some(m) = world.scene.world(id)
                    {
                        let at = maimbrain::gfx3d::vec3(m.0[12], m.0[13], m.0[14]);
                        let info = world.info.get(&id.to_bits());
                        let name = info.map(|i| if !i.name.is_empty() { i.name.clone() } else { i.tag.clone() }).filter(|n| !n.is_empty()).unwrap_or_else(|| "the subject".into());
                        let [sw, sh, ox, oy, k] = h.screen;
                        let c = cam3.unwrap_or_else(|| world.cam.camera(maimbrain::gfx3d::Vec3::ZERO));
                        let pr = c.project(at, sw, sh);
                        j.push_str(",\"subject\":[");
                        quote(&mut j, &name);
                        let _ = write!(j, ",\"3d\",{:.4},{:.4},{:.4},{:.3},{:.3}]", at.x, at.y, at.z, (pr.x - ox) / k, (pr.y - oy) / k);
                    }
                }
            }
        }
        j.push_str(",\"things\":[");
        let mut n_out = 0;
        for (id, _) in h.scene.on_view(&view) {
            let Some(n) = h.scene.get(id) else { continue };
            if matches!(n.kind, Kind::Layer | Kind::Tilemap | Kind::Group | Kind::Emitter) || h.scene.is_fixed(id) {
                continue;
            }
            let label = if !n.name.is_empty() { n.name.clone() } else if !n.tag.is_empty() { n.tag.clone() } else { n.kind.name().to_ascii_lowercase() };
            let (wx, wy) = h.scene.world_position(id);
            let (sx, sy) = still.to_screen(w, hh, wx, wy);
            if n_out > 0 {
                j.push(',');
            }
            let _ = write!(j, "[{},", id.to_bits());
            quote(&mut j, &label);
            let _ = write!(j, ",{sx:.2},{sy:.2}]");
            n_out += 1;
            if n_out == POSE_THINGS {
                break;
            }
        }
        j.push_str("],\"marks\":[");
        for (i, m) in h.marks.iter().enumerate() {
            if i > 0 {
                j.push(',');
            }
            quote(&mut j, m);
        }
        j.push_str("]}");
        Some(j)
    }
}

impl Studio2 {
    /// The readability check's subject on the stage now (see [`Isolate`]).
    pub(crate) fn subject(&self) -> Option<Subject> {
        use crate::api::Subject as S;
        let h = self.on_stage()?.host();
        let alive = |s: S| match s {
            S::D2(n) => h.scene.alive(n),
            #[cfg(three)]
            S::D3(n) => h.three.as_ref().is_some_and(|w| w.scene.alive(n)),
        };
        if let Some(s) = h.subject.filter(|s| alive(*s)) {
            return Some(Subject { node: s, why: "round.subject" });
        }
        if let Some(t) = h.quick.players().find(|t| h.scene.alive(t.node)) {
            return Some(Subject { node: S::D2(t.node), why: "the player" });
        }
        #[cfg(three)]
        if let Some(f) = h.three.as_ref().and_then(|w| w.follow.as_ref()).filter(|f| alive(S::D3(f.node))) {
            return Some(Subject { node: S::D3(f.node), why: "the camera follows it" });
        }
        if let Some(f) = h.libs.follow.as_ref().filter(|f| h.scene.alive(f.node)) {
            return Some(Subject { node: S::D2(f.node), why: "the camera follows it" });
        }
        let player = |name: &str, tag: &str| name.eq_ignore_ascii_case("player") || tag.eq_ignore_ascii_case("player");
        if let Some(id) = h.scene.ids().into_iter().find(|id| h.scene.get(*id).is_some_and(|n| player(&n.name, &n.tag))) {
            return Some(Subject { node: S::D2(id), why: "it's called player" });
        }
        #[cfg(three)]
        if let Some(w) = &h.three
            && let Some(id) = w.scene.ids().into_iter().find(|id| w.info.get(&id.to_bits()).is_some_and(|i| player(&i.name, &i.tag)))
        {
            return Some(Subject { node: S::D3(id), why: "it's called player" });
        }
        None
    }

    /// The subject in a frame description: `{ "name", "why", "kind" }`, and
    /// for a 2D one its box on the screen (`box`: x0, y0, x1, y1 in screen
    /// units; none when it isn't drawn) and the nodes drawn behind it that
    /// it overlaps, nearest first (`behind`). `null` without one.
    fn subject_json(&self, j: &mut String, view: &View) {
        use crate::api::Subject as S;
        let (Some(s), Some(v), Some(p)) = (self.subject(), self.on_stage(), &self.project) else {
            j.push_str("null");
            return;
        };
        let h = v.host();
        let image = |i: Option<u16>| i.and_then(|i| p.images.get(i as usize)).map(|i| i.name.clone());
        let label = |id| {
            let n = h.scene.get(id)?;
            Some(if !n.name.is_empty() {
                n.name.clone()
            } else if !n.tag.is_empty() {
                n.tag.clone()
            } else {
                image(n.image).unwrap_or_else(|| n.kind.name().to_ascii_lowercase())
            })
        };
        match s.node {
            S::D2(id) => {
                j.push_str("{\"name\":");
                quote(j, &label(id).unwrap_or_default());
                j.push_str(",\"why\":");
                quote(j, s.why);
                j.push_str(",\"kind\":\"2d\"");
                let order = h.scene.draw_order();
                let Some(at) = order.iter().position(|e| e.0 == id) else {
                    j.push('}');
                    return;
                };
                let (ox, oy, k) = self.transform();
                let sbox = h.scene.screen_box(id, view, order[at].3);
                let _ = write!(j, ",\"box\":[{:.1},{:.1},{:.1},{:.1}]", ox + sbox.0 * k, oy + sbox.1 * k, ox + sbox.2 * k, oy + sbox.3 * k);
                // What's drawn under it there, nearest first: nodes whose box
                // overlaps its own, and the layers (which fill the view).
                let overlaps = |b: (f32, f32, f32, f32)| b.0 < sbox.2 && b.2 > sbox.0 && b.1 < sbox.3 && b.3 > sbox.1;
                let mut behind: Vec<String> = Vec::new();
                for e in order[..at].iter().rev() {
                    if under(&h.scene, e.0, id) || e.2 <= 0.0 {
                        continue;
                    }
                    let Some(n) = h.scene.get(e.0) else { continue };
                    let near = match n.kind {
                        Kind::Layer | Kind::Tilemap => true,
                        Kind::Group | Kind::Emitter => false,
                        _ => overlaps(h.scene.screen_box(e.0, view, e.3)),
                    };
                    if let Some(l) = near.then(|| label(e.0)).flatten().filter(|l| !behind.contains(l)) {
                        behind.push(l);
                    }
                    if behind.len() == 3 {
                        break;
                    }
                }
                j.push_str(",\"behind\":[");
                for (i, b) in behind.iter().enumerate() {
                    if i > 0 {
                        j.push(',');
                    }
                    quote(j, b);
                }
                j.push_str("]}");
            }
            #[cfg(three)]
            S::D3(id) => {
                let info = h.three.as_ref().and_then(|w| w.info.get(&id.to_bits()));
                let name = info.map(|i| if !i.name.is_empty() { i.name.clone() } else { i.tag.clone() }).filter(|n| !n.is_empty()).unwrap_or_else(|| "the subject".into());
                j.push_str("{\"name\":");
                quote(j, &name);
                j.push_str(",\"why\":");
                quote(j, s.why);
                j.push_str(",\"kind\":\"3d\"}");
            }
        }
    }
}

/// The grounding check's records for the view (crates/mb-cli's grounding.rs
/// reads them): `f<name>,l,t,r,b,flags,fx,base` for each Walker on the view,
/// `F<y>,<x0>,<x1>` for the floor under it.
fn ground(scene: &maimbrain::scene::Scene, view: &View) -> String {
    let map = scene.ids().into_iter().rev().find_map(|id| scene.get(id).and_then(|n| n.tilemap.as_ref().map(|m| (id, m))));
    let to_screen = |x: f32, y: f32| scene.camera.to_screen(view.w, view.h, x, y);
    let mut out = String::new();
    for (id, (x0, y0, x1, _)) in scene.on_view(view) {
        let Some(n) = scene.get(id).filter(|n| n.kind == Kind::Walker) else { continue };
        let name = if !n.name.is_empty() { n.name.clone() } else if !n.tag.is_empty() { n.tag.clone() } else { "walker".into() };
        let name: String = name.chars().filter(|c| *c != ',' && *c != ';').collect();
        let fixed = scene.is_fixed(id);
        let (wx, wy) = scene.world_position(id);
        let (fx, feet) = if fixed { (wx, wy) } else { to_screen(wx, wy) };
        // Shadow (1): 2.0 draws none, so the check doesn't ask for one. Lifted (4): moving up or down.
        let flags = 1 | if n.vy.abs() > 1.0 { 4 } else { 0 };
        let _ = write!(out, "f{name},{x0:.1},{y0:.1},{x1:.1},{feet:.1},{flags},{fx:.1},{:.1};", (x1 - x0) * 0.5);
        let Some((mid, m)) = map else { continue };
        let (ox, oy) = scene.world_position(mid);
        let (cx, cy) = (((wx - ox) / m.tile_w).floor() as i64, ((wy - oy - 0.5) / m.tile_h).floor() as i64);
        for row in cy + 1..=cy + 4 {
            if m.cell(cx, row).is_some_and(|t| t.solid || t.oneway) {
                let top = oy + row as f32 * m.tile_h;
                let (sx0, sy) = to_screen(ox + cx as f32 * m.tile_w, top);
                let (sx1, _) = to_screen(ox + (cx + 1) as f32 * m.tile_w, top);
                let _ = write!(out, "F{sy:.1},{:.1},{:.1};", sx0.min(sx1), sx0.max(sx1));
                break;
            }
        }
    }
    out
}
