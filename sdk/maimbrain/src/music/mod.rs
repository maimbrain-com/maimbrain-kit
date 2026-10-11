//! Music that follows the game: a [`Director`] that plays the music `mb
//! music --zones`, `--sections` or `--variations` makes, and drives the
//! layers and the tension kit from the game.
//!
//! **A music graph** (the default; [`Director::from_graph`]): one long
//! generated take and its graph (`<name>.music.json`, [`Graph`]), played the
//! way shipped games play adaptive music.
//! - The take plays **straight through**, and at a bar line now and then the
//!   Director takes a **jump link** (two bar lines where the bar before and
//!   the bar after each sound alike: Paul Lamere's Infinite Jukebox), so a
//!   zone's music goes on for as long as the player stays there without a
//!   loop point to hear. Jumps are random but seeded, so a replay hears the
//!   same music; a jump back of fewer than 8 bars is never taken.
//! - At the end of a zone's stretch of the take, it jumps back by a link, or
//!   **restarts the zone** from its entry under the swell.
//! - A change of zone ([`Director::go`]) waits for the **next exit cue** (the
//!   end of a 4-bar phrase), unless a seamless link into the zone (or into the
//!   bars the take leads into it with) comes first, or the take runs into it
//!   on its own. With neither, the new zone's segment starts on the cue under
//!   its stinger (or the swell). [`Change::NextBar`] doesn't wait for the cue.
//! - Every join is spliced like a segment in Wwise or FMOD: the new voice
//!   starts a little before the bar line (its **pre-entry**, the take's own
//!   pickup) fading in, and the old one's **post-exit** dies away after it,
//!   two voices overlapping (`Sound::play_from`, `Voice::stop_at`).
//! - The base and high layers move together, and the tension kit is kept on
//!   the take's phrase (it moves with each jump).
//!
//! **Loops** ([`Director::new`], `mb music --loops`): loops that share one
//! bar grid, switched on the bar under the new loop's stinger or the swell,
//! or crossfaded; variations rotated every N bars.
//!
//! Either way the high layer rises with intensity, and one `danger` 0–1
//! drives the **tension kit** (`--layers …,tension`): the tension layer and
//! the clock rise, a heartbeat quickens on the beat, the music narrows with a
//! low-pass, a sting on a near-catch, a clear release on the escape.
//!
//! Audio is output only, so none of this touches replays. The levels are the
//! ones the skill's `mix_check.py` passes and that are clearly heard on a
//! phone at full danger (the skill's audio.md "Music that follows the game"); with a
//! graph, the tension kit's are raised by the gain `mb music` measured for
//! the take ([`Levels::for_graph`]), since a loud take buries the defaults.
//!
//! [`Director::stop`] lets everything it started die away (the round is
//! over); [`Director::restart`] starts a zone again, never on top of what's
//! still playing.
//!
//! ```ignore
//! use maimbrain::music::{Change, Director, Graph, Mood};
//! // Load assets/run.music.json, then every file it names (graph.files()), as Sounds.
//! let graph = Graph::parse(&graph_asset.read().unwrap()).unwrap();
//! let mut d = Director::from_graph(graph, |path| sounds.get(path).copied(), sys::rand_seed());
//! let (town, grove) = (d.zone("lantern_town").unwrap(), d.zone("bamboo_grove").unwrap());
//! d.start(town, sys::time());
//! // Each frame:
//! d.update(sys::time(), dt, Mood { intensity: speed_k, danger: 1.0 - heron_gap / 8.0 });
//! // Entering the grove: at the next exit cue (or sooner through a seamless link).
//! d.go(grove, Change::Auto, sys::time());
//! // The heron strikes and misses:
//! d.near_miss(sys::time());
//! // Caught: the music dies away over a second; the next round starts it again.
//! d.stop(sys::time(), 1.0);
//! d.restart(town, sys::time());
//! ```

mod graph;
mod songs;
mod json;

pub use graph::{Files, Graph, Link, Section, TensionFiles, Zone};
pub use songs::{Setlist, Songs};

use crate::audio::{Bus, OPEN, Sound, Voice};

/// Where the director's sound goes: [`Live`] plays it; [`Log`] writes it down
/// (tests, and `mix_check.py` renders). `at` and `now` are game time ([`crate::sys::time`]).
pub trait Out {
    /// Starts `sound` heard from `at` (looping or not) on `bus`.
    fn play_at(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, looped: bool, bus: Bus) -> Voice;
    /// Starts `sound` heard from `at`, `from` seconds into it, rising over `fade` seconds ([`Sound::play_from`]).
    fn play_from(&mut self, sound: Sound, vol: f32, at: f64, from: f64, fade: f32, looped: bool, bus: Bus) -> Voice;
    fn set(&mut self, voice: Voice, vol: f32, now: f64);
    fn stop(&mut self, voice: Voice, now: f64);
    /// Lets `voice` die away from `at` over `fade` seconds ([`Voice::stop_at`]).
    fn stop_at(&mut self, voice: Voice, at: f64, fade: f32);
    /// The music bus's low-pass ([`Bus::lowpass`], resonance 0.707).
    fn lowpass(&mut self, cutoff: f32, ramp: f32, now: f64);
    /// The music bus's volume ([`Bus::volume`]).
    fn music_volume(&mut self, gain: f32, ramp: f32, now: f64);
}

/// Plays through the platform.
#[derive(Default)]
pub struct Live;

impl Out for Live {
    fn play_at(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, looped: bool, bus: Bus) -> Voice {
        let v = if looped { sound.play_looped_at(vol, 0.0, pitch, at) } else { sound.play_at(vol, 0.0, pitch, at) };
        v.bus(bus)
    }
    fn play_from(&mut self, sound: Sound, vol: f32, at: f64, from: f64, fade: f32, looped: bool, bus: Bus) -> Voice {
        let v = if looped { sound.play_looped_from(vol, 0.0, 1.0, at, from, fade) } else { sound.play_from(vol, 0.0, 1.0, at, from, fade) };
        v.bus(bus)
    }
    fn set(&mut self, voice: Voice, vol: f32, _now: f64) {
        voice.set(vol, 0.0, 1.0);
    }
    fn stop(&mut self, voice: Voice, _now: f64) {
        voice.stop();
    }
    fn stop_at(&mut self, voice: Voice, at: f64, fade: f32) {
        voice.stop_at(at, fade);
    }
    fn lowpass(&mut self, cutoff: f32, ramp: f32, _now: f64) {
        Bus::Music.lowpass(cutoff, 0.707, ramp);
    }
    fn music_volume(&mut self, gain: f32, ramp: f32, _now: f64) {
        Bus::Music.volume(gain, ramp);
    }
}

/// Writes what would be played as `mix_check.py` event lines, naming each
/// sound with [`Log::name`]:
/// `<seconds> <sound> <vol> <pitch> [loop|set|stop] [from=<s>] [fade=<s>] id=<voice>`
/// (`from`: seconds into the sound it starts at; `fade` on a start: its
/// fade-in, on a stop: its release; `id` tells two voices of one sound
/// apart). Bus changes become comments
/// (`# bus <seconds> lowpass <hz> <ramp>`, `# bus <seconds> volume <gain> <ramp>`).
#[derive(Default)]
pub struct Log {
    pub lines: Vec<String>,
    names: Vec<(u32, String)>,
    voices: Vec<(u32, u32, f32)>, // voice, sound, last vol
    /// Voices let go with a release: voice, sound, when (a sooner one replaces it).
    released: Vec<(u32, u32, f64)>,
    next: u32,
}

impl Log {
    /// A log whose voice ids start after `first`, so several logs (one per
    /// song of [`Songs`]) can be merged into one render without two voices sharing an id.
    pub fn with_ids_after(first: u32) -> Log {
        Log { next: first, ..Log::default() }
    }

    pub fn name(&mut self, sound: Sound, name: &str) {
        self.names.push((sound.0, name.to_string()));
    }

    fn sound_name(&self, s: u32) -> String {
        self.names.iter().find(|n| n.0 == s).map(|n| n.1.clone()).unwrap_or_else(|| format!("sound{s}"))
    }

    /// The lines in time order, one per line.
    pub fn text(&self) -> String {
        let time = |s: &str| -> f64 { s.trim_start_matches("# bus").split_whitespace().next().and_then(|t| t.parse().ok()).unwrap_or(0.0) };
        let mut l: Vec<(f64, &String)> = self.lines.iter().map(|s| (time(s), s)).collect();
        l.sort_by(|a, b| a.0.total_cmp(&b.0));
        l.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join("\n") + "\n"
    }

    fn start(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, looped: bool, extra: String) -> Voice {
        self.next += 1;
        self.voices.push((self.next, sound.0, vol));
        self.lines.push(format!("{at:.3} {} {vol:.3} {pitch:.4}{}{extra} id={}", self.sound_name(sound.0), if looped { " loop" } else { "" }, self.next));
        Voice(self.next)
    }
}

impl Out for Log {
    fn play_at(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, looped: bool, _bus: Bus) -> Voice {
        self.start(sound, vol, pitch, at, looped, String::new())
    }
    fn play_from(&mut self, sound: Sound, vol: f32, at: f64, from: f64, fade: f32, looped: bool, _bus: Bus) -> Voice {
        let mut extra = String::new();
        if from > 0.0 {
            extra += &format!(" from={from:.4}");
        }
        if fade > 0.0 {
            extra += &format!(" fade={fade:.3}");
        }
        self.start(sound, vol, 1.0, at, looped, extra)
    }
    fn set(&mut self, voice: Voice, vol: f32, now: f64) {
        let Some(i) = self.voices.iter().position(|v| v.0 == voice.0) else { return };
        if (self.voices[i].2 - vol).abs() < 0.005 {
            return;
        }
        self.voices[i].2 = vol;
        let name = self.sound_name(self.voices[i].1);
        self.lines.push(format!("{now:.3} {name} {vol:.3} 1.0 set id={}", voice.0));
    }
    fn stop(&mut self, voice: Voice, now: f64) {
        if let Some(i) = self.voices.iter().position(|v| v.0 == voice.0) {
            let (_, s, _) = self.voices.remove(i);
            let name = self.sound_name(s);
            self.lines.push(format!("{now:.3} {name} 0 1 stop id={}", voice.0));
        }
    }
    fn stop_at(&mut self, voice: Voice, at: f64, fade: f32) {
        let s = if let Some(i) = self.voices.iter().position(|v| v.0 == voice.0) {
            let (_, s, _) = self.voices.remove(i);
            self.released.push((voice.0, s, at));
            s
        } else if let Some(r) = self.released.iter_mut().find(|r| r.0 == voice.0 && r.2 > at + 1e-6) {
            // A release set for later, brought forward (Director::stop).
            r.2 = at;
            r.1
        } else {
            return;
        };
        let name = self.sound_name(s);
        self.lines.push(format!("{at:.3} {name} 0 1 stop fade={fade:.3} id={}", voice.0));
    }
    fn lowpass(&mut self, cutoff: f32, ramp: f32, now: f64) {
        self.lines.push(format!("# bus {now:.3} lowpass {cutoff:.0} {ramp:.3}"));
    }
    fn music_volume(&mut self, gain: f32, ramp: f32, now: f64) {
        self.lines.push(format!("# bus {now:.3} volume {gain:.3} {ramp:.3}"));
    }
}

