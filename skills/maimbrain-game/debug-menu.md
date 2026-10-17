# The debug menu

A build that isn't live yet (a draft, or one waiting for review) can be played in **debug mode**: the creator always, a moderator reviewing it, and testers the creator allowed. So can a live game, by its creator only: someone tells them about a bug on level 3, they jump there on their phone, see it, and tap *Write your agent a note*, which files a to-do with the run at that moment (it reaches you through `mb feedback`, with a `see it` command). A Debug button on the game's card opens a menu the platform draws from items your game declares. It's how the owner, testers and reviewers reach the boss on floor 4 or the event that happens one run in ten without playing 12 minutes to get there. Nothing played in debug mode counts (no score, no board, no verified progress), and it has saved data of its own, so the game needs no special care for either. `mb serve` always has it (the DEBUG button in the corner, or the backtick key).

## When a game needs one

**Required** for a game with any of these:
- distinct levels, floors, stages, worlds or zones;
- bosses or set pieces;
- story beats, cutscenes or endings;
- unlocks or saved progress (`progress`, meta upgrades);
- rare events: anything a player meets in fewer than about one run in five (a golden enemy, a storm, a secret room);
- runs longer than about 3 minutes.

Optional otherwise: a 40-second arcade game whose every part shows up in a normal round doesn't need one. `mb build` warns when a game that looks long or varied declares none.

## What goes in it

Group items into a few sections (each is a tab). A good menu lets someone reach **every** part of the game in a couple of taps:
- **Jump:** the start of each level, floor or stage; each boss at its door; each cutscene, ending and epilogue. A jump puts the game in a consistent state for that point: the save holds what a player would have by then (bosses beaten before it, the gear they'd have), then it starts there the way a normal start does.
- **State:** what changes how the game plays: lives, health, money, the weapon or gear, unlocks, counters the story reads. Use `number` and `choice` rows that show the current value.
- **Cheats:** invincible, one-hit kills, infinite ammo or time, show hitboxes, skip dialog.
- **Trigger:** each rare event, each story beat, a death or a win, now.

Don't add game speed, frame stepping, restart or resetting the save: the platform adds those to every menu.

## The code

```rust
use maimbrain::debug::{self, Menu, Pick};

// Ids: your own constants, 1 ..= 2^31, unique.
const JUMP_FLOOR: u32 = 100;  // + floor number
const BOSS: u32 = 200;        // + boss number
const GOLD: u32 = 1;
const GOD: u32 = 2;
const STORM: u32 = 3;

fn menu(&self) -> Menu {
    let mut m = Menu::new();
    m.section("Jump");
    for f in 1..=4 {
        m.action(JUMP_FLOOR + f, &format!("Floor {f} · {}", FLOOR_NAMES[f as usize]));
    }
    m.action(BOSS + 1, "Boss · Snapjaw").hint("at his door, floors before him cleared");
    m.section("State");
    m.number(GOLD, "Gold", self.gold as f32).range(0.0, 9999.0).step(10.0);
    m.section("Cheats");
    m.toggle(GOD, "Invincible", self.god);
    m.section("Trigger");
    m.action(STORM, "Storm now").stay();
    m
}

// In update, with the rest of the input:
for e in input::poll() {
    if let Some(p) = Pick::from(&e) {
        match p.id {
            GOLD => self.gold = p.value as u32,
            GOD => self.god = p.on(),
            STORM => self.sim.start_storm(),
            id if (JUMP_FLOOR..JUMP_FLOOR + 10).contains(&id) => self.jump_to_floor(id - JUMP_FLOOR),
            _ => {}
        }
        continue;
    }
    // … the game's own input
}
debug::show(&self.menu()); // every frame is fine: it's only sent when it changed
```

- Rows: `action` (closes the menu and goes back to play; `.stay()` keeps it open, `.confirm()` asks first), `toggle(id, label, on)`, `number(id, label, value).range(min, max).step(s)`, `choice(id, label, index, &options)`; `.hint(..)` adds a line under any of them. Labels up to 48 characters; at most 12 sections and 200 rows.
- A value pick is the new state ("gold is now 150"), not a nudge. Apply it, and `show` the menu again so it reads back.
- **Never branch on being in debug mode.** The game can't tell, and must play the same either way: outside debug mode nothing is shown and no pick ever comes. Cheat flags only change when a pick sets them.
- A pick is an input event, recorded in the replay like a touch, so a run that used the menu replays exactly (`mb.verify()` still matches) and the creator can watch it, with each pick marked on the timeline.
- A jump mid-round should carry on the round rather than end it; from the title it starts one, as a tap would.
- Keep jumps to the same code paths as normal play: set the save and the parameters, then start the scene the way a session does. A jump that builds a scene its own way tests the jump, not the game.

## Test it

Every item gets a test that it does what it says, with the test host:

```rust
let host = Host::start(7);
let mut g: MyGame = host.init();
let menu = host.debug_menu().expect("a debug menu");
let floor3 = menu.find_label("Floor 3 · Glass Halls").expect("a jump to floor 3").id;
host.debug_pick(floor3, 0.0);
host.frame(&mut g, 1.0 / 60.0);
assert_eq!(g.sim.floor, 3);
```

Loop over every jump and check where it landed (mode, level, boss), and that a round played after a jump still ends, reports and replays as normal. In the preview, `mb.debug.pick(id, value)` makes a pick from the console.
