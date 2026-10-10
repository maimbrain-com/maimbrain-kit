//! The music graph of a take (`mb music --zones/--sections/--variations`):
//! how a game moves through one long generated performance instead of
//! repeating loops cut out of it (the SDK's `music::Director` plays it).
//!
//! A generative model writes music that develops rather than loops, so a
//! loop cut out of a take joins weakly, and cutting it halves the variety.
//! Games solved adaptive music long ago; this does what they do:
//!
//! - **Segments** (Wwise and FMOD practice). Each section of the take is a
//!   segment: a body from its entry cue (its first bar line) to its exit cue
//!   (the last), with the take's own audio around it as a **pre-entry** (the
//!   beat before the entry: the pickup) and a **post-exit** (the bar after
//!   the exit, its tail). On a transition the next segment starts so that
//!   its entry cue lands on the exit cue, its pre-entry fading in under the
//!   last beat, and the old segment's post-exit dies away over it.
//! - **Transitions only at exit cues** (horizontal re-sequencing): exit cues
//!   are at the ends of the take's 4-bar phrases, counted from each section's
//!   entry, and every bar line is an intermediate cue for an urgent change.
//!   The transition table says, for each pair of zones, whether the take
//!   already runs from one into the other, how good the best seamless jump
//!   between them is, and what covers a change that has none (the target's
//!   stinger, or the swell).
//! - **Jump links** (Paul Lamere's Infinite Jukebox). The take is analysed
//!   beat by beat on its one bar grid (grid.rs: harmony from pitch-class
//!   profiles, level and timbre in eight bands per half beat, rhythm from the
//!   onset envelope). Two bar lines are linked when the bar before each and
//!   the bar after each sound alike: playing the take up to one and going on
//!   from the other keeps the beat of the bar, the harmony and the groove, so
//!   there is no loop point to hear. The threshold starts at 0.85 and is
//!   lowered per zone (to 0.7 at the least) until each has enough links.
//!   The Director plays the take straight through and takes a link now and
//!   then, so a zone's music goes on for as long as the player stays,
//!   without a loop anyone can hear; and it reaches another zone through a
//!   link into it, or the take's own transition, at the next good cue.
//!
//! Positions are bar numbers: bar k starts at `first_bar_seconds + k ×
//! bar_seconds` in the take file. Links join bar lines: `from` is the bar
//! the take would play next, `to` the bar played instead.

use serde_json::{Value, json};

use super::grid::{Feats, Grid, Match};

/// Bars per phrase: exit cues are every this many bars from a section's entry.
pub const PHRASE_BARS: usize = 4;
/// Links are kept from this score down, lowered per zone until it has enough.
pub const LINK_START: f32 = 0.85;
/// The lowest a zone's link threshold goes.
pub const LINK_FLOOR: f32 = 0.7;
/// Links a zone should have (each a jump of at least `MIN_REPEAT` bars, or forward).
pub const MIN_LINKS: usize = 6;
/// A link between zones this good needs no cover.
pub const SEAMLESS: f32 = 0.8;
/// Links between zones are kept from this score (the best few per bar line).
pub const CROSS_FLOOR: f32 = 0.65;
/// The shortest jump kept, bars (a jump back of one bar is a stutter).
pub const MIN_JUMP: usize = 2;
/// A jump back of fewer bars than this repeats what was just heard; the Director avoids them.
pub const MIN_REPEAT: usize = 8;
/// How a link is spliced: the new voice fades in over this much of a beat before the bar line,
/// and the old one dies away over this many beats after it.
pub const LINK_PRE_BEATS: f64 = 0.25;
pub const LINK_POST_BEATS: f64 = 1.0;
/// A segment's pre-entry (beats) and post-exit (bars).
pub const PRE_ENTRY_BEATS: f64 = 1.0;
pub const POST_EXIT_BARS: f64 = 1.0;
/// Bars quieter than the take's median by this much at its ends are its fade-in and fade-out.
const FADE_DB: f32 = 8.0;
/// How far (bars) a section's entry may move from where it was asked for, onto the biggest change.
const ENTRY_SEARCH: i64 = 2;

