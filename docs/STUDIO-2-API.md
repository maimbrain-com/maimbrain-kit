# Studio engine 2.0: the reference

This is what Studio engine 2.0 has, page by page, as the engine in
`games/studio-engine2` implements it. Each `##` is one page: the connector's
`reference` tool returns one by its topic (in brackets). The design
is docs/STUDIO-2.md; where they differ, this page is what runs.

Three things are kept in step by a test (`api_reference.rs` in the engine):
the engine, this page, and the type definitions `luau-analyze` checks a
project with (`games/studio-engine2/types/love.d.luau` and
`studio.d.luau`). The `<!-- names: … -->` line under each page lists every
name the page covers; a name the engine has and no page lists fails it, and
so does a listed name the engine doesn't have.

## Studio engine 2.0 [index]

**Two layers, both one way:**

- **LÖVE's layer.** A 2.0 game is a LÖVE program: `love.load`,
  `love.update(dt)`, `love.draw()`, `love.keypressed`, `love.mousepressed`,
  `love.touchpressed` and the rest, and `love.graphics`, `love.audio`,
  `love.math` and the others, with LÖVE's names and meanings. What a
  deterministic, sandboxed phone game can't have is left out, and every
  difference is in one table ([love]).
- **The retained layer.** Nodes (`Sprite`, `Shape`, `Text`, `Layer`,
  `Group`, `Tilemap`, `Walker`, `Emitter`) that the engine keeps, moves and
  draws, in Rust, for what LÖVE does by hand each frame. `love.draw` is the
  immediate layer: right for a HUD, a laser or a grid, and paid for in Lua
  time every frame. Nodes are what make a few hundred moving things cheap.
- **Maimbrain's additions** are what LÖVE has no API for: rounds, the
  camera that follows, effects, music, voices, widgets, sensors, haptics.
  They're plain globals (`round`, `camera`, `fx`, `spring`, `music`,
  `audio`, `say`, `ui`, `input`, `sensors`, `haptic`, `scene`, `save`,
  `progress`, `debug`; in 3D also `world`, `geometry`, `material`,
  `physics`), there from `main.lua`'s first line, as LÖVE's libraries
  (`flux`, `timer`, …) are: nothing needs `require`. Anything not under
  `love.` and not one of LÖVE's libraries is ours.

**What runs in Rust: reach for these first.** Anything that moves every
frame belongs to the engine, not to `love.update`, where a few hundred
things cost a frame:

- tweens and back-and-forth motion: `flux.to(node, secs, { y = … })` and
  `:yoyo()`; delays and repeats: `timer.after`, `timer.every` ([libraries]);
- springs and hits: `spring(node, { x = … })`, `fx.punch`, `fx.shake`,
  `fx.hitstop`, `fx.slowmo` ([juice]);
- straight-line motion, falling and a steady spin: a node's `vx`, `vy`,
  `gravity` and `vr` (`vz`, `vrx`, `vry`, `vrz` in 3D) ([scene], [3d]);
- frame animation: `sprite:play("run")`, or anim8 ([2d], [libraries]);
- the camera: `camera:follow(node, …)`, set once ([camera]);
- particles and popups: `Emitter` nodes, `fx.burst`, `fx.popup` ([2d], [juice]);
- a game's rules from a map: the quick path's `Tilemap`, behaviours
  (`player.platformer`, `enemy.patrol`, …) and `on("overlap")` ([quick]);
- walking figures: `Walker`, whose feet plant as it moves ([2d]); in 3D,
  `model:play`, `AnimationTree`, `plantFeet` and `gait` ([models]);
- characters that run, jump and climb: `CharacterBody2D` and
  `CharacterBody3D` with `moveAndSlide()` ([physics]);
- falling, stacking, joints and vehicles: `love.physics` bodies that nodes
  follow, or Godot's `RigidBody3D`, `VehicleBody3D` and the rest ([physics]);
- enemies that find their way: `NavigationAgent3D{ autoMove = true }`
  ([physics]), `Tilemap:path` ([2d]); boxes that collide: bump ([libraries]);
- outdoor 3D: `Terrain{}` with its photographed ground, and `scatter` on
  it ([terrain]); many copies of a thing: `node:instance{}` ([3d]);
- music that follows the game (`music.mood{}`, `music.go`, `on("beat")`),
  footsteps, hums and impacts (`footsteps`, `hum`, `audio.impact`) ([sound]);
- talking characters: `say`, `dialogue{}`, `bark`, `cutscene{}` ([voice]);
- buttons, bars, hints, lessons and the stats card, in the game's
  identity: `ui.button{}` and the rest ([ui]);
- aiming by pointing the phone: `sensors.scope{}` ([input]);
- a surface effect (a dissolve, water, toon shading): a WGSL shader on a
  `material{}` or an image ([shaders]).

**Conventions:**

- **Units.** Logical units: `x` right, `y` down, `(0, 0)` the top left of
  the canvas, 360×640 upright or 640×360 sideways unless `studio.toml` says
  `size`. Angles are radians, clockwise on screen (but a 3D camera's
  `fov` is degrees, as in three.js and Godot). Times are seconds of
  game time. Speeds are units a second, gravity units a second².
- **Colours** are LÖVE's `r, g, b, a` from 0 to 1, as numbers or a table
  `{ r, g, b, a }`, or a `0xRRGGBB` number (ours) anywhere a colour goes.
  A node's `color` reads back as `0xRRGGBB`.
- **Names are paths.** `"hero"` is `images/hero.png` (or `.jpg`), `"jump"`
  is `sounds/jump.ogg` or `sounds/jump.sfx.toml`, `"level1"` is
  `maps/level1.map`. A subfolder is part of the name (`"bg/dusk"`). LÖVE's
  full paths (`"images/hero.png"`) work too.
- **Node constructors take one table** (`Sprite{ image = "hero", x = 40 }`),
  and everything in it is a property you can set later.
- **Errors name the line.** A mistake raises an error with its file and
  line (`game/main.lua:40: …`), and a misspelled name says what it might be
  ("no property `colour` on Sprite; did you mean `color`?").

**Pages:** [project], [love], [libraries], [scene], [2d], [text], [camera],
[juice], [sound], [voice], [input], [rounds], [identity], [ui], [saves], [debug], [bot], [quick],
[3d], [models], [world], [terrain], [physics], [shaders], [3d1], [arcade], [maths], [generate], [budgets], [lua], [upgrade].

## Project and canvas [project]

```
studio.toml           title, engine, kind, orientation, size, lives, scores, the game-over text
game/main.lua         love.load, love.update, love.draw and the callbacks; require("name") loads game/name.lua
game/<name>.lua       more of the game's Lua (folders too: require("enemies/boss"))
images/<name>.png     any picture up to 4096×4096 (.jpg for opaque paintings); images/<name>.toml beside it
tiles/<name>.png      a tile set's sheet, with tiles/<name>.toml
maps/<name>.map       a tile map as text
sounds/<name>.ogg     a sound effect, or sounds/<name>.sfx.toml (a synth sound)
music/<name>.ogg      a music loop, or music/<name>.song.toml; a music graph music/<name>.music.json and its files; a setlist music/<name>.songs.json
fonts/<name>.ttf      a font of the project's own (library fonts need no file)
models/<name>.glb     a 3D game's models ([3d])
```

`studio.toml` takes the place of LÖVE's `conf.lua`:

```toml
title = "Catch"
engine = "2.0"                # this engine
kind = "2d"                   # or "3d": the 3D build ([3d])
orientation = "portrait"      # portrait | landscape
size = [360, 640]             # the canvas in logical units (default 360×640, or 640×360 landscape)
lives = 3                     # round.lives at the start (1–99, default 1)
fonts = ["Fraunces"]          # library fonts beyond the ones the Lua names
level = true                  # a level, won or lost: the difficulty bot judges it on wins ([bot])
sensors = ["haptics", "tilt"] # what the game reads beyond touch: tilt, attitude, motion, loudness ([input])

[text]
over = "Out of fish!"         # the game-over heading when the lives run out

[[scores]]
label = "Fish"                # board 0's name: round.score goes here
order = "high"                # high | low (lower is better)

[[scores]]                    # more boards, each with an id ([rounds])
id = "streak"                 # round.post("streak", n)
label = "Longest streak"

[[scores]]
id = "daily"
daily = true                  # the daily board: round.score on the day's seed, a new board each UTC day
```

**Images.** `images/<name>.toml`, all optional:

```toml
frame = [32, 32]              # a sheet of equal frames, left to right then down (else the whole image)
pixel = true                  # pixel art: drawn crisp (sampled nearest)
walker = true                 # a Walker's picture: mb build finds its body and foot
# or say where they are: body = [x, y, w, h], foot = [x, y, w, h], in the picture's pixels

[anims]
run = { frames = "1-8", fps = 12 }   # frames count from 1; ranges as anim8 writes them
idle = { frames = [9, 10], fps = 4 }
jump = { frames = "3" }              # fps defaults to 8
```

One pixel is one logical unit: a 64×64 picture is 64 units across until a
node scales it. `mb build` packs small PNGs into atlases and keeps JPEGs
whole; nothing changes for the game.

**Tile sets.** `tiles/<name>.png` is a sheet of square tiles (or `frame =
[w, h]`), and `tiles/<name>.toml` names them. Frames count from 1. A sheet
without a `.toml` is one solid tile. Tiles are pixel art unless they say
`pixel = false`.

```toml
frame = [32, 32]
[tiles.ground]
frame = 1
solid = true
[tiles.ledge]
frame = 2
oneway = true                 # solid from above only
[tiles.spikes]
frame = 3
hazard = true
```

**Maps.** `maps/<name>.map`: a legend, then the rows after `[map]`. Each
character is one cell; `.` and a space are empty. A legend entry is a
tile (`"ground"` is the set's first tile, `"ground.ledge"` a named one), a
thing, or a marker. A map draws from one tile set, and is at most
1024×1024 cells.

```
[legend]
"#" = "ground"
"=" = "ground.ledge"
"*" = { thing = "coin", value = 5 }    # a Sprite of images/coin, tagged "coin"; value goes in its data
"@" = { marker = "start" }             # map:marker("start") → x, y

[map]
..........*.....
@.....====......
################
```

**Lua files.** `game/main.lua` runs first, at the start of every round,
then `love.load`. `require("name")` runs `game/name.lua` once and returns
what it returns; `require` of a library's usual name (`"flux"`,
`"hump.timer"`, `"hump.camera"`, `"anim8"`, `"bump"`, `"classic"`) returns
the built-in one. `print(…)` writes to the game's log (what `play_test`
reports), never to the screen. `mb build` compiles the Lua to bytecode; a
syntax error stops it with the file and line.

<!-- names: require print -->

## The LÖVE subset [love]

**Callbacks**, with LÖVE's arguments:

```lua
function love.load() end                                   -- a round starts (after main.lua's top level)
function love.update(dt) end                               -- every frame, dt in seconds (0 during fx.hitstop, slowed by fx.slowmo)
function love.draw() end                                   -- after update: the immediate layer
function love.keypressed(key, scancode, isrepeat) end      -- also love.keyreleased(key, scancode)
function love.mousepressed(x, y, button, istouch, presses) end  -- a touch is the mouse too, as on LÖVE's phones
function love.mousereleased(x, y, button, istouch, presses) end
function love.mousemoved(x, y, dx, dy, istouch) end
function love.wheelmoved(x, y) end
function love.touchpressed(id, x, y, dx, dy, pressure) end -- also love.touchmoved, love.touchreleased
function love.gamepadpressed(joystick, button) end         -- also love.gamepadreleased
function love.gamepadaxis(joystick, axis, value) end
function love.resize(w, h) end
```

