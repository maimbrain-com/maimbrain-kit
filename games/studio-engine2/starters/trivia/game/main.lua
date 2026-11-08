-- Bright Spark, the trivia starter: a fast quiz staged like a 1950s TV game
-- show. Watt, a lightbulb quiz host with a polka-dot bow tie, asks; you tap
-- one of four answers before his clock runs out. Faster answers score
-- more, streaks multiply, every fifth question is golden, a stats card
-- ends each round of the show (sim.ROUND_QUESTIONS: 10 questions), and the
-- clock tightens every question. Three misses and Watt's bulb blows. Keys and controllers play too: the arrows
-- (or the d-pad) move between the answers and Enter or A picks one, or
-- press 1 to 4.
--
-- Make it yours:
-- - The questions are game/questions.lua: a theme of your own (one
--   subject, a kids' edition, true or false) is the quickest new game.
-- - The rules and their knobs (the clock, tiers, points, streaks, golden
--   questions, hearts) are at the top of game/sim.lua.
-- - Words, colours, category emblems and where things sit are
--   game/look.lua; the pictures are images/ (stage.png, Watt's seven faces
--   in host.png, the emblems in cats.png, burst.png). The cards, buttons,
--   clock and hearts are the identity's widgets (studio.toml's [identity]).
-- - Sounds and the band's mix are game/sound.lua (sounds/).
-- - This file is the feel: input, juice, Watt, the title card's loop.

local sim = require("sim")
local look = require("look")
local sound = require("sound")
local shared = require("shared")

