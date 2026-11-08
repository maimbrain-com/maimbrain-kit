-- Bun Run, the endless runner starter: a bread bun in red sneakers runs by
-- itself through a meadow, and you keep it alive. Tap (or Space, Up, W, or
-- a controller's A or B) to jump, hold to jump higher, tap again in the air
-- for a double jump. Crates, pits, tall stacks, hungry gulls, bees, low
-- gulls and logs join one family every 15 s, the run speeds up every 12 s,
-- coins arc along the ideal jumps, and the sky goes day → dusk → night →
-- dawn as you go. A score is a point a meter, 5 a coin, 10 a narrow escape.
--
-- It's the runner kit's Bun Run, with its own pixel art, sounds, words and
-- look: all of it is this starter's, for the game to replace with its own.
--
-- Make it yours:
--   game/sim.lua    the rules: the jump, the course generator, the tumble.
--                   The tuning knobs are at the top (speed, gaps, the jump,
--                   the difficulty curve).
--   game/look.lua   the words (headings, hints, banners), the sky's colours
--                   through the day, and how everything is drawn.
--   game/sound.lua  which sound plays for what, and the mix.
--   images/, sounds/, music/   the art and the sound; studio.toml's
--                   [identity] is the look of the cards and the score.
-- This file is the host: input, juice, hints, the HUD and the title card's
-- run, which plays itself.

local sim = require("sim")
local look = require("look")
local sound = require("sound")

local HERO_X, PX_PER_M = sim.HERO_X, sim.PX_PER_M
local GROUND_Y, W = look.GROUND_Y, look.W
local g = love.graphics
local floor, min, max, sin, exp = math.floor, math.min, math.max, math.sin, math.exp

-- The tap hint comes back after this long without a tap (s).
local HINT_IDLE = 3.5
-- A CLOSE! call slows time to this for this long (s): savour the escape.
local SLOWMO = 0.35
local SLOWMO_TIME = 0.3

-- Debug (the debug menu, debug/scenarios.toml): the autopilot plays the
-- round, or plays it fast until SKIP_TO seconds in.
AUTOPLAY = false
SKIP_TO = 0

local run -- the round's run (sim.lua), or the title card's
local pilot -- the autopilot
local time, play_t, idle, taught, held, slowmo, faster_t, punch, finished
local banner -- a new family's banner: { kind, age }
local puffs -- dust: { x, y, vx, vy, age, life, r }
local sky

-- The title card's run: it plays itself, past the warm-up, so the card
-- shows jumps rather than open ground.
local function attract()
  local seed = love.math.random(1, 2 ^ 30)
  run = sim.Sim.new(seed)
  pilot = sim.Autopilot.new(seed + 1, 0.05)
  for _ = 1, 170 do
    pilot:drive(run)
    run:step(1 / 60)
    run:generate()
  end
  run.cues = {}
end

function love.load()
  look.load()
  sound.load()
  time, play_t, idle, taught, held = 0, 0, 0, false, 0
  slowmo, faster_t, punch, finished = 0, 9, 0, false
  puffs = {}
end

-- The first frame: the title card's run, or the round's. (round.title is
-- only set from the first frame on.)
local function begin()
  if round.title then
    attract()
  else
    local seed = love.math.random(1, 2 ^ 30)
    run = sim.Sim.new(seed)
    pilot = sim.Autopilot.new(seed + 1, 0.02)
    sound.start()
  end
end

-- ---- Input: a press jumps, holding holds -------------------------------------

local function down()
  if round.title or round.ended then return end
  if not run then begin() end -- (the tap that starts the round is its first jump)
  held += 1
  if play_t > 0.4 then taught = true end -- (the tap that started the round doesn't count)
  idle = 0
  run:press()
end

local function up()
  if round.title or not run then return end
  held = max(held - 1, 0)
  if held == 0 then run:release() end
end

local function isJump(key)
  return key == "space" or key == "up" or key == "w"
end

-- A finger is the mouse too: fingers come through touchpressed (all of
-- them), the mouse itself through mousepressed.
function love.touchpressed() down() end
function love.touchreleased() up() end
function love.mousepressed(_, _, _, istouch) if not istouch then down() end end
function love.mousereleased(_, _, _, istouch) if not istouch then up() end end
function love.keypressed(key) if isJump(key) then down() end end
function love.keyreleased(key) if isJump(key) then up() end end
function love.gamepadpressed(_, button) if button == "a" or button == "b" then down() end end
function love.gamepadreleased(_, button) if button == "a" or button == "b" then up() end end

-- ---- Juice ------------------------------------------------------------------------

local function puff(x, y, n, spread, speed)
  local r = love.math.random
  for _ = 1, n do
    puffs[#puffs + 1] = {
      x = x + (r() * 2 - 1) * spread, y = y - r() * 4,
      vx = -speed * (0.4 + r() * 0.4) + (r() * 2 - 1) * 40, vy = -(15 + r() * 45),
      age = 0, life = 0.3 + r() * 0.25, r = 3 + r() * 3,
    }
  end
end

-- Effects (and sound) for one cue from the run. playing: the round, not
-- the title card's run.
local function juice(c, playing)
  sound.cue(c, playing)
  local kind = c.type
  local hero_y = GROUND_Y - run.hero.h - 20
  local function sx(x) return HERO_X + (x - run.dist) end
  if kind == "jump" then
    if run.hero.h < 2 then puff(HERO_X, GROUND_Y, 5, 8, run.speed) end
  elseif kind == "double" then
    fx.burst("ring", HERO_X, hero_y, { count = 1, color = look.WHITE })
    fx.burst("sparkles", HERO_X, hero_y, { count = 6, color = 0xfff2b0 })
  elseif kind == "land" then
    local n = floor(2 + c.speed / 160)
    puff(HERO_X - 10, GROUND_Y, n, 6, run.speed)
    puff(HERO_X + 10, GROUND_Y, n, 6, run.speed * 0.3)
    if c.speed > 650 and playing then fx.shake(2, 0.15) end
  elseif kind == "coin" then
    fx.burst("sparkles", sx(c.x), GROUND_Y - c.y, { count = 4, color = 0xfff2b0 })
    if playing then punch = 1 end
  elseif kind == "arcdone" then
    -- Every coin of an arc: the jump was spot on.
    fx.burst("stars", sx(c.x), GROUND_Y - c.y, { count = 6, color = look.GOLD })
    if playing then fx.popup(look.NICE, sx(c.x) + 10, GROUND_Y - c.y - 26, { color = look.GOLD, size = 20 }) end
  elseif kind == "close" then
    fx.popup(look.CLOSE .. " +" .. sim.CLOSE_POINTS, HERO_X + 16, hero_y - 44, { color = look.GOLD, size = 26 })
    fx.burst("sparkles", HERO_X, hero_y, { count = 8, color = look.WHITE })
    if playing then
      slowmo = SLOWMO_TIME
      fx.flash({ 1, 1, 1, 0.22 }, 0.25)
    end
  elseif kind == "faster" and playing then
    fx.popup(look.FASTER, W / 2, GROUND_Y - 120, { color = look.WHITE, size = 26 })
    faster_t = 0
  elseif kind == "newkind" and playing then
    banner = { kind = c.kind, age = 0 }
  elseif kind == "milestone" and playing then
    -- (Above a new family's banner when there's one.)
    local _, sy = love.window.getSafeArea()
    fx.popup(c.m .. " m", W / 2, if banner then 108 else sy + 190, { color = look.WHITE, size = 22 })
    fx.burst("confetti", HERO_X, hero_y - 40, { count = 26, color = look.CONFETTI })
  elseif kind == "hit" then
    local hx, hy = sx(c.x), GROUND_Y - c.y
    fx.hitstop(0.1)
    fx.shake(9, 0.4)
    fx.burst("stars", hx, hy, { count = 9, color = look.GOLD })
    fx.burst("burst", hx, hy, { count = 10, color = 0xfff2d0 })
    fx.flash({ 1, 1, 1, 0.44 }, 0.25)
  elseif kind == "fall" then
    fx.shake(4, 0.25)
  elseif kind == "bounce" then
    puff(HERO_X, GROUND_Y, 6, 14, 0)
  elseif kind == "stuck" then
    fx.shake(3, 0.2)
    puff(HERO_X + 20, GROUND_Y, 8, 16, 0)
  end
end

-- The tumble is over: the score, and the heading that says how it ended.
local function finish()
  finished = true
  local score = run:score()
  local pit = run.dying and run.dying.cause == "pit"
  local best = save.get("best") or 0
  sound.over(score > best and best > 0)
  if score > best then save.set("best", score) end
  print(string.format("over at %.1fs, %d m, score %d (%s)", run.t, run.meters, score, if pit then "a pit" else "a bonk"))
  round.over(if pit then look.HEADING_PIT else look.HEADING_BONK, if pit then "pit" else run.dying.cause)
end

-- ---- The frame -----------------------------------------------------------------

local function cues(playing)
  local list = run.cues
  if #list == 0 then return end
  run.cues = {}
  for _, c in list do juice(c, playing) end
end

function love.update(dt)
  if not run then begin() end
  time += dt
  if round.title then
    pilot:drive(run)
    run:step(dt)
    run:generate()
    cues(false)
    if run.over then attract() end
  elseif not round.ended then
    if SKIP_TO > run.t and run:alive() then
      -- Debug: the autopilot plays fast up to SKIP_TO, quietly.
      for _ = 1, 15 do
        pilot:drive(run)
        run:step(1 / 60)
        if run.t >= SKIP_TO then break end
      end
      run:generate()
      run.cues = {}
      taught = true
    else
      play_t += dt
      idle += dt
      local k = 1
      if slowmo > 0 then
        slowmo -= dt
        k = SLOWMO
      end
      if AUTOPLAY then
        pilot:drive(run)
        taught, idle = true, 0
      end
      run:step(dt * k)
      run:generate()
      cues(true)
    end
    round.score = run:score()
    if run.over and not finished then finish() end
  else
    -- Under the game-over card the world steps on, with no input.
    run:step(dt)
    cues(false)
  end
  sound.update(dt)
  if banner then
    banner.age += dt
    if banner.age > 2 then banner = nil end
  end
  faster_t += dt
  punch *= exp(-12 * dt)
  for i = #puffs, 1, -1 do
    local p = puffs[i]
    p.age += dt
    if p.age >= p.life then
      table.remove(puffs, i)
    else
      p.x += p.vx * dt
      p.y += p.vy * dt
      p.vy *= exp(-3 * dt)
    end
  end
  sky = look.skyAt(run.dist / PX_PER_M)
  look.updateBackdrop(sky, run.dist, time)
end

-- How the hero looks this frame (look.drawHero's pose).
local function pose()
  local d = run.dying
  if d then
    local face, legs = "scared", 0
    if d.cause == "pit" then
      legs = time * 3.5 -- jammed in a pit: legs kicking in the air
    elseif d.resting or d.t > 0.6 then
      face = "dizzy"
    end
    return { x = HERO_X + (d.x - run.dist), y = GROUND_Y - d.h, sx = 1, sy = 1, rot = d.rot, run = legs,
             grounded = d.resting, rising = false, face = face, t = time }
  end
  local h = run.hero
  local sx, sy, rot
  if h.grounded then
    -- Squash on landing (harder landings squash more), a bob while running.
    local k = max(1 - h.land_t / 0.16, 0) * min(h.land_speed / 700, 1.2)
    local bob = sin(h.run_phase * math.pi * 4) * 0.035
    sy = 1 - 0.28 * k + bob
    sx = 1 + 0.22 * k - bob * 0.5
    rot = 0.05
  else
    -- Stretch with speed, extra right at launch.
    local s = math.abs(math.clamp(h.vy / sim.JUMP_VY, -1, 1))
    local launch = max(1 - h.jump_t / 0.08, 0)
    sy = 1 + 0.16 * s + 0.12 * launch
    sx = 1 - 0.1 * s - 0.08 * launch
    rot = if h.flip > 0 then h.flip else math.clamp(-h.vy / 3500, -0.18, 0.18)
  end
  local nxt = run:nextObstacle()
  local near = if nxt then run:timeTo(nxt) else 9
  local face = "run"
  if h.flip > 0 then
    face = "spin"
  elseif not h.grounded then
    face = "jump"
  elseif run.close_t < 0.8 then
    face = "phew"
  elseif run.coin_t < 0.25 then
    face = "happy"
  elseif near < 0.45 and near > 0 then
    face = "worried"
  end
  if h.h < 0 then sx, sy = 1, 1 end
  return { x = HERO_X, y = GROUND_Y - h.h, sx = sx, sy = sy, rot = rot, run = h.run_phase,
           grounded = h.grounded, rising = h.vy > 0, face = face, t = time }
end

-- How to play, as it's needed: TAP TO JUMP, each family's own hint, the
-- warning for what flies in, a new family's banner.
local function hints()
  if not run:alive() then return end
  local t = run.t
  local bob = sin(time * 5) * 5
  -- The first of a new family on screen: its own hint takes over.
  local intro, hint
  for _, o in run.obstacles do
    if o.intro and not o.passed and o.kind ~= "crate" and HERO_X + sim.left(o, t) - run.dist < W - 10 then
      intro = o
      break
    end
  end
  if intro and not (intro.kind == "stack" and run.longest_hold > 0.3) then hint = look.HINTS[intro.kind] end
  -- TAP TO JUMP: a dotted hop over what's next, until they've jumped on
  -- purpose, and again when they stall (not while the right move is to
  -- stay low).
  local nxt = run:nextObstacle()
  local jumpNext = not nxt or nxt.window < 1
  if not hint and (not taught or (idle > HINT_IDLE and jumpNext)) then
    local target = if nxt then HERO_X + sim.left(nxt, t) - run.dist + nxt.w * 0.5 else HERO_X + 150
    local span = math.clamp(target - HERO_X, 80, 220) + 20
    for i = 0, 8 do
      local u = (i + (time * 2) % 1) / 9
      look.color(look.WHITE, 0.9 * (1 - u * 0.5))
      g.circle("fill", HERO_X + span * u, GROUND_Y - 24 - 400 * u * (1 - u), 3.2)
    end
    look.big(look.HINT_TAP, W / 2, GROUND_Y - 175 + bob)
    -- A thumb tapping, where the thumb is.
    local k = (time * 1.6) % 1
    local press = 1 - (1 - min(k / 0.25, 1)) ^ 2
    look.color(look.WHITE, 0.8 * (1 - press))
    g.setLineWidth(3)
    g.circle("line", W / 2, 545, 16 + 18 * press)
    look.color(look.WHITE, 0.75)
    g.circle("fill", W / 2, 545, 13 - 3 * (1 - press))
  end
  if intro and hint then
    look.big(hint, W / 2, GROUND_Y - 215 + bob)
    if intro.kind == "stack" or intro.kind == "log" then
      -- Chevrons pointing up over it: go high.
      local cx = HERO_X + sim.left(intro, t) - run.dist + intro.w * 0.5
      look.color(look.WHITE, 0.82)
      g.setLineWidth(3.5)
      for i = 0, 2 do
        local y = GROUND_Y - intro.h - 30 - i * 14 - (time * 30) % 14
        g.line(cx - 9, y + 6, cx, y - 3, cx + 9, y + 6)
      end
    end
    if intro.kind == "stack" and not run.hero.grounded and run.hero.jumps == 1 and run.double_jumps == 0 then
      look.big(look.HINT_AGAIN, HERO_X + 30, GROUND_Y - run.hero.h - 80)
    end
  end
  -- Something flying in from off screen.
  for _, o in run.obstacles do
    if (o.kind == "gull" or o.kind == "skimmer") and not o.passed then
      if HERO_X + sim.left(o, t) - run.dist > W and run:timeTo(o) < 1.4 then
        look.drawIncoming(GROUND_Y - o.y - o.h * 0.5, time)
      end
    end
  end
  -- A new family's banner.
  if banner then
    local age = banner.age
    local u = min(age / 0.3, 1) - 1
    local s = 1 + 2.70158 * u ^ 3 + 1.70158 * u ^ 2 -- back out
    local a = if age > 1.4 then 1 - (age - 1.4) / 0.4 else 1
    if a > 0 then
      g.push()
      g.translate(W / 2, 150)
      g.scale(s, s)
      look.ribbon(look.NAMES[banner.kind], a)
      g.pop()
    end
  end
end

function love.draw()
  camera:attach()
  look.drawWorld(run, sky, pose(), time, puffs, faster_t)
  camera:detach()
  if round.title or round.ended then return end
  -- The coin count, at the right inside the safe area, under the score
  -- (the HUD's: a long score reaches the right edge).
  local sx, sy, sw = love.window.getSafeArea()
  look.coinPill(run.coins_got, sx + sw - 12, sy + 96, 1 + 0.35 * punch)
  hints()
end

debug.toggle("Autopilot", function(on) AUTOPLAY = on end, false)
debug.action("Skip ahead 15 s", function() SKIP_TO = run.t + 15 end)
