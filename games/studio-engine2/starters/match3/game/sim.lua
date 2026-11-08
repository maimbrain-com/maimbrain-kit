-- The rules: a 7×8 board of poppets, swap two neighbours to line up three or
-- more of a kind, matches pop, the rest fall, new ones drop in. A wake meter
-- drains all the time (faster and faster); pops refill it. Empty = asleep =
-- the round is over.
--
-- Pure game state: no drawing, no sound. Input comes in as sim:swap(a, b),
-- time as sim:step(dt); what happened goes out as sim.cues, for main.lua's
-- sound and juice. The board resolves in steps (swap, pop, fall, pop, …,
-- ready), and each step lasts a fixed time that draw.lua animates (the
-- phase and its timer), so the same seed and the same swaps at the same
-- times give the same round.
--
-- A cell is a number from 1 to COLS × ROWS, row by row from the top left:
-- sim.col(i) and sim.row(i) are its column and row, counting from 0.

local sim = {}

-- ---- Tuning knobs -----------------------------------------------------------
-- Board size in cells. Wider boards have more moves and longer cascades.
local COLS = 7
local ROWS = 8
-- Kinds of poppet in play at the start. Fewer kinds = more matches and
-- cascades (5 is generous; 6 is a lot harder to read).
local START_COLORS = 5
-- The wake meter is 0…1. Drain per second at the start of a round.
local DRAIN_START = 0.042
-- Drain added per second of play (the meter empties faster and faster).
local DRAIN_GROWTH = 0.0016
-- The drain stops growing here (bar fractions per second).
local DRAIN_MAX = 0.3
-- Meter refilled per poppet popped. Raise it and every round gets longer.
local GAIN_PIECE = 0.0175
-- Each cascade step multiplies the refill by (1 + GAIN_CHAIN × step).
local GAIN_CHAIN = 0.5
-- Refill for making a special (line, bomb, rainbow) and for a rock broken.
local GAIN_MADE = 0.04
local GAIN_ROCK = 0.03
-- Refill for popping a star poppet (from level 6).
local GAIN_STAR = 0.16
-- Below this the meter is in danger (a cue, the tension music, faces nodding off).
local DROWSY = 0.25
-- Faces start to droop below this.
local DROWSY_FROM = 0.45
-- Saved from below this: "PHEW!".
local PHEW_BELOW = 0.1
-- Seconds per level. Every level brings something new (sim.news).
local LEVEL_TIME = 15
-- Animation timings the sim waits for (seconds; the fall is gravity in cells/s²).
local SWAP_TIME = 0.13
local POP_TIME = 0.2
local FALL_G = 80
local LAND_SETTLE = 0.1
local SHUFFLE_TIME = 0.75
-- Between the meter running out and the game-over card.
local ENDING = 1.7
-- A blast reaches cells this much later per cell of distance (a wave).
local BLAST_STAGGER = 0.035
-- A swipe made while the board is still settling is kept this long.
local QUEUE_TIME = 0.3
-- Rocks: chance per new poppet once rocks are on, and the most on the board.
local ROCK_CHANCE = 0.05
local ROCKS_MAX = 4
-- Chance a new poppet is a star once stars are on.
local STAR_CHANCE = 0.03
-- Points per poppet popped, times the cascade step.
local POINTS_PIECE = 10

local N = COLS * ROWS
local abs, max, min, floor = math.abs, math.max, math.min, math.floor

-- ---- Cells and pieces -------------------------------------------------------

local function col(i) return (i - 1) % COLS end
local function row(i) return (i - 1) // COLS end
local function cell(c, r) return r * COLS + c + 1 end

local function adjacent(a, b)
  return abs(col(a) - col(b)) + abs(row(a) - row(b)) == 1
end

