-- Nova Pip, the shooter starter: a vertical arcade shooter for one thumb.
-- Pip, a little neon pod with a face in its visor, flies up through waves
-- of jellies, swoopers, darters and bulbs. It fires by itself; you only
-- steer: drag anywhere and the ship moves as far as your thumb does (×1.25),
-- from where it is, never jumping to the thumb and always sitting above it.
-- The arrows, WASD, a d-pad or a stick steer too.
--
-- Something new joins every ~11 s (swoopers at 13 s, darters at 24 s,
-- bulbs at 35 s), everything speeds up every 11 s, a WARNING at 45.5 s and
-- the Gloom Queen at 48 s: Bloom, Rain and Sweep, faster below 40 %. Beat
-- her and the loop starts again, harder, with spinners. Three hearts: a hit
-- costs one (a blink of safety, and the bullets around Pip are cleared);
-- the last one goes in slow motion. Points a kill × the combo (×2 at 5 kills
-- in a chain … ×8), gems 25, grazes 10, power-ups 200, the queen 5,000.
--
-- It's the shooter kit's Nova Pip, with its art, sounds and words: all of it
-- is this starter's, for the game to replace with its own.
--
-- Make it yours:
--   game/sim.lua    the rules: the ship, the enemies (sim.KINDS), firing,
--                   the boss, power-ups, hits and the score. The tuning
--                   knobs are at the top.
--   game/waves.lua  the formations and when each enemy type joins.
--   game/look.lua   the words, the colours, and where each picture is in
--                   images/sprites.png (and where the faces go in them).
--   game/draw.lua   how everything is drawn: the art plus the code-drawn
--                   faces, grid, bullets, glows and the HUD's own pieces.
--   game/sound.lua  which sound plays for what, the mix and the music.
--   images/, sounds/, music/   the art and the sound; studio.toml's
--                   [identity] is the look of the cards and the score.
--   game/pilot.lua  how the title card's ship (and game/bot.lua) flies.
-- This file is the host: input, juice, the hint, the HUD's extras and the
-- title card's run, which plays itself.

local sim = require("sim")
local look = require("look")
local draw = require("draw")
local sound = require("sound")
local pilot = require("pilot")
local shared = require("shared")

local W, H = sim.W, sim.H
local min, max, sqrt, exp = math.min, math.max, math.sqrt, math.exp

-- ---- Feel -------------------------------------------------------------------
-- The drag hint shows until the thumb has dragged this far (units) in a
-- round, and comes back after this long without the thumb moving (s).
local HINT_UNTIL, HINT_IDLE = 60, 3.0
-- The title card's run starts this far into a round (bullets already
-- flying), is pre-run for ATTRACT_WARMUP s so its first frame is mid-action,
-- and starts over after ATTRACT_SECONDS (it reaches the queen).
local ATTRACT_FROM, ATTRACT_WARMUP, ATTRACT_SECONDS = 12, 3, 75

-- ---- Debug (the debug menu, debug/scenarios.toml) ------------------------------
AUTO = false -- the ship flies itself
SKIP_TO = 0 -- start this many seconds into the loop (45: the WARNING, then the queen)
LOOP = 0 -- start in this loop (1: loop 2, with spinners)
HEARTS = 0 -- start with this many hearts (1: the last one)

