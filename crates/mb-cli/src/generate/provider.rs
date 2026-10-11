//! The provider layer: what a generation request and result look like, the
//! model catalog with documented prices, an HTTP transport that tests can
//! replace, and `pick`, which turns a provider name into an implementation.
//!
//! Providers: `gemini` (Google's Gemini API: Nano Banana images, Lyria
//! music), `vertex` (the same Google models through Vertex AI, plus
//! lyria-002), `openai` (GPT Image), `elevenlabs` (sound effects, music),
//! `tripo` and `meshy` (3D models) and `mock` (offline and deterministic,
//! for trying the pipeline and for tests).

use std::time::Duration;

use super::audio::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Image,
    Music,
    Sfx,
    /// 3D models (mb model).
    Model,
    /// Speech and designed voices (mb voice).
    Voice,
}

#[derive(Clone, Copy, Debug)]
pub enum Price {
    /// Per image, by resolution tier ("0.5K", "1K", "2K", "4K").
    PerImage(&'static [(&'static str, f64)]),
    /// Per call (a clip or a song).
    PerCall(f64),
    /// Per minute of audio requested.
    PerMinute(f64),
    /// Per 1,000 characters of text spoken (text to speech).
    PerKChars(f64),
    /// Billed by image output tokens: (USD per 1M tokens, rough tokens per 1K image at low/medium/high).
    ImageTokens(f64, [u32; 3]),
    /// A 3D model, USD by step: the mesh, its texture, from an image instead of text (extra), a rig, each animation.
    Model3d { mesh: f64, texture: f64, image: f64, rig: f64, animation: f64 },
    Free,
}

pub struct Model {
    pub provider: &'static str,
    pub id: &'static str,
    pub media: Media,
    pub price: Price,
    pub seed: bool,
    pub default: bool,
    pub note: &'static str,
}

