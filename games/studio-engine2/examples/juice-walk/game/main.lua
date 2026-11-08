-- Juice Walk: Phase 3c's showcase. A hauler on a mech gait stomps along
-- the meadow (footsteps from its gait, a servo hum from its speed) with a
-- smaller one walking its clip beside it (footsteps from its planted
-- feet). Crates drop ahead: each lands with a clang as loud as it fell,
-- a punch and a shake, and three in a row make a combo. The last, big one
-- comes down in slow motion, with the world muffled.

local ground = Terrain{}
world.sky{ atmosphere = "day", clouds = { cover = 0.3 } }

local SPEED = 2.2
local big = Model{ model = "hauler", gait = "mech", footsteps = "stomp", hum = { sound = "servo", idle = 0.3, full = 3 } }
local small = Model{ model = "hauler", scale = 0.55, plantFeet = true, footsteps = "step", x = 4.2, z = -6 }
small:play("Walk", { speed = 1.4 })
big.y = ground:heightAt(0, 0)
small.y = ground:heightAt(small.x, small.z)
-- A three-quarter view from the left, a little ahead, kept over the ground.
local function frame()
  local cx, cz = big.x - 3, big.z + 15
  camera.x, camera.z = cx, cz
  camera.y = math.max(big.y + 6, ground:heightAt(cx, cz) + 2)
  camera:lookAt(big.x - 0.5, big.y + 2.5, big.z - 8)
end
frame()

local clang = love.audio.newSource("clang", "static")
local ding = love.audio.newSource("combo", "static")
local box = geometry.box(1.2, 1.2, 1.2)
local wood = material{ color = 0xb07a45, roughness = 0.8 }
local gold = material{ color = 0xe8b830, roughness = 0.4, metalness = 0.6 }

local comboText = Text{ text = "", x = 180, y = 96, size = 44, align = "center", color = 0xffd23f }
local combo, comboAt = 0, -10
local crates = {}
local drops = { { at = 1.4, x = -3.2 }, { at = 2.1, x = -0.8 }, { at = 2.8, x = -2 }, { at = 4.6, x = -1.5, big = true } }
local stepsHeard = 0
-- When Lua hears each footfall (the frame after it lands): the log, against the sound log.
big.stepped:connect(function(foot) stepsHeard = stepsHeard + 1 print(string.format("stepped big %d %.3f", foot, round.time)) end)
small.stepped:connect(function(foot) print(string.format("stepped small %d %.3f", foot, round.time)) end)

local function drop(d)
  local z = big.z - 10
  local c = Mesh{ geometry = box, material = d.big and gold or wood, x = d.x, z = z, y = ground:heightAt(d.x, z) + (d.big and 26 or 14), gravity = 18 }
  if d.big then c.scale = 1.8 end
  crates[#crates + 1] = { node = c, big = d.big, landed = false, slowed = false }
end

local function land(c)
  local n = c.node
  local fall = -n.vy
  n.gravity, n.vy = 0, 0
  n.y = ground:heightAt(n.x, n.z) + 0.6 * n.scale
  clang:setPosition(n.x, n.y, n.z)
  audio.impact(clang, fall, 20)
  fx.punch(n, { scale = 0.35 })
  fx.burst("dust", n.x, n.y - 0.5, n.z, { count = c.big and 40 or 16 })
  fx.shake(c.big and 0.5 or 0.15, c.big and 0.4 or 0.2)
  if round.time - comboAt < 1.5 then combo = combo + 1 else combo = 1 end
  comboAt = round.time
  if combo >= 2 then
    comboText.text = "x" .. combo .. " COMBO"
    fx.punch(comboText, 0.4)
    print(string.format("combo %s at %.3f", comboText.text, round.time))
    ding:play()
  end
  if c.big then
    fx.punch(big, { y = 0.5, scale = 0.08 })
    haptic("heavy")
  end
end

function love.update(dt)
  -- The big hauler stops for a breath at 3.4 s (its hum winds down) and goes on.
  local t = round.time
  local v = (t > 3.4 and t < 4.2) and 0 or SPEED
  big.z = big.z - v * dt
  big.y = ground:heightAt(big.x, big.z)
  small.z = small.z - SPEED * dt
  small.y = ground:heightAt(small.x, small.z)
  frame()
  for _, d in ipairs(drops) do
    if not d.done and t >= d.at then d.done = true drop(d) end
  end
  for _, c in ipairs(crates) do
    local n = c.node
    if not c.landed then
      local floor = ground:heightAt(n.x, n.z) + 0.6 * n.scale
      if c.big and not c.slowed and n.y - floor < 9 then
        c.slowed = true
        fx.slowmo(0.25, 0.9)
        flux.to(audio.bus("master"), 0.2, { lowpass = 700 })
        timer.after(1.6, function() flux.to(audio.bus("master"), 0.4, { lowpass = 20000 }) end)
      end
      if n.y <= floor then c.landed = true land(c) end
    end
  end
  if round.time - comboAt > 1.5 and combo > 0 then
    combo = 0
    flux.to(comboText, 0.3, { alpha = 0 }):oncomplete(function() comboText.text = "" comboText.alpha = 1 end)
  end
end

function love.draw()
  love.graphics.print(string.format("t %.2f  steps %d  hum %s", round.time, stepsHeard, tostring(big.hum)), 12, 600)
end
