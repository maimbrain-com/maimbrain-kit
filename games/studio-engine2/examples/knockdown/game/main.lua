-- Knockdown: a fairground can toss at dusk. Tap where to throw: the ball's
-- arc is solved to pass through what's under your finger. Knock every can
-- and crate off the table before the balls run out; four balls a stack,
-- and the ones you don't use carry over to the next, bigger stack.
--
-- The Rust kit's Knockdown (games/knockdown) rebuilt in Lua, on its numbers:
-- the table, the cans, crates and planks, the ball and its 11 m/s throw,
-- the camera, the stacks and what counts as down are sim.rs's.
--
-- Godot's nodes: RigidBody3D cans (CylinderShape3D), crates, planks and the
-- ball (continuousCd), StaticBody3D floor, table and back wall;
-- camera:ray + physics.raycast to aim; bodyEntered for the clangs.

local sqrt, max, min, abs, floor, pi = math.sqrt, math.max, math.min, math.abs, math.floor, math.pi

-- The stall (meters): the table's top, centre and size.
TABLE_Y, TABLE_Z = 0.8, -5.0
TABLE_W, TABLE_H, TABLE_D = 2.6, 0.12, 0.7
CAN_R, CAN_H = 0.11, 0.27
CRATE = 0.3
PLANK_W, PLANK_H, PLANK_D = 0.9, 0.06, 0.3
BALL_R = 0.09
THROW_SPEED = 11.0          -- m/s: the throw passes through the tapped point
local BALLS_PER_STACK = 4
local COOLDOWN = 0.35       -- seconds between throws
local SETTLE_MAX = 3.5      -- after the last ball: judge once things stop, or this long
local CLEAR_BEAT = 1.6      -- "CLEAR!" before the next stack builds

-- Masses from sim.rs's densities (kg/m³ × volume).
local CAN_MASS = 110 * pi * CAN_R * CAN_R * CAN_H
local CRATE_MASS = 180 * CRATE ^ 3
local PLANK_MASS = 400 * PLANK_W * PLANK_H * PLANK_D
local BALL_MASS = 2200 * 4 / 3 * pi * BALL_R ^ 3

-- The camera: sim.rs's, a thrower's eye a little above the table.
CAM = { x = 0, y = 1.5, z = TABLE_Z + 3.2, lx = 0, ly = 1.05, lz = TABLE_Z, fov = 0.8 * 180 / pi }
camera.x, camera.y, camera.z = CAM.x, CAM.y, CAM.z
camera:lookAt(CAM.lx, CAM.ly, CAM.lz)
camera.fov = CAM.fov

-- What the bot (game/bot.lua) reads: it can't see main.lua's locals, and
-- scene.all doesn't list 3D nodes, so the round keeps its own lists in
-- game/shared.lua.
local K = require("shared") :: any   -- the round adds its own fields as it goes
K.CAN_R, K.CAN_H, K.CRATE, K.PLANK_W, K.PLANK_H = CAN_R, CAN_H, CRATE, PLANK_W, PLANK_H

-- The debug menu's scenarios (debug/scenarios.toml) set these: start at a stack.
JUMP_TO, JUMP_LAYOUT = nil :: number?, nil :: number?

-- The look ----------------------------------------------------------------------

world.sky{ atmosphere = "dusk", clouds = { cover = 0.3 } }
world.sun{ dir = { 0.45, -0.6, -0.65 }, color = 0xffdcb4, intensity = 2.8, shadows = true, shadowDistance = 10 }
world.post{ exposure = 1.05, bloom = 0.45, bloomThreshold = 1.0, vignette = 0.3, saturation = 1.1, contrast = 1.05 }
world.grade("goldenHour", 0.15)

local CREAM, RED, NAVY = 0xfff3dc, 0xd7323a, 0x3a5f8f
local CAN_COLORS = { 0xe63946, 0x2a9d8f, 0xf4a261, 0x4361ee }

local plain = material{ roughness = 0.8 }
local cloth = material{ roughness = 0.95 }
local tin = material{ metalness = 0.3, roughness = 0.35 }
local wood = material{ roughness = 0.75 }
local glowBulb = material{ color = 0xfff1c0, emissive = 0xffd27a, emissiveIntensity = 4, unlit = true }

