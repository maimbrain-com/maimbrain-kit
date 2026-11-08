-- How a player plays Swerve, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it reads the road from the game's `race` (what's on screen: the
-- car's lane, the traffic, the roadworks, the blinkers) and taps the half
-- of the screen toward the clearest lane it can reach, looking about two
-- seconds ahead, as the kit's bot does for a good player. When that lane
-- has something beside the car, it taps just as the car clears it, as a
-- player who has seen it coming does. It never steers itself; the engine
-- adds the person: reaction, look-aways, aim and timing, by skill.
local LANE_W, CAR_L = 2.4, 1.9
local HORIZON = 2.1 -- seconds ahead it plans for
-- Half width and half length of what's on the road (main.lua's).
local HALF = { car = { 0.85, 2.0 }, truck = { 1.0, 4.2 }, cones = { 0.95, 7.0 }, oil = { 0.9, 1.6 } }

local function lane_x(l) return (l - 1) * LANE_W end

-- The race as main.lua shares it (game/shared.lua), read afresh each frame.
local shared = require("shared")
local race: any

-- For a lane: how long until what's beside the car there is behind it
-- (`busy`, 0 if nothing), and until the next thing reaches it (`ttc`,
-- huge if nothing does within the horizon). A truck blinking to move into
-- the lane counts as in it.
local function look(lane)
  local p = math.max(1, race.speed * (1 + 0.45 * race.boost_k))
  local busy, ttc = 0, math.huge
  for _, o in ipairs(race.obstacles) do
    if not o.knock and not o.passed then
      local half = HALF[o.kind]
      local here = math.abs(o.x - lane_x(lane)) < half[1] + 0.6
      if here or (o.change == lane and o.blink ~= nil) then
        local rel, closing = o.d - race.dist, math.max(1, p - o.v)
        local gap = rel - half[2] - CAR_L
        if gap <= 0 then
          busy = math.max(busy, (rel + half[2] + CAR_L - 0.3) / closing)
        elseif gap / closing < HORIZON then
          ttc = math.min(ttc, gap / closing)
        end
      end
    end
  end
  return busy, ttc
end

-- The lane to head for, and how long to wait before the tap.
local function plan()
  local here = race.lane
  local busy, ttc = {}, {}
  for l = 0, 2 do busy[l], ttc[l] = look(l) end
  if ttc[here] == math.huge and busy[here] == 0 then return here, 0 end
  local best, score, wait = here, ttc[here], 0
  for l = 0, 2 do
    local steps = math.abs(l - here)
    if steps > 0 then
      local w = busy[l]
      -- Crossing two lanes needs the middle clear while you cross.
      if steps == 2 then w = math.max(w, busy[1]) end
      -- Waiting for something beside the car: tap a moment after it's past.
      if w > 0 then w = w + 0.12 end
      local ok = w + 0.25 * steps < ttc[here] and (steps == 1 or ttc[1] > w + 0.9)
      local s = ttc[l] - 0.12 * steps - 0.5 * w
      if ok and s > score + 0.05 then best, score, wait = l, s, w end
    end
  end
  return best, wait
end

local goal     -- the lane it's heading for, once it has decided
local pending  -- a tap asked for and not landed yet: the presses before it, and when it's given up on
local last = -1 -- when the last tap was wanted

function bot(act)
  race = shared.race
  if not race or race.phase ~= "run" or race.oil > 0 then return end
  local now = round.time
  -- One tap a decision: wait for it to land before the next.
  if pending then
    if race.presses > pending.presses or now > pending.quit then pending = nil else return end
  end
  if goal == race.lane then goal = nil end
  local want, wait = plan()
  if want == race.lane then return end
  local w, h = love.graphics.getDimensions()
  local dir = want > race.lane and 1 or -1
  -- The second tap of a two-lane move follows the first: no new reaction.
  local again = goal == want
  goal = want
  -- The engine takes presses wanted within 0.25 s of each other for one.
  local at = math.max(0, wait, last + 0.26 - now)
  last = now + at
  pending = { presses = race.presses, quit = now + at + 1.2 }
  act.tap(dir < 0 and w * 0.25 or w * 0.75, h * 0.62, { size = 150, at = at, new = not again })
end