/// Prices from the providers' pricing pages as of October 2026:
/// https://ai.google.dev/gemini-api/docs/pricing,
/// https://cloud.google.com/vertex-ai/generative-ai/pricing,
/// https://developers.openai.com/api/docs/pricing, https://elevenlabs.io/pricing/api
pub const MODELS: &[Model] = &[
    Model {
        provider: "gemini",
        id: "gemini-nano-banana-2.1",
        media: Media::Image,
        price: Price::PerImage(&[("1K", 0.0336), ("2K", 0.0504), ("4K", 0.113)]),
        seed: true,
        default: true,
        note: "Nano Banana 2.1: the workhorse; style references, edits, good text",
    },
    Model {
        provider: "gemini",
        id: "gemini-3.1-flash-lite-image",
        media: Media::Image,
        price: Price::PerImage(&[("1K", 0.0336)]),
        seed: true,
        default: false,
        note: "Nano Banana 2 Lite: fastest; 1K only; weak with several references",
    },
    Model {
        provider: "gemini",
        id: "gemini-3.1-flash-image",
        media: Media::Image,
        price: Price::PerImage(&[("0.5K", 0.045), ("1K", 0.067), ("2K", 0.101), ("4K", 0.151)]),
        seed: true,
        default: false,
        note: "Nano Banana 2: the only one with 512 px output",
    },
    Model {
        provider: "gemini",
        id: "gemini-3-pro-image",
        media: Media::Image,
        price: Price::PerImage(&[("1K", 0.134), ("2K", 0.134), ("4K", 0.24)]),
        seed: true,
        default: false,
        note: "Nano Banana Pro: best for hard briefs (many references, consistency); 4× the price",
    },
    Model {
        provider: "gemini",
        id: "lyria-3-clip-preview",
        media: Media::Music,
        price: Price::PerCall(0.04),
        seed: true,
        default: true,
        note: "Lyria 3 Clip: always 30 s, MP3 (44.1 kHz stereo); loops are cut from it",
    },
    Model {
        provider: "gemini",
        id: "lyria-3.5",
        media: Media::Music,
        price: Price::PerCall(0.08),
        seed: true,
        default: false,
        note: "Lyria 3.5: full songs (length set in the prompt), WAV; for longer loops",
    },
    Model {
        provider: "vertex",
        id: "gemini-nano-banana-2.1",
        media: Media::Image,
        price: Price::PerImage(&[("1K", 0.0336), ("2K", 0.0504), ("4K", 0.113)]),
        seed: true,
        default: true,
        note: "the Gemini image model through Vertex AI (Google Cloud credits apply)",
    },
    Model {
        provider: "vertex",
        id: "lyria-3-clip-preview",
        media: Media::Music,
        price: Price::PerCall(0.04),
        seed: true,
        default: true,
        note: "Lyria 3 Clip through Vertex AI",
    },
    Model {
        provider: "vertex",
        id: "lyria-002",
        media: Media::Music,
        price: Price::PerCall(0.06),
        seed: true,
        default: false,
        note: "Lyria 2: 30 s instrumental WAV at 48 kHz; honors seed and a negative prompt",
    },
    Model {
        provider: "openai",
        id: "gpt-image-2.5-flare",
        media: Media::Image,
        price: Price::ImageTokens(30.0, [272, 1056, 4160]),
        seed: false,
        default: true,
        note: "GPT Image 2.5 Flare: native transparent backgrounds; no seed",
    },
    Model {
        provider: "openai",
        id: "gpt-image-2.5-sunburst",
        media: Media::Image,
        price: Price::ImageTokens(30.0, [272, 1056, 4160]),
        seed: false,
        default: false,
        note: "GPT Image 2.5 Sunburst: most precise edits; no seed",
    },
    Model {
        provider: "elevenlabs",
        id: "eleven_text_to_sound_v2",
        media: Media::Sfx,
        price: Price::PerMinute(0.12),
        seed: false,
        default: true,
        note: "sound effects 0.5–30 s, can loop; free plan includes a few a month",
    },
    Model {
        provider: "elevenlabs",
        id: "music_v2_5",
        media: Media::Music,
        price: Price::PerMinute(0.15),
        seed: false,
        default: true,
        note: "Eleven Music: exact length, guaranteed instrumental; commercial use on paid plans",
    },
    Model {
        provider: "elevenlabs",
        id: "eleven_v4",
        media: Media::Voice,
        price: Price::PerKChars(0.08),
        seed: true,
        default: true,
        note: "Eleven v4: the most expressive speech; audio tags ([angry], [laughs], [whispers]) steer the delivery",
    },
    Model {
        provider: "elevenlabs",
        id: "eleven_v3",
        media: Media::Voice,
        price: Price::PerKChars(0.08),
        seed: true,
        default: false,
        note: "Eleven v3: dramatic delivery with audio tags; stability only 0, 0.5 or 1",
    },
    Model {
        provider: "elevenlabs",
        id: "eleven_flash_v2_5",
        media: Media::Voice,
        price: Price::PerKChars(0.04),
        seed: true,
        default: false,
        note: "Flash v2.5: half the price, flatter; no audio tags (--emotion is dropped)",
    },
    Model {
        provider: "elevenlabs",
        id: "eleven_ttv_v3",
        media: Media::Voice,
        price: Price::PerCall(0.03),
        seed: true,
        default: false,
        note: "Voice Design (mb voice design): about 3 previews of a voice from a description; billed as their sample text",
    },
    Model {
        provider: "tripo",
        id: "P1-20260311",
        media: Media::Model,
        price: Price::Model3d { mesh: 0.30, texture: 0.10, image: 0.10, rig: 0.25, animation: 0.10 },
        seed: true,
        default: true,
        note: "Tripo P1: low-poly game models (≤ 20 k faces); rigs bipeds, quadrupeds, fish, snakes…",
    },
    Model {
        provider: "tripo",
        id: "v3.1-20260211",
        media: Media::Model,
        price: Price::Model3d { mesh: 0.10, texture: 0.10, image: 0.10, rig: 0.25, animation: 0.10 },
        seed: true,
        default: false,
        note: "Tripo H3.1: the most detailed shapes (dense meshes mb decimates); same rigging",
    },
    Model {
        provider: "meshy",
        id: "meshy-7.1",
        media: Media::Model,
        price: Price::Model3d { mesh: 0.40, texture: 0.20, image: 0.0, rig: 0.10, animation: 0.06 },
        seed: false,
        default: true,
        note: "Meshy 7.1 at the Pro plan's $0.02/credit; rigs humanoids only (walk and run come with the rig)",
    },
    Model {
        provider: "meshy",
        id: "meshy-6-lite",
        media: Media::Model,
        price: Price::Model3d { mesh: 0.10, texture: 0.20, image: 0.0, rig: 0.10, animation: 0.06 },
        seed: false,
        default: false,
        note: "Meshy 6 Lite: cheaper, simpler shapes",
    },
    Model {
        provider: "mock",
        id: "mock-image",
        media: Media::Image,
        price: Price::Free,
        seed: true,
        default: true,
        note: "offline, deterministic placeholder art",
    },
    Model { provider: "mock", id: "mock-music", media: Media::Music, price: Price::Free, seed: true, default: true, note: "offline, deterministic test song" },
    Model { provider: "mock", id: "mock-sfx", media: Media::Sfx, price: Price::Free, seed: true, default: true, note: "offline, deterministic test effect" },
    Model { provider: "mock", id: "mock-3d", media: Media::Model, price: Price::Free, seed: true, default: true, note: "offline, deterministic placeholder model" },
    Model { provider: "mock", id: "mock-voice", media: Media::Voice, price: Price::Free, seed: true, default: true, note: "offline, deterministic babble with timings" },
];

/// Estimated USD to speak `chars` characters (text to speech), or the model's per-call price.
pub fn estimate_chars(model: &Model, chars: usize) -> Option<f64> {
    match model.price {
        Price::PerKChars(p) => Some(p * chars as f64 / 1000.0),
        Price::PerCall(p) => Some(p),
        Price::Free => Some(0.0),
        _ => None,
    }
}

pub fn find_model(provider: &str, id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.provider == provider && m.id == id)
}

pub fn default_model(provider: &str, media: Media) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.provider == provider && m.media == media && m.default)
}

/// Estimated USD for one call.
pub fn estimate(model: &Model, resolution: &str, quality: usize, seconds: f32) -> Option<f64> {
    match model.price {
        Price::PerImage(tiers) => tiers.iter().find(|t| t.0 == resolution).or(tiers.first()).map(|t| t.1),
        Price::PerCall(p) => Some(p),
        Price::PerMinute(p) => Some(p * (seconds.max(1.0) as f64) / 60.0),
        // A typical line (about 100 characters); estimate_chars has the real count.
        Price::PerKChars(p) => Some(p * 0.1),
        Price::ImageTokens(per_m, toks) => {
            let scale = match resolution {
                "2K" => 2.5,
                "4K" => 6.0,
                _ => 1.0,
            };
            Some(per_m * toks[quality.min(2)] as f64 * scale / 1e6)
        }
        Price::Model3d { mesh, texture, .. } => Some(mesh + texture),
        Price::Free => Some(0.0),
    }
}

