//! `mb music` and `mb sfx`: generate sound and post-process it to the
//! platform's audio rules (SPEC §5.6, the skill's sfx.py and check_audio.py):
//! mono Ogg Vorbis, loudness-normalized (music ≈ −20 dBFS RMS, effects
//! ≈ −14), peaks ≤ −1 dBFS decoded; music cut to a whole number of bars with
//! a cross-faded seam so `play_looped` repeats it without a click.
//!
//!     mb music games/moth music "dreamy harp and glass chimes, gentle pulse" --bars 8
//!     mb music games/moth music "driving synthwave" --bpm 128 --layers   # music_base.ogg + music_hi.ogg
//!     mb music games/fish chase "tense surf rock" --layers base,hi,tension   # + chase_tension/_tick/_heartbeat/_sting
//!     mb music games/fish chase --reprocess --layers base,hi,tension         # the same, from the take you have (free)
//!     mb music games/fish run "taiko chase" --zones "town: lanterns; grove: bamboo" --bpm 120   # run_take, run.music.json (graph.rs), run_into_*
//!     mb music games/fish theme "synthwave" --variations 4                    # one take in 4 sections: one key, tempo and grid (score.rs)
//!     mb music games/fish run --audition                  # the graph's jumps and every change between zones, in one file to listen to
//!     mb music games/fish run --reprocess --loops         # also a loop per section that repeats on its own, as before the graph
//!     mb music games/fish run --reprocess --fix-loops     # with the loops, a Lyria 3 Clip for each that joins badly (~$0.04 each)
//!     mb music games/fish run --downbeat 1                # the piece's 1 a beat later, from the take you have
//!     mb music games/moth win "a short triumphant fanfare" --once
//!     mb sfx games/moth chime "a bright glass chime"
//!     mb sfx games/moth wind "soft wind" --loop --seconds 4
//!     mb sfx games/moth plunge "a deep plunge" --brighten   # bass's harmonics, for a phone speaker

use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::audio::{self, Kind};
use super::provider::{self, AudioRequest, Media};
use super::graph;
use super::score;
use super::style::Style;
use super::tension;
use super::{FileRef, Sidecar};

#[derive(Args, Clone, Default)]
pub struct Common {
    /// Seed (recorded; honored by Lyria and the mock).
    #[arg(long)]
    pub seed: Option<u64>,
    /// gemini (music default), vertex, elevenlabs (sfx default) or mock; maimbrain uses Maimbrain's keys
    /// (if your account has access).
    #[arg(long)]
    pub provider: Option<String>,
    /// Model id (see `mb models`).
    #[arg(long)]
    pub model: Option<String>,
    /// Only use your own API keys, never Maimbrain's (also MB_KEYS_SOURCE=own).
    #[arg(long)]
    pub own_keys: bool,
    /// Output path relative to the game (default assets/<name>.ogg).
    #[arg(long)]
    pub out: Option<String>,
    /// Sample rate of the Ogg file (22050–48000).
    #[arg(long, default_value_t = 44100)]
    pub rate: u32,
    /// Vorbis quality (0–10, as oggenc -q; lower is smaller).
    #[arg(long, default_value_t = 4.0)]
    pub quality: f32,
    /// Ignore art/style.toml's [music]/[sfx] prompt.
    #[arg(long)]
    pub no_style: bool,
    /// Let a flagged name through (it's yours, or a false positive). Recorded in the sidecar.
    #[arg(long)]
    pub allow_name: Vec<String>,
    /// Refuse if the estimated cost is above this many USD (default 1.00, or MB_MAX_COST).
    #[arg(long)]
    pub max_cost: Option<f64>,
    /// Print the prompt and the cost without calling the provider.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Clone)]
pub struct MusicArgs {
    /// Game directory.
    pub game: PathBuf,
    /// Asset name: writes assets/<name>.ogg.
    pub name: String,
    /// Genre, instruments, mood, energy. Original: no "in the style of <artist>". Not needed with
    /// --reprocess, --downbeat or --audition.
    /// With --zone, optional: added to every zone's prompt (what they share, e.g. "for a mech-duel game").
    #[arg(required_unless_present_any = ["reprocess", "downbeat", "audition", "zone"])]
    pub prompt: Option<String>,
    /// A different song for a zone, on purpose: NAME="genre, instruments, mood, tempo" (repeatable),
    /// e.g. --zone harbor="bluesy hard rock, 128 bpm" --zone neon="flamenco-tango, 112 bpm". Each is its
    /// own piece in its own key and tempo (a "N bpm" in its prompt is asked for), with its own music
    /// graph (<name>_<zone>.music.json), stinger, swell and tension kit; <name>.songs.json lists them for
    /// music::Songs, which changes song at an exit cue under the new song's stinger. One call each.
    /// A run adds to <name>.songs.json (a zone made again replaces its entry), so one --zone is fine;
    /// a zone that fails is reported with a command to make it again, and the others still generate.
    #[arg(long = "zone", value_name = "NAME=PROMPT", conflicts_with_all = ["zones", "sections", "variations", "once", "seconds", "reprocess", "downbeat", "key"])]
    pub zone: Vec<String>,
    /// Loop length in bars of 4/4 (default 8, fewer if it doesn't fit). Past ~26 s of music,
    /// gemini uses lyria-3.5 (Lyria 3 Clip is always 30 s).
    #[arg(long)]
    pub bars: Option<u32>,
    /// N loops of one piece that share its key, tempo and bar grid (<name>_v1 … <name>_vN), to
    /// rotate between so the music doesn't repeat. One request; every loop has its downbeat 10 ms in.
    #[arg(long, conflicts_with_all = ["sections", "once", "seconds"])]
    pub variations: Option<u32>,
    /// A form, e.g. a,b,a,bridge: one loop per distinct section (<name>_a, <name>_b, <name>_bridge)
    /// in one key, tempo and bar grid, and a transition stinger into each (<name>_into_b …).
    #[arg(long, conflicts_with_all = ["zones", "once", "seconds"])]
    pub sections: Option<String>,
    /// A theme per zone: "lantern town: warm lanterns, a slow pulse; bamboo grove: hollow wood…"
    /// (or @file, one "name: description" per line). One loop each (<name>_lantern_town …) in one
    /// key, tempo and bar grid, plus a stinger into each that lands on the bar (<name>_into_bamboo_grove).
    /// With --variations K, K loops per zone (<name>_lantern_town_1 …).
    #[arg(long, conflicts_with_all = ["once", "seconds"])]
    pub zones: Option<String>,
    /// For --variations/--sections/--zones: ask again (up to N more times) when a section can't be
    /// corrected to the piece's key and tempo; the take with the fewest mismatches is kept.
    #[arg(long, default_value_t = 1)]
    pub retries: u32,
    /// Redo the post-processing from the take you already have (art/raw/; no provider call, free),
    /// with the options given now: e.g. `--reprocess --layers base,hi,tension` adds the tension kit.
    #[arg(long)]
    pub reprocess: bool,
    /// The crossfade baked into every loop, in bars: the music that followed the loop's end
    /// fades into its start (equal power), so the file repeats seamlessly at its exact length.
    #[arg(long, value_name = "BARS")]
    pub loop_xfade: Option<f32>,
    /// For --variations/--sections/--zones: a section whose loop joins worse than this (default
    /// 0.85; 1 for every section) gets a loop of its own, one Lyria 3 Clip from its prompt at the piece's tempo and key
    /// (about $0.04 each), corrected like a section and kept if it joins better. Costs money: off
    /// unless given. The clips stay in art/raw/ and --reprocess reuses them.
    #[arg(long, num_args = 0..=1, default_missing_value = "0.85", value_name = "BELOW")]
    pub fix_loops: Option<f32>,
    /// For --variations/--sections/--zones: move the piece's downbeat by N beats (e.g. 1, -1, 2)
    /// when the loops' 1 is audibly on another beat; works from the take you have (no provider call).
    #[arg(long, value_name = "BEATS", allow_hyphen_values = true)]
    pub downbeat: Option<f32>,
    /// For --variations/--sections/--zones: also cut a loop of each section that repeats on its own
    /// (<name>_<section>.ogg, the way before the music graph), for music::Director's loop mode.
    /// The graph (<name>.music.json and the take) is written either way.
    #[arg(long)]
    pub loops: bool,
    /// For --variations/--sections/--zones: render the music graph's jumps and every change between
    /// zones the Director makes (and with --loops each loop's join) into one file to listen to (a
    /// soft click before each), default art/raw/<name>-audition.ogg.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "PATH")]
    pub audition: Option<String>,
    /// Tempo: asked for in the prompt and used instead of detecting it.
    #[arg(long)]
    pub bpm: Option<f32>,
    /// Loop length in seconds, ignoring the beat (ambient pieces without a steady pulse).
    #[arg(long)]
    pub seconds: Option<f32>,
    /// Also write layers. `--layers` (= base,hi): <name>_base.ogg (low-passed, calm) and
    /// <name>_hi.ogg (the rest); start both together and raise the high layer with intensity.
    /// Add `tension` for dread as a threat closes in: <name>_tension.ogg and <name>_tick.ogg
    /// (loops on the track's beat grid, in its key) and <name>_heartbeat.ogg and <name>_sting.ogg
    /// (one-shots). `--layers tension` keeps the full mix and adds those.
    #[arg(long, num_args = 0..=1, default_missing_value = "base,hi", value_name = "LIST")]
    pub layers: Option<String>,
    /// The track's key for the tension layers ("A minor", "F#m", "Eb"), instead of measuring it.
    #[arg(long)]
    pub key: Option<String>,
    /// Crossover frequency for --layers, Hz.
    #[arg(long, default_value_t = 700.0)]
    pub split_hz: f32,
    /// A one-shot (jingle, sting, game-over tune) instead of a loop.
    #[arg(long)]
    pub once: bool,
    /// What to keep out (Vertex lyria-002 only).
    #[arg(long)]
    pub negative: Option<String>,
    #[command(flatten)]
    pub common: Common,
}

#[derive(Args, Clone)]
pub struct SfxArgs {
    /// Game directory.
    pub game: PathBuf,
    /// Asset name: writes assets/<name>.ogg.
    #[arg(required_unless_present = "pack")]
    pub name: Option<String>,
    /// What it sounds like: source, material, action, length, character.
    #[arg(required_unless_present = "pack")]
    pub prompt: Option<String>,
    /// Length in seconds (0.5–30 for ElevenLabs; default: the model decides).
    #[arg(long)]
    pub seconds: Option<f32>,
    /// Make it loop seamlessly (engines, wind, ambience).
    #[arg(long = "loop")]
    pub looping: bool,
    /// If a phone speaker would lose more than 10 dB of it (bass it can't play), add the bass's
    /// harmonics, which it can ("virtual bass"). The raw take stays in art/raw/.
    #[arg(long)]
    pub brighten: bool,
    /// A recipe set instead of one sound: `mech` makes a mech's (or any big machine's) foley,
    /// stomp, servo (loop), thruster (loop), hiss, impact and tear, as assets/<prefix><sound>.ogg,
    /// for the SDK's `foley` module. About $0.01 in all on ElevenLabs.
    #[arg(long, value_parser = ["mech"], conflicts_with_all = ["name", "prompt", "seconds", "looping", "brighten"])]
    pub pack: Option<String>,
    /// For --pack: the file names' prefix (default `<pack>_`, e.g. mech_stomp).
    #[arg(long, requires = "pack")]
    pub prefix: Option<String>,
    /// For --pack: what it's made of, added to every prompt (e.g. "a rusty junkyard robot", "a sleek chrome racer").
    #[arg(long, requires = "pack")]
    pub flavor: Option<String>,
    #[command(flatten)]
    pub common: Common,
}

/// A sound in a recipe set (`mb sfx --pack`): its name, prompt, seconds, whether it loops and is brightened.
pub struct Recipe {
    pub sound: &'static str,
    pub prompt: &'static str,
    pub seconds: f32,
    pub looping: bool,
    pub brighten: bool,
}

/// `mb sfx --pack mech`: what a walking, flying, fighting machine needs. The
/// prompts ask for a bright attack so a phone hears the weight.
pub const MECH_PACK: &[Recipe] = &[
    Recipe {
        sound: "stomp",
        prompt: "a giant metal robot foot stomping onto concrete: a heavy thud with a sharp metal clank on top and a little grit, starts immediately, no echo",
        seconds: 0.6,
        looping: false,
        brighten: true,
    },
    Recipe {
        sound: "servo",
        prompt: "a robot joint's servo motor running at a steady speed: a smooth electric whine with a light gear buzz, constant pitch, no start and no stop",
        seconds: 2.0,
        looping: true,
        brighten: false,
    },
    Recipe {
        sound: "thruster",
        prompt: "a jet thruster burning steadily: a roaring rush of air over a low rumble, constant, no start and no stop",
        seconds: 2.0,
        looping: true,
        brighten: true,
    },
    Recipe {
        sound: "hiss",
        prompt: "a short hydraulic hiss: pressurized air venting from a robot's piston, a quick puff then a fade, soft, not shrill",
        seconds: 0.6,
        looping: false,
        brighten: false,
    },
    Recipe {
        sound: "impact",
        prompt: "a heavy metal punch hitting a steel robot body: a loud clang with a sharp crack and a short ring, starts immediately",
        seconds: 0.8,
        looping: false,
        brighten: true,
    },
    Recipe {
        sound: "tear",
        prompt: "a robot arm being ripped off: tearing sheet metal, snapping cables and a short spray of sparks",
        seconds: 1.0,
        looping: false,
        brighten: false,
    },
];

/// The recipes of a pack `mb sfx --pack` knows.
pub fn pack(name: &str) -> Option<&'static [Recipe]> {
    (name == "mech").then_some(MECH_PACK)
}

