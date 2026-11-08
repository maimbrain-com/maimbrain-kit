-- Puffin Hop on engine 2.0: tap (or press Space, or A on a controller) to
-- flap through the gaps between the cliffs. Touch a cliff or the sea and
-- it's over.
--
-- Engine 1.0's Puffin Hop (games/studio-engine/examples/flappy), moved up
-- by upgrade_engine and finished by hand. It keeps 1.0's look with the
-- Arcade preset (studio.toml), and its quick path: the puffin is
-- images/puffin.toml (`player.flappy`: gravity and flap are its knobs), the
-- cliffs are `hazard` things, and the sea is a hazard tile in
-- maps/level1.map.

gap = 70                    -- the opening between the cliffs (art pixels; the debug menu and a scenario change it)
local pace, drift = 1.6, -55
local W = love.graphics.getWidth()
local sfx

-- The cliffs are plain boxes (no picture), with a picture cap at each end.
kind("pipe", { w = 14, h = 16, color = 9, behaviour = "hazard" })

local function cliffs()
  local mid = 64 + love.math.random() * 104
  local top, bottom = mid - gap / 2, mid + gap / 2
  local x = W + 10
  spawn("pipe", { x = x, y = top / 2, h = top, vx = drift })
  spawn("cap", { x = x, y = top - 2.5, vx = drift })
  local scorer = spawn("pipe", { x = x, y = (bottom + 240) / 2, h = 240 - bottom, vx = drift })
  scorer.data.scorer, scorer.data.mid = true, mid
  spawn("cap", { x = x, y = bottom + 2.5, vx = drift })
end

function love.load()
  Tilemap{ map = "level1" }
  music.play("theme")
  sfx = { point = love.audio.newSource("point", "static") }
  cliffs()
  timer.every(pace, cliffs)
end

-- A point for every gap the puffin gets through.
on("tick", "pipe", function(p)
  local puffin = all("puffin")[1]
  if p.data.scorer and not p.data.passed and puffin and p.x < puffin.x then
    p.data.passed = true
    round.score = round.score + 1
    sfx.point:clone():play()
    if round.score % 5 == 0 then speedUp(1.06) end
  end
end)

-- A cliff or the sea takes the puffin's one life (the round ends with
-- studio.toml's [text] over); the flap and hurt sounds play by themselves.
on("hurt", function(puffin)
  fx.shake(4)
  fx.flash(7)
  fx.hitstop(0.07)
  haptic("fail")
end)

-- The title card plays by itself: a flap whenever the puffin sinks below
-- the next gap's middle.
function love.update(dt)
  if not round.title then return end
  local puffin = all("puffin")[1]
  if not puffin then return end
  local aim, nearest = 128, math.huge
  for _, p in ipairs(all("pipe")) do
    if p.data.scorer and p.x > puffin.x - 16 and p.x < nearest then aim, nearest = p.data.mid + 4, p.x end
  end

  if puffin.y > aim and puffin.vy > 0 then puffin.vy = -165 end
end

debug.number("Gap", gap, function(v) gap = v end, 40, 120, 2)
