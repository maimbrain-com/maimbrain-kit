-- The rules and the physics: a bird flies back and forth over a plate on a
-- pole, carrying a snack; a tap lets go. Whatever comes to rest on the
-- tower counts, whatever falls off costs a heart. Later the wind gusts and
-- the plate tilts. Pure game state: input arrives as calls (sim:drop()),
-- sounds, haptics and juice leave as cues (sim.cues); nothing here draws
-- or plays.
--
-- Every snack is a real rigid body (love.physics): how a snack lands,
-- wobbles, wedges or slides is never scripted. Units are the canvas's
-- (px, px/s, px/s²), at 100 px a meter.

local sim = {}

-- ---- Tuning knobs -------------------------------------------------------------
-- The numbers that shape the game, roughly in order of how much they matter.
-- `mb difficulty` (the reference's [bot]) measures what a change does to
-- round lengths.

-- Width of the plate the tower stands on, px. Narrower is harder at once.
sim.PLATE_W = 156
-- The bird's sweep: half-width in px (start, added per snack stacked, max).
-- Wider sweeps mean more drift to judge. (The kit adds 4 a snack: 3 keeps
-- the kit's round lengths against the difficulty bot, whose players miss
-- by their timing, more at speed, where the kit's bot misjudged by pixels.)
sim.SWEEP_AMP = { 14, 3, 88 }
-- The bird's sweep speed in rad/s of its sine (start, per snack, max).
-- Faster is harder to time; past ~2.4 it feels frantic.
sim.SWEEP_SPEED = { 1.3, 0.035, 1.8 }
-- How much of the bird's sideways speed a dropped snack keeps (0 = drops
-- straight down, 1 = all of it). The landing ghost always shows the result.
sim.CARRY = 0.2
-- Seconds from a drop until the bird has the next snack. Lower is a faster
-- tempo (and taller towers in the same time).
sim.RESPAWN = 0.6
-- Hearts: tumbles before it's over (studio.toml's `lives` shows them).
sim.LIVES = 3
-- Snacks that fall within this many seconds of a lost heart belong to the
-- same tumble: a toppling tower costs one heart, not one per snack.
sim.TUMBLE_GRACE = 2.5
-- This many snacks in a row without a fall wins a lost heart back (0 = never).
-- Rewards steady play: skilled players go on much longer.
sim.HEART_BACK = 10
-- When each snack joins the mix: { snacks stacked, kind }. A newly unlocked
-- kind always comes next, so the player meets it at once.
sim.UNLOCKS = { { 0, "box" }, { 3, "cube" }, { 6, "plank" }, { 10, "wedge" }, { 15, "bun" }, { 21, "ball" } }
-- Twists (wind gusts, then a tilting plate, alternating): seconds into the
-- round of the first one, and the random gap between later ones.
sim.TWIST_FIRST = 16
sim.TWIST_GAP = { 11, 16 }
-- Each twist is announced this long before it starts.
sim.WARN_TIME = 1.5
-- Gust: peak sideways push (px/s², gravity is 700) and how long it blows.
-- Every twist is a bit stronger than the last (TWIST_GROWTH).
sim.GUST_ACCEL = 70
sim.GUST_TIME = 2.4
-- Tilt: peak plate angle (radians) and how long one tilt-and-back takes.
sim.TILT_MAX = 0.07
sim.TILT_TIME = 4.0
-- How much stronger each twist is than the one before (0.2 = +20 %), up to ×TWIST_CAP.
sim.TWIST_GROWTH = 0.2
sim.TWIST_CAP = 2.0
-- Gravity, px/s² (real is 981). Lower is floatier: softer landings and
-- slower, more readable topples.
sim.GRAVITY = 700
-- Grip of every snack (friction). Lower and towers slide apart. Two
-- surfaces slide with the smaller of their frictions here.
sim.FRICTION = 0.9
-- Snacks this far (px) below the top of the tower, once still, settle into
-- a solid base (welded to the plate): only the top of the tower can topple,
-- so skilled players can go on and the physics stays cheap. Lower it and
-- the game gets more forgiving; math.huge turns it off.
sim.SOLID_BELOW = 130
-- A landing snack grips for a moment: on first touch its motion is damped
-- this hard (per second) for GRIP_TIME, so it settles instead of skidding
-- off. 0 is pure physics (bouncier, harder).
sim.GRIP = 10
sim.GRIP_TIME = 0.25
-- How bouncy snacks are (two surfaces bounce by the larger of theirs): the
-- jelly a little, the rest hardly.
sim.BOUNCE = 0.02
sim.BOUNCE_JELLY = 0.085

