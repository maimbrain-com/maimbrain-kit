-- The Studio engine's thing layer (docs/STUDIO-API.md), run before the
-- game's main.lua in every VM. Things are plain Lua tables: a table field
-- costs ~19 ns here, a Rust-backed one ~510 ns (docs/spike/studio-lua-vm.md),
-- so things, behaviours, spawners, timers, touches and leaving are all Lua.
-- Rust does what's cheaper there: tile collision (__move, __tile), drawing
-- and sprite animation (__sprite), maps (__level), sound and effects.
--
-- Rust calls `step` (returned at the end) once a frame, in mb_update only.
-- Determinism: only arrays and numeric loops where order matters, handlers
-- run in the order they were registered, and nothing reads a clock.
local R = ...
-- Builtins as locals, so a script that redefines a global (rawget, say)
-- can't get past the guards below.
local smt, rawget, type, error, ipairs, tostring, pairs = setmetatable, rawget, type, error, ipairs, tostring, pairs
local abs, min, max, floor, sqrt, atan = math.abs, math.min, math.max, math.floor, math.sqrt, math.atan
local W, H = R.W, R.H
local draw_sprite, draw_shape, draw_box, move_box, tile_at_raw = R.sprite, R.shape, R.box, R.move, R.tile
local load_level, play_sfx, has_sound = R.level, R.sfx, R.has_sound

-- __gc and weak tables are off: when they run depends on the collector.
setmetatable = function(t, mt)
  if mt ~= nil and (rawget(mt, "__gc") ~= nil or rawget(mt, "__mode") ~= nil) then
    error("__gc and weak tables are off in Studio games", 2)
  end
  return smt(t, mt)
end

-- The canvas, in art pixels (144×256 upright, 256×144 sideways).
_G.W, _G.H = W, H
score, lives, level, speed = 0, R.lives, nil, 1

local kinds, kind_list = {}, {}
local things, spawners, timers = {}, {}, {}
local h = { tick = {}, tick_kind = {}, tap = {}, hold = {}, release = {}, swipe = {}, tilt = {},
  touch = {}, leave = {}, timer = {}, press = {}, start = {},
  flap = {}, jump = {}, land = {}, hurt = {}, pickup = {}, shoot = {} }
local nextid, now, over, heading, frozen = 1, 0, false, nil, 0
local layers = {}
local camx, camy, mapw, maph = 0, 0, W, H
local invincible, show_boxes = false, false
local dbg_items = {}
local players = {}

-- Maths -----------------------------------------------------------------------
function lerp(a, b, t) return a + (b - a) * t end
function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function dist(x1, y1, x2, y2) local dx, dy = x2 - x1, y2 - y1 return sqrt(dx * dx + dy * dy) end
function angle(x1, y1, x2, y2) return atan(y2 - y1, x2 - x1) end
function time() return now end

-- Kinds ------------------------------------------------------------------------
-- Behaviours, each with its knobs: { default, min, max, step } (the debug
-- menu shows them as live values; Edit by hand as sliders).
local B = {}

local trace = R.trace

local function fire(ev, a, b)
  local list = h[ev]
  for i = 1, #list do list[i](a, b) end
  if has_sound(ev) then play_sfx(ev) end
  -- `mb play`'s event list (a debug parameter; never on a device).
  if trace then trace(now, ev, a and a.kind or "-", b and b.kind or "-", a and a.x or 0, a and a.y or 0) end
end

local function has_anim(t, name) return t._k.anims[name] ~= nil end
local function auto_anim(t, name) if has_anim(t, name) then t.anim = name end end

local function view_clamp_x(t)
  local hw = t.w / 2
  if t.x < camx + hw then t.x = camx + hw elseif t.x > camx + W - hw then t.x = camx + W - hw end
end
local function view_clamp_y(t)
  local hh = t.h / 2
  if t.y < camy + hh then t.y = camy + hh elseif t.y > camy + H - hh then t.y = camy + H - hh end
end

