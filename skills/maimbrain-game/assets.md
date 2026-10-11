# Art, sound and models: generated or code-drawn

A game's assets come from one of two places, and most games use both:

- **Generated** with `mb art`, `mb music`, `mb sfx` and `mb model`: an image, audio or 3D model makes it, `mb` post-processes it to the platform's rules and writes it into `assets/` with a provenance sidecar. Painterly backgrounds, characters with texture and personality, an icon that looks like a real game, a real music track. Costs a few cents a call and needs the user's API key, or an account the Maimbrain team has given access to Maimbrain's keys (then `mb login` is enough). Full reference: `docs/GENERATE.md` (in the kit: `docs/GENERATE.md`).
- **Code-drawn** with a script in `games/<name>/tools/` (`scripts/pixel.py`, `scripts/sfx.py`) or at runtime with mb2d shapes: free, exact, remixable, tweakable by numbers.

Code-drawn art is pixel art too, so it matches generated pixel sprites and stays crisp. Draw it with `pixel.py` on a small canvas at the art's real resolution (a 24–48 px sprite, a 64×64 icon) and store it at `scale=4`; at runtime, place and size mb2d rects in whole art pixels (2 units each) and keep circles to a few art pixels across. No soft alpha glows, blurred halos or anti-aliased blobs (a glow is a ring of one lighter pixel color), and no emoji-style faces stuck on top of the scene. A few flat colors, clear silhouettes and one consistent pixel size read better on a phone than many smooth shapes.

## Which to use

Free isn't the goal; looking good to a stranger is. Code-drawn assets are free, but for anything a player looks at closely (the hero, the threat, characters, creatures, vehicles, a key background) a generated asset is usually far better for a few cents to a dollar. A 3D game's typical budget is $3–8: generated models for the hero, the threat and 2–4 signature props, a sky, a music track, the effects. Spend it.


| Use generated for | Use code for |
|---|---|
| characters, creatures, props with detail or texture | anything that changes shape or color with state (draw it in greys and tint it; see SKILL.md §3) |
| backgrounds, parallax layers, tiles | geometry the rules depend on (hitboxes you must match exactly, platforms, grids, meters) |
| the icon | particles, glows, trails, screen flashes (mb2d/mb3d do these better live) |
| music: a real track with instruments | effects that must sit exactly on the beat or follow a pitch scale (sfx.py, tuned to the game's key) |
| sound effects with a real-world source (glass, wood, creatures, weather), when there's an ElevenLabs key | 8-bit blips, UI ticks, sweeps; any effect when there's no ElevenLabs key |
| restyling the user's photo or sketch into the game (`--ref`) | text: never in generated images; draw it with the game's fonts (`mb font add`, below) |

Run `mb models` first: models marked `✓` use the user's own key, `M` ones Maimbrain's keys (their account has access; the last line shows how much of this month's budget is left; Tripo's 3D models only with 3D access, a separate grant). Without either, `mb` says so. Tell the user what they'd need in one line: `mb keys set gemini` with billing on (docs/GENERATE.md "What to set up"), or ask the Maimbrain team for access to its keys and `mb login`. Then carry on with code-drawn assets rather than waiting. Don't use `--provider maimbrain` to get around the user's own key, and never pass `--allow-name` to slip a flagged name past the check (Maimbrain's keys refuse it anyway). You can still build the whole pipeline with `--provider mock` placeholders and swap them later with `mb regen <asset> --provider gemini`: `mb art check` lists every mock leftover. Never publish mock placeholders.

Check `mb models` (prices, which keys are set) before a batch, and tell the user roughly what it will cost; on Maimbrain's keys, also whether it fits what's left of their monthly budget. A typical 2D game is under a dollar: a hero, a few enemies or items, a background or three parallax layers, the icon (each ~$0.03–0.05 at the default model) and one or two music loops ($0.04 each). Use `--variants` and Pro models deliberately; every command refuses to spend more than $1 unless given `--max-cost`.

## When to generate a 3D model and when to build it in code

A 3D game's shapes come from `mb model` (a text-to-3D service makes a textured, optionally rigged and animated `.glb`), from code (the SDK's `MeshData` primitives, lofts and subdivision, `tools/make_glb.py`, `tools/make_rigged_glb.py`), or both.

| Generate (`mb model`) | Build in code |
|---|---|
| living things: animals, fish, characters, monsters (built from spheres and cones they read as blobs: a player called a primitive koi "very polygonal, barely looks like a fish at all"). Simple, smooth creatures (fish, eels, tentacles) can also be lofted in code and swum on code-built bones (`MeshData::fish`, `Skeleton`, `Swim`: 3d.md "Meshes in code") | what the rules measure: tracks, lanes, platforms, walls, colliders, grids, anything you hit-test exactly |
| props with sculpted shape and painted texture (a carved lantern, a treasure chest, a statue) | repeated architecture and terrain built in chunks and instanced (Mistwood's forest, the canal's houses) |
| a hero object players look at up close | glows, beams, particles, trails, shockwaves: mb3d does them live |
| a character that walks, runs, idles or swims (`--animate`) | things that change shape with state (a growing tower, a bending bridge), and simple shapes a primitive already reads as (coins, gems, crates, balls) |

