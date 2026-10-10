//! A different song per zone (`mb music … --zone harbor="bluesy hard rock,
//! 128 bpm" --zone neon="flamenco-tango, 112 bpm"`): each zone its own
//! piece in its own genre, tempo and key, with its own tension kit, and
//! [`Songs`] moving between them the way a game changes tracks: the song
//! playing goes on to its next exit cue (the end of a phrase), dies away
//! under the next song's stinger, and the next song starts on that bar line
//! from its top. No crossfade between keys or tempos that don't match.
//!
//! ```ignore
//! use maimbrain::music::{Change, Director, Graph, Mood, Setlist, Songs};
//! // assets/brawl.songs.json lists each zone's music graph; load them, then every file they name.
//! let list = Setlist::parse(&setlist_asset.read().unwrap()).unwrap();
//! let mut songs = Songs::new();
//! for (zone, graph_path) in &list.songs {
//!     let g = Graph::parse(&assets[graph_path].read().unwrap()).unwrap();
//!     songs.add(zone, Director::from_graph(g, |p| sounds.get(p).copied(), sys::rand_seed()).unwrap());
//! }
//! let (harbor, neon) = (songs.song("harbor").unwrap(), songs.song("neon").unwrap());
//! songs.start(harbor, sys::time());
//! // Each frame: the song playing follows the fight.
//! songs.update(sys::time(), dt, Mood { intensity: combo_k, danger: 1.0 - boss_hp });
//! // The next boss walks in: its song at the end of this phrase, under its stinger.
//! songs.go(neon, Change::Auto, sys::time());
//! ```

use super::json::Json;
use super::{Change, Director, LOOKAHEAD, Live, Mood, Out};
use crate::audio::Bus;

/// `<name>.songs.json`, written by `mb music --zone`: each zone's song, as
/// the asset path of its music graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setlist {
    pub name: String,
    /// (zone, graph path), in the order they were asked for.
    pub songs: Vec<(String, String)>,
}

impl Setlist {
    pub fn parse(bytes: &[u8]) -> Result<Setlist, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "setlist: not UTF-8".to_string())?;
        let j = Json::parse(text)?;
        if j.get("format").str() != Some("maimbrain-songs") {
            return Err("setlist: not a maimbrain-songs file".into());
        }
        if j.get("version").u32() != Some(1) {
            return Err("setlist: unknown version (update the SDK)".into());
        }
        let songs = j
            .get("songs")
            .arr()
            .iter()
            .map(|s| Some((s.get("zone").str()?.to_string(), s.get("graph").str()?.to_string())))
            .collect::<Option<Vec<_>>>()
            .ok_or("setlist: a song without a zone or a graph")?;
        Ok(Setlist { name: j.get("name").str().unwrap_or("").to_string(), songs })
    }
}

/// A change of song waiting for its bar line.
#[derive(Clone, Copy, Debug)]
struct Switch {
    to: usize,
    /// The bar line the old song leaves on and the new one starts (game time).
    cue: f64,
    /// Under the new song's stinger (or swell); else the old song dies away over `fade`.
    covered: bool,
    fade: f32,
}

/// Several songs, one per zone, each its own [`Director`] (its own graph,
/// layers and tension kit), one playing at a time.
pub struct Songs<O: Out = Live> {
    songs: Vec<(String, Director<O>)>,
    current: Option<usize>,
    pending: Option<Switch>,
}

impl Songs<Live> {
    pub fn new() -> Songs<Live> {
        Songs::empty()
    }
}

impl Default for Songs<Live> {
    fn default() -> Songs<Live> {
        Songs::new()
    }
}

impl<O: Out> Songs<O> {
    /// No songs yet, for any [`Out`] ([`super::Log`] in tests and renders).
    pub fn empty() -> Songs<O> {
        Songs { songs: Vec::new(), current: None, pending: None }
    }

    /// Adds a zone's song; returns its index.
    pub fn add(&mut self, zone: &str, director: Director<O>) -> usize {
        self.songs.push((zone.to_string(), director));
        self.songs.len() - 1
    }

    /// The song of the zone named `zone`.
    pub fn song(&self, zone: &str) -> Option<usize> {
        self.songs.iter().position(|s| s.0 == zone)
    }

    pub fn director(&self, song: usize) -> Option<&Director<O>> {
        self.songs.get(song).map(|s| &s.1)
    }

    pub fn director_mut(&mut self, song: usize) -> Option<&mut Director<O>> {
        self.songs.get_mut(song).map(|s| &mut s.1)
    }

    /// The song playing (or the one a waiting change goes to).
    pub fn current(&self) -> Option<usize> {
        self.pending.map(|p| p.to).or(self.current)
    }

    /// When the waiting change of song happens (game time), if one is waiting.
    pub fn change_at(&self) -> Option<f64> {
        self.pending.map(|p| p.cue)
    }