/// How a [`Director::go`] changes the music.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Change {
    /// The default. **A graph**: at the next exit cue (the end of a phrase),
    /// or sooner through a seamless link into the zone or the take running
    /// into it; without either, the zone's segment starts on the cue under its
    /// stinger (or the swell). **Loops**: covered: the new loop's stinger if
    /// it has one, else the piece's swell ([`Director::swell`]), is played so
    /// its hit lands on the first bar it can reach; the old loop fades out
    /// under the last half beat before it and the new one starts from its top
    /// on that bar, so the listener hears a fill land, not two loops meeting.
    /// With neither, a one-bar crossfade.
    #[default]
    Auto,
    /// The new music starts on the next bar line while the old fades out over
    /// this many bars (loops: and the new one fades in over them, equal power).
    Crossfade(f32),
    /// **A graph**: always under the zone's stinger (or the swell), on the
    /// next exit cue. **Loops**: the same as `Auto` (the name it had first).
    Stinger,
    /// **A graph**: on the next bar line rather than the next exit cue, under
    /// the stinger or the swell unless a seamless link is there. **Loops**: as `Auto`.
    NextBar,
}

/// The tension kit (`mb music --layers …,tension`): all optional.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tension {
    /// `<name>_tension`: loops with the music, rises with the danger.
    pub layer: Option<Sound>,
    /// `<name>_tick`: the clock, only when it's close.
    pub tick: Option<Sound>,
    /// `<name>_heartbeat`: scheduled on the beat, more often as the danger grows.
    pub heartbeat: Option<Sound>,
    /// `<name>_sting`: a near-catch ([`Director::near_miss`]).
    pub sting: Option<Sound>,
}

/// What the music follows, each 0–1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mood {
    /// Energy: speed, combo, a crowd. Brings the high layer up.
    pub intensity: f32,
    /// A threat closing in (e.g. `1 − chaser_gap / 8 m`): the tension.
    pub danger: f32,
}

/// The levels and curves, public so a game can tune them.
#[derive(Clone, Copy, Debug)]
pub struct Levels {
    /// The high layer's share at rest; intensity and danger bring it up by `hi_rise`.
    pub hi_floor: f32,
    pub hi_rise: f32,
    /// The tension layer's vol at full danger. Its file sits 3 dB under the
    /// track, so 1.4 brings it level with it: +3–4 dB on a phone, clearly heard.
    pub tension_max: f32,
    pub tick_max: f32,
    /// The heartbeat's vol from where it starts (danger 0.35) to full danger.
    pub heartbeat: (f32, f32),
    /// The music bus's low-pass at full danger, Hz.
    pub narrow_hz: f32,
    /// A transition stinger's vol, and the near-catch sting's.
    pub stinger: f32,
    pub sting: f32,
    /// Seconds the danger takes to rise and to fall (faster down: an escape is heard at once).
    pub rise_s: f32,
    pub fall_s: f32,
}

impl Default for Levels {
    fn default() -> Levels {
        Levels { hi_floor: 0.55, hi_rise: 0.25, tension_max: 1.4, tick_max: 1.0, heartbeat: (0.15, 0.3), narrow_hz: 2500.0, stinger: 0.6, sting: 0.7, rise_s: 0.6, fall_s: 0.25 }
    }
}

impl Levels {
    /// The levels for a music graph: the defaults with the tension kit (the
    /// layer, the clock and the heartbeat) raised by the graph's
    /// `tension_gain`, which `mb music` measures from the take so the
    /// tension lifts it by a clear step on a phone at full danger (a loud,
    /// bright take needs more). No voice goes past the platform's vol 4.
    /// [`Director::from_graph`] uses these; a graph without a gain gets the defaults.
    pub fn for_graph(graph: &Graph) -> Levels {
        Levels::default().with_tension_gain(graph.tension_gain)
    }

    /// These levels with the tension kit's raised (or lowered) by `gain`.
    pub fn with_tension_gain(self, gain: f32) -> Levels {
        let g = if gain.is_finite() && gain > 0.0 { gain } else { 1.0 };
        let v = |x: f32| (x * g).min(MAX_VOL);
        Levels { tension_max: v(self.tension_max), tick_max: v(self.tick_max), heartbeat: (v(self.heartbeat.0), v(self.heartbeat.1)), ..self }
    }
}

/// The loudest a voice plays (`Sound::play`'s `vol` 0–4).
pub const MAX_VOL: f32 = 4.0;

/// How a graph's jumps are taken, public so a game can tune them.
#[derive(Clone, Copy, Debug)]
pub struct Wander {
    /// Bars after a jump before another may be taken at random.
    pub settle_bars: u32,
    /// The chance of taking a link at a bar line where one is good, from
    /// `settle_bars` on, rising by `chance_step` a bar to `chance_max`.
    pub chance: f32,
    pub chance_step: f32,
    pub chance_max: f32,
    /// [`Change::Auto`] waits at most this many bars for an exit cue (or a
    /// seamless link) after the bar line it could first be heard on; then
    /// it changes on the bar line under the stinger. 0 waits for the cue however far.
    pub wait_bars: u32,
}

impl Default for Wander {
    fn default() -> Wander {
        Wander { settle_bars: 4, chance: 0.15, chance_step: 0.1, chance_max: 0.5, wait_bars: 1 }
    }
}

/// A move the Director made through a graph, on a bar line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    /// Game time of the bar line.
    pub at: f64,
    /// The bar the take would have played from it, and the bar it plays.
    pub from: u32,
    pub to: u32,
    pub kind: MoveKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveKind {
    /// A jump link within the zone, and its score.
    Link(f32),
    /// A seamless link into the zone asked for (or into the bars the take leads into it with).
    Into(f32),
    /// The take ran into the zone asked for.
    Take,
    /// Into the zone asked for at an exit cue (or the next bar), its segment
    /// from its entry, under its stinger (`true`) or the swell.
    Change { stinger: bool },
    /// A plain change ([`Change::Crossfade`]): no cover.
    Crossfade,
    /// The zone again from its entry at the end of its stretch of the take, under the swell.
    Restart,
}

/// One loop of the piece, maybe split into base and high layers.
#[derive(Clone, Copy, Debug)]
struct Part {
    main: Sound,
    hi: Option<Sound>,
    stinger: Option<Cover>,
}

/// A one-shot that lands a hit at `land` seconds in.
type Cover = (Sound, f32);

/// A part that's sounding (or about to): its voices and its fade.
#[derive(Clone, Copy, Debug)]
struct Playing {
    part: usize,
    main: Voice,
    hi: Option<Voice>,
    /// Fade in from .0 to .1 (game time); an instant start has them equal.
    fade_in: (f64, f64),
    fade_out: Option<(f64, f64)>,
}

/// Lookahead for scheduling, seconds: anything heard sooner than this can't start from its top.
pub const LOOKAHEAD: f64 = 0.12;

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A voice of the take (and its high layer), until it has died away.
#[derive(Clone, Copy, Debug)]
struct TakeVoice {
    main: Voice,
    hi: Option<Voice>,
    /// Game time it starts (its pre-entry) and has died away by.
    from: f64,
    until: f64,
}

/// A zone change waiting for its bar line.
#[derive(Clone, Copy, Debug)]
struct Pending {
    zone: usize,
    urgent: bool,
    stinger: bool,
    /// A plain change: the old music dies away over this many seconds, no cover.
    plain: Option<f64>,
    /// Game time it was asked for.
    asked: f64,
}

/// Playing a graph.
struct GraphPlay {
    g: Graph,
    main: Sound,
    hi: Option<Sound>,
    stingers: Vec<Option<Cover>>,
    takes: Vec<TakeVoice>,
    /// The next bar line to decide (game time), and the take's bar after it if nothing changes.
    next_line: f64,
    next_bar: u32,
    zone: usize,
    pending: Option<Pending>,
    since_jump: u32,
    /// Bars heard lately, newest last.
    recent: Vec<u32>,
    rng: u64,
    moves: Vec<Move>,
    /// The tension kit's place: at bar line .0 (game time) it plays its bar .1.
    kit: Option<(f64, u32)>,
    /// The tension kit's loop: bars, its 1 seconds in.
    kit_bars: u32,
    kit_downbeat: f64,
    /// The levels the take's high layer and the tension voices were last set to.
    hi_vol: f32,
    kit_vols: (f32, f32),
    /// Seconds a decision is made ahead of its bar line (pre-entries, stingers, lookahead).
    lead: f64,
}

pub struct Director<O: Out = Live> {
    pub out: O,
    pub levels: Levels,
    /// How a graph's jumps are taken.
    pub wander: Wander,
    bpm: f64,
    bars: u32,
    /// Seconds into each loop file its downbeat is (the sidecar's `downbeat_seconds`).
    pre: f64,
    origin: f64,
    started: bool,
    parts: Vec<Part>,
    swell: Option<Cover>,
    tension: Tension,
    tension_voices: (Option<Voice>, Option<Voice>),
    playing: Vec<Playing>,
    current: usize,
    /// A change waiting for its bar: (part, the bar's time).
    pending: Option<(usize, f64)>,
    /// Parts to rotate through, every so many bars, and when the next turn is.
    rotation: Option<(Vec<usize>, u32, f64)>,
    danger: f32,
    peak: f32,
    relief_at: Option<f64>,
    next_beat: i64,
    last_heart: f64,
    duck_until: Option<f64>,
    cutoff: f32,
    seed: u32,
    graph: Option<Box<GraphPlay>>,
    /// One-shots it started (stingers, the swell, heartbeats, the sting) and when, until they're surely over.
    shots: Vec<(Voice, f64)>,
}

/// Seconds a one-shot the Director started is kept to be stopped by [`Director::stop`] (longer than any stinger).
const SHOT_KEEP: f64 = 8.0;

