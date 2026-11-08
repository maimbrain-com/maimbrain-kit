-- Drawing the world: a cut-paper meadow, the plate on its stick, snacks
-- with faces, the paper bird, the landing ghost, wind. The pictures are
-- the kit's generated art (images/sprites.png, bird.png, faces.png,
-- paper.png; their rects in look.lua); motion, shadows, the sky's colours
-- and the effects are code. Reads the sim, changes nothing.

local look = require("look")
local sim = require("sim")

local draw = {}

local g = love.graphics
local abs, min, max, sin, cos, floor = math.abs, math.min, math.max, math.sin, math.cos, math.floor
local W, PLATE_TOP, PLATE_H, PLATE_W, GROUND_Y = sim.W, sim.PLATE_TOP, sim.PLATE_H, sim.PLATE_W, sim.GROUND_Y
local KINDS, ART, INK, WHITE = sim.KINDS, look.ART, look.INK, look.WHITE

-- Where paper shadows fall (down and a little right, like the sun at noon).
local SHADOW_X, SHADOW_Y = 1.6, 3.2

local img = {}
local quad
local fonts = {}

function draw.load()
  for _, name in { "sprites", "bird", "faces", "paper", "fade" } do
    local i = g.newImage(name)
    img[name] = { image = i, w = i:getWidth(), h = i:getHeight() }
  end
  quad = g.newQuad(0, 0, 1, 1, 1, 1)
end

-- ---- Helpers ---------------------------------------------------------------------

local function rgb(c)
  return floor(c / 65536) % 256 / 255, floor(c / 256) % 256 / 255, c % 256 / 255
end

function draw.color(c, a)
  local r, gg, b = rgb(c)
  g.setColor(r, gg, b, a or 1)
end
local color = draw.color

function draw.mix(a, b, t)
  local ar, ag, ab = rgb(a)
  local br, bg, bb = rgb(b)
  local function m(x, y) return floor((x + (y - x) * t) * 255 + 0.5) end
  return m(ar, br) * 65536 + m(ag, bg) * 256 + m(ab, bb)
end

-- Pixels `src` ({ x, y, w, h }) of an image, stretched over (x, y, w, h).
local function blit(name, src, x, y, w, h)
  local a = img[name]
  quad:setViewport(src[1], src[2], src[3], src[4], a.w, a.h)
  g.draw(a.image, quad, x, y, 0, w / src[3], h / src[4])
end

-- A piece of the atlas with a soft two-step paper drop shadow toward (ox, oy).
local function paper(src, x, y, w, h, tint, a, ox, oy)
  for _, s in { { 0.55, 0.16 }, { 1.0, 0.11 } } do
    color(INK, s[2] * a)
    blit("sprites", src, x + ox * s[1], y + oy * s[1], w, h)
  end
  color(tint, a)
  blit("sprites", src, x, y, w, h)
end

local function rotated(x, y, a)
  local c, s = cos(a), sin(a)
  return x * c - y * s, x * s + y * c
end

-- A font of the game's (look.FONT) at a size, made once.
function draw.font(size)
  local f = fonts[size]
  if not f then
    f = g.newFont(look.FONT, size)
    fonts[size] = f
  end
  return f
end

-- Words with a plum paper edge and a soft shadow, centred on (x, y).
function draw.words(text, x, y, size, c, a)
  a = a or 1
  local f = draw.font(size)
  g.setFont(f)
  local x0, y0 = x - f:getWidth(text) / 2, y - f:getHeight() / 2
  color(INK, 0.35 * a)
  g.print(text, x0, y0 + size * 0.12)
  color(INK, a)
  local o = max(size * 0.08, 1.5)
  for i = 0, 7 do
    local an = i * math.pi / 4
    g.print(text, x0 + o * cos(an), y0 + o * sin(an))
  end
  color(c, a)
  g.print(text, x0, y0)
  return f:getWidth(text)
