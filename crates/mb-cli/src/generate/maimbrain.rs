//! Maimbrain's generation keys. Creators the Maimbrain team has given access
//! (a toggle on the server's admin page) can generate with the platform's
//! Gemini and ElevenLabs keys, within a monthly budget, without a key of
//! their own; with a second toggle, 3D models on its Tripo key too, within the
//! same budget. The server makes the provider call and returns the raw image
//! or sound; everything after that (keying, fitting, loops, loudness,
//! sidecars) still happens here. For Tripo the server passes Tripo's task
//! API through instead, and the usual Tripo client talks to it
//! (`Tripo::on_maimbrain` in tripo.rs).
//!
//! Which keys a call uses:
//!   1. your own key for the provider, if you have one (you pay) — unchanged;
//!   2. otherwise, when you're signed in (`mb login`) and the server says your
//!      account has access, Maimbrain's keys (`GET /api/v1/generate/status`,
//!      then `POST /api/v1/generate/image|music|sfx`, or Tripo's paths under
//!      `/api/v1/generate/tripo/`);
//!   3. otherwise the usual "set a key" message, plus how to ask for access.
//!
//! `--own-keys` (or `MB_KEYS_SOURCE=own`) never uses Maimbrain's keys;
//! `--provider maimbrain` (or `MB_KEYS_SOURCE=maimbrain`) always does, even
//! when you have a key of your own.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::Deserialize;
use serde_json::{Value, json};

use super::audio::Kind;
use super::provider::{AudioRequest, ImageRequest, Output, Provider, Transport, b64, unb64};

/// Providers the server calls with Maimbrain's keys.
pub const SERVER_PROVIDERS: &[&str] = &["gemini", "elevenlabs", "tripo"];

/// Those the server calls in one request (this file's `Maimbrain` provider); tripo goes through tripo.rs.
const PROXIED: &[&str] = &["gemini", "elevenlabs"];

pub const ASK: &str = "or ask the Maimbrain team for access to its keys";

/// What a signed-out run without a key of your own says first. An account
/// with access needs nothing but `mb login`, and an agent that reads "set a
/// key" first asks its user for a key they don't have.
pub const SIGN_IN: &str = "If your Maimbrain account has access to Maimbrain's generation keys, you don't need a key of your own: \
     run mb login (it prints a link and a code to confirm in a browser), then run this again. \
     mb models then shows your access and what's left of this month's budget.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Your key if you have one, else Maimbrain's if your account has access.
    Auto,
    /// Only your own keys (--own-keys).
    Own,
    /// Only Maimbrain's keys (--provider maimbrain).
    Maimbrain,
}

static MODE: AtomicU8 = AtomicU8::new(0);

/// The mode for this run: the flags, else MB_KEYS_SOURCE (auto, own, maimbrain), else auto.
pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Auto,
        2 => Mode::Own,
        3 => Mode::Maimbrain,
        _ => env_mode(),
    }
}

fn env_mode() -> Mode {
    match std::env::var("MB_KEYS_SOURCE").unwrap_or_default().trim() {
        "own" => Mode::Own,
        "maimbrain" => Mode::Maimbrain,
        _ => Mode::Auto,
    }
}

/// Reads `--provider maimbrain` and `--own-keys` into the run's mode; returns
/// the provider flag left to use (none for `maimbrain`: the default upstream).
pub fn take_flags(provider: Option<&str>, own_keys: bool) -> Result<Option<String>, String> {
    let m = mode_for(provider, own_keys, env_mode())?;
    MODE.store(
        match m {
            Mode::Auto => 1,
            Mode::Own => 2,
            Mode::Maimbrain => 3,
        },
        Ordering::Relaxed,
    );
    Ok(provider.filter(|p| *p != "maimbrain").map(str::to_string))
}

pub fn mode_for(provider: Option<&str>, own_keys: bool, env: Mode) -> Result<Mode, String> {
    Ok(match (provider, own_keys) {
        (Some("maimbrain"), true) => return Err("--provider maimbrain uses Maimbrain's keys and --own-keys forbids them: pick one".into()),
        (Some("maimbrain"), false) => Mode::Maimbrain,
        (_, true) => Mode::Own,
        _ => env,
    })
}

