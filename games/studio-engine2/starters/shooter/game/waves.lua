-- Formations and the wave schedule: which enemies come when, and in what
-- shape. Pure data and maths, driven only by the sim's wave generator, so
-- a daily round gives everyone the same waves whatever they do.
--
-- To add a formation: add a builder to BUILD below, then put its name in
-- `pool` from the moment it should start turning up.

local waves = {}

local W = 360

-- The opening, the same every round: easy jellies right where an idle
-- ship's shots go, so the first kill lands within about two seconds.
waves.OPENING = { { 0.4, "row" }, { 3.6, "vee" }, { 6.8, "columns" } }

-- When each new enemy type joins (seconds into loop `loop`, counting from
-- 0) with a wave of its own and a banner: something new every ~10–15 s
-- until the boss, and spinners in loop 2.
waves.INTROS = {
  { loop = 0, at = 13.0, formation = "swoopTrain", kind = "swooper" },
  { loop = 0, at = 24.0, formation = "darters", kind = "darter" },
  { loop = 0, at = 35.0, formation = "bulb", kind = "bulb" },
  { loop = 1, at = 10.0, formation = "spinner", kind = "spinner" },
}

-- The "two nasty surprises in a row" rule: one of these is never followed
-- by another.
waves.NASTY = { darters = true, bulbPair = true, spinner = true, swoopCross = true }

-- The formations that can come up `phase` seconds into loop `loop`.
function waves.pool(loop, phase)
  local p = { "row", "vee", "columns", "scatter" }
  local function after(l, t) return loop > l or (loop == l and phase >= t) end
  local function add(...) for _, f in { ... } do p[#p + 1] = f end end
  if after(0, 13.0) then add("swoopTrain", "swoopTrain") end
  if after(0, 20.0) then add("swoopCross") end
  if after(0, 24.0) then add("darters") end
  if after(0, 35.0) then add("bulb", "escort") end
  if loop >= 1 then add("bulbPair") end
  if after(1, 10.0) then add("spinner") end
  return p
end

-- A spawn: `delay` seconds after its wave starts, a `kind` at (x, y) on a `path`.
local function spawn(delay, kind, x, y, path)
  return { delay = delay, kind = kind, x = x, y = y, path = path }
end
local function jelly(x, delay, vy, sway)
  return spawn(delay, "jelly", x, -24, { type = "drift", vy = vy, sway = sway })
end

local BUILD = {
  -- A row of jellies drifting down.
  row = function(out, rng)
    local cx = rng(150, 210)
    for i = 0, 4 do out[#out + 1] = jelly(cx + (i - 2) * 52, i * 0.05, 62, 10) end
  end,
  -- A V of jellies, point first.
  vee = function(out, rng)
    local cx = rng(130, 230)
    for i = 0, 6 do
      local k = i - 3
      out[#out + 1] = jelly(cx + k * 34, math.abs(k) * 0.22, 74, 0)
    end
  end,
  -- Two columns of jellies, one each side.
  columns = function(out, rng)
    local inset = rng(60, 90)
    for i = 0, 3 do
      out[#out + 1] = jelly(inset, i * 0.45, 82, 14)
      out[#out + 1] = jelly(W - inset, i * 0.45 + 0.2, 82, 14)
    end
  end,
  -- Jellies scattered across the sky, one after another.
  scatter = function(out, rng)
    for i = 0, 6 do out[#out + 1] = jelly(rng(36, W - 36), i * 0.32, rng(70, 100), rng(0, 24)) end
  end,
  -- Darters that stop, aim at the ship (a line shows where) and dash.
  darters = function(out, rng)
    local n = if rng() < 0.5 then 3 else 2
    for i = 0, n - 1 do
      local x = (i + 0.5) * W / n + rng(-20, 20)
      out[#out + 1] = spawn(i * 0.5, "darter", x, -24, { type = "dive", stopY = rng(90, 150) })
    end
  end,
  -- One big bulb that hovers and fires fans.
  bulb = function(out, rng)
    local x = rng(110, 250)
    out[#out + 1] = spawn(0, "bulb", x, -40, { type = "hover", stopY = rng(120, 170), hold = 6.0 })
  end,
  -- Two bulbs, one each side.
  bulbPair = function(out)
    for i, x in { 95, W - 95 } do
      out[#out + 1] = spawn((i - 1) * 0.6, "bulb", x, -40, { type = "hover", stopY = 140, hold = 5.5 })
    end
  end,
  -- A spinner that hovers and sprays a spiral.
  spinner = function(out, rng)
    local x = rng(120, 240)
    out[#out + 1] = spawn(0, "spinner", x, -36, { type = "hover", stopY = 150, hold = 5.0 })
  end,
  -- Jellies in a row with a bulb behind them.
  escort = function(out, rng)
    local x = rng(130, 230)
    out[#out + 1] = spawn(0.6, "bulb", x, -40, { type = "hover", stopY = 120, hold = 5.0 })
    for i = 0, 3 do out[#out + 1] = jelly(x + (i - 1.5) * 50, 0, 70, 8) end
  end,
}

-- A train of swoopers crossing from one side bobbing, or two trains
-- crossing from both sides at once.
local function swoopers(out, rng, cross)
  local sides = if cross then { -1, 1 } elseif rng() < 0.5 then { -1 } else { 1 }
  for _, side in sides do
    local y0 = rng(70, 150)
    for i = 0, 4 do
      local x = if side < 0 then -30 else W + 30
      out[#out + 1] = spawn(i * 0.32, "swooper", x, y0, { type = "swoop", vx = -side * 120, vy = 26, amp = 46, freq = 2.6 })
    end
  end
end
BUILD.swoopTrain = function(out, rng) swoopers(out, rng, false) end
BUILD.swoopCross = function(out, rng) swoopers(out, rng, true) end

-- The spawns for one wave. `rng(a, b)` is the wave generator (a number
-- from a to b, or 0–1 with none), never the gameplay one.
function waves.build(formation, rng)
  local out = {}
  BUILD[formation](out, rng)
  -- Keep every spawn on screen horizontally (formations near an edge).
  for _, s in out do
    if s.path.type ~= "swoop" then s.x = math.clamp(s.x, 26, W - 26) end
  end
  return out
end

return waves
