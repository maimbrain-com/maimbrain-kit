-- Studio engine 2.0's own API (everything not under `love.` and not one of
-- LÖVE's libraries), as type definitions for luau-analyze. See
-- love.d.luau's header: the same rules and the same test.

-- A colour: 0xRRGGBB, or { r, g, b[, a] } from 0 to 1.
type Color = number | { number }

-- One node's event (Godot's signals): sprite.animationFinished:connect(fn).
declare extern type Signal with
    function connect(self, fn: (...any) -> ...any): (...any) -> ...any
    function disconnect(self, fn: (...any) -> ...any): ()
    function emit(self, ...: any): ()
end

-- One step of a Godot-style Tween (tween:tweenProperty returns it).
declare extern type Tweener with
    function setTrans(self, trans: "linear" | "sine" | "quad" | "cubic" | "quart" | "quint" | "expo" | "circ" | "back" | "elastic" | "bounce"): Tweener
    function setEase(self, ease: "in" | "out" | "inOut" | "outIn"): Tweener
    function setDelay(self, secs: number): Tweener
end

-- node:createTween(): Godot's Tween over flux.
declare extern type Tween with
    finished: Signal
    function tweenProperty(self, property: string, final: number, secs: number): Tweener
    function tweenInterval(self, secs: number): Tweener
    function tweenCallback(self, fn: () -> ()): Tweener
    function setParallel(self, parallel: boolean?): Tween
    function parallel(self): Tween
    function kill(self): ()
end

-- What every node has.
declare extern type Node with
    x: number
    y: number
    rotation: number
    scale: number
    sx: number
    sy: number
    vx: number
    vy: number
    gravity: number
    vr: number
    alpha: number
    visible: boolean
    color: Color
    order: number
    fixed: boolean
    parent: Node?
    children: { Node }
    name: string
    tag: string
    data: { [any]: any }
    alive: boolean
    -- Foley (docs/STUDIO-2-API.md [sound]): a loop that follows the node's speed.
    hum: Hum?
    -- A love.physics Body it follows, drawn between steps (2D; nil: none).
    body: Body?
    function destroy(self): ()
    function find(self, name: string): Node?
    function worldPosition(self): (number, number)
    function createTween(self): Tween
end

declare extern type Sprite extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    image: string?
    frame: number
    anchorX: number
    anchorY: number
    flipX: boolean
    flipY: boolean
    w: number
    h: number
    animationFinished: Signal
    function play(self, anim: string, options: { once: boolean?, speed: number?, restart: boolean? }?): ()
end

declare extern type Shape extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    shape: "rect" | "circle" | "poly" | "line"
    w: number
    h: number
    radius: number
    points: { number }
    corner: number
    line: number
end

declare extern type Text extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    text: string
    font: string
    size: number
    align: "left" | "center" | "right"
    width: number
    w: number
    h: number
end

declare extern type Layer extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    image: string?
    parallax: number
    repeatX: boolean
    repeatY: boolean
    fill: Color?
end

declare extern type Group extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
end

declare extern type Tilemap extends Node with
    map: string?
    tiles: string?
    w: number
    h: number
    function solidAt(self, x: number, y: number): boolean
    function tileAt(self, x: number, y: number): string?
    function marker(self, name: string): (number, number)
    function path(self, x1: number, y1: number, x2: number, y2: number, options: { diagonal: boolean? }?): { number }?
end

-- A character in a love.physics world (Phase 3b): a node whose vx, vy only
-- moveAndSlide moves it by (Godot's CharacterBody2D).
declare extern type CharacterBody2D extends Node with
    floorMaxAngle: number
    floorSnapLength: number
    maxStepHeight: number
    floorStopOnSlope: boolean
    floorBlockOnWall: boolean
    wallMinSlideAngle: number
    slideOnCeiling: boolean
    function moveAndSlide(self, dropThrough: boolean?): boolean
    function isOnFloor(self): boolean
    function isOnFloorOnly(self): boolean
    function isOnWall(self): boolean
    function isOnWallOnly(self): boolean
    function isOnCeiling(self): boolean
    function getFloorNormal(self): (number, number)
    function getWallNormal(self): (number, number)
    function getFloorAngle(self): number
    function getRealVelocity(self): (number, number)
    function getSlideCollisionCount(self): number
    function getSlideCollision(self, i: number): { nx: number, ny: number }
end
declare function CharacterBody2D(props: {
    world: PhysicsWorld, shape: { w: number?, h: number?, radius: number? }, x: number?, y: number?, name: string?, tag: string?,
    floorMaxAngle: number?, floorSnapLength: number?, maxStepHeight: number?, floorStopOnSlope: boolean?, floorBlockOnWall: boolean?,
    wallMinSlideAngle: number?, slideOnCeiling: boolean?,
}): CharacterBody2D

declare extern type Walker extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    image: string?
    height: number
    speed: number
    flipX: boolean
    w: number
    h: number
    stepped: Signal
    footsteps: string?
end

declare extern type Emitter extends Node with
    behaviour: Behaviour?
    behaviours: { Behaviour }
    knobs: { [string]: number }
    preset: "sparkles" | "confetti" | "stars" | "burst" | "sparks" | "ring" | "dust" | "smoke"
    rate: number
    colors: Color | { Color }
    size: number?
    life: number?
    speed: number?
    spread: number?
    additive: boolean
    emitting: boolean
    function emit(self, count: number?): ()
end

-- A node constructor's table: any of the kind's properties, and `parent`.
type NodeProps = {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    hum: Hum?,
}

declare function Sprite(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    image: string?, frame: number?, anchorX: number?, anchorY: number?, flipX: boolean?, flipY: boolean?,
    w: number?, h: number?,
}?): Sprite
declare function Shape(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    shape: ("rect" | "circle" | "poly" | "line")?, w: number?, h: number?, radius: number?, points: { number }?,
    corner: number?, line: number?,
}?): Shape
declare function Text(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    text: (string | number)?, font: string?, size: number?, align: ("left" | "center" | "right")?, width: number?,
}?): Text
declare function Layer(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    image: string?, parallax: number?, repeatX: boolean?, repeatY: boolean?, fill: Color?,
}?): Layer
declare function Group(props: NodeProps?): Group
declare function Tilemap(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    map: string, tiles: string?,
}): Tilemap
declare function Walker(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    image: string?, height: number?, speed: number?, flipX: boolean?, footsteps: string?, hum: Hum?,
}?): Walker
declare function Emitter(props: {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    preset: ("sparkles" | "confetti" | "stars" | "burst" | "sparks" | "ring" | "dust" | "smoke")?,
    rate: number?, colors: (Color | { Color })?, size: number?, life: (number | { number })?, speed: (number | { number })?, spread: number?,
    additive: boolean?, emitting: boolean?,
    -- In a 3D game (docs/STUDIO-2-API.md [3d]): z, sizes in meters, life and speed { min, max }, drag.
    z: number?, sizes: { number }?, drag: number?,
}?): Emitter

