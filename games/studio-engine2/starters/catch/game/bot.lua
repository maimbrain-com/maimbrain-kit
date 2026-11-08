-- How a player plays Catch, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it sees the game through the scene's tags (main.lua's locals
-- aren't visible here) and keeps a finger under the apple that will land
-- first (a faster one can overtake a lower one). The engine adds the
-- person: reaction, aim and timing, by skill.
local GROUND = 580                  -- main.lua's

function bot(act)
  local player = scene.all("player")[1]
  if not player then return end
  local next, soonest
  for _, tag in ipairs({ "item", "bonus" }) do
    for _, thing in ipairs(scene.all(tag)) do
      local lands = (GROUND - thing.y) / math.max(thing.vy, 1)
      if lands > 0 and (not soonest or lands < soonest) then next, soonest = thing, lands end
    end
  end
  -- The basket follows the finger across: hold it where the catch will be.
  -- (The basket is its target: the apple only has to land somewhere on it.)
  act.hold(next and next.x or player.x, player.y, { size = player.w * 0.8 })
end