When you replace an asset (a `heron2.glb` for `heron.glb`), move the old one out of `assets/` (to `art/retired/`): everything in `assets/` is packed, and `mb build` lists assets nothing names when the bundle nears its 10 MiB or 512 files (a path or file name in `src/`, a format pattern like `format!("assets/vo/{}/{}", ch, id)` or `"bd_"`, or a voice script row in `art/*.csv` all count as naming). Parallel `mb model … &` runs are fine: Tripo runs 5 P1 models at once per account, and `mb` queues the rest (and retries the provider's "too many tasks" with backoff), so start them all and wait. Defaults that work: `--polys 3000`–`8000` for a hero (a frame's budget is ~150 k triangles), 500–1500 for a prop seen small or many times, `--size` in meters as it should stand in the game, `--texture-size` 1024 (the default) for a hero or close-ups, 512 for mid-size things, 256 for small props (a 1024² model takes about 8 MB of mb3d's 96 with its normal and metal-rough maps at half size, the default `--maps-size`, and 17 MB with all three at 1024²; a 512² one about 2 MB: shrink the props and the far scenery to fit the budget, not the hero; `--reprocess --texture-size N` changes it later for free), `--glow` for things that should bloom at night, `--animate swim` (fish, on Tripo) or `idle,run` (characters). Look at `art/preview/<name>.png` (six sides with an arrow for the way it faces, then each clip) and then at the game with `mb shot`; if it faces the wrong way, `--turn`. A swimming fish is about $0.75 on Tripo, a running character with idle and run about $0.85, a prop $0.40. Without a 3D key (`mb models`: tripo or meshy), tell the user `mb keys set tripo` (docs/GENERATE.md "3D models") and build it in code meanwhile; `--provider mock` makes a placeholder (a generic fish or critter) to wire up loading and clips, and `mb art check` lists it until it's replaced.

**Parts, props and vehicles** (a mech's guns, blades and armor plates; a crate; a dropship): pass `--kind part` (or `prop`, `vehicle`). The style guide's `[3d]` words, `style` and `keywords` are written for characters, and sent with a gun they turn it into a little robot with a head and legs; a `--kind part` model gets the `[3d_object]` words and the palette instead, and "character, robot, face, arms, legs" as the negative prompt. mb guesses the kind from the prompt when you leave it out and prints what it picked: check that line. Name the part's shape and say it's one piece ("a single twin-barrel shoulder cannon, boxy, nothing attached"). Generated parts come back posed as the generator liked (a blade on a diagonal, a plate leaning back), so give each the pose its socket expects with `--orient`: `tip=+y` for blades and poles (point up, flat side to +z), `tip=+z up=+y` for guns and barrels (muzzle forward), `face=+z` for plates and shields (outer face to +z). Parts are centered on the origin, so the part's node goes on the socket as is; `mb model info assets/<part>.glb` prints its axes, tip, face, glow spot and the socket offset, and which `--orient` suits it. Don't work these out from the mesh by hand. For a glowing weak spot (a gem, a core, coolant stripes) use `--glow-color '#rrggbb'` with the color as it came back in the texture: `--glow` lights by brightness, so on a white or yellow part it lights everything. Geometry is written compressed (quantized, about half the bytes; nothing to run afterwards): the node that draws a model's mesh carries the scale that undoes it, so attach and move a part by a node above it (its socket or the spawn node), never by `set_transform` on `"<asset>/body"`. If the run warns that it came back as polished metal (it reads near-black outdoors, with nothing bright to reflect), `--reprocess --metal 0.15 --min-roughness 0.6` mattes it; do it there rather than in game code, where a material's roughness factor can't raise its map's roughness.

**Rigs and clips on generated characters.** A static model you already have can be rigged later: `mb model games/<g> <name> --rig --animate idle` (no prompt) rigs `assets/<name>.glb` from the task its sidecar recorded ($0.25, no new mesh) and records the rig task, so a later `--animate slash` adds a clip to the same rig ($0.10) without rigging again; `--reprocess` keeps the rig and clips. `mb model presets` lists Tripo's real clip names (`--animate idle,slash,hit_to_body_01`; `punch` isn't one, `box_01` is; `chop` is the same motion as `slash`), with their lengths. The mocap clips are long and mostly filler (Idle 15 s, Hurt 14 s): keep the part you use with `--clip Idle=0..4,Slash=0.6..3.4`, or cut beats out of one, `--clip Windup=Slash@0.6..1.8,Strike=Slash@1.8..3.4` (free, in the recipe, `--reprocess` changes it). There is no strafe, back-step, heavy landing or lunge preset: do those in code on the rig. Mechs and armored characters: `--reprocess --rigid` makes each vertex follow one bone (plates move as plates, not rubber), and `--pin cape=spine` (or `skirt=hips`, `auto`) moves a cape or skirt that the auto-rig tied to the arms and legs onto the body, so big arm moves don't stretch it; look at `art/preview/<name>.weights.png` (each clip before, then after). A rigged biped is turned to face the way its rig says (toes forward of the ankles, `L_`/`R_` sides), so a mech is no longer "facing uncertain"; Tripo's come back facing +x, so a reprocess turns a model that game code was turning by hand (`--turn 360` keeps the old way). The run's rig report (`result.rig` in the sidecar) pairs joints by their side names (`L_`/`R_`, `Left`/`Right`, `.L`/`.R`, `_l`/`_r`), so "the rig isn't symmetric" means a limb really is missing on one side.

## Fonts

Type is part of the art. Pick the game's fonts in the art-direction step ([identity.md](identity.md)) and match the style guide to them: a slab western face wants a wood-and-brass palette, a ballpoint hand wants paper. `mb fonts` lists the 96 library fonts with mood, era, uses and pairings; `mb fonts --preview` draws a specimen sheet; `mb fonts --category pixel` or `--search horror` narrows it. `mb font add games/<name> <id>` downloads one from github.com/google/fonts (pinned, checksum-verified), bakes a subset atlas into `assets/fonts/<id>.mbf` with its license, and records it in `fonts.toml` so `mb build` re-bakes it if you change the entry. A font of your own (a .ttf/.otf you may redistribute) works the same: `mb font add games/<name> fonts-src/Mine.ttf --license fonts-src/LICENSE.txt`. Keep a game to 2–3 fonts of 25–60 KB each.

## 1. Write the style guide first

`mb art init games/<name>` writes `art/style.toml`. Fill it in from DESIGN.md before generating anything, because every image gets it:

- `style`: one or two concrete sentences: medium, shape language, line, light, mood, taken from the game's identity (Reference and palette). **Games are crisp pixel art by default**: the template turns `pixel_art` on and leaves `style` empty for you to say which pixel art ("16-bit console pixel art, two shading steps per color, cool moonlit palette", "Game Boy pixel art: four greens, chunky clusters"). Not a cartoon: smooth flat-vector shapes with thick outlines and cel shading is the generic look players have told us they dislike, so don't reach for it unless the user asks. Another medium (a painting, ink, paper, clay) is fine when the identity or the user calls for it: turn `pixel_art` off and describe it as concretely. If the user has a look in mind (or dislikes one), that wins over anything here.
- `keywords`: 3–6 short words that matter for *this feed* and this look: readable silhouettes, high contrast, strong value separation.
- `palette`: 5–8 hex colors, darkest first, with clear value steps (a dark, two mid tones, a highlight, one accent that means "danger" or "reward"). Check that the player, the hazards and the background are different values, not only different hues.
- `avoid`: what would break the look (photorealism, gradients in the sky, busy detail).
- `pixel_art = true` (with `pixel_scale = 4`, both the template's default): every image is reduced to a grid of whole art pixels and stored at 4 texels per art pixel (SPEC §5.3: draw each art pixel 2 logical units wide). Usually with `palette_lock = true`. For 3D (textures, backdrops, a Mode 7 floor under the retro look), sample them nearest (`Texture::nearest`, `Model::pixelated`) so art pixels stay square. Backdrop layers (`gfx3d::backdrop`) are strips 256–512 px wide with a transparent sky above a silhouette, tiling left to right; a script (`games/mb3d-demo/tools/make_retro_art.py`) or `mb art` with `pixel_art = true` and a seamless prompt.
- `[sprite] size`: the size your code draws sprites at × 2 (pixels per logical unit). The main character ≥ 1/3 of the screen width (120+ units → 240+ px); anything the player tracks ≥ ~40 units (80 px).
- `[music] prompt` / `[sfx] prompt`: the sound's palette (instruments, texture), so all tracks and effects belong together.

Generate the hero first, iterate until it's right, then add it to `references` (`references = ["assets/hero.png"]`): later generations match it.

## 2. Write prompts like briefs

Describe what it *looks like*, as something original: subject, pose or action, the one or two details that make it read. "A round moth with stained-glass wings spread wide, big curious eyes" — not "a cool moth character". The kind's framing (centered, margin, no ground, flat background, no text) is added for you; `--dry-run` shows the full prompt.

- Never name existing characters, franchises, games, brands or artists. `mb` refuses obvious names (`--allow-name` only for genuine false positives or the user's own IP) and the community guidelines reject copies. "In the style of <studio or living artist>" gets a warning: describe the qualities instead.
- One subject per sprite. For animation, use `mb art frames … --frames 4..8` ("the moth flapping its wings", "the frog mid-hop"): the frames come out the same size in equal cells, bottom-anchored. Look at them in sequence: if one frame is off, regenerate (consistency isn't guaranteed).
- A character who talks (voice lines, taunts) gets a face sheet from `mb art portrait <game> <name> "<character>" --expressions smug,angry,scared`: 4 mouth shapes (closed → wide open) across, a row per expression (neutral first) and a blink row last, aligned so the head holds still, ready for `ui::Portrait` (GENERATE.md "Talking portraits"). A fighter's face should show the fight: add `hurt,battered` to the expressions, and drive `portrait.set_health(hp)` and `portrait.hit(strength)` on every hit, for the opponent and for the player's own portrait in the HUD (sweat, soot, cracked glass, static, sparks and a red alarm as health drops; a shake, flash and knock on each hit; UI.md "Damage and hit reactions"). The overlays work on any sheet, so an existing portrait gets them without regenerating. A face in a frame (a comm screen, a HUD corner) gets `.clipped()`, so a hit knocks it about inside the frame instead of out of it.
- Backgrounds: say what's *behind* the play ("a moonlit cathedral nave, stained-glass windows high up"); the prompt already asks for a calm, lower-contrast middle where the game happens.
- Music: genre, instruments, mood, energy, and `--bpm` if the game keeps time with it. "Bouncy marimba and hand claps, playful, light". The prompt already asks for instrumental, steady, no intro or ending. `--layers` gives a calm base and a bright high layer to fade in with intensity (the mothglass/summoned pattern; audio.md "Music that follows the game" adds bus effects: a filter that opens with speed, reverb in a tunnel). For a threat that closes in (a chaser, a timer), `--layers base,hi,tension` also writes a tension layer and a ticking clock on the loop's beat and in its key, and a heartbeat and a sting in that key; audio.md has the recipe that maps danger onto them.
- Effects: source, material, action, length. "A short bright glass chime, instant attack", `--seconds 0.3`.

What the generators do that the docs don't say:

- **ElevenLabs** (`mb sfx`) allows 4 requests at once per key (shared with everyone using it: run `mb sfx` one at a time) and never returns less than ~0.5 s, so a click or tick comes with a tail: play it quieter or trim it. A harsh take: ask for "a soft attack, no sparkle, no shimmer, no high ring, one short note" and a mellow material (felt, wood, rubber). A late one: "starts immediately". It also happily returns bass-heavy takes ("a deep plunge", "a soft deep gong") that a phone speaker loses 10–30 dB of: `mb sfx` measures that right after each take (`phone` in its log, `phone_loss_db` in the sidecar) and warns past 10 dB. Ask for a brighter take ("a crisp …, with a bright click on the attack"), or `mb regen assets/<name>.ogg --reprocess --brighten` (free): it adds the bass's harmonics, which a phone can play, so a deep thud still reads as deep; `--no-brighten` undoes it, and the raw take stays in `art/raw/`.
- **Lyria** (`mb music`) ignores the key you ask for (and drifts from the BPM): use the key and BPM `mb music` reports (it measures the key the way `mix_check.py` does).
- **Characters**: generate every pose or expression of one character as one sheet with `mb art frames` (they stay the same character), then use the hero as `--ref` (or in `references`) for its color variants and everything after. When code depends on a shape, say it: "a perfect square exactly as wide as it is tall", "almost twice as wide as it is tall". For sprites, "floating on the flat background with no ground and no shadow beneath it" and "nothing peeks out behind it" (models add backing layers and drop shadows that key badly). Pick a `key` color the subject never contains (pure green for candy, magenta for foliage) and put that color in `avoid`.

## 3. Iterate cheaply

- `--dry-run` first when unsure: prompt and price, no call.
- `--variants 3`: three candidates and `art/variants/<name>-sheet.png` to compare them in one look; `mb art pick games/<name> <name> 2` keeps one.
- `mb regen games/<name>/assets/hero.png` — same recipe and seed; `--new-seed` for another take; `--seed N` to lock one you liked; `--prompt "…"` to change the description. The sidecar keeps everything else (size, grid, provider, model).
- `mb regen <asset> --reprocess` — redo only the post-processing from the saved raw output: free, offline. Use it after changing the palette, `colors`, `palette_lock` or `--tolerance`.
- Seeds are honored by Gemini/Lyria (best effort) and the mock, not by OpenAI or ElevenLabs: there the raw output in `art/raw/` is how you keep a result.
- After editing `art/style.toml`, `mb art check` lists assets made with the old version; regenerate the ones that matter.

## 4. Look at every result

You can't judge an image from its file size. After each generation, **open the preview** the command prints (`art/preview/<name>.png`: the asset over a checkerboard, a dark and a light ground, enlarged) and check:

- the silhouette reads at game size (squint: is it still a moth?), and against your background's value
- no leftover background: a halo or fringe of the key color around the edges (regenerate, or raise `--tolerance` and `--reprocess`), holes where the subject had the key's color (set a different `key` in the style guide)
- nothing cut off (the log warns when the subject touched the image edges), no stray text, letters or watermark-like marks
- it matches the hero and the palette
- frames: same size, same facing, a believable cycle
- tiles and layers: put two side by side in your head (or in the game) and look at the seam

Then look at it **in the game** (`mb serve` and screenshots, SKILL.md §4): scale, contrast against the background, the card overlays.

For sound, you can't listen: read the numbers each command prints (length, peak, loud RMS, the loop seam "clean" or "CLICK", how much a phone loses, the loop's bars, BPM and key) and run `scripts/check_audio.py` on the files (it prints the same phone loss next to brightness, and flags an effect past −10 dB) (`--loops <name>` for loops whose names don't contain "music" or "loop"), then check how they sit together (§5). Tell the user what you couldn't verify by ear, and that generated music should be heard once on a phone before publishing. For rhythm games, build the beat grid from the BPM `mb music` reports for the cut loop (it's derived from the loop's exact length), not the BPM you asked for.

## 5. Mix the sound

You can't hear the mix, so measure it. `scripts/mix_check.py` renders a bot-played round with the real files the way the platform mixes (master 0.8 into a −9 dB limiter, 32 voices) and measures loudness K-weighted (like LUFS) per bus and per sound. The kits' sound modules record every event in test builds and `tests.rs` has a `mix_log` test that plays a bot round through them: `MB_MIX_LOG=/tmp/mix.log cargo test -p <crate> mix_log`, then `python3 <skill dir>/scripts/mix_check.py games/<name>/assets /tmp/mix.log` (`--fx-loop <name>` for a loop that is an effect, like a ticking clock). Keep that recorder when you change the sound; in a game without one, log `<seconds> <sound> <vol> <pitch> [play|loop|set|stop]` lines the same way.

Levels. Files are saved at −20 dBFS RMS for music and −14 for effects (`mb`, sfx.py), so effects play well under vol 1. The rules mix_check enforces:

| | rule (vs the music, in the same round) | typical vol |
|---|---|---|
| music | the reference: about −23 LUFS short-term; louder hits the limiter | 0.85–1.0 (a quiet stem more) |
| all effects together | music over them by ≥ +3 dB in the median 400 ms window, and ≥ −6 dB in 90 % of windows | |
| frequent (≥ 40 a minute: shots, coins, pops, thuds, ticks) | heard on a phone (below); its own 3 s loudness ≥ 6 dB under the music, its loudest 400 ms ≤ +4 dB, under half the effects' energy, brightness ≤ 5 kHz | 0.2–0.4 |
| regular (6–40 a minute: a swap, a landing, a ding per answer) | heard on a phone; ≥ 4 dB under, peaks ≤ +4 dB | 0.3–0.5 |
| rare and important (a hit, a fail, a level, a boss, a new best) | heard on a phone, near the music's loudness; peaks ≤ +8 dB; may duck the music 3–6 dB (vol × 0.5–0.7) for under a second, never deeper | 0.5–0.8 |
| master limiter | pulls > 3 dB less than 5 % of the time (more pumps the music) | |
| tension (`*_tension`, `*_tick` and a `*_heartbeat` over them) | music meant to rise with the danger: at its loudest 3 s it lifts the music ≥ +3 dB on a phone; at its quietest ≤ +1 dB (it lets go); the heartbeat's loudest quarter of plays heard on a phone (loud ≥ −10, heard ≥ +3) | tension 0 → 1.4, clock 0 → 1.0, heartbeat 0.15 → 0.3 |

Heard on a phone. Players listen on a phone speaker, which barely plays below ~300 Hz and is at its best at 1–4 kHz, like the ear. A pickup is short: what makes it heard is its attack standing out from the music at that moment, while what tires the ear and buries the music is its energy over time. So mix_check caps frequent sounds by their 3 s level and share (−6 dB adds 1 dB to the music's level, about the smallest change anyone notices), not by their peak, and listens to every play's loudest 50 ms through a phone-speaker model, in third-octave bands (about the ear's critical bands). Each must hold for 3 plays in 4:

- `loud`: the peak's loudness against the music's typical loudness, summed across bands the way the ear does (a sound spread over many bands is louder than a pure tone with the same energy; 10 dB is half as loud). Frequent and regular sounds ≥ −10 dB (at least half as loud as the music: less is background); rare ones ≥ −5.
- `heard`: in its best band, how far it stands over the music playing then. A tone is barely detectable ~4 dB under the noise in its band, and recognizable at a glance a few dB over it: ≥ +3 dB, rare ones ≥ +6.
- `phone`: how much of it the speaker loses. A thud, a gong or a soft low pluck can lose 20–30 dB: no level fixes that. Pitch it up an octave or two, layer a click or a brighter partial on it, regenerate it brighter, or `mb regen <file> --reprocess --brighten` a generated one. `mb sfx` and `check_audio.py` report the same number when the sound is made, before any round is played.

Tension is music, not an effect. Loops named `*_tension` or `*_tick` (`mb music --layers …,tension`) and a `*_heartbeat` played over them are dread on top of the track: the music's typical level (what effects are judged against) and its key come from the track's own layers, so the tension rising doesn't make the calm look ducked or the effects look quiet. mix_check reports how much the tension lifts the music, on a phone and to the ear, at its loudest and quietest 3 s: run it on a round that reaches full danger. A heartbeat isn't held to the caps on frequent effects (it's meant to fill the bar at full danger); its loudest quarter of plays must be heard on a phone (the quiet ones, at low danger, are meant to be felt more than heard). The levels in audio.md "Music that follows the game" (and `music::Director`'s) pass, with the tension clearly heard: +4 dB on a phone at full danger over a real chase take.

A music graph's take (`*_take`, or `*_take_base` + `*_take_hi`) is music too, though the Director plays it as voices started part-way in, two overlapping at every jump: its `Log` writes `from=`, `fade=` and `id=` on those lines, and mix_check starts each voice where it says, fades it in, lets a released one die away (60 dB over its `fade`) and keeps two voices of one sound apart, as the platform does. Stingers and the swell are one-shots on the music bus; mix_check judges them as effects, and a stinger whose riser is mostly low may be flagged as masked on a phone.

Flagged too quiet, raise the vol until `loud` passes; if that breaks the 3 s cap, the sound rings too long or is too pure for its rate: shorten it, brighten it, or layer it (engagement.md §8, pickups). Bus volume and filters aren't in the log: a sound must pass without a duck.

Variation and voices:

- Every frequent sound varies each play: level ±1–2 dB, and pitch ±3–5 % if it is unpitched. A tonal sound varies its level, its pitch ≤ ±1 % (±4 % is most of a semitone off key).
- Cap same-sound voices at 3–4 and rate-limit them (one per 60–100 ms; fold a burst into one sound). To steal a voice, `set` its vol to 0 and `stop` it a frame later (`stop` alone clicks). The platform's 32-voice limit steals the oldest one-shot, not the least important one.

Key and pitch:

- Pitch every tonal effect onto the music's notes: mix_check flags a tonal sound whose played notes clash with the measured key, and a music layer (a tension stem) that clashes with the main loop, with the shift that fits best. It takes the key from the track's own layers, not from loops named `*_tension` or `*_tick`.
- Pitch ladders (combo, streak) climb on the scale's notes, stay within ±7 semitones of the sample (further, it chipmunks or turns to sludge), and start over. A take that's too low for a phone (a pluck around 300 Hz) can be baked an octave or two up into the file first: a near-pure tone has no formants to chipmunk.
- Effects come from the music's world: the same instrument family, synthesized effects for synthesized music.

Every moving thing makes sound. A mech that strides, turns and flies in silence reads as a model on a turntable, whatever the art; the first thing a player says is "the mechs don't have any sound effects". Tie the sound to the motion, not to events you remember to add (`maimbrain::foley`, audio.md "Foley: sound from motion"):

- **Steps** from the stride: a stomp each time a foot comes down (`Gait` from the walk's phase, or `Contact` from a foot's height), harder at a run. Big things are heavy: a low thud with a bright clank on top so a phone hears it.
- **Joints** whine with how fast they move: a servo loop whose level and pitch follow the turn rate or the arm's speed (`Drive::servo`), silent when still. Hydraulics hiss on a hard stop or a landing.
- **Thrusters and engines** follow their thrust (`Drive::thruster`): spool up, roar, spool down.
- **Impacts** as hard as they were: a landing from a hop taps, a fall from the sky booms (`Foley::impact` from the speed); a punch clangs; a part torn off screeches.
- **Placed in the scene**: quieter with distance, panned to its side of the camera, and a voice budget so the loudest heard win (six at once by default): four mechs stomping don't bury the music.
- The sounds: `mb sfx <game> --pack mech [--flavor "a rusty junkyard robot"]` generates stomp, servo, thruster, hiss, impact and tear (about $0.01 on ElevenLabs); `python3 scripts/sfx.py --pack mech games/<name>/assets` makes them procedurally, free. A creature or a vehicle needs the same set in its own materials (claws on stone and a wingbeat; tires, an engine and a skid).

Before shipping: `check_audio.py` clean; `mix_check.py` clean on a typical bot round and on a long one, or each remaining flag explained in DESIGN.md; the title card sparse (cards play sound in the feed); and tell the user the mix was measured, not heard.

## Voice and characters

Characters who talk back (a rival pilot who taunts you, a boss with a catchphrase) make a short game feel alive, and `mb voice` makes it cheap: a bark costs well under a cent. Reference: GENERATE.md "Voices", UI.md "Talking portraits and dialogue".

When to voice:
- **Barks** during play: sparse, short (≤ 1.5 s, one short sentence or less: "Too slow!", "Not the paint job!") and reactive. Characters who talk all the time are more annoying than engaging; long talking belongs in cutscenes. Fire them from gameplay events (a big hit, a near miss, a part breaking, low health) with `talk.barks.trigger(tag)`, which lets a late one go stale, and `trigger_priority` for the few that must be heard (defeat, victory). Give each pool 3–6 lines: `Barks` deals them from a shuffle bag, never repeats a recent line, speaks one voice at a time with 5 s between barks, rests each pool 8 s and caps each character at 4 a minute. Leave idle chatter off. A bark never blocks input.
- **Cutscene lines** between rounds or before a duel: a sentence or two each, three or four lines at most. Each plays to the end of its voice: tap through it, or `.auto_advance(0.4)` to move on by itself after each voice; never end a scene on a timer, wait for `DialogueEvent::Finished`. Feed sessions are short: the intro must not stand between the player and the game for more than a few seconds, and it's skippable. Intros and endings with camera moves, titles and credits go on `ui::Sequence` (UI.md "Cutscenes"); an ending takes 45–60 s in five beats (engagement.md "Writing an ending").
- **Subtitles always**, for every line, barks too: many players have the sound off. The subtitle is the line without its audio tags; the typewriter follows the voice.

Writing rules (every game is all-ages, in a public feed):
- **No romantic, flirtatious or sexual content involving minors or child characters**, ever, including subtext, innuendo, "devotion" or "longing" framing. A child character's relationship with an adult (a nanny, a parent, a mentor, a ward) stays plainly non-romantic: affection is familial or comic, never charged.
- **Caretaker, mentor and family bonds stay non-romantic** for adults too unless romance is the explicit, appropriate point of the game. Rivalries are about pride, skill and grudges, not attraction.
- **No innuendo** in an all-ages game; camp and silliness come from theatricality, puns and big egos.
- **Read the whole script aloud, in order, before running `mb voice`.** Spoken delivery (emotion tags, pauses, "[whispers]") can add subtext the text didn't mean. A line that could read as romantic between the wrong characters gets rewritten, not re-voiced. When in doubt, cut it.

Casting zany characters:
- Make every voice distinct on three axes at once, so two are never confused with the sound half on: **pitch** (a deep drawl next to a high creak), **accent or rhythm** (a lazy drawl, a fast clipped city accent), **energy** (a showman's yell, a muttering grump).
- Describe the voice, not a person: age, accent, pitch, texture (raspy, nasal, booming, creaky), energy, attitude, "cartoonish and over the top". `mb voice` refuses clones, impressions and "sounds like <a name>", and you shouldn't try to get around it.
- Design once, listen to the three previews (or, if you can't listen, pick by what `mb voice design` measured: median pitch, length, phone loss; keep characters' pitches far apart), and keep the cast in `art/voices.toml`, committed.
- **Every kept voice takes one of the account's voice slots** (10 on ElevenLabs Starter), for good. Run `mb voice slots` before designing a cast: it shows what's free and which game uses which voice. Don't design throwaway voices to "try a few": design once per character, with a careful description. When `design` says there's no free slot, stop and tell the user what `mb voice slots` shows; don't delete anything on their account yourself (`mb voice gc` without `--yes` shows what could go; the user decides). A finished game's cast can give its slots back with `mb voice release <game> --all` (the voices are kept as audio in `art/voice_ref/`, commit it; the user adds `--delete --yes`): later lines are spoken by a close recreation, so voice a scene's lines in one `mb voice script` batch. Minor characters (an announcer, a shopkeeper) can take a premade voice instead, which needs no slot: `mb voice premades`, then `mb voice cast <game> <who> --premade <name> --delivery creative --style 0.6`.

Writing lines:
- Punchy: short words, one idea, the joke at the end. Give each character a catchphrase and a verbal tic, and use them.
- React to what the player just did: separate pools for the player's hit, the player's miss, the character getting hit, a near-miss, a comeback, winning, losing. Reactions that land at the right moment are what make a character feel alive.
- Act it with `--emotion` (smug, angry, scared, excited, sarcastic) or tags in the line (`[cackles]`, `[sighs]`, `[whispers]`); `--variants 3` for the lines heard most, then keep the best.
- Keep the script in `art/lines.csv` and generate with `mb voice script`: build the scene with `--provider mock` stand-ins (free, real timings), then speak it for real. Editing one line respeaks only that line. Add a `tag` column (or name ids `<tag><n>`, like `part_broken2`) and `talk.barks.load_script(include_str!("../art/lines.csv"))` loads every line into its pool.

Making it funny, not banal. A generic taunt ("You'll never beat me!") is filler: players tune it out by the second fight. What makes a line land:
- **Specific beats generic.** Name the thing on screen, the part that just broke, the exact way the player won. A joke about *this* moment can't be swapped into another game.
- **Humor comes from the character's want and flaw.** Give each one a want (to be admired, to retire, to win back a trophy) and a flaw that keeps getting in the way (vanity, cowardice, being terrible with names). Their lines are that flaw colliding with what just happened.
- **One running gag per character that pays off.** Set it up in their intro, call back to it in a bark, and pay it off in their defeat line. A callback across fights (the first boss's grudge mentioned by the third) makes a run feel written.
- **Rule of three:** two beats set a pattern, the third breaks it ("Lucky." / "Lucky again." / "Okay, I'm starting to think it isn't luck.").
- **Brevity:** cut every word that isn't the joke, and put the funny word last. If a bark needs a second sentence, it's a cutscene line.

| Before (banal) | After (a vain duelist who loves his paint job) |
|---|---|
| "Ha! You missed!" | "Were you aiming at the cloud? Fine. The cloud had it coming." |
| "Ow! You'll pay for that!" | "That was the *good* coat." |
| "I'll be back!" (defeat) | "Tell no one I lost. Tell everyone I looked fabulous." |

No inside baseball. The player has seen your game for a minute; you have lived in its world for a week. Every faction, district, company, technology and backstory name you invented is noise to them, and a line that leans on one is a line they can't follow:
- **A budget for names.** About five proper nouns in the whole game (the hero, each opponent, maybe the place), and at most one new one per scene. Faction, company, gadget and lore names: zero in barks, none in a cutscene unless the thing is on screen.
- **A name is on screen before it's spoken.** A character's name appears on their portrait's tag, the fight card or the HUD before any line uses it. Anything else is described by what the player can see: its color, its size, its gear, what it just did. "The big guy with the crane", "the one who threw the fridge", "the red one", not the name of the crew he runs with.
- **One idea per line, in a kid's words.** No "which is", no "ever since the…", no history. If a line needs a clause to explain who someone is, cut the clause or the line.
- **The 10-year-old check.** Read the script aloud in order, then retell the story in one sentence using only words that were on screen or that a 10-year-old knows. If you need a name they haven't seen, or the sentence starts "So there's this group called…", rewrite until you don't.

| Before (inside baseball) | After (what's on screen) |
|---|---|
| "The Ferrocant Syndicate won't forgive what happened at Dockline Seven." | "Big guy with the crane? Still mad you dented it." |
| "Initiate the Corvane protocol before the Tidewardens arrive!" | "Hit the red button before the boats get here!" |
| "Since the Collapse, only Arbiter-class frames can enter the Spire." | "Only the big ones get into the tower. Let's get big." |

Slapstick and puns, when the game is a comedy (its tone in DESIGN.md says so; a grim, eerie or earnest game keeps its own humor, or none). Cartoon comedy works in a two-second bark when the picture does half the work:
- **The setup is on screen, the line is the punchline.** The arm just flew off, the mech just tripped over its own cannon, the boss just got stuck to the magnet: the line doesn't describe it, it reacts to it. Fire the bark from that exact event, so the picture and the words land together.
- **The pun is the last word.** One pun per line, on a word a 10-year-old knows (bolt, nuts, screw, spring, scrap, gear, wrench, steam, shock) and tied to what just happened. Put it last so the line stops on the laugh; a pun in the middle gets talked over.
- **Show it with the body.** Physical comedy the mech or the portrait can do: a beat of stillness, then the fall; a head spinning a full turn; an arm dangling by one wire; a hop on the good leg; steam puffing from a vent on the angry word; the portrait going from smug to horrified on the punchline. Overreaction and timing are the joke (§9 in engagement.md).
- **Escalate the same gag.** The first bonk is small, the second bigger, the third takes the whole torso off. Rule of three again.
- **Let the groan be part of it.** A pun can be bad on purpose if someone notices: the hero's flat "No." after a terrible one is a second laugh.
- Keep it slapstick, not cruel: things fall off, bounce, spring and clang; nobody is really hurt, and nobody is laughed at for who they are.

| Before (no picture, no pun) | After (on screen, then the line) |
|---|---|
| "Curse you! My systems are damaged!" | His arm clanks to the floor. He looks at it. "Well. I'm… disarmed." |
| "Ha! Take that!" (and misses) | A huge swing, a full spin, a wobble. "Meant to do that. Both times." |
| "You hit my leg! Not fair!" | Hopping on one foot, arms windmilling: "Gimme a sec. I'm on my last leg." |
| "You can't escape my magnet!" | Stuck flat to her own magnet, upside down: "I'm just… very attached to this." |

A villain turn. When someone the hero trusts (a mentor, a sponsor, a coach) turns out to be the villain, make it clear and final; an all-ages audience wants someone to boo:
- **A clear evil want**, said once, plainly, in their own words: to own every mech in the city, to melt the champions down for their own army, to win by never letting anyone else win. Not "complicated reasons".
- **A cruel choice on screen**: they do something unforgivable that the player sees, like crushing the hero's trophy, selling out the friend who trusted them, or leaving their own sidekick behind in the junk pile. A villain who only talks about being evil isn't one yet.
- **No redemption, no last-second turn.** No "maybe I was wrong", no sacrifice, no sad backstory dropped in the last fight, no music swelling with sympathy. Beaten, they're still themselves, and they're carted off still scheming (a slapstick exit makes a good release: towed away by the crane they used to own).
- **Set it up with one early line** that sounds kind in the first scene and chilling later: "Every mech in this city should answer to one hand." "Don't worry about who's paying for your repairs." One line, not a trail of hints; the player should be surprised and then think "oh, *that's* what that meant".
- **The reveal is an act, then one line of want.** No monologue explaining the plan.
- **Give the hero a clean emotional beat**: one short line that's hurt and decided, then the fight. Not a speech, not pleading. The jokes stop here (the tone arc below).
- All-ages still: the cruelty is betrayal, greed and broken things, never gore, torture or threats against children.

| Before (muddy) | After (clear) |
|---|---|
| Mentor: "I did what I had to. Perhaps one day you'll understand." | Mentor, crushing the hero's trophy in his mech's fist: "I don't need champions. I need scrap. You're first." |
| Hero: "Why? I trusted you! After everything we've been through…" | Hero: "You taught me to fight. Lesson's over." |
| Defeated mentor: "You were right… I'm sorry." | Defeated, dangling from a tow hook: "Put me down! I own the tow trucks! …I owned the tow trucks." |

A story the player can retell. One line of premise ("a rookie pilot has one night to beat four champions for the city's title"), one twist, and no exposition dumps: show the stakes in a line or two before each fight, never a paragraph. If the player can't retell it in one sentence, it's too complicated; cut until they can.

A hero who's cool, not annoying. Make the player's character a rookie with quiet confidence and dry one-liners ("Noted." "Next."), and let the opponents be the loud ones: a hero who yells and gloats on every hit gets old fast and stands between the player and the character they're playing. Give the hero one vulnerability (a debt, a rival, a promise to someone) and let the story press on it once, near the end.

A tone arc. Start light, escalate (higher stakes, sharper banter, the running gags piling up), then turn to drama for the finale: set the turn up early (one line in fight one that means more later), drop the jokes completely when it turns (a joker going quiet says more than any speech), let the music and pacing carry it (fewer, slower lines, the music thinned out or changed key), and after the last hit give a small release: one warm or funny line so the run ends on a smile.

| Before | After |
|---|---|
| Finale boss: "Now face my ultimate power!" | The chatty rival from fight one, quietly: "No jokes this time. You know what's in that hangar." |
| Hero after a win: "Yeah! I'm the best! Nobody can stop me!" | "Next." |

Faces: a talking portrait per character (`mb art portrait … --expressions smug,angry,scared`), the mouth driven by the line's `.lips.txt`, the expression chosen per line. A bundle holds 512 files: `mb build` packs each character's `.lips.txt` files into one `vo/<character>/lips.txt` and leaves the sidecars out, so a voiced line costs one entry, and warns from 80% with a count by kind.

Mixing: voices go on `Bus::Sfx` above the music (they're mastered at −16 dBFS against the music's −20); duck the music 3–6 dB while a voice speaks (`Dialogue` does it with its `Ducker`, 5 dB by default: then set the music's level through `ducker.set_base`, not `Bus::Music.volume`). Don't play an effect's loud tail over a bark's words: pan or lower competing effects while a line plays.

## 6. Budgets

- Bundle ≤ 10 MB, startup set (wasm + `startup_assets`) ≤ 3 MB, ≤ 128 images, each ≤ 4096² (SPEC §1, §5.3). `mb art check` adds it up.
- Typical sizes after mb's PNG optimizer: a 128² sprite 5–20 KB (pixel art 1–3 KB), a 720×1280 background 300–900 KB, a 2-screen parallax layer 200–600 KB, a 30 s mono music loop at `-q 4` ~250 KB.
- Big backgrounds: `--colors 128` or `--max-kb 400` (quantizes until it fits; check the preview for banding on gradients), or generate at a smaller `--size` and draw it scaled up if the art is soft anyway.
- Many sprites: `mb art atlas games/<name> sprites` packs them into one PNG and writes `src/sprites.rs` with source rects; delete the individual PNGs (`--remove-inputs`) once the game draws from the atlas, or they ship too.
- Only list what the first seconds need in `startup_assets`; music and late assets load in the background.

## 7. Provenance

Each asset's sidecar (`assets/<file>.gen.json`) records prompt, model, seed, date, references and cost (and for a model its triangles, textures, joints and clips), and stays in the game's folder for `mb regen` and reviewers; `mb build` leaves sidecars out of the bundle unless the code reads them (names a `.gen.json`, or `provenance.json` for all of them packed into one). Don't delete or hand-edit sidecars; if you hand-edit a generated image, `mb art check` notes it. Commit `art/style.toml`, `art/ref/` and the sidecars; `art/raw/`, `art/variants/` and `art/preview/` are git-ignored.
