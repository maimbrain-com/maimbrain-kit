//! Voice-acted dialogue: pixel-art portraits that talk in time with the
//! voice, a dialogue box whose text types out as the words are spoken, short
//! barks that never block play, and music that dips under the voice.
//!
//! `mb voice say` writes each line as `assets/vo/<character>/<line-id>.ogg`
//! (the voice) and `<line-id>.lips.txt` (when each word is spoken and how
//! open the mouth is, frame by frame: [`Lips`]). A [`VoiceLine`] loads the
//! pair from the base path; a [`Portrait`] draws a face from a sprite sheet
//! with its mouth following the line; a [`Dialogue`] queues lines and draws
//! them, as a cutscene (a box at the bottom that waits for a tap, or moves on
//! by itself when the voice is done) or as barks (a small bubble that goes
//! away by itself), picked from situation pools by [`super::Barks`].
//!
//! ```ignore
//! use maimbrain::ui::{Dialogue, DialogueEvent, Portrait, VoiceLine};
//! // init
//! let rocko = Portrait::load("assets/portraits/rocko.png", 32, 32).expression("smug", 1).blinks(2);
//! let taunt = VoiceLine::load("assets/vo/rocko/taunt1");
//! let mut talk = Dialogue::new(theme).cast("rocko", rocko);
//! talk.barks.load_script(include_str!("../art/lines.csv")).unwrap();
//! talk.say("rocko", &taunt, "smug");            // a cutscene line
//! talk.barks.trigger("part_broken");            // during play: a bark, if one may start
//! // update
//! for e in &events { if talk.input(e) { continue } /* the game's own input */ }
//! if talk.update(dt) == Some(DialogueEvent::Finished) { /* back to play */ }
//! if !talk.active() { /* step the game */ }
//! // render, over the scene
//! talk.draw(&layout);
//! ```
//!
//! Subtitles always show: many players have the sound off, and a line whose
//! audio is missing or still loading runs on its text alone. Everything is
//! timed from game time ([`crate::sys::time`]), so it replays exactly.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use super::barks::Barks;
use super::damage::{self, Condition, Grid, Reaction};
use super::layout::{Layout, Rect};
use super::shape::poly;
use super::text::text;
use super::theme::Theme;
use super::widgets::{panel, pill_in, pill_width};
use crate::audio::{Bus, Sound, Voice};
use crate::gfx2d::{self, Image};
use crate::input::{Event, Kind};
use crate::motion::Ease;
use crate::sys::{self, Asset, AssetState};

/// Characters a second for text with no timing of its own.
const FALLBACK_CPS: f32 = 30.0;
/// How long a line whose timing file couldn't load stays up.
const FAILED_DUR: f32 = 2.0;
/// How far ahead a line is scheduled, so its voice starts from the top.
const LEAD: f64 = 0.1;
/// How long a line waits for its files before it starts without them.
const MAX_WAIT: f64 = 1.0;
/// After the box opens, taps are ignored this long (the tap that started
/// the cutscene shouldn't also skip its first line).
const TAP_GUARD: f64 = 0.2;

// ---------------------------------------------------------------------------
// Lips

/// A voice line's timing, parsed from its `.lips.txt` file:
///
/// ```text
/// mblips 1
/// hz 30
/// dur 1.84
/// mouth 0012332100123321000
/// text Eat my dust, grandma!
/// reveal 0 40 80 120 ...
/// ```
///
/// `mouth` is one digit per frame at `hz` frames a second from the start of
/// the line: 0 closed, 1 slightly open, 2 open, 3 wide. `dur` is the audio's
/// length in seconds, `text` the subtitle, `reveal` the millisecond each
/// character of `text` is spoken (one per `char`). Unknown lines are skipped,
/// so newer files still load; without `reveal` the text types out evenly
/// over `dur`, and without `mouth` the mouth flaps with the letters.
#[derive(Clone, Debug, PartialEq)]
pub struct Lips {
    hz: f32,
    dur: f32,
    mouth: Vec<u8>,
    text: String,
    /// Seconds each char is spoken at (non-decreasing), or empty.
    reveal: Vec<f32>,
    chars: usize,
}

impl Lips {
    /// Parses a `.lips.txt` file. None if it isn't UTF-8 or doesn't start
    /// with `mblips 1`.
    pub fn parse(bytes: &[u8]) -> Option<Lips> {
        let s = std::str::from_utf8(bytes).ok()?;
        let mut lines = s.lines();
        let mut head = lines.next()?.split_whitespace();
        if head.next()? != "mblips" || head.next()?.parse::<u32>().ok()? != 1 {
            return None;
        }
        let (mut hz, mut dur, mut mouth, mut text, mut reveal) = (30.0f32, None, Vec::new(), String::new(), Vec::new());
        for line in lines {
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "hz" => hz = rest.trim().parse().ok().filter(|h: &f32| h.is_finite() && *h > 0.0).unwrap_or(30.0),
                "dur" => dur = rest.trim().parse::<f32>().ok().filter(|d| d.is_finite() && *d >= 0.0),
                "mouth" => mouth = rest.trim().bytes().filter(u8::is_ascii_digit).map(|b| (b - b'0').min(3)).collect(),
                "text" => text = rest.to_string(),
                "reveal" => {
                    let ms: Option<Vec<f32>> = rest.split_whitespace().map(|v| v.parse::<f32>().ok().filter(|v| v.is_finite())).collect();
                    let mut last = 0.0f32;
                    // Kept non-decreasing whatever the file says.
                    reveal = ms.unwrap_or_default().into_iter().map(|m| {
                        last = last.max(m / 1000.0);
                        last
                    }).collect();
                }
                _ => {}
            }
        }
        let chars = text.chars().count();
        let dur = dur
            .or_else(|| (!mouth.is_empty()).then(|| mouth.len() as f32 / hz))
            .or_else(|| reveal.last().map(|r| r + 0.3))
            .unwrap_or(chars as f32 / FALLBACK_CPS + 0.3);
        Some(Lips { hz, dur, mouth, text, reveal, chars })
    }

    /// Timing for a line with no file: the text types out at 30 characters a
    /// second and the mouth flaps with the letters.
    pub fn from_text(text: &str) -> Lips {
        let chars = text.chars().count();
        Lips { hz: 30.0, dur: chars as f32 / FALLBACK_CPS + 0.3, mouth: Vec::new(), text: text.to_string(), reveal: Vec::new(), chars }
    }

    /// Parses a loaded asset. None until it's ready, or if it isn't a valid file.
    pub fn new(asset: Asset) -> Option<Lips> {
        (asset.state() == AssetState::Ready).then(|| asset.read()).flatten().and_then(|b| Lips::parse(&b))
    }

    /// Seconds of audio.
    pub fn dur(&self) -> f32 {
        self.dur
    }

    /// The subtitle.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How open the mouth is `t` seconds into the line: 0 closed … 3 wide;
    /// 0 before and after.
    pub fn mouth_at(&self, t: f32) -> u8 {
        if !(0.0..self.dur).contains(&t) {
            return 0;
        }
        if self.mouth.is_empty() {
            return self.flap(t);
        }
        self.mouth.get((t * self.hz) as usize).copied().unwrap_or(0)
    }

    /// How many characters of the text have been spoken `t` seconds in (all
    /// of them from `dur` on).
    pub fn chars_at(&self, t: f32) -> usize {
        if t < 0.0 {
            0
        } else if t >= self.dur {
            self.chars
        } else if !self.reveal.is_empty() {
            let n = self.reveal.len().min(self.chars);
            self.reveal[..n].partition_point(|&r| r <= t)
        } else if self.dur > 0.0 {
            ((t / self.dur * self.chars as f32) as usize + 1).min(self.chars)
        } else {
            self.chars
        }
    }

    /// A mouth for lines without one: open on the letter being spoken
    /// (wider on vowels), shut between, flapping at 5 Hz.
    fn flap(&self, t: f32) -> u8 {
        let n = self.chars_at(t);
        if n == 0 || (t * 10.0) as i64 % 2 == 1 {
            return 0;
        }
        match self.text.chars().nth(n - 1) {
            Some(c) if "aeiouyAEIOUY".contains(c) => 2,
            Some(c) if c.is_alphanumeric() => 1,
            _ => 0,
        }
    }
}

/// The first line of a lips pack: every line's timing in a directory in one
/// file, which `mb build` writes as `assets/vo/<character>/lips.txt` (each
/// file is one of a bundle's 512 entries). After it, each line's `.lips.txt`
/// follows an `@ <line-id>` line:
///
/// ```text
/// mblipspack 1
/// @ taunt1
/// mblips 1
/// dur 1.84
/// text Eat my dust, grandma!
/// @ ouch
/// mblips 1
/// ...
/// ```
pub const LIPS_PACK_HEADER: &str = "mblipspack 1";

/// Parses a lips pack ([`LIPS_PACK_HEADER`]): line id → timing. Entries that
/// don't parse are left out. None if it isn't a pack.
pub fn parse_lips_pack(bytes: &[u8]) -> Option<HashMap<String, Lips>> {
    let s = std::str::from_utf8(bytes).ok()?;
    let mut lines = s.lines();
    if lines.next()?.trim() != LIPS_PACK_HEADER {
        return None;
    }
    let mut out = HashMap::new();
    let mut cur: Option<(&str, String)> = None;
    let finish = |cur: Option<(&str, String)>, out: &mut HashMap<String, Lips>| {
        if let Some((id, body)) = cur
            && let Some(l) = Lips::parse(body.as_bytes())
        {
            out.insert(id.to_string(), l);
        }
    };
    for line in lines {
        if let Some(id) = line.strip_prefix("@ ") {
            finish(cur.take(), &mut out);
            cur = Some((id.trim(), String::new()));
        } else if let Some((_, body)) = &mut cur {
            body.push_str(line);
            body.push('\n');
        }
    }
    finish(cur, &mut out);
    Some(out)
}

