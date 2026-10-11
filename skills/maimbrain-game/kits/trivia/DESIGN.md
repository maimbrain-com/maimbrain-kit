# Bright Spark

A fast quiz for the feed, staged like a 1950s TV game show. Watt, a lightbulb quiz host with a polka-dot bow tie, asks a question; you tap one of four answers before the ring runs out. Right answers score more the faster they come, streaks multiply, every fifth question is golden, and the clock gets tighter with every question. Three misses and Watt's bulb blows.

- **The verb**: tap an answer (one of four big buttons in the lower half, under the thumb).
- **The 1-second read**: a big lightbulb with a face and a bow tie, eyeing a question card while a countdown ring drains; the card flips over to the answer on green and he grins. "BRIGHT SPARK, how bright are you?" It reads as a quiz show at a glance.
- **The first 10 seconds**: the tap that enters play deals the first card at once (a flip, a jingle). It is the warm-up: always an easy question, a generous clock, and a miss costs no heart. A finger taps the answers in turn and "TAP THE ANSWER" sits under the card until the first answer. A right answer pops the button green, sparkles, rolls the score, and Watt grins with stars bursting off him; a wrong one shakes the button red, buzzes, dims and flickers Watt, and lights the right answer green.
- **Tension and failure**: the countdown ring drains green → gold → red; the last three seconds tick (sound and haptic), the screen edges redden, the music's tension layer comes up, and Watt sweats with wide eyes looking at the clock. A miss or a timeout costs a heart (it bursts off the HUD). The last heart: the reveal holds on the right answer, the bulb flickers, then pops (glass shards, sparks, a flash, a thud) and smokes. The game-over card is Watt, cracked, X-eyed and smoking, under "LIGHTS OUT!".
- **The loop**: a first round is ~30–50 s (7 questions), a good player's 1–3 min. Every question is harder in one of two ways: the clock (its pace drops 12% a question until it bottoms out by question 6) and the tier (easy → medium from question 3, hard from question 6, earlier on a streak). New things arrive about every 15–25 s: the first golden question (×2, gold card, fanfare) at question 5, the ×2 streak flame at 3 in a row, ×3 at 6, ×4 at 10, and a heart back for every 8 in a row.
- **The cards**: title (attract mode): a question card flips in, Watt reads along while the ring drains, the card turns over to show the answer on green and Watt grins, the next card flips in (every 3.8 s, easy questions short enough to read at a glance, all text at its play size); the bouncing title, the tagline and the best above; no answer buttons and no instructions. Game over: the blown bulb smoking, LIGHTS OUT!, the score rolling up, BEST or NEW BEST! with confetti, "N RIGHT · BEST STREAK N", SCORES and DAILY. A replay reads from the HUD, the ring and the button colors alone.
- **Score**: points (tier × speed × streak multiplier × golden), shown big at the top; best saved per mode; board 0 endless, board 1 daily.
- **The round flow** is the SDK's `Rounds` (`maimbrain::rounds`): the tap on the title starts a round, and when the sim reports over (after its own ending: the reveal, the flicker, the pop, all still `Playing`, so `.ending(0.0)`) it submits the score, saves the best, shows the floating LIGHTS OUT! card and reports `Over`, in that order; the card ignores taps for its 0.35 s guard, then any tap retries. SCORES and DAILY are the kit's own buttons, fed events before `Rounds`. `tests::keeps_the_platform_rules` checks the flow on a fake host.

