# UI and juice kit

The SDK ships a UI and motion kit so a game's menus, HUD and "juice" look finished without hand-rolling them: `maimbrain::motion` (easing, tweens, springs, shake, counters), `maimbrain::juice` (confetti, sparkles, popups, a combo meter, flashes) and `maimbrain::ui` (themes, shapes, text, icons, buttons, meters, layout, and ready-made title, results and HUD screens). It's plain Rust on top of mb2d (SPEC §5.3) plus three small host calls for anti-aliased shapes and styled text.

`games/gallery` shows every piece on its pages under the 17 identities and the five original themes. Build it with `mb serve games/gallery` and page with the arrows (or ← →, 1–9; T or the button top-right cycles the theme; `--param page=N` in `mb shot` opens page N). The STATS page (`page=11`) is the between-fights card over a stand-in arena, a different wipe each time, with its sounds (`--param theme=N` starts on theme N; `--param ink=navy` gives the theme dark ink on a dark panel, which the card must still read on); DAMAGE (`page=12`) is a portrait beaten from full health to near death; ENDING (`page=13`) an epilogue on `ui::Sequence`. AIM (`page=10`) is a target range aimed by pointing the phone with `sensors::Scope` (SPEC §5.7 attitude): tap to fire, RECENTER to make where the phone points the middle, DEFAULT / PRECISE (or M; `--param aim=precise`) to compare the scope's default light-gun mapping (`Scope::new`: 32° across, a fixed 8 ms of smoothing, the same gain at every speed) with `Scope::precise` (40° across, speed-adaptive smoothing, a slower gain for slow turns, friction on the targets); it starts on DEFAULT, with a readout of the sensor, the angles, the turn rate, the filter, its smoothing right now and the gain's multiplier right now; in the preview the mouse points the phone. It draws in Inter: the identities' fonts aren't baked into it (see [Identities](#identities) for them with their fonts).

## The rules it keeps

- **Determinism.** Everything advances only by the `dt` you pass to `update` and draws from its own seeded `Rng`, so runs replay exactly (SPEC §3). Update kit state in `Game::update`, draw it in `Game::render`, seed particle pools from your game's `Rng` (or `sys::rand_seed()`), never from anything else.
- **Feed cards.** The title and game-over screens are feed cards (SPEC §5.2): live, untouchable, with the platform's title strip over the bottom ~70 pt and buttons down the right ~56 pt. `Layout::card` is the area clear of both (symmetric, so centred things stay centred); the screens keep their text and buttons inside it. The title card sells (a name, a hook, the best), never "tap to play".
- **The platform's own UI.** The pause pill (top-left in play) and leaderboards are the platform's. The kit has no pause button or score list; `Layout::pill_zone()` is the corner to keep clear, and a SCORES button should call `store::show_scores`.
- **Size.** Only what a game calls is linked. Icons are function pointers, so naming `Icon::TROPHY` links the trophy alone. Measured with `mb build` (wasm-opt): the 2D template went from 24.6 KB to 84.0 KB of wasm (+25 KB gzipped) with the title card, HUD, results card and pop juice; Stack from 42.3 KB to 102.3 KB (+25 KB gzipped). The styles and identities add more, since the screens can draw every layout: the 2D template using `Theme::from_identity` is 160.9 KB, Stack 192.5 KB, the gallery 222.9 KB. Each game font adds its atlas (25–60 KB, shown by `mb font add`).

## Host calls (mb2d)

| Call | What |
|---|---|
| `gfx2d::rrect(x, y, w, h, radius, stroke, feather, top, bottom)` | Signed-distance rounded rect, anti-aliased under any transform. `radius` clamps to half the short side (pills, circles); `stroke` > 0 draws only a border; `feather` blurs the edge (shadows, glows, soft dots); vertical gradient. |
| `gfx2d::text_style(TextStyle { weight, outline, outline_rgba, soft })` | How distance-field fonts (Inter and game fonts) draw until changed or the frame ends: bolder or lighter, an outline, a soft outer edge. In ems, up to ~0.12 em in total. |
| `gfx2d::antialias(true)` | `poly` gets a one-pixel soft fringe and `line` smooth edges until turned off or the frame ends. |

Game fonts (mb2d 2) are `Font::asset("assets/fonts/<name>.mbf", fallback)`, baked by `mb font add`: a `Font` is a small `Copy` value usable in any theme or text call, drawing in its fallback until its asset is ready. It isn't a `const fn` (its first call starts the load and claims one of 16 slots), so keep the path in a `const` and the `Font` in your state or `Theme`; calling it again with the same path is cheap and returns the same font. The kit wraps these; most games never call them directly. The Inter SDF atlas is baked with exact distances at 8× oversampling (`tools/fontbake`), so outlines and glows stay smooth. Text is crisp at any size; measuring is unchanged.

## motion

```rust
use maimbrain::motion::*;

let mut fade = Tween::new(0.0, 1.0, 0.4, Ease::CubicOut).delay(0.2);   // any Lerp value: f32, [f32; N]
let mut pos = Spring::bouncy(0.0, 3.0);  pos.target = 120.0;           // critical(), bouncy(), wobbly(), new(v, hz, damping)
let mut cam = Follow::new(Vec3::ZERO, 0.15);  cam.update(player, dt);   // critically damped chase of a moving target (f32, Vec3)
let mut seq = Seq::new(0.0).to(1.0, 0.3, Ease::BackOut).wait(0.5).to(0.0, 0.2, Ease::QuadIn);
let k = Ease::BackOut.at(stagger(t, i, 0.05, 0.4));                    // item i of a staggered group
let mut shake = Shake::new();  shake.add(0.5);                          // trauma: 0.25 small hit, 0.8 death
let mut punch = Punch::new();  punch.kick(0.3);                         // scale() jumps and springs back
let mut score = Counter::new(0);  score.set(1500);                      // rolls up; update() says when it ticked
let mut stop = HitStop::default();  stop.freeze(0.06);                  // sim_dt = stop.step(dt)
let mut flash = Pulse::new(0.3);  flash.fire();                         // value(): 1 → 0
// each frame, in update:   fade.update(dt); pos.update(dt); seq.update(dt); shake.update(dt); ...
// in render:               gfx2d::push(); shake.apply(180.0, 320.0); /* scene */ gfx2d::pop();
```

`Ease` has linear and quad, cubic, quart, quint, sine, expo, circ, back, elastic and bounce in in/out/in-out (`Ease::ALL`). Springs are integrated exactly, so they're stable at any `dt`. `Follow` is the one for anything that moves after something else (a camera, what it looks at, a pet trailing the player): a critically damped spring solved in closed form, with its velocity carried over, so its path has no kinks, it never overshoots a target that stops, and 30, 60, 120 and 144 Hz trace the same path. Its `halflife` is the time to close half the gap; `update_angle` follows an angle the short way round. It trails a target moving steadily at speed v by `lag(v)` ≈ 1.19·v·halflife; `Follow::lag_for(halflife, v)` gives the same without a `Follow` at hand, and `motion::HALFLIFE_W` is the constant behind both (ω = `HALFLIFE_W` / halflife). Helpers: `lerp`, `inv_lerp`, `remap`, `damp` (frame-rate independent first-order smoothing, for numbers rather than motion), `approach`, `wrap_angle`, `noise1`, `Timer`.

## juice

```rust
use maimbrain::juice::*;

let mut fx = Particles::new(rng.next_u32() as u64);   // a pool (600 max; the oldest give way)
fx.confetti(x, y, 80, &theme.confetti);               // paper that tumbles and flutters
fx.confetti_cannons(layout.screen, 80, &theme.confetti);
fx.sparkles(x, y, 60.0, 12, 0xfff2b0ff);              // twinkles
fx.stars(x, y, 10, theme.gold);                       // spinning stars bursting out
fx.burst(x, y, 20, theme.accent);                     // soft additive puff
fx.sparks(x, y, -FRAC_PI_2, 1.0, 30, theme.gold);     // streaks in a cone
fx.ring(x, y, 120.0, 0xffffffc0);                     // shockwave
fx.celebrate(x, y, &theme);                           // ring + stars + sparkles + confetti
let mut popups = Popups::new();  popups.score(x, y, 100, theme.gold);   // "+100" pops, rises, fades
let mut combo = Combo::new(2.0);  combo.hit();        // breaks after 2 s without a hit; multiplier(); draw()
let mut flash = Flash::new(0.3);  flash.fire(0xffffff60);
let mut hurt = VignettePulse::new(0xff2040ff);  hurt.fire();             // or hurt.level = 0.4 for a steady warning
// update: fx.update(dt); popups.update(dt); combo.update(dt); ...   render: fx.draw(); popups.draw(); ...
```

A `Particle` is a plain struct (look, position, velocity, gravity, drag, life, size, color, spin) if you want your own presets: build one and `fx.add(p)`.

## ui

**Theme.** One struct of colors, fonts and a `Style` that everything reads. Start from the game's identity: `Theme::from_identity("haunted carnival, crimson, jittery")` or `Theme::preset("saloon")` (below), then `.load_fonts()` for the library fonts it was designed with, and change any field. `with_accent`, `with_panel`, `with_radius`, `with_font`, `with_style`; `theme.backdrop(rect, t)` paints its background; `theme.case(s)` applies its letter case. The original `Theme::candy()`, `night()`, `arcade()`, `paper()` and `jungle()` remain for existing games, but they are the house look docs/IDENTITY.md describes: don't start new games from them.

**Style** (`theme.style`, ui/style.rs): what makes themes look different beyond color.
- `shape`: `Rounded`, `Sharp`, `Pill`, `Cut` (chamfered), `Wobbly` (hand-drawn), `Pixel` (stepped corners), `Bevel`, `Ticket` (punched notches). Panels, buttons (the round retry button becomes a square, octagon or stepped block), pills and badges follow it.
- `border`: `None`, `Line(w)`, `Double(w)`, `Dashed(w)`. `shadow`: `Soft`, `Hard(dx, dy)` (a solid offset copy: print, neo-brutalism; buttons sink into it), `Glow` (neon), `None`.
- `texture` on panels and `backdrop_texture` on the background: `Grain`, `Scanlines`, `Halftone`, `Stripes`, `Grid`, `Lines` (ruled paper). `backdrop`: `Gradient`, `Flat`, `Rays`, `Vignette`, `Horizon` (a synthwave grid floor).
- `title`: `Drop` (letters drop in and bob), `Stack` (one word per line, left-aligned and huge, with a rule), `Banner` (on a tilted band), `Stamp` (a rubber stamp thumped down), `Typewriter` (typed out with a cursor), `Marquee` (a sign ringed with chasing bulbs), `Arc` (letters on an arch), `Neon` (a tube that flickers on), `Plate` (an engraved plaque with ornaments).
- `results`: `Panel`, `Floating`, `Receipt` (a printed slip with dotted rows), `Scoreboard` (digits in tiles, a lamp for a new best), `Stamp` (the heading stamped over a huge number), `Editorial` (left-aligned label, rule, huge number). `hud`: `Center`, `Corner` (top-right), `Badge` (in a shaped badge).
- `motion`: `Bouncy`, `Snappy`, `Floaty`, `Mechanical` (stepped), `Jittery` (small jolts), `Elastic`. Entrances, idles and staggers in every screen use it (`Motion::enter`, `progress`, `idle`, `spring` for your own animation).
- `case` (`AsWritten`, `Upper`, `Lower`), `tracking` (ems, titles and headings) and `outlined` (titles get `theme.outline` around them).

**Shapes** (`ui::shape`): `rrect` (`shape::rrect(rect, radius, color)` takes a `Rect` and fills it; the host call `gfx2d::rrect(x, y, w, h, radius, stroke, feather, top, bottom)` takes loose numbers and does strokes, feathering and gradients), `rrect_gradient`, `rrect_stroke`, `shadow`, `glow`, `disc`, `soft_disc`, `ring`, `ellipse` (`ellipse_points` for its outline), `arc` (round-ended bands: progress rings, gauges; its angles are a clock face's, radians from 0 = up, clockwise, so a smile through the bottom is `arc(cx, cy, r, w, PI / 2 + 0.4, 3.0 * PI / 2 - 0.4, c)`; add `PI / 2` to an `atan2` angle), `capsule`, `polyline`, `poly` (anti-aliased), `rounded_poly` / `rounded_points` (fillet any polygon's corners), `star`, `sparkle`, `gradient`, `vignette`, `nine_slice` (an image region stretched without stretching its corners).

**Text.**
```rust
ui::text("STACK").size(64.0).color(theme.text)
    .outline(0.08, theme.outline).soft_shadow(0.0, 4.0, 0.06, theme.shadow)
    .middle().draw(180.0, 120.0);                         // returns the bounds
ui::text(long).font(Font::Sans).size(16.0).wrap(280.0).center().draw(180.0, 400.0);
let (w, h) = ui::text("BEST 42").size(18.0).measure();
ui::text(&title).size(56.0).max_width(stamp.w - 24.0).middle().draw(cx, cy);   // shrinks to fit
```
Measure with the same builder you draw with: `text(s).font(f).size(z).measure()` gives the block's (width, height) with wrapping and tracking applied (`gfx2d::measure(font, size, s)` is one line's raw width). `align(Left | Center | Right)`, `valign(Top | Middle | Baseline | Bottom)` (Middle centres the capitals: right for buttons); `.middle()` is `.center().valign(Middle)`, so it replaces an earlier `.right()`: use `.vcenter()` to centre vertically and keep the alignment, `max_width(w)` (shrinks the text, never grows it, until its widest line fits `w`: a title in a fixed box, a name of any length; bitmap fonts step down whole grid multiples, and `fitted_size()` is the size it lands on), `glow`, `weight`, `tracking`, `leading`, `alpha`. The pixel font gets outlines and shadows from offset copies a whole art pixel apart; keep it at multiples of 8 (`theme.fit(font, size)` rounds for you). `fmt_int(12345)` → `"12,345"`.

**Icons.** `Icon::PLAY.draw(cx, cy, size, color)`: play, pause, restart, star, heart, trophy, crown, lock, unlock, check, close, plus, minus, four arrows, home, gear, sound, mute, music, coin, bolt, flag, clock, medal, fire, gem, shield, target, person, bomb (`Icon::ALL`). Original drawings, one color with darker or lighter details, any size.

**Widgets.**
```rust
let mut play = Button::new(Rect::centered(180.0, 420.0, 200.0, 56.0), "PLAY").with_icon(Icon::PLAY)
    .with_haptic(|| sensors::haptic(Haptic::Tap));        // needs sensors = ["haptics"]
let mut retry = Button::icon(180.0, 500.0, 64.0, Icon::RESTART);
// update: for e in &events { if play.handle(e) == Some(Tap::Clicked) { start() } }  play.update(dt);
// render: play.draw(&theme);
ui::panel(rect, &theme);
ui::pill("BEST 42", cx, cy, 32.0, bg, fg, theme.body_font, Some(Icon::TROPHY));
ui::progress_bar(rect, 0.6, theme.accent, track);
ui::segments(rect, 5, 3.5, theme.gold, track);
ui::ring_meter(cx, cy, 26.0, 7.0, 0.25, theme.accent, track);
ui::ribbon("NEW BEST!", cx, cy, 40.0, theme.gold, theme.outline, theme.title_font, -0.05);
```
Buttons are `Primary` (raised, sinks when pressed), `Secondary` (translucent with a border) or `Ghost`; the touch area is at least 44 × 44 even when the button looks smaller. `handle` returns `Pressed` (going down: a tick, a haptic), `Clicked` (released on it) or `Cancelled`; any `Some` means the event was the button's, so don't also treat it as a tap on the game.

**Layout.** `Layout::new()` (once, in init) gives `screen`, `safe` (inside the insets), `card` (clear of the card overlays) and `hud`, plus `pill_zone()`. `Rect` has `centered`, `inset`, `place(Anchor, w, h, margin)`, `split_top/bottom/left`, `columns`, `rows`, `at_least`, `contains`, `lerp`. `Column` hands out rows top to bottom; `stack_v` / `stack_h` lay out a group centred in an area.

## Identities

Seventeen complete themes that look nothing alike, each designed around fonts from the library. `Theme::preset(name)` returns one; `Theme::from_identity(words)` picks the closest to a description and applies any colors, shapes, motions, textures and layouts it names, and logs what it understood (`mb shot` prints it: `identity "…": saloon (wild west); crimson (accent), wobbly (shape)`; `Theme::explain_identity(words)` returns it). Besides an identity's name or a word for its era or genre, it knows colors (red, crimson, orange, mustard, yellow, gold, lime, green, olive, teal, cyan, blue, navy, purple, violet, pink, magenta, brown, black, white, cream: the first is the accent, the second the highlight), shapes (sharp, rounded, pill, chamfered, wobbly or hand-drawn, pixelated, bevel, ticket), motion (snappy, floaty, bouncy, mechanical, jittery, elastic), textures (grain, scanlines, dots, stripes, grid, lined, flat), shadows (hard, glow, flat), layouts (receipt, scoreboard, stamp, typewriter, marquee, banner, neon, arc, plate, editorial) and case (uppercase, lowercase). `mb font add <game> --identity <name>` bakes an identity's fonts, and `.load_fonts()` points the theme at them. The contact sheet [identity/identities.jpg](identity/identities.jpg) shows each one's title, results and HUD with its fonts.

| Identity | Look | Shape · layouts · motion | Fonts (title / body / numbers) |
|---|---|---|---|
| `riso` | Risograph zine: cream paper, fluoro pink, teal misregistration, grain | sharp, hard teal shadow · stack / receipt / corner · snappy | Bricolage Grotesque 800 / Space Mono / Bricolage 800 |
| `crt` | Phosphor-green arcade monitor, scanlines, glow | pixel-stepped · marquee / scoreboard / corner · mechanical | Press Start 2P / VT323 / Press Start 2P |
| `neon` | Synthwave night: magenta and cyan tubes over a grid floor | pill, glow · neon / floating / center · floaty | Monoton / Exo 2 / Orbitron 700 |
| `storybook` | Picture book: parchment, forest green, hand-drawn frames | wobbly · arc / panel / badge · floaty | Fraunces 800 / Patrick Hand / Fraunces 800 |
| `saloon` | Wild west: dark wood, brass, red, ticket stubs | ticket, double border, hard shadow · plate / stamp / badge · mechanical | Rye / Zilla Slab 700 / Rye |
| `swiss` | International style: white, black, one red, lowercase | sharp, no shadow · stack / editorial / corner · snappy | Archivo 800 / Archivo / Archivo 800 |
| `brutalist` | Neo-brutalism: yellow, thick black borders, 6 px offset shadows | sharp · banner / panel / badge · snappy | Rubik Mono One / Space Grotesk 700 / Rubik Mono One |
| `comic` | Comic book: halftone sunburst, red and yellow, outlined caps | sharp, hard shadow · stamp / stamp / badge · elastic | Bangers / Comic Neue 700 / Bangers |
| `gothic` | Blackletter and blood red on black, candle flicker | chamfered, double border · drop / editorial / center · jittery | UnifrakturMaguntia / EB Garamond / EB Garamond 700 |
| `bubblegum` | Pastel pink to mint, soft pills, rounded type | pill · drop / panel / center · bouncy | Fredoka 700 / Nunito 800 / Fredoka 700 |
| `terminal` | Amber monochrome console, dashed boxes, lowercase | sharp, dashed · typewriter / receipt / corner · mechanical | JetBrains Mono 800 / JetBrains Mono / JetBrains Mono 800 |
| `field` | Military kit: olive drab, khaki, hazard yellow, stencils | chamfered · stamp / stamp / corner · mechanical | Black Ops One / Chakra Petch 700 / Big Shoulders Stencil 800 |
| `orbital` | Spacecraft HUD: black-blue, cyan lines, grid | chamfered, glow · typewriter / scoreboard / corner · snappy | Orbitron 900 / Chakra Petch / Tektur 700 |
| `gala` | Art deco black tie: black and gold, wide-tracked didone | sharp, double border · plate / editorial / center · floaty | Playfair Display 900 / EB Garamond / Playfair Display 700 |
| `notebook` | Ballpoint doodles on ruled paper, red pen, highlighter | wobbly · stack / receipt / badge · jittery | Caveat 700 / Patrick Hand / Caveat 700 |
| `stadium` | Scoreboard sports: navy, orange, stripes, condensed caps | sharp, hard shadow · banner / scoreboard / badge · snappy | Big Shoulders Display 900 / Barlow Condensed 700 / Jersey 10 |
| `cozy-pixel` | Cozy pixel RPG: moss green, pumpkin, cream | pixel-stepped, hard shadow · drop / panel / badge · mechanical | Pixelify Sans 700 / Pixelify Sans / Pixelify Sans 700 |

An identity is a starting point, not a uniform: change the palette and fonts to the game's own (the skill's identity.md). Two games on the same preset will still look alike.

