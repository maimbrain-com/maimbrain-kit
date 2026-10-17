//! The platform's debug menu (SPEC §5.1 `mb_debug_menu`, §5.2 kind 15).
//!
//! A test build (a draft or a build waiting for review, never a live
//! version) can be played in *debug mode*: the creator, a moderator, or a
//! tester the creator allowed opens a menu from the game's card and jumps
//! to any level, sets the state, turns on cheats or fires a rare event. The
//! platform draws the menu; the game only declares its items and applies
//! the picks. Nothing scored in debug mode counts anywhere, and a debug
//! session plays on a copy of the player's save, so a game needs no special care for
//! either.
//!
//! Declare the menu in `init` and again whenever a value it shows changes
//! ([`show`] only sends it when it changed), and handle picks in `update`:
//!
//! ```
//! use maimbrain::debug::{self, Menu, Pick};
//! use maimbrain::input;
//!
//! const FLOOR_3: u32 = 1;
//! const GOLD: u32 = 2;
//! const GOD: u32 = 3;
//!
//! struct Game { floor: u32, gold: u32, god: bool }
//!
//! impl Game {
//!     fn menu(&self) -> Menu {
//!         let mut m = Menu::new();
//!         m.section("Jump");
//!         m.action(FLOOR_3, "Floor 3 · Glass Halls");
//!         m.section("State");
//!         m.number(GOLD, "Gold", self.gold as f32).range(0.0, 9999.0).step(10.0);
//!         m.section("Cheats");
//!         m.toggle(GOD, "Invincible", self.god);
//!         m
//!     }
//!
//!     fn update(&mut self) {
//!         for e in input::poll() {
//!             if let Some(p) = Pick::from(&e) {
//!                 match p.id {
//!                     FLOOR_3 => self.floor = 3,
//!                     GOLD => self.gold = p.value as u32,
//!                     GOD => self.god = p.on(),
//!                     _ => {}
//!                 }
//!             }
//!         }
//!         debug::show(&self.menu());
//!     }
//! }
//! # let mut g = Game { floor: 1, gold: 0, god: false };
//! # g.update();
//! ```
//!
//! Outside debug mode the host ignores the menu and no pick ever arrives, so
//! the same build plays exactly the same either way. Don't branch on
//! anything else: a game never learns whether it's in debug mode.
//!
//! A pick is an input event, recorded in the replay like a touch, so a run
//! that used the menu replays exactly. Treat a value pick as the new state
//! ("gold is now 150"), not as a press.
//!
//! The platform adds its own section to every menu (game speed, step
//! frames, restart, leave debug mode); don't declare those.

use crate::input::{Event, Kind};

/// Most sections a menu shows.
pub const MAX_SECTIONS: usize = 12;
/// Most items in all.
pub const MAX_ITEMS: usize = 200;
/// Most options a choice has.
pub const MAX_OPTIONS: usize = 24;
/// Longest item label (characters).
pub const MAX_LABEL: usize = 48;
/// Longest section title (characters).
pub const MAX_TITLE: usize = 24;
/// Longest hint (characters).
pub const MAX_HINT: usize = 64;

/// What an item is.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A jump or a one-shot trigger. Picking it closes the menu and goes
    /// back to play, unless `stay`; with `confirm` the player is asked first.
    Action { stay: bool, confirm: bool },
    /// On or off.
    Toggle(bool),
    /// A number, changed in steps of `step` between `min` and `max`.
    Number { value: f32, min: f32, max: f32, step: f32 },
    /// One of `options`.
    Choice { index: usize, options: Vec<String> },
}

/// One row of the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The game's own id for it (1 ..= 2³¹), unique in the menu.
    pub id: u32,
    pub label: String,
    /// A short line shown under the label.
    pub hint: Option<String>,
    pub item: Item,
}

/// A titled group of rows (a tab in the menu).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub entries: Vec<Entry>,
}

/// The game's debug menu. Build it with [`Menu::section`] and the item
/// methods, then [`show`] it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub sections: Vec<Section>,
}

/// The row just added, to set its options: `m.number(..).range(0.0, 9.0)`.
pub struct Row<'a>(&'a mut Entry);

