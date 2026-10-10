//! `mb music <game> <name> --audition`: every join and switch of a piece in
//! sections (`--zones`, `--sections`, `--variations`) in one file, so a
//! person can hear in a few minutes what a game will do with it, mixed the
//! way the SDK's `music::Director` mixes it. A soft click in a short silence
//! comes before each; the timeline is printed.
//!
//! **A music graph** (`<name>.music.json`, graph.rs), spliced as the
//! Director splices (the new voice's pre-entry fading in before the bar line,
//! the old one's post-exit dying away after it, 60 dB over its length):
//! - **Jumps**: every jump link in each zone: two bars of the take up to the
//!   link's bar line, then two bars from where it jumps to.
//! - **Restarts**: a zone that has no link back at the end of its stretch of
//!   the take, into its entry again under the swell.
//! - **Changes** from each zone into every other: the best seamless link if
//!   it has one (the Director takes it when it comes before the next exit
//!   cue), else the change at its first exit cue under the new zone's stinger
//!   (or the swell); and where the take itself runs from one into the other,
//!   that too.
//!
//! **Loops** (with no graph: assets made before it, `--loops`):
//! - **Joins**: a loop's last bar, then (on repeat) its first.
//! - **Switches between zones or sections** (`Change::Auto`, the Director's
//!   default): a bar of the old loop from the middle of it; the new loop's
//!   stinger (or the piece's swell) lands on the next bar line, where the old
//!   one has faded out over half a beat and the new one starts from its top;
//!   then two bars of it.
//! - **A zone's variations** (the Director's rotation): a one-bar crossfade
//!   from each to the next.
//!
//! The loops are played as written (base + hi, the full mix), the stinger
//! and swell at the Director's level for them.

use std::path::Path;

use serde_json::Value;

use super::audio;
use super::score::Section;

/// The Director's level for a stinger or swell (`Levels::stinger`).
const COVER_VOL: f32 = 0.6;
/// Silence before each item, with the click in its middle (seconds).
const GAP: f64 = 0.6;

struct Part {
    name: String,
    samples: Vec<f32>,
    /// The stinger into it and where its hit is (samples).
    stinger: Option<(Vec<f32>, usize)>,
    loop_match: Option<f64>,
}

fn load(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (ch, rate) = audio::decode(&b, "audio/ogg").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((audio::to_mono(&ch), rate))
}

fn land(sidecar: &Path, rate: u32) -> Option<usize> {
    let sc = super::read_sidecar(sidecar).ok()?;
    Some((sc.result["land_seconds"].as_f64()? * rate as f64).round() as usize)
}

/// A soft click (a short 2 kHz tick, about −20 dBFS).
fn click(rate: u32) -> Vec<f32> {
    let n = (0.02 * rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            (std::f32::consts::TAU * 2000.0 * t).sin() * (-t / 0.003).exp() * 0.1
        })
        .collect()
}

/// Mixes `x` × `gain(i)` into `out` from sample `at` (growing it as needed).
fn add(out: &mut Vec<f32>, at: usize, x: &[f32], gain: impl Fn(usize) -> f32) {
    if out.len() < at + x.len() {
        out.resize(at + x.len(), 0.0);
    }
    for (i, v) in x.iter().enumerate() {
        out[at + i] += v * gain(i);
    }
}

/// `n` samples of loop `x` from sample `from`, wrapping.
fn take(x: &[f32], from: usize, n: usize) -> Vec<f32> {
    (0..n).map(|i| x[(from + i) % x.len()]).collect()
}

/// Renders the audition of the music named `name` in `assets` to `out` (Ogg
/// Vorbis): its music graph's, or with none its loops'.
pub fn render(assets: &Path, name: &str, out: &Path, quality: f32) -> Result<(), String> {
    let sc = super::sound::find_music_sidecar(assets, name)?;
    let stem = Path::new(sc.recipe["out"].as_str().unwrap_or("")).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name.to_string());
    let graph = assets.join(format!("{stem}.music.json"));
    if graph.exists() {
        return render_graph(assets, name, &graph, out, quality);
    }
    render_loops(assets, name, out, quality)
}