end

-- A colour fading in from the screen's four edges, `d` deep (screen space).
function draw.edges(c, a, d)
  local f = img.fade
  color(c, a)
  g.draw(f.image, 0, 0, 0, W / f.w, d / f.h)
  g.draw(f.image, 0, 640, 0, W / f.w, -d / f.h)
  g.draw(f.image, 0, 640, -math.pi / 2, 640 / f.w, d / f.h)
  g.draw(f.image, W, 0, math.pi / 2, 640 / f.w, d / f.h)
end

-- ---- The sky ---------------------------------------------------------------------------

local function skyColors(height)
  local s = look.SKY
  for i = 1, #s - 1 do
    if height <= s[i + 1][1] then
      local t = (height - s[i][1]) / (s[i + 1][1] - s[i][1])
      return draw.mix(s[i][2], s[i + 1][2], t), draw.mix(s[i][3], s[i + 1][3], t)
    end
  end
  return s[#s][2], s[#s][3]
end

-- The sky behind everything (screen space): the colour of the hour, the
-- sun, paper clouds, stars at night, all over a sheet of paper.
function draw.sky(s, time)
  local top, bottom = skyColors(s.height)
  color(bottom)
  g.rectangle("fill", 0, 0, W, 640)
  color(top)
  g.draw(img.fade.image, 0, 0, 0, W / img.fade.w, 640 / img.fade.h)
  local night = math.clamp((s.height - 1100) / 500, 0, 1)
  if night > 0 then
    for i = 0, 35 do
      local x = (i * 97) % 360
      local y = ((i * 53) % 600 - s.camY * 0.04) % 640
      local tw = 0.6 + 0.4 * sin(time * 2 + i)
      color(look.STAR, night * tw)
      g.circle("fill", x, y, 1 + (i % 3) * 0.5)
    end
  end
  local day = 1 - math.clamp((s.height - 900) / 600, 0, 1)
  if day > 0 then
    -- The sun sinks a little and turns slowly.
    g.push()
    g.translate(296, 84 - s.camY * 0.06 + (1 - day) * 60)
    g.rotate(time * 0.08)
    color(WHITE, day)
    blit("sprites", ART.sun, -38, -38, 76, 76)
    g.pop()
  end
  local a = 0.35 + 0.65 * day
  for i = 0, 3 do
    local src, w = ART.cloud1, 84 + i * 6
    if i % 2 == 1 then src, w = ART.cloud2, 112 - i * 4 end
    local h = w * src[4] / src[3]
    local x = ((i * 131 + time * (5 + i * 2)) % 560) - 140
    local y = ((60 + i * 150 - s.camY * 0.25) % 760) - 70
    paper(src, x, y, w, h, WHITE, a, SHADOW_X, SHADOW_Y)
  end
  -- Paper grain over the whole sky (multiplied, so it only darkens the fibres).
  g.setBlendMode("multiply")
  color(WHITE)
  local t = 112
  local oy = (-s.camY * 0.25) % t
  for j = -1, 5 do
    for i = 0, 3 do
      blit("paper", { 0, 0, 128, 128 }, i * t, j * t + oy, t, t)
    end
  end
  g.setBlendMode("alpha")
end

-- ---- The world --------------------------------------------------------------------------

local function ground(s)
  -- Far hills scroll a little slower than the world (parallax), with a
  -- field below them so no sky shows through as they rise.
  local hy = 474 + s.camY * 0.3
  local hw = 392
  local hh = hw * ART.hills[4] / ART.hills[3]
  color(look.FIELD)
  g.rectangle("fill", -20, hy + hh - 4, W + 40, 400)
  color(WHITE)
  blit("sprites", ART.hills, (W - hw) / 2, hy, hw, hh)
  -- The near meadow and the blanket the stick stands on (fallen snacks land on it).
  color(look.FIELD_NEAR)
  g.rectangle("fill", -20, GROUND_Y + 30, W + 40, 400)
  local mw = 392
  local mh = mw * ART.meadow[4] / ART.meadow[3]
  color(WHITE)
  blit("sprites", ART.meadow, (W - mw) / 2, GROUND_Y - 14, mw, mh)
  local bw = 236
  local bh = bw * ART.blanket[4] / ART.blanket[3]
  paper(ART.blanket, W / 2 - bw / 2, GROUND_Y - 30, bw, bh, WHITE, 1, SHADOW_X, SHADOW_Y)
end

local function poleAndPlate(s)
  local cx = W / 2
  local poleTop = PLATE_TOP + PLATE_H
  paper(ART.pole, cx - 7, poleTop - 4, 14, GROUND_Y - poleTop + 10, WHITE, 1, SHADOW_X, SHADOW_Y)
  local x, y = s.plate:getPosition()
  local angle = s.plate:getAngle()
  g.push()
  g.translate(x, y)
  g.rotate(angle)
  -- The plate's top edge sits on the collider's top: snacks rest on it.
  local ox, oy = rotated(SHADOW_X, SHADOW_Y, -angle)
  paper(ART.plate, -PLATE_W / 2 - 3, -PLATE_H / 2 - 1, PLATE_W + 6, PLATE_H + 4, WHITE, 1, ox, oy)
  g.pop()
end

-- The face a snack makes right now: "excited" (on the bird), "scared" (in
-- the air), "happy", "worried" (the tower sways or it was hit), "sleepy"
-- (settled deep in the tower), "dizzy" (fell off).
function draw.mood(p, wobble, ending)
  if p.state == "lost" then return "dizzy" end
  -- The plate tipped: whoever's still on it is not happy about it.
  if ending then return "worried" end
  if p.state == "falling" and (p.fear > 0.2 or p.age < 0.6) then return "scared" end
  if p.fear > 0.4 then return "worried" end
  if p.state == "landed" and p.solid and wobble < 0.3 then return "sleepy" end
  return "happy"
end

-- Which cell of faces.png shows a mood (from 0).
local function faceFrame(mood, fear, blink)
  if mood == "happy" then return if blink then 1 else 0 end
  if mood == "excited" then return 2 end
  if mood == "scared" then return 3 end
  if mood == "worried" then return if fear > 0.6 then 5 else 4 end
  if mood == "sleepy" then return 6 end
  return 7
end

-- A snack drawn centred at (0, 0) in its own frame, its picture stretched
-- exactly over its collider, with a paper shadow toward (ox, oy). `a` fades it.
function draw.snack(kind, variant, a, ox, oy)
  local s = KINDS[kind]
  local lk = look.SNACKS[kind].looks[variant]
  paper(lk[1], -s.w / 2, -s.h / 2, s.w, s.h, lk[2], a, ox, oy)
end

-- A face in the snack's frame. `t` animates blinks and the worried shiver.
function draw.face(kind, mood, fear, t)
  local s = KINDS[kind]
  -- Faces sit on the visual middle (a wedge's is low, where it's wide).
  -- `tall`: how much of the snack's height the face may take (a baguette's
  -- face spills a little past its thin crust, or it would be too small to read).
  local cx, cy, wide, tall = 0, 0, 1, 0.8
  if kind == "wedge" then
    cy, wide = 5, 0.62
  elseif kind == "ball" then
    cy = 3
  elseif kind == "plank" then
    tall = 1.1
  end
  local k = min(s.w * wide * 0.68 / 48, s.h * tall / 32, 0.85)
  local fw, fh = 48 * k, 32 * k
  local shake = if mood == "worried" then sin(t * 40) * fear * 0.8 else 0
  local blink = (mood == "happy" or mood == "excited") and (t * 0.31) % 1 < 0.04
  local f = faceFrame(mood, fear, blink)
  -- Scared faces swell a little with fear.
  if mood == "scared" then
    fw *= 1 + fear * 0.12
    fh *= 1 + fear * 0.12
  end
  color(WHITE)
  local c = look.FACE_CELL
  blit("faces", { f * c[1], 0, c[1], c[2] }, cx - fw / 2 + shake, cy - fh / 2, fw, fh)