/// The resolved recipe for a sound, stored in the sidecar.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SoundJob {
    pub kind: String,
    pub name: String,
    pub subject: String,
    pub out: String,
    pub provider: String,
    pub model: String,
    pub seed: u64,
    #[serde(default)]
    pub bars: Option<u32>,
    #[serde(default)]
    pub bpm: Option<f32>,
    #[serde(default)]
    pub seconds: Option<f32>,
    #[serde(default)]
    pub layers: bool,
    /// The tension kit (`--layers …,tension`).
    #[serde(default)]
    pub tension: bool,
    /// The key the tension kit is in, if given (else it's measured).
    #[serde(default)]
    pub key: Option<String>,
    /// Effects: add the bass's harmonics for a phone speaker (`--brighten`).
    #[serde(default)]
    pub brighten: bool,
    #[serde(default = "split")]
    pub split_hz: f32,
    #[serde(default)]
    pub once: bool,
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub negative: Option<String>,
    /// Music in sections (`--variations`, `--sections`, `--zones`): the form, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<score::Section>,
    /// Write a transition stinger into each section (`--sections`, `--zones`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stingers: bool,
    /// More takes to ask for when a section doesn't fit the piece.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retries: u32,
    /// Seconds per section in the piece asked for (where to find them again on --reprocess).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_seconds: Option<f32>,
    /// The crossfade baked into each loop, bars (`--loop-xfade`; default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_xfade_bars: Option<f32>,
    /// Also cut self-looping loops per section (`--loops`); the graph is written either way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loops: bool,
    /// Beats the piece's downbeat was moved by (`--downbeat`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downbeat_shift: Option<f32>,
    /// Sections with a loop of their own from a separate clip (`--fix-loops`): section → raw take.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub section_clips: std::collections::BTreeMap<String, String>,
    /// `--fix-loops`: ask for a clip for sections that join worse than this (this run only).
    #[serde(skip)]
    pub fix_below: Option<f32>,
    pub rate: u32,
    pub quality: f32,
    #[serde(default)]
    pub no_style: bool,
    #[serde(default)]
    pub allow_names: Vec<String>,
}

fn split() -> f32 {
    700.0
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn resolve(kind: &str, game: &Path, name: &str, subject: &str, c: &Common, style: &Style) -> Result<SoundJob, String> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(format!("bad asset name {name:?}"));
    }
    if !(22050..=48000).contains(&c.rate) {
        return Err("--rate must be 22050–48000 (SPEC §5.6)".into());
    }
    let _ = game;
    let media = if kind == "music" { Media::Music } else { Media::Sfx };
    let order: &[&str] = if kind == "music" { &["gemini", "vertex", "elevenlabs"] } else { &["elevenlabs"] };
    let style_provider = style.provider.as_deref().filter(|p| provider::default_model(p, media).is_some());
    let flag = super::maimbrain::take_flags(c.provider.as_deref(), c.own_keys)?;
    let chosen = provider::choose(flag.as_deref(), style_provider, order, if kind == "music" { "music" } else { "sound effects" });
    let chosen = if chosen.is_err() && c.dry_run { Ok(order[0].to_string()) } else { chosen };
    let provider = chosen.map_err(|e| {
        if kind == "sfx" {
            format!(
                "{e}\n\nNo key? Effects are quick to make procedurally and cost nothing: the maimbrain-game skill's scripts/sfx.py \
                 (8-bit voices, sweeps, noise, a step sequencer) writes Ogg files that already meet these rules."
            )
        } else {
            e
        }
    })?;
    let model = c
        .model
        .clone()
        .or_else(|| provider::default_model(&provider, media).map(|m| m.id.to_string()))
        .ok_or(format!("{provider} has no {kind} model (see `mb models`)"))?;
    Ok(SoundJob {
        kind: kind.into(),
        name: name.into(),
        subject: subject.into(),
        out: c.out.clone().unwrap_or_else(|| format!("assets/{name}.ogg")),
        provider,
        model,
        seed: c.seed.unwrap_or_else(super::fresh_seed),
        bars: None,
        bpm: None,
        seconds: None,
        layers: false,
        tension: false,
        key: None,
        brighten: false,
        split_hz: 700.0,
        once: false,
        looping: false,
        negative: None,
        sections: Vec::new(),
        stingers: false,
        retries: 0,
        section_seconds: None,
        loop_xfade_bars: None,
        downbeat_shift: None,
        loops: false,
        section_clips: Default::default(),
        fix_below: None,
        rate: c.rate,
        quality: c.quality,
        no_style: c.no_style,
        allow_names: c.allow_name.clone(),
    })
}

pub fn music(a: MusicArgs) -> Result<(), String> {
    if let Some(k) = &a.key {
        audio::Key::parse(k).ok_or(format!("--key {k:?}: give a key like \"A minor\", \"F#m\" or \"Eb\""))?;
    }
    if a.loop_xfade.is_some_and(|b| !(0.0..=4.0).contains(&b)) {
        return Err("--loop-xfade: 0–4 bars".into());
    }
    if a.fix_loops.is_some_and(|b| !(0.0..=1.0).contains(&b)) {
        return Err("--fix-loops: a loop match between 0 and 1 (default 0.85)".into());
    }
    if a.downbeat.is_some_and(|b| b.abs() > 8.0) {
        return Err("--downbeat: beats, between -8 and 8".into());
    }
    if a.reprocess || a.downbeat.is_some() {
        reprocess_music(&a)?;
        return audition_after(&a);
    }
    if !a.zone.is_empty() {
        return songs(&a);
    }
    if a.prompt.is_none() {
        // --audition alone: the files you have.
        return audition_after(&a);
    }
    let style = if a.common.no_style { Style::default() } else { Style::load(&a.game)?.0 };
    let mut job = resolve("music", &a.game, &a.name, a.prompt.as_deref().unwrap_or(""), &a.common, &style)?;
    job.bars = a.bars;
    job.bpm = a.bpm;
    job.seconds = a.seconds;
    (job.layers, job.tension) = parse_layers(a.layers.as_deref())?;
    if job.tension && a.once {
        return Err("tension layers loop with the track: drop --once".into());
    }
    job.key = a.key.clone();
    job.split_hz = a.split_hz;
    job.once = a.once;
    job.negative = a.negative.clone();
    (job.sections, job.stingers) = form(&a)?;
    job.retries = if job.sections.is_empty() { 0 } else { a.retries };
    job.loop_xfade_bars = a.loop_xfade;
    job.fix_below = a.fix_loops;
    job.loops = a.loops || a.fix_loops.is_some();
    if job.sections.is_empty() && (a.fix_loops.is_some() || a.audition.is_some() || a.loops) {
        return Err("--loops, --fix-loops and --audition are for music in sections (--zones, --sections, --variations)".into());
    }
    // Lyria 3 Clip is always 30 s: a longer piece needs Lyria 3.5.
    let long = !job.sections.is_empty() || job.bars.is_some_and(|b| b as f32 * 240.0 / job.bpm.unwrap_or(score::DEFAULT_BPM) > 26.0);
    if long && a.common.model.is_none() && job.provider == "gemini" && job.model == "lyria-3-clip-preview" {
        job.model = "lyria-3.5".into();
        eprintln!("  using lyria-3.5: Lyria 3 Clip is always 30 s, too short for this");
    }
    if long && ["lyria-3-clip-preview", "lyria-002"].contains(&job.model.as_str()) {
        let need = if job.sections.is_empty() {
            job.bars.unwrap_or(8) as f32 * 240.0 / job.bpm.unwrap_or(score::DEFAULT_BPM)
        } else {
            plan_of(&job).total
        };
        eprintln!("  warning: {} makes 30 s, and this needs about {need:.0} s; use --provider gemini (lyria-3.5)", job.model);
    }
    execute(&a.game, &job, a.common.dry_run, super::max_cost(a.common.max_cost), None)?;
    if a.common.dry_run {
        return Ok(());
    }
    audition_after(&a)
}

/// `--zone NAME=PROMPT`s: (zone, its prompt, the tempo it asks for).
pub fn parse_song_zones(specs: &[String]) -> Result<Vec<(String, String, Option<f32>)>, String> {
    let mut v: Vec<(String, String, Option<f32>)> = Vec::new();
    for s in specs {
        let (name, prompt) = s.split_once('=').ok_or(format!("--zone {s:?}: give NAME=PROMPT, e.g. harbor=\"bluesy hard rock, 128 bpm\""))?;
        let (name, prompt) = (score::slug(name), prompt.trim().trim_matches('"').trim().to_string());
        if name.is_empty() || prompt.is_empty() {
            return Err(format!("--zone {s:?}: a name and a prompt"));
        }
        if v.iter().any(|z| z.0 == name) {
            return Err(format!("--zone: {name} is named twice"));
        }
        let bpm = tempo_in(&prompt);
        v.push((name, prompt, bpm));
    }
    Ok(v)
}

/// A tempo a prompt asks for: "128 bpm", "112 BPM", "90bpm".
fn tempo_in(prompt: &str) -> Option<f32> {
    let low = prompt.to_ascii_lowercase();
    let i = low.find("bpm")?;
    let head = low[..i].trim_end();
    let digits: String = head.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
    digits.parse::<f32>().ok().filter(|b| (40.0..=240.0).contains(b))
}

