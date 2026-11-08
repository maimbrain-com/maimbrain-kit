-- The rules, with no drawing or sound: jellies march down a winding road
-- toward the cake; towers on build pads beside the road pop them for coins;
-- every jelly that reaches the cake takes a bite (a heart). Out of hearts,
-- the cake is devoured.
--
-- The sim pushes cues ({ kind = "pop", … }: fired, popped, bitten, wave
-- started…) that main.lua turns into juice and sound.lua into sound. It
-- never draws or plays. All its randomness is its own generator, seeded by
-- the round, so a round depends only on its seed and the taps.

local sim = {}

-- ---- Tuning knobs -----------------------------------------------------------
-- Coins at the start of a round: enough for exactly one POP or CHILL tower
-- (BOOM costs more). Raise it and the opening gets easier.
sim.START_COINS = 60
-- Hearts (cake slices). Each jelly that reaches the cake bites one off (a
-- king bites KING_BITE). studio.toml's `lives` is the same number: the
-- HUD's hearts are these.
sim.HEARTS = 5
-- Seconds between wave starts once the first wave has begun. The main
-- pacing knob: lower means waves overlap sooner and rounds end faster.
sim.WAVE_GAP = 10
-- Wave 1 waits for the first tower (then starts FIRST_WAVE_DELAY later),
-- or starts by itself after this many seconds.
sim.FIRST_WAVE_WAIT = 9
sim.FIRST_WAVE_DELAY = 1.5
-- Coins paid at the start of each wave: PAYDAY + PAYDAY_GROWTH × wave.
sim.PAYDAY = 8
sim.PAYDAY_GROWTH = 2
-- Calling the next wave early pays this many coins per second skipped.
sim.EARLY_BONUS_PER_SEC = 2.5
-- Jelly hit points are multiplied by HP_BASE × HP_GROWTH^(wave − 1).
-- HP_GROWTH is the difficulty curve: 1.15 is gentle and long, 1.25 a wall
-- where every skill level dies around the same wave.
sim.HP_BASE = 0.8
sim.HP_GROWTH = 1.2
-- Wave 1's jellies are this much softer still: one POP on any pad pops them
-- all, so the first wave is always a win.
sim.FIRST_WAVE_HP = 0.6
-- Jellies also speed up a little each wave (fraction per wave, capped).
sim.SPEED_GROWTH = 0.012
sim.SPEED_GROWTH_MAX = 0.3
-- A king (boss) leads every 5th wave from this wave on.
sim.FIRST_BOSS_WAVE = 10
-- Rush waves (8, 13, 18…) pack their jellies this much tighter.
sim.RUSH_SPACING = 0.55
-- Seconds the devouring plays before the round is over.
sim.ENDING = 1.9

-- The towers, by kind, per level (1 = as built, 3 = maxed). POP: a fast
-- single-target gumball shooter. CHILL: a frost pulse that slows (and
-- nicks) everything in range. BOOM: a lobbed cherry bomb with splash damage.
--   cost      build, upgrade to level 2, upgrade to level 3
--   range     units
--   interval  seconds between shots (or pulses)
--   damage    per hit
--   special   BOOM: splash radius; CHILL: the slow factor (speed multiplier)
sim.KINDS = { "pop", "chill", "boom" }
sim.TOWERS = {
  pop = { name = "POP", cost = { 50, 40, 70 }, range = { 88, 96, 106 }, interval = { 0.5, 0.38, 0.28 }, damage = { 1.0, 1.5, 2.2 }, special = { 0, 0, 0 } },
  chill = { name = "CHILL", cost = { 60, 45, 75 }, range = { 72, 82, 92 }, interval = { 1.3, 1.15, 1.0 }, damage = { 0.4, 0.7, 1.1 }, special = { 0.55, 0.45, 0.35 } },
  boom = { name = "BOOM", cost = { 80, 60, 95 }, range = { 104, 114, 126 }, interval = { 1.7, 1.45, 1.2 }, damage = { 2.2, 3.4, 5.0 }, special = { 38, 46, 56 } },
}
-- POP's gumball speed (units/s) and BOOM's flight time (s).
sim.POP_SHOT_SPEED = 460
sim.BOOM_FLIGHT = 0.55
-- How long a CHILL pulse keeps a jelly slowed (s).
sim.SLOW_TIME = 1.5

