# What makes a Maimbrain game engaging

The audience is someone swiping a vertical feed on their phone, one-handed, often with sound off, deciding in about a second whether your game is worth a tap. They didn't choose your game and won't read anything. They will instantly feel whether it responds to them. Everything below follows from that.

## 1. The first second: the card sells the game

The title screen isn't a menu. It's the feed card: an advertisement playing live, and an ad, not a tutorial. A card takes no input (no touches, no sensors), so it must never ask for an in-game action. "BLOW!", "TILT!", "SHAKE!", "DRAG" or an animated phone icon on a card invite something that does nothing, and the player feels the game is broken. Its job is to make a stranger *want* to play. Tapping to play is the platform's job (it teaches that itself), so the card doesn't say "TAP" either.

- **Show the game, not a logo.** The first frame shows the character and the play space mid-action: ball mid-bounce, beer mid-pour, tower mid-wobble. The title text is small; the scene is big.
- **Motion from frame one.** Something on the card is always moving (idle animation, swaying object, blinking eyes). A static card reads as a picture and gets swiped past.
- **A clear hook.** One funny or intriguing thing a stranger gets in one glance: a character with a face and a problem.
- **Stakes and drama, not steps.** Loop the most tempting moment: a near-miss, the wobble before the tower falls, the candle that almost relights, the face just before disaster. Tease the payoff without resolving it.
- **One line of desire, if any**: a tagline or challenge rather than an instruction. "90 CANDLES. ONE GRANDMA." / "DON'T LET BRIAN GO THIRSTY." / "HOW HIGH?" (these show the shape; write your own in the game's voice: copies of them are all over the feed) Add the player's best ("BEST 42") once they have one: it's a reason to come back.
- **How to play waits for play.** Every "how to" (BLOW, TILT TO POUR, the phone icon, arrows, ghost fingers) appears in the first seconds after the tap, when the action actually works (§2).

## 2. Instant, obvious interactivity

- **The first tap starts playing**: no menu, no "press start", no settings, no tutorial screen. The tap that entered play can be the first move.
- **Respond within one frame**, every time, on every input: something moves, squashes, flashes or makes a sound. A tap with no visible response feels broken.
- **Teach by doing, with on-screen affordances**, not words:
  - Arrows, ghost fingers, dotted paths and target rings show *where* and *which way*.
  - A pulsing outline marks what to touch; an arrow shows the direction to swipe or tilt.
  - A live indicator mirrors the control (a needle, a tilting glass, a power bar), so the player sees their input's effect even before it matters.
  - Hints persist until the player performs the action once, then fade. Bring them back if the player stalls (no input for ~3 s).
- **≤ 5 words of text at a time**, big (pixel font 16+), with a dark shadow or panel behind it. Verbs, not explanations: "TILT TO POUR", "TAP TO JUMP".
- **The first 3 seconds always succeed.** The opening is a no-fail warm-up: generous timing, slow pace, a guaranteed early win (first coin, first sip, first block placed) with a reward burst. The player should feel competent before they feel challenged.
  - **Easy isn't slow, and fast isn't frantic.** In anything about speed (runners, racers, flyers), the opening must *feel* fast from the first second while staying forgiving. Get the feeling from the camera and the world, not raw velocity: a low camera, a field of view that opens with speed, `speed_blur`, streaks, things whipping past close to the camera, a rising whoosh. Bound the real speed by reading time: at any speed, a hazard must be visible and recognizable at least ~1.0 s (early game ~1.5 s) before the player has to act, plus the time the action itself takes. If testers say it's "unhinged" or they die to things they never saw, it's too fast: slow it and add sensation instead.

## 3. Juice: make every action feel good

Juice is the feedback layered on an action so it feels physical. It's the difference between "works" and "fun".
- **Squash and stretch** on impacts and launches; **anticipation** (wind-up) before big moves.
- **Particles** for every success (sparkles, splashes, crumbs) and a bigger burst for milestones.
- **Screen shake**, small (2–4 units, 0.1–0.2 s) for hits, bigger for failure. Never constant.
- **Pops and easing**: numbers scale up then settle when they change; things ease in and out, never snap (except to show impact).
- **Sound on every action**, pitched up with streaks/combos (each consecutive success a semitone higher feels great).
- **Haptics** in step with the big visual beats: a tick on each small success, a thud on impact and failure. Silent mode must still feel physical.
- **Hit-pause**: freeze 50–80 ms on a big impact. It makes hits land.

## 4. Readable tension

- **Show danger before it kills.** Every fail state has a ramp the player can see: color shifting, a face reacting, shaking, a meter filling, a warning sound speeding up. The best tell is a character's face: wince, sweat, panic.
- **Show near-misses.** "SO CLOSE!" or a slow-mo on a narrow escape makes players feel skilled and want another try.
- **Always show why you failed.** Freeze or slow the moment, point at the cause, then play the fail animation. An unclear death feels unfair, and unfair means swipe.
- **Fair randomness**: random within designed bounds, never an unwinnable situation, and never two nasty surprises in a row.
- **One action, one look.** When different hazards need different moves (jump over vs duck under, tap vs hold, left vs right), each move gets its own color *and* shape *and* height, used for nothing else, and the player learns the mapping in the first seconds with each hazard's first appearance. Being able to *see* a hazard isn't enough: at speed the player must know in a glance which move it asks for. Test it in greyscale screenshots: the shapes alone must say "over" or "under".

## 5. The loop: short rounds, instant retry, constant novelty

- **Round length**: a first-time player's first round ends in 20–45 s; a good player's lasts 1–3 min. Sessions are a few rounds, not one long one.
- **Escalate fast.** Every ~10–15 s something gets faster, narrower or new. A new element (obstacle, shape, enemy, rule) every 15–30 s keeps people watching *and* players playing.
- **The world changes as you get further.** Progress has to be visible in the place itself, not only in the score: new zones or biomes every ~30–60 s (the canal opens into a lake, the town gives way to bamboo, the city to a festival), the time of day or the weather turning (dusk → night → dawn, rain coming in), landmarks that mark distance (the big gate at 500 m), and music that moves with it. A world that looks the same at 3 minutes as at 3 seconds tells the player they've seen everything. **Zones must be unmistakably different**, not the same scene re-tinted: each gets its own palette and sky (time of day, weather, clouds), its own signature props and silhouettes on the skyline, its own ambient life, its own hazard or twist, and its own music variation. The test: shown one screenshot from mid-zone, a stranger can name the zone.
- **A cold open sets the stakes.** In a chase or a quest, the first 1–2 s after the entering tap tell the story without text: a different camera angle shows what the player wants and what's after them (Temple Run's idol grab and the demons bursting out), then the camera swings behind the player and control begins. Never longer than ~2 s, and any input skips it. On later runs it can be shorter, but keep the beat that shows the threat.
- **Power-ups end safely.** When a power-up or transformation ends, the player must land in a clear stretch (≥ ~1.5 s of open course ahead), with the end telegraphed (flashing, a sound, a countdown) so they're ready. Dying the instant a reward runs out feels like a betrayal.
- **Failure is the punchline, and the threat delivers it.** Make the fail animation funny, gross, spectacular or satisfying, 1–2 s long. It's the most-shared moment and the game-over card. If something is chasing the player, *it* should end the run (it catches, snatches, swallows), not a generic explosion: the payoff of being chased is being caught.
- **One-tap retry.** The game-over screen's tap target is the whole screen (minus a small separate button like SCORES), and retry starts play instantly.
- **Progress pull**: show the score big, the best, and the gap ("3 MORE TO BEAT YOUR BEST"). Celebrate a new best in the game's own style (a ribbon, a stamp, a lamp, a line on a receipt).

