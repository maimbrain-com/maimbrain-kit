# Nova Pip

A vertical arcade shooter for one thumb. Pip, a little neon pod with a face in its dark visor, flies up through waves of jellies, swoopers, darters and bulbs. Pip fires on its own; the player only steers. Around the 50-second mark the Gloom Queen arrives with a health bar and three attacks. Beat her and the loop starts again, faster.

- **The verb**: drag (relative). Touch down anywhere and the ship moves by as much as the thumb moves (×1.25), from where it is. It never jumps to the thumb, and it eases up to sit at least 88 units above the thumb, so the thumb never covers it. Firing is automatic.
- **The 1-second read**: a little pod with big eyes, bright pink bullets everywhere, a swarm of enemies with faces glaring at it. "One tiny pod. One big swarm." Something always moves: Pip flies itself through a wave on the title card.
- **The first 10 seconds**: the tap that enters play starts the round and is already the steering touch. A row of jellies drifts down right into Pip's shots: the first kill lands within 2 s (+100, pop, burst), with no enemy fire for the first 4 s. A ghost thumb slides side to side under Pip with arrows and "DRAG TO FLY" until the player has dragged 60 units, and it comes back after 3 s without the thumb moving. A V of jellies, then two columns, come down before anything shoots back; gems pop out of every kill and the combo meter starts counting.
- **Tension and failure**: enemy bullets are big hot-pink rings with white-hot cores, a dark outline and a glow (orange for big ones), over a synthwave night, and every shooter telegraphs: a swelling glow at its mouth for 0.35 s before it fires, darters draw a dashed line to where they'll dash (flashing red when locked), and the queen opens her mouth while rings close in on it before each attack. Pip's face worries (big eyes, brows up, a sweat drop) when bullets are close, and the screen edges pulse red on the last heart. A hit costs a heart (with a red flash, shake, hit-stop and 1.8 s of blinking invulnerability; the bullets right around Pip are cleared, so a hit is never instantly followed by another). The last hit freezes for 0.3 s with a ring where the bullet landed, then Pip spins in slow motion with little pops for 1.7 s and blows up. Grazes (a bullet passing within 14 units) score and flash "CLOSE!".
- **The loop**: difficulty steps up every 11 s (bullet speed, fire rate, wave density); a new enemy type joins with its own banner at 13 s (swoopers), 24 s (darters) and 35 s (bulbs), a WARNING at 45.5 s and the boss at 48 s; she cycles Bloom (rings), Rain (aimed fans from her hands) and Sweep (a two-armed spiral), enrages below 40 %. Beating her turns every bullet into a gem, drops a heart, and starts loop 2 with spinners (at 10 s) and harder everything.
- **The cards**: title: the attract mode (the bot flies Pip through the opening waves; nothing is scored), with NOVA PIP, the tagline, the best and a DAILY button. Game over: SHOT DOWN!, the score rolling up, BEST or NEW BEST!, SCORES and DAILY buttons, over the aftermath (the enemies drifting on where Pip was). A replay reads from the score, the hearts, the faces and the telegraphs.
- **Score**: points per kill (jelly 100, swooper 150, darter 250, spinner 500, bulb 600) × the combo multiplier (×2 at 5 kills in a chain, ×3 at 10, up to ×8; a chain survives 1.6 s between kills and breaks on a hit), gems 25, grazes 10, power-ups 200, the queen 5,000 × loop. Shown big at the top; best saved per mode (board 0, and board 1 for the daily).
- **Power-ups** (one at least every 20 kills; bulbs often drop one): Spread (3 shots, 5 if picked again; 10 s), Shield (takes one hit), Magnet (gems and power-ups fly to Pip; 10 s).
- **Daily mode**: the same wave sequence for everyone today (`sys::daily_seed`). Waves come from their own `Rng` (`Sim::wave_rng`), so what the player does never changes which formation comes next.

## Look and sound

Neon synthwave vector: glowing magenta and cyan tube outlines around dark indigo glass, on a retro night (the style guide is `art/style.toml`). Everything with texture is generated; everything that moves with the rules is code.