/// `--zone`: a song per zone, each its own sectioned piece of one zone (its
/// graph, stinger, swell and tension kit), and `<name>.songs.json` listing them.
/// A zone that fails is reported and the rest still generate; the list merges
/// with the one already there (a zone made again replaces its entry), so
/// zones can be made one run at a time.
fn songs(a: &MusicArgs) -> Result<(), String> {
    let zones = parse_song_zones(&a.zone)?;
    let style = if a.common.no_style { Style::default() } else { Style::load(&a.game)?.0 };
    let (layers, _) = parse_layers(a.layers.as_deref())?;
    if a.common.out.is_some() {
        return Err("--out names one file; --zone writes assets/<name>_<zone>… for each zone".into());
    }
    let shared = a.prompt.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let mut list = Vec::new();
    let mut failed: Vec<(String, String, String)> = Vec::new();
    for (zone, prompt, bpm) in &zones {
        match song(a, &style, layers, zone, prompt, *bpm, shared) {
            Ok(asked) => list.push((zone.clone(), format!("{}_{zone}", a.name), prompt.clone(), asked)),
            Err(e) => {
                eprintln!("  {zone} failed: {e}");
                if zones.len() > 1 {
                    eprintln!("  going on with the other zones");
                }
                failed.push((zone.clone(), prompt.clone(), e));
            }
        }
    }
    if a.common.dry_run {
        return Ok(());
    }
    let assets = a.game.join("assets");
    let path = assets.join(format!("{}.songs.json", a.name));
    let mut summary = Vec::new();
    let mut made = Vec::new();
    for (zone, song, prompt, asked) in &list {
        let take = [format!("{song}_take.ogg"), format!("{song}_take_base.ogg")].into_iter().find_map(|f| super::read_sidecar(&assets.join(f)).ok());
        let facts = take.map(|sc| sc.result).unwrap_or_default();
        let (bpm, key) = (facts["bpm"].as_f64().map(|b| (b * 100.0).round() / 100.0), facts["key"].as_str().map(str::to_string));
        summary.push(format!("{zone}: {}{}", bpm.map(|b| format!("{b:.0} BPM")).unwrap_or("?".into()), key.as_deref().map(|k| format!(", {k}")).unwrap_or_default()));
        made.push(json!({ "zone": zone, "graph": format!("assets/{song}.music.json"), "prompt": prompt, "bpm_asked": asked, "bpm": bpm, "key": key }));
    }
    if !made.is_empty() {
        let before = match std::fs::read(&path) {
            Ok(b) => Some(serde_json::from_slice::<Value>(&b).map_err(|e| format!("{}: {e} (fix or move it; this run's songs are made)", path.display()))?),
            Err(_) => None,
        };
        let (songs, kept) = merge_setlist(before.as_ref().map(|v| &v["songs"]), made);
        let doc = json!({ "format": "maimbrain-songs", "version": 1, "name": a.name, "songs": songs });
        let mut bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!(
            "{} song{} made ({}){}, each in its own key and tempo on purpose; music::Songs changes between them at an exit cue under the next song's stinger: {}",
            list.len(),
            if list.len() == 1 { "" } else { "s" },
            summary.join("; "),
            if kept.is_empty() { String::new() } else { format!(", listed with the {} already there ({})", kept.len(), kept.join(", ")) },
            path.strip_prefix(&a.game).unwrap_or(&path).display()
        );
    }
    if a.audition.is_some() {
        for (_, song, _, _) in &list {
            super::audition::render(&assets, song, &a.game.join(format!("art/raw/{song}-audition.ogg")), a.common.quality)?;
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    Err(failure_report(a, &path, zones.len(), &failed, shared))
}

/// What `--zone` says about the zones that failed: why, which words likely
/// tripped a content filter, and the command that makes each again on its own.
fn failure_report(a: &MusicArgs, setlist: &Path, of: usize, failed: &[(String, String, String)], shared: Option<&str>) -> String {
    let mut msg = format!(
        "{} of {of} zone{} failed: {}",
        failed.len(),
        if of == 1 { "" } else { "s" },
        failed.iter().map(|f| f.0.as_str()).collect::<Vec<_>>().join(", ")
    );
    let flags = retry_flags(a);
    for (zone, prompt, e) in failed {
        msg.push_str(&format!("\n  {zone}: {e}"));
        let retry = match refusal_hint(e, prompt) {
            Some((words, softer)) => {
                msg.push_str(&format!(
                    "\n    the provider's filter refused the prompt; words that likely tripped it: {}",
                    words.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(", ")
                ));
                softer
            }
            None if is_refusal(e) => {
                msg.push_str("\n    the provider's filter refused the prompt; describe the music in plainer words (instruments, tempo, mood)");
                prompt.clone()
            }
            None => prompt.clone(),
        };
        msg.push_str(&format!(
            "\n    make it again on its own (it's added to {}): mb music {} {}{}{flags} --zone {zone}=\"{retry}\"",
            setlist.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
            a.game.display(),
            a.name,
            shared.map(|s| format!(" \"{s}\"")).unwrap_or_default()
        ));
    }
    msg
}

/// The flags of a `--zone` run worth repeating in a retry command.
fn retry_flags(a: &MusicArgs) -> String {
    let mut f = String::new();
    if let Some(p) = &a.common.provider {
        f.push_str(&format!(" --provider {p}"));
    }
    if let Some(m) = &a.common.model {
        f.push_str(&format!(" --model {m}"));
    }
    if let Some(b) = a.bars {
        f.push_str(&format!(" --bars {b}"));
    }
    if let Some(l) = &a.layers {
        f.push_str(&format!(" --layers {l}"));
    }
    f
}

/// One `--zone` song: its job, made, with a fallback when no loop can be cut
/// from the take (a longer music model, or fewer bars). Gives the tempo asked for.
fn song(a: &MusicArgs, style: &Style, layers: bool, zone: &str, prompt: &str, bpm: Option<f32>, shared: Option<&str>) -> Result<Option<f32>, String> {
    let song = format!("{}_{zone}", a.name);
    let subject = match shared {
        Some(shared) => format!("{prompt}. {shared}"),
        None => prompt.to_string(),
    };
    let mut job = resolve("music", &a.game, &song, &subject, &a.common, style)?;
    // A song plays on its own for a whole fight: 16 bars a section gives its graph room to wander.
    job.bars = Some(a.bars.unwrap_or(16));
    job.bpm = bpm.or(a.bpm);
    (job.layers, job.tension) = (layers, true);
    job.split_hz = a.split_hz;
    job.negative = a.negative.clone();
    job.sections = vec![score::Section { name: zone.to_string(), desc: prompt.to_string(), variation_of: None }];
    job.stingers = true;
    job.retries = a.retries;
    job.loop_xfade_bars = a.loop_xfade;
    job.fix_below = a.fix_loops;
    job.loops = a.loops || a.fix_loops.is_some();
    if a.common.model.is_none() && job.provider == "gemini" && job.model == "lyria-3-clip-preview" {
        job.model = "lyria-3.5".into();
    }
    eprintln!("{song} ({zone}{}):", job.bpm.map(|b| format!(", {b:.0} BPM")).unwrap_or_default());
    if a.bars.is_none()
        && let Some(fit) = clip_bars(&job)
    {
        eprintln!("  {} makes 30 s clips: {fit} bars a loop, since 16 don't fit in one", job.model);
        job.bars = Some(fit);
    }
    let limit = super::max_cost(a.common.max_cost);
    match execute(&a.game, &job, a.common.dry_run, limit, None) {
        Ok(()) => Ok(job.bpm),
        Err(e) if e.contains("could be cut into a loop") => {
            let Some(why) = loop_fallback(&mut job, a.common.model.is_some()) else {
                return Err(e);
            };
            eprintln!("  {e}; trying once more with {why}");
            execute(&a.game, &job, a.common.dry_run, limit, None).map(|()| job.bpm).map_err(|e2| format!("{e2} (also with {why})"))
        }
        Err(e) => Err(e),
    }
}

/// Music models that always make 30 s, whatever length is asked for.
fn fixed_clip(model: &str) -> bool {
    ["lyria-3-clip-preview", "lyria-002"].contains(&model)
}

/// The bars a loop can have when the model makes 30 s clips and the bars
/// asked for don't fit in one (score::plan's window is the loop and 3 bars);
/// None when they fit.
fn clip_bars(job: &SoundJob) -> Option<u32> {
    if !fixed_clip(&job.model) {
        return None;
    }
    let bar = 240.0 / job.bpm.unwrap_or(score::DEFAULT_BPM);
    let fits = |b: u32| (b as f32 + 3.0) * bar <= 30.0;
    let bars = job.bars.unwrap_or(8);
    if fits(bars) {
        return None;
    }
    Some([12, 8].into_iter().find(|&b| b < bars && fits(b)).unwrap_or(4))
}

/// After "no section could be cut into a loop": a longer music model when the
/// provider has one and none was named, else half the bars and a new seed.
/// What changed, or None when there's nothing left to try.
fn loop_fallback(job: &mut SoundJob, model_named: bool) -> Option<String> {
    if !model_named && job.provider == "gemini" && job.model != "lyria-3.5" {
        job.model = "lyria-3.5".into();
        return Some("lyria-3.5 (full-length songs)".into());
    }
    let bars = job.bars.unwrap_or(8);
    if bars <= 4 {
        return None;
    }
    let half = (bars / 2).max(4);
    job.bars = Some(half);
    job.seed = job.seed.wrapping_add(1000);
    Some(format!("{half} bars a loop and a new seed"))
}

/// The songs of a setlist after a run: the ones listed before, in their
/// order, a zone made again replaced where it was, then the new zones; and
/// the zones kept from before.
fn merge_setlist(before: Option<&Value>, made: Vec<Value>) -> (Vec<Value>, Vec<String>) {
    let mut songs: Vec<Value> = before.and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut kept: Vec<String> = songs.iter().filter_map(|s| s["zone"].as_str().map(str::to_string)).collect();
    for m in made {
        kept.retain(|k| m["zone"].as_str() != Some(k));
        match songs.iter().position(|s| s["zone"] == m["zone"]) {
            Some(i) => songs[i] = m,
            None => songs.push(m),
        }
    }
    (songs, kept)
}

/// Whether a provider's error is its content filter refusing the prompt.
fn is_refusal(e: &str) -> bool {
    let l = e.to_lowercase();
    ["safety", "blocked", "policy", "bad_prompt", "moderation", "content filter", "violat", "guideline", "prohibited"].iter().any(|w| l.contains(w))
}

/// Words music filters take badly (violence, greed, aggression), by stem, and
/// a plainer word for each ("" drops it). Stems of four letters or fewer take
/// only -s, -er, -ing, -ed after them ("war" isn't in "warm").
const TOUCHY: &[(&str, &str)] = &[
    ("stomp", "heavy"),
    ("greed", "brash"),
    ("swagger", "confident"),
    ("aggress", "driving"),
    ("brutal", "heavy"),
    ("violen", "intense"),
    ("kill", ""),
    ("murder", ""),
    ("blood", ""),
    ("gore", ""),
    ("war", "epic"),
    ("battle", "epic"),
    ("fight", "action"),
    ("evil", "dark"),
    ("demon", "dark"),
    ("satan", "dark"),
    ("hell", "dark"),
    ("menac", "dark"),
    ("sinister", "mysterious"),
    ("rage", "intense"),
    ("angry", "intense"),
    ("gun", ""),
    ("weapon", ""),
    ("bomb", "big"),
    ("explos", "big"),
    ("destr", "powerful"),
    ("smash", "powerful"),
    ("crush", "powerful"),
    ("drunk", "loose"),
    ("drug", ""),
    ("sexy", "smooth"),
    ("seduc", "smooth"),
    ("villain", "dark"),
    ("death", ""),
    ("dead", ""),
    ("doom", "dark"),
];

/// For a refusal: the prompt's words that likely tripped the filter, and the
/// prompt with plainer words in their place. None when it isn't a refusal or
/// no word stands out.
fn refusal_hint(e: &str, prompt: &str) -> Option<(Vec<String>, String)> {
    if !is_refusal(e) {
        return None;
    }
    let mut words = Vec::new();
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String, words: &mut Vec<String>| {
        let low = word.to_lowercase();
        let short = |stem: &str| low.strip_prefix(stem).is_some_and(|rest| ["", "s", "er", "ers", "ing", "ed"].contains(&rest));
        match TOUCHY.iter().find(|(stem, _)| if stem.len() <= 4 { short(stem) } else { low.starts_with(stem) }) {
            Some((_, plain)) => {
                words.push(word.clone());
                out.push_str(plain);
            }
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in prompt.chars() {
        if c.is_alphanumeric() || c == '\'' {
            word.push(c);
        } else {
            flush(&mut word, &mut out, &mut words);
            out.push(c);
        }
    }
    flush(&mut word, &mut out, &mut words);
    if words.is_empty() {
        return None;
    }
    // Tidy what a dropped word leaves: doubled spaces, empty list items.
    let mut softer = out.split(',').map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
    while softer.contains("  ") {
        softer = softer.replace("  ", " ");
    }
    Some((words, softer))
}

/// `--audition`: every switch and join of the music named, into one file.
fn audition_after(a: &MusicArgs) -> Result<(), String> {
    let Some(path) = &a.audition else {
        return Ok(());
    };
    let assets = a.game.join(a.common.out.as_deref().map(|o| Path::new(o).parent().unwrap_or(Path::new("assets")).to_path_buf()).unwrap_or("assets".into()));
    let out = if path.is_empty() { a.game.join(format!("art/raw/{}-audition.ogg", a.name)) } else { PathBuf::from(path) };
    super::audition::render(&assets, &a.name, &out, a.common.quality)
}

/// The sections asked for (`--variations`, `--sections`, `--zones`), and whether to write stingers.
fn form(a: &MusicArgs) -> Result<(Vec<score::Section>, bool), String> {
    if a.variations == Some(0) || a.variations.is_some_and(|n| n > 12) {
        return Err("--variations: 1–12".into());
    }
    if let Some(z) = &a.zones {
        let zones = score::parse_zones(z)?;
        let k = a.variations.unwrap_or(1);
        let v = if k <= 1 {
            zones
        } else {
            zones
                .iter()
                .flat_map(|z| {
                    (1..=k).map(move |i| score::Section {
                        name: format!("{}_{i}", z.name),
                        desc: if i == 1 { z.desc.clone() } else { "another theme for the same place: the same instruments and energy, a new melody".into() },
                        variation_of: (i > 1).then(|| format!("{}_1", z.name)),
                    })
                })
                .collect()
        };
        return Ok((v, true));
    }
    if let Some(s) = &a.sections {
        return Ok((score::parse_sections(s)?, true));
    }
    Ok((a.variations.filter(|&n| n > 1).map(score::variations).unwrap_or_default(), false))
}

/// `mb music <game> <name> --reprocess [--layers …] [--key …]`: the recorded
/// recipe, with the options given now, from the take in art/raw/.
fn reprocess_music(a: &MusicArgs) -> Result<(), String> {
    let assets = a.game.join(a.common.out.as_deref().map(|o| Path::new(o).parent().unwrap_or(Path::new("assets")).to_path_buf()).unwrap_or("assets".into()));
    let sc = find_music_sidecar(&assets, &a.name)?;
    let mut job: SoundJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("sidecar recipe: {e}"))?;
    if a.prompt.as_deref().is_some_and(|p| p != job.subject) {
        eprintln!("  note: --reprocess keeps the take and its recorded prompt ({:?}); generate without --reprocess for a new one", job.subject);
    }
    if a.variations.is_some() || a.sections.is_some() || a.zones.is_some() {
        let (sections, _) = form(a)?;
        if sections != job.sections {
            return Err("the take you have was made for another form; generate it again (without --reprocess) for new sections".into());
        }
    }
    if a.layers.is_some() {
        (job.layers, job.tension) = parse_layers(a.layers.as_deref())?;
    }
    if a.key.is_some() {
        job.key = a.key.clone();
    }
    if a.bars.is_some() {
        job.bars = a.bars;
    }
    if a.bpm.is_some() {
        job.bpm = a.bpm;
    }
    if a.loop_xfade.is_some() {
        job.loop_xfade_bars = a.loop_xfade;
    }
    if a.loops || a.fix_loops.is_some() {
        job.loops = true;
    }
    if let Some(d) = a.downbeat {
        job.downbeat_shift = (d != 0.0).then_some(d);
    }
    job.fix_below = a.fix_loops;
    if job.sections.is_empty() && (a.downbeat.is_some() || a.fix_loops.is_some()) {
        return Err(format!("--downbeat and --fix-loops are for music in sections; {} is a single loop", a.name));
    }
    let raw = load_raw(&a.game, &sc)?;
    execute(&a.game, &job, a.common.dry_run, super::max_cost(a.common.max_cost), Some(raw))
}

/// The sidecar of a music asset named `name` in `assets`: <name>.ogg, its
/// base layer, or any file it wrote (a variation, a zone, the tension kit).
pub(crate) fn find_music_sidecar(assets: &Path, name: &str) -> Result<super::Sidecar, String> {
    for f in [format!("{name}.ogg"), format!("{name}_base.ogg")] {
        if let Ok(sc) = super::read_sidecar(&assets.join(f)) {
            return Ok(sc);
        }
    }
    let mut found: Vec<_> = std::fs::read_dir(assets).map_err(|e| format!("{}: {e}", assets.display()))?.flatten().map(|e| e.path()).collect();
    found.sort();
    for p in found {
        let f = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if let Some(asset) = f.strip_suffix(".gen.json")
            && asset.starts_with(&format!("{name}_"))
            && let Ok(sc) = super::read_sidecar(&assets.join(asset))
            && sc.recipe["kind"] == "music"
            && sc.recipe["name"] == name
        {
            return Ok(sc);
        }
    }
    Err(format!("no music named {name} in {} (no sidecar): generate it first, without --reprocess", assets.display()))
}

/// The raw take a sidecar points at, with its mime type.
fn load_raw(game: &Path, sc: &super::Sidecar) -> Result<(Vec<u8>, String, String), String> {
    let p = game.join(&sc.raw);
    let b = std::fs::read(&p).map_err(|e| format!("{}: {e} (the raw take is git-ignored; regenerate without --reprocess)", p.display()))?;
    let mime = match p.extension().and_then(|e| e.to_str()) {
        Some("mp3") => "audio/mpeg".to_string(),
        Some("ogg") => "audio/ogg".to_string(),
        Some("pcm") => std::fs::read_to_string(game.join(format!("{}.mime", sc.raw))).unwrap_or_else(|_| "audio/pcm;rate=24000".into()),
        _ => "audio/wav".to_string(),
    };
    Ok((b, mime, sc.raw.clone()))
}

/// `--layers <list>` → (split into base and high, the tension kit).
fn parse_layers(list: Option<&str>) -> Result<(bool, bool), String> {
    let Some(list) = list else {
        return Ok((false, false));
    };
    let words: Vec<&str> = list.split(',').map(str::trim).filter(|w| !w.is_empty()).collect();
    if let Some(w) = words.iter().find(|w| !["base", "hi", "tension"].contains(w)) {
        return Err(format!("--layers: unknown layer {w:?} (base, hi, tension)"));
    }
    let (base, hi) = (words.contains(&"base"), words.contains(&"hi"));
    if base != hi {
        return Err("--layers: base and hi come together (base + hi is the full mix)".into());
    }
    if words.is_empty() {
        return Err("--layers: name the layers (base,hi or base,hi,tension)".into());
    }
    Ok((base, words.contains(&"tension")))
}

pub fn sfx(a: SfxArgs) -> Result<(), String> {
    let style = if a.common.no_style { Style::default() } else { Style::load(&a.game)?.0 };
    if let Some(p) = &a.pack {
        let recipes = pack(p).ok_or(format!("--pack {p}: the packs are mech"))?;
        if a.common.out.is_some() {
            return Err("--out names one file; a pack writes assets/<prefix><sound>.ogg (use --prefix)".into());
        }
        let prefix = a.prefix.clone().unwrap_or(format!("{p}_"));
        for r in recipes {
            let name = format!("{prefix}{}", r.sound);
            let prompt = match &a.flavor {
                Some(f) => format!("{} ({})", r.prompt, f.trim()),
                None => r.prompt.to_string(),
            };
            let mut job = resolve("sfx", &a.game, &name, &prompt, &a.common, &style)?;
            job.seconds = Some(r.seconds);
            job.looping = r.looping;
            job.brighten = r.brighten;
            eprintln!("{name}:");
            execute(&a.game, &job, a.common.dry_run, super::max_cost(a.common.max_cost), None)?;
        }
        return Ok(());
    }
    let (Some(name), Some(prompt)) = (&a.name, &a.prompt) else {
        return Err("give a name and a prompt, or --pack".into());
    };
    let mut job = resolve("sfx", &a.game, name, prompt, &a.common, &style)?;
    job.seconds = a.seconds;
    job.looping = a.looping;
    job.brighten = a.brighten;
    execute(&a.game, &job, a.common.dry_run, super::max_cost(a.common.max_cost), None)
}

pub fn compose(job: &SoundJob, style: &Style) -> String {
    let mut p = vec![format!("{}.", job.subject.trim().trim_end_matches('.'))];
    let extra = style.kind(&job.kind).prompt;
    if !job.no_style && !extra.trim().is_empty() {
        p.push(format!("{}.", extra.trim().trim_end_matches('.')));
    }
    if job.kind == "music" {
        p.push("Instrumental only, no vocals.".into());
        if !job.sections.is_empty() {
            p.push(score::prompt_lines(&job.sections, &plan_of(job)));
        } else if job.once {
            p.push("A short, complete piece with a clear ending.".into());
        } else {
            p.push(format!(
                "{}A steady tempo and one consistent groove all the way through: no intro, no breakdown, no fade-out and no ending, so it can loop seamlessly as game music.",
                job.bpm.map(|b| format!("{b:.0} BPM, 4/4. ")).unwrap_or_default()
            ));
            if job.bars.is_some_and(|b| b > 8) {
                p.push("Let the arrangement develop over its length (a second melody, a layer entering) without changing the tempo or the key.".into());
            }
        }
        if job.model == "lyria-3.5" && job.sections.is_empty() {
            // Enough for the loop asked for, its intro skipped and its seam judged.
            let want = job.bars.map(|b| b as f32 * 240.0 / job.bpm.unwrap_or(score::DEFAULT_BPM) + 12.0);
            p.push(format!("About {:.0} seconds long.", job.seconds.or(want).unwrap_or(45.0).max(30.0)));
        }
        p.push("An original composition, not imitating any existing song or artist.".into());
    } else {
        p.push(format!(
            "A single clean sound effect for a mobile game{}, starting immediately with no silence before it; no music, no voice.",
            job.seconds.map(|s| format!(", about {s:.1} seconds")).unwrap_or_default()
        ));
        if job.looping {
            p.push("It loops seamlessly.".into());
        }
    }
    p.join(" ")
}

/// Generates (or with `raw`, reprocesses) a sound job.
pub fn execute(game: &Path, job: &SoundJob, dry_run: bool, limit: f64, raw: Option<(Vec<u8>, String, String)>) -> Result<(), String> {
    if !game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", game.display()));
    }
    let (style, style_sha) = if job.no_style { (Style::default(), None) } else { Style::load(game)? };
    let style_ref = style_sha.map(|s| FileRef { path: "art/style.toml".into(), sha256: s });
    let verdict = super::ip::check(&[&job.subject, &style.kind(&job.kind).prompt], &job.allow_names);
    if !verdict.ok() {
        return Err(super::ip::refusal_message(&verdict));
    }
    for w in &verdict.warnings {
        eprintln!("  warning: the prompt references a named style ({w}); make it your own sound");
    }
    let full_prompt = compose(job, &style);
    let model = provider::find_model(&job.provider, &job.model);
    let sectioned = job.kind == "music" && !job.sections.is_empty();
    let plan = sectioned.then(|| plan_of(job));
    let req_secs = match job.kind.as_str() {
        "music" if plan.is_some() => plan.as_ref().map(|p| p.total),
        "music" if job.provider == "elevenlabs" || job.provider == "mock" => {
            let want = job.bars.map(|b| b as f32 * 240.0 / job.bpm.unwrap_or(score::DEFAULT_BPM) + 6.0).unwrap_or(0.0);
            Some(job.seconds.unwrap_or(30.0).max(16.0).max(want) + 6.0)
        }
        "music" => job.seconds,
        _ => job.seconds,
    };
    let est = model.and_then(|m| provider::estimate(m, "", 0, req_secs.unwrap_or(if job.kind == "sfx" { 2.0 } else { 30.0 })));
    let extra = match (job.layers, job.tension) {
        (true, true) => " (+ _base/_hi layers and the tension kit)",
        (true, false) => " (+ _base/_hi layers)",
        (false, true) => " (+ the tension kit)",
        _ => "",
    };
    eprintln!("{} {} → {}{extra}", job.kind, job.name, job.out);
    if !job.sections.is_empty() {
        let names: Vec<String> = score::unique(&job.sections).into_iter().map(|s| s.name).collect();
        eprintln!("  {} loops in one key, tempo and bar grid: {}", names.len(), names.join(", "));
    }
    eprintln!("  {} {} · seed {}{}", job.provider, job.model, job.seed, if model.is_some_and(|m| !m.seed) { " (not honored)" } else { "" });
    let rate = job.rate;
    let mut takes: Vec<Take> = Vec::new();
    let mut chosen: Option<(Take, score::Cut)> = None;
    match raw {
        Some((b, m, r)) => {
            eprintln!("  reprocessing {r} (no provider call)");
            // Keep how the raw output was made.
            let via = super::read_sidecar(&game.join(&job.out)).ok().and_then(|s| s.via);
            takes.push(Take { bytes: b, mime: m, raw: r, cost: None, text: None, via, seed: job.seed });
        }
        None => {
            let calls = 1 + job.retries;
            super::check_cost(est, calls, limit, &format!("this {}{}", if job.kind == "sfx" { "effect" } else { "track" }, if calls > 1 { " if every retry is needed" } else { "" }))?;
            if dry_run {
                eprintln!("  prompt: {full_prompt}\n  (dry run: nothing generated)");
                return Ok(());
            }
            let prov = provider::pick(&job.provider, &job.model, est)?;
            if prov.via().is_some() && !verdict.allowed.is_empty() {
                return Err(super::maimbrain::allow_name_refused(&verdict.allowed, &job.provider));
            }
            for attempt in 0..calls {
                let seed = job.seed.wrapping_add(attempt as u64);
                let take = fetch(game, job, prov.as_ref(), &full_prompt, req_secs, seed)?;
                if let Some(p) = &plan {
                    // Sections: keep the take whose sections fit the piece best.
                    let (chans, src_rate) = audio::decode(&take.bytes, &take.mime).map_err(|e| format!("{}: {e}", job.provider))?;
                    let mono = audio::resample(&audio::to_mono(&chans), src_rate, rate);
                    // A take no loop can be cut from is a bad take: ask again while retries last.
                    let cut = match score::cut(&mono, rate, &job.sections, p, given_key(job)?, &cut_opts(job)) {
                        Ok(c) => c,
                        Err(e) if attempt + 1 < calls => {
                            eprintln!("  take {} (seed {seed}): {e}; asking again", attempt + 1);
                            continue;
                        }
                        Err(_) if chosen.is_some() => break,
                        Err(e) => return Err(e),
                    };
                    let n = cut.mismatches.len();
                    if n > 0 {
                        eprintln!("  take {} (seed {seed}): {n} section(s) don't fit: {}", attempt + 1, cut.mismatches.join("; "));
                    }
                    if chosen.as_ref().is_none_or(|c| n < c.1.mismatches.len()) {
                        chosen = Some((take, cut));
                    }
                    if n == 0 {
                        break;
                    }
                    if attempt + 1 < calls {
                        eprintln!("  asking again");
                    }
                } else {
                    takes.push(take);
                }
            }
        }
    }
    let (take, cut) = match chosen {
        Some((t, c)) => (t, Some(c)),
        None => (takes.remove(0), None),
    };
    // What the sidecars record: the take that was used (its seed, so regen gets it again).
    let mut job = SoundJob { seed: take.seed, section_seconds: plan.as_ref().map(|p| p.per), ..job.clone() };
    let Take { bytes, mime, raw: raw_rel, cost, text, via, .. } = take;
    let (chans, src_rate) = audio::decode(&bytes, &mime).map_err(|e| format!("{}: {e}", job.provider))?;
    let mono = audio::resample(&audio::to_mono(&chans), src_rate, job.rate);
    let mut facts = json!({ "source_seconds": mono.len() as f32 / rate as f32, "source_channels": chans.len(), "source_rate": src_rate });
    let mut warnings: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // The music graph of a piece in sections, written next to its audio after it.
    let mut graph: Option<Value> = None;
    let outputs: Vec<Out> = if let Some(p) = &plan {
        let mut cut = match cut {
            Some(c) => c,
            None => score::cut(&mono, rate, &job.sections, p, given_key(&job)?, &cut_opts(&job))?,
        };
        if job.loops {
            section_clips(game, &mut job, &mut cut, dry_run, limit, &mut notes)?;
        }
        mix_on_phone(&mono, rate, &mut facts, &mut warnings);
        let (outs, g) = sectioned_outputs(&job, cut, &mono, rate, &mut facts, &mut warnings, &mut notes)?;
        graph = Some(g);
        outs
    } else if job.kind == "music" && !job.once {
        let lp = make_loop(&mono, rate, &job, &mut facts, &mut warnings)?;
        let key = music_key(&job, &lp, rate, &mut facts, &mut notes)?;
        mix_on_phone(&lp, rate, &mut facts, &mut warnings);
        let mut outs = if job.layers {
            let (base, hi) = audio::split_layers(&lp, rate, job.split_hz);
            let g = audio::gain_for(&lp, Kind::Music);
            vec![Out::music("_base", audio::limit_loop(&base, g, rate)), Out::music("_hi", audio::limit_loop(&hi, g, rate))]
        } else {
            vec![Out::music("", audio::limit_loop(&lp, audio::gain_for(&lp, Kind::Music), rate))]
        };
        if job.tension {
            outs.extend(tension_kit(&job, &lp, rate, key, &mut facts, &mut warnings, &mut notes));
        }
        outs
    } else if job.kind == "music" {
        let x = audio::trim_silence(&mono, rate, -55.0);
        music_key(&job, &x, rate, &mut facts, &mut notes)?;
        mix_on_phone(&x, rate, &mut facts, &mut warnings);
        vec![Out::new("", audio::normalize(&x, Kind::Music, rate), false, false)]
    } else if job.looping {
        let xf = ((0.05 * rate as f32) as usize).min(mono.len() / 4);
        let x = if job.brighten { brighten(&mono, rate, false, &mut facts, &mut notes) } else { mono };
        let lp = audio::cut_loop(&x, 0, x.len() - xf, xf);
        vec![Out::new("", audio::limit_loop(&lp, audio::gain_for(&lp, Kind::Sfx), rate), true, false)]
    } else {
        let mut t = audio::trim_silence(&mono, rate, -50.0);
        // Hold the model to the length asked for (with a little slack).
        if let Some(s) = job.seconds
            && t.len() as f32 > s * rate as f32 * 1.1
        {
            facts["trimmed_from_seconds"] = json!(t.len() as f32 / rate as f32);
            t = audio::cut_to(&t, rate, s);
        }
        if t.len() as f32 / rate as f32 > 4.0 {
            warnings.push(format!("{:.1} s is long for an effect; pass --seconds", t.len() as f32 / rate as f32));
        }
        if job.brighten {
            t = brighten(&t, rate, true, &mut facts, &mut notes);
        }
        vec![Out::new("", audio::normalize(&t, Kind::Sfx, rate), false, false)]
    };
    let out_base = game.join(&job.out);
    if let Some(d) = out_base.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut written = Vec::new();
    let stem = out_base.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    for o in &outputs {
        let suffix = o.suffix.as_str();
        let dest = out_base.with_file_name(format!("{stem}{suffix}.ogg"));
        let decoded = match audio::encode_ogg(&o.samples, rate, job.quality, &dest) {
            Ok(d) => d,
            Err(e) => {
                super::ensure_art_dir(game)?;
                let wav = game.join(format!("art/raw/{stem}{suffix}.processed.wav"));
                std::fs::write(&wav, audio::wav_bytes(&o.samples, rate)).map_err(|e| e.to_string())?;
                return Err(format!("{e}\nthe processed audio is kept in {}", wav.display()));
            }
        };
        let chk = if o.seam_at > 0 { audio::check(&audio::rotate(&decoded, o.seam_at), rate, o.is_loop) } else { audio::check(&decoded, rate, o.is_loop) };
        let ogg = std::fs::read(&dest).map_err(|e| e.to_string())?;
        let mut f = facts.clone();
        f["seconds"] = json!(chk.secs);
        f["peak_db"] = json!(chk.peak_db);
        f["loud_rms_db"] = json!(chk.rms_db);
        let phone = audio::phone_loss_db(&decoded, rate, !o.is_loop);
        f["phone_loss_db"] = json!((phone * 10.0).round() / 10.0);
        if job.kind == "sfx" && phone < audio::PHONE_WARN_DB {
            let rel = super::rel(&dest, game);
            warnings.push(if job.brighten {
                format!("{rel}: a phone speaker still loses {:.0} dB of it after --brighten; ask for a brighter take (\"a crisp …, with a bright click\") or pitch it up an octave", -phone)
            } else {
                format!(
                    "{rel}: a phone speaker loses {:.0} dB of it (it's mostly bass under ~{:.0} Hz, which a phone can't play). Ask for a brighter take (\"a crisp …, with a bright click\"), or add the bass's harmonics for free: mb regen {} --reprocess --brighten",
                    -phone,
                    audio::PHONE_HP,
                    dest.display()
                )
            });
        }
        if let Some(s) = chk.seam {
            f["seam_ratio"] = json!(s);
            f["seam_jump_db"] = json!((chk.seam_jump_db.unwrap_or(-120.0) * 10.0).round() / 10.0);
            if chk.click() {
                warnings.push(format!("{}: a click at the loop seam ({s:.1}×); regenerate or try other --bars", dest.display()));
            }
        }
        if !suffix.is_empty() {
            f["layer"] = json!(suffix.trim_start_matches('_'));
        }
        if let Value::Object(m) = &o.facts {
            for (k, v) in m {
                f[k] = v.clone();
            }
        }
        f["warnings"] = json!(warnings);
        // The tension kit is mb's own synthesis, derived from the generated loop: no provider, no cost.
        let (provider, model_id, note, est_c, cost_c, via_c) = if o.synth {
            f["synthesized"] = json!(format!("by mb on {stem}'s beat grid and in its key, from the generated loop (no provider call)"));
            (
                "mb".to_string(),
                if suffix.starts_with("_into_") {
                    "transition-stinger"
                } else if suffix == "_swell" {
                    "transition-swell"
                } else {
                    "tension-kit"
                }
                .to_string(),
                format!("Synthesized by mb to fit {stem}; the generated loop's provenance is below."),
                Some(0.0),
                Some(0.0),
                None,
            )
        } else {
            (job.provider.clone(), job.model.clone(), super::note_for(&job.provider), est, cost, via.clone())
        };
        let sc = Sidecar {
            generator: format!("mb {} {}", env!("CARGO_PKG_VERSION"), job.kind),
            date: super::now_utc(),
            provider,
            model: model_id,
            via: via_c,
            prompt: job.subject.clone(),
            full_prompt: full_prompt.clone(),
            seed: job.seed,
            seed_honored: o.synth || model.is_none_or(|m| m.seed),
            references: Vec::new(),
            style: style_ref.clone(),
            est_cost_usd: est_c,
            cost_usd: cost_c,
            raw: raw_rel.clone(),
            raw_sha256: super::sha256_hex(&bytes),
            output_sha256: super::sha256_hex(&ogg),
            allowed_names: verdict.allowed.clone(),
            model_text: text.clone(),
            recipe: serde_json::to_value(&job).unwrap(),
            result: f,
            note,
            derived: Vec::new(),
        };
        super::write_sidecar(&dest, &sc)?;
        eprintln!("  wrote {} ({} KB): {chk} · phone {phone:+.1} dB", dest.display(), ogg.len().div_ceil(1024));
        written.push(dest);
    }
    // The graph: what the take files are and how to move through them. Its sidecar is the take's.
    if let Some(g) = graph {
        let dest = out_base.with_file_name(format!("{stem}.music.json"));
        let text = serde_json::to_string_pretty(&g).unwrap() + "\n";
        std::fs::write(&dest, &text).map_err(|e| format!("{}: {e}", dest.display()))?;
        let take = written.iter().find(|w| w.file_name().and_then(|f| f.to_str()).is_some_and(|f| f.contains("_take"))).cloned();
        if let Some(t) = take {
            let mut sc = super::read_sidecar(&t)?;
            sc.output_sha256 = super::sha256_hex(text.as_bytes());
            sc.result = json!({ "graph": g["name"], "sections": g["sections"].as_array().map_or(0, Vec::len), "links": g["links"].as_array().map_or(0, Vec::len), "risks": g["risks"] });
            super::write_sidecar(&dest, &sc)?;
        }
        eprintln!("  wrote {} ({} KB): the music graph", dest.display(), text.len().div_ceil(1024));
        written.push(dest);
    }
    for n in &notes {
        eprintln!("  {n}");
    }
    for w in &warnings {
        eprintln!("  warning: {w}");
    }
    if let Some(l) = facts["loop"].as_str() {
        eprintln!("  {l}");
    }
    if let Some(l) = facts["graph"].as_str() {
        eprintln!("  {l}");
    }
    if !job.sections.is_empty() {
        eprintln!(
            "  in the game: maimbrain::music::Director::from_graph plays the take, takes a jump link now and then so a zone never loops audibly, \
             and changes zone at the next exit cue through a link, the take's own transition or a stinger, with the tension in step \
             (the skill's \"Music that follows the game\"). Listen to its jumps and changes: mb music <game> {} --audition",
            job.name
        );
        if job.loops {
            eprintln!(
                "  the loops (--loops) are {} samples long with a bar's 1 at downbeat_seconds, for Director::new's loop mode",
                facts["loop_samples"]
            );
        }
    } else if job.tension {
        eprintln!(
            "  in the game: start every loop with play_looped_at and the same `at` (the tension layers at vol 0), raise {stem}_tension and then {stem}_tick with the danger, \
             schedule {stem}_heartbeat on the beat grid (more often as it grows) and play {stem}_sting on a near-catch (the skill's \"Music that follows the game\")"
        );
    } else if job.layers {
        eprintln!("  in the game: start both with play_looped in the same frame (the high layer at vol 0) and raise its vol with intensity");
    } else if job.kind == "music" && !job.once {
        eprintln!("  in the game: Sound::play_looped; for rhythm, schedule it with play_looped_at on the beat grid above (SPEC §5.6)");
    }
    eprintln!("  you can't hear it: check the numbers above (and the skill's check_audio.py) and listen on a phone before publishing");
    for w in &written {
        println!("{}", w.display());
    }
    Ok(())
}

/// One file `execute` writes.
struct Out {
    suffix: String,
    samples: Vec<f32>,
    is_loop: bool,
    /// Synthesized by mb (the tension kit), not the provider's audio.
    synth: bool,
    /// Facts about this file alone (a section's measurements), merged into the sidecar's result.
    facts: Value,
    /// Where to check a loop for a click: its seam (0, the file's ends, unless it was rotated).
    seam_at: usize,
}

impl Out {
    fn new(suffix: &str, samples: Vec<f32>, is_loop: bool, synth: bool) -> Out {
        Out { suffix: suffix.to_string(), samples, is_loop, synth, facts: Value::Null, seam_at: 0 }
    }

    fn music(suffix: &str, samples: Vec<f32>) -> Out {
        Out::new(suffix, samples, true, false)
    }
}

/// What a provider returned for one request, saved in art/raw/.
struct Take {
    bytes: Vec<u8>,
    mime: String,
    raw: String,
    cost: Option<f64>,
    text: Option<String>,
    via: Option<String>,
    seed: u64,
}

/// One provider call; the result is kept in art/raw/<name>-<seed>.<ext>.
fn fetch(game: &Path, job: &SoundJob, prov: &dyn provider::Provider, prompt: &str, secs: Option<f32>, seed: u64) -> Result<Take, String> {
    let req = AudioRequest {
        model: job.model.clone(),
        prompt: prompt.to_string(),
        kind: if job.kind == "music" { Kind::Music } else { Kind::Sfx },
        seconds: secs,
        looping: job.looping,
        seed: Some(seed),
        negative: job.negative.clone(),
    };
    eprint!("  generating…");
    let t0 = std::time::Instant::now();
    let out = prov.audio(&req)?;
    eprintln!(" {:.1} s", t0.elapsed().as_secs_f32());
    if let Some(n) = prov.note() {
        eprintln!("  {n}");
    }
    super::ensure_art_dir(game)?;
    let ext = match out.mime.as_str() {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" => "ogg",
        m if m.starts_with("audio/pcm") => "pcm",
        _ => "wav",
    };
    let r = format!("art/raw/{}-{seed}.{ext}", job.name);
    std::fs::write(game.join(&r), &out.bytes).map_err(|e| format!("{r}: {e}"))?;
    // Keep the mime for raw PCM next to it.
    if ext == "pcm" {
        let _ = std::fs::write(game.join(format!("{r}.mime")), &out.mime);
    }
    Ok(Take { bytes: out.bytes, mime: out.mime, raw: r, cost: out.cost_usd, text: out.text, via: prov.via().map(str::to_string), seed })
}

/// Where a sectioned job's sections are in the piece asked for.
fn plan_of(job: &SoundJob) -> score::Plan {
    score::plan(&job.sections, job.bars.unwrap_or(8), job.bpm.unwrap_or(score::DEFAULT_BPM), job.section_seconds)
}

/// How a sectioned job is cut (`--loop-xfade`, `--downbeat`).
fn cut_opts(job: &SoundJob) -> score::Opts {
    score::Opts { xfade_bars: job.loop_xfade_bars.unwrap_or(1.0), shift_beats: job.downbeat_shift.unwrap_or(0.0) }
}

/// Sections with a loop of their own from a separate clip: the ones made
/// before (the recipe's `section_clips`, free), and with `--fix-loops` a new
/// clip for each section that joins worse than asked. A clip's loop is used
/// only where it joins better than the section's own.
fn section_clips(game: &Path, job: &mut SoundJob, cut: &mut score::Cut, dry_run: bool, limit: f64, notes: &mut Vec<String>) -> Result<(), String> {
    let rate = job.rate;
    let fit = cut.fit();
    let apply = |cut: &mut score::Cut, name: &str, bytes: &[u8], mime: &str, raw: &str, notes: &mut Vec<String>| -> Result<(), String> {
        let (chans, src_rate) = audio::decode(bytes, mime).map_err(|e| format!("{raw}: {e}"))?;
        let mono = audio::resample(&audio::to_mono(&chans), src_rate, rate);
        let Some(i) = cut.loops.iter().position(|l| l.name == name) else {
            return Ok(());
        };
        match score::fit_clip(name, &mono, rate, &fit) {
            Ok(mut c) if c.score > cut.loops[i].score => {
                let own = &cut.loops[i];
                notes.push(format!(
                    "{name}: its own clip ({raw}) joins at {:.2}, its section of the piece at {:.2}: the clip's loop is used",
                    c.score, own.score
                ));
                c.facts["section_loop_match"] = own.facts["loop_match"].clone();
                c.facts["window_seconds"] = own.facts["window_seconds"].clone();
                c.facts["clip_raw"] = json!(raw);
                cut.loops[i] = c;
                // What was said about the section's own loop no longer applies.
                cut.notes.retain(|n| !n.starts_with(&format!("{name}: ")));
            }
            Ok(c) => notes.push(format!(
                "{name}: its clip ({raw}) joins at {:.2}, no better than its section's {:.2}: the section's loop is kept",
                c.score, cut.loops[i].score
            )),
            Err(e) => notes.push(format!("{e}; the section's loop is kept")),
        }
        Ok(())
    };
    for (name, raw) in job.section_clips.clone() {
        let p = game.join(&raw);
        match std::fs::read(&p) {
            Ok(b) => apply(cut, &name, &b, mime_of(&p), &raw, notes)?,
            Err(e) => notes.push(format!("{name}: its clip {raw} isn't here ({e}); the section's loop is used")),
        }
    }
    let Some(below) = job.fix_below else {
        let weak: Vec<String> = cut.weak(score::FIX_BELOW).iter().filter(|l| !job.section_clips.contains_key(&l.name)).map(|l| l.name.clone()).collect();
        if !weak.is_empty() {
            notes.push(format!(
                "{} {} under {:.2}: {}. Listen to them (--audition); `--reprocess --fix-loops` asks for a loop of their own (one Lyria 3 Clip each, ~$0.04)",
                weak.len(),
                if weak.len() == 1 { "loop joins" } else { "loops join" },
                score::FIX_BELOW,
                weak.join(", ")
            ));
        }
        return Ok(());
    };
    let weak: Vec<String> = cut.weak(below).iter().filter(|l| !job.section_clips.contains_key(&l.name)).map(|l| l.name.clone()).collect();
    if weak.is_empty() {
        notes.push(format!("--fix-loops: every loop joins at {below:.2} or better; no clips asked for"));
        return Ok(());
    }
    let model = provider::default_model(&job.provider, Media::Music).ok_or(format!("{} has no music model for --fix-loops", job.provider))?;
    let est = provider::estimate(model, "", 0, 30.0);
    eprintln!("  --fix-loops: {} join under {below:.2} ({}); asking {} {} for a loop of their own", weak.len(), weak.join(", "), job.provider, model.id);
    super::check_cost(est, weak.len() as u32, limit, "the clips")?;
    if dry_run {
        notes.push("--fix-loops: dry run, no clips asked for".into());
        return Ok(());
    }
    let prov = provider::pick(&job.provider, model.id, est)?;
    for name in weak {
        let Some(sec) = job.sections.iter().find(|s| s.name == name).cloned() else {
            continue;
        };
        let prompt = score::clip_prompt(&job.subject, &sec, &job.sections, cut.bpm, cut.key);
        let cjob = SoundJob { name: format!("{}_{name}", job.name), model: model.id.to_string(), sections: Vec::new(), ..job.clone() };
        // A new seed for each try (a clip that didn't fit isn't asked for again).
        let tries = std::fs::read_dir(game.join("art/raw"))
            .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{}-", cjob.name))).count())
            .unwrap_or(0);
        let take = fetch(game, &cjob, prov.as_ref(), &prompt, Some(30.0), job.seed.wrapping_add(1 + tries as u64))?;
        apply(cut, &name, &take.bytes, &take.mime, &take.raw, notes)?;
        job.section_clips.insert(name, take.raw);
    }
    Ok(())
}

fn mime_of(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()) {
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        _ => "audio/wav",
    }
}

fn given_key(job: &SoundJob) -> Result<Option<audio::Key>, String> {
    job.key.as_ref().map(|k| audio::Key::parse(k).ok_or(format!("--key {k:?}: give a key like \"A minor\", \"F#m\" or \"Eb\""))).transpose()
}

/// The zone each entry of a piece's form belongs to: a zone and its
/// variations are one (`--zones … --variations K`: `lantern_town_1` and
/// `_2` are `lantern_town`), each section of `--sections` is its own (a name
/// repeated in the form is the same zone), and `--variations` alone is one
/// zone named after the music.
fn zone_names(job: &SoundJob) -> Vec<String> {
    if !job.stingers {
        return job.sections.iter().map(|_| job.name.clone()).collect();
    }
    let zone = |name: &str| -> String {
        let s = job.sections.iter().find(|s| s.name == name);
        let root = s.and_then(|s| s.variation_of.clone()).unwrap_or_else(|| name.to_string());
        if job.sections.iter().any(|o| o.variation_of.as_deref() == Some(root.as_str())) {
            // `lantern_town_1` → `lantern_town`.
            match root.rsplit_once('_') {
                Some((z, k)) if k.chars().all(|c| c.is_ascii_digit()) => z.to_string(),
                _ => root,
            }
        } else {
            root
        }
    };
    job.sections.iter().map(|s| zone(&s.name)).collect()
}

/// The files for a piece in sections: the take (or its base and high layers)
/// and its music graph, the tension kit on the piece's grid, a stinger into
/// each zone and the swell; with `--loops`, a loop per section too.
fn sectioned_outputs(
    job: &SoundJob,
    cut: score::Cut,
    x: &[f32],
    rate: u32,
    facts: &mut Value,
    warnings: &mut Vec<String>,
    notes: &mut Vec<String>,
) -> Result<(Vec<Out>, Value), String> {
    let names: Vec<String> = score::unique(&job.sections).into_iter().map(|s| s.name).collect();
    let loop_names: Vec<String> = cut.loops.iter().map(|l| l.name.clone()).collect();
    let beats = cut.bars as usize * 4;
    let secs = cut.len as f32 / rate as f32;
    facts["bars"] = json!(cut.bars);
    facts["bpm"] = json!(cut.bpm);
    facts["loop_samples"] = json!(cut.len);
    facts["loop_seconds"] = json!(secs);
    facts["downbeat_seconds"] = json!(score::PREROLL);
    facts["grid"] = cut.piece.clone();
    facts["key"] = json!(cut.key.name());
    facts["sections"] = json!(names);
    facts["mismatches"] = json!(cut.mismatches);
    if job.key.is_some() {
        facts["key_given"] = json!(cut.key.name());
    }
    if job.loops {
        facts["loop_xfade_bars"] = json!(job.loop_xfade_bars.unwrap_or(1.0));
        facts["loop"] = json!(format!(
            "loops: {} × {:.3} s = {} bars of 4/4 at {:.2} BPM, each with a bar's 1 of the piece's one grid {:.0} ms in (downbeat_seconds) and {} bar{} of crossfade baked in, in {}",
            loop_names.len(),
            secs,
            cut.bars,
            cut.bpm,
            score::PREROLL * 1000.0,
            job.loop_xfade_bars.unwrap_or(1.0),
            if job.loop_xfade_bars.unwrap_or(1.0) == 1.0 { "" } else { "s" },
            cut.key.name()
        ));
        notes.extend(cut.notes.iter().cloned());
    } else {
        // What was said about each section's loop doesn't apply to the graph.
        notes.extend(cut.notes.iter().filter(|n| !loop_names.iter().any(|l| n.starts_with(&format!("{l}: ")))).cloned());
    }
    notes.push(format!(
        "grid: one for the piece, its 1 at {:.3} s in the take ({} sections agree on that beat){}",
        cut.piece["downbeat_in_take_seconds"].as_f64().unwrap_or(0.0),
        cut.piece["sections_agreeing"].as_str().unwrap_or("?"),
        match job.downbeat_shift {
            Some(d) => format!(", moved {d:+} beats (--downbeat)"),
            None => String::new(),
        }
    ));
    for l in &cut.loops {
        let f = &l.facts;
        if !job.loops {
            notes.push(format!(
                "{}: {} ({:.0} % in {}){}",
                l.name,
                f["key"].as_str().unwrap_or("?"),
                f["in_key"].as_f64().unwrap_or(0.0) * 100.0,
                cut.key.name(),
                f["tempo_bpm"].as_f64().map(|t| format!(", {t:.2} BPM")).unwrap_or_default()
            ));
            continue;
        }
        let mut fixes = Vec::new();
        if let Some(t) = f["tempo_fix_pct"].as_f64() {
            fixes.push(format!("tempo {t:+.1} % (it played at {:.1} BPM)", f["tempo_bpm"].as_f64().or(f["clip_bpm"].as_f64()).unwrap_or(0.0)));
        }
        if let Some(s) = f["pitch_fix_semitones"].as_i64() {
            fixes.push(format!("pitch {s:+} semitone"));
        }
        let parts = &f["loop_match_parts"];
        notes.push(format!(
            "{}: {} ({:.0} % in {}), loop match {:.2}{} (harmony {:.2}, level ±{:.1} dB, rhythm {:.2}){}{}",
            l.name,
            f["key"].as_str().unwrap_or("?"),
            f["in_key"].as_f64().unwrap_or(0.0) * 100.0,
            cut.key.name(),
            l.score,
            if l.score < score::FIX_BELOW { " (weak)" } else { "" },
            parts["harmony"].as_f64().unwrap_or(0.0),
            parts["level_db"].as_f64().unwrap_or(0.0),
            parts["rhythm"].as_f64().unwrap_or(0.0),
            match f["phrase_bars"].as_u64() {
                Some(b) if b < cut.bars as u64 => format!(", a {b}-bar phrase"),
                _ => String::new(),
            },
            if f["source"] == "clip" {
                "; from its own clip".to_string()
            } else if fixes.is_empty() {
                String::new()
            } else {
                format!("; corrected {}", fixes.join(", "))
            }
        ));
    }
    for m in &cut.mismatches {
        let apart = if m.contains("semitone") && job.stingers { " (zones meant to be different songs, in their own keys and tempos: --zone NAME=PROMPT for each)" } else { "" };
        warnings.push(format!("doesn't fit the piece: {m}. Regenerate (mb regen … --new-seed) or drop that section{apart}"));
    }
    let mut outs = Vec::new();
    if job.loops {
        for l in &cut.loops {
            let mut f = l.facts.clone();
            f["variation"] = json!(l.name);
            if job.layers {
                // Split after the crossfade is baked in: every layer has the same join, and base + hi is the loop.
                let (base, hi) = audio::split_layers(&l.samples, rate, job.split_hz);
                let g = audio::gain_for(&l.samples, Kind::Music);
                for (suffix, x) in [("base", base), ("hi", hi)] {
                    let mut o = Out::music(&format!("_{}_{suffix}", l.name), audio::limit_loop(&x, g, rate));
                    o.facts = f.clone();
                    outs.push(o);
                }
            } else {
                let mut o = Out::music(&format!("_{}", l.name), audio::limit_loop(&l.samples, audio::gain_for(&l.samples, Kind::Music), rate));
                o.facts = f;
                outs.push(o);
            }
        }
    }
    // The music graph of the take (graph.rs), and the take itself up to its last section's post-exit.
    let zones = zone_names(job);
    let asked: Vec<graph::Asked> = job
        .sections
        .iter()
        .zip(&cut.windows)
        .zip(&zones)
        .map(|((s, &(start, _)), z)| graph::Asked { section: s.name.clone(), zone: z.clone(), start })
        .collect();
    let gr = graph::build(x, rate, cut.grid, &asked)?;
    let end = (((gr.bar_at(gr.end) + graph::POST_EXIT_BARS * gr.bar_seconds() + 0.25) * rate as f64).round() as usize).min(x.len());
    let mut take = x[..end].to_vec();
    let fade = (0.05 * rate as f32) as usize;
    for i in 0..fade.min(end) {
        take[end - 1 - i] *= i as f32 / fade as f32;
    }
    let stem = Path::new(&job.out).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| job.name.clone());
    let dir = Path::new(&job.out).parent().map(|p| p.to_string_lossy().to_string()).filter(|p| !p.is_empty()).unwrap_or_else(|| "assets".into());
    let file = |suffix: &str| format!("{dir}/{stem}{suffix}.ogg");
    let take_facts = json!({ "graph": format!("{dir}/{stem}.music.json"), "take_bars": [gr.first, gr.end], "first_bar_seconds": gr.bar_at(0) });
    let g = audio::gain_for(&take, Kind::Music);
    let mut files = if job.layers {
        let (base, hi) = audio::split_layers(&take, rate, job.split_hz);
        for (suffix, y) in [("_take_base", base), ("_take_hi", hi)] {
            let mut o = Out::new(suffix, audio::limit(&y, g, rate), false, false);
            o.facts = take_facts.clone();
            outs.push(o);
        }
        json!({ "base": file("_take_base"), "hi": file("_take_hi") })
    } else {
        let mut o = Out::new("_take", audio::limit(&take, g, rate), false, false);
        o.facts = take_facts.clone();
        outs.push(o);
        json!({ "take": file("_take") })
    };
    let beat = 60.0 / cut.bpm as f64;
    let mut levels = None;
    if job.tension {
        // One kit for the piece: on its grid (8 bars, its 1 PREROLL in) and in its key.
        let grid = tension::Grid { len: cut.len, rate, beats, phase: (score::PREROLL * rate as f32) as usize };
        let kit = tension::kit(&grid, cut.key, job.seed);
        notes.push(format!("tension kit: synthesized in {} on the piece's grid ({beats} beats, each downbeat {:.0} ms in)", cut.key.name(), score::PREROLL * 1000.0));
        // How loud the Director plays it over this take: a loud, bright take needs the kit louder to be heard on a phone.
        let at_level: Vec<f32> = take.iter().map(|v| v * g).collect();
        let lift = tension::lift(&at_level, &kit, rate, gr.bar_seconds());
        notes.push(format!(
            "tension level: ×{:.2} on the Director's defaults (the graph's levels.tension_gain): at full danger it lifts a typical 3 s of the take by {:+.1} dB on a phone ({:+.1} dB at the defaults; mix_check wants >= +3 at its peak)",
            lift.gain, lift.db, lift.default_db
        ));
        levels = Some(json!({ "tension_gain": (lift.gain as f64 * 100.0).round() / 100.0, "tension_lift_phone_db": (lift.db as f64 * 10.0).round() / 10.0 }));
        // Its loops are seamless by construction; checked between beats, away from the downbeat's attack.
        let between = grid.phase + cut.len / beats * 3 / 8;
        let (mut t, mut k) = (Out::new("_tension", kit.tension, true, true), Out::new("_tick", kit.tick, true, true));
        (t.seam_at, k.seam_at) = (between, between);
        outs.extend([t, k, Out::new("_heartbeat", kit.heartbeat, false, true), Out::new("_sting", kit.sting, false, true)]);
        files["tension"] = json!({
            "layer": file("_tension"),
            "tick": file("_tick"),
            "heartbeat": file("_heartbeat"),
            "sting": file("_sting"),
            "bars": cut.bars,
            "downbeat_seconds": (score::PREROLL as f64 * 1e4).round() / 1e4,
        });
    }
    // A stinger into each zone (its first section), on the chord the take opens it with.
    let mut zone_extra = vec![Value::Null; gr.zones.len()];
    if job.stingers {
        let bar = (cut.grid.bar).round() as usize;
        for (i, s) in score::unique(&job.sections).iter().enumerate() {
            if s.variation_of.is_some() {
                continue;
            }
            let Some(span) = gr.spans.iter().find(|sp| sp.section == s.name) else { continue };
            let at = (gr.bar_at(span.entry) * rate as f64).round() as usize;
            let chord = score::opening_chord(&x[at.min(x.len())..], rate, bar, cut.key);
            let (y, land) = tension::stinger(rate, cut.key, beat, 2.0, &chord, job.seed ^ i as u64);
            let mut o = Out::new(&format!("_into_{}", s.name), y, false, true);
            let land_s = land as f32 / rate as f32;
            o.facts = json!({
                "into": s.name,
                "land_seconds": land_s,
                "chord": chord.iter().map(|&pc| audio::NOTE_NAMES[pc as usize]).collect::<Vec<_>>(),
            });
            outs.push(o);
            zone_extra[span.zone] = json!({ "stinger": { "file": file(&format!("_into_{}", s.name)), "land_seconds": (land_s as f64 * 1e4).round() / 1e4 } });
        }
        notes.push(format!(
            "stingers: _into_<zone>, a 2-beat riser landing on the chord the zone opens with ({:.3} s); the Director plays one when a change into that zone has no seamless link",
            2.0 * beat
        ));
    }
    // A swell that covers any other change (no stinger, or an urgent one).
    let (y, land) = tension::swell(rate, cut.key, beat, job.seed);
    let mut o = Out::new("_swell", y, false, true);
    o.facts = json!({ "land_seconds": land as f32 / rate as f32, "use": "the Director's cover for a change without a stinger of its own" });
    outs.push(o);
    files["swell"] = json!({ "file": file("_swell"), "land_seconds": ((land as f64 / rate as f64) * 1e4).round() / 1e4 });
    notes.push(format!("swell: _swell, a one-beat swell landing on the bar ({beat:.3} s), the Director's cover for changes without a stinger"));
    // What the graph found, in the log.
    let per = gr.links_per_span();
    for (s, (n, p)) in gr.spans.iter().zip(&per) {
        notes.push(format!(
            "graph: {} (bars {}–{}, entry {}): {n} jump link{} out ({p} phrase to phrase), zone threshold {:.2}",
            s.section,
            s.entry,
            s.exit,
            match s.entry as i64 - s.planned {
                0 => "where asked".to_string(),
                d => format!("moved {d:+} bar{} onto the change", if d.abs() == 1 { "" } else { "s" }),
            },
            if *n == 1 { "" } else { "s" },
            gr.zones[s.zone].threshold
        ));
    }
    for t in gr.transitions() {
        notes.push(format!(
            "graph: {} → {}: {}{} of {} exit cues have a seamless link (best {:.2}){}",
            gr.zones[t.from].name,
            gr.zones[t.to].name,
            if t.take { "the take runs into it; " } else { "" },
            t.seamless_cues,
            t.cues,
            t.best,
            if t.seamless_cues * 2 >= t.cues.max(1) { "" } else { "; the others change under its stinger or the swell" }
        ));
    }
    for r in &gr.returns {
        let s = &gr.spans[r.span];
        notes.push(if r.link {
            format!("graph: {} ends its zone at bar {}: the music jumps back into it by a link before then", s.section, r.from)
        } else {
            format!(
                "graph: {} ends its zone at bar {}: the zone starts again from its entry (bar {}) under the swell{}",
                s.section,
                r.from,
                r.to,
                r.best.map(|b| format!(" (the best link back joins at {b:.2})")).unwrap_or_default()
            )
        });
    }
    for r in &gr.risks {
        notes.push(format!("graph: listen: {r}"));
    }
    let links = gr.links.iter().filter(|l| !l.cross).count();
    facts["graph"] = json!(format!(
        "music graph: {stem}.music.json, the take {:.1} s ({} bars of 4/4 at {:.2} BPM in {}) in {} sections and {} zones, {links} jump links, {} links between zones",
        end as f64 / rate as f64,
        gr.end - gr.first,
        cut.bpm,
        cut.key.name(),
        gr.spans.len(),
        gr.zones.len(),
        gr.links.len() - links
    ));
    let mut graph = gr.to_json(&job.name, cut.bpm, files, &zone_extra);
    if let Some(l) = levels {
        graph["levels"] = l;
    }
    Ok((outs, graph))
}