-- Vertical stripes: n panels across w, alternating a and b, the first at
-- the origin and the rest to its right.
local function stripes(w, h, d, n, a, b)
  local pw = w / n
  local g = geometry.box(pw, h, d):paint(a)
  for i = 1, n - 1 do
    g:merge(geometry.box(pw, h, d):paint(i % 2 == 0 and a or b), i * pw, 0, 0)
  end
  return g
end

-- Stripes centred at (x, y, z).
local function striped(w, h, d, n, a, b, x, y, z, parent)
  return Mesh{ geometry = stripes(w, h, d, n, a, b), material = cloth, x = x - w / 2 + w / n / 2, y = y, z = z, parent = parent }
end

-- The floor: boards.
floorBody = StaticBody3D{ shape = BoxShape3D{ w = 30, h = 1, d = 30 }, y = -0.5, z = -3, friction = 0.9 }
do
  local g = geometry.box(0.5, 0.04, 14):paint(0xc9a77c)
  for i = 1, 11 do
    g:merge(geometry.box(0.48, 0.04, 14):paint(i % 2 == 0 and 0xc9a77c or 0xb48d62), i * 0.5, 0, 0)
  end
  Mesh{ geometry = g, material = plain, x = -2.75, y = -0.02, z = -3 }
end

-- The table: a dark wooden top over a red and cream skirt.
tableBody = StaticBody3D{ shape = BoxShape3D{ w = TABLE_W, h = TABLE_H, d = TABLE_D }, y = TABLE_Y - TABLE_H / 2, z = TABLE_Z, friction = 0.7 }
Mesh{ geometry = geometry.box(TABLE_W, TABLE_H, TABLE_D):paint(0x8d5a3b), material = wood, parent = tableBody }
Mesh{ geometry = geometry.box(TABLE_W + 0.04, 0.035, TABLE_D + 0.04):paint(0x6b3f27), material = wood, y = TABLE_Y - TABLE_H - 0.01, z = TABLE_Z }
do
  local skirt = TABLE_Y - TABLE_H
  striped(TABLE_W, skirt, 0.03, 12, CREAM, RED, 0, skirt / 2, TABLE_Z + TABLE_D / 2 - 0.02)
  -- A gold trim along the skirt's top edge.
  Mesh{ geometry = geometry.box(TABLE_W + 0.02, 0.04, 0.04):paint(0xf2c14e), material = tin, y = skirt - 0.02, z = TABLE_Z + TABLE_D / 2 }
  -- A painted board on the skirt.
  local sz = TABLE_Z + TABLE_D / 2 + 0.01
  Mesh{ geometry = geometry.box(0.92, 0.24, 0.03):paint(0x7a1e24), material = wood, y = 0.42, z = sz }
  Mesh{ geometry = geometry.box(0.86, 0.18, 0.03):paint(CREAM), material = wood, y = 0.42, z = sz + 0.006 }
  Text{ text = "KNOCK 'EM ALL DOWN", size = 0.07, color = 0xb3202a, font = "Rye", billboard = true, x = 0, y = 0.42, z = sz + 0.03 }
end

-- The back wall catches wild throws: navy and cream stripes.
backWall = StaticBody3D{ shape = BoxShape3D{ w = 8, h = 3, d = 0.2 }, y = 1.5, z = TABLE_Z - 2.2 }
striped(7.0, 3.2, 0.1, 14, CREAM, NAVY, 0, 1.6, TABLE_Z - 2.12)

-- Prizes: plush bears hanging from a gold rail either side of the stack.
local function bear(color)
  local g = geometry.sphere(0.13, 16, 12):paint(color)                 -- body
  g:merge(geometry.sphere(0.095, 16, 12):paint(color), 0, 0.18, 0)     -- head
  g:merge(geometry.sphere(0.04, 10, 8):paint(color), -0.07, 0.26, 0)   -- ears
  g:merge(geometry.sphere(0.04, 10, 8):paint(color), 0.07, 0.26, 0)
  g:merge(geometry.sphere(0.045, 10, 8):paint(0xf5e3c8), 0, 0.16, 0.075) -- snout
  g:merge(geometry.sphere(0.014, 8, 6):paint(0x1d1b2c), 0, 0.175, 0.118) -- nose
  g:merge(geometry.sphere(0.012, 8, 6):paint(0x1d1b2c), -0.035, 0.21, 0.083) -- eyes
  g:merge(geometry.sphere(0.012, 8, 6):paint(0x1d1b2c), 0.035, 0.21, 0.083)
  g:merge(geometry.sphere(0.05, 10, 8):paint(color), -0.12, 0.03, 0.03) -- arms
  g:merge(geometry.sphere(0.05, 10, 8):paint(color), 0.12, 0.03, 0.03)
  g:merge(geometry.sphere(0.055, 10, 8):paint(color), -0.07, -0.1, 0.07) -- legs
  g:merge(geometry.sphere(0.055, 10, 8):paint(color), 0.07, -0.1, 0.07)
  g:merge(geometry.sphere(0.06, 10, 8):paint(0xf5e3c8), 0, -0.01, 0.1)  -- tummy
  return g
