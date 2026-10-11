//! `mb model`: a generated 3D model as a game-ready `.glb`. A provider
//! (tripo by default, meshy, or the offline mock) makes the mesh, its PBR
//! textures and, when asked, a rig and animation clips; mb then merges the
//! clips into one file, fits a triangle budget, scales it to meters, turns
//! it to face −z and puts its origin on the ground (or at its center),
//! resizes and re-encodes the textures, can add a glow, drops what mb3d
//! doesn't read, checks it the way the validator and mb3d will, and writes
//! `assets/<name>.glb`, a sidecar and a contact-sheet preview.
//!
//!     mb model games/koi koi "a stylized koi with glowing fins" --animate swim --glow 2
//!     mb model games/run hero --prompt "a fox adventurer" --animate idle,run --size 1.2
//!     mb model games/run crate "a wooden crate" --polys 500 --ref art/ref/crate.jpg

use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::provider::{self, Media, ModelRequest, RefImage};
use super::shape::{self, Origin, Placement, TexFormat};
use super::style::Style;
use super::{FileRef, Sidecar, glb, raster};

#[derive(Args, Clone)]
pub struct ModelArgs {
    /// Game directory.
    pub game: PathBuf,
    /// Asset name: writes assets/<name>.glb. (`mb model presets` lists Tripo's clips;
    /// `mb model info <glb>` says where a model points.)
    #[arg(default_value = "")]
    pub name: String,
    /// What it is: subject, shape, colors, materials. Original designs only.
    #[arg(conflicts_with = "prompt_flag")]
    pub prompt: Option<String>,
    /// The same, as a flag.
    #[arg(long = "prompt", value_name = "PROMPT")]
    pub prompt_flag: Option<String>,
    /// Make it from this image (a drawing, a render, a photo of something you own) instead.
    #[arg(long = "ref", value_name = "IMAGE")]
    pub reference: Option<String>,
    /// Triangle budget: asked of the provider, then enforced by simplifying [default: 8000].
    #[arg(long)]
    pub polys: Option<u32>,
    /// Add a skeleton (implied by --animate).
    #[arg(long)]
    pub rig: bool,
    /// Rig type instead of the provider's guess (tripo: biped, quadruped, hexapod, octopod, avian, serpentine, aquatic).
    #[arg(long)]
    pub rig_type: Option<String>,
    /// On a model made earlier: adopt this rig task (one made outside mb, or before mb recorded
    /// them) instead of rigging again; --animate then adds clips to it. `mb model <game> <name>
    /// --rig` (no prompt) rigs a model made earlier from its generation task; `--animate` adds
    /// clips to its recorded rig.
    #[arg(long, value_name = "TASK")]
    pub rig_task: Option<String>,
    /// Animation clips, comma-separated: walk, run, idle, swim, jump… or a provider's own id
    /// (tripo `preset:biped:wave_goodbye_01`, a meshy action id). Each becomes a clip named
    /// Walk, Run… in the .glb.
    #[arg(long, value_delimiter = ',')]
    pub animate: Vec<String>,
    /// Trim, rename, cut or drop clips, comma-separated: `Idle=0..4` (keep 0–4 s), `Strike=Slash@1.8..3.4`
    /// (a new clip from part of one; Slash goes unless kept by `Slash=…`), `Swing=Slash` (rename),
    /// `Chop=drop`. Recorded in the recipe; with --reprocess, edits replace those of the same name
    /// (`--clip none` clears them). `mb model presets` lists Tripo's clips and their long parts.
    #[arg(long = "clip", value_delimiter = ',', value_name = "NAME=[OLD@]FROM..TO")]
    pub clip: Vec<String>,
    /// Hard-surface skin: each vertex follows only its strongest bone, so armor moves as plates
    /// instead of bending and stretching like skin. `all` (the default), or bones and groups:
    /// `arms,legs`, `L_Forearm,R_Forearm`; `off` clears it on --reprocess. Needs a named rig.
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "BONES")]
    pub rigid: Option<String>,
    /// Re-weight what hangs off a limb (a cape, a skirt, a long binder) to one bone so arm and leg
    /// moves don't stretch it: `cape=spine` (behind the body), `skirt=hips` (below the hips, front
    /// and sides), `hanging=<bone>` (both), `auto` (cape=spine,skirt=hips); bones: spine, chest,
    /// hips, waist or a joint name. `none` clears them on --reprocess. Needs a named rig.
    #[arg(long, value_delimiter = ',', value_name = "REGION=BONE")]
    pub pin: Vec<String>,
    /// Longest side in meters [default: 1].
    #[arg(long)]
    pub size: Option<f32>,
    /// ground: stands on y = 0; center: centered on the origin. Default: center for swimmers
    /// (swim clips, fish prompts), else ground.
    #[arg(long, value_enum)]
    pub origin: Option<Origin>,
    /// Which way the model faces: -z (the way the camera looks and runners run; no turn needed)
    /// or +z (glTF's convention, as make_rigged_glb.py's fox) [default: -z].
    #[arg(long, value_parser = ["-z", "+z"], allow_hyphen_values = true)]
    pub facing: Option<String>,
    /// Extra turn about +y in degrees (fix a model that came back facing sideways).
    #[arg(long, allow_hyphen_values = true)]
    pub turn: Option<f32>,
    /// Turn so the longest horizontal side runs along z (fish, cars, boats).
    #[arg(long)]
    pub lengthwise: bool,
    /// Largest texture side in pixels (mb3d takes up to 2048) [default: 1024]. Keep 1024 for a hero
    /// or anything seen close (512 at the least); 512 or 256 for props seen small or many times.
    /// Changing it later is free: `mb regen assets/<name>.glb --reprocess` or
    /// `mb model <game> <name> --reprocess --texture-size N` redo it from the raw download.
    #[arg(long)]
    pub texture_size: Option<u32>,
    /// Largest side of the normal and metal-rough maps [default: half --texture-size, at least 64].
    /// Under matte paint they barely show, and a half-size normal map reads the same on a phone.
    #[arg(long)]
    pub maps_size: Option<u32>,
    /// Scale the metalness (0–1; 0.15 keeps 15 %): generated textures come back chrome-like,
    /// which reads near-black outdoors with nothing bright to reflect.
    #[arg(long)]
    pub metal: Option<f32>,
    /// Raise the roughness to at least this (0–1; 0.6 is satin), baked into the metal-rough map.
    #[arg(long)]
    pub min_roughness: Option<f32>,
    /// auto: JPEG unless alpha is needed; or jpeg, png [default: auto].
    #[arg(long, value_enum)]
    pub texture_format: Option<TexFormat>,
    /// Make the bright parts of its texture glow at this emissive strength (2–4 blooms nicely).
    #[arg(long)]
    pub glow: Option<f32>,
    /// What counts as bright for --glow: texel value 0–1 (0 = all of it) [default: 0.6].
    #[arg(long)]
    pub glow_above: Option<f32>,
    /// Light only the texels near these colors instead of the bright ones: a gem, a core,
    /// coolant stripes, on a white or yellow part that --glow would light all over
    /// (`#5aca2b` or `#5aca2b,#ff8020`; strength from --glow, else 3).
    #[arg(long, value_delimiter = ',', value_name = "#RRGGBB")]
    pub glow_color: Vec<String>,
    /// How near a texel must be to a --glow-color (Oklab distance, lightness counted half) [default: 0.15].
    #[arg(long)]
    pub glow_tolerance: Option<f32>,
    /// Write float geometry instead of quantized (KHR_mesh_quantization, about half the
    /// bytes, which mb3d reads on the web and on iOS).
    #[arg(long, conflicts_with = "compress")]
    pub no_compress: bool,
    /// With --reprocess: quantize again after a --no-compress.
    #[arg(long)]
    pub compress: bool,
    /// Turn a part into a stated convention from its shape: `tip=+y` (a blade or barrel's
    /// slim end up, its flat side to +z), `tip=+z up=+y` (a gun's muzzle forward), `face=+z`
    /// (a plate or shield's outer face to +z, its top up). Features: tip (the long axis'
    /// slimmer end), face (the thinnest axis, outer side), up (the model's own up). Replaces
    /// --facing and the facing check; --turn still turns it after. `mb model info` shows what it finds.
    #[arg(long, value_name = "FEATURE=AXIS …", allow_hyphen_values = true)]
    pub orient: Option<String>,
    /// With --reprocess: drop the recorded glow.
    #[arg(long, conflicts_with_all = ["glow", "glow_color"])]
    pub no_glow: bool,
    /// What to keep out (tripo).
    #[arg(long)]
    pub negative: Option<String>,
    /// Make N candidates in art/variants/ and pick one with --pick.
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    /// Promote candidate N from --variants to assets/<name>.glb.
    #[arg(long)]
    pub pick: Option<u32>,
    /// Redo only the processing of assets/<name>.glb from its saved raw output with the
    /// placement, budget and texture options given here (free, offline; the rest as recorded).
    #[arg(long)]
    pub reprocess: bool,
    /// Seed (tripo and the mock honor it).
    #[arg(long)]
    pub seed: Option<u64>,
    /// tripo (default), meshy or mock.
    #[arg(long)]
    pub provider: Option<String>,
    /// Model id (see `mb models`).
    #[arg(long)]
    pub model: Option<String>,
    /// Only use your own API keys, never Maimbrain's (also MB_KEYS_SOURCE=own).
    #[arg(long)]
    pub own_keys: bool,
    /// Output path relative to the game (default assets/<name>.glb).
    #[arg(long)]
    pub out: Option<String>,
    /// Ignore art/style.toml.
    #[arg(long)]
    pub no_style: bool,
    /// What it is: character (gets the game's [3d] style words), or part, prop, vehicle (one
    /// hard-surface object: the [3d_object] words and the palette, never the character words,
    /// which turn a gun into a little robot). auto guesses from the prompt [default: auto].
    #[arg(long, value_enum)]
    pub kind: Option<KindArg>,
    /// Let a flagged name through (it's yours, or a false positive). Recorded in the sidecar.
    #[arg(long)]
    pub allow_name: Vec<String>,
    /// Refuse if the estimated cost is above this many USD (default 1.00, or MB_MAX_COST).
    #[arg(long)]
    pub max_cost: Option<f64>,
    /// Print the prompt and the cost without calling the provider.
    #[arg(long)]
    pub dry_run: bool,
    /// Give it code-driven bones: a chain of joints head to tail with smooth weights
    /// (`spine_0` at the head), plus two-bone wings for birds. Replaces the provider's rig
    /// and clips; the game bends it with `Skeleton::spine` + `Swim`/`Wave`/`Flight`.
    /// auto (wings if it looks like a bird), head-to-tail, wings, off.
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "auto")]
    pub spine: Option<super::spine::SpineMode>,
    /// Joints along the spine (2–32) [default: 8].
    #[arg(long)]
    pub spine_joints: Option<u32>,
    /// Process a .glb you already have (no provider, free): one mb made earlier (faces −z)
    /// or another tool's (add --turn 180 for glTF's +z). Placement, budget, --spine apply.
    #[arg(long, value_name = "GLB", conflicts_with_all = ["reprocess", "variants", "reference"])]
    pub import: Option<PathBuf>,
}

