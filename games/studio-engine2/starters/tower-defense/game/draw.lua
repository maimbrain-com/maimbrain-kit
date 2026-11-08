-- Drawing: the painted clay pictures (images/) for the map, pads, towers,
-- jellies and the cake; love.graphics for what code does better (shadows,
-- shots, rings, the picker, the coin pill, the next-wave button, hints and
-- banners). All in love.draw, read only: nothing here changes the game.
--
-- Soft shadows and glows are one picture, images/soft.png (a white disc
-- that fades out), tinted and stretched; the red edge of the last heart is
-- images/vignette.png.

local sim = require("sim")
local look = require("look")
local layout = require("layout")

local draw = {}

local lg = love.graphics
local sin, cos, abs, min, max, floor, pi = math.sin, math.cos, math.abs, math.min, math.max, math.floor, math.pi

local img, quads, fonts = {}, {}, {}

-- 0xRRGGBB → r, g, b (0…1).
local function rgb(h)
  return floor(h / 65536) % 256 / 255, floor(h / 256) % 256 / 255, h % 256 / 255
end

local function color(h, a)
  local r, g, b = rgb(h)
  lg.setColor(r, g, b, a or 1)
end
draw.color = color

local function backOut(k)
  local c1 = 1.70158
  local f = k - 1
  return 1 + (c1 + 1) * f * f * f + c1 * f * f
end
local function quadOut(k) return 1 - (1 - k) * (1 - k) end
local function quadIn(k) return k * k end
local function quadInOut(k)
  if k < 0.5 then return 2 * k * k end
  return 1 - (-2 * k + 2) ^ 2 / 2
end

-- A cheap deterministic hash in 0…1 (scenery placement; no random in drawing).
local function hash(i)
  local x = math.sin(i * 12.9898 + 78.233) * 43758.5453
  return x - floor(x)
end

function draw.load()
  img.map = lg.newImage("map")
  img.soft = lg.newImage("soft")
  img.vignette = lg.newImage("vignette")
  for name, s in look.SHEETS do
    local im = lg.newImage(name)
    img[name] = im
    local q = {}
    for i = 1, s.frames do q[i] = lg.newQuad((i - 1) * s.cell, 0, s.cell, s.cell, im) end
    quads[name] = q
  end
  fonts[11] = lg.newFont(look.FONT, 11)
  fonts[13] = lg.newFont(look.FONT, 13)
  fonts[15] = lg.newFont(look.FONT, 15)
  fonts[17] = lg.newFont(look.FONT, 17)
  fonts[20] = lg.newFont(look.FONT, 20)
  fonts[24] = lg.newFont(look.FONT, 24)
  fonts[30] = lg.newFont(look.FONT, 30)
  fonts[36] = lg.newFont(look.FONT, 36)
end

-- ---- Helpers ----------------------------------------------------------------

-- A soft round blob (shadow, glow) of radii rx, ry.
local function soft(x, y, rx, ry, h, a)
  color(h, a)
  lg.draw(img.soft, x, y, 0, rx / 32, ry / 32, 32, 32)
end

local function ring(x, y, r, w, h, a)
  color(h, a)
  lg.setLineWidth(w)
  lg.circle("line", x, y, r)
end

