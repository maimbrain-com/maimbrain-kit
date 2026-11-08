-- Drawing, in love.draw: the generated art (images/sprites.png, an atlas
-- cut with quads, and images/backdrop.png) plus what code does better: the
-- floor grid rushing at you, stars and speed streaks, faces whose eyes
-- follow you, the exhaust, glows, the queen's tentacles, bullets, shots and
-- the HUD's own pieces (the combo, power-ups, the boss's bar, banners, the
-- drag hint). Reads the game, changes nothing.
--
-- Bullets, shots and glows are drawn straight each frame (no nodes): a
-- boss's spiral is a couple of hundred of them, made and gone every second.

local look = require("look")
local sim = require("sim")

local draw = {}

local lg = love.graphics
local sin, cos, abs, min, max, floor, sqrt, pi = math.sin, math.cos, math.abs, math.min, math.max, math.floor, math.sqrt, math.pi
local clamp, atan2 = math.clamp, math.atan2
local TAU = 2 * pi
local W, H = sim.W, sim.H
local S = look.SPRITES

local atlas, backdrop, glowImg, fadeImg
local quads = {}
local swoopQuads = {}
local fonts = {}
local stars, streaks = {}, {}

-- ---- Colour helpers ------------------------------------------------------------

local cache = {}
local function rgb(h)
  local c = cache[h]
  if not c then
    c = { floor(h / 65536) % 256 / 255, floor(h / 256) % 256 / 255, h % 256 / 255 }
    cache[h] = c
  end
  return c
end

local function color(h, a)
  local c = rgb(h)
  lg.setColor(c[1], c[2], c[3], a or 1)
end

-- Colour a → b by t, at alpha `a`.
local function mixColor(ha, hb, t, a)
  local ca, cb = rgb(ha), rgb(hb)
  lg.setColor(ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t, ca[3] + (cb[3] - ca[3]) * t, a or 1)
end

-- Colour h lightened toward white by t.
local function light(h, t, a)
  mixColor(h, 0xffffff, t, a)
end

local function add() lg.setBlendMode("add") end
local function alpha() lg.setBlendMode("alpha") end

-- ---- Shape helpers -----------------------------------------------------------

-- A soft glow of radius r at (x, y) (call between add() and alpha()).
local function glow(x, y, r, h, a)
  color(h, a)
  lg.draw(glowImg, x, y, 0, r / 32, r / 32, 32, 32)
end

-- A line with round ends, `w` thick.
local function capsule(x1, y1, x2, y2, w)
  lg.setLineWidth(w)
  lg.line(x1, y1, x2, y2)
  lg.circle("fill", x1, y1, w / 2)
  lg.circle("fill", x2, y2, w / 2)
end

local function ring(x, y, r, w)
  lg.setLineWidth(w)
  lg.circle("line", x, y, r)
end

local function arc(x, y, r, w, a1, a2)
  lg.setLineWidth(w)
  lg.arc("line", "open", x, y, r, a1, a2)
end

-- Picture `name` from the atlas, centred at (x, y), `w` units wide (`h` tall, or
-- in proportion), turned `r`.
local function sprite(name, x, y, w, h, r)
  local s = S[name]
  h = h or w * s[4] / s[3]
  lg.draw(atlas, quads[name], x, y, r or 0, w / s[3], h / s[4], s[3] / 2, s[4] / 2)
end

-- The same picture again, added on top: a white hit flash of strength k.
local function flashOver(name, x, y, w, h, k, quad)
  if k > 0.02 then
    add()
    lg.setColor(1, 1, 1, min(k, 1))
    local s = S[name]
    local cw = if quad then s[4] else s[3]
    lg.draw(atlas, quad or quads[name], x, y, 0, w / cw, (h or w) / s[4], cw / 2, s[4] / 2)
    alpha()
  end
end

-- A neon tube through `pts`: a wide additive glow, the tube, a white-hot core.
local function neon(pts, width, h, a)
  add()
  color(h, 0.18 * a)
  lg.setLineWidth(width * 2.6)
  lg.line(pts)
  alpha()
  color(h, a)
  lg.setLineWidth(width)
  lg.line(pts)
  light(h, 0.7, a)
  lg.setLineWidth(width * 0.38)
  lg.line(pts)
end

-- Text centred at (x, y) with a dark outline: `size` picks the font.
local function say(text, x, y, font, h, a, ink)
  a = a or 1
  local tw, th = font:getWidth(text), font:getHeight()
  local ox, oy = x - tw / 2, y - th / 2
  lg.setFont(font)
  color(ink or look.INK, a)
  local o = max(1.5, th * 0.07)
  for i = 0, 7 do
    local an = i * pi / 4
    lg.print(text, ox + cos(an) * o, oy + sin(an) * o)
  end
  color(h, a)
  lg.print(text, ox, oy)
