//! `mb voice`: voice acting. A character's voice is designed from a
//! description (never cloned from a real person), kept in the game's
//! `art/voices.toml`, and speaks lines that are post-processed for a phone
//! (mono Ogg, dialogue loudness, trimmed, a high-pass and a presence boost)
//! with a `.lips.txt` next to each: the mouth-open envelope at 30 Hz and when
//! each character of the subtitle is spoken, for `ui::Portrait` and
//! `ui::Dialogue` (docs/UI.md).
//!
//!     mb voice design games/duel rocko "a smug rock-star mech pilot, 30s, raspy, drawling, full of himself"
//!     mb voice pick games/duel rocko 2                  # keep preview 2 (listen to art/voices/rocko/preview_*.ogg)
//!     mb voice say games/duel rocko taunt1 "Is that all you got?" --emotion smug
//!     mb voice say games/duel rocko hit1 "Ow! My paint job!" --emotion angry --variants 3
//!     mb voice keep games/duel rocko hit1 2
//!     mb voice script games/duel lines.csv              # every line, only what's missing or changed
//!     mb voice list games/duel
//!     mb voice lengths games/duel                       # src/vo_lengths.rs: each line's length (say, script and keep write it too)
//!     mb voice slots                                    # the account's voice slots and which game uses which (voice_account.rs)
//!     mb voice cast games/duel announcer --premade Adam # a premade voice: no slot
//!
//! Layout: `art/voices.toml` (the cast), `art/voices/<who>/preview_N.ogg`,
//! `assets/vo/<who>/<line>.ogg` + `.ogg.gen.json` + `.lips.txt`, and the voice
//! itself as audio in `art/voice_ref/<who>/` (to recreate it once released).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::audio::{self, Biquad, BUTTER4_Q};
use super::provider::{self, Media};
use super::voice_account::Speakers;
use super::{FileRef, Sidecar};

// --- the provider side -------------------------------------------------------------

pub struct DesignRequest {
    pub model: String,
    pub description: String,
    /// What the previews say (100–1000 characters for ElevenLabs); None lets the provider write it.
    pub text: Option<String>,
    pub seed: Option<u64>,
}

pub struct Preview {
    /// The provider's id for this preview (kept with `create`).
    pub id: String,
    pub bytes: Vec<u8>,
    pub mime: String,
}

pub struct SpeechRequest {
    pub model: String,
    pub voice_id: String,
    /// What's sent, audio tags included ("[smug] Is that all you got?").
    pub text: String,
    pub stability: f32,
    pub similarity: f32,
    pub style: Option<f32>,
    pub speed: Option<f32>,
    pub seed: Option<u64>,
}

/// When each character of the text sent was spoken, in seconds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Alignment {
    pub chars: Vec<String>,
    pub start: Vec<f32>,
    pub end: Vec<f32>,
}

pub struct Speech {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub alignment: Option<Alignment>,
    pub cost_usd: Option<f64>,
}

/// How many custom voices the account holds and may hold (ElevenLabs: designed,
/// cloned and library voices each take a slot; premade voices don't).
#[derive(Clone, Debug, Default)]
pub struct Slots {
    pub tier: String,
    pub used: u32,
    pub limit: u32,
    /// Voices added or edited this billing period, and the most allowed.
    pub add_edits: Option<(u32, u32)>,
    /// Whether the plan allows Instant Voice Cloning (recreating a released voice).
    pub can_clone: Option<bool>,
}

impl Slots {
    pub fn free(&self) -> u32 {
        self.limit.saturating_sub(self.used)
    }
}

/// A voice on the account (or a premade one).
#[derive(Clone, Debug, Default)]
pub struct AccountVoice {
    pub id: String,
    pub name: String,
    /// generated (Voice Design), cloned, professional, premade…
    pub category: String,
    /// gender, age, accent, descriptive, use_case…
    pub labels: BTreeMap<String, String>,
    pub description: String,
    pub created_unix: Option<i64>,
}

pub trait VoiceProvider {
    /// Previews of a new voice from a description, and the text they say.
    fn design(&self, req: &DesignRequest) -> Result<(Vec<Preview>, String), String>;
    /// Keeps a preview as a voice; returns its id.
    fn create(&self, generated_id: &str, name: &str, description: &str) -> Result<String, String>;
    fn speak(&self, req: &SpeechRequest) -> Result<Speech, String>;
    /// The account's voice slots.
    fn slots(&self) -> Result<Slots, String>;
    /// The account's own voices (the ones that take slots), or with `premade` the provider's stock voices.
    fn voices(&self, premade: bool) -> Result<Vec<AccountVoice>, String>;
    /// Recreates a released voice from its reference audio (ElevenLabs Instant
    /// Voice Cloning); returns the new voice's id. Only ever given audio mb
    /// made with the game's own designed voice (voice_account::reference_audio
    /// checks every file's hash), never a recording of a person.
    fn clone_voice(&self, name: &str, description: &str, files: &[RefAudio]) -> Result<String, String>;
    fn delete_voice(&self, id: &str) -> Result<(), String>;
}

/// An audio file to recreate a voice from.
#[derive(Debug)]
pub struct RefAudio {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Offline voices: babble with the line's rhythm (vowels voiced, consonants
/// short noise, pauses at spaces and punctuation) and exact timings, so the
/// whole pipeline and the lip flap run without a key. Same text, same bytes.
pub struct MockVoice;

const MOCK_RATE: u32 = 24000;

pub fn babble(text: &str, voice: &str) -> (Vec<f32>, Alignment) {
    let h = super::mock::hash(&[voice.as_bytes()]);
    let f0 = 95.0 + (h % 160) as f32;
    let r = MOCK_RATE as f32;
    let mut x: Vec<f32> = vec![0.0; (0.08 * r) as usize];
    let mut al = Alignment::default();
    let mut noise = h | 1;
    let mut phase = 0.0f32;
    for (i, c) in text.chars().enumerate() {
        let lc = c.to_ascii_lowercase();
        let (secs, voiced, amp) = if "aeiouy".contains(lc) {
            (0.09, true, 0.5)
        } else if "mnlrwbdgvz".contains(lc) {
            (0.05, true, 0.2)
        } else if c.is_alphanumeric() {
            (0.05, false, 0.15)
        } else if ".,!?;:".contains(c) {
            (0.16, false, 0.0)
        } else {
            (0.05, false, 0.0)
        };
        al.chars.push(c.to_string());
        al.start.push(x.len() as f32 / r);
        let n = (secs * r) as usize;
        for k in 0..n {
            let env = ((k as f32 / (0.01 * r)).min(1.0)) * (((n - k) as f32 / (0.01 * r)).min(1.0));
            let f = f0 * (1.0 + 0.05 * ((i as f32 * 0.7).sin()));
            phase = (phase + f / r).fract();
            let s = if voiced {
                (1..10).map(|m| (std::f32::consts::TAU * phase * m as f32).sin() / m as f32).sum::<f32>() * 0.5
            } else {
                noise ^= noise << 13;
                noise ^= noise >> 7;
                noise ^= noise << 17;
                (noise >> 40) as f32 / (1u64 << 23) as f32 - 1.0
            };
            x.push(s * amp * env);
        }
        al.end.push(x.len() as f32 / r);
    }
    x.extend(std::iter::repeat_n(0.0, (0.1 * r) as usize));
    (x, al)
}

impl VoiceProvider for MockVoice {
    fn design(&self, req: &DesignRequest) -> Result<(Vec<Preview>, String), String> {
        let text = req.text.clone().unwrap_or_else(|| "Well well well. Look who finally showed up. Ready to lose?".into());
        let h = super::mock::hash(&[req.description.as_bytes(), &req.seed.unwrap_or(0).to_le_bytes()]);
        Ok((
            (1..=3)
                .map(|i| {
                    let id = format!("mock-{:x}-{i}", h % 0xffff_ffff);
                    let (x, _) = babble(&text, &id);
                    Preview { id, bytes: audio::wav_bytes(&x, MOCK_RATE), mime: "audio/wav".into() }
                })
                .collect(),
            text,
        ))
    }

    fn create(&self, generated_id: &str, _name: &str, _description: &str) -> Result<String, String> {
        Ok(generated_id.to_string())
    }

    fn speak(&self, req: &SpeechRequest) -> Result<Speech, String> {
        let (x, al) = babble(&subtitle(&req.text), &req.voice_id);
        Ok(Speech { bytes: audio::wav_bytes(&x, MOCK_RATE), mime: "audio/wav".into(), alignment: Some(al), cost_usd: Some(0.0) })
    }

    fn slots(&self) -> Result<Slots, String> {
        Ok(Slots { tier: "mock".into(), used: 0, limit: 10, add_edits: None, can_clone: Some(true) })
    }

    fn voices(&self, premade: bool) -> Result<Vec<AccountVoice>, String> {
        if !premade {
            return Ok(Vec::new());
        }
        let v = |name: &str, gender: &str, age: &str, accent: &str, what: &str| AccountVoice {
            id: format!("mock-premade-{}", name.to_lowercase()),
            name: name.into(),
            category: "premade".into(),
            labels: [("gender", gender), ("age", age), ("accent", accent), ("descriptive", what)].iter().map(|(k, x)| (k.to_string(), x.to_string())).collect(),
            description: String::new(),
            created_unix: None,
        };
        Ok(vec![
            v("Aria", "female", "young", "american", "bright"),
            v("Bram", "male", "middle_aged", "british", "gruff"),
            v("Cora", "female", "old", "irish", "warm"),
        ])
    }

    fn clone_voice(&self, name: &str, _description: &str, files: &[RefAudio]) -> Result<String, String> {
        if files.is_empty() {
            return Err("mock: no audio to recreate the voice from".into());
        }
        Ok(format!("mock-clone-{:x}", super::mock::hash(&[name.as_bytes(), &files[0].bytes]) % 0xffff_ffff))
    }