## 6. Cards: three screens strangers see

The platform turns three screens into feed cards, all shown live but untouchable, with the title and creator over the bottom ~70 pt and buttons down the right ~56 pt.
1. **Title card**: the ad (§1). Character, scene, hook, a tempting looping moment, maybe a tagline. No instructions.
2. **Game-over card**: what you see swiping back to a game you just played. Show the fail moment's aftermath (the mess, the reaction), the score big and BEST ("NEW BEST!" or "3 MORE TO BEAT IT"). It's a brag and a dare, not a menu: no "TAP TO RETRY" (the platform handles tapping, and a tap does retry). It should make a stranger want to try.
3. **Replay card**: your best run (or a friend's) playing back. It must be watchable with no input: make the state obvious from the visuals alone (score counter, danger tells, events with clear animations).

## 7. Phone ergonomics

- One thumb, portrait. Tap targets ≥ 44 pt (≈ 44 logical units at 360 wide). Primary actions in the lower-middle, never under the card overlays (bottom ~70 pt, right ~56 pt) or the pause pill (top-left).
- Don't make players cover what they need to see: put the action above the thumb, or let a tap anywhere count.
- **Motion controls**:
  - Calibrate relative to how the phone is held when play starts (and after a pause), so it works standing, slumped or in bed.
  - Keep the needed range within ~45° of rotation so the screen stays readable, amplifying in the game if the action needs more (`SKILL.md` §Platform facts).
  - Don't add lag: phone tilt is already smooth (fused gravity, SPEC §5.7). Smooth ≤ ~10 ms with a small dead zone near neutral, and keep the total from hand to screen ≤ ~60 ms.
  - Show a live on-screen mirror of the reading at all times (a tilting object, a needle or bubble level).
  - Teach the direction with an animated phone icon and arrow.
  - **Decide what tilt *is* in your game's world.** Tilt mapped straight to a position (16° = the outer lane) feels like a precise left/right control; tilt mapped to a sideways *push* (acceleration, with momentum, drag and a top speed) feels like steering something heavy that leans and drifts: a boat listing, a fish banking, a marble rolling. Both are valid; pick the one the fiction asks for, and let the character's body show it (lean into the turn, bank, trail a wake). **Momentum is a look, not a delay**: the character must answer the hand within a frame or two (bank, tilt its body, start sliding); weight comes from how it moves and settles, never from inertia that holds back the response. Inertia that puts the player 50+ ms behind the hand reads as lag.

## 8. Audio design

Many players have sound off, so it never carries the game. For those who have it on, it's half the juice. Make it good:
- **Every moving thing makes sound**: footsteps on its stride, joints and engines that follow their speed, landings as hard as the fall, all placed where they are (`maimbrain::foley`; assets.md §5). A walking, flying mech in silence looks fake however good it looks.
- **Every action has a sound**, and the important ones are distinct at a glance-equivalent: a success, a near-miss, a fail and a milestone should be recognizable with eyes closed.
- **Vary repeats**: the same sample 10 times a second machine-guns. Vary pitch ±3–5% per play, alternate 2–3 variants, and let streaks climb a scale (each consecutive success a step up).
- **Sounds in key with the music**: pitch melodic effects (pickups, chimes, combos) to notes of the music's scale so they sit inside it rather than clash.
- **Pickups** (coins, gems, pearls: the sound a player hears most) have to cut through on a phone speaker without tiring the ear:
  - Short (80–150 ms) and bright, with a fast attack (a few ms): the attack is what's heard over the music; a short sound keeps its energy over time low. A soft low pluck (around 300 Hz) vanishes on a phone at any sane level.
  - Layered: a tonal "ding" in the music's key with its body at 1–4 kHz (where the speaker and the ear are best), plus a transient click or sparkle on top. Two layers are louder to the ear than one pure tone with the same energy.
  - On a pitch ladder for streaks (each pickup a scale step up, starting over after a pause), with a sweetener on milestones (every 5th or 10th, a full row): an extra sparkle, a chord, an octave up.
  - Big pickups (a power-up, a rare gem) dip the music 1–2 dB for ~80 ms, like a sidechain: `Bus::Music.volume(0.8, 0.01)`, then `Bus::Music.volume(1.0, 0.08)` a few frames later.
  - `mix_check.py` measures it (assets.md §5): every pickup at least half as loud as the music on a phone, clear of it in its own band.
- **Music**:
  - The bar is a track a player would hum: a melodic hook, chords that move (not one chord under a riff), drums that match the game's energy, and an arrangement that changes over the loop (a B section, a fill, a layer entering) instead of 2 bars repeated. If it sounds like a test tone with a beat, it isn't done.
  - For a real instrumental track, use `mb music` (assets.md); synthesized loops (`sfx.py`) suit chiptune and arcade identities, not "a soundtrack".
  - A short loop (8–16 bars) with a clear identity in the first bar.
  - Built in **stems** (base + tension layer, generated together by `Song.stems` and saved with `save_stems` so they keep their balance) so it can intensify by raising a layer's volume, not just by pitching the whole loop up, which sounds cheap past ~5%.
  - **Not the same loop for the whole game.** A chase-runner player's complaint, "the music is repetitive and doesn't change with the action", is about both: one 8-bar loop heard for two minutes, whatever happens. Give each place its own theme and each theme two or more variations, all in one key and tempo on one bar grid (`mb music … --zones "…" --variations 2`), switch on the bar when the world changes (a stinger into a new zone, a crossfade otherwise), and rotate a zone's variations every few bars. `maimbrain::music::Director` does the switching and rotation (SKILL.md "Music that follows the game").
  - **Characters get their own song.** A duel game's owner: "the music is not differentiated enough." Variations of one piece all sound like one piece; when each boss or arena is a character, give each a song in its own genre that matches its personality (the dock brawler's bluesy hard rock, the showboat's flamenco-tango), still reacting to the fight with its own tension kit, and change songs at a phrase end under a stinger (`mb music … --zone boss=… --zone …`, `music::Songs`).
  - **A threat should be heard closing in.** Players expect a chase to get anxious when they're almost caught. With a generated track, `mb music --layers base,hi,tension` makes the pieces (a tension layer, a clock, a heartbeat, a sting, all on the track's beat and in its key); SKILL.md "Music that follows the game" maps the danger onto them: the tension layer rising, the music narrowing, a heartbeat quickening on the beat, a sting on a near-catch, and a clear release when the player gets away. The release is half of it: relief only lands if the dread was audible.
  - Start it with play; on the title card keep only a quiet bed or none.