/// A take as one signal: its full mix, or its base and high layers summed.
fn load_take(root: &Path, files: &Value) -> Result<(Vec<f32>, u32), String> {
    if let Some(t) = files["take"].as_str() {
        return load(&root.join(t));
    }
    let (b, rate) = load(&root.join(files["base"].as_str().ok_or("the graph names no take")?))?;
    let (h, _) = load(&root.join(files["hi"].as_str().ok_or("the graph names no high layer")?))?;
    Ok((b.iter().zip(&h).map(|(a, b)| a + b).collect(), rate))
}

/// A one-shot and where its hit lands (samples), from the graph's `{ file, land_seconds }`.
fn cover(root: &Path, v: &Value, rate: u32) -> Option<(Vec<f32>, usize)> {
    let x = load(&root.join(v["file"].as_str()?)).ok()?.0;
    Some((x, (v["land_seconds"].as_f64()? * rate as f64).round() as usize))
}

/// The music graph's jumps, restarts and changes (see the module's docs).
fn render_graph(assets: &Path, name: &str, path: &Path, out: &Path, quality: f32) -> Result<(), String> {
    let g: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| format!("{}: {e}", path.display()))?;
    // Paths in the graph are from the game's root, as the game loads them.
    let root = assets.parent().unwrap_or(Path::new("."));
    let (take, rate) = load_take(root, &g["files"])?;
    let r = rate as f64;
    let bar = g["grid"]["bar_seconds"].as_f64().unwrap_or(2.0);
    let first = g["grid"]["first_bar_seconds"].as_f64().unwrap_or(0.0);
    let beat = bar / 4.0;
    let seamless = g["splice"]["seamless"].as_f64().unwrap_or(0.8);
    let (lpre, lpost) = (g["splice"]["link_pre_beats"].as_f64().unwrap_or(0.25) * beat, g["splice"]["link_post_beats"].as_f64().unwrap_or(1.0) * beat);
    let pos = |k: u64| ((first + k as f64 * bar) * r).round() as usize;
    let secs = |s: f64| (s * r).round() as usize;
    let sections = g["sections"].as_array().cloned().unwrap_or_default();
    let zones = g["zones"].as_array().cloned().unwrap_or_default();
    let links = g["links"].as_array().cloned().unwrap_or_default();
    let n = |v: &Value, k: &str| v[k].as_u64().unwrap_or(0);
    let zone_name = |z: u64| zones.get(z as usize).and_then(|v| v["name"].as_str()).unwrap_or("?").to_string();
    let section_of = |k: u64| sections.iter().find(|s| (n(s, "entry_bar")..n(s, "exit_bar")).contains(&k));
    let pre_entry = |s: &Value| s["segment"]["entry_seconds"].as_f64().unwrap_or(0.0) - s["segment"]["pre_entry_seconds"].as_f64().unwrap_or(0.0);
    let post_exit = |s: &Value| s["segment"]["post_exit_seconds"].as_f64().unwrap_or(0.0) - s["segment"]["exit_seconds"].as_f64().unwrap_or(0.0);
    let swell = cover(root, &g["files"]["swell"], rate);
    let stingers: Vec<Option<(Vec<f32>, usize)>> = zones.iter().map(|z| cover(root, &z["stinger"], rate)).collect();
    let mut mix: Vec<f32> = Vec::new();
    let mut timeline: Vec<String> = Vec::new();
    let tick = click(rate);
    let gap = (GAP * r) as usize;
    let stamp = |n: usize| {
        let s = n as f64 / r;
        format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
    };
    let lead = secs(2.0 * bar);
    // Two bars of the take up to bar line `from`, then on from bar `to`: the new voice from `pre`
    // seconds early fading in (a quarter sine), the old dying away over `post` from the bar line.
    let splice = |mix: &mut Vec<f32>, from: u64, to: u64, pre: f64, post: f64, cv: Option<&(Vec<f32>, usize)>| -> usize {
        let at = mix.len();
        add(mix, at + gap / 2, &tick, |_| 1.0);
        mix.resize(at + gap, 0.0);
        let t = at + gap + lead;
        let (p0, pn) = (pos(from), pos(to));
        let (pre_s, post_s) = (secs(pre).min(pn), secs(post));
        let tc = (post / 6.9).max(1e-4) * r;
        let o0 = p0.saturating_sub(lead);
        let old = &take[o0.min(take.len())..(p0 + post_s).min(take.len())];
        add(mix, t - (p0 - o0), old, |i| if i + o0 < p0 { 1.0 } else { (-((i + o0 - p0) as f64) / tc).exp() as f32 });
        let new = &take[(pn - pre_s).min(take.len())..(pn + lead).min(take.len())];
        add(mix, t - pre_s, new, |i| if i < pre_s { (i as f32 / pre_s as f32 * std::f32::consts::FRAC_PI_2).sin() } else { 1.0 });
        if let Some((x, land)) = cv {
            add(mix, t.saturating_sub(*land), x, |_| COVER_VOL);
        }
        t
    };
    // Jumps.
    let mut jumps = 0;
    for (zi, _) in zones.iter().enumerate() {
        for l in links.iter().filter(|l| l["into_zone"].is_null() && section_of(n(l, "to")).is_some_and(|s| n(s, "zone") == zi as u64)) {
            let t = splice(&mut mix, n(l, "from"), n(l, "to"), lpre, lpost, None);
            jumps += 1;
            timeline.push(format!(
                "{}  {}: jump at bar {} to bar {} (link {:.2}: harmony {:.2}, level ±{:.1} dB, rhythm {:.2})",
                stamp(t),
                zone_name(zi as u64),
                n(l, "from"),
                n(l, "to"),
                l["score"].as_f64().unwrap_or(0.0),
                l["harmony"].as_f64().unwrap_or(0.0),
                l["level_db"].as_f64().unwrap_or(0.0),
                l["rhythm"].as_f64().unwrap_or(0.0)
            ));
        }
    }
    // Restarts.
    let mut restarts = 0;
    for rt in g["returns"].as_array().cloned().unwrap_or_default().iter().filter(|rt| rt["via"] == "restart") {
        let (from, to) = (n(rt, "at_bar"), n(rt, "to_bar"));
        let (Some(into), Some(left)) = (section_of(to), from.checked_sub(1).and_then(section_of)) else { continue };
        let t = splice(&mut mix, from, to, pre_entry(into), post_exit(left), swell.as_ref());
        restarts += 1;
        timeline.push(format!(
            "{}  {}: at its end (bar {from}) back to its entry (bar {to}) under the swell{}",
            stamp(t),
            zone_name(n(into, "zone")),
            rt["best_link"].as_f64().map(|b| format!("; its best link back joins at {b:.2}")).unwrap_or_default()
        ));
    }
    // Changes between zones.
    let mut changes = 0;
    for tr in g["transitions"].as_array().cloned().unwrap_or_default() {
        let (a, b) = (n(&tr, "from"), n(&tr, "to"));
        let (za, zb) = (&zones[a as usize], &zones[b as usize]);
        let spans_a: Vec<&Value> = za["sections"].as_array().into_iter().flatten().filter_map(|s| sections.get(s.as_u64()? as usize)).collect();
        let Some(&entry_b) = zb["sections"].as_array().and_then(|s| s.first()).and_then(|s| sections.get(s.as_u64()? as usize)).as_ref() else { continue };
        // The best seamless link from the zone into the other, if there is one.
        let best = links
            .iter()
            .filter(|l| l["into_zone"].as_u64() == Some(b) && l["score"].as_f64().unwrap_or(0.0) >= seamless && section_of(n(l, "from").saturating_sub(1)).is_some_and(|s| n(s, "zone") == a))
            .max_by(|x, y| x["score"].as_f64().unwrap_or(0.0).total_cmp(&y["score"].as_f64().unwrap_or(0.0)));
        let line = match best {
            Some(l) => {
                let t = splice(&mut mix, n(l, "from"), n(l, "to"), lpre, lpost, None);
                format!("{}  {} → {}: a seamless link at bar {} to bar {} ({:.2})", stamp(t), zone_name(a), zone_name(b), n(l, "from"), n(l, "to"), l["score"].as_f64().unwrap_or(0.0))
            }
            None => {
                let Some(cue) = spans_a.first().and_then(|s| s["exit_cues"].as_array()?.first()?.as_u64()) else { continue };
                let (cv, what) = match &stingers[b as usize] {
                    Some(s) => (Some(s), "its stinger"),
                    None => (swell.as_ref(), "the swell"),
                };
                let left = section_of(cue - 1).unwrap_or(spans_a[0]);
                let t = splice(&mut mix, cue, n(entry_b, "entry_bar"), pre_entry(entry_b), post_exit(left), cv);
                format!("{}  {} → {}: at the exit cue at bar {cue}, from its entry (bar {}) under {what}", stamp(t), zone_name(a), zone_name(b), n(entry_b, "entry_bar"))
            }
        };
        timeline.push(line);
        changes += 1;
        if tr["take_runs_into_it"] == true {
            // The take's own way from one into the other: two bars either side of where it does.
            let Some(s) = spans_a.iter().find(|s| section_of(n(s, "exit_bar")).is_some_and(|x| n(x, "zone") == b)) else { continue };
            let k = n(s, "exit_bar");
            let t = splice(&mut mix, k, k, 0.0, 0.0, None);
            timeline.push(format!("{}  {} → {}: the take's own transition at bar {k}", stamp(t), zone_name(a), zone_name(b)));
            changes += 1;
        }
    }
    mix.resize(mix.len() + gap, 0.0);
    let mix = audio::limit(&mix, 1.0, rate);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    audio::encode_ogg(&mix, rate, quality, out)?;
    eprintln!("audition of {name}'s music graph: {jumps} jumps, {restarts} restarts and {changes} changes, {:.0} s → {}", mix.len() as f64 / r, out.display());
    eprintln!("  a soft click before each; listen for the beat, the harmony and the groove carrying on through each one:");
    for t in &timeline {
        eprintln!("  {t}");
    }
    println!("{}", out.display());
    Ok(())
}

