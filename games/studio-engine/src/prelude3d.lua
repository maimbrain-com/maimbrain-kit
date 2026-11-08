-- 3D games (docs/STUDIO-3D-API.md), run before the game's
-- main.lua in every VM of the engine's 3D build. Handles are small tables
-- { id = n } with methods in a metatable, so a field read stays a Lua table
-- read (~20 ns), and each method is one Lua call into one Rust call with
-- plain numbers (scene.rs; docs/spike/studio-lua-3d.md "Bindings").
--
-- It keeps the Arcade prelude's contract with the engine: Rust calls `step`
-- once a frame (in mb_update only) with the same input table, and `begin`
-- runs `start()`; `debug.*`, `__menu` and the debug picks work the same, so
-- the debug menu, scenarios and `mb play` need nothing new.
local api = ...
local smt, rawget, type, error, ipairs, tostring = setmetatable, rawget, type, error, ipairs, tostring
local floor, sqrt, atan = math.floor, math.sqrt, math.atan

-- __gc and weak tables are off: when they run depends on the collector.
setmetatable = function(t, mt)
  if mt ~= nil and (rawget(mt, "__gc") ~= nil or rawget(mt, "__mode") ~= nil) then
    error("__gc and weak tables are off in Studio games", 2)
  end
  return smt(t, mt)
end
local setmetatable = smt

-- The round -------------------------------------------------------------------
score, lives = 0, api.lives
-- True on the title card, where the game plays itself behind the title.
title = api.title
-- True once the round is lost; ticks keep coming (with no input) so the world can settle under the card.
over = false
local heading, now = nil, 0
local invincible = false

function game_over(text)
  if invincible or title then return end
  if not over then over, heading = true, text end
end

-- Maths ---------------------------------------------------------------------------
function lerp(a, b, t) return a + (b - a) * t end
function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function dist(x1, y1, x2, y2) local dx, dy = x2 - x1, y2 - y1 return sqrt(dx * dx + dy * dy) end
function angle(x1, y1, x2, y2) return atan(y2 - y1, x2 - x1) end
function time() return now end

-- Meshes -------------------------------------------------------------------------
local Mesh = {}
Mesh.__index = Mesh
local function mesh_of(id) return setmetatable({ id = id }, Mesh) end

mesh = {}
for _, k in ipairs({ "box", "sphere", "ico", "cylinder", "cone", "plane", "torus", "empty" }) do
  local make = api[k]
  mesh[k] = function(...) return mesh_of(make(...)) end
end
function mesh.lathe(profile, segments) return mesh_of(api.lathe(profile, segments)) end
function mesh.extrude(poly, depth) return mesh_of(api.extrude(poly, depth)) end
-- Triangles from a flat list, 9 numbers (three corners) each.
function mesh.tris(list) return mesh_of(api.tris(list)) end

local mesh_transform = api.mesh_transform
function Mesh:color(c) api.mesh_color(self.id, c) return self end
-- Vertex colours by height: `below` up to y, `above` over it.
function Mesh:split(y, below, above) api.mesh_split(self.id, y, below, above) return self end
function Mesh:move(x, y, z) mesh_transform(self.id, x, y, z) return self end
function Mesh:turn(yaw, pitch, roll) mesh_transform(self.id, 0, 0, 0, yaw or 0, pitch or 0, roll or 0) return self end
function Mesh:scale(x, y, z) mesh_transform(self.id, 0, 0, 0, 0, 0, 0, x, y or x, z or x) return self end
function Mesh:clamp_y(lo) api.mesh_clamp_y(self.id, lo) return self end
function Mesh:add(other) api.mesh_add(self.id, other.id) return self end
function Mesh:flat() api.mesh_flat(self.id) return self end

-- Materials ------------------------------------------------------------------------
local Material = {}
Material.__index = Material
local function material_of(id) return id and setmetatable({ id = id }, Material) end

-- material { color, emissive, glow, metal, rough, unlit, double, alpha,
--            additive, style = { toon, rim, outline, ink, nofog } }
function material(t) return material_of(api.material(t or {})) end
function Material:style(t) api.material_style(self.id, t) return self end

-- Nodes ----------------------------------------------------------------------------
local Node = {}
Node.__index = Node
local function node_of(id) return id and setmetatable({ id = id }, Node) end

function node(m, mat) return node_of(api.node(m and m.id, mat and mat.id)) end

