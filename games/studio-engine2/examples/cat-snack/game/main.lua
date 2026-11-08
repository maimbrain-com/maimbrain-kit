-- Cat Snack on engine 2.0: drag the cat (or use the arrows, or a
-- controller) and catch every fish. Gold fish are worth 5. Miss one and the
-- round is over.
--
-- Engine 1.0's Cat Snack (games/studio-engine/examples/catch), moved up by
-- upgrade_engine and finished by hand. It keeps 1.0's look with the Arcade
-- preset (studio.toml), and its quick path: the fish fall from the F in
-- maps/level1.map, and images/cat.toml and images/fish.toml say what the
-- cat and fish do (`player.drag` and `pickup`).

local caught = 0
local sfx
goldRush = false   -- the debug menu's Gold rush (and its scenario)
fastFish = false   -- the Fast fish scenario: once, the world twice as fast

function love.load()
  Tilemap{ map = "level1" }
  music.play("theme")
  sfx = {
    chomp = love.audio.newSource("chomp", "static"),
    gold = love.audio.newSource("gold", "static"),
    miss = love.audio.newSource("miss", "static"),
  }
end

on("pickup", function(cat, fish)
  caught = caught + 1
  cat:play("chomp", { once = true, restart = true })
  timer.after(0.15, function() if cat.alive then cat:play("idle") end end)
  if fish.tag == "gold" then
    sfx.gold:clone():play()
    fx.popup("+5", fish.x, fish.y - 8)
    fx.burst("sparkles", fish.x, fish.y)
  else
    sfx.chomp:clone():play()
  end
  if caught % 8 == 0 then speedUp(1.12) end
  if goldRush or chance(0.12) then
    spawn("gold", { x = 12 + love.math.random() * 120, y = -8, vy = 70 })
  end
end)

-- A fish that reaches the grass is missed; a gold one just goes.
on("tick", "fish", function(fish)
  if fish.y > 226 then
    sfx.miss:play()
    fx.shake(3)
    fx.burst("burst", fish.x, 224)
    haptic("fail")
    fish:destroy()
    round.over("MISSED!", "missed")
  end
end)
on("tick", "gold", function(g) if g.y > 226 then g:destroy() end end)

function love.update(dt)
  if fastFish then
    fastFish = false
    speedUp(2)
  end
end

-- This game's own debug items (the Studio tab has levels, score and more).
debug.action("Gold fish now", function() spawn("gold", { x = 72, y = 0, vy = 70 }) end)
debug.toggle("Gold rush", function(on) goldRush = on end)