end
draw.say = say

function draw.load()
  atlas = lg.newImage("sprites")
  backdrop = lg.newImage("backdrop")
  glowImg = lg.newImage("glow")
  fadeImg = lg.newImage("fade")
  for name, s in S do quads[name] = lg.newQuad(s[1], s[2], s[3], s[4], atlas) end
  local sw = S.swooper
  for f = 0, 3 do swoopQuads[f + 1] = lg.newQuad(sw[1] + f * sw[4], sw[2], sw[4], sw[4], atlas) end
  fonts.banner = lg.newFont("Orbitron", 38)
  fonts.small = lg.newFont("Exo 2", 15)
  fonts.combo = lg.newFont("Orbitron", 22)
  fonts.chip = lg.newFont("Orbitron", 12)
  fonts.hint = lg.newFont("Exo 2", 21)
  fonts.mark = lg.newFont("Orbitron", 22)
  -- The sky's stars and the streaks: fixed places (their own generator).
  local r = love.math.newRandomGenerator(101)
  for i = 1, 28 do
    stars[i] = { x = r:random() * W, y = r:random() * (look.HORIZON - 30), rate = 1.2 + r:random() * 3, size = 1 + r:random() }
  end
  for i = 1, 12 do streaks[i] = { x = r:random() * W, y = r:random() * H, speed = 150 + 90 * r:random() } end
end

-- ---- The world -----------------------------------------------------------------

-- The generated synthwave backdrop (sky, sun, wireframe mountains), dimmed a
-- little so bullets pop and warming with the heat; over it, in code, a neon
-- floor grid rushing toward you, twinkling stars and speed streaks.
function draw.background(s, time)
  local hot = clamp(s:heat() - 1, 0, 1)
  mixColor(look.BG_TINT, look.BG_HOT_TINT, hot)
  lg.draw(backdrop, 0, 0, 0, W / 540, H / 960)
  local speed = 1 + 0.6 * hot
  local HZ = look.HORIZON
  add()
  -- The horizon's glow, up into the sky and down onto the floor.
  color(look.HORIZON_GLOW, 0.3)
  lg.draw(fadeImg, 0, HZ, 0, W / 4, -14 / 64)
  color(look.HORIZON_GLOW, 0.28)
  lg.draw(fadeImg, 0, HZ, 0, W / 4, 40 / 64)
  -- Cross lines, spaced by perspective, scrolling toward you.
  local depth = H - HZ
  local phase = (time * 0.6 * speed) % 1
  for i = 0, 9 do
    local d = (i + phase) / 10
    local y = HZ + depth * d * d
    local w = 0.8 + 1.6 * d
    mixColor(look.GRID, look.GRID_HOT, hot, 0.08 + 0.3 * d)
    lg.rectangle("fill", 0, y - w * 0.5, W, w)
  end
  -- Rails converging on the vanishing point, brighter toward you.
  for i = -8, 8 do
    local x0, x1 = W / 2 + i * 5, W / 2 + i * 62
    for _, seg in { { 0, 0.35, 0.1 }, { 0.35, 0.7, 0.2 }, { 0.7, 1, 0.3 } } do
      local f0, f1 = seg[1], seg[2]
      mixColor(look.GRID, look.GRID_HOT, hot, seg[3])
      lg.setLineWidth(0.8 + 1.2 * f1)
      lg.line(x0 + (x1 - x0) * f0, HZ + depth * f0, x0 + (x1 - x0) * f1, HZ + depth * f1)
    end
  end
  -- Stars twinkling in the sky.
  for i, st in stars do
    local tw = 0.5 + 0.5 * sin(time * st.rate + i)
    color(look.STAR, 0.8 * tw)
    lg.rectangle("fill", st.x, st.y, st.size, st.size)
  end
  -- Speed streaks falling past, faster as the heat rises.
  color(look.STREAK, 0.22)
  for _, st in streaks do
    local y = (st.y + time * st.speed * speed) % (H + 40) - 20
    lg.rectangle("fill", st.x, y, 1.5, 8 + 10 * speed)
  end
  alpha()
end

