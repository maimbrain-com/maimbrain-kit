//! `mb studio-asset <project> voice <who>` for engine 2.0 projects
//! (docs/STUDIO-2.md §5 [voice], §7): the server speaks a line (or designs
//! a voice) on ElevenLabs and hands the raw audio here; this does `mb
//! voice`'s post-processing on it, so the server's lines match the kit's:
//!
//! - a line (`--line <id> --text "…"`, each candidate its raw take and the
//!   provider's character timings, `take.pcm,take.timing.json`): mono Ogg
//!   at the dialogue level (`voice::process`: high-pass, presence, trimmed,
//!   −16 dBFS), time-stretched for a speed the model ignores, and its lip
//!   track (`voice::lip_flap`), written as `voices/<who>/<line>.ogg`,
//!   `voices/<who>/<line>.lips.txt` ("mblips 1": hz, dur, mouth, text,
//!   reveal, as `maimbrain::ui::Lips::parse` reads it) and the Ogg's sidecar;
//! - a design's previews (`--preview`, each candidate one preview's raw
//!   audio): processed the same, for listening, as `preview.ogg`.
//!
//! Each candidate also gets `reference.ogg`: its audio unprocessed (mono, its
//! own rate, Ogg q6, as `mb voice` keeps in art/voice_ref/), which the server
//! keeps so a designed voice could be made again by cloning (Decided 24, 54).
//!
//! The report: `{ kind, name, candidates: [{ files: [{ file, path }],
//! seconds, subtitle, pitch_hz, phone_loss_db, timed_by, reference, warnings }] }`,
//! the Ogg first (the judge and the card play it).

use std::path::Path;

use serde_json::{Value, json};

use super::asset::AssetArgs;
use crate::generate::audio;
use crate::generate::voice::{self, Alignment, LIPS_HZ};
use crate::generate::{Sidecar, sha256_hex};

/// The Ogg's rate and quality (`mb voice say`'s defaults).
const RATE: u32 = 44100;
const QUALITY: f32 = 4.0;
/// A line longer than this is long for a game (`mb voice`'s warning).
const LONG_SECS: f32 = 6.0;

