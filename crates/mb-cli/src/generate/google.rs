//! Google's models through the Interactions API: the Gemini API
//! (`generativelanguage.googleapis.com`, an API key) or Vertex AI
//! (`aiplatform.googleapis.com`, a Google Cloud project and an OAuth token
//! from gcloud). Images: Nano Banana (`gemini-nano-banana-2.1` and
//! friends). Music: Lyria 3 / 3.5, and on Vertex also `lyria-002` through
//! its `predict` endpoint (seed and negative prompt, WAV).
//!
//! Docs: https://ai.google.dev/gemini-api/docs/image-generation,
//! https://ai.google.dev/gemini-api/docs/music-generation,
//! https://cloud.google.com/vertex-ai/generative-ai/docs/music/generate-music

use serde_json::{Value, json};

use super::provider::{AudioRequest, ImageRequest, Output, Provider, Transport, b64, http_error, unb64};

enum Auth {
    ApiKey(String),
    Vertex { project: String, location: String },
}

pub struct Google {
    auth: Auth,
    http: Box<dyn Transport>,
}

impl Google {
    pub fn gemini(key: String, http: Box<dyn Transport>) -> Google {
        Google { auth: Auth::ApiKey(key), http }
    }

    pub fn vertex(project: String, location: String, http: Box<dyn Transport>) -> Google {
        Google { auth: Auth::Vertex { project, location }, http }
    }

    fn label(&self) -> &'static str {
        match self.auth {
            Auth::ApiKey(_) => "gemini",
            Auth::Vertex { .. } => "vertex",
        }
    }

    fn interactions_url(&self) -> String {
        match &self.auth {
            Auth::ApiKey(_) => "https://generativelanguage.googleapis.com/v1beta/interactions".into(),
            Auth::Vertex { project, location } => {
                let host = if location == "global" { "aiplatform.googleapis.com".to_string() } else { format!("{location}-aiplatform.googleapis.com") };
                format!("https://{host}/v1beta1/projects/{project}/locations/{location}/interactions")
            }
        }
    }

    fn headers(&self) -> Result<Vec<(String, String)>, String> {
        Ok(match &self.auth {
            Auth::ApiKey(k) => vec![("x-goog-api-key".into(), k.clone())],
            Auth::Vertex { .. } => vec![("Authorization".into(), format!("Bearer {}", vertex_token()?))],
        })
    }

    fn interact(&self, body: Value) -> Result<Value, String> {
        let r = self.http.post(&self.interactions_url(), &self.headers()?, "application/json", serde_json::to_vec(&body).unwrap())?;
        if r.status != 200 {
            return Err(http_error(self.label(), &r));
        }
        serde_json::from_slice(&r.body).map_err(|e| format!("{}: unreadable response: {e}", self.label()))
    }
}

/// An OAuth token for Vertex AI: VERTEX_ACCESS_TOKEN, else `gcloud auth print-access-token`.
fn vertex_token() -> Result<String, String> {
    if let Ok(t) = std::env::var("VERTEX_ACCESS_TOKEN") {
        return Ok(t.trim().to_string());
    }
    let out = if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "gcloud", "auth", "print-access-token"]).output()
    } else {
        std::process::Command::new("gcloud").args(["auth", "print-access-token"]).output()
    };
    match out {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        _ => Err("vertex: no access token; install the gcloud CLI and run `gcloud auth login` (or set VERTEX_ACCESS_TOKEN)".into()),
    }
}

/// The image or audio blocks of an interaction, as (type, base64 data, mime type),
/// plus any text. Handles both response shapes: `steps[].content[]` (Gemini API
/// since May 2026) and `outputs[]` (Vertex's documented shape).
pub fn media_blocks(v: &Value) -> (Vec<(String, String, String)>, Vec<String>) {
    let mut media = Vec::new();
    let mut text = Vec::new();
    let mut take = |c: &Value| {
        let ty = c["type"].as_str().unwrap_or("");
        match ty {
            "image" | "audio" => {
                if let Some(d) = c["data"].as_str() {
                    let mime = c["mime_type"].as_str().or_else(|| c["mimeType"].as_str()).unwrap_or("").to_string();
                    media.push((ty.to_string(), d.to_string(), mime));
                }
            }
            "text" => {
                if let Some(t) = c["text"].as_str() {
                    text.push(t.to_string());
                }
            }
            _ => {}
        }
    };
    if let Some(steps) = v["steps"].as_array() {
        for s in steps.iter().filter(|s| s["type"] == "model_output") {
            for c in s["content"].as_array().into_iter().flatten() {
                take(c);
            }
        }
    }
    for c in v["outputs"].as_array().into_iter().flatten() {
        take(c);
    }
    (media, text)
}

fn failure(label: &str, v: &Value, what: &str, text: &[String]) -> String {
    let status = v["status"].as_str().unwrap_or("?");
    let err = v["error"]["message"].as_str().unwrap_or("");
    let said = text.join(" ");
    let said: String = said.chars().take(400).collect();
    format!(
        "{label}: no {what} in the response (status {status}){}{}. A safety filter may have blocked the prompt: describe the subject differently.",
        if err.is_empty() { String::new() } else { format!(": {err}") },
        if said.is_empty() { String::new() } else { format!("; the model said: {said}") }
    )
}

