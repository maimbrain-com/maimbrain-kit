-- Hill Buggy: a dune buggy over rolling hills. It's a VehicleBody3D with four
-- VehicleWheel3Ds (Godot's ray-cast car): the springs hold it up, and off a
-- crest at speed it leaves the ground and lands on them. Hold the left or
-- right of the screen to steer; let go and it drives itself down the valley.
-- Score: seconds in the air.

local abs, max, min, atan2, sqrt = math.abs, math.max, math.min, math.atan2, math.sqrt
SIDE = false   -- the debug menu's "Side view" sets it

-- Ramps along the way: hills the track runs over, every 90 m.
local ramps = {}
for i = 1, 14 do
  ramps[#ramps + 1] = { "hill", (i % 2 == 0) and 6 or -6, -60 - (i - 1) * 90, radius = 16, height = 3.6 }
end

world.sky{ atmosphere = "day", clouds = { cover = 0.35 }, disc = "sun" }
ground = Terrain{ noise = { seed = 11, ridged = 0.2 }, height = 60, detail = 2, stamps = ramps }

-- The buggy --------------------------------------------------------------------

local paint = material{ color = 0xe8572a, roughness = 0.45, metalness = 0.2 }
local dark = material{ color = 0x23262b, roughness = 0.8 }
local rubber = material{ color = 0x1a1a1c, roughness = 0.95 }
local rim = material{ color = 0xc9ced6, metalness = 0.8, roughness = 0.3 }

local function buggy(x, z)
  local car = VehicleBody3D{
    shape = { BoxShape3D{ w = 1.7, h = 0.45, d = 3.2 }, BoxShape3D{ w = 1.2, h = 0.5, d = 1.4, y = 0.45, z = 0.3 } },
    mass = 450, linearDamp = 0.08, angularDamp = 0.8,
    x = x, y = ground:heightAt(x, z) + 1.4, z = z,
  }
  Mesh{ geometry = geometry.box(1.7, 0.42, 3.2), material = paint, parent = car }
  Mesh{ geometry = geometry.box(1.25, 0.5, 1.3), material = dark, parent = car, y = 0.45, z = 0.35 }
  Mesh{ geometry = geometry.box(1.5, 0.08, 0.9), material = paint, parent = car, y = 0.18, z = -1.3, rx = -0.25 }
  -- A roll hoop behind the seat, and a bar from it to the dash.
  for _, side in ipairs({ -0.6, 0.6 }) do
    Mesh{ geometry = geometry.cylinder(0.05, 0.85, 8), material = dark, parent = car, x = side, y = 0.85, z = 0.95 }
    Mesh{ geometry = geometry.cylinder(0.045, 1.25, 8), material = dark, parent = car, x = side, y = 0.92, z = 0.4, rx = math.pi / 2 - 0.25 }
  end
  Mesh{ geometry = geometry.cylinder(0.05, 1.25, 8), material = dark, parent = car, y = 1.27, z = 0.95, rz = math.pi / 2 }
  local wheels = {}
  for _, p in ipairs({ { -0.95, -1.15 }, { 0.95, -1.15 }, { -0.95, 1.2 }, { 0.95, 1.2 } }) do
    local front = p[2] < 0
    local w = VehicleWheel3D{
      parent = car, x = p[1], y = -0.1, z = p[2],
      wheelRadius = 0.46, wheelRestLength = 0.4, suspensionTravel = 0.35,
      suspensionStiffness = 28, suspensionMaxForce = 30000,
      dampingCompression = 2.2, dampingRelaxation = 3.0,
      wheelFrictionSlip = 2.0, useAsTraction = true, useAsSteering = front,
    }
    Mesh{ geometry = geometry.cylinder(0.46, 0.34, 18), material = rubber, parent = w, rz = math.pi / 2 }
    Mesh{ geometry = geometry.cylinder(0.26, 0.36, 10), material = rim, parent = w, rz = math.pi / 2 }
    wheels[#wheels + 1] = w
  end
  return car, wheels
end

car, wheels = buggy(0, 0)
camera.fov = 62
camera:follow(car, { behind = 9, above = 3.2, lag = 0.3, lookAhead = 5 })

-- Dust behind the rear wheels while they're on the ground.
local dust = {}
for i = 3, 4 do
  dust[i] = Emitter{ preset = "dust", parent = wheels[i] :: any, y = -0.4, rate = 0, colors = { 0xcdb894, 0xa8936d } }
end

-- Driving -----------------------------------------------------------------------

local heading = 0           -- where it's driving: −z, with a wobble to weave between ramps
local steerHeld = 0
local air, airStart, best = 0, nil, 0

function love.mousepressed(x) steerHeld = (x < love.graphics.getWidth() / 2) and 1 or -1 end
function love.mousereleased() steerHeld = 0 end

local function onGround()
  local n = 0
  for _, w in ipairs(wheels) do if w:isInContact() then n = n + 1 end end
  return n
end

function love.update(dt)
  -- Its heading (radians from −z, positive left), and where the track wants it.
  local yaw = car.ry
  local steer
  if steerHeld ~= 0 then
    steer = 0.45 * steerHeld
  else
    -- Autopilot: back toward x = 0, ahead down the valley.
    local want = atan2(car.x, 40)
    local err = want - yaw
    steer = max(-0.4, min(0.4, err * 1.6 - car.vry * 0.15))
  end
  local speed = car.speed
  car.steering = steer
  car.engineForce = speed < 31 and 950 or 0
  car.brake = 0

  -- The debug menu's "Side view": the camera beside the buggy, to see it fly.
  if SIDE then
    camera.x, camera.y, camera.z = car.x + 14, car.y + 1.5, car.z + 2
    camera:lookAt(car.x, car.y, car.z)
  end

  -- Air time: from all four wheels leaving to the first landing.
  local touching = onGround()
  if touching == 0 then
    airStart = airStart or round.time
  elseif airStart then
    local t = round.time - airStart
    airStart = nil
    if t > 0.25 then
      air = air + t
      best = max(best, t)
      round.score = math.floor(air * 10) / 10
      fx.popup(string.format("%.1fs AIR", t), car.x, car.y + 2.2, car.z, { color = 0xffd23f, size = 0.45 })
      fx.shake(min(0.35, t * 0.25), 0.25)
    end
  end
  for i = 3, 4 do
    dust[i].rate = (wheels[i]:isInContact() and speed > 6) and 40 or 0
  end
end

-- The HUD: speed, air, and the frame rate as the phone sees it.
local frames, fpsT, fps, worst, missed, total = 0, 0, 60, 0, 0, 0
function love.draw()
  local dt = love.timer.getDelta()
  frames, fpsT = frames + 1, fpsT + dt
  worst = max(worst, dt)
  total = total + 1
  if dt > 1 / 50 then missed = missed + 1 end
  if fpsT >= 1 then fps, frames, fpsT = frames / fpsT, 0, 0 end
  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  love.graphics.setColor(0, 0, 0, 0.45)
  love.graphics.rectangle("fill", 8, h - 66, 150, 52, 8)
  love.graphics.setColor(1, 1, 1)
  love.graphics.print(string.format("%d km/h", math.floor(abs(car.speed) * 3.6)), 18, h - 60)
  love.graphics.print(string.format("air %.1fs", air), 18, h - 40)
  love.graphics.setColor(1, 1, 1, 0.7)
  love.graphics.print(string.format("%d fps  %.0f%% slow  %d ms", math.floor(fps + 0.5), 100 * missed / max(1, total), math.floor(worst * 1000)), w - 196, h - 34)
  if airStart then
    love.graphics.setColor(1, 0.82, 0.25)
    love.graphics.print("AIR!", w / 2 - 20, 90)
  end
end