local place, pos, turn, size, get_pos = api.place, api.pos, api.turn, api.size, api.get_pos
local show, instances, text3d = api.show, api.instances, api.text
-- The common case in one call: where, which way it faces, how big.
function Node:place(x, y, z, yaw, s) place(self.id, x, y, z, yaw, s) end
function Node:pos(x, y, z)
  if type(x) == "table" then x, y, z = x[1], x[2], x[3] end
  if x == nil then return get_pos(self.id) end
  pos(self.id, x, y, z)
end
function Node:turn(yaw, pitch, roll) turn(self.id, yaw, pitch, roll) end
function Node:size(x, y, z) size(self.id, x, y, z) end
function Node:parent(p) api.parent(self.id, p and p.id) end
function Node:show(on) show(self.id, on ~= false) end
function Node:destroy() api.destroy(self.id) end
function Node:world() return api.world(self.id) end
function Node:fade(from, to) api.fade(self.id, from, to) end
function Node:light(color, intensity, range) return api.light(self.id, color, intensity, range) end
-- text, or "" to remove it; t = { size, color, outline, width, font,
-- billboard, screen, ontop, nofog }
function Node:text(s, t) text3d(self.id, s, t) end
-- A flat list: x, y, z, then (stride > 3) yaw, scale, colour, pitch, roll, tall.
function Node:instances(list, stride) return instances(self.id, list, stride) end
function Node:find(name) return node_of(api.find(self.id, name)) end
function Node:material() return material_of(api.node_material(self.id)) end
-- Animation (models only): play("Run"), play("Jump", { once = true, fade = 0.06, restart = true })
function Node:play(clip, o)
  o = o or {}
  api.play(self.id, clip, o.once, o.fade, o.restart)
end
function Node:speed(k) api.anim_speed(self.id, k) end
function Node:clip() return api.clip(self.id) end

-- Models and particles ---------------------------------------------------------------
local Model = {}
Model.__index = Model
-- model("fox") is models/fox.glb.
function model(name) return setmetatable({ id = api.model(name) }, Model) end
function spawn(m, parent) return node_of(api.spawn(m.id, parent and parent.id)) end

local Emitter = {}
Emitter.__index = Emitter
local emit = api.emit
-- emitter { colors = { 4 × rgba }, sizes = { 4 }, life = { a, b }, speed = { a, b },
--           spread, drag, gravity = { x, y, z }, stretch, alpha }
function emitter(t) return setmetatable({ id = api.emitter(t) }, Emitter) end
-- A burst at x, y, z along dx, dy, dz; with vx, vy, vz, in a frame moving that fast.
function Emitter:emit(x, y, z, dx, dy, dz, n, vx, vy, vz) emit(self.id, x, y, z, dx, dy, dz, n, vx, vy, vz) end

-- The camera, the light, the look, the 2D on top, sound ----------------------------
camera = api.camera -- camera(x, y, z, look_x, look_y, look_z, fov, near, far)
project = api.project -- project(x, y, z) → sx, sy, on_screen
sky, sun, post, fog, dof = api.sky, api.sun, api.post, api.fog, api.dof
grade, shafts, halos, film = api.grade, api.shafts, api.halos, api.film
text = api.hud_text -- text(s, x, y, { size, color, align, shadow })
rect = api.hud_rect -- rect(x, y, w, h, color)

-- Sound and haptics are output only: replays never hear them.
sfx = api.sfx -- sfx(name, vol, pan, pitch): once
music = api.music -- music(name, vol, fade): loops, and carries on across rounds; music(nil) fades it out
lowpass = api.lowpass -- lowpass("music" | "sfx" | "master", cutoff_hz, seconds)
haptic = api.haptic -- haptic("tap" | "success" | "fail" | "heavy")
local Voice = {}
Voice.__index = Voice
-- loop(name, vol, pan, pitch): a sound that repeats until :stop(), for this round.
function loop(name, vol, pan, pitch) return setmetatable({ id = api.loop(name, vol, pan, pitch) }, Voice) end
function Voice:set(vol, pan, pitch) api.voice_set(self.id, vol, pan, pitch) end
function Voice:stop(fade) api.voice_stop(self.id, fade) end

-- Events and timers ------------------------------------------------------------------
-- What the pointer, keys and pad are doing now (logical units, 360×640 upright):
-- x, y, down; ax, ay (−1…1 from the arrows, WASD, the d-pad or the stick); jump.
input = { x = 0, y = 0, down = false, ax = 0, ay = 0, jump = false }