/// The image size Google is asked for (and bills): Nano Banana 2.1 has no
/// 0.5K tier, and the Lite model makes 1K only.
fn billed_size(req: &ImageRequest) -> &str {
    let size = if req.resolution == "0.5K" && req.model != "gemini-3.1-flash-image" { "1K" } else { req.resolution.as_str() };
    if req.model == "gemini-3.1-flash-lite-image" { "1K" } else { size }
}

/// What a call costs by Google's price list: Google's responses don't say,
/// but these models are billed per image or per call at a fixed price, so
/// the list price for what was made is the charge (input tokens for the
/// prompt and references, a fraction of a cent, aren't counted).
fn list_price(label: &str, model: &str, size: &str) -> Option<f64> {
    super::provider::find_model(label, model).and_then(|m| super::provider::estimate(m, size, 1, 0.0))
}

pub fn image_body(req: &ImageRequest) -> Value {
    let mut input = vec![json!({ "type": "text", "text": req.prompt })];
    for r in &req.refs {
        input.push(json!({ "type": "image", "mime_type": r.mime, "data": b64(&r.bytes) }));
    }
    let size = billed_size(req);
    let mut body = json!({
        "model": req.model,
        "input": input,
        "response_format": { "type": "image", "mime_type": "image/jpeg", "aspect_ratio": req.aspect, "image_size": size },
        "store": false,
    });
    if let Some(s) = req.seed {
        body["generation_config"] = json!({ "seed": s as i64 & 0x7fff_ffff });
    }
    body
}

pub fn music_body(req: &AudioRequest) -> Value {
    let mut body = json!({ "model": req.model, "input": req.prompt, "store": false });
    if req.model == "lyria-3.5" {
        body["response_format"] = json!({ "type": "audio" }); // WAV (the default is MP3)
    }
    if let Some(s) = req.seed {
        body["generation_config"] = json!({ "seed": s as i64 & 0x7fff_ffff });
    }
    body
}

impl Provider for Google {
    fn name(&self) -> &'static str {
        self.label()
    }

    fn image(&self, req: &ImageRequest) -> Result<Output, String> {
        let v = self.interact(image_body(req))?;
        let (media, text) = media_blocks(&v);
        // The last image is the final one (thinking models may send drafts first).
        let Some((_, data, mime)) = media.into_iter().rev().find(|m| m.0 == "image") else {
            return Err(failure(self.label(), &v, "image", &text));
        };
        let cost_usd = list_price(self.label(), &req.model, billed_size(req));
        Ok(Output { bytes: unb64(&data)?, mime, text: (!text.is_empty()).then(|| text.join("\n")), cost_usd })
    }

    fn audio(&self, req: &AudioRequest) -> Result<Output, String> {
        if req.model == "lyria-002" {
            return self.lyria2(req);
        }
        if !req.model.starts_with("lyria") {
            return Err(format!("{}: {} isn't a music model (try lyria-3-clip-preview)", self.label(), req.model));
        }
        let v = self.interact(music_body(req))?;
        let (media, text) = media_blocks(&v);
        let Some((_, data, mime)) = media.into_iter().rev().find(|m| m.0 == "audio") else {
            return Err(failure(self.label(), &v, "audio", &text));
        };
        let mime = if mime.is_empty() { "audio/mpeg".into() } else { mime };
        let cost_usd = list_price(self.label(), &req.model, "");
        Ok(Output { bytes: unb64(&data)?, mime, text: (!text.is_empty()).then(|| text.join("\n")), cost_usd })
    }
}

impl Google {
    /// Lyria 2 on Vertex AI: `predict`, regional, WAV at 48 kHz.
    fn lyria2(&self, req: &AudioRequest) -> Result<Output, String> {
        let Auth::Vertex { project, location } = &self.auth else {
            return Err("lyria-002 is only on Vertex AI: use --provider vertex".into());
        };
        let loc = if location == "global" { "us-central1" } else { location };
        let url = format!("https://{loc}-aiplatform.googleapis.com/v1/projects/{project}/locations/{loc}/publishers/google/models/lyria-002:predict");
        let mut inst = json!({ "prompt": req.prompt });
        if let Some(n) = &req.negative {
            inst["negative_prompt"] = json!(n);
        }
        if let Some(s) = req.seed {
            inst["seed"] = json!(s as i64 & 0x7fff_ffff);
        }
        let body = json!({ "instances": [inst], "parameters": {} });
        let r = self.http.post(&url, &self.headers()?, "application/json", serde_json::to_vec(&body).unwrap())?;
        if r.status != 200 {
            return Err(http_error("vertex", &r));
        }
        let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("vertex: unreadable response: {e}"))?;
        let p = &v["predictions"][0];
        let data = p["audioContent"].as_str().or_else(|| p["bytesBase64Encoded"].as_str()).ok_or_else(|| failure("vertex", &v, "audio", &[]))?;
        Ok(Output { bytes: unb64(data)?, mime: p["mimeType"].as_str().unwrap_or("audio/wav").to_string(), text: None, cost_usd: list_price("vertex", "lyria-002", "") })
    }
}

