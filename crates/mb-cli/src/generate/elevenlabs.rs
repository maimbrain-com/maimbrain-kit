//! ElevenLabs (optional provider): sound effects (`/v1/sound-generation`,
//! 0.5–30 s, optionally made to loop) and music (`/v1/music`, an exact
//! length, instrumental). Responses are the audio itself.
//! https://elevenlabs.io/docs/api-reference/text-to-sound-effects/convert,
//! https://elevenlabs.io/docs/api-reference/music/compose

use serde_json::json;

use super::audio::Kind;
use super::provider::{AudioRequest, ImageRequest, Output, Provider, Response, Transport, http_error, multipart, unb64};
use super::voice::{AccountVoice, Alignment, DesignRequest, Preview, RefAudio, Slots, Speech, SpeechRequest, VoiceProvider, api_f, speed_native};

/// Where an ElevenLabs key's permissions are set.
pub const KEY_SETTINGS: &str = "https://elevenlabs.io/app/settings/api-keys";

pub struct ElevenLabs {
    key: String,
    http: Box<dyn Transport>,
    slots: Option<std::path::PathBuf>,
}

/// Where mb runs on this machine queue for the account's concurrency limit.
pub fn slots_dir() -> Option<std::path::PathBuf> {
    crate::remote::config_dir().ok().map(|d| d.join("elevenlabs-slots"))
}

/// How many generation requests an account runs at once (3 on the plan
/// builders hit; MB_ELEVENLABS_CONCURRENCY for another plan's).
pub fn concurrency() -> usize {
    std::env::var("MB_ELEVENLABS_CONCURRENCY").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(3)
}

impl ElevenLabs {
    pub fn new(key: String, http: Box<dyn Transport>) -> ElevenLabs {
        ElevenLabs { key, http, slots: None }
    }

    /// Queues generation requests with other `mb` runs through lock files in `dir`.
    pub fn with_slots(mut self, dir: Option<std::path::PathBuf>) -> ElevenLabs {
        self.slots = dir;
        self
    }

    /// A generation request (speech, a design, a sound): holds one of the account's concurrency slots while it runs.
    fn post(&self, path: &str, format: &str, body: &serde_json::Value) -> Result<Response, String> {
        let _slot = super::provider::take_slot(self.slots.as_deref(), "requests", concurrency(), std::time::Duration::from_secs(600), "elevenlabs");
        let url = format!("https://api.elevenlabs.io{path}?output_format={format}");
        self.http.post(&url, &[("xi-api-key".to_string(), self.key.clone())], "application/json", serde_json::to_vec(body).unwrap())
    }

    fn headers(&self) -> Vec<(String, String)> {
        vec![("xi-api-key".to_string(), self.key.clone())]
    }

    /// A GET of JSON; `doing` names the call in errors ("read your voice slots").
    fn get_json(&self, path: &str, doing: &str) -> Result<serde_json::Value, String> {
        let r = self.http.get(&format!("https://api.elevenlabs.io{path}"), &self.headers())?;
        if r.status != 200 {
            return Err(el_error(&r, doing));
        }
        serde_json::from_slice(&r.body).map_err(|e| format!("elevenlabs: {doing}: {e}"))
    }
}