/// `--kind`: what the model is, or `auto` to guess from the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum KindArg {
    Auto,
    Character,
    Part,
    Prop,
    Vehicle,
}

/// What a model is, which decides the style words it gets. A character (a
/// person, a creature, a mech) gets the game's `[3d]` words; a part (a gun, a
/// blade, an armor plate), a prop or a vehicle is one hard-surface object and
/// gets `[3d_object]`'s: style words written for characters ("anime mecha",
/// "visible mechanical joints") turn objects into little robots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ObjectKind {
    #[default]
    Character,
    Part,
    Prop,
    Vehicle,
}

impl ObjectKind {
    pub fn name(self) -> &'static str {
        match self {
            ObjectKind::Character => "character",
            ObjectKind::Part => "part",
            ObjectKind::Prop => "prop",
            ObjectKind::Vehicle => "vehicle",
        }
    }
}

const PART_WORDS: &[&str] = &[
    "gun", "cannon", "rifle", "pistol", "blaster", "launcher", "missile", "barrel", "muzzle", "turret", "weapon", "blade", "sword", "saber", "sabre", "knife",
    "dagger", "axe", "hammer", "spear", "lance", "bow", "staff", "wand", "hilt", "shield", "plate", "armor", "armour", "pauldron", "helmet", "visor", "gauntlet",
    "claw", "pincer", "drill", "thruster", "jetpack", "backpack", "booster", "antenna", "dish", "canister", "gem", "core", "reactor", "module", "pod", "part",
    "limb", "forearm", "attachment",
];
const VEHICLE_WORDS: &[&str] = &[
    "car", "truck", "kart", "bike", "motorcycle", "boat", "ship", "spaceship", "starship", "dropship", "airship", "plane", "jet", "helicopter", "train", "tram",
    "submarine", "rover", "buggy", "vehicle", "hovercraft", "sled", "tank",
];
const PROP_WORDS: &[&str] = &[
    "crate", "box", "chest", "rock", "boulder", "stone", "tree", "bush", "plant", "flower", "house", "building", "tower", "spire", "ruin", "wall", "fence", "gate",
    "door", "lamp", "lantern", "torch", "table", "chair", "bench", "pillar", "column", "statue", "crystal", "coin", "pickup", "sign", "crane", "bridge", "well",
    "prop", "furniture", "mushroom", "shell",
];
const CHARACTER_WORDS: &[&str] = &[
    "person", "man", "woman", "girl", "boy", "kid", "hero", "heroine", "knight", "warrior", "pilot", "wizard", "witch", "robot", "mech", "mecha", "android",
    "cyborg", "creature", "monster", "animal", "beast", "dragon", "fox", "wolf", "dog", "cat", "bird", "heron", "owl", "fish", "koi", "shark", "whale", "frog",
    "snake", "insect", "spider", "bug", "golem", "zombie", "alien", "character", "mascot", "adventurer", "soldier", "ninja", "samurai",
];

/// Guesses a model's kind from its prompt: the kind of the earliest word that
/// names one ("a beam saber for a giant robot" is a part, "a robot with a
/// saber" a character). Rigged or animated models are characters; no match is
/// a character too (the style words apply, as before `--kind`).
pub fn guess_kind(subject: &str, rigged: bool) -> ObjectKind {
    if rigged {
        return ObjectKind::Character;
    }
    let p = format!(" {} ", subject.to_lowercase().replace(|c: char| !c.is_alphanumeric(), " "));
    let mut best: Option<(usize, ObjectKind)> = None;
    for (words, kind) in [(CHARACTER_WORDS, ObjectKind::Character), (PART_WORDS, ObjectKind::Part), (VEHICLE_WORDS, ObjectKind::Vehicle), (PROP_WORDS, ObjectKind::Prop)] {
        for w in words {
            // Whole words, plurals too ("guns", "boxes").
            for form in [format!(" {w} "), format!(" {w}s "), format!(" {w}es ")] {
                if let Some(at) = p.find(&form) {
                    if best.is_none_or(|(b, _)| at < b) {
                        best = Some((at, kind));
                    }
                }
            }
        }
    }
    best.map_or(ObjectKind::Character, |(_, k)| k)
}

/// The resolved recipe, stored in the sidecar for `mb regen`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelJob {
    pub kind: String,
    pub name: String,
    pub subject: String,
    pub out: String,
    pub provider: String,
    pub model: String,
    pub seed: u64,
    #[serde(default)]
    pub reference: Option<String>,
    pub polys: u32,
    #[serde(default)]
    pub rig: bool,
    #[serde(default)]
    pub rig_type: Option<String>,
    #[serde(default)]
    pub animate: Vec<String>,
    /// `--clip` edits (trims, renames, cuts, drops), applied after the clips are merged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<String>,
    /// `--rigid`: bones whose vertices follow them alone ("all", groups, names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rigid: Option<String>,
    /// `--pin`: hanging regions re-weighted to a bone ("cape=spine").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin: Vec<String>,
    pub size: f32,
    pub origin: Origin,
    pub facing: String,
    #[serde(default)]
    pub turn: f32,
    #[serde(default)]
    pub lengthwise: bool,
    pub texture_size: u32,
    /// Normal and metal-rough maps' largest side; None: half `texture_size` (at least 64).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps_size: Option<u32>,
    /// `--metal`: metalness times this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metal: Option<f32>,
    /// `--min-roughness`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_roughness: Option<f32>,
    #[serde(default)]
    pub texture_format: TexFormat,
    #[serde(default)]
    pub glow: Option<f32>,
    #[serde(default)]
    pub glow_above: f32,
    /// `--glow-color`: light only texels near these colors (#rrggbb).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glow_colors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow_tolerance: Option<f32>,
    #[serde(default)]
    pub negative: Option<String>,
    #[serde(default)]
    pub no_style: bool,
    #[serde(default)]
    pub allow_names: Vec<String>,
    #[serde(default)]
    pub spine: super::spine::SpineMode,
    #[serde(default)]
    pub spine_joints: Option<u32>,
    /// What it is (`--kind`): decides the style words.
    #[serde(default)]
    pub object: ObjectKind,
    /// `--orient`: the convention a part is turned into ("tip=+y face=+z").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orient: Option<String>,
    /// Quantized geometry (KHR_mesh_quantization); `--no-compress` writes floats.
    #[serde(default = "yes")]
    pub compress: bool,
}

fn yes() -> bool {
    true
}

/// Which way a provider's models face (rotated to +z first, then to --facing).
fn provider_front(_provider: &str) -> &'static str {
    match _provider {
        // An imported .glb is taken as facing −z (mb's own output); --turn fixes others.
        "import" => "-z",
        // All face glTF's +z. (Tripo's docs describe an X-forward export option, but its P1
        // models come back facing +z: checked with a real koi, October 2026.)
        _ => "+z",
    }
}

/// Radians about +y that turn `front` to face +z.
fn to_plus_z(front: &str) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    match front {
        "+x" => -FRAC_PI_2,
        "-x" => FRAC_PI_2,
        "-z" => PI,
        _ => 0.0,
    }
}

const SWIMMERS: &[&str] = &["swim", "fly", "hover", "march"];