impl Row<'_> {
    /// An action that leaves the menu open (a trigger you'd fire several times).
    pub fn stay(self) -> Self {
        if let Item::Action { stay, .. } = &mut self.0.item {
            *stay = true;
        }
        self
    }

    /// An action the player confirms first (anything that throws state away).
    pub fn confirm(self) -> Self {
        if let Item::Action { confirm, .. } = &mut self.0.item {
            *confirm = true;
        }
        self
    }

    /// A number's bounds (default 0 ..= 100).
    pub fn range(self, min: f32, max: f32) -> Self {
        if let Item::Number { min: lo, max: hi, .. } = &mut self.0.item {
            *lo = min;
            *hi = max;
        }
        self
    }

    /// A number's step (default 1).
    pub fn step(self, step: f32) -> Self {
        if let Item::Number { step: s, .. } = &mut self.0.item {
            *s = step;
        }
        self
    }

    /// A short line under the label.
    pub fn hint(self, hint: &str) -> Self {
        self.0.hint = Some(hint.to_string());
        self
    }
}

impl Menu {
    pub fn new() -> Menu {
        Menu::default()
    }

    /// Starts a section; the items after it go in it. Items added before any
    /// section go in one called "Game".
    pub fn section(&mut self, title: &str) -> &mut Self {
        self.sections.push(Section { title: title.to_string(), entries: Vec::new() });
        self
    }