end

local function piece(s, p, time)
  local mood = draw.mood(p, s.wobble, s.ending ~= nil)
  local x, y = p.body:getPosition()
  local angle = p.body:getAngle()
  -- The shadow falls the same way on screen however the snack is turned.
  local ox, oy = rotated(SHADOW_X, SHADOW_Y, -angle)
  g.push()
  g.translate(x, y)
  g.rotate(angle)
  draw.snack(p.kind, p.variant, if p.state == "lost" then 0.8 else 1, ox, oy)
  draw.face(p.kind, mood, p.fear, time + p.id * 0.37)
  g.pop()
end

-- Where the carried snack would land: a dotted path and a ghost outline.
local function landingGhost(s, pred, time)
  local held = s.held
  if not held or not pred or held.age < 0.25 then return end
  local a = 0.75 + 0.2 * sin(time * 6)
  local col = if pred.hits then look.GHOST else look.DANGER
  local arc = pred.arc
  local n = #arc / 2
  for i = 2, n - 1 do
    local x, y = arc[i * 2 - 1], arc[i * 2]
    color(INK, 0.22)
    g.circle("fill", x, y, 3.4)
    color(col, a)
    g.circle("fill", x, y, 2.2)
  end
  -- A shadow of the snack where it would land, with a bright edge.
  local k = KINDS[held.kind]
  g.push()
  g.translate(pred.lx, pred.ly)
  g.setLineWidth(2.5)
  if held.kind == "ball" then
    color(INK, 0.22)
    g.circle("fill", 0, 0, k.w / 2)
    color(col, a)
    g.circle("line", 0, 0, k.w / 2)
  elseif held.kind == "wedge" then
    local pts = sim.wedgePoints()
    color(INK, 0.22)
    g.polygon("fill", pts)
    color(col, a)
    g.polygon("line", pts)
  else
    local r = if held.kind == "box" or held.kind == "cube" then 6 else max(k.r, k.h / 2)
    r = min(r, k.h / 2)
    color(INK, 0.22)
    g.rectangle("fill", -k.w / 2, -k.h / 2, k.w, k.h, r)
    color(col, a)
    g.rectangle("line", -k.w / 2, -k.h / 2, k.w, k.h, r)
  end
  g.pop()