pub fn run(a: ModelArgs) -> Result<(), String> {
    // `mb model info <glb>`: where a model points, and how to hang it on a socket.
    if a.game.as_os_str() == "info" && !a.game.is_dir() {
        return super::orient::info(Path::new(&a.name));
    }
    // `mb model presets`: Tripo's clips, with what real runs showed.
    if a.game.as_os_str() == "presets" && !a.game.is_dir() {
        print!("{}", super::tripo::presets_text());
        return Ok(());
    }
    if let Some(n) = a.pick {
        return pick(&a.game, &a.name, n);
    }
    if a.name.is_empty() || a.name.contains(['/', '\\']) || a.name.starts_with('.') {
        return Err(format!("bad asset name {:?}", a.name));
    }
    if a.size.is_some_and(|s| !(0.01..=1000.0).contains(&s)) {
        return Err("--size is the longest side in meters (0.01–1000)".into());
    }
    if a.texture_size.is_some_and(|t| !(16..=2048).contains(&t)) || a.maps_size.is_some_and(|t| !(16..=2048).contains(&t)) {
        return Err("--texture-size and --maps-size must be 16–2048 (mb3d's limit is 2048²)".into());
    }
    if a.metal.is_some_and(|t| !(0.0..=1.0).contains(&t)) || a.min_roughness.is_some_and(|t| !(0.0..=1.0).contains(&t)) {
        return Err("--metal and --min-roughness are 0–1 (try --metal 0.15 --min-roughness 0.6)".into());
    }
    if a.polys.is_some_and(|p| !(12..=500_000).contains(&p)) {
        return Err("--polys must be 12–500000 (a frame's whole budget is ~150 k)".into());
    }
    for c in &a.glow_color {
        super::img::parse_hex(c).map_err(|e| format!("--glow-color: {e}"))?;
    }
    if let Some(o) = a.orient.as_deref().filter(|o| *o != "off") {
        super::orient::Orient::parse(o)?;
    }
    if a.glow_tolerance.is_some_and(|t| !(0.01..=1.0).contains(&t)) {
        return Err("--glow-tolerance is an Oklab distance, 0.01–1 (0.15 by default; 0.25 is loose)".into());
    }
    for c in a.clip.iter().filter(|c| !c.eq_ignore_ascii_case("none")) {
        shape::ClipEdit::parse(c)?;
    }
    // `--rig`/`--animate` with no prompt on a model made earlier: rig it, or add clips to its rig.
    let existing = a.game.join(a.out.clone().unwrap_or_else(|| format!("assets/{}.glb", a.name)));
    let made_earlier = a.prompt.is_none() && a.prompt_flag.is_none() && a.import.is_none() && wants_rig(&a) && super::read_sidecar(&existing).is_ok();
    if a.reprocess || made_earlier {
        return reprocess(&a);
    }
    if a.rig_task.is_some() {
        return Err(format!("--rig-task adopts a rig for a model made earlier: {} has no sidecar", existing.display()));
    }
    let subject = a.prompt.clone().or(a.prompt_flag.clone()).or_else(|| a.import.as_ref().map(|_| a.name.replace(['_', '-'], " "))).ok_or("say what to make: mb model <game> <name> \"<prompt>\" (or --prompt)")?;
    let style = if a.no_style { Style::default() } else { Style::load(&a.game)?.0 };
    let style_provider = style.provider.as_deref().filter(|p| provider::default_model(p, Media::Model).is_some());
    let flag = if a.import.is_some() { Some("import".to_string()) } else { super::maimbrain::take_flags(a.provider.as_deref(), a.own_keys)? };
    let order = ["tripo", "meshy"];
    let chosen = if flag.as_deref() == Some("import") { Ok("import".to_string()) } else { provider::choose(flag.as_deref(), style_provider, &order, "3D models") };
    let chosen = if chosen.is_err() && a.dry_run { Ok(order[0].to_string()) } else { chosen };
    let provider = chosen.map_err(|e| {
        format!("{e}\n\nNo key? Shapes built in code (the SDK's MeshData, the skill's make_glb.py and make_rigged_glb.py) cost nothing; see the skill's assets.md")
    })?;
    let model = a
        .model
        .clone()
        .or_else(|| (provider == "import").then(|| "file".to_string()))
        .or_else(|| provider::default_model(&provider, Media::Model).map(|m| m.id.to_string()))
        .ok_or(format!("{provider} has no 3D model (see `mb models`)"))?;
    let swimmer = a.animate.iter().any(|x| SWIMMERS.iter().any(|s| x.to_lowercase().contains(s))) || super::mock3d::is_fish(&subject);
    let rigged = a.rig || a.rig_type.is_some() || !a.animate.is_empty() || a.spine.is_some_and(|s| s != super::spine::SpineMode::Off);
    let object = match a.kind.unwrap_or(KindArg::Auto) {
        KindArg::Auto => {
            let k = guess_kind(&subject, rigged);
            if a.import.is_none() {
                eprintln!("kind: {} (from the prompt; --kind character|part|prop|vehicle to say)", k.name());
            }
            k
        }
        KindArg::Character => ObjectKind::Character,
        KindArg::Part => ObjectKind::Part,
        KindArg::Prop => ObjectKind::Prop,
        KindArg::Vehicle => ObjectKind::Vehicle,
    };
    let job = ModelJob {
        kind: "model".into(),
        name: a.name.clone(),
        subject,
        out: a.out.clone().unwrap_or_else(|| format!("assets/{}.glb", a.name)),
        provider,
        model,
        seed: a.seed.unwrap_or_else(super::fresh_seed),
        reference: a.reference.clone(),
        polys: a.polys.unwrap_or(8000),
        rig: a.rig || a.rig_type.is_some(),
        rig_type: a.rig_type.clone(),
        animate: a.animate.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        clips: merge_clip_edits(&[], &a.clip),
        rigid: a.rigid.clone().filter(|r| r != "off"),
        pin: a.pin.iter().filter(|p| *p != "none").cloned().collect(),
        size: a.size.unwrap_or(1.0),
        // A part hangs on a socket, so it's centered; the rest stand on the ground.
        origin: a.origin.unwrap_or(if swimmer || object == ObjectKind::Part { Origin::Center } else { Origin::Ground }),
        facing: a.facing.clone().unwrap_or_else(|| "-z".into()),
        turn: a.turn.unwrap_or(0.0),
        lengthwise: a.lengthwise,
        texture_size: a.texture_size.unwrap_or(1024),
        maps_size: a.maps_size,
        metal: a.metal,
        min_roughness: a.min_roughness,
        texture_format: a.texture_format.unwrap_or_default(),
        glow: a.glow,
        glow_above: a.glow_above.unwrap_or(0.6),
        glow_colors: a.glow_color.clone(),
        glow_tolerance: a.glow_tolerance,
        negative: a.negative.clone(),
        no_style: a.no_style,
        allow_names: a.allow_name.clone(),
        spine: a.spine.unwrap_or_default(),
        spine_joints: a.spine_joints,
        object,
        orient: a.orient.clone(),
        compress: !a.no_compress,
    };
    if let Some(src) = &a.import {
        return import(&a.game, job, src);
    }
    if job.provider == "tripo" {
        for n in super::tripo::check_actions(job.rig_type.as_deref(), &job.animate)? {
            eprintln!("  note: {n}");
        }
    }
    execute(&a.game, &job, a.variants.max(1), a.dry_run, super::max_cost(a.max_cost), None).map(|_| ())
}

/// Whether the command asks to rig or animate (a model made earlier, when no prompt is given).
fn wants_rig(a: &ModelArgs) -> bool {
    a.rig || a.rig_type.is_some() || a.rig_task.is_some() || !a.animate.is_empty()
}

/// `mb model <game> <name> --reprocess [options]`: the recorded recipe with the given processing options.
/// With `--rig`/`--animate`/`--rig-task` (with or without `--reprocess`), on a model made
/// earlier: rigs it from its generation task and/or adds clips to its rig first.
fn reprocess(a: &ModelArgs) -> Result<(), String> {
    let out = a.out.clone().unwrap_or_else(|| format!("assets/{}.glb", a.name));
    let asset = a.game.join(&out);
    let mut sc = super::read_sidecar(&asset).map_err(|e| format!("{e}\n(--reprocess redoes the model at --out, default assets/{}.glb, from its sidecar)", a.name))?;
    let mut job: ModelJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("sidecar recipe: {e}"))?;
    // Write it back where it is: the recipe's path may be where it was made
    // (`--out art/pending/…`) before it was moved.
    if job.out != out {
        eprintln!("  writing {out} (made as {})", job.out);
        job.out = out.clone();
    }
    if let Some(v) = a.polys {
        job.polys = v;
    }
    if let Some(v) = a.size {
        job.size = v;
    }
    if let Some(v) = a.origin {
        job.origin = v;
    }
    if let Some(v) = &a.facing {
        job.facing = v.clone();
    }
    if let Some(v) = a.turn {
        job.turn = v;
    }
    if a.lengthwise {
        job.lengthwise = true;
    }
    if let Some(v) = a.texture_size {
        job.texture_size = v;
    }
    if a.maps_size.is_some() {
        job.maps_size = a.maps_size;
    }
    if a.metal.is_some() {
        job.metal = a.metal;
    }
    if a.min_roughness.is_some() {
        job.min_roughness = a.min_roughness;
    }
    if let Some(v) = a.texture_format {
        job.texture_format = v;
    }
    if a.glow.is_some() {
        job.glow = a.glow;
    }
    if let Some(v) = a.glow_above {
        job.glow_above = v;
    }
    if !a.glow_color.is_empty() {
        job.glow_colors = a.glow_color.clone();
    } else if a.glow.is_some() && a.glow_above.is_some() {
        // --glow S --glow-above V asks for the brightness mode again.
        job.glow_colors.clear();
    }
    if a.glow_tolerance.is_some() {
        job.glow_tolerance = a.glow_tolerance;
    }
    if a.no_glow {
        job.glow = None;
        job.glow_colors.clear();
    }
    if let Some(o) = &a.orient {
        job.orient = (!o.trim().is_empty() && o != "off").then(|| o.clone());
    }
    if a.no_compress {
        job.compress = false;
    } else if a.compress {
        job.compress = true;
    }
    if let Some(v) = a.spine {
        job.spine = v;
    }
    if a.spine_joints.is_some() {
        job.spine_joints = a.spine_joints;
    }
    job.clips = merge_clip_edits(&job.clips, &a.clip);
    if let Some(r) = &a.rigid {
        job.rigid = (r != "off").then(|| r.clone());
    }
    for p in &a.pin {
        if p == "none" {
            job.pin.clear();
        } else {
            // A pin replaces the one for the same region.
            let region = |s: &str| s.split('=').next().unwrap_or(s).trim().to_lowercase();
            job.pin.retain(|o| region(o) != region(p) && !(p == "auto" || o == "auto"));
            job.pin.push(p.clone());
        }
    }
    let mut raw = read_raw(&a.game, &sc)?;
    if wants_rig(a) && !add_rig(a, &mut job, &mut sc, &mut raw)? {
        return Ok(());
    }
    execute(&a.game, &job, 1, false, f64::INFINITY, Some((raw, sc))).map(|_| ())
}

/// The rig task recorded in a sidecar's `result.tasks` (`rig (biped)`), with its rig type.
pub fn rig_task_of(result: &Value) -> Option<(String, String)> {
    result["tasks"].as_array()?.iter().rev().find_map(|t| {
        let step = t["step"].as_str()?;
        let ty = step.strip_prefix("rig (")?.strip_suffix(')')?;
        Some((t["id"].as_str()?.to_string(), ty.to_string()))
    })
}