/// How long what's playing takes to die away when [`Director::restart`] (or a second
/// [`Director::start`]) starts the music again, seconds.
pub const RESTART_FADE: f32 = 0.25;

impl Director<Live> {
    /// Loops of `bars` bars of 4/4 at `bpm` (the sidecar's `bpm` and `bars`).
    pub fn new(bpm: f64, bars: u32) -> Director<Live> {
        Director::with_out(bpm, bars, Live)
    }

    /// Plays a music graph (`<name>.music.json`). `sound` gives the Sound for
    /// each audio file the graph names (`graph.files()`: the take or its
    /// layers, the swell, the stingers and the tension kit), None for one
    /// that isn't loaded; the take is required. `seed` makes the jumps the
    /// same every run (`sys::rand_seed()`, so a replay hears what was played).
    /// None if the take isn't there.
    pub fn from_graph(graph: Graph, sound: impl FnMut(&str) -> Option<Sound>, seed: u64) -> Option<Director<Live>> {
        Director::with_graph(graph, sound, seed, Live)
    }
}

impl<O: Out> Director<O> {
    pub fn with_out(bpm: f64, bars: u32, out: O) -> Director<O> {
        Director {
            out,
            levels: Levels::default(),
            wander: Wander::default(),
            bpm,
            bars: bars.max(1),
            pre: 0.0,
            origin: 0.0,
            started: false,
            parts: Vec::new(),
            swell: None,
            tension: Tension::default(),
            tension_voices: (None, None),
            playing: Vec::new(),
            current: 0,
            pending: None,
            rotation: None,
            danger: 0.0,
            peak: 0.0,
            relief_at: None,
            next_beat: 0,
            last_heart: f64::MIN,
            duck_until: None,
            cutoff: OPEN,
            seed: 0x9e37_79b9,
            graph: None,
            shots: Vec::new(),
        }
    }

    /// [`Director::from_graph`] into any [`Out`] ([`Log`] in tests and renders).
    pub fn with_graph(graph: Graph, mut sound: impl FnMut(&str) -> Option<Sound>, seed: u64, out: O) -> Option<Director<O>> {
        let f = graph.files.clone();
        let main = sound(f.take.as_deref().or(f.base.as_deref())?)?;
        let hi = if f.take.is_some() { None } else { f.hi.as_deref().and_then(&mut sound) };
        let mut d = Director::with_out(graph.bpm, graph.phrase_bars, out);
        d.levels = Levels::for_graph(&graph);
        if let Some((s, land)) = &f.swell
            && let Some(x) = sound(s)
        {
            d.swell = Some((x, *land));
        }
        let (mut kit_bars, mut kit_downbeat) = (8, 0.0);
        if let Some(t) = &f.tension {
            d.tension = Tension { layer: sound(&t.layer), tick: sound(&t.tick), heartbeat: sound(&t.heartbeat), sting: sound(&t.sting) };
            (kit_bars, kit_downbeat) = (t.bars.max(1), t.downbeat);
        }
        let stingers: Vec<Option<Cover>> = graph.zones.iter().map(|z| z.stinger.as_ref().and_then(|(s, land)| Some((sound(s)?, *land)))).collect();
        // Decisions are made far enough ahead for the longest pre-entry or cover to start on time.
        let pre = graph.sections.iter().map(|s| s.pre_entry).fold(graph.link_pre * graph.beat(), f64::max);
        let cover = stingers.iter().flatten().chain(d.swell.iter()).map(|c| c.1 as f64).fold(0.0, f64::max);
        let lead = LOOKAHEAD + pre.max(cover).min(graph.bar) + 0.02;
        d.seed = (seed as u32 ^ (seed >> 32) as u32) | 1;
        d.graph = Some(Box::new(GraphPlay {
            main,
            hi,
            stingers,
            takes: Vec::new(),
            next_line: 0.0,
            next_bar: 0,
            zone: 0,
            pending: None,
            since_jump: 0,
            recent: Vec::new(),
            rng: seed ^ 0x6a09_e667_f3bc_c909,
            moves: Vec::new(),
            kit: None,
            kit_bars,
            kit_downbeat,
            hi_vol: d.levels.hi_floor,
            kit_vols: (0.0, 0.0),
            lead,
            g: graph,
        }));
        Some(d)
    }

    /// The zone of the graph named `name`.
    pub fn zone(&self, name: &str) -> Option<usize> {
        self.graph.as_ref().and_then(|p| p.g.zone(name))
    }

    /// The graph being played.
    pub fn graph(&self) -> Option<&Graph> {
        self.graph.as_ref().map(|p| &p.g)
    }

    /// Every move made through the graph so far (jumps, changes, restarts), oldest first.
    pub fn moves(&self) -> &[Move] {
        self.graph.as_ref().map_or(&[], |p| &p.moves)
    }

    /// Adds a loop (its full mix, or its base and high layers); returns its index.
    pub fn add(&mut self, main: Sound, hi: Option<Sound>) -> usize {
        self.parts.push(Part { main, hi, stinger: None });
        self.parts.len() - 1
    }

    /// The stinger into `part` (`<name>_into_<section>`) and its sidecar's `land_seconds`.
    pub fn stinger(&mut self, part: usize, sound: Sound, land_seconds: f32) {
        if let Some(p) = self.parts.get_mut(part) {
            p.stinger = Some((sound, land_seconds));
        }
    }

    /// The piece's swell (`<name>_swell`) and its sidecar's `land_seconds`:
    /// the cover for a switch into a loop without a stinger of its own.
    pub fn swell(&mut self, sound: Sound, land_seconds: f32) {
        self.swell = Some((sound, land_seconds));
    }

    pub fn tension(&mut self, t: Tension) {
        self.tension = t;
    }

    /// Where each loop's downbeat is, seconds into the file: the sidecar's
    /// `downbeat_seconds` (0.01 for `mb music --loops`). Set it before
    /// [`Director::start`]. A graph knows its own.
    pub fn downbeat(&mut self, seconds: f64) {
        self.pre = seconds.max(0.0);
    }

    pub fn beat(&self) -> f64 {
        60.0 / self.bpm
    }

    pub fn bar(&self) -> f64 {
        match &self.graph {
            Some(p) => p.g.bar,
            None => 4.0 * self.beat(),
        }
    }

    /// Seconds per loop (every part's length).
    pub fn loop_seconds(&self) -> f64 {
        self.bars as f64 * self.bar()
    }

    /// Game time of bar 0's downbeat (every loop's grid; a graph's first bar line).
    pub fn origin(&self) -> f64 {
        self.origin
    }

    /// The first bar line at or after `t`.
    pub fn next_bar(&self, t: f64) -> f64 {
        let k = ((t - self.origin) / self.bar() - 1e-9).ceil().max(0.0);
        self.origin + k * self.bar()
    }

    /// The part playing (or the one a pending change goes to); with a graph, the zone.
    pub fn current(&self) -> usize {
        if let Some(p) = &self.graph {
            return p.pending.map_or(p.zone, |c| c.zone);
        }
        self.pending.map_or(self.current, |p| p.0)
    }

    /// Starts `part` (with a graph: the zone, from its entry) and the tension
    /// loops, silent, on a downbeat just ahead of `now`. Anything still
    /// playing dies away first ([`Director::restart`]).
    pub fn start(&mut self, part: usize, now: f64) {
        self.stop(now, RESTART_FADE);
        if self.graph.is_some() {
            self.start_graph(part, now);
            return;
        }
        self.origin = now + LOOKAHEAD + self.pre;
        self.started = true;
        self.current = part;
        let at = self.origin;
        let p = self.begin(part, at, (at, at));
        self.playing = vec![p];
        let t = self.tension;
        let file_at = at - self.pre;
        self.tension_voices = (
            t.layer.map(|s| self.out.play_at(s, 0.0, 1.0, file_at, true, Bus::Music)),
            t.tick.map(|s| self.out.play_at(s, 0.0, 1.0, file_at, true, Bus::Music)),
        );
        self.next_beat = 0;
    }

    /// [`Director::start`] timed so `part`'s first downbeat (with a graph:
    /// the zone's entry) is heard at game time `downbeat`, which must be at
    /// least [`LOOKAHEAD`] plus its pickup ahead to start from the top. This
    /// is how [`Songs`] starts the next song on the bar line the last one
    /// leaves on.
    pub fn start_at(&mut self, part: usize, downbeat: f64) {
        let pre = self.entry_pre(part);
        self.start(part, downbeat - LOOKAHEAD - pre);
    }

    /// Seconds of the music before `part`'s first downbeat that start sounding first (a graph's pre-entry).
    fn entry_pre(&self, part: usize) -> f64 {
        match &self.graph {
            Some(p) => {
                let zone = part.min(p.g.zones.len().saturating_sub(1));
                let entry = p.g.top(zone);
                p.g.sections[p.g.zones[zone].sections[0]].pre_entry.min(p.g.bar_at(entry))
            }
            None => self.pre,
        }
    }

    /// The first exit cue (the end of a phrase) at or after game time
    /// `after`, if the music plays on as it is: with a graph, the take's
    /// cues; with loops, every `bars` from the start. A bar line within two
    /// phrases if there's no cue that soon.
    pub fn next_exit(&self, after: f64) -> f64 {
        let bar = self.bar();
        if let Some(p) = &self.graph {
            // The take's bar at bar line `t`, if it plays straight on from the next undecided one.
            let k0 = ((after - p.next_line) / bar - 1e-9).ceil();
            let first = p.next_line + k0 * bar;
            for i in 0..(2 * p.g.phrase_bars.max(1)) as i64 {
                let t = first + i as f64 * bar;
                let b = p.next_bar as i64 + k0 as i64 + i;
                if b > 0 && p.g.exit_cue(b as u32) {
                    return t;
                }
            }
            return first;
        }
        let phrase = bar * self.bars.clamp(1, 4) as f64;
        let k = ((after - self.origin) / phrase - 1e-9).ceil().max(0.0);
        self.origin + k * phrase
    }

    /// What covers a change into `part`: its stinger, else the swell.
    fn cover_into(&self, part: usize) -> Option<Cover> {
        match &self.graph {
            Some(p) => p.stingers.get(part).copied().flatten().or(self.swell),
            None => self.parts.get(part).and_then(|p| p.stinger).or(self.swell),
        }
    }

