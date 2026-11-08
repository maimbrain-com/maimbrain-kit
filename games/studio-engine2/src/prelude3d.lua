-- The 3D build's prelude (docs/STUDIO-2.md §9.2 row 1b): run after
-- prelude.lua in a 3D game's VM, before main.lua. It carries the rest of
-- engine 1.0's 3D page (docs/STUDIO-3D-API.md) onto 2.0's: its round
-- (`score`, `lives`, `title`, `over`, `game_over`, `start`, `time`), its
-- events (`on("hold")`, `on("release")`, `on("press", button)`,
-- `on("timer", name)`), its timers (`timer(name, secs)`, `after`,
-- `every`), saves (`save(key, value)`, `load(key)`), `rnd`, `music(name)`
-- and its `input` fields. The 3D scene's names themselves are Rust
-- (api/three.rs). A 2.0 game uses LÖVE's callbacks and 2.0's names beside
-- them; these stay as aliases (Decided 7).
--
-- `score` and `lives` are round.score and round.lives, and `title` and
-- `over` read round.title and round.ended: the VM's globals do that
-- (vm.rs, api/mod.rs), since a global can't alias a field in Lua.
--
-- The quick path is 2D's (it moves sprites over tile maps), so a 3D game's
-- quick-path names say what to use instead.

local engine, extra = ...
local type, error, tostring = type, error, tostring

