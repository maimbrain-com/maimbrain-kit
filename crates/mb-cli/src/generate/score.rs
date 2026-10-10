//! Music in sections for `mb music --variations N`, `--sections a,b,bridge`
//! and `--zones "lantern town: …; bamboo grove: …"`: one piece in sections,
//! measured on one key, one tempo and one bar grid. Its music graph
//! (graph.rs) is built on that grid; with `--loops`, it's also cut into
//! loops of the same length, so a game can switch between them on any bar
//! (the SDK's `music::Director`).
//!
//! How they're kept together. Separate requests don't hold a key or a
//! tempo: Lyria ignores the key it's asked for, each take lands a few BPM
//! from the one asked for, and the seed doesn't carry either across
//! prompts. So the sections are asked for in **one** request: one piece,
//! one tempo and key throughout, with a timestamped line per section
//! (Lyria 3.5 takes `[0:00 - 0:20] …` structure in the prompt and makes a
//! couple of minutes). One performance keeps its tempo and key far better
//! than separate takes. Then every section is measured, and corrected only
//! within small limits:
//!
//! - **Tempo**: the whole piece's bar length comes from its onsets (how
//!   long the music takes to repeat near the requested loop length, to a
//!   sample or so). Each section's own period is measured the same way; one
//!   off by more than 0.3 % is time-stretched (WSOLA, pitch kept) by up to
//!   5 %. Every loop is then cut to exactly the same number of samples.
//! - **Key**: measured per section as mix_check.py does, against the key of
//!   all of them (or `--key`). A section that clashes is pitch-shifted when
//!   a shift of one semitone fixes it; further off, it's reported.
//! - **Grid**: one for the whole piece (grid.rs): its beats and its 1 from
//!   the whole take, never a section's own guess. Every loop starts on one of
//!   its bar lines (10 ms of pre-roll before it), so every file puts the same
//!   beat of the bar at its start and a switch on a bar line keeps the bar.
//!   `--downbeat N` moves the piece's 1 by N beats when it's audibly off.
//! - **Loop**: of the windows of whole bars on that grid inside a section,
//!   the one whose end runs into its start most like the music did (grid.rs
//!   `Match`); a 4-bar phrase written twice when the section has no 8-bar
//!   loop that joins (Lyria 3.5's sections are short and keep developing).
//!   The bar that followed the loop is cross-faded into its start (equal
//!   power, `--loop-xfade` bars), so the file repeats seamlessly at its exact
//!   length. Layers are split after that: they share the join.
//! - **A loop of its own** (`--fix-loops`, sound.rs): a section that still
//!   joins badly can be asked for again as one Lyria 3 Clip, fitted to the
//!   piece within the same limits ([`fit_clip`]) and used if it joins better.
//!
//! A section that can't be corrected (missing, tempo or key too far off)
//! is a mismatch: mb asks again (`--retries`, default 1) and keeps the take
//! with the fewest; anything left is in the log and the sidecar.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::audio::{self, Key};
use super::grid::{self, Feats, Grid, Match};

/// One section of a piece: a file name part and what it should sound like.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    #[serde(default)]
    pub desc: String,
    /// A further variation of this zone (`--zones … --variations K`): no stinger of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation_of: Option<String>,
}

/// Most a section's tempo may be corrected (as a fraction).
pub const MAX_TEMPO_FIX: f64 = 0.05;
/// Tempo deviations under this are left alone (a 16 s loop drifts < 50 ms).
pub const TEMPO_TOLERANCE: f64 = 0.003;
/// Most a section's pitch may be corrected (semitones).
pub const MAX_PITCH_FIX: i32 = 1;
/// The tempo sections are planned at when none is given (and asked for).
pub const DEFAULT_BPM: f32 = 120.0;
/// How far from the asked-for tempo the piece's tempo is looked for.
pub const PRIOR_DEV: f64 = 0.12;
/// Each loop starts this long before its downbeat (seconds): the first hit's
/// attack whole, and the file's ends not on a transient.
pub const PREROLL: f32 = 0.01;

/// A file-name-safe slug: "Lantern Town" → "lantern_town".
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// `--variations N`: v1…vN, the same groove with a new melody and arrangement each.
pub fn variations(n: u32) -> Vec<Section> {
    (1..=n).map(|i| Section { name: format!("v{i}"), desc: String::new(), variation_of: None }).collect()
}

/// `--sections a,b,a,bridge`: the form, in order (a repeated name is one loop).
pub fn parse_sections(list: &str) -> Result<Vec<Section>, String> {
    let v: Vec<Section> = list.split(',').map(slug).filter(|s| !s.is_empty()).map(|name| Section { name, desc: String::new(), variation_of: None }).collect();
    if v.is_empty() {
        return Err("--sections: name them, e.g. --sections a,b,bridge".into());
    }
    Ok(v)
}

