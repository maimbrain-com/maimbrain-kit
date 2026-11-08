//! `mb studio-asset` for an engine 2.0 project's music that follows the game
//! (docs/STUDIO-2.md §7.2, Phase 3d): what `mb music --zones`, `--sections`,
//! `--zone` and `--layers` make, from takes the Maimbrain server made.
//!
//! - `--zones "town: lanterns; grove: bamboo"` or `--sections a,b,c`: one
//!   long take in sections (Lyria 3.5, Decided 52), cut into a music graph,
//!   `music/<name>.music.json`, with the take (or its `_take_base` and
//!   `_take_hi` layers), a stinger into each zone, the swell, and with
//!   `--layers …,tension` the tension kit.
//! - `--setlist "harbor: bluesy rock, 128 bpm; neon: flamenco"`: a song per
//!   zone, each its own take and graph (`music/<name>_<zone>.music.json`),
//!   listed in `music/<name>.songs.json`. A candidate is its songs' takes,
//!   comma-separated in the setlist's order.
//! - `--layers` on a plain loop: `music/<name>_base.ogg` and `_hi.ogg` (and
//!   the tension kit), and `music/<name>.music.json` saying they're one loop
//!   (`"loop": true`), so the engine's Director plays them in step.
//!
//! `--plan` prints the prompt and length of each take to ask for, as JSON:
//! `{ kind, name, model, takes: [{ zone, prompt, seconds }] }` (forms only;
//! a plain loop's prompt is the server's).
//!
//! The report: `{ kind, name, candidates: [{ files: [{ file, path }], main,
//! listen, seconds, warnings }] }`. `main` is what the Lua names (the graph
//! or the setlist), `listen` a full mix of the take for the judge and the
//! card (not a project file), and every file has its `.gen.json` beside it.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::asset::AssetArgs;
use crate::generate::sound::{self, SoundJob};
use crate::generate::{audio, score};

/// The form asked for.
enum Form {
    /// Sections or zones of one take (stingers into each).
    Take(Vec<score::Section>),
    /// A song per zone: (zone, prompt).
    Setlist(Vec<(String, String)>),
    /// A plain loop, split into layers.
    Loop,
}

/// Whether this music job is this module's: a form, or layers.
pub fn wanted(a: &AssetArgs) -> bool {
    a.zones.is_some() || a.sections.is_some() || a.setlist.is_some() || a.layers.is_some()
}

fn form(a: &AssetArgs) -> Result<Form, String> {
    let n = [a.zones.is_some(), a.sections.is_some(), a.setlist.is_some()].iter().filter(|x| **x).count();
    if n > 1 {
        return Err("--zones, --sections and --setlist are three different forms: give one".into());
    }
    if let Some(z) = &a.zones {
        return Ok(Form::Take(score::parse_zones(z)?));
    }
    if let Some(s) = &a.sections {
        let v = score::parse_sections(s)?;
        if v.len() < 2 {
            return Err("--sections: name two or more".into());
        }
        return Ok(Form::Take(v));
    }
    if let Some(l) = &a.setlist {
        let v: Vec<(String, String)> = score::parse_zones(l)?.into_iter().map(|s| (s.name, s.desc)).collect();
        return Ok(Form::Setlist(v));
    }
    Ok(Form::Loop)
}

/// A job as `mb music` would make it, for one take.
fn job(a: &AssetArgs, name: &str, subject: &str, out: &str) -> Result<SoundJob, String> {
    let (layers, tension) = match &a.layers {
        Some(l) => sound::parse_layers(Some(l))?,
        None => (false, false),
    };
    Ok(SoundJob {
        kind: "music".into(),
        name: name.into(),
        subject: subject.into(),
        out: out.into(),
        provider: a.provider.clone(),
        model: a.model.clone(),
        seed: a.seed,
        bars: None,
        bpm: a.bpm,
        seconds: None,
        layers,
        tension,
        key: None,
        brighten: false,
        split_hz: 700.0,
        once: false,
        looping: true,
        negative: None,
        sections: Vec::new(),
        stingers: false,
        retries: 0,
        section_seconds: None,
        loop_xfade_bars: None,
        downbeat_shift: None,
        loops: false,
        section_clips: Default::default(),
        fix_below: None,
        rate: 44_100,
        quality: 4.0,
        no_style: true,
        allow_names: Vec::new(),
    })
}

