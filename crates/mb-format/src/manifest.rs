//! `manifest.toml` schema (SPEC §2).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Host ABI major version this crate validates against.
pub const ABI: u32 = 0;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub abi: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub creator: String,
    pub orientation: Orientation,
    /// In the game's natural orientation: `[640, 360]` for a landscape game;
    /// an `any` game gives its portrait size, swapped when it's turned.
    pub logical_size: [u32; 2],
    /// How the logical size meets the screen (SPEC §2).
    #[serde(default)]
    pub fit: Fit,
    /// Every input the game can be played with.
    pub inputs: Vec<Input>,
    /// Inputs the game can't be played without (a subset of `inputs`). The
    /// feed only offers the game where one is connected.
    #[serde(default)]
    pub requires: Vec<Input>,
    pub perf_tier: PerfTier,
    /// Host engines and their versions, e.g. `{ mb2d = 1 }`.
    #[serde(default)]
    pub stdlib: BTreeMap<String, u32>,
    #[serde(default)]
    pub sensors: Vec<Sensor>,
    /// Sensors the game can't be played without (a subset of `sensors`). The
    /// feed only offers the game on devices that have them.
    #[serde(default)]
    pub needs: Vec<Sensor>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub startup_assets: Vec<String>,
    /// Leaderboards the game submits to (SPEC §5.8).
    #[serde(default)]
    pub scores: Vec<ScoreBoard>,
    #[serde(default)]
    pub remix: Remix,
    /// Where a replay card puts its "Beat …" banner (SPEC §5.2).
    #[serde(default)]
    pub replay_banner: ReplayBanner,
    /// A Studio game: the engine its `game.wasm` is, `"studio@1.0"` (SPEC
    /// "Studio bundles"). An app whose validator predates the field refuses
    /// the manifest, so an older app never runs a Studio bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

/// Where the feed draws a replay card's "Beat …" banner (SPEC §5.2).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplayBanner {
    /// Over the bottom ~230 pt, with the title (the default).
    #[default]
    Bottom,
    /// Just below the top inset, ~110 pt: for games whose player sits low on
    /// the screen (a runner or racer seen from behind), so replays show it.
    Top,
}

/// A platform-rendered leaderboard. Games can't create boards at runtime.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScoreBoard {
    /// The `board` argument to `mb_score_submit` / `mb_score_show`, 0–15.
    pub board: u32,
    pub label: String,
    pub order: ScoreOrder,
    #[serde(default)]
    pub format: ScoreFormat,
    /// A daily board: played on `mb_daily_seed`, its standings starting over
    /// each UTC day (SPEC §5.8).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub daily: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScoreOrder {
    /// Bigger values rank first.
    Higher,
    /// Smaller values rank first (e.g. times).
    Lower,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScoreFormat {
    #[default]
    Integer,
    /// Shown as m:ss.mmm.
    Milliseconds,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Portrait,
    Landscape,
    /// Either way up: the game follows how the phone is held, relaying out
    /// when it turns (SPEC §5.2 `Resize`).
    Any,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
            Orientation::Any => "any",
        }
    }
}

/// How the logical size meets a screen of another shape (SPEC §2).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Exactly `logical_size`, scaled to fit, with bars on the other sides:
    /// every player sees the same playfield.
    #[default]
    Letterbox,
    /// `logical_size` is the least the game gets: the screen grows along
    /// whichever side has room, to the device's own shape, with no bars.
    Fill,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Input {
    Touch,
    Mouse,
    Keyboard,
    Gamepad,
}