**A frame, in order:** this frame's input callbacks; then the libraries'
work in Rust (flux, hump.timer, anim8 and the Sprites' animations) and the
callbacks and signals it fires; then `love.update(dt)`; then
`love.draw()`; then, in Rust, the nodes' motion (`vx`, `vy`, `gravity`, `vr`),
the walkers' feet and the particles; then the camera follows. So a tween's
`oncomplete` runs before the `love.update` of the frame it finishes in, and
the camera sees where things ended up.

**`love.graphics`**, called in `love.draw` as in LÖVE:

| | |
|---|---|
| Shapes | `rectangle(mode, x, y, w, h, rx)` (`rx` rounds the corners), `circle(mode, x, y, r)`, `ellipse(mode, x, y, rx, ry)`, `arc(mode, [arctype,] x, y, r, a1, a2)` (`"pie"`, `"open"`, `"closed"`), `line(x1, y1, x2, y2, …)`, `polygon(mode, …)`, `points(…)`; points as numbers or one table. `mode` is `"fill"` or `"line"` |
| Text | `print(text, x, y, r, sx, sy, ox, oy)`, `printf(text, x, y, limit, align, …)` (`"left"`, `"center"`, `"right"`, `"justify"`); fonts: `newFont`, `setNewFont`, `setFont`, `getFont` ([text]) |
| Pictures | `newImage(name)`, `newQuad(x, y, w, h, sw, sh)` (or the image in place of `sw, sh`), `draw(drawable, [quad,] x, y, r, sx, sy, ox, oy)` for an Image, a Text object, a SpriteBatch or a ParticleSystem; `newSpriteBatch(image, max)`; `newText(font, text)` |
| State | `setColor(r, g, b, a)` / `getColor()`, `setBackgroundColor` / `getBackgroundColor`, `setLineWidth` / `getLineWidth`, `setPointSize`, `setBlendMode(mode)` (`"alpha"`, `"add"`, `"multiply"`, `"screen"`), `setDefaultFilter("nearest")` / `getDefaultFilter` |
| Transform | `push`, `pop`, `translate(dx, dy)`, `rotate(r)`, `scale(sx, sy)`, `origin` |
| The canvas | `getWidth`, `getHeight`, `getDimensions`: the canvas in logical units |
| Canvases | `newCanvas([w, h,] [settings])`, `setCanvas(canvas)` / `setCanvas()`, `getCanvas`, `clear([r, g, b, a])` (the canvas drawn to, or this frame's drawing so far), and `draw(canvas, x, y, …)`, tinted by the colour as LÖVE does. What's drawn into a canvas stays until it's cleared, from any callback, so trails and a level drawn once work as in LÖVE. It's kept as what was drawn (mb2d has no render targets), not as pixels: no clipping to its size, no read-back, and at most 32,768 things in one (a line that carries on from the last is the same thing); 16 canvases a round |

`love.draw` draws in canvas units, as LÖVE draws in the window's pixels;
`camera:attach()` and `camera:detach()` put what's between them in the
world ([camera]). The colour and the transform start fresh each frame.

**Objects** from `love.graphics`:

- **Image** (`newImage`): `getWidth`, `getHeight`, `getDimensions`,
  `setFilter("nearest" | "linear")`, `getFilter`, `typeOf`.
- **Quad** (`newQuad`): `getViewport`, `setViewport`, `typeOf`.
- **SpriteBatch** (`newSpriteBatch`): `add([quad,] x, y, r, sx, sy, ox,
  oy)` returns an id, `set(id, …)`, `clear`, `getCount`, `setColor`,
  `getImage`, `typeOf`. One batch holds `max` sprites (default 1000).
- **ParticleSystem** (`newParticleSystem(preset, buffer)`): our particles
  with a preset's look (`"sparkles"`, `"confetti"`, `"stars"`, `"burst"`,
  `"sparks"`, `"ring"`, `"dust"`, `"smoke"`; an Image in its place gives
  `"burst"`). As in LÖVE, call `ps:update(dt)` in `love.update` and
  `love.graphics.draw(ps, x, y)` in `love.draw`. `emit(n)`,
  `setPosition` / `moveTo` / `getPosition`, `setEmissionRate` /
  `getEmissionRate`, `setParticleLifetime(min, max)`, `setSpeed(min,
  max)`, `setSpread(angle)` (the whole angle, as LÖVE's),
  `setLinearAcceleration(xmin, ymin, xmax, ymax)`, `setColors(…)`,
  `setSizes(size)` (in units), `start`, `stop`, `pause`, `reset`,
  `isActive`, `getCount`, `setBufferSize` / `getBufferSize` (up to 2000),
  `typeOf`. Ranges take their middle; a particle takes one of the colours
  rather than fading through them. `Emitter` nodes are the same particles
  without the bookkeeping ([2d]).
- **Canvas** (`newCanvas`): `getWidth`, `getHeight`, `getDimensions`,
  `renderTo(fn)`, `setFilter` (no effect), `typeOf`; `newImageData` is an
  error (no pixels).

**`love.timer`:** `getDelta()` (this frame's `dt`), `getTime()` (game time
since the round started), `getFPS()` (60), `getAverageDelta()`.

**`love.window`:** `getDimensions()` (the canvas), `getSafeArea()` (x, y,
w, h of the canvas the phone's notch and corners leave clear),
`getDPIScale()` (2), `getMode()` (the canvas's width and height, and an
empty table of flags). The platform owns the window: `setMode(w, h)`
changes nothing, and is accepted when it asks for the canvas `studio.toml`'s
`size` makes (a LÖVE game's window becomes `size`), an error naming the
`size` to write otherwise.

**`os`**, as far as a round has it: `os.time()` is the round's clock (a
moment in 2026 picked by the round's seed, then game time), so
`math.randomseed(os.time())` gives each round its own numbers and a replay
the same ones; `os.clock()` is game time; `os.difftime(a, b)`; `os.exit()`
ends the round. There's no calendar (`os.time(date)`, `os.date`), files or
processes.

**`love.system`:** `getOS()` (`"Maimbrain"`), and `vibrate` ([sound]).

`love.audio` is on [sound], `love.math` on [maths], `love.keyboard`,
`love.mouse`, `love.touch` and `love.joystick` on [input].

**Where 2.0 differs from LÖVE**, every difference in one place:

| LÖVE | 2.0 | Why |
|---|---|---|
| `love.load` runs once per program | Once per round: each round is a fresh Lua VM, with `main.lua` run again first | A round depends only on its seed and input |
| `love.timer.getTime()` is the wall clock | Game time since the round started; no `love.timer.sleep` | No clock |
| `love.math.random` and `math.random` | Seeded per round by the platform; `setRandomSeed` and `newRandomGenerator(seed)` work, but the sequences aren't LÖVE's | Replays; the SDK's PCG32 |
| `love.filesystem`, `love.thread`, `love.event`, `love.data`, `love.image`, `love.sound`, `love.video` | Absent | The sandbox; one thread; the platform owns the loop |
| `love.physics` is Box2D | The same API on Rapier, in a 2D game: same calls, a different simulation, fixed 60 Hz steps; what differs is on [physics]. A 3D game's bodies are Godot's | Determinism and the SDK's proven physics |
| `love.window.setMode` | Accepted for the canvas `studio.toml`'s `size` makes, and changes nothing | The platform owns the window |
| `love.window.setTitle`, `setFullscreen`, `love.quit` | Absent | The platform owns the window and rounds |
| `newCanvas` | What's drawn into it, kept and drawn again; no pixels, no clipping to its size | mb2d has no render targets |
| `stencil`, `setScissor`, `newMesh`, `shear` | Absent | mb2d has no clipping or meshes |
| `newShader`, `setShader`, `getShader`, `Shader:send` | WGSL, not GLSL | A shader is `fn sprite` in `shaders/<name>.wgsl`, for images ([shaders]) |
| `os.time()`, `os.clock()` | The round's clock, from its seed and game time; `os.exit()` ends the round | Replays; the sandbox has no clock |
| Cursors (`getSystemCursor`, `setCursor`, `setVisible`) | Accepted, and do nothing | A phone has no pointer |
| Asset paths (`newImage("assets/ship.png")`, `newFont("assets/f.ttf", 16)`) | Found by the file's name in `images/` and `fonts/` | A project keeps pictures and fonts there |
| Blend modes `subtract`, `lighten`, `darken`, `replace` | An error | mb2d has four modes |
| `setDefaultFilter("nearest")`, `Image:setFilter` | Work (mb2d 3's nearest filter) | Pixel art |
| `newFont(file, size)` | A library family by name (`"Fraunces"`), `"pixel"`, `"sans"`, `"bold"`, or `fonts/<name>.ttf`, baked by `mb build` ([text]) | Fonts are baked atlases |
| `newParticleSystem(image, buffer)` | A preset's particles: the first argument is a preset's name, and only the methods above | Particles are the engine's, in Rust |
| `newSource(name, "queue")`, `Source` effects and filters | Absent | No sample access; effects are a bus's (`audio.bus`, [sound]) |
| `Source:tell`, `Source:seek`, `isPlaying` | Estimates from game time and the sound's length | Audio output is never read back |
| `isrepeat` in `love.keypressed` | Always false | The host doesn't send key repeat |
| Gamepad `start` and `back` | Never sent | The platform's pause and menu |
| `love.errorhandler`, `love.run` | Ours: the error with its line, and the platform's loop | The platform owns the loop |
| `conf.lua` | `studio.toml` | The project format |
| — | `0xRRGGBB` colours anywhere; `input.ax`, `input.ay` (one steering axis from arrows, WASD, d-pad and stick) | Generated palettes; touch, key and pad parity |

<!-- names: love.load love.update love.draw love.keypressed love.keyreleased love.mousepressed love.mousereleased love.mousemoved love.wheelmoved love.touchpressed love.touchmoved love.touchreleased love.gamepadpressed love.gamepadreleased love.gamepadaxis love.resize
love.graphics.rectangle love.graphics.circle love.graphics.ellipse love.graphics.arc love.graphics.line love.graphics.polygon love.graphics.points love.graphics.print love.graphics.printf love.graphics.newFont love.graphics.setNewFont love.graphics.setFont love.graphics.getFont love.graphics.newImage love.graphics.newQuad love.graphics.draw love.graphics.newSpriteBatch love.graphics.newText love.graphics.setColor love.graphics.getColor love.graphics.setBackgroundColor love.graphics.getBackgroundColor love.graphics.setLineWidth love.graphics.getLineWidth love.graphics.setPointSize love.graphics.setBlendMode love.graphics.setDefaultFilter love.graphics.getDefaultFilter love.graphics.push love.graphics.pop love.graphics.translate love.graphics.rotate love.graphics.scale love.graphics.origin love.graphics.getWidth love.graphics.getHeight love.graphics.getDimensions love.graphics.newParticleSystem
love.graphics.newCanvas love.graphics.setCanvas love.graphics.getCanvas love.graphics.clear Canvas:getWidth Canvas:getHeight Canvas:getDimensions Canvas:renderTo Canvas:setFilter Canvas:newImageData Canvas:typeOf
love.window.setMode love.window.getMode os.time os.clock os.difftime os.exit
Image:getWidth Image:getHeight Image:getDimensions Image:setFilter Image:getFilter Image:typeOf Quad:getViewport Quad:setViewport Quad:typeOf
SpriteBatch:add SpriteBatch:set SpriteBatch:clear SpriteBatch:getCount SpriteBatch:setColor SpriteBatch:getImage SpriteBatch:typeOf
ParticleSystem:update ParticleSystem:emit ParticleSystem:setPosition ParticleSystem:moveTo ParticleSystem:getPosition ParticleSystem:setEmissionRate ParticleSystem:getEmissionRate ParticleSystem:setParticleLifetime ParticleSystem:setSpeed ParticleSystem:setSpread ParticleSystem:setLinearAcceleration ParticleSystem:setColors ParticleSystem:setSizes ParticleSystem:start ParticleSystem:stop ParticleSystem:pause ParticleSystem:reset ParticleSystem:isActive ParticleSystem:getCount ParticleSystem:setBufferSize ParticleSystem:getBufferSize ParticleSystem:typeOf
love.timer.getDelta love.timer.getTime love.timer.getFPS love.timer.getAverageDelta love.window.getDimensions love.window.getSafeArea love.window.getDPIScale love.system.getOS -->

## LÖVE's libraries [libraries]

The best-known MIT libraries for LÖVE are built in under their own names,
so `require` isn't needed (it works too: [project]):

| Library | How | Why that way |
|---|---|---|
| **flux** (tweens) | Its API, run in Rust: `flux.to(obj, secs, { x = 100 })` | Per-frame work for many things belongs in Rust |
| **hump.timer** | `timer.after`, `every`, `during`, `script`, `tween`, `cancel`, `clear`, run in Rust | The same |
| **anim8** (frame animation) | Its API, run in Rust | The same |
| **bump** (box collision) | Its API, run in Rust | Platformers in LÖVE are mostly bump; in Lua it's slow past a few hundred items |
| **classic** (classes) | The real library, as Lua, as the global `Object` | Nothing per frame |

The engine updates them each frame before `love.update`, so `flux.update`,
`timer.update` and an animation's `update` do nothing (calling them is
harmless).

**flux.** `flux.to(obj, secs, vars)` tweens numbers on a node (`x`, `y`,
`rotation`, `scale`, `alpha`, `vx`, `size`, …), the `camera` (`x`, `y`,
`scale`, `rot`), or a plain table; a node's `color` or a Layer's `fill`
tweens as a colour. It returns a tween:

- `:ease(name)`: flux's names (`"linear"`, `"quadin"`, `"quadout"`,
  `"quadinout"`, `"cubic…"`, `"quart…"`, `"quint…"`, `"expo…"`,
  `"sine…"`, `"circ…"`, `"back…"`, `"elastic…"`) or easings.net's
  (`"outBack"`, `"inOutSine"`); the default is `"quadout"`, as flux's;
- `:delay(secs)`; `:onstart(fn)`, `:onupdate(fn)`, `:oncomplete(fn)`;
- `:after([obj,] secs, vars)`: the next tween, when this one ends;
- `:stop()`;
- `:yoyo()` (ours): there and back again, for ever.

Writing a property a tween is moving stops the tween moving it.

**hump.timer.** `timer.after(delay, fn)`, `timer.every(delay, fn, count)`
(`fn` returning `false` stops it), `timer.during(delay, fn(dt, left),
after)`, `timer.cancel(handle)` or `handle:cancel()`, `timer.clear()`,
`timer.script(function(wait) … wait(1) … end)`, and `timer.tween(secs,
subject, target, method, after)` (hump's names, `"in-out-quad"`, on
flux). Timers run in the order they were made.

**anim8.** `anim8.newGrid(frameWidth, frameHeight, imageWidth,
imageHeight, left, top, border)` makes a grid; calling it, `grid("1-8",
1)`, or `grid:getFrames(…)` gives frames by columns and rows (from 1).
`anim8.newAnimation(frames, durations, onLoop)`: `durations` is seconds a
frame, or a table by frame (keys may be ranges); `onLoop` is a function,
`"pauseAtEnd"` or `"pauseAtStart"`. An animation has `draw(image, x, y, r,
sx, sy, ox, oy)` (in `love.draw`), `gotoFrame(n)`, `pause`, `resume`,
`pauseAtEnd`, `pauseAtStart`, `flipH`, `flipV`, `clone`, `getDimensions`,
`update`, and reads `position`, `timer`, `status`, `flippedH`, `flippedV`,
`frames`, `durations`, `totalDuration`. A `Sprite` plays its sheet's
animations with `sprite:play("run")` instead ([2d]).

**bump.** `bump.newWorld(cellSize)` (default 64), then on the world:
`add(item, x, y, w, h)`, `remove(item)`, `update(item, x, y, w, h)`,
`move(item, goalX, goalY, filter)` → `actualX, actualY, cols, len`, and
`check` (the same without moving), `queryRect(x, y, w, h, filter)`,
`queryPoint(x, y, filter)`, `hasItem`, `getRect`, `countItems`,
`getItems`, `getCellSize`. A filter returns `"slide"` (the default),
`"touch"`, `"cross"`, `"bounce"` or nil. Answers come in the order items
were added, where bump.lua's order isn't fixed.

**classic.** `local Enemy = Object:extend()`, `function Enemy:new(x, y)
… end`, `Enemy(10, 20)`; `Object:implement(mixin)`, `obj:is(Enemy)`.

**Godot's Tween**, for those who know it: `node:createTween()` returns a
Tween over flux. `tween:tweenProperty("y", 120, 0.4)` returns a step with
`:setTrans("sine")`, `:setEase("inOut")` and `:setDelay(secs)`;
`tweenInterval(secs)`, `tweenCallback(fn)`, `setParallel()` (every step
from then on runs at once), `parallel()` (the next step runs with the one
before), `kill()`, and the signal `tween.finished`. Steps run one after
another unless the tween is parallel.

<!-- names: flux.to flux.update FluxTween:ease FluxTween:delay FluxTween:onstart FluxTween:onupdate FluxTween:oncomplete FluxTween:after FluxTween:stop FluxTween:yoyo
timer.after timer.every timer.during timer.cancel timer.clear timer.script timer.tween timer.update TimerHandle:cancel
anim8.newGrid anim8.newAnimation Grid:getFrames Animation:draw Animation:gotoFrame Animation:pause Animation:resume Animation:pauseAtEnd Animation:pauseAtStart Animation:flipH Animation:flipV Animation:clone Animation:getDimensions Animation:update Animation.position Animation.timer Animation.status Animation.flippedH Animation.flippedV Animation.frames Animation.durations Animation.totalDuration
bump.newWorld World:add World:remove World:update World:move World:check World:queryRect World:queryPoint World:hasItem World:getRect World:countItems World:getItems World:getCellSize
Object.extend Object.new Object.implement Object.is
Node:createTween Tween:tweenProperty Tween:tweenInterval Tween:tweenCallback Tween:setParallel Tween:parallel Tween:kill Tween.finished Tweener:setTrans Tweener:setEase Tweener:setDelay -->

## Nodes [scene]

The retained layer. Making a node puts it in the scene, under its `parent`
or at the top. Every node has:

| Property | | |
|---|---|---|
| `x`, `y` | Position in the parent's space | units |
| `rotation` | Turn, clockwise | radians |
| `scale`; `sx`, `sy` | 1 is as made; `scale` sets both | |
| `vx`, `vy`, `gravity` | Moved in Rust after `love.update` | units/s; `gravity` units/s² onto `vy` |
| `vr` | Spin: turned in Rust after `love.update`, as `vx` moves it. A spun `rotation` stays within −π..π; writing `rotation` spins on from there | radians/s, clockwise |
| `alpha`, `visible` | `alpha` 0–1, multiplied down the tree | |
| `color` | Tints a sprite or text; a shape's fill | a colour; reads `0xRRGGBB` |
| `order` | Draw order: higher on top. Nodes at 0 or less draw before `love.draw`, above 0 after it | |
| `fixed` | Ignores the camera (a HUD) | |
| `parent`, `children` | Setting `parent` keeps the node where it is on screen (`nil` is the top); `children` is read only | |
| `name`, `tag` | For `scene.find(name)`, `scene.all(tag)` | |
| `data` | The game's own table on the node | |
| `alive` | False once destroyed | |

| Method | |
|---|---|
| `node:destroy()` | Removes it and its children. A destroyed node's properties stay readable for a while (so a handler can read what it just destroyed); writing them is an error |
| `node:find(name)` | A descendant by name, or nil |
| `node:worldPosition()` | x, y after its parents |
| `node:createTween()` | Godot's Tween ([libraries]) |
| `scene.find(name)` | The first node with that name, or nil (2D nodes first, then 3D ones) |
| `scene.all(tag)` | Every node with that tag, in a list: the 2D ones, then the 3D ones, each as made |

- **Unknown names are errors**, not new fields: a game's own values go in
  `node.data` (`node.data.hp = 3`), and a near miss says what you meant.
- **A constructor's `parent`** puts the node under it, and its `x` and `y`
  are in the parent's space; setting `parent` later keeps the node where it
  is.
- **Signals.** A node's events are its signals (Godot's idea):
  `sprite.animationFinished:connect(fn)`, `walker.stepped:connect(fn)`. A
  signal has `connect(fn)` (returns `fn`), `disconnect(fn)` and
  `emit(…)`.

<!-- names: Node.x Node.y Node.rotation Node.scale Node.sx Node.sy Node.vx Node.vy Node.gravity Node.vr Node.alpha Node.visible Node.color Node.order Node.fixed Node.parent Node.children Node.name Node.tag Node.data Node.alive Node:destroy Node:find Node:worldPosition scene.find scene.all Signal:connect Signal:disconnect Signal:emit -->

## 2D nodes [2d]

Each kind's own properties, beyond [scene]'s:

| Node | Its own properties and methods |
|---|---|
| `Sprite{ image, frame, anchorX, anchorY, flipX, flipY, w, h }` | `image` is a name; `frame` counts from 1 in a sheet (`frame = [w, h]` in its `.toml`); `anchorX`, `anchorY` 0–1 (default 0.5, the middle) is the point at `x, y`; `w` and `h` read its drawn size and set it (by scaling). `sprite:play(name, { once, speed, restart })` plays an animation from its `.toml`'s `[anims]`, in Rust; `animationFinished` fires when a `once` one ends |
| `Shape{ shape, w, h, radius, points, corner, line }` | `shape` is `"rect"` (default, 32×32), `"circle"` (`radius`, default 16), `"poly"` or `"line"` (`points`, a flat list `{ x1, y1, x2, y2, … }` around the node); `corner` rounds a rect; `line` is a stroke width (0 fills); `x, y` is the middle; the colour is `color` |
| `Text{ text, font, size, align, width }` | `size` in units (default 16); `align` `"left"` (default), `"center"`, `"right"`: which edge or the middle is at `x`; `y` is the top; `width` wraps (0: never); `w` and `h` read its measured size. Fonts: [text] |
| `Layer{ image, parallax, repeatX, repeatY, fill }` | A picture behind or before the world that moves by `parallax` of the camera's travel (0 stays put, 1 moves with the world, default 1); `repeatX`, `repeatY` tile it to fill the view; `x, y` is its top left; `fill` paints a colour below it to the bottom of the view |
| `Group{}` | An empty node, to move things together |
| `Tilemap{ map, tiles }` | `maps/<map>.map` drawn from its tile set (`tiles` names it, or the map's legend does); `x, y` is its top left; `w`, `h` read its size in units. `map:solidAt(x, y)`, `map:tileAt(x, y)` (the tile's name or nil) and `map:marker(name)` (x, y of the marked cell's middle) are in world units. `map:path(x1, y1, x2, y2, { diagonal })` is A* from one cell to another round the solid tiles: the cells' middles, `{ x1, y1, x2, y2, … }`, or nil when there's no way; the same cells among equally short ways on every phone; with `diagonal = true` it cuts across, never past a solid corner. Making it places the legend's things: a `Sprite` of `images/<kind>` (a grey box without one) tagged with the kind, its numbers set as properties or into its `data`. `map` and `tiles` are set only when it's made |
| `Walker{ image, height, speed, flipX }` | A side-view figure whose feet plant by the distance it moves (by `vx`, a tween or writes to `x`), with bob and lean; its `x, y` is its feet. `image` is a picture with `walker = true` in its `.toml` (or `body` and `foot`); without one it's a greybox. `height` in units, feet to head (default 60); `speed` the full stride's speed (default 150: near it, it runs); it faces the way it moves unless `flipX`. `w`, `h` read its size. `stepped` fires on each footfall with the foot (1 or 2) and where it landed. A platformer's hero: `behaviour = "player.platformer"` ([quick]) |
| `Emitter{ preset, rate, colors, size, life, speed, spread, gravity, additive, emitting }` | Particles at the node, in the world. `preset`: `"sparkles"` (default), `"confetti"`, `"stars"`, `"burst"`, `"sparks"`, `"ring"`, `"dust"`, `"smoke"`; `rate` a second (default 0: only `e:emit(n)`); `colors` one colour or a list; `size`, `life`, `speed`, `spread` (radians either side of up), `gravity` change the preset's (nil keeps it); `additive` glows; `emitting = false` pauses the rate |

<!-- names: Sprite Shape Text Layer Group Tilemap Walker Emitter
Sprite.image Sprite.frame Sprite.anchorX Sprite.anchorY Sprite.flipX Sprite.flipY Sprite.w Sprite.h Sprite:play Sprite.animationFinished
Shape.shape Shape.w Shape.h Shape.radius Shape.points Shape.corner Shape.line
Text.text Text.font Text.size Text.align Text.width Text.w Text.h
Layer.image Layer.parallax Layer.repeatX Layer.repeatY Layer.fill
Tilemap.map Tilemap.tiles Tilemap.w Tilemap.h Tilemap:solidAt Tilemap:tileAt Tilemap:marker Tilemap:path
Walker.image Walker.height Walker.speed Walker.flipX Walker.w Walker.h Walker.stepped
Emitter.preset Emitter.rate Emitter.colors Emitter.size Emitter.life Emitter.speed Emitter.spread Emitter.additive Emitter.emitting Emitter:emit -->

## Text and fonts [text]

- **Fonts by name:** `"sans"` (Inter, the default), `"bold"` (Inter Bold),
  `"pixel"` (5×7, crisp at multiples of 8), any family in the font library
  (`"Press Start 2P"`, `"Fraunces"`; `mb fonts` lists them all, on a computer), or the
  project's own `fonts/<name>.ttf` by `"<name>"`. `mb build` bakes each
  font the Lua names (or `studio.toml`'s `fonts` lists); up to 16 a game.
- **Where text goes:** `Text{}` nodes for anything that stays;
  `love.graphics.print` and `printf` for one frame; a text object
  (`love.graphics.newText(font, text)`) to draw with `love.graphics.draw`.
- **`love.graphics.newFont(size)`** or **`newFont(name, size)`** (size in
  units, default 16) returns a Font; `setFont(font)` uses it,
  `setNewFont(…)` makes and uses one.
- **A Font** measures as LÖVE's does: `getWidth(text)`, `getHeight()`,
  `getLineHeight()`, `getWrap(text, limit)` (width, lines), `getAscent()`,
  `getDescent()`, `getBaseline()`, `setFilter` (fonts are drawn crisp
  already), `typeOf`. Layout is the host's, the same on every phone and in
  a replay.
- **A text object** has `set(text)`, `setf(text, limit, align)`, `clear()`,
  `getWidth()`, `getHeight()`, `getDimensions()`, `typeOf`.

<!-- names: Font:getWidth Font:getHeight Font:getLineHeight Font:getWrap Font:getAscent Font:getDescent Font:getBaseline Font:setFilter Font:typeOf TextObject:set TextObject:setf TextObject:clear TextObject:getWidth TextObject:getHeight TextObject:getDimensions TextObject:typeOf -->

## Camera [camera]

One `camera`, hump.camera's API, run in Rust. It moves the nodes (except
`fixed` ones) and what `love.draw` draws between `attach` and `detach`.

| | |
|---|---|
| `camera.x`, `camera.y` | The world point at the middle of the view (starts at the canvas's middle) |
| `camera.scale`, `camera.rot` | Zoom (above 0) and turn (radians) |
| `camera:lookAt(x, y)`, `camera:move(dx, dy)`, `camera:zoomTo(s)`, `camera:rotateTo(r)` | As hump's |
| `camera:attach()`, `camera:detach()` | Around world drawing in `love.draw` |
| `camera:worldCoords(sx, sy)` → x, y; `camera:cameraCoords(x, y)` → sx, sy | Screen to world and back (a touch is in screen units) |
| `camera:follow(node, { lag, lead, deadzone, bounds })` | Follows the node every frame, after everything has moved. `lag` seconds to catch up (default 0.12; 0 is locked), `lead` units ahead the way it's heading, `deadzone = { w, h }` it moves freely in, `bounds` a `Tilemap` or `{ x, y, w, h }` the view stays inside. `camera:follow(nil)` stops |

Set `follow` once (in `love.load`), not every frame. `require("hump.camera")`
returns a maker that sets and returns this camera. The shake is `fx.shake`
([juice]). In a 3D game `camera` is the 3D camera ([3d]): `x`, `y`, `z`,
`fov` (degrees), `roll`, `lookAt(x, y, z)`, `follow` and the rest; 1.0's
`camera(x, y, z, …)` places it too ([3d1]).

<!-- names: camera camera.x camera.y camera.scale camera.rot camera:lookAt camera:move camera:zoomTo camera:rotateTo camera:attach camera:detach camera:worldCoords camera:cameraCoords camera:follow -->

## Juice [juice]

| Call | |
|---|---|
| `fx.shake(amount, secs)` | Shakes the view (default 6 units, 0.25 s; meters in 3D); the same in a replay |
| `fx.hitstop(secs)` | Holds the world (default 0.06 s, up to 0.5): `love.update`'s `dt` is 0 and nothing moves; time, drawing and input go on |
| `fx.flash(color, secs)` | A flash over the whole view, fading (default white, 0.12 s). It starts at about 70% unless the colour gives its own alpha: `fx.flash({ 1, 1, 1, 0.3 })` |
| `fx.burst(preset, x, y, { count, color })` | One-shot particles in the world (default 16). `preset`: `"sparkles"`, `"confetti"`, `"stars"`, `"burst"`, `"sparks"`, `"ring"`, `"dust"`, `"smoke"` (a singular works too), as an `Emitter`'s ([2d]); `color` one or a list. In 3D: `fx.burst(preset, x, y, z, { … })` |
| `fx.popup(text, x, y, { color, size })` | Text that rises and fades ("+5"), in the world (default white, 18 units). In 3D: `fx.popup(text, x, y, z, { … })`, `size` in meters |
| `fx.slowmo(rate, secs)` | The world at `rate` of its speed (0.05–1, default 0.3) for `secs` (default 1, at most 3), easing in over 0.2 s and back over 0.6 s. `love.update`'s `dt`, tweens, springs, velocities and animation slow; `round.time`, the shake and punches keep real time. `fx.slowmo(1)` eases back now. A near miss, a last hit, a big moment: past 3 s it reads as the game freezing |
| `fx.punch(node, amount)` | A kick that springs back, on top of whatever else moves the node (2D or 3D): `amount` 0.2 makes it 20% bigger at once (default 0.2, −0.5 to 1). `fx.punch(node, { x, y, z, scale })` throws it about that far (units, meters in 3D) and back, and kicks its size by `scale` (a 3D physics body takes only a size punch: its place is the physics'). A score that changed, a hit, a landing |
| `spring(target, { key = goal }, { frequency, damping })` | Each key heads for its goal on a spring: `frequency` swings a second (default 4), `damping` 1 doesn't overshoot, under 1 bounces (default 0.5). The target is a node, the camera, a bus (`audio.bus`) or a table. Call again to retarget: it goes on from where it is, at its speed. Writing the key stops it, as for a tween |

Tweens are flux's and timers hump.timer's ([libraries]); particles that
last are an `Emitter`. Springs and slow motion run in Rust by the world's
`dt`, so they replay exactly.

```lua
-- A coin that pops when it's caught, and a camera that settles on the hero.
fx.punch(coinLabel, 0.3)
spring(camera, { x = hero.x, y = hero.y - 40 }, { frequency = 2, damping = 1 })
-- The last enemy down: slow, shake, punch.
fx.slowmo(0.25, 0.8) fx.shake(4, 0.3) fx.punch(hero, { y = -12, scale = 0.15 })
```

<!-- names: fx.shake fx.hitstop fx.flash fx.burst fx.popup fx.slowmo fx.punch spring -->

## Sound and music [sound]

- **Sound effects, LÖVE's way:** `local s = love.audio.newSource("jump",
  "static")` (the name is `sounds/jump.ogg` or `sounds/jump.sfx.toml`;
  `"stream"` works the same), then `s:play()`. Make each source once, in
  `love.load`, and `s:clone():play()` to overlap; a round has at most 512.
- **A Source:** `play`, `stop`, `pause`, `resume`, `clone`, `isPlaying`,
  `setLooping` / `isLooping`, `setVolume` / `getVolume` (0–1),
  `setPitch` / `getPitch` (above 0, up to 8; 2 is an octave up), `tell`,
  `seek`, `getDuration`, `setPosition` / `getPosition` (below),
  `typeOf`.
- **`love.audio`:** `play(…)`, `stop(…)`, `pause(…)` (the sources given,
  or for `stop` and `pause` every one), `setVolume` / `getVolume` (0–1,
  the sfx bus's volume), `getActiveSourceCount`, `setPosition` /
  `getPosition` (below).
- **Sound in the world:** `src:setPosition(x, y, z)` places a source (`z`
  is 0 if left out): it's quieter the farther it is from where the round
  is heard and pans to its side, as it plays. The round is heard from the
  camera (a 3D game's camera, or the middle of a 2D view) unless
  `love.audio.setPosition(x, y, z)` says where (`nil`: the camera again).
  In 3D a sound is full within 4 m and gone by 60; in 2D, full within half
  the canvas's width and gone by three widths.
- **Impacts:** `audio.impact(src, speed, fullSpeed)` plays a clone of
  `src` (where it's placed), as loud and heavy as the hit was hard: silent
  under 5% of `fullSpeed` (default 12 m/s in 3D, 600 units/s in 2D), then
  louder and lower up to it. It says whether it played: the same sound
  again within 0.05 s from the same place folds into the louder, and at
  most 6 such one-shots sound at once (the loudest heard win).
- **Foley:** `node.footsteps = "step"` on a `Walker` or a `Model` plays the
  sound at each footfall, harder the faster it goes: a Walker's, a
  `gait`'s, or (with `plantFeet`) each foot of a model's walk as it
  touches the ground, which then also fires its `stepped`. `node.hum =
  "engine"` on any node is a loop that follows its speed: silent standing
  still, louder and higher as it goes, quiet again as it stops
  (`{ sound, idle, full }` sets the speeds: default 0.3 and 8 m/s in 3D,
  10 and 300 units/s in 2D). Both are placed where the node is, and vary a
  little each time (the same in a replay). `nil` stops them.
- **Buses:** every sound plays into `audio.bus("sfx")`, music into
  `audio.bus("music")`, and both into `audio.bus("master")`. A bus has
  `volume` (0–4, 1 is as is), `lowpass` and `highpass` (Hz: a lowpass at
  500 is under water, a highpass at 800 a cheap radio; 20000 and 20 are
  off), `reverb` (`"room"`, `"hall"`, `"cave"`, `"off"`, or `{ wet, decay,
  damping, predelay }`) and `delay` (`{ time, feedback, wet, damping }`,
  `nil` off). Changes glide; `volume`, `lowpass` and `highpass` tween and
  spring (`flux.to(audio.bus("music"), 0.4, { lowpass = 600 })`). What a
  round does to the buses is undone when it ends.
- **Music:** `music.play(name, { fade, volume, zone })` plays
  `music/<name>.ogg` (or a `.song.toml`) as a loop, a music graph
  (`music/<name>.music.json`) or a setlist (`music/<name>.songs.json`);
  fade default 0.5 s (what was playing dies away over it), volume 0.7.
  `music.stop(fade)`. Below: music that follows the game.
- **Haptics:** `haptic("tap" | "success" | "fail" | "heavy")`, or
  `love.system.vibrate(seconds)`.
- The title card's attract mode plays no sound effects or haptics. Sound
  never changes the game: `isPlaying` and `tell` are estimates from game
  time, and a sound that hasn't loaded yet plays silently.

**Music that follows the game.** The `generate` tool makes it ([generate]):
`zones` (one take in parts, a music graph), `sections`, `setlist` (a song
per zone) and `layers` (a calm base, a high layer, and the tension kit).
A graph plays straight through, takes a jump now and then on a bar line so
a zone never loops audibly, and changes zone at the end of a phrase.

```lua
function love.load()
  music.play("run")                                  -- music/run.music.json, from its first zone
end
on("beat", function(beat, bar) end)                  -- beat 1–4 in its bar, bar from 1; a third argument is the beat's round.time
on("bar", function(bar) end)                         -- each downbeat, before its beat
function love.update(dt)
  music.mood{ intensity = speed / 300, danger = 1 - gap / 200 }   -- 0–1 each: the high layer; the tension kit
  if inCave and music.zone ~= "cave" then music.go("cave") end    -- at the end of this phrase
end
-- music.go("cave", { change = "nextBar" })          -- or "stinger", or "crossfade" with bars = 2
-- music.nearMiss()                                  -- a near catch: the sting, and the music dips
```

- **The grid is the round's clock.** Bars and beats are the graph's tempo
  map in game time, never the audio clock: a beat fires on the first
  frame at or past it, the same in a replay, so gameplay can hang on it
  (spawn on `on("beat")`, judge a tap against the third argument).
  `music.time` is seconds since bar 1's downbeat, `music.bar` the bar (0
  before the first downbeat), `music.beat` the beat in it with its
  fraction (1 to 4.99…: `music.beat % 1` is how far to the next),
  `music.bpm`, `music.zone` (the zone heard; a setlist's song) and
  `music.playing`.
- **A round starts its music on its own clock.** What a round plays
  carries into the next, and starts again there from its zone's top
  (or the zone `music.play` named), so a round's beats never depend on
  the round before. `music.play` of what's already playing does nothing.
- **Mood:** `intensity` brings up the high layer; `danger` the tension
  layer, then the clock, a heartbeat on the beat, and the music narrows;
  falling from high danger to low gives a moment of relief. A field left
  out keeps its value.
- **Zones:** `music.go(zone)` waits for the end of the phrase (or a
  seamless join on the way), and covers the change with the zone's
  stinger or a swell. A setlist's songs change the same way, each in its
  own key and tempo, and the grid changes with the song.
- A plain `.ogg` loop has no grid: `music.bar` and `music.beat` stay 0 and
  `on("beat")` doesn't fire. It carries on playing across rounds.

**Synth sounds** (as in 1.0): a sound can be a file Claude writes,
`sounds/<name>.sfx.toml`, which `mb build` renders to an `.ogg`:

```toml
# sounds/jump.sfx.toml
wave = "square"         # square (pulse), triangle, saw, sine, noise
duty = 0.25             # square's pulse width, 0.05–0.95
freq = 330              # Hz at the start
freq_end = 880          # Hz at the end (a slide)
duration = 0.18         # seconds held
attack = 0.005
decay = 0.08            # the fade after
arp = [0, 4, 7]         # semitone steps, cycled
arp_speed = 0.06        # seconds a step
vibrato = [0.4, 8]      # depth in semitones, rate in Hz
volume = 0.8
```

```lua
-- A mech that stomps and whines, a crate that clangs as hard as it lands,
-- and the world muffled under water.
mech.footsteps, mech.hum = "stomp", "servo"
local clang = love.audio.newSource("clang", "static")
clang:setPosition(crate.x, crate.y, crate.z)
audio.impact(clang, fallSpeed, 15)
flux.to(audio.bus("master"), 0.3, { lowpass = 500 })
```

<!-- names: love.audio.newSource love.audio.play love.audio.stop love.audio.pause love.audio.setVolume love.audio.getVolume love.audio.getActiveSourceCount love.audio.setPosition love.audio.getPosition
Source:play Source:stop Source:pause Source:resume Source:clone Source:isPlaying Source:setLooping Source:isLooping Source:setVolume Source:getVolume Source:setPitch Source:getPitch Source:tell Source:seek Source:getDuration Source:setPosition Source:getPosition Source:typeOf
music.play music.stop music.mood music.go music.nearMiss music.time music.bar music.beat music.bpm music.zone music.playing haptic love.system.vibrate audio.bus audio.impact Bus.name Bus.volume Bus.lowpass Bus.highpass Bus.reverb Bus.delay Walker.footsteps Model.footsteps Node.hum Node3D.hum -->

## Voice and dialogue [voice]

Characters talk in a box at the bottom with their portrait, whose mouth
moves with the words, whose eyes blink, and whose face shows the line's
expression. The text types out as it's spoken, subtitles always show, and
the music dips under voices. Lines live in `voices/lines.csv`
(`character,line,text,emotion,tag`); `generate` speaks them
(`voices/<who>/<line>.ogg` and its lip track) and draws the portraits
(`images/portraits/<who>.png`) ([generate]). A line nobody has spoken yet
plays as its subtitle, typed out at 30 characters a second, with the mouth
flapping, so a scene can be written and played before its voices exist.

```lua
cast("gruk", { portrait = "gruk", side = "right", name = "GRUK" })   -- side: "left" (default) or "right"
cast("pip", { portrait = "pip" })
local line = say("gruk", "halt", "smug")    -- one line in the box; the expression is optional
line.done:connect(function() end)           -- a Signal: the line finished or was tapped on
dialogue{
  { "gruk", "halt", "smug" },
  { "pip", "please" },
  { "gruk", "riddle1", choices = { "A shadow", "A river", "A coat" } },
  done = function(choice) end,              -- the last answer (1-based), or nil without choices
}
bark("wrong")                               -- a line tagged "wrong" (its tag column), in a bubble
narrate("gruk", "legend1")                  -- told in the next quiet stretch, never cut off
cast("gruk").expression = "annoyed"         -- the face for lines that don't say one
cast("pip").health = 0.4                    -- damage on the portrait: sweat, soot, cracks, sparks
cast("pip"):hit(0.34)                       -- a knock that shakes the face; health goes down by it
cutscene{
  { "letterbox", 0.12 },                    -- bars, a share of the screen's height; 0 takes them away
  { "camera", 200, 470, 1.35, 1, "inOutSine" },   -- 2D: centre x, y, zoom, secs, ease
  { "say", "gruk", "halt", "smug" },        -- waits for the line
  { "wait", 0.5 },
  { "cue", "gate" },                        -- on("cue", "gate", fn) hears it; the scene goes straight on
  { "title", "THE BRIDGE", "chapter one", 2 },    -- heading, subtitle, secs
  { "credits", { { "THE TROLL", "GRUK" }, { "THE TRAVELLER", "PIP" } }, 6 },
  { "fade", "out", 1 },                     -- or "in" (the scene starts covered)
  done = function() end,                    -- at the end, or when it's skipped
}
on("cue", "gate", function() gate:play("open") end)
```

- **A line's face:** the expression it's given, else its `emotion` from the
  script when the portrait has that row, else the cast member's
  `expression`. A portrait's expressions are its rows' names
  (`portrait = [...]` in its `.toml`); the neutral face blinks, and rows
  named `hurt` and `battered` show at low health.
- **The box takes the taps** while it's up: a tap shows the rest of the
  text, the next moves on (`advance = "auto"` on a `dialogue{}` moves on
  by itself 0.6 s after each voice). A line with `choices` waits, its text
  out, for an answer: the answers stack above the box and a tap on one
  picks it. Releases always reach the game.
- **Barks** are short lines in a bubble that never hold the game: one at a
  time, never over a line in the box, a cooldown between, and each tag's
  lines in a shuffled order. A bark may show while a question waits for
  its answer. `bark(tag)` says whether one will show.
- **Cutscenes:** steps run in order and each waits for itself (a `camera`
  move its seconds, a `say` its line), except `cue` and `letterbox`; the
  word `"meanwhile"` after a step lets the next start with it (a camera
  move under a title). A 3D game's camera step is `{ "shot", x, y, z, tx,
  ty, tz, secs }`. A tap hurries a line or cuts a move short; holding a
  finger down skips the rest: its `cue`s still fire, in order, then its
  `done`. The HUD steps aside while one plays, and the bars slide away
  after it. One that ends on `{ "fade", "out" }` leaves the screen covered
  until the next cutscene, which takes over from it: start that one with
  `{ "fade", "in", secs }` (alone, it just shows the game again). With no
  cutscene after it, the cover lifts after 3 s while the round goes on, and
  stays once the round has ended (an ending under its card). One
  cutscene at a time; no barks during one.
- `talking()` is true while a cutscene or a line in the box is up.
- **Timing.** Every line is timed by its lip track (or, without one, its
  sound's length, or its text), on game time unscaled by slow motion, so a
  replay plays the same whenever its audio arrives. A line's sound is
  decoded only once it's queued, with the next one; bark lines (those with
  a tag in the tag column) from the round's start, so a bark is heard the
  moment it shows.

<!-- names: cast say dialogue bark narrate cutscene talking CastMember.expression CastMember.health CastMember:hit -->

## Input [input]

LÖVE's callbacks ([love]) and modules are the input API. A touch is the
mouse too, as on LÖVE's phones: the first finger down also sends
`mousepressed(x, y, 1, true, 1)`, so a game that handles the mouse works
with a finger. Positions are in canvas units.

| | |
|---|---|
| `love.keyboard.isDown(key, …)`, `isScancodeDown` | LÖVE's key names (`"space"`, `"left"`, `"a"`, `"return"`, …); an unknown name is an error |
| `love.mouse.getPosition()`, `getX()`, `getY()`, `isDown(button, …)` | |
| `love.mouse.getSystemCursor(kind)`, `setCursor`, `getCursor`, `setVisible`, `isVisible`, `isCursorSupported` | Accepted for LÖVE code, and do nothing: a phone has no pointer (`isCursorSupported()` is false, `getCursor()` nil, a Cursor's `getType()` "arrow") |
| `love.touch.getTouches()` → ids; `love.touch.getPosition(id)` → x, y | |
| `love.joystick.getJoysticks()`, `getJoystickCount()` | The one controller, once it's been used |
| A Joystick | `isGamepadDown(button, …)` (`"a"`, `"b"`, `"x"`, `"y"`, `"leftshoulder"`, `"rightshoulder"`, `"leftstick"`, `"rightstick"`, `"dpup"`, `"dpdown"`, `"dpleft"`, `"dpright"`), `getGamepadAxis(axis)` (`"leftx"`, `"lefty"`, `"rightx"`, `"righty"`, `"triggerleft"`, `"triggerright"`), `getAxis(n)`, `getAxes()`, `getAxisCount()`, `getName()`, `getID()`, `isConnected()`, `isGamepad()`, `typeOf` |
| `input.ax`, `input.ay` (ours) | −1…1 from the arrows, WASD, the d-pad or the left stick: one steering axis for keys and pads (`ay` −1 is up) |
| `input.pad` (ours) | `"xbox"` once a controller has been used (for button glyphs), else nil |

Every way of playing does everything: if a drag steers, the arrows and the
stick steer too.

**Sensors** (ours). A game that reads one lists it in studio.toml's
`sensors` (`sensors = ["haptics", "tilt"]`); `mb build` then packs the
engine build that has it. Reading one that isn't listed is an error. Each
reads the same all frame, and is nil while unavailable (before the round,
on a phone without it, in the title card's attract mode), so the game
must still play without it.

| | |
|---|---|
| `sensors.tilt()` → roll, pitch | Radians. Roll: sideways tip, right edge down positive (±π/2). Pitch: 0 upright, π/2 lying flat face-up. Absolute: read it in `love.load` and steer by the difference |
| `sensors.attitude()` → yaw, pitch, roll | Which way the phone points. Yaw: turned right since the round's first reading (it drifts about a degree a minute). Pitch: the back of the phone above level (−π/2 face-up). Roll as tilt's |
| `sensors.motion()` → x, y, z | The hand's shake without gravity, in g (x right, y down the screen, z toward the player): about 0 still, 1–3 in a brisk shake. Add it up over frames to measure a shake |
| `sensors.loudness()` → 0–1 | The mic: a quiet room with the game's music reads ~0.45, a hard blow ~0.85 (treat that as full). nil until the player allows the mic: say what to do ("ALLOW THE MIC TO BLOW") |
| `sensors.scope{ precise, w, h }` | A crosshair aimed by pointing the phone (`attitude`, else `tilt`): `.x`, `.y` over the canvas (or `w` × `h`), `.source` (`"attitude"`, `"tilt"`, `"drag"`), `:recenter()` (center is where it points now), `:drag(dx, dy)` (a finger moves it too). `precise = true` steadies a resting hand and slows small corrections, for small targets. Make it once, in `love.load` |

- **Touch parity:** a sensor game steers by touch and keys too. Tilt to
  roll: a finger held anywhere pulls toward it, and `input.ax`, `input.ay`
  push. Aim with a scope: `scope:drag(dx, dy)` from `love.mousemoved`, and
  the arrows. Blow to fly: holding a finger blows too. Shake to roll the
  dice: a tap rolls them too.
- **`player.tilt`** (the quick path) steers by tilt in a build with it,
  and by dragging and the keys always; `mb build` adds `tilt` for it.
- **Play-tests** script the sensors: `{ at: 1, tilt: [-20, 10], for: 0.4 }`
  (roll and pitch from upright, in degrees, turned there over `for`),
  `{ at: 2, aim: [15, -5] }` (yaw and pitch), `{ at: 3, shake: 2, for: 0.5 }`
  (g), `{ at: 4, loud: 0.85, for: 1 }`. Without them the phone is upright
  and still, and the room is quiet.

<!-- names: sensors.tilt sensors.attitude sensors.motion sensors.loudness sensors.scope Scope.x Scope.y Scope.source Scope:recenter Scope:drag -->

<!-- names: love.keyboard.isDown love.keyboard.isScancodeDown love.mouse.getPosition love.mouse.getX love.mouse.getY love.mouse.isDown love.touch.getTouches love.touch.getPosition love.joystick.getJoysticks love.joystick.getJoystickCount
love.mouse.getSystemCursor love.mouse.setCursor love.mouse.getCursor love.mouse.setVisible love.mouse.isVisible love.mouse.isCursorSupported Cursor:getType Cursor:typeOf
Joystick:isGamepadDown Joystick:getGamepadAxis Joystick:getAxis Joystick:getAxes Joystick:getAxisCount Joystick:getName Joystick:getID Joystick:isConnected Joystick:isGamepad Joystick:typeOf input.ax input.ay input.pad -->

## Rounds [rounds]

The engine runs the platform's rounds: the title card (which plays the
game by itself), a press starts a round, the game-over card, bests. Each
round is a fresh VM: `main.lua`, then `love.load`, then a frame at a time.

| | |
|---|---|
| `round.score` | Board 0's score, sent when the round ends |
| `round.lives` | Starts at `studio.toml`'s `lives`; at 0 the round ends with `[text] over`'s heading, at once unless `round.endDelay` says |
| `round.endDelay` | Seconds (real time, at most 10) the round goes on at 0 lives before it ends, for a game's own ending: a tipping plate, a slow-motion fall. Nil (the default) ends it at once; `round.over` ends it sooner, with its heading |
| `round.over(heading, cause, won)` | Ends the round: the heading on the game-over card, `cause` for the reports ("fell", "time") |
| `round.time` | Seconds since the round started, unscaled: a hitstop or a lesson's slow motion slows `love.update`'s `dt`, not it. What should slow with the world keeps its own clock from `dt` (`clock = clock + dt`) |
| `round.title` | True on the title card: play an attract mode (there's no input) |
| `round.ended` | True after `over`: the world keeps updating under the card, with no input |
| `round.daily` | True in a daily round: on the day's seed, scored on the daily board |
| `round.subject` | The node the player must see at a glance (the hero, the ship, the basket), 2D or 3D: what `play_test(checks: ["readability"])` measures. Left nil, it's the first quick-path player, else the node the camera follows, else a node named or tagged `"player"` |
| `round.post(id, value)` | The round's best on another board (its `id` in studio.toml), sent when the round ends |
| `round.board(id)` | Opens that board's leaderboard (the round's own without an id) |

**The readability check.** `play_test(checks: ["readability"])` looks at
each frame of play for the subject: whether its outline is lighter or
darker than what's just around it (greyscale, as a glance sees it), its
size on the screen, the HUD over it, and whether the whole frame is too
dark for a phone in daylight. Each problem comes back as a line naming the
subject, what's behind it and when ("fox doesn't stand out against dusk_2
at 2.7 s: only 11% of its outline differs in lightness by 15+ …; aim for
35%+"), with the worst frame in greyscale and the subject outlined in
magenta. Set `round.subject` when the subject isn't the player or what the
camera follows.

**Boards.** The first `[[scores]]` is board 0, `round.score`'s. Each
later one needs an `id` (lowercase letters, digits, `_` and `-`), up to 16
boards in all. `round.post("streak", 12)` as often as you like: the
round's best on `"streak"` (by that board's `order`) goes up once, when
the game-over card does. Nothing posted on the title card counts. A game
shows a board with `round.board("streak")`, from a button of its own.

**The daily board.** One later board may say `daily = true` (it ranks
`round.score`, so it takes board 0's `order`; its label defaults to
"Today"). The engine then puts DAILY on the title card and the game-over
card, and SCORES on the game-over card: 112×44 buttons on the identity's
panel colour at the bottom corners of the card's area (DAILY at the right,
SCORES at the left; on the game-over card just above the challenge strip's
band), so keep what the title's attract mode and the ending must show out
of those corners. A daily round plays on the day's
seed, the same for every player that UTC day, so `love.math.random` and
the rest give everyone the same course: `round.daily` is true, its score
goes to the daily board, and its best is the day's. Make what the seed
decides the course (the obstacles, the deal of the cards), not the
player's unlocks.

**The game-over card covers the game.** The card, its heading and its
buttons are the platform's (the cards' words stay its own: [identity]), drawn
over `love.draw` and the nodes, so a game can't put its own text on it. A
game's ending (a last line, a slow-motion fall, what took the last life) plays
before the card: at 0 lives during `round.endDelay`, or before the game calls
`round.over`, whose heading is the card's.

<!-- names: round.score round.lives round.endDelay round.over round.time round.title round.ended round.daily round.subject round.post round.board -->

## Identity, cards and HUD [identity]

The title card, the game-over card and the HUD are the engine's, drawn in
the game's identity: its fonts, colours, shapes, layouts and motion. The
round's flow and what the cards say stay the platform's.

```toml
identity = "saloon"                                   # one of the 17
identity = "haunted carnival, crimson, jittery"       # or words, which pick one and change it

[identity]                                            # or a table, with overrides
name = "storybook"                                    # or describe = "…"
colors = "art"                                        # the key art's colours (art = "dusk_0", else the biggest picture)
accent = 0xd4312a
title_font = "Rye"                                    # library families (mb fonts); body_font, number_font too
hud = "badge"                                         # center | corner | badge
results = "receipt"                                   # panel | floating | receipt | scoreboard | stamp | editorial
title = "marquee"                                     # drop | stack | banner | stamp | typewriter | marquee | arc | neon | plate
```

- **The 17:** `riso`, `crt`, `neon`, `storybook`, `saloon`, `swiss`,
  `brutalist`, `comic`, `gothic`, `bubblegum`, `terminal`, `field`,
  `orbital`, `gala`, `notebook`, `stadium`, `cozy-pixel`. Without an
  identity a game is `greybox` (grey, the host sans, plain): the starters'
  placeholder, to be replaced. Two more keep engine 1.0's look for a game
  moved up ([upgrade]): `arcade` (the Arcade preset's default) and `night`
  (1.0's 3D cards).
- **Fonts** are baked by `mb build`: the identity's own and any it names.
  `love.graphics.print` with no `setFont` uses the identity's body font.
- **`colors = "art"`:** `mb build` samples the key art into the backdrop,
  panels and accent, with text that reads on them; the same picture always
  gives the same colours.

| | |
|---|---|
| `ui.hud{ score, best, lives, timer, meter, combo, level }` | What the HUD shows; it merges, so pass what changed (`ui.hud{ meter = 0.4 }` each frame is fine). Each is `true`, `false`, a value, or one of the game's nodes |
| `ui.identity(name or words)` | Another identity from here on (a new chapter, a boss): the HUD, the game-over card and the widgets change |

- **The default HUD:** the score, when `studio.toml` names a board
  (`[[scores]]`; without one, as a LÖVE game brought in has, the game's
  own HUD is the only one); the lives when `studio.toml` says
  `lives` above 1; the level's name when there's more than one map.
- **The elements:** `score` and `best` (shown until it's beaten); `lives`
  (hearts); `timer = 90` counts down from 90 s by `round.time` (what
  happens at 0 is the game's); `meter = 0.6`, a bar; `combo = 4` shows ×4;
  `level = true` the map's name (or a string or number of the game's).
- **The game's own node** stands in for an element: `ui.hud{ score =
  rosette }` writes the score into its `Text` named `value` (or into it,
  if it's a `Text`). `ui.hud{ score = false }` and the game draws its own.

<!-- names: ui.hud ui.identity -->

## The UI kit: widgets, hints and lessons [ui]

Buttons, panels and meters, hints that point at things, lessons that
teach a move, the countdown, the between-rounds stats card and the combo
meter. All of them are drawn in the game's identity ([identity]): a pixel
identity's bar is stepped blocks, a print one's sits on a hard shadow,
neon's glows. A game never draws its own button.

```lua
local shop = ui.panel{ x = 180, y = 300, w = 300, h = 260 }
ui.pill{ text = "WAVE 3", icon = "flag", x = 180, y = 190, tone = "accent" }
local hp = ui.bar{ x = 180, y = 240, w = 240, value = 1, tone = "good" }
ui.button{ text = "BUY", icon = "plus", x = 120, y = 400, tap = function() buy() end }
ui.button{ text = "SKIP", kind = "secondary", x = 250, y = 400, tap = nextWave }
Text{ text = "1,240", x = 70, y = 220, color = ui.color("text") }   -- reads on the panel
hp.value = 0.4                                                       -- later: it eases there
```

**Widgets** are nodes ([scene]): `x`, `y` (their centre), `alpha`,
`visible`, `parent`, `order`, `destroy()` and the rest work, and they're
drawn in made order like any node (make a panel before what goes on it).
They're fixed to the screen unless `fixed = false`.

| | |
|---|---|
| `ui.button{ text, icon, x, y, w, h, kind, tone, tap, enabled, focus }` | A button (160×56 unless `w`, `h` say; an icon alone is round, 60 across). `kind` is `"primary"` (the default), `"secondary"` or `"ghost"`; `tone = "good"`, `"bad"` or `"gold"` makes it the identity's green, red or gold (a right or wrong answer). `tap()` runs when a finger lifts on it |
| `ui.panel{ x, y, w, h, tone }` | A card in the identity's frame (240×160) |
| `ui.pill{ text, icon, x, y, h, tone }` | A label; its width is its text's |
| `ui.bar{ x, y, w, h, value, tone }` | A meter filled to `value` (0–1), 160×14 |
| `ui.ring{ x, y, r, value, text, icon, tone }` | A round meter, `r` 40, with words or an icon in the middle |
| `ui.icon(name, { x, y, size, tone })` | One of the icons: `play`, `pause`, `restart`, `star`, `heart`, `trophy`, `crown`, `lock`, `unlock`, `check`, `close`, `plus`, `minus`, `up`, `down`, `left`, `right`, `home`, `gear`, `sound`, `mute`, `music`, `coin`, `bolt`, `flag`, `clock`, `medal`, `fire`, `gem`, `shield`, `target`, `person`, `bomb` |
| `ui.color(name)` | The identity's `"text"`, `"dim"`, `"accent"`, `"onAccent"`, `"panel"`, `"good"`, `"bad"`, `"gold"`, `"outline"` or `"background"`, as `0xRRGGBB` |

A widget's own properties: `text`, `value` (bars and rings), `icon`,
`tone` (`"plain"`, `"accent"`, `"good"`, `"bad"`, `"gold"`), and a
button's `kind`, `enabled`, `focus` and `tap`; `w` and `h` read back its
drawn size. `color` tints it (white leaves the identity's colours).

**Pressing a button.** Its press is the button's: `love.mousepressed`
and `love.touchpressed` don't hear it, and a drag off it before lifting
cancels. It takes a thumb within 44 units of its middle however small it
looks. Enter or a controller's A presses the focused button (`focus =
true`, else the first made; the arrows and d-pad move the focus, and the
game still hears them), and only a player using keys or a pad sees the
focus ring. While a button is on screen Enter and A are its, so a game
that jumps on A shows buttons between rounds of play (a shop, a choice),
not during them. The press that starts a round never presses a button.

**Hints** say what to do, by what it's done to:

```lua
local h = ui.hint{ text = "TAP THE BELL", at = bell }   -- follows the bell as it moves
ui.hint{ text = "HOLD", at = rope, priority = "threat" } -- a threat beats a teaching hint
h:dismiss()
```

| | |
|---|---|
| `ui.hint{ text, at, priority, ask, linger }` | A hint, until `hint:dismiss()` or its `at` is destroyed. `at` is a node (2D or 3D, through the camera) or `{ x, y }`; without one it sits low in the middle. `priority` is `"ambient"`, `"teach"` (the default) or `"threat"`. Five words at most, a verb first |
| `hint:showing()` | It's on screen now |

- **One at a time.** The most important shows; one outranked waits and
  comes back when the other goes. Equal ones take turns every 2.5 s.
  The same words again move and change that hint rather than add one.
- **What it asks** is read off its first words: "TAP …" (or press, hit),
  "HOLD …" (keep holding), "LET GO" (release), anything else asks nothing
  of a finger. `ask = "tap" | "hold" | "let go" | "look"` says it outright.
  The difficulty bot does what they ask ([bot]).

**Lessons** teach a move the first time it's needed, with a moment of slow
motion that always ends:

```lua
if gap.x < 200 then
  ui.lesson{ prompt = "TAP TO JUMP", at = fox, slow = 0.3, learned = function() return fox.data.jumped end }
end
```

| | |
|---|---|
| `ui.lesson{ prompt, at, slow, hold, learned }` | Teaches now: the prompt shows (as a teaching hint at `at`) and the world slows to `slow` (0.3; 1 is none) for `hold` seconds (2.5, never more than 3) whether or not the player acts. The prompt stays until `learned()` returns true or `lesson:learned()` |
| `lesson:learned()`, `lesson:showing()` | The move was made; the prompt is up |

Slow motion scales `dt` (`love.update`'s, motion, timers and tweens);
`round.time` runs on unscaled ([rounds]), so a game that times its world
by `round.time` won't slow: time it by its own `clock = clock + dt`.

Once a round: asking again for the same prompt is the same lesson, so a
game can call `ui.lesson` every frame its moment lasts. Each round is a
fresh VM, so a returning player is taught again, and makes the move in a
blink.

**The rest:**

| | |
|---|---|
| `ui.countdown(n, fn)` | "3, 2, 1, GO!" (from 1 to 10) over the round, then `fn()`. The world keeps going: hold it yourself until `fn` runs |
| `ui.stats{ title, rows, rank, next, wipe, tap, done }` | The between-rounds stats card: `rows = { { "Coins", 12 }, { "Accuracy", 0.87, "percent" }, { "Time", 42.3, "time" }, { "Hits", 2, bad = true } }` (6 at most) roll up one by one; `rank` is a letter, a 0–1 score, or `true` (from the percent rows); `next = { "THE WARDEN", "he doesn't miss" }` teases what's next; `wipe` is `"iris"`, `"slide"` or `"shutter"`. The world holds while it's up; a tap shows it all, and it moves on by itself after 2.5 s (`tap = true` waits for a tap). `done()` runs once the wipe covers the screen: set up what's next there |
| `ui.combo{ window, hud }` | A combo meter: `combo:hit()` → the count, `combo:reset()` (a miss), `.count`, `.best`, `.multiplier` (1, 2 at 5, 3 at 10…). It breaks after `window` seconds (2) without a hit. The HUD shows the last one made, `×count` with its time draining (`hud = false` keeps it off) |

<!-- names: ui.button ui.panel ui.pill ui.bar ui.ring ui.icon ui.color ui.hint ui.lesson ui.countdown ui.stats ui.combo Widget.text Widget.value Widget.icon Widget.tone Widget.kind Widget.enabled Widget.focus Widget.tap Widget.w Widget.h Hint:dismiss Hint:showing Lesson:learned Lesson:showing Combo:hit Combo:reset Combo.count Combo.best Combo.multiplier -->

## Saves and progress [saves]

| | |
|---|---|
| `save.get(key)` | The value saved as `key`, or nil |
| `save.set(key, value)` | Saves it (nil deletes): numbers, strings, true/false and plain tables, 256 KB in all; keys up to 57 characters |
| `progress.get(key)`, `progress.set(key, value)` | Unlocks and story flags, which the server checks by replaying the run (rolled back if it doesn't replay the same); 64 KB; keys up to 55 characters |
| `progress.checkpoint(state)` | Where a long run is, to pick up in a later session |
| `progress.resume()` | That state, or nil |
| `progress.clear()` | When the run ends |

- A table is saved with its keys sorted, so the same table is always the
  same bytes. Tables nest up to 32 deep; functions and nodes can't be
  saved.
- Every NaN is saved as one NaN (its sign and payload bits differ between
  CPUs, and progress has to be the same bytes on the phone and in the
  server's replay).
- A debug session plays on a copy of the saves and progress.
- `mb build` declares the `progress` capability when the game's Lua uses
  `progress.`.
- **How progress is verified.** The phone uses what a round sets at once.
  After the round, the server plays the session's run again from the
  progress it started with and keeps what that ends with; a run that
  doesn't play the same (it was tampered with, or it started from progress
  the server never verified) is thrown out, and the player's progress goes
  back to the last run that did. So set progress only from what happened
  in play, never from the clock or anything outside the run.
- **Checkpoints.** A player who leaves mid-round is caught up when they
  come back, for up to 15 minutes of play. A longer run (a dungeon of
  floors, a story chapter) calls `progress.checkpoint(state)` at its safe
  points, reads `progress.resume()` in `love.load` to carry on in a later
  session, and calls `progress.clear()` when the run ends. A checkpoint is
  progress, verified like the rest. In a daily round, leave unlocks out of
  the course, so every player plays the same one.

<!-- names: save.get save.set progress.get progress.set progress.checkpoint progress.resume progress.clear -->

## Debug [debug]

Every game has a debug menu, on the platform's debug pill (the creator,
moderators and testers on a build that isn't live). Nothing played in it
counts, and it plays on a copy of the saves.

| | |
|---|---|
| `debug.action(label, fn)` | A button: `fn()` |
| `debug.toggle(label, fn, on)` | `fn(on)`; `on` is where it starts |
| `debug.number(label, value, fn, min, max, step)` | A slider: `fn(value)` |
| `debug.choice(label, options, fn, index)` | `fn(option, index)` |
| `debug.invincible()` | True while the menu's Invincible is on: a game that takes lives itself checks it (`hurt` does) |
| `debug.mark(label)` | This frame does something sudden on purpose (`"cut"`, `"respawn"`): the smoothness check doesn't count a jump within two frames of it |

Declare items at `main.lua`'s top level or in `love.load`. The engine adds
its own tabs:

- **Scenarios** from `debug/scenarios.toml`: named sets of picks.
  ```toml
  [[scenario]]
  name = "Gold rush"
  note = "a full bank, at the boss"
  level = "boss"                 # a map: the round starts there (nextLevel)
  score = 38
  lives = 1
  invincible = true
  spawn = ["coin", "coin"]
  [scenario.knobs]
  "player.speed" = 300           # kind.knob
  [scenario.set]
  gold = true                    # globals, set once love.load has run
  ```
- **Studio:** jump to each map, Invincible, Score, Lives, Show hitboxes,
  and Restart round during one.
- **Spawn:** each kind, next to the player.
- **Knobs:** each kind's behaviours' knobs, live.
- The game's own items, under its title.

A scenario, a level jump or Restart round starts the round again, in
place, with every pick so far.

<!-- names: debug.toggle debug.number debug.choice debug.action debug.invincible debug.mark -->

## The difficulty bot [bot]

The `difficulty` tool plays the game on the server, many seeded rounds per
kind of player (first-timer, casual, good, expert), and reports how long
rounds last, how they end and whether skill matters. A bot plays: the
game's `game/bot.lua`, or without one, the quick path's default for its
player's behaviour (`player.flappy` flaps at the gaps, `player.runner`
jumps hazards and gaps, `player.platformer` walks the level and jumps,
`player.drag`, `player.paddle` and `player.tilt` go for pickups and away
from hazards). Write `bot.lua` when the default plays wrong, or for a game
without behaviours.

```lua
-- game/bot.lua: hold ahead to run, jump at gaps and thorns
function bot(act)
  local fox, map = scene.all("fox")[1], scene.find("path1")
  if not fox then return end
  local w, h = love.graphics.getDimensions()
  act.hold(w * 0.85, h * 0.6)                  -- held beside the fox, the platformer walks that way
  local ahead = fox.x + 48
  if not map:solidAt(ahead, fox.y + 24) or map:tileAt(ahead, fox.y) == "thorns" then
    act.swipe(-math.pi / 2, h * 0.2, { x = w * 0.85, y = h * 0.6 })   -- the thumb flicked up jumps
  end
end
```

`bot.lua` is loaded only in bot runs, after `main.lua` and `love.load`, and
`bot(act)` is called after each `love.update`. It sees what a player sees:
the scene (by tag and name) and the game's globals, never `main.lua`'s
locals, so tag what it needs (`Sprite{ …, tag = "wall" }`). It never moves
anything itself: it says what a perfect player would press, where and
when, and the engine adds the person by skill: a reaction (from 0.15 s,
about 0.4 s for a first-timer) and now and then a look away on a new
situation, the time a thumb takes to get there and where it lands (Fitts'
law, by the target's `size`), and timing noise. One finger: a new press
lifts it first, and a press waits while it's busy.

| | |
|---|---|
| `act.tap(x, y, { size, at, hold, new })` | A press at (x, y), canvas units as `love.mouse` gives them, on a target `size` across (60); ideally `at` s from now (0), held `hold` s (0.06). Asked again every frame for the same moment, it's one press |
| `act.hold(x, y, { size, new })` | Keep the finger down there: it presses, then steers after it as a thumb does: a jump of more than `size` (40) is a quick drag there (Fitts' law), and under that it follows continuously, a little behind and wandering by a few units, more for a first-timer than an expert. `new = true` costs a reaction (once, however often it's asked). Ask every frame with where it's wanted now. Held until `act.release()`; after a tap, the finger comes back down here rather than lifting |
| `act.release()` | Lift the finger |
| `act.swipe(angle, length, { x, y, at, new })` | A flick (radians, 0 is right, y down), from (x, y) or where the finger is |
| `act.press(button, { at, hold, new })` | A controller button: `"a"`, `"b"`, `"x"`, `"y"`, `"leftshoulder"`, `"rightshoulder"`, `"dpup"`, `"dpdown"`, `"dpleft"`, `"dpright"` |
| `act.stick(ax, ay, { new })` | The left stick, -1 to 1 each |
| `act.finger()` → x, y, down | Where the finger is |
| `act.hints()` | The hints on screen ([ui]), the most important first: `{ text, ask, priority, x, y, age }` each (`x`, `y` where it points, if it does; `age` seconds up) |
| `act.followHints()` | Also do what they say this frame, as the bot does by itself (below) |
| `act.preset` | The kind of player this run is (`"first-timer"`, `"casual"`, `"good"`, `"expert"`), for skill the engine can't add: how much of a quiz it knows, how fast it reads |

`new = false` is the next beat of a rhythm the player is already in (the
next flap): no reaction and no look away, only timing noise. Everything else
is new by default. Times are in game seconds: a policy that reads speeds
should read the game's own (a world sped up by `speedUp` moves faster).

**Hints.** A game with no `bot.lua` and no quick-path player is played
from its hints and lessons' prompts ([ui]), as a player who does exactly
what they say: once a hint has been up for that preset's reaction time, it
taps what a TAP hint points at every 0.3 s while it shows, holds on HOLD
until a LET GO, and does nothing else. So a game that teaches through
hints gets a bot for free, and one whose hints ask for something that
hurts shows it in the report.

**The report.** Each preset's round length (p10, p50, p90) against its
target (first-timer 20–45 s, good 60–180 s by default; `target` sets
others), each round capped at its preset's cap (the target's top; casual
90 s, expert 180 s); how rounds end, by `round.over`'s `cause` (or the
hazard that took the last life, or "time" at the cap), and the win rate
for a game that can be won (`round.over(heading, cause, true)`); whether
skill matters, each preset doing better than the one below it (more wins,
a longer median, or more rounds reaching the shorter cap; "can't tell"
when both sit at the cap); and the pacing of what the game's handlers hear
(pickups, hurts, overlaps starting, things leaving), with any that feel
constant. A preset stops early once its answer is clear (20 rounds at most,
40 for a final check). The report is saved as `debug/bot-report.tsv`, and
the next run says what changed.

**Levels.** A game played to a win (light every lantern, reach the flag)
is judged on how often each kind of player wins instead: first-timer
15–40%, casual 30–60%, good 60–85%, expert 85% or more, and how long the
wins take. Every preset then plays up to 180 s, so a good player's winning
run has to fit in it ("too long" when runs reach that before they win). A
game is a level when `studio.toml` says `level = true`, or when most of the
expert's rounds are won. Tune a level by what it asks of the player
(thorns' width and place, gaps, the time limit, what there is to collect),
not by the bot. A game whose logic measures text plays in
the phone's own runtime on the server; others may play natively, checked
against it.

## The quick path [quick]

Engine 1.0's things and behaviours on 2.0's nodes, run in Rust before
`love.update`. A *thing* is a node tagged with its kind: a `Sprite` of its
image, or the kind's box.

```toml
# images/cat.toml
frame = [32, 32]
behaviour = "player.drag"           # or behaviours = ["enemy.patrol", "hazard"]
hitbox = [4, 6, 24, 22]             # in the frame's pixels (else its whole box)
spawns = "fish"                     # what a spawner or a shooter makes
[knobs]
speed = 300
```

| | |
|---|---|
| `kind(name, { w, h, color, shape, image, behaviour, behaviours, knobs, hitbox, spawns, order })` | A kind without an image (a `w` × `h` box, `shape = "circle"`), or changes to one |
| `spawn(kind, props)` | A thing, with any node properties; returns it |
| `spawnEvery(kind, secs, props)` | One every `secs` (props are numbers); returns a handle with `:stop()` |
| `all(kind)`, `count(kind)`, `nearest(kind, x, y)` | Nodes with that tag (`all()`: every thing) |
| `hurt(node)` | One life off `round.lives` and a blink of safety; false while it blinks or Invincible is on |
| `speedUp(f)` | The world (not the players) goes `f` times faster: things and spawners |
| `nextLevel(name)` | Replaces the level's map and its things (the next map in name order without a name; past the last, the round ends, won) |
| `node.behaviour`, `node.behaviours`, `node.knobs` | On any node but a `Tilemap`: `Sprite{ image = "fox", behaviour = "player.platformer", knobs = { speed = 200 } }` |
| `round.level` | The level's map name (`""` before there's one) |

**Behaviours and their knobs** (1.0's numbers, in 1.0's art pixels, times
2.5 units):

| Behaviour | Does | Knobs |
|---|---|---|
| `player.drag` | Follows the finger (where it came down), or the arrows, WASD, d-pad, stick | `speed`, `x`, `y` (0 locks that axis) |
| `player.paddle` | The same, across only | `speed` |
| `player.tilt` | The same, and tipping the phone steers it while no finger drags it (`mb build` adds `tilt` to `sensors`) | `speed` |
| `player.flappy` | A tap (any finger), Space, Up or A flaps | `gravity`, `flap`, `max_fall` |
| `player.runner` | A tap (any finger) jumps; hold for higher | `gravity`, `jump`, `speed`, `jumps` |
| `player.platformer` | A finger held beside it, at any height, walks it that way (held over it, it stands; lifted, it eases to a stop, and a jump keeps its way until it lands). A second finger's tap, the walking thumb flicked up, or a press on it jumps, the moment it happens; a tap gets the whole jump. The arrows walk; Space or A jumps | `gravity`, `jump`, `speed`, `jumps` |
| `enemy.patrol` | Walks, turning at walls and ledges | `speed`, `gravity`, `ledges` |
| `enemy.chase` | Goes for the player in `sight` | `speed`, `sight`, `gravity` |
| `enemy.shooter` | Fires its `spawns` | `every`, `speed`, `aim` |
| `bullet` | Flies; gone at a wall | `speed` |
| `pickup` | A player touching it scores `value` and takes it | `value` |
| `hazard` | Hurts a player touching it | |
| `solid` | Blocks things that collide, as solid tiles do | |
| `spawner` | Makes its `spawns` | `every`, `spread`, `vx`, `vy` |

Players, platformers, runners, enemies and bullets collide with the
level's tiles (the last `Tilemap` made), and a hazard tile hurts a player.
A platformer moves on `CharacterBody2D`'s controller ([physics]): it
lands on a one-way tile (`oneway = true`, a ledge) from above and jumps up
through it from below. A map bigger than the view makes the camera follow the first player,
unless the game moves the camera. A map's legend places things
(`{ thing = "lantern" }`) or spawners (`{ thing = "fish", every = 1.2,
spread = 48 }`).

**Events:** `on(event, …)`; handlers run in the order they were added.

| | |
|---|---|
| `on("overlap", tagA, tagB, fn(a, b))` | Tagged boxes overlap, each frame they do, in the tags' order (`"*"` is any) |
| `on("tick", fn(dt))`, `on("tick", kind, fn(thing, dt))` | Each frame, before `love.update` |
| `on("leave", kind, fn(thing))` | A thing left the level (it's destroyed after). A player leaving with no handler ends the round |
| `on("tap", fn(x, y))`, `on("swipe", fn(dx, dy))` | A press; a quick drag |
| `on("start", fn())` | After `love.load` |
| `on("flap" \| "jump" \| "land" \| "hurt" \| "pickup" \| "shoot", fn(thing, other))` | What behaviours do: `thing` did it (or was hurt), `other` is what it touched. A sound named for the event plays by itself |

**Names from elsewhere.** A name from engine 1.0, Roblox or LÖVE that 2.0
calls something else is an error saying what to use: `score` is
`round.score`, `game_over` is `round.over`, `rnd(n)` is
`love.math.random() * n`, `Instance.new` is a node constructor,
`love.filesystem` is `save`, `on("touch")` is `on("overlap")`.

<!-- names: on hurt kind spawn spawnEvery all count nearest nextLevel speedUp
Sprite.behaviour Sprite.behaviours Sprite.knobs Shape.behaviour Shape.behaviours Shape.knobs Text.behaviour Text.behaviours Text.knobs Layer.behaviour Layer.behaviours Layer.knobs Group.behaviour Group.behaviours Group.knobs Walker.behaviour Walker.behaviours Walker.knobs Emitter.behaviour Emitter.behaviours Emitter.knobs round.level -->

## 3D: building blocks [3d]

A 3D game (`kind = "3d"` in `studio.toml`) plays on engine 2.0's 3D build:
a world of meshes, models and lights that mb3d draws, with everything 2D
on top of it (the nodes, `love.draw`, the HUD in the game's identity, the
cards). It has 2.0's whole API, three.js's names for what the world is
made of (`Mesh`, `geometry.box`, `material{}`), Godot's for how it moves
and animates ([models]), and engine 1.0's names beside them ([3d1]).

The world is in meters: right-handed, +y up. **A node's front is −z**, as
a camera looks: `node:faceDirection(dx, dz)` and `node:lookAt(x, y, z)`
turn it; nobody works out an angle. Turns are radians, applied y (yaw),
then x (pitch), then z (roll). Colours are 2.0's: `0xRRGGBB`, or
`{ r, g, b }` from 0 to 1, as picked; they read back as `0xRRGGBB`.

```lua
local crate = Mesh{ geometry = geometry.box(1, 1, 1),
                    material = material{ color = 0xb07a45, roughness = 0.8 }, y = 0.5 }
local lamp = Light{ parent = crate, color = 0xffc070, intensity = 3, range = 6, y = 1 }
flux.to(crate, 2, { ry = math.pi }):ease("sineinout"):yoyo()
```

**Nodes.** Every 3D node has `x`, `y`, `z`, `rx`, `ry`, `rz`, `scale`
(`sx`, `sy`, `sz`), `vx`, `vy`, `vz` and `gravity` (moved in Rust;
positive falls), `vrx`, `vry`, `vrz` (a spin in radians a second, turned
in Rust: `coin.vry = 3`), `visible`, `parent` (setting it keeps the node where it
is; a constructor's `parent` puts it there in the parent's space),
`children`, `name`, `tag`, `data` and `alive`; `fade = { from, to }`
dithers it out between those distances from the camera; `wake = true`
pushes a wake along the water as it moves ([world]). Its methods:
`node:faceDirection(dx, dz)`, `node:lookAt(x, y, z)` (yaw and pitch),
`node:worldPosition()` → x, y, z, `node:instance(list, stride)`,
`node:find(name)`, `node:destroy()`, `node:createTween()`. A write
reaches the GPU once a frame, however many writes there were.

| Node | Its own |
|---|---|
| `Mesh{ geometry, material }` | Draws a geometry with a material (the default grey without one); one Mesh is one draw. Both can change later |
| `Group{}` | An empty node to move things together. In a 3D game `Group{}` and `Emitter{}` are 3D unless their `parent` is a 2D node |
| `Light{ color, intensity, range }` | A point light at the node (8 at most; the sun doesn't count) |
| `Emitter{ preset, rate, colors, sizes, life, speed, spread, drag, gravity, additive, emitting }` | Particles at the node, simulated by the host: `preset` as 2D's (`sparkles`, `confetti`, `stars`, `burst`, `sparks`, `ring`, `dust`, `smoke`); `rate` a second; `e:emit(n, { dx, dy, dz })` a burst along a direction; `sizes` meters, `life` and `speed` `{ min, max }`, `gravity` pulls the particles down (m/s²) |
| `Trail{ color, width, length }` | A ribbon behind the node as it moves (`length` in seconds); `trail.parent = ship` |
| `Text{ text, size, color, billboard = true, font }` | Words in the world facing the camera, `size` in meters. A `Text` without `billboard` (or a 3D parent) is the 2D HUD's |

**Geometry**, built in code. Each edit changes it and returns it, so they
chain; a Mesh draws it as it was when the Mesh was made; one geometry and
one material can be shared by many Meshes.

| Call | |
|---|---|
| `geometry.box(w, h, d)`, `.sphere(r, segs, rings)`, `.ico(r, n)`, `.cylinder(r, h, segs)`, `.cone(r, h, segs)`, `.plane(w, d)`, `.torus(R, r, segs, sides)` | Centred shapes (a plane lies flat, facing +y) |
| `geometry.rock(size, seed)` | A jagged, flat-shaded boulder `size` meters across (1), a fifth of it below its origin so it sits on the ground; the same `seed` (1) makes the same rock. A few seeds scattered make a rocky field: `scatter(Mesh{ geometry = geometry.rock(1, 3):paint(0x7a7268) }, { on = ground, … })` |
| `geometry.lathe({ r1, y1, r2, y2, … }, segs)`, `.extrude({ x1, y1, … }, depth)`, `.tris({ x, y, z, … })` | Turned, pushed and raw shapes |
| `geometry.loft{ points = { x, y, z, … }, sides, rings, radius, open, caps }` | A tube along a smooth curve through the points; `radius` a number or `function(t, u)` (t along it, u around it, 0–1): horns, tails, tentacles; `open = true` a ribbon |
| `geometry.fish{ length, height, fins }` | A fish, body, fins and eyes |
| `g:merge(other, x, y, z, rx, ry, rz, s)`, `g:paint(color)`, `g:smooth(crease)`, `g:subdivide(n)` | Another geometry placed into this one; every vertex one colour; smooth normals; rounder (each step ×4 triangles). 1.0's edits work too ([3d1]) |

**Materials:** `material{ color, emissive, emissiveIntensity, metalness,
roughness, texture, unlit, doubleSided, transparent, opacity, additive,
toon, rim, outline, ink, fog }`, three.js's names: `toon` is light bands
(2–8), `outline` pixels and `ink` its colour, `rim` an edge light (0–1),
`fog = false` keeps it out of the fog, `texture` an image's name (512 px a
side or more, else the packer put it in an atlas). 1.0's keys (`metal`,
`rough`, `glow`, `style`) still work; a table with only `color` is 1.0's
when the number is past `0xffffff` (`0xRRGGBBAA`).

**Many copies in one draw.** `node:instance(list, stride)` draws its
geometry at each `x, y, z` of the flat list, then by `stride` `yaw, scale,
color, pitch, roll, tall`. `scatter(node, { count, area, seed, scale = {
min, max }, tint = { colorA, colorB }, turn })` lays them out in Rust, the
same for the same seed: `area` is a square's side in meters around the
node, or `{ w, d }`; `on = ground` stands each copy on a `Terrain`
([terrain]). A copy's colour multiplies the material's.

**The camera.** In a 3D game `camera` is the 3D one: `camera.x`, `.y`,
`.z`, `camera.fov` (vertical, **degrees**, 50–70 looks natural),
`camera.roll` (radians), `camera.near`, `camera.far`;
`camera:lookAt(x, y, z)`; `camera:project(x, y, z)` → sx, sy, onScreen
(canvas units); `camera:ray(sx, sy)` → ox, oy, oz, dx, dy, dz (a tap into
the world). `camera:follow(node, { behind = 5, above = 2, lag = 0.25,
lookAhead = 1, bank = 0 })` keeps behind the node's front, in Rust, after
everything moved; `bank` rolls into its turns; over a `Terrain` it keeps
above the ground and rises over (or comes in front of) a hill between it
and the node ([terrain]). Writing `camera.x`, `.y`,
`.z` or calling `lookAt` stops following. `fx.shake` shakes it (meters).

**Effects in 3D:** `fx.burst(preset, x, y, z, { count, color })`,
`fx.popup(text, x, y, z, { color, size })` (meters), `shockwave(x, y, z,
radius, strength, secs)`.

**Budgets** (the phone): about 200 draws and 150k triangles a frame;
4,096 nodes; 8 lights. Instances and `scatter` are one draw.

<!-- names: Mesh Light Trail Mesh.geometry Mesh.material Light.color Light.intensity Light.range Trail.color Trail.width Trail.length
Emitter3D.preset Emitter3D.rate Emitter3D.colors Emitter3D.sizes Emitter3D.life Emitter3D.speed Emitter3D.spread Emitter3D.drag Emitter3D.gravity Emitter3D.additive Emitter3D.emitting
Text3D.text Text3D.size Text3D.color Text3D.billboard Text3D.font
Node3D.name Node3D.tag Node3D.fade Node3D.wake Node3D:faceDirection Node3D:lookAt Node3D:worldPosition Node3D:instance
geometry.box geometry.sphere geometry.ico geometry.rock geometry.cylinder geometry.cone geometry.plane geometry.torus geometry.lathe geometry.extrude geometry.tris geometry.loft geometry.fish
Geometry:merge Geometry:paint Geometry:smooth Geometry:subdivide scatter shockwave
camera.z camera.fov camera.roll camera.near camera.far camera:project camera:ray -->

## Models, animation and IK [models]

A model is `models/<name>.glb`; **models face −z** (generated and imported
ones are turned to when they land), so `faceDirection` and `lookAt` point
them.

```lua
local fox = Model{ model = "fox", scale = 1.2 }
fox:play("Run", { fade = 0.15 })                        -- one clip; { once = true } plays once
fox:play("Wave", { layer = "upper", mask = "spine" })   -- the upper body only, over Run
local hand = fox:bone("hand_r")                         -- a node to hang things on; nil if the rig has none
Model{ model = "lantern", parent = hand, scale = 0.35 }
fox:reach("hand_l", x, y, z)                            -- two-bone IK to a point (reach("hand_l", nil) lets go)
fox:headLook(x, y, z, { limit = 0.8 })                  -- only the head turns (headLook(nil) lets go)
fox.plantFeet = true                                    -- feet don't sink into the ground (IK)
```

| | |
|---|---|
| `Model{ model }` | Spawns `models/<model>.glb` (loaded once a session, behind the title card) |
| `m:play(clip, { fade, once, speed, restart, layer, mask })` | Crossfades to a clip (clips are capitalised: `"Idle"`, `"Run"`). A looping clip whose hips end the cycle somewhere else than they start (a generated walk sinking through its cycle) is closed, the drift taken out in step, so it never jumps at the loop. With `layer`, the clip plays on a layer of its own over the rest, on the bones from `mask` down (a bone's name or a list; a new layer needs one), fading in and, for a `once` clip, out at its end; `m:play(nil, { layer })` fades a layer out |
| `m.clip`, `m.clipTime` | The clip playing most (the tree's heaviest), and how far in |
| `m.animationFinished` | A signal: a `once` clip reached its end |
| `m:bone(name)` | A bone by its standard name, else the rig's own: `hips`, `spine`, `chest`, `neck`, `head`, `shoulder_l`, `upper_arm_l`, `forearm_l`, `hand_l`, `thigh_l`, `shin_l`, `foot_l`, `toe_l` and the same with `_r`. A node made under it hangs from where it is now, turned as the model and at size 1 (meters), and follows it. `m:find(name)` is the rig's own names (`"Socket.Tail"`) |
| `m:reach(bone, x, y, z)` | A hand or foot to a point, every frame until `m:reach(bone, nil)` |
| `m:headLook(x, y, z, { limit })` | The head turns toward the point within `limit` radians |
| `m.plantFeet = true` | After the clips, a foot that would go below the model's floor is lifted onto it. Over a `Terrain` each foot stands on the ground under it, the body lowered for the downhill one ([terrain]); a `gait`'s feet land on it too |
| `m.gait = "mech"` | A heavy biped's walk from how fast the node moves: feet planted in the world, a dip at each footfall, the hips rolling; `m.stepped` is a signal (foot, x, y, z, strength) for dust and stomps |
| `m.flight = { flap, bank, size }` | A `spine` rig (`spine_0`, `spine_1`, …, wings) flaps, glides, banks and pitches by its velocity |
| `m.swim = { length, amplitude, vertical }` | A `spine` rig swims: a wave down its body, beating with its speed |
| `m:restyle{ toon, outline, rim, ink, fog }` | Every material the model came with (all its copies) |

Gaits, flight and swims take the node's velocity from how it moved (its
`vx`, a tween, writes alike). Without the bones they need they do nothing
and say so in the log. Clips, layers, trees and IK run in Rust after
`love.update`, so a playing model costs no Lua.

**An `AnimationTree`** (Godot's) runs a model's clips from parameters:

```lua
local tree = AnimationTree{ model = fox, start = "move",
  states = {
    move = { blend = { Idle = 0, Walk = 2, Run = 6 }, by = "speed" },   -- a 1D blend space
    aim = { blend = { Idle = { 0, 0 }, Left = { -1, 0 }, Right = { 1, 0 }, Up = { 0, 1 } }, by = { "x", "y" } },   -- 2D
    jump = { clip = "Jump", once = true },
    land = { clip = "Land", once = true, next = "move" },               -- moves on at its end
  },
  transitions = {
    { from = "move", to = "jump", when = "airborne", fade = 0.1 },
    { from = "jump", to = "land", when = "onFloor", fade = 0.05 },
  },
}
tree:set("speed", 3.2); tree:set("airborne", true)
```

| | |
|---|---|
| `tree:set(parameter, value)`, `tree:get(parameter)` | A number (a blend's `by`) or true/false (a transition's `when`) |
| `tree:travel(state)` | Goes there, crossfading with the fade of a transition into it (else 0.2 s) |
| `tree.state`, `tree.model` | The state now; the model it runs |

A blend space's clips play in step (a walk and a run keep their feet
together); a 2D one weighs the triangle of points the value falls in.
Transitions are checked in their order, the first whose `when` holds
winning; `from` left out (or `"*"`) is any state. A tree takes over the
model's `play`; layers still play over it.

<!-- names: Model Model.model Model.clip Model.clipTime Model.animationFinished Model.stepped Model:play Model:bone Model:reach Model:headLook Model:restyle Model.plantFeet Model.gait Model.flight Model.swim
AnimationTree AnimationTree:set AnimationTree:get AnimationTree:travel AnimationTree.state AnimationTree.model -->

## The world's look [world]

Each call sets the look until it's called again; each takes a preset's
name or a table. Colours are 2.0's.

| Call | |
|---|---|
| `world.sky{ atmosphere, clouds, disc, panorama, stars }` | Sky v2, which also lights the scene: `atmosphere` `"day"`, `"dusk"`, `"night"`; `clouds = { cover = 0.4 }` (or `true`, or a preset's name); `disc` `"sun"` or `"moon"`; `panorama` an image painted around the world; `stars` 0–4 |
| `world.sun{ dir = { x, y, z }, color, intensity, shadows = true, shadowDistance }` | The one shadowed light; `dir` is the way the light travels |
| `world.lighting("hero")` | Sun, sky light and exposure that make a textured hero read |
| `world.fog{ color, mode = "exp2", density, start, ["end"], height, falloff, sky }` | `mode` `"exp"`, `"exp2"`, `"linear"`, `"off"` |
| `world.post{ exposure, bloom, bloomThreshold, vignette, aberration, saturation, contrast, sharpen }`, `world.dof{ focus, range, blur }` (`world.dof()` off), `world.speedBlur{ amount, x, y }` | The final image; depth of field; a zoom blur toward a canvas point |
| `world.grade("moonlit", amount)`, `world.shafts("moonbeams")`, `world.halos("mist")`, `world.film("subtle")` | The beauty pass: `grade` `moonlit`, `goldenHour`, `neon`; `shafts` `moonbeams`, `sunbeams`; `halos` `mist`; `film` `subtle`; `"off"` for none |
| `world.water{ preset = "lake", height = 0, size = 200, flow = { x, z, speed } }` | A reflecting, rippling water plane: `lake`, `pond`, `canal`; `world.water()` takes it away. `splash(x, y, z, size)` throws water and rings; a node's `wake = true` leaves a wake |
| `world.retro("ps1")`, `world.backdrop{ { image, scroll, y, fill }, … }` | The retro look (`ps1`, `saturn`, `snes`, `arcade`, `"off"`) and its parallax layers (4 at most) |

Tune them in this order, against frames of the game being played: the
sky and sun, the fog, the grade, shafts and halos, bloom, then film.
The player, the course and the hazards stay the clearest things on screen.

<!-- names: world.sky world.sun world.lighting world.fog world.post world.dof world.speedBlur world.grade world.shafts world.halos world.film world.water world.retro world.backdrop splash -->

## Terrain [terrain]

The ground of an outdoor 3D world: hills, mountains, craters and river
beds from noise and stamps, made in Rust, the same land on every phone
for the same numbers. It's drawn in tiles around the camera, finer near
it, so a world can be endless.

```lua
local ground = Terrain{
  noise  = { seed = 7, octaves = 5, frequency = 0.004, ridged = 0.3, warp = 25 },
  height = 90,                                     -- meters from the lowest ground to the highest
  stamps = { { "hill", 20, -10, radius = 18, height = 6 },
             { "crater", 80, 40, radius = 30, depth = 12, rim = 3 },
             { "river", { -60, 0, 0, 10, 60, 5 }, width = 6, bank = 20, bed = 2 } },
  paint  = { { surface = "grass" }, { above = 60, surface = "snow" }, { slope = 0.2, surface = "rock" } },
}
hero.y = ground:heightAt(hero.x, hero.z)            -- stand things on it
scatter(grass, { count = 4000, area = 120, on = ground })
```

| Key | |
|---|---|
| `noise = { seed, octaves, frequency, ridged, warp }` | `frequency` is about 1 ÷ a hill's width (0.002, the default: hills ~500 m apart); `octaves` 1–8 layers of detail (6); `ridged` 0–1 raises mountain ranges with sharp crests, snow and rock between broad basins of rolling land (0.6; 0 for rolling land only). The ranges are kept back from (0, 0): the ground within about 60 m of it is gentle, and they rise to their full height by 180 m, so a game that starts at the middle starts in a meadow with peaks around it; `warp` meters the land is bent by (default 0.12 ÷ `frequency`; 0 for none). Valleys are broad and the high ground steeper, as mountains are; at the defaults half the land is under 26° and about 1 part in 170 past 60°, all of it in the ranges. `noise = false`: flat ground at 0, stamps only |
| `height` | Meters from the noise's lowest to its highest (150): the basins sit in the lower third, the peaks reach it |
| `stamps` | In order, on the noise: `{ "hill", x, z, radius, height }` (a negative height is a dip); `{ "crater", x, z, radius, depth, rim }`; `{ "river", { x1, z1, x2, z2, … }, width, bank, bed }`, a flat bed `width` across at `bed` meters (a little under the land along it if left out), back up to the land over `bank` either side, or wider where the land stands high above it (a valley, not a canyon: sides a figure can walk, under about 38°, up to 30 m over the bed, about 45° on higher land it cuts past), wandering a little either side of its line. A river only cuts |
| `paint` | Which ground is where. The first rule is the base; each later one lays its ground over it where it holds: `above` or `below` a height (meters), `slope` steeper than (`1 − normal.y`: 0 flat, 0.13 is 30°, 0.29 is 45°, 0.5 is 60°), with `blend` meters (or slope) of edge, broken by a little noise so it isn't a ruled line. A rule names a `surface`: `"grass"`, `"meadow"` (dry, patchy grass), `"dirt"`, `"rock"` or `"snow"`, photographed and drawn per pixel; or a `color` (`0xRRGGBB`): the surface nearest that colour, recoloured to it; or both (`{ surface = "rock", color = 0x8a5a44 }`, red rock). Left out, the ground is a sunny mountain landscape: grass in several greens with dry meadow in broken patches, sand in the lowest hollows, dirt on steep ground and in gullies, rock on cliffs and the high ground, snow over a wavering snow line, crests lighter, hollows darker |
| `surfaces` | The game's own picture for a surface: `{ rock = "basalt" }` (`images/basalt`, 512 px or more a side, so not in an atlas), or `{ rock = { image = "basalt", normal = "basalt_n", size = 6 } }` with its normal map and the meters one repeat covers |
| `texture`, `textureSize` | `texture = "tiles"`: `images/tiles` under every rule that names only a colour (recoloured to it), every `textureSize` meters (4). `texture = false`: no pictures at all, every rule its plain colour (still drawn per pixel) |
| `reach`, `detail` | Meters drawn around the camera (400) and between the ground's points near it: 0.5, 1 (the default, a game on foot), 2 (a vehicle), 4. `reach ÷ detail` over 800 is an error; over 400 a warning, as it takes much of the phone's triangles |
| `fog` | `true` (the default): air you see the land through, unless the game calls `world.fog{}`: the near and middle ground at full contrast, then from 60% of `reach` the far ranges going to the sky's horizon colour, gone at `reach` |
| `size` | A square world this many meters a side around (0, 0); left out, it's endless. The ground stops at its edge |
| `collision` | `true` (the default): bodies land on the ground, a heightfield of the true ground ([physics]); `false` turns it off |

| | |
|---|---|
| `ground:heightAt(x, z)`, `ground:normalAt(x, z)` → nx, ny, nz | The true ground, the same for every player whatever is drawn |
| `ground:raise(x, z, radius, amount)` | Lifts the ground in a smooth bump (a negative `amount` digs): `heightAt` has it at once, the drawn ground next frame |
| `ground.progress`, `ground.built` | How much of the first view is built (0–1), and a signal when it is. The title card waits for it, so a round starts on finished ground |

Without `world.sun{}` (or `world.lighting`), a terrain brings a sun of its
own: 30° up, warm, from behind a camera looking down −z, with shadows, so
the hills' shape reads. A follow camera stays over the ground and keeps
the node's legs in view over a crest; a camera the game places itself is
the game's to keep there (`heightAt`). A model's `plantFeet` stands it on
the terrain: each foot on the ground under it and along its slope, the
body eased down for the downhill foot. Models face −z: a
generated or imported one is turned so when it lands, and one that walks
sideways faces another way (`Model{ ry = math.pi / 2 }` turns it).

**What's drawn.** Tiles of 32 × 32 quads in three levels, each farther one
covering four times the ground, built a tile a frame as the camera moves,
nearest first. About 30 draws and 70k triangles at `reach ÷ detail` = 400,
of the phone's 200 and 150k. The surfaces are photographs blended per
pixel by the phone's ground shader: up close each shows its grain and catches
the sun; rock is laid from the side on cliffs, so it doesn't streak; each
repeats in two sizes and turns, so no grid shows; and farther out each fades
to its own colour, so the far ground stays sharp (snow lines and rock edges
per pixel) without a repeating grain. Far ground is coarser: on its third level
(past `64 × detail` meters) it can be half a meter off `heightAt`, so
`scatter`'s copies on it fade out before there (set the node's `fade` to
keep them further).

One `Terrain` a round. It stays where it is: it has `heightAt`, `normalAt`,
`raise`, `progress`, `built`, `visible`, `name`, `tag`, `data` and `destroy`,
and no position or turn.

<!-- names: Terrain Terrain.progress Terrain.built Terrain:heightAt Terrain:normalAt Terrain:raise -->

## Physics [physics]

Rigid bodies on the SDK's Rapier, built into the engine: in 2D it's
`love.physics`, Box2D's API as LÖVE has it; in 3D it's Godot's nodes
(`RigidBody3D` and the rest). Both step at a fixed 60 Hz inside, from the
frame's `dt`, so a round replays to the same bits on every phone, and both
draw bodies between their last two steps, so motion is smooth at 120 Hz.

### 2D: love.physics

```lua
love.physics.setMeter(64)                                       -- px a meter (LÖVE's default is 30)
world = love.physics.newWorld(0, 9.81 * 64, true)
local ground = love.physics.newBody(world, 180, 600, "static")
love.physics.newFixture(ground, love.physics.newRectangleShape(360, 40))
local body = love.physics.newBody(world, 180, 100, "dynamic")    -- "static", "dynamic" or "kinematic"
local f = love.physics.newFixture(body, love.physics.newRectangleShape(48, 48), 1)   -- density
f:setFriction(0.8); f:setRestitution(0.1); f:setUserData(crate)
crate.body = body                                               -- the node follows it (ours)
world:setCallbacks(beginContact, endContact, preSolve, postSolve)
function love.update(dt) world:update(dt) end
```

Everything is LÖVE's: the same names, arguments, return values and units.
Positions, sizes and velocities are pixels (`setMeter` and `getMeter` say
how many make a meter, as in LÖVE), angles radians clockwise; forces and
impulses scale by the meter and torques by its square, as LÖVE scales them.
A fixture's friction starts at 0.2 and two fixtures bounce by the larger
restitution, as in Box2D. Two fixtures slide with the smaller of their
frictions (Box2D mixes by √(a·b)): a frictionless one stays frictionless,
and `contact:getFriction()` says what was used.

**Making things.** `love.physics.newWorld(gx, gy, sleep)`,
`newBody(world, x, y, type)`, `newFixture(body, shape, density)`; shapes
`newRectangleShape(w, h)` or `(x, y, w, h, angle)`, `newCircleShape(r)` or
`(x, y, r)`, `newPolygonShape(x1, y1, …)`, `newEdgeShape(x1, y1, x2, y2)`,
`newChainShape(loop, x1, y1, …)`; joints `newWeldJoint`,
`newRevoluteJoint`, `newDistanceJoint`, `newRopeJoint`,
`newPrismaticJoint`, `newWheelJoint` (two bodies, then the anchors, LÖVE's
argument lists and `collideConnected`) and `newMouseJoint(body, x, y)`.

**A node that follows a body:** `node.body = body` (any 2D node; ours).
After `love.update` the node goes where the body is drawn, turned with it,
so a sprite needs no `love.draw`. It's in the world: a node with a parent
can't follow one. `node.body = nil` lets go.

| Object | Methods (LÖVE's) |
|---|---|
| World | `update`, `setGravity`, `getGravity`, `setCallbacks`, `getCallbacks`, `rayCast(x1, y1, x2, y2, fn)` (`fn(fixture, x, y, xn, yn, fraction)` nearest first; return -1 to skip, 0 to stop, a fraction to clip, 1 to go on), `queryBoundingBox(x1, y1, x2, y2, fn)`, `getBodies`, `getBodyCount`, `getJoints`, `getJointCount`, `getContacts`, `getContactCount`, `setSleepingAllowed`, `isSleepingAllowed`, `isLocked`, `destroy`, `isDestroyed` |
| Body | `getPosition`, `getX`, `getY`, `getAngle`, `setPosition`, `setX`, `setY`, `setAngle`, `getTransform`, `setTransform`, `getLinearVelocity`, `setLinearVelocity`, `getAngularVelocity`, `setAngularVelocity`, `applyForce`, `applyLinearImpulse`, `applyTorque`, `applyAngularImpulse`, `getMass`, `setMass`, `getInertia`, `resetMassData`, `getMassData`, `getType`, `setType`, `isAwake`, `setAwake`, `isSleepingAllowed`, `setSleepingAllowed`, `isFixedRotation`, `setFixedRotation`, `isBullet`, `setBullet`, `getGravityScale`, `setGravityScale`, `getLinearDamping`, `setLinearDamping`, `getAngularDamping`, `setAngularDamping`, `getWorldPoint`, `getWorldPoints`, `getLocalPoint`, `getWorldVector`, `getLocalVector`, `getWorldCenter`, `getLocalCenter`, `getLinearVelocityFromWorldPoint`, `getLinearVelocityFromLocalPoint`, `getFixtures`, `getJoints`, `getContacts`, `isTouching`, `getUserData`, `setUserData`, `getWorld`, `destroy`, `isDestroyed`, `isActive`, `setActive` |
| Fixture | `getBody`, `getShape`, `getType`, `setFriction`, `getFriction`, `setRestitution`, `getRestitution`, `setDensity`, `getDensity`, `setSensor`, `isSensor`, `setUserData`, `getUserData`, `setCategory`, `getCategory`, `setMask`, `getMask`, `setGroupIndex`, `getGroupIndex`, `setFilterData`, `getFilterData`, `testPoint`, `getBoundingBox`, `getMassData`, `destroy`, `isDestroyed` |
| Shape | `getType`, `getRadius`, `getPoint`, `getPoints`, `getVertexCount`, `getChildCount`, `testPoint`, `setRadius`, `setPoint` |
| Joint | `getType`, `getBodies`, `getAnchors`, `getReactionForce`, `getReactionTorque`, `getCollideConnected`, `getUserData`, `setUserData`, `destroy`, `isDestroyed`; revolute, prismatic and wheel: `setLimits`, `getLimits`, `setLimitsEnabled`, `areLimitsEnabled`, `setLowerLimit`, `setUpperLimit`, `getLowerLimit`, `getUpperLimit`, `setMotorEnabled`, `isMotorEnabled`, `setMotorSpeed`, `getMotorSpeed`, `setMaxMotorTorque`, `getMaxMotorTorque`, `setMaxMotorForce`, `getMaxMotorForce`, `getJointAngle`, `getJointTranslation`, `getJointSpeed`, `getAxis`; distance and rope: `setLength`, `getLength`, `setMaxLength`, `getMaxLength`; springs: `setFrequency`, `getFrequency`, `setDampingRatio`, `getDampingRatio`, `setSpringFrequency`, `getSpringFrequency`, `setSpringDampingRatio`, `getSpringDampingRatio`; mouse: `setTarget`, `getTarget`, `setMaxForce`, `getMaxForce` |
| Contact | `getPositions`, `getNormal`, `getFixtures`, `isTouching`, `setEnabled`, `isEnabled`, `getFriction`, `setFriction`, `resetFriction`, `getRestitution`, `setRestitution`, `resetRestitution`, `getChildren` |

Every object also has LÖVE's `type()`, `typeOf(name)` and `release()`. In
the type definitions LÖVE's World and Shape are `PhysicsWorld` and
`PhysicsShape` (bump's `World` and the `Shape` node have those names).

**Callbacks** run inside `world:update`, after the step that caused them:
`beginContact(a, b, contact)` and `endContact` as pairs start and stop
touching (a sensor fixture's overlaps too), then `preSolve(a, b, contact)`
and `postSolve(a, b, contact, normalImpulse, tangentImpulse)` for every
pair still touching. As in Box2D, a body, fixture or joint can't be made or
destroyed inside one: remember it, and do it after `world:update` (the
error says so).

**A character in the world** (ours, as LÖVE has none): Godot's
`CharacterBody2D`, a node in a love.physics world.

```lua
hero = CharacterBody2D{ world = world, shape = { w = 20, h = 40 }, x = 100, y = 300, floorSnapLength = 2 }
Walker{ image = "fox", height = 46, parent = hero, y = 20 }    -- drawn with it
local shelf = love.physics.newFixture(ledge, love.physics.newRectangleShape(200, 10))
shelf:setOneWay(true)                                           -- it lands on it from above, jumps up through it
function love.update(dt)
  world:update(dt)
  hero.vx = input.ax * 200
  hero.vy = hero:isOnFloor() and 0 or hero.vy + 900 * dt        -- gravity is the game's, as in Godot
  if jump and hero:isOnFloor() then hero.vy = -500 end
  hero:moveAndSlide()                                           -- moveAndSlide(true) drops down through one-way fixtures
end
```

`shape` is a box `{ w, h }`, a circle `{ radius }` or a capsule `{ radius,
h }`, in pixels; `x`, `y` is its middle, in the world (it has no parent).
Its `vx`, `vy` move it only through `moveAndSlide()` (the node doesn't
drift by them), which slides along what it meets and returns whether it
met anything; then `isOnFloor()`, `isOnFloorOnly()`, `isOnWall()`,
`isOnWallOnly()`, `isOnCeiling()`, `getFloorNormal()` and `getWallNormal()`
(x, y), `getFloorAngle()`, `getRealVelocity()` (x, y),
`getSlideCollisionCount()` and `getSlideCollision(i)` (`{ nx, ny }`). Its
settings are Godot's: `floorMaxAngle` (radians, 45° unless set),
`floorSnapLength` (pixels it holds on to a floor it walks off, 1),
`maxStepHeight` (ours: ledges it steps up, 0), `floorStopOnSlope`,
`floorBlockOnWall`, `wallMinSlideAngle` and `slideOnCeiling`. The world's
bodies meet it: crates land on it, and it stands on them.
`fixture:setOneWay(true)` (ours; `isOneWay()` reads it) makes a fixture a
one-way platform for characters; bodies meet it from every side. The quick
path's `player.platformer` runs on the same controller ([quick]).

| Box2D in LÖVE | 2.0 | Why |
|---|---|---|
| `World:update(dt)` steps by `dt` | Fixed 60 Hz steps from the frames' `dt`. `World:update(dt, velocityiterations)` sets Rapier's solver iterations from then on (4 unless given; Box2D's 8 make a welded tower stiffer, and `getReactionTorque` reads nearer the true load); position iterations are taken and ignored | Replays, and the SDK's proven physics |
| Reads in `love.draw` see the last step | Positions and angles read in `love.draw` are where the body is drawn, between its last two steps | Smooth at 120 Hz |
| `preSolve` runs before the solver, and `contact:setEnabled(false)` there lets a body pass (one-way platforms) | `preSolve` runs after the step, with `postSolve`; `setEnabled`, `setFriction` and `setRestitution` on a contact change nothing (the log says so once). One-way platforms are `fixture:setOneWay(true)`, for `CharacterBody2D` | Callbacks run in Lua between steps, never inside Rapier's |
| A contact can be kept | A contact lasts as long as its callback; after that its methods say it's gone | |
| Two fixtures' friction is √(a·b) | The smaller of the two (0.2 and 0.8 slide at 0.2, not 0.4); 0 against anything is still 0 | Rapier has no geometric mean; the smaller keeps frictionless frictionless |
| `setGroupIndex` filters pairs | Kept and read back, but doesn't filter: use `setCategory` and `setMask` (and joined bodies don't collide unless `collideConnected`) | |
| `getReactionForce` is a vector | Its size, in x (y is 0) | |
| `getReactionForce(invdt)` and `getReactionTorque(invdt)` take `1 / dt` | They take the physics' step rate: pass `60` (or nothing), not `1 / dt`, which reads twice the load on a 120 Hz screen | Physics steps at a fixed 60 Hz |
| `setFrequency` softens weld and distance joints | Weld and distance joints stay rigid; wheel and mouse joints' springs take it. Rapier's welds are stiffer than Box2D's, so a game that breaks welds by torque tunes its threshold (a `debug.number`) | |
| Motor, gear, pulley and friction joints, `getDistance` | Absent (the error says what to use) | |
| `World:translateOrigin`, `setContactFilter`, `Body:setActive` | Absent, or (`setActive`) logged and ignored | |

### 3D: Godot's nodes

```lua
local ground = Terrain{}                                   -- bodies land on it
local crate = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, mass = 20, x = 0, y = 12, z = 0 }
Mesh{ geometry = geometry.box(1, 1, 1), parent = crate }   -- what it looks like rides along
crate.contactMonitor = true
crate.bodyEntered:connect(function(other) fx.shake(0.2, 0.1) end)
crate:applyCentralImpulse(0, 200, 0)
local hit = physics.raycast(0, 50, 0, 0, -50, 0)           -- { node, x, y, z, nx, ny, nz, distance }
```

Godot's class names and meanings, members in camelCase, numbers instead of
`Vector3`s. Bodies are 3D nodes: meshes and models go under them as
children. A body's `vx`, `vy`, `vz` are its velocity and `vrx`, `vry`,
`vrz` its spin (radians a second), read and written on the body; its
`gravity` does nothing (gravity is the world's, times `gravityScale`).

| Node | Has | Notes |
|---|---|---|
| `RigidBody3D{ shape, mass, friction, bounce, gravityScale, linearDamp, angularDamp, contactMonitor, sleeping, freeze, canSleep, continuousCd, lockRotation, collisionLayer, collisionMask }` | `applyImpulse(x, y, z, px, py, pz)` (at an offset from its origin, in world axes), `applyCentralImpulse(x, y, z)`, `applyForce(x, y, z, px, py, pz)` and `applyCentralForce(x, y, z)` (this frame: call them every frame you push), `applyTorque(x, y, z)`, `applyTorqueImpulse(x, y, z)`, `getCollidingBodies()`; signals `bodyEntered(other)`, `bodyExited(other)` with `contactMonitor = true` | `mass` 1 kg, `friction` 1, `bounce` 0 unless set, as Godot's. `freeze = true` holds it: moving its node then pushes what's in the way. `continuousCd = true` for small fast things (a thrown ball): each step it sweeps ahead and meets what's there face on rather than passing through |
| `StaticBody3D{ shape, friction, bounce, collisionLayer, collisionMask }` | | Moved by writing `x`, `y`, `z` (or a tween), it becomes a moving platform that pushes and carries |
| `Area3D{ shape, monitoring, collisionLayer, collisionMask }` | signals `bodyEntered`, `bodyExited`, `areaEntered`, `areaExited`; `getOverlappingBodies()`, `getOverlappingAreas()`, `overlapsBody(node)` | Triggers, pickups, hurtboxes. Under a moving node (a hand), it follows it |
| `VehicleBody3D{ shape, mass, engineForce, brake, steering, … }` with `VehicleWheel3D{ parent = car, useAsTraction, useAsSteering, wheelRadius, wheelRestLength, suspensionTravel, suspensionStiffness, suspensionMaxForce, dampingCompression, dampingRelaxation, wheelFrictionSlip, engineForce, brake, steering }` children | A RigidBody3D's members, and `speed` (ours: m/s along its front, negative backing up); a wheel's `isInContact()` | Godot's ray-cast car: each wheel is a ray with a spring, so it catches air off a crest and lands on its springs. Placed where its suspension starts; the engine turns and steers the wheel node, so a wheel mesh under it spins. `engineForce` is newtons on each traction wheel, `brake` an impulse a step (5–20 stops a buggy), `steering` radians, positive left. **It faces −z**, every 2.0 node's front, so `camera:follow` sits behind it (Godot's faces +z). `mass` 40 unless set |
| `BoxShape3D{ w, h, d }`, `SphereShape3D{ radius }`, `CapsuleShape3D{ radius, height }` (`height` the whole capsule, Godot 4's), `CylinderShape3D{ radius, height }`, `ConvexPolygonShape3D{ geometry }` (its hull), `ConcavePolygonShape3D{ geometry }` (static bodies and areas only) | each with `x`, `y`, `z`, `rx`, `ry`, `rz`: where it sits in its body | A body's `shape` is one shape or a list of them (Godot's `CollisionShape3D` children). `HeightMapShape3D` is an error: a `Terrain` is the ground's shape already |
| `PinJoint3D`, `HingeJoint3D`, `SliderJoint3D`, `ConeTwistJoint3D`, `Generic6DOFJoint3D{ nodeA, nodeB, x, y, z, … }` | `limitLower`, `limitUpper` (radians for a hinge, meters for a slider), `motorVelocity`, `motorMaxImpulse`; ours: `breakForce`, `breakTorque` (newtons, newton-meters; 0 never breaks), read-only `force` and `torque` from the last step, and a `broken` signal | Joints are nodes, placed where they join. A hinge turns about the joint's z axis and a slider slides along its x (turn the joint with `rx`, `ry`); `Generic6DOFJoint3D` holds every axis (a weld); `ConeTwistJoint3D` is a ball joint without its limits yet |
| `physics.raycast(x1, y1, z1, x2, y2, z2, { mask, exclude, areas })` | → `{ node, x, y, z, nx, ny, nz, distance }` or nil | Godot's `intersect_ray`: `mask` the layers it sees, `exclude` a node or a list, `areas = true` to see areas. It finds a `Terrain` wherever it meets the ground. Also from a tap: `camera:ray(sx, sy)` |
| `physics.setGravity(x, y, z)`, `physics.getGravity()` | | 9.8 m/s² down unless set |

**Moving bodies yourself.** Writing a dynamic body's `x`, `y`, `z`, `rx`,
`ry`, `rz` (or a tween, or `faceDirection`) teleports it; a static, frozen
or area body is moved there over the next step, pushing what's in its way.
A body under a parent node follows the parent. A `RigidBody3D` can't go
under another body: join them, or give one body several shapes.

**Layers** are Godot's: `collisionLayer` is a body's layers and
`collisionMask` the layers it looks for, both bit numbers (layer 1 is 1,
layer 2 is 2, both 3; both default to 1), and two bodies touch when either
one's layer is in the other's mask.

**The ground.** A `Terrain`'s `collision` (on unless `false`) makes the
ground a heightfield for the bodies: tiles of the true ground, at the
terrain's `detail`, made around each dynamic body before the step that
needs them, so a body lands on what `heightAt` says however far it goes.

**Characters.** `CharacterBody3D{ shape, floorMaxAngle, floorSnapLength,
maxStepHeight, … }` is Godot's: a body that moves only when you call
`moveAndSlide()`, by its `vx`, `vy`, `vz` over the frame, sliding along
what it meets, climbing slopes up to `floorMaxAngle` (radians, 45° unless
set) and sliding down steeper ones, stepping up ledges as tall as
`maxStepHeight` (ours: 0, none, unless set), and held on to a floor it
walks off by `floorSnapLength` (0.1 m). Gravity is the game's to add to
`vy`. Its origin is its shape's middle (a capsule's: put its model at `y =
-height / 2`).

```lua
knight = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.5 }, y = 8, floorSnapLength = 0.3, maxStepHeight = 0.35 }
Model{ model = "knight", parent = knight, y = -0.75 }
-- love.update(dt):
knight.vx, knight.vz = input.ax * 5, input.ay * 5
if knight:isOnFloor() then knight.vy = jump and 6.5 or 0 else knight.vy = knight.vy - 18 * dt end
knight:moveAndSlide()
```

`moveAndSlide()` returns whether it met anything and leaves `vx`, `vy`,
`vz` as Godot does: into a wall or ceiling taken out, the fall reset on the
floor. Then `isOnFloor()`, `isOnFloorOnly()`, `isOnWall()`,
`isOnWallOnly()`, `isOnCeiling()`, `getFloorNormal()` and `getWallNormal()`
(x, y, z), `getFloorAngle()`, `getRealVelocity()` (x, y, z: how fast it
really moved), `getSlideCollisionCount()` and `getSlideCollision(i)` (`{
node, x, y, z, nx, ny, nz }`). Godot's `floorStopOnSlope`,
`floorBlockOnWall`, `wallMinSlideAngle` (15°) and `slideOnCeiling` are
there too, and `collisionLayer`, `collisionMask`. Rigid bodies and areas
meet it (an `Area3D`'s `bodyEntered` hears it), it stands on the terrain
and on static bodies, and moving it by writing `x`, `y`, `z` puts it there.

**Navigation.** `NavigationRegion3D{ bake = { agentRadius, agentHeight,
maxSlope, maxClimb, cellSize, cellHeight }, size }` bakes a navmesh
(Recast) after `love.load` returns, from the `Terrain` inside its square
(`size` meters across, around its `x`, `z`: the terrain's `size`, or 64)
and the static bodies under it (or, if none are, every `StaticBody3D`):
16 m tiles, a few a frame, behind the title card, which waits for it.
`baked` and `progress` say how far it is, `bakeFinished` fires when it's
done, `region:bake()` bakes again the tiles whose level changed (a wall put
up), and `region:path(x1, y1, z1, x2, y2, z2)` is a path's corners `{ x, y,
z, … }`, or nil. `maxSlope` is radians, like `floorMaxAngle`; Godot's
defaults otherwise (an agent 0.5 m wide, 1.5 m tall, climbing 0.25 m, 0.25 m
cells), but cells 0.05 m tall. A slope is also too steep where it climbs
more than `maxClimb` over two cells (Recast's rule): 26° at a 0.25 m climb,
39° at 0.4 m, so a ramp up to a platform wants `maxClimb = 0.4` or more.
Static bodies' tops and ramps (tilted bodies) are walkable; an upright
body's inside isn't.

```lua
NavigationRegion3D{ bake = { agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 } }
local goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.3, height = 1.2 }, x = 20, y = 8, z = 20 }
local agent = NavigationAgent3D{ parent = goblin, maxSpeed = 3.2, autoMove = true }
timer.every(0.5, function() agent:setTargetPosition(knight.x, knight.y, knight.z) end)
agent.targetReached:connect(function() caught() end)
```

A `NavigationAgent3D` goes under the body it steers. It walks a crowd
(DetourCrowd): `setTargetPosition(x, y, z)`, `getNextPathPosition()` (x,
y, z: the next corner more than `pathDesiredDistance` away), its `vx`,
`vy`, `vz` (read-only: the crowd's velocity for it), `isNavigationFinished()`,
`isTargetReached()`, `isTargetReachable()`, `distanceToTarget()`,
`getFinalPosition()`, and `targetReached` and `navigationFinished` fire
once when its body comes within `targetDesiredDistance` (1 m) of the
target. `maxSpeed` (5 m/s; 0 stops it), `maxAcceleration` (20), `radius`,
`height`. **`avoidance` is on unless set false** (Godot's is off): agents
keep apart and steer round each other. **`autoMove = true`** (ours) moves
its `CharacterBody3D` in Rust each frame, along the path, turning it to
face its way, with gravity: a crowd of 50 costs no Lua. Moving the body
(`goblin.x, goblin.z = …`, a respawn) puts it there, and its agent goes on
to its target from there. Without it the
agent is a path and the game moves the body (toward
`getNextPathPosition()`, or by its `vx`, `vz`).

**The debug menu's Physics tab** (a game with bodies): Show colliders
draws every collider over the game (dynamic orange, asleep grey, static
blue-grey, kinematic green, areas faint; in 3D within 60 m of the camera,
the ground's tiles near it; navmeshes as green lines), and Pause physics
holds the bodies still while the rest of the game goes on.

| Godot | 2.0 | Why |
|---|---|---|
| `linear_velocity` (a `Vector3`) | `vx`, `vy`, `vz`; `angular_velocity` is `vrx`, `vry`, `vrz` | Numbers, as every 2.0 node |
| `CollisionShape3D` children | `shape = BoxShape3D{ … }` or a list, each with its own `x`, `y`, `z`, `rx`, `ry`, `rz` | Projects are text; one table makes a body |
| `VehicleBody3D` drives toward +z | Toward −z | 2.0's front, which `camera:follow` and `faceDirection` use |
| Joints break only in code | `breakForce`, `breakTorque` and `broken` (ours) | Crates that weld and snap |
| `_physics_process(delta)` | `love.update(dt)`; bodies step at a fixed 60 Hz inside | One loop |
| `velocity` (a `Vector3`) on a `CharacterBody3D` | `vx`, `vy`, `vz` | Numbers, as every 2.0 node |
| `NavigationMesh` resource, baked in the editor | `bake = { … }`, baked in the game behind the title card, in tiles | Levels are made by Lua at load |
| `NavigationAgent3D.avoidance_enabled` off | `avoidance` on | A crowd that walks through itself reads as a bug |
| An agent's `velocity_computed` signal to move the body | `autoMove = true` moves it in Rust | No Lua a frame for a crowd |
| `floor_max_angle` etc. in degrees in the editor | Radians, as Godot's code has them | |

<!-- names: love.physics.setMeter love.physics.getMeter love.physics.newWorld love.physics.newBody love.physics.newFixture love.physics.newRectangleShape love.physics.newCircleShape love.physics.newPolygonShape love.physics.newEdgeShape love.physics.newChainShape love.physics.newWeldJoint love.physics.newRevoluteJoint love.physics.newDistanceJoint love.physics.newRopeJoint love.physics.newPrismaticJoint love.physics.newWheelJoint love.physics.newMouseJoint Node.body
Body:applyAngularImpulse Body:applyForce Body:applyLinearImpulse Body:applyTorque Body:destroy Body:getAngle Body:getAngularDamping Body:getAngularVelocity Body:getContacts Body:getFixtures Body:getGravityScale Body:getInertia Body:getJoints Body:getLinearDamping Body:getLinearVelocity Body:getLinearVelocityFromLocalPoint Body:getLinearVelocityFromWorldPoint Body:getLocalCenter Body:getLocalPoint Body:getLocalVector Body:getMass Body:getMassData Body:getPosition Body:getTransform Body:getType Body:getUserData Body:getWorld Body:getWorldCenter Body:getWorldPoint Body:getWorldPoints Body:getWorldVector Body:getX Body:getY Body:isActive Body:isAwake Body:isBullet Body:isDestroyed Body:isFixedRotation Body:isSleepingAllowed Body:isTouching Body:release Body:resetMassData Body:setActive Body:setAngle Body:setAngularDamping Body:setAngularVelocity Body:setAwake Body:setBullet Body:setFixedRotation Body:setGravityScale Body:setLinearDamping Body:setLinearVelocity Body:setMass Body:setPosition Body:setSleepingAllowed Body:setTransform Body:setType Body:setUserData Body:setX Body:setY Body:type Body:typeOf Contact:getChildren Contact:getFixtures Contact:getFriction Contact:getNormal Contact:getPositions Contact:getRestitution Contact:isEnabled Contact:isTouching Contact:release Contact:resetFriction Contact:resetRestitution Contact:setEnabled Contact:setFriction Contact:setRestitution Contact:type Contact:typeOf Fixture:destroy Fixture:getBody Fixture:getBoundingBox Fixture:getCategory Fixture:getDensity Fixture:getFilterData Fixture:getFriction Fixture:getGroupIndex Fixture:getMask Fixture:getMassData Fixture:getRestitution Fixture:getShape Fixture:getType Fixture:getUserData Fixture:isDestroyed Fixture:isSensor Fixture:release Fixture:setCategory Fixture:setDensity Fixture:setFilterData Fixture:setFriction Fixture:setGroupIndex Fixture:setMask Fixture:setRestitution Fixture:setSensor Fixture:setUserData Fixture:testPoint Fixture:type Fixture:typeOf Joint:areLimitsEnabled Joint:destroy Joint:getAnchors Joint:getAxis Joint:getBodies Joint:getCollideConnected Joint:getDampingRatio Joint:getFrequency Joint:getJointAngle Joint:getJointSpeed Joint:getJointTranslation Joint:getLength Joint:getLimits Joint:getLowerLimit Joint:getMaxForce Joint:getMaxLength Joint:getMaxMotorForce Joint:getMaxMotorTorque Joint:getMotorSpeed Joint:getReactionForce Joint:getReactionTorque Joint:getSpringDampingRatio Joint:getSpringFrequency Joint:getTarget Joint:getType Joint:getUpperLimit Joint:getUserData Joint:isDestroyed Joint:isMotorEnabled Joint:release Joint:setDampingRatio Joint:setFrequency Joint:setLength Joint:setLimits Joint:setLimitsEnabled Joint:setLowerLimit Joint:setMaxForce Joint:setMaxLength Joint:setMaxMotorForce Joint:setMaxMotorTorque Joint:setMotorEnabled Joint:setMotorSpeed Joint:setSpringDampingRatio Joint:setSpringFrequency Joint:setTarget Joint:setUpperLimit Joint:setUserData Joint:type Joint:typeOf PhysicsShape:getChildCount PhysicsShape:getPoint PhysicsShape:getPoints PhysicsShape:getRadius PhysicsShape:getType PhysicsShape:getVertexCount PhysicsShape:release PhysicsShape:setPoint PhysicsShape:setRadius PhysicsShape:testPoint PhysicsShape:type PhysicsShape:typeOf PhysicsWorld:destroy PhysicsWorld:getBodies PhysicsWorld:getBodyCount PhysicsWorld:getCallbacks PhysicsWorld:getContactCount PhysicsWorld:getContacts PhysicsWorld:getGravity PhysicsWorld:getJointCount PhysicsWorld:getJoints PhysicsWorld:isDestroyed PhysicsWorld:isLocked PhysicsWorld:isSleepingAllowed PhysicsWorld:queryBoundingBox PhysicsWorld:rayCast PhysicsWorld:release PhysicsWorld:setCallbacks PhysicsWorld:setGravity PhysicsWorld:setSleepingAllowed PhysicsWorld:type PhysicsWorld:typeOf PhysicsWorld:update
Area3D Area3D.areaEntered Area3D.areaExited Area3D.bodyEntered Area3D.bodyExited Area3D.collisionLayer Area3D.collisionMask Area3D.monitoring Area3D.shape Area3D:getOverlappingAreas Area3D:getOverlappingBodies Area3D:overlapsBody BoxShape3D CapsuleShape3D ConcavePolygonShape3D ConeTwistJoint3D ConvexPolygonShape3D CylinderShape3D Generic6DOFJoint3D HeightMapShape3D HingeJoint3D Joint3D.breakForce Joint3D.breakTorque Joint3D.broken Joint3D.force Joint3D.limitLower Joint3D.limitUpper Joint3D.motorMaxImpulse Joint3D.motorVelocity Joint3D.nodeA Joint3D.nodeB Joint3D.torque PinJoint3D RigidBody3D RigidBody3D.angularDamp RigidBody3D.bodyEntered RigidBody3D.bodyExited RigidBody3D.bounce RigidBody3D.canSleep RigidBody3D.collisionLayer RigidBody3D.collisionMask RigidBody3D.contactMonitor RigidBody3D.continuousCd RigidBody3D.freeze RigidBody3D.friction RigidBody3D.gravityScale RigidBody3D.linearDamp RigidBody3D.lockRotation RigidBody3D.mass RigidBody3D.shape RigidBody3D.sleeping RigidBody3D:applyCentralForce RigidBody3D:applyCentralImpulse RigidBody3D:applyForce RigidBody3D:applyImpulse RigidBody3D:applyTorque RigidBody3D:applyTorqueImpulse RigidBody3D:getCollidingBodies SliderJoint3D SphereShape3D StaticBody3D StaticBody3D.bounce StaticBody3D.collisionLayer StaticBody3D.collisionMask StaticBody3D.friction StaticBody3D.shape VehicleBody3D VehicleBody3D.angularDamp VehicleBody3D.bodyEntered VehicleBody3D.bodyExited VehicleBody3D.bounce VehicleBody3D.brake VehicleBody3D.canSleep VehicleBody3D.collisionLayer VehicleBody3D.collisionMask VehicleBody3D.contactMonitor VehicleBody3D.continuousCd VehicleBody3D.engineForce VehicleBody3D.freeze VehicleBody3D.friction VehicleBody3D.gravityScale VehicleBody3D.linearDamp VehicleBody3D.lockRotation VehicleBody3D.mass VehicleBody3D.shape VehicleBody3D.sleeping VehicleBody3D.speed VehicleBody3D.steering VehicleBody3D:applyCentralForce VehicleBody3D:applyCentralImpulse VehicleBody3D:applyForce VehicleBody3D:applyImpulse VehicleBody3D:applyTorque VehicleBody3D:applyTorqueImpulse VehicleBody3D:getCollidingBodies VehicleWheel3D VehicleWheel3D.brake VehicleWheel3D.dampingCompression VehicleWheel3D.dampingRelaxation VehicleWheel3D.engineForce VehicleWheel3D.steering VehicleWheel3D.suspensionMaxForce VehicleWheel3D.suspensionStiffness VehicleWheel3D.suspensionTravel VehicleWheel3D.useAsSteering VehicleWheel3D.useAsTraction VehicleWheel3D.wheelFrictionSlip VehicleWheel3D.wheelRadius VehicleWheel3D.wheelRestLength VehicleWheel3D:isInContact physics.getGravity physics.raycast physics.setGravity
Fixture:setOneWay Fixture:isOneWay
CharacterBody2D CharacterBody2D.floorBlockOnWall CharacterBody2D.floorMaxAngle CharacterBody2D.floorSnapLength CharacterBody2D.floorStopOnSlope CharacterBody2D.maxStepHeight CharacterBody2D.slideOnCeiling CharacterBody2D.wallMinSlideAngle CharacterBody2D:getFloorAngle CharacterBody2D:getFloorNormal CharacterBody2D:getRealVelocity CharacterBody2D:getSlideCollision CharacterBody2D:getSlideCollisionCount CharacterBody2D:getWallNormal CharacterBody2D:isOnCeiling CharacterBody2D:isOnFloor CharacterBody2D:isOnFloorOnly CharacterBody2D:isOnWall CharacterBody2D:isOnWallOnly CharacterBody2D:moveAndSlide
CharacterBody3D CharacterBody3D.collisionLayer CharacterBody3D.collisionMask CharacterBody3D.floorBlockOnWall CharacterBody3D.floorMaxAngle CharacterBody3D.floorSnapLength CharacterBody3D.floorStopOnSlope CharacterBody3D.maxStepHeight CharacterBody3D.shape CharacterBody3D.slideOnCeiling CharacterBody3D.wallMinSlideAngle CharacterBody3D:getFloorAngle CharacterBody3D:getFloorNormal CharacterBody3D:getRealVelocity CharacterBody3D:getSlideCollision CharacterBody3D:getSlideCollisionCount CharacterBody3D:getWallNormal CharacterBody3D:isOnCeiling CharacterBody3D:isOnFloor CharacterBody3D:isOnFloorOnly CharacterBody3D:isOnWall CharacterBody3D:isOnWallOnly CharacterBody3D:moveAndSlide
NavigationAgent3D NavigationAgent3D.autoMove NavigationAgent3D.avoidance NavigationAgent3D.height NavigationAgent3D.maxAcceleration NavigationAgent3D.maxSpeed NavigationAgent3D.navigationFinished NavigationAgent3D.pathDesiredDistance NavigationAgent3D.radius NavigationAgent3D.targetDesiredDistance NavigationAgent3D.targetReached NavigationAgent3D:distanceToTarget NavigationAgent3D:getFinalPosition NavigationAgent3D:getNextPathPosition NavigationAgent3D:isNavigationFinished NavigationAgent3D:isTargetReachable NavigationAgent3D:isTargetReached NavigationAgent3D:setTargetPosition
NavigationRegion3D NavigationRegion3D.bakeFinished NavigationRegion3D.baked NavigationRegion3D.progress NavigationRegion3D.size NavigationRegion3D:bake NavigationRegion3D:path -->

## Shaders [shaders]

A shader is WGSL in `shaders/<name>.wgsl` that plugs into the engine's own
drawing, as Godot's shaders do: a 3D material's surface (`fn surface`), or
a 2D picture's colour (`fn sprite`). The phone checks every shader before
it draws (and `write_files` and `mb build` run the same checks): a shader
that fails them is refused with its line, and one that passes can't hang
the phone. Use one for what the materials and the world's look can't do:
a dissolve, water that moves, cartoon shading, a glow that ripples.

**A 3D material's shader** is `fn surface(in: Surface) -> SurfaceOut`. The
engine lights what it returns as it lights its own materials (the sun and
its shadow, point lights, the sky, fog), unless the material is `unlit`.

```wgsl
// shaders/dissolve.wgsl: burns away where a noise picture is under t.
struct Uniforms { t: f32, edge: vec3<f32> }

