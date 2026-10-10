//! A music graph (`<name>.music.json`, written by `mb music --zones`,
//! `--sections` or `--variations`): one long generated take, where its
//! sections and their cues are, and which of its bar lines can be joined
//! without the join being heard. [`super::Director::from_graph`] plays it.
//!
//! Positions are bars of the take: bar `k` starts `first_bar + k × bar`
//! seconds into the take's file. A [`Link`] joins two bar lines: at bar line
//! `from` (where the take would go on into bar `from`) the music can go on
//! from bar `to` instead.

use super::json::Json;

/// The audio a graph plays, as asset paths (`"assets/run_take_base.ogg"`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Files {
    /// The take as one file, or its base and high layers (`--layers`).
    pub take: Option<String>,
    pub base: Option<String>,
    pub hi: Option<String>,
    /// The swell and where its hit lands (seconds in).
    pub swell: Option<(String, f32)>,
    pub tension: Option<TensionFiles>,
}

/// The tension kit (`--layers …,tension`): loops of `bars` bars on the take's grid, their 1 `downbeat` seconds in.
#[derive(Clone, Debug, PartialEq)]
pub struct TensionFiles {
    pub layer: String,
    pub tick: String,
    pub heartbeat: String,
    pub sting: String,
    pub bars: u32,
    pub downbeat: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub name: String,
    pub sections: Vec<usize>,
    /// The score its jump links are kept from.
    pub threshold: f32,
    /// Its stinger and where its hit lands (seconds in).
    pub stinger: Option<(String, f32)>,
}

/// A section of the take, a segment: its body from its entry bar to its exit
/// bar, with the take's own audio before it (pre-entry) and after it (post-exit).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub zone: usize,
    pub entry: u32,
    pub exit: u32,
    /// Bar lines that end a phrase (every 4 bars from its entry) and its exit.
    pub cues: Vec<u32>,
    /// Seconds of pre-entry before its entry, and of post-exit after its exit.
    pub pre_entry: f64,
    pub post_exit: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub from: u32,
    pub to: u32,
    pub score: f32,
    /// A link into another zone (or into the bars that lead into one): that zone.
    pub into_zone: Option<usize>,
}

pub struct Graph {
    pub name: String,
    pub bpm: f64,
    /// Seconds per bar, and where bar 0 starts in the take (seconds).
    pub bar: f64,
    pub first_bar: f64,
    pub phrase_bars: u32,
    /// The first usable bar and the bar after the last.
    pub first: u32,
    pub end: u32,
    /// How a link is spliced: the new voice fades in over `link_pre` beats before the bar line,
    /// the old one dies away over `link_post` beats after it.
    pub link_pre: f64,
    pub link_post: f64,
    /// A link into another zone at least this good needs no cover.
    pub seamless: f32,
    /// A jump back shorter than this (bars) repeats what was just heard.
    pub min_repeat: u32,
    pub files: Files,
    /// How much louder than [`super::Levels::default`] the tension kit plays
    /// (`levels.tension_gain`): `mb music` measures the take on a phone and
    /// writes what lifts it by a clear step at full danger. 1 in a graph
    /// written before it ([`super::Levels::for_graph`]).
    pub tension_gain: f32,
    pub zones: Vec<Zone>,
    pub sections: Vec<Section>,
    pub links: Vec<Link>,
}

fn s(j: &Json, k: &str) -> Option<String> {
    j.get(k).str().map(str::to_string)
}