- **Mix**:
  - Effects louder than music; music about −6 dB under effects (`save_ogg(kind="music")` does this).
  - Duck the music briefly for big moments (a fail, a boss) and let a short stinger play.
  - Stop or filter the music on the game-over screen so the punchline lands.
- **Tone**: plain full-volume square waves are harsh. Use the presets (filtered leads, plucks, triangle bass, detuned pads), short envelopes for effects, and a touch of reverb on big sounds only.
- **Silence is a tool**: a beat of quiet before a payoff (the freeze before an explosion) makes it hit harder.
- **Haptics follow the same beats** as the sound: a tick per small event, a thud on impact, rising pulses with tension.

## 9. Personality

- **A character with a face beats an abstract shape.** Faces react to everything: delight on success, worry near danger, horror on failure.
- **Humor** lands through animation timing (anticipation, overreaction, a beat of stillness before the payoff), not text.
- **Characters who talk are funny, not banal.** Specific jokes about what just happened beat generic taunts; each character's humor comes from their want and flaw, with one running gag that pays off; setups and callbacks across a run; the funny word last; short. The hero is the cool one (quiet confidence, dry one-liners), the opponents are loud. ([assets.md, "Voice and characters"](assets.md#voice-and-characters) has the rules and examples.)
- **No inside baseball.** A player never needs a name they haven't seen on screen: about five proper nouns in the whole game, at most one new one per scene, none for factions or lore; describe by what's visible ("the big guy with the crane"). The test: could a 10-year-old retell the story after one play, in one sentence?
- **Cartoon slapstick and puns**, when the game is a comedy (not every game is: a tense, eerie or earnest one keeps its tone): the setup is on screen (the arm just fell off), the bark is the punchline with the pun as its last word, and the mech or portrait shows it (a beat of stillness, then the collapse; smug to horrified on the punchline).
- **A villain turn is clear and final.** A trusted figure who turns evil has a plain evil want, makes a cruel choice the player sees, and isn't redeemed at the last second; one early line sets it up, and the hero gets one short, hurt, decided line before the fight.
- **A story in one sentence, with a tone arc.** One-line premise, one twist, no exposition. Light at first, escalating, then a turn to drama for the finale (set up early, jokes dropped, music and pacing carrying it), then a small release so the run ends on a smile.
- **Cohesive look**: one style the whole game shares (crisp pixel art unless the identity says otherwise; `art/style.toml`), a limited palette (8–16 colors), one consistent pixel size, and strong value contrast between the player, hazards and background so silhouettes read on a small screen. Not a smooth flat-vector cartoon with thick outlines and cel shading unless the user asks for one.
- **Music**: a short, catchy loop (8–16 s) that can intensify (layer or pitch up with tension). Keep effects louder than music.