## Screens

**`TitleCard`**: the name in the theme's title layout (dropping in letter by letter, stacked, on a banner, stamped, typed, on a marquee, arched, in neon, on a plaque), a tagline, the best in a pill. It leaves the middle of the screen to your live scene.

**`ResultsCard`**: in the theme's results layout (receipt, scoreboard, stamp, editorial; or the original:) a heading straddling a panel, the score rolling up (punched when it lands), then the best, or, if the score beat a previous best, a "NEW BEST!" ribbon stamping in with confetti, cannons and a gold rim; "3 MORE TO BEAT IT" when it was close; a round retry button; an optional extra button. `.floating()` drops the panel and floats outlined text near the top, for games whose aftermath is the card (Stack's tower). `update` returns `Tick`, `NewBest` or `Landed` for sounds and haptics. A first-ever score shows as the best without the celebration. `input(e)` keeps the card's input rule: its buttons, and after `ResultsCard::RETRY_GUARD` (0.35 s, so a frantic tap during the failure doesn't count) a press anywhere else retries.

**Your own card.** `.state_only()` keeps everything but the drawing: `draw()` then draws only the new-best confetti, there's no retry button (any tap retries after the guard), and you draw the card from your world with its state: `age()`, `shown_score()` (the roll-up) and `score_scale()` (its punch), `best()`, `landed_at()`, `new_best_at()` / `new_best_shown()` (when to swap the best for a NEW BEST), `stamp()` (that ribbon's spring, 0 → 1) and `flash()`, `gap_to_best()` (the "3 MORE TO BEAT IT" dare, when it applies), `retry_ready()`. Buttons of your own go in with `.extra(button)` or by setting `card.retry`, at the rects you draw them; `input` still sorts their taps from whole-screen retries. For a title, `TitleCard::age()` times your own entrance; the rules are yours to keep (no input on it, any tap starts, and that tap is also the first move).