/// A script line's text as a subtitle: audio tags (`[laughs]`,
/// `[whispers]`) left out, spaces collapsed.
pub fn subtitle_text(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A directory's lips pack, shared by every line in it.
#[derive(Debug)]
enum Pack {
    Loading(Asset),
    Ready(Rc<HashMap<String, Lips>>),
    /// Not in the bundle (a game run from its folder, or packed by an older
    /// `mb`): each line loads its own `.lips.txt`.
    Missing,
}

thread_local! {
    static PACKS: RefCell<HashMap<String, Rc<RefCell<Pack>>>> = RefCell::new(HashMap::new());
}

fn pack_for(dir: &str) -> Rc<RefCell<Pack>> {
    PACKS.with(|p| {
        p.borrow_mut()
            .entry(dir.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(Pack::Loading(Asset::load(&format!("{dir}/lips.txt"))))))
            .clone()
    })
}

impl Pack {
    fn poll(&mut self) {
        if let Pack::Loading(a) = *self {
            *self = match a.state() {
                AssetState::Pending => return,
                AssetState::Failed => Pack::Missing,
                AssetState::Ready => match a.read().and_then(|b| parse_lips_pack(&b)) {
                    Some(m) => Pack::Ready(Rc::new(m)),
                    None => Pack::Missing,
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceLine

#[derive(Debug)]
struct Files {
    sound_asset: Option<Asset>,
    /// The directory's lips pack and this line's id in it, until it's looked in.
    pack: Option<(Rc<RefCell<Pack>>, String)>,
    lips_asset: Option<Asset>,
    sound: Option<Sound>,
    lips: Option<Lips>,
    failed: bool,
    path: String,
    /// The line's script text, shown (typed out at 30 characters a second)
    /// when its timing doesn't load.
    fallback: Option<String>,
}

impl Files {
    fn poll(&mut self) {
        if let Some((pack, id)) = &self.pack {
            pack.borrow_mut().poll();
            let found = match &*pack.borrow() {
                Pack::Loading(_) => None,
                Pack::Ready(m) => Some(m.get(id).cloned()),
                Pack::Missing => Some(None),
            };
            if let Some(found) = found {
                self.pack = None;
                match found {
                    Some(l) => self.lips = Some(l),
                    None => self.lips_asset = Some(Asset::load(&format!("{}.lips.txt", self.path))),
                }
            }
        }
        if let Some(a) = self.sound_asset {
            match a.state() {
                AssetState::Pending => {}
                AssetState::Failed => {
                    self.sound_asset = None;
                    sys::log(sys::Level::Warn, &format!("VoiceLine: {}.ogg didn't load; the line shows its subtitle without a voice", self.path));
                }
                AssetState::Ready => {
                    self.sound = Sound::new(a);
                    self.sound_asset = None;
                }
            }
        }
        if let Some(a) = self.lips_asset {
            match a.state() {
                AssetState::Pending => {}
                AssetState::Failed => self.fail(),
                AssetState::Ready => match Lips::new(a) {
                    Some(l) => {
                        self.lips = Some(l);
                        self.lips_asset = None;
                    }
                    None => self.fail(),
                },
            }
        }
    }

    fn fail(&mut self) {
        self.lips_asset = None;
        match &self.fallback {
            Some(text) => {
                self.lips = Some(Lips::from_text(text));
                sys::log(sys::Level::Warn, &format!("VoiceLine: {}.lips.txt didn't load; showing the script's text as the subtitle", self.path));
            }
            None => {
                self.failed = true;
                sys::log(sys::Level::Warn, &format!("VoiceLine: {}.lips.txt didn't load; the line plays without subtitles (give it its text with VoiceLine::text_if_missing)", self.path));
            }
        }
    }
}

/// One spoken line: its voice (`.ogg`) and its timing and subtitle
/// (`.lips.txt`, [`Lips`]). Cloning is cheap and clones share the loading
/// files, so keep one per line and pass it to [`Dialogue::say`] as often as
/// you like.
#[derive(Clone, Debug)]
pub struct VoiceLine {
    files: Rc<RefCell<Files>>,
    /// Playback volume, 0–4 (1 = as recorded).
    pub vol: f32,
}

/// A line that's been started: when it's heard from and for how long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing {
    /// Game time the line is heard from.
    pub start: f64,
    /// Seconds it lasts.
    pub dur: f32,
    /// The voice, if the line has sound.
    pub voice: Option<Voice>,
}

impl Playing {
    /// Seconds since the line started (negative while it's scheduled).
    pub fn elapsed(&self) -> f32 {
        (sys::time() - self.start) as f32
    }

    /// Whether its voice is (or is about to be) heard now.
    pub fn speaking(&self) -> bool {
        self.voice.is_some() && !self.done()
    }

    pub fn done(&self) -> bool {
        sys::time() >= self.start + self.dur as f64
    }

    /// Lets the voice die away quickly.
    pub fn stop(&self) {
        if let Some(v) = self.voice {
            v.stop_at(0.0, 0.08);
        }
    }
}

impl VoiceLine {
    /// Starts loading `<base>.ogg` and its timing (base like
    /// "assets/vo/rocko/taunt1"): from the directory's lips pack
    /// (`assets/vo/rocko/lips.txt`, which `mb build` writes; one load for
    /// all of a character's lines), or `<base>.lips.txt` when the pack
    /// doesn't have it (a game run from its folder, or an older bundle).
    /// Load lines in `init`: a line asked for while its files are still
    /// loading waits up to a second for them.
    pub fn load(base: &str) -> VoiceLine {
        let (pack, lips_asset) = match base.rsplit_once('/') {
            Some((dir, id)) => (Some((pack_for(dir), id.to_string())), None),
            None => (None, Some(Asset::load(&format!("{base}.lips.txt")))),
        };
        let files = Files {
            sound_asset: Some(Asset::load(&format!("{base}.ogg"))),
            pack,
            lips_asset,
            sound: None,
            lips: None,
            failed: false,
            path: base.to_string(),
            fallback: None,
        };
        VoiceLine { files: Rc::new(RefCell::new(files)), vol: 1.0 }
    }

    /// The line's script text, for when its files don't load: without its
    /// timing the subtitle is this text typed out at 30 characters a second
    /// (the portrait's mouth flapping with it), and without its voice the
    /// subtitle still shows. Audio tags like `[laughs]` are left out.
    /// [`super::Barks::load_script`] gives every line its text.
    pub fn text_if_missing(self, text: &str) -> VoiceLine {
        {
            let mut f = self.files.borrow_mut();
            let t = subtitle_text(text);
            if f.failed && f.lips.is_none() && !t.is_empty() {
                f.lips = Some(Lips::from_text(&t));
                f.failed = false;
            }
            f.fallback = (!t.is_empty()).then_some(t);
        }
        self
    }

    /// A line with no recording: the text types out at 30 characters a
    /// second and the portrait's mouth flaps with it.
    pub fn subtitle(text: &str) -> VoiceLine {
        VoiceLine::new(None, Lips::from_text(text))
    }

    /// A line from a sound and timing you already have.
    pub fn new(sound: Option<Sound>, lips: Lips) -> VoiceLine {
        let files = Files { sound_asset: None, pack: None, lips_asset: None, sound, lips: Some(lips), failed: false, path: String::new(), fallback: None };
        VoiceLine { files: Rc::new(RefCell::new(files)), vol: 1.0 }
    }

    pub fn volume(mut self, vol: f32) -> VoiceLine {
        self.vol = vol;
        self
    }

    /// Picks up the files once they're ready (cheap; [`Dialogue`] calls it).
    /// True when the subtitle is known, or its file failed to load.
    pub fn poll(&self) -> bool {
        self.files.borrow_mut().poll();
        self.ready()
    }

    /// Which line this is: the same for its clones.
    pub(crate) fn key(&self) -> usize {
        Rc::as_ptr(&self.files) as usize
    }

    /// The timing is loaded (or failed, so the line won't wait for it).
    pub fn ready(&self) -> bool {
        let f = self.files.borrow();
        f.lips.is_some() || f.failed
    }

    pub fn has_sound(&self) -> bool {
        self.files.borrow().sound.is_some()
    }

    /// The timing, once loaded.
    pub fn lips(&self) -> Option<Lips> {
        self.files.borrow().lips.clone()
    }

    /// The subtitle ("" until loaded).
    pub fn text(&self) -> String {
        self.files.borrow().lips.as_ref().map(|l| l.text.clone()).unwrap_or_default()
    }

    /// Seconds the line lasts.
    pub fn dur(&self) -> f32 {
        self.files.borrow().lips.as_ref().map_or(FAILED_DUR, Lips::dur)
    }

    /// [`Lips::mouth_at`] (0 without timing).
    pub fn mouth_at(&self, t: f32) -> u8 {
        self.files.borrow().lips.as_ref().map_or(0, |l| l.mouth_at(t))
    }

    /// [`Lips::chars_at`] (0 without timing).
    pub fn chars_at(&self, t: f32) -> usize {
        self.files.borrow().lips.as_ref().map_or(0, |l| l.chars_at(t))
    }

    /// Schedules the line to be heard from game time `at` (on [`Bus::Sfx`],
    /// where every voice plays; there's no separate voice bus). Without a
    /// sound (missing, still decoding, or a [`VoiceLine::subtitle`]) it
    /// still returns its timing, so the subtitles run on their own.
    pub fn play(&self, at: f64) -> Playing {
        self.poll();
        let voice = self.files.borrow().sound.map(|s| s.play_at(self.vol, 0.0, 1.0, at));
        Playing { start: at, dur: self.dur(), voice }
    }
}

// ---------------------------------------------------------------------------
// Ducker

/// Dips the music ([`Bus::Music`]'s volume) by `db` decibels while a voice
/// is speaking, and brings it back after. 3–6 dB keeps the music present
/// and the words clear; 5 is the default.
///
/// It owns `Bus::Music`'s volume once it has ducked: a game that sets its own
/// music level (a menu fade, a settings slider) gives it to
/// [`Ducker::set_base`] instead of calling `Bus::Music.volume`, and the
/// ducker multiplies the two. Until the first line it never touches the bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ducker {
    /// How far the music dips, in dB (0 = off).
    pub db: f32,
    /// Seconds to dip.
    pub attack: f32,
    /// Seconds to come back.
    pub release: f32,
    base: f32,
    on: bool,
    sent: Option<f32>,
}

impl Default for Ducker {
    fn default() -> Self {
        Ducker::new(5.0)
    }
}

impl Ducker {
    pub fn new(db: f32) -> Ducker {
        Ducker { db, attack: 0.12, release: 0.6, base: 1.0, on: false, sent: None }
    }

    /// The game's own music level (0–4, 1 = as is), applied now with a short
    /// glide; ducking multiplies it.
    pub fn set_base(&mut self, gain: f32) {
        self.base = gain;
        if self.sent.is_some() {
            self.send(0.1);
        }
    }

    pub fn base(&self) -> f32 {
        self.base
    }

    /// Ducks (or not). Sends a volume to the bus only when it changes.
    pub fn set(&mut self, on: bool) {
        let changed = on != self.on;
        self.on = on;
        if changed && (on || self.sent.is_some()) {
            self.send(if on { self.attack } else { self.release });
        }
    }

    pub fn ducked(&self) -> bool {
        self.on
    }

    /// The music volume it's heading for.
    pub fn gain(&self) -> f32 {
        if self.on { self.base * 10f32.powf(-self.db.max(0.0) / 20.0) } else { self.base }
    }

    fn send(&mut self, ramp: f32) {
        let g = self.gain();
        if self.sent != Some(g) {
            Bus::Music.volume(g, ramp);
            self.sent = Some(g);
        }
    }
}

// ---------------------------------------------------------------------------
// Portrait

/// Whether a face blinks at game time `t`: about every 2.5–4.5 s, for 0.12
/// s, at moments fixed by `t` and `seed` alone (so it replays, and two faces
/// with different seeds don't blink together).
pub fn blinking(t: f64, seed: u32) -> bool {
    const SLOT: f64 = 3.5;
    const LEN: f64 = 0.12;
    if t < 0.0 {
        return false;
    }
    let k = (t / SLOT).floor();
    // Each 3.5 s slot has one blink somewhere in its first second, so blinks
    // are 2.5–4.5 s apart.
    let at = k * SLOT + unit_hash(k as u64 ^ ((seed as u64) << 32));
    t >= at && t < at + LEN
}

/// A number in [0, 1) from `x` (splitmix64).
fn unit_hash(x: u64) -> f64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// A pixel-art face drawn from a sprite sheet of equal cells, 4 columns
/// wide. Each row is one expression and its columns are the mouth: closed,
/// slightly open, open, wide (mouth 0–3). Row 0 is the neutral face; name
/// more rows with [`Portrait::expression`]. An optional blink row
/// ([`Portrait::blinks`], the last row `mb art portrait` writes) is the
/// neutral face with its eyes shut, with the same four mouths.
///
/// ```text
///          mouth 0   mouth 1   mouth 2   mouth 3
/// row 0    neutral   neutral   neutral   neutral
/// row 1    smug      smug      smug      smug        .expression("smug", 1)
/// row 2    angry     angry     angry     angry       .expression("angry", 2)
/// row 3    blink     blink     blink     blink       .blinks(3)
/// ```
///
/// The neutral face blinks, talking or not; the other expressions hold their
/// eyes. It lifts one art pixel on open vowels while talking.
///
/// **Damage** ([`super::damage`]): [`Portrait::set_health`] (0–1) shows
/// sweat, then soot, cracked glass, static and a flickering light, then
/// sparks and a red alarm with a WARNING tag; [`Portrait::hit`] shakes and
/// flashes the face, knocks it aside and springs it back, with a pained face
/// for a moment. Rows named "hurt" and "battered" (`mb art portrait
/// --expressions smug,angry,hurt,battered`) are used when the sheet has
/// them: "hurt" for a hit and from health 0.5 down, "battered" from 0.25
/// down, in place of the neutral face. Clones share the damage, so hit the
/// game's copy and the dialogue's shows it.
#[derive(Clone, Debug)]
pub struct Portrait {
    asset: Option<Asset>,
    image: Cell<Option<Image>>,
    /// Size of one cell, in sheet pixels.
    pub cell_w: f32,
    pub cell_h: f32,
    expressions: Vec<(String, u32)>,
    blink_row: Option<u32>,
    /// Drawn on the right of the dialogue box, mirrored to face in.
    pub right: bool,
    /// Sheet pixels per art pixel ([`Portrait::art_pixels`]).
    pub density: f32,
    /// Nothing drawn outside the rect it's drawn in ([`Portrait::clipped`]).
    pub clip: bool,
    seed: u32,
    cond: Rc<RefCell<Condition>>,
}

/// The art pixels across a cell that `mb art portrait` draws pixel-art
/// faces on (its default `--grid 48x48`).
pub const PORTRAIT_GRID: u32 = 48;

/// Screen units per sheet pixel to draw a `cell_w` × `cell_h` cell, with
/// `density` sheet pixels per art pixel, as large as fits in `w` × `h`:
/// each art pixel is a whole number of device pixels (half units, at the
/// canvas's 2 device pixels per unit), at least one.
pub fn portrait_scale(cell_w: f32, cell_h: f32, density: f32, w: f32, h: f32) -> f32 {
    let d = density.max(1e-3);
    // Units per art pixel that fit, then whole half units of it.
    let fit = (w / cell_w).min(h / cell_h) * d;
    let art = ((fit * 2.0 + 1e-3).floor()).max(1.0) / 2.0;
    art / d
}

impl Portrait {
    /// Starts loading the sheet (a PNG) with cells `cell_w` × `cell_h` pixels.
    pub fn load(path: &str, cell_w: u32, cell_h: u32) -> Portrait {
        let seed = path.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
        Portrait::with(Some(Asset::load(path)), None, cell_w, cell_h, seed)
    }

    /// A sheet already decoded.
    pub fn from_image(image: Image, cell_w: u32, cell_h: u32) -> Portrait {
        Portrait::with(None, Some(image), cell_w, cell_h, image.0.wrapping_mul(0x9e37_79b9))
    }

    fn with(asset: Option<Asset>, image: Option<Image>, cell_w: u32, cell_h: u32, seed: u32) -> Portrait {
        Portrait {
            asset,
            image: Cell::new(image),
            cell_w: cell_w as f32,
            cell_h: cell_h as f32,
            expressions: Vec::new(),
            blink_row: None,
            right: false,
            density: guess_density(cell_w, cell_h),
            clip: false,
            seed,
            cond: Rc::new(RefCell::new(Condition::default())),
        }
    }

    /// Health 0–1 (1 = unhurt): picks the damage stage
    /// ([`damage::stage`]). Shared by the portrait's clones.
    pub fn set_health(&self, health: f32) {
        self.cond.borrow_mut().health = health.clamp(0.0, 1.0);
    }

    pub fn health(&self) -> f32 {
        self.cond.borrow().health
    }

    /// A hit of `strength` 0–1 now (game time): a shake, a white then red
    /// flash, a knock that springs back over ~0.5 s, the pained face for
    /// 0.25–0.6 s, and a burst of static. It doesn't change the health.
    pub fn hit(&self, strength: f32) {
        self.cond.borrow_mut().hit(strength, sys::time());
    }

    /// What the hits are doing to the face at game time `t`, for drawing it
    /// your own way (offsets are fractions of its width).
    pub fn reaction(&self, t: f64) -> Reaction {
        self.cond.borrow().reaction(t, self.seed, if self.right { 1.0 } else { -1.0 })
    }

    /// The row names used for a hit and low health ("hurt", "battered").
    pub fn damage_faces(self, hurt: &str, battered: &str) -> Portrait {
        {
            let mut c = self.cond.borrow_mut();
            c.hurt = hurt.to_string();
            c.battered = battered.to_string();
        }
        self
    }

    /// Without the damage overlays (sweat, soot, cracks, the alarm): hits
    /// and the hurt faces only.
    pub fn no_damage_overlays(self) -> Portrait {
        self.cond.borrow_mut().overlays = false;
        self
    }

    fn has(&self, expression: &str) -> bool {
        self.expressions.iter().any(|(n, _)| n == expression)
    }

    /// Names the expression drawn on sheet row `row`.
    pub fn expression(mut self, name: &str, row: u32) -> Portrait {
        self.expressions.push((name.to_string(), row));
        self
    }

    /// The sheet row with the blink frames.
    pub fn blinks(mut self, row: u32) -> Portrait {
        self.blink_row = Some(row);
        self
    }

    /// Sits on the right of the dialogue box, mirrored.
    pub fn on_right(mut self) -> Portrait {
        self.right = true;
        self
    }

    /// The cell is `across` art pixels wide: the `--grid` the sheet was made
    /// with (`mb art portrait`'s sidecar has it as `recipe.grid`). It sizes
    /// the face so every art pixel is a whole number of device pixels. A
    /// cell that's a whole multiple of 48 sheet pixels on both sides is
    /// taken as 48 art pixels across (what `mb art portrait` makes);
    /// otherwise each sheet pixel is an art pixel. `.art_pixels(cell_w)`
    /// says so for a sheet drawn one sheet pixel per art pixel.
    pub fn art_pixels(mut self, across: u32) -> Portrait {
        self.density = self.cell_w / across.max(1) as f32;
        self
    }

    /// Keeps the face, its hits and its damage inside the rect it's drawn
    /// in: a knock or shake that would cross the edge is cut off there (the
    /// face slides behind the frame), and the alarm border is the rect.
    pub fn clipped(mut self) -> Portrait {
        self.clip = true;
        self
    }

    /// Screen units per sheet pixel when drawn in `r` (see [`Portrait::draw`]).
    pub fn scale_in(&self, r: Rect) -> f32 {
        portrait_scale(self.cell_w, self.cell_h, self.density, r.w, r.h)
    }

    /// The row of a named expression (0, neutral, for "" or unknown names).
    pub fn row(&self, expression: &str) -> u32 {
        self.expressions.iter().find(|(n, _)| n == expression).map_or(0, |e| e.1)
    }

    /// The sheet cell (column, row) to draw for `expression` with the mouth
    /// at `mouth` at game time `t`.
    /// With damage: the "hurt" row while a hit stings (or the "battered"
    /// one, without a "hurt" row), and in place of the neutral face "hurt"
    /// from health 0.5 down and "battered" from 0.25 down, when the sheet
    /// has those rows.
    pub fn cell(&self, expression: &str, mouth: u8, t: f64) -> (u32, u32) {
        let row = {
            let c = self.cond.borrow();
            let st = damage::stage(c.health);
            let pained = self.reaction(t).pained;
            let pick = |names: &[&str]| names.iter().copied().find(|n| self.has(n)).map(|n| self.row(n));
            let worn = match st {
                3 => pick(&[&c.battered, &c.hurt]),
                2 => pick(&[&c.hurt]),
                _ => None,
            };
            if pained {
                pick(&[&c.hurt, &c.battered]).unwrap_or_else(|| self.row(expression))
            } else if (expression.is_empty() || !self.has(expression)) && let Some(r) = worn {
                r
            } else {
                self.row(expression)
            }
        };
        match self.blink_row {
            Some(b) if row == 0 && blinking(t, self.seed) => (mouth.min(3) as u32, b),
            _ => (mouth.min(3) as u32, row),
        }
    }

    /// The sheet, once decoded.
    pub fn image(&self) -> Option<Image> {
        if self.image.get().is_none()
            && let Some(a) = self.asset
        {
            self.image.set(Image::new(a));
        }
        self.image.get()
    }

    /// Draws the face as large as fits in `r`, standing on its bottom edge
    /// and centred across it. Each art pixel is a whole number of half units
    /// (device pixels on a 2× screen; how many sheet pixels make an art
    /// pixel is [`Portrait::art_pixels`]) at whole-unit positions, so the
    /// pixels stay square; images are filtered, so for the crispest edges
    /// store the sheet at 2× or more (SPEC §5.3). A hit knocks it past `r`
    /// unless it's [`Portrait::clipped`]. Returns where the face is
    /// (unclipped; nothing is drawn until the sheet has loaded).
    pub fn draw(&self, r: Rect, expression: &str, mouth: u8, t: f64) -> Rect {
        let px = self.scale_in(r);
        let (w, h) = (self.cell_w * px, self.cell_h * px);
        let lift = if mouth >= 2 { (px * self.density).max(1.0).round() } else { 0.0 };
        let home = Rect::new((r.cx() - w / 2.0).round(), (r.bottom() - h - lift).round(), w, h);
        let Some(img) = self.image() else { return home };
        let clip = self.clip.then_some(r);
        let cond = self.cond.borrow();
        let re = cond.reaction(t, self.seed, if self.right { 1.0 } else { -1.0 });
        // Knocked and shaken, in whole units so the pixels stay put.
        let dst = Rect::new(home.x + (re.dx * w).round(), home.y + (re.dy * w).round(), w, h);
        let (col, row) = self.cell(expression, mouth, t);
        let src = [col as f32 * self.cell_w, row as f32 * self.cell_h, self.cell_w, self.cell_h];
        let grid = Grid { clip, ..Grid::over(dst) };
        let glitch = damage::glitch(&cond, &re, t, self.seed);
        let blit = |tint: u32| {
            if glitch <= 0.0 {
                self.blit(img, src, dst, tint, clip);
                return;
            }
            // A torn picture: bands of rows, some slipped sideways.
            let bands = 8u32;
            let bh = (grid.rows as f32 / bands as f32).ceil() * grid.q;
            for i in 0..bands {
                let y0 = i as f32 * bh;
                if y0 >= h {
                    break;
                }
                let hh = bh.min(h - y0);
                let dx = (damage::tear(glitch, t, self.seed, i) * w / grid.q).round() * grid.q;
                let s = [src[0], src[1] + y0 / px, src[2], hh / px];
                self.blit(img, s, Rect::new(dst.x + dx, dst.y + y0, w, hh), tint, clip);
            }
        };
        blit(0xffffffff);
        damage::flash(&re, damage::light(&cond, t, self.seed), blit);
        damage::draw_overlays(&cond, &re, &grid, dst, t, self.seed);
        dst
    }

    /// One sprite, mirrored when on the right, cut to `clip`.
    fn blit(&self, img: Image, src: [f32; 4], dst: Rect, tint: u32, clip: Option<Rect>) {
        let Some((src, dst)) = clip_sprite(src, dst, clip, self.right) else { return };
        if self.right {
            gfx2d::push();
            gfx2d::translate(dst.x + dst.w, dst.y);
            gfx2d::scale(-1.0, 1.0);
            gfx2d::sprite(img, src, [0.0, 0.0, dst.w, dst.h], tint);
            gfx2d::pop();
        } else {
            gfx2d::sprite(img, src, [dst.x, dst.y, dst.w, dst.h], tint);
        }
    }
}

/// How many sheet pixels make an art pixel, without being told: 48 art
/// pixels across a cell that's a whole multiple of 48 on both sides (an `mb
/// art portrait` pixel-art sheet), else one.
fn guess_density(cell_w: u32, cell_h: u32) -> f32 {
    if cell_w >= PORTRAIT_GRID && cell_h >= PORTRAIT_GRID && cell_w.is_multiple_of(PORTRAIT_GRID) && cell_h.is_multiple_of(PORTRAIT_GRID) {
        (cell_w / PORTRAIT_GRID) as f32
    } else {
        1.0
    }
}

/// A sprite's source and destination cut to `clip` (`None`: nothing left).
/// `mirrored`: drawn flipped across, so the source's left edge is at the
/// destination's right.
fn clip_sprite(src: [f32; 4], dst: Rect, clip: Option<Rect>, mirrored: bool) -> Option<([f32; 4], Rect)> {
    let Some(k) = clip else { return Some((src, dst)) };
    let c = dst.intersect(&k)?;
    let (sx, sy) = (src[2] / dst.w, src[3] / dst.h);
    let left = if mirrored { dst.right() - c.right() } else { c.x - dst.x };
    Some(([src[0] + left * sx, src[1] + (c.y - dst.y) * sy, c.w * sx, c.h * sy], c))
}

// ---------------------------------------------------------------------------
// Dialogue

/// What [`Dialogue::update`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogueEvent {
    /// A cutscene line began (a good moment for a camera cut).
    LineStarted,
    /// The last queued cutscene line was dismissed: back to play.
    Finished,
}

/// A line on screen, for drawing it your own way.
#[derive(Clone, Debug, PartialEq)]
pub struct Showing {
    pub speaker: String,
    pub expression: String,
    pub text: String,
    /// Characters of `text` shown so far.
    pub revealed: usize,
    /// The speaker's mouth now, 0–3.
    pub mouth: u8,
    /// All of the text is out and the voice has finished (a cutscene line
    /// then waits for a tap, or moves on by itself with [`Advance::Auto`]).
    pub complete: bool,
}

#[derive(Clone, Debug)]
struct Said {
    speaker: String,
    line: VoiceLine,
    expression: String,
    queued: f64,
    playing: Option<Playing>,
    /// Tapped to show the whole text.
    shown: bool,
    /// Tapped once all the text was out but the voice wasn't done: moves
    /// on when it is.
    next: bool,
}

impl Said {
    fn new(speaker: &str, line: &VoiceLine, expression: &str) -> Said {
        Said { speaker: speaker.to_string(), line: line.clone(), expression: expression.to_string(), queued: sys::time(), playing: None, shown: false, next: false }
    }

    /// Starts it once its files are in (or have waited long enough).
    fn start(&mut self, now: f64) -> bool {
        if self.playing.is_none() && (self.line.poll() || now - self.queued >= MAX_WAIT) {
            self.playing = Some(self.line.play(now + LEAD));
            return true;
        }
        false
    }

    fn elapsed(&self, now: f64) -> Option<f32> {
        self.playing.map(|p| (now - p.start) as f32)
    }

    fn revealed(&self, now: f64) -> usize {
        match self.elapsed(now) {
            _ if self.shown => usize::MAX,
            Some(t) => self.line.chars_at(t),
            None => 0,
        }
    }

    fn speaking(&self, now: f64) -> bool {
        self.playing.is_some_and(|p| p.voice.is_some() && now < p.start + p.dur as f64)
    }

    /// Not started yet, or its line (voice or subtitle) still running.
    fn busy(&self, now: f64) -> bool {
        self.playing.is_none_or(|p| now < p.start + p.dur as f64)
    }

    /// Game time its line ends, once started.
    fn end(&self) -> Option<f64> {
        self.playing.map(|p| p.start + p.dur as f64)
    }

    fn stop(&self) {
        if let Some(p) = self.playing {
            p.stop();
        }
    }

    fn showing(&self, now: f64) -> Showing {
        let text = self.line.text();
        let n = text.chars().count();
        let revealed = self.revealed(now).min(n);
        Showing {
            speaker: self.speaker.clone(),
            expression: self.expression.clone(),
            mouth: self.elapsed(now).map_or(0, |t| self.line.mouth_at(t)),
            complete: self.playing.is_some() && revealed >= n && !self.speaking(now),
            revealed,
            text,
        }
    }
}

/// How a cutscene moves from one line to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Advance {
    /// Each line waits for a tap once it's all out (the default).
    #[default]
    Tap,
    /// Each line moves on by itself `pause` seconds after its voice (or,
    /// without one, its subtitle's timing) ends; taps still hurry it.
    Auto { pause: f32 },
}

/// Spoken lines on screen, in two modes:
///
/// - **Cutscene** ([`Dialogue::say`]): lines queue up in a box at the bottom
///   with the speaker's portrait and name. The text types out as it's
///   spoken. A line is never cut off by itself: with [`Advance::Tap`] it
///   waits for a tap once its text is out *and* its voice has finished, and
///   with [`Advance::Auto`] it moves on a pause after its voice ends, so the
///   scene lasts as long as its lines. A tap while it types shows the rest
///   at once; a tap once the text is out but the voice isn't moves on when
///   the voice ends; another tap then (or [`Dialogue::skip`]) cuts it short.
///   While [`Dialogue::active`], hold the game and give it the input.
/// - **Bark** ([`Dialogue::bark`], or picked from [`Dialogue::barks`]): a
///   quick line in a small bubble near the top that goes away by itself
///   `bark_hold` after the line ends. It takes no input and never holds the
///   game. Only one voice is heard at a time: a bark never starts while a
///   bark or a cutscene line is being spoken, nor within `bark_cooldown` of
///   the last one, so a line on every hit doesn't turn into chatter.
///
/// Music ducks under both ([`Ducker`]; `dialogue.ducker.db = 0.0` turns it
/// off). Subtitles always show.
#[derive(Clone, Debug)]
pub struct Dialogue {
    pub theme: Theme,
    pub ducker: Ducker,
    /// Seconds between the starts of two barks; [`Dialogue::bark`] drops one
    /// asked for sooner, [`Dialogue::barks`] waits or lets it go stale. Default 5.
    pub bark_cooldown: f32,
    /// Seconds a bark stays after its line ends. Default 0.6.
    pub bark_hold: f32,
    /// Top of the bark bubble (default: 96 below the safe area's top, under the HUD's score).
    pub bark_y: Option<f32>,
    /// How cutscene lines move on ([`Advance::Tap`] by default).
    pub advance: Advance,
    /// The bark pools: `talk.barks.trigger("part_broken")` during play
    /// ([`Barks`]). Empty until lines are added.
    pub barks: Barks,
    cast: Vec<(String, Portrait)>,
    line_drawer: Option<LineDrawer>,
    queue: VecDeque<Said>,
    current: Option<Said>,
    bark: Option<Said>,
    last_bark: Option<f64>,
    opened: f64,
    box_open: bool,
    finished: bool,
    now: f64,
}

impl Dialogue {
    pub fn new(theme: Theme) -> Dialogue {
        Dialogue {
            theme,
            ducker: Ducker::default(),
            bark_cooldown: 5.0,
            bark_hold: 0.6,
            bark_y: None,
            advance: Advance::Tap,
            barks: Barks::new(sys::rand_seed()),
            cast: Vec::new(),
            line_drawer: None,
            queue: VecDeque::new(),
            current: None,
            bark: None,
            last_bark: None,
            opened: 0.0,
            box_open: false,
            finished: false,
            now: sys::time(),
        }
    }