-- Pip's face in its dark visor (about 16 × 14 units): two tall white eyes.
-- mood: "calm", "worried", "happy", "ouch" or "dead"; (lx, ly) where it looks.
local function face(cx, cy, mood, lookX, lookY, time, a)
  local ex = 3.7
  local d = max(sqrt(lookX * lookX + lookY * lookY), 1e-3)
  local lx, ly = lookX / d, lookY / d * 1.1
  if mood == "calm" or mood == "worried" then
    local worried = mood == "worried"
    local blinking = not worried and (time % 3.3) < 0.12
    for side = -1, 1, 2 do
      local x = cx + side * ex
      color(look.EYE, a)
      if blinking then
        capsule(x - 2, cy, x + 2, cy, 1.5)
      else
        local hh, w = if worried then 1.9 else 1.4, if worried then 4.8 else 4.2
        capsule(x, cy - hh, x, cy + hh, w)
        color(look.INK, a)
        lg.circle("fill", x + lx, cy + ly, if worried then 1.0 else 1.35)
      end
      if worried then
        -- Brows up in the middle.
        color(look.SHIP_TRIM, a)
        capsule(x - side * 0.6, cy - 5.6, x + side * 2.6, cy - 4.2, 1.2)
      end
    end
    if worried then
      local k = (time * 3) % 1
      color(look.SWEAT, a * (1 - k))
      lg.circle("fill", cx + 10.5, cy - 3 + k * 6, 1.8)
    end
  else
    color(look.EYE, a)
    lg.setLineWidth(1.5)
    for side = -1, 1, 2 do
      local x = cx + side * ex
      if mood == "happy" then
        lg.line(x - 2.4, cy + 1, x, cy - 1.8, x + 2.4, cy + 1)
      elseif mood == "ouch" then
        lg.line(x - side * 2.4, cy - 2.4, x + side * 1.6, cy, x - side * 2.4, cy + 2.4)
      else
        capsule(x - 2, cy - 2, x + 2, cy + 2, 1.4)
        capsule(x - 2, cy + 2, x + 2, cy - 2, 1.4)
      end
    end
  end
end

-- Pip. `wreck`: where its wreck floats under the game-over card ({ x, y }), or nil.
function draw.ship(s, time, punch, wreck)
  local sh = s.ship
  local dying = s.dying ~= nil
  local x, y = sh.x, sh.y
  if wreck then
    x, y = wreck.x, wreck.y
    -- Smoke curling up off it.
    for i = 0, 2 do
      local k = (time * 0.7 + i / 3) % 1
      local px = x + sin(time * 2 + i * 2.1) * 7 * k
      lg.setColor(0.54, 0.5, 0.63, 0.35 * (1 - k))
      lg.draw(glowImg, px, y - 12 - k * 40, 0, (5 + k * 10) / 24, (5 + k * 10) / 24, 32, 32)
    end
  end
  local blink = sh.invuln > 0 and not dying and floor(time * 14) % 2 == 0
  local a = if blink then 0.28 else 1
  local tilt = if dying or wreck then sh.spin else clamp(sh.vx * 0.0016, -0.35, 0.35)
  lg.push()
  lg.translate(x, y)
  lg.rotate(tilt)
  lg.scale(punch, punch)
  if not dying and not wreck then
    -- A cyan under-glow and the exhaust: three nested flames, additive.
    local flick = sin(time * 43) * 3 + sin(time * 71) * 2
    local n = look.PIP_NOZZLE
    add()
    glow(0, 0, 40, look.SHIP_TRIM, 0.13 * a)
    glow(0, n + 4, 14, look.FLAME_OUT, 0.45 * a)
    -- A longer burn while the thumb is on the glass: you're steering.
    local len = 15 + flick + (if s.grab then 7 else 0)
    color(look.FLAME_EDGE, 0.75 * a)
    lg.polygon("fill", -5.5, n, 5.5, n, 0, n + len)
    color(look.FLAME_OUT, a)
    lg.polygon("fill", -3.6, n, 3.6, n, 0, n + len * 0.78)
    color(look.FLAME_IN, a)
    lg.polygon("fill", -1.8, n, 1.8, n, 0, n + len * 0.45)
    alpha()
  end
  if wreck then color(look.WRECK_TINT) else lg.setColor(1, 1, 1, a) end
  sprite("pip", 0, 0, look.PIP_SIZE)
  if sh.spread > 0 and not wreck then
    -- Little cannons glowing on the wing tips while Spread is on.
    color(look.SHOT, a)
    for side = -1, 1, 2 do
      local wx, wy = side * look.PIP_WINGTIP[1], look.PIP_WINGTIP[2]
      capsule(wx, wy, wx, wy - 10, 4)
    end
  end
  local mood = "calm"
  if dying or wreck then
    mood = "dead"
  elseif sh.invuln > sim.INVULN_TIME - 0.5 then
    mood = "ouch"
  elseif sh.happy > 0 then
    mood = "happy"
  elseif sh.danger > 0.45 then
    mood = "worried"
  end
  -- Eyes on the nearest bullet (or up the screen).
  local lx, ly, best = 0, -1, 140 * 140
  for _, b in s.bullets do
    local dx, dy = b.x - sh.x, b.y - sh.y
    local d = dx * dx + dy * dy
    if d < best then lx, ly, best = dx, dy, d end
  end
  face(look.PIP_VISOR[1], look.PIP_VISOR[2], mood, lx, ly, time, if wreck then 0.6 else a)
  lg.pop()
  if dying or wreck then return end
  if sh.shield then
    local k = sin(time * 5) * 0.5 + 0.5
    add()
    glow(sh.x, sh.y, 44, look.SHIELD, 0.16 + 0.06 * k)
    alpha()
    color(look.SHIELD, 0.8)
    ring(sh.x, sh.y, 36 + k, 2.5)
    lg.setColor(1, 1, 1, 0.7)
    arc(sh.x, sh.y, 33, 3, time * 2, time * 2 + 0.9)
    color(look.SHIP_TRIM, 0.7)
    arc(sh.x, sh.y, 33, 3, time * 2 + pi, time * 2 + pi + 0.9)
  end
  if sh.magnetT > 0 then
    local k = (time * 1.6) % 1
    color(look.POWER.magnet, 0.35 * k)
    ring(sh.x, sh.y, 40 + 60 * (1 - k), 2)
  end
