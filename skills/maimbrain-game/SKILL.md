---
name: maimbrain-game
description: Design and build a Maimbrain game (a short Rust→wasm game for the iPhone feed) — from a brief to a validated, playable, genuinely engaging .mbx. Use when creating a new game under games/, reworking an existing one, adding art or sound to one, or tuning how one feels.
---

# Making a Maimbrain game

A Maimbrain game lives in a vertical feed, between other games, in front of someone with a thumb on the glass and about one second of patience. It isn't installed, chosen or explained. Building a *valid* game is the easy part (`docs/SPEC.md` + `mb build`). Building one people stop swiping for is the job.

**Read what this game needs, when it needs it.** The docs are long (SPEC.md alone is ~150 KB), and everything you read stays in your context for the rest of the session, slowing every later step. So don't `cat` whole docs or SDK modules: find the section (`grep -n '^#' docs/SPEC.md`) or the item (`grep -n 'pub fn' sdk/maimbrain/src/gfx3d/mod.rs`) and read that.

Before writing code:
1. **[engagement.md](engagement.md)**: what makes a game work *in this feed*. Its rules are requirements, not suggestions.
2. **`docs/SPEC.md` §2, §3, §5.2, §5.7 and §9 only**: the manifest, determinism, feed navigation (browse vs play, the card overlay areas), sensors only in play, and build, preview, test. The rest is reference.
3. **[kits/](kits/)**: if the brief is a familiar genre (runner, stacker, shooter, match-3, racer, tower defense, trivia), start from that genre's kit instead of the template: see [Start from a kit](#start-from-a-kit). Otherwise, **[template/](template/)**: a tiny complete game (tap the bubble) to start from; **[template-3d/](template-3d/)** is the 3D one (catch the drone; see [3d.md](3d.md)). `mb new games/<name>` creates a game from the 2D template, `mb new --3d games/<name>` from the 3D one. Signed in (`mb login`), the id and creator come from your account; signed out it still works, with a placeholder id and creator (or pass `--id dev.you.name --creator @you`), which build and preview fine but must become your namespace before `mb publish`. You can also copy a template to `games/<name>/` by hand and replace `NAME` in `Cargo.toml` and `manifest.toml` (and `icon.png` with your own). Keep its shape: pure rules in `src/sim.rs` with tests, and a thin host layer in `src/lib.rs` that handles tap to start, round reporting, score-before-Over, the retry guard and saving the best score. The template or kit you start from is the best API reference: it already calls most of what a game of its kind needs.
4. **[identity.md](identity.md)**: the art-direction step. Pick and write down the game's own look (reference, palette, a font pairing from the library, shape language, motion, one signature idea) before building, so it doesn't come out looking like every other Maimbrain game (docs/IDENTITY.md has the measurements).