-- The jellies, by kind.
--   hp       before the wave's multiplier
--   speed    units a second along the road
--   armor    subtracted from every hit (a hit always does at least a quarter),
--            so it counters many small hits: POP barely scratches a helmet
--   bounty   coins when popped
--   radius   units (its size on the road)
--   spacing  seconds between two of these in a wave
sim.CREEPS = {
  gummy = { name = "GUMMY", hp = 3.0, speed = 42, armor = 0.0, bounty = 5, radius = 13, spacing = 0.85 },
  zipper = { name = "ZIPPER", hp = 2.0, speed = 78, armor = 0.0, bounty = 4, radius = 10, spacing = 0.45 },
  helmet = { name = "HELMET", hp = 5.0, speed = 33, armor = 1.0, bounty = 8, radius = 14, spacing = 1.1 },
  splitter = { name = "SPLITTER", hp = 7.0, speed = 37, armor = 0.0, bounty = 5, radius = 17, spacing = 1.5 },
  mini = { name = "MINI", hp = 1.5, speed = 58, armor = 0.0, bounty = 2, radius = 8, spacing = 0.3 },
  king = { name = "KING", hp = 70.0, speed = 24, armor = 0.4, bounty = 50, radius = 24, spacing = 2.0 },
}
-- Hearts a king bites off.
sim.KING_BITE = 3
-- A splitter pops into this many minis.
sim.SPLIT_INTO = 3

-- ---- The map ----------------------------------------------------------------
-- The road, top (off screen) to the cake: the painted map (images/map.jpg)
-- has it baked in along exactly these points. Jellies walk it at constant speed.
sim.PATH = { { 196, -24 }, { 196, 160 }, { 84, 160 }, { 84, 284 }, { 276, 284 }, { 276, 404 }, { 180, 404 }, { 180, 492 } }
sim.ROAD_W = 36
sim.CAKE_X, sim.CAKE_Y = 180, 522
sim.CAKE_R = 34
-- Build pads beside the road (centres). Each is PAD units square.
sim.PADS = {
  { 128, 100 },
  { 258, 124 },
  { 140, 222 },
  { 226, 222 },
  { 326, 300 },
  { 32, 222 },
  { 180, 344 },
  { 84, 352 },
  { 326, 410 },
  { 124, 448 },
  { 248, 470 },
}
sim.PAD = 52
-- The pad the first-build hint points at (the one covering the most road).
sim.HINT_PAD = 3

-- ---- Waves ------------------------------------------------------------------

