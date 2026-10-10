//! The tension kit is heard on a phone over a real take, as the SDK's
//! `music::Director` plays it by default, measured the way the skill's
//! mix_check.py does: so the levels `mb music` writes into a music graph and
//! the Director's defaults can't drift apart from what mix_check passes.
//!
//! The take is 24.5 s of a Lyria chase take (`mb music … --zones`, the
//! music-demo's `run_take_base` + `run_take_hi`, mono at 22.05 kHz: bars
//! 37–49 of the take, a stretch a little louder on a phone than its median),
//! in `testdata/chase_take_excerpt.ogg`. The kit is synthesized for it here
//! as `mb music` does. The Director's event log is rendered as mix_check.py
//! renders one (each voice from where it starts in its sound, its fade-in,
//! its level changes and its release), and the tension's lift is mix_check's:
//! the music's power over 3 s with and without the tension, K-weighted and
//! through a phone speaker.

use ::maimbrain::audio::Sound;
use ::maimbrain::music::{Director, Graph, Levels, Log, Mood};

use super::audio;
use super::score::PREROLL;
use super::tension::{self, DEFAULT_LEVELS, Grid, Kit};

const BPM: f64 = 120.005;
const BAR: f64 = 1.999921;
/// Where bar 0 starts in the excerpt, and how many bars it holds.
const FIRST_BAR: f64 = 0.25;
const BARS: u32 = 12;
/// mix_check.py's rules: the tension's lift on a phone at its peak, and at its quietest.
const RULE_TENSION_RISE: f64 = 3.0;
const RULE_TENSION_FLOOR: f64 = 1.0;

fn take() -> (Vec<f32>, u32) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/chase_take_excerpt.ogg")).unwrap();
    let (ch, rate) = audio::decode(&bytes, "audio/ogg").unwrap();
    (audio::to_mono(&ch), rate)
}

fn kit(take: &[f32], rate: u32) -> Kit {
    // As `mb music --zones … --layers …,tension` makes it: 8 bars on the take's grid, in its key.
    let grid = Grid { len: (8.0 * BAR * rate as f64).round() as usize, rate, beats: 32, phase: (PREROLL * rate as f32) as usize };
    tension::kit(&grid, audio::detect_key(take, rate), 7)
}

/// A one-zone graph of the excerpt, with the levels `mb music` would write (None: an older graph).
fn graph(gain: Option<f32>) -> Graph {
    let exit = FIRST_BAR + BARS as f64 * BAR;
    let levels = gain.map(|g| format!(r#""levels": {{ "tension_gain": {g} }},"#)).unwrap_or_default();
    let json = format!(
        r#"{{
      "format": "maimbrain-music-graph", "version": 1, "name": "run", {levels}
      "files": {{ "take": "take",
                  "tension": {{ "layer": "tension", "tick": "tick", "heartbeat": "heartbeat", "sting": "sting", "bars": 8, "downbeat_seconds": {PREROLL} }} }},
      "grid": {{ "bpm": {BPM}, "beats_per_bar": 4, "bar_seconds": {BAR}, "first_bar_seconds": {FIRST_BAR}, "phrase_bars": 4, "first_bar": 0, "end_bar": {BARS} }},
      "zones": [ {{ "name": "chase", "sections": [0] }} ],
      "sections": [ {{ "name": "chase", "zone": 0, "entry_bar": 0, "exit_bar": {BARS}, "exit_cues": [4, 8, 12],
                      "segment": {{ "pre_entry_seconds": {pre}, "entry_seconds": {FIRST_BAR}, "exit_seconds": {exit}, "post_exit_seconds": {post} }} }} ],
      "links": []
    }}"#,
        pre = FIRST_BAR - BAR / 16.0,
        post = exit + 0.2,
    );
    Graph::parse(json.as_bytes()).unwrap()
}

const NAMES: [&str; 5] = ["take", "tension", "tick", "heartbeat", "sting"];

