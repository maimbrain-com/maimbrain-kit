-- Studio engine 2.0's prelude (docs/STUDIO-2.md §4.1): run once in each
-- VM before the game's main.lua. It makes the game's writable `love` (the
-- engine's is read-only, with love.graphics and the rest in it), `round`
-- and `require`, and returns the engine's two entry points: `load`, which
-- runs love.load, and `step`, which Rust calls once a frame.
--
-- A frame's step, in order: LÖVE's input callbacks for this frame's
-- events; then the libraries' frame in Rust (flux, hump.timer, anim8, the
-- Sprites' animations) and what it hands back to run in Lua (timer and
-- tween callbacks, nodes' signals); then (as later builds add them) `on`
-- handlers and the quick path's behaviours; then love.update(dt); then
-- love.draw(), which the engine records and draws in mb_render.
--
-- A build's own prelude (the 3D build's prelude3d.lua) gets `extra` back
-- to add to the frame: `extra.on` takes the events it knows before `on`
-- does, and `extra.input` runs after LÖVE's input callbacks.
--
-- Also here, in Lua: nodes' signals (Godot's idea), the Godot-style Tween
-- over flux (`node:createTween()`), hump.timer's `script` and `tween`, and
-- `require` of the libraries by their usual names.
--
-- Written for Luau and run as bytecode compiled when the engine is built.

-- The engine's own entry points, handed to the prelude only (never a global);
-- the project's lives; whether this VM is the title card's; whether it's
-- a daily round.
local engine, args, title, daily = ...
local rawget, type, error, select = rawget, type, error, select
local smt = setmetatable
local extra = {}

-- Weak tables and finalizers would make a round depend on when the
-- collector runs. The VM ignores __mode anyway (its lgc.cpp patch); this
-- says so where it's written. (`setmetatable` is in the compiler's
-- disabledBuiltins, so this wrapper is what runs.)
setmetatable = function(t, mt)
  if type(mt) == "table" and (rawget(mt, "__gc") ~= nil or rawget(mt, "__mode") ~= nil) then
    error("weak tables and __gc are off in Studio games (a round must play the same every time)", 2)
  end
  return smt(t, mt)
end

-- LuaJIT's names, which LÖVE code uses.
bit = bit32