-- Follow the finger (relative to where it came down), or move with the
-- arrows, WASD, the d-pad or the left stick.
local function drag(t, k, i, dt, xs, ys)
  if i.tap then t._ox, t._oy = t.x - i.tx, t.y - i.ty end
  if i.down and t._ox then
    if xs then t.x = i.x + t._ox end
    if ys then t.y = i.y + t._oy end
  else
    if xs then t.x = t.x + i.ax * k.speed * dt end
    if ys then t.y = t.y + i.ay * k.speed * dt end
  end
  if i.release then t._ox = nil end
  view_clamp_x(t)
  view_clamp_y(t)
end

B["player.drag"] = { player = true,
  knobs = { speed = { 120, 0, 400, 5 }, x = { 1, 0, 1, 1 }, y = { 1, 0, 1, 1 } },
  input = function(t, k, i, dt) drag(t, k, i, dt, k.x ~= 0, k.y ~= 0) end }

B["player.paddle"] = { player = true,
  knobs = { speed = { 160, 0, 400, 5 } },
  input = function(t, k, i, dt) drag(t, k, i, dt, true, false) end }

B["player.tilt"] = { player = true,
  knobs = { speed = { 160, 0, 400, 5 } },
  input = function(t, k, i, dt)
    if i.tilt and not i.down then t.x = t.x + i.tilt * k.speed * dt end
    drag(t, k, i, dt, true, false)
  end }

B["player.flappy"] = { player = true, collide = true,
  knobs = { gravity = { 520, 0, 2000, 10 }, flap = { 170, 0, 600, 5 }, max_fall = { 260, 0, 1000, 10 } },
  input = function(t, k, i)
    if i.tap or i.jump then
      t.vy = -k.flap
      auto_anim(t, "flap")
      t._flap = 0.25
      fire("flap", t)
    end
  end,
  update = function(t, k, dt)
    t.vy = min(t.vy + k.gravity * dt, k.max_fall)
    if t._flap then t._flap = t._flap - dt if t._flap <= 0 then t._flap = nil auto_anim(t, "idle") end end
    if t.y - t.h / 2 < camy and t.vy < 0 then t.vy = 0 end
  end }

local function jumper(t, k, i, dt, tap)
  t._coyote = (t.ground and 0.1) or max(0, (t._coyote or 0) - dt)
  local want = i.jump or tap
  if want then t._buffer = 0.12 end
  if t._buffer and t._buffer > 0 then
    t._buffer = t._buffer - dt
    local left = (t._jumps or 0)
    if t._coyote > 0 or left > 0 then
      if t._coyote <= 0 then t._jumps = left - 1 end
      t.vy = -k.jump
      t.ground, t._coyote, t._buffer = false, 0, nil
      fire("jump", t)
    end
  end
  -- Let go early for a lower jump: the finger, the key or the button.
  if (i.release or i.jumpup) and t.vy < 0 then t.vy = t.vy * (k.cut or 0.45) end
end

B["player.runner"] = { player = true, collide = true,
  knobs = { gravity = { 900, 0, 3000, 10 }, jump = { 300, 0, 800, 5 }, speed = { 0, 0, 400, 5 }, jumps = { 1, 1, 3, 1 } },
  input = function(t, k, i, dt) jumper(t, k, i, dt, i.tap) end,
  update = function(t, k, dt)
    if t.ground then t._jumps = k.jumps - 1 end
    t.vy = t.vy + k.gravity * dt
    t.vx = k.speed
    auto_anim(t, t.ground and "run" or "jump")
  end }

B["player.platformer"] = { player = true, collide = true,
  knobs = { gravity = { 900, 0, 3000, 10 }, jump = { 280, 0, 800, 5 }, speed = { 90, 0, 300, 5 }, jumps = { 1, 1, 3, 1 } },
  input = function(t, k, i, dt)
    -- Touch: hold to walk toward the finger; a tap above the player jumps.
    local dir = i.ax
    local above = i.tap and i.ty < t.y - 12
    if i.down and not above then
      local dx = i.x - t.x
      if abs(dx) > 6 then dir = dx > 0 and 1 or -1 end
    end
    t.vx = dir * k.speed
    jumper(t, k, i, dt, above)
  end,
  update = function(t, k, dt)
    if t.ground then t._jumps = k.jumps - 1 end
    t.vy = t.vy + k.gravity * dt
    if t.vx ~= 0 then t.flip = t.vx < 0 end
    auto_anim(t, (not t.ground and "jump") or (t.vx ~= 0 and "walk") or "idle")
  end }

