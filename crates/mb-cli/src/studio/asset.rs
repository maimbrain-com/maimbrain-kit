//! `mb studio-asset`: generated art and music made to fit a Studio project
//! (docs/STUDIO.md "Art and sound: the asset router"). The Maimbrain server
//! makes the provider calls (its router, credits and keys) and hands the raw
//! outputs here; this does the post-processing `mb art` and `mb music` do,
//! to the project's rules:
//!
//! - sprites and frames: the flat background keyed out, every frame at one
//!   scale, reduced to the sprite's grid (16×16 or 32×32 art pixels) by
//!   nearest-neighbour cells, and snapped to the project's 16 colours, each
//!   pixel solid or clear; frames side by side;
//! - tiles: a square crop, made to wrap, reduced to 16×16 and snapped; with
//!   `--frame`, that frame of the project's existing set is replaced (or
//!   one added at the end);
//! - the icon: 256×256, opaque, as 64×64 art pixels on the palette;
//! - music: a seamless loop at the platform's loudness, as a mono Ogg (the
//!   same as `mb music`);
//! - sfx: a sound effect trimmed and at the platform's loudness, as a mono
//!   Ogg (the same as `mb sfx`).
//!
//!     mb studio-asset <project> <kind> <name> --out DIR --candidate a.png[,b.png…] …
//!         [--size 16|32] [--frame N] [--key "#ff00ff"]
//!         [--prompt TEXT] [--full-prompt TEXT] [--provider P] [--model M] [--seed N] [--cost USD]
//!
//! Each `--candidate` is one result (for frames, its frames in order). They
//! are written as DIR/1.png, DIR/2.png… (music and sfx: .ogg), each with its
//! `.gen.json` sidecar, and DIR/report.json says what came out:
//! `{ kind, name, palette, candidates: [{ file, width, height, frames, colours, warnings }] }`.
//! The project itself is only read (studio.toml, and a tile set being added to).
//!
//! An engine 2.0 project's pictures (image, frames, walker, background,
//! layers, tile, ui, icon) are asset2.rs's, with `--plan` for their prompts
//! and `--dims`, `--pixel`, `--stands` and the rest; its music and sound
//! effects are these.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::generate::img::{self, Anchor, Image};
use crate::generate::{Sidecar, sha256_hex};

