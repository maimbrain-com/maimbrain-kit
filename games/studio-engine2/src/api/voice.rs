//! Voice and dialogue (docs/STUDIO-2.md §5 [voice]): `cast`, `say`,
//! `dialogue{}` with choices, `bark`, `narrate` and `cutscene{}`, over the
//! SDK's `ui::Dialogue` (the box, portraits that talk and blink, typed
//! subtitles, barks, narration, the music ducking under voices) and
//! `ui::Sequence` (letterbox, camera moves and shots, titles, credits,
//! fades, hold to skip).
//!
//! The globals are the prelude's (Lua, so `say()` can hand back a line
//! with a `done` signal); they call the bindings here. Each line is
//! `voices/lines.csv`'s, timed by its lip track (or its length, or its
//! text) from the bundle's text files, never by when its audio decoded,
//! so a replay plays the same. Lines and cutscenes run on game time
//! unscaled by slow motion (`round.time`'s clock): a slow-motion moment
//! doesn't slow the words.
//!
//! What's done comes back to Lua in the libraries' frame (`__engine.update`):
//! each finished line's or dialogue's function, a cutscene's `done`, and
//! `on("cue", name)`'s handlers, in the order they happened.
//!
//! Audio (Decided 62): a line's sound is decoded only once it's queued,
//! with the line after it, so a voiced script holds a line or two of
//! decoded audio, not all of it (`wanted`, which the engine turns into
//! sounds). Letting go of what's past needs the host to free a decoded
//! sound, which the ABI has no call for yet.

use std::collections::VecDeque;
use std::ffi::{c_int, c_void};

use maimbrain::audio::Sound;
use maimbrain::input::{Event, Kind};
use maimbrain::motion::Ease;
use maimbrain::ui::{Advance, Button, ButtonKind, Dialogue, Layout, Portrait, Rect, SeqEvent, Sequence, Shot, Theme, VoiceLine};

use super::*;
use crate::libs::Due;

/// One cast member: the name Lua knows them by, the name on their plate,
/// their portrait, and the face they wear between lines.
pub struct Cast {
    pub key: String,
    pub name: String,
    pub portrait: Option<Portrait>,
    pub expression: String,
    /// 0–1: the engine's, set by `.health` and `:hit`, so the game reads the
    /// same natively, on the server and on the phone; the portrait only
    /// shows it.
    pub health: f32,
    /// A portrait asked for whose image hasn't decoded yet: its image and
    /// whether it sits on the right. It's made once the image is there.
    pub wanted: Option<(usize, bool)>,
}

/// A line queued in the dialogue box, waiting to be done: its function
/// (`say()`'s line's `done`, or a dialogue's `done` on its last line),
/// its answers, and the dialogue it's part of.
struct Pend {
    done: Option<c_int>,
    choices: Vec<String>,
    group: Option<u32>,
}

/// A running `cutscene{}`.
struct Cut {
    seq: Sequence,
    done: Option<c_int>,
}

pub struct Voice {
    pub theme: Theme,
    /// Lines said with `say` and `dialogue{}`, barks and narration.
    pub talk: Dialogue,
    /// Each of the project's lines (`Project::voices`), its sound given
    /// once decoded.
    pub lines: Vec<VoiceLine>,
    /// Lines whose sound is wanted now (queued, or next after one that is).
    pub wanted: Vec<bool>,
    pub casts: Vec<Cast>,
    pends: VecDeque<Pend>,
    seen: u64,
    /// Each dialogue's last answer.
    answers: Vec<Option<u32>>,
    cut: Option<Cut>,
    /// A finished cutscene's bars going away: how much of the screen they
    /// covered, seconds since.
    outro: Option<(f32, f32)>,
    /// The fade a finished cutscene left (how covered, its colour, seconds
    /// since): it stays until the next cutscene, which can fade in from it,
    /// or for good once the round has ended; otherwise it lifts after
    /// [`COVER_HOLD`], so play that goes on after a fade out isn't left black.
    cover: Option<(f32, u32, f32)>,
    cues: Vec<(String, c_int)>,
    /// Barks are loaded from the script at the first `bark`.
    barks_loaded: bool,
    /// The title card's attract mode: nothing heard, no ducking.
    mute: bool,
    /// The answer just picked (a `say` with choices has no dialogue).
    chosen: Option<u32>,
    /// A cutscene's lines, and how many have started (each decodes the next).
    cut_lines: Vec<usize>,
    cut_said: usize,
    /// This frame's step on the unscaled clock.
    pub dt: f32,
    /// The music is dipped under a voice (music_mix's VOICE part).
    ducked: bool,
    /// The camera before the cutscene (2D: x, y, zoom), and a 3D game's.
    three: bool,
}

/// Seconds a finished cutscene's bars take to slide away.
const OUTRO: f32 = 0.6;
/// How long a finished cutscene's fade out holds (s) before it lifts, when
/// no cutscene follows it and the round hasn't ended.
const COVER_HOLD: f32 = 3.0;

