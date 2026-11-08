//! The UI kit (docs/STUDIO-2.md §4.12, §5 [ui]): the HUD and the identity
//! at run time, the widgets, hints, lessons, the countdown, the stats card
//! and the combo meter, all drawn in the game's identity. The logic is the
//! SDK's (`maimbrain::ui`: `Widget`, `Hints`, `Lesson`, `Countdown`,
//! `StatsCard`; `juice::Combo`); this is how Lua reaches it.
//!
//! - `ui.hud{ score, best, lives, timer, meter, combo, level }` says what the
//!   engine's HUD shows; `ui.identity(name)` changes the identity mid-round.
//! - `ui.button`, `ui.panel`, `ui.pill`, `ui.bar`, `ui.ring`, `ui.icon` make
//!   nodes (`Kind::Widget`), fixed to the screen unless `fixed = false`.
//!   A button's press is taken before LÖVE's callbacks hear it (its touch
//!   is the button's, not the game's); Enter or a pad's A press the
//!   focused one, and the arrows and d-pad move the focus.
//! - `ui.hint{ text, at, priority }`: one on screen at a time, by priority,
//!   following `at`; `ui.lesson{ prompt, slow, learned }`: a hint taught
//!   once, with slow motion capped at 3 s. The difficulty bot reads both.
//! - `ui.countdown(n, fn)`, `ui.stats{ … }` (the world holds while it's up)
//!   and `ui.combo{ window }`.

use std::ffi::{c_int, c_void};

use maimbrain::scene::{Kind, NodeId, View};
use maimbrain::ui::hints::Priority;
use maimbrain::ui::{Ask, ButtonKind, Countdown, Focus, Hint, Hints, Lesson, Rect, StatFormat, StatsCard, Tap, Tone, Widget, WidgetKind, Wipe, icon_name, icon_named};

use super::atoms::A;
use super::node::{boolean, key_at, node_at, number, parent_at, push_node, set, string, unknown};
use super::*;
use crate::input::LoveEvent;

/// One HUD element: off, on (the engine's), a value of the game's, or the
/// game's own node (the engine writes the value into its `value` Text).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum El {
    #[default]
    Off,
    On,
    Num(f64),
    Text(String),
    Node(NodeId),
}

impl El {
    pub fn on(&self) -> bool {
        !matches!(self, El::Off)
    }
}

/// What the HUD shows.
#[derive(Clone, Debug, PartialEq)]
pub struct HudSpec {
    pub score: El,
    pub best: El,
    pub lives: El,
    /// Seconds it counts down from, by `round.time`.
    pub timer: El,
    /// 0–1, a bar.
    pub meter: El,
    pub combo: El,
    pub level: El,
}

impl HudSpec {
    /// The default (§4.12): the score, the lives when there's more than
    /// one, the level when there's more than one map.
    pub fn default_for(lives: u32, maps: usize, scored: bool) -> HudSpec {
        HudSpec {
            score: if scored { El::On } else { El::Off },
            best: El::Off,
            lives: if lives > 1 { El::On } else { El::Off },
            timer: El::Off,
            meter: El::Off,
            combo: El::Off,
            level: if maps > 1 { El::On } else { El::Off },
        }
    }
}

/// What a hint or lesson points at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    /// Nothing: it sits low in the middle.
    Free,
    Node(NodeId),
    /// A 3D node (its id's bits), projected through the camera.
    Node3(u32),
    /// A place on the canvas.
    Point(f32, f32),
}

/// A hint the game made (`ui.hint`), until it's dismissed or its `at` goes.
#[derive(Clone, Debug)]
pub struct HintEntry {
    pub id: u32,
    pub hint: Hint,
    pub at: At,
}

/// A lesson (`ui.lesson`): the SDK's, its prompt as a hint, and the game's
/// `learned` check (a registry ref, 0 for none).
#[derive(Clone, Debug)]
pub struct LessonEntry {
    pub lesson: Lesson,
    pub at: At,
    pub learned_ref: c_int,
}

/// `ui.stats{ … }` until the engine lays the card out (it needs the screen),
/// then the card; `done` (a registry ref) runs once the screen is covered.
#[derive(Clone, Debug)]
pub struct Stats {
    pub title: String,
    pub rows: Vec<(String, f32, StatFormat, bool)>,
    pub rank: Option<String>,
    pub next: Option<(String, String)>,
    pub wipe: Wipe,
    pub manual: bool,
    pub done_ref: c_int,
    pub card: Option<StatsCard>,
    pub covered: bool,
}

/// The round's UI beyond its nodes: see the module docs.
#[derive(Debug, Default)]
pub struct UiState {
    /// The hints on screen (one at a time), from the entries and lessons.
    pub hints: Hints,
    pub entries: Vec<HintEntry>,
    next_hint: u32,
    pub lessons: Vec<LessonEntry>,
    /// The countdown and the function it calls at GO (a registry ref).
    pub countdown: Option<(Countdown, c_int)>,
    pub stats: Option<Stats>,
    pub combos: Vec<maimbrain::juice::Combo>,
    /// The combo the HUD shows (the last made).
    pub hud_combo: Option<usize>,
    /// Buttons' `tap` functions (registry refs).
    pub taps: Vec<(NodeId, c_int)>,
    pub focus: Focus<NodeId>,
    /// The button a pointer is pressing, and the touch behind it (its
    /// touch events are the button's too).
    held: Option<(NodeId, Option<u8>)>,
    /// Releases to keep from the game: Enter's and A's after they pressed a button.
    swallow: Vec<&'static str>,
}

impl UiState {
    /// Whether a frame's input has to go past the buttons (any made, or a
    /// press or key still theirs).
    pub fn wants_input(&self, scene: &maimbrain::scene::Scene) -> bool {
        self.held.is_some() || !self.swallow.is_empty() || !self.taps.is_empty() || scene.has_widgets()
    }