    /// Gives a speaker a portrait (speakers without one just get a name plate).
    pub fn cast(mut self, name: &str, portrait: Portrait) -> Dialogue {
        self.cast.push((name.to_string(), portrait));
        self
    }

    /// Draws the cutscene box your own way: `f` gets each frame's
    /// [`LineBox`] (where the box goes, the speaker, the text typed so far,
    /// the portrait) in place of the theme's panel, portrait, name and text.
    /// Barks keep the default bubble.
    pub fn draw_line(mut self, f: impl Fn(&LineBox) + 'static) -> Dialogue {
        self.line_drawer = Some(LineDrawer(Rc::new(f)));
        self
    }

    /// Cutscene lines move on by themselves `pause` seconds after each voice ends ([`Advance::Auto`]).
    pub fn auto_advance(mut self, pause: f32) -> Dialogue {
        self.advance = Advance::Auto { pause };
        self
    }

    /// Music dips this many dB under voices (0 = off; 3–6 is right for most games).
    pub fn duck(mut self, db: f32) -> Dialogue {
        self.ducker.db = db;
        self
    }

    /// Queues a cutscene line: `speaker` (a name given to [`Dialogue::cast`],
    /// or any name) says `line` with `expression` (a portrait row name; ""
    /// is neutral). A bark being spoken finishes first; one still waiting
    /// for its files is dropped.
    pub fn say(&mut self, speaker: &str, line: &VoiceLine, expression: &str) {
        if self.bark.as_ref().is_some_and(|b| b.playing.is_none()) {
            self.bark = None;
        }
        self.queue.push_back(Said::new(speaker, line, expression));
    }