pub fn process(a: &AssetArgs) -> Result<Value, String> {
    voice::check_name("character", &a.name)?;
    if a.candidates.is_empty() {
        return Err("--candidate: at least one".into());
    }
    let line = match (&a.voice.line, a.voice.preview) {
        (Some(_), true) => return Err("--line or --preview, not both".into()),
        (Some(l), false) => {
            voice::check_name("line id", l)?;
            if voice::subtitle(&a.voice.text).is_empty() {
                return Err(format!("{} {l}: --text has no words", a.name));
            }
            Some(l.clone())
        }
        (None, true) => None,
        (None, false) => return Err("--line <id> --text \"…\" for a line, or --preview for a design's previews".into()),
    };
    let mut out = Vec::new();
    for (i, c) in a.candidates.iter().enumerate() {
        let n = i + 1;
        let files: Vec<&str> = c.split(',').filter(|s| !s.is_empty()).collect();
        let raw_path = files.first().ok_or(format!("candidate {n} names no files"))?;
        let bytes = std::fs::read(raw_path).map_err(|e| format!("{raw_path}: {e}"))?;
        let mime = voice::mime_for_ext(Path::new(raw_path));
        let alignment = match files.get(1) {
            Some(t) => {
                let text = std::fs::read_to_string(t).map_err(|e| format!("{t}: {e}"))?;
                Some(serde_json::from_str::<Alignment>(&text).map_err(|e| format!("{t}: {e}"))?)
            }
            None => None,
        };
        let (chans, src_rate) = audio::decode(&bytes, &mime).map_err(|e| format!("candidate {n}: {e}"))?;
        let mono = audio::to_mono(&chans);
        let dir = a.out.join(n.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let reference = reference(&mono, src_rate, &dir.join("reference.ogg"))?;
        let mut x = audio::resample(&mono, src_rate, RATE);
        let mut al = alignment;
        let mut warnings = Vec::new();
        let mut stretched = None;
        if line.is_some()
            && let Some(f) = stretch_for(&a.model, a.voice.speed)
        {
            x = audio::stretch(&x, RATE, f);
            al = al.map(|v| Alignment { chars: v.chars, start: v.start.iter().map(|t| t * f as f32).collect(), end: v.end.iter().map(|t| t * f as f32).collect() });
            stretched = Some(f);
        }
        let pitch = voice::median_pitch(&x, RATE).map(f32::round);
        let (y, offset) = voice::process(&x, RATE);
        let rel = match &line {
            Some(l) => format!("voices/{}/{l}.ogg", a.name),
            None => "preview.ogg".to_string(),
        };
        let dest = dir.join(&rel);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        let decoded = audio::encode_ogg(&y, RATE, QUALITY, &dest)?;
        let ogg = std::fs::read(&dest).map_err(|e| e.to_string())?;
        let chk = audio::check(&decoded, RATE, false);
        let phone = (audio::phone_loss_db(&decoded, RATE, true) * 10.0).round() / 10.0;
        if phone < audio::PHONE_WARN_DB {
            warnings.push(format!("a phone speaker loses {:.0} dB of it: a very deep voice; design it a little higher, or lean on the subtitle", -phone));
        }
        let secs = y.len() as f32 / RATE as f32;
        let mut files_out = vec![json!({ "file": format!("{n}/{rel}"), "path": rel })];
        let mut result = json!({
            "seconds": chk.secs,
            "peak_db": chk.peak_db,
            "loud_rms_db": chk.rms_db,
            "phone_loss_db": phone,
            "pitch_hz": pitch,
        });
        let mut subtitle = String::new();
        let mut timed_by = Value::Null;
        if let Some(l) = &line {
            if secs > LONG_SECS {
                warnings.push(format!("{secs:.1} s is long for a game line: barks are 1–2 s, cutscene lines a sentence or two"));
            }
            subtitle = voice::subtitle(&a.voice.text);
            let shifted = al.as_ref().map(|v| shift(v, offset as f32 / RATE as f32));
            let lips = voice::lip_flap(&y, RATE, shifted.as_ref(), &subtitle);
            let lips_rel = format!("voices/{}/{l}.lips.txt", a.name);
            std::fs::write(dir.join(&lips_rel), lips.to_text(&subtitle)).map_err(|e| e.to_string())?;
            files_out.push(json!({ "file": format!("{n}/{lips_rel}"), "path": lips_rel }));
            timed_by = json!(if shifted.is_some() { "the provider's character timings" } else { "loudness" });
            result["subtitle"] = json!(subtitle);
            result["lips"] = json!(lips_rel);
            result["lips_hz"] = json!(LIPS_HZ);
            result["timed_by"] = timed_by.clone();
            if let Some(f) = stretched {
                result["speed"] = json!(format!("time-stretched to {f:.3}× the length after ({} ignores the speed setting), pitch kept", a.model));
            }
        }
        result["warnings"] = json!(warnings);
        sidecar(a, &dest, &ogg, &bytes, line.as_deref(), result)?;
        out.push(json!({
            "files": files_out, "seconds": (secs * 1000.0).round() / 1000.0, "subtitle": if line.is_some() { json!(subtitle) } else { Value::Null },
            "pitch_hz": pitch, "phone_loss_db": phone, "timed_by": timed_by, "reference": format!("{n}/reference.ogg"),
            "reference_seconds": reference, "warnings": warnings,
        }));
    }
    Ok(json!({ "kind": "voice", "name": a.name, "line": line, "candidates": out }))
}

/// How much longer to make a line, for a speed its model ignores (Eleven v3 and v4; `mb voice`'s `stretch_for`).
fn stretch_for(model: &str, speed: Option<f32>) -> Option<f64> {
    speed.map(|s| s.clamp(0.7, 1.2)).filter(|s| (s - 1.0).abs() > 0.005 && !voice::speed_native(model)).map(|s| 1.0 / voice::api_f(s))
}

fn shift(a: &Alignment, by: f32) -> Alignment {
    Alignment { chars: a.chars.clone(), start: a.start.iter().map(|t| (t - by).max(0.0)).collect(), end: a.end.iter().map(|t| (t - by).max(0.0)).collect() }
}

/// The audio as it came, for making the voice again: mono, at most 44.1 kHz,
/// peaks at 0.9, Ogg q6 (`mb voice`'s `save_reference`). Its seconds.
fn reference(mono: &[f32], rate: u32, dest: &Path) -> Result<f32, String> {
    let (mut x, rate) = if rate > 44100 { (audio::resample(mono, rate, 44100), 44100) } else { (mono.to_vec(), rate) };
    let p = audio::peak(&x);
    if p > 0.9 {
        x.iter_mut().for_each(|v| *v *= 0.9 / p);
    }
    audio::encode_ogg(&x, rate, 6.0, dest)?;
    Ok((x.len() as f32 / rate as f32 * 100.0).round() / 100.0)
}

fn sidecar(a: &AssetArgs, dest: &Path, out: &[u8], raw: &[u8], line: Option<&str>, result: Value) -> Result<(), String> {
    let v = &a.voice;
    // What `generate` compares to know a line is unchanged: who, the line, the words, the emotion and the voice.
    let recipe = json!({
        "kind": if line.is_some() { "voice" } else { "voice_preview" }, "engine": "2.0", "character": a.name, "line": line,
        "text": if line.is_some() { json!(v.text.trim()) } else { Value::Null }, "emotion": v.emotion, "voice": v.voice_id, "speed": v.speed,
        "rate": RATE, "quality": QUALITY,
    });
    let sc = Sidecar {
        generator: format!("mb {} studio-asset voice", env!("CARGO_PKG_VERSION")),
        date: crate::generate::now_utc(),
        provider: a.provider.clone(),
        model: a.model.clone(),
        via: Some("maimbrain".into()),
        prompt: if line.is_some() { v.text.trim().to_string() } else { a.prompt.clone() },
        full_prompt: a.full_prompt.clone(),
        seed: a.seed,
        seed_honored: true,
        references: Vec::new(),
        style: None,
        est_cost_usd: a.cost,
        cost_usd: a.cost,
        raw: "(not kept: made by the Maimbrain server)".into(),
        raw_sha256: sha256_hex(raw),
        output_sha256: sha256_hex(out),
        allowed_names: Vec::new(),
        model_text: None,
        recipe,
        result,
        note: "Generated with ElevenLabs from a voice designed for this game or one of its premade voices (never a clone of a person).".into(),
        derived: Vec::new(),
    };
    crate::generate::write_sidecar(dest, &sc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::asset::VoiceArgs;
    use std::path::PathBuf;

    fn dir(test: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mb-studio-voice-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("studio.toml"), "title = \"T\"\nengine = \"2.0\"\nkind = \"2d\"\n").unwrap();
        d
    }

    fn args(d: &Path, voice: VoiceArgs, candidates: Vec<String>) -> AssetArgs {
        let mut a = super::super::asset2::tests::args(d, "voice", [1, 1], candidates);
        a.name = "gruk".into();
        a.model = "eleven_v4".into();
        a.provider = "elevenlabs".into();
        a.dims = None;
        a.voice = voice;
        a
    }

    /// Raw PCM at 24 kHz, as ElevenLabs sends it, with `voice::babble`'s exact timings.
    fn take(d: &Path, name: &str, sent: &str) -> String {
        let (x, al) = voice::babble(sent, "gruk");
        let pcm: Vec<u8> = x.iter().flat_map(|v| ((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let raw = d.join(format!("{name}.pcm"));
        std::fs::write(&raw, pcm).unwrap();
        let timing = d.join(format!("{name}.timing.json"));
        std::fs::write(&timing, serde_json::to_string(&al).unwrap()).unwrap();
        format!("{},{}", raw.display(), timing.display())
    }

    #[test]
    fn a_line_is_an_ogg_with_its_lip_track_sidecar_and_reference() {
        let d = dir("line");
        let text = "Oi. Nobody crosses Gruk's bridge for free.";
        let sent = format!("[smug] {text}");
        let v = VoiceArgs { line: Some("halt".into()), text: text.into(), emotion: Some("smug".into()), voice_id: "V1".into(), ..VoiceArgs::default() };
        let a = args(&d, v, vec![take(&d, "a", &sent), take(&d, "b", &sent)]);
        let r = process(&a).unwrap();
        let c = r["candidates"].as_array().unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0]["files"][0]["path"], "voices/gruk/halt.ogg");
        assert_eq!(c[0]["files"][1]["path"], "voices/gruk/halt.lips.txt");
        assert_eq!(c[0]["subtitle"], text);
        assert_eq!(c[0]["timed_by"], "the provider's character timings");
        let lips = std::fs::read_to_string(d.join("out/1/voices/gruk/halt.lips.txt")).unwrap();
        let rows: Vec<&str> = lips.lines().collect();
        assert_eq!(rows[0], "mblips 1");
        assert_eq!(rows[1], "hz 30");
        assert!(rows[2].starts_with("dur "));
        assert!(rows[3].starts_with("mouth ") && rows[3][6..].chars().all(|c| ('0'..='3').contains(&c)));
        assert_eq!(rows[4], format!("text {text}"));
        let reveal: Vec<u32> = rows[5].strip_prefix("reveal ").unwrap().split(' ').map(|s| s.parse().unwrap()).collect();
        assert_eq!(reveal.len(), text.chars().count(), "a time for every subtitle character, the [smug] tag left out");
        assert!(reveal.windows(2).all(|w| w[0] <= w[1]));
        let sc: Value = serde_json::from_str(&std::fs::read_to_string(d.join("out/1/voices/gruk/halt.ogg.gen.json")).unwrap()).unwrap();
        assert_eq!(sc["recipe"]["text"], text);
        assert_eq!(sc["recipe"]["emotion"], "smug");
        assert_eq!(sc["recipe"]["voice"], "V1");
        assert_eq!(sc["result"]["lips_hz"], 30);
        assert!(d.join("out/1/reference.ogg").is_file() && d.join("out/2/voices/gruk/halt.ogg").is_file());
        let ogg = std::fs::read(d.join("out/1/voices/gruk/halt.ogg")).unwrap();
        let (chans, rate) = audio::decode(&ogg, "audio/ogg").unwrap();
        assert_eq!((chans.len(), rate), (1, RATE));
        let level = audio::db(audio::loud_rms(&chans[0]));
        assert!((level - voice::VOICE_TARGET_DB).abs() < 2.0, "at the dialogue level: {level}");
    }

    #[test]
    fn a_premade_voice_at_another_speed_is_stretched() {
        let d = dir("speed");
        let text = "Lovely day for a riddle.";
        let v = VoiceArgs { line: Some("idle2".into()), text: text.into(), speed: Some(0.8), ..VoiceArgs::default() };
        let slow = process(&args(&d, v.clone(), vec![take(&d, "a", text)])).unwrap();
        let even = process(&args(&d, VoiceArgs { speed: None, ..v }, vec![take(&d, "a", text)])).unwrap();
        let s = |r: &Value| r["candidates"][0]["seconds"].as_f64().unwrap();
        assert!(s(&slow) > s(&even) * 1.15, "{} vs {}", s(&slow), s(&even));
    }

    #[test]
    fn previews_are_oggs_to_listen_to_and_a_line_needs_words() {
        let d = dir("preview");
        let v = VoiceArgs { preview: true, ..VoiceArgs::default() };
        let r = process(&args(&d, v, vec![take(&d, "p1", "Hello there, traveller."), take(&d, "p2", "Hello there, traveller.")])).unwrap();
        assert_eq!(r["candidates"][1]["files"][0]["path"], "preview.ogg");
        assert!(r["candidates"][0]["pitch_hz"].is_number());
        assert!(d.join("out/2/preview.ogg").is_file() && d.join("out/2/reference.ogg").is_file());
        let none = VoiceArgs { line: Some("x".into()), text: "[smug]".into(), ..VoiceArgs::default() };
        assert!(process(&args(&d, none, vec![take(&d, "a", "hi")])).unwrap_err().contains("no words"));
        assert!(process(&args(&d, VoiceArgs::default(), vec![take(&d, "a", "hi")])).is_err());
    }
}