/// `--zones "lantern town: warm lanterns, a slow pulse; bamboo grove: …"`, or
/// `--zones @zones.txt` (one `name: description` per line).
pub fn parse_zones(spec: &str) -> Result<Vec<Section>, String> {
    let text = match spec.strip_prefix('@') {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("--zones {path}: {e}"))?,
        None => spec.to_string(),
    };
    let mut v = Vec::new();
    for part in text.split([';', '\n']).map(str::trim).filter(|p| !p.is_empty() && !p.starts_with('#')) {
        let (name, desc) = part.split_once(':').ok_or(format!("--zones: {part:?} needs a name and a description (\"lantern town: warm lanterns …\")"))?;
        let name = slug(name);
        if name.is_empty() {
            return Err(format!("--zones: {part:?} has no name"));
        }
        if v.iter().any(|s: &Section| s.name == name) {
            return Err(format!("--zones: {name} is named twice"));
        }
        v.push(Section { name, desc: desc.trim().to_string(), variation_of: None });
    }
    if v.len() < 2 {
        return Err("--zones: give at least two zones, separated by ';'".into());
    }
    Ok(v)
}

/// The distinct sections, first appearance first.
pub fn unique(form: &[Section]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for s in form {
        if !out.iter().any(|o| o.name == s.name) {
            out.push(s.clone());
        }
    }
    out
}

/// Where each section of the form sits in the requested piece.
#[derive(Clone, Debug)]
pub struct Plan {
    pub bpm: f32,
    pub bars: u32,
    /// (start, end) seconds per entry of the form.
    pub windows: Vec<(f32, f32)>,
    pub total: f32,
    /// Seconds per section.
    pub per: f32,
}

/// Each section long enough to hold a loop of `bars` bars, a bar to settle
/// in and room to judge the seam: whole bars, at least 16 s.
/// `per` keeps the section length a take was asked for (its recipe), so a
/// later --reprocess finds the sections where they were.
pub fn plan(form: &[Section], bars: u32, bpm: f32, per: Option<f32>) -> Plan {
    let bar = 4.0 * 60.0 / bpm;
    let per = per.unwrap_or_else(|| {
        let p = ((bars as f32 + 3.0) * bar).max(18.0);
        (p / bar).ceil() * bar
    });
    let windows = (0..form.len()).map(|i| (i as f32 * per, (i + 1) as f32 * per)).collect();
    Plan { bpm, bars, windows, total: per * form.len() as f32, per }
}

fn stamp(s: f32) -> String {
    let s = s.round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn describe(s: &Section, i: usize, form: &[Section]) -> String {
    if !s.desc.is_empty() {
        return format!("{}: {}", title(&s.name), s.desc.trim_end_matches('.'));
    }
    let n = &s.name;
    if let Some(k) = n.strip_prefix('v').and_then(|k| k.parse::<usize>().ok()) {
        return if k == 1 {
            "Variation 1: the main theme".into()
        } else {
            format!("Variation {k}: the same groove and instruments, a new melody and a different arrangement")
        };
    }
    let again = form[..i].iter().any(|o| o.name == s.name);
    match n.as_str() {
        "intro" => "Intro: the groove at full tempo with fewer instruments".into(),
        "bridge" => "Bridge: new chords, thinner and lifting back toward the theme".into(),
        "outro" => "Outro: the theme once more, still at full tempo".into(),
        _ if again => format!("Section {}: the {} theme again", n.to_uppercase(), n.to_uppercase()),
        _ => format!("Section {}: {}", n.to_uppercase(), if i == 0 { "the main theme" } else { "a contrasting melody over the same groove" }),
    }
}

fn title(s: &str) -> String {
    s.split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect::<Vec<_>>().join(" ")
}

/// The prompt's middle for a sectioned piece.
pub fn prompt_lines(form: &[Section], p: &Plan) -> String {
    let mut s = format!(
        "One continuous instrumental piece at {:.0} BPM in 4/4, in one key and at one tempo from start to finish: the sections change the melody, the instruments and the energy, never the tempo or the key. No intro, no breakdown, no silence, no fade-out and no ending; each section moves straight into the next. About {:.0} seconds long, in these sections:",
        p.bpm, p.total
    );
    for (i, (sec, w)) in form.iter().zip(&p.windows).enumerate() {
        s.push_str(&format!("\n[{} - {}] {}.", stamp(w.0), stamp(w.1), describe(sec, i, form)));
    }
    s
}

/// The prompt for a section's own loop (`--fix-loops`: one Lyria 3 Clip):
/// the piece's subject and that section, at the piece's tempo and in its key,
/// one groove made to loop.
pub fn clip_prompt(subject: &str, s: &Section, form: &[Section], bpm: f32, key: Key) -> String {
    let i = form.iter().position(|o| o.name == s.name).unwrap_or(0);
    let what = match s.variation_of.as_ref().and_then(|v| form.iter().position(|o| &o.name == v)) {
        Some(j) => format!("{}; {}", describe(&form[j], j, form), s.desc.trim_end_matches('.')),
        None => describe(s, i, form),
    };
    format!(
        "{}. {what}. Instrumental only, no vocals. {bpm:.0} BPM in 4/4, in {}. A steady tempo and one consistent groove all the way through: no intro, no breakdown, no fade-out and no ending, so it can loop seamlessly as game music. An original composition, not imitating any existing song or artist.",
        subject.trim().trim_end_matches('.'),
        key.name()
    )
}

/// One cut loop and what was measured and done to it.
pub struct Loop {
    pub name: String,
    pub samples: Vec<f32>,
    pub facts: Value,
    /// How well it joins end to start (`facts["loop_match"]`).
    pub score: f32,
}

pub struct Cut {
    pub loops: Vec<Loop>,
    pub bpm: f32,
    pub bars: u32,
    pub key: Key,
    pub len: usize,
    /// The crossfade baked into each loop, samples.
    pub xfade: usize,
    /// Facts about the whole piece (its grid, how the sections agree on it), the same in every sidecar.
    pub piece: Value,
    /// The piece's grid in the take (samples), and where each entry of the form is in it.
    pub grid: Grid,
    pub windows: Vec<(usize, usize)>,
    /// Sections that couldn't be made to fit, in words.
    pub mismatches: Vec<String>,
    pub notes: Vec<String>,
}

impl Cut {
    /// Sections whose loop joins worse than `below` (1: every one).
    pub fn weak(&self, below: f32) -> Vec<&Loop> {
        self.loops.iter().filter(|l| l.score < below || below >= 1.0).collect()
    }

    /// What a loop from elsewhere must fit to join these.
    pub fn fit(&self) -> Fit {
        Fit { bpm: self.bpm, bars: self.bars, len: self.len, key: self.key, xfade: self.xfade }
    }
}

/// How a piece is cut (`--loop-xfade`, `--downbeat`).
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// The crossfade baked into each loop, in bars.
    pub xfade_bars: f32,
    /// Beats to move the piece's downbeat by, when it's audibly off.
    pub shift_beats: f32,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { xfade_bars: 1.0, shift_beats: 0.0 }
    }
}

