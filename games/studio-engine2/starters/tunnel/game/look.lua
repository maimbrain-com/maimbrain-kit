-- The look: the sky, the light, the fog and the final image, and every
-- shape, material, colour and particle main.lua builds the world from.
-- main.lua holds the rules and where things go; this holds what they look
-- like.
--
-- This is the starter's look, a lava tube: scorched rock rings, glowing
-- ember markers on the gaps, a spark of fire for the player, ash and
-- embers in a red haze. Make all of it the game's own, in DESIGN.md's look:
-- the sky and light, the rocks, the gap markers, the player, the particles
-- and the hint.

local look = {}

-- A deep red glow far down the tube, warm light from above, a haze that
-- swallows the far rings.
sky { seed = 4, colors = { { 0.10, 0.015, 0.01 }, { 0.22, 0.05, 0.02 }, { 0.04, 0.01, 0.01 } }, nebula = 0.35, stars = 0, sun_disc = 0 }
sun { dir = { 0.25, -0.85, -0.45 }, color = { 1.0, 0.62, 0.38 }, intensity = 2.6, shadows = false }
fog { color = { 0.16, 0.035, 0.015 }, mode = "exp2", density = 0.02, sky = 0.5 }
post { exposure = 1.15, bloom = 0.75, bloom_threshold = 0.95, vignette = 0.5, saturation = 1.1, contrast = 1.06 }

-- A ring's rocks: lumpy faceted lava rock, tinted per instance (look.rock_tints).
local INK = { 0.02, 0.005, 0.0 }
look.rock_mesh = mesh.ico(0.42, 1):scale(1, 0.85, 1.1):color(0xffffffff):flat()
look.rock = material { rough = 0.9, style = { toon = 3, rim = 0.4, outline = 1.1, ink = INK } }
look.rock_tints = { 0x4a2e2aff, 0x5e3a2eff, 0x3a2624ff, 0x6a4030ff }

-- The gap: glowing embers along its arc and a bigger one at each edge,
-- gold on a still ring, hot red on a turning one.
look.marker_mesh = mesh.ico(0.09, 1)
look.edge_mesh = mesh.ico(0.22, 1)
look.marker = material { color = { 0, 0, 0, 1 }, emissive = { 1, 1, 1 }, glow = 3.0, unlit = true }
look.gap_still = 0xffc04aff
look.gap_turning = 0xff4a1aff

-- The player: a white-hot spark in a flame shell.
function look.player()
  local p = node()
  node(mesh.ico(0.17, 2), material { color = { 0, 0, 0, 1 }, emissive = { 2.6, 2.0, 1.2 }, glow = 1.2, unlit = true }):parent(p)
  node(mesh.ico(0.28, 2), material { color = { 1.0, 0.4, 0.1, 0.35 }, unlit = true, additive = true, alpha = true }):parent(p)
  return p
end

-- Particles: a trail of embers, a burst through a gap, and a crash of
-- molten rock.
look.trail = emitter {
  colors = { { 1, 0.95, 0.7, 1 }, { 1, 0.6, 0.2, 0.9 }, { 0.8, 0.2, 0.05, 0.5 }, { 0, 0, 0, 0 } },
  sizes = { 0.06, 0.05, 0.03, 0 }, life = { 0.2, 0.45 }, speed = { 1.0, 3.0 },
  spread = 0.7, drag = 1.5, stretch = 0.03,
}
look.pass = emitter {
  colors = { { 1, 0.9, 0.5, 1 }, { 1, 0.55, 0.15, 0.8 }, { 0.7, 0.2, 0.05, 0.4 }, { 0, 0, 0, 0 } },
  sizes = { 0.09, 0.07, 0.05, 0 }, life = { 0.3, 0.6 }, speed = { 2, 5 }, spread = 1.0, drag = 3.0, stretch = 0.04,
}
look.crash = emitter {
  colors = { { 1, 0.8, 0.4, 1 }, { 0.9, 0.35, 0.1, 1 }, { 0.3, 0.12, 0.08, 0.8 }, { 0, 0, 0, 0 } },
  sizes = { 0.22, 0.18, 0.14, 0.05 }, life = { 1.2, 2.2 }, speed = { 2, 7 }, spread = 1.0, drag = 0.8,
}

-- The hint at the start, drawn in love.draw (the score is the HUD's, in
-- the game's identity).
look.hint = { 1, 0.86, 0.66, 0.85 }

return look