/// The music's key: `--key`, else measured (mix_check.py's method), for the
/// log, the sidecar and the tension kit.
fn music_key(job: &SoundJob, x: &[f32], rate: u32, facts: &mut serde_json::Value, notes: &mut Vec<String>) -> Result<audio::Key, String> {
    let measured = audio::detect_key(x, rate);
    facts["key"] = json!(measured.name());
    facts["key_fit"] = json!((measured.fit * 100.0).round() / 100.0);
    let key = match &job.key {
        Some(k) => {
            let k = audio::Key::parse(k).ok_or(format!("--key {k:?}: give a key like \"A minor\", \"F#m\" or \"Eb\""))?;
            facts["key_given"] = json!(k.name());
            k
        }
        None => measured,
    };
    notes.push(format!(
        "key: ~{} (measured; {:.0}% of its pitched energy on that scale){}",
        measured.name(),
        measured.fit * 100.0,
        if job.key.is_some() { format!("; the tension kit uses {} (--key)", key.name()) } else { String::new() }
    ));
    Ok(key)
}

/// How much of the whole track a phone speaker loses; a warning past 10 dB.
fn mix_on_phone(x: &[f32], rate: u32, facts: &mut serde_json::Value, warnings: &mut Vec<String>) {
    let loss = audio::phone_loss_db(x, rate, false);
    facts["mix_phone_loss_db"] = json!((loss * 10.0).round() / 10.0);
    if loss < audio::PHONE_WARN_DB {
        warnings.push(format!(
            "a phone speaker loses {:.0} dB of this track (it's mostly bass under ~{:.0} Hz): ask for brighter instruments and a lighter low end (\"plucked\", \"bright\", \"no sub-bass\")",
            -loss,
            audio::PHONE_HP
        ));
    }
}