/// Each take of the form: (zone for a setlist, its job).
fn takes(a: &AssetArgs, form: &Form) -> Result<Vec<(Option<String>, SoundJob)>, String> {
    let name = &a.name;
    Ok(match form {
        Form::Take(sections) => {
            let mut j = job(a, name, &a.prompt, &format!("music/{name}.ogg"))?;
            j.sections = sections.clone();
            j.stingers = true;
            vec![(None, j)]
        }
        Form::Setlist(zones) => {
            let shared = a.prompt.trim();
            let mut v = Vec::new();
            for (zone, prompt) in zones {
                let song = format!("{name}_{zone}");
                let subject = if shared.is_empty() { prompt.clone() } else { format!("{prompt}. {shared}") };
                let mut j = job(a, &song, &subject, &format!("music/{song}.ogg"))?;
                // A song plays on its own for a whole fight: 16 bars a section gives its graph room to wander (mb music --zone).
                j.bars = Some(16);
                j.bpm = sound::tempo_in(prompt).or(a.bpm);
                j.sections = vec![score::Section { name: zone.clone(), desc: prompt.clone(), variation_of: None }];
                j.stingers = true;
                v.push((Some(zone.clone()), j));
            }
            v
        }
        Form::Loop => vec![(None, job(a, name, &a.prompt, &format!("music/{name}.ogg"))?)],
    })
}

/// `--plan`: each take's prompt and length.
pub fn plan(a: &AssetArgs) -> Result<Value, String> {
    let form = form(a)?;
    if matches!(form, Form::Loop) {
        return Err("--plan for music is for --zones, --sections and --setlist (a plain loop's prompt is the server's)".into());
    }
    let takes: Vec<Value> = takes(a, &form)?
        .into_iter()
        .map(|(zone, j)| {
            let prompt = sound::compose(&j, &crate::generate::style::Style::default());
            json!({ "zone": zone, "prompt": prompt, "seconds": (sound::plan_of(&j).total * 10.0).round() / 10.0 })
        })
        .collect();
    Ok(json!({ "kind": "music", "name": a.name, "model": "lyria-3.5", "takes": takes }))
}

fn mime_of(file: &Path) -> Result<&'static str, String> {
    Ok(match file.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        x => return Err(format!("{}: music comes as .wav, .mp3, .ogg or .flac, not .{x}", file.display())),
    })
}

/// Every option: its takes made into the game's files under `<out>/<n>/`.
pub fn process(a: &AssetArgs) -> Result<Value, String> {
    let form = form(a)?;
    let jobs = takes(a, &form)?;
    // Each option's provider calls, a take each (`--made`), for its sidecars.
    let calls = super::asset::Made::read(a)?;
    let mut made = Vec::new();
    for (i, c) in a.candidates.iter().enumerate() {
        let n = i + 1;
        let files: Vec<PathBuf> = c.split(',').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
        if files.len() != jobs.len() {
            return Err(format!("candidate {n} has {} take(s); this form needs {} (a setlist's songs, comma-separated in order)", files.len(), jobs.len()));
        }
        let dir = a.out.join(n.to_string());
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (file, (_, j)) in files.iter().zip(&jobs) {
            let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let raw = format!("{} (not kept: made by the Maimbrain server)", file.file_name().and_then(|f| f.to_str()).unwrap_or("raw"));
            sound::execute(&dir, j, false, f64::MAX, Some((bytes, mime_of(file)?.to_string(), raw)))?;
        }
        let main = match &form {
            Form::Setlist(zones) => {
                let songs: Vec<Value> = zones
                    .iter()
                    .map(|(zone, _)| {
                        let song = format!("{}_{zone}", a.name);
                        let facts = take_sidecar(&dir, &song).map(|s| s.result).unwrap_or_default();
                        json!({ "zone": zone, "graph": format!("music/{song}.music.json"), "bpm": facts["bpm"], "key": facts["key"] })
                    })
                    .collect();
                let doc = json!({ "format": "maimbrain-songs", "version": 1, "name": a.name, "songs": songs });
                let path = format!("music/{}.songs.json", a.name);
                std::fs::write(dir.join(&path), serde_json::to_string_pretty(&doc).unwrap() + "\n").map_err(|e| format!("{path}: {e}"))?;
                path
            }
            Form::Loop => loop_graph(a, &dir)?,
            Form::Take(_) => format!("music/{}.music.json", a.name),
        };
        made.push(option(a, &calls, &dir, n, &main, &form)?);
    }
    Ok(json!({ "kind": "music", "name": a.name, "candidates": made }))
}

