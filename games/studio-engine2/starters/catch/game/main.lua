-- The catch starter: apples fall from the top; drag the basket (or use the
-- arrows, A and D, or a controller's stick or d-pad) to catch every one. A
-- golden apple is worth 5. Miss one and the round is over.
--
-- Its look is the starter's: the orchard (images/backdrop.png), the basket
-- (images/player.png), the apples (item, bonus), the sounds and the
-- identity in studio.toml. Make all of it the game's own, in DESIGN.md's
-- look; the rules are here.

local W, H = 360, 640
local GROUND = 580
local SPEED = 340     -- the catcher's top speed with keys or a stick

-- The difficulty (tuned with the difficulty bot to the kit's targets: a
-- first-timer's round lasts 20-45 s, a good player's 60-180 s).
local START_FALL = 130      -- how fast apples fall at first (units a second)
local DROP_EVERY = 0.75     -- seconds between apples at the start speed
local RAMP_EVERY = 8        -- apples caught between speed-ups
local RAMP = 1.04           -- each speed-up: this much faster, and apples come this much sooner
local BONUS_CHANCE = 0.12   -- the share of golden apples

local player, items, wait, fall, caught, flash, sfx
local stick, pad = 0, 0

local function drop()
  local kind = "item"
  if love.math.random() < BONUS_CHANCE then kind = "bonus" end
  local item = Sprite{ image = kind, tag = kind, x = 40 + love.math.random() * (W - 80), y = -20, vy = fall }
  items[#items + 1] = item
end

function love.load()
  Layer{ image = "backdrop", parallax = 0, order = -10 }
  player = Sprite{ image = "player", tag = "player", x = W / 2, y = GROUND - 20 }
  items, wait, fall, caught, flash = {}, 0.6, START_FALL, 0, 0
  sfx = {
    catch = love.audio.newSource("catch", "static"),
    bonus = love.audio.newSource("bonus", "static"),
    miss = love.audio.newSource("miss", "static"),
  }
end

local function touching(a, b)
  return math.abs(a.x - b.x) < (a.w + b.w) / 2 and math.abs(a.y - b.y) < (a.h + b.h) / 2
end

local function catch(item)
  if item.tag == "bonus" then
    round.score = round.score + 5
    sfx.bonus:clone():play()
  else
    round.score = round.score + 1
    sfx.catch:clone():play()
  end
  caught = caught + 1
  if caught % RAMP_EVERY == 0 then fall = fall * RAMP end
  player.frame = 2
  flash = 0.15
end

local function lowest()
  local best
  for _, item in ipairs(items) do
    if not best or item.y > best.y then best = item end
  end
  return best
end

function love.update(dt)
  if round.ended then return end

  -- Steer: a finger or the mouse leads; keys, the d-pad and the stick move.
  local dir = stick + pad
  if love.keyboard.isDown("left", "a") then dir = dir - 1 end
  if love.keyboard.isDown("right", "d") then dir = dir + 1 end
  local target = player.x + math.max(-1, math.min(1, dir)) * SPEED * dt
  if love.mouse.isDown(1) then target = love.mouse.getX() end
  if round.title then
    local next = lowest()   -- the title card plays by itself
    if next then target = next.x end
  end
  local step = SPEED * 2 * dt
  player.x = math.max(32, math.min(W - 32, player.x + math.max(-step, math.min(step, target - player.x))))

  if flash > 0 then
    flash = flash - dt
    if flash <= 0 then player.frame = 1 end
  end

  wait = wait - dt
  if wait <= 0 then
    drop()
    wait = DROP_EVERY * START_FALL / fall
  end

  for i = #items, 1, -1 do
    local item = items[i]
    if touching(item, player) then
      catch(item)
      item:destroy()
      table.remove(items, i)
    elseif item.y > GROUND - 10 then
      item:destroy()
      table.remove(items, i)
      if item.tag == "item" and not round.title then
        sfx.miss:play()
        haptic("fail")
        round.over("MISSED!", "missed")
        return
      end
    end
  end
end

function love.gamepadaxis(_, axis, value)
  if axis == "leftx" then
    if math.abs(value) > 0.25 then stick = value else stick = 0 end
  end
end

function love.gamepadpressed(_, button)
  if button == "dpleft" then pad = pad - 1 elseif button == "dpright" then pad = pad + 1 end
end

function love.gamepadreleased(_, button)
  if button == "dpleft" then pad = pad + 1 elseif button == "dpright" then pad = pad - 1 end
end
