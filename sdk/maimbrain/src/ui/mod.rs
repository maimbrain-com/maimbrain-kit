//! A UI kit on top of mb2d: themes, anti-aliased shapes, styled text,
//! icons, buttons and meters, layout helpers for the safe area and the
//! feed's card overlays, ready-made title, results, between-rounds stats
//! and HUD screens, and
//! voice-acted dialogue with talking portraits (ui/dialogue.rs).
//! See docs/UI.md for a tour and recipes.
//!
//! ```ignore
//! use maimbrain::ui::{self, Layout, Theme, TitleCard};
//! let layout = Layout::new();
//! let mut title = TitleCard::new("BUBBLE", Theme::candy()).tagline("don't let it pop").best(best);
//! // update: title.update(dt);   render: title.draw(&layout);
//! ```
//!
//! Draw calls follow mb2d's rules (transforms apply; nothing is retained),
//! and widget state (button presses, counters, entrances) advances only in
//! `update(dt)`, so the kit replays exactly (SPEC §3).

pub mod barks;
pub mod color;
pub mod damage;
pub mod dialogue;
pub mod icon;
pub mod layout;
pub mod layouts;
pub mod screens;
pub mod sequence;
pub mod identity;
pub mod shape;
pub mod stats;
pub mod style;
pub mod text;
pub mod theme;
pub mod widgets;

pub use color::{contrast, darken, fade, hex, hsv, lighten, mix, over, readable, rgba, with_alpha};
pub use barks::{BarkLine, Barks, ScriptLine, parse_script};
pub use damage::Reaction;
pub use dialogue::{Advance, Dialogue, DialogueEvent, Ducker, LIPS_PACK_HEADER, LineBox, Lips, Playing, Portrait, Showing, VoiceLine, parse_lips_pack, subtitle_text};
pub use icon::Icon;
pub use layout::{Anchor, Column, Layout, Rect, stack_h, stack_v};
pub use sequence::{SeqEvent, Sequence, Shot};
pub use screens::{Countdown, Hud, ResultsAction, ResultsCard, ResultsEvent, TitleCard};
pub use stats::{CardSounds, StatFormat, StatsCard, StatsEvent, Wipe};
pub use text::{Align, VAlign, fmt_int, text};
pub use style::{Backdrop, Border, Case, HudLayout, Motion, ResultsLayout, Shadow, Shape, Style, Texture, TitleLayout};
pub use theme::{FontNames, Theme};
pub use widgets::{Button, ButtonKind, Tap, panel, pill, pill_width, progress_bar, ribbon, ring_meter, segments};