end
local plush = material{ roughness = 1.0 }
do
  local z, rail = TABLE_Z - 2.0, 2.62
  Mesh{ geometry = geometry.cylinder(0.02, 5, 8):paint(0xf2c14e), material = tin, rz = pi / 2, y = rail, z = z }
  local hung = { { -1.2, 0xffd166, 0.30 }, { -0.68, 0xf28cb1, 0.16 }, { 0.68, 0x7ec8e3, 0.22 }, { 1.2, 0xb388eb, 0.12 } }
  for i, h in ipairs(hung) do
    local x, drop = h[1], h[3]
    Mesh{ geometry = geometry.cylinder(0.004, drop, 4):paint(0xeeeeee), material = plain, x = x, y = rail - drop / 2, z = z }
    Mesh{ geometry = bear(h[2]), material = plush, x = x, y = rail - drop - 0.3, z = z, rz = (i % 2 == 0) and 0.1 or -0.12 }
  end
end

-- The awning across the top: a striped valance with scallops and a string
-- of bulbs, which warms the stall from the front.
do
  local z, bottom, n, w = TABLE_Z + 1.0, 2.08, 14, 3.5
  local pw = w / n
  striped(w, 0.5, 0.04, n, RED, CREAM, 0, bottom + 0.25, z)
  local scallops = geometry.cylinder(pw / 2, 0.035, 18):paint(RED)
  for i = 1, n - 1 do
    scallops:merge(geometry.cylinder(pw / 2, 0.035, 18):paint(i % 2 == 0 and RED or CREAM), i * pw, 0, 0)
  end
  Mesh{ geometry = scallops, material = cloth, x = -w / 2 + pw / 2, y = bottom, z = z, rx = pi / 2 }
  local bulbs = geometry.sphere(0.022, 10, 8)
  for i = 1, n - 1 do bulbs:merge(geometry.sphere(0.022, 10, 8), i * pw, 0, 0) end
  Mesh{ geometry = bulbs, material = glowBulb, x = -w / 2 + pw / 2, y = bottom - pw / 2 - 0.01, z = z + 0.03 }
  Light{ color = 0xffc46b, intensity = 3, range = 4.5, x = -0.6, y = bottom - 0.15, z = z - 0.1 }
  Light{ color = 0xffc46b, intensity = 3, range = 4.5, x = 0.6, y = bottom - 0.15, z = z - 0.1 }
  Light{ color = 0xffa860, intensity = 2, range = 5, x = 0, y = 2.3, z = TABLE_Z - 1.2 }
end

-- The targets ---------------------------------------------------------------------

local LABEL, RIM = 0xf8f4ec, 0xc8c8d0

-- A tin can: rims, its colour, a cream label band with a stripe.
local function canGeometry(color)
  local h = CAN_H
  local g = geometry.cylinder(CAN_R * 1.004, h * 0.28, 20):paint(LABEL)
  g:merge(geometry.cylinder(CAN_R * 1.008, h * 0.05, 20):paint(color), 0, 0, 0)
  g:merge(geometry.cylinder(CAN_R, h * 0.3, 20):paint(color), 0, -h * 0.29, 0)
  g:merge(geometry.cylinder(CAN_R, h * 0.3, 20):paint(color), 0, h * 0.29, 0)
  g:merge(geometry.cylinder(CAN_R * 0.96, h * 0.06, 20):paint(RIM), 0, -h * 0.47, 0)
  g:merge(geometry.cylinder(CAN_R * 0.96, h * 0.06, 20):paint(RIM), 0, h * 0.47, 0)
  return g
end
local cans = {}
for i, c in ipairs(CAN_COLORS) do cans[i] = canGeometry(c) end