fn surface(in: Surface) -> SurfaceOut {
    let n = textureSample(mb_tex0, mb_samp, in.uv).r;
    if (n < uniforms.t) { discard; }
    let rim = 1.0 - smoothstep(uniforms.t, uniforms.t + 0.1, n);
    var o: SurfaceOut;
    o.albedo = in.color.rgb;
    o.alpha = 1.0;
    o.emission = uniforms.edge * rim * 6.0;
    o.roughness = 0.7;
    o.normal = in.normal;
    return o;
}
```

```lua
local burn = { t = 0, edge = 0xff6a10 }
local m = material{ shader = "dissolve", textures = { "noise" }, uniforms = burn, color = 0xb98a52 }
Mesh{ geometry = geometry.box(1, 1, 1), material = m, y = 0.5 }
flux.to(burn, 1, { t = 1 })   -- the engine reads `burn` every frame
```

| `Surface` (what it's given) | |
|---|---|
| `uv` | The mesh's texture coordinate |
| `position`, `local` | Where the pixel is in the world, and in the mesh's own space |
| `normal` | The surface's direction there (world, unit, facing the camera) |
| `view` | Toward the camera (unit) |
| `light` | Toward the sun (unit) |
| `color` | The material's `color` × the vertex colour (linear) |
| `time` | Seconds of the world's time |

| `SurfaceOut` (what it returns) | |
|---|---|
| `albedo` | The colour light falls on (linear) |
| `alpha` | Coverage, where the material is `transparent` (else ignored) |
| `emission` | Light of its own, added after lighting (glows with bloom) |
| `roughness`, `metalness` | 0–1, as the material's (0 roughness is a mirror) |
| `normal` | The direction it's lit by (`in.normal` for the surface's own) |

Start from `var o: SurfaceOut;` (everything 0) and set each field.
`discard;` leaves the pixel out (a cut-out, a dissolve).

**A vertex hook**, `fn vertex(in: Vertex) -> Vertex`, moves the mesh's
points first (`position`, `normal`, `uv` in the mesh's own space, and
`time`): waves on water, a flag in the wind, a wobble. A point moves at
most half the mesh's size; build what moves from enough triangles
(`geometry.tris`, `g:subdivide`).

**A 2D sprite shader** is `fn sprite(in: Sprite) -> vec4<f32>`, returning
the pixel's colour (straight alpha). It colours the images drawn after
`love.graphics.setShader(s)` (images, quads, sprite batches; shapes and
text draw as usual), until `setShader()`; each frame starts without one.
`love.graphics.getShader()` is the one in use; `shader:hasUniform(name)`
says whether its `struct Uniforms` has `name`; `shader:typeOf("Shader")`.

```wgsl
// shaders/shimmer.wgsl
struct Uniforms { strength: f32 }

