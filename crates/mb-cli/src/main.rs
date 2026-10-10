//! `mb`: make, build, validate and publish Maimbrain games.
//!
//!     mb new frogger                  # a new game crate from the skill's template (--3d for mb3d)
//!     mb new --kit runner frogger     # …or from a genre starter kit (mb kits lists them)
//!     mb build frogger                # cargo build → wasm-opt → pack → validate
//!     mb login                        # sign in to maimbrain.com (device code)
//!     mb publish frogger              # build, upload a private draft, wait for validation
//!     mb submit frogger               # send the draft to review (after testing it on your phone)
//!     mb submit frogger --season      # …as a new season (or --fix, --update; its number suggests one)
//!     mb validate feed/dev.maimbrain.stack-0.1.0.mbx
//!     mb serve frogger                # preview in a browser; reload to rebuild
//!     mb serve --watch frogger        # …or rebuild on save and update the preview by itself
//!     mb shot frogger --autoplay --sheet   # screenshots at 0.5, 2, 5 and 10 s, and a sheet of them
//!     mb measure frogger --autoplay   # 10 s live: frame times, GPU ms, input latency, mb.verify()
//!     mb doctor                       # check the toolchain
//!     mb art sprite frogger hero "a frog in a tiny knight's helmet"   # generated art (mb art --help)
//!     mb music frogger music "bouncy marimba and claps" --bars 8      # a seamless loop
//!     mb sfx frogger croak "a short wet croak"                        # a sound effect
//!     mb regen frogger/assets/hero.png --seed 42                      # again, from its .gen.json
//!     mb fonts                        # the font library (--preview draws a specimen sheet)
//!     mb font add frogger bungee      # bake a library font (or a .ttf) into assets/fonts/

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};
use mb_format::bundle::{ASSET_EXTENSIONS, SOURCE_EXTENSIONS};
use mb_format::manifest::PerfTier;
use mb_format::{Manifest, Report};

mod budget;
mod cdp;
mod devbuild;
mod fonts;
mod generate;
mod glyphs;
mod live;
mod measure;
mod packnotes;
mod readability;
mod remote;
mod replay;
mod scaffold;
mod serve;
mod shot;
mod smooth;
mod watch;

/// `mb --version`: the version, the commit it was built from ("dev" when that
/// isn't known) and the embedded web runtime's id ("none" when it has none),
/// e.g. `mb 0.7.0 (build b11bbf6, runtime 3f1c2a9e)` (build.rs).
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (build ", env!("MB_BUILD_ID"), ", runtime ", env!("MB_RUNTIME_ID"), ")");