/// One stretch of the take: a section of the form, on whole bars.
#[derive(Clone, Debug)]
pub struct Span {
    pub section: String,
    pub zone: usize,
    /// Its first bar and the bar after its last.
    pub entry: usize,
    pub exit: usize,
    /// Where the request put its entry (bars), and how much the music changes at the entry found (0–1).
    pub planned: i64,
    pub novelty: f32,
}

impl Span {
    /// Its exit cues: the bar lines that end a phrase, and its exit.
    pub fn cues(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (1..).map(|k| self.entry + k * PHRASE_BARS).take_while(|&b| b < self.exit).collect();
        v.push(self.exit);
        v
    }
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub name: String,
    pub spans: Vec<usize>,
    /// Its links' threshold, after lowering.
    pub threshold: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub m: Match,
    /// Both ends on phrase lines (the end of a phrase into the start of one).
    pub phrase: bool,
    /// Between zones.
    pub cross: bool,
}

pub struct Graph {
    /// The take's grid in samples.
    pub grid: Grid,
    pub rate: u32,
    /// The first and the bar after the last usable bar (past the fade-in, short of the fade-out).
    pub first: usize,
    pub end: usize,
    pub spans: Vec<Span>,
    pub zones: Vec<Zone>,
    pub links: Vec<Link>,
    pub returns: Vec<Return>,
    /// Things that may be heard, in words.
    pub risks: Vec<String>,
}

/// A stretch of the take asked for: its section and zone, and where it starts (samples).
pub struct Asked {
    pub section: String,
    pub zone: String,
    pub start: usize,
}

impl Graph {
    pub fn bar_seconds(&self) -> f64 {
        self.grid.bar / self.rate as f64
    }

    /// Seconds into the take of bar `k`'s line.
    pub fn bar_at(&self, k: usize) -> f64 {
        (self.grid.downbeat + k as f64 * self.grid.bar) / self.rate as f64
    }

    /// The span bar `k` is in.
    pub fn span_of(&self, k: usize) -> Option<usize> {
        self.spans.iter().position(|s| (s.entry..s.exit).contains(&k))
    }

    pub fn zone_of(&self, k: usize) -> Option<usize> {
        self.span_of(k).map(|s| self.spans[s].zone)
    }

    /// Whether bar line `k` ends a phrase of the bar before it (or is a span's exit).
    pub fn phrase_end(&self, k: usize) -> bool {
        k > 0 && self.span_of(k - 1).is_some_and(|s| (k - self.spans[s].entry).is_multiple_of(PHRASE_BARS) || k == self.spans[s].exit)
    }

    /// Whether bar `k` starts a phrase.
    pub fn phrase_start(&self, k: usize) -> bool {
        self.span_of(k).is_some_and(|s| (k - self.spans[s].entry).is_multiple_of(PHRASE_BARS))
    }
}

/// Per-bar loudness (dB of RMS) of the bars `grid` has in `x`.
fn bar_levels(x: &[f32], g: Grid) -> Vec<f32> {
    let mut v = Vec::new();
    let mut k = 0;
    loop {
        let a = (g.downbeat + k as f64 * g.bar).round() as usize;
        let b = (g.downbeat + (k + 1) as f64 * g.bar).round() as usize;
        if b > x.len() {
            break;
        }
        let p = x[a..b].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (b - a).max(1) as f64;
        v.push((10.0 * (p + 1e-12).log10()) as f32);
        k += 1;
    }
    v
}

fn median(v: &[f32]) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s.get(s.len() / 2).copied().unwrap_or(0.0)
}