// --- the server's answer ----------------------------------------------------------

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ServerModel {
    pub id: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ServerProvider {
    pub name: String,
    #[serde(default)]
    pub models: Vec<ServerModel>,
}

/// `GET /api/v1/generate/status`.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Status {
    pub allowed: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub budget_usd: f64,
    #[serde(default)]
    pub used_usd: f64,
    #[serde(default)]
    pub remaining_usd: f64,
    #[serde(default)]
    pub resets_at: String,
    /// Providers the server has keys for (only when allowed).
    #[serde(default)]
    pub providers: Vec<ServerProvider>,
    #[serde(default)]
    pub message: Option<String>,
}

impl Status {
    /// Whether the server calls this provider (and model, when given) for you.
    pub fn supports(&self, provider: &str, model: Option<&str>) -> bool {
        self.providers.iter().any(|p| p.name == provider && model.is_none_or(|m| p.models.iter().any(|x| x.id == m)))
    }

    pub fn summary(&self) -> String {
        format!("${:.2} of ${:.2} used this month", self.used_usd, self.budget_usd)
    }

    fn resets(&self) -> &str {
        self.resets_at.get(..10).unwrap_or("next month")
    }
}

/// Where the status comes from (tests use a fake).
pub trait Server {
    /// None when signed out.
    fn status(&self) -> Result<Option<Status>, String>;
}

/// The signed-in server (`mb login`), asked at most once per run.
pub struct Live;

impl Server for Live {
    fn status(&self) -> Result<Option<Status>, String> {
        static CACHE: OnceLock<Result<Option<Status>, String>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                crate::remote::generation_status()?
                    .map(|v| serde_json::from_value(v).map_err(|e| format!("unreadable answer about Maimbrain's keys: {e}")))
                    .transpose()
            })
            .clone()
    }
}

// --- deciding ---------------------------------------------------------------------

#[derive(Debug)]
pub enum Route {
    Mock,
    /// Your own key, and where it came from (an env var or `mb keys`).
    Own(String),
    Maimbrain(Status),
}

fn no_key(provider: &str) -> String {
    format!("no {provider} key: {}", super::keys::help_for(provider))
}

/// Which keys a call to `provider` (and `model`) uses. `own_key` is where your
/// key for it comes from, if you have one.
pub fn decide(provider: &str, model: Option<&str>, mode: Mode, own_key: Option<&str>, server: &dyn Server) -> Result<Route, String> {
    if provider == "mock" {
        return Ok(Route::Mock);
    }
    match (mode, own_key) {
        (Mode::Own, Some(from)) | (Mode::Auto, Some(from)) => return Ok(Route::Own(from.to_string())),
        (Mode::Own, None) => return Err(format!("{} (--own-keys: Maimbrain's keys aren't used)", no_key(provider))),
        _ => {}
    }
    let forced = mode == Mode::Maimbrain;
    if !SERVER_PROVIDERS.contains(&provider) {
        return Err(if forced {
            format!("Maimbrain's keys cover gemini (images, music), elevenlabs (sound effects, music) and tripo (3D models), not {provider}")
        } else {
            no_key(provider)
        });
    }
    match server.status() {
        Ok(None) if forced => Err("Maimbrain's keys need you signed in: run mb login (the Maimbrain team gives access per account)".into()),
        Ok(None) => Err(format!(
            "no {provider} key, and mb isn't signed in to Maimbrain.\n{SIGN_IN}\nWithout that access: {}, {ASK}",
            super::keys::help_for(provider)
        )),
        Ok(Some(s)) if !s.allowed => {
            let who = if s.username.is_empty() { "your account".to_string() } else { format!("@{}", s.username) };
            if forced {
                Err(s.message.clone().unwrap_or_else(|| {
                    format!("{who} can't use Maimbrain's generation keys; ask the Maimbrain team for access, or set your own key with mb keys set {provider}")
                }))
            } else {
                Err(format!("{}\n{ASK} ({who} doesn't have it)", no_key(provider)))
            }
        }
        Ok(Some(s)) if !s.supports(provider, model) => {
            let what = model.map(|m| format!("{provider} {m}")).unwrap_or_else(|| provider.to_string());
            let lead = if forced { String::new() } else { format!("{}\n", no_key(provider)) };
            Err(format!("{lead}Maimbrain's keys don't cover {what} right now (`mb models` lists what they do)"))
        }
        Ok(Some(s)) => Ok(Route::Maimbrain(s)),
        Err(e) if forced => Err(e),
        Err(e) => Err(format!("{}\n(couldn't check Maimbrain's keys: {e})", no_key(provider))),
    }
}

