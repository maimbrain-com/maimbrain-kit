-- The UI kit over a 3D world: a crystal field under a dusk sky, a hint on
-- the crystal to take (it follows it through the camera as it bobs), a
-- lesson at 1 s, the HUD's combo, a pill and a meter in a corner panel,
-- an action button, and the stats card at 5 s.

local W, H = 360, 640

sky{ colors = { { 0.10, 0.12, 0.30 }, { 0.42, 0.30, 0.52 }, { 0.95, 0.62, 0.45 } } }
fog{ color = 0x2a2440, near = 18, far = 60 }
Mesh{ geometry = geometry.plane(80, 80), material = material{ color = 0x3a4a5c, roughness = 1 } }
local rocks = Mesh{ geometry = geometry.rock(1.2, 4):paint(0x6a6478) }
scatter(rocks, { count = 40, area = 40, seed = 3, scale = { 0.5, 1.6 } })

local glass = material{ color = 0x6fe0ff, emissive = 0x2a8cff, emissiveIntensity = 1.4, roughness = 0.2 }
local crystals = {}
for i, p in ipairs({ { -2.2, -6 }, { 1.4, -4.5 }, { 3.2, -9 }, { -4, -11 } }) do
  crystals[i] = Mesh{ geometry = geometry.cone(0.45, 1.6, 6), material = glass, x = p[1], y = 0.9, z = p[2] }
  crystals[i].vry = 0.8
end
Light{ color = 0x7fd8ff, intensity = 3, range = 8, x = 1.4, y = 2, z = -4.5 }

camera.x, camera.y, camera.z = 0, 3.2, 4
camera:lookAt(0, 0.8, -6)

-- The UI.
ui.hud{ score = true }
round.score = 12
combo = ui.combo{ window = 4 }
for i = 1, 3 do combo:hit() end
ui.panel{ x = 92, y = 556, w = 160, h = 104 }
ui.pill{ text = "SECTOR 7", icon = "target", x = 92, y = 526, tone = "accent" }
ui.bar{ x = 92, y = 560, w = 130, value = 0.62, tone = "good" }
ui.bar{ x = 92, y = 584, w = 130, h = 10, value = 0.3, tone = "bad" }
grab = ui.button{ text = "GRAB", icon = "gem", x = 268, y = 566, w = 140, h = 60 }
ui.ring{ x = 300, y = 470, r = 30, value = 0.4, icon = "bolt" }

local target = crystals[2]
ui.hint{ text = "TAP THE CRYSTAL", at = target }

local t, taught, shown = 0, false, false
function love.update(dt)
  t = t + dt
  target.y = 0.9 + math.sin(t * 2) * 0.25
  if t > 1 and not taught then
    taught = true
    ui.lesson{ prompt = "HOLD TO SCAN", slow = 0.3, hold = 1.5 }
  end
  if t > 5 and not shown then
    shown = true
    ui.stats{ title = "Sector clear", rows = { { "Crystals", 12 }, { "Scanned", 0.75, "percent" }, { "Time", 63.4, "time" } }, rank = "A" }
  end
end
