-- The runner starter: tap (or Space, Up or A) to jump, and hold it for a
-- higher jump. Blocks and flyers end the run; a bonus is worth 5. The
-- ground is maps/level1.map; what comes at the player is a class (classic)
-- that hump.timer sends, and bump says what the player touches. Flyers are
-- drawn in love.draw with anim8; the dust at the player's feet is an Emitter.
--
-- Its look is the starter's: the desert at sundown (images/sky.png and
-- images/scrub.png, two Layers scrolling at their own speeds), the fox
-- (images/player.png), the cacti (block), vultures (flyer), gold nuggets
-- (bonus), the sand (tiles/ground.png), the sounds and the identity in
-- studio.toml. Make all of it the game's own, in DESIGN.md's look; the
-- rules are here.

local W, H = 640, 360
local GRAVITY = 1800        -- how hard the fox falls (units a second²)
local JUMP = 600            -- how fast a jump sends it up
local CUT = 0.45            -- letting go early keeps this much of the rise
local PACE = 220            -- how fast the world comes at you at first (units a second)

-- The difficulty (tuned with the difficulty bot to the kit's targets: a
-- first-timer's run lasts 20-45 s, a good player's 60-180 s). The world
-- speeds up every few seconds, and what comes is SPACING to SPACING +
-- SPREAD apart at any speed, so it comes sooner as you go faster.
local FIRST_AFTER = 2       -- seconds before the first thing comes
local SPACING = 280         -- the least room between two things (units)
local SPREAD = 200          -- up to this much more, at random
local RAMP_EVERY = 8        -- seconds between speed-ups
local RAMP = 1.08           -- each speed-up: this much faster
local TOP_SPEED = 2.2       -- the fastest it gets (times PACE)
local FLYERS = 0.25         -- the share of flyers
local BONUSES = 0.25        -- the share of bonuses (the rest are blocks)

local level, player, dust, world, things, speed, dist, bonus, grounded, sfx
local ground                -- the top of the ground the player last stood on
local flyerImage, flyerAnim, font, sky, scrub

-- Something coming at the player, a node Rust moves. Blocks and bonuses
-- are Sprites; a flyer is an unseen box, drawn in love.draw with anim8.
-- Each is tagged with its kind (game/bot.lua finds them by it).
local Thing = Object:extend()
Thing.w, Thing.h = 24, 24

function Thing:new(kind, x, y, vx)
  self.kind = kind
  if kind == "flyer" then
    self.node = Shape{ shape = "rect", w = self.w, h = self.h, visible = false, tag = kind, x = x, y = y, vx = vx }
  else
    self.node = Sprite{ image = kind, tag = kind, x = x, y = y, vx = vx }
  end
  world:add(self, self:box())
end

function Thing:pos()
  return self.node.x, self.node.y
end

function Thing:box()
  local x, y = self:pos()
  return x - self.w / 2, y - self.h / 2, self.w, self.h
end

function Thing:update(dt)
  world:update(self, self:box())
end

function Thing:stop()
  self.node.vx = 0
end

function Thing:remove()
  world:remove(self)
  self.node:destroy()
end

local Bonus = Thing:extend()
Bonus.value = 5

local function groundTop()
  -- The top of the ground tile under the player's feet.
  return level.y + math.floor((player.y + 16 - level.y) / 32) * 32
end

