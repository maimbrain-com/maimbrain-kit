-- Comet Lanes: a glowing comet hugs the wall of a dark tunnel of asteroid
-- rings. Each ring is a circle of rocks with one gap; drag left and right to
-- slide the comet around the tunnel and thread the gaps.
--
-- The tunnel frame: an angle a around the tunnel's axis, 0 at the bottom and
-- growing to the right. The comet sits at angle `steer`; a ring's gap is at
-- angle g. On screen the comet is always at the bottom: the tunnel turns
-- around it (each ring is rolled by g - steer).
--
-- The comet stays at z = 0 and the rings fly toward it: a ring d meters along
-- the lane is at z = -(d - dist).

local sin, cos, floor, max, min, abs, pi = math.sin, math.cos, math.floor, math.max, math.min, math.abs, math.pi
local TAU = 2 * pi

local R = 3.0 -- the tunnel's radius: rocks and comet both ride it
local SPACING = 22.0 -- meters between rings
local RINGS = 7 -- the ring pool (fog hides the far ones)
local FIRST = 25.0 -- the first ring: under 2 s at the start speed
local START_SPEED, MAX_SPEED, ACCEL = 13.5, 34.0, 0.55
local ROCK = 0.42 -- a rock's radius
local COMET_MARGIN = 0.1 -- radians of the comet's body inside a gap's edge
local SCRAPE = 0.09 -- a pass with less slack than this (radians) is a scrape
local STEER = pi / 260 -- radians per logical pixel dragged

local function wrap(a)
  a = (a + pi) % TAU
  return a - pi
end


-- A small seeded generator (xorshift32), so a ring's rocks have their own look.
local function seeded(seed)
  local s = bit32.bor((bit32.band(seed, 0xffffffff)), 1)
  return function(a, b)
    s = bit32.bxor(s, (bit32.band((bit32.lshift(s, 13)), 0xffffffff)))
    s = bit32.bxor(s, (bit32.rshift(s, 17)))
    s = bit32.bxor(s, (bit32.band((bit32.lshift(s, 5)), 0xffffffff)))
    local u = s / 4294967296.0
    if a then return a + (b - a) * u end
    return u
  end
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
      if miss < r.half - COMET_MARGIN then
        run.passed = run.passed + 1
        -- slack, scaled so a scrape is the outer part of the gap however wide it is
        local room = r.half - COMET_MARGIN
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

-- The look --------------------------------------------------------------------


sky { seed = 11, colors = { { 0.004, 0.006, 0.02 }, { 0.03, 0.02, 0.08 }, { 0.06, 0.015, 0.09 } }, nebula = 0.8, stars = 0.7, star_brightness = 1.0, sun_disc = 0 }
-- A cold star behind and above the camera lights the rocks' faces.
sun { dir = { 0.35, -0.55, -0.75 }, color = { 0.7, 0.75, 1.0 }, intensity = 3.0, shadows = false }
fog { color = { 0.012, 0.01, 0.035 }, mode = "exp2", density = 0.015, sky = 0.6 }
post { exposure = 1.2, bloom = 0.7, bloom_threshold = 1.0, vignette = 0.45, aberration = 0.15, saturation = 1.15, contrast = 1.08 }
film("subtle", { streaks = 0 })

local INK = { 0.01, 0.008, 0.02 }

-- Rocks: lumpy faceted icos, tinted per instance.
local rock_mesh = mesh.ico(ROCK, 1):scale(1, 0.85, 1.1):color(0xffffffff):flat()
local rock_mat = material { rough = 0.85, style = { toon = 3, rim = 0.55, outline = 1.2, ink = INK } }
-- Gap lights: glowing beads strung across the gap, beacons at its edges.
local bead_mesh = mesh.ico(0.09, 1)
local beacon_mesh = mesh.ico(0.22, 1)
local bead_mat = material { color = { 0, 0, 0, 1 }, emissive = { 1, 1, 1 }, glow = 3.0, unlit = true }

local tints = { 0x8a7a6cff, 0x6f675fff, 0x9a8670ff, 0x5f5a66ff, 0x7d6a5aff }

local rings = {}
for i = 1, RINGS do
  local root = node()
  local rocks = node(rock_mesh, rock_mat)
  rocks:parent(root)
  local beads = node(bead_mesh, bead_mat)
  beads:parent(root)
  local beacons = node(beacon_mesh, bead_mat)
  beacons:parent(root)
  root:show(false)
  rings[i] = { root = root, rocks = rocks, beads = beads, beacons = beacons, plan = nil }
end

