-- The rules, with no drawing or sound: a round is a run of questions. Each
-- one is dealt (the card flips in), asked (the clock runs) and revealed
-- (right: points; wrong or out of time: a heart). The first question is a
-- warm-up that can't cost one. The sim never draws or plays: it pushes cues
-- (game.cues) that main.lua turns into juice and sound.lua into sound.
--
-- These are the trivia kit's rules (Bright Spark) and its knobs, unchanged.

local Q = require("questions")

local sim = {}

-- ---- Knobs ------------------------------------------------------------------
-- Hearts a round. Raise it and every round lasts longer.
sim.LIVES = 3
-- The clock for a question is TIME_FIXED + pace × (TIME_BASE + TIME_PER_WORD
-- × words), words being the question plus half the answers' (they're
-- scanned, not read): fair for long and short questions alike.
sim.TIME_FIXED = 1.2 -- seconds to react and tap, never squeezed
sim.TIME_BASE = 1.5 -- seconds to decide, squeezed by the pace
sim.TIME_PER_WORD = 0.42 -- seconds a word to read; raise it for slower readers
-- The pace starts at 1 and loses PACE_DROP a question, down to PACE_MIN:
-- the main escalation (the clock tightens every question).
sim.PACE_DROP = 0.12
sim.PACE_MIN = 0.45
-- The question number (from 0) where medium and hard questions start,
-- whatever the streak. Lower is harder sooner.
sim.TIER2_FROM = 2
sim.TIER3_FROM = 5
-- A streak this long jumps a tier early (a miss drops it back).
sim.TIER2_STREAK = 2
sim.TIER3_STREAK = 4
-- A daily round deals everyone the same questions, so its tiers follow the
-- question number only.
sim.DAILY_TIER2_FROM = 3
sim.DAILY_TIER3_FROM = 7
-- Points for a right answer by tier, at full speed (before multipliers),
-- and the share an answer at the buzzer gets.
sim.POINTS = { 100, 200, 300 }
sim.SPEED_FLOOR = 0.35
-- Streak lengths for ×2, ×3 and ×4.
sim.MULT_STEPS = { 3, 6, 10 }
-- Every Nth question is golden: double points (0: none).
sim.GOLDEN_EVERY = 5
-- A round of the show is this many questions, then a break for its stats
-- card (main.lua) before the next (0: no breaks). Each break holds play for
-- about 6 s, so a shorter round makes every game longer.
sim.ROUND_QUESTIONS = 10
-- A streak of this many (8, 16, …) gives back a lost heart (0: never).
sim.HEART_EVERY = 8
-- The first N questions are a warm-up: a miss costs no heart.
sim.WARMUP = 1
-- The rhythm, seconds: the card flipping in, a right and a wrong answer
-- shown, the bulb blowing at the end.
sim.DEAL_TIME = 0.5
sim.REVEAL_RIGHT = 0.75
sim.REVEAL_WRONG = 1.5
sim.ENDING_TIME = 1.8
-- -----------------------------------------------------------------------------

-- The bank by tier: { tier, cat, text, answers = { right, wrong, wrong, wrong }, words }.
local bank = { {}, {}, {} }
sim.bank = bank

local function words(s)
  local n = 0
  for _ in string.gmatch(s, "%S+") do n += 1 end
  return n
end

