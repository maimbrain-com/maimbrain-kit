-- Moss Knight (docs/STUDIO-2.md §6.2): a small knight with a lantern on a
-- moonlit hillside gathers wisps before the moon sets, while a goblin hunts
-- it through the trees. Phase 3's acceptance game: a hero on a
-- CharacterBody3D that climbs and slides over a Terrain, and a hunter on a
-- NavigationAgent3D (autoMove) that finds it round 40 trees.
-- The hero is a placeholder rigged model (games/mistwood's fox); the
-- goblin, the trees and the lantern are built from shapes.
--
-- Hold a finger where you want to go (above the knight is ahead); tap to
-- hop. Until you first touch, the knight follows its own path to the
-- nearest wisp (the title card's attract mode plays the same way).

local SPEED, JUMP, GRAVITY, MOON, EDGE = 5, 6.5, 18, 90, 27
-- knight, goblin and wisps: game/bot.lua plays from them (game/shared.lua).
local body, tree, ground, agent, guide, rig, jump, hop, sfx
knight, goblin, wisps = nil, nil, {}
local shared = require("shared")
shared.wisps = wisps
local steered, down, downAt, downX, downY = false, false, 0, 0, 0
local stunned, growled, nearest = 0, -10, math.huge
local skip, aim, trees, lastAim, lastDist = {}, nil, {}, nil, 0
local fps = { n = 0, t = 0, late = 0, long = 0, show = "" }

-- Somewhere open: not by a tree, not on a steep bank.
local function open(x, z)
  local _, ny = ground:normalAt(x, z)
  if ny < 0.85 then return false end
  for _, t in ipairs(trees) do
    if (t.x - x) ^ 2 + (t.z - z) ^ 2 < 4 then return false end
  end
  return true
end

-- A wisp: a bobbing mote with a halo, a few sparks rising off it and a
-- faint pillar of light that shows through the fog, so you can see where
-- the far ones are.
local mote = material{ color = 0xd8f4ff, emissive = 0xbfe8ff, emissiveIntensity = 4, unlit = true }
local halo = material{ color = 0x7fd0ff, unlit = true, transparent = true, opacity = 0.35, additive = true }
local pillar = material{ color = 0x6fb8ff, unlit = true, transparent = true, opacity = 0.16, additive = true, fog = false }

local function wisp()
  local x, z
  repeat x, z = love.math.random() * 50 - 25, love.math.random() * 50 - 25 until x * x + z * z < 625 and open(x, z)
  local area = Area3D{ shape = SphereShape3D{ radius = 0.8 }, x = x, y = ground:heightAt(x, z) + 0.9, z = z, tag = "wisp" }
  Mesh{ parent = area, geometry = geometry.ico(0.2, 1), material = mote }
  Mesh{ parent = area, geometry = geometry.sphere(0.42, 12, 8), material = halo }
  Mesh{ parent = area, geometry = geometry.cylinder(0.12, 9, 6), material = pillar, y = 4.2 }
  Emitter{ parent = area, preset = "sparkles", rate = 5, colors = { 0xbfe8ff, 0x8fd8ff }, sizes = { 0.03, 0.07 },
           life = { 0.8, 1.6 }, speed = { 0.2, 0.6 }, gravity = -0.4, additive = true }
  flux.to(area, 1.2, { y = area.y + 0.5 }):ease("sineinout"):yoyo()
  wisps[area] = true
  area.bodyEntered:connect(function(who)
    if who ~= knight then return end
    fx.burst("sparkles", area.x, area.y, area.z, { count = 30, color = 0xbfe8ff })
    sfx.chime:setPitch(1 + (round.score % 8) * 0.06)
    sfx.chime:clone():play()
    haptic("tap")
    wisps[area] = nil
    area:destroy()
    wisp()
    round.score = round.score + 1
  end)
end

local function caught()
  if stunned > round.time or not hurt(knight) then return end
  sfx.thud:play(); haptic("heavy"); fx.shake(0.3, 0.25)
  fx.popup("OOF", knight.x, knight.y + 1.6, knight.z, { color = 0xff6050, size = 0.45 })
  hop = true
  stunned = round.time + 1.2            -- the goblin gloats a moment before it hunts again
end

-- The trees: one geometry (trunk and three tiers of needles, painted), a
-- StaticBody3D trunk each that the knight slides along and the navmesh
-- bakes round. A tree between the camera and the knight turns see-through.
local bark, ghost
local function forest()
  local g = geometry.box(0.01, 0.01, 0.01):merge(geometry.cylinder(0.3, 6, 8):paint(0x5a4232), 0, 3, 0)
  g:merge(geometry.cone(1.7, 3, 9):paint(0x23402c), 0, 4.6, 0)
  g:merge(geometry.cone(1.35, 2.6, 9):paint(0x2b4c33), 0, 5.8, 0, 0, 0.4, 0)
  g:merge(geometry.cone(0.95, 2.1, 9):paint(0x335a3b), 0, 7.0, 0, 0, 0.8, 0)
  bark = { material{ color = 0xe8f0e0, roughness = 0.95 }, material{ color = 0xc8d8d0, roughness = 0.95 } }
  ghost = material{ color = 0xc8d8d0, transparent = true, opacity = 0.2 }
  love.math.setRandomSeed(3)
  local n = 0
  while n < 40 do
    local x, z = love.math.random() * 54 - 27, love.math.random() * 54 - 27
    if x * x + z * z > 16 then
      n = n + 1
      local s = 0.85 + love.math.random() * 0.4
      local y = ground:heightAt(x, z)
      local t = StaticBody3D{ shape = CylinderShape3D{ radius = 0.3 * s + 0.05, height = 6 * s }, x = x, y = y + 3 * s - 0.3, z = z, tag = "tree" }
      local look = bark[love.math.random() < 0.5 and 1 or 2]
      t.data = { r = 1.7 * s, look = Mesh{ parent = t, geometry = g, material = look, y = -3 * s, scale = s, ry = love.math.random() * 6.28 }, material = look }
      trees[#trees + 1] = t
    end
  end
  -- The wood beyond the clearing: more of the same tree round the edge,
  -- drawn in one call (nobody walks there), so the world ends in forest.
  local rng, list = love.math.newRandomGenerator(9), {}
  for _ = 1, 90 do
    local a, r = rng:random() * 6.283, 36 + rng:random() * 11
    local x, z = math.cos(a) * r, math.sin(a) * r
    for _, v in ipairs({ x, ground:heightAt(x, z) - 0.3, z, rng:random() * 6.28, 0.9 + rng:random() * 0.5 }) do list[#list + 1] = v end
  end
  Mesh{ geometry = g, material = bark[2] }:instance(list, 5)
end

-- Trees on the line from the camera to the knight (and a little round it) go see-through.
local function clearView()
  local cx, cz, kx, kz = camera.x, camera.z, knight.x, knight.z
  local lx, lz = kx - cx, kz - cz
  local ll = lx * lx + lz * lz
  if ll < 0.01 then return end
  for _, t in ipairs(trees) do
    local s = clamp(((t.x - cx) * lx + (t.z - cz) * lz) / ll, 0, 1)
    local px, pz = cx + lx * s - t.x, cz + lz * s - t.z
    local hide = s < 0.97 and px * px + pz * pz < (t.data.r + 0.4) ^ 2
    if hide ~= t.data.hidden then
      t.data.hidden = hide
      t.data.look.material = hide and ghost or t.data.material
    end
  end
end

local function goblinMade()
  goblin = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.2 }, x = 22, y = ground:heightAt(22, 22) + 1, z = 22, tag = "goblin" }
  shared.goblin = goblin
  local skin = material{ color = 0x6f9a3e, roughness = 0.7, rim = 0.5 }
  local rag = material{ color = 0x5a3a22, roughness = 1 }
  local eye = material{ color = 0xffe040, emissive = 0xffb020, emissiveIntensity = 5, unlit = true }
  local gm = Group{ parent = goblin, y = -0.6 }
  Mesh{ parent = gm, geometry = geometry.cylinder(0.3, 0.7, 10), material = rag, y = 0.55, rx = 0.25 }
  Mesh{ parent = gm, geometry = geometry.sphere(0.32, 12, 8), material = skin, y = 1.05, z = -0.12 }
  Mesh{ parent = gm, geometry = geometry.cone(0.08, 0.3, 6), material = skin, y = 1.0, z = -0.45, rx = -1.4 }   -- a long nose
  for _, s in ipairs({ -1, 1 }) do
    Mesh{ parent = gm, geometry = geometry.cone(0.1, 0.55, 6), material = skin, x = s * 0.38, y = 1.18, z = -0.08, rz = -s * 1.25 }
    Mesh{ parent = gm, geometry = geometry.sphere(0.06), material = eye, x = s * 0.12, y = 1.12, z = -0.4 }
    Mesh{ parent = gm, geometry = geometry.cylinder(0.08, 0.35, 6), material = skin, x = s * 0.14, y = 0.17 }
    Mesh{ parent = gm, geometry = geometry.cylinder(0.06, 0.55, 6), material = skin, x = s * 0.36, y = 0.62, z = -0.15, rx = 0.8, rz = s * 0.3 }
  end
  flux.to(gm, 0.18, { y = -0.52 }):ease("sineinout"):yoyo()   -- a loping bob
  Light{ parent = goblin, color = 0xff6030, intensity = 1.6, range = 3.5, y = 1 }
  agent = NavigationAgent3D{ parent = goblin, maxSpeed = 3.2, autoMove = true }
  timer.every(0.5, function() agent:setTargetPosition(knight.x, knight.y, knight.z) end)
  local grab = Area3D{ parent = goblin, shape = SphereShape3D{ radius = 0.8 } }
  grab.bodyEntered:connect(function(who) if who == knight then caught() end end)
end

function love.load()
  world.sky{ atmosphere = "night", disc = "moon", stars = 1.2 }
  world.sun{ dir = { -0.35, -0.8, 0.5 }, color = 0xa8bcff, intensity = 1.1, shadows = true }
  world.fog{ color = 0x1c2747, mode = "exp2", density = 0.03 }
  world.lighting("hero")
  world.grade("moonlit", 0.6)

  -- Rolling enough for dirt banks and a hill to climb; under 2% of it past
  -- 45° (the knight scrambles up to 50°, the goblin's navmesh stops at 40°).
  -- (96 m of ground, so the camera never looks off its edge; the knight keeps within 27 m of the middle.)
  ground = Terrain{ size = 96, height = 5, noise = { seed = 7, octaves = 4, frequency = 0.03 },
                    stamps = { { "hill", 8, -6, radius = 10, height = 4 } },
                    paint = { { surface = "grass", color = 0x3a5a36 }, { slope = 0.15, surface = "dirt", color = 0x5e5446 } } }
  local tuft = geometry.cone(0.05, 0.55, 3)
  tuft:merge(geometry.cone(0.04, 0.45, 3), 0.07, -0.03, 0.02, 0, 0, -0.4)
  tuft:merge(geometry.cone(0.04, 0.5, 3), -0.06, -0.02, -0.03, 0.35, 0, 0.35)
  scatter(Mesh{ geometry = tuft, material = material{ color = 0x5d8a44, roughness = 1 } },
          { count = 2700, area = 72, on = ground, seed = 7, scale = { 0.6, 1.4 }, tint = { 0x3f6a30, 0x86a848 } })
  scatter(Mesh{ geometry = geometry.rock(1, 4):paint(0x6c707a) },
          { count = 70, area = 60, on = ground, seed = 11, scale = { 0.2, 0.5 }, tint = { 0x8a8e98, 0x5a5e66 } })
  local shroom = geometry.box(0.01, 0.01, 0.01):merge(geometry.cylinder(0.025, 0.16, 5):paint(0xd8e8e0), 0, 0.08, 0)
  shroom:merge(geometry.cone(0.1, 0.07, 8):paint(0x60f0d0), 0, 0.18, 0)
  scatter(Mesh{ geometry = shroom, material = material{ color = 0xffffff, emissive = 0x40c8b0, emissiveIntensity = 1.5 } },
          { count = 160, area = 60, on = ground, seed = 23, scale = { 0.5, 1.2 }, tint = { 0xffffff, 0xa0c0ff } })

  forest()
  NavigationRegion3D{ size = 64, bake = { agentRadius = 0.4, agentHeight = 1.6, maxSlope = 0.7 } }

  knight = CharacterBody3D{ shape = CapsuleShape3D{ radius = 0.35, height = 1.2 }, y = ground:heightAt(0, 0) + 1,
                            floorSnapLength = 0.3, maxStepHeight = 0.35, floorMaxAngle = 0.87, tag = "player" }
  shared.knight = knight
  body = Model{ model = "fox", parent = knight, y = -0.6, ry = math.pi, scale = 1.5 }
  body:restyle{ rim = 0.55 }   -- an edge of moonlight, so it reads in the trees' shadows
  tree = AnimationTree{ model = body, start = "move",
    states = {
      move = { blend = { Idle = 0, Walk = 1.5, Run = SPEED }, by = "speed" },
      jump = { clip = "Jump", once = true },
    },
    transitions = {
      { from = "move", to = "jump", when = "airborne", fade = 0.1 },
      { from = "jump", to = "move", when = "onFloor", fade = 0.15 },
    } }
  -- The lantern, on a crook over its back: a warm glass and the light it throws.
  local back = Group{ parent = knight, y = 0.35, z = 0.25 }
  Mesh{ parent = back, geometry = geometry.cylinder(0.025, 0.9, 5), material = material{ color = 0x6a4a30 }, y = 0.4, rx = -0.35 }
  local lantern = Group{ parent = back, y = 0.75, z = -0.32 }
  local iron = material{ color = 0x3a3028, metalness = 0.6, roughness = 0.5 }
  Mesh{ parent = lantern, geometry = geometry.cylinder(0.12, 0.04, 8), material = iron, y = -0.12 }
  Mesh{ parent = lantern, geometry = geometry.cone(0.14, 0.1, 8), material = iron, y = 0.15 }
  Mesh{ parent = lantern, geometry = geometry.torus(0.05, 0.012, 10, 4), material = iron, y = 0.23, rx = math.pi / 2 }
  Mesh{ parent = lantern, geometry = geometry.cylinder(0.09, 0.2, 8), material = material{ color = 0xffd890, emissive = 0xffb040, emissiveIntensity = 4, unlit = true } }
  Mesh{ parent = lantern, geometry = geometry.sphere(0.26, 10, 6), material = material{ color = 0xffb050, unlit = true, transparent = true, opacity = 0.22, additive = true } }
  Light{ parent = lantern, color = 0xffc070, intensity = 3.6, range = 7 }
  flux.to(lantern, 0.45, { rx = 0.18 }):ease("sineinout"):yoyo()   -- it swings

  -- Its own agent (no autoMove): before anyone steers, it follows its path to a wisp.
  guide = NavigationAgent3D{ parent = knight, radius = 0.35, avoidance = false }
  timer.every(0.5, function()
    -- A wisp its path can't reach (up a slope too steep), or one it has
    -- stopped getting nearer to (its path ends short of it), is left for the player.
    local d = guide:distanceToTarget()
    if aim and wisps[aim] and (not guide:isTargetReachable() or (aim == lastAim and d > lastDist - 0.05)) then skip[aim] = true end
    lastAim, lastDist = aim, d
    local best, bw = math.huge, nil
    for w in pairs(wisps) do
      local d = (w.x - knight.x) ^ 2 + (w.z - knight.z) ^ 2
      if d < best and not skip[w] then best, bw = d, w end
    end
    aim = bw
    if bw then guide:setTargetPosition(bw.x, bw.y, bw.z) end
  end)

  -- The camera rides a rig that turns lazily toward where the knight goes,
  -- so steering sideways curves rather than spins, and walking back toward
  -- the camera doesn't swing it round.
  rig = Group{ x = knight.x, y = knight.y, z = knight.z }
  camera.fov = 55
  camera:follow(rig, { behind = 6.5, above = 3, lag = 0.25, lookAhead = 1.5 })
  round.subject = knight

  -- Fireflies drifting round wherever you are.
  Emitter{ parent = rig, preset = "sparkles", rate = 6, y = 1.2, colors = { 0xe8ff80, 0xb0ff60 }, sizes = { 0.03, 0.05 },
           life = { 2, 4 }, speed = { 0.1, 0.4 }, spread = 6, additive = true }

  goblinMade()
  for _ = 1, 10 do wisp() end
  ui.hud{ score = true, timer = MOON }
  sfx = { hop = love.audio.newSource("hop", "static"), chime = love.audio.newSource("chime", "static"),
          thud = love.audio.newSource("thud", "static"), growl = love.audio.newSource("growl", "static") }
  music.play("glade")
end

-- A tap hops; a held finger steers toward itself.
function love.mousepressed(x, y) down, downAt, downX, downY = true, round.time, x, y end
function love.mousereleased(x, y)
  if down and round.time - downAt < 0.22 and (x - downX) ^ 2 + (y - downY) ^ 2 < 400 then jump = true end
  down = false
end
function love.keypressed(key) if key == "space" or key == "up" then jump = true end end
function love.gamepadpressed(_, button) if button == "a" then jump = true end end

-- The rig's front and right on the ground (a node's front is −z).
local function axes()
  local fx_, fz = -math.sin(rig.ry), -math.cos(rig.ry)
  return fx_, fz, -fz, fx_
end

function love.update(dt)
  -- The frame-rate line (a phone run's HUD): rolling fps, late frames, the longest.
  fps.n, fps.t = fps.n + 1, fps.t + dt
  if dt > 1 / 50 then fps.late = fps.late + 1 end
  fps.long = math.max(fps.long, dt)
  if fps.t >= 2 then
    fps.show = string.format("%.0f fps  %d%% late  %.0f ms", fps.n / fps.t, fps.late * 100 / fps.n, fps.long * 1000)
    fps.n, fps.t, fps.late, fps.long = 0, 0, 0, 0
  end

  -- The goblin: still a moment after a catch, then quicker as the moon sinks.
  agent.maxSpeed = stunned > round.time and 0 or 3.4 + math.min(round.time, MOON) / MOON * 1.2
  local gx, gz = goblin.x - knight.x, goblin.z - knight.z
  nearest = math.sqrt(gx * gx + gz * gz)
  if nearest < 7 and round.time - growled > 4 and not round.title then
    growled = round.time
    sfx.growl:setPosition(goblin.x, goblin.y, goblin.z)
    sfx.growl:play()
  end

  if round.ended then knight.vx, knight.vz = 0, 0 end
  local fx_, fz, rx, rz = axes()
  local ax, az = 0, 0
  local sx, sy = input.ax, input.ay
  if not round.ended and love.mouse.isDown(1) and round.time - downAt > 0.08 then
    -- Toward the finger, from where the knight is on the screen.
    local mx, my = love.mouse.getPosition()
    local kx, ky = camera:project(knight.x, knight.y, knight.z)
    sx = clamp((mx - kx) / 90, -1, 1)
    sy = clamp((my - ky) / 90, -1, 1)
    if sx * sx + sy * sy < 0.04 then sx, sy = 0, 0 end
  end
  if sx ~= 0 or sy ~= 0 then
    steered = true
    ax, az = rx * sx - fx_ * sy, rz * sx - fz * sy
    local m = math.sqrt(ax * ax + az * az)
    if m > 1 then ax, az = ax / m, az / m end
  elseif not steered and not round.ended then
    -- Nobody has steered yet: it follows its agent's path to the nearest
    -- wisp (in a round at an amble, so playing always beats leaving it).
    local nx, _, nz = guide:getNextPathPosition()
    local bx, bz = nx - knight.x, nz - knight.z
    local d = math.sqrt(bx * bx + bz * bz)
    local pace = round.title and 1 or 0.6
    if d > 0.1 and not guide:isNavigationFinished() then ax, az = bx / d * pace, bz / d * pace end
  end

  -- The clearing's edge (a circle): it can walk along it, not past it.
  local r = math.sqrt(knight.x ^ 2 + knight.z ^ 2)
  if r > EDGE then
    local nx, nz = knight.x / r, knight.z / r
    local out = ax * nx + az * nz
    if out > 0 then ax, az = ax - out * nx, az - out * nz end
  end
  knight.vx, knight.vz = ax * SPEED, az * SPEED
  if knight:isOnFloor() then
    knight.vy = 0
    if jump then knight.vy = JUMP; sfx.hop:play() elseif hop then knight.vy = 4 end
  else
    knight.vy = knight.vy - GRAVITY * dt
  end
  jump, hop = false, false
  knight:moveAndSlide()
  if ax * ax + az * az > 0.04 then knight:faceDirection(ax, az) end

  tree:set("speed", math.sqrt(knight.vx ^ 2 + knight.vz ^ 2))
  tree:set("airborne", not knight:isOnFloor())
  tree:set("onFloor", knight:isOnFloor())

  -- The camera rig: on the knight, turning toward its way as it goes ahead.
  rig.x, rig.y, rig.z = knight.x, knight.y, knight.z
  local m = math.sqrt(ax * ax + az * az)
  if m > 0.2 then
    local ahead = (ax * fx_ + az * fz) / m
    local want = math.atan2(-ax, -az)
    local turn = (want - rig.ry + math.pi) % (2 * math.pi) - math.pi
    -- (Following its own path it swings round at once; steered, slowly
    -- when the knight comes back toward the camera.)
    local rate = steered and 2.2 * (0.25 + 0.75 * math.max(ahead, 0)) or 3
    rig.ry = rig.ry + clamp(turn, -rate * dt, rate * dt)
  end

  clearView()
  if round.time >= MOON and not round.ended then round.over("MOONSET", "time") end
end

function love.draw()
  local x, y, w, h = love.window.getSafeArea()
  -- The goblin, when it's close and off the screen: a red mark on the edge toward it.
  if not round.title and not round.ended and nearest < 12 then
    local sx, sy, on = camera:project(goblin.x, goblin.y + 0.6, goblin.z)
    if not on or sx < x or sx > x + w or sy < y or sy > y + h then
      local kx, ky = camera:project(knight.x, knight.y, knight.z)
      local fx_, fz, rx, rz = axes()
      local gx, gz = goblin.x - knight.x, goblin.z - knight.z
      local a = math.atan2(-(gx * fx_ + gz * fz), gx * rx + gz * rz)
      local px = clamp(kx + math.cos(a) * w, x + 22, x + w - 22)
      local py = clamp(ky + math.sin(a) * h, y + 120, y + h - 40)
      local pulse = 0.6 + 0.4 * math.sin(round.time * 10)
      love.graphics.setColor(1, 0.25, 0.15, pulse * clamp((12 - nearest) / 6, 0.3, 1))
      love.graphics.circle("fill", px, py, 11)
      love.graphics.setColor(1, 0.9, 0.6, pulse)
      love.graphics.printf("!", px - 11, py - 9, 22, "center")
    end
  end
  -- A finger steering: a faint ring under it.
  if down and love.mouse.isDown(1) and round.time - downAt > 0.08 then
    local mx, my = love.mouse.getPosition()
    love.graphics.setColor(1, 0.85, 0.55, 0.25)
    love.graphics.circle("line", mx, my, 26)
  end
  love.graphics.setColor(1, 1, 1, 0.3)
  love.graphics.print(fps.show, x + 8, y + h - 20, 0, 0.75, 0.75)
end