**Daily mode** (the DAILY button on the title and results cards): `Rng::new(sys::daily_seed())`, so everyone gets the same questions in the same order with the answers in the same places. In daily mode the tiers follow the question number only (in endless they also follow the streak, which would make players' sequences diverge), and each question's answer order is seeded from its number, not from what came before. Separate board and best (`daily:<seed>` key).

**The question bank** is `src/questions.txt`, one question per line, `tier | CATEGORY | question | right | wrong | wrong | wrong` (204 questions: 59 easy, 72 medium, 73 hard, in ten categories). It is compiled in with `include_str!` rather than loaded as an asset: the round needs its questions the instant the first tap lands, and an async asset would need a loading state and a fallback, while compiled in there is nothing to wait for and nothing that could differ in a replay. It stays plain text to edit; `cargo test -p kit_trivia` checks every line (seven fields, lengths that fit the card and buttons, four different answers, characters Inter can draw, no repeats, ≥ 30 per tier). Answers are shuffled with the seeded `Rng`; nothing repeats within a round.

## Look and sound

**1950s television game-show print.** Mid-century cartoon illustration with bold ink outlines, flat poster colors (tomato red, mustard, teal, a cream highlight) slightly off-register, coarse halftone shading, warm off-white paper, atomic-age starbursts and boomerangs. The style guide is `art/style.toml`; every generated image gets it, with Watt's sheet as the style reference.

- **Generated** (`tools/regen.sh`, sidecars beside each file): Watt's sheet (`assets/host.png`, seven expressions in 320×352 cells: idle, thinking, nervous, happy, star-eyed, flicker, cracked; `src/art.rs` maps each `Mood` to a cell), the stage (`stage.png`: curtains, the teal wall, starbursts, the stage edge; cover-fitted), ten category emblems (`cats.png`, in `look::category` order), the marquee sign the title is lettered on (`marquee.png`), the starburst behind the streak multiplier and the golden card (`burst.png`), and the icon.
- **Code-drawn in the same style**: the question card and answer buttons (paper faces, thick ink rims, hard offset print shadows, an inner rule), the stopwatch, the hearts strip, the white-gloved hint finger, sunburst rays behind a thrilled Watt, the flicker (the sheet's flicker frame dims and catches the light in a stutter), smoke, sparkles and confetti in the palette. **All text is code-drawn** (crisp at any size; a reskin is a string change), including the title on the marquee.
- **Loading**: images load asynchronously (the stage, Watt, the emblems and the marquee are startup assets); until one is ready the old code-drawn Watt, a gradient stage, a code-drawn sign and star and a question-mark emblem stand in, so the first frame always looks right.
- **Music**: a big-band swing loop (brass riffs, walking bass, brushed swing drums, piano comping; 8 bars, 12.05 s at 159.4 BPM), split by `mb music --layers` into a calm low layer and a bright high layer. The high layer never drops below half (`HI_FLOOR`) and opens fully with the streak and the clock. A ticking stopwatch loop (`tension.ogg`) fades in from 60% of the clock and peaks in the last seconds while the band steps back; it plays at `TENSION_PITCH` so its tick-tocks fall on the band's quarter notes (loop lengths: 5.95 s with 15 tick-tocks; 12.048 s with 32 beats → 1.0536; the phase between them is arbitrary, the tempo matches).
- **Effects** (ElevenLabs): a brass fanfare sting to start, a card flick, a contestant-panel button click, a bell ding with a brass stab for right (rising with the streak on notes that fit the band, at most a fifth up), a buzzer for wrong plus the bulb's electric fizzle, an alarm bell for time-up, a clock tick for the last three seconds, a whoosh for a streak step, a golden-question fanfare, a harp glissando for a heart back, the bulb's pop with glass and a dying fizzle, a sad trombone for game over, a brass hit for a new best, a radio-switch click for UI. All normalized by `mb` and passing the skill's `check_audio.py`.

## Tuning knobs

All in `src/sim.rs`, top of the file:

| Knob | What it does |
|---|---|
| `TIME_FIXED`, `TIME_BASE`, `TIME_PER_WORD` | The clock: `TIME_FIXED + pace × (TIME_BASE + TIME_PER_WORD × words)`, words = question + half the answers. Raise `TIME_PER_WORD` for slower readers or longer questions. |
| `PACE_DROP`, `PACE_MIN` | How fast the clock tightens per question and how tight it gets. The main difficulty lever: `PACE_DROP` up = shorter first rounds. |
| `TIER2_FROM`, `TIER3_FROM` | The question number where medium and hard questions start regardless of streak. |
| `TIER2_STREAK`, `TIER3_STREAK` | A streak this long jumps a tier early (difficulty ramps with the streak; a miss drops it back). |
| `DAILY_TIER2_FROM`, `DAILY_TIER3_FROM` | The fixed tier schedule of daily rounds. |
| `POINTS`, `SPEED_FLOOR` | Points per tier at full speed, and the share an answer gets at the buzzer. |
| `MULT_STEPS` | Streak lengths for ×2, ×3, ×4. |
| `GOLDEN_EVERY` | Every Nth question pays double (0 = off). |
| `HEART_EVERY` | A streak of N gives back a lost heart (0 = off). Lengthens good players' rounds. |
| `LIVES`, `WARMUP` | Hearts per round; how many opening questions are free. |
| `DEAL_TIME`, `REVEAL_RIGHT`, `REVEAL_WRONG`, `ENDING_TIME` | The rhythm: card flip, how long a right / wrong answer is shown, the bulb blowing. |

Host-side feel lives at the top of `src/lib.rs` (`HINT_IDLE`, `HINT_LEFT`, `POP_AT`, `PLAY_HOST`, `TITLE_HOST`, `MARQUEE_W`), in `src/look.rs` (every string, the palette, the theme, category emblems and colors) and in `src/sound.rs` (`MUSIC_VOL`, `HI_FLOOR`, `TENSION_VOL`, `STREAK_STEP`).

## Make it yours

Three changes turn this kit into its own game:

1. **A new host, look and voice** (`art/style.toml`, `tools/regen.sh`, `src/look.rs`). Rewrite the style guide's `style`, `keywords`, `palette` and `[music]`/`[sfx]` words; rewrite the prompts in `tools/regen.sh` (the host's seven expressions keep their order: every `Mood` needs a frame); set `references = []`, run `sh tools/regen.sh host` and look at `art/preview/host.png` until the character is right (`mb regen assets/host.png --new-seed`), put the reference back and run `sh tools/regen.sh` for the rest (~$0.35). Then match the code to the art: `TITLE_LINES`, `TAGLINE`, `HEADING`, the palette constants and `theme()` in `src/look.rs`; `HOST_GLASS` (the glass's centre and radius in a cell) and `MARQUEE_PANEL` in `src/art.rs` if your images put them elsewhere; `TENSION_PITCH` in `src/sound.rs` if you regenerate the music or the stopwatch (see "Look and sound"). Read every preview and listen on a phone. Ideas:
   - a nervous robot whose antenna sparks on a streak and who shorts out on game over;
   - a fortune-teller cat with a crystal ball that clouds over as the clock runs down;
   - a game-show cactus that wilts a little with every miss;
   - an owl professor whose glasses fog up when the clock is low;
   - a deep-sea quiz: an anglerfish host whose lure is the timer.
2. **Your own questions** (`src/questions.txt`, `look::category`). A theme narrows the bank and gives the game its identity; keep the format and the tests tell you what doesn't fit. Ideas:
   - one subject done deep (space only, animals only, the human body);
   - "true or false" (two answers: let `questions::parse` accept two-answer lines, shuffle two slots in `Sim::deal`, and lay out two big buttons in `Geom::new`);
   - "higher or lower" numbers (which is bigger: two answers generated in code instead of a bank);
   - emoji or picture questions (draw the clue with shapes in the card instead of text);
   - a kids' edition: tier 1 and 2 only, slower `TIME_PER_WORD`, no `PACE_DROP`.
3. **One new rule or power-up** (`src/sim.rs` `resolve` / `deal`, plus a `Cue` for its juice). Ideas:
   - **50:50**: a lifeline earned every 5 in a row that greys out two wrong answers (a `Phase::Ask` flag the buttons read);
   - **Freeze**: a one-time tap on the ring that stops the clock for 3 s;
   - **Sudden death round**: after question 15, one life only but ×5 points;
   - **Category roulette**: before each card a wheel spins to pick the category (a short phase before `Deal`, filtering `draw`);
   - **Steal**: a "rival" answering on screen (move `tests::Bot` out of the test module and feed it the sim), with points for beating it to the answer.

## Difficulty (bot)

`cargo test -p kit_trivia --profile bot -- --ignored --nocapture difficulty` (200 seeded rounds per preset, endless and daily, reported by the SDK's `bot::Difficulty`: p10/p50/p90 against the targets, each round's questions, right answers, score, best streak and share of misses that timed out, and the tier of the last miss; it fails if a better preset doesn't last longer; `first_timer_rounds_last_20_to_45_seconds` checks the first-timer median on 40 seeds in every `cargo test`).

The bot reads like a person: the clock it beats is reading time (question words at its reading speed, answers scanned at twice that, from the moment the card starts flipping in) plus a decision time plus a 150–350 ms reaction. Per question it either knows the answer (chance by tier) and picks it after a short think (with a small slip rate, the fat finger), or guesses after a longer think (sometimes ruling one wrong answer out first). When its plan would land after the buzzer, it either taps whatever it leans toward in the last second ("clutch") or freezes and times out. The good preset reads faster and knows more; that is its anticipation.

| Preset | Reading | Knows (easy/med/hard) | Median round | p10–p90 | Questions | How it loses |
|---|---|---|---|---|---|---|
| first-timer | 3.2 words/s | 86 / 60 / 42 % | 40 s | 33–49 s | 7 | half the misses are timeouts once the clock bottoms out; last miss on a hard question 90% of the time |
| decent | 4.0 words/s | 92 / 74 / 56 % | 51 s | 36–79 s | 11 | 31% timeouts; wrong guesses on hard questions |
| good | 5.2 words/s | 98 / 90 / 76 % | 79 s | 46–140 s | 20 | 6% timeouts; hard questions it doesn't know (hearts come back on streaks of 8) |

Daily rounds (fixed tier schedule): first-timer 43 s (34–52), decent 56 s (42–77), good 86 s (55–160).

## Determinism

All randomness is `Rng`: the round seed from `Rounds::seed()` (a new one each round from the session's seed, or `sys::daily_seed()` in daily mode), the title loop's questions from the session's `Rng::from_host()`, the decks shuffled once per round, each question's answer order from `seed ^ question number`, particles seeded from the game's `Rng`, sound jitter from the sound module's own fixed seed (output only). Time is only `dt`; the host's blinks, bobs and smoke are functions of accumulated game time; nothing changes in `render`. `the_sim_is_deterministic` checks the sim cue for cue.

In the preview (`mb serve --watch`, held session driven with `mb.tap` + `mb.step`): 8542 frames covering the title card's attract loop (several card flips), rounds answered with rotating taps through the bulb popping and the results card, retries by tapping the screen, and more rounds to game over: `await mb.verify()` → `{ frames: 8542, match: true, firstMismatch: null }`, draw hash `48e510aba688deb0`. The generated images load asynchronously and are polled in `update`, so they replay too. `mb.stats()`: `mb_update` ≈ 0.00 ms, `mb_render` 0.11 ms avg (0.2 max), ~640 mb2d quads a frame. `mb build`: `game.wasm` 159 KB, `.mbx` 893 KB (startup set 450 KB); `mb art check`: 5 images, 0.70 MB of assets, ~$0.37 of generation (host.png shows as "older style guide" only because `references` was set to it after it was made).

## Not verified on a phone

- Whether the clock feels fair with a thumb and real reading: `TIME_PER_WORD`, `PACE_DROP` and `PACE_MIN` are tuned against a modeled reader, not people. If real first rounds are much shorter than 30 s, raise `PACE_MIN` first.
- Haptics (success on right, fail on wrong and timeout, ticks on the last three seconds, a heavy thud on the pop) and the audio balance by ear. Measured on a decent bot's 45 s round (`MB_MIX_LOG=/tmp/tr.log cargo test -p kit_trivia mix_log`, then the skill's `scripts/mix_check.py assets /tmp/tr.log --fx-loop tension`): the band (`MUSIC_VOL` 1.3: its base stem is saved quiet) now sits a median +10 dB over all effects together (it was 0 dB, effects louder 50 % of the time, now 20 %); clock ticks and the stopwatch 11–12 dB under it; stings peak at most +7 dB over it and duck it 4 dB, not 9. Nothing generated was heard at all: the music loop's seam, the stopwatch's tempo lock (`TENSION_PITCH`), the ding's climb (measured onto notes of E minor/A minor; the swing tune is chromatic, so the key fit is only 0.73) and every effect's character were judged from numbers only.
- The art at phone resolution: Watt's cells are 320×352 px, so the title (×1.4) and results (×1.5) draw him at ~1.7 texels per unit (soft on a 3× screen); the stage is drawn from 720×1280 with 24 colors (check the teal wall for banding).
- The layout with real safe areas: everything is laid out from `Layout::safe` / `Layout::card`, and `the_layout_fits_phones_and_the_preview` checks a 59/34 pt and a 47/34 pt inset phone plus the preview for overlaps (buttons ≥ 44 pt, Watt and the clock clear of the pause pill and HUD, the title card inside the card area and clear of DAILY). Only the desktop preview (no top inset) was looked at.
- Text measurement on device: card and button text is fitted with `gfx2d::measure`, the same on every host, but the longest lines were only checked in the preview.