/// The ease a cutscene step names (flux's names or easings.net's).
pub fn ease_of(name: &str) -> Option<Ease> {
    let l = name.to_ascii_lowercase();
    if l == "linear" {
        return Some(Ease::Linear);
    }
    let (dir, kind) = ["inout", "in", "out"]
        .iter()
        .find_map(|d| l.strip_prefix(d).filter(|k| !k.is_empty()).map(|k| (*d, k.to_string())))
        .or_else(|| ["inout", "in", "out"].iter().find_map(|d| l.strip_suffix(d).filter(|k| !k.is_empty()).map(|k| (*d, k.to_string()))))?;
    use Ease::*;
    let e = [
        ("quad", [QuadIn, QuadOut, QuadInOut]),
        ("cubic", [CubicIn, CubicOut, CubicInOut]),
        ("quart", [QuartIn, QuartOut, QuartInOut]),
        ("quint", [QuintIn, QuintOut, QuintInOut]),
        ("sine", [SineIn, SineOut, SineInOut]),
        ("expo", [ExpoIn, ExpoOut, ExpoInOut]),
        ("circ", [CircIn, CircOut, CircInOut]),
        ("back", [BackIn, BackOut, BackInOut]),
        ("elastic", [ElasticIn, ElasticOut, ElasticInOut]),
        ("bounce", [BounceIn, BounceOut, BounceInOut]),
    ]
    .into_iter()
    .find(|(k, _)| *k == kind)?
    .1;
    Some(match dir {
        "in" => e[0],
        "out" => e[1],
        _ => e[2],
    })
}

impl Voice {
    pub fn new(project: &crate::project::Project, theme: Theme, seed: u64, mute: bool) -> Voice {
        let vol = if mute { 0.0 } else { 1.0 };
        let mut talk = Dialogue::new(theme);
        talk.barks = maimbrain::ui::Barks::new(seed ^ 0x766f_6963);
        // The dip under voices is one part of the music bus's volume
        // (sound.rs `music_mix`), times the game's own and the Director's.
        talk.ducker.to_bus = false;
        if mute {
            talk.ducker.db = 0.0;
        }
        Voice {
            theme,
            talk,
            // A line with a recording counts as spoken whether or not its sound has decoded (timing never depends on it).
            lines: project.voices.iter().map(|v| { let l = VoiceLine::new(None, v.lips.clone()).volume(vol); if v.ogg { l.expecting_sound() } else { l } }).collect(),
            // Bark lines (those the script's tag column puts in a pool)
            // decode from the start: a bark shows the frame it's asked
            // for, so its sound can't wait to be asked for.
            wanted: project.voices.iter().map(|v| v.ogg && v.tagged).collect(),
            casts: Vec::new(),
            pends: VecDeque::new(),
            seen: 0,
            answers: Vec::new(),
            cut: None,
            outro: None,
            cover: None,
            chosen: None,
            cut_lines: Vec::new(),
            cut_said: 0,
            cues: Vec::new(),
            barks_loaded: false,
            mute,
            dt: 0.0,
            ducked: false,
            three: project.three,
        }
    }

    /// A new identity (`ui.identity`): the box and the cutscene take it.
    pub fn set_theme(&mut self, t: Theme) {
        self.theme = t;
        self.talk.theme = t;
        if let Some(c) = &mut self.cut {
            c.seq.theme = t;
            c.seq.talk.theme = t;
        }
    }

    /// A line's sound, decoded (the engine hands it over).
    pub fn give_sound(&mut self, i: usize, s: Sound) {
        if let Some(l) = self.lines.get(i) {
            l.set_sound(s);
        }
    }

    /// Each cast member's portrait whose image has decoded since it was
    /// asked for (on a phone at once; natively never): made from its rows,
    /// showing the cast member's health, and given to the box and to a
    /// running cutscene. Only drawing depends on it.
    pub fn make_portraits(h: &mut Host) {
        for i in 0..h.voice.casts.len() {
            let Some((im, right)) = h.voice.casts[i].wanted else { continue };
            let Some(image) = h.scene.image(im as u16).and_then(|s| s.image) else { continue };
            let info = &h.project.images[im];
            let mut port = Portrait::from_image(image, info.frame.0, info.frame.1).at(info.at.0, info.at.1);
            for (r, name) in info.portrait.iter().flatten().enumerate() {
                if name == "blink" {
                    port = port.blinks(r as u32);
                } else if r > 0 {
                    port = port.expression(name, r as u32);
                }
            }
            if right {
                port = port.on_right();
            }
            let c = &mut h.voice.casts[i];
            port.set_health(c.health);
            c.wanted = None;
            c.portrait = Some(port.clone());
            let name = c.name.clone();
            h.voice.talk.set_cast(&name, port.clone());
            if let Some(cut) = &mut h.voice.cut {
                cut.seq.talk.set_cast(&name, port);
            }
        }
    }

    /// How covered the screen is by the fade a finished cutscene left (0–1).
    pub fn covered(&self) -> f32 {
        self.cover.map_or(0.0, |(fade, _, t)| fade * (1.0 - Ease::CubicIn.at(((t - COVER_HOLD) / OUTRO).clamp(0.0, 1.0))))
    }

    /// A cutscene is running: the HUD steps aside.
    pub fn cinematic(&self) -> bool {
        self.cut.as_ref().is_some_and(|c| !c.seq.finished())
    }

    /// Something holds the taps: a cutscene, or a line in the box.
    pub fn busy(&self) -> bool {
        self.cinematic() || self.talk.active()
    }

    fn cast_index(&self, key: &str) -> Option<usize> {
        self.casts.iter().position(|c| c.key == key)
    }

