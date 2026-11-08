-- Cake Keep, the tower-defense starter: angry jellies march down a winding
-- clay road toward a cake with a face. Tap a pad beside the road, then a
-- tower in the picker to build it (POP gumballs, CHILL ice cream that
-- slows, BOOM cherry bombs that splash); tap a tower, then the green arrow,
-- to upgrade it (levels 2 and 3). Popped jellies pay coins. Waves come every
-- 10 s whatever you do; the button top right shows what's next and calls it
-- early for bonus coins. Every jelly that reaches the cake takes a bite (a
-- heart; a king takes three). Five bites and the cake is devoured.
--
-- Keys and controllers play too: the arrows (or WASD, the d-pad, the
-- stick) move a cursor between the pads and the next-wave button, Space,
-- Enter or A presses what it's on (left and right pick a tower in the
-- picker), Escape or B closes it, N or Y calls the next wave.
--
-- Make it yours:
-- - The rules and their knobs (coins, waves, hp growth, the towers' and
--   jellies' numbers, which jellies come in which wave, the map's road and
--   pads) are in game/sim.lua, knobs at the top. How the title card (and
--   the bot) build is game/planner.lua.
-- - Words, colours and which picture is what are in game/look.lua; the
--   art is images/ (the map with its road baked in, so a new road needs a
--   new map.jpg). Shadows, shots, rings, the picker and the HUD bits are
--   drawn in code in game/draw.lua; where the buttons are, game/layout.lua.
-- - Sounds and the mix are game/sound.lua (sounds/, music/: the march and
--   its urgent twin).
-- - This file is the feel: input, juice, hints, the title card's game.

local sim = require("sim")
local planner = require("planner")
local look = require("look")
local layout = require("layout")
local draw = require("draw")
local sound = require("sound")
local shared = require("shared")

-- ---- Feel -------------------------------------------------------------------
-- Hints come back after this long without a tap.
local HINT_IDLE = 3.0
-- The title card's autopilot: seconds between its moves, and how it builds
-- (few enough towers that the cake falls now and then, in the lower half
-- so the fight happens below the title).
local ATTRACT_THINK = 1.6
local ATTRACT_STYLE = { maxTowers = 5, padsBelow = 280, upgrades = true, callEarly = false }
-- The debug Autopilot plays like a good player.
local AUTO_STYLE = { maxTowers = 99, padsBelow = 0, upgrades = true, callEarly = true }
-- A popped jelly's burst lasts this long; a banner this long.
local SPLAT_LIFE = 0.32
local BANNER_LIFE = 1.6

local PADS = sim.PADS
local WAVE_TARGET = #PADS + 1 -- the cursor's index for the next-wave button
local abs, min, max, sin, floor = math.abs, math.min, math.max, math.sin, math.floor

local game -- the round (game/sim.lua)
local time, playT, idle = 0, 0, 0
local sel = { what = "none", pad = 0 } -- "pad": the picker is open; "tower": its upgrade bubble
local selT = 0
local denied -- { which button, seconds }: a picker button that refused (too poor)
-- The build and upgrade hints show every round, gone once done.
local taughtBuild, taughtUpgrade = false, false
local banner -- { text, age, big }
local splats, rings = {}, {}
local wince, coinPunch, hurtFlash = 0, 0, 0
local attractT = 0
local ended = false -- the devouring has been reported
local overAt = 0 -- when the game-over card came up
-- The key and pad cursor, shown once a key or a pad is used: on a pad, or
-- the next-wave button; `slot` is the picker's tower it points at.
local cursor = { at = sim.HINT_PAD, slot = 1, shown = false }
local stickX, stickY = 0, 0
local order = {} -- jellies in drawing order (reused)
local applied = false

local function playing()
  return not round.title and not round.ended
end

-- Input counts once the round is under way (the press that started it
-- starts it, and the hint shows at once).
local function live()
  return playing() and playT > 0.1
end