    /// Shows a bark unless a cutscene is up, a bark is still being spoken,
    /// or the last bark started less than `bark_cooldown` ago. Replaces a
    /// bark bubble whose line is over. True if it shows.
    pub fn bark(&mut self, speaker: &str, line: &VoiceLine, expression: &str) -> bool {
        let now = sys::time();
        if !self.free_to_bark(now) {
            return false;
        }
        self.show_bark(Said::new(speaker, line, expression), now);
        true
    }

    /// Whether a bark may start now: no cutscene, nothing being spoken, the gap since the last bark passed.
    fn free_to_bark(&self, now: f64) -> bool {
        !self.active() && !self.bark.as_ref().is_some_and(|b| b.busy(now)) && self.last_bark.is_none_or(|t| now - t >= self.bark_cooldown as f64)
    }

    fn show_bark(&mut self, said: Said, now: f64) {
        if let Some(b) = self.bark.take() {
            b.stop();
        }
        self.bark = Some(said);
        self.last_bark = Some(now);
    }

    /// Seconds of cutscene left if nobody taps: each line's voice (the rest
    /// of the one showing) plus the [`Advance::Auto`] pause after it. With
    /// [`Advance::Tap`] the player's taps come on top. Lines whose files
    /// haven't loaded count 2 s.
    pub fn remaining(&self) -> f32 {
        let pause = match self.advance {
            Advance::Auto { pause } => pause,
            Advance::Tap => 0.0,
        };
        let now = sys::time();
        let line = |s: &Said| match s.end() {
            Some(end) => (end + pause as f64 - now).max(0.0) as f32,
            None => LEAD as f32 + s.line.dur() + pause,
        };
        self.current.iter().chain(&self.queue).map(line).sum()
    }

