-- Tally: Phase 4c's test game (docs/STUDIO-2.md §9.5). Every tap climbs a
-- floor of a tower, and the round ends three seconds in. It keeps progress
-- the server verifies (runs played, the highest floor, a tower unlocked at
-- floor 5), checkpoints the climb so a later session carries on, posts its
-- taps to a second board, and has a daily board, where what each floor is
-- worth is the day's.

local ROUND = 3
local floor, taps, worth

function love.load()
  -- A climb left mid-round carries on from its checkpoint.
  local cp = progress.resume()
  floor = cp and cp.floor or 0
  taps = 0
  -- The course: what a floor is worth, from the round's seed (the day's in a daily round).
  worth = love.math.random(1, 3)
  round.score = floor * worth
end

function love.mousepressed()
  if round.title or round.ended then return end
  floor = floor + 1
  taps = taps + 1
  round.score = floor * worth
  round.post("taps", taps)
  progress.checkpoint({ floor = floor })
  -- Unlocks only outside the daily round, whose course is everyone's.
  if floor >= 5 and not round.daily then progress.set("tower", true) end
end

function love.update(dt)
  if round.title or round.ended then return end
  if round.time >= ROUND then
    progress.set("runs", (progress.get("runs") or 0) + 1)
    if floor > (progress.get("floor") or 0) then progress.set("floor", floor) end
    progress.clear()
    round.over("TIME", "time", true)
  end
end

function love.draw()
  love.graphics.setColor(0.2, 0.25, 0.4)
  for i = 1, math.min(floor, 12) do
    love.graphics.rectangle("fill", 150, 600 - i * 40, 60, 36)
  end
  love.graphics.setColor(1, 1, 1)
  love.graphics.print("floor " .. floor .. " x" .. worth .. (round.daily and "  daily" or ""), 20, 80)
end
