-- Bell Keeper: one bell in the tower glows at a time; tap it before it
-- dims to ring it. Now and then a storm blows in: hold the rope until it
-- passes, or the bells swing loose. The game teaches through hints alone
-- (the first storm with a lesson), and the difficulty bot plays it from
-- them: it has no bot.lua.

local W, H = 360, 640
local SLOTS = { { 90, 250 }, { 180, 215 }, { 270, 250 }, { 90, 395 }, { 180, 360 }, { 270, 395 } }
local DIM, LIT = 0x5b4a3a, 0xf2c14e

-- The tower.
Shape{ x = W / 2, y = H / 2, w = W, h = H, color = 0x1b1f33 }
local gloom = Shape{ x = W / 2, y = H / 2, w = W, h = H, color = 0x05060c, alpha = 0, order = 5 }
Shape{ x = W / 2, y = 330, w = 300, h = 330, color = 0x2c2a3a }
Shape{ shape = "poly", x = W / 2, y = 140, points = { -170, 30, 0, -70, 170, 30 }, color = 0x3a2f45 }
local bells = {}
for i, p in ipairs(SLOTS) do
  Shape{ x = p[1], y = p[2] - 38, w = 4, h = 22, color = 0x15131c }
  bells[i] = Shape{ shape = "circle", x = p[1], y = p[2], radius = 28, color = DIM, tag = "bell" }
end
local rope = Shape{ x = W / 2, y = 560, w = 8, h = 110, color = 0xb08a5a }
local knot = Shape{ shape = "circle", x = W / 2, y = 615, radius = 12, color = 0xb08a5a }

local combo = ui.combo{ window = 2.5 }
local lit, litUntil, nextAt = nil, 0, 1.0
local ringHint
local storm -- { warn, start, stop, slack, hurt }
local nextStorm = 9
local letGo, letGoUntil
local stormRing, stormHint
local taught = false

-- The game's own clock, from dt: it slows with the lesson's slow motion
-- (round.time doesn't: it's the round's real time).
local clock = 0

local function life() return math.max(0.5, 1.7 - clock * 0.011) end
local function gap() return math.max(0.25, 0.9 - clock * 0.01) end

local function unlight()
  if lit then lit.color, lit.scale = DIM, 1 end
  lit = nil
  if ringHint then ringHint:dismiss() ringHint = nil end
end

local function light()
  local b
  repeat b = bells[love.math.random(#bells)] until b ~= lit
  unlight()
  lit, litUntil = b, clock + life()
  b.color = LIT
  -- The bell's own prompt: it's what the game asks, every time.
  ringHint = ui.hint{ text = "RING IT", at = b, ask = "tap" }
end

function love.mousepressed(x, y)
  if storm then return end
  if lit and (x - lit.x) ^ 2 + (y - lit.y) ^ 2 < 46 * 46 then
    combo:hit()
    round.score = round.score + combo.multiplier
    fx.burst("sparkles", lit.x, lit.y)
    unlight()
    nextAt = clock + gap()
  else
    combo:reset()
  end
end

local function holding() return love.mouse.isDown(1) end

function love.update(dt)
  clock = clock + dt
  local t = clock
  if round.ended then return end
  -- Storms: a warning, then hold until it passes.
  if not storm and t >= nextStorm then
    unlight()
    storm = { warn = t, start = t + 1.2, stop = t + 4.2, slack = 0 }
    if not taught then
      taught = true
      ui.lesson{ prompt = "HOLD THE ROPE", at = knot, slow = 0.4, learned = holding }
    else
      stormHint = ui.hint{ text = "HOLD THE ROPE", at = knot, priority = "threat" }
    end
    stormRing = ui.ring{ x = W / 2 + 130, y = 600, r = 24, value = 1, icon = "bolt", tone = "bad", order = 6 }
  end
  if storm then
    gloom.alpha = math.min(0.55, (t - storm.warn) * 0.6)
    stormRing.value = math.max(0, (storm.stop - t) / (storm.stop - storm.warn))
    if t >= storm.start then
      for _, b in ipairs(bells) do b.rotation = math.sin(t * 9 + b.x) * (holding() and 0.08 or 0.4) end
      if not holding() then storm.slack = storm.slack + dt end
      if storm.slack > 0.5 and not storm.hurt then
        storm.hurt = true
        hurt(rope)
        combo:reset()
      end
    end
    if t >= storm.stop then
      if stormHint then stormHint:dismiss() stormHint = nil end
      stormRing:destroy()
      for _, b in ipairs(bells) do b.rotation = 0 end
      gloom.alpha = 0
      storm = nil
      nextStorm = t + 9 + love.math.random() * 4
      nextAt = t + 0.8
      if holding() then letGo = ui.hint{ text = "LET GO", at = knot, priority = "threat" } end
    end
    return
  end
  if letGo and not holding() then letGo:dismiss() letGo = nil end
  -- Bells: one glows at a time.
  if lit and t >= litUntil then
    unlight()
    hurt(rope)
    combo:reset()
    nextAt = t + gap()
  elseif lit then
    lit.scale = 1 + 0.08 * math.sin(t * 18)
  elseif t >= nextAt then
    light()
  end
end