Then, as the game needs them:
- **The SDK** (`sdk/maimbrain/src/`): look up what you call, when you call it. The doc comment on each item is its documentation. For the UI and juice kit (`ui/`, `motion.rs`, `juice.rs`), [UI and juice](#ui-and-juice) below has the recipes and **`docs/UI.md`** the tour, by section.
- **[assets.md](assets.md)** before making art or sound: when to generate it (`mb art`, `mb music`, `mb sfx`, `mb voice`, `mb model` for 3D) and when to draw it in code, the style guide, prompts, iterating, checking results, budgets. `docs/GENERATE.md` is the reference for each generator's flags.
- **[3d.md](3d.md)** for a 3D game (mb3d): the template, budgets, meshes, models, characters, looks, lighting, effects, previewing. `docs/MB3D.md` is the reference.
- **[audio.md](audio.md)** when sound has to follow the game: music that reacts to danger and progress (`music::Director`), foley from motion, rhythm games.
- **`docs/PHYSICS.md`** if the game has things that stack, topple, bounce, roll, swing or break (see [Physics](#physics)).
- **[publish.md](publish.md)** when the user asks to publish, update or describe a game, and before working on one that's already published.

## Start from a kit

The kits are small, complete, polished games, one per common genre, that already do everything this skill asks: a live title card, the UI kit's HUD and results card, juice, sound and music, a difficulty curve tuned with a bot, rounds, score submission, the retry guard, the best score, a daily mode where it fits, pure rules in `src/sim.rs` with tests, and the tuning knobs at the top of that file. Starting from one puts you at "good" on the first build; your job becomes making it *yours*.

```sh
mb kits                                  # the list, with what each plays like
mb new --kit runner games/<name>         # copies the kit; same --id/--creator rules as the templates
```

| Kit | Ships as | Tech | Pick it when the brief is… |
|---|---|---|---|
| `runner` | Bun Run | 2D | an endless runner, one-tap jumper, "dodge things as you go", auto-scrolling platformer |
| `stacker` | Picnic Pile | 2D + physics2d | stacking, balancing, dropping things on a pile, "how high can you build", anything wobbly |
| `shooter` | Nova Pip | 2D | a vertical shooter, bullet dodger, space/sky/sea shoot-'em-up, waves and a boss |
| `match3` | Poppets | 2D | a swap or match puzzle, gems/candies/tiles, cascades and combos |
| `racer` | Swerve | 3D (mb3d 2) | driving, lane racing, traffic dodging, anything "behind the vehicle" in 3D |
| `tower-defense` | Cake Keep | 2D | defending a base, placing towers or units, lanes and waves |
| `trivia` | Bright Spark | 2D, text | a quiz, questions and answers, true/false, guess-the-thing |

Don't force a kit onto a brief it doesn't fit: games built on a sensor (tilt, mic, shake), rhythm games (`games/summoned`), toys and anything with an unusual verb start from the template. A brief that mixes genres ("a runner where you shoot") starts from the kit with the core verb and borrows from another (copy the module; kits share the same shape).

**After `mb new --kit`:**
1. Read `games/<name>/DESIGN.md` (how it plays, the tuning knobs, "Make it yours", the bot's numbers) and skim `src/`. `cargo test -p <name>` and `mb build` pass as copied; keep them passing.
2. Rewrite DESIGN.md for *your* game before changing code (the workflow below still applies: the verb, the 1-second read, the first 10 seconds…).
3. Make the three changes below, then re-tune with the bot (`cargo test -p <name> --release -- --ignored --nocapture difficulty`), preview, `mb.verify()`, screenshots, the engagement checklist.

**Make it yours: three changes, every time.** A kit played as is feels like every other game made from it. Do all three:
1. **A new identity.** Theme, character, world, title, tagline, palette, sounds and icon, changed together (each kit groups its look in one place; its art and audio come from the generators: edit `art/style.toml` and the prompts in `tools/regen.sh`, then rerun it, or `mb regen` one asset; see [assets.md](assets.md)). A stranger who has seen the kit must not recognize your title card. Never ship the kit's title, character or tagline.
2. **A twist on the verb or a rule.** One change to *how it plays*, not just how it looks: a runner that flips gravity instead of jumping, a stacker whose platform tilts, a match-3 where matches push the board up, a racer that drifts, trivia where you pick the wrong answer. Each kit's DESIGN.md lists ideas and the code to touch.
3. **One new thing.** A mechanic, enemy, power-up or event the kit doesn't have, introduced 15–30 s in, so the second half of a round surprises.

Also change the fail moment and the game-over card's aftermath (they're the most-shared screens), and keep what the kit gets right: the sim/test/bot split, the round and score hooks, the retry guard, determinism (`mb.verify()` must still match).

## Workflow

**Continuing a game that's been published (a draft or live)?** Run `mb feedback <game dir>` first, before changing anything: it's what the user and their testers noted about it (see [Feedback and test builds](publish.md#feedback-and-test-builds)).

### 1. Design on one page first
Write `games/<name>/DESIGN.md` before any code, answering:
- **The verb**: the one thing the player does (tap, hold, swipe, tilt). If you need two, the second must come later.
- **The 1-second read**: what a stranger understands, and why they'd want to play, from a single still frame of the title card. It's an ad, not instructions. Sketch it.
- **The first 10 seconds**: exactly what happens after the first tap, beat by beat. The player must succeed at something within 3 s.
- **Tension and failure**: what makes it hard, how the player sees danger coming, and why failing is *funny or satisfying*, not just over.
- **The loop**: round length (aim for 30–90 s for a typical player), what escalates, and what's new every ~15 s.
- **The cards**: what the title card, the game-over card and a replay card each show (engagement.md §Cards).
- **Score**: what's counted, shown big, and saved as best.

### 1b. Art direction: choose an identity
Follow [identity.md](identity.md): add an `## Identity` section to DESIGN.md with a reference era or medium, a palette, a font pairing from the library (`mb fonts`, `mb fonts --preview`), a shape language, a motion personality, the title/results/HUD layouts, and one signature visual idea. Set `IDENTITY` in `src/lib.rs` to match (`mb new` puts a random kit identity there so new games don't all start alike; don't ship it unexamined). Bake the fonts with `mb font add`. Avoid the house defaults that docs/IDENTITY.md lists: pixel-font titles with hard shadows, purple gradients, centred "DON'T…" taglines, "SPLAT!" + big number + "NEW BEST!" cards.

### 2. Simulation first, in plain Rust, with tests
Split the crate: pure simulation modules (state, rules, physics, difficulty) that take inputs as arguments, a pure `control.rs` that turns raw input events into those inputs (the templates have one; the bot shares it), plus one thin host layer (`lib.rs` in the template, or a `game.rs` it calls) that reads input/sensors, calls the sim, plays sound/haptics, and draws. The SDK compiles on the host with no-op stubs (SPEC §9), so `cargo test -p <name>` runs your sim natively. Put tests in `src/` (`#[cfg(test)] mod tests;`): the crate is a `cdylib`, so `tests/` can't link it unless you add `"rlib"` to `crate-type`. Natively `sys::time()` is a clock your test drives: it stays at 0 unless you move it, so a test (or a bot round) that runs code reading `sys::time()` — a beat grid, `play_at`, timers — calls `sys::test::update(&mut game, dt)` each frame (or `sys::test::advance(dt)` before calling `update` itself); `sys::test::reset()` starts a test at 0.

**Write a bot** that plays the sim like a person, so you can measure difficulty: how long the first round lasts, where players die, whether the first seconds are unfailable. Start from the template's: `mb new` puts a reference bot in `src/bot.rs` (both templates; the kits have their own), and `cargo test -p <name> -- --nocapture difficulty` prints its report. It's short and meant to be adapted: keep its shape and rewrite what it looks at and how it acts for your game. A bot is only as good as its model of a human. A plain controller makes every skill level die at the same place, and a bot that misses more than people do makes an easy game look hard. Take the human numbers from `input::synth::Skill` (`first_timer()`, `casual()`, `good()`, `expert()`, or `Skill::ALL`), research-based presets per input with their sources in the docs: reaction time (`reaction_time(rng)`), lapses, timing noise for a press to a predictable moment (`timed`), and a Fitts'-law `Pointing` model for each way of aiming (`touch` in points, `point` for aiming by pointing the phone and `tilt` in degrees): `time(distance, width)` is how long a move takes to land on a target that size about 96% of the time and `scatter(distance, secs)` where a move made that fast lands (a finger never better than ~9 pt, 1 SD). `skill.aim(from, to, size°, t0, seed)` is a hand swinging the phone onto a target as that player does (Fitts time, overshoot, landing error, tremor); `skill.tap(from, to, size, t0, rng)` and `skill.flick(from, angle, len, t0, rng)` are a finger's. The templates' bots take theirs from these. Don't invent a bot's precision: if your bot needs a number the presets don't have, derive it from them, and say so in DESIGN.md. Model at least:
- **reaction delay**: it acts on what it saw `reaction_time` ago (0.23–0.42 s by skill)
- **timing noise drawn close to the event**: track the target every frame and fire when the remaining time (to contact, to the gap, to the beat) reaches the player's intended lead, drawing the timing noise *then*. Don't pre-plan an action's timing at first sight: people track continuously and commit late, so their error doesn't grow with how far ahead they saw it. A bot that timed each jump when the obstacle appeared, seconds out, was ±0.3 s off and every preset died in 11 s.
- **one action in flight**: sequence actions, and plan the next only once the previous is committed (a lane change started, a tap made). Planning several at once, or letting a stale correction fire after a newer move, makes a bot fight itself. A correction belongs to the move it corrects.
- **motor noise**: the preset's scatter and tremor in *control* units, scaled by your control's gain (a 2.6× tilt gain multiplies hand tremor by 2.6)
- **overshoot** on the first move of each new situation, then **caution** (smaller corrections) after an overshoot
- **lapses**: now and then it looks away for half a second or so
- **anticipation** for skilled players: reacting to tells before events

Drive the game only through the input events a player produces (touches, with their times), through the same code `lib.rs` uses. The templates keep that code in `src/control.rs`: pure, no host calls, it turns raw events into game actions (taps, swipes, holds, with their thresholds) and applies them to the sim. `lib.rs` feeds it `input::poll()`; the bot's `play` feeds it the bot's touches, frame by frame. Put every input rule there, never a second copy in the bot's harness: a bot that maps touches itself, or sets sim state, measures a different game. Make touches with `maimbrain::input::synth`: `tap(x, y, t)`, `hold(x, y, t, secs)`, `stroke(&points, t0, secs, synth::PANEL_HZ)` (down, a move every 1/120 s at an even speed, up), `flick(from, to, t0, secs, seed)` for a swipe as a thumb makes it (speed rising and falling, lifting while still moving, a slightly bowed path, a little noise; use it wherever the bot swipes, so swipe thresholds are tuned against real-looking input), and a `synth::Queue` that hands them out as the host would (`queue.due(frame_end)`), so a swipe spans the frames it really does.

Give it three presets (from `synth::Skill`'s first-timer, casual and good) and report each one's round length (p10, p50, p90) over ~200 seeded runs:
- **first-timer**: slow, noisy, no anticipation; should die in 20–45 s
- **casual**
- **good**: should last 1–3 min

For a game with a fixed end (boss fights, levels, a run to win) report, per preset, the win rate *and* accuracy (hits ÷ shots), time to kill each opponent and damage taken, with targets for each: a first-timer should win the first fight most of the time and lose a later one; a good player should usually win but be pushed (hit, low on health, sometimes losing the last fight); time to kill should rise across fights.

Model the specific skills your game demands too (holding still to aim, easing off near a limit, leading a moving target the way hits are actually judged). Report *how and where* each preset dies (deaths by cause) and how it misses (missed or misread inputs), not just when, and print one traced run, because medians swing a lot on small changes.

Tune the game's numbers against the bot, not your intuition. If every preset dies at about the same time, the ramp is too steep at the end: a fixed step per point soon outruns any player, so skill barely matters. The templates ramp with the square root of the score (fast at first, ever more slowly after; `src/sim.rs`), which puts their presets inside the targets. If a brief's per-level timings conflict with these targets, follow the targets and say so in DESIGN.md.

**Difficulty sanity check.** A bot's numbers are only as good as its model, and the owner's hands are the real thing. Before trusting a report:
- **Calibrate against a real run when you can.** Have the owner (or yourself, in `mb serve`) play a round with the real input, then save the replay from the preview console: `mb.save("human.json", JSON.stringify(mb.snapshot().log))`. Log the game's own stats at round end (`sys::log`: shots, hits, time per kill, damage taken), replay the log (`mb.restart(JSON.stringify({ replay: JSON.parse(await mb.load("human.json")), hold: true }))` reads them back on the same seed), and run the bot on that seed. If the human's accuracy or time to kill is clearly better than the bot preset you think matches them, the bot is too weak: fix the bot (its aim error, its reaction, how it uses the game's aim help) before the game.
- **"It's too easy" from the owner beats any bot number.** If the owner wins first time and says it's easy, it's easy, whatever the bot says (one game's bot said a good player wins 92% and a first-timer 0%, and the owner, playing for the first time, found it far too easy: its bot missed far more than a hand does). Make it harder, then recheck the bot against the owner's run.
- **Targets aren't only win rates**: set and report accuracy and time-to-kill bands per preset and per fight too (e.g. a good player hits 70–85% and takes 20–40 s per boss), and check that an aim assist (`Scope` friction, magnetism) is in the bot's input path, as it is in the player's.

Determinism (SPEC §3): all randomness from `Rng` seeded by `sys::rand_seed()`, time only from `dt`, no iteration over hash maps. Replays and friend challenges depend on it.

### 3. Art and sound
Two sources, usually mixed; [assets.md](assets.md) says which to use for what:
- **Generated**: `mb art init games/<name>` writes the game's style guide (`art/style.toml`: style words, palette, references), then `mb art sprite|frames|background|layers|tile|ui|icon` (and `mb art sky` for an mb3d panorama sky), `mb music` (a seamless whole-bar loop, optional base/high layers), `mb sfx`, `mb voice` (designed character voices speaking lines, with subtitle and lip-flap timings for `ui::Dialogue`; see assets.md "Voice and characters"), and for 3D games `mb model` (a textured, optionally rigged and animated `.glb`). mb post-processes to the platform's rules (transparent sprites, exact sizes, pixel grids, palettes, seamless tiles, PNG budgets; mono Ogg at the right loudness), writes a provenance sidecar per asset and a preview to look at (`art/preview/`), and shows the cost of each call. `mb regen <asset>` redoes one (same seed, `--new-seed`, `--prompt`, or `--reprocess` for free). Needs the user's key (`mb keys list`, `mb models`); `--provider mock` makes placeholders offline. Reference: `docs/GENERATE.md`.
- **Code-drawn**, with the bundled scripts (`mb new` already put copies of pixel.py, check_audio.py, mix_check.py and sfx.py, plus make_glb.py in a 3D game, in the game's `tools/`):
  - `scripts/pixel.py`: an RGBA canvas (rects, ellipses, polygons, lines, outline, dither, blit, flip), a PNG writer and an atlas packer that writes a Rust table of source rects. Stdlib only.
  - `scripts/check_audio.py`: you can't listen, so run this on your assets after every audio change. It reports length, peak, loudness, harshness, how much a phone speaker loses and loop clicks, and flags silent, clipping and bass-heavy files.
  - `scripts/mix_check.py`: renders a bot round's sound events with the real files and checks the mix: music audible over the effects, each effect heard over the music on a phone speaker, each sound's level for how often it plays, voices, variation, key. The rules and how to get the event log are in [assets.md §5](assets.md#5-mix-the-sound).
  - `scripts/sfx.py`: 8-bit effects (square/triangle/saw/noise with sweeps, vibrato, arpeggios, stutters) and a step sequencer for chiptune loops, encoded to mono Ogg Vorbis with `mb ogg`. `save_ogg` normalizes loudness and limits peaks, then decodes what it wrote and re-encodes it quieter if Vorbis overshot, so its files pass check_audio's clip check; very peaky sounds (an explosion with a long tail) come out quieter than the loudness target as a result. `python3 sfx.py --self-test` checks the encoder setup.
  - `scripts/make_glb.py`: an example `.glb` generator for 3D games (see [3d.md](3d.md)).

Sound files: games play mono Ogg Vorbis (a bundle can't carry WAV or MP3). `mb ogg in.wav assets/x.ogg [-q 0–10]` encodes any WAV, MP3 or Ogg, and `mb ogg x.ogg x.wav` decodes; it's built into mb on every OS, so don't install oggenc or ffmpeg or write an encoder. Sound you synthesize yourself: write a WAV, then `mb ogg` it (or use sfx.py's `save_ogg`, which also sets the loudness).

Put your own generator script in `games/<name>/tools/` and commit it (the art stays remixable). Draw anything that changes color with mood or state (skin, a liquid) in light greys and tint it at draw time: `rgba_tint` multiplies, so one sprite set gives smooth pink → red → purple transitions. Your scripts import the copies beside them in `tools/` (`import pixel`: Python puts a script's own directory on its path), so the game's tools work without the skill; commit them with the game. They're snapshots: when this skill's `scripts/` has a fix you want, copy that file over the game's (`cp <this skill's directory>/scripts/sfx.py games/<name>/tools/`; in the Maimbrain repo that's `.claude/skills/maimbrain-game/scripts`, from the plugin the installed skill's directory), and copy them in the same way for a game made before `mb new` did this. `mix_check.py` stays in the skill; run it from there. Make things big: the main character ≥ 1/3 of the screen width, anything the player must track ≥ ~40 units, and an art pixel ≥ 2–3 units. Small sprites that look fine in a desktop screenshot are unreadable on a phone. Store pixel art at `scale=4` (crisp under the platform's linear filtering) and draw each art pixel as a whole number of logical units.

**The icon** (`icon.png`, 256×256, opaque) is the game's face in the feed, the studio and every share link, so make one even when all the art is drawn in code: a close-up of your character or hook, not text. `mb art icon <game dir> "…"` generates it in the style guide's look; without a key, draw it as pixel art with `pixel.py` (a 64×64 canvas at `scale=4`: one subject, big and crisp, no soft glows; the racer kit's `tools/gen_icon.py` shows the canvas calls). `mb new` starts every game with a placeholder: `mb build` warns while it's there (or while a kit's icon is), and `mb publish` refuses it.

### 4. Build, preview, look at it
- `mb build <game dir>` compiles, packs and validates. Fix every error and warning. (In the Maimbrain repo, `mb` is `mise exec -- cargo run -q -p mb-cli --`.)
- `mb serve --watch <game dir>` runs it in a desktop browser at http://127.0.0.1:8765/runtime/index.html?overlays and rebuilds every time you save: new code is swapped into the open page within about half a second (a fast build without LTO or wasm-opt; `mb build` is still the real one), changed assets are hot-swapped, and compiler errors appear over the game. It runs in the foreground, so start it in the background once and open the page once; don't reload it by hand. Use `--port` if 8765 is taken. Mouse = touch, mouse position = tilt (for a game with attitude, where the phone points), console = log, haptics and leaderboard calls. An installed `mb` has the web runtime built in; in the Maimbrain repo, run `just runtime` once first (it builds `runtime/dist`, which `cargo run -p mb-cli -- serve` then embeds automatically). If `mb serve` says it was built without the web runtime, that's what's missing. (Plain `mb serve`, without `--watch`, rebuilds when you reload the page instead.)
- **The iteration loop** with `--watch`: edit, then `curl -s http://127.0.0.1:8765/__mb/wait` (it returns once your change is built and running in the page, as JSON). `"state": "running"` means look at the page; `"failed"` carries the compiler's `errors` (file, line, message), `"error"` a page's runtime error (a trap, a panic message in `pages[].message`). The server's own output has the same as `[mb] build N ok|failed …` and `[mb] page N running|error|log …` lines. Then, in the page's console, `await mb.restore()` replays the session you had before the change into the new build up to the same frame (held if it was held) and resolves to `{ frames, firstMismatch, … }`: `firstMismatch: null` means the run went exactly as before; a number is the first frame that came out differently, expected when you changed what happens or how it's drawn. So set up a moment once (a held session, scripted taps, `mb.step`), and after each edit `wait` → `mb.restore()` → screenshot shows the same moment with your change. Add `&restore` to the URL to restore automatically after every reload. SPEC §9 has the details.
- Preview aids (SPEC §9): `?overlays` shades the card overlays and the pause pill. `?speed=0.25` slows time. In the console, `mb.suspend()`, `await mb.step(n)` and `mb.resume()` freeze a moment for a screenshot (wait ~100 ms after stepping before the screenshot). A hidden tab or headless browser runs no animation frames, so there start a held session (`await mb.restart('{"hold": true}')`) and drive it with `mb.tap(x, y)` and `await mb.step(n)`. For screenshots from a script, use `mb shot` (below) rather than driving a browser yourself. Add `DEBUG_*` constants (start level, force a shape, fake a sensor reading) to reach late states quickly, and reset them before you finish.
- **Check determinism** before you call it done: play a round (or script one in a held session), then `await mb.verify()` in the console. It replays the run frame by frame and compares draw hashes: `{ match: true }` is the goal; otherwise `firstMismatch` is the first frame that differs, and the cause is something unrecorded (randomness not from the session seed, `dt`-independent timing, hash-map iteration order, game state changed in `render`). SPEC §9 shows the manual version with `mb.restart({ hold: true })`, `mb.step` and `mb.snapshot()`.
- **Look at the game after every visual change**: `mb shot <game dir> --autoplay --sheet` (add the input that reaches the moment you changed) and Read the sheet it prints, `screenshots/shot-sheet.png`. It builds, renders the game in headless Chrome like a phone (360×640 logical, 2× pixels, touch) and saves shots at 0.5, 2, 5 and 10 s of a fresh held session, the same pixels every run; `--autoplay` taps the middle at 0.5 s, after the first shot, to get past the title card. Script play with `--at 1,3,6`, `--tap x,y@t`, `--swipe x1,y1,x2,y2@t[:secs]` (real touch events, a move every frame; one finger at a time) and `--tilt <roll°>[,<pitch°>]@t[:secs]` (degrees, not radians like `Tilt::roll()`: `--tilt 25@3`; the phone is upright until then: touches don't tilt it, as the mouse does in `mb serve`; `--tilt roll@t:0.4` turns it there over 0.4 s as a hand does, which is what to tune tilt feel against); To shoot a moment rather than a time (the boss arrives, a part breaks, the first crash), call `sys::mark("boss")` in the `update` where it happens and shoot `--at mark:boss,mark:boss+0.5`: the shot comes after the frame the game first marks it (and 0.5 s later), however long that takes (up to `--mark-wait`, 60 s), so no guessed times or debug parameters; it works in `--sheet` lists beside plain times. `--overlays` shades the feed's overlays, `--name before` keeps a set to compare against, and `--hz 120` runs the game in 1/120 s frames as a ProMotion iPhone does (shoot the same moments at both rates: if they differ, something moves per frame rather than per second; `await mb.step(n, { dt: 1 / 120 })` does the same in the console). A 4-shot run takes a few seconds. It prints what the game logged (`sys::log`, info and up) with the frame and time of each line, and `--sheet` writes them into `shot-sheet.txt` beside the sheet; `--logs` adds debug lines, `--logs-match player` keeps the lines you're after (log a node's handle from `Scene::build` at debug to learn which handle a budget warning names). Look at the sheet critically, as a player glancing at a phone: at the speed the game moves, can you tell at once where the player is, what's coming and what kills? Is there contrast between the player, the hazards and the background (not near-black on black, edges of the track or lanes visible)? Does the camera show what's coming? Is the HUD readable and clear of the player and the feed's overlays? In 3D, measure it too: call `node.mark_subject()` on the player's node (the hero, the mech, the car) when it spawns and add `--readability`. Under each shot it prints how much of the screen the subject covers, how its outline stands out from what's right around it in lightness (CIE L*, what survives in greyscale) and in color, its own tonal range, how much of it the HUD covers and the frame's lightness range, and warns when it doesn't separate (less than a third of the outline 15 L* from its surroundings, or a median under 12), is tiny (under 0.6% of the screen), is one flat tone, sits under the HUD, or the frame is dark (95% under L* 30); with the retro look on it says the look may be the cause. It also writes `shot-<t>s.readability.png`, the frame in greyscale with the subject outlined: Read it. A subject that only separates by hue disappears there. `--subject <handle>` names a node without `mark_subject`. Fix what you see and shoot again. Then cover the title, the first seconds, mid-round, danger, failure and game-over, and check them against the [engagement checklist](engagement.md#checklist).
- **Measure how it runs** instead of writing your own DevTools script: `mb measure <game dir> --autoplay` (with the same `--tap`/`--swipe`/`--tilt` as `mb shot`, `--seconds 10` by default) plays the game live in headless Chrome and prints frame intervals and long frames (the first 0.5 s, shaders compiling, on a startup line of its own; `--from` moves it), CPU ms per callback, GPU ms (the scene at a few moments drawn over and over; `--gpu-at 5,20` picks the moments), draws and triangles, input latency by kind (touch down, move, up, tilt) and `mb.verify()`'s verdict, in one short block (`--verbose` for every line); it exits non-zero if the run doesn't replay. While tuning, `mb measure <game> --autoplay --quick` (a few seconds: the GPU and draws at the `--gpu-at` moments, no live run) tells you whether a change helped; the default run's GPU ± is within one browser session, so use `--final` (three sessions, slower) for a number you'll keep or report. To judge an optimization, `mb measure <game> --autoplay --ab feed/<the build before>.mbx` times both builds in turn and prints B − A with a confidence interval (`--gpu-sessions 6` to resolve differences of a few tenths of a ms); compare builds that way, not with two separate runs, whose machine state can differ. To price one feature, read a debug parameter that switches it off (`sys::param("shafts")`) and run `mb measure <game> --autoplay --ab-param shafts=1,0`: one build, both values, B − A with its own interval. An older commit copied out of the workspace for `--ab` is built against this build's SDK (mb says so), or refused if it can't be. `--hz 120` runs 1/120 s frames on a 120 Hz timer; `--json` gives the raw frames and objects. If it prints a `pacing` line, the browser ran frames slower than asked (30 fps from a busy machine or a laptop on battery) while your game used a small part of the frame: not your game; don't tune for it (or let a quality governor react to it), rerun when the machine is quieter. Its latency numbers are headless Chrome's, good for comparing two builds, not for what a phone feels (SPEC §5.2 has the device's); `received→submit` is the part your game and the runtime own.

### 5. Publish (when the user asks)
Read [publish.md](publish.md) first: `mb publish --model "<you>"` makes a private draft the user plays on their phone, `mb describe` sends DESCRIPTION.txt, and `mb submit` sends it to review only once the user says so. It also covers updating a published game (which version number to bump, NOTES.md, never a new season unless asked) and `mb feedback` / test builds.

### 6. Hand off honestly
Write down what you couldn't verify without a phone (feel of tilt, haptics, audio balance, safe areas), with the exact numbers to tune (gain, dead zone, timings) collected as named constants at the top of one file.

## Platform facts that bite

- **Feed cards are untouchable.** In browse, the title or game-over screen *is* the card, running live but receiving no touches and no sensor readings. A tap enters play and is also delivered to your game, so a tap anywhere on the title must start the round. Never start a round from a sensor reading, a timer or a swipe. Because a card can't respond, it must never show playing instructions: no "BLOW", "TILT", "SHAKE" or "TAP", and no phone icons or arrows. Cards sell, play teaches (engagement.md §1).
- **Card overlays.** On a card, the platform draws the title and creator over the bottom ~70 pt and like/comment/report buttons down the right ~56 pt, plus a pause pill top-left in play (inside the top inset). A replay card's "Beat …" banner takes the bottom ~230 pt, or the top ~110 pt with `replay_banner = "top"` in the manifest (for runners and racers, below). Keep buttons and key text out of these areas: use `sys::screen()` insets (the device's safe areas; they don't include the card overlays), then stay inside the middle.
- **Report rounds** with `sys::round`: `Idle` on the title, `Playing` the moment a round starts, `Over` when it ends. `Over` sends the player back to browse after ~1.2 s, with your game-over screen as the card, so submit the score *before* reporting `Over` and make the game-over screen great.
- **Sound is off for many players** (ringer switch). Everything must read without audio. Sound and haptics confirm what the eye already saw.
- **Haptics** only play in play mode and are capped at ~30/s; never use them as the only signal.
- **Assets load asynchronously**, including `startup_assets`. The first frame must draw something good even with nothing loaded (shapes, gradients, text), then swap in sprites when `Asset::state()` is ready.
- **The screen** is your `logical_size` letterboxed to the phone; 360×640 portrait is the norm. Thumb-reachable area is the lower-middle; the top is for score.
- **Motion controls**: read `sensors::tilt()` and use `Tilt::roll()` for tipping left/right (it works held upright, slumped or flat).
  - **Calibrate**: take a reference when play starts, at each new level, and after a pause, but only while the phone is still (motion near zero). A reference taken mid-shake is garbage. Let the reference follow the player when they tip back past it, so a player still tipped from the last level isn't stuck.
  - **Amplify**: physical actions often need more rotation than a comfortable ~45° of phone. Multiply (gains of 2–3 are typical) and make the on-screen object show the amplified angle, so what the player sees is what they're doing.
  - **Don't smooth what's already smooth.** On iOS the tilt is CoreMotion's fused gravity at 100 Hz (SPEC §5.7): hand shake doesn't move it and it has no filter lag, so smooth it ≤ ~10 ms at most, with a dead zone of ~1–2°. Every filter or spring after it adds lag on a moving hand: a critically damped follow with angular frequency ω trails by about 2/ω (ω 24 → ~83 ms), which reads as mush. Measure your total lag (sensor → what moves on screen) rather than guessing; aim for ≤ 60 ms.
  - **Pointing to aim** (a light-gun scope, a flashlight, a camera you look around with): tilt can't see the phone turning left and right, so declare `sensors = ["attitude"]` (gyro-fused orientation + rotation rate, SPEC §5.7) and use `sensors::Scope`: `let mut scope = Scope::new(w, h);` then `scope.recenter()` on the tap that starts play, `scope.update(dt)` every frame and draw the crosshair at `scope.position()`. Its defaults are the recipe, the simple light-gun mapping, which a playtest on an iPhone preferred to the precision options below:
    - gain ~30–45° of turn across the screen (default 32, so a ±16° wrist turn reaches both edges): less is twitchy, more makes the player twist. Every degree is the same at any speed, so the crosshair goes where the phone points;
    - a light fixed smoothing (`Smoothing::Fixed(0.008)`, ≤ 0.01): more and every swing trails the hand;
    - a tiny backlash dead zone (0.1°): wobble smaller than it moves nothing, at no cost in lag;
    - precision options, off by default; `Scope::precise(w, h)` turns them all on (with 40° across at full gain) and each is a field to set on its own. Try them when fine aiming at small targets is the game (a sniper, tiny targets far apart) and players miss by a hair, and compare with the default on a phone, not in the preview:
      - smoothing that adapts to speed: `Smoothing::PRECISE` is a One Euro filter (1.5 Hz still, +0.5 Hz per °/s), which cuts a resting hand's tremor to about a third yet trails a 60°/s swing by ~5 ms (a fixed filter can't do both: one that steadies the rest drags every swing). Slow corrections trail a little;
      - a precision zone: `GainCurve::PRECISE` gives slow turns (≤ 10°/s, homing in) ×0.6 of the gain, easing to the full gain at 50°/s (swings), so fine aiming is finer without longer swings;
      - friction on targets: `scope.friction` (0.4 in `precise`) with `scope.set_targets(&[Target::new(x, y, r)])` every frame drops the gain to ×0.6 over one (easing back by 1.5× its radius), so a swing stops on it and corrections on it are fine. It only scales the hand's movement; the crosshair never moves by itself. `scope.magnetism` (off in both; 0.1–0.3) adds a gentle pull toward the target in proportion to the hand's movement, for assisted modes;
      - with the curve or friction the crosshair adds up gained movements, so it isn't pinned to a heading (a slow turn out and a fast one back ends a little off): recentering matters more;
    - recenter on play start and on a gesture (a two-finger tap, or a button), because the heading is relative and drifts slowly; `Scope` also eases a crosshair resting near the center back to it, which soaks up the drift without fighting a player aiming off-center;
    - hold the crosshair at the edges with a little give, and leave the edge as soon as the player turns back (no unwinding the overshoot);
    - fall back when there's no gyroscope: `scope.source()` is `Tilt` (roll aims) or `Drag`; pass finger moves to `scope.drag(dx, dy)` and say "drag to aim". Leave `attitude` out of `needs` unless the game truly can't be played that way.
    - Fire on a tap anywhere (the player is pointing, not touching the target), and keep the shot cheap to land: a slightly generous hit radius reads as "I hit what I pointed at".
    - Show it: `scope.gain_scale()` (the multiplier now), `scope.time_constant()` (the smoothing now) and `scope.speed()` (°/s) are there for a debug readout; the gallery's AIM page has one and a DEFAULT/PRECISE toggle to feel the difference.
    - Bots and tests: `input::synth::aim(from, to, t0, secs, seed).at(t)` is a hand swinging onto a target (overshoot, settle, tremor); feed it to `scope.update_with(dt, Some(att), None)`. For a bot, `synth::Skill::good().aim(from, to, size°, t0, seed)` takes the swing's time, landing error and tremor from how precisely people point a held device (2–3 bits/s, not a finger's 6–7), and `.landed()` is when it settles. With the gain curve (`precise`) a swing lands a little short of a target computed at full gain, so aim like a player: swing, read `scope.position()`, then correct by the miss ÷ (width ÷ gain × `scope.gain_scale()`) degrees. Check feel in the preview (the mouse points the phone: across the window turns it ±45°) and script it with `mb shot --aim yaw,pitch@t[:secs]`.
- **Mic loudness** (`sensors::loudness()`):
  - It's `None` until the player allows the mic, and if they refuse. Show a clear "ALLOW THE MIC TO PLAY" state, never a dead game.
  - Rooms differ, so learn the noise floor continuously: track the minimum recent reading, let it creep up slowly (~0.03/s) and drop instantly to any lower reading. Treat input relative to that floor. Don't wait for a quiet first second; players blow on the first tap.
  - The game's own audio comes out of a speaker a few centimetres from the mic: measured, music alone read ~0.45–0.55. So:
    - lower the music while the mic is listening
    - require a blow to stay above the threshold for ~0.1 s before it counts (effects are short bursts; blows are sustained)
    - never drive a sound's volume from the mic level (a wind sound that grows as you blow feeds back into the mic as more wind)
  - Measured on an iPhone: quiet room plus game music ~0.40–0.55, hardest blow ~0.86. The range is narrow, so treat ~0.85 as full strength (a "too hard" line above that can't be reached).
  - Always show a live level (a meter, a flame bending) so players learn the scale.
  - The platform's mic badge sits beside the pause pill in play: keep the top-left ~110×50 pt clear.
- **Motion** (`sensors::motion()`):
  - One reading per frame. Measure a shake by accumulating energy, e.g. `energy += max(0, magnitude - 0.3) * dt`, with decay. Don't react to single frames.
  - A vigorous shake is ~1.5–3 g.
  - Show the build-up continuously (a gauge, a wobble, a bulge) and scale haptics with it.
  - Shakes blur the screen, so make the key state readable at a glance.
- **Failure order**: when the player fails, play the freeze and fail animation while still `Playing`, and report `Over` (after submitting the score) when the game-over screen appears. Reporting `Over` at the moment of failure would send the player back to browse ~1.2 s later, mid-animation.
- **After a round ends**: ignore taps for ~0.35 s after reporting `Over`, so a frantic tap during the failure doesn't instantly restart, then let any tap retry.
- The helpers every game needs (outlined text, easing, springs, a particle pool, screen shake, buttons, a title and a game-over card) are in the SDK's kit: see [UI and juice](#ui-and-juice). Don't hand-roll them.
- **Text and fonts**: use the game's own fonts (identity.md). `mb font add <game> <library id>` bakes one of 96 library fonts (OFL/Apache) into `assets/fonts/<id>.mbf`; draw with `Font::asset("assets/fonts/<id>.mbf", fallback)` anywhere a font goes (`gfx2d::text`, `ui::text`, a `Theme`, 3D text). Needs `stdlib = { mb2d = 2 }`. Fonts are subset to `--chars` (Latin-1 by default: accented letters, curly quotes, dashes), so text outside it draws as `?`; `mb build` warns, naming the characters and where they are. Distance-field fonts are crisp at any size and take the host's outlines and glows. Bitmap (pixel) fonts are crisp at multiples of `font.pixel_em()` (`theme.fit(font, size)` rounds for you). Use ≥ 16 units for anything the player must read. The built-in Inter (0/1) and 5×7 pixel font (2) are fallbacks; both cover Latin-1, and the pixel font adds `× ÷ · ° • … ← ↑ → ↓ ♥ ★` (SPEC §5.3).

## UI and juice

Hand-rolled title screens, score labels and game-over text are where games look cheapest. Use the SDK's kit instead (`docs/UI.md` has the full tour; `games/gallery` shows every piece, `mb serve games/gallery`):
- `maimbrain::ui`: a `Theme` composed from the game's identity (`Theme::from_identity("…")`, or one of 17 `Theme::preset`s that look nothing alike: shape language, borders, textures, title/results/HUD layouts, motion personality, letter case, library fonts; the old `candy`/`night`/`arcade`/`paper`/`jungle` are the house look), anti-aliased shapes (`ui::shape`), styled text (`ui::text(..)` with outline, soft shadow, glow, wrap, alignment, measuring), 33 vector `Icon`s, `Button`s (pressed state, 44 pt touch area, haptic hook), pills, progress bars, ring meters, ribbons, `Layout` (safe area, the card area clear of the feed overlays, the pause-pill zone), and ready-made `TitleCard`, `ResultsCard`, `StatsCard` (between rounds or fights: stats rolling up, a rank stamp, the next opponent, a wipe into the next scene; `.sounds(CardSounds::load("assets/sfx"))` after `scripts/card_sfx.py games/<name>/assets/sfx` gives it a jingle, ticks, a stamp, whooshes and a sting), `Hud` and `Countdown`.
- `maimbrain::motion`: `Ease` (the full easings.net set), `Tween`, `Spring` (critical / bouncy / wobbly), `Seq`, `stagger`, `Shake`, `Punch`, `Counter` (rolling numbers), `HitStop`, `Pulse`.
- `maimbrain::juice`: `Particles` (confetti, cannons, sparkles, stars, puffs, sparks, rings, `celebrate`), `Popups` ("+100"), `Combo`, `Flash`, `VignettePulse`.

The templates already use it: copy their shape, not their look. Rules: update every kit object in `update(dt)` and draw it in `render` (it never reads a clock, so it replays); seed `Particles` from your game's `Rng`; restyle by changing the `Theme` and its `style`, not by forking widgets; keep cards' buttons inside `layout.card`. When no layout fits the game's world, draw your own card (a receipt, a rosette, a chalkboard) with the kit's text and shapes: that is often the most memorable thing in a game. Build the `ResultsCard` `.state_only()` and draw from its state (`shown_score()`, `best()`, `new_best_shown()`, `stamp()`, `gap_to_best()`; docs/UI.md "Your own card"), so the roll-up, the new-best moment and the retry rule stay the platform's.

**Title screen in 10 lines**
```rust
// init
let layout = Layout::new();
let theme = Theme::from_identity(IDENTITY).load_fonts();   // identity.md
let title = TitleCard::new("BUBBLE", theme).tagline("don't let it pop").best(best as i64);
// update
self.title.update(dt);
// render, over your live scene (the card sells; any tap starts the round)
self.title.draw(&self.layout);
```

**Results card** (on game over, after `store::submit_score`, before `sys::round(Round::Over)`):
```rust
self.results = Some(ResultsCard::new(score, prev_best, self.theme, &self.layout, self.rng.next_u32() as u64)
    .heading("SPLAT!").label("POINTS"));          // .floating() for no panel; .extra(button) for SCORES
// update: feed it events first; its buttons, or any other tap after its guard, retry
if let Some(card) = &mut self.results {
    let retry = card.input(&e) == Some(ResultsAction::Retry);    // per event
    if card.update(dt) == Some(ResultsEvent::NewBest) { /* fanfare, haptic */ }
}
// render
if let Some(card) = &self.results { card.draw() }
```
It rolls the score up, then shows BEST (and "3 MORE TO BEAT IT" when close) or celebrates a new best, in the theme's results layout (a panel with a ribbon, a receipt, a scoreboard, a stamp, an editorial page). Write the heading in the game's own voice; "SPLAT!" is the example everyone copies.

**Combo meter**
```rust
self.combo = Combo::new(1.5);                                     // init: 1.5 s window
let n = self.combo.hit();                                         // each success
self.popups.score(x, y, 10 * self.combo.multiplier() as i64, theme.gold);
self.combo.break_combo();                                         // a miss
if let Some(lost) = self.combo.update(dt) { /* timed out after `lost` hits */ }
self.combo.draw(300.0, self.layout.hud.y + 90.0, &theme);         // render
```

**Juice on every hit**: `shake.add(0.3)` (apply in render inside `gfx2d::push`/`pop`), `punch.kick(0.25)` on the score, `hitstop.freeze(0.05)` (run the sim with `hitstop.step(dt)`), `fx.burst(..)` or `fx.sparkles(..)` at the point, `popups.score(..)`, and the sound pitched up with the combo. Celebrate milestones with `fx.celebrate(x, y, &theme)`. `Hud::set_score` rolls and punches the score for you and marks a beaten best.

Pause-safe HUD: the platform draws the pause pill and pauses the game by not calling `update`, so a HUD built from the kit freezes with it. Don't draw a pause button; if the game needs a run-up after a pause, start a `Countdown` in `Game::resume` and skip the simulation while it's `active()`.

## Physics

The SDK has deterministic rigid-body physics (Rapier inside your wasm): `maimbrain::physics2d` in logical pixels for mb2d games, `maimbrain::physics3d` in meters for mb3d games. Turn it on in `Cargo.toml`: `maimbrain = { …, features = ["physics2d"] }` (or `"physics3d"`). `docs/PHYSICS.md` is the full API; `games/topple` (2D crane stacker) and `games/knockdown` (3D can toss) are worked examples.

**When to use it**: anything whose fun is how solid things react — stacking and toppling, knocking things over, ragdolls, marbles and pinball, cars and contraptions, launchers, chains and ropes, destruction. **When not to**: games whose motion you want to author exactly (a rhythm game, a platformer with tight jumps, a slingshot that must feel the same every time): hand-written kinematics are smaller, cheaper and easier to tune. Physics adds ~0.45–0.6 MB (2D) or ~0.6–0.8 MB (3D) to `game.wasm`; never enable both.

**The shape of a physics game** (the template's split still holds):
- `sim.rs` owns a `World`, built in `Sim::new` from your seeded `Rng`. `Sim::step(dt)` calls `world.step(dt)` once, then reads `world.events()` for scoring, sounds (push cues) and effects. Input becomes impulses, forces or `move_kinematic` targets in `update`, never in `render`.
- `render` reads only: `world.with_transform(id, || draw centered at 0,0)` or `draw_pose(id)` in 2D, `world.sync(id, node)` in 3D. Prototype with `world.draw_debug()` (2D), then replace it with art that matches the shapes.
- Keep your own `Vec` of game objects holding their `BodyId` and game state (kind, color, "landed"); use `Body::tag` only for a quick lookup. Never iterate a `HashMap` of ids.

**Patterns that work**:
- Sound and haptics from `Event::ContactBegin`: `speed` is the closing speed (px/s or m/s), mass-independent. Ignore contacts under ~60–90 px/s (2D) or ~0.8 m/s (3D), set volume from speed, play at most 2–3 thuds a frame, haptic `Tap` above a threshold and `Heavy` for big hits.
- Goals, pickups, kill zones and "fell in the water": a `.sensor()` shape and `Event::SensorEnter`.
- "Has it settled?": `world.awake_count() == 0`, or a body's speed under a threshold for ~0.3 s.
- Things the player drags or steers: a kinematic body moved with `move_kinematic(id, pos, angle)` every frame; it pushes dynamic bodies properly.
- Held objects: `Joint::rope` from a kinematic body plus `set_fixed_rotation(id, true)` keeps it level while swinging (Topple's crane); release with `unjoin` and `set_fixed_rotation(id, false)`.
- Breakable structures: `Joint::fixed` between pieces, `unjoin` when `joint_torque`/`joint_force` passes a limit you tune with a bot.
- Show where a throw or drop will go (a dotted arc, a landing marker): players can't judge momentum on a phone. Topple's `predicted_arc` simulates the free fall in a few lines.
- Aiming in 3D: a ray through the tapped pixel from your `Camera` (Knockdown's `screen_ray`), `world.raycast` for the target, then solve the launch velocity so the arc passes through it.

**Pitfalls** (all in docs/PHYSICS.md):
- Units: 2D is pixels (100 px = 1 m, gravity 981 px/s², +y down, clockwise angles like `gfx2d::rotate`); keep bodies 10–1000 px. 3D is meters; keep things above ~5 cm and give small fast things `.bullet()`.
- Build stacks awake and a hair apart; `.asleep()` stacks float when the bottom is knocked out.
- `apply_force` lasts one frame: call it every frame you push. Kicks are `apply_impulse` or `add_velocity`.
- Create joints after placing both bodies (anchors are world points now); joined bodies don't collide unless `.collide(true)`.
- Draw `draw_pose`/`sync`, not `position`, or motion judders at 120 Hz.
- Masses that touch should be within ~10:1; tall stacks want `set_solver_iterations(6..8)`.

**Budgets**: ≤ ~150 awake 2D bodies or ≤ ~80 awake 3D bodies keeps physics near 1 ms on an iPhone 13 class phone (an M1 Pro steps 200 bodies in ~0.2 ms 2D, ~0.4 ms 3D). Sleeping bodies are nearly free. Remove what leaves the play area.

**Determinism**: it's built in (fixed 60 Hz steps from the recorded `dt`, `enhanced-determinism`), so the usual check applies unchanged: play a round, `await mb.verify()`, expect `match: true`. If it fails, look for a world changed in `render`, bodies spawned from unordered iteration, or physics fed from anything but game state.

## Before you call it done

**Review it like a stranger first.** Your tools flag what they measure (mix, smoothness, budgets, replays); they can't tell you the hero looks like a placeholder, the world never changes or nobody knows why they're running. So before you call it done, and again whenever you continue an existing game: take a fresh `mb shot --sheet` of the title card, the first 10 s, a turn or set piece, 60 s and 150 s in, and the game-over card (shoot set pieces with `--at mark:<label>` where the game `sys::mark`s them; reach late moments with debug parameters rather than edited constants: read `sys::param_as::<usize>("zone")`, `sys::param("autopilot")` in `init` to choose where a session starts, and shoot with `--param zone=3 --param autopilot=1`; they're never set on a device, so the feed always starts from the beginning), read them against the brief, the hero bar (3D section), [identity.md](identity.md) and every [engagement](engagement.md) section (especially §9 Personality and §10 A reason to keep going), and write `REVIEW.md`: the 5 changes that would most improve the game for a player, ranked, each with evidence (a screenshot, a measurement, a file:line). Be demanding: assume a stranger swipes past in one second. Then do the top items before polishing anything else, and keep the review next to DESIGN.md.

- [ ] `mb build` validates with no warnings; `cargo test -p <name>` passes; the bot (with `synth::Skill` numbers) says the difficulty curve is right, its accuracy and time-to-kill are in their bands, and it agrees with a real run or the owner's verdict ("Difficulty sanity check").
- [ ] `await mb.verify()` after a played round returns `match: true` (the run replays exactly).
- [ ] Every item in the [engagement checklist](engagement.md#checklist).
- [ ] 3D: the player's node is `mark_subject`ed and `mb shot … --readability` at a few moments (including the darkest scene) gives no warnings.
- [ ] An `mb shot … --sheet` of the final build looked at (readable at speed, the player and hazards standing out from the background, the HUD clear of the player), and screenshots reviewed for the title card, game-over card, mid-round and both edges of danger.
- [ ] 3D: `mb shot … --smoothness` over a played stretch lists no spikes in the camera's motion; the cuts and impulses you meant are `sys::mark`ed and show as marked (see 3d.md "Smooth motion").
- [ ] The identity check in [identity.md](identity.md#3-check-it-against-the-house-look) answered against docs/IDENTITY.md: the game's own fonts (`mb build` gives no font or theme warning), palette and cards, and no "yes" left that you couldn't justify.
- [ ] Started from a kit? All three "Make it yours" changes are in, and nothing of the kit's title, character or tagline is left.
- [ ] DESIGN.md updated to match what you built, with the "not verified on device" list and tuning constants.
- [ ] The game's own `icon.png`: no placeholder or kit icon left (`mb build` says so), and you've looked at it.
- [ ] Published with `--model` naming you (your own model name and version)?
- [ ] An update to a published game? The right number bumped (fix, update or new season), NOTES.md has the version's entry, and no new season unless the user asked for one (or a board that ranks differently needs one, and they agreed).
- [ ] Published? `DESCRIPTION.txt` says what the game is and how to play it, its first line works on its own, and `mb describe` sent it.
- [ ] Generated assets: every preview looked at, `mb art check` clean (no mock placeholders, within budgets), and the user told what the art and sound cost.
- [ ] Sound: `check_audio.py` and `mix_check.py` (on a bot round) clean, or each flag explained in DESIGN.md.
