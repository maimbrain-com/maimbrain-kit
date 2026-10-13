# Landscape, adaptive and controller games

Read this when a game is held sideways, turns with the phone, fills the screen's own shape, or is played with a controller or keyboard. SPEC §2 ("Orientation and fit", "Inputs") and §5.2 have the rules.

## Pick the shape

| You want | Manifest | What the game sees |
| --- | --- | --- |
| The usual feed game | `orientation = "portrait"`, `logical_size = [360, 640]` | That size, letterboxed, the same all session |
| A wide playfield (side-scroller, racer, fighter) | `orientation = "landscape"`, `logical_size = [640, 360]` | `[640, 360]` in play. As a feed card in the portrait feed, `[360, 640]`: draw the title and game-over cards for it. Tapping in turns the app landscape (a `Resize`) and waits for the player to be ready |
| Either way up | `orientation = "any"`, `logical_size = [360, 640]` | `[360, 640]` upright, `[640, 360]` sideways, switching mid-round with a `Resize` |
| No bars on any screen | add `fit = "fill"` | At least `logical_size`, grown to the screen's shape (360×780 on a modern iPhone), changing with it |

Letterbox keeps every player's playfield the same, which leaderboards like. Fill trades that for a screen with no bars: a game that shows more of its world on a longer screen gives those players a little more warning, so keep what matters (spawns, threats) inside the declared size and use the extra room for scenery, HUD and breathing space.

## Handle a resize

Any game that isn't portrait-letterboxed can be told its screen changed:

```rust
for e in input::poll() {
    if e.is_resize() {
        self.screen = sys::screen();          // the new width, height and insets
        self.layout = Layout::for_screen(&self.screen);
        continue;
    }
    // ...
}
```

- Lay out from `sys::screen()` every time, never from constants: HUD corners, touch-control positions, the camera's view width.
- `screen.is_landscape()` says which way it's held.
- A `Resize` comes first in its frame, before that frame's touches. Its coordinates are already the new screen's.
- An `any` game keeps playing through a turn, so don't reset the round. Re-anchor the camera so the player stays where they were in the world.
- The insets move with the phone: the notch is at the side when it's sideways. Keep buttons inside them.
- Test both ways: in the preview, resize the browser window (a wide window is landscape). `mb shot` emulates the declared `logical_size`; `mb shot --landscape` shoots an `any` game sideways, and `mb shot --portrait` a landscape game's vertical feed card.

## Controllers

Declare `inputs = ["touch", "gamepad"]` (add `"keyboard"` for desktop players). A controller-only game also declares `requires = ["gamepad"]`; the feed then shows it only while one is connected. Touch-first games should still say `gamepad` when they play well with one: with a controller connected, the feed puts them ahead of games that don't.

Buttons use the W3C standard mapping; `input::pad` names them:

```rust
match e.kind() {
    Kind::PadAxis if e.code == pad::LX => self.stick.x = e.a,   // −1 left … 1 right, 0.1 dead zone applied
    Kind::PadAxis if e.code == pad::LY => self.stick.y = e.a,   // −1 up
    _ => {}
}
if let Some(b) = e.pad_pressed() {
    self.using_pad = true;                    // hide touch controls, show pad prompts
    self.pad_style = e.pad_style();
    match b { pad::A => self.jump(), pad::X => self.attack(), pad::B | pad::RB => self.dash(), _ => {} }
}
```

- **Prompts** name the player's own buttons: `self.pad_style.label(pad::A)` is "A", "✕" or "B" (Switch). Never hard-code "A". Library fonts have no ✕ ○ □ △: draw prompts with `ui::pad_button(style, pad::A, x, y, size)` (the controller's own mark and colors, the PlayStation shapes drawn), and in a sentence use `style.word(b)` ("CROSS", "A"). Words beside a badge go at `x + ui::pad_button_width(style, b, size) / 2 + gap`, vertically centred on `y` (`.middle()`), not from the text's top: a shoulder button's "RB" pill is wider than `size`.
- **Teach the shoulders** for an action they also do (R1/RB to dash, not ○/B): every controller has them, and they're where a player's fingers rest.
- **Start/Menu is the platform's pause**, and games never receive it. Pause your game from `suspend()` as usual.
- **The round starts on A.** `Rounds` takes A, Enter and Space as the tap that starts a round and retries from the game-over card (`Event::is_confirm()`), so a controller player never touches the screen. A hand-rolled title should do the same.
- **D-pad and stick both move.** Read the d-pad buttons (`pad::UP` … `pad::RIGHT`) and the left stick, whichever moved last.
- **Tight controls:** act on the press event, not on a polled state at the frame's end, and use `e.time` for buffers and windows (coyote time, jump buffering, parries).
- **Haptics** rumble the controller while one is in use. Nothing to do.
- **Keyboard:** `input::key` has the common codes (arrows, WASD, J/K/L, Z/X, Space, Enter). Accept both arrows and WASD.

## Touch controls for a controller-style game

When a game has a stick and buttons, touch needs them on screen:

- **A floating stick:** the thumb comes down anywhere on the left half, and that's the stick's center. Drag for direction; the center trails the thumb past the stick's radius, so reversing is quick.
- **2–3 buttons on the right,** big (at least 64 pt) and in the thumb's arc, with the main action largest and nearest the thumb.
- **Hide them** while a controller is in use (after any pad event), and bring them back on the next touch.
- In landscape, both thumbs rest at the sides: keep the HUD at the top center and out of the bottom corners.

## Feed cards

Every card is upright and full screen. A landscape game gets a portrait Screen while it's a card, so its title and game-over screens are a vertical card: lay them out from `sys::screen()` (key art filling the height, the title large, the best and a prompt), and the playfield for landscape; a `Resize` comes when the player taps in and the app turns. `Rounds`' cards follow the Screen by themselves. An `any` game's card is upright too.

On every `Resize`, rebuild the layout (`Layout::new()`) and keep a game-over card's text and buttons inside its `results`, so the challenge strip's band stays clear in both shapes. With a controller docked, the app stays landscape after a round, so the game-over card can be seen sideways too. There, `results` is only about 186 units tall: put the score and buttons side by side rather than stacked. `ResultsCard` shrinks itself to fit.
