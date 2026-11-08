-- Lantern Path (docs/STUDIO-2.md §6.1), Phase 1's acceptance game, as the
-- design wrote it. Its art would come from generate: layers "dusk" (3, far
-- to near), walker "fox", image "lantern" (frames: dark, lit), image
-- "firefly", tile "moss"; music "dusk". Here it's placeholder art drawn by
-- a script, and synth sounds and music.

local NIGHT_SECS = 75            -- dusk to dark with no lantern lit

local fox, dark, lit, total, sfx

function love.load()
  -- the painted dusk, far to near; each wraps as the camera runs right
  Layer{ image = "dusk_0", parallax = 0.1, repeatX = true, order = -30 }
  Layer{ image = "dusk_1", parallax = 0.35, repeatX = true, order = -20 }
  Layer{ image = "dusk_2", parallax = 0.7, repeatX = true, order = -10 }

  local level = Tilemap{ map = "path1", tiles = "moss" }
  local sx, sy = level:marker("start")
  fox = Walker{ image = "fox", height = 46, x = sx, y = sy, tag = "fox",
                behaviour = "player.platformer", knobs = { speed = 200, jump = 480, gravity = 900 } }
  camera:follow(fox, { lag = 0.12, lead = 90, bounds = level })

  sfx = { light = love.audio.newSource("light", "static"),
          chime = love.audio.newSource("chime", "static") }
  dark, lit, total = 0, 0, count("lantern")
  ui.hud{ score = false }          -- the lanterns are the HUD (love.draw)
  music.play("dusk")
end

on("overlap", "fox", "lantern", function(_, lantern)
  if lantern.data.lit then return end
  lantern.data.lit = true
  lantern:play("lit")
  lit = lit + 1
  round.score = lit
  dark = math.max(0, dark - 0.25)  -- each lantern pushes the night back
  fx.burst("sparkles", lantern.x, lantern.y - 20, { count = 24, color = 0xffd27a })
  sfx.light:setPitch(1 + lit * 0.05)
  sfx.light:clone():play()
  haptic("success")
  if lit == total then round.over("THE PATH IS LIT", "lit", true) end
end)

on("overlap", "fox", "firefly", function(_, fly)
  fly:destroy()
  dark = math.max(0, dark - 0.05)
  fx.popup("+light", fly.x, fly.y - 12, { color = 0xfff3b0 })
  sfx.chime:clone():play()
end)

on("hurt", function(thing)       -- thorns: the platformer behaviour takes a life
  fx.shake(6, 0.2)
  fx.flash(0xff4040, 0.1)
end)

on("leave", "fox", function()    -- fell into a gap
  round.over("LOST IN THE DARK", "fell")
end)

function love.update(dt)
  if round.title or round.ended then return end
  dark = math.min(1, dark + dt / NIGHT_SECS)
  if dark >= 1 then round.over("NIGHT FELL", "dark") end
end

function love.draw()
  local w, h = love.graphics.getDimensions()
  -- the night, over the world (nodes at order 0 or less are drawn before this)
  love.graphics.setColor(0.04, 0.06, 0.19, dark * 0.85)
  love.graphics.rectangle("fill", 0, 0, w, h)

  -- the lanterns lit, and the night still to come, inside the safe area
  -- (a round's, so not on the title card)
  if round.title then return end
  local sx, sy, sw = love.window.getSafeArea()
  for i = 1, total do
    if i <= lit then love.graphics.setColor(1, 0.82, 0.48) else love.graphics.setColor(0.23, 0.27, 0.4) end
    love.graphics.circle("fill", sx + 24 + (i - 1) * 20, sy + 22, 7)
  end
  love.graphics.setColor(1, 1, 1, 0.25)
  love.graphics.rectangle("fill", sx + sw - 124, sy + 18, 100, 8, 4)
  love.graphics.setColor(1, 0.95, 0.69, 0.9)
  love.graphics.rectangle("fill", sx + sw - 124, sy + 18, 100 * (1 - dark), 8, 4)
end