/// Below this loop match a section's join can be heard; `--fix-loops` asks
/// for a loop of its own (one Lyria 3 Clip) and keeps whichever joins better.
pub const FIX_BELOW: f32 = 0.85;
/// A phrase shorter than the loop (written twice to fill it) is taken only
/// when the whole-length loop is weak and the phrase this much better.
const SHORT_PHRASE_GAIN: f32 = 0.05;

/// The phrase lengths to try for a loop of `bars`: it, then halves down to 4 bars.
fn phrases(bars: u32) -> Vec<u32> {
    let mut v = vec![bars];
    let mut p = bars;
    while p.is_multiple_of(2) && p / 2 >= 4 {
        p /= 2;
        v.push(p);
    }
    v
}

/// A loop found on a grid.
struct Choice {
    /// The bar line it starts on (samples in its source).
    bar_line: f64,
    phrase: u32,
    m: Match,
    /// The best whole-length loop's match, when a shorter phrase was taken.
    full: Option<Match>,
    candidates: usize,
}

/// The best loop of `bars` bars, or of a shorter whole phrase of 4 bars or
/// more, that starts on a bar line of `g` and lies (with its crossfade's
/// tail) within [lo, hi] of the source: the one whose end runs into its
/// start most like the music did (see grid.rs).
fn best_loop(feats: &Feats, g: Grid, lo: f64, hi: f64, bars: u32, xfade: usize, pre: usize) -> Option<Choice> {
    let mut best: Vec<Option<(f64, Match)>> = Vec::new();
    let mut candidates = 0;
    let before_weight = (0.5 * (1.0 - xfade as f64 / g.bar)).clamp(0.0, 0.5) as f32;
    for &phrase in &phrases(bars) {
        let span = phrase as f64 * g.bar;
        let mut top: Option<(f64, Match)> = None;
        for s in g.bars_in(lo.max(pre as f64), hi - span - xfade as f64) {
            let (i, e) = (feats.index(s), feats.index(s + span));
            // With a bar's crossfade baked in, what's heard is bar P turning into
            // bar 0: their likeness is the join. With less, bar P−1 runs into bar 0
            // more directly, and how bar −1 led into it counts too.
            let Some(after) = feats.compare(e, i, 4) else {
                continue;
            };
            let before = feats.compare(e - 4, i - 4, 4);
            let m = Match::blend(after, before, before_weight);
            candidates += 1;
            if top.is_none_or(|t| m.score > t.1.score) {
                top = Some((s, m));
            }
        }
        best.push(top);
    }
    let full = best[0];
    // The longest phrase unless it's weak and a shorter one is clearly better.
    let mut pick = full.map(|f| (bars, f));
    for (k, &phrase) in phrases(bars).iter().enumerate().skip(1) {
        if let Some(c) = best[k]
            && pick.is_none_or(|p| p.1.1.score < FIX_BELOW && c.1.score >= p.1.1.score + SHORT_PHRASE_GAIN)
        {
            pick = Some((phrase, c));
        }
    }
    let (phrase, (bar_line, m)) = pick?;
    Some(Choice { bar_line, phrase, m, full: if phrase < bars { full.map(|f| f.1) } else { None }, candidates })
}

