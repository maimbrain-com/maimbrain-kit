-- The look: the sky, the light, the land, the road and every shape,
-- material, colour and particle main.lua builds the world from. main.lua
-- holds the rules and where things go; this holds what they look like.
--
-- This is the starter's look, a coast road at sunset (the `neon`
-- identity's outrun): a low sun ahead, a sea on the left past a strip of
-- beach, golden hills and mountains on the right, palms down both sides,
-- red and white kerbs, and a hot pink car, the one thing on the road that
-- colour is saved for (with gold coins, cyan boost pads, amber blinkers and
-- orange roadworks). Make all of it the game's own, in DESIGN.md's look:
-- the sky and light, the land and sea, the road and its roadside, the cars,
-- the hazards, the pickups and the particles.

local look = {}

local LANE_W = 2.4 -- main.lua's: the road is built around its lanes

-- The light: dusk, the sun low ahead and to the right, so the hills glow,
-- the cars' sides catch it and their shadows stretch back toward you.
world.sky { atmosphere = "dusk", clouds = { cover = 0.3 }, disc = "sun" }
world.sun { dir = { -0.8, -0.3, 0.5 }, color = 0xffc08a, intensity = 3.0, shadows = true, shadowDistance = 70 }
world.grade("goldenHour", 0.3)
world.post { exposure = 1.3, bloom = 0.5, bloomThreshold = 0.95, vignette = 0.25, saturation = 1.05, contrast = 1.05 }