-- A four-point sparkle.
local function sparkle(x, y, r, rot, h, a)
  local pts = {}
  for k = 0, 7 do
    local ang = rot + k * pi / 4
    local rr = if k % 2 == 0 then r else r * 0.32
    pts[#pts + 1] = x + cos(ang) * rr
    pts[#pts + 1] = y + sin(ang) * rr
  end
  color(h, a)
  lg.polygon("fill", pts)
end

-- Text centred on (x, y) in the game's font, with an ink outline.
local function say(text, size, x, y, h, a, outline)
  local f = fonts[size] or fonts[17]
  lg.setFont(f)
  local w, th = f:getWidth(text), f:getHeight()
  local tx, ty = x - w / 2, y - th / 2
  a = a or 1
  local o = outline or max(1.2, size * 0.09)
  color(look.INK, a)
  for k = 0, 7 do
    local ang = k * pi / 4
    lg.print(text, tx + cos(ang) * o, ty + sin(ang) * o)
  end
  color(h, a)
  lg.print(text, tx, ty)
end
draw.say = say

-- A coin: a gold disc with a darker rim and a shine.
local function coin(x, y, r, a)
  color(0xc98a1a, a)
  lg.circle("fill", x, y + 0.8, r)
  color(look.GOLD, a)
  lg.circle("fill", x, y, r * 0.86)
  color(0xfff2b0, a)
  lg.circle("fill", x - r * 0.3, y - r * 0.3, r * 0.25)
end

-- Frame `i` of sheet `name` with its bottom centre at (x, y), `w` units
-- wide, mirrored when `flip`, tinted.
local function sprite(name, i, x, y, w, flip, h, a)
  local s = look.SHEETS[name]
  local k = w / s.cell
  color(h or look.WHITE, a or 1)
  lg.draw(img[name], quads[name][i], x, y, 0, if flip then -k else k, k, s.cell / 2, s.cell)
end

-- The same frame again, added on top in `h` at `amount` (0…1): a white hit
-- flash, a frosty sheen.
local function glow(name, i, x, y, w, flip, h, amount)
  if amount <= 0 then return end
  lg.setBlendMode("add")
  sprite(name, i, x, y, w, flip, h, min(amount, 1) * 0.78)
  lg.setBlendMode("alpha")
end

-- ---- The board --------------------------------------------------------------

function draw.map()
  lg.setColor(1, 1, 1, 1)
  lg.draw(img.map, 0, 0, 0, 360 / img.map:getWidth(), 640 / img.map:getHeight())
end

-- Little pressed dimples that drift down the road toward the cake: which
-- way the jellies come.
function draw.roadFlow(t)
  local d = (t * 14) % 22
  local r = sim.ROAD_W * 0.07
  while d < sim.ROAD_LEN do
    local x, y = sim.pointAt(d)
    color(look.ROAD_DOT_SHADE, 0.5)
    lg.circle("fill", x, y + 0.6, r + 0.3)
    color(look.ROAD_DOT)
    lg.circle("fill", x, y, r)
    d += 22
  end
end

-- A selected pad's (or tower's) reach, popping open.
function draw.reach(x, y, r, k)
  r *= backOut(math.clamp(k, 0, 1))
  color(look.WHITE, 0.19)
  lg.circle("fill", x, y, r)
  ring(x, y, r, 2.5, look.WHITE, 0.7)
end

function draw.pad(x, y, empty, selected)
  soft(x, y + 8, 30, 22, look.SHADOW, 0.35)
  sprite("pad", 1, x, y + sim.PAD / 2 + 4, sim.PAD + 8, false)
  if selected then
    color(look.WHITE)
    lg.setLineWidth(3)
    lg.rectangle("line", x - sim.PAD / 2 - 1, y - sim.PAD / 2 - 1, sim.PAD + 2, sim.PAD + 2, 14)
  end
  if empty then
    color(look.PAD_PLUS, 0.75)
    lg.rectangle("fill", x - 9, y - 2, 18, 5, 2.5)
    lg.rectangle("fill", x - 2.5, y - 8.5, 5, 18, 2.5)
  end
end

-- ---- Jellies ----------------------------------------------------------------

function draw.jellyShadow(c)
  local r = sim.CREEPS[c.kind].radius
  soft(c.x, c.y + 1, r * 1.15, r * 0.44, look.SHADOW, 0.3)
end

-- One jelly picture: frame `frame` of `kind`'s sheet with its feet at (x,
-- y), sized by radius `r`, squashed by `wobble` (−1…1), mirrored when
-- `flip`. Shared by the road, the next-wave button and the cake's pile.
function draw.jellyPic(x, y, r, kind, frame, flip, wobble, a, flash, frost)
  local name = "jelly_" .. kind
  local w = r * look.JELLY_CELL_PER_R[kind]
  local foot = w * 6 / look.SHEETS[name].cell -- the sheet's bottom margin (6 texels)
  lg.push()
  lg.translate(x, y + foot)
  lg.scale(1 + 0.08 * wobble, 1 - 0.08 * wobble)
  sprite(name, frame, 0, 0, w, flip, look.WHITE, a)
  if frost then glow(name, frame, 0, 0, w, flip, look.FROST_GLOW, 0.45) end
  if flash and flash > 0 then glow(name, frame, 0, 0, w, flip, look.WHITE, flash) end
  lg.pop()
