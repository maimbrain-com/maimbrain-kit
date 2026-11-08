//! Phase 3d: music that follows the game (api/music.rs), on Storm Drum's
//! music graph (examples/storm-drum: two zones, layers, the tension kit).

use std::path::Path;
use std::rc::Rc;

use maimbrain::audio::Sound;
use maimbrain::music::{Director, Graph, Log, Mood};

use crate::input::Held;
use crate::project::{Files, Project};
use crate::tests::{compile, read};
use crate::vm::{Mode, Vm};

const DT: f64 = 1.0 / 60.0;

/// Storm Drum's files with main.lua replaced.
fn drum(main: &str, more: &[(&str, &str)]) -> Files {
    let mut f = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/storm-drum"));
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    for (p, s) in more {
        f.insert(p.to_string(), s.as_bytes().to_vec());
    }
    compile(&mut f);
    f
}

fn vm_seeded(files: &Files, seed: u64) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files)?);
    Vm::new(&p, seed, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0))
}

fn steps(v: &mut Vm, n: usize) {
    for _ in 0..n {
        v.step(DT as f32, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

/// Lua that logs every beat (beat, bar, its time on the grid, the frame's
/// round.time) and every bar, and prints the log.
const LOGS: &str = r#"
local log = {}
on("beat", function(beat, bar, at) log[#log + 1] = string.format("%d %d %.6f %.6f %s", beat, bar, at, round.time, music.zone or "-") end)
on("bar", function(bar, at) log[#log + 1] = string.format("bar %d %.6f", bar, at) end)
function love.draw() love.graphics.print(table.concat(log, "|"), 0, 0) end
"#;

/// (beat, bar, at, round.time, zone) per beat logged, and the bars' lines.
fn beats(v: &Vm) -> (Vec<(u32, i64, f64, f64, String)>, Vec<String>) {
    let text = printed(v).concat();
    let (mut b, mut bars) = (Vec::new(), Vec::new());
    for l in text.split('|').filter(|l| !l.is_empty()) {
        if l.starts_with("bar ") {
            bars.push(l.to_string());
            continue;
        }
        let w: Vec<&str> = l.split(' ').collect();
        b.push((w[0].parse().unwrap(), w[1].parse().unwrap(), w[2].parse().unwrap(), w[3].parse().unwrap(), w[4].to_string()));
    }
    (b, bars)
}

/// The graph's first downbeat when it starts its first zone at round time
/// `t`: what the SDK's Director makes of it, on its own.
fn origin(files: &Files, t: f64) -> (f64, f64) {
    let g = Graph::parse(&files["music/drum.music.json"]).unwrap();
    let names: Vec<String> = g.files().iter().map(|f| f.to_string()).collect();
    let mut d = Director::with_graph(g, |p| names.iter().position(|f| f == p).map(|i| Sound(i as u32 + 1)), 1, Log::default()).unwrap();
    d.start(0, t);
    let grid = d.grid().unwrap();
    (grid.origin, grid.beat)
}

#[test]
fn beats_land_on_the_graphs_tempo_map_in_game_time() {
    let files = drum(&format!("{LOGS}\nfunction love.load() music.play(\"drum\") end\n"), &[]);
    let mut v = vm_seeded(&files, 7).unwrap();
    steps(&mut v, 600);
    let (origin, beat) = origin(&files, 0.0);
    assert!((beat - 1.999898 / 4.0).abs() < 1e-9);
    let (b, bars) = beats(&v);
    assert!(b.len() >= 18, "{} beats in 10 s at 120 BPM", b.len());
    for (n, (k, bar, at, time, _)) in b.iter().enumerate() {
        let want = origin + n as f64 * beat;
        assert!((at - want).abs() < 1e-6, "beat {n} at {at}, the tempo map says {want}");
        assert_eq!((*k, *bar), ((n % 4) as u32 + 1, (n / 4) as i64 + 1), "beat {n}");
        // Heard on the first frame at or past it.
        assert!(*time >= at - 1e-9 && *time < at + DT + 1e-9, "beat {n} at {at} fired at {time}");
    }
    assert_eq!(bars.len(), b.len().div_ceil(4));
    assert!(bars[0].starts_with("bar 1 "));
    // music.time, .bar, .beat, .bpm read the same clock.
    let mut v = vm_seeded(&drum("function love.load() music.play(\"drum\") end\nfunction love.draw() love.graphics.print(string.format(\"%.4f %d %.4f %.3f %s %s\", music.time, music.bar, music.beat, music.bpm, music.zone, tostring(music.playing)), 0, 0) end\n", &[]), 7).unwrap();
    steps(&mut v, 300);
    let t = 300.0 * DT - origin;
    let k = t / beat;
    let want = format!("{t:.4} {} {:.4} {:.3} calm true", (k / 4.0).floor() as i64 + 1, k.rem_euclid(4.0) + 1.0, 60.0 / beat);
    assert_eq!(printed(&v), [want]);
}

#[test]
fn a_zone_change_lands_on_a_bar_line_and_the_beat_goes_on() {
    let main = format!("{LOGS}\nfunction love.load() music.play(\"drum\") end\non(\"bar\", function(bar) if bar == 3 then music.go(\"storm\") end end)\n");
    let files = drum(&main, &[]);
    let run = || {
        let mut v = vm_seeded(&files, 11).unwrap();
        steps(&mut v, 1500);
        beats(&v)
    };
    let (b, bars) = run();
    let (origin, beat) = origin(&files, 0.0);
    // No beat lost or doubled across the change: one grid from start to end.
    for (n, x) in b.iter().enumerate() {
        assert!((x.2 - (origin + n as f64 * beat)).abs() < 1e-6, "beat {n}");
    }
    let first = b.iter().position(|x| x.4 == "storm").expect("the storm came");
    assert_eq!(b[first].0, 1, "the zone changes on a downbeat");
    assert!(b[first].1 >= 4 && b[first].1 <= 8, "at the next phrase (bar {})", b[first].1);
    assert!(b[first..].iter().all(|x| x.4 == "storm"));
    // The same round, the same music: jumps and all.
    assert_eq!(run(), (b, bars));
}

#[test]
fn the_round_owns_the_clock_and_carried_music_starts_again_on_it() {
    // A round whose love.load asks for nothing, handed the last round's music, beats as one that asked.
    let asked = drum(&format!("{LOGS}\nfunction love.load() music.play(\"drum\", {{ volume = 0.5 }}) end\n"), &[]);
    let mut a = vm_seeded(&asked, 3).unwrap();
    let mut old = vm_seeded(&asked, 9).unwrap();
    steps(&mut old, 200);
    let carried = old.hand_over_music();
    assert_eq!(carried.as_ref().map(|c| (c.name.as_str(), c.volume)), Some(("drum", 0.5)));
    assert!(!old.host().tunes.asked.is_some(), "the old round let it go");
    let mut b = vm_seeded(&drum(LOGS, &[]), 3).unwrap();
    b.carry_music(carried);
    steps(&mut a, 400);
    steps(&mut b, 400);
    assert_eq!(beats(&a), beats(&b));
    // A round that plays something else isn't handed anything.
    let mut c = vm_seeded(&drum(&format!("{LOGS}\nfunction love.load() music.play(\"drum\", {{ zone = \"storm\" }}) end\n"), &[]), 3).unwrap();
    c.carry_music(Some(crate::api::music::Asked { name: "drum".into(), zone: None, volume: 0.7 }));
    steps(&mut c, 200);
    assert!(beats(&c).0.iter().all(|x| x.4 == "storm"));
}

#[test]
fn a_setlist_changes_song_on_a_cue_and_its_grid_with_it() {
    let list = r#"{ "format": "maimbrain-songs", "version": 1, "name": "fight",
      "songs": [ { "zone": "harbor", "graph": "music/drum.music.json" }, { "zone": "neon", "graph": "music/drum.music.json" } ] }"#;
    let main = format!("{LOGS}\nfunction love.load() music.play(\"fight\") end\non(\"bar\", function(bar) if bar == 2 then music.go(\"neon\") end end)\n");
    let mut v = vm_seeded(&drum(&main, &[("music/fight.songs.json", list)]), 5).unwrap();
    steps(&mut v, 1200);
    let (b, _) = beats(&v);
    let first = b.iter().position(|x| x.4 == "neon").expect("the next song");
    assert_eq!(b[first].0, 1, "a song starts on its downbeat");
    // The new song's grid starts on the cue: beats a beat apart on either side of it.
    for w in b.windows(2) {
        let gap = w[1].2 - w[0].2;
        assert!((gap - 1.999898 / 4.0).abs() < 1e-3, "{w:?}");
    }
    assert!(b.windows(2).all(|w| w[1].1 >= w[0].1), "bars count on across songs");
}

#[test]
fn mood_go_and_play_say_what_they_take() {
    let err = |main: &str| {
        let mut v = match vm_seeded(&drum(main, &[]), 1) {
            Err(e) => return e,
            Ok(v) => v,
        };
        v.step(DT as f32, &[], &Held::default(), false, false).expect_err("an error")
    };
    assert!(err("music.play(\"drum\", { zone = \"cave\" })").contains("has no zone \"cave\" (its zones: calm, storm)"));
    assert!(err("music.play(\"drum\")\nmusic.go(\"cave\")").contains("no zone \"cave\" (the music's zones: calm, storm)"));
    assert!(err("music.play(\"drum\")\nmusic.go(\"storm\", { change = \"soon\" })").contains("\"auto\", \"nextBar\", \"stinger\" or \"crossfade\""));
    assert!(err("music.mood{ danger = \"high\" }").contains("numbers from 0 to 1"));
    assert!(err("music.play(\"nope\")").contains("no music \"nope\""));
    // A graph that names a file the project hasn't got is the project's error.
    let mut f = drum("", &[]);
    f.remove("music/drum_swell.ogg");
    assert!(Project::parse(&f).unwrap_err().contains("music/drum.music.json plays music/drum_swell.ogg"));
    // Mood is kept between calls, field by field.
    let mut v = vm_seeded(&drum("music.play(\"drum\")\nmusic.mood{ intensity = 0.4 }\nmusic.mood{ danger = 2 }", &[]), 1).unwrap();
    steps(&mut v, 1);
    assert_eq!(v.host().tunes.mood, Mood { intensity: 0.4, danger: 1.0 });
}

/// Storm Drum's music as a round plays it, written as the skill's
/// `mix_check.py` event log (render it to a WAV with `--out`): calm for
/// eight bars as the danger rises and falls, then the storm, a near miss,
/// and the round's end. `MB_MUSIC_LOG=<file>` writes it; otherwise it only
/// checks that every join of the take lands on a bar line.
#[test]
fn storm_drums_mixdown_joins_on_bar_lines() {
    let f = drum("", &[]);
    let g = Graph::parse(&f["music/drum.music.json"]).unwrap();
    let names: Vec<String> = g.files().iter().map(|f| f.to_string()).collect();
    let mut log = Log::default();
    for (i, n) in names.iter().enumerate() {
        log.name(Sound(i as u32 + 1), n.trim_start_matches("music/").trim_end_matches(".ogg"));
    }
    let mut d = Director::with_graph(g, |p| names.iter().position(|f| f == p).map(|i| Sound(i as u32 + 1)), 7, log).unwrap();
    d.start(0, 0.0);
    let (origin, bar) = (d.origin(), d.bar());
    let mut t = 0.0;
    let mut asked = false;
    while t < 70.0 {
        let k = (t - origin) / bar;
        // Danger swells over bars 3–6 and lets go; the storm from bar 8; a near miss in it.
        let danger = if (3.0..6.0).contains(&k) { ((k - 3.0) / 2.0).min(1.0) } else if k > 12.0 { 0.6 } else { 0.0 };
        if k >= 8.0 && !asked {
            d.go(d.zone("storm").unwrap(), maimbrain::music::Change::Auto, t);
            asked = true;
        }
        if (t - 40.0).abs() < DT / 2.0 {
            d.near_miss(t);
        }
        d.update(t, DT as f32, Mood { intensity: (k / 20.0).clamp(0.0, 1.0) as f32, danger: danger as f32 });
        t += DT;
    }
    d.stop(t, 1.0);
    for m in d.moves() {
        let bars = (m.at - origin) / bar;
        assert!((bars - bars.round()).abs() < 1e-6, "{m:?} isn't on a bar line");
    }
    assert!(d.moves().iter().any(|m| !matches!(m.kind, maimbrain::music::MoveKind::Link(_))), "the storm came: {:?}", d.moves());
    if let Ok(path) = std::env::var("MB_MUSIC_LOG") {
        let mut text = format!("# storm-drum: origin {origin:.6} bar {bar:.6}\n");
        for m in d.moves() {
            text.push_str(&format!("# move {:.6} {} {} {:?}\n", m.at, m.from, m.to, m.kind));
        }
        text.push_str(&d.out.text());
        std::fs::write(path, text).unwrap();
    }
}

/// The beats of mb-cli's `mb_play_lands_beats_on_the_tempo_map` (the same
/// Lua, 30 s), natively: `MB_BEATS_OUT=<file>` writes them, one a line, to
/// compare with what `mb play` printed in V8. They're the same lines.
#[test]
fn the_beats_mb_play_checks_natively() {
    let main = "local log = {}\non(\"beat\", function(beat, bar, at) log[#log + 1] = string.format(\"beat %d %d %.6f %.6f\", beat, bar, at, round.time) end)\non(\"bar\", function(bar) if bar == 3 then music.go(\"storm\") end end)\nfunction love.load() music.play(\"drum\") end\nfunction love.draw() love.graphics.print(table.concat(log, \"\\n\"), 0, 0) end\n";
    let mut v = vm_seeded(&drum(main, &[]), 1).unwrap();
    steps(&mut v, 1800);
    let text = printed(&v).concat();
    assert_eq!(text.lines().next(), Some("beat 1 1 0.620000 0.633333"));
    if let Ok(path) = std::env::var("MB_BEATS_OUT") {
        std::fs::write(path, text + "\n").unwrap();
    }
}