    /// The press a button has (`Some`), and the touch behind it if a finger's.
    pub fn button_touch(&self) -> Option<Option<u8>> {
        self.held.map(|(_, t)| t)
    }

    /// Whether a stats card is up and not yet covering the screen.
    pub fn stats_up(&self) -> bool {
        self.stats.as_ref().is_some_and(|s| !s.covered)
    }

    /// How fast the world runs (1: full speed): the slowest lesson's slow
    /// motion, and held (0) while a stats card covers it.
    pub fn rate(&self) -> f32 {
        if self.stats.as_ref().is_some_and(|s| !s.covered) {
            return 0.0;
        }
        self.lessons.iter().map(|l| l.lesson.rate()).fold(1.0, f32::min)
    }
}

/// The UI kit's handle tags (api/mod.rs `tag`).
pub mod tag_ui {
    pub use super::tag::{COMBO, HINT, LESSON};
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(
            L,
            ud,
            "ui",
            &[
                (c"hud", hud),
                (c"identity", identity),
                (c"button", mk_button),
                (c"panel", mk_panel),
                (c"pill", mk_pill),
                (c"bar", mk_bar),
                (c"ring", mk_ring),
                (c"icon", mk_icon),
                (c"countdown", countdown),
                (c"hint", hint),
                (c"lesson", lesson),
                (c"stats", stats),
                (c"combo", combo),
                (c"color", color),
            ],
        );
        register(L, ud, "__engine", &[(c"ui", ui_frame)]);
        register_methods(L, ud, tag_ui::HINT, c"Hint", &[(c"dismiss", hint_dismiss), (c"showing", hint_showing)]);
        register_methods(L, ud, tag_ui::LESSON, c"Lesson", &[(c"learned", lesson_learned), (c"showing", lesson_showing)]);
        register_kind(L, ud, tag_ui::COMBO, c"Combo", &[(c"__index", combo_index), (c"__namecall", combo_call)]);
    }
}

unsafe fn el(L: *mut lua_State, i: c_int, key: &str) -> Result<El, String> {
    unsafe {
        Ok(match lua_type(L, i) {
            LUA_TBOOLEAN if lua_toboolean(L, i) != 0 => El::On,
            LUA_TBOOLEAN => El::Off,
            LUA_TNUMBER => El::Num(lua_tonumber(L, i)),
            LUA_TSTRING => El::Text(to_s(L, i)),
            LUA_TUSERDATA => match node_at(L, i) {
                Some(n) => El::Node(n),
                None => return Err(format!("ui.hud: {key} takes true, false, a value or one of your nodes")),
            },
            _ => return Err(format!("ui.hud: {key} takes true, false, a value or one of your nodes, got {}", type_name(L, i))),
        })
    }
}

// `ui.hud{ … }`: merges (pass only what changed).
b!(hud, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "ui.hud", "table");
    }
    let mut spec = h.hud.clone();
    lua_pushnil(L);
    while lua_next(L, 1) != 0 {
        let key = st(L, -2).unwrap_or("").to_string();
        let v = match el(L, -1, &key) {
            Ok(v) => v,
            Err(m) => {
                lua_pop(L, 2);
                return err(L, &m);
            }
        };
        let slot = match key.as_str() {
            "score" => &mut spec.score,
            "best" => &mut spec.best,
            "lives" => &mut spec.lives,
            "timer" => &mut spec.timer,
            "meter" => &mut spec.meter,
            "combo" => &mut spec.combo,
            "level" => &mut spec.level,
            _ => {
                lua_pop(L, 2);
                return err(L, &format!("ui.hud: no `{key}` (score, best, lives, timer, meter, combo, level)"));
            }
        };
        *slot = v;
        lua_pop(L, 1);
    }
    h.hud = spec;
    0
});

// `ui.identity(name or words)`: the cards and HUD change with it.
b!(identity, |L, h| match st(L, 1) {
    Some(s) => {
        h.identity_change = Some(s.to_string());
        0
    }
    None => bad(L, 1, "ui.identity", "identity name or description"),
});

// `ui.color(name)` → the identity's colour as 0xRRGGBB: what a game's own
// Text on a ui.panel is written in, say.
b!(color, |L, h| {
    let th = &h.scene.theme;
    let c = match st(L, 1) {
        Some("text") => th.text,
        Some("dim") => th.text_dim,
        Some("accent") => th.accent,
        Some("onAccent") => th.on_accent,
        Some("panel") => th.panel_top,
        Some("good") => th.good,
        Some("bad") => th.bad,
        Some("gold") => th.gold,
        Some("outline") => th.outline,
        Some("background") => th.bg_top,
        _ => return err(L, "ui.color takes \"text\", \"dim\", \"accent\", \"onAccent\", \"panel\", \"good\", \"bad\", \"gold\", \"outline\" or \"background\""),
    };
    ret(L, (c >> 8) as f64)
});

// ---- Widgets ---------------------------------------------------------------

/// The widget on a node.
fn widget(h: &Host, id: NodeId) -> Option<&Widget> {
    h.scene.get(id).and_then(|n| n.widget.as_deref())
}

fn widget_mut(h: &mut Host, id: NodeId) -> Option<&mut Widget> {
    h.scene.node_mut(id).and_then(|n| n.widget.as_deref_mut())
}

