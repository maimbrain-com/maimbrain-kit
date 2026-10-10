//! Generated assets: `mb art` (images), `mb music`, `mb sfx`, `mb model` (3D),
//! `mb regen`, `mb keys` and `mb models`. A provider makes the raw image,
//! sound or model; mb post-processes it to the platform's rules (sizes,
//! transparency, pixel grids, palettes, PNG size; loops, loudness, Ogg
//! Vorbis; triangle budgets, meters and orientation, textures), writes it into
//! the game's `assets/`, and records how it was made in a sidecar
//! (`<asset>.gen.json`) so it can be audited and regenerated.
//!
//! Layout inside a game directory:
//!     art/style.toml        the style guide (mb art init)
//!     art/raw/              what providers returned (git-ignored; lets `mb regen --reprocess` redo post-processing for free)
//!     art/variants/         candidates from --variants N, and a contact sheet to compare them
//!     art/preview/          each asset over a checkerboard, dark and light ground, enlarged: look at it
//!     assets/x.png + assets/x.png.gen.json (assets/x.glb + assets/x.glb.gen.json)

pub mod art;
pub mod atlas;
pub mod audio;
pub mod audition;
pub mod elevenlabs;
pub mod glb;
pub mod google;
pub mod graph;
pub mod grid;
pub mod img;
pub mod ip;
pub mod keys;
pub mod maimbrain;
pub mod meshy;
pub mod mock3d;
pub mod model;
pub mod orient;
pub mod mock;
pub mod openai;
pub mod provider;
pub mod raster;
pub mod shape;
pub mod sky;
pub mod score;
pub mod sound;
pub mod spine;
pub mod style;
pub mod tension;
#[cfg(test)]
mod tension_mix;
pub mod tripo;
pub mod voice;
pub mod voice_account;
pub mod weights;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

/// Now as an RFC 3339 UTC timestamp (MB_GEN_DATE overrides it, for reproducible tests).
pub fn now_utc() -> String {
    if let Ok(d) = std::env::var("MB_GEN_DATE") {
        return d;
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    utc(secs)
}

/// A Unix time as `2026-10-10T01:32:14Z`.
pub fn utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// A fresh seed when none was given (recorded, so `mb regen --seed` can lock it).
pub fn fresh_seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    (mock::hash(&[&t.to_le_bytes(), &std::process::id().to_le_bytes()]) % 2_000_000_000) + 1
}

/// The provenance sidecar written next to every generated asset.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Sidecar {
    /// "mb <version> art sprite" etc.
    pub generator: String,
    pub date: String,
    pub provider: String,
    pub model: String,
    /// "maimbrain" when the call went through Maimbrain's keys (the server called the provider).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// The user's description of the asset.
    pub prompt: String,
    /// Exactly what was sent to the model.
    pub full_prompt: String,
    pub seed: u64,
    /// Whether the provider honors seeds (if not, regenerating gives a new result).
    pub seed_honored: bool,
    #[serde(default)]
    pub references: Vec<FileRef>,
    #[serde(default)]
    pub style: Option<FileRef>,
    pub est_cost_usd: Option<f64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// The raw provider output, relative to the game directory.
    pub raw: String,
    pub raw_sha256: String,
    pub output_sha256: String,
    /// Names the IP check let through because of --allow-name.
    #[serde(default)]
    pub allowed_names: Vec<String>,
    /// Text the model returned alongside the asset, if any.
    #[serde(default)]
    pub model_text: Option<String>,
    /// What `mb regen` needs to do it again (the resolved options).
    pub recipe: Value,
    /// Facts about the result (size, colors, loop points, BPM…).
    #[serde(default)]
    pub result: Value,
    pub note: String,
    /// Edits made to the file after mb wrote it, oldest first (`mb art edited`).
    /// `output_sha256` stays the generated file's; the last edit's is the file's now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<Derived>,
}

/// One edit to a generated file after mb wrote it: a script in the game
/// (`tools/bright.py`), another program, or a hand edit.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Derived {
    /// What made it: a script's path in the game, a program, or "hand".
    pub tool: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// What it did, in a few words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub date: String,
    /// The file before the edit (the generated output, or the previous edit's).
    pub input_sha256: String,
    /// The file after it.
    pub output_sha256: String,
}

impl Derived {
    pub fn describe(&self) -> String {
        let mut s = std::iter::once(self.tool.as_str()).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
        if let Some(n) = &self.note {
            s += &format!(" ({n})");
        }
        s
    }
}

