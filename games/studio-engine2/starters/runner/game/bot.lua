-- How a player plays Runner, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it sees the game through the scene's tags (main.lua's locals
-- aren't visible here) and jumps each block so the top of the jump is over
-- it, holding for a full jump, a little later or earlier if a flyer would
-- pass overhead while it's up. The engine adds the person: reaction, aim
-- and timing, by skill.
local GRAVITY, JUMP = 1800, 600     -- main.lua's

-- The nearest thing of a kind not yet past the player.
local function next(kind, player)
  local best
  for _, n in ipairs(scene.all(kind)) do
    if n.x + 12 > player.x - 8 and n.vx < 0 and (not best or n.x < best.x) then best = n end
  end
  return best
end

function bot(act)
  local player = scene.all("player")[1]
  if not player then return end
  local block = next("block", player)
  if not block then return end
  local up = JUMP / GRAVITY          -- seconds to the top of a jump
  local t = (block.x - player.x) / -block.vx - up
  -- Planned a second ahead; not once it's too late.
  if t >= 1 or t < -0.15 then return end
  local flyer = next("flyer", player)
  if flyer then
    local when = (flyer.x - player.x) / -flyer.vx
    if when > t - 0.1 and when < t + 2 * up + 0.1 then
      if when + 0.1 - t < 0.2 then
        t = when + 0.1
      elseif t - (when - 2 * up - 0.1) < 0.2 then
        t = when - 2 * up - 0.1
      end
    end
  end
  act.tap(320, 180, { size = 200, at = math.max(0, t), hold = up })
end
