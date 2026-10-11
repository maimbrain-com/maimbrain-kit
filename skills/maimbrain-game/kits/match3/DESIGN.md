# Poppets

A swap-to-match puzzle for one thumb: a 7×8 board of little watercolor poppets (a coral puffball, an apricot marshmallow square, a butter rice-ball triangle, a sage hexagon with a sprout, a powder-blue cloud; later a lavender star). Swipe one into its neighbour's place to line up three of a kind. Popping keeps the poppets awake; the wake meter drains faster and faster, and when it's empty the whole board nods off.

- **The verb**: swipe a poppet toward a neighbour (tap one, then tap a neighbour, works too). A swap that matches nothing bounces back.
- **The 1-second read**: a board of plump painted poppets with rosy cheeks in front of a bedroom window at night, eyelids drooping, the lamp light fading; then a pop, everyone's eyes snap open. "don't let them doze off".
- **The first 10 seconds**: the tap that enters play deals a fresh board (it pops in cell by cell) and doesn't pick anything up, so the hint shows at once: the two cells of a good swap glow, a ghost thumb slides from one to the other with an arrow, "SWIPE TO SWAP" above the board. The meter is full and drains slowly (an idle round lasts ~19 s, so the first 3 s can't be lost). The first swipe the hint shows always matches: pop, sparkles, "+30", the meter tops up with a white flash, faces brighten. The hint stays until the first match, then returns after 3 s without a touch.
- **Tension and failure**: the meter drains continuously (only while the board is waiting for you, never during cascades) and the drain grows every second. Below 45 % the faces droop (each nods at its own pace, yawning near the end), the lamp's warm glow fades and the room sinks into blue night with stars twinkling in the window, the meter turns yellow then red and blinks, a heartbeat of soft plush thuds ticks, the lullaby winds down like a music box and an uneasy string drone comes in. Saved from nearly empty: "PHEW!". Empty: the poppets fall asleep in a wave from the top, slump, z's float up (1.7 s, still Playing: the sim plays it before it reports `over`, so `Rounds` runs with `.ending(0.0)`), then `Rounds` submits the score, saves the best, shows the game-over card and reports Over. Taps in its first 0.35 s don't retry.
- **The loop**: every 15 s a level, each with something new: 15 s FASTER!, 30 s NEW POPPET! (a sixth kind, the purple star: fewer matches), 45 s ROCKS INCOMING! (grumpy rocks that can't move or match, cracked by a match beside them or a blast), 60 s FASTER!, 75 s STAR POPPETS! (a star badge: a big refill), 90 s FASTER!, 105 s MORE ROCKS!, then faster every 15 s. Specials: 4 in a row → a line blaster (pops its row or column), an L or T → a bomb (3×3), 5 in a row → a rainbow (swap it with any poppet to pop every one of that color). Specials caught in a pop go off and chain. Swapping two specials combines them: line+line a cross, line+bomb three rows and three columns, bomb+bomb a 5×5, rainbow+special turns every poppet of that color into that special and sets them all off, rainbow+rainbow clears the board. Cascades multiply points and refill ("×3 CASCADE!"), each step popping a note higher. No starting matches; always a move (a stuck board reshuffles with a "SHUFFLE!" flourish).
- **The cards**: title: the board plays itself; the poppets nod off, the sky darkens, then a move pops and wakes everyone (loops every ~3 s), with POPPETS, the tagline, BEST and a DAILY toggle. Game over: the sleeping board under a starry sky with z's, NAP TIME!, the points rolling up, BEST / NEW BEST!, SCORES and DAILY. A replay reads from the score, the meter, the faces and the sky.
- **Score**: 10 points per poppet × the cascade step, rocks double, +30 per special made, +100 per star poppet, +200 per special combo. Board 0 (endless) and board 1 (daily: `sys::daily_seed()`, the same board and refills for everyone today, its own best key `daily:<seed>`; `Rounds::daily` keeps both). DAILY and SCORES are the kit's own buttons, offered events before `Rounds`.

## Look and sound

**Soft watercolor storybook.** Hand-painted transparent washes that pool darker at the edges, cold-press paper grain, gentle warm-plum pencil outlines, plump round shapes, a cozy bedtime mood. The full style guide is `art/style.toml`; every generated image gets it, and the first poppet (`art/ref/poppet.png`) is the style reference for the rest.

- **The cast** (`assets/cast.png`, packed by `mb art atlas`, rects in `src/cast.rs`): seven faceless painted bodies with stubby feet, a grumpy pebble and a rainbow-banded puffball, 128 px each (drawn ~46 units wide, `look::SPRITE_SPAN`). Silhouettes differ as much as colors: ball with ears, square with ears, rounded triangle, hexagon with a sprout, scalloped cloud, star, heart.
- **Faces are code** (`draw::face`): ink-oval eyes that shut from the top as the meter drains, a heavy lid line with a lash, rosy cheeks, a pencil mouth that yawns. They have to animate continuously (the droop is the danger tell), which painted frames can't do. `look::FACE_Y` places each kind's face, `FACE_SCALE` sizes them.
- **The room** (`assets/bg.png`, 540×960): the view out of a child's bedroom window at night (crescent moon, clouds, rooftops, curtains, a cream sill). Drowsiness multiplies it toward `NIGHT_TINT`; an additive bedside-lamp glow (`LAMP`) fades out; extra stars twinkle; a shade along the bottom keeps the feed's title readable over the sill.
- **The board**: an indigo wash (`BOARD_WASH`) with paler cells, the generated paper tile (`assets/paper.png`) multiplied over it for grain, and a wobbly two-pass pencil border. The meter, banners, buttons and popups use the same plum ink, cream pencil, coral accent and butter gold (`look::theme`).
- **Code-drawn on purpose**: faces, specials (line arrows and stripe, the bomb's glow ring and fuse spark, the rainbow's halo), blasts, particles, the meter, text. Every painted asset has a code fallback, so the first frames draw before the PNGs arrive.
- **Music**: a dreamy lullaby (music box and celesta over soft strings and felt piano, F major, 8 bars at 84.3 BPM, `assets/music.ogg`) and a separate tension layer (`assets/tension.ogg`: a low string drone with slow tremolo, free-length 12.4 s loop). As the poppets get sleepy the drone fades in (it came out as an E minor 7 chord, so it plays two semitones down, `TENSION_PITCH`, as D minor 7: F major's relative minor) and the lullaby slows by up to 4.5 % (`WIND_DOWN`, the drone with it), a music box winding down.
- **Effects** (ElevenLabs, all short and soft): soft bubbly pops that rise a semitone with each cascade step (varied, at most 3 at once, mixed well under the lullaby) plus a celesta chime from the second step that climbs F major's pentatonic notes, a marimba tap to pick up, a plush swish to swap, a rubbery boing for a bounce, plush thuds for the drowsy heartbeat, sparkles and harp sweeps for specials, a music-box fanfare for a level and a new best, a long yawn when the danger starts and a tiny snore and sigh when they fall asleep.

Generated with `mb art`/`mb music`/`mb sfx` (Gemini Nano Banana 2.1, Lyria 3 Clip, ElevenLabs sound effects) for about $0.76 including discarded variants. Each asset's prompt, seed and recipe is in its sidecar (`assets/*.gen.json`, `icon.png.gen.json`); the packed sprites' sidecars stay too, so `mb regen assets/p_orange.png` works though the PNG is gone. `tools/regen.sh` rebuilds everything.

## Tuning knobs

All in `src/sim.rs`, top of file (`// ---- Tuning knobs`). The ones that matter most:

| Knob | What it does |
|---|---|
| `DRAIN_START` (0.042/s) | Meter drain at the start. Raises the floor: slow players die sooner. |
| `DRAIN_GROWTH` (0.0016/s²) | How fast the drain grows. This sets how long good players last. |
| `DRAIN_MAX` (0.3/s) | The drain stops growing here; lower it and great players last forever. |
| `GAIN_PIECE` (0.0175) | Refill per poppet popped. Every round gets longer when raised. |
| `GAIN_CHAIN` (0.5) | Extra refill per cascade step (rewards setting up cascades). |
| `GAIN_MADE`, `GAIN_ROCK`, `GAIN_STAR` | Refill for making a special, breaking a rock, popping a star. |
| `START_COLORS` (5) | Kinds at the start. 4 is a cascade festival, 6 is hard to read. |
| `LEVEL_TIME` (15 s) and `news()` | Level length and what each level brings. |
| `ROCK_CHANCE`, `ROCKS_MAX`, `STAR_CHANCE` | How often blockers and star poppets drop in. |
| `SWAP_TIME`, `POP_TIME`, `FALL_G`, `LAND_SETTLE` | Animation pacing the sim waits for: snappier = more moves per minute. |
| `DROWSY`, `DROWSY_FROM` | Where the danger cue and the drooping faces start. |

Feel and look live elsewhere: `look.rs` (title, words, colors, cell size, board position), `lib.rs` top (`SWIPE_MIN`, `HINT_IDLE`, `DEMO_WAIT`), `sound.rs` (mix levels, the scale).

## Make it yours

Three changes turn this kit into your own game. Do all three.

1. **A new cast and world** (`art/style.toml`, `tools/regen.sh`, `look.rs`). Restyle everything by editing the style guide and regenerating:
   1. Edit `art/style.toml`: `style`, `keywords`, `palette`, `avoid`, and the `[music]`/`[sfx]` words. Set `references = []` for now.
   2. `tools/regen.sh hero` (with `MB="mise exec -- cargo run -q -p mb-cli --"` in the Maimbrain repo) and look at `art/preview/p_coral.png`. New character? `mb regen assets/p_coral.png --prompt "…"`; another take: `--new-seed`.
   3. Copy the one you like to `art/ref/poppet.png` and set `references = ["art/ref/poppet.png"]` again.
   4. `tools/regen.sh all`: the rest of the cast (each with its recorded description; change one with `mb regen assets/<name>.png --prompt "…"`), the atlas, the window, the paper, the icon, the music and every effect. About $0.50. Look at every `art/preview/*.png`.
   5. In `look.rs`: names and words, `PIECES` (each kind's color, matched to its painted body), `FACE_Y` (where each face sits), the night tint, lamp, board and theme colors. In `sound.rs`: `TENSION_PITCH` (put the drone in the music's key: the generators ignore the key you ask for, so measure it) and the mix levels. Check the mix with the skill's `scripts/mix_check.py` on a bot round: `MB_MIX_LOG=/tmp/m3.log cargo test -p kit_match3 mix_log`, then `python3 <skill>/scripts/mix_check.py assets /tmp/m3.log` (it reports the music's key, each sound's level against the music and anything out of key).

   Ideas:
   - Fruit in a blender: matches squeeze juice into a glass that's your meter; a "brain freeze" when it empties.
   - Fish in an aquarium: bubbles pop; the meter is oxygen; murky water instead of night.
   - Space gems mined by a little robot; the meter is battery; it powers down instead of falling asleep.
   - Pets at a shelter: match three to get them adopted; they cry instead of yawn.
   - Monsters at a sleepover: the opposite fiction, matches keep them quiet before the parents wake.
2. **A twist on the verb or a rule** (`sim.rs`: `swap_valid`, `find_groups`, `resolve`, `collapse`). Ideas:
   - Gravity sideways or upward (change `collapse` and the spawn side; `fall_y` works for any axis).
   - Diagonal swaps allowed, or only swaps that match 4+.
   - Move limit instead of a meter: N swaps per level, a target score to advance.
   - Matches must include a "key" poppet; everything else is filler.
   - Pieces age: unmatched poppets turn to rock after K turns (add a counter to `Piece`).
3. **One new mechanic, piece or power-up** (`Kind`/`Special` in `sim.rs`, its look in `draw::piece`, its cue in `sound.rs`). Ideas:
   - Ice: a poppet frozen in place until a match next to it thaws it (like rocks, but matchable).
   - A ticking bomb poppet that must be cleared within N moves or it drains a chunk of the meter.
   - A "magnet" special that pulls one color into a column.
   - Bubbles that float up instead of falling, clearing the top when they reach it.
   - Boss rows: a big character on top that takes damage from matches under it.

## Difficulty (bot)

`cargo test -p kit_match3 --profile bot -- --ignored --nocapture difficulty` (200 seeded rounds per preset, seeds 1000–1199, through `maimbrain::bot::Difficulty`: round length p10/p50/p90 against the targets, score, swaps/min, best cascade, and the level each round fell asleep in; then one traced first-timer round). The bot plays the sim through the same `swap` the host uses. It models a person scanning a board: a reaction delay after the board settles (0.15–0.35 s), then think time to scan; each scan spots each available move only with some probability (more on later scans), judges a move's worth with noise (and, for less skilled players, without valuing specials), sometimes misreads and swaps two things that don't match (a bounce), then takes motor time to swipe. Skilled players anticipate: part of their scan happens while the previous cascade is still falling. It sees only the first step of a move (what a person sees), not the refills.

| Preset | Think | Spots | Misreads | Median round | p10–p90 | Swaps/min | Falls asleep in |
|---|---|---|---|---|---|---|---|
| first-timer | 1.8–3.6 s | 30 % | 15 % | 36 s | 26–47 s | 14 | level 3 mostly (30–45 s, just after the new color) |
| decent | 1.0–2.0 s | 50 % | 6 % | 58 s | 46–70 s | 22 | levels 4–5 (rocks) |
| good | 0.45–1.0 s | 80 % | 1.5 % | 133 s | 99–160 s | 36 | levels 8–11 |

`a_first_timer_lasts_20_to_45_seconds` (24 seeds, runs in the normal `cargo test -p kit_match3`) keeps the first-timer median in band. Other rule tests: no starting matches and always a move, the first seconds can't be lost (idle round ≈ 19 s), match → pop → score → refill → refilled board, bounce leaves the board untouched, 4/L/5 make line/bomb/rainbow, a line sets off another, rainbow+poppet, bomb+bomb 5×5, rocks, cascades multiply, stuck boards shuffle, the level schedule, the demo never ends, and the same swaps replay the same round (uneven frame times). `keeps_the_platform_rules` runs the SDK's conformance check on the whole game (a tap starts the round, the score goes in before Over, a frantic tap doesn't retry, the best is saved and read back, the session replays), with a few blind swipes so the round scores.

## Determinism

- All randomness from `Rng`: the sim's from `Rounds::seed()` (from `sys::rand_seed()`, or `sys::daily_seed()` in daily mode), the title's demo boards from the host's `Rng`; particles from the host `Rng`; sound jitter from the sound module's own fixed seed (output only). Time only from `dt`; no hash maps; `render` only reads.
- `mb.verify()` in the preview (measured before the round flow moved to `Rounds`, which seeds rounds differently): **match: true over 3688 held frames**, hash `0436752eda73ebfe` (the scripted session: title, start, 20 tap-tap swaps, a full idle round to sleep, the card, a retry, 6 more swaps; 3688 frames).
- `mb.stats()`: `mb_update` avg < 0.01 ms, `mb_render` avg 0.4 ms (max 0.6), ~4.7 k mb2d quads mid-round (painted bodies are one quad each), on an M-series Mac.

## Not verified on a phone

- Swipe feel: `SWIPE_MIN` (14 units) and the swap timing with real touch latency; whether tap-tap is discoverable.
- Haptics density during long cascades (pops, blasts and level-ups can stack; the platform caps ~30/s).
- The generated audio by ear: the lullaby's melody and loop seam (`mb music` reports a clean seam, match 0.97; `check_audio.py` passes), whether the tension drone sits well two semitones down against it (measured in key, not heard), how the 4.5 % wind-down sounds, and the effects' character (picked from their envelopes: the pop, select and UI click hit in the first 50 ms; the swap swish peaks at 100–200 ms; the yawn and snore are 1.2 s and 2 s).
- Audio balance by ear. Measured on a decent bot's 60 s round (mix_check.py): the lullaby sits a median +4.6 dB over all effects together (effects louder 30 % of the time, was 51 %); pops 8.5 dB under it, the limiter never pulls more than 1.4 dB.
- The watercolor at phone resolution: the window is 540×960 (1.5 texels per unit) and quantized to 96 colors; the sprites are 128 px for ~46 units (about 2.8 per unit, a little soft on a 3× phone).
- Contrast of the painted bodies on the indigo board in a bright room; the sleepy lids at arm's length.
- CPU on an iPhone 13 class device (≈ 0.4 ms render on a Mac; ~4.7 k quads).
- Readability of the six shapes at arm's length, and of the drowsy faces as the danger tell.
