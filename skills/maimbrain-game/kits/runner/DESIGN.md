# Bun Run

The runner kit: a bread bun in red sneakers runs by itself through a meadow; you keep it alive. Tap to jump, hold to jump higher, tap again in the air for a double jump. Pure 2D, hand-written kinematics, chunky generated pixel art and a chiptune soundtrack.

- **The verb**: tap (and hold). Holding is the same verb, longer; the double jump is a second tap.
- **The 1-second read**: a bun with a face running toward a crate, coins arcing over it, "BUN RUN / how far can a bun go?". The card is the game playing itself (an autopilot that skips the warm-up, so there's always a jump coming), never instructions.
- **The first 10 seconds**: the tap that enters play is the first jump. 3.3 s of open ground with a row of 9 coins at running height: the first coin lands in under a second whatever the player does, the full row pops "NICE!" with stars. Until the player jumps on purpose, "TAP TO JUMP" bobs over a dotted hop that arcs over whatever is next, with a thumb ring pulsing at the bottom middle. At 3.3 s the first crate (low, small, a wide 0.34 s press window, coins along the perfect jump). Then more crates; at 15 s the first pit.
- **Tension and failure**: obstacles come at you; the bun's face goes worried when one is under 0.45 s away; flying ones get a pulsing "!" at the right edge before they enter. Each new family arrives with a ribbon ("PITS!", "TALL STACKS!", "HUNGRY GULLS!"…) and its own ≤5-word hint while the first one approaches ("JUMP THE GAP", "HOLD TO JUMP HIGHER" with chevrons over the stack, "STAY LOW!", "TIME IT!"). Hitting something: hit-stop, a white flash, stars, a heavy haptic, then the bun flips through the air, bounces twice and lands on its back with X eyes and stars circling (1.5 s, still Playing). Missing a pit: a slide-whistle, and it ends up jammed head-first in the hole, sneakers kicking. Narrow escapes (≤9 px) pop "CLOSE! +10" with 0.3 s of slow motion.
- **The loop**: rounds of 30–40 s for a first-timer, 2–3 min for a good player (bot, below). Speed +15 px/s every 12 s ("FASTER!" with speed lines); a new family every 15 s (crates, pits, tall stacks, gulls you must not jump into, bobbing bees, fast low gulls, long logs); the press window tightens from 0.34 s to 0.17 s at 90 s and on to 0.12 s. Never two hard ones in a row. The sky drifts day → dusk → night → dawn every 260 m.
- **The cards**: title: the bun running and jumping by itself under the bouncing title, BEST pill, a DAILY toggle. Game over: the aftermath (the bun on its back by the crate, or kicking in the pit), "BONK!" / "WHOOPS!", the score rolling up, BEST or a NEW BEST! ribbon with confetti, "N MORE TO BEAT IT" when close, the distance in meters, SCORES and DAILY buttons; any tap outside them retries. Replays read from the bun, the score and the faces.
- **Score**: 1 point per meter + 5 per coin + 10 per close call, rolled up big in the HUD; coins in a pill top-right. Best saved (`best`, and `daily:<seed>` for today's daily), submitted to board 0 (endless) or 1 (daily).

Daily mode seeds the course with `sys::daily_seed()`: the same obstacles for everyone today, on its own board and best.

## Look and sound

- **Art direction**: chunky 16-bit console-platformer pixel art (original designs): bold dark outlines, two or three flat shading steps, round friendly shapes, a bright saturated sunny palette. The style guide is `art/style.toml` (style sentence, keywords, avoid list, a 25-color locked palette, `pixel_art = true`, `pixel_scale = 3`, `assets/hero_run.png` as the style reference for everything after it).
- **Pixel grid**: every art pixel is 1.5 units = exactly 3 canvas pixels (3 texels per art pixel, drawn at their size), so the screen is a classic 240 art pixels wide and nothing is resampled. `look.rs` snaps sprite positions to half units. Crates and logs are any size the generator picks, so they're drawn *sliced*: frame and corners at size, middles repeated, never stretched (`CRATE_X/Y`, `LOG_X/Y`).
- **What's generated** (`tools/regen.sh`): the bun (a 4-frame run cycle; jump, fall, tuck for the double-jump flip, bonked), crate, log, gull and low swift (4-frame flaps), bee (2 frames), coin (6-frame spin), sun, moon, three clouds, soil and grass tiles, three parallax layers (far mountains, low hills, a near hedge of trees), the icon. Sprites and tiles are packed into `assets/sprites.png` (rects in `src/sprites.rs`); the layers load on their own.
- **What code draws**: the banded sky (so it can drift day → dusk → night → dawn), stars, the grass fringe, blades and flowers on the ground line, the pit, the sweat drop and dizzy stars, dust (square pixel puffs), particles and the UI. The time of day tints each layer and the props (`Sky::far/mid/near/cloud/ground/prop`) and hazes the far layers toward the horizon.
- **UI**: the kit's candy theme in the art's ink, red and gold, deep-blue panels, small radii, and the pixel font for the title, score and ribbons.
- **Music**: an upbeat chiptune loop (square lead hook, octave bass, arpeggios, chip drums; 8 bars at 150 BPM) from `mb music --layers`: `music_base` (bass and drums) and `music_hi` (lead and arpeggios). It was asked for in C major but came out in E, borrowing from both E major and E minor (the bass walks E, C#/C, B, A; the lead has G and G#). The high stem plays at 60 % from the start and swells to full with speed and danger (`HI_FLOOR`, `sound.rs`).
- **Effects**: designed with `mb sfx`: boings for the jump and double jump, a near-miss whoosh, a crate bonk, a slide whistle into a pit, squelch when stuck, the speed-up zip, game over and a new best. The four tonal ones you hear during play are synthesized in the music's key by `tools/gen_sfx.py` (free, offline), using only E, F#, A and B (the notes E major and E minor share): the coin (a soft, round 0.12 s triangle blip, B5 to E6), the "collect streak" sparkle for a full coin arc, the 100 m fanfare and the new-family alert.

### The mix

The music is the reference: both stems at `MUSIC_VOL` 1.0 come to about -24 LUFS, and every effect's level (`VOL_*` in `sound.rs`) is set against it. Coins are the most frequent sound by far (about 120–150 a minute in a bot round, in bursts of 5 to 9 roughly 0.1 s apart, because they sit along the jump arcs), so they carry the least energy: about 9 dB under the music over 3 s. Each one is short and bright, though, and peaks over half as loud as the music on a phone speaker, so it's heard (at 13 dB under, the old level, it wasn't). A coin less than `COIN_GAP` (0.085 s) after the last coin sound is folded into it, at most two coin voices ring at once (`CAP`, the oldest stopped), each coin varies by ±1.5 dB and ±0.6 % pitch, and a streak climbs `LADDER` (0, 5, 7 semitones), gets a little quieter with each step and starts again at the next streak. When the arc's last coin is taken, the soft sparkle plays as the one "streak" sound. Jumps are about 10 dB under the music, CLOSE!, a new family, a milestone and FASTER! 9–11 dB under it, and only the end of a run (the bonk, the slide whistle) goes over it, while the music fades out.

To check the mix after changing a sound or a level (you can't listen to it here), render a bot round's sound offline with the real files: `MB_MIX_LOG=/tmp/mix.log cargo test -p kit_runner --release -- --ignored mix_log`, then `python3 <skill>/scripts/mix_check.py assets /tmp/mix.log` (`MB_MIX_SEED`, `MB_MIX_SKILL` pick the round). Before this pass, on decent seed 11: the effects were 6 dB *over* the music in the median 400 ms window (louder 66 % of the time), coins alone 8 dB over it (60 % of all effect energy, 6 voices at once, 3.9 kHz bright, pitched up to 2.24× and off key). After: the music sits 6 dB over all the effects in the median window (p10 +1 dB, effects louder 4 % of the time), coins 9 dB under the music over 3 s and 26 % of the effects, each peaking 9 dB under the music's loudness on a phone (the check wants at least −10: half as loud) and 13 dB clear of it in their own band, at most 2 coin voices, 1.9 kHz, on key, and the master limiter never pulls more than 0.5 dB. Still flagged as hard to hear on a phone: the jump boing (12 dB under), the landing thump (mostly under 300 Hz, which a phone speaker barely plays) and the new-family alert; they need brighter takes, not just more level.

## Tuning knobs

All in `src/sim.rs`, the block at the top. The ones that matter most:

| Knob | Now | What it does |
|---|---|---|
| `WINDOW_START` / `WINDOW_END` / `MIN_WINDOW` | 0.34 / 0.17 / 0.12 s | **The difficulty curve.** Every obstacle is resized until the press window that clears it is about this long (wide at the start, `WINDOW_END` at `RAMP_TIME`, then on down to `MIN_WINDOW`). Raise them all for a gentler game. |
| `RAMP_TIME` | 90 s | When the window reaches `WINDOW_END` and gaps reach `GAP_END`. |
| `START_SPEED` / `SPEED_STEP` / `SPEED_EVERY` / `MAX_SPEED` | 235 / 15 / 12 s / 385 px/s | How fast, how often "FASTER!". Faster also means less look-ahead time (250 px ahead of the bun). |
| `FAMILY_EVERY` | 15 s | A new obstacle family joins this often (`Kind::ORDER`). |
| `GAP_START` / `GAP_END` | 1.15–1.85 / 0.6–1.05 s | Open ground between obstacles, in seconds of running. |
| `JUMP_VY`, `GRAVITY_HOLD`, `GRAVITY` | 760, 1800, 2600 | The jump: a full hold peaks at ~157 px after ~0.42 s, lands at ~0.77 s. Lower `GRAVITY` = floatier falls. |
| `CUT_VY`, `MIN_HOLD`, `MAX_HOLD` | 430 px/s, 0.11 s, 0.42 s | Variable height: letting go cuts the rise to `CUT_VY`; a quick tap counts as `MIN_HOLD` (a ~109 px hop that clears any crate); holding past `MAX_HOLD` adds nothing. |
| `DOUBLE_JUMP`, `DOUBLE_VY` | on, 560 px/s | The air jump. It's also the safety net good players use to save early jumps; raising `DOUBLE_VY` makes the game a lot easier. |
| `WARMUP` | 3.3 s | Open ground before the first obstacle (the unfailable start). |
| `CLOSE_PX` | 9 px | How close counts as CLOSE!. |
| `COIN_POINTS`, `CLOSE_POINTS`, `PX_PER_M` | 5, 10, 30 | Scoring. |
| `HERO_X` | 110 | Where the bun runs on screen; everything right of it is look-ahead. |

Obstacle sizes live in `Obstacle::resize` (the min/max per family the generator may resize within) and in `spawn_one` (starting sizes, gull speeds, the bee's bob). Host-side feel (slow-mo on CLOSE!, hint timing) is at the top of `src/lib.rs`.

## Make it yours

Three changes turn this kit into a game of its own. Do at least one of each.

1. **A new world and hero**: the words, sky colors, layer tints and how things are drawn are in `src/look.rs` (`TITLE`, `TAGLINE`, the game-over headings, `kind_name`/`kind_hint`, the `DAY`/`DUSK`/`NIGHT`/`DAWN` skies, `theme()`, `draw_hero`, `draw_obstacle`, `draw_backdrop`, `draw_ground`). The art and sound are generated, so a new world is mostly prompts:
   1. Edit `art/style.toml`: the `style` sentence, `keywords`, `avoid`, the `palette` (darkest first; it's locked), the `[music]` and `[sfx]` words. Keep `pixel_art`/`pixel_scale = 3` for pixel art, or set `pixel_art = false` for painted art (then sprites are resampled: drop the half-unit snapping if you like).
   2. Edit the subjects in `tools/regen.sh` (hero, obstacles, sky, ground, layers, music, effects; keep each frame list in the order `look.rs` reads it) and run `sh tools/regen.sh` (about $1; `art`, `music` or `sfx` to do one part). Clear `references` in the style guide first if the hero changes, then point it at the new `assets/hero_run.png` once that looks right. New music comes out in whatever key it likes: `mix_check.py` (below, under The mix) prints it; then set the notes in `tools/gen_sfx.py` to that key's notes and rerun it, and keep `LADDER` in `sound.rs` on them.
   3. Open every `art/preview/*.png`; redo what's off with `mb regen assets/<name>.png --new-seed` (or `--variants 3` and `mb art pick`), then rerun the atlas line of `regen.sh`. If a sprite's proportions change, adjust its offsets in `draw_obstacle`/`draw_hero` and the crate/log slices.
   4. `python3 tools/check_audio.py assets/*.ogg` (or the skill's `scripts/check_audio.py`) and `mb art check .`, then look at it in `mb serve`. Ideas:
   - a robot vacuum fleeing across a kitchen floor: crates → toys, pits → stairs, gulls → the cat's paw, bees → a swinging ceiling fan cord;
   - a penguin sliding on ice at night (aurora sky keyframes), pits → cracks in the ice, gulls → skuas;
   - a skateboarding snail in a city: hydrants, open manholes, pigeons;
   - a ghost running through a haunted hallway (DUSK/NIGHT only), candles, trapdoors, bats;
   - a tiny astronaut on the moon with low gravity (`GRAVITY` 1500): craters, rocks, meteors.
2. **A twist on the verb or a rule** — `src/sim.rs` (`press`, `release`, `Hero::fall`, `tick`):
   - gravity flip: a tap flips the hero to the ceiling (a second ground line), obstacles on both;
   - no double jump but a slide (hold = duck under) for the high gull, taps jump;
   - lanes: the ground is two levels, a tap hops up or down a level (Kind::Pit becomes a missing floor);
   - momentum: each perfect arc (`Cue::ArcDone`) speeds you up for bonus points, a crate hit costs speed instead of the run (3 hearts via `ui::Hud::lives`);
   - one-button rhythm: obstacles land on the music's beat grid (see SKILL.md "Rhythm games");
   - the double jump recharges only by collecting a coin.
3. **One new mechanic, obstacle or power-up** — add a `Kind` (geometry in `Obstacle`, a `holds()` entry so plans and coin arcs work, a `resize` rule, an entry in `Kind::ORDER`), draw it in `look.rs`, name and hint it:
   - a springboard pad that launches you high over a wall of crates;
   - a magnet power-up (coins fly to you for 5 s);
   - a rolling barrel coming toward you (like `Skimmer`, on the ground);
   - a low ceiling you must not jump into (like `Gull`, but wide);
   - a shield bubble that absorbs one hit (the tumble becomes a pop);
   - a boss gull that dives in a telegraphed arc every 60 s.

The generator stays fair whatever you add: it measures each obstacle's press window with the real jump (`plan`, `clears`) and resizes it, and `every_course_can_be_run_by_a_careful_player` checks it.

## Difficulty (bot)

`cargo test -p kit_runner --release -- --ignored --nocapture difficulty` (200 seeded rounds per preset). The bot (`src/tests.rs`) plays like a person: it notices an obstacle when it's within its view (170–250 px; flying ones also ~0.9 s before they arrive, as the "!" warns), plans the press from what it sees then (`sim::plan`, the same measurement the generator uses), and presses no sooner than its reaction delay (0.14–0.35 s), with a lateness bias and timing noise (σ ≈ 63 / 46 / 32 ms), and holds with an error of σ ≈ 20 / 12 / 6 %. In the air it can't plan the next one until it lands. After each jump, a reaction delay later, it checks whether the jump will work and, with some probability, saves it with the double jump. Some players jump at gulls they should run under (35 / 12 / 3 %).

| Preset | Median round | p10–p90 | Median score | Median distance | Dies on (of 200) |
|---|---|---|---|---|---|
| first-timer | 34.5 s | 7.7–62.8 s | 640 | 286 m | crates 133, gulls 31, stacks 20, bees 13, pits 2, logs 1 |
| decent | 96.4 s | 34.5–123.7 s | 2013 | 923 m | crates 92, stacks 38, bees 30, gulls 27, logs 13 |
| good | 175.9 s | 106.6–323.0 s | 4189 | 1926 m | logs 76, stacks 51, crates 42, bees 23, gulls 4 |

The fast test `first_timers_last_20_to_45_seconds` checks the first-timer median on 24 seeds (the whole suite runs in about a second in debug). A careful player (the autopilot pressing within ±0.03 s of the plan) survives 150 s on every tested seed (`every_course_can_be_run_by_a_careful_player`; a 300 s probe run survived too): no course is unwinnable. Pits are the gentlest family (wide windows); they read as danger more than they kill. The p10 of 7.7 s for first-timers means roughly one round in ten ends at one of the first two crates.

## Determinism

All randomness comes from `Rng`: the course from the round's seed (`Rng::from_host` per session, `sys::daily_seed()` in daily mode), particles from a seed drawn from it; sound jitter uses its own fixed seed and never feeds back. The sim advances in fixed 1/120 s ticks accumulated from `dt`. Nothing changes in `render`.

`await mb.verify()` in the preview, over a held session of 2765 frames with scripted taps (the title card, a round crashing into a crate, the NEW BEST card, a retry from the card, a second crash and its card): `{ match: true, firstMismatch: null }`, hash `f01f9bfe8fc468c6`. Loading images doesn't touch the sim; until the atlas is ready its sprites just aren't drawn. `the_same_seed_and_taps_replay_the_same_run` checks the sim natively with uneven frame times.

Budgets (preview on an M-series Mac): `mb_update` ≈ 0.01 ms and `mb_render` ≈ 0.05 ms average, ~400–650 mb2d quads a frame; `game.wasm` 166 KB, the `.mbx` 574 KB (startup set 343 KB); the kit directory is ~735 KB. The sprite sources stay in `assets/` next to the atlas (about 20 KB) so `mb regen` and their sidecars keep working.

## Not verified on a phone

- How the jump feels under a real thumb: tap vs hold timing (`MIN_HOLD`, `CUT_VY`, `MAX_HOLD`) and whether the 250 px of look-ahead at `MAX_SPEED` is enough on a small screen.
- Haptics (a tick per jump, success on CLOSE!, heavy on a hit), and that ~30/s isn't approached.
- Audio balance and character, none of it heard by ear yet (only measured, above): whether the coins are still easy to hear under the music on a phone speaker (raise `VOL_COIN` a little if they vanish; the rule is to stay at least 8 dB under the music), whether the synthesized coin, sparkle, fanfare and alert sit well next to the generated chiptune, every other effect's feel, the slide whistle. The game-over jingle (`over`, around D#) is off the music's key, but it plays after the music has stopped.
- `check_audio.py` flags `music_base.ogg` (seam 4.1× its first-2000-sample average step), but the seam step (0.0072) equals the waveform's own slope there (~0.0067 per sample): a fast-rising bass note, not a click. Listen to the loop point on a phone.
- The pixel art on a real screen: canvas pixels are exact on a 720-wide canvas; the squash/stretch and the flip resample the hero (on purpose).
- Whether the bun (about 52 × 50 units, a seventh of the width) reads well enough: a runner needs the space ahead, so the hero is far under the skill's "a third of the screen". If it's too small, raise everything's scale together (hitbox constants in `sim.rs` and the drawing in `look.rs`) and lower the speeds.
- Safe areas and the card overlays on a notched phone (laid out with `Layout`, checked only with `?overlays` in the desktop preview).