/// `--rig`/`--animate`/`--rig-task` on a model made earlier: rigs it from its
/// generation task (or its uploaded raw .glb), or adopts a rig task made
/// earlier, and retargets the clips it doesn't have yet onto that rig, without
/// rigging again. The rigged model and clips go into `raw` and the recipe; the
/// tasks and the cost into the sidecar. False for a dry run (nothing to write).
fn add_rig(a: &ModelArgs, job: &mut ModelJob, sc: &mut Sidecar, raw: &mut Raw) -> Result<bool, String> {
    use super::maimbrain::{self, Mode};
    let provider_name = match maimbrain::take_flags(a.provider.as_deref(), a.own_keys)? {
        Some(p) => p,
        None if matches!(job.provider.as_str(), "import" | "") => "tripo".to_string(),
        // --provider maimbrain on a model from a provider its keys don't cover.
        None if maimbrain::mode() == Mode::Maimbrain && !maimbrain::SERVER_PROVIDERS.contains(&job.provider.as_str()) => "tripo".to_string(),
        None => job.provider.clone(),
    };
    let model_id = if provider_name == job.provider { job.model.clone() } else { provider::default_model(&provider_name, Media::Model).map(|m| m.id.to_string()).unwrap_or_default() };
    // A task lives on the account that ran it, yours or Maimbrain's: only those on this run's can be built on.
    let account = if a.dry_run { None } else { maimbrain::would_use(&provider_name, Some(&model_id)).then_some("maimbrain") };
    let here = |id: &str| a.dry_run || task_via(&sc.result, id) == account;
    let mut rig = rig_task_of(&sc.result);
    if let Some(id) = &a.rig_task {
        let ty = a.rig_type.clone().or(job.rig_type.clone()).unwrap_or_else(|| "biped".into());
        eprintln!("  adopting rig task {id} ({ty}); no new rig");
        rig = Some((id.clone(), ty));
    }
    let rigged = job.rig || !job.animate.is_empty();
    let known: Vec<String> = sc.result["clips"].as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(str::to_string)).collect();
    let new: Vec<String> = a
        .animate
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !job.animate.iter().any(|o| shape::same_clip(o, s)) && !known.iter().any(|k| shape::same_clip(k, s)))
        .collect();
    for s in a.animate.iter().filter(|s| !new.contains(s)) {
        eprintln!("  {s}: already a clip of it; not made again");
    }
    if let Some((id, _)) = rig.clone().filter(|(id, _)| a.rig_task.is_none() && !new.is_empty() && !here(id)) {
        eprintln!("  note: rig task {id} was made on {} keys, which these can't add clips to; rigging again", if account.is_some() { "your own" } else { "Maimbrain's" });
        rig = None;
    }
    // A rig mb can't add clips to (made before rig tasks were recorded): rig again, or adopt one with --rig-task.
    let need_rig = rig.is_none() && (!rigged || !new.is_empty() || a.rig_type.is_some());
    if rigged && rig.is_none() && !new.is_empty() && a.rig_task.is_none() {
        eprintln!("  warning: no rig task is recorded for it (rigged before mb recorded them), so it's rigged again; --rig-task <id> adopts the old one instead");
    }
    if !need_rig && new.is_empty() {
        let (id, ty) = rig.clone().unwrap_or_default();
        eprintln!("  already rigged ({ty}, task {id}); nothing to add (--animate adds clips to this rig)");
        if a.rig_task.is_some() {
            job.rig = true;
            job.rig_type = Some(ty.clone());
            add_tasks(sc, &[(format!("rig ({ty})"), id)], &["adopted the rig task with --rig-task".to_string()], 0.0, account);
        }
        return Ok(true);
    }
    let rig_type = a.rig_type.clone().or_else(|| rig.as_ref().map(|r| r.1.clone())).or(job.rig_type.clone());
    if provider_name == "tripo" {
        for n in super::tripo::check_actions(rig_type.as_deref(), &new)? {
            eprintln!("  note: {n}");
        }
    }
    let gen_task = sc.result["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["step"].as_str().is_some_and(|s| s.ends_with("-to-model") || s == "mock"))
        .and_then(|t| t["id"].as_str().map(str::to_string))
        .filter(|id| here(id));
    let price = provider::find_model(&provider_name, &model_id).and_then(|m| match m.price {
        provider::Price::Model3d { rig, animation, .. } => Some((rig, animation)),
        _ => None,
    });
    let est = price.map(|(r, an)| if need_rig { r } else { 0.0 } + an * new.len() as f64);
    eprintln!(
        "rig {} ({}{}{}){}",
        job.name,
        if need_rig { format!("rig from {}", if gen_task.is_some() && provider_name == job.provider { "its generation task" } else { "an upload of its raw model" }) } else { format!("on its rig, task {}", rig.as_ref().map(|r| r.0.as_str()).unwrap_or("?")) },
        if new.is_empty() { String::new() } else { format!(", clips {}", new.join(", ")) },
        rig_type.as_deref().map(|t| format!(", {t}")).unwrap_or_default(),
        est.map(|e| format!(" · about ${e:.2}")).unwrap_or_default()
    );
    super::check_cost(est, 1, super::max_cost(a.max_cost), "this rig")?;
    if a.dry_run {
        eprintln!("  (dry run: nothing rigged)");
        return Ok(false);
    }
    let prov = provider::pick(&provider_name, &model_id, est)?;
    if let Some(e) = est {
        provider::check_balance(prov.as_ref(), e)?;
    }
    let req = provider::RigRequest {
        task: gen_task.clone().filter(|_| provider_name == job.provider),
        glb: if gen_task.is_none() || provider_name != job.provider { raw.glb.clone() } else { Vec::new() },
        rig_task: if need_rig { None } else { rig.as_ref().map(|r| r.0.clone()) },
        rig_type: rig_type.clone(),
        animations: new.clone(),
        prompt: job.subject.clone(),
        seed: Some(job.seed),
    };
    let t0 = std::time::Instant::now();
    let out = prov.rig(&req)?;
    eprintln!("  done in {:.0} s{}", t0.elapsed().as_secs_f32(), out.cost_usd.map(|c| format!(", ${c:.2}")).unwrap_or_default());
    for n in &out.notes {
        eprintln!("  note: {n}");
    }
    let mut tasks = out.tasks.clone();
    if a.rig_task.is_some() && !need_rig {
        tasks.insert(0, (format!("rig ({})", rig_type.clone().unwrap_or_else(|| "biped".into())), a.rig_task.clone().unwrap_or_default()));
    }
    if !out.glb.is_empty() {
        // Keep the unrigged download beside the rigged one (execute writes the rigged one in its place).
        let canonical = a.game.join(format!("art/raw/{}-{}.glb", job.name, sc.seed));
        if !rigged && canonical.is_file() {
            let keep = a.game.join(format!("art/raw/{}-{}.unrigged.glb", job.name, sc.seed));
            std::fs::copy(&canonical, &keep).map_err(|e| format!("{}: {e}", keep.display()))?;
            eprintln!("  kept the unrigged model as {}", super::rel(&keep, &a.game));
        }
        raw.glb = out.glb.clone();
        // Clips retargeted onto an older rig would move bones by the old names: the new rig's clips only.
        if rigged && !raw.clips.is_empty() {
            eprintln!("  note: rigged again, so the {} clip(s) made on the old rig are kept and merged by bone name", raw.clips.len());
        }
    }
    raw.clips.extend(out.clips.iter().cloned());
    let (_, ty) = out.rig.clone().or(rig).unwrap_or_default();
    job.rig = true;
    if !ty.is_empty() {
        job.rig_type = Some(ty);
    }
    job.animate.extend(out.clips.iter().map(|c| c.0.clone()));
    if provider_name != job.provider && job.provider == "import" {
        job.provider = provider_name;
    }
    add_tasks(sc, &tasks, &out.notes, out.cost_usd.unwrap_or(0.0), prov.via());
    Ok(true)
}

/// Adds a rig's tasks, notes and cost to a sidecar about to be reprocessed.
fn add_tasks(sc: &mut Sidecar, tasks: &[(String, String)], notes: &[String], cost: f64, via: Option<&str>) {
    let mut all = sc.result["tasks"].as_array().cloned().unwrap_or_default();
    all.extend(tasks.iter().map(|(s, id)| task_json(s, id, via)));
    sc.result["tasks"] = json!(all);
    let mut n = sc.result["notes"].as_array().cloned().unwrap_or_default();
    n.extend(notes.iter().map(|s| json!(s)));
    sc.result["notes"] = json!(n);
    sc.cost_usd = Some(sc.cost_usd.unwrap_or(0.0) + cost);
}

/// A provider task for the sidecar's `result.tasks`, with `"via": "maimbrain"`
/// when it ran on Maimbrain's keys (so on Maimbrain's Tripo account).
fn task_json(step: &str, id: &str, via: Option<&str>) -> Value {
    let mut t = json!({ "step": step, "id": id });
    if let Some(v) = via {
        t["via"] = json!(v);
    }
    t
}

/// Whose keys ran the task `id` in a sidecar's `result.tasks` ("maimbrain", or None: yours).
fn task_via<'a>(result: &'a Value, id: &str) -> Option<&'a str> {
    result["tasks"].as_array()?.iter().find(|t| t["id"] == id)?["via"].as_str()
}

/// The recipe's `--clip` edits with `new` ones: an edit replaces the one
/// making the same clip, `none` clears them all.
pub fn merge_clip_edits(old: &[String], new: &[String]) -> Vec<String> {
    let mut out = old.to_vec();
    for n in new.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if n.eq_ignore_ascii_case("none") {
            out.clear();
            continue;
        }
        let target = n.split('=').next().unwrap_or(n);
        out.retain(|o| !shape::same_clip(o.split('=').next().unwrap_or(o), target));
        out.push(n.to_string());
    }
    out
}

/// `mb model <game> <name> --import file.glb`: the processing alone, on a .glb you already have.
fn import(game: &Path, mut job: ModelJob, src: &Path) -> Result<(), String> {
    let glb = std::fs::read(src).map_err(|e| format!("--import {}: {e}", src.display()))?;
    // Carry an mb sidecar's subject and rig type over, so --spine auto knows a heron from a dragon.
    let side = PathBuf::from(format!("{}.gen.json", src.display()));
    if let Ok(old) = super::read_sidecar(&side) {
        job.subject = old.prompt.clone();
        if job.rig_type.is_none() {
            job.rig_type = old.recipe["rig_type"].as_str().map(str::to_string);
        }
    }
    let sha = super::sha256_hex(&glb);
    let sc = Sidecar {
        generator: String::new(),
        date: String::new(),
        provider: "import".into(),
        model: "file".into(),
        via: None,
        prompt: job.subject.clone(),
        full_prompt: String::new(),
        seed: job.seed,
        seed_honored: true,
        references: vec![FileRef { path: src.display().to_string(), sha256: sha.clone() }],
        style: None,
        est_cost_usd: Some(0.0),
        cost_usd: Some(0.0),
        raw: String::new(),
        raw_sha256: sha,
        output_sha256: String::new(),
        allowed_names: Vec::new(),
        model_text: None,
        recipe: Value::Null,
        result: json!({ "notes": [format!("imported from {}", src.display())] }),
        note: String::new(),
        derived: Vec::new(),
    };
    execute(game, &job, 1, false, f64::INFINITY, Some((Raw { glb, clips: Vec::new() }, sc))).map(|_| ())
}

fn read_raw(game: &Path, sc: &Sidecar) -> Result<Raw, String> {
    let glb = std::fs::read(game.join(&sc.raw)).map_err(|e| format!("{}: {e} (raw outputs live in art/raw/; regenerate without --reprocess)", sc.raw))?;
    let mut clips = Vec::new();
    for c in sc.result["raw_clips"].as_array().into_iter().flatten() {
        let p = c["path"].as_str().unwrap_or("");
        clips.push((c["action"].as_str().unwrap_or("clip").to_string(), std::fs::read(game.join(p)).map_err(|e| format!("{p}: {e}"))?));
    }
    Ok(Raw { glb, clips })
}