-- Cells apart, counting diagonals as one (a blast's wave).
local function dist(a, b)
  return max(abs(col(a) - col(b)), abs(row(a) - row(b)))
end

-- A piece: kind is "poppet" (with color 1…7), "rainbow" (made from 5 in a
-- row) or "rock" (can't move or match; a match next to it or a blast breaks
-- it). A poppet can carry a special: "lineH" or "lineV" (made from 4 in a
-- row: pops its row or column) or "bomb" (an L or T: pops the 3×3 around
-- it). A star poppet is a big refill. id is unique (faces blink on their
-- own clock); fromX, fromY are where it was drawn when a fall or a shuffle
-- began (in cells).
local function isSpecial(p)
  return p.special ~= nil or p.kind == "rainbow"
end

local function colorOf(p)
  if p and p.kind == "poppet" then return p.color end
  return nil
end

local function copy(p)
  return { kind = p.kind, color = p.color, special = p.special, star = p.star, id = p.id, fromX = p.fromX, fromY = p.fromY }
end

-- ---- Matching ---------------------------------------------------------------

-- Runs of 3 or more of one color: rows, then columns.
local function runs(b)
  local out = {}
  for r = 0, ROWS - 1 do
    local c = 0
    while c < COLS do
      local k = colorOf(b[cell(c, r)])
      local e = c + 1
      if k then
        while e < COLS and colorOf(b[cell(e, r)]) == k do e += 1 end
        if e - c >= 3 then
          local cells = {}
          for x = c, e - 1 do cells[#cells + 1] = cell(x, r) end
          out[#out + 1] = { cells = cells, horiz = true }
        end
      end
      c = e
    end
  end
  for c = 0, COLS - 1 do
    local r = 0
    while r < ROWS do
      local k = colorOf(b[cell(c, r)])
      local e = r + 1
      if k then
        while e < ROWS and colorOf(b[cell(c, e)]) == k do e += 1 end
        if e - r >= 3 then
          local cells = {}
          for y = r, e - 1 do cells[#cells + 1] = cell(c, y) end
          out[#out + 1] = { cells = cells, horiz = false }
        end
      end
      r = e
    end
  end
  return out
end

-- Every match on the board, runs that share a cell merged into one group:
-- { cells, color, longest, longestHoriz, mid (the middle of the longest
-- run), cross (a cell in both a row run and a column run: an L, T or +) }.
local function findGroups(b)
  local rs = runs(b)
  local n = #rs
  if n == 0 then return rs end
  local parent = {}
  for i = 1, n do parent[i] = i end
  local function root(i)
    while parent[i] ~= i do
      parent[i] = parent[parent[i]]
      i = parent[i]
    end
    return i
  end
  -- A column run joins the row run through any of its cells.
  local across = {}
  for i = 1, n do
    if rs[i].horiz then
      for _, c in rs[i].cells do across[c] = i end
    end
  end
  for j = 1, n do
    if not rs[j].horiz then
      for _, c in rs[j].cells do
        local i = across[c]
        if i then parent[root(i)] = root(j) end
      end
    end
  end
  local out = {}
  for i = 1, n do
    if root(i) == i then
      local cells, seen = {}, {}
      local longest, horiz, mid, cross = 0, true, 1, nil
      for j = 1, n do
        if root(j) == i then
          local run = rs[j]
          for _, c in run.cells do
            if seen[c] then
              cross = c
            else
              seen[c] = true
              cells[#cells + 1] = c
            end
          end
          if #run.cells > longest then
            longest, horiz = #run.cells, run.horiz
            mid = run.cells[#run.cells // 2 + 1]
          end
        end
      end
      out[#out + 1] = { cells = cells, color = colorOf(b[cells[1]]) or 1, longest = longest, longestHoriz = horiz, mid = mid, cross = cross }
    end
  end
  return out
end

-- What a group makes: 5 in a row a rainbow, an L/T a bomb, 4 a line blaster.
-- Returns the kind and the special, or nil.
local function makeOf(g)
  if g.longest >= 5 then
    return "rainbow", nil
  elseif g.cross then
    return "poppet", "bomb"
  elseif g.longest == 4 then
    return "poppet", g.longestHoriz and "lineH" or "lineV"
  end
  return nil, nil
end

-- Does the piece at i sit in a line of 3?
local function matchAt(b, i)
  local k = colorOf(b[i])
  if not k then return false end
  local x, y = col(i), row(i)
  local n = 1
  local c = x + 1
  while c < COLS and colorOf(b[cell(c, y)]) == k do n += 1; c += 1 end
  c = x - 1
  while c >= 0 and colorOf(b[cell(c, y)]) == k do n += 1; c -= 1 end
  if n >= 3 then return true end
  n = 1
  local r = y + 1
  while r < ROWS and colorOf(b[cell(x, r)]) == k do n += 1; r += 1 end
  r = y - 1
  while r >= 0 and colorOf(b[cell(x, r)]) == k do n += 1; r -= 1 end
  return n >= 3
end

-- Is there a match anywhere?
local function anyMatch(b)
  for i = 1, N do
    if matchAt(b, i) then return true end
  end
  return false
end

-- Whether swapping a and c does something (a match, a rainbow, two specials).
local function swapValid(b, a, c)
  local pa, pc = b[a], b[c]
  if not pa or not pc or not adjacent(a, c) or pa.kind == "rock" or pc.kind == "rock" then
    return false
  end
  if pa.kind == "rainbow" or pc.kind == "rainbow" or (isSpecial(pa) and isSpecial(pc)) then
    return true
  end
  b[a], b[c] = pc, pa
  local ok = matchAt(b, a) or matchAt(b, c)
  b[a], b[c] = pa, pc
  return ok
end

-- Every useful swap on the board (each pair once): { a, b } cells.
local function moves(b)
  local out = {}
  for i = 1, N do
    if col(i) < COLS - 1 and swapValid(b, i, i + 1) then out[#out + 1] = { i, i + 1 } end
    if row(i) < ROWS - 1 and swapValid(b, i, i + COLS) then out[#out + 1] = { i, i + COLS } end
  end
  return out
end

local function hasMove(b)
  for i = 1, N do
    if col(i) < COLS - 1 and swapValid(b, i, i + 1) then return true end
    if row(i) < ROWS - 1 and swapValid(b, i, i + COLS) then return true end
  end
  return false
end

-- ---- Resolving --------------------------------------------------------------

-- The cells a special covers when it goes off at `at`: kind is "row", "col",
-- "area" (a square n cells out), "cross" (rows and columns n either side).
local function area(at, kind, n)
  local c0, r0 = col(at), row(at)
  local out = {}
  for i = 1, N do
    local dx, dy = abs(col(i) - c0), abs(row(i) - r0)
    local yes
    if kind == "row" then yes = dy == 0
    elseif kind == "col" then yes = dx == 0
    elseif kind == "area" then yes = dx <= n and dy <= n
    elseif kind == "cross" then yes = dx <= n or dy <= n
    else yes = true end
    if yes then out[#out + 1] = i end
  end
  return out
end

-- The color with the most poppets (rainbows hit by a blast take it).
local function commonest(b, skip)
  local count = {}
  for i = 1, N do
    local k = colorOf(b[i])
    if k and not skip[i] then count[k] = (count[k] or 0) + 1 end
  end
  local best, most = 1, -1
  for k = 1, 7 do
    if (count[k] or 0) > most then best, most = k, count[k] or 0 end
  end
  return best
end

-- Pops whatever the board (just swapped `swap`, if a player move) has
-- matched, fires the specials caught in it, makes new specials, and removes
-- the popped pieces (leaving holes for collapse). `ids` hands out piece ids
-- (ids.next) and a random number (ids.random()). Returns the step:
--   popping  { at, piece, delay (when it pops), to (flies into the special its group made) }
--   blasts   { kind, n, at, delay, color, targets (a rainbow's zaps) }
--   made     { at, piece }   groups { x, y (in cells), points }
--   pieces, rocks, stars, fires, combo, dur
local function resolve(b, ids, swap, chain)
  local st = { popping = {}, blasts = {}, made = {}, groups = {}, pieces = 0, rocks = 0, stars = 0, fires = 0, combo = false, dur = 0 }
  local hit, delay, fly, done, protect, queue = {}, {}, {}, {}, {}, {}
  local function hitCell(c, d)
    if hit[c] then
      delay[c] = min(delay[c], d)
    else
      hit[c], delay[c] = true, d
    end
  end

  -- Swapping a rainbow or two specials: a combo instead of a match.
  if swap then
    local a, to = swap[1], swap[2]
    local pm, po = b[to], b[a] -- the moved piece is now at `to`
    if pm.kind == "rainbow" or po.kind == "rainbow" then
      st.combo = true
      local rc, oc, o = a, to, pm
      if pm.kind == "rainbow" then rc, oc, o = to, a, po end
      hitCell(rc, 0)
      done[rc] = true
      if o.kind == "rainbow" then
        done[oc] = true
        for c = 1, N do hitCell(c, dist(c, to) * BLAST_STAGGER) end
        st.blasts[#st.blasts + 1] = { kind = "board", n = 0, at = to, delay = 0 }
        st.fires += 2
      elseif o.kind == "poppet" then
        local k = o.color
        local targets = {}
        for c = 1, N do
          local p = b[c]
          if p and colorOf(p) == k then
            local d = 0.08 + dist(c, rc) * BLAST_STAGGER
            if o.special and not p.special then
              -- Every poppet of that color becomes the special, then they all go off.
              local q = copy(p)
              if o.special == "bomb" then
                q.special = "bomb"
              else
                q.special = ids.random() < 0.5 and "lineH" or "lineV"
              end
              b[c] = q
            end
            hitCell(c, d)
            targets[#targets + 1] = c
          end
        end
        st.blasts[#st.blasts + 1] = { kind = "zap", n = 0, at = rc, delay = 0, color = k, targets = targets }
        st.fires += 1
      end
    elseif isSpecial(pm) and isSpecial(po) then
      st.combo = true
      done[a], done[to] = true, true
      hitCell(a, 0)
      hitCell(to, 0)
      local bombs = (pm.special == "bomb" and 1 or 0) + (po.special == "bomb" and 1 or 0)
      local kind, n = "cross", bombs -- line+line a cross, line+bomb three rows and columns
      if bombs == 2 then kind, n = "area", 2 end -- bomb+bomb a 5×5
      for _, c in area(to, kind, n) do hitCell(c, dist(c, to) * BLAST_STAGGER) end
      st.blasts[#st.blasts + 1] = { kind = kind, n = n, at = to, delay = 0, color = colorOf(pm) }
      st.fires += 2
    end
  end

  -- Matches, and the specials they make.
  if not st.combo then
    for _, g in findGroups(b) do
      local kind, special = makeOf(g)
      local at
      if kind then
        local inGroup = {}
        for _, c in g.cells do inGroup[c] = true end
        local where
        if swap and inGroup[swap[2]] then
          where = swap[2]
        elseif swap and inGroup[swap[1]] then
          where = swap[1]
        else
          where = g.cross or g.mid
        end
        if not protect[where] then
          protect[where] = true
          -- A special already there goes off before it's replaced.
          if b[where] and isSpecial(b[where]) then queue[#queue + 1] = { where, 0 } end
          local piece = { kind = kind, color = kind == "poppet" and g.color or nil, special = special, star = false, id = ids.next, fromX = col(where), fromY = row(where) }
          ids.next += 1
          st.made[#st.made + 1] = { at = where, piece = piece }
          at = where
        end
      end
      local sx, sy = 0, 0
      for _, c in g.cells do
        sx += col(c)
        sy += row(c)
        if c ~= at then
          hitCell(c, 0)
          fly[c] = at
        end
        -- Rocks next to a match crack.
        local x, y = col(c), row(c)
        if x > 0 and b[c - 1] and b[c - 1].kind == "rock" then hitCell(c - 1, 0.05) end
        if x < COLS - 1 and b[c + 1] and b[c + 1].kind == "rock" then hitCell(c + 1, 0.05) end
        if y > 0 and b[c - COLS] and b[c - COLS].kind == "rock" then hitCell(c - COLS, 0.05) end
        if y < ROWS - 1 and b[c + COLS] and b[c + COLS].kind == "rock" then hitCell(c + COLS, 0.05) end
      end
      local n = #g.cells
      st.groups[#st.groups + 1] = { x = sx / n, y = sy / n, points = n * POINTS_PIECE * chain }
    end
  end

  -- Specials caught in the pop go off, and set off others: a chain reaction.
  for c = 1, N do
    if hit[c] and not done[c] and not protect[c] and b[c] and isSpecial(b[c]) then
      queue[#queue + 1] = { c, delay[c] }
    end
  end
  local qi = 1
  while qi <= #queue do
    local at, t0 = queue[qi][1], queue[qi][2]
    qi += 1
    local p = b[at]
    if not done[at] and p then
      done[at] = true
      local kind, n, targets, color
      if p.kind == "rainbow" then
        color = commonest(b, hit)
        kind, n, targets = "zap", 0, {}
        for c = 1, N do
          if colorOf(b[c]) == color then targets[#targets + 1] = c end
        end
      else
        kind, n = "area", 1
        if p.special == "lineH" then kind = "row" elseif p.special == "lineV" then kind = "col" end
        targets, color = area(at, kind, n), p.color
      end
      for _, c in targets do
        if not protect[c] and c ~= at then
          local d = t0 + dist(c, at) * BLAST_STAGGER
          local fresh = not hit[c]
          hitCell(c, d)
          if fresh and not done[c] and b[c] and isSpecial(b[c]) then queue[#queue + 1] = { c, d } end
        end
      end
      st.fires += 1
      st.blasts[#st.blasts + 1] = { kind = kind, n = n, at = at, delay = t0, color = color, targets = kind == "zap" and targets or nil }
    end
  end

  -- Take the popped pieces off the board; put the new specials on it.
  local longest = 0
  for c = 1, N do
    local p = b[c]
    if hit[c] and not protect[c] and p then
      local d = delay[c]
      longest = max(longest, d)
      st.popping[#st.popping + 1] = { at = c, piece = p, delay = d, to = fly[c] }
      if p.kind == "rock" then st.rocks += 1 else st.pieces += 1 end
      if p.star then st.stars += 1 end
      b[c] = false
    end
  end
  for _, m in st.made do b[m.at] = m.piece end
  st.dur = POP_TIME + longest
  return st
end

-- Seconds a piece takes to fall d cells.
local function fallTime(d)
  if d <= 0 then return 0 end
  return math.sqrt(2 * d / FALL_G)
end

-- Where a piece falling from row `from` to row `to` is, t seconds into the
-- fall, and how long it has been down (negative while still falling).
function sim.fallY(from, to, t)
  local ft = fallTime(to - from)
  if t >= ft then return to, t - ft end
  return from + 0.5 * FALL_G * t * t, t - ft
end

-- ---- Levels -----------------------------------------------------------------

-- What a level brings when it arrives (level 1 is the start):
-- "faster", "newColor", "rocks", "stars" or "moreRocks".
function sim.news(level)
  if level == 3 then return "newColor" end
  if level == 4 then return "rocks" end
  if level == 6 then return "stars" end
  if level == 8 then return "moreRocks" end
  return "faster"
end

-- ---- The round --------------------------------------------------------------

local Sim = {}
Sim.__index = Sim

-- A new round's board (no matches, at least one move). `demo` is the title
-- card's: no drain, no levels.
function sim.new(seed, demo)
  local s = setmetatable({
    board = {},
    rng = love.math.newRandomGenerator(seed),
    -- Where the board is in its turn: "ready" (waiting for a swap), "swap"
    -- (two pieces sliding past each other; valid ones pop next, others
    -- bounce back), "bounce", "pop" (last.popping popping, each after its
    -- delay), "fall" (everything falling into place, sim.fallY from each
    -- piece's fromY), "shuffle" (no moves left: the pieces fly to new places)
    -- or "asleep" (the meter ran out: everyone falls asleep, then over).
    -- pt is the time in it; pa, pb and pvalid the swap's.
    phase = "ready",
    pt = 0, pdur = 0, pa = 0, pb = 0, pvalid = false,
    t = 0, -- seconds of play
    level = 1,
    energy = 1, -- 0…1; empty = asleep
    score = 0,
    chain = 0, -- the cascade step of the current move (0 while ready)
    bestChain = 0,
    colors = START_COLORS,
    rocksOn = false, rocksMax = ROCKS_MAX, rocksDue = 0,
    starsOn = false,
    last = nil, -- the last resolution step (what's popping now)
    moves = {}, -- useful swaps while ready; hint is the best-looking one
    hint = nil,
    settles = 0,
    swaps = 0,
    cues = {}, -- what happened this frame, for sound and juice
    over = false,
    demo = demo,
    queued = nil,
    nextId = 1,
  }, Sim)
  for i = 1, N do s.board[i] = false end
  s:fill()
  s:settle()
  return s
end

function Sim:piece(kind, color)
  local id = self.nextId
  self.nextId += 1
  return { kind = kind, color = color, special = nil, star = false, id = id, fromX = 0, fromY = 0 }
end

-- A fresh board with no matches and at least one move.
function Sim:fill()
  local b = self.board
  repeat
    for i = 1, N do
      local c, r = col(i), row(i)
      local k = self.rng:random(self.colors)
      for _ = 1, self.colors do
        local left = c >= 2 and colorOf(b[i - 1]) == k and colorOf(b[i - 2]) == k
        local up = r >= 2 and colorOf(b[i - COLS]) == k and colorOf(b[i - 2 * COLS]) == k
        if not left and not up then break end
        k = k % self.colors + 1
      end
      local p = self:piece("poppet", k)
      p.fromX, p.fromY = c, r
      b[i] = p
    end
  until hasMove(b)
end

-- A new poppet for the top of a column.
function Sim:spawn()
  if self.rocksOn then
    local rocks = 0
    for i = 1, N do
      if self.board[i] and self.board[i].kind == "rock" then rocks += 1 end
    end
    if rocks < self.rocksMax and (self.rocksDue > 0 or self.rng:random() < ROCK_CHANCE) then
      self.rocksDue = max(0, self.rocksDue - 1)
      return self:piece("rock", nil)
    end
  end
  local p = self:piece("poppet", self.rng:random(self.colors))
  p.star = self.starsOn and self.rng:random() < STAR_CHANCE
  return p
end

-- The meter's drain right now, per second.
function Sim:drain()
  return min(DRAIN_START + DRAIN_GROWTH * self.t, DRAIN_MAX)
end

function Sim:ready()
  return self.phase == "ready"
end

-- The player swaps the piece at a toward b. False if that's not a swap at
-- all (not neighbours, a rock, a hole, the round is over). A swipe while
-- the board is settling is kept for a moment.
function Sim:swap(a, b)
  if self.over or self.phase == "asleep" or a < 1 or a > N or b < 1 or b > N or not adjacent(a, b) then
    return false
  end
  if not self:ready() then
    self.queued = { a, b, QUEUE_TIME }
    return true
  end
  local pa, pb = self.board[a], self.board[b]
  if not pa or not pb or pa.kind == "rock" or pb.kind == "rock" then
    return false
  end
  self.phase, self.pt, self.pa, self.pb = "swap", 0, a, b
  self.pvalid = swapValid(self.board, a, b)
  self.swaps += 1
  self.cues[#self.cues + 1] = { kind = "swap" }
  return true
end

-- How good swapping m looks right now: its first step only (what a person
-- sees, no peeking at refills). -1 if it does nothing.
local preview = { next = 0, random = function() return 0.3 end }
function Sim:value(m)
  if not swapValid(self.board, m[1], m[2]) then return -1 end
  local b = table.clone(self.board)
  b[m[1]], b[m[2]] = b[m[2]], b[m[1]]
  preview.next = self.nextId
  local s = resolve(b, preview, m, 1)
  return s.pieces + s.rocks + 3 * #s.made + 1.5 * s.fires
end

local function cue(self, c)
  self.cues[#self.cues + 1] = c
end

function Sim:step(dt)
  if self.over then return end
  local draining = self.phase == "ready" or self.phase == "swap" or self.phase == "bounce"
  if not self.demo and self.phase ~= "asleep" then
    self.t += dt
    if draining then
      local before = self.energy
      self.energy = max(0, self.energy - self:drain() * dt)
      if before >= DROWSY and self.energy < DROWSY then cue(self, { kind = "drowsy" }) end
    end
    local level = 1 + floor(self.t / LEVEL_TIME)
    while self.level < level do
      self.level += 1
      self:levelUp()
    end
  end
  if self.queued then
    self.queued[3] -= dt
    if self.queued[3] <= 0 then self.queued = nil end
  end
  local ph = self.phase
  if ph == "ready" then
    if self.energy <= 0 and not self.demo then
      self.phase, self.pt = "asleep", 0
      cue(self, { kind = "asleep" })
    elseif self.queued then
      local q = self.queued
      self.queued = nil
      self:swap(q[1], q[2])
    end
  elseif ph == "swap" then
    self.pt += dt
    if self.pt >= SWAP_TIME then
      if self.pvalid then
        local a, b = self.pa, self.pb
        self.board[a], self.board[b] = self.board[b], self.board[a]
        self:resolve({ a, b })
      else
        self.phase, self.pt = "bounce", 0
        cue(self, { kind = "bounce" })
      end
    end
  elseif ph == "bounce" then
    self.pt += dt
    if self.pt >= SWAP_TIME then self.phase = "ready" end
  elseif ph == "pop" then
    self.pt += dt
    if self.pt >= self.pdur then
      self.phase, self.pt, self.pdur = "fall", 0, self:collapse()
    end
  elseif ph == "fall" then
    self.pt += dt
    if self.pt >= self.pdur then
      if anyMatch(self.board) then
        self:resolve(nil)
      else
        self:settle()
      end
    end
  elseif ph == "shuffle" then
    self.pt += dt
    if self.pt >= SHUFFLE_TIME then self.phase = "ready" end
  elseif ph == "asleep" then
    self.pt += dt
    if self.pt >= ENDING then self.over = true end
  end
end

function Sim:levelUp()
  local n = sim.news(self.level)
  if n == "newColor" then
    self.colors = min(self.colors + 1, 6)
  elseif n == "rocks" then
    self.rocksOn = true
    self.rocksDue += 3
  elseif n == "stars" then
    self.starsOn = true
  elseif n == "moreRocks" then
    self.rocksMax += 3
    self.rocksDue += 3
  end
  cue(self, { kind = "level", level = self.level, news = n })
end

-- One resolution step: pop, score, refill the meter.
function Sim:resolve(swap)
  self.chain += 1
  self.bestChain = max(self.bestChain, self.chain)
  local rng = self.rng
  local ids = { next = self.nextId, random = function() return rng:random() end }
  local st = resolve(self.board, ids, swap, self.chain)
  self.nextId = ids.next
  local chain = self.chain
  self.score += (st.pieces + 2 * st.rocks) * POINTS_PIECE * chain + #st.made * 30 + st.stars * 100 + (st.combo and 200 or 0)
  local before = self.energy
  local gain = st.pieces * GAIN_PIECE * (1 + GAIN_CHAIN * (chain - 1)) + #st.made * GAIN_MADE + st.rocks * GAIN_ROCK + st.stars * GAIN_STAR
  if not self.demo then self.energy = min(1, self.energy + gain) end
  cue(self, { kind = "pop", chain = chain, pieces = st.pieces })
  if st.combo then cue(self, { kind = "combo" }) end
  for _, b in st.blasts do cue(self, { kind = "fire", blast = b.kind }) end
  for _, m in st.made do cue(self, { kind = "made", at = m.at, piece = m.piece }) end
  if st.rocks > 0 then cue(self, { kind = "rocks", n = st.rocks }) end
  for _, p in st.popping do
    if p.piece.star then cue(self, { kind = "star", at = p.at }) end
  end
  if before < PHEW_BELOW and self.energy >= PHEW_BELOW + 0.08 then cue(self, { kind = "phew" }) end
  self.phase, self.pt, self.pdur = "pop", 0, st.dur
  self.last = st
end

-- Everything falls into the holes; new poppets drop in from the top.
-- Returns how long the fall takes.
function Sim:collapse()
  local b = self.board
  local longest = 0
  for c = 0, COLS - 1 do
    local write = ROWS
    for r = ROWS - 1, 0, -1 do
      local p = b[cell(c, r)]
      if p then
        b[cell(c, r)] = false
        write -= 1
        p.fromX, p.fromY = c, r
        longest = max(longest, fallTime(write - r))
        b[cell(c, write)] = p
      end
    end
    local empty = write
    for r = 0, empty - 1 do
      local p = self:spawn()
      p.fromX, p.fromY = c, r - empty - 0.3
      longest = max(longest, fallTime(empty + 0.3))
      b[cell(c, r)] = p
    end
  end
  return longest + LAND_SETTLE
end

-- The board has come to rest: a new turn (or a shuffle if it's stuck).
function Sim:settle()
  self.chain = 0
  self.moves = moves(self.board)
  if #self.moves == 0 then
    self:shuffle()
    self.moves = moves(self.board)
    self.phase, self.pt = "shuffle", 0
    cue(self, { kind = "shuffle" })
  else
    self.phase = "ready"
  end
  local best, bestValue = nil, -math.huge
  for _, m in self.moves do
    local v = self:value(m)
    if v > bestValue then best, bestValue = m, v end
  end
  self.hint = best
  self.settles += 1
end

-- Moves the poppets around (rocks stay) until there's a move and no match.
function Sim:shuffle()
  local b = self.board
  local cells, pieces = {}, {}
  for i = 1, N do
    local p = b[i]
    if p and p.kind ~= "rock" then
      p.fromX, p.fromY = col(i), row(i)
      cells[#cells + 1] = i
      pieces[#pieces + 1] = p
    end
  end
  for attempt = 1, 200 do
    for i = #pieces, 2, -1 do
      local j = self.rng:random(i)
      pieces[i], pieces[j] = pieces[j], pieces[i]
    end
    if attempt > 100 then
      -- Stuck (rocks everywhere): new colors too.
      for _, p in pieces do
        if not p.special and p.kind == "poppet" then p.color = self.rng:random(self.colors) end
      end
    end
    for k, i in cells do b[i] = pieces[k] end
    if not anyMatch(b) and hasMove(b) then return end
  end
  -- Still nothing: clear the rocks and deal a fresh board.
  for i = 1, N do b[i] = false end
  self:fill()
end

-- The meter as the faces show it: 0 wide awake … 1 asleep.
function Sim:drowsiness()
  return math.clamp((DROWSY_FROM - self.energy) / DROWSY_FROM, 0, 1)
end

-- Debug and scenarios: put a special (or a star) on the board at cell i.
function Sim:place(i, kind, special, star)
  local p = self.board[i]
  if not p then return end
  local q = copy(p)
  q.kind, q.special, q.star = kind, special, star or false
  if kind ~= "poppet" then q.color = nil elseif not q.color then q.color = 1 end
  self.board[i] = q
  self:settle()
end

-- Debug and scenarios: jump ahead to a level, with everything it brings.
function Sim:skipTo(level)
  self.t = math.max(self.t, (level - 1) * LEVEL_TIME)
end

-- For draw.lua and main.lua.
sim.COLS, sim.ROWS, sim.N = COLS, ROWS, N
sim.SWAP_TIME, sim.POP_TIME, sim.SHUFFLE_TIME, sim.LEVEL_TIME = SWAP_TIME, POP_TIME, SHUFFLE_TIME, LEVEL_TIME
sim.DROWSY = DROWSY
sim.col, sim.row, sim.cell, sim.adjacent = col, row, cell, adjacent
sim.isSpecial = isSpecial

return sim