end
local jellyPic = draw.jellyPic

function draw.jelly(c, t)
  local spec = sim.CREEPS[c.kind]
  local r = spec.radius
  local speed = sim.curSpeed(c) / spec.speed
  local phase = c.age * 7 * max(speed, 0.4) + c.id * 1.7
  local wobble = sin(phase * 1.4)
  local hop = abs(sin(phase * 1.4)) * 2
  -- Which way it walks: mirror the sheet when heading left.
  local ax = sim.pointAt(c.d + 4)
  local flip = ax < c.x - 0.5
  if c.kind == "zipper" then
    -- Speed lines behind it.
    local bx, by = sim.pointAt(c.d - 1)
    local mx, my = math.sign(c.x - bx) * min(abs(c.x - bx), 1), math.sign(c.y - by) * min(abs(c.y - by), 1)
    color(look.WHITE, 0.63)
    lg.setLineWidth(3.2)
    for k = 0, 2 do
      local o = (k - 1) * 5
      local px, py = c.x - mx * 12 + my * o, c.y - 8 - my * 12 - mx * o
      lg.line(px, py, px - mx * 8, py - my * 8)
    end
  end
  local frame = if c.kind == "gummy" and c.flash > 0.55 then look.GUMMY_HIT else floor(phase / 1.6) % 2 + 1
  jellyPic(c.x, c.y - hop, r, c.kind, frame, flip, wobble, 1, c.flash * 0.8, c.slow > 0)
  if c.slow > 0 then sparkle(c.x + r * 0.7, c.y - r * 1.8, 3.5, t * 3 + c.id, 0xe6f9ff) end
  if c.hp < c.maxHp then
    local w = max(r * 2, 18)
    local bx, by = c.x - w / 2, c.y - r * 2.1 - 9 - hop
    color(look.INK, 0.6)
    lg.rectangle("fill", bx - 1, by - 1, w + 2, 6, 3)
    color(if c.kind == "king" then look.GOLD else 0x7dff8a)
    lg.rectangle("fill", bx, by, w * math.clamp(c.hp / c.maxHp, 0, 1), 4, 2)
  end
end

-- A popped jelly: its sheet's pop frame, bursting outward and fading.
function draw.splat(sp, life)
  local k = math.clamp(sp.age / life, 0, 1)
  local r = sim.CREEPS[sp.kind].radius * (1 + 0.35 * quadOut(k))
  jellyPic(sp.x, sp.y, r, sp.kind, look.POP_FRAME[sp.kind], sp.flip, 0, 1 - quadIn(k))
end

-- ---- Towers and shots -------------------------------------------------------