/// The prompt sent to the provider: the subject, the game's style, and what makes a good game model.
pub fn compose(job: &ModelJob, style: &Style) -> (String, Option<String>) {
    let mut p = vec![format!("{}.", job.subject.trim().trim_end_matches('.'))];
    let object = job.object != ObjectKind::Character;
    if !job.no_style {
        // A part, prop or vehicle gets [3d_object]'s words and the palette only: the
        // game's style sentence and keywords describe its characters ("anime mecha",
        // "visible mechanical joints") and turn a gun or a plate into a little robot.
        let k = style.kind(if object { "3d_object" } else { "3d" }).prompt;
        if !k.trim().is_empty() {
            p.push(format!("{}.", k.trim().trim_end_matches('.')));
        }
        if !object && !style.style.trim().is_empty() {
            p.push(format!("Style: {}.", style.style.trim().trim_end_matches('.')));
        }
        if !object && !style.keywords.is_empty() {
            p.push(format!("{}.", style.keywords.join(", ")));
        }
        if !style.palette.is_empty() {
            p.push(format!("Colors from this palette: {}.", style.palette.join(", ")));
        }
    }
    if object {
        let what = match job.object {
            ObjectKind::Part => "a separate part that attaches to something else",
            ObjectKind::Vehicle => "a vehicle",
            _ => "a prop",
        };
        p.push(format!("One single solid hard-surface object shown on its own, {what}: clean manufactured shapes with flat, beveled and rounded surfaces, all in one piece; no base, ground or background."));
        // Generators pose parts as they like (a blade along a diagonal, a plate leaning
        // back); asking for a plain rest pose makes --orient's job easy.
        if job.object == ObjectKind::Part {
            p.push(super::orient::rest_pose_words(job.orient.as_deref()).into());
        }
    } else {
        p.push("A single game-ready 3D model with clean, readable shapes that read well small on a phone; the whole object, no base, ground or background.".into());
    }
    // A rig fits the pose the model was sculpted in, and text-to-3D models sculpt "dynamic"
    // poses (a koi curled mid-leap) unless told not to; a curled body barely swims.
    if job.rig || !job.animate.is_empty() {
        if job.animate.iter().any(|a| SWIMMERS.iter().any(|s| a.contains(s))) {
            p.push("Neutral rest pose for rigging: body perfectly straight and horizontal, tail straight behind, fins or wings relaxed and spread; not curled, not leaping, not banking.".into());
        } else {
            p.push("Neutral rest pose for rigging: standing straight, limbs apart and relaxed (an A-pose for a person), looking straight ahead.".into());
        }
    }
    p.push("An original design.".into());
    let mut prompt = p.join(" ");
    // Meshy takes 800 characters, Tripo 1024.
    if prompt.chars().count() > 800 {
        prompt = prompt.chars().take(800).collect();
    }
    let mut neg: Vec<String> = Vec::new();
    if !job.no_style {
        neg.extend(style.avoid.iter().cloned());
    }
    // What an object turns into when it's read as a character. Positive words like
    // "no arms" ask for arms; the negative prompt is where they belong.
    if object {
        neg.push("character, robot, figure, face, eyes, head, arms, legs, hands, mechanical joints".into());
    }
    neg.extend(job.negative.iter().cloned());
    (prompt, (!neg.is_empty()).then(|| neg.join(", ")))
}

/// The raw provider output: the model and one .glb per clip, as saved in art/raw/.
pub struct Raw {
    pub glb: Vec<u8>,
    pub clips: Vec<(String, Vec<u8>)>,
}