-- ---- Rules that rarely need changing -------------------------------------------

sim.W = 360
-- World y of the plate's top surface (the camera starts at 0).
sim.PLATE_TOP = 404
sim.PLATE_H = 14
-- The picnic blanket under the pole: where fallen snacks end up.
sim.GROUND_Y = 560
-- A snack whose centre falls this far below the plate top is lost.
sim.LOST_BELOW = 70
-- Where the bird flies, on screen, and the least room it keeps above the tower.
sim.BIRD_SCREEN_Y = 222
sim.BIRD_CLEARANCE = 70
-- From the bird's centre to the top of the snack it carries.
sim.HOLD_GAP = 20
-- Where the camera keeps the top of the tower, on screen.
sim.TOWER_TOP_ON_SCREEN = 372
-- A dropped snack counts once it has rested on the tower this long, this still.
sim.SETTLE_TIME = 0.3
sim.SETTLE_SPEED = 22
-- Landing within this many px of the centre of what it sits on is "neat";
-- further out than this fraction of its half-width is a close one.
sim.NEAT_PX = 7
sim.CLOSE_FRACTION = 0.6
-- Impacts slower than this (px/s) are silent; above HEAVY_SPEED they thump.
sim.THUD_SPEED = 70
sim.HEAVY_SPEED = 420
-- After the last heart: the plate tips and the collapse plays out this long.
sim.ENDING = 2.0
-- Fallen snacks lie dizzy on the blanket this long, then go (not after the
-- last heart: the pile is the game-over card's aftermath).
sim.LOST_LINGER = 3.0

local W, PLATE_TOP, PLATE_H, PLATE_W = sim.W, sim.PLATE_TOP, sim.PLATE_H, sim.PLATE_W
local abs, min, max, sin, cos, exp, sqrt, pi = math.abs, math.min, math.max, math.sin, math.cos, math.exp, math.sqrt, math.pi
local lp = love.physics
-- One physics step (the engine steps at a fixed 60 Hz).
local STEP = 1 / 60

-- ---- Snacks ---------------------------------------------------------------------
-- Piece shapes. The names are physical; what they look like (toast, jelly,
-- a baguette, cheese…) lives in look.lua.
--   box    a wide box: the easy one
--   cube   a square, a little bouncy
--   plank  long and thin: a great platform, easy to tip
--   wedge  wide at the bottom, narrow on top: steady, but a small target
--   bun    a box with very round corners: slides and rocks
--   ball   round: rolls (damped so it settles)
-- Full size in px (a ball's is its diameter), and the corner radius of the
-- drawn and simulated shape (matched to the art: the baguette's ends are round).
sim.KINDS = {
  box = { w = 66, h = 42, r = 0 },
  cube = { w = 44, h = 44, r = 0 },
  plank = { w = 116, h = 22, r = 9 },
  wedge = { w = 84, h = 42, r = 0 },
  bun = { w = 64, h = 36, r = 16 },
  ball = { w = 46, h = 46, r = 23 },
}
local KINDS = sim.KINDS

-- The wedge's corners in its own frame: bottom-left, bottom-right,
-- top-right, top-left (the top is 46 % of the bottom, as in the art).
function sim.wedgePoints()
  local s = KINDS.wedge
  local hw, hh, top = s.w / 2, s.h / 2, s.w * 0.23
  return { -hw, hh, hw, hh, top, -hh, -top, -hh }
end

-- A rounded rectangle as a polygon: `n` points around each corner.
local function roundRect(w, h, r, n)
  local pts = {}
  local hw, hh = w / 2 - r, h / 2 - r
  local corners = { { hw, hh, 0 }, { -hw, hh, pi / 2 }, { -hw, -hh, pi }, { hw, -hh, 3 * pi / 2 } }
  for _, c in corners do
    for i = 0, n - 1 do
      local a = c[3] + (pi / 2) * i / (n - 1)
      pts[#pts + 1] = c[1] + r * cos(a)
      pts[#pts + 1] = c[2] + r * sin(a)
    end
  end
  return pts
end

local function shapeOf(kind)
  local s = KINDS[kind]
  if kind == "wedge" then
    return lp.newPolygonShape(sim.wedgePoints())
  elseif kind == "ball" then
    return lp.newCircleShape(s.w / 2)
  elseif s.r > 0 then
    return lp.newPolygonShape(roundRect(s.w, s.h, s.r, 5))
  end
  return lp.newRectangleShape(s.w, s.h)
end

-- Linear and angular damping (balls roll, then stop).
local function damping(kind)
  if kind == "ball" then return 0.3, 4.0 end
  return 0.02, 0.3
end

-- Wedges and balls are awkward: never two in a row.
local function awkward(kind)
  return kind == "wedge" or kind == "ball"
end

-- Half its height when turned by `angle` (for measuring the tower).
function sim.halfHeight(kind, angle)
  local s = KINDS[kind]
  if kind == "ball" then return s.w / 2 end
  return 0.5 * (s.w * abs(sin(angle)) + s.h * abs(cos(angle)))
end
local halfHeight = sim.halfHeight

-- ---- The sim ---------------------------------------------------------------------
-- A piece: { body, kind, variant (which of its three looks), state ("falling",
-- "landed", "lost"), rest (s still on the tower while falling), t (s since
-- it was lost), age (s since dropped), fear (0–1: hits, speed, the tower
-- wobbling; drives the face), solid (its weld to the plate, once it's in the
-- solid base), grip (s of landing grip left; nil until it first touches) }.
-- Twists: twist = { phase = "calm" | "warn" | "on", kind = "gust" | "tilt", dir = ±1, t, left }.
-- Cues (sim.cues, for sound and juice): { type = "drop" | "thud" | "land" |
-- "lost" | "heartback" | "newkind" | "warn" | "start" | "collapse" | "over", … }.

local Sim = {}
Sim.__index = Sim
sim.Sim = Sim

-- The world's callbacks store what touched here, for after the step.
local begun = {}

local function beginContact(fa, fb, contact)
  local nx, ny = contact:getNormal()
  local x, y = contact:getPositions()
  begun[#begun + 1] = { a = fa:getBody(), b = fb:getBody(), nx = nx, ny = ny, x = x, y = y }
end

function Sim.new(seed)
  local self = setmetatable({}, Sim)
  lp.setMeter(100)
  local world = lp.newWorld(0, sim.GRAVITY, true)
  world:setCallbacks(beginContact)
  self.world = world
  -- The plate is kinematic so it can tilt (twists, the final tip).
  local plate = lp.newBody(world, W / 2, PLATE_TOP + PLATE_H / 2, "kinematic")
  local pf = lp.newFixture(plate, lp.newPolygonShape(roundRect(PLATE_W, PLATE_H, 6, 4)))
  pf:setFriction(1.0)
  self.plate = plate
  -- The ground catches what falls (it's far below the tower; nothing counts there).
  local ground = lp.newBody(world, W / 2, sim.GROUND_Y + 30, "static")
  lp.newFixture(ground, lp.newRectangleShape(1200, 60)):setFriction(0.9)
  self.ground = ground
  self.rng = love.math.newRandomGenerator(seed)
  self.pieces = {}
  self.held = nil -- { kind, variant, age }: the snack the bird carries (not a body until it's let go)
  self.t = 0 -- seconds since the round began
  self.phase = 0
  -- The sweep eases in from nothing, so the first seconds can't go wrong.
  self.amp = 0
  self.speed = sim.SWEEP_SPEED[1]
  self.moving = false -- the bird waits at the centre until the first drop
  self.score, self.lives, self.streak, self.drops = 0, sim.LIVES, 0, 0
  self.respawn = nil
  self.grace = 0 -- seconds left of the current tumble (further falls are free)
  self.unlocked = 1 -- kinds met so far (entries of UNLOCKS)
  self.lastKind = nil
  self.twist = { phase = "calm", left = sim.TWIST_FIRST }
  self.twists = 0
  self.wind = 0 -- the wind's push right now (px/s², + is right)
  self.plateAngle = 0 -- the plate's angle right now (radians, + is clockwise)
  self.plateSpin = 0
  self.ending = nil -- seconds since the last heart was lost (the collapse plays out first)
  self.tipDir = 1
  self.over = false
  self.camY = 0 -- world y at the top of the screen (negative as the tower grows)
  -- What moves camY: a critically damped spring (half-life 0.3 s), so the
  -- camera glides up when a snack lands rather than starting with a jolt.
  self.cam = { value = 0, velocity = 0, target = 0, halflife = 0.3 }
  self.height, self.bestHeight = 0, 0 -- the tower's height above the plate, px
  self.wobble = 0 -- how much the tower is moving (0 calm … 1 swaying), eased
  self.cues = {}
  self:spawn()
  return self
end

function Sim:destroy()
  self.world:destroy()
end

function Sim:cue(c)
  self.cues[#self.cues + 1] = c
end

-- ---- where things are ---------------------------------------------------------

-- The bird's centre (world).
function Sim:bird()
  local top = PLATE_TOP - self.height
  -- Room for the tallest snack under it, whatever it carries now.
  local y = min(self.camY + sim.BIRD_SCREEN_Y, top - sim.BIRD_CLEARANCE - 46 - sim.HOLD_GAP)
  return W / 2 + self.amp * sin(self.phase), y
end

-- The bird's sideways speed, px/s.
function Sim:birdVx()
  if self.moving then return self.amp * self.speed * cos(self.phase) end
  return 0
end

-- Centre of the carried snack (world), or nil.
function Sim:heldPos()
  local h = self.held
  if not h then return nil end
  local bx, by = self:bird()
  return bx, by + sim.HOLD_GAP + KINDS[h.kind].h / 2
end

-- The top of the tower: the centre x of the highest landed snack (or the
-- plate's) and the y of its top.
function Sim:towerTop()
  local bx, by = W / 2, PLATE_TOP
  for _, p in self.pieces do
    if p.state == "landed" then
      local x, y = p.body:getPosition()
      local top = y - halfHeight(p.kind, p.body:getAngle())
      if top < by then bx, by = x, top end
    end
  end
  return bx, by
end

-- The ray test's answer (nearest first, so the first hit is the one).
local hitFraction
local function firstHit(_, _, _, _, _, fraction)
  hitFraction = fraction
  return 0
end

-- Where the carried snack would land if let go now: its free fall (keeping
-- CARRY of the bird's speed, pushed by the wind blowing now), raycast from
-- its bottom corners and centre against everything. Returns { arc = { x1,
-- y1, … } (its centre every 1/15 s), lx, ly (its centre when it first
-- touches something), hits (false: it would miss everything) }, or nil.
function Sim:predict()
  local held = self.held
  if not held then return nil end
  local sx, sy = self:heldPos()
  local s = KINDS[held.kind]
  local g = sim.GRAVITY
  local vx, ax = self:birdVx() * sim.CARRY, self.wind
  local feet = { -0.42 * s.w, 0, 0.42 * s.w }
  local h = 1 / 30
  local arc = {}
  local half = s.h / 2
  local world = self.world
  for k = 0, 89 do
    local t0, t1 = k * h, (k + 1) * h
    local x0, y0 = sx + vx * t0 + 0.5 * ax * t0 * t0, sy + 0.5 * g * t0 * t0
    local x1, y1 = sx + vx * t1 + 0.5 * ax * t1 * t1, sy + 0.5 * g * t1 * t1
    if k % 2 == 0 then
      arc[#arc + 1] = x0
      arc[#arc + 1] = y0
    end
    local first
    for _, dx in feet do
      hitFraction = nil
      world:rayCast(x0 + dx, y0 + half, x1 + dx, y1 + half, firstHit)
      if hitFraction and (not first or hitFraction < first) then first = hitFraction end
    end
    if first then
      local lx, ly = x0 + (x1 - x0) * first, y0 + (y1 - y0) * first
      arc[#arc + 1] = lx
      arc[#arc + 1] = ly
      return { arc = arc, lx = lx, ly = ly, hits = true }
    end
    if y1 > PLATE_TOP + sim.LOST_BELOW + 60 then
      return { arc = arc, lx = x1, ly = y1, hits = false }
    end
  end
  return { arc = arc, lx = arc[#arc - 1], ly = arc[#arc], hits = false }
end

-- How strong twists are right now (1 = the first one).
function Sim:strength()
  return min(1 + max(self.twists - 1, 0) * sim.TWIST_GROWTH, sim.TWIST_CAP)
end

-- ---- input ------------------------------------------------------------------------

-- Lets go of the carried snack (the player's tap). False if the bird has
-- none right now.
function Sim:drop()
  if self.ending or not self.held then return false end
  self:release()
  self.drops += 1
  self.moving = true
  self.respawn = sim.RESPAWN
  self:cue({ type = "drop" })
  return true
end

-- Turns the carried snack into a falling body.
function Sim:release()
  local x, y = self:heldPos()
  local h = self.held
  if not x or not h then return end
  self.held = nil
  local body = lp.newBody(self.world, x, y, "dynamic")
  local f = lp.newFixture(body, shapeOf(h.kind), 1)
  f:setFriction(sim.FRICTION)
  f:setRestitution(if h.kind == "cube" then sim.BOUNCE_JELLY else sim.BOUNCE)
  body:setLinearVelocity(self:birdVx() * sim.CARRY, 0)
  local lin, ang = damping(h.kind)
  body:setLinearDamping(lin)
  body:setAngularDamping(ang)
  local p = { body = body, kind = h.kind, variant = h.variant, state = "falling", rest = 0, t = 0, age = 0, fear = 0, id = self.drops }
  body:setUserData(p)
  self.pieces[#self.pieces + 1] = p
end

-- ---- what comes next ----------------------------------------------------------------

-- The kind of the next snack: a newly unlocked kind first, else random among
-- the unlocked ones (never two awkward ones in a row).
function Sim:nextKind()
  local u = sim.UNLOCKS
  if self.unlocked < #u and self.score >= u[self.unlocked + 1][1] then
    self.unlocked += 1
    local k = u[self.unlocked][2]
    self:cue({ type = "newkind", kind = k })
    return k
  end
  while true do
    local k = u[self.rng:random(1, self.unlocked)][2]
    if not (awkward(k) and self.lastKind and awkward(self.lastKind)) then return k end
  end
end

function Sim:spawn()
  local kind = self:nextKind()
  self.lastKind = kind
  self.held = { kind = kind, variant = self.rng:random(1, 3), age = 0 }
end

-- ---- the step ----------------------------------------------------------------------

function Sim:step(dt)
  self.t += dt
  -- The bird: sweeps wider and faster as the tower grows (eased).
  local s = self.score
  local amp = min(sim.SWEEP_AMP[1] + sim.SWEEP_AMP[2] * s, sim.SWEEP_AMP[3])
  local speed = min(sim.SWEEP_SPEED[1] + sim.SWEEP_SPEED[2] * s, sim.SWEEP_SPEED[3])
  local k = 1 - exp(-dt * 1.5)
  self.amp += (amp - self.amp) * k
  self.speed += (speed - self.speed) * k
  if self.moving then self.phase += self.speed * dt end
  if self.held then self.held.age += dt end

  self:doTwists(dt)
  -- Wind pushes every snack in the air or on the tower.
  if self.wind ~= 0 then
    for _, p in self.pieces do
      if p.state ~= "lost" then p.body:applyForce(self.wind * p.body:getMass(), 0) end
    end
  end
  -- The plate turns toward its angle over the next step (it only moves
  -- while it tilts, so a resting tower can sleep).
  local diff = self.plateAngle - self.plate:getAngle()
  local spin = if abs(diff) > 1e-5 then diff / STEP else 0
  if spin ~= self.plateSpin then
    self.plateSpin = spin
    self.plate:setAngularVelocity(spin)
  end
  -- The approach speed of what touches this step: velocities from before it.
  for _, p in self.pieces do
    p.vx, p.vy = p.body:getLinearVelocity()
  end
  self.world:update(dt)
  self:contacts()
  self:settle(dt)
  self:fallOff(dt)

  if self.respawn then
    self.respawn -= dt
    if self.respawn <= 0 and not self.ending then
      self.respawn = nil
      self:spawn()
    end
  end

  -- Camera: keep the top of the tower in view, easing.
  self.height = self:measureHeight()
  self.bestHeight = max(self.bestHeight, self.height)
  local want = min(PLATE_TOP - self.height - sim.TOWER_TOP_ON_SCREEN, 0)
  -- After the last heart the camera drops to the pile on the blanket.
  if self.ending then
    want = 0
    self.cam.halflife = 0.28
  end
  self.camY = self:follow(want, dt)

  if self.ending then
    self.ending += dt
    if self.ending >= sim.ENDING and not self.over then
      self.over = true
      self:cue({ type = "over" })
    end
  end
end

-- The camera's spring (critically damped: maimbrain's motion::Follow).
function Sim:follow(target, dt)
  local f = self.cam
  if dt <= 0 then return f.value end
  local w = 1.678347 / max(f.halflife, 1e-4)
  local u = (target - f.target) / dt
  local lead = u * (2 / w)
  local e0, e1 = f.value - f.target + lead, f.velocity - u
  local j = e1 + e0 * w
  local k = exp(-w * dt)
  f.value = target - lead + (e0 + j * dt) * k
  f.velocity = u + (e1 - j * (w * dt)) * k
  f.target = target
  return f.value
end

-- Wind gusts and plate tilts: announced, then on, then calm again.
function Sim:doTwists(dt)
  if self.ending then
    -- The plate tips everything off (the fail animation).
    self.wind = 0
    self.plateAngle = self.tipDir * 0.7 * min(self.ending / 1.0, 1)
    return
  end
  local tw = self.twist
  if tw.phase == "calm" then
    tw.left -= dt
    if tw.left <= 0 then
      local kind = if self.twists % 2 == 0 then "gust" else "tilt"
      local dir = if self.rng:random(1, 2) == 1 then 1 else -1
      self.twists += 1
      self:cue({ type = "warn", kind = kind, dir = dir })
      self.twist = { phase = "warn", kind = kind, dir = dir, t = 0 }
    end
  elseif tw.phase == "warn" then
    tw.t += dt
    if tw.t >= sim.WARN_TIME then
      self:cue({ type = "start", kind = tw.kind })
      self.twist = { phase = "on", kind = tw.kind, dir = tw.dir, t = 0 }
    end
  else
    tw.t += dt
    local len = if tw.kind == "gust" then sim.GUST_TIME else sim.TILT_TIME
    if tw.t >= len then
      self.twist = { phase = "calm", left = sim.TWIST_GAP[1] + self.rng:random() * (sim.TWIST_GAP[2] - sim.TWIST_GAP[1]) }
    end
  end
  local strength = self:strength()
  tw = self.twist
  self.wind, self.plateAngle = 0, 0
  if tw.phase == "on" and tw.kind == "gust" then
    self.wind = tw.dir * sim.GUST_ACCEL * strength * sin(pi * tw.t / sim.GUST_TIME)
  elseif tw.phase == "on" then
    self.plateAngle = tw.dir * sim.TILT_MAX * min(strength, 1.5) * sin(pi * tw.t / sim.TILT_TIME)
  end
end

local function pieceOf(body)
  local u = body:getUserData()
  if type(u) == "table" then return u end
  return nil
end

-- Impacts: thuds (at most three a step) and scared faces.
function Sim:contacts()
  local thuds = 0
  for i = 1, #begun do
    local c = begun[i]
    local a, b = pieceOf(c.a), pieceOf(c.b)
    -- A falling snack's first touch: it grips.
    for _, p in { a, b } do
      if p and p.grip == nil and p.state == "falling" then
        p.grip = sim.GRIP_TIME
        p.body:setLinearDamping(sim.GRIP)
        p.body:setAngularDamping(sim.GRIP)
      end
    end
    local avx, avy = if a then a.vx or 0 else 0, if a then a.vy or 0 else 0
    local bvx, bvy = if b then b.vx or 0 else 0, if b then b.vy or 0 else 0
    local rx, ry = avx - bvx, avy - bvy
    local speed = if c.nx ~= 0 or c.ny ~= 0 then abs(rx * c.nx + ry * c.ny) else sqrt(rx * rx + ry * ry)
    if speed >= sim.THUD_SPEED then
      if thuds < 3 then
        thuds += 1
        local vol = math.clamp((speed - 50) / 500, 0.1, 1)
        local x, y = c.x, c.y
        if not x then x, y = c.a:getPosition() end
        self:cue({ type = "thud", x = x, y = y, vol = vol, speed = speed, heavy = speed > sim.HEAVY_SPEED })
      end
      for _, p in { a, b } do
        if p then p.fear = min(p.fear + speed / 600, 1) end
      end
    end
  end
  table.clear(begun)
end

-- A dropped snack counts once it rests on the tower; faces calm down or
-- get worried with the tower's sway.
function Sim:settle(dt)
  local sway = 0
  for i = 1, #self.pieces do
    local p = self.pieces[i]
    local body = p.body
    local vx, vy = body:getLinearVelocity()
    local speed = sqrt(vx * vx + vy * vy)
    local spin = abs(body:getAngularVelocity())
    p.age += dt
    if p.grip and p.grip > 0 then
      p.grip -= dt
      if p.grip <= 0 then
        local lin, ang = damping(p.kind)
        body:setLinearDamping(lin)
        body:setAngularDamping(ang)
      end
    end
    local calm = max(1 - dt * 1.2, 0)
    p.fear = max(p.fear * calm, math.clamp((speed - 30) / 260, 0, 1), min(spin / 2.5, 1))
    if p.state == "landed" then
      sway = max(sway, speed / 120 + spin / 1.5)
    elseif p.state == "falling" then
      local still = speed < sim.SETTLE_SPEED and spin < 0.5
      local sx, half = self:support(p)
      if still and sx then p.rest += dt else p.rest = 0 end
      if p.rest >= sim.SETTLE_TIME and not self.ending then
        p.state = "landed"
        self:land(p, sx or W / 2, half or PLATE_W / 2)
      end
    end
  end
  -- The tower's sway (rises fast, settles slowly) worries every face on it.
  local target = max(min(sway, 1), if self.wind ~= 0 or self.plateAngle ~= 0 then 0.5 else 0)
  local rate = if target > self.wobble then 8 else 1.5
  self.wobble += (target - self.wobble) * (1 - exp(-dt * rate))
  for _, p in self.pieces do
    if p.state == "landed" and not p.solid then p.fear = max(p.fear, self.wobble * 0.9) end
  end
  -- Deep, still snacks settle into the solid base.
  local _, top = self:towerTop()
  if not self.ending then
    for _, p in self.pieces do
      if p.state == "landed" and not p.solid then
        local x, y = p.body:getPosition()
        local vx, vy = p.body:getLinearVelocity()
        if y > top + sim.SOLID_BELOW and vx * vx + vy * vy < 64 then
          p.solid = lp.newWeldJoint(self.plate, p.body, x, y, false)
        end
      end
    end
  end
end

-- If a piece rests on the tower (the plate or a landed snack), what it
-- rests on: its centre x and half its width.
function Sim:support(p)
  local _, me = p.body:getPosition()
  local bx, by, bhalf
  for _, c in p.body:getContacts() do
    if c:isTouching() then
      local fa, fb = c:getFixtures()
      local other = fa:getBody()
      if other == p.body then other = fb:getBody() end
      local half
      if other == self.plate then
        half = PLATE_W / 2
      else
        local o = pieceOf(other)
        if o and o.state == "landed" then half = KINDS[o.kind].w / 2 end
      end
      if half then
        local ox, oy = other:getPosition()
        if oy > me and (not by or oy < by) then bx, by, bhalf = ox, oy, half end
      end
    end
  end
  return bx, bhalf
end

function Sim:land(p, supportX, half)
  self.score += 1
  self.streak += 1
  local x, y = p.body:getPosition()
  local off = abs(x - supportX)
  local neat, close = off < sim.NEAT_PX, off > half * sim.CLOSE_FRACTION
  self:cue({ type = "land", x = x, y = y, streak = self.streak, neat = neat, close = close })
  if sim.HEART_BACK > 0 and self.streak % sim.HEART_BACK == 0 and self.lives < sim.LIVES then
    self.lives += 1
    self:cue({ type = "heartback" })
  end
end

-- Snacks that fall off cost a heart; the last one starts the collapse.
-- Fallen snacks leave the world once out of sight.
function Sim:fallOff(dt)
  self.grace = max(self.grace - dt, 0)
  for _, p in self.pieces do
    if p.state == "lost" then
      p.t += dt
    else
      local x, y = p.body:getPosition()
      if y > PLATE_TOP + sim.LOST_BELOW then
        local landed = p.state == "landed"
        p.state, p.t, p.fear = "lost", 0, 1
        local counted = not self.ending and self.grace <= 0
        self:cue({ type = "lost", x = x, y = y, kind = p.kind, counted = counted, landed = landed })
        if counted then
          self.grace = sim.TUMBLE_GRACE
          self:loseHeart(x)
        end
      end
    end
  end
  local linger = if self.ending then math.huge else sim.LOST_LINGER
  local kept = {}
  for _, p in self.pieces do
    local gone = p.state == "lost" and (p.t > linger or select(2, p.body:getPosition()) > sim.GROUND_Y + 200)
    if gone then
      p.body:destroy()
    else
      kept[#kept + 1] = p
    end
  end
  self.pieces = kept
end

function Sim:loseHeart(x)
  self.streak = 0
  self.lives = max(self.lives - 1, 0)
  if self.lives == 0 then
    self.ending = 0
    self.twist = { phase = "calm", left = math.huge }
    -- Tip away from the side it fell on: whatever's left goes the other way.
    local lean = self:towerTop() - W / 2
    self.tipDir = if abs(lean) > 4 then math.sign(lean) elseif x < W / 2 then 1 else -1
    self:cue({ type = "collapse" })
    -- Everything comes loose for the fall.
    for _, p in self.pieces do
      if p.solid then
        p.solid:destroy()
        p.solid = nil
      end
    end
    -- The bird lets go of what it carries too.
    if self.held then self:release() end
    self.respawn = nil
  end
end

-- The highest point of anything resting on the tower, above the plate.
function Sim:measureHeight()
  local _, top = self:towerTop()
  return max(PLATE_TOP - top, 0)
end

-- True while a dropped snack is still on its way (not yet counted or lost).
function Sim:falling()
  for _, p in self.pieces do
    if p.state == "falling" then return true end
  end
  return false
end

-- ---- The title card's autopilot -------------------------------------------------
-- The title card's bird stacks by itself: mostly neat drops, every fifth
-- one sloppy, so the card wobbles and now and then tumbles. Deterministic
-- (it reads only the sim).
function Sim:autopilot(pred)
  local held = self.held
  if not held or held.age < 0.7 or not pred then return end
  local slack = if self.drops % 5 == 4 then 24 else 5
  if abs(pred.lx - self:towerTop()) < slack then self:drop() end
end

return sim