/// Whether a call to `provider` (and `model`) would go through Maimbrain's keys.
pub fn would_use(provider: &str, model: Option<&str>) -> bool {
    let own = super::keys::lookup(provider);
    matches!(decide(provider, model, mode(), own.as_ref().map(|k| k.1.as_str()), &Live), Ok(Route::Maimbrain(_)))
}

/// Refuses a call the month's remaining budget can't cover.
pub fn check_budget(s: &Status, est: Option<f64>, provider: &str) -> Result<(), String> {
    let est = est.unwrap_or(0.0);
    if est > s.remaining_usd + 1e-9 {
        return Err(format!(
            "est. ${est:.4} is more than the ${:.2} left of your ${:.2} monthly budget on Maimbrain's keys (it resets {}); \
             ask the Maimbrain team for more, or use your own key: mb keys set {provider}",
            s.remaining_usd.max(0.0),
            s.budget_usd,
            s.resets()
        ));
    }
    Ok(())
}

/// `--allow-name` lets a flagged name through on your own keys only: the
/// server refuses every flagged name on Maimbrain's.
pub fn allow_name_refused(names: &[String], provider: &str) -> String {
    format!(
        "--allow-name ({}) isn't honored on Maimbrain's keys: the server refuses prompts that name someone else's character, \
         franchise or brand. If you own the rights, use your own key (mb keys set {provider}, then --own-keys)",
        names.join(", ")
    )
}

/// The message when no provider has a key for `media`; `setup` lists how to
/// set up each. Signed out, signing in comes first.
pub fn no_provider(media: &str, setup: &str, mode: Mode, server: &dyn Server) -> String {
    let offline = "or try the pipeline offline with --provider mock";
    if mode == Mode::Own {
        return format!("no provider key for {media}. Set one up:\n{setup}\n{offline}");
    }
    let why = match server.status() {
        Ok(None) => {
            return format!(
                "no provider key for {media}, and mb isn't signed in to Maimbrain.\n{SIGN_IN}\nWithout that access, set up a key of your own:\n{setup}\n{ASK}, {offline}"
            );
        }
        Ok(Some(s)) if !s.allowed => format!("{ASK} (@{} doesn't have it)", s.username),
        Ok(Some(_)) => "(Maimbrain's keys don't cover this; see mb models)".into(),
        Err(e) => format!("{ASK} (couldn't check: {e})"),
    };
    format!("no provider key for {media}. Set one up:\n{setup}\n{offline}\n{why}")
}

/// One line for `mb models` and `mb doctor`.
pub fn describe(server: &dyn Server) -> (bool, String) {
    match server.status() {
        Ok(Some(s)) if s.allowed => {
            let names: Vec<&str> = s.providers.iter().map(|p| p.name.as_str()).collect();
            (true, format!("Maimbrain's keys: @{} has access · {} · {}", s.username, s.summary(), if names.is_empty() { "no providers configured".into() } else { names.join(", ") }))
        }
        Ok(Some(s)) => (false, format!("Maimbrain's keys: @{} doesn't have access ({})", s.username, ASK.trim_start_matches("or "))),
        Ok(None) => (false, format!("Maimbrain's keys: not signed in; if your account has access, run mb login ({ASK})")),
        Err(e) => (false, format!("Maimbrain's keys: couldn't check ({e})")),
    }
}

// --- the provider -------------------------------------------------------------------

/// Sends the provider request to the Maimbrain server, which calls the
/// provider with its key and returns the raw output.
pub struct Maimbrain {
    server: String,
    token: String,
    upstream: &'static str,
    http: Box<dyn Transport>,
    last: RefCell<Option<String>>,
}

impl Maimbrain {
    pub fn new(server: &str, token: &str, upstream: &str, http: Box<dyn Transport>) -> Result<Maimbrain, String> {
        let upstream = PROXIED.iter().find(|p| **p == upstream).ok_or(format!("Maimbrain's keys don't cover {upstream} this way"))?;
        Ok(Maimbrain { server: server.trim_end_matches('/').to_string(), token: token.to_string(), upstream, http, last: RefCell::new(None) })
    }

    /// With the `mb login` credentials.
    pub fn signed_in(upstream: &str, http: Box<dyn Transport>) -> Result<Maimbrain, String> {
        let c = crate::remote::credentials()?;
        Maimbrain::new(&c.server, &c.token, upstream, http)
    }

