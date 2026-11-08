-- The look: every name, word and colour a reskin changes, in one place.
-- Shapes and physics are in sim.lua; how things are drawn is draw.lua.
-- The pictures are the kit's paper cut-outs: images/sprites.png (an atlas:
-- each picture's rect below), bird.png (four flapping frames), faces.png
-- (eight expressions laid over the snacks), paper.png (a paper grain tile).

local look = {}

-- ---- Words ---------------------------------------------------------------------
-- (The title, the tagline and the game-over heading are studio.toml's.)
-- The in-play hint (5 words at most), shown until the player has dropped a few.
look.HINT = "TAP TO DROP"
look.NEAT = "NEAT!"
-- A near miss: it only just stayed on.
look.CLOSE = "PHEW!"
-- Popups when a heart goes: a snack that missed, a tower that tumbled.
look.MISS = "OOPS!"
look.TUMBLE = "TUMBLE!"
look.GUST = "WIND!"
look.TILT = "WOBBLE!"
look.LAST_HEART = "LAST HEART!"
-- A streak won a heart back.
look.HEART_BACK = "+1 HEART"
-- "NEW: JELLY!" when a snack first arrives.
look.NEW = "NEW: %s!"
look.STREAK = "%d IN A ROW!"
-- The font for the game's own words (banners, popups, the hint): a library
-- family (`mb fonts` lists them, on a computer).
look.FONT = "Lilita One"

-- ---- Snacks ----------------------------------------------------------------------
-- Each snack's name (shown when it first arrives) and its three looks: a
-- picture in images/sprites.png ({ x, y, w, h } in its pixels) and a tint
-- multiplied over it (white keeps the picture's own colours). Pictures are
-- drawn stretched over the snack's collider, so a new picture should fill
-- its sim.KINDS box the same way (toast: a 3:2 rectangle, cheese: a
-- trapezoid, tomato: a circle).
local TOAST = { 666, 399, 132, 84 }
local BAGUETTE = { 599, 489, 222, 44 }
local CHEESE = { 227, 489, 168, 68 }
look.SNACKS = {
  box = { name = "TOAST", looks = { { TOAST, 0xffffff }, { TOAST, 0xf3e3d2 }, { TOAST, 0xfff3e2 } } },
  cube = { name = "JELLY", looks = { { { 326, 399, 87, 88 }, 0xffffff }, { { 415, 399, 87, 88 }, 0xffffff }, { { 236, 399, 88, 88 }, 0xffffff } } },
  plank = { name = "BAGUETTE", looks = { { BAGUETTE, 0xffffff }, { BAGUETTE, 0xf6e6d6 }, { BAGUETTE, 0xfff1dc } } },
  wedge = { name = "CHEESE", looks = { { CHEESE, 0xffffff }, { CHEESE, 0xfff2d4 }, { CHEESE, 0xffe7bc } } },
  bun = { name = "MACARON", looks = { { { 116, 489, 109, 72 }, 0xffffff }, { { 2, 489, 112, 72 }, 0xffffff }, { { 800, 399, 119, 72 }, 0xffffff } } },
  ball = { name = "TOMATO", looks = { { { 724, 304, 92, 92 }, 0xffffff }, { { 912, 304, 92, 91 }, 0xffffff }, { { 818, 304, 92, 92 }, 0xffffff } } },
}
-- The rest of the atlas.
look.ART = {
  pole = { 2, 2, 29, 300 },
  hills = { 33, 2, 622, 200 },
  sun = { 657, 2, 128, 128 },
  meadow = { 2, 304, 720, 93 },
  blanket = { 2, 399, 232, 88 },
  cloud1 = { 504, 399, 160, 86 },
  cloud2 = { 397, 489, 200, 62 },
  leaf = { 823, 489, 19, 32 },
  plate = { 2, 563, 340, 23 },
}
-- bird.png: four flapping frames, each cell this many pixels (feet at the bottom).
look.BIRD_CELL = { 200, 140 }
look.BIRD_FRAMES = 4
-- faces.png: eight expressions in a row (draw.lua's FACE_FRAME), cells this big.
look.FACE_CELL = { 96, 64 }

-- ---- Colours -------------------------------------------------------------------------
-- Plum ink: the darkest paper (text edges, the ghost's shade, paper shadows).
look.INK = 0x3a2433
look.WHITE = 0xffffff
-- Paper strips (wind streaks, tilt arrows).
look.PAPER_WHITE = 0xfff8ec
look.STAR = 0xfff4c8
look.GOLD = 0xffd84d
-- Hearts, misses and the last-heart pulse: tomato red.
look.DANGER = 0xe8564f
-- Sparkles and dust.
look.SPARKLE = 0xfff2b0
look.CRUMB = 0xfff3dc
-- Sky (top, bottom) by the tower's height in px: noon, golden hour, sunset,
-- night. Flat, papery colours: the paper grain is multiplied over them.
look.SKY = {
  { 0, 0x86cdee, 0xd9f1f8 },
  { 500, 0x79b6e8, 0xffe0a6 },
  { 1000, 0x8a6fd0, 0xffa47c },
  { 1600, 0x1c2350, 0x4b3b80 },
}
-- The ground below the hills and below the meadow (matched to the art's edges).
look.FIELD = 0x527237
look.FIELD_NEAR = 0x4f7a3a
-- The landing ghost and its dotted path (red when it would miss everything).
look.GHOST = 0xffffff
look.CONFETTI = { 0xe8564f, 0xffd84d, 0x8cc56a, 0x6fc3e8, 0xfff4e0 }

return look