    /// The name on a speaker's plate (and their portrait's key in the box).
    fn plate(&self, who: &str) -> String {
        match self.cast_index(who) {
            Some(i) => self.casts[i].name.clone(),
            None => who.to_uppercase(),
        }
    }

    /// The face for a line: what the call says, else the script's emotion
    /// when the portrait has it, else the face the cast member wears now.
    fn face(&self, project: &crate::project::Project, who: &str, line: usize, asked: Option<String>) -> String {
        if let Some(e) = asked {
            return e;
        }
        let emotion = &project.voices[line].emotion;
        let cast = self.cast_index(who).map(|i| &self.casts[i]);
        let has = |e: &str| cast.and_then(|c| c.portrait.as_ref()).is_some_and(|p| p.row(e) != 0 || e == "neutral");
        if !emotion.is_empty() && has(emotion) {
            return emotion.clone();
        }
        cast.map(|c| c.expression.clone()).unwrap_or_default()
    }

    /// Marks line `i` and the next of `list` as wanted (decoded a line ahead).
    fn want(&mut self, i: usize) {
        if let Some(w) = self.wanted.get_mut(i) {
            *w = true;
        }
    }

    /// Feeds one input event (screen units). True if a cutscene or the box
    /// took it: a press shows a line's text, moves on, picks an answer, or
    /// (held) skips a cutscene. Releases go to the game.
    pub fn input(&mut self, e: &Event, layout: &Layout) -> bool {
        if let Some(c) = &mut self.cut
            && !c.seq.finished()
        {
            return c.seq.input(e);
        }
        if !self.talk.active() {
            return false;
        }
        if self.talk.asking()
            && let Some(p) = self.pends.front()
            && !p.choices.is_empty()
        {
            if !e.is_press() {
                return !e.is_release() && !matches!(e.kind(), Kind::KeyUp);
            }
            for (i, r) in choice_rects(layout, &self.theme, &p.choices).into_iter().enumerate() {
                if r.contains(e.x, e.y) {
                    if let Some(g) = p.group {
                        self.answers[g as usize] = Some(i as u32 + 1);
                    }
                    let choice = i as u32 + 1;
                    self.chosen = Some(choice);
                    self.talk.skip();
                    return true;
                }
            }
            return true;
        }
        self.talk.input(e)
    }

    /// The frame (from the libraries' frame, before Lua's callbacks run):
    /// the box, the cutscene and the camera move on, and what finished
    /// goes to `due` (functions to call) and `release` (refs to free).
    pub fn frame(h: &mut Host) {
        let dt = h.voice.dt;
        if h.voice.casts.iter().any(|c| c.wanted.is_some()) {
            Voice::make_portraits(h);
        }
        let v = &mut h.voice;
        // Narration waits for a quiet stretch, and a cutscene isn't one.
        let busy = v.cinematic();
        v.talk.narration.set_busy(busy);
        v.talk.update(dt);
        let mut due: Vec<Due> = Vec::new();
        // Lines dismissed since last frame, in the order they were queued.
        let done = v.talk.lines_done();
        while v.seen < done {
            v.seen += 1;
            let Some(p) = v.pends.pop_front() else { break };
            let choice = match p.group {
                Some(g) => v.answers[g as usize],
                None => v.chosen,
            };
            if !p.choices.is_empty() {
                v.chosen = None;
            }
            if let Some(f) = p.done {
                due.push(Due::Call { f, args: choice.map(|c| vec![c as f64]).unwrap_or_default(), pass_self: false, timer: None });
                h.libs.release.push(f);
            }
        }
        let ended = h.over.is_some() || h.last_lives <= 0.0;
        let v = &mut h.voice;
        let mut cam2: Option<[f32; 3]> = None;
        let mut cam3: Option<Shot> = None;
        if let Some(c) = &mut v.cut {
            for e in c.seq.update(dt) {
                match e {
                    SeqEvent::Cue(name) => {
                        for (n, f) in &v.cues {
                            if *n == name {
                                due.push(Due::Call { f: *f, args: Vec::new(), pass_self: false, timer: None });
                            }
                        }
                    }
                    SeqEvent::LineStarted => {
                        v.cut_said += 1;
                        if let Some(&next) = v.cut_lines.get(v.cut_said + 1)
                            && let Some(w) = v.wanted.get_mut(next)
                        {
                            *w = true;
                        }
                    }
                    SeqEvent::Finished => {
                        if let Some(f) = c.done.take() {
                            due.push(Due::Call { f, args: Vec::new(), pass_self: false, timer: None });
                            h.libs.release.push(f);
                        }
                    }
                    _ => {}
                }
            }
            cam2 = c.seq.view2d();
            cam3 = c.seq.shot_now();
            if c.seq.finished() {
                // Its bars go over a moment, not at once; a fade out stays.
                v.outro = Some((c.seq.bars(), 0.0));
                v.cover = (c.seq.fade() > 0.0).then_some((c.seq.fade(), c.seq.fade_color, 0.0));
                v.cut = None;
            }
        } else if let Some(o) = &mut v.outro {
            o.1 += dt;
            if o.1 >= OUTRO {
                v.outro = None;
            }
        }
        // A fade out holds; once the round has ended it stays for its card.
        if v.cut.is_none()
            && !ended
            && let Some(c) = &mut v.cover
        {
            c.2 += dt;
            if c.2 >= COVER_HOLD + OUTRO {
                v.cover = None;
            }
        }
        if let Some([x, y, z]) = cam2 {
            h.libs.written(crate::libs::Target::Camera, "x");
            h.libs.written(crate::libs::Target::Camera, "y");
            h.camera_touched = true;
            let c = &mut h.scene.camera;
            (c.x, c.y) = (x, y);
            if z > 0.0 {
                c.scale = z;
            }
        }
        #[cfg(three)]
        if let (Some(s), Some(w)) = (cam3, h.three.as_deref_mut()) {
            w.cam.pos = s.pos;
            w.cam.rot = maimbrain::gfx3d::Camera::looking_at(s.pos, s.target).rot;
            w.cam.roll = 0.0;
            w.cam_set = true;
            w.follow = None;
        }
        #[cfg(not(three))]
        let _ = cam3;
        h.libs.pending.extend(due);
        // The music dips under a voice, the box's or a cutscene's, on top of
        // the game's music volume and the Director's (never instead of them).
        let v = &mut h.voice;
        let ducked = !v.mute && (v.talk.ducker.ducked() || v.cut.as_ref().is_some_and(|c| c.seq.talk.ducker.ducked()));
        if ducked != v.ducked {
            v.ducked = ducked;
            let d = &v.talk.ducker;
            let (gain, ramp) = if ducked { (10f32.powf(-d.db.max(0.0) / 20.0), d.attack) } else { (1.0, d.release) };
            super::sound::music_mix::set(super::sound::music_mix::VOICE, gain, ramp);
        }
    }