    fn delete_voice(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }
}

/// The voice provider for a name, with your own key (Maimbrain's keys don't cover voices).
pub fn pick(name: &str) -> Result<Box<dyn VoiceProvider>, String> {
    match name {
        "mock" => Ok(Box::new(MockVoice)),
        "elevenlabs" => {
            let (key, from) = super::keys::lookup("elevenlabs")
                .ok_or_else(|| format!("no ElevenLabs key for voices: {}\nor try the pipeline offline with --provider mock", super::keys::help_for("elevenlabs")))?;
            eprintln!("  keys: your elevenlabs key (from {from})");
            Ok(Box::new(super::elevenlabs::ElevenLabs::new(key, Box::new(provider::Http)).with_slots(super::elevenlabs::slots_dir())))
        }
        _ => Err(format!("{name} doesn't make voices (elevenlabs, or mock to try it offline)")),
    }
}

pub fn choose(flag: Option<&str>, dry_run: bool) -> Result<String, String> {
    if let Some(p) = flag {
        return Ok(p.to_string());
    }
    if let Ok(p) = std::env::var("MB_PROVIDER")
        && (p == "mock" || p == "elevenlabs")
    {
        return Ok(p);
    }
    if super::keys::lookup("elevenlabs").is_some() || dry_run {
        return Ok("elevenlabs".into());
    }
    Err(format!("no provider key for voices. Set one up:\n  elevenlabs: {}\nor try the pipeline offline with --provider mock", super::keys::help_for("elevenlabs")))
}

// --- originality ----------------------------------------------------------------------

/// Voices are designed, never copied: no cloning, no impressions, no "sounds
/// like <a real person>" (a voice is part of someone's likeness). The names
/// list of mb art applies too. A capitalized word after "like", "voice of",
/// "voiced by"… is taken as a name; `--allow-name` lets your own character's
/// name through ("sounds like Rocko, but older").
pub fn check_voice_text(texts: &[&str], allow: &[String]) -> Result<Vec<String>, String> {
    let v = super::ip::check(texts, allow);
    if !v.ok() {
        return Err(super::ip::refusal_message(&v));
    }
    for t in texts {
        let lower = t.to_lowercase();
        for w in ["clone", "cloned", "cloning", "soundalike", "sound-alike", "impersonat", "voice match", "deepfake"] {
            if lower.contains(w) {
                return Err(format!(
                    "mb voice designs original voices: it won't clone, impersonate or match anyone's voice ({w:?} in {t:?}). \
                     Describe the voice instead: age, accent, pitch, texture, energy, attitude"
                ));
            }
        }
        let words: Vec<&str> = t.split_whitespace().collect();
        for (i, w) in words.iter().enumerate() {
            let wl = w.to_lowercase();
            let wl = wl.trim_matches(|c: char| !c.is_alphanumeric());
            let two = words.get(i + 1).map(|n| format!("{wl} {}", n.to_lowercase())).unwrap_or_default();
            let skip = if matches!(wl, "like" | "imitating" | "imitate" | "mimicking" | "channeling" | "channelling") {
                1
            } else if matches!(two.as_str(), "voice of" | "voiced by" | "impression of" | "style of" | "voice like") {
                2
            } else {
                0
            };
            if skip == 0 {
                continue;
            }
            let Some(next) = words.get(i + skip) else { continue };
            let name = next.trim_matches(|c: char| !c.is_alphanumeric());
            if name.chars().next().is_some_and(char::is_uppercase) && !allow.iter().any(|a| a.eq_ignore_ascii_case(name)) {
                return Err(format!(
                    "{t:?} asks for someone's voice ({name:?} after {:?}). mb voice makes original voices only: describe it \
                     (\"a raspy 60-year-old with a Texas drawl, slow and amused\") instead of naming a person or character. \
                     If {name:?} is your own character or a place, pass --allow-name {name}",
                    words[i..i + skip].join(" ")
                ));
            }
        }
    }
    Ok(v.allowed)
}

// --- the cast: art/voices.toml ---------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PreviewEntry {
    pub id: String,
    pub file: String,
    /// How long it is, trimmed (the same text in each preview: a slower voice runs longer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f32>,
    /// Median pitch of the voiced parts, Hz (a deep voice ~90–130, a high one 220+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_hz: Option<f32>,
    /// Loudness as the provider delivered it, dBFS (before mb's leveling to −16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness_db: Option<f32>,
    /// How much a phone speaker loses of it, dB (below −10 is a warning: too deep for a phone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_loss_db: Option<f32>,
}

/// The median pitch of a voice's voiced frames (YIN on 40 ms frames at 8 kHz, 60–500 Hz), if it has any.
pub fn median_pitch(x: &[f32], rate: u32) -> Option<f32> {
    let sr = 8000u32;
    let y = audio::resample(x, rate, sr);
    let (n, hop) = (320usize, 160usize);
    let (min_lag, max_lag) = ((sr / 500) as usize, (sr / 60) as usize);
    let frames: Vec<&[f32]> = (0..y.len().saturating_sub(n + max_lag)).step_by(hop).map(|i| &y[i..i + n + max_lag]).collect();
    let energy = |f: &[f32]| (f[..n].iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt();
    let loudest = frames.iter().map(|f| energy(f)).fold(0.0f32, f32::max);
    let mut f0s = Vec::new();
    for f in frames {
        if energy(f) < (loudest * 0.15).max(1e-4) {
            continue;
        }
        // The cumulative-mean-normalized difference; the first dip under 0.15 is the period.
        let d: Vec<f32> = (0..=max_lag).map(|t| (0..n).map(|j| (f[j] - f[j + t]).powi(2)).sum()).collect();
        let mut run = 0.0f32;
        let mut best: Option<(usize, f32)> = None;
        for t in 1..=max_lag {
            run += d[t];
            let c = d[t] * t as f32 / run.max(1e-12);
            if t < min_lag {
                continue;
            }
            if c < 0.15 {
                // Down to the bottom of this dip.
                let mut k = t;
                while k < max_lag && d[k + 1] * (k + 1) as f32 / (run + d[k + 1]).max(1e-12) < c {
                    k += 1;
                }
                best = Some((k, c));
                break;
            }
            if best.is_none_or(|b| c < b.1) {
                best = Some((t, c));
            }
        }
        if let Some((t, c)) = best
            && c < 0.3
        {
            f0s.push(sr as f32 / t as f32);
        }
    }
    if f0s.len() < 3 {
        return None;
    }
    f0s.sort_by(f32::total_cmp);
    Some(f0s[f0s.len() / 2])
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct VoiceEntry {
    pub description: String,
    pub provider: String,
    /// The voice to speak with, once a preview is kept (`mb voice pick`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
    /// Which preview it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picked: Option<usize>,
    pub design_model: String,
    pub seed: u64,
    #[serde(default)]
    pub sample_text: String,
    #[serde(default)]
    pub previews: Vec<PreviewEntry>,
    /// Speech defaults for this character's lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    /// Style exaggeration 0–1 for this character's lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<f32>,
    /// Cast with one of the provider's premade voices (`mb voice cast --premade`): its name. Takes no slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premade: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_names: Vec<String>,
    pub date: String,
    /// The voice's own audio, kept in the game (art/voice_ref/<who>/) so it
    /// can be recreated once its slot is freed: the picked preview and the
    /// first lines it spoke. Each file's hash is checked before it's used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference: Vec<RefFile>,
    /// Set by `mb voice release`: the voice may leave the account, and lines are spoken with a recreation of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<Released>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RefFile {
    pub file: String,
    pub sha256: String,
    /// "preview 2", "line taunt1"
    pub from: String,
    pub seconds: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Released {
    pub date: String,
    /// The voice was deleted from the account (`--delete`, or `mb voice gc`).
    #[serde(default)]
    pub deleted: bool,
    /// A recreation kept on the account with `--keep`, reused by the next batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_voice_id: Option<String>,
}

fn voices_path(game: &Path) -> PathBuf {
    game.join("art/voices.toml")
}

pub fn load_voices(game: &Path) -> Result<BTreeMap<String, VoiceEntry>, String> {
    let p = voices_path(game);
    match std::fs::read_to_string(&p) {
        Ok(t) => toml::from_str(&t).map_err(|e| format!("{}: {e}", p.display())),
        Err(_) => Ok(BTreeMap::new()),
    }
}

pub fn save_voices(game: &Path, v: &BTreeMap<String, VoiceEntry>) -> Result<(), String> {
    super::ensure_art_dir(game)?;
    let p = voices_path(game);
    let head = "# The cast's voices (mb voice design / pick). Designed from descriptions, not cloned.\n\n";
    std::fs::write(&p, format!("{head}{}", toml::to_string_pretty(v).map_err(|e| e.to_string())?)).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn check_name(what: &str, s: &str) -> Result<(), String> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("bad {what} {s:?}: use letters, digits, - and _ (it's a file name)"));
    }
    Ok(())
}

// --- the command -----------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum VoiceCmd {
    /// Design a character's voice from a description: writes previews to listen to.
    Design(DesignArgs),
    /// Keep one of a character's previews as its voice.
    Pick {
        game: PathBuf,
        character: String,
        /// The preview's number (art/voices/<character>/preview_N.ogg).
        n: usize,
    },
    /// Speak a line in a character's voice: assets/vo/<character>/<line>.ogg + .lips.txt.
    Say(SayArgs),
    /// Keep one of a line's --variants.
    Keep {
        game: PathBuf,
        character: String,
        line: String,
        /// The variant's number.
        n: usize,
    },
    /// Speak every line of a script (.csv or .toml), only those missing or changed.
    Script(ScriptArgs),
    /// The cast and their lines.
    List { game: PathBuf },
    /// Rewrite src/vo_lengths.rs, each recorded line's length as a constant (say, script and keep do it too).
    Lengths { game: PathBuf },
    /// Your ElevenLabs voice slots: how many are used, and which games here use which voice.
    Slots(super::voice_account::SlotsArgs),
    /// Free a character's voice slot: its voice is kept in the game as audio and recreated when it speaks.
    Release(super::voice_account::ReleaseArgs),
    /// List (and with --yes delete) the account's voices no game here speaks with.
    Gc(super::voice_account::GcArgs),
    /// Cast a character with one of the provider's premade voices (no voice slot needed).
    Cast(super::voice_account::CastArgs),
    /// The provider's premade voices, with their gender, age and accent.
    Premades(super::voice_account::PremadesArgs),
}

#[derive(Args)]
pub struct DesignArgs {
    pub game: PathBuf,
    /// Who (a file name: rocko, granny).
    pub character: String,
    /// The voice: age, gender, accent, pitch, texture, energy, attitude. Never a real person.
    pub description: String,
    /// What the previews say (100–1000 characters); default: the provider writes something fitting.
    #[arg(long)]
    pub sample: Option<String>,
    /// Keep preview N right away (skip listening).
    #[arg(long)]
    pub pick: Option<usize>,
    /// elevenlabs (default) or mock.
    #[arg(long)]
    pub provider: Option<String>,
    /// Voice design model (default eleven_ttv_v3).
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub seed: Option<u64>,
    #[arg(long)]
    pub allow_name: Vec<String>,
    /// Design even when the account has no free voice slot (previews don't take one; keeping one does).
    #[arg(long)]
    pub ignore_slots: bool,
    #[arg(long)]
    pub max_cost: Option<f64>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Clone, Default)]