    /// Moves past the cutscene line showing (cutting its voice short): the
    /// player asked to skip it.
    pub fn skip(&mut self) {
        if self.current.is_some() {
            self.next_line();
        }
    }

    fn next_line(&mut self) {
        if let Some(cur) = self.current.take() {
            cur.stop();
        }
        if self.queue.is_empty() {
            self.finished = true;
            self.box_open = false;
            self.ducker.set(false);
        }
    }

    /// A cutscene is showing or queued: hold the game.
    pub fn active(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }

    /// Clears everything (cutscene and bark) and lets the music back up.
    pub fn skip_all(&mut self) {
        for s in self.current.take().into_iter().chain(self.queue.drain(..)).chain(self.bark.take()) {
            s.stop();
        }
        self.box_open = false;
        self.ducker.set(false);
    }

    /// Feeds one input event. True if the dialogue took it (any touch, click
    /// or key while a cutscene is up): don't also give it to the game. A press
    /// shows the whole line; once it's all shown, moves on (when the voice
    /// ends, if it's still speaking; a press after that skips it). Barks
    /// take nothing.
    pub fn input(&mut self, e: &Event) -> bool {
        if !self.active() {
            return false;
        }
        let pointer = !matches!(e.kind(), Kind::PadAxis | Kind::MouseWheel | Kind::Text | Kind::Unknown);
        if !(e.is_press() || e.kind() == Kind::KeyDown || e.kind() == Kind::PadButton) {
            return pointer;
        }
        let t = e.time.max(self.now);
        if t < self.opened + TAP_GUARD {
            return true;
        }
        let Some(cur) = &mut self.current else { return true };
        if cur.playing.is_none() {
            return true;
        }
        let s = cur.showing(t);
        if s.complete || cur.next {
            // Done, or a second tap while the voice finishes: the player skips it.
            self.next_line();
        } else if s.revealed < s.text.chars().count() {
            cur.shown = true;
        } else {
            // All the text is out but the voice isn't: move on when it ends.
            cur.next = true;
        }
        true
    }

    /// Advances: starts lines whose files are in, retires barks, ducks the
    /// music. Call every frame, with or without anything showing.
    pub fn update(&mut self, _dt: f32) -> Option<DialogueEvent> {
        let now = sys::time();
        self.now = now;
        let mut event = None;
        // A line moves on by itself once its voice is over: at once if it was
        // tapped, after the pause with auto-advance.
        let due = self.current.as_ref().and_then(|cur| match self.advance {
            _ if cur.next => cur.end(),
            Advance::Auto { pause } => cur.end().map(|end| end + pause as f64),
            Advance::Tap => None,
        });
        if due.is_some_and(|at| now >= at) {
            self.next_line();
        }
        // A bark being spoken finishes before the cutscene opens.
        let bark_busy = self.bark.as_ref().is_some_and(|b| b.busy(now));
        if self.current.is_none()
            && !bark_busy
            && let Some(next) = self.queue.pop_front()
        {
            // Opening the box plays its entrance; moving to the next line doesn't.
            if !self.box_open {
                self.opened = now;
                self.box_open = true;
            }
            // It waits for its files from now, not from when it was queued.
            self.current = Some(Said { queued: now, ..next });
        }
        // Take in the queued lines' files as they arrive, so each starts at once when its turn comes.
        for s in &self.queue {
            s.line.poll();
        }
        if let Some(cur) = &mut self.current
            && cur.start(now)
        {
            event = Some(DialogueEvent::LineStarted);
        }
        if self.finished && !self.active() {
            self.finished = false;
            event = Some(DialogueEvent::Finished);
        }
        let free = self.free_to_bark(now);
        if let Some(b) = self.barks.take(now, free) {
            self.show_bark(Said::new(&b.speaker, &b.line, &b.expression), now);
        }
        if let Some(b) = &mut self.bark {
            b.start(now);
            if b.playing.is_some_and(|p| now >= p.start + p.dur as f64 + self.bark_hold as f64 + BARK_OUT as f64) {
                self.bark = None;
            }
        }
        let speaking = self.current.as_ref().is_some_and(|s| s.speaking(now)) || self.bark.as_ref().is_some_and(|s| s.speaking(now));
        self.ducker.set(speaking && self.ducker.db > 0.0);
        event
    }

    /// The cutscene line on screen.
    pub fn current(&self) -> Option<Showing> {
        self.current.as_ref().map(|s| s.showing(self.now))
    }

    /// The bark on screen.
    pub fn current_bark(&self) -> Option<Showing> {
        self.bark.as_ref().map(|s| s.showing(self.now))
    }

    fn portrait(&self, speaker: &str) -> Option<&Portrait> {
        self.cast.iter().find(|(n, _)| n == speaker).map(|c| &c.1)
    }

    /// Draws the cutscene box and the bark, over the scene.
    pub fn draw(&self, layout: &Layout) {
        if let Some(s) = self.current() {
            match &self.line_drawer {
                Some(f) => (f.0)(&LineBox {
                    rect: self.box_rect(layout),
                    portrait: self.portrait(&s.speaker),
                    theme: &self.theme,
                    t: self.now,
                    line: s,
                }),
                None => self.draw_box(layout, &s),
            }
        }
        if let (Some(s), Some(b)) = (self.current_bark(), &self.bark) {
            self.draw_bark(layout, &s, b);
        }
    }

