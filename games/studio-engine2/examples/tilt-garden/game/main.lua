-- Tilt Garden: tip the phone to roll a marble round a walled garden and
-- pick up gems before the time runs out. The wells swallow it (back to the
-- gate). Tilt steers when the phone has it; a finger held anywhere pulls
-- the marble toward it, and the arrows or the stick push it, so every way
-- of playing does everything.

local W, H = 720, 1100          -- the garden, in units (the view is 360 × 640)
local WALL = 28
local ROUND = 60                -- seconds
local ROLL = 900                -- units/s² at a full tip (about 50°)
local PULL = 700                -- units/s² toward a finger
local DRAG = 1.6                -- rolling friction, per second
local R = 21                    -- the marble's radius

local marble, shine, shadow
local gems = {}
local wells = {}
local level = 0                 -- the phone's pitch when play began (sensors.tilt is absolute)
local gate = { x = W / 2, y = H - 120 }

local LEAF = { 0x6f9a52, 0x7fae5c, 0x5d8a48 }
local PETAL = { 0xf4d35e, 0xee964b, 0xf6f1e9, 0xe86a92 }
local GEM = { 0x4cc9f0, 0xb388eb, 0xff6b6b, 0x80ed99 }

local function rnd(a, b) return a + love.math.random() * (b - a) end

local function free(x, y, room)
  for _, w in ipairs(wells) do
    if (x - w.x) ^ 2 + (y - w.y) ^ 2 < (w.r + room) ^ 2 then return false end
  end
  return (x - gate.x) ^ 2 + (y - gate.y) ^ 2 > (room + 60) ^ 2
end

local function place(g)
  repeat
    g.x, g.y = rnd(WALL + 40, W - WALL - 40), rnd(WALL + 40, H - WALL - 40)
  until free(g.x, g.y, 46)
end

-- The first gems trail along the path from the gate; the rest land anywhere free.
local TRAIL = { { 450, 1010 }, { 540, 1010 }, { 625, 960 }, { 640, 860 }, { 640, 760 } }