-- What wave `n` brings: { n, groups = { { kind, count }, … }, boss, newKind,
-- rush }. The first ten are written by hand (a new jelly every couple of
-- waves); after that it's a budget spent on a random mix, with a king every
-- fifth wave and a rush of zippers and minis in between.
function sim.planWave(n, rng)
  local groups, newKind
  if n == 1 then groups = { { "gummy", 4 } }
  elseif n == 2 then groups = { { "gummy", 7 } }
  elseif n == 3 then groups, newKind = { { "gummy", 6 }, { "zipper", 6 } }, "zipper"
  elseif n == 4 then groups, newKind = { { "helmet", 3 }, { "gummy", 8 } }, "helmet"
  elseif n == 5 then groups = { { "gummy", 10 }, { "zipper", 6 }, { "helmet", 2 } }
  elseif n == 6 then groups = { { "zipper", 10 }, { "helmet", 3 }, { "gummy", 4 } }
  elseif n == 7 then groups, newKind = { { "splitter", 4 }, { "gummy", 8 } }, "splitter"
  elseif n == 8 then groups = { { "zipper", 18 }, { "helmet", 3 } }
  elseif n == 9 then groups = { { "splitter", 5 }, { "helmet", 5 }, { "zipper", 8 } }
  elseif n == 10 then groups, newKind = { { "king", 1 }, { "gummy", 8 } }, "king"
  else
    local budget = 14 + 3 * n
    if n % 5 == 3 then
      return { n = n, groups = { { "zipper", budget // 2 }, { "mini", budget // 3 } }, boss = false, rush = true }
    end
    groups = {}
    if n % 5 == 0 and n >= sim.FIRST_BOSS_WAVE then
      groups[#groups + 1] = { "king", 1 + (n - sim.FIRST_BOSS_WAVE) // 10 }
      budget = budget // 2
    end
    -- Two kinds, never all the hard ones at once.
    local pool = { { "gummy", 2 }, { "zipper", 2 }, { "helmet", 4 }, { "splitter", 5 } }
    local a = math.floor(rng:random() * 4) % 4
    local b = (a + 1 + math.floor(rng:random() * 3) % 3) % 4
    for _, p in { pool[a + 1], pool[b + 1] } do
      local share = budget // 2
      groups[#groups + 1] = { p[1], math.max(share // p[2], 2) }
    end
  end
  local boss = false
  for _, g in groups do
    if g[1] == "king" then boss = true end
  end
  return { n = n, groups = groups, boss = boss, newKind = newKind, rush = n == 8 }
end

-- ---- The round --------------------------------------------------------------

local Sim = {}
Sim.__index = Sim

local function dist(ax, ay, bx, by)
  local dx, dy = ax - bx, ay - by
  return math.sqrt(dx * dx + dy * dy)
end

-- The road's length up to each waypoint.
local PATH_LEN = { 0 }
for i = 2, #sim.PATH do
  local a, b = sim.PATH[i - 1], sim.PATH[i]
  PATH_LEN[i] = PATH_LEN[i - 1] + dist(a[1], a[2], b[1], b[2])
end
sim.ROAD_LEN = PATH_LEN[#PATH_LEN]

-- The point `d` units along the road (clamped to its ends).
function sim.pointAt(d)
  d = math.clamp(d, 0, sim.ROAD_LEN)
  local P = sim.PATH
  for i = 1, #P - 1 do
    local l0, l1 = PATH_LEN[i], PATH_LEN[i + 1]
    if d <= l1 or i == #P - 1 then
      local k = if l1 > l0 then (d - l0) / (l1 - l0) else 0
      local a, b = P[i], P[i + 1]
      return a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k
    end
  end
  local last = P[#P]
  return last[1], last[2]
end
local pointAt = sim.pointAt

-- The pad under a tap, with a generous touch area.
function sim.padAt(x, y)
  local half = sim.PAD / 2 + 4
  for i, p in sim.PADS do
    if math.abs(x - p[1]) <= half and math.abs(y - p[2]) <= half then return i end
  end
  return nil
end

-- Road length inside a circle: how much a tower on `pad` with `range` sees.
function sim.coverage(pad, range)
  local p = sim.PADS[pad]
  local n, d = 0, 0
  while d < sim.ROAD_LEN do
    local x, y = pointAt(d)
    if dist(x, y, p[1], p[2]) <= range then n += 1 end
    d += 4
  end
  return n * 4
end

-- A new round on `seed`.
function sim.new(seed)
  local rng = love.math.newRandomGenerator(seed)
  local s = setmetatable({
    t = 0,
    coins = sim.START_COINS,
    hearts = sim.HEARTS,
    score = 0, -- jellies popped
    wave = 0, -- waves started so far (the one on screen)
    nextIn = sim.FIRST_WAVE_WAIT, -- seconds until the next wave starts by itself
    creeps = {},
    towers = {}, -- by pad: { kind, level, cd, aim, recoil, age } or nil
    shots = {},
    cues = {},
    ending = nil, -- seconds since the hearts ran out (the devouring)
    over = false,
    built = 0,
    upgrades = 0,
    killer = nil, -- the kind that took the last bite
    spawns = {}, -- jellies still to come: { seconds until, kind }
    hpMult = 1,
    speedMult = 1,
    chain = 0,
    chainT = 0,
    nextId = 1,
    rng = rng,
  }, Sim)
  s.next = sim.planWave(1, rng)
  return s
end

function Sim:range(a, b)
  return a + (b - a) * self.rng:random()
end

function Sim:cue(c)
  self.cues[#self.cues + 1] = c
end

function Sim:playing()
  return not self.over and self.ending == nil
end

-- Cost of upgrading the tower on `pad`, nil if empty or maxed.
function Sim:upgradeCost(pad)
  local t = self.towers[pad]
  if not t or t.level >= 3 then return nil end
  return sim.TOWERS[t.kind].cost[t.level + 1]
end

function Sim:towerCount()
  local n = 0
  for p = 1, #sim.PADS do
    if self.towers[p] then n += 1 end
  end
  return n
end

-- Builds a tower on an empty pad. False (and a "denied" cue if it was the
-- price) when it can't.
function Sim:build(pad, kind)
  if not self:playing() or not sim.PADS[pad] or self.towers[pad] then return false end
  local cost = sim.TOWERS[kind].cost[1]
  if self.coins < cost then
    self:cue({ kind = "denied" })
    return false
  end
  self.coins -= cost
  self.towers[pad] = { kind = kind, level = 1, cd = 0.25, aim = -math.pi / 2, recoil = 0, age = 0 }
  self.built += 1
  self:cue({ kind = "built", pad = pad, tower = kind })
  -- The first tower calls wave 1 (soon, not instantly: a beat to enjoy it).
  if self.wave == 0 then self.nextIn = math.min(self.nextIn, sim.FIRST_WAVE_DELAY) end
  return true
end

-- Upgrades the tower on `pad` one level.
function Sim:upgrade(pad)
  if not self:playing() then return false end
  local cost = self:upgradeCost(pad)
  if not cost then return false end
  if self.coins < cost then
    self:cue({ kind = "denied" })
    return false
  end
  self.coins -= cost
  local t = self.towers[pad]
  t.level += 1
  t.age = 0
  self.upgrades += 1
  self:cue({ kind = "upgraded", pad = pad, level = t.level })
  return true
end

-- Coins for calling the next wave now.
function Sim:earlyBonus()
  if self.wave == 0 then return 0 end
  return math.floor(self.nextIn * sim.EARLY_BONUS_PER_SEC)
end

-- Starts the next wave now, for a bonus. Returns the bonus paid (nil if over).
function Sim:callWave()
  if not self:playing() then return nil end
  local bonus = self:earlyBonus()
  self.coins += bonus
  self:cue({ kind = "early", bonus = bonus })
  self:startWave()
  return bonus
end

function Sim:startWave()
  local plan = self.next
  self.wave = plan.n
  local w = self.wave - 1
  self.hpMult = sim.HP_BASE * sim.HP_GROWTH ^ w * (if self.wave == 1 then sim.FIRST_WAVE_HP else 1)
  self.speedMult = 1 + math.min(sim.SPEED_GROWTH * w, sim.SPEED_GROWTH_MAX)
  -- Interleave the groups so a wave reads as a mixed parade, after
  -- whatever of the last wave is still to come.
  local left = {}
  for i, g in plan.groups do left[i] = { g[1], g[2] } end
  local last = self.spawns[#self.spawns]
  local base = if last then math.max(last[1], 0) else 0
  local function anyLeft()
    for _, g in left do
      if g[2] > 0 then return true end
    end
    return false
  end
  local at = 0
  while anyLeft() do
    for _, g in left do
      if g[2] > 0 then
        self.spawns[#self.spawns + 1] = { base + at, g[1] }
        at += sim.CREEPS[g[1]].spacing * self:range(0.85, 1.15) * (if plan.rush then sim.RUSH_SPACING else 1)
        g[2] -= 1
      end
    end
  end
  local pay = math.floor(sim.PAYDAY + sim.PAYDAY_GROWTH * self.wave)
  self.coins += pay
  self:cue({ kind = "payday", coins = pay })
  self:cue({ kind = "wave", n = self.wave, boss = plan.boss, rush = plan.rush, newKind = plan.newKind })
  self.next = sim.planWave(self.wave + 1, self.rng)
  self.nextIn = sim.WAVE_GAP
end

function Sim:spawn(kind, d)
  local s = sim.CREEPS[kind]
  local hp = s.hp * (if kind == "mini" then math.sqrt(self.hpMult) else self.hpMult)
  local x, y = pointAt(d)
  self.creeps[#self.creeps + 1] = {
    id = self.nextId,
    kind = kind,
    d = d,
    x = x,
    y = y,
    hp = hp,
    maxHp = hp,
    speed = s.speed * self.speedMult * self:range(0.96, 1.04),
    slow = 0, -- seconds of slow left
    slowK = 1, -- the speed multiplier while slowed
    flash = 0, -- hit flash, 1 → 0 (drawing)
    age = 0,
    gone = false, -- popped or at the cake (a POP shot stops homing)
  }
  self.nextId += 1
end

function sim.curSpeed(c)
  return c.speed * (if c.slow > 0 then c.slowK else 1)
end
local curSpeed = sim.curSpeed

-- Hits jelly `c` for `dmg` (armor applies). It pops in `reap`.
local function hurt(c, dmg)
  c.hp -= math.max(dmg - sim.CREEPS[c.kind].armor, dmg * 0.25)
  c.flash = 1
end

-- Removes jellies at 0 hp: coins, score, cues, splitting.
function Sim:reap()
  local i = 1
  while i <= #self.creeps do
    local c = self.creeps[i]
    if c.hp > 0 then
      i += 1
    else
      table.remove(self.creeps, i)
      c.gone = true
      local coins = sim.CREEPS[c.kind].bounty
      self.coins += coins
      self.score += 1
      self.chain = if self.chainT > 0 then self.chain + 1 else 1
      self.chainT = 0.6
      self:cue({ kind = "pop", x = c.x, y = c.y, creep = c.kind, coins = coins, chain = self.chain })
      if c.kind == "splitter" then
        self:cue({ kind = "split", x = c.x, y = c.y })
        for k = 0, sim.SPLIT_INTO - 1 do
          self:spawn("mini", math.max(c.d + (k - 1) * 10, 0))
        end
      end
    end
  end
end

function Sim:step(dt)
  if self.over then return end
  self.t += dt
  self.chainT -= dt
  local creeps = self.creeps
  if self.ending then
    -- The devouring: every jelly rushes the cake.
    self.ending += dt
    local i = 1
    while i <= #creeps do
      local c = creeps[i]
      c.d += c.speed * 6 * dt
      c.age += dt
      c.x, c.y = pointAt(c.d)
      if c.d >= sim.ROAD_LEN then
        self:cue({ kind = "chomp", x = c.x, y = c.y })
        table.remove(creeps, i)
      else
        i += 1
      end
    end
    table.clear(self.shots)
    if self.ending >= sim.ENDING then
      self.over = true
      self:cue({ kind = "over" })
    end
    return
  end

  -- Waves.
  self.nextIn -= dt
  if self.nextIn <= 0 then self:startWave() end
  local k = 1
  while k <= #self.spawns do
    local sp = self.spawns[k]
    sp[1] -= dt
    if sp[1] <= 0 then
      table.remove(self.spawns, k)
      self:spawn(sp[2], 0)
    else
      k += 1
    end
  end

  -- Jellies walk; the ones that reach the cake bite.
  for _, c in creeps do
    c.d += curSpeed(c) * dt
    c.slow = math.max(c.slow - dt, 0)
    c.flash = math.max(c.flash - dt * 6, 0)
    c.age += dt
    c.x, c.y = pointAt(c.d)
  end
  local i = 1
  while i <= #creeps do
    local c = creeps[i]
    if c.d >= sim.ROAD_LEN then
      table.remove(creeps, i)
      c.gone = true
      local bite = if c.kind == "king" then sim.KING_BITE else 1
      self.hearts = math.max(self.hearts - bite, 0)
      self:cue({ kind = "bite", x = c.x, y = c.y, hearts = self.hearts, creep = c.kind })
      if self.hearts == 0 then
        self.killer = c.kind
        self.ending = 0
        self:cue({ kind = "ending" })
        return
      end
    else
      i += 1
    end
  end

  -- Towers.
  for p, pad in sim.PADS do
    local t = self.towers[p]
    if t then
      t.age += dt
      t.recoil = math.max(t.recoil - dt * 5, 0)
      t.cd -= dt
      if t.cd <= 0 then
        local px, py = pad[1], pad[2]
        local spec = sim.TOWERS[t.kind]
        local lv = t.level
        local range = spec.range[lv]
        -- Target: the jelly furthest along the road that's in range.
        local best
        for _, c in creeps do
          if dist(c.x, c.y, px, py) <= range and (best == nil or c.d > best.d) then best = c end
        end
        if best then
          t.cd = spec.interval[lv]
          t.recoil = 1
          t.aim = math.atan2(best.y - py, best.x - px)
          if t.kind == "pop" then
            self.shots[#self.shots + 1] = { kind = "pop", x = px, y = py - 10, sx = px, sy = py - 10, tx = best.x, ty = best.y, target = best, damage = spec.damage[lv], splash = 0, t = 0 }
            self:cue({ kind = "fire", tower = "pop", x = px, y = py })
          elseif t.kind == "boom" then
            -- Lob to where the jelly will be when the bomb lands.
            local lx, ly = pointAt(best.d + curSpeed(best) * sim.BOOM_FLIGHT)
            self.shots[#self.shots + 1] = { kind = "boom", x = px, y = py, sx = px, sy = py - 8, tx = lx, ty = ly, damage = spec.damage[lv], splash = spec.special[lv], t = 0 }
            self:cue({ kind = "fire", tower = "boom", x = px, y = py })
          else
            local dmg, slowK = spec.damage[lv], spec.special[lv]
            for _, c in creeps do
              if dist(c.x, c.y, px, py) <= range then
                c.slowK = if c.slow > 0 then math.min(c.slowK, slowK) else slowK
                c.slow = sim.SLOW_TIME
                hurt(c, dmg)
              end
            end
            self:cue({ kind = "frost", x = px, y = py, r = range })
          end
        end
      end
    end
  end

  -- Shots.
  local s = 1
  local shots = self.shots
  while s <= #shots do
    local sh = shots[s]
    sh.t += dt
    local done = false
    if sh.kind == "pop" then
      -- Home on the target while it lives; else fly to its last spot.
      local target = sh.target
      if target and target.gone then target = nil end
      local tx, ty = sh.tx, sh.ty
      if target then tx, ty = target.x, target.y end
      local dx, dy = tx - sh.x, ty - sh.y
      local d = math.sqrt(dx * dx + dy * dy)
      local stepLen = sim.POP_SHOT_SPEED * dt
      if d <= math.max(stepLen, 8) then
        done = true
        if target then
          hurt(target, sh.damage)
          self:cue({ kind = "hit", x = tx, y = ty })
        end
      else
        sh.x += dx / d * stepLen
        sh.y += dy / d * stepLen
        sh.tx, sh.ty = tx, ty
      end
      if sh.t > 1.5 then done = true end
    else
      local kk = math.min(sh.t / sim.BOOM_FLIGHT, 1)
      sh.x = sh.sx + (sh.tx - sh.sx) * kk
      sh.y = sh.sy + (sh.ty - sh.sy) * kk
      if kk >= 1 then
        done = true
        for _, c in creeps do
          if dist(c.x, c.y, sh.tx, sh.ty) <= sh.splash + sim.CREEPS[c.kind].radius * 0.5 then hurt(c, sh.damage) end
        end
        self:cue({ kind = "boom", x = sh.tx, y = sh.ty, r = sh.splash })
      end
    end
    if done then
      table.remove(shots, s)
    else
      s += 1
    end
  end
  self:reap()
end

-- 0 (calm) … 1 (a jelly at the cake, or one heart left): drives the urgent
-- music, the cake's face and the warning vignette.
function Sim:danger()
  local near = 0
  local king = false
  for _, c in self.creeps do
    near = math.max(near, c.d / sim.ROAD_LEN)
    if c.kind == "king" then king = true end
  end
  local road = math.clamp((near - 0.6) / 0.4, 0, 1)
  local hearts = 1 - (self.hearts - 1) / (sim.HEARTS - 1)
  return math.max(road, hearts * 0.85, if king then 0.7 else 0)
end

-- Whether a king is on the road.
function Sim:king()
  for _, c in self.creeps do
    if c.kind == "king" then return true end
  end
  return false
end

-- Jellies still to come in waves already started.
function Sim:pendingSpawns()
  return #self.spawns
end

-- BOOM's height above the ground along its arc (for drawing).
function sim.lift(sh)
  if sh.kind ~= "boom" then return 0 end
  local k = math.clamp(sh.t / sim.BOOM_FLIGHT, 0, 1)
  return 4 * k * (1 - k) * 70
end

return sim