local game -- the round's sim (sim.lua), or the title card's
local thumb -- the autopilot's thumb (pilot.lua): the title card, or AUTO
local finger -- the steering touch: its id ("mouse" for a mouse)
local time, idle, moved = 0, 0, 0
local shipPunch, comboPunch, graze, edge = 1, 1, 0, 0
local finished, applied = false, false
local overT = 0 -- seconds since the game-over card came up
-- A tint over the whole screen, fading: { colour, alpha, seconds, left }.
-- (fx.flash is always strong; these are the kit's gentler ones.)
local tint = nil

local function flash(h, a, secs)
  tint = { h, a, secs, secs }
end

-- The title card's run: it plays itself, past the opening, so the card
-- shows bullets and a swarm rather than an empty sky.
local function attract()
  game = sim.Sim.new(love.math.random(1, 2 ^ 30), true)
  thumb = pilot.thumb(love.math.random(1, 2 ^ 30))
  game:skipTo(ATTRACT_FROM)
  for _ = 1, ATTRACT_WARMUP * 60 do
    thumb:drive(game, 1 / 60)
    game:step(1 / 60)
  end
  game.cues = {}
end

function love.load()
  draw.load()
  sound.load()
  if round.title then
    attract()
  else
    -- A daily round draws the day's seed here: everyone gets the same waves.
    game = sim.Sim.new(love.math.random(1, 2 ^ 30), false)
    thumb = pilot.thumb(love.math.random(1, 2 ^ 30))
    sound.start()
  end
  shared.game = game
end

-- A scenario's picks (set once love.load has run), applied on the first frame.
local function applyScenario()
  applied = true
  if SKIP_TO > 0 or LOOP > 0 then game:skipTo(SKIP_TO, LOOP) end
  if HEARTS > 0 then game.ship.hp = HEARTS end
end

debug.toggle("Autopilot", function(on) AUTO = on end, AUTO)
debug.action("The queen next", function() game:skipTo(sim.BOSS_AT - sim.WARNING_TIME - 0.5, game.loop) end)
debug.action("Shield", function() game.ship.shield = true end)
debug.action("Spread", function() game.ship.spread, game.ship.spreadT = 2, sim.POWER_TIME end)

-- ---- Input: a drag steers, from wherever it starts ---------------------------------

local function playing()
  return not round.title and not round.ended
end

local function down(id, x, y)
  if not playing() or finger then return end
  finger = id
  game:touchDown(x, y)
  shipPunch = 1.25 -- Pip perks up the moment the thumb lands
  idle = 0
end

local function move(id, x, y)
  if not playing() then return end
  if not finger then
    -- The touch that started the round is already steering.
    down(id, x, y)
    return
  end
  if id ~= finger then return end
  local g = game.grab
  if g then
    local d = sqrt((x - g.x) ^ 2 + (y - g.y) ^ 2)
    if d > 0.5 then
      idle = 0
      moved += d
    end
  end
  game:touchMove(x, y)
end

local function up(id)
  if finger == id then
    finger = nil
    game:touchUp()
  end
end

-- A finger is the mouse too: fingers come through the touch callbacks, the
-- mouse itself through the mouse ones.
function love.touchpressed(id, x, y) down(id, x, y) end
function love.touchmoved(id, x, y) move(id, x, y) end
function love.touchreleased(id) up(id) end
function love.mousepressed(x, y, _, istouch) if not istouch then down("mouse", x, y) end end
function love.mousemoved(x, y, _, _, istouch)
  if not istouch and (finger == "mouse" or love.mouse.isDown(1)) then move("mouse", x, y) end
end
function love.mousereleased(_, _, _, istouch) if not istouch then up("mouse") end end

-- ---- Juice ------------------------------------------------------------------------

-- Effects for one cue from the sim. inPlay: the round, not the title card's run.
local function juice(c, inPlay)
  local t = c.type
  if inPlay then sound.cue(c) end
  if t == "kill" then
    local col = look.ENEMY[c.kind]
    local big = c.kind == "bulb" or c.kind == "spinner"
    fx.burst("burst", c.x, c.y, { count = if big then 26 else 12, color = col })
    fx.burst("sparks", c.x, c.y, { count = if big then 18 else 8, color = { col, look.WHITE } })
    fx.burst("ring", c.x, c.y, { count = 1, color = col })
    if inPlay then
      -- Kept below the score, sized by the points.
      local size = min(14 + 3 * math.log10(c.points), 25)
      local _, sy = love.window.getSafeArea()
      fx.popup("+" .. c.points, math.clamp(c.x, 34, W - 34), max(c.y - 14, sy + 210), { color = if c.mult > 1 then look.GOLD else look.WHITE, size = size })
      if c.combo % 5 == 0 then comboPunch = 1.5 end
      fx.shake(if big then 5 else 2, if big then 0.25 else 0.12)
      if big then fx.hitstop(0.05) end
    end
  elseif t == "hit" or t == "bossHit" then
    fx.burst("sparks", c.x, c.y, { count = 2, color = 0xfff2c0 })
  elseif t == "gem" then
    fx.burst("sparkles", c.x, c.y, { count = 2, color = look.GEM })
  elseif t == "power" then
    local col = look.POWER[c.power]
    fx.burst("ring", c.x, c.y, { count = 1, color = col })
    fx.burst("sparkles", c.x, c.y, { count = 10, color = col })
    if inPlay then
      fx.popup(look.POWER_NAMES[c.power], c.x, c.y - 26, { color = col, size = 22 })
      flash(col, 0.25, 0.25)
    end
  elseif t == "heart" then
    fx.burst("ring", c.x, c.y, { count = 1, color = look.HEART })
    fx.burst("stars", c.x, c.y, { count = 8, color = look.HEART })
    if inPlay then fx.popup("+1 HEART", c.x, c.y - 26, { color = look.HEART, size = 22 }) end
  elseif t == "graze" then
    fx.burst("sparks", c.x, c.y, { count = 3, color = look.WHITE })
    if inPlay and graze <= 0 then
      graze = 1.6
      fx.popup(look.CLOSE, game.ship.x, game.ship.y - 36, { color = look.WHITE, size = 18 })
    end
  elseif t == "shieldBreak" then
    fx.burst("ring", c.x, c.y, { count = 1, color = look.SHIELD })
    fx.burst("burst", c.x, c.y, { count = 20, color = look.SHIELD })
    fx.shake(6, 0.25)
    fx.hitstop(0.06)
  elseif t == "hurt" then
    fx.burst("burst", c.x, c.y, { count = 18, color = look.DANGER })
    fx.burst("sparks", c.x, c.y, { count = 14, color = 0xffd0d0 })
    fx.shake(9, 0.3)
    fx.hitstop(0.09)
    if inPlay then
      flash(0xff3040, 0.38, 0.25)
      edge = 1
    end
  elseif t == "dying" then
    -- A longer freeze first, so the player sees what got them.
    fx.hitstop(0.3)
    fx.shake(6, 0.3)
    if inPlay then print(string.format("nova pip: last heart lost at %.1fs (%s)", game.t, game.lastHit)) end
  elseif t == "boom" then
    fx.burst("burst", c.x, c.y, { count = 8 + math.floor(c.size * 30), color = 0xff5ad0 })
    fx.burst("sparks", c.x, c.y, { count = 6 + math.floor(c.size * 24), color = 0xffd890 })
    fx.burst("ring", c.x, c.y, { count = 1, color = look.WHITE })
    fx.shake(2 + 9 * c.size, 0.2 + 0.25 * c.size)
    if c.size > 0.9 then flash(look.WHITE, 0.56, 0.2) end
  elseif t == "bossBeaten" then
    fx.hitstop(0.22)
    flash(look.WHITE, 0.7, 0.25)
    fx.shake(9, 0.4)
    fx.burst("ring", c.x, c.y, { count = 1, color = look.WHITE })
  elseif t == "bossDown" then
    fx.burst("confetti", c.x, c.y, { count = 60, color = look.CONFETTI })
    if inPlay then fx.popup("+" .. c.points, c.x, c.y + 40, { color = look.GOLD, size = 34 }) end
    fx.shake(10, 0.5)
  elseif t == "warning" then
    if inPlay then flash(0xff2030, 0.31, 0.3) end
  elseif t == "bossArrive" then
    fx.shake(7, 0.4)
  end
end

local function cues(inPlay)
  local list = game.cues
  if #list == 0 then return end
  game.cues = {}
  for _, c in list do juice(c, inPlay) end
end

-- The slow-motion death has played: the score is in, the card comes up.
local function finish()
  finished = true
  local best = save.get("best") or 0
  sound.over(game.score > best and best > 0)
  if game.score > best then save.set("best", game.score) end
  print(string.format("nova pip: over at %.1fs, score %d, %d kills, %d bosses, best chain %d (%s)",
    game.t, game.score, game.kills, game.loop, game.bestCombo, game.lastHit))
  round.over(look.HEADING, game.lastHit)
end

-- ---- The frame -------------------------------------------------------------------

function love.update(dt)
  time += dt
  graze -= dt
  shipPunch = 1 + (shipPunch - 1) * exp(-10 * dt)
  comboPunch = 1 + (comboPunch - 1) * exp(-9 * dt)
  edge = max(edge - dt * 2.5, 0)
  if tint then
    tint[4] -= dt
    if tint[4] <= 0 then tint = nil end
  end
  if round.title then
    thumb:drive(game, dt)
    game:step(dt)
    cues(false)
    if game.t > ATTRACT_SECONDS then attract() end
  elseif not round.ended then
    if not applied then applyScenario() end
    idle += dt
    game.invincible = debug.invincible()
    if AUTO then
      thumb:drive(game, dt)
      idle, moved = 0, HINT_UNTIL
    elseif input.ax ~= 0 or input.ay ~= 0 then
      -- Keys and pads steer too.
      game:nudge(input.ax, input.ay, dt)
      idle = 0
      moved += sim.KEY_SPEED * dt
    end
    game:step(dt)
    cues(true)
    round.score = game.score
    if game:alive() then
      round.lives = game.ship.hp
    else
      -- The last heart is gone: the hearts leave the HUD while Pip goes down
      -- (the round ends when the death has played, not at 0 lives).
      ui.hud({ lives = false })
    end
    sound.update(dt, game)
    if game.over and not finished then finish() end
  else
    -- Under the game-over card the world drifts on, with no input.
    overT += dt
    sound.quiet()
    game:step(dt)
    cues(false)
  end
  shared.game = game
end

-- Where Pip's wreck floats under the game-over card: from where it fell,
-- easing into the gap under the results.
local function wreck()
  local k = min(overT / 1.6, 1)
  k = k * k * (3 - 2 * k)
  local sh = game.ship
  local x = sh.x + (math.clamp(sh.x, 80, W - 110) - sh.x) * k
  local y = sh.y + (548 + 4 * math.sin(time * 1.3) - sh.y) * k
  return { x = x, y = y }
end

function love.draw()
  draw.background(game, time)
  draw.world(game, time, shipPunch, if round.ended then wreck() else nil)
  if tint then draw.tint(tint[1], tint[2] * tint[4] / tint[3]) end
  if not playing() then return end
  local sx, sy, sw = love.window.getSafeArea()
  -- The chain ring sits under the score's band (a 6-digit score reaches
  -- it), its label above the popups' floor at sy + 210.
  draw.combo(game, sx + sw - 42, sy + 150, comboPunch)
  draw.powers(game, sx + 28, sy + 152)
  draw.bossBar(game, sy + 112)
  draw.banner(game)
  if game:alive() and (moved < HINT_UNTIL or idle > HINT_IDLE) then draw.hint(game, time) end
  -- The last heart: the screen's edges pulse red.
  local last = if game:alive() and game.ship.hp == 1 then 0.22 + 0.08 * math.sin(time * 4) else 0
  draw.edges(max(last, edge * 0.5))
end