/// The music graph of take `x` on its grid `g`, with the stretches the request asked for.
pub fn build(x: &[f32], rate: u32, g: Grid, asked: &[Asked]) -> Result<Graph, String> {
    let beat = g.beat();
    let feats = Feats::new(x, rate, g.downbeat, beat);
    let bi = |k: usize| feats.index(g.downbeat + k as f64 * g.bar);
    // Usable bars: a bar of context before (for the link windows) and after, past the fade-in, short of the fade-out.
    let levels = bar_levels(x, g);
    let total = levels.len();
    if total < 2 * PHRASE_BARS + 2 {
        return Err(format!("the take has {total} bars, too few for a music graph"));
    }
    let med = median(&levels);
    let mut first = 1;
    while first < total && levels[first] < med - FADE_DB {
        first += 1;
    }
    let mut end = total - 1; // the last bar is context for the one before
    while end > first + 1 && levels[end - 1] < med - FADE_DB {
        end -= 1;
    }
    let mut risks = Vec::new();
    // How alike n beats before and after bar lines i and j are (the window a jump from i to j splices).
    let around = |i: usize, j: usize, before: usize, after: usize| -> Option<Match> {
        let (a, b) = (bi(i) - before as isize, bi(j) - before as isize);
        feats.compare(a, b, before + after)
    };
    // How much the music changes at bar line k: the two bars before against the two after.
    let novelty = |k: usize| -> f32 { if k < 2 { 0.0 } else { feats.compare(bi(k) - 8, bi(k), 8).map_or(0.0, |m| 1.0 - m.score) } };
    // Zones, in the order first asked.
    let mut zones: Vec<Zone> = Vec::new();
    for a in asked {
        if !zones.iter().any(|z| z.name == a.zone) {
            zones.push(Zone { name: a.zone.clone(), spans: Vec::new(), threshold: LINK_START });
        }
    }
    // Spans: each asked stretch from the bar nearest its start, moved up to two bars onto the biggest change.
    let mut spans: Vec<Span> = Vec::new();
    for (n, a) in asked.iter().enumerate() {
        let planned = ((a.start as f64 - g.downbeat) / g.bar).round() as i64;
        let (entry, nov) = if n == 0 {
            (first, 0.0)
        } else {
            let lo = (planned - ENTRY_SEARCH).max(first as i64 + 1) as usize;
            let hi = ((planned + ENTRY_SEARCH) as usize).min(end.saturating_sub(1));
            let prev = spans.last().map_or(first, |s: &Span| s.entry);
            (lo.max(prev + PHRASE_BARS)..=hi).map(|k| (k, novelty(k))).fold((planned.max(0) as usize, -1.0f32), |b, c| if c.1 > b.1 { c } else { b })
        };
        let zone = zones.iter().position(|z| z.name == a.zone).unwrap();
        spans.push(Span { section: a.section.clone(), zone, entry, exit: 0, planned, novelty: nov.max(0.0) });
    }
    for n in 0..spans.len() {
        spans[n].exit = if n + 1 < spans.len() { spans[n + 1].entry } else { end };
    }
    spans.retain(|s| s.exit > s.entry + 1);
    for (i, s) in spans.iter().enumerate() {
        zones[s.zone].spans.push(i);
    }
    let mut graph = Graph { grid: g, rate, first, end, spans, zones, links: Vec::new(), returns: Vec::new(), risks: Vec::new() };
    // Every pair of bar lines: in-zone candidates (leaving zone Z's bar before `from` for Z's bar `to`)
    // and cross-zone ones (into another zone's bar, or the two bars that lead into one of its spans).
    let mut inzone: Vec<Link> = Vec::new();
    let mut cross: Vec<Link> = Vec::new();
    for i in first + 1..=end {
        let Some(za) = graph.zone_of(i - 1) else { continue };
        for j in first..end {
            if j.abs_diff(i) < MIN_JUMP {
                continue;
            }
            let zb = graph.zone_of(j);
            // A lead-in: one of the two bars before a span of another zone (the take's own run-up into it).
            let lead = graph.spans.iter().find(|s| s.zone != za && j < s.entry && j + 2 >= s.entry).map(|s| s.zone);
            let target = if zb == Some(za) { Some(za) } else { lead.or(zb) };
            let Some(zt) = target else { continue };
            let Some(m) = around(i, j, 4, 4) else { continue };
            let phrase = graph.phrase_end(i) && graph.phrase_start(j);
            let l = Link { from: i, to: j, m, phrase, cross: zt != za };
            if zt == za {
                inzone.push(l);
            } else if m.score >= CROSS_FLOOR {
                cross.push(l);
            }
        }
    }
    // Each zone's threshold: lowered until it has enough links that aren't short repeats.
    for z in 0..graph.zones.len() {
        let mine: Vec<&Link> = inzone.iter().filter(|l| graph.zone_of(l.from - 1) == Some(z)).collect();
        let useful = |t: f32| mine.iter().filter(|l| l.m.score >= t && (l.to > l.from || l.from - l.to >= MIN_REPEAT)).count();
        let mut t = LINK_START;
        while t > LINK_FLOOR + 1e-6 && useful(t) < MIN_LINKS {
            t -= 0.025;
        }
        graph.zones[z].threshold = (t * 1000.0).round() / 1000.0;
        if useful(t) < MIN_LINKS {
            let n = useful(t);
            risks.push(format!("{}: only {n} jump link{} at {t:.2} or better (not counting short jumps back); it plays its stretch of the take through more often", graph.zones[z].name, if n == 1 { "" } else { "s" }));
        }
    }
    let mut links: Vec<Link> = inzone.iter().filter(|l| l.m.score >= graph.zones[graph.zone_of(l.from - 1).unwrap()].threshold).copied().collect();
    // The way back from the end of every span whose next bar leaves its zone: a link from its last
    // phrase into the zone at least MIN_REPEAT bars back (or into another of its spans), or else a
    // restart: the zone's first segment again from its entry, under the swell.
    let mut returns = Vec::new();
    for s in 0..graph.spans.len() {
        let sp = graph.spans[s].clone();
        if graph.zone_of(sp.exit) == Some(sp.zone) {
            continue;
        }
        let lo = sp.exit.saturating_sub(PHRASE_BARS).max(sp.entry + 1);
        let back = |l: &&Link| (lo..=sp.exit).contains(&l.from) && graph.zone_of(l.from - 1) == Some(sp.zone) && (l.to + MIN_REPEAT <= l.from || graph.span_of(l.to) != Some(s));
        let best = inzone.iter().filter(back).max_by(|a, b| a.m.score.total_cmp(&b.m.score)).map(|l| l.m.score);
        let top = graph.spans[graph.zones[sp.zone].spans[0]].entry;
        let link = links.iter().any(|l| back(&l));
        returns.push(Return { span: s, from: sp.exit, to: top, link, best });
    }
    // Cross-zone: the best three per bar line and target zone.
    cross.sort_by(|a, b| b.m.score.total_cmp(&a.m.score));
    let mut kept: Vec<(usize, usize, usize)> = Vec::new(); // from, zone, count
    for l in cross {
        let zt = graph.zone_of(l.to).filter(|&z| Some(z) != graph.zone_of(l.from - 1)).or_else(|| graph.spans.iter().find(|s| l.to < s.entry && l.to + 2 >= s.entry).map(|s| s.zone)).unwrap();
        match kept.iter_mut().find(|k| k.0 == l.from && k.1 == zt) {
            Some(k) if k.2 >= 3 => continue,
            Some(k) => k.2 += 1,
            None => kept.push((l.from, zt, 1)),
        }
        links.push(l);
    }
    links.sort_by_key(|l| (l.from, l.to));
    graph.links = links;
    graph.returns = returns;
    graph.risks = risks;
    Ok(graph)
}