    /// Starts `part` with its downbeat heard at `at` (the file `pre` earlier).
    fn begin(&mut self, part: usize, at: f64, fade_in: (f64, f64)) -> Playing {
        let p = self.parts[part];
        let v0 = if fade_in.1 > fade_in.0 { 0.0 } else { 1.0 };
        let at = at - self.pre;
        let main = self.out.play_at(p.main, v0, 1.0, at, true, Bus::Music);
        let hi = p.hi.map(|h| self.out.play_at(h, v0 * self.levels.hi_floor, 1.0, at, true, Bus::Music));
        Playing { part, main, hi, fade_in, fade_out: None }
    }

    /// Changes to `part` (with a graph: the zone) on a bar line (see
    /// [`Change`]). A change already waiting for its bar is replaced.
    pub fn go(&mut self, part: usize, change: Change, now: f64) {
        if let Some(p) = &mut self.graph {
            if !self.started || part >= p.g.zones.len() {
                return;
            }
            if part == p.pending.map_or(p.zone, |c| c.zone) {
                return;
            }
            // Back to the zone that's playing: just don't change.
            if part == p.zone {
                p.pending = None;
                return;
            }
            let bar = p.g.bar;
            p.pending = Some(Pending {
                zone: part,
                urgent: matches!(change, Change::NextBar | Change::Crossfade(_)),
                stinger: change == Change::Stinger,
                plain: match change {
                    Change::Crossfade(b) => Some(b.max(0.0) as f64 * bar),
                    _ => None,
                },
                asked: now,
            });
            return;
        }
        if !self.started || part >= self.parts.len() || part == self.current() {
            return;
        }
        if let Some(r) = &self.rotation
            && !r.0.contains(&part)
        {
            self.rotation = None;
        }
        // Undo a change that hasn't been heard yet.
        if let Some((_, at)) = self.pending.take() {
            let keep: Vec<Playing> = self.playing.iter().filter(|p| p.fade_in.0 >= at && p.part != self.current).copied().collect();
            for p in keep {
                self.out.stop(p.main, now);
                if let Some(h) = p.hi {
                    self.out.stop(h, now);
                }
            }
            self.playing.retain(|p| !(p.fade_in.0 >= at && p.part != self.current));
            for p in self.playing.iter_mut() {
                p.fade_out = None;
            }
        }
        let bar = self.bar();
        let cover = self.parts[part].stinger.or(self.swell);
        let (at, fade_in, out) = match (change, cover) {
            (Change::Auto | Change::Stinger | Change::NextBar, Some((sound, land))) => {
                let at = self.next_bar(now + LOOKAHEAD + self.pre + land as f64);
                self.shot(sound, self.levels.stinger, 1.0, at - land as f64, Bus::Music);
                let half = self.beat() / 2.0;
                (at, (at, at), (at - half, at))
            }
            (Change::Crossfade(bars), _) => {
                let at = self.next_bar(now + LOOKAHEAD + self.pre);
                let len = bars.max(0.0) as f64 * bar;
                (at, (at, at + len), (at, at + len))
            }
            (Change::Auto | Change::Stinger | Change::NextBar, None) => {
                let at = self.next_bar(now + LOOKAHEAD + self.pre);
                (at, (at, at + bar), (at, at + bar))
            }
        };
        for p in self.playing.iter_mut() {
            if p.fade_out.is_none() {
                p.fade_out = Some(out);
            }
        }
        let p = self.begin(part, at, fade_in);
        self.playing.push(p);
        self.pending = Some((part, at));
    }

    /// Rotates through `parts` (the one playing first, if it's among them),
    /// crossfading to the next every `every_bars` bars: two or more
    /// variations of a zone heard in turn instead of one loop on repeat.
    /// A graph doesn't need it (its zones move through their own variations):
    /// there it does nothing.
    pub fn rotate(&mut self, parts: &[usize], every_bars: u32, now: f64) {
        if self.graph.is_some() {
            return;
        }
        let parts: Vec<usize> = parts.iter().copied().filter(|&p| p < self.parts.len()).collect();
        if parts.len() < 2 || every_bars == 0 {
            self.rotation = None;
            return;
        }
        // Counted from the bar the current part starts on (or the next one).
        let from = self.pending.map_or(now + LOOKAHEAD + self.pre, |p| p.1);
        let next = self.next_bar(from) + every_bars as f64 * self.bar();
        self.rotation = Some((parts, every_bars, next));
    }

    /// A near-catch: the sting now, the music dipped under it and back.
    pub fn near_miss(&mut self, now: f64) {
        if !self.started {
            return;
        }
        if let Some(s) = self.tension.sting {
            self.shot(s, self.levels.sting, 1.0, now, Bus::Sfx);
        }
        self.out.music_volume(0.6, 0.02, now);
        self.duck_until = Some(now + 0.5);
    }

    /// Stops the music: every voice the Director started (the take or the
    /// loops and their high layers, the tension layer and the clock, and any
    /// stinger, swell, heartbeat or sting not over yet, even one scheduled
    /// ahead) dies away from `now` over `fade` seconds, as [`Voice::stop_at`]
    /// does (60 dB over it; 0 cuts it with a few ms of ramp). The music bus's
    /// low-pass opens and its level comes back over the same time. Until
    /// [`Director::start`] or [`Director::restart`], [`Director::update`],
    /// [`Director::go`] and [`Director::near_miss`] do nothing. Calling it
    /// again does nothing more.
    pub fn stop(&mut self, now: f64, fade: f32) {
        let fade = fade.max(0.0);
        let mut vs: Vec<Voice> = Vec::new();
        for p in self.playing.drain(..) {
            vs.push(p.main);
            vs.extend(p.hi);
        }
        let (layer, tick) = std::mem::take(&mut self.tension_voices);
        vs.extend(layer.into_iter().chain(tick));
        vs.extend(self.shots.drain(..).map(|s| s.0));
        if let Some(p) = &mut self.graph {
            // A voice already dying (or set to die at a bar line ahead) is let go now too.
            for v in p.takes.drain(..).filter(|v| v.until > now) {
                vs.push(v.main);
                vs.extend(v.hi);
            }
            p.pending = None;
            p.kit = None;
        }
        for v in vs {
            self.out.stop_at(v, now, fade);
        }
        self.started = false;
        self.pending = None;
        self.rotation = None;
        (self.danger, self.peak, self.relief_at) = (0.0, 0.0, None);
        if self.cutoff < OPEN {
            self.cutoff = OPEN;
            self.out.lowpass(OPEN, fade.max(0.02), now);
        }
        if self.duck_until.take().is_some() {
            self.out.music_volume(1.0, fade.max(0.02), now);
        }
    }

    /// Starts the music again from `part` (with a graph: the zone, from its
    /// entry), as [`Director::start`] does, after a [`Director::stop`] or in
    /// place of what's playing: anything still sounding dies away over
    /// [`RESTART_FADE`] first, so there are never two sets of voices.
    pub fn restart(&mut self, part: usize, now: f64) {
        self.start(part, now);
    }

    /// Whether it's playing: started and not stopped.
    pub fn playing(&self) -> bool {
        self.started
    }