/// A widget's own property (node.rs asks first): `None` for what every
/// node has.
pub unsafe fn widget_get(L: *mut lua_State, h: &Host, id: NodeId, n: &maimbrain::scene::Node, a: A) -> Option<c_int> {
    unsafe {
        let w = n.widget.as_deref()?;
        Some(match a {
            A::Text => rets(L, w.text()),
            A::Value => ret(L, w.value as f64),
            A::Icon => match w.button.icon.and_then(icon_name) {
                Some(s) => rets(L, s),
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::Enabled => retb(L, w.button.enabled),
            A::Tone => rets(L, w.tone.name()),
            A::ButtonKind => rets(L, w.button.kind.name()),
            A::Focus => retb(L, h.ui.focus.on == Some(id)),
            A::Tap => match h.ui.taps.iter().find(|t| t.0 == id) {
                Some((_, r)) => {
                    lua_rawgeti(L, LUA_REGISTRYINDEX, *r);
                    1
                }
                None => {
                    lua_pushnil(L);
                    1
                }
            },
            A::W | A::H => {
                let (lw, lh) = h.scene.local_size(id);
                ret(L, if a == A::W { lw } else { lh } as f64)
            }
            _ => return None,
        })
    }
}

/// Writes a widget's own property; `None` for what every node has.
pub unsafe fn widget_set(L: *mut lua_State, h: &mut Host, id: NodeId, a: A, v: c_int) -> Option<Result<(), String>> {
    unsafe {
        let kind = widget(h, id)?.kind;
        if !matches!(a, A::Text | A::Value | A::Icon | A::Enabled | A::Tone | A::ButtonKind | A::Focus | A::Tap) {
            return None;
        }
        let key = a.name();
        let only = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("a ui.{} has no {key} ({what})", kind.name())) };
        let r: Result<(), String> = (|| {
            match a {
                A::Text => {
                    only(matches!(kind, WidgetKind::Button | WidgetKind::Pill | WidgetKind::Ring), "buttons, pills and rings have text")?;
                    let s = string(L, v, key)?;
                    widget_mut(h, id).unwrap().set_text(&s);
                }
                A::Value => {
                    only(matches!(kind, WidgetKind::Bar | WidgetKind::Ring), "bars and rings have a value")?;
                    let x = number(L, v, key)?;
                    widget_mut(h, id).unwrap().value = x.clamp(0.0, 1.0);
                }
                A::Icon => {
                    let icon = match lua_type(L, v) {
                        LUA_TNIL => None,
                        _ => {
                            let s = string(L, v, key)?;
                            Some(icon_named(&s).ok_or_else(|| format!("icon: no icon \"{s}\" ({})", ICONS.join(", ")))?)
                        }
                    };
                    if kind == WidgetKind::Icon && icon.is_none() {
                        return Err("a ui.icon's icon can't be nil".into());
                    }
                    widget_mut(h, id).unwrap().button.icon = icon;
                }
                A::Enabled => {
                    only(kind == WidgetKind::Button, "only buttons are enabled or not")?;
                    let on = boolean(L, v, key)?;
                    let w = widget_mut(h, id).unwrap();
                    w.button.enabled = on;
                    if !on {
                        w.button.reset();
                    }
                }
                A::Tone => {
                    let s = string(L, v, key)?;
                    widget_mut(h, id).unwrap().tone = Tone::named(&s).ok_or_else(|| format!("tone is \"plain\", \"accent\", \"good\", \"bad\" or \"gold\", got \"{s}\""))?;
                }
                A::ButtonKind => {
                    only(kind == WidgetKind::Button, "buttons are primary, secondary or ghost")?;
                    let s = string(L, v, key)?;
                    widget_mut(h, id).unwrap().button.kind = ButtonKind::named(&s).ok_or_else(|| format!("kind is \"primary\", \"secondary\" or \"ghost\", got \"{s}\""))?;
                }
                A::Focus => {
                    only(kind == WidgetKind::Button, "only buttons take the focus")?;
                    if boolean(L, v, key)? {
                        h.ui.focus.on = Some(id);
                    } else if h.ui.focus.on == Some(id) {
                        h.ui.focus.on = None;
                    }
                }
                A::Tap => {
                    only(kind == WidgetKind::Button, "only buttons are tapped")?;
                    let r = match lua_type(L, v) {
                        LUA_TFUNCTION => {
                            lua_pushvalue(L, v);
                            let r = lua_ref(L, -1);
                            lua_pop(L, 1);
                            Some(r)
                        }
                        LUA_TNIL => None,
                        _ => return Err(format!("tap takes a function, got {}", type_name(L, v))),
                    };
                    if let Some(i) = h.ui.taps.iter().position(|t| t.0 == id) {
                        lua_unref(L, h.ui.taps[i].1);
                        h.ui.taps.remove(i);
                    }
                    if let Some(r) = r {
                        h.ui.taps.push((id, r));
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        Some(r)
    }
}

const ICONS: [&str; 33] = {
    let mut out = [""; 33];
    let mut i = 0;
    while i < 33 {
        out[i] = maimbrain::ui::Icon::ALL[i].0;
        i += 1;
    }
    out
};

/// Makes a widget node from the table at `t` (or none): what every node
/// takes, the widget's own, and `r` (a ring's radius) and `size` (an
/// icon's) as its `w` and `h`.
unsafe fn make_widget(L: *mut lua_State, h: &mut Host, kind: WidgetKind, t: c_int, ctor: &str) -> c_int {
    unsafe {
        let tt = lua_type(L, t);
        if tt != LUA_TTABLE && tt != LUA_TNIL && tt != -1 {
            return err(L, &format!("{ctor} takes one table: {ctor}{{ x = 180, y = 320 }} (got {})", type_name(L, t)));
        }
        let id = match h.scene.add(Kind::Widget, None) {
            Ok(id) => id,
            Err(_) => return err(L, &format!("too many nodes ({} at once): destroy what's gone, or reuse nodes", maimbrain::scene::MAX_NODES)),
        };
        {
            let n = h.scene.node_mut(id).unwrap();
            n.widget = Some(Box::new(Widget::new(kind)));
            n.fixed = true;
        }
        if tt == LUA_TTABLE {
            let mut r: Result<(), String> = Ok(());
            lua_pushnil(L);
            while lua_next(L, t) != 0 {
                let (a, key) = key_at(L, -2);
                r = match (a, key.as_str()) {
                    (_, "r") if kind == WidgetKind::Ring => number(L, -1, "r").map(|x| {
                        let n = h.scene.node_mut(id).unwrap();
                        (n.w, n.h) = (x.max(1.0) * 2.0, x.max(1.0) * 2.0);
                    }),
                    (Some(A::Size), _) if kind == WidgetKind::Icon => number(L, -1, "size").map(|x| {
                        let n = h.scene.node_mut(id).unwrap();
                        (n.w, n.h) = (x.max(1.0), x.max(1.0));
                    }),
                    (Some(A::Parent), _) => parent_at(L, h, id, -1, false),
                    (Some(a), _) if super::node::has(Kind::Widget, a) && !super::node::is_method(a) => set(L, h, id, a, -1),
                    _ if lua_type(L, -2) != LUA_TSTRING => Err(format!("{ctor}{{…}} takes names = values, got a {} key", type_name(L, -2))),
                    _ => Err(unknown_widget(kind, &key)),
                };
                if r.is_err() {
                    lua_pop(L, 2);
                    break;
                }
                lua_pop(L, 1);
            }
            if let Err(m) = r {
                super::node::destroy_node(L, h, id);
                return err(L, &format!("{ctor}{{…}}: {m}"));
            }
        }
        if let Some(w) = widget_mut(h, id) {
            w.settle();
        }
        push_node(L, h, id);
        1
    }
}

fn unknown_widget(kind: WidgetKind, key: &str) -> String {
    let m = unknown(Kind::Widget, key, false);
    m.replace("Widget", &format!("ui.{}", kind.name()))
}

b!(mk_button, |L, h| make_widget(L, h, WidgetKind::Button, 1, "ui.button"));
b!(mk_panel, |L, h| make_widget(L, h, WidgetKind::Panel, 1, "ui.panel"));
b!(mk_pill, |L, h| make_widget(L, h, WidgetKind::Pill, 1, "ui.pill"));
b!(mk_bar, |L, h| make_widget(L, h, WidgetKind::Bar, 1, "ui.bar"));
b!(mk_ring, |L, h| make_widget(L, h, WidgetKind::Ring, 1, "ui.ring"));

// `ui.icon(name, { x, y, size, tone })`.
b!(mk_icon, |L, h| {
    let Some(name) = st(L, 1).map(str::to_string) else { return bad(L, 1, "ui.icon", "icon name") };
    let Some(icon) = icon_named(&name) else { return err(L, &format!("ui.icon: no icon \"{name}\" ({})", ICONS.join(", "))) };
    let r = make_widget(L, h, WidgetKind::Icon, 2, "ui.icon");
    if r == 1
        && let Some(id) = node_at(L, -1)
        && let Some(w) = widget_mut(h, id)
    {
        w.button.icon = Some(icon);
    }
    r
});

/// The pointer events a frame's buttons take, before LÖVE's callbacks: a
/// press on a button (its touch area at least 44 × 44) is the button's
/// until it lifts, and a release on it taps it; Enter or a pad's A tap the
/// focused button; the arrows and the d-pad move the focus (and still
/// reach the game). Returns the events the game gets, and queues the taps'
/// functions to run this frame.
pub fn route_input(h: &mut Host, events: &[LoveEvent]) -> Vec<LoveEvent> {
    use maimbrain::input::{Event, Kind as K};
    // The press that started the round is the round's, never a button's.
    if h.frames <= 1 {
        return events.to_vec();
    }
    let view = View { w: h.project.size.0 as f32, h: h.project.size.1 as f32 };
    // The buttons on screen, top first, with their boxes.
    let mut buttons: Vec<(NodeId, Rect)> = Vec::new();
    for (id, _, alpha, fixed) in h.scene.draw_order().into_iter().rev() {
        if alpha <= 0.0 {
            continue;
        }
        let Some(w) = widget(h, id) else { continue };
        if w.kind != WidgetKind::Button || !w.button.enabled {
            continue;
        }
        let (x0, y0, x1, y1) = h.scene.screen_box(id, &view, fixed);
        buttons.push((id, Rect::new(x0, y0, x1 - x0, y1 - y0)));
    }
    // Focus: made order (draw order's reverse is top first).
    let mut made: Vec<(NodeId, f32, f32)> = buttons.iter().map(|(id, r)| (*id, r.cx(), r.cy())).collect();
    made.sort_by_key(|b| h.scene.made_order(b.0));
    h.ui.focus.keep(&made);
    if buttons.is_empty() && h.ui.held.is_none() && h.ui.swallow.is_empty() {
        return events.to_vec();
    }
    let rect_of = |id: NodeId, list: &[(NodeId, Rect)]| list.iter().find(|b| b.0 == id).map(|b| b.1);
    let mut out = Vec::with_capacity(events.len());
    let mut taps: Vec<NodeId> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let pointer = |kind: K, x: f32, y: f32| Event { kind: kind as u8, x, y, id: 0, ..Default::default() };
        let took = match *e {
            LoveEvent::MousePressed { x, y, button: 1, istouch } if h.ui.held.is_none() => {
                h.ui.focus.shown = false;
                let hit = buttons.iter().find(|(_, r)| r.at_least(maimbrain::ui::layout::MIN_TOUCH, maimbrain::ui::layout::MIN_TOUCH).contains(x, y)).copied();
                match hit {
                    Some((id, r)) if widget_mut(h, id).and_then(|w| w.press(&pointer(K::MouseDown, x, y), r)).is_some() => {
                        // The touch behind a finger's press is the button's too.
                        let touch = istouch
                            .then(|| events[..i].iter().rev().find_map(|p| match *p {
                                LoveEvent::TouchPressed { id, x: tx, y: ty, .. } if tx == x && ty == y => Some(id),
                                _ => None,
                            }))
                            .flatten();
                        h.ui.held = Some((id, touch));
                        true
                    }
                    _ => false,
                }
            }
            LoveEvent::MouseMoved { x, y, .. } => match h.ui.held {
                Some((id, _)) => {
                    let r = rect_of(id, &buttons).unwrap_or_default();
                    if let Some(w) = widget_mut(h, id) {
                        w.press(&pointer(K::MouseMove, x, y), r);
                    }
                    true
                }
                None => false,
            },
            LoveEvent::MouseReleased { x, y, button: 1, .. } => match h.ui.held.take() {
                Some((id, _)) => {
                    let r = rect_of(id, &buttons).unwrap_or_default();
                    if widget_mut(h, id).and_then(|w| w.press(&pointer(K::MouseUp, x, y), r)) == Some(Tap::Clicked) {
                        taps.push(id);
                    }
                    true
                }
                None => false,
            },
            LoveEvent::TouchPressed { id, .. } | LoveEvent::TouchMoved { id, .. } | LoveEvent::TouchReleased { id, .. } => {
                // Its press comes before the mouse event that takes it: look ahead.
                let ahead = matches!(e, LoveEvent::TouchPressed { .. })
                    && events[i + 1..].iter().find_map(|p| match *p {
                        LoveEvent::MousePressed { x, y, button: 1, istouch: true } => Some((x, y)),
                        _ => None,
                    }).is_some_and(|(x, y)| {
                        let LoveEvent::TouchPressed { x: tx, y: ty, .. } = *e else { return false };
                        h.ui.held.is_none() && tx == x && ty == y && buttons.iter().any(|(_, r)| r.at_least(maimbrain::ui::layout::MIN_TOUCH, maimbrain::ui::layout::MIN_TOUCH).contains(x, y))
                    });
                ahead || h.ui.held.is_some_and(|(_, t)| t == Some(id))
            }
            LoveEvent::KeyPressed(k @ "return") | LoveEvent::GamepadPressed(k @ "a") => {
                h.ui.focus.shown = true;
                match h.ui.focus.on.filter(|f| buttons.iter().any(|b| b.0 == *f)) {
                    Some(id) => {
                        if widget_mut(h, id).and_then(Widget::activate).is_some() {
                            taps.push(id);
                        }
                        h.ui.swallow.push(k);
                        true
                    }
                    None => false,
                }
            }
            LoveEvent::KeyReleased(k) | LoveEvent::GamepadReleased(k) if h.ui.swallow.contains(&k) => {
                h.ui.swallow.retain(|s| *s != k);
                true
            }
            LoveEvent::KeyPressed(k) | LoveEvent::GamepadPressed(k) => {
                h.ui.focus.shown = true;
                let dir = match k {
                    "up" | "dpup" => Some((0.0, -1.0)),
                    "down" | "dpdown" => Some((0.0, 1.0)),
                    "left" | "dpleft" => Some((-1.0, 0.0)),
                    "right" | "dpright" => Some((1.0, 0.0)),
                    _ => None,
                };
                if let Some((dx, dy)) = dir
                    && made.len() > 1
                {
                    h.ui.focus.step(&made, dx, dy);
                }
                false
            }
            _ => false,
        };
        if !took {
            out.push(e.clone());
        }
    }
    // Its button went (destroyed, hidden, disabled): the press is forgotten.
    if let Some((id, _)) = h.ui.held
        && !buttons.iter().any(|b| b.0 == id)
    {
        h.ui.held = None;
    }
    for id in taps {
        if let Some((_, r)) = h.ui.taps.iter().find(|t| t.0 == id) {
            h.libs.pending.push(crate::libs::Due::Call { f: *r, args: Vec::new(), pass_self: false, timer: None });
        }
    }
    let focus = h.ui.focus.on;
    let shown = h.ui.focus.shown;
    for (id, _) in &buttons {
        if let Some(w) = widget_mut(h, *id) {
            w.focused = focus == Some(*id);
            w.ring = shown;
        }
    }
    out
}

// ---- Hints and lessons -----------------------------------------------------

unsafe fn at_field(L: *mut lua_State, t: c_int, f: &str) -> Result<At, String> {
    unsafe {
        lua_rawgetfield(L, t, c"at".as_ptr());
        let r = match lua_type(L, -1) {
            LUA_TNIL => Ok(At::Free),
            LUA_TUSERDATA => match node_at(L, -1) {
                Some(n) => Ok(At::Node(n)),
                None => match handle(L, -1, tag::NODE3) {
                    Some(b) => Ok(At::Node3(b)),
                    None => Err(format!("{f}: at takes a node or {{ x, y }}, got {}", type_name(L, -1))),
                },
            },
            LUA_TTABLE => {
                lua_rawgeti(L, -1, 1);
                lua_rawgeti(L, -2, 2);
                let p = (num(L, -2), num(L, -1));
                lua_pop(L, 2);
                match p {
                    (Some(x), Some(y)) => Ok(At::Point(x as f32, y as f32)),
                    _ => Err(format!("{f}: at takes a node or {{ x, y }}")),
                }
            }
            _ => Err(format!("{f}: at takes a node or {{ x, y }}, got {}", type_name(L, -1))),
        };
        lua_pop(L, 1);
        r
    }
}

unsafe fn str_field(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<String> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let s = (lua_type(L, -1) == LUA_TSTRING || lua_type(L, -1) == LUA_TNUMBER).then(|| to_s(L, -1));
        lua_pop(L, 1);
        s
    }
}

unsafe fn num_field(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<f64> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let v = num(L, -1);
        lua_pop(L, 1);
        v
    }
}

/// A function field as a registry ref (0: none).
unsafe fn fn_field(L: *mut lua_State, t: c_int, k: &std::ffi::CStr, f: &str) -> Result<c_int, String> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let r = match lua_type(L, -1) {
            LUA_TFUNCTION => Ok(lua_ref(L, -1)),
            LUA_TNIL => Ok(0),
            _ => Err(format!("{f}: {} takes a function, got {}", k.to_str().unwrap_or(""), type_name(L, -1))),
        };
        lua_pop(L, 1);
        r
    }
}