-- The game's love: its callbacks go here. Its modules are copies of the
-- engine's (which are read-only), and what LÖVE has that 2.0 doesn't says
-- what to use instead (§5 [love]'s differences).
local ABSENT = {
  filesystem = "love.filesystem isn't in 2.0: assets load by name, and saves are save.get(key) and save.set(key, value)",
  thread = "love.thread isn't in 2.0: a game is one thread",
  event = "love.event isn't in 2.0: the platform owns the loop (end a round with round.over)",
  data = "love.data isn't in 2.0: there's no byte or pixel access in the sandbox",
  image = "love.image isn't in 2.0: there's no pixel access in the sandbox; draw with love.graphics or nodes",
  sound = "love.sound isn't in 2.0: there's no sample access; play sounds with love.audio.newSource(name)",
  video = "love.video isn't in 2.0",
  physics = "love.physics is 2D's: a 3D game's bodies are RigidBody3D, StaticBody3D, VehicleBody3D and Area3D",
  font = "love.font isn't in 2.0: fonts are love.graphics.newFont(\"Family Name\", size), baked by the packer",
}
local MISSING = {
  graphics = {
    stencil = "stencils aren't in 2.0 (mb2d has no clipping)",
    setStencilTest = "stencils aren't in 2.0 (mb2d has no clipping)",
    setScissor = "setScissor isn't in 2.0 (mb2d has no clipping yet)",
    newMesh = "meshes aren't in 2.0's 2D: use love.graphics.polygon, or Shape{ shape = \"poly\" }",
    shear = "shear isn't in 2.0: rotate and scale",
    newShader = "shaders take WGSL in a later phase (shader materials); GLSL never runs",
    setShader = "shaders take WGSL in a later phase (shader materials)",
    newImageData = "there's no pixel access in the sandbox",
    captureScreenshot = "screenshots are the platform's (mb play draws frames)",
  },
  window = {
    setTitle = "the platform owns the window: studio.toml's title",
    setFullscreen = "the platform owns the window",
  },
  timer = { sleep = "love.timer.sleep isn't in 2.0: wait with timer.after(secs, fn)" },
  audio = {
    setEffect = "effects are a bus's in 2.0: audio.bus(\"sfx\").reverb = \"hall\" (or .delay, .lowpass)",
    setDistanceModel = "a source placed with src:setPosition(x, y, z) fades with distance by itself",
    setOrientation = "the round is heard from the camera (love.audio.setPosition moves where)",
  },
  physics = {
    newMotorJoint = "motor joints aren't in 2.0's love.physics: weld, revolute, distance, rope, prismatic, wheel and mouse joints are",
    newGearJoint = "gear joints aren't in 2.0's love.physics: weld, revolute, distance, rope, prismatic, wheel and mouse joints are",
    newPulleyJoint = "pulley joints aren't in 2.0's love.physics: weld, revolute, distance, rope, prismatic, wheel and mouse joints are",
    newFrictionJoint = "friction joints aren't in 2.0's love.physics: use body:setLinearDamping and setAngularDamping",
    getDistance = "love.physics.getDistance isn't in 2.0: measure between the bodies' positions",
  },
}
local engineLove = love
local gameLove = {}
for name, mod in pairs(engineLove) do
  if type(mod) == "table" then
    local copy = {}
    for k, v in pairs(mod) do copy[k] = v end
    local missing = MISSING[name]
    if missing then
      smt(copy, { __index = function(_, k)
        local m = missing[k]
        if m then error("love." .. name .. "." .. tostring(k) .. ": " .. m, 2) end
      end })
    end
    gameLove[name] = copy
  else
    gameLove[name] = mod
  end
end
love = smt(gameLove, { __index = function(_, k)
  local m = ABSENT[k]
  if m then error(m, 2) end
end })

-- args: the project's lives
round = { score = 0, lives = args or 1, time = 0, title = title == true, ended = false, daily = daily == true, level = "" }
-- The other boards (boards.rs): the round's best on one, and a board's leaderboard.
round.post = engine.post
round.board = engine.board
function round.over(heading, cause, won)
  engine.over(heading, cause, won)
  round.ended = true
end

-- Signals: one node's events (sprite.animationFinished, walker.stepped).
local Signal = {}
Signal.__index = Signal
function Signal:connect(fn)
  if type(fn) ~= "function" then error("connect takes a function", 2) end
  self[#self + 1] = fn
  return fn
end
function Signal:disconnect(fn)
  for i = #self, 1, -1 do
    if self[i] == fn then table.remove(self, i) end
  end
end
function Signal:emit(...)
  local list = { unpack(self) }
  for i = 1, #list do list[i](...) end
end
local signals = {}
local function signalFor(node, name)
  local s = signals[node]
  if not s then s = {} signals[node] = s end
  local x = s[name]
  if not x then x = smt({}, Signal) s[name] = x end
  return x
end

-- Godot's Tween, a thin alias over flux (§5 [godot]): node:createTween()
-- :tweenProperty("y", 120, 0.4):setTrans("sine"):setEase("inOut"). Steps
-- run one after another unless the tween is parallel; `finished` is its
-- signal. flux is the real API.
local TRANS = { linear = true, sine = true, quint = true, quart = true, quad = true, expo = true, elastic = true, cubic = true, circ = true, bounce = true, back = true }
local EASE = { ["in"] = "in", out = "out", inOut = "inout", outIn = "inout" }
local Tweener = {}
Tweener.__index = Tweener
local function applyEase(tw)
  if tw.trans == "linear" then tw.f:ease("linear") else tw.f:ease(tw.trans .. tw.ease) end
end
function Tweener:setTrans(t)
  if not TRANS[t] then error("setTrans: \"" .. tostring(t) .. "\" (linear, sine, quad, cubic, quart, quint, expo, circ, back, elastic, bounce)", 2) end
  self.trans = t
  applyEase(self)
  return self
end
function Tweener:setEase(e)
  if not EASE[e] then error("setEase: \"" .. tostring(e) .. "\" (in, out, inOut)", 2) end
  self.ease = EASE[e]
  applyEase(self)
  return self
end
function Tweener:setDelay(d)
  self.f:delay(d)
  return self
end
local Tween = {}
Tween.__index = Tween
local function addStep(self, obj, secs, vars)
  local f
  if self.last and not self.isParallel and not self.together then
    f = self.last:after(obj, secs, vars)
  else
    f = flux.to(obj, secs, vars)
  end
  self.together = false
  self.steps[#self.steps + 1] = f
  self.last = f
  f:oncomplete(function() if self.last == f then self.finished:emit() end end)
  local tw = smt({ f = f, trans = "linear", ease = "inout" }, Tweener)
  applyEase(tw)
  return tw
end
function Tween:tweenProperty(prop, final, secs)
  return addStep(self, self.node, secs, { [prop] = final })
end
function Tween:tweenInterval(secs)
  return addStep(self, {}, secs, {})
end
function Tween:tweenCallback(fn)
  local tw = addStep(self, {}, 0, {})
  tw.f:oncomplete(fn)
  return tw
end
function Tween:setParallel(on)
  self.isParallel = on ~= false
  return self
end
function Tween:parallel()
  self.together = true
  return self
end
function Tween:kill()
  for i = 1, #self.steps do self.steps[i]:stop() end
  self.steps, self.last = {}, nil
end
local nodeMethods = {
  createTween = function(node)
    return smt({ node = node, steps = {}, finished = smt({}, Signal) }, Tween)
  end,
}
engine.hooks(signalFor, nodeMethods)

-- hump.timer's script and tween, in Lua over its timers and flux.
local function humpEase(m)
  if m == nil or m == "linear" then return "linear" end
  local kind = m:gsub("^in%-out%-", ""):gsub("^out%-in%-", ""):gsub("^in%-", ""):gsub("^out%-", "")
  if m:find("^in%-out%-") or m:find("^out%-in%-") then return kind .. "inout" end
  if m:find("^out%-") then return kind .. "out" end
  return kind .. "in"
end
timer = smt({
  script = function(f)
    local co = coroutine.wrap(f)
    co(function(t) timer.after(t, co) coroutine.yield() end)
  end,
  tween = function(secs, subject, target, method, after, ...)
    local t = flux.to(subject, secs, target):ease(humpEase(method))
    if after then local args = { ... } t:oncomplete(function() after(unpack(args)) end) end
    return t
  end,
}, { __index = timer })

-- require: the game's game/<name>.lua, else a library by its usual name.
local hump = smt({ new = function(x, y, zoom, rot)
  if x then camera:lookAt(x, y) end
  if zoom then camera:zoomTo(zoom) end
  if rot then camera:rotateTo(rot) end
  return camera
end }, { __call = function(self, ...) return self.new(...) end })
local builtin = {
  flux = function() return flux end,
  anim8 = function() return anim8 end,
  bump = function() return bump end,
  classic = function() return Object end,
  ["hump.timer"] = function() return timer end,
  timer = function() return timer end,
  ["hump.camera"] = function() return hump end,
}
local modules = {}
function require(name)
  if type(name) ~= "string" then error("require takes a module name: require(\"enemies\") loads game/enemies.lua", 2) end
  local m = modules[name]
  if m ~= nil then return m end
  local ok, chunk = pcall(engine.chunk, name)
  if not ok then
    local key = name:gsub("/", ".")
    for lib, get in pairs(builtin) do
      if key == lib or key:sub(-#lib - 1) == "." .. lib then
        modules[name] = get()
        return modules[name]
      end
    end
    error(chunk, 2)
  end
  local r = chunk()
  if r == nil then r = true end
  modules[name] = r
  return r
end

-- The one controller, as LÖVE's callbacks pass it (love.joystick).
local pad = engine.joystick()

-- Ours (§5 [input]): one steering axis from the arrows, WASD, the d-pad and
-- the stick, and the controller's style for glyphs.
input = { ax = 0, ay = 0 }
local function call(name, ...)
  local f = love[name]
  if f then f(...) end
end

-- Taps and swipes (`on`), below.
local pointer

-- One event: { name, args..., n = count }.
local function deliver(e)
  local name = e[1]
  if name == "keypressed" then call("keypressed", e[2], e[2], false)
  elseif name == "keyreleased" then call("keyreleased", e[2], e[2])
  elseif name == "mousepressed" or name == "mousereleased" then
    call(name, e[2], e[3], e[4], e[5], 1)
    if e[4] == 1 then pointer(name, e[2], e[3]) end
  elseif name == "mousemoved" then call(name, e[2], e[3], e[4], e[5], e[6])
  elseif name == "touchpressed" or name == "touchmoved" or name == "touchreleased" then call(name, e[2], e[3], e[4], e[5], e[6], e[7])
  elseif name == "gamepadpressed" or name == "gamepadreleased" then call(name, pad, e[2])
  elseif name == "gamepadaxis" then call(name, pad, e[2], e[3])
  elseif name == "wheelmoved" then call(name, e[2], e[3])
  elseif name == "resize" then call(name, e[2], e[3])
  end
end

-- Maths (§5 [maths]): ours, as 1.0 had them.
function chance(p) return love.math.random() < p end
function lerp(a, b, t) return a + (b - a) * t end
function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function dist(x1, y1, x2, y2) local dx, dy = x2 - x1, y2 - y1 return math.sqrt(dx * dx + dy * dy) end
function angle(x1, y1, x2, y2) return math.atan2(y2 - y1, x2 - x1) end

-- The game's events (§5 [scene], [quick]): handlers run in the order
-- they were added. The quick path's events come from Rust each frame.
local handlers = { tick = {}, tickKind = {}, leave = {}, overlap = {}, swipe = {}, tap = {}, start = {},
  flap = {}, jump = {}, land = {}, hurt = {}, pickup = {}, shoot = {}, beat = {}, bar = {} }
local LATER = {
  fixed = "on(\"fixed\") comes with physics, in a later engine build",
}
local RENAMED = {
  touch = "on(\"overlap\", tagA, tagB, fn)",
  timer = "timer.after(secs, fn) or timer.every(secs, fn)",
  press = "love.gamepadpressed(joystick, button) or love.keypressed(key)",
  hold = "love.mouse.isDown(1) in love.update",
  release = "love.mousereleased(x, y)",
  update = "love.update(dt)",
  draw = "love.draw()",
}
local EVENTS = "overlap, tick, leave, tap, swipe, start, flap, jump, land, hurt, pickup, shoot, beat, bar, cue"
local function need(f, what)
  if type(f) ~= "function" then error(what .. " needs a function", 3) end
  return f
end
local function add(list, f) list[#list + 1] = f end
function on(ev, a, b, c)
  if extra.on and extra.on(ev, a, b, c) then return end
  if ev == "overlap" then
    if type(a) ~= "string" or type(b) ~= "string" then error("on(\"overlap\", tagA, tagB, fn) takes two tags (\"*\" is any)", 2) end
    need(c, "on(\"overlap\", a, b, …)")
    local i = engine.watch("overlap", a, b)
    handlers.overlap[i] = handlers.overlap[i] or {}
    add(handlers.overlap[i], c)
  elseif ev == "tick" and type(a) ~= "function" then
    if type(a) ~= "string" then error("on(\"tick\", fn) or on(\"tick\", kind, fn)", 2) end
    need(b, "on(\"tick\", kind, …)")
    engine.watch("tick", a)
    handlers.tickKind[a] = handlers.tickKind[a] or {}
    add(handlers.tickKind[a], b)
  elseif ev == "leave" then
    if type(a) ~= "string" then error("on(\"leave\", kind, fn)", 2) end
    need(b, "on(\"leave\", kind, …)")
    engine.watch("leave", a)
    handlers.leave[a] = handlers.leave[a] or {}
    add(handlers.leave[a], b)
  elseif ev == "cue" then
    if type(a) ~= "string" then error("on(\"cue\", name, fn): a cutscene's { \"cue\", name } step fires it", 2) end
    engine.voiceCue(a, need(b, "on(\"cue\", name, …)"))
  elseif handlers[ev] and ev ~= "tickKind" then
    add(handlers[ev], need(a, "on(\"" .. ev .. "\", …)"))
  elseif LATER[ev] then
    error(LATER[ev], 2)
  elseif RENAMED[ev] then
    error("on(\"" .. ev .. "\") is engine 1.0's: use " .. RENAMED[ev], 2)
  else
    error("unknown event '" .. tostring(ev) .. "' (" .. EVENTS .. ")", 2)
  end
end
local function fire(ev, a, b)
  local list = handlers[ev]
  for i = 1, #list do list[i](a, b) end
  engine.sfx(ev, a, b)
end

-- hurt(node): one life off, then a blink of safety (§5 [rounds]); never
-- while the debug menu's Invincible is on. At 0 lives the engine ends the round.
function hurt(node)
  -- Once the round has ended its lives are what the card shows (the world
  -- steps on under the card, as engine 1.0's didn't).
  if round.ended or round.lives <= 0 or not engine.safe(node) then return false end
  round.lives = round.lives - 1
  fire("hurt", node)
  return true
end

-- Voice and dialogue (§5 [voice], api/voice.rs). The engine runs the box,
-- the portraits and the cutscene; what's done comes back in the libraries'
-- frame. A cast member is a handle: .expression, .health, :hit(amount).
-- A mistake is the game's line's, not this file's.
local function voice(f, ...)
  local ok, r = pcall(f, ...)
  if not ok then error((tostring(r):gsub("^prelude%.lua:%d+: ", "")), 3) end
  return r
end
local casts = {}
function cast(name, opts)
  if type(name) ~= "string" then error("cast(name, { portrait = \"gruk\", side = \"right\", name = \"GRUK\" })", 2) end
  if opts ~= nil then
    if type(opts) ~= "table" then error("cast(name, opts): opts is a table ({ portrait, side, name, expression })", 2) end
    voice(engine.voiceCast, name, opts)
  elseif not casts[name] then
    voice(engine.voiceCast, name)
  end
  local c = casts[name]
  if not c then
    local function hit(_, amount) voice(engine.voiceHit, name, amount) end
    c = smt({}, {
      __index = function(_, k)
        if k == "hit" then return hit end
        return voice(engine.voiceGet, name, k)
      end,
      __newindex = function(_, k, v) voice(engine.voiceSet, name, k, v) end,
      __tostring = function() return "cast " .. name end,
    })
    casts[name] = c
  end
  return c
end
-- One line in the box; its done signal fires when it's finished or tapped on
-- (with the answer, for a line with choices).
function say(who, line, expression)
  local l = { done = smt({}, Signal), who = who, line = line }
  voice(engine.voiceSay, who, line, expression, function(choice) l.done:emit(choice) end)
  return l
end
function dialogue(t)
  if type(t) ~= "table" then error("dialogue{ { \"gruk\", \"halt\", \"smug\" }, done = function(choice) end }", 2) end
  voice(engine.voiceDialogue, t)
end
function bark(tag) return voice(engine.voiceBark, tag) end
function narrate(who, line, expression) voice(engine.voiceNarrate, who, line, expression) end
function cutscene(t)
  if type(t) ~= "table" then error("cutscene{ { \"letterbox\", 0.12 }, { \"say\", \"gruk\", \"halt\" }, done = fn }", 2) end
  voice(engine.voiceCutscene, t)
end
function talking() return engine.voiceBusy() end


-- The quick path's frame: { name, a, b or false, value }.
local function quickEvents(list, dt)
  for i = 1, #list do
    local e = list[i]
    local name, a, b, v = e[1], e[2], e[3], e[4]
    if b == false then b = nil end
    if name == "pickup" then
      -- Under the game-over card nothing scores: the round's score was sent.
      if not round.ended then
        round.score = round.score + v
        fire("pickup", a, b)
        if b and b.alive then b:destroy() end
      end
    elseif name == "hazard" then
      hurt(a)
    elseif name == "gone" then
      if a.alive then a:destroy() end
    elseif name == "overlap" then
      local hs = handlers.overlap[v]
      if hs then
        for j = 1, #hs do
          if a.alive and b.alive then hs[j](a, b) end
        end
      end
    elseif name == "tick" then
      local hs = handlers.tickKind[a.tag]
      if hs then for j = 1, #hs do if a.alive then hs[j](a, dt) end end end
    elseif name == "leave" then
      local hs = handlers.leave[a.tag]
      if hs then for j = 1, #hs do hs[j](a) end end
      if a.alive then a:destroy() end
      if v == 1 and not hs then round.over(nil, "leave") end
    else
      fire(name, a, b)
    end
  end
end

-- A quick drag is a swipe.
local press
pointer = function(name, x, y)
  if name == "mousepressed" then
    press = { x, y, round.time }
    fire("tap", x, y)
  elseif name == "mousereleased" and press then
    local dx, dy = x - press[1], y - press[2]
    if round.time - press[3] < 0.35 and dx * dx + dy * dy > 40 * 40 then fire("swipe", dx, dy) end
    press = nil
  end
end

local function load()
  call("load")
  local list = handlers.start
  for i = 1, #list do list[i]() end
end

-- What the libraries' frame hands back, in order: { f, timer, args…, n }
-- or { "signal", node, name, args…, n }. A timer's function that returns
-- false stops it (hump.timer's every).
local function run(due)
  local stopped
  for i = 1, #due do
    local e = due[i]
    if e[1] == "signal" then
      local s = signals[e[2]]
      s = s and s[e[3]]
      if s then s:emit(unpack(e, 4, e.n)) end
    else
      local t = e[2]
      if not (t and stopped and stopped[t]) then
        local r = e[1](unpack(e, 3, e.n))
        if t and r == false then
          stopped = stopped or {}
          stopped[t] = true
          engine.timerCancel(t)
        end
      end
    end
  end
end

-- round.subject as the engine last heard it (the readability check's node).
local subject
local function step(dt, events, time, title, ended)
  round.time, round.title = time, title
  if ended then round.ended = true end
  input.ax, input.ay, input.pad = engine.steer()
  for i = 1, #events do deliver(events[i]) end
  if extra.input then extra.input(events, dt) end
  run(engine.update(dt))
  -- The music's beats due by now, on its grid in game time (api/music.rs):
  -- { beat, bar, at, … }; a downbeat is heard by on("bar") first.
  local b = engine.beats()
  if b then
    local bars, beats = handlers.bar, handlers.beat
    for i = 1, #b, 3 do
      local beat, bar, at = b[i], b[i + 1], b[i + 2]
      if beat == 1 then for j = 1, #bars do bars[j](bar, at) end end
      for j = 1, #beats do beats[j](beat, bar, at) end
    end
  end
  quickEvents(engine.quick(dt), dt)
  round.level = engine.level() or ""
  local ticks = handlers.tick
  for i = 1, #ticks do ticks[i](dt) end
  call("update", dt)
  engine.ui()
  engine.drawing(true)
  call("draw")
  engine.drawing(false)
  if round.subject ~= subject then
    subject = round.subject
    engine.subject(subject)
  end
  -- round.endDelay: at 0 lives the round ends that many seconds later
  -- (real time, at most 10), so the game's own ending plays first.
  local delay = round.endDelay
  if delay ~= nil and (type(delay) ~= "number" or delay ~= delay) then
    error("round.endDelay is seconds (a number, at most 10) or nil, got " .. tostring(delay), 0)
  end
  return round.score, round.lives, delay or 0
end

return load, step, extra