B["enemy.patrol"] = { collide = true,
  knobs = { speed = { 40, 0, 300, 5 }, gravity = { 900, 0, 3000, 10 }, ledges = { 1, 0, 1, 1 } },
  init = function(t, k) t._dir = t.flip and 1 or -1 end,
  update = function(t, k, dt)
    if t.hitx then t._dir = -t._dir end
    -- Turn at a ledge rather than walk off it.
    if k.ledges ~= 0 and t.ground then
      local ax = t.x + t._dir * (t.w / 2 + 1)
      if tile_at_raw(ax, t.y + t.h / 2 + 2) == 0 then t._dir = -t._dir end
    end
    t.vx = t._dir * k.speed
    t.vy = k.gravity > 0 and (t.vy + k.gravity * dt) or 0
    t.flip = t._dir < 0
    auto_anim(t, "walk")
  end }

B["enemy.chase"] = { collide = true,
  knobs = { speed = { 40, 0, 300, 5 }, sight = { 200, 0, 1000, 10 }, gravity = { 0, 0, 3000, 10 } },
  update = function(t, k, dt)
    local p = players[1]
    local near = p and p.alive and dist(t.x, t.y, p.x, p.y) <= k.sight
    if near then
      local dx, dy = p.x - t.x, p.y - t.y
      local d = max(1, sqrt(dx * dx + dy * dy))
      t.vx = dx / d * k.speed
      if k.gravity == 0 then t.vy = dy / d * k.speed end
      t.flip = dx < 0
    else
      t.vx = 0
      if k.gravity == 0 then t.vy = 0 end
    end
    if k.gravity > 0 then t.vy = t.vy + k.gravity * dt end
  end }

local function shoot(t, k)
  local name = t._k.spawns
  if not name or not kinds[name] then return end
  local vx, vy = (t.flip and -1 or 1) * k.speed, 0
  local p = players[1]
  if k.aim ~= 0 and p and p.alive then
    local dx, dy = p.x - t.x, p.y - t.y
    local d = max(1, sqrt(dx * dx + dy * dy))
    vx, vy = dx / d * k.speed, dy / d * k.speed
  end
  local b = spawn(name, { x = t.x, y = t.y, vx = vx, vy = vy })
  b._from = t
  fire("shoot", t, b)
end

B["enemy.shooter"] = {
  knobs = { every = { 1.5, 0.1, 10, 0.1 }, speed = { 90, 0, 400, 5 }, aim = { 1, 0, 1, 1 } },
  init = function(t, k) t._next = k.every end,
  update = function(t, k, dt)
    t._next = t._next - dt * speed
    if t._next <= 0 then t._next = t._next + k.every shoot(t, k) end
  end }

B["bullet"] = { collide = true,
  knobs = { speed = { 120, 0, 600, 5 } },
  init = function(t, k)
    if t.vx == 0 and t.vy == 0 then t.vx = (t.flip and -1 or 1) * k.speed end
  end,
  update = function(t) if t.hitx or t.hity then t:destroy() end end }

B["pickup"] = { knobs = { value = { 1, -100, 100, 1 } } }
B["hazard"] = { knobs = {} }
B["solid"] = { knobs = {} }

B["spawner"] = {
  knobs = { every = { 1.5, 0.1, 10, 0.1 }, spread = { 0, 0, 512, 4 }, vx = { 0, -600, 600, 5 }, vy = { 0, -600, 600, 5 } },
  init = function(t, k) t._next = k.every end,
  update = function(t, k, dt)
    t._next = t._next - dt * speed
    local name = t._k.spawns
    if t._next <= 0 and name and kinds[name] then
      t._next = t._next + k.every
      spawn(name, { x = t.x + (rnd(1) - 0.5) * k.spread, y = t.y, vx = k.vx, vy = k.vy })
    end
  end }

