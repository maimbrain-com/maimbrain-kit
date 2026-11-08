-- The rules of the run: the hero's jump, the course generator, collisions,
-- scoring and the tumble when it goes wrong. It draws and plays nothing:
-- it pushes cues (jumps, coins, close calls, hits…) that main.lua turns
-- into sound, haptics and juice, and look.lua draws what it says.
--
-- Units: world x in px (the hero runs toward +x and sits at `dist`),
-- heights in px above the ground (+ up), time in seconds. A world x is
-- drawn at screen x HERO_X + (x - dist), a height h at GROUND_Y - h.

local floor, max, min, abs, cos, exp, pi = math.floor, math.max, math.min, math.abs, math.cos, math.exp, math.pi
local TAU = 2 * pi

-- The random generators, one for the run and one for the autopilot, made
-- once and seeded again for each new run: a round may make only 256, and
-- the title card starts a new run after every crash.
local generators = {}
local function generator(role, seed)
  local g = generators[role]
  if not g then
    g = love.math.newRandomGenerator(seed)
    generators[role] = g
  end
  g:setSeed(seed)
  return g
end

-- ---- Tuning knobs -----------------------------------------------------
-- Running speed at the start, px/s. Raise it and the first obstacles come
-- sooner and need earlier taps.
local START_SPEED = 235
-- Speed added every SPEED_EVERY seconds, px/s ("FASTER!").
local SPEED_STEP = 15
-- Seconds between speed-ups.
local SPEED_EVERY = 12
-- Top speed, px/s. Past ~400 the 250 px of look-ahead is under 0.65 s.
local MAX_SPEED = 385
-- A new obstacle family joins every this many seconds (see ORDER).
local FAMILY_EVERY = 15
-- Seconds of open ground between obstacles (min, max) at the start and at
-- full difficulty (RAMP_TIME). Lower = denser course.
local GAP_START = { 1.15, 1.85 }
local GAP_END = { 0.6, 1.05 }
-- Seconds until gaps are at GAP_END and the press window at WINDOW_END.
local RAMP_TIME = 90
-- Seconds of open ground (with a row of coins) before the first obstacle.
local WARMUP = 3.3
-- Launch speed of a jump, px/s. With GRAVITY_HOLD it sets the full jump's
-- height (v² / 2g ≈ 160 px).
local JUMP_VY = 760
-- Gravity while the finger is down and the hero is rising, px/s². Lower =
-- floatier, higher held jumps.
local GRAVITY_HOLD = 1800
-- Gravity otherwise (falling, or rising after release), px/s².
local GRAVITY = 2600
-- Letting go cuts the upward speed to this, px/s: the lower, the shorter a
-- quick tap's hop. Holding longer than MAX_HOLD s does nothing more.
local CUT_VY = 430
local MAX_HOLD = 0.42
-- The quickest tap counts as held this long, s, so it always clears a crate.
local MIN_HOLD = 0.11
-- One extra jump in the air (and its launch speed, px/s).
local DOUBLE_JUMP = true
local DOUBLE_VY = 560
-- A pass this close (px) counts as a CLOSE! call (bonus + slow-mo).
local CLOSE_PX = 9
-- Points: one per meter run, plus these.
local COIN_POINTS = 5
local CLOSE_POINTS = 10
local PX_PER_M = 30
-- The difficulty curve, in the one number that matters: how long (s) the
-- window is in which a press clears an obstacle. The generator resizes each
-- obstacle (within resize's limits) toward WINDOW_START at the start and
-- WINDOW_END at RAMP_TIME, then on toward MIN_WINDOW, never below it.
-- Wider = gentler; this is the knob for "too hard / too easy".
local WINDOW_START = 0.34
local WINDOW_END = 0.17
local MIN_WINDOW = 0.12
-- Where the hero runs on screen (px from the left edge): everything to its
-- right (360 - HERO_X px) is the player's look-ahead.
local HERO_X = 110
-- ---- End of tuning knobs ----------------------------------------------

-- The sim advances in fixed ticks (no tunnelling through thin crates at any dt).
local STEP = 1 / 120
-- The hero's hitbox: half-width and height (px). Drawn a bit bigger: near
-- misses look closer than they are, which is the fair direction.
local HERO_HW = 12
local HERO_H = 40
-- After running off an edge, a press still counts as a ground jump this long (s).
local COYOTE = 0.08
-- A press just before landing jumps on landing (s).
local BUFFER = 0.12
-- The ends of a pit you can still stand on (px), and how far below the
-- ground the hero can be over a pit and still pop back up at its far edge.
local PIT_GRACE = 6
local PIT_FALL = 14
-- From the hit to the game-over card (s): the tumble plays first.
local DEATH_TIME = 1.5
-- Run-cycle stride (px per step of the legs).
local STRIDE = 46
-- Bees bob at this rate (rad/s).
local BOB_RATE = 4.2
-- How far ahead of the hero the course is made (px).
local SPAWN_AHEAD = 820
-- Speed changes glide at this rate (px/s²).
local SPEED_ACCEL = 30
-- The generator measures each obstacle from this far before it (s).
local LOOK = 1.4

-- Obstacle families, in the order they join the run:
--   crate    a low crate: a tap clears it
--   pit      a gap in the ground: jump it (hold for wide ones)
--   stack    a tall stack of crates: hold for a high jump (or double jump)
--   gull     a gull at head height + a bit: stay low, don't jump into it
--   bee      a bee bobbing up and down: time it
--   skimmer  a gull skimming the ground, fast: jump it
--   log      a long low log: hold for a long jump
local ORDER = { "crate", "pit", "stack", "gull", "bee", "skimmer", "log" }

-- How often each is picked once it's joined (relative).
local WEIGHT = { crate = 3, pit = 2.2, stack = 2, gull = 1.6, bee = 1.6, skimmer = 1.6, log = 1.6 }

-- Hold times worth trying, in order of preference (the plan takes the first
-- with a comfortable window).
local HOLDS = {
  crate = { MIN_HOLD, 0.2 },
  skimmer = { MIN_HOLD, 0.2 },
  pit = { MIN_HOLD, 0.2, 0.3, MAX_HOLD },
  stack = { MAX_HOLD },
  gull = {},
  bee = { MIN_HOLD, 0.2, MAX_HOLD },
  log = { 0.2, 0.3, MAX_HOLD },
}

local function clamp(x, lo, hi)
  if x < lo then return lo elseif x > hi then return hi end
  return x
end

-- ---- Obstacles ------------------------------------------------------------
-- An obstacle: kind; x (its left edge at time t0, moving at vx); w, h; y
-- (its bottom above the ground at its lowest); bob and phase (a bee's
-- bobbing); nasty (a hard one: never two in a row); intro (the first of its
-- family); announced, passed; window (the press window the generator
-- measured; 1 = run under it); min_gap (the closest pass, for CLOSE!).

local function left(o, t)
  return o.x + o.vx * (t - o.t0)
end

local function bottom(o, t)
  if o.bob == 0 then return o.y end
  return o.y + o.bob * (0.5 - 0.5 * cos(BOB_RATE * (t - o.t0) + o.phase))
end

local function overPit(o, hx)
  return hx > o.x + PIT_GRACE and hx < o.x + o.w - PIT_GRACE
end

-- Would a hero centred at world x hx, feet h above the ground, be in
-- trouble at time t? (For a pit: standing over its middle.)
local function hits(o, t, hx, h)
  if o.kind == "pit" then
    return h <= 0 and overPit(o, hx)
  end
  local l = left(o, t)
  if hx + HERO_HW <= l or hx - HERO_HW >= l + o.w then return false end
  local b = bottom(o, t)
  return h < b + o.h and h + HERO_H > b
end

-- One step bigger/harder (grow) or smaller/easier, within this kind's
-- limits; false if it's already at the limit. v: the speed it's met at.
local function resize(o, grow, v)
  local k = grow and 1.08 or 1 / 1.08
  local w, h, vx, phase = o.w, o.h, o.vx, o.phase
  local kind = o.kind
  if kind == "crate" then
    o.h = clamp(o.h * k, 18, 62)
    o.w = clamp(o.w * k, 22, 52)
  elseif kind == "stack" then
    o.h = clamp(o.h * k, 66, 134)
  elseif kind == "log" then
    o.w = clamp(o.w * k, 70, 170)
  elseif kind == "pit" then
    o.w = clamp(o.w * k, v * 0.22, v * 0.62)
  elseif kind == "gull" then
    o.vx = -clamp(-o.vx * k, 50, 260)
  elseif kind == "skimmer" then
    o.vx = -clamp(-o.vx * k, 50, 260)
    o.h = clamp(o.h * k, 22, 44)
  elseif kind == "bee" then
    o.phase += 0.6
  end
  return w ~= o.w or h ~= o.h or vx ~= o.vx or phase ~= o.phase
end

-- ---- The jump ---------------------------------------------------------------

-- One tick of the jump's vertical motion: low gravity while held and
-- rising (the first MIN_HOLD always counts as held), a cut on release.
local function fall(hero, dt)
  local holding = hero.held or hero.jump_t < MIN_HOLD
  if not holding and not hero.cut then
    hero.cut = true
    hero.vy = min(hero.vy, CUT_VY)
  end
  local g = (holding and hero.vy > 0 and hero.jump_t < MAX_HOLD) and GRAVITY_HOLD or GRAVITY
  hero.vy -= g * dt
  hero.h += hero.vy * dt
  hero.jump_t += dt
end

local function launch(hero, vy)
  hero.vy = vy
  hero.jump_t = 0
  hero.cut = false
  hero.grounded = false
  hero.buffer = 0
end

-- Heights above the ground, one a tick after a press, for a jump held
-- `hold` seconds (until it lands). Each hold's curve is worked out once.
local curves = {}
local function jumpCurve(hold)
  local c = curves[hold]
  if c then return c end
  c = {}
  local b = { h = 0, vy = 0, jump_t = 0, cut = false, held = true, grounded = true, buffer = 0 }
  launch(b, JUMP_VY)
  while #c < 400 do
    if b.jump_t >= hold then b.held = false end
    fall(b, STEP)
    if b.h <= 0 then break end
    c[#c + 1] = b.h
  end
  curves[hold] = c
  return c
end

-- True if a hero at hero_x at time `now`, running at `speed`, gets past o
-- pressing `tau` seconds from now with the jump `curve` (or not pressing:
-- nil). The game's heaviest loop, so everything in it is inlined.
local function clears(o, now, hero_x, speed, tau, curve)
  local ox, ovx, ot0, ow, oh, oy, bob, phase = o.x, o.vx, o.t0, o.w, o.h, o.y, o.bob, o.phase
  local closing = max(speed - ovx, 1)
  local gap = ox + ovx * (now - ot0) - hero_x - HERO_HW - 4
  -- Only the ticks near the obstacle can touch it.
  local k0 = max(floor(gap / closing / STEP) - 2, 1)
  local n = curve and #curve or 0
  local pit = o.kind == "pit"
  for k = k0, k0 + 899 do
    local t = k * STEP
    local hx = hero_x + speed * t
    local h = 0
    if tau and t > tau then
      local j = floor((t - tau) / STEP + 0.5)
      if j >= 1 and j <= n then h = curve[j] end
    end
    if pit then
      if h <= 0 and hx > ox + PIT_GRACE and hx < ox + ow - PIT_GRACE then return false end
      if hx > ox + ow then return true end
    else
      local tt = now + t
      local l = ox + ovx * (tt - ot0)
      local r = l + ow
      if hx + HERO_HW > l and hx - HERO_HW < r then
        local b = oy
        if bob ~= 0 then b = oy + bob * (0.5 - 0.5 * cos(BOB_RATE * (tt - ot0) + phase)) end
        if h < b + oh and h + HERO_H > b then return false end
      end
      if hx - HERO_HW > r then return true end
    end
  end
  return true
end

-- The best way past o from here: { press = seconds from now (nil: just run
-- under it), hold, window }. No jump if that's safe (when the family allows
-- it), else the first hold (in the family's order) with a window of at
-- least MIN_WINDOW, else the widest window found, else nil.
local function plan(o, now, hero_x, speed)
  if o.kind == "gull" or o.kind == "bee" then
    -- Safe to just run under it, even a little off in time?
    if clears(o, now - 0.1, hero_x, speed) and clears(o, now, hero_x, speed) and clears(o, now + 0.1, hero_x, speed) then
      return { press = nil, hold = 0, window = 1 }
    end
  end
  local closing = max(speed - o.vx, 1)
  local arrive = (left(o, now) - hero_x - HERO_HW) / closing
  local span = clamp(arrive, 0, 1.3) + 0.05
  local n = floor(span / STEP)
  local best
  for _, hold in HOLDS[o.kind] do
    local curve = jumpCurve(hold)
    -- Try a press every tick; keep the longest run of them that clears.
    local run, bestRun, bestEnd = 0, 0, 0
    for i = 0, n do
      if clears(o, now, hero_x, speed, i * STEP, curve) then
        run += 1
        if run > bestRun then bestRun, bestEnd = run, i end
      else
        run = 0
      end
    end
    if bestRun > 0 then
      local window = bestRun * STEP
      local p = { press = (bestEnd - (bestRun - 1) / 2) * STEP, hold = hold, window = window }
      if window >= MIN_WINDOW then return p end
      if not best or window > best.window then best = p end
    end
  end
  return best
end

-- ---- The run ------------------------------------------------------------------

local Sim = {}
Sim.__index = Sim

-- A run on the course `seed` makes.
function Sim.new(seed)
  local s = setmetatable({
    t = 0,
    dist = 0, -- world x of the hero's centre
    speed = START_SPEED,
    hero = { h = 0, vy = 0, grounded = true, held = false, jump_t = 9, jumps = 0, cut = false,
             air_t = 0, buffer = 0, land_t = 9, land_speed = 0, flip = 0, run_phase = 0 },
    obstacles = {},
    coins = {}, -- { x, y (its centre's height), arc, taken }
    coins_got = 0,
    closes = 0,
    dying = nil, -- the tumble after a mistake
    over = false,
    cues = {},
    tier = 0, -- speed-ups so far
    longest_hold = 0, -- the longest a jump has been held (s): the hint teaches holding until then
    jumps = 0,
    double_jumps = 0,
    coin_t = 9, -- seconds since the last coin and the last close call (the face reacts)
    close_t = 9,
    meters = 0, -- distance run, in meters (frozen at the mistake)
    rng = generator("run", seed),
    acc = 0,
    cursor = START_SPEED * WARMUP,
    next_id = 1,
    next_arc = 1,
    introduced = 0,
    last_kind = nil,
    rep = 0,
    last_nasty = false,
    streak = 0,
  }, Sim)
  -- The warm-up: a row of coins at running height, so the first win comes
  -- in under a second whatever the player does.
  local arc = s:newArc()
  for i = 0, 8 do
    s.coins[#s.coins + 1] = { x = 120 + i * 34, y = HERO_H * 0.5, arc = arc, taken = false }
  end
  while s.cursor < s.dist + SPAWN_AHEAD do s:spawnOne() end
  return s
end

function Sim:score()
  return self.meters + self.coins_got * COIN_POINTS + self.closes * CLOSE_POINTS
end

function Sim:alive()
  return self.dying == nil
end

function Sim:cue(c)
  self.cues[#self.cues + 1] = c
end

-- The finger went down: jump (from the ground, or once more in the air).
function Sim:press()
  if self.dying or self.over then return end
  local hero = self.hero
  hero.held = true
  if hero.grounded or (hero.jumps == 0 and hero.air_t < COYOTE) then
    launch(hero, JUMP_VY)
    hero.jumps = 1
    self.jumps += 1
    self:cue({ type = "jump" })
  elseif DOUBLE_JUMP and hero.jumps < 2 then
    launch(hero, DOUBLE_VY)
    hero.jumps = 2
    hero.flip = 0.001
    self.double_jumps += 1
    self:cue({ type = "double" })
  else
    hero.buffer = BUFFER
  end
end

-- The finger came up: a jump still rising is cut short.
function Sim:release()
  self.hero.held = false
end

-- Advances by dt seconds in fixed ticks.
function Sim:step(dt)
  if self.over then return end
  self.acc += dt
  while self.acc >= STEP * 0.999 and not self.over do
    self.acc -= STEP
    self:tick(STEP)
  end
end

-- Once a frame: makes the course ahead (one obstacle at most, so no frame
-- pays for more than one) and forgets what's behind.
function Sim:generate()
  if self.cursor < self.dist + SPAWN_AHEAD then self:spawnOne() end
  local behind, t = self.dist - 300, self.t
  local obs = self.obstacles
  if #obs > 0 and max(left(obs[1], t), obs[1].x) + obs[1].w <= behind then
    local keep = {}
    for _, o in obs do
      if max(left(o, t), o.x) + o.w > behind then keep[#keep + 1] = o end
    end
    self.obstacles = keep
  end
  local coins = self.coins
  if #coins > 0 and coins[1].x <= behind then
    local keep = {}
    for _, c in coins do
      if c.x > behind then keep[#keep + 1] = c end
    end
    self.coins = keep
  end
end

-- The first obstacle the hero hasn't got past yet.
function Sim:nextObstacle()
  for _, o in self.obstacles do
    if not o.passed then return o end
  end
  return nil
end

-- Seconds until the hero reaches o at the current speeds.
function Sim:timeTo(o)
  return (left(o, self.t) - self.dist - HERO_HW) / max(self.speed - o.vx, 1)
end

-- The best way past o from the hero's current state on the ground.
function Sim:planFor(o)
  return plan(o, self.t, self.dist, self.speed)
end

function Sim:groundUnder(x)
  for _, o in self.obstacles do
    if o.kind == "pit" and overPit(o, x) then return false end
  end
  return true
end

function Sim:newArc()
  self.next_arc += 1
  return self.next_arc
end

function Sim:tick(dt)
  self.t += dt
  if self.dying then
    self:tumble(dt)
    return
  end
  -- Faster every SPEED_EVERY seconds, gliding to the new speed.
  local tier = floor(self.t / SPEED_EVERY)
  if tier > self.tier then
    self.tier = tier
    if START_SPEED + SPEED_STEP * tier <= MAX_SPEED + SPEED_STEP * 0.5 then
      self:cue({ type = "faster" })
    end
  end
  local target = min(START_SPEED + SPEED_STEP * self.tier, MAX_SPEED)
  self.speed = self.speed < target and min(self.speed + SPEED_ACCEL * dt, target) or target
  self.dist += self.speed * dt
  local m = floor(self.dist / PX_PER_M)
  if floor(m / 100) > floor(self.meters / 100) then
    self:cue({ type = "milestone", m = floor(m / 100) * 100 })
  end
  self.meters = m
  self.coin_t += dt
  self.close_t += dt

  -- The hero.
  local overGround = self:groundUnder(self.dist)
  local hero = self.hero
  hero.land_t += dt
  if hero.held and not hero.grounded then
    self.longest_hold = max(self.longest_hold, min(hero.jump_t, MAX_HOLD))
  end
  if hero.buffer > 0 then hero.buffer -= dt end
  if hero.grounded then
    hero.run_phase += self.speed * dt / STRIDE
    if not overGround then
      -- Ran off an edge.
      hero.grounded = false
      hero.jumps = 0
      hero.air_t = 0
      hero.vy = 0
      hero.jump_t = 9
      hero.cut = true
    end
  end
  if not hero.grounded then
    hero.air_t += dt
    fall(hero, dt)
    if hero.flip > 0 then
      hero.flip += dt * TAU / 0.42
      if hero.flip >= TAU then hero.flip = 0 end
    end
    if hero.h <= 0 and hero.vy <= 0 then
      if overGround then
        local speed = -hero.vy
        hero.h = 0
        hero.vy = 0
        hero.grounded = true
        hero.jumps = 0
        hero.flip = 0
        hero.land_t = 0
        hero.land_speed = speed
        self:cue({ type = "land", speed = speed })
        if hero.buffer > 0 then
          -- A press just before landing: jump now.
          launch(hero, JUMP_VY)
          hero.jumps = 1
          self.jumps += 1
          self:cue({ type = "jump" })
        end
      elseif hero.h < -PIT_FALL then
        self:die("pit")
        return
      end
    end
  end

  -- Obstacles: hits, near misses, passing.
  local t, hx, h = self.t, self.dist, hero.h
  local hit, closes
  for _, o in self.obstacles do
    if o.intro and not o.announced and left(o, t) - hx < 255 then
      o.announced = true
      if o.kind ~= "crate" then self:cue({ type = "newkind", kind = o.kind }) end
    end
    if not o.passed then
      if o.kind == "pit" then
        if hx >= o.x + o.w - PIT_GRACE then
          o.passed = true
          -- How high the hero was crossing the far edge.
          if h > 0 and h < CLOSE_PX * 1.6 then
            closes = closes or {}
            closes[#closes + 1] = { o.x + o.w, 0 }
          end
        end
      elseif hits(o, t, hx, h) then
        hit = { kind = o.kind, x = max(left(o, t), hx - HERO_HW), y = h + HERO_H * 0.5 }
        break
      else
        local l = left(o, t)
        local r = l + o.w
        if hx + HERO_HW > l and hx - HERO_HW < r then
          local b = bottom(o, t)
          local gap = if h >= b + o.h then h - (b + o.h) else b - (h + HERO_H)
          o.min_gap = min(o.min_gap, gap)
        end
        if hx - HERO_HW > r then
          o.passed = true
          if o.min_gap < CLOSE_PX then
            closes = closes or {}
            closes[#closes + 1] = { r, h + HERO_H * 0.5 }
          end
        end
      end
    end
  end
  if hit then
    self:cue({ type = "hit", kind = hit.kind, x = hit.x, y = hit.y })
    self:die(hit.kind)
    return
  end
  if closes then
    for _, c in closes do
      self.closes += 1
      self.close_t = 0
      self:cue({ type = "close", x = c[1], y = c[2] })
    end
  end

  -- Coins.
  local cy = hero.h + HERO_H * 0.5
  for _, c in self.coins do
    if not c.taken and abs(c.x - hx) < 22 and abs(c.y - cy) < 30 then
      c.taken = true
      self.streak = if self.coin_t < 0.45 then self.streak + 1 else 1
      self.coin_t = 0
      self.coins_got += 1
      self:cue({ type = "coin", x = c.x, y = c.y, streak = self.streak })
      local done = true
      for _, d in self.coins do
        if d.arc == c.arc and not d.taken then
          done = false
          break
        end
      end
      if done then self:cue({ type = "arcdone", x = c.x, y = c.y }) end
    end
  end
end

-- cause: "pit", or the kind of thing it bonked into.
function Sim:die(cause)
  local vx, vy, spin
  if cause == "pit" then
    vx, vy, spin = 0.4 * self.speed, min(self.hero.vy, 0) - 60, 9
    self:cue({ type = "fall" })
  else
    vx, vy, spin = -45, 540, -11
  end
  local x = self.dist
  local wall = x + 40
  for _, o in self.obstacles do
    if o.kind == "pit" and x > o.x and x < o.x + o.w then
      wall = o.x + o.w
      break
    end
  end
  self.dying = { t = 0, cause = cause, x = x, h = self.hero.h, vx = vx, vy = vy, rot = 0, spin = spin, resting = false, wall = wall }
end

function Sim:tumble(dt)
  -- The world stops (fast) while the hero tumbles, so it stays in view.
  self.speed *= exp(-14 * dt)
  self.dist += self.speed * dt
  local d = self.dying
  d.t += dt
  if not d.resting then
    d.vy -= GRAVITY * 0.75 * dt
    d.h += d.vy * dt
    d.x += d.vx * dt
    d.rot += d.spin * dt
    if d.cause == "pit" then
      -- Head first into the hole, jammed against the far wall.
      if d.x > d.wall - 20 then
        d.x = d.wall - 20
        d.vx = 0
      end
      if d.h <= -16 then
        d.h = -16
        d.resting = true
        self:cue({ type = "stuck" })
      end
    elseif self:groundUnder(d.x) and d.h <= 0 and d.vy < 0 then
      d.h = 0
      if -d.vy > 170 then
        d.vy = -d.vy * 0.45
        d.vx *= 0.55
        d.spin *= 0.5
        self:cue({ type = "bounce" })
      else
        d.resting = true
        d.vy = 0
      end
    end
    d.h = max(d.h, -400)
  else
    -- Slide to a stop and roll onto its back, feet in the air.
    d.vx *= exp(-9 * dt)
    d.x += d.vx * dt
    local rest = floor((d.rot / pi - 1) / 2 + 0.5) * 2 * pi + pi
    d.rot += (rest - d.rot) * (1 - exp(-14 * dt))
  end
  if d.t >= DEATH_TIME and not self.over then
    self.over = true
    self:cue({ type = "over" })
  end
end

-- ---- The course generator ---------------------------------------------------

function Sim:range(a, b)
  return a + (b - a) * self.rng:random()
end

function Sim:pickKind(unlocked)
  if self.last_nasty then
    -- Never two hard ones in a row: a crate (or a narrow pit) to breathe.
    return if unlocked >= 2 and self.rng:random() < 0.3 then "pit" else "crate"
  end
  for _ = 1, 3 do
    local total = 0
    for i = 1, unlocked do total += WEIGHT[ORDER[i]] end
    local r = self.rng:random() * total
    local pick = ORDER[1]
    for i = 1, unlocked do
      r -= WEIGHT[ORDER[i]]
      if r <= 0 then
        pick = ORDER[i]
        break
      end
    end
    -- The same family three times running is boring: roll again.
    if not (pick == self.last_kind and self.rep >= 2) then return pick end
  end
  return "crate"
end

function Sim:spawnOne()
  local speed = max(self.speed, 1)
  -- When (and how fast) the hero will get there.
  local eta = self.t + (self.cursor - self.dist) / speed
  local tier = floor(eta / SPEED_EVERY)
  local v = max(min(START_SPEED + SPEED_STEP * tier, MAX_SPEED), self.speed)
  local d = clamp(eta / RAMP_TIME, 0, 1)
  local unlocked = min(1 + floor(eta / FAMILY_EVERY), #ORDER)
  local kind, intro
  if unlocked > self.introduced and not self.last_nasty then
    self.introduced = unlocked
    kind, intro = ORDER[unlocked], true
  else
    kind, intro = self:pickKind(unlocked), false
  end
  -- The first of a family, and the breather after a hard one, come easy.
  local easy = intro or self.last_nasty
  -- Room to take off: big jumps start further back.
  if kind == "stack" or kind == "log" then
    self.cursor += 0.3 * v
  elseif kind == "pit" then
    self.cursor += 0.1 * v
  end
  local function lerp(a, b) return a + (b - a) * d end
  local o = {
    id = self.next_id, kind = kind, x = self.cursor, t0 = self.t + (self.cursor - self.dist) / speed,
    w = 30, h = 36, y = 0, vx = 0, bob = 0, phase = 0,
    nasty = kind ~= "crate", intro = intro, announced = false, passed = false, window = 1, min_gap = math.huge,
  }
  self.next_id += 1
  if kind == "crate" then
    o.w = self:range(28, 38)
    o.h = if easy then 30 else lerp(30, 44) * self:range(0.9, 1.05)
  elseif kind == "pit" then
    -- Widths in seconds of running: wide ones need a held jump.
    o.w = v * (if easy then 0.3 else lerp(0.32, 0.5) * self:range(0.9, 1.08))
    o.nasty = o.w > v * 0.4
  elseif kind == "stack" then
    o.w = self:range(38, 46)
    o.h = if intro then 84 else lerp(84, 114) * self:range(0.93, 1)
  elseif kind == "gull" then
    o.w, o.h, o.y = 56, 26, HERO_H + 22
    o.vx = -lerp(70, 120)
  elseif kind == "bee" then
    o.w, o.h, o.bob = 38, 34, 104
    o.phase = self:range(0, TAU)
  elseif kind == "skimmer" then
    o.w, o.h, o.y = 52, 28, 4
    o.vx = -lerp(110, 160)
  elseif kind == "log" then
    o.w = if intro then 86 else lerp(86, 128) * self:range(0.92, 1.05)
    o.h = 28
  end
  -- Fair by construction, and the difficulty curve: resize (or re-time) it
  -- until the press window that clears it is about the target for this
  -- point of the run (wide early, tight late), never under MIN_WINDOW. (It
  -- keeps tightening past RAMP_TIME, down to MIN_WINDOW: everyone falls
  -- eventually.)
  local k = min(eta / RAMP_TIME, 2)
  local target = max(WINDOW_START + (WINDOW_END - WINDOW_START) * k, MIN_WINDOW)
  if intro then
    target = WINDOW_START
  elseif easy then
    target = min(target + 0.04, WINDOW_START)
  end
  local best
  for _ = 1, 24 do
    best = plan(o, o.t0 - LOOK, o.x - v * LOOK, v)
    if best and best.press == nil then break end -- run under it
    local w = if best then best.window else 0
    o.window = w
    local grow
    if w < MIN_WINDOW or w < target * 0.9 then
      grow = false
    elseif w > target * 1.25 then
      grow = true
    else
      break
    end
    if not resize(o, grow, v) and w >= MIN_WINDOW then break end -- as big (or small) as this kind goes
  end
  -- Coins trace the ideal path: a reward for a good jump.
  local hx = o.x - v * LOOK
  local stop = o.x + o.w
  local coins = intro or self.rng:random() < 0.7
  if best and best.press then
    local curve = jumpCurve(best.hold)
    local px = hx + v * best.press
    stop = max(stop, px + v * #curve * STEP)
    if coins then
      local arc = self:newArc()
      local n = if #curve > 70 then 7 else 5
      for i = 0, n - 1 do
        local j = min(floor((i + 0.5) / n * #curve), #curve - 1)
        self.coins[#self.coins + 1] = { x = px + v * (j + 1) * STEP, y = curve[j + 1] + HERO_H * 0.5, arc = arc, taken = false }
      end
    end
  elseif coins then
    -- Run under it: a line of coins on the ground where you meet it.
    local arc = self:newArc()
    for i = 0, 4 do
      self.coins[#self.coins + 1] = { x = o.x - 50 + i * 30, y = HERO_H * 0.5, arc = arc, taken = false }
    end
  end
  local gap = self:range(lerp(GAP_START[1], GAP_END[1]), lerp(GAP_START[2], GAP_END[2]))
  if o.nasty then gap += 0.3 end
  if intro then gap += 0.35 end
  self.cursor = stop + gap * v
  self.rep = if kind == self.last_kind then self.rep + 1 else 1
  self.last_kind = kind
  self.last_nasty = o.nasty
  self.obstacles[#self.obstacles + 1] = o
end

-- ---- The autopilot ------------------------------------------------------------
-- Plays from the plans, nearly perfectly: the title card's run, and the
-- debug menu's Autopilot. `sloppy` (s) adds timing error.

local Autopilot = {}
Autopilot.__index = Autopilot

function Autopilot.new(seed, sloppy)
  return setmetatable({ rng = generator("autopilot", seed), sloppy = sloppy, done = 0 }, Autopilot)
end

-- Call once a frame before sim:step.
function Autopilot:drive(sim)
  if self.release_at and sim.t >= self.release_at then
    sim:release()
    self.release_at = nil
  end
  if self.press_at then
    if sim.t >= self.press_at then
      sim:press()
      self.release_at = sim.t + self.hold
      self.press_at = nil
    end
    return
  end
  if not sim.hero.grounded or self.release_at then return end
  local o
  for _, x in sim.obstacles do
    if not x.passed and x.id > self.done then
      o = x
      break
    end
  end
  if not o or sim:timeTo(o) > 1.1 then return end
  self.done = o.id
  local p = sim:planFor(o)
  if p and p.press then
    local err = (self.rng:random() * 2 - 1) * min(self.sloppy, p.window * 0.5)
    self.press_at = sim.t + max(p.press + err, 0)
    self.hold = p.hold
  end
end

return {
  Sim = Sim,
  Autopilot = Autopilot,
  left = left,
  bottom = bottom,
  HERO_X = HERO_X,
  HERO_H = HERO_H,
  JUMP_VY = JUMP_VY,
  START_SPEED = START_SPEED,
  MAX_SPEED = MAX_SPEED,
  PX_PER_M = PX_PER_M,
  CLOSE_POINTS = CLOSE_POINTS,
}