/// `--brighten` stops adding harmonics once a phone loses no more than this.
const BRIGHT_TARGET_DB: f32 = -6.0;

/// `--brighten`: if a phone would lose more than 10 dB of the effect, add
/// its bass's harmonics, the least that brings it within 6 dB.
fn brighten(x: &[f32], rate: u32, peak: bool, facts: &mut serde_json::Value, notes: &mut Vec<String>) -> Vec<f32> {
    let before = audio::phone_loss_db(x, rate, peak);
    if before >= audio::PHONE_WARN_DB {
        facts["brighten"] = json!({ "applied": false, "phone_loss_db": before });
        notes.push(format!("--brighten: not needed (a phone loses {:.1} dB of it); left as is", -before));
        return x.to_vec();
    }
    let mut best = (x.to_vec(), before, 0.0);
    for amount in [0.05, 0.08, 0.12, 0.2, 0.35, 0.6, 1.0, 1.6, 2.5] {
        let y = audio::brighten(x, rate, amount);
        let after = audio::phone_loss_db(&y, rate, peak);
        best = (y, after, amount);
        if after >= BRIGHT_TARGET_DB {
            break;
        }
    }
    facts["brighten"] = json!({ "applied": true, "amount": best.2, "phone_loss_before_db": before, "phone_loss_after_db": best.1 });
    notes.push(format!("--brighten: added the bass's harmonics ({}× its level); a phone now loses {:.1} dB of it, was {:.1}", best.2, -best.1, -before));
    best.0
}