/// Estimated USD for one 3D model: textured, from an image or text, rigged, with `animations` clips.
pub fn estimate_model(model: &Model, image: bool, rig: bool, animations: usize) -> Option<f64> {
    match model.price {
        Price::Model3d { mesh, texture, image: img, rig: r, animation } => {
            let rigged = rig || animations > 0;
            Some(mesh + texture + if image { img } else { 0.0 } + if rigged { r } else { 0.0 } + animation * animations as f64)
        }
        Price::Free => Some(0.0),
        _ => None,
    }
}

pub fn price_label(model: &Model) -> String {
    match model.price {
        Price::PerImage(t) => t.iter().map(|(r, p)| format!("{r} ${p:.4}")).collect::<Vec<_>>().join(", ") + " per image",
        Price::PerCall(p) => format!("${p:.2} per call"),
        Price::PerMinute(p) => format!("${p:.2} per minute"),
        Price::PerKChars(p) => format!("${p:.3} per 1K characters"),
        Price::ImageTokens(p, t) => format!("${p:.0}/1M image tokens (≈ ${:.3}–${:.3} per 1K image)", p * t[0] as f64 / 1e6, p * t[2] as f64 / 1e6),
        Price::Model3d { mesh, texture, image, rig, animation } => format!(
            "${:.2} textured{}, rig +${rig:.2}, +${animation:.2} per animation",
            mesh + texture,
            if image > 0.0 { format!(" (+${image:.2} from an image)") } else { String::new() }
        ),
        Price::Free => "free".into(),
    }
}

// --- requests -----------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RefImage {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// What the mock provider needs to draw a plausible stand-in (real providers ignore it).
#[derive(Clone, Debug, Default)]
pub struct MockHint {
    pub kind: String,
    pub frames: u32,
    pub key: Option<[u8; 3]>,
    pub palette: Vec<[u8; 3]>,
    pub pixel_art: bool,
}

#[derive(Clone, Debug)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    /// Style references first, then subject references (photos to restyle).
    pub refs: Vec<RefImage>,
    pub aspect: String,
    pub resolution: String,
    /// Ask for a transparent background where the model can make one.
    pub transparent: bool,
    pub seed: Option<u64>,
    /// 0 low, 1 medium, 2 high (OpenAI).
    pub quality: usize,
    pub hint: MockHint,
}

#[derive(Clone, Debug)]
pub struct AudioRequest {
    pub model: String,
    pub prompt: String,
    pub kind: Kind,
    pub seconds: Option<f32>,
    pub looping: bool,
    pub seed: Option<u64>,
    pub negative: Option<String>,
}

/// A 3D model request (mb model).
#[derive(Clone, Debug, Default)]
pub struct ModelRequest {
    pub model: String,
    /// What to make (subject, style words).
    pub prompt: String,
    /// What to keep out, where the provider takes it.
    pub negative: Option<String>,
    /// Make it from this image instead of from the prompt alone.
    pub image: Option<RefImage>,
    /// The face budget to ask for (mb decimates to its own budget afterwards anyway).
    pub polys: u32,
    pub rig: bool,
    /// Force a rig type (tripo: biped, quadruped, hexapod, octopod, avian, serpentine, aquatic).
    pub rig_type: Option<String>,
    /// Animations to make, by action ("walk", "run", "idle", "swim", or a provider's own id).
    pub animations: Vec<String>,
    pub seed: Option<u64>,
}

/// What a 3D provider returned: the model, plus one .glb per animation (same rig) when they come separately.
#[derive(Clone, Debug, Default)]
pub struct ModelOutput {
    pub glb: Vec<u8>,
    /// (action asked for, .glb holding its clip).
    pub clips: Vec<(String, Vec<u8>)>,
    pub rigged: bool,
    pub cost_usd: Option<f64>,
    pub text: Option<String>,
    /// What didn't go as asked (a rig the provider couldn't make, an animation it doesn't have…).
    pub notes: Vec<String>,
    /// (step, provider task id), for support requests and the sidecar.
    pub tasks: Vec<(String, String)>,
    /// The rig task and its rig type, when the provider rigged it: clips can be added to it later.
    pub rig: Option<(String, String)>,
}

/// Rigging (or animating) a model made earlier (`mb model <game> <asset> --rig`).
#[derive(Clone, Debug, Default)]
pub struct RigRequest {
    /// The task that generated it (tripo: rigs from it with no upload).
    pub task: Option<String>,
    /// Its raw .glb, uploaded when there's no task.
    pub glb: Vec<u8>,
    /// A rig made earlier: only add clips to it.
    pub rig_task: Option<String>,
    pub rig_type: Option<String>,
    pub animations: Vec<String>,
    /// What it is and its seed (the mock makes its stand-in from them).
    pub prompt: String,
    pub seed: Option<u64>,
}

pub struct Output {
    pub bytes: Vec<u8>,
    pub mime: String,
    /// Any text the model sent back (commentary, lyrics, structure).
    pub text: Option<String>,
    /// The actual cost when the response says (token usage).
    pub cost_usd: Option<f64>,
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn image(&self, req: &ImageRequest) -> Result<Output, String>;
    fn audio(&self, req: &AudioRequest) -> Result<Output, String>;
    /// A 3D model (only the 3D providers make them).
    fn model(&self, _req: &ModelRequest) -> Result<ModelOutput, String> {
        Err(format!("{} doesn't make 3D models; use --provider tripo, meshy or mock", self.name()))
    }
    /// Rigs a model made earlier and/or adds clips to its rig. The output's
    /// `glb` is empty when only clips were added.
    fn rig(&self, _req: &RigRequest) -> Result<ModelOutput, String> {
        Err(format!("{} can't rig a model made earlier; regenerate it with --rig, or rig it with --provider tripo", self.name()))
    }
    /// A line to print after a call (Maimbrain's keys: the month's usage).
    fn note(&self) -> Option<String> {
        None
    }
    /// Set when the call went through someone else's keys ("maimbrain"); recorded in the sidecar.
    fn via(&self) -> Option<&'static str> {
        None
    }
    /// What the account can still spend, in USD, for providers that say
    /// (tripo); None for the others. Asked before a paid batch.
    fn balance_usd(&self) -> Option<Result<f64, String>> {
        None
    }
}

