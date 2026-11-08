-- What a reskin changes first: the words, the colours, each category's
-- emblem, and where things sit. The pictures are images/ (the stage
-- backdrop, Watt's sheet of seven faces, the category emblems, the
-- starburst); the cards, buttons, clock and hearts are the identity's
-- widgets (studio.toml's [identity]), tinted from here.

local look = {}

-- ---- Words ------------------------------------------------------------------
look.HINT = "TAP THE ANSWER" -- until the first answer (five words at most, a verb first)
look.WARMUP = "WARM-UP"
look.GOLDEN = "GOLDEN ×2"
look.TIME_UP = "TIME!"
look.QUICK = "QUICK!"
look.OVER = "LIGHTS OUT!" -- the game-over heading (studio.toml's [text] over too)
-- The stats card's title every sim.ROUND_QUESTIONS (10) questions: the
-- round of the show just played. (Every fifth question is golden.)
look.PART = "ROUND %d"
-- The game-over card's line: right answers and the best streak.
look.SUMMARY = "%d RIGHT · BEST STREAK %d"

-- ---- Colours (1950s game-show print) ------------------------------------------
look.INK = 0x2a1e1a
look.PAPER = 0xfbf1dc
look.CREAM = 0xffe9a8
look.MUSTARD = 0xf2a922
look.TOMATO = 0xe2412f
look.TEAL = 0x1f7a7a
-- An answer button's tint (its face is the identity's accent, paper): white
-- leaves it paper, these make it the right and the wrong answer.
look.RIGHT = 0x63c46f
look.WRONG = 0xff6a55
-- The golden question's card.
look.GOLD = 0xffd769
-- Watt's glow behind his glass, and the red that creeps in from the edges
-- as the clock runs out.
look.GLOW = 0xffd75a
look.ALARM = 0xd8352a

-- ---- Watt's faces: frames of images/host.png (4 across, 320×352 each) --------
look.FACE = { idle = 1, think = 2, nervous = 3, happy = 4, star = 5, flicker = 6, cracked = 7 }
-- Where the glass's middle is in a frame (0–1), and its radius in pixels.
look.GLASS = { 0.5, 165 / 352, 90 }

-- ---- Categories: { frame of images/cats.png, colour } -------------------------
-- Add a line here (and an emblem to the sheet) for a category of your own;
-- one with no line gets a plain pill and no emblem.
look.CATEGORIES = {
  SCIENCE = { 1, 0x1f7a7a },
  SPACE = { 2, 0x34507e },
  WORLD = { 3, 0x2c8a68 },
  NATURE = { 4, 0x5c8a32 },
  FOOD = { 5, 0xd2691e },
  NUMBERS = { 6, 0xc8382c },
  WORDS = { 7, 0x8a4a78 },
  INVENTIONS = { 8, 0x7a6656 },
  BODY = { 9, 0xb83040 },
  TIME = { 10, 0xb87a12 },
}

-- A category's colour, lightened for its pill (the pill's ink reads on it).
function look.light(c)
  local function ch(s) return math.floor(bit32.extract(c, s, 8) * 0.45 + 255 * 0.55) end
  return ch(16) * 65536 + ch(8) * 256 + ch(0)
end

-- ---- Where things sit, from the safe area -------------------------------------
-- The four answers stack in the lower half under the thumb; the question
-- card sits on them, Watt peeks over its top-left corner with the clock
-- at the right and the streak between them.
function look.layout(sx, sy, sw, sh)
  local L = {}
  local h, gap = 50, 9
  local bottom = sy + sh - 14
  local top = bottom - 4 * h - 3 * gap
  L.answers = {}
  for i = 1, 4 do
    L.answers[i] = { x = sx + sw / 2, y = top + (i - 1) * (h + gap) + h / 2, w = sw - 40, h = h }
  end
  L.card = { x = sx + sw / 2, y = top - 20 - 64, w = sw - 36, h = 128 }
  L.host = { x = sx + 74, y = L.card.y - 64 - 56 }
  L.timer = { x = sx + sw - 58, y = L.host.y + 4 }
  L.streak = { x = sx + sw / 2 + 6, y = L.host.y + 6 }
  L.hearts = { x = sx + sw - 20, y = sy + 74 }
  -- The game-over card: the blown bulb under its retry button, the line of
  -- right answers and the best streak above SCORES and DAILY.
  L.overHost = { x = sx + sw / 2, y = sy + sh * 0.515 }
  L.overLine = { x = sx + sw / 2, y = sy + sh * 0.615 }
  -- The title card: a question card between the title and DAILY, Watt
  -- bigger behind it.
  L.titleCard = { x = sx + sw / 2, y = sy + sh * 0.62, w = sw - 36, h = 128 }
  L.titleHost = { x = sx + 70, y = L.titleCard.y - 64 - 62 }
  L.titleTimer = { x = sx + sw - 56, y = L.titleCard.y - 64 - 48 }
  -- A paper sign under the marquee, for the tagline to read on.
  L.titleSign = { x = sx + sw / 2, y = sy + 149, w = 214, h = 30 }
  return L
end

return look
