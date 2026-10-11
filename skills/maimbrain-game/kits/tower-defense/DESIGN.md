# Cake Keep

The tower-defense starter kit. Angry jellies march down a winding road toward a cake with a face. Build towers on the pads beside the road, upgrade them, pop jellies for coins. Every jelly that reaches the cake takes a bite (a heart). Five bites and the cake is devoured.

- **The verb**: tap. Tap a pad, then tap a tower in the picker to build it. Tap a tower, then tap the green arrow to upgrade it. The optional third tap is the next-wave button (top right), which calls the next wave early for bonus coins.
- **The 1-second read**: a clay cake on a plate at the bottom of a cream clay road through a candy garden. Squishy jellies with angry faces waddle toward it, and gumball, ice-cream and cherry-bomb towers shoot them. The cake's face reacts. Tagline: "they want the cake".
- **The first 10 seconds**: the tap that enters play is also the first move: if it lands on a pad, the picker opens. Until the first tower is built, a bouncing arrow and "BUILD HERE" sit on the pad that covers the most road (`HINT_PAD`). Once the picker is open, a ring pulses on POP with "PICK A TOWER". Start coins (60) buy exactly one POP or CHILL. Wave 1 waits for that first tower and starts 1.5 s after it. Wave 1 is four extra-soft gummies that one POP on *any* pad pops (tested), so the first thing the player sees their tower do is win: pops, "+5" coins, the score ticking up.
- **Tension and failure**: jellies arrive in waves every 10 s, with a "next wave" preview (kinds and counts) and a countdown ring. A new kind arrives every couple of waves: zippers (fast) at wave 3, helmets (armored: POP barely scratches them, BOOM cracks them) at 4, splitters (pop into three minis) at 7, a zipper rush at 8, and the king (a boss that bites three hearts) at 10. After wave 10 the waves are a random budget mix that grows each wave, with a king every 5th wave and a rush in between. Danger shows before it costs anything: the cake goes from happy to nervous to an open-mouthed panic with a sweat drop, the music crosses from the march to its urgent twin (also for as long as a king is on the road) and the last heart pulses a red vignette. Each bite is a "CHOMP!" with a hit-stop, a shake, a wince, crumbs on the plate and a slightly smaller cake. Losing the last heart is the punchline (1.9 s, still Playing): every jelly rushes the cake, they pile on and chomp, and the cake shrinks, shakes and winces. The game-over card shows the aftermath: an empty plate, a cherry with X eyes, and four smug, happy jellies.
- **The loop**: build, earn, upgrade, watch the next wave preview, decide whether to call it early. Escalation every 10 s (each wave adds ~25% jelly hp, a little speed and more jellies), something new every ~10–30 s (see above). One tap retries.
- **The cards**: title: the game plays itself (an autopilot builds up to five towers in the lower half, so the fight happens below the title). It starts mid-game, two waves on the road, so the card shows action from its first frame, and every so often the cake falls. Game over: DEVOURED!, jellies popped rolling up, BEST or NEW BEST!, "REACHED WAVE n", and the aftermath behind. A replay reads from the hearts, the score, the cake's face and the banners.
- **Score**: jellies popped (board 0), saved as best (`store` key `best`). The wave reached is shown on the results card.

### Why rounds run longer than the feed's 20–45 s

A tower defense round is a build-up: the fun is watching a defense you made hold, then crack. Below about a minute there's no time to build more than two towers or meet more than one new jelly. So the kit aims at a first-timer round of ~60–80 s (they meet zippers and helmets and usually fall to the first armored waves, which teaches BOOM for round two) and 2–3 minutes for a good player. Sessions stay short: waves come every 10 s whatever you do, the HP curve is steep (1.2× per wave), and good players can call waves early to go faster. The first seconds can't be lost: wave 1 waits for the first tower.

## Look and sound

**Claymation.** Everything looks hand-sculpted from plasticine and shot from above under soft studio light: a candy-garden lawn with thumbprints, a cream clay road with caramel edges, shortbread build pads, squishy translucent jelly blobs with angry faces, a gumball machine, an ice-cream cone and a cherry-bomb mortar that each grow fancier at levels 2 and 3, and a strawberry cake whose face goes happy → nervous → panic, winces at bites and ends as an empty plate with an X-eyed cherry. The style guide is `art/style.toml` (style sentence, keywords, avoid list, palette, the green key colour, the gummy jelly as the reference every other image matches).