    /// Draws the cutscene (bars, title, credits, its box, the fade), the
    /// box and barks, and a question's answers, over everything but the cards.
    pub fn draw(&self, layout: &Layout) {
        // A finished cutscene's fade, under the box (a line over the black).
        if let Some((_, color, _)) = self.cover {
            let s = layout.screen;
            maimbrain::gfx2d::rect(s.x, s.y, s.w, s.h, maimbrain::ui::with_alpha(color, self.covered() * ((color & 0xff) as f32 / 255.0)));
        }
        self.talk.draw(layout);
        if self.talk.asking()
            && let Some(p) = self.pends.front()
            && !p.choices.is_empty()
        {
            for (r, c) in choice_rects(layout, &self.theme, &p.choices).into_iter().zip(&p.choices) {
                Button::new(r, c).kind(ButtonKind::Primary).draw(&self.theme);
            }
        }
        if let Some(c) = &self.cut {
            c.seq.draw(layout);
        }
        if let Some((bars, t)) = self.outro {
            let k = 1.0 - Ease::CubicIn.at((t / OUTRO).clamp(0.0, 1.0));
            let s = layout.screen;
            let bar = (bars * k * s.h * 0.5).round();
            if bar > 0.0 {
                maimbrain::gfx2d::rect(s.x, s.y, s.w, bar, 0x000000ff);
                maimbrain::gfx2d::rect(s.x, s.bottom() - bar, s.w, bar, 0x000000ff);
            }
        }
    }
}

/// Where a question's answers go: stacked above the dialogue box (whose
/// height is the SDK's, `Dialogue::box_rect`), the first on top.
pub fn choice_rects(layout: &Layout, th: &Theme, choices: &[String]) -> Vec<Rect> {
    let safe = layout.safe;
    let box_h = (safe.h * 0.24).clamp(116.0, 168.0);
    let (bh, gap) = (44.0, 8.0);
    // As wide as the longest answer, so the scene still shows either side.
    let size = th.fit(th.body_font, 18.0);
    let widest = choices.iter().map(|c| maimbrain::ui::text(c).font(th.body_font).size(size).measure().0).fold(0.0f32, f32::max);
    let w = (widest + 56.0).clamp(170.0, safe.w - 40.0);
    let bottom = safe.bottom() - box_h - 10.0 - 14.0;
    let n = choices.len();
    (0..n).map(|i| Rect::new((safe.cx() - w / 2.0).round(), (bottom - (n - i) as f32 * (bh + gap) + gap).round(), w.round(), bh)).collect()
}

pub unsafe fn install(L: *mut lua_State, ud: *mut c_void) {
    unsafe {
        register(
            L,
            ud,
            "__engine",
            &[
                (c"voiceCast", cast),
                (c"voiceGet", get),
                (c"voiceSet", set),
                (c"voiceHit", hit),
                (c"voiceSay", say),
                (c"voiceDialogue", dialogue),
                (c"voiceBark", bark),
                (c"voiceNarrate", narrate),
                (c"voiceCutscene", cutscene),
                (c"voiceCue", cue),
                (c"voiceBusy", busy),
            ],
        );
    }
}

/// The string at `t[k]` (an array slot), if it is one.
unsafe fn str_i(L: *mut lua_State, t: c_int, k: c_int) -> Option<String> {
    unsafe {
        lua_rawgeti(L, t, k);
        let s = if lua_type(L, -1) == LUA_TSTRING { Some(to_s(L, -1)) } else { None };
        lua_pop(L, 1);
        s
    }
}

unsafe fn num_i(L: *mut lua_State, t: c_int, k: c_int) -> Option<f64> {
    unsafe {
        lua_rawgeti(L, t, k);
        let n = if lua_type(L, -1) == LUA_TNUMBER { Some(lua_tonumber(L, -1)) } else { None };
        lua_pop(L, 1);
        n
    }
}