end

-- Two eyes at (x ± dx, y) looking at the ship, under angry neon brows.
local function eyes(x, y, dx, r, tx, ty, brow)
  for side = -1, 1, 2 do
    local ex = x + side * dx
    local vx, vy = tx - ex, ty - y
    local d = max(sqrt(vx * vx + vy * vy), 1e-3)
    color(look.EYE)
    lg.circle("fill", ex, y, r)
    color(look.INK)
    lg.circle("fill", ex + vx / d * r * 0.4, y + vy / d * r * 0.4, r * 0.58)
    -- The brow cuts down across the top of the eye: a glare.
    light(brow, 0.25)
    capsule(ex - side * r * 1.25, y - r * 1.45, ex + side * r * 0.9, y - r * 0.55, r * 0.75)
  end
end

local function hpArc(x, y, r, frac, h)
  if frac < 0.999 then
    color(look.INK, 0.6)
    ring(x, y, r, 3)
    light(h, 0.3)
    arc(x, y, r, 3, -pi / 2, -pi / 2 + TAU * max(frac, 0))
  end
end

local function enemy(s, e, time)
  local tx, ty = s.ship.x, s.ship.y
  local base = look.ENEMY[e.kind]
  local r = sim.KINDS[e.kind].radius
  local x, y = e.x, e.y
  local fl = e.flash * 0.85
  -- The tell: a glow that swells before it fires.
  local tell = 0
  if sim.telegraph(e) and e.y < 430 and s:alive() and s.t > sim.GRACE then tell = 1 - e.fireCd / sim.TELL end
  add()
  glow(x, y, r * 2.2, base, 0.2 + 0.25 * e.flash)
  alpha()
  local mouthY = y + r * 0.7
  lg.setColor(1, 1, 1)
  if e.kind == "jelly" then
    -- Squash and stretch on a bob; the bell sits on the body's centre.
    local bob = sin(e.age * 5 + e.x0)
    local w, h = look.JELLY_SIZE * (1 + 0.06 * bob), look.JELLY_SIZE * (1 - 0.06 * bob)
    local cy = y - look.JELLY_FACE_Y * h / look.JELLY_SIZE
    sprite("jelly", x, cy, w, h)
    flashOver("jelly", x, cy, w, h, fl)
    eyes(x, y, 3.8, 3.1, tx, ty, base)
  elseif e.kind == "swooper" then
    local f = floor(e.age * 12) % 4 + 1
    local cy = y - look.SWOOPER_LIFT
    local sz = look.SWOOPER_SIZE
    lg.draw(atlas, swoopQuads[f], x, cy, 0, sz / 112, sz / 112, 56, 56)
    flashOver("swooper", x, cy, sz, sz, fl, swoopQuads[f])
    eyes(x, cy + look.SWOOPER_FACE_Y[f], 2.9, 2.3, tx, ty, base)
  elseif e.kind == "darter" then
    -- Points where it's going; a dashed line shows where it will dash.
    local ang = if e.stage == 0 then pi / 2 else atan2(e.ay - y, e.ax - x)
    if e.stage == 1 or e.stage == 2 then
      local locked = e.stage == 2
      local on = not locked or floor(time * 16) % 2 == 0
      color(look.DANGER, if locked then (if on then 0.85 else 0.35) else 0.22)
      local ux, uy = cos(ang), sin(ang)
      local d = 24
      lg.setLineWidth(if locked then 3.5 else 2)
      while d < 700 do
        lg.line(x + ux * d, y + uy * d, x + ux * (d + 12), y + uy * (d + 12))
        d += 26
      end
    end
    if e.stage == 3 then
      -- Afterimages behind the dash.
      add()
      for k = 1, 3 do
        local f = k * 0.025
        color(base, 0.35 / k)
        sprite("darter", x - e.vx * f, y - e.vy * f, look.DARTER_SIZE, nil, ang)
      end
      alpha()
    end
    lg.setColor(1, 1, 1)
    lg.push()
    lg.translate(x, y)
    lg.rotate(ang)
    sprite("darter", 0, 0, look.DARTER_SIZE)
    flashOver("darter", 0, 0, look.DARTER_SIZE, nil, fl)
    -- One eye, looking where it's headed.
    color(look.EYE)
    lg.circle("fill", look.DARTER_EYE, 0, 3.6)
    color(look.INK)
    lg.circle("fill", look.DARTER_EYE + 1.3, 0, 1.9)
    lg.pop()
    if e.stage == 2 then say("!", x, y - r - 14, fonts.mark, look.DANGER) end
  elseif e.kind == "bulb" then
    sprite("bulb", x, y, look.BULB_SIZE)
    flashOver("bulb", x, y, look.BULB_SIZE, nil, fl)
    -- One big eye on the ship, a brow, and a mouth that opens as it charges.
    local vx, vy = tx - x, ty - (y - 3)
    local d = max(sqrt(vx * vx + vy * vy), 1e-3)
    color(look.EYE)
    lg.circle("fill", x, y - 3, 7)
    color(look.BULB_IRIS)
    lg.circle("fill", x + vx / d * 2.6, y - 3 + vy / d * 2.6, 4)
    color(look.INK)
    lg.circle("fill", x + vx / d * 3.2, y - 3 + vy / d * 3.2, 2)
    light(base, 0.25)
    capsule(x - 8, y - 11.5, x + 8, y - 10, 2.8)
    local open = 1.6 + 4.4 * tell
    mixColor(base, look.BULLET_BIG_RIM, tell)
    lg.rectangle("fill", x - 5, y + 7 - open * 0.3, 10, open, open * 0.5)
    mouthY = y + 7
    hpArc(x, y, r + 7, e.hp / e.maxHp, base)
  else -- spinner
    sprite("spinner", x, y, look.SPINNER_SIZE, nil, e.spin)
    if fl > 0.02 then
      add()
      lg.setColor(1, 1, 1, min(fl, 1))
      sprite("spinner", x, y, look.SPINNER_SIZE, nil, e.spin)
      alpha()
    end
    eyes(x, y + 0.5, 3.8, 2.9, tx, ty, base)
    hpArc(x, y, r + 8, e.hp / e.maxHp, base)
  end
  if tell > 0 then
    add()
    glow(x, mouthY, 8 + 14 * tell, look.BULLET_RIM, 0.6 * tell)
    alpha()
    lg.setColor(1, 1, 1, tell)
    lg.circle("fill", x, mouthY, 2 + 3 * tell)
  end