/// A chase: calm, the threat closes in, full danger for half a minute, the escape.
fn danger(t: f64) -> f32 {
    match t {
        t if t < 4.0 => 0.0,
        t if t < 8.0 => ((t - 4.0) / 4.0) as f32,
        t if t < 40.0 => 1.0,
        _ => 0.0,
    }
}
const FULL: (f64, f64) = (10.0, 40.0);
const END: f64 = 52.0;

/// The Director's log over the chase, with `levels` (None: what it picks for the graph).
fn chase(g: Graph, levels: Option<Levels>) -> String {
    let mut log = Log::default();
    for (i, n) in NAMES.iter().enumerate() {
        log.name(Sound(i as u32 + 1), n);
    }
    let mut d = Director::with_graph(g, |p| NAMES.iter().position(|n| *n == p).map(|i| Sound(i as u32 + 1)), 3, log).unwrap();
    if let Some(l) = levels {
        d.levels = l;
    }
    d.start(0, 0.0);
    let mut t = 0.0;
    while t < END {
        d.update(t, 1.0 / 60.0, Mood { intensity: 0.5, danger: danger(t) });
        t += 1.0 / 60.0;
    }
    d.out.text()
}

/// A voice as mix_check.py schedules it.
struct Voice {
    sound: usize,
    start: f64,
    end: f64,
    looped: bool,
    from: f64,
    fade_in: f64,
    release: Option<(f64, f64)>,
    /// (time, vol, pitch), from the start.
    segs: Vec<(f64, f64, f64)>,
}

fn schedule(log: &str, lens: &[f64]) -> Vec<Voice> {
    let mut vs: Vec<Voice> = Vec::new();
    let mut ids: Vec<(u32, usize)> = Vec::new();
    for line in log.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let w: Vec<&str> = line.split_whitespace().collect();
        let (t, sound, vol, pitch): (f64, usize, f64, f64) = (w[0].parse().unwrap(), NAMES.iter().position(|n| *n == w[1]).unwrap(), w[2].parse().unwrap(), w[3].parse().unwrap());
        let op = w.get(4).filter(|x| !x.contains('=')).copied().unwrap_or("play");
        let kv = |k: &str| w.iter().find_map(|x| x.strip_prefix(k)?.strip_prefix('=')?.parse::<f64>().ok());
        let id = kv("id").unwrap() as u32;
        match op {
            "play" | "loop" => {
                let (looped, from) = (op == "loop", kv("from").unwrap_or(0.0));
                let end = if looped { f64::INFINITY } else { t + (lens[sound] - from).max(0.0) / pitch };
                ids.push((id, vs.len()));
                vs.push(Voice { sound, start: t, end, looped, from, fade_in: kv("fade").unwrap_or(0.0), release: None, segs: vec![(t, vol, pitch)] });
            }
            "set" | "stop" => {
                let Some(&(_, i)) = ids.iter().find(|x| x.0 == id) else { continue };
                let v = &mut vs[i];
                if op == "set" {
                    let p = v.segs.last().unwrap().2;
                    v.segs.push((t, vol, p));
                } else if let Some(f) = kv("fade").filter(|f| *f > 0.0) {
                    v.release = Some((t, f));
                    v.end = v.end.min(t + f);
                } else {
                    v.end = v.end.min(t);
                }
            }
            _ => panic!("{line}"),
        }
    }
    vs
}