/// The tension kit for a loop: on its beat grid and in `key`.
fn tension_kit(
    job: &SoundJob,
    lp: &[f32],
    rate: u32,
    key: audio::Key,
    facts: &mut serde_json::Value,
    warnings: &mut Vec<String>,
    notes: &mut Vec<String>,
) -> Vec<Out> {
    let (beats, phase) = match facts["bars"].as_u64() {
        Some(bars) => {
            let beats = bars as usize * 4;
            let (phase, conf) = audio::beat_phase(lp, rate, beats);
            facts["downbeat_seconds"] = json!(phase as f32 / rate as f32);
            facts["beat_confidence"] = json!((conf * 100.0).round() / 100.0);
            notes.push(format!(
                "tension kit: synthesized in {} on the loop's grid ({beats} beats, the first downbeat {:.3} s in; beat match {conf:.2})",
                key.name(),
                phase as f32 / rate as f32
            ));
            if conf < 0.15 {
                warnings.push(
                    "the beat in the loop is faint, so the tension pulse may sit off it; pass --bpm, or listen to the _tension layer against the base".into(),
                );
            }
            (beats, phase)
        }
        None => {
            let beats = ((lp.len() as f32 / rate as f32 * 2.0).round() as usize).max(1);
            warnings
                .push(format!("the loop has no steady beat, so the tension kit keeps its own pulse ({beats} beats over the loop); pass --bpm if it has one"));
            (beats, 0)
        }
    };
    let grid = tension::Grid { len: lp.len(), rate, beats, phase };
    let kit = tension::kit(&grid, key, job.seed);
    vec![
        Out::new("_tension", kit.tension, true, true),
        Out::new("_tick", kit.tick, true, true),
        Out::new("_heartbeat", kit.heartbeat, false, true),
        Out::new("_sting", kit.sting, false, true),
    ]
}