- **Generated** (`mb art`, 13 sheets in `assets/` with their `.gen.json` recipes): the map (road baked in, drawn over `art/ref/map_sketch.png` so it lies exactly on `PATH`), the pad, the cherry bomb, six jelly sheets (walk A, walk B, pop; the gummy also has a hit squash and a smug full face), three tower sheets (levels 1–3), the cake's five moods and the icon. Each sheet is one row of equal cells; `src/art.rs` lists them (cell size, frames, which frame is which).
- **Code-drawn**, restyled to match: soft shadows under everything, the gumball shots (clay balls in the dome's colours), the road's drifting dimples, reach rings, particles, the crumbs, hp bars, the picker, the HUD and cards (theme in `src/look.rs`: plum-chocolate clay panels, strawberry accent, extra-round corners).
- **Motion personality**: jellies waddle (two walk frames, a squash-and-stretch wobble, a hop), flash white when hit and get a frosty blue sheen when slowed; popped jellies play their burst frame as they swell and fade; towers squash on each shot and lean toward their target; the cake breathes, shakes and shrinks.
- **Music** (`mb music`, Lyria): a bouncy circus marching band (oompah tuba, snare, clarinet and cornet, glockenspiel) and an urgent twin (snare rolls, brass stabs, cymbals), both 8 bars at 120 BPM, measured in B♭ major (Lyria ignores the key you ask for). They share a tempo but not a downbeat, so `src/sound.rs` crosses between them (0.5 s in, 1.2 s out) instead of stacking them.
- **Effects** (`mb sfx`, ElevenLabs, cartoon clay foley): a squelchy jelly pop that climbs gently on quick chains, a gumball thwip, a mortar thoomp, a cherry-bomb boom, a frost shimmer, a coin clink, a clay thunk for building, a glockenspiel upgrade, a bugle call for waves and a brass-and-timpani one for kings, a chomp for bites, and a chomp-down-and-trombone wah-wah when the cake is devoured.

## Tuning knobs

All in `src/sim.rs`, the `Tuning knobs` block at the top (towers and jellies are tables right under it):

| Knob | What it does |
|---|---|
| `WAVE_GAP` (10 s) | Seconds between waves. The main pacing knob: lower and waves overlap sooner and rounds end faster. |
| `HP_GROWTH` (1.2) / `HP_BASE` (0.8) | Jelly hp × `HP_BASE × HP_GROWTH^(wave−1)`. HP_GROWTH is the difficulty curve: 1.15 is gentle and long, 1.25 is a wall where every skill level dies around the same wave. |
| `FIRST_WAVE_HP` (0.6) | Makes wave 1 a guaranteed win. Raise it and test `wave_one_is_a_guaranteed_win_from_any_pad`. |
| `START_COINS` (60) | Enough for one tower. Raise it and the opening gets easier. |
| `PAYDAY`, `PAYDAY_GROWTH` | Coins at each wave start (8 + 2 × wave). The economy's floor. |
| `EARLY_BONUS_PER_SEC` (2.5) | What calling a wave early pays per second skipped. |
| `HEARTS` (5) / `KING_BITE` (3) | Mistakes allowed. More hearts means longer, more forgiving rounds. |
| `TOWERS` | Per tower kind and level: cost, range, fire interval, damage, special (CHILL's slow factor, BOOM's splash radius). |
| `CREEPS` | Per jelly kind: hp, speed, armor (subtracted per hit, so it counters many small hits), bounty, radius, spawn spacing. |
| `plan_wave` | Which jellies come in which wave: waves 1–10 by hand, then a budget mix with kings every 5th wave and rushes in between. |
| `PATH`, `PADS`, `HINT_PAD` | The map: the road's waypoints, the build pads (52 units square) and the pad the first hint points at. |

The title card's autopilot (`ATTRACT_THINK`, `ATTRACT_STYLE`) and the picker's geometry (`SLOT_R`, `SLOT_GAP`, `SLOT_OFFSET`) are at the top of `src/lib.rs`. Words, the theme and the palette are in `src/look.rs`.

## Make it yours

Three changes turn this kit into its own game. Do all three.

1. **A new theme, characters and names** (`art/style.toml`, `tools/regen.sh`, `src/look.rs`). The rules don't care what anything looks like. To restyle: rewrite `art/style.toml` (the look, palette, `avoid`, the `[music]` and `[sfx]` words; set `references = []` until your new hero exists), rewrite the descriptions in `tools/regen.sh` for your characters, then run `tools/regen.sh jelly_gummy` (the hero), look at `art/preview/jelly_gummy.png`, point `references` at it, and run `tools/regen.sh` for the rest (about $1; `tools/regen.sh <name>` redoes one asset, `mb regen assets/<name>.png --new-seed` tries another take). Look at every `art/preview/*.png`, then in the game: fix `TOWER_LEVEL_FRAME` and `JELLY_CELL_PER_R` in `src/art.rs` and `TOWER_LEVEL_GROW` in `src/draw.rs` if sizes or frame order changed. A new map layout: change `PATH`/`PADS` in `src/sim.rs`, copy them into `tools/map_sketch.py`, and run `tools/regen.sh map`. Ideas:
   - Bees defending a honey jar from beetles: honey drips as hearts, a hive as the base.
   - A sandcastle at the bottom of a beach, crabs scuttling down; towers are buckets, shells and a seagull.
   - A sleepy dragon guarding its gold hoard from greedy knights. The gold shrinks with each bite.
   - Ghosts floating toward a lighthouse at night; lamps, bells and a fog horn as towers.
   - Space: asteroids drifting at a planet with a face, satellites as towers.
   Change `TITLE`/`TAGLINE`/the banners and the code-drawn colours in `src/look.rs` (pick them from your new art), and the sheet layout in `src/art.rs` if you add frames or kinds.
2. **A twist on the verb or a rule** (`src/sim.rs`, `src/lib.rs`). Ideas:
   - **Drag to aim**: one tower the player swipes toward jellies (a slingshot) on top of the auto-towers.
   - **Moving pads**: tap a tower and tap another pad to move it (a cooldown); the defense follows the danger.
   - **Combo merges**: building a tower next to the same kind merges them into a stronger one (`build`).
   - **Tap to pop**: tapping a jelly directly does a little damage, so the player is never idle.
   - **Paths that change**: every 5 waves the road gets a new bend (`PATH` per phase), and pads off the new road are refunded.
   - **One-lane chaos**: two roads that merge before the base, the second opening at wave 6.
3. **One new mechanic, enemy or power-up** (`CreepKind`/`CREEPS` + `plan_wave`, or a new `Cue` + host juice). Ideas:
   - A **healer** jelly that restores neighbours' hp every second (a priority target).
   - A **flyer** that skips the road and floats straight at the base (only POP can hit it).
   - A **digger** that hides underground for part of the road (untargetable), so coverage near the end matters.
   - A **sugar rush** power-up dropped by kings: tap it for 5 s of double fire rate.
   - A **sell** button in the upgrade bubble (70% refund) so layouts can change.
   - **Daily maps**: a few `PATH`/`PADS` sets picked by `sys::daily_seed()`, with a second board (the kit has no daily mode; stack shows the toggle).

## Difficulty (bot)

`cargo test -p kit_tower_defense --profile bot -- --ignored --nocapture difficulty` plays 200 seeded rounds per preset with a bot that builds the way a person does. Under it is `autopilot::Planner`, a sensible heuristic: POP first, a BOOM once armor is coming, a CHILL at three towers, the best uncovered pad by road coverage, then upgrades when they're better value per coin. The bot (`src/tests.rs`) adds the human part:
- it **glances** at its coins only every so often (first-timer 3–8 s, good 0.3–0.8 s), because it's watching the jellies;
- a **reaction delay** (150–350 ms) plus the time for the picker's **second tap** (0.25–1.4 s), and **missed taps** that cost another try;
- **mistakes**: a random pad (first-timer 50%, good 5%) or a random tower kind (first-timer 50%, good 3%);
- **late discovery** of upgrades (first-timer after 60 s, decent after 35 s, good at once), **distractions** (first-timer 35% of actions, 3–8 s);
- the first tower always follows the game's hint (the pulsing pad, POP pulsing in the picker);
- only the good preset calls waves early, when the board is calm and its money is spent.

| Preset | Median round | p10–p90 | Jellies popped (median) | Wave reached (median) | Last bite by |
|---|---|---|---|---|---|
| first-timer | 76 s | 65–101 s | 50 | 7 | helmets, zippers (the first armored waves) |
| decent | 139 s | 73–154 s | 170 | 14 | zippers, kings, helmets |
| good | 150 s | 137–193 s | 224 | 15 | zipper rushes, kings |

Decent and good end close together because the HP curve is a wall around waves 14–16; what skill buys before then is a cleaner defense (fewer bites) and, for good players, faster rounds and more coins from early calls. The fast test `first_timer_round_length_is_in_range` checks the first-timer median stays in 40–90 s on 24 seeds (well under a second in debug).

## Determinism

- All randomness comes from `Rng`: the sim's from `Rng::from_host()` → `sys::rand_seed()`, the particles' from the same generator, the sound's pitch jitter from its own fixed-seed `Rng` (output only). Time comes only from `dt`; no hash maps; `render` reads only (scenery positions come from a hash function, not an RNG).
- `mb.verify()` in the preview: `{ match: true, firstMismatch: null }` over a held session of **14178 frames** (3 s of title, a played round with builds, the devouring, the results card, a retry and 10 s of a second round), draw hash `32e18a47df6e27c6`. The art changes nothing here: sheets load at startup, and popped-jelly bursts (`Splat`) are host state advanced by `dt` only.
- `the_same_inputs_replay_the_same_round` runs the sim twice with the decent bot and uneven frame times and compares score, coins, hearts, wave and every jelly's hp and position bit for bit.
- Budgets measured in the preview on an M-series Mac over that session (up to ~1,100 mb2d quads; sprites replaced most code shapes): `mb_update` 0.01 ms, `mb_render` 0.12 ms average (0.3 ms worst). `game.wasm` 176 KB, `.mbx` 875 KB, startup set 537 KB (wasm, the 13 sheets, the opening sounds and the march). The kit directory is ~970 KB: map 191 KB, music 2 × 120 KB (32 kHz, `-q 2`), 17 effects ~170 KB, sprite sheets ~110 KB (96 colours), sidecars ~100 KB.

## Not verified on a phone

- How the picker feels under a thumb: the buttons are 54 units across with a 6-unit grace ring, offered above pads on the lower half and below pads near the top (`SLOT_*` in `src/lib.rs`). Pads near the screen edges may want the picker nudged further in.
- Whether the HUD fits on phones with a large top inset: the coin pill and the next-wave button sit under the safe-area top, and on a notched phone they might crowd the top row of pads (A and B).
- Haptics: Tap on build, Success on upgrade, Heavy on bites and kings, Fail when the cake falls, ticks on fast pop chains (`src/sound.rs`).
- Audio balance by ear. Measured on a decent bot's two-minute round (`MB_MIX_LOG=/tmp/td.log cargo test -p kit_tower_defense mix_log`, then the skill's `scripts/mix_check.py assets /tmp/td.log`): the march (`MUSIC_VOL` 1.0) now sits a median +3.5 dB over all effects together (it was 6 dB *under* them, effects louder 79 % of the time, now 21 %). The gumball thwips (rate-limited to one per 100 ms), hits, splats and booms are each held to 3 voices, varied ±1.5 dB, and sit 6–13 dB under the music; the mortar thoomp is pitched a semitone down onto B♭. ElevenLabs makes effects of at least 0.5 s, so the thwip, hit and UI sounds may have tails that want shortening in code (lower volume) or a regenerate. Neither loop has been heard on a phone speaker, nor the crossfade between them (they're out of phase by design; it should read as a switch, not a clash).
- The art at arm's length: the jellies, towers and cake read well in 720 × 1558 preview screenshots (2 px per unit, a 390 × 844 phone); the dark purple helmet jelly is the lowest-contrast thing on the cream road.
- CPU on an iPhone 13 class device (on the Mac the frame costs ~0.13 ms; the worst case is a late wave with ~60 jellies and every pad built).