#[derive(clap::Args)]
pub struct AssetArgs {
    /// The Studio project's directory (its studio.toml names the palette).
    pub project: PathBuf,
    /// sprite, frames, tile, icon, music or sfx.
    pub kind: String,
    /// The asset's name (sprites/<name>.png, tiles/<name>.png, music/<name>.ogg, sounds/<name>.ogg; the icon is icon.png).
    pub name: String,
    /// Where the candidates and report.json go.
    #[arg(long)]
    pub out: PathBuf,
    /// One result's raw output: a file, or (frames, layers) its steps' files in order, comma-separated. Repeat for each candidate.
    #[arg(long = "candidate")]
    pub candidates: Vec<String>,
    /// Sprites and frames: the frame's size in art pixels, 16 or 32 (default 16).
    #[arg(long)]
    pub size: Option<u32>,
    /// Tiles: the frame of the project's tile set to replace (its frame count adds one at the end).
    #[arg(long)]
    pub frame: Option<u32>,
    /// Sound effects: one that loops seamlessly (a 3D game's `loop("name")`), as `mb sfx --loop` makes it.
    #[arg(long = "loop")]
    pub looping: bool,
    /// The flat colour the model was asked to paint behind a sprite (helps keying).
    #[arg(long, default_value = "#ff00ff")]
    pub key: String,
    /// What was asked for, in the creator's words (for the sidecar).
    #[arg(long, default_value = "")]
    pub prompt: String,
    /// Exactly what the model was sent (for the sidecar).
    #[arg(long, default_value = "")]
    pub full_prompt: String,
    #[arg(long, default_value = "unknown")]
    pub provider: String,
    #[arg(long, default_value = "unknown")]
    pub model: String,
    #[arg(long, default_value_t = 0)]
    pub seed: u64,
    /// What one candidate cost, in USD (for the sidecar).
    #[arg(long)]
    pub cost: Option<f64>,
    /// Engine 2.0: print the prompts the provider calls take, as JSON, and make nothing (asset2.rs).
    #[arg(long)]
    pub plan: bool,
    /// Engine 2.0: the picture's size in pixels, WxH (a layer's, a walker's cell, a frame's).
    #[arg(long, value_parser = super::asset2::parse_dims)]
    pub dims: Option<[u32; 2]>,
    /// Engine 2.0: pixel art (`pixel = true`): crisp square pixels, one texel each.
    #[arg(long)]
    pub pixel: bool,
    /// Engine 2.0 frames: how many.
    #[arg(long)]
    pub frames: Option<u32>,
    /// Engine 2.0 frames: the movement ("a walk cycle").
    #[arg(long, default_value = "")]
    pub motion: String,
    /// Engine 2.0 layers: how many, far to near.
    #[arg(long)]
    pub count: Option<u32>,
    /// Engine 2.0: it stands on the ground: its footing goes in its .toml.
    #[arg(long)]
    pub stands: bool,
    /// Engine 2.0 tiles: not made to wrap (an edge or a ledge piece).
    #[arg(long)]
    pub no_seamless: bool,
    /// sky: where the painting's horizon is, 0–1 from its top (found by default).
    #[arg(long)]
    pub horizon: Option<f32>,
    /// Engine 2.0: a picture in the project whose subject to redraw (repeatable).
    #[arg(long = "ref")]
    pub refs: Vec<String>,
    /// Engine 2.0 portrait: its expressions, neutral first (comma-separated; the blink row is added).
    #[arg(long, value_delimiter = ',')]
    pub expressions: Vec<String>,
    #[command(flatten)]
    pub voice: VoiceArgs,
    /// Engine 2.0 music: one take in zones, "town: lanterns; grove: bamboo" (a music graph; asset_music.rs).
    #[arg(long)]
    pub zones: Option<String>,
    /// Engine 2.0 music: one take in sections, a,b,c (a music graph).
    #[arg(long)]
    pub sections: Option<String>,
    /// Engine 2.0 music: a song per zone, "harbor: bluesy rock, 128 bpm; neon: flamenco" (graphs and a setlist).
    #[arg(long)]
    pub setlist: Option<String>,
    /// Engine 2.0 music: layers, base,hi or base,hi,tension (or tension).
    #[arg(long)]
    pub layers: Option<String>,
    /// Engine 2.0 music: the tempo to ask for.
    #[arg(long)]
    pub bpm: Option<f32>,
    /// Engine 2.0 layers, with `--plan`: the far layer as it came back (repeatable, one an
    /// option): the nearer layers' key colour is picked away from its colours.
    #[arg(long = "far")]
    pub far: Vec<String>,
    /// Engine 2.0 layers: the key colour the nearer layers were asked for (what `--plan
    /// --far` picked), so they're keyed by it.
    #[arg(long)]
    pub layer_key: Option<String>,
    /// What each candidate's provider calls were (for the sidecars): a JSON file,
    /// `{ "prompts": [a step's prompt, …], "candidates": [[{ "seed", "image", "of" }, a step's …], …] }`.
    /// `image` and `of` say which picture of a call that made several it was.
    #[arg(long)]
    pub made: Option<PathBuf>,
}

/// Engine 2.0 `voice` (asset_voice.rs): a spoken line, or a design's previews.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct VoiceArgs {
    /// The line's id (voices/<who>/<line>.ogg).
    #[arg(long)]
    pub line: Option<String>,
    /// What's said, as written ([audio tags] left out of the subtitle).
    #[arg(long, default_value = "")]
    pub text: String,
    /// How it's said (the audio tag sent in front of it).
    #[arg(long)]
    pub emotion: Option<String>,
    /// Speaking speed 0.7–1.2: time-stretched after for models that ignore it.
    #[arg(long)]
    pub speed: Option<f32>,
    /// The provider's voice id it was spoken with (for the sidecar).
    #[arg(long = "voice", default_value = "")]
    pub voice_id: String,
    /// The candidates are a voice design's previews.
    #[arg(long)]
    pub preview: bool,
}

/// One provider call a raw came from: its seed, which of its pictures it
/// was when it made several (`image` of `of`, from 1), and what that picture
/// cost (`usd`: its share of the call, with any attempt it replaced).
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Call {
    pub seed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
}

