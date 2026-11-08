-- Where the game's own buttons are (main.lua taps them, draw.lua draws
-- them, bot.lua aims at them): the picker's three towers, the upgrade
-- bubble, the next-wave button and the coin pill.

local sim = require("sim")
local look = require("look")

local layout = {}

-- The picker's three buttons around an empty pad: above it on the lower
-- half of the screen, below it near the top, kept on screen. POP, CHILL, BOOM.
function layout.pickerSlots(pad)
  local p = sim.PADS[pad]
  local y = if p[2] > 200 then p[2] - look.SLOT_OFFSET else p[2] + look.SLOT_OFFSET
  local cx = math.clamp(p[1], 16 + look.SLOT_GAP + look.SLOT_R, 344 - look.SLOT_GAP - look.SLOT_R)
  return { { cx - look.SLOT_GAP, y }, { cx, y }, { cx + look.SLOT_GAP, y } }
end

-- The upgrade bubble over (or under) a tower.
function layout.upgradeSlot(pad)
  local p = sim.PADS[pad]
  return math.clamp(p[1], 52, 308), if p[2] > 200 then p[2] - 64 else p[2] + 64
end

-- The top of the game's HUD row: under the phone's notch.
local function top()
  local _, sy = love.window.getSafeArea()
  return sy
end

-- The next-wave button (top right, under the hearts, clear of the pad
-- below it): x, y, w, h.
function layout.waveButton()
  return 238, top() + 50, 112, 44
end

-- The coin counter (top left, under the hearts): x, y, w, h.
function layout.coinPill()
  return 10, top() + 57, 92, 30
end

return layout