-- Words to read: the question plus half the answers (they're scanned).
function sim.readWords(q)
  local w = words(q.text)
  for _, a in q.answers do w += 0.5 * words(a) end
  return w
end

for i, row in Q do
  local tier, cat, text = row[1], row[2], row[3]
  local where = "game/questions.lua, question " .. i
  assert(#row == 7, where .. ": write { tier, \"CATEGORY\", \"question\", \"right\", \"wrong\", \"wrong\", \"wrong\" }")
  assert(tier == 1 or tier == 2 or tier == 3, where .. ": the tier is 1, 2 or 3")
  assert(#text <= 96, where .. ": the question is longer than 96 characters")
  local seen = {}
  for k = 4, 7 do
    assert(#row[k] <= 22, where .. ": the answer \"" .. row[k] .. "\" is longer than 22 characters")
    assert(not seen[row[k]], where .. ": two answers are the same")
    seen[row[k]] = true
  end
  local q = { tier = tier, cat = cat, text = text, answers = { row[4], row[5], row[6], row[7] } }
  q.words = sim.readWords(q)
  table.insert(bank[tier], q)
end
assert(#bank[1] + #bank[2] + #bank[3] > 0, "game/questions.lua has no questions")

-- A list shuffled in place (Fisher–Yates) by a random generator.
local function shuffle(t, rng)
  for i = #t, 2, -1 do
    local j = rng:random(1, i)
    t[i], t[j] = t[j], t[i]
  end
end

-- The streak multiplier for a streak this long.
function sim.multiplier(streak)
  local m = 1
  for _, s in sim.MULT_STEPS do
    if streak >= s then m += 1 end
  end
  return m
end

-- How tight the clock is for question `index` (from 0): 1, down to PACE_MIN.
function sim.pace(index)
  return math.max(1 - sim.PACE_DROP * index, sim.PACE_MIN)
end

-- Seconds on the clock for question q dealt as number `index`.
function sim.timeLimit(q, index)
  return sim.TIME_FIXED + (sim.TIME_BASE + sim.TIME_PER_WORD * q.words) * sim.pace(index)
end

-- Which tier question `index` comes from.
function sim.tierFor(index, streak, daily)
  if daily then
    return 1 + (index >= sim.DAILY_TIER2_FROM and 1 or 0) + (index >= sim.DAILY_TIER3_FROM and 1 or 0)
  end
  if index < sim.WARMUP then return 1 end
  local byIndex = 1 + (index >= sim.TIER2_FROM and 1 or 0) + (index >= sim.TIER3_FROM and 1 or 0)
  local byStreak = 1 + (streak >= sim.TIER2_STREAK and 1 or 0) + (streak >= sim.TIER3_STREAK and 1 or 0)
  return math.max(byIndex, byStreak)
end

function sim.golden(index)
  return sim.GOLDEN_EVERY > 0 and index % sim.GOLDEN_EVERY == sim.GOLDEN_EVERY - 1
end

local Game = {}
Game.__index = Game

-- A new round on `seed` (a daily round: the day's, so everyone gets the
-- same questions in the same order, with the answers in the same places).
function sim.new(seed, daily)
  local g = setmetatable({
    daily = daily,
    phase = "deal", -- deal | ask | reveal | break | ending | over
    phaseT = 0, -- seconds in the phase
    t = 0, -- seconds since the round began
    score = 0,
    streak = 0,
    bestStreak = 0,
    right = 0, -- right answers this round
    lives = sim.LIVES,
    over = false,
    answeredLeft = 0, -- seconds left when the answer came in
    lastPoints = 0,
    picked = nil, -- the slot tapped (nil: time ran out), during a reveal
    wasRight = false,
    cues = {},
    seed = seed,
    decks = { {}, {}, {} },
    next = { 0, 0, 0 },
    lastTick = 4,
    cur = nil,
    rng = love.math.newRandomGenerator(seed),
  }, Game)
  -- Each tier's questions, shuffled once a round.
  for t = 1, 3 do
    for i = 1, #bank[t] do g.decks[t][i] = i end
    shuffle(g.decks[t], g.rng)
  end
  g:deal(0)
  return g
end

function Game:cue(kind, fields)
  local c = fields or {}
  c.kind = kind
  table.insert(self.cues, c)
end

-- The next question of `tier` (another tier when that one has run out; the
-- decks start over only once the whole bank has been asked).
function Game:draw(tier)
  for _ = 1, 2 do
    for _, k in { tier, math.max(tier - 1, 1), math.min(tier + 1, 3), 1, 2, 3 } do
      if self.next[k] < #self.decks[k] then
        self.next[k] += 1
        return bank[k][self.decks[k][self.next[k]]]
      end
    end
    -- Everything asked (a very long round): the decks start over.
    self.next = { 0, 0, 0 }
  end
end

function Game:deal(index)
  local tier = sim.tierFor(index, self.streak, self.daily)
  local q = self:draw(tier)
  -- The answers' order comes from the question number alone, so a daily
  -- round puts them in the same places for everyone, whatever came before.
  self.rng:setSeed(self.seed + (index + 1) * 7919)
  local order = { 1, 2, 3, 4 }
  shuffle(order, self.rng)
  local tierUp = index > 0 and self.cur ~= nil and tier > self.cur.tier
  self.cur = {
    q = q,
    order = order, -- screen slot → index into q.answers (1 is the right one)
    tier = tier,
    golden = sim.golden(index),
    limit = sim.timeLimit(q, index),
    index = index, -- from 0
  }
  self.phase, self.phaseT, self.lastTick = "deal", 0, 4
  self.picked, self.wasRight = nil, false
  self:cue("deal", { golden = self.cur.golden, tier = tier, tierUp = tierUp })
end

-- The answer on screen in slot i (1–4).
function Game:answer(i)
  return self.cur.q.answers[self.cur.order[i]]
end

-- The slot (1–4) the right answer is in.
function Game:rightSlot()
  for i = 1, 4 do
    if self.cur.order[i] == 1 then return i end
  end
  return 1
end

function Game:warmup()
  return self.cur.index < sim.WARMUP
end

-- Seconds left on the clock (frozen once answered).
function Game:timeLeft()
  if self.phase == "deal" then return self.cur.limit end
  if self.phase == "ask" then return math.max(self.cur.limit - self.phaseT, 0) end
  return self.answeredLeft
end

-- 0…1 over the clock's last 3 seconds: Watt's sweat, the red edges and the
-- band's brass follow it.
function Game:danger()
  if self.phase ~= "ask" then return 0 end
  return math.clamp(1 - self:timeLeft() / 3, 0, 1)
end

function Game:multiplier()
  return sim.multiplier(self.streak)
end

-- The player taps slot i (1–4). False if answers aren't live.
function Game:tap(i)
  if self.phase ~= "ask" or i < 1 or i > 4 then return false end
  self:resolve(i)
  return true
end

function Game:resolve(picked)
  self.answeredLeft = self:timeLeft()
  local right = picked == self:rightSlot()
  if right then
    local before = self:multiplier()
    self.streak += 1
    self.bestStreak = math.max(self.bestStreak, self.streak)
    self.right += 1
    local frac = self.answeredLeft / self.cur.limit
    local base = sim.POINTS[self.cur.tier] * (sim.SPEED_FLOOR + (1 - sim.SPEED_FLOOR) * frac)
    base = math.round(base / 10) * 10
    local points = base * self:multiplier() * (self.cur.golden and 2 or 1)
    self.score += points
    self.lastPoints = points
    self:cue("right", { slot = picked, points = points, streak = self.streak, fast = frac > 0.6 })
    if self:multiplier() > before then self:cue("multUp", { mult = self:multiplier() }) end
    if sim.HEART_EVERY > 0 and self.streak % sim.HEART_EVERY == 0 and self.lives < sim.LIVES then
      self.lives += 1
      self:cue("heartBack", { lives = self.lives })
    end
  else
    self.streak, self.lastPoints = 0, 0
    if picked then self:cue("wrong", { slot = picked }) else self:cue("timeout") end
    if not self:warmup() then
      self.lives = math.max(self.lives - 1, 0)
      self:cue("lifeLost", { left = self.lives })
    end
  end
  self.phase, self.phaseT = "reveal", 0
  self.picked, self.wasRight = picked, right
end

-- Moves the round on by dt.
function Game:step(dt)
  if self.over then return end
  self.t += dt
  self.phaseT += dt
  if self.phase == "deal" then
    if self.phaseT >= sim.DEAL_TIME then
      self.phase, self.phaseT = "ask", self.phaseT - sim.DEAL_TIME
      self:cue("ask")
    end
  elseif self.phase == "ask" then
    local left = self:timeLeft()
    local whole = math.ceil(left)
    if whole < self.lastTick and whole <= 3 and left > 0 then
      self.lastTick = whole
      self:cue("tick", { left = whole })
    end
    if left <= 0 then self:resolve(nil) end
  elseif self.phase == "reveal" then
    local shown = self.wasRight and sim.REVEAL_RIGHT or sim.REVEAL_WRONG
    if self.phaseT >= shown then
      if self.lives == 0 then
        self.phase, self.phaseT = "ending", 0
        self:cue("ending")
      elseif sim.ROUND_QUESTIONS > 0 and (self.cur.index + 1) % sim.ROUND_QUESTIONS == 0 then
        -- A round of the show is over: a break for main.lua's stats card,
        -- until it calls advance().
        self.phase, self.phaseT = "break", 0
        self:cue("break")
      else
        self:deal(self.cur.index + 1)
      end
    end
  elseif self.phase == "ending" then
    if self.phaseT >= sim.ENDING_TIME then
      self.phase, self.over = "over", true
      self:cue("over")
    end
  end
end

-- After a break: the next card.
function Game:advance()
  if self.phase == "break" then self:deal(self.cur.index + 1) end
end

return sim