-- The land: flat at the road; on the left a beach sloping into a sea basin,
-- on the right golden hills and mountains, the same for every round (a
-- fixed seed). Stamps laid along the first 16 km; past that it's a plain.
local SEA = -1.2 -- the sea's height
local function land()
  local gen = love.math.newRandomGenerator(7)
  local function r(a, b) return a + (b - a) * gen:random() end
  local stamps = {}
  local function row(side, every, near, far, rmin, rmax, hmin, hmax)
    local z = r(0, every)
    while z < 16000 do
      local radius = r(rmin, rmax)
      -- A hill's edge stays `near` meters off the road's middle.
      local x = side * (near + radius + r(0, far))
      stamps[#stamps + 1] = { "hill", x, 60 - z, radius = radius, height = r(hmin, hmax) }
      z = z + every * r(0.6, 1.4)
    end
  end
  -- Dips (hills of negative height) on the left: the beach and the sea bed.
  row(-1, 45, 20, 0, 110, 130, -9, -7)
  row(-1, 160, 120, 40, 260, 320, -40, -30)
  row(1, 38, 14, 18, 18, 34, 4, 12)        -- low banks close to the road
  row(1, 110, 40, 60, 50, 110, 18, 45)     -- rolling hills behind them
  row(1, 420, 180, 200, 180, 320, 90, 170) -- mountains on the skyline
  return Terrain {
    noise = false,
    stamps = stamps,
    detail = 2, reach = 600,
    paint = {
      { color = 0xc79a52 },                                -- dry golden grass
      { slope = 0.05, color = 0x9a7a45, blend = 0.05 },   -- olive scrub as it rises
      { slope = 0.22, color = 0x8a5a44, blend = 0.08 },   -- red rock on the steep
      { above = 80, color = 0xb98a78, blend = 25 },       -- dusty tops in the haze
      { below = -0.3, color = 0xe6c89a, blend = 0.4 },    -- the beach
      { below = SEA + 0.2, color = 0xa98d68, blend = 0.3 }, -- wet sand at the water
    },
  }
end
look.ground = land()

-- The sea: a glossy plane that rides along beside the car (main.lua moves
-- every node in look.ride with it, by its data.z).
look.ride = {
  Mesh { geometry = geometry.plane(1700, 2400), x = -880, y = SEA, data = { z = -600 },
         material = material { color = 0x2a78a8, roughness = 0.3, metalness = 0.05 } },
}

-- The road: one 64 m stretch built once, drawn as a few Meshes laid end to
-- end that leapfrog ahead. Asphalt, white lines, faint bands across it
-- every 4 m, red and white kerbs a block a meter and posts every 2 m (what
-- streams past the corner of your eye is what makes it feel fast).
look.STRETCH = 64
local ASPHALT, SHOULDER, LINE = 0x4a4350, 0x655858, 0xf4efe6
local KERB = { 0xf4efe6, 0xd8322f }
local function road()
  local L = look.STRETCH
  local half = LANE_W * 1.5 + 0.6 -- three lanes and a little shoulder
  local g = geometry.box(half * 2, 0.1, L):paint(ASPHALT)
  g:merge(geometry.box(1.6, 0.08, L):paint(SHOULDER), -(half + 1.4), 0, 0)
  g:merge(geometry.box(1.6, 0.08, L):paint(SHOULDER), half + 1.4, 0, 0)
  for z = -L / 2 + 2, L / 2, 4 do
    g:merge(geometry.box(half * 2, 0.002, 0.35):paint(0x534b5a), 0, 0.051, z)
  end
  for _, x in ipairs({ -LANE_W / 2, LANE_W / 2 }) do
    for z = -L / 2 + 2, L / 2, 8 do
      g:merge(geometry.box(0.14, 0.01, 3.2):paint(LINE), x, 0.055, z)
    end
  end
  for side = -1, 1, 2 do
    g:merge(geometry.box(0.16, 0.01, L):paint(LINE), side * (half - 0.25), 0.055, 0)
    for k = 0, L - 1 do
      g:merge(geometry.box(0.55, 0.14, 1.0):paint(KERB[k % 2 + 1]), side * (half + 0.28), 0.07, -L / 2 + k + 0.5)
    end
    for k = 0, L / 2 - 1 do
      local z = -L / 2 + k * 2 + 1
      g:merge(geometry.box(0.14, 0.9, 0.14):paint(0xf2ece2), side * (half + 2.6), 0.45, z)
      g:merge(geometry.box(0.16, 0.16, 0.06):paint(k % 2 == 0 and 0xffffff or 0xffa62e), side * (half + 2.6), 0.8, z + 0.08)
    end
    g:merge(geometry.box(0.08, 0.12, L):paint(0xd9d2c6), side * (half + 2.6), 0.62, 0)
  end
  return g
end
look.road = road()
look.road_material = material { color = 0xffffff, roughness = 0.8 }

-- The roadside: palms, bushes and rocks, a stretch at a time. Each kind is
-- laid out by main.lua as these say: `every` meters down a side (a row) or
-- `count` scattered, `x` meters out from the road's middle, never under
-- `above` meters of ground (the sea), `scale` and `sides` (-1 left, 1 right).
local function palm()
  -- A trunk leaning out in a curve, thinner as it rises.
  local g = geometry.loft { points = { 0, -0.3, 0, 0.15, 2, 0, 0.6, 4, 0, 1.3, 5.8, 0 }, sides = 5, rings = 6,
                            radius = function(t) return 0.24 - 0.1 * t end }:paint(0x7a5638)
  local tx, top = 1.3, 5.8
  -- Nine fronds, each flat near the trunk and drooping at its end.
  for k = 0, 8 do
    local a = k / 9 * 2 * math.pi + 0.3
    local ux, uz = math.sin(a), math.cos(a)
    local green = k % 2 == 0 and 0x3f8a45 or 0x55a04e
    g:merge(geometry.box(0.6, 0.06, 1.6):paint(green), tx + ux * 0.8, top + 0.1, uz * 0.8, -0.12, a, 0)
    g:merge(geometry.box(0.45, 0.05, 1.6):paint(green), tx + ux * 2.2, top - 0.28, uz * 2.2, 0.6, a, 0)
  end
  return g
end
look.palm = palm()
look.bush = geometry.ico(0.9, 1):scale(1.3, 0.7, 1.3):paint(0x5e6e34)
look.rock = geometry.rock(1.6, 4):paint(0x9a6a52)
look.prop_material = material { color = 0xffffff, roughness = 0.9 }
look.props = {
  { geometry = look.palm, every = 16, x = { 10.5, 12 }, sides = { -1, 1 }, scale = { 0.85, 1.15 } },
  { geometry = look.palm, count = 5, x = { 15, 70 }, sides = { 1 }, scale = { 0.8, 1.3 }, above = 0 },
  { geometry = look.bush, count = 16, x = { 9.5, 45 }, sides = { 1 }, scale = { 0.6, 1.4 } },
  { geometry = look.rock, count = 8, x = { 10, 40 }, sides = { 1 }, scale = { 0.5, 1.5 } },
  { geometry = look.rock, count = 5, x = { 14, 40 }, sides = { -1 }, scale = { 0.6, 1.6 }, above = SEA - 0.6 },
}

-- The player's car: a chunky coupe in hot pink, the one thing on the road
-- in that colour.
local HERO = 0xff2e93
function look.car(body)
  local g = geometry.box(1.7, 0.55, 3.8):paint(body)
  g:merge(geometry.box(1.45, 0.48, 1.9):paint(body), 0, 0.5, 0.25)
  g:merge(geometry.box(1.3, 0.36, 0.06):paint(0x1f1a26), 0, 0.52, -0.73) -- windscreen
  g:merge(geometry.box(1.3, 0.3, 0.06):paint(0x1f1a26), 0, 0.54, 1.21)   -- rear window
  g:merge(geometry.box(0.04, 0.3, 1.5):paint(0x1f1a26), -0.73, 0.54, 0.25) -- side windows
  g:merge(geometry.box(0.04, 0.3, 1.5):paint(0x1f1a26), 0.73, 0.54, 0.25)
  g:merge(geometry.box(1.74, 0.08, 3.84):paint(0x262029), 0, -0.24, 0)     -- a dark sill all round
  for _, w in ipairs({ { -0.82, -1.25 }, { 0.82, -1.25 }, { -0.82, 1.2 }, { 0.82, 1.2 } }) do
    g:merge(geometry.cylinder(0.36, 0.3, 10):paint(0x18151b), w[1], -0.12, w[2], 0, 0, math.pi / 2)
  end
  g:merge(geometry.box(0.4, 0.12, 0.05):paint(0xfff6d8), -0.55, 0.05, -1.92) -- lamps
  g:merge(geometry.box(0.4, 0.12, 0.05):paint(0xfff6d8), 0.55, 0.05, -1.92)
  g:merge(geometry.box(0.42, 0.11, 0.05):paint(0xff3030), -0.55, 0.1, 1.92)
  g:merge(geometry.box(0.42, 0.11, 0.05):paint(0xff3030), 0.55, 0.1, 1.92)
  return g
end
look.hero = look.car(HERO)
look.hero_material = material { color = 0xffffff, roughness = 0.35, metalness = 0.15, emissive = HERO, emissiveIntensity = 0.06 }
look.hero_lift = 0.48 -- the body's middle over the road
look.roof = 0.3       -- how much higher the middle sits with the car on its roof

-- Traffic: the same car shape and a box truck, white so each copy's tint
-- colours it: a sunset coast's faded paints, never the player's pink.
look.traffic_car = look.car(0xffffff)
local truck = geometry.box(2.0, 2.3, 6.2):paint(0xffffff)
truck:merge(geometry.box(1.9, 1.5, 1.9):paint(0xffffff), 0, -0.35, -3.6)
truck:merge(geometry.box(1.7, 0.55, 0.06):paint(0x1f1a26), 0, 0.0, -4.56)
truck:merge(geometry.box(2.02, 0.25, 6.22):paint(0x8a8a8a), 0, -0.6, 0) -- a stripe
for _, w in ipairs({ { -0.95, -3.4 }, { 0.95, -3.4 }, { -0.95, 2.4 }, { 0.95, 2.4 }, { -0.95, 1.3 }, { 0.95, 1.3 } }) do
  truck:merge(geometry.cylinder(0.45, 0.32, 10):paint(0x18151b), w[1], -1.05, w[2], 0, 0, math.pi / 2)
end
look.truck = truck
look.truck_lift = 1.5
look.traffic_material = material { color = 0xffffff, roughness = 0.55 }
look.car_tints = { 0x8cc4e0, 0xf2d47e, 0xb4dcae, 0xe8a07c, 0xc4b2ec, 0xf4efe2, 0x86a4c4, 0xe6cc9c }
look.truck_tints = { 0xe8e2d0, 0x8fb4c8, 0xd9b27a, 0xa7b59a }

-- A blinker: an amber light each side of a truck about to move over, and
-- an arrow over it pointing where.
look.blinker = geometry.box(0.3, 0.3, 0.3):paint(0xffffff)
look.blinker_material = material { color = 0xffb02e, emissive = 0xffb02e, emissiveIntensity = 3, unlit = true }
local arrow = geometry.extrude({ -0.5, -0.35, 0.15, -0.35, 0.15, -0.7, 0.75, 0, 0.15, 0.7, 0.15, 0.35, -0.5, 0.35 }, 0.15)
look.arrow = arrow:paint(0xffffff)

-- Roadworks: a striped barrier across the lane, facing you, and a line of
-- cones on past it (14 m in all).
local cones = geometry.box(1.9, 0.9, 0.25):paint(0xff7a1a)
for k = -2, 2 do
  cones:merge(geometry.box(0.32, 0.22, 0.27):paint(0xffffff), k * 0.38, 0.1, 0)
end
for k = 0, 6 do
  local z = -1.0 - k * 2.0
  cones:merge(geometry.cone(0.32, 0.85, 8):paint(0xff7a1a), 0, -0.02, z)
  cones:merge(geometry.cylinder(0.22, 0.12, 8):paint(0xffffff), 0, 0.05, z)
end
look.cones = cones
look.cones_lift = 0.45
look.cones_material = material { color = 0xffffff, roughness = 0.6, emissive = 0xff7a1a, emissiveIntensity = 0.25 }

-- Oil: a dark glossy puddle.
look.oil = geometry.cylinder(1.0, 0.02, 18):scale(1, 1, 1.6):paint(0x111214)
look.oil_material = material { color = 0xffffff, roughness = 0.08, metalness = 0.3 }

-- Coins: thick gold discs that spin, each a little out of step with the
-- next so a line of them flashes; a boost pad: cyan chevrons pointing down
-- the road.
local coin = geometry.cylinder(0.3, 0.14, 16):paint(0xffc53a)
look.coin = coin:merge(geometry.cylinder(0.18, 0.16, 12):paint(0xffe58a), 0, 0, 0)
look.coin_material = material { color = 0xffffff, emissive = 0xffb000, emissiveIntensity = 0.5, roughness = 0.3, metalness = 0.6 }
look.coin_spin = 4      -- radians a second
look.coin_stagger = 0.9 -- radians between one coin and the next in a line
local pad = geometry.box(2.0, 0.04, 3.4):paint(0x1d6f86)
for k = 0, 2 do
  local z = 1.0 - k * 1.0
  pad:merge(geometry.box(1.0, 0.03, 0.28):paint(0x7ff4ff), -0.38, 0.03, z, 0, 0.7)
  pad:merge(geometry.box(1.0, 0.03, 0.28):paint(0x7ff4ff), 0.38, 0.03, z, 0, -0.7)
end
look.pad = pad
look.pad_material = material { color = 0xffffff, emissive = 0x3fd8ff, emissiveIntensity = 0.6, roughness = 0.4 }

-- Particles: sparks on a crash or a smash, gold on a coin, a flame jet on
-- boost, a puff of smoke from the wreck.
look.sparks = { count = 40, color = { 0xfff2c0, 0xffa030 } }
look.smash = { count = 24, color = { 0xffffff, 0xffd060 } }
look.coin_burst = { count = 8, color = { 0xfff1a0, 0xffc53a } }
look.flame = { count = 2, color = { 0xfff6d0, 0xff6ad5, 0x7ff4ff } }
look.smoke = { count = 1, color = { 0x4a4048, 0x8a7c80 } }

-- Words in the world and on screen.
look.near = 0xffffff     -- "CLOSE! +50"
look.paper = 0xfff0fb    -- the speedo and hint
look.banner = 0x2a0030   -- under a banner

return look
