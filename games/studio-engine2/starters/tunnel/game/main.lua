-- The tunnel starter: the player hugs the wall of a tunnel of rings. Each
-- ring is a circle of rocks with one gap; drag left and right (or use the
-- arrows, or a controller) to slide around the tunnel and thread the gaps.
--
-- Everything it looks like is in game/look.lua: the starter's lava tube,
-- a stand-in. Make that the game's own, in DESIGN.md's look; this file
-- holds the rules.
--
-- The tunnel frame: an angle a around the tunnel's axis, 0 at the bottom and
-- growing to the right. The player sits at angle `steer`; a ring's gap is at
-- angle g. On screen the player is always at the bottom: the tunnel turns
-- around it (each ring is rolled by g - steer).
--
-- The player stays at z = 0 and the rings fly toward it: a ring d meters
-- along the tunnel is at z = -(d - dist).

local look = require("look")

local sin, cos, floor, max, min, abs, pi = math.sin, math.cos, math.floor, math.max, math.min, math.abs, math.pi
local TAU = 2 * pi

local R = 3.0 -- the tunnel's radius: rocks and player both ride it
local SPACING = 22.0 -- meters between rings
local RINGS = 7 -- the ring pool (fog hides the far ones)
local FIRST = 25.0 -- the first ring: under 2 s at the start speed
local START_SPEED, MAX_SPEED, ACCEL = 13.5, 34.0, 0.55
local ROCK = 0.42 -- about a rock's radius, for spacing them
local MARGIN = 0.1 -- radians of the player's body inside a gap's edge
local SCRAPE = 0.09 -- a pass with less slack than this (radians) is a scrape
local STEER = pi / 260 -- radians per unit dragged

local function wrap(a)
  a = (a + pi) % TAU
  return a - pi
end

-- The run ---------------------------------------------------------------------

local run -- t, dist, speed, steer, passed, over, next_d, count, rings (data), last_gap

-- A ring's plan: where it is, how wide its gap, where the gap starts and how
-- fast the ring turns. n counts from 1.
local function plan(n, d, last_gap)
  local half, gap, spin
  if n == 1 then
    half, gap, spin = 0.95, 0, 0
  else
    half = max(0.3, 0.9 - (n - 2) * 0.028)
    -- The next gap is always somewhere else: a little way round early, up to
    -- most of the way round later.
    local reach = min(2.5, 0.95 + n * 0.07)
    local off = 0.55 + love.math.random() * (reach - 0.55)
    if chance(0.5) then off = -off end
    gap = wrap(last_gap + off)
    spin = 0
    if n >= 10 and chance(min(0.65, 0.25 + n * 0.012)) then
      spin = min(0.85, 0.25 + n * 0.012) * (chance(0.5) and 1 or -1)
    end
  end
  return { n = n, d = d, half = half, gap0 = gap, spin = spin, born = run.t, passed = false, owner = run }
end

-- Where a ring's gap is now, in the tunnel frame.
local function gap_of(r) return wrap(r.gap0 + r.spin * (run.t - r.born)) end

