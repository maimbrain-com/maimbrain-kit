-- Crate Crane (docs/STUDIO-2.md §6.3): Topple (games/topple) in love.physics.
-- A crane swings blocks with faces over a rock in the sea; tap to let go.
-- A block that lands level on the tower sets in mortar at once (a weld
-- joint); one that lands crooked counts once it comes to rest. Mortar that
-- twists too far cracks, and anything that reaches the sea costs a heart.
-- The crane sweeps wider and faster as the tower grows, new shapes join
-- every few blocks, and the sky goes from day to golden hour, sunset and
-- night with the height. Everything is drawn in love.draw, as LÖVE would.

local W = 360
local ROCK_TOP, ROCK_W = 430, 168       -- the rock the tower stands on (world y)
local SEA_Y = 504                       -- the sea's surface: anything that reaches it is lost
local RAIL_Y, ROPE = 150, 64            -- the crane's rail on screen; trolley to the block's top
local TOWER_ON_SCREEN = 370             -- where the camera keeps the tower's top
local GRAVITY = 981                     -- px/s²: 9.81 m/s² at 100 px a metre
local SETTLE_TIME, SETTLE_SPEED = 0.35, 24   -- a crooked block counts once it rests this long, this still
local STICK_SPEED = 90                  -- a level block landing slower than this sets at once
local RESPAWN = 0.85                    -- the next block, after a drop
local WELD_TORQUE = 200000   -- mortar cracks past this twist (getReactionTorque, at 8 iterations)

local INK = 0x2b2135
local PALETTE = { 0xff6b6b, 0xffa94d, 0xffd43b, 0x69db7c, 0x4dabf7, 0xb197fc }
local KINDS = {
  crate  = { w = 64,  h = 44 },
  small  = { w = 44,  h = 40 },
  plank  = { w = 108, h = 22 },
  pillar = { w = 34,  h = 74 },
  wedge  = { w = 70,  h = 52 },
  wheel  = { w = 46,  h = 46 },
  pillow = { w = 66,  h = 34 },
}
local UNLOCKS = {                        -- by blocks landed: what the crane may bring
  { 0,  { "crate", "small" } },
  { 5,  { "crate", "small", "plank" } },
  { 9,  { "crate", "plank", "pillar", "pillow" } },
  { 13, { "crate", "plank", "pillar", "pillow", "wedge" } },
  { 18, { "crate", "small", "plank", "pillar", "pillow", "wedge", "wheel" } },
}
local AWKWARD = { wedge = true, wheel = true, pillar = true }

