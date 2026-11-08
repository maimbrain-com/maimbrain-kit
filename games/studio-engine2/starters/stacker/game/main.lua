-- Picnic Pile, the stacker starter: a paper bird flies back and forth over
-- a plate balanced on a pole, carrying snacks with faces. Tap (or Space,
-- Enter, Down, or a controller's A) to let go. Every snack is a real rigid
-- body (love.physics): how it lands, wobbles, wedges or tumbles is never
-- scripted. A dotted path and a shadow show where it would land right now.
-- A snack that falls off costs one of three hearts; ten in a row wins one
-- back. The bird sweeps wider and faster with every snack, a new snack
-- joins at 3, 6, 10, 15 and 21 stacked (jelly, baguette, cheese, macaron,
-- tomato), and from 16 s a wind gust or a tilting plate every 11–16 s. The
-- sky turns golden, then sunset, then night as the tower climbs. The score
-- is snacks stacked; the last heart tips the plate and the bird flies off.
--
-- It's the stacker kit's Picnic Pile, with its paper cut-out art, sounds,
-- words and look: all of it is this starter's, for the game to replace
-- with its own.
--
-- Make it yours:
--   game/sim.lua    the rules and the physics: the bird, the snacks'
--                   shapes, landing, hearts, twists. The tuning knobs are at
--                   the top (plate width, the sweep, gravity, grip, twists).
--   game/look.lua   the words (popups, banners, the hint), each snack's
--                   name and pictures, the sky's colours by height.
--   game/draw.lua   how the world is drawn from the art: sky, meadow,
--                   plate, snacks, faces, the bird, the ghost, wind.
--   game/sound.lua  which sound plays for what, the music and the mix.
--   images/, sounds/, music/   the art and the sound; studio.toml's
--                   title, words and [identity] are the cards and the HUD.
-- This file is the host: input, juice, hints, banners, the HUD and the
-- title card's tower, which stacks itself.

local sim = require("sim")
local look = require("look")
local draw = require("draw")
local sound = require("sound")
local shared = require("shared")

local W = sim.W
local g = love.graphics
local abs, min, max, sin = math.abs, math.min, math.max, math.sin

-- The how-to hint comes back after this long without a tap (s).
local HINT_IDLE = 3.0
-- The title card's own tower starts over after this long (or when it falls).
local ATTRACT_ROUND = 40
-- The game-over card comes up this long before the collapse is done (the
-- engine's ending), so it lands as the bird flies off.
local CARD_EARLY = 0.7
-- A lost heart freezes the physics for a moment.
local HITSTOP = 0.07
-- On the title card the view looks this far right of the plate, so the
-- attract tower stands left of the engine's DAILY button (bottom right).
local TITLE_SHIFT = 72

-- Debug (the debug menu, debug/scenarios.toml): the round plays itself;
-- it builds a tower of BUILD snacks first (fast, quietly); it starts with
-- LIVES hearts; the next twist comes at once.
AUTO = false
BUILD = 0
LIVES = 0
TWIST_NOW = false

local s -- the sim (game/sim.lua): the round's, or the title card's
local pred -- where the carried snack would land now (sim:predict)
local time, idle, finished, applied = 0, 0, false, false
local banner -- { text, color, icon ("left", "right", "heart"), t }
local hurt = 0 -- the red edge's pulse when a heart goes (1 → 0)
local lastCause = "miss"

local function newSim()
  if s then s:destroy() end
  s = sim.Sim.new(love.math.random(1, 2 ^ 30))
  pred = nil
end

function love.load()
  draw.load()
  sound.load()
  time, idle, finished = 0, 0, false
  newSim()
  if not round.title then
    sound.start()
    -- The engine's HUD: the score, the best until it's beaten, the hearts.
    ui.hud({ score = true, best = true, lives = true })
  end
end

-- ---- Input: any press lets go ---------------------------------------------------------
-- (The press that starts the round is its first drop: a slice of toast
-- under a bird that hasn't started moving, so it always lands.)

local function down()
  if round.title or round.ended or not s then return end
  if s:drop() then idle = 0 end
end

local function isDrop(key)
  return key == "space" or key == "return" or key == "down" or key == "s"
end

-- A finger is the mouse too: fingers come through touchpressed, the mouse
-- itself through mousepressed.
function love.touchpressed() down() end
function love.mousepressed(_, _, _, istouch) if not istouch then down() end end
function love.keypressed(key) if isDrop(key) then down() end end
function love.gamepadpressed(_, button)
  if button == "a" or button == "b" or button == "x" or button == "y" or button == "dpdown" then down() end
end

-- ---- Juice ----------------------------------------------------------------------------

local function setBanner(text, color, icon)
  banner = { text = text, color = color, icon = icon, t = 0 }
end

-- Particles, popups, shake and banners from one of the sim's cues.
-- playing: the round, not the title card's tower.
local function juice(c, playing)
  local k = c.type
  if k == "drop" then
    local bx, by = s:bird()
    fx.burst("dust", bx, by + 16, { count = 4, color = look.WHITE })
  elseif k == "thud" then
    fx.burst("dust", c.x, c.y, { count = if c.heavy then 7 else 3, color = look.CRUMB })
    if c.heavy and playing then fx.shake(2 + 2 * c.vol, 0.2) end
  elseif not playing then
    return
  elseif k == "land" then
    if c.close then
      fx.popup(look.CLOSE, c.x, c.y - 34, { color = look.WHITE, size = 22 })
      fx.burst("dust", c.x, c.y, { count = 6, color = look.CRUMB })
    elseif c.neat then
      fx.popup(look.NEAT, c.x, c.y - 34, { color = look.GOLD, size = 24 })
      fx.burst("sparkles", c.x, c.y, { count = 10, color = look.SPARKLE })
    else
      fx.popup("+1", c.x, c.y - 30, { color = look.WHITE, size = 20 })
      fx.burst("sparkles", c.x, c.y, { count = 5, color = look.SPARKLE })
    end
    if c.streak >= 5 and c.streak % 5 == 0 then
      fx.popup(string.format(look.STREAK, c.streak), c.x, c.y - 62, { color = look.GOLD, size = 20 })
      fx.burst("stars", c.x, c.y, { count = 8, color = look.GOLD })
    end
  elseif k == "lost" then
    if c.counted then
      lastCause = if c.landed then "tumble" else "miss"
      -- Shown on screen even if it fell out of view.
      local x, y = math.clamp(c.x, 80, 280), math.clamp(c.y, s.camY + 140, s.camY + 540)
      fx.popup(if c.landed then look.TUMBLE else look.MISS, x, y, { color = look.DANGER, size = 26 })
      fx.burst("burst", x, y, { count = 14, color = look.DANGER })
      fx.shake(7, 0.35)
      fx.hitstop(HITSTOP)
      hurt = 1
      if s.lives == 1 then setBanner(look.LAST_HEART, look.DANGER, "heart") end
    end
  elseif k == "heartback" then
    setBanner(look.HEART_BACK, look.DANGER, "heart")
    local bx, by = s:bird()
    fx.burst("stars", bx, by, { count = 8, color = look.DANGER })
  elseif k == "newkind" then
    setBanner(string.format(look.NEW, look.SNACKS[c.kind].name), look.GOLD, nil)
    local hx, hy = s:heldPos()
    if hx then fx.burst("sparkles", hx, hy, { count = 14, color = look.SPARKLE }) end
  elseif k == "warn" then
    setBanner(if c.kind == "gust" then look.GUST else look.TILT, look.WHITE, if c.dir > 0 then "right" else "left")
  elseif k == "collapse" then
    fx.shake(10, 0.5)
    -- Paper bits fly as the plate tips.
    fx.burst("confetti", W / 2, sim.PLATE_TOP, { count = 24, color = look.CONFETTI })
    -- No hearts left to show while the plate tips.
    ui.hud({ lives = false })
  end
end

local function cues(playing)
  local list = s.cues
  if #list == 0 then return end
  s.cues = {}
  if playing then sound.cues(list) end
  for _, c in list do juice(c, playing) end
end

-- ---- The round ----------------------------------------------------------------------

-- The plate has tipped: the score, the card, and how it ended.
local function finish()
  finished = true
  local key = if round.daily then "best_daily" else "best"
  local best = save.get(key)
  local was = if type(best) == "number" then best else 0
  if s.score > was then save.set(key, s.score) end
  sound.over(s.score > was and was > 0)
  print(string.format("over at %.1fs: %d snacks, tower %.1f m (%s)", s.t, s.score, s.bestHeight / 100, lastCause))
  round.over(nil, lastCause)
end

-- A careful player's drop (the debug autopilot): over the top of the tower
-- (nudged toward the middle), never into a twist or onto a moving tower.
local function pilot(p)
  local h = s.held
  if not h or h.age < 0.5 or not p or s:falling() or s.twist.phase ~= "calm" then return end
  local top = s:towerTop()
  if abs(p.lx - (top + (W / 2 - top) * 0.35)) < 4 then s:drop() end
end

-- Debug picks, once love.load has run.
local function applyPicks()
  applied = true
  if LIVES > 0 then s.lives = math.min(LIVES, sim.LIVES) end
  if TWIST_NOW and s.twist.phase == "calm" then s.twist.left = 1 end
end

-- Debug: the careful autopilot builds the tower fast, quietly.
local function build()
  for _ = 1, 15 do
    local p = if s.held and not s:falling() then s:predict() else nil
    pilot(p)
    s:step(1 / 60)
    if s.score >= BUILD or s.ending then break end
  end
  s.cues = {}
  idle = 0
end

function love.update(dt)
  time += dt
  if round.title then
    -- The title card's bird stacks by itself, a little sloppily.
    s:autopilot(pred)
    s:step(dt)
    cues(false)
    if s.over or s.t > ATTRACT_ROUND then newSim() end
  elseif not round.ended then
    if not applied then applyPicks() end
    if BUILD > s.score and not s.ending then
      build()
    else
      idle += dt
      if AUTO then
        pilot(pred)
        idle = 0
      end
      s:step(dt)
      cues(true)
    end
    round.score = s.score
    round.lives = max(s.lives, 1)
    if s.ending and s.ending >= sim.ENDING - CARD_EARLY and not finished then finish() end
  else
    -- Under the game-over card the world steps on, with no input.
    s:step(dt)
    cues(false)
  end
  -- (The ghost shows once the bird has swooped in with the snack.)
  pred = if s.ending or not s.held or s.held.age < 0.2 then nil else s:predict()
  shared.sim, shared.pred = s, pred
  sound.update(dt, if round.title then nil else s)
  if banner then
    banner.t += dt
    if banner.t > 1.6 then banner = nil end
  end
  hurt = max(hurt - dt * 2.5, 0)
  camera:lookAt(W / 2 + (if round.title then TITLE_SHIFT else 0), s.camY + 320)
end

-- ---- Drawing ------------------------------------------------------------------------

-- Until the player has dropped a few themselves, and whenever they stall.
local function showHint()
  return not round.title and not round.ended and not s.ending and s.held ~= nil and s.held.age > 0.4 and (s.drops < 3 or idle > HINT_IDLE)
end

-- A ring pulsing out of the carried snack (in the world).
local function hintRing()
  local x, y = s:heldPos()
  if not x then return end
  local k = (time * 1.4) % 1
  local r = 18 + (1 - (1 - k) ^ 3) * 34
  draw.color(look.WHITE, 0.8 * (1 - k))
  g.setLineWidth(3)
  g.circle("line", x, y, r)
end

-- The red edge: a pulse when a heart goes, and a slow throb on the last one.
local function vignette()
  local level = hurt * 0.5
  if not round.title and not round.ended and not s.ending and s.lives == 1 then
    level = max(level, 0.22 + 0.08 * sin(time * 4))
  end
  if level > 0.01 then draw.edges(look.DANGER, level, 120) end
end

-- A heart (for the heart banners), centred on (x, y).
local function heart(x, y, r)
  g.circle("fill", x - r * 0.5, y - r * 0.2, r * 0.55)
  g.circle("fill", x + r * 0.5, y - r * 0.2, r * 0.55)
  g.polygon("fill", x - r * 1.03, y - r * 0.05, x + r * 1.03, y - r * 0.05, x, y + r)
end

-- A big word in the middle of the screen for a moment ("WIND! →", "NEW: JELLY!").
local function drawBanner()
  local b = banner
  if not b then return end
  local u = min(b.t / 0.25, 1) - 1
  local pop = max(1 + 2.70158 * u ^ 3 + 1.70158 * u ^ 2, 0.01) -- back out
  local a = if b.t > 1.3 then 1 - (b.t - 1.3) / 0.3 else 1
  local _, sy = love.window.getSafeArea()
  g.push()
  g.translate(W / 2, sy + 122)
  g.scale(pop, pop)
  local w = draw.words(b.text, 0, 0, 34, b.color, a)
  if b.icon then
    local x = w / 2 + 24 + sin(time * 10) * 3
    draw.color(look.INK, a)
    if b.icon == "heart" then
      heart(x, 2, 14)
      draw.color(b.color, a)
      heart(x, 0, 11)
    else
      local d = if b.icon == "right" then 1 else -1
      for _, layer in { { look.INK, 2, 3 }, { b.color, 0, 0 } } do
        draw.color(layer[1], a)
        local o, e = layer[2], layer[3]
        g.rectangle("fill", x - 12 - e, -4 - e + o, 16 + 2 * e, 8 + 2 * e, 3)
        g.polygon("fill", x + d * 4 - d * e, -12 - e * 1.4 + o, x + d * 4 - d * e, 12 + e * 1.4 + o, x + d * (18 + e * 1.6), o)
      end
    end
  end
  g.pop()
end

function love.draw()
  draw.sky(s, time)
  camera:attach()
  draw.world(s, time, if round.ended then nil else pred)
  if showHint() then hintRing() end
  camera:detach()
  vignette()
  if round.title or round.ended then return end
  drawBanner()
  -- (A banner has the floor while it's up: the hint waits.)
  if showHint() and not banner then
    local _, by = s:bird()
    local k = 1 + 0.05 * sin(time * 6)
    g.push()
    g.translate(W / 2, by - s.camY - 56)
    g.scale(k, k)
    draw.words(look.HINT, 0, 0, 24, look.WHITE)
    g.pop()
  end
end

debug.toggle("Autopilot", function(on) AUTO = on end, false)
debug.action("Build 10 more", function() BUILD = s.score + 10 end)
debug.action("Twist now", function()
  if s.twist.phase == "calm" then s.twist.left = 0.05 end
end)