fn sprite(in: Sprite) -> vec4<f32> {
    let c = textureSample(mb_tex0, mb_samp, in.uv) * in.color;
    let wave = 0.5 + 0.5 * sin(in.time * 4.0 + in.local.x * 6.0);
    return vec4<f32>(c.rgb + vec3<f32>(wave * uniforms.strength), c.a);
}
```

```lua
local shimmer = love.graphics.newShader("shimmer")
function love.draw()
  love.graphics.setShader(shimmer)
  shimmer:send("strength", 0.3)
  love.graphics.draw(gem, 100, 200)
  love.graphics.setShader()
end
```

`Sprite` has `uv` (in the picture's image, which may be a packed atlas),
`local` (0–1 across the drawn picture), `color` (`setColor`'s), `position`
(canvas units) and `time`.

**Uniforms.** Name them in `struct Uniforms { … }` (floats and `vec2`,
`vec3`, `vec4` of floats, 16 `vec4`s' worth) and read them as
`uniforms.t`. A 3D material's come from its `uniforms` table, read again
every frame, so a write or a tween to the table changes the shader; a
sprite shader's from `shader:send("t", 0.5)`. A `vec3` or `vec4` takes `{
x, y, z }` (or `{ r, g, b }`) or a colour `0xRRGGBB`. Without a `struct
Uniforms`, a shader reads `mb_u[0]`…`mb_u[15]` (`vec4`s), set from a list
(`uniforms = { { 1, 0, 0, 0 } }`, `shader:send("mb_u", { … })`).

**Pictures.** `textures = { "noise", "sand" }` gives a material's shader
`mb_tex0`…`mb_tex3` (images 512 px or more a side), sampled with
`textureSample(mb_tex0, mb_samp, uv)` (repeating, linear). A sprite
shader's `mb_tex0` is the picture it's drawing; `shader:send("tex1",
image)` gives it `mb_tex1`…`mb_tex3`. Pictures are sampled as stored (sRGB
numbers): for a colour picture's light, `pow(c.rgb, vec3<f32>(2.2))`.

**The checks.** What `write_files` refuses, with the line:

- WGSL that doesn't parse or validate (naga's, as WebGPU's);
- a shader over 64 KB, brackets nested over 64 deep, more than 16,384
  expressions, more than 4 KB of a function's own variables;
- its own bindings, storage buffers, atomics, `override`, entry points
  (`@vertex`, `@fragment`, `@compute`), names starting `mb_` but the ones
  above;
- **a cost over 4,000 a pixel.** Each operation costs about 1, `sin`,
  `pow`, `sqrt` and the like 4, a texture sample **256**, and a loop its
  body × its trips: 256 unless it's `for (var i = 0; i < N; i++)` with a
  number for N. `write_files` says each shader's cost and the loop that
  costs most. Every loop stops after 256 trips, whatever it says.

**When the phone is slower than the shader**, it draws the plain material
instead: a frame with many shaded things covering the screen draws the
ones past its budget plain that frame, and a page whose frames take too
long draws every shader material plain from then on (and so does a server
frame past its budget; `play_test` says so). Plan for the plain material:
give it the colour the shader starts from.

**What shaders don't do (yet):** skinned and instanced meshes, the
reflection and the view under water draw a shader material plain; a sprite
shader doesn't reach `Sprite` nodes, only `love.draw`'s images; shadows
don't follow a vertex hook; a game with shaders can't read sensors (tilt,
motion, the microphone).

<!-- names: love.graphics.newShader love.graphics.setShader love.graphics.getShader Shader:send Shader:hasUniform Shader:typeOf -->

## Engine 1.0's 3D names [3d1]

A 3D game also has engine 1.0's 3D names (docs/STUDIO-3D-API.md), so
1.0's 3D games run with almost no change. They work on the same nodes as
the names in [3d] and [models]; new games use those.

The world is in meters: right-handed, +y up, and a camera looks down −z.
Turns are radians, applied y (yaw), then x (pitch), then z (roll).

**Meshes**, built in code (each edit changes the mesh and returns it, so
they chain; a node draws a mesh as it was when the node was made):

| Call | |
|---|---|
| `mesh.box(w, h, d)`, `mesh.sphere(r, segs, rings)`, `mesh.ico(r, subdiv)`, `mesh.cylinder(r, h, segs)`, `mesh.cone(r, h, segs)`, `mesh.plane(w, d)`, `mesh.torus(R, r, segs, sides)`, `mesh.empty()` | Shapes centred on the origin (a plane lies flat, facing +y) |
| `mesh.lathe({ r1, y1, r2, y2, … }, segs)`, `mesh.extrude({ x1, y1, … }, depth)`, `mesh.tris({ x, y, z, … })` | A profile turned around y; a polygon pushed along z; your own triangles, 9 numbers each |
| `m:color(0xRRGGBBAA)`, `m:split(y, below, above)` | Paint every vertex; two colours by height |
| `m:move(x, y, z)`, `m:turn(yaw, pitch, roll)`, `m:scale(x, y, z)`, `m:clamp_y(lo)` | Transform the vertices |
| `m:add(other)`, `m:flat()` | Merge another mesh in (one mesh is one draw); faceted shading |

**Materials:** `material{ color = 0xffc94aff, emissive = 0xffa21f, glow,
metal, rough, unlit, double, alpha, additive, style = { toon, rim, outline,
ink, nofog } }`, and `mat:style{ … }`.

**Nodes:** `node(mesh, material)` draws a mesh (the default material
without one); `node()` is empty (a group, a socket). A node is a 2.0 node:
its properties write through, and the engine sends the nodes that changed
to the GPU once a frame, however many writes they had.

| Property | |
|---|---|
| `x`, `y`, `z` | Its place in its parent's space |
| `rx`, `ry`, `rz` | Its turn: pitch, yaw, roll |
| `scale`; `sx`, `sy`, `sz` | Its size (`scale` sets all three) |
| `vx`, `vy`, `vz`, `gravity` | Meters a second, moved in Rust; `gravity` pulls down (9.8 falls) |
| `vrx`, `vry`, `vrz` | Spin onto `rx`, `ry`, `rz`, radians a second, turned in Rust with the velocity. A spun angle stays within −π..π; writing it spins on from there |
| `visible` | |
| `parent`, `children` | Read-only here: `n:parent(p)` sets it |
| `data` | Your own table |
| `alive` | False once destroyed |

Engine 1.0's methods, on the same node:

| Method | |
|---|---|
| `n:place(x, y, z, yaw, size)` | Where, which way it faces and how big, in one call |
| `n:pos(x, y, z)`, `n:pos()` → x, y, z; `n:turn(yaw, pitch, roll)`; `n:size(s)` | One part of it |
| `n:parent(p)`, `n:show(false)`, `n:destroy()` | Move with `p` (keeping its place in `p`'s space); hide; remove with its children |
| `n:world()` → x, y, z | Where it is after its parents and animation |
| `n:fade(from, to)` | Dither out between those distances from the camera |
| `n:light(color, intensity, range)` | A point light here (8 at most) |
| `n:text("+10", { size, color, outline, width, font = "pixel", billboard, screen, ontop, nofog })` | Text in the world (`""` removes it) |
| `n:instances(list, stride)` | Its mesh many times in one draw: `x, y, z`, then by `stride` `yaw, scale, colour, pitch, roll, tall` |
| `n:find(name)`, `n:material()` | A model's part (a bone, a mesh) by name; the material it draws with |
| `n:play(clip, { once, fade, restart })`, `n:speed(k)`, `n:clip()` | A spawned model's animation |
| `n:createTween()` | Godot's Tween, as on a 2D node ([libraries]) |

flux moves a 3D node's numbers (`flux.to(n, 1, { z = -10, ry = 1 })`),
and a write wins over a tween, as in 2D.

**Models:** `spawn(model("fox"), parent)` spawns `models/fox.glb` and
returns its root node; `fox:find("Socket.Tail")` is a part. A part keeps
the model's pose (showing or hiding it, or its velocity, doesn't change
that) until you write its place, turn or size, which replaces the pose
from then on, as in 1.0. Models load once a session, behind the title
card.

**Particles:** `local sparks = emitter{ colors = { 4 × { r, g, b, a } },
sizes = { 4 }, life = { a, b }, speed = { a, b }, spread, drag, gravity =
{ x, y, z }, stretch, alpha }`, then `sparks:emit(x, y, z, dx, dy, dz,
count)`, with `vx, vy, vz` after it in a frame moving that fast.

**The look:**

| Call | |
|---|---|
| `camera(x, y, z, look_x, look_y, look_z, fov, near, far)` | The 3D camera; `fov` in radians here (about 0.9). It can't roll |
| `project(x, y, z)` → sx, sy, onScreen | Where a world point is on the canvas |
| `sky{ seed, colors = { 3 colours }, nebula, stars, star_brightness, sun_disc }` | The sky, which also lights the scene |
| `sun{ dir = { x, y, z }, color, intensity, shadows = true, shadow_distance }` | The one shadowed light |
| `fog{ color, mode = "exp" \| "exp2" \| "linear" \| "off", density, start, ["end"], height, falloff, sky }` | |
| `post{ exposure, bloom, bloom_threshold, vignette, aberration, saturation, contrast, sharpen }`; `dof{ focus, range, blur, near }`, `dof()` | The final image; depth of field, and off |
| `grade("moonlit", { amount })`, `shafts("moonbeams")`, `halos("mist", { reach })`, `film("subtle", { grain, streaks })` | The beauty pass, by preset |

Colours on this page are 1.0's: `0xRRGGBBAA` (as picked) where there's
alpha, `0xRRGGBB` where there isn't (`emissive`, `light`, the sky, the
sun, the fog), or `{ r, g, b(, a) }` in linear light, which can pass 1 to
glow and bloom.

**Over the 3D:** 2D nodes, `love.draw`, the HUD and the cards draw on top,
in canvas units. 1.0's `text(s, x, y, { size, color = 0xRRGGBBAA, align,
shadow })` and `rect(x, y, w, h, color)` draw this frame (`love.draw`'s
way, from wherever they're called).

**Sound:** `sfx(name, vol, pan, pitch)` plays a sound once; `local v =
loop(name, vol, pan, pitch)` repeats until `v:stop(fade)` (`v:set(vol, pan,
pitch)` changes it); `lowpass("music" | "sfx" | "master", hz, seconds)`
muffles a bus (20000 opens it). `music(name, vol, fade)` is
`music.play`, and `music(nil)` stops it.

**Engine 1.0's round and events**, beside 2.0's, so 1.0's code runs:

| 1.0 | is |
|---|---|
| `score`, `lives` | `round.score`, `round.lives` (reading and writing) |
| `title`, `over` | `round.title`, `round.ended` |
| `game_over(heading)` | `round.over(heading)`, except on the title card or while Invincible is on |
| `function start()` | Runs as a round starts, after `love.load` |
| `time()`, `rnd(n)` | `round.time`; `love.math.random() * n` |
| `on("tick", fn(dt))`, `on("tap", fn(x, y))`, `on("swipe", …)`, `on("start", …)` | As in 2D ([quick]) |
| `on("hold", fn(x, y))`, `on("release", fn(x, y))` | Each frame a finger is down; when it lifts |
| `on("press", name, fn)` | A button: `"left"`, `"right"`, `"up"`, `"down"`, `"a"` (Space, Z, J or the pad's A), `"b"`, `"x"`, `"y"`, `"lb"`, `"rb"`, or `"jump"` for any jump (those, Up and W) |
| `input.x`, `input.y`, `input.down`, `input.jump` | The pointer, whether it's down, a jump pressed this frame (with `input.ax`, `input.ay`) |
| `timer(name, secs, repeats)`, `on("timer", name, fn)`; `after(secs, fn)`, `every(secs, fn)` | hump.timer's `after` and `every` ([libraries]) |
| `save(key, value)`, `load(key)` | `save.set`, `save.get` |
| `hurt(node)` | A life off, without 2D's blink of safety |

The quick path is 2D's (sprites over tile maps): in a 3D game `kind`,
`spawn(kind)`, `spawnEvery`, `all`, `count`, `nearest`, `nextLevel`,
`speedUp` and its events (`on("overlap")` and the rest) say so.

<!-- names: mesh.box mesh.sphere mesh.ico mesh.cylinder mesh.cone mesh.plane mesh.torus mesh.empty mesh.lathe mesh.extrude mesh.tris
Geometry:color Geometry:split Geometry:move Geometry:turn Geometry:scale Geometry:clamp_y Geometry:add Geometry:flat material Material:style node
Node3D.x Node3D.y Node3D.z Node3D.rx Node3D.ry Node3D.rz Node3D.scale Node3D.sx Node3D.sy Node3D.sz Node3D.vx Node3D.vy Node3D.vz Node3D.gravity Node3D.vrx Node3D.vry Node3D.vrz Node3D.visible Node3D.parent Node3D.children Node3D.data Node3D.alive
Node3D:place Node3D:pos Node3D:turn Node3D:size Node3D:parent Node3D:show Node3D:destroy Node3D:world Node3D:fade Node3D:light Node3D:text Node3D:instances Node3D:find Node3D:material Node3D:play Node3D:speed Node3D:clip Node3D:createTween
model emitter Emitter3D:emit project sky sun fog post dof grade shafts halos film text rect sfx loop Voice:set Voice:stop lowpass
score lives title over game_over time rnd input.x input.y input.down input.jump after every load -->

## The Arcade preset [arcade]

`preset = "arcade"` in `studio.toml` keeps engine 1.0's look and limits,
with everything else in 2.0 working inside them. A 1.0 2D game moves up
with it and looks as it did.

```toml
preset = "arcade"
palette = "sunset16"          # or "night16", or 16 "#rrggbb" colours
background = 4                # a palette colour: the canvas and the cards' backdrop
```

- **The world is in art pixels:** 144×256 (256×144 landscape), drawn 2.5
  units a pixel, positions snapped to whole pixels. `size` isn't allowed.
- **0–15 are the palette's colours** wherever a colour goes (`color = 3`,
  `love.graphics.setColor(12)`); bigger numbers are `0xRRGGBB` as usual.
- **Pictures** are PNGs in the 16 colours, every pixel solid or clear
  (`mb build` checks): images 16×16 or 32×32 a frame (square frames side
  by side without a `frame`), up to 8 frames; tiles 16×16. They're pixel
  art, drawn crisp (sampled nearest) unless a `.toml` says `pixel = false`.
- **The pixel font only:** `print`, `Text` (8 units) and `newFont("pixel")`.
- **The quick path's knobs** are 1.0's numbers as they were (art pixels).
- **The cards and HUD** are 1.0's arcade theme in the project's colours,
  in the pixel font, until `identity` names another; the HUD is 1.0's
  too (the score small and white, top centre, and hearts for lives).

A limit error says how to lift it: "Arcade images are 16×16 or 32×32, and
cat.png is 48×48. Without preset = "arcade" any size works."

## Maths [maths]

- **Random numbers** are the round's, seeded by the platform, so a replay
  draws the same ones: `love.math.random()` (0–1), `random(max)` (1 to
  max), `random(min, max)`; `math.random` is the same generator, and
  `math.randomseed` is `love.math.setRandomSeed(seed)`.
  `love.math.randomNormal(stddev, mean)`.
- **`love.math.newRandomGenerator(seed)`** makes another (up to 256 a
  round): `rng:random(…)`, `rng:randomNormal(…)`, `rng:setSeed(seed)`,
  `typeOf`.
- **`love.math.noise(x, y, z, w)`**: smooth noise, about −1 to 1.
- **Polygons:** `love.math.triangulate(…)` (a list of triangles),
  `love.math.isConvex(…)`; points as numbers or one table.
- **Colours:** `love.math.colorFromBytes(r, g, b, a)` (0–255 to 0–1) and
  `colorToBytes`.
- **Ours:** `chance(p)` (true with probability `p`, from the round's
  random numbers), `lerp(a, b, t)`, `clamp(v, lo, hi)`, `dist(x1, y1, x2,
  y2)`, and `angle(x1, y1, x2, y2)`, which is `math.atan2(y2 - y1, x2 -
  x1)` (Luau's `math.atan` takes one argument: [lua]).
- Luau's `math` also has `clamp`, `lerp`, `sign`, `round` and `noise`.

<!-- names: chance lerp clamp dist angle love.math.random love.math.randomNormal love.math.setRandomSeed love.math.newRandomGenerator love.math.noise love.math.triangulate love.math.isConvex love.math.colorFromBytes love.math.colorToBytes math.random math.randomseed RandomGenerator:random RandomGenerator:randomNormal RandomGenerator:setSeed RandomGenerator:typeOf -->

## Making art with generate [generate]

The connector's `generate` tool makes a project's pictures with AI and writes them in as one
change, landing as this engine reads them. Every job makes several options
in one charge (4 on free credits, 2 on Plus), a judge puts the best in
place, and the rest wait under `alternates/` (never packed):
the `generate` tool again with the same kind and name and `pick: N` swaps
another in for nothing. The `estimate` tool says what a job costs first.

| `kind` | Its options | Writes |
|---|---|---|
| `image` | `size` (pixels: `256`, or `"320x480"`; default 256), `pixel`, `stands`, `ref` | `images/<name>.png`, the thing on a clear background |
| `frames` | `frames` (2–8), `motion`, `size` (each frame) | `images/<name>.png`, the frames side by side, and `frame = [w, h]` in its `.toml` |
| `walker` | `height` (units, default 60), `ref` | `images/<name>.png`, a body without feet and one foot, and `walker = true`, `frame`, `body` and `foot` in its `.toml` |
| `background` | `ref`, `key_art` | `images/<name>.jpg`, the screen at 2 pixels a unit, opaque |
| `layers` | `count` (2–4, default 3), `screens` (wide, default 2), `key_art` | `images/<name>_0.jpg` (the far one, opaque) and `_1.png` … (nearer, clear above their shapes), each wrapping sideways |
| `tile` | `size` (default 32), `frame` (from 1: replaces it, or one past the last adds one), `seamless` (default true) | `tiles/<name>.png` and `frame = [w, h]` in `tiles/<name>.toml` |
| `ui` | `size` (default `"256x128"`), `pixel` | `images/ui/<name>.png` (the Lua says `"ui/<name>"`) |
| `icon` | | `icon.png` |
| `sky` | `horizon` (where the painting's horizon is, 0–1; found by default). A 3D game's | `images/<name>.jpg`, 2048×1024 and equirectangular: `world.sky{ panorama = "<name>" }` |
| `model` | Below. A 3D game's | `models/<name>.glb`, its sidecar, and a contact sheet at `alternates/models/<name>/preview.png` |
| `portrait` | `expressions` (after neutral, a row each: at most 7 with neutral), `size` (a cell, square: default 192, pixel art 48), `pixel`, `ref` | `images/portraits/<name>.png`: 4 mouth shapes across (closed to wide open), a row an expression and the blink row last; `frame = [w, h]` and `portrait = ["neutral", …, "blink"]` in its `.toml` ([voice]) |
| `voice` | `mode`: `design`, `cast`, `say` or `script` (below); `name` is the character | `voices/cast.toml`; a line's `voices/<name>/<line>.ogg` and `.lips.txt`; its row in `voices/lines.csv` ([voice]) |
| `music` | `seconds` (a loop, 10–60), and for music that follows the game ([sound]): `zones` (`{ town = "…", cave = "…" }`, 2–6, one take in parts), `sections` (2–6 names), `setlist` (`{ harbor = "…", neon = "…" }`, 2–4 songs, each its own key and tempo), `layers` (`true`: base, high and the tension kit), `bpm` | A loop: `music/<name>.ogg`. Zones or sections: `music/<name>.music.json` with its take, a stinger into each zone, a swell (and with `layers`, the take as `_take_base` and `_take_hi`, and the tension kit). A setlist: `music/<name>.songs.json` and a graph per song (`<name>_<zone>`). `layers` on a loop: `<name>_base`, `_hi`, the tension kit and `<name>.music.json`. Lua plays any of them by name: `music.play("<name>")` |
| `sfx` | `seconds`, `loop` (a 3D game's) | `sounds/<name>.ogg`, on any credits (a synth `.sfx.toml` costs nothing) |

A loop is a few credits (4 options); zones and sections are one long take
on a better model, 2 options, about 16 credits; a setlist about 16 a song.
The `estimate` tool says exactly.

**A model comes in two steps,** as it's the dear kind:

1. `generate(kind = "model", name = "fox", prompt = "a fox adventurer")` makes
   concept pictures to choose from, for a credit or so: an `image` job at
   `images/fox_concept.png`, with options and `pick` as usual.
2. `generate(kind = "model", name = "fox", concept = 2, rig = "biped", animate
   = { "idle", "run" })` builds that one. `from_text = true` builds from the
   prompt with no pictures. The concept leaves `images/` once it's built, and
   its options stay under `alternates/`.

| Option | What it does |
|---|---|
| `object` | `character`, `prop`, `part` or `vehicle` (guessed from the prompt) |
| `rig` | `biped`, `quadruped`, `hexapod`, `octopod`, `avian`, `serpentine`, `aquatic`, or `true` to let it guess |
| `animate` | Clips by name (`idle`, `run`, `jump`…), which rig it. In the Lua they're capitalised: `fox:play("Run")` |
| `clips` | Trim, rename, cut or drop them: `Idle=0..4`, `Strike=Slash@1.8..3.4`, `Swing=Slash`, `Chop=drop` |
| `spine`, `joints` | `auto`, `head-to-tail` or `wings`: a chain of bones for `flight` and `swim` instead of a rig, at no extra cost |
| `polys` | Triangle budget (default 8000) |
| `size` | Its longest side in meters (default 1) |
| `origin`, `facing` | `ground` or `center`; `-z` or `+z` |
| `glow` | Bright parts of its texture glow at this strength (2–4 blooms nicely) |
| `metal`, `min_roughness` | Scale its metalness (0.15 keeps 15%) and raise its roughness (0.6 is satin): shiny metal reads near-black with nothing bright to reflect |
| `rigid`, `pin` | Hard-surface skin (`all`, `arms,legs`); what hangs off a limb (`cape=spine`, `skirt=hips`, `auto`) |

A static model from a concept is about 30 credits, and from text 20. A rig adds
25 and each clip 10, so a biped with idle and run from a concept is 75. The
`estimate` tool says exactly. Every model asks the creator first if they set a
per-job limit. A rigged model takes minutes: the tool answers `still_making`,
and the model lands as its own change. Its contact sheet shows it from every
side, with an arrow the way it faces and a row per clip: look at it before
building on it.

A model that came out facing the wrong way turns in the Lua, for nothing:
`Model{ model = "crane", ry = math.pi }`. The raw download isn't kept, so
changing anything else about a built model means making it again.

A `.glb` written with `write_files` (`content_base64`, with `size` for its
longest side in meters) is fitted the same way: scale, ground, −z, textures and
compression. A model is at most 8 MB.

- **Its look.** Every prompt gets the game's look: `art/style.toml` if
  there is one, else DESIGN.md's Look. Say only what the thing is.
- **Sizes.** One pixel is one unit. Paintings (`background`, `layers`)
  are made at 2 pixels a unit for a sharp phone screen, so draw them at
  `scale = 0.5`. An `image` is the size you ask for: make it twice the
  size it's drawn and set `scale = 0.5`, or set `w` and `h`. A `Walker`
  draws its picture at its `height`, whatever its size.
- **`pixel = true`** is pixel art at one texel an art pixel, sampled crisp:
  a 24-pixel firefly drawn at `scale = 3` stays sharp. `pixel` goes in its
  `.toml`. The Arcade preset makes `image`, `frames`, `tile` and `icon`
  only, at 16 or 32 pixels in the palette.
- **Footing.** `stands = true` on an `image` or `frames` (a figure, a tree,
  a crate) writes `footing = [x, y]` in its `.toml`: where it meets the
  ground, in its frame's pixels. A walker's `body` and `foot` are its
  footing: its `y` is its feet.
- **`.toml`s are kept.** generate sets only its own keys (`frame`,
  `pixel`, `footing`, `walker`, `body`, `foot`) and leaves the rest, so a
  sheet's `[anims]`, behaviours and knobs stay. Choosing another option
  puts its keys in too.
- **Layers.** Far to near, at growing `parallax` (0.1, 0.35, 0.7 for
  three), `repeatX = true`, and lower `order` further back. The judge and
  the card see each option's layers stacked as the game shows them. The
  prompt is the whole scene, sky and all: only the far layer paints it, and
  the nearer ones are its shapes in front (a nearest layer that covers too
  much is made again).
- **Key art.** `key_art = true` names the picture (a layer set's far
  layer) as `art` under `[identity]` in `studio.toml`, for `colors =
  "art"` and the cards ([identity]).
- **`ref`** redraws a picture already in the project: a walker of
  `images/hero.png`, say.

```lua
-- after the generate tool made layers "dusk" (count 3)
-- and a walker "fox" (height 46)
Layer{ image = "dusk_0", parallax = 0.1, repeatX = true, scale = 0.5, order = -30 }
Layer{ image = "dusk_1", parallax = 0.35, repeatX = true, scale = 0.5, order = -20 }
Layer{ image = "dusk_2", parallax = 0.7, repeatX = true, scale = 0.5, order = -10 }
local fox = Walker{ image = "fox", height = 46, x = 80, y = 300 }
```

**Voices** come in four modes, and `name` is always the character, as
`cast("gruk")` says it:

- `generate(kind = "voice", mode = "design", name = "gruk", prompt = "a
  gravelly, bored old troll who loves his own jokes")` makes 3 previews that
  say the same words (3 credits). The judge's pick is gruk's voice in
  `voices/cast.toml`, and `pick = N` (or the card) makes another one the voice
  for nothing. `sample` (100–1000 characters) is what the previews say.
- `mode = "cast", premade = "Jessica"` gives a character one of ElevenLabs'
  premade voices, for nothing (`style` 0–1, `speed` 0.7–1.2). A wrong name
  lists them.
- `mode = "say", line = "halt", text = "Oi. Nobody crosses for free.",
  emotion = "smug"` speaks one line: 2 takes, judged, and `pick` with `line`
  puts the other in. It writes the line's row into `voices/lines.csv`.
- `mode = "script"` speaks every row of `voices/lines.csv`
  (`character,line,text,emotion,tag`) that's new or changed since it was
  spoken (its words, emotion or voice), one take each, at most 40 a call. With
  `name`, only that character's.

Lines cost by their length: about a credit for every 125 characters of a take.
The emotion is sent as an audio tag in front of the line, so name it as the
portrait's expression is named and the face matches; `[audio tags]` inside the
text act it too (`"[laughs] Too slow!"`) and stay out of the subtitle. A
project holds 4 designed voices and an account 12; premade voices don't count,
and deleting a project frees its own. While ElevenLabs' voice slots (which
every creator shares) are full, a design is refused: cast a premade voice
instead.