unsafe fn field_str(L: *mut lua_State, t: c_int, k: &std::ffi::CStr) -> Option<String> {
    unsafe {
        lua_rawgetfield(L, t, k.as_ptr());
        let s = if lua_type(L, -1) == LUA_TSTRING { Some(to_s(L, -1)) } else { None };
        lua_pop(L, 1);
        s
    }
}

/// A function argument kept (a registry ref), or None for nil.
unsafe fn keep(L: *mut lua_State, i: c_int) -> Option<c_int> {
    unsafe {
        if lua_type(L, i) != LUA_TFUNCTION {
            return None;
        }
        lua_pushvalue(L, i);
        let r = lua_ref(L, -1);
        lua_pop(L, 1);
        Some(r)
    }
}

/// The project's line `line` of `who`, or the error that says what's there.
fn line_of(h: &Host, who: &str, line: &str) -> Result<usize, String> {
    if let Some(i) = h.project.voice(who, line) {
        return Ok(i);
    }
    let theirs: Vec<&str> = h.project.voices.iter().filter(|v| v.who == who).map(|v| v.line.as_str()).collect();
    Err(if h.project.voices.is_empty() {
        format!("there's no line {line:?} for {who}: lines live in voices/lines.csv (character,line,text,emotion,tag)")
    } else if theirs.is_empty() {
        format!("voices/lines.csv has no lines for {who:?} (its characters: {})", {
            let mut w: Vec<&str> = h.project.voices.iter().map(|v| v.who.as_str()).collect();
            w.sort();
            w.dedup();
            w.join(", ")
        })
    } else {
        format!("voices/lines.csv has no line {line:?} for {who} (theirs: {})", theirs.join(", "))
    })
}

b!(
    /// `__engine.voiceCast(name, opts)`: `{ portrait, side, name, expression }`.
    cast, |L, h| {
    let Some(key) = st(L, 1).map(str::to_string) else { return bad(L, 1, "cast", "a name") };
    let i = match h.voice.cast_index(&key) {
        Some(i) => i,
        None => {
            h.voice.casts.push(Cast { key: key.clone(), name: key.to_uppercase(), portrait: None, expression: String::new(), health: 1.0, wanted: None });
            h.voice.casts.len() - 1
        }
    };
    if lua_type(L, 2) == LUA_TTABLE {
        if let Some(n) = field_str(L, 2, c"name") {
            h.voice.casts[i].name = n;
        }
        if let Some(e) = field_str(L, 2, c"expression") {
            h.voice.casts[i].expression = e;
        }
        let side = field_str(L, 2, c"side");
        if let Some(s) = &side
            && s != "left"
            && s != "right"
        {
            return err(L, &format!("cast {key:?}: side is \"left\" or \"right\", got {s:?}"));
        }
        if let Some(p) = field_str(L, 2, c"portrait") {
            let found = h.project.image(&format!("portraits/{p}")).or_else(|| h.project.image(&p));
            let Some(im) = found else {
                return err(L, &format!("cast {key:?}: there's no portrait {p:?} (images/portraits/{p}.png, made with generate kind \"portrait\")"));
            };
            let info = &h.project.images[im];
            if info.portrait.is_none() {
                return err(L, &format!("cast {key:?}: images/{}.png isn't a portrait: its .toml says its rows, portrait = [\"neutral\", …, \"blink\"]", info.name));
            }
            let c = &mut h.voice.casts[i];
            c.portrait = None;
            c.wanted = Some((im, side.as_deref() == Some("right")));
        } else if let Some(s) = &side
            && let Some(w) = &mut h.voice.casts[i].wanted
        {
            w.1 = s == "right";
        } else if let Some(s) = &side
            && let Some(p) = h.voice.casts[i].portrait.take()
        {
            let mut p = p;
            p.right = s == "right";
            let name = h.voice.casts[i].name.clone();
            h.voice.talk.set_cast(&name, p.clone());
            h.voice.casts[i].portrait = Some(p);
        }
        Voice::make_portraits(h);
        // A new name on the plate carries the portrait with it.
        let c = &h.voice.casts[i];
        if let Some(p) = &c.portrait {
            let (name, p) = (c.name.clone(), p.clone());
            h.voice.talk.set_cast(&name, p);
        }
    }
    0
});

b!(
    /// `__engine.voiceGet(name, key)`: a cast member's expression, health or name.
    get, |L, h| {
    let (Some(key), Some(k)) = (st(L, 1), st(L, 2)) else { return 0 };
    let Some(i) = h.voice.cast_index(key) else { return err(L, &format!("{key:?} isn't cast: cast({key:?}, {{ portrait = … }}) first")) };
    let c = &h.voice.casts[i];
    match k {
        "expression" => rets(L, &c.expression),
        "health" => ret(L, c.health as f64),
        "name" => rets(L, &c.name),
        _ => {
            lua_pushnil(L);
            1
        }
    }
});

