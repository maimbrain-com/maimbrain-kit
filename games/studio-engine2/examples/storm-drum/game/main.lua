-- Storm Drum (Phase 3d's test game): stones fall on the drum's beat into
-- three lanes; tap a side (or the arrows) to change lane. A stone that hits
-- costs a life; one that passes scores. Eight bars in the storm comes: the
-- music goes to its "storm" zone at the next phrase, and from then stones
-- come on every beat, not every other.
--
-- Everything on the beat comes from on("beat"), the music's grid in game
-- time, so a replay lands every stone on the same frame. The music follows
-- the game: intensity with the score, danger with the nearest stone in
-- your lane (the tension kit, the heartbeat, the narrowing), and a near
-- miss when a stone falls through the lane you just left.

local W, H = 360, 640
local LANES = { 90, 180, 270 }
local PLAYER_Y = 560
local FALL_BEATS = 4          -- a stone takes this many beats from the top to the player
local STORM_AT = 8            -- the bar the storm is asked for

local lane, player, ring, stones, lines, sky, glow, storm, label

local function stoneSpeed()
  -- Lands on the beat FALL_BEATS after it's dropped, whatever the tempo.
  local spb = 60 / math.max(music.bpm, 1)
  return (PLAYER_Y + 40) / (FALL_BEATS * spb)
end

function love.load()
  sky = Shape{ w = W, h = H, x = W / 2, y = H / 2, color = 0x101428, order = -20 }
  glow = Shape{ w = W, h = H, x = W / 2, y = H / 2, color = 0x3a4cff, alpha = 0, order = -19 }
  lines = {}
  for i, x in ipairs(LANES) do
    lines[i] = Shape{ shape = "line", points = { 0, -H / 2, 0, H / 2 }, line = 2, x = x, y = H / 2, color = 0x2a3a6a, order = -10 }
  end
  ring = Shape{ shape = "circle", radius = 30, line = 3, x = LANES[2], y = PLAYER_Y, color = 0x9fd8ff, alpha = 0 }
  player = Shape{ shape = "circle", radius = 18, x = LANES[2], y = PLAYER_Y, color = 0xffd25a, tag = "player" }
  label = Text{ text = "", size = 18, align = "center", x = W / 2, y = 90, color = 0xffffff, alpha = 0 }
  lane, stones, storm = 2, {}, false
  music.play("drum", { volume = 0.8 })
end

local function drop(at_lane)
  local s = Shape{ shape = "rect", w = 46, h = 30, corner = 8, x = LANES[at_lane], y = -40, color = 0xb7c3dc, tag = "stone" }
  s.vy = stoneSpeed()
  s.data.lane = at_lane
  stones[#stones + 1] = s
end

-- The beat: the lanes flash, the drum's downbeat lights the sky, and stones drop.
on("beat", function(beat, bar)
  for _, l in ipairs(lines) do l.alpha = 1 end
  ring.alpha, ring.scale = 0.9, 1
  if beat == 1 then glow.alpha = storm and 0.35 or 0.18 end
  if bar >= 2 and (storm or beat % 2 == 1) then
    drop(love.math.random(1, 3))
  end
end)

on("bar", function(bar)
  if bar == STORM_AT and not storm then
    music.go("storm")
  end
end)

local left, leftAt = 0, -1   -- the lane just left, and when

local function move(d)
  local to = math.max(1, math.min(3, lane + d))
  if to ~= lane then left, leftAt, lane = lane, round.time, to end
end

function love.keypressed(key)
  if key == "left" or key == "a" then move(-1) elseif key == "right" or key == "d" then move(1) end
end

function love.mousepressed(x)
  if not round.title then move(x < W / 2 and -1 or 1) end
end

function love.update(dt)
  -- The music's zone tells the look: the storm darkens the sky.
  if music.zone == "storm" and not storm then
    storm = true
    sky.color = 0x1a0c1e
    label.text, label.alpha = "THE STORM", 1
  end
  label.alpha = math.max(0, label.alpha - dt * 0.6)
  for _, l in ipairs(lines) do l.alpha = math.max(0.25, l.alpha - dt * 3) end
  glow.alpha = math.max(0, glow.alpha - dt * 0.8)
  ring.alpha = math.max(0, ring.alpha - dt * 2.5)
  ring.scale = ring.scale + dt * 1.5
  ring.x = player.x

  if round.title then
    -- The title card dodges by itself: away from the next stone in its lane.
    for _, s in ipairs(stones) do
      if s.alive and s.data.lane == lane and s.y > 300 and s.y < PLAYER_Y then move(lane == 3 and -1 or 1) end
    end
  end
  player.x = player.x + (LANES[lane] - player.x) * math.min(1, dt * 18)

  -- The danger: the nearest stone falling in your lane.
  local near = 1e9
  for i = #stones, 1, -1 do
    local s = stones[i]
    if s.y > PLAYER_Y + 60 then
      if not round.ended then round.score = round.score + 1 end
      s:destroy()
      table.remove(stones, i)
    elseif s.data.lane == lane and math.abs(s.y - PLAYER_Y) < 34 and math.abs(s.x - player.x) < 40 then
      hurt(player)
      fx.shake(6, 0.25)
      s:destroy()
      table.remove(stones, i)
    else
      if s.data.lane == lane and s.y < PLAYER_Y then near = math.min(near, PLAYER_Y - s.y) end
      -- A stone through the lane you left a moment ago: a near miss.
      if s.data.lane == left and not s.data.passed and s.y > PLAYER_Y then
        s.data.passed = true
        if round.time - leftAt < 0.6 then music.nearMiss() end
      end
    end
  end
  music.mood{ intensity = math.min(round.score / 40, 1), danger = math.max(0, 1 - near / 420) }
end
