-- A sensible tower-building heuristic: the title card plays with it, the
-- hints point where it would build, and game/bot.lua plays like a person
-- on top of it. POP first, a BOOM once armor is coming, a CHILL at three
-- towers, the best empty pad by how much road it covers, then upgrades
-- when they're better value per coin.

local sim = require("sim")

local planner = {}
planner.__index = planner

-- Road covered by each pad at each tower kind's first-level range (worked
-- out once: it's the same every round).
local COVER = {}
for p = 1, #sim.PADS do
  COVER[p] = {}
  for _, k in sim.KINDS do
    COVER[p][k] = sim.coverage(p, sim.TOWERS[k].range[1])
  end
end

function planner.coverage(pad, kind)
  return COVER[pad][kind]
end

-- Empty pads, best first for `kind`.
function planner.padsFor(g, kind)
  local v = {}
  for p = 1, #sim.PADS do
    if not g.towers[p] then v[#v + 1] = p end
  end
  table.sort(v, function(a, b)
    local ca, cb = COVER[a][kind], COVER[b][kind]
    if ca ~= cb then return ca > cb end
    return a < b
  end)
  return v
end

-- The tower kind a sensible player adds next.
function planner.nextKind(g)
  local n = { pop = 0, chill = 0, boom = 0 }
  for p = 1, #sim.PADS do
    local t = g.towers[p]
    if t then n[t.kind] += 1 end
  end
  local total = n.pop + n.chill + n.boom
  local tough = g.wave >= 4
  for _, grp in g.next.groups do
    if grp[1] == "helmet" or grp[1] == "splitter" or grp[1] == "king" then tough = true end
  end
  if total == 0 then return "pop" end
  if tough and n.boom == 0 then return "boom" end
  if total >= 3 and n.chill == 0 then return "chill" end
  if tough and n.boom * 2 < n.pop then return "boom" end
  return "pop"
end

-- Rough damage per second a tower adds, for comparing options.
local function dps(kind, level)
  local s = sim.TOWERS[kind]
  local base = s.damage[level] / s.interval[level]
  if kind == "chill" then return base * 2.5 + (1 - s.special[level]) * 2 end
  if kind == "boom" then return base * 2 end
  return base
end

-- An action: { what = "build", pad, kind } | { what = "upgrade", pad } |
-- { what = "call" }. Its price in coins:
function planner.cost(g, a)
  if a.what == "build" then return sim.TOWERS[a.kind].cost[1] end
  if a.what == "upgrade" then return g:upgradeCost(a.pad) or math.huge end
  return 0
end

-- Does it to the round; true if it happened.
function planner.apply(g, a)
  if a.what == "build" then return g:build(a.pad, a.kind) end
  if a.what == "upgrade" then return g:upgrade(a.pad) end
  return g:callWave() ~= nil
end

-- What to do next (which may need saving up for), or nil. `style`:
--   maxTowers   stop building past this many
--   padsBelow   only build on pads at least this far down the screen
--   upgrades    whether to upgrade
--   callEarly   call waves early when the board is calm and the money's spent
function planner.want(g, style)
  if not g:playing() then return nil end
  local towers = g:towerCount()
  local kind = planner.nextKind(g)
  local build
  if towers < style.maxTowers then
    for _, p in planner.padsFor(g, kind) do
      if sim.PADS[p][2] >= style.padsBelow then
        build = p
        break
      end
    end
  end
  -- Value per coin of building vs the best upgrade.
  local buildValue = build and COVER[build][kind] * dps(kind, 1) / sim.TOWERS[kind].cost[1]
  local up, upValue
  if style.upgrades and towers >= 3 then
    for p = 1, #sim.PADS do
      local t, cost = g.towers[p], g:upgradeCost(p)
      if t and cost then
        local gain = dps(t.kind, t.level + 1) - dps(t.kind, t.level)
        local v = COVER[p][t.kind] * gain / cost
        if not up or v > upValue then up, upValue = p, v end
      end
    end
  end
  local action
  if build and up then
    if towers < 4 or buildValue * 1.1 >= upValue then action = { what = "build", pad = build, kind = kind } else action = { what = "upgrade", pad = up } end
  elseif build then
    action = { what = "build", pad = build, kind = kind }
  elseif up then
    action = { what = "upgrade", pad = up }
  end
  -- Calling early: only with the board calm, full hearts and money spent.
  if style.callEarly and g.wave >= 2 and g:pendingSpawns() == 0 and g.hearts == sim.HEARTS and g.nextIn > 3 then
    local far = true
    for _, c in g.creeps do
      if c.d >= sim.ROAD_LEN * 0.3 then far = false end
    end
    local broke = action == nil or planner.cost(g, action) > g.coins + 20
    if far and broke and #g.creeps < 4 then return { what = "call" } end
  end
  return action
end

return planner
