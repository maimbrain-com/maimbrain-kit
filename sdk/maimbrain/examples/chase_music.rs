//! A chase through three zones with `music::Director`, written down as a
//! `mix_check.py` event log instead of played: the Director's decisions are
//! the same either way (swap `Log` for `Live` in a game).
//!
//! The music is `mb music … --zones "…" --variations 2 --layers base,hi,tension`.
//! With its music graph (the default), the take (`_take_base` + `_take_hi`),
//! the stingers, the swell and the tension kit are named in the graph:
//!
//!     cargo run -p maimbrain --example chase_music -- <game>/assets/run.music.json 7 > chase.log
//!     python3 <skill>/scripts/mix_check.py <game>/assets chase.log --out chase.wav
//!
//! (the second argument is the seed). The moves it made through the graph go
//! to stderr. With `--loops`, per zone `<name>_<zone>_1` and `_2` (each as
//! `_base` + `_hi`), a stinger `<name>_into_<zone>_1`, the swell and the kit;
//! values from the sidecars (`bpm`, `bars`, `downbeat_seconds`, the stingers'
//! `land_seconds`; the swell's is one beat):
//!
//!     cargo run -p maimbrain --example chase_music -- run 125 8 0.01 0.96 lantern_town,bamboo_grove,lotus_marsh > chase.log
//!
//! 75 s: calm in Lantern Town while the heron is far off; it closes in;
//! into the Bamboo Grove (asked for at 21 s); a near-catch at 33 s; easing
//! off; into the Lotus Marsh (asked for at 44 s); it closes in again, a
//! second near-catch at 57 s, and the escape.

use maimbrain::audio::Sound;
use maimbrain::music::{Change, Director, Graph, Log, Mood, Out, Tension};

/// The game: which zone the player is in, how far behind the heron is, how fast it's going.
fn zone_at(t: f64) -> usize {
    if t < 21.0 {
        0
    } else if t < 44.0 {
        1
    } else {
        2
    }
}

fn danger_at(t: f64) -> f32 {
    let ramp = |a: f64, b: f64, from: f32, to: f32| from + (to - from) * (((t - a) / (b - a)).clamp(0.0, 1.0) as f32);
    match t {
        t if t < 8.0 => 0.0,
        t if t < 20.0 => ramp(8.0, 20.0, 0.0, 0.6),
        t if t < 33.0 => ramp(24.0, 32.0, 0.6, 1.0), // closing in; the heron strikes at 33 s
        t if t < 36.0 => 1.0,
        t if t < 46.0 => ramp(36.0, 40.0, 1.0, 0.45), // it misses, the player pulls away a little
        t if t < 57.5 => ramp(48.0, 56.0, 0.45, 1.0), // closing in again; a second strike at 57 s
        t if t < 59.5 => 1.0,
        _ => 0.0, // the escape
    }
}

fn intensity_at(t: f64) -> f32 {
    (0.2 + t as f32 / 60.0).min(0.9)
}