pub struct LineOpts {
    /// Speak with another provider than the voice was designed with: `mock` makes stand-in lines with real timings, to try the scene before paying for speech.
    #[arg(long)]
    pub provider: Option<String>,
    /// How it's said: an audio tag the model acts (angry, smug, scared, excited, laughing, whispering, shouting, sarcastic…).
    #[arg(long)]
    pub emotion: Option<String>,
    /// creative (most expressive, least steady), natural (default) or robust (steadiest).
    #[arg(long)]
    pub delivery: Option<String>,
    /// Style exaggeration 0–1 (models that take it).
    #[arg(long)]
    pub style: Option<f32>,
    /// Speaking speed 0.7–1.2.
    #[arg(long)]
    pub speed: Option<f32>,
    /// Speech model (default eleven_v4; `mb models`).
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub seed: Option<u64>,
    /// Sample rate of the Ogg (22050–48000).
    #[arg(long, default_value_t = 44100)]
    pub rate: u32,
    #[arg(long, default_value_t = 4.0)]
    pub quality: f32,
    #[arg(long)]
    pub allow_name: Vec<String>,
    /// For a released character: keep the voice recreated for this batch on the account afterwards
    /// (it takes a slot), so later batches reuse it instead of recreating it.
    #[arg(long)]
    pub keep: bool,
    #[arg(long)]
    pub max_cost: Option<f64>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SayArgs {
    pub game: PathBuf,
    pub character: String,
    /// The line's id (a file name: taunt1, hit_3).
    pub line: String,
    /// What they say. Audio tags in brackets act it ("[laughs] Too slow!"); the subtitle leaves them out.
    pub text: String,
    /// Make N takes in art/variants/vo/ to compare; keep one with `mb voice keep`.
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    #[command(flatten)]
    pub opts: LineOpts,
}

#[derive(Args)]
pub struct ScriptArgs {
    pub game: PathBuf,
    /// A .csv (character,line,text[,emotion]) or .toml ([[line]] character, id, text, emotion).
    pub script: PathBuf,
    /// Speak every line again, changed or not.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub opts: LineOpts,
}

pub fn run(cmd: VoiceCmd) -> Result<(), String> {
    match cmd {
        VoiceCmd::Design(a) => design(a),
        VoiceCmd::Pick { game, character, n } => pick_preview(&game, &character, n, true),
        VoiceCmd::Say(a) => say(a),
        VoiceCmd::Keep { game, character, line, n } => keep(&game, &character, &line, n),
        VoiceCmd::Script(a) => script(a),
        VoiceCmd::List { game } => list(&game),
        VoiceCmd::Lengths { game } => {
            if !game.join("src").is_dir() {
                return Err(format!("{} has no src/ (pass the game directory)", game.display()));
            }
            refresh_lengths(&game)?;
            eprintln!("{}: {} line lengths, up to date", game.join(LENGTHS_FILE).display(), line_lengths(&game).len());
            Ok(())
        }
        VoiceCmd::Slots(a) => super::voice_account::slots(a),
        VoiceCmd::Release(a) => super::voice_account::release(a),
        VoiceCmd::Gc(a) => super::voice_account::gc(a),
        VoiceCmd::Cast(a) => super::voice_account::cast(a),
        VoiceCmd::Premades(a) => super::voice_account::premades(a),
    }
}

fn design(a: DesignArgs) -> Result<(), String> {
    check_name("character", &a.character)?;
    if !a.game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", a.game.display()));
    }
    let allowed = check_voice_text(&[&a.description, a.sample.as_deref().unwrap_or("")], &a.allow_name)?;
    if let Some(s) = &a.sample
        && !(100..=1000).contains(&s.chars().count())
    {
        return Err(format!("--sample is {} characters; the previews need 100–1000", s.chars().count()));
    }
    let prov_name = choose(a.provider.as_deref(), a.dry_run)?;
    let model = a.model.clone().unwrap_or_else(|| if prov_name == "mock" { "mock-voice".into() } else { "eleven_ttv_v3".into() });
    let m = provider::find_model(&prov_name, &model).ok_or(format!("unknown voice design model {prov_name} {model} (see `mb models`)"))?;
    let seed = a.seed.unwrap_or_else(super::fresh_seed);
    eprintln!("voice {} → art/voices.toml", a.character);
    eprintln!("  {prov_name} {model} · seed {seed}");
    let est = provider::estimate_chars(m, 0);
    super::check_cost(est, 1, super::max_cost(a.max_cost), "the previews")?;
    if a.dry_run {
        eprintln!("  description: {}\n  (dry run: nothing generated)", a.description);
        return Ok(());
    }
    let prov = pick(&prov_name)?;
    // Keeping a voice takes a slot: say so before paying for previews that can't be kept.
    if !a.ignore_slots {
        super::voice_account::check_free_slot(prov.as_ref(), &a.game, &a.character, true)?;
    }
    let (previews, text) = prov.design(&DesignRequest { model: model.clone(), description: a.description.clone(), text: a.sample.clone(), seed: Some(seed) })?;
    let art = super::ensure_art_dir(&a.game)?;
    let dir = art.join("voices").join(&a.character);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(art.join("raw/voices")).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (i, p) in previews.iter().enumerate() {
        let n = i + 1;
        let ext = ext_for(&p.mime);
        std::fs::write(art.join(format!("raw/voices/{}_preview_{n}.{ext}", a.character)), &p.bytes).map_err(|e| e.to_string())?;
        let (chans, rate) = audio::decode(&p.bytes, &p.mime).map_err(|e| format!("preview {n}: {e}"))?;
        let x = audio::resample(&audio::to_mono(&chans), rate, 44100);
        let (y, _) = process(&x, 44100);
        let dest = dir.join(format!("preview_{n}.ogg"));
        let decoded = audio::encode_ogg(&y, 44100, 4.0, &dest)?;
        let rel = super::rel(&dest, &a.game);
        // Numbers to pick by (they all say the same text).
        let r1 = |v: f32| (v * 10.0).round() / 10.0;
        let secs = y.len() as f32 / 44100.0;
        let pitch = median_pitch(&x, 44100).map(f32::round);
        let loud = r1(audio::db(audio::loud_rms(&x)));
        let phone = r1(audio::phone_loss_db(&decoded, 44100, true));
        let pitch_s = pitch.map(|p| format!("{p:.0} Hz")).unwrap_or_else(|| "?".into());
        let warn = if phone < audio::PHONE_WARN_DB { "  (too deep for a phone speaker)" } else { "" };
        eprintln!("  wrote {}: {secs:.1} s · median pitch {pitch_s} · loudness {loud:+.1} dBFS as delivered · phone {phone:+.1} dB{warn}", dest.display());
        entries.push(PreviewEntry { id: p.id.clone(), file: rel, seconds: Some(r1(secs)), pitch_hz: pitch, loudness_db: Some(loud), phone_loss_db: Some(phone) });
    }
    let mut cast = load_voices(&a.game)?;
    let old = cast.remove(&a.character);
    let entry = VoiceEntry {
        description: a.description.clone(),
        provider: prov_name.clone(),
        voice_id: old.as_ref().and_then(|o| o.voice_id.clone()),
        picked: old.as_ref().and_then(|o| o.picked),
        design_model: model,
        seed,
        sample_text: text,
        previews: entries,
        model: old.as_ref().and_then(|o| o.model.clone()),
        delivery: old.as_ref().and_then(|o| o.delivery.clone()),
        speed: old.as_ref().and_then(|o| o.speed),
        style: old.as_ref().and_then(|o| o.style),
        premade: old.as_ref().and_then(|o| o.premade.clone()),
        allowed_names: allowed,
        date: super::now_utc(),
        // The kept voice (if any) is still the one speaking until a preview is picked.
        reference: old.as_ref().map(|o| o.reference.clone()).unwrap_or_default(),
        released: old.as_ref().and_then(|o| o.released.clone()),
    };
    cast.insert(a.character.clone(), entry);
    save_voices(&a.game, &cast)?;
    if let Some(c) = est {
        eprintln!("  cost: est. ${c:.2}");
    }
    match a.pick {
        Some(n) => pick_preview(&a.game, &a.character, n, false),
        None => {
            eprintln!(
                "  listen to the previews (they all say the same text), then keep one: mb voice pick {} {} N\n  \
                 previews expire after a day or so on the provider's side: pick soon, or design again",
                a.game.display(),
                a.character
            );
            Ok(())
        }
    }
}

pub fn ext_for(mime: &str) -> &'static str {
    if mime.contains("pcm") {
        "pcm"
    } else if mime.contains("wav") {
        "wav"
    } else if mime.contains("ogg") {
        "ogg"
    } else {
        "mp3"
    }
}

pub fn mime_for_ext(p: &Path) -> String {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "pcm" => "audio/pcm;rate=24000".into(),
        "wav" => "audio/wav".into(),
        "ogg" => "audio/ogg".into(),
        _ => "audio/mpeg".into(),
    }
}

/// The name a game's voice has on the account ("duel rocko").
pub fn account_name(game: &Path, character: &str) -> String {
    let dir = if game.file_name().is_none() { std::fs::canonicalize(game).unwrap_or_else(|_| game.to_path_buf()) } else { game.to_path_buf() };
    format!("{} {character}", dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
}

fn pick_preview(game: &Path, character: &str, n: usize, check_slots: bool) -> Result<(), String> {
    let mut cast = load_voices(game)?;
    let e = cast.get_mut(character).ok_or(format!("no voice for {character:?} in art/voices.toml: mb voice design {} {character} \"…\"", game.display()))?;
    let p = e.previews.get(n.wrapping_sub(1)).ok_or(format!("{character} has previews 1–{}", e.previews.len()))?.clone();
    let prov = pick(&e.provider)?;
    if check_slots {
        super::voice_account::check_free_slot(prov.as_ref(), game, character, false)?;
    }
    let name = account_name(game, character);
    let id = prov.create(&p.id, &name, &e.description).map_err(|err| super::voice_account::with_slot_help(err, game, character))?;
    let before = e.voice_id.replace(id.clone()).filter(|old| *old != id && e.provider != "mock");
    e.picked = Some(n);
    e.released = None;
    e.premade = None;
    // Keep the preview's own audio in the game, so the voice can be recreated once its slot is freed.
    e.reference = match super::voice_account::reference_from_preview(game, character, n, true) {
        Ok(r) => vec![r],
        Err(err) => {
            eprintln!("  note: no reference audio kept for {character} ({err}); `mb voice release` keeps some from its lines");
            Vec::new()
        }
    };
    save_voices(game, &cast)?;
    eprintln!("{character}'s voice is preview {n} ({id}); give it lines: mb voice say {} {character} <line-id> \"…\" --emotion …", game.display());
    if let Some(old) = before {
        eprintln!("  its previous voice {old} is still on your account and takes a slot: `mb voice gc` lists voices no game uses");
    }
    Ok(())
}

// --- lines -------------------------------------------------------------------------------------

/// The recipe of a line, stored in its sidecar.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LineJob {
    pub kind: String,
    pub character: String,
    pub line: String,
    /// As written (tags included).
    pub text: String,
    #[serde(default)]
    pub emotion: Option<String>,
    pub delivery: String,
    #[serde(default)]
    pub style: Option<f32>,
    #[serde(default)]
    pub speed: Option<f32>,
    pub provider: String,
    pub model: String,
    pub voice_id: String,
    pub seed: u64,
    pub rate: u32,
    pub quality: f32,
    pub out: String,
    #[serde(default)]
    pub allow_names: Vec<String>,
}

