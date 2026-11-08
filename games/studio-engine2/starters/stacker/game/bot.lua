-- How a player plays Picnic Pile, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]): it watches the landing ghost (main.lua shares the sim and its
-- prediction in game/shared.lua) and taps for the moment the ghost will be
-- over the top snack, nudged a little toward the plate's middle to
-- straighten a leaning tower, as the kit's good bot does. It waits out
-- gusts and tilts, and for the last snack to settle, rather than dropping
-- into them. It never drops anything itself; the engine adds the person:
-- reaction to each new snack, timing noise on the tap, by skill.
local W = 360
-- It commits to a tap when the ghost will be over the target this soon (s),
-- as a player watching it slide toward the top does. The engine's reaction
-- to each new snack comes on top: a slow first-timer taps late, a good
-- player (reacting in under this) on time.
local LOOK = 0.38
-- Aims this fraction of the way from the top snack's centre back toward
-- the middle.
local CORRECT = 0.35
-- Taps anyway after carrying a snack this long without a good moment.
local FED_UP = 6.0

local shared = require("shared")
local had, waited, asked = nil :: any, 0, nil :: number?

function bot(act)
  local s, pred = shared.sim, shared.pred
  if not s or not pred or s.ending then return end
  local held = s.held
  if not held then
    had = nil
    return
  end
  if held ~= had then
    had, waited, asked = held, 0, nil
  end
  waited += love.timer.getDelta()
  -- A tap asked for and not landed by then (the finger was busy): plan again.
  if asked and waited > asked then asked = nil end
  if asked or held.age < 0.35 then return end
  local w, h = love.graphics.getDimensions()
  if waited > FED_UP then
    act.tap(w / 2, h * 0.8, { size = 200 })
    asked = waited + 1
    return
  end
  -- Patient: no drops into a twist or onto a tower still moving.
  if s.twist.phase ~= "calm" or s:falling() then return end
  local top = s:towerTop()
  local target = top + (W / 2 - top) * CORRECT
  -- Where the ghost will be τ from now: the bird's sweep carried forward,
  -- plus the drift the snack keeps (sim.CARRY of the bird's speed while it falls).
  local bx = s:bird()
  local vx = s:birdVx()
  local _, sy = s:heldPos()
  local fall = math.sqrt(math.max(2 * (pred.ly - sy) / 700, 0))
  local base = pred.lx - bx - 0.2 * vx * fall
  local amp, speed, phase = s.amp, s.speed, s.phase
  local prev
  for i = 0, LOOK * 60 do
    local t = i / 60
    local ph = phase + (if s.moving then speed * t else 0)
    local x = W / 2 + amp * math.sin(ph) + base + 0.2 * (if s.moving then amp * speed * math.cos(ph) else 0) * fall
    local d = x - target
    if math.abs(d) < 1.5 or (prev and prev * d < 0) then
      act.tap(w / 2, h * 0.8, { size = 200, at = t })
      asked = waited + t + 0.8
      return
    end
    prev = d
  end
end