end

-- The paper bird, flapping, with the snack in its feet.
local function bird(s, time)
  -- After the last heart it flies off, up and away.
  local away = if s.ending then s.ending * s.ending * 260 else 0
  local bx, by = s:bird()
  bx, by = bx + away * 0.6, by - away
  local vx = s:birdVx()
  local dir = if vx < -1 then -1 else 1
  local bob = sin(time * 4) * 3
  local x, y = bx, by + bob
  local h = s.held
  if h then
    local hx, hy = s:heldPos()
    -- It swoops in with each new snack.
    local k = min(h.age / 0.25, 1)
    hy = hy + bob - (1 - k) * 30
    g.push()
    g.translate(hx, hy)
    local sway = sin(time * 3) * 0.04 - vx * 0.0006
    g.rotate(sway)
    g.scale(max(k, 0.01), max(k, 0.01))
    local ox, oy = rotated(SHADOW_X, SHADOW_Y, -sway)
    draw.snack(h.kind, h.variant, 1, ox, oy)
    draw.face(h.kind, "excited", 0, time)
    g.pop()
  end
  -- The sheet's feet are at the bottom of each cell: they grip the snack's top.
  local frame = floor(time * 11) % look.BIRD_FRAMES
  local cell = look.BIRD_CELL
  local src = { frame * cell[1], 0, cell[1], cell[2] }
  local w, hh = cell[1] / 2, cell[2] / 2
  g.push()
  g.translate(x, y)
  g.scale(dir, 1)
  for _, sh in { { 1.4, 0.12 }, { 2.6, 0.08 } } do
    color(INK, sh[2])
    blit("bird", src, -w / 2 + SHADOW_X * sh[1] * dir, 22 - hh + SHADOW_Y * sh[1], w, hh)
  end
  color(WHITE)
  blit("bird", src, -w / 2, 22 - hh, w, hh)
  g.pop()
