-- How a player plays Nova Pip, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it reads the round from game/shared.lua (the ship, the bullets,
-- the enemies: what's on screen) and keeps a thumb on the glass where it
-- puts the ship somewhere safe, lined up under the lowest enemy, chasing
-- pickups (game/pilot.lua plans it, as the kit's bot does). It never moves
-- the ship itself: the engine adds the person (reaction, look-aways, how
-- fast and how well a thumb lands), by skill.

local shared = require("shared")
local pilot = require("pilot")
local sim = require("sim")

-- What each player takes in (`act.preset`, Decided 94): a first-timer
-- reads bullets as the kit's first-timer does (not ahead, only near ones);
-- everyone else as the kit's good player (0.25 s ahead, within 160 units),
-- and the engine's hand (reaction, lapses, how the thumb steers) makes the
-- rest of the difference. `margin` is the room kept around bullets, and
-- `thumbSpeed` the ship's speed it plans with. (Reading as the kit's
-- decent player made the casual one die at 63 s, not the kit's 90.)
local GOOD = { anticipate = 0.25, awareness = 160, margin = 14, greed = 0.45, thumbSpeed = 300 }
local SKILLS = {
  ["first-timer"] = { anticipate = 0, awareness = 75, margin = 10, greed = 0.9, thumbSpeed = 300 },
  casual = GOOD,
  good = GOOD,
  expert = GOOD,
}
local SKILL -- the preset's, from the first frame
-- How often it looks again (s), and how big a change of plan (units of
-- ship travel) is a new decision, which costs a reaction: a smaller one is
-- a correction of the move under way.
local REPLAN, NEW_MOVE = 0.1, 70
-- The held thumb's target: a jump of more than this is a quick drag there
-- (Fitts' law); under it the thumb follows continuously.
local SIZE = 40

local nextPlan, goal = 0, nil
local lastX, lastY = 0, 0

function bot(act)
  SKILL = SKILL or SKILLS[act.preset] or GOOD
  local s = shared.game
  if not s or round.ended or not s:alive() then return end
  local sh = s.ship
  if not s.grab then
    -- Thumb down under the ship.
    act.hold(sh.x, math.min(sh.y + 110, sim.H - 6), { size = 60 })
    goal = nil
    return
  end
  local now = round.time
  if now >= nextPlan or not goal then
    nextPlan = now + REPLAN
    local gx, gy = pilot.plan(s, SKILL, SKILL.anticipate)
    local new = math.sqrt((gx - lastX) ^ 2 + (gy - lastY) ^ 2) > NEW_MOVE
    lastX, lastY = gx, gy
    -- Where the thumb has to be for the ship to go there (relative drag).
    local fx, fy = s:fingerFor(gx, gy)
    if fx and fy then goal = { math.clamp(fx, 2, sim.W - 2), math.clamp(fy, 2, sim.H - 2), new } end
  end
  if goal then act.hold(goal[1], goal[2], { size = SIZE, new = goal[3] }) end
end
