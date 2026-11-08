-- Poppets, the match3 starter: a swap-to-match puzzle for one thumb. A 7×8
-- board of little painted poppets in front of a bedroom window at night.
-- Swipe a poppet into its neighbour's place (or tap one, then a neighbour)
-- to line up three of a kind; they pop, the rest fall, cascades multiply.
-- A swap that matches nothing bounces back. Keys and controllers play too:
-- the arrows (or WASD, or the d-pad) move a cursor, Space, Enter or A picks
-- a poppet up, a direction swaps it, B or Escape puts it down.
--
-- Popping keeps everyone awake: the wake meter drains faster and faster
-- while the board waits for you, and when it's empty the whole board nods
-- off. Every 15 s a level brings something new (faster, a sixth poppet,
-- rocks, star poppets). 4 in a row makes a line blaster, an L or T a bomb,
-- 5 in a row a rainbow; swap two specials together for a combo.
--
-- Make it yours:
-- - The rules and their knobs (drain, refills, levels, timings) are in
--   game/sim.lua, knobs at the top. What each level brings is sim.news.
-- - Words, colours, the board's place and which picture is which poppet
--   are in game/look.lua; the art is images/ (the cast in images/cast/,
--   the window bg.png). Faces, specials and effects are drawn in code in
--   game/draw.lua.
-- - Sounds and the mix are game/sound.lua (sounds/, music/lullaby.ogg).
-- - This file is the feel: input, juice, the title card's demo, the HUD.

local sim = require("sim")
local look = require("look")
local draw = require("draw")
local sound = require("sound")

-- ---- Feel -------------------------------------------------------------------
-- The swipe hint comes back after this long without a touch.
local HINT_IDLE = 3.0
-- A drag this far (units) is a swipe.
local SWIPE_MIN = 14
-- Title card: how long the poppets doze before the demo makes a move.
local DEMO_WAIT = 2.4

local CELL, BX, BY = look.CELL, look.BOARD_X, look.BOARD_Y
local COLS, ROWS = sim.COLS, sim.ROWS
local col, row, cell = sim.col, sim.row, sim.cell
local sin, cos, abs, min, max, floor, pi = math.sin, math.cos, math.abs, math.min, math.max, math.floor, math.pi
local cellCenter = draw.cellCenter

local game -- the sim (game/sim.lua)
local playT, idle = 0, 0
-- The player has made a match this round (the swipe hint stops showing on
-- its own). Every round teaches again: never remembered.
local taught = false
local touch -- { cell, x0, y0, done }: a finger down on a poppet
local selected -- a picked-up poppet's cell
local nudge -- { cell, t }: a rock poked wobbles
local banner -- { text, t, piece }
local later = {} -- puffs waiting for their piece to pop (blasts pop in a wave)
local demoWait = 0
-- What the faces and the sky show (eased): 0 awake … 1 asleep.
local sleep = 0
local meterShown, meterFlash = 1, 0
-- The key and pad cursor, shown once a key or a pad is used.
local cursor = { c = 3, r = 4, shown = false }
local stickX, stickY = 0, 0
local loggedTurn = 0 -- the last turn logged (DEBUG_HINT)
local applied = false

local function playing()
  return not round.title and not round.ended
end

-- ---- Debug: scenarios (debug/scenarios.toml) and the menu's own items -------
-- Set by a scenario once love.load has run; love.update applies them once.
LEVEL = 1 -- start at this level, with everything it brings
METER = 1 -- the wake meter, 0…1
SPECIALS = false -- a line blaster, a bomb and a rainbow on the board
AUTOPLAY = false -- make the hinted move after AUTOPLAY_WAIT seconds without a touch
AUTOPLAY_WAIT = 0.8
DEBUG_HINT = false -- log each turn's hinted move (for scripted play tests)