local function lay_out()
  while run.next_d < run.dist + SPACING * RINGS - 4 do
    run.count = run.count + 1
    local r = plan(run.count, run.next_d, run.last_gap)
    run.last_gap = r.gap0
    run.rings[#run.rings + 1] = r
    run.next_d = run.next_d + SPACING
  end
end

local function new_run()
  run = { t = 0, dist = 0, speed = START_SPEED, steer = 0, passed = 0, over = false, next_d = FIRST, count = 0, rings = {}, last_gap = 0, events = {} }
  lay_out()
end

local function run_step(dt)
  local ev = run.events
  for i = #ev, 1, -1 do ev[i] = nil end
  if run.over or not (dt > 0) then return end
  run.t = run.t + dt
  run.speed = min(run.speed + ACCEL * dt, MAX_SPEED)
  run.dist = run.dist + run.speed * dt
  for _, r in ipairs(run.rings) do
    if not r.passed and r.d <= run.dist then
      r.passed = true
      local miss = abs(wrap(run.steer - gap_of(r)))
      if miss < r.half - MARGIN then
        run.passed = run.passed + 1
        -- slack, scaled so a scrape is the outer part of the gap however wide it is
        local room = r.half - MARGIN
        ev[#ev + 1] = { "passed", r, (room - miss) / min(1, room / 0.3) }
      else
        run.over = true
        ev[#ev + 1] = { "crashed", r }
        return
      end
    end
  end
  -- Forget rings well behind, plan new ones ahead.
  local keep = {}
  for _, r in ipairs(run.rings) do if r.d > run.dist - 8 then keep[#keep + 1] = r end end
  run.rings = keep
  lay_out()
end

-- The attract mode's pilot: eases toward the next gap, a little late.
local function autopilot(dt)
  for _, r in ipairs(run.rings) do
    if not r.passed then
      local ahead = (r.d - run.dist) / run.speed
      if ahead < 1.6 then
        local want = gap_of(r) + r.spin * ahead * 0.8
        local diff = wrap(want - run.steer)
        run.steer = wrap(run.steer + diff * min(1, dt * 5.5))
      end
      return
    end
  end
end

-- The world --------------------------------------------------------------------

local rings = {}
for i = 1, RINGS do
  local root = node()
  local rocks = node(look.rock_mesh, look.rock)
  rocks:parent(root)
  local markers = node(look.marker_mesh, look.marker)
  markers:parent(root)
  local edges = node(look.edge_mesh, look.marker)
  edges:parent(root)
  root.visible = false
  rings[i] = { root = root, rocks = rocks, markers = markers, edges = edges, plan = nil }
end

-- One generator for the rings' rocks, seeded again for each ring (a round
-- may make only 256 generators, and a long one builds more rings than that).
local gen = love.math.newRandomGenerator(1)

-- Builds a ring's rocks around everything but its gap (centred at the bottom).
local function build(slot, r)
  slot.plan = r
  -- Each ring's rocks sit their own way, the same every time it's built.
  gen:setSeed(0x0c0e7000 + r.n * 7919)
  local function rand(a, b)
    local u = gen:random()
    if a then return a + (b - a) * u end
    return u
  end
  local tints = look.rock_tints
  local edge = r.half + ROCK / R * 0.9
  local span = TAU - 2 * edge
  local count = max(6, floor(span * R / (ROCK * 1.7)))
  local t = {}
  for k = 0, count - 1 do
    local a = edge + span * k / (count - 1)
    local rr = R + rand(-0.12, 0.12)
    local n = #t
    -- x, y, z, yaw, scale, colour, pitch, roll
    t[n + 1], t[n + 2], t[n + 3] = rr * sin(a), -rr * cos(a), rand(-0.25, 0.25)
    t[n + 4], t[n + 5], t[n + 6] = rand(0, TAU), rand(0.8, 1.25), tints[floor(rand() * #tints) + 1]
    t[n + 7], t[n + 8] = rand(0, TAU), rand(0, TAU)
  end
  -- An outer scatter, so the ring reads as a belt and not a necklace.
  for _ = 1, floor(count * 0.6) do
    local a = edge + rand() * span
    local rr = R + rand(0.55, 1.3)
    local n = #t
    t[n + 1], t[n + 2], t[n + 3] = rr * sin(a), -rr * cos(a), rand(-0.6, 0.6)
    t[n + 4], t[n + 5], t[n + 6] = rand(0, TAU), rand(0.5, 1.0), tints[floor(rand() * #tints) + 1]
    t[n + 7], t[n + 8] = rand(0, TAU), rand(0, TAU)
  end
  slot.rocks:instances(t, 8)
  -- Markers along the gap's arc, and one at each edge.
  local color = r.spin ~= 0 and look.gap_turning or look.gap_still
  local b = {}
  local markers = max(3, floor(2 * r.half * R / 0.45))
  for k = 0, markers - 1 do
    local a = -r.half + 2 * r.half * (k + 0.5) / markers
    local n = #b
    b[n + 1], b[n + 2], b[n + 3], b[n + 4], b[n + 5], b[n + 6] = R * sin(a), -R * cos(a), 0, 0, 1, color
  end
  slot.markers:instances(b, 6)
  local e = {}
  for _, s in ipairs({ -1, 1 }) do
    local a = s * r.half
    local n = #e
    e[n + 1], e[n + 2], e[n + 3], e[n + 4], e[n + 5], e[n + 6] = R * sin(a), -R * cos(a), 0, 0, 1, color
  end
  slot.edges:instances(e, 6)
  slot.root.visible = true
end

local player = look.player()

local CY = -R -- the player's height on screen: the bottom of the tunnel
local lean, shake, crash_t = 0, 0, -1

local function world(dt, steer_rate)
  local dist, steer = run.dist, run.steer
  -- Assign planned rings to pool slots: ring n always lives in slot n % RINGS.
  for _, r in ipairs(run.rings) do
    local slot = rings[r.n % RINGS + 1]
    if slot.plan ~= r then build(slot, r) end
  end
  for _, slot in ipairs(rings) do
    local r = slot.plan
    if r and r.owner == run and r.d > dist - 8 then
      slot.root.z = -(r.d - dist)
      slot.root.rz = gap_of(r) - steer
    elseif r then
      slot.root.visible = false
    end
  end

  -- The player leans into a turn and leaves a trail.
  local now = round.time
  lean = lean + (max(-1, min(1, steer_rate * 0.25)) - lean) * min(1, dt * 8)
  if not run.over then
    player.visible = true
    player.y = CY + 0.15 + 0.05 * sin(now * 7)
    player.rz = -lean * 0.5
    if dt > 0 then
      look.trail:emit(0, CY + 0.15, 0.25, lean * 0.6, 0.15, 1, 3, -lean * 2, 0, run.speed * 0.35)
    end
  end

  for _, e in ipairs(run.events) do
    if e[1] == "passed" then
      look.pass:emit(0, CY + 0.15, -0.5, 0, 0.6, 0, 22, 0, 0, run.speed * 0.6)
      if e[3] < SCRAPE then
        local side = wrap(gap_of(e[2]) - steer) > 0 and -1 or 1 -- the edge we grazed
        look.trail:emit(side * 0.2, CY + 0.15, -0.3, side, 0.4, 0.3, 18)
      end
    elseif e[1] == "crashed" then
      player.visible = false
      look.crash:emit(0, CY + 0.2, -0.3, 0, 0.5, 0.3, 40)
      shake, crash_t = 0.35, now
    end
  end

  -- The chase camera, low behind the player; it shakes on a crash and drifts back after.
  shake = max(0, shake - dt * 0.6)
  local sx = shake * sin(now * 53)
  local sy = shake * cos(now * 47)
  local back = 0
  if crash_t >= 0 then back = min(2.5, (now - crash_t) * 1.2) end
  local fov = 0.95 + (run.speed - START_SPEED) / (MAX_SPEED - START_SPEED) * 0.2
  if round.title then
    local sway = 0.35 * sin(now * 0.5)
    camera(sway, CY + 1.7, 4.6, 0, CY * 0.35, -14, 1.0, 0.1, 160)
  else
    camera(sx - lean * 0.25, CY + 1.45 + sy + back * 0.4, 4.3 + back, 0, CY * 0.45, -14, fov, 0.1, 160)
  end
end

-- Haptics ---------------------------------------------------------------------------
-- A light tap for each ring, a success every 10th, a heavy thud on a crash.
-- (No sound yet: the generate tool makes it, with kind "music" or "sfx".)

local function feel()
  if round.title then return end
  for _, e in ipairs(run.events) do
    if e[1] == "passed" then
      if run.passed % 10 == 0 then haptic("success") else haptic("tap") end
    elseif e[1] == "crashed" then
      haptic("heavy")
    end
  end
end

-- The round ---------------------------------------------------------------------

local steer_rate = 0 -- radians a second, for the player's lean
local font

-- The debug menu (and the "Autopilot" scenario, which sets AUTO): fly the round by itself.
AUTO = false
debug.toggle("Autopilot", function(on) AUTO = on end, AUTO)

function love.load()
  new_run()
  font = love.graphics.newFont(16)
end

-- Drag left and right to slide around the tunnel.
function love.mousemoved(x, y, dx, dy)
  if round.title or not love.mouse.isDown(1) then return end
  if not run.over then run.steer = wrap(run.steer + dx * STEER) end
end

function love.update(dt)
  local before = run.steer
  if round.title then
    autopilot(dt)
    run_step(dt)
    if run.over then new_run() end
  elseif not round.ended then
    if AUTO then autopilot(dt) end
    -- The arrows, A/D, the d-pad or the stick steer too (3 radians a second at full tilt).
    if input.ax ~= 0 and not run.over then run.steer = wrap(run.steer + input.ax * 3 * dt) end
    run_step(dt)
    round.score = run.passed
    if run.over then round.over("BURNT OUT!", "crashed") end
  else
    local ev = run.events
    for i = #ev, 1, -1 do ev[i] = nil end
    run.t = run.t + dt
    -- Momentum carries the wreck on through the ring, slowing to a drift.
    run.speed = run.speed * math.exp(-1.6 * dt)
    run.dist = run.dist + run.speed * dt
  end
  if dt > 0 then
    steer_rate = steer_rate + (wrap(run.steer - before) / dt - steer_rate) * min(1, dt * 12)
  end
  world(dt, steer_rate)
  feel()
end

function love.draw()
  if round.title or round.ended or run.t >= 2.6 then return end
  love.graphics.setFont(font)
  love.graphics.setColor(look.hint)
  local y = love.graphics.getHeight() * 0.6
  love.graphics.printf("DRAG LEFT AND RIGHT", 0, y, love.graphics.getWidth(), "center")
  love.graphics.printf("GO THROUGH THE GAPS", 0, y + 22, love.graphics.getWidth(), "center")
end