/// How the music stays in a zone at the end of one of its spans.
#[derive(Clone, Copy, Debug)]
pub struct Return {
    pub span: usize,
    /// The span's exit, and the zone's first entry (where a restart goes).
    pub from: usize,
    pub to: usize,
    /// A link from its last phrase back into the zone is good enough; else it restarts.
    pub link: bool,
    /// The best link back there, good enough or not.
    pub best: Option<f32>,
}

fn r2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}

fn r4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// What the zones' changes look like, from `a` into `b`: whether the take runs from one into
/// the other, how many of `a`'s exit cues have a seamless link into `b`, and the best link.
pub struct Transition {
    pub from: usize,
    pub to: usize,
    pub take: bool,
    pub seamless_cues: usize,
    pub cues: usize,
    pub best: f32,
}

impl Graph {
    /// The zone a cross link goes into.
    pub fn target_zone(&self, l: &Link) -> Option<usize> {
        if !l.cross {
            return self.zone_of(l.to);
        }
        let from = self.zone_of(l.from - 1);
        self.zone_of(l.to).filter(|&z| Some(z) != from).or_else(|| self.spans.iter().find(|s| l.to < s.entry && l.to + 2 >= s.entry).map(|s| s.zone))
    }

    pub fn transitions(&self) -> Vec<Transition> {
        let mut v = Vec::new();
        for a in 0..self.zones.len() {
            for b in 0..self.zones.len() {
                if a == b {
                    continue;
                }
                let take = self.zones[a].spans.iter().any(|&s| self.zone_of(self.spans[s].exit) == Some(b));
                let cues: Vec<usize> = self.zones[a].spans.iter().flat_map(|&s| self.spans[s].cues()).collect();
                let into = |k: usize| self.links.iter().filter(move |l| l.from == k && l.cross && self.target_zone(l) == Some(b));
                let seamless_cues = cues.iter().filter(|&&k| into(k).any(|l| l.m.score >= SEAMLESS)).count();
                let best = self.links.iter().filter(|l| l.cross && self.zone_of(l.from - 1) == Some(a) && self.target_zone(l) == Some(b)).map(|l| l.m.score).fold(0.0f32, f32::max);
                v.push(Transition { from: a, to: b, take, seamless_cues, cues: cues.len(), best });
            }
        }
        v
    }