/// Renders the voices of `sounds` (K-weighted, at `rate`) into one buffer of `n` samples.
fn render(vs: &[Voice], which: &[usize], src: &[Vec<f32>], rate: u32, n: usize) -> Vec<f32> {
    let r = rate as f64;
    let mut out = vec![0f32; n];
    for v in vs.iter().filter(|v| which.contains(&v.sound)) {
        let x = &src[v.sound];
        let len = x.len() as f64;
        let i0 = (v.start * r).round() as usize;
        let i1 = n.min((v.end.min(END) * r).round() as usize);
        let mut pos = v.from * r;
        let mut seg = 0;
        for i in i0..i1 {
            let t = i as f64 / r;
            while seg + 1 < v.segs.len() && v.segs[seg + 1].0 <= t {
                seg += 1;
            }
            let (_, vol, pitch) = v.segs[seg];
            let q = if v.looped {
                pos.rem_euclid(len)
            } else if pos >= len - 1.0 {
                break;
            } else {
                pos
            };
            let (k, f) = (q as usize, (q - q.floor()) as f32);
            let s = x[k] + (x[(k + 1) % x.len()] - x[k]) * f;
            let j = (i - i0) as f64;
            let mut env = if v.fade_in > 0.0 && j < v.fade_in * r { (std::f64::consts::FRAC_PI_2 * j / (v.fade_in * r)).sin() } else { 1.0 };
            if let Some((t0, f)) = v.release
                && t >= t0
            {
                env *= (-(t - t0) / (f / 6.9)).exp();
            }
            out[i] += (vol * env) as f32 * s;
            pos += pitch;
        }
    }
    out
}

/// mix_check.py's tension rise on a phone: per 0.1 s block, over the 3 s up to it, how much the
/// tension (layer, clock, heartbeat) lifts the take. (time of the block's end, dB)
fn rise(log: &str, take: &[f32], kit: &Kit, rate: u32) -> Vec<(f64, f64)> {
    let k = |x: &[f32]| {
        let [a, b] = audio::k_filters(rate);
        audio::filter(x, &[a, b])
    };
    let src: Vec<Vec<f32>> = vec![k(take), k(&kit.tension), k(&kit.tick), k(&kit.heartbeat), k(&kit.sting)];
    let lens: Vec<f64> = src.iter().map(|x| x.len() as f64).collect();
    let vs = schedule(log, &lens);
    let n = (END * rate as f64) as usize;
    // The phone's roll-off on the summed (already K-weighted) music and tension: linear, so as mix_check does it per sound.
    let hp = |x: Vec<f32>| {
        let f = |q| audio::Biquad::highpass(rate, audio::PHONE_HP, q);
        audio::filter(&x, &[f(audio::BUTTER4_Q[0]), f(audio::BUTTER4_Q[1])])
    };
    let core = tension::block_power(&hp(render(&vs, &[0], &src, rate, n)), rate);
    let tens = tension::block_power(&hp(render(&vs, &[1, 2, 3], &src, rate, n)), rate);
    let (mut ac, mut at) = (0.0, 0.0);
    let mut out = Vec::new();
    for i in 0..core.len() {
        ac += core[i];
        at += tens[i];
        if i >= 30 {
            ac -= core[i - 30];
            at -= tens[i - 30];
        }
        if i >= 30 && ac > 1e-12 {
            out.push(((i + 1) as f64 * 0.1, 10.0 * ((ac + at) / ac).log10()));
        }
    }
    out
}

struct Report {
    /// mix_check.py's: the most and the least the tension lifts the music over the round.
    peak: f64,
    low: f64,
    /// Over the 3 s windows wholly at full danger: the most, the median, and the share of them over +3 dB.
    full_peak: f64,
    full_median: f64,
    full_heard: f64,
}

fn report(r: &[(f64, f64)]) -> Report {
    let mut full: Vec<f64> = r.iter().filter(|x| x.0 - 3.0 >= FULL.0 && x.0 <= FULL.1).map(|x| x.1).collect();
    full.sort_by(f64::total_cmp);
    Report {
        peak: r.iter().map(|x| x.1).fold(f64::MIN, f64::max),
        low: r.iter().map(|x| x.1).fold(f64::MAX, f64::min),
        full_peak: full[full.len() - 1],
        full_median: full[full.len() / 2],
        full_heard: full.iter().filter(|&&x| x >= RULE_TENSION_RISE).count() as f64 / full.len() as f64,
    }
}