-- A tower standing on its pad (x, y is the pad's centre). `scale` < 1 for
-- the picker's icons (no lean, no pop-in).
function draw.tower(x, y, t, scale, time)
  local pop = if scale < 1 then 1 else backOut(min(t.age / 0.35, 1))
  local kick = t.recoil * t.recoil
  lg.push()
  lg.translate(x, y + 22 * scale)
  lg.scale(scale * pop, scale * pop)
  if t.kind == "chill" then
    -- A frosty glow that breathes, brighter on each pulse.
    local g = 0.5 + 0.5 * sin(time * 3)
    soft(0, -30 - (t.level - 1) * 5, 30 + (t.level - 1) * 3, 30 + (t.level - 1) * 3, look.FROST, 0.16 + 0.08 * g + 0.3 * kick)
  end
  -- Squash on each shot, and lean toward the target.
  local lean = if scale < 1 then 0 else cos(t.aim) * 0.08
  lg.rotate(lean)
  lg.scale(1 + 0.1 * kick, 1 - 0.14 * kick)
  local flip = scale >= 1 and t.kind ~= "chill" and cos(t.aim) < -0.2
  sprite("tower_" .. t.kind, t.level, 0, 6, look.TOWER_W * look.TOWER_LEVEL_GROW[t.kind][t.level], flip)
  lg.pop()
end

function draw.shot(sh)
  if sh.kind == "pop" then
    -- A clay gumball: one of the dome's colours, a soft rim and a highlight.
    local id = if sh.target then sh.target.id else 0
    local col = look.GUMBALLS[id % #look.GUMBALLS + 1]
    local dx, dy = sh.tx - sh.x, sh.ty - sh.y
    local n = max(math.sqrt(dx * dx + dy * dy), 1)
    color(look.WHITE, 0.38)
    lg.setLineWidth(7)
    lg.line(sh.x - dx / n * 9, sh.y - dy / n * 9, sh.x, sh.y)
    local r, g, b = rgb(col)
    lg.setColor(r * 0.65, g * 0.65, b * 0.65, 1)
    lg.circle("fill", sh.x, sh.y + 0.8, 5)
    lg.setColor(r, g, b, 1)
    lg.circle("fill", sh.x, sh.y, 4.4)
    soft(sh.x - 1.4, sh.y - 1.6, 2.2, 2.2, look.WHITE, 0.8)
  else
    local lift = sim.lift(sh)
    -- Its shadow on the ground shows where it lands.
    soft(sh.x, sh.y, 8, 3.2, look.SHADOW, 0.35)
    ring(sh.tx, sh.ty, sh.splash * 0.5, 1.5, look.WHITE, 0.38)
    sprite("bomb", 1, sh.x, sh.y - lift + 9, 20, false)
  end
end

-- ---- The cake ---------------------------------------------------------------

local PILE = { { -22, -40, "gummy" }, { 20, -44, "helmet" }, { -36, -14, "zipper" }, { 36, -18, "gummy" }, { 0, -58, "splitter" }, { -8, -30, "zipper" } }
local FAT = { { -48, 34, 1.25, false }, { 50, 30, 1.2, true }, { -22, 50, 1.1, false }, { 26, 52, 1.15, true } }

-- The cake at the end of the road: a clay cake on a plate whose face reacts
-- (happy, nervous, panicking, wincing at a bite), and the empty plate with
-- four smug jellies once it's devoured. `v`: { g, time, wince, gone }.
function draw.cake(v)
  local g, time = v.g, v.time
  local cx, cy = sim.CAKE_X, sim.CAKE_Y
  local endK = if g.ending then math.clamp(g.ending / sim.ENDING, 0, 1) else 0
  -- The plate's foot: where the sheet's bottom sits.
  local base = cy + 38
  local W = sim.CAKE_R * 3.05
  soft(cx, base - 6, 54, 16, look.SHADOW, 0.35)
  if v.gone then
    sprite("cake", look.CAKE_GONE, cx, base, W, false)
    -- The culprits, full and smug.
    for i, f in FAT do
      local w = sin(time * 2.5 + i * 1.3)
      jellyPic(cx + f[1], cy + f[2], 13 * f[3], "gummy", look.GUMMY_SMUG, f[4], w, 1)
    end
    return
  end
  local jx, jy = 0, 0
  if endK > 0 then jx, jy = sin(time * 50) * 2.5 * endK, cos(time * 43) * 1.5 * endK end
  local k = (1 - 0.45 * quadIn(endK)) * (1 - 0.04 * (sim.HEARTS - g.hearts))
  local danger = g:danger()
  local breathe = 1 + 0.02 * sin(time * 2) + 0.06 * v.wince
  local mood = look.CAKE_HAPPY
  if v.wince > 0.3 then mood = look.CAKE_WINCE
  elseif danger > 0.6 or endK > 0 then mood = look.CAKE_PANIC
  elseif danger > 0.25 then mood = look.CAKE_NERVOUS end
  lg.push()
  lg.translate(cx + jx, base + jy)
  lg.scale(k * breathe, k / math.sqrt(breathe))
  sprite("cake", mood, 0, 0, W, false)
  -- Bites: a handful of crumbs on the plate per heart lost.
  for i = 0, (sim.HEARTS - g.hearts) * 5 - 1 do
    local bx = (hash(i * 3 + 300) - 0.5) * 68
    local by = -7 - hash(i * 3 + 301) * 6
    local r = 1.4 + hash(i * 3 + 302) * 1.6
    color(look.SHADOW, 0.3)
    lg.circle("fill", bx, by + 0.8, r)
    color(if i % 4 == 3 then look.CAKE_FROSTING else look.CAKE_SPONGE)
    lg.circle("fill", bx, by, r)
  end
  lg.pop()
  if endK > 0 then
    -- The devouring: jellies pile on and chomp.
    local n = min(floor(endK * 1.6 * #PILE), #PILE)
    for i = 1, n do
      local p = PILE[i]
      local w = sin(time * 14 + i * 2)
      local hop = abs(sin(time * 12 + i)) * 4
      local frame = (floor(time * 8) + i) % 2 + 1
      jellyPic(cx + p[1] + jx, cy + p[2] * k + 8 - hop, sim.CREEPS[p[3]].radius, p[3], frame, p[1] > 0, w, 1)
    end
  end
end

-- ---- Effects ----------------------------------------------------------------

-- Expanding rings: { x, y, r, color, age, life }.
function draw.rings(list)
  for _, rg in list do
    local k = rg.age / rg.life
    ring(rg.x, rg.y, rg.r * (0.35 + 0.65 * quadOut(k)), 3 * (1 - k) + 1, rg.color, (rg.alpha or 0.85) * (1 - k))
  end
end

-- The red edge: the last heart's warning, and a flash on each bite.
function draw.vignette(a)
  if a <= 0.01 then return end
  color(look.DANGER, min(a, 1))
  lg.draw(img.vignette, 0, 0, 0, 360 / img.vignette:getWidth(), 640 / img.vignette:getHeight())
end

-- ---- The picker and the HUD -------------------------------------------------

-- The tower picker (an empty pad) or the upgrade bubble (a tower). `v`:
-- { g, sel, selT, denied = { k, t }, time, focus = slot the keys point at }.
function draw.picker(v)
  local g = v.g
  local function pop(i) return backOut(math.clamp((v.selT - (i - 1) * 0.04) / 0.22, 0, 1)) end
  local function shakeX(k)
    if v.denied and v.denied[1] == k then return sin(v.denied[2] * 70) * 5 * (1 - v.denied[2] / 0.4) end
    return 0
  end
  local R = look.SLOT_R
  if v.sel.what == "pad" then
    local slots = layout.pickerSlots(v.sel.pad)
    local pad = sim.PADS[v.sel.pad]
    local px, py = pad[1], pad[2]
    local a = pop(1)
    local panelX, panelY = slots[1][1] - R - 8, slots[1][2] - R - 8
    local panelW, panelH = slots[3][1] - slots[1][1] + 2 * R + 16, 2 * R + 30
    -- A tail pointing at the pad.
    local above = slots[1][2] < py
    local tailY = if above then panelY + panelH else panelY
    local dir = if above then 1 else -1
    local tx = math.clamp(px, panelX + 20, panelX + panelW - 20)
    color(look.INK, 0.85 * a)
    lg.polygon("fill", tx - 10, tailY, tx + 10, tailY, px, tailY + dir * 12)
    lg.rectangle("fill", panelX, panelY, panelW, panelH, 18)
    for k, s in slots do
      local kind = sim.KINDS[k]
      local cost = sim.TOWERS[kind].cost[1]
      local ok = g.coins >= cost
      local sc = pop(k)
      local x, y = s[1] + shakeX(k), s[2]
      lg.push()
      lg.translate(x, y)
      lg.scale(sc, sc)
      color(if ok then look.CREAM else 0x8a7a8a)
      lg.circle("fill", 0, 0, R)
      ring(0, 0, R, if v.focus == k then 4.5 else 2.5, if v.focus == k then look.WHITE elseif ok then look.TOWER[kind] else 0x5a4a5a, 1)
      draw.tower(0, 2, { kind = kind, level = 1, aim = -pi / 2, recoil = 0, age = 1 }, 0.7, v.time)
      lg.pop()
      coin(x - 13, y + R + 9, 6.5, 1)
      say(tostring(cost), 15, x + 6, y + R + 8, if ok then look.GOLD else 0xff8a9a, 1)
    end
  elseif v.sel.what == "tower" then
    local x, y = layout.upgradeSlot(v.sel.pad)
    x += shakeX(1)
    local sc = pop(1)
    lg.push()
    lg.translate(x, y)
    lg.scale(sc, sc)
    local cost = g:upgradeCost(v.sel.pad)
    if cost then
      local ok = g.coins >= cost
      color(look.INK)
      lg.circle("fill", 0, 0, R + 2)
      color(if ok then look.GOOD else 0x8a7a8a)
      lg.circle("fill", 0, 0, R)
      if v.focus then ring(0, 0, R + 2, 3, look.WHITE, 1) end
      -- An arrow up.
      color(look.WHITE)
      lg.polygon("fill", -11, -6, 11, -6, 0, -18)
      lg.rectangle("fill", -4.5, -7, 9, 11, 2)
      say(tostring(cost), 15, 0, 13, if ok then look.GOLD else 0xffb0b8, 1)
    else
      color(look.INK)
      lg.rectangle("fill", -32, -15, 64, 30, 15)
      say("MAX", 17, 0, 0, look.GOLD, 1)
    end
    lg.pop()
  end
end

-- The coin pill (top left) and the next-wave button (top right): what's
-- coming, a countdown ring, tap to call it early for the bonus under it.
function draw.hud(v)
  local g = v.g
  -- Coins.
  local x, y, w, h = layout.coinPill()
  local k = v.coinPunch
  lg.push()
  lg.translate(x + w / 2, y + h / 2)
  lg.scale(k, k)
  color(look.INK, 0.72)
  lg.rectangle("fill", -w / 2, -h / 2, w, h, h / 2)
  coin(-w / 2 + 16, 0, 9, 1)
  say(tostring(g.coins), 20, 10, 0, look.GOLD, 1)
  lg.pop()
  -- Next wave.
  x, y, w, h = layout.waveButton()
  color(look.INK, 0.7)
  lg.rectangle("fill", x, y, w, h, 16)
  if v.focus then
    color(look.WHITE)
    lg.setLineWidth(3)
    lg.rectangle("line", x - 2, y - 2, w + 4, h + 4, 18)
  end
  local n = if g.wave == 0 then 1 else g.next.n
  say("WAVE " .. n, 11, x + 34, y + 10, look.WHITE, 0.8, 1)
  for i, grp in g.next.groups do
    if i > 3 then break end
    local gx = x + 12 + (i - 1) * 25
    local r = math.clamp(sim.CREEPS[grp[1]].radius * 0.6, 6, 9)
    jellyPic(gx, y + 37, r, grp[1], 1, false, 0, 1)
    say(tostring(grp[2]), 11, gx + 9, y + 34, look.WHITE, 1, 1.2)
  end
  if g.next.newKind then
    -- A badge on the corner: a jelly that hasn't come before.
    local a = 0.75 + 0.25 * sin(v.time * 6)
    color(look.GOLD, a)
    lg.rectangle("fill", x - 6, y - 8, 38, 17, 8.5)
    say("NEW", 11, x + 13, y + 0.5, look.INK, a, 0.01)
  end
  local cx, cy = x + w - 22, y + h / 2
  local frac = if g.wave == 0 then 0 else 1 - math.clamp(g.nextIn / sim.WAVE_GAP, 0, 1)
  color(look.CREAM)
  lg.circle("fill", cx, cy, 18)
  color(0x000000, 0.19)
  lg.setLineWidth(4)
  lg.circle("line", cx, cy, 16)
  if frac > 0 then
    color(look.DANGER)
    lg.arc("line", "open", cx, cy, 16, -pi / 2, -pi / 2 + frac * 2 * pi)
  end
  color(look.INK)
  for _, o in { -4, 4 } do
    lg.polygon("fill", cx + o - 4, cy - 6, cx + o + 4, cy, cx + o - 4, cy + 6)
  end
  local bonus = g:earlyBonus()
  if bonus > 0 and g:playing() then say("+" .. bonus, 13, cx, y + h + 8, look.GOLD, 1) end
end

-- The hint that teaches a tap, where it works: a bouncing arrow onto a pad
-- with its words under it, or a pulsing ring with words above.
function draw.hint(x, y, words, onPad, t)
  local phase = (t * 1.4) % 1
  ring(x, y, 24 + 14 * phase, 3.5, look.WHITE, 1 - phase)
  local bob = quadInOut(math.clamp(sin(t * 2.8) * 0.5 + 0.5, 0, 1)) * 8
  local tx = math.clamp(x, 80, 280)
  if onPad then
    local ay = y - 44 - bob
    local function arrow(grow, dy, h, a)
      local top, neck, tip = ay - 20 - grow + dy, ay + 2 + dy, ay + 18 + grow + dy
      color(h, a)
      lg.rectangle("fill", x - 6 - grow, top, 12 + 2 * grow, neck - top + 2, 4 + grow)
      lg.polygon("fill", x - 15 - grow * 1.6, neck, x + 15 + grow * 1.6, neck, x, tip)
    end
    arrow(3, 2, look.INK, 0.35)
    arrow(3, 0, look.INK, 1)
    arrow(0, 0, look.WHITE, 1)
    say(words, 17, tx, y + 42, look.WHITE, 1)
  else
    say(words, 17, tx, y - 46 - bob * 0.4, look.WHITE, 1)
  end
end

-- A big centred line ("WAVE 4", "HELMETS!"), popping in and fading.
function draw.banner(words, age, big)
  local k = backOut(min(age / 0.3, 1))
  local a = if age > 1.2 then 1 - (age - 1.2) / 0.4 else 1
  if a <= 0 then return end
  lg.push()
  lg.translate(180, 300)
  lg.scale(k, k)
  say(words, if big then 36 else 30, 0, 0, if big then look.GOLD else look.WHITE, a, if big then 3 else 2.5)
  lg.pop()
end

-- The key and pad cursor: a bracket around what Space or A would press.
function draw.cursor(x, y, w, h, t)
  local p = 2 + sin(t * 6) * 1.5
  color(look.GOLD)
  lg.setLineWidth(3)
  local l, r, tp, b = x - w / 2 - p, x + w / 2 + p, y - h / 2 - p, y + h / 2 + p
  local c = 10
  lg.line(l, tp + c, l, tp, l + c, tp)
  lg.line(r - c, tp, r, tp, r, tp + c)
  lg.line(l, b - c, l, b, l + c, b)
  lg.line(r - c, b, r, b, r, b - c)
end

-- Under the game-over card: how far the round got.
function draw.reached(wave, a)
  if a <= 0 then return end
  local text = string.format(look.REACHED, max(wave, 1))
  local f = fonts[17]
  local w = f:getWidth(text) + 36
  local y = 604
  color(look.INK, 0.72 * a)
  lg.rectangle("fill", 180 - w / 2, y - 15, w, 30, 15)
  say(text, 17, 180, y, look.WHITE, a, 1.2)
end

return draw
