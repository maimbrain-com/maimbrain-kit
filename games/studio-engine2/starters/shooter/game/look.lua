-- The look: every word and colour a reskin changes. Shapes are in draw.lua,
-- the art in images/ (sprites.png, the atlas: SPRITES says where each
-- picture is in it; backdrop.png, the sky).

local look = {}

-- ---- Words -----------------------------------------------------------------
look.HEADING = "SHOT DOWN!" -- the game-over card's heading (studio.toml's [text] over too)
look.HINT = "DRAG TO FLY" -- under the ghost thumb until the player drags
look.BOSS_NAME = "GLOOM QUEEN"
look.CLOSE = "CLOSE!" -- a graze
look.NEW_ENEMY = "NEW ENEMY"
-- A new enemy type's banner.
look.NAMES = { jelly = "JELLIES!", swooper = "SWOOPERS!", darter = "DARTERS!", bulb = "BULBS!", spinner = "SPINNERS!" }
look.WARNING = { "WARNING", "THE GLOOM QUEEN" }
look.CLEAR = { "QUEEN DOWN!", "AGAIN, FASTER" }
look.POWER_NAMES = { spread = "SPREAD!", shield = "SHIELD!", magnet = "MAGNET!" }

-- ---- The art -----------------------------------------------------------------
-- Where each picture is in images/sprites.png (x, y, w, h in its pixels).
-- The art is drawn at half size: 2 pixels a unit.
look.SPRITES = {
  boss = { 2, 2, 320, 256 },
  bulb = { 324, 2, 128, 128 },
  pip = { 2, 260, 128, 128 },
  spinner = { 132, 260, 128, 128 },
  swooper = { 2, 390, 448, 112 }, -- 4 flap frames, 112 square each
  darter = { 2, 504, 96, 96 },
  jelly = { 100, 504, 96, 96 },
  hand = { 198, 504, 64, 64 },
  heart = { 264, 504, 64, 64 },
  magnet = { 330, 504, 64, 64 },
  shield = { 396, 504, 64, 64 },
  spread = { 2, 602, 64, 64 },
  gem = { 68, 602, 48, 48 },
}
-- The sizes things are drawn at (units), and where each face window is in
-- its picture (from the picture's middle): the code draws eyes there.
look.PIP_SIZE = 64
look.PIP_VISOR = { 0, -4.4 }
look.PIP_NOZZLE = 25
look.PIP_WINGTIP = { 24, 8 }
look.JELLY_SIZE, look.JELLY_FACE_Y = 46, -4.5
look.SWOOPER_SIZE, look.SWOOPER_LIFT = 56, 13
look.SWOOPER_FACE_Y = { 15.1, 15.1, 10.9, 15.1 } -- the face's y in each flap frame
look.DARTER_SIZE, look.DARTER_EYE = 48, 0.8
look.BULB_SIZE, look.SPINNER_SIZE = 64, 58
look.BOSS_SIZE, look.BOSS_LIFT = 160, 6
look.BOSS_FACE = { -0.5, 1.5 }
look.HAND_SIZE, look.GEM_SIZE, look.POWER_SIZE, look.HEART_SIZE = 30, 28, 38, 34
-- The backdrop's horizon line (units): the floor grid starts here.
look.HORIZON = 352

-- ---- Colours -------------------------------------------------------------------
-- Neon synthwave: deep indigo night, magenta and cyan neon, a gold sun. The
-- art is generated; these are for everything drawn in code (the grid,
-- bullets, glows, faces, banners) so it matches the art.

-- The backdrop is drawn tinted by this (a little dimmer than the art, so
-- bullets pop), warming toward BG_HOT_TINT as the heat rises.
look.BG_TINT = 0xc4bcdc
look.BG_HOT_TINT = 0xe4b4d2
look.BG = 0x140c3c
-- The floor grid below the horizon, and the horizon's glow.
look.GRID = 0xff2fb4
look.GRID_HOT = 0xff7a3a
look.HORIZON_GLOW = 0xff5ad0
look.STAR = 0xdff4ff
look.STREAK = 0x7ff3ff -- speed streaks falling past

-- Pip's engine and face (the pod itself is the art).
look.SHIP_TRIM = 0x22e6ff
look.SHIP_TRIM_DARK = 0x1a8fb8
look.INK = 0x0a0820
look.EYE = 0xf4f0ff
look.FLAME_IN = 0xe8ffff
look.FLAME_OUT = 0x22c8ff
look.FLAME_EDGE = 0xff2fb4
look.SHIELD = 0x5f9bff
look.WRECK_TINT = 0x6a5e86 -- Pip's wreck under the game-over card
look.SWEAT = 0x8fd8ff

-- The ship's shots: cool cyan neon, nothing like the enemy bullets.
look.SHOT = 0x8ffcff

-- Each enemy's neon (glows, bursts, banners, brows).
look.ENEMY = { jelly = 0xff4fd0, swooper = 0xff9a3c, darter = 0xb46bff, bulb = 0xffc23a, spinner = 0xff5a6e }
look.BULB_IRIS = 0x2a4fd0

-- Enemy bullets: hot pink rims around white-hot cores, inside a dark outline
-- so they read on the sun and the grid alike. Big ones orange.
look.BULLET_RIM = 0xff2f86
look.BULLET_BIG_RIM = 0xff7a1a
look.BULLET_CORE = 0xffffff
look.BULLET_OUTLINE = 0x0a0418

look.GEM = 0xffc23a
look.HEART = 0xff4f6b
look.POWER = { spread = 0x22e6ff, shield = 0x5f9bff, magnet = 0xff2fb4 }

look.BOSS = 0xc04dff
look.BOSS_NEON = 0xff3fc8 -- the queen's tentacles and arms
look.BOSS_NEON_2 = 0x22e6ff
look.BOSS_RAGE = 0xff8a9a
look.DANGER = 0xff3355
look.GOLD = 0xffc23a
look.WHITE = 0xffffff
look.CONFETTI = { 0x22e6ff, 0xffc23a, 0xff2fb4, 0xb46bff, 0xffffff }

return look