    fn push(&mut self, id: u32, label: &str, item: Item) -> Row<'_> {
        if self.sections.is_empty() {
            self.section("Game");
        }
        let entries = &mut self.sections.last_mut().expect("a section").entries;
        entries.push(Entry { id, label: label.to_string(), hint: None, item });
        Row(entries.last_mut().expect("just pushed"))
    }

    /// A jump or one-shot trigger. Its pick's value is 0.
    pub fn action(&mut self, id: u32, label: &str) -> Row<'_> {
        self.push(id, label, Item::Action { stay: false, confirm: false })
    }

    /// An on/off switch showing `on`. Its pick's value is 1 or 0 ([`Pick::on`]).
    pub fn toggle(&mut self, id: u32, label: &str, on: bool) -> Row<'_> {
        self.push(id, label, Item::Toggle(on))
    }

    /// A number showing `value`. Its pick's value is the new number.
    pub fn number(&mut self, id: u32, label: &str, value: f32) -> Row<'_> {
        self.push(id, label, Item::Number { value, min: 0.0, max: 100.0, step: 1.0 })
    }

    /// One of `options`, showing `options[index]`. Its pick's value is the
    /// new index ([`Pick::index`]).
    pub fn choice(&mut self, id: u32, label: &str, index: usize, options: &[&str]) -> Row<'_> {
        self.push(id, label, Item::Choice { index, options: options.iter().map(|o| o.to_string()).collect() })
    }

    /// Every row, in order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.sections.iter().flat_map(|s| s.entries.iter())
    }

    /// The row with this id.
    pub fn find(&self, id: u32) -> Option<&Entry> {
        self.entries().find(|e| e.id == id)
    }

    /// The row with this label (tests: `menu.find_label("Floor 3")`).
    pub fn find_label(&self, label: &str) -> Option<&Entry> {
        self.entries().find(|e| e.label == label)
    }

    /// What's wrong with it, if anything: ids out of range or repeated,
    /// labels too long, too many rows. The host rejects a menu with problems
    /// (and keeps the last good one); `show` logs them.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.sections.len() > MAX_SECTIONS {
            out.push(format!("{} sections (at most {MAX_SECTIONS})", self.sections.len()));
        }
        let n = self.entries().count();
        if n > MAX_ITEMS {
            out.push(format!("{n} items (at most {MAX_ITEMS})"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for s in &self.sections {
            if s.title.chars().count() > MAX_TITLE {
                out.push(format!("section \"{}\": title over {MAX_TITLE} characters", s.title));
            }
            for e in &s.entries {
                if e.id == 0 || e.id > 1 << 31 {
                    out.push(format!("\"{}\": id {} (use 1 ..= 2^31)", e.label, e.id));
                }
                if !seen.insert(e.id) {
                    out.push(format!("\"{}\": id {} used twice", e.label, e.id));
                }
                if e.label.chars().count() > MAX_LABEL {
                    out.push(format!("\"{}\": label over {MAX_LABEL} characters", e.label));
                }
                if e.hint.as_ref().is_some_and(|h| h.chars().count() > MAX_HINT) {
                    out.push(format!("\"{}\": hint over {MAX_HINT} characters", e.label));
                }
                match &e.item {
                    Item::Choice { index, options } => {
                        if options.is_empty() || options.len() > MAX_OPTIONS {
                            out.push(format!("\"{}\": {} options (1 ..= {MAX_OPTIONS})", e.label, options.len()));
                        } else if *index >= options.len() {
                            out.push(format!("\"{}\": index {index} past its {} options", e.label, options.len()));
                        }
                    }
                    Item::Number { min, max, step, value } => {
                        let ordered = min.is_finite() && max.is_finite() && min <= max;
                        if !ordered || step.is_nan() || *step <= 0.0 || !value.is_finite() {
                            out.push(format!("\"{}\": needs min ≤ max, step > 0 and a finite value", e.label));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// The JSON the host reads (SPEC §5.1 `mb_debug_menu`).
    pub fn to_json(&self) -> String {
        let mut j = String::from("{\"sections\":[");
        for (si, s) in self.sections.iter().enumerate() {
            if si > 0 {
                j.push(',');
            }
            j.push_str("{\"title\":");
            quote(&mut j, &s.title);
            j.push_str(",\"items\":[");
            for (ei, e) in s.entries.iter().enumerate() {
                if ei > 0 {
                    j.push(',');
                }
                j.push_str(&format!("{{\"id\":{},\"label\":", e.id));
                quote(&mut j, &e.label);
                if let Some(h) = &e.hint {
                    j.push_str(",\"hint\":");
                    quote(&mut j, h);
                }
                match &e.item {
                    Item::Action { stay, confirm } => {
                        j.push_str(",\"kind\":\"action\"");
                        if *stay {
                            j.push_str(",\"stay\":true");
                        }
                        if *confirm {
                            j.push_str(",\"confirm\":true");
                        }
                    }
                    Item::Toggle(on) => j.push_str(&format!(",\"kind\":\"toggle\",\"value\":{}", *on as u8)),
                    Item::Number { value, min, max, step } => {
                        j.push_str(&format!(",\"kind\":\"number\",\"value\":{},\"min\":{},\"max\":{},\"step\":{}", num(*value), num(*min), num(*max), num(*step)));
                    }
                    Item::Choice { index, options } => {
                        j.push_str(&format!(",\"kind\":\"choice\",\"value\":{index},\"options\":["));
                        for (oi, o) in options.iter().enumerate() {
                            if oi > 0 {
                                j.push(',');
                            }
                            quote(&mut j, o);
                        }
                        j.push(']');
                    }
                }
                j.push('}');
            }
            j.push_str("]}");
        }
        j.push_str("]}");
        j
    }
}

fn num(v: f32) -> String {
    if v.is_finite() { format!("{v}") } else { "0".to_string() }
}

fn quote(j: &mut String, s: &str) {
    j.push('"');
    for c in s.chars() {
        match c {
            '"' => j.push_str("\\\""),
            '\\' => j.push_str("\\\\"),
            c if (c as u32) < 0x20 => j.push_str(&format!("\\u{:04x}", c as u32)),
            c => j.push(c),
        }
    }
    j.push('"');
}

thread_local! {
    /// The JSON last sent, so an unchanged menu isn't sent again.
    static SENT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Declares the menu (replacing the last one). Cheap to call every frame:
/// it's only sent when it changed. Problems ([`Menu::problems`]) are logged
/// as a warning and the menu isn't sent.
pub fn show(menu: &Menu) {
    let json = menu.to_json();
    if SENT.with(|s| *s.borrow() == json) {
        return;
    }
    let problems = menu.problems();
    if !problems.is_empty() {
        crate::sys::log(crate::sys::Level::Warn, &format!("debug menu not shown: {}", problems.join("; ")));
        SENT.with(|s| *s.borrow_mut() = json);
        return;
    }
    unsafe { crate::ffi::mb_debug_menu(json.as_ptr(), json.len() as u32) };
    #[cfg(not(target_arch = "wasm32"))]
    crate::sys::test::host::debug_menu(menu.clone());
    SENT.with(|s| *s.borrow_mut() = json);
}

/// Forget what was sent (a new test host starts with no menu).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn reset() {
    SENT.with(|s| s.borrow_mut().clear());
}

/// A pick from the menu (an input event of [`Kind::Debug`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    /// The item's id.
    pub id: u32,
    /// Its new value: 1 or 0 for a toggle, the number, a choice's index; 0
    /// for an action.
    pub value: f32,
}

impl Pick {
    /// The pick this event carries, if it's one.
    pub fn from(e: &Event) -> Option<Pick> {
        (e.kind() == Kind::Debug).then_some(Pick { id: e.code, value: e.a })
    }

    /// A toggle's new state.
    pub fn on(&self) -> bool {
        self.value >= 0.5
    }

    /// A choice's new index.
    pub fn index(&self) -> usize {
        self.value.max(0.0) as usize
    }

    /// The event the host sends for it (tests: `host.push([Pick { .. }.event(t)])`).
    pub fn event(&self, time: f64) -> Event {
        Event { kind: Kind::Debug as u8, code: self.id, a: self.value, time, ..Event::default() }
    }
}

impl Event {
    /// The debug-menu pick this event carries, if it's one ([`Pick::from`]).
    pub fn debug_pick(&self) -> Option<Pick> {
        Pick::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_has_every_kind() {
        let mut m = Menu::new();
        m.section("Jump");
        m.action(1, "Floor \"3\"").hint("the halls");
        m.action(2, "Wipe").confirm().stay();
        m.section("State");
        m.number(3, "Gold", 140.0).range(0.0, 9999.0).step(10.0);
        m.choice(4, "Gear", 2, &["None", "Sword", "Armor"]);
        m.toggle(5, "Invincible", true);
        assert!(m.problems().is_empty());
        assert_eq!(
            m.to_json(),
            r#"{"sections":[{"title":"Jump","items":[{"id":1,"label":"Floor \"3\"","hint":"the halls","kind":"action"},{"id":2,"label":"Wipe","kind":"action","stay":true,"confirm":true}]},{"title":"State","items":[{"id":3,"label":"Gold","kind":"number","value":140,"min":0,"max":9999,"step":10},{"id":4,"label":"Gear","kind":"choice","value":2,"options":["None","Sword","Armor"]},{"id":5,"label":"Invincible","kind":"toggle","value":1}]}]}"#
        );
    }

    #[test]
    fn problems_are_found() {
        let mut m = Menu::new();
        m.action(0, "Zero");
        m.action(7, "A");
        m.action(7, "B");
        m.choice(8, "Empty", 0, &[]);
        let p = m.problems();
        assert_eq!(p.len(), 3, "{p:?}");
        assert_eq!(m.sections[0].title, "Game");
    }

    #[test]
    fn picks_read_back() {
        let e = Pick { id: 9, value: 1.0 }.event(0.5);
        assert_eq!(e.kind(), Kind::Debug);
        assert!(!e.is_press() && !e.is_confirm());
        let p = e.debug_pick().unwrap();
        assert!(p.on());
        assert_eq!(p.index(), 1);
        assert_eq!(Pick::from(&Event::default()), None);
    }

    #[test]
    fn the_test_host_sees_the_menu_and_delivers_picks() {
        use crate::sys::test::Host;
        struct G {
            gold: f32,
        }
        impl crate::Game for G {
            fn init() -> Self {
                let g = G { gold: 5.0 };
                g.declare();
                g
            }
            fn update(&mut self, _dt: f32) {
                for e in crate::input::poll() {
                    if let Some(p) = Pick::from(&e) {
                        self.gold = p.value;
                    }
                }
                self.declare();
            }
            fn render(&self) {}
        }
        impl G {
            fn declare(&self) {
                let mut m = Menu::new();
                m.number(1, "Gold", self.gold);
                show(&m);
            }
        }
        let host = Host::start(1);
        let mut g: G = host.init();
        assert_eq!(host.debug_menu().unwrap().find(1).unwrap().item, Item::Number { value: 5.0, min: 0.0, max: 100.0, step: 1.0 });
        host.debug_pick(1, 40.0);
        host.frame(&mut g, 1.0 / 60.0);
        assert_eq!(g.gold, 40.0);
        assert!(matches!(host.debug_menu().unwrap().find_label("Gold").unwrap().item, Item::Number { value: 40.0, .. }));
    }
}
