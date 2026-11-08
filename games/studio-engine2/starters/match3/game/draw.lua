-- Drawing: poppets (painted bodies, faces drawn in code, specials), the
-- painted window and its night, the board, the wake meter, blasts, the
-- swipe hint, banners. All in love.draw, read only: nothing here changes the
-- game.
--
-- The soft glows (the lamp, cheeks, shadows, halos) are two pictures,
-- images/glow.png and images/soft.png, a white disc that fades out, tinted
-- and stretched.

local look = require("look")
local sim = require("sim")

local draw = {}

local lg = love.graphics
local sin, cos, abs, min, max, floor, pi = math.sin, math.cos, math.abs, math.min, math.max, math.floor, math.pi
local TAU = 2 * pi
local CELL, BX, BY = look.CELL, look.BOARD_X, look.BOARD_Y
local COLS, ROWS = sim.COLS, sim.ROWS

-- 0xRRGGBB as { r, g, b } from 0 to 1.
local function rgb(h)
  return { floor(h / 65536) % 256 / 255, floor(h / 256) % 256 / 255, h % 256 / 255 }
end

local function mix(a, b, t)
  return { a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t }
end

local function lighten(c, t) return mix(c, { 1, 1, 1 }, t) end
local function darken(c, t) return mix(c, { 0, 0, 0 }, t) end

local function color(c, a)
  lg.setColor(c[1], c[2], c[3], a or 1)
end

local INK = rgb(look.INK)
local PENCIL = rgb(look.PENCIL)
local GOLD = rgb(look.GOLD)
local LAMP = rgb(look.LAMP)
local BLUSH = rgb(look.BLUSH)
local WHITE = { 1, 1, 1 }
local NIGHT = rgb(look.NIGHT_TINT)
local SHADOW = rgb(0x0d0a20)
local CREAM = rgb(0xfffaf0)
local TONGUE = rgb(0xf07a8e)
local BLUE_GLOW = rgb(0x9fd0ff)
local SPARK = rgb(0xfff3c0)
local HINT_GLOW = rgb(0xffe6b0)
local STAR_TW = rgb(0xfff6d0)
local THUMB = rgb(0xfff6ee)
local RAINBOW_GLOW = rgb(0xfff0d0)
local BOMB_GLOW = rgb(0xffe0a0)
local SUN_GLOW = rgb(0xfff2c8)
local PIECES, METER = {}, {}
for k, h in look.PIECES do PIECES[k] = rgb(h) end
for k, h in look.METER do METER[k] = rgb(h) end
draw.PIECES, draw.INK, draw.GOLD = PIECES, INK, GOLD

local img = {}
local paper -- the paper grain over the board: a SpriteBatch of tiles
local border -- the board's pencil line, two wobbly passes
local fonts = {}