/// Before a paid batch: refuses when the provider's balance can't cover the
/// estimate `need` (USD), saying how much is missing; a balance that can't be
/// read is a note, not a refusal.
pub fn check_balance(prov: &dyn Provider, need: f64) -> Result<(), String> {
    let Some(b) = prov.balance_usd() else { return Ok(()) };
    let credits = |usd: f64| if prov.name() == "tripo" { format!(" ({:.0} credits)", usd * 100.0) } else { String::new() };
    match b {
        Ok(have) if have + 1e-9 < need => Err(format!(
            "{}: the account has ${have:.2}{} and this needs about ${need:.2}{}: ${:.2}{} short. Top up{} and run it again, or make fewer (--variants, fewer clips)",
            prov.name(),
            credits(have),
            credits(need),
            need - have,
            credits(need - have),
            if prov.name() == "tripo" { " at https://developers.tripo3d.ai" } else { "" }
        )),
        Ok(have) => {
            eprintln!("  {} balance ${have:.2}{}; this takes about ${need:.2}", prov.name(), credits(have));
            Ok(())
        }
        Err(e) => {
            eprintln!("  note: {e}; going ahead without checking the balance");
            Ok(())
        }
    }
}

// --- transport ------------------------------------------------------------------

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub trait Transport {
    fn post(&self, url: &str, headers: &[(String, String)], content_type: &str, body: Vec<u8>) -> Result<Response, String>;
    /// A GET (polling a task, downloading a result).
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String>;
    /// A DELETE (a temporary voice).
    fn delete(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String>;
}

/// HTTP through ureq, retrying what a network blip or a busy provider breaks
/// (see [`retrying`]) without paying twice: a POST (a paid generation) is
/// sent again only when the provider can't have started it, because the
/// connection failed before the whole request went out, or it answered 429
/// or 503 (refused, not run). GETs (polling, downloads) and DELETEs are
/// safe to repeat and also retry on 500, 502 and 504.
pub struct Http;

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(concat!("mb/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn read_response(mut resp: ureq::http::Response<ureq::Body>, limit: u64) -> Result<Response, Failure> {
    let status = resp.status().as_u16();
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
    // The provider has answered: a body cut off now is after the work was done.
    let body = resp.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| Failure { msg: format!("reading the response: {e}"), unsent: false })?;
    RETRY_AFTER.with(|r| r.set(retry_after));
    Ok(Response { status, content_type, body })
}

thread_local! {
    /// The last response's Retry-After, in seconds.
    static RETRY_AFTER: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// A request that failed without a response.
#[derive(Debug)]
pub struct Failure {
    pub msg: String,
    /// The request never fully went out, so the provider can't have run it.
    pub unsent: bool,
}

/// The request body, counting what ureq has taken of it: all of it taken is
/// the request (possibly) delivered.
struct Counted<'a> {
    data: &'a [u8],
    taken: &'a std::cell::Cell<usize>,
}

impl std::io::Read for Counted<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let at = self.taken.get();
        let n = buf.len().min(self.data.len() - at);
        buf[..n].copy_from_slice(&self.data[at..at + n]);
        self.taken.set(at + n);
        Ok(n)
    }
}

impl Http {
    fn post_once(url: &str, headers: &[(String, String)], content_type: &str, body: &[u8]) -> Result<Response, Failure> {
        let mut req = agent(600).post(url).header("Content-Type", content_type).header("Content-Length", body.len().to_string());
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let taken = std::cell::Cell::new(0);
        let mut reader = Counted { data: body, taken: &taken };
        match req.send(ureq::SendBody::from_reader(&mut reader)) {
            Ok(resp) => read_response(resp, 200 << 20),
            // Short of the whole body the request isn't complete, and a server can't act on it.
            Err(e) => Err(Failure { msg: format!("can't reach {}: {e}", host(url)), unsent: taken.get() < body.len() || is_connect(&e) }),
        }
    }

    fn call_once(method: &str, url: &str, headers: &[(String, String)], timeout: u64, limit: u64) -> Result<Response, Failure> {
        let a = agent(timeout);
        let mut req = if method == "DELETE" { a.delete(url) } else { a.get(url) };
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.call() {
            Ok(resp) => read_response(resp, limit),
            Err(e) => Err(Failure { msg: format!("can't reach {}: {e}", host(url)), unsent: true }),
        }
    }
}

/// A ureq error from before any request went out (no connection).
fn is_connect(e: &ureq::Error) -> bool {
    use ureq::Timeout;
    matches!(e, ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Timeout(Timeout::Resolve | Timeout::Connect))
}

impl Transport for Http {
    fn post(&self, url: &str, headers: &[(String, String)], content_type: &str, body: Vec<u8>) -> Result<Response, String> {
        retrying(host(url), false, &mut || Http::post_once(url, headers, content_type, &body), &mut sleep)
    }

    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String> {
        retrying(host(url), true, &mut || Http::call_once("GET", url, headers, 300, 200 << 20), &mut sleep)
    }

