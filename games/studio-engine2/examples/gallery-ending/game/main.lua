-- Last Round: the Rust UI Gallery's ENDING page (games/gallery, page 13) as
-- a Studio 2.0 game. The five beats of an ending, about 20 s, played by one
-- cutscene{} over a 3D arena at dusk: the defeat (Granny's line, her mech
-- falls on the "fall" cue and smokes), the cost and the hero's reaction
-- (Rocko's two lines), the world after (ONE YEAR LATER, a closing line),
-- credits with the cast while the camera cranes up, and a fade.

local HERO = { x = -1.5, z = 0 }     -- Rocko's mech, orange, an arm short
local CHAMP = { x = 1.5, z = 0.4 }   -- Granny's, purple, beaten

-- The world: a dusk sky with the sun low behind the arena, a dark floor
-- with a glowing grid, a ring of floodlights.
world.sky{ atmosphere = "dusk", disc = "sun", clouds = { cover = 0.25 } }
world.sun{ dir = { 0.05, -0.12, 1 }, color = 0xffb27a, intensity = 2.2, shadows = true }
world.fog{ color = 0x5a3048, mode = "exp2", density = 0.018 }
world.post{ bloom = 0.5, vignette = 0.35 }

Mesh{ geometry = geometry.plane(80, 80), material = material{ color = 0x2a1d30, roughness = 0.95 } }
local grid = geometry.box(0.04, 0.01, 24)
for i = -6, 6 do
  grid:merge(geometry.box(0.04, 0.01, 24), i * 2, 0.006, 0)
  grid:merge(geometry.box(24, 0.01, 0.04), 0, 0.006, i * 2)
end
Mesh{ geometry = grid, material = material{ color = 0xff8a6a, unlit = true, transparent = true, opacity = 0.35 } }

-- Floodlight towers round the arena's edge, their lamps the only hard light.
local lamp = material{ color = 0xffe2b0, emissive = 0xffd08a, emissiveIntensity = 3, unlit = true }
local steel = material{ color = 0x3a2f45, roughness = 0.7, metalness = 0.5 }
for _, p in ipairs({ { -9, -8 }, { 9, -8 }, { -11, 3 }, { 11, 3 } }) do
  Mesh{ geometry = geometry.cylinder(0.12, 6, 8), material = steel, x = p[1], y = 3, z = p[2] }
  Mesh{ geometry = geometry.box(0.9, 0.4, 0.3), material = lamp, x = p[1], y = 6.1, z = p[2] }
end
-- The floodlights' fill on the fighters' fronts (the sun is behind them).
Light{ color = 0xffc890, intensity = 9, range = 16, x = 0.5, y = 4, z = 5 }
Light{ color = 0xff9a70, intensity = 5, range = 10, x = -4, y = 2.5, z = 3 }

-- A mech from boxes: the root at its feet, so it leans and falls from there.
local function mech(color, dark, at, oneArm)
  local body = material{ color = color, roughness = 0.5, metalness = 0.2, emissive = color, emissiveIntensity = 0.18 }
  local shade = material{ color = dark, roughness = 0.6, metalness = 0.3, emissive = dark, emissiveIntensity = 0.15 }
  local root = Group{ x = at.x, z = at.z }
  Mesh{ parent = root, geometry = geometry.box(0.3, 0.8, 0.3), material = shade, x = -0.3, y = 0.4 }
  Mesh{ parent = root, geometry = geometry.box(0.3, 0.8, 0.3), material = shade, x = 0.3, y = 0.4 }
  Mesh{ parent = root, geometry = geometry.box(1.1, 0.85, 0.7), material = body, y = 1.175 }
  Mesh{ parent = root, geometry = geometry.box(0.5, 0.4, 0.5), material = body, y = 1.8 }
  Mesh{ parent = root, geometry = geometry.box(0.24, 0.7, 0.3), material = shade, x = 0.7, y = 1.15 }
  if oneArm then
    -- The left arm's stump, still sparking.
    Mesh{ parent = root, geometry = geometry.box(0.24, 0.22, 0.3), material = shade, x = -0.7, y = 1.42 }
    Emitter{ parent = root, preset = "sparks", rate = 4, x = -0.72, y = 1.3, colors = { 0xffd060, 0xff7a30 }, sizes = { 0.03, 0.06 } }
  else
    Mesh{ parent = root, geometry = geometry.box(0.24, 0.7, 0.3), material = shade, x = -0.7, y = 1.15 }
  end
  local eye = Mesh{ parent = root, geometry = geometry.sphere(0.07, 10, 8),
                    material = material{ color = 0x7ff6ff, emissive = 0x7ff6ff, emissiveIntensity = 3, unlit = true },
                    x = 0.05, y = 1.82, z = 0.26 }
  return root, eye
end

local hero = mech(0xe07a2a, 0x924f1b, HERO, true)
local champ, champEye = mech(0x8a6ad0, 0x5a4587, CHAMP, false)
champ.rz = 0.25                       -- swaying, about to go

local smoke = Emitter{ preset = "smoke", rate = 9, emitting = false, x = CHAMP.x - 0.9, y = 0.4, z = CHAMP.z,
                       colors = { 0x6a5a6a, 0x4a3e4c }, sizes = { 0.4, 1.4 }, life = { 2, 3.5 }, speed = { 0.4, 0.9 } }

cast("granny", { portrait = "portraits/granny", side = "right", name = "GRANNY" })
cast("rocko", { portrait = "portraits/rocko", side = "left", name = "ROCKO" })
cast("granny").health = 0.15          -- the champion, nearly done
cast("rocko").health = 0.6            -- the winner, scuffed

local crash, creak, sting

-- The champion's mech gives way: it creaks, topples toward the winner, hits
-- the floor hard and smokes from then on.
on("cue", "fall", function()
  print(("fall %.2f"):format(round.time))
  creak:play()
  flux.to(champ, 0.9, { rz = 1.57 }):ease("bounceout")
  timer.after(0.42, function()
    crash:play()
    haptic("heavy")
    fx.shake(0.12, 0.35)
    fx.burst("dust", CHAMP.x - 1, 0.1, CHAMP.z, { count = 40, color = 0x8a6a7a })
    champEye.material = material{ color = 0xff3030, emissive = 0xff3030, emissiveIntensity = 2, unlit = true }
    smoke.emitting = true
  end)
end)
on("cue", "later", function() sting:play() end)

local liveEye = champEye.material
local epilogue
local covered = false                 -- black between the fade out and the next showing

-- The epilogue from the top: the champion back on her feet, the smoke out,
-- the wide shot; then the scene. Like the gallery page, it plays again
-- a moment after it ends.
epilogue = function()
  covered = false
  champ.rz = 0.25
  champEye.material = liveEye
  smoke.emitting = false
  camera.fov = 52
  camera.x, camera.y, camera.z = 0, 2.4, 9.5
  camera:lookAt(0, 1, 0)
  cutscene{
    { "letterbox", 0.22 },
    { "shot", 0, 2.4, 9.5, 0, 1, 0, 0 },                    -- wide
    { "fade", "in", 1.2 }, "meanwhile",
    -- The defeat.
    { "shot", 4.2, 1.9, 5.2, 1.2, 0.8, 0.4, 3 }, "meanwhile", -- on Granny
    { "say", "granny", "defeat" },
    { "cue", "fall" },
    { "wait", 1.2 },
    -- The cost, and the hero's reaction.
    { "shot", -3, 1.8, 3.8, -1.5, 1.4, 0, 2 }, "meanwhile",  -- on Rocko
    { "say", "rocko", "cost" },
    { "say", "rocko", "goodfight", "smug" },
    -- The world after, and a closing line.
    { "shot", 0, 1.2, 7, 0, 3.5, -30, 3 }, "meanwhile",      -- the sunset
    { "cue", "later" },
    { "title", "ONE YEAR LATER", "the arena, rebuilt", 3 },
    { "say", "granny", "nextyear", "smug" },
    -- Credits with the cast, the camera craning up over the arena.
    { "shot", 0.5, 8, 8, 0, 0, 0, 6.5 }, "meanwhile",
    { "credits", { { "THE CHAMPION", "GRANNY" }, { "THE CHALLENGER", "ROCKO" }, { "THANKS FOR PLAYING", "UI GALLERY" } }, 6 },
    { "fade", "out", 1.2 },
    done = function()
      print(("done %.2f"):format(round.time))
      covered = true
      timer.after(2, epilogue)
    end,
  }
end

function love.load()
  crash = love.audio.newSource("crash", "static")
  creak = love.audio.newSource("creak", "static")
  sting = love.audio.newSource("sting", "static")
  music.play("dusk", { volume = 0.6, fade = 1.5 })
  epilogue()
end

-- The fade out holds until the next showing fades in.
function love.draw()
  if covered then
    love.graphics.setColor(0, 0, 0, 1)
    love.graphics.rectangle("fill", 0, 0, 360, 640)
  end
end