/// The loop for a choice: from its bar line (less the pre-roll), its phrase
/// with the crossfade baked in, written out to `len` samples.
fn bake(src: &[f32], c: &Choice, bars: u32, len: usize, xfade: usize, pre: usize) -> (Vec<f32>, usize) {
    let plen = (len as f64 * c.phrase as f64 / bars as f64).round() as usize;
    let start = (c.bar_line.round() as usize).saturating_sub(pre);
    let (lp, xf) = audio::bake_loop(src, start, plen, xfade);
    (if plen == len { lp } else { audio::repeat_to(&lp, len) }, xf)
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn match_facts(facts: &mut Value, c: &Choice, xf: usize, rate: u32) {
    facts["loop_match"] = json!(round2(c.m.score));
    facts["loop_match_parts"] = json!({ "harmony": round2(c.m.harmony), "level_db": (c.m.level_db * 10.0).round() / 10.0, "rhythm": round2(c.m.rhythm) });
    facts["phrase_bars"] = json!(c.phrase);
    if let Some(f) = c.full {
        facts["loop_match_whole"] = json!(round2(f.score));
    }
    facts["loop_candidates"] = json!(c.candidates);
    facts["xfade_seconds"] = json!(((xf as f64 / rate as f64) * 1000.0).round() / 1000.0);
}

/// The sub-beat offset (ms) of the beats in `x[lo..hi]` from grid `g`'s beats (its own beats, for the record).
fn beat_offset_ms(x: &[f32], rate: u32, g: Grid, lo: usize, hi: usize) -> (f32, f32) {
    let beat = g.beat();
    let (ph, conf) = grid::beat_phase(&x[lo..hi], rate, beat);
    let d = (lo as f64 + ph - g.downbeat).rem_euclid(beat);
    let d = if d > beat / 2.0 { d - beat } else { d };
    ((d / rate as f64 * 1000.0).round() as f32, conf)
}

/// Cuts every distinct section of `x` (the whole piece, mono at `rate`) to a
/// loop of `p.bars` bars, all the same length and key, on the piece's one grid.
pub fn cut(x: &[f32], rate: u32, form: &[Section], p: &Plan, given_key: Option<Key>, o: &Opts) -> Result<Cut, String> {
    let secs = x.len() as f32 / rate as f32;
    let mut notes = Vec::new();
    // The piece's tempo near the one asked for (Lyria lands within ~5 % of
    // it; a general tempo detector confuses 125 BPM with 4/3 of it when taiko
    // play triplets): the beat period within ±12 % of the asked one (no other
    // multiple of the beat is that close), then refined over two bars and
    // the whole loop, each within ±1 %.
    let asked = p.bpm;
    let lag = |bpm: f32, beats: f64| (beats * 60.0 / bpm as f64 * rate as f64).round() as usize;
    let refine = |bpm: f32, beats: f64| match audio::local_period(x, rate, lag(bpm, beats), 0.01) {
        Some((l, conf)) if conf > 0.15 => (beats * 60.0 * rate as f64 / l as f64) as f32,
        _ => bpm,
    };
    let mut bpm = match audio::local_period(x, rate, lag(asked, 1.0), PRIOR_DEV) {
        Some((l, conf)) if conf > 0.15 => refine((60.0 * rate as f64 / l as f64) as f32, 8.0),
        _ => match audio::detect_bpm(x, rate) {
            Some((mut b, _)) => {
                while b < asked * 0.7 {
                    b *= 2.0;
                }
                while b > asked * 1.4 {
                    b /= 2.0;
                }
                b
            }
            None => asked,
        },
    };
    if (bpm / asked - 1.0).abs() > 0.03 {
        notes.push(format!("asked for {asked:.0} BPM, it plays at {bpm:.1}"));
    }
    // Then the exact loop length: the lag near it where the whole piece repeats best.
    let len0 = (p.bars as f64 * 4.0 * 60.0 / bpm as f64 * rate as f64).round() as usize;
    let len = match audio::local_period(x, rate, len0, 0.01) {
        Some((l, conf)) if conf > 0.15 => l,
        _ => len0,
    };
    bpm = (p.bars as f64 * 4.0 * 60.0 * rate as f64 / len as f64) as f32;
    let bar = len as f64 / p.bars as f64;
    let beat = bar / 4.0;
    let xfade = ((o.xfade_bars.max(0.0) as f64 * bar).round() as usize).min(len);
    // The returned piece may be a little longer or shorter than asked: stretch the plan to it.
    let scale = if (0.85..1.2).contains(&(secs / p.total)) { secs / p.total } else { 1.0 };
    if scale != 1.0 && (scale - 1.0).abs() > 0.03 {
        notes.push(format!("the piece is {secs:.0} s (asked for {:.0}); section windows scaled to it", p.total));
    }
    // Each entry of the form's window of the take, and each distinct section's.
    let windows: Vec<(usize, usize)> =
        p.windows.iter().map(|&(a, b)| ((a * scale * rate as f32) as usize, ((b * scale * rate as f32) as usize).min(x.len()))).collect();
    let mut sections: Vec<(Section, usize, usize)> = Vec::new();
    for (i, s) in form.iter().enumerate() {
        if sections.iter().any(|o| o.0.name == s.name) {
            continue;
        }
        let (a, b) = p.windows[i];
        sections.push((s.clone(), (a * scale * rate as f32) as usize, ((b * scale * rate as f32) as usize).min(x.len())));
    }
    // The piece's one grid: its beats from the onsets of the whole take, its
    // 1 from the harmonic changes and low onsets of the whole take (grid.rs).
    let (first, beat_conf) = grid::beat_phase(x, rate, beat);
    let ranges: Vec<(f64, f64)> = sections.iter().map(|s| (s.1 as f64, s.2 as f64)).collect();
    let db = grid::downbeat(x, rate, first, beat, &ranges);
    let g = Grid { downbeat: (first + (db.class as f64 + o.shift_beats as f64) * beat).rem_euclid(bar), bar };
    let voted = db.per_range.iter().filter(|v| v.is_some()).count();
    let agree = db.per_range.iter().filter(|v| **v == Some(db.class)).count();
    let piece = json!({
        "downbeat_in_take_seconds": (g.downbeat / rate as f64 * 10000.0).round() / 10000.0,
        "bar_seconds": (bar / rate as f64 * 100000.0).round() / 100000.0,
        "downbeat_from": "the whole take: harmonic change on the 1, low onsets",
        "downbeat_shift_beats": o.shift_beats,
        "sections_agreeing": format!("{agree} of {voted}"),
        "beat_confidence": round2(beat_conf),
    });
    if voted > 0 && agree * 2 <= voted {
        notes.push(format!(
            "the piece's downbeat: only {agree} of {voted} sections agree on which beat is the 1; listen (--audition) and move it with --downbeat N if it's off"
        ));
    }
    let mut mismatches = Vec::new();
    let pre = (PREROLL * rate as f32) as usize;
    let skip_ctx = (1.5 * bar) as usize;
    // (section, its source region, the region's grid, the window in it, facts, chroma)
    struct Prepared {
        s: Section,
        src: Vec<f32>,
        a0: usize,
        stretch: f64,
        grid: Grid,
        win: (f64, f64),
        facts: Value,
    }
    let mut prepared: Vec<Prepared> = Vec::new();
    for (s, a, b) in &sections {
        let (a, b) = (*a, *b);
        let mut facts = json!({ "section": s.name, "window_seconds": [a as f32 / rate as f32, b as f32 / rate as f32] });
        if b <= a || b - a < len + rate as usize * 3 / 2 {
            mismatches.push(format!("{}: missing or too short in the take ({:.1} s of it)", s.name, b.saturating_sub(a) as f32 / rate as f32));
            facts["mismatch"] = json!("missing");
            continue;
        }
        // The window with a bar and a half either side (for the bar before a loop and its crossfade's tail).
        let (a0, b0) = (a.saturating_sub(skip_ctx), (b + skip_ctx).min(x.len()));
        let mut src = x[a0..b0].to_vec();
        let mut f = 1.0f64;
        // Its own two-bar period (short enough to leave most of the window to compare).
        let span = 2 * len / p.bars as usize;
        if let Some((per, conf)) = audio::local_period(&x[a..b], rate, span, MAX_TEMPO_FIX * 1.6) {
            let r = per as f64 / span as f64;
            facts["tempo_bpm"] = json!(((bpm as f64 / r) * 100.0).round() / 100.0);
            facts["tempo_confidence"] = json!((conf * 100.0).round() / 100.0);
            if conf > 0.2 && (r - 1.0).abs() > TEMPO_TOLERANCE {
                if (r - 1.0).abs() <= MAX_TEMPO_FIX {
                    f = 1.0 / r;
                    src = audio::stretch(&src, rate, f);
                    // The tempo change it took (negative: it was fast and was slowed).
                    facts["tempo_fix_pct"] = json!(((r - 1.0) * 1000.0).round() / 10.0);
                } else {
                    mismatches.push(format!("{}: plays at {:.1} BPM against {bpm:.1} ({:+.1} %, more than ±{:.0} % to correct)", s.name, bpm as f64 / r, (1.0 / r - 1.0) * 100.0, MAX_TEMPO_FIX * 100.0));
                    facts["mismatch"] = json!("tempo");
                }
            }
        }
        // The piece's grid in this region. Time-stretched, the section's bars are
        // the piece's from the first bar line in its window on (the performance is
        // continuous there), at the piece's tempo, moved by less than a quarter
        // beat onto the section's own beats (its tempo wasn't the piece's, so the
        // piece's beats drift through it). Which beat is the 1 stays the piece's.
        let win = ((a - a0) as f64 * f, (b - a0) as f64 * f);
        let grid = if f == 1.0 {
            Grid { downbeat: (g.downbeat - a0 as f64).rem_euclid(bar), bar }
        } else {
            let anchor = g.bars_in(a as f64, a as f64 + bar)[0];
            let mut gr = Grid { downbeat: ((anchor - a0 as f64) * f).rem_euclid(bar), bar };
            let (off, conf) = beat_offset_ms(&src, rate, gr, win.0 as usize, (win.1 as usize).min(src.len()));
            let off = off as f64 / 1000.0 * rate as f64;
            if conf > 0.1 && off.abs() < beat / 4.0 {
                gr.downbeat = (gr.downbeat + off).rem_euclid(bar);
                facts["grid_nudge_ms"] = json!((off / rate as f64 * 1000.0).round());
            }
            gr
        };
        prepared.push(Prepared { s: s.clone(), src, a0, stretch: f, grid, win, facts });
    }
    // The key of all of them (each weighted alike), or the one given.
    let chroma_of = |p: &Prepared| audio::chroma(&p.src[p.win.0 as usize..(p.win.1 as usize).min(p.src.len())], rate);
    let chromas: Vec<[f64; 12]> = prepared.iter().map(chroma_of).collect();
    let mut sum = [0.0f64; 12];
    for ch in &chromas {
        let t = ch.iter().sum::<f64>().max(1e-12);
        for k in 0..12 {
            sum[k] += ch[k] / t;
        }
    }
    // The key most sections fit (as mix_check judges a layer), then the best
    // fit overall: a section or two that wandered off mustn't pull it.
    let measured = audio::keys_of(&sum)
        .into_iter()
        .map(|k| {
            let fits = chromas.iter().filter(|ch| ch.iter().sum::<f64>() > 0.0 && !audio::key_fit(ch, k).3).count();
            let share: f32 = chromas.iter().map(|ch| audio::in_scale(ch, k, 0)).sum();
            (k, fits, share)
        })
        .fold(None::<(Key, usize, f32)>, |b, c| match b {
            Some(b) if (b.1, b.2) >= (c.1, c.2 - 1e-6) => Some(b),
            _ => Some(c),
        })
        .map(|b| b.0)
        .unwrap_or(audio::keys_of(&sum)[0]);
    let key = given_key.unwrap_or(measured);
    let mut loops = Vec::new();
    for (mut pr, ch) in prepared.into_iter().zip(chromas) {
        let name = pr.s.name.clone();
        let own = audio::keys_of(&ch)[0];
        let (here, best, best_share, clash) = audio::key_fit(&ch, key);
        pr.facts["key"] = json!(own.name());
        pr.facts["in_key"] = json!(round2(here));
        if clash {
            if best != 0 && best.abs() <= MAX_PITCH_FIX && best_share >= 0.7 {
                pr.src = audio::pitch_shift(&pr.src, rate, best as f32);
                pr.facts["pitch_fix_semitones"] = json!(best);
                pr.facts["in_key"] = json!(round2(best_share));
            } else {
                mismatches.push(format!(
                    "{name}: {:.0} % of it in {} ({} fits best{}); more than {MAX_PITCH_FIX} semitone to correct",
                    here * 100.0,
                    key.name(),
                    if best == 0 { "as it is".to_string() } else { format!("{best:+} semitones") },
                    if best == 0 { ", but it's a different scale" } else { "" }
                ));
                pr.facts["mismatch"] = json!("key");
            }
        }
        let feats = Feats::new(&pr.src, rate, pr.grid.downbeat, beat);
        let lo = pr.win.0 - bar / 2.0;
        let hi = (pr.win.1 + bar / 2.0).min(pr.src.len() as f64);
        let Some(c) = best_loop(&feats, pr.grid, lo, hi, p.bars, xfade, pre) else {
            mismatches.push(format!("{name}: no loop of {} bars fits in its section", p.bars));
            continue;
        };
        let (lp, xf) = bake(&pr.src, &c, p.bars, len, xfade, pre);
        let take_pos = pr.a0 as f64 + c.bar_line / pr.stretch;
        pr.facts["loop_start_seconds"] = json!((take_pos / rate as f64 * 1000.0).round() / 1000.0);
        pr.facts["loop_bar"] = json!(g.bar_number(take_pos));
        match_facts(&mut pr.facts, &c, xf, rate);
        let (off, conf) = beat_offset_ms(&pr.src, rate, pr.grid, pr.win.0 as usize, (pr.win.1 as usize).min(pr.src.len()));
        pr.facts["beat_offset_ms"] = json!(off);
        pr.facts["beat_confidence"] = json!(round2(conf));
        if c.phrase < p.bars {
            notes.push(format!(
                "{name}: a {}-bar phrase joins far better than any {}-bar loop ({:.2} against {:.2}); it's written {}× to fill the loop",
                c.phrase,
                p.bars,
                c.m.score,
                c.full.map_or(0.0, |f| f.score),
                p.bars / c.phrase
            ));
        }
        loops.push(Loop { name, samples: lp, facts: pr.facts, score: c.m.score });
    }
    if loops.is_empty() {
        return Err("no section could be cut into a loop".into());
    }
    Ok(Cut { loops, bpm, bars: p.bars, key, len, xfade, piece, grid: g, windows, mismatches, notes })
}

/// What a loop from elsewhere must fit to join a piece's loops.
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    pub bpm: f32,
    pub bars: u32,
    pub len: usize,
    pub key: Key,
    pub xfade: usize,
}