/// The loops' joins and switches (see the module's docs).
fn render_loops(assets: &Path, name: &str, out: &Path, quality: f32) -> Result<(), String> {
    let sc = super::sound::find_music_sidecar(assets, name)?;
    let sections: Vec<Section> = serde_json::from_value(sc.recipe["sections"].clone()).unwrap_or_default();
    if sections.is_empty() {
        return Err(format!("{name} isn't music in sections (--zones, --sections, --variations): nothing to switch between"));
    }
    let r = &sc.result;
    let (bars, bpm) = (r["bars"].as_u64().unwrap_or(8) as usize, r["bpm"].as_f64().unwrap_or(120.0));
    let pre_s = r["downbeat_seconds"].as_f64().unwrap_or(0.0);
    let stem = Path::new(sc.recipe["out"].as_str().unwrap_or("")).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name.to_string());
    let layered = sc.recipe["layers"].as_bool().unwrap_or(false);
    let file = |suffix: &str| assets.join(format!("{stem}{suffix}.ogg"));
    let mut rate = 0u32;
    let mut parts: Vec<Part> = Vec::new();
    for s in super::score::unique(&sections) {
        let (samples, sr, side) = if layered {
            let (b, sr) = load(&file(&format!("_{}_base", s.name)))?;
            let (h, _) = load(&file(&format!("_{}_hi", s.name)))?;
            (b.iter().zip(&h).map(|(a, b)| a + b).collect::<Vec<f32>>(), sr, file(&format!("_{}_base", s.name)))
        } else {
            let (x, sr) = load(&file(&format!("_{}", s.name)))?;
            (x, sr, file(&format!("_{}", s.name)))
        };
        rate = sr;
        let into = file(&format!("_into_{}", s.name));
        let stinger = if into.exists() { Some((load(&into)?.0, land(&into, sr).unwrap_or(0))) } else { None };
        let loop_match = super::read_sidecar(&side).ok().and_then(|sc| sc.result["loop_match"].as_f64());
        parts.push(Part { name: s.name.clone(), samples, stinger, loop_match });
    }
    let swell = if file("_swell").exists() { Some((load(&file("_swell"))?.0, land(&file("_swell"), rate).unwrap_or(0))) } else { None };
    let len = parts[0].samples.len();
    let bar = len / bars;
    let half_beat = (60.0 / bpm / 2.0 * rate as f64) as usize;
    let pre = (pre_s * rate as f64).round() as usize;
    // Groups: a zone and its variations (rotated); --variations is one group; --sections each its own.
    let stingers = sc.recipe["stingers"].as_bool().unwrap_or(false);
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let s = sections.iter().find(|s| s.name == p.name);
        let parent = s.and_then(|s| s.variation_of.clone());
        match parent.and_then(|v| groups.iter().position(|g| parts[g[0]].name == v)) {
            Some(g) => groups[g].push(i),
            None if !stingers && !groups.is_empty() => groups[0].push(i),
            None => groups.push(vec![i]),
        }
    }
    let mut mix: Vec<f32> = Vec::new();
    let mut timeline: Vec<String> = Vec::new();
    let tick = click(rate);
    let gap = (GAP * rate as f64) as usize;
    let stamp = |n: usize| {
        let s = n as f64 / rate as f64;
        format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
    };
    let begin = |mix: &mut Vec<f32>| -> usize {
        let at = mix.len();
        add(mix, at + gap / 2, &tick, |_| 1.0);
        mix.resize(at + gap, 0.0);
        at + gap
    };
    // Joins: the last bar, then the first (on repeat).
    for p in &parts {
        let at = begin(&mut mix);
        add(&mut mix, at, &take(&p.samples, len - bar, 2 * bar), |_| 1.0);
        timeline.push(format!(
            "{}  join of {} (its join at {}){}",
            stamp(at),
            p.name,
            stamp(at + bar),
            p.loop_match.map(|m| format!(", loop match {m:.2}")).unwrap_or_default()
        ));
    }
    // One switch: a bar of `a` from the middle of it, then `b` from its top on the bar line.
    let mut switch =
        |mix: &mut Vec<f32>, a: usize, b: usize, rotate: bool| {
            let at = begin(mix);
            let from = (bars / 2).saturating_sub(1) * bar;
            let t = at + bar; // the bar line the switch is on
            let (pa, pb) = (&parts[a], &parts[b]);
            // Both files have their 1 `pre` in: the old one is laid `pre` early so its bars fall on the same lines.
            let how = if rotate {
                // A one-bar crossfade, equal power.
                add(mix, at - pre, &take(&pa.samples, from, 2 * bar + pre), |i| {
                    let k = i.saturating_sub(bar + pre);
                    if i < bar + pre { 1.0 } else { (k as f32 / bar as f32 * std::f32::consts::FRAC_PI_2).cos() }
                });
                add(mix, t - pre, &take(&pb.samples, 0, 2 * bar), |i| ((i.saturating_sub(pre) as f32 / bar as f32).min(1.0) * std::f32::consts::FRAC_PI_2).sin());
                "a one-bar crossfade (rotation)"
            } else {
                // The old loop fades out over the half beat before the bar line; the new one starts on it.
                let cover = pb.stinger.as_ref().map(|s| (s, "its stinger")).or(swell.as_ref().map(|s| (s, "the swell")));
                add(mix, at - pre, &take(&pa.samples, from, bar + pre), |i| {
                    let k = i as isize - (bar + pre - half_beat) as isize;
                    if k <= 0 { 1.0 } else { ((k as f32 / half_beat as f32).min(1.0) * std::f32::consts::FRAC_PI_2).cos() }
                });
                add(mix, t - pre, &take(&pb.samples, 0, 2 * bar + pre), |_| 1.0);
                match cover {
                    Some(((x, land), what)) => {
                        add(mix, t - land, x, |_| COVER_VOL);
                        what
                    }
                    None => "a cut on the bar (no stinger or swell)",
                }
            };
            timeline.push(format!("{}  {} → {} at {}, {how}", stamp(at), pa.name, pb.name, stamp(t)));
        };
    for (gi, ga) in groups.iter().enumerate() {
        for (gj, gb) in groups.iter().enumerate() {
            if gi != gj {
                switch(&mut mix, ga[0], gb[0], false);
            }
        }
    }
    for g in &groups {
        if g.len() < 2 {
            continue;
        }
        // Each to the next, round (with two: there and back).
        for k in 0..g.len() {
            switch(&mut mix, g[k], g[(k + 1) % g.len()], true);
        }
    }
    mix.resize(mix.len() + gap, 0.0);
    let mix = audio::limit(&mix, 1.0, rate);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    audio::encode_ogg(&mix, rate, quality, out)?;
    eprintln!(
        "audition of {name}: {} joins and {} switches, {:.0} s → {}",
        parts.len(),
        timeline.len() - parts.len(),
        mix.len() as f64 / rate as f64,
        out.display()
    );
    eprintln!("  a soft click before each; listen for the beat carrying on through each join and switch:");
    for t in &timeline {
        eprintln!("  {t}");
    }
    println!("{}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let mut v = vec![0.0f32; 2];
        add(&mut v, 1, &[1.0, 2.0], |i| i as f32 + 1.0);
        assert_eq!(v, vec![0.0, 1.0, 4.0]);
        assert_eq!(take(&[1.0, 2.0, 3.0], 2, 4), vec![3.0, 1.0, 2.0, 3.0]);
        let c = click(22050);
        assert!(audio::peak(&c) < 0.11 && audio::peak(&c) > 0.05);
    }
}