-- The round, as 1.0 had it -------------------------------------------------------
-- game_over(heading): ends the round (never on the title card, nor while
-- the debug menu's Invincible is on).
function game_over(heading)
  if round.title or debug.invincible() then return end
  if not round.ended then round.over(heading) end
end
-- Seconds since the round started, in game time.
function time() return round.time end
-- A decimal in [0, n), seeded by the round.
function rnd(n) return love.math.random() * (n or 1) end

-- hurt(node): a life off (2D's blink of safety is a sprite's; a 3D game
-- draws its own). Never while Invincible is on.
local hurtHandlers = {}
function hurt(node)
  if debug.invincible() then return false end
  round.lives = round.lives - 1
  for i = 1, #hurtHandlers do hurtHandlers[i](node) end
  return true
end

-- save(key, value) and load(key): save.set and save.get.
local saved = save
save = setmetatable({}, { __index = saved, __call = function(_, k, v) saved.set(k, v) end })
function load(k) return saved.get(k) end

-- music(name, vol, fade) loops a track across rounds; music(nil) fades it out.
local tunes = music
music = setmetatable({}, { __index = tunes, __call = function(_, name, vol, fade)
  if name == nil then tunes.stop(fade) else tunes.play(name, { volume = vol, fade = fade }) end
end })

-- Events and timers ----------------------------------------------------------------
local handlers = { hold = {}, release = {}, press = {}, timer = {} }
-- timer("wave", 5) fires on("timer", "wave") in 5 s; timer("wave", 5, true) every 5 s (hump.timer's).
local hump = timer
local function fireTimer(name)
  local list = handlers.timer
  for i = 1, #list do if list[i][1] == name then list[i][2]() end end
end
timer = setmetatable({}, { __index = hump, __call = function(_, name, secs, repeats)
  if type(secs) ~= "number" or secs <= 0 then error("timer needs a positive number of seconds", 2) end
  local f = function() fireTimer(name) end
  if repeats then hump.every(secs, f) else hump.after(secs, f) end
end })
-- after(secs, fn) once; every(secs, fn) over and over.
function after(secs, f)
  if type(secs) ~= "number" or secs < 0 then error("after needs a number of seconds", 2) end
  return hump.after(secs, f)
end
function every(secs, f)
  if type(secs) ~= "number" or secs <= 0 then error("every needs a positive number of seconds", 2) end
  return hump.every(secs, function() f() end)
end

-- 1.0's events, which prelude.lua's `on` hands here first; the quick
-- path's are 2D's.
local QUICK = { overlap = true, leave = true, flap = true, jump = true, land = true, pickup = true, shoot = true }
local QUICK_HELP = " is the quick path's, which is 2D (sprites over tile maps): a 3D game makes things with node(mesh, material) and moves them in love.update or with flux"
extra.on = function(ev, a, b)
  if ev == "hold" or ev == "release" or ev == "hurt" then
    if type(a) ~= "function" then error("on(\"" .. ev .. "\", fn) needs a function", 3) end
    local list = ev == "hurt" and hurtHandlers or handlers[ev]
    list[#list + 1] = a
    return true
  elseif ev == "press" or ev == "timer" then
    if type(a) ~= "string" then error("on(\"" .. ev .. "\", name, fn) needs a name", 3) end
    if type(b) ~= "function" then error("on(\"" .. ev .. "\", name, fn) needs a function", 3) end
    local list = handlers[ev]
    list[#list + 1] = { a, b }
    return true
  elseif QUICK[ev] or (ev == "tick" and type(a) == "string") then
    error("on(\"" .. tostring(ev) .. "\")" .. QUICK_HELP, 3)
  end
  return false
end

-- The quick path's functions.
local function quick2d(name)
  return function() error(name .. QUICK_HELP, 2) end
end
kind = quick2d("kind")
spawnEvery = quick2d("spawnEvery")
all = quick2d("all")
count = quick2d("count")
nearest = quick2d("nearest")
nextLevel = quick2d("nextLevel")
speedUp = quick2d("speedUp")

-- start(): 1.0's, as a round starts, after love.load (and before on("start")'s).
on("start", function()
  if type(start) == "function" then start() end
end)

-- What the pointer, keys and pad are doing, as 1.0's input had it (with
-- ax, ay and pad from prelude.lua): x, y and down for the finger or mouse,
-- and jump, pressed this frame (Space, Z, J, Up or W, or a pad's face or
-- shoulder buttons).
input.x, input.y, input.down, input.jump = 0, 0, false, false
-- 1.0's button names for on("press", name): the direction, "a" for Space,
-- Z and J, and the pad's own (a, b, x, y, lb, rb). "jump" is any of the jumps.
local KEYS = { left = "left", a = "left", right = "right", d = "right", up = "up", w = "up", down = "down", s = "down", space = "a", z = "a", j = "a" }
local PADS = { dpleft = "left", dpright = "right", dpup = "up", dpdown = "down", a = "a", b = "b", x = "x", y = "y", leftshoulder = "lb", rightshoulder = "rb" }
local JUMPS = { up = true, a = true, b = true, x = true, y = true, lb = true, rb = true }
local pressed = {}
extra.input = function(events)
  local released, rx, ry = false, 0, 0
  local n = 0
  for i = 1, #events do
    local e = events[i]
    local name = e[1]
    if name == "mousepressed" and e[4] == 1 then
      input.down, input.x, input.y = true, e[2], e[3]
    elseif name == "mousemoved" then
      input.x, input.y = e[2], e[3]
    elseif name == "mousereleased" and e[4] == 1 then
      input.down, input.x, input.y = false, e[2], e[3]
      released, rx, ry = true, e[2], e[3]
    elseif name == "keypressed" or name == "gamepadpressed" then
      local p = (name == "keypressed" and KEYS or PADS)[e[2]]
      if p then n = n + 1 pressed[n] = p end
    end
  end
  local jump = false
  for i = 1, n do if JUMPS[pressed[i]] then jump = true end end
  input.jump = jump
  if input.down then
    local list = handlers.hold
    for i = 1, #list do list[i](input.x, input.y) end
  end
  if released then
    local list = handlers.release
    for i = 1, #list do list[i](rx, ry) end
  end
  local list = handlers.press
  for i = 1, n do
    local p = pressed[i]
    for k = 1, #list do
      local want = list[k][1]
      if want == p or (want == "jump" and JUMPS[p]) then list[k][2]() end
    end
    pressed[i] = nil
  end
end