local h = { tick = {}, tap = {}, hold = {}, release = {}, swipe = {}, press = {}, timer = {}, start = {} }
-- on("tick" | "tap" | "hold" | "release" | "swipe" | "start", fn), on("press", button, fn), on("timer", name, fn)
function on(ev, a, b)
  if ev == "press" or ev == "timer" then
    if type(a) ~= "string" then error("on(\"" .. ev .. "\", name, fn) needs a name", 2) end
    if type(b) ~= "function" then error("on(\"" .. ev .. "\", name, fn) needs a function", 2) end
    h[ev][#h[ev] + 1] = { a, b }
  elseif h[ev] then
    if type(a) ~= "function" then error("on(\"" .. tostring(ev) .. "\", fn) needs a function", 2) end
    h[ev][#h[ev] + 1] = a
  else
    error("unknown event '" .. tostring(ev) .. "' (tick, tap, hold, release, swipe, press, timer, start)", 2)
  end
end

local timers = {}
-- timer("wave", 5) fires on("timer", "wave") in 5 s; timer("wave", 5, true) every 5 s.
function timer(name, secs, repeats)
  if type(secs) ~= "number" or secs <= 0 then error("timer needs a positive number of seconds", 2) end
  timers[#timers + 1] = { name = name, every = repeats and secs or nil, at = now + secs }
end
-- after(secs, fn) once; every(secs, fn) over and over.
function after(secs, f)
  if type(secs) ~= "number" or secs < 0 then error("after needs a number of seconds", 2) end
  timers[#timers + 1] = { f = f, at = now + secs }
end
function every(secs, f)
  if type(secs) ~= "number" or secs <= 0 then error("every needs a positive number of seconds", 2) end
  timers[#timers + 1] = { f = f, every = secs, at = now + secs }
end

-- The debug menu's game tab (declare items at the top of main.lua) -------------------
debug = {}
local dbg_items = {}
local function item(kindname, label, f, extra)
  if type(label) ~= "string" then error("debug." .. kindname .. " needs a label", 3) end
  if type(f) ~= "function" then error("debug." .. kindname .. " needs a function", 3) end
  local it = extra or {}
  it.kind, it.label, it.f = kindname, label, f
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
-- The round's Invincible is on: a game that takes lives itself skips it.
function debug.invincible() return invincible end

-- What the engine reads for its menu and `mb play` (not for games). The 3D
-- view has no kinds or knobs: its menu is the game's items and the round's.
function __things() return {} end
function __menu()
  local items = {}
  for _, it in ipairs(dbg_items) do
    items[#items + 1] = { it.kind, it.label, it.value or 0, it.min or 0, it.max or 0, it.step or 0, it.options }
  end
  return {}, {}, items
end

local function debug_input(d)
  for _, c in ipairs(d) do
    local op = c[1]
    if op == "score" then score = c[2]
    elseif op == "lives" then lives = c[2]
    elseif op == "inv" then invincible = c[2] ~= 0
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

-- One frame ---------------------------------------------------------------------------
local function step(dt, i)
  if i.dbg then debug_input(i.dbg) end
  now = now + dt
  input.x, input.y, input.down, input.ax, input.ay, input.jump = i.x, i.y, i.down, i.ax, i.ay, i.jump
  if i.tap then for _, f in ipairs(h.tap) do f(i.tx, i.ty) end end
  if i.down then for _, f in ipairs(h.hold) do f(i.x, i.y) end end
  if i.release then for _, f in ipairs(h.release) do f(i.rx, i.ry) end end
  if i.swipe then for _, f in ipairs(h.swipe) do f(i.sdx, i.sdy) end end
  if i.press then
    for _, name in ipairs(i.press) do
      for _, ph in ipairs(h.press) do if ph[1] == name then ph[2]() end end
    end
  end
  local k = 1
  while k <= #timers do
    local t = timers[k]
    if now >= t.at then
      if t.f then t.f() else
        for _, th in ipairs(h.timer) do if th[1] == t.name then th[2]() end end
      end
      if t.every then
        t.at = t.at + t.every
      else
        table.remove(timers, k)
        k = k - 1
      end
    end
    k = k + 1
  end
  local ticks = h.tick
  for n = 1, #ticks do ticks[n](dt) end
  return over, heading, 0, 0
end

-- The game's start(), then on("start") handlers.
local function begin()
  if type(start) == "function" then start() end
  for _, f in ipairs(h.start) do f() end
end

return step, begin