-- A wobbly pencil line around a rounded rectangle (two light passes).
local function pencilPoints(x, y, w, h, r)
  local passes = {}
  for pass = 0, 1 do
    local pts = {}
    local i = 0
    local corners = { { x + w - r, y + r, -pi / 2 }, { x + w - r, y + h - r, 0 }, { x + r, y + h - r, pi / 2 }, { x + r, y + r, pi } }
    for _, c in corners do
      for k = 0, 8 do
        local a = c[3] + k / 8 * pi / 2
        pts[#pts + 1] = c[1] + r * cos(a) + 0.9 * sin(i * 1.71 + pass * 2)
        pts[#pts + 1] = c[2] + r * sin(a) + 0.9 * cos(i * 2.13 + pass * 1.3)
        i += 1
      end
    end
    pts[#pts + 1], pts[#pts + 2] = pts[1], pts[2]
    passes[pass + 1] = pts
  end
  return passes
end

function draw.load()
  for k, name in look.SPRITES do img[k] = lg.newImage(name) end
  img.rainbow = lg.newImage(look.RAINBOW)
  img.rock = lg.newImage(look.ROCK)
  img.bg = lg.newImage("bg")
  img.glow = lg.newImage("glow")
  img.soft = lg.newImage("soft")
  img.fade = lg.newImage("fade")
  img.paper = lg.newImage("paper")
  -- Paper grain over the wash: one 64-unit tile per 128-texel paper.
  local x0, y0 = BX - 2, BY - 2
  local w, h = CELL * COLS + 4, CELL * ROWS + 4
  paper = lg.newSpriteBatch(img.paper, 64)
  local y = y0
  while y < y0 + h do
    local th = min(64, y0 + h - y)
    local x = x0
    while x < x0 + w do
      local tw = min(64, x0 + w - x)
      paper:add(lg.newQuad(0, 0, tw * 2, th * 2, img.paper), x, y, 0, 0.5, 0.5)
      x += 64
    end
    y += 64
  end
  border = pencilPoints(BX - 6, BY - 6, CELL * COLS + 12, CELL * ROWS + 12, 20)
  fonts.hint = lg.newFont(look.FONT, 20)
  fonts.banner = lg.newFont(look.FONT, 30)
  fonts.z = lg.newFont(look.FONT, 20)
  fonts.pill = lg.newFont(look.FONT, 15)
end

function draw.cellCenter(c, r)
  return BX + (c + 0.5) * CELL, BY + (r + 0.5) * CELL
end
local cellCenter = draw.cellCenter

-- A soft round glow of radius r (glow.png fades all the way; soft.png is
-- solid to half way).
local function soft(x, y, rx, ry, c, a, which)
  local im = which or img.soft
  color(c, a)
  lg.draw(im, x, y, 0, rx / 32, ry / 32, 32, 32)
end

-- A filled ring.
local function ring(x, y, r, w, c, a)
  color(c, a)
  lg.setLineWidth(w)
  lg.circle("line", x, y, r)
end

-- A four-point sparkle.
local function sparkle(x, y, r, rot, c, a)
  local pts = {}
  for k = 0, 7 do
    local ang = rot + k * pi / 4
    local rr = (k % 2 == 0) and r or r * 0.32
    pts[#pts + 1] = x + rr * sin(ang)
    pts[#pts + 1] = y - rr * cos(ang)
  end
  color(c, a)
  lg.polygon("fill", pts)
end

-- A five-point star.
local function star(x, y, r, c, a)
  local pts = {}
  for k = 0, 9 do
    local ang = k * pi / 5
    local rr = (k % 2 == 0) and r or r * 0.48
    pts[#pts + 1] = x + rr * sin(ang)
    pts[#pts + 1] = y - rr * cos(ang)
  end
  color(c, a)
  lg.polygon("fill", pts)
end

-- An arc as the kit draws them: angles from straight up, clockwise.
local function arc(x, y, r, w, a0, a1, c, a)
  color(c, a)
  lg.setLineWidth(w)
  lg.arc("line", "open", x, y, r, a0 - pi / 2, a1 - pi / 2, 10)
end

-- Text centred on (x, y) with an ink outline.
local function outlined(s, font, x, y, c, a, ow)
  lg.setFont(font)
  local h = font:getHeight()
  local top = y - h / 2
  color(INK, a)
  for k = 0, 7 do
    local ang = k * pi / 4
    lg.printf(s, x - 200 + ow * cos(ang), top + ow * sin(ang), 400, "center")
  end
  color(c, a)
  lg.printf(s, x - 200, top, 400, "center")
end
draw.outlined = outlined

-- ---- Faces ------------------------------------------------------------------

-- A storybook face at (x, y), u units to one face unit: ink-oval eyes that
-- close from the top as the poppet gets sleepy, a lid line with a lash, rosy
-- cheeks, a small pencil mouth. mood is "happy", "excited" (picked up,
-- swapped, about to pop), "huh" (a swap that didn't work), "sleepy" (d: 0
-- awake … 1 nearly shut), "asleep" or "squeeze" (popping).
local function face(x, y, u, mood, d, lx, ly, blink, a)
  local function X(v) return x + v * u end
  local function Y(v) return y + v * u end
  lx, ly = lx * 0.05, ly * 0.04
  local ey = -0.04
  local big = mood == "excited" and 1.2 or 1
  local rx, ry = 0.095 * big, 0.13 * big
  -- Cheeks first, under everything.
  local blush = 0.42
  if mood == "asleep" then blush = 0.55 elseif mood == "sleepy" then blush = 0.4 + 0.2 * d end
  for s = -1, 1, 2 do
    soft(X(s * 0.5), Y(ey + 0.24), 0.2 * u, 0.2 * u, BLUSH, blush * a)
  end
  for s = -1, 1, 2 do
    local ex = s * 0.3
    if mood == "asleep" then
      arc(X(ex), Y(ey - 0.08), 0.12 * u, 0.055 * u, pi * 0.68, pi * 1.32, INK, a)
    elseif mood == "squeeze" then
      color(INK, a)
      lg.setLineWidth(0.065 * u)
      lg.line(X(ex - s * 0.12), Y(ey - 0.1), X(ex + s * 0.05), Y(ey), X(ex - s * 0.12), Y(ey + 0.1))
    elseif blink then
      arc(X(ex), Y(ey - 0.07), 0.11 * u, 0.05 * u, pi * 0.7, pi * 1.3, INK, a)
    else
      local dd = mood == "sleepy" and math.clamp(d, 0, 1) or 0
      -- The eye shuts from the top: what's left of it hangs under the lid.
      local open = 1 - 0.82 * dd
      local h = ry * open
      local cy = ey + ry - h
      color(INK, a)
      lg.ellipse("fill", X(ex + lx * open), Y(cy + ly * open), rx * u, h * u)
      if open > 0.45 then
        color(CREAM, a)
        lg.circle("fill", X(ex + lx - 0.03), Y(cy + ly - h * 0.45), 0.035 * big * u)
      end
      if dd > 0.05 then
        -- The lid: a heavy line across the top, drooping at the outer end, with a lash.
        local top = cy - h
        local droop = 0.05 * dd
        color(INK, a)
        lg.setLineWidth(0.06 * u)
        lg.line(X(ex - s * 0.13), Y(top + 0.005), X(ex + s * 0.14), Y(top + droop), X(ex + s * 0.2), Y(top + droop - 0.05))
      end
      if mood == "huh" then
        color(INK, a)
        lg.setLineWidth(0.05 * u)
        lg.line(X(ex - 0.13), Y(ey - 0.27 - s * 0.04), X(ex + 0.13), Y(ey - 0.27 + s * 0.04))
      end
    end
  end
  local my = ey + 0.3
  if mood == "happy" then
    arc(X(0), Y(my - 0.13), 0.14 * u, 0.055 * u, pi * 0.7, pi * 1.3, INK, a)
  elseif mood == "excited" or mood == "squeeze" then
    local h = mood == "squeeze" and 0.15 or 0.12
    color(INK, a)
    lg.rectangle("fill", X(-0.12), Y(my - 0.06), 0.24 * u, h * 2 * u, h * u)
    color(TONGUE, a)
    lg.rectangle("fill", X(-0.07), Y(my - 0.06 + h * 1.15), 0.14 * u, h * 0.7 * u, h * 0.35 * u)
  elseif mood == "huh" then
    color(INK, a)
    lg.setLineWidth(0.055 * u)
    lg.line(X(-0.08), Y(my), X(0.1), Y(my - 0.03))
  elseif mood == "sleepy" then
    if d > 0.55 then
      -- A yawn, slowly opening.
      local o = 0.05 + 0.06 * (d - 0.55) / 0.45
      color(INK, a)
      lg.ellipse("fill", X(0), Y(my), o * u, o * 1.25 * u)
    else
      arc(X(0), Y(my - 0.13 + d * 0.06), 0.13 * u, 0.05 * u, pi * (0.74 + d * 0.12), pi * (1.26 - d * 0.12), INK, a)
    end
  else -- asleep
    color(INK, a)
    lg.ellipse("fill", X(0), Y(my), 0.045 * u, 0.055 * u)
  end
end

-- ---- Poppets ----------------------------------------------------------------

-- A painted body: the picture, `r` from centre to edge, squashed by sx, sy.
local function body(im, x, y, r, sx, sy, a, rot)
  local s = look.SPRITE_SPAN * r / 128
  lg.setColor(1, 1, 1, a)
  lg.draw(im, x, y, rot or 0, s * sx, s * sy, 64, 64)
end

-- A poppet p at (x, y), r from centre to edge, squashed by (sx, sy).
-- mood and d as face's; lx, ly where it looks (-1…1); t the clock.
function draw.piece(p, x, y, r, sx, sy, mood, d, lx, ly, t, a)
  if a <= 0.01 or r <= 0.5 then return end
  local blink = ((t * 0.31 + p.id * 0.6180339) % 1) < 0.035 and mood ~= "asleep" and mood ~= "squeeze"
  local u = r * look.FACE_SCALE
  if p.kind == "poppet" then
    local k = p.color
    if p.special == "lineH" or p.special == "lineV" then
      local pulse = 0.5 + 0.5 * sin(t * 7)
      lg.setBlendMode("add")
      soft(x, y, 1.5 * r, 1.5 * r, BLUE_GLOW, (0.3 + 0.25 * pulse) * a)
      lg.setBlendMode("alpha")
    elseif p.special == "bomb" then
      local pulse = 0.5 + 0.5 * sin(t * 6)
      lg.setBlendMode("add")
      soft(x, y, (1.55 + 0.1 * pulse) * r, (1.55 + 0.1 * pulse) * r, LAMP, (0.4 + 0.25 * pulse) * a)
      lg.setBlendMode("alpha")
      ring(x, y, 1.24 * r, 0.1 * r, INK, 0.8 * a)
      ring(x, y, 1.16 * r, 0.06 * r, GOLD, a)
    end
    soft(x, y + 0.2 * r, 0.95 * r, 0.95 * r, SHADOW, 0.28 * a)
    body(img[k], x, y, r, sx, sy, a)
    if p.special == "lineH" or p.special == "lineV" then
      local h = p.special == "lineH"
      local push = 1.18 + 0.08 * abs(sin(t * 7))
      for s = -1, 1, 2 do
        local cx, cy = 0, s * push
        if h then cx, cy = s * push, 0 end
        local tri
        if h then
          tri = { cx + s * 0.34, cy, cx - s * 0.12, cy - 0.32, cx - s * 0.12, cy + 0.32 }
        else
          tri = { cx, cy + s * 0.34, cx - 0.32, cy - s * 0.12, cx + 0.32, cy - s * 0.12 }
        end
        local outer, inner = {}, {}
        for k2 = 1, 6, 2 do
          local px, py = tri[k2], tri[k2 + 1]
          outer[k2], outer[k2 + 1] = x + (cx + (px - cx) * 1.45) * r, y + (cy + (py - cy) * 1.45) * r
          inner[k2], inner[k2 + 1] = x + px * r, y + py * r
        end
        color(INK, a)
        lg.polygon("fill", outer)
        color(PENCIL, a)
        lg.polygon("fill", inner)
        -- A painted stripe across the body, the way it will blast.
        color(PENCIL, 0.85 * a)
        lg.setLineWidth(0.13 * r)
        if h then
          lg.line(x - 0.62 * r, y + s * 0.5 * r, x + 0.62 * r, y + s * 0.5 * r)
        else
          lg.line(x + s * 0.5 * r, y - 0.62 * r, x + s * 0.5 * r, y + 0.62 * r)
        end
      end
    elseif p.special == "bomb" then
      sparkle(x, y - 1.2 * r, (0.34 + 0.08 * sin(t * 13)) * r, t * 3, SPARK, a)
    end
    face(x, y + look.FACE_Y[k] * r * sy, u, mood, d, lx, ly, blink, a)
    if p.star then
      local tw = 1 + 0.12 * sin(t * 5 + p.id)
      star(x + 0.68 * r, y - 0.68 * r, 0.44 * tw * r, INK, a)
      star(x + 0.68 * r, y - 0.68 * r, 0.34 * tw * r, GOLD, a)
    end
  elseif p.kind == "rainbow" then
    soft(x, y + 0.2 * r, 0.95 * r, 0.95 * r, SHADOW, 0.28 * a)
    lg.setBlendMode("add")
    soft(x, y, 1.6 * r, 1.6 * r, RAINBOW_GLOW, (0.25 + 0.1 * sin(t * 4)) * a)
    lg.setBlendMode("alpha")
    body(img.rainbow, x, y, r, sx, sy, a, 0.08 * sin(t * 1.6 * 1.3))
    face(x, y + look.FACE_Y[9] * r * sy, u, mood, d, lx, ly, blink, a)
  else -- a rock: a grumpy face, beady eyes under flat, frowning brows
    soft(x, y + 0.2 * r, 0.95 * r, 0.95 * r, SHADOW, 0.28 * a)
    body(img.rock, x, y, r, sx, sy, a)
    local fy = y + look.FACE_Y[8] * r
    color(INK, a)
    lg.setLineWidth(0.13 * r)
    for s = -1, 1, 2 do
      lg.circle("fill", x + s * 0.3 * r, fy + 0.02 * r, 0.085 * r)
      lg.line(x + s * 0.14 * r, fy - 0.12 * r, x + s * 0.46 * r, fy - 0.2 * r)
    end
    arc(x, fy + 0.5 * r, 0.18 * r, 0.06 * r, -pi * 0.28, pi * 0.28, INK, a)
  end
end

-- ---- The room ---------------------------------------------------------------

-- The painted bedroom window. As the poppets get sleepy the lamp's warm glow
-- fades, the room sinks into blue night and stars come out.
function draw.sky(sleep, t)
  local tint = mix(WHITE, NIGHT, sleep)
  color(tint, 1)
  lg.draw(img.bg, 0, 0, 0, 360 / 540, 640 / 960)
  -- The bedside lamp, off to the lower left, glowing while they're awake.
  lg.setBlendMode("add")
  local flicker = 1 + 0.04 * sin(t * 1.7)
  soft(-20, 600, 330 * flicker, 330, LAMP, 0.2 * (1 - sleep), img.glow)
  soft(380, 80, 180, 180, SUN_GLOW, 0.08 * (1 - sleep), img.glow)
  lg.setBlendMode("alpha")
  if sleep > 0.05 then
    -- Extra stars in the window, twinkling (the moon is painted, top right).
    for i = 0, 13 do
      local x = 60 + (i * 97.31) % 170
      local y = 64 + (i * 53.7 + 11) % 100
      local tw = 0.5 + 0.5 * sin(t * 2 + i * 1.3)
      sparkle(x, y, 1.6 + 2.4 * tw, 0, STAR_TW, sleep * (0.3 + 0.55 * tw))
    end
  end
  -- A soft shade along the bottom, so the feed's title reads over the windowsill.
  color(INK, 0.8)
  lg.draw(img.fade, 0, 530, 0, 360 / 4, 110 / 64)
end

-- The board: an indigo wash on watercolour paper, paler cells, a pencil border.
function draw.board(sleep)
  local x, y, w, h = BX - 8, BY - 8, CELL * COLS + 16, CELL * ROWS + 16
  -- A soft shadow under it.
  for k = 1, 3 do
    color(SHADOW, 0.13)
    lg.rectangle("fill", x - 2 * k, y + 6 - k, w + 4 * k, h + 4 * k, 22 + 2 * k)
  end
  local wash = look.BOARD_WASH
  lg.setColor(wash[1], wash[2], wash[3], wash[4])
  lg.rectangle("fill", x, y, w, h, 22)
  local cw = look.CELL_WASH
  lg.setColor(cw[1], cw[2], cw[3], cw[4])
  for r = 0, ROWS - 1 do
    for c = (r % 2), COLS - 1, 2 do
      lg.rectangle("fill", BX + c * CELL + 2, BY + r * CELL + 2, CELL - 4, CELL - 4, 12)
    end
  end
  lg.setBlendMode("multiply")
  lg.setColor(1, 1, 1, 1)
  lg.draw(paper)
  lg.setBlendMode("add")
  lg.setColor(1, 1, 1, 0x12 / 255)
  lg.draw(paper)
  lg.setBlendMode("alpha")
  local a = 1 - sleep * 0.5
  color(PENCIL, 0.6 * a)
  lg.setLineWidth(1.4)
  lg.line(border[1])
  color(PENCIL, 0.35 * a)
  lg.setLineWidth(1.0)
  lg.line(border[2])
end

-- ---- The wake meter ---------------------------------------------------------

-- A painted capsule that drains, with a moon and a sun at its ends. `shown`
-- trails `energy` (the refill grows in); `flash` 0…1 after a refill.
function draw.meter(cx, cy, w, energy, shown, flash, t)
  local h = 18
  local x, y = cx - w / 2, cy - h / 2
  color(INK, 0.75)
  lg.rectangle("fill", x - 3, y - 3, w + 6, h + 6, (h + 6) / 2)
  color(PENCIL, 0.7)
  lg.setLineWidth(1.3)
  lg.rectangle("line", x - 3.5, y - 3.5, w + 7, h + 7, (h + 7) / 2)
  local c
  if energy > 0.5 then
    c = mix(METER[2], METER[1], (energy - 0.5) * 2)
  else
    c = mix(METER[3], METER[2], max(0, energy - 0.25) * 4)
  end
  local blink = 1
  if energy < sim.DROWSY then blink = 0.55 + 0.45 * sin(t * 9) end
  local gain = max(shown, energy)
  if gain > 0.005 then
    color(PENCIL, 0.3 + 0.5 * flash)
    lg.rectangle("fill", x, y, max(w * gain, h), h, h / 2)
  end
  local lo = min(energy, shown)
  if lo > 0.005 then
    local fw = max(w * lo, h)
    color(darken(c, 0.12), blink)
    lg.rectangle("fill", x, y, fw, h, h / 2)
    color(lighten(c, 0.2), blink)
    lg.rectangle("fill", x, y, fw, h * 0.62, h * 0.31)
    -- A watercolour bloom along the top edge.
    if fw > 16 then
      lg.setColor(1, 0.98, 0.94, 0.38 * blink)
      lg.setLineWidth(3)
      lg.line(x + 7, y + 5, x + fw - 7, y + 5)
    end
  end
  -- Moon at the empty end, sun at the full end.
  local mx = x - 17
  color(INK)
  lg.circle("fill", mx, cy, 12)
  lg.setColor(1, 0.945, 0.784, 1)
  lg.circle("fill", mx, cy, 9)
  color(INK)
  lg.circle("fill", mx + 4.5, cy - 3, 7.5)
  local sx = x + w + 17
  color(INK)
  lg.circle("fill", sx, cy, 13)
  color(GOLD)
  lg.setLineWidth(2.5)
  local spin = t * 0.8
  for i = 0, 7 do
    local an = spin + i * TAU / 8
    lg.line(sx + 6 * sin(an), cy - 6 * cos(an), sx + 10.5 * sin(an), cy - 10.5 * cos(an))
  end
  lg.circle("fill", sx, cy, 6)
end

-- ---- Blasts -----------------------------------------------------------------

-- A special going off, age seconds after its own start.
function draw.blast(b, age)
  if age < 0 then return end
  local x, y = cellCenter(sim.col(b.at), sim.row(b.at))
  local c = b.color and PIECES[b.color] or WHITE
  local k = math.clamp(age / 0.35, 0, 1)
  local left, right = BX - 6, BX + CELL * COLS + 6
  local top, bottom = BY - 6, BY + CELL * ROWS + 6
  local function beam(horiz, at)
    local w = 30 * (1 - k)
    lg.setBlendMode("add")
    color(c, 0.5 * (1 - k))
    lg.setLineWidth(max(0.5, w * 1.6))
    if horiz then lg.line(left, at, right, at) else lg.line(at, top, at, bottom) end
    color(PENCIL, 1 - k)
    lg.setLineWidth(max(0.5, w * 0.5))
    if horiz then lg.line(left, at, right, at) else lg.line(at, top, at, bottom) end
    lg.setBlendMode("alpha")
  end
  if b.kind == "row" then
    if k < 1 then beam(true, y) end
  elseif b.kind == "col" then
    if k < 1 then beam(false, x) end
  elseif b.kind == "cross" then
    if k < 1 then
      for d = -b.n, b.n do
        beam(true, y + d * CELL)
        beam(false, x + d * CELL)
      end
    end
  elseif b.kind == "area" then
    if k < 1 then
      local e = 1 - (1 - k) * (1 - k)
      local rr = (b.n + 0.8) * CELL * (0.3 + 0.7 * e)
      lg.setBlendMode("add")
      soft(x, y, rr * 1.3, rr * 1.3, BOMB_GLOW, 0.6 * (1 - k), img.glow)
      lg.setBlendMode("alpha")
      ring(x, y, rr, 10 * (1 - k) + 1, PENCIL, 1 - k)
    end
  elseif b.kind == "zap" then
    local kz = math.clamp(age / 0.45, 0, 1)
    if kz < 1 then
      local a = 1 - kz
      lg.setBlendMode("add")
      for i, cl in b.targets do
        local tx, ty = cellCenter(sim.col(cl), sim.row(cl))
        local mx = (x + tx) / 2 + 10 * sin(i * 2.1 + age * 40)
        local my = (y + ty) / 2 + 10 * cos(i * 1.3 + age * 37)
        color(c, 0.7 * a)
        lg.setLineWidth(5 * a + 1)
        lg.line(x, y, mx, my, tx, ty)
        lg.setColor(1, 1, 1, a)
        lg.setLineWidth(1.6)
        lg.line(x, y, mx, my, tx, ty)
      end
      soft(x, y, 48, 40, WHITE, 0.7 * a, img.glow)
      lg.setBlendMode("alpha")
    end
  elseif b.kind == "board" then
    local kb = math.clamp(age / 0.6, 0, 1)
    if kb < 1 then
      lg.setBlendMode("add")
      lg.setColor(1, 1, 1, 0.6 * (1 - kb))
      lg.rectangle("fill", left, top, right - left, bottom - top, 16)
      lg.setBlendMode("alpha")
      ring(x, y, 40 + 400 * kb, 14 * (1 - kb) + 1, PENCIL, 1 - kb)
    end
  end
end

-- ---- Hints, banners, z's ----------------------------------------------------

local function cubicInOut(k)
  if k < 0.5 then return 4 * k * k * k end
  local f = -2 * k + 2
  return 1 - f * f * f / 2
end

-- The swipe hint: both cells glow and a ghost thumb slides from a to b.
function draw.swipeHint(ax, ay, bx, by, t, strength)
  local pulse = 0.5 + 0.5 * sin(t * 5)
  lg.setBlendMode("add")
  soft(ax, ay, CELL * 0.7, CELL * 0.7, HINT_GLOW, (0.25 + 0.3 * pulse) * strength, img.glow)
  soft(bx, by, CELL * 0.7, CELL * 0.7, HINT_GLOW, (0.25 + 0.3 * pulse) * strength, img.glow)
  lg.setBlendMode("alpha")
  local k = (t * 0.9) % 1
  local mk = cubicInOut(math.clamp((k - 0.2) / 0.5, 0, 1))
  local alpha = strength
  if k < 0.1 then alpha *= k / 0.1 elseif k > 0.85 then alpha *= (1 - k) / 0.15 end
  local fx, fy = ax + (bx - ax) * mk, ay + (by - ay) * mk
  -- An arrow along the way.
  local dx, dy = (bx - ax) / CELL, (by - ay) / CELL
  local mx, my = (ax + bx) / 2, (ay + by) / 2
  local px, py = -dy, dx
  color(PENCIL, alpha * 0.9)
  lg.polygon("fill", mx + dx * 9, my + dy * 9, mx - dx * 5 + px * 9, my - dy * 5 + py * 9, mx - dx * 5 - px * 9, my - dy * 5 - py * 9)
  local press = (k >= 0.12 and k < 0.8) and 0.85 or 1
  color(SHADOW, 0.31 * alpha)
  lg.circle("fill", fx + 3, fy + 5, 17 * press)
  color(THUMB, 0.88 * alpha)
  lg.circle("fill", fx, fy, 17 * press)
  ring(fx, fy, 17 * press, 3, INK, 0.8 * alpha)
end

-- "SWIPE TO SWAP" above the board.
function draw.hintText(t)
  outlined(look.HINT, fonts.hint, 180, BY - 26 + sin(t * 4) * 3, WHITE, 1, 2)
end

-- A level's banner: a ribbon in the accent, scaled by k, faded by a.
local ACCENT = rgb(look.ACCENT)
function draw.banner(s, y, k, a)
  local font = fonts.banner
  lg.push()
  lg.translate(180, y)
  lg.rotate(-0.04)
  lg.scale(k, k)
  local w = font:getWidth(s) + 44
  local h = 46
  -- The tails, folded behind.
  local dark = darken(ACCENT, 0.3)
  color(dark, a)
  lg.polygon("fill", -w / 2 - 24, -h / 2 + 8, -w / 2 + 10, -h / 2 + 8, -w / 2 + 10, h / 2 + 8, -w / 2 - 24, h / 2 + 8, -w / 2 - 12, 8)
  lg.polygon("fill", w / 2 + 24, -h / 2 + 8, w / 2 - 10, -h / 2 + 8, w / 2 - 10, h / 2 + 8, w / 2 + 24, h / 2 + 8, w / 2 + 12, 8)
  color(SHADOW, 0.35 * a)
  lg.rectangle("fill", -w / 2, -h / 2 + 5, w, h, 10)
  color(INK, a)
  lg.rectangle("fill", -w / 2 - 2.5, -h / 2 - 2.5, w + 5, h + 5, 12)
  color(ACCENT, a)
  lg.rectangle("fill", -w / 2, -h / 2, w, h, 10)
  color(lighten(ACCENT, 0.25), a)
  lg.rectangle("fill", -w / 2 + 6, -h / 2 + 4, w - 12, 6, 3)
  outlined(s, font, 0, 1, WHITE, a, 2)
  lg.pop()
end

-- "z"s floating up off sleeping poppets.
function draw.zzz(t, alpha)
  for i = 0, 6 do
    local k = (t * 0.45 + i * 0.37) % 1
    local c = (i * 3 + 1) % COLS
    local r = (i * 5 + 2) % ROWS
    local x, y = cellCenter(c, r)
    local a = alpha * (k < 0.2 and k / 0.2 or 1 - (k - 0.2) / 0.8)
    local size = (14 + 12 * k) / 20
    lg.push()
    lg.translate(x + 12 + 14 * k + 5 * sin(k * 9), y - 14 - 50 * k)
    lg.scale(size, size)
    outlined("z", fonts.z, 0, 0, PENCIL, a, 2)
    lg.pop()
  end
end

-- The key and pad cursor: a pencil square on its cell, gold while it holds one.
function draw.cursor(c, r, held, t)
  local x, y = BX + c * CELL, BY + r * CELL
  local pulse = 0.5 + 0.5 * sin(t * 6)
  color(INK, 0.7)
  lg.setLineWidth(5)
  lg.rectangle("line", x + 1, y + 1, CELL - 2, CELL - 2, 12)
  color(held and GOLD or PENCIL, 0.7 + 0.3 * pulse)
  lg.setLineWidth(2.5)
  lg.rectangle("line", x + 1, y + 1, CELL - 2, CELL - 2, 12)
end

-- A small label in a dark pill (the level), centred on (x, y).
function draw.pill(s, x, y)
  local font = fonts.pill
  local w = font:getWidth(s) + 22
  color(INK, 0.55)
  lg.rectangle("fill", x - w / 2, y - 13, w, 26, 13)
  lg.setFont(font)
  lg.setColor(1, 1, 1, 1)
  lg.printf(s, x - 100, y - font:getHeight() / 2, 200, "center")
end

-- The glow under a picked-up poppet.
function draw.selectGlow(x, y, t)
  local pulse = 0.5 + 0.5 * sin(t * 8)
  lg.setBlendMode("add")
  soft(x, y, CELL * 0.66, CELL * 0.66, HINT_GLOW, 0.5 + 0.3 * pulse, img.glow)
  lg.setBlendMode("alpha")
end

return draw