end

-- A strip with round ends from (x1, y) to (x2, y), `r` thick.
local function strip(x1, x2, y, r)
  local x0 = min(x1, x2)
  g.rectangle("fill", x0 - r, y - r, abs(x2 - x1) + 2 * r, 2 * r, r)
end

-- Wind: paper streaks and leaves while a gust is announced or blowing;
-- tilt arrows at the plate.
local function wind(s, time)
  local tw = s.twist
  if tw.phase == "calm" then return end
  local k = if tw.phase == "warn" then 0.45 + 0.2 * sin(tw.t * 8) else 1
  local dir = tw.dir
  if tw.kind == "gust" then
    local strength = if k >= 1 then math.clamp(abs(s.wind) / 70, 0.4, 1) else k
    local span = W + 160
    for i = 0, 11 do
      local x = ((i * 71 + time * 460 * (0.8 + (i % 3) * 0.2)) % span) - 80
      if dir < 0 then x = W - x end
      local y = s.camY + 110 + (i * 43) % 420
      local len = 34 + (i % 4) * 14
      color(INK, 0.18 * strength)
      strip(x, x - dir * len, y + 2.5, 3)
      color(look.PAPER_WHITE, 0.85 * strength)
      strip(x, x - dir * len, y, 3)
    end
    for i = 0, 5 do
      local x = ((i * 97 + time * 300) % span) - 80
      if dir < 0 then x = W - x end
      local y = s.camY + 160 + (i * 83) % 360 + sin(time * 5 + i) * 12
      g.push()
      g.translate(x, y)
      g.rotate(time * 6 * dir + i)
      paper(ART.leaf, -6, -10, 12, 20, WHITE, strength, SHADOW_X, SHADOW_Y)
      g.pop()
    end
  else
    -- Arrows at the plate's ends (kept on screen when the plate is below it).
    local y = math.clamp(PLATE_TOP + PLATE_H / 2, s.camY + 140, s.camY + 600)
    local bounce = sin(time * 7) * 4
    for _, sx in { -1, 1 } do
      local x = W / 2 + sx * (PLATE_W / 2 + 22)
      -- A clockwise tilt (dir > 0) takes the right end down.
      local up = sx * dir < 0
      local y0, y1 = y - 14, y + 16 + bounce
      if up then y0, y1 = y + 14, y - 16 + bounce end
      local d = if y1 < y0 then -1 else 1
      for _, layer in { { INK, 0.3, 2 }, { look.PAPER_WHITE, 0.6 * k + 0.3, 0 } } do
        color(layer[1], layer[2])
        local off = layer[3]
        g.rectangle("fill", x - 5, min(y0, y1) + off - 5, 10, abs(y1 - y0) + 10, 5)
        g.polygon("fill", x - 9, y1 + off, x + 9, y1 + off, x, y1 + d * 11 + off)
      end
    end
  end
end

-- The world, in world coordinates (call between camera:attach and detach).
-- `pred` is the landing ghost's prediction (nil: no ghost).
function draw.world(s, time, pred)
  ground(s)
  for _, p in s.pieces do
    if p.state == "lost" then piece(s, p, time) end
  end
  poleAndPlate(s)
  for _, p in s.pieces do
    if p.state ~= "lost" then piece(s, p, time) end
  end
  landingGhost(s, pred, time)
  bird(s, time)
  wind(s, time)
end

return draw
