-- How a player plays Cake Keep, for the difficulty bot (docs/STUDIO-2-API.md
-- [bot]), as the tower-defense kit's bot does: it reads the round from
-- game/shared.lua (what's on screen: coins, towers, the jellies, the next
-- wave) and builds with game/planner.lua's sensible plan (POP first, a
-- BOOM once armor is coming, a CHILL at three towers, the best empty pad
-- by road covered, then upgrades when they're better value per coin), as
-- taps: a pad, then the tower in the picker; a tower, then its arrow; the
-- next-wave button to call a wave early. The first tower follows the
-- game's hint (the pulsing pad, POP in the picker).
--
-- The person on top is the kit's: a player glances at the coins only every
-- so often (they're watching the jellies), builds on a random pad or picks
-- a random tower now and then, finds upgrades late, gets distracted, and
-- only a good player calls waves early. The engine adds the rest
-- (reaction, look-aways, where each tap lands: a tap that misses a picker
-- button closes the picker and costs another try).
--
-- Which kind of player it is: the engine doesn't tell a policy its preset,
-- so the bot works it out as a player's habits would show, from how long
-- its taps take to land (reaction plus the move, by Fitts' law, from the
-- engine's published presets) and plays that preset's habits from the
-- kit's bot (src/tests.rs: FIRST_TIMER, DECENT, GOOD).

local sim = require("sim")
local planner = require("planner")
local layout = require("layout")
local shared = require("shared")

-- The kit's players: seconds between glances at the coins, the chance of a
-- random pad or a random tower kind, when upgrades occur to them, whether
-- they call waves early, and distractions (chance per action, seconds).
local HABITS = {
  ["first-timer"] = { glance = { 3, 8 }, wrongPad = 0.5, wrongKind = 0.5, upgradeAfter = 60, callEarly = false, lapse = 0.35, lapseLen = { 3, 8 } },
  casual = { glance = { 2, 5 }, wrongPad = 0.3, wrongKind = 0.25, upgradeAfter = 35, callEarly = false, lapse = 0.15, lapseLen = { 1.5, 4 } },
  good = { glance = { 0.3, 0.8 }, wrongPad = 0.05, wrongKind = 0.03, upgradeAfter = 0, callEarly = true, lapse = 0.03, lapseLen = { 0.5, 1.5 } },
  expert = { glance = { 0.2, 0.5 }, wrongPad = 0.02, wrongKind = 0.01, upgradeAfter = 0, callEarly = true, lapse = 0.01, lapseLen = { 0.5, 1 } },
}
-- The engine's presets (sdk input/synth/skill.rs): reaction (mean), and a
-- finger's Fitts' law: a, throughput (bits/s), floor (units).
local PRESETS = {
  { name = "first-timer", reaction = 0.42, a = 0.18, tp = 4.5, floor = 11 },
  { name = "casual", reaction = 0.34, a = 0.14, tp = 5.5, floor = 9.5 },
  { name = "good", reaction = 0.28, a = 0.12, tp = 6.5, floor = 8.5 },
  { name = "expert", reaction = 0.23, a = 0.1, tp = 7.5, floor = 7.5 },
}
-- Taps measured before it trusts what it's seen (it plays as casual till then).
local MEASURED = 3

local PAD_SIZE = sim.PAD
local SLOT_SIZE = 54
local WAVE_SIZE = 46

local function predicted(p, d, size)
  local usable = math.max(math.sqrt(math.max(size * size - (4.133 * p.floor) ^ 2, 0)), size / 8)
  return p.reaction + p.a + math.log(d / usable + 1, 2) / p.tp
end

local err = { 0, 0, 0, 0 }
local measured = 0
local habits = HABITS.casual

-- A tap it has asked for and is watching: where the press count was, when
-- it was asked (and how far the finger had to go), and when it gives up.
local pending
-- The action under way: { what, pad, kind, tries, level } (level: the tower's before an upgrade).
local doing
local wait = 0.4 -- until the next glance at the board
local rng = nil :: any
local last = -1 -- when the last tap was wanted

local function range(r) return r[1] + (r[2] - r[1]) * rng:random() end

local function measure(p)
  local lag = shared.pressedAt - p.asked
  if lag <= 0 or lag > 3 then return end
  for i, ps in PRESETS do
    local want = predicted(ps, p.d, p.size)
    -- A long look-away isn't skill: count at most half a second over.
    local e = math.min(lag - want, 0.5)
    err[i] += e * e
  end
  measured += 1
  if measured >= MEASURED then
    local best = 1
    for i = 2, #PRESETS do
      if err[i] < err[best] then best = i end
    end
    habits = HABITS[PRESETS[best].name]
  end
end

-- Asks for a tap on (x, y), a target `size` across, `at` seconds from now;
-- `new` false for the picker's second tap, which the player already has in
-- mind (no fresh reaction, only the move).
local function tap(act, x, y, size, at, new)
  local fx, fy = act.finger()
  local now = round.time
  -- The engine takes presses wanted within 0.25 s of each other for one
  -- (a policy asking every frame presses once): keep them apart.
  at = math.max(at, last + 0.26 - now)
  last = now + at
  pending = { presses = shared.presses, asked = now + at, d = math.sqrt((x - fx) ^ 2 + (y - fy) ^ 2), size = size, quit = now + at + 3, clean = at == 0 and new }
  act.tap(x, y, { size = size, at = at, new = new })
end

local function towerKind(g, kind)
  if rng:random() < habits.wrongKind then return sim.KINDS[math.floor(rng:random() * 3) + 1] end
  return kind
end

local function emptyPad(g)
  local v = {}
  for p = 1, #sim.PADS do
    if not g.towers[p] then v[#v + 1] = p end
  end
  return v[math.floor(rng:random() * #v) + 1]
end

-- What to do now (the kit's Bot::think), or nil.
local function decide(g)
  local style = { maxTowers = 99, padsBelow = 0, upgrades = g.t >= habits.upgradeAfter, callEarly = habits.callEarly }
  local a = planner.want(g, style)
  if not a then return nil end
  if g.built == 0 then
    -- The first tower follows the hint.
    a = { what = "build", pad = sim.HINT_PAD, kind = "pop" }
  elseif a.what == "build" then
    local pad = if rng:random() < habits.wrongPad then emptyPad(g) else a.pad
    a = { what = "build", pad = pad, kind = towerKind(g, a.kind) }
  end
  if planner.cost(g, a) > g.coins then return nil end -- saving up
  return a
end

function bot(act)
  local g = shared.game
  if not g or not shared.live or not g:playing() then return end
  -- Its own numbers (the round's sim has its own generator; nothing else draws from love.math after love.load).
  if not rng then rng = love.math.newRandomGenerator(love.math.random(1, 2147483646)) end
  local now = round.time
  local sel = shared.sel
  -- One tap at a time: wait for it to land (or give up on it).
  if pending then
    if shared.presses > pending.presses then
      if pending.clean then measure(pending) end
      pending = nil
    elseif now > pending.quit then
      pending, doing = nil, nil
    else
      return
    end
  end
  if doing then
    local d = doing
    if d.what == "call" then
      doing = nil
      wait = range(habits.glance) * 0.5
      return
    end
    if d.what == "build" and g.towers[d.pad] then
      doing = nil
      wait = range(habits.glance) * 0.5
      return
    end
    if d.what == "upgrade" and (g.towers[d.pad] == nil or g.towers[d.pad].level > d.level) then
      doing = nil
      wait = range(habits.glance) * 0.5
      return
    end
    d.tries += 1
    if d.tries > 6 or planner.cost(g, d) > g.coins then
      doing = nil
      return
    end
    local open = if d.what == "build" then "pad" else "tower"
    if sel.what == open and sel.pad == d.pad then
      -- The picker (or the arrow) is open: the second tap.
      if d.what == "build" then
        local s = layout.pickerSlots(d.pad)[table.find(sim.KINDS, d.kind) or 1]
        tap(act, s[1], s[2], SLOT_SIZE, 0, false)
      else
        local x, y = layout.upgradeSlot(d.pad)
        tap(act, x, y, SLOT_SIZE, 0, false)
      end
    else
      local p = sim.PADS[d.pad]
      tap(act, p[1], p[2], PAD_SIZE, 0, true)
    end
    return
  end
  wait -= love.timer.getDelta()
  if wait > 0 then return end
  wait = range(habits.glance)
  local a = decide(g)
  if not a then return end
  doing = { what = a.what, pad = a.pad, kind = a.kind, tries = 1, level = if a.pad and g.towers[a.pad] then g.towers[a.pad].level else 0 }
  -- Distracted (watching the jellies) before getting to it.
  local at = if rng:random() < habits.lapse then range(habits.lapseLen) else 0
  if a.what == "call" then
    local x, y, w, h = layout.waveButton()
    tap(act, x + w / 2, y + h / 2, WAVE_SIZE, at, true)
  else
    local p = sim.PADS[a.pad]
    tap(act, p[1], p[2], PAD_SIZE, at, true)
  end
end