-- Builds a ring's rocks around everything but its gap (centred at the bottom).
local function build(slot, r)
  slot.plan = r
  local rand = seeded(bit32.bxor(0x0c0e7000, (r.n * 7919)))
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
  -- Beads along the gap's arc: warm when it's still, cyan when it turns.
  local color = r.spin ~= 0 and 0x5ff2ffff or 0xffc25aff
  local b = {}
  local beads = max(3, floor(2 * r.half * R / 0.45))
  for k = 0, beads - 1 do
    local a = -r.half + 2 * r.half * (k + 0.5) / beads
    local n = #b
    b[n + 1], b[n + 2], b[n + 3], b[n + 4], b[n + 5], b[n + 6] = R * sin(a), -R * cos(a), 0, 0, 1, color
  end
  slot.beads:instances(b, 6)
  local e = {}
  for _, s in ipairs({ -1, 1 }) do
    local a = s * r.half
    local n = #e
    e[n + 1], e[n + 2], e[n + 3], e[n + 4], e[n + 5], e[n + 6] = R * sin(a), -R * cos(a), 0, 0, 1, color
  end
  slot.beacons:instances(e, 6)
  slot.root:show(true)
end

-- Two lights hang in the nearest gaps.
local gap_lights = {}
for i = 1, 2 do
  gap_lights[i] = node()
  gap_lights[i]:light({ 1.0, 0.7, 0.3 }, 3.0, 7)
end

-- The comet: a white-hot core in a glowing shell, with a tail of cones.
local comet = node()
local core = node(mesh.ico(0.17, 2), material { color = { 0, 0, 0, 1 }, emissive = { 2.6, 2.4, 2.0 }, glow = 1.2, unlit = true })
core:parent(comet)
local shell = node(mesh.ico(0.27, 2), material { color = { 0.2, 0.45, 1.0, 0.3 }, unlit = true, additive = true, alpha = true })
shell:parent(comet)
local tail = node(mesh.cone(0.22, 1.9, 14):turn(0, pi / 2, 0):move(0, 0, 1.0), material { color = { 0.08, 0.22, 0.6, 0.3 }, unlit = true, additive = true, alpha = true, double = true })
tail:parent(comet)
local tail2 = node(mesh.cone(0.1, 1.2, 10):turn(0, pi / 2, 0):move(0, 0, 0.65), material { color = { 0.6, 0.5, 0.35, 0.4 }, unlit = true, additive = true, alpha = true, double = true })
tail2:parent(comet)
local comet_light = node()
comet_light:parent(comet)
comet_light:light({ 0.55, 0.8, 1.0 }, 4.0, 9)

local sparks = emitter {
  colors = { { 5, 4.2, 2.4, 1 }, { 1.6, 2.4, 4.5, 1 }, { 0.4, 0.8, 2.0, 0.6 }, { 0, 0, 0, 0 } },
  sizes = { 0.05, 0.04, 0.025, 0 }, life = { 0.2, 0.4 }, speed = { 1.0, 3.0 },
  spread = 0.7, drag = 1.5, stretch = 0.02,
}
local dust = emitter {
  colors = { { 0, 0, 0, 0 }, { 0.6, 0.7, 1.1, 0.8 }, { 0.5, 0.6, 1.0, 0.6 }, { 0, 0, 0, 0 } },
  sizes = { 0.03, 0.04, 0.04, 0.03 }, life = { 1.6, 2.4 }, speed = { 0, 0.1 }, stretch = 0.02,
}
local debris = emitter {
  colors = { { 1.2, 1.0, 0.85, 1 }, { 0.45, 0.38, 0.32, 1 }, { 0.25, 0.2, 0.18, 0.8 }, { 0, 0, 0, 0 } },
  sizes = { 0.22, 0.18, 0.14, 0.05 }, life = { 1.2, 2.2 }, speed = { 2, 7 }, spread = 1.0, drag = 0.8,
}
local flash = emitter {
  colors = { { 8, 6, 3, 1 }, { 4, 2.2, 0.8, 1 }, { 1.2, 0.4, 0.1, 0.6 }, { 0, 0, 0, 0 } },
  sizes = { 0.12, 0.1, 0.06, 0 }, life = { 0.4, 0.9 }, speed = { 4, 11 }, spread = 1.0, drag = 2.0, stretch = 0.05,
}
local pass_burst = emitter {
  colors = { { 3, 2.2, 0.8, 1 }, { 2, 1.2, 0.4, 0.8 }, { 0.8, 0.4, 0.1, 0.4 }, { 0, 0, 0, 0 } },
  sizes = { 0.08, 0.07, 0.05, 0 }, life = { 0.3, 0.6 }, speed = { 2, 5 }, spread = 1.0, drag = 3.0, stretch = 0.03,
}

-- The world, once a frame ------------------------------------------------------

local CY = -R -- the comet's height on screen: the bottom of the tunnel
local lean, shake, crash_t = 0, 0, -1
local next_dust = 0