declare scene: {
    find: (name: string) -> Node?,
    all: (tag: string) -> { Node },
}

-- The one camera (hump.camera's API, run in Rust).
declare extern type Camera with
    x: number
    y: number
    scale: number
    rot: number
    -- A 3D game's camera (docs/STUDIO-2-API.md [3d]): z, fov in degrees, roll in radians.
    z: number
    fov: number
    roll: number
    near: number
    far: number
    function lookAt(self, x: number, y: number, z: number?): ()
    function move(self, dx: number, dy: number): ()
    function zoomTo(self, scale: number): ()
    function rotateTo(self, angle: number): ()
    function attach(self): ()
    function detach(self): ()
    function worldCoords(self, sx: number, sy: number): (number, number)
    function cameraCoords(self, x: number, y: number): (number, number)
    function follow(self, node: (Node | Node3D)?, options: { lag: number?, lead: number?, deadzone: { w: number?, h: number? }?, bounds: (Tilemap | { x: number?, y: number?, w: number?, h: number? })?, behind: number?, above: number?, lookAhead: number?, bank: number? }?): ()
    function project(self, x: number, y: number, z: number): (number, number, boolean)
    function ray(self, sx: number, sy: number): (number, number, number, number, number, number)
    -- A 3D game also calls it: engine 1.0's camera(x, y, z, look_x, look_y, look_z, fov, near, far), fov in radians.
    function __call(self, x: number?, y: number?, z: number?, lookX: number?, lookY: number?, lookZ: number?, fov: number?, near: number?, far: number?): ()
end

declare camera: Camera

declare fx: {
    -- In a 3D game, with z after y: (preset, x, y, z, options).
    burst: (preset: string, x: number, y: number, zOrOptions: (number | { count: number?, color: (Color | { Color })? })?, options: { count: number?, color: (Color | { Color })? }?) -> (),
    popup: (text: string | number, x: number, y: number, zOrOptions: (number | { color: Color?, size: number? })?, options: { color: Color?, size: number? }?) -> (),
    flash: (color: Color?, secs: number?) -> (),
    shake: (amount: number?, secs: number?) -> (),
    hitstop: (secs: number?) -> (),
    -- The world at `rate` of its speed for `secs`, easing in and back (1 eases back now).
    slowmo: (rate: number?, secs: number?) -> (),
    -- A size kick (0.2 is 20% bigger), or { x, y, z, scale }: out and springing back.
    punch: (node: Node | Node3D, amount: (number | { x: number?, y: number?, z: number?, scale: number? })?) -> (),
}

-- Heads for the values on a spring; call again to retarget.
declare function spring(target: Node | Node3D | Camera | Bus | { [string]: any }, goals: { [string]: number }, options: { frequency: number?, damping: number? }?): ()

-- `node.hum`: a sound's name, or { sound, idle, full } (speeds).
type Hum = string | { sound: string, idle: number?, full: number? }

-- A mixing bus (docs/STUDIO-2-API.md [sound]): music and sfx feed master.
declare extern type Bus with
    name: string
    volume: number
    lowpass: number
    highpass: number
    reverb: (string | { wet: number?, decay: number?, damping: number?, predelay: number? })?
    delay: { time: number?, feedback: number?, wet: number?, damping: number? }?
end

declare audio: {
    bus: (name: "master" | "music" | "sfx") -> Bus,
    impact: (src: Source, speed: number, fullSpeed: number?) -> boolean,
}

declare extern type MusicPlayer with
    -- A loop (music/<name>.ogg), a music graph (<name>.music.json) or a setlist (<name>.songs.json).
    play: (name: string, options: { fade: number?, volume: number?, zone: string? }?) -> ()
    stop: (fade: number?) -> ()
    mood: (mood: { intensity: number?, danger: number? }) -> ()
    go: (zone: string, options: { change: ("auto" | "nextBar" | "stinger" | "crossfade")?, bars: number? }?) -> ()
    nearMiss: () -> ()
    -- On the round's clock: seconds since bar 1, the bar (from 1), the beat in it (1 to 4.99…).
    time: number
    bar: number
    beat: number
    bpm: number
    zone: string?
    playing: boolean
    -- A 3D game's: engine 1.0's music(name, vol, fade).
    function __call(self, name: string?, volume: number?, fade: number?): ()
end

declare music: MusicPlayer

declare function haptic(kind: "tap" | "success" | "fail" | "heavy"): ()

declare round: {
    score: number,
    lives: number,
    time: number,
    title: boolean,
    ended: boolean,
    daily: boolean,
    level: string,
    -- What the player must see at a glance, for play_test's readability check.
    subject: (Node | Node3D)?,
    -- At 0 lives the round ends this many seconds later (real time, at most 10), so the game's own ending plays first.
    endDelay: number?,
    over: (heading: string?, cause: string?, won: boolean?) -> (),
    -- The round's best on another board (its id in studio.toml), sent when the round ends.
    post: (board: string, value: number) -> (),
    -- A board's leaderboard (the round's own without an id).
    board: (board: string?) -> (),
}

