# Picnic Pile

The stacker starter kit (`maimbrain = { features = ["physics2d"] }`). A delivery bird flies back and forth over a plate balanced on a pole, carrying snacks with faces. Tap to let go. Every snack is a real rigid body (deterministic Rapier, docs/PHYSICS.md): how it lands, wobbles, wedges or tumbles is never scripted.

- **The verb**: tap (the bird lets go of the snack).
- **The 1-second read**: a wobbly tower of smiling snacks on a tiny plate, a bird bringing one more. "how high can lunch go?"
- **The first 10 seconds**: the tap that enters play drops the first snack, a slice of toast held dead centre by a bird that hasn't started moving, so it always lands (+1, NEAT!, chime, sparkles). The bird swoops in with the next one and starts drifting, slowly at first (the sweep eases in from zero). A dotted path and a shadow of the snack show exactly where it would land right now; "TAP TO DROP" and a pulsing ring sit on the snack until the player has dropped three (and come back after 3 s idle). A random tapper can't lose a heart in the first 3.5 s (tested).
- **Tension and failure**: the bird sweeps wider and faster with every snack; the snack keeps a little of the bird's speed (the ghost includes it). Snacks rest on each other by friction alone, so an off-centre drop leans the tower, and a lean grows. Faces show it: excited in the bird's feet, scared in the air, content on the tower, worried brows and a sweat drop when the tower sways or gets hit, dozing once they're deep in the base, X eyes on the blanket. A snack that falls off costs one of three hearts ("OOPS!" for a miss, "TUMBLE!" when the tower sheds; a whole tumble costs one heart). On the last heart the screen pulses red. Losing it makes the plate tip: everything slides off onto the picnic blanket and the bird flies away (2 s), then the game-over card shows the pile.
- **The loop**: a first-timer's round lasts ~35 s, a good player's ~65 s (bot, below). Something new about every 15 s: a new snack joins at 3, 6, 10, 15 and 21 stacked (jelly, baguette, cheese, macaron, tomato, each introduced by name and carried next), and from 16 s a twist every 11–16 s, alternating a wind gust (announced "WIND!" with an arrow, streaks and leaves; pushes the tower and the falling snack, and the ghost shows the drift) and a tilting plate ("WOBBLE!", arrows at the plate's ends). Each twist is 20 % stronger than the last. Ten in a row wins a heart back. The sky turns golden, then sunset, then night as the tower climbs.
- **The cards**: title: the bird stacks by itself (an autopilot that drops every fifth snack sloppily, so the tower wobbles and sometimes tumbles), the name, the tagline and the best. Game over: the tipped plate and the dizzy pile on the blanket under SPLAT!, the count rolling up, BEST or NEW BEST! with confetti, "N MORE TO BEAT IT" when close; SCORES and DAILY buttons. A replay reads from the score, hearts, faces and banners alone.
- **Score**: snacks stacked (a snack counts once it rests on the tower for 0.3 s). Board 0; daily mode (same snacks and twists for everyone today, `sys::daily_seed()`) has board 1 and its own best.

## Look and sound

- **Look**: a paper cut-out collage. Every picture is cut from matte craft paper with slightly irregular scissor edges and visible fibers, a few layers deep, flat colors and no drawn outlines, in bright noon sun (`art/style.toml`: the style sentence, the avoid list and the eight-color palette, plum ink to cream paper). Snacks are faceless paper shapes with a separate sheet of eight cut-paper expressions laid over them (`faces.png`: happy, blink, excited, scared, worried, very worried with a sweat drop, dozing, X eyes). The bird is a cream paper bird with four flapping frames whose feet grip the snack. Behind: rolling paper hills with trees (parallax), a flowery meadow, a gingham blanket, paper clouds and a turning paper sun, all over a multiplied paper-grain texture. Code adds what moves: soft two-step drop shadows under every paper piece (always cast down-right on screen however a snack turns), the sky's hour (noon, golden, sunset, night with stars), wind streaks and spinning leaves, the landing ghost, particles and the UI kit in the palette's colors.
- **Honest physics**: each snack picture is stretched exactly over its collider (`Kind::size()`), and the colliders follow the art: the baguette is a rounded bar (`Kind::radius` 9) and the cheese a trapezoid whose top is 46 % of its bottom.
- **Sound**: a jaunty acoustic loop (ukulele, whistled melody, shaker and woodblock, 112 BPM, 8 bars) with a 4-bar tension layer (nervous pizzicato, ticking woodblock, tremolo cello) locked onto it (`tools/align_loop.py`) that rises in twists, when the tower sways and on the last heart. Effects are designed (ElevenLabs): soft paper and food thuds scaled by impact (four voices at most: a collapse doesn't stack twenty), a marimba note that climbs G major's pentatonic notes (the music came out in G major) with the streak and starts over after six, a kalimba twinkle for NEAT, a crowd "ooh" for near-tumbles (a PHEW landing, or a hard sway), a whoosh for gusts, a wooden creak for the tilt, a slide-whistle plop for a lost snack, a comic collapse slide, a muted-trumpet "wah wah" game-over sting and a whistle-and-cheer for a new best.

## Files

| File | What |
|---|---|
| `src/sim.rs` | All rules and physics, the tuning knobs at the top, `Cue`s out. No host calls. |
| `src/look.rs` | Every word, name and color, each snack's pictures, and the UI theme: what a reskin changes first. |
| `src/draw.rs` | The art on screen: sky, hills, meadow, plate, snacks (`snack()`), faces (`face()`, `mood()`), the bird, the landing ghost, wind. |
| `src/sprites.rs` | Source rects in `assets/sprites.png`, written by `mb art atlas` (don't edit). |
| `src/sound.rs` | Cues → sounds (varied, voice-capped, landings climb the pentatonic in G), the music and its tension layer, haptics. Check the mix with `MB_MIX_LOG=/tmp/st.log cargo test -p kit_stacker mix_log` and the skill's `scripts/mix_check.py assets /tmp/st.log`. |
| `src/lib.rs` | Title → Play → Over, input, round reporting, best/daily, cards, HUD, juice. |
| `src/tests.rs` | Rule tests and the bot. |
| `art/style.toml` | The style guide every `mb art` call uses (style, palette, music and effect words). |
| `art/sprites/` | The generated pictures before packing (PNGs git-ignored; their `.gen.json` sidecars kept), `art/ref/` the style references. |
| `tools/regen.sh` | Every `mb art`/`mb music`/`mb sfx` command with its prompt; regenerates, packs and checks all assets. |
| `tools/quantize.py`, `tools/align_loop.py` | Used by regen.sh: indexed PNGs for the atlases; the tension layer locked onto the music (stdlib + `mb ogg`). |

## Tuning knobs

At the top of `src/sim.rs`, in order of how much they matter:

| Knob | What it does |
|---|---|
| `PLATE_W` | Width of the plate (156 px). Narrower is harder at once. |
| `SWEEP_AMP` | Bird's sweep half-width: start, per snack, max (14 → 88 px). Wider sweeps mean more to judge. |
| `SWEEP_SPEED` | Bird's sweep speed in rad/s: start, per snack, max (1.3 → 1.8). |
| `CARRY` | How much of the bird's speed a dropped snack keeps (0.2). Higher makes landings skid. |
| `GRAVITY` | 700 px/s² (real is 981): softer landings, slower, more readable topples. |
| `GRIP`, `GRIP_TIME` | A landing snack's motion is damped hard for 0.25 s so it settles instead of skidding. 0 is pure physics (much harder). |
| `SOLID_BELOW` | Snacks this far below the top, once still, are held to the plate: only the top ~3 snacks can topple. Lower is more forgiving. |
| `LIVES`, `TUMBLE_GRACE`, `HEART_BACK` | 3 hearts; falls within 2.5 s of a lost heart are the same tumble; 10 in a row wins one back. |
| `UNLOCKS` | When each snack shape joins (and so how often something new appears). |
| `TWIST_FIRST`, `TWIST_GAP`, `GUST_ACCEL`, `TILT_MAX`, `TWIST_GROWTH` | When twists start, how often they come, how hard the wind pushes and the plate tilts, how fast they grow. |
| `RESPAWN` | Seconds until the bird has the next snack (tempo). |
| `FRICTION` | Grip between snacks (0.9). |

Shapes and sizes are `Kind::size()` / `Kind::shape()` in the same file.

## Make it yours

Three changes turn this kit into a game of its own. Do all three; the order doesn't matter.

1. **A new theme, characters and names** (`art/style.toml` and `tools/regen.sh` first, then `look.rs`):
   - Rocks with sleepy faces stacked into a cairn on a river stone, a heron as the dropper.
   - Presents on a sleigh, dropped by a reindeer; snow at night.
   - Sushi on a conveyor, dropped by chopsticks (draw the chopsticks in `bird()`).
   - Cats: loaf cats (Box), long cats (Plank), a curled cat (Ball); a crane-claw arcade machine.
   - Toy blocks on a baby's high chair; a sock monkey as the dropper.
   Rewrite `art/style.toml` (the style sentence, keywords, avoid list, palette, and the `[music]`/`[sfx]` words), then the prompts in `tools/regen.sh` (one per snack variant, the bird's flap cycle, the face sheet, hills, meadow, blanket, plate, stick, sun, clouds, leaf, icon, the two music loops, fifteen effects). Run `tools/regen.sh snacks` first and look at `art/preview/`: once the first snack looks right it becomes the style reference (`art/ref/toast.png`) for everything after it. Keep each snack filling its collider's box (the script's header says how; change `Kind::size`/`wedge_points`/`radius` in `sim.rs` if your art's shapes differ). Then `tools/regen.sh all` (about $1) regenerates the rest, packs the atlas, quantizes it and runs `mb art check`; fix single assets with `mb regen art/sprites/<name>.png --new-seed` and `tools/regen.sh pack`. Finally change the names and words in `look.rs` (`snack()` maps each `Kind` to its three pictures and tints), the sky and field colors there, and `draw::face_frame` if your expression sheet is in another order.
2. **A twist on the verb or a rule** (`sim.rs`: `drop_piece`, `bird`, `land`, `step`):
   - Hold to lower the bird (drop from closer for precision), release to drop.
   - A second tap while it falls flips the snack 90° (`world.set_angle`), so planks can stand up.
   - Score height instead of count (`best_height` is already tracked), or race a 60 s timer.
   - The plate rides a rocking boat: `plate_angle` follows a slow sine all round, not just in twists.
   - Goal lines: reach each height marker to bank the snacks below it (set them solid at once).
3. **One new mechanic** (a new `Kind` and/or `TwistKind`):
   - Honey: a sticky snack that glues to what it lands on (`world.join(Joint::fixed(..))` in `land`).
   - A balloon snack that floats (`Body::gravity_scale(-0.2)`) and lifts what's on it.
   - Ice: `friction(0.05)`, it slides unless dropped dead centre.
   - A thief: every 30 s a crow swoops and knocks the top snack (`apply_impulse` on it).
   - Rain: friction drops for a few seconds (`world.set_friction` on every shape).
   - An anvil: a heavy snack that squashes the faces below it (`.density(4.0)`).

For each, add a `Cue` for its moment and give it a sound, a haptic and a popup in `lib.rs::juice` / `sound.rs`.

## Difficulty (bot)

`cargo test -p kit_stacker --release -- --ignored --nocapture difficulty` (200 seeded rounds per preset). The bot plays like a person: it watches the landing ghost and taps when it's over the top snack, but acts on what it saw a reaction delay ago; it misjudges the ghost by a fresh random bias each snack (more for a kind it's never seen: overshoot), gets more careful (a narrower window) for three drops after losing a heart, and the better presets anticipate their own delay (lead), steer a leaning tower back toward the middle, and wait out twists and the previous snack settling.

| Preset | Delay | Misjudge | Lead | Median round | p10–p90 | Snacks (median) | Tower |
|---|---|---|---|---|---|---|---|
| first-timer | 0.20–0.35 s | ±15 px | 15 % | 38 s | 30–47 s | 13 | 4.7 m |
| decent | 0.17–0.28 s | ±9 px | 55 % | 53 s | 48–64 s | 17 | 6.5 m |
| good | 0.15–0.22 s | ±5 px | 90 % | 67 s | 60–76 s | 23 | 7.9 m |

How they lose: hearts go mostly to misses (snacks that never settle: a slightly off drop onto a small or tilted top) and, for good players, increasingly to tumbles (47 % of their lost hearts) once round snacks and macarons are in the mix; 12–15 % of lost hearts fall during a twist. A first-timer loses the first heart at ~21 s, a good player at ~48 s. The first seconds can't be failed (`the_first_seconds_cannot_fail`), and `a_first_timer_lasts_long_enough` checks the first-timer median on a few seeds in every `cargo test`. The bot is a weaker stacker than a thoughtful person (it never plans for the shape on top), so real good players should last longer than its 67 s; `HEART_BACK` rewards them.

Physics budget: at most 29 awake bodies seen in 600 rounds (fallen snacks leave the world after 3 s; settled snacks sleep, and the deep ones are held to the plate).

## Determinism

All randomness comes from `Rng` seeded by `sys::rand_seed()` (or `sys::daily_seed()`), the sim only runs from `update`'s `dt`, the physics world is stepped and changed only in `update`, bodies live in a `Vec`, and the juice `Particles` are seeded from the game's `Rng`. Sound pitch jitter uses its own fixed-seed `Rng` and never feeds back.

- `the_same_taps_replay_the_same_round` (cargo test): the same seed and bot over 2400 frames with uneven `dt` give identical physics state hashes, scores and cues.
- `mb.verify()` in the preview: a 5392-frame held session (blind taps every 1.1–1.7 s through three stretches of play, a game over with the collapse and the results card, a retry from the card and more play) replayed with `match: true`, `firstMismatch: null`, hash `c4f6d2818ca88ffd` (with the generated art; the seed differs per session, so the hash does too). All images are startup assets, so they're decoded before the first frame in play and in replay alike.

## Not verified on a phone

- The feel of the timing with a thumb and real touch latency (`SWEEP_*`, `CARRY`, `RESPAWN`).
- Haptics (landing ticks, heavy thuds, the fail buzz) and the audio mix by ear. Measured on a decent bot's 58 s round (mix_check.py): the music (`MUSIC_VOL` 0.85) sits a median +4.1 dB over all effects together (effects louder 27 % of the time, was 50 %), thuds 10–12 dB under it, the limiter never pulls more than 0.8 dB (it hit 6 dB in the collapse before). Nothing generated has been heard: the music, the tension layer's fit against it (aligned by onsets, measured in key), and the marimba and sparkle pitches (measured onto G major notes) all need a listen on a phone.
- The art at arm's length: faces on the thin baguette are the smallest; the paper-grain texture is multiplied over the sky.
- CPU on an iPhone 13 class device: in the desktop preview (a heavily loaded Mac) `mb_update` averaged 0.10 ms and `mb_render` 0.16 ms in play (~1000 mb2d quads); physics stays under ~30 awake bodies.
- Safe areas on notched phones (the HUD uses the UI kit's `Layout`).