fn ask_named(s: &str) -> Option<Ask> {
    Some(match s {
        "tap" => Ask::Tap,
        "hold" => Ask::Hold,
        "let go" | "letgo" | "release" => Ask::LetGo,
        "look" | "none" => Ask::Look,
        _ => return None,
    })
}

// `ui.hint{ text, at, priority = "teach", ask, linger }` → a Hint.
b!(hint, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "ui.hint", "table");
    }
    let Some(text) = str_field(L, 1, c"text") else { return err(L, "ui.hint: text is the hint's words (5 at most, a verb first: \"TAP TO JUMP\")") };
    let at = match at_field(L, 1, "ui.hint") {
        Ok(a) => a,
        Err(m) => return err(L, &m),
    };
    let mut hint = Hint::look(&text);
    hint.ask = Hint::ask_in(&text);
    if let Some(a) = str_field(L, 1, c"ask") {
        match ask_named(&a) {
            Some(a) => hint.ask = a,
            None => return err(L, &format!("ui.hint: ask is \"tap\", \"hold\", \"let go\" or \"look\", got \"{a}\"")),
        }
    }
    match str_field(L, 1, c"priority").as_deref() {
        None | Some("teach") => {}
        Some("threat") => hint = hint.threat(),
        Some("ambient") => hint = hint.ambient(),
        Some(p) => return err(L, &format!("ui.hint: priority is \"ambient\", \"teach\" or \"threat\", got \"{p}\"")),
    }
    if let Some(l) = num_field(L, 1, c"linger") {
        hint = hint.linger(l as f32);
    }
    // The same words again: that hint, moved and changed.
    let id = match h.ui.entries.iter_mut().find(|e| e.hint.text == text) {
        Some(e) => {
            (e.hint, e.at) = (hint, at);
            e.id
        }
        None => {
            h.ui.next_hint += 1;
            let id = h.ui.next_hint;
            h.ui.entries.push(HintEntry { id, hint, at });
            id
        }
    };
    push_handle(L, tag_ui::HINT, id)
});

