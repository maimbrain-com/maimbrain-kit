-- Ember Dock: crates on a dock at dusk. Tap a red crate to burn it away;
-- burning a plain one costs 3 seconds. A new crate assembles where each
-- one burned. Forty seconds.
--
-- The burn is shaders/dissolve.wgsl: each crate's material has its own
-- uniforms table, and flux tweens its `t` (0 whole, 1 gone).

local ROUND = 40
local box = geometry.box(0.9, 0.9, 0.9)
local left = ROUND
local crates = {}

world.sky{ atmosphere = "dusk", clouds = { cover = 0.4 } }
world.sun{ dir = { -0.55, -0.42, -0.7 }, intensity = 2.4, shadows = true }
world.water{ preset = "lake", height = -0.7, size = 160 }
world.fog{ color = 0x5a4560, mode = "exp2", density = 0.018 }
world.post{ bloom = 0.7, bloomThreshold = 1.2, vignette = 0.3 }

-- The dock: planks on posts, and two lanterns.
local plank = material{ color = 0x6e4a2e, roughness = 0.92 }
local post = material{ color = 0x3d2a1c, roughness = 0.95 }
for i = -4, 4 do
  Mesh{ geometry = geometry.box(7.6, 0.16, 0.92), material = plank, x = 0, y = -0.08, z = -8 + i * 0.96, ry = (i % 2) * 0.01 }
end
for _, p in ipairs({ { -3.6, -12 }, { 3.6, -12 }, { -3.6, -4.2 }, { 3.6, -4.2 } }) do
  Mesh{ geometry = geometry.cylinder(0.16, 2.4, 10), material = post, x = p[1], y = -0.4, z = p[2] }
end
local glow = material{ color = 0xffd08a, emissive = 0xffa040, emissiveIntensity = 6, unlit = true }
for _, p in ipairs({ { -3.6, -12 }, { 3.6, -12 } }) do
  Mesh{ geometry = geometry.sphere(0.16, 14, 10), material = glow, x = p[1], y = 0.95, z = p[2] }
  Light{ color = 0xffa040, intensity = 7, range = 8, x = p[1], y = 1.1, z = p[2] }
end

-- Nine places for crates, three rows of three.
local slots = {}
for row = 0, 2 do
  for col = -1, 1 do
    slots[#slots + 1] = { x = col * 1.7, z = -6.4 - row * 1.8 }
  end
end

-- A crate assembles in its slot: its burn tweened back from 1 to 0.
local function make(slot)
  local red = love.math.random() < 0.55
  local u = { t = 1, edge = 0xff6a10, width = 0.14 }
  local m = material{ shader = "dissolve", textures = { "noise" }, uniforms = u,
                      color = red and 0xc23b22 or 0xb98a52, roughness = 0.8 }
  local node = Mesh{ geometry = box, material = m, x = slot.x, y = 0.45, z = slot.z,
                     ry = (love.math.random() - 0.5) * 0.35 }
  local c = { node = node, u = u, red = red, slot = slot, burning = true }
  flux.to(u, 0.6, { t = 0 }):ease("quadout"):oncomplete(function() c.burning = false end)
  slot.crate = c
  return c
end

for _, s in ipairs(slots) do make(s) end

local function burn(c)
  if c.burning then return end
  c.burning = true
  if c.red then
    round.score = round.score + 1
    fx.popup("+1", c.node.x, 1.3, c.node.z, { color = 0xffc040 })
  elseif not round.title then
    left = left - 3
    fx.popup("-3s", c.node.x, 1.3, c.node.z, { color = 0xff5050 })
    fx.shake(0.08, 0.15)
  end
  fx.burst("sparks", c.node.x, 0.6, c.node.z, { count = 18, color = 0xff8a30 })
  flux.to(c.u, 0.85, { t = 1 }):ease("quadin"):oncomplete(function()
    c.node:destroy()
    c.slot.crate = nil
    timer.after(0.35, function() make(c.slot) end)
  end)
end

-- Which crate a tap's ray meets first (each a 0.9 m box, near enough upright).
local function pick(sx, sy)
  local ox, oy, oz, dx, dy, dz = camera:ray(sx, sy)
  local best, bestT = nil, math.huge
  for _, s in ipairs(slots) do
    local c = s.crate
    if c and not c.burning then
      local lo, hi = 0, math.huge
      local ok = true
      for _, a in ipairs({ { ox, dx, c.node.x }, { oy, dy, 0.45 }, { oz, dz, c.node.z } }) do
        local o, d, m = a[1], a[2], a[3]
        if math.abs(d) < 1e-6 then
          if math.abs(o - m) > 0.5 then ok = false end
        else
          local t1, t2 = (m - 0.5 - o) / d, (m + 0.5 - o) / d
          if t1 > t2 then t1, t2 = t2, t1 end
          lo, hi = math.max(lo, t1), math.min(hi, t2)
        end
      end
      if ok and lo <= hi and lo < bestT then best, bestT = c, lo end
    end
  end
  return best
end

function love.mousepressed(x, y)
  if round.title or round.ended then return end
  local c = pick(x, y)
  if c then burn(c) end
end

camera.x, camera.y, camera.z = 0, 6.8, -1.2
camera:lookAt(0, 0.1, -8.4)
camera.fov = 50

-- The title card burns red crates by itself.
local auto = 0
function love.update(dt)
  if round.title then
    auto = auto + dt
    if auto > 0.7 then
      auto = 0
      for _, s in ipairs(slots) do
        if s.crate and s.crate.red and not s.crate.burning then burn(s.crate) break end
      end
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
  love.graphics.setColor(1, 0.85, 0.6)
  love.graphics.printf(string.format("%d", math.ceil(left)), 0, 104, w, "center")
end