local function newGem(at)
  local c = GEM[love.math.random(#GEM)]
  local g = Shape{ shape = "poly", points = { 0, -19, 15, 0, 0, 19, -15, 0 }, color = c, order = -2, tag = "gem" }
  Shape{ parent = g, shape = "poly", points = { 0, -12, 7, -1, 0, 4, -5, -2 }, color = 0xffffff, alpha = 0.6 }
  Shape{ parent = g, shape = "circle", y = 22, radius = 12, sy = 0.35, color = 0x2f4a2c, alpha = 0.3, order = -1 }
  g.data.bob = rnd(0, 6)
  if at then g.x, g.y = at[1], at[2] else place(g) end
  gems[#gems + 1] = g
end

local function wellsAt()
  for i, p in ipairs({ { 200, 300 }, { 520, 420 }, { 360, 640 }, { 170, 820 }, { 540, 860 } }) do
    wells[#wells + 1] = { x = p[1], y = p[2], r = 26 + (i % 2) * 6 }
  end
end

-- The garden: lawn, a gravel path, beds of flowers, stone walls, the wells.
local function garden()
  Shape{ x = W / 2, y = H / 2, w = W + 400, h = H + 400, color = 0x3e5a3a, order = -20 }
  Shape{ x = W / 2, y = H / 2, w = W, h = H, color = 0x86b26a, order = -19 }
  -- mown stripes
  for i = 0, 9 do
    Shape{ x = W / 2, y = WALL + 50 + i * 110, w = W - 2 * WALL, h = 55, color = 0x8fbb72, order = -18 }
  end
  -- a gravel path in a loop
  Shape{ x = W / 2, y = H / 2, w = W - 200, h = H - 260, corner = 120, color = 0xd9c7a3, order = -17 }
  Shape{ x = W / 2, y = H / 2, w = W - 290, h = H - 350, corner = 80, color = 0x86b26a, order = -16 }
  wellsAt()
  -- flower beds
  for _ = 1, 70 do
    local x, y
    repeat x, y = rnd(WALL + 10, W - WALL - 10), rnd(WALL + 10, H - WALL - 10) until free(x, y, 16)
    Shape{ shape = "circle", x = x, y = y, radius = rnd(7, 12), color = LEAF[love.math.random(#LEAF)], order = -15 }
    Shape{ shape = "circle", x = x + rnd(-3, 3), y = y + rnd(-3, 3), radius = rnd(2.5, 4), color = PETAL[love.math.random(#PETAL)], order = -14 }
  end
  -- the wells: a stone ring round the dark
  for _, w in ipairs(wells) do
    Shape{ shape = "circle", x = w.x, y = w.y + 4, radius = w.r + 10, color = 0x3b4a3a, alpha = 0.35, order = -13 }
    Shape{ shape = "circle", x = w.x, y = w.y, radius = w.r + 8, color = 0xa49e94, order = -12 }
    Shape{ shape = "circle", x = w.x, y = w.y, radius = w.r, color = 0x1d2430, order = -11 }
    Shape{ shape = "circle", x = w.x - w.r * 0.25, y = w.y - w.r * 0.3, radius = w.r * 0.45, color = 0x2c3a4d, order = -10 }
  end
  -- the walls
  local stone = 0x9c8f7c
  Shape{ x = W / 2, y = WALL / 2, w = W, h = WALL, color = stone, order = -9 }
  Shape{ x = W / 2, y = H - WALL / 2, w = W, h = WALL, color = stone, order = -9 }
  Shape{ x = WALL / 2, y = H / 2, w = WALL, h = H, color = stone, order = -9 }
  Shape{ x = W - WALL / 2, y = H / 2, w = WALL, h = H, color = stone, order = -9 }
  for i = 0, 12 do
    Shape{ x = 40 + i * 54, y = WALL / 2, w = 4, h = WALL, color = 0x7d7262, order = -8 }
    Shape{ x = 40 + i * 54, y = H - WALL / 2, w = 4, h = WALL, color = 0x7d7262, order = -8 }
  end
  -- the gate
  Shape{ shape = "circle", x = gate.x, y = gate.y, radius = 34, color = 0xe8dcc2, order = -12 }
  Shape{ shape = "circle", x = gate.x, y = gate.y, radius = 26, color = 0xd9c7a3, order = -11 }
end

local function home()
  marble.x, marble.y = gate.x, gate.y
  marble.data.vx, marble.data.vy = 0, 0
end

function love.load()
  garden()
  shadow = Shape{ shape = "circle", radius = R, color = 0x203020, alpha = 0.35, order = 1 }
  marble = Shape{ shape = "circle", radius = R, color = 0x2c5d94, order = 2, name = "marble" }
  Shape{ parent = marble, shape = "circle", radius = R, line = 3, color = 0x142236 }
  Shape{ parent = marble, shape = "circle", x = 4, y = 5, radius = 13, color = 0x2b5480, alpha = 0.6 }
  shine = Shape{ shape = "circle", radius = 6, color = 0xffffff, alpha = 0.85, order = 3 }
  marble.data.vx, marble.data.vy = 0, 0
  home()
  shine.x, shine.y = marble.x - 7, marble.y - 8
  round.subject = marble
  for _, at in ipairs(TRAIL) do newGem(at) end
  for _ = 1, 4 do newGem() end
  camera:follow(marble, { lag = 0.18, bounds = { 0, 0, W, H } })
  ui.hud{ timer = ROUND }
  local _, pitch = sensors.tilt()
  level = pitch or 0
end

-- Where the marble is pushed this frame, units/s².
local function push()
  local ax, ay = 0, 0
  local roll, pitch = sensors.tilt()
  if roll then
    ax = math.sin(math.max(-0.9, math.min(0.9, roll))) * ROLL
    ay = -math.sin(math.max(-0.9, math.min(0.9, pitch - level))) * ROLL
  end
  if love.mouse.isDown(1) then
    local wx, wy = camera:worldCoords(love.mouse.getPosition())
    local dx, dy = wx - marble.x, wy - marble.y
    local d = math.sqrt(dx * dx + dy * dy)
    if d > 4 then
      local k = math.min(1, d / 120)
      ax, ay = ax + dx / d * PULL * k, ay + dy / d * PULL * k
    end
  end
  return ax + input.ax * PULL, ay + input.ay * PULL
end

function love.update(dt)
  if round.title then return end
  local ax, ay = push()
  local d = marble.data
  d.vx = (d.vx + ax * dt) * math.exp(-DRAG * dt)
  d.vy = (d.vy + ay * dt) * math.exp(-DRAG * dt)
  local x, y = marble.x + d.vx * dt, marble.y + d.vy * dt
  -- the walls: a bounce that loses some speed
  if x < WALL + R then x, d.vx = WALL + R, math.abs(d.vx) * 0.5 end
  if x > W - WALL - R then x, d.vx = W - WALL - R, -math.abs(d.vx) * 0.5 end
  if y < WALL + R then y, d.vy = WALL + R, math.abs(d.vy) * 0.5 end
  if y > H - WALL - R then y, d.vy = H - WALL - R, -math.abs(d.vy) * 0.5 end
  marble.x, marble.y = x, y
  marble.rotation = marble.rotation + d.vx * dt / R
  shadow.x, shadow.y = x + 4, y + 6
  shine.x, shine.y = x - 7, y - 8

  for _, w in ipairs(wells) do
    if (x - w.x) ^ 2 + (y - w.y) ^ 2 < (w.r - 4) ^ 2 then
      fx.burst("ring", w.x, w.y, { color = 0x4cc9f0 })
      haptic("fail")
      round.score = math.max(0, round.score - 1)
      debug.mark("respawn")
      home()
      fx.flash(0x1d2430, 0.2)
    end
  end

  for i = #gems, 1, -1 do
    local g = gems[i]
    g.data.bob = g.data.bob + dt * 3
    g.scale = 1 + math.sin(g.data.bob) * 0.08
    if (g.x - x) ^ 2 + (g.y - y) ^ 2 < (R + 16) ^ 2 then
      round.score = round.score + 1
      fx.burst("stars", g.x, g.y, { color = g.color })
      fx.popup("+1", g.x, g.y - 18)
      haptic("tap")
      g:destroy()
      table.remove(gems, i)
      newGem()
    end
  end

  if round.time >= ROUND then
    round.over("TIME'S UP", "time")
  end
end