-- ---- Feel -------------------------------------------------------------------
-- The how-to hint comes back when a player has been still this long on a
-- question and the clock is nearly out (reading isn't stalling; freezing is).
local HINT_IDLE = 3.0
local HINT_LEFT = 2.5
-- Seconds into the bulb's flicker when it pops.
local POP_AT = 0.8
-- Watt's glass radius in units: in play, on the title card.
local PLAY_HOST = 41
local TITLE_HOST = 50
-- The title card's loop, seconds: a card flips in, the clock drains, at
-- ATTRACT_REVEAL it turns over to show the answer, then the next one.
local ATTRACT_PERIOD = 3.8
local ATTRACT_REVEAL = 2.2

local sin, cos, abs, min, max, floor, ceil, pi = math.sin, math.cos, math.abs, math.min, math.max, math.floor, math.ceil, math.pi

-- ---- Debug: scenarios (debug/scenarios.toml) and the menu's own items -------
-- Set by a scenario once love.load has run; love.update applies them once.
START_AT = 0 -- start the round at this question (from 0), answered right up to it
HEARTS = 0 -- start with this many hearts (0: the rule's)
AUTOPLAY = false -- answer right halfway through every clock (a hands-off run)
AUTOPLAY_MISS = false -- with AUTOPLAY: answer wrong instead

local game -- the round's sim (game/sim.lua)
local L -- look.layout()
local time, idle = 0, 0
local hurt = 0 -- a miss: the red edges pulse, 1 fading to 0
local answeredOnce = false -- every round teaches again: never remembered
local popped = false
local applied = false
local endedAt -- round.over has been called
-- This round of the show (sim.ROUND_QUESTIONS questions): for its stats card.
local function newPart(n)
  return { n = n, right = 0, points = 0, asked = 0, streak = 0, best = 0 }
end
local part = newPart(1)

-- Nodes.
local stage, dim, glows, host, goldBurst, summary
local card -- { group, panel, emblem, cat, tag, text, back, backText }
local ring, streakPill, streakBurst
local hearts, heartStrip = {}, nil
local buttons = {} -- the four answers, with their motion: { node, punch, jolt }
local hint
local smoke

-- Watt: his face now, until when a reaction holds it, and its glass radius.
local face = { rest = "idle", react = nil, untilT = 0 }
local attract = { t = 0, q = nil }

local function playing()
  return not round.title and not round.ended
end

-- ---- Watt -------------------------------------------------------------------
local function react(mood, secs)
  face.react, face.untilT, face.since = mood, time + secs, time
end

local hostR = PLAY_HOST
local function placeHost(x, y, r)
  host.x, host.y = x, y
  hostR = r
  host.scale = r / look.GLASS[3]
  for i, g in glows do
    g.x, g.y = x, y
    g.radius = r * (1 + 0.22 * i)
  end
end

local function updateHost(dt)
  if face.react and time >= face.untilT then face.react = nil end
  local mood = face.react or face.rest
  local frame = look.FACE[mood] or 1
  -- A miss: the bulb stutters between lit and dim for a moment (the last
  -- heart: until it pops).
  local stutter = (game and game.phase == "ending") and POP_AT or 0.45
  if mood == "flicker" and time - (face.since or 0) < stutter and floor(time * 14) % 2 == 0 then frame = look.FACE.nervous end
  if popped then frame = look.FACE.cracked end
  host.frame = frame
  local lit = (popped or mood == "flicker") and 0 or 1
  local mult = game and playing() and game:multiplier() or 1
  local target = lit * (0.1 + 0.07 * (mult - 1)) * (0.85 + 0.15 * sin(time * 3))
  for _, g in glows do g.alpha += (target - g.alpha) * min(dt * 8, 1) end
end

-- ---- The card ---------------------------------------------------------------
-- The question's text size and how many lines it takes, from its length
-- (estimated, so the round never waits on text measurement).
-- (A character is about 0.47 of the size across, and wrapping at words
-- leaves lines about 85% full.)
local function linesAt(s, size)
  return max(1, ceil(#s * size * 0.47 / ((L.card.w - 34) * 0.85)))
end

local function questionSize(s)
  for _, size in { 24, 22 } do
    if linesAt(s, size) <= 2 then return size, linesAt(s, size) end
  end
  for _, size in { 20, 19, 18 } do
    if linesAt(s, size) <= 3 then return size, linesAt(s, size) end
  end
  return 17, linesAt(s, 17)
end

local function makeCard()
  local g = Group{}
  local c = { group = g }
  c.panel = ui.panel{ x = 0, y = 0, w = L.card.w, h = L.card.h, parent = g }
  c.text = Text{ text = "", x = 0, y = -30, width = L.card.w - 34, align = "center", size = 20, color = ui.color("text"), parent = g }
  c.emblem = Sprite{ image = "cats", frame = 1, x = -60, y = -L.card.h / 2 + 2, scale = 40 / 98, parent = g }
  c.cat = ui.pill{ text = "SCIENCE", x = 10, y = -L.card.h / 2 + 2, h = 28, tone = "plain", parent = g }
  c.tag = Text{ text = "", x = L.card.w / 2 - 14, y = -L.card.h / 2 + 12, align = "right", size = 14, color = look.INK, parent = g }
  c.tag.alpha = 0.65
  -- The back: the right answer, big, on green (the title card's reveal).
  c.backShade = Text{ text = "", x = 2, y = -15, width = L.card.w - 40, align = "center", size = 30, font = "Luckiest Guy", color = look.INK, parent = g }
  c.backText = Text{ text = "", x = 0, y = -18, width = L.card.w - 40, align = "center", size = 30, font = "Luckiest Guy", color = look.PAPER, parent = g }
  c.check = ui.icon("check", { x = 0, y = -L.card.h / 2, size = 30, tone = "gold", parent = g })
  return c
end

-- Fills the card with question q (`tag` top right; golden on gold).
local function showQuestion(q, tag, golden)
  local size, lines = questionSize(q.text)
  card.text.text = q.text
  card.text.size = size
  -- Centred between the category pill and the card's foot (where the hint sits).
  card.text.y = 4 - size * 1.15 * lines / 2
  card.text.visible = true
  card.backText.visible, card.backShade.visible, card.check.visible = false, false, false
  card.panel.color = golden and look.GOLD or 0xffffff
  local cat = look.CATEGORIES[q.cat]
  card.emblem.visible = cat ~= nil
  if cat then card.emblem.frame = cat[1] end
  card.cat.text = q.cat
  card.cat.color = cat and look.light(cat[2]) or 0xffffff
  card.cat.visible = true
  card.tag.text = tag
  goldBurst.visible = golden
end

-- The card's back: the answer on green.
local function showAnswer(a)
  card.text.visible, card.cat.visible, card.emblem.visible = false, false, false
  card.tag.text = ""
  card.panel.color = look.RIGHT
  local size = #a > 14 and 30 or 38
  for _, t in { card.backText, card.backShade } do
    t.text, t.size, t.visible = a, size, true
    t.y = (t == card.backShade and 3 or 0) - size * 0.55
  end
  card.check.visible = true
  goldBurst.visible = false
end

-- ---- The round --------------------------------------------------------------
local function tagFor(d)
  if d.golden then return look.GOLDEN end
  if d.index < sim.WARMUP then return look.WARMUP end
  return "Q" .. (d.index + 1)
end

local function heartsShown(n)
  for i, h in hearts do
    local on = i <= n
    h.tone = on and "bad" or "plain"
    h.alpha = on and 1 or 0.25
  end
end

local function setupButtons()
  for i = 1, 4 do
    local a = L.answers[i]
    local b = ui.button{ text = "", x = a.x, y = a.y, w = a.w, h = a.h, tap = function()
      idle = 0
      if game and game:tap(i) then
        answeredOnce = true
        buttons[i].punch = -0.08
        sound.tap()
      end
    end }
    buttons[i] = { node = b, punch = 0, jolt = 9 }
  end
end

local function heartAt(i)
  local h = hearts[i]
  return h.x, h.y
end

local partCard -- below

-- Juice for the sim's cues (sounds and haptics are sound.lua's).
local function juice(cues)
  for _, c in cues do
    local k = c.kind
    if k == "deal" then
      local d = game.cur
      print(string.format("trivia: question %d at %.1f s (tier %d, %.1f s to answer)", d.index + 1, round.time, d.tier, d.limit))
      showQuestion(d.q, tagFor(d), d.golden)
      for i, b in buttons do
        b.node.text = game:answer(i)
        b.node.color = 0xffffff
        b.punch, b.jolt = 0, 9
      end
      if c.golden then
        react("star", 0.9)
        fx.burst("sparkles", L.card.x, L.card.y, { count = 18, color = { 0xfff2b0, look.GOLD } })
      end
      part.asked += 1
    elseif k == "ask" then
      idle = 0
    elseif k == "tick" then
      ring.scale = 1.18
    elseif k == "right" then
      local a = L.answers[c.slot]
      buttons[c.slot].node.color = look.RIGHT
      buttons[c.slot].punch = 0.2
      react((c.streak >= 5 or game.cur.golden) and "star" or "happy", 0.85)
      fx.burst("sparkles", a.x, a.y, { count = 14, color = { 0xfff2b0, 0xffffff } })
      fx.burst("stars", L.host.x, L.host.y - 30, { count = 5 + min(c.streak, 8), color = look.MUSTARD })
      fx.popup("+" .. c.points, a.x + a.w / 2 - 50, a.y - 26, { color = look.CREAM, size = 24 })
      if c.fast then fx.popup(look.QUICK, a.x - 70, a.y - 24, { color = 0xffffff, size = 18 }) end
      round.score = game.score
      fx.shake(3, 0.15)
      part.right += 1
      part.points += c.points
      part.streak += 1
      part.best = max(part.best, part.streak)
    elseif k == "wrong" then
      part.streak = 0
      local a = L.answers[c.slot]
      buttons[c.slot].node.color = look.WRONG
      buttons[c.slot].jolt = 0
      buttons[game:rightSlot()].node.color = look.RIGHT
      react("flicker", 1.4)
      fx.burst("burst", a.x - a.w / 2 + 30, a.y, { count = 14, color = look.ALARM })
      fx.shake(7, 0.3)
      fx.hitstop(0.08)
      hurt = 1
    elseif k == "timeout" then
      part.streak = 0
      buttons[game:rightSlot()].node.color = look.RIGHT
      react("flicker", 1.4)
      fx.popup(look.TIME_UP, L.timer.x, L.timer.y + 40, { color = look.WRONG, size = 26 })
      fx.shake(6, 0.3)
      hurt = 1
    elseif k == "lifeLost" then
      local x, y = heartAt(c.left + 1)
      fx.burst("burst", x, y, { count = 16, color = look.ALARM })
      fx.burst("sparks", x, y, { count = 12, color = look.ALARM })
      heartsShown(c.left)
    elseif k == "multUp" then
      streakPill.scale = 1.5
      fx.popup("×" .. c.mult .. " STREAK!", L.streak.x, L.streak.y - 36, { color = look.MUSTARD, size = 24 })
      fx.burst("stars", L.streak.x, L.streak.y, { count = 8, color = look.MUSTARD })
    elseif k == "heartBack" then
      heartsShown(c.lives)
      local x, y = heartAt(c.lives)
      fx.burst("sparkles", x, y, { count = 10, color = look.ALARM })
      fx.popup("+♥", x - 10, y + 26, { color = look.ALARM, size = 24 })
    elseif k == "break" then
      partCard()
    elseif k == "ending" then
      face.rest = "flicker"
      react("flicker", 9)
    end
  end
end

-- The bulb blows: glass, sparks, smoke, a thump.
local function pop()
  print("trivia: pop")
  popped = true
  local x, y = host.x, host.y
  fx.burst("confetti", x, y - 10, { count = 18, color = { 0xffffff, 0xcfe6ff } })
  fx.burst("sparks", x, y - 10, { count = 26, color = look.GLOW })
  fx.burst("ring", x, y, { count = 1, color = 0xffffff })
  fx.shake(10, 0.5)
  fx.flash(0xffffff, 0.3)
  sound.pop()
  smoke.on = true
end

-- The stats card after every sim.ROUND_QUESTIONS questions (a round of the
-- show), then the next round.
function partCard()
  local n = part.n
  print(string.format("trivia: round %d done at %.1f s (%d of %d right, best streak %d)", n, round.time, part.right, part.asked, part.best))
  ui.stats{
    title = string.format(look.PART, n),
    rows = {
      { "Right", part.right / max(part.asked, 1), "percent" },
      { "Points", part.points },
      { "Best streak", part.best },
    },
    rank = true,
    wipe = "iris",
    done = function()
      part = newPart(n + 1)
      game:advance()
    end,
  }
end

function love.load()
  L = look.layout(love.window.getSafeArea())
  shared.layout = L
  sound.load()

  stage = Sprite{ image = "stage", x = 180, y = 320, scale = 0.5, order = -10 }
  -- Lights out: the studio dims behind the game-over card.
  dim = Shape{ x = 180, y = 320, w = 360, h = 640, color = look.INK, alpha = 0 }
  goldBurst = Sprite{ image = "burst", x = L.card.x + L.card.w / 2 - 40, y = L.card.y - L.card.h / 2 - 50, scale = 112 / 256, vr = 0.5, visible = false }
  streakBurst = Sprite{ image = "burst", x = L.streak.x, y = L.streak.y, scale = 0.3, vr = 0.8, visible = false }
  -- Watt's glow: rings that fade out from his glass.
  glows = {}
  for i = 3, 1, -1 do glows[i] = Shape{ shape = "circle", radius = 60, color = look.GLOW, alpha = 0 } end
  host = Sprite{ image = "host", frame = 1, anchorX = look.GLASS[1], anchorY = look.GLASS[2] }
  smoke = { puffs = {}, at = 0, on = false }
  card = makeCard()
  ring = ui.ring{ x = L.timer.x, y = L.timer.y, r = 30, value = 1, text = "", tone = "good" }
  streakPill = ui.pill{ text = "", icon = "fire", x = L.streak.x, y = L.streak.y, h = 36, tone = "gold", visible = false }

  if round.title then
    -- The title card plays itself: Watt reads a card, the clock drains, the
    -- card turns over to the answer, and the next flips in.
    card.group.x, card.group.y = L.titleCard.x, L.titleCard.y
    placeHost(L.titleHost.x, L.titleHost.y, TITLE_HOST)
    ring.x, ring.y = L.titleTimer.x, L.titleTimer.y
    goldBurst.y = L.titleCard.y - L.titleCard.h / 2 - 50
    attract.rng = love.math.newRandomGenerator(love.math.random(1, 2147483646))
    ui.panel{ x = L.titleSign.x, y = L.titleSign.y, w = L.titleSign.w, h = L.titleSign.h }
    return
  end

  game = sim.new(love.math.random(1, 2147483646), round.daily)
  shared.game = game
  card.group.x, card.group.y = L.card.x, L.card.y
  placeHost(L.host.x, L.host.y, PLAY_HOST)
  heartStrip = ui.panel{ x = L.hearts.x - 38, y = L.hearts.y, w = 92, h = 34 }
  for i = 1, sim.LIVES do
    hearts[i] = ui.icon("heart", { x = L.hearts.x - 38 + (i - 2) * 26, y = L.hearts.y, size = 22, tone = "bad" })
  end
  setupButtons()
  ui.hud{ score = true, best = true }
  sound.start()
  face.rest = "think"
  -- The tap that started the round deals the first card at once: that flip
  -- (and the jingle) is its answer, the warm-up question its first move.
  juice(game.cues)
  sound.cues(game.cues)
  game.cues = {}
end

-- The title card's loop.
local function attractStep(dt)
  local t0 = attract.t
  attract.t += dt
  if not attract.q or attract.t >= ATTRACT_PERIOD then
    -- A short easy question, so the card reads at a glance (the shortest
    -- there are, if none is short).
    local pool, limit = {}, 48
    while #pool == 0 do
      for t = 1, 3 do
        for _, q in sim.bank[t] do
          if #q.text <= limit and (t == 1 or #sim.bank[1] == 0) then pool[#pool + 1] = q end
        end
      end
      limit += 16
    end
    attract.q = pool[attract.rng:random(1, #pool)]
    attract.t = 0
    showQuestion(attract.q, "", false)
  elseif t0 < ATTRACT_REVEAL and attract.t >= ATTRACT_REVEAL then
    react("happy", 1.2)
    fx.burst("sparkles", L.titleCard.x, L.titleCard.y, { count = 14, color = { 0xfff2b0, 0xffffff } })
  end
  local t = attract.t
  local asking = t < ATTRACT_REVEAL
  face.rest = asking and "think" or "idle"
  -- A 5-second clock that drains until Watt "answers".
  local left = 5 - max(min(t, ATTRACT_REVEAL) - 0.4, 0) * 1.6
  ring.value = left / 5
  ring.text = tostring(ceil(left))
  ring.tone = "good"
  -- The card flips in, turns over at the reveal, and flips out at the end.
  local turn = 0.15
  local flip
  if t < ATTRACT_REVEAL then
    flip = 1 - (1 - min(t / 0.4, 1)) ^ 3
  elseif t < ATTRACT_REVEAL + turn then
    flip = 1 - (t - ATTRACT_REVEAL) / turn
  else
    if card.text.visible then showAnswer(attract.q.answers[1]) end
    local open = min((t - ATTRACT_REVEAL - turn) / turn, 1)
    local close = math.clamp((t - (ATTRACT_PERIOD - 0.25)) / 0.25, 0, 1)
    flip = open * (1 - close)
  end
  card.group.sx = max(flip, 0.01)
  host.y = L.titleHost.y + sin(time * 2) * 2
end

local function applyScenario()
  applied = true
  if START_AT > 0 then
    -- Answered right up to it, through any break for a stats card.
    local function to(phase)
      while game.phase ~= phase do
        if game.phase == "break" then game:advance() else game:step(0.05) end
      end
    end
    for _ = 1, START_AT do
      to("ask")
      game:tap(game:rightSlot())
      to("deal")
    end
    game.score, game.streak, game.right, game.bestStreak = 0, 0, 0, 0
    part = newPart(sim.ROUND_QUESTIONS > 0 and START_AT // sim.ROUND_QUESTIONS + 1 or 1)
    round.score = 0
    juice({ { kind = "deal", golden = game.cur.golden } })
    game.cues = {}
  end
  if HEARTS > 0 then
    game.lives = math.clamp(HEARTS, 1, sim.LIVES)
    heartsShown(game.lives)
  end
end

-- How far the card has turned in (and, at the end of a reveal, out).
local function cardFlip()
  local t = game.phaseT
  if game.phase == "deal" then
    local k = min(t / (sim.DEAL_TIME * 0.7), 1)
    -- Back-out: overshoots a little and settles.
    local s = 1.70158
    k -= 1
    return 1 + k * k * ((s + 1) * k + s)
  elseif game.phase == "break" then
    return 0
  elseif game.phase == "reveal" then
    local shown = game.wasRight and sim.REVEAL_RIGHT or sim.REVEAL_WRONG
    local k = math.clamp((t - (shown - 0.16)) / 0.16, 0, 1)
    return 1 - k * k
  end
  return 1
end

local function playStep(dt)
  if not applied then applyScenario() end
  if AUTOPLAY and game.phase == "ask" and game.phaseT > game.cur.limit * 0.5 then
    local i = game:rightSlot()
    if AUTOPLAY_MISS then i = i % 4 + 1 end
    buttons[i].node.tap()
  end
  game:step(dt)
  local cues = game.cues
  game.cues = {}
  juice(cues)
  sound.cues(cues)
  if game.phase == "ending" and not popped and game.phaseT >= POP_AT then pop() end
  if game.over and not endedAt then
    endedAt = time
    -- A new best gets the brass hit, the rest the sad trombone.
    local key = round.daily and "best-daily" or "best"
    local prev = save.get(key) or 0
    sound.verdict(game.score > prev and prev > 0)
    if game.score > prev then save.set(key, game.score) end
    local tiers = { "easy", "medium", "hard" }
    round.over(look.OVER, "missed a " .. tiers[game.cur.tier] .. " one")
  end

  -- Watt: thinking while asked, sweating when the clock is tight.
  if game.phase == "ask" then face.rest = game:danger() > 0.35 and "nervous" or "think" end
  host.y = L.host.y + sin(time * 2) * 2

  -- The card turns in and out; the clock drains green, gold, red.
  local flip = cardFlip()
  card.group.sx = max(flip, 0.01)
  local left = game:timeLeft()
  ring.value = left / game.cur.limit
  ring.text = tostring(ceil(left))
  ring.tone = left <= 3 and "bad" or (left / game.cur.limit < 0.5 and "gold" or "good")
  ring.visible = game.phase ~= "ending" and game.phase ~= "over"
  ring.scale += (1 - ring.scale) * min(dt * 10, 1)

  -- The streak: ×2 and up on a starburst.
  local mult = game:multiplier()
  streakPill.visible = mult > 1 and not round.ended
  streakBurst.visible = streakPill.visible
  streakPill.text = "×" .. mult
  streakBurst.scale = 0.26 + 0.05 * mult
  streakPill.scale += (1 - streakPill.scale) * min(dt * 8, 1)

  -- The answers: slide in staggered while the card turns, then live, then judged.
  for i, b in buttons do
    local a = L.answers[i]
    local n = b.node
    local dx, alpha = 0, 1
    if game.phase == "deal" then
      local e = math.clamp((game.phaseT - (i - 1) * 0.06) / 0.3, 0, 1)
      dx = (1 - (1 - (1 - e) ^ 3)) * 70
      alpha = e
    elseif game.phase ~= "ask" then
      local judged = (i == game.picked) or (i == game:rightSlot() and (game.picked ~= nil or not game.wasRight))
      alpha = judged and flip or 0.45 * flip
      if game.phase ~= "reveal" then alpha = judged and 1 or 0.45 end
      if b.jolt < 0.45 then dx = sin(b.jolt * 60) * 9 * (1 - b.jolt / 0.45) end
    end
    b.jolt += dt
    b.punch += (0 - b.punch) * min(dt * 9, 1)
    n.x, n.y = a.x + dx, a.y
    n.alpha = alpha
    n.scale = 1 + b.punch
  end

  -- Teach the verb until the first answer, and again when a player freezes.
  local stalled = idle > HINT_IDLE and game:timeLeft() < HINT_LEFT
  if game.phase == "ask" and game.phaseT > 0.3 and (not answeredOnce or stalled) then
    if not hint then hint = ui.hint{ text = look.HINT, at = { L.answers[1].x, L.answers[1].y - 8 } } end
  elseif hint then
    hint:dismiss()
    hint = nil
  end

  -- The band brightens with the streak and the clock; the stopwatch ticks in as it runs down.
  local danger = game:danger()
  local clock = 0
  if game.phase == "ask" then
    clock = max(math.clamp((0.6 - left / game.cur.limit) / 0.45, 0, 1), danger)
  end
  sound.intensity(min(max(danger, 0.25 * (mult - 1)), 1), clock)
end

-- Under the game-over card: the studio dims, the round's things go, and
-- the blown bulb stands smoking under the retry button with the round's line.
local function overStep(dt)
  if not summary then
    for _, n in { card.group, ring, streakPill, streakBurst, goldBurst, heartStrip } do n.visible = false end
    for _, h in hearts do h.visible = false end
    for _, b in buttons do b.node.visible = false end
    if hint then hint:dismiss() end
    summary = Text{ text = string.format(look.SUMMARY, game.right, game.bestStreak), x = L.overLine.x, y = L.overLine.y, align = "center", size = 17, color = look.PAPER }
    flux.to(host, 0.5, { x = L.overHost.x, y = L.overHost.y, scale = 30 / look.GLASS[3] }):ease("backout")
    hostR = 30
  end
  dim.alpha = min(dim.alpha + dt / 0.6 * 0.55, 0.55)
  for _, g in glows do g.x, g.y = host.x, host.y end
end

-- Smoke curls up from a blown bulb: grey puffs that rise, swell and fade.
local function smokeStep(dt)
  if not smoke.on then return end
  smoke.at -= dt
  if smoke.at <= 0 and #smoke.puffs < 12 then
    smoke.at = 0.16
    local p = Shape{ shape = "circle", radius = 6, x = host.x + hostR * 0.45 + sin(time * 13) * 3, y = host.y - hostR * 1.1, color = 0x9a918a, alpha = 0.6 }
    p.data.age = 0
    table.insert(smoke.puffs, p)
  end
  for i = #smoke.puffs, 1, -1 do
    local p = smoke.puffs[i]
    local age = p.data.age + dt
    p.data.age = age
    p.y -= 20 * dt
    p.x += sin(age * 2 + i) * 8 * dt
    p.radius = 6 + age * 9
    p.alpha = max(0.6 * (1 - age / 1.6), 0)
    if age >= 1.6 then
      p:destroy()
      table.remove(smoke.puffs, i)
    end
  end
end

function love.update(dt)
  time += dt
  idle += dt
  hurt = max(hurt - dt / 0.5, 0)
  if round.title then
    attractStep(dt)
  elseif game and round.ended then
    overStep(dt)
  elseif game then
    playStep(dt)
  end
  updateHost(dt)
  smokeStep(dt)
  sound.update(dt)
end

-- The clock's last seconds: red creeps in from the edges.
function love.draw()
  if not game or round.title then return end
  local a = max(game:danger() * 0.45, hurt * 0.7)
  if a <= 0 then return end
  local w, h = love.graphics.getDimensions()
  love.graphics.setColor(0.85, 0.2, 0.16, a)
  for i = 0, 5 do
    local d = i * 5
    love.graphics.setColor(0.85, 0.2, 0.16, a * (1 - i / 6) * 0.5)
    love.graphics.rectangle("fill", 0, d, w, 5)
    love.graphics.rectangle("fill", 0, h - d - 5, w, 5)
    love.graphics.rectangle("fill", d, 0, 5, h)
    love.graphics.rectangle("fill", w - d - 5, 0, 5, h)
  end
end

-- 1 to 4 answer straight away (the arrows, the d-pad, Enter and A work the
-- buttons by themselves).
function love.keypressed(key)
  idle = 0
  local i = tonumber(key)
  if game and playing() and i and i >= 1 and i <= 4 and game:tap(i) then
    answeredOnce = true
    buttons[i].punch = -0.08
    sound.tap()
  end
end

function love.mousepressed()
  idle = 0
end

debug.action("Next question right", function()
  if game and game.phase == "ask" then game:tap(game:rightSlot()) end
end)
debug.action("Lose a heart", function()
  if game and game.phase == "ask" then game:tap(game:rightSlot() % 4 + 1) end
end)
debug.toggle("Autoplay", function(on) AUTOPLAY = on end, AUTOPLAY)