b!(hint_dismiss, |L, h| {
    let Some(id) = handle(L, 1, tag_ui::HINT) else { return bad(L, 1, "dismiss", "Hint") };
    h.ui.entries.retain(|e| e.id != id);
    0
});

b!(hint_showing, |L, h| {
    let Some(id) = handle(L, 1, tag_ui::HINT) else { return bad(L, 1, "showing", "Hint") };
    let text = h.ui.entries.iter().find(|e| e.id == id).map(|e| e.hint.text.clone());
    retb(L, text.is_some_and(|t| h.ui.hints.showing().any(|s| s.text == t)))
});

// `ui.lesson{ prompt, slow = 0.3, hold = 2.5, at, learned = fn }` → a Lesson, taught now (once a round).
b!(lesson, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "ui.lesson", "table");
    }
    let Some(prompt) = str_field(L, 1, c"prompt") else { return err(L, "ui.lesson: prompt is the move in 5 words at most, a verb first (\"HOLD to charge\")") };
    if let Some(i) = h.ui.lessons.iter().position(|l| l.lesson.prompt == prompt) {
        return push_handle(L, tag_ui::LESSON, i as u32);
    }
    let at = match at_field(L, 1, "ui.lesson") {
        Ok(a) => a,
        Err(m) => return err(L, &m),
    };
    let rate = num_field(L, 1, c"slow").unwrap_or(0.3);
    if !(0.05..=1.0).contains(&rate) {
        return err(L, &format!("ui.lesson: slow is how fast the world runs while it teaches, 0.05–1 (1: no slow motion), got {}", fmt_num(rate)));
    }
    let hold = num_field(L, 1, c"hold").unwrap_or(2.5).clamp(0.0, maimbrain::ui::SlowMo::MAX_HOLD as f64);
    let learned_ref = match fn_field(L, 1, c"learned", "ui.lesson") {
        Ok(r) => r,
        Err(m) => return err(L, &m),
    };
    let mut lesson = Lesson::new(&prompt).slow(rate as f32, hold as f32);
    lesson.teach();
    h.ui.lessons.push(LessonEntry { lesson, at, learned_ref });
    push_handle(L, tag_ui::LESSON, (h.ui.lessons.len() - 1) as u32)
});

