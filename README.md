# Maimbrain creator kit

Make games for [Maimbrain](https://maimbrain.com): tiny games people swipe through on their phones. Games are Rust → WebAssembly; you describe one, and your agent builds it with the `maimbrain-game` skill.

## Get started

macOS or Linux:

```sh
curl -fsSL https://maimbrain.com/install.sh | sh   # the mb tool
mb doctor                                          # checks Rust, the wasm32 target and Python
mb login                                           # sign in to maimbrain.com
```

Windows (PowerShell):

```powershell
irm https://maimbrain.com/install.ps1 | iex        # installs mb.exe and adds it to your PATH
mb doctor
mb login
```

On Windows, install Rust with [rustup](https://rustup.rs) (it sets up the Visual Studio C++ build tools), then `rustup target add wasm32-unknown-unknown`. The skill's art and sound scripts need Python 3 (`winget install Python.Python.3.12`); use `python` or `py` where the skill says `python3`. Sound needs nothing else: `mb ogg` encodes Ogg Vorbis on every platform. WSL works too: follow the Linux steps inside it.

In Claude Code:

```
/plugin marketplace add terracatta/maimbrain-kit
/plugin install maimbrain@maimbrain-kit
```

Then ask: *"Use the maimbrain-game skill to make a game: …"*. Your agent will run:

```sh
mb new mygame        # a game crate from the template, with your id (--3d for a 3D one; works signed out too)
mb new --kit runner mygame   # …or start from a complete genre game to reskin and twist (mb kits lists them)
mb serve --watch mygame  # preview at http://127.0.0.1:8765; rebuilds and updates the page on every save
mb shot mygame --autoplay --sheet  # screenshots at 0.5, 2, 5 and 10 s from headless Chrome, and one sheet of them
mb measure mygame --autoplay  # 10 s live in headless Chrome: frame times, GPU ms, input latency, a verify
mb publish mygame --model "Claude Opus 5.5"  # upload a private draft, saying which model built it
                     # (it's the agent's own); play it in the Maimbrain app (Account → My games)
mb describe mygame   # tell players what it is (mygame/DESCRIPTION.txt, or the text as an argument)
mb submit mygame     # when it's right, send it for review; track it at maimbrain.com/create
```

`mb publish` uploads a draft only you can see; publish again to replace it while you test. Once a moderator approves what you submit, it's in everyone's feed. `mb publish --submit` skips the draft and goes straight to review. `--model` names the AI model that built the game (an agent gives its own; `none` if you wrote it yourself), and players see it under About this game with the SDK version the game was built with. The description isn't part of the bundle: players see its first line on the game's card and all of it under About this game, and `mb describe` (or My Game Studio in the app) changes it any time, with no review. For a new version of a published game, the number you bump says what it is: the last for a fix, the middle for an update, the first for a new season (the only kind that may change how scores rank). `mb publish` sends the version's entry from the game's `NOTES.md` (a `## 1.3.0` heading, then what's new for players).

### Fonts and identity

Every game should look like itself, not like every other Maimbrain game (docs/IDENTITY.md shows how alike they had become). `mb new` starts each game from one of the kit's 17 identities at random; the skill's art-direction step replaces it with the game's own. Fonts come from a library of 96 OFL/Apache families (pixel, display, condensed, slab, didone, handwritten, brush, script, blackletter, mono, techno, stencil, comic…), downloaded on demand from github.com/google/fonts and checked against pinned checksums:

```sh
mb fonts                         # the library, with mood, era, uses and pairings (--category pixel, --search western)
mb fonts --preview               # a specimen sheet, fonts.png
mb font add mygame rye           # bake a font into mygame/assets/fonts/rye.mbf (+ its license)
mb font add mygame --identity saloon   # every font a kit identity uses
mb font remove mygame --all      # take fonts out again (or name them), e.g. mb new's starting ones
```

Draw with `Font::asset("assets/fonts/rye.mbf", Font::SansBold)` or a theme's `.load_fonts()` (needs `stdlib = { mb2d = 2 }`); see docs/UI.md and docs/SPEC.md §5.3.

### Generated art and sound (optional)

With an API key, `mb` can generate a game's art and music in a consistent style and post-process it to the platform's rules:

```sh
mb keys set gemini                          # a Gemini API key with billing (images and music)
mb art init mygame                          # mygame/art/style.toml: style words, palette, references
mb art sprite mygame hero "a frog in a tiny knight's helmet"
mb art background mygame sky "a misty pond at dawn"
mb music mygame music "bouncy marimba and claps" --bars 8
mb sfx mygame croak "a short wet croak"     # sound effects need an ElevenLabs key (free plan available)
mb models                                   # models, prices, which keys are set
```

Each call shows its estimated cost (a few cents) and every asset gets a `.gen.json` provenance sidecar. `--provider mock` tries everything offline. See [docs/GENERATE.md](docs/GENERATE.md) for providers, keys, the style guide and the pipeline.

No key of your own? If the Maimbrain team has given your account access to **Maimbrain's keys**, `mb login` is all you need: without a key, the commands above go through the Maimbrain server, within a monthly budget (`mb models` shows what's left). Your own key always comes first; `--own-keys` never uses Maimbrain's. See "Maimbrain's keys" in docs/GENERATE.md.

## What's here

| Path | What |
|---|---|
| `skills/maimbrain-game/` | The skill: workflow (SKILL.md), engagement guide, art and sound guide, guides read when a game needs them (3D in `3d.md`, music and foley in `audio.md`, publishing and feedback in `publish.md`), 2D `template/` and 3D `template-3d/`, genre starter `kits/` (runner, stacker, shooter, match3, racer, tower-defense, trivia; `mb new --kit`), `pixel.py`, `sfx.py`, `check_audio.py`, `mix_check.py`, `make_glb.py`, `make_rigged_glb.py` (a rigged, animated character), `lut.py` (bake a color grade into a LUT) |
| `docs/SPEC.md` | The game spec (manifest, host API, limits, determinism) |
| `docs/MB3D.md` | The 3D engine: PBR, sky lighting, shadows, post effects, particles, trails, glTF; mb3d 2: animated characters, instancing, fog, toon and outlines, 3D text, depth of field, reflective water (water v2), sky v2, the beauty pass |
| `docs/GENERATE.md` | Generated art and sound: `mb art`, `mb music`, `mb sfx`, providers and keys, the style guide |
| `docs/UI.md` | The UI and juice kit: themes, buttons, icons, text, motion, confetti, title and results cards |
| `examples/gallery/` | Every kit widget and effect on nine pages: `mb serve examples/gallery` |
| `examples/mistwood/` | The mb3d 2 showcase: an animated toon fox in a foggy, instanced, moonlit forest (`mb serve examples/mistwood`) |
| `examples/mb3d-demo/` | mb3d scenes, including the moonlit canal: fog, reflective water that reacts to a leaping koi and flows, foams and glints (water v2), lanterns, sky v2, and the beauty pass (a moonlit grade, moonbeams, halos, grain; tap to compare) (`mb serve examples/mb3d-demo`, tap "CANAL"); and a koi pond of organic meshes swimming on bones under clear water (tap "KOI") |
| `docs/PHYSICS.md` | Deterministic 2D/3D rigid-body physics in the SDK (`features = ["physics2d"]` / `["physics3d"]`) |
| `sdk/maimbrain/` | The Rust SDK (games depend on it via git) |
| `crates/mb-cli/` | The `mb` tool |
| `crates/mb-format/` | The bundle format and validator (the same one the server runs) |

Build `mb` from source: `cargo install --git https://github.com/terracatta/maimbrain-kit mb-cli`.

This repository is generated from the Maimbrain monorepo; open issues here.

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Games you make with it are yours.
