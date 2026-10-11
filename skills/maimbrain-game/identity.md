# Art direction: give the game its own identity

Maimbrain games tend to come out looking like one game. `docs/IDENTITY.md` measured the feed: 19 of 27 titles were set in the 5×7 pixel font with a hard drop shadow, and the same layouts kept coming back. Titles sat centred in the top fifth with an all-caps "DON'T…" tagline. The score was big and white at top centre. Game-over cards were "SPLAT!", a huge number, then "NEW BEST!" on a dark box. Backgrounds were purple gradients. That happens when every game starts from the same defaults and the same example lines. A player swiping past reads it as more of the same, before they have seen what the game does.

So before building, pick an identity and write it down. It takes ten minutes and it decides the fonts, colors, shapes and motion of everything the kit draws.

## 1. Write the identity into DESIGN.md

Add an `## Identity` section with these seven lines, each specific to this game:

- **Reference**: an era, medium or object the look borrows from. Examples: "1950s seed packet", "airport departure board", "risograph zine", "Saturday-morning cartoon", "brass orrery", "VHS rental shop", "ballpoint doodles in a maths notebook". Pick something the game's subject suggests. The art itself is crisp pixel art by default (assets.md §1), so the reference shapes the palette, subjects and chrome around it: a diner game is pixel art with a diner's colors and a receipt for a results card, not a flat-vector cartoon. Use another medium (painterly, ink, paper, clay) when the subject really calls for it, never a generic flat-vector cartoon unless asked; if the user has said what they want (or what they don't), that decides it.
- **Palette**: 3–5 hex colors, with which one is the background, the ink (text), the accent and the highlight. Avoid purple-to-plum gradients unless the reference really calls for them.
- **Fonts**: a pairing from the library (`mb fonts`, `mb fonts --preview` draws them all): a display face for the title and headings, and a body or number face. Check each one's `pairs_with` and `caps_only`. The built-in Inter and the 5×7 pixel font are fallbacks, not choices. If the game is pixel art, use one of the ten library pixel fonts (Press Start 2P, Silkscreen, Pixelify Sans, Jersey 10, Tiny5, Micro 5, VT323, Workbench, Sixtyfour, Jacquarda Bastarda 9), whichever fits; don't default to the 5×7.
- **Shape language**: sharp, rounded, pill, chamfered, hand-drawn (wobbly), pixel-stepped, bevelled or ticket. Plus borders (none, line, double, dashed) and shadows (soft, hard offset, glow, none).
- **Motion personality**: snappy, floaty, bouncy, mechanical, jittery or elastic. It sets how the kit's entrances, idles and presses move, and how your own animation should move too.
- **Layouts**: the title layout (drop, stack, banner, stamp, typewriter, marquee, arc, neon, plate), the results layout (panel, floating, receipt, scoreboard, stamp, editorial) and where the HUD sits (center, corner, badge). Better still, a card that comes from the game's world: a receipt, a scoreboard, a ticket, a rosette, a wanted poster.
- **One signature idea**: the single visual thing someone would remember. Examples: "the score is chalked on the bar's blackboard", "every UI panel is a torn piece of masking tape", "the title is a neon sign that buzzes and flickers off when you lose".

Then write the game's own taglines and headlines in its voice. The skill's examples ("HOW HIGH?", "DON'T LET BRIAN GO THIRSTY.", "SPLAT!") show the shape of a good line. Don't reuse them.

## 2. Make it real in code

```rust
// The closest kit identity, adjusted by your words (docs/UI.md lists all 17):
const IDENTITY: &str = "1970s diner menu, mustard and teal, snappy, receipt";
let mut theme = Theme::from_identity(IDENTITY);
// Your palette and fonts win over the preset's:
theme.bg_top = hex(0xf3e9d2);
theme.accent = hex(0xd9a21b);
theme.title_font = Font::asset("assets/fonts/shrikhand.mbf", Font::SansBold);
theme.body_font = Font::asset("assets/fonts/courier-prime.mbf", Font::Sans);
theme.number_font = theme.title_font;
theme.style.results = ResultsLayout::Receipt;
```

```sh
mb font add games/<name> shrikhand              # → assets/fonts/shrikhand.mbf (+ its OFL license)
mb font add games/<name> courier-prime
mb font add games/<name> --identity riso        # or every font a kit identity was designed with
```

- `Theme::preset("riso")` and the other 16 presets are complete starting points. `Theme::from_identity(words)` picks the closest one and applies the colors, shapes, motion, textures and layouts you name. `.load_fonts()` points the theme at the preset's own fonts, which `mb font add --identity <name>` bakes.
- Manifest: `stdlib = { mb2d = 2 }` for game fonts. `Font::asset` draws in its fallback until the font loads, so naming it in `init` is fine. Set 3D text (`Node::set_text`) only once `font.ready()`, because it's laid out when it's set.
- Fonts are subset atlases: Latin-1 by default ("Touché", "Señor", curly quotes and dashes), `--chars caps` for caps-only display faces (the default when the library marks the font `caps_only`), `--chars digits` for a score-only face, `--chars ascii` to save about half when the text is plain English. A Latin-1 font is typically 60–110 KB (35–75 KB in the bundle, which is compressed). Keep a game to 2–3 fonts.
- Draw the scene in the identity too, not just the kit's chrome. Use the palette for your sprites and shapes. Paint `theme.backdrop(screen, t)` or your own background, and give your animation the motion personality you chose.
- `mb build` warns when a game uses only the built-in fonts or a stock `Theme::candy()`-style preset as-is.

## 3. Check it against the house look

Before you call the game done, put its title, play and game-over screenshots next to `docs/identity/before-live.jpg` (the measured feed; `docs/identity/identities.jpg` shows the kit identities) and answer docs/IDENTITY.md's checklist. Any "yes" is a place to change something. The most common ones:

1. Is the title in the pixel font with a hard drop shadow, or in outlined Inter Bold dropping in letter by letter?
2. Is it centred in the top fifth with an all-caps "DON'T…" / "HOW…?" tagline?
3. Is the background a purple, plum or dusk vertical gradient?
4. Is the score a big white number at top centre?
5. Is the game-over card headline, then big number, then "NEW BEST!" on a dark translucent box?
6. Is the theme a stock kit preset used as-is, or did you copy another game's `fx.rs` / `text_c`?
7. Could this game's title, HUD and game-over card move onto another game's art without looking out of place?

If question 7 is a yes, the chrome says nothing about this game yet. Go back to the signature idea.
