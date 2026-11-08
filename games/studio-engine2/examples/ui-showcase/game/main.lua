-- The UI kit's showcase: a shop between waves over a dusk field, with
-- every widget in the identity. A hint points at the bell; ringing it
-- feeds the combo. At 4 s a countdown runs; at 6 s the wave's stats card.
-- (Frames for docs/STUDIO-2.md §9.5's contact sheet: `mb play` at 2.5 s.)

local W, H = 360, 640

-- The world: sky, hills, a bell to ring.
Shape{ x = W / 2, y = H / 2, w = W, h = H, color = 0x2a3550 }
Shape{ x = W / 2, y = H - 80, w = W, h = 220, color = 0x3d5a4a }
Shape{ shape = "circle", x = 70, y = H - 150, radius = 90, color = 0x4c6b55 }
Shape{ shape = "circle", x = 300, y = H - 130, radius = 110, color = 0x456450 }
bell = Shape{ shape = "circle", x = 270, y = H - 118, radius = 22, color = 0xe2b33c, tag = "bell" }
Shape{ x = 270, y = H - 150, w = 6, h = 30, color = 0x2b2b2b }

-- The shop: a panel of widgets.
local px, py = W / 2, 300
ui.panel{ x = px, y = py, w = 316, h = 300 }
ui.pill{ text = "WAVE 3", icon = "flag", x = px, y = py - 122, tone = "accent" }
ui.icon("coin", { x = px - 120, y = py - 78, size = 26, tone = "gold" })
gold = Text{ text = "1,240", x = px - 100, y = py - 92, size = 22, color = ui.color("text") }
ui.pill{ text = "x2", x = px + 116, y = py - 78 }

ui.bar{ x = px, y = py - 40, w = 260, h = 14, value = 0.72 }
ui.bar{ x = px, y = py - 14, w = 260, h = 10, value = 0.35, tone = "bad" }

ui.ring{ x = px - 80, y = py + 46, r = 36, value = 0.65, text = "65" }
ui.ring{ x = px, y = py + 46, r = 36, value = 0.3, icon = "clock", tone = "good" }
for i, name in ipairs({ "heart", "gem", "bolt", "shield" }) do
  ui.icon(name, { x = px + 62 + ((i - 1) % 2) * 34, y = py + 28 + math.floor((i - 1) / 2) * 34, size = 26, tone = i == 1 and "bad" or "plain" })
end

local log = {}
buy = ui.button{ text = "BUY", icon = "plus", x = px - 70, y = py + 116, w = 132, h = 52, tap = function() log[#log + 1] = "buy" end }
skip = ui.button{ text = "SKIP", kind = "secondary", x = px + 58, y = py + 116, w = 108, h = 52, tap = function() log[#log + 1] = "skip" end }
ui.button{ icon = "gear", kind = "ghost", x = px + 136, y = py - 122, w = 44, h = 44 }

-- The combo, on the HUD; the bell feeds it.
combo = ui.combo{ window = 3 }
for i = 1, 6 do combo:hit() end
round.score = 1240

-- The hint follows the bell as it sways.
ui.hint{ text = "TAP THE BELL", at = bell }

function love.mousepressed(x, y)
  if (x - bell.x) ^ 2 + (y - bell.y) ^ 2 < 40 * 40 then
    combo:hit()
    round.score = round.score + 10 * combo.multiplier
  end
end

local t, counted, shown = 0, false, false
function love.update(dt)
  t = t + dt
  bell.x = 270 + math.sin(t * 2) * 12
  if t > 4 and not counted then
    counted = true
    ui.countdown(3, function() log[#log + 1] = "go" end)
  end
  if t > 6.2 and not shown then
    shown = true
    ui.stats{ title = "Wave 3 clear", rows = { { "Coins", 1240 }, { "Accuracy", 0.87, "percent" }, { "Time", 42.3, "time" }, { "Hits taken", 2, bad = true } }, rank = true, next = { "THE WARDEN", "he doesn't miss" } }
  end
end