    fn call(&self, path: &str, body: Value) -> Result<Output, String> {
        let url = format!("{}/api/v1/generate/{path}", self.server);
        let r = self.http.post(&url, &[("Authorization".into(), format!("Bearer {}", self.token))], "application/json", serde_json::to_vec(&body).unwrap())?;
        let v: Value = serde_json::from_slice(&r.body).unwrap_or_default();
        if r.status != 200 {
            return Err(server_error(r.status, &v, self.upstream));
        }
        let data = v["data"].as_str().ok_or("Maimbrain's keys: no data in the server's response")?;
        let u = &v["usage"];
        if let (Some(used), Some(budget)) = (u["used_usd"].as_f64(), u["budget_usd"].as_f64()) {
            self.last.replace(Some(format!("Maimbrain's keys: ${used:.2} of ${budget:.2} used this month")));
        }
        Ok(Output {
            bytes: unb64(data)?,
            mime: v["mime"].as_str().unwrap_or("application/octet-stream").to_string(),
            text: v["text"].as_str().map(str::to_string),
            cost_usd: v["cost_usd"].as_f64(),
        })
    }
}

fn server_error(status: u16, v: &Value, upstream: &str) -> String {
    let msg = v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
    match status {
        401 => "Maimbrain's keys: your sign-in has expired or was revoked; run mb login".into(),
        403 | 402 => format!("Maimbrain's keys: {msg}"),
        429 => format!("Maimbrain's keys: {msg} (or use your own key: mb keys set {upstream})"),
        _ => format!("Maimbrain's keys: {msg}"),
    }
}

