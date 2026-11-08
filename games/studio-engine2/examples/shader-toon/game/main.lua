-- Blob Bonk: round cartoon blobs hop about a little island. Tap one to bonk
-- it (a gold one is worth three); more keep hopping in. Thirty seconds.
--
-- Everything is drawn with shaders/toon.wgsl (flat light bands, a warm
-- shadow colour, a rim, an ink line inside the silhouette) plus each
-- material's own outline; a bonk flashes the blob through its uniforms.

local ROUND = 30
local sin, cos, pi, sqrt = math.sin, math.cos, math.pi, math.sqrt
local left = ROUND

world.sky{ atmosphere = "day", clouds = { cover = 0.35 } }
world.sun{ dir = { -0.5, -0.8, -0.35 }, intensity = 3, shadows = true }
world.water{ preset = "lake", height = -0.35, size = 200 }
world.post{ bloom = 0.25, saturation = 1.12 }

-- A toon material: its colour, and the shader's uniforms (its own table, so a flash is its own).
local function toon(color, extra)
  local u = { shadow = 0x6a4a8c, bands = 3, ink = 0x1c1430, rim = 0.55, flash = 0 }
  local m = material{ shader = "toon", uniforms = u, color = color, unlit = true, outline = extra or 2.5, ink = 0x1c1430 }
  return m, u
end

-- The island: grass on a sandy rim, rocks and round trees.
Mesh{ geometry = geometry.cylinder(7.2, 0.7, 40), material = toon(0xe9cf8e, 0), y = -0.3 }
Mesh{ geometry = geometry.cylinder(6.4, 0.72, 40), material = toon(0x7fc95a, 0), y = -0.26 }
local rockMat = toon(0x9a9aa8)
local trunk, leaf = toon(0x8a5a34), toon(0x3f9c4a)
for _, p in ipairs({ { -5.2, -2.4, 1.0 }, { 4.8, -3.6, 0.8 }, { 5.6, 1.8, 0.6 }, { -4.4, 3.4, 0.7 } }) do
  Mesh{ geometry = geometry.rock(p[3] * 1.6, p[1] > 0 and 3 or 5), material = rockMat, x = p[1], y = 0.1, z = p[2] }
end
for _, p in ipairs({ { -3.8, -4.6 }, { 2.6, -5.2 }, { -5.8, 0.6 } }) do
  Mesh{ geometry = geometry.cylinder(0.22, 1.4, 10), material = trunk, x = p[1], y = 0.75, z = p[2] }
  Mesh{ geometry = geometry.sphere(1.05, 18, 14), material = leaf, x = p[1], y = 2.0, z = p[2] }
end

-- Blobs.
local COLORS = { 0xff6f91, 0x5fb8ff, 0xffb347, 0x9b7bff, 0x4fd6a8 }
local body = geometry.sphere(0.6, 24, 18)
local eye = geometry.sphere(0.1, 10, 8)
local eyeMat = material{ color = 0x14101c, unlit = true }
local blobs = {}

local function spawn()
  if #blobs >= 7 then return end
  local gold = love.math.random() < 0.15
  local m, u = toon(gold and 0xffd23a or COLORS[love.math.random(#COLORS)])
  local a, r = love.math.random() * 2 * pi, 0.5 + love.math.random() * 3.6
  local b = { x = sin(a) * r * 1.1, z = cos(a) * r + 0.8, u = u, gold = gold, hop = love.math.random() * 2, phase = love.math.random() * 6, alive = true }
  b.node = Group{ x = b.x, y = 0.6, z = b.z }
  b.mesh = Mesh{ parent = b.node, geometry = body, material = m }
  Mesh{ parent = b.node, geometry = eye, material = eyeMat, x = -0.2, y = 0.3, z = 0.46 }
  Mesh{ parent = b.node, geometry = eye, material = eyeMat, x = 0.2, y = 0.3, z = 0.46 }
  b.node.scale = 0.01
  flux.to(b.node, 0.35, { scale = 1 }):ease("backout")
  blobs[#blobs + 1] = b
end
for _ = 1, 5 do spawn() end

local function bonk(b)
  b.alive = false
  b.u.flash = 1
  flux.to(b.u, 0.25, { flash = 0 })
  local pts = b.gold and 3 or 1
  if not round.title then
    round.score = round.score + pts
    fx.popup("+" .. pts, b.node.x, b.node.y + 1.1, b.node.z, { color = b.gold and 0xffd23a or 0xffffff })
    fx.shake(0.05, 0.1)
  end
  fx.burst("confetti", b.node.x, b.node.y, b.node.z, { count = 24 })
  flux.to(b.node, 0.12, { scale = 1.35 }):after(b.node, 0.18, { scale = 0 }):ease("backin"):oncomplete(function()
    b.node:destroy()
    for i, x in ipairs(blobs) do
      if x == b then table.remove(blobs, i) break end
    end
    timer.after(0.3 + love.math.random() * 0.5, spawn)
  end)
end

local function pick(sx, sy)
  local ox, oy, oz, dx, dy, dz = camera:ray(sx, sy)
  local best, bestT = nil, math.huge
  for _, b in ipairs(blobs) do
    if b.alive then
      local cx, cy, cz = b.node.x - ox, b.node.y - oy, b.node.z - oz
      local t = cx * dx + cy * dy + cz * dz
      local d2 = cx * cx + cy * cy + cz * cz - t * t
      -- A little bigger than the blob: a thumb is wide.
      if t > 0 and d2 < 0.85 * 0.85 and t < bestT then best, bestT = b, t end
    end
  end
  return best
end

function love.mousepressed(x, y)
  if round.title or round.ended then return end
  local b = pick(x, y)
  if b then bonk(b) end
end

camera.x, camera.y, camera.z = 0, 9.5, 10.5
camera:lookAt(0, 0, 0.6)
camera.fov = 50

local clock, auto = 0, 0
function love.update(dt)
  clock = clock + dt
  -- Each blob hops: up and down, squashing as it lands, drifting about.
  for _, b in ipairs(blobs) do
    if b.alive then
      b.hop = b.hop + dt * (b.gold and 2.6 or 1.8)
      local s = sin(b.hop * pi) -- 0 → 1 → 0 each hop
      b.node.y = 0.6 + math.abs(s) * 0.9
      local squash = 1 - (1 - math.abs(s)) ^ 6 * 0.25
      b.mesh.sy, b.mesh.sx, b.mesh.sz = squash, 2 - squash, 2 - squash
      -- Wandering in a loop around where it landed.
      local a = clock * 0.7 + b.phase
      b.node.x, b.node.z = b.x + cos(a) * 1.1, b.z + sin(a * 1.3) * 0.9
    end
  end
  if round.title then
    auto = auto + dt
    if auto > 0.9 and blobs[1] and blobs[1].alive then
      auto = 0
      bonk(blobs[1])
    end
    return
  end
  if round.ended then return end
  left = left - dt
  if left <= 0 then
    left = 0
    round.over("TIME!", "time")
  end
end

function love.draw()
  if round.title then return end
  local w = love.graphics.getWidth()
  love.graphics.setColor(0.12, 0.08, 0.2)
  love.graphics.printf(string.format("%d", math.ceil(left)), 0, 104, w, "center")
end