**`StatsCard`**: the card between rounds or fights. A title slams in ("FIGHT 2 CLEAR"), then the stat rows roll up one at a time on a panel (`.percent("ACCURACY", 0.87)`, `.int("PARTS BROKEN", 6)`, `.time("TIME", 42.3)`; `.bad()` colors the last one as a cost, like damage taken), a rank letter stamps on with a flash (`.rank("S")`; `StatsCard::rank_for(score)` maps 0–1 to S/A/B/C), the next opponent slides in (`.next("GRANNY", "never lost a duel")`), and after `.hold(secs)` (2.5 s; `.manual()` waits for a tap) a wipe covers the screen and opens on the next scene (`.wipe(Wipe::Iris | Wipe::Slide | Wipe::Shutter)`). It runs on `update(dt)`, which returns what happened this frame (`StatsEvent::Row(i)`, `Tick`, `Landed(i)`, `Rank(letter)`, `Teaser`, `Out`, `Covered`, `Done`) for sounds and haptics; on `Covered` switch to the next scene and keep drawing the card over it until `finished()`. `input(e)`: the first tap shows everything at once, the next starts the wipe (taps in the first 0.35 s are ignored). `draw_with(|rect, reveal| portrait.draw(rect, "smug", 0, sys::time()))` puts the opponent's portrait in the teaser's frame (`draw()` leaves it empty). The text stays inside `Layout::card` and below the pause pill; the scrim and the wipe cover the screen; colors come from the theme (the wipe in `outline` edged with `accent`), but text that wouldn't read on the card's panel over the scrim (a theme with dark ink on a dark panel: navy on navy) is drawn near-white or near-black instead, whichever reads (`ui::readable(color, bg, min_contrast)`, with `ui::contrast` and `ui::over`); `.ink(color)` sets the numbers' and names' color yourself. The next opponent's line shrinks to fit its column (down to 11 pt), then wraps, the panel grows to hold it, and "TAP TO CONTINUE" keeps below it. Keep it to six rows, and show stats the player can be proud of or wants to beat: accuracy, time, parts broken, the best combo.