impl LineJob {
    /// What's sent: the emotion as an audio tag in front, where the model acts tags.
    pub fn sent_text(&self) -> String {
        if !tags_ok(&self.model) {
            return subtitle(&self.text);
        }
        match &self.emotion {
            Some(e) if !e.trim().is_empty() => format!("[{}] {}", e.trim().trim_matches(['[', ']']), self.text.trim()),
            _ => self.text.trim().to_string(),
        }
    }

    fn stability(&self) -> f32 {
        match self.delivery.as_str() {
            "creative" => 0.0,
            "robust" => 1.0,
            _ => 0.5,
        }
    }

    /// Changes when anything that changes the audio does (`mb voice script` skips lines whose key matches).
    pub fn key(&self) -> String {
        let v = json!([self.voice_id, self.model, self.sent_text(), self.delivery, self.style, self.speed, self.rate, self.quality]);
        super::sha256_hex(v.to_string().as_bytes())[..16].to_string()
    }
}

/// Whether a speech model takes the speed setting. Eleven v3 and v4 ignore it
/// (the same seed comes back the same length at any speed), so their lines
/// are time-stretched after, pitch kept.
pub fn speed_native(model: &str) -> bool {
    !(model.starts_with("eleven_v3") || model.starts_with("eleven_v4"))
}

/// A setting as the API should see it: two decimals (an f32 0.6 is 0.6000000238418579 as f64).
pub fn api_f(x: f32) -> f64 {
    (x as f64 * 100.0).round() / 100.0
}

/// How much longer to make a line in post-processing, for a speed its model ignores.
fn stretch_for(job: &LineJob) -> Option<f64> {
    job.speed.map(|s| s.clamp(0.7, 1.2)).filter(|s| (s - 1.0).abs() > 0.005 && !speed_native(&job.model)).map(|s| 1.0 / api_f(s))
}

fn tags_ok(model: &str) -> bool {
    !(model.contains("flash") || model.contains("multilingual") || model.contains("turbo_v2"))
}

/// The subtitle: the text without [audio tags], spaces tidied.
pub fn subtitle(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn line_job(game: &Path, character: &str, line: &str, text: &str, emotion: Option<String>, o: &LineOpts) -> Result<LineJob, String> {
    check_name("character", character)?;
    check_name("line id", line)?;
    if !(22050..=48000).contains(&o.rate) {
        return Err("--rate must be 22050–48000 (SPEC §5.6)".into());
    }
    if subtitle(text).is_empty() {
        return Err(format!("{character} {line}: the line has no words"));
    }
    let cast = load_voices(game)?;
    let e = cast.get(character).ok_or(format!("no voice for {character:?} in art/voices.toml: mb voice design {} {character} \"<description>\"", game.display()))?;
    let voice_id = match &e.voice_id {
        Some(v) => v.clone(),
        None if o.dry_run => "(not picked yet)".into(),
        None => return Err(format!("{character} has previews but no voice yet: listen, then mb voice pick {} {character} N", game.display())),
    };
    let prov = o.provider.clone().unwrap_or_else(|| e.provider.clone());
    let model = o
        .model
        .clone()
        .or_else(|| e.model.clone().filter(|_| prov == e.provider))
        .or_else(|| provider::default_model(&prov, Media::Voice).map(|m| m.id.to_string()))
        .ok_or(format!("{prov} has no speech model"))?;
    let delivery = o.delivery.clone().or_else(|| e.delivery.clone()).unwrap_or_else(|| "natural".into());
    if !matches!(delivery.as_str(), "creative" | "natural" | "robust") {
        return Err(format!("--delivery {delivery:?}: creative, natural or robust"));
    }
    let mut allow = o.allow_name.clone();
    allow.extend(e.allowed_names.iter().cloned());
    Ok(LineJob {
        kind: "voice".into(),
        character: character.into(),
        line: line.into(),
        text: text.trim().into(),
        emotion,
        delivery,
        style: o.style.or(e.style),
        speed: o.speed.or(e.speed),
        provider: prov,
        model,
        voice_id,
        seed: o.seed.unwrap_or_else(super::fresh_seed),
        rate: o.rate,
        quality: o.quality,
        out: format!("assets/vo/{character}/{line}.ogg"),
        allow_names: allow,
    })
}

fn line_cost(job: &LineJob) -> Option<f64> {
    provider::find_model(&job.provider, &job.model).and_then(|m| provider::estimate_chars(m, job.sent_text().chars().count()))
}

fn say(a: SayArgs) -> Result<(), String> {
    if !a.game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", a.game.display()));
    }
    let job = line_job(&a.game, &a.character, &a.line, &a.text, a.opts.emotion.clone(), &a.opts)?;
    let n = a.variants.max(1);
    let est = line_cost(&job);
    eprintln!("line {} {} → {}", job.character, job.line, job.out);
    eprintln!("  {} {} · \"{}\"", job.provider, job.model, job.sent_text());
    check_voice_text(&[&job.text, job.emotion.as_deref().unwrap_or("")], &job.allow_names)?;
    super::check_cost(est, n, super::max_cost(a.opts.max_cost), if n > 1 { "these takes" } else { "this line" })?;
    if a.opts.dry_run {
        eprintln!("  (dry run: nothing generated)");
        return Ok(());
    }
    if job.emotion.is_some() && !tags_ok(&job.model) {
        eprintln!("  note: {} doesn't act audio tags; --emotion is dropped (use eleven_v4 or eleven_v3)", job.model);
    }
    let mut sp = Speakers::new(&a.game, pick(&job.provider)?, a.opts.keep);
    if n == 1 {
        speak_line(&a.game, &job, Some(&mut sp), None, None)?;
        return sp.finish();
    }
    for i in 1..=n {
        let j = LineJob { seed: job.seed.wrapping_add(i as u64 - 1), out: format!("art/variants/vo/{}/{}_{i}.ogg", job.character, job.line), ..job.clone() };
        speak_line(&a.game, &j, Some(&mut sp), None, None)?;
    }
    sp.finish()?;
    eprintln!("  listen, then keep one: mb voice keep {} {} {} N", a.game.display(), job.character, job.line);
    Ok(())
}

fn keep(game: &Path, character: &str, line: &str, n: usize) -> Result<(), String> {
    let from = game.join(format!("art/variants/vo/{character}/{line}_{n}.ogg"));
    if !from.exists() {
        return Err(format!("no {} (mb voice say … --variants N writes them)", from.display()));
    }
    let to = game.join(format!("assets/vo/{character}/{line}.ogg"));
    std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
    std::fs::copy(lips_path(&from), lips_path(&to)).map_err(|e| e.to_string())?;
    let mut sc = super::read_sidecar(&from)?;
    let mut job: LineJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| e.to_string())?;
    job.out = super::rel(&to, game);
    sc.recipe = serde_json::to_value(&job).unwrap();
    super::write_sidecar(&to, &sc)?;
    eprintln!("kept take {n} as {}", to.display());
    refresh_lengths(game)?;
    println!("{}", to.display());
    Ok(())
}

pub fn lips_path(ogg: &Path) -> PathBuf {
    ogg.with_extension("lips.txt")
}

/// Writes a file whole or not at all (a temporary file beside it, renamed over it).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_file_name(format!(".{}.{}.part", path.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Held while a line is being made: a second run of the same line (another
/// terminal, a parallel script) is refused instead of overwriting it.
pub struct LineLock(#[allow(dead_code)] std::fs::File);

pub fn lock_line(game: &Path, out: &str) -> Result<LineLock, String> {
    let dir = std::env::temp_dir().join("mb-voice-lines");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let canon = std::fs::canonicalize(game).unwrap_or_else(|_| game.to_path_buf());
    let key = &super::sha256_hex(format!("{}\n{out}", canon.display()).as_bytes())[..16];
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(format!("{key}.lock"))).map_err(|e| e.to_string())?;
    match f.try_lock() {
        Ok(()) => Ok(LineLock(f)),
        Err(std::fs::TryLockError::WouldBlock) => Err(format!(
            "{out} is being generated by another mb run right now: wait for it to finish (two runs of one line would overwrite each other; --variants N makes several takes)"
        )),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("locking {out}: {e}")),
    }
}