    /// A one-shot, kept so [`Director::stop`] can stop it.
    fn shot(&mut self, sound: Sound, vol: f32, pitch: f32, at: f64, bus: Bus) -> Voice {
        let v = self.out.play_at(sound, vol, pitch, at, false, bus);
        self.shots.retain(|s| s.1 > at - SHOT_KEEP);
        self.shots.push((v, at));
        v
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Call every frame with game time, the frame's dt and what the music follows.
    pub fn update(&mut self, now: f64, dt: f32, mood: Mood) {
        if !self.started {
            return;
        }
        let lv = self.levels;
        // The danger, smoothed: up over ~0.6 s, down over ~0.25 s.
        let raw = mood.danger.clamp(0.0, 1.0);
        let k = if raw > self.danger { dt / lv.rise_s } else { dt / lv.fall_s };
        self.danger += (raw - self.danger) * k.min(1.0);
        let d = self.danger;
        // An escape: from close (≥ 0.6) to clear (≤ 0.3) gives two bars of relief.
        self.peak = self.peak.max(d);
        if self.peak >= 0.6 && d <= 0.3 {
            self.relief_at = Some(now);
            self.peak = d;
        }
        let relief = match self.relief_at {
            Some(t0) if now - t0 < 2.0 * self.bar() => 0.25 * (std::f64::consts::PI * (now - t0) / (2.0 * self.bar())).sin() as f32,
            _ => 0.0,
        };
        let hi = (lv.hi_floor + lv.hi_rise * smoothstep(0.0, 0.6, mood.intensity.max(d)) + relief).min(1.0);
        let kit = (lv.tension_max * smoothstep(0.05, 0.7, d), lv.tick_max * smoothstep(0.55, 0.95, d));
        if self.graph.is_some() {
            self.update_graph(now, hi, kit);
        } else {
            self.update_loops(now, hi);
        }
        // The tension.
        if let Some(v) = self.tension_voices.0 {
            self.out.set(v, kit.0, now);
        }
        if let Some(v) = self.tension_voices.1 {
            self.out.set(v, kit.1, now);
        }
        // The world narrows: open → narrow_hz at full danger (exponential in Hz).
        let cut = OPEN * (lv.narrow_hz / OPEN).powf(smoothstep(0.4, 1.0, d));
        if (cut / self.cutoff - 1.0).abs() > 0.02 {
            self.cutoff = cut;
            self.out.lowpass(if cut > OPEN * 0.95 { OPEN } else { cut }, 0.1, now);
        }
        // The heartbeat: on beat 3 from 0.35 (the tension layer has its own on 1), on every
        // beat from 0.7, never closer than 0.33 s; louder with the danger, a little different each time.
        if let Some(hb) = self.tension.heartbeat {
            let beat = self.bar() / 4.0;
            let first = ((now + LOOKAHEAD - self.origin) / beat).ceil() as i64;
            self.next_beat = self.next_beat.max(first);
            let at = self.origin + self.next_beat as f64 * beat;
            if at <= now + LOOKAHEAD + 0.1 {
                let b = self.next_beat.rem_euclid(4);
                let due = (d >= 0.7 && b != 0) || (d >= 0.35 && b == 2);
                if due && at - self.last_heart >= 0.33 {
                    let vol = (lv.heartbeat.0 + (lv.heartbeat.1 - lv.heartbeat.0) * smoothstep(0.35, 1.0, d)) * 10f32.powf((self.rand() * 3.0 - 1.5) / 20.0);
                    let pitch = 1.0 + (self.rand() - 0.5) * 0.02;
                    self.shot(hb, vol, pitch, at, Bus::Music);
                    self.last_heart = at;
                }
                self.next_beat += 1;
            }
        }
        // Back from the duck.
        if let Some(t) = self.duck_until
            && now >= t
        {
            self.out.music_volume(1.0, 0.3, now);
            self.duck_until = None;
        }
    }

    /// Loops: changes heard, rotation, and each part's level (equal-power fades).
    fn update_loops(&mut self, now: f64, hi: f32) {
        // A change heard: it's the current part now.
        if let Some((part, at)) = self.pending
            && now >= at
        {
            self.current = part;
            self.pending = None;
        }
        // Rotation.
        if self.pending.is_none()
            && let Some((parts, every, next)) = self.rotation.clone()
            && now + LOOKAHEAD + 0.05 >= next
        {
            let i = parts.iter().position(|&p| p == self.current).map_or(0, |i| (i + 1) % parts.len());
            let to = parts[i];
            self.rotation = Some((parts, every, next + every as f64 * self.bar()));
            if to != self.current {
                self.go(to, Change::Crossfade(1.0), now);
            }
        }
        let mut done = Vec::new();
        for (i, p) in self.playing.iter().enumerate() {
            let fin = if p.fade_in.1 > p.fade_in.0 { ((now - p.fade_in.0) / (p.fade_in.1 - p.fade_in.0)).clamp(0.0, 1.0) as f32 } else { 1.0 };
            let fout = match p.fade_out {
                Some((a, b)) if b > a => ((now - a) / (b - a)).clamp(0.0, 1.0) as f32,
                Some((_, b)) if now >= b => 1.0,
                _ => 0.0,
            };
            let g = (fin * std::f32::consts::FRAC_PI_2).sin() * (fout * std::f32::consts::FRAC_PI_2).cos();
            if now < p.fade_in.0 - self.pre {
                continue; // not heard yet: it started at its own vol
            }
            self.out.set(p.main, g, now);
            if let Some(h) = p.hi {
                self.out.set(h, g * hi, now);
            }
            if fout >= 1.0 {
                done.push(i);
            }
        }
        for i in done.into_iter().rev() {
            let p = self.playing.remove(i);
            self.out.stop(p.main, now);
            if let Some(h) = p.hi {
                self.out.stop(h, now);
            }
        }
    }
}

/// What the Director does at a bar line of a graph.
enum Act {
    /// The take goes on.
    Stay,
    /// Into bar `to`, the new voice coming in `pre` seconds early, the old dying away over `post`,
    /// under a cover that lands on the bar line.
    Jump { to: u32, pre: f64, post: f64, cover: Option<Cover>, kind: MoveKind },
}

impl<O: Out> Director<O> {
    fn start_graph(&mut self, zone: usize, now: f64) {
        let Some(p) = &mut self.graph else { return };
        let zone = zone.min(p.g.zones.len() - 1);
        let entry = p.g.top(zone);
        let take_at = p.g.bar_at(entry);
        let pre = p.g.sections[p.g.zones[zone].sections[0]].pre_entry.min(take_at);
        let t0 = now + LOOKAHEAD + pre;
        self.origin = t0;
        self.started = true;
        p.zone = zone;
        p.pending = None;
        p.next_line = t0 + p.g.bar;
        p.next_bar = entry + 1;
        p.recent = vec![entry];
        p.since_jump = 0;
        let main = self.out.play_from(p.main, 1.0, t0 - pre, take_at - pre, pre as f32, false, Bus::Music);
        let hi = p.hi.map(|h| self.out.play_from(h, p.hi_vol, t0 - pre, take_at - pre, pre as f32, false, Bus::Music));
        p.takes = vec![TakeVoice { main, hi, from: t0 - pre, until: f64::INFINITY }];
        // The tension kit, silent, on the take's phrase.
        let kb = kit_bar(&p.g, entry, p.kit_bars);
        let from = p.kit_downbeat + kb as f64 * p.g.bar;
        let t = self.tension;
        self.tension_voices = (
            t.layer.map(|s| self.out.play_from(s, 0.0, t0, from, 0.0, true, Bus::Music)),
            t.tick.map(|s| self.out.play_from(s, 0.0, t0, from, 0.0, true, Bus::Music)),
        );
        if t.layer.is_some() || t.tick.is_some() {
            p.kit = Some((t0, kb));
        }
        self.next_beat = 0;
    }

    fn update_graph(&mut self, now: f64, hi: f32, kit: (f32, f32)) {
        let Some(p) = &mut self.graph else { return };
        p.kit_vols = kit;
        loop {
            let p = self.graph.as_mut().unwrap();
            let t = p.next_line;
            if t - p.lead > now {
                break;
            }
            p.next_line += p.g.bar;
            self.decide(t);
        }
        let Some(p) = &mut self.graph else { return };
        p.hi_vol = hi;
        p.takes.retain(|v| v.until > now);
        for v in &p.takes {
            if let (Some(h), true) = (v.hi, v.until.is_infinite() && v.from <= now) {
                self.out.set(h, hi, now);
            }
        }
    }