impl Input {
    pub fn as_str(self) -> &'static str {
        match self {
            Input::Touch => "touch",
            Input::Mouse => "mouse",
            Input::Keyboard => "keyboard",
            Input::Gamepad => "gamepad",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PerfTier {
    Lite,
    Full,
}

impl PerfTier {
    /// Maximum declared linear memory, in 64 KiB pages (SPEC §3).
    pub fn max_memory_pages(self) -> u64 {
        match self {
            PerfTier::Lite => 2048, // 128 MiB
            PerfTier::Full => 4096, // 256 MiB
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Sensor {
    Tilt,
    Motion,
    /// The phone's orientation (gyro-fused) and rotation rate: aiming by pointing it.
    Attitude,
    Loudness,
    Light,
    Haptics,
}

impl Sensor {
    pub fn as_str(self) -> &'static str {
        match self {
            Sensor::Tilt => "tilt",
            Sensor::Motion => "motion",
            Sensor::Attitude => "attitude",
            Sensor::Loudness => "loudness",
            Sensor::Light => "light",
            Sensor::Haptics => "haptics",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Store,
    Score,
    TextInput,
    Links,
    Net,
    /// Saved data under `progress/` is the player's progress, which the
    /// platform verifies by replaying each run and keeps with their account
    /// (SPEC §5.8). Needs `store`.
    Progress,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Store => "store",
            Capability::Score => "score",
            Capability::TextInput => "text_input",
            Capability::Links => "links",
            Capability::Net => "net",
            Capability::Progress => "progress",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Remix {
    #[serde(default = "yes")]
    pub allowed: bool,
    /// `id@version` of the game this remixes; empty if original.
    #[serde(default)]
    pub parent: String,
}

impl Default for Remix {
    fn default() -> Self {
        Remix { allowed: true, parent: String::new() }
    }
}

fn yes() -> bool {
    true
}

/// Host engines a manifest may declare, with the newest version this host
/// provides. A manifest declares the version it needs (1 up to that): newer
/// versions only add imports, so a game built for an older one keeps working
/// unchanged, and an import added in version N needs `name = N` or later
/// (`abi::Requires::Stdlib`).
pub const STDLIB: &[(&str, u32)] = &[("mb2d", 2), ("mb3d", 2), ("gpu", 1)];

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Field-level checks that don't need the rest of the bundle.
    pub fn check(&self, errors: &mut Vec<String>) {
        let mut err = |m: String| errors.push(format!("manifest: {m}"));
        if self.abi != ABI {
            err(format!("abi = {} is not supported (this host speaks abi {ABI})", self.abi));
        }
        if !valid_id(&self.id) {
            err(format!("id {:?} must be reverse-DNS: lowercase [a-z0-9-] labels, at least two, ≤ 128 chars", self.id));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 64 {
            err("name must be 1–64 characters".into());
        }
        if !valid_semver(&self.version) {
            err(format!("version {:?} must be MAJOR.MINOR.PATCH", self.version));
        }
        if !self.creator.starts_with('@') || self.creator.len() < 2 {
            err(format!("creator {:?} must look like @handle", self.creator));
        }
        if self.logical_size.iter().any(|&d| !(64..=4096).contains(&d)) {
            err(format!("logical_size {:?}: each side must be 64–4096", self.logical_size));
        }
        let [w, h] = self.logical_size;
        match self.orientation {
            Orientation::Landscape if w < h => err(format!("logical_size {:?}: a landscape game is at least as wide as it is tall (e.g. [640, 360])", self.logical_size)),
            Orientation::Portrait | Orientation::Any if w > h => err(format!(
                "logical_size {:?}: a{} {} game gives its portrait size, no wider than it is tall (e.g. [360, 640])",
                self.logical_size,
                if self.orientation == Orientation::Any { "n" } else { "" },
                self.orientation.as_str()
            )),
            _ => {}
        }
        if self.inputs.is_empty() {
            err("inputs must list at least one of touch, mouse, keyboard, gamepad".into());
        }
        if has_dupes(&self.inputs) || has_dupes(&self.requires) || has_dupes(&self.sensors) || has_dupes(&self.needs) || has_dupes(&self.capabilities) {
            err("inputs, requires, sensors, needs and capabilities must not repeat entries".into());
        }
        for i in &self.requires {
            if !self.inputs.contains(i) {
                err(format!("requires = [\"{0}\"] also needs inputs = [\"{0}\"]", i.as_str()));
            }
        }
        // Phones have touch; a game that can't use it has to say what it needs instead.
        if !self.inputs.contains(&Input::Touch) && self.requires.is_empty() {
            err("a game without touch in inputs must say what it can't be played without: requires = [\"gamepad\"] (or keyboard, mouse)".into());
        }
        if self.requires.contains(&Input::Touch) {
            err("requires: touch is always there on a phone; list only what a player has to connect (gamepad, keyboard, mouse)".into());
        }
        for s in &self.needs {
            if *s == Sensor::Haptics {
                err("needs: haptics is output-only; a game can always be played without it".into());
            } else if !self.sensors.contains(s) {
                err(format!("needs = [\"{0}\"] also needs sensors = [\"{0}\"]", s.as_str()));
            }
        }
        for (name, version) in &self.stdlib {
            match STDLIB.iter().find(|(n, _)| n == name) {
                None => err(format!("unknown stdlib {name:?} (known: mb2d, mb3d, gpu)")),
                Some((_, v)) if *version == 0 || version > v => err(format!("stdlib {name} = {version} is not available (host has 1–{v})")),
                _ => {}
            }
        }
        let mut boards: Vec<u32> = self.scores.iter().map(|b| b.board).collect();
        boards.sort();
        if boards.windows(2).any(|w| w[0] == w[1]) {
            err("scores: board numbers must be unique".into());
        }
        for b in &self.scores {
            if b.board > 15 {
                err(format!("scores: board {} is out of range (0–15)", b.board));
            }
            if b.label.trim().is_empty() || b.label.chars().count() > 32 {
                err(format!("scores: board {} label must be 1–32 characters", b.board));
            }
        }
        match (self.capabilities.contains(&Capability::Score), self.scores.is_empty()) {
            (true, true) => err("capability \"score\" needs at least one [[scores]] board".into()),
            (false, false) => err("[[scores]] boards need capabilities = [\"score\"]".into()),
            _ => {}
        }
        if self.capabilities.contains(&Capability::Progress) && !self.capabilities.contains(&Capability::Store) {
            err("capability \"progress\" also needs \"store\" (progress is saved data under progress/)".into());
        }
        let daily: Vec<u32> = self.scores.iter().filter(|b| b.daily).map(|b| b.board).collect();
        if daily.contains(&0) {
            err("scores: board 0 can't be daily (it's the game's main board; give the daily run a board of its own)".into());
        }
        if daily.len() > 1 {
            err("scores: at most one board can be daily".into());
        }
        if let Some(e) = &self.engine
            && crate::studio::parse_engine(e).is_none()
        {
            err(format!("engine = {e:?}: the only engine is Studio's, \"studio@<major>.<minor>\" (e.g. \"studio@1.0\")"));
        }
        if self.capabilities.contains(&Capability::Net) {
            err("capability \"net\" is reserved and not available in v0".into());
        }
    }

    /// Bundle formats beyond the plain one this game needs the app to know
    /// ([`crate::studio::FORMATS`]): `studio` for a Studio game.
    pub fn needs_formats(&self) -> Vec<&'static str> {
        if self.engine.is_some() { vec!["studio"] } else { Vec::new() }
    }

    pub fn has_stdlib(&self, name: &str) -> bool {
        self.stdlib.contains_key(name)
    }

    /// The declared version of a stdlib (0 if it isn't declared).
    pub fn stdlib_version(&self, name: &str) -> u32 {
        self.stdlib.get(name).copied().unwrap_or(0)
    }
}

fn has_dupes<T: Ord + Clone>(v: &[T]) -> bool {
    let mut s = v.to_vec();
    s.sort();
    s.windows(2).any(|w| w[0] == w[1])
}

fn valid_id(id: &str) -> bool {
    let labels: Vec<_> = id.split('.').collect();
    id.len() <= 128
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn valid_semver(v: &str) -> bool {
    let parts: Vec<_> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