/// Speaks (or with `raw`, reprocesses) one line and writes the Ogg, its `.lips.txt` and its sidecar.
pub fn speak_line(
    game: &Path,
    job: &LineJob,
    speakers: Option<&mut Speakers>,
    raw: Option<(Vec<u8>, String, String, Option<Alignment>)>,
    cost_hint: Option<f64>,
) -> Result<PathBuf, String> {
    let allowed = check_voice_text(&[&job.text, job.emotion.as_deref().unwrap_or("")], &job.allow_names)?;
    // One run at a time per line: two `mb voice say` of the same line would overwrite each other.
    let _line = lock_line(game, &job.out)?;
    let sent = job.sent_text();
    let est = line_cost(job);
    let mut recreated = false;
    let (bytes, mime, raw_rel, alignment, cost) = match raw {
        Some((b, m, r, al)) => (b, m, r, al, cost_hint),
        None => {
            let speakers = speakers.ok_or("speak_line: no provider to speak with")?;
            let (sp, rec) = speakers.speak(
                job,
                SpeechRequest {
                    model: job.model.clone(),
                    voice_id: job.voice_id.clone(),
                    text: sent.clone(),
                    stability: job.stability(),
                    similarity: 0.75,
                    style: job.style,
                    speed: job.speed,
                    seed: Some(job.seed),
                },
            )?;
            recreated = rec;
            if !rec && job.out.starts_with("assets/vo/") {
                super::voice_account::reference_from_line(game, job, &sp.bytes, &sp.mime);
            }
            let art = super::ensure_art_dir(game)?;
            let dir = art.join("raw/vo").join(&job.character);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let stem = Path::new(&job.out).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let raw_path = dir.join(format!("{stem}.{}", ext_for(&sp.mime)));
            write_atomic(&raw_path, &sp.bytes)?;
            if let Some(al) = &sp.alignment {
                write_atomic(&raw_path.with_extension("timing.json"), serde_json::to_string(al).unwrap().as_bytes())?;
            }
            (sp.bytes, sp.mime, super::rel(&raw_path, game), sp.alignment, sp.cost_usd)
        }
    };
    let (chans, src_rate) = audio::decode(&bytes, &mime).map_err(|e| format!("{}: {e}", job.provider))?;
    let mut mono = audio::resample(&audio::to_mono(&chans), src_rate, job.rate);
    let mut alignment = alignment;
    let stretched = stretch_for(job);
    if let Some(f) = stretched {
        eprintln!("  {} ignores the speed setting: time-stretched to {f:.2}× the length, pitch kept", job.model);
        mono = audio::stretch(&mono, job.rate, f);
        alignment = alignment.map(|a| Alignment {
            chars: a.chars,
            start: a.start.iter().map(|t| t * f as f32).collect(),
            end: a.end.iter().map(|t| t * f as f32).collect(),
        });
    }
    let (y, offset) = process(&mono, job.rate);
    let sub = subtitle(&job.text);
    let al = alignment.as_ref().map(|a| shift(a, offset as f32 / job.rate as f32));
    let lips = lip_flap(&y, job.rate, al.as_ref(), &sub);
    let dest = game.join(&job.out);
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    // Written next to the line and renamed into place at the end: a game (or a watcher) never sees half a line.
    let part = dest.with_file_name(format!(".{}.{}.part.ogg", dest.file_stem().unwrap_or_default().to_string_lossy(), std::process::id()));
    let decoded = audio::encode_ogg(&y, job.rate, job.quality, &part)?;
    let lips_text = lips.to_text(&sub);
    std::fs::write(lips_path(&part), &lips_text).map_err(|e| e.to_string())?;
    let chk = audio::check(&decoded, job.rate, false);
    let phone = audio::phone_loss_db(&decoded, job.rate, true);
    let mut warnings = Vec::new();
    if phone < audio::PHONE_WARN_DB {
        warnings.push(format!("a phone speaker loses {:.0} dB of it: a very deep voice; design it a little higher, or lean on the subtitle", -phone));
    }
    let secs = y.len() as f32 / job.rate as f32;
    if secs > 6.0 {
        warnings.push(format!("{secs:.1} s is long for a game line: barks are 1–2 s, cutscene lines a sentence or two"));
    }
    let ogg = std::fs::read(&part).map_err(|e| e.to_string())?;
    let mut result = json!({
        "seconds": chk.secs,
        "peak_db": chk.peak_db,
        "loud_rms_db": chk.rms_db,
        "phone_loss_db": (phone * 10.0).round() / 10.0,
        "subtitle": sub,
        "lips": super::rel(&lips_path(&dest), game),
        "lips_hz": LIPS_HZ,
        "timed_by": if al.is_some() { "the provider's character timings" } else { "loudness" },
        "key": job.key(),
        "warnings": warnings,
    });
    if let Some(f) = stretched {
        result["speed"] = json!(format!("time-stretched to {f:.3}× the length after ({} ignores the speed setting), pitch kept", job.model));
    }
    if recreated {
        result["voice"] = json!(format!(
            "recreated from art/voice_ref/{}/ (Instant Voice Cloning of the designed voice's own audio: very close, not identical)",
            job.character
        ));
    }
    let raw_bytes = std::fs::read(game.join(&raw_rel)).unwrap_or(bytes);
    let sc = Sidecar {
        generator: format!("mb {} voice", env!("CARGO_PKG_VERSION")),
        date: super::now_utc(),
        provider: job.provider.clone(),
        model: job.model.clone(),
        via: None,
        prompt: job.text.clone(),
        full_prompt: sent,
        seed: job.seed,
        seed_honored: true,
        references: Vec::new(),
        style: Some(FileRef { path: "art/voices.toml".into(), sha256: super::sha256_hex(&std::fs::read(voices_path(game)).unwrap_or_default()) }),
        est_cost_usd: est,
        cost_usd: cost,
        raw: raw_rel,
        raw_sha256: super::sha256_hex(&raw_bytes),
        output_sha256: super::sha256_hex(&ogg),
        allowed_names: allowed,
        model_text: None,
        recipe: serde_json::to_value(job).unwrap(),
        result,
        note: if job.provider == "mock" {
            super::note_for("mock")
        } else {
            "Generated with ElevenLabs from a voice designed for this game (not a clone); check your plan's license terms.".into()
        },
        derived: Vec::new(),
    };
    super::write_sidecar(&part, &sc)?;
    for (from, to) in [(super::sidecar_path(&part), super::sidecar_path(&dest)), (lips_path(&part), lips_path(&dest)), (part.clone(), dest.clone())] {
        std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    eprintln!("  wrote {} ({} KB, {secs:.2} s): {chk} · phone {phone:+.1} dB · \"{sub}\"", dest.display(), ogg.len().div_ceil(1024));
    for w in &warnings {
        eprintln!("  warning: {w}");
    }
    println!("{}", dest.display());
    if job.out.starts_with("assets/vo/") {
        refresh_lengths(game)?;
    }
    Ok(dest)
}

// --- the length table ---------------------------------------------------------------------------

/// Where `mb voice` keeps each line's length for the game's code.
pub const LENGTHS_FILE: &str = "src/vo_lengths.rs";

/// Every recorded line's length: (`"<character>/<line>"`, seconds from its
/// `.lips.txt`), sorted.
pub fn line_lengths(game: &Path) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    for who in std::fs::read_dir(game.join("assets/vo")).into_iter().flatten().flatten().filter(|e| e.path().is_dir()) {
        let name = who.file_name().to_string_lossy().to_string();
        for f in std::fs::read_dir(who.path()).into_iter().flatten().flatten().map(|e| e.path()) {
            let file = f.file_name().unwrap_or_default().to_string_lossy().to_string();
            let Some(id) = file.strip_suffix(".lips.txt") else { continue };
            // A line's timing goes with its recording (a stray .lips.txt isn't a line).
            if id.is_empty() || !f.with_file_name(format!("{id}.ogg")).is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            let dur = text.lines().find_map(|l| l.strip_prefix("dur ")).and_then(|d| d.trim().parse::<f32>().ok()).filter(|d| d.is_finite() && *d >= 0.0);
            if let Some(d) = dur {
                out.push((format!("{name}/{id}"), d));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A name as a Rust identifier: `-` → `_`, a leading digit or a keyword escaped.
fn ident(s: &str, upper: bool) -> String {
    const KEYWORDS: [&str; 51] = [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
    ];
    let mut id: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { if upper { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() } } else { '_' }).collect();
    if id.starts_with(|c: char| c.is_ascii_digit()) || id == "_" {
        id.insert(0, '_');
    } else if KEYWORDS.contains(&id.as_str()) {
        id.push('_');
    }
    id
}

/// The Rust source of the length table (`src/vo_lengths.rs`): `ALL`, every
/// line by `"<character>/<line>"`, and a constant per line in a module per
/// character (`narrator::F1_END`). Compile-time constants, so narration
/// timed from them replays the same however the audio loads.
pub fn lengths_source(lines: &[(String, f32)]) -> String {
    let mut s = String::from(
        "//! Each recorded voice line's length in seconds, from assets/vo/*/*.lips.txt.\n\
         //! Written by `mb voice` (say, script, keep, lengths): don't edit it; `mb build`\n\
         //! warns when it's out of date with the recordings. Compile-time constants, so\n\
         //! narration timed from them (`talk.narration.lengths = vo_lengths::ALL`) and\n\
         //! quiet stretches cleared for it replay the same whenever the audio loads.\n\
         #![allow(dead_code)]\n\n\
         /// Every line, by \"<character>/<line>\".\n\
         pub const ALL: &[(&str, f32)] = &[\n",
    );
    for (key, d) in lines {
        s += &format!("    ({key:?}, {d:.3}),\n");
    }
    s += "];\n";
    let mut who = "";
    let mut names: Vec<String> = Vec::new();
    for (key, d) in lines {
        let (w, id) = key.split_once('/').unwrap_or(("", key));
        if w != who {
            if !who.is_empty() {
                s += "}\n";
            }
            s += &format!("\npub mod {} {{\n", ident(w, false));
            who = w;
            names.clear();
        }
        let c = ident(id, true);
        // Two ids that differ only by - and _ get one constant (ALL has both).
        if !names.contains(&c) {
            s += &format!("    pub const {c}: f32 = {d:.3};\n");
            names.push(c);
        }
    }
    if !who.is_empty() {
        s += "}\n";
    }
    s
}

/// The values of a length table's `ALL` (for checking it's current).
pub fn parse_lengths(src: &str) -> Vec<(String, f32)> {
    let Some(start) = src.find("pub const ALL") else { return Vec::new() };
    let body = &src[start..];
    let body = &body[..body.find("];").unwrap_or(body.len())];
    body.lines()
        .filter_map(|l| {
            let l = l.trim().strip_prefix("(\"")?;
            let (key, rest) = l.split_once('"')?;
            let d = rest.trim_start_matches(',').trim().trim_end_matches(',').trim_end_matches(')').trim().parse().ok()?;
            Some((key.to_string(), d))
        })
        .collect()
}

/// Writes `src/vo_lengths.rs` when the game has a `src/` and its recorded
/// lines' lengths changed (a game with no recordings gets none).
pub fn refresh_lengths(game: &Path) -> Result<(), String> {
    let lines = line_lengths(game);
    let path = game.join(LENGTHS_FILE);
    if !game.join("src").is_dir() || (lines.is_empty() && !path.exists()) {
        return Ok(());
    }
    let src = lengths_source(&lines);
    let old = std::fs::read_to_string(&path).ok();
    if old.as_deref() == Some(src.as_str()) {
        return Ok(());
    }
    write_atomic(&path, src.as_bytes())?;
    eprintln!("  wrote {LENGTHS_FILE} ({} line lengths)", lines.len());
    if old.is_none() {
        eprintln!(
            "  to time narration from it (story lines in quiet stretches, the same in every replay): `mod vo_lengths;` in src/lib.rs and `talk.narration.lengths = vo_lengths::ALL;` (docs/UI.md \"Narration\")"
        );
    }
    Ok(())
}

// --- post-processing -----------------------------------------------------------------------------

/// Dialogue sits above music (−20) and a little under effects (−14).
pub const VOICE_TARGET_DB: f32 = -16.0;

/// A peaking EQ (RBJ): `gain_db` around `f`.
fn peaking(rate: u32, f: f32, q: f32, gain_db: f32) -> Biquad {
    let a = 10f64.powf(gain_db as f64 / 40.0);
    let w = std::f64::consts::TAU * f as f64 / rate as f64;
    let (c, al) = (w.cos(), w.sin() / (2.0 * q as f64));
    Biquad::new([1.0 + al * a, -2.0 * c, 1.0 - al * a], [1.0 + al / a, -2.0 * c, 1.0 - al / a])
}

/// For a phone: rumble and plosive thumps out (high-pass at 90 Hz), +3 dB of
/// presence around 3 kHz (where speech is understood, and where a phone
/// speaker is loud), trimmed to the words with a breath of room, loudness to
/// VOICE_TARGET_DB with the look-ahead limiter. Returns the samples and how
/// many were trimmed from the front (to shift the timings).
pub fn process(x: &[f32], rate: u32) -> (Vec<f32>, usize) {
    let y = audio::filter(
        x,
        &[Biquad::highpass(rate, 90.0, BUTTER4_Q[0]), Biquad::highpass(rate, 90.0, BUTTER4_Q[1]), peaking(rate, 3000.0, 0.9, 3.0)],
    );
    let p = audio::peak(&y);
    if p <= 0.0 || y.is_empty() {
        return (y, 0);
    }
    let thr = p * audio::undb(-40.0);
    let first = y.iter().position(|v| v.abs() > thr).unwrap_or(0);
    let last = y.iter().rposition(|v| v.abs() > thr).unwrap_or(y.len() - 1);
    let start = first.saturating_sub((0.02 * rate as f32) as usize);
    let end = (last + (0.12 * rate as f32) as usize).min(y.len());
    let mut t = y[start..end].to_vec();
    let n = t.len();
    let fi = ((0.005 * rate as f32) as usize).min(n / 2);
    let fo = ((0.03 * rate as f32) as usize).min(n / 2);
    for (i, v) in t.iter_mut().take(fi).enumerate() {
        *v *= i as f32 / fi as f32;
    }
    for i in 0..fo {
        t[n - 1 - i] *= i as f32 / fo as f32;
    }
    let g = audio::undb(VOICE_TARGET_DB) / audio::loud_rms(&t).max(1e-9);
    (audio::limit(&t, g, rate), start)
}

fn shift(a: &Alignment, by: f32) -> Alignment {
    Alignment {
        chars: a.chars.clone(),
        start: a.start.iter().map(|t| (t - by).max(0.0)).collect(),
        end: a.end.iter().map(|t| (t - by).max(0.0)).collect(),
    }
}

// --- lip flap -------------------------------------------------------------------------------------

pub const LIPS_HZ: u32 = 30;

/// A line's mouth and subtitle timing (written as `.lips.txt`).
#[derive(Clone, Debug, PartialEq)]
pub struct Lips {
    /// 0 closed, 1 a little open, 2 open, 3 wide; one per 1/LIPS_HZ s.
    pub mouth: Vec<u8>,
    /// When each char of the subtitle is spoken, ms.
    pub reveal: Vec<u32>,
    pub dur: f32,
}

impl Lips {
    pub fn to_text(&self, subtitle: &str) -> String {
        let mouth: String = self.mouth.iter().map(|m| char::from(b'0' + m.min(&3))).collect();
        let reveal: Vec<String> = self.reveal.iter().map(u32::to_string).collect();
        format!("mblips 1\nhz {LIPS_HZ}\ndur {:.3}\nmouth {mouth}\ntext {subtitle}\nreveal {}\n", self.dur, reveal.join(" "))
    }
}

fn frame_rms(x: &[f32], hop: usize) -> Vec<f32> {
    x.chunks(hop).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len().max(1) as f32).sqrt()).collect()
}

/// The mouth from loudness in the speech band (the jaw follows it), shut on
/// m/b/p, barely open on f/v and hisses (s, sh: lots of energy over 4 kHz,
/// little under), wide only on loud vowels; then one-frame flickers removed.
pub fn lip_flap(x: &[f32], rate: u32, al: Option<&Alignment>, sub: &str) -> Lips {
    let hop = (rate / LIPS_HZ) as usize;
    let band = audio::filter(x, &[Biquad::highpass(rate, 250.0, 0.707), Biquad::lowpass(rate, 3000.0, 0.707)]);
    let hi = audio::filter(x, &[Biquad::highpass(rate, 4500.0, 0.707)]);
    let b = frame_rms(&band, hop);
    let h = frame_rms(&hi, hop);
    let mut sorted: Vec<f32> = b.iter().copied().filter(|v| *v > 1e-5).collect();
    sorted.sort_by(f32::total_cmp);
    let reference = sorted.get(sorted.len() * 9 / 10).copied().unwrap_or(1.0).max(1e-6);
    let char_at = |t: f32| -> Option<char> {
        let a = al?;
        let i = a.start.iter().rposition(|s| *s <= t)?;
        (t < a.end.get(i).copied().unwrap_or(0.0) + 0.02).then(|| a.chars[i].chars().next()).flatten()
    };
    let mut m: Vec<u8> = b
        .iter()
        .zip(&h)
        .enumerate()
        .map(|(i, (&bv, &hv))| {
            let level = bv / reference;
            let mut o: u8 = if level < 0.12 {
                0
            } else if level < 0.35 {
                1
            } else if level < 0.7 {
                2
            } else {
                3
            };
            if hv > bv * 1.2 && o > 1 {
                o = 1;
            }
            match char_at((i as f32 + 0.5) / LIPS_HZ as f32).map(|c| c.to_ascii_lowercase()) {
                Some('m' | 'b' | 'p') => o = 0,
                Some('f' | 'v') => o = o.min(1),
                Some('o' | 'u' | 'w') => o = o.min(2),
                _ => {}
            }
            o
        })
        .collect();
    // No single-frame flicker: a frame that differs from both neighbours, which agree, takes their value.
    for i in 1..m.len().saturating_sub(1) {
        if m[i - 1] == m[i + 1] && m[i] != m[i - 1] && m[i].abs_diff(m[i - 1]) == 1 {
            m[i] = m[i - 1];
        }
    }
    let dur = x.len() as f32 / rate as f32;
    let n = sub.chars().count();
    let reveal = match al {
        Some(a) => {
            // Each subtitle char takes the time of the same char in the timings, in order; [tags] are skipped.
            let mut spoken: Vec<(char, f32)> = Vec::new();
            let mut depth = 0;
            for (c, t) in a.chars.iter().zip(&a.start) {
                let ch = c.chars().next().unwrap_or(' ');
                match ch {
                    '[' => depth += 1,
                    ']' if depth > 0 => depth -= 1,
                    _ if depth == 0 => spoken.push((ch.to_ascii_lowercase(), *t)),
                    _ => {}
                }
            }
            let mut j = 0;
            let mut last = 0.0f32;
            sub.chars()
                .map(|c| {
                    let c = c.to_ascii_lowercase();
                    if let Some(k) = (j..(j + 12).min(spoken.len())).find(|&k| spoken[k].0 == c) {
                        last = last.max(spoken[k].1);
                        j = k + 1;
                    }
                    (last * 1000.0).round() as u32
                })
                .collect()
        }
        None => {
            // Spread over the voiced part.
            let first = m.iter().position(|v| *v > 0).unwrap_or(0) as f32 / LIPS_HZ as f32;
            let last = m.iter().rposition(|v| *v > 0).map_or(dur, |i| (i + 1) as f32 / LIPS_HZ as f32);
            (0..n).map(|i| ((first + (last - first) * i as f32 / n.max(1) as f32) * 1000.0).round() as u32).collect()
        }
    };
    Lips { mouth: m, reveal, dur }
}

// --- scripts --------------------------------------------------------------------------------------

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ScriptLine {
    pub character: String,
    #[serde(alias = "line")]
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub emotion: Option<String>,
}

/// Lines from a .toml (`[[line]]` tables) or a .csv (character,line,text[,emotion], a header row optional; quotes for commas).
pub fn parse_script(path: &Path, text: &str) -> Result<Vec<ScriptLine>, String> {
    if path.extension().is_some_and(|e| e == "toml") {
        #[derive(Deserialize)]
        struct F {
            #[serde(default)]
            line: Vec<ScriptLine>,
        }
        let f: F = toml::from_str(text).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(f.line);
    }
    let mut out = Vec::new();
    for (n, row) in text.lines().enumerate() {
        if row.trim().is_empty() || row.trim_start().starts_with('#') {
            continue;
        }
        let cells = csv_row(row);
        if n == 0 && cells.first().is_some_and(|c| c.eq_ignore_ascii_case("character")) {
            continue;
        }
        if cells.len() < 3 {
            return Err(format!("{}:{}: want character,line,text[,emotion]", path.display(), n + 1));
        }
        out.push(ScriptLine {
            character: cells[0].trim().into(),
            id: cells[1].trim().into(),
            text: cells[2].trim().into(),
            emotion: cells.get(3).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        });
    }
    Ok(out)
}

fn csv_row(row: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cells.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

fn script(a: ScriptArgs) -> Result<(), String> {
    let text = std::fs::read_to_string(&a.script).map_err(|e| format!("{}: {e}", a.script.display()))?;
    let lines = parse_script(&a.script, &text)?;
    let mut todo = Vec::new();
    let mut fresh = 0;
    for l in &lines {
        // A script's line keeps the seed it was made with, so an unchanged line stays unchanged.
        let mut o = a.opts.clone();
        let existing = super::read_sidecar(&a.game.join(format!("assets/vo/{}/{}.ogg", l.character, l.id))).ok();
        if o.seed.is_none() {
            o.seed = existing.as_ref().map(|s| s.seed).or(Some(super::mock::hash(&[l.character.as_bytes(), l.id.as_bytes()]) % 2_000_000_000));
        }
        let job = line_job(&a.game, &l.character, &l.id, &l.text, l.emotion.clone(), &o)?;
        check_voice_text(&[&job.text, job.emotion.as_deref().unwrap_or("")], &job.allow_names)?;
        let same = existing.as_ref().is_some_and(|s| s.result["key"].as_str() == Some(job.key().as_str()));
        if same && !a.force {
            fresh += 1;
        } else {
            todo.push(job);
        }
    }
    let chars: usize = todo.iter().map(|j| j.sent_text().chars().count()).sum();
    let est: Option<f64> = todo.iter().map(line_cost).sum();
    eprintln!("script {}: {} lines, {fresh} up to date, {} to speak ({chars} characters)", a.script.display(), lines.len(), todo.len());
    for j in &todo {
        eprintln!("  {}/{}: \"{}\"", j.character, j.line, j.sent_text());
    }
    if todo.is_empty() {
        return refresh_lengths(&a.game);
    }
    super::check_cost(est, 1, super::max_cost(a.opts.max_cost), "the script")?;
    if a.opts.dry_run {
        eprintln!("  (dry run: nothing generated)");
        return Ok(());
    }
    // One batch: a released character's voice is recreated once for all its lines, and deleted at the end.
    let mut provs: BTreeMap<String, Speakers> = BTreeMap::new();
    for j in &todo {
        if !provs.contains_key(&j.provider) {
            provs.insert(j.provider.clone(), Speakers::new(&a.game, pick(&j.provider)?, a.opts.keep));
        }
        speak_line(&a.game, j, provs.get_mut(&j.provider), None, None)?;
    }
    for sp in provs.values_mut() {
        sp.finish()?;
    }
    voice_warnings(&a.game);
    Ok(())
}

/// What `mb build` will warn about the game's voices (lines with no voice file, …).
fn voice_warnings(game: &Path) {
    let mut src = String::new();
    let mut stack = vec![game.join("src")];
    while let Some(d) = stack.pop() {
        for p in std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()) {
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                src += &std::fs::read_to_string(&p).unwrap_or_default();
            }
        }
    }
    for w in crate::voicecheck::warnings(game, &src) {
        eprintln!("warning: {w}");
    }
}