    /// Starts `song` from its top (anything playing dies away first).
    pub fn start(&mut self, song: usize, now: f64) {
        if song >= self.songs.len() {
            return;
        }
        self.stop(now, super::RESTART_FADE);
        self.songs[song].1.start(0, now);
        self.current = Some(song);
    }

    /// Changes to `song`. [`Change::Auto`] and [`Change::Stinger`]: at the
    /// playing song's next exit cue, under the new song's stinger (else its
    /// swell): the old song dies away under the stinger's rise and the new
    /// one starts from its top on the cue. [`Change::NextBar`]: the same on
    /// the next bar line. [`Change::Crossfade`]`(bars)`: on the next bar line
    /// without a cover, the old song dying away over that many of its bars.
    /// A change already waiting is replaced; asking for the song playing
    /// cancels it.
    pub fn go(&mut self, song: usize, change: Change, now: f64) {
        if song >= self.songs.len() {
            return;
        }
        let Some(cur) = self.current else {
            self.start(song, now);
            return;
        };
        if song == cur {
            self.pending = None;
            return;
        }
        let new = &self.songs[song].1;
        let cover = new.cover_into(0);
        let lead = LOOKAHEAD + new.entry_pre(0) + cover.map_or(0.0, |c| c.1 as f64) + 0.05;
        let old = &self.songs[cur].1;
        let (cue, covered, fade) = match change {
            Change::Auto | Change::Stinger => (old.next_exit(now + lead), cover.is_some(), 0.5),
            Change::NextBar => (old.next_bar(now + lead), cover.is_some(), 0.5),
            Change::Crossfade(bars) => (old.next_bar(now + LOOKAHEAD + 0.05), false, (bars.max(0.0) as f64 * old.bar()) as f32),
        };
        let fade = if covered || matches!(change, Change::Crossfade(_)) { fade } else { old.bar() as f32 };
        self.pending = Some(Switch { to: song, cue, covered, fade });
    }

    /// Call every frame: the song playing follows `mood`, and a waiting
    /// change is made in time for its bar line.
    pub fn update(&mut self, now: f64, dt: f32, mood: Mood) {
        if let Some(sw) = self.pending {
            let new = &self.songs[sw.to].1;
            let cover = if sw.covered { new.cover_into(0) } else { None };
            let lead = LOOKAHEAD + new.entry_pre(0) + cover.map_or(0.0, |c| c.1 as f64) + 0.02;
            if now + lead >= sw.cue {
                self.pending = None;
                if let Some(cur) = self.current {
                    let old = &mut self.songs[cur].1;
                    // Under a stinger the old song lets go half a beat before the cue, under the rise.
                    let from = if cover.is_some() { sw.cue - 0.5 * old.beat() } else { sw.cue };
                    old.stop(from.max(now), sw.fade);
                }
                let new = &mut self.songs[sw.to].1;
                new.start_at(0, sw.cue);
                if let Some((s, land)) = cover {
                    let vol = new.levels.stinger;
                    new.shot(s, vol, 1.0, sw.cue - land as f64, Bus::Music);
                }
                self.current = Some(sw.to);
            }
        }
        if let Some(cur) = self.current {
            self.songs[cur].1.update(now, dt, mood);
        }
    }

    /// A near-catch on the song playing ([`Director::near_miss`]).
    pub fn near_miss(&mut self, now: f64) {
        if let Some(cur) = self.current {
            self.songs[cur].1.near_miss(now);
        }
    }

    /// Stops every song ([`Director::stop`]) and forgets a waiting change.
    pub fn stop(&mut self, now: f64, fade: f32) {
        self.pending = None;
        for (_, d) in &mut self.songs {
            if d.playing() {
                d.stop(now, fade);
            }
        }
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Graph, Log, Mood};
    use super::*;
    use crate::audio::Sound;

