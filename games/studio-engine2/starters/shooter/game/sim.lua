-- The rules: the ship and its auto-fire, enemies and their bullets, the
-- waves and the boss, power-ups, hits, scoring and the combo. Nothing here
-- draws or plays a sound: the sim pushes cues (`self.cues`) that main.lua
-- turns into juice and sound.lua into sound, and draw.lua reads its state.
-- All its randomness is its own two generators, seeded by the round.

local waves = require("waves")

local sim = {}

-- ---- Tuning knobs -------------------------------------------------------
-- Thumb-to-ship gain: the ship moves this many units per unit of thumb
-- travel. Above 1 crosses the screen with a shorter drag; much higher feels twitchy.
sim.DRAG_GAIN = 1.25
-- How fast the ship catches up with where the thumb puts it (1/s). Lower feels floaty.
sim.FOLLOW_RATE = 26.0
-- The ship keeps at least this far above the thumb (units), easing up, so the thumb never covers it.
sim.MIN_LIFT = 88.0
-- The ship's speed steered by the arrows, WASD, the d-pad or the stick (units/s).
sim.KEY_SPEED = 330.0
-- Seconds between auto-fire volleys. Lower = more damage (and more shots on screen).
sim.FIRE_INTERVAL = 0.11
-- Hits the ship can take: the HUD's hearts (studio.toml's `lives` says the same).
sim.LIVES = 3
-- Seconds of blinking invulnerability after a hit.
sim.INVULN_TIME = 1.8
-- Opening seconds with no enemy fire: the first seconds can't be failed.
sim.GRACE = 4.0
-- Difficulty steps up every this many seconds: faster, denser bullets and waves.
sim.HEAT_STEP = 11.0
-- What each step adds to the heat multiplier (1.0 at the start; capped at HEAT_MAX).
sim.HEAT_GAIN = 0.14
sim.HEAT_MAX = 2.2
-- Seconds between waves at heat 1 (divided by heat^0.8 later).
sim.WAVE_GAP = 2.7
-- Seconds into each loop when the boss arrives (its WARNING shows WARNING_TIME before).
sim.BOSS_AT = 48.0
sim.WARNING_TIME = 2.5
-- The first boss's health in shots; each later boss has 50 % more.
sim.BOSS_HP = 140.0
-- Base speed of enemy bullets (units/s) at heat 1. The single biggest difficulty lever.
sim.BULLET_SPEED = 185.0
-- Seconds a kill keeps the combo alive. Longer = easier multipliers.
sim.COMBO_WINDOW = 1.6
-- A power-up drops at least once every this many kills.
sim.POWER_EVERY = 20
-- Seconds a Spread or Magnet power-up lasts.
sim.POWER_TIME = 10.0
-- Ship hit radius (units). Smaller is more forgiving; the art is ~2.5× bigger.
sim.SHIP_R = 7.0
-- Bullets passing this close (units, edge to edge) without hitting count as a graze: bonus points.
sim.GRAZE = 14.0
-- Jellies start firing (single slow aimed shots) after this many seconds.
sim.JELLY_FIRE_AT = 7.0
-- Seconds of the slow-motion death before the game-over card.
sim.DEATH_TIME = 1.7
-- -------------------------------------------------------------------------

-- Geometry.
local W, H = 360, 640
sim.W, sim.H = W, H
-- Where the ship may go: below the HUD, above the thumb.
sim.SHIP_X = { 18, W - 18 }
sim.SHIP_Y = { 200, 560 }
sim.SHIP_START = { 180, 470 }
local DEMO_Y_MAX = 420
local SHOT_SPEED = 780
-- Enemies only fire from above this line, and only this far above the ship.
local FIRE_LINE, FIRE_CLEARANCE = 430, 130
-- A telegraph glow shows this long before an enemy fires.
sim.TELL = 0.35
local DEATH_SLOWMO = 0.3
local GEM_VALUE, GRAZE_VALUE = 25, 10

-- Boss attack timing: charge (the telegraph), fire, rest.
sim.CHARGE = 0.8
sim.ATTACK_FIRE = 4.6
local ATTACK_REST = 0.9
sim.BOSS_DEATH = 1.6
-- The boss's hit ellipse (half-width, half-height) and where it hovers.
sim.BOSS_RX, sim.BOSS_RY = 62.0, 44.0
local BOSS_Y = 196.0
-- Where the boss's hands are, from its centre (the Rain fires from them).
sim.BOSS_HAND = { 84.0, 18.0 }
local NEXT_ATTACK = { bloom = "rain", rain = "sweep", sweep = "bloom" }

local sin, cos, sqrt, abs, exp, floor, min, max, pi = math.sin, math.cos, math.sqrt, math.abs, math.exp, math.floor, math.min, math.max, math.pi
local atan2, clamp = math.atan2, math.clamp
local TAU = 2 * pi

