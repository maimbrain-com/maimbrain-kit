-- Lantern Sea: a little sailboat on a rolling evening sea. Hold the left
-- or right half of the screen to turn; sail through the floating lanterns.
-- Forty-five seconds.
--
-- The sea is shaders/water.wgsl on a grid: its vertex hook lifts the swell,
-- its surface adds ripples, colour and foam. The boat rides the same swell,
-- worked out here from the same sum of waves and the same clock (`u.time`).

local ROUND = 45
local SIZE, CELLS = 288, 96 -- the sea: meters across, and cells a side
local ARENA = 70 -- lanterns are within this of the middle; past it the boat turns home
local sin, cos, abs, min, max, pi = math.sin, math.cos, math.abs, math.min, math.max, math.pi

local u = { deep = 0x123a5c, shallow = 0x34a6b4, foam = 0xf2f6f0, swell = 1.0, time = 0 }

-- The same swell as the shader's (its `swell`).
local function swell(x, z, t)
  return (sin(x * 0.31 + t * 1.05) * 0.26 + sin(z * 0.47 - t * 1.32 + x * 0.17) * 0.17 + sin((x - z) * 0.83 + t * 1.9) * 0.07) * u.swell
end

world.sky{ atmosphere = "dusk", clouds = { cover = 0.55 }, disc = "sun" }
world.sun{ dir = { 0.35, -0.22, 0.9 }, intensity = 2.6, color = 0xffc890, shadows = true }
world.fog{ color = 0x8a6f86, mode = "exp2", density = 0.011 }
world.post{ bloom = 0.8, bloomThreshold = 1.1, vignette = 0.25, saturation = 1.08 }

-- The sea: a flat grid the shader moves.
local step = SIZE / CELLS
local tris = {}
for i = 0, CELLS - 1 do
  local x0, x1 = -SIZE / 2 + i * step, -SIZE / 2 + (i + 1) * step
  for j = 0, CELLS - 1 do
    local z0, z1 = -SIZE / 2 + j * step, -SIZE / 2 + (j + 1) * step
    local n = #tris
    tris[n + 1], tris[n + 2], tris[n + 3] = x0, 0, z0
    tris[n + 4], tris[n + 5], tris[n + 6] = x0, 0, z1
    tris[n + 7], tris[n + 8], tris[n + 9] = x1, 0, z0
    tris[n + 10], tris[n + 11], tris[n + 12] = x1, 0, z0
    tris[n + 13], tris[n + 14], tris[n + 15] = x0, 0, z1
    tris[n + 16], tris[n + 17], tris[n + 18] = x1, 0, z1
  end
end
Mesh{ geometry = geometry.tris(tris), material = material{ shader = "water", textures = { "ripples" }, uniforms = u, roughness = 0.1 } }

-- The boat: a hull, a bow, a mast and a sail, on a node that rides the swell.
local boat = Group{ name = "boat", tag = "player" }
local hullMat = material{ color = 0x7b3f22, roughness = 0.7 }
Mesh{ parent = boat, geometry = geometry.box(1.1, 0.5, 2.6), material = hullMat, y = 0.15 }
Mesh{ parent = boat, geometry = geometry.cone(0.55, 1.0, 12), material = hullMat, rx = -pi / 2, y = 0.15, z = -1.75 }
Mesh{ parent = boat, geometry = geometry.box(1.0, 0.08, 2.3), material = material{ color = 0xc9a26b, roughness = 0.8 }, y = 0.43 }
Mesh{ parent = boat, geometry = geometry.cylinder(0.06, 2.8, 8), material = material{ color = 0x3a2a1e }, y = 1.8 }
Mesh{ parent = boat, geometry = geometry.box(0.04, 2.0, 1.4), material = material{ color = 0xf4ead6, roughness = 0.9, doubleSided = true }, y = 2.0, z = 0.25 }
round.subject = boat

local heading, speed, clock = 0, 7, 0
local left, turning = ROUND, 0
local bx, bz = 0, 0

-- Lanterns: glowing paper globes that bob, each with its light.
local glow = material{ color = 0xffd9a0, emissive = 0xffa040, emissiveIntensity = 4, unlit = true }
local globe = geometry.sphere(0.38, 16, 12)
local lanterns = {}
local function place(l, near)
  local a, r = love.math.random() * 2 * pi, 12 + love.math.random() * (ARENA - 12)
  l.x, l.z = sin(a) * r, cos(a) * r
  if near then
    -- The first few lie ahead of where the boat starts.
    l.x, l.z = (love.math.random() - 0.5) * 14, -10 - near * 9
  end
end
for k = 1, 14 do
  local l = { node = Mesh{ geometry = globe, material = glow } }
  if k <= 4 then l.light = Light{ parent = l.node, color = 0xffa040, intensity = 5, range = 7 } end
  place(l, k <= 3 and k or nil)
  lanterns[k] = l
end

camera:follow(boat, { behind = 7, above = 3.4, lag = 0.35, lookAhead = 4 })
camera.fov = 60

function love.update(dt)
  clock = clock + dt
  u.time = clock
  if not round.ended then
    -- Steer: hold a side (on the title card, a gentle weave by itself).
    if round.title then
      turning = sin(clock * 0.5) * 0.6
    else
      turning = 0
      if love.mouse.isDown(1) then
        local x = love.mouse.getX()
        turning = x < love.graphics.getWidth() / 2 and 1 or -1
      end
    end
    -- Past the arena, it turns for home.
    local d = math.sqrt(bx * bx + bz * bz)
    if d > ARENA + 6 then
      local home = math.atan2(-bx, -bz)
      local diff = (home - heading + pi) % (2 * pi) - pi
      turning = diff > 0 and 1 or -1
    end
    heading = heading + turning * 1.25 * dt
    bx, bz = bx - sin(heading) * speed * dt, bz - cos(heading) * speed * dt
  end
  -- Ride the swell: height at the middle, pitch from bow to stern, roll from side to side.
  local fx_, fz = -sin(heading), -cos(heading)
  local h = swell(bx, bz, clock)
  local bow, stern = swell(bx + fx_ * 1.3, bz + fz * 1.3, clock), swell(bx - fx_ * 1.3, bz - fz * 1.3, clock)
  local port, star = swell(bx - fz * 0.6, bz + fx_ * 0.6, clock), swell(bx + fz * 0.6, bz - fx_ * 0.6, clock)
  boat.x, boat.y, boat.z = bx, h, bz
  boat.ry = heading
  boat.rx = math.atan2(bow - stern, 2.6) * 0.9
  boat.rz = math.atan2(port - star, 1.2) * 0.9 - turning * 0.12
  for _, l in ipairs(lanterns) do
    l.node.x, l.node.z = l.x, l.z
    l.node.y = swell(l.x, l.z, clock) + 0.35
    if not round.ended then
      local dx, dz = l.x - bx, l.z - bz
      if dx * dx + dz * dz < 2.2 * 2.2 then
        if not round.title then
          round.score = round.score + 1
          fx.popup("+1", l.x, l.node.y + 1, l.z, { color = 0xffd27a })
        end
        fx.burst("sparkles", l.x, l.node.y, l.z, { count = 20, color = 0xffc060 })
        place(l)
      end
    end
  end
  if round.title or round.ended then return end
  left = left - dt
  if left <= 0 then
    left = 0
    round.over("DUSK FALLS", "time")
  end
end

function love.draw()
  if round.title then return end
  local w = love.graphics.getWidth()
  love.graphics.setColor(1, 0.92, 0.75)
  love.graphics.printf(string.format("%d", math.ceil(left)), 0, 104, w, "center")
end