## Budgets [budgets]

| Budget | Past it |
|---|---|
| Lua time: 1M Luau interrupts a frame (loop back-edges, calls, returns), 2M for `main.lua` and `love.load` | An error naming the loop's line: set goals (flux, `vx`, `follow`; `vr` or `vry` for a steady spin) rather than positions or angles each frame |
| Lua memory: 32 MB | An error: pool things, make fewer tables a frame |
| Nodes: 4,096 live | An error naming the cap: destroy what's gone, or reuse nodes |
| `love.draw`: 32,768 draws a frame | An error: use nodes for what stays |
| Images: 128 a session, each up to 4096×4096 | An error at load |
| Fonts: 16 a game | An error |
| A round's sources 512, text objects 1,024, sprite batches 256, particle systems 64, bump worlds 64, random generators 256, animations 4,096 | An error naming what to reuse |
| Particles: 600 an `Emitter`; a particle system's buffer (default 1,000, up to 2,000) | The oldest go |
| Popups (`fx.popup`): 64 at once | More aren't made |
| 3D: about 200 draws and 150k triangles; a `Terrain` at `reach ÷ detail` = 400 is about 30 and 70k | GPU time: a smaller `reach` or a larger `detail` ([terrain]) |

Built-ins run in Rust and don't count against the Lua budget.