#[derive(Parser)]
#[command(name = "mb", version = VERSION, about = "Make, build, validate and publish Maimbrain games")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new game crate from the maimbrain-game skill's template, or from a genre starter
    /// kit with --kit (id com.maimbrain.<you>.<name>).
    /// Works signed out: without --id the id and creator are placeholders until you sign in.
    /// tools/ gets copies of the skill's sfx.py, pixel.py, check_audio.py and mix_check.py (and make_glb.py and lut.py in 3D).
    New {
        /// Directory to create; its name becomes the crate name and the id's last part
        /// (lowercase letters, digits, - and _: koi-gate makes crate koi_gate, id ….koi-gate).
        #[arg(required_unless_present = "kit")]
        dir: Option<PathBuf>,
        /// Start from the 3D template (mb3d scene with an mb2d HUD) instead of the 2D one.
        #[arg(long = "3d")]
        three_d: bool,
        /// Start from a genre starter kit: a complete, polished game to reskin and twist
        /// (runner, stacker, shooter, match3, racer, tower-defense, trivia). `--kit list` lists them.
        #[arg(long)]
        kit: Option<String>,
        /// Bundle id to use (reverse-DNS, e.g. dev.example.frogger) instead of your account's namespace.
        #[arg(long)]
        id: Option<String>,
        /// Creator handle to use (e.g. @ada) instead of your account's.
        #[arg(long)]
        creator: Option<String>,
        /// The kit identity to start from (default: one at random, so new games don't start alike).
        #[arg(long)]
        identity: Option<String>,
        /// Don't download and bake the identity's fonts.
        #[arg(long)]
        no_fonts: bool,
    },
    /// List the genre starter kits for `mb new --kit` (same as `mb new --kit list`).
    Kits,
    /// Sign in to the Maimbrain server (opens a browser to confirm a code).
    Login {
        /// Server URL (default https://maimbrain.com, or MB_SERVER).
        #[arg(long)]
        server: Option<String>,
    },
    /// Show who you're signed in as.
    Whoami,
    /// Build a game (or take a .mbx) and upload it as a private draft: play it in the
    /// Maimbrain app (Account → My games), then `mb submit` it for review. From a game
    /// directory, the entry for its version in NOTES.md (`## 1.2.0`) goes with it as
    /// what's new for players.
    Publish {
        /// Game directory or .mbx bundle.
        path: PathBuf,
        /// Where `mb build` writes the bundle.
        #[arg(long, default_value = "feed")]
        out: PathBuf,
        /// Send it straight to review instead of leaving a draft to test first: as --fix, --update
        /// or --season, or without one what its version number suggests.
        #[arg(long)]
        submit: bool,
        #[command(flatten)]
        level: LevelFlags,
        /// The AI model that built the game, by its name and version ("Claude Opus 5.5"), or
        /// `none` if a person wrote it. Players see it under About this game. An AI agent
        /// publishing gives its own. Asked for when missing and run in a terminal.
        #[arg(long, value_name = "NAME")]
        model: Option<String>,
    },
    /// Submit your draft of a game for review (moderators play it before it's in the feed).
    Submit {
        /// Game directory, .mbx bundle, game id (com.maimbrain.you.frogger) or just its name (frogger).
        game: String,
        #[command(flatten)]
        level: LevelFlags,
    },
    /// Set the description players see for your game: one line on its card in the feed, all of it
    /// under About this game and in shared links. It isn't part of the bundle, so it changes right
    /// away, with no new upload or review.
    Describe {
        /// Game directory, .mbx bundle, game id (com.maimbrain.you.frogger) or just its name (frogger).
        game: String,
        /// The description: up to 300 characters and 4 lines, no links. Without it (and without
        /// --file), a game directory's DESCRIPTION.txt.
        text: Option<String>,
        /// Read the description from this file.
        #[arg(long, conflicts_with = "text")]
        file: Option<PathBuf>,
        /// Remove the description.
        #[arg(long, conflicts_with_all = ["text", "file"])]
        clear: bool,
    },
    /// Check that the toolchain for building games is installed.
    Doctor,
    /// Preview a game in a desktop browser (rebuilds on reload when files change; with
    /// --watch, rebuilds on save and updates the open preview by itself).
    Serve {
        /// Game directory or .mbx bundle.
        path: PathBuf,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Where `mb build` writes the bundle (not used with --watch, which keeps builds in memory).
        #[arg(long, default_value = "feed")]
        out: PathBuf,
        /// Rebuild as soon as a file is saved (a fast build: no LTO or wasm-opt) and push it to
        /// the open preview: new code is swapped in, changed assets are refetched, compiler errors
        /// are shown over the game. Prints `[mb] build N ok|failed` and `[mb] page N running`
        /// lines; GET /__mb/wait blocks until what's on disk is built and running.
        #[arg(long)]
        watch: bool,
    },
    /// Screenshot a game at given moments from a headless Chrome, rendered like a phone (logical
    /// size, 2× pixels, touch), deterministically (a held session stepped at 60 Hz, or --hz 120),
    /// with scripted taps, swipes and tilt; --sheet adds one image of them all. Builds a game
    /// directory first.
    Shot(shot::ShotArgs),
    /// Measure a game running in real time in a headless Chrome, with scripted input as in
    /// `mb shot`: frame intervals, CPU and GPU ms, what frames drew, input latency by kind, and
    /// whether the run replays (exits non-zero if it doesn't). 60 Hz runs on the browser's
    /// animation frames (60 Hz in headless Chrome); another --hz steps frames on a timer.
    Measure(measure::MeasureArgs),
    /// Compile a game crate to wasm, optimize it, pack it into a .mbx and validate it.
    Build {
        /// Game directory (manifest.toml, Cargo.toml, src/, icon.png, assets/).
        dir: PathBuf,
        /// Where to write the bundle.
        #[arg(long, default_value = "feed")]
        out: PathBuf,
    },
    /// Pack a game directory and an already-built wasm into a .mbx.
    Pack {
        dir: PathBuf,
        #[arg(long)]
        wasm: PathBuf,
        #[arg(long, default_value = "feed")]
        out: PathBuf,
    },
    /// Print the host import table (SPEC §5) as JSON, for the runtime.
    Abi,
    /// Write feed.json (SPEC §8) and icons/ for the given bundles, in order.
    Feed {
        bundles: Vec<PathBuf>,
        /// Directory the feed is served from; bundle URLs are relative to it.
        #[arg(long, default_value = "feed")]
        out: PathBuf,
    },
    /// Apply loop metering to a wasm file (what the client does before running a game).
    Meter {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Unpack a bundle as a client serves it: its files under <out>, with game.wasm loop-metered.
    /// The server plays games on the web from this.
    Unpack {
        bundle: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Re-run a replay log on a bundle with no screen, as fast as it can go, and say what the run
    /// scored and its draw hash: the same runtime as the phone, in Deno (which it needs), with no
    /// GPU. Shows whether a run reproduces on a build: its own (checking a score) or a newer one
    /// (does an update change how it plays?).
    Replay {
        /// The game (.mbx) to run it on.
        bundle: PathBuf,
        /// The replay log (JSON), or a file with one under `log`.
        log: PathBuf,
        /// One line of JSON: frames, hash, scores (with the frame each came at), rounds, error, ms,
        /// the bundle's sha256 and the one the log was recorded on.
        #[arg(long)]
        json: bool,
        /// A built runtime (runtime/dist) instead of the one in this mb (default: $MB_RUNTIME_DIR, if set).
        #[arg(long)]
        runtime: Option<PathBuf>,
        /// Stop a run that takes longer than this many seconds.
        #[arg(long, default_value_t = 120)]
        timeout: u64,
    },
    /// Lay out runtime + game for a browser or headless run: <out>/runtime/, <out>/game/ (with boot.json).
    Stage {
        bundle: PathBuf,
        #[arg(long, default_value = "build/stage")]
        out: PathBuf,
        /// Built runtime (runtime/dist).
        #[arg(long, default_value = "runtime/dist")]
        runtime: PathBuf,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// UTC date for the daily seed (YYYY-MM-DD).
        #[arg(long, default_value = "2026-10-06")]
        date: String,
    },
    /// Generate images for a game (sprites, frames, backgrounds, parallax layers, tiles, UI, the icon)
    /// in the style of its art/style.toml, post-processed to the platform's rules.
    Art {
        #[command(subcommand)]
        cmd: generate::art::ArtCmd,
    },
    /// Generate a looping music track (or a one-shot) as a mono Ogg at the platform's loudness.
    Music(generate::sound::MusicArgs),
    /// Generate a sound effect (ElevenLabs; without a key, use the skill's procedural sfx.py).
    Sfx(generate::sound::SfxArgs),
    /// Voice acting: design a character's voice from a description (never a real person's), then
    /// speak lines as phone-ready Oggs with subtitle and lip-flap timings (.lips.txt) for ui::Dialogue.
    Voice {
        #[command(subcommand)]
        cmd: generate::voice::VoiceCmd,
    },
    /// Generate a 3D model (Tripo, Meshy): a textured, optionally rigged and animated .glb fitted
    /// to a triangle budget, scaled to meters, facing −z, checked for mb3d, with a preview.
    Model(Box<generate::model::ModelArgs>),
    /// Convert a WAV, MP3 or Ogg file to mono Ogg Vorbis, the format games play (or, to a .wav, decode
    /// it). Built in, so nothing else (oggenc, ffmpeg) is needed: `mb ogg jump.wav assets/jump.ogg`.
    Ogg {
        /// The file to read: .wav, .mp3 or .ogg.
        input: PathBuf,
        /// The file to write: .ogg encodes (mixing down to mono), .wav decodes (16-bit, as oggdec does).
        output: PathBuf,
        /// Vorbis quality, 0–10 as oggenc -q (lower is smaller).
        #[arg(short, long, default_value_t = 5.0)]
        quality: f32,
        /// Don't print the summary line (warnings still print).
        #[arg(short = 'Q', long)]
        quiet: bool,
    },
    /// Regenerate a generated asset from its sidecar: same recipe, optionally a new seed or prompt,
    /// or --reprocess to redo only the post-processing (free).
    Regen(generate::RegenArgs),
    /// Manage API keys for art and sound providers (stored outside any repository).
    Keys {
        #[command(subcommand)]
        cmd: generate::keys::KeysCmd,
    },
    /// List the art and sound models mb can use, with prices and which keys are set.
    Models,
    /// Check prompt text (stdin) against the originality list of mb art/music/sfx; prints JSON.
    /// The server runs this on prompts sent to Maimbrain's generation keys.
    #[command(hide = true)]
    IpCheck,
    /// Bake a font into a game: a library font (`mb fonts`) or a .ttf/.otf of your own becomes
    /// assets/fonts/<name>.mbf (a subset glyph atlas with metrics and kerning) plus its license,
    /// recorded in the game's fonts.toml so `mb build` re-bakes it when the entry changes.
    Font {
        #[command(subcommand)]
        cmd: FontCmd,
    },
    /// List the font library (OFL/Apache fonts from github.com/google/fonts) with tags, or draw a
    /// specimen sheet of them (--preview).
    Fonts {
        /// Only this category (e.g. pixel, display, condensed, slab, handwritten, mono).
        #[arg(long)]
        category: Option<String>,
        /// Only fonts whose id, family, description, mood, era or uses mention this.
        #[arg(long)]
        search: Option<String>,
        /// Print JSON.
        #[arg(long)]
        json: bool,
        /// Render a specimen sheet PNG (downloads the fonts it draws).
        #[arg(long)]
        preview: bool,
        /// Where --preview writes the sheet.
        #[arg(long, default_value = "fonts.png")]
        out: PathBuf,
    },
    /// Check a .mbx against the spec.
    Validate {
        bundle: PathBuf,
        /// Print the report as JSON (includes the full manifest).
        #[arg(long)]
        json: bool,
        /// Also write the bundle's icon.png here, if the bundle is valid (the server uses this).
        #[arg(long)]
        icon: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum FontCmd {
    /// Bake a font into <game>/assets/fonts/<name>.mbf (re-running replaces it).
    Add {
        /// Game directory.
        game: PathBuf,
        /// A library font id (`mb fonts`) or a path to a .ttf/.otf inside the game directory.
        #[arg(required_unless_present = "identity")]
        font: Option<String>,
        /// Instead of one font: all the fonts a kit identity (Theme::preset) was designed with.
        #[arg(long, conflicts_with = "font")]
        identity: Option<String>,
        /// Weight (e.g. 700); variable fonts take any value in their range.
        #[arg(long)]
        weight: Option<u32>,
        /// Characters to bake: latin1 (default: ASCII and accented letters), ascii, caps
        /// (lowercase drawn as capitals), digits, or literal characters; join with + (e.g.
        /// latin1+ŁŚ). Fewer = smaller.
        #[arg(long)]
        chars: Option<String>,
        /// Atlas pixels per em (SDF: 16–64, default 32; 48 keeps big titles' corners sharper).
        /// For --bitmap: the font's pixel grid.
        #[arg(long)]
        em: Option<u32>,
        /// File name under assets/fonts/ (default: the font id, plus -<weight> if not 400).
        #[arg(long = "as")]
        name: Option<String>,
        /// Bake 1-bit glyphs on a pixel grid (the default for library pixel fonts).
        #[arg(long)]
        bitmap: bool,
        /// Bake a distance field even for a pixel font (smooth scaling, host outlines).
        #[arg(long, conflicts_with = "bitmap")]
        sdf: bool,
        /// For a font file of your own: its license file (copied next to the atlas).
        #[arg(long)]
        license: Option<String>,
    },
}

/// What kind of release a submit is (docs/UPDATES.md "Version levels"). Without one, what the
/// version number suggests (`mb publish` prints it).
#[derive(clap::Args)]
#[group(multiple = false)]
struct LevelFlags {
    /// Submit it as a fix: bug fixes and polish. Players aren't told.
    #[arg(long)]
    fix: bool,
    /// Submit it as an update: something worth telling players about. Scores carry over.
    #[arg(long)]
    update: bool,
    /// Submit it as a new season: fresh leaderboards for every player (the old ones are archived).
    /// Needed when a board is removed or ranks the other way.
    #[arg(long)]
    season: bool,
}

impl LevelFlags {
    /// The `level` the server is sent: fix, update or season.
    fn level(&self) -> Option<&'static str> {
        [(self.fix, "fix"), (self.update, "update"), (self.season, "season")].into_iter().find_map(|(on, l)| on.then_some(l))
    }
}

fn main() -> ExitCode {
    // Every subcommand lives in run(), one big function: a debug build's frame
    // for it outgrows the 1 MB main thread Windows gives a process (macOS and
    // Linux give 8 MB), and mb overflowed its stack on start. A thread with a
    // stack of its own sizes it the same everywhere.
    match std::thread::Builder::new().name("mb".into()).stack_size(64 << 20).spawn(run) {
        Ok(t) => t.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(e) => {
            eprintln!("error: couldn't start mb: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> ExitCode {
    let result = match Cli::parse().cmd {
        Cmd::Kits => {
            print!("{}", scaffold::kit_list());
            Ok(())
        }
        Cmd::New { kit: Some(k), .. } if k == "list" => {
            print!("{}", scaffold::kit_list());
            Ok(())
        }
        Cmd::New { dir: None, .. } => Err("give the game a directory, e.g. mb new --kit runner games/hopper".into()),
        Cmd::New { dir: Some(dir), three_d, kit, id, creator, identity, no_fonts } => {
            // Signed out (no saved credentials) this makes no network request.
            let who = if id.is_some() && creator.is_some() { None } else { remote::whoami().ok() };
            let pair = who.as_ref().and_then(|w| Some((w["username"].as_str()?, w["namespace"].as_str()?)));
            let opts = scaffold::Options { three_d, kit: kit.as_deref(), id: id.as_deref(), creator: creator.as_deref(), who: pair, identity: identity.as_deref(), bake_fonts: !no_fonts };
            scaffold::new_game(&dir, &opts)
        }
        Cmd::Login { server } => remote::login(server.as_deref()),
        Cmd::Whoami => remote::whoami().map(|w| println!("@{} ({})", w["username"].as_str().unwrap_or("?"), remote::server(None))),
        Cmd::Publish { level, submit: false, .. } if level.level().is_some() => {
            let l = level.level().unwrap_or_default();
            Err(format!("--{l} needs --submit: it says what kind of release you're submitting"))
        }
        Cmd::Publish { path, out, submit, model, level } => {
            let hint = path.display().to_string();
            let level = level.level();
            // Before building, so a missing --model or a placeholder icon fails fast.
            built_with(model, &hint).and_then(|model| {
                if path.extension().is_some_and(|e| e == "mbx") {
                    bundle_icon(&path)
                        .map_or(Ok(()), |png| check_publish_icon(&png, "<game dir>"))
                        .and_then(|()| remote::publish(&path, submit, level, model.as_deref(), None, &hint))
                } else {
                    let notes = read_manifest(&path).and_then(|m| release_notes(&path, &m.version))?;
                    std::fs::read(path.join("icon.png"))
                        .map_or(Ok(()), |png| check_publish_icon(&png, &hint))
                        .and_then(|()| build_and_pack(&path, &out))
                        .and_then(|bundle| remote::publish(&bundle, submit, level, model.as_deref(), notes.as_deref(), &hint))
                }
            })
        }
        Cmd::Submit { game, level } => submit_target(&game).and_then(|t| remote::submit(&t, level.level())),
        Cmd::Describe { game, text, file, clear } => {
            description_text(&game, text, file.as_deref(), clear).and_then(|d| submit_target(&game).and_then(|t| remote::describe(&t.query, d.as_deref())))
        }
        Cmd::Doctor => doctor(),
        Cmd::Serve { path, port, watch: true, .. } => serve::watch(&path, port, |m| boot_json(m, 1, "2026-10-06")),
        Cmd::Serve { path, port, out, watch: false } => serve::serve(
            &path,
            port,
            |dir| build_and_pack(dir, &out),
            |bundle, site, runtime| stage(bundle, site, runtime, 1, "2026-10-06"),
        ),
        Cmd::Shot(a) => shot::run(a),
        Cmd::Measure(a) => measure::run(a),
        Cmd::Build { dir, out } => build_and_pack(&dir, &out).map(|_| ()),
        Cmd::Pack { dir, wasm, out } => pack(&dir, &wasm, &out).map(|_| ()),
        Cmd::Validate { bundle, json, icon } => validate(&bundle, json, icon.as_deref()),
        Cmd::Unpack { bundle, out } => unpack(&bundle, &out).map(|_| ()),
        Cmd::Stage { bundle, out, runtime, seed, date } => stage(&bundle, &out, &runtime, seed, &date),
        Cmd::Replay { bundle, log, json, runtime, timeout } => {
            let runtime = runtime.or_else(|| std::env::var_os("MB_RUNTIME_DIR").map(PathBuf::from));
            replay::replay(&bundle, &log, &replay::Options { json, runtime, timeout: std::time::Duration::from_secs(timeout) })
        }
        Cmd::Meter { input, output } => std::fs::read(&input)
            .map_err(|e| format!("{}: {e}", input.display()))
            .and_then(|w| mb_format::meter::instrument(&w))
            .and_then(|w| std::fs::write(&output, w).map_err(|e| e.to_string())),
        Cmd::Feed { bundles, out } => feed(&bundles, &out),
        Cmd::Art { cmd } => generate::art::run(cmd),
        Cmd::Music(a) => generate::sound::music(a),
        Cmd::Sfx(a) => generate::sound::sfx(a),
        Cmd::Voice { cmd } => generate::voice::run(cmd),
        Cmd::Model(a) => generate::model::run(*a),
        Cmd::Ogg { input, output, quality, quiet } => generate::audio::convert(&input, &output, quality, quiet),
        Cmd::Regen(a) => generate::regen(a),
        Cmd::Keys { cmd } => generate::keys::run(cmd),
        Cmd::Models => generate::models(),
        Cmd::IpCheck => generate::ip::check_stdin(),
        Cmd::Font { cmd: FontCmd::Add { game, identity: Some(id), .. } } => fonts::add_identity(&game, &id),
        Cmd::Font { cmd: FontCmd::Add { game, font, weight, chars, em, name, bitmap, sdf, license, .. } } => {
            fonts::add(&game, font.as_deref().unwrap_or_default(), weight, chars, em, name, bitmap, sdf, license)
        }
        Cmd::Fonts { category, search, json, preview: false, .. } => fonts::list(category.as_deref(), search.as_deref(), json),
        Cmd::Fonts { category, search, preview: true, out, .. } => fonts::preview(category.as_deref(), search.as_deref(), &out),
        Cmd::Abi => {
            println!("{}", abi_json());
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What `mb submit <game>` names: a game directory, a .mbx, or an id or name to look up.
fn submit_target(game: &str) -> Result<remote::SubmitTarget, String> {
    let path = Path::new(game);
    if path.is_dir() {
        let m = read_manifest(path)?;
        return Ok(remote::SubmitTarget { query: m.id, version: Some(m.version), sha256: None, source: game.into() });
    }
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|e| format!("{game}: {e}"))?;
        let report = mb_format::validate(&bytes);
        let m = report.manifest.ok_or_else(|| format!("{game}: not a valid bundle (mb validate {game})"))?;
        return Ok(remote::SubmitTarget { query: m.id, version: Some(m.version), sha256: Some(report.sha256), source: game.into() });
    }
    Ok(remote::SubmitTarget { query: game.into(), version: None, sha256: None, source: game.into() })
}

/// The AI model `mb publish` records as having built the game (None: a person
/// did): `--model`, else asked for in a terminal. An agent runs it without one,
/// so there it's an error that tells the agent to name itself.
fn built_with(flag: Option<String>, game: &str) -> Result<Option<String>, String> {
    let answer = match flag {
        Some(m) => m,
        None if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() => {
            eprint!("Which AI model built this game? Its name and version (like Claude Opus 5.5), or none if you wrote it yourself: ");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
            line
        }
        None => {
            return Err(format!(
                "say which AI model built this game: mb publish {game} --model \"<name and version>\"\n\
                 If you're an AI agent, that's you: give your own model's name and version as you know them \
                 (like \"Claude Opus 5.5\"), not a guess at another's. Players see it under About this game.\n\
                 If a person wrote the game without an AI model, use --model none."
            ));
        }
    };
    model_name(&answer)
}

/// What's new in `version` for players: its entry in the game's NOTES.md, the
/// text under a `## <version>` heading (`## v1.2.0` and `## 1.2.0 (Oct 9)` count
/// too) up to the next heading. None without the file or an entry.
fn release_notes(dir: &Path, version: &str) -> Result<Option<String>, String> {
    let path = dir.join("NOTES.md");
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(None) };
    let heading = |line: &str| line.strip_prefix("## ").map(|h| h.split_whitespace().next().unwrap_or("").trim_start_matches('v').to_string());
    let mut lines = text.lines().skip_while(|l| heading(l).as_deref() != Some(version));
    if lines.next().is_none() {
        return Ok(None);
    }
    let notes = lines.take_while(|l| !l.starts_with("## ") && !l.starts_with("# ")).collect::<Vec<_>>().join("\n");
    match notes.trim() {
        "" => Ok(None),
        n if n.chars().count() > 500 => Err(format!("{}: the entry for {version} is over 500 characters; players read it in a glance", path.display())),
        n => Ok(Some(n.to_string())),
    }
}

/// A `--model` answer as it's sent: trimmed, None for `none`.
fn model_name(answer: &str) -> Result<Option<String>, String> {
    match answer.trim() {
        "" => Err("give the AI model's name and version, or none".into()),
        m if m.eq_ignore_ascii_case("none") => Ok(None),
        m if m.chars().count() > 60 => Err("the model's name is over 60 characters; give just its name and version".into()),
        m if m.chars().any(char::is_control) => Err("the model's name must be one line".into()),
        m => Ok(Some(m.to_string())),
    }
}

/// What `mb describe` sets: the text, --file, a game directory's DESCRIPTION.txt, or None to clear it.
fn description_text(game: &str, text: Option<String>, file: Option<&Path>, clear: bool) -> Result<Option<String>, String> {
    if clear {
        return Ok(None);
    }
    let text = match (text, file) {
        (Some(t), _) => t,
        (None, Some(f)) => std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?,
        (None, None) if Path::new(game).is_dir() => {
            let f = Path::new(game).join("DESCRIPTION.txt");
            std::fs::read_to_string(&f).map_err(|_| format!("give the description, --file or --clear, or write it in {}", f.display()))?
        }
        (None, None) => return Err("give the description (or --file, or --clear)".into()),
    };
    match text.trim() {
        "" => Err("the description is empty; use --clear to remove it".into()),
        t => Ok(Some(t.to_string())),
    }
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join("manifest.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The game crate's package name, from its Cargo.toml.
fn package_name(dir: &Path) -> Result<String, String> {
    let path = dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    doc.get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{}: no [package] name", path.display()))
}

/// `mb build`: the release build and the bundle, under the game's build lock
/// (a parallel `mb shot` or `mb build` of the game waits, then reuses it).
fn build_and_pack(dir: &Path, out: &Path) -> Result<PathBuf, String> {
    let _lock = devbuild::lock(dir);
    build(dir).and_then(|wasm| pack(dir, &wasm, out))
}

fn build(dir: &Path) -> Result<PathBuf, String> {
    let manifest = read_manifest(dir)?;
    let name = package_name(dir)?;
    // Fonts first: they're assets the pack step picks up.
    fonts::rebake(dir)?;
    for w in sameness_warnings(dir) {
        eprintln!("warning: {w}");
    }
    if let Some(w) = glyphs::warning(dir, &rust_sources(&dir.join("src"))) {
        eprintln!("warning: {w}");
    }
    let max_memory = manifest.perf_tier.max_memory_pages() * 65536;
    eprintln!("building {name} ({:?}, max memory {} MiB)", manifest.perf_tier, max_memory >> 20);
    let status = Command::new("cargo")
        // The game's own Cargo.toml: works for a standalone crate and for a workspace member.
        .args(["build", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["-p", &name, "--target", "wasm32-unknown-unknown", "--release"])
        // SPEC §3: the declared memory maximum is set by perf tier.
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", format!("-C link-arg=--max-memory={max_memory}"))
        .status()
        .map_err(|e| format!("running cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build failed for {name}"));
    }
    let target = workspace_target_dir(dir)?;
    let raw = target.join("wasm32-unknown-unknown/release").join(format!("{}.wasm", name.replace('-', "_")));
    let opt = raw.with_extension("opt.wasm");
    let wasm_opt = Command::new("wasm-opt")
        .args(["-O3", "--strip-debug", "--strip-producers"])
        .args(mb_format_features())
        .arg(&raw)
        .arg("-o")
        .arg(&opt)
        .status();
    match wasm_opt {
        Ok(s) if s.success() => Ok(opt),
        Ok(_) => Err("wasm-opt failed".into()),
        Err(_) => {
            eprintln!("warning: wasm-opt not found; packing unoptimized wasm");
            Ok(raw)
        }
    }
}

/// Every .rs file under `root`, concatenated.
fn rust_sources(root: &Path) -> String {
    let mut src = String::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                src.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    src
}

/// A nudge away from the house look (docs/IDENTITY.md): only the built-in
/// fonts, or a stock kit theme used as-is.
fn sameness_warnings(dir: &Path) -> Vec<String> {
    let src = rust_sources(&dir.join("src"));
    let mut out = Vec::new();
    if !fonts::game_has_fonts(dir) && !src.contains("Font::asset(") {
        out.push("this game draws only with the built-in fonts (Inter and the 5×7 pixel font), like most Maimbrain games: pick a library font that fits its identity (`mb fonts`, `mb font add`; docs/IDENTITY.md)".into());
    }
    let stock = ["Theme::candy()", "Theme::night()", "Theme::arcade()", "Theme::paper()", "Theme::jungle()", "Theme::default()"];
    let identity = src.contains("Theme::preset(") || src.contains("Identity::") || src.contains("Theme::from_identity(") || src.contains("Theme {");
    if let Some(t) = stock.iter().find(|t| src.contains(**t))
        && !identity
    {
        out.push(format!("this game uses the stock {t} as-is, the house look: compose an identity instead (Theme::from_identity, the identity presets in docs/UI.md)"));
    }
    match std::fs::read(dir.join("icon.png")).ok().as_deref().and_then(scaffold::stock_icon) {
        Some(scaffold::StockIcon::Template(t)) => {
            out.push(format!("icon.png is still the {t} template's placeholder, and `mb publish` refuses it: {}", make_icon_hint(dir)))
        }
        Some(scaffold::StockIcon::Kit(k)) => out.push(format!("icon.png is still the {k} kit's icon: {}", make_icon_hint(dir))),
        None => {}
    }
    out
}

/// How to replace a stock icon.png.
fn make_icon_hint(dir: &Path) -> String {
    format!(
        "make the game's own (256×256, a close-up of its hero or hook, not text) with `mb art icon {} \"…\"`, or draw it in code with the skill's scripts/pixel.py",
        dir.display()
    )
}

/// `mb publish` won't upload a game still wearing a template's placeholder
/// icon: in the feed and the studio it would look like every other new game.
/// (A kit's icon is real art; `mb build` warns about it instead.)
fn check_publish_icon(png: &[u8], dir: &str) -> Result<(), String> {
    match scaffold::stock_icon(png) {
        Some(scaffold::StockIcon::Template(t)) => Err(format!("icon.png is still the {t} template's placeholder: {}; then publish again", make_icon_hint(Path::new(dir)))),
        _ => Ok(()),
    }
}

/// The icon.png inside a bundle (None if it isn't a valid bundle; the server says why).
fn bundle_icon(bundle: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(bundle).ok()?;
    mb_format::Bundle::open(&bytes).ok()?.file("icon.png").map(<[u8]>::to_vec)
}

/// wasm-opt flags matching the features mb-format allows (Wasm 2.0).
fn mb_format_features() -> [&'static str; 7] {
    [
        "--enable-bulk-memory",
        "--enable-reference-types",
        "--enable-sign-ext",
        "--enable-nontrapping-float-to-int",
        "--enable-multivalue",
        "--enable-simd",
        "--enable-mutable-globals",
    ]
}

fn workspace_target_dir(dir: &Path) -> Result<PathBuf, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("running cargo metadata: {e}"))?;
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    meta["target_directory"].as_str().map(PathBuf::from).ok_or_else(|| "cargo metadata: no target_directory".into())
}

fn pack(dir: &Path, wasm: &Path, out: &Path) -> Result<PathBuf, String> {
    let manifest = read_manifest(dir)?;
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let mut files = BTreeMap::new();
    files.insert("manifest.toml".to_string(), read(&dir.join("manifest.toml"))?);
    files.insert("icon.png".to_string(), read(&dir.join("icon.png"))?);
    files.insert("game.wasm".to_string(), read(wasm)?);
    // SPEC §1: src/ holds the crate the wasm was built from.
    files.insert("src/Cargo.toml".to_string(), read(&dir.join("Cargo.toml"))?);
    add_tree(&dir.join("src"), "src/src", SOURCE_EXTENSIONS, &mut files)?;
    if dir.join("assets").is_dir() {
        add_tree(&dir.join("assets"), "assets", ASSET_EXTENSIONS, &mut files)?;
    }
    let src = rust_sources(&dir.join("src"));
    // Every file is one of the bundle's 512 entries: provenance sidecars stay
    // out (the game doesn't read them) and lip timings ship packed, one file
    // per character (the source tree keeps them per asset).
    let folded = packnotes::fold(&mut files, &src);
    if folded.sidecars_left_out > 0 {
        eprintln!(
            "left out {} provenance sidecars ({:.1} KB): they're for mb regen and audits and stay in the game's folder (code that names provenance.json gets them packed)",
            folded.sidecars_left_out,
            folded.left_out_bytes as f64 / 1000.0
        );
    }
    if folded.sidecars > 0 {
        eprintln!("packed {} provenance sidecars into {} (the game reads it)", folded.sidecars, packnotes::PROVENANCE);
    }
    if folded.lips > 0 {
        eprintln!("packed {} lip timings into {} lips.txt file(s)", folded.lips, folded.lips_packs);
    }
    if folded.saved() > 0 {
        eprintln!("{} bundle entries saved", folded.saved());
    }
    // What the code and the voice scripts name, for the maybe-unused list.
    let scripts: Vec<String> = std::fs::read_dir(dir.join("art"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    let refs = packnotes::Refs::new(&src, &scripts);
    if let Some(w) = packnotes::entry_notes(&files, &folded, &refs) {
        eprintln!("warning: {w}");
    }
    // mb3d texture memory, known before the game runs (SPEC §5.4 limits).
    if manifest.stdlib.contains_key("mb3d") {
        let budget = dir.join("art/budget.toml");
        let groups = match std::fs::read_to_string(&budget) {
            Ok(t) => Some(packnotes::LoadGroups::parse(&t)?),
            Err(_) => None,
        };
        let frees = src.contains("Texture::free") || src.contains("Model::free") || src.contains(".free()");
        let (line, warnings) = packnotes::texture_notes(&packnotes::texture_uses(&files), groups.as_ref(), frees);
        eprintln!("{line}");
        for w in warnings {
            eprintln!("warning: {w}");
        }
    }
    let bytes = mb_format::pack(&files).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(format!("{}-{}.mbx", manifest.id, manifest.version));
    std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let report = mb_format::validate(&bytes);
    print_report(&path, &report);
    // Near or over a size limit: the biggest assets and those nothing names.
    let notes = packnotes::size_notes(&files, &refs, report.compressed_size, report.startup_size, report.wasm_size, &manifest.startup_assets);
    for w in notes {
        eprintln!("warning: {w}");
    }
    if report.ok() { Ok(path) } else { Err(format!("{} failed validation", path.display())) }
}

/// What `mb build` and the maimbrain-game skill's scripts need.
fn doctor() -> Result<(), String> {
    let run = |cmd: &str, args: &[&str]| Command::new(cmd).args(args).output().ok().filter(|o| o.status.success());
    let mut ok = true;
    let mut check = |name: &str, found: bool, required: bool, fix: &str| {
        let mark = if found { "✓" } else if required { "✗" } else { "!" };
        eprintln!("{mark} {name}{}", if found { String::new() } else { format!("  →  {fix}") });
        ok &= found || !required;
    };
    check("cargo / rustc", run("cargo", &["--version"]).is_some(), true, "install Rust: https://rustup.rs");
    let targets = run("rustup", &["target", "list", "--installed"]).map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    check("wasm32-unknown-unknown target", targets.contains("wasm32-unknown-unknown"), true, "rustup target add wasm32-unknown-unknown");
    // Install hints for this OS.
    let (binaryen, python) = if cfg!(target_os = "macos") {
        ("brew install binaryen", "brew install python")
    } else if cfg!(windows) {
        ("winget install WebAssembly.Binaryen  (or scoop install binaryen)", "winget install Python.Python.3.12")
    } else {
        ("sudo apt install binaryen  (or your distro's package)", "sudo apt install python3")
    };
    check("wasm-opt (smaller, faster games)", run("wasm-opt", &["--version"]).is_some(), false, &format!("{binaryen} (optional)"));
    // Windows usually has `python` or the `py` launcher rather than `python3`.
    let has_python = run("python3", &["--version"]).is_some() || run("python", &["--version"]).is_some() || run("py", &["-3", "--version"]).is_some();
    check("Python 3 (the skill's art and sound scripts)", has_python, false, python);
    check("Ogg Vorbis encoder (built in: mb ogg)", true, false, "");
    match remote::whoami() {
        Ok(w) => check(&format!("signed in as @{}", w["username"].as_str().unwrap_or("?")), true, false, ""),
        Err(_) => check("signed in", false, false, "mb login"),
    }
    // Optional: keys for `mb art`, `mb music` and `mb sfx`.
    for (name, what) in [("gemini", "art and music: mb art, mb music"), ("elevenlabs", "sound effects: mb sfx"), ("tripo", "3D models: mb model")] {
        let found = generate::keys::lookup(name).is_some();
        check(&format!("{name} key ({what})"), found, false, &format!("{} (optional)", generate::keys::help_for(name)));
    }
    // Or Maimbrain's keys, for accounts the Maimbrain team has given access.
    let (allowed, line) = generate::maimbrain::describe(&generate::maimbrain::Live);
    check(&line, allowed, false, "optional: without a key of your own, generation uses these");
    if ok { Ok(()) } else { Err("install the missing required tools above".into()) }
}

fn add_tree(root: &Path, prefix: &str, exts: &[&str], files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let key = format!("{prefix}/{name}");
        if path.is_dir() {
            add_tree(&path, &key, exts, files)?;
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e.to_ascii_lowercase().as_str())) {
            files.insert(key, std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?);
        } else {
            eprintln!("warning: skipping {} (type not allowed in {prefix}/)", path.display());
        }
    }
    Ok(())
}

fn validate(path: &Path, json: bool, icon: Option<&Path>) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let report = mb_format::validate(&bytes);
    if let (true, Some(out)) = (report.ok(), icon) {
        let bundle = mb_format::Bundle::open(&bytes).map_err(|r| r.errors.join("; "))?;
        let png = bundle.file("icon.png").ok_or("bundle has no icon.png")?;
        std::fs::write(out, png).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": report.ok(),
                "errors": report.errors,
                "warnings": report.warnings,
                "sha256": report.sha256,
                "size": report.compressed_size,
                "wasm_size": report.wasm_size,
                "startup_size": report.startup_size,
                "sdk_version": report.sdk_version,
                "id": report.manifest.as_ref().map(|m| m.id.clone()),
                "version": report.manifest.as_ref().map(|m| m.version.clone()),
                "manifest": report.manifest,
            })
        );
    } else {
        print_report(path, &report);
    }
    if report.ok() { Ok(()) } else { Err("validation failed".into()) }
}

/// Writes the bundle's files under `out` as a client serves them, and returns its manifest.
fn unpack(bundle: &Path, out: &Path) -> Result<Manifest, String> {
    let bytes = std::fs::read(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let b = mb_format::Bundle::open(&bytes).map_err(|r| format!("{} is invalid: {}", bundle.display(), r.errors.join("; ")))?;
    for path in b.paths() {
        let dest = out.join(path);
        std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
        // Serve the metered wasm, exactly as the iOS client does.
        let data = if path == "game.wasm" { b.runtime_wasm() } else { b.file(path).unwrap() };
        std::fs::write(&dest, data).map_err(|e| e.to_string())?;
    }
    Ok(b.manifest)
}

fn stage(bundle: &Path, out: &Path, runtime: &Path, seed: u64, date: &str) -> Result<(), String> {
    let game = out.join("game");
    let _ = std::fs::remove_dir_all(out);
    let manifest = unpack(bundle, &game)?;
    let boot = boot_json(&manifest, seed, date);
    std::fs::write(game.join("boot.json"), serde_json::to_string_pretty(&boot).unwrap()).map_err(|e| e.to_string())?;
    let rt = out.join("runtime");
    std::fs::create_dir_all(&rt).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(runtime).map_err(|e| format!("{}: {e} (run `just runtime`)", runtime.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        std::fs::copy(entry.path(), rt.join(entry.file_name())).map_err(|e| e.to_string())?;
    }
    eprintln!("staged {} at {} — serve it and open /runtime/index.html", manifest.id, out.display());
    Ok(())
}

/// game/boot.json for a browser or headless run (runtime/src/boot.ts).
fn boot_json(manifest: &Manifest, seed: u64, date: &str) -> serde_json::Value {
    serde_json::json!({
        "manifest": manifest,
        "seed": seed.to_string(),
        "dailySeed": mb_format::daily_seed(&manifest.id, date).to_string(),
        "store": {},
        "playerId": "00112233445566778899aabbccddeeff",
        "locale": "en-US",
        "insets": { "top": 0, "right": 0, "bottom": 0, "left": 0 },
        "stripHeight": 24,
    })
}

fn feed(bundles: &[PathBuf], out: &Path) -> Result<(), String> {
    let icons = out.join("icons");
    std::fs::create_dir_all(&icons).map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for path in bundles {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let b = mb_format::Bundle::open(&bytes).map_err(|r| format!("{} is invalid: {}", path.display(), r.errors.join("; ")))?;
        let m = &b.manifest;
        std::fs::write(icons.join(format!("{}.png", m.id)), b.file("icon.png").unwrap()).map_err(|e| e.to_string())?;
        let name = path.file_name().unwrap().to_string_lossy();
        items.push(serde_json::json!({
            "id": m.id,
            "version": m.version,
            "url": name,
            "sha256": b.sha256,
            "size": bytes.len(),
            "title": m.name,
            "creator": m.creator,
            "icon": format!("icons/{}.png", m.id),
            "orientation": m.orientation,
            "inputs": m.inputs,
            "needs": m.needs,
            "perf_tier": m.perf_tier,
            "likes": 0,
            "remix_of": if m.remix.parent.is_empty() { serde_json::Value::Null } else { m.remix.parent.clone().into() },
        }));
    }
    let doc = serde_json::json!({ "version": 0, "items": items });
    let dest = out.join("feed.json");
    std::fs::write(&dest, serde_json::to_string_pretty(&doc).unwrap()).map_err(|e| e.to_string())?;
    eprintln!("wrote {} with {} items", dest.display(), bundles.len());
    Ok(())
}

fn abi_json() -> String {
    use mb_format::abi::{IMPORTS, Requires, Ty};
    let ty = |t: &Ty| match t {
        Ty::I32 => "i32",
        Ty::I64 => "i64",
        Ty::F32 => "f32",
        Ty::F64 => "f64",
    };
    let imports: Vec<_> = IMPORTS
        .iter()
        .map(|i| {
            let requires = match i.requires {
                Requires::Always => serde_json::Value::Null,
                Requires::Stdlib(s, v) => serde_json::json!({ "stdlib": s, "version": v }),
                Requires::Sensor(s) => serde_json::json!({ "sensor": s.as_str() }),
                Requires::Capability(c) => serde_json::json!({ "capability": c.as_str() }),
            };
            serde_json::json!({
                "name": i.name,
                "params": i.params.iter().map(ty).collect::<Vec<_>>(),
                "results": i.results.iter().map(ty).collect::<Vec<_>>(),
                "requires": requires,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "abi": mb_format::manifest::ABI, "imports": imports })).unwrap()
}

fn print_report(path: &Path, r: &Report) {
    let status = if r.ok() { "ok" } else { "INVALID" };
    eprintln!("{status}: {}", path.display());
    if let Some(m) = &r.manifest {
        let tier = match m.perf_tier {
            PerfTier::Lite => "lite",
            PerfTier::Full => "full",
        };
        let sdk = r.sdk_version.as_ref().map(|v| format!(", SDK {v}")).unwrap_or_default();
        eprintln!("  {} {} by {} ({tier}{sdk})", m.id, m.version, m.creator);
    }
    eprintln!(
        "  {} bytes · wasm {} bytes · startup set {} bytes · sha256 {}",
        r.compressed_size, r.wasm_size, r.startup_size, r.sha256
    );
    for w in &r.warnings {
        eprintln!("  warning: {w}");
    }
    for e in &r.errors {
        eprintln!("  error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::release_notes;

    #[test]
    fn release_notes_are_the_entry_for_the_version() {
        let d = std::env::temp_dir().join(format!("mb-notes-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(release_notes(&d, "1.3.0"), Ok(None), "no NOTES.md");
        std::fs::write(
            d.join("NOTES.md"),
            "# Pour\n\n## v1.3.0 (Oct 9)\n\nTen new sewer levels.\nThe fish swim the right way.\n\n## 1.2.1\nFixed small phones.\n## 1.2.0\n\n",
        )
        .unwrap();
        assert_eq!(release_notes(&d, "1.3.0"), Ok(Some("Ten new sewer levels.\nThe fish swim the right way.".into())));
        assert_eq!(release_notes(&d, "1.2.1"), Ok(Some("Fixed small phones.".into())));
        assert_eq!(release_notes(&d, "1.2.0"), Ok(None), "an empty entry");
        assert_eq!(release_notes(&d, "1.4.0"), Ok(None), "no entry yet");
        std::fs::write(d.join("NOTES.md"), format!("## 2.0.0\n{}", "x".repeat(501))).unwrap();
        assert!(release_notes(&d, "2.0.0").unwrap_err().contains("over 500 characters"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