    /// A random number in [0, 1) from the graph's own generator.
    fn graph_rand(&mut self) -> f32 {
        let Some(p) = &mut self.graph else { return 0.0 };
        p.rng ^= p.rng << 13;
        p.rng ^= p.rng >> 7;
        p.rng ^= p.rng << 17;
        (p.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Decides bar line `t`: the take goes on, or jumps.
    fn decide(&mut self, t: f64) {
        let act = self.choose(t);
        let p = self.graph.as_mut().unwrap();
        let b = p.next_bar;
        let played = match act {
            Act::Stay => {
                p.since_jump += 1;
                b
            }
            Act::Jump { to, pre, post, cover, kind } => {
                p.moves.push(Move { at: t, from: b, to, kind });
                p.since_jump = 0;
                let take_at = p.g.bar_at(to);
                let pre = pre.min(take_at).max(0.0);
                for v in p.takes.iter_mut().filter(|v| v.until > t) {
                    self.out.stop_at(v.main, t, post as f32);
                    if let Some(h) = v.hi {
                        self.out.stop_at(h, t, post as f32);
                    }
                    v.until = t + post;
                }
                let main = self.out.play_from(p.main, 1.0, t - pre, take_at - pre, pre as f32, false, Bus::Music);
                let hi = p.hi.map(|h| self.out.play_from(h, p.hi_vol, t - pre, take_at - pre, pre as f32, false, Bus::Music));
                p.takes.push(TakeVoice { main, hi, from: t - pre, until: f64::INFINITY });
                if let Some((s, land)) = cover {
                    let at = t - land as f64;
                    let v = self.out.play_at(s, self.levels.stinger, 1.0, at, false, Bus::Music);
                    self.shots.retain(|s| s.1 > at - SHOT_KEEP);
                    self.shots.push((v, at));
                }
                to
            }
        };
        p.next_bar = played + 1;
        p.recent.push(played);
        let keep = p.g.min_repeat as usize;
        if p.recent.len() > keep {
            p.recent.remove(0);
        }
        // The tension kit follows the take's phrase: moved when the bar it plays isn't the take's.
        if let Some((t0, b0)) = p.kit {
            let n = p.kit_bars;
            let at = (b0 as i64 + ((t - t0) / p.g.bar).round() as i64).rem_euclid(n as i64) as u32;
            let want = kit_bar(&p.g, played, n);
            if at != want {
                let x = p.g.beat() / 4.0;
                let from = p.kit_downbeat + want as f64 * p.g.bar - x;
                let len = n as f64 * p.g.bar;
                let from = from.rem_euclid(len);
                let t_ = self.tension;
                let vols = p.kit_vols;
                let old = self.tension_voices;
                self.tension_voices = (
                    t_.layer.map(|s| self.out.play_from(s, vols.0, t - x, from, x as f32, true, Bus::Music)),
                    t_.tick.map(|s| self.out.play_from(s, vols.1, t - x, from, x as f32, true, Bus::Music)),
                );
                for v in [old.0, old.1].into_iter().flatten() {
                    self.out.stop_at(v, t, (2.0 * x) as f32);
                }
                let p = self.graph.as_mut().unwrap();
                p.kit = Some((t, want));
            }
        }
    }

    /// What to do at bar line `t` (see the module's docs).
    fn choose(&mut self, t: f64) -> Act {
        let (r, r2) = (self.graph_rand(), self.graph_rand());
        let w = self.wander;
        let swell = self.swell;
        let p = self.graph.as_mut().unwrap();
        let g = &p.g;
        let b = p.next_bar;
        let zc = p.zone;
        let beat = g.beat();
        let link_splice = |to: u32, kind: MoveKind| Act::Jump { to, pre: g.link_pre * beat, post: g.link_post * beat, cover: None, kind };
        // A segment from zone z's entry: its pre-entry, and the post-exit of the section being left.
        let leaving = b.checked_sub(1).and_then(|k| g.section_of(k));
        let post = leaving.map_or(g.bar, |s| g.sections[s].post_exit.max(beat));
        let segment = |z: usize, cover: Option<Cover>, kind: MoveKind, post: f64| {
            let to = g.top(z);
            Act::Jump { to, pre: g.sections[g.zones[z].sections[0]].pre_entry, post, cover, kind }
        };
        // A link from b that keeps to zone z, isn't a short repeat, and isn't into what was just heard.
        let fresh = |l: &&Link, z: usize, thr: f32| {
            l.from == b && l.into_zone.is_none() && g.zone_of(l.to) == Some(z) && l.score >= thr && (l.to > b || b - l.to >= g.min_repeat) && !p.recent.contains(&l.to)
        };
        if let Some(c) = p.pending {
            // The take runs into the zone asked for.
            if g.zone_of(b) == Some(c.zone) {
                p.zone = c.zone;
                p.pending = None;
                p.moves.push(Move { at: t, from: b, to: b, kind: MoveKind::Take });
                return Act::Stay;
            }
            // A seamless link into it (or into the bars the take leads into it with).
            if !c.stinger && c.plain.is_none() {
                let best = g.links.iter().filter(|l| l.from == b && g.link_zone(l) == Some(c.zone) && l.score >= g.seamless).max_by(|x, y| x.score.total_cmp(&y.score)).copied();
                if let Some(l) = best {
                    if g.zone_of(l.to) == Some(c.zone) {
                        p.zone = c.zone;
                        p.pending = None;
                    }
                    return link_splice(l.to, MoveKind::Into(l.score));
                }
            }
            // At an exit cue, or now: if it's urgent, it has waited long enough, or the zone runs out. Its segment, covered.
            let waited = w.wait_bars > 0 && t - c.asked >= p.lead + w.wait_bars as f64 * g.bar;
            if c.urgent || waited || g.exit_cue(b) || g.zone_of(b) != Some(zc) {
                p.zone = c.zone;
                p.pending = None;
                if let Some(post) = c.plain {
                    return segment(c.zone, None, MoveKind::Crossfade, post.max(beat));
                }
                let stinger = p.stingers[c.zone];
                let cover = stinger.or(swell);
                return segment(c.zone, cover, MoveKind::Change { stinger: stinger.is_some() }, post);
            }
            return Act::Stay;
        }
        let thr = g.zones[zc].threshold;
        // About to leave the zone (or the take ends): back by a link, or the zone again from its entry.
        if g.zone_of(b) != Some(zc) {
            let best = g.links.iter().filter(|l| fresh(l, zc, thr)).max_by(|x, y| x.score.total_cmp(&y.score)).copied();
            return match best {
                Some(l) => link_splice(l.to, MoveKind::Link(l.score)),
                None => segment(zc, swell, MoveKind::Restart, post),
            };
        }
        let mut options: Vec<Link> = g.links.iter().filter(|l| fresh(l, zc, thr)).copied().collect();
        if options.is_empty() {
            return Act::Stay;
        }
        // In the last phrase before the zone runs out, a way back is taken whenever there is one.
        let mut out = b;
        while g.zone_of(out) == Some(zc) && out < g.end {
            out += 1;
        }
        let back: Vec<Link> = options.iter().filter(|l| l.to < b).copied().collect();
        if out - b <= g.phrase_bars && !back.is_empty() {
            options = back;
        } else {
            if p.since_jump < w.settle_bars {
                return Act::Stay;
            }
            let chance = (w.chance + w.chance_step * (p.since_jump - w.settle_bars) as f32).min(w.chance_max);
            if r >= chance {
                return Act::Stay;
            }
        }
        // Which one: by chance, the better ones more often.
        let total: f32 = options.iter().map(|l| l.score).sum();
        let mut x = r2 * total;
        let mut pick = options[0];
        for l in &options {
            if x < l.score {
                pick = *l;
                break;
            }
            x -= l.score;
        }
        link_splice(pick.to, MoveKind::Link(pick.score))
    }
}

/// The tension kit's bar for take bar `k`: its phrase follows the section's, from its entry.
fn kit_bar(g: &Graph, k: u32, bars: u32) -> u32 {
    let entry = g.section_of(k).map_or(g.first, |s| g.sections[s].entry);
    k.saturating_sub(entry) % bars.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn director() -> Director<Log> {
        let mut log = Log::default();
        for (i, n) in ["a", "a_hi", "b", "b_hi", "into_b", "tension", "tick", "heartbeat", "sting"].iter().enumerate() {
            log.name(Sound(i as u32 + 1), n);
        }
        let mut d = Director::with_out(120.0, 8, log);
        let a = d.add(Sound(1), Some(Sound(2)));
        let b = d.add(Sound(3), Some(Sound(4)));
        d.stinger(b, Sound(5), 1.0);
        d.tension(Tension { layer: Some(Sound(6)), tick: Some(Sound(7)), heartbeat: Some(Sound(8)), sting: Some(Sound(9)) });
        assert_eq!((a, b), (0, 1));
        d
    }

    fn run(d: &mut Director<Log>, from: f64, to: f64, mood: impl Fn(f64) -> Mood) {
        let mut t = from;
        while t < to {
            d.update(t, 1.0 / 60.0, mood(t));
            t += 1.0 / 60.0;
        }
    }

    /// The op of a log line: "" for a one-shot, "loop", "set" or "stop".
    fn op(l: &str) -> &str {
        l.split_whitespace().nth(4).filter(|w| !w.contains('=')).unwrap_or("")
    }

    /// A `key=value` of a log line.
    fn field(l: &str, key: &str) -> Option<f64> {
        l.split_whitespace().find_map(|w| w.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
    }

    fn events<'a>(d: &'a Director<Log>, name: &str, which: &str) -> Vec<(f64, &'a str)> {
        d.out
            .lines
            .iter()
            .filter(|l| l.split_whitespace().nth(1) == Some(name) && op(l) == which)
            .map(|l| (l.split_whitespace().next().unwrap().parse().unwrap(), l.as_str()))
            .collect()
    }

    #[test]
    fn stinger_lands_on_the_bar_and_the_new_part_starts_there() {
        let mut d = director();
        d.start(0, 10.0);
        let calm = |_| Mood::default();
        run(&mut d, 10.0, 13.3, calm);
        d.go(1, Change::Stinger, 13.3);
        // Bars are 2 s from 10.12; the stinger needs 1 s + lookahead: the bar at 16.12.
        let st = events(&d, "into_b", "");
        assert_eq!(st.len(), 1);
        assert!((st[0].0 - 15.12).abs() < 1e-3, "{:?}", st);
        let b = events(&d, "b", "loop");
        assert!((b[0].0 - 16.12).abs() < 1e-3, "{b:?}");
        assert_eq!(d.current(), 1);
        run(&mut d, 13.3, 18.0, calm);
        // The old part fades out under the riser's last half beat and stops by the bar.
        let stop = events(&d, "a", "stop");
        assert!(stop.len() == 1 && stop[0].0 >= 16.12 && stop[0].0 < 16.2, "{stop:?}");
        assert!(events(&d, "a", "set").iter().any(|(t, l)| *t > 15.8 && *t < 16.12 && l.contains(" 0.")));
    }

    #[test]
    fn the_default_change_is_covered_by_a_stinger_or_the_swell() {
        let mut log = Log::default();
        for (i, n) in ["a", "b", "c", "into_b", "swell"].iter().enumerate() {
            log.name(Sound(i as u32 + 1), n);
        }
        let mut d = Director::with_out(120.0, 8, log);
        let (a, b, c) = (d.add(Sound(1), None), d.add(Sound(2), None), d.add(Sound(3), None));
        d.stinger(b, Sound(4), 1.0);
        d.swell(Sound(5), 0.5);
        assert_eq!(Change::default(), Change::Auto);
        d.start(a, 0.0);
        let calm = |_| Mood::default();
        run(&mut d, 0.0, 3.0, calm);
        // Into b: its own stinger; into c: the swell, its hit on c's first bar.
        d.go(b, Change::default(), 3.0);
        run(&mut d, 3.0, 9.0, calm);
        d.go(c, Change::default(), 9.0);
        run(&mut d, 9.0, 14.0, calm);
        let (sb, sw) = (events(&d, "into_b", ""), events(&d, "swell", ""));
        let (lb, lc) = (events(&d, "b", "loop"), events(&d, "c", "loop"));
        assert_eq!((sb.len(), sw.len()), (1, 1), "{:?}", d.out.lines);
        assert!((sb[0].0 + 1.0 - lb[0].0).abs() < 1e-6 && (sw[0].0 + 0.5 - lc[0].0).abs() < 1e-6);
        // The new loop starts at full level on the hit; the old one is gone by then.
        assert!(lc[0].1.contains(" 1.000 "), "{lc:?}");
        let stop_b = events(&d, "b", "stop");
        assert!(stop_b.len() == 1 && stop_b[0].0 >= lc[0].0 && stop_b[0].0 < lc[0].0 + 0.05, "{stop_b:?}");
    }

    #[test]
    fn crossfade_and_rotation_stay_on_the_grid() {
        let mut d = director();
        d.downbeat(0.01); // mb's loops have their downbeat 10 ms in
        d.start(0, 0.0);
        d.rotate(&[0, 1], 4, 0.0);
        run(&mut d, 0.0, 40.0, |_| Mood { intensity: 0.5, danger: 0.0 });
        // Every start is on a bar line from the origin, and the parts take turns every 4 bars.
        let starts: Vec<f64> = d.out.lines.iter().filter(|l| op(l) == "loop" && !l.contains("tension") && !l.contains("tick") && !l.contains("_hi")).map(|l| l.split_whitespace().next().unwrap().parse().unwrap()).collect();
        assert!(starts.len() >= 4, "{starts:?}");
        for s in &starts {
            let bars = (s + 0.01 - d.origin()) / d.bar();
            assert!((bars - bars.round()).abs() < 1e-6, "{s} is off the grid");
        }
        let gaps: Vec<f64> = starts.windows(2).map(|w| ((w[1] - w[0]) / d.bar()).round()).collect();
        assert!(gaps.iter().all(|&g| g == 4.0), "{gaps:?}");
    }

    #[test]
    fn danger_drives_tension_heartbeat_and_release() {
        let mut d = director();
        d.start(0, 0.0);
        // Calm, then the heron closes in, then the escape.
        let mood = |t: f64| Mood { intensity: 0.3, danger: if t < 4.0 { 0.0 } else if t < 14.0 { ((t - 4.0) / 8.0).min(1.0) as f32 } else { 0.0 } };
        run(&mut d, 0.0, 20.0, mood);
        let vol_at = |name: &str, t: f64| -> f32 {
            events(&d, name, "set").iter().rfind(|e| e.0 <= t).map(|e| e.1.split_whitespace().nth(2).unwrap().parse().unwrap()).unwrap_or(0.0)
        };
        assert_eq!(vol_at("tension", 3.9), 0.0);
        assert!((vol_at("tension", 13.9) - 1.4).abs() < 0.01, "full danger: the tension layer level with the track");
        assert!(vol_at("tick", 13.9) > 0.9 && vol_at("tick", 16.0) < 0.01, "the clock stops on the escape");
        // Heartbeats on beats, faster with danger, never closer than 0.33 s.
        let hb: Vec<f64> = events(&d, "heartbeat", "").iter().map(|e| e.0).collect();
        assert!(hb.len() >= 6, "{hb:?}");
        for t in &hb {
            let beats = (t - d.origin()) / d.beat();
            assert!((beats - beats.round()).abs() < 1e-6);
        }
        assert!(hb.windows(2).all(|w| w[1] - w[0] >= 0.33 - 1e-9));
        assert!(hb.iter().all(|&t| t < 15.0), "none after the escape");
        // The low-pass narrows and opens again; the high layer swells with relief after the escape.
        assert!(d.out.lines.iter().any(|l| l.starts_with("# bus") && l.contains("lowpass 25")));
        assert!(d.out.lines.iter().rev().find(|l| l.contains("lowpass")).unwrap().contains("lowpass 20000"));
        assert!(vol_at("a_hi", 15.5) > vol_at("a_hi", 3.0) + 0.15, "relief");
        // A near-miss: the sting, a dip and back.
        d.near_miss(20.0);
        run(&mut d, 20.0, 21.0, mood);
        assert_eq!(events(&d, "sting", "").len(), 1);
        assert!(d.out.lines.iter().any(|l| l.contains("volume 0.600")) && d.out.lines.iter().any(|l| l.contains("volume 1.000")));
    }

    // --- a music graph -------------------------------------------------------------

    /// Three zones of 12 bars (2 s each, bar 0 at 0.1 s): a has links back and one forward and a
    /// seamless link into b; b one link back; c none. b and c have stingers; the swell; the tension kit.
    const GRAPH: &str = r#"{
      "format": "maimbrain-music-graph", "version": 1, "name": "run",
      "files": { "base": "assets/run_take_base.ogg", "hi": "assets/run_take_hi.ogg",
                 "swell": { "file": "assets/run_swell.ogg", "land_seconds": 0.5 },
                 "tension": { "layer": "assets/run_tension.ogg", "tick": "assets/run_tick.ogg", "heartbeat": "assets/run_heartbeat.ogg",
                              "sting": "assets/run_sting.ogg", "bars": 8, "downbeat_seconds": 0.01 } },
      "grid": { "bpm": 120.0, "beats_per_bar": 4, "bar_seconds": 2.0, "first_bar_seconds": 0.1, "phrase_bars": 4, "first_bar": 0, "end_bar": 36 },
      "splice": { "link_pre_beats": 0.25, "link_post_beats": 1.0, "seamless": 0.8, "min_repeat_bars": 8 },
      "zones": [
        { "name": "town", "sections": [0], "link_threshold": 0.85 },
        { "name": "grove", "sections": [1], "link_threshold": 0.85, "stinger": { "file": "assets/run_into_grove.ogg", "land_seconds": 1.0 } },
        { "name": "marsh", "sections": [2], "link_threshold": 0.85, "stinger": { "file": "assets/run_into_marsh.ogg", "land_seconds": 1.0 } }
      ],
      "sections": [
        { "name": "town", "zone": 0, "entry_bar": 0, "exit_bar": 12, "exit_cues": [4, 8, 12],
          "segment": { "pre_entry_seconds": 0.0, "entry_seconds": 0.1, "exit_seconds": 24.1, "post_exit_seconds": 26.1 } },
        { "name": "grove", "zone": 1, "entry_bar": 12, "exit_bar": 24, "exit_cues": [16, 20, 24],
          "segment": { "pre_entry_seconds": 23.6, "entry_seconds": 24.1, "exit_seconds": 48.1, "post_exit_seconds": 50.1 } },
        { "name": "marsh", "zone": 2, "entry_bar": 24, "exit_bar": 36, "exit_cues": [28, 32, 36],
          "segment": { "pre_entry_seconds": 47.6, "entry_seconds": 48.1, "exit_seconds": 72.1, "post_exit_seconds": 74.1 } }
      ],
      "links": [
        { "from": 9, "to": 1, "score": 0.9 }, { "from": 11, "to": 3, "score": 0.92 }, { "from": 6, "to": 9, "score": 0.88 },
        { "from": 10, "to": 9, "score": 0.99 }, { "from": 5, "to": 21, "score": 0.86, "into_zone": 1 },
        { "from": 22, "to": 13, "score": 0.9 }
      ]
    }"#;

    const FILES: [&str; 10] = [
        "assets/run_take_base.ogg",
        "assets/run_take_hi.ogg",
        "assets/run_swell.ogg",
        "assets/run_tension.ogg",
        "assets/run_tick.ogg",
        "assets/run_heartbeat.ogg",
        "assets/run_sting.ogg",
        "assets/run_into_grove.ogg",
        "assets/run_into_marsh.ogg",
        "unused",
    ];

    fn graph_director(seed: u64) -> Director<Log> {
        let g = Graph::parse(GRAPH.as_bytes()).unwrap();
        assert_eq!(g.files().len(), 9);
        let mut log = Log::default();
        for (i, f) in FILES.iter().enumerate() {
            log.name(Sound(i as u32 + 1), f.trim_start_matches("assets/").trim_end_matches(".ogg"));
        }
        let sound = |p: &str| FILES.iter().position(|f| *f == p).map(|i| Sound(i as u32 + 1));
        Director::with_graph(g, sound, seed, log).unwrap()
    }

    /// The take's bar heard from each bar line, from the moves: (game time, bar).
    fn bars_heard(d: &Director<Log>, start_bar: u32, to: f64) -> Vec<(f64, u32)> {
        let mut v = Vec::new();
        let (mut t, mut b) = (d.origin(), start_bar);
        while t < to {
            if let Some(m) = d.moves().iter().find(|m| (m.at - t).abs() < 1e-6) {
                assert_eq!(m.from, b, "move at {t}");
                b = m.to;
            }
            v.push((t, b));
            t += 2.0;
            b += 1;
        }
        v
    }

    #[test]
    fn a_graph_plays_the_take_and_jumps_on_bar_lines_without_short_repeats() {
        let mut d = graph_director(7);
        d.start(d.zone("town").unwrap(), 0.0);
        run(&mut d, 0.0, 120.0, |_| Mood::default());
        let heard = bars_heard(&d, 0, 118.0);
        // It never leaves the town, and it jumped (the links are good and it stayed a minute).
        assert!(heard.iter().all(|h| h.1 < 12), "{heard:?}");
        let jumps: Vec<&Move> = d.moves().iter().collect();
        assert!(jumps.len() >= 3, "{jumps:?}");
        for m in &jumps {
            assert!(matches!(m.kind, MoveKind::Link(s) if s >= 0.85) || m.kind == MoveKind::Restart, "{m:?}");
            assert!(m.to > m.from || m.from - m.to >= 8, "a short repeat: {m:?}");
            assert!(((m.at - d.origin()) / 2.0 - ((m.at - d.origin()) / 2.0).round()).abs() < 1e-9, "on a bar line");
        }
        // Each jump: the base and the high layer start a quarter beat early from the take's bar
        // (fading in), and the voices before die away over a beat from the bar line.
        for m in jumps.iter().filter(|m| matches!(m.kind, MoveKind::Link(_))) {
            let at = format!("{:.3} run_take_base", m.at - 0.125);
            let l = d.out.lines.iter().find(|l| l.starts_with(&at) && op(l) == "").unwrap_or_else(|| panic!("no take at {at}: {:?}", d.out.lines));
            assert!((field(l, "from").unwrap() - (0.1 + m.to as f64 * 2.0 - 0.125)).abs() < 1e-3 && field(l, "fade").unwrap() == 0.125, "{l}");
            assert!(d.out.lines.iter().any(|l| l.starts_with(&format!("{:.3} run_take_hi", m.at - 0.125))));
            let stops = d.out.lines.iter().filter(|l| l.starts_with(&format!("{:.3} run_take_base 0 1 stop fade=0.500", m.at))).count();
            assert_eq!(stops, 1, "{m:?}");
        }
        // The same seed, the same music; another seed, other jumps.
        let mut again = graph_director(7);
        again.start(0, 0.0);
        run(&mut again, 0.0, 120.0, |_| Mood::default());
        assert_eq!(again.out.lines, d.out.lines);
        let mut other = graph_director(8);
        other.start(0, 0.0);
        run(&mut other, 0.0, 120.0, |_| Mood::default());
        assert_ne!(other.moves(), d.moves());
    }

    #[test]
    fn a_zone_change_waits_for_an_exit_cue_unless_a_seamless_link_or_the_take_gets_there_first() {
        // Into the grove, asked for after the cue at bar 4 was decided: the town's link from bar 5
        // into it is seamless, and comes before the next cue (8).
        let mut d = graph_director(1);
        d.wander.chance_max = 0.0; // no jumps of its own
        d.wander.chance = 0.0;
        d.wander.wait_bars = 0; // and it waits for the cue however far
        d.start(0, 0.0);
        run(&mut d, 0.0, 7.5, |_| Mood::default());
        d.go(d.zone("grove").unwrap(), Change::Auto, 7.5);
        assert_eq!(d.current(), 1);
        run(&mut d, 7.5, 20.0, |_| Mood::default());
        assert_eq!(d.moves()[0].kind, MoveKind::Into(0.86));
        assert_eq!((d.moves()[0].from, d.moves()[0].to), (5, 21));
        assert!(events(&d, "run_into_grove", "").is_empty(), "no stinger for a seamless link");
        // Into the marsh: no link, so on the grove's next exit cue under the marsh's stinger, its
        // pre-entry (a beat) coming in under the grove's post-exit (a bar).
        d.go(d.zone("marsh").unwrap(), Change::Auto, 20.0);
        run(&mut d, 20.0, 40.0, |_| Mood::default());
        let m = *d.moves().iter().find(|m| matches!(m.kind, MoveKind::Change { .. })).unwrap();
        assert_eq!(m.kind, MoveKind::Change { stinger: true });
        assert_eq!(m.to, 24);
        assert!([16, 20, 24].contains(&m.from), "{m:?}");
        let st = events(&d, "run_into_marsh", "");
        assert_eq!(st.len(), 1);
        assert!((st[0].0 - (m.at - 1.0)).abs() < 1e-3, "the hit lands on the cue");
        assert!(d.out.lines.iter().any(|l| l.starts_with(&format!("{:.3} run_take_base", m.at - 0.5)) && (field(l, "from").unwrap() - (48.1 - 0.5)).abs() < 1e-3));
        assert!(d.out.lines.iter().any(|l| l.starts_with(&format!("{:.3} run_take_base 0 1 stop fade=2.000", m.at))));
        // The marsh has no links: at its end, it starts again from its entry under the swell.
        run(&mut d, 40.0, 70.0, |_| Mood::default());
        let r = d.moves().iter().find(|m| m.kind == MoveKind::Restart).unwrap();
        assert_eq!((r.from, r.to), (36, 24));
        assert!(events(&d, "run_swell", "").iter().any(|e| (e.0 - (r.at - 0.5)).abs() < 1e-3));
        // Urgent: the very next bar line it can make.
        d.go(d.zone("town").unwrap(), Change::NextBar, 70.0);
        run(&mut d, 70.0, 74.0, |_| Mood::default());
        let u = *d.moves().last().unwrap();
        assert_eq!((u.to, u.kind), (0, MoveKind::Change { stinger: false }));
        assert!(u.at - 70.0 <= 2.0 + 0.62 + 1e-6, "{u:?}");
    }

    #[test]
    fn a_change_waits_a_bar_at_most_by_default() {
        let mut d = graph_director(1);
        d.wander.chance_max = 0.0;
        d.start(d.zone("marsh").unwrap(), 0.0);
        run(&mut d, 0.0, 9.0, |_| Mood::default());
        // The marsh's cues are at bars 28, 32, 36; asked for at 9 s, the next cue is bar 32 (about 16 s).
        d.go(d.zone("town").unwrap(), Change::Auto, 9.0);
        run(&mut d, 9.0, 20.0, |_| Mood::default());
        let m = d.moves()[0];
        assert_eq!(m.kind, MoveKind::Change { stinger: false });
        assert!(m.at > 9.0 + 1.14 && m.at <= 9.0 + 1.14 + 2.0 * 2.0, "{m:?}");
        assert!(![28, 32, 36].contains(&m.from), "not on a cue: {m:?}");
    }

    #[test]
    fn the_tension_kit_follows_the_takes_phrase() {
        let mut d = graph_director(3);
        d.start(0, 0.0);
        let danger = |t: f64| Mood { intensity: 0.0, danger: if t > 2.0 { 1.0 } else { 0.0 } };
        run(&mut d, 0.0, 60.0, danger);
        // After a jump to bar k the kit is at bar k mod 8 of the town (from its entry, bar 0).
        let jumps: Vec<Move> = d.moves().iter().filter(|m| (m.from as i64 - m.to as i64).rem_euclid(8) != 0).copied().collect();
        assert!(!jumps.is_empty());
        for m in jumps {
            let x = 0.125; // a quarter beat
            let l = d.out.lines.iter().find(|l| l.starts_with(&format!("{:.3} run_tension", m.at - x))).unwrap_or_else(|| panic!("{m:?}"));
            let want = 0.01 + (m.to % 8) as f64 * 2.0 - x;
            assert!((field(l, "from").unwrap() - want.rem_euclid(16.0)).abs() < 1e-3, "{l} for {m:?}");
            assert!(field(l, "fade").unwrap() > 0.0 && l.split_whitespace().nth(2).unwrap().parse::<f32>().unwrap() > 1.0, "at the danger's level: {l}");
        }
        // The heartbeat is on the game's beat grid throughout.
        for (t, _) in events(&d, "run_heartbeat", "") {
            let beats = (t - d.origin()) / 0.5;
            assert!((beats - beats.round()).abs() < 1e-6);
        }
    }

    /// Voices started (with their sound's name and start time) that no stop line ends.
    fn unstopped(d: &Director<Log>) -> Vec<(u32, String, f64)> {
        let id = |l: &str| field(l, "id").map(|v| v as u32);
        let stopped: Vec<u32> = d.out.lines.iter().filter(|l| op(l) == "stop").filter_map(|l| id(l)).collect();
        d.out
            .lines
            .iter()
            .filter(|l| !l.starts_with('#') && matches!(op(l), "" | "loop"))
            .filter_map(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                Some((id(l)?, w[1].to_string(), w[0].parse().ok()?))
            })
            .filter(|v| !stopped.contains(&v.0))
            .collect()
    }

