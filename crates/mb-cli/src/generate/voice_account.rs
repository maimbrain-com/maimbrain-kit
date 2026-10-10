//! Voice slots: an ElevenLabs account holds a fixed number of custom voices
//! (10 on Starter, 30 on Creator…), and every voice a game keeps with
//! `mb voice pick` takes one for as long as it exists. This is the account
//! side of `mb voice`: what's using the slots and which game uses which voice.
//!
//!     mb voice slots                      # the account's slots, and which games under here use which voice
//!     mb voice slots games/duel games/gallery
//!     mb voice release games/duel rocko --delete --yes   # keep rocko as audio in the game, free its slot
//!     mb voice say games/duel rocko t1 "…"            # recreates the voice for the batch, then deletes it
//!     mb voice gc                                      # voices no game here uses (a dry run; --yes deletes)
//!
//! Key permissions: `user_read` (the slot count), `voices_read` (the list),
//! `voices_write` (deleting, and recreating with Instant Voice Cloning).

use std::path::{Path, PathBuf};

use clap::Args;

use super::elevenlabs::is_permission_error;
use super::voice::{self, AccountVoice, VoiceProvider};

/// What a temporary voice's name starts with, so one left behind by an interrupted batch can be found.
pub const TEMP_PREFIX: &str = "mb temp ";

#[derive(Args)]
pub struct SlotsArgs {
    /// Where to look for games' art/voices.toml (default: the repository you're in, or the current directory).
    pub dirs: Vec<PathBuf>,
    /// elevenlabs (default) or mock.
    #[arg(long)]
    pub provider: Option<String>,
}

// --- which game uses which voice ---------------------------------------------------------------

/// How a game uses a voice on the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// The character speaks with it.
    Speaking,
    /// Released (`mb voice release`): the game keeps it as audio; deleting it frees a slot.
    Released,
    /// A released character's recreation kept with `--keep`, which later batches reuse.
    Kept,
}

/// A character in a game's art/voices.toml and the account voice it uses.
#[derive(Clone, Debug)]
pub struct CastRef {
    pub game: PathBuf,
    pub character: String,
    pub voice_id: String,
    pub used: Use,
}

/// The directory to scan when none is given: the repository you're in (the
/// nearest directory up with a `.git`), or the current directory.
pub fn workspace() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf).unwrap_or(cwd)
}