-- ---- Debug: scenarios (debug/scenarios.toml) and the menu's own items -------
-- Set by a scenario once love.load has run; love.update applies them once.
AUTO = false -- the round plays itself, as a good player builds
WAVE = 0 -- start just before this wave, with a defense built to match
HEARTS = 0 -- start with this many hearts (0: all)
COINS = 0 -- extra coins

-- The title card's game: already under way (three towers, a couple of
-- waves on the road) so the card shows the action from its first frame.
local function attractGame(seed, warm)
  local g = sim.new(seed)
  g.coins += 150
  g:build(7, "pop")
  g:build(11, "boom")
  g:build(8, "chill")
  g.coins = 0
  for _ = 1, warm do g:step(1 / 30) end
  g:callWave()
  for _ = 1, 90 do g:step(1 / 30) end
  table.clear(g.cues)
  return g
end

-- A defense built as the planner would, `coins` worth.
local function prebuild(g, coins)
  g.coins = coins
  for _ = 1, 40 do
    local a = planner.want(g, { maxTowers = 99, padsBelow = 0, upgrades = true, callEarly = false })
    if not a or planner.cost(g, a) > g.coins then break end
    planner.apply(g, a)
  end
end

local function applyScenario()
  applied = true
  if round.title then return end
  if WAVE > 1 then
    -- What a decent player has by then: about the waves' paydays and bounties.
    prebuild(game, 60 + WAVE * 70)
    for t = 1, #PADS do
      if game.towers[t] then game.towers[t].age = 1 end
    end
    game.coins = 40
    game.wave = WAVE - 1
    game.next = sim.planWave(WAVE, game.rng)
    game.nextIn = 2.5
    taughtBuild, taughtUpgrade = true, true
  end
  if HEARTS > 0 then game.hearts = math.clamp(HEARTS, 1, sim.HEARTS) end
  game.coins += COINS
  table.clear(game.cues)
end

debug.action("Give 200 coins", function() game.coins += 200 end)
debug.action("Call the next wave", function() game:callWave() end)
debug.action("Lose a heart", function()
  if game.hearts > 1 then game.hearts -= 1 end
end)
debug.toggle("Autopilot", function(on) AUTO = on end, AUTO)

-- ---- The round --------------------------------------------------------------

