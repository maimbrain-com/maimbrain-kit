-- The racer starter: the racer kit's Swerve (an endless lane racer for one
-- thumb) in Lua. Tap the left or right half of the screen to change lanes
-- (a swipe does the same; so do the arrows, A/D, the d-pad and the stick).
-- Dodge traffic, trucks that move over, roadworks and oil; grab coins and
-- boost pads (boosting smashes through everything). It speeds up every
-- 12 s; something new joins the road every ~15 s; a hit ends the round.
--
-- Everything it looks like is in game/look.lua: the starter's coast road
-- at sunset, a stand-in. Make that the game's own, in DESIGN.md's look; this
-- file holds the rules, the camera and the feel.
--
-- The road runs from the start down −z, straight and flat: something `d`
-- meters along it is at z = −d, lane l (0, 1, 2) at x = (l − 1) × LANE_W.
-- The car really drives down it (the land is a Terrain, which stays put).
-- The rules are the kit's, step for step; its bot report is the target
-- (debug/bot-report.tsv, from game/bot.lua): a first-timer's round about
-- 40 s, a good player's about two minutes.

local look = require("look")

local sin, cos, floor, max, min, abs, exp, pi = math.sin, math.cos, math.floor, math.max, math.min, math.abs, math.exp, math.pi

-- Tuning knobs (the kit's) ------------------------------------------------------
local START_SPEED = 16.0     -- m/s at the start (16 ≈ 58 km/h). Raise it and the first seconds get hard
local SPEED_STEP = 2.2       -- m/s added at each speed-up
local SPEED_EVERY = 12.0     -- seconds between speed-ups
local MAX_SPEED = 44.0       -- top cruising speed (boost goes past it)
local TRAFFIC_SPEED = 7.0    -- traffic's own speed: you catch up with it
local GAP_START = 1.2        -- seconds of clear road between rows at the start: the main difficulty dial
local GAP_MIN = 0.66         -- ... and the least it shrinks to
local GAP_SHRINK = 0.011     -- ... and how much it shrinks a second
local LANE_STIFFNESS = 170.0 -- the lane change spring: higher snaps faster
local LANE_DAMPING = 0.62    -- under 1 overshoots (springy), 1 slides
local BOOST_TIME = 2.6       -- seconds of boost from a pad
local BOOST_MULT = 1.45      -- how much faster a boost goes
local NEAR_TTC = 0.55        -- a dodge this late (seconds before impact) is a near miss
local COIN_POINTS, NEAR_POINTS, SMASH_POINTS = 10, 50, 100 -- and a point a meter
-- When each new thing first shows up (seconds), and when nasty rows may come back to back.
local UNLOCK_WALLS, UNLOCK_BOOST, UNLOCK_TRUCKS = 9.0, 11.0, 17.0
local UNLOCK_CHANGERS, UNLOCK_CONES, UNLOCK_OIL = 31.0, 46.0, 62.0
local SAFE_START = 3.5       -- nothing reaches you before this
local RELENTLESS = 90.0
-- --------------------------------------------------------------------------------

local LANES, LANE_W = 3, 2.4
local CAR_W, CAR_L = 0.8, 1.9  -- the player's car: half width and half length (m)
local AHEAD = 150.0            -- rows are laid out this far ahead (the fog hides the rest)
local BLINK_TTC, BLINK_LEAD, SHIFT_TIME = 2.1, 0.85, 0.6 -- a truck blinks, waits, then slides over
local OIL_LOCK = 0.45          -- seconds of no steering after oil
local SLOWMO_TIME, SLOWMO, CRASH_TIME = 0.9, 0.25, 1.8   -- the crash plays out before the card
local CHAIN_WINDOW = 3.0       -- near misses this close together chain (×2, ×3 …)
local REACT, LANE_T = 0.33, 0.3 -- what the generator allows a person to react and change a lane
local SWIPE = 34               -- units of sideways drag that make a swipe
local SPEEDO = 7.2             -- km/h on the speedo per m/s (arcade: twice the real speed)
-- The chase camera: meters behind and above the car at the start and at top
-- speed (it comes in lower and closer as the field of view widens, so the
-- road stretches and streams past faster), and that field of view (radians).
local CAM_BACK, CAM_BACK_FAST, CAM_HIGH, CAM_HIGH_FAST = 8.8, 7.4, 4.0, 3.3
local FOV_SLOW, FOV_FAST, FOV_BOOST = 0.98, 1.2, 0.2

-- Half width and half length of each kind (m); traffic moves.
local KINDS = {
  car = { 0.85, 2.0, true }, truck = { 1.0, 4.2, true },
  cones = { 0.95, 7.0, false }, oil = { 0.9, 1.6, false },
}
local NEWS = { truck = "TRUCKS!", changer = "TRUCKS MOVE OVER!", cones = "ROADWORKS!", oil = "OIL!" }

local function lane_x(l) return (l - 1) * LANE_W end
local function smooth(x)
  local k = max(0, min(1, x))
  return k * k * (3 - 2 * k)
end
local function rand(a, b) return a + (b - a) * love.math.random() end
local function lane_bit(l) return 2 ^ l end
local function has(bits, l) return floor(bits / 2 ^ l) % 2 == 1 end

-- The race (shared, so game/bot.lua can read the road as a player sees it) ---

local shared = require("shared")
local race

local function new_race()
  race = {
    t = 0, dist = 0, speed = START_SPEED, tier = 0,
    lane = 1, x = 0, vx = 0,
    boost = 0, boost_k = 0, oil = 0,
    coins = 0, coin_run = 0, near_chain = 0, chain_t = 0, bonus = 0,
    phase = "run", crash_t = 0, crash_w = 0, over_t = 0,
    obstacles = {}, coin_list = {}, pads = {}, cues = {},
    steers = 0, presses = 0, idle = 0, touch_from = 1,
    -- how fast it feels (feel(), never the rules)
    feel = 0.35, kick = 0, surge = 0, clock = 0, last_speed = START_SPEED, shake = 0,
    -- the generator
    clear_at = START_SPEED * (SAFE_START - GAP_START),
    prev_free = { true, true, true }, last_static = { -1000, -1000, -1000 },
    nasty = false, announced = 0,
  }
  shared.race = race
  -- The warm-up: a carpet of coins in every lane, so the first tap pays.
  for k = 0, 6 do
    for l = 0, LANES - 1 do
      race.coin_list[#race.coin_list + 1] = { d = 9 + k * 4.5, lane = l }
    end
  end
end

local function cue(kind, a, b, c)
  race.cues[#race.cues + 1] = { kind, a, b, c }
end

local function kick(k) race.kick = min(1.2, race.kick + k) end

local function cruise_at(t) return min(START_SPEED + SPEED_STEP * floor(t / SPEED_EVERY), MAX_SPEED) end

local function crashed() return race.phase ~= "run" end

-- The speed now, with boost (and the wreck sliding to a stop).
local function speed_now()
  if race.phase == "run" then return race.speed * (1 + (BOOST_MULT - 1) * race.boost_k) end
  if race.phase == "crash" then return race.crash_speed * exp(-3 * race.crash_w) end
  return 0
end

-- Lane-change progress of a truck moving over, 0…1.
local function shift(o)
  if not o.blink then return 0 end
  return max(0, min(1, (o.blink - BLINK_LEAD) / SHIFT_TIME))
end
local function signalling(o) return o.change ~= nil and shift(o) < 1 end
local function covers(o, l) return abs(o.x - lane_x(l)) < KINDS[o.kind][1] + CAR_W - 0.2 end

-- Steering ------------------------------------------------------------------------

local function lane_cue(dir)
  race.steers = race.steers + 1
  cue("lane", dir, race.vx * dir < -2)
end

local function steer(dir)
  race.idle = 0
  if crashed() or race.oil > 0 then return end
  local want = max(0, min(LANES - 1, race.lane + dir))
  if want == race.lane then
    race.vx = race.vx + dir * 2.5
    cue("bump", dir)
    return
  end
  race.lane = want
  lane_cue(dir)
end

-- A press on the left (−1) or right (+1) half.
local function press(dir)
  race.presses = race.presses + 1
  race.touch_from = race.lane
  steer(dir)
end

-- A swipe in the same touch aims from the lane the touch started in.
local function swipe(dir)
  if crashed() or race.oil > 0 then return end
  local want = max(0, min(LANES - 1, race.touch_from + dir))
  if want ~= race.lane then
    local d = want > race.lane and 1 or -1
    race.lane = want
    lane_cue(d)
  end
end

-- The road ahead ------------------------------------------------------------------

-- How far you drive from touching something to being level with its middle.
local function reach(kind, pm)
  local v = KINDS[kind][3] and TRAFFIC_SPEED or 0
  return (KINDS[kind][2] + CAR_L) * pm / max(1, pm - v)
end

-- A row's obstacles for pattern `id` ({ kind, lane, to }), or nil if it isn't fair here.
local function plan_row(id, front, gap)
  local function r() return love.math.random(0, 2) end
  local function other(a) return (a + love.math.random(1, 2)) % 3 end
  local items, blocked, nasty = {}, 0, false
  if id == 0 then
    local a = r(); items = { { "car", a } }; blocked = lane_bit(a)
  elseif id == 1 then
    local a = r(); local b = other(a)
    items = { { "car", a }, { "car", b } }; blocked = lane_bit(a) + lane_bit(b); nasty = true
  elseif id == 2 then
    local a = r(); items = { { "truck", a } }; blocked = lane_bit(a)
  elseif id == 3 then
    local a = r(); local b = other(a)
    items = { { "truck", a }, { "car", b } }; blocked = lane_bit(a) + lane_bit(b); nasty = true
  elseif id == 4 then
    -- A truck that moves over (blinker first) into a free lane.
    local a = r()
    local b = a == 1 and (love.math.random() < 0.5 and 0 or 2) or 1
    items = { { "truck", a, b } }; blocked = lane_bit(a) + lane_bit(b); nasty = true
  elseif id == 5 then
    local a = r(); items = { { "cones", a } }; blocked = lane_bit(a); nasty = true
  elseif id == 6 then
    local a = r(); items = { { "oil", a } }; blocked = lane_bit(a)
    if love.math.random() < 0.5 then
      local b = other(a); items[2] = { "car", b }; blocked = blocked + lane_bit(b)
    end
  end
  -- Moving things can't start behind something still in their lane.
  local p = max(1, race.speed)
  for _, it in ipairs(items) do
    if KINDS[it[1]][3] then
      local clear = AHEAD * TRAFFIC_SPEED / p + 12
      if front - race.last_static[it[2] + 1] < clear then return nil end
      if it[3] and front - race.last_static[it[3] + 1] < clear + 15 then return nil end
    end
  end
  -- Fair: from every lane free in the last row, a free lane here is in reach in time.
  local any = false
  for l = 0, LANES - 1 do any = any or not has(blocked, l) end
  if not any then return nil end
  for f = 0, LANES - 1 do
    if race.prev_free[f + 1] then
      local ok = false
      for g = 0, LANES - 1 do
        if not has(blocked, g) and abs(f - g) * LANE_T + REACT <= gap then ok = true end
      end
      if not ok then return nil end
    end
  end
  return { items = items, blocked = blocked, nasty = nasty }
end

-- Places a planned row: `front` is where you first touch it, `t_meet` when.
local function place(plan, front, t_meet, pm, p)
  local clear = front + 4
  for _, it in ipairs(plan.items) do
    local kind, lane, to = it[1], it[2], it[3]
    local rch = reach(kind, pm)
    local centre = front + rch + rand(0, 0.8)
    clear = max(clear, centre + rch)
    local v = KINDS[kind][3] and TRAFFIC_SPEED or 0
    local o = {
      kind = kind, d = centre - v * (centre - race.dist) / p, x = lane_x(lane), lane = lane, from = lane,
      v = v, tint = love.math.random(1, 8), change = to,
    }
    race.obstacles[#race.obstacles + 1] = o
    if not KINDS[kind][3] then race.last_static[lane + 1] = centre + KINDS[kind][2] end
  end
  local free = {}
  for l = 0, LANES - 1 do if not has(plan.blocked, l) then free[#free + 1] = l end end
  -- Coins in a free lane: the reward for picking the right one.
  if #free > 0 and (#plan.items == 0 or love.math.random() < 0.45) then
    local lane = free[love.math.random(1, #free)]
    for k = 0, 4 do race.coin_list[#race.coin_list + 1] = { d = front + 4 + (k - 2) * 2.6, lane = lane } end
  end
  -- A boost pad just before the row now and then.
  if t_meet >= UNLOCK_BOOST and #free > 0 and love.math.random() < 0.13 then
    race.pads[#race.pads + 1] = { d = front - 5, lane = free[love.math.random(1, #free)] }
  end
  for l = 0, LANES - 1 do race.prev_free[l + 1] = not has(plan.blocked, l) end
  race.nasty = plan.nasty
  race.clear_at = clear
end

local function row(front, t_meet, gap, pm, p)
  -- Nasty patterns never come twice in a row until the late game.
  local calm = race.nasty and t_meet < RELENTLESS
  local menu = { { 3.0, 0 }, { 0.5, 7 } }
  local function add(w, id) menu[#menu + 1] = { w, id } end
  if t_meet >= UNLOCK_WALLS and not calm then add(2.2, 1) end
  if t_meet >= UNLOCK_TRUCKS then
    add(2.0, 2)
    if not calm then add(1.0, 3) end
  end
  if t_meet >= UNLOCK_CHANGERS and not calm then add(1.6, 4) end
  if t_meet >= UNLOCK_CONES and not calm then add(1.3, 5) end
  if t_meet >= UNLOCK_OIL then add(1.1, 6) end
  local total = 0
  for _, m in ipairs(menu) do total = total + m[1] end
  for _ = 1, 10 do
    local pick, id = rand(0, total), 0
    for _, m in ipairs(menu) do
      if pick < m[1] then id = m[2]; break end
      pick = pick - m[1]
    end
    local plan = plan_row(id, front, gap)
    if plan then return place(plan, front, t_meet, pm, p) end
  end
  local lane = love.math.random(0, 2)
  place({ items = { { "car", lane } }, blocked = lane_bit(lane), nasty = false }, front, t_meet, pm, p)
end

-- Lays rows out to AHEAD meters, in time: each starts `gap` seconds after
-- you're clear of the last, placed by where you'll meet it.
local function spawn()
  local p = max(1, race.speed)
  while race.clear_at < race.dist + AHEAD do
    local t_clear = race.t + (race.clear_at - race.dist) / p
    local gap = max(GAP_MIN, GAP_START - GAP_SHRINK * t_clear)
    local pm = cruise_at(t_clear)
    row(race.clear_at + gap * pm, t_clear + gap, gap, pm, p)
  end
end

-- The step ------------------------------------------------------------------------

local function move_obstacles(dt)
  local p = speed_now()
  for _, o in ipairs(race.obstacles) do
    local k = o.knock
    if k then
      k.t = k.t + dt
      k.vy = k.vy - 22 * dt
      local drag = exp(-1.6 * dt)
      k.vx, k.vz = k.vx * drag, k.vz * drag
      k.x, k.y, k.z = k.x + k.vx * dt, k.y + k.vy * dt, k.z + k.vz * dt
      if k.y < 0 then k.y = 0; k.vy = -k.vy * 0.35; k.spin = k.spin * 0.6 end
      k.angle = k.angle + k.spin * dt
      -- A knocked car rolls on at its own (braking) speed, so the wreck
      -- doesn't drive through it.
      o.d = o.d + o.v * dt
    else
      o.d = o.d + o.v * dt
      if o.change then
        local gap = o.d - race.dist - KINDS[o.kind][2] - CAR_L
        local ttc = gap / max(1, p - o.v)
        if o.blink then
          o.blink = o.blink + dt
        elseif ttc < BLINK_TTC and ttc > 0 and p > o.v then
          o.blink = 0
          cue("blinker")
        end
        local s = smooth(shift(o))
        o.x = lane_x(o.from) + (lane_x(o.change) - lane_x(o.from)) * s
        if s >= 1 then o.lane = o.change end
      end
    end
  end
end

local function knock(o, vx, vy, vz, spin)
  o.knock = { t = 0, vx = vx, vy = vy, vz = vz, x = 0, y = 0, z = 0, spin = spin, angle = 0 }
end

local function collide(p)
  local hit
  for _, o in ipairs(race.obstacles) do
    if not o.knock and not o.passed then
      local ow, ol = KINDS[o.kind][1], KINDS[o.kind][2]
      local rel = o.d - race.dist
      local lateral = abs(o.x - race.x) < ow + CAR_W - 0.22
      local ttc = (rel - ol - CAR_L) / max(1, p - o.v)
      -- Still in the way just before impact: dodging now is a near miss.
      if lateral and ttc > 0 and ttc < NEAR_TTC and o.kind ~= "oil" then o.near = true end
      if lateral and abs(rel) < ol + CAR_L - 0.3 then
        if o.kind == "oil" then
          o.passed = true
          if race.boost <= 0 and race.oil <= 0 then
            local dir = love.math.random() < 0.5 and -1 or 1
            if race.lane + dir < 0 or race.lane + dir > LANES - 1 then dir = -dir end
            race.lane = race.lane + dir
            race.oil = OIL_LOCK
            race.vx = race.vx + dir * 3
            cue("oil", dir)
          end
        elseif race.boost > 0 then
          local side = o.x > race.x and 1 or (o.x < race.x and -1 or (love.math.random() < 0.5 and 1 or -1))
          knock(o, side * rand(4, 7), rand(5, 8), -rand(4, 8), side * rand(5, 9))
          race.bonus = race.bonus + SMASH_POINTS
          race.shake = min(1, race.shake + 0.35)
          cue("smash", o.x, -o.d)
        else
          hit = o
          break
        end
      elseif rel + ol < -CAR_L then
        -- Passed it.
        o.passed = true
        if o.near then
          race.near_chain = race.near_chain + 1
          race.chain_t = CHAIN_WINDOW
          local points = NEAR_POINTS * min(5, race.near_chain)
          race.bonus = race.bonus + points
          race.shake = min(1, race.shake + 0.18)
          kick(0.25)
          cue("near", race.near_chain, points, o.x)
        end
      end
    end
  end
  if hit then
    -- The wreck slides away from what it hit, toward clear road if it can.
    local pref = race.x > hit.x + 0.05 and 1 or (race.x < hit.x - 0.05 and -1 or (love.math.random() < 0.5 and 1 or -1))
    local function busy(side)
      for _, q in ipairs(race.obstacles) do
        if not q.knock and KINDS[q.kind][3] and abs(q.d - race.dist) < 10 and (q.x - race.x) * side > 0.6 then return true end
      end
      return false
    end
    race.crash_dir = (busy(pref) and not busy(-pref)) and -pref or pref
    race.crash_x = race.x
    race.crash_speed = p
    -- Shoved ahead at about the speed you hit it, away from the wreck's side.
    knock(hit, -race.crash_dir * 3.5, 2.5, -(p - hit.v) * 0.9, -race.crash_dir * 2)
    race.phase, race.crash_t, race.crash_w = "crash", 0, 0
    race.cause = hit.kind
    race.boost = 0
    race.shake = 1
    cue("crash", (race.x + hit.x) / 2, -(race.dist + max(0, hit.d - race.dist - KINDS[hit.kind][2])))
  end
end

local function pickups()
  local got = 0
  for _, c in ipairs(race.coin_list) do
    if not c.taken and abs(c.d - race.dist) < 1.7 and abs(lane_x(c.lane) - race.x) < 1.15 then
      c.taken = true
      got = got + 1
    end
  end
  for _ = 1, got do
    race.coins = race.coins + 1
    race.coin_run = race.coin_run + 1
    cue("coin", race.coin_run)
  end
  if got == 0 then
    for _, c in ipairs(race.coin_list) do
      if not c.taken and c.d < race.dist - 2 and c.d > race.dist - 3 then race.coin_run = 0 end
    end
  end
  for _, pd in ipairs(race.pads) do
    if not pd.used and abs(pd.d - race.dist) < 2 and abs(lane_x(pd.lane) - race.x) < 1.2 then
      pd.used = true
      race.boost = BOOST_TIME
      race.shake = min(1, race.shake + 0.25)
      kick(0.9)
      cue("boost")
    end
  end
end

local function forget()
  local behind = race.dist - 16
  local function keep(list, f)
    local out = {}
    for _, x in ipairs(list) do if f(x) then out[#out + 1] = x end end
    return out
  end
  race.obstacles = keep(race.obstacles, function(o)
    return o.d + KINDS[o.kind][2] > behind or (o.knock and o.knock.t < 2.5)
  end)
  race.coin_list = keep(race.coin_list, function(c) return c.d > behind end)
  race.pads = keep(race.pads, function(p) return p.d > behind end)
end

local UNLOCKS = { { UNLOCK_WALLS }, { UNLOCK_BOOST }, { UNLOCK_TRUCKS, "truck" }, { UNLOCK_CHANGERS, "changer" },
                  { UNLOCK_CONES, "cones" }, { UNLOCK_OIL, "oil" } }

local function run(dt)
  race.t = race.t + dt
  race.idle = race.idle + dt
  -- Speed-ups, announced; the cruise eases up to the new speed.
  local tier = floor(race.t / SPEED_EVERY)
  if tier > race.tier and cruise_at(race.t) > race.speed + 0.1 then
    race.tier = tier
    kick(0.6)
    cue("speedup", tier)
  end
  local target = cruise_at(race.t)
  race.speed = max(min(race.speed + 3 * dt, target), min(race.speed, target))
  while race.announced < #UNLOCKS and race.t >= UNLOCKS[race.announced + 1][1] do
    race.announced = race.announced + 1
    if UNLOCKS[race.announced][2] then cue("new", UNLOCKS[race.announced][2]) end
  end
  race.boost = max(0, race.boost - dt)
  local want = race.boost > 0 and 1 or 0
  race.boost_k = race.boost_k + (want - race.boost_k) * (1 - exp(-dt * (want > race.boost_k and 9 or 3)))
  race.oil = max(0, race.oil - dt)
  race.chain_t = max(0, race.chain_t - dt)
  if race.chain_t <= 0 then race.near_chain = 0 end
  local p = speed_now()
  race.dist = race.dist + p * dt
  -- The lane spring, in small steps so a long frame stays stable.
  local n = max(1, math.ceil(dt * 240))
  local h = dt / n
  local damp = 2 * LANE_DAMPING * math.sqrt(LANE_STIFFNESS)
  for _ = 1, n do
    local a = LANE_STIFFNESS * (lane_x(race.lane) - race.x) - damp * race.vx
    race.vx = race.vx + a * h
    race.x = race.x + race.vx * h
  end
  move_obstacles(dt)
  collide(p)
  pickups()
  spawn()
  forget()
end

-- How fast it feels: 0.35 at the start speed, 1 at top cruise, ~1.7 on
-- boost, plus kicks. The camera, FOV, speed lines and buzz read it; the rules never do.
local function feel(dt)
  local s = speed_now()
  race.clock = race.clock + dt
  race.kick = race.kick * exp(-2.2 * dt)
  local frac = max(0, min(1, (s - START_SPEED) / (MAX_SPEED - START_SPEED)))
  local base = s <= START_SPEED and 0.35 * max(0, min(1, s / START_SPEED)) or 0.35 + 0.65 * frac
  if race.phase == "run" then race.feel = base + 0.55 * race.boost_k + race.kick else race.feel = base * 0.5 end
  local accel = (s - race.last_speed) / dt
  race.last_speed = s
  local want = race.phase == "run" and max(-0.9, min(1.8, accel * 0.03)) or 0
  local tau = want > race.surge and 0.06 or 0.35
  race.surge = race.surge + (want - race.surge) * (1 - exp(-dt / tau))
  race.shake = max(0, race.shake - 1.6 * dt)
end

local function step(dt)
  for i = #race.cues, 1, -1 do race.cues[i] = nil end
  if not (dt > 0) then return end
  if race.phase == "run" then
    run(dt)
  elseif race.phase == "crash" then
    local wdt = dt * (race.crash_t < SLOWMO_TIME and SLOWMO or 1)
    race.crash_t, race.crash_w = race.crash_t + dt, race.crash_w + wdt
    race.dist = race.dist + speed_now() * wdt
    for _, o in ipairs(race.obstacles) do o.v = o.v * exp(-2.5 * wdt) end -- traffic brakes for the wreck
    move_obstacles(wdt)
    if race.crash_t >= CRASH_TIME then race.phase = "over"; cue("over") end
  else
    race.over_t = race.over_t + dt
    move_obstacles(dt)
  end
  feel(dt)
end

-- The planner: the attract mode drives with it (game/bot.lua has its own,
-- seeing only what a player sees). Seconds until something in `lane`
-- reaches the car, looking `horizon` ahead.
local function threat(lane, horizon, anticipate)
  local p = max(1, speed_now())
  local best = math.huge
  for _, o in ipairs(race.obstacles) do
    if not o.knock and not o.passed and (covers(o, lane) or (anticipate and signalling(o) and o.change == lane)) then
      local ol = KINDS[o.kind][2]
      local rel = o.d - race.dist
      if rel + ol >= -CAR_L then
        local gap = rel - ol - CAR_L
        local ttc = gap <= 0 and 0 or gap / max(1, p - o.v)
        if ttc < horizon then best = min(best, ttc) end
      end
    end
  end
  return best
end

local function plan(horizon, anticipate)
  local here = race.lane
  local th = {}
  for l = 0, LANES - 1 do th[l] = threat(l, horizon, anticipate) end
  if th[here] == math.huge then return here end
  local best, score = here, th[here]
  for l = 0, LANES - 1 do
    if l ~= here and not (abs(l - here) == 2 and th[1] < 0.8) then
      local s = th[l] - 0.12 * abs(l - here)
      if s > score + 0.05 then best, score = l, s end
    end
  end
  return best
end

local function autopilot()
  if crashed() or race.oil > 0 then return end
  local want = plan(1.4, true)
  if want == race.lane or abs(race.x - lane_x(race.lane)) > 0.5 then return end
  -- Leaves one-lane dodges late: near misses sell the title card.
  if abs(want - race.lane) > 1 or threat(race.lane, 2.0, true) < 0.5 then
    steer(want > race.lane and 1 or -1)
  end
end

-- The world -------------------------------------------------------------------------

local ground = look.ground

local roads = {}
for i = 1, 5 do roads[i] = Mesh { geometry = look.road, material = look.road_material, y = 0.05 } end

-- Draws a node's copies from a flat list (hidden when there are none).
local function show(node, list, stride)
  node.visible = #list > 0
  if #list > 0 then node:instance(list, stride) end
end

-- The roadside, a stretch at a time: what look.props lists, stood on the
-- ground. One Mesh a geometry a stretch: every copy of it is one draw.
local SIDE_LEN = 96
local kinds = {} -- the props' geometries, each once
for _, p in ipairs(look.props) do
  if not table.find(kinds, p.geometry) then kinds[#kinds + 1] = p.geometry end
end
local sides = {}
for i = 1, 5 do
  local meshes = {}
  for k, g in ipairs(kinds) do meshes[k] = Mesh { geometry = g, material = look.prop_material } end
  sides[i] = { meshes = meshes, n = nil }
end
local prop_gen = love.math.newRandomGenerator(1)
local function dress(slot, n)
  slot.n = n
  prop_gen:setSeed(4099 + n * 7919)
  local function r(a, b) return a + (b - a) * prop_gen:random() end
  local lists = {}
  for k = 1, #kinds do lists[k] = {} end
  local z0 = -n * SIDE_LEN
  for _, p in ipairs(look.props) do
    local list = lists[table.find(kinds, p.geometry)]
    local function put(x, z)
      local y = ground:heightAt(x, z)
      if p.above and y < p.above then return end
      local k = #list
      list[k + 1], list[k + 2], list[k + 3] = x, y, z
      list[k + 4], list[k + 5], list[k + 6] = r(0, 6.3), r(p.scale[1], p.scale[2]), 0xffffff
    end
    for _, side in ipairs(p.sides) do
      if p.every then
        -- A row down this side, a little uneven.
        for z = z0 - p.every / 2, z0 - SIDE_LEN, -p.every do
          put(side * r(p.x[1], p.x[2]), z + r(-1, 1))
        end
      else
        for _ = 1, floor(p.count / #p.sides + 0.5) do
          put(side * r(p.x[1], p.x[2]), z0 - r(0, SIDE_LEN))
        end
      end
    end
  end
  for k, m in ipairs(slot.meshes) do show(m, lists[k], 6) end
end

-- Things that ride along with the car (the sea), each at its data.z ahead.
local riders = look.ride or {}

local hero = Mesh { geometry = look.hero, material = look.hero_material }

-- One Mesh for every copy of a kind, its copies listed each frame.
local function many(geo, mat) return Mesh { geometry = geo, material = mat, visible = false } end
local cars = many(look.traffic_car, look.traffic_material)
local trucks = many(look.truck, look.traffic_material)
local roadworks = many(look.cones, look.cones_material)
local oils = many(look.oil, look.oil_material)
local coins = many(look.coin, look.coin_material)
local pads = many(look.pad, look.pad_material)
local blinkers = many(look.blinker, look.blinker_material)
local arrows = many(look.arrow, look.blinker_material)

-- The car's pose: across, up, and its turns (the crash tumble by the crash's world time).
local function pose()
  if race.phase == "run" then
    local wobble = race.oil > 0 and sin(race.oil * 40) * 0.25 * (race.oil / OIL_LOCK) or 0
    return race.x, 0, max(-0.3, min(0.3, -race.vx * 0.03)) + wobble, max(-0.12, min(0.12, race.vx * 0.014)), 0.02 * race.boost_k
  end
  local w = race.phase == "crash" and race.crash_w or 2
  local dir, k = race.crash_dir, smooth(w / 0.5)
  return race.crash_x + dir * 1.1 * smooth(w / 0.8), max(0, 5 * w - 11 * w * w) + 0.05 * k,
    dir * 0.9 * smooth(w / 0.8), -dir * pi * k, 0.25 * sin(pi * k)
end

local lean = 0

local function world(dt, title)
  local dist = race.dist
  -- The road and roadside leapfrog ahead of the car.
  local L = look.STRETCH
  local first = floor((dist - 40) / L)
  for i, m in ipairs(roads) do m.z = -((first + i - 1) * L + L / 2) end
  local n0 = floor((dist - 30) / SIDE_LEN)
  for i = 0, #sides - 1 do
    local n = n0 + i
    local slot = sides[n % #sides + 1]
    if slot.n ~= n then dress(slot, n) end
  end
  for _, node in ipairs(riders) do node.z = node.data.z - dist end

  -- The player's car.
  local x, lift, yaw, roll, pitch = pose()
  -- Upside down, the roof rests on the road (the body turns about its middle).
  hero.x, hero.y, hero.z = x, look.hero_lift + lift + look.roof * (1 - cos(roll)) / 2, -dist
  hero.ry, hero.rz, hero.rx = yaw, roll, pitch
  -- A flame from each pipe on boost (left behind as a trail), smoke from the wreck.
  if dt > 0 and race.boost_k > 0.3 and race.phase == "run" then
    fx.burst("sparks", x - 0.5, 0.3, -dist + 2.0, look.flame)
    fx.burst("sparks", x + 0.5, 0.3, -dist + 2.0, look.flame)
  end
  if dt > 0 and race.phase ~= "run" and floor(round.time * 60) % 4 == 0 then
    fx.burst("smoke", x, 0.9 + lift, -dist, look.smoke)
  end

  -- Everything else, a list per kind: x, y, z, yaw, scale, colour, pitch, roll.
  local lists = { car = {}, truck = {}, cones = {}, oil = {} }
  local blink, arrow = {}, {}
  local now = round.time
  for _, o in ipairs(race.obstacles) do
    local list = lists[o.kind]
    local ox, oy, oz, oyaw, opitch, oroll = o.x, 0, -o.d, 0, 0, 0
    local k = o.knock
    if k then
      ox, oy, oz = ox + k.x, k.y, oz + k.z
      oyaw, oroll = k.angle, k.angle * 0.5
    elseif o.change then
      oyaw = -0.12 * sin(pi * shift(o)) * (o.change > o.from and 1 or -1)
    end
    local tint = o.kind == "truck" and look.truck_tints[(o.tint - 1) % #look.truck_tints + 1]
      or o.kind == "car" and look.car_tints[o.tint] or 0xffffff
    local lift = o.kind == "car" and look.hero_lift or o.kind == "truck" and look.truck_lift
      or o.kind == "cones" and look.cones_lift or 0.03
    local c = #list
    list[c + 1], list[c + 2], list[c + 3], list[c + 4] = ox, oy + lift, oz + (o.kind == "cones" and 7 or 0), oyaw
    list[c + 5], list[c + 6], list[c + 7], list[c + 8] = 1, tint, opitch, oroll
    -- A truck about to move over blinks (on the side it's going) with an arrow over it.
    if not k and signalling(o) and floor(now * 3.4) % 2 == 0 then
      local side = o.change > o.from and 1 or -1
      for _, z in ipairs({ -4.5, 3.05 }) do
        local b = #blink
        blink[b + 1], blink[b + 2], blink[b + 3] = ox + side * 1.02, 0.9, oz + z
      end
      local a = #arrow
      arrow[a + 1], arrow[a + 2], arrow[a + 3], arrow[a + 4], arrow[a + 5] = ox, 3.6, oz, side > 0 and 0 or pi, 1.4
      arrow[a + 6], arrow[a + 7] = 0xffffff, 0
    end
  end
  show(cars, lists.car, 8)
  show(trucks, lists.truck, 8)
  show(roadworks, lists.cones, 8)
  show(oils, lists.oil, 8)
  show(blinkers, blink, 3)
  show(arrows, arrow, 7)
  local c, spin = {}, now * look.coin_spin
  for _, coin in ipairs(race.coin_list) do
    if not coin.taken then
      local k = #c
      c[k + 1], c[k + 2], c[k + 3], c[k + 4], c[k + 5], c[k + 6], c[k + 7] = lane_x(coin.lane), 0.75, -coin.d, spin + coin.d * look.coin_stagger, 1, 0xffffff, pi / 2
    end
  end
  show(coins, c, 7)
  local pl = {}
  for _, pd in ipairs(race.pads) do
    local k = #pl
    pl[k + 1], pl[k + 2], pl[k + 3] = lane_x(pd.lane), 0.1, -pd.d
  end
  show(pads, pl, 3)

  -- The chase camera: lower and closer as it gets faster while the FOV
  -- widens (the road stretches past), left behind when the car surges.
  local cruise = smooth(min(1, race.feel))
  local back = CAM_BACK - (CAM_BACK - CAM_BACK_FAST) * cruise + race.surge
  local high = CAM_HIGH - (CAM_HIGH - CAM_HIGH_FAST) * cruise
  local px, py, pz = x * 0.5, high, -dist + back
  local tx, ty, tz = x * 0.7, 0.5 + 0.3 * (high - CAM_HIGH), -dist - 12
  local crash_t = race.phase == "crash" and race.crash_t or race.phase == "over" and CRASH_TIME + race.over_t
  if crash_t then
    -- Swing out to the side the car slid to, and up, round the wreck.
    local k = smooth(crash_t / 1.2)
    local a = race.crash_dir * (0.5 + 0.3 * smooth(crash_t / 4)) + 0.04 * sin(crash_t * 0.6)
    local cx, cy, cz = x, 0.6 + lift, -dist
    px, py, pz = px + (cx + sin(a) * 8.2 - px) * k, py + (cy + 5.4 - py) * k, pz + (cz + cos(a) * 8.2 - pz) * k
    tx, ty, tz = tx + (cx - tx) * k, ty + (cy + 2.2 - ty) * k, tz + (cz - tz) * k
  end
  if title then
    -- The title card: low beside the car's shoulder, swaying.
    px, py, pz = x + 2.1 + 0.35 * sin(now * 0.31), 1.9 + 0.15 * sin(now * 0.23), -dist + 7.4
    tx, ty, tz = x - 0.9, 0.9, -dist - 4
  end
  -- Road buzz that grows with the speed, and shake from hits.
  local f = min(1.8, race.feel)
  local buzz = title and 0 or 0.035 * f * f
  local t = race.clock * 9 * 2 * pi
  px = px + buzz * 0.5 * (sin(t * 0.71 + 2.1) * 0.6 + sin(t * 1.93) * 0.4) + race.shake ^ 2 * 0.45 * sin(now * 61)
  py = py + buzz * (sin(t) * 0.6 + sin(t * 2.37 + 1.3) * 0.3 + sin(t * 5.11 + 0.4) * 0.1) + race.shake ^ 2 * 0.45 * cos(now * 53)
  camera.x, camera.y, camera.z = px, py, pz
  camera:lookAt(tx, ty, tz)
  lean = lean + (((crash_t or title) and 0 or max(-0.06, min(0.06, -race.vx * 0.01))) - lean) * min(1, dt * 10)
  camera.roll = lean
  local fov = title and 0.92 or FOV_SLOW + (FOV_FAST - FOV_SLOW) * cruise + FOV_BOOST * race.boost_k + 0.1 * min(1, race.kick)
  camera.fov = math.deg(fov)
end

-- Sound and haptics (the title card's attract mode plays neither) ---------------

-- Each sound has three voices, reused in turn (a round may make 512 sources).
local sfx, VOICES = {}, 3
local function play(name, volume, pitch)
  local v = sfx[name]
  v.next = v.next % VOICES + 1
  local s = v[v.next]
  s:stop()
  s:setVolume(volume or 1)
  s:setPitch(pitch or 1)
  s:play()
end

local NOTES = { 1, 1.122, 1.26, 1.498, 1.682 } -- a coin run climbs a scale

local banner, banner_t = nil, -10

local function react()
  for _, c in ipairs(race.cues) do
    local k = c[1]
    if k == "lane" then
      play("lane", 0.7, c[3] and 1.15 or 1)
      haptic("tap")
    elseif k == "bump" then
      play("bump", 0.6)
    elseif k == "coin" then
      play("coin", 0.6, NOTES[(c[2] - 1) % 5 + 1] * (c[2] > 5 and 2 or 1))
      fx.burst("sparkles", race.x, 1.0, -race.dist - 1.5, look.coin_burst)
    elseif k == "near" then
      play("near", 0.8, 1 + 0.12 * (min(5, c[2]) - 1))
      haptic("tap")
      local words = c[2] > 1 and ("CLOSE! ×" .. c[2] .. "  +" .. c[3]) or ("CLOSE! +" .. c[3])
      fx.popup(words, race.x * 0.6, 2.4, -race.dist - 7, { color = look.near, size = 0.32 })
    elseif k == "boost" then
      play("boost", 0.9)
      haptic("heavy")
      fx.flash(0x7ff4ff, 0.15)
    elseif k == "smash" then
      play("smash", 0.9)
      haptic("heavy")
      fx.burst("burst", c[2], 1, c[3], look.smash)
      fx.popup("SMASH! +" .. SMASH_POINTS, race.x * 0.6, 2.6, -race.dist - 7, { color = look.near, size = 0.36 })
    elseif k == "oil" then
      play("oil", 0.8)
      haptic("heavy")
    elseif k == "blinker" then
      play("blinker", 0.5)
    elseif k == "speedup" then
      play("speedup", 0.7)
      banner, banner_t = "SPEED UP!  " .. floor(cruise_at(race.t) * SPEEDO + 0.5) .. " KM/H", round.time
    elseif k == "new" then
      banner, banner_t = NEWS[c[2]], round.time
      play("speedup", 0.6, 0.8)
    elseif k == "crash" then
      play("crash", 1)
      haptic("fail")
      fx.burst("sparks", c[2], 0.8, c[3], look.sparks)
      fx.hitstop(0.08)
      fx.flash(0xffffff, 0.12)
    end
  end
end

-- The round -----------------------------------------------------------------------

-- The debug menu (and the "Autopilot" scenario, which sets AUTO): drive by itself.
AUTO = false
debug.toggle("Autopilot", function(on) AUTO = on end, AUTO)
-- A boost now, as a pad gives (to see and tune how a boost looks and feels).
debug.action("Boost", function()
  if race.phase ~= "run" then return end
  race.boost = BOOST_TIME
  kick(0.9)
  cue("boost")
end)

local font, small
local touch -- the touch now: where it went down and whether it swiped
local stick = 0 -- the steering axis's last side (arrows, A/D, the d-pad, the stick)
local over_said = false

function love.load()
  new_race()
  spawn()
  for _, name in ipairs({ "lane", "bump", "coin", "near", "boost", "smash", "oil", "blinker", "speedup", "crash" }) do
    local s = love.audio.newSource(name, "static")
    sfx[name] = { s, s:clone(), s:clone(), next = 0 }
  end
  font = love.graphics.newFont(22)
  small = love.graphics.newFont(14)
end

-- Tap the left or right half to change lanes; a swipe in the same touch aims from where it began.
function love.mousepressed(x, y)
  if round.title or round.ended then return end
  touch = { x = x, swiped = false }
  press(x < love.graphics.getWidth() / 2 and -1 or 1)
end

function love.mousemoved(x, y)
  if not touch or touch.swiped or round.title or round.ended then return end
  if abs(x - touch.x) > SWIPE then
    touch.swiped = true
    swipe(x > touch.x and 1 or -1)
  end
end

function love.mousereleased() touch = nil end

function love.update(dt)
  if round.title then
    autopilot()
    step(dt)
    if race.phase == "over" then new_race(); spawn() end
  elseif not round.ended then
    if AUTO then autopilot() end
    -- Every way of playing does everything: a push of the axis is a tap.
    local side = input.ax > 0.5 and 1 or (input.ax < -0.5 and -1 or 0)
    if side ~= 0 and side ~= stick then press(side) end
    stick = side
    step(dt)
    round.score = floor(race.dist) + race.coins * COIN_POINTS + race.bonus
    if race.phase == "over" and not over_said then
      over_said = true
      round.over("WRECKED!", race.cause)
    end
  else
    step(dt)
  end
  world(dt, round.title)
  if not round.title then react() end
end

-- A 0xRRGGBB colour as LÖVE's r, g, b.
local function rgb(c) return floor(c / 65536) % 256 / 255, floor(c / 256) % 256 / 255, c % 256 / 255 end

-- What the HUD doesn't show: the speedo, the hint until you've steered, and banners.
function love.draw()
  local r, g, b = rgb(look.banner)
  if round.title then return end
  local w, h = love.graphics.getDimensions()
  -- Speed lines from the vanishing point, in an outer ring, once it's fast.
  local lines = max(0, min(1, (race.feel - 0.75) / 0.75))
  if lines > 0 and race.phase == "run" then
    local cx, cy = camera:project(race.x * 0.7, 0.8, -race.dist - 60)
    love.graphics.setLineWidth(1.5)
    for i = 1, 18 do
      local a = i * 2.39996 + floor(round.time * 24) * 0.7
      local r0 = (0.55 + 0.25 * ((i * 0.618 + round.time * 3) % 1)) * h * 0.5
      local r1 = r0 + 40 + 60 * lines
      love.graphics.setColor(1, 1, 1, 0.22 * lines)
      love.graphics.line(cx + cos(a) * r0, cy + sin(a) * r0, cx + cos(a) * r1, cy + sin(a) * r1)
    end
  end
  if round.ended or race.phase ~= "run" then return end
  -- The speedo, bottom right.
  love.graphics.setFont(font)
  love.graphics.setColor(look.paper)
  local kmh = floor(speed_now() * SPEEDO + 0.5)
  love.graphics.printf(tostring(kmh), 0, h - 64, w - 64, "right")
  love.graphics.setFont(small)
  love.graphics.printf("KM/H", 0, h - 56, w - 20, "right")
  -- The hint: until you've steered three times, and again when you stall early on.
  if race.steers < 3 or (race.idle > 3 and race.t < 30) then
    local a = 0.6 + 0.3 * sin(round.time * 6)
    local y = h * 0.47
    local nudge = 4 * sin(round.time * 6)
    love.graphics.setColor(r, g, b, 0.55 * a)
    love.graphics.rectangle("fill", w / 2 - 78, y - 4, 156, 26, 13, 13)
    for side = -1, 1, 2 do
      local x = w / 2 + side * (108 + nudge)
      love.graphics.polygon("fill", x + side * 14, y + 9, x - side * 4, y - 6, x - side * 4, y + 24)
    end
    love.graphics.setColor(1, 1, 1, a)
    love.graphics.setFont(small)
    love.graphics.printf("TAP LEFT · RIGHT", 0, y + 1, w, "center")
    for side = -1, 1, 2 do
      local x = w / 2 + side * (108 + nudge)
      love.graphics.polygon("fill", x + side * 12, y + 9, x - side * 2, y - 2, x - side * 2, y + 20)
    end
  end
  -- A banner: a speed-up or something new on the road.
  if banner and round.time - banner_t < 1.6 then
    local k = round.time - banner_t
    local a = min(1, k * 6) * min(1, (1.6 - k) * 4)
    love.graphics.setColor(r, g, b, 0.75 * a)
    love.graphics.rectangle("fill", 0, h * 0.24 - 4, w, 34)
    love.graphics.setColor(1, 1, 1, a)
    love.graphics.setFont(font)
    love.graphics.printf(banner, 0, h * 0.24, w, "center")
  end
end