local function send()
  local r = love.math.random()
  local x, vx = W + 20, -PACE * speed
  local floor = ground - 16   -- where the player runs, even while it's in the air
  if r < FLYERS then
    things[#things + 1] = Thing("flyer", x, floor - 60, vx * 1.15)
  elseif r < FLYERS + BONUSES then
    for i = 0, 2 do things[#things + 1] = Bonus("bonus", x + i * 28, floor - 68, vx) end
  else
    things[#things + 1] = Thing("block", x, floor, vx)
  end
  timer.after((SPACING + love.math.random() * SPREAD) / (PACE * speed), send)
end

function love.load()
  -- The desert: far mesas and nearer scrub, each wrapping as it scrolls.
  sky = Layer{ image = "sky", parallax = 0, repeatX = true, order = -30 }
  scrub = Layer{ image = "scrub", parallax = 0, repeatX = true, order = -20 }
  level = Tilemap{ map = "level1", y = H - 12 * 32 }
  local sx, sy = level:marker("start")
  player = Sprite{ image = "player", tag = "player", x = sx, y = sy, gravity = GRAVITY, order = 1 }
  player:play("run")
  dust = Emitter{ preset = "dust", rate = 14, parent = player, y = 16, colors = 0xe8c48a }
  world = bump.newWorld(64)
  things, speed, dist, bonus, grounded = {}, 1, 0, 0, true
  ground = groundTop()
  flyerImage = love.graphics.newImage("flyer")
  flyerAnim = anim8.newAnimation(anim8.newGrid(32, 32, flyerImage:getWidth(), flyerImage:getHeight())("1-2", 1), 0.08)
  font = love.graphics.newFont("Rye", 28)
  ui.hud{ score = false }   -- the distance is drawn here, in love.draw
  sfx = {
    jump = love.audio.newSource("jump", "static"),
    pickup = love.audio.newSource("pickup", "static"),
    hurt = love.audio.newSource("hurt", "static"),
  }
  timer.after(FIRST_AFTER, send)
  timer.every(RAMP_EVERY, function() speed = math.min(TOP_SPEED, speed * RAMP) end)
end

local function jump()
  if round.ended or not grounded then return end
  player.vy = -JUMP
  grounded = false
  if not round.title then sfx.jump:clone():play() end
end

-- Letting go early cuts the jump short.
local function release()
  if player.vy < 0 then player.vy = player.vy * CUT end
end

local function crash()
  sfx.hurt:play()
  haptic("fail")
  fx.shake(6, 0.25)
  fx.flash(0xffffff, 0.12)
  fx.hitstop(0.1)
  fx.burst("burst", player.x, player.y, { count = 24, color = 0xffd28a })
  for _, t in ipairs(things) do t:stop() end
  player.vy, player.gravity = 0, 0
  dust.emitting = false
  player:play("jump")
  round.over("CRASHED!", "crashed")
end

function love.update(dt)
  if round.ended then return end
  sky.x = sky.x - PACE * speed * 0.05 * dt
  scrub.x = scrub.x - PACE * speed * 0.3 * dt

  -- Standing on the ground, or falling onto it.
  if player.vy >= 0 and level:solidAt(player.x, player.y + 16) then
    ground = groundTop()
    player.y, player.vy = ground - 16, 0
    if not grounded then player:play("run") end
    grounded = true
  elseif grounded and not level:solidAt(player.x, player.y + 17) then
    grounded = false
  end
  if not grounded then player:play("jump") end
  dust.emitting = grounded

  -- The title card plays by itself: it jumps the blocks.
  if round.title then
    for _, t in ipairs(things) do
      local x = t:pos()
      if t.kind == "block" and x > player.x and x - player.x < 70 then jump() end
    end
    if player.vy < 0 and player.y < groundTop() - 120 then release() end
  end

  dist = dist + PACE * speed * dt
  round.score = math.floor(dist / 32) + bonus

  for i = #things, 1, -1 do
    local t = things[i]
    t:update(dt)
    if t:pos() < -40 then
      t:remove()
      table.remove(things, i)
    end
  end

  -- What the player touches: a bonus is collected; anything else ends the run.
  local hits, len = world:queryRect(player.x - 8, player.y - 14, 16, 28)
  for i = 1, len do
    local t = hits[i]
    if t:is(Bonus) then
      bonus = bonus + t.value
      local x, y = t:pos()
      fx.popup("+" .. t.value, x, y - 16)
      if not round.title then sfx.pickup:clone():play() end
      t:remove()
      for j, u in ipairs(things) do if u == t then table.remove(things, j) break end end
    elseif not round.title then
      return crash()
    end
  end
end

function love.draw()
  for _, t in ipairs(things) do
    if t.kind == "flyer" then
      local x, y = t:pos()
      flyerAnim:draw(flyerImage, x - 16, y - 16)
    end
  end
  if round.title or round.ended then return end   -- the cards say the score
  -- The distance, top right inside the safe area, with a drop shadow.
  local sx, sy, sw = love.window.getSafeArea()
  love.graphics.setFont(font)
  love.graphics.setColor(0.24, 0.12, 0.16, 0.8)
  love.graphics.printf(tostring(round.score), sx, sy + 18, sw - 22, "right")
  love.graphics.setColor(1, 0.94, 0.8)
  love.graphics.printf(tostring(round.score), sx, sy + 16, sw - 24, "right")

end

function love.keypressed(key)
  if key == "space" or key == "up" or key == "w" then jump() end
end

function love.keyreleased(key)
  if key == "space" or key == "up" or key == "w" then release() end
end

function love.mousepressed() jump() end
function love.mousereleased() release() end

function love.gamepadpressed(_, button)
  if button == "a" or button == "b" then jump() end
end

function love.gamepadreleased(_, button)
  if button == "a" or button == "b" then release() end
end
