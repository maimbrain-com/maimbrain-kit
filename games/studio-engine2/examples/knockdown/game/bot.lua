-- How a player plays Knockdown, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]). The Rust kit's bot (games/knockdown/src/tests.rs, bot_round):
-- aim at the standing target nearest the middle of the stack, projected to
-- the screen, once the last ball has had 0.9 s to land. It never throws
-- itself; the engine adds the person: the reaction to each new moment, the
-- time a thumb takes to get there, and where it lands, scattered by the
-- target's size on screen (a can is ~50 units across: a first-timer lands
-- within about 12 of its middle, as Rust's ±13 px).
--
-- main.lua shares the round in game/shared.lua (the bot can't see its
-- locals, and scene.all doesn't list 3D nodes).
local SETTLE = 0.9   -- seconds the last ball gets before the next throw
local knock = require("shared")

local asked = nil    -- the throw count when it last asked, and when
local askedAt = 0

-- How big the target is on screen: its narrower side, projected.
local function screenSize(t)
  local b = t.body
  local hw, hh
  if t.kind == "can" then
    hw, hh = knock.CAN_R, knock.CAN_H / 2
  elseif t.kind == "crate" then
    hw, hh = knock.CRATE / 2, knock.CRATE / 2
  else
    hw, hh = knock.PLANK_W / 2, knock.PLANK_H / 2
  end
  local x1 = camera:project(b.x - hw, b.y, b.z)
  local x2 = camera:project(b.x + hw, b.y, b.z)
  local _, y1 = camera:project(b.x, b.y - hh, b.z)
  local _, y2 = camera:project(b.x, b.y + hh, b.z)
  return math.min(math.abs(x2 - x1), math.abs(y2 - y1))
end

function bot(act)
  local k = knock
  if not k or k.phase ~= "aim" or k.ballsLeft == 0 then return end
  local throws = k.throws or 0
  -- One press per throw: wait for it to land (or give up after 2 s).
  if asked == throws and k.clock - askedAt < 2 then return end
  if k.cooldown > 0 then return end
  for _, ball in ipairs(k.balls) do
    if ball.age < SETTLE then return end
  end
  -- The standing target nearest the middle of what's standing. The press
  -- lands a reaction and a reach later, so (unlike Rust's bot, which
  -- throws the frame it decides) it waits for the stack to stop wobbling,
  -- as a player does before aiming.
  local up = {}
  local cx, cy, cz = 0, 0, 0
  local wobbling = false
  for _, t in ipairs(k.targets) do
    if not t.down then
      local b = t.body
      up[#up + 1] = t
      cx, cy, cz = cx + b.x, cy + b.y, cz + b.z
      if b.vx * b.vx + b.vy * b.vy + b.vz * b.vz > 0.0025 then wobbling = true end
    end
  end
  if wobbling and k.clock - (k.thrownAt or 0) < 2.5 then return end
  if #up == 0 then return end
  cx, cy, cz = cx / #up, cy / #up, cz / #up
  local best, bestD
  for _, t in ipairs(up) do
    local b = t.body
    local d = (b.x - cx) ^ 2 + (b.y - cy) ^ 2 + (b.z - cz) ^ 2
    if not bestD or d < bestD then best, bestD = t, d end
  end
  local b = best.body
  local sx, sy = camera:project(b.x, b.y, b.z)
  act.tap(sx, sy, { size = screenSize(best) })
  asked, askedAt = throws, k.clock
end
