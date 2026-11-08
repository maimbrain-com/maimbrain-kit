-- Neon Dash on engine 2.0: tap (or Space, Up or A) to jump, and hold it
-- for a higher jump. Crates and drones end the run; coins are worth 5.
--
-- Engine 1.0's Neon Dash (games/studio-engine/examples/runner), moved up by
-- upgrade_engine and finished by hand. It keeps 1.0's look with the Arcade
-- preset (studio.toml), and its quick path: the runner is
-- images/runner.toml (`player.runner`), the road is maps/street.map, and
-- crates and drones are `hazard` things, coins `pickup` ones.

local pace = 100   -- how fast the street comes at you (art pixels a second)
local W = love.graphics.getWidth()
local dist, applied = 0, 1
speed = 1          -- the world's speed: up 8 % every 8 s (a scenario sets it)

local function nextThing()
  local r = love.math.random()
  if r < 0.5 then
    spawn("crate", { x = W + 12, y = 104, vx = -pace })
  elseif r < 0.75 then
    spawn("drone", { x = W + 12, y = 74, vx = -pace * 1.15 })
  else
    for i = 0, 2 do spawn("coin", { x = W + 12 + i * 14, y = 70, vx = -pace }) end
  end
  timer.after((0.75 + love.math.random() * 0.9) / speed, nextThing)
end

function love.load()
  Tilemap{ map = "street" }
  music.play("theme")
  timer.after(1.0, nextThing)
  timer.every(8, function() speed = speed * 1.08 end)
end

function love.update(dt)
  -- speedUp moves the things already out there too.
  if speed ~= applied then
    speedUp(speed / applied)
    applied = speed
  end
  -- The title card plays by itself: it jumps what's coming.
  if round.title then
    local runner = all("runner")[1]
    if runner and runner.vy == 0 then
      for _, c in ipairs(all("crate")) do
        if c.x > runner.x and c.x - runner.x < 26 then runner.vy = -290 end
      end
    end
  end
  -- A point for every tile run.
  if round.ended then return end

  local before = math.floor(dist / 16)
  dist = dist + pace * speed * dt
  round.score = round.score + math.floor(dist / 16) - before
end

on("pickup", function(runner, coin) fx.popup("+5", coin.x, coin.y - 8) end)

-- A crate or a drone takes the runner's one life (the round ends with
-- studio.toml's [text] over); the jump, pickup and hurt sounds play by themselves.
on("hurt", function(runner)
  fx.shake(5)
  fx.flash(7)
  fx.hitstop(0.1)
  fx.burst("burst", runner.x, runner.y)
  haptic("fail")
end)

debug.action("Crate now", function() spawn("crate", { x = W + 12, y = 104, vx = -pace }) end)
