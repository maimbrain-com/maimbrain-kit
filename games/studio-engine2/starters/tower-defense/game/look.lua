-- The look: everything a reskin changes first. Words, the palette, which
-- picture is what and how big it's drawn. The art is images/ (painted clay,
-- generated for the tower-defense kit): the map with the road baked in,
-- the pad, the cake's five moods, the cherry bomb, three tower sheets
-- (levels 1–3) and six jelly sheets (walk A, walk B, pop). What moves and
-- what code does better (shadows, shots, rings, the picker, the coin pill,
-- the next-wave button) is drawn in game/draw.lua from these colours.

local look = {}

-- ---- Words ------------------------------------------------------------------
-- The game-over heading (and studio.toml's [text] over).
look.HEADING = "DEVOURED!"
-- In-play hints (5 words at most).
look.HINT_BUILD = "BUILD HERE"
look.HINT_BUY = "PICK A TOWER"
look.HINT_UPGRADE = "TAP TO UPGRADE"
-- Banners: a king wave, a rush wave, and a jelly kind's first wave.
look.BOSS_BANNER = "THE KING RETURNS!"
look.RUSH_BANNER = "ZIPPER RUSH!"
look.NEW_KIND_BANNER = { zipper = "ZIPPERS!", helmet = "HELMETS!", splitter = "SPLITTERS!", king = "KING JELLY!" }
-- Under the game-over card.
look.REACHED = "REACHED WAVE %d"
-- The game's own words (banners, hints, pills, prices): a library family.
look.FONT = "Fredoka"

-- ---- Palette ----------------------------------------------------------------
-- Picked from the clay art, so code-drawn things sit in the same world.
look.INK = 0x3a1f35 -- plum chocolate: outlines, panels
look.WHITE = 0xffffff
look.SHADOW = 0x24502c -- soft shadows on the lawn and the road (drawn at ~28%)
look.PAD_PLUS = 0xa8743e -- the "+" on an empty pad
look.ROAD_DOT = 0xe8cf9c -- the dimples that drift down the road
look.ROAD_DOT_SHADE = 0xc99a62
look.GOLD = 0xffd23f
look.DANGER = 0xff4f6d
look.FROST = 0x8fe3ff
-- Added over a slowed jelly (a cold blue sheen that keeps its own colour).
look.FROST_GLOW = 0x3c8cd0
look.GOOD = 0x7be38a -- an affordable upgrade
look.CREAM = 0xfff4dc -- picker buttons
-- POP's gumballs (the colours in its dome).
look.GUMBALLS = { 0xff7fa8, 0xffd23f, 0x7fcfff, 0xfff4e6 }
-- Jelly colours for splashes, by kind.
look.JELLY = { gummy = 0xff5d73, zipper = 0xffc93c, helmet = 0x9b4f9b, splitter = 0xff9f43, mini = 0xff9f43, king = 0xe8439b }
-- Tower colours for particles and the picker's rings, by kind.
look.TOWER = { pop = 0xff7fa0, chill = 0x9fe0ff, boom = 0xe0484b }
-- Crumbs and bite bursts.
look.CAKE_SPONGE = 0xf0c27a
look.CAKE_FROSTING = 0xff7f9c

-- ---- Pictures ---------------------------------------------------------------
-- Each sheet is one row of equal cells, bottom-anchored: frame i is the
-- cell at (i − 1) × cell width.
look.SHEETS = {
  pad = { cell = 112, frames = 1 },
  cake = { cell = 176, frames = 5 },
  bomb = { cell = 48, frames = 1 },
  tower_pop = { cell = 128, frames = 3 },
  tower_chill = { cell = 128, frames = 3 },
  tower_boom = { cell = 128, frames = 3 },
  jelly_gummy = { cell = 96, frames = 5 },
  jelly_zipper = { cell = 80, frames = 3 },
  jelly_helmet = { cell = 96, frames = 3 },
  jelly_splitter = { cell = 112, frames = 3 },
  jelly_mini = { cell = 64, frames = 3 },
  jelly_king = { cell = 144, frames = 3 },
}
-- The cake's moods: frames of its sheet.
look.CAKE_HAPPY, look.CAKE_NERVOUS, look.CAKE_PANIC, look.CAKE_WINCE, look.CAKE_GONE = 1, 2, 3, 4, 5
-- The gummy's extra frames: a hit squash, a smug full face (the aftermath).
look.GUMMY_HIT, look.GUMMY_SMUG = 3, 5
-- The pop frame of each jelly's sheet.
look.POP_FRAME = { gummy = 4, zipper = 3, helmet = 3, splitter = 3, mini = 3, king = 3 }
-- How wide a jelly's cell is drawn, in units per unit of its radius: the
-- walking body fills about 60% of the cell.
look.JELLY_CELL_PER_R = { gummy = 4.0, zipper = 4.4, helmet = 4.0, splitter = 3.8, mini = 4.4, king = 3.6 }
-- How wide a tower's cell is drawn (units), and how much bigger levels 1–3
-- are drawn per kind, so a small first-level model still reads on its pad.
look.TOWER_W = 80
look.TOWER_LEVEL_GROW = { pop = { 1.45, 1.2, 1.0 }, chill = { 1.35, 1.12, 1.0 }, boom = { 1.0, 1.0, 0.95 } }

-- ---- The picker -------------------------------------------------------------
-- Its round buttons: radius, the gap between their centres, and how far
-- above (on the lower half) or below (near the top) the pad they sit.
look.SLOT_R = 27
look.SLOT_GAP = 64
look.SLOT_OFFSET = 70

return look