#[cfg(test)]
mod tests {
    use super::super::audio::Kind;
    use super::super::provider::tests::{Fake, json as reply};
    use super::super::provider::{MockHint, RefImage};
    use super::*;
    use std::rc::Rc;

    fn req() -> ImageRequest {
        ImageRequest {
            model: "gemini-nano-banana-2.1".into(),
            prompt: "a moth".into(),
            refs: vec![RefImage { bytes: vec![1, 2, 3], mime: "image/png".into() }],
            aspect: "9:16".into(),
            resolution: "0.5K".into(),
            transparent: true,
            seed: Some(42),
            quality: 1,
            hint: MockHint::default(),
        }
    }

    #[test]
    fn image_request_and_response() {
        let fake = Rc::new(Fake::new(vec![reply(
            200,
            json!({ "status": "completed", "steps": [
                { "type": "thought" },
                { "type": "model_output", "content": [
                    { "type": "text", "text": "Here is your moth." },
                    { "type": "image", "mime_type": "image/png", "data": b64(b"draft") },
                    { "type": "image", "mime_type": "image/png", "data": b64(b"final") }
                ]}
            ]}),
        )]));
        let g = Google::gemini("KEY".into(), Box::new(fake.clone()));
        let out = g.image(&req()).unwrap();
        assert_eq!(out.bytes, b"final");
        assert_eq!(out.text.as_deref(), Some("Here is your moth."));
        assert_eq!(out.cost_usd, Some(0.0336), "the list price of the 1K image it billed");
        let sent = fake.sent.borrow();
        let (url, headers, body) = &sent[0];
        assert_eq!(url, "https://generativelanguage.googleapis.com/v1beta/interactions");
        assert_eq!(headers[0], ("x-goog-api-key".to_string(), "KEY".to_string()));
        assert_eq!(body["model"], "gemini-nano-banana-2.1");
        assert_eq!(body["input"][0]["text"], "a moth");
        assert_eq!(body["input"][1]["type"], "image");
        assert_eq!(body["input"][1]["data"], b64(&[1, 2, 3]));
        assert_eq!(body["response_format"]["aspect_ratio"], "9:16");
        assert_eq!(body["response_format"]["image_size"], "1K", "0.5K maps to 1K on 2.1");
        assert_eq!(body["generation_config"]["seed"], 42);
        assert_eq!(body["store"], false);
    }

    #[test]
    fn vertex_shape_and_refusals() {
        let v = json!({ "status": "completed", "outputs": [ { "type": "text", "text": "lyrics" }, { "type": "audio", "mime_type": "audio/mpeg", "data": b64(b"mp3") } ] });
        let (m, t) = media_blocks(&v);
        assert_eq!(m, vec![("audio".to_string(), b64(b"mp3"), "audio/mpeg".to_string())]);
        assert_eq!(t, vec!["lyrics"]);
        let fake = Rc::new(Fake::new(vec![reply(
            200,
            json!({ "status": "completed", "steps": [ { "type": "model_output", "content": [ { "type": "text", "text": "I can't make that." } ] } ] }),
        )]));
        let g = Google::gemini("KEY".into(), Box::new(fake));
        let e = g.image(&req()).err().unwrap();
        assert!(e.contains("no image") && e.contains("I can't make that."), "{e}");
    }

    #[test]
    fn music_request() {
        let fake =
            Rc::new(Fake::new(vec![reply(200, json!({ "steps": [ { "type": "model_output", "content": [ { "type": "audio", "data": b64(b"ID3") } ] } ] }))]));
        let g = Google::gemini("KEY".into(), Box::new(fake.clone()));
        let r = AudioRequest {
            model: "lyria-3-clip-preview".into(),
            prompt: "chiptune".into(),
            kind: Kind::Music,
            seconds: None,
            looping: true,
            seed: None,
            negative: None,
        };
        let out = g.audio(&r).unwrap();
        assert_eq!(out.mime, "audio/mpeg");
        assert_eq!(out.cost_usd, Some(0.04));
        let body = &fake.sent.borrow()[0].2;
        assert_eq!(body["input"], "chiptune");
        assert!(body.get("response_format").is_none());
        assert!(g.audio(&AudioRequest { model: "lyria-002".into(), ..r }).err().unwrap().contains("Vertex"));
    }

    #[test]
    fn http_errors_surface() {
        let fake = Rc::new(Fake::new(vec![reply(400, json!({ "error": { "message": "aspect_ratio 7:3 is not supported" } }))]));
        let g = Google::gemini("KEY".into(), Box::new(fake));
        assert_eq!(g.image(&req()).err().unwrap(), "gemini: HTTP 400: aspect_ratio 7:3 is not supported");
    }
}