b!(
    /// `__engine.voiceSet(name, key, value)`.
    set, |L, h| {
    let (Some(key), Some(k)) = (st(L, 1).map(str::to_string), st(L, 2).map(str::to_string)) else { return 0 };
    let Some(i) = h.voice.cast_index(&key) else { return err(L, &format!("{key:?} isn't cast: cast({key:?}, {{ portrait = … }}) first")) };
    match k.as_str() {
        "expression" => h.voice.casts[i].expression = st(L, 3).unwrap_or("").to_string(),
        "health" => match num(L, 3) {
            Some(v) => {
                let c = &mut h.voice.casts[i];
                c.health = (v as f32).clamp(0.0, 1.0);
                if let Some(p) = &c.portrait {
                    p.set_health(c.health);
                }
            }
            None => return err(L, "a cast member's health is a number from 0 to 1"),
        },
        "name" => return err(L, "a cast member's name is set with cast(name, { name = \"…\" })"),
        _ => return err(L, &format!("a cast member has expression and health, and :hit(amount); not {k}")),
    }
    0
});

b!(
    /// `__engine.voiceHit(name, amount)`: a hit on the portrait; health goes down by it.
    hit, |L, h| {
    let Some(key) = st(L, 1) else { return 0 };
    let Some(i) = h.voice.cast_index(key) else { return err(L, &format!("{key:?} isn't cast: cast({key:?}, {{ portrait = … }}) first")) };
    let amount = opt(L, 2, 0.25).clamp(0.0, 1.0) as f32;
    let c = &mut h.voice.casts[i];
    c.health = (c.health - amount).clamp(0.0, 1.0);
    // The knock is only drawn: a portrait not made yet shows the health when it is.
    if let Some(p) = &c.portrait {
        p.hit(amount.max(0.2));
        p.set_health(c.health);
    }
    0
});

/// Queues one line in the box; `choices` makes it a question.
fn queue(h: &mut Host, who: &str, line: usize, face: Option<String>, choices: Vec<String>, done: Option<c_int>, group: Option<u32>) {
    let face = h.voice.face(&h.project, who, line, face);
    let plate = h.voice.plate(who);
    let v = &mut h.voice;
    v.want(line);
    let l = v.lines[line].clone();
    if choices.is_empty() {
        v.talk.say(&plate, &l, &face);
    } else {
        v.talk.say_held(&plate, &l, &face);
    }
    v.pends.push_back(Pend { done, choices, group });
}

b!(
    /// `__engine.voiceSay(who, line, expression, fn)`.
    say, |L, h| {
    let (Some(who), Some(line)) = (st(L, 1).map(str::to_string), st(L, 2).map(str::to_string)) else { return bad(L, 1, "say", "who and the line's id: say(\"gruk\", \"halt\")") };
    let i = match line_of(h, &who, &line) {
        Ok(i) => i,
        Err(e) => return err(L, &format!("say: {e}")),
    };
    let face = st(L, 3).map(str::to_string);
    let f = keep(L, 4);
    queue(h, &who, i, face, Vec::new(), f, None);
    0
});

b!(
    /// `__engine.voiceDialogue(t)`: `{ { who, line, expression, choices = {…} }, …, done = fn, advance = "auto" }`.
    dialogue, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "dialogue", "a table of lines: dialogue{ { \"gruk\", \"halt\" }, done = function(choice) end }");
    }
    let n = lua_objlen(L, 1);
    if n == 0 {
        return err(L, "dialogue{} has no lines: dialogue{ { \"gruk\", \"halt\", \"smug\" }, … }");
    }
    // Read every line first, so a mistake queues nothing.
    let mut lines = Vec::new();
    for k in 1..=n {
        lua_rawgeti(L, 1, k);
        if lua_type(L, -1) != LUA_TTABLE {
            lua_pop(L, 1);
            return err(L, &format!("dialogue line {k} is {{ who, line, expression }}"));
        }
        let t = lua_gettop(L);
        let (Some(who), Some(id)) = (str_i(L, t, 1), str_i(L, t, 2)) else {
            lua_pop(L, 1);
            return err(L, &format!("dialogue line {k} is {{ who, line, expression }}: {{ \"gruk\", \"halt\", \"smug\" }}"));
        };
        let face = str_i(L, t, 3);
        let mut choices = Vec::new();
        lua_rawgetfield(L, t, c"choices".as_ptr());
        if lua_type(L, -1) == LUA_TTABLE {
            let c = lua_gettop(L);
            for j in 1..=lua_objlen(L, c) {
                match str_i(L, c, j) {
                    Some(s) => choices.push(s),
                    None => {
                        lua_pop(L, 2);
                        return err(L, &format!("dialogue line {k}: choices are strings"));
                    }
                }
            }
        }
        lua_pop(L, 2);
        if choices.len() > 4 {
            return err(L, &format!("dialogue line {k} has {} choices; at most 4 fit above the box", choices.len()));
        }
        let i = match line_of(h, &who, &id) {
            Ok(i) => i,
            Err(e) => return err(L, &format!("dialogue line {k}: {e}")),
        };
        lines.push((who, i, face, choices));
    }
    match field_str(L, 1, c"advance").as_deref() {
        Some("auto") => h.voice.talk.advance = Advance::Auto { pause: 0.6 },
        Some("tap") | None => h.voice.talk.advance = Advance::Tap,
        Some(o) => return err(L, &format!("dialogue's advance is \"tap\" or \"auto\", got {o:?}")),
    }
    lua_rawgetfield(L, 1, c"done".as_ptr());
    let done = keep(L, -1);
    lua_pop(L, 1);
    let g = h.voice.answers.len() as u32;
    h.voice.answers.push(None);
    let last = lines.len() - 1;
    for (k, (who, i, face, choices)) in lines.into_iter().enumerate() {
        queue(h, &who, i, face, choices, if k == last { done } else { None }, Some(g));
    }
    // The next line is decoded with the first.
    0
});

