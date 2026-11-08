-- game/bot.lua: hold a finger toward the nearest wisp, veering away from the
-- goblin when it's close, and step round anything the knight has stuck on.
-- The game steers toward the finger from where the knight is on the screen
-- (90 units is full speed), forward being away from the camera.
local side, sideUntil, lastX, lastZ, still = 1, -1, 0, 0, 0

-- It reads the knight, the goblin and the wisps (a set of the wisps' areas)
-- from game/shared.lua; scene.all doesn't list 3D nodes yet.
local shared = require("shared")
function bot(act)
  local knight, goblin, wisps = shared.knight, shared.goblin, shared.wisps
  if not knight or round.title or round.ended then return end

  -- The nearest wisp.
  local best, wx, wz = math.huge, nil, nil
  for w in pairs(wisps) do
    local d = (w.x - knight.x) ^ 2 + (w.z - knight.z) ^ 2
    if d < best then best, wx, wz = d, w.x, w.z end
  end
  if not wx then act.release() return end
  local dx, dz = wx - knight.x, wz - knight.z
  local d = math.sqrt(dx * dx + dz * dz)
  dx, dz = dx / d, dz / d

  -- Away from the goblin, harder the closer it is.
  if goblin then
    local gx, gz = knight.x - goblin.x, knight.z - goblin.z
    local g = math.sqrt(gx * gx + gz * gz)
    if g < 6 then
      local push = (6 - g) / 6 * 2
      dx, dz = dx + gx / g * push, dz + gz / g * push
      local m = math.sqrt(dx * dx + dz * dz)
      dx, dz = dx / m, dz / m
    end
  end

  -- Stuck on a trunk or a bank: go sideways a moment.
  local moved = math.sqrt((knight.x - lastX) ^ 2 + (knight.z - lastZ) ^ 2)
  lastX, lastZ = knight.x, knight.z
  still = moved < 0.02 and still + 1 or 0
  if still > 20 then side, sideUntil, still = -side, round.time + 0.7, 0 end
  if round.time < sideUntil then dx, dz = -dz * side, dx * side end

  -- Into the camera's frame (forward is from the camera to the knight).
  local fx, fz = knight.x - camera.x, knight.z - camera.z
  local f = math.sqrt(fx * fx + fz * fz)
  if f < 0.01 then return end
  fx, fz = fx / f, fz / f
  local rx, rz = -fz, fx
  local sx, sy = dx * rx + dz * rz, -(dx * fx + dz * fz)
  local kx, ky = camera:project(knight.x, knight.y, knight.z)
  local w, h = love.graphics.getDimensions()
  act.hold(clamp(kx + sx * 110, 10, w - 10), clamp(ky + sy * 110, 10, h - 10), { size = 80 })
end
