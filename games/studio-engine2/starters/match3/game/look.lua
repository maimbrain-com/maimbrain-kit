-- Everything a reskin changes first: words, colours, the board's place on
-- screen, which picture each poppet is. The painted art is images/ (the
-- cast in images/cast/, the window in bg.png, the paper grain in
-- paper.png); faces, specials and effects are drawn in code (draw.lua).

local look = {}

-- ---- Words ------------------------------------------------------------------
-- The game-over heading (and studio.toml's [text] over).
look.HEADING = "NAP TIME!"
-- The in-play hint that teaches the swipe (5 words at most).
look.HINT = "SWIPE TO SWAP"
-- What each level's news says.
look.NEWS = {
  faster = "FASTER!",
  newColor = "NEW POPPET!",
  rocks = "ROCKS INCOMING!",
  stars = "STAR POPPETS!",
  moreRocks = "MORE ROCKS!",
}
-- The font for the game's own words (banners, the hint, the z's): a library
-- family (`mb fonts` lists them, on a computer).
look.FONT = "Titan One"

-- ---- Board geometry ---------------------------------------------------------
-- Cell size in units (anything a thumb must hit wants 44 or more).
look.CELL = 46
-- Top left of the board.
look.BOARD_X = (360 - 46 * 7) / 2
look.BOARD_Y = 176

-- ---- Art --------------------------------------------------------------------
-- The picture for each poppet kind (images/cast/): coral puffball, apricot
-- marshmallow square, butter rice-ball triangle, sage sprout hexagon,
-- powder-blue cloud, lavender star, bubblegum heart. Shape AND colour
-- differ, so it reads colourblind.
look.SPRITES = { "cast/coral", "cast/orange", "cast/yellow", "cast/green", "cast/blue", "cast/purple", "cast/pink" }
look.RAINBOW = "cast/rainbow"
look.ROCK = "cast/rock"
-- A picture is drawn this many piece radii wide (the painted body fills
-- ~84 % of its 128 px, so ~2.3 makes the body about one radius from centre
-- to edge).
look.SPRITE_SPAN = 2.3
-- Where each kind's face sits (piece radii below centre): the triangle's is
-- low in its wide bottom, the heart's high between its lobes. 8 is the
-- rock, 9 the rainbow.
look.FACE_Y = { 0.02, 0.0, 0.2, 0.1, 0.02, 0.1, -0.06, 0.08, 0.04 }
-- Faces are drawn this much bigger than their unit layout (bigger reads sleepier).
look.FACE_SCALE = 1.15

-- ---- Colours ----------------------------------------------------------------
-- One colour per poppet kind, matched to its painted body: particles, banners.
look.PIECES = { 0xe8695f, 0xf0a050, 0xf2c95e, 0x7fb88a, 0x86b2e8, 0xb294e0, 0xee86c4 }
-- Pencil ink for faces, outlines and text outlines (a warm dark plum).
look.INK = 0x34203f
look.ROCK_COLOR = 0x9a9088
-- Rosy cheeks.
look.BLUSH = 0xff8a9a
-- Drowsiness tints the painted room: awake, no tint; asleep, this.
look.NIGHT_TINT = 0x6a72b8
-- The bedside lamp's warm glow while everyone's awake (added light).
look.LAMP = 0xffc884
-- The board: an indigo wash on paper, paler cells, a pencil border.
look.BOARD_WASH = { 0x23 / 255, 0x1d / 255, 0x4a / 255, 0xd8 / 255 }
look.CELL_WASH = { 1, 0xf3 / 255, 0xdc / 255, 0x1c / 255 }
look.PENCIL = 0xf6eedd
-- The wake meter: full, half, nearly empty (sage, butter, coral).
look.METER = { 0x8fd09a, 0xf2d37a, 0xe8695f }
look.GOLD = 0xf6d77e
-- The accent (banners), as the identity's.
look.ACCENT = 0xe2706f

return look