/// Every game (a directory with art/voices.toml) under `dirs`, skipping
/// hidden directories, build output and dependencies.
pub fn find_games(dirs: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(d: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if d.join("art/voices.toml").is_file() {
            out.push(d.to_path_buf());
        }
        if depth == 0 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                !n.starts_with('.') && !matches!(n.as_str(), "target" | "node_modules" | "build" | "dist" | "raw" | "variants")
            })
            .map(|e| e.path())
            .collect();
        subs.sort();
        for s in subs {
            walk(&s, depth - 1, out);
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        walk(d, 6, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

/// The voices `games` use (only those on a provider's account: not the mock's).
pub fn cast_refs(games: &[PathBuf]) -> (Vec<CastRef>, Vec<String>) {
    let mut refs = Vec::new();
    let mut errors = Vec::new();
    for g in games {
        match voice::load_voices(g) {
            Ok(cast) => {
                for (who, e) in cast {
                    // Premade voices aren't the account's: they take no slot.
                    if e.provider == "mock" || e.premade.is_some() {
                        continue;
                    }
                    let r =|id: &str, used| CastRef { game: g.clone(), character: who.clone(), voice_id: id.to_string(), used };
                    match (&e.voice_id, &e.released) {
                        (Some(id), None) => refs.push(r(id, Use::Speaking)),
                        (Some(id), Some(rel)) => {
                            if !rel.deleted {
                                refs.push(r(id, Use::Released));
                            }
                            if let Some(k) = &rel.kept_voice_id {
                                refs.push(r(k, Use::Kept));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }
    (refs, errors)
}

/// What an account voice is to the games scanned.
#[derive(Clone, Debug, PartialEq)]
pub enum Class {
    /// A character speaks with it (or a kept recreation of a released voice).
    InUse,
    /// Released by its game, which keeps it as audio: deleting it frees a slot.
    Released,
    /// Named by mb for a game that isn't under the directories scanned ("boltbrawl kai"):
    /// probably a game in another checkout or on another branch, so not taken for unused.
    OtherGame(String),
    /// A temporary recreation an interrupted batch left behind.
    Temp,
    /// No game scanned uses it.
    Unused,
}

/// The game in a voice name mb gave ("<game> <who>", "mb temp <game> <who>"), if it's one.
pub fn mb_game_of(name: &str) -> Option<String> {
    let (g, c) = name.strip_prefix(TEMP_PREFIX).unwrap_or(name).split_once(' ')?;
    let ok = |s: &str| s.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_' || ch == '.');
    (!c.is_empty() && ok(g) && ok(c)).then(|| g.to_string())
}

/// The names of the games scanned (the directory names mb names voices by).
pub fn game_names(games: &[PathBuf]) -> Vec<String> {
    games.iter().map(|g| voice::account_name(g, "x").trim_end_matches(" x").to_string()).collect()
}

pub fn classify(v: &AccountVoice, refs: &[CastRef], names: &[String]) -> Class {
    let mine: Vec<&CastRef> = refs.iter().filter(|r| r.voice_id == v.id).collect();
    if mine.iter().any(|r| r.used == Use::Speaking || r.used == Use::Kept) {
        Class::InUse
    } else if !mine.is_empty() {
        Class::Released
    } else if let Some(g) = mb_game_of(&v.name).filter(|g| !names.contains(g)) {
        Class::OtherGame(g)
    } else if v.name.starts_with(TEMP_PREFIX) {
        Class::Temp
    } else {
        Class::Unused
    }
}

/// A path as typed from here (relative to the current directory when it's under it).
fn show(p: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    p.strip_prefix(&cwd).map(|r| r.display().to_string()).unwrap_or_else(|_| p.display().to_string())
}

fn who_uses(id: &str, refs: &[CastRef]) -> String {
    refs.iter().filter(|r| r.voice_id == id).map(|r| format!("{}: {}", show(&r.game), r.character)).collect::<Vec<_>>().join(", ")
}

fn date_of(v: &AccountVoice) -> String {
    v.created_unix.map(|t| super::utc(t)[..10].to_string()).unwrap_or_default()
}

fn row(v: &AccountVoice) -> String {
    format!("{:<22} {:<28} {:<12} {}", v.id, format!("\"{}\"", v.name), v.category, date_of(v))
}

// --- slots --------------------------------------------------------------------------------------

/// What to do when the account is full.
pub fn no_slot_help(game: Option<&Path>, character: Option<&str>) -> String {
    let g = game.map(|g| g.display().to_string()).unwrap_or_else(|| "<game>".into());
    let c = character.unwrap_or("<character>");
    format!(
        "  see what's using them:                mb voice slots\n  \
         free one a game here uses:            mb voice release {g} <character> --delete --yes   (kept as audio, recreated to speak)\n  \
         delete voices no game here uses:      mb voice gc            (a dry run; add --yes to delete)\n  \
         or cast a premade voice (no slot):    mb voice premades, then mb voice cast {g} {c} --premade <name>"
    )
}

/// A provider's slot-limit error with what to do about it.
pub fn with_slot_help(e: String, game: &Path, character: &str) -> String {
    if e.starts_with("no free voice slot") { format!("{e}\n{}", no_slot_help(Some(game), Some(character))) } else { e }
}

/// Before keeping a voice: refuses when the account has no free slot.
/// A key that can't read slots (no `user_read`) only gets a note.
pub fn check_free_slot(prov: &dyn VoiceProvider, game: &Path, character: &str, designing: bool) -> Result<(), String> {
    match prov.slots() {
        Ok(s) if s.free() == 0 => {
            let then = if designing {
                "\n  previews don't take a slot but keeping one does: free one first, or pass --ignore-slots to design now and pick once one's free"
            } else {
                ""
            };
            Err(format!(
                "no free voice slot: your ElevenLabs account holds {} of {} custom voices, and keeping {character}'s voice needs one{then}\n{}",
                s.used,
                s.limit,
                no_slot_help(Some(game), Some(character))
            ))
        }
        Ok(s) => {
            if s.tier != "mock" {
                eprintln!("  voice slots: {} of {} used", s.used, s.limit);
            }
            Ok(())
        }
        Err(e) if is_permission_error(&e) => {
            eprintln!("  note: can't check your voice slots first: {}", e.replace("\n  ", "\n        "));
            Ok(())
        }
        Err(e) => {
            eprintln!("  note: can't check your voice slots first: {e}");
            Ok(())
        }
    }
}

pub fn slots(a: SlotsArgs) -> Result<(), String> {
    let prov = voice::pick(&voice::choose(a.provider.as_deref(), false)?)?;
    let dirs = if a.dirs.is_empty() { vec![workspace()] } else { a.dirs.clone() };
    eprint!("{}", slots_report(prov.as_ref(), &dirs)?);
    Ok(())
}

/// The account's slots and voices against the games under `dirs`.
pub fn slots_report(prov: &dyn VoiceProvider, dirs: &[PathBuf]) -> Result<String, String> {
    let games = find_games(dirs);
    let (refs, errors) = cast_refs(&games);
    let slots = prov.slots();
    let voices = prov.voices(false);
    let mut out = String::new();
    let mut hints = Vec::new();
    match &slots {
        Ok(s) => {
            out += &format!("voice slots: {} of {} used, {} free ({})\n", s.used, s.limit, s.free(), if s.tier.is_empty() { "?" } else { &s.tier });
            if let Some((n, max)) = s.add_edits {
                out += &format!("  voices added or edited this month: {n} of {max}\n");
            }
            if s.can_clone == Some(false) {
                out += "  this plan can't use Instant Voice Cloning\n";
            }
        }
        Err(e) => hints.push(e.clone()),
    }
    let where_ = dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ");
    out += &format!("games with art/voices.toml under {where_}: {}\n", games.len());
    for e in &errors {
        out += &format!("  warning: {e}\n");
    }
    match &voices {
        Ok(vs) => {
            let names = game_names(&games);
            let group = |c: Class| vs.iter().filter(|v| classify(v, &refs, &names) == c).collect::<Vec<_>>();
            let in_use = group(Class::InUse);
            let temp = group(Class::Temp);
            let unused = group(Class::Unused);
            let other: Vec<&AccountVoice> = vs.iter().filter(|v| matches!(classify(v, &refs, &names), Class::OtherGame(_))).collect();
            out += &format!("\nin use here ({}):\n", in_use.len());
            for v in &in_use {
                out += &format!("  {}  {}\n", row(v), who_uses(&v.id, &refs));
            }
            let released = group(Class::Released);
            if !released.is_empty() {
                out += &format!("\nreleased ({}): their games keep them as audio and recreate them to speak; deleting them frees a slot\n", released.len());
                for v in &released {
                    out += &format!("  {}  {}\n", row(v), who_uses(&v.id, &refs));
                }
            }
            if !temp.is_empty() {
                out += &format!("\nleft behind by an interrupted batch ({}): temporary recreations, safe to delete\n", temp.len());
                for v in &temp {
                    out += &format!("  {}\n", row(v));
                }
            }
            if !other.is_empty() {
                out += &format!(
                    "\nmade by mb for games not under here ({}): another checkout or branch? mb voice gc leaves them (--other-games deletes them too)\n",
                    other.len()
                );
                for v in &other {
                    out += &format!("  {}\n", row(v));
                }
            }
            out += &format!("\nnot used by any game here ({}):\n", unused.len());
            for v in &unused {
                out += &format!("  {}\n", row(v));
            }
            let on_account: Vec<&str> = vs.iter().map(|v| v.id.as_str()).collect();
            let missing: Vec<&CastRef> = refs.iter().filter(|r| r.used == Use::Speaking && !on_account.contains(&r.voice_id.as_str())).collect();
            if !missing.is_empty() {
                out += &format!("\ncast here but not on this account ({}): another account's key, or deleted\n", missing.len());
                for r in missing {
                    out += &format!("  {:<22} {}: {}\n", r.voice_id, show(&r.game), r.character);
                }
            }
        }
        Err(e) => hints.push(e.clone()),
    }
    if slots.is_err() && voices.is_err() {
        return Err(hints.join("\n"));
    }
    for h in hints {
        out += &format!("\n{h}\n");
    }
    if slots.as_ref().is_ok_and(|s| s.free() == 0) {
        out += &format!("\nno free voice slot: keeping a designed voice needs one\n{}\n", no_slot_help(None, None));
    }
    Ok(out)
}

// --- reference audio: a voice kept in the game ------------------------------------------------------

/// How many of a voice's first lines are kept as reference audio, next to its preview.
pub const MAX_REF_LINES: usize = 4;

pub fn ref_dir(game: &Path, who: &str) -> PathBuf {
    game.join("art/voice_ref").join(who)
}

/// Keeps audio of a character's own voice in art/voice_ref/<who>/<stem>.ogg:
/// unprocessed (no EQ or loudness), mono, at its own rate, Ogg q6.
fn save_reference(game: &Path, who: &str, stem: &str, bytes: &[u8], mime: &str, from: &str) -> Result<voice::RefFile, String> {
    let (chans, rate) = super::audio::decode(bytes, mime)?;
    let mut x = super::audio::to_mono(&chans);
    let mut rate = rate;
    if rate > 44100 {
        x = super::audio::resample(&x, rate, 44100);
        rate = 44100;
    }
    let p = super::audio::peak(&x);
    if p > 0.9 {
        x.iter_mut().for_each(|v| *v *= 0.9 / p);
    }
    let dir = ref_dir(game, who);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(format!("{stem}.ogg"));
    super::audio::encode_ogg(&x, rate, 6.0, &dest)?;
    let ogg = std::fs::read(&dest).map_err(|e| e.to_string())?;
    Ok(voice::RefFile { file: super::rel(&dest, game), sha256: super::sha256_hex(&ogg), from: from.into(), seconds: x.len() as f32 / rate as f32 })
}

/// The picked preview's audio, as the provider returned it (art/raw/), else mb's processed preview.
/// With `fresh` (a new pick: a new voice), what an earlier voice left is cleared first.
pub fn reference_from_preview(game: &Path, who: &str, n: usize, fresh: bool) -> Result<voice::RefFile, String> {
    let raw = ["pcm", "wav", "mp3"].iter().map(|e| game.join(format!("art/raw/voices/{who}_preview_{n}.{e}"))).find(|p| p.is_file());
    let src = raw.unwrap_or_else(|| game.join(format!("art/voices/{who}/preview_{n}.ogg")));
    let bytes = std::fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    if fresh && let Ok(rd) = std::fs::read_dir(ref_dir(game, who)) {
        for f in rd.flatten() {
            let _ = std::fs::remove_file(f.path());
        }
    }
    save_reference(game, who, "preview", &bytes, &voice::mime_for_ext(&src), &format!("preview {n}"))
}

/// After a line is spoken with the character's own voice: keeps it as
/// reference audio while there are fewer than MAX_REF_LINES. Best effort.
pub fn reference_from_line(game: &Path, job: &voice::LineJob, bytes: &[u8], mime: &str) {
    let Ok(mut cast) = voice::load_voices(game) else { return };
    let Some(e) = cast.get_mut(&job.character) else { return };
    if e.provider != job.provider || e.voice_id.as_deref() != Some(job.voice_id.as_str()) || e.released.is_some() {
        return;
    }
    let from = format!("line {}", job.line);
    if e.reference.iter().filter(|r| r.from.starts_with("line ")).count() >= MAX_REF_LINES || e.reference.iter().any(|r| r.from == from) {
        return;
    }
    match save_reference(game, &job.character, &format!("line_{}", job.line), bytes, mime, &from) {
        Ok(r) => {
            e.reference.push(r);
            let _ = voice::save_voices(game, &cast);
        }
        Err(err) => eprintln!("  note: couldn't keep this line as reference audio: {err}"),
    }
}

/// Makes sure a character's voice is kept in the game before it's released:
/// the picked preview if it's still here, and up to MAX_REF_LINES of the
/// lines its own voice spoke (their raw takes in art/raw/vo/).
pub fn ensure_reference(game: &Path, who: &str, e: &mut voice::VoiceEntry) -> Result<(), String> {
    e.reference.retain(|r| game.join(&r.file).is_file());
    if !e.reference.iter().any(|r| r.from.starts_with("preview"))
        && let Some(n) = e.picked
        && let Ok(r) = reference_from_preview(game, who, n, false)
    {
        e.reference.insert(0, r);
    }
    let mut lines: Vec<PathBuf> = std::fs::read_dir(game.join("assets/vo").join(who))
        .map(|d| d.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|x| x == "ogg")).collect())
        .unwrap_or_default();
    lines.sort();
    for ogg in lines {
        if e.reference.iter().filter(|r| r.from.starts_with("line ")).count() >= MAX_REF_LINES {
            break;
        }
        let Ok(sc) = super::read_sidecar(&ogg) else { continue };
        let Ok(job) = serde_json::from_value::<voice::LineJob>(sc.recipe.clone()) else { continue };
        let from = format!("line {}", job.line);
        if sc.provider != e.provider || e.voice_id.as_deref() != Some(job.voice_id.as_str()) || sc.result.get("voice").is_some() || e.reference.iter().any(|r| r.from == from) {
            continue;
        }
        let raw = game.join(&sc.raw);
        let Ok(bytes) = std::fs::read(&raw) else { continue };
        if let Ok(r) = save_reference(game, who, &format!("line_{}", job.line), &bytes, &voice::mime_for_ext(&raw), &from) {
            e.reference.push(r);
        }
    }
    if e.reference.is_empty() {
        return Err(format!(
            "no audio of {who}'s voice is here to keep (its previews and raw takes aren't committed, and this checkout has none): \
             releasing it would lose the voice. Speak a line or two with it first (mb voice say {} {who} …), then release it",
            game.display()
        ));
    }
    Ok(())
}

/// The reference audio to recreate a voice from, as WAV. Every file must be
/// one mb wrote (in art/voice_ref/<who>/, with the hash recorded when it was
/// kept): mb recreates a game's own designed voice, never anyone else's.
pub fn reference_audio(game: &Path, who: &str, e: &voice::VoiceEntry) -> Result<Vec<voice::RefAudio>, String> {
    let prefix = format!("art/voice_ref/{who}/");
    let mut out = Vec::new();
    for r in &e.reference {
        if !r.file.starts_with(&prefix) || r.file.contains("..") {
            return Err(format!("{who}'s reference {} isn't in {prefix}: mb only recreates a voice from the audio it kept itself", r.file));
        }
        let p = game.join(&r.file);
        let bytes = std::fs::read(&p).map_err(|err| format!("{}: {err} ({who}'s voice can't be recreated without it)", p.display()))?;
        if super::sha256_hex(&bytes) != r.sha256 {
            return Err(format!(
                "{} changed since mb kept it: mb only recreates a voice from the game's own designed voice, never other audio",
                p.display()
            ));
        }
        let (chans, rate) = super::audio::decode(&bytes, "audio/ogg")?;
        let stem = Path::new(&r.file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        out.push(voice::RefAudio { name: format!("{stem}.wav"), bytes: super::audio::wav_bytes(&super::audio::to_mono(&chans), rate), mime: "audio/wav".into() });
    }
    if out.is_empty() {
        return Err(format!("{who} has no reference audio in art/voices.toml, so its released voice can't be recreated"));
    }
    Ok(out)
}

// --- speaking with released voices -----------------------------------------------------------------

/// Speaks a batch of lines. A released character's voice is recreated once
/// (Instant Voice Cloning from its reference audio) the first time it's
/// needed, used for all its lines, and deleted at the end (`finish`, or when
/// dropped on an error), unless `keep`.
pub struct Speakers {
    pub prov: Box<dyn VoiceProvider>,
    game: PathBuf,
    keep: bool,
    temp: Vec<(String, String)>,
}

impl Speakers {
    pub fn new(game: &Path, prov: Box<dyn VoiceProvider>, keep: bool) -> Speakers {
        Speakers { prov, game: game.to_path_buf(), keep, temp: Vec::new() }
    }

    fn temp_for(&self, who: &str) -> Option<String> {
        self.temp.iter().find(|(w, _)| w == who).map(|(_, id)| id.clone())
    }

    fn update(&self, who: &str, f: impl FnOnce(&mut voice::Released)) {
        if let Ok(mut cast) = voice::load_voices(&self.game)
            && let Some(rel) = cast.get_mut(who).and_then(|e| e.released.as_mut())
        {
            f(rel);
            let _ = voice::save_voices(&self.game, &cast);
        }
    }

    /// Speaks as `job`'s character; true when it was a recreation of a released voice.
    pub fn speak(&mut self, job: &voice::LineJob, mut req: voice::SpeechRequest) -> Result<(voice::Speech, bool), String> {
        let entry = voice::load_voices(&self.game)?.remove(&job.character);
        let released = entry.as_ref().filter(|e| e.provider == job.provider && e.voice_id.as_deref() == Some(job.voice_id.as_str())).and_then(|e| e.released.clone());
        let Some(rel) = released else { return self.prov.speak(&req).map(|s| (s, false)) };
        let gone = |e: &str| e.contains("voice_not_found");
        if let Some(id) = self.temp_for(&job.character).or(rel.kept_voice_id.clone()) {
            req.voice_id = id;
            match self.prov.speak(&req) {
                Ok(s) => return Ok((s, true)),
                Err(e) if gone(&e) => self.update(&job.character, |r| r.kept_voice_id = None),
                Err(e) => return Err(e),
            }
        } else if !rel.deleted {
            // Released but still on the account: the original is the voice itself.
            match self.prov.speak(&req) {
                Ok(s) => return Ok((s, false)),
                Err(e) if gone(&e) => self.update(&job.character, |r| r.deleted = true),
                Err(e) => return Err(e),
            }
        }
        req.voice_id = self.recreate(&job.character, entry.as_ref().unwrap())?;
        self.prov.speak(&req).map(|s| (s, true))
    }

    fn recreate(&mut self, who: &str, e: &voice::VoiceEntry) -> Result<String, String> {
        let files = reference_audio(&self.game, who, e)?;
        match self.prov.slots() {
            Ok(s) if s.can_clone == Some(false) => {
                return Err(format!(
                    "{who}'s voice was released, and recreating it needs Instant Voice Cloning, which this ElevenLabs plan doesn't include"
                ));
            }
            Ok(s) if s.free() == 0 => {
                return Err(format!(
                    "no free voice slot: {who}'s voice was released, and recreating it takes a slot for the length of this batch\n{}",
                    no_slot_help(Some(&self.game), Some(who))
                ));
            }
            _ => {}
        }
        let name = format!("{TEMP_PREFIX}{}", voice::account_name(&self.game, who));
        let secs: f32 = e.reference.iter().map(|r| r.seconds).sum();
        let about = format!("Temporary: {who}'s designed voice, recreated by mb voice from its own audio. {}", e.description);
        let id = self.prov.clone_voice(&name, &about, &files).map_err(|err| with_slot_help(err, &self.game, who))?;
        eprintln!(
            "  {who}'s voice was released: recreated it from {} reference files ({secs:.0} s) as {id}, {} after this batch",
            files.len(),
            if self.keep { "kept" } else { "deleted" }
        );
        self.temp.push((who.to_string(), id.clone()));
        Ok(id)
    }

    /// Deletes this batch's recreations (or with `keep`, records them for the next batch).
    pub fn finish(&mut self) -> Result<(), String> {
        let mut failed = Vec::new();
        for (who, id) in std::mem::take(&mut self.temp) {
            if self.keep {
                self.update(&who, |r| r.kept_voice_id = Some(id.clone()));
                eprintln!("  kept {who}'s recreated voice {id} on your account (it takes a slot; mb voice gc lists it as in use)");
                continue;
            }
            match self.prov.delete_voice(&id) {
                Ok(()) => {}
                Err(e) => failed.push(format!("{who}'s temporary voice {id}: {e}")),
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!("couldn't delete {}; `mb voice gc --yes` removes it later", failed.join("; ")))
        }
    }
}

impl Drop for Speakers {
    fn drop(&mut self) {
        if !self.temp.is_empty()
            && let Err(e) = self.finish()
        {
            eprintln!("  warning: {e}");
        }
    }
}

// --- release ------------------------------------------------------------------------------------------

#[derive(Args)]
pub struct ReleaseArgs {
    pub game: PathBuf,
    /// The character (or --all).
    pub character: Option<String>,
    /// Every character of the game with a designed voice.
    #[arg(long)]
    pub all: bool,
    /// Also delete the voice from your account now, freeing its slot (with --yes; without, says what it would delete).
    #[arg(long)]
    pub delete: bool,
    #[arg(long)]
    pub yes: bool,
}

pub fn release(a: ReleaseArgs) -> Result<(), String> {
    let mut cast = voice::load_voices(&a.game)?;
    let who: Vec<String> = match (&a.character, a.all) {
        (Some(_), true) => return Err("give a character or --all, not both".into()),
        (None, false) => return Err(format!("which character? mb voice release {} <character> (or --all)", a.game.display())),
        (Some(c), false) => {
            if !cast.contains_key(c) {
                return Err(format!("no {c:?} in {}/art/voices.toml", a.game.display()));
            }
            vec![c.clone()]
        }
        (None, true) => cast.iter().filter(|(_, e)| e.voice_id.is_some() && !e.released.as_ref().is_some_and(|r| r.deleted)).map(|(k, _)| k.clone()).collect(),
    };
    let mut targets = Vec::new();
    for w in &who {
        let e = &cast[w];
        let Some(id) = e.voice_id.clone() else {
            eprintln!("{w}: no voice kept yet (nothing to release)");
            continue;
        };
        if e.released.as_ref().is_some_and(|r| r.deleted) {
            eprintln!("{w}: already released and deleted from the account");
            continue;
        }
        if let Some(p) = &e.premade {
            eprintln!("{w}: speaks with the premade voice {p}, which takes no slot (nothing to release)");
            continue;
        }
        targets.push((w.clone(), id, e.provider.clone()));
    }
    if targets.is_empty() {
        return Ok(());
    }
    if a.delete && !a.yes {
        eprintln!("would release these voices, keeping each as audio in the game, and delete them from your account:");
        for (w, id, _) in &targets {
            eprintln!("  {id}  \"{}\"  ({w})", voice::account_name(&a.game, w));
        }
        eprintln!("(nothing changed: add --yes to do it)");
        return Ok(());
    }
    for (w, _, _) in &targets {
        let e = cast.get_mut(w).unwrap();
        ensure_reference(&a.game, w, e)?;
        let secs: f32 = e.reference.iter().map(|r| r.seconds).sum();
        let prev = e.released.take().unwrap_or_default();
        e.released = Some(voice::Released { date: super::now_utc(), ..prev });
        eprintln!("{w}: released; kept {} reference files ({secs:.0} s) in art/voice_ref/{w}/ (commit them)", e.reference.len());
    }
    voice::save_voices(&a.game, &cast)?;
    if !a.delete {
        eprintln!(
            "the voices stay on your account (and keep speaking) until deleted: mb voice release {} … --delete --yes, or mb voice gc --yes",
            a.game.display()
        );
        return Ok(());
    }
    let mut failed = Vec::new();
    for (w, id, prov_name) in &targets {
        let prov = voice::pick(prov_name)?;
        match prov.delete_voice(id) {
            Ok(()) => eprintln!("{w}: deleted {id} from your account"),
            Err(e) if e.contains("voice_not_found") => eprintln!("{w}: {id} was already gone from your account"),
            Err(e) => {
                failed.push(format!("{w}: {e}"));
                continue;
            }
        }
        if let Some(r) = cast.get_mut(w).and_then(|e| e.released.as_mut()) {
            r.deleted = true;
        }
    }
    voice::save_voices(&a.game, &cast)?;
    eprintln!("lines in these voices are now spoken with a recreation of the voice, made for each batch and deleted after it");
    if failed.is_empty() { Ok(()) } else { Err(failed.join("\n")) }
}

// --- gc: voices no game here uses ----------------------------------------------------------------------

#[derive(Args)]
pub struct GcArgs {
    /// Where to look for games' art/voices.toml (default: the repository you're in, or the current directory).
    pub dirs: Vec<PathBuf>,
    /// Delete them (without it, a dry run that lists what would be deleted).
    #[arg(long)]
    pub yes: bool,
    /// Never delete this voice (an id or exact name), even if no game here uses it. Repeatable.
    #[arg(long)]
    pub except: Vec<String>,
    /// Also the voices mb named for games that aren't under these directories (another checkout or branch).
    #[arg(long)]
    pub other_games: bool,
    /// elevenlabs (default) or mock.
    #[arg(long)]
    pub provider: Option<String>,
}

pub fn gc(a: GcArgs) -> Result<(), String> {
    let prov = voice::pick(&voice::choose(a.provider.as_deref(), false)?)?;
    let dirs = if a.dirs.is_empty() { vec![workspace()] } else { a.dirs.clone() };
    gc_with(prov.as_ref(), &dirs, a.yes, &a.except, a.other_games)
}

/// Lists the account's voices no game under `dirs` speaks with (released
/// ones, temporary ones left behind, and ones no game uses) and with `yes`
/// deletes exactly those.
pub fn gc_with(prov: &dyn VoiceProvider, dirs: &[PathBuf], yes: bool, except: &[String], other_games: bool) -> Result<(), String> {
    let games = find_games(dirs);
    let (refs, errors) = cast_refs(&games);
    if !errors.is_empty() {
        // An unreadable voices.toml might be what uses a voice: don't guess.
        return Err(format!("can't read every cast, so can't tell what's unused:\n  {}", errors.join("\n  ")));
    }
    let where_ = dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ");
    if games.is_empty() {
        return Err(format!(
            "no art/voices.toml under {where_}: run mb voice gc from the repository with your games (or pass their directories), \
             or it would take every voice on your account for unused"
        ));
    }
    let voices = prov.voices(false)?;
    let mut doomed: Vec<(&AccountVoice, Class)> = Vec::new();
    let mut spared = Vec::new();
    let names = game_names(&games);
    for v in &voices {
        let c = classify(v, &refs, &names);
        if c == Class::InUse {
            continue;
        }
        if let Class::OtherGame(g) = &c
            && !other_games
        {
            spared.push((v, format!("made for {}, a game not under here: another checkout or branch? (--other-games)", if g.is_empty() { "a game run as ." } else { g })));
        } else if except.iter().any(|x| *x == v.id || *x == v.name) {
            spared.push((v, "--except".into()));
        } else {
            doomed.push((v, c));
        }
    }
    eprintln!("{} voices on the account; {} games with art/voices.toml under {where_}", voices.len(), games.len());
    for (v, why) in &spared {
        eprintln!("  keeping {} \"{}\": {why}", v.id, v.name);
    }
    if doomed.is_empty() {
        eprintln!("nothing to delete");
        return Ok(());
    }
    eprintln!("{} {} voices no game here speaks with:", if yes { "deleting" } else { "would delete" }, doomed.len());
    for (v, c) in &doomed {
        let why = match c {
            Class::Released => format!("released by {} (kept there as audio)", who_uses(&v.id, &refs)),
            Class::Temp => "a temporary recreation left by an interrupted batch".into(),
            Class::OtherGame(g) => format!("made for {g}, which isn't under here (--other-games)"),
            _ => "no game here uses it".into(),
        };
        eprintln!("  {}  {why}", row(v));
    }
    if !yes {
        eprintln!("(a dry run: nothing deleted. Check the list, then add --yes; --except <id> keeps one)");
        return Ok(());
    }
    let mut failed = Vec::new();
    for (v, c) in &doomed {
        match prov.delete_voice(&v.id) {
            Ok(()) => eprintln!("  deleted {} \"{}\"", v.id, v.name),
            Err(e) if e.contains("voice_not_found") => eprintln!("  {} was already gone", v.id),
            Err(e) => {
                // A missing permission fails every one the same way: say it once.
                if is_permission_error(&e) {
                    return Err(e);
                }
                failed.push(format!("{}: {e}", v.id));
                continue;
            }
        }
        if *c == Class::Released {
            for r in refs.iter().filter(|r| r.voice_id == v.id) {
                if let Ok(mut cast) = voice::load_voices(&r.game)
                    && let Some(rel) = cast.get_mut(&r.character).and_then(|e| e.released.as_mut())
                {
                    rel.deleted = true;
                    let _ = voice::save_voices(&r.game, &cast);
                }
            }
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(format!("couldn't delete:\n  {}", failed.join("\n  "))) }
}

// --- premade voices: no slot -------------------------------------------------------------------------

#[derive(Args)]
pub struct PremadesArgs {
    /// Only these (matched against the labels): female, male, young, old, british, american…
    pub filter: Vec<String>,
    /// elevenlabs (default) or mock.
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Args)]
pub struct CastArgs {
    pub game: PathBuf,
    /// Who (a file name: rocko, granny).
    pub character: String,
    /// A premade voice's name or id (`mb voice premades`).
    #[arg(long)]
    pub premade: String,
    /// creative, natural or robust: the character's default delivery.
    #[arg(long)]
    pub delivery: Option<String>,
    /// Style exaggeration 0–1, the character's default.
    #[arg(long)]
    pub style: Option<f32>,
    /// Speaking speed 0.7–1.2, the character's default.
    #[arg(long)]
    pub speed: Option<f32>,
    /// Speech model, the character's default.
    #[arg(long)]
    pub model: Option<String>,
    /// elevenlabs (default) or mock.
    #[arg(long)]
    pub provider: Option<String>,
}

fn labels(v: &AccountVoice) -> String {
    ["gender", "age", "accent", "descriptive", "use_case"]
        .iter()
        .filter_map(|k| v.labels.get(*k).filter(|s| !s.is_empty()).map(|s| s.replace('_', " ")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A premade voice by id, by name (any case), or by a name only one starts with.
pub fn find_premade<'a>(all: &'a [AccountVoice], want: &str) -> Result<&'a AccountVoice, String> {
    let w = want.trim().to_lowercase();
    if let Some(v) = all.iter().find(|v| v.id == want.trim() || v.name.to_lowercase() == w) {
        return Ok(v);
    }
    // "Rachel - calm" style names: match the first word too.
    let starts: Vec<&AccountVoice> = all.iter().filter(|v| v.name.to_lowercase().starts_with(&w)).collect();
    match starts.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no premade voice {want:?}: mb voice premades lists them")),
        many => Err(format!("{want:?} could be {}: give the full name or the id", many.iter().map(|v| format!("{} ({})", v.name, v.id)).collect::<Vec<_>>().join(", "))),
    }
}

pub fn premades(a: PremadesArgs) -> Result<(), String> {
    let prov = voice::pick(&voice::choose(a.provider.as_deref(), false)?)?;
    eprint!("{}", premades_report(prov.as_ref(), &a.filter)?);
    Ok(())
}

pub fn premades_report(prov: &dyn VoiceProvider, filter: &[String]) -> Result<String, String> {
    let mut all = prov.voices(true)?;
    all.sort_by(|x, y| x.name.cmp(&y.name));
    let f: Vec<String> = filter.iter().map(|s| s.to_lowercase().replace(' ', "_")).collect();
    let shown: Vec<&AccountVoice> =
        all.iter().filter(|v| f.iter().all(|w| v.labels.values().any(|l| l.to_lowercase() == *w || l.to_lowercase().replace(' ', "_") == *w))).collect();
    let mut out = format!("premade voices ({} of {}; they take no voice slot):\n", shown.len(), all.len());
    for v in shown {
        let what = labels(v);
        let what = if what.is_empty() { v.description.chars().take(80).collect() } else { what };
        out += &format!("  {:<22} {:<40} {what}\n", v.id, v.name);
    }
    out += "cast one: mb voice cast <game> <character> --premade <name> [--delivery …] [--style …] [--speed …]\n";
    Ok(out)
}

pub fn cast(a: CastArgs) -> Result<(), String> {
    voice::check_name("character", &a.character)?;
    if !a.game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", a.game.display()));
    }
    let prov_name = voice::choose(a.provider.as_deref(), false)?;
    let prov = voice::pick(&prov_name)?;
    cast_with(prov.as_ref(), &prov_name, a)
}

pub fn cast_with(prov: &dyn VoiceProvider, prov_name: &str, a: CastArgs) -> Result<(), String> {
    if let Some(d) = &a.delivery
        && !matches!(d.as_str(), "creative" | "natural" | "robust")
    {
        return Err(format!("--delivery {d:?}: creative, natural or robust"));
    }
    if a.style.is_some_and(|s| !(0.0..=1.0).contains(&s)) {
        return Err("--style is 0–1".into());
    }
    let all = prov.voices(true)?;
    let v = find_premade(&all, &a.premade)?;
    let mut cast = voice::load_voices(&a.game)?;
    let old = cast.remove(&a.character);
    let was = old.as_ref().filter(|o| o.premade.is_none() && o.provider == prov_name).and_then(|o| o.voice_id.clone());
    let mut e = old.unwrap_or_else(|| voice::VoiceEntry {
        description: format!("premade voice {}: {}", v.name, labels(v)),
        provider: prov_name.to_string(),
        design_model: "premade".into(),
        ..Default::default()
    });
    e.provider = prov_name.to_string();
    e.voice_id = Some(v.id.clone());
    e.premade = Some(v.name.clone());
    e.picked = None;
    e.released = None;
    e.delivery = a.delivery.or(e.delivery);
    e.style = a.style.or(e.style);
    e.speed = a.speed.map(|s| s.clamp(0.7, 1.2)).or(e.speed);
    e.model = a.model.or(e.model);
    e.date = super::now_utc();
    cast.insert(a.character.clone(), e);
    voice::save_voices(&a.game, &cast)?;
    eprintln!("{} speaks with the premade voice {} ({}; {}), which takes no voice slot", a.character, v.name, v.id, labels(v));
    if let Some(old) = was {
        eprintln!("  its designed voice {old} is no longer used here and still takes a slot: mb voice slots lists it as unused");
    }
    eprintln!("  give it lines: mb voice say {} {} <line-id> \"…\" --emotion …", a.game.display(), a.character);
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::super::elevenlabs::ElevenLabs;
    use super::super::provider::tests::{Fake, json as reply};
    use super::*;
    use serde_json::json;
    use std::rc::Rc;

    pub fn temp_game(name: &str, toml: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mb-slots-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("games/duel");
        std::fs::create_dir_all(game.join("art")).unwrap();
        std::fs::write(game.join("art/voices.toml"), toml).unwrap();
        root
    }

    pub const CAST: &str = r#"
[rocko]
description = "a smug rock-star pilot"
provider = "elevenlabs"
voice_id = "V_ROCKO"
picked = 2
design_model = "eleven_ttv_v3"
seed = 1
date = "2026-10-10T00:00:00Z"

[granny]
description = "a grumpy grandma"
provider = "elevenlabs"
voice_id = "V_GRANNY"
picked = 1
design_model = "eleven_ttv_v3"
seed = 2
date = "2026-10-10T00:00:00Z"

[bot]
description = "a beepy robot"
provider = "mock"
voice_id = "mock-1"
design_model = "mock-voice"
seed = 3
date = "2026-10-10T00:00:00Z"
"#;

    pub fn subscription(used: u32, limit: u32) -> serde_json::Value {
        json!({ "tier": "starter", "voice_slots_used": used, "voice_limit": limit, "voice_add_edit_counter": 4, "max_voice_add_edits": 95,
                "can_use_instant_voice_cloning": true, "character_count": 1000, "character_limit": 30000 })
    }

    pub fn voice_list() -> serde_json::Value {
        json!({ "voices": [
            { "voice_id": "V_ROCKO", "name": "duel rocko", "category": "generated", "labels": {}, "created_at_unix": 1760000000 },
            { "voice_id": "V_OLD", "name": "Old Narrator", "category": "generated", "labels": { "gender": "male" }, "created_at_unix": 1700000000 },
            { "voice_id": "V_TEMP", "name": "mb temp duel granny", "category": "cloned", "labels": {} }
        ], "has_more": false, "next_page_token": null })
    }

    #[test]
    fn games_are_found_and_mock_voices_skipped() {
        let root = temp_game("find", CAST);
        std::fs::create_dir_all(root.join("target/x/art")).unwrap();
        std::fs::write(root.join("target/x/art/voices.toml"), CAST).unwrap();
        let games = find_games(&[root.clone()]);
        assert_eq!(games, vec![root.join("games/duel")], "build output skipped");
        let (refs, errors) = cast_refs(&games);
        assert!(errors.is_empty());
        let ids: Vec<&str> = refs.iter().map(|r| r.voice_id.as_str()).collect();
        assert_eq!(ids, vec!["V_GRANNY", "V_ROCKO"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slots_report_sorts_the_account_by_use() {
        let root = temp_game("report", CAST);
        let fake = Rc::new(Fake::new(vec![reply(200, subscription(10, 10)), reply(200, voice_list())]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let r = slots_report(&e, &[root.clone()]).unwrap();
        assert!(r.contains("voice slots: 10 of 10 used, 0 free (starter)"), "{r}");
        assert!(r.contains("added or edited this month: 4 of 95"), "{r}");
        let in_use = &r[r.find("in use here (1)").unwrap()..r.find("left behind").unwrap()];
        assert!(in_use.contains("V_ROCKO") && in_use.contains(": rocko"), "{r}");
        assert!(r[r.find("left behind").unwrap()..].contains("V_TEMP"), "{r}");
        let unused = &r[r.find("not used by any game here (1)").unwrap()..];
        assert!(unused.contains("V_OLD") && unused.contains("2023-11-14"), "{r}");
        assert!(r.contains("cast here but not on this account (1)") && r.contains("V_GRANNY"), "{r}");
        assert!(r.contains("no free voice slot") && r.contains("mb voice gc"), "{r}");
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "GET https://api.elevenlabs.io/v1/user/subscription");
        assert!(sent[1].0.starts_with("GET https://api.elevenlabs.io/v2/voices?page_size=100&voice_type=non-default"), "{}", sent[1].0);
        assert_eq!(sent[1].1[0], ("xi-api-key".to_string(), "xi".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn voice_lists_follow_pages() {
        let page1 = json!({ "voices": [{ "voice_id": "A", "name": "a", "category": "generated" }], "has_more": true, "next_page_token": "tok/1=" });
        let page2 = json!({ "voices": [{ "voice_id": "B", "name": "b", "category": "cloned" }, { "voice_id": "P", "name": "p", "category": "premade" }], "has_more": false });
        let fake = Rc::new(Fake::new(vec![reply(200, page1), reply(200, page2)]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let v = e.voices(false).unwrap();
        assert_eq!(v.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["A", "B"], "premade ones never count");
        assert!(fake.sent.borrow()[1].0.ends_with("&next_page_token=tok%2F1%3D"));
    }

    #[test]
    fn missing_permissions_name_the_permission_and_where_to_set_it() {
        let denied = |perm: &str| {
            reply(
                401,
                json!({ "detail": { "status": "missing_permissions", "message": format!("The API key you used is missing the permission {perm} to execute this operation.") } }),
            )
        };
        let root = temp_game("perm", CAST);
        // Only user_read missing: the voices still show, with the hint.
        let fake = Rc::new(Fake::new(vec![denied("user_read"), reply(200, voice_list())]));
        let r = slots_report(&ElevenLabs::new("xi".into(), Box::new(fake)), &[root.clone()]).unwrap();
        assert!(r.contains("missing the `user_read` permission (User: Read)"), "{r}");
        assert!(r.contains("https://elevenlabs.io/app/settings/api-keys"), "{r}");
        assert!(r.contains("V_ROCKO"));
        // Both missing: an error naming both.
        let fake = Rc::new(Fake::new(vec![denied("user_read"), denied("voices_read")]));
        let e = slots_report(&ElevenLabs::new("xi".into(), Box::new(fake)), &[root.clone()]).unwrap_err();
        assert!(e.contains("`user_read`") && e.contains("`voices_read` permission (Voices: Read)"), "{e}");
        // design's check only notes it and carries on.
        let fake = Rc::new(Fake::new(vec![denied("user_read")]));
        assert!(check_free_slot(&ElevenLabs::new("xi".into(), Box::new(fake)), &root, "rocko", true).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_full_account_stops_design_before_it_spends() {
        let fake = Rc::new(Fake::new(vec![reply(200, subscription(10, 10))]));
        let e = check_free_slot(&ElevenLabs::new("xi".into(), Box::new(fake)), Path::new("games/duel"), "rocko", true).unwrap_err();
        assert!(e.starts_with("no free voice slot: your ElevenLabs account holds 10 of 10"), "{e}");
        assert!(e.contains("--ignore-slots") && e.contains("mb voice slots") && e.contains("mb voice cast games/duel rocko --premade"), "{e}");
        let fake = Rc::new(Fake::new(vec![reply(200, subscription(3, 10))]));
        assert!(check_free_slot(&ElevenLabs::new("xi".into(), Box::new(fake)), Path::new("games/duel"), "rocko", false).is_ok());
    }

    #[test]
    fn instant_voice_cloning_and_deletion_requests() {
        let fake = Rc::new(Fake::new(vec![reply(200, json!({ "voice_id": "TMP1", "requires_verification": false })), reply(200, json!({ "status": "ok" }))]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let files = vec![voice::RefAudio { name: "preview.wav".into(), bytes: b"RIFFxxxxWAVE".to_vec(), mime: "audio/wav".into() }];
        assert_eq!(e.clone_voice("mb temp duel rocko", "temporary", &files).unwrap(), "TMP1");
        e.delete_voice("TMP1").unwrap();
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://api.elevenlabs.io/v1/voices/add");
        let ct = sent[0].2["content_type"].as_str().unwrap();
        assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
        let body = sent[0].2["text"].as_str().unwrap();
        assert!(body.contains("name=\"name\"\r\n\r\nmb temp duel rocko\r\n"), "{body}");
        assert!(body.contains("name=\"files\"; filename=\"preview.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFxxxxWAVE\r\n"), "{body}");
        assert!(body.contains("name=\"remove_background_noise\"\r\n\r\nfalse"), "{body}");
        assert_eq!(sent[1].0, "DELETE https://api.elevenlabs.io/v1/voices/TMP1");
        assert_eq!(sent[1].1[0].0, "xi-api-key");
    }

    fn mock_game(name: &str) -> Option<PathBuf> {
        let game = std::env::temp_dir().join(format!("mb-release-{name}-{}", std::process::id())).join("duel");
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(&game).unwrap();
        voice::run(voice::VoiceCmd::Design(voice::DesignArgs {
            game: game.clone(),
            character: "rocko".into(),
            description: "a smug rock-star pilot, raspy".into(),
            sample: None,
            pick: Some(2),
            provider: Some("mock".into()),
            model: None,
            seed: Some(1),
            allow_name: vec![],
            ignore_slots: false,
            max_cost: None,
            dry_run: false,
        }))
        .unwrap();
        Some(game)
    }

    fn say(game: &Path, line: &str, text: &str) {
        let opts = voice::LineOpts { rate: 44100, quality: 4.0, ..voice::LineOpts::default() };
        voice::run(voice::VoiceCmd::Say(voice::SayArgs { game: game.into(), character: "rocko".into(), line: line.into(), text: text.into(), variants: 1, opts }))
            .unwrap();
    }

    #[test]
    fn a_released_voice_is_kept_as_audio_and_recreated_to_speak() {
        let Some(game) = mock_game("flow") else { return };
        // Picking keeps the preview's audio; the first lines are kept too.
        let cast = voice::load_voices(&game).unwrap();
        assert_eq!(cast["rocko"].reference.len(), 1);
        assert_eq!(cast["rocko"].reference[0].from, "preview 2");
        assert!(game.join("art/voice_ref/rocko/preview.ogg").is_file());
        say(&game, "t1", "Is that all you got?");
        let key_before = super::super::read_sidecar(&game.join("assets/vo/rocko/t1.ogg")).unwrap().result["key"].clone();
        assert_eq!(voice::load_voices(&game).unwrap()["rocko"].reference.len(), 2, "the first line is kept as reference audio");
        // --delete without --yes changes nothing.
        let rel = |delete: bool, yes: bool| ReleaseArgs { game: game.clone(), character: Some("rocko".into()), all: false, delete, yes };
        release(rel(true, false)).unwrap();
        assert!(voice::load_voices(&game).unwrap()["rocko"].released.is_none());
        release(rel(false, false)).unwrap();
        let r = voice::load_voices(&game).unwrap()["rocko"].released.clone().unwrap();
        assert!(!r.deleted);
        release(rel(true, true)).unwrap();
        assert!(voice::load_voices(&game).unwrap()["rocko"].released.as_ref().unwrap().deleted);
        // Speaking now recreates the voice; the line's key (what makes it stale) doesn't change.
        say(&game, "t1", "Is that all you got?");
        let sc = super::super::read_sidecar(&game.join("assets/vo/rocko/t1.ogg")).unwrap();
        assert!(sc.result["voice"].as_str().unwrap().starts_with("recreated from art/voice_ref/rocko/"), "{}", sc.result);
        assert_eq!(sc.result["key"], key_before);
        assert_eq!(voice::load_voices(&game).unwrap()["rocko"].reference.len(), 2, "a recreation's lines aren't reference audio");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    #[test]
    fn a_recreation_lives_for_one_batch_and_only_from_the_games_own_audio() {
        let Some(game) = mock_game("batch") else { return };
        let mut cast = voice::load_voices(&game).unwrap();
        let e = cast.get_mut("rocko").unwrap();
        e.provider = "elevenlabs".into();
        e.voice_id = Some("V_ROCKO".into());
        e.released = Some(voice::Released { date: "x".into(), deleted: true, kept_voice_id: None });
        voice::save_voices(&game, &cast).unwrap();
        let pcm = super::super::provider::b64(&[0u8, 0, 10, 0, 20, 0]);
        let speech = || reply(200, json!({ "audio_base64": pcm, "alignment": null }));
        let fake = Rc::new(Fake::new(vec![
            reply(200, subscription(3, 10)),
            reply(200, json!({ "voice_id": "TMP1", "requires_verification": false })),
            speech(),
            speech(),
            reply(200, json!({ "status": "ok" })),
        ]));
        let mut sp = Speakers::new(&game, Box::new(ElevenLabs::new("xi".into(), Box::new(fake.clone()))), false);
        let job = |line: &str| voice::LineJob {
            kind: "voice".into(),
            character: "rocko".into(),
            line: line.into(),
            text: "Ha!".into(),
            emotion: None,
            delivery: "natural".into(),
            style: None,
            speed: None,
            provider: "elevenlabs".into(),
            model: "eleven_v4".into(),
            voice_id: "V_ROCKO".into(),
            seed: 1,
            rate: 44100,
            quality: 4.0,
            out: format!("assets/vo/rocko/{line}.ogg"),
            allow_names: vec![],
        };
        let req = || voice::SpeechRequest {
            model: "eleven_v4".into(),
            voice_id: "V_ROCKO".into(),
            text: "Ha!".into(),
            stability: 0.5,
            similarity: 0.75,
            style: None,
            speed: None,
            seed: None,
        };
        assert!(sp.speak(&job("a"), req()).unwrap().1);
        assert!(sp.speak(&job("b"), req()).unwrap().1, "the second line reuses the recreation");
        sp.finish().unwrap();
        let sent = fake.sent.borrow();
        let urls: Vec<&str> = sent.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(urls[0], "GET https://api.elevenlabs.io/v1/user/subscription");
        assert_eq!(urls[1], "https://api.elevenlabs.io/v1/voices/add");
        assert!(sent[1].2["text"].as_str().unwrap().contains("mb temp duel rocko"));
        assert!(urls[2].contains("/v1/text-to-speech/TMP1/") && urls[3].contains("/v1/text-to-speech/TMP1/"), "{urls:?}");
        assert_eq!(urls[4], "DELETE https://api.elevenlabs.io/v1/voices/TMP1");
        assert_eq!(urls.len(), 5);
        drop(sent);
        // A reference file that isn't what mb kept is refused (never another voice's audio).
        std::fs::write(game.join("art/voice_ref/rocko/preview.ogg"), b"someone else").unwrap();
        let e = reference_audio(&game, "rocko", &voice::load_voices(&game).unwrap()["rocko"]).unwrap_err();
        assert!(e.contains("changed since mb kept it"), "{e}");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    #[test]
    fn a_full_account_cant_recreate_and_says_why() {
        let Some(game) = mock_game("full") else { return };
        let mut cast = voice::load_voices(&game).unwrap();
        let e = cast.get_mut("rocko").unwrap();
        e.released = Some(voice::Released { date: "x".into(), deleted: true, kept_voice_id: None });
        let entry = e.clone();
        voice::save_voices(&game, &cast).unwrap();
        let fake = Rc::new(Fake::new(vec![reply(200, subscription(10, 10))]));
        let mut sp = Speakers::new(&game, Box::new(ElevenLabs::new("xi".into(), Box::new(fake))), false);
        let err = sp.recreate("rocko", &entry).unwrap_err();
        assert!(err.starts_with("no free voice slot") && err.contains("mb voice slots"), "{err}");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    fn premade_list() -> serde_json::Value {
        json!({ "voices": [
            { "voice_id": "21m00Tcm4TlvDq8ikWAM", "name": "Rachel", "category": "premade",
              "labels": { "accent": "american", "age": "young", "gender": "female", "descriptive": "calm", "use_case": "narration" } },
            { "voice_id": "pNInz6obpgDQGcFmaJgB", "name": "Adam", "category": "premade",
              "labels": { "accent": "american", "age": "middle_aged", "gender": "male", "descriptive": "deep" } },
            { "voice_id": "XB0fDUnXU5powFXDhCwa", "name": "Charlotte", "category": "premade",
              "labels": { "accent": "swedish", "age": "young", "gender": "female" } }
        ], "has_more": false })
    }

    #[test]
    fn premades_are_listed_with_labels_and_found_by_name() {
        let fake = Rc::new(Fake::new(vec![reply(200, premade_list())]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let r = premades_report(&e, &["female".into()]).unwrap();
        assert!(r.contains("premade voices (2 of 3"), "{r}");
        assert!(r.contains("Rachel") && r.contains("female, young, american, calm, narration"), "{r}");
        assert!(!r.contains("Adam"));
        assert!(fake.sent.borrow()[0].0.contains("/v2/voices?page_size=100&category=premade"), "{}", fake.sent.borrow()[0].0);
        let all: Vec<AccountVoice> = premade_list()["voices"].as_array().unwrap().iter().map(|v| AccountVoice {
            id: v["voice_id"].as_str().unwrap().into(),
            name: v["name"].as_str().unwrap().into(),
            ..Default::default()
        }).collect();
        assert_eq!(find_premade(&all, "rachel").unwrap().id, "21m00Tcm4TlvDq8ikWAM");
        assert_eq!(find_premade(&all, "pNInz6obpgDQGcFmaJgB").unwrap().name, "Adam");
        assert_eq!(find_premade(&all, "Char").unwrap().name, "Charlotte");
        assert!(find_premade(&all, "Zed").unwrap_err().contains("mb voice premades"));
    }

    #[test]
    fn casting_a_premade_takes_no_slot_and_keeps_style_settings() {
        let root = temp_game("premade", CAST);
        let game = root.join("games/duel");
        let fake = Rc::new(Fake::new(vec![reply(200, premade_list())]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake));
        let args = CastArgs {
            game: game.clone(),
            character: "rocko".into(),
            premade: "Adam".into(),
            delivery: Some("creative".into()),
            style: Some(0.6),
            speed: Some(1.1),
            model: None,
            provider: None,
        };
        cast_with(&e, "elevenlabs", args).unwrap();
        let c = voice::load_voices(&game).unwrap();
        let r = &c["rocko"];
        assert_eq!((r.voice_id.as_deref(), r.premade.as_deref()), (Some("pNInz6obpgDQGcFmaJgB"), Some("Adam")));
        assert_eq!((r.delivery.as_deref(), r.style, r.speed), (Some("creative"), Some(0.6), Some(1.1)));
        assert_eq!(r.description, "a smug rock-star pilot", "the character's description stays");
        // A premade voice isn't the account's: slots don't count it, release leaves it alone.
        let (refs, _) = cast_refs(&[game.clone()]);
        assert!(refs.iter().all(|x| x.character != "rocko"));
        // A new character gets an entry of its own.
        let fake = Rc::new(Fake::new(vec![reply(200, premade_list())]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake));
        let args = CastArgs { game: game.clone(), character: "ref".into(), premade: "rachel".into(), delivery: None, style: None, speed: None, model: None, provider: None };
        cast_with(&e, "elevenlabs", args).unwrap();
        let c = voice::load_voices(&game).unwrap();
        assert!(c["ref"].description.starts_with("premade voice Rachel: female"), "{}", c["ref"].description);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn gc_list() -> serde_json::Value {
        json!({ "voices": [
            { "voice_id": "V_ROCKO", "name": "duel rocko", "category": "generated" },
            { "voice_id": "V_GRANNY", "name": "duel granny", "category": "generated" },
            { "voice_id": "V_OLD", "name": "Old Narrator", "category": "generated" },
            { "voice_id": "V_MINE", "name": "My own clone", "category": "cloned" },
            { "voice_id": "V_TEMP", "name": "mb temp duel granny", "category": "cloned" },
            { "voice_id": "V_BOLT", "name": "boltbrawl kai", "category": "generated" },
            { "voice_id": "V_DOT", "name": " mags", "category": "generated" }
        ], "has_more": false })
    }

    #[test]
    fn gc_is_a_dry_run_until_yes_and_deletes_exactly_what_it_listed() {
        let root = temp_game("gc", CAST);
        let game = root.join("games/duel");
        let mut c = voice::load_voices(&game).unwrap();
        c.get_mut("rocko").unwrap().released = Some(voice::Released { date: "x".into(), deleted: false, kept_voice_id: None });
        voice::save_voices(&game, &c).unwrap();
        // Dry run: lists, deletes nothing.
        let fake = Rc::new(Fake::new(vec![reply(200, gc_list())]));
        gc_with(&ElevenLabs::new("xi".into(), Box::new(fake.clone())), &[root.clone()], false, &[], false).unwrap();
        assert!(fake.sent.borrow().iter().all(|s| !s.0.starts_with("DELETE")));
        // --yes, sparing one with --except.
        let ok = || reply(200, json!({ "status": "ok" }));
        let fake = Rc::new(Fake::new(vec![reply(200, gc_list()), ok(), ok(), ok()]));
        gc_with(&ElevenLabs::new("xi".into(), Box::new(fake.clone())), &[root.clone()], true, &["V_MINE".into()], false).unwrap();
        let deleted: Vec<String> = fake.sent.borrow().iter().filter(|s| s.0.starts_with("DELETE")).map(|s| s.0.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(deleted, vec!["V_ROCKO", "V_OLD", "V_TEMP"], "never V_GRANNY (in use), V_MINE (--except) or another game's");
        assert_eq!(mb_game_of("boltbrawl kai").as_deref(), Some("boltbrawl"));
        assert_eq!(mb_game_of(" mags").as_deref(), Some(""));
        assert_eq!(mb_game_of("mb temp duel granny").as_deref(), Some("duel"));
        assert_eq!(mb_game_of("Old Narrator"), None);
        // --other-games takes those too.
        let fake = Rc::new(Fake::new(vec![reply(200, gc_list()), ok(), ok(), ok()]));
        gc_with(&ElevenLabs::new("xi".into(), Box::new(fake.clone())), &[root.clone()], true, &["V_MINE".into(), "V_OLD".into(), "V_ROCKO".into()], true).unwrap();
        let deleted: Vec<String> = fake.sent.borrow().iter().filter(|s| s.0.starts_with("DELETE")).map(|s| s.0.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(deleted, vec!["V_TEMP", "V_BOLT", "V_DOT"], "{deleted:?}");
        assert!(voice::load_voices(&game).unwrap()["rocko"].released.as_ref().unwrap().deleted, "the game knows its released voice is gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gc_refuses_without_games_and_stops_on_a_missing_permission() {
        let empty = std::env::temp_dir().join(format!("mb-gc-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let fake = Rc::new(Fake::new(vec![]));
        let e = gc_with(&ElevenLabs::new("xi".into(), Box::new(fake.clone())), &[empty.clone()], true, &[], false).unwrap_err();
        assert!(e.contains("no art/voices.toml"), "{e}");
        assert!(fake.sent.borrow().is_empty(), "not even a list call");
        let root = temp_game("gcperm", CAST);
        let denied = reply(
            401,
            json!({ "detail": { "status": "missing_permissions", "message": "The API key you used is missing the permission voices_write to execute this operation." } }),
        );
        let fake = Rc::new(Fake::new(vec![reply(200, gc_list()), denied]));
        let e = gc_with(&ElevenLabs::new("xi".into(), Box::new(fake.clone())), &[root.clone()], true, &[], false).unwrap_err();
        assert!(e.contains("can't delete voices: it's missing the `voices_write` permission (Voices: Write)"), "{e}");
        assert_eq!(fake.sent.borrow().iter().filter(|s| s.0.starts_with("DELETE")).count(), 1, "stops after the first");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