local function behaviour(name, level)
  local b = B[name]
  if not b then
    local known = {}
    for n in pairs(B) do known[#known + 1] = n end
    table.sort(known)
    error("no behaviour called '" .. tostring(name) .. "' (" .. table.concat(known, ", ") .. ")", level + 1)
  end
  return b
end

-- kind("fish", { w = 8, h = 8, color = 11, shape = "circle", behaviour = "pickup", knobs = { value = 1 } })
-- Sprites (sprites/<name>.toml) are kinds already; kind() adds to or changes one.
function kind(name, p)
  if type(name) ~= "string" then error("kind needs a name", 2) end
  p = p or {}
  local k = kinds[name]
  if not k then
    k = { name = name, w = 8, h = 8, ox = 0, oy = 0, spr = -1, anims = { idle = 0 }, beh = {}, knobs = {},
      layer = 0, color = 7, circle = false }
    kinds[name] = k
    kind_list[#kind_list + 1] = k
  end
  if p.w then k.w = p.w end
  if p.h then k.h = p.h end
  if p.color then k.color = p.color end
  if p.shape then k.circle = p.shape == "circle" end
  if p.layer then k.layer = p.layer end
  local found = false
  for _, l in ipairs(layers) do if l == k.layer then found = true end end
  if not found then layers[#layers + 1] = k.layer table.sort(layers) end
  if p.spawns then k.spawns = p.spawns end
  local list = p.behaviours or (p.behaviour and { p.behaviour })
  if list then
    k.beh = {}
    for _, bn in ipairs(list) do k.beh[#k.beh + 1] = behaviour(bn, 2) end
    k.beh_names = list
  end
  for _, b in ipairs(k.beh) do
    for kn, d in pairs(b.knobs) do if k.knobs[kn] == nil then k.knobs[kn] = d[1] end end
    if b.player then k.player = true end
    if b.collide then k.collide = true end
  end
  if p.knobs then for kn, v in pairs(p.knobs) do k.knobs[kn] = v end end
  k.pickup, k.hazard, k.solid = false, false, false
  for _, bn in ipairs(k.beh_names or {}) do
    if bn == "pickup" then k.pickup = true elseif bn == "hazard" then k.hazard = true elseif bn == "solid" then k.solid = true end
  end
  if p.player ~= nil then k.player = p.player end
  return k
end

for _, d in ipairs(R.kinds) do
  local k = kind(d.name, { w = d.w, h = d.h, color = d.color, shape = d.circle and "circle" or nil, layer = d.layer,
    behaviours = d.behaviours, knobs = d.knobs, spawns = d.spawns })
  k.ox, k.oy, k.spr, k.anims = d.ox, d.oy, d.spr, d.anims
end

local function K(name, level)
  local k = kinds[name]
  if not k then
    error("no kind named '" .. tostring(name) .. "' (add sprites/" .. tostring(name) .. ".toml, or kind(\"" .. tostring(name) .. "\", {...}))", level + 1)
  end
  return k
end

-- Things -----------------------------------------------------------------------
local Thing = {}
Thing.__index = Thing
Thing.__tostring = function(t) return t.kind .. " " .. t.id end
function Thing:destroy() self.alive = false end
function Thing:flip() self.flip = not self.flip end
function Thing:is(name) return self.kind == name end
function Thing:hurt() hurt(self) end

local function make(k, p)
  local t = smt({ id = nextid, kind = k.name, _k = k, x = W / 2, y = H / 2, vx = 0, vy = 0,
    w = k.w, h = k.h, anim = "idle", flip = false, alive = true, ground = false, _at = 0, _anim = "idle" }, Thing)
  nextid = nextid + 1
  if p then for key, v in pairs(p) do t[key] = v end end
  for _, b in ipairs(k.beh) do if b.init then b.init(t, k.knobs) end end
  things[#things + 1] = t
  if k.player then players[#players + 1] = t end
  return t
end

function spawn(name, p) return make(K(name, 2), p) end

-- spawn_every("fish", 1.2, { y = -8 }), or a function that returns the props
function spawn_every(name, secs, p)
  local k = K(name, 2)
  if type(secs) ~= "number" or secs <= 0 then error("spawn_every needs a positive number of seconds", 2) end
  local s = { k = k, every = secs, next = now + secs, p = p, on = true }
  spawners[#spawners + 1] = s
  return { stop = function() s.on = false end }
end

function all(name)
  local out = {}
  for i = 1, #things do
    local t = things[i]
    if t.alive and (name == nil or t.kind == name) then out[#out + 1] = t end
  end
  return out
end

function count(name)
  local n = 0
  for i = 1, #things do local t = things[i] if t.alive and t.kind == name then n = n + 1 end end
  return n
end

function nearest(name, x, y)
  local best, bd = nil, math.huge
  for i = 1, #things do
    local t = things[i]
    if t.alive and t.kind == name then
      local dx, dy = t.x - x, t.y - y
      local d = dx * dx + dy * dy
      if d < bd then best, bd = t, d end
    end
  end
  return best
end

-- Events -----------------------------------------------------------------------
local function fn(f, what) if type(f) ~= "function" then error(what .. " needs a function", 3) end return f end

-- on("tick", fn) / on("tick", kind, fn) / on("tap" | "hold" | "release" | "swipe" | "tilt", fn)
-- on("touch", a, b, fn) / on("leave", kind, fn) / on("timer", name, fn) / on("press", button, fn)
-- on("flap" | "jump" | "land" | "hurt" | "pickup" | "shoot", fn) / on("start", fn)
function on(ev, a, b, c)
  if ev == "tick" and type(a) ~= "function" then
    h.tick_kind[#h.tick_kind + 1] = { K(a, 2).name, fn(b, "on(\"tick\", kind, ...)") }
  elseif ev == "touch" then
    h.touch[#h.touch + 1] = { K(a, 2).name, K(b, 2).name, fn(c, "on(\"touch\", a, b, ...)") }
  elseif ev == "leave" then
    h.leave[#h.leave + 1] = { K(a, 2).name, fn(b, "on(\"leave\", kind, ...)") }
  elseif ev == "timer" or ev == "press" then
    if type(a) ~= "string" then error("on(\"" .. ev .. "\", name, fn) needs a name", 2) end
    h[ev][#h[ev] + 1] = { a, fn(b, "on(\"" .. ev .. "\", name, ...)") }
  elseif h[ev] and ev ~= "tick_kind" then
    h[ev][#h[ev] + 1] = fn(a, "on(\"" .. ev .. "\", ...)")
  else
    error("unknown event '" .. tostring(ev) .. "'", 2)
  end
end

-- timer("wave", 5) fires on("timer", "wave") in 5 s; timer("wave", 5, true) every 5 s.
function timer(name, secs, repeats)
  if type(secs) ~= "number" or secs <= 0 then error("timer needs a positive number of seconds", 2) end
  timers[#timers + 1] = { name = name, every = repeats and secs or nil, at = now + secs }
end
function after(secs, f)
  if type(secs) ~= "number" or secs < 0 then error("after needs a number of seconds", 2) end
  timers[#timers + 1] = { f = fn(f, "after(secs, ...)"), at = now + secs }
end

-- Rounds -----------------------------------------------------------------------
function game_over(text)
  if invincible then return end
  if not over then over, heading = true, text end
end

function hurt(t)
  if invincible or not t or not t.alive or (t._safe and t._safe > 0) then return end
  lives = lives - 1
  fire("hurt", t)
  if lives <= 0 then game_over() else t._safe = 1.2 end
end

function speed_up(f)
  if type(f) ~= "number" or f <= 0 then error("speed_up needs a positive number", 2) end
  speed = speed * f
end

local function clear_world()
  for i = 1, #things do things[i].alive = false end
  things, spawners, players = {}, {}, {}
end

local function enter(name)
  local m, err = load_level(name)
  if not m then return nil, err end
  clear_world()
  level = m.name
  mapw, maph = max(W, m.w), max(H, m.h)
  if m.music then music(m.music) end
  for _, s in ipairs(m.spawns) do
    local k = kinds[s.kind]
    if not k then error("maps/" .. m.name .. ".map places '" .. s.kind .. "', but there's no such kind", 0) end
    if s.every then
      local sp = s.spread
      local props = s.props
      spawners[#spawners + 1] = { k = k, every = s.every, next = now + s.every, on = true, p = function()
        local q = { x = s.x + (rnd(1) - 0.5) * sp, y = s.y }
        for key, v in pairs(props) do q[key] = v end
        return q
      end }
    else
      local q = { x = s.x, y = s.y }
      for key, v in pairs(s.props) do q[key] = v end
      make(k, q)
    end
  end
  return true
end

-- next_level() goes to the next map in order, next_level("boss") to that one.
-- Past the last one the round ends, won.
function next_level(name)
  if name == nil then
    local found = false
    for _, n in ipairs(R.levels) do
      if found then name = n break end
      if n == level then found = true end
    end
    if not name then
      over, heading = true, R.win
      return
    end
  end
  local ok, err = enter(name)
  if not ok then error(err, 2) end
end

-- Feel and the rest are Rust (sfx, music, shake, flash, hitstop, particles,
-- popup, text, save, load, rnd, chance).
-- hitstop(frames) freezes the world for a few frames (a hit lands).
function hitstop(frames) frozen = max(frozen, floor(frames or 3)) end
function solid_at(x, y) return tile_at_raw(x, y) & 1 ~= 0 end
tile_at = R.tile_name

-- The debug menu's game tab (declare items at the top of main.lua).
debug = {}
local function item(kindname, label, f, extra)
  if type(label) ~= "string" then error("debug." .. kindname .. " needs a label", 3) end
  local it = extra or {}
  it.kind, it.label, it.f = kindname, label, fn(f, "debug." .. kindname)
  dbg_items[#dbg_items + 1] = it
end
function debug.action(label, f) item("action", label, f) end
function debug.toggle(label, f, on) item("toggle", label, f, { value = on and 1 or 0 }) end
function debug.number(label, value, f, lo, hi, st)
  item("number", label, f, { value = value or 0, min = lo or 0, max = hi or 100, step = st or 1 })
end
function debug.choice(label, options, f, index)
  if type(options) ~= "table" or #options == 0 then error("debug.choice needs a list of options", 2) end
  item("choice", label, f, { value = (index or 1) - 1, options = options })
end

-- What the engine reads for its own menu (not for games).
-- Every live thing as { kind, x, y }, for `mb play`'s summary (not for games).
function __things()
  local out = {}
  for i = 1, #things do
    local t = things[i]
    if t.alive then out[#out + 1] = { t.kind, t.x, t.y } end
  end
  return out
end

function __menu()
  local ks, items, names = {}, {}, {}
  for _, k in ipairs(kind_list) do
    names[#names + 1] = k.name
    for _, b in ipairs(k.beh) do
      local keys = {}
      for kn in pairs(b.knobs) do keys[#keys + 1] = kn end
      table.sort(keys)
      for _, kn in ipairs(keys) do
        local d = b.knobs[kn]
        ks[#ks + 1] = { k.name, kn, k.knobs[kn], d[2], d[3], d[4] }
      end
    end
  end
  for _, it in ipairs(dbg_items) do
    items[#items + 1] = { it.kind, it.label, it.value or 0, it.min or 0, it.max or 0, it.step or 0, it.options }
  end
  return names, ks, items
end

-- One frame -----------------------------------------------------------------------
local function overlaps(a, b)
  return abs(a.x - b.x) * 2 < a.w + b.w and abs(a.y - b.y) * 2 < a.h + b.h
end

local function debug_input(d)
  for _, c in ipairs(d) do
    local op = c[1]
    if op == "score" then score = c[2]
    elseif op == "lives" then lives = c[2]
    elseif op == "inv" then invincible = c[2] ~= 0
    elseif op == "boxes" then show_boxes = c[2] ~= 0
    elseif op == "level" then local ok, err = enter(c[2]) if not ok then error(err, 0) end
    elseif op == "spawn" then
      local k = kinds[c[2]]
      if k then
        local p = players[1]
        make(k, { x = camx + W / 2, y = p and p.alive and (p.y - 48) or camy + H / 3 })
      end
    elseif op == "knob" then
      local k = kinds[c[2]]
      if k then k.knobs[c[3]] = c[4] end
    elseif op == "item" then
      local it = dbg_items[c[2]]
      if it then
        if it.kind ~= "action" then it.value = c[3] end
        if it.kind == "toggle" then it.f(c[3] ~= 0)
        elseif it.kind == "choice" then it.f(it.options[floor(c[3]) + 1], floor(c[3]) + 1)
        elseif it.kind == "number" then it.f(c[3])
        else it.f() end
      end
    elseif op == "set" then _G[c[2]] = c[3]
    end
  end
end

local function move(t, dt)
  local dx, dy = t.vx * dt, t.vy * dt
  if not t._k.collide then
    t.x, t.y = t.x + dx, t.y + dy
    return
  end
  local was = t.ground
  local nx, ny, flags = move_box(t.x, t.y, t.w, t.h, dx, dy, t.drop and 1 or 0)
  t.hitx, t.hity = flags & 1 ~= 0, flags & 6 ~= 0
  t.ground = flags & 2 ~= 0
  if flags & 2 ~= 0 and t.vy > 0 then t.vy = 0 end
  if flags & 4 ~= 0 and t.vy < 0 then t.vy = 0 end
  t.x, t.y = nx, ny
  if flags & 8 ~= 0 and t._k.player then hurt(t) end
  if t.ground and not was then fire("land", t) end
end

-- Things with `solid` block the ones that collide, as solid tiles do.
local function push_out(t, s)
  if not overlaps(t, s) then return end
  local ox = (t.w + s.w) / 2 - abs(t.x - s.x)
  local oy = (t.h + s.h) / 2 - abs(t.y - s.y)
  if ox < oy then
    t.x = t.x + (t.x < s.x and -ox or ox)
    t.hitx = true
  else
    if t.y < s.y then
      t.y = t.y - oy
      if t.vy > 0 then t.vy = 0 end
      if not t.ground then t.ground = true fire("land", t) end
    else
      t.y = t.y + oy
      if t.vy < 0 then t.vy = 0 end
    end
    t.hity = true
  end
end

local function step(dt, i)
  if i.dbg then debug_input(i.dbg) end
  i.x, i.y, i.tx, i.ty, i.rx, i.ry = i.x + camx, i.y + camy, i.tx + camx, i.ty + camy, i.rx + camx, i.ry + camy

  if frozen > 0 then
    frozen = frozen - 1
  else
    now = now + dt
    local wdt = dt * speed

    -- Input: the game's handlers, then the players' behaviours.
    if i.tap then for _, f in ipairs(h.tap) do f(i.tx, i.ty) end end
    if i.down then for _, f in ipairs(h.hold) do f(i.x, i.y) end end
    if i.release then for _, f in ipairs(h.release) do f(i.rx, i.ry) end end
    if i.swipe then for _, f in ipairs(h.swipe) do f(i.sdx, i.sdy) end end
    if i.tilt then for _, f in ipairs(h.tilt) do f(i.tilt) end end
    if i.press then
      for _, name in ipairs(i.press) do
        for _, ph in ipairs(h.press) do if ph[1] == name then ph[2]() end end
      end
    end
    for n = 1, #players do
      local p = players[n]
      if p.alive then
        for _, b in ipairs(p._k.beh) do if b.input then b.input(p, p._k.knobs, i, dt) end end
      end
    end

    -- Spawners and timers.
    for _, s in ipairs(spawners) do
      if s.on then
        s.next = s.next - (wdt - dt)
        while s.next <= now do
          local p = s.p
          if type(p) == "function" then p = p() end
          make(s.k, p)
          s.next = s.next + s.every
        end
      end
    end
    -- Timers set by a handler here wait for the next frame, after the rest.
    local list, keep = timers, {}
    timers = {}
    for k = 1, #list do
      local tm = list[k]
      if tm.at <= now then
        if tm.f then tm.f() else
          for _, th in ipairs(h.timer) do if th[1] == tm.name then th[2]() end end
        end
        if tm.every then tm.at = tm.at + tm.every keep[#keep + 1] = tm end
      else
        keep[#keep + 1] = tm
      end
    end
    for k = 1, #timers do keep[#keep + 1] = timers[k] end
    timers = keep
    local n

    for _, f in ipairs(h.tick) do f(dt) end
    for _, kh in ipairs(h.tick_kind) do
      local name, f = kh[1], kh[2]
      for k = 1, #things do
        local t = things[k]
        if t.alive and t.kind == name then f(t, dt) end
      end
    end

    -- Behaviours, then motion (players at the round's own pace, the rest at `speed`).
    local solids = {}
    n = #things
    for k = 1, n do
      local t = things[k]
      if t.alive then
        local kd = t._k
        local tdt = kd.player and dt or wdt
        for _, b in ipairs(kd.beh) do if b.update then b.update(t, kd.knobs, tdt) end end
        if kd.solid then solids[#solids + 1] = t end
      end
    end
    for k = 1, n do
      local t = things[k]
      if t.alive then
        move(t, t._k.player and dt or wdt)
        if t._k.collide then
          for s = 1, #solids do if solids[s] ~= t then push_out(t, solids[s]) end end
        end
        if t._safe then t._safe = t._safe - dt if t._safe <= 0 then t._safe = nil end end
      end
    end

    -- Touches: pickups and hazards on players, then the game's handlers.
    for pn = 1, #players do
      local p = players[pn]
      for k = 1, #things do
        local t = things[k]
        if p.alive and t.alive and t ~= p and (t._k.pickup or t._k.hazard) and overlaps(p, t) then
          if t._k.pickup then
            score = score + (t._k.knobs.value or 1)
            t.alive = false
            fire("pickup", p, t)
          else
            hurt(p)
          end
        end
      end
    end
    for _, th in ipairs(h.touch) do
      local a, b, f = th[1], th[2], th[3]
      local n2 = #things
      for x = 1, n2 do
        local ta = things[x]
        if ta.alive and ta.kind == a then
          for y = 1, n2 do
            local tb = things[y]
            if tb.alive and tb.kind == b and ta ~= tb and overlaps(ta, tb) then
              f(ta, tb)
              if not ta.alive then break end
            end
          end
        end
      end
    end

    -- Leaving: out of the level (the screen, for a one-screen map), having
    -- been in it (or far away). A thing the camera scrolls past stays.
    n = #things
    for k = 1, n do
      local t = things[k]
      if t.alive then
        local l, r, tp, bt = t.x - t.w / 2, t.x + t.w / 2, t.y - t.h / 2, t.y + t.h / 2
        local out = r < 0 or l > mapw or bt < 0 or tp > maph
        if not out then
          t.seen = true
        elseif t.seen or r < -W or l > mapw + W or bt < -H or tp > maph + H then
          local handled = false
          for _, lh in ipairs(h.leave) do
            if lh[1] == t.kind then lh[2](t) handled = true end
          end
          t.alive = false
          if t._k.player and not handled then game_over() end
        end
      end
    end
  end

  -- The camera follows the first player across a map bigger than the screen.
  local p = players[1]
  if p and p.alive then
    if mapw > W then camx = floor(clamp(p.x - W / 2, 0, mapw - W) + 0.5) end
    if maph > H then camy = floor(clamp(p.y - H / 2, 0, maph - H) + 0.5) end
  end

  -- The dead go; the living are drawn, layer by layer.
  local j = 0
  local n = #things
  for k = 1, n do
    local t = things[k]
    things[k] = nil
    if t.alive then j = j + 1 things[j] = t end
  end
  j = 0
  n = #players
  for k = 1, n do
    local t = players[k]
    players[k] = nil
    if t.alive then j = j + 1 players[j] = t end
  end
  for _, layer in ipairs(layers) do
    for k = 1, #things do
      local t = things[k]
      local kd = t._k
      if kd.layer == layer then
        if t.anim ~= t._anim then t._anim, t._at = t.anim, 0 else t._at = t._at + dt end
        local blink = t._safe and floor(t._safe * 10) % 2 == 0
        if not blink then
          if kd.spr >= 0 then
            draw_sprite(kd.spr, kd.anims[t.anim] or 0, t._at, t.x - kd.ox, t.y - kd.oy, t.flip and 1 or 0)
          else
            draw_shape(t.x, t.y, t.w, t.h, kd.color, kd.circle and 1 or 0)
          end
        end
        if show_boxes then draw_box(t.x, t.y, t.w, t.h) end
      end
    end
  end
  return over, heading, camx, camy
end

-- The first level, then the game's start().
local function begin()
  if R.levels[1] then local ok, err = enter(R.start_level or R.levels[1]) if not ok then error(err, 0) end end
  if type(start) == "function" then start() end
  for _, f in ipairs(h.start) do f() end
end

return step, begin