b!(
    /// `__engine.voiceBark(tag)`: true if a line will show.
    bark, |L, h| {
    let Some(tag) = st(L, 1).map(str::to_string) else { return bad(L, 1, "bark", "a tag from voices/lines.csv's tag column") };
    if !h.voice.barks_loaded {
        h.voice.barks_loaded = true;
        for (i, l) in h.project.voices.iter().enumerate() {
            let face = h.voice.face(&h.project, &l.who, i, None);
            let plate = h.voice.plate(&l.who);
            let line = h.voice.lines[i].clone();
            h.voice.talk.barks.add(&l.tag, &plate, &line, &face);
        }
    }
    if h.voice.talk.barks.lines(&tag) == 0 {
        let mut tags: Vec<&str> = h.project.voices.iter().map(|v| v.tag.as_str()).filter(|t| !t.is_empty()).collect();
        tags.sort();
        tags.dedup();
        return err(L, &format!("bark: no line in voices/lines.csv is tagged {tag:?} (tags: {})", tags.join(", ")));
    }
    // A cutscene has the floor: no barks over it.
    if h.voice.cinematic() {
        return retb(L, false);
    }
    for (i, l) in h.project.voices.iter().enumerate() {
        if l.tag == tag {
            h.voice.wanted[i] = true;
        }
    }
    retb(L, h.voice.talk.barks.trigger(&tag))
});

b!(
    /// `__engine.voiceNarrate(who, line, expression)`.
    narrate, |L, h| {
    let (Some(who), Some(line)) = (st(L, 1).map(str::to_string), st(L, 2).map(str::to_string)) else { return bad(L, 1, "narrate", "who and the line's id") };
    let i = match line_of(h, &who, &line) {
        Ok(i) => i,
        Err(e) => return err(L, &format!("narrate: {e}")),
    };
    let face = h.voice.face(&h.project, &who, i, st(L, 3).map(str::to_string));
    let plate = h.voice.plate(&who);
    h.voice.want(i);
    let l = h.voice.lines[i].clone();
    h.voice.talk.narration.tell(&plate, &l, &face);
    0
});

b!(
    /// `__engine.voiceCue(name, fn)`: on("cue", name, fn).
    cue, |L, h| {
    let Some(name) = st(L, 1).map(str::to_string) else { return bad(L, 1, "on(\"cue\", …)", "the cue's name") };
    let Some(f) = keep(L, 2) else { return bad(L, 2, "on(\"cue\", name, …)", "a function") };
    h.voice.cues.push((name, f));
    0
});

b!(
    /// `__engine.voiceBusy()`: a cutscene or a line holds the taps.
    busy, |L, h| retb(L, h.voice.busy()));