b!(lesson_learned, |L, h| {
    let Some(i) = handle(L, 1, tag_ui::LESSON) else { return bad(L, 1, "learned", "Lesson") };
    if let Some(l) = h.ui.lessons.get_mut(i as usize) {
        l.lesson.learned();
    }
    0
});

b!(lesson_showing, |L, h| {
    let Some(i) = handle(L, 1, tag_ui::LESSON) else { return bad(L, 1, "showing", "Lesson") };
    retb(L, h.ui.lessons.get(i as usize).is_some_and(|l| l.lesson.showing() && !l.lesson.is_learned()))
});

/// Where `at` is on the canvas now, or `None` once it's gone.
fn place(h: &mut Host, at: At) -> Option<Option<(f32, f32)>> {
    let (cw, ch) = (h.project.size.0 as f32, h.project.size.1 as f32);
    match at {
        At::Free => Some(None),
        At::Point(x, y) => Some(Some((x, y))),
        At::Node(id) => {
            if !h.scene.alive(id) {
                return None;
            }
            let (x, y) = h.scene.world_position(id);
            if h.scene.is_fixed(id) { Some(Some((x, y))) } else { Some(Some(h.scene.camera.to_screen(cw, ch, x, y))) }
        }
        #[cfg(three)]
        At::Node3(bits) => {
            let [sw, sh, ox, oy, s] = h.screen;
            let w = h.three.as_deref_mut()?;
            let id = maimbrain::scene::three::Node3Id::from_bits(bits);
            let m = w.world_of(id)?;
            let p = w.camera().project(maimbrain::gfx3d::vec3(m.0[12], m.0[13], m.0[14]), sw, sh);
            Some(Some(((p.x - ox) / s, (p.y - oy) / s)))
        }
        #[cfg(not(three))]
        At::Node3(_) => None,
    }
}