fn list(game: &Path) -> Result<(), String> {
    let cast = load_voices(game)?;
    if cast.is_empty() {
        eprintln!("no voices yet: mb voice design {} <character> \"<description>\"", game.display());
    }
    for (who, e) in &cast {
        let state = match (&e.voice_id, e.picked) {
            _ if e.premade.is_some() => format!("premade {}", e.premade.as_deref().unwrap_or("")),
            _ if e.released.is_some() => "released: recreated to speak".into(),
            (Some(_), Some(n)) => format!("preview {n}"),
            (Some(_), None) => "picked".into(),
            _ => format!("{} previews, none kept", e.previews.len()),
        };
        eprintln!("{who} ({}, {state}): {}", e.provider, e.description);
        let dir = game.join("assets/vo").join(who);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map(|d| d.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|x| x == "ogg")).collect()).unwrap_or_default();
        files.sort();
        for f in files {
            let sc = super::read_sidecar(&f).ok();
            let secs = sc.as_ref().and_then(|s| s.result["seconds"].as_f64()).unwrap_or(0.0);
            let sub = sc.as_ref().and_then(|s| s.result["subtitle"].as_str().map(str::to_string)).unwrap_or_default();
            eprintln!("  {:<14} {secs:4.1} s  {sub}", f.file_stem().unwrap().to_string_lossy());
        }
    }
    voice_warnings(game);
    Ok(())
}