/// An ElevenLabs error, with a hint for what stops builders: a key missing a
/// permission (they're per feature: `voices_read`, `voices_write`,
/// `user_read`, `text_to_speech`…) says which one and where to turn it on.
pub fn el_error(r: &Response, doing: &str) -> String {
    let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap_or_default();
    let status = v["detail"]["status"].as_str().unwrap_or("");
    let msg = v["detail"]["message"].as_str().unwrap_or("");
    if status == "missing_permissions" || msg.contains("missing the permission") {
        let perm = permission_in(msg).unwrap_or("the one this call needs");
        let label = match perm {
            "voices_read" => " (Voices: Read)",
            "voices_write" => " (Voices: Write)",
            "user_read" => " (User: Read)",
            "text_to_speech" => " (Text to Speech)",
            _ => "",
        };
        return format!(
            "elevenlabs: this key can't {doing}: it's missing the `{perm}` permission{label}.\n  \
             ElevenLabs keys have per-feature permissions: turn it on for this key at {KEY_SETTINGS} (or make a key with it and `mb keys set elevenlabs`)"
        );
    }
    if status == "voice_not_found" {
        return format!("elevenlabs: voice_not_found: {msg}");
    }
    let lower = msg.to_lowercase();
    if r.status == 429 && (status.contains("concurrent") || lower.contains("concurrent")) {
        return format!(
            "elevenlabs: too many requests at once: {msg} (mb runs {} at a time across this machine; set MB_ELEVENLABS_CONCURRENCY to your plan's limit)",
            concurrency()
        );
    }
    if lower.contains("add/edit") || lower.contains("voice_add_edit") || status.contains("add_edit") {
        return format!(
            "elevenlabs: this account has used all its voice adds and edits for the month, so it can't {doing} until they reset \
             (`mb voice slots` shows the count): {msg}"
        );
    }
    if status == "voice_limit_reached" || status.contains("voice_limit") || lower.contains("maximum amount of custom voices") || lower.contains("custom voice limit") {
        return format!("no free voice slot: run `mb voice slots` (elevenlabs couldn't {doing}: {msg})");
    }
    http_error("elevenlabs", r)
}

/// Whether an error is a missing key permission (callers that only wanted to look can carry on).
pub fn is_permission_error(e: &str) -> bool {
    e.contains("permission") && e.contains(KEY_SETTINGS)
}