/// The sidecar of a song's take (or its base layer).
fn take_sidecar(dir: &Path, song: &str) -> Option<crate::generate::Sidecar> {
    [format!("music/{song}_take.ogg"), format!("music/{song}_take_base.ogg")].into_iter().find_map(|f| crate::generate::read_sidecar(&dir.join(f)).ok())
}

/// A layered plain loop's graph: one zone, the loop's bars, `"loop": true`.
fn loop_graph(a: &AssetArgs, dir: &Path) -> Result<String, String> {
    let name = &a.name;
    let parts = loop_parts(dir, name);
    let sc = crate::generate::read_sidecar(&parts[0])?;
    let f = &sc.result;
    let rate = 44_100;
    let mix = mixdown(&parts)?;
    let len = mix.len() as f64 / rate as f64;
    let bpm = f["bpm"].as_f64().unwrap_or(score::DEFAULT_BPM as f64);
    let bars = f["bars"].as_u64().map(|b| b as u32).unwrap_or_else(|| ((len * bpm / 240.0).round() as u32).max(1)).max(1);
    let bar = len / bars as f64;
    let downbeat = match f["downbeat_seconds"].as_f64() {
        Some(d) => d,
        None => audio::beat_phase(&mix, rate, bars as usize * 4).0 as f64 / rate as f64,
    };
    let r4 = |x: f64| (x * 1e4).round() / 1e4;
    let mut files = if parts.len() == 2 {
        json!({ "base": format!("music/{name}_base.ogg"), "hi": format!("music/{name}_hi.ogg") })
    } else {
        json!({ "take": format!("music/{name}.ogg") })
    };
    if dir.join(format!("music/{name}_tension.ogg")).is_file() {
        files["tension"] = json!({
            "layer": format!("music/{name}_tension.ogg"), "tick": format!("music/{name}_tick.ogg"),
            "heartbeat": format!("music/{name}_heartbeat.ogg"), "sting": format!("music/{name}_sting.ogg"),
            "bars": bars, "downbeat_seconds": r4(downbeat),
        });
    }
    let cues: Vec<u32> = (1..=bars).filter(|k| k % 4 == 0 || *k == bars).collect();
    let g = json!({
        "format": "maimbrain-music-graph", "version": 1, "name": name, "loop": true,
        "files": files,
        "grid": { "bpm": r4(240.0 / bar), "beats_per_bar": 4, "bar_seconds": r4(bar), "first_bar_seconds": r4(downbeat),
                  "phrase_bars": bars.min(4), "first_bar": 0, "end_bar": bars },
        "zones": [ { "name": name, "sections": [0] } ],
        "sections": [ { "name": name, "zone": 0, "entry_bar": 0, "exit_bar": bars, "exit_cues": cues,
                        "segment": { "pre_entry_seconds": r4(downbeat), "entry_seconds": r4(downbeat),
                                     "exit_seconds": r4(downbeat + len), "post_exit_seconds": r4(downbeat + len) } } ],
        "links": [],
    });
    let path = format!("music/{name}.music.json");
    let text = serde_json::to_string_pretty(&g).unwrap() + "\n";
    std::fs::write(dir.join(&path), &text).map_err(|e| format!("{path}: {e}"))?;
    // Its sidecar is the loop's, with the graph's own sha.
    let mut gsc = sc.clone();
    gsc.output_sha256 = crate::generate::sha256_hex(text.as_bytes());
    gsc.result = json!({ "graph": name, "loop": true, "bars": bars, "bpm": r4(240.0 / bar) });
    crate::generate::write_sidecar(&dir.join(&path), &gsc)?;
    Ok(path)
}

/// A plain loop's files: its base and high layers, or the loop itself (tension alone).
fn loop_parts(dir: &Path, name: &str) -> Vec<PathBuf> {
    let base = dir.join(format!("music/{name}_base.ogg"));
    if base.is_file() { vec![base, dir.join(format!("music/{name}_hi.ogg"))] } else { vec![dir.join(format!("music/{name}.ogg"))] }
}

/// The sum of these Oggs (mono, 44.1 kHz), as long as the longest.
fn mixdown(files: &[PathBuf]) -> Result<Vec<f32>, String> {
    let mut out: Vec<f32> = Vec::new();
    for f in files {
        let bytes = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let (ch, rate) = audio::decode(&bytes, "audio/ogg")?;
        let x = audio::resample(&audio::to_mono(&ch), rate, 44_100);
        if x.len() > out.len() {
            out.resize(x.len(), 0.0);
        }
        for (o, v) in out.iter_mut().zip(&x) {
            *o += v;
        }
    }
    Ok(out)
}