declare input: {
    ax: number,
    ay: number,
    pad: string?,
    -- A 3D game's, as engine 1.0 had them: the pointer, whether it's down, a jump pressed this frame.
    x: number,
    y: number,
    down: boolean,
    jump: boolean,
}

-- sensors.scope's crosshair: aimed by pointing the phone (the attitude, else tilt), and by a finger (drag).
declare extern type Scope with
    x: number
    y: number
    source: ("attitude" | "tilt" | "drag")?
    function recenter(self): ()
    function drag(self, dx: number, dy: number): ()
end

-- Each needs its sensor in studio.toml's `sensors`; nil while unavailable.
declare sensors: {
    tilt: () -> (number?, number?),
    attitude: () -> (number?, number?, number?),
    motion: () -> (number?, number?, number?),
    loudness: () -> number?,
    scope: (opts: { precise: boolean?, w: number?, h: number? }?) -> Scope,
}

-- Maths helpers (the prelude's).
declare function chance(p: number): boolean
declare function lerp(a: number, b: number, t: number): number
declare function clamp(v: number, lo: number, hi: number): number
declare function dist(x1: number, y1: number, x2: number, y2: number): number
declare function angle(x1: number, y1: number, x2: number, y2: number): number

-- A thing's or a node's behaviours (the quick path).
type Behaviour = "player.drag" | "player.paddle" | "player.tilt" | "player.flappy" | "player.runner" | "player.platformer" | "enemy.patrol" | "enemy.chase" | "enemy.shooter" | "bullet" | "pickup" | "hazard" | "solid" | "spawner"

-- Voice and dialogue ([voice]): lines from voices/lines.csv by speaker and id.
declare extern type CastMember with
    expression: string
    health: number
    function hit(self, amount: number?): ()
end
type CastOptions = { portrait: string?, side: ("left" | "right")?, name: string?, expression: string? }
declare function cast(name: string, opts: CastOptions?): CastMember
declare function say(who: string, line: string, expression: string?): { done: Signal, who: string, line: string }
-- { { who, line, expression?, choices = { … }? }, …, done = function(choice) end, advance = "tap" | "auto" }
declare function dialogue(lines: { [any]: any }): ()
declare function bark(tag: string): boolean
declare function narrate(who: string, line: string, expression: string?): ()
-- Steps in order: { "letterbox", share }, { "camera", x, y, zoom, secs, ease }, { "shot", x, y, z, tx, ty, tz, secs },
-- { "say", who, line, expression }, { "wait", secs }, { "cue", name }, { "title", heading, sub, secs },
-- { "fade", "in" | "out", secs }, { "credits", { { role, name }, … }, secs }, "meanwhile"; done = fn.
declare function cutscene(steps: { [any]: any }): ()
declare function talking(): boolean

-- The game's events (`on`).
declare function on(event: string, a: any, b: any?, c: any?): ()
declare function hurt(node: Node): boolean
declare function kind(name: string, props: {
    w: number?, h: number?, color: Color?, shape: ("rect" | "circle")?, image: string?,
    behaviour: Behaviour?, behaviours: { Behaviour }?, knobs: { [string]: number }?,
    hitbox: { number }?, spawns: string?, order: number?,
}?): ()
-- spawn(kind, props) in a 2D game; spawn(model("fox"), parent) in a 3D one.
declare spawn: ((kind: string, props: { [string]: any }?) -> Node) & ((model: ModelFile, parent: Node3D?) -> Node3D)
declare function spawnEvery(kind: string, secs: number, props: { [string]: number }?): { stop: (self: any) -> () }
declare function all(kind: string?): { Node }
declare function count(kind: string): number
declare function nearest(kind: string, x: number, y: number): Node?
declare function nextLevel(name: string?): ()
declare function speedUp(f: number): ()

-- A value saves can keep.
type Saved = number | string | boolean | { [any]: any }

declare extern type SaveStore with
    get: (key: string) -> Saved?
    set: (key: string, value: Saved?) -> ()
    -- A 3D game's: engine 1.0's save(key, value).
    function __call(self, key: string, value: Saved?): ()
end

declare save: SaveStore

declare progress: {
    get: (key: string) -> Saved?,
    set: (key: string, value: Saved?) -> (),
    checkpoint: (state: Saved) -> (),
    resume: () -> Saved?,
    clear: () -> (),
}

declare debug: {
    toggle: (label: string, fn: (on: boolean) -> (), on: boolean?) -> (),
    number: (label: string, value: number, fn: (value: number) -> (), min: number?, max: number?, step: number?) -> (),
    choice: (label: string, options: { string }, fn: (option: string, index: number) -> (), index: number?) -> (),
    action: (label: string, fn: () -> ()) -> (),
    invincible: () -> boolean,
    mark: (label: string) -> (),
}

-- A HUD element: on or off, a value of the game's, or one of its nodes.
type HudElement = boolean | number | string | Node
type HudElements = { score: HudElement?, best: HudElement?, lives: HudElement?, timer: HudElement?, meter: HudElement?, combo: HudElement?, level: HudElement? }

type IconName = "play" | "pause" | "restart" | "star" | "heart" | "trophy" | "crown" | "lock" | "unlock" | "check" | "close" | "plus" | "minus" | "up" | "down" | "left" | "right" | "home" | "gear" | "sound" | "mute" | "music" | "coin" | "bolt" | "flag" | "clock" | "medal" | "fire" | "gem" | "shield" | "target" | "person" | "bomb"
type Tone = "plain" | "accent" | "good" | "bad" | "gold"

-- A widget (ui.button, ui.panel, ui.pill, ui.bar, ui.ring, ui.icon): a node
-- drawn in the identity, fixed to the screen unless fixed = false.
declare extern type Widget extends Node with
    text: string
    value: number
    icon: IconName?
    enabled: boolean
    tone: Tone
    kind: "primary" | "secondary" | "ghost"
    focus: boolean
    tap: (() -> ())?
    w: number
    h: number
end

type WidgetProps = {
    x: number?, y: number?, rotation: number?, scale: number?, sx: number?, sy: number?,
    vx: number?, vy: number?, gravity: number?, vr: number?, alpha: number?, visible: boolean?, color: Color?,
    order: number?, fixed: boolean?, parent: Node?, name: string?, tag: string?, data: { [any]: any }?,
    w: number?, h: number?, text: (string | number)?, icon: IconName?, tone: Tone?, value: number?, r: number?, size: number?,
    enabled: boolean?, kind: ("primary" | "secondary" | "ghost")?, focus: boolean?, tap: (() -> ())?,
}

declare extern type Hint with
    function dismiss(self): ()
    function showing(self): boolean
end

declare extern type Lesson with
    function learned(self): ()
    function showing(self): boolean
end

declare extern type Combo with
    count: number
    multiplier: number
    best: number
    function hit(self): number
    function reset(self): ()
end

type HintProps = { text: string, at: (Node | Node3D | { number })?, priority: ("ambient" | "teach" | "threat")?, ask: ("tap" | "hold" | "let go" | "look")?, linger: number? }
type LessonProps = { prompt: string, at: (Node | Node3D | { number })?, slow: number?, hold: number?, learned: (() -> boolean)? }
type StatsRow = { any }
type StatsProps = { title: string?, rows: { StatsRow }?, rank: (string | number | boolean)?, next: ({ string } | string)?, wipe: ("iris" | "slide" | "shutter")?, tap: boolean?, done: (() -> ())? }
type UiColor = "text" | "dim" | "accent" | "onAccent" | "panel" | "good" | "bad" | "gold" | "outline" | "background"

declare ui: {
    hud: (elements: HudElements) -> (),
    identity: (nameOrWords: string) -> (),
    button: (props: WidgetProps?) -> Widget,
    panel: (props: WidgetProps?) -> Widget,
    pill: (props: WidgetProps?) -> Widget,
    bar: (props: WidgetProps?) -> Widget,
    ring: (props: WidgetProps?) -> Widget,
    icon: (name: IconName, props: WidgetProps?) -> Widget,
    countdown: (from: number, fn: (() -> ())?) -> (),
    hint: (props: HintProps) -> Hint,
    lesson: (props: LessonProps) -> Lesson,
    stats: (props: StatsProps) -> (),
    combo: (props: { window: number?, hud: boolean? }?) -> Combo,
    color: (name: UiColor) -> number,
}

-- 3D games (kind = "3d"): engine 1.0's 3D names (docs/STUDIO-2-API.md [3d]).
-- Colours here are 1.0's: 0xRRGGBBAA where there's alpha, 0xRRGGBB where
-- there isn't, or { r, g, b[, a] } in linear light (over 1 glows).
type Color3D = number | { number }

-- A mesh built in code: each edit changes it and returns it.
declare extern type Geometry with
    function color(self, rgba: number): Geometry
    function split(self, y: number, below: number, above: number): Geometry
    function move(self, x: number, y: number, z: number): Geometry
    function turn(self, yaw: number, pitch: number?, roll: number?): Geometry
    function scale(self, x: number, y: number?, z: number?): Geometry
    function clamp_y(self, lo: number): Geometry
    function add(self, other: Geometry): Geometry
    function flat(self): Geometry
    -- 2.0's edits (colours 0xRRGGBB).
    function merge(self, other: Geometry, x: number?, y: number?, z: number?, rx: number?, ry: number?, rz: number?, s: number?): Geometry
    function paint(self, color: Color): Geometry
    function smooth(self, crease: number?): Geometry
    function subdivide(self, n: number?): Geometry
end

type MaterialStyle = { toon: number?, rim: number?, outline: number?, ink: Color3D?, nofog: boolean? }

declare extern type Material with
    function style(self, style: MaterialStyle): Material
end

-- A model file (models/<name>.glb), to spawn.
declare extern type ModelFile with
end

-- 1.0's emitter{} and, in a 3D game, 2.0's Emitter{} node (its look's fields).
declare extern type Emitter3D with
    preset: string
    rate: number
    colors: any
    sizes: { number }
    life: { number }
    speed: { number }
    spread: number
    drag: number
    gravity: number
    additive: boolean
    emitting: boolean
    -- 1.0's e:emit(x, y, z, dx, dy, dz, count, …); 2.0's e:emit(n, { dx, dy, dz }).
    function emit(self, a: number, b: any?, z: number?, dx: number?, dy: number?, dz: number?, count: number?, vx: number?, vy: number?, vz: number?): ()
end

-- loop(name)'s sound.
declare extern type Voice with
    function set(self, volume: number?, pan: number?, pitch: number?): ()
    function stop(self, fade: number?): ()
end

-- A 3D node: write-through properties, and engine 1.0's methods. (Its
-- `parent` reads as a property; n:parent(p) sets it.)
declare extern type Node3D with
    x: number
    y: number
    z: number
    rx: number
    ry: number
    rz: number
    scale: number
    sx: number
    sy: number
    sz: number
    vx: number
    vy: number
    vz: number
    gravity: number
    vrx: number
    vry: number
    vrz: number
    visible: boolean
    children: { Node3D }
    data: { [any]: any }
    alive: boolean
    function place(self, x: number, y: number, z: number, yaw: number?, size: number?): ()
    function pos(self, x: (number | { number })?, y: number?, z: number?): (number, number, number)
    function turn(self, yaw: number, pitch: number?, roll: number?): ()
    function size(self, x: number, y: number?, z: number?): ()
    function parent(self, parent: Node3D?): ()
    function show(self, on: boolean?): ()
    function destroy(self): ()
    function world(self): (number, number, number)
    function fade(self, from: number, to: number): ()
    function light(self, color: Color3D, intensity: number, range: number): boolean
    function text(self, text: string | number, options: { size: number?, color: Color3D?, outline: Color3D?, width: number?, font: string?, billboard: boolean?, screen: boolean?, ontop: boolean?, nofog: boolean? }?): ()
    function instances(self, list: { number }, stride: number?): boolean
    function find(self, name: string): Node3D?
    function material(self): Material?
    function play(self, clip: string?, options: { once: boolean?, fade: number?, restart: boolean?, speed: number?, layer: string?, mask: (string | { string })? }?): ()
    function speed(self, k: number): ()
    function clip(self): string?
    function createTween(self): Tween
    -- 2.0's (docs/STUDIO-2-API.md [3d]).
    name: string
    tag: string
    fade: { number }?
    wake: boolean
    hum: Hum?
    function faceDirection(self, dx: number, dz: number): ()
    function lookAt(self, x: number, y: number, z: number): ()
    function worldPosition(self): (number, number, number)
    function instance(self, list: { number }, stride: number?): boolean
end

-- 2.0's 3D nodes: three.js's names (docs/STUDIO-2-API.md [3d], [models]).
-- Colours are 2.0's: 0xRRGGBB or { r, g, b } from 0 to 1.
type Node3DProps = {
    x: number?, y: number?, z: number?, rx: number?, ry: number?, rz: number?, scale: number?, sx: number?, sy: number?, sz: number?,
    vx: number?, vy: number?, vz: number?, gravity: number?, vrx: number?, vry: number?, vrz: number?, visible: boolean?, parent: Node3D?, name: string?, tag: string?,
    data: { [any]: any }?, fade: { number }?, wake: boolean?, hum: Hum?,
}

declare extern type Mesh extends Node3D with
    geometry: Geometry
    material: Material?
end

declare extern type Model extends Node3D with
    model: string
    clip: string?
    clipTime: number
    plantFeet: boolean
    gait: string?
    flight: any
    swim: any
    animationFinished: Signal
    stepped: Signal
    footsteps: string?
    function play(self, clip: string?, options: { once: boolean?, fade: number?, restart: boolean?, speed: number?, layer: string?, mask: (string | { string })? }?): ()
    function bone(self, name: string): Node3D?
    function reach(self, bone: string, x: number?, y: number?, z: number?): ()
    function headLook(self, x: number?, y: number?, z: number?, options: { limit: number? }?): ()
    function restyle(self, style: { toon: number?, outline: number?, rim: number?, ink: Color?, fog: boolean? }): ()
end

declare extern type Light extends Node3D with
    color: number
    intensity: number
    range: number
end

declare extern type Trail extends Node3D with
    color: number
    width: number
    length: number
end


declare extern type Text3D extends Node3D with
    text: string
    size: number
    color: number
    billboard: boolean
    font: string
end

declare function Mesh(props: Node3DProps & { geometry: Geometry, material: Material? }): Mesh
declare function Model(props: Node3DProps & { model: string, footsteps: string? }): Model
declare function Light(props: (Node3DProps & { color: Color?, intensity: number?, range: number? })?): Light
declare function Trail(props: (Node3DProps & { color: Color?, width: number?, length: number? })?): Trail

-- Terrain (docs/STUDIO-2-API.md [terrain]): the ground, from noise and stamps.
declare extern type Terrain extends Node3D with
    progress: number
    built: Signal
    function heightAt(self, x: number, z: number): number
    function normalAt(self, x: number, z: number): (number, number, number)
    function raise(self, x: number, z: number, radius: number, amount: number): ()
end

type TerrainStamp = { any }
type GroundSurface = "grass" | "meadow" | "dirt" | "rock" | "snow"
type SurfaceImage = string | { image: string, normal: string?, size: number? }
declare function Terrain(props: {
    noise: ({ seed: number?, octaves: number?, frequency: number?, ridged: number?, warp: number? } | boolean)?,
    height: number?, stamps: { TerrainStamp }?,
    paint: { { surface: GroundSurface?, color: Color?, above: number?, below: number?, slope: number?, blend: number? } }?,
    surfaces: { grass: SurfaceImage?, meadow: SurfaceImage?, dirt: SurfaceImage?, rock: SurfaceImage?, snow: SurfaceImage? }?,
    reach: number?, detail: number?, fog: boolean?, size: number?, collision: boolean?,
    texture: (string | boolean)?, textureSize: number?, name: string?, tag: string?, visible: boolean?,
}?): Terrain

-- 3D physics (docs/STUDIO-2-API.md [physics]): Godot's bodies, areas,
-- vehicles and joints, as 3D nodes. A shape is made once and given to a
-- body as its `shape` (one, or a list).
declare extern type Shape3D with
end

type ShapeAt = { x: number?, y: number?, z: number?, rx: number?, ry: number?, rz: number? }
declare function BoxShape3D(props: (ShapeAt & { w: number?, h: number?, d: number? })?): Shape3D
declare function SphereShape3D(props: (ShapeAt & { radius: number? })?): Shape3D
declare function CapsuleShape3D(props: (ShapeAt & { radius: number?, height: number? })?): Shape3D
declare function CylinderShape3D(props: (ShapeAt & { radius: number?, height: number? })?): Shape3D
declare function ConvexPolygonShape3D(props: ShapeAt & { geometry: Geometry }): Shape3D
declare function ConcavePolygonShape3D(props: ShapeAt & { geometry: Geometry }): Shape3D
declare function HeightMapShape3D(props: any): never

declare extern type StaticBody3D extends Node3D with
    shape: Shape3D?
    collisionLayer: number
    collisionMask: number
    friction: number
    bounce: number
end

declare extern type RigidBody3D extends Node3D with
    shape: Shape3D?
    collisionLayer: number
    collisionMask: number
    friction: number
    bounce: number
    mass: number
    gravityScale: number
    linearDamp: number
    angularDamp: number
    contactMonitor: boolean
    sleeping: boolean
    freeze: boolean
    canSleep: boolean
    continuousCd: boolean
    lockRotation: boolean
    bodyEntered: Signal
    bodyExited: Signal
    function applyImpulse(self, x: number, y: number, z: number, px: number?, py: number?, pz: number?): ()
    function applyCentralImpulse(self, x: number, y: number, z: number): ()
    function applyForce(self, x: number, y: number, z: number, px: number?, py: number?, pz: number?): ()
    function applyCentralForce(self, x: number, y: number, z: number): ()
    function applyTorque(self, x: number, y: number, z: number): ()
    function applyTorqueImpulse(self, x: number, y: number, z: number): ()
    function getCollidingBodies(self): { Node3D }
end

declare extern type VehicleBody3D extends Node3D with
    shape: Shape3D?
    collisionLayer: number
    collisionMask: number
    friction: number
    bounce: number
    mass: number
    gravityScale: number
    linearDamp: number
    angularDamp: number
    contactMonitor: boolean
    sleeping: boolean
    freeze: boolean
    canSleep: boolean
    continuousCd: boolean
    lockRotation: boolean
    bodyEntered: Signal
    bodyExited: Signal
    function applyImpulse(self, x: number, y: number, z: number, px: number?, py: number?, pz: number?): ()
    function applyCentralImpulse(self, x: number, y: number, z: number): ()
    function applyForce(self, x: number, y: number, z: number, px: number?, py: number?, pz: number?): ()
    function applyCentralForce(self, x: number, y: number, z: number): ()
    function applyTorque(self, x: number, y: number, z: number): ()
    function applyTorqueImpulse(self, x: number, y: number, z: number): ()
    function getCollidingBodies(self): { Node3D }
    engineForce: number
    brake: number
    steering: number
    speed: number
end

declare extern type VehicleWheel3D extends Node3D with
    useAsTraction: boolean
    useAsSteering: boolean
    wheelRadius: number
    wheelRestLength: number
    suspensionTravel: number
    suspensionStiffness: number
    suspensionMaxForce: number
    dampingCompression: number
    dampingRelaxation: number
    wheelFrictionSlip: number
    engineForce: number
    brake: number
    steering: number
    function isInContact(self): boolean
end

declare extern type Area3D extends Node3D with
    shape: Shape3D?
    collisionLayer: number
    collisionMask: number
    monitoring: boolean
    bodyEntered: Signal
    bodyExited: Signal
    areaEntered: Signal
    areaExited: Signal
    function getOverlappingBodies(self): { Node3D }
    function getOverlappingAreas(self): { Area3D }
    function overlapsBody(self, body: Node3D): boolean
end

declare extern type Joint3D extends Node3D with
    nodeA: Node3D
    nodeB: Node3D
    breakForce: number
    breakTorque: number
    force: number
    torque: number
    limitLower: number
    limitUpper: number
    motorVelocity: number
    motorMaxImpulse: number
    broken: Signal
end

type BodyProps = Node3DProps & { shape: (Shape3D | { Shape3D })?, collisionLayer: number?, collisionMask: number?, friction: number?, bounce: number? }
type RigidProps = BodyProps & {
    mass: number?, gravityScale: number?, linearDamp: number?, angularDamp: number?, contactMonitor: boolean?, sleeping: boolean?,
    freeze: boolean?, canSleep: boolean?, continuousCd: boolean?, lockRotation: boolean?,
}
type JointProps = Node3DProps & { nodeA: Node3D, nodeB: Node3D, breakForce: number?, breakTorque: number?, limitLower: number?, limitUpper: number?, motorVelocity: number?, motorMaxImpulse: number? }
declare function StaticBody3D(props: BodyProps): StaticBody3D
declare function RigidBody3D(props: RigidProps): RigidBody3D
declare function VehicleBody3D(props: RigidProps & { engineForce: number?, brake: number?, steering: number? }): VehicleBody3D
declare function VehicleWheel3D(props: Node3DProps & {
    useAsTraction: boolean?, useAsSteering: boolean?, wheelRadius: number?, wheelRestLength: number?, suspensionTravel: number?,
    suspensionStiffness: number?, suspensionMaxForce: number?, dampingCompression: number?, dampingRelaxation: number?,
    wheelFrictionSlip: number?, engineForce: number?, brake: number?, steering: number?,
}): VehicleWheel3D
declare function Area3D(props: BodyProps & { monitoring: boolean? }): Area3D
declare function PinJoint3D(props: JointProps): Joint3D
declare function HingeJoint3D(props: JointProps): Joint3D
declare function SliderJoint3D(props: JointProps): Joint3D
declare function ConeTwistJoint3D(props: JointProps): Joint3D
declare function Generic6DOFJoint3D(props: JointProps): Joint3D

-- Characters and navigation (Phase 3b): Godot's CharacterBody3D,
-- NavigationRegion3D and NavigationAgent3D.
type SlideCollision3D = { node: Node3D?, x: number, y: number, z: number, nx: number, ny: number, nz: number }
declare extern type CharacterBody3D extends Node3D with
    shape: Shape3D?
    collisionLayer: number
    collisionMask: number
    floorMaxAngle: number
    floorSnapLength: number
    maxStepHeight: number
    floorStopOnSlope: boolean
    floorBlockOnWall: boolean
    wallMinSlideAngle: number
    slideOnCeiling: boolean
    function moveAndSlide(self): boolean
    function isOnFloor(self): boolean
    function isOnFloorOnly(self): boolean
    function isOnWall(self): boolean
    function isOnWallOnly(self): boolean
    function isOnCeiling(self): boolean
    function getFloorNormal(self): (number, number, number)
    function getWallNormal(self): (number, number, number)
    function getFloorAngle(self): number
    function getRealVelocity(self): (number, number, number)
    function getSlideCollisionCount(self): number
    function getSlideCollision(self, i: number): SlideCollision3D
end

declare extern type NavigationRegion3D extends Node3D with
    size: number?
    baked: boolean
    progress: number
    bakeFinished: Signal
    function bake(self): ()
    function path(self, x1: number, y1: number, z1: number, x2: number, y2: number, z2: number): { number }?
end

declare extern type NavigationAgent3D extends Node3D with
    maxSpeed: number
    maxAcceleration: number
    radius: number
    height: number
    pathDesiredDistance: number
    targetDesiredDistance: number
    avoidance: boolean
    autoMove: boolean
    targetReached: Signal
    navigationFinished: Signal
    function setTargetPosition(self, x: number, y: number, z: number): ()
    function getNextPathPosition(self): (number, number, number)
    function isNavigationFinished(self): boolean
    function isTargetReached(self): boolean
    function isTargetReachable(self): boolean
    function distanceToTarget(self): number
    function getFinalPosition(self): (number, number, number)
end

type CharacterProps = Node3DProps & {
    shape: Shape3D, collisionLayer: number?, collisionMask: number?, floorMaxAngle: number?, floorSnapLength: number?,
    maxStepHeight: number?, floorStopOnSlope: boolean?, floorBlockOnWall: boolean?, wallMinSlideAngle: number?, slideOnCeiling: boolean?,
}
declare function CharacterBody3D(props: CharacterProps): CharacterBody3D
declare function NavigationRegion3D(props: (Node3DProps & {
    size: number?, bake: { agentRadius: number?, agentHeight: number?, maxSlope: number?, maxClimb: number?, cellSize: number?, cellHeight: number? }?,
})?): NavigationRegion3D
declare function NavigationAgent3D(props: (Node3DProps & {
    maxSpeed: number?, maxAcceleration: number?, radius: number?, height: number?, pathDesiredDistance: number?,
    targetDesiredDistance: number?, avoidance: boolean?, autoMove: boolean?,
})?): NavigationAgent3D

type RayHit = { node: Node3D, x: number, y: number, z: number, nx: number, ny: number, nz: number, distance: number }
declare physics: {
    raycast: (x1: number, y1: number, z1: number, x2: number, y2: number, z2: number, options: { mask: number?, exclude: (Node3D | { Node3D })?, areas: boolean? }?) -> RayHit?,
    setGravity: (x: number, y: number, z: number) -> (),
    getGravity: () -> (number, number, number),
}

declare extern type AnimationTree with
    state: string?
    model: Model
    function travel(self, state: string): ()
    function set(self, parameter: string, value: number | boolean): ()
    function get(self, parameter: string): number
end

type AnimState = { clip: string?, once: boolean?, next: string?, fade: number?, speed: number?, blend: { [string]: number | { number } }?, by: (string | { string })? }
declare function AnimationTree(props: {
    model: Model, start: string?, states: { [string]: AnimState },
    transitions: { { from: string?, to: string, when: string?, fade: number? } }?,
}): AnimationTree

declare geometry: {
    box: (w: number?, h: number?, d: number?) -> Geometry,
    sphere: (r: number?, segments: number?, rings: number?) -> Geometry,
    ico: (r: number?, subdivisions: number?) -> Geometry,
    rock: (size: number?, seed: number?) -> Geometry,
    cylinder: (r: number?, h: number?, segments: number?) -> Geometry,
    cone: (r: number?, h: number?, segments: number?) -> Geometry,
    plane: (w: number?, d: number?) -> Geometry,
    torus: (R: number?, r: number?, segments: number?, sides: number?) -> Geometry,
    lathe: (profile: { number }, segments: number?) -> Geometry,
    extrude: (polygon: { number }, depth: number) -> Geometry,
    tris: (list: { number }) -> Geometry,
    loft: (props: { points: { number }, sides: number?, rings: number?, radius: number | ((t: number, u: number) -> number), open: boolean?, caps: boolean?, ends: number? }) -> Geometry,
    fish: (props: { length: number?, height: number?, fins: number? }?) -> Geometry,
}

type Preset = string
declare world: {
    sky: (props: { atmosphere: ("day" | "dusk" | "night")?, clouds: (boolean | string | { cover: number? })?, disc: ("sun" | "moon")?, panorama: string?, panoramaIntensity: number?, stars: number?, nebula: number? }) -> (),
    sun: (props: { dir: { number }?, color: Color?, intensity: number?, shadows: boolean?, shadowDistance: number? }) -> (),
    lighting: (preset: "hero") -> (),
    fog: (props: { color: Color?, mode: ("exp" | "exp2" | "linear" | "off")?, density: number?, start: number?, ["end"]: number?, height: number?, falloff: number?, sky: number? }) -> (),
    post: (props: { exposure: number?, bloom: number?, bloomThreshold: number?, vignette: number?, aberration: number?, saturation: number?, contrast: number?, sharpen: number? }) -> (),
    dof: (props: { focus: number?, range: number?, blur: number?, near: number? }?) -> (),
    speedBlur: (props: { amount: number?, x: number?, y: number?, motion: { number }?, stretch: number? }) -> (),
    grade: (preset: Preset, amount: number?) -> (),
    shafts: (preset: Preset) -> (),
    halos: (preset: Preset) -> (),
    film: (preset: Preset) -> (),
    water: (props: { preset: ("lake" | "pond" | "canal")?, height: number?, size: number?, flow: { x: number?, z: number?, speed: number? }? }?) -> (),
    retro: (preset: Preset) -> (),
    backdrop: (layers: { { image: string, scroll: number?, y: number?, fill: boolean? } }?) -> (),
}

declare function scatter(node: Node3D, props: { count: number?, area: (number | { w: number, d: number })?, seed: number?, scale: { number }?, tint: { Color }?, turn: boolean?, on: Terrain? }): ()
declare function shockwave(x: number, y: number, z: number, radius: number?, strength: number?, secs: number?): ()
declare function splash(x: number, y: number, z: number, size: number?): ()

declare mesh: {
    box: (w: number?, h: number?, d: number?) -> Geometry,
    sphere: (r: number?, segments: number?, rings: number?) -> Geometry,
    ico: (r: number?, subdivisions: number?) -> Geometry,
    cylinder: (r: number?, h: number?, segments: number?) -> Geometry,
    cone: (r: number?, h: number?, segments: number?) -> Geometry,
    plane: (w: number?, d: number?) -> Geometry,
    torus: (R: number?, r: number?, segments: number?, sides: number?) -> Geometry,
    empty: () -> Geometry,
    lathe: (profile: { number }, segments: number?) -> Geometry,
    extrude: (polygon: { number }, depth: number) -> Geometry,
    tris: (list: { number }) -> Geometry,
}

-- three.js's keys (colours 0xRRGGBB or { r, g, b } from 0 to 1), or 1.0's
-- (metal, rough, glow, double, alpha, style; colours 0xRRGGBBAA).
declare function material(props: {
    color: Color3D?, emissive: Color3D?, glow: number?, metal: number?, rough: number?,
    unlit: boolean?, double: boolean?, alpha: boolean?, additive: boolean?, style: MaterialStyle?,
    emissiveIntensity: number?, metalness: number?, roughness: number?, texture: string?, doubleSided: boolean?,
    transparent: boolean?, opacity: number?, toon: number?, rim: number?, outline: number?, ink: Color?, fog: boolean?,
    -- A shader (shaders/<name>.wgsl), its uniforms (a table the engine reads each frame) and up to 4 images.
    shader: string?, uniforms: { [any]: any }?, textures: { string }?,
}?): Material
declare function node(mesh: Geometry?, material: Material?): Node3D
declare function model(name: string): ModelFile
declare function emitter(props: {
    colors: { Color3D }?, sizes: { number }?, life: { number }?, speed: { number }?,
    spread: number?, drag: number?, gravity: { number }?, stretch: number?, alpha: boolean?,
}): Emitter3D
declare function project(x: number, y: number, z: number): (number, number, boolean)

declare function sky(props: { seed: number?, colors: { Color3D }?, nebula: number?, stars: number?, star_brightness: number?, sun_disc: number? }): ()
declare function sun(props: { dir: { number }?, color: Color3D?, intensity: number?, shadows: boolean?, shadow_distance: number? }): ()
declare function fog(props: { color: Color3D?, mode: ("exp" | "exp2" | "linear" | "off")?, density: number?, start: number?, ["end"]: number?, height: number?, falloff: number?, sky: number? }): ()
declare function post(props: { exposure: number?, bloom: number?, bloom_threshold: number?, vignette: number?, aberration: number?, saturation: number?, contrast: number?, sharpen: number? }): ()
declare function dof(props: { focus: number?, range: number?, blur: number?, near: number? }?): ()
declare function grade(preset: "moonlit" | string, options: { amount: number? }?): ()
declare function shafts(preset: "moonbeams" | string): ()
declare function halos(preset: "mist" | string, options: { reach: number? }?): ()
declare function film(preset: "subtle" | string, options: { grain: number?, streaks: number? }?): ()

-- Drawn this frame over the 3D, in canvas units; colours 0xRRGGBBAA.
declare function text(s: any, x: number, y: number, options: { size: number?, color: number?, align: ("left" | "center" | "right")?, shadow: boolean? }?): ()
declare function rect(x: number, y: number, w: number, h: number, color: number): ()

declare function sfx(name: string, volume: number?, pan: number?, pitch: number?): ()
declare function loop(name: string, volume: number?, pan: number?, pitch: number?): Voice
declare function lowpass(bus: "music" | "sfx" | "master", hz: number, seconds: number?): ()

-- Engine 1.0's round, in a 3D game: score and lives are round.score and
-- round.lives; title and over read round.title and round.ended.
declare score: number
declare lives: number
declare title: boolean
declare over: boolean
declare function game_over(heading: string?): ()
declare function time(): number
declare function rnd(n: number?): number
declare function load(key: string): Saved?
declare function after(secs: number, fn: () -> ()): TimerHandle
declare function every(secs: number, fn: () -> ()): TimerHandle