    /// A song of one zone: 16 bars of `bar` seconds, cues every 4, a stinger landing 1 s in.
    fn graph(name: &str, bpm: f64) -> String {
        let bar = 240.0 / bpm;
        format!(
            r#"{{ "format": "maimbrain-music-graph", "version": 1, "name": "{name}",
              "files": {{ "take": "assets/{name}_take.ogg", "swell": {{ "file": "assets/{name}_swell.ogg", "land_seconds": 0.5 }} }},
              "grid": {{ "bpm": {bpm}, "beats_per_bar": 4, "bar_seconds": {bar}, "first_bar_seconds": 0.1, "phrase_bars": 4, "first_bar": 0, "end_bar": 16 }},
              "splice": {{ "link_pre_beats": 0.25, "link_post_beats": 1.0, "seamless": 0.8, "min_repeat_bars": 8 }},
              "zones": [ {{ "name": "{name}", "sections": [0], "link_threshold": 0.85,
                            "stinger": {{ "file": "assets/{name}_into.ogg", "land_seconds": 1.0 }} }} ],
              "sections": [ {{ "name": "{name}", "zone": 0, "entry_bar": 0, "exit_bar": 16, "exit_cues": [4, 8, 12, 16],
                  "segment": {{ "pre_entry_seconds": 0.0, "entry_seconds": 0.1, "exit_seconds": {e}, "post_exit_seconds": {p} }} }} ],
              "links": [] }}"#,
            e = 0.1 + 16.0 * bar,
            p = 0.1 + 17.0 * bar
        )
    }

    fn songs() -> Songs<Log> {
        let mut s = Songs::empty();
        for (i, (name, bpm)) in [("harbor", 128.0), ("neon", 112.0)].into_iter().enumerate() {
            let g = Graph::parse(graph(name, bpm).as_bytes()).unwrap();
            let files = g.files().iter().map(|f| f.to_string()).collect::<Vec<_>>();
            let mut log = Log::with_ids_after(i as u32 * 1000);
            let base = i as u32 * 10 + 1;
            for (k, f) in files.iter().enumerate() {
                log.name(Sound(base + k as u32), f.trim_start_matches("assets/").trim_end_matches(".ogg"));
            }
            let sound = |p: &str| files.iter().position(|f| f == p).map(|k| Sound(base + k as u32));
            s.add(name, Director::with_graph(g, sound, 5, log).unwrap());
        }
        s
    }

    #[test]
    fn setlists_parse() {
        let l = Setlist::parse(br#"{"format":"maimbrain-songs","version":1,"name":"brawl","songs":[{"zone":"harbor","graph":"assets/brawl_harbor.music.json"},{"zone":"neon","graph":"assets/brawl_neon.music.json"}]}"#).unwrap();
        assert_eq!(l.songs[1], ("neon".to_string(), "assets/brawl_neon.music.json".to_string()));
        assert!(Setlist::parse(b"{}").is_err());
    }

    #[test]
    fn a_change_waits_for_the_exit_cue_and_lands_the_stinger_on_the_new_songs_downbeat() {
        let mut s = songs();
        let (harbor, neon) = (s.song("harbor").unwrap(), s.song("neon").unwrap());
        s.start(harbor, 0.0);
        let origin = s.director(harbor).unwrap().origin();
        let bar = 240.0 / 128.0;
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        while t < 3.0 {
            s.update(t, dt as f32, Mood::default());
            t += dt;
        }
        // Asked for in bar 2: the cue is the end of the first phrase (bar line 4).
        s.go(neon, Change::Auto, t);
        let cue = s.change_at().unwrap();
        assert!((cue - (origin + 4.0 * bar)).abs() < 1e-6, "{cue} vs {}", origin + 4.0 * bar);
        assert_eq!(s.current(), Some(neon));
        while t < cue + 4.0 {
            s.update(t, dt as f32, Mood::default());
            t += dt;
        }
        let new = s.director(neon).unwrap();
        assert!((new.origin() - cue).abs() < 1e-6, "the new song starts on the cue");
        assert!(!s.director(harbor).unwrap().playing());
        let old_log = s.director(harbor).unwrap().out.text();
        let new_log = s.director(neon).unwrap().out.text();
        // The old take lets go half a beat before the cue; the stinger's hit lands on it.
        let stop = old_log.lines().find(|l| l.contains("harbor_take 0 1 stop")).unwrap();
        let at: f64 = stop.split(' ').next().unwrap().parse().unwrap();
        assert!((at - (cue - 0.5 * 60.0 / 128.0)).abs() < 2e-3, "{stop}");
        let sting = new_log.lines().find(|l| l.contains("neon_into")).unwrap();
        let at: f64 = sting.split(' ').next().unwrap().parse().unwrap();
        assert!((at - (cue - 1.0)).abs() < 2e-3, "{sting}");
        assert!(new_log.contains("neon_take"));
        // Ids don't collide, so the two logs merge into one render.
        assert!(!new_log.contains(" id=1\n") && old_log.contains(" id=1"));
    }

    #[test]
    fn crossfade_has_no_cover_and_going_back_cancels() {
        let mut s = songs();
        let (harbor, neon) = (0, 1);
        s.start(harbor, 0.0);
        s.update(0.5, 0.016, Mood::default());
        s.go(neon, Change::Crossfade(1.0), 0.5);
        s.go(harbor, Change::Auto, 0.6);
        assert_eq!(s.change_at(), None);
        s.go(neon, Change::Crossfade(1.0), 0.6);
        let cue = s.change_at().unwrap();
        let mut t = 0.6;
        while t < cue + 1.0 {
            s.update(t, 0.016, Mood::default());
            t += 0.016;
        }
        assert!(!s.director(neon).unwrap().out.text().contains("neon_into"));
        assert!(s.director(harbor).unwrap().out.text().contains(&format!("harbor_take 0 1 stop fade={:.3}", 240.0 / 128.0)));
        s.stop(t, 1.0);
        assert_eq!(s.current(), None);
        assert!(!s.director(neon).unwrap().playing());
    }
}