impl Provider for Maimbrain {
    fn name(&self) -> &'static str {
        match self.upstream {
            "gemini" => "gemini on Maimbrain's keys",
            _ => "elevenlabs on Maimbrain's keys",
        }
    }

    fn image(&self, req: &ImageRequest) -> Result<Output, String> {
        let refs: Vec<Value> = req.refs.iter().map(|r| json!({ "mime": r.mime, "data": b64(&r.bytes) })).collect();
        let mut body = json!({
            "provider": self.upstream, "model": req.model, "prompt": req.prompt, "refs": refs,
            "aspect": req.aspect, "resolution": req.resolution,
        });
        if let Some(s) = req.seed {
            body["seed"] = json!(s);
        }
        self.call("image", body)
    }

    fn audio(&self, req: &AudioRequest) -> Result<Output, String> {
        let mut body = json!({ "provider": self.upstream, "model": req.model, "prompt": req.prompt, "loop": req.looping });
        if let Some(s) = req.seconds {
            body["seconds"] = json!(s);
        }
        if let Some(s) = req.seed {
            body["seed"] = json!(s);
        }
        self.call(if req.kind == Kind::Sfx { "sfx" } else { "music" }, body)
    }

    fn note(&self) -> Option<String> {
        self.last.borrow().clone()
    }

    fn via(&self) -> Option<&'static str> {
        Some("maimbrain")
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::tests::{Fake, json as reply};
    use super::super::provider::{MockHint, RefImage};
    use super::*;
    use std::rc::Rc;

    struct FakeServer(Result<Option<Status>, String>);

    impl Server for FakeServer {
        fn status(&self) -> Result<Option<Status>, String> {
            self.0.clone()
        }
    }

    fn allowed(used: f64) -> Status {
        Status {
            allowed: true,
            username: "ada".into(),
            budget_usd: 10.0,
            used_usd: used,
            remaining_usd: 10.0 - used,
            resets_at: "2026-11-01T00:00:00Z".into(),
            providers: vec![
                ServerProvider {
                    name: "gemini".into(),
                    models: vec![
                        ServerModel { id: "gemini-nano-banana-2.1".into() },
                        ServerModel { id: "lyria-3-clip-preview".into() },
                    ],
                },
                ServerProvider { name: "elevenlabs".into(), models: vec![ServerModel { id: "eleven_text_to_sound_v2".into() }] },
            ],
            message: None,
        }
    }

    fn denied() -> Status {
        Status { allowed: false, username: "bob".into(), message: Some("@bob can't use Maimbrain's generation keys".into()), ..Status::default() }
    }

    const NB: Option<&str> = Some("gemini-nano-banana-2.1");

    #[test]
    fn your_own_key_wins_in_auto() {
        let s = FakeServer(Ok(Some(allowed(0.0))));
        assert!(matches!(decide("gemini", NB, Mode::Auto, Some("GEMINI_API_KEY"), &s), Ok(Route::Own(f)) if f == "GEMINI_API_KEY"));
        assert!(matches!(decide("mock", None, Mode::Maimbrain, None, &s), Ok(Route::Mock)));
    }

    #[test]
    fn without_a_key_an_allowed_account_uses_maimbrain() {
        let s = FakeServer(Ok(Some(allowed(1.5))));
        assert!(matches!(decide("gemini", NB, Mode::Auto, None, &s), Ok(Route::Maimbrain(st)) if st.username == "ada"));
        assert!(matches!(decide("elevenlabs", Some("eleven_text_to_sound_v2"), Mode::Auto, None, &s), Ok(Route::Maimbrain(_))));
    }

    #[test]
    fn without_a_key_or_access_the_message_says_how_to_get_either() {
        let e = decide("gemini", NB, Mode::Auto, None, &FakeServer(Ok(None))).unwrap_err();
        assert!(e.contains("mb keys set gemini") && e.contains(ASK) && e.contains("mb login"), "{e}");
        assert!(e.find("mb login") < e.find("GEMINI_API_KEY"), "signed out, signing in comes first: {e}");
        let e = decide("gemini", NB, Mode::Auto, None, &FakeServer(Ok(Some(denied())))).unwrap_err();
        assert!(e.contains("mb keys set gemini") && e.contains("@bob doesn't have it"), "{e}");
        let e = decide("gemini", NB, Mode::Auto, None, &FakeServer(Err("can't reach the server".into()))).unwrap_err();
        assert!(e.contains("mb keys set gemini") && e.contains("can't reach"), "{e}");
        // Providers the server doesn't call: the plain message.
        let e = decide("openai", None, Mode::Auto, None, &FakeServer(Ok(Some(allowed(0.0))))).unwrap_err();
        assert!(e.contains("mb keys set openai") && !e.contains("Maimbrain"), "{e}");
    }

    #[test]
    fn three_d_models_go_through_maimbrain_when_the_server_lists_tripo() {
        let mut st = allowed(0.0);
        let s = FakeServer(Ok(Some(st.clone())));
        let e = decide("tripo", Some("P1-20260311"), Mode::Auto, None, &s).unwrap_err();
        assert!(e.contains("mb keys set tripo") && e.contains("don't cover tripo P1-20260311"), "without 3D access: {e}");
        st.providers.push(ServerProvider { name: "tripo".into(), models: vec![ServerModel { id: "P1-20260311".into() }] });
        let s = FakeServer(Ok(Some(st)));
        assert!(matches!(decide("tripo", Some("P1-20260311"), Mode::Auto, None, &s), Ok(Route::Maimbrain(_))));
        assert!(matches!(decide("tripo", Some("P1-20260311"), Mode::Auto, Some("TRIPO_API_KEY"), &s), Ok(Route::Own(_))));
        assert!(decide("meshy", Some("meshy-7.1"), Mode::Maimbrain, None, &s).unwrap_err().contains("tripo (3D models), not meshy"));
        assert!(Maimbrain::new("http://s", "t", "tripo", Box::new(Rc::new(Fake::new(vec![])))).is_err(), "tripo goes through tripo.rs");
    }

    #[test]
    fn models_the_server_does_not_offer_are_refused() {
        let s = FakeServer(Ok(Some(allowed(0.0))));
        let e = decide("gemini", Some("gemini-3-pro-image"), Mode::Auto, None, &s).unwrap_err();
        assert!(e.contains("don't cover gemini gemini-3-pro-image"), "{e}");
    }

    #[test]
    fn own_keys_never_asks_the_server() {
        struct Panics;
        impl Server for Panics {
            fn status(&self) -> Result<Option<Status>, String> {
                panic!("asked the server")
            }
        }
        let e = decide("gemini", NB, Mode::Own, None, &Panics).unwrap_err();
        assert!(e.contains("--own-keys"), "{e}");
        assert!(matches!(decide("gemini", NB, Mode::Own, Some("mb keys"), &Panics), Ok(Route::Own(_))));
        assert!(!no_provider("images", "  gemini: …", Mode::Own, &Panics).contains("Maimbrain"));
    }

    #[test]
    fn forcing_maimbrain_skips_your_key_and_explains_refusals() {
        let s = FakeServer(Ok(Some(allowed(0.0))));
        assert!(matches!(decide("gemini", NB, Mode::Maimbrain, Some("GEMINI_API_KEY"), &s), Ok(Route::Maimbrain(_))));
        let e = decide("openai", None, Mode::Maimbrain, None, &s).unwrap_err();
        assert!(e.contains("not openai"), "{e}");
        let e = decide("gemini", NB, Mode::Maimbrain, None, &FakeServer(Ok(Some(denied())))).unwrap_err();
        assert_eq!(e, "@bob can't use Maimbrain's generation keys");
        let e = decide("gemini", NB, Mode::Maimbrain, None, &FakeServer(Ok(None))).unwrap_err();
        assert!(e.contains("mb login"), "{e}");
    }

    #[test]
    fn flags_set_the_mode() {
        assert_eq!(mode_for(Some("maimbrain"), false, Mode::Auto), Ok(Mode::Maimbrain));
        assert_eq!(mode_for(Some("gemini"), true, Mode::Maimbrain), Ok(Mode::Own));
        assert_eq!(mode_for(None, false, Mode::Own), Ok(Mode::Own), "MB_KEYS_SOURCE when no flag");
        assert!(mode_for(Some("maimbrain"), true, Mode::Auto).is_err());
    }

    #[test]
    fn the_budget_is_checked_before_calling() {
        let s = allowed(9.98);
        assert!(check_budget(&s, Some(0.0336), "gemini").unwrap_err().contains("$0.02 left of your $10.00"));
        assert!(check_budget(&s, Some(0.01), "gemini").is_ok());
    }

    #[test]
    fn image_request_goes_to_the_server_and_the_raw_output_comes_back() {
        let fake = Rc::new(Fake::new(vec![reply(
            200,
            json!({ "data": b64(b"\x89PNG..."), "mime": "image/png", "text": "a moth", "cost_usd": 0.0336,
                    "usage": { "used_usd": 0.5336, "budget_usd": 10.0, "remaining_usd": 9.4664 } }),
        )]));
        let m = Maimbrain::new("https://maimbrain.test/", "tok", "gemini", Box::new(fake.clone())).unwrap();
        let req = ImageRequest {
            model: "gemini-nano-banana-2.1".into(),
            prompt: "a moth".into(),
            refs: vec![RefImage { bytes: vec![1, 2, 3], mime: "image/png".into() }],
            aspect: "1:1".into(),
            resolution: "1K".into(),
            transparent: true,
            seed: Some(7),
            quality: 1,
            hint: MockHint::default(),
        };
        let out = m.image(&req).unwrap();
        assert_eq!(out.bytes, b"\x89PNG...");
        assert_eq!(out.cost_usd, Some(0.0336));
        assert_eq!(m.note().unwrap(), "Maimbrain's keys: $0.53 of $10.00 used this month");
        assert_eq!(m.via(), Some("maimbrain"));
        let (url, headers, body) = &fake.sent.borrow()[0];
        assert_eq!(url, "https://maimbrain.test/api/v1/generate/image");
        assert_eq!(headers[0], ("Authorization".to_string(), "Bearer tok".to_string()));
        assert_eq!(body["provider"], "gemini");
        assert_eq!(body["model"], "gemini-nano-banana-2.1");
        assert_eq!(body["refs"][0]["data"], b64(&[1, 2, 3]));
        assert_eq!(body["seed"], 7);
        assert_eq!(body["aspect"], "1:1");
    }

    #[test]
    fn audio_requests_and_refusals() {
        let fake = Rc::new(Fake::new(vec![
            reply(200, json!({ "data": b64(&[0, 0]), "mime": "audio/pcm;rate=24000", "cost_usd": 0.004 })),
            reply(402, json!({ "error": "this would go over your $10.00 monthly budget ($0.01 left)" })),
            reply(403, json!({ "error": "ask the Maimbrain team for access, or set your own key with mb keys set elevenlabs" })),
        ]));
        let m = Maimbrain::new("http://s", "t", "elevenlabs", Box::new(fake.clone())).unwrap();
        let req = AudioRequest {
            model: "eleven_text_to_sound_v2".into(),
            prompt: "zap".into(),
            kind: Kind::Sfx,
            seconds: Some(1.0),
            looping: false,
            seed: Some(3),
            negative: None,
        };
        assert_eq!(m.audio(&req).unwrap().mime, "audio/pcm;rate=24000");
        assert!(m.audio(&req).err().unwrap().contains("monthly budget"));
        assert!(m.audio(&req).err().unwrap().contains("ask the Maimbrain team"));
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "http://s/api/v1/generate/sfx");
        assert_eq!(sent[0].2["seconds"], 1.0);
        assert_eq!(sent[0].2["loop"], false);
        assert!(Maimbrain::new("http://s", "t", "openai", Box::new(Rc::new(Fake::new(vec![])))).is_err());
    }
}