- **Generated** (`tools/regen.sh` has every prompt; each asset's `.gen.json` sidecar records model, seed and cost): Pip, the five enemies (the swooper as a 4-frame flap sheet), the Gloom Queen's crowned dome and her claw hands, the gem, the heart and the three power-up orbs, all packed into one atlas (`assets/sprites.png`, rects in `src/sprites.rs`, quantized to 256 colors); the backdrop (`assets/backdrop.png`: indigo sky, a striped neon sun, wireframe mountains, an empty floor); the icon.
- **Code**, restyled to match: the floor grid rushing toward you below the horizon (it speeds up and turns orange with the heat, as the backdrop warms), twinkling stars and speed streaks, every face (the sprites have dark empty windows; eyes and brows drawn there follow Pip, Pip's own eyes follow the nearest bullet), Pip's three-layer exhaust and under-glow, the queen's tentacles and arms as swaying neon tubes, telegraph glows, enemy bullets (glow, dark outline, hot rim, white core: they read on the sun, the grid and the swarm), cyan shots, particles, and the UI kit theme (indigo glass panels with a cyan edge, a hot magenta accent).
- **Music**: a driving 120 BPM synthwave loop (arpeggiated bass, gated snares, saw pads) split at 900 Hz into `music_base` and `music_hi`. The high layer sits at 45 % at rest and opens fully with bullets, heat and the last heart. The WARNING sinks the loop, and the queen's arrival switches hard to `music_boss` (132 BPM, distorted bass, a screaming lead), hidden under her arrival boom. When she falls, the main loop comes back 1.6 s after her explosion.
- **Effects** (synthesized by `tools/synth_sfx.py`: deterministic, free, every recipe a commented function): an analog-synth palette from the music's own family, so nothing sounds like foley. Detuned saws and PWM pulses, two-operator FM bells, filtered-noise sweeps, sine sub drops, 1980s gated-reverb snaps and dotted-eighth echoes at the loop's 120 BPM. Every tonal sound is in the music's key, measured from the decoded loop (it came out G minor whatever the prompt asked; the bass walks F, G, G, Eb): the pop dives onto G, the gem rings on G5, the stingers use G minor's notes (the power-up a G minor arpeggio, the new-enemy stab Eb falling to D, game over D Bb G, new best the synthwave Eb, F, G cadence), and the pop and gem ladders climb the G minor pentatonic one step per kill or gem, turn round at the octave and come back down. The boss loop centres on A, so while it plays every tonal effect moves up a whole tone, and the queen's own sounds (her attack charge, her enrage stab below 40 %, the thunk of a hit on her) are on A.
  - Kills: a bright saw zap diving two octaves plus a short band-passed noise burst and a gated snap (it replaced the old bitcrushed crunch). Faster enemy types start higher up the ladder; bulbs and spinners pop an octave down with a synth explosion (noise through a falling low-pass over a G sub drop) under it.
  - The auto-fire is a 75 ms pulse "pew" (G6 to G5, low-passed, no noise) on every other volley; a shot that hits but doesn't kill is a soft dull tick.
  - The mix (`MIX` in `src/sound.rs`): the music is the reference. The frequent sounds (shots ~270 a minute, hits and pops ~70, gems ~40) are the quietest, with ±1.5–2 dB level and ±3–8 % pitch jitter (±0.4 % on the tuned ones), a voice cap per sound (2–3, the oldest stopped) and a minimum gap between starts. Rare ones (a hit on Pip, the shield breaking, the warning, the queen, the death, the verdict) are loud and are the only ones that duck the music. Measured with `tools/mixsim.py` on a decent-bot round (111 s, with the boss): the frequent sounds together sit 10 dB under the music at the median (p90 −3.6 dB), and all effects together are louder than the music in 21 % of 400 ms windows (the old set: +4.8 dB median, louder in 95 %).

## Tuning knobs

All in `src/sim.rs`, at the top (`// ---- Tuning knobs`). The ones that matter most:

| Knob | Now | What it does |
|---|---|---|
| `DRAG_GAIN` | 1.25 | Ship units per unit of thumb travel. Higher crosses the screen with a shorter drag, but amplifies tremor. |
| `MIN_LIFT` | 88 | How far above the thumb the ship eases to when the thumb is right under it. |
| `FIRE_INTERVAL` | 0.11 s | Auto-fire rate: the player's damage per second. |
| `GRACE` | 4 s | No enemy fire at the start (the first seconds can't fail). |
| `HEAT_STEP` / `HEAT_GAIN` | 11 s / +0.14 | How often and how much the difficulty rises (bullet speed, fire rate, wave gaps). |
| `BULLET_SPEED` | 185 | Enemy bullet speed at heat 1. The single biggest difficulty lever. |
| `WAVE_GAP` | 2.7 s | Time between waves at heat 1. |
| `BOSS_AT` / `BOSS_HP` | 48 s / 140 | When the queen comes and how many shots she takes. |
| `COMBO_WINDOW` | 1.6 s | How long a chain survives between kills. |
| `POWER_EVERY` / `POWER_TIME` | 20 kills / 10 s | Power-up frequency and duration. |
| `SHIP_R` / `GRAZE` | 7 / 14 | Ship hit radius (the art is ~2.5× bigger) and the graze band. |
| `INVULN_TIME` | 1.8 s | Blinking after a hit. |

To look at a late moment, set `DEBUG_SKIP_TO` in `src/lib.rs` (e.g. `sim::BOSS_AT - 3.0` starts every round at the boss's warning), and back to 0 to ship. Enemy stats (HP, size, points, gems) are in `Kind`'s methods in `src/sim.rs`; fire rates in `Sim::move_world`; boss attacks in `Sim::boss_step`; formations and the schedule (when each type is introduced) in `src/waves.rs`. Colors and words in `src/look.rs`; where the faces sit in the art, and the art's drawn sizes, at the top of `src/draw.rs`.

## Make it yours

Three changes that turn this kit into its own game:

1. **A new theme, hero and cast**. Rename everything in `src/look.rs` (title, tagline, heading, banner lines, boss name) and repaint its palette (the code-drawn grid, glows, bullets and UI). Then regenerate the art and music (`docs/GENERATE.md`; `mb models` must show a gemini key; about $1.15 for the whole set) and resynthesize the effects (free):
   1. Edit `art/style.toml`: the `style` sentence, `keywords`, `avoid`, `palette`, and the `[music]`/`[sfx]` words. Set `references = []`.
   2. Rewrite the prompts at the top of `tools/regen.sh` for your cast (keep "a plain dark empty oval where a face will be added later" if you keep the code-drawn eyes; keep the darter pointing right and the backdrop's horizon "exactly halfway down" with an empty floor for the grid).
   3. `tools/regen.sh hero`, look at `art/variants/pip-sheet.png`, `mb art pick <game> pip <n>`, then `tools/regen.sh ref` and set `references = ["art/ref/pip.jpg"]`: every later image matches the hero.
   4. `tools/regen.sh sprites backdrop icon music sfx`. Open every preview in `art/preview/` (regenerate off-model ones with `tools/regen.sh <name>` or `mb regen <game>/art/sprites/<name>.png --new-seed`), run the skill's `scripts/check_audio.py --loops hum` on `assets/*.ogg`. If the music changed, measure its key and set `ROOT` in `tools/synth_sfx.py` (and `PENTA`/`KEY_BOSS` in `src/sound.rs`) before `tools/regen.sh sfx`; then check the mix with `tools/mixsim.py` (its docstring has the two commands).
   5. `tools/measure_faces.sh`, and copy the face offsets (and the backdrop's horizon) into the constants at the top of `src/draw.rs`. Check it in the preview: faces in their windows, bullets readable on the new backdrop.

   Or keep the art and only redraw parts: each `draw::enemy` match arm is a sprite plus code eyes. Ideas:
   - A paper airplane over a schoolyard, shooting paper balls at angry homework (bullets become ink blots).
   - A bee defending the hive from wasps; pollen gems, a hornet queen.
   - A submarine rising through the deep: jellyfish, anglerfish with lures as the tell, a kraken boss.
   - A cupcake ship in a candy storm: gumdrops, licorice darters, a cake boss with candle "bullets".
   - Night-time fireflies vs. moths around a lamp boss; the background brightens as the heat rises.
2. **A twist on the verb or a rule** (`Sim::steer`, `Sim::fire`, `Sim::collide_ship`, `Sim::kill`). Ideas:
   - Firing only while the thumb is still (or only while moving): a rhythm of dodge then shoot.
   - Lift the thumb to fire a charged blast that clears bullets; hold to steer.
   - Bullets you graze charge a meter; full meter = a screen-clearing bomb.
   - Gravity: the ship sinks unless the thumb drags up (a "flappy" shooter).
   - Absorb bullets of one color, die to the other (tap to swap polarity).
   - One heart, but every 1,000 points of combo buys a shield.
3. **One new mechanic, enemy or power-up** (`Kind` in `src/sim.rs` with its `hp/radius/points/gems`, its movement in `move_world`, its pattern in `enemy_fire`, a formation in `src/waves.rs` with an entry in `INTROS`, its art in `draw::enemy`, its banner line in `look::banner`). Ideas:
   - A splitter that breaks into two small ones when shot.
   - A shielded enemy only hittable from the side (or after its shell is shot off).
   - A mine field that drifts down and explodes into a ring if shot too close.
   - A "drone" power-up: a little buddy that orbits and shoots too.
   - A laser enemy: a thin telegraph line, then a beam for a second.
   - A second boss with different attacks for loop 2 (copy `Attack`, add a variant set).

## Difficulty (bot)

`cargo test -p kit_shooter --profile bot -- --ignored --nocapture difficulty`, 200 seeded rounds per preset, through `maimbrain::bot::Difficulty` (round length against the targets, score, what killed each round and where, and "won": beating the first boss; it fails if a better preset doesn't do better). The bot (`src/bot.rs`) plays through the same touch API as a person: it re-plans every reaction delay from where bullets were that long ago (wound back by their velocity), anticipates forward if skilled, only notices bullets within its awareness radius, steers a thumb with a speed limit and a slowly wandering tremor (multiplied by `DRAG_GAIN` on the ship), overshoots big new moves and corrects cautiously after, has attention lapses (doesn't re-plan), and greedily lines up under enemies and chases pickups. It chooses among 77 nearby spots by scoring where each threat will be over the next 0.6 s.

| Preset | Reaction | Awareness | Median round | p10–p90 | Score (median) | Beats the boss | Dies in |
|---|---|---|---|---|---|---|---|
| first-timer | 240–380 ms, no anticipation, 30 % lapses | 75 | 40 s | 26–57 s | 24,000 | 2 % | bulbs 38 %, darters 28 %, boss 20 %, swoopers 11 % |
| decent | 190–290 ms, 0.12 s anticipation | 105 | 90 s | 62–111 s | 44,000 | 70 % | loop 2+ 65 %, boss 30 % |
| good | 150–220 ms, 0.25 s anticipation | 160 | 122 s | 96–206 s | 53,000 | 98 % | loop 2+ 76 %, boss 24 % |

Almost every death is a bullet (94–97 %), the rest ramming an enemy. In the preview, a thumb held still dies at ~18 s and a lazy side-to-side sway at ~45–50 s, consistent with the first-timer row. A fast check (`a_first_timer_lasts_twenty_to_fortyfive_seconds`, 24 seeds, part of `cargo test -p kit_shooter`) keeps the first-timer median in range when knobs change.

## Determinism

All randomness is from `Rng`: the sim's two generators are seeded from the session seed (`sys::rand_seed` through the game's `Rng`) or `sys::daily_seed`; particles from the game's `Rng`; sound's pitch jitter from its own fixed seed (output only). Time only from `dt` (the stars scroll on the host's own `dt`-summed clock), no hash maps, nothing changes in `render`. The sim test `the_same_seed_and_thumb_replay_the_same_round` checks 3,000 uneven frames bit for bit.

In the preview (`mb serve --watch`, held sessions driven by real pointer events through the runtime's touch path):
- `keeps_the_platform_rules` (cargo test): the round flow is `maimbrain::rounds::Rounds` (no extra ending: the sim's slow-motion death is it); on a fake host a tap starts the round, the score is in before Over, a tap in the card's guard doesn't retry, the best is kept and the same seed replays the same session.
- `mb.verify()` → `{ frames: 6113, match: true, firstMismatch: null }`, hash `cdab24004417e65e`: the title card, the entering touch steering (a scripted lazy sway for 40 s, then hands off), a round to 7,790 with the slow-motion death, the results card, the retry button and more steering. (Before the synthesized effects and the enrage cue: 4,314 frames, hash `7122e7a60f36ed7b`; with the first art, 3,174 frames, hash `53b17252bfef6fba`.)
- Daily mode (checked before the art change; the sim is unchanged): the same thumb path in two separate sessions gave the identical round (51.2 s, 35,535).

Performance (Mac, the watch build without wasm-opt, live frames in the boss fight with a Bloom and escorts on screen): `mb_update` 0.05 ms avg (0.2 max), `mb_render` 0.16 ms avg (0.5 max), ~700–900 `quads2d` (budget ~32,000). `mb_init` ~1 ms (it pre-runs 3 s of the attract mode). `mb build`: `game.wasm` 182 KB, `.mbx` 727 KB, startup set 358 KB. The kit directory (tracked files) is ~870 KB: the atlas 62 KB, the backdrop 70 KB, the icon 27 KB, the music 41 + 77 + 69 KB (mono, 24 kHz, q1), 22 effects 249 KB (32 kHz, q3), sidecars ~80 KB.

## Not verified on a phone

- The feel of relative drag with a real thumb: `DRAG_GAIN` (1.25), `FOLLOW_RATE` (26/s), `MIN_LIFT` (88). The lift eases the ship up only when the thumb is within 70 units horizontally of it.
- Bullet readability at phone size and brightness (hot pink with a dark outline and a white core, rings 17–23 units across, over the backdrop dimmed to ~78 %; the brightest spot is the sun behind the queen).
- Haptics (a tap per kill, heavy on hits and big kills, success on power-ups; ~8 kills/s at peak stays under the platform's ~30/s cap).
- The audio mix, by ear: the loops and effects were checked only by numbers (`check_audio.py`, loop seams clean; `tools/mixsim.py`; each tonal effect's pitch measured against the loop's key). Listen for: the auto-fire (vol 0.075) present but never tiring over a minute; the kill pops reading over the music in a chain without crowding it, and their ladder sounding in key (it should feel like part of the arpeggio); the gem ladder; the hard switch to the boss loop (under the boom) and back; the shield hum (0.09) against the music; and the effects' tails at speed.
- CPU on an iPhone 13 class phone with the boss's Sweep plus a bulb on screen (the busiest moment).