## The Lua dialect [lua]

2.0 runs **Luau**, Roblox's Lua: Lua 5.1 plus `continue`, compound
assignment (`+=`, `-=`, `..=`), string interpolation in backticks, `if …
then … else` expressions, generalized `for k, v in t`, and optional type
annotations. Write plain LÖVE-style Lua; most LÖVE code runs unchanged.

It lacks some of Lua 5.4:

- `goto`;
- integers: every number is a double (`7 // 2` is `3`);
- the bitwise operators: use `bit32` (or `bit`, LuaJIT's name, which is
  the same table);
- `<const>` and `<close>`;
- two-argument `math.atan(y, x)`: it's `math.atan2(y, x)`.

And one trap that fails silently: `string.rep(s, n, sep)` ignores `sep`.

`goto` has no mechanical fix in general; the two common ones are exact:
a `goto` to a label at the end of its function is `return`, and `goto
continue` to a label at the end of a loop's body is `continue`.

The sandbox: `os` is only the round's clock and `exit` ([love]); no `debug`, `io`, `load`, `loadstring`, `getfenv`,
`setfenv`, `collectgarbage` or `newproxy`; the standard library tables
are read only; weak tables and `__gc` are off (a round must play the same
every time). `print` writes to the log ([project]).

<!-- names: bit -->