/// "…missing the permission voices_read to execute…" → voices_read.
fn permission_in(msg: &str) -> Option<&str> {
    let rest = &msg[msg.find("permission")? + "permission".len()..];
    let rest = rest.trim_start_matches(|c: char| c == 's' || c == ':' || c.is_whitespace());
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// Query-string escaping for a page token.
fn escape(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn account_voice(v: &serde_json::Value) -> AccountVoice {
    AccountVoice {
        id: v["voice_id"].as_str().unwrap_or("").to_string(),
        name: v["name"].as_str().unwrap_or("").to_string(),
        category: v["category"].as_str().unwrap_or("").to_string(),
        labels: v["labels"].as_object().map(|m| m.iter().filter_map(|(k, x)| x.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default(),
        description: v["description"].as_str().unwrap_or("").to_string(),
        created_unix: v["created_at_unix"].as_i64(),
    }
}

fn mime_for(format: &str, content_type: &str) -> String {
    if let Some(rate) = format.strip_prefix("pcm_") {
        format!("audio/pcm;rate={rate}")
    } else if content_type.starts_with("audio/") {
        content_type.split(';').next().unwrap_or("audio/mpeg").to_string()
    } else {
        "audio/mpeg".into()
    }
}

impl Provider for ElevenLabs {
    fn name(&self) -> &'static str {
        "elevenlabs"
    }

    fn image(&self, _req: &ImageRequest) -> Result<Output, String> {
        Err("elevenlabs makes sound, not images; use --provider gemini or openai".into())
    }

    fn audio(&self, req: &AudioRequest) -> Result<Output, String> {
        let (path, body) = match req.kind {
            Kind::Sfx => {
                let mut b = json!({ "text": req.prompt, "model_id": req.model, "prompt_influence": 0.5, "loop": req.looping });
                if let Some(s) = req.seconds {
                    b["duration_seconds"] = json!(s.clamp(0.5, 30.0));
                }
                ("/v1/sound-generation", b)
            }
            Kind::Music => {
                let mut b = json!({ "prompt": req.prompt, "model_id": req.model, "force_instrumental": true });
                if let Some(s) = req.seconds {
                    b["music_length_ms"] = json!(((s * 1000.0) as u32).clamp(3000, 600_000));
                }
                ("/v1/music", b)
            }
        };
        // Raw PCM where the plan allows it (no MP3 decoding, no encoder delay), else MP3.
        let mut format = if req.kind == Kind::Sfx { "pcm_24000" } else { "mp3_44100_128" };
        let mut r = self.post(path, format, &body)?;
        if req.kind == Kind::Sfx && (r.status == 403 || r.status == 422 || r.status == 400) && String::from_utf8_lossy(&r.body).contains("output_format") {
            format = "mp3_44100_128";
            r = self.post(path, format, &body)?;
        }
        if r.status != 200 {
            // A music prompt it won't make (bad_prompt) comes with a rewrite it would.
            let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap_or_default();
            let suggestion = v["detail"]["data"]["prompt_suggestion"].as_str().map(|s| format!("; elevenlabs suggests: \"{s}\"")).unwrap_or_default();
            return Err(format!("{}{suggestion}", http_error("elevenlabs", &r)));
        }
        let seconds = req.seconds.unwrap_or(if req.kind == Kind::Sfx { 2.0 } else { 30.0 });
        let per_min = if req.kind == Kind::Sfx { 0.12 } else { 0.15 };
        Ok(Output { mime: mime_for(format, &r.content_type), bytes: r.body, text: None, cost_usd: Some(per_min * seconds as f64 / 60.0) })
    }
}

/// Voices: Voice Design (`/v1/text-to-voice/design` → previews, then
/// `/v1/text-to-voice` keeps one) and text to speech with character timings
/// (`/v1/text-to-speech/{voice}/with-timestamps`), which the typewriter text
/// and the lip flap are timed by.
/// https://elevenlabs.io/docs/api-reference/text-to-voice/design,
/// https://elevenlabs.io/docs/api-reference/text-to-voice/create,
/// https://elevenlabs.io/docs/api-reference/text-to-speech/convert-with-timestamps
impl VoiceProvider for ElevenLabs {
    fn design(&self, req: &DesignRequest) -> Result<(Vec<Preview>, String), String> {
        let mut b = json!({ "voice_description": req.description, "model_id": req.model });
        match &req.text {
            Some(t) => b["text"] = json!(t),
            None => b["auto_generate_text"] = json!(true),
        }
        if let Some(s) = req.seed {
            b["seed"] = json!(s % 4_294_967_295);
        }
        let mut pcm = true;
        let mut r = self.post("/v1/text-to-voice/design", "pcm_24000", &b)?;
        if r.status != 200 && String::from_utf8_lossy(&r.body).contains("output_format") {
            pcm = false;
            r = self.post("/v1/text-to-voice/design", "mp3_44100_128", &b)?;
        }
        if r.status != 200 {
            return Err(el_error(&r, "design voices"));
        }
        let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|e| format!("elevenlabs: voice design reply: {e}"))?;
        let mut out = Vec::new();
        for p in v["previews"].as_array().cloned().unwrap_or_default() {
            let bytes = unb64(p["audio_base_64"].as_str().unwrap_or(""))?;
            // What came back: asked for raw PCM, an MP3 if the plan doesn't allow it.
            let mime = if bytes.starts_with(b"RIFF") {
                "audio/wav"
            } else if pcm {
                "audio/pcm;rate=24000"
            } else {
                "audio/mpeg"
            }
            .to_string();
            out.push(Preview { id: p["generated_voice_id"].as_str().unwrap_or("").to_string(), bytes, mime });
        }
        if out.is_empty() {
            return Err("elevenlabs: voice design returned no previews".into());
        }
        Ok((out, v["text"].as_str().unwrap_or("").to_string()))
    }

    fn create(&self, generated_id: &str, name: &str, description: &str) -> Result<String, String> {
        let b = json!({ "voice_name": name, "voice_description": description, "generated_voice_id": generated_id });
        let url = "https://api.elevenlabs.io/v1/text-to-voice";
        let r = self.http.post(url, &[("xi-api-key".to_string(), self.key.clone())], "application/json", serde_json::to_vec(&b).unwrap())?;
        if r.status != 200 {
            let e = el_error(&r, "keep a designed voice");
            let ours = e.starts_with("no free voice slot") || is_permission_error(&e) || e.contains("adds and edits");
            return Err(if ours { e } else { e + " (previews expire after a while: design the voice again if it's old)" });
        }
        let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|e| format!("elevenlabs: create voice reply: {e}"))?;
        v["voice_id"].as_str().map(str::to_string).ok_or("elevenlabs: the created voice has no voice_id".into())
    }

    fn speak(&self, req: &SpeechRequest) -> Result<Speech, String> {
        // Two decimals: an f32 0.6 would go as 0.6000000238418579, which Flash refuses.
        let mut settings = json!({ "stability": api_f(req.stability), "similarity_boost": api_f(req.similarity), "use_speaker_boost": true });
        if let Some(s) = req.style {
            settings["style"] = json!(api_f(s));
        }
        // voice_settings.speed (0.7–1.2); Eleven v3 and v4 ignore it, so mb time-stretches their lines instead (speak_line).
        if let Some(s) = req.speed
            && speed_native(&req.model)
        {
            settings["speed"] = json!(api_f(s.clamp(0.7, 1.2)));
        }
        let mut b = json!({ "text": req.text, "model_id": req.model, "voice_settings": settings });
        if let Some(s) = req.seed {
            b["seed"] = json!(s % 4_294_967_295);
        }
        let path = format!("/v1/text-to-speech/{}/with-timestamps", req.voice_id);
        let mut format = "pcm_24000";
        let mut r = self.post(&path, format, &b)?;
        if (r.status == 403 || r.status == 422 || r.status == 400) && String::from_utf8_lossy(&r.body).contains("output_format") {
            format = "mp3_44100_128";
            r = self.post(&path, format, &b)?;
        }
        if r.status != 200 {
            return Err(el_error(&r, "speak lines"));
        }
        let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|e| format!("elevenlabs: speech reply: {e}"))?;
        let bytes = unb64(v["audio_base64"].as_str().ok_or("elevenlabs: the speech reply has no audio_base64")?)?;
        let al = &v["alignment"];
        let alignment = al["characters"].as_array().map(|c| Alignment {
            chars: c.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect(),
            start: al["character_start_times_seconds"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default(),
            end: al["character_end_times_seconds"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default(),
        });
        let price = if req.model.contains("flash") || req.model.contains("turbo") { 0.04 } else { 0.08 };
        Ok(Speech { mime: mime_for(format, "audio/mpeg"), bytes, alignment, cost_usd: Some(price * req.text.chars().count() as f64 / 1000.0) })
    }

    /// `GET /v1/user/subscription` (permission `user_read`).
    /// https://elevenlabs.io/docs/api-reference/user/subscription/get
    fn slots(&self) -> Result<Slots, String> {
        let v = self.get_json("/v1/user/subscription", "read your voice slots")?;
        let n = |k: &str| v[k].as_u64().map(|x| x as u32);
        Ok(Slots {
            tier: v["tier"].as_str().unwrap_or("").to_string(),
            used: n("voice_slots_used").ok_or("elevenlabs: the subscription reply has no voice_slots_used")?,
            limit: n("voice_limit").ok_or("elevenlabs: the subscription reply has no voice_limit")?,
            add_edits: n("voice_add_edit_counter").zip(n("max_voice_add_edits")),
            can_clone: v["can_use_instant_voice_cloning"].as_bool(),
        })
    }

    /// `GET /v2/voices` (permission `voices_read`), every page: the account's
    /// own voices (`voice_type=non-default`: designed, cloned and added from
    /// the library, the ones that take slots) or the premade ones.
    /// https://elevenlabs.io/docs/api-reference/voices/search
    fn voices(&self, premade: bool) -> Result<Vec<AccountVoice>, String> {
        let filter = if premade { "category=premade" } else { "voice_type=non-default" };
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..50 {
            let mut path = format!("/v2/voices?page_size=100&{filter}&include_total_count=false");
            if let Some(t) = &token {
                path += &format!("&next_page_token={}", escape(t));
            }
            let v = self.get_json(&path, "list your voices")?;
            out.extend(v["voices"].as_array().into_iter().flatten().map(account_voice).filter(|a| !a.id.is_empty()));
            token = v["next_page_token"].as_str().map(str::to_string).filter(|t| !t.is_empty());
            if !v["has_more"].as_bool().unwrap_or(false) || token.is_none() {
                break;
            }
        }
        if !premade {
            out.retain(|a| a.category != "premade");
        }
        Ok(out)
    }

    /// `POST /v1/voices/add` (multipart: `name`, `description`, `files`;
    /// permission `voices_write`). An Instant Voice Clone takes a slot like
    /// any other voice, and counts as a voice added this month.
    /// https://elevenlabs.io/docs/api-reference/voices/ivc/create
    fn clone_voice(&self, name: &str, description: &str, files: &[RefAudio]) -> Result<String, String> {
        let parts: Vec<(&str, &str, &str, &[u8])> = files.iter().map(|f| ("files", f.name.as_str(), f.mime.as_str(), f.bytes.as_slice())).collect();
        let (ct, body) = multipart(&[("name", name), ("description", description), ("remove_background_noise", "false")], &parts);
        let r = self.http.post("https://api.elevenlabs.io/v1/voices/add", &self.headers(), &ct, body)?;
        if r.status != 200 {
            return Err(el_error(&r, "recreate a voice (Instant Voice Cloning)"));
        }
        let v: serde_json::Value = serde_json::from_slice(&r.body).map_err(|e| format!("elevenlabs: voice clone reply: {e}"))?;
        if v["requires_verification"].as_bool() == Some(true) {
            eprintln!("  note: ElevenLabs says the recreated voice needs verification before it can speak");
        }
        v["voice_id"].as_str().map(str::to_string).ok_or("elevenlabs: the recreated voice has no voice_id".into())
    }

    /// `DELETE /v1/voices/{id}` (permission `voices_write`).
    /// https://elevenlabs.io/docs/api-reference/voices/delete
    fn delete_voice(&self, id: &str) -> Result<(), String> {
        let r = self.http.delete(&format!("https://api.elevenlabs.io/v1/voices/{}", escape(id)), &self.headers())?;
        if r.status != 200 {
            return Err(el_error(&r, "delete voices"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::tests::{Fake, json as reply};
    use super::*;
    use std::rc::Rc;

    fn audio(status: u16, body: &[u8]) -> Response {
        Response { status, content_type: "audio/mpeg".into(), body: body.to_vec() }
    }

    #[test]
    fn sound_effect_request_with_mp3_fallback() {
        let fake =
            Rc::new(Fake::new(vec![reply(403, json!({ "detail": { "message": "output_format pcm_24000 requires a higher tier" } })), audio(200, b"ID3...")]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let req = AudioRequest {
            model: "eleven_text_to_sound_v2".into(),
            prompt: "a glassy chime".into(),
            kind: Kind::Sfx,
            seconds: Some(0.2),
            looping: false,
            seed: None,
            negative: None,
        };
        let out = e.audio(&req).unwrap();
        assert_eq!(out.mime, "audio/mpeg");
        let sent = fake.sent.borrow();
        assert!(sent[0].0.ends_with("/v1/sound-generation?output_format=pcm_24000"));
        assert!(sent[1].0.ends_with("output_format=mp3_44100_128"));
        assert_eq!(sent[0].1[0], ("xi-api-key".to_string(), "xi".to_string()));
        assert_eq!(sent[0].2["duration_seconds"], 0.5, "clamped to the API's minimum");
        assert_eq!(sent[0].2["text"], "a glassy chime");
    }

    #[test]
    fn requests_queue_for_the_accounts_three_slots() {
        use super::super::provider::take_slot;
        let dir = std::env::temp_dir().join(format!("mb-el-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let zero = std::time::Duration::ZERO;
        assert_eq!(concurrency(), 3);
        let held: Vec<_> = (0..3).map(|_| take_slot(Some(&dir), "requests", 3, zero, "elevenlabs")).collect();
        assert!(held.iter().all(|s| s.0.is_some()), "three free slots");
        assert!(take_slot(Some(&dir), "requests", 3, zero, "elevenlabs").0.is_none(), "a fourth waits (here: gives up at once)");
        drop(held);
        // (A moment of grace: a test elsewhere spawning a process can hold a copy of a lock file briefly.)
        let soon = std::time::Duration::from_secs(3);
        assert!(take_slot(Some(&dir), "requests", 3, soon, "elevenlabs").0.is_some());
        // A request holds a slot only while it runs.
        let fake = Rc::new(Fake::new(vec![audio(200, &[0, 0])]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake)).with_slots(Some(dir.clone()));
        let req = AudioRequest { model: "eleven_text_to_sound_v2".into(), prompt: "zap".into(), kind: Kind::Sfx, seconds: None, looping: false, seed: None, negative: None };
        e.audio(&req).unwrap();
        let all: Vec<_> = (0..3).map(|_| take_slot(Some(&dir), "requests", 3, soon, "elevenlabs")).collect();
        assert!(all.iter().all(|s| s.0.is_some()));
        let busy = reply(429, json!({ "detail": { "status": "too_many_concurrent_requests", "message": "Too many concurrent requests." } }));
        assert!(el_error(&busy, "speak lines").contains("MB_ELEVENLABS_CONCURRENCY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slot_limit_and_permission_errors_say_what_to_do() {
        let full = || {
            reply(
                400,
                json!({ "detail": { "status": "voice_limit_reached", "message": "You have reached your maximum amount of custom voices (10 / 10). You can upgrade your subscription to increase your custom voice limit." } }),
            )
        };
        let denied = |p: &str| reply(401, json!({ "detail": { "status": "missing_permissions", "message": format!("The API key you used is missing the permission {p} to execute this operation.") } }));
        let fake = Rc::new(Fake::new(vec![
            full(),
            full(),
            denied("voices_write"),
            denied("text_to_speech"),
            reply(400, json!({ "detail": { "status": "voice_add_edit_limit_reached", "message": "You have reached your monthly voice add/edit limit." } })),
            reply(404, json!({ "detail": { "status": "voice_not_found", "message": "A voice with the voice_id X was not found." } })),
        ]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let files = [RefAudio { name: "a.wav".into(), bytes: vec![1], mime: "audio/wav".into() }];
        let err = e.create("g1", "duel rocko", "smug").unwrap_err();
        assert!(err.starts_with("no free voice slot: run `mb voice slots`"), "{err}");
        assert!(!err.contains("previews expire"), "{err}");
        let err = super::super::voice_account::with_slot_help(e.clone_voice("mb temp duel rocko", "t", &files).unwrap_err(), std::path::Path::new("games/duel"), "rocko");
        assert!(err.starts_with("no free voice slot") && err.contains("mb voice release games/duel") && err.contains("--premade"), "{err}");
        let err = e.delete_voice("V1").unwrap_err();
        assert!(err.contains("can't delete voices: it's missing the `voices_write` permission (Voices: Write)") && err.contains(KEY_SETTINGS), "{err}");
        let req = SpeechRequest { model: "eleven_v4".into(), voice_id: "V".into(), text: "Hi".into(), stability: 0.5, similarity: 0.75, style: None, speed: None, seed: None };
        let err = e.speak(&req).err().unwrap();
        assert!(err.contains("`text_to_speech` permission (Text to Speech)"), "{err}");
        let err = e.clone_voice("x", "t", &files).unwrap_err();
        assert!(err.contains("voice adds and edits for the month"), "{err}");
        assert!(e.delete_voice("X").unwrap_err().contains("voice_not_found"));
    }

    #[test]
    fn pcm_mime_and_music() {
        let fake = Rc::new(Fake::new(vec![audio(200, &[0, 0]), audio(200, b"ID3")]));
        let e = ElevenLabs::new("xi".into(), Box::new(fake.clone()));
        let mut req = AudioRequest {
            model: "eleven_text_to_sound_v2".into(),
            prompt: "zap".into(),
            kind: Kind::Sfx,
            seconds: None,
            looping: true,
            seed: None,
            negative: None,
        };
        assert_eq!(e.audio(&req).unwrap().mime, "audio/pcm;rate=24000");
        req.kind = Kind::Music;
        req.model = "music_v2_5".into();
        req.seconds = Some(16.0);
        e.audio(&req).unwrap();
        let body = &fake.sent.borrow()[1].2;
        assert_eq!(body["music_length_ms"], 16000);
        assert_eq!(body["force_instrumental"], true);
    }
}