## 10. A reason to keep going

A runner, a dodger or a collector is a loop of actions; what makes it a game is that the player knows *why*:
- **A premise in the first seconds, shown not told**: who the character is, what they want, what's after them. One beat after the entering tap (the shadow sweeping over the water, the gate slamming behind, the thief grabbing the pearl) is worth more than any text. If a stranger asks "why is it doing that?", the premise is missing.
- **A threat you can feel**: pressure the player sees and hears getting closer: the chaser's shadow or sound growing when they slip, a timer they can read, a world collapsing behind them. A danger that only exists in the rules (a stumble counter) isn't pressure. Bring the threat on screen at the start, at every slip, and at the end.
- **Every collectible has a purpose the player understands**: it buys, unlocks, powers up, heals or scores something the player can see (a meter that fills toward a power-up, a revive that costs them, a shop of upgrades or looks between runs, a multiplier). Coins that only add to a number feel like busywork. Say what they're for the first time one is picked up.
- **The best rewards cost a risk.** Put the valuable pickups where getting them means a choice: next to a hazard, on the risky line through a turn, at the edge of a leap, behind a near-miss. Give collectibles **tiers** (common, rare, a jackpot) with clearly different looks and sounds, and keep the play space tight enough that collecting everything isn't free. A wide, safe lane full of coins is a chore, not a decision.
- **A world that feels alive**: something moves everywhere you look besides the player: ambient life (birds, fish, fireflies, petals, people's silhouettes in windows), weather and light (drifting clouds, mist, flickering lanterns), and the world reacting to the player (ripples, birds scattering, lanterns swinging as they pass). An empty, static world reads as a tech demo. **The sky is half the screen: fill it too** (birds crossing, sky lanterns rising, kites, shooting stars, drifting clouds, the chaser circling overhead). Use instancing for crowds of small things; most of them can be particles.
- **A chase the player feels**: if something is after the player, they must see it, hear it and feel it close in: on screen or its shadow at the edges, its sound growing, the music tightening, a near-miss that shows its claws or beak inches away, the camera shaking with its strike. If a player can't say whether they're being chased, they aren't.
- **Creatures that feel alive.** Anything with a body (the chaser, the hero, birds, fish) needs continuous, organic motion: never linear paths or instant turns (follow a spring-smoothed path and bank into turns), secondary motion (breathing, head looking at its target, tail and wing lag, a flap cycle that changes with climb and glide), and anticipation before every big action (the heron pulls back before the strike). A creature that snaps between poses or slides along a line reads as broken, not threatening.

### Writing an ending

A story game that stops on its last hit ends abruptly: a player who just won wants a moment to feel it. Give the ending 45–60 s (an intro stays a few seconds; an ending has earned more), as a cutscene on `ui::Sequence` (UI.md "Cutscenes"), in five beats:
1. **The defeat.** Hold on the loser: their battered portrait (low health keeps the cracked glass and the alarm), a last line that's in character (proud, bitter, funny), then the fall as a `.cue("fall")` the game animates. Slow the camera down; nothing more is at stake.
2. **The cost.** What winning took: the damage on the hero's machine, a teammate's sacrifice, the arena in ruins. One line or one shot. A victory with no cost feels cheap.
3. **The hero's reaction.** The winner's face and a line that says how they feel about it now: relief, respect for the rival, a joke. This is the beat players remember.
4. **The world after.** A title card ("ONE YEAR LATER", "THAT NIGHT") over a wide or rising shot of the world changed by the win, and a **closing line** that ties back to the premise or teases a rematch.
5. **Credits with the cast.** Roll the characters with their roles (`.credits(&[("THE CHAMPION", "GRANNY"), …])`), the camera craning up and away, then fade out to the results card.

Keep it watchable and skippable: lines short (a sentence each, voiced with `mb voice`), letterbox bars on (`.letterbox(0.22)`) so it reads as a scene, the lines in the game's own panel if it has one (`.draw_line(|l| ..)`, UI.md "Cutscenes"), the music swapping to an ending theme or dying under the voices (`.cue("music")` and the Director, or `.sound(sting, 1.0)`), a tap hurrying each beat and a held finger skipping the rest. The gallery's ENDING page is a 20 s version of these beats.

## Checklist

**First second**
- [ ] A still of the title card alone tells a stranger what the game is about.
- [ ] Something on the title card is always moving.
- [ ] The title card gives no instructions (no BLOW/TILT/SHAKE/TAP, no phone icons or arrows): it sells the game with stakes, character and a tempting looping moment.
- [ ] A tap anywhere (not under overlays) starts play immediately; the first tap can be the first move.
- [ ] The first seconds of play show how to play with animated affordances, right where the action works.

**First ten seconds**
- [ ] Every input gets a visible response the same frame, plus sound and (in play) haptics.
- [ ] Hints show where and which way, persist until performed, and return if the player stalls.
- [ ] The opening seconds can't be failed, and there's an early win with a reward burst.
- [ ] No text block over 5 words; key text is ≥ 16 px with a shadow or panel.

**Tension and failure**
- [ ] Every fail state has a visible ramp (face, color, shake, meter) before it triggers.
- [ ] The cause of failure is shown clearly; the fail animation is a 1–2 s payoff.
- [ ] Difficulty escalates every ~10–15 s, with something new every 15–30 s; the bot confirms a first-timer's round lasts 20–45 s.

**Why play**
- [ ] A stranger can say who the character is and what's after them within 5 s of play.
- [ ] The threat is on screen and gets visibly closer when the player slips.
- [ ] Every collectible's purpose is shown the first time one is collected.
- [ ] The valuable pickups sit where they cost a risk, and collectibles come in visibly different tiers.
- [ ] A speed game feels fast from the first second without being frantic: every hazard is readable ≥ ~1 s before the player must act.
- [ ] A cold open (≤ 2 s, skippable) shows the stakes and the threat.
- [ ] Power-ups end in a clear stretch, with the end telegraphed.
- [ ] Every moving thing makes sound: steps on its stride, joints, engines or thrusters following their speed, impacts scaled by how hard, placed by distance and side (`maimbrain::foley`).
- [ ] Creatures move organically (smooth paths, banking, secondary motion), checked with `mb shot --smoothness --track-node`.
- [ ] Hazards that need different moves look different in greyscale.
- [ ] Something besides the player moves in every part of the screen.
- [ ] The world visibly changes as the player progresses (zones, time of day, landmarks), not only the obstacles.
- [ ] The hero is a recognizable, living character (generated or organically built and animated), not primitives.
- [ ] Characters who talk: every line read aloud is specific to its moment (no generic taunts), each character has a running gag that pays off, the hero is cool rather than loud, the story fits in one sentence, and a multi-fight game turns from light to dramatic for its finale and ends on a small release.
- [ ] No inside baseball: every name a line uses was on screen first (≤ ~5 proper nouns in the game, no faction or lore names), and a 10-year-old could retell the story in one sentence.
- [ ] Jokes land in two seconds: the setup is on screen, the pun or punchline is the last word, and the character's body or face shows it.
- [ ] A villain turn (if there is one) is unambiguous: an evil want said plainly, a cruel act on screen, no redemption, set up by one early line, and a clean beat for the hero.

**Loop and cards**
- [ ] Score is big and pops on change; best is saved and "NEW BEST!" celebrates.
- [ ] Retry is one tap; the game-over screen is a good card (aftermath, score, best), with no instructions.
- [ ] A replay is understandable with no input.
- [ ] Nothing important is under the card overlays (bottom ~70 pt, right ~56 pt) or the pause pill.

**Platform**
- [ ] `sys::round` reports Idle / Playing / Over; the score is submitted before Over.
- [ ] Works fully with sound off; haptics never carry information alone.
- [ ] Music loops seamlessly, intensifies with layers, starts with play and gets out of the way on game-over.
- [ ] Effects are varied, in key, louder than the music, and no sound is harsh or clips.
- [ ] Pickups are short, bright and layered, climb a ladder on streaks, and are heard on a phone speaker (`mix_check.py`).
- [ ] The first frame looks good before any asset has loaded.