// `__engine.ui()`, after love.update: lessons checked and moved on, hints
// placed and chosen, the countdown and combos moved on (real time for what
// teaches, the world's for combos).
b!(ui_frame, |L, h| {
    let wall = h.dt_wall;
    // Each lesson's `learned` check.
    for i in 0..h.ui.lessons.len() {
        let l = &h.ui.lessons[i];
        if l.learned_ref == 0 || l.lesson.is_learned() {
            continue;
        }
        lua_rawgeti(L, LUA_REGISTRYINDEX, l.learned_ref);
        if lua_pcall(L, 0, 1, 0) != LUA_OK {
            return -2;
        }
        let yes = lua_toboolean(L, -1) != 0;
        lua_pop(L, 1);
        if yes {
            h.ui.lessons[i].lesson.learned();
        }
    }
    for l in &mut h.ui.lessons {
        l.lesson.update(wall);
    }
    // Hints: the game's, then the lessons' prompts (teaching), placed.
    let entries = std::mem::take(&mut h.ui.entries);
    let mut kept = Vec::with_capacity(entries.len());
    for mut e in entries {
        match place(h, e.at) {
            None => continue, // its `at` went away: so does the hint
            Some(p) => {
                e.hint.at = p;
                h.ui.hints.want(e.hint.clone());
                kept.push(e);
            }
        }
    }
    h.ui.entries = kept;
    for i in 0..h.ui.lessons.len() {
        let l = &h.ui.lessons[i];
        if !l.lesson.showing() || l.lesson.is_learned() {
            continue;
        }
        let (prompt, at) = (l.lesson.prompt.clone(), l.at);
        let mut hint = Hint::look(&prompt);
        hint.ask = Hint::ask_in(&prompt);
        if let Some(Some(p)) = place(h, at) {
            hint.at = Some(p);
        }
        h.ui.hints.want(hint);
    }
    h.ui.hints.update(wall);
    // The countdown: its function at GO.
    let mut go = None;
    if let Some((c, f)) = &mut h.ui.countdown {
        if c.update(wall) == Some(0) && *f != 0 {
            go = Some(*f);
        }
        if !c.showing() {
            h.ui.countdown = None;
        }
    }
    let world = h.dt;
    for c in &mut h.ui.combos {
        c.update(world);
    }
    if let Some(f) = go {
        lua_rawgeti(L, LUA_REGISTRYINDEX, f);
        if lua_pcall(L, 0, 0, 0) != LUA_OK {
            return -2;
        }
    }
    0
});

// `ui.countdown(n, fn)`: "3, 2, 1, GO!", then `fn()`.
b!(countdown, |L, h| {
    let n = match num(L, 1) {
        Some(n) if (1.0..=10.0).contains(&n) => n as u32,
        Some(n) => return err(L, &format!("ui.countdown counts down from 1 to 10, got {}", fmt_num(n))),
        None => return bad(L, 1, "ui.countdown", "number"),
    };
    let f = match lua_type(L, 2) {
        LUA_TFUNCTION => {
            lua_pushvalue(L, 2);
            let r = lua_ref(L, -1);
            lua_pop(L, 1);
            r
        }
        LUA_TNIL | -1 => 0,
        _ => return bad(L, 2, "ui.countdown", "function"),
    };
    let mut c = Countdown::default();
    c.start(n);
    h.ui.countdown = Some((c, f));
    0
});