-- ---- The cast ---------------------------------------------------------------
-- Each enemy kind: hit points (loop 1), body radius, points a kill (before the
-- combo multiplier), gems it drops, seconds after arriving before its first
-- shot, and seconds between shots at heat 1.
sim.KINDS = {
  -- Small, slow, comes in formations. The bread and butter.
  jelly = { hp = 1, radius = 15, points = 100, gems = 1, first = 2.6, every = 4.0, index = 1 },
  -- Crosses the screen bobbing, fires aimed shots.
  swooper = { hp = 2, radius = 15, points = 150, gems = 1, first = 0.8, every = 1.6, index = 2 },
  -- Stops, aims at the ship (a line shows where), then dashes. Never fires.
  darter = { hp = 3, radius = 14, points = 250, gems = 2, first = 99, every = 99, index = 3 },
  -- Big and tough; hovers and fires fans of big bullets.
  bulb = { hp = 14, radius = 26, points = 600, gems = 5, first = 1.1, every = 1.7, index = 4 },
  -- Hovers and sprays a spiral.
  spinner = { hp = 10, radius = 22, points = 500, gems = 4, first = 1.2, every = 0.12, index = 5 },
}
local KINDS = sim.KINDS

local Sim = {}
Sim.__index = Sim
sim.Sim = Sim

-- A number from a to b (or 0–1) from generator `gen`.
local function ranger(gen)
  return function(a: number?, b: number?): number
    local r = gen:random()
    if a and b then return a + (b - a) * r end
    return r
  end
end

-- A fresh round from `seed`. `demo`: the title card's, where hits only flash
-- and nothing scores.
function Sim.new(seed, demo)
  local self = setmetatable({}, Sim)
  -- Waves come from their own generator, so a daily seed gives everyone
  -- the same waves whatever they do; everything else from the other.
  self.waveRng = ranger(love.math.newRandomGenerator(seed))
  self.rng = ranger(love.math.newRandomGenerator(seed + 7919))
  self.demo = demo == true
  self.t = 0 -- seconds of play (real time, including the slow-motion death)
  self.phase = 0 -- seconds into the current loop (the boss comes at BOSS_AT)
  self.loop = 0 -- bosses beaten
  self.ship = {
    x = sim.SHIP_START[1], y = sim.SHIP_START[2], tx = sim.SHIP_START[1], ty = sim.SHIP_START[2], vx = 0,
    hp = sim.LIVES, invuln = 0, shield = false, spread = 0, spreadT = 0, magnetT = 0,
    fireCd = 0.05, side = 1, danger = 0, happy = 0, spin = 0,
  }
  self.grab = nil -- the thumb's drag: { fx, fy (where it touched down), sx, sy (the ship's goal then), x, y }
  self.shots, self.enemies, self.bullets, self.drops = {}, {}, {}, {}
  self.boss = nil
  self.banner = nil -- { kind = "warning" | "new" | "clear", enemy, age }
  self.score, self.kills, self.combo, self.comboLeft, self.bestCombo, self.grazes = 0, 0, 0, 0, 0, 0
  self.dying = nil -- seconds since the slow-motion death began
  self.killer = nil -- where the fatal hit landed: { x, y, by }
  self.lastHit = "bullet" -- what the last hit was (for the report)
  self.over = false
  self.cues = {}
  self.pending = {}
  self.nextWave, self.opening, self.introsDone = 0, 1, 1
  self.lastNasty, self.warned = false, false
  self.seen = {}
  self.killsSincePower, self.lastPower = 0, nil
  self.gemStreak, self.gemStreakT, self.deathBooms = 0, 0, 0
  return self
end

