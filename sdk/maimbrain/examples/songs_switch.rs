//! A fight that moves from one boss's song to the next with
//! `music::Songs`, written down as one `mix_check.py` event log.
//!
//! The music is `mb music <game> brawl "…" --zone harbor="bluesy hard rock,
//! 128 bpm" --zone neon="flamenco-tango, 112 bpm"`: a song per zone, listed in
//! `brawl.songs.json`.
//!
//!     cargo run -p maimbrain --example songs_switch -- <game>/assets brawl > songs.log
//!     python3 <skill>/scripts/mix_check.py <game>/assets songs.log --out songs.wav
//!
//! 50 s: the first song while its boss fight heats up (the danger rising to
//! full by 20 s, the tension kit with it), the next boss walks in at 24 s
//! (the change lands on the first song's next exit cue under the second
//! song's stinger), then the second song calms and heats up again.

use maimbrain::audio::Sound;
use maimbrain::music::{Change, Director, Graph, Log, Mood, Setlist, Songs};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (assets, name) = match args.as_slice() {
        [a, n] => (std::path::PathBuf::from(a), n.clone()),
        _ => {
            eprintln!("usage: songs_switch <game>/assets <name>");
            std::process::exit(2);
        }
    };
    let read = |p: &str| std::fs::read(assets.join(p.trim_start_matches("assets/"))).unwrap_or_else(|e| panic!("{p}: {e}"));
    let list = Setlist::parse(&read(&format!("{name}.songs.json"))).unwrap();
    let mut songs = Songs::empty();
    let mut next_sound = 1u32;
    for (i, (zone, graph)) in list.songs.iter().enumerate() {
        let g = Graph::parse(&read(graph)).unwrap();
        let files: Vec<String> = g.files().iter().map(|f| f.to_string()).collect();
        let mut log = Log::with_ids_after(i as u32 * 100_000);
        let first = next_sound;
        for (k, f) in files.iter().enumerate() {
            log.name(Sound(first + k as u32), f.trim_start_matches("assets/").trim_end_matches(".ogg"));
        }
        next_sound += files.len() as u32;
        let sound = |p: &str| files.iter().position(|f| f == p).map(|k| Sound(first + k as u32));
        songs.add(zone, Director::with_graph(g, sound, 7, log).unwrap());
    }
    let dt = 1.0 / 60.0;
    let mut asked = false;
    songs.start(0, 0.0);
    let mut t = 0.0f64;
    while t < 50.0 {
        if !asked && t >= 24.0 && list.songs.len() > 1 {
            songs.go(1, Change::Auto, t);
            eprintln!("asked for {} at {t:.2} s; it starts at {:.2} s", list.songs[1].0, songs.change_at().unwrap_or(t));
            asked = true;
        }
        let fight = if t < 24.0 { t / 20.0 } else { (t - 30.0) / 15.0 };
        songs.update(t, dt as f32, Mood { intensity: fight.clamp(0.0, 1.0) as f32, danger: fight.clamp(0.0, 1.0) as f32 });
        t += dt;
    }
    songs.stop(t, 1.5);
    let mut lines: Vec<(f64, String)> = Vec::new();
    for i in 0..list.songs.len() {
        for l in songs.director(i).unwrap().out.lines.iter() {
            let at = l.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            lines.push((at, l.clone()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, l) in lines {
        println!("{l}");
    }
}