local function world(dt, steer_rate)
  local dist, steer = run.dist, run.steer
  -- Assign planned rings to pool slots: ring n always lives in slot n % RINGS.
  for _, r in ipairs(run.rings) do
    local slot = rings[r.n % RINGS + 1]
    if slot.plan ~= r then build(slot, r) end
  end
  local lit = 0
  for _, slot in ipairs(rings) do
    local r = slot.plan
    if r and r.owner == run and r.d > dist - 8 then
      local z = -(r.d - dist)
      local roll = gap_of(r) - steer
      slot.root:pos(0, 0, z)
      slot.root:turn(0, 0, roll)
      -- The two nearest unpassed gaps get a light.
      if not r.passed and lit < 2 and z > -70 then
        lit = lit + 1
        local a = roll
        gap_lights[lit]:pos(R * 0.8 * sin(a), -R * 0.8 * cos(a), z + 0.5)
      end
    elseif r then
      slot.root:show(false)
    end
  end
  for i = lit + 1, 2 do gap_lights[i]:pos(0, 0, 50) end

  -- Space dust streams past and turns with the tunnel.
  local flow = run.over and run.speed * 0.15 or run.speed
  next_dust = next_dust - dt
  while next_dust <= 0 do
    next_dust = next_dust + 0.03
    local a, rr = love.math.random() * (TAU), 1.0 + love.math.random() * (5)
    dust:emit(rr * sin(a), rr * cos(a), -(15 + love.math.random() * (55)), 0, 0, 0, 1, 0, 0, flow)
  end

  -- The comet leans into a turn and trails sparks.
  lean = lean + (max(-1, min(1, steer_rate * 0.25)) - lean) * min(1, dt * 8)
  if not run.over then
    comet:show(true)
    local bob = 0.05 * sin(round.time * 7)
    comet:pos(0, CY + 0.15 + bob, 0)
    comet:turn(0, 0, -lean * 0.5)
    core:turn(round.time * 3, round.time * 2, 0)
    if dt > 0 then
      sparks:emit(0, CY + 0.15, 0.25, lean * 0.6, 0.15, 1, 3, -lean * 2, 0, run.speed * 0.35)
    end
  end

  for _, e in ipairs(run.events) do
    if e[1] == "passed" then
      pass_burst:emit(0, CY + 0.15, -0.5, 0, 0.6, 0, 22, 0, 0, run.speed * 0.6)
      if e[3] < SCRAPE then
        local side = wrap(gap_of(e[2]) - steer) > 0 and -1 or 1 -- the edge we grazed
        sparks:emit(side * 0.2, CY + 0.15, -0.3, side, 0.4, 0.3, 18)
      end
    elseif e[1] == "crashed" then
      comet:show(false)
      debris:emit(0, CY + 0.2, -0.3, 0, 0.5, 0.3, 40)
      flash:emit(0, CY + 0.2, -0.3, 0, 0.3, 0.4, 60)
      sparks:emit(0, CY + 0.2, -0.3, 0, 0.5, 0, 30)
      shake, crash_t = 0.35, round.time
    end
  end

  -- The chase camera, low behind the comet; it shakes on a crash and drifts back after.
  shake = max(0, shake - dt * 0.6)
  local sx = shake * sin(round.time * 53)
  local sy = shake * cos(round.time * 47)
  local back = 0
  if crash_t >= 0 then back = min(2.5, (round.time - crash_t) * 1.2) end
  local fov = 0.95 + (run.speed - START_SPEED) / (MAX_SPEED - START_SPEED) * 0.2
  if round.title then
    local sway = 0.35 * sin(round.time * 0.5)
    camera(sway, CY + 1.7, 4.6, 0, CY * 0.35, -14, 1.0, 0.1, 160)
  else
    camera(sx - lean * 0.25, CY + 1.45 + sy + back * 0.4, 4.3 + back, 0, CY * 0.45, -14, fov, 0.1, 160)
  end
end

-- Haptics ---------------------------------------------------------------------------
-- A light tap for each ring, a success every 10th, a heavy thud on a crash.
-- (No sound yet: generate(kind: "music") and generate(kind: "sfx") make it.)

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

new_run()

local drag_x = nil
local steer_rate = 0 -- radians a second, for the comet's lean
local moved = 0

-- Drag left and right to steer: the comet slides by how far the finger moves.
function love.mousepressed(x, y)
  drag_x = x
end

function love.mousemoved(x, y)
  if not drag_x or not love.mouse.isDown(1) then return end
  local dx = x - drag_x
  drag_x = x
  if not run.over then
    run.steer = wrap(run.steer + dx * STEER)
    moved = moved + dx * STEER
  end
end

function love.mousereleased() drag_x = nil end

-- The debug menu (and the "Autopilot" scenario): fly the round by itself.
AUTO = false

debug.toggle("Autopilot", function(on) AUTO = on end, AUTO)

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
    if run.over then round.over("CRASHED!", "crashed") end
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

-- The score is the HUD's, in the identity (studio.toml); the first seconds say how to play.
function love.draw()
  if round.title or round.ended or run.t >= 2.6 then return end
  local w, h = love.graphics.getDimensions()
  love.graphics.setColor(0.75, 0.89, 1, 0.82)
  love.graphics.printf("DRAG LEFT AND RIGHT", 0, h * 0.6, w, "center")
  love.graphics.printf("FLY THROUGH THE LIT GAPS", 0, h * 0.6 + 22, w, "center")
end