local world, sfx
local blocks, welds = {}, {}
local held                              -- the block on the hook (no body until it's let go)
local phase, drops, streak, nextColor, respawn = 0, 0, 0, 1, nil
local camTop, height, skyH, idle, clock = 0, 0, 0, 0, 0
local sinks, impacts = {}, {}           -- what the callbacks saw, handled after the step
local bias = 0                          -- the title card's crane: how sloppy this drop is

-- What a player sees, shared with game/bot.lua: the hook and the top of the tower.
local shared = require("shared")
local crane, tower = shared.crane, shared.tower

local function rgb(hex, a)
  return math.floor(hex / 65536) % 256 / 255, math.floor(hex / 256) % 256 / 255, hex % 256 / 255, a or 1
end
local function color(hex, a) love.graphics.setColor(rgb(hex, a)) end
local function mix(a, b, t)
  t = math.max(0, math.min(1, t))
  local ar, ag, ab = rgb(a)
  local br, bg, bb = rgb(b)
  return math.floor((ar + (br - ar) * t) * 255 + 0.5) * 65536 + math.floor((ag + (bg - ag) * t) * 255 + 0.5) * 256
    + math.floor((ab + (bb - ab) * t) * 255 + 0.5)
end
local function shade(c, t) return t > 0 and mix(c, 0xffffff, t) or mix(c, 0x100818, -t) end

-- The trolley's sweep: half-width and angular speed, by blocks landed.
local function sweep()
  local s = round.score
  return math.min(100, 60 + s), math.min(1.6, 0.9 + s * 0.018)
end

local function trolley()
  local amp, speed = sweep()
  return W / 2 + amp * math.sin(phase), camTop + RAIL_Y, amp * speed * math.cos(phase)
end

local function nextKind()
  if drops < 3 then return "crate" end
  local list = UNLOCKS[1][2]
  for _, u in ipairs(UNLOCKS) do if round.score >= u[1] then list = u[2] end end
  local last = blocks[#blocks] and blocks[#blocks].kind
  while true do
    local k = list[love.math.random(1, #list)]
    if not (AWKWARD[k] and k == last) then return k end
  end
end

local function spawn()
  local kind = nextKind()
  held = { kind = kind, w = KINDS[kind].w, h = KINDS[kind].h, state = "hanging", color = nextColor,
           age = 0, fear = 0, rest = 0, vx = 0, vy = 0 }
  nextColor = (nextColor + love.math.random(1, 3) - 1) % #PALETTE + 1
  crane.n = crane.n + 1
  bias = (love.math.random() - 0.5) * 34
end

local function shapeFor(kind, w, h)
  if kind == "wedge" then return love.physics.newPolygonShape(-w / 2, h / 2, w / 2, h / 2, 0, -h / 2) end
  if kind == "wheel" then return love.physics.newCircleShape(w / 2) end
  if kind == "pillow" then
    local r = 12
    return love.physics.newPolygonShape(-w / 2 + r, -h / 2, w / 2 - r, -h / 2, w / 2, -h / 2 + r, w / 2, h / 2 - r,
                                        w / 2 - r, h / 2, -w / 2 + r, h / 2, -w / 2, h / 2 - r, -w / 2, -h / 2 + r)
  end
  return love.physics.newRectangleShape(w, h)
end

-- Let go of the block on the hook: it keeps the swing's speed.
local function drop()
  if not held or round.ended then return false end
  local b = held
  held = nil
  local x, y, vx = trolley()
  b.body = love.physics.newBody(world, x, y + ROPE + b.h / 2, "dynamic")
  b.body:setLinearVelocity(vx, 0)
  b.body:setAngularDamping(0.05)
  b.fixture = love.physics.newFixture(b.body, shapeFor(b.kind, b.w, b.h), 1)
  b.fixture:setFriction(0.85); b.fixture:setRestitution(0.04); b.fixture:setUserData(b)
  b.state, b.rest = "falling", 0
  crane.holding = false
  blocks[#blocks + 1] = b
  drops = drops + 1
  respawn = RESPAWN
  idle = 0
  if not round.title then sfx.drop:clone():play() end
  return true
end

local function isTower(ud) return ud == "rock" or (type(ud) == "table" and ud.state == "landed") end

-- Box2D's rule: nothing made or destroyed inside a callback, so remember it.
local function beginContact(a, b)
  local ua, ub = a:getUserData(), b:getUserData()
  if ua == "sea" then sinks[#sinks + 1] = ub return end
  if ub == "sea" then sinks[#sinks + 1] = ua return end
  local va = type(ua) == "table" and ua or nil
  local vb = type(ub) == "table" and ub or nil
  local dvx = (va and va.vx or 0) - (vb and vb.vx or 0)
  local dvy = (va and va.vy or 0) - (vb and vb.vy or 0)
  local cx, cy = a:getBody():getPosition()
  impacts[#impacts + 1] = { speed = math.sqrt(dvx * dvx + dvy * dvy), a = va, b = vb, x = cx, y = cy }
end

local function land(b)
  b.state = "landed"
  if not round.ended then round.score = round.score + 1 end
  streak = streak + 1
  local x, y = b.body:getPosition()
  for _, c in ipairs(b.body:getContacts()) do
    if c:isTouching() then
      local fa, fb = c:getFixtures()
      local other = fa:getBody() == b.body and fb or fa
      if isTower(other:getUserData()) then
        local ox, oy = other:getBody():getPosition()
        local j = love.physics.newWeldJoint(other:getBody(), b.body, (x + ox) / 2, (y + oy) / 2)
        welds[#welds + 1] = { joint = j, a = other:getUserData(), b = b, twist = 0 }
      end
    end
  end
  local text = (streak >= 3 and streak % 3 == 0) and ("+1  x" .. streak) or "+1"
  fx.popup(text, x, y - 34, { color = 0xfff3bf, size = 20 })
  fx.burst("sparkles", x, y - b.h / 2, { count = 14, color = { 0xfff2a6, 0xffffff } })
  if not round.title then
    local semis = ({ 0, 2, 4, 7, 9, 12, 14, 16, 19, 21 })[math.min(streak, 10)]
    local s = sfx.land:clone()
    s:setPitch(2 ^ (semis / 12)); s:setVolume(0.75); s:play()
    haptic("tap")
  end
end

local function sink(b)
  if type(b) ~= "table" or b.state == "sunk" or not b.body then return end
  b.state, b.sunkT = "sunk", 0
  b.body:setLinearDamping(6); b.body:setAngularDamping(4); b.body:setGravityScale(0.15)
  local x = b.body:getX()
  fx.burst("burst", x, SEA_Y, { count = 26, color = { 0xd8f4ff, 0xffffff, 0x8fd0ff } })
  streak = 0
  if round.title then return end
  sfx.splash:clone():play()
  if round.ended or debug.invincible() then return end
  sfx.lose:play()
  haptic("fail")
  fx.shake(4, 0.25)
  round.lives = round.lives - 1                -- at 0 the engine ends the round: TOPPLED!
  if round.lives <= 0 and held then drop() end -- and the crane lets go of what it holds
end

local function dust(x, y, n)
  fx.burst("dust", x, y, { count = n, color = 0xf4e7d0 })
end

local function measure()
  local top = ROCK_TOP
  local tx, ty = W / 2, ROCK_TOP
  for _, b in ipairs(blocks) do
    if b.state == "landed" then
      local x, y = b.body:getPosition()
      local a = b.body:getAngle()
      local half = 0.5 * (b.w * math.abs(math.sin(a)) + b.h * math.abs(math.cos(a)))
      top = math.min(top, y - half)
      if y - b.h / 2 < ty then tx, ty = x, y - b.h / 2 end
    end
  end
  return ROCK_TOP - top, tx, ty
end

function love.load()
  love.physics.setMeter(100)
  world = love.physics.newWorld(0, GRAVITY, true)
  world:setCallbacks(beginContact)

  local rock = love.physics.newBody(world, W / 2, ROCK_TOP + 14, "static")
  local top = love.physics.newFixture(rock, love.physics.newRectangleShape(ROCK_W, 28))
  top:setFriction(1); top:setUserData("rock")
  love.physics.newFixture(rock, love.physics.newRectangleShape(0, 50, ROCK_W - 30, 80)):setUserData("rock")
  local sea = love.physics.newBody(world, W / 2, SEA_Y + 400, "static")
  local water = love.physics.newFixture(sea, love.physics.newRectangleShape(6000, 800))
  water:setSensor(true); water:setUserData("sea")

  sfx = {}
  for _, n in ipairs({ "thud", "thud2", "land", "splash", "drop", "lose", "crack" }) do
    sfx[n] = love.audio.newSource(n, "static")
  end
  music.play("harbour", { volume = 0.5 })
  -- The swing starts where the first block, let go at once (the tap that
  -- starts the round), comes down dead centre on the rock.
  local fall = math.sqrt(2 * (ROCK_TOP - (RAIL_Y + ROPE + KINDS.crate.h)) / GRAVITY)
  local _, speed = sweep()
  phase = -math.atan(speed * fall)
  spawn()
end

function love.mousepressed() if not round.title then drop() end end
function love.keypressed(key) if key == "space" or key == "return" then drop() end end
function love.gamepadpressed(_, button) if button == "a" then drop() end end

-- Where the block on the hook would land if let go now (x), and the arc there.
local function landing(arc)
  if not held then return nil end
  local x, y, vx = trolley()
  y = y + ROPE + held.h / 2
  local floor = tower.y - held.h / 2
  local vy, h = 0, 1 / 30
  for _ = 1, 90 do
    if y >= floor then break end
    vy = vy + GRAVITY * h
    x, y = x + vx * h, y + vy * h
    if arc then arc[#arc + 1] = { x, y } end
  end
  return x
end

function love.update(dt)
  clock = clock + dt
  local _, speed = sweep()
  phase = phase + speed * dt
  world:update(dt, 8, 3)                 -- Box2D's iterations, as Topple's world has: a stiff tower of welds

  for _, b in ipairs(sinks) do sink(b) end
  sinks = {}
  for _, h in ipairs(impacts) do
    if h.speed > 90 then
      dust(h.x, h.y, math.min(10, math.floor(h.speed / 60)))
      if not round.title then
        audio.impact(h.speed > 380 and sfx.thud2 or sfx.thud, h.speed, 580)
        if h.speed > 380 then haptic("heavy") end
      end
      if h.speed > 380 then fx.shake(math.min(5, (h.speed - 300) / 120), 0.2) end
      for _, b in ipairs({ h.a, h.b }) do if b then b.fear = math.min(1, b.fear + h.speed / 500) end end
    end
  end
  impacts = {}

  -- Settling: a released block counts once it rests on the tower.
  for i = #blocks, 1, -1 do
    local b = blocks[i]
    b.age = b.age + dt
    local vx, vy = b.body:getLinearVelocity()
    local spd, spin = math.sqrt(vx * vx + vy * vy), math.abs(b.body:getAngularVelocity())
    b.vx, b.vy = vx, vy
    b.fear = math.max(b.fear * math.max(0, 1 - dt * 1.5), math.max(0, math.min(1, (spd - 40) / 300)), math.min(1, spin / 3))
    if b.state == "falling" then
      local x, y = b.body:getPosition()
      local onTower, fromAbove = false, false
      for _, c in ipairs(b.body:getContacts()) do
        if c:isTouching() then
          local fa, fb = c:getFixtures()
          local other = fa:getBody() == b.body and fb or fa
          if isTower(other:getUserData()) then
            onTower = true
            if other:getBody():getY() > y + 4 then fromAbove = true end
          end
        end
      end
      local level = b.kind == "wheel" or math.abs(math.sin(b.body:getAngle())) < 0.25
      local still = spd < SETTLE_SPEED and spin < 0.5
      b.rest = (onTower and still) and b.rest + dt or 0
      if (fromAbove and level and spd < STICK_SPEED) or b.rest >= SETTLE_TIME then land(b) end
    elseif b.state == "sunk" then
      b.sunkT = b.sunkT + dt
      if b.sunkT > 1.2 then
        b.body:destroy()
        table.remove(blocks, i)
      end
    end
  end

  -- Mortar cracks under too much twist: a lopsided tower leaning on it,
  -- or a hard hit. Rapier's welds are stiffer than Box2D's, so a landing
  -- spikes the torque for a step or two: the mortar feels the twist over
  -- about a tenth of a second, and only a hit three times as hard cracks it
  -- at once.
  local k = 1 - math.exp(-dt / 0.1)
  for i = #welds, 1, -1 do
    local w = welds[i]
    if w.joint:isDestroyed() then
      table.remove(welds, i)
    elseif dt > 0 then
      -- Per physics step (fixed 60 Hz), not per frame: at 120 Hz, 1 / dt
      -- would read twice the torque WELD_TORQUE was tuned against.
      local t = w.joint:getReactionTorque(60)
      w.twist = (w.twist or 0) + (t - (w.twist or 0)) * k
      if w.twist > WELD_TORQUE or t > WELD_TORQUE * 3 then
        local ax, ay = w.joint:getAnchors()
        w.joint:destroy()
        table.remove(welds, i)
        dust(ax, ay, 8)
        if not round.title then sfx.crack:clone():play() end
      end
    end
  end

  if respawn then
    respawn = respawn - dt
    if respawn <= 0 and not round.ended then respawn = nil spawn() end
  end
  if held then held.age = held.age + dt end

  -- The camera keeps the tower's top in view.
  local tx, ty
  height, tx, ty = measure()
  skyH = skyH + (height - skyH) * (1 - math.exp(-dt / 1.2))   -- the sky follows the height, never in a jump
  tower.x, tower.y = tx, ty
  local want = math.min(0, (ROCK_TOP - height) - TOWER_ON_SCREEN)
  camTop = camTop + (want - camTop) * (1 - math.exp(-dt * 2.5))
  camera:lookAt(W / 2, camTop + 320)

  local x, y, vx = trolley()
  local amp, spd = sweep()
  crane.x, crane.y, crane.vx, crane.phase, crane.amp, crane.speed = x, y + ROPE + (held and held.h / 2 or 0), vx, phase, amp, spd
  crane.holding, crane.h, crane.age = held ~= nil, held and held.h or 0, held and held.age or 0

  -- The title card: the crane builds a tower by itself, sloppily.
  if round.title and held and held.age > 0.6 then
    local lx = landing()
    if lx and math.abs(lx + bias - tower.x) < 7 then drop() end
    if held and held.age > 5 then drop() end
  end
  if not round.title then idle = idle + dt end
end

-- ---- Drawing ---------------------------------------------------------------

local SKY = {   -- tower height (px): top and bottom of the sky
  { 0,    0x6cc6ff, 0xd8f1ff },
  { 500,  0x5aa8f0, 0xffd9a0 },
  { 1000, 0x6a5acd, 0xff9e6b },
  { 1600, 0x0b1030, 0x3a2f6b },
}
local function sky()
  for i = 1, #SKY - 1 do
    local a, b = SKY[i], SKY[i + 1]
    if skyH <= b[1] then
      local t = (skyH - a[1]) / (b[1] - a[1])
      return mix(a[2], b[2], t), mix(a[3], b[3], t)
    end
  end
  return SKY[#SKY][2], SKY[#SKY][3]
end

local function background()
  local top, bottom = sky()
  for i = 0, 31 do
    color(mix(top, bottom, i / 31))
    love.graphics.rectangle("fill", 0, i * 20, W, 21)
  end
  -- The sun goes down as the tower climbs; the stars come out.
  local dusk = math.min(1, skyH / 1300)
  local sy = 130 + dusk * 420 + camTop * 0.02
  color(mix(0xfff2b0, 0xff7a4a, dusk), 0.35)
  love.graphics.circle("fill", 270, sy, 46)
  color(mix(0xfff6cf, 0xff9a5a, dusk))
  love.graphics.circle("fill", 270, sy, 28)
  local night = math.max(0, math.min(1, (skyH - 1000) / 600))
  if night > 0 then
    for i = 0, 39 do
      local x = (i * 97) % 360
      local y = ((i * 53) % 520 - camTop * 0.05) % 560
      color(0xffffff, night * (0.6 + 0.4 * math.sin(clock * 2 + i)))
      love.graphics.circle("fill", x, y, 1 + (i % 3) * 0.4)
    end
    color(0xf4f1de, night)
    love.graphics.circle("fill", 80, 110, 16)
    color(mix(SKY[4][2], 0x3a2f6b, 0.3), night)
    love.graphics.circle("fill", 87, 105, 14)
  end
  local day = 1 - math.max(0, math.min(1, (skyH - 800) / 700))
  for i = 0, 4 do
    local x = (i * 97 + clock * (6 + i * 2)) % 520 - 80
    local y = (60 + i * 130 - camTop * 0.3) % 760 - 60
    local r = 16 + (i % 3) * 6
    color(mix(mix(top, bottom, y / 640), 0xffffff, 0.6 * day))   -- opaque, so the puffs don't overlap darker
    love.graphics.circle("fill", x, y, r)
    love.graphics.circle("fill", x + r, y - r * 0.4, r * 1.2)
    love.graphics.circle("fill", x + r * 2.2, y, r * 0.9)
    love.graphics.rectangle("fill", x, y - 2, r * 2.2, r * 0.9)
  end
  -- Far islands on the horizon, slower than the world.
  local hy = SEA_Y - 6 - camTop * 0.35
  local isle = mix(mix(top, bottom, 0.8), 0x2b3a55, 0.45)
  color(isle)
  love.graphics.polygon("fill", -10, hy, 30, hy - 26, 70, hy - 34, 110, hy - 12, 130, hy)
  love.graphics.polygon("fill", 250, hy, 285, hy - 18, 320, hy - 22, 350, hy - 8, 370, hy)
  color(mix(bottom, 0x2f8fdf, 0.5))
  love.graphics.rectangle("fill", -10, hy, W + 20, 640)
end

local function rock()
  local cx, half = W / 2, ROCK_W / 2
  color(0x6b5d52)
  love.graphics.polygon("fill", cx - half + 8, ROCK_TOP + 20, cx + half - 8, ROCK_TOP + 20,
                        cx + half + 30, SEA_Y + 60, cx - half - 30, SEA_Y + 60)
  color(0x7d6e62)
  love.graphics.polygon("fill", cx - half + 22, ROCK_TOP + 26, cx + 10, ROCK_TOP + 26, cx - 6, SEA_Y + 60, cx - half - 6, SEA_Y + 60)
  color(INK)
  love.graphics.rectangle("fill", cx - half - 2, ROCK_TOP - 2, ROCK_W + 4, 32, 4)
  color(0x9c8a7a)
  love.graphics.rectangle("fill", cx - half, ROCK_TOP, ROCK_W, 28, 3)
  color(0xb9a796)
  love.graphics.rectangle("fill", cx - half, ROCK_TOP, ROCK_W, 6, 3)
  color(0x7fb069)
  for i = 0, 6 do love.graphics.circle("fill", cx - half + 10 + i * 24, ROCK_TOP + 2, 5 + (i % 2) * 2) end
end

local function sea()
  local bottom = camTop + 700
  for i = 0, 7 do
    color(mix(0x2f8fdf, 0x0d3a7a, i / 7), 0.85 + i * 0.02)
    love.graphics.rectangle("fill", -20, SEA_Y + i * (bottom - SEA_Y) / 8, W + 40, (bottom - SEA_Y) / 8 + 1)
  end
  color(0x5fb3f0, 0.9)
  for i = 0, 13 do
    local x = i * 28 + (clock * 18) % 28 - 28
    love.graphics.circle("fill", x, SEA_Y + 2 + math.sin(clock * 2 + i) * 2, 10)
  end
  color(0xbfe6ff, 0.75)
  love.graphics.rectangle("fill", -20, SEA_Y + 2, W + 40, 3)
end

local function rounded(x, y, w, h, r, c, a)
  color(c, a)
  love.graphics.rectangle("fill", x, y, w, h, math.min(r, w / 2, h / 2))
end

local function bodyShape(b, c)
  local w, h, o = b.w, b.h, 2.5
  if b.kind == "wheel" then
    local r = w / 2
    color(INK); love.graphics.circle("fill", 0, 0, r + o)
    color(c); love.graphics.circle("fill", 0, 0, r)
    color(shade(c, 0.12)); love.graphics.circle("fill", 0, 0, r * 0.75)
  elseif b.kind == "wedge" then
    color(INK)
    love.graphics.polygon("fill", -w / 2 - o, h / 2 + o * 0.6, w / 2 + o, h / 2 + o * 0.6, 0, -h / 2 - o * 1.2)
    color(c)
    love.graphics.polygon("fill", -w / 2, h / 2, w / 2, h / 2, 0, -h / 2)
  elseif b.kind == "pillow" then
    rounded(-w / 2 - o, -h / 2 - o, w + 2 * o, h + 2 * o, 12 + o, INK)
    rounded(-w / 2, -h / 2, w, h, 12, c)
    rounded(-w / 2 + 6, -h / 2 + 4, w - 12, 6, 3, shade(c, 0.3))
  else
    rounded(-w / 2 - o, -h / 2 - o, w + 2 * o, h + 2 * o, 4, INK)
    rounded(-w / 2, -h / 2, w, h, 2, c)
    rounded(-w / 2, -h / 2, w, 5, 2, shade(c, 0.3))
    rounded(-w / 2, h / 2 - 5, w, 5, 2, shade(c, -0.18))
    if b.kind == "plank" then
      color(shade(c, -0.15))
      love.graphics.setLineWidth(1.5)
      for k = 1, 3 do
        local x = -w / 2 + w * k / 4
        love.graphics.line(x, -h / 2 + 5, x, h / 2 - 5)
      end
    end
  end
end

local function smile(y, m)
  love.graphics.line(-m, y, -m * 0.4, y + m * 0.45, m * 0.4, y + m * 0.45, m, y)
end

-- A face that shows what the block is going through: excited on the hook,
-- "oh no" in the air, content on the tower, worried when it wobbles, X-eyed
-- in the sea. Drawn in the block's frame; `ang` is its turn.
local function face(b, ang, px, py)
  local w, h = b.w, b.h
  local cy = b.kind == "wedge" and h * 0.16 or 0
  local s = math.max(0.6, math.min(1.1, math.min(w, h) / 44))
  local dx = math.max(7, math.min(w * 0.2, 14 * s))
  local ey, er, my = cy - 4 * s, 4.5 * s, cy + 7 * s
  local lx, ly = 0, 0
  if b.state == "hanging" then
    local tx, ty = tower.x - px, tower.y - py
    local d = math.max(1, math.sqrt(tx * tx + ty * ty))
    lx, ly = tx / d * 1.6, ty / d * 1.6
  elseif b.state == "falling" then
    lx, ly = 1.5 * math.sin(ang), 1.5 * math.cos(ang)
  end
  love.graphics.setLineWidth(2.2)
  color(INK)
  if b.state == "sunk" then
    for _, sx in ipairs({ -1, 1 }) do
      local x = sx * dx
      love.graphics.line(x - er, ey - er, x + er, ey + er)
      love.graphics.line(x - er, ey + er, x + er, ey - er)
    end
    love.graphics.circle("fill", 0, my + 1, 3.5 * s)
    return
  end
  local blink = b.state == "landed" and (b.age * 0.37 + b.color * 0.21) % 1 < 0.035
  if blink then
    for _, sx in ipairs({ -1, 1 }) do love.graphics.line(sx * dx - er, ey, sx * dx + er, ey) end
    smile(my, 6 * s)
  else
    local scared = b.fear > 0.35 or b.state == "falling"
    local big = scared and 1.25 or 1
    for _, sx in ipairs({ -1, 1 }) do
      local x = sx * dx
      color(INK); love.graphics.circle("fill", x, ey, er * big + 1.5)
      color(0xffffff); love.graphics.circle("fill", x, ey, er * big)
      color(INK); love.graphics.circle("fill", x + lx, ey + ly, er * 0.5)
    end
    color(INK)
    if b.fear > 0.35 then
      for _, sx in ipairs({ -1, 1 }) do
        local x = sx * dx
        love.graphics.line(x - sx * er, ey - er * 1.9, x + sx * er, ey - er * 1.4)
      end
      local m = 6 * s
      love.graphics.line(-m, my, -m / 2, my + 2, 0, my, m / 2, my + 2, m, my)
      if b.fear > 0.6 then
        color(0x8fd0ff); love.graphics.circle("fill", dx + er + 3, ey - er, 2.5)
      end
    elseif b.state == "falling" then
      love.graphics.circle("fill", 0, my + 1, 4 * s)
    elseif b.state == "hanging" then
      love.graphics.polygon("fill", -6 * s, my - 1, 6 * s, my - 1, 0, my + 5 * s)
    else
      smile(my, 6 * s)
    end
  end
  color(0xff8fa3, 0.5)
  for _, sx in ipairs({ -1, 1 }) do love.graphics.circle("fill", sx * (dx + er * 0.8), my - 1, 2.5 * s) end
end

local function drawBlock(b, x, y, ang)
  local c = PALETTE[b.color]
  if b.state == "sunk" then c = mix(c, 0x1f5fa8, 0.55) end
  love.graphics.push()
  love.graphics.translate(x, y)
  love.graphics.rotate(ang)
  bodyShape(b, c)
  face(b, ang, x, y)
  love.graphics.pop()
end

local function crane_()
  local y = camTop + RAIL_Y
  rounded(-10, y - 14, W + 20, 10, 0, INK)
  rounded(-10, y - 12, W + 20, 6, 0, 0xf0b429)
  color(0xc98a0c)
  love.graphics.setLineWidth(2)
  for i = 0, 9 do love.graphics.line(i * 40, y - 12, i * 40 + 20, y - 6) end
  local tx, ty = trolley()
  rounded(tx - 14, ty - 18, 28, 16, 3, INK)
  rounded(tx - 12, ty - 16, 24, 12, 2, 0x495057)
  color(INK)
  love.graphics.setLineWidth(2.5)
  local hookY = held and ty + ROPE or ty + ROPE * 0.6
  love.graphics.line(tx, ty, tx, hookY - 6)
  love.graphics.circle("fill", tx, hookY - 6, 5)
  color(0xadb5bd); love.graphics.circle("fill", tx, hookY - 6, 3)
  if held then drawBlock(held, tx, ty + ROPE + held.h / 2, 0) end
end

local function heightMarker()
  if height < 40 then return end
  local y = ROCK_TOP - height
  color(0xffffff, 0.8)
  love.graphics.setLineWidth(2)
  love.graphics.line(14, y, 40, y)
  local label = string.format("%.1f M", height / 100)
  color(INK, 0.6); love.graphics.print(label, 17, y - 19)
  color(0xffffff); love.graphics.print(label, 15, y - 21)
end

local function hint()
  if not held or round.title or round.ended then return end
  local tx, ty = trolley()
  local by = ty + ROPE + held.h
  if drops < 2 or idle > 3 then
    local arc = {}
    landing(arc)
    color(0xffffff, 0.75)
    for k, p in ipairs(arc) do
      if k % 2 == 0 and p[2] > by + 24 then love.graphics.circle("fill", p[1], p[2], 2.5) end
    end
    local bob = math.sin(clock * 6) * 4
    local a = 0.75 + 0.25 * math.sin(clock * 6)
    color(0xffffff, a)
    love.graphics.polygon("fill", tx - 8, by + 22 + bob, tx + 8, by + 22 + bob, tx, by + 32 + bob)
    color(INK, a * 0.7); love.graphics.printf("TAP TO DROP", tx - 80 + 2, by + 40 + math.abs(bob) + 2, 160, "center")
    color(0xffffff, a); love.graphics.printf("TAP TO DROP", tx - 80, by + 40 + math.abs(bob), 160, "center")
  else
    local lx = landing()
    local y = tower.y - 18
    color(0xffffff, 0.35 + 0.15 * math.sin(clock * 5))
    love.graphics.polygon("fill", lx - 6, y, lx + 6, y, lx, y + 7)
  end
end

function love.draw()
  background()
  camera:attach()
  rock()
  for _, b in ipairs(blocks) do
    if b.state == "sunk" then drawBlock(b, b.body:getX(), b.body:getY(), b.body:getAngle()) end
  end
  sea()
  for _, b in ipairs(blocks) do
    if b.state ~= "sunk" then drawBlock(b, b.body:getX(), b.body:getY(), b.body:getAngle()) end
  end
  crane_()
  heightMarker()
  hint()
  camera:detach()
end