/// Turns a provider's raw output into the game's .glb. Returns the bytes, the facts and warnings.
pub fn process(job: &ModelJob, raw: &Raw) -> Result<(Vec<u8>, Value, Vec<String>), String> {
    let mut sc = glb::read(&raw.glb).map_err(|e| format!("{}: {e}", job.provider))?;
    let mut warnings = Vec::new();
    for (action, bytes) in &raw.clips {
        let other = glb::read(bytes).map_err(|e| format!("the {action} clip: {e}"))?;
        let name = shape::clip_name(action.rsplit(':').next().unwrap_or(action));
        match shape::merge_clips(&mut sc, &other, &name) {
            Ok(0) => {}
            Ok(lost) => warnings.push(format!("the {name} clip moves {lost} channel(s) on bones the model doesn't have; dropped them")),
            // The clip's file carries its own rigged copy of the model (providers export clips
            // "with skin"): when its bones don't match, use that copy as the model.
            Err(_) if sc.anims.is_empty() && !other.skins.is_empty() && other.triangles() > 0 => {
                let mut o = other.clone();
                for (k, a) in o.anims.iter_mut().enumerate() {
                    a.name = if k == 0 { name.clone() } else { format!("{name}{}", k + 1) };
                }
                o.dropped.extend(sc.dropped.iter().cloned());
                sc = o;
                warnings.push(format!("the {name} clip's bones didn't match the rigged model's; used the clip's own copy of the model"));
            }
            Err(e) => warnings.push(e),
        }
    }
    // --clip: trims, renames, cuts and drops, on the merged clips.
    let edits = job.clips.iter().map(|c| shape::ClipEdit::parse(c)).collect::<Result<Vec<_>, _>>()?;
    warnings.extend(shape::edit_clips(&mut sc, &edits)?);
    let dropped: Vec<String> = sc.dropped.iter().cloned().collect();
    let morphs = shape::limit_morphs(&mut sc);
    if morphs > 0 {
        warnings.push(format!("a mesh had {morphs} morph targets; kept the first 4 (mb3d blends 4)"));
    }
    let normals = shape::ensure_normals(&mut sc);
    let (before, after, err) = shape::fit_triangles(&mut sc, job.polys as usize);
    if after > job.polys as usize {
        warnings.push(format!("simplified to {after} triangles, not {}: the shape wouldn't go lower without falling apart", job.polys));
    }
    // --rigid, --pin: hard-surface weights, hanging parts on one bone.
    let mut weight_facts = Value::Null;
    if (job.rigid.is_some() || !job.pin.is_empty()) && job.spine == super::spine::SpineMode::Off {
        let (f, notes) = super::weights::fix(&mut sc, job.rigid.as_deref(), &job.pin)?;
        weight_facts = f;
        warnings.extend(notes);
    }
    let (glowing, glow_share) = if !job.glow_colors.is_empty() {
        let colors = job.glow_colors.iter().map(|c| super::img::parse_hex(c)).collect::<Result<Vec<_>, _>>()?;
        let (n, share) = shape::add_glow_by(&mut sc, job.glow.unwrap_or(3.0), &shape::GlowBy::Colors(colors, job.glow_tolerance.unwrap_or(0.15)))?;
        if share < 0.001 {
            warnings.push(format!("--glow-color {} lit nothing: no texel is that color; look at the texture (art/raw/) and pick one from it, or raise --glow-tolerance", job.glow_colors.join(",")));
        } else if share > 0.4 {
            warnings.push(format!("--glow-color lit {:.0}% of the texture: lower --glow-tolerance to light only the spot", share * 100.0));
        }
        (n, Some(share))
    } else {
        match job.glow {
            Some(s) => (shape::add_glow(&mut sc, s, job.glow_above)?, None),
            None => (0, None),
        }
    };
    sc.prune();
    let metal_before = shape::metal_rough(&sc);
    if job.metal.is_some() || job.min_roughness.is_some() {
        shape::matte(&mut sc, job.metal, job.min_roughness)?;
    } else if let Some((m, r)) = metal_before.filter(|&(m, r)| m >= 0.3 || (m >= 0.15 && r < 0.3)) {
        warnings.push(format!(
            "it's polished metal (metalness {m:.2}, roughness {r:.2} on average): with nothing bright to reflect, that reads near-black outdoors; `--reprocess --metal 0.15 --min-roughness 0.6` mattes it"
        ));
    }
    let metal_after = shape::metal_rough(&sc);
    let mut front = to_plus_z(provider_front(&job.provider));
    // A rigged biped says where it faces: its toes point forward from its ankles, and its
    // L_/R_ joints put its right side to the right. That settles a mech the shape can't.
    let mut rig_facing = None;
    if job.orient.is_none() && job.spine == super::spine::SpineMode::Off
        && let Some((fwd, by, unsure)) = super::weights::rig_forward(&sc)
    {
        let (axis, to_z) = super::weights::axis_name(fwd);
        // Snap to the axis when it's within 15°; a rig leaning 3° shouldn't tilt the model.
        let snapped = (to_z / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2;
        let to_z = if (to_z - snapped).abs() < 15f32.to_radians() { snapped } else { to_z };
        let provider = provider_front(&job.provider);
        if unsure {
            warnings.push(format!("facing: its rig's toes and its left and right sides disagree about its front ({}); check the preview's arrow", by.join(" and ")));
        } else if job.turn == 0.0 {
            if axis != provider && job.provider != "import" {
                warnings.push(format!(
                    "its rig faces {axis} (its {} say so), not {provider} as {}'s models usually do: turned it to face {} by the rig; `--turn 360` keeps the old way",
                    by.join(" and "),
                    job.provider,
                    job.facing
                ));
            }
            front = to_z;
        } else {
            let face_turn = if job.facing == "+z" { 0.0 } else { std::f32::consts::PI };
            let ends = glb::rotate(glb::quat_y(front + face_turn + job.turn.to_radians()), fwd);
            let (now, _) = super::weights::axis_name(ends);
            if now != job.facing {
                warnings.push(format!("by its rig ({}) it now faces {now}, not {}: check --turn {}", by.join(" and "), job.facing, job.turn));
            }
        }
        rig_facing = Some(json!({ "rig_forward": axis, "by": by, "unsure": unsure, "turn_degrees": ((to_z.to_degrees() * 10.0).round() / 10.0) }));
    }
    let face = if job.facing == "+z" { 0.0 } else { std::f32::consts::PI };
    // Rig safety: a rig over mb3d's 64 joints loses leaf joints (merged into their parents)
    // instead of drawing unskinned, and a one-sided rig (one wing) is called out.
    // `--spine` replaces the rig, so the provider's joints don't matter (nor warn).
    let mut rig_facts = json!(null);
    if !sc.skins.is_empty() && job.spine == super::spine::SpineMode::Off {
        let before_j = sc.skins.iter().map(|s| s.joints.len()).max().unwrap_or(0);
        let merged = super::spine::merge_leaf_joints(&mut sc, 64);
        if merged > 0 {
            warnings.push(format!("the rig had {before_j} joints (mb3d takes 64): merged {merged} leaf joint(s) into their parents"));
        }
        let mirror = super::spine::mirror_check(&sc);
        if job.spine == super::spine::SpineMode::Off {
            for m in &mirror {
                warnings.push(format!("{m}; `mb model {} {} --reprocess --spine auto` gives it bones mb lays out itself", "<game>", job.name));
            }
        }
        rig_facts = json!({ "joints_from_provider": before_j, "merged_leaves": merged, "symmetric": mirror.is_empty(), "tree": super::spine::joint_tree(&sc) });
    }
    if job.rig && job.spine == super::spine::SpineMode::Off && sc.anims.is_empty() && !job.animate.is_empty() {
        warnings.push("asked for clips but none came back usable: `--reprocess --spine auto` gives it code-driven bones (Skeleton::spine + Swim/Flight)".into());
    }
    let unplaced = sc.clone();
    // --orient: turn the part so its tip or face lands on the stated axis, from its shape.
    let mut orient_facts = json!(null);
    let pre = match job.orient.as_deref() {
        Some(spec) => {
            let o = super::orient::Orient::parse(spec)?;
            let found = super::orient::analyze(&sc).ok_or("the model has no triangles")?;
            let q = super::orient::rotation(&found, &o);
            if found.tip_ratio < 1.3 && o.first.0 == super::orient::Feature::Tip {
                warnings.push(format!("--orient {spec}: its two ends are alike ({:.2}×), so the tip may be the wrong end; check the preview and flip the axis's sign in --orient (`--reprocess --orient tip=-y`) if so", found.tip_ratio));
            }
            if found.face_by == "unsure" && o.first.0 == super::orient::Feature::Face {
                warnings.push(format!("--orient {spec}: its two sides look alike (no glow on one, neither bulges), so its face may be the back; check the preview and flip the axis's sign in --orient if so"));
            }
            orient_facts = json!({ "spec": spec, "rotation": q, "found": found.facts() });
            q
        }
        None => [0.0, 0.0, 0.0, 1.0],
    };
    let mut placement = if job.orient.is_some() {
        Placement { size: job.size, origin: job.origin, turn: job.turn.to_radians(), lengthwise: false, pre }
    } else {
        Placement { size: job.size, origin: job.origin, turn: front + face + job.turn.to_radians(), lengthwise: job.lengthwise, pre }
    };
    let mut placed = shape::place(&mut sc, &job.name, &placement)?;
    // Facing check for long creatures (F10): the head is the thicker end. Clearly backwards
    // → turned round (and said so); a close call → "facing uncertain", look at the preview.
    let mut facing_check = json!(null);
    if job.orient.is_some() {
        facing_check = json!({ "verdict": "oriented" });
    } else if let Some(r) = rig_facing.filter(|r| r["unsure"] == false) {
        facing_check = r;
        facing_check["verdict"] = json!("rig");
    } else if let Some((head_neg, ratio)) = super::spine::head_end(&sc) {
        let want_neg = job.facing != "+z";
        let verdict = if ratio < 1.3 {
            warnings.push(format!("facing uncertain: its two ends are about as thick ({ratio:.2}×); check which end the preview's arrow points at and pass --turn 180 if it's backwards"));
            "uncertain"
        } else if head_neg != want_neg && job.turn == 0.0 {
            placement.turn += std::f32::consts::PI;
            sc = unplaced;
            placed = shape::place(&mut sc, &job.name, &placement)?;
            warnings.push(format!("it came back facing backwards (its thicker end, {ratio:.1}× the other, was behind): turned it 180°; --turn 180 turns it back"));
            "turned"
        } else {
            "ok"
        };
        facing_check = json!({ "verdict": verdict, "thicker_end_ratio": (ratio * 100.0).round() / 100.0 });
    } else {
        // Squat or round (a mech, a crate, a lamp): no long axis to tell head from tail.
        warnings.push("facing uncertain: not a long shape, so mb can't tell its front; check the preview's arrow (the way it faces) and pass --turn 90, 180 or -90 if its front is elsewhere".into());
        facing_check = json!({ "verdict": "uncertain", "reason": "not a long shape" });
    }
    let spine_facts = if job.spine != super::spine::SpineMode::Off {
        let rest = shape::posed(&sc, &shape::pose(&sc, None));
        let (lo, hi) = shape::bounds(&rest).ok_or("the model has no triangles")?;
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let wings = super::spine::wants_wings(job.spine, job.rig_type.as_deref(), &job.subject, ext);
        let had = sc.anims.len();
        let f = super::spine::add_spine(&mut sc, &job.name, job.spine_joints.unwrap_or(8) as usize, wings, job.facing != "+z")?;
        if had > 0 {
            warnings.push(format!("--spine replaced the provider's rig and its {had} clip(s): pose it in code (Skeleton::spine)"));
        }
        f
    } else {
        super::shape::name_nodes(&mut sc, &job.name);
        json!(null)
    };
    // Fit the file under the .glb limit by halving the textures if it has to.
    let mut tex_max = job.texture_size;
    let mut maps_max = job.maps_size.unwrap_or((job.texture_size / 2).max(64));
    let mut float_bytes = 0;
    let (bytes, tex_facts) = loop {
        let mut s2 = sc.clone();
        let facts = shape::fit_textures(&mut s2, tex_max, maps_max, job.texture_format)?;
        let generator = concat!("mb ", env!("CARGO_PKG_VERSION"), " model");
        let b = if job.compress { glb::write_quantized(&s2, generator) } else { glb::write(&s2, generator) };
        if job.compress {
            float_bytes = glb::write(&s2, generator).len();
        }
        if b.len() <= mb_format::bundle::MAX_GLB || tex_max <= 128 {
            break (b, facts);
        }
        tex_max /= 2;
        maps_max /= 2;
    };
    if tex_max < job.texture_size {
        warnings.push(format!("textures cut to {tex_max} px to fit the 8 MB .glb limit"));
    }
    let (checked, mut w) = shape::check(&bytes)?;
    warnings.append(&mut w);
    // The nodes it draws with, by name: what `Node::find` takes on a spawn of it.
    let mesh_nodes: Vec<String> = glb::read(&bytes)?.nodes.iter().filter(|n| n.mesh.is_some() && !n.name.is_empty()).map(|n| n.name.clone()).collect();
    if !raw.clips.is_empty() || !sc.anims.is_empty() {
        let back = glb::read(&bytes)?;
        if back.skins.is_empty() && !back.anims.is_empty() {
            warnings.push("clips but no skin: they move whole nodes".into());
        }
    }
    let facts = json!({
        "triangles_from_provider": before,
        "triangles": checked["triangles"],
        "simplify_error": err,
        "vertices": checked["vertices"],
        "textures": tex_facts,
        "texture_sizes": checked["textures"],
        "metal_rough": metal_after.map(|(m, r)| json!({ "metalness": (m * 100.0).round() / 100.0, "roughness": (r * 100.0).round() / 100.0 })),
        "metal_rough_from_provider": metal_before.map(|(m, r)| json!({ "metalness": (m * 100.0).round() / 100.0, "roughness": (r * 100.0).round() / 100.0 })),
        "joints": checked["joints"],
        "clips": checked["clips"],
        "bytes": bytes.len(),
        "compressed": job.compress,
        "bytes_uncompressed": if job.compress { json!(float_bytes) } else { json!(null) },
        "placement": placed,
        "facing": job.facing,
        "dropped": dropped,
        "normals_added": normals,
        "glowing_materials": glowing,
        "glow_share": glow_share.map(|s| (s * 1000.0).round() / 1000.0),
        "source_generator": sc.generator,
        "rig": rig_facts,
        "weights": weight_facts,
        "spine": spine_facts,
        "facing_check": facing_check,
        "orient": orient_facts,
        "mesh_nodes": mesh_nodes,
    });
    Ok((bytes, facts, warnings))
}

fn read_image(game: &Path, r: &str) -> Result<(RefImage, FileRef), String> {
    let p = if Path::new(r).is_absolute() { PathBuf::from(r) } else { game.join(r) };
    let bytes = std::fs::read(&p).map_err(|e| format!("--ref {}: {e}", p.display()))?;
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else {
        return Err(format!("--ref {} isn't a PNG or JPEG", p.display()));
    };
    if bytes.len() > 20 << 20 {
        return Err(format!("--ref {} is over 20 MB", p.display()));
    }
    let sha256 = super::sha256_hex(&bytes);
    Ok((RefImage { bytes, mime: mime.into() }, FileRef { path: r.to_string(), sha256 }))
}

/// Generates (or with `raw`, reprocesses) a model job; returns the written path.
pub fn execute(game: &Path, job: &ModelJob, variants: u32, dry_run: bool, limit: f64, raw: Option<(Raw, Sidecar)>) -> Result<PathBuf, String> {
    if !game.is_dir() {
        return Err(format!("{} isn't a directory (pass the game directory first)", game.display()));
    }
    let (style, style_sha) = if job.no_style { (Style::default(), None) } else { Style::load(game)? };
    let style_ref = style_sha.map(|s| FileRef { path: "art/style.toml".into(), sha256: s });
    let verdict = super::ip::check(&[&job.subject, &style.style, &style.keywords.join(" "), &style.kind("3d").prompt, &style.kind("3d_object").prompt], &job.allow_names);
    if !verdict.ok() {
        return Err(super::ip::refusal_message(&verdict));
    }
    for w in &verdict.warnings {
        eprintln!("  warning: the prompt references a named style ({w}); make sure the result is your own design");
    }
    if let Some(w) = style.unset_warning().filter(|_| !job.no_style && raw.is_none()) {
        eprintln!("  warning: {w}");
    }
    let (full_prompt, negative) = compose(job, &style);
    let model = provider::find_model(&job.provider, &job.model);
    let rigged = job.rig || !job.animate.is_empty();
    eprintln!(
        "model {} → {} ({} m, ≤ {} triangles{}{})",
        job.name,
        job.out,
        job.size,
        job.polys,
        if rigged { ", rigged" } else { "" },
        if job.animate.is_empty() { String::new() } else { format!(", clips {}", job.animate.join(", ")) }
    );
    eprintln!("  {} {} · seed {}{}", job.provider, job.model, job.seed, if model.is_some_and(|m| !m.seed) { " (not honored)" } else { "" });
    let est = model.and_then(|m| provider::estimate_model(m, job.reference.is_some(), job.rig, job.animate.len()));
    let art = super::ensure_art_dir(game)?;
    let (outs, ref_files): (Vec<(u64, Raw, Option<f64>, Option<String>, Vec<String>, Vec<Value>, Option<String>)>, Vec<FileRef>) = match raw {
        Some((r, sc)) => {
            eprintln!("  reprocessing {} (no provider call)", sc.raw);
            // Keep what the generation cost and reported.
            let notes = sc.result["notes"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_string)).collect();
            let tasks = sc.result["tasks"].as_array().cloned().unwrap_or_default();
            (vec![(sc.seed, r, sc.cost_usd, sc.model_text.clone(), notes, tasks, sc.via.clone())], sc.references.clone())
        }
        None => {
            super::check_cost(est, variants, limit, "this model")?;
            let image = job.reference.as_deref().map(|r| read_image(game, r)).transpose()?;
            if dry_run {
                eprintln!("  prompt: {full_prompt}");
                if let Some(n) = &negative {
                    eprintln!("  avoid: {n}");
                }
                if let Some((_, fr)) = &image {
                    eprintln!("  from image: {}", fr.path);
                }
                eprintln!("  (dry run: nothing generated)");
                return Ok(game.join(&job.out));
            }
            let prov = provider::pick(&job.provider, &job.model, est.map(|e| e * variants as f64))?;
            if let Some(e) = est {
                provider::check_balance(prov.as_ref(), e * variants as f64)?;
            }
            let mut outs = Vec::new();
            for v in 0..variants {
                let seed = job.seed + v as u64;
                let req = ModelRequest {
                    model: job.model.clone(),
                    prompt: full_prompt.clone(),
                    negative: negative.clone(),
                    image: image.as_ref().map(|i| i.0.clone()),
                    polys: job.polys,
                    rig: job.rig,
                    rig_type: job.rig_type.clone(),
                    animations: job.animate.clone(),
                    seed: Some(seed),
                };
                eprintln!("  generating with {}{} (3D takes a few minutes)…", prov.name(), if variants > 1 { format!(" {}/{variants}", v + 1) } else { String::new() });
                let t0 = std::time::Instant::now();
                let out = prov.model(&req)?;
                eprintln!("  done in {:.0} s{}", t0.elapsed().as_secs_f32(), out.cost_usd.map(|c| format!(", ${c:.2}")).unwrap_or_default());
                if let Some(n) = prov.note() {
                    eprintln!("  {n}");
                }
                for n in &out.notes {
                    eprintln!("  note: {n}");
                }
                if rigged && !out.rigged {
                    eprintln!("  warning: the model came back without a rig");
                }
                let raw = Raw { glb: out.glb, clips: out.clips };
                let tasks = out.tasks.iter().map(|(s, id)| task_json(s, id, prov.via())).collect();
                outs.push((seed, raw, out.cost_usd, out.text, out.notes, tasks, prov.via().map(str::to_string)));
            }
            (outs, image.map(|i| vec![i.1]).unwrap_or_default())
        }
    };
    let mut written = Vec::new();
    let mut previews = Vec::new();
    for (v, (seed, raw, cost, text, notes, tasks, via)) in outs.into_iter().enumerate() {
        // Keep the raw output: `mb regen --reprocess` redoes the processing from it for free.
        let raw_rel = format!("art/raw/{}-{seed}.glb", job.name);
        std::fs::write(game.join(&raw_rel), &raw.glb).map_err(|e| format!("{raw_rel}: {e}"))?;
        let mut raw_clips = Vec::new();
        for (action, b) in &raw.clips {
            let safe: String = action.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
            let rel = format!("art/raw/{}-{seed}.{safe}.glb", job.name);
            std::fs::write(game.join(&rel), b).map_err(|e| format!("{rel}: {e}"))?;
            raw_clips.push(json!({ "action": action, "path": rel, "sha256": super::sha256_hex(b) }));
        }
        let (bytes, mut facts, warnings) = process(job, &raw)?;
        // --rigid/--pin: the clips before and after the weight change, side by side.
        if (job.rigid.is_some() || !job.pin.is_empty()) && !facts["weights"].is_null() && facts["clips"].as_array().is_some_and(|c| !c.is_empty()) && variants == 1 {
            let mut plain = job.clone();
            plain.rigid = None;
            plain.pin.clear();
            let (b0, _, _) = process(&plain, &raw)?;
            let sheet = raster::compare_clips(&glb::read(&b0)?, &glb::read(&bytes)?, job.facing == "+z", 160);
            let rel = format!("art/preview/{}.weights.png", job.name);
            std::fs::create_dir_all(art.join("preview")).map_err(|e| e.to_string())?;
            std::fs::write(game.join(&rel), sheet.encode_png()).map_err(|e| format!("{rel}: {e}"))?;
            facts["weights"]["compare"] = json!(rel);
            eprintln!("  weights: {} (each clip before, then after with the green bar)", game.join(&rel).display());
        }
        facts["raw_clips"] = json!(raw_clips);
        facts["rigged"] = json!(facts["joints"].as_u64().unwrap_or(0) > 0);
        facts["warnings"] = json!(warnings);
        facts["notes"] = json!(notes);
        facts["tasks"] = json!(tasks);
        // The rig clips are added to later (`--animate` on this model), from the tasks.
        facts["rig_task"] = rig_task_of(&facts).map(|(id, t)| json!({ "id": id, "rig_type": t })).unwrap_or(Value::Null);
        let dest = if variants == 1 { game.join(&job.out) } else { art.join("variants").join(format!("{}-{}.glb", job.name, v + 1)) };
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
        let mut j = job.clone();
        j.seed = seed;
        let sc = Sidecar {
            generator: format!("mb {} model", env!("CARGO_PKG_VERSION")),
            date: super::now_utc(),
            provider: job.provider.clone(),
            model: job.model.clone(),
            via,
            prompt: job.subject.clone(),
            full_prompt: full_prompt.clone(),
            seed,
            seed_honored: model.is_none_or(|m| m.seed),
            references: ref_files.clone(),
            style: style_ref.clone(),
            est_cost_usd: est,
            cost_usd: cost,
            raw: raw_rel,
            raw_sha256: super::sha256_hex(&raw.glb),
            output_sha256: super::sha256_hex(&bytes),
            allowed_names: verdict.allowed.clone(),
            model_text: text,
            recipe: serde_json::to_value(&j).unwrap(),
            result: facts.clone(),
            note: super::note_for(&job.provider),
            derived: Vec::new(),
        };
        super::write_sidecar(&dest, &sc)?;
        let scene = glb::read(&bytes)?;
        let sheet = raster::contact_sheet(&scene, if variants > 1 { 128 } else { 192 });
        let pv = if variants == 1 {
            art.join("preview").join(format!("{}.png", job.name))
        } else {
            art.join("variants").join(format!("{}-{}.preview.png", job.name, v + 1))
        };
        std::fs::create_dir_all(pv.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&pv, sheet.encode_png()).map_err(|e| e.to_string())?;
        report(&dest, &facts, &warnings);
        written.push(dest);
        previews.push(sheet);
    }
    if variants > 1 {
        let sheet = art.join("variants").join(format!("{}-sheet.png", job.name));
        std::fs::write(&sheet, super::img::contact_sheet(&previews).encode_png()).map_err(|e| e.to_string())?;
        eprintln!("  compare: {} (variants 1–{variants}, left to right, top to bottom)", sheet.display());
        eprintln!("  then: mb model {} {} --pick <n>", game.display(), job.name);
        println!("{}", sheet.display());
    } else {
        eprintln!(
            "  look at it: {} (top row: front, front-right, right, back, left, above; the orange arrow is the way it faces; then a row per clip)",
            art.join("preview").join(format!("{}.png", job.name)).display()
        );
        eprintln!("  in the game: Model::load(asset) then model.spawn(parent); clips play through gfx3d::Animator (model.clip(\"Swim\"))");
        println!("{}", written[0].display());
    }
    Ok(written.remove(0))
}