    /// Outgoing links (in-zone, not forced) per span, and how many are phrase-aligned.
    pub fn links_per_span(&self) -> Vec<(usize, usize)> {
        self.spans
            .iter()
            .map(|s| {
                let mine: Vec<&Link> = self.links.iter().filter(|l| !l.cross && (s.entry + 1..=s.exit).contains(&l.from)).collect();
                (mine.len(), mine.iter().filter(|l| l.phrase).count())
            })
            .collect()
    }

    /// The `<name>.music.json` asset: `files` names the audio (take, layers, stingers, kit).
    pub fn to_json(&self, name: &str, bpm: f32, files: Value, zone_extra: &[Value]) -> Value {
        let beat = self.bar_seconds() / 4.0;
        let spans: Vec<Value> = self
            .spans
            .iter()
            .map(|s| {
                let entry = self.bar_at(s.entry);
                let exit = self.bar_at(s.exit);
                json!({
                    "name": s.section,
                    "zone": s.zone,
                    "entry_bar": s.entry,
                    "exit_bar": s.exit,
                    "exit_cues": s.cues(),
                    "segment": {
                        "pre_entry_seconds": r4((entry - PRE_ENTRY_BEATS * beat).max(0.0)),
                        "entry_seconds": r4(entry),
                        "exit_seconds": r4(exit),
                        "post_exit_seconds": r4(exit + POST_EXIT_BARS * self.bar_seconds()),
                    },
                    "entry_change": r2(s.novelty),
                    "entry_moved_bars": s.entry as i64 - s.planned,
                })
            })
            .collect();
        let zones: Vec<Value> = self
            .zones
            .iter()
            .enumerate()
            .map(|(i, z)| {
                let mut v = json!({ "name": z.name, "sections": z.spans, "link_threshold": (z.threshold as f64 * 1000.0).round() / 1000.0 });
                if let Some(Value::Object(m)) = zone_extra.get(i) {
                    for (k, x) in m {
                        v[k] = x.clone();
                    }
                }
                v
            })
            .collect();
        let transitions: Vec<Value> = self
            .transitions()
            .iter()
            .map(|t| {
                json!({
                    "from": t.from,
                    "to": t.to,
                    "when": "exit_cue",
                    "take_runs_into_it": t.take,
                    "seamless_cues": t.seamless_cues,
                    "cues": t.cues,
                    "best_link": r2(t.best),
                    "cover": if t.seamless_cues * 2 >= t.cues.max(1) { "none" } else { "stinger" },
                })
            })
            .collect();
        let links: Vec<Value> = self
            .links
            .iter()
            .map(|l| {
                let mut v = json!({ "from": l.from, "to": l.to, "score": r2(l.m.score), "harmony": r2(l.m.harmony), "level_db": (l.m.level_db as f64 * 10.0).round() / 10.0, "rhythm": r2(l.m.rhythm) });
                if l.phrase {
                    v["phrase"] = json!(true);
                }
                if l.cross {
                    v["into_zone"] = json!(self.target_zone(l));
                }

                v
            })
            .collect();
        json!({
            "format": "maimbrain-music-graph",
            "version": 1,
            "name": name,
            "files": files,
            "grid": {
                "bpm": (bpm as f64 * 1000.0).round() / 1000.0,
                "beats_per_bar": 4,
                "bar_seconds": (self.bar_seconds() * 1e6).round() / 1e6,
                "first_bar_seconds": (self.grid.downbeat / self.rate as f64 * 1e6).round() / 1e6,
                "phrase_bars": PHRASE_BARS,
                "first_bar": self.first,
                "end_bar": self.end,
            },
            "splice": {
                "link_pre_beats": LINK_PRE_BEATS,
                "link_post_beats": LINK_POST_BEATS,
                "seamless": r2(SEAMLESS),
                "min_repeat_bars": MIN_REPEAT,
            },
            "zones": zones,
            "sections": spans,
            "transitions": transitions,
            "returns": self.returns.iter().map(|r| json!({
                "section": r.span,
                "at_bar": r.from,
                "to_bar": r.to,
                "via": if r.link { "link" } else { "restart" },
                "best_link": r.best.map(r2),
            })).collect::<Vec<_>>(),
            "links": links,
            "risks": self.risks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::audio;

    /// A take of three sections on one grid: a and c the same groove on the same chords, b another.
    #[test]
    fn links_join_alike_bars_and_zones_end_with_a_way_back() {
        let rate = 22050;
        let bpm = 120.0;
        let x = audio::tests::song(bpm, 60.0, rate);
        let bar = 4.0 * 60.0 / bpm as f64 * rate as f64;
        let (ph, _) = super::super::grid::beat_phase(&x, rate, bar / 4.0);
        let g = Grid { downbeat: ph, bar };
        let asked = [
            Asked { section: "a".into(), zone: "town".into(), start: 0 },
            Asked { section: "b".into(), zone: "grove".into(), start: 20 * rate as usize },
            Asked { section: "c".into(), zone: "town".into(), start: 40 * rate as usize },
        ];
        let gr = build(&x, rate, g, &asked).unwrap();
        assert_eq!(gr.zones.len(), 2);
        assert_eq!(gr.zones[0].spans, vec![0, 2]);
        // The test song repeats its chords every 4 bars: links join bars 4k apart at the same place in the progression.
        let inz: Vec<&Link> = gr.links.iter().filter(|l| !l.cross).collect();
        assert!(inz.len() >= MIN_LINKS, "{}", inz.len());
        for l in &inz {
            assert!(l.m.score >= LINK_FLOOR);
            assert_eq!(gr.zone_of(l.from - 1), gr.zone_of(l.to));
        }
        assert!(inz.iter().filter(|l| l.m.score > 0.9).all(|l| l.from.abs_diff(l.to) % 4 == 0), "the best links keep the progression");
        // Every span ends where its zone does (c at the take's end), so each has a way back: the town's
        // a link (the song repeats); the grove's is too short for a jump back of 8 bars, so it restarts.
        let r: Vec<(usize, bool, usize)> = gr.returns.iter().map(|r| (r.span, r.link, r.to)).collect();
        assert_eq!(r, vec![(0, true, gr.spans[0].entry), (1, false, gr.spans[1].entry), (2, true, gr.spans[0].entry)], "{:?}", gr.returns);
        let j = gr.to_json("run", 120.0, json!({}), &[]);
        assert_eq!(j["format"], "maimbrain-music-graph");
        assert_eq!(j["sections"].as_array().unwrap().len(), 3);
        assert!(j["transitions"].as_array().unwrap().iter().any(|t| t["from"] == 0 && t["to"] == 1 && t["take_runs_into_it"] == true));
    }
}