impl Graph {
    /// Reads `<name>.music.json` (`sys::Asset::read`).
    pub fn parse(bytes: &[u8]) -> Result<Graph, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "music graph: not UTF-8".to_string())?;
        let j = Json::parse(text)?;
        if j.get("format").str() != Some("maimbrain-music-graph") {
            return Err("music graph: not a maimbrain-music-graph file".into());
        }
        if j.get("version").u32() != Some(1) {
            return Err("music graph: unknown version (update the SDK)".into());
        }
        let g = j.get("grid");
        let num = |v: &Json, what: &str| v.f64().ok_or_else(|| format!("music graph: no {what}"));
        let f = j.get("files");
        let cover = |v: &Json| -> Option<(String, f32)> { Some((s(v, "file")?, v.get("land_seconds").f64()? as f32)) };
        let t = f.get("tension");
        let tension = (|| {
            Some(TensionFiles {
                layer: s(t, "layer")?,
                tick: s(t, "tick")?,
                heartbeat: s(t, "heartbeat")?,
                sting: s(t, "sting")?,
                bars: t.get("bars").u32()?,
                downbeat: t.get("downbeat_seconds").f64()?,
            })
        })();
        let files = Files { take: s(f, "take"), base: s(f, "base"), hi: s(f, "hi"), swell: cover(f.get("swell")), tension };
        if files.take.is_none() && files.base.is_none() {
            return Err("music graph: no take file".into());
        }
        let zones = j
            .get("zones")
            .arr()
            .iter()
            .map(|z| Zone {
                name: s(z, "name").unwrap_or_default(),
                sections: z.get("sections").arr().iter().filter_map(|v| v.u32().map(|n| n as usize)).collect(),
                threshold: z.get("link_threshold").f64().unwrap_or(0.85) as f32,
                stinger: cover(z.get("stinger")),
            })
            .collect::<Vec<_>>();
        let sections = j
            .get("sections")
            .arr()
            .iter()
            .map(|v| {
                let seg = v.get("segment");
                let entry_s = seg.get("entry_seconds").f64().unwrap_or(0.0);
                let exit_s = seg.get("exit_seconds").f64().unwrap_or(0.0);
                Ok(Section {
                    name: s(v, "name").unwrap_or_default(),
                    zone: v.get("zone").u32().ok_or("music graph: a section without a zone")? as usize,
                    entry: v.get("entry_bar").u32().ok_or("music graph: a section without an entry")?,
                    exit: v.get("exit_bar").u32().ok_or("music graph: a section without an exit")?,
                    cues: v.get("exit_cues").arr().iter().filter_map(Json::u32).collect(),
                    pre_entry: (entry_s - seg.get("pre_entry_seconds").f64().unwrap_or(entry_s)).max(0.0),
                    post_exit: (seg.get("post_exit_seconds").f64().unwrap_or(exit_s) - exit_s).max(0.0),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let links = j
            .get("links")
            .arr()
            .iter()
            .filter_map(|l| {
                Some(Link {
                    from: l.get("from").u32()?,
                    to: l.get("to").u32()?,
                    score: l.get("score").f64()? as f32,
                    into_zone: l.get("into_zone").u32().map(|z| z as usize),
                })
            })
            .collect();
        let sp = j.get("splice");
        let graph = Graph {
            name: s(&j, "name").unwrap_or_default(),
            bpm: num(g.get("bpm"), "bpm")?,
            bar: num(g.get("bar_seconds"), "bar_seconds")?,
            first_bar: num(g.get("first_bar_seconds"), "first_bar_seconds")?,
            phrase_bars: g.get("phrase_bars").u32().unwrap_or(4).max(1),
            first: g.get("first_bar").u32().unwrap_or(0),
            end: g.get("end_bar").u32().ok_or("music graph: no end_bar")?,
            link_pre: sp.get("link_pre_beats").f64().unwrap_or(0.25),
            link_post: sp.get("link_post_beats").f64().unwrap_or(1.0),
            seamless: sp.get("seamless").f64().unwrap_or(0.8) as f32,
            min_repeat: sp.get("min_repeat_bars").u32().unwrap_or(8),
            files,
            tension_gain: j.get("levels").get("tension_gain").f64().filter(|g| g.is_finite() && *g > 0.0).unwrap_or(1.0) as f32,
            zones,
            sections,
            links,
        };
        if graph.zones.is_empty() || graph.sections.is_empty() || graph.bar <= 0.0 {
            return Err("music graph: no zones or sections".into());
        }
        if graph.sections.iter().any(|s| s.zone >= graph.zones.len() || s.exit <= s.entry) || graph.zones.iter().any(|z| z.sections.iter().any(|&s| s >= graph.sections.len())) {
            return Err("music graph: a section or zone refers to one that isn't there".into());
        }
        Ok(graph)
    }

    /// The zone named `name`.
    pub fn zone(&self, name: &str) -> Option<usize> {
        self.zones.iter().position(|z| z.name == name)
    }

    /// Every audio file the graph plays (load each with `sys::Asset::load`).
    pub fn files(&self) -> Vec<&str> {
        let f = &self.files;
        let mut v: Vec<&str> = [&f.take, &f.base, &f.hi].into_iter().flatten().map(String::as_str).collect();
        v.extend(f.swell.iter().map(|s| s.0.as_str()));
        if let Some(t) = &f.tension {
            v.extend([t.layer.as_str(), t.tick.as_str(), t.heartbeat.as_str(), t.sting.as_str()]);
        }
        v.extend(self.zones.iter().filter_map(|z| z.stinger.as_ref().map(|s| s.0.as_str())));
        v
    }

    /// Seconds into the take where bar `k` starts.
    pub fn bar_at(&self, k: u32) -> f64 {
        self.first_bar + k as f64 * self.bar
    }

    pub fn beat(&self) -> f64 {
        self.bar / 4.0
    }

    /// The section bar `k` is in.
    pub fn section_of(&self, k: u32) -> Option<usize> {
        self.sections.iter().position(|s| (s.entry..s.exit).contains(&k))
    }

    pub fn zone_of(&self, k: u32) -> Option<usize> {
        self.section_of(k).map(|s| self.sections[s].zone)
    }

    /// Whether bar line `k` is an exit cue of the section the bar before it is in.
    pub fn exit_cue(&self, k: u32) -> bool {
        k > 0 && self.section_of(k - 1).is_some_and(|s| self.sections[s].cues.contains(&k))
    }

    /// Where zone `z` starts: its first section's entry.
    pub fn top(&self, z: usize) -> u32 {
        self.sections[self.zones[z].sections[0]].entry
    }

    /// The zone a link takes the music into.
    pub fn link_zone(&self, l: &Link) -> Option<usize> {
        l.into_zone.or_else(|| self.zone_of(l.to))
    }
}