/// A section's own loop from a separate take (one Lyria 3 Clip, `--fix-loops`):
/// its tempo and key measured and corrected to the piece's within the same
/// limits as a section's, then cut like one on its own grid (its own beats
/// and 1: it's another performance). The prompt asked for the piece's tempo
/// and key.
pub fn fit_clip(name: &str, x: &[f32], rate: u32, f: &Fit) -> Result<Loop, String> {
    let lag = |bpm: f32, beats: f64| (beats * 60.0 / bpm as f64 * rate as f64).round() as usize;
    let mut own = match audio::local_period(x, rate, lag(f.bpm, 1.0), PRIOR_DEV) {
        Some((l, conf)) if conf > 0.15 => (60.0 * rate as f64 / l as f64) as f32,
        _ => {
            return Err(format!("{name}: no steady beat near {:.0} BPM in the clip", f.bpm));
        }
    };
    if let Some((l, conf)) = audio::local_period(x, rate, lag(own, 8.0), 0.01)
        && conf > 0.15
    {
        own = (8.0 * 60.0 * rate as f64 / l as f64) as f32;
    }
    let mut facts = json!({ "section": name, "source": "clip", "clip_bpm": (own * 100.0).round() / 100.0 });
    let r = own as f64 / f.bpm as f64;
    if (r - 1.0).abs() > MAX_TEMPO_FIX {
        return Err(format!("{name}: the clip plays at {own:.1} BPM, more than ±{:.0} % from the piece's {:.1}", MAX_TEMPO_FIX * 100.0, f.bpm));
    }
    let mut y = x.to_vec();
    if (r - 1.0).abs() > TEMPO_TOLERANCE {
        y = audio::stretch(&y, rate, r);
        facts["tempo_fix_pct"] = json!(((1.0 / r - 1.0) * 1000.0).round() / 10.0);
    }
    let ch = audio::chroma(&y, rate);
    let (here, best, best_share, clash) = audio::key_fit(&ch, f.key);
    facts["key"] = json!(audio::keys_of(&ch)[0].name());
    facts["in_key"] = json!(round2(here));
    if clash {
        if best != 0 && best.abs() <= MAX_PITCH_FIX && best_share >= 0.7 {
            y = audio::pitch_shift(&y, rate, best as f32);
            facts["pitch_fix_semitones"] = json!(best);
            facts["in_key"] = json!(round2(best_share));
        } else {
            return Err(format!(
                "{name}: the clip came back in {} ({:.0} % in {}; {}), more than {MAX_PITCH_FIX} semitone to correct",
                audio::keys_of(&ch)[0].name(),
                here * 100.0,
                f.key.name(),
                if best == 0 { "a different scale".to_string() } else { format!("{best:+} semitones would fit") }
            ));
        }
    }
    let bar = f.len as f64 / f.bars as f64;
    let (first, conf) = grid::beat_phase(&y, rate, bar / 4.0);
    let db = grid::downbeat(&y, rate, first, bar / 4.0, &[]);
    let g = Grid { downbeat: first + db.class as f64 * bar / 4.0, bar };
    let feats = Feats::new(&y, rate, g.downbeat, bar / 4.0);
    let pre = (PREROLL * rate as f32) as usize;
    // Past a fade-in, and short of a fade-out.
    let (lo, hi) = (0.5 * rate as f64, y.len() as f64 - 1.0 * rate as f64);
    let c = best_loop(&feats, g, lo, hi, f.bars, f.xfade, pre).ok_or(format!("{name}: the clip is too short for a loop of {} bars", f.bars))?;
    let (lp, xf) = bake(&y, &c, f.bars, f.len, f.xfade, pre);
    facts["loop_start_seconds"] = json!((c.bar_line / rate as f64 * 1000.0).round() / 1000.0);
    facts["beat_confidence"] = json!(round2(conf));
    facts["downbeat_from"] = json!("the clip's own grid (another performance)");
    match_facts(&mut facts, &c, xf, rate);
    Ok(Loop { name: name.to_string(), samples: lp, facts, score: c.m.score })
}