// `ui.stats{ title, rows = { { "Coins", 12 }, { "Accuracy", 0.87, "percent" }, … }, rank, next, wipe, tap, done }`.
b!(stats, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "ui.stats", "table");
    }
    let title = str_field(L, 1, c"title").unwrap_or_else(|| "CLEAR".into());
    let mut rows = Vec::new();
    lua_rawgetfield(L, 1, c"rows".as_ptr());
    if lua_type(L, -1) == LUA_TTABLE {
        let n = lua_objlen(L, -1);
        if n > 6 {
            lua_pop(L, 1);
            return err(L, &format!("ui.stats: 6 rows fit on the card, got {n}"));
        }
        for i in 1..=n {
            lua_rawgeti(L, -1, i);
            if lua_type(L, -1) != LUA_TTABLE {
                lua_pop(L, 2);
                return err(L, &format!("ui.stats: rows[{i}] is {{ label, value }} or {{ label, value, \"percent\" | \"time\" }}"));
            }
            lua_rawgeti(L, -1, 1);
            lua_rawgeti(L, -2, 2);
            lua_rawgeti(L, -3, 3);
            let label = to_s(L, -3);
            let value = num(L, -2);
            let format = st(L, -1).map(str::to_string);
            lua_pop(L, 3);
            lua_rawgetfield(L, -1, c"bad".as_ptr());
            let bad_row = lua_toboolean(L, -1) != 0;
            lua_pop(L, 2);
            let Some(value) = value else {
                lua_pop(L, 1);
                return err(L, &format!("ui.stats: rows[{i}]'s value isn't a number"));
            };
            let format = match format.as_deref() {
                None | Some("int") => StatFormat::Int,
                Some("percent") => StatFormat::Percent,
                Some("time") => StatFormat::Time,
                Some(f) => {
                    lua_pop(L, 1);
                    return err(L, &format!("ui.stats: rows[{i}]'s format is \"int\", \"percent\" (0–1) or \"time\" (seconds), got \"{f}\""));
                }
            };
            rows.push((label, value as f32, format, bad_row));
        }
    }
    lua_pop(L, 1);
    lua_rawgetfield(L, 1, c"rank".as_ptr());
    let rank = match lua_type(L, -1) {
        LUA_TSTRING => Some(to_s(L, -1)),
        LUA_TNUMBER => Some(StatsCard::rank_for(lua_tonumber(L, -1) as f32).to_string()),
        LUA_TBOOLEAN if lua_toboolean(L, -1) != 0 => {
            // From the rows given as percents.
            let p: Vec<f32> = rows.iter().filter(|r| r.2 == StatFormat::Percent).map(|r| r.1).collect();
            (!p.is_empty()).then(|| StatsCard::rank_for(p.iter().sum::<f32>() / p.len() as f32).to_string())
        }
        _ => None,
    };
    lua_pop(L, 1);
    lua_rawgetfield(L, 1, c"next".as_ptr());
    let next = if lua_type(L, -1) == LUA_TTABLE {
        lua_rawgeti(L, -1, 1);
        lua_rawgeti(L, -2, 2);
        let n = (to_s(L, -2), if lua_type(L, -1) == LUA_TSTRING { to_s(L, -1) } else { String::new() });
        lua_pop(L, 2);
        Some(n)
    } else if lua_type(L, -1) == LUA_TSTRING {
        Some((to_s(L, -1), String::new()))
    } else {
        None
    };
    lua_pop(L, 1);
    let wipe = match str_field(L, 1, c"wipe").as_deref() {
        None | Some("iris") => Wipe::Iris,
        Some("slide") => Wipe::Slide,
        Some("shutter") => Wipe::Shutter,
        Some(w) => return err(L, &format!("ui.stats: wipe is \"iris\", \"slide\" or \"shutter\", got \"{w}\"")),
    };
    lua_rawgetfield(L, 1, c"tap".as_ptr());
    let manual = lua_toboolean(L, -1) != 0;
    lua_pop(L, 1);
    let done_ref = match fn_field(L, 1, c"done", "ui.stats") {
        Ok(r) => r,
        Err(m) => return err(L, &m),
    };
    if let Some(old) = h.ui.stats.take()
        && old.done_ref != 0
    {
        lua_unref(L, old.done_ref);
    }
    h.ui.stats = Some(Stats { title, rows, rank, next, wipe, manual, done_ref, card: None, covered: false });
    0
});

// `ui.combo{ window = 2, hud = true }` → a Combo: `:hit()`, `:reset()`, `.count`, `.multiplier`, `.best`.
b!(combo, |L, h| {
    let window = if lua_type(L, 1) == LUA_TTABLE { num_field(L, 1, c"window").unwrap_or(2.0) } else { 2.0 };
    if !(window > 0.0 && window <= 60.0) {
        return err(L, &format!("ui.combo: window is the seconds a combo lasts without a hit (0–60), got {}", fmt_num(window)));
    }
    let on_hud = if lua_type(L, 1) == LUA_TTABLE {
        lua_rawgetfield(L, 1, c"hud".as_ptr());
        let v = lua_type(L, -1) != LUA_TBOOLEAN || lua_toboolean(L, -1) != 0;
        lua_pop(L, 1);
        v
    } else {
        true
    };
    if h.ui.combos.len() >= 16 {
        return err(L, "ui.combo: 16 combos at most (keep one and reset it)");
    }
    h.ui.combos.push(maimbrain::juice::Combo::new(window as f32));
    let i = h.ui.combos.len() - 1;
    if on_hud {
        h.ui.hud_combo = Some(i);
    }
    push_handle(L, tag_ui::COMBO, i as u32)
});

b!(combo_index, |L, h| {
    let Some(i) = handle(L, 1, tag_ui::COMBO) else { return err(L, "not a combo") };
    let Some(c) = h.ui.combos.get(i as usize) else { return err(L, "not a combo") };
    match st(L, 2).unwrap_or("") {
        "count" => ret(L, c.count as f64),
        "multiplier" => ret(L, c.multiplier() as f64),
        "best" => ret(L, c.best as f64),
        k if k == "hit" || k == "reset" => err(L, "call it with a colon: combo:hit(), combo:reset()"),
        k => err(L, &format!("a combo has :hit(), :reset(), .count, .multiplier and .best, not `{k}`")),
    }
});

// `combo:hit()` → the new count; `combo:reset()` ends it (a miss).
b!(combo_call, |L, h| {
    let Some(i) = handle(L, 1, tag_ui::COMBO) else { return err(L, "not a combo") };
    let mut atom = -1;
    let name = std::ffi::CStr::from_ptr(lua_namecallatom(L, &mut atom)).to_str().unwrap_or("");
    let Some(c) = h.ui.combos.get_mut(i as usize) else { return 0 };
    match name {
        "hit" => ret(L, c.hit() as f64),
        "reset" => {
            c.break_combo();
            0
        }
        k => err(L, &format!("a combo has :hit() and :reset(), not :{k}()")),
    }
});

/// The priority's name, for the bot's `act.hints()`.
pub fn priority_name(p: Priority) -> &'static str {
    match p {
        Priority::Ambient => "ambient",
        Priority::Teach => "teach",
        Priority::Threat => "threat",
    }
}

pub fn ask_name(a: Ask) -> &'static str {
    match a {
        Ask::Tap => "tap",
        Ask::Hold => "hold",
        Ask::LetGo => "let go",
        Ask::Look => "look",
    }
}