/// What a file made by `calls` cost: theirs, added up, when the server said
/// each; else `fallback` (`--cost`).
pub fn cost_of(calls: &[Call], fallback: Option<f64>) -> Option<f64> {
    if calls.is_empty() || calls.iter().any(|c| c.usd.is_none()) {
        return fallback;
    }
    Some((calls.iter().filter_map(|c| c.usd).sum::<f64>() * 1e6).round() / 1e6)
}

/// `--made`: each step's prompt, and each candidate's calls, a step each.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Made {
    #[serde(default)]
    pub prompts: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<Vec<Call>>,
}

impl Made {
    pub fn read(a: &AssetArgs) -> Result<Made, String> {
        let Some(p) = &a.made else { return Ok(Made::default()) };
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Candidate `i`'s call for step `s` (from 0), if the server said.
    pub fn call(&self, i: usize, s: usize) -> Option<&Call> {
        self.candidates.get(i)?.get(s)
    }

    /// The prompt step `s` was sent, else `fallback` (`--full-prompt`).
    pub fn prompt<'a>(&'a self, s: usize, fallback: &'a str) -> &'a str {
        self.prompts.get(s).map(String::as_str).filter(|p| !p.is_empty()).unwrap_or(fallback)
    }
}

/// Texels per art pixel while fitting (as `mb art` does for pixel styles).
const WORK: u32 = 8;
/// Keying tolerance (mb art's default).
const TOLERANCE: f32 = 0.09;
/// The icon: 256×256 pixels, 64×64 art pixels.
const ICON: u32 = 256;
const ICON_GRID: u32 = 64;

pub fn run(a: AssetArgs) -> Result<(), String> {
    if !super::is_project(&a.project) {
        return Err(format!("{} isn't a Studio project (no studio.toml)", a.project.display()));
    }
    if a.name.is_empty() || a.name.contains(['/', '\\', '.']) {
        return Err(format!("bad asset name {:?}", a.name));
    }
    let t = super::read_toml(&a.project.join("studio.toml"))?;
    // Engine 2.0's pictures (images/, tiles/, any size): asset2.rs. Its music
    // and sound effects are 1.0's, below.
    if super::asset2::is_v2(&t) && a.kind != "music" && a.kind != "sfx" {
        let report = if a.plan { super::asset2::plan(&a, &t)? } else { super::asset2::process(&a, &t)? };
        if !a.plan {
            std::fs::create_dir_all(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?;
            let path = a.out.join("report.json");
            std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
        }
        println!("{}", serde_json::to_string(&report).unwrap());
        return Ok(());
    }
    // Engine 2.0's music that follows the game (forms and layers): asset_music.rs.
    if super::asset2::is_v2(&t) && a.kind == "music" && (super::asset_music::wanted(&a) || a.plan) {
        if !a.plan && a.candidates.is_empty() {
            return Err("--candidate: at least one".into());
        }
        let report = if a.plan { super::asset_music::plan(&a)? } else { super::asset_music::process(&a)? };
        if !a.plan {
            let path = a.out.join("report.json");
            std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
        }
        println!("{}", serde_json::to_string(&report).unwrap());
        return Ok(());
    }
    if [a.zones.is_some(), a.sections.is_some(), a.setlist.is_some(), a.layers.is_some(), a.bpm.is_some()].contains(&true) {
        return Err("--zones, --sections, --setlist, --layers and --bpm are for an engine 2.0 project's music".into());
    }
    if a.candidates.is_empty() {
        return Err("--candidate: at least one".into());
    }
    let (pal, pal_name) = super::palette(&t)?;
    let palette: Vec<[u8; 3]> = pal.iter().map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect();
    let background = t.get("background").and_then(toml::Value::as_integer).unwrap_or(0).clamp(0, 15) as usize;
    std::fs::create_dir_all(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?;
    let calls = Made::read(&a)?;
    let mut made = Vec::new();
    for (i, c) in a.candidates.iter().enumerate() {
        let files: Vec<PathBuf> = c.split(',').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
        if files.is_empty() {
            return Err(format!("candidate {} names no files", i + 1));
        }
        let n = i + 1;
        let calls = calls.candidates.get(i).cloned().unwrap_or_default();
        let entry = if a.kind == "music" || a.kind == "sfx" {
            sound(&a, &files, n, &calls)?
        } else {
            picture(&a, &files, n, &palette, background, &calls)?
        };
        made.push(entry);
    }
    let report = json!({ "kind": a.kind, "name": a.name, "palette": pal_name, "candidates": made });
    let path = a.out.join("report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}

fn read_image(p: &Path) -> Result<(Image, Vec<u8>), String> {
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let im = Image::decode(&bytes).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok((im, bytes))
}

/// One image candidate, processed and written as <out>/<n>.png with its sidecar.
fn picture(a: &AssetArgs, files: &[PathBuf], n: usize, palette: &[[u8; 3]], background: usize, calls: &[Call]) -> Result<Value, String> {
    let mut raws = Vec::new();
    let mut raw_bytes = Vec::new();
    for f in files {
        let (im, b) = read_image(f)?;
        raws.push(im);
        raw_bytes.extend_from_slice(&b);
    }
    let mut warnings = Vec::new();
    let (out, frames) = match a.kind.as_str() {
        "sprite" | "frames" => {
            let size = a.size.unwrap_or(16);
            if size != 16 && size != 32 {
                return Err("--size is 16 or 32 (Arcade sprites)".into());
            }
            if a.kind == "sprite" && raws.len() > 1 {
                return Err("a sprite is one image: use frames for more".into());
            }
            if raws.len() > 8 {
                return Err(format!("{} frames: Arcade sprites have 8 at most", raws.len()));
            }
            (sprite_sheet(&raws, size, palette, &a.key, &mut warnings), raws.len() as u32)
        }
        "tile" => {
            if raws.len() != 1 {
                return Err("a tile is one image".into());
            }
            let tile = tile(&raws[0], palette);
            into_set(&a.project, &a.name, tile, a.frame)?
        }
        "icon" => {
            if raws.len() != 1 {
                return Err("the icon is one image".into());
            }
            (icon(&raws[0], palette, palette[background]), 1)
        }
        k => return Err(format!("unknown kind {k:?}: sprite, frames, tile, icon, music or sfx")),
    };
    let png = out.encode_png();
    let dest = a.out.join(format!("{n}.png"));
    std::fs::write(&dest, &png).map_err(|e| format!("{}: {e}", dest.display()))?;
    let colours = out.distinct_colors(64).iter().filter(|c| c[3] > 0).count();
    if colours == 0 {
        warnings.push("nothing is left of the picture after keying: regenerate it".into());
    }
    let mut result = json!({ "width": out.w, "height": out.h, "frames": frames, "colours": colours, "warnings": warnings });
    if !calls.is_empty() {
        result["calls"] = json!(calls);
    }
    let seed = calls.first().map_or(a.seed, |c| c.seed);
    sidecar(a, &dest, &png, &raw_bytes, seed, cost_of(calls, a.cost), json!({ "kind": a.kind, "size": a.size, "frame": a.frame, "frames": frames, "key": a.key }), result)?;
    Ok(json!({ "file": format!("{n}.png"), "width": out.w, "height": out.h, "frames": frames, "colours": colours, "warnings": warnings }))
}

/// Frames side by side, `size` art pixels square each, keyed, at one scale
/// (so the thing doesn't change size from frame to frame), feet on the same
/// line, pixelated and snapped to the palette.
pub(super) fn sprite_sheet(raws: &[Image], size: u32, palette: &[[u8; 3]], key: &str, warnings: &mut Vec<String>) -> Image {
    let hint = img::KeyHint { expected: img::parse_hex(key).ok(), top: false };
    let keyed: Vec<Image> = raws
        .iter()
        .map(|r| {
            if r.has_transparent_border() {
                r.trimmed()
            } else {
                let (k, rep) = img::key_out_with(r, TOLERANCE, hint);
                if rep.cleared < 0.05 {
                    warnings.push("almost nothing was keyed out: the background may not be flat".into());
                }
                k.trimmed()
            }
        })
        .collect();
    let cell = size * WORK;
    // A pixel of margin all round, so nothing touches the frame's edge.
    let margin = WORK;
    let maxw = keyed.iter().map(|k| k.w).max().unwrap_or(1).max(1) as f32;
    let maxh = keyed.iter().map(|k| k.h).max().unwrap_or(1).max(1) as f32;
    let k = ((cell - 2 * margin) as f32 / maxw).min((cell - 2 * margin) as f32 / maxh);
    let anchor = if keyed.len() > 1 { Anchor::Bottom } else { Anchor::Center };
    let mut sheet = Image::new(size * keyed.len() as u32, size, [0; 4]);
    for (i, p) in keyed.iter().enumerate() {
        let placed = crate::generate::art::place(p, k, cell, cell, margin, anchor);
        let mut c = img::pixelate(&placed, size, size);
        img::lock_palette(&mut c, palette);
        sheet.blit(&c, i as u32 * size, 0);
    }
    sheet
}

/// A 16×16 tile: the middle square, made to wrap both ways, reduced and snapped (opaque).
pub(super) fn tile(raw: &Image, palette: &[[u8; 3]]) -> Image {
    let side = raw.w.min(raw.h);
    let sq = raw.crop((raw.w - side) / 2, (raw.h - side) / 2, side, side).flatten(palette[0]);
    let wrapped = img::seamless(&sq, true, true, 0.12);
    let mut t = img::pixelate(&wrapped.resize(16 * WORK, 16 * WORK), 16, 16);
    for p in &mut t.px {
        p[3] = 255;
    }
    img::lock_palette(&mut t, palette);
    t
}

/// The tile set `name` with `tile` as frame `frame` (replacing it, or added
/// after the last); a new set when there's none (or no frame is given).
fn into_set(project: &Path, name: &str, tile: Image, frame: Option<u32>) -> Result<(Image, u32), String> {
    let path = project.join(format!("tiles/{name}.png"));
    let Some(f) = frame else { return Ok((tile, 1)) };
    let set = match std::fs::read(&path) {
        Ok(b) => Image::decode(&b).map_err(|e| format!("tiles/{name}.png: {e}"))?,
        Err(_) if f == 0 => return Ok((tile, 1)),
        Err(_) => return Err(format!("tiles/{name}.png doesn't exist yet, so its first tile is frame 0")),
    };
    if set.h != 16 || set.w % 16 != 0 {
        return Err(format!("tiles/{name}.png is {}×{}: a tile set is 16 high, its tiles side by side", set.w, set.h));
    }
    let count = set.w / 16;
    if f > count {
        return Err(format!("tiles/{name}.png has {count} tiles (frames 0–{}): --frame {count} adds one", count.saturating_sub(1)));
    }
    let mut out = Image::new(16 * count.max(f + 1), 16, [0; 4]);
    out.blit(&set, 0, 0);
    out.blit(&tile, f * 16, 0);
    Ok((out, count.max(f + 1)))
}

/// The icon: the picture covering the square, as 64×64 art pixels on the
/// palette, scaled up to 256×256, opaque (on the project's background colour).
pub(super) fn icon(raw: &Image, palette: &[[u8; 3]], bg: [u8; 3]) -> Image {
    let base = if raw.is_opaque() { raw.clone() } else { raw.flatten(bg) };
    let mut g = img::pixelate(&base.cover(ICON_GRID * WORK, ICON_GRID * WORK), ICON_GRID, ICON_GRID);
    for p in &mut g.px {
        if p[3] == 0 {
            *p = [bg[0], bg[1], bg[2], 255];
        }
        p[3] = 255;
    }
    img::lock_palette(&mut g, palette);
    g.scale_nearest(ICON / ICON_GRID)
}

/// One music or sound-effect candidate: `mb music`'s loop and loudness, or
/// `mb sfx`'s trim and loudness, as <out>/<n>.ogg.
fn sound(a: &AssetArgs, files: &[PathBuf], n: usize, calls: &[Call]) -> Result<Value, String> {
    use crate::generate::sound::{SoundJob, execute};
    let [file] = files else { return Err(format!("a {} candidate is one file", a.kind)) };
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mime = match file.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        x => return Err(format!("{}: {} comes as .wav, .mp3, .ogg or .flac, not .{x}", file.display(), a.kind)),
    }
    .to_string();
    let job = SoundJob {
        kind: a.kind.clone(),
        name: a.name.clone(),
        subject: a.prompt.clone(),
        out: format!("{n}.ogg"),
        provider: a.provider.clone(),
        model: a.model.clone(),
        // Each option is its own call, at its own seed.
        seed: calls.first().map_or(a.seed, |c| c.seed),
        bars: None,
        bpm: None,
        seconds: None,
        layers: false,
        tension: false,
        key: None,
        brighten: false,
        split_hz: 700.0,
        once: false,
        // Music loops; a sound effect is a one-shot unless asked to loop.
        looping: a.kind == "music" || a.looping,
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
    };
    let raw = format!("{n}.raw (not kept: made by the Maimbrain server)");
    execute(&a.out, &job, false, f64::MAX, Some((bytes, mime, raw)))?;
    let dest = a.out.join(format!("{n}.ogg"));
    // `mb music`'s sidecar, with what the server knows that mb couldn't.
    let mut sc = crate::generate::read_sidecar(&dest)?;
    sc.generator = format!("mb {} studio-asset {}", env!("CARGO_PKG_VERSION"), a.kind);
    sc.via = Some("maimbrain".into());
    sc.full_prompt = if a.full_prompt.is_empty() { sc.full_prompt } else { a.full_prompt.clone() };
    sc.est_cost_usd = cost_of(calls, a.cost);
    sc.cost_usd = sc.est_cost_usd;
    if !calls.is_empty() {
        sc.result["calls"] = json!(calls);
    }
    crate::generate::write_sidecar(&dest, &sc)?;
    let seconds = sc.result.get("loop_seconds").or_else(|| sc.result.get("seconds")).cloned().unwrap_or(Value::Null);
    let warnings = sc.result.get("warnings").cloned().unwrap_or(json!([]));
    Ok(json!({ "file": format!("{n}.ogg"), "seconds": seconds, "warnings": warnings }))
}

fn sidecar(a: &AssetArgs, dest: &Path, out: &[u8], raw: &[u8], seed: u64, cost: Option<f64>, recipe: Value, result: Value) -> Result<(), String> {
    let sc = Sidecar {
        generator: format!("mb {} studio-asset {}", env!("CARGO_PKG_VERSION"), a.kind),
        date: crate::generate::now_utc(),
        provider: a.provider.clone(),
        model: a.model.clone(),
        via: Some("maimbrain".into()),
        prompt: a.prompt.clone(),
        full_prompt: a.full_prompt.clone(),
        seed,
        seed_honored: true,
        references: Vec::new(),
        style: None,
        est_cost_usd: cost,
        cost_usd: cost,
        raw: "(not kept: made by the Maimbrain server)".into(),
        raw_sha256: sha256_hex(raw),
        output_sha256: sha256_hex(out),
        allowed_names: Vec::new(),
        model_text: None,
        recipe,
        result,
        note: String::new(),
        derived: Vec::new(),
    };
    crate::generate::write_sidecar(dest, &sc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUNSET: &str = "title = \"T\"\nkind = \"2d\"\npalette = \"sunset16\"\nbackground = 11\n";

    /// A fresh project directory for one test.
    fn project(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mb-studio-asset-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("studio.toml"), SUNSET).unwrap();
        dir
    }

    /// A 512×512 "model output": an orange blob on a flat magenta background.
    fn raw_sprite(path: &Path, w: u32) {
        let mut im = Image::new(512, 512, [255, 0, 255, 255]);
        for y in 150..400 {
            for x in (256 - w / 2)..(256 + w / 2) {
                im.set(x, y, [230, 110, 60, 255]);
            }
        }
        std::fs::write(path, im.encode_png()).unwrap();
    }

    fn args(dir: &Path, kind: &str, candidates: Vec<String>) -> AssetArgs {
        AssetArgs {
            project: dir.to_path_buf(),
            kind: kind.into(),
            name: "cat".into(),
            out: dir.join("out"),
            candidates,
            size: None,
            frame: None,
            looping: false,
            key: "#ff00ff".into(),
            prompt: "an orange cat".into(),
            full_prompt: String::new(),
            provider: "fal".into(),
            model: "test".into(),
            seed: 7,
            cost: Some(0.002),
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
            expressions: Vec::new(),
            voice: Default::default(),
            zones: None,
            sections: None,
            setlist: None,
            layers: None,
            bpm: None,
            far: Vec::new(),
            layer_key: None,
            made: None,
        }
    }

    fn on_palette(im: &Image) -> bool {
        let pal: Vec<[u8; 3]> = super::super::PALETTES[0].1.iter().map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect();
        im.px.iter().all(|p| p[3] == 0 || (p[3] == 255 && pal.contains(&[p[0], p[1], p[2]])))
    }

    #[test]
    fn a_sprite_is_snapped_to_the_grid_and_the_palette() {
        let dir = project("a_sprite_is_snapped_to_the_grid_and_the_palette");
        raw_sprite(&dir.as_path().join("a.png"), 200);
        raw_sprite(&dir.as_path().join("b.png"), 100);
        let a = args(dir.as_path(), "sprite", vec![dir.as_path().join("a.png").display().to_string(), dir.as_path().join("b.png").display().to_string()]);
        run(a).unwrap();
        for n in [1, 2] {
            let im = Image::decode(&std::fs::read(dir.as_path().join(format!("out/{n}.png"))).unwrap()).unwrap();
            assert_eq!((im.w, im.h), (16, 16));
            assert!(on_palette(&im));
            assert!(im.px.iter().any(|p| p[3] == 255) && im.px.iter().any(|p| p[3] == 0), "keyed, with something left");
            assert!(dir.as_path().join(format!("out/{n}.png.gen.json")).is_file());
        }
        let report: Value = serde_json::from_str(&std::fs::read_to_string(dir.as_path().join("out/report.json")).unwrap()).unwrap();
        assert_eq!(report["candidates"].as_array().unwrap().len(), 2);
        assert_eq!(report["palette"], "sunset16");
    }

    #[test]
    fn frames_go_side_by_side_at_one_scale() {
        let dir = project("frames_go_side_by_side_at_one_scale");
        let files: Vec<String> = (0..3)
            .map(|i| {
                let p = dir.as_path().join(format!("f{i}.png"));
                raw_sprite(&p, 120);
                p.display().to_string()
            })
            .collect();
        let mut a = args(dir.as_path(), "frames", vec![files.join(",")]);
        a.size = Some(32);
        run(a).unwrap();
        let im = Image::decode(&std::fs::read(dir.as_path().join("out/1.png")).unwrap()).unwrap();
        assert_eq!((im.w, im.h), (96, 32));
        assert!(on_palette(&im));
        // The same picture three times comes out the same three times.
        assert_eq!(im.crop(0, 0, 32, 32).px, im.crop(64, 0, 32, 32).px);
    }

    #[test]
    fn a_tile_goes_into_its_set() {
        let dir = project("a_tile_goes_into_its_set");
        std::fs::create_dir_all(dir.as_path().join("tiles")).unwrap();
        std::fs::write(dir.as_path().join("tiles/cat.png"), Image::new(32, 16, [0x1b, 0x14, 0x28, 255]).encode_png()).unwrap();
        let raw = dir.as_path().join("t.png");
        std::fs::write(&raw, Image::new(512, 512, [140, 210, 100, 255]).encode_png()).unwrap();
        let mut a = args(dir.as_path(), "tile", vec![raw.display().to_string()]);
        a.frame = Some(2);
        run(a).unwrap();
        let im = Image::decode(&std::fs::read(dir.as_path().join("out/1.png")).unwrap()).unwrap();
        assert_eq!((im.w, im.h), (48, 16));
        assert!(on_palette(&im));
        assert_eq!(im.get(40, 8), [0x8f, 0xd1, 0x6a, 255], "the green snapped to the palette's green");
        assert_eq!(im.get(0, 0), [0x1b, 0x14, 0x28, 255], "the set's other tiles are kept");
        let mut a = args(dir.as_path(), "tile", vec![raw.display().to_string()]);
        a.frame = Some(5);
        assert!(run(a).unwrap_err().contains("--frame 2 adds one"));
    }

    #[test]
    fn the_icon_is_opaque_and_on_the_palette() {
        let dir = project("the_icon_is_opaque_and_on_the_palette");
        raw_sprite(&dir.as_path().join("i.png"), 300);
        run(args(dir.as_path(), "icon", vec![dir.as_path().join("i.png").display().to_string()])).unwrap();
        let im = Image::decode(&std::fs::read(dir.as_path().join("out/1.png")).unwrap()).unwrap();
        assert_eq!((im.w, im.h), (256, 256));
        assert!(im.is_opaque() && on_palette(&im));
    }

    #[test]
    fn music_becomes_a_loop() {
        let dir = project("music_becomes_a_loop");
        // 12 s of a steady 120 BPM click over a chord.
        let rate = 44_100;
        let x: Vec<f32> = (0..rate * 12)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let beat = (t * 2.0).fract();
                0.2 * (t * 220.0 * std::f32::consts::TAU).sin() + 0.2 * (t * 330.0 * std::f32::consts::TAU).sin() + if beat < 0.03 { 0.5 } else { 0.0 }
            })
            .collect();
        let wav = dir.as_path().join("m.wav");
        std::fs::write(&wav, crate::generate::audio::wav_bytes(&x, rate)).unwrap();
        run(args(dir.as_path(), "music", vec![wav.display().to_string()])).unwrap();
        assert!(dir.as_path().join("out/1.ogg").is_file());
        let sc = crate::generate::read_sidecar(&dir.as_path().join("out/1.ogg")).unwrap();
        assert_eq!(sc.via.as_deref(), Some("maimbrain"));
        assert_eq!(sc.provider, "fal");
    }

    #[test]
    fn a_sound_effect_is_trimmed_to_its_sound() {
        let dir = project("a_sound_effect_is_trimmed_to_its_sound");
        // 0.3 s of silence, a 0.4 s falling zap, then 0.5 s of silence.
        let rate = 24_000;
        let x: Vec<f32> = (0..rate * 6 / 5)
            .map(|i| {
                let t = i as f32 / rate as f32 - 0.3;
                if (0.0..0.4).contains(&t) { 0.6 * (t * (900.0 - 1200.0 * t) * std::f32::consts::TAU).sin() * (1.0 - t / 0.4) } else { 0.0 }
            })
            .collect();
        let wav = dir.as_path().join("s.wav");
        std::fs::write(&wav, crate::generate::audio::wav_bytes(&x, rate)).unwrap();
        let mut a = args(dir.as_path(), "sfx", vec![wav.display().to_string()]);
        a.provider = "elevenlabs".into();
        run(a).unwrap();
        let out = std::fs::read(dir.as_path().join("out/1.ogg")).unwrap();
        let (ch, r) = crate::generate::audio::decode(&out, "audio/ogg").unwrap();
        let secs = ch[0].len() as f32 / r as f32;
        assert!(secs < 0.9, "trimmed to about the zap: {secs} s");
        let sc = crate::generate::read_sidecar(&dir.as_path().join("out/1.ogg")).unwrap();
        assert_eq!((sc.via.as_deref(), sc.provider.as_str()), (Some("maimbrain"), "elevenlabs"));
        assert!(sc.generator.ends_with("studio-asset sfx"));
    }

    /// `--loop`: `mb sfx --loop`'s loop (the whole take, its seam
    /// crossfaded), not a one-shot trimmed to its sound.
    #[test]
    fn a_looping_sound_effect_keeps_its_whole_take() {
        let dir = project("a_looping_sound_effect_keeps_its_whole_take");
        // A hum with 0.3 s of quiet before and after: a one-shot would be trimmed to the hum.
        let rate = 24_000;
        let x: Vec<f32> = (0..rate * 21 / 10)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let level = if (0.3..1.8).contains(&t) { 0.5 } else { 0.0 };
                level * (t * 110.0 * std::f32::consts::TAU).sin()
            })
            .collect();
        let wav = dir.as_path().join("hum.wav");
        std::fs::write(&wav, crate::generate::audio::wav_bytes(&x, rate)).unwrap();
        let secs = |looping: bool| {
            let mut a = args(dir.as_path(), "sfx", vec![wav.display().to_string()]);
            a.provider = "elevenlabs".into();
            a.looping = looping;
            run(a).unwrap();
            let (ch, r) = crate::generate::audio::decode(&std::fs::read(dir.as_path().join("out/1.ogg")).unwrap(), "audio/ogg").unwrap();
            ch[0].len() as f32 / r as f32
        };
        let (once, looped) = (secs(false), secs(true));
        assert!(once < 1.7, "a one-shot is trimmed: {once} s");
        assert!(looped > 1.9, "a loop keeps the take (less its crossfade): {looped} s");
    }

    #[test]
    fn names_and_kinds_are_checked() {
        let dir = project("names_and_kinds_are_checked");
        let mut a = args(dir.as_path(), "sprite", vec!["x.png".into()]);
        a.name = "../cat".into();
        assert!(run(a).unwrap_err().contains("bad asset name"));
        raw_sprite(&dir.as_path().join("a.png"), 100);
        let a = args(dir.as_path(), "statue", vec![dir.as_path().join("a.png").display().to_string()]);
        assert!(run(a).unwrap_err().contains("unknown kind"));
    }
}