local function poppetCells()
  local out = {}
  for i = 1, sim.N do
    local p = game.board[i]
    if p and p.kind == "poppet" and not p.special then out[#out + 1] = i end
  end
  return out
end

local function make(kind, special)
  local cells = poppetCells()
  if #cells > 0 and game:ready() then
    game:place(cells[love.math.random(#cells)], kind, special)
  end
end

debug.action("Make a rainbow", function() make("rainbow", nil) end)
debug.action("Make a bomb", function() make("poppet", "bomb") end)
debug.action("Make a line blaster", function() make("poppet", love.math.random() < 0.5 and "lineH" or "lineV") end)
debug.action("Fill the meter", function() game.energy = 1 end)
debug.action("Nearly asleep", function() game.energy = 0.12 end)
debug.action("Next level", function() game:skipTo(game.level + 1) end)
debug.toggle("Autoplay", function(on) AUTOPLAY = on end, AUTOPLAY)

local function applyScenario()
  applied = true
  if LEVEL > 1 then game:skipTo(LEVEL) end
  game.energy = math.clamp(METER, 0, 1)
  meterShown = game.energy
  if SPECIALS then
    -- Specials side by side in the middle, so swapping two is a combo.
    game:place(cell(2, 4), "poppet", "lineH")
    game:place(cell(3, 4), "poppet", "bomb")
    game:place(cell(4, 4), "rainbow", nil)
    game:place(cell(3, 5), "poppet", "lineV")
  end
end

-- ---- The round --------------------------------------------------------------

function love.load()
  draw.load()
  sound.load()
  -- The title card's board plays itself (no drain, no levels).
  game = sim.new(love.math.random(1, 2147483646), round.title)
  if not round.title then sound.start() end
  -- The engine's HUD shows the score and the best; the wake meter and the
  -- level pill are the game's own (draw.meter, draw.pill).
  ui.hud({ score = true, best = true, level = false })
end

local function cellAt(x, y)
  local fx, fy = (x - BX) / CELL, (y - BY) / CELL
  if fx >= 0 and fy >= 0 and fx < COLS and fy < ROWS then return cell(floor(fx), floor(fy)) end
  return nil
end

local function isRock(i)
  local p = game.board[i]
  return p and p.kind == "rock"
end

local function swap(a, b)
  selected, touch, idle = nil, nil, 0
  if not game:swap(a, b) then
    nudge = { isRock(a) and a or b, 0 }
    sound.rockPoke()
  end
end

-- A press in play: picks a poppet up, or swaps the one picked up with this neighbour.
local function press(x, y)
  idle = 0
  if game.phase == "asleep" then return end
  local c = cellAt(x, y)
  if not c then
    selected = nil
    return
  end
  if selected and sim.adjacent(selected, c) then
    swap(selected, c)
    return
  end
  if isRock(c) then
    nudge, selected = { c, 0 }, nil
    sound.rockPoke()
    return
  end
  selected = selected ~= c and c or nil
  touch = { cell = c, x0 = x, y0 = y, done = false }
  sound.select()
end

local function drag(x, y)
  if not touch or touch.done then return end
  local dx, dy = x - touch.x0, y - touch.y0
  if max(abs(dx), abs(dy)) < SWIPE_MIN then return end
  touch.done = true
  local c = touch.cell
  local tc, tr = col(c), row(c)
  if abs(dx) > abs(dy) then tc += dx > 0 and 1 or -1 else tr += dy > 0 and 1 or -1 end
  if tc >= 0 and tr >= 0 and tc < COLS and tr < ROWS then
    swap(c, cell(tc, tr))
  else
    selected = nil
  end
end

-- Input counts once the round is under way: the press that started it
-- deals a fresh board and picks nothing up, so the hint shows at once.
local function live()
  return playing() and playT > 0.1
end

function love.mousepressed(x, y, button)
  if live() and button == 1 then press(x, y) end
end

function love.mousemoved(x, y)
  if playing() then drag(x, y) end
end

function love.mousereleased()
  touch = nil
end

-- Keys and pads: the cursor moves, or swaps the poppet it holds that way.
local function cursorMove(dc, dr)
  cursor.shown, idle = true, 0
  local c, r = math.clamp(cursor.c + dc, 0, COLS - 1), math.clamp(cursor.r + dr, 0, ROWS - 1)
  if selected then
    if c ~= cursor.c or r ~= cursor.r then swap(selected, cell(c, r)) end
    selected = nil
  end
  cursor.c, cursor.r = c, r
end

local function cursorPick()
  cursor.shown, idle = true, 0
  if game.phase == "asleep" then return end
  local c = cell(cursor.c, cursor.r)
  if selected then
    selected = nil
  elseif isRock(c) then
    nudge = { c, 0 }
    sound.rockPoke()
  else
    selected = c
    sound.select()
  end
end

local function cursorDrop()
  cursor.shown = true
  selected = nil
end

local KEYS = { left = { -1, 0 }, a = { -1, 0 }, right = { 1, 0 }, d = { 1, 0 }, up = { 0, -1 }, w = { 0, -1 }, down = { 0, 1 }, s = { 0, 1 } }
local PADS = { dpleft = { -1, 0 }, dpright = { 1, 0 }, dpup = { 0, -1 }, dpdown = { 0, 1 } }

function love.keypressed(key)
  if not live() then return end
  local d = KEYS[key]
  if d then
    cursorMove(d[1], d[2])
  elseif key == "space" or key == "return" or key == "z" then
    cursorPick()
  elseif key == "escape" or key == "x" or key == "backspace" then
    cursorDrop()
  end
end

function love.gamepadpressed(_, button)
  if not live() then return end
  local d = PADS[button]
  if d then
    cursorMove(d[1], d[2])
  elseif button == "a" then
    cursorPick()
  elseif button == "b" then
    cursorDrop()
  end
end

-- The stick steps the cursor once each time it's pushed past half way.
function love.gamepadaxis(_, axis, value)
  if not live() then return end
  local v = value > 0.5 and 1 or (value < -0.5 and -1 or 0)
  if axis == "leftx" then
    if v ~= 0 and v ~= stickX then cursorMove(v, 0) end
    stickX = v
  elseif axis == "lefty" then
    if v ~= 0 and v ~= stickY then cursorMove(0, v) end
    stickY = v
  end
end

-- ---- Juice ------------------------------------------------------------------

local CENTER_X, CENTER_Y = cellCenter(COLS / 2 - 0.5, ROWS / 2 - 0.5)
local SHAKE = { row = { 0.3, 0.04 }, col = { 0.3, 0.04 }, area = { 0.45, 0.06 }, cross = { 0.45, 0.06 }, zap = { 0.35, 0.05 }, board = { 0.8, 0.1 } }
local MADE_WORD = { lineH = "BLASTER!", lineV = "BLASTER!", bomb = "BOMB!" }

-- The kit's shake is "trauma" (0…1); fx.shake is units and seconds.
local function shake(trauma)
  fx.shake(14 * min(trauma, 1), 0.2 + 0.25 * min(trauma, 1))
end

local function react(cues)
  local live = playing()
  for _, c in cues do
    local k = c.kind
    if k == "pop" then
      local st = game.last
      local n = #st.popping
      for _, p in st.popping do
        local x, y = cellCenter(col(p.at), row(p.at))
        local color = 0xffffff
        if p.piece.kind == "poppet" then color = look.PIECES[p.piece.color] elseif p.piece.kind == "rock" then color = look.ROCK_COLOR end
        later[#later + 1] = { t = p.delay + 0.08, x = x, y = y, color = color, big = n < 12 or sim.isSpecial(p.piece) }
      end
      if live then
        for _, g in st.groups do
          local x, y = cellCenter(g.x, g.y)
          fx.popup("+" .. g.points, x, y - 6, { color = look.GOLD, size = 20 })
        end
        if c.chain >= 2 then
          fx.popup("×" .. c.chain .. " CASCADE!", CENTER_X, BY + 26, { color = look.GOLD, size = 24 + 3 * min(c.chain, 6) })
          shake(0.08 * min(c.chain, 5))
        end
        taught = true
      end
      meterFlash = 1
      demoWait = 0
    elseif k == "fire" then
      local s = SHAKE[c.blast]
      shake(s[1])
      fx.hitstop(s[2])
    elseif k == "combo" then
      fx.flash({ 1, 1, 1, 0.44 }, 0.35)
      if live then fx.popup("COMBO!", CENTER_X, CENTER_Y, { color = 0xffffff, size = 34 }) end
    elseif k == "made" then
      local x, y = cellCenter(col(c.at), row(c.at))
      fx.burst("ring", x, y, { count = 1, color = 0xffffff })
      fx.burst("sparkles", x, y, { count = 8, color = 0xfff2b0 })
      local word = c.piece.kind == "rainbow" and "RAINBOW!" or MADE_WORD[c.piece.special]
      if live then fx.popup(word, x, y + 24, { color = 0xffffff, size = 18 }) end
    elseif k == "star" then
      local x, y = cellCenter(col(c.at), row(c.at))
      fx.burst("stars", x, y, { count = 10, color = look.GOLD })
      if live then fx.popup("WIDE AWAKE!", x, y - 30, { color = look.GOLD, size = 18 }) end
    elseif k == "level" then
      local piece
      if c.news == "newColor" then
        piece = { kind = "poppet", color = game.colors, id = 7 }
      elseif c.news == "rocks" or c.news == "moreRocks" then
        piece = { kind = "rock", id = 7 }
      elseif c.news == "stars" then
        piece = { kind = "poppet", color = 3, star = true, id = 7 }
      end
      banner = { text = look.NEWS[c.news], t = 0, piece = piece }
    elseif k == "shuffle" then
      banner = { text = "SHUFFLE!", t = 0 }
    elseif k == "phew" then
      if live then fx.popup("PHEW!", CENTER_X, CENTER_Y - 40, { color = 0x8ff7c0, size = 30 }) end
    elseif k == "asleep" then
      shake(0.5)
      selected = nil
    end
  end
end

-- Title card: the poppets nod off, then a move wakes them (and again).
local function autopilot(dt)
  if not game:ready() then return end
  demoWait += dt
  if demoWait > DEMO_WAIT and game.hint then
    game:swap(game.hint[1], game.hint[2])
    demoWait = 0
  end
end

function love.update(dt)
  if not applied then applyScenario() end
  idle += dt
  if round.title then
    autopilot(dt)
  elseif playing() then
    playT += dt
    if AUTOPLAY and game:ready() and idle > AUTOPLAY_WAIT and game.hint then swap(game.hint[1], game.hint[2]) end
  end
  game:step(dt)
  local cues = game.cues
  game.cues = {}
  react(cues)
  sound.cues(cues)
  if game.over and playing() then
    round.score = game.score
    -- A new best gets the music box's fanfare as the card comes up.
    local best = save.get("best") or 0
    if game.score > best then
      save.set("best", game.score)
      sound.best()
    end
    round.over(look.HEADING, "asleep")
  end

  -- What the faces and the sky show.
  local target
  if round.ended then
    target = 1
  elseif round.title then
    local k = math.clamp(demoWait / DEMO_WAIT, 0, 1)
    target = game:ready() and 0.85 * k * k or 0
  elseif game.phase == "asleep" then
    target = min(game.pt / 0.8, 1)
  else
    target = game:drowsiness()
  end
  local rate = target < sleep and 6 or 2.5
  sleep += (target - sleep) * (1 - math.exp(-dt * rate))
  if game.energy > meterShown then
    meterShown += (game.energy - meterShown) * (1 - math.exp(-dt * 5))
  else
    meterShown = game.energy
  end
  meterFlash = max(0, meterFlash - dt * 3)
  -- The drone follows how sleepy they are.
  sound.tension(game:drowsiness() * 1.4)
  sound.update(dt, playing() and game.energy < sim.DROWSY and game.phase ~= "asleep")

  for i = #later, 1, -1 do
    local f = later[i]
    f.t -= dt
    if f.t <= 0 then
      table.remove(later, i)
      fx.burst("burst", f.x, f.y, { count = f.big and 10 or 4, color = f.color })
      if f.big then fx.burst("sparkles", f.x, f.y, { count = 3, color = 0xfff2b0 }) end
    end
  end
  if nudge then
    nudge[2] += dt
    if nudge[2] > 0.3 then nudge = nil end
  end
  if banner then
    banner.t += dt
    if banner.t > 1.8 then banner = nil end
  end
  if playing() then round.score = game.score end
  if DEBUG_HINT and game.hint and game.settles ~= loggedTurn then
    loggedTurn = game.settles
    print(("hint %d,%d -> %d,%d at %.2f"):format(col(game.hint[1]), row(game.hint[1]), col(game.hint[2]), row(game.hint[2]), love.timer.getTime()))
  end
end

-- ---- Drawing ----------------------------------------------------------------

local function backOut(k)
  local c1 = 1.70158
  local f = k - 1
  return 1 + (c1 + 1) * f * f * f + c1 * f * f
end

local function quadInOut(k)
  if k < 0.5 then return 2 * k * k end
  return 1 - (-2 * k + 2) ^ 2 / 2
end

-- Where the piece in cell i is drawn: x, y, squash x, squash y, alpha.
local function piecePos(i, p)
  local c, r = col(i), row(i)
  local bx, by = cellCenter(c, r)
  local ph = game.phase
  if ph == "swap" or ph == "bounce" then
    local a, b = game.pa, game.pb
    if i == a or i == b then
      local k = quadInOut(math.clamp(game.pt / sim.SWAP_TIME, 0, 1))
      if ph == "bounce" then k = 1 - k end
      local o = i == a and b or a
      local ox, oy = cellCenter(col(o), row(o))
      return bx + (ox - bx) * k, by + (oy - by) * k, i == a and 1 + 0.1 * sin(k * pi) or 1, 1, 1
    end
  elseif ph == "fall" then
    local yc, landed = sim.fallY(p.fromY, r, game.pt)
    local _, y = cellCenter(c, yc)
    local sx, sy = 1, 1
    if p.fromY < r - 0.01 then
      if landed < 0 then
        sx, sy = 0.94, 1.08
      elseif landed < 0.16 then
        local s = sin(landed / 0.16 * pi) * 0.16
        sx, sy = 1 + s, 1 - s
      end
    end
    return bx, y + (1 - sy) * CELL * 0.3, sx, sy, math.clamp((yc + 1) * 1.5, 0, 1)
  elseif ph == "shuffle" then
    local k = math.clamp(game.pt / sim.SHUFFLE_TIME, 0, 1)
    -- Back in and out: they gather, then fly to their new places.
    local c2 = 1.70158 * 1.525
    local e
    if k < 0.5 then e = ((2 * k) ^ 2 * ((c2 + 1) * 2 * k - c2)) / 2 else e = ((2 * k - 2) ^ 2 * ((c2 + 1) * (k * 2 - 2) + c2) + 2) / 2 end
    local fx0, fy0 = cellCenter(p.fromX, p.fromY)
    local s = 1 - 0.3 * sin(e * pi)
    return fx0 + (bx - fx0) * e, fy0 + (by - fy0) * e, s, s, 1
  elseif ph == "asleep" then
    -- Slumping: sag a little, one by one.
    local k = math.clamp((game.pt * 1.6 - r * 0.1 - c * 0.04) / 0.5, 0, 1)
    return bx, by + 4 * k, 1 + 0.06 * k, 1 - 0.1 * k, 1
  end
  if nudge and nudge[1] == i then
    return bx + 5 * sin(nudge[2] * 40) * max(0, 1 - nudge[2] / 0.3), by, 1, 1, 1
  end
  return bx, by, 1, 1, 1
end

-- How the face in cell i looks: a mood and how drowsy (draw.piece's).
local function mood(i, p, t)
  if round.ended then return "asleep", 1 end
  local ph = game.phase
  if ph == "asleep" then
    local k = game.pt * 1.6 - row(i) * 0.1 - col(i) * 0.04
    if k > 0.35 then return "asleep", 1 end
    return "sleepy", 0.7 + k
  elseif (ph == "swap" or ph == "bounce") and (i == game.pa or i == game.pb) then
    return ph == "swap" and "excited" or "huh", 0
  end
  if selected == i then return "excited", 0 end
  if sleep > 0.02 then
    -- Each one nods at its own pace.
    return "sleepy", sleep * (0.85 + 0.15 * sin(t * 1.3 + p.id * 0.9))
  end
  return "happy", 0
end

-- Where the face in cell i looks: at the picked-up poppet, or wandering.
local function gaze(i, p, t)
  if selected then
    local dx, dy = col(selected) - col(i), row(selected) - row(i)
    local l = math.sqrt(dx * dx + dy * dy)
    if l > 0 then return dx / l, dy / l end
  end
  return sin(t * 0.6 + p.id * 1.7), cos(t * 0.45 + p.id * 2.3) * 0.6
end

local RADIUS = CELL * 0.43

local function drawPiece(i, p, radius, t)
  local x, y, sx, sy, a = piecePos(i, p)
  if playing() then
    -- The board pops in cell by cell as the round starts.
    local k = math.clamp((playT - (col(i) + row(i)) * 0.015) / 0.22, 0, 1)
    if k < 1 then radius *= backOut(k) end
  end
  local m, d = mood(i, p, t)
  local lx, ly = gaze(i, p, t)
  draw.piece(p, x, y, radius, sx, sy, m, d, lx, ly, t, a)
end

local function drawBoard(t)
  local ph = game.phase
  local lifted = (ph == "swap" or ph == "bounce") and game.pa or nil
  for i = 1, sim.N do
    local p = game.board[i]
    if p and i ~= selected and i ~= lifted then drawPiece(i, p, RADIUS, t) end
  end
  if lifted and game.board[lifted] then drawPiece(lifted, game.board[lifted], RADIUS * 1.06, t) end
  if selected and game.board[selected] then
    local x, y = cellCenter(col(selected), row(selected))
    draw.selectGlow(x, y, t)
    drawPiece(selected, game.board[selected], RADIUS * 1.12, t)
  end
  -- Popping pieces: swell, squeeze, vanish (or fly into the special they made).
  if ph == "pop" then
    local pt = game.pt
    for _, pp in game.last.popping do
      local local_t = pt - pp.delay
      local bx, by = cellCenter(col(pp.at), row(pp.at))
      if local_t < 0 then
        local jig = 1.5 * sin(t * 50 + col(pp.at))
        draw.piece(pp.piece, bx + jig, by, RADIUS, 1, 1, "excited", 0, 0, 0, t, 1)
      else
        local k = math.clamp(local_t / sim.POP_TIME, 0, 1)
        if pp.to then
          local tx, ty = cellCenter(col(pp.to), row(pp.to))
          local e = k * k
          draw.piece(pp.piece, bx + (tx - bx) * e, by + (ty - by) * e, RADIUS * (1 - 0.5 * e), 1, 1, "squeeze", 0, 0, 0, t, 1 - 0.6 * e)
        else
          local s
          if k < 0.35 then
            local q = k / 0.35
            s = 1 + 0.4 * (1 - (1 - q) * (1 - q))
          else
            local q = (k - 0.35) / 0.65
            s = 1.4 * (1 - q * q)
          end
          draw.piece(pp.piece, bx, by, RADIUS * s, 1, 1, "squeeze", 0, 0, 0, t, 1 - k * 0.5)
        end
      end
    end
    for _, b in game.last.blasts do draw.blast(b, pt - b.delay) end
  end
end

local function drawBanner(t)
  if not banner then return end
  local k = backOut(min(banner.t / 0.3, 1))
  local a = banner.t > 1.5 and 1 - (banner.t - 1.5) / 0.3 or 1
  if a <= 0 then return end
  local y = BY + CELL * 3.6
  draw.banner(banner.text, y, k * (0.9 + 0.1 * a), a)
  if banner.piece then
    local bob = sin(t * 6) * 4
    draw.piece(banner.piece, 180, y - 62 + bob, CELL * 0.62 * k, 1, 1, "excited", 0, 0, 0.3, t, a)
  end
end

function love.draw()
  local t = love.timer.getTime()
  draw.sky(sleep, t)
  draw.board(sleep)
  drawBoard(t)
  if game.phase == "asleep" or round.ended then draw.zzz(t, sleep) end
  if playing() then
    local _, sy = love.window.getSafeArea()
    draw.meter(180, sy + 112, 220, game.energy, meterShown, meterFlash, t)
    local sx, _, sw = love.window.getSafeArea()
    draw.pill("LV " .. game.level, sx + sw - 40, sy + 28)
    local hint = game:ready() and not touch and not selected and ((not taught and playT > 0.5) or idle > HINT_IDLE)
    if hint and game.hint then
      local strength = 1
      if taught then strength = math.clamp((idle - HINT_IDLE) / 0.4, 0, 1) end
      local ax, ay = cellCenter(col(game.hint[1]), row(game.hint[1]))
      local bx, by = cellCenter(col(game.hint[2]), row(game.hint[2]))
      draw.swipeHint(ax, ay, bx, by, t, strength)
      if not taught then draw.hintText(t) end
    end
    if cursor.shown then draw.cursor(cursor.c, cursor.r, selected ~= nil, t) end
    drawBanner(t)
  end
end
