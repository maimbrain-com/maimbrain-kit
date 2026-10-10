# Swerve

A 3D endless lane racer for one thumb, in portrait (the `racer` kit). A chunky teal coupe tears down a three-lane sunset highway; tap the left or right half of the screen (or swipe) to change lanes. Dodge traffic, trucks that move over, roadworks and oil; grab coins and boost pads. mb3d 2: toon shading with outlines, fog, instancing, 3D text, depth of field. Hand-written car feel, no physics.

- **The verb**: tap left / tap right (a swipe does the same). One lane per tap, with a springy slide, body roll and a tyre swish.
- **The 1-second read**: a toon car from a low chase angle, weaving through traffic on a road that melts into a pink-orange fog under a low sun. "SWERVE / the sunset road never ends", and BEST once there is one.
- **The first 10 seconds**: the tap that enters play is the first lane change (left or right half). A carpet of coins fills all three lanes for the first two seconds, so whichever way the first tap went it pays (coin plucks climbing a scale). Chevrons beside the car, a ghost thumb tapping each side and "TAP LEFT · RIGHT" teach the verb until the player has steered three times; they come back after 3 s without a press, early in a player's life or when something is coming down their lane. Nothing reaches the car before 3.5 s; the first car arrives at ~4 s, alone, with 1.5 s of clear road around it.
- **Tension and failure**: traffic is slower than you, so you catch up with it lane by lane; the rows get closer together (1.5 s → 0.66 s of clear road) and faster (a speed-up every 12 s, 16 → 44 m/s, which the speedo shows as 115 → 317 km/h); after 90 s the nasty rows can come back to back. Danger is always visible: everything comes out of the fog far ahead, trucks that will move over blink for 0.85 s before they slide (with an orange arrow over them), roadworks start with a flashing barrier. Late dodges (still in the way 0.55 s before impact) score a near miss ("CLOSE! +50" in 3D text, a whoosh and a ding, a little shake; chains within 3 s multiply, ×2 … ×5). A hit ends the round with a 1.8 s crash: hit-stop, slow motion, the car flips onto its roof in a shower of sparks and glass, a shockwave and flash, traffic brakes, the camera swings round the wreck. Then the results card over the smoking wreck.
- **The loop**: first-timer rounds ~40 s, good players ~2 min. Every 12 s it speeds up ("SPEED UP! 131 KM/H"); something new arrives every ~15 s: two-car walls (9 s), boost pads (11 s), trucks (17 s), trucks that change lanes (31 s), roadworks (46 s), oil (62 s), each announced with a ribbon. Until 90 s, never two nasty rows in a row; every row leaves a lane you can reach in time.
- **The cards**: title: attract mode, the car drives itself (plans ahead, dodges one lane late for near misses), camera low behind its shoulder, DAILY under the card; game over: the wreck on its roof, smoking, the camera swinging slowly round it, WRECKED!, the score rolling up, BEST or NEW BEST!, SCORES and DAILY buttons; a replay reads from the score counter, the speed pill and the crash.
- **Score**: 1 point per meter + 10 per coin + 50 × chain per near miss + 100 per smash (boosting through traffic). Best saved (`best`, and `daily:<seed>` for the daily road); board 0, board 1 for the daily.

## Look and sound