/// Runs the chase: `go(zone)` when the player enters one.
fn chase<O: Out>(d: &mut Director<O>, ids: &[usize], mut go: impl FnMut(&mut Director<O>, usize, f64)) {
    let dt = 1.0 / 60.0;
    let mut now = 0.0f64;
    d.start(ids[0], now);
    go(d, 0, now);
    let mut zone = 0;
    let mut strikes = [33.0, 57.0].into_iter().peekable();
    while now < 75.0 {
        let z = zone_at(now).min(ids.len() - 1);
        if z != zone {
            zone = z;
            go(d, z, now);
        }
        if strikes.peek().is_some_and(|&s| now >= s) {
            strikes.next();
            d.near_miss(now);
        }
        d.update(now, dt as f32, Mood { intensity: intensity_at(now), danger: danger_at(now) });
        now += dt;
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, d: &str| a.get(i).cloned().unwrap_or_else(|| d.to_string());
    let first = arg(0, "run");
    // Sound handles: in a game, `Sound::new(asset)` once each has loaded.
    let mut log = Log::default();
    let mut next = 0u32;
    let mut sound = |log: &mut Log, n: &str| {
        next += 1;
        log.name(Sound(next), n);
        Sound(next)
    };

    if first.ends_with(".music.json") {
        let graph = Graph::parse(&std::fs::read(&first).expect("the graph")).expect("a music graph");
        let seed: u64 = arg(1, "7").parse().unwrap();
        let zones: Vec<String> = arg(2, "lantern_town,bamboo_grove,lotus_marsh").split(',').map(str::to_string).collect();
        // The log names each sound by its file name, as mix_check.py finds it in assets/.
        let files: Vec<(String, Sound)> = graph
            .files()
            .iter()
            .map(|f| {
                let n = f.rsplit('/').next().unwrap().trim_end_matches(".ogg").to_string();
                (f.to_string(), sound(&mut log, &n))
            })
            .collect();
        let mut d = Director::with_graph(graph, |p| files.iter().find(|f| f.0 == p).map(|f| f.1), seed, log).expect("the take");
        let ids: Vec<usize> = zones.iter().map(|z| d.zone(z).unwrap_or_else(|| panic!("no zone {z}"))).collect();
        chase(&mut d, &ids, |d, z, now| {
            if now > 0.0 {
                d.go(ids[z], Change::Auto, now);
            }
        });
        let g = d.graph().unwrap();
        for m in d.moves() {
            let zone = |b: u32| g.zone_of(b).map_or("-".to_string(), |z| g.zones[z].name.clone());
            eprintln!("{:7.3}  bar {:2} ({}) → {:2} ({}): {:?}", m.at, m.from, zone(m.from), m.to, zone(m.to), m.kind);
        }
        print!("{}", d.out.text());
        return;
    }

    let name = first;
    let bpm: f64 = arg(1, "125").parse().unwrap();
    let bars: u32 = arg(2, "8").parse().unwrap();
    let downbeat: f64 = arg(3, "0.01").parse().unwrap();
    let land: f32 = arg(4, "0.96").parse().unwrap();
    let zones: Vec<String> = arg(5, "lantern_town,bamboo_grove,lotus_marsh").split(',').map(str::to_string).collect();
    let mut parts = Vec::new(); // per zone: [variation 1, variation 2], with the stinger into 1
    let mut stingers = Vec::new();
    for z in &zones {
        let mut v = Vec::new();
        for k in 1..=2 {
            let base = sound(&mut log, &format!("{name}_{z}_{k}_base"));
            let hi = sound(&mut log, &format!("{name}_{z}_{k}_hi"));
            v.push((base, hi));
        }
        parts.push(v);
        stingers.push(sound(&mut log, &format!("{name}_into_{z}_1")));
    }
    let t = Tension {
        layer: Some(sound(&mut log, &format!("{name}_tension"))),
        tick: Some(sound(&mut log, &format!("{name}_tick"))),
        heartbeat: Some(sound(&mut log, &format!("{name}_heartbeat"))),
        sting: Some(sound(&mut log, &format!("{name}_sting"))),
    };
    let swell = sound(&mut log, &format!("{name}_swell"));

    let mut d = Director::with_out(bpm, bars, log);
    d.downbeat(downbeat);
    let ids: Vec<[usize; 2]> = parts
        .iter()
        .zip(&stingers)
        .map(|(v, &st)| {
            let a = d.add(v[0].0, Some(v[0].1));
            let b = d.add(v[1].0, Some(v[1].1));
            d.stinger(a, st, land);
            [a, b]
        })
        .collect();
    d.tension(t);
    d.swell(swell, (60.0 / bpm) as f32);
    let firsts: Vec<usize> = ids.iter().map(|v| v[0]).collect();
    chase(&mut d, &firsts, |d, z, now| {
        if now > 0.0 {
            d.go(ids[z][0], Change::Auto, now);
        }
        d.rotate(&ids[z], 4, now);
    });
    print!("{}", d.out.text());
}