-- A crate: light boards with darker edge battens.
local crateGeo
do
  local s = CRATE
  crateGeo = geometry.box(s * 0.98, s * 0.98, s * 0.98):paint(0xd4a16a)
  local t = s * 0.12
  local e = s / 2 - t / 2 + 0.004
  for _, a in ipairs({ -1, 1 }) do
    for _, b in ipairs({ -1, 1 }) do
      crateGeo:merge(geometry.box(s * 1.01, t, t):paint(0x8a5a2b), 0, a * e, b * e)
      crateGeo:merge(geometry.box(t, s * 1.01, t):paint(0x8a5a2b), a * e, 0, b * e)
      crateGeo:merge(geometry.box(t, t, s * 1.01):paint(0x8a5a2b), a * e, b * e, 0)
    end
  end
end
local plankGeo = geometry.box(PLANK_W, PLANK_H, PLANK_D):paint(0xd9a066)

local function addTarget(kind, x, y, z)
  local b
  if kind == "can" then
    b = RigidBody3D{ shape = CylinderShape3D{ radius = CAN_R, height = CAN_H }, mass = CAN_MASS, friction = 0.5, bounce = 0.25,
                     x = x, y = y, z = z, contactMonitor = true, data = { kind = "can" } }
    Mesh{ geometry = cans[love.math.random(1, #cans)], material = tin, parent = b }
  elseif kind == "crate" then
    b = RigidBody3D{ shape = BoxShape3D{ w = CRATE, h = CRATE, d = CRATE }, mass = CRATE_MASS, friction = 0.7, bounce = 0.1,
                     x = x, y = y, z = z, contactMonitor = true, data = { kind = "crate" } }
    Mesh{ geometry = crateGeo, material = wood, parent = b }
  else
    b = RigidBody3D{ shape = BoxShape3D{ w = PLANK_W, h = PLANK_H, d = PLANK_D }, mass = PLANK_MASS, friction = 0.8,
                     x = x, y = y, z = z, contactMonitor = true, data = { kind = "plank" } }
    Mesh{ geometry = plankGeo, material = wood, parent = b }
  end
  local t = { body = b, kind = kind, down = false, vx = 0, vy = 0, vz = 0 }
  b.data.t = t
  b.bodyEntered:connect(function(other) K.contact(t, other) end)
  K.targets[#K.targets + 1] = t
end

local function canPyramid(base, x, z, y0)
  local gap = CAN_R * 2 + 0.004
  for row = 0, base - 1 do
    local n = base - row
    for i = 0, n - 1 do
      addTarget("can", x + (i - (n - 1) / 2) * gap, y0 + CAN_H / 2 + row * (CAN_H + 0.002), z)
    end
  end
end

-- Builds the next stack: sim.rs's next_level. It grows with the stack;
-- random within designed shapes from stack 4 on.
local function nextStack()
  for _, t in ipairs(K.targets) do t.body:destroy() end
  K.targets = {}
  K.stack = K.stack + 1
  K.ballsLeft = K.ballsLeft + BALLS_PER_STACK
  K.phase = "aim"
  K.waiting = nil
  local top, z = TABLE_Y, TABLE_Z
  local s = K.stack
  if s == 1 then
    canPyramid(3, 0, z, top)
  elseif s == 2 then
    canPyramid(4, 0, z, top)
  elseif s == 3 then
    -- Two crates with a plank across, cans on top.
    addTarget("crate", -0.32, top + CRATE / 2, z)
    addTarget("crate", 0.32, top + CRATE / 2, z)
    addTarget("plank", 0, top + CRATE + PLANK_H / 2, z)
    canPyramid(3, 0, z, top + CRATE + PLANK_H)
  else
    local variant = K.force or love.math.random(0, 2)
    K.force = nil
    local extra = min(s - 4, 3)
    if variant == 0 then
      -- Two pyramids side by side.
      canPyramid(3 + min(extra, 1), -0.55, z, top)
      canPyramid(3 + min(extra, 1), 0.55, z, top)
    elseif variant == 1 then
      -- A crate wall with cans along the top.
      local n = 3 + min(extra, 2)
      local rows = 2 + floor(extra / 2)
      for r = 0, rows - 1 do
        for i = 0, n - 1 do
          local off = (r % 2 == 1) and CRATE * 0.5 or 0
          addTarget("crate", (i - (n - 1) / 2) * (CRATE + 0.01) + off - CRATE * 0.25, top + CRATE / 2 + r * (CRATE + 0.004), z)
        end
      end
      local y = top + rows * (CRATE + 0.004)
      for i = 0, n - 1 do
        addTarget("can", (i - (n - 1) / 2) * (CRATE + 0.01), y + CAN_H / 2, z)
      end
    else
      -- A tall tower: crates, plank, cans, plank, cans.
      addTarget("crate", -0.3, top + CRATE / 2, z)
      addTarget("crate", 0.3, top + CRATE / 2, z)
      local y = top + CRATE
      for k = 0, 1 + min(extra, 1) do
        addTarget("plank", 0, y + PLANK_H / 2, z)
        y = y + PLANK_H
        local n = 3 - min(k, 1)
        for i = 0, n - 1 do
          addTarget("can", (i - (n - 1) / 2) * 0.3, y + CAN_H / 2, z)
        end
        y = y + CAN_H + 0.002
      end
    end
  end
  ui.hud{ level = "STACK " .. s }
end

-- Sound ---------------------------------------------------------------------------

local sfx = {}
function love.load()
  for _, name in ipairs({ "clang", "knock", "throw", "tick", "clear", "over" }) do
    sfx[name] = love.audio.newSource(name, "static")
  end
  sfx.ticks = {}
  for i = 1, 8 do sfx.ticks[i] = sfx.tick:clone() end
  bigFont = love.graphics.newFont("bold", 44)
  smallFont = love.graphics.newFont("bold", 16)
end
local tickN = 0
local PENTA = { 0, 2, 4, 7, 9, 12, 14, 16 }

-- Throwing ------------------------------------------------------------------------

local G = -select(2, physics.getGravity())
local ballGeo = geometry.sphere(BALL_R, 20, 14):paint(0xf8f9fa)
ballGeo:merge(geometry.torus(BALL_R * 0.985, 0.014, 24, 8):paint(0xe63946), 0, 0, 0)
local ballMat = material{ roughness = 0.35 }

-- What the tap at (sx, sy) aims at: what the ray from the camera meets,
-- else a point 7 m out.
function aimPoint(sx, sy)
  local ox, oy, oz, dx, dy, dz = camera:ray(sx, sy)
  local skip = {}
  for _, b in ipairs(K.balls) do skip[#skip + 1] = b.body end
  local hit = physics.raycast(ox, oy, oz, ox + dx * 30, oy + dy * 30, oz + dz * 30, { exclude = skip })
  if hit then return hit.x, hit.y, hit.z end
  return ox + dx * 7, oy + dy * 7, oz + dz * 7
end

local lastAim -- { x, y, t } for the fading ring

-- Throws a ball at what's under the screen point; false if it can't now.
function throw(sx, sy)
  if K.phase ~= "aim" or K.ballsLeft == 0 or K.cooldown > 0 then return false end
  local tx, ty, tz = aimPoint(sx, sy)
  local x0, y0, z0 = CAM.x, CAM.y - 0.35, CAM.z - 0.4
  local dx, dy, dz = tx - x0, ty - y0, tz - z0
  local t = max(sqrt(dx * dx + dy * dy + dz * dz) / THROW_SPEED, 0.05)
  local b = RigidBody3D{ shape = SphereShape3D{ radius = BALL_R }, mass = BALL_MASS, bounce = 0.3, friction = 0.6,
                         continuousCd = true, contactMonitor = true, x = x0, y = y0, z = z0, data = { kind = "ball" } }
  Mesh{ geometry = ballGeo, material = ballMat, parent = b }
  b.vx, b.vy, b.vz = dx / t, dy / t + 0.5 * G * t, dz / t
  b.vrx = -8
  local ball = { body = b, age = 0, speed = THROW_SPEED }
  b.bodyEntered:connect(function(other) K.ballHit(ball, other) end)
  K.balls[#K.balls + 1] = ball
  if not K.infinite then K.ballsLeft = K.ballsLeft - 1 end
  K.cooldown = COOLDOWN
  K.combo = 0
  K.throws = (K.throws or 0) + 1
  K.thrownAt = K.clock
  if K.ballsLeft == 0 then K.waiting = 0 end
  if not round.title then
    sfx.throw:clone():play()
    lastAim = { x = sx, y = sy, t = 0 }
  end
  return true
end

-- Contacts ------------------------------------------------------------------------

-- Sparks off tin and dust off wood: emitters moved to each hit.
local sparks = Emitter{ preset = "sparks", rate = 0, colors = { 0xfff1a8, 0xffb347 }, sizes = { 0.018, 0.012, 0.006, 0 },
                        life = { 0.12, 0.3 }, speed = { 1.2, 3.2 }, gravity = 6, additive = true }
local dust = Emitter{ preset = "dust", rate = 0, colors = { 0xd8c4a0, 0xb8a080 }, sizes = { 0.04, 0.09, 0.13, 0.16 },
                      life = { 0.4, 0.8 }, speed = { 0.3, 0.9 } }
local function puff(e, x, y, z, n)
  e.x, e.y, e.z = x, y, z
  e:emit(n)
end

local function speedOf(o)
  local t = o.data and o.data.t
  if t then return t.vx, t.vy, t.vz end
  return 0, 0, 0
end

-- A target touched something: a clang (a can) or a knock (wood), as hard as
-- the hit, where it happened. Speeds are last frame's, from before the hit.
function K.contact(t, other)
  local ax, ay, az = t.vx, t.vy, t.vz
  local bx, by, bz = speedOf(other)
  if other.data and other.data.kind == "ball" then
    local ball = other.data.ball
    if ball then bx, by, bz = ball.vx or 0, ball.vy or 0, ball.vz or 0 end
  end
  local speed = sqrt((ax - bx) ^ 2 + (ay - by) ^ 2 + (az - bz) ^ 2)
  if speed < 0.8 then return end
  local src = (t.kind == "can") and sfx.clang or sfx.knock
  local p = t.body
  src:setPosition(p.x, p.y, p.z)
  audio.impact(src, speed, 8)
end

-- The ball hit something: sparks off tin, dust off wood, and a shake for a hard hit.
function K.ballHit(ball, other)
  local b = ball.body
  if not b.alive then return end
  local kind = other.data and other.data.kind
  local speed = ball.speed or 0
  if kind == "can" or kind == "crate" or kind == "plank" then
    local x, y, z = (b.x + other.x) / 2, (b.y + other.y) / 2, (b.z + other.z) / 2
    if kind == "can" then
      puff(sparks, x, y, z, floor(4 + speed * 1.5))
    else
      puff(dust, x, y, z, floor(3 + speed * 0.6))
    end
    if speed > 5 and not round.title then
      fx.shake(min(0.03, 0.002 * speed), 0.2)
      haptic("tap")
    end
  elseif speed > 3 then
    puff(dust, b.x, b.y - BALL_R, b.z, 3)
  end
end

-- The round -----------------------------------------------------------------------

local hint
local clearAt -- K.clock when the stack was cleared
local carried = 0

local function standing()
  local n = 0
  for _, t in ipairs(K.targets) do if not t.down then n = n + 1 end end
  return n
end

-- Knocked down: off the table, or (cans) tipped over on it. sim.rs's rule.
local function checkDown()
  local hx, hz = TABLE_W / 2, TABLE_D / 2
  for _, t in ipairs(K.targets) do
    if not t.down then
      local b = t.body
      local offEdge = abs(b.x) > hx + 0.05 or abs(b.z - TABLE_Z) > hz + 0.05
      -- The can's up axis: its y after the turns (y, then x, then z) is cos(rx)·cos(rz).
      local tipped = t.kind == "can" and math.cos(b.rx) * math.cos(b.rz) < 0.5
      if b.y < TABLE_Y - 0.12 or (offEdge and b.y < TABLE_Y + 0.05) or tipped then
        t.down = true
        if not round.title and not round.ended then
          round.score = round.score + 1
          K.combo = (K.combo or 0) + 1
          tickN = tickN % #sfx.ticks + 1
          local s = sfx.ticks[tickN]
          s:setPitch(2 ^ (PENTA[min(K.combo, #PENTA)] / 12))
          s:play()
          fx.popup("+1", b.x, b.y + 0.22, b.z, { color = 0xffd23f, size = 0.1 })
        end
      end
    end
  end
end

local function anythingMoving()
  for _, ball in ipairs(K.balls) do
    if ball.age < 4 and ball.speed > 0.5 then return true end
  end
  for _, t in ipairs(K.targets) do
    if not t.body.sleeping and (t.vx * t.vx + t.vy * t.vy + t.vz * t.vz) > 0.01 then return true end
  end
  return false
end

nextStack()

-- The debug menu: jump to any stack (and from 4 on, its layout), clear it,
-- run out, or throw forever.
local function jump(stack, variant)
  for _, ball in ipairs(K.balls) do ball.body:destroy() end
  K.balls = {}
  K.stack, K.ballsLeft, K.force = stack - 1, 0, variant
  nextStack()
end
for _, j in ipairs({ { "Stack 1 · small pyramid", 1 }, { "Stack 2 · big pyramid", 2 }, { "Stack 3 · crates and a plank", 3 },
                     { "Stack 4 · twin pyramids", 4, 0 }, { "Stack 4 · crate wall", 4, 1 }, { "Stack 4 · plank tower", 4, 2 } }) do
  debug.action(j[1], function() jump(j[2], j[3]) end)
end
debug.action("Clear the stack", function()
  for _, t in ipairs(K.targets) do
    if not t.down then t.down = true; round.score = round.score + 1 end
  end
end)
debug.action("Out of balls", function() K.infinite = false; K.ballsLeft = 0; K.waiting = SETTLE_MAX + 1 end)
debug.toggle("Infinite balls", function(on) K.infinite = on end, false)

-- The title card's thrower: a ball at a random can every couple of seconds.
local botAt = 1.0
local function autopilot()
  if K.clock < botAt then return end
  botAt = K.clock + 1.6 + love.math.random()
  local left = {}
  for _, t in ipairs(K.targets) do if not t.down then left[#left + 1] = t end end
  if #left == 0 then return end
  local t = left[love.math.random(1, #left)].body
  local sx, sy = camera:project(t.x, t.y, t.z)
  throw(sx + love.math.random(-8, 8), sy + love.math.random(-8, 8))
end

-- The ball's own sweep (a workaround): at 11 m/s it moves 18 cm a frame,
-- and a hit near a crate's edge can land it deep enough inside that the
-- physics pushes it out through the side and the crate never moves, even
-- with continuousCd (an engine bug, reported). Five rays along this
-- frame's step; if one meets a target, the ball is put just short of it,
-- so the step makes the contact on the face it hit.
local function sweep(ball, dt)
  local b = ball.body
  if ball.speed < 4 then return end
  local s = ball.speed
  local ux, uy, uz = b.vx / s, b.vy / s, b.vz / s
  local reach = s * dt * 1.05 + BALL_R
  -- Two directions across the motion: u × up, then u × that.
  local ax, ay, az = -uz, 0, ux
  local al = sqrt(ax * ax + az * az)
  if al < 1e-3 then return end
  ax, az = ax / al, az / al
  local bx, by, bz = uy * az - uz * ay, uz * ax - ux * az, ux * ay - uy * ax
  local best
  local r = BALL_R * 0.75
  for _, o in ipairs({ { 0, 0 }, { r, 0 }, { -r, 0 }, { 0, r }, { 0, -r } }) do
    local ox, oy, oz = b.x + ax * o[1] + bx * o[2], b.y + ay * o[1] + by * o[2], b.z + az * o[1] + bz * o[2]
    local hit = physics.raycast(ox, oy, oz, ox + ux * reach, oy + uy * reach, oz + uz * reach, { exclude = b })
    if hit and hit.node.data and hit.node.data.t and (not best or hit.distance < best) then best = hit.distance end
  end
  if best and best > BALL_R * 0.5 then
    local d = best - BALL_R * 0.6
    if d < s * dt then
      b.x, b.y, b.z = b.x + ux * d, b.y + uy * d, b.z + uz * d
      b.vx, b.vy, b.vz = ux * s, uy * s, uz * s
    end
  end
end

function love.mousepressed(x, y)
  if round.title or round.ended then return end
  throw(x, y)
end

function love.update(dt)
  -- A debug scenario's jump (debug/scenarios.toml sets these).
  if JUMP_TO then
    jump(JUMP_TO, JUMP_LAYOUT)
    JUMP_TO, JUMP_LAYOUT = nil, nil
  end
  K.clock = K.clock + dt
  K.cooldown = max(K.cooldown - dt, 0)
  if lastAim then lastAim.t = lastAim.t + dt end

  -- Velocities as they were before this frame's step (for contact speeds).
  for _, t in ipairs(K.targets) do
    local b = t.body
    t.vx, t.vy, t.vz = b.vx, b.vy, b.vz
  end
  for i = #K.balls, 1, -1 do
    local ball = K.balls[i]
    local b = ball.body
    ball.age = ball.age + dt
    ball.vx, ball.vy, ball.vz = b.vx, b.vy, b.vz
    ball.speed = sqrt(b.vx * b.vx + b.vy * b.vy + b.vz * b.vz)
    b.data.ball = ball
    sweep(ball, dt)
    if ball.age > 5 or b.y < -2 then
      b:destroy()
      table.remove(K.balls, i)
    end
  end

  checkDown()

  if round.title then
    autopilot()
    if K.phase == "over" or K.stack > 3 then
      K.stack, K.ballsLeft = 0, 0
      nextStack()
      botAt = K.clock + 1.0
    end
  end

  if K.phase == "aim" then
    if standing() == 0 then
      K.phase = "cleared"
      clearAt = K.clock
      carried = K.ballsLeft
      fx.burst("confetti", -0.5, TABLE_Y + 0.25, TABLE_Z + 0.3, { count = 50 })
      fx.burst("confetti", 0.5, TABLE_Y + 0.25, TABLE_Z + 0.3, { count = 50 })
      if not round.title then
        sfx.clear:play()
        haptic("success")
      end
    elseif K.waiting then
      K.waiting = K.waiting + dt
      if K.waiting > SETTLE_MAX or (K.waiting > 1.2 and not anythingMoving()) then
        K.phase = "over"
        if not round.title and not round.ended then
          sfx.over:play()
          haptic("fail")
          round.over("OUT OF BALLS", "stack " .. K.stack)
        end
      end
    end
  elseif K.phase == "cleared" and K.clock - clearAt > CLEAR_BEAT then
    nextStack()
  end

  -- Nothing down two seconds in: show where to aim.
  local lost = not round.title and K.stack == 1 and round.score == 0 and K.phase == "aim" and K.ballsLeft > 0 and K.clock > 2
  if lost and not hint then
    hint = ui.hint{ text = "TAP THE CANS", at = K.targets[2].body }
  elseif hint and not lost then
    hint:dismiss()
    hint = nil
  end
end

-- The HUD: the balls left along the bottom, the aim ring, CLEAR!
function love.draw()
  local g = love.graphics
  if round.title then return end
  local w = g.getWidth()
  local n = K.ballsLeft
  local shown = min(n, 10)
  for i = 0, shown - 1 do
    local x = w / 2 + (i - (shown - 1) / 2) * 30
    g.setColor(0, 0, 0, 0.35)
    g.circle("fill", x + 2, 590, 11)
    g.setColor(0.97, 0.97, 0.98)
    g.circle("fill", x, 588, 11)
    g.setColor(0.9, 0.22, 0.27)
    g.rectangle("fill", x - 10.6, 585.5, 21.2, 5)
  end
  if n > 10 then
    g.setFont(smallFont)
    g.setColor(1, 1, 1)
    g.print("+" .. (n - 10), w / 2 + 5 * 30 + 4, 579)
  end
  if lastAim and lastAim.t < 0.4 then
    local a = (0.4 - lastAim.t) / 0.4
    g.setColor(1, 1, 1, a * 0.8)
    g.setLineWidth(2.5)
    g.circle("line", lastAim.x, lastAim.y, 10 + lastAim.t * 40)
  end
  if K.phase == "cleared" and clearAt then
    local t = K.clock - clearAt
    local pop = 1 + 0.3 * (1 - min(t * 4, 1))
    g.setFont(bigFont)
    g.push()
    g.translate(w / 2, 262)
    g.scale(pop, pop)
    g.setColor(0.11, 0.1, 0.17, 0.7)
    g.printf("CLEAR!", -w / 2 + 3, -22 + 3, w, "center")
    g.setColor(1, 0.88, 0.4)
    g.printf("CLEAR!", -w / 2, -22, w, "center")
    g.pop()
    if carried > 0 then
      g.setFont(smallFont)
      g.setColor(1, 1, 1)
      g.printf("+" .. carried .. " BALL" .. (carried == 1 and "" or "S") .. " CARRIED", 0, 300, w, "center")
    end
  end
end
