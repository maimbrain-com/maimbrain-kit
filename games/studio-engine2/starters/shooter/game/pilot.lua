-- How a player flies Pip: where to put the ship next. The title card's
-- attract mode and the debug Autopilot fly with it (main.lua), and the
-- difficulty bot (game/bot.lua) asks it where the thumb should go.
--
-- It scores 77 spots around the ship by where every bullet and enemy it
-- has noticed will be over the next 0.6 s (on the way there, then there),
-- and prefers lining up under the lowest enemy and grabbing pickups. It
-- reads bullets a little ahead (`anticipate`) and only notices those within
-- `awareness`. (The kit's bot, src/bot.rs, for a good player.)

local sim = require("sim")

local pilot = {}

local abs, sqrt, exp, min, max = math.abs, math.sqrt, math.exp, math.min, math.max
local clamp = math.clamp

pilot.GOOD = { anticipate = 0.25, awareness = 160, margin = 9, greed = 0.45, thumbSpeed = 950, react = { 0.15, 0.22 }, lapse = 0.04, overshoot = 0.1 }

-- Candidate offsets from the ship it considers moving to.
local DX = { -130, -90, -60, -36, -18, 0, 18, 36, 60, 90, 130 }
local DY = { -80, -50, -24, 0, 24, 50, 80 }
-- Times (s) ahead at which it checks where things will be.
local LOOK = { 0.06, 0.16, 0.28, 0.42, 0.6 }
-- Where it likes to sit when nothing's happening.
local HOME_Y = 470

local function hypot(x, y) return sqrt(x * x + y * y) end

-- Where the ship should go (x, y), from the sim as it is, wound forward by
-- `wind` seconds (the skill's anticipation less its reaction).
function pilot.plan(s, skill, wind)
  local sh = s.ship
  local aw = skill.awareness
  -- Threats it noticed: { x, y, vx, vy, radius }.
  local threats = {}
  for _, b in s.bullets do
    local x, y = b.x + b.vx * wind, b.y + b.vy * wind
    if abs(x - sh.x) < aw * 1.2 and abs(y - sh.y) < aw * 1.6 and hypot(x - sh.x, y - sh.y) < aw * 1.6 then
      threats[#threats + 1] = { x, y, b.vx, b.vy, b.r }
    end
  end
  local heat = s:heat()
  for _, e in s.enemies do
    local x, y = e.x + e.vx * wind, e.y + e.vy * wind
    local r = sim.KINDS[e.kind].radius
    if e.kind == "darter" and e.stage >= 1 and e.stage < 3 then
      -- A darter about to dash: its lane is dangerous (good players read the line).
      local dx, dy = e.ax - e.x, e.ay - e.y
      local d = max(hypot(dx, dy), 1)
      local v = 460 * sqrt(heat)
      if skill.anticipate > 0.05 or e.stage == 2 then threats[#threats + 1] = { x, y, dx / d * v, dy / d * v, r } end
    elseif hypot(x - sh.x, y - sh.y) < aw * 2 then
      threats[#threats + 1] = { x, y, e.vx, e.vy, r * 0.8 }
    end
  end
  -- What it wants: an enemy to shoot (the lowest one above it), pickups.
  local target
  local lowest = -1
  for _, e in s.enemies do
    if e.y > 0 and e.y < sh.y - 60 and e.y > lowest then target, lowest = e.x, e.y end
  end
  if not target and s.boss then target = s.boss.x end
  local b = s.boss
  local bestCost, bx, by = math.huge, sh.x, sh.y
  for _, dy in DY do
    for _, dx in DX do
      local cx = clamp(sh.x + dx, sim.SHIP_X[1], sim.SHIP_X[2])
      local cy = clamp(sh.y + dy, sim.SHIP_Y[1], sim.SHIP_Y[2])
      local travel = hypot(cx - sh.x, cy - sh.y)
      -- Time to get there, roughly (far-off spots are judged later in the future).
      local eta = travel / (skill.thumbSpeed * sim.DRAG_GAIN)
      local cost = travel * 0.0025 + ((cy - HOME_Y) / 160) ^ 2 * 0.4
      for _, th in threats do
        local x, y, vx, vy = th[1], th[2], th[3], th[4]
        local clear = th[5] + sim.SHIP_R + skill.margin
        local reach = clear * 2.2
        for _, t in LOOK do
          local k = if eta > 0 then min(t / eta, 1) else 1
          local px, py = sh.x + (cx - sh.x) * k, sh.y + (cy - sh.y) * k
          local ddx, ddy = x + vx * t - px, y + vy * t - py
          if abs(ddx) < reach and abs(ddy) < reach then
            local d = sqrt(ddx * ddx + ddy * ddy)
            if d < reach then cost += ((reach - d) / clear) ^ 2 * 4 * (1 - t * 0.9) end
          end
        end
      end
      if b then
        local ex, ey = (cx - b.x) / (sim.BOSS_RX + 40), (cy - b.y) / (sim.BOSS_RY + 60)
        if ex * ex + ey * ey < 1 then cost += 6 end
      end
      if target then cost -= skill.greed * 0.8 * exp(-abs(cx - target) / 40) end
      for _, d in s.drops do
        local want = if d.kind == "gem" then 0.15 else 1.2
        cost -= skill.greed * want * exp(-hypot(d.x - cx, d.y + 30 - cy) / 45)
      end
      if cost < bestCost then bestCost, bx, by = cost, cx, cy end
    end
  end
  return bx, by
end

-- A thumb flying the ship through the sim's own touch API (the title card's
-- attract mode and the debug Autopilot): it re-plans every reaction delay,
-- sometimes not at all (a lapse), overshoots big moves and moves no faster
-- than a thumb.
local Thumb = {}
Thumb.__index = Thumb
pilot.Thumb = Thumb

function pilot.thumb(seed, skill)
  return setmetatable({ rng = love.math.newRandomGenerator(seed), skill = skill or pilot.GOOD, finger = nil,
                        goal = nil, think = 0, last = { 0, 0 }, cautious = false }, Thumb)
end

function Thumb:drive(s, dt)
  if not s:alive() then
    if self.finger then
      s:touchUp()
      self.finger = nil
    end
    return
  end
  local sk, r = self.skill, self.rng
  if not s.grab then
    -- Thumb down under the ship (wherever is comfortable).
    local f = { s.ship.x + (r:random() * 40 - 20), min(s.ship.y + 110, sim.H - 6) }
    s:touchDown(f[1], f[2])
    self.finger, self.goal, self.last = f, { f[1], f[2] }, { s.ship.x, s.ship.y }
    return
  end
  local f = self.finger or { s.grab.x, s.grab.y }
  self.think -= dt
  if self.think <= 0 then
    local delay = sk.react[1] + (sk.react[2] - sk.react[1]) * r:random()
    self.think = delay
    if r:random() >= sk.lapse then
      local gx, gy = pilot.plan(s, sk, sk.anticipate - delay)
      -- A big new move overshoots; the correction after it is cautious.
      local sh = s.ship
      local ox, oy = gx, gy
      if hypot(gx - self.last[1], gy - self.last[2]) > 45 and not self.cautious then
        ox, oy = sh.x + (gx - sh.x) * (1 + sk.overshoot), sh.y + (gy - sh.y) * (1 + sk.overshoot)
        self.cautious = true
      elseif self.cautious then
        ox, oy = sh.x + (gx - sh.x) * 0.8, sh.y + (gy - sh.y) * 0.8
        self.cautious = false
      end
      self.last = { gx, gy }
      local fx, fy = s:fingerFor(ox, oy)
      if fx and fy then self.goal = { clamp(fx, 0, sim.W), clamp(fy, 0, sim.H) } end
    end
  end
  -- The thumb moves toward its goal no faster than it can.
  local g = self.goal or f
  local dx, dy = g[1] - f[1], g[2] - f[2]
  local d = hypot(dx, dy)
  local step = sk.thumbSpeed * dt
  if d > step then
    f = { f[1] + dx / d * step, f[2] + dy / d * step }
  else
    f = { g[1], g[2] }
  end
  self.finger = f
  s:touchMove(f[1], f[2])
end

return pilot