    #[test]
    fn stop_lets_every_voice_go_and_restart_starts_one_set_again() {
        let mut d = graph_director(5);
        d.start(0, 0.0);
        let chase = |t: f64| Mood { intensity: 0.5, danger: if t > 3.0 { 1.0 } else { 0.0 } };
        run(&mut d, 0.0, 30.0, chase);
        // A change under a stinger, decided but not yet heard, and a near-catch.
        d.go(d.zone("marsh").unwrap(), Change::NextBar, 30.0);
        run(&mut d, 30.0, 31.4, chase);
        d.near_miss(31.4);
        assert!(!unstopped(&d).is_empty());
        let n = d.out.lines.len();
        d.stop(31.5, 0.4);
        assert!(!d.playing());
        // Everything it started is let go at 31.5 over 0.4 s: the take, the kit, the stinger, the sting.
        assert!(unstopped(&d).iter().all(|v| v.2 < 31.5 - SHOT_KEEP + 1.0), "{:?}", unstopped(&d));
        let stops: Vec<&String> = d.out.lines[n..].iter().filter(|l| op(l) == "stop").collect();
        assert!(stops.iter().all(|l| l.starts_with("31.500 ") && field(l, "fade") == Some(0.4)), "{stops:?}");
        for name in ["run_take_base", "run_take_hi", "run_tension", "run_tick", "run_into_marsh", "run_sting"] {
            assert!(stops.iter().any(|l| l.split_whitespace().nth(1) == Some(name)), "{name} not stopped: {stops:?}");
        }
        // The bus opens again (the danger had narrowed it, the sting ducked it).
        assert!(d.out.lines[n..].iter().any(|l| l.contains("lowpass 20000")) && d.out.lines[n..].iter().any(|l| l.contains("volume 1.000")));
        // Stopped: nothing more happens, whatever the game asks for; stopping again adds nothing.
        let n = d.out.lines.len();
        run(&mut d, 31.5, 45.0, chase);
        d.go(d.zone("town").unwrap(), Change::Auto, 45.0);
        d.near_miss(45.0);
        d.stop(45.0, 1.0);
        assert_eq!(d.out.lines.len(), n, "{:?}", &d.out.lines[n..]);
        // Restarted in the grove: one take voice (and its high layer) and one kit, from the grove's entry.
        d.restart(d.zone("grove").unwrap(), 50.0);
        assert!(d.playing());
        run(&mut d, 50.0, 60.0, chase);
        let live = unstopped(&d);
        let count = |name: &str| live.iter().filter(|v| v.1 == name && v.2 >= 50.0).count();
        assert_eq!((count("run_take_base"), count("run_take_hi"), count("run_tension"), count("run_tick")), (1, 1, 1, 1), "{live:?}");
        let take = d.out.lines.iter().find(|l| l.starts_with("50.") && l.contains("run_take_base")).unwrap();
        assert!((field(take, "from").unwrap() - (24.1 - 0.5)).abs() < 1e-3, "{take}");
    }

