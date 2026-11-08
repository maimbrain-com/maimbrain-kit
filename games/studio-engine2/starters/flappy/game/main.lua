-- The flappy starter: tap (or press Space, or A on a controller) to flap
-- through the gaps between the walls. Touch a wall or the floor and it's
-- over. The floor is maps/level1.map's hazard tiles; the walls are a class
-- (classic) that hump.timer makes every few seconds; flux tilts the player.
--
-- Its look is the starter's: the sky (images/backdrop.png), the chick
-- (images/player.png), the pillars' colours (PILLAR, below) and caps, the
-- sea (tiles/hazard.png), the sounds and the identity in studio.toml. Make
-- all of it the game's own, in DESIGN.md's look; the rules are here.

local W, H = 360, 640
local GRAVITY = 1000      -- how hard the chick falls (units a second²)
local FLAP = 360          -- how fast a flap sends it up
local PACE, DRIFT = 1.6, -140   -- seconds between walls, and how fast they come (units a second)
local WALL = 28

-- The difficulty (tuned with the difficulty bot to the kit's targets: a
-- first-timer's round lasts 20-45 s, a good player's 60-180 s). Gaps start
-- wide and close up as you pass them; each gap is at most SHIFT above or
-- below the last, and that grows too.
local GAP_START = 290     -- the first gap's height (units)
local GAP_MIN = 160       -- the smallest it gets
local GAP_SHRINK = 2      -- how much smaller each gap is than the one before
local SHIFT_START = 60    -- how far the second gap may be above or below the first
local SHIFT_MAX = 220     -- the most it gets
local SHIFT_GROW = 4      -- how much more each gap may move than the one before
local PILLAR, EDGE, SHINE = 0x8fe0c8, 0x4a3a6a, 0xd2f6ea   -- the pillars' mint, outline and shine

local level, player, score, walls, sfx

-- A wall pair: two pillars (each an outline, its colour and a shine) with a
-- cap at each gap end, drifting left. The pillars are tagged "wall"
-- (game/bot.lua finds them by it).
local Wall = Object:extend()

function Wall:new(x, mid, gap)
  local top, bottom = mid - gap / 2, mid + gap / 2
  local floor = H - 32
  self.parts = {}
  for _, span in ipairs({ { 0, top }, { bottom, floor } }) do
    local y, h = (span[1] + span[2]) / 2, span[2] - span[1]
    table.insert(self.parts, Shape{ shape = "rect", x = x, y = y, w = WALL + 4, h = h, color = EDGE, vx = DRIFT })
    table.insert(self.parts, 1, Shape{ shape = "rect", x = x, y = y, w = WALL, h = h, color = PILLAR, vx = DRIFT, tag = "wall" })
    table.insert(self.parts, Shape{ shape = "rect", x = x - WALL / 2 + 6, y = y, w = 4, h = h, color = SHINE, vx = DRIFT })
  end
  table.insert(self.parts, Sprite{ image = "cap", x = x, y = top - 6, vx = DRIFT })
  table.insert(self.parts, Sprite{ image = "cap", x = x, y = bottom + 6, vx = DRIFT })
  self.top, self.bottom = top, bottom
end

function Wall:x() return self.parts[1].x end

-- Whether a circle at (x, y) touches either box or cap.
function Wall:hits(x, y, r)
  local wx = self:x()
  if math.abs(x - wx) > WALL / 2 + 2 + r then return false end
  return y - r < self.top or y + r > self.bottom
end

function Wall:destroy()
  for _, p in ipairs(self.parts) do p:destroy() end
end

-- The next pair: its gap a little smaller, and somewhere near the last one.
local last_mid, made
local function addWall()
  local gap = math.max(GAP_MIN, GAP_START - made * GAP_SHRINK)
  local shift = math.min(SHIFT_MAX, SHIFT_START + made * SHIFT_GROW)
  local lo, hi = 150, H - 32 - 150
  local mid = math.max(lo, math.min(hi, last_mid + (love.math.random() * 2 - 1) * shift))
  walls[#walls + 1] = Wall(W + 30, mid, gap)
  last_mid, made = mid, made + 1
end

function love.load()
  Layer{ image = "backdrop", parallax = 0, order = -10 }
  level = Tilemap{ map = "level1" }
  local sx, sy = level:marker("start")
  player = Sprite{ image = "player", tag = "player", x = sx, y = sy, gravity = GRAVITY, order = 1 }
  score = Text{ text = "0", font = "Press Start 2P", size = 28, x = W / 2, y = 60, align = "center", color = 0x5a2a6a, order = 2 }
  ui.hud{ score = score }   -- the HUD's score is this Text: the engine writes the value in
  walls, last_mid, made = {}, (H - 32) / 2, 0
  sfx = {
    flap = love.audio.newSource("flap", "static"),
    point = love.audio.newSource("point", "static"),
    hurt = love.audio.newSource("hurt", "static"),
  }
  addWall()
  timer.every(PACE, addWall)
end

local function flap()
  if round.ended then return end
  player.vy = -FLAP
  player:play("flap", { once = true, restart = true })
  flux.to(player, 0.1, { rotation = -0.45 })
  if not round.title then sfx.flap:clone():play() end
end

local function crash()
  sfx.hurt:play()
  haptic("fail")
  fx.shake(8, 0.3)
  fx.flash(0xffffff, 0.15)
  fx.hitstop(0.08)
  fx.burst("burst", player.x, player.y, { count = 24, color = 0xdddddd })
  player.vy, player.gravity = 0, 0
  for _, w in ipairs(walls) do
    for _, p in ipairs(w.parts) do p.vx = 0 end
  end
  round.over("BONK!", "crashed")
end

function love.update(dt)
  score.visible = not round.title   -- the title card has the title there
  if round.ended then return end

  -- The title card plays by itself: flap when falling below the next gap.
  if round.title then
    local next = walls[1]
    for _, w in ipairs(walls) do
      if w:x() > player.x - WALL then next = w break end
    end
    local aim = next and (next.top + next.bottom) / 2 + 30 or H / 2
    if player.y > aim and player.vy > 0 then flap() end
  end

  -- Nose down as it falls.
  if player.vy > 200 then player.rotation = math.min(1.2, player.rotation + 3 * dt) end

  for i = #walls, 1, -1 do
    local w = walls[i]
    if not w.passed and w:x() < player.x then
      w.passed = true
      round.score = round.score + 1
      score.scale = 1.4
      flux.to(score, 0.25, { scale = 1 })
      if not round.title then sfx.point:clone():play() end
    end
    if w:x() < -WALL then
      w:destroy()
      table.remove(walls, i)
    elseif w:hits(player.x, player.y, 12) and not round.title then
      return crash()
    end
  end

  if player.y < 0 then player.y, player.vy = 0, 0 end
  if level:solidAt(player.x, player.y + 14) then
    if round.title then player.vy = -FLAP else return crash() end
  end
end

function love.keypressed(key)
  if key == "space" or key == "up" or key == "w" then flap() end
end

function love.mousepressed() flap() end

function love.gamepadpressed(_, button)
  if button == "a" or button == "b" then flap() end
end
