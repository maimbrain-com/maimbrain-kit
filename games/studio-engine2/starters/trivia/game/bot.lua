-- The difficulty bot (the `difficulty` tool): plays like a person at a quiz.
-- The clock it beats is reading time (the question at its reading speed,
-- the answers scanned at twice that, from when the card starts to flip in)
-- plus a moment to decide plus a reaction. For each question it either
-- knows the answer (a chance by tier) and picks it after a short think, now
-- and then tapping the wrong one by mistake, or it guesses after a longer
-- think (sometimes ruling one wrong answer out first). When its plan would
-- land after the buzzer it either taps what it leans toward in the last
-- second ("clutch") or freezes and times out. Better players read faster
-- and know more. These are the trivia kit's bot and numbers (its
-- bot-report.tsv is the target; its "decent" player is casual here).
--
-- The engine then adds the hand: the thumb's travel to the button and
-- where on it it lands, and timing noise.

local shared = require("shared")

local SKILLS = {
  ["first-timer"] = { know = { 0.86, 0.6, 0.42 }, wps = 3.2, known = { 0.4, 1.0 }, guess = { 1.5, 3.5 }, react = { 0.2, 0.35 }, eliminate = 0.25, slip = 0.03, clutch = 0.5 },
  casual = { know = { 0.92, 0.74, 0.56 }, wps = 4.0, known = { 0.3, 0.8 }, guess = { 1.2, 3.0 }, react = { 0.17, 0.3 }, eliminate = 0.4, slip = 0.02, clutch = 0.7 },
  good = { know = { 0.98, 0.9, 0.76 }, wps = 5.2, known = { 0.2, 0.55 }, guess = { 1.0, 2.4 }, react = { 0.15, 0.25 }, eliminate = 0.6, slip = 0.01, clutch = 0.9 },
  expert = { know = { 0.995, 0.96, 0.88 }, wps = 6.4, known = { 0.15, 0.4 }, guess = { 0.8, 2.0 }, react = { 0.13, 0.2 }, eliminate = 0.75, slip = 0.005, clutch = 0.95 },
}
local DEAL_TIME = 0.5 -- sim.DEAL_TIME: reading starts as the card begins to flip in

local skill -- the preset's (act.preset), from the first frame
-- Its own random numbers (never the game's), different each round.
local rng = love.math.newRandomGenerator(os.time() % 2147483646 + 1)
local plannedFor -- the question (its game.cur table) the plan is for
local plan -- { at = the sim's clock (game.t) to tap at, slot = 1–4 }, or false: freeze

local function range(r) return r[1] + rng:random() * (r[2] - r[1]) end

local function words(s)
  local n = 0
  for _ in string.gmatch(s, "%S+") do n += 1 end
  return n
end

local function think(game)
  local d = game.cur
  local knows = rng:random() < skill.know[d.tier]
  local answers = 0
  for _, a in d.q.answers do answers += words(a) end
  local read = (words(d.q.text) + answers * 0.5) / skill.wps * (0.85 + rng:random() * 0.35)
  local decide = knows and range(skill.known) or range(skill.guess)
  -- Seconds into the clock the tap lands (reading began as the card flipped in).
  local at = read + decide + range(skill.react) - DEAL_TIME * 0.5
  local right = game:rightSlot()
  local slot
  if knows then
    slot = rng:random() < skill.slip and (right + rng:random(1, 3) - 1) % 4 + 1 or right
  else
    -- A guess: maybe rule one wrong answer out, then pick among the rest.
    local n = rng:random() < skill.eliminate and 3 or 4
    local k = rng:random(0, n - 1)
    slot = k == 0 and right or (right + k - 1) % 4 + 1
  end
  if at > d.limit - 0.15 then
    if rng:random() < skill.clutch then
      -- The ring is nearly empty: tap whatever they lean toward.
      at = d.limit - (0.15 + rng:random() * 0.75)
      if not knows or rng:random() < 0.5 then slot = (right + rng:random(0, 3) - 1) % 4 + 1 end
    else
      return false
    end
  end
  local askStart = game.t + (game.phase == "deal" and DEAL_TIME - game.phaseT or -game.phaseT)
  return { at = askStart + math.max(at, 0.05), slot = slot }
end

function bot(act)
  skill = skill or SKILLS[act.preset] or SKILLS.good
  local game, L = shared.game, shared.layout
  if not game or round.ended then return end
  if game.phase ~= "deal" and game.phase ~= "ask" then return end
  if plannedFor ~= game.cur then
    plannedFor = game.cur
    plan = think(game)
  end
  if not plan then return end
  local a = L.answers[plan.slot]
  act.tap(a.x, a.y, { size = a.h, at = math.max(plan.at - game.t, 0) })
end
