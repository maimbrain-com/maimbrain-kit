-- The look: the sky, the light, the fog and the final image, and every
-- shape, material, colour and particle main.lua builds the world from.
-- main.lua holds the rules and where things go; this holds what they look
-- like.
--
-- This is the greybox's look: grey shapes in flat, even light, a stand-in
-- for the game's own. Make all of it the game's, in DESIGN.md's look: the
-- sky and light, the rocks, the gap markers, the player, the particles and
-- the HUD's text.

local look = {}

sky { seed = 1, colors = { { 0.3, 0.3, 0.3 }, { 0.2, 0.2, 0.2 }, { 0.12, 0.12, 0.12 } }, nebula = 0, stars = 0, sun_disc = 0 }
sun { dir = { 0.3, -0.8, -0.5 }, color = { 1, 1, 1 }, intensity = 2.5, shadows = false }
-- Fog hides the far rings as they're laid out.
fog { color = { 0.2, 0.2, 0.2 }, mode = "linear", start = 50, ["end"] = 150 }
post { exposure = 1.0 }

-- A ring's rocks: grey blocks, tinted per instance (look.rock_tints).
look.rock_mesh = mesh.box(0.7, 0.7, 0.7):color(0xffffffff)
look.rock = material { rough = 0.9 }
look.rock_tints = { 0x9a9a9aff, 0x8a8a8aff, 0xaaaaaaff }

-- The gap: small markers along its arc and a bigger one at each edge, white
-- on a still ring, dark on a turning one.
look.marker_mesh = mesh.sphere(0.09, 8, 6)
look.edge_mesh = mesh.sphere(0.22, 10, 8)
look.marker = material { color = 0xffffffff, unlit = true }
look.gap_still = 0xffffffff
look.gap_turning = 0x333333ff

-- The player: a plain white ball.
function look.player()
  return node(mesh.sphere(0.25, 16, 12), material { color = 0xf0f0f0ff, rough = 0.6 })
end

-- Particles, all grey: a trail, a pass, a crash.
look.trail = emitter {
  colors = { { 1, 1, 1, 1 }, { 0.7, 0.7, 0.7, 0.8 }, { 0.4, 0.4, 0.4, 0.5 }, { 0, 0, 0, 0 } },
  sizes = { 0.05, 0.04, 0.025, 0 }, life = { 0.2, 0.4 }, speed = { 1.0, 3.0 },
  spread = 0.7, drag = 1.5, stretch = 0.02,
}
look.pass = emitter {
  colors = { { 1, 1, 1, 1 }, { 0.8, 0.8, 0.8, 0.8 }, { 0.5, 0.5, 0.5, 0.4 }, { 0, 0, 0, 0 } },
  sizes = { 0.08, 0.07, 0.05, 0 }, life = { 0.3, 0.6 }, speed = { 2, 5 }, spread = 1.0, drag = 3.0, stretch = 0.03,
}
look.crash = emitter {
  colors = { { 0.9, 0.9, 0.9, 1 }, { 0.5, 0.5, 0.5, 1 }, { 0.3, 0.3, 0.3, 0.8 }, { 0, 0, 0, 0 } },
  sizes = { 0.22, 0.18, 0.14, 0.05 }, life = { 1.2, 2.2 }, speed = { 2, 7 }, spread = 1.0, drag = 0.8,
}

-- The HUD's text: the score, and the hint at the start.
look.score = { size = 40, color = 0xffffffff, shadow = true }
look.hint = { size = 16, color = 0xffffffc0 }

return look