**The card's sound.** `.sounds(CardSounds::load("assets/sfx"))` plays a set at the card's events: a short jingle as it comes in (`intro`), a swish as each row slides in (`row`), a blip per step of a rolling number (`tick`, at most every 45 ms, pitched up as the number rolls), a thunk as it lands (`land`), the rank's stamp (`rank`), a whoosh as the next opponent slides in (`teaser`) and as the wipe closes (`wipe`), and a sting into the next fight (`outro`). The skill's `scripts/card_sfx.py games/<name>/assets/sfx --key A` writes those eight files with sfx.py (free, deterministic, ~95 KB in all), the jingle and sting in the music's key; any missing file is left out, and `CardSounds::new()` with your own `Sound`s in its fields works too. Keep one set, call its `poll()` each frame and clone it into each card, so each card doesn't load its own. The gallery's STATS page plays them.

The music around the card, with a `music::Director` (the skill's audio.md "Music that follows the game"):
- **In:** as the fight ends and the card comes up, `director.stop(sys::time(), 0.6)`: the fight music dies away under the intro jingle. (Or keep it under the card at a lower level with `dialogue.ducker.set_base(0.5)`, if the dialogue owns the music bus.)
- **Out:** on `StatsEvent::Out` the card plays the wipe and the sting; on `StatsEvent::Covered` start the next fight's music, `director.restart(next_zone, sys::time())`, so it comes in as the wipe opens. If the music kept playing under the card, `director.go(next_zone, Change::Stinger, sys::time())` on `StatsEvent::Teaser` instead: the change lands on the next exit cue under the zone's stinger, about when the wipe goes (within two bars).

**`Hud`**: the score big at the top (rolling and punching), the best under it until it's beaten, then a gold NEW BEST marker; optional hearts (`lives`) top-right and a progress bar. It stays clear of the pause pill.

**`Countdown`**: "3, 2, 1, GO!" with a draining ring. Start it in `Game::resume` (a recorded lifecycle call) when your game needs a run-up after a pause, and skip the simulation while `active()`. Since everything else runs on `update(dt)`, the rest of the kit simply freezes while the platform has the game paused.

## Talking portraits and dialogue

`ui::dialogue` puts voiced lines on screen: a pixel-art face whose mouth moves with the voice, text that types out as the words are spoken, and music that dips under the voice. Two modes:

- **Cutscene** (`dialogue.say`): lines queue in a box at the bottom with the speaker's portrait and name plate. While `dialogue.active()`, hold the game and let the dialogue have the input.
- **Bark** (`dialogue.barks.trigger(tag)`, or `dialogue.bark(…)` for one line): a quick line in a small bubble near the top (`bark_y` moves it) that goes away by itself `bark_hold` (0.6 s) after the line ends. It takes no input and never holds the game.

**A line is never cut off by itself.** A cutscene line's down arrow (`Showing::complete`) comes up once its text is out *and* its voice has finished. How it moves on is `dialogue.advance`:
- `Advance::Tap` (the default): it waits for a tap. A tap while the text types shows the rest at once; a tap once the text is out but the voice is still going moves on the moment the voice ends; another tap then cuts it short (the player asked). Taps in the first 0.2 s after the box opens don't count, so the tap that started the scene can't skip it.
- `Advance::Auto { pause }` (`.auto_advance(0.4)`): each line moves on by itself `pause` seconds after its voice ends (a line without sound runs on its subtitle's timing). Taps still hurry it as above. A scene lasts as long as its lines, so don't end it on a timer of your own: wait for `DialogueEvent::Finished`. `dialogue.remaining()` says how long it has left if nobody taps (each line's voice, 0.1 s lead, and the pause).

`dialogue.skip()` moves past the line showing and `skip_all()` clears everything: the only ways a voice is stopped early. A cutscene queued while a bark is being spoken waits for the bark to finish.

**Barks: sparse, reactive, never the same line twice running.** In a fight, characters who talk all the time get annoying fast; ones who react to what just happened, briefly, feel alive. `dialogue.barks` (`Barks`) holds the lines in pools by situation, a free-form tag (`taunt`, `player_hit`, `player_miss`, `part_broken`, `low_health`, `near_win`, `defeat`, `gloat`…), and picks one when the game reports an event:
- **Event-driven.** `barks.trigger("part_broken")` wants a bark for `barks.window` (1 s); if none may start by then, it's dropped: a reaction that comes late is worse than none. Triggering a tag already waiting renews it. Idle chatter is off; `barks.idle("idle", 20.0)` turns it on (a line after that many seconds without a bark, never under 10 s).
- **One voice at a time.** A bark never starts while a bark or a cutscene line is being spoken, and two barks start at least `dialogue.bark_cooldown` (5 s) apart. `dialogue.bark(…)` follows the same rules and returns false when it can't start.
- **Shuffle bags.** Each pool deals all its lines once, in an order from the session seed (so replays hear the same lines), before any comes round again, and a line isn't heard again within the last `barks.recent` (3) barks across every pool.
- **Cooldowns and budgets.** A pool rests `barks.cooldown` (8 s) after it speaks (`barks.set_cooldown("player_hit", 12.0)` per pool), and one speaker says at most `barks.per_minute` (4) barks in any 60 s of game time.
- **Priority.** `barks.trigger_priority("defeat")` is never dropped: it waits until a bark may start (after a cutscene, the gap and its pool's cooldown), goes before ordinary barks and isn't held back by the speaker's budget. `barks.clear()` forgets everything waiting.

Keep barks short (≤ 1.5 s) and give each pool 3–6 lines. `barks.add(tag, speaker, &line, expression)` adds a line; `barks.load_script(include_str!("../art/lines.csv"))` adds every line of the `mb voice script` file, each from `assets/vo/<character>/<id>`, wearing its `emotion` as the expression, so cast portraits under the script's character names. The script takes an optional `tag` column (`character,line,text,emotion,tag`, or `tag = "…"` in a TOML `[[line]]`; `mb voice` ignores it); without one, a line's pool is its id without the trailing number (`taunt3` → `taunt`, `part_broken_2` → `part_broken`). `ui::parse_script(text)` reads one without loading anything.

Subtitles always show, whatever the sound setting: many players have it off. A line whose audio is missing or still decoding runs on its text alone; one whose files haven't loaded waits up to a second for them, then goes without.

**Voice lines.** `mb voice say` writes `assets/vo/<character>/<line-id>.ogg` and `<line-id>.lips.txt`; `VoiceLine::load("assets/vo/rocko/taunt1")` loads both. Load lines in `init`. Clones share the loading files, so keep one per line and pass it as often as you like. `VoiceLine::subtitle("…")` is a line with no recording (30 characters a second, the mouth flapping with the letters); `VoiceLine::new(sound, lips)` builds one from parts. **Missing files:** a line whose `.ogg` doesn't load shows its subtitle without a voice, and `VoiceLine::load(base).text_if_missing("…")` gives it the script's text (audio tags like `[laughs]` left out, `subtitle_text`) for when its timing doesn't load either: the text types out at 30 characters a second with the mouth flapping, instead of an empty box. Each case logs a warning naming the file. `Barks::load_script` does this for every line of the script. `line.play(at)` schedules it on its own (`Sound::play_at` on game time, on `Bus::Sfx` like every voice) and returns its `Playing` timing.

The `.lips.txt` file is UTF-8 text, a key and its value per line:
```text
mblips 1
hz 30
dur 1.84
mouth 0012332100123321000
text Eat my dust, grandma!
reveal 0 40 80 120 ...
```
`mouth` is one digit per frame at `hz` frames a second from the start: 0 closed, 1 slightly open, 2 open, 3 wide (vowel peaks). `dur` is the audio's length in seconds, `text` the subtitle (the rest of the line), `reveal` the millisecond each character of `text` is spoken. Unknown lines are skipped, so newer files still load. Without `reveal` the text types out evenly over `dur`; without `mouth` the mouth flaps with the letters. `Lips::parse(bytes)` reads one; `mouth_at(t)` and `chars_at(t)` give the mouth and the characters shown `t` seconds into the line.

**In the bundle, one file per character.** Every file is one of a bundle's 512 entries, so `mb build` packs a directory's `.lips.txt` files into one lips pack, `assets/vo/<character>/lips.txt`: a first line `mblipspack 1`, then each line's file after an `@ <line-id>` line. `VoiceLine::load` reads the pack once for all of a character's lines and falls back to `<base>.lips.txt` when the pack hasn't got the line (a game run from its folder with `mb dev`, or a bundle packed by an older `mb`). `parse_lips_pack(bytes)` reads one. Keep the per-line files in `assets/`; only the bundle changes.

**Portrait sheets.** A PNG of equal cells, 4 columns wide. Each row is one expression; its columns are the mouth closed, slightly open, open and wide (mouth 0–3). Row 0 is the neutral face. The last row, the blink row, is the neutral face with its eyes shut and the same four mouths. `mb art portrait` writes exactly this sheet (docs/GENERATE.md "Talking portraits"):

|  | mouth 0 | mouth 1 | mouth 2 | mouth 3 | |
|---|---|---|---|---|---|
| row 0 | neutral | neutral | neutral | neutral | always |
| row 1 | smug | smug | smug | smug | `.expression("smug", 1)` |
| row 2 | angry | angry | angry | angry | `.expression("angry", 2)` |
| row 3 | blink | blink | blink | blink | `.blinks(3)` |

`Portrait::load(path, cell_w, cell_h)` (cells in sheet pixels), then `.expression(name, row)`, `.blinks(row)` and `.on_right()` (it sits on the right of the box, mirrored to face in). Unknown expression names draw neutral. It blinks every 2.5–4.5 s for 0.12 s at times fixed by game time (`ui::dialogue::blinking(t, seed)`) when it wears the neutral face, talking or not (other expressions hold their eyes), and lifts one art pixel on open vowels. It draws as large as fits, at a whole number of half units (device pixels on a 2× screen) per *art* pixel, at whole-unit positions, so pixels stay square. `.art_pixels(n)` says the cell is `n` art pixels across (the `--grid` it was made with; the sidecar's `recipe.grid`, and the `rust` line `mb art portrait` prints). Without it, a cell that's a whole multiple of 48 on both sides is taken as 48 art pixels (what `mb art portrait` makes: a 192 cell is 4 sheet pixels an art pixel, so in a 132-unit box it draws 120 units, 2.5 an art pixel), and anything else as one sheet pixel an art pixel. Images are filtered, so store the sheet at 2× or more for the crispest edges (SPEC §5.3). `portrait.draw(rect, expression, mouth, t)` draws one anywhere and returns where the face is; `portrait.scale_in(rect)` is the units per sheet pixel it would use; `portrait.cell(expression, mouth, t)` says which cell it would use.

**Damage and hit reactions.** A fight reads on the faces when the portraits take the beating too. `portrait.set_health(hp / max_hp)` picks a damage stage (`ui::damage::stage`), and each stage adds to the last: at 0.75 and below, sweat runs down the brow; at 0.5, soot smudges, a crack across the glass, scanlines, static bursts now and then and a red cockpit light that flickers out; at 0.25, more cracks, sparks spitting from the frame's edge, a red alarm border pulsing at 2 Hz and a blinking WARNING tag. `portrait.hit(strength)` (0–1, at game time) shakes the face, flashes it white then red, knocks it aside (away from the middle) and springs it back over about half a second, tears the picture with static and shows the pained face for 0.25–0.6 s. Everything is procedural, so it works on any sheet: it's drawn on a grid of about 40 square "fx pixels" across the portrait, in whole half units, so it matches pixel art, and each frame is a closed-form function of game time and the portrait's seed (it replays, at any frame rate). Clones share the damage: hit the game's copy and the dialogue's portrait (box and barks) shows it. It works for the player's own face too: a portrait in a HUD corner (`me.draw(rect, "", 0, sys::time())`) with `me.set_health(..)` and `me.hit(..)` when the player is hit. A hit knocks the face up to about an eighth of its width past where it stands; **`.clipped()`** keeps it in the rect it's drawn in (mb2d has no scissor, so the SDK cuts the sprite and the overlays to it): the face slides behind the frame's edge and the alarm border is the rect, which holds still while the face shakes. Clip a portrait that fills a framed box (a comm screen, a HUD corner); without it, leave the knock room around it. `portrait.reaction(t)` gives the offsets, flashes and whether the pained face shows, to draw your own way; `.no_damage_overlays()` keeps the hits but drops the overlays.

Two paths for the faces themselves:
- **Procedural only** (any sheet): the overlays and hit reactions above, on the faces the sheet has.
- **Damage rows** (`mb art portrait … --expressions smug,angry,hurt,battered`; the prompt spells out "hurt" as wincing and "battered" as the same bust bruised, sooty and scratched with a cracked visor): name them with `.expression("hurt", 3).expression("battered", 4)`, and the portrait uses "hurt" for a hit and in place of the neutral face from health 0.5 down, and "battered" from 0.25 down (lines spoken with a named expression keep it, except for the flinch of a hit). `.damage_faces(hurt, battered)` renames the rows it looks for. The overlays draw on top either way.

The gallery's DAMAGE page (`--param page=12`) beats an opponent from full health to near death a hit a second, with the player's own portrait taking hits in the corner, both clipped to their frames; tap to land a hard one.

**Ducking.** While a voice is speaking, the dialogue's `Ducker` dips `Bus::Music` by `ducker.db` (5 dB by default; 3–6 keeps the music present and the words clear) over 0.12 s and brings it back over 0.6 s. Once it has ducked it owns the music bus's volume: set your own music level with `dialogue.ducker.set_base(gain)` rather than `Bus::Music.volume`, and it multiplies the two. `.duck(0.0)` turns it off. Text-only lines don't duck.

```rust
// init
let rocko = Portrait::load("assets/portraits/rocko.png", 32, 32).expression("smug", 1).blinks(2);
let taunt = VoiceLine::load("assets/vo/rocko/taunt1");
let ouch = VoiceLine::load("assets/vo/rocko/ouch");
let mut talk = Dialogue::new(theme).cast("rocko", rocko).duck(5.0).auto_advance(0.4);
// art/lines.csv:  character,line,text,emotion,tag
//                 rocko,ouch1,Not the paint job!,angry,part_broken
//                 rocko,ko1,"No, no, NO!",angry,defeat
talk.barks.load_script(include_str!("../art/lines.csv")).unwrap();
talk.barks.set_cooldown("part_broken", 12.0);
// a cutscene: each line plays to the end, then the next 0.4 s later
talk.say("rocko", &taunt, "smug");
talk.say("rocko", &VoiceLine::subtitle("Catch me if you can."), "");
// during play: report what happened; a bark comes if one may
if arm_lost { talk.barks.trigger("part_broken"); }
if hp <= 0 { talk.barks.trigger_priority("defeat"); }
// or one line by hand (false if it can't start now)
talk.bark("rocko", &ouch, "");
// update
for e in &events { if talk.input(e) { continue } /* the game's own input */ }
if talk.update(dt) == Some(DialogueEvent::Finished) { /* back to play */ }
if !talk.active() { /* step the game */ }
// render, over the scene
talk.draw(&layout);
```
`talk.current()` and `talk.current_bark()` return what's showing (speaker, expression, text, characters revealed, mouth, whether it's all out), to draw it your own way. `.draw_line(|l| ..)` swaps the cutscene box for the game's own: each frame the line is up it gets a `LineBox` with `rect` (where the default box is, sliding up as it opens), `line` (the `Showing`), `portrait` (the speaker's, if cast), `theme` and `t`; `l.shown()` is the text typed out so far and `l.draw_text(x, y, w, size)` types it out in the theme's body font, wrapped as the whole line will be. Barks keep their bubble.

## Cutscenes

`ui::Sequence` plays an intro or an ending as a timeline of steps, on game time, the same at any frame rate. It builds on `Dialogue` (its lines are a `Dialogue` with auto-advance, `seq.talk`) and adds the camera, bars, fades, titles and credits:

```rust
use maimbrain::ui::{SeqEvent, Sequence, Shot};
let wide = Shot::new(vec3(0.0, 2.4, 9.5), vec3(0.0, 1.0, 0.0)).fov(0.9);
let on_varn = Shot::new(vec3(4.2, 1.9, 5.2), vec3(1.2, 0.8, 0.4)).fov(0.75);
self.ending = Sequence::new(theme).cast("VARN", varn_portrait).cast("KIT", kit_portrait)
    .letterbox(0.22)                                   // bars slide in; doesn't hold the timeline
    .shot(wide, 0.0)                                   // cut (and hold 0 s)
    .fade_in(1.2).meanwhile()                          // `meanwhile`: the next step starts with this one
    .move_to(on_varn, 3.0, Ease::SineInOut).meanwhile()
    .line("VARN", &VoiceLine::load("assets/vo/varn/last"), "battered")   // waits for the voice
    .cue("fall")                                       // SeqEvent::Cue("fall"): the game's part
    .wait(1.2)
    .title("ONE YEAR LATER", "the arena, rebuilt", 3.0)
    .sound(sting, 1.0)
    .credits(&[("THE CHAMPION", "VARN"), ("THE CHALLENGER", "KIT")], 6.0)
    .fade_out(1.2);
// update: for e in &events { if self.ending.input(e) { continue } }
//         for e in self.ending.update(dt) { if e == SeqEvent::Cue("fall".into()) { knock_over() } }
//         if self.ending.finished() { /* the results card */ }
// render: gfx3d::camera(&self.ending.camera().unwrap()); /* the scene */ self.ending.draw(&layout);
```

**Its own panel.** The lines' box is the theme's panel unless the game draws it: `.draw_line(|l| ..)` (as on `Dialogue`) gets each frame's `LineBox`, with `l.rect` above the letterbox bar. `seq.talk.theme = other` gives the box another theme's panel and fonts instead.

```rust
let intro = Sequence::new(theme).cast("KIT", kit_portrait)
    .draw_line(|l| {
        let r = l.rect;
        gfx2d::rect(r.x, r.y, r.w, r.h, 0x101820f0);          // the game's comm panel
        gfx2d::rect(r.x, r.y, r.w, 2.0, 0x40ff80ff);
        let mut x = r.x + 12.0;
        if let Some(p) = l.portrait {
            p.draw(Rect::new(x, r.y + 12.0, 96.0, r.h - 24.0), &l.line.expression, l.line.mouth, l.t);
            x += 108.0;
        }
        text(&l.line.speaker).font(Font::Pixel).size(16.0).color(0x40ff80ff).draw(x, r.y + 12.0);
        l.draw_text(x, r.y + 36.0, r.right() - 12.0 - x, 16.0);  // typed out as it's spoken
    })
    .line("KIT", &VoiceLine::load("assets/vo/kit/go"), "");
```

Steps: `shot(shot, hold)` cuts, `move_to(shot, secs, ease)` moves from wherever the camera is (position, target and field of view eased straight across, so it keeps the target in view), `line(speaker, &line, expression)` says a line in the dialogue box with its portrait and holds until the voice and the 0.5 s pause are over, `wait`, `fade_in` / `fade_out` (to `seq.fade_color`, black), `letterbox(amount)` (the fraction of the screen the two bars cover; 0 takes them away), `title(text, sub, secs)`, `credits(&[(role, name)], secs)` (rolling up), `sound(sound, vol)` and `cue(name)`. Each step starts at the exact moment the one before ends, not at the next frame, so the timeline is a function of game time; `.meanwhile()` lets the next step start with the one before it. `camera()` (or `shot_now()`) is the camera now, `bars()` and `fade()` the overlays' state, `age()` the sequence's clock.

The player stays in charge: taps in the first 0.3 s don't count; after that a tap hurries a line (as in `Dialogue`) or cuts a move, wait or title short (the clock jumps to its end, so everything lands where it would have), and holding a finger down `HOLD_TO_SKIP` (0.7 s) skips the rest: the camera, bars and fade go where the steps would have left them, the cues not reached yet come in that update (so the game's state matches), then `SeqEvent::Skipped` and `Finished`. A ring and "HOLD TO SKIP" show top right for the first seconds and while a finger is down. `input` takes every event while it runs. The gallery's ENDING page (`--param page=13`) is a 20 s epilogue in the beats of the skill's "Writing an ending", over a scene drawn through `Camera::project`.

## Recipes

**A title screen in 10 lines**
```rust
// fields:  layout: Layout, title: TitleCard
let layout = Layout::new();
let theme = Theme::from_identity("seed packet, 1950s, mustard and teal").load_fonts();
let title = TitleCard::new("BUBBLE", theme).tagline("don't let it pop").best(best as i64);
// update:
self.title.update(dt);
// render, after drawing your live scene:
self.title.draw(&self.layout);
// any tap starts the round (the card never asks for one)
```

**A results card**
```rust
// on game over, after store::submit_score and before sys::round(Round::Over):
let prev = self.best;                                         // the best before this round
self.results = Some(ResultsCard::new(score, prev, theme, &self.layout, self.rng.next_u32() as u64)
    .heading("SPLAT!").label("POINTS")
    .extra(Button::new(Rect::default(), "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY)));
// update: feed events, then advance
if let Some(card) = &mut self.results {
    for e in &events {
        match card.input(e) {                                     // buttons, and any other tap after the guard
            Some(ResultsAction::Retry) => restart = true,
            Some(ResultsAction::Extra) => store::show_scores(BOARD),
            None => {}
        }
    }
    if card.update(dt) == Some(ResultsEvent::NewBest) { sfx.play(FANFARE) }
}
// render:
if let Some(card) = &self.results { card.draw() }
```

**A card drawn from your world** (the score chalked on a wall, the best on a flag)
```rust
self.results = Some(ResultsCard::new(score, prev, theme, &self.layout, seed).state_only());
// update: the same input and update calls as above
// render:
if let Some(card) = &self.results {
    let k = card.score_scale();
    draw_chalk_number(card.shown_score(), 180.0, 260.0, 64.0 * k);
    if card.new_best_shown() {
        draw_flag("NEW BEST", card.stamp());                     // springs in when the roll-up lands
    } else if card.finished() {
        draw_flag(&format!("BEST {}", card.best()), 1.0);
        if let Some(gap) = card.gap_to_best() { draw_dare(gap) }
    }
    card.draw();                                                  // the confetti, on a new best
}
```

**A combo meter**
```rust
// fields: combo: Combo, popups: Popups, fx: Particles
self.combo = Combo::new(1.5);                    // 1.5 s to keep it going
// on each success:
let n = self.combo.hit();
self.popups.score(x, y, 10 * self.combo.multiplier() as i64, theme.gold);
if n % 10 == 0 { self.fx.stars(x, y, 10, theme.gold) }
// on a miss:
self.combo.break_combo();
// update (returns Some(count) when a combo times out):
if let Some(lost) = self.combo.update(dt) { if lost >= 5 { sfx.play(AWW) } }
// render:
self.combo.draw(300.0, layout.hud.y + 90.0, &theme);
```

**Juice on a hit**: `shake.add(0.3); punch.kick(0.25); stop.freeze(0.05); fx.burst(x, y, 16, color); popups.score(x, y, 50, theme.gold);` and play the hit sound pitched up with the combo.

The 2D and 3D templates (`mb new`, `mb new --3d`) start with the title card, HUD and results card wired up, and `games/stack` uses all three plus popups, sparkles and confetti.