end

local function boss(s, b, time)
  local dying = b.dying or 0
  local enraged = sim.enraged(b)
  -- The art is tinted: red when enraged, flickering as she dies.
  local tint, tintK = look.WHITE, 0
  if enraged then tint, tintK = look.BOSS_RAGE, 0.35 + 0.2 * sin(time * 9) end
  if b.dying then
    tint = if floor(time * 20) % 2 == 0 then look.WHITE else 0x7050b0
    tintK = min(dying / sim.BOSS_DEATH, 1) * 0.7
  end
  local neonA = if b.dying and floor(time * 20) % 2 == 1 then 0.5 else 1
  local x, y = b.x, b.y
  add()
  glow(x, y, 150, look.BOSS, 0.22)
  alpha()
  -- Tentacles: neon tubes swaying under the frill.
  for i = 0, 6 do
    local k = i - 3
    local tx = x + k * 17
    local w = sin(time * 4 + i * 0.9) * 9
    neon({ tx, y + 34, tx + w * 0.5, y + 58, tx - w * 0.4, y + 84 }, 5.5 - abs(k) * 0.6, if i % 2 == 0 then look.BOSS_NEON else look.BOSS_NEON_2, neonA)
  end
  -- Arms out to the hands (they fire the Rain).
  local rain = b.attack == "rain" and b.attackT > sim.CHARGE * 0.5 and b.age >= 2.4
  local hands = {}
  for side = -1, 1, 2 do
    local hx = x + side * sim.BOSS_HAND[1]
    local hy = y + sim.BOSS_HAND[2] + sin(time * 3 + side) * 3
    local mx, my = x + side * 74, y + 2 + cos(time * 3 + side) * 2
    neon({ x + side * 56, y + 14, mx, my, hx, hy }, 5, look.BOSS_NEON, neonA)
    hands[#hands + 1] = { hx, hy }
  end
  for _, hnd in hands do
    if rain then
      add()
      glow(hnd[1], hnd[2], 30, look.BULLET_RIM, 0.6)
      alpha()
    end
    mixColor(look.WHITE, tint, tintK)
    sprite("hand", hnd[1], hnd[2], look.HAND_SIZE)
  end
  -- The crowned dome.
  local dx, dy = x, y - look.BOSS_LIFT
  local dh = look.BOSS_SIZE * S.boss[4] / S.boss[3]
  mixColor(look.WHITE, tint, tintK)
  sprite("boss", dx, dy, look.BOSS_SIZE)
  flashOver("boss", dx, dy, look.BOSS_SIZE, dh, b.flash * 0.7)
  -- Eyes, brows and the mouth that opens to charge, in the face plate.
  local fx, fy = x + look.BOSS_FACE[1], y + look.BOSS_FACE[2]
  local tx, ty = s.ship.x, s.ship.y
  for side = -1, 1, 2 do
    local ex, ey = fx + side * 8.5, fy - 5
    local vx, vy = tx - ex, ty - ey
    local d = max(sqrt(vx * vx + vy * vy), 1e-3)
    color(look.EYE)
    lg.circle("fill", ex, ey, 6)
    color(if enraged then look.DANGER else look.INK)
    lg.circle("fill", ex + vx / d * 2.4, ey + vy / d * 2.4, 3.1)
    color(look.BOSS_NEON)
    capsule(ex - side * 7.5, ey - 10.5, ex + side * 3.5, ey - 6.5, 3.4)
  end
  local charge = sim.charge(b)
  local firing = b.age >= 2.4 and not b.dying and b.attackT > sim.CHARGE and b.attackT < sim.CHARGE + sim.ATTACK_FIRE
  local open = if charge > 0 then 2.5 + 9 * charge elseif firing then 7 else 2.5
  local my = fy + 10
  mixColor(look.BOSS_NEON, look.WHITE, max(charge, if firing then 0.6 else 0))
  lg.rectangle("fill", fx - 9, my - open * 0.4, 18, open, open * 0.5)
  if charge > 0 then
    -- Rings closing in on the mouth: the attack is coming.
    color(look.BULLET_RIM, charge)
    ring(fx, my, 10 + 50 * (1 - charge), 3)
  end
end

-- Enemy bullets: a glow, then a dark outline, the hot rim and a white core,
-- so they read on the sun, the grid and the swarm alike.
local function bullets(s)
  local list = s.bullets
  add()
  for _, b in list do
    glow(b.x, b.y, b.r * 3.2, if b.big then look.BULLET_BIG_RIM else look.BULLET_RIM, 0.55)
  end
  alpha()
  color(look.BULLET_OUTLINE)
  for _, b in list do
    lg.circle("fill", b.x, b.y, b.r * 1.45 * (1 + 0.08 * sin(b.age * 14)) + 1.8)
  end
  for _, b in list do
    color(if b.big then look.BULLET_BIG_RIM else look.BULLET_RIM)
    lg.circle("fill", b.x, b.y, b.r * 1.45 * (1 + 0.08 * sin(b.age * 14)))
  end
  color(look.BULLET_CORE)
  for _, b in list do lg.circle("fill", b.x, b.y, b.r * 0.82) end
end

local function shots(s)
  add()
  color(look.SHOT, 0.6)
  for _, p in s.shots do capsule(p.x, p.y, p.x - p.vx * 0.02, p.y - p.vy * 0.02, 7) end
  lg.setColor(1, 1, 1)
  for _, p in s.shots do capsule(p.x, p.y, p.x - p.vx * 0.02, p.y - p.vy * 0.02, 2.6) end
  alpha()
end

local function drop(d, time)
  if d.kind == "gem" then
    -- Spins by squeezing its width, lit from inside.
    local w = cos(d.age * 7 + d.x * 0.1)
    add()
    glow(d.x, d.y, 16, look.GEM, 0.45)
    alpha()
    local gw = look.GEM_SIZE * max(abs(w), 0.25)
    if w > 0 then lg.setColor(1, 1, 1) else color(0xd8c8b0) end
    sprite("gem", d.x, d.y, gw, look.GEM_SIZE)
    add()
    color(look.GEM, 0.55)
    sprite("gem", d.x, d.y, gw, look.GEM_SIZE)
    alpha()
  elseif d.kind == "power" then
    local bob = sin(time * 4 + d.x) * 3
    local x, y = d.x, d.y + bob
    local k = sin(time * 6) * 0.5 + 0.5
    add()
    glow(x, y, 30 + 4 * k, look.POWER[d.power], 0.5)
    alpha()
    lg.setColor(1, 1, 1)
    sprite(d.power, x, y, look.POWER_SIZE)
  else
    local k = 1 + 0.12 * sin(time * 8)
    add()
    glow(d.x, d.y, 30, look.HEART, 0.5)
    alpha()
    lg.setColor(1, 1, 1)
    sprite("heart", d.x, d.y, look.HEART_SIZE * k)
  end
end

-- Everything in the world, back to front. `wreck`: where Pip's wreck floats
-- under the game-over card, or nil.
function draw.world(s, time, punch, wreck)
  for _, d in s.drops do drop(d, time) end
  for _, e in s.enemies do enemy(s, e, time) end
  if s.boss then boss(s, s.boss, time) end
  shots(s)
  draw.ship(s, time, punch, wreck)
  bullets(s)
  if s.dying and s.killer and not s.over then
    -- Why you died: a ring where the fatal hit landed.
    local k = sin(time * 10) * 0.5 + 0.5
    color(look.DANGER)
    ring(s.killer.x, s.killer.y, 16 + 5 * k, 3)
  end
  lg.setColor(1, 1, 1)
end

-- ---- The HUD's own pieces ----------------------------------------------------------
-- (The score and the hearts are the engine's HUD, in studio.toml's identity.)

-- The combo: a chain count until it's worth a multiplier, then ×N, in a ring
-- that empties as the chain runs out. (cx, cy): its middle.
function draw.combo(s, cx, cy, punch)
  if s.combo < 2 then return end
  local m = s:multiplier()
  local heat = min((m - 1) / 4, 1)
  lg.push()
  lg.translate(cx, cy)
  lg.scale(punch, punch)
  color(look.INK, 0.6)
  lg.circle("fill", 0, 0, 25)
  mixColor(look.SHIP_TRIM, look.GOLD, heat, 0.25)
  ring(0, 0, 25, 5)
  mixColor(look.SHIP_TRIM, look.GOLD, heat)
  arc(0, 0, 25, 5, -pi / 2, -pi / 2 + TAU * clamp(s.comboLeft / sim.COMBO_WINDOW, 0, 1))
  say(if m >= 2 then "×" .. m else tostring(s.combo), 0, -1, fonts.combo, look.WHITE)
  lg.pop()
  local c = if heat > 0.5 then look.GOLD else look.SHIP_TRIM
  say(if m >= 2 then s.combo .. " CHAIN" else "CHAIN", cx, cy + 36, fonts.small, c)
end

-- The power-ups running, as chips from (x, y) rightward, each with the
-- time it has left.
function draw.powers(s, x, y)
  local sh = s.ship
  local function chip(p, frac)
    color(look.INK, 0.7)
    lg.circle("fill", x, y, 15)
    lg.setColor(1, 1, 1)
    sprite(p, x, y, 30)
    if p == "spread" then say(if sh.spread > 1 then "5" else "3", x + 10, y + 10, fonts.chip, look.WHITE) end
    if frac then
      color(look.POWER[p])
      arc(x, y, 15, 3, -pi / 2, -pi / 2 + TAU * clamp(frac, 0, 1))
    end
    x += 34
  end
  if sh.spread > 0 then chip("spread", sh.spreadT / sim.POWER_TIME) end
  if sh.shield then chip("shield", nil) end
  if sh.magnetT > 0 then chip("magnet", sh.magnetT / sim.POWER_TIME) end
end

-- The queen's health bar, centred at y.
function draw.bossBar(s, y)
  local b = s.boss
  if not b then return end
  local fill = if b.age < 2.4 then min(b.age / 2, 1) else 1
  local frac = max(b.hp / b.maxHp, 0) * fill
  local x0, w, h = W / 2 - 100, 200, 16
  color(look.INK, 0.8)
  lg.rectangle("fill", x0 - 3, y - h / 2 - 3, w + 6, h + 6, 11)
  local c = if sim.enraged(b) then look.DANGER else look.BOSS
  if frac > 0 then
    color(c)
    lg.rectangle("fill", x0, y - h / 2, w * frac, h, 8)
    light(c, 0.35, 0.8)
    lg.rectangle("fill", x0 + 3, y - h / 2 + 2, max(w * frac - 6, 0), h * 0.35, 4)
  end
  say(look.BOSS_NAME, W / 2, y, fonts.small, look.WHITE)
end

-- A banner across the middle: a new enemy, the WARNING, the queen down.
function draw.banner(s)
  local b = s.banner
  if not b then return end
  local big, small, h = "", look.NEW_ENEMY, look.WHITE
  if b.kind == "warning" then
    big, small = look.WARNING[1], look.WARNING[2]
  elseif b.kind == "clear" then
    big, small, h = look.CLEAR[1], look.CLEAR[2], look.GOLD
  else
    big = look.NAMES[b.enemy]
  end
  local t = b.age
  local u = min(t / 0.35, 1) - 1
  local enter = 1 + 2.70158 * u ^ 3 + 1.70158 * u ^ 2 -- back out
  local a = if t > 1.7 then max(1 - (t - 1.7) / 0.5, 0) else 1
  local y = 262
  if b.kind == "warning" then
    local k = 0.5 + 0.5 * sin(t * 12)
    color(look.DANGER, (0.35 + 0.25 * k) * a)
    lg.rectangle("fill", 0, y - 34, W, 68)
    color(look.DANGER, a)
    lg.rectangle("fill", 0, y - 38, W, 4)
    lg.rectangle("fill", 0, y + 34, W, 4)
  end
  lg.push()
  lg.translate(W / 2, y)
  lg.scale(0.4 + 0.6 * enter, 0.4 + 0.6 * enter)
  if b.kind == "new" then
    -- The enemy's own neon, lightened so it reads on the sky.
    local c = rgb(look.ENEMY[b.enemy])
    h = floor((c[1] + (1 - c[1]) * 0.3) * 255) * 65536 + floor((c[2] + (1 - c[2]) * 0.3) * 255) * 256 + floor((c[3] + (1 - c[3]) * 0.3) * 255)
  end
  say(big, 0, -6, fonts.banner, h, a)
  say(small, 0, 24, fonts.small, look.WHITE, a)
  lg.pop()
end

-- The drag hint: a ghost thumb sliding side to side under the ship, arrows
-- either side, and the words.
function draw.hint(s, time)
  local sh = s.ship
  local swing = sin(time * 2.2)
  local fx, fy = sh.x + swing * 46, min(sh.y + sim.MIN_LIFT + 14, H - 24)
  lg.setColor(1, 1, 1, 0.2)
  capsule(sh.x, sh.y + 26, fx, fy - 18, 2)
  local pulse = (time * 1.6) % 1
  color(look.SHIP_TRIM, 0.7 * (1 - pulse))
  ring(fx, fy, 18 + 16 * pulse, 2.5)
  lg.setColor(1, 1, 1, 0.65)
  lg.rectangle("fill", fx - 15, fy - 19, 30, 38, 15)
  color(look.INK, 0.5)
  lg.setLineWidth(2)
  lg.rectangle("line", fx - 15, fy - 19, 30, 38, 15)
  color(look.SHIP_TRIM, 0.3)
  lg.rectangle("fill", fx - 9, fy - 15, 18, 13, 6)
  for side = -1, 1, 2 do
    local ax = sh.x + side * 82
    local push = if side * swing > 0 then abs(swing) * 4 else 0
    local px = ax + side * push
    color(look.INK, 0.6)
    lg.polygon("fill", px + side * 13, fy, px - side * 7, fy - 13, px - side * 7, fy + 13)
    lg.setColor(1, 1, 1, 0.85)
    lg.polygon("fill", px + side * 10, fy, px - side * 5, fy - 10, px - side * 5, fy + 10)
  end
  say(look.HINT, W / 2, min(fy + 42, H - 18), fonts.hint, look.WHITE)
end

-- A tint over the whole screen (a flash), colour h at alpha a.
function draw.tint(h, a)
  color(h, a)
  lg.rectangle("fill", 0, 0, W, H)
end

-- The last heart: the screen's edges pulse red (`k` 0…1).
function draw.edges(k)
  if k <= 0.01 then return end
  color(look.DANGER, k)
  local d = 70
  lg.draw(fadeImg, 0, 0, 0, W / 4, d / 64)
  lg.draw(fadeImg, 0, H, 0, W / 4, -d / 64)
  lg.draw(fadeImg, 0, 0, -pi / 2, -H / 4, d / 64)
  lg.draw(fadeImg, W, 0, pi / 2, H / 4, d / 64)
end

return draw