/// Cuts the best whole-bar loop (or a free-length one without a steady beat).
fn make_loop(x: &[f32], rate: u32, job: &SoundJob, facts: &mut serde_json::Value, warnings: &mut Vec<String>) -> Result<Vec<f32>, String> {
    let secs = x.len() as f32 / rate as f32;
    let skip = (if secs > 20.0 { 1.5 } else { 0.5 } * rate as f32) as usize;
    let xfade = (0.03 * rate as f32) as usize;
    let tail = 1.0 * rate as f32; // leave the last second (fade-outs)
    let room = (x.len() as f32 - skip as f32 - tail).max(0.0) / rate as f32;
    let detected = if job.bpm.is_none() && job.seconds.is_none() { audio::detect_bpm(x, rate) } else { None };
    let beat = match (job.seconds, job.bpm, detected) {
        (Some(_), _, _) => None,
        (None, Some(b), _) => Some((b, "given".to_string())),
        (None, None, Some((b, conf))) if conf >= 0.08 => Some((b, format!("detected; confidence {conf:.2}"))),
        (None, None, d) => {
            warnings.push(format!(
                "no steady beat found{}; cut a free-length loop (pass --bpm if it has one)",
                d.map(|(b, c)| format!(" (best guess {b:.0} BPM, confidence {c:.2})")).unwrap_or_default()
            ));
            None
        }
    };
    let (start, len, score, label, bake) = match beat {
        Some((bpm, how)) => {
            let bar = 4.0 * 60.0 / bpm;
            let mut bars = job.bars.unwrap_or(8).max(1);
            while bars > 1 && bars as f32 * bar > room * 0.9 {
                bars /= 2;
            }
            if (bars as f32 * bar) > room {
                return Err(format!("the track is {secs:.1} s, too short for a bar at {bpm:.0} BPM; pass --seconds"));
            }
            if job.bars.is_some_and(|b| b != bars) {
                warnings.push(format!("only {bars} bars fit in the {secs:.0} s source (asked for {})", job.bars.unwrap()));
            }
            let len = (bars as f32 * bar * rate as f32).round() as usize;
            // Room for the baked crossfade after the loop if there is any.
            let bake = (job.loop_xfade_bars.unwrap_or(1.0) * bar * rate as f32) as usize;
            let (s, l, score) = audio::find_loop(x, rate, len, len / 250, skip, bake)
                .or_else(|| audio::find_loop(x, rate, len, len / 250, skip, xfade))
                .ok_or("couldn't find a loop in the track")?;
            let final_bpm = bars as f32 * 4.0 * 60.0 * rate as f32 / l as f32;
            facts["bars"] = json!(bars);
            facts["bpm"] = json!(final_bpm);
            let label = format!(
                "loop: {:.3} s = {bars} bars of 4/4 at {final_bpm:.2} BPM ({how}), from {:.2} s into the source; match {score:.2}",
                l as f32 / rate as f32,
                s as f32 / rate as f32
            );
            (s, l, score, label, bake)
        }
        None => {
            let want = job.seconds.unwrap_or(16.0).min(room);
            if want < 2.0 {
                return Err(format!("the track is {secs:.1} s, too short to loop"));
            }
            let len = (want * rate as f32) as usize;
            let (s, l, score) = audio::find_loop(x, rate, len, len / 20, skip, xfade).ok_or("couldn't find a loop in the track")?;
            let label = format!("loop: {:.3} s (free length), from {:.2} s into the source; match {score:.2}", l as f32 / rate as f32, s as f32 / rate as f32);
            (s, l, score, label, (rate as usize).min(l / 4))
        }
    };
    if score < 0.6 {
        warnings.push(format!("the best loop point is a weak match ({score:.2}); it may jump audibly. Regenerate or change --bars"));
    }
    facts["loop_start_seconds"] = json!(start as f32 / rate as f32);
    facts["loop_seconds"] = json!(len as f32 / rate as f32);
    facts["loop_samples"] = json!(len);
    facts["loop_match"] = json!(score);
    facts["loop"] = json!(label);
    // The music after the loop's end baked into its start (equal power), as far as the take goes on.
    let (lp, xf) = audio::bake_loop(x, start, len, bake);
    facts["xfade_seconds"] = json!((xf as f32 / rate as f32 * 1000.0).round() / 1000.0);
    Ok(lp)
}