## Moving up from 1.0 [upgrade]

The `upgrade_engine` tool moves a 1.0 project to 2.0 on a try-it copy
(`try/engine-2`), plays the old and the new version with the same seed,
and says what differs. What it changes by itself:

| 1.0 | 2.0 |
|---|---|
| `engine = "1.0"` | `engine = "2.0"`, and a 2D game gets `preset = "arcade"`, so it keeps its look ([arcade]); a 3D game gets `[identity] name = "night"`, 1.0's 3D cards ([identity]) |
| A 3D game drawing its own score (`text(score, …)`, or what it sets `score` to) | `ui.hud{ score = false }`, so it isn't drawn twice |
| `orientation = "sideways"` | `orientation = "landscape"` |
| `canvas` | Gone: the Arcade preset's canvas is 144×256 art pixels |
| `sprites/` | `images/` |
| Frames from 0 (a tile's `frame`, a sprite's `[anims]`) | From 1 |
| `score`, `lives`, `level` | `round.score`, `round.lives`, `round.level` |
| `game_over(text)` | `round.over(text)` |
| `function start()` | `function love.load()` |
| `save(key, value)`, `load(key)` | `save.set(key, value)`, `save.get(key)` |
| `rnd(n)`, `rnd()` | `love.math.random() * n`, `love.math.random()` |
| `time()` | `round.time` |
| `after(secs, fn)`, `every(secs, fn)` | `timer.after(secs, fn)`, `timer.every(secs, fn)` |
| `next_level`, `speed_up`, `spawn_every` | `nextLevel`, `speedUp`, `spawnEvery` |
| `shake(n)`, `flash(c)`, `particles(kind, x, y)`, `popup(text, x, y)` | `fx.shake`, `fx.flash`, `fx.burst`, `fx.popup` |
| `music(name)`, `music(nil)` | `music.play(name)`, `music.stop()` |
| `on("touch", a, b, fn)` | `on("overlap", a, b, fn)` |

What isn't mechanical, and the `write_files` tool's checks name on the copy:

- `text(s, x, y)` in a tick: `love.graphics.print(s, x, y)` in
  `love.draw`, or a `Text` node made once ([text]).
- `sfx(name)`: make the source once in `love.load`
  (`love.audio.newSource(name, "static")`) and `:play()` it ([sound]).
- `W` and `H`: `love.graphics.getWidth()` and `getHeight()`.
- `hitstop(frames)`: `fx.hitstop(seconds)`.
- Named timers, `timer("next", secs, repeat)` with `on("timer", "next",
  fn)`: `timer.after(secs, fn)` or `timer.every(secs, fn)`.
- `solid_at`, `tile_at`: the level's `Tilemap`, `map:solidAt(x, y)`.

The checks can't tell a thing from a game's own table, so what a game
reads off a thing shows up when it plays, as an error saying what to use:
1.0's `thing.kind` is its `tag` in 2.0, its own values from `spawn(kind,
{ … })` are in `thing.data`, and `thing.anim = "run"` is
`thing:play("run")`.
- Lua 5.4 that Luau lacks ([lua]): `goto`, the bitwise operators,
  two-argument `math.atan`.

A live game whose new version scores its top runs differently needs a new
season when it's published (its score check says).