/// One option's report entry: its files (with the server's provenance in
/// their sidecars) and a mix of its take to listen to.
fn option(a: &AssetArgs, made: &super::asset::Made, dir: &Path, n: usize, main: &str, form: &Form) -> Result<Value, String> {
    // The take a file came from: a setlist's song by its name's stem, else the one take.
    let take_of = |fname: &str| -> usize {
        match form {
            Form::Setlist(z) => z.iter().position(|(zone, _)| fname.starts_with(&format!("{}_{zone}_", a.name)) || fname.starts_with(&format!("{}_{zone}.", a.name))).unwrap_or(0),
            _ => 0,
        }
    };
    let mut files = Vec::new();
    let mut warnings: Vec<Value> = Vec::new();
    let mut seconds = Value::Null;
    let music = dir.join("music");
    let mut entries: Vec<_> = std::fs::read_dir(&music).map_err(|e| format!("{}: {e}", music.display()))?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let fname = p.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
        if fname.ends_with(".gen.json") || !p.is_file() {
            continue;
        }
        let path = format!("music/{fname}");
        if let Ok(mut sc) = crate::generate::read_sidecar(&p) {
            sc.generator = format!("mb {} studio-asset music", env!("CARGO_PKG_VERSION"));
            if sc.provider != "mb" {
                sc.via = Some("maimbrain".into());
                if !a.full_prompt.is_empty() && !matches!(form, Form::Setlist(_)) {
                    sc.full_prompt = a.full_prompt.clone();
                }
                // Its own call: the take's seed and what it cost (`--made`), else the job's split (`--cost`).
                let k = take_of(&fname);
                let call: Vec<super::asset::Call> = made.call(n - 1, k).cloned().into_iter().collect();
                if let Some(c) = call.first() {
                    sc.seed = c.seed;
                    sc.result["calls"] = json!(call);
                }
                sc.full_prompt = made.prompt(k, &sc.full_prompt).to_string();
                sc.est_cost_usd = super::asset::cost_of(&call, a.cost);
                sc.cost_usd = sc.est_cost_usd;
                if fname.contains("_take") && seconds.is_null() {
                    seconds = sc.result.get("source_seconds").cloned().unwrap_or(Value::Null);
                }
                if let Some(w) = sc.result.get("warnings").and_then(Value::as_array) {
                    for x in w {
                        if !warnings.contains(x) {
                            warnings.push(x.clone());
                        }
                    }
                }
            }
            crate::generate::write_sidecar(&p, &sc)?;
        }
        files.push(json!({ "file": format!("{n}/music/{fname}"), "path": path }));
    }
    if !files.iter().any(|f| f["path"] == main) {
        return Err(format!("option {n}: {main} wasn't made"));
    }
    // The mix to listen to: the take, or the first song's (both layers summed).
    let stem = match form {
        Form::Setlist(z) => format!("{}_{}", a.name, z[0].0),
        _ => a.name.clone(),
    };
    let parts: Vec<PathBuf> = match form {
        Form::Loop => loop_parts(dir, &stem),
        _ => {
            let whole = music.join(format!("{stem}_take.ogg"));
            if whole.is_file() { vec![whole] } else { vec![music.join(format!("{stem}_take_base.ogg")), music.join(format!("{stem}_take_hi.ogg"))] }
        }
    };
    let mix = mixdown(&parts)?;
    if seconds.is_null() {
        seconds = json!((mix.len() as f64 / 44_100.0 * 100.0).round() / 100.0);
    }
    let peak = mix.iter().fold(0f32, |m, v| m.max(v.abs()));
    let mix: Vec<f32> = if peak > 0.98 { mix.iter().map(|v| v * 0.98 / peak).collect() } else { mix };
    audio::encode_ogg(&mix, 44_100, 3.0, &dir.join("listen.ogg"))?;
    Ok(json!({ "files": files, "main": main, "listen": format!("{n}/listen.ogg"), "seconds": seconds, "warnings": warnings }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::mock::Mock;
    use crate::generate::provider::{AudioRequest, Provider};
    use maimbrain::audio::Sound;
    use maimbrain::music::{Director, Graph, Log, Setlist};

    fn project(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mb-studio-asset-music-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("studio.toml"), "title = \"T\"\nengine = \"2.0\"\nkind = \"2d\"\n").unwrap();
        dir
    }

    fn args(dir: &Path) -> AssetArgs {
        AssetArgs {
            project: dir.to_path_buf(),
            kind: "music".into(),
            name: "run".into(),
            out: dir.join("out"),
            candidates: Vec::new(),
            size: None,
            frame: None,
            looping: false,
            key: "#ff00ff".into(),
            prompt: "taiko drums and flutes".into(),
            full_prompt: String::new(),
            provider: "gemini".into(),
            model: "lyria-3.5".into(),
            seed: 7,
            cost: Some(0.08),
            plan: false,
            dims: None,
            pixel: false,
            frames: None,
            motion: String::new(),
            count: None,
            stands: false,
            no_seamless: false,
            horizon: None,
            refs: Vec::new(),
            zones: None,
            sections: None,
            setlist: None,
            layers: None,
            bpm: None,
            far: Vec::new(),
            layer_key: None,
            made: None,
            expressions: Vec::new(),
            voice: Default::default(),
        }
    }

    /// What a provider sends for a take: the mock's song, in the sections the prompt asks for.
    fn take(dir: &Path, name: &str, prompt: &str, seconds: f32, seed: u64) -> String {
        let r = AudioRequest { model: "mock-music".into(), prompt: prompt.into(), kind: audio::Kind::Music, seconds: Some(seconds), looping: true, seed: Some(seed), negative: None };
        let out = Mock.audio(&r).unwrap();
        let p = dir.join(format!("{name}.wav"));
        std::fs::write(&p, out.bytes).unwrap();
        p.display().to_string()
    }

    /// Plays a graph's files through the SDK's Director: it must load and start.
    fn plays(dir: &Path, graph: &str) -> Graph {
        let g = Graph::parse(&std::fs::read(dir.join(graph)).unwrap()).unwrap();
        for f in g.files() {
            assert!(dir.join(f).is_file(), "{graph} names {f}, which isn't there");
        }
        let files: Vec<String> = g.files().iter().map(|f| f.to_string()).collect();
        let mut d = Director::with_graph(g.clone(), |p| files.iter().position(|f| f == p).map(|i| Sound(i as u32 + 1)), 1, Log::default()).unwrap();
        d.start(0, 0.0);
        assert!(d.grid().is_some());
        g
    }

    #[test]
    fn zones_make_one_take_into_a_graph_with_layers_stingers_and_the_tension_kit() {
        let dir = project("zones");
        let mut a = args(&dir);
        a.zones = Some("town: warm lanterns; grove: hollow bamboo".into());
        a.layers = Some("base,hi,tension".into());
        a.bpm = Some(120.0);
        a.plan = true;
        let p = plan(&a).unwrap();
        assert_eq!(p["model"], "lyria-3.5");
        let takes = p["takes"].as_array().unwrap();
        assert_eq!(takes.len(), 1);
        let prompt = takes[0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("120 BPM") && prompt.contains("[0:00 - ") && prompt.contains("Grove: hollow bamboo"), "{prompt}");
        assert!(!prompt.to_lowercase().contains("chiptune"));
        let secs = takes[0]["seconds"].as_f64().unwrap() as f32;
        a.plan = false;
        a.candidates = vec![take(&dir, "a", prompt, secs, 1), take(&dir, "b", prompt, secs, 2)];
        // Each option's own call (the server's --made): its seed and its cost.
        let made = dir.join("made.json");
        std::fs::write(&made, r#"{ "prompts": ["the take's prompt"], "candidates": [[{ "seed": 11, "usd": 0.08 }], [{ "seed": 12, "usd": 0.081 }]] }"#).unwrap();
        a.made = Some(made);
        let report = process(&a).unwrap();
        let c = report["candidates"].as_array().unwrap();
        assert_eq!(c.len(), 2);
        let paths: Vec<&str> = c[0]["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        for want in ["music/run.music.json", "music/run_take_base.ogg", "music/run_take_hi.ogg", "music/run_swell.ogg", "music/run_into_grove.ogg", "music/run_tension.ogg", "music/run_sting.ogg"] {
            assert!(paths.contains(&want), "{want} in {paths:?}");
        }
        assert_eq!(c[0]["main"], "music/run.music.json");
        let o = a.out.join("1");
        assert!(a.out.join(c[0]["listen"].as_str().unwrap()).is_file());
        for f in c[0]["files"].as_array().unwrap() {
            assert!(a.out.join(format!("{}.gen.json", f["file"].as_str().unwrap())).is_file(), "{f}");
        }
        let take_sc = crate::generate::read_sidecar(&o.join("music/run_take_base.ogg")).unwrap();
        assert_eq!((take_sc.via.as_deref(), take_sc.cost_usd, take_sc.seed), (Some("maimbrain"), Some(0.08), 11));
        assert_eq!(take_sc.result["calls"], json!([{ "seed": 11, "usd": 0.08 }]));
        assert_eq!(take_sc.full_prompt, "the take's prompt");
        let other = crate::generate::read_sidecar(&a.out.join("2/music/run_take_hi.ogg")).unwrap();
        assert_eq!((other.seed, other.cost_usd), (12, Some(0.081)));
        // mb's own synthesis (the tension kit) has no provider call.
        let kit = crate::generate::read_sidecar(&o.join("music/run_tension.ogg")).unwrap();
        assert_eq!((kit.provider.as_str(), kit.result.get("calls")), ("mb", None));
        let g = plays(&o, "music/run.music.json");
        assert_eq!(g.zones.iter().map(|z| z.name.as_str()).collect::<Vec<_>>(), ["town", "grove"]);
        assert!(!g.looped);
    }

    #[test]
    fn a_setlist_is_a_graph_per_song_and_the_list() {
        let dir = project("setlist");
        let mut a = args(&dir);
        a.setlist = Some("harbor: bluesy rock, 128 bpm; neon: flamenco guitar, 104 bpm".into());
        a.prompt = String::new();
        let p = plan(&a).unwrap();
        let takes = p["takes"].as_array().unwrap();
        assert_eq!(takes.iter().map(|t| t["zone"].as_str().unwrap()).collect::<Vec<_>>(), ["harbor", "neon"]);
        assert!(takes[0]["prompt"].as_str().unwrap().contains("128 BPM"));
        let c: Vec<String> = takes.iter().enumerate().map(|(i, t)| take(&dir, &format!("s{i}"), t["prompt"].as_str().unwrap(), t["seconds"].as_f64().unwrap() as f32, 3)).collect();
        a.candidates = vec![c.join(",")];
        let report = process(&a).unwrap();
        let o = a.out.join("1");
        assert_eq!(report["candidates"][0]["main"], "music/run.songs.json");
        let list = Setlist::parse(&std::fs::read(o.join("music/run.songs.json")).unwrap()).unwrap();
        assert_eq!(list.songs, [("harbor".to_string(), "music/run_harbor.music.json".to_string()), ("neon".to_string(), "music/run_neon.music.json".to_string())]);
        for (_, g) in &list.songs {
            plays(&o, g);
        }
        // One take per song, or it says so.
        a.candidates = vec![c[0].clone()];
        assert!(process(&a).unwrap_err().contains("needs 2"));
    }

    #[test]
    fn layers_on_a_plain_loop_make_a_looped_graph() {
        let dir = project("loop");
        let mut a = args(&dir);
        a.layers = Some("base,hi,tension".into());
        a.provider = "fal".into();
        a.model = "fal-ai/ace-step/prompt-to-audio".into();
        assert!(plan(&a).is_err(), "a plain loop's prompt is the server's");
        a.candidates = vec![take(&dir, "l", "taiko drums, 120 BPM", 30.0, 4)];
        let report = process(&a).unwrap();
        let o = a.out.join("1");
        let c = &report["candidates"][0];
        assert_eq!(c["main"], "music/run.music.json");
        let paths: Vec<&str> = c["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        for want in ["music/run_base.ogg", "music/run_hi.ogg", "music/run_tension.ogg", "music/run_tick.ogg", "music/run_heartbeat.ogg"] {
            assert!(paths.contains(&want), "{want} in {paths:?}");
        }
        let g = plays(&o, "music/run.music.json");
        assert!(g.looped && g.end >= 1 && g.zones[0].name == "run");
        // The loop is the bars it says, to the sample's worth.
        let base = mixdown(&[o.join("music/run_base.ogg")]).unwrap();
        assert!((base.len() as f64 / 44_100.0 - g.end as f64 * g.bar).abs() < 0.01, "{} vs {}", base.len(), g.end as f64 * g.bar);
    }

    #[test]
    fn forms_are_one_at_a_time() {
        let dir = project("forms");
        let mut a = args(&dir);
        a.zones = Some("a: x; b: y".into());
        a.sections = Some("a,b".into());
        assert!(plan(&a).unwrap_err().contains("give one"));
    }
}