    /// Where the cutscene box is now (sliding up from below as it opens).
    fn box_rect(&self, layout: &Layout) -> Rect {
        let safe = layout.safe;
        let h = (safe.h * 0.24).clamp(116.0, 168.0);
        let k = Ease::CubicOut.at(((self.now - self.opened) / 0.22).clamp(0.0, 1.0) as f32);
        Rect::new(safe.x + 10.0, safe.bottom() - h - 10.0 + (1.0 - k) * (h + 20.0), safe.w - 20.0, h)
    }

    fn draw_box(&self, layout: &Layout, s: &Showing) {
        let th = &self.theme;
        let r = self.box_rect(layout);
        let h = r.h;
        panel(r, th);
        let portrait = self.portrait(&s.speaker);
        let ps = (h - 16.0).min(132.0);
        let right = portrait.is_some_and(|p| p.right);
        let (tx, tw) = match portrait {
            Some(p) => {
                let pr = if right { Rect::new(r.right() - 8.0 - ps, r.bottom() - 8.0 - ps, ps, ps) } else { Rect::new(r.x + 8.0, r.bottom() - 8.0 - ps, ps, ps) };
                p.draw(pr, &s.expression, s.mouth, self.now);
                if right { (r.x + 18.0, r.w - ps - 42.0) } else { (r.x + ps + 22.0, r.w - ps - 40.0) }
            }
            None => (r.x + 18.0, r.w - 36.0),
        };
        if !s.speaker.is_empty() {
            let name = th.case(&s.speaker);
            let pw = pill_width(&name, 28.0, th.body_font, false);
            let cx = if right { tx + tw - pw / 2.0 } else { tx + pw / 2.0 };
            pill_in(th, &name, cx, r.y, 28.0, th.accent, th.on_accent, None);
        }
        let size = th.fit(th.body_font, 18.0);
        typed(&s.text, s.revealed, th, size, tx, r.y + 24.0, tw, 1.0);
        if s.complete {
            let bob = ((self.now * 6.0).sin() * 2.0) as f32;
            let (x, y) = (r.right() - 22.0, r.bottom() - 18.0 + bob);
            poly(&[x - 7.0, y - 4.0, x + 7.0, y - 4.0, x, y + 5.0], th.accent);
        }
    }

    fn draw_bark(&self, layout: &Layout, s: &Showing, b: &Said) {
        let Some(p) = b.playing else { return };
        let th = &self.theme;
        let safe = layout.safe;
        let end = p.start + p.dur as f64 + self.bark_hold as f64;
        let k_in = Ease::BackOut.at(((self.now - (p.start - LEAD)) / 0.25).clamp(0.0, 1.0) as f32);
        let k_out = 1.0 - ((self.now - end) / BARK_OUT as f64).clamp(0.0, 1.0) as f32;
        let k = k_in * k_out;
        if k <= 0.0 {
            return;
        }
        let portrait = self.portrait(&s.speaker);
        let right = portrait.is_some_and(|p| p.right);
        let ps = if portrait.is_some() { 52.0 } else { 0.0 };
        let bw = (safe.w - 40.0 - ps).min(280.0);
        let size = th.fit(th.body_font, 15.0);
        let name_size = th.fit(th.body_font, 12.0);
        let (_, text_h) = text(&s.text).font(th.body_font).size(size).wrap(bw - 24.0).measure();
        let bh = (text_h + name_size * 1.4 + 20.0).max(ps);
        let y = self.bark_y.unwrap_or(safe.y + 96.0);
        let (px, bx) = if right { (safe.right() - 12.0 - ps, safe.right() - 18.0 - ps - bw) } else { (safe.x + 12.0, safe.x + 18.0 + ps) };
        let bubble = Rect::new(bx, y, bw, bh);
        gfx2d::push();
        let (ox, oy) = (if right { bubble.right() } else { bubble.x }, bubble.cy());
        gfx2d::translate(ox, oy);
        gfx2d::scale(k, k);
        gfx2d::translate(-ox, -oy);
        if let Some(port) = portrait {
            port.draw(Rect::new(px, y, ps, ps), &s.expression, s.mouth, self.now);
        }
        panel(bubble, th);
        text(&th.case(&s.speaker)).font(th.body_font).size(name_size).color(th.accent).draw(bubble.x + 12.0, bubble.y + 9.0);
        typed(&s.text, s.revealed, th, size, bubble.x + 12.0, bubble.y + 9.0 + name_size * 1.4, bw - 24.0, k_out);
        gfx2d::pop();
    }
}

/// A cutscene line to draw your own way ([`Dialogue::draw_line`],
/// [`super::Sequence::draw_line`]), each frame it's on screen.
pub struct LineBox<'a> {
    /// Where the default box would be, sliding up from below as it opens:
    /// across the safe area, 116–168 tall, above the bottom inset (and a
    /// sequence's letterbox bar).
    pub rect: Rect,
    /// The line: speaker, expression, text, characters revealed, mouth,
    /// whether it's all out.
    pub line: Showing,
    /// The speaker's portrait (`portrait.draw(r, &line.expression, line.mouth, t)`).
    pub portrait: Option<&'a Portrait>,
    pub theme: &'a Theme,
    /// Game time now, for the portrait and anything that moves.
    pub t: f64,
}

impl LineBox<'_> {
    /// The text typed out so far.
    pub fn shown(&self) -> &str {
        let n = self.line.text.char_indices().nth(self.line.revealed).map_or(self.line.text.len(), |(b, _)| b);
        &self.line.text[..n]
    }

    /// Draws the text typed out so far in the theme's body font at `size`,
    /// wrapped to `w` as the whole line would be (so words don't jump lines
    /// as they type out), from (x, y).
    pub fn draw_text(&self, x: f32, y: f32, w: f32, size: f32) {
        typed(&self.line.text, self.line.revealed, self.theme, size, x, y, w, 1.0);
    }
}

/// A [`Dialogue::draw_line`] hook.
#[derive(Clone)]
struct LineDrawer(Rc<dyn Fn(&LineBox)>);

impl std::fmt::Debug for LineDrawer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LineDrawer")
    }
}

/// Seconds a bark takes to shrink away.
const BARK_OUT: f32 = 0.2;