b!(
    /// `__engine.voiceCutscene(t)`: its steps in order, `done = fn`.
    cutscene, |L, h| {
    if lua_type(L, 1) != LUA_TTABLE {
        return bad(L, 1, "cutscene", "a table of steps: cutscene{ { \"letterbox\", 0.12 }, { \"say\", \"gruk\", \"halt\" }, done = fn }");
    }
    if h.voice.cinematic() {
        return err(L, "a cutscene is already playing: start the next from its done");
    }
    let th = h.voice.theme;
    let c = h.scene.camera;
    let mut seq = Sequence::new(th).from_view2d(c.x, c.y, c.scale);
    seq.talk.barks = maimbrain::ui::Barks::new(h.seed ^ 0x6375_7473);
    seq.talk.ducker.to_bus = false;
    if h.voice.mute {
        seq.talk.ducker.db = 0.0;
    }
    for cm in &h.voice.casts {
        if let Some(p) = &cm.portrait {
            seq = seq.cast(&cm.name, p.clone());
        }
    }
    let fov = {
        #[cfg(three)]
        {
            h.three.as_deref().map_or(1.0, |w| w.cam.fov)
        }
        #[cfg(not(three))]
        {
            1.0
        }
    };
    let n = lua_objlen(L, 1);
    let mut wanted = Vec::new();
    for k in 1..=n {
        lua_rawgeti(L, 1, k);
        if lua_type(L, -1) == LUA_TSTRING && to_s(L, -1) == "meanwhile" {
            // The step before doesn't hold the scene: the next starts with it.
            lua_pop(L, 1);
            seq = seq.meanwhile();
            continue;
        }
        if lua_type(L, -1) != LUA_TTABLE {
            lua_pop(L, 1);
            return err(L, &format!("cutscene step {k} is a table ({{ \"wait\", 0.5 }}), or \"meanwhile\""));
        }
        let t = lua_gettop(L);
        let what = str_i(L, t, 1).unwrap_or_default();
        let nums = |from: c_int, count: c_int| -> Option<Vec<f32>> { (from..from + count).map(|j| num_i(L, t, j).map(|v| v as f32)).collect() };
        let step_err = |m: &str| format!("cutscene step {k} ({what}): {m}");
        let r: Result<Sequence, String> = match what.as_str() {
            "letterbox" => match num_i(L, t, 2) {
                Some(a) => Ok(seq.letterbox(a as f32)),
                None => Err(step_err("{ \"letterbox\", 0.12 }: the bars' share of the screen's height (0 takes them away)")),
            },
            "wait" => match num_i(L, t, 2) {
                Some(s) if s >= 0.0 => Ok(seq.wait(s as f32)),
                _ => Err(step_err("{ \"wait\", secs }")),
            },
            "camera" => match nums(2, 4) {
                Some(v) => {
                    let ease = str_i(L, t, 6).map(|e| ease_of(&e).ok_or(e));
                    match ease {
                        Some(Err(e)) => Err(step_err(&format!("there's no ease {e:?} (linear, inOutSine, outBack…)"))),
                        e => Ok(seq.pan(v[0], v[1], v[2], v[3].max(0.0), e.and_then(Result::ok).unwrap_or(Ease::SineInOut))),
                    }
                }
                None => Err(step_err("{ \"camera\", x, y, zoom, secs, ease }")),
            },
            "shot" => match nums(2, 7) {
                Some(v) if h.voice.three => Ok(seq.move_to(
                    Shot::new(maimbrain::gfx3d::vec3(v[0], v[1], v[2]), maimbrain::gfx3d::vec3(v[3], v[4], v[5])).fov(fov),
                    v[6].max(0.0),
                    str_i(L, t, 9).and_then(|e| ease_of(&e)).unwrap_or(Ease::SineInOut),
                )),
                Some(_) => Err(step_err("a shot is 3D's camera; a 2D game's is { \"camera\", x, y, zoom, secs, ease }")),
                None => Err(step_err("{ \"shot\", x, y, z, tx, ty, tz, secs }")),
            },
            "say" => match (str_i(L, t, 2), str_i(L, t, 3)) {
                (Some(who), Some(id)) => match line_of(h, &who, &id) {
                    Ok(i) => {
                        let face = h.voice.face(&h.project, &who, i, str_i(L, t, 4));
                        let plate = h.voice.plate(&who);
                        wanted.push(i);
                        Ok(seq.line(&plate, &h.voice.lines[i], &face))
                    }
                    Err(e) => Err(step_err(&e)),
                },
                _ => Err(step_err("{ \"say\", who, line, expression }")),
            },
            "cue" => match str_i(L, t, 2) {
                Some(c) => Ok(seq.cue(&c)),
                None => Err(step_err("{ \"cue\", name }: on(\"cue\", name, fn) hears it")),
            },
            "title" => match (str_i(L, t, 2), num_i(L, t, 4).or(num_i(L, t, 3))) {
                (Some(a), secs) => Ok(seq.title(&a, &str_i(L, t, 3).unwrap_or_default(), secs.unwrap_or(2.5) as f32)),
                _ => Err(step_err("{ \"title\", heading, subtitle, secs }")),
            },
            "fade" => match (str_i(L, t, 2), num_i(L, t, 3)) {
                (Some(d), s) if d == "in" || d == "out" => {
                    let s = s.unwrap_or(1.0) as f32;
                    Ok(if d == "in" { seq.fade_in(s) } else { seq.fade_out(s) })
                }
                _ => Err(step_err("{ \"fade\", \"in\" or \"out\", secs }")),
            },
            "credits" => {
                lua_rawgeti(L, t, 2);
                let mut list = Vec::new();
                if lua_type(L, -1) == LUA_TTABLE {
                    let c = lua_gettop(L);
                    for j in 1..=lua_objlen(L, c) {
                        lua_rawgeti(L, c, j);
                        let p = lua_gettop(L);
                        if let (Some(a), Some(b)) = (str_i(L, p, 1), str_i(L, p, 2)) {
                            list.push((a, b));
                        }
                        lua_pop(L, 1);
                    }
                }
                lua_pop(L, 1);
                let secs = num_i(L, t, 3).unwrap_or(1.6 * list.len() as f64 + 3.0) as f32;
                if list.is_empty() {
                    Err(step_err("{ \"credits\", { { \"THE TROLL\", \"GRUK\" }, … }, secs }"))
                } else {
                    let refs: Vec<(&str, &str)> = list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                    Ok(seq.credits(&refs, secs))
                }
            }
            "meanwhile" => Ok(seq.meanwhile()),
            "" => Err(step_err("each step starts with what it is: letterbox, camera, shot, say, wait, cue, title, fade, credits")),
            o => Err(format!("cutscene step {k}: there's no step {o:?} (letterbox, camera, shot, say, wait, cue, title, fade, credits, meanwhile)")),
        };
        lua_pop(L, 1);
        match r {
            Ok(s) => seq = s,
            Err(e) => return err(L, &e),
        }
    }
    lua_rawgetfield(L, 1, c"done".as_ptr());
    let done = keep(L, -1);
    lua_pop(L, 1);
    // The first two lines decode now; the rest a line ahead as it plays.
    for i in wanted.iter().take(2) {
        h.voice.want(*i);
    }
    h.voice.cut_lines = wanted;
    h.voice.cut_said = 0;
    // Barks asked for before it began don't come out in the middle of it.
    h.voice.talk.barks.clear();
    // It takes over from the last one's fade (a fade in starts covered).
    h.voice.cover = None;
    h.voice.cut = Some(Cut { seq, done });
    0
});

impl Drop for Voice {
    /// A round that ends mid-line gives the music back.
    fn drop(&mut self) {
        if self.ducked {
            super::sound::music_mix::set(super::sound::music_mix::VOICE, 1.0, 0.3);
        }
    }
}
