-- The look. Everything a reskin changes lives here: the words, the sky
-- that drifts day → dusk → night → dawn with distance, and how the pixel
-- art is drawn: the hero, the obstacles, coins, the scenery and the ground.
--
-- The art is images/*.png: 3 texels to an art pixel, drawn at half size, so
-- an art pixel is 1.5 units (3 of the phone's pixels) and nothing is
-- resampled. The sky, sun, clouds and the three hill layers are nodes the
-- engine draws behind love.draw; the world in front (pits, coins,
-- obstacles, the hero, the ground) is drawn here each frame. Code also draws
-- the sky's bands, the stars, the grass fringe, sweat and dizzy stars.
--
-- The rules (sim.lua) know none of it: to them the hero is a 24 × 40 box
-- and a gull is a box that flies. Swap the bun for a robot, the crates for
-- cacti and the gulls for drones here, and the game plays the same.

local sim = require("sim")
local left, bottom = sim.left, sim.bottom
local g = love.graphics
local floor, min, max, sin, cos, pi = math.floor, math.min, math.max, math.sin, math.cos, math.pi
local TAU = 2 * pi

local look = {}

-- ---- Words ---------------------------------------------------------------
-- (The title and tagline are studio.toml's.)
-- Game-over headings: hit something / fell in a pit.
look.HEADING_BONK = "BONK!"
look.HEADING_PIT = "WHOOPS!"
look.HINT_TAP = "TAP TO JUMP"
look.HINT_AGAIN = "TAP AGAIN!"
look.FASTER = "FASTER!"
look.CLOSE = "CLOSE!"
-- Every coin of an arc collected.
look.NICE = "NICE!"
-- The banner when a family first shows up.
look.NAMES = {
  crate = "CRATES!", pit = "PITS!", stack = "TALL STACKS!", gull = "HUNGRY GULLS!",
  bee = "BUSY BEES!", skimmer = "LOW GULLS!", log = "LOGS!",
}
-- The hint while the first of a family comes at you (≤ 5 words).
look.HINTS = {
  pit = "JUMP THE GAP", stack = "HOLD TO JUMP HIGHER", gull = "STAY LOW!",
  bee = "TIME IT!", skimmer = "JUMP IT!", log = "HOLD FOR A LONG JUMP",
}

-- ---- Layout --------------------------------------------------------------
-- The ground line on screen: lower-middle, above the thumb.
local GROUND_Y = 430
local W, H = 360, 640
look.GROUND_Y, look.W, look.H = GROUND_Y, W, H

-- ---- Palette (the art's, for what code draws) -----------------------------
local INK = 0x1a1230
local GOLD = 0xffe23a
local RED = 0xec3b35
local LEAF_DARK = 0x1c5a3a
local LEAF_LIGHT = 0xb6f075
local SKY_PALE = 0xc4ecff
local SKY_DEEP = 0x6fc3ff
local CREAM = 0xfff1c2
local WHITE = 0xffffff
look.INK, look.GOLD, look.RED, look.WHITE = INK, GOLD, RED, WHITE
look.CONFETTI = { RED, GOLD, 0x6bd04a, 0x6fc3ff, 0xffffff }

-- 0xRRGGBB's channels, 0–1.
local function rgb(c)
  return floor(c / 65536) % 256 / 255, floor(c / 256) % 256 / 255, c % 256 / 255
end

-- Draws in colour c (0xRRGGBB) at alpha a.
local function color(c, a)
  local r, gg, b = rgb(c)
  g.setColor(r, gg, b, a or 1)
end
look.color = color

-- Two colours mixed: k = 0 is a, 1 is b.
local function mix(a, b, k)
  local ar, ag, ab = floor(a / 65536) % 256, floor(a / 256) % 256, a % 256
  local br, bg, bb = floor(b / 65536) % 256, floor(b / 256) % 256, b % 256
  return floor(ar + (br - ar) * k + 0.5) * 65536 + floor(ag + (bg - ag) * k + 0.5) * 256 + floor(ab + (bb - ab) * k + 0.5)
end

-- Units an art pixel: 3 texels, drawn at half size.
local PX = 1.5

-- Rounds to the phone's pixel grid (half units), so pixels stay crisp.
local function snap(v)
  return floor(v * 2 + 0.5) * 0.5
end
look.snap = snap

-- A repeatable pseudo-random number in 0…1 for an integer (scenery layout).
local function hash(i, salt)
  local x = sin(i * 12.9898 + salt * 78.233) * 43758.5453
  return x - floor(x)
end

-- ---- The art ---------------------------------------------------------------

local img = {} -- name → { image, w, h }
local quad -- one quad, pointed at each piece before it's drawn
local fonts: { [string]: Font } = {}

-- Images, fonts and the backdrop's nodes. Call once, in love.load.
function look.load()
  for _, name in { "hero_run", "hero_air", "gull", "skimmer", "bee", "coin", "crate", "log", "dirt", "grass" } do
    local i = g.newImage(name)
    img[name] = { image = i, w = i:getWidth(), h = i:getHeight() }
  end
  quad = g.newQuad(0, 0, 1, 1, 1, 1)
  fonts.hint = g.newFont("bold", 24)
  fonts.ribbon = g.newFont("Press Start 2P", 16)
  fonts.pill = g.newFont("bold", 20)
  fonts.mark = g.newFont("bold", 18)
  look.fonts = fonts
  look.backdrop()
end

-- Texels (sx, sy, sw, sh) of an image, top left at (x, y), at their size.
local function blit(name, sx, sy, sw, sh, x, y)
  local a = img[name]
  quad:setViewport(sx, sy, sw, sh, a.w, a.h)
  g.draw(a.image, quad, snap(x), snap(y), 0, 0.5, 0.5)
end

-- Frame i (from 0) of an n-frame strip, top left at (x, y).
local function cell(name, i, n, x, y)
  local a = img[name]
  local fw = a.w / n
  blit(name, fw * (i % n), 0, fw, a.h, x, y)
end

-- One axis of a sliced sprite, in art pixels: the first part, the middle
-- (repeated, never stretched) and the last part. Where each piece of it
-- goes: { texel offset in the sprite, start, length in units }, flat.
local function spans(s, at, len)
  local a, c = (s[2] - s[1]) * PX, (s[6] - s[5]) * PX
  local k = if a + c > len then len / (a + c) else 1
  a, c = snap(a * k), snap(c * k)
  local out = { s[1] * 3, at, a }
  local tile = (s[4] - s[3]) * PX
  local x, stop = at + a, at + len - c
  while tile > 0 and x < stop - 0.01 do
    local d = min(stop - x, tile)
    out[#out + 1] = s[3] * 3
    out[#out + 1] = x
    out[#out + 1] = d
    x += d
  end
  out[#out + 1] = s[6] * 3 - c * 2
  out[#out + 1] = stop
  out[#out + 1] = c
  return out
end

-- Draws an image into (x, y, w, h) at any size: the edges and corners at
-- their size, the middles repeated (pixel art can't stretch).
local function sliced(name, xs, ys, x, y, w, h)
  local a = img[name]
  x, y = snap(x), snap(y)
  local cx, cy = spans(xs, x, snap(w)), spans(ys, y, snap(h))
  for i = 1, #cx, 3 do
    local u, px, pw = cx[i], cx[i + 1], cx[i + 2]
    if pw > 0.01 then
      for j = 1, #cy, 3 do
        local v, py, ph = cy[j], cy[j + 1], cy[j + 2]
        if ph > 0.01 then
          quad:setViewport(u, v, pw * 2, ph * 2, a.w, a.h)
          g.draw(a.image, quad, px, py, 0, 0.5, 0.5)
        end
      end
    end
  end
end

-- Fills (x, y, w, h) with copies of a tile (texels sx, sy, sw, sh) starting
-- at (ox, oy), clipped to the box.
local function tiled(name, sx, sy, sw, sh, x, y, w, h, ox, oy)
  local a = img[name]
  local tw, th = sw * 0.5, sh * 0.5
  local right, bot = x + w, y + h
  local ty = oy
  while ty < bot do
    local y0, y1 = max(ty, y), min(ty + th, bot)
    local tx = ox
    while tx < right do
      local x0, x1 = max(tx, x), min(tx + tw, right)
      if x1 > x0 and y1 > y0 then
        quad:setViewport(sx + (x0 - tx) * 2, sy + (y0 - ty) * 2, (x1 - x0) * 2, (y1 - y0) * 2, a.w, a.h)
        g.draw(a.image, quad, x0, y0, 0, 0.5, 0.5)
      end
      tx += tw
    end
    ty += th
  end
end

-- A square art pixel (or a block of w × h of them) in code.
local function pixel(x, y, w, h)
  g.rectangle("fill", snap(x), snap(y), w * PX, h * PX)
end

-- ---- The sky ---------------------------------------------------------------
-- One moment of the day: the sky's colours, how each depth is lit (tints:
-- white = as drawn) for the far mountains, the hills, the near trees, the
-- clouds, the ground and the obstacles; the sun or moon's height (0 =
-- horizon, 1 = high), the stars' alpha, how dark the world is (0 day … 1).

local DAY = { top = 0x3d8fe8, bottom = 0xc4ecff, far = 0xffffff, mid = 0xffffff, near = 0xb8d4c4, cloud = 0xffffff, ground = 0xffffff, prop = 0xffffff, sun_up = 1, stars = 0, dim = 0 }
local DUSK = { top = 0x5a4a9c, bottom = 0xffa86b, far = 0xe0a0c0, mid = 0xd8a8a0, near = 0xc09090, cloud = 0xffc0b0, ground = 0xe0b8a8, prop = 0xf0d0c0, sun_up = 0.25, stars = 0.15, dim = 0.25 }
local NIGHT = { top = 0x0b1236, bottom = 0x24418f, far = 0x3a4a8a, mid = 0x34477c, near = 0x2a3a68, cloud = 0x5a6aa0, ground = 0x5a6aa8, prop = 0x9aa4d8, sun_up = 0.8, stars = 1, dim = 0.55 }
local DAWN = { top = 0x6a8ad8, bottom = 0xffc7a0, far = 0xc8c0e8, mid = 0xd0d8d0, near = 0xb8c8c0, cloud = 0xffe0d0, ground = 0xe8dcd0, prop = 0xf8f0e8, sun_up = 0.2, stars = 0.2, dim = 0.15 }
local KEYS = { DAY, DUSK, NIGHT, DAWN }

-- Meters from one time of day to the next (day → dusk → night → dawn → day).
local CYCLE_M = 260

-- The sky after running `meters`.
local sky = {}
function look.skyAt(meters)
  local p = (meters / CYCLE_M) % 4
  local i = floor(p)
  -- Hold each look for a while, then blend to the next.
  local k = math.clamp((p - i - 0.55) / 0.45, 0, 1)
  k = k * k * (3 - 2 * k)
  local a, b = KEYS[i % 4 + 1], KEYS[(i + 1) % 4 + 1]
  for _, f in { "top", "bottom", "far", "mid", "near", "cloud", "ground", "prop" } do
    sky[f] = mix(a[f], b[f], k)
  end
  for _, f in { "sun_up", "stars", "dim" } do
    sky[f] = a[f] + (b[f] - a[f]) * k
  end
  return sky
end

-- ---- The backdrop: nodes behind love.draw -----------------------------------

local bands, stars, sun, moon, clouds, layers, hazes = {}, {}, nil, nil, {}, {}, {}
local BANDS, STARS = 16, 46
local SKY_TOP = -40
local BAND = math.ceil((GROUND_Y + 10 - SKY_TOP) / BANDS / PX) * PX
-- The hill layers, far to near: image, parallax, how far down, haze.
local LAYERS = { { "hills_1", 0.06, 120, 0.45 }, { "hills_2", 0.2, 20, 0.2 }, { "hills_3", 0.45, 175, 0 } }

function look.backdrop()
  -- A banded sky, 16-bit style: flat steps from top to horizon (a margin
  -- all round: the screen shakes).
  for i = 1, BANDS do
    bands[i] = Shape{ w = W + 80, h = BAND, x = W / 2, y = SKY_TOP + BAND * (i - 0.5), order = -100 }
  end
  -- Stars: single pixels, a few bigger twinkling crosses.
  local field = Group{ order = -99, visible = false }
  for i = 1, STARS do
    local s = Shape{ parent = field, w = PX, h = PX, color = WHITE, y = snap(hash(i, 2) * (GROUND_Y - 170)) }
    if hash(i, 4) > 0.8 then
      for _, d in { { -PX, 0 }, { PX, 0 }, { 0, -PX }, { 0, PX } } do
        Shape{ parent = s, w = PX, h = PX, color = WHITE, x = d[1], y = d[2], alpha = 0.5 }
      end
    end
    stars[i] = s
  end
  stars.field = field
  -- The sun by day, the moon by night.
  sun = Sprite{ image = "sun", scale = 0.5, x = 272, order = -98 }
  moon = Sprite{ image = "moon", scale = 0.5, x = 272, order = -98, alpha = 0 }
  -- Clouds drift and scroll a little.
  for i = 0, 3 do
    clouds[i + 1] = Sprite{ image = "cloud", frame = i % 3 + 1, scale = 0.5, y = snap(70 + hash(i, 7) * 130), order = -97 }
  end
  -- Mountains, hills, trees: further = slower. Each layer is 720 units wide
  -- and wraps; the near trees sit low so their crowns read as a hedge.
  -- Aerial haze: further layers fade toward the horizon's colour.
  for i, l in LAYERS do
    layers[i] = Layer{ image = l[1], scale = 0.5, repeatX = true, parallax = 0, y = GROUND_Y + 10 + l[3] - 480, order = -90 + i * 2 }
    if l[4] > 0 then
      local top = GROUND_Y - 190
      hazes[i] = Shape{ w = W + 80, h = GROUND_Y + 10 - top, x = W / 2, y = (top + GROUND_Y + 10) / 2, order = -89 + i * 2 }
    end
  end
end

-- Each frame: the backdrop for this sky, `scroll` world px run, at time t.
function look.updateBackdrop(s, scroll, t)
  for i = 1, BANDS do
    local k = (i - 1) / (BANDS - 1)
    bands[i].color = mix(s.top, s.bottom, k * k)
  end
  local on = s.stars > 0.01
  stars.field.visible = on
  if on then
    stars.field.alpha = s.stars
    for i = 1, STARS do
      local st = stars[i]
      st.x = snap((hash(i, 1) * W - scroll * 0.01) % W)
      st.alpha = 0.55 + 0.45 * sin(t * (1 + hash(i, 3) * 2) + i)
    end
  end
  local sy = snap(GROUND_Y - 100 - 140 * s.sun_up)
  local m = math.clamp((s.stars - 0.4) / 0.4, 0, 1)
  sun.y, sun.alpha, sun.visible = sy, 1 - m, m < 1
  moon.y, moon.alpha, moon.visible = sy, m, m > 0
  for i = 1, 4 do
    local c = clouds[i]
    c.x = snap(((i - 1) * 151 - scroll * 0.06 - t * 5) % (W + 160) - 80)
    c.color = s.cloud
  end
  local tints = { s.far, s.mid, s.near }
  for i, l in LAYERS do
    local layer = layers[i]
    layer.x = -snap((scroll * l[2]) % 720)
    layer.color = tints[i]
    local hz = hazes[i]
    if hz then
      hz.color = s.bottom
      hz.alpha = l[4] * (1 - s.dim)
    end
  end
end

-- ---- The ground --------------------------------------------------------------

-- A stretch of solid ground from screen x0 to x1: a grass lip over soil.
function look.drawGround(s, x0, x1, scroll)
  x0, x1 = snap(x0), snap(x1)
  if x1 <= x0 then return end
  local deep = H + 40 - GROUND_Y
  color(mix(s.ground, INK, 0.3))
  tiled("dirt", 0, 0, 96, 96, x0, GROUND_Y, x1 - x0, deep, snap(-(scroll % 48)) - 48, GROUND_Y + 6)
  -- Deeper is darker, in flat bands.
  color(INK, 0.13)
  for i = 0, 3 do
    local y = GROUND_Y + 48 + 42 * i
    g.rectangle("fill", x0, y, x1 - x0, H + 40 - y)
  end
  -- The grass: the top rows of the grass tile, an ink line over it, a dark
  -- fringe and drips under it, blades poking up.
  color(s.ground)
  tiled("grass", 0, 0, 48, 21, x0, GROUND_Y - 3, x1 - x0, 10.5, snap(-(scroll % 24)) - 24, GROUND_Y - 3)
  local lit = function(c) return mix(c, s.ground, s.dim * 0.8) end
  local dark, light = lit(LEAF_DARK), lit(LEAF_LIGHT)
  color(INK)
  g.rectangle("fill", x0, GROUND_Y - 4.5, x1 - x0, PX)
  color(dark)
  g.rectangle("fill", x0, GROUND_Y + 7.5, x1 - x0, PX)
  local step = 9
  local i0, i1 = floor((scroll + x0) / step), math.ceil((scroll + x1) / step) - 1
  for i = i0, i1 do
    local x = i * step - scroll
    if x >= x0 and x + 4.5 <= x1 then
      local r = hash(i, 34)
      if r < 0.5 then
        color(dark)
        pixel(x, GROUND_Y + 9, 3, 1)
        pixel(x + PX, GROUND_Y + 10.5, 1, 1)
      elseif r > 0.7 then
        -- A blade of grass.
        color(INK)
        pixel(x + PX, GROUND_Y - 7.5, 1, 2)
        color(light)
        pixel(x + PX, GROUND_Y - 6, 1, 1)
        if r > 0.94 then
          -- A flower.
          color(lit(if hash(i, 35) > 0.5 then RED else GOLD))
          pixel(x + 3, GROUND_Y - 9, 1, 1)
        end
      end
    end
  end
  -- Ink edges where a pit cuts the ground.
  color(INK)
  g.rectangle("fill", x0, GROUND_Y - 4.5, PX, deep + 4.5)
  g.rectangle("fill", x1 - PX, GROUND_Y - 4.5, PX, deep + 4.5)
end

-- A pit's inside (drawn before the hero, so a falling hero is seen in it).
function look.drawPit(s, x0, x1)
  x0, x1 = snap(x0), snap(x1)
  color(mix(s.ground, INK, 0.72))
  tiled("dirt", 0, 0, 96, 96, x0, GROUND_Y, x1 - x0, H + 40 - GROUND_Y, x0, GROUND_Y)
  color(INK, 0.3)
  for i = 0, 4 do
    local y = GROUND_Y + 24 + 30 * i
    g.rectangle("fill", x0, y, x1 - x0, H + 40 - y)
  end
  -- Water glinting far below.
  color(SKY_DEEP, 0.55)
  g.rectangle("fill", x0 + 3, H - 33, x1 - x0 - 6, 3)
  color(SKY_PALE, 0.7)
  g.rectangle("fill", x0 + 9, H - 33, 6, 1.5)
end

-- ---- The hero ------------------------------------------------------------

-- Where the hero turns and squashes: the middle of the bun, above its feet.
local HERO_MID = 30

-- The hero: a bread bun in red sneakers, 4 run frames (hero_run) and jump /
-- fall / tuck / bonked (hero_air), 66 × 66 units with the feet at the
-- bottom, around a 24 × 40 hitbox. p: x, y (feet centre on screen), sx, sy
-- (squash and stretch), rot, run (leg cycle in strides), grounded, rising,
-- face ("run", "jump", "spin", "worried", "phew", "happy", "dizzy",
-- "scared"), t.
function look.drawHero(p)
  local face = p.face
  local name, frame = "hero_air", 1
  if face == "spin" then
    frame = 2
  elseif face == "dizzy" then
    frame = 3
  elseif p.grounded then
    -- (Jammed in a pit, scared: the run cycle upside down is legs kicking.)
    name, frame = "hero_run", floor((p.run * 4) % 4)
  elseif face == "scared" then
    frame = 1
  elseif p.rising then
    frame = 0
  end
  g.push()
  g.translate(snap(p.x), snap(p.y - HERO_MID * p.sy))
  g.rotate(p.rot)
  g.scale(p.sx, p.sy)
  color(WHITE)
  local a = img[name]
  quad:setViewport(frame * 132, 0, 132, 132, a.w, a.h)
  g.draw(a.image, quad, -33, HERO_MID - 64.5, 0, 0.5, 0.5)
  if face == "phew" or face == "worried" then
    -- A sweat drop by the brow.
    local x, y = 18, -24 + floor(sin(p.t * 6) * 1.5 + 0.5)
    color(SKY_PALE)
    pixel(x, y, 1, 1)
    color(SKY_DEEP)
    pixel(x - PX * 0.5, y + PX, 2, 2)
    color(INK)
    pixel(x - PX * 0.5, y + PX * 3, 2, 1)
  elseif face == "dizzy" then
    -- Stars circling the head.
    color(GOLD)
    for i = 0, 2 do
      local a2 = p.t * 4 + i * TAU / 3
      local x, y = cos(a2) * 22, -34 + sin(a2) * 6
      pixel(x - PX, y, 3, 1)
      pixel(x, y - PX, 1, 3)
    end
  end
  g.pop()
end

-- The hero's shadow on the ground, shrinking as it rises: a stepped pixel oval.
function look.drawShadow(x, h)
  local k = math.clamp(1 - h / 260, 0.25, 1)
  local w = floor(12 * k + 0.5)
  color(INK, 0.3 * k)
  pixel(x - w * PX * 0.5, GROUND_Y - 4.5, w, 1)
  pixel(x - (w - 4) * PX * 0.5, GROUND_Y - 6, w - 4, 1)
end

-- ---- Obstacles and coins -------------------------------------------------

-- The crate's slices (art pixels; first, middle, last as pairs): frame and
-- corners kept, the planks repeated.
local CRATE_X = { 0, 5, 5, 19, 19, 24 }
local CRATE_Y = { 0, 5, 10, 15, 18, 24 }
-- The log: its cut end, a stretch of bark repeated, the far end.
local LOG_X = { 5, 13, 13, 38, 40, 59 }
local LOG_Y = { 1, 19, 0, 0, 19, 19 }

local function crate(x, y, w, h)
  -- The sprite has one clear pixel all round: the wood fills the hitbox.
  sliced("crate", CRATE_X, CRATE_Y, x - PX, y - PX, w + 2 * PX, h + 2 * PX)
end

-- An obstacle at screen x sx (its left edge), its bottom `bot` px above the ground.
function look.drawObstacle(s, kind, sx, w, h, bot, t)
  local y = GROUND_Y - bot - h -- its top on screen
  local cx, cy = sx + w * 0.5, y + h * 0.5
  color(s.prop)
  if kind == "crate" then
    crate(sx, y, w, h)
  elseif kind == "stack" then
    local n = if h > 100 then 3 else 2
    local ch = h / n
    for i = 0, n - 1 do
      crate(sx + (if i % 2 == 1 then 3 else 0), GROUND_Y - ch * (i + 1), w, ch)
    end
  elseif kind == "log" then
    sliced("log", LOG_X, LOG_Y, sx, GROUND_Y - bot - 27, w, 27)
  elseif kind == "gull" then
    -- Birds and the bee: frames bottom-anchored in their cells; the offsets
    -- put the body (not the raised wings) on the hitbox.
    cell("gull", floor(t * 9), 4, cx - 39, cy - 30)
  elseif kind == "skimmer" then
    -- Speed lines behind it.
    color(WHITE, 0.6)
    for i = 0, 2 do
      pixel(cx + 30 + i * 3, cy - 6 + i * 6, 6 + i * 2, 1)
    end
    color(s.prop)
    cell("skimmer", floor(t * 16), 4, cx - 39, cy - 37.5)
  elseif kind == "bee" then
    cell("bee", floor(t * 24), 2, cx - 24, cy - 28.5)
  end
end

-- A bee's up-and-down track (a dotted line), so its rhythm reads ahead of time.
function look.drawBeeTrack(cx, top, bot)
  color(WHITE, 0.56)
  local y = top
  while y < bot do
    pixel(cx - 0.75, y, 1, 1)
    y += 9
  end
end

-- A coin at (x, y) on screen; `spin` (radians) turns it.
function look.drawCoin(x, y, spin)
  cell("coin", floor((spin / TAU * 6) % 6), 6, x - 12, y - 12)
end

-- A warning at the right edge for something flying in from off screen.
function look.drawIncoming(y, t)
  local pulse = 1 + 0.12 * sin(t * 14)
  g.push()
  g.translate(W - 22, snap(y))
  g.scale(pulse, pulse)
  color(INK)
  g.polygon("fill", 10, 0, -2, -12, -2, 12)
  g.circle("fill", -8, 0, 13)
  color(RED)
  g.circle("fill", -8, 0, 11)
  color(WHITE)
  local f = fonts.mark
  g.setFont(f)
  g.print("!", -8 - f:getWidth("!") / 2, -f:getHeight() / 2)
  g.pop()
end

-- ---- The world ------------------------------------------------------------------

-- The whole world in front of the backdrop: pits, coins, obstacles, the
-- hero (pose: look.drawHero's), the ground, dust and speed lines.
function look.drawWorld(run, s, pose, time, puffs, faster_t)
  local t, dist = run.t, run.dist
  local hx = sim.HERO_X - dist
  -- Pits: their insides first (a falling hero shows in them), ground last.
  local pits = {}
  for _, o in run.obstacles do
    if o.kind == "pit" then
      local x0 = hx + o.x
      if x0 < W + 10 and x0 + o.w > -10 then
        pits[#pits + 1] = x0
        pits[#pits + 1] = x0 + o.w
        look.drawPit(s, x0, x0 + o.w)
      end
    end
  end
  color(WHITE)
  for _, c in run.coins do
    local x = hx + c.x
    if not c.taken and x > -20 and x < W + 20 then
      look.drawCoin(x, GROUND_Y - c.y, time * 5 + c.x * 0.02)
    end
  end
  for _, o in run.obstacles do
    if o.kind ~= "pit" then
      local x = hx + left(o, t)
      if x <= W + 20 and x + o.w >= -60 then
        if o.kind == "bee" then
          look.drawBeeTrack(x + o.w * 0.5, GROUND_Y - o.y - o.bob - o.h * 0.5, GROUND_Y - o.y - o.h * 0.5)
        end
        look.drawObstacle(s, o.kind, x, o.w, o.h, bottom(o, t), time)
      end
    end
  end
  local overPit = false
  for i = 1, #pits, 2 do
    if pose.x > pits[i] + 6 and pose.x < pits[i + 1] - 6 then overPit = true end
  end
  if not overPit and pose.y <= GROUND_Y + 1 then
    look.drawShadow(pose.x, GROUND_Y - pose.y)
  end
  look.drawHero(pose)
  -- The ground, in stretches between the pits (they come in order).
  local x = -40
  for i = 1, #pits, 2 do
    look.drawGround(s, x, pits[i], dist)
    x = max(x, pits[i + 1])
  end
  look.drawGround(s, x, W + 40, dist)
  -- Dust: chunky pixel squares, shrinking.
  for _, p in puffs do
    local k = p.age / p.life
    local sz = max(floor(p.r * (1 - 0.6 * k) / PX + 0.5), 1) * PX
    color(CREAM, 0.85 * (1 - k))
    g.rectangle("fill", snap(p.x - sz * 0.5), snap(p.y - sz * 0.5), sz, sz)
  end
  -- Speed lines after a speed-up.
  if faster_t < 0.9 then
    color(WHITE, 0.55 * (1 - faster_t / 0.9))
    for i = 0, 8 do
      local y = 110 + (i * 53) % 290
      local lx = W + 60 - ((faster_t * 1100 + i * 131) % (W + 160))
      g.rectangle("fill", floor(lx + 0.5), y, 48, 3)
    end
  end
end

-- ---- Words on screen --------------------------------------------------------------

-- Big white words with an ink outline and a shadow, centred on (x, y).
function look.big(text, x, y)
  local f = fonts.hint
  g.setFont(f)
  local x0, y0 = snap(x - f:getWidth(text) / 2), snap(y - f:getHeight() / 2)
  color(INK, 0.45)
  g.print(text, x0, y0 + 3)
  color(INK)
  local o = 2.5
  g.print(text, x0 - o, y0)
  g.print(text, x0 + o, y0)
  g.print(text, x0, y0 - o)
  g.print(text, x0, y0 + o)
  g.print(text, x0 - o * 0.7, y0 - o * 0.7)
  g.print(text, x0 + o * 0.7, y0 - o * 0.7)
  g.print(text, x0 - o * 0.7, y0 + o * 0.7)
  g.print(text, x0 + o * 0.7, y0 + o * 0.7)
  color(WHITE)
  g.print(text, x0, y0)
end

-- One of a ribbon's tails, folded behind its end (side -1 left, 1 right),
-- notched; d insets it (for the fill inside the outline).
local function tail(side, w, h, d)
  local inner, outer, notch = side * (w / 2 - 10 + d), side * (w / 2 + 24 - d), side * (w / 2 + 14 - d)
  local top, mid, bot = -h / 2 + 8 + d, 8, h / 2 + 8 - d
  g.polygon("fill", inner, top, outer, top, notch, mid, inner, mid)
  g.polygon("fill", inner, mid, notch, mid, outer, bot, inner, bot)
end

-- A banner ribbon centred on (0, 0) (translate and scale it first), alpha a.
function look.ribbon(text, a)
  local f = fonts.ribbon
  local h = 40
  local w = f:getWidth(text) + 44
  g.rotate(-0.04)
  color(INK, a)
  tail(-1, w, h, 0)
  tail(1, w, h, 0)
  color(0xa3202c, a)
  tail(-1, w, h, 3)
  tail(1, w, h, 3)
  -- The band.
  color(INK, a)
  g.rectangle("fill", -w / 2 - 3, -h / 2 - 3, w + 6, h + 6, 4)
  color(RED, a)
  g.rectangle("fill", -w / 2, -h / 2, w, h, 3)
  color(0xff8f8a, a * 0.6)
  g.rectangle("fill", -w / 2 + 3, -h / 2 + 3, w - 6, 3)
  g.setFont(f)
  local tx, ty = snap(-f:getWidth(text) / 2), snap(-f:getHeight() / 2)
  color(INK, a)
  g.print(text, tx + 2, ty + 2)
  color(WHITE, a)
  g.print(text, tx, ty)
end

-- The coin count in a pill, its right edge at x, middle at y, scaled k.
function look.coinPill(count, x, y, k)
  local f = fonts.pill
  local s = tostring(count)
  local h = 32
  local w = f:getWidth(s) + 44
  g.push()
  g.translate(x - w / 2, y)
  g.scale(k, k)
  color(INK, 0.55)
  g.rectangle("fill", -w / 2, -h / 2, w, h, h / 2)
  color(WHITE)
  cell("coin", 0, 6, -w / 2 + 5, -12)
  g.setFont(f)
  local tx, ty = snap(-w / 2 + 33), snap(-f:getHeight() / 2)
  color(INK)
  g.print(s, tx, ty + 2)
  color(GOLD)
  g.print(s, tx, ty)
  g.pop()
end

return look