/// Draws the first `n` chars of `s` wrapped to `w`, laid out as the whole
/// text would be (so words don't jump lines as they type out).
#[allow(clippy::too_many_arguments)]
fn typed(s: &str, n: usize, th: &Theme, size: f32, x: f32, y: f32, w: f32, alpha: f32) {
    let t = text(s).font(th.body_font).size(size).color(th.text).wrap(w).alpha(alpha);
    let line_h = super::text::metrics(th.body_font).line * size;
    for (i, line) in t.lines().into_iter().enumerate() {
        let start = s[..line.as_ptr() as usize - s.as_ptr() as usize].chars().count();
        let show = n.saturating_sub(start);
        if show == 0 {
            break;
        }
        let end = line.char_indices().nth(show).map_or(line.len(), |(b, _)| b);
        text(&line[..end]).font(th.body_font).size(size).color(t.color).draw(x, y + i as f32 * line_h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::synth;

    const SAMPLE: &str = "mblips 1\nhz 30\ndur 1.84\nmouth 0012332100123321000\ntext Eat my dust, grandma!\nreveal 0 40 80 120 160 200 240 280 320 360 400 440 480 520 560 600 640 680 720 760\n";

    #[test]
    fn a_line_whose_timing_is_missing_shows_its_script_text() {
        assert_eq!(subtitle_text("[excited] No promises!  [laughs] Launching!"), "No promises! Launching!");
        // Timing that failed to load, then the text arrives (or arrived first): the subtitle is the text.
        let line = VoiceLine::new(None, Lips::from_text(""));
        line.files.borrow_mut().lips = None;
        line.files.borrow_mut().path = "assets/vo/kai/c1".into();
        line.files.borrow_mut().fail();
        assert!(line.ready() && line.text().is_empty());
        let line = line.text_if_missing("[angry] I'm not for sale!");
        assert_eq!(line.text(), "I'm not for sale!");
        assert!(line.dur() > 0.5);
        // Before it fails: kept, and used when it does.
        let line = VoiceLine::new(None, Lips::from_text("")).text_if_missing("Clean hit.");
        line.files.borrow_mut().lips = None;
        line.files.borrow_mut().fail();
        assert_eq!(line.text(), "Clean hit.");
    }

    #[test]
    fn a_lips_pack_holds_each_lines_timing() {
        let pack = "mblipspack 1\n@ taunt1\nmblips 1\ndur 1.5\ntext Ha, too slow!\n@ ouch\nmblips 1\ntext Not the paint!\n@ broken\nnot lips\n";
        let m = parse_lips_pack(pack.as_bytes()).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m["taunt1"].text(), "Ha, too slow!");
        assert!((m["taunt1"].dur() - 1.5).abs() < 1e-6);
        assert_eq!(m["ouch"].text(), "Not the paint!");
        assert!(parse_lips_pack(b"mblips 1\ntext hi\n").is_none());
    }

    #[test]
    fn lips_parse_the_cli_format() {
        let l = Lips::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(l.text(), "Eat my dust, grandma!");
        assert_eq!(l.dur(), 1.84);
        assert_eq!(l.hz, 30.0);
        assert_eq!(l.mouth.len(), 19);
        assert_eq!(l.reveal.len(), 20, "one short: the last char comes at the end");
        assert_eq!(l.reveal[3], 0.12);
    }

    #[test]
    fn lips_skip_unknown_lines_and_survive_a_missing_reveal() {
        let l = Lips::parse(b"mblips 1\nspeaker rocko\nhz 30\nfuture 1 2 3\ndur 2\ntext Hi there\n\n").unwrap();
        assert_eq!(l.text(), "Hi there");
        assert!(l.reveal.is_empty());
        // Even over dur: char i at i * dur / n.
        assert_eq!(l.chars_at(0.0), 1);
        assert_eq!(l.chars_at(1.0), 5);
        assert_eq!(l.chars_at(2.0), 8);
        assert_eq!(l.chars_at(-0.1), 0);
        // No mouth: it flaps with the letters, shut on the space.
        assert_eq!(l.mouth_at(0.01), 1, "H");
        assert_eq!(l.mouth_at(0.26), 2, "i, an open vowel");
        assert_eq!(l.mouth_at(0.15), 0, "the flap's closed half");
        // Text with spaces at the start keeps them; CRLF files read the same.
        let crlf = Lips::parse(b"mblips 1\r\ndur 1\r\ntext  two spaces\r\n").unwrap();
        assert_eq!(crlf.text(), " two spaces");
        assert_eq!(crlf.dur(), 1.0);
    }

    #[test]
    fn lips_reject_other_formats() {
        assert!(Lips::parse(b"").is_none());
        assert!(Lips::parse(b"hz 30\ntext hi").is_none());
        assert!(Lips::parse(b"mblips 2\ntext hi").is_none());
        assert!(Lips::parse(&[0xff, 0xfe]).is_none());
        // A bad reveal is ignored rather than half-read; dur falls back to the mouth's length.
        let l = Lips::parse(b"mblips 1\nhz 10\nmouth 0123\ntext abc\nreveal 0 x 20\n").unwrap();
        assert!(l.reveal.is_empty());
        assert!((l.dur() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn mouth_and_chars_follow_the_line() {
        let l = Lips::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(l.mouth_at(0.0), 0);
        assert_eq!(l.mouth_at(2.5 / 30.0), 1);
        assert_eq!(l.mouth_at(4.5 / 30.0), 3);
        assert_eq!(l.mouth_at(-0.01), 0);
        assert_eq!(l.mouth_at(1.84), 0, "after the end");
        assert_eq!(l.mouth_at(1.0), 0, "past the mouth data");
        assert_eq!(l.chars_at(0.0), 1);
        assert_eq!(l.chars_at(0.039), 1);
        assert_eq!(l.chars_at(0.04), 2);
        assert_eq!(l.chars_at(0.1), 3);
        // Chars without a reveal time (the last one here) come at the end.
        assert_eq!(l.chars_at(1.0), 20);
        assert_eq!(l.chars_at(1.84), 21);
    }

    fn line(text: &str, ms_per_char: u32, dur: f32) -> VoiceLine {
        let reveal: Vec<String> = (0..text.chars().count() as u32).map(|i| (i * ms_per_char).to_string()).collect();
        let f = format!("mblips 1\ndur {dur}\ntext {text}\nreveal {}\n", reveal.join(" "));
        VoiceLine::new(None, Lips::parse(f.as_bytes()).unwrap())
    }

    fn run(d: &mut Dialogue, seconds: f32) -> Vec<DialogueEvent> {
        let mut out = Vec::new();
        for _ in 0..(seconds * 60.0).round() as usize {
            sys::test::advance(1.0 / 60.0);
            out.extend(d.update(1.0 / 60.0));
        }
        out
    }

    fn tap(d: &mut Dialogue) -> bool {
        let t = sys::time();
        synth::tap(180.0, 300.0, t).iter().fold(false, |a, e| d.input(e) | a)
    }

    #[test]
    fn the_typewriter_follows_the_reveal_times() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.say("Rocko", &line("Hello", 100, 0.6), "");
        assert!(d.active());
        assert_eq!(d.update(1.0 / 60.0), Some(DialogueEvent::LineStarted));
        // Scheduled LEAD ahead: nothing shows until it's heard.
        assert_eq!(d.current().unwrap().revealed, 0);
        sys::test::set_time(LEAD + 0.25);
        d.update(0.0);
        let s = d.current().unwrap();
        assert_eq!((s.revealed, s.complete), (3, false));
        assert_eq!(s.speaker, "Rocko");
        sys::test::set_time(LEAD + 0.45);
        d.update(0.0);
        assert!(d.current().unwrap().complete);
        assert!(d.active(), "a cutscene waits for a tap");
    }

    #[test]
    fn a_tap_shows_the_line_and_the_next_moves_on() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.say("A", &line("One two three", 100, 1.4), "");
        d.say("B", &VoiceLine::subtitle("Four"), "");
        d.update(0.0);
        // The tap that opened the box doesn't count.
        assert!(tap(&mut d));
        assert_eq!(d.current().unwrap().revealed, 0);
        run(&mut d, 0.4);
        assert!(!d.current().unwrap().complete);
        assert!(tap(&mut d), "the cutscene takes the tap");
        assert!(d.current().unwrap().complete, "first tap: the rest of the text at once");
        assert_eq!(d.current().unwrap().speaker, "A");
        tap(&mut d);
        assert_eq!(run(&mut d, 0.1), [DialogueEvent::LineStarted]);
        assert_eq!(d.current().unwrap().speaker, "B");
        run(&mut d, 0.5);
        assert!(d.current().unwrap().complete, "a subtitle types out at 30 chars a second");
        tap(&mut d);
        assert!(!d.active());
        assert_eq!(run(&mut d, 0.1), [DialogueEvent::Finished]);
        assert!(!tap(&mut d), "nothing up: the game gets its taps");
    }

    #[test]
    fn barks_replace_each_other_and_cool_down() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.bark_cooldown = 1.0;
        assert!(d.bark("Rocko", &VoiceLine::subtitle("Ha!"), ""));
        run(&mut d, 0.5);
        assert!(!d.bark("Rocko", &VoiceLine::subtitle("Again!"), ""), "within the cooldown");
        assert_eq!(d.current_bark().unwrap().text, "Ha!");
        assert!(!d.active(), "barks never hold the game");
        assert!(!tap(&mut d), "and take no input");
        run(&mut d, 0.6);
        assert_eq!(d.current_bark().unwrap().text, "Ha!", "still up (line, then bark_hold)");
        assert!(d.bark("Rocko", &VoiceLine::subtitle("Too slow!"), "smug"));
        run(&mut d, 0.05);
        assert_eq!(d.current_bark().unwrap().text, "Too slow!", "the newer bark replaces the older");
        // It goes after the line, bark_hold and the shrink.
        run(&mut d, 0.6);
        assert!(d.current_bark().is_some());
        run(&mut d, 0.1 + 9.0 / 30.0 + 0.3 + 0.6 + 0.25 - 0.6);
        assert!(d.current_bark().is_none());
        // A cutscene clears barks and blocks new ones.
        d.say("Rocko", &VoiceLine::subtitle("Listen."), "");
        assert!(!d.bark("Rocko", &VoiceLine::subtitle("Ha!"), ""));
    }

    /// `line` with a voice: the typewriter is done long before the audio.
    fn voiced(text: &str, ms_per_char: u32, dur: f32) -> VoiceLine {
        VoiceLine::new(Some(Sound(1)), line(text, ms_per_char, dur).lips().unwrap())
    }

    /// Steps until `f` holds, returning the game time it did (None within `seconds`).
    fn until(d: &mut Dialogue, seconds: f32, mut f: impl FnMut(&Dialogue, Option<DialogueEvent>) -> bool) -> Option<f64> {
        for _ in 0..(seconds * 60.0).round() as usize {
            sys::test::advance(1.0 / 60.0);
            let e = d.update(1.0 / 60.0);
            if f(d, e) {
                return Some(sys::time());
            }
        }
        None
    }

    #[test]
    fn a_line_is_never_cut_off_while_its_voice_plays() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.say("A", &voiced("Hi there", 50, 2.0), "");
        d.say("B", &VoiceLine::subtitle("Next"), "");
        run(&mut d, 0.6);
        let s = d.current().unwrap();
        assert_eq!(s.revealed, 8, "the text is all out…");
        assert!(!s.complete, "…but the voice isn't done, so no arrow yet");
        // The tap that used to cut the last words off now moves on when the voice ends.
        assert!(tap(&mut d));
        assert!(until(&mut d, 1.4, |d, _| d.current().unwrap().speaker != "A").is_none(), "A is heard to the end");
        let t = until(&mut d, 0.2, |_, e| e == Some(DialogueEvent::LineStarted)).unwrap();
        assert!((t - (LEAD + 2.0)).abs() < 0.03, "B starts as A's voice ends: {t}");
        // Without a tap, a finished line waits (tap mode).
        d.skip_all();
        d.say("A", &voiced("Hi there", 50, 1.0), "");
        run(&mut d, 3.0);
        let s = d.current().unwrap();
        assert!(s.speaker == "A" && s.complete, "waits for a tap, arrow up");
        tap(&mut d);
        assert!(!d.active());
    }

    #[test]
    fn auto_advance_moves_on_a_pause_after_the_voice() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default()).auto_advance(0.5);
        d.say("A", &voiced("Hello", 50, 1.0), "");
        d.say("B", &voiced("Bye", 50, 0.5), "");
        d.update(0.0);
        let t = until(&mut d, 3.0, |_, e| e == Some(DialogueEvent::LineStarted)).unwrap();
        assert!((t - (LEAD + 1.0 + 0.5)).abs() < 0.03, "after A's voice and the pause: {t}");
        assert_eq!(d.current().unwrap().speaker, "B");
        let end = until(&mut d, 3.0, |_, e| e == Some(DialogueEvent::Finished)).unwrap();
        assert!((end - t - (LEAD + 0.5 + 0.5)).abs() < 0.03, "{end}");
        assert!(!d.active());
    }

    #[test]
    fn taps_and_skip_cut_a_line_short() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default()).auto_advance(0.5);
        d.say("A", &voiced("One two three four", 100, 3.0), "");
        d.say("B", &voiced("Five", 50, 3.0), "");
        d.say("C", &voiced("Six", 50, 3.0), "");
        run(&mut d, 0.5);
        tap(&mut d);
        assert_eq!(d.current().unwrap().revealed, 18, "first tap: all the text");
        tap(&mut d);
        assert_eq!(d.current().unwrap().speaker, "A", "second: move on when the voice ends");
        tap(&mut d);
        assert_eq!(run(&mut d, 0.05), [DialogueEvent::LineStarted], "third: skipped now");
        assert_eq!(d.current().unwrap().speaker, "B");
        d.skip();
        run(&mut d, 0.05);
        assert_eq!(d.current().unwrap().speaker, "C", "skip() moves on too");
        d.skip();
        assert_eq!(run(&mut d, 0.05), [DialogueEvent::Finished]);
    }

    #[test]
    fn a_cutscene_lasts_as_long_as_its_lines() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default()).auto_advance(0.3);
        for (who, dur) in [("A", 0.5), ("B", 0.8), ("C", 1.0)] {
            d.say(who, &voiced("Words", 40, dur), "");
        }
        let total = 3.0 * (LEAD as f32 + 0.3) + 0.5 + 0.8 + 1.0;
        assert!((d.remaining() - total).abs() < 1e-4, "{}", d.remaining());
        let end = until(&mut d, 6.0, |_, e| e == Some(DialogueEvent::Finished)).unwrap();
        assert!((end - total as f64).abs() < 0.06, "finished at {end}, lines add up to {total}");
        assert_eq!(d.remaining(), 0.0);
    }

    #[test]
    fn a_cutscene_waits_for_a_bark_being_spoken() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        assert!(d.bark("A", &voiced("Argh!", 50, 1.0), ""));
        run(&mut d, 0.3);
        d.say("B", &voiced("Round two.", 50, 1.0), "");
        assert!(d.active());
        let t = until(&mut d, 2.0, |_, e| e == Some(DialogueEvent::LineStarted)).unwrap();
        assert!((t - (LEAD + 1.0)).abs() < 0.03, "the bark is heard out first: {t}");
    }

    #[test]
    fn lines_wait_a_moment_for_their_files() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        // Natively assets never load: the line waits, then runs without them.
        d.say("Rocko", &VoiceLine::load("assets/vo/rocko/taunt1"), "");
        run(&mut d, 0.5);
        assert!(d.current().unwrap().revealed == 0 && !d.current().unwrap().complete);
        assert!(run(&mut d, 0.6).contains(&DialogueEvent::LineStarted));
        run(&mut d, 0.2);
        assert!(d.current().unwrap().complete, "no text to show");
    }

    #[test]
    fn blinks_are_deterministic_and_spaced() {
        let frames: Vec<bool> = (0..60 * 60).map(|i| blinking(i as f64 / 60.0, 7)).collect();
        assert_eq!(frames, (0..60 * 60).map(|i| blinking(i as f64 / 60.0, 7)).collect::<Vec<_>>());
        let starts: Vec<usize> = (1..frames.len()).filter(|&i| frames[i] && !frames[i - 1]).collect();
        assert!(starts.len() >= 12 && starts.len() <= 25, "{}", starts.len());
        for w in starts.windows(2) {
            let gap = (w[1] - w[0]) as f32 / 60.0;
            assert!((2.4..=4.6).contains(&gap), "{gap}");
        }
        assert_ne!(frames, (0..60 * 60).map(|i| blinking(i as f64 / 60.0, 8)).collect::<Vec<_>>(), "other faces blink at other times");
    }

    #[test]
    fn portraits_pick_their_cells() {
        let p = Portrait::from_image(Image(3), 32, 32).expression("smug", 1).expression("angry", 2).blinks(3);
        let t = (0..600).map(|i| i as f64 / 60.0).find(|&t| blinking(t, p.seed)).unwrap();
        let open = (0..600).map(|i| i as f64 / 60.0).find(|&t| !blinking(t, p.seed)).unwrap();
        assert_eq!(p.cell("", 0, open), (0, 0));
        assert_eq!(p.cell("smug", 2, open), (2, 1));
        assert_eq!(p.cell("angry", 7, open), (3, 2), "mouths past 3 are wide");
        assert_eq!(p.cell("nope", 1, open), (1, 0), "unknown expressions are neutral");
        assert_eq!(p.cell("", 0, t), (0, 3), "the neutral face blinks");
        assert_eq!(p.cell("", 2, t), (2, 3), "talking too: the blink row has every mouth");
        assert_eq!(p.cell("angry", 1, t), (1, 2), "other expressions hold their eyes");
        let still = Portrait::from_image(Image(3), 32, 32);
        assert_eq!(still.cell("", 0, t), (0, 0), "no blink row, no blinks");
    }

    #[test]
    fn portraits_scale_by_whole_device_pixels_per_art_pixel() {
        // `mb art portrait`'s pixel art: 48 art pixels across, 4 sheet pixels each.
        let p = Portrait::from_image(Image(3), 192, 192);
        assert_eq!(p.density, 4.0, "a multiple of 48 is taken as 48 art pixels");
        let size = |p: &Portrait, s: f32| p.scale_in(Rect::new(0.0, 0.0, s, s)) * p.cell_w;
        assert_eq!(size(&p, 240.0), 240.0, "5 units an art pixel fills the box (it was 192 at a whole sheet scale)");
        assert_eq!(size(&p, 132.0), 120.0, "2.5 units an art pixel (it was 96)");
        assert_eq!(size(&p, 84.0), 72.0, "1.5 units an art pixel: it fits (it was 96, over the box)");
        assert_eq!(size(&p, 10.0), 24.0, "never under a device pixel an art pixel");
        for s in [60.0, 84.0, 100.0, 132.0, 200.0, 333.0] {
            let art = p.scale_in(Rect::new(0.0, 0.0, s, s)) * p.density * 2.0;
            assert_eq!(art, art.round(), "whole device pixels an art pixel at {s}");
            assert!(size(&p, s) <= s);
        }
        // Told the grid.
        let fine = Portrait::from_image(Image(3), 96, 96).art_pixels(48);
        assert_eq!(fine.density, 2.0);
        assert_eq!(size(&fine, 132.0), 120.0);
        let one_to_one = Portrait::from_image(Image(3), 96, 96).art_pixels(96);
        assert_eq!(size(&one_to_one, 132.0), 96.0, "sheet pixels as art pixels: whole half units each");
        // Anything else: a sheet pixel an art pixel, as before.
        let small = Portrait::from_image(Image(3), 32, 32);
        assert_eq!(small.density, 1.0);
        assert_eq!(size(&small, 84.0), 80.0);
        // Wide boxes: the height decides.
        assert_eq!(p.scale_in(Rect::new(0.0, 0.0, 400.0, 100.0)) * 192.0, 96.0);
    }

    #[test]
    fn a_clipped_portrait_stays_in_its_rect() {
        let src = [0.0, 0.0, 100.0, 100.0];
        let r = Rect::new(0.0, 0.0, 50.0, 50.0);
        assert_eq!(clip_sprite(src, r, None, false), Some((src, r)), "unclipped");
        // Knocked 10 units left: the left fifth is cut.
        let dst = Rect::new(-10.0, 0.0, 50.0, 50.0);
        assert_eq!(clip_sprite(src, dst, Some(r), false), Some(([20.0, 0.0, 80.0, 100.0], Rect::new(0.0, 0.0, 40.0, 50.0))));
        // Mirrored, the cut-off left of the screen is the sheet's right.
        assert_eq!(clip_sprite(src, dst, Some(r), true), Some(([0.0, 0.0, 80.0, 100.0], Rect::new(0.0, 0.0, 40.0, 50.0))));
        // Down and right.
        let dst = Rect::new(5.0, 10.0, 50.0, 50.0);
        assert_eq!(clip_sprite(src, dst, Some(r), false), Some(([0.0, 0.0, 90.0, 80.0], Rect::new(5.0, 10.0, 45.0, 40.0))));
        assert_eq!(clip_sprite(src, Rect::new(60.0, 0.0, 50.0, 50.0), Some(r), false), None, "all outside");
        // A hard hit throws the face past its box; clipped, what's drawn stays in it.
        sys::test::reset();
        sys::test::set_time(20.0);
        let p = Portrait::from_image(Image(3), 192, 192).clipped();
        let boxed = Rect::new(60.0, 100.0, 240.0, 240.0);
        p.hit(1.0);
        let thrown = (1..30).map(|i| p.draw(boxed, "", 0, 20.0 + i as f64 / 60.0)).find(|d| d.x < boxed.x).expect("the knock leaves the box");
        let (_, cut) = clip_sprite([0.0, 0.0, 192.0, 192.0], thrown, Some(boxed), false).unwrap();
        assert!(cut.x >= boxed.x && cut.right() <= boxed.right() && cut.bottom() <= boxed.bottom());
    }

    #[test]
    fn damage_picks_the_hurt_faces_and_clones_share_it() {
        sys::test::reset();
        sys::test::set_time(50.0);
        let p = Portrait::from_image(Image(3), 32, 32).expression("smug", 1).expression("hurt", 2).expression("battered", 3);
        let talk_copy = p.clone();
        assert_eq!(p.cell("", 0, 50.0).1, 0, "healthy: neutral");
        p.set_health(0.45);
        assert_eq!(talk_copy.cell("", 1, 50.0), (1, 2), "damaged: hurt in place of neutral, on the clone too");
        assert_eq!(p.cell("smug", 0, 50.0).1, 1, "a chosen expression stays");
        p.set_health(0.2);
        assert_eq!(p.cell("", 0, 50.0).1, 3, "critical: battered");
        p.set_health(0.9);
        p.hit(0.8);
        assert_eq!(p.cell("smug", 0, 50.05).1, 2, "a hit: the pained face over anything");
        assert_eq!(p.cell("smug", 0, 51.0).1, 1, "then back");
        assert!(talk_copy.reaction(50.01).red > 0.0);
        // A sheet without those rows keeps its faces.
        let plain = Portrait::from_image(Image(3), 32, 32).expression("smug", 1);
        plain.set_health(0.1);
        plain.hit(1.0);
        assert_eq!(plain.cell("smug", 2, 50.05), (2, 1));
    }

    #[test]
    fn the_ducker_dips_and_restores_the_music() {
        let mut k = Ducker::new(6.0);
        assert_eq!(k.gain(), 1.0);
        k.set(false);
        assert_eq!(k.sent, None, "doesn't touch the bus before the first line");
        k.set(true);
        assert!((k.gain() - 0.501).abs() < 1e-3);
        k.set_base(0.5);
        assert!((k.gain() - 0.2506).abs() < 1e-3, "multiplies the game's level");
        k.set(false);
        assert_eq!(k.sent, Some(0.5));
        // The dialogue ducks while a voice speaks; subtitles alone don't.
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        d.say("A", &VoiceLine::subtitle("Hi"), "");
        run(&mut d, 0.2);
        assert!(!d.ducker.ducked());
        d.skip_all();
        d.say("A", &VoiceLine::new(Some(Sound(1)), Lips::from_text("Hello there")), "");
        run(&mut d, 0.2);
        assert!(d.ducker.ducked());
        run(&mut d, 1.0);
        assert!(!d.ducker.ducked(), "back up when the voice ends, even before the tap");
    }
}