fn report(dest: &Path, facts: &Value, warnings: &[String]) {
    let clips: Vec<String> = facts["clips"].as_array().into_iter().flatten().map(|c| format!("{} {:.2} s", c["name"].as_str().unwrap_or("?"), c["seconds"].as_f64().unwrap_or(0.0))).collect();
    let size = &facts["placement"]["size_m"];
    let kb = facts["bytes"].as_u64().unwrap_or(0).div_ceil(1024);
    let floats = facts["bytes_uncompressed"].as_u64().map(|b| format!("; {} KB with float geometry", b.div_ceil(1024))).unwrap_or_default();
    eprintln!(
        "  wrote {} ({kb} KB{floats}): {} triangles (provider: {}), {} textures {}, {:.2} × {:.2} × {:.2} m{}{}",
        dest.display(),
        facts["triangles"],
        facts["triangles_from_provider"],
        facts["texture_sizes"].as_array().map_or(0, Vec::len),
        facts["texture_sizes"].as_array().map(|a| a.iter().map(|t| format!("{}×{}", t[0], t[1])).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        size[0].as_f64().unwrap_or(0.0),
        size[1].as_f64().unwrap_or(0.0),
        size[2].as_f64().unwrap_or(0.0),
        if facts["joints"].as_u64().unwrap_or(0) > 0 { format!(", {} joints", facts["joints"]) } else { String::new() },
        if clips.is_empty() { String::new() } else { format!(", clips: {}", clips.join(", ")) },
    );
    if let Some(n) = facts["mesh_nodes"].as_array().filter(|n| !n.is_empty()) {
        let names: Vec<&str> = n.iter().filter_map(Value::as_str).collect();
        eprintln!("  nodes: {} (spawn.find(\"{}\"); Model::node_names lists bones too)", names.join(", "), names[0]);
    }
    if let Some(s) = facts["glow_share"].as_f64() {
        eprintln!("  glow: {:.1}% of the texture lit by color", s * 100.0);
    }
    if let Some(d) = facts["dropped"].as_array().filter(|d| !d.is_empty()) {
        eprintln!("  dropped what mb3d doesn't read: {}", d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "));
    }
    for w in warnings {
        eprintln!("  warning: {w}");
    }
}

fn pick(game: &Path, name: &str, variant: u32) -> Result<(), String> {
    let src = game.join("art/variants").join(format!("{name}-{variant}.glb"));
    let sc = super::read_sidecar(&src)?;
    let out = sc.recipe["out"].as_str().ok_or("variant sidecar has no output path")?.to_string();
    let dest = game.join(&out);
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    super::write_sidecar(&dest, &sc)?;
    let pv = game.join("art/variants").join(format!("{name}-{variant}.preview.png"));
    if pv.exists() {
        let _ = std::fs::create_dir_all(game.join("art/preview"));
        let _ = std::fs::copy(&pv, game.join("art/preview").join(format!("{name}.png")));
    }
    eprintln!("picked variant {variant} (seed {}) → {}", sc.seed, dest.display());
    println!("{}", dest.display());
    Ok(())
}

pub fn regen(a: &super::RegenArgs, sc: &Sidecar) -> Result<(), String> {
    let mut job: ModelJob = serde_json::from_value(sc.recipe.clone()).map_err(|e| format!("sidecar recipe: {e}"))?;
    let asset_rel = super::rel(&a.asset, Path::new(""));
    let game = if asset_rel.ends_with(&job.out) {
        super::game_of(&a.asset, &job.out)
    } else if asset_rel.contains("art/variants/") {
        return Err(format!("{} is a candidate; `mb model <game> {} --pick N` it first", a.asset.display(), job.name));
    } else {
        // Moved since it was made (`--out art/pending/…`, then into assets/): the game is
        // the folder its raw output is under, and the new version goes where it is now.
        let game = a
            .asset
            .ancestors()
            .skip(1)
            .find(|d| !sc.raw.is_empty() && d.join(&sc.raw).is_file())
            .map(Path::to_path_buf)
            .ok_or(format!("{}: made as {}, and its raw output ({}) isn't under any folder above it", a.asset.display(), job.out, sc.raw))?;
        let game = if game.as_os_str().is_empty() { PathBuf::from(".") } else { game };
        let now = super::rel(&a.asset, &game);
        eprintln!("  writing {now} (made as {})", job.out);
        job.out = now;
        game
    };
    if let Some(s) = a.seed {
        job.seed = s;
    } else if a.new_seed {
        job.seed = super::fresh_seed();
    }
    if let Some(p) = &a.prompt {
        job.subject = p.clone();
    }
    // --provider maimbrain on a model from a provider its keys don't cover: tripo.
    let switch = a.provider.clone().or_else(|| {
        (super::maimbrain::mode() == super::maimbrain::Mode::Maimbrain && !super::maimbrain::SERVER_PROVIDERS.contains(&job.provider.as_str())).then(|| "tripo".to_string())
    });
    if let Some(p) = &switch {
        job.provider = p.clone();
        if a.model.is_none() {
            job.model = provider::default_model(p, Media::Model).map(|m| m.id.to_string()).unwrap_or_default();
        }
    }
    if let Some(m) = &a.model {
        job.model = m.clone();
    }
    let raw = if a.reprocess { Some((read_raw(&game, sc)?, sc.clone())) } else { None };
    execute(&game, &job, a.variants.max(1), a.dry_run, super::max_cost(a.max_cost), raw).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn job(provider: &str) -> ModelJob {
        ModelJob {
            kind: "model".into(),
            name: "koi".into(),
            subject: "a stylized koi".into(),
            out: "assets/koi.glb".into(),
            provider: provider.into(),
            model: "mock-3d".into(),
            seed: 1,
            reference: None,
            polys: 3000,
            rig: false,
            rig_type: None,
            animate: vec!["swim".into()],
            clips: vec![],
            rigid: None,
            pin: vec![],
            size: 0.6,
            origin: Origin::Center,
            facing: "-z".into(),
            turn: 0.0,
            lengthwise: false,
            texture_size: 256,
            maps_size: None,
            metal: None,
            min_roughness: None,
            texture_format: TexFormat::Auto,
            glow: Some(2.5),
            glow_above: 0.6,
            glow_colors: vec![],
            glow_tolerance: None,
            negative: None,
            no_style: false,
            allow_names: vec![],
            spine: Default::default(),
            spine_joints: None,
            object: ObjectKind::Character,
            orient: None,
            compress: true,
        }
    }

    #[test]
    fn tasks_record_whose_keys_ran_them() {
        let result = json!({ "tasks": [task_json("text-to-model", "task_gen", None), task_json("rig (biped)", "task_rig", Some("maimbrain"))] });
        assert_eq!(result["tasks"][0], json!({ "step": "text-to-model", "id": "task_gen" }), "yours: no via, as before");
        assert_eq!((task_via(&result, "task_gen"), task_via(&result, "task_rig")), (None, Some("maimbrain")));
        assert_eq!(rig_task_of(&result), Some(("task_rig".to_string(), "biped".to_string())));
    }

    #[test]
    fn a_clip_whose_bones_dont_match_brings_its_own_model() {
        let unrigged = super::super::mock3d::model(&ModelRequest { prompt: "a koi".into(), seed: Some(1), ..ModelRequest::default() });
        let rigged = super::super::mock3d::model(&ModelRequest { prompt: "a koi".into(), seed: Some(1), animations: vec!["swim".into()], ..ModelRequest::default() });
        let (_, facts, warnings) = process(&job("mock"), &Raw { glb: unrigged.glb, clips: rigged.clips }).unwrap();
        assert_eq!(facts["clips"][0]["name"], "Swim");
        assert_eq!(facts["joints"], 5);
        assert!(warnings[0].contains("own copy of the model"), "{warnings:?}");
    }

    #[test]
    fn a_rigged_biped_faces_where_its_rig_says() {
        // Tripo's mechs come back facing +x (toes and L_/R_ sides say so), not +z.
        let raw = glb::write(&super::super::weights::tests::biped(), "test");
        let mut j = job("tripo");
        j.animate.clear();
        j.rig = true;
        j.glow = None;
        j.origin = Origin::Ground;
        let (bytes, facts, warnings) = process(&j, &Raw { glb: raw.clone(), clips: vec![] }).unwrap();
        assert_eq!(facts["facing_check"]["verdict"], "rig", "{facts}");
        assert_eq!(facts["facing_check"]["rig_forward"], "+x");
        assert!(warnings.iter().any(|w| w.contains("its rig faces +x")), "{warnings:?}");
        assert!(!warnings.iter().any(|w| w.contains("not a long shape") || w.contains("symmetric")), "{warnings:?}");
        let (fwd, _, _) = super::super::weights::rig_forward(&glb::read(&bytes).unwrap()).unwrap();
        assert!(fwd[2] < -0.99, "faces −z now: {fwd:?}");
        // A --turn of its own: kept, and checked against the rig.
        j.turn = 90.0;
        let (_, _, warnings) = process(&j, &Raw { glb: raw, clips: vec![] }).unwrap();
        assert!(warnings.iter().any(|w| w.contains("by its rig") && w.contains("it now faces")), "{warnings:?}");
    }

    #[test]
    fn prompts_carry_the_style() {
        let style: Style = toml::from_str("style = \"paper lanterns at night\"\nkeywords = [\"soft glow\"]\npalette = [\"#101020\", \"#ff8040\"]\navoid = [\"photorealism\"]\n[3d]\nprompt = \"low-poly, faceted\"\n").unwrap();
        let (p, neg) = compose(&job("mock"), &style);
        for want in ["a stylized koi.", "low-poly, faceted.", "Style: paper lanterns at night.", "soft glow.", "#ff8040", "An original design."] {
            assert!(p.contains(want), "missing {want:?} in {p}");
        }
        assert!(!p.contains("standing pose"), "a swimmer isn't asked to stand");
        assert_eq!(neg.as_deref(), Some("photorealism"));
    }

    #[test]
    fn parts_get_object_words_not_the_character_style() {
        let style: Style = toml::from_str("style = \"real-robot anime cel art\"\nkeywords = [\"anime mecha\"]\npalette = [\"#101020\"]\navoid = [\"gore\"]\n[3d]\nprompt = \"visible mechanical joints\"\n[3d_object]\nprompt = \"crisp panel lines\"\n").unwrap();
        let mut j = job("mock");
        j.subject = "a shoulder cannon".into();
        j.animate.clear();
        j.object = ObjectKind::Part;
        let (p, neg) = compose(&j, &style);
        for gone in ["anime mecha", "visible mechanical joints", "real-robot"] {
            assert!(!p.contains(gone), "{gone:?} in {p}");
        }
        for want in ["crisp panel lines", "#101020", "hard-surface", "attaches to something else"] {
            assert!(p.contains(want), "missing {want:?} in {p}");
        }
        let neg = neg.unwrap();
        assert!(neg.contains("gore") && neg.contains("robot"), "{neg}");
        // A character keeps the style.
        j.object = ObjectKind::Character;
        let (p, _) = compose(&j, &style);
        assert!(p.contains("anime mecha") && p.contains("visible mechanical joints"), "{p}");
    }

    #[test]
    fn kinds_are_guessed_from_the_first_word_that_names_one() {
        assert_eq!(guess_kind("a single long beam saber for a giant robot", false), ObjectKind::Part);
        assert_eq!(guess_kind("a giant robot holding a saber", false), ObjectKind::Character);
        assert_eq!(guess_kind("twin shoulder cannons, gunmetal", false), ObjectKind::Part);
        assert_eq!(guess_kind("a hovering dropship", false), ObjectKind::Vehicle);
        assert_eq!(guess_kind("a wooden crate", false), ObjectKind::Prop);
        assert_eq!(guess_kind("a stylized koi", false), ObjectKind::Character);
        assert_eq!(guess_kind("something shiny", false), ObjectKind::Character);
        assert_eq!(guess_kind("an armor plate", true), ObjectKind::Character, "rigged means a character");
    }

    #[test]
    fn mock_fish_becomes_a_small_swimming_glowing_koi() {
        let j = job("mock");
        let out = super::super::mock3d::model(&ModelRequest { prompt: "a stylized koi".into(), seed: Some(1), animations: vec!["swim".into()], ..ModelRequest::default() });
        let (bytes, facts, warnings) = process(&j, &Raw { glb: out.glb, clips: out.clips }).unwrap();
        // The mock koi's ends are alike, so the facing check can only say it's unsure.
        assert!(warnings.iter().all(|w| w.starts_with("facing uncertain")), "{warnings:?}");
        assert_eq!(facts["facing_check"]["verdict"], "uncertain");
        assert!(facts["triangles"].as_u64().unwrap() <= 3000);
        assert!(facts["triangles_from_provider"].as_u64().unwrap() > 20_000);
        assert_eq!(facts["clips"][0]["name"], "Swim");
        assert_eq!(facts["texture_sizes"][0], json!([256, 256]));
        assert_eq!(facts["dropped"], json!(["KHR_materials_specular"]));
        let sc = glb::read(&bytes).unwrap();
        let (lo, hi) = shape::bounds(&shape::posed(&sc, &shape::pose(&sc, None))).unwrap();
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        assert!((ext[2] - 0.6).abs() < 1e-3, "0.6 m long along z: {ext:?}");
        assert!(((lo[1] + hi[1]) / 2.0).abs() < 1e-3, "centered for a swimmer");
        // Facing −z: the head (the Root joint, at the front) is at the −z end.
        let root = sc.nodes.iter().position(|n| n.name == "Root").unwrap();
        let w = sc.world(&sc.rest_locals());
        assert!(w[root][14] < -0.1, "head toward −z: {}", w[root][14]);
        assert!(sc.materials[0].emissive_tex.is_some());
    }
}