function love.load()
  draw.load()
  sound.load()
  local seed = love.math.random(1, 2147483646)
  if round.title then
    game = attractGame(seed, 270)
  else
    game = sim.new(seed)
    sound.start()
  end
  -- The engine's HUD: the score (jellies popped), the best and the hearts
  -- (studio.toml's lives = sim.HEARTS). Coins and the next wave are the game's.
  ui.hud({ score = true, best = true, lives = true })
end

local function select(what, pad)
  if what ~= sel.what or pad ~= sel.pad then
    sel = { what = what, pad = pad or 0 }
    selT = 0
    denied = nil
  end
end

local function addRing(x, y, r, c, life, alpha)
  rings[#rings + 1] = { x = x, y = y, r = r, color = c, age = 0, life = life or 0.45, alpha = alpha }
end

local function inside(x, y, rx, ry, rw, rh, grace)
  return x >= rx - grace and x <= rx + rw + grace and y >= ry - grace and y <= ry + rh + grace
end

local function callWave()
  local bonus = game:callWave()
  if bonus and bonus > 0 then
    local bx, by, bw, bh = layout.waveButton()
    fx.popup("+" .. bonus, bx + bw / 2, by + bh + 8, { color = look.GOLD, size = 22 })
    coinPunch = max(coinPunch, 0.3)
  end
  select("none")
end

local function build(pad, k)
  if game:build(pad, sim.KINDS[k]) then
    taughtBuild = true
    select("none")
  else
    denied = { k, 0 }
  end
end

local function upgrade(pad)
  if game:upgrade(pad) then
    taughtUpgrade = true
    select("none")
  elseif game:upgradeCost(pad) then
    denied = { 1, 0 }
  end
end

-- A tap in play. Every tap gets a visible answer.
local function tap(x, y)
  idle = 0
  if not game:playing() then return end
  -- A soft ripple wherever the finger lands.
  addRing(x, y, 22, look.WHITE, 0.35, 0.55)
  local bx, by, bw, bh = layout.waveButton()
  if inside(x, y, bx, by, bw, bh, 4) then
    callWave()
    return
  end
  if sel.what == "pad" then
    for k, s in layout.pickerSlots(sel.pad) do
      local dx, dy = x - s[1], y - s[2]
      if dx * dx + dy * dy <= (look.SLOT_R + 6) ^ 2 then
        build(sel.pad, k)
        return
      end
    end
  elseif sel.what == "tower" then
    local ux, uy = layout.upgradeSlot(sel.pad)
    local dx, dy = x - ux, y - uy
    if dx * dx + dy * dy <= (look.SLOT_R + 8) ^ 2 then
      upgrade(sel.pad)
      return
    end
  end
  local p = sim.padAt(x, y)
  if p then
    local want = if game.towers[p] then "tower" else "pad"
    if want == sel.what and p == sel.pad then
      select("none")
      sound.ui(false)
    else
      select(want, p)
      cursor.slot = if taughtBuild then table.find(sim.KINDS, planner.nextKind(game)) or 1 else 1
      sound.ui(true)
    end
  else
    if sel.what ~= "none" then sound.ui(false) end
    select("none")
  end
end

function love.mousepressed(x, y, button)
  if button ~= 1 or not live() then return end
  shared.presses += 1
  shared.pressedAt = round.time
  cursor.shown = false
  tap(x, y)
end

-- ---- Keys and pads ----------------------------------------------------------

local function targetPos(i)
  if i == WAVE_TARGET then
    local x, y, w, h = layout.waveButton()
    return x + w / 2, y + h / 2
  end
  return PADS[i][1], PADS[i][2]
end

-- Moves the cursor to the nearest pad (or the next-wave button) that way.
local function cursorMove(dx, dy)
  idle = 0
  if not cursor.shown then
    cursor.shown = true
    return
  end
  if sel.what == "pad" and dy == 0 then
    -- In the picker, left and right pick a tower.
    cursor.slot = math.clamp(cursor.slot + dx, 1, 3)
    return
  end
  local x0, y0 = targetPos(cursor.at)
  local best, bestScore
  for i = 1, WAVE_TARGET do
    if i ~= cursor.at then
      local x, y = targetPos(i)
      local along = (x - x0) * dx + (y - y0) * dy
      local across = abs((x - x0) * dy - (y - y0) * dx)
      if along > 4 and across <= along * 1.6 then
        local score = along + across * 2
        if not best or score < bestScore then best, bestScore = i, score end
      end
    end
  end
  if best then
    cursor.at = best
    if sel.what ~= "none" then select("none") end
  end
end

local function cursorPress()
  idle = 0
  cursor.shown = true
  if not game:playing() then return end
  if sel.what == "pad" then
    build(sel.pad, cursor.slot)
  elseif sel.what == "tower" then
    upgrade(sel.pad)
  elseif cursor.at == WAVE_TARGET then
    callWave()
  else
    local x, y = targetPos(cursor.at)
    tap(x, y)
  end
end

local function cursorBack()
  cursor.shown = true
  if sel.what ~= "none" then sound.ui(false) end
  select("none")
end

local KEYS = { left = { -1, 0 }, a = { -1, 0 }, right = { 1, 0 }, d = { 1, 0 }, up = { 0, -1 }, w = { 0, -1 }, down = { 0, 1 }, s = { 0, 1 } }
local PAD_DIRS = { dpleft = { -1, 0 }, dpright = { 1, 0 }, dpup = { 0, -1 }, dpdown = { 0, 1 } }

function love.keypressed(key)
  if not live() then return end
  local d = KEYS[key]
  if d then
    cursorMove(d[1], d[2])
  elseif key == "space" or key == "return" or key == "z" then
    cursorPress()
  elseif key == "escape" or key == "x" or key == "backspace" then
    cursorBack()
  elseif key == "n" then
    callWave()
  end
end

function love.gamepadpressed(_, button)
  if not live() then return end
  local d = PAD_DIRS[button]
  if d then
    cursorMove(d[1], d[2])
  elseif button == "a" then
    cursorPress()
  elseif button == "b" then
    cursorBack()
  elseif button == "y" then
    callWave()
  end
end

-- The stick steps the cursor once each time it's pushed past half way.
function love.gamepadaxis(_, axis, value)
  if not live() then return end
  local v = if value > 0.5 then 1 elseif value < -0.5 then -1 else 0
  if axis == "leftx" then
    if v ~= 0 and v ~= stickX then cursorMove(v, 0) end
    stickX = v
  elseif axis == "lefty" then
    if v ~= 0 and v ~= stickY then cursorMove(0, v) end
    stickY = v
  end
end

-- ---- Juice ------------------------------------------------------------------

-- The kit's shake is "trauma" (0…1); fx.shake is units and seconds.
local function shake(trauma)
  fx.shake(14 * min(trauma, 1), 0.2 + 0.25 * min(trauma, 1))
end

local function react(cues)
  local on = playing()
  for _, c in cues do
    local k = c.kind
    if k == "built" then
      taughtBuild = true
      local p = PADS[c.pad]
      fx.burst("burst", p[1], p[2], { count = 14, color = look.TOWER[c.tower] })
      fx.burst("sparkles", p[1], p[2] - 10, { count = 6, color = 0xfff2b0 })
      addRing(p[1], p[2], 46, look.WHITE)
      shake(0.12)
    elseif k == "upgraded" then
      taughtUpgrade = true
      local p = PADS[c.pad]
      fx.burst("stars", p[1], p[2] - 12, { count = 4 + 2 * c.level, color = look.GOLD })
      addRing(p[1], p[2], 56, look.GOLD)
      fx.popup(if c.level == 2 then "LV 2" else "MAX!", p[1], p[2] - 34, { color = look.GOLD, size = 20 })
    elseif k == "frost" then
      addRing(c.x, c.y, c.r, look.FROST, 0.5, 0.8)
      fx.burst("sparkles", c.x, c.y, { count = 5, color = 0xe6f9ff })
    elseif k == "boom" then
      fx.burst("burst", c.x, c.y, { count = 16, color = 0xffa340 })
      fx.burst("sparks", c.x, c.y, { count = 10, color = look.GOLD })
      addRing(c.x, c.y, c.r * 1.2, 0xffd9a0, 0.4)
      shake(0.08)
    elseif k == "hit" then
      fx.burst("sparkles", c.x, c.y, { count = 2, color = look.WHITE })
    elseif k == "pop" then
      splats[#splats + 1] = { x = c.x, y = c.y, kind = c.creep, flip = c.chain % 2 == 1, age = 0 }
      fx.burst("burst", c.x, c.y, { count = 10, color = look.JELLY[c.creep] })
      if c.creep == "king" then
        fx.burst("confetti", c.x, c.y, { count = 30 })
        shake(0.4)
        fx.hitstop(0.08)
      end
      if on then
        fx.popup("+" .. c.coins, c.x, c.y - 14, { color = look.GOLD, size = if c.chain >= 3 then 22 else 18 })
        coinPunch = max(coinPunch, 0.15)
      end
    elseif k == "split" then
      addRing(c.x, c.y, 30, 0xffd0a0, 0.35)
    elseif k == "bite" then
      shake(0.45)
      hurtFlash = 1
      wince = 1
      fx.hitstop(0.06)
      fx.burst("burst", c.x, c.y, { count = 12, color = look.CAKE_SPONGE })
      fx.burst("burst", sim.CAKE_X, sim.CAKE_Y, { count = 10, color = look.CAKE_FROSTING })
      if on then fx.popup("CHOMP!", sim.CAKE_X, sim.CAKE_Y - 50, { color = look.DANGER, size = 24 }) end
    elseif k == "wave" then
      local text = "WAVE " .. c.n
      if c.newKind then text = look.NEW_KIND_BANNER[c.newKind]
      elseif c.boss then text = look.BOSS_BANNER
      elseif c.rush then text = look.RUSH_BANNER end
      banner = { text = text, age = 0, big = c.boss or c.rush or c.newKind ~= nil }
      if c.boss then shake(0.3) end
    elseif k == "payday" then
      if on and game.wave > 1 then
        local x, y, w, h = layout.coinPill()
        fx.popup("+" .. c.coins, x + w / 2, y + h + 12, { color = look.GOLD, size = 18 })
      end
    elseif k == "ending" then
      shake(0.8)
      fx.hitstop(0.12)
      select("none")
    elseif k == "denied" then
      shake(0.05)
    elseif k == "chomp" then
      fx.burst("burst", c.x, c.y, { count = 6, color = look.CAKE_SPONGE })
      fx.burst("burst", sim.CAKE_X, sim.CAKE_Y, { count = 5, color = look.CAKE_FROSTING })
      wince = 1
      shake(0.12)
    end
  end
end

-- ---- Hints ------------------------------------------------------------------

-- Where the hint points: x, y, words, on a pad; or nil.
local function hint()
  if not playing() or not game:playing() then return nil end
  local stalled = idle > HINT_IDLE
  if sel.what == "pad" and (not taughtBuild or stalled) then
    local k = if taughtBuild then table.find(sim.KINDS, planner.nextKind(game)) or 1 else 1
    local s = layout.pickerSlots(sel.pad)[k]
    if game.coins >= sim.TOWERS[sim.KINDS[k]].cost[1] then return s[1], s[2], look.HINT_BUY, false end
  elseif sel.what == "tower" and (not taughtUpgrade or stalled) then
    local cost = game:upgradeCost(sel.pad)
    if cost and cost <= game.coins then
      local x, y = layout.upgradeSlot(sel.pad)
      return x, y, look.HINT_UPGRADE, false
    end
  elseif sel.what == "none" and not taughtBuild then
    local p = PADS[sim.HINT_PAD]
    return p[1], p[2], look.HINT_BUILD, true
  elseif sel.what == "none" and stalled then
    local kind = planner.nextKind(game)
    if game.coins >= sim.TOWERS[kind].cost[1] then
      local p = planner.padsFor(game, kind)[1]
      if p then return PADS[p][1], PADS[p][2], look.HINT_BUILD, true end
    end
    if not taughtUpgrade then
      for p = 1, #PADS do
        local cost = game:upgradeCost(p)
        if cost and cost <= game.coins then return PADS[p][1], PADS[p][2], look.HINT_UPGRADE, true end
      end
    end
  end
  return nil
end

-- ---- Autopilots -------------------------------------------------------------

-- The title card's game (and the debug Autopilot): the planner's move
-- every so often, when it can pay. On the title, the cake falls now and
-- then and a new game starts.
local function autopilot(dt, style)
  attractT += dt
  if game.over then
    if round.title and attractT > 1.5 then
      game = attractGame(love.math.random(1, 2147483646), 150)
      attractT = 0
    end
    return
  end
  if attractT < ATTRACT_THINK then return end
  local a = planner.want(game, style)
  if a and (a.what ~= "call" or style.callEarly) and planner.cost(game, a) <= game.coins then
    planner.apply(game, a)
    attractT = 0
  end
end

-- ---- The frame --------------------------------------------------------------

function love.update(dt)
  if not applied then applyScenario() end
  time += dt
  idle += dt
  selT += dt
  if round.title then
    autopilot(dt, ATTRACT_STYLE)
  elseif playing() then
    playT += dt
    if AUTO then
      autopilot(dt, AUTO_STYLE)
      idle = 0 -- no hints for a round that plays itself
    end
  end
  if not round.ended then game:step(dt) end
  local cues = game.cues
  game.cues = {}
  react(cues)
  sound.cues(cues)
  -- A pad selected that just got built on (the autopilot).
  if sel.what == "pad" and game.towers[sel.pad] then select("none") end

  if playing() then
    round.score = game.score
    -- The hearts are round.lives; the last one goes when the devouring
    -- has played (round.over below), not when it starts.
    if game.ending then
      if not ended then
        ended = true
        ui.hud({ lives = false })
        round.post("wave", game.wave)
      end
    else
      round.lives = game.hearts
    end
    if game.over then
      local best = save.get("best") or 0
      if game.score > best then
        save.set("best", game.score)
        sound.best()
      end
      overAt = time
      -- The aftermath: the plate, the smug culprits, and a road gone quiet.
      table.clear(game.creeps)
      round.over(look.HEADING, string.lower(sim.CREEPS[game.killer or "gummy"].name))
    end
  end

  if denied then
    denied[2] += dt
    if denied[2] > 0.4 then denied = nil end
  end
  if banner then
    banner.age += dt
    if banner.age > BANNER_LIFE then banner = nil end
  end
  for i = #splats, 1, -1 do
    local s = splats[i]
    s.age += dt
    if s.age >= SPLAT_LIFE then table.remove(splats, i) end
  end
  for i = #rings, 1, -1 do
    local r = rings[i]
    r.age += dt
    if r.age >= r.life then table.remove(rings, i) end
  end
  wince = max(wince - dt / 0.5, 0)
  hurtFlash = max(hurtFlash - dt / 0.5, 0)
  coinPunch = max(coinPunch - dt * (coinPunch * 8 + 0.2), 0)
  -- The music turns urgent with danger, and for as long as a king is on the road.
  local danger = if playing() then max(game:danger(), if game:king() then 1 else 0) else 0
  sound.update(dt, danger)

  shared.game, shared.sel, shared.live = game, sel, live()
end

-- ---- Drawing ----------------------------------------------------------------

local function byY(a, b)
  if a.y ~= b.y then return a.y < b.y end
  return a.id < b.id
end

function love.draw()
  local g = game
  draw.map()
  draw.roadFlow(time)
  -- The selected tower's reach (or a POP's, over an empty pad).
  if sel.what ~= "none" then
    local p = PADS[sel.pad]
    local t = g.towers[sel.pad]
    local r = if t then sim.TOWERS[t.kind].range[t.level] else sim.TOWERS.pop.range[1]
    draw.reach(p[1], p[2], r, selT / 0.25)
  end
  for i, p in PADS do
    draw.pad(p[1], p[2], g.towers[i] == nil, sel.pad == i and sel.what ~= "none")
  end
  draw.cake({ g = g, time = time, wince = wince, gone = g.over or round.ended })
  -- Jellies, back to front, then towers over them where they overlap.
  table.clear(order)
  for i, c in g.creeps do order[i] = c end
  table.sort(order, byY)
  for _, c in order do draw.jellyShadow(c) end
  for _, c in order do draw.jelly(c, time) end
  for _, s in splats do draw.splat(s, SPLAT_LIFE) end
  for i, p in PADS do
    local t = g.towers[i]
    if t then draw.tower(p[1], p[2], t, 1, time) end
  end
  for _, sh in g.shots do draw.shot(sh) end
  draw.rings(rings)
  local last = if playing() and g.hearts == 1 and g:playing() then 0.3 * (0.8 + 0.2 * sin(time * 4)) else 0
  draw.vignette(max(last, hurtFlash * 0.55))

  if playing() then
    local cur = cursor.shown and g:playing()
    draw.picker({ g = g, sel = sel, selT = selT, denied = denied, time = time, focus = if cur and sel.what == "pad" then cursor.slot elseif cur and sel.what == "tower" then 1 else nil })
    draw.hud({ g = g, time = time, coinPunch = 1 + coinPunch, focus = cur and sel.what == "none" and cursor.at == WAVE_TARGET })
    local hx, hy, words, onPad = hint()
    if hx then draw.hint(hx, hy, words, onPad, time) end
    if banner then draw.banner(banner.text, banner.age, banner.big) end
    if cur and sel.what == "none" and cursor.at ~= WAVE_TARGET then
      draw.cursor(PADS[cursor.at][1], PADS[cursor.at][2], sim.PAD, sim.PAD, time)
    end
  elseif round.ended then
    draw.reached(g.wave, math.clamp((time - overAt - 0.9) / 0.3, 0, 1))
  end
end