// --- regen ----------------------------------------------------------------------------------------

pub fn regen(a: &super::RegenArgs, sc: &Sidecar) -> Result<(), String> {
    let mut job: LineJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("the sidecar's recipe: {e}"))?;
    let game = super::game_of(&a.asset, &job.out);
    if let Some(p) = &a.prompt {
        job.text = p.clone();
    }
    if let Some(s) = a.seed {
        job.seed = s;
    } else if a.new_seed {
        job.seed = super::fresh_seed();
    }
    if let Some(m) = &a.model {
        job.model = m.clone();
    }
    if a.reprocess {
        let raw = game.join(&sc.raw);
        let bytes = std::fs::read(&raw).map_err(|e| format!("{}: {e} (the raw take is gone: regenerate without --reprocess)", raw.display()))?;
        let al = std::fs::read_to_string(raw.with_extension("timing.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
        eprintln!("line {} {}: reprocessing {} (no provider call)", job.character, job.line, sc.raw);
        speak_line(&game, &job, None, Some((bytes, mime_for_ext(&raw), sc.raw.clone(), al)), sc.cost_usd)?;
        return Ok(());
    }
    let est = line_cost(&job);
    eprintln!("line {} {} → {}", job.character, job.line, job.out);
    super::check_cost(est, 1, super::max_cost(a.max_cost), "this line")?;
    if a.dry_run {
        eprintln!("  \"{}\"\n  (dry run: nothing generated)", job.sent_text());
        return Ok(());
    }
    let mut sp = Speakers::new(&game, pick(&job.provider)?, false);
    speak_line(&game, &job, Some(&mut sp), None, None)?;
    sp.finish()
}

#[cfg(test)]
mod tests {
    use super::super::provider::tests::{Fake, json as reply};
    use super::*;
    use std::rc::Rc;

    #[test]
    fn real_voices_are_refused() {
        assert!(check_voice_text(&["a smug rock-star pilot, raspy, 30s"], &[]).is_ok());
        assert!(check_voice_text(&["sounds like a grumpy pirate"], &[]).is_ok());
        let e = check_voice_text(&["sounds like Morgan Freeman, warm"], &[]).unwrap_err();
        assert!(e.contains("Morgan") && e.contains("original"), "{e}");
        assert!(check_voice_text(&["in the voice of Gandalf"], &[]).is_err());
        assert!(check_voice_text(&["clone my uncle's voice"], &[]).is_err());
        assert!(check_voice_text(&["like Rocko but older"], &["rocko".into()]).is_ok());
        // mb art's names list applies.
        assert!(check_voice_text(&["Mario's voice"], &[]).is_err());
    }

    #[test]
    fn subtitles_drop_audio_tags_and_emotion_becomes_one() {
        assert_eq!(subtitle("[laughs] Too   slow! [sighs]"), "Too slow!");
        let job = LineJob {
            kind: "voice".into(),
            character: "rocko".into(),
            line: "t".into(),
            text: "Is that all?".into(),
            emotion: Some("smug".into()),
            delivery: "natural".into(),
            style: None,
            speed: None,
            provider: "mock".into(),
            model: "eleven_v4".into(),
            voice_id: "v".into(),
            seed: 1,
            rate: 44100,
            quality: 4.0,
            out: "assets/vo/rocko/t.ogg".into(),
            allow_names: vec![],
        };
        assert_eq!(job.sent_text(), "[smug] Is that all?");
        let flash = LineJob { model: "eleven_flash_v2_5".into(), ..job.clone() };
        assert_eq!(flash.sent_text(), "Is that all?");
        assert_ne!(job.key(), LineJob { text: "Is that all!".into(), ..job.clone() }.key());
        assert_eq!(job.key(), LineJob { seed: 9, ..job.clone() }.key(), "the seed alone doesn't make a line stale");
    }

    #[test]
    fn lips_follow_the_words() {
        let (x, al) = babble("Ma ma, papa!", "v");
        let x = audio::resample(&x, MOCK_RATE, 44100);
        let (y, off) = process(&x, 44100);
        let al = shift(&al, off as f32 / 44100.0);
        let lips = lip_flap(&y, 44100, Some(&al), "Ma ma, papa!");
        assert!(lips.mouth.iter().any(|m| *m >= 2), "open on the vowels: {:?}", lips.mouth);
        assert!(lips.mouth.contains(&0), "shut on m and p");
        assert_eq!(lips.reveal.len(), "Ma ma, papa!".chars().count());
        assert!(lips.reveal.windows(2).all(|w| w[0] <= w[1]));
        assert!(*lips.reveal.last().unwrap() as f32 / 1000.0 < lips.dur);
        let t = lips.to_text("Ma ma, papa!");
        assert!(t.starts_with("mblips 1\nhz 30\n") && t.contains("\ntext Ma ma, papa!\n"), "{t}");
        // Without timings: spread over the voiced part.
        let l2 = lip_flap(&y, 44100, None, "Ma ma, papa!");
        assert_eq!(l2.reveal.len(), 12);
        assert!(l2.reveal[0] < l2.reveal[11]);
    }

    #[test]
    fn processing_meets_the_dialogue_level() {
        let (x, _) = babble("Hello there, pilot.", "v");
        let x: Vec<f32> = std::iter::repeat_n(0.0, 12000).chain(x.iter().map(|v| v * 0.1)).collect();
        let (y, off) = process(&audio::resample(&x, MOCK_RATE, 44100), 44100);
        assert!(off > 20000, "leading silence trimmed: {off}");
        assert!((audio::db(audio::loud_rms(&y)) - VOICE_TARGET_DB).abs() < 2.0);
        assert!(audio::db(audio::peak(&y)) <= audio::CEILING_DB);
    }

    #[test]
    fn scripts_from_csv_and_toml() {
        let csv = "character,line,text,emotion\nrocko,t1,\"Ha, too slow!\",smug\ngranny,g1,Back in my day…\n";
        let l = parse_script(Path::new("x.csv"), csv).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "Ha, too slow!");
        assert_eq!(l[0].emotion.as_deref(), Some("smug"));
        assert_eq!(l[1].emotion, None);
        let t = "[[line]]\ncharacter = \"rocko\"\nid = \"t1\"\ntext = \"Ha!\"\n";
        assert_eq!(parse_script(Path::new("x.toml"), t).unwrap()[0].id, "t1");
    }

    #[test]
    fn elevenlabs_design_create_and_speech() {
        use super::super::elevenlabs::ElevenLabs;
        let pcm = provider::b64(&[0u8, 0, 10, 0, 20, 0]);
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "previews": [{ "audio_base_64": pcm, "generated_voice_id": "g1", "media_type": "audio/pcm", "duration_secs": 1.0 }], "text": "Sample." })),
            reply(200, json!({ "voice_id": "V123", "name": "duel rocko" })),
            reply(
                200,
                json!({ "audio_base64": pcm, "alignment": { "characters": ["H", "i"], "character_start_times_seconds": [0.0, 0.1], "character_end_times_seconds": [0.1, 0.2] } }),
            ),
        ]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let (p, text) = e.design(&DesignRequest { model: "eleven_ttv_v3".into(), description: "a grumpy grandma".into(), text: None, seed: Some(7) }).unwrap();
        assert_eq!((p[0].id.as_str(), p[0].mime.as_str(), text.as_str()), ("g1", "audio/pcm;rate=24000", "Sample."));
        assert_eq!(e.create("g1", "duel granny", "a grumpy grandma").unwrap(), "V123");
        let s = e
            .speak(&SpeechRequest {
                model: "eleven_v4".into(),
                voice_id: "V123".into(),
                text: "[angry] Hi".into(),
                stability: 0.5,
                similarity: 0.75,
                style: None,
                speed: Some(1.5),
                seed: Some(3),
            })
            .unwrap();
        assert_eq!(s.alignment.unwrap().start, vec![0.0, 0.1]);
        let sent = fake.sent.borrow();
        assert!(sent[0].0.ends_with("/v1/text-to-voice/design?output_format=pcm_24000"));
        assert_eq!(sent[0].2["auto_generate_text"], true);
        assert_eq!(sent[0].2["model_id"], "eleven_ttv_v3");
        assert!(sent[1].0.ends_with("/v1/text-to-voice"));
        assert_eq!(sent[1].2["generated_voice_id"], "g1");
        assert!(sent[2].0.contains("/v1/text-to-speech/V123/with-timestamps?output_format=pcm_24000"));
        assert!(sent[2].2["voice_settings"].get("speed").is_none(), "v4 ignores speed: mb stretches instead");
        assert_eq!(sent[2].2["text"], "[angry] Hi");
        drop(sent);
        // Flash takes speed: clamped, and every setting as plain two-decimal numbers.
        fake.reply.borrow_mut().push(reply(200, json!({ "audio_base64": pcm, "alignment": null })));
        let req = SpeechRequest {
            model: "eleven_flash_v2_5".into(),
            voice_id: "V123".into(),
            text: "Hi".into(),
            stability: 0.5,
            similarity: 0.75,
            style: Some(0.6),
            speed: Some(1.5),
            seed: None,
        };
        assert!(e.speak(&req).is_ok());
        let sent = fake.sent.borrow();
        let vs = &sent[3].2["voice_settings"];
        assert_eq!(vs["speed"].to_string(), "1.2");
        assert_eq!(vs["style"].to_string(), "0.6", "not 0.6000000238418579");
        assert_eq!(vs["similarity_boost"].to_string(), "0.75");
    }

    #[test]
    fn one_run_at_a_time_per_line() {
        let game = std::env::temp_dir().join(format!("mb-voice-lock-{}", std::process::id()));
        std::fs::create_dir_all(&game).unwrap();
        let held = lock_line(&game, "assets/vo/rocko/t1.ogg").unwrap();
        let e = lock_line(&game, "assets/vo/rocko/t1.ogg").err().unwrap();
        assert!(e.contains("being generated by another mb run"), "{e}");
        assert!(lock_line(&game, "assets/vo/rocko/t2.ogg").is_ok(), "other lines go ahead");
        drop(held);
        // (Another test spawning a process can hold a copy of the lock file for a moment.)
        let t0 = std::time::Instant::now();
        while lock_line(&game, "assets/vo/rocko/t1.ogg").is_err() && t0.elapsed().as_secs() < 3 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(lock_line(&game, "assets/vo/rocko/t1.ogg").is_ok(), "free again once the first run is done");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn median_pitch_finds_the_voice() {
        let rate = 44100;
        for f0 in [110.0f32, 180.0, 260.0] {
            // A buzzy voice: harmonics, with pauses and a little noise.
            let x: Vec<f32> = (0..rate * 2)
                .map(|i| {
                    let t = i as f32 / rate as f32;
                    let on = if (t * 2.0).fract() < 0.7 { 1.0 } else { 0.0 };
                    on * (1..8).map(|k| (std::f32::consts::TAU * f0 * k as f32 * t).sin() / k as f32).sum::<f32>() * 0.3
                })
                .collect();
            let p = median_pitch(&x, rate).unwrap();
            assert!((p / f0 - 1.0).abs() < 0.04, "{f0} Hz read as {p}");
        }
        assert_eq!(median_pitch(&vec![0.0; 44100], rate), None);
        // The mock's babble has its voice's pitch.
        let (x, _) = babble("Well well well, look who showed up.", "v");
        let f0 = 95.0 + (super::super::mock::hash(&[b"v"]) % 160) as f32;
        let p = median_pitch(&x, MOCK_RATE).unwrap();
        assert!((p / f0 - 1.0).abs() < 0.08, "{f0} vs {p}");
    }

    #[test]
    fn a_speed_the_model_ignores_is_made_by_time_stretching() {
        assert!(!speed_native("eleven_v4") && !speed_native("eleven_v3") && speed_native("eleven_flash_v2_5") && speed_native("eleven_multilingual_v2"));
        let game = std::env::temp_dir().join(format!("mb-voice-speed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(&game).unwrap();
        let d = DesignArgs {
            game: game.clone(),
            character: "rocko".into(),
            description: "a smug pilot".into(),
            sample: None,
            pick: Some(1),
            provider: Some("mock".into()),
            model: None,
            seed: Some(1),
            allow_name: vec![],
            ignore_slots: false,
            max_cost: None,
            dry_run: false,
        };
        design(d).unwrap();
        // The mock speaks any model's lines: as eleven_v4, a speed is a stretch after.
        let line = |id: &str, speed: Option<f32>| {
            let opts = LineOpts { rate: 44100, quality: 4.0, model: Some("eleven_v4".into()), speed, seed: Some(1), ..LineOpts::default() };
            say(SayArgs { game: game.clone(), character: "rocko".into(), line: id.into(), text: "Is that all you got, pilot?".into(), variants: 1, opts }).unwrap();
            super::super::read_sidecar(&game.join(format!("assets/vo/rocko/{id}.ogg"))).unwrap()
        };
        let plain = line("a", None);
        let fast = line("b", Some(1.2));
        let (s1, s2) = (plain.result["seconds"].as_f64().unwrap(), fast.result["seconds"].as_f64().unwrap());
        assert!((s2 / s1 - 1.0 / 1.2).abs() < 0.06, "{s1} → {s2}");
        assert!(fast.result["speed"].as_str().unwrap().contains("eleven_v4 ignores the speed setting"));
        assert!(plain.result.get("speed").is_none());
        let lips = std::fs::read_to_string(lips_path(&game.join("assets/vo/rocko/b.ogg"))).unwrap();
        let last: f64 = lips.lines().find(|l| l.starts_with("reveal ")).unwrap().split(' ').next_back().unwrap().parse().unwrap();
        assert!(last / 1000.0 < s2, "the subtitle timings were stretched with the audio");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn the_whole_pipeline_offline() {
        let game = std::env::temp_dir().join(format!("mb-voice-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(&game).unwrap();
        design(DesignArgs {
            game: game.clone(),
            character: "granny".into(),
            description: "a grumpy grandma, 80, creaky, sharp".into(),
            sample: None,
            pick: Some(2),
            provider: Some("mock".into()),
            model: None,
            seed: Some(1),
            allow_name: vec![],
            ignore_slots: false,
            max_cost: None,
            dry_run: false,
        })
        .unwrap();
        let cast = load_voices(&game).unwrap();
        assert_eq!(cast["granny"].picked, Some(2));
        assert_eq!(cast["granny"].previews.len(), 3);
        let p = &cast["granny"].previews[0];
        assert!(p.seconds.unwrap() > 1.0 && p.pitch_hz.is_some() && p.loudness_db.is_some() && p.phone_loss_db.is_some(), "{p:?}");
        let opts = LineOpts { rate: 44100, quality: 4.0, ..LineOpts::default() };
        say(SayArgs { game: game.clone(), character: "granny".into(), line: "g1".into(), text: "Back in my day!".into(), variants: 1, opts: opts.clone() }).unwrap();
        let ogg = game.join("assets/vo/granny/g1.ogg");
        let lips = std::fs::read_to_string(lips_path(&ogg)).unwrap();
        assert!(lips.contains("text Back in my day!"));
        let left: Vec<_> = std::fs::read_dir(game.join("assets/vo/granny")).unwrap().flatten().map(|f| f.file_name().to_string_lossy().to_string()).collect();
        assert!(left.iter().all(|n| !n.contains(".part")), "written in place by renames: {left:?}");
        let sc = super::super::read_sidecar(&ogg).unwrap();
        assert_eq!(sc.recipe["kind"], "voice");
        // The script skips the line that's already there, and speaks the new one.
        let script = game.join("lines.csv");
        std::fs::write(&script, "granny,g1,Back in my day!\ngranny,g2,Hmph.\n").unwrap();
        let before = std::fs::metadata(&ogg).unwrap().modified().unwrap();
        // g1 was made with a fresh seed; the script keeps it (the seed doesn't make a line stale).
        script_run(&game, &script, &opts);
        assert!(game.join("assets/vo/granny/g2.ogg").exists());
        assert_eq!(std::fs::metadata(&ogg).unwrap().modified().unwrap(), before, "unchanged line not spoken again");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn the_length_table_round_trips() {
        let game = std::env::temp_dir().join(format!("mb-voice-lengths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        for d in ["src", "assets/vo/narrator", "assets/vo/old-man"] {
            std::fs::create_dir_all(game.join(d)).unwrap();
        }
        let line = |who: &str, id: &str, dur: &str| {
            std::fs::write(game.join(format!("assets/vo/{who}/{id}.ogg")), b"OggS").unwrap();
            std::fs::write(game.join(format!("assets/vo/{who}/{id}.lips.txt")), format!("mblips 1\nhz 30\ndur {dur}\ntext Hi.\n")).unwrap();
        };
        line("narrator", "f1_end", "3.204");
        line("narrator", "1st", "0.5");
        line("old-man", "type", "12.75");
        // A timing file without its recording isn't a line.
        std::fs::write(game.join("assets/vo/narrator/stray.lips.txt"), "mblips 1\ndur 9\n").unwrap();
        refresh_lengths(&game).unwrap();
        let src = std::fs::read_to_string(game.join(LENGTHS_FILE)).unwrap();
        let want = vec![("narrator/1st".to_string(), 0.5), ("narrator/f1_end".to_string(), 3.204), ("old-man/type".to_string(), 12.75)];
        assert_eq!(line_lengths(&game), want);
        assert_eq!(parse_lengths(&src), want, "{src}");
        for decl in ["pub mod narrator {", "pub const F1_END: f32 = 3.204;", "pub const _1ST: f32 = 0.500;", "pub mod old_man {", "pub const TYPE: f32 = 12.750;"] {
            assert!(src.contains(decl), "{decl} in {src}");
        }
        // Unchanged recordings: not rewritten. A re-recorded line: rewritten.
        let before = std::fs::metadata(game.join(LENGTHS_FILE)).unwrap().modified().unwrap();
        refresh_lengths(&game).unwrap();
        assert_eq!(std::fs::metadata(game.join(LENGTHS_FILE)).unwrap().modified().unwrap(), before);
        line("narrator", "f1_end", "4.1");
        refresh_lengths(&game).unwrap();
        assert_eq!(parse_lengths(&std::fs::read_to_string(game.join(LENGTHS_FILE)).unwrap())[1].1, 4.1);
        assert_eq!((ident("self", false).as_str(), ident("9lives", false).as_str(), ident("a-b", true).as_str()), ("self_", "_9lives", "A_B"));
        let _ = std::fs::remove_dir_all(&game);
    }

    fn script_run(game: &Path, script: &Path, opts: &LineOpts) {
        super::script(ScriptArgs { game: game.to_path_buf(), script: script.to_path_buf(), force: false, opts: opts.clone() }).unwrap();
    }
}
