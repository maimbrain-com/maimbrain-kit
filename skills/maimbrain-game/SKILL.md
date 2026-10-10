---
name: maimbrain-game
description: Design and build a Maimbrain game (a short Rust→wasm game for the iPhone feed) — from a brief to a validated, playable, genuinely engaging .mbx. Use when creating a new game under games/, reworking an existing one, adding art or sound to one, or tuning how one feels.
---

# Making a Maimbrain game

A Maimbrain game lives in a vertical feed, between other games, in front of someone with a thumb on the glass and about one second of patience. It isn't installed, chosen or explained. Building a *valid* game is the easy part (`docs/SPEC.md` + `mb build`). Building one people stop swiping for is the job.

Read these before writing code:
1. **`docs/SPEC.md`**: the contract. Especially §2 (manifest), §3 (determinism), §5.2 (feed navigation: browse vs play, the card overlay areas), §5.7 (sensors only in play), §9 (build, preview, test).
2. **[engagement.md](engagement.md)**: what makes a game work *in this feed*. Its rules are requirements, not suggestions.
3. **[kits/](kits/)**: if the brief is a familiar genre (runner, stacker, shooter, match-3, racer, tower defense, trivia), start from that genre's kit instead of the template: see [Start from a kit](#start-from-a-kit). Otherwise, **[template/](template/)**: a tiny complete game (tap the bubble) to start from; **[template-3d/](template-3d/)** is the 3D one (catch the drone; see the 3D section). `mb new games/<name>` creates a game from the 2D template, `mb new --3d games/<name>` from the 3D one. Signed in (`mb login`), the id and creator come from your account; signed out it still works, with a placeholder id and creator (or pass `--id dev.you.name --creator @you`), which build and preview fine but must become your namespace before `mb publish`. You can also copy a template to `games/<name>/` by hand and replace `NAME` in `Cargo.toml` and `manifest.toml` (and `icon.png` with your own). Keep its shape: pure rules in `src/sim.rs` with tests, and a thin host layer in `src/lib.rs` that handles tap to start, round reporting, score-before-Over, the retry guard and saving the best score.
4. **`sdk/maimbrain/src/`**: the SDK. Read the core (lib, sys, input, gfx2d, audio, store, sensors) in full; for the UI and juice kit (`ui/`, `motion.rs`, `juice.rs`), **`docs/UI.md`** is the tour and the section [UI and juice](#ui-and-juice) below has the recipes.
5. **[identity.md](identity.md)**: the art-direction step. Pick and write down the game's own look (reference, palette, a font pairing from the library, shape language, motion, one signature idea) before building, so it doesn't come out looking like every other Maimbrain game (docs/IDENTITY.md has the measurements).
6. **[assets.md](assets.md)** before making art or sound: when to generate it (`mb art`, `mb music`, `mb sfx`, `mb voice`, `mb model` for 3D) and when to draw it in code, the style guide, prompts, iterating, checking results, budgets.
7. **`docs/PHYSICS.md`** if the game has things that stack, topple, bounce, roll, swing or break (see the Physics section).

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
  - `scripts/make_glb.py`: an example `.glb` generator for 3D games (see the 3D section).

Sound files: games play mono Ogg Vorbis (a bundle can't carry WAV or MP3). `mb ogg in.wav assets/x.ogg [-q 0–10]` encodes any WAV, MP3 or Ogg, and `mb ogg x.ogg x.wav` decodes; it's built into mb on every OS, so don't install oggenc or ffmpeg or write an encoder. Sound you synthesize yourself: write a WAV, then `mb ogg` it (or use sfx.py's `save_ogg`, which also sets the loudness).

Put your own generator script in `games/<name>/tools/` and commit it (the art stays remixable). Draw anything that changes color with mood or state (skin, a liquid) in light greys and tint it at draw time: `rgba_tint` multiplies, so one sprite set gives smooth pink → red → purple transitions. Your scripts import the copies beside them in `tools/` (`import pixel`: Python puts a script's own directory on its path), so the game's tools work without the skill; commit them with the game. They're snapshots: when this skill's `scripts/` has a fix you want, copy that file over the game's (`cp <this skill's directory>/scripts/sfx.py games/<name>/tools/`; in the Maimbrain repo that's `.claude/skills/maimbrain-game/scripts`, from the plugin the installed skill's directory), and copy them in the same way for a game made before `mb new` did this. `mix_check.py` stays in the skill; run it from there. Make things big: the main character ≥ 1/3 of the screen width, anything the player must track ≥ ~40 units, and an art pixel ≥ 2–3 units. Small sprites that look fine in a desktop screenshot are unreadable on a phone. Store pixel art at `scale=4` (crisp under the platform's linear filtering) and draw each art pixel as a whole number of logical units.

**The icon** (`icon.png`, 256×256, opaque) is the game's face in the feed, the studio and every share link, so make one even when all the art is drawn in code: a close-up of your character or hook, not text. `mb art icon <game dir> "…"` generates it in the style guide's look; without a key, draw it with `pixel.py` (the racer kit's `tools/gen_icon.py` is an example). `mb new` starts every game with a placeholder: `mb build` warns while it's there (or while a kit's icon is), and `mb publish` refuses it.

### 4. Build, preview, look at it
- `mb build <game dir>` compiles, packs and validates. Fix every error and warning. (In the Maimbrain repo, `mb` is `mise exec -- cargo run -q -p mb-cli --`.)
- `mb serve --watch <game dir>` runs it in a desktop browser at http://127.0.0.1:8765/runtime/index.html?overlays and rebuilds every time you save: new code is swapped into the open page within about half a second (a fast build without LTO or wasm-opt; `mb build` is still the real one), changed assets are hot-swapped, and compiler errors appear over the game. It runs in the foreground, so start it in the background once and open the page once; don't reload it by hand. Use `--port` if 8765 is taken. Mouse = touch, mouse position = tilt (for a game with attitude, where the phone points), console = log, haptics and leaderboard calls. An installed `mb` has the web runtime built in; in the Maimbrain repo, run `just runtime` once first (it builds `runtime/dist`, which `cargo run -p mb-cli -- serve` then embeds automatically). If `mb serve` says it was built without the web runtime, that's what's missing. (Plain `mb serve`, without `--watch`, rebuilds when you reload the page instead.)
- **The iteration loop** with `--watch`: edit, then `curl -s http://127.0.0.1:8765/__mb/wait` (it returns once your change is built and running in the page, as JSON). `"state": "running"` means look at the page; `"failed"` carries the compiler's `errors` (file, line, message), `"error"` a page's runtime error (a trap, a panic message in `pages[].message`). The server's own output has the same as `[mb] build N ok|failed …` and `[mb] page N running|error|log …` lines. Then, in the page's console, `await mb.restore()` replays the session you had before the change into the new build up to the same frame (held if it was held) and resolves to `{ frames, firstMismatch, … }`: `firstMismatch: null` means the run went exactly as before; a number is the first frame that came out differently, expected when you changed what happens or how it's drawn. So set up a moment once (a held session, scripted taps, `mb.step`), and after each edit `wait` → `mb.restore()` → screenshot shows the same moment with your change. Add `&restore` to the URL to restore automatically after every reload. SPEC §9 has the details.
- Preview aids (SPEC §9): `?overlays` shades the card overlays and the pause pill. `?speed=0.25` slows time. In the console, `mb.suspend()`, `await mb.step(n)` and `mb.resume()` freeze a moment for a screenshot (wait ~100 ms after stepping before the screenshot). A hidden tab or headless browser runs no animation frames, so there start a held session (`await mb.restart('{"hold": true}')`) and drive it with `mb.tap(x, y)` and `await mb.step(n)`. For screenshots from a script, use `mb shot` (below) rather than driving a browser yourself. Add `DEBUG_*` constants (start level, force a shape, fake a sensor reading) to reach late states quickly, and reset them before you finish.
- **Check determinism** before you call it done: play a round (or script one in a held session), then `await mb.verify()` in the console. It replays the run frame by frame and compares draw hashes: `{ match: true }` is the goal; otherwise `firstMismatch` is the first frame that differs, and the cause is something unrecorded (randomness not from the session seed, `dt`-independent timing, hash-map iteration order, game state changed in `render`). SPEC §9 shows the manual version with `mb.restart({ hold: true })`, `mb.step` and `mb.snapshot()`.
- **Look at the game after every visual change**: `mb shot <game dir> --autoplay --sheet` (add the input that reaches the moment you changed) and Read the sheet it prints, `screenshots/shot-sheet.png`. It builds, renders the game in headless Chrome like a phone (360×640 logical, 2× pixels, touch) and saves shots at 0.5, 2, 5 and 10 s of a fresh held session, the same pixels every run; `--autoplay` taps the middle at 0.5 s, after the first shot, to get past the title card. Script play with `--at 1,3,6`, `--tap x,y@t`, `--swipe x1,y1,x2,y2@t[:secs]` (real touch events, a move every frame; one finger at a time) and `--tilt <roll°>[,<pitch°>]@t[:secs]` (degrees, not radians like `Tilt::roll()`: `--tilt 25@3`; the phone is upright until then: touches don't tilt it, as the mouse does in `mb serve`; `--tilt roll@t:0.4` turns it there over 0.4 s as a hand does, which is what to tune tilt feel against); To shoot a moment rather than a time (the boss arrives, a part breaks, the first crash), call `sys::mark("boss")` in the `update` where it happens and shoot `--at mark:boss,mark:boss+0.5`: the shot comes after the frame the game first marks it (and 0.5 s later), however long that takes (up to `--mark-wait`, 60 s), so no guessed times or debug parameters; it works in `--sheet` lists beside plain times. `--overlays` shades the feed's overlays, `--name before` keeps a set to compare against, and `--hz 120` runs the game in 1/120 s frames as a ProMotion iPhone does (shoot the same moments at both rates: if they differ, something moves per frame rather than per second; `await mb.step(n, { dt: 1 / 120 })` does the same in the console). A 4-shot run takes a few seconds. It prints what the game logged (`sys::log`, info and up) with the frame and time of each line, and `--sheet` writes them into `shot-sheet.txt` beside the sheet; `--logs` adds debug lines, `--logs-match player` keeps the lines you're after (log a node's handle from `Scene::build` at debug to learn which handle a budget warning names). Look at the sheet critically, as a player glancing at a phone: at the speed the game moves, can you tell at once where the player is, what's coming and what kills? Is there contrast between the player, the hazards and the background (not near-black on black, edges of the track or lanes visible)? Does the camera show what's coming? Is the HUD readable and clear of the player and the feed's overlays? In 3D, measure it too: call `node.mark_subject()` on the player's node (the hero, the mech, the car) when it spawns and add `--readability`. Under each shot it prints how much of the screen the subject covers, how its outline stands out from what's right around it in lightness (CIE L*, what survives in greyscale) and in color, its own tonal range, how much of it the HUD covers and the frame's lightness range, and warns when it doesn't separate (less than a third of the outline 15 L* from its surroundings, or a median under 12), is tiny (under 0.6% of the screen), is one flat tone, sits under the HUD, or the frame is dark (95% under L* 30); with the retro look on it says the look may be the cause. It also writes `shot-<t>s.readability.png`, the frame in greyscale with the subject outlined: Read it. A subject that only separates by hue disappears there. `--subject <handle>` names a node without `mark_subject`. Fix what you see and shoot again. Then cover the title, the first seconds, mid-round, danger, failure and game-over, and check them against the [engagement checklist](engagement.md#checklist).
- **Measure how it runs** instead of writing your own DevTools script: `mb measure <game dir> --autoplay` (with the same `--tap`/`--swipe`/`--tilt` as `mb shot`, `--seconds 10` by default) plays the game live in headless Chrome and prints frame intervals and long frames (the first 0.5 s, shaders compiling, on a startup line of its own; `--from` moves it), CPU ms per callback, GPU ms (the scene at a few moments drawn over and over; `--gpu-at 5,20` picks the moments), draws and triangles, input latency by kind (touch down, move, up, tilt) and `mb.verify()`'s verdict, in one short block (`--verbose` for every line); it exits non-zero if the run doesn't replay. While tuning, `mb measure <game> --autoplay --quick` (a few seconds: the GPU and draws at the `--gpu-at` moments, no live run) tells you whether a change helped; the default run's GPU ± is within one browser session, so use `--final` (three sessions, slower) for a number you'll keep or report. To judge an optimization, `mb measure <game> --autoplay --ab feed/<the build before>.mbx` times both builds in turn and prints B − A with a confidence interval (`--gpu-sessions 6` to resolve differences of a few tenths of a ms); compare builds that way, not with two separate runs, whose machine state can differ. To price one feature, read a debug parameter that switches it off (`sys::param("shafts")`) and run `mb measure <game> --autoplay --ab-param shafts=1,0`: one build, both values, B − A with its own interval. An older commit copied out of the workspace for `--ab` is built against this build's SDK (mb says so), or refused if it can't be. `--hz 120` runs 1/120 s frames on a 120 Hz timer; `--json` gives the raw frames and objects. If it prints a `pacing` line, the browser ran frames slower than asked (30 fps from a busy machine or a laptop on battery) while your game used a small part of the frame: not your game; don't tune for it (or let a quality governor react to it), rerun when the machine is quieter. Its latency numbers are headless Chrome's, good for comparing two builds, not for what a phone feels (SPEC §5.2 has the device's); `received→submit` is the part your game and the runtime own.

### 5. Publish (when the user asks)
Publishing has three steps, and the user plays the game on their own phone before anyone else sees it:

1. **`mb publish <game dir> --model "<your model name and version>"`** builds the game, uploads it to maimbrain.com and waits for the server's validation (the same validator as `mb build`). It lands as a **private draft**: only the user can see it. Tell them to open Maimbrain on their iPhone, where it's first in their feed tagged *DRAFT · ONLY YOU* and under Account → My games. If they find something to fix, fix it and `mb publish` again; the new upload replaces the draft, and it can keep the same `version`.
   **`--model` is you: identify yourself.** Give your own model's name and version exactly as you know them (like `--model "Claude Opus 5.5"`), not the user's guess, a product or tool name ("Claude Code"), or another model you think is likely. Players see it with your maker's logo under *About this game*, beside the SDK version the game was built with. If you aren't sure of your exact version, say what you are sure of ("Claude Opus"); never invent one. Only a game a person wrote without an AI model gets `--model none`. `mb publish` refuses to upload without `--model`.
2. **Describe it.** Write `games/<name>/DESCRIPTION.txt` and run **`mb describe <game dir>`**. Players see its first line under the title on the game's card (about 40 characters fit, so lead with the hook) and all of it under *About this game* and in shared links; the moderator reads it with the game. Two sentences in the game's voice: what it is, then how you play it ("A clown on a unicycle, riding the rooftops at sunset. Tip the phone to keep him upright; slide a thumb to set his speed."). Up to 300 characters and 4 lines, plain text, no links; don't repeat the title, and leave the list of inputs to the app (the About sheet shows chips for the inputs, the sensors the game needs and its leaderboards). It isn't part of the bundle, so it stays across versions and changes right away without review: run `mb describe` again only when it should change. The user can also edit it in My Game Studio, and `mb describe` replaces what they wrote, so ask before overwriting a description you didn't write.
3. **`mb submit <game dir>`** sends the draft to review, once the user says it's ready. They can also tap *Submit for review* in the app or at `maimbrain.com/create`. Don't submit on your own initiative unless the user has asked for that. A moderator plays it; once approved it's in everyone's feed. If an older version is live, it stays live until then.

`mb publish --model "…" --submit` does steps 1 and 3 at once, for when the user wants to skip testing on the phone. `maimbrain.com/create` shows each upload's status and any reviewer notes. Ids must be in your namespace (`mb whoami` shows it: `com.maimbrain.<username>`, with `_` as `-`) and `creator` must be `@<username>` (`mb new` sets both). Each submitted version needs a higher `version` than the last one submitted; drafts don't count. A rejected version's number is the exception: the build that fixes it can keep it (and its notes carry over if the new upload brings none).

**Updating a published game.** Every release is one of three kinds, and the kind is chosen when it's submitted (docs/UPDATES.md "Version levels"). The `version` you bump suggests one, so bump it sensibly:
- **Fix:** bump the last number (1.2.0 → 1.2.1). Bug fixes and polish. Players get it without being told.
- **Update:** bump the middle number (1.2.1 → 1.3.0). Something worth telling players about. Scores carry over.
- **New season:** bump the first number (1.3.0 → 2.0.0), when the user wants fresh leaderboards. Every player's standing starts over; the old season's boards stay, archived, and their top 10 become champions.

`0.x` isn't special: 0.1.0 → 0.2.0 is an update, and 0.x → 1.0.0 is a new season, so don't jump to 1.0.0 just because the game feels finished.

`mb publish` prints what the number suggests. `mb submit` (and `mb publish --submit`) uses the suggestion unless you pass `--fix`, `--update` or `--season`. Pass one when the user said what they want ("just a fix", "start fresh boards"); the number is only a suggestion, so 2.0.0 can go out as an update.

**A board that ranks differently needs a new season.** Removing a board or flipping its `order` changes how existing scores rank, so only a new season fits it (adding a board, or changing a label or format, is fine). Such a draft uploads and plays like any other; `mb publish` says only a new season fits and why, and submitting it as a fix or an update is refused.

**Never start a new season unless the user asked for one, or the board rule requires it.** It wipes every player's standing. When the board rule requires one, tell the user before submitting: which board changed, and that every player's scores start over (or offer to keep the board as it was). Only then submit with `--season`. If `mb publish --submit` prints that it was **not sent for review**, it's still a draft: tell the user why, and submit it with the command it prints once they agree.

For an update or a new season, add the version's entry to `games/<name>/NOTES.md` before you publish, newest first. It's required: players see it (the What's new card, the inbox, About this game), and the server refuses to submit an update or a new season without it.
- a `## <version>` heading;
- then a sentence or two for players about what changed for them, in the game's voice ("Ten new sewer levels. The fish finally swim the right way.");
- up to 500 characters, plain text, no links.

`mb publish` sends the entry for the manifest's version. My Game Studio shows notes from NOTES.md read-only, so when the user wants them changed, change the entry and publish again. They're frozen once submitted, so the moderator reviews what players will read.

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

## Music that follows the game

Music that never changes gets old by the second minute. A chase-runner player said it plainly: "The music is repetitive and doesn't change with the action." Make it move with the game: loops of one piece to switch between, layers that rise with intensity, a tension kit for a threat, and bus effects (`maimbrain::audio`, `maimbrain::music`, SPEC §5.6).
- **Not the same 8 bars.** Generate one piece in sections, in one request, so it keeps one key, tempo and bar grid (docs/GENERATE.md "Music in sections"): `mb music <game> run "<the game's sound>" --zones "lantern town: …; bamboo grove: …" --variations 2 --bpm 120 --layers base,hi,tension` writes the take (`run_take_base` + `run_take_hi`), its **music graph** `run.music.json`, a stinger into each zone, the swell and the tension kit. `--variations 4` alone makes one zone that moves through four themes; `--sections a,b,bridge` a form. The graph is how shipped games do it: each section is a segment with its own pickup (pre-entry) and tail (post-exit) from the take; changes happen at exit cues (phrase ends); and jump links (the Infinite Jukebox) join bar lines where the music before and after sounds alike, so a zone plays on without a loop point. Read the log: per section its links and threshold, each pair of zones (seamless links, or a stinger), each zone's way back (a link, or a restart under the swell), and what to listen to; a section the take got wrong (tempo or key too far off) is named, and mb asks once more by default. `--loops` also cuts the old self-repeating loops (for `Director::new`).
- **A different song per zone, when they should differ.** One piece in one key suits places the player moves through. When each zone (each boss, each arena) should have its own song in its own genre, matching its personality, give each its own prompt: `mb music <game> brawl "<what they share>" --zone harbor="bluesy hard rock, 128 bpm" --zone neon="flamenco-tango, 112 bpm"`. Each is its own piece (own key and tempo, own graph, stinger and tension kit, still reacting to the fight), listed in `brawl.songs.json`. A run adds to that list (a zone made again replaces its entry), so one `--zone` at a time works, and a zone that fails (a content filter, a take no loop could be cut from) is reported with the command to make it again while the others still generate. `music::Songs` plays them: a `Director` per song, and `songs.go(next, Change::Auto, now)` changes at the playing song's next exit cue under the next song's stinger, with no crossfade between keys (docs/GENERATE.md "A different song per zone").
- **Listen before you wire it up.** `mb music <game> run --audition` puts every jump, restart and change into one file (`art/raw/run-audition.ogg`), spliced as the Director splices, a soft click before each and the timeline (with each link's score) in the log: about six minutes for four zones of two sections. A jump or change you can hear is a reason to try another take (`mb regen … --new-seed`, $0.08); every 1 on the wrong beat is `--downbeat 1` (or `-1`), free.
- **`music::Director` plays it**: the take straight through with a jump now and then (seeded, so replays hear the same music), changes of zone at the next good point (a seamless link, the take's own transition, or the next exit cue under the stinger, at most a bar later than it could), the base and high layers together, the tension kit in step with the take's phrase, and the tension from game state. It writes a `mix_check.py` log in tests (`Director::with_graph(graph, sounds, seed, Log::default())`). `sdk/maimbrain/examples/chase_music.rs` is a whole chase (three zones, two near-catches, an escape):
  ```rust
  use maimbrain::music::{Change, Director, Graph, Mood};
  // Load assets/run.music.json and every file it names (graph.files()) as Sounds, then:
  let graph = Graph::parse(&graph_asset.read().unwrap()).unwrap();
  let mut d = Director::from_graph(graph, |path| sounds.get(path).copied(), sys::rand_seed()).unwrap();
  let (town, grove) = (d.zone("lantern_town").unwrap(), d.zone("bamboo_grove").unwrap());
  d.start(town, sys::time());
  // Each frame (danger 0–1: e.g. 1 − heron_gap / 8 m):
  d.update(sys::time(), dt, Mood { intensity: speed_k, danger });
  // Into the grove: at the next good point.
  d.go(grove, Change::Auto, sys::time());
  // The heron strikes and misses:
  d.near_miss(sys::time());
  ```
  `Change::Auto` (the default) waits for an exit cue, at most a bar past the first bar line it could use (`d.wander.wait_bars`), unless a seamless link or the take gets there first; `Change::NextBar` doesn't wait; `Change::Stinger` always covers; `Change::Crossfade(bars)` lets the old music die away over that many bars without a cover. Ask for the change as the player crosses into the zone; it lands within about two bars (4 s at 120 BPM). How often it jumps is `d.wander`, the levels `d.levels`. `d.moves()` lists every jump and change it made. When the round ends (or the game pauses into a menu), `d.stop(sys::time(), 1.0)` lets every voice it started die away over that many seconds (the take, the tension loops, any stinger, heartbeat or sting still sounding or scheduled), opens the music bus again, and `update`/`go`/`near_miss` do nothing until `d.restart(zone, sys::time())`, which starts that zone from its entry as `start` does. `restart` (or a second `start`) while it's still playing lets the old voices go first (over `music::RESTART_FADE`), so there are never two takes at once. With `--loops`, `Director::new(bpm, bars)` plays the loops as before: `add` each, `stinger`, `swell`, `rotate` a zone's variations every few bars, and the same `go`, `update` and tension.
- **Layers.** `mb music --layers` writes `<name>_base.ogg` and `<name>_hi.ogg` beside the full mix (base + high = the full mix, one shared gain). Without the Director: start both with `play_looped_at` and the same `at` (they stay sample-aligned), `.bus(Bus::Music)` on each, the high layer low, and raise its vol with intensity (speed, danger, combo) through `Voice::set`. Smooth intensity yourself over ~0.5–1 s; `set` only glides ~10 ms.
- **Dread as a threat closes in** (a chaser, a closing clock, rising water). When the fish is almost caught, the music should make you anxious. `mb music … --layers base,hi,tension` adds a tension kit made for the loop: `<name>_tension` (a drone on the root and fifth, an eighth-note pulse, a heartbeat on each bar's 1, string tremolo on two notes a semitone apart that swells over each phrase, a riser into the next phrase) and `<name>_tick` (a tick-tock clock), both exactly the loop's length, on its beat and in its measured key; plus two one-shots in that key, `<name>_heartbeat` and `<name>_sting`. They're synthesized to fit, not generated: Lyria can't make a stem that lines up with another take. A track you already have gets them for free from its take: `mb regen <game>/assets/<name>.ogg --add-layers tension`. The Director does what follows; by hand, start the loops with the base's `at`, `.bus(Bus::Music)`, at vol 0, and drive everything from one smoothed `danger` 0–1, smoothed faster on the way down than up so an escape is heard at once:
  ```rust
  let k = if raw > self.danger { dt / 0.6 } else { dt / 0.25 };
  self.danger += (raw - self.danger) * k.min(1.0);
  let d = self.danger;
  tension.set(1.4 * smoothstep(0.05, 0.7, d), 0.0, 1.0);  // in at the first sign of danger; level with the track at full
  tick.set(smoothstep(0.55, 0.95, d), 0.0, 1.0);          // the clock only when it's close: tempo feel
  hi.set(0.55 + 0.25 * smoothstep(0.0, 0.6, d) + self.relief, 0.0, 1.0);
  // The world narrows a little: 20 kHz → 2.5 kHz at full danger (not under water: it's still music).
  Bus::Music.lowpass(OPEN * (2500.0 / OPEN).powf(smoothstep(0.4, 1.0, d)), 0.707, 0.1);
  ```
  (`maimbrain::music::smoothstep(a, b, x)`: 0 below a, 1 above b, eased between.) The tension file sits 3 dB under the track, so vol 1.4 at full danger brings it level: with the clock and the heartbeat that lifts the music 3–5 dB on a phone, a change nobody misses. At vol 1.0 it was 2–2.6 dB over a real take, easy to miss on a phone. A loud, bright take needs more: one at −22 LUFS short-term lifted only +0.3 dB at these levels. So `mb music` measures the take on a phone and writes the kit's level into the music graph (`levels.tension_gain`: the factor on the tension layer, the clock and the heartbeat that lifts a typical 3 s of the take by +4.5 dB at full danger, never under 1, capped where the layer reaches vol 4), and `Director::from_graph` plays at `Levels::for_graph(&graph)`; a graph made before it gets the defaults. Playing by hand, multiply the vols below by it. Setting `d.levels` yourself replaces it.
  - **A heartbeat that speeds up, on the grid.** The sidecar of any of the files has `bpm` and `downbeat_seconds`: beat n is heard at `at + downbeat_seconds + n * 60 / bpm`. Each frame, `play_at` the heartbeat for the next beat ~0.1 s ahead when it's due: on beat 3 from danger 0.35 (with the loop's own on beat 1, twice a bar), on every beat from 0.7, never closer than ~0.33 s apart (a racing heart, not a drum roll; at 160 BPM keep it on 1 and 3). Vol 0.15–0.3 with the danger, ±1.5 dB and ±1 % pitch per beat. Under 0.15 it's lost on a phone while the danger hovers mid-way.
  - **Tempo feel from the clock and the pulse**, never by pitching the music up: `pitch` changes the loop's speed, so it slides off the grid, off the other stems and out of key.
  - **A near-catch**: `sting.play(0.7, 0.0, 1.0)` at that moment (it's an event, not a beat), and dip the music under it: `Bus::Music.volume(0.6, 0.02)`, back to 1.0 over ~0.3 s half a second later.
  - **The escape is the payoff**: the danger falls fast (above), the low-pass opens over ~0.8 s, the clock stops first, and the high layer swells for two bars (`self.relief` up to ~0.25 and back): relief has to be heard as clearly as the threat.
  - Check it with `mix_check.py` on a bot round that reaches full danger: it treats `*_tension` and `*_tick` (and a `*_heartbeat` over them) as music that's meant to rise. It takes the music's typical level and key from the track's own layers, and reports how much the tension lifts the music on a phone at its peak (it wants at least +3 dB) and at its quietest (at most +1 dB: it has to let go). It checks that the heartbeat's louder plays are heard on a phone, without the caps on frequent effects. Bus effects aren't in it, so keep the low-pass a narrowing, not a muffle.
- **Bus effects.** Every voice starts on `Bus::Sfx`; `Voice::bus` moves it to `Bus::Music` or `Bus::Master` (both feed Master). Each bus has `lowpass`, `highpass`, `reverb`, `delay` and `volume`, and every call takes a `ramp` in seconds, so calling them every frame from game state is fine. Recipes:
  - under water: `Bus::Music.lowpass(500.0, 1.0, 0.3)`, and `lowpass(OPEN, 0.707, 0.6)` on surfacing;
  - speed: open the music's lowpass from ~1200 Hz at the start toward `OPEN` at top speed, mapping speed to cutoff exponentially (`1200.0 * (OPEN / 1200.0).powf(k)`), so the track "opens up" as the run builds;
  - a tunnel or cave: `Bus::Master.reverb(Reverb::CAVE, 0.4)`, then `Reverb::OFF` on the way out (animate `wet`, not `decay`: a new decay builds a new room, at most 5 times a second);
  - a pause or results screen: `Bus::Music.volume(0.4, 0.5)`; a big hit: dip it to ~0.6 for under a second.
- Effects are output only (never read back, not in the draw hash), so they can't break replays, but they also aren't in `mix_check.py`: its music rules apply to the open, dry mix. Phone speakers give back little under ~200 Hz, so a lowpass below ~300 Hz leaves almost nothing; keep a muffle a moment, not a level. `mb.audio()` in the preview shows each voice's bus and each bus's settings.

## Foley: sound from motion

Anything that walks, turns, flies or crashes makes sound by itself when its motion drives `maimbrain::foley` (assets.md §5 "Every moving thing makes sound"). One `Foley` per game places every sound from the camera (`Space`: full level within 4 m, −6 dB per doubling, silent past 60 m; panned up to 0.6 to its side), varies each play (±1.5 dB, ±4 % pitch, from its own seed, so replays match), folds a burst of one sound from one place into one, and keeps at most `max_voices` (6) one-shots, the loudest heard winning. Per moving thing:

```rust
use maimbrain::foley::{Contact, Drive, Foley, Gait, Listener};
let mut foley = Foley::new(sys::rand_seed());
let (mut gait, mut feet) = (Gait::biped(), Contact::new());          // Gait::quadruped(), Gait::new(&[…])
let (mut servo, mut jets) = (Drive::servo(s.servo), Drive::thruster(s.thruster));
// Each frame, after moving things:
foley.update(dt, Listener::from_camera(&cam));
for _foot in gait.update(mech.stride_phase) { foley.step(s.stomp, mech.pos, run_k); }  // 0.3 walk … 1 run
servo.update(&mut foley, turn_rate.abs() + arm_speed, mech.pos, dt);   // silent ≤ 0.15, full at 3
jets.update(&mut foley, thrust, mech.pos, dt);                        // 0–1
if let Some(v) = feet.update(mech.height, dt) { foley.impact(s.stomp, mech.pos, v, 12.0); }
// A punch lands; an arm comes off:
foley.impact(s.impact, at, hit_speed, 20.0);
foley.hit(s.hiss, at, 0.4, 1.0, 0.6);
foley.hit(s.tear, at, 0.8, 1.0, 1.0);
```

`Gait` steps each foot at its offset in the stride (the phase the legs are animated with, in cycles), however long the frame; `Contact` lands a foot or a body from its height with hysteresis and gives the speed it came down at. `Drive` is a loop whose level and pitch follow a speed or a thrust over `idle`…`full`, with an attack and a release that are the same at any frame rate; it starts itself and stops when still. `Foley::impact` is as loud and as low as the hit was hard. The sounds: `mb sfx <game> --pack mech` (generated) or `sfx.py --pack mech` (procedural, free). `sdk/maimbrain/examples/mech_foley.rs` writes a `mix_check.py` log of two mechs (walk, turn, hop, land, punches, a torn arm) through `foley::Log`; mb3d-demo's retro scene (`--param scene=retro`) plays it live.

## Rhythm games

Anything judged against music (rhythm games, beat-synced dodgers, a jingle the player taps along to) follows the recipe in SPEC §5.6 "Rhythm games". In short: the beat grid is game time; music is scheduled on it with `Sound::play_looped_at(…, origin)`; presses are judged with `Event::time`; the grid is re-anchored when `sys::time_lost() > 0` and after a resume; a small self-calibration absorbs the rest. `games/summoned` (`src/rhythm.rs`, `src/sound.rs`, the `time_lost`/`resumed` handling in `src/sim.rs`) is a complete worked example. What it learned:
- **Never anchor the grid to "the frame that started the music".** `play` starts whenever the sound finishes decoding and the output is ready. Put beat 0 ~0.12 s in the future (`origin = sys::time() + 0.12`) and schedule every stem with that same `at`; if a stem's asset arrives late, call `play_looped_at` with the *original* `at` when it does, and it joins on the grid.
- **Make each music loop exactly a whole number of phrases** (sfx.py's `Song.stems` + `save_stems` do), so the loop's wrap lands on the grid and later phrases need no scheduling at all. Run `check_audio.py` for end padding and clicks at the seam.
- **Judge with the event's own time**, not `sys::time()`: the host gives touches their own game time within the frame (SPEC §5.2), so judging with the frame's time throws away ±½ frame of accuracy and lets frame pacing leak into judgements. Apply a frame's presses before you auto-miss notes in that update, and clamp each press to `[time before this update, sys::time()]`. If your sim keeps its own clock (from 0 each round), convert: `Event::time` and `play_at`'s `at` are host game time (`sys::time()`), so shift by `sys::time() − sim.t` both ways. Summoned 0.1.0 didn't, and every press silently clamped to its frame's end.
- **Re-anchor, don't patch.** When `sys::time_lost() > 0` (a hitch the `dt` clamp cut short) the music is ahead of the grid: at the next phrase boundary, schedule the music again at a fresh `origin` and move the grid there. After `resume`, restart the current phrase the same way so a returning player gets a run-up. Never guess from `dt` (e.g. "dt ≥ 0.095 means a clamp"): `time_lost` is exact and recorded.
- **Calibrate a little, not a lot.** The host already compensates audio output latency (Bluetooth included) and when frames are shown. Learn the rest from the median of the first ~3–8 judged presses (ignore offsets beyond ±0.2 s), clamp it to about −60…+100 ms, save it as next run's prior, and keep the first encounter's windows wide (×1.4) until it settles.
- **Model the bot's presses with their own times** (planned time, clamped into the frame), with a lateness bias and noise per preset, and test that a steady late player gets calibrated into Perfects.
- **Everything must read without sound.** Many players have the ringer off: markers, rings closing on the beat and on-beat pulses carry the timing; the music confirms it.
- **Audio isn't in the draw hash**, so `mb.verify()` can't see a desync. In the preview, `mb.audio()` lists each voice's `at` and the AudioContext time it's heard from: stems must share one `when`, and a late voice must show an `offset` matching its lateness.

## 3D (mb3d)

**The hero is the most-looked-at thing on screen.** Whatever the player controls or chases sits mid-frame for the whole game, so it sets the game's quality bar. Never ship a hero or main creature assembled from primitives (spheres, cones, boxes) or flat-shaded blobs: a stranger reads it as a placeholder in one glance. **Generate anything with a recognizable silhouette**: the hero, the threat or chaser, creatures beyond a simple fish, characters, vehicles and boats, signature props (`mb model … --animate …`, a rigged and animated .glb; ~$0.40–0.85 each). Code-built meshes (`MeshData::loft` + `subdivide`, bones with `Skeleton` and `Swim`; `MeshData::fish` is a worked example) are for simple organic shapes, geometry the rules depend on, and things repeated by the hundred (rocks, walls, reeds, lily pads, terrain); a dragon, a heron or a boat built from lofts reads as a placeholder. Spending $3–6 on the hero, the threat and a few key models is the cheapest quality upgrade a 3D game has, so don't economize there. Judge every generated model where it lives: at its in-game size, in the game's lighting, against the water or ground it sits on, and as a black silhouette. A boat that reads as a boat in the studio preview can vanish into a dark canal; fix it with a clearer silhouette, a brighter material or rim light, and a prompt that names the shapes that make it read. Give it a silhouette you'd recognize in black, and make it move like a living thing (bend, sway, breathe). If an existing game's hero is primitive-built, replacing it is usually the biggest visual win available: do it before polishing anything else.

Worked examples: `games/mistwood` (mb3d 2: animated character, fog, toon, instancing) and `games/mb3d-demo` (incl. the moonlit canal scene: fog, reflective water, sky v2, and a leaping koi whose wake, splashes and drifting petals make the water react; and the koi pond: organic meshes built in code, swimming on bones). In the kit they are `examples/mistwood` and `examples/mb3d-demo`; the same goes for every `games/<name>` these docs mention that the kit ships (`examples/gallery`). Games only in the Maimbrain repo (starfall, summoned, topple, knockdown) are named for reference.

Start from **[template-3d/](template-3d/)** (`mb new --3d games/<name>`): a small complete 3D game (catch the drone) that builds, validates and passes its tests as is. Read its files before writing your own:
- `src/scene.rs` builds the scene once: the procedural sky (also the image-based lighting), a shadow-casting sun, post (bloom, vignette), meshes made in code (a lathed platform, pillars merged into one mesh, a faceted drone with a glowing ring as a child node), one particle emitter, one trail and two point lights (one on the drone, one flash that decays).
- `src/sim.rs` holds the rules *and* the camera, with tests: the tap test projects the drone with `Camera::project`, the same formula the host uses, so hit-testing is plain tested Rust. The camera follows the drone with a critically damped spring, and a test checks it moves smoothly at 60 and 120 Hz (below).
- `src/control.rs` turns touches into actions, shared by `lib.rs` and the bot (`src/bot.rs`, see "Write a bot").
- `src/lib.rs` is the host layer: rounds, input, moving nodes, and the HUD drawn with mb2d at `gfx3d::project` positions (a reticle on the drone, a hint under it).

The API is SPEC §5.4; the SDK binding is `maimbrain::gfx3d`.

- **Manifest**: `stdlib = { mb3d = 2, mb2d = 2 }` (mb2d for the HUD; 2 for game fonts, which 3D text can use too) and `perf_tier = "full"`. mb3d 2 (the template's default) adds animated characters, instancing, freeing, fog, toon shading and outlines, 3D text and depth of field (below); a game declaring `mb3d = 1` gets v1 unchanged.
- **Shape of a 3D game**: build meshes, materials, emitters, trails and nodes in `init` (or when assets arrive), keep their handles in your state, update node transforms in `update`, and in `render` call `gfx3d::camera(..)`, `gfx3d::render()`, then any mb2d HUD. The scene persists; don't recreate it every frame.
- **Build once, pool; free what you're done with.** Meshes, textures, materials, models, emitters and trails count against the limits (512 meshes, 1024 materials, 64 textures, 64 models, 256 emitters, 64 trails; SPEC §5.4). Build them once and pool: one emitter per particle look fired wherever it's needed, one material per look (`Material::set` to pulse it), a fixed pool of enemy nodes hidden with `set_visible(false)` and reused, trails moved between nodes with `detach()` + `attach(node)` (that starts a fresh ribbon; the old one fades out where it was). `Node::destroy` frees a node and its children; with mb3d 2, `free()` on a mesh, material, texture, emitter, trail or model releases it (a level's props when the next level loads; don't create and free things every few frames).
- **Point lights**: 8 slots. A light keeps its slot from its first `point_light` call until its node is destroyed; intensity 0 darkens it but keeps the slot, and hiding the node doesn't switch it off. Keep one or two flash lights and move them.
- **Units and colors**: meters, +y up, the camera looks down −z (`Camera::looking_at`, `look_at`, `OrbitCamera`; to follow something, `SpringCamera` or `motion::Follow`, below). Colors are linear: write `srgb(0xe63946)` / `srgba(0xe63946ff)` for picked colors. Values above 1 glow: an emissive of `[0.4, 1.6, 4.0]` × `emissive_strength` 3 blooms; use `Material::UNLIT` for pure light sources (engine glows, neon).

### Performance budgets (60 fps on an iPhone 13 class phone)
`mb shot` and `mb measure` warn when a frame goes over the draws, triangles, skinned or particles budget below (counting the reflection's draws and triangles, and water v2's draws under the water), and name the nodes that cost the most, with a hint when an instanced node spans so much of the level that culling can't skip it; the preview's `?stats` readout turns red. `mb.stats().top` lists the costliest nodes of the last frame.

| Thing | Budget | Notes |
|---|---|---|
| Drawn meshes | ≤ 200 per frame | Each node with a mesh is a draw. Merge static scenery into one mesh (`MeshData::merge`, `transformed`). |
| Triangles | ≤ 150 k on screen | A 48×32 UV sphere is 3 k; a faceted ship ~1 k. |
| Live particles | a few thousand (the tools warn past 5000) | 16 k is the hard cap. Big soft particles cost fill rate; prefer more small sparks over many huge smoke puffs. |
| Point lights | ≤ 8, keep 3–4 | Every lit pixel loops over them. |
| Sun shadows | one 2048² map | Keep `shadow_distance` tight around the action (15–30 m) for sharp shadows. |
| Sky | set once | `gfx3d::sky`, the atmosphere and the clouds' look re-bake the lighting over the next frames. Never change them per frame (moving the sun is fine; clouds drift on their own). Clouds cost ~0.5 ms on an A15 over half the screen; a whole night sky (stars, clouds, moon) ~1.8 ms, most of it the stars. |
| Textures | ≤ 1024² mostly | 64 textures and 96 MB in all; a 2048² texture takes 22 MB, a generated model's three 1024² maps 17 MB. Over it, `Texture::new`/`Model::load` stay None and the page logs `runtime: <asset> refused (−4): …`; `mb build` prints the bundle's estimate (all at once, or the biggest scene's when `art/budget.toml` lists what each scene loads: `[load] always = ["p_*.png"]`, `harbor = ["sky_harbor.jpg", "bd_harbor_*.png"]`), `mb shot --stats` and `mb measure` the use. |
| glTF loading | one model per frame, in `update` | Parsing happens inside your callback (watchdogs, SPEC §3). |
| Instances (mb3d 2) | thousands; count instances × triangles in the triangle budget | One instanced node is one draw. Re-send a list only when it changes (coins that spin: fine; a static forest: once). Split big fields into chunk nodes so culling works. |
| Skinned characters | a handful (or a school of ~5 code-built fish); the tools warn past 16 skinned draws | ≤ 64 joints per skin, 128 skinned draws per frame max. Each skinned mesh is a skinned draw: a koi with translucent fins is two. |
| Building organic meshes | in `init`, once | A `MeshData::fish` is ~2 ms natively (lofts, smooth normals, weights); the koi pond's whole scene took 35 ms in one frame in wasm on a desktop. Build creatures in `init` (or a few per frame), never per frame. |
| Outlines | characters, props, big shapes | An outlined draw draws twice. |
| Depth of field | on when it matters | ~0.3–0.6 ms GPU (estimate); `blur: 0` turns it off. |
| Reflector | one water plane | Everything it reflects is drawn again (`mb.stats()` `reflected`), at half resolution: ~0.6–1 ms on an A15 for a canal scene. Set `max_distance` to where fog hides things; free when no reflective surface is on screen. |
| Ripples and wakes | a wake or two, rings for events | 16 rings, 4 wakes. They cost only on reflective pixels they reach (~0.3–0.5 ms for the canal's koi); calm water costs nothing. |
| Water v2 (`gfx3d::water`) | one look per scene | Per water pixel: a few texture reads and a loop over the lights. Seeing into the water or contact foam (`depth_fade`, `foam`) draws what reaches below the surface again at the reflection's resolution (`mb.stats()` `under`). Measured on an M-series Mac: +0.2–0.5 ms GPU for the canal, +0.9 ms over a plain mirror for the koi pond (water over 60% of the screen, half of it the second view of the bowl and five skinned koi); roughly 4× that on an A15. Cheaper: the reflector's `resolution` 0.35 (both views shrink), `depth_fade` and `foam` 0 (no second view). |
| Beauty pass | grade + grain always fine; shafts or streaks where they show | Measured on the canal (desktop GPU): grade and halos too small to tell from noise, shafts +0.4 ms, streaks +0.3 ms, the whole pass +0.5 ms: on an A15 roughly 2 ms for all of it, 1.5 ms of that shafts and streaks. Turn shafts off (`intensity` 0) when the light is out of view; streaks are the first thing to drop. |
| 3D text | a few dozen labels | Each text node is a draw; recoloring is cheap, changing the words relays it out. |

Check these with `mb.stats()` in the preview: draws, triangles, instances, skinned draws, outlines, text, shadow casters, reflected draws, particles, and CPU ms per callback.

Hide nodes you'll reuse (`set_visible(false)`) instead of destroying and recreating them every few frames. Changing a material (`Material::set`) is cheap: pulse emissives freely.

### Meshes in code
The SDK's `MeshData` builders cover most game shapes: `cube`, `cuboid`, `plane`, `uv_sphere`, `icosphere`, `cylinder`, `cone`, `frustum`, `torus`, `lathe` (revolve a profile: bottles, rockets, fuselages), `extrude` (a 2D polygon along z: wings, fins, logos, level pieces). Then:
- `.flat_shaded()` for the faceted low-poly look (every triangle gets its face normal);
- `.with_vertex_colors(0xRRGGBBAA)` or `.with_vertex_colors_by(|pos, normal| ..)` to color without textures (one material, many colors);
- `.transformed(&Transform::at(..).with_rot(..).matrix())` and `.merge(&other)` to assemble parts (mirroring flips winding back for you);
- `.upload()` → a `Mesh`.

```rust
// A faceted rock: a jittered icosphere, flat shaded, tinted by height.
let mut m = MeshData::icosphere(1.0, 1);
for (i, p) in m.positions.iter_mut().enumerate() {
    *p = *p * (0.8 + 0.4 * (i as f32 * 12.9898).sin().abs());
}
let rock = m.flat_shaded().with_vertex_colors_by(|p, _| if p.y > 0.3 { 0xb8b0a8ff } else { 0x6b625cff }).upload().unwrap();
```
Keep parts that need different materials as separate meshes on child nodes of one parent (the template's drone: a body node and a ring node under one parent you move). Front faces wind counter-clockwise; `cargo test` the builder code like any other logic (the SDK compiles on the host, and mb3d constructors return handle 1 there).

**Organic shapes** (creatures, tentacles, leaves, hulls, cloth). Primitives merged together look like primitives merged together: faceted and stiff. For anything alive, sweep a cross-section along a smooth spine and smooth it:
- `MeshData::loft(&Loft { spine, rings, sides, closed, caps, ends, crease, up }, |t, u| section)`: a section swept along a Catmull-Rom spine through `spine`. The closure gives the section's point at `t` (0–1 along the spine) and `u` (0–1 across it) as `vec3(side, up, along)`: change its size, shape or offset per `t` (a body that swells and tapers, a tail whose trailing edge is forked by a negative `along`). `closed` makes a tube (caps optional; a section shrinking to 0 closes itself), open a strip (a fin, a leaf, a banner: draw it with `Material::DOUBLE_SIDED`, or add `.double_sided()`). Counter-clockwise sections face out. `Spline` is the same curve on its own (paths, cameras).
- `.smooth_normals(crease)`: smooth shading across edges flatter than `crease` radians, hard across sharper ones (UV seams and touching parts shade as one).
- `.subdivide(levels, crease)`: Loop subdivision, so a coarse cage (a jittered icosphere, boxes stacked into a lantern) becomes a smooth surface; edges sharper than `crease` stay crisp. 1–2 levels (×4 triangles each).
- `MeshData::fish(&FishDesc { length, height, width, fins, bones, .. })` returns `FishMesh { body, fins, eyes }`: a koi from lofts, facing −z, ready to swim (below). It's also the recipe to copy for other creatures (`sdk/maimbrain/src/gfx3d/organic.rs`).

```rust
// A koi: a white body with red patches from noise in vertex colors, translucent fins.
let desc = FishDesc { fins: 2.0, ..Default::default() };          // butterfly fins; 8 bones
let fish = MeshData::fish(&desc);
let mut body = fish.body.with_vertex_colors_by(|p, n| if n.y > -0.3 && (p.z * 11.0).sin() + (p.x * 9.0 + p.y * 7.0).cos() > 0.6 { 0xe0401cff } else { 0xf6f2eaff });
body.merge(&fish.eyes.with_vertex_colors(0x0a0a0cff));
let koi = Node::with_mesh(body.upload().unwrap(), skin).unwrap();            // PBR, roughness ~0.3, rim 0.35
let fins = Node::with_mesh(fish.fins.upload().unwrap(), fin_mat).unwrap();   // Material::ALPHA | DOUBLE_SIDED
fins.set_parent(Some(koi));
```
Color creatures by position (`with_vertex_colors_by`): a creature needs ~1–3 k vertices for patterns to read; patches on the back and sides, a pale belly. `games/mb3d-demo/src/koi.rs` is the worked example (five koi varieties, koinobori streamers, a stone lantern from a subdivided cage).

### Making a .glb with a script
For textured or hand-shaped models, generate a glTF binary from a script in `tools/` so the game stays remixable. `mb new --3d` puts a copy of **[scripts/make_glb.py](scripts/make_glb.py)** (stdlib Python only) in your game's `tools/`: change it there; `python3 tools/make_glb.py assets/model.glb` writes a textured cube "relic" as it stands. It works like this:
1. Build the geometry as lists (positions, normals, UVs, u16 indices).
2. Paint textures procedurally from one height field: base color, a normal map from its slopes (OpenGL/glTF convention: green = up), a metal-rough map (roughness in G, metallic in B), an emissive map. Write PNGs with `zlib`; convert the base color to JPEG with `sips` on macOS (smaller).
3. Write the `.glb`: a JSON chunk (asset, scene, nodes, meshes, materials, textures, images, buffers, bufferViews, accessors) and a BIN chunk, each 4-byte aligned.
4. Optionally compress: `npx gltfpack -i raw.glb -o assets/model.glb -c` quantizes and meshopt-compresses the mesh (supported), keeping PNG/JPEG textures. Don't use `-tc` (KTX2 isn't supported yet).

Load it with `Model::load(asset)` once `asset.state()` is `Ready`, then `model.spawn(parent)` returns a root node to move. Use `KHR_materials_emissive_strength` for glowing parts; unsupported required extensions (Draco, KTX2, WebP) fail to load.

### Generated models (`mb model`)
For creatures, characters and detailed props, generate the model rather than assembling primitives (assets.md has the when): `mb model games/<name> koi "a stylized koi, orange and white, glowing fins" --animate swim --size 0.7 --glow 2.5` writes `assets/koi.glb`, already fitted to `--polys` triangles, `--size` meters on its longest side, facing −z (the way the camera looks and runners run, so no turn in a runner), standing on y = 0 (centered for swimmers), with ≤ 1024² textures (the default; keep it for the hero and anything the camera gets close to: a generated model's detail is its texture, and at 256² a mech filling a third of the screen is a smear; 512 for mid-size things, 256 for small props or crowds; changing it later is free and offline: `mb model games/<name> <asset> --reprocess --texture-size 512`), and clips named `Swim`, `Run`, `Idle`…. Load it like any `.glb` (`Model::load`, `model.spawn`) and play its clips with `Animator` (next section): `anim.play(model.clip("Swim").unwrap(), Play::Loop, 0.0)`. Clips are in place: move the root node yourself. Look at `art/preview/<name>.png` first, then `mb shot`. Needs a Tripo or Meshy key (`mb models`); docs/GENERATE.md "3D models". Parts that attach to something (a mech's guns, blades, armor plates) take `--kind part` (no character style words, which turn guns into little robots), `--orient tip=+y|tip=+z up=+y|face=+z` (the pose the socket expects) and `--glow-color '#rrggbb'` for a glowing weak spot; `mb model info <glb>` prints a part's tip, face, glow spot and socket offset (assets.md has the rest).

### Animated characters (mb3d 2)
A rigged glTF plays its clips through `gfx3d::Animator`. Bones and sockets are nodes you find by name, so holding a sword (or hanging a lantern) is parenting. **You own animation time**: the animator moves only when you `update(dt)` it, and the host samples clips at exactly the times you pass, so replays pose identically. Make the model with **[scripts/make_rigged_glb.py](scripts/make_rigged_glb.py)** (stdlib Python: a skeleton, a skinned mesh with per-vertex weights, a morph target and four clips; `games/mistwood` uses it as is), or any glTF with skins (≤ 64 joints) and animations.

```rust
// When the asset is ready (in update):
let model = Model::load(asset).unwrap();
let fox = model.spawn(None).unwrap();                          // the root: move and turn this
let (idle, run, jump) = (model.clip("Idle").unwrap(), model.clip("Run").unwrap(), model.clip("Jump").unwrap());
lantern.set_parent(fox.find("Socket.Tail"));                     // attach to a bone or socket
fox.find("Fox").and_then(|n| n.material()).map(|m| m.set_style(&MaterialStyle { toon_bands: 3, outline_width: 1.8, rim: 0.5, ..Default::default() }));
sys::log(sys::Level::Info, &format!("fox nodes: {:?}", model.node_names()));  // what find() can find: bones, meshes
model.tint([1.5, 1.3, 1.1]);   // lighter and warmer, maps kept (every spawn; Material::tint for one material)
model.brighten(0.3);           // glows with 0.3 of its own colors: reads on a dark screen without regenerating
let mut anim = Animator::new();
anim.play(idle, Play::Loop, 0.0);
// Every frame:
if jumped { anim.restart(jump, Play::Once, 0.06) } else if anim.finished() || running { anim.play(run, Play::Loop, 0.15) }
anim.set_speed(ground_speed / 8.0);                             // feet match the ground
anim.update(dt);
anim.apply(fox);
let hand = fox.find("Hand.R").and_then(|n| n.world());          // where a bone is, for hit tests
```
**Clips on some bones, and code on top of clips:** a clip normally moves every bone it has, and overrides a bone you set by hand. Layer instead:
```rust
let upper = Mask::below(model, mech, mech.find("Spine01").unwrap(), 1.0);   // the bone and everything under it
let legs = Mask::below(model, mech, mech.find("Pelvis").unwrap(), 1.0);
walk.apply_masked(mech, &legs);                  // two Animators, each on its own bones
punch.apply_masked(mech, &upper);
bones.add_turn(chest, Quat::from_rotation_x(-0.3 * recoil));   // code on top of the clip pose (after apply)
bones.add_clip(mech, idle, t, 0.4, Some(&upper));              // or a clip at 40 % on top of a pose set in code
```
**Posing a rig in code** (a generated mech or character rigged by Tripo, or any Mixamo/Blender rig): `let rig = Humanoid::find(model, mech)?` right after spawning names its bones `hips`, `spine`, `chest`, `neck`, `head`, `upper_arm_l`, `forearm_r`, `hand_r`, `thigh_l`, `shin_l`, `foot_l`… whatever the rig called them, and `rig.turn("chest", Quat::from_rotation_y(-0.35))` turns a bone **in the model's frame** (+y up, the model's forward), so a pose you key by hand means the same on every rig. Never key poses with `Skeleton::turn` on a generated rig: its bone axes point anywhere. Feet and hands go where you want with IK, last in the frame: `let legs = [TwoBoneIk::leg(&rig, 'l')?, TwoBoneIk::leg(&rig, 'r')?]` then `legs[0].apply(&rig.skeleton, foot_world, body_forward)` (knees bend toward the pole direction, feet flat); a head or turret tracks with `LookAt` (`update(dt, &rig.skeleton, Some(player))`, `apply`), limited and rate-limited. Key attack beats as poses on the sim's own clock, never wall time: `PoseKeys::new(guard).to(0.7, windup, Ease::CubicInOut).hold(0.3).trembling(1.0)` sampled at `tell_t / tell` (the tremble in the held pose says "now"), a fast ease-in strike, a held follow-through, a slow recovery; put weight in it with `motion::kick(t - hit_time, 16.0, 0.3)` on the chest (a strike's stop, a hit's recoil, a landing's squash): a pure function of the time since, so replays match. A mech (or any heavy biped) walks with `MechGait`: no mocap walk clip strafes, and a walk clip on a body that slides sideways moonwalks.
```rust
let mut gait = MechGait::from_rig(&rig, body)?;            // after spawning
for s in gait.update(dt, pos, yaw, vel) { dust(s.pos); shake.add(0.3 * s.strength); foley.step(s.pos, s.strength); }
body.set_transform(&Transform::at(pos + gait.body_offset()).with_rot(Quat::from_rotation_y(yaw) * gait.body_tilt()));
gait.apply(&rig, &legs, Quat::from_rotation_y(yaw) * Vec3::Z); // last: planted feet by IK
```
Generated parts (a shield, a blade, a claw) go on bones with `rig.attach(part, "forearm_l")` (or `part.attach_to(bone)`): place the part where it belongs on the model at rest (your FITS/SOCKETS numbers), then attach, and it rides the bone from there. `games/mb3d-demo/src/mech.rs` (`mb shot games/mb3d-demo --param scene=mech`) is the whole recipe on a Tripo-rigged mech: gait, keyed strike with a spring stop, a hit, a clip layered over code, a look-at and a part in its hand.

`play_synced(clip, fade)` crossfades walk ↔ run at the same point of the stride. If you also simulate the motion (a jump arc you collide with), drive both from one curve, as Mistwood's `sim::jump_lift` mirrors the Jump clip. A model's root faces wherever the file's +z points: turn the root node, not the bones.

### Generated creatures that move (`mb model --spine`, `Flight`)

A generated model with no rig, a rig over 64 joints, one wing or no usable clip (Tripo's avian and serpentine rigs) still moves: `mb model <game> heron --import heron.glb --spine` (or `--spine` on a new model) lays a chain of joints head to tail with smooth weights, plus two-bone wings for a bird. Then, right after spawning:

```rust
let node = model.spawn(None)?;
let bones = Skeleton::spine(node)?;                 // spine_0 = head … tail, then wing_l_0/1, wing_r_0/1
// a fish: Swim as for the koi built in code; a dragon flying: bend up and down
let mut swim = Swim::new(4.5);
swim.wave = Wave { axis: Vec3::X, amplitude: 0.32, head: 0.25, waves: 0.75 };
// a bird: wingbeats from its climb and speed, glides, banks, a level head, a strike
let mut flight = Flight::new(2.4).level_with(&bones);   // level_with: it was sculpted standing
let mut path = Follow::new(start, 0.45);
// every frame:
let pos = path.update(target + vec3(0.0, flight.strike_move.height() * 2.0, 0.0), dt);
flight.update(dt, path.velocity);
node.set_transform(&Transform::at(pos + flight.bob()).with_rot(flight.rotation()));
flight.apply(&bones);
if attack { flight.strike(); }                      // rear back, dive wings swept, flare; hit-test at strike_move.reach() ≈ 1
```

**A creature that feels alive** (a checklist; each is a few lines):
- **Never a raw path.** Move it after a `Follow<Vec3>` of a target (halflife 0.3–0.6 s for big animals), and face it along the follow's velocity: no linear legs, no instant direction changes. `Follow` reads a target as moving in a straight line since the last frame, so a target that jumps (the next phase of a hunt, a new waypoint) reads as one frame of very fast motion and the path depends on the frame rate (`--smoothness` reports position jumps at one rate): call `follow.jump_target(new)` in that frame and the creature glides over the same way at 60 and 120 Hz (`snap` jumps it too, for a cut or respawn). Turning limits: `alive::turn_toward`.
- **Secondary motion.** Breathing (`alive::breath`, a 1–2 % swell at ~0.25 Hz), a tail that lags (a `Wave` whose phase trails the body, or `Flight`'s tail bend), a head that looks at things (`alive::look_yaw` through a `Follow`, ±60°) and stays level in banks (`Flight::head_steady`).
- **Effort shows.** Wingbeats or tail beats follow speed and climb (`Swim`, `Flight` already do), glides when it can, a bob on each downstroke.
- **Anticipation and follow-through.** Wind up before an action (`alive::anticipate`, `Strike`'s rear-back), overshoot a little after it (a `Spring`), ease in and out of every pose (`motion::Seq`), never snap.
- **Bank into turns** (`Flight::bank`, or roll by `atan(speed × turn rate / g)` yourself) and pitch with the climb.
- **Check it:** `mb shot --smoothness --track-node <its node handle>` (draw the handle in a debug HUD) must say "smooth" through a whole maneuver, and look at an `mb shot --sheet` mid-action, at the game's camera distance.

### Bones for meshes built in code (mb3d 2)
A mesh built in code can bend too: give it skin weights, make a `Skeleton` of nodes, bind it, and turn the bones every frame. That's how fish swim, tails and tentacles sway, banners and streamers wave, and tassels swing. The host bends the mesh in the vertex shader (shadows, outlines, fog and reflections follow), and bones are nodes, so it replays exactly.
```rust
// Once: bones along the fish's spine, under its node; body and fins share them.
let bones = Skeleton::chain(koi, &desc.spine()).unwrap();
bones.bind(koi);
bones.bind(fins);
let mut swim = Swim::new(desc.length).with_phase(k as f32);    // a different phase per fish
// Every frame: move the fish along its path, then let the wave follow its speed.
koi.set_transform(&Transform { pos, rot: Quat::look_rotation(vel, Vec3::Y), scale: Vec3::ONE });
swim.update(dt, vel.length());                                 // beats faster the faster it goes
swim.apply(&bones, turn_rate * 0.1);                           // bends into turns

// A banner or streamer: any mesh skinned along a chain, and a Wave.
let pts: Vec<Vec3> = (0..8).map(|i| vec3(i as f32 * 0.2, 0.0, 0.0)).collect();
let flag = Node::with_mesh(cloth.with_chain_weights(&pts).upload().unwrap(), cloth_mat).unwrap();
let flag_bones = Skeleton::chain(flag, &pts).unwrap();
flag_bones.bind(flag);
phase += TAU * 0.9 * dt;
Wave { axis: Vec3::Y, amplitude: 0.3, head: 0.35, waves: 1.2 }.apply(&flag_bones, phase, 0.0);
```
- `with_chain_weights(&points)` skins each vertex to the bone it's nearest, blending around joints; `with_joint(j)` pins a rigid part (an eye, a horn) to one bone; set `mesh.joints`/`weights` yourself for anything else. Use the same points for `Skeleton::chain`. For other skeletons, `Skeleton::new(parent, &[(parent_index, rest_transform)])`; `bones.turn(i, rot)` poses one bone from its rest pose; `Node::set_skin` is the raw call.
- Things ride bones the way they ride any node: `bones.bone(i)` is bone `i`'s node (0 is a chain's first point, `bones.tip()` its last; `with_names(&["head", ..])` then `bones.find("head")` if you'd rather name them), so `lantern.set_parent(bones.bone(7))`. `bones.socket(i, point)` makes a node on bone `i` at a point of the mesh's rest pose (where you built the eyes), which then follows that bone: eyes, a hat, a light, a lantern on a tail. Cheaper than skinning a second mesh just to make it follow.
- Make the bones children of the mesh's node (`Skeleton::chain(node, ..)` does): the skinned node's own transform is ignored, so move the node and the bones carry the mesh. Other meshes that share the bones (fins beside a body) should sit at the same place: parent them to the node with an identity transform.
- Keep each bone's turn modest (≲ 0.4 rad): this is linear blend skinning, and big twists pinch. More bones (8–12 along a fish) bend more smoothly than bigger angles.
- Drive `Swim`/`Wave` phases from `dt` (they are), so 60 and 120 Hz look the same; pass the real speed so the beat matches the motion (a fish that glides with a fast tail looks wrong).

### Lots of copies: instancing (mb3d 2)
Bullets, coins, grass, trees, crowds: one node, one mesh, one material, many `Instance`s (position, rotation, scale, a tint that also scales emissive), one draw call.
```rust
let coins = Node::with_mesh(coin_mesh, gold).unwrap();
let list: Vec<Instance> = coins_alive.iter().map(|c| Instance::at(c.pos).with_rot(Quat::from_rotation_y(t * 3.0)).with_color(0xffd27aff)).collect();
coins.set_instances(&list);   // replaces the list; send it again only when it changes
```
The node is culled as one sphere around all its instances: for a forest, use a node per chunk of ground (Mistwood moves 7 chunk nodes forward as the run goes on and re-lays out a chunk's instances only when it leapfrogs). Instances of a transparent material aren't sorted among themselves.

### Looks: sky, fog, toon, outlines, 3D text, depth of field, reflective water that reacts (mb3d 2)
```rust
gfx3d::atmosphere(&Atmosphere::night());   // or ::dusk(), ::day(), or your own zenith/horizon/ground, horizon band and sun halo
gfx3d::clouds(&Clouds { coverage: 0.45, ..Clouds::night() });   // drifting with game time, lit from the sun/moon
gfx3d::sky_disc(&SkyDisc::moon());          // a moon with seas and a corona where the sun's light comes from (or SkyDisc::sun())
gfx3d::sky_texture(Texture::new(sky_asset), &SkyTexture { fade: 0.3, ..Default::default() });   // a painted 2:1 panorama (mb art sky)
gfx3d::fog(&Fog { mode: FogMode::Exponential, color: [0.05, 0.075, 0.12], density: 0.045, height_falloff: 0.22, sky: 0.9, ..Default::default() });
material.set_style(&MaterialStyle { toon_bands: 3, outline_width: 1.5, outline_color: [0.01, 0.015, 0.03], rim: 0.3, ..Default::default() });
sign_label.set_text(&TextDesc::new(Font::SansBold, 0.3).with_outline([0.0, 0.0, 0.0, 1.0], 0.07), "100 m");     // in the node's xy plane, facing +z
pop.set_text(&TextDesc { flags: TextDesc::BILLBOARD | TextDesc::SCREEN, ..TextDesc::new(Font::SansBold, 26.0) }.with_color([2.4, 1.8, 0.6, fade]), "+10");
gfx3d::depth_of_field(&DepthOfField { focus: camera_to_player, range: 1.3, blur: 5.0, near: 0.4 });
let water = Material::new(&MaterialDesc { base_color: [0.006, 0.01, 0.018, 1.0], roughness: 0.06, flags: Material::REFLECTIVE, ..Default::default() }).unwrap();
gfx3d::reflector(&Reflector { max_distance: 70.0, ..Reflector::water(0.0) });   // the plane y = 0; rippling, half resolution
gfx3d::water(&Water::canal().flowing(Vec3::Z, 0.35));                          // water v2: current, see-through body, foam, glints, caustics
gfx3d::wake(0, koi_pos, koi_vel, 0.3, if swimming { 0.7 } else { 0.0 });       // every frame: a V wake, bow wave and foam behind it
gfx3d::ripple(landed_at, 0.9, 1.6, 2.5);                                        // a ring where something hit the water
```
- **Sky v2** makes an empty sky a place. `Atmosphere` replaces the `Sky`'s flat background with a zenith → horizon → ground gradient, a glow `band` along the horizon and a `glow` halo around the sun or moon (which follows `gfx3d::sun`); keep the `Sky` for its stars (they fade near the horizon and behind the glow) and a faint nebula (0.2–0.4) as a Milky Way. `Clouds` add a drifting layer lit from the sun's direction (`color` facing it, `shade` away, a `silver` lining near it); the reflector mirrors them, which is most of their charm over water. Set clouds once: their drift is free, changing their look re-bakes the lighting. `SkyDisc::moon()` replaces `Sky::sun_disc` with a limb-darkened moon with seas and a corona; leave its `direction` zero so it sits where the sun's light comes from (its highlight on water then lines up with its reflection); if you want the moon in view straight ahead, give the disc its own `direction` and keep the key light (`gfx3d::sun`) off to the side, or its glitter lands on the player, and keep a moon's `color` around 1–1.5 so tonemapping doesn't flatten it to white. A painted sky (`sky_texture`, a 2:1 equirectangular texture) gives a game a hand-made horizon — a skyline, mountains, an aurora — that also lights the scene; layer the procedural sky over it (stars over its dark parts, clouds, the moon) or let it `fade` into the procedural sky above the horizon. It's baked once, so it costs nothing per frame, but it's a big texture: 2048×1024 JPEG, 11 MB of the 96 MB texture budget. Make one with `mb art sky games/<name> sky "<the sky and the distant horizon, nothing in front>"` (~$0.05): it comes back 2:1, wrapping left to right, with its horizon on the middle row and calm poles; leave the sun or moon and the clouds out of the prompt if the game draws its own. Match the fog color to the atmosphere's `horizon` so the far end melts into it.
- **Fog** sells depth and hides where the world ends: match its color to the sky near the horizon and set `sky` so the horizon melts. `height_falloff` > 0 makes ground mist. Glows fade out in it; give the player `MaterialStyle::NO_FOG` if it must always read.
- **Distance fade: put cull distances inside the fog; fade, never pop.** Things that appear or vanish at a distance (houses, trees, props) read as glitches. `node.set_fade(start, end)` dithers a node out between `start` and `end` m and stops drawing it beyond (each instance of an instanced node fades on its own); `if let Some((s, e)) = fog.draw_distance() { gfx3d::draw_distance(s, e) }` does it for every small node, out of the mist. Keep `end` ≤ the fog's far distance and fade over a band, never a hard cutoff; `node.never_fade()` for the player and landmarks.
- **Toon + outline** is the fastest way to a coherent stylized look for code-built, flat-colored meshes: 2–4 bands, outlines 1–2.5 px in a dark color from the palette (not pure black). Not on a textured generated model: the bands flatten its texture to two or three tones; give it smooth shading with a rim and a thin outline (`MaterialStyle::hero()`). Backlighting (sun toward the camera) lets the rim and outlines carry the shapes only in a bright scene: under a dark sky the subject's lit side faces away and it turns to a silhouette on dark (check with `mb shot --readability`).
- **3D text**: world-space for signs and labels in the scene, billboards for floating scores (`SCREEN` keeps them a readable size at any distance). The host fonts only; colors above 1 glow.
- **Depth of field**: focus on the player every frame (distance from the camera); keep `blur` ≤ 6 px on a phone. It's for mood and for separating the player from busy backgrounds, not for hiding things.
- **Water v2** (`gfx3d::water(&Water)`) is what makes reflective water look like water rather than a rippled mirror: small ripples carried downstream by a current (`flow`, `flow_speed`, `detail`; the current can turn every frame, `Water::canal().flowing(dir, speed)` each frame, and the ripples carry on from where they are), a body you see into (`depth_fade`: the fish, the bed and the posts below the surface, bent by the ripples and fading into the water's `color` with depth), light glowing inside it under lanterns and toward a low sun (`tint`, `scatter`), foam where it laps walls, rocks, boats and anything else that goes *under* the surface (`foam`; things that stop at the surface don't foam), sharp glints of the moon and lights (`sparkle`), and caustics dancing on things under the water and on walls just above it (`caustics`). Start from `Water::canal()` (murky, flowing; turn its current with `.flowing(dir, speed)`), `Water::lake()` (deep blue, wind ripples, a long glitter path) or `Water::pond()` (clear and shallow: koi, the bed and its caustics show); it needs the reflector on and a `Material::REFLECTIVE` surface, and `Water::default()` turns it off. Keep the water's own `base_color` near black (the look adds the body), lower the reflector's `ripple` to 0.1–0.2 (the look's ripples carry the detail; a strong swell scatters the reflection into confetti), sink what should foam a few cm into the water, and keep fish swimming a little under the surface (one just under it foams over its back). The water's shadow no longer falls on what's under it while the look is on. `games/mb3d-demo`'s canal (tap for a low view by the wall, and for water v1) and koi pond are worked examples.
- **Reflective water** turns a dark night scene readable: lanterns, glowing signs and the moon (a low sun with `SkyDisc::moon()`) mirror and glint on it, and it shows where the course is. Make the water a dark, smooth (`roughness` 0.05–0.1) `Material::REFLECTIVE` plane at the reflector's height; reflections are strongest at grazing angles, so a low camera looking down the water shows them best. Give it emissive and lit things above it to reflect, keep the fog on (reflections are fogged too), and mark things the water can never show `MaterialStyle::NO_REFLECTION`. One reflector per scene. `games/mb3d-demo`'s moonlit canal (`canal.rs`) is a worked example.
- **The beauty pass** (a color grade, light shafts, halos around lamps in fog, film grain): the last layer, once the scene reads; see "Beauty pass" below.
- **Water that reacts**: `gfx3d::wake(index, pos, velocity, size, strength)` every frame for anything moving on the water (4 wakes: a V of waves behind it, a bow wave, churned foam along its track; it curves where the thing turned, and strength 0 lets it fade where it is), `gfx3d::ripple(at, strength, speed, seconds)` for rings (16 at once). They show on reflective materials while the reflector is on, tilting highlights, bending the reflection and foaming on steep crests. `Ring { center, strength, speed, seconds }` starts a ring and tells you how high it lifts the water anywhere (`ring.height(p, age)`), for things to bob on.

### Beauty pass: grading, light shafts, halos, grain (mb3d 2)
The last layer, once the scene already reads and looks good without it: it takes a well-lit night canal from "nice" to something people screenshot. It can't rescue flat lighting, and overdone it turns everything into haze.
```rust
gfx3d::grade(&Grade::moonlit());                         // or golden_hour(), neon(), Grade { shadows, highlights, toe, .. }, Grade::lerp(a, b, t)
gfx3d::shafts(&LightShafts::moonbeams());                // god rays from the sun or moon through whatever stands in front of it
gfx3d::halos(&Halos { reach: 1.5, ..Halos::mist() });    // every point light glows in the air around it, cut off by what's in front
gfx3d::film(&Film::subtle());                            // grain (moves with game time), faint anamorphic streaks off the brightest points
gfx3d::grade(&Grade { lut: Texture::new(lut_asset), ..Grade::default() });   // a grade baked by hand into a LUT (tools/lut.py)
```
- **Tune in this order**, and take the `mb shot --sheet` set from "Previewing" after each step, comparing with the sheet before it: (1) lighting and sky, (2) fog, (3) the grade, (4) shafts and halos, (5) bloom, (6) grain and streaks. Each later step multiplies what the earlier ones did, so going back up the list is how to fix a problem, not piling on further down.
- **The grade** sets the mood: lift (`shadows`) tints the blacks, gain (`highlights`) the whites, `midtones` the middle; `shadow_tint`/`highlight_tint` split-tone by hue without changing brightness (teal shadows and warm lantern light is the classic night look); `toe` deepens the blacks, `shoulder` softens the whites, `contrast` is an S-curve that never clips. Keep tints desaturated (the presets' are 0.1–0.4) and push `balance` negative so only real highlights get the warm tint: on the canal a stronger one turned the moon cream along with the lanterns. Dial a preset back with `amount` rather than editing every field. For a look a photo editor makes easily (curves, a film preset): `python3 tools/lut.py stamp screenshots/shot-10s.png stamped.png`, grade `stamped.png` with color adjustments only, `python3 tools/lut.py extract graded.png assets/grade.png`, and load it as the `lut`.
- **Light shafts** need the light in or near the view and something with gaps in front of it (a gate, trees, rooftops, a window frame); in open sky they're only a glow. `fog` keeps them in the distance; `size` (the glow they come from) small for distinct beams; `intensity` 0.3–0.8 at night. A wide glow over a fogged midground turns the whole frame into haze: if the sheet looks washed out, shrink `size` before anything else.
- **Halos** come from point lights only (≤ 8, as bright as each light), so give a lamp that should glow in the mist a light, even a dim short-range one. `reach` 1–2 m keeps them around the lamp; more washes the scene in its color. They're not bloom: they're cut off by what's in front and spread over what's behind.
- **Grain** 0.03–0.05 is film; above that it's a style, and on a phone it reads as noise sooner than on a monitor. Streaks only off lamps and glints (`streak_threshold` 4–6), `streaks` ≤ 0.3.
- **Readability first.** After the pass, the course, the player and the hazards must still be the clearest things on screen: look at the sheet in greyscale (value structure, "Reading a course at speed"). If a shaft or a halo is brighter than the next hazard, or grain shows on the HUD's background, pull it back. `games/mb3d-demo`'s canal (tap to compare with and without) and `games/mistwood` are worked examples.

### Retro look: 16-bit and 32-bit consoles (mb3d 2)
For "SNES pixel art meets Saturn 3D", "PS1 style", "low-poly retro": draw the 3D at a console's resolution and palette, keep the HUD sharp.
```rust
gfx3d::retro(&Retro::saturn());     // 320×224, 15-bit color, light Bayer dither, vertex wobble, banded fog; ps1(), arcade(), snes()
gfx3d::retro(&Retro { vertex_snap: 0.0, ..Retro::ps1() });   // tune one field
```
- `resolution` is a pixel budget: the short side of the phone gets `min(w, h)` square pixels (240 across a portrait phone for 320×240), the long side what fits. The mb2d HUD draws at full resolution over it: pixel-art HUDs and portraits stay crisp at whole-pixel multiples.
- `dither` 0 keeps flat pixel-art colors (SNES); 0.5–1 gives the Saturn/PS1 checkerboard gradients. `vertex_snap` 1 is the wobble; keep it 0 for a game where the player must aim precisely at distant things. `fog_bands` 6–10.
- Pixel-art textures need nearest sampling or they blur: `Texture::new(asset).map(Texture::nearest)`, and `Model::load(asset).map(Model::pixelated)` for a .glb's textures. Generate them with `pixel_art = true` (assets.md) and keep them small (64–256 px).
- **Parallax backdrops** behind the 3D, SNES-style: up to 4 pixel-art layers around the camera, far first; slower `scroll` reads as farther.
  ```rust
  gfx3d::backdrop(&[
      Layer::new(sky).scroll(0.2).repeat(2.0).at(-0.3).fill_below(),   // banded sky, barely turning
      Layer::new(mountains).scroll(0.45).repeat(3.0).at(-0.02),        // far: half the world's rate
      Layer::new(hills).scroll(0.75).repeat(4.0).at(-0.03),            // near: faster
  ]);
  ```
  Layers are textures with transparent tops (a skyline, a ridge), a few hundred pixels wide; `repeat` whole times around 360°; `at` the bottom edge (0 = the horizon, a little below so the floor covers it). They work without the retro look too.
- **Mode 7 floor**: a big tiled ground in perspective.
  ```rust
  let floor = Node::with_mesh(Mesh::new(&MeshData::mode7_floor(160.0, 4.0, 40))?, unlit_with(ground.nearest()))?;
  // every frame: keep it under the camera, snapped to whole tiles so the pattern never slides
  floor.set_transform(&Transform { pos: gfx3d::mode7_anchor(cam.pos, 4.0, 0.0), ..Default::default() });
  gfx3d::fog(&Fog { color: hills_bottom_linear, mode: FogMode::Linear, start: 40.0, end: 140.0, ..Default::default() }); // fade into the backdrop
  ```
  A 32–64 px tile with two tones (a checker, grass with tufts, road stripes) reads best; the floor's radius past the fog's end.
- **When the retro look hurts**: it's for low-poly, flat-colored models built for it, big on screen, in a lit scene. It hides small subjects (a mech that's 3% of the screen is a few dozen of its pixels), dark scenes (few color levels and banded fog crush the darks to one tone: a dusk arena at 6-bit is mud) and textured generated models (their detail is what you paid for, and 240 pixels or a few bits per channel throw it away). Bolt Brawl's owner couldn't see the mechs at 640×448, 6-bit color with fog bands. For those, keep full resolution with crisp edges instead (no `retro`, a thin dark outline from `MaterialStyle::hero()`, `Lighting::hero()` in "Lighting recipes"); if you want a period flavor, keep it in the HUD and backdrops (pixel-art HUD, `gfx3d::backdrop` layers), not on the subject. Toon bands do the same to a texture: 2–3 bands cut a generated model's texture to that many flat tones, so use toon on code-built flat-colored models and smooth shading (rim, outline) on textured ones. `mb shot --readability` says when the subject doesn't separate, and with the look on, that it may be the cause: shoot with it off to compare.
- Low-poly models (a few hundred triangles, flat-shaded) and small textures read best; smooth PBR detail turns to mush at 240 pixels. Bloom and the beauty pass still run (at the low resolution); a little bloom suits it, grain doesn't.
- Worked example: `mb shot games/mb3d-demo --param scene=retro --sheet` (two mechs on a Mode 7 floor under parallax mountains; add `--param retro=off` for the same scene at full resolution; its art is drawn by `games/mb3d-demo/tools/make_retro_art.py`).

### Lighting recipes
- **A textured hero against a bright sky** (a generated mech, character or creature that has to read at a glance; a crisp modern anime look): `gfx3d::lighting(&Lighting::hero().key_from(cam.rot.forward()))` sets a clear day atmosphere (the sky is the fill light from every side, and what metal and glossy paint reflect), a warm key light from high over the camera's shoulder with shadows, exposure 1, little bloom, no vignette; then `for m in model.materials() { m.set_style(&MaterialStyle::hero()); }` (smooth PBR shading, a rim on the silhouette, a 1 px dark outline). Full resolution, no toon bands, 1024² textures on the hero. Keep the key on the camera's side (a backlit subject shows the camera its dark side), the sky behind the subject lighter or darker than it, never the same, and an arena's ground mid-toned. `mb shot games/mb3d-demo --param scene=hero` (and `--param look=muddy` for the same dragon lit the way that hides it: backlit, dark, toon, retro 6-bit); check yours with `mb shot --readability`.
- **Crisp textures at an angle** (a mech's flank, an arena floor going into the distance): mb3d already samples textures mipmapped with 16× anisotropic filtering, so nothing to set; if they're soft, it's the texture size (1024² on the hero) or a camera that only sees them edge-on. For a crisper finish over the whole image, `gfx3d::post(&Post { sharpen: 0.4, ..Lighting::hero().post })` (0.3–0.5 is mild, without halos; nearly free; not with the retro look). `mb shot games/mb3d-demo --param scene=hero --param view=grazing --param sharpen=0.4` shows both on a steel deck.
- **Space / night**: a near-black sky background with saturated nebula colors (`[0.05, 0.19, 0.42]`, `[0.55, 0.07, 0.32]`, density ~1) gives colorful reflections; a warm sun (intensity ~3, shadows on) as the key; two colored point lights on opposite sides as rims; bloom 0.6–0.8 at threshold 1; vignette 0.3.
- **Dusk / golden hour** (`Grade::golden_hour()`, `LightShafts::sunbeams()` at ~0.45 through a gate or trees): `Atmosphere::dusk()` with the sun 2–6° above the horizon in front of the camera, warm (`[1.0, 0.55, 0.3]`, intensity 1.5–2) so everything is rimmed in orange; fog in a dusky rose (`[0.24, 0.13, 0.11]`); the halo (`glow`) is the sun's glare; a `SkyDisc::sun()` with a reddened `color` (`[14.0, 6.0, 2.2]`) and `Clouds::dusk()` lit orange from below finish it.
- **Bright and friendly**: raise all three sky colors (the sky is the ambient light: `[0.25, 0.35, 0.55]` etc., nebula density low), sun intensity 3–5 from high up, exposure ~1, bloom low (0.2) so whites don't glow, saturation 1.1.
- **Neon**: dark scene, unlit or emissive materials with strength 2–6 for the signs, point lights in the same colors near them so they light their surroundings, bloom 0.8+, chromatic aberration 0.3; `Grade::neon()`, `Halos::mist()` in a little fog and `Film::subtle()` with streaks finish it.
- **Readable metals**: metals show their environment; in a dark sky they look black. Give metals roughness 0.3–0.5 so the sun's highlight spreads, or keep the brightest nebula behind the camera.
- **Night that reads**: night is a mood, not black. Light the sky (`Atmosphere::night()` and up: deep blue `[0.04, 0.07, 0.16]` zenith, a lighter horizon `[0.12, 0.16, 0.28]`, a faint horizon `band`), use a cool moon as the shadow-casting sun (intensity 1.5–2.5, low and off to one side of the course, 30–60° from where the camera looks, so things are rimmed but its glitter path on water runs beside the player, not under it), fog in the horizon color so distance turns blue rather than black, and let warm point lights and emissives carry the accents. Then the beauty pass: `Grade::moonlit()`, `LightShafts::moonbeams()`, halos on the lamps. The ground the player travels on must be the most readable surface in the frame: never darker than the sky behind it.
- **Flashes**: an explosion is particles plus `shockwave` plus a point light whose intensity decays (`90 × exp(−7t)`) — the flash lighting nearby surfaces sells it more than the particles do. Camera shake is yours: offset the camera with your seeded `Rng`.

### Reading a course at speed (runners, racers, chases)

The player must read the next two seconds of course in a glance, at full speed, on a phone outdoors. Check every one of these in screenshots of mid-play at top speed, not on the title card:
- **Camera**: high and behind enough to see 2–3 s of course ahead (for a runner: ~3–5 m above the player, looking ~15–25 m ahead, fov 60–75°). The player takes up ~10–15 % of the screen height in the lower third, never so big it hides what's coming. Widen the fov a little with speed rather than pulling the camera in.
- **Smooth motion**: a kink is invisible in screenshots and to `mb.verify()` but felt at once on a phone. Keep the camera's velocity continuous: ease turns, lane changes and jump arcs so speed never changes within a frame, and never let what the camera follows jump. Smoothing `x += (target − x) * k` starts at full speed the frame its target jumps (a lane that snaps), so smooth a snapping target with a critically damped spring (second order) instead; and a constant `k` follows twice as fast at 120 Hz. `gfx3d::SpringCamera` does both: `SpringCamera::new(pos, look, 0.15, fov)` once, `cam.update(want_pos, want_look, dt)` in `update`, `gfx3d::camera(&cam.camera())` in `render`, `cam.snap(..)` on a cut. Check with `mb shot <game> --autoplay --at 10 <input> --smoothness` (add `--track-node <handle>` for the player's node; log `node.raw()` to learn it): it steps the run at 60 and 120 Hz and lists each spike in the camera's acceleration (position, direction and field of view) with its game time. Ease a field of view that follows speed (a boost's zoom) like any other camera motion: one that steps the frame a boost starts is a spike too. ~2× at 120 Hz is a velocity kink, ~4× a jump in position or per-frame smoothing; each spike lists what the game logged (`sys::log`) within two frames of it. Mark every impulse you mean (a cut from title to play, a jump's take-off, a dash, a knockback) with `sys::mark("jump")` in the `update` that applies it: a spike there is listed as `marked: jump` and not counted, so what remains are the kinks you didn't mean.
- **Value structure**: three separate value bands, so the scene still reads in greyscale: the course surface mid-light, hazards darkest or brightest with a hard edge, the background darker or hazier than both. Lane edges and the path's borders get a continuous bright line or rim; turns and gaps are announced by something bright at eye level well before the player reaches them.
- **One accent color means danger**, one means reward, and nothing else in the scene uses them.
- **Depth cues**: fog or haze over distance, repeated objects of known size (posts, gates, lamps) marching toward the horizon, and motion (particles, streaks, water flow) moving past the camera so speed is felt.
- **HUD**: in the top band and the corners, never over the player or the course just ahead.
- **Replays show the player**: a replay card's "Beat …" banner covers the bottom ~230 pt, right where a runner's player sits (~80 % down the screen). Put `replay_banner = "top"` in manifest.toml and the feed draws it under the top bar instead, over the top ~110 pt (the HUD; the button shows the run's score). Check with `mb shot <game> --autoplay --overlays`, which shades the banner where it will go: the player must stay clear of it.
- **Look at it**: take screenshots at 3 s, 15 s and 60 s into a round and at a turn, put them side by side, and ask: where is the course, where is the next hazard, what do I do next? If the answer takes more than a second, fix the light before adding content.

### Smooth motion: anything the camera follows
"Smooth" is a property of the second derivative. Kinks in a camera's path read as stutter even at a steady 60 fps.
- **Critically damped springs, never lerp-by-dt.** `gfx3d::SpringCamera` for a chase camera (a `motion::Follow<Vec3>` for where it is and one for where it looks), and `motion::Follow` (f32 or `Vec3`; `update_angle` for a yaw) for anything else that chases something. `a += (b − a)·(1 − e^(−k·dt))` (`damp`, the older `FollowCamera`) is first order: its speed jumps whenever the target jumps or turns. Pick a `halflife` (0.1–0.2 s for a chase camera); it trails a target moving at speed v by `lag(v)` (`Follow::lag_for(halflife, v)` without one at hand, e.g. for a `SpringCamera`), so aim at `target + velocity * lag` if the camera must sit on it.
- **Turn velocity changes into curves.** A spring smooths what it's given; if the player's direction snaps 90° at a turn, the path it follows has a corner. Blend the velocity over a few frames (or steer along an arc) in the sim, and follow a point that never teleports (snap the spring with `snap` on a real cut).
- **Test the second difference** of the camera at 60 and 120 Hz: step the sim and camera natively at both rates and compare the peak of `|p[i+1] − 2p[i] + p[i−1]| / dt²`. A smooth path has about the same peak at both; a snap or a kink shows as a peak that doubles with the rate. template-3d's `the_camera_moves_smoothly_at_60_and_120_hz` (src/sim.rs) is the pattern.

### Effects
`Emitter` (particle look + physics: color and size over life, lifetime, speed, cone, gravity, drag, additive or alpha, optional texture, velocity stretch for sparks) and `Trail` (ribbon that follows a node). The host simulates them deterministically, so replays match: never feed them anything but game state. A good explosion is four emitters fired together, each a different look:

```rust
// Built once in init, fired anywhere. Linear colors above 1 bloom.
let fire = Emitter::new(&EmitterDesc { colors: [[4.5, 2.6, 1.0, 1.0], [2.6, 0.8, 0.2, 0.75], [0.7, 0.14, 0.05, 0.35], [0.0; 4]],
    sizes: [0.5, 1.3, 1.7, 1.9], lifetime: [0.35, 0.7], speed: [0.4, 2.6], drag: 2.5, gravity: vec3(0.0, 0.6, 0.0), ..Default::default() }).unwrap();
let sparks = Emitter::new(&EmitterDesc { colors: [[9.0, 6.0, 2.5, 1.0], [6.0, 2.2, 0.5, 1.0], [2.5, 0.5, 0.1, 0.8], [0.4, 0.05, 0.0, 0.0]],
    sizes: [0.07, 0.06, 0.045, 0.02], lifetime: [0.5, 1.2], speed: [4.0, 11.0], drag: 1.6, gravity: vec3(0.0, -3.5, 0.0), stretch: 0.045, ..Default::default() }).unwrap();
let smoke = Emitter::new(&EmitterDesc { colors: [[0.2, 0.12, 0.08, 0.0], [0.13, 0.09, 0.09, 0.5], [0.07, 0.06, 0.08, 0.3], [0.04, 0.04, 0.05, 0.0]],
    sizes: [0.6, 1.3, 1.9, 2.4], lifetime: [1.4, 2.4], speed: [0.3, 1.4], drag: 1.2, blend: FxBlend::Alpha, gravity: vec3(0.0, 0.35, 0.0), ..Default::default() }).unwrap();
// The boom: a burst of each, a shockwave, and a flash light that decays (90 × exp(−7t)).
fire.emit(at, Vec3::ZERO, 36);
sparks.emit(at, Vec3::ZERO, 170);
smoke.emit(at, Vec3::ZERO, 26);
gfx3d::shockwave(at, 4.5, 0.012, 0.9);
```

**Things that move.** Particles live in world space and keep only the velocity their burst gave them. If what exploded was moving (an enemy flying alongside an on-rails camera), plain `emit` leaves the cloud behind and it streams away; fire it with `emit_moving(at, dir, count, velocity)` instead, passing the object's velocity, and the whole cloud rides along while it spreads (the template's catch does this with the drone's velocity). The other way is a floating frame: keep the player and camera near the origin and move the world past them, so things flying with the camera are nearly still. Pick one frame for the whole game. Trails follow their node's world position each frame, so a trail on something the camera flies with streaks only if the node really moves in the world.

**Selling speed** (racers, chases, dashes): `gfx3d::speed_blur(&SpeedBlur { radial, center, motion, stretch, .. })` (mb3d 2) adds a radial blur toward `center` (put it on the projected player), a directional motion blur along `motion` (how far a far point moved on screen since the last frame) and stretching at the screen edges, all cheap. Scale them with speed, and pair them with a field of view that widens with speed, a low camera, a constant small road shake and streak particles flying past the lens.

### Water that reacts
Still water under a moving player reads as a painted floor. Anything on or in water should disturb it, and the water should remember for a second or two. With a reflector and a `Material::REFLECTIVE` surface (above), and ideally a water v2 look (`gfx3d::water`, above: wake and crest foam then breaks into patches carried by the current, in its `foam_color`):
- **A wake behind anything that moves on the water**: `gfx3d::wake(i, pos, vel, size, strength)` every frame (4 at once). Turn strength to 0 when it leaves the water (a leap, a dive) or stops; its wake fades in place.
- **Rings and a splash when something enters or leaves the water**: `Splash` fires a crown of water, glinting droplets and a puff of mist with two rings, sized to the thing (`size`) and to how hard it hit (`speed`). Splash on the way out and again, harder, on the way back in. Smaller rings on their own (`gfx3d::ripple`) work for rain, a fish rising, a lantern touching down.
- **Spray at the bow** of fast things: `splash.spray(bow, vel, 2)` a few times a second.
- **Life on the surface**: petals, leaves or flotsam drifting on a slow current, as one instanced node, pushed aside by whatever swims through them and bobbing on rings (`Ring::height`). Keep them off the reflection (`MaterialStyle::NO_REFLECTION`).
- **Light the splash for the scene.** Particles aren't lit: pass the color of what lights the water (moonlight `[0.6, 0.62, 0.8]`, lanterns `[1.0, 0.6, 0.3]`, daylight `[1.6, 1.6, 1.5]`). Crown, mist and spray fade to the fog color with distance, and the droplets glint and fade out.
```rust
// Once, in init (four emitters):
let splash = Splash::new([0.6, 0.62, 0.8], 0.3).unwrap();         // moonlight; a koi-sized thing
// Every frame:
gfx3d::wake(0, koi.pos, koi.vel, 0.3, if koi.in_air { 0.0 } else { 0.7 });
if koi.left_water || koi.hit_water {
    let rings = splash.splash(vec3(koi.pos.x, 0.0, koi.pos.z), 0.3, if koi.hit_water { 4.5 } else { 2.5 });
    self.rings.extend(rings.map(|r| (r, now)));                     // for things to bob on
}
for p in &mut petals {
    p.y = rings.iter().map(|(r, t0)| r.height(p.pos, now - t0)).sum();
}
```
The presets are also there separately (`EmitterDesc::water_crown`, `water_droplets`, `water_mist`, `water_spray`, each `(light, scale)`) to tweak or fire on their own. `games/mb3d-demo`'s canal does all of this with a leaping koi.

### HUD over 3D
`gfx3d::project(world)` gives the logical screen position (mb2d's coordinates), depth and whether it's on screen: put labels, reticles and off-screen arrows there. Everything mb2d draws in the frame lands on top of the 3D image. Call it after `gfx3d::camera` in `render` (it uses the latest camera). For game logic (lock-on, tap hit tests), use `camera.project(world, W, H)` on your own `Camera` value instead: the same formula (SPEC §5.4) in plain Rust, so it runs in `cargo test` (where `gfx3d::project` returns zeros) and agrees with the host to within float rounding.

### Previewing
`mb serve` and `mb shot` work the same. The first few frames show the sky without its nebula while the lighting bakes; screenshots after `await mb.step(n)` with n ≥ 10 are representative. Check that effects replay with `await mb.verify()` (see Build, preview, look at it): explosions, particles and trails are part of the draw hash, so `match: true` means they land exactly where they did live. `await mb.screenshot("name")` saves the frame to `screenshots/name.png` in your game directory; `mb.stats()` shows what the frame drew against the budgets above (`mb shot --stats` prints a line per shot: draws, triangles, reflected draws, casters, particles and the p95 ms since the previous shot; `--stats=json` everything).

A 3D game that moves (a runner, a racer, a flyer) is where an unreadable course hides, so take this set after every change to the camera, lighting, fog, materials or track, and look at the sheet before anything else:
```sh
mb shot games/<name> --sheet --autoplay --at 0.5,2,5,10,20    # title, the first seconds, cruising, top speed
mb shot games/<name> --sheet --name dodge --at 3,3.3,3.6 --autoplay --swipe 100,500,260,500@3   # mid-action, frame by frame
```
In each shot: the player stands out from the ground and the sky (not dark on dark: give it a rim or emissive, the ground some value contrast); the edges of the track, lanes or path are visible all the way to the horizon; the next hazard is visible early enough to react at that speed (the camera far enough back and high enough, the fog not hiding it); hazards read as different from scenery; and the HUD sits clear of the player and of the card's overlays (`--overlays`).

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
- [ ] 3D: `mb shot … --smoothness` over a played stretch lists no spikes in the camera's motion; the cuts and impulses you meant are `sys::mark`ed and show as marked (see Smooth motion).
- [ ] The identity check in [identity.md](identity.md#3-check-it-against-the-house-look) answered against docs/IDENTITY.md: the game's own fonts (`mb build` gives no font or theme warning), palette and cards, and no "yes" left that you couldn't justify.
- [ ] Started from a kit? All three "Make it yours" changes are in, and nothing of the kit's title, character or tagline is left.
- [ ] DESIGN.md updated to match what you built, with the "not verified on device" list and tuning constants.
- [ ] The game's own `icon.png`: no placeholder or kit icon left (`mb build` says so), and you've looked at it.
- [ ] Published with `--model` naming you (your own model name and version)?
- [ ] An update to a published game? The right number bumped (fix, update or new season), NOTES.md has the version's entry, and no new season unless the user asked for one (or a board that ranks differently needs one, and they agreed).
- [ ] Published? `DESCRIPTION.txt` says what the game is and how to play it, its first line works on its own, and `mb describe` sent it.
- [ ] Generated assets: every preview looked at, `mb art check` clean (no mock placeholders, within budgets), and the user told what the art and sound cost.
- [ ] Sound: `check_audio.py` and `mix_check.py` (on a bot round) clean, or each flag explained in DESIGN.md.