pub fn regen(a: &super::RegenArgs, sc: &Sidecar) -> Result<(), String> {
    let mut job: SoundJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("sidecar recipe: {e}"))?;
    let game = super::game_of(&a.asset, &job.out);
    if let Some(s) = a.seed {
        job.seed = s;
    } else if a.new_seed {
        job.seed = super::fresh_seed();
    }
    if let Some(p) = &a.prompt {
        job.subject = p.clone();
    }
    if let Some(p) = &a.provider {
        job.provider = p.clone();
        if a.model.is_none() {
            let media = if job.kind == "music" { Media::Music } else { Media::Sfx };
            job.model = provider::default_model(p, media).map(|m| m.id.to_string()).unwrap_or_default();
        }
    }
    if let Some(m) = &a.model {
        job.model = m.clone();
    }
    if a.brighten || a.no_brighten {
        job.brighten = a.brighten;
    }
    let relayer = a.layers.is_some() || a.add_layers.is_some();
    if let Some(l) = &a.layers {
        (job.layers, job.tension) = parse_layers(Some(l))?;
    }
    if let Some(l) = &a.add_layers {
        let (layers, tension) = parse_layers(Some(l))?;
        job.layers |= layers;
        job.tension |= tension;
    }
    if job.tension && job.once {
        return Err("tension layers loop with the track, and this is a one-shot (--once)".into());
    }
    // New layers for the take you have: no new take unless one was asked for.
    let new_take = a.seed.is_some() || a.new_seed || a.prompt.is_some() || a.provider.is_some() || a.model.is_some();
    let reprocess = a.reprocess || (relayer && !new_take && game.join(&sc.raw).exists());
    if reprocess && !a.reprocess {
        eprintln!("  new layers from the take you have ({}; no provider call). Pass --new-seed for a new take.", sc.raw);
    }
    let raw = if reprocess { Some(load_raw(&game, sc)?) } else { None };
    execute(&game, &job, a.dry_run, super::max_cost(a.max_cost), raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> SoundJob {
        SoundJob {
            kind: "music".into(),
            name: "m".into(),
            subject: "dreamy harp".into(),
            out: "assets/m.ogg".into(),
            provider: "mock".into(),
            model: "mock-music".into(),
            seed: 1,
            bars: Some(4),
            bpm: None,
            seconds: None,
            layers: false,
            tension: false,
            key: None,
            brighten: false,
            split_hz: 700.0,
            once: false,
            looping: false,
            negative: None,
            sections: vec![],
            stingers: false,
            retries: 0,
            section_seconds: None,
            loop_xfade_bars: None,
            downbeat_shift: None,
            loops: false,
            section_clips: Default::default(),
            fix_below: None,
            rate: 22050,
            quality: 4.0,
            no_style: false,
            allow_names: vec![],
        }
    }

    #[test]
    fn prompts() {
        let mut j = job();
        j.bpm = Some(120.0);
        let p = compose(&j, &Style::default());
        for want in ["dreamy harp.", "120 BPM", "Instrumental only", "loop seamlessly", "An original composition"] {
            assert!(p.contains(want), "missing {want:?} in {p}");
        }
        j.kind = "sfx".into();
        j.seconds = Some(0.4);
        j.looping = true;
        let p = compose(&j, &Style::default());
        assert!(p.contains("about 0.4 seconds") && p.contains("loops seamlessly"));
    }

    #[test]
    fn songs_per_zone_parse_with_their_tempos() {
        let z = parse_song_zones(&["Harbor=\"bluesy hard rock, 128 bpm\"".into(), "neon=flamenco-tango at 112BPM".into(), "yard=junkyard polka".into()]).unwrap();
        assert_eq!(z[0], ("harbor".into(), "bluesy hard rock, 128 bpm".into(), Some(128.0)));
        assert_eq!((z[1].2, z[2].2), (Some(112.0), None));
        assert_eq!(parse_song_zones(&["a=x".into()]).unwrap().len(), 1, "one zone at a time adds to the setlist");
        assert!(parse_song_zones(&["a=x".into(), "a=y".into()]).is_err());
        assert!(parse_song_zones(&["a".into(), "b=y".into()]).is_err());
        assert_eq!(tempo_in("dub, 9000 bpm"), None);
    }

    #[test]
    fn a_setlist_merges_with_the_one_before() {
        let before = json!([{ "zone": "harbor", "bpm": 128 }, { "zone": "neon", "bpm": 112 }]);
        let (songs, kept) = merge_setlist(Some(&before), vec![json!({ "zone": "neon", "bpm": 100 }), json!({ "zone": "yard", "bpm": 90 })]);
        let zones: Vec<_> = songs.iter().map(|s| (s["zone"].as_str().unwrap(), s["bpm"].as_u64().unwrap())).collect();
        assert_eq!(zones, vec![("harbor", 128), ("neon", 100), ("yard", 90)], "neon replaced in place, yard added");
        assert_eq!(kept, vec!["harbor"]);
        let (songs, kept) = merge_setlist(None, vec![json!({ "zone": "a" })]);
        assert_eq!((songs.len(), kept.len()), (1, 0));
    }

    #[test]
    fn a_refusal_names_the_words_that_likely_tripped_it() {
        let e = "gemini: no audio in the response (status completed). A safety filter may have blocked the prompt: describe the subject differently.";
        let (words, softer) = refusal_hint(e, "stomping industrial metal, greedy swagger, warm bass, 120 bpm").unwrap();
        assert_eq!(words, vec!["stomping", "greedy", "swagger"]);
        assert_eq!(softer, "heavy industrial metal, brash confident, warm bass, 120 bpm");
        let (words, softer) = refusal_hint("elevenlabs: HTTP 400: bad_prompt", "a war drum, killer groove, guns blazing").unwrap();
        assert_eq!(words, vec!["war", "killer", "guns"]);
        assert_eq!(softer, "a epic drum, groove, blazing");
        assert!(refusal_hint(e, "calm lute and harp").is_none(), "nothing stands out");
        assert!(refusal_hint("can't reach api: connection reset", "stomping metal").is_none(), "not a refusal");
        assert!(is_refusal("Maimbrain's keys: the prompt violates the content policy"));
    }

    #[test]
    fn loops_that_dont_fit_a_30_s_clip_get_fewer_bars_then_a_longer_model() {
        let mut job = job();
        (job.provider, job.bars, job.bpm, job.model) = ("vertex".into(), Some(16), Some(128.0), "lyria-002".into());
        assert_eq!(clip_bars(&job), Some(12), "16 bars at 128 BPM are 30 s: no room");
        job.bpm = Some(90.0);
        assert_eq!(clip_bars(&job), Some(8));
        job.model = "lyria-3.5".into();
        assert_eq!(clip_bars(&job), None, "full-length songs fit");
        (job.provider, job.model) = ("gemini".into(), "lyria-3-clip-preview".into());
        assert_eq!(loop_fallback(&mut job, false).as_deref(), Some("lyria-3.5 (full-length songs)"));
        assert_eq!(loop_fallback(&mut job, false).as_deref(), Some("8 bars a loop and a new seed"));
        job.bars = Some(4);
        assert_eq!(loop_fallback(&mut job, false), None);
    }

    #[test]
    fn loop_from_mock_song_is_whole_bars() {
        let rate = 22050;
        let x = super::super::mock::song(112.0, 30.0, rate, 5);
        let mut facts = json!({});
        let mut warnings = Vec::new();
        let lp = make_loop(&x, rate, &job(), &mut facts, &mut warnings).unwrap();
        let bpm = facts["bpm"].as_f64().unwrap();
        assert!((bpm - 112.0).abs() < 1.5, "{bpm}");
        assert_eq!(facts["bars"], 4);
        assert!(audio::seam_ratio(&lp) < 4.0);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