- **Look**: low-poly toon 3D at golden hour. Chunky faceted shapes, two-to-three band cel shading, dark plum ink outlines, a low sun straight ahead so rims glow, and a warm pink-orange haze that swallows the distance. The hero is the only saturated teal on the road (traffic is muted pastels). All of it is drawn in code (`src/scene.rs` meshes, `src/look.rs` colors). `art/style.toml` describes the same look in words for anything generated: the icon now, and any sky, billboard or decal textures a reskin adds.
- **Boost**: everything about a boost points the way you drive. The pad's three chevrons point down the road, brightest at the front (`pad()` in `src/scene.rs`; they used to point back at the camera). The flames are one tapering jet trailing back from the pipes (they ride along with the car at `FLAME_DRIFT` of the road's speed, instead of being flung at the camera as separate puffs), the wind streaks and speed lines stream past toward the camera, and the car surges ahead of the camera before it catches up.
- **Sound**: a sunset outrun synth set. The music is one generated Lyria loop (8 bars at ~120 BPM, in B major around F#), split into a low layer (bass, kick, pad body) and a bright layer (hats, snare snap, arps, lead). The bright layer sits at 35% at the start and opens up with each speed-up, boosts and near-miss chains. It works like a filter sweep that follows your speed. The effects are ElevenLabs: tyre swish, an engine loop that is pitched with speed, a wind roar that fades in with speed, a Doppler whoosh for every car you pass close by (panned to its side), soft marimba-like coin plucks that climb a five-note ladder in the music's key and start again, the near-miss whoosh and ting (a chain climbs in the key's notes), the boost ignition roar, the smash crunch, a blinker tick-tock while a truck signals, the oil splat and skid, and a big crash with glass. The alert, game-over and new-best stings and the tyre squeal are pitched into the key too. Every file has a `.gen.json` sidecar with its exact prompt.
- **The mix** (`FX`, `MUSIC_VOL`, `ENGINE_VOL`, `WIND_VOL` at the top of `src/sound.rs`): the music is the reference and every effect is set against it, measured rather than heard. The `mix_log` test drives a bot round through the real sound module and writes every play, loop level and stop; the skill's `scripts/mix_check.py` renders that with the real assets and measures it (`MB_MIX_LOG=/tmp/racer.log cargo test -p kit_racer mix_log`, then `python3 <skill>/scripts/mix_check.py assets /tmp/racer.log`). In a decent player's 93 s round (seed 3), K-weighted:

  | | Before | After |
  |---|---|---|
  | Music over all one-shots (400 ms windows), median / p10 | +1.9 / −9.0 dB | +8.4 / −0.6 dB |
  | Music over engine + wind, median / p10 | +6.4 / +1.0 dB | +9.7 / +5.1 dB |
  | Coins (68 a minute): level vs music (3 s, p90) / loudest moment / share of the effects / at once | +6.5 / +12.5 dB / 53% / 8 | −6.7 / −1.1 dB / 12% / 3 |
  | Pass-bys (27 a minute) / lane swishes (21 a minute) vs music | −3.4 / −7.8 dB | −8.8 / −10.7 dB |
  | Near miss / boost / crash, loudest moment vs music | +11.0 / +4.9 / +12.8 dB | +6.3 / +1.8 / +7.9 dB |

  The coin was the problem: a bright bell ping (4.5 kHz brightness, instant attack) played at the music's level, often 8 at once, climbing to 1.7× its pitch. It is now a mellow pluck (1.6 kHz), ~7 dB under the music over 3 s (each pluck peaks over half as loud as the music on a phone speaker; at 10 dB under, the first level, it peaked at under half and was hard to hear), at most 3 at once, ±1.5 dB per play, between 0.71× and 1.41× its pitch. The music went up 3 dB (`MUSIC_VOL` 1.0) and the effects came down around it, so the round is about as loud as before with the music on top. Frequent effects vary ±1–1.5 dB and a few percent in pitch (not the tonal coin), and every effect has a cap on voices at once (`cap`; the oldest still ringing is stopped), which keeps the worst case under the platform's 32 voices. The wind roar tops out lower on boost (`WIND_MAX`). The crash is the one sound well over the music, and the music cuts out under it. Still flagged as hard to hear on a phone: the boost roar and the game-over sting (mostly under 300 Hz, which a phone speaker barely plays), the speed-up zip and the lane swish; they need brighter takes, not just more level.

- **Regenerating** (`tools/regen.sh`, about $0.12 for the lot): the script lists every `mb music`, `mb sfx` and `mb art icon` command that made the shipped assets. `sh tools/regen.sh` remakes them all, `sh tools/regen.sh coin crash` remakes just those, and `mb regen assets/coin.ogg` replays one recipe. The `[music]` and `[sfx]` words in `art/style.toml` are added to every prompt. After a new music track, find its key (Lyria ignores "in A minor") and set `MUSIC_ROOT` in `src/sound.rs` so the coins land on its key note. If you change coin.ogg, re-measure its pitch too (it's a G5 now, so `MUSIC_ROOT` is −1), and if a regenerated effect's length changes, its `secs` in `FX`. Then rerun the mix check. ElevenLabs has no seeds, so every run is a new take. Read the numbers each command prints and run the skill's `check_audio.py --loops engine,wind assets/*.ogg`. `tools/gen_audio.py` and `tools/gen_icon.py` are the old code-made set (free and offline, but without `wind`, `pass` and `blinker`, which then just stay silent).

## Sense of speed

Players said Swerve didn't feel fast. Research into how racing games sell speed, and what Swerve now does about it:

- **Edge rate beats everything.** Perception studies separate *global optic flow* (speed ÷ eye height) from *edge rate* (edges passing per second). Edge rate explains more of how fast people think they're going ([Larish & Flach](https://corescholar.libraries.wright.edu/psychology/367), [NASA N92-21475](https://ntrs.nasa.gov/api/citations/19920012232/downloads/19920012232.pdf)). Road engineers use this to slow drivers down with transverse bars spaced closer and closer ([Denton](https://vtechworks.lib.vt.edu/handle/10919/27759)). OutRun throws palms and signs at the camera, and F-Zero alternates track tones. Sparse, distant scenery reads as slow whatever the number says ([critpoints](https://critpoints.net/2016/11/24/feeling-of-speed/)). *Swerve*: red and white rumble strips a block per meter down both edges, faint bands across the asphalt every 4 m, rail posts every 2 m with alternating white and amber reflectors, scrub and stones scattered close to the road, and light-gantry "tunnels" (one gantry every 14 m for 210 m of every 640 m). The gantries strobe overhead and sweep shadow bands toward the camera. At the start that is ~16 kerb edges per second per side; at top speed, 44.
- **Contrast should fall off with distance, not everywhere.** Fog that hides only the distance makes drivers *over*estimate speed. Uniform haze makes them underestimate it ([Pretto et al.](https://pmc.ncbi.nlm.nih.gov/articles/PMC3482687)). *Swerve*: the fog stays, but depth of field no longer blurs the near field in play.
- **Field of view that widens with speed, and kicks on boost.** MotoGP widens FOV gradually toward its top speed ([Hargreaves](https://www.shawnhargreaves.com/blog/speed-is-a-matter-of-perspective.html)). SuperTuxKart adds FOV above 70% speed, more on nitro ([STK](https://forum.supertuxkart.net/thread-200.html)). A study of third-person racers found a wider FOV raised perceived speed ([Holm et al. 2017](https://eudl.eu/doi/10.1007/978-3-319-55834-9_14)). Portrait makes this matter more: a 0.92 rad vertical FOV is only ~31° across. *Swerve*: 0.98 rad at the start, 1.2 at top cruise, +0.2 on boost, +0.1 on a kick.
- **Camera placement: closer, lower, and left behind when the car surges.** A camera that comes in as the FOV widens keeps the car the same size while the world stretches (the dolly-zoom used in racing cameras). Chase cameras lag behind acceleration ([Godot recipe](https://kidscancode.org/godot_recipes/3.x/3d/kinematic_car/car_camera/index.html)), and STK pulls back on boost. *Swerve*: 7.4 m behind and 3.5 m up at the start, coming in to 6.0 m and 2.85 m at top cruise. On boost the car surges up to 1.8 m away and the camera takes ~0.35 s to catch up.
- **Vibration, not jitter.** Shake works best as smooth noise scaled by trauma², not per-frame random offsets ([Eiserloh, GDC](https://www.gamedeveloper.com/programming/video-sprucing-up-cameras-with-math)). *Swerve*: a ~9 Hz road buzz from summed out-of-step sines that grows with feel², capped small. There is no rotational shake at constant speed. The trauma shake on hits stays as it was.
- **Speed lines and streaks** are cheaper than radial blur on mobile and read better ([Unity forum](https://discussions.unity.com/t/speed-sensation-effects/432597)). Strong motion blur actually *lowered* perceived speed in the Holm study, so Swerve has none. *Swerve*: screen-space streaks shoot out from the vanishing point in an outer ring only, never over the car or the road just ahead of it. They start at feel 0.75 and are full at 1.5. Wind streaks in 3D stream past with the road. Chromatic fringing, vignette and bloom ramp up with speed and boost.
- **Sound.** Burnout's engine pitch keeps climbing toward a ceiling it never reaches, with no gear drops. Its pass-bys are fired early so the middle of the whoosh lands as you pass, with a sharper Doppler curve at higher speed. Wind and tyre layers rise with speed ([Ben Minto on Burnout](https://designingsound.org/2017/08/31/burnout-a-sound-design-retrospective-with-ben-minto/)). *Swerve*: the engine pitch climbs with every speed-up and revs on boost and kicks. A wind roar fades in from near silence. Every car or truck passing within 4 m gets a panned whoosh 0.45 s early (`PASS_LEAD`), pitched up with the closing speed.
- **Numbers.** F-Zero's speedo reads four digits. *Swerve*: the speedo shows arcade km/h, 2× the real speed (`SPEEDO` in `src/lib.rs`), so 115 km/h at the start, 317 at top cruise and 460 on boost.
- **Not done, and why**: radial or motion blur (expensive on an A15 and measured as *slower*). Lowering the camera further (the hazards ahead would hide behind traffic). Faster traffic contrast: `TRAFFIC_SPEED` is a difficulty knob (lower means you blow past cars faster but they arrive sooner), so it was left at 7 m/s and the bot results still hold.

All of it hangs off one number, `sim.feel.k` (`src/feel.rs`): ~0.35 at the start speed, 1 at top cruise, up to ~1.7 boosting, plus decaying kicks on a speed-up (0.6), a boost pad (0.9) and a near miss (0.25). The knobs are at the top of `src/feel.rs`: `FEEL_START`, `FEEL_BOOST`, the `KICK_*` values, `FOV_*`, `CAM_*`, `SURGE_*`, `RUMBLE*` and `LINES_FROM`. `feel` is stepped in the sim with no randomness, so it replays exactly. The rules never read it.

Budget: draws are unchanged (62–74, plus 2 inside gantry zones). Triangles went from 20–24 k to 25.5–30 k (kerbs, posts and reflectors are part of the one road mesh; gantries are one instanced node). `mb_update` max went from 0.09 to 0.2 ms (desktop).

## Tuning knobs

All in `src/sim.rs`, top of the file (`// ---- Tuning knobs`):

| Knob | What it does |
|---|---|
| `START_SPEED` (16 m/s) | Speed at the start. Raise it and the first seconds get hard. |
| `SPEED_STEP`, `SPEED_EVERY`, `MAX_SPEED` (2.2 m/s every 12 s, up to 44) | The speed-ups: how much, how often, how far. |
| `TRAFFIC_SPEED` (7 m/s) | Traffic's own speed. Closer to yours = you pass cars more slowly, so they block lanes for longer (harder to weave). |
| `GAP_START`, `GAP_MIN`, `GAP_SHRINK` (1.5 s → 0.66 s, −0.011 s per second) | Clear road between rows, in seconds. The main difficulty dial. |
| `LANE_STIFFNESS`, `LANE_DAMPING` (170, 0.62) | The lane change spring: higher stiffness snaps faster; damping < 1 overshoots (springy), 1 slides. |
| `BOOST_TIME`, `BOOST_MULT` (2.6 s, ×1.45) | Boost pads: how long and how fast. Boosting smashes through everything. |
| `NEAR_TTC` (0.55 s) | How late a dodge must be to count as a near miss. Raise it for more near misses. |
| `COIN_POINTS`, `NEAR_POINTS`, `SMASH_POINTS` | Scoring. |
| `UNLOCK_*`, `RELENTLESS` (90 s) | When each new thing first shows up; when nasty rows may come back to back. |
| `SAFE_START` (3.5 s) | Nothing can reach you before this. |

The look (names, words, colors, the UI theme) is in `src/look.rs`; the scene (meshes, sky, fog, effects) in `src/scene.rs`; how fast it *feels* (FOV, camera, vibration, speed lines) in `src/feel.rs` (see "Sense of speed").

## Make it yours

Three changes that turn this kit into your own game:

1. **A new world, vehicle and name** (`art/style.toml`, `tools/regen.sh`, `src/look.rs`, `src/scene.rs`, `manifest.toml`):
   - Sound and icon first, since it's the cheapest big change. Rewrite `style` and the `[music]`, `[sfx]` and `[icon]` words in `art/style.toml` for the new world (a neon night city: "dark synth, sidechained pads, rain hiss"), adjust the prompts in `tools/regen.sh` (each one names what the effect is for), then run `sh tools/regen.sh` (needs `mb keys set gemini` and `mb keys set elevenlabs`; ~$0.12). Set `MUSIC_ROOT` in `src/sound.rs` to the new track's key. See "Look and sound".
   - A neon night city: dark sky colors in `look::SKY`, fog to deep blue, lamps and windows glowing (unlit materials, bloom up in `Scene::build`'s `Post`), wet road (lower `roughness` on the road material).
   - A snowy mountain pass: white ground, pines instead of palms (`palm()` → a cone tree like games/mistwood's), snow particles instead of wind streaks, a sled or snowmobile instead of the car.
   - Space: hover-cars in a canyon lane, asteroids as cones, no road mesh but a glowing track (`road_mesh()`).
   - Under the sea: a submarine, fish schools as traffic (`sedan()` → a fish shape), bubbles as exhaust.
   - Change `TITLE`, `TAGLINE`, `HEADING`, the palette consts, the hero mesh in `hero_body()`, the icon and the music.
2. **A twist on the verb or a rule** (`src/sim.rs`: `press`, `steer`, `collide`, `row`):
   - Jump instead of dodge for some obstacles: a tap on the car hops it over oil and cones (add `lift` to `Pose` and a jump timer like Mistwood's).
   - Five lanes and a wider road (`LANES`, `LANE_W`, the road mesh), or lanes that narrow to two at roadworks.
   - Reverse: you're the slow one, traffic comes at you from behind (negative relative speed; show it in a mirror pip).
   - Fuel instead of instant death: hits cost fuel, coins are fuel cans, the round ends when it runs dry.
   - Drift scoring: holding a lane change mid-way (hold the touch) scores a drift multiplier.
3. **One new mechanic, hazard or power-up** (`Kind`, `plan_row`, `Scene::obstacles`, `Cue`):
   - A magnet power-up that pulls coins from the next lane.
   - Police cars that chase from behind and ram you if you stay in one lane too long.
   - Ramps: a pad that launches the car over the next row (sets a jump timer; collisions off while airborne).
   - Rolling barrels that cross lanes diagonally (an obstacle with a lateral speed; `covers()` already handles in-between lanes).
   - A shield pickup that eats one hit (the crash becomes a smash, then the shield pops).
   - Night sections where only headlights show the road (fog density up, a point light ahead of the car).

## Difficulty (bot)

`cargo test -p kit_racer --release -- --ignored --nocapture difficulty` plays 200 seeded rounds per preset (and prints one traced run). The bot (`src/tests.rs`) drives like a person: it only plans for threats within its look-ahead (first-timer 1.25 s, decent 1.6 s, good 2.1 s), reacts after a delay (0.24–0.42 / 0.2–0.32 / 0.15–0.24 s), taps one lane at a time with a gap between taps, sometimes taps the wrong way (7% / 3.5% / 1.2%) or overshoots a one-lane move, won't cross a lane that's about to be occupied, and only the good preset reads blinkers before the truck moves.

| Preset | Median round | p10–p90 | Score (median) | Near misses / min | What ends it (of 200) |
|---|---|---|---|---|---|
| first-timer | 38.8 s | 12.7–97.4 s | 1,176 | 2.9 | car 149, truck 43, roadworks 8 |
| decent | 59.9 s | 15.5–168.8 s | 2,031 | 3.2 | car 133, truck 52, roadworks 15 |
| good | 109.8 s | 35.8–334.1 s | 5,007 | 4.9 | car 122, truck 51, roadworks 21 (6 reached the 10 min cap) |

Targets met: first-timer 20–45 s, good 1–3 min; nothing can touch you for 3.5 s (`the_first_seconds_cannot_fail`, 40 seeds, idle and frantic). Oil never kills directly: it slides you a lane, and the crash that follows counts as the car or truck you slid into. 11 of 200 first-timer rounds end before 10 s (wrong-way taps and overshoots into early traffic). The good preset's long tail comes from the speed cap (44 m/s from ~2:36); `RELENTLESS` and a higher `MAX_SPEED` are the levers if that matters. The fast test `first_timer_median_in_range` checks 40 seeds in under a second in debug. The attract mode (`Sim::autopilot`) is tested too: at most 3 of 10 one-minute runs crash, with ≥ 10 near misses.

The generator lays rows out in time, not meters: traffic you only gain on at the difference in speed occupies a lane for longer, so each row starts `gap` seconds after you're clear of the last one, every lane that was free in the last row can reach a free lane in this one in time (`REACT` + `LANE_T` per lane), and traffic is placed by where you'll meet it, so a row is exactly the pattern designed when you get there. `every_row_leaves_a_lane_open` checks 30 seeds × 150 s of a good driver.

## Determinism

`await mb.verify()` in the preview (a held session driven by script):
- Final build, a scripted thumb (taps and a swipe, both halves): play, a crash at ~7.5 s, the results card, a retry from a whole-screen tap, more play: **match over 2,038 frames** (hash `071295e417e7b2c7`, with the speed feel and the generated sound).
- With `DEBUG_AUTOPILOT = 60` (since reset to 0): a frantic first round, crash, retry, a 60 s second round with boosts, smashes, near-miss 3D text, speed-ups, the truck/roadworks/oil banners, a crash and NEW BEST: **match over 4,916 frames** (hash `0601051b50c2d6f7`).
- After the boost and mix fixes: `DEBUG_AUTOPILOT = 60` (a 75 s round with boosts, crash, results card, retry, 10 s more play): **match over 5,392 frames**; with it back at 0, a hands-off crash, retry and 12 taps: **match over 1,502 frames**.
- A live session paused and resumed mid-round (the platform's pause: `mb_suspend`/`mb_resume`, which starts a 3-2-1 countdown before the road moves again): match over 182 frames.

All randomness is the sim's `Rng` (seeded from `sys::rand_seed()` or `sys::daily_seed()`), the camera shake has its own seeded `Rng`, roadside chunks are seeded by their slab of road, particles and juice by the game's `Rng`; sound jitter has its own fixed seed and never feeds back. `same_seed_same_inputs_same_round` replays 3,600 uneven frames bit for bit.

Budgets (`mb.stats()`, desktop Chrome, M-series Mac, 50 s of autopilot): 62–74 draws (+2 in gantry zones), 25.5–30 k triangles, 27–31 outlines, 18–21 shadow casters, 36–180 particles (a few hundred in a crash); `mb_update` 0.02–0.2 ms and `mb_render` 0.01–0.05 ms average. `game.wasm` 202 KB, `.mbx` 657 KB (startup set 384 KB), kit directory 834 KB (assets 0.5 MB, the icon 92 KB).

## Not verified on a phone

- The feel of the lane spring and the tap/swipe split with a real thumb and touch latency (`LANE_STIFFNESS`, `LANE_DAMPING`, `SWIPE` in `src/lib.rs`).
- Frame time on an iPhone 13 class device (A15) with outlines on ~25 draws, depth of field, and a few hundred particles in a crash.
- Haptics (a tick per lane change and near miss, heavy on smash and oil, fail on the crash).
- The generated sound, by ear: the music loop's seam and groove, whether the coin plucks really sit in the track's key (both pitches were measured, not heard), whether the coins are easy to hear (measured through a phone-speaker model, not heard), the mix of engine, wind roar and music as the speed climbs (measured: the music stays 5–10 dB over them), the pass-by whooshes (how often, how loud, the panning), and whether the lane swish (it swells to its peak at ~0.25 s, with the slide) feels instant enough with the haptic tick.
- The speed feel at arm's length: whether the wider FOV still lets you read hazards in time (the bot doesn't see, so it can't tell), whether the speed lines and road buzz are exciting or tiring, and the gantry strobe (2–5 Hz).
- Whether the fog density hides the road's end on a phone's smaller, brighter screen.