/// The pitch classes a loop opens with: the strongest in-key notes of its first bar, strongest first.
pub fn opening_chord(lp: &[f32], rate: u32, bar: usize, key: Key) -> Vec<u8> {
    let ch = audio::chroma(&lp[..bar.min(lp.len())], rate);
    let sc = key.scale();
    let mut pcs: Vec<u8> = (0..12u8).filter(|pc| sc.iter().any(|d| (key.root + d) % 12 == *pc)).collect();
    pcs.sort_by(|a, b| ch[*b as usize].total_cmp(&ch[*a as usize]));
    if ch.iter().sum::<f64>() < 1e-9 {
        return vec![key.root, (key.root + if key.minor { 3 } else { 4 }) % 12, (key.root + 7) % 12];
    }
    pcs.truncate(3);
    pcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forms() {
        let z = parse_zones("Lantern Town: warm lanterns, a slow pulse; bamboo grove: hollow wood").unwrap();
        assert_eq!(z[0], Section { name: "lantern_town".into(), desc: "warm lanterns, a slow pulse".into(), variation_of: None });
        assert_eq!(z[1].name, "bamboo_grove");
        assert!(parse_zones("one: only").is_err() && parse_zones("a: x; a: y").is_err() && parse_zones("no colon; b: y").is_err());
        let s = parse_sections("a,b,a,Bridge").unwrap();
        assert_eq!(unique(&s).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["a", "b", "bridge"]);
        assert_eq!(variations(3)[2].name, "v3");
        assert_eq!(slug("  Maple  Gorge! "), "maple_gorge");
    }

    /// The test song at `bpm`, `semis` higher (made slower, then sped up: the same tempo, higher).
    fn section(bpm: f32, semis: f32, secs: f32, rate: u32) -> Vec<f32> {
        let r = 2f32.powf(semis / 12.0);
        let x = audio::tests::song(bpm / r, secs * r + 0.1, rate);
        let mut y = audio::resample_ratio(&x, 1.0 / r as f64);
        y.truncate((secs * rate as f32) as usize);
        y
    }

    #[test]
    fn sections_are_measured_corrected_and_cut_alike() {
        let rate = 22050;
        let form = parse_sections("a,b,c,d").unwrap();
        let p = plan(&form, 4, 120.0, Some(14.0));
        // a as asked; b 3 % fast; c a semitone sharp; d three semitones off.
        let mut x = section(120.0, 0.0, 14.0, rate);
        x.extend(section(123.6, 0.0, 14.0, rate));
        x.extend(section(120.0, 1.0, 14.0, rate));
        x.extend(section(120.0, 3.0, 14.0, rate));
        let c = cut(&x, rate, &form, &p, None, &Opts::default()).unwrap();
        assert!((c.bpm - 120.0).abs() < 0.3, "{}", c.bpm);
        assert_eq!(c.loops.len(), 4);
        for l in &c.loops {
            assert_eq!(l.samples.len(), c.len, "{} is as long as the others", l.name);
        }
        let f = |n: usize, k: &str| c.loops[n].facts[k].clone();
        assert!(f(0, "tempo_fix_pct").is_null() && f(0, "pitch_fix_semitones").is_null());
        let fix = f(1, "tempo_fix_pct").as_f64().unwrap();
        assert!((fix + 2.9).abs() < 0.5, "b slowed by ~2.9 %: {fix}");
        assert_eq!(f(2, "pitch_fix_semitones"), -1, "c brought down a semitone");
        assert_eq!(f(3, "mismatch"), "key", "d is too far off to correct");
        assert_eq!(c.mismatches.len(), 1, "{:?}", c.mismatches);
        assert!(c.mismatches[0].starts_with("d: "));
        // Each loop starts PREROLL before a downbeat (the kick).
        for l in &c.loops[..3] {
            let pre = (PREROLL * rate as f32) as usize;
            let w = (0.03 * rate as f32) as usize;
            let e = |s: usize| -> f32 { l.samples[s..s + w].iter().map(|v| v * v).sum() };
            assert!(e(pre) > 3.0 * e(pre + c.len / 32), "{}: no hit on its first downbeat", l.name);
        }
    }

    #[test]
    fn a_clip_is_brought_to_the_piece_and_cut_on_its_own_grid() {
        let rate = 22050;
        let key = audio::detect_key(&section(120.0, 0.0, 20.0, rate), rate);
        let len = (4.0 * 2.0 * rate as f64).round() as usize;
        let fit = Fit { bpm: 120.0, bars: 4, len, key, xfade: len / 4 };
        // 2.5 % fast and a semitone sharp, as a separate take can come back.
        let l = fit_clip("b", &section(123.0, 1.0, 30.0, rate), rate, &fit).unwrap();
        assert_eq!(l.samples.len(), len);
        assert!((l.facts["tempo_fix_pct"].as_f64().unwrap() + 2.4).abs() < 0.3, "{}", l.facts);
        assert_eq!(l.facts["pitch_fix_semitones"], -1);
        assert!(l.score > 0.85 && l.facts["source"] == "clip", "{}", l.facts);
        let pre = (PREROLL * rate as f32) as usize;
        let w = (0.03 * rate as f32) as usize;
        let e = |s: usize| -> f32 { l.samples[s..s + w].iter().map(|v| v * v).sum() };
        assert!(e(pre) > 3.0 * e(pre + len / 32), "a hit on its first downbeat");
        // Three semitones off is too far to correct.
        assert!(fit_clip("c", &section(120.0, 3.0, 30.0, rate), rate, &fit).is_err());
    }

    #[test]
    fn plan_and_prompt() {
        let form = parse_sections("a,b,a").unwrap();
        let p = plan(&form, 8, 120.0, None);
        // 8 bars (16 s) + 3 bars = 22 s each, whole bars.
        assert_eq!(p.windows, vec![(0.0, 22.0), (22.0, 44.0), (44.0, 66.0)]);
        let t = prompt_lines(&form, &p);
        assert!(t.contains("120 BPM") && t.contains("[0:00 - 0:22] Section A: the main theme") && t.contains("[0:44 - 1:06] Section A: the A theme again"), "{t}");
        let z = parse_zones("lantern town: warm lanterns; bamboo grove: hollow wood").unwrap();
        let t = prompt_lines(&z, &plan(&z, 4, 100.0, None));
        assert!(t.contains("] Lantern Town: warm lanterns.") && t.contains("never the tempo or the key"), "{t}");
    }
}
