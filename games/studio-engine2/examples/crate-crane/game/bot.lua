-- Crate Crane's bot: a player who watches the swing and lets go when the
-- block would come down on the top of the tower. It looks ahead along the
-- trolley's sweep for the next moment the drop lands there and asks for
-- the tap then; the engine adds the person's reaction, lapses and timing.
--
-- What the engine can't add is judging where a swinging block will come
-- down. Topple's bot (games/topple/src/tests.rs), the game this one is
-- measured against, misjudges each block's landing by about 26 px as a
-- first-timer, 14 as a decent player, 7 when good (an SD of 15, 8 and
-- 4 px); this policy carries the same misjudgement by preset.

local MISJUDGE = { ["first-timer"] = 26, casual = 14, good = 7, expert = 4 }
local STEP = 1 / 60
local shared = require("shared")
local crane, tower = shared.crane, shared.tower
local seen, bias, rng, gaveUp = nil :: number?, 0, nil :: any, nil :: number?

local function gauss()   -- Topple's: four uniforms, SD about 0.58
  local s = 0
  for _ = 1, 4 do s = s + rng:random() end
  return s - 2
end

local function landsAt(phase)
  local x = 180 + crane.amp * math.sin(phase)
  local vx = crane.amp * crane.speed * math.cos(phase)
  local fall = math.max(0, (tower.y - crane.h / 2) - crane.y)
  return x + vx * math.sqrt(2 * fall / shared.gravity)
end

function bot(act)
  if not crane.holding then return end
  rng = rng or love.math.newRandomGenerator(love.math.random(1, 2 ^ 30))   -- its own numbers, not the game's
  if seen ~= crane.n then
    seen, bias = crane.n, gauss() * (MISJUDGE[act.preset] or 7)
  end
  if crane.age < 0.4 then return end
  if crane.age > 6 then                     -- no good moment comes: drop it anyway, once
    if gaveUp ~= crane.n then gaveUp = crane.n act.tap(180, 420, { size = 200 }) end
    return
  end
  local target = tower.x + bias
  for k = 0, 72 do
    if math.abs(landsAt(crane.phase + crane.speed * k * STEP) - target) < 4 then
      act.tap(180, 420, { at = k * STEP, size = 200 })
      return
    end
  end
end