    fn delete(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String> {
        retrying(host(url), true, &mut || Http::call_once("DELETE", url, headers, 60, 1 << 20), &mut sleep)
    }
}

fn sleep(d: Duration) {
    std::thread::sleep(d);
}

/// How many times a request is tried in all (MB_HTTP_TRIES, default 4; 1 turns retrying off).
fn tries() -> u32 {
    std::env::var("MB_HTTP_TRIES").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(4).clamp(1, 10)
}

/// Runs `once` until it gets an answer worth keeping, waiting 1, 2, 4 … s
/// between tries (or the provider's Retry-After, up to 30 s). Retried: a
/// failure before the request went out; a 429 or 503 (the provider refused
/// it without running it); and, when `idempotent`, any failure and a 500,
/// 502 or 504. A POST that failed after it was sent isn't retried: the
/// provider may have run it and billed for it, so the error says so.
pub fn retrying(host: &str, idempotent: bool, once: &mut dyn FnMut() -> Result<Response, Failure>, sleep: &mut dyn FnMut(Duration)) -> Result<Response, String> {
    let n = tries();
    let mut attempt = 1;
    loop {
        let wait = Duration::from_secs(1 << (attempt - 1).min(4));
        let r = once();
        let why = match &r {
            Ok(resp) if matches!(resp.status, 429 | 503) || idempotent && matches!(resp.status, 500 | 502 | 504) => format!("HTTP {}", resp.status),
            Ok(_) => return r.map_err(|f| f.msg),
            Err(f) if f.unsent || idempotent => f.msg.clone(),
            Err(f) => {
                return Err(format!(
                    "{} (the connection dropped after the request was sent, so {host} may have run it, and billed for it: not retried; run it again to retry)",
                    f.msg
                ));
            }
        };
        if attempt >= n {
            return match r {
                Ok(resp) => Ok(resp),
                Err(f) => Err(if n > 1 { format!("{} (after {n} tries)", f.msg) } else { f.msg }),
            };
        }
        let wait = match RETRY_AFTER.with(|c| c.take()) {
            Some(s) if r.is_ok() => Duration::from_secs(s.min(30)),
            _ => wait,
        };
        eprintln!("    {host}: {why}; trying again in {} s ({}/{n})", wait.as_secs(), attempt + 1);
        sleep(wait);
        attempt += 1;
    }
}
/// A held slot of a concurrency pool shared by every `mb` run on this machine
/// (a lock file; dropping it frees the slot).
pub struct PoolSlot(#[allow(dead_code)] pub Option<std::fs::File>);

/// Takes one of `n` slots of `pool` (lock files in `dir`), waiting while
/// other `mb` runs hold them all, up to `timeout`; then goes ahead (the
/// provider's own refusal is the backstop). No `dir`: no limit.
pub fn take_slot(dir: Option<&std::path::Path>, pool: &str, n: usize, timeout: Duration, provider: &str) -> PoolSlot {
    let Some(dir) = dir.filter(|d| std::fs::create_dir_all(d).is_ok()) else { return PoolSlot(None) };
    let t0 = std::time::Instant::now();
    let mut said = false;
    loop {
        for i in 0..n.max(1) {
            let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(format!("{pool}-{i}.lock")));
            if let Ok(f) = f
                && f.try_lock().is_ok()
            {
                if said {
                    eprintln!("    {provider}: a slot is free after {:.0} s; starting", t0.elapsed().as_secs_f32());
                }
                return PoolSlot(Some(f));
            }
        }
        if !said {
            eprintln!("    {provider}: {n} requests are already running from mb on this machine (the account's limit); waiting for one to finish");
            said = true;
        }
        if t0.elapsed() >= timeout {
            return PoolSlot(None);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// A multipart/form-data body: text fields, then files. Returns the content type (with its boundary) and the body.
pub fn multipart(fields: &[(&str, &str)], files: &[(&str, &str, &str, &[u8])]) -> (String, Vec<u8>) {
    let boundary = format!("mb-{:016x}", super::mock::hash(&[&std::process::id().to_le_bytes(), &files.iter().map(|f| f.3.len()).sum::<usize>().to_le_bytes()]));
    let mut b = Vec::new();
    for (name, value) in fields {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    for (name, filename, mime, bytes) in files {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
        b.extend_from_slice(bytes);
        b.extend_from_slice(b"\r\n");
    }
    b.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), b)
}

/// How often to ask about a 3D task and how long to wait (MB_MODEL_TIMEOUT seconds, default 900).
#[derive(Clone, Copy, Debug)]
pub struct Poll {
    pub every: Duration,
    pub timeout: Duration,
}

impl Poll {
    pub fn from_env() -> Poll {
        let t = std::env::var("MB_MODEL_TIMEOUT").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(900);
        Poll { every: Duration::from_secs(3), timeout: Duration::from_secs(t) }
    }

    /// Calls `check` until it returns a value or an error, printing each new status.
    pub fn wait<T>(&self, what: &str, mut check: impl FnMut() -> Result<(Option<T>, String), String>) -> Result<T, String> {
        let t0 = std::time::Instant::now();
        let mut last = String::new();
        loop {
            let (done, status) = check()?;
            if status != last {
                eprintln!("    {what}: {status} ({:.0} s)", t0.elapsed().as_secs_f32());
                last = status;
            }
            if let Some(v) = done {
                return Ok(v);
            }
            if t0.elapsed() > self.timeout {
                return Err(format!("{what} is still running after {} s (MB_MODEL_TIMEOUT); it may still finish on the provider's site", self.timeout.as_secs()));
            }
            std::thread::sleep(self.every);
        }
    }
}

/// Downloads a result file. The URLs are signed: no API key goes to the file host.
pub fn download(http: &dyn Transport, provider: &str, url: &str) -> Result<Vec<u8>, String> {
    let r = http.get(url, &[])?;
    if r.status != 200 {
        return Err(format!("{provider}: downloading from {}: HTTP {}", host(url), r.status));
    }
    Ok(r.body)
}

fn host(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

/// An error message from a JSON error body, or the status.
pub fn http_error(provider: &str, r: &Response) -> String {
    let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap_or_default();
    let msg = v["error"]["message"]
        .as_str()
        .or_else(|| v["error"].as_str())
        .or_else(|| v["detail"]["message"].as_str())
        .or_else(|| v["detail"][0]["msg"].as_str())
        .or_else(|| v["detail"].as_str())
        .or_else(|| v["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(&r.body).chars().take(300).collect());
    let hint = match r.status {
        401 | 403 => " (check the API key: `mb keys list`)",
        429 => " (rate limited or out of quota; wait, or check billing)",
        _ => "",
    };
    format!("{provider}: HTTP {}: {msg}{hint}", r.status)
}

/// The provider implementation for a name: with your key from the environment
/// or `mb keys` if you have one, else through Maimbrain's keys when your
/// account has access (generate/maimbrain.rs). `est` is the estimated cost of
/// the whole command, checked against what's left of a Maimbrain budget.
pub fn pick(name: &str, model: &str, est: Option<f64>) -> Result<Box<dyn Provider>, String> {
    use super::maimbrain::{self, Route};
    if !matches!(name, "mock" | "gemini" | "vertex" | "openai" | "elevenlabs" | "tripo" | "meshy") {
        return Err(format!("unknown provider {name:?} (gemini, vertex, openai, elevenlabs, tripo, meshy, maimbrain, mock)"));
    }
    let own = super::keys::lookup(name);
    match maimbrain::decide(name, Some(model), maimbrain::mode(), own.as_ref().map(|k| k.1.as_str()), &maimbrain::Live)? {
        Route::Mock => Ok(Box::new(super::mock::Mock)),
        Route::Maimbrain(s) => {
            maimbrain::check_budget(&s, est, name)?;
            eprintln!("  keys: Maimbrain's (@{}) · {}", s.username, s.summary());
            if name == "tripo" {
                return Ok(Box::new(super::tripo::Tripo::signed_in(Box::new(Http), Poll::from_env())?));
            }
            Ok(Box::new(maimbrain::Maimbrain::signed_in(name, Box::new(Http))?))
        }
        Route::Own(from) => {
            eprintln!("  keys: your {name} key (from {from})");
            let key = own.map(|k| k.0).unwrap_or_default();
            Ok(match name {
                "gemini" => Box::new(super::google::Google::gemini(key, Box::new(Http))),
                "vertex" => {
                    let location = std::env::var("GOOGLE_CLOUD_LOCATION").unwrap_or_else(|_| "global".into());
                    Box::new(super::google::Google::vertex(key, location, Box::new(Http)))
                }
                "openai" => Box::new(super::openai::OpenAi::new(key, Box::new(Http))),
                "tripo" => Box::new(super::tripo::Tripo::new(key, Box::new(Http), Poll::from_env()).with_slots(crate::remote::config_dir().ok().map(|d| d.join("tripo-slots")))),
                "meshy" => Box::new(super::meshy::Meshy::new(key, Box::new(Http), Poll::from_env())),
                _ => Box::new(super::elevenlabs::ElevenLabs::new(key, Box::new(Http)).with_slots(super::elevenlabs::slots_dir())),
            })
        }
    }
}

/// The provider to use when none was named: MB_PROVIDER, then the first with
/// a key among `order`, then (signed in, with access) the first of `order`
/// that Maimbrain's keys cover.
pub fn choose(flag: Option<&str>, style: Option<&str>, order: &[&str], media: &str) -> Result<String, String> {
    let has_key = |p: &str| super::keys::lookup(p).is_some();
    choose_with(flag, style, order, media, super::maimbrain::mode(), &has_key, &super::maimbrain::Live)
}

pub fn choose_with(
    flag: Option<&str>,
    style: Option<&str>,
    order: &[&str],
    media: &str,
    mode: super::maimbrain::Mode,
    has_key: &dyn Fn(&str) -> bool,
    server: &dyn super::maimbrain::Server,
) -> Result<String, String> {
    use super::maimbrain::{Mode, SERVER_PROVIDERS};
    if let Some(p) = flag {
        return Ok(p.to_string());
    }
    if mode == Mode::Maimbrain {
        // The style's provider if Maimbrain's keys cover it, else the first of `order` they do.
        return style
            .filter(|p| SERVER_PROVIDERS.contains(p))
            .or_else(|| order.iter().copied().find(|p| SERVER_PROVIDERS.contains(p)))
            .map(str::to_string)
            .ok_or_else(|| format!("Maimbrain's keys don't cover {media}"));
    }
    if let Some(p) = style {
        return Ok(p.to_string());
    }
    if let Ok(p) = std::env::var("MB_PROVIDER") {
        return Ok(p);
    }
    for p in order {
        if has_key(p) {
            return Ok(p.to_string());
        }
    }
    if mode == Mode::Auto
        && let Ok(Some(s)) = server.status()
        && s.allowed
        && let Some(p) = order.iter().find(|p| s.supports(p, None))
    {
        return Ok(p.to_string());
    }
    Err(format!(
        "no provider key for {media}. Set one up:\n{}\nor try the pipeline offline with --provider mock{}",
        order.iter().map(|p| format!("  {p}: {}", super::keys::help_for(p))).collect::<Vec<_>>().join("\n"),
        super::maimbrain::hint(mode, server)
    ))
}

/// Base64 (standard alphabet, padded).
pub fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| format!("bad base64 in the response: {e}"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Runs `retrying` over scripted outcomes; what it gave, the tries made and the waits.
    fn retried(idempotent: bool, script: Vec<Result<u16, bool>>) -> (Result<u16, String>, usize, Vec<u64>) {
        let mut script = script.into_iter();
        let mut tries = 0;
        let mut waits = Vec::new();
        let r = retrying(
            "api.example",
            idempotent,
            &mut || {
                tries += 1;
                match script.next().expect("asked once too often") {
                    Ok(status) => {
                        RETRY_AFTER.with(|c| c.set(None));
                        Ok(Response { status, content_type: String::new(), body: Vec::new() })
                    }
                    Err(unsent) => Err(Failure { msg: "can't reach api.example: connection reset".into(), unsent }),
                }
            },
            &mut |d| waits.push(d.as_secs()),
        );
        (r.map(|r| r.status), tries, waits)
    }

    #[test]
    fn a_blip_is_retried_but_a_request_that_went_out_is_not_paid_twice() {
        // The connection reset before the request went out, twice: the third try lands.
        assert_eq!(retried(false, vec![Err(true), Err(true), Ok(200)]), (Ok(200), 3, vec![1, 2]));
        // Refused without being run (busy, overloaded): asked again.
        assert_eq!(retried(false, vec![Ok(503), Ok(429), Ok(200)]), (Ok(200), 3, vec![1, 2]));
        // Reset after the whole request was sent: the provider may have run it, so no second call.
        let (r, tries, _) = retried(false, vec![Err(false)]);
        assert_eq!(tries, 1);
        assert!(r.unwrap_err().contains("may have run it, and billed for it: not retried"));
        // A 500 on a POST may have been run too: returned as it is.
        assert_eq!(retried(false, vec![Ok(500)]), (Ok(500), 1, vec![]));
        // GETs (polling, downloads) are safe to repeat whatever went wrong.
        assert_eq!(retried(true, vec![Err(false), Ok(502), Ok(200)]), (Ok(200), 3, vec![1, 2]));
        // Four tries, then the last answer or error.
        assert_eq!(retried(true, vec![Ok(503); 4]), (Ok(503), 4, vec![1, 2, 4]));
        let (r, _, _) = retried(true, vec![Err(true); 4]);
        assert!(r.unwrap_err().ends_with("(after 4 tries)"));
    }

    #[test]
    fn a_real_post_tells_unsent_from_sent() {
        // Nothing listening: the request never went out.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = l.local_addr().unwrap();
        drop(l);
        let f = Http::post_once(&format!("http://{closed}/v1/x"), &[], "application/json", b"{\"prompt\":\"x\"}").err().unwrap();
        assert!(f.unsent, "{f:?}");
        // A server that takes the whole request and hangs up without answering: it may have run it.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            use std::io::Read;
            let (mut s, _) = l.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while !String::from_utf8_lossy(&got).contains("\"x\"}") {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            String::from_utf8_lossy(&got).into_owned()
        });
        let f = Http::post_once(&format!("http://{addr}/v1/x"), &[], "application/json", b"{\"prompt\":\"x\"}").err().unwrap();
        assert!(!f.unsent, "{f:?}");
        let req = server.join().unwrap();
        assert!(req.to_ascii_lowercase().contains("content-length: 14") && !req.to_ascii_lowercase().contains("chunked"), "{req}");
    }

    #[test]
    fn a_post_body_is_counted_as_it_goes_out() {
        use std::io::Read;
        let taken = std::cell::Cell::new(0);
        let mut c = Counted { data: b"hello world", taken: &taken };
        let mut buf = [0u8; 4];
        assert_eq!(c.read(&mut buf).unwrap(), 4);
        assert_eq!(taken.get(), 4, "a partial body: the request can't have been complete");
        let mut rest = Vec::new();
        c.read_to_end(&mut rest).unwrap();
        assert_eq!((rest.as_slice(), taken.get()), (&b"o world"[..], 11));
    }

    /// A transport that records requests and answers with canned responses.
    pub struct Fake {
        pub sent: RefCell<Vec<(String, Vec<(String, String)>, serde_json::Value)>>,
        pub reply: RefCell<Vec<Response>>,
    }

    impl Fake {
        pub fn new(replies: Vec<Response>) -> Fake {
            Fake { sent: RefCell::new(Vec::new()), reply: RefCell::new(replies) }
        }
    }

    impl Transport for std::rc::Rc<Fake> {
        fn post(&self, url: &str, headers: &[(String, String)], content_type: &str, body: Vec<u8>) -> Result<Response, String> {
            // A JSON body as JSON; anything else (multipart) as its content type and text.
            let v = serde_json::from_slice(&body)
                .unwrap_or_else(|_| serde_json::json!({ "content_type": content_type, "text": String::from_utf8_lossy(&body) }));
            self.sent.borrow_mut().push((url.to_string(), headers.to_vec(), v));
            let mut r = self.reply.borrow_mut();
            if r.is_empty() { Err("no canned reply".into()) } else { Ok(r.remove(0)) }
        }

        fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String> {
            self.sent.borrow_mut().push((format!("GET {url}"), headers.to_vec(), serde_json::Value::Null));
            let mut r = self.reply.borrow_mut();
            if r.is_empty() { Err("no canned reply".into()) } else { Ok(r.remove(0)) }
        }

        fn delete(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String> {
            self.sent.borrow_mut().push((format!("DELETE {url}"), headers.to_vec(), serde_json::Value::Null));
            let mut r = self.reply.borrow_mut();
            if r.is_empty() { Err("no canned reply".into()) } else { Ok(r.remove(0)) }
        }
    }

    pub fn json(status: u16, v: serde_json::Value) -> Response {
        Response { status, content_type: "application/json".into(), body: serde_json::to_vec(&v).unwrap() }
    }

    #[test]
    fn every_provider_has_defaults_and_prices() {
        for (p, media) in [
            ("gemini", Media::Image),
            ("gemini", Media::Music),
            ("vertex", Media::Music),
            ("openai", Media::Image),
            ("elevenlabs", Media::Sfx),
            ("mock", Media::Image),
            ("tripo", Media::Model),
            ("meshy", Media::Model),
            ("mock", Media::Model),
        ] {
            let m = default_model(p, media).unwrap_or_else(|| panic!("{p} {media:?}"));
            assert!(estimate(m, "1K", 1, 30.0).is_some());
        }
        let nb = find_model("gemini", "gemini-nano-banana-2.1").unwrap();
        assert_eq!(estimate(nb, "1K", 1, 0.0), Some(0.0336));
        assert_eq!(estimate(nb, "2K", 1, 0.0), Some(0.0504));
        assert_eq!(estimate(find_model("gemini", "lyria-3-clip-preview").unwrap(), "", 0, 30.0), Some(0.04));
        let sfx = find_model("elevenlabs", "eleven_text_to_sound_v2").unwrap();
        assert!((estimate(sfx, "", 0, 2.0).unwrap() - 0.004).abs() < 1e-9);
        let p1 = find_model("tripo", "P1-20260311").unwrap();
        assert!((estimate_model(p1, false, false, 0).unwrap() - 0.40).abs() < 1e-9, "P1 textured: 40 credits");
        assert!((estimate_model(p1, false, true, 1).unwrap() - 0.75).abs() < 1e-9, "a swimming fish: 75 credits");
        assert!((estimate_model(p1, true, true, 3).unwrap() - 1.05).abs() < 1e-9);
        let oa = find_model("openai", "gpt-image-2.5-flare").unwrap();
        assert!((estimate(oa, "1K", 1, 0.0).unwrap() - 0.03168).abs() < 1e-6);
    }

    #[test]
    fn choosing_a_provider_with_and_without_maimbrains_keys() {
        use super::super::maimbrain::{Mode, Server, ServerModel, ServerProvider, Status};
        struct S(Option<Status>);
        impl Server for S {
            fn status(&self) -> Result<Option<Status>, String> {
                Ok(self.0.clone())
            }
        }
        let ok = S(Some(Status {
            allowed: true,
            username: "ada".into(),
            providers: vec![ServerProvider { name: "gemini".into(), models: vec![ServerModel { id: "gemini-nano-banana-2.1".into() }] }],
            ..Status::default()
        }));
        let order = ["gemini", "vertex", "openai"];
        let none = |_: &str| false;
        let openai = |p: &str| p == "openai";
        // Your key first, in order; then Maimbrain's keys; --own-keys never falls back.
        assert_eq!(choose_with(None, None, &order, "images", Mode::Auto, &openai, &ok).unwrap(), "openai");
        assert_eq!(choose_with(None, None, &order, "images", Mode::Auto, &none, &ok).unwrap(), "gemini");
        let e = choose_with(None, None, &order, "images", Mode::Own, &none, &ok).unwrap_err();
        assert!(e.contains("GEMINI_API_KEY") && !e.contains("Maimbrain"), "{e}");
        // --provider maimbrain: the first provider in order the server covers, over your key and the style's choice.
        assert_eq!(choose_with(None, Some("openai"), &order, "images", Mode::Maimbrain, &openai, &ok).unwrap(), "gemini");
        assert_eq!(choose_with(None, None, &["elevenlabs"], "sfx", Mode::Maimbrain, &none, &ok).unwrap(), "elevenlabs");
        assert!(choose_with(None, None, &["openai"], "images", Mode::Maimbrain, &none, &ok).is_err());
        // Signed out or without access: the setup message says how to get access.
        let e = choose_with(None, None, &order, "images", Mode::Auto, &none, &S(None)).unwrap_err();
        assert!(e.contains("ask the Maimbrain team") && e.contains("mb login"), "{e}");
        let denied = S(Some(Status { allowed: false, username: "bob".into(), ..Status::default() }));
        assert!(choose_with(None, None, &order, "images", Mode::Auto, &none, &denied).unwrap_err().contains("@bob doesn't have it"));
        // A named provider is always taken as is.
        assert_eq!(choose_with(Some("vertex"), None, &order, "images", Mode::Maimbrain, &none, &ok).unwrap(), "vertex");
    }

    #[test]
    fn errors_are_readable() {
        let r = json(403, serde_json::json!({"error": {"message": "API key not valid"}}));
        assert_eq!(http_error("gemini", &r), "gemini: HTTP 403: API key not valid (check the API key: `mb keys list`)");
        let r = json(422, serde_json::json!({"detail": [{"msg": "duration too long"}]}));
        assert!(http_error("elevenlabs", &r).contains("duration too long"));
    }
}