    #[test]
    fn restart_without_a_stop_never_plays_two_sets() {
        let mut d = graph_director(9);
        d.start(0, 0.0);
        run(&mut d, 0.0, 20.0, |_| Mood { intensity: 0.0, danger: 0.8 });
        let before: Vec<u32> = unstopped(&d).iter().filter(|v| v.2 > 20.0 - SHOT_KEEP).map(|v| v.0).collect();
        assert!(before.len() >= 4);
        d.restart(d.zone("marsh").unwrap(), 20.0);
        // Each voice still sounding is let go at 20 s over RESTART_FADE.
        for id in before {
            let l = d.out.lines.iter().find(|l| op(l) == "stop" && field(l, "id") == Some(id as f64)).unwrap();
            assert!(l.starts_with("20.000 ") && field(l, "fade") == Some(RESTART_FADE as f64), "{l}");
        }
        run(&mut d, 20.0, 40.0, |_| Mood { intensity: 0.0, danger: 0.8 });
        let live: Vec<String> = unstopped(&d).into_iter().filter(|v| v.1 != "run_heartbeat").map(|v| v.1).collect();
        assert_eq!(live.iter().filter(|n| *n == "run_take_base").count(), 1, "{live:?}");
        assert_eq!(live.iter().filter(|n| *n == "run_tension").count(), 1, "{live:?}");
    }

    #[test]
    fn stop_works_for_loops_too() {
        let mut d = director();
        d.start(0, 0.0);
        run(&mut d, 0.0, 12.0, |_| Mood { intensity: 0.5, danger: 1.0 });
        d.go(1, Change::Stinger, 12.0);
        d.stop(12.5, 0.0);
        assert!(unstopped(&d).iter().all(|v| v.2 < 12.5 - SHOT_KEEP + 1.0), "{:?}", unstopped(&d));
        assert!(d.out.lines.iter().rev().find(|l| l.contains("lowpass")).unwrap().contains("lowpass 20000"));
        let n = d.out.lines.len();
        run(&mut d, 12.5, 20.0, |_| Mood { intensity: 0.5, danger: 1.0 });
        assert_eq!(d.out.lines.len(), n);
        d.restart(1, 20.0);
        run(&mut d, 20.0, 22.0, |_| Mood::default());
        let live = unstopped(&d);
        assert_eq!(live.iter().filter(|v| v.1 == "b" || v.1 == "b_hi").count(), 2, "{live:?}");
        assert!(live.iter().all(|v| v.1 != "a" && v.1 != "a_hi"));
    }

    #[test]
    fn a_graphs_tension_gain_raises_the_kit() {
        let d = graph_director(1);
        assert_eq!(d.levels.tension_max, Levels::default().tension_max, "a graph without levels: the defaults");
        let g = Graph::parse(GRAPH.replace(r#""name": "run","#, r#""name": "run", "levels": { "tension_gain": 2.0 },"#).as_bytes()).unwrap();
        assert_eq!(g.tension_gain, 2.0);
        let l = Levels::for_graph(&g);
        assert!((l.tension_max - 2.8).abs() < 1e-6 && (l.tick_max - 2.0).abs() < 1e-6 && (l.heartbeat.0 - 0.3).abs() < 1e-6 && (l.heartbeat.1 - 0.6).abs() < 1e-6);
        assert_eq!((l.hi_floor, l.stinger, l.narrow_hz), (Levels::default().hi_floor, Levels::default().stinger, Levels::default().narrow_hz));
        // Never past the platform's vol 4.
        assert_eq!(Levels::default().with_tension_gain(9.0).tension_max, MAX_VOL);
        // The Director built from the graph plays at those levels.
        let d = Director::with_graph(g, |p| FILES.iter().position(|f| *f == p).map(|i| Sound(i as u32 + 1)), 1, Log::default()).unwrap();
        assert!((d.levels.tension_max - 2.8).abs() < 1e-6);
    }

    #[test]
    fn graphs_are_checked_when_read() {
        assert!(Graph::parse(b"{}").is_err());
        assert!(Graph::parse(GRAPH.replace("\"version\": 1", "\"version\": 2").as_bytes()).is_err());
        assert!(Graph::parse(GRAPH.replace("\"zone\": 2,", "\"zone\": 7,").as_bytes()).is_err());
        let g = Graph::parse(GRAPH.as_bytes()).unwrap();
        assert_eq!((g.zone("marsh"), g.zone_of(30), g.top(1), g.exit_cue(8), g.exit_cue(9)), (Some(2), Some(2), 12, true, false));
        assert!((g.sections[1].pre_entry - 0.5).abs() < 1e-9 && (g.sections[1].post_exit - 2.0).abs() < 1e-9);
        // Without its take, no Director.
        assert!(Director::with_graph(g, |_| None, 1, Log::default()).is_none());
    }
}
