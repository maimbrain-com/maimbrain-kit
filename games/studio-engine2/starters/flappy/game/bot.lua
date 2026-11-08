-- How a player plays Flappy, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it sees the game through the scene's tags (main.lua's locals
-- aren't visible here) and flaps when the player would fall below the next
-- gap. Each flap is the next beat of a rhythm (new = false): the engine
-- adds timing noise, not a reaction, and the rest of the person by skill.
local H, FLOOR = 640, 640 - 32
-- How fast a flap sends it up: seen as the game plays (main.lua's FLAP).
local flap = 0

function bot(act)
  local player = scene.all("player")[1]
  if not player then return end
  flap = math.max(flap, -player.vy)
  local gravity = player.gravity
  -- The next wall not yet passed, and the gap in it.
  local x
  for _, w in ipairs(scene.all("wall")) do
    if w.x + w.w / 2 > player.x - 12 and (not x or w.x < x) then x = w.x end
  end
  local top, bottom = 0, FLOOR
  if x then
    for _, w in ipairs(scene.all("wall")) do
      if w.x == x then
        if w.y < H / 2 then top = w.y + w.h / 2 else bottom = w.y - w.h / 2 end
      end
    end
  end
  -- Where it'll be by the time a flap lands (a press takes a while), and
  -- how high a flap takes it: flap to stay just under the gap's middle.
  local lead = 0.15
  local soon = player.y + player.vy * lead + 0.5 * gravity * lead * lead
  local rise = gravity > 0 and flap * flap / (2 * gravity) or 0
  local aim = math.min((top + bottom) / 2 + 10, bottom - 40)
  if soon > aim and soon - rise > top + 14 then
    act.tap(180, 400, { size = 200, new = false })
  end
end