impl Sidecar {
    /// The hash the file should have now: the last recorded edit's, else the generated output's.
    pub fn current_sha256(&self) -> &str {
        self.derived.last().map_or(&self.output_sha256, |d| &d.output_sha256)
    }

    /// How the file at hand relates to what was generated and the edits
    /// recorded since, for `mb art check` (None: it's the generated file).
    pub fn provenance_of(&self, bytes: &[u8]) -> Option<String> {
        let sha = sha256_hex(bytes);
        let chain = || self.derived.iter().map(Derived::describe).collect::<Vec<_>>().join(", then ");
        if sha == self.current_sha256() {
            (!self.derived.is_empty()).then(|| format!("edited after generation: {}", chain()))
        } else if sha == self.output_sha256 {
            Some(format!("the generated file again (its recorded edits are undone: {})", chain()))
        } else if self.derived.is_empty() {
            Some("edited since it was generated (say how: mb art edited <file> --tool …)".into())
        } else {
            Some(format!("edited since its last recorded edit ({}); say how: mb art edited <file> --tool …", chain()))
        }
    }
}

/// `mb art edited`: records an edit made to a generated file after mb wrote it.
pub fn record_edit(asset: &Path, tool: &str, args: &[String], note: Option<&str>) -> Result<(), String> {
    let mut sc = read_sidecar(asset)?;
    let bytes = std::fs::read(asset).map_err(|e| format!("{}: {e}", asset.display()))?;
    let sha = sha256_hex(&bytes);
    if sha == sc.current_sha256() {
        return Err(format!(
            "{} hasn't changed since {}: nothing to record (run the edit first, then this)",
            asset.display(),
            if sc.derived.is_empty() { "it was generated" } else { "its last recorded edit" }
        ));
    }
    let d = Derived {
        tool: tool.to_string(),
        args: args.to_vec(),
        note: note.map(str::to_string),
        date: now_utc(),
        input_sha256: sc.current_sha256().to_string(),
        output_sha256: sha,
    };
    eprintln!("recorded in {}: edited by {}", sidecar_path(asset).display(), d.describe());
    sc.derived.push(d);
    write_sidecar(asset, &sc)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileRef {
    pub path: String,
    pub sha256: String,
}

pub fn sidecar_path(asset: &Path) -> PathBuf {
    let mut s = asset.as_os_str().to_owned();
    s.push(".gen.json");
    PathBuf::from(s)
}

pub fn write_sidecar(asset: &Path, s: &Sidecar) -> Result<(), String> {
    let p = sidecar_path(asset);
    std::fs::write(&p, serde_json::to_string_pretty(s).unwrap() + "\n").map_err(|e| format!("{}: {e}", p.display()))
}

pub fn read_sidecar(asset: &Path) -> Result<Sidecar, String> {
    let p = sidecar_path(asset);
    let t = std::fs::read_to_string(&p).map_err(|_| format!("{} has no {} (it wasn't made by mb art/music/sfx)", asset.display(), p.display()))?;
    serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn note_for(provider: &str) -> String {
    match provider {
        "gemini" | "vertex" => "Generated with a Google model; Google embeds an imperceptible SynthID watermark.".into(),
        "openai" => "Generated with an OpenAI GPT Image model (C2PA metadata is not kept after post-processing).".into(),
        "elevenlabs" => "Generated with ElevenLabs; check your plan's license terms (the free plan has non-commercial terms).".into(),
        "tripo" => "Generated with Tripo; assets made on paid credits are yours, those made with free credits stay Tripo's (its terms).".into(),
        "meshy" => "Generated with Meshy; paid plans own their assets, the free plan's are CC BY 4.0 (credit Meshy).".into(),
        "mock" => "Placeholder from the offline mock provider: replace before publishing.".into(),
        _ => String::new(),
    }
}

/// Path relative to `base` with forward slashes (for sidecars and logs).
pub fn rel(path: &Path, base: &Path) -> String {
    let p = path.strip_prefix(base).unwrap_or(path);
    p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// The game directory an asset belongs to, from its sidecar's output path.
pub fn game_of(asset: &Path, out_rel: &str) -> PathBuf {
    let depth = out_rel.split('/').count();
    let mut g = asset.to_path_buf();
    for _ in 0..depth {
        g = g.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    if g.as_os_str().is_empty() { PathBuf::from(".") } else { g }
}

/// The spending limit for one command: --max-cost, MB_MAX_COST, else $1.
pub fn max_cost(flag: Option<f64>) -> f64 {
    flag.or_else(|| std::env::var("MB_MAX_COST").ok().and_then(|v| v.parse().ok())).unwrap_or(1.0)
}

pub fn check_cost(est: Option<f64>, n: u32, limit: f64, what: &str) -> Result<(), String> {
    match est {
        Some(e) => {
            let total = e * n as f64;
            eprintln!(
                "  cost: est. ${e:.4}{} for {what} (provider list price, Oct 2026; `mb models` for the table)",
                if n > 1 { format!(" × {n} = ${total:.4}") } else { String::new() }
            );
            if total > limit + 1e-9 {
                return Err(format!(
                    "estimated ${total:.2} is over the ${limit:.2} limit for one command; pass --max-cost {:.2} (or set MB_MAX_COST) to go ahead",
                    total
                ));
            }
        }
        None => eprintln!("  cost: unknown for {what}"),
    }
    Ok(())
}

/// Makes art/ with a .gitignore for raw outputs and previews (they're big and reproducible).
pub fn ensure_art_dir(game: &Path) -> Result<PathBuf, String> {
    let art = game.join("art");
    std::fs::create_dir_all(art.join("raw")).map_err(|e| format!("{}: {e}", art.display()))?;
    let gi = art.join(".gitignore");
    if !gi.exists() {
        std::fs::write(&gi, "# Provider outputs, variants and previews: large, and reproducible with `mb regen`.\nraw/\nvariants/\npreview/\n# Voice previews (mb voice design): only for choosing; the chosen voice is in voices.toml.\nvoices/\n")
            .map_err(|e| format!("{}: {e}", gi.display()))?;
    }
    Ok(art)
}

/// The manifest's logical size and orientation, if the directory is a game.
pub fn game_screen(game: &Path) -> ([u32; 2], bool) {
    let text = std::fs::read_to_string(game.join("manifest.toml")).unwrap_or_default();
    match mb_format::Manifest::parse(&text) {
        Ok(m) => {
            let [w, h] = m.logical_size;
            ([w, h], h >= w)
        }
        Err(_) => ([360, 640], true),
    }
}

#[derive(clap::Args)]
pub struct RegenArgs {
    /// A generated asset (assets/x.png, assets/music.ogg, assets/x.glb, icon.png): reads its .gen.json.
    pub asset: PathBuf,
    /// Use this seed (lock one you liked, or try another).
    #[arg(long)]
    pub seed: Option<u64>,
    /// A new seed (the default reuses the recorded one).
    #[arg(long)]
    pub new_seed: bool,
    /// Replace the description.
    #[arg(long)]
    pub prompt: Option<String>,
    /// Redo only the post-processing from the saved raw output (no provider call, free).
    #[arg(long)]
    pub reprocess: bool,
    /// Images: background keying tolerance (Oklab distance; the recorded one, else 0.09; raise if a halo is left).
    #[arg(long)]
    pub tolerance: Option<f64>,
    /// Images: the background color to key out, #rrggbb (default: the one the asset was generated with).
    #[arg(long)]
    pub key: Option<String>,
    /// Effects: add the bass's harmonics so a phone speaker plays it (`mb sfx --brighten`).
    #[arg(long, conflicts_with = "no_brighten")]
    pub brighten: bool,
    /// Effects: undo --brighten.
    #[arg(long)]
    pub no_brighten: bool,
    /// Music: write these layers instead of the recorded ones (base,hi / tension / base,hi,tension).
    /// Without --new-seed, --seed, --prompt, --provider or --model, from the take you have (free).
    #[arg(long, value_name = "LIST", conflicts_with = "add_layers")]
    pub layers: Option<String>,
    /// Music: add layers to the recorded ones, e.g. `--add-layers tension` for the tension kit.
    /// Without --new-seed, --seed, --prompt, --provider or --model, from the take you have (free).
    #[arg(long, value_name = "LIST")]
    pub add_layers: Option<String>,
    /// Another provider (gemini, vertex, openai, elevenlabs, tripo, meshy, mock), or maimbrain to use Maimbrain's keys.
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    /// Only use your own API keys, never Maimbrain's (also MB_KEYS_SOURCE=own).
    #[arg(long)]
    pub own_keys: bool,
    /// Make N candidates in art/variants/ instead (images and models; pick one with `mb art pick` or `mb model --pick`).
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    #[arg(long)]
    pub max_cost: Option<f64>,
    /// Show the prompt and the cost, call nothing.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn regen(mut a: RegenArgs) -> Result<(), String> {
    a.provider = maimbrain::take_flags(a.provider.as_deref(), a.own_keys)?;
    let sc = read_sidecar(&a.asset)?;
    if !sc.derived.is_empty() && !a.dry_run {
        let edits = sc.derived.iter().map(Derived::describe).collect::<Vec<_>>().join(", then ");
        eprintln!("note: {} was edited after generation ({edits}); the new file won't have that: run it again, then `mb art edited`", a.asset.display());
    }
    if (a.brighten || a.no_brighten) && sc.recipe["kind"] != "sfx" {
        return Err("--brighten is for effects (mb sfx); for music, ask for brighter instruments and a lighter low end".into());
    }
    if (a.layers.is_some() || a.add_layers.is_some()) && sc.recipe["kind"] != "music" {
        return Err("--layers and --add-layers are for music (mb music)".into());
    }
    if sc.recipe["kind"] == "model" {
        return model::regen(&a, &sc);
    }
    // --provider maimbrain on an asset made with a provider Maimbrain's keys don't cover: their default for its kind.
    if maimbrain::mode() == maimbrain::Mode::Maimbrain && a.provider.is_none() && !maimbrain::SERVER_PROVIDERS.contains(&sc.provider.as_str()) {
        a.provider = Some(if sc.recipe["kind"] == "sfx" { "elevenlabs" } else { "gemini" }.to_string());
    }
    let kind = sc.recipe["kind"].as_str().unwrap_or("").to_string();
    match kind.as_str() {
        "music" | "sfx" => sound::regen(&a, &sc),
        "voice" => voice::regen(&a, &sc),
        _ => art::regen(&a, &sc),
    }
}

/// `mb models`: the catalog with prices and which keys are set.
pub fn models() -> Result<(), String> {
    use provider::{MODELS, Media, price_label};
    // Signed out, this asks nothing.
    use maimbrain::Server as _;
    let server = maimbrain::Live.status().ok().flatten().filter(|s| s.allowed);
    for (media, title) in [(Media::Image, "Images (mb art)"), (Media::Music, "Music (mb music)"), (Media::Sfx, "Sound effects (mb sfx)"), (Media::Model, "3D models (mb model)"), (Media::Voice, "Voices (mb voice)")] {
        eprintln!("{title}");
        for m in MODELS.iter().filter(|m| m.media == media) {
            let key = if m.provider == "mock" || keys::lookup(m.provider).is_some() {
                "✓"
            } else if server.as_ref().is_some_and(|s| s.supports(m.provider, Some(m.id))) {
                "M"
            } else {
                "·"
            };
            eprintln!(
                "  {key} {:10} {:28} {}{}{}",
                m.provider,
                m.id,
                price_label(m),
                if m.default { " · default" } else { "" },
                if m.seed { "" } else { " · no seed" }
            );
            eprintln!("    {:10} {}", "", m.note);
        }
        eprintln!();
    }
    eprintln!("✓ = your key is set (mb keys list); M = no key of yours, but Maimbrain's keys cover it for your account.");
    eprintln!("{}", maimbrain::describe(&maimbrain::Live).1);
    eprintln!("Prices: ai.google.dev/gemini-api/docs/pricing, cloud.google.com/vertex-ai/generative-ai/pricing,");
    eprintln!("developers.openai.com/api/docs/pricing, elevenlabs.io/pricing/api, developers.tripo3d.com/en/pricing,");
    eprintln!("docs.meshy.ai/api/pricing (as of October 2026; check before big batches).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths() {
        let d = now_utc();
        assert_eq!(d.len(), 20, "{d}");
        assert!(d.ends_with('Z'));
        assert_eq!(game_of(Path::new("games/x/assets/m.png"), "assets/m.png"), PathBuf::from("games/x"));
        assert_eq!(game_of(Path::new("games/x/icon.png"), "icon.png"), PathBuf::from("games/x"));
        assert_eq!(game_of(Path::new("icon.png"), "icon.png"), PathBuf::from("."));
        assert_eq!(rel(Path::new("games/x/assets/m.png"), Path::new("games/x")), "assets/m.png");
        assert_eq!(sidecar_path(Path::new("a/b.png")), PathBuf::from("a/b.png.gen.json"));
    }

    #[test]
    fn cost_limit() {
        assert!(check_cost(Some(0.04), 3, 1.0, "x").is_ok());
        assert!(check_cost(Some(0.134), 10, 1.0, "x").is_err());
    }
}
