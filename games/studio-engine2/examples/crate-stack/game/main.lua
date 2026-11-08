-- Crate Stack: a hook swings over a harbour; tap to drop the crate it holds.
-- Crates that land soft weld together, a weld that twists too far snaps, and
-- a crate in the sea costs a life. love.physics, as LÖVE writes it: the
-- crates' shapes follow their bodies (`crate.body = body`), so love.draw
-- only draws the rope.

local DOCK_Y, PX = 560, 64          -- 64 px a metre, LÖVE's convention
local world, hook, held, top, period
local crates, toWeld = {}, {}
local snapAt = 15000                -- a weld snaps past this twist (getReactionTorque)

local COLORS = { 0xc8743a, 0xd99a4e, 0xb85f33, 0xe0b15e }

local function swing(toRight)
  flux.to(hook, period, { x = toRight and 260 or 100 }):ease("quadinout")
    :oncomplete(function() swing(not toRight) end)
end

local function nextCrate()
  local w = 44 + love.math.random(0, 2) * 12
  held = Shape{ shape = "rect", w = w, h = 44, corner = 5, color = COLORS[love.math.random(1, #COLORS)],
                parent = hook, y = 40, tag = "crate" }
end

local function drop()
  if not held or round.title or round.ended then return end
  local c = held
  held = nil
  c.parent = nil                    -- keeps its place on screen
  local body = love.physics.newBody(world, c.x, c.y, "dynamic")
  local f = love.physics.newFixture(body, love.physics.newRectangleShape(c.w, c.h), 1)
  f:setFriction(0.8); f:setRestitution(0.05); f:setUserData(c)
  c.body = body                     -- the shape follows it
  crates[#crates + 1] = c
  timer.after(0.7, nextCrate)
end

-- Box2D's rule: no joints made inside a callback, so remember the pair.
local function beginContact(a, b, contact)
  local ca, cb = a:getUserData(), b:getUserData()
  if cb == "sea" then ca, cb = cb, ca end
  if ca == "sea" and type(cb) == "userdata" and not cb.data.sunk then
    cb.data.sunk = true
    fx.burst("burst", cb.x, DOCK_Y + 40, { count = 30, color = 0xbfe8ff })
    if not debug.invincible() then round.lives = round.lives - 1 end
  elseif type(ca) == "userdata" and type(cb) == "userdata" and not ca.data.weld then
    local vx, vy = ca.body:getLinearVelocity()
    if vx * vx + vy * vy < 260 * 260 then toWeld[#toWeld + 1] = { ca, cb, contact:getPositions() } end
  end
end

local function postSolve(a, b, contact, impulse)
  if impulse > 900 then fx.shake(math.min(6, impulse / 600), 0.12) end
end

function love.load()
  Shape{ shape = "rect", w = 360, h = 700, x = 180, y = 320, color = 0x8fd3ff, fixed = true, order = -100 }
  Shape{ shape = "rect", w = 360, h = 300, x = 180, y = 520, color = 0xb7e3ff, fixed = true, order = -99, alpha = 0.6 }

  love.physics.setMeter(PX)
  world = love.physics.newWorld(0, 9.81 * PX, true)
  world:setCallbacks(beginContact, nil, nil, postSolve)

  local dock = love.physics.newBody(world, 180, DOCK_Y + 12, "static")
  love.physics.newFixture(dock, love.physics.newRectangleShape(240, 24)):setFriction(0.9)
  Shape{ shape = "rect", x = 180, y = DOCK_Y + 12, w = 240, h = 24, color = 0x5b4636, corner = 3 }
  for _, x in ipairs({ 100, 180, 260 }) do Shape{ shape = "rect", x = x, y = DOCK_Y + 60, w = 14, h = 80, color = 0x3f3026 } end
  local sea = love.physics.newBody(world, 180, DOCK_Y + 70, "static")
  local water = love.physics.newFixture(sea, love.physics.newRectangleShape(3000, 80))
  water:setSensor(true); water:setUserData("sea")
  Shape{ shape = "rect", x = 180, y = DOCK_Y + 80, w = 3000, h = 100, color = 0x1d4e6b, alpha = 0.9, order = 10 }

  hook = Shape{ shape = "rect", w = 26, h = 10, x = 100, y = 70, color = 0x2d3640, corner = 3 }
  period, top = 1.6, DOCK_Y
  swing(true)
  nextCrate()
end

function love.mousepressed() drop() end
function love.keypressed(key) if key == "space" then drop() end end

function love.update(dt)
  world:update(dt)
  for _, w in ipairs(toWeld) do
    local a, b, x, y = w[1], w[2], w[3], w[4]
    if a.alive and b.alive and not a.data.weld and x then
      a.data.weld = love.physics.newWeldJoint(a.body, b.body, x, y)
    end
  end
  toWeld = {}

  local highest = DOCK_Y
  for _, c in ipairs(crates) do
    local j = c.data.weld
    if j and not j:isDestroyed() and j:getReactionTorque(60) > snapAt then
      j:destroy(); c.data.weld = nil
      fx.burst("sparks", c.x, c.y, { count = 12, color = 0xffe08a })
    end
    if not c.body:isAwake() and not c.data.sunk then highest = math.min(highest, c.y - c.h / 2) end
  end
  if highest < top then
    top = highest
    round.score = math.floor((DOCK_Y - top) / PX * 10) / 10
    flux.to(camera, 0.6, { y = math.min(320, top + 160) }):ease("quadout")
    flux.to(hook, 0.6, { y = top - 250 }):ease("quadout")
    period = math.max(0.7, 1.6 - (DOCK_Y - top) / 1500)
  end
end

function love.draw()
  -- The crane's rope, in the world.
  camera:attach()
  love.graphics.setColor(0.18, 0.2, 0.24)
  love.graphics.setLineWidth(3)
  love.graphics.line(hook.x, hook.y - 400, hook.x, hook.y)
  camera:detach()
  love.graphics.setColor(1, 1, 1)
  love.graphics.print(string.format("%.1f m", round.score), 14, 14)
end