function Sim:cue(c)
  self.cues[#self.cues + 1] = c
end

-- ---- Input --------------------------------------------------------------------

-- The thumb touches down anywhere: steering starts from here. The ship
-- doesn't jump to the thumb; it moves by how far the thumb moves.
function Sim:touchDown(x, y)
  if self.dying or self.over then return end
  self.grab = { fx = x, fy = y, sx = self.ship.tx, sy = self.ship.ty, x = x, y = y }
end

function Sim:touchMove(x, y)
  if self.grab then self.grab.x, self.grab.y = x, y end
end

function Sim:touchUp()
  self.grab = nil
end

-- Where the thumb would have to be for the ship to head to (x, y), if it's
-- touching (the autopilot and the bot steer with this).
function Sim:fingerFor(x, y)
  local g = self.grab
  if not g then return nil end
  return g.fx + (x - g.sx) / sim.DRAG_GAIN, g.fy + (y - g.sy) / sim.DRAG_GAIN
end

-- Keys and pads: the arrows, WASD, the d-pad or the stick move the ship's
-- goal at KEY_SPEED (input.ax, input.ay: −1…1). A thumb on the glass wins.
function Sim:nudge(ax, ay, dt)
  if self.grab or not self:alive() or (ax == 0 and ay == 0) then return end
  local s = self.ship
  s.tx = clamp(s.tx + ax * sim.KEY_SPEED * dt, sim.SHIP_X[1], sim.SHIP_X[2])
  s.ty = clamp(s.ty + ay * sim.KEY_SPEED * dt, sim.SHIP_Y[1], sim.SHIP_Y[2])
end

-- ---- Derived state -------------------------------------------------------------

-- Difficulty multiplier: 1 at the start, +HEAT_GAIN every HEAT_STEP seconds.
function Sim:heat()
  return min(1 + sim.HEAT_GAIN * floor(self.t / sim.HEAT_STEP) + 0.15 * self.loop, sim.HEAT_MAX)
end

-- Score multiplier from the combo: ×1, ×2 at 5 kills, ×3 at 10… up to ×8.
function Sim:multiplier()
  return min(1 + floor(self.combo / 5), 8)
end

-- 0…1: how tense it is (the boss, bullets on screen, the last heart, the heat).
function Sim:intensity()
  if self.dying or self.over then return 0 end
  local boss = if self.boss or (self.warned and self.phase > sim.BOSS_AT - sim.WARNING_TIME) then 1 else 0
  local bullets = min(#self.bullets / 30, 1) * 0.6
  local low = if self.ship.hp == 1 then 0.7 else 0
  local heat = clamp((self:heat() - 1) / 0.6, 0, 0.5)
  return max(boss, bullets, low, heat)
end

function Sim:alive()
  return not self.dying and not self.over
end

-- The boss is on screen and fighting (not arriving, not dying).
function Sim:bossLive()
  local b = self.boss
  return b ~= nil and b.dying == nil
end

-- ---- The step ------------------------------------------------------------------

function Sim:step(dt)
  if self.over then
    -- The aftermath under the game-over card: the world drifts on, quietly,
    -- and Pip's wreck tumbles.
    self.ship.spin += dt * 1.1
    self:moveWorld(dt, false)
    self:bannerAge(dt)
    return
  end
  self.t += dt
  local wdt = dt
  if self.dying then
    self.dying += dt
    wdt = dt * DEATH_SLOWMO
    self:dyingStep(dt)
  else
    self:steer(dt)
    self:fire(dt)
  end
  self:schedule(wdt)
  self:moveWorld(wdt, self:alive())
  self:bossStep(wdt)
  self:collideShots()
  if self:alive() then
    self:collideShip()
    self:pickups(wdt)
  end
  self:timers(wdt)
  self:bannerAge(dt)
end

function Sim:bannerAge(dt)
  local b = self.banner
  if b then
    b.age += dt
    if b.age > 2.2 then self.banner = nil end
  end
end

function Sim:show(kind, enemy)
  self.banner = { kind = kind, enemy = enemy, age = 0 }
end

function Sim:steer(dt)
  local s, g = self.ship, self.grab
  if g then
    local tx = g.sx + (g.x - g.fx) * sim.DRAG_GAIN
    local ty = g.sy + (g.y - g.fy) * sim.DRAG_GAIN
    -- Ease the ship up when the thumb is right under it.
    local gap = g.y - ty
    if gap < sim.MIN_LIFT and abs(g.x - tx) < 70 then
      local lift = (sim.MIN_LIFT - gap) * (1 - exp(-8 * dt))
      g.sy -= lift
      ty -= lift
    end
    -- At an edge, move the grab with the thumb so coming back responds at once.
    local cx = clamp(tx, sim.SHIP_X[1], sim.SHIP_X[2])
    -- On the title card the autopilot stays clear of the card's words.
    local cy = clamp(ty, sim.SHIP_Y[1], if self.demo then DEMO_Y_MAX else sim.SHIP_Y[2])
    g.sx += cx - tx
    g.sy += cy - ty
    s.tx, s.ty = cx, cy
  end
  local k = 1 - exp(-sim.FOLLOW_RATE * dt)
  local nx = s.x + (s.tx - s.x) * k
  s.vx = if dt > 0 then (nx - s.x) / dt else 0
  s.x = nx
  s.y += (s.ty - s.y) * k
end

local SPREADS = { [0] = { 0 }, [1] = { -0.17, 0, 0.17 }, [2] = { -0.32, -0.16, 0, 0.16, 0.32 } }

function Sim:fire(dt)
  local s = self.ship
  s.fireCd -= dt
  while s.fireCd <= 0 do
    s.fireCd += sim.FIRE_INTERVAL
    s.side = -s.side
    local ox = if s.spread == 0 then s.side * 5 else 0
    for _, a in SPREADS[s.spread] do
      self.shots[#self.shots + 1] = { x = s.x + ox + a * 30, y = s.y - 18, vx = sin(a) * SHOT_SPEED, vy = -cos(a) * SHOT_SPEED }
    end
    self:cue({ type = "shot" })
  end
end

-- Waves, intros, the boss's warning and arrival.
function Sim:schedule(dt)
  self.phase += dt
  -- Pending spawns from waves already started.
  local hpK = 1 + 0.3 * self.loop
  local i = 1
  while i <= #self.pending do
    local p = self.pending[i]
    p.delay -= dt
    if p.delay <= 0 then
      table.remove(self.pending, i)
      self:addEnemy(p, hpK)
    else
      i += 1
    end
  end
  if not self:alive() then return end
  -- The boss.
  if not self.boss and not self.warned and self.phase >= sim.BOSS_AT - sim.WARNING_TIME then
    self.warned = true
    self:show("warning")
    self:cue({ type = "warning" })
  end
  if not self.boss and self.warned and self.phase >= sim.BOSS_AT then
    local hp = sim.BOSS_HP * (1 + 0.5 * self.loop)
    self.boss = { x = W / 2, y = -90, hp = hp, maxHp = hp, age = 0, attack = "bloom", attackT = 0, shotT = 0,
                  spin = 0, flash = 0, dying = nil, booms = 0, hand = false }
    self:cue({ type = "bossArrive" })
  end
  if self.warned then
    -- Waves pause from the warning until the boss is beaten, except a few
    -- jellies now and then to keep the combo going.
    local b = self.boss
    if b and not self:bossEntering() and not b.dying and self.phase >= self.nextWave then
      self.nextWave = self.phase + 7
      self:startWave("row")
    end
    return
  end
  -- The opening (fixed), then the intros of new enemy types, then random waves.
  if self.loop == 0 and self.opening <= #waves.OPENING then
    local o = waves.OPENING[self.opening]
    if self.phase >= o[1] then
      self.opening += 1
      self:startWave(o[2])
      self.nextWave = self.phase + sim.WAVE_GAP
    end
    return
  end
  local intro = waves.INTROS[self.introsDone]
  if intro and self.loop >= intro.loop and (self.loop > intro.loop or self.phase >= intro.at) then
    self.introsDone += 1
    if not self.seen[intro.kind] then
      self:startWave(intro.formation)
      self.nextWave = self.phase + sim.WAVE_GAP * 1.2
      return
    end
  end
  if self.phase >= self.nextWave then
    local pool = waves.pool(self.loop, self.phase)
    local f = pool[1 + floor(self.waveRng() * #pool)]
    if self.lastNasty and waves.NASTY[f] then f = "row" end
    self.lastNasty = waves.NASTY[f] == true
    self:startWave(f)
    local gap = max(sim.WAVE_GAP / self:heat() ^ 0.8, 1.5)
    if f == "bulb" or f == "bulbPair" or f == "spinner" then gap *= 1.4 end
    self.nextWave = self.phase + gap
  end
end

function Sim:startWave(f)
  for _, s in waves.build(f, self.waveRng) do
    if not self.seen[s.kind] then
      self.seen[s.kind] = true
      if s.kind ~= "jelly" then
        self:cue({ type = "newEnemy", kind = s.kind })
        self:show("new", s.kind)
      end
    end
    self.pending[#self.pending + 1] = s
  end
end

function Sim:addEnemy(s, hpK)
  local k = KINDS[s.kind]
  local hp = max(floor(k.hp * hpK + 0.5), 1)
  self.enemies[#self.enemies + 1] = {
    kind = s.kind, x = s.x, y = s.y, vx = 0, vy = 0, x0 = s.x, y0 = s.y, age = 0, hp = hp, maxHp = hp,
    path = s.path, fireCd = k.first * self.rng(0.8, 1.25), flash = 0,
    -- Darters: 0 arriving, 1 aiming, 2 locked (the line flashes), 3 dashing.
    stage = 0, ax = s.x, ay = H,
    spin = self.rng(0, TAU), -- spinners: the spray's angle
  }
end

-- True while enemy `e` may fire now and its telegraph should show.
function sim.telegraph(e)
  return e.fireCd < sim.TELL and e.kind ~= "darter"
end

function Sim:moveWorld(dt, canFire)
  local heat = self:heat()
  local speedK = sqrt(heat)
  local sx, sy = self.ship.x, self.ship.y
  local fired = nil
  for _, e in self.enemies do
    local px, py = e.x, e.y
    e.age += dt
    e.flash = max(e.flash - dt * 6, 0)
    local p = e.path
    if p.type == "drift" then
      e.y += p.vy * speedK * dt
      e.x = e.x0 + p.sway * sin(e.age * 1.7 + e.x0 * 0.05)
    elseif p.type == "swoop" then
      e.x = e.x0 + p.vx * speedK * e.age
      e.y = e.y0 + p.vy * e.age + p.amp * sin(e.age * p.freq)
    elseif p.type == "hover" then
      if e.age < p.hold + 1.4 then
        e.y += (p.stopY - e.y) * (1 - exp(-2.6 * dt))
        e.x = e.x0 + 22 * sin(e.age * 0.9)
      else
        -- Done: leave upward, out of the way.
        e.y -= 70 * min(e.age - p.hold - 1.4, 1.5) * dt * 2
        e.fireCd = max(e.fireCd, 1)
      end
    else -- dive
      if e.stage == 0 then
        e.y += (p.stopY - e.y) * (1 - exp(-3 * dt))
        if e.age > 1.1 then e.stage, e.age = 1, 0 end
      elseif e.stage == 1 then
        e.ax, e.ay = sx, sy
        if e.age > 0.55 then
          e.stage, e.age = 2, 0
          self:cue({ type = "darterLock", x = e.x, y = e.y })
        end
      elseif e.stage == 2 then
        if e.age > 0.4 then
          e.stage, e.age = 3, 0
          local dx, dy = e.ax - e.x, e.ay - e.y
          local d = max(sqrt(dx * dx + dy * dy), 1)
          local v = 460 * speedK
          e.vx, e.vy = dx / d * v, dy / d * v
        end
      else
        e.x += e.vx * dt
        e.y += e.vy * dt
      end
    end
    if not (p.type == "dive" and e.stage == 3) and dt > 0 then
      e.vx, e.vy = (e.x - px) / dt, (e.y - py) / dt
    end
    -- Firing, with a telegraph glow for TELL seconds first.
    local k = KINDS[e.kind]
    local may = canFire and e.kind ~= "darter" and e.y > 10 and e.y < min(FIRE_LINE, sy - FIRE_CLEARANCE) and e.x > 8 and e.x < W - 8
    local jellyOk = e.kind ~= "jelly" or self.t > sim.JELLY_FIRE_AT or self.loop > 0
    if may and jellyOk and self.t > sim.GRACE then
      e.fireCd -= dt
      if e.fireCd <= 0 then
        fired = fired or {}
        fired[#fired + 1] = { e.x, e.y, e.kind, e.spin }
        e.fireCd = k.every / heat ^ 1.4
        if e.kind == "spinner" then
          e.spin += 0.42
          -- Bursts of 14 shots, then a breather.
          if floor(e.spin / 0.42) % 14 == 13 then e.fireCd = 1.1 / heat end
        end
      end
    end
  end
  if fired then
    for _, f in fired do self:enemyFire(f[1], f[2], f[3], f[4]) end
  end
  -- Gone off screen: dropped (in order, so the drawing order holds).
  local list, n = self.enemies, 0
  for i = 1, #list do
    local e = list[i]
    if e.y < H + 50 and e.y > -80 and e.x > -60 and e.x < W + 60 then
      n += 1
      list[n] = e
    end
  end
  for i = #list, n + 1, -1 do list[i] = nil end
  list, n = self.shots, 0
  for i = 1, #list do
    local s = list[i]
    s.x += s.vx * dt
    s.y += s.vy * dt
    if s.y > -20 and s.x > -20 and s.x < W + 20 then
      n += 1
      list[n] = s
    end
  end
  for i = #list, n + 1, -1 do list[i] = nil end
  list, n = self.bullets, 0
  for i = 1, #list do
    local b = list[i]
    b.x += b.vx * dt
    b.y += b.vy * dt
    b.age += dt
    if b.y < H + 20 and b.y > -40 and b.x > -20 and b.x < W + 20 then
      n += 1
      list[n] = b
    end
  end
  for i = #list, n + 1, -1 do list[i] = nil end
  local magnet = self.ship.magnetT > 0 and self:alive()
  local kv, kg, kf = exp(-3 * dt), 1 - exp(-8 * dt), 1 - exp(-2.5 * dt)
  list, n = self.drops, 0
  for i = 1, #list do
    local d = list[i]
    d.age += dt
    local dx, dy = sx - d.x, sy - d.y
    local dist = max(sqrt(dx * dx + dy * dy), 1)
    if magnet and dist < 170 then
      d.vx += (dx / dist * 620 - d.vx) * kg
      d.vy += (dy / dist * 620 - d.vy) * kg
    else
      -- Pop out, then sink gently.
      local fall = if d.kind == "gem" then 70 else 55
      d.vx *= kv
      d.vy += (fall - d.vy) * kf
    end
    d.x = clamp(d.x + d.vx * dt, 10, W - 10)
    d.y += d.vy * dt
    if d.y < H + 20 then
      n += 1
      list[n] = d
    end
  end
  for i = #list, n + 1, -1 do list[i] = nil end
end

function Sim:aimed(x, y)
  return atan2(self.ship.y - y, self.ship.x - x)
end

-- An enemy bullet: big and bright in the art; `r` is its hit radius.
function Sim:bullet(x, y, angle, speed, big)
  self.bullets[#self.bullets + 1] = { x = x, y = y, vx = cos(angle) * speed, vy = sin(angle) * speed,
                                      r = if big then 8 else 6, big = big, grazed = false, age = 0 }
end

function Sim:enemyFire(x, y, kind, spin)
  local v = sim.BULLET_SPEED * (0.75 + 0.25 * self:heat())
  local a = self:aimed(x, y)
  if kind == "jelly" then
    self:bullet(x, y + 10, a, v * 0.8, false)
  elseif kind == "swooper" then
    self:bullet(x, y + 10, a, v, false)
  elseif kind == "bulb" then
    local n = if self.loop == 0 then 3 else 5
    for i = 0, n - 1 do
      self:bullet(x, y + 18, a + (i - (n - 1) / 2) * 0.24, v * 0.85, true)
    end
  elseif kind == "spinner" then
    for arm = 0, 1 do self:bullet(x, y, spin + arm * pi, v * 0.8, false) end
  end
  self:cue({ type = "enemyFire", big = kind == "bulb" or kind == "spinner" })
end

function Sim:bossEntering()
  return self.boss ~= nil and self.boss.age < 2.4
end

-- 0…1 while the boss charges up her current attack (the telegraph).
function sim.charge(b)
  if b.age < 2.4 or b.dying then return 0 end
  if b.attackT >= sim.CHARGE then return 0 end
  return min(b.attackT / sim.CHARGE, 1)
end

-- Below 40 % health she attacks faster.
function sim.enraged(b)
  return b.hp < b.maxHp * 0.4
end

function Sim:bossStep(dt)
  local b = self.boss
  if not b then return end
  b.age += dt
  b.flash = max(b.flash - dt * 6, 0)
  if b.dying then
    b.dying += dt
    local d = b.dying
    b.x += sin(d * 40) * 2
    -- A chain of explosions across her body, then the big one.
    local due = min(floor(d / 0.18), 8)
    while b.booms < due do
      b.booms += 1
      self:cue({ type = "boom", x = b.x + self.rng(-sim.BOSS_RX, sim.BOSS_RX), y = b.y + self.rng(-sim.BOSS_RY, sim.BOSS_RY), size = 0.5 })
    end
    if d >= sim.BOSS_DEATH then
      self.boss = nil
      self:bossDown(b.x, b.y)
    end
    return
  end
  b.y += (BOSS_Y - b.y) * (1 - exp(-2.2 * dt))
  if b.age < 2.4 then return end
  local sway = if b.attack == "sweep" then 95 else 55
  b.x = W / 2 + sway * sin(b.age * 0.7)
  local speed = (if sim.enraged(b) then 1.35 else 1) * (1 + 0.2 * self.loop)
  local prev = b.attackT
  b.attackT += dt
  if prev == 0 then self:cue({ type = "bossCharge", attack = b.attack }) end
  if b.attackT > sim.CHARGE and b.attackT < sim.CHARGE + sim.ATTACK_FIRE and self:alive() then
    b.shotT -= dt * speed
    local v = sim.BULLET_SPEED * (0.75 + 0.25 * self:heat())
    while b.shotT <= 0 do
      if b.attack == "bloom" then
        -- Rings of bullets bursting outward (none straight up).
        b.shotT += 0.95
        local n = 16 + 2 * min(self.loop, 4)
        b.spin += pi / n
        for i = 0, n - 1 do
          local a = b.spin + i * TAU / n
          if sin(a) > -0.5 then self:bullet(b.x, b.y + 10, a, v * 0.85, true) end
        end
      elseif b.attack == "rain" then
        -- Aimed fans from alternating hands.
        b.shotT += 0.5
        b.hand = not b.hand
        local hx = b.x + (if b.hand then sim.BOSS_HAND[1] else -sim.BOSS_HAND[1])
        local hy = b.y + sim.BOSS_HAND[2]
        local a = self:aimed(hx, hy)
        for k = -1, 1 do self:bullet(hx, hy, a + k * 0.2, v * 1.15, false) end
      else
        -- Sweep: a two-armed spiral while she sways.
        b.shotT += 0.1
        b.spin += 0.29
        for arm = 0, 1 do self:bullet(b.x, b.y + 10, b.spin + arm * pi, v * 0.8, false) end
      end
      self:cue({ type = "enemyFire", big = true })
    end
  end
  if b.attackT > sim.CHARGE + sim.ATTACK_FIRE + ATTACK_REST then
    b.attackT, b.shotT = 0, 0
    b.attack = NEXT_ATTACK[b.attack]
  end
end

function Sim:bossDown(x, y)
  local points = 5000 * (self.loop + 1)
  self:addScore(points)
  self:cue({ type = "bossDown", x = x, y = y, points = points })
  self:cue({ type = "boom", x = x, y = y, size = 1 })
  self.drops[#self.drops + 1] = { kind = "heart", x = x, y = y, vx = 0, vy = -60, age = 0 }
  for i = 0, 13 do
    local a = i / 14 * TAU
    self.drops[#self.drops + 1] = { kind = "gem", x = x, y = y, vx = cos(a) * 160, vy = sin(a) * 160 - 40, age = 0 }
  end
  self.loop += 1
  self.phase, self.nextWave = 0, 3
  self.warned, self.lastNasty = false, false
  self:show("clear")
end

function Sim:collideShots()
  local shots = self.shots
  local i = 1
  while i <= #shots do
    local s = shots[i]
    local hit = false
    local enemies = self.enemies
    for j = 1, #enemies do
      local e = enemies[j]
      local r = KINDS[e.kind].radius + 5
      local dx, dy = s.x - e.x, s.y - e.y
      if dx * dx + dy * dy < r * r and e.y > -10 then
        hit = true
        e.hp -= 1
        e.flash = 1
        if e.hp <= 0 then
          table.remove(enemies, j)
          self:kill(e)
        else
          self:cue({ type = "hit", x = s.x, y = s.y })
        end
        break
      end
    end
    local b = self.boss
    if not hit and b and not b.dying and b.y > 40 then
      local dx, dy = (s.x - b.x) / sim.BOSS_RX, (s.y - b.y) / sim.BOSS_RY
      if dx * dx + dy * dy < 1 then
        hit = true
        local calm = not sim.enraged(b)
        b.hp -= 1
        b.flash = 1
        self:cue({ type = "bossHit", x = s.x, y = s.y })
        if calm and sim.enraged(b) and b.hp > 0 then self:cue({ type = "bossEnrage" }) end
        if b.hp <= 0 then
          b.dying = 0
          self:cue({ type = "bossBeaten", x = b.x, y = b.y })
          -- Every bullet on screen turns into a gem, and every enemy pops.
          for _, bl in self.bullets do
            self.drops[#self.drops + 1] = { kind = "gem", x = bl.x, y = bl.y, vx = 0, vy = -30, age = 0 }
          end
          self.bullets = {}
          local all = self.enemies
          self.enemies = {}
          for _, e in all do self:kill(e) end
        end
      end
    end
    if hit then
      shots[i] = shots[#shots]
      shots[#shots] = nil
    else
      i += 1
    end
  end
end

function Sim:addScore(points)
  if not self.demo then self.score += points end
end

function Sim:kill(e)
  local k = KINDS[e.kind]
  self.kills += 1
  self.combo += 1
  self.comboLeft = sim.COMBO_WINDOW
  self.bestCombo = max(self.bestCombo, self.combo)
  local mult = self:multiplier()
  local points = k.points * mult
  self:addScore(points)
  self:cue({ type = "kill", x = e.x, y = e.y, kind = e.kind, points = points, mult = mult, combo = self.combo })
  for g = 0, k.gems - 1 do
    local a = -pi / 2 + (g - (k.gems - 1) / 2) * 0.7 + self.rng(-0.3, 0.3)
    local sp = self.rng(70, 130)
    self.drops[#self.drops + 1] = { kind = "gem", x = e.x, y = e.y, vx = cos(a) * sp, vy = sin(a) * sp, age = 0 }
  end
  self.killsSincePower += 1
  local bonus = e.kind == "bulb" and self.rng() < 0.6
  if self.killsSincePower >= sim.POWER_EVERY or bonus then
    self.killsSincePower = 0
    local choices = {}
    for _, p in { "spread", "shield", "magnet" } do
      if p ~= self.lastPower then choices[#choices + 1] = p end
    end
    local p = choices[1 + floor(self.rng() * #choices)]
    self.lastPower = p
    self.drops[#self.drops + 1] = { kind = "power", power = p, x = e.x, y = e.y, vx = 0, vy = -50, age = 0 }
  end
end

function Sim:collideShip()
  local s = self.ship
  if s.invuln > 0 then return end -- blinking: bullets pass through
  local sx, sy = s.x, s.y
  local hitX, hitY, by
  local list = self.bullets
  local hitAt = 0
  for i = 1, #list do
    local b = list[i]
    local dx, dy = b.x - sx, b.y - sy
    local d = sqrt(dx * dx + dy * dy)
    if d < b.r + sim.SHIP_R and not hitX then
      hitX, hitY, by, hitAt = b.x, b.y, "bullet", i
    elseif not b.grazed and d < b.r + sim.SHIP_R + sim.GRAZE then
      b.grazed = true
      self.grazes += 1
      self:addScore(GRAZE_VALUE)
      self:cue({ type = "graze", x = b.x, y = b.y })
    end
  end
  if hitAt > 0 then table.remove(list, hitAt) end
  -- Ramming an enemy hurts both (a darter dies of it).
  if not hitX then
    for i, e in self.enemies do
      local r = KINDS[e.kind].radius * 0.8 + sim.SHIP_R
      local dx, dy = e.x - sx, e.y - sy
      if dx * dx + dy * dy < r * r then
        hitX, hitY, by = e.x, e.y, "body"
        e.hp -= 3
        e.flash = 1
        if e.hp <= 0 then
          table.remove(self.enemies, i)
          self:kill(e)
        end
        break
      end
    end
  end
  local b = self.boss
  if not hitX and b and not b.dying then
    local dx, dy = (sx - b.x) / (sim.BOSS_RX + sim.SHIP_R), (sy - b.y) / (sim.BOSS_RY + sim.SHIP_R)
    if dx * dx + dy * dy < 1 then hitX, hitY, by = sx, sy - 10, "body" end
  end
  if hitX then self:hurt(hitX, hitY, by) end
end

-- Where the round is, for the difficulty report ("bullet in darters"): the
-- newest enemy type of loop 1, the boss and her attack ("boss rain"), or
-- loop 2 and later.
function Sim:stretch()
  if self.loop > 0 then return "loop 2+" end
  if self.boss then return "boss " .. self.boss.attack end
  if self.warned then return "boss" end
  if self.phase >= 35 then return "bulbs" end
  if self.phase >= 24 then return "darters" end
  if self.phase >= 13 then return "swoopers" end
  return "jellies"
end

-- The ship is hit by a bullet or a body (`by`). While `self.invincible` (the
-- debug menu's Invincible, which main.lua copies here) it only blinks.
function Sim:hurt(x, y, by)
  local s = self.ship
  -- Mercy: clear the bullets right around the ship.
  local list, n = self.bullets, 0
  for i = 1, #list do
    local b = list[i]
    if (b.x - s.x) ^ 2 + (b.y - s.y) ^ 2 > 80 * 80 then
      n += 1
      list[n] = b
    end
  end
  for i = #list, n + 1, -1 do list[i] = nil end
  if s.shield then
    s.shield, s.invuln = false, 1.0
    self:cue({ type = "shieldBreak", x = s.x, y = s.y })
    return
  end
  if self.combo >= 5 then self:cue({ type = "comboEnd", count = self.combo }) end
  self.combo, self.comboLeft = 0, 0
  if self.demo or self.invincible then
    s.invuln = sim.INVULN_TIME
    self:cue({ type = "hurt", x = x, y = y, hp = s.hp })
    return
  end
  s.hp = max(s.hp - 1, 0)
  s.spread, s.spreadT = 0, 0
  self.lastHit = by .. " in " .. self:stretch()
  self:cue({ type = "hurt", x = x, y = y, hp = s.hp })
  if s.hp == 0 then
    self.dying = 0
    self.killer = { x = x, y = y }
    self.grab = nil
    self:cue({ type = "dying", x = s.x, y = s.y })
  else
    s.invuln = sim.INVULN_TIME
  end
end

function Sim:dyingStep(dt)
  local s, d = self.ship, self.dying
  s.spin += dt * (4 + d * 6)
  s.y += 26 * dt
  -- Little pops on the ship, then the big one just before the card.
  local due = min(floor(d / 0.28), 5)
  while self.deathBooms < due do
    self.deathBooms += 1
    self:cue({ type = "boom", x = s.x + self.rng(-14, 14), y = s.y + self.rng(-14, 14), size = 0.35 })
  end
  if d >= sim.DEATH_TIME - 0.15 and self.deathBooms < 6 then
    self.deathBooms = 6
    self:cue({ type = "boom", x = s.x, y = s.y, size = 1 })
  end
  if d >= sim.DEATH_TIME then
    self.over = true
    self:cue({ type = "over" })
  end
end

function Sim:pickups(dt)
  local s = self.ship
  self.gemStreakT -= dt
  if self.gemStreakT <= 0 then self.gemStreak = 0 end
  local list = self.drops
  local i = 1
  while i <= #list do
    local d = list[i]
    local reach = if d.kind == "gem" then 26 else 30
    if d.age > 0.25 and (d.x - s.x) ^ 2 + (d.y - s.y) ^ 2 < reach * reach then
      list[i] = list[#list]
      list[#list] = nil
      self:collect(d)
    else
      i += 1
    end
  end
end

function Sim:collect(d)
  local s = self.ship
  if d.kind == "gem" then
    self.gemStreak += 1
    self.gemStreakT = 0.6
    self:addScore(GEM_VALUE)
    self:cue({ type = "gem", x = d.x, y = d.y, streak = self.gemStreak })
  elseif d.kind == "power" then
    s.happy = 1.2
    if d.power == "spread" then
      s.spread = min(s.spread + 1, 2)
      s.spreadT = sim.POWER_TIME
    elseif d.power == "shield" then
      s.shield = true
    else
      s.magnetT = sim.POWER_TIME
    end
    self:addScore(200)
    self:cue({ type = "power", x = d.x, y = d.y, power = d.power })
  else -- a heart
    s.happy = 1.5
    if s.hp < sim.LIVES then s.hp += 1 else self:addScore(1000) end
    self:cue({ type = "heart", x = d.x, y = d.y })
  end
end

function Sim:timers(dt)
  local s = self.ship
  s.invuln = max(s.invuln - dt, 0)
  s.happy = max(s.happy - dt, 0)
  if s.spreadT > 0 then
    s.spreadT -= dt
    if s.spreadT <= 0 then s.spread = 0 end
  end
  s.magnetT = max(s.magnetT - dt, 0)
  if self.combo > 0 then
    self.comboLeft -= dt
    if self.comboLeft <= 0 then
      if self.combo >= 5 then self:cue({ type = "comboEnd", count = self.combo }) end
      self.combo = 0
    end
  end
  -- How worried the face is: bullets close by.
  local near = 0
  for _, b in self.bullets do
    local dx, dy = b.x - s.x, b.y - s.y
    near += max(1 - sqrt(dx * dx + dy * dy) / 90, 0)
  end
  s.danger += (min(near, 1) - s.danger) * (1 - exp(-10 * dt))
end

-- ---- Debug ------------------------------------------------------------------------

-- Skips ahead to `phase` seconds into loop `loop` (scenarios: the boss, loop 2).
function Sim:skipTo(phase, loop)
  self.loop = loop or 0
  self.phase, self.t = phase, phase + 60 * self.loop
  self.opening = #waves.OPENING + 1
  local done = 1
  for _, intro in waves.INTROS do
    if intro.loop < self.loop or (intro.loop == self.loop and intro.at <= phase) then
      done += 1
      self.seen[intro.kind] = true
    end
  end
  self.introsDone = done
  self.seen.jelly = true
  self.nextWave = phase
end

return sim