/// What mix_check.py asks, with margin: at full danger the tension lifts the music by a clear
/// step on a phone at its best (mix_check's rule, over the round and over its full-danger
/// stretch alone), by +4 dB over a typical 3 s, and by +3 over at least three 3 s in four; and it
/// lets go when the danger is gone.
fn assert_heard(r: &Report, what: &str) {
    assert!(r.peak >= RULE_TENSION_RISE && r.full_peak >= RULE_TENSION_RISE, "{what}: mix_check: the tension lifts the music only {:+.1} dB on a phone at its peak (want >= +3)", r.full_peak);
    assert!(r.low <= RULE_TENSION_FLOOR, "{what}: mix_check: the tension never lets go ({:+.1} dB at its quietest)", r.low);
    assert!(r.full_median >= RULE_TENSION_RISE + 1.0, "{what}: at full danger it lifts a typical 3 s by only {:+.1} dB", r.full_median);
    assert!(r.full_heard >= 0.75, "{what}: at full danger only {:.0}% of it is lifted by +3 dB", r.full_heard * 100.0);
}

#[test]
fn mb_music_levels_are_the_sdks() {
    let l = Levels::default();
    assert_eq!(DEFAULT_LEVELS, (l.tension_max, l.tick_max, l.heartbeat.1), "tension::DEFAULT_LEVELS must match music::Levels::default()");
    assert_eq!(tension::MAX_GAIN * l.tension_max, ::maimbrain::music::MAX_VOL);
}

#[test]
fn the_tension_is_heard_on_a_phone_over_a_real_take() {
    let (x, rate) = take();
    let kit = kit(&x, rate);
    let lift = tension::lift(&x, &kit, rate, BAR);
    // What `mb music` writes into the graph, played by the Director as it is by default.
    let r = report(&rise(&chase(graph(Some(lift.gain)), None), &x, &kit, rate));
    eprintln!(
        "gain ×{:.2} (aim {:+.1} dB, {:+.1} dB at the defaults): peak {:+.1} dB, low {:+.1} dB; at full danger: best {:+.1}, median {:+.1}, {:.0}% over +3",
        lift.gain,
        lift.db,
        lift.default_db,
        r.peak,
        r.low,
        r.full_peak,
        r.full_median,
        r.full_heard * 100.0
    );
    assert!(lift.gain > 1.0, "this take needs the kit louder than the defaults: {lift:?}");
    assert_heard(&r, "the demo take");
    // The estimate `mb music` makes agrees with the render.
    assert!((r.full_median - lift.db as f64).abs() < 1.0, "estimated {:+.1}, rendered {:+.1}", lift.db, r.full_median);
}

#[test]
fn a_louder_take_gets_a_louder_kit() {
    // The same take 6 dB hotter against the kit (a loud, bright Lyria take): the defaults
    // fall short, as a creator found; the gain mb music writes brings the tension back.
    let (x, rate) = take();
    let kit = kit(&x, rate);
    let hot: Vec<f32> = x.iter().map(|v| v * 2.0).collect();
    let lift = tension::lift(&hot, &kit, rate, BAR);
    assert!(lift.gain > 2.0 && lift.default_db < 2.0, "{lift:?}");
    let defaults = report(&rise(&chase(graph(None), Some(Levels::default())), &hot, &kit, rate));
    assert!(defaults.full_peak < RULE_TENSION_RISE, "the defaults: {:+.1} dB", defaults.full_peak);
    let r = report(&rise(&chase(graph(Some(lift.gain)), None), &hot, &kit, rate));
    eprintln!("hot take: gain ×{:.2}: at full danger best {:+.1}, median {:+.1}; the defaults: best {:+.1}", lift.gain, r.full_peak, r.full_median, defaults.full_peak);
    assert_heard(&r, "a take 6 dB hotter");
}
