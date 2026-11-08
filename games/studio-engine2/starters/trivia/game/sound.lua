-- Sounds, music and haptics from the sim's cues. Output only: nothing here
-- feeds back into the game, and the pitch jitter comes from this file's own
-- random generator, never the game's, so sound can't change a round.
--
-- The band is a big-band swing loop in two stems that start together:
-- sounds/swing.ogg (calm: bass, drums, piano) and sounds/swing_hi.ogg (the
-- brass and cymbals), which never drops below HI_FLOOR and opens up with
-- the streak and the clock. A ticking stopwatch loop (sounds/tension.ogg)
-- fades in under the band as the clock runs down, played at TENSION_PITCH
-- so its tick-tocks land on the band's quarter notes. The right-answer ding
-- climbs with the streak on notes that fit the band, at most a fifth up.

local sound = {}

-- ---- The mix ----------------------------------------------------------------
-- The band: the level everything else is mixed against.
local MUSIC_VOL = 1.0
-- Effects against the band (the kit's levels, with its band at 1.3).
local FX = 0.77
-- The brass stem never drops below this share, so the band always swings.
local HI_FLOOR = 0.5
-- (5.95 s loop / 15 ticks) / (12.048 s / 32 beats): re-derive it when you
-- make either loop again.
local TENSION_PITCH = 1.0536
local TENSION_VOL = 0.65
-- The ding (a G and F dyad) by streak: semitones that stay in the band's key.
local RIGHT_STEPS = { -3, 2, 4, 7 }
-- The clock tick for 3, 2 and 1 seconds left: rising into the band's key.
local TICK_STEPS = { -1, 1, 6 }

local NAMES = { "start", "flip", "tap", "right", "wrong", "timeup", "tick", "multup", "golden", "heart", "pop", "over", "best", "fizzle" }
-- Voices a sound can play at once (each a Source made once and reused).
local VOICES = { tick = 2, flip = 2, tap = 2 }

local pools, turn = {}, {}
local later = {} -- delayed one-shots: { seconds, name, volume, pitch }
local base, hiStem, tension
local hi, hiTarget = 0, 0
local tick, tickTarget = 0, 0
local duck = 1
local rng
local on = false -- a round is playing (the title's attract mode is silent)

local function semis(s) return 2 ^ (s / 12) end

function sound.load()
  rng = love.math.newRandomGenerator(0x59a7c)
  for _, name in NAMES do
    local src = love.audio.newSource(name, "static")
    local pool = { src }
    for _ = 2, VOICES[name] or 1 do pool[#pool + 1] = src:clone() end
    pools[name], turn[name] = pool, 1
  end
  base = love.audio.newSource("swing", "static")
  hiStem = love.audio.newSource("swing_hi", "static")
  tension = love.audio.newSource("tension", "static")
  for _, s in { base, hiStem, tension } do
    s:setLooping(true)
    s:setVolume(0)
  end
  tension:setPitch(TENSION_PITCH)
end

local function play(name, vol, pitch)
  if not on then return end
  local pool = pools[name]
  local i = turn[name]
  turn[name] = i % #pool + 1
  local s = pool[i]
  s:stop()
  s:setVolume(math.clamp(vol * FX, 0, 1))
  s:setPitch(pitch or 1)
  s:play()
end
sound.play = play

local function jitter() return 0.96 + rng:random() * 0.08 end
-- A level change of ±db dB, so a sound heard often never repeats exactly.
local function vary(db) return 10 ^ ((rng:random() * 2 - 1) * db / 20) end

-- A round starts: the jingle, and the band from the top (both stems in step).
function sound.start()
  on = true
  later = {}
  play("start", 0.6, 1)
  hi, tick, duck = 0, 0, 1
  for _, s in { base, hiStem, tension } do
    s:setVolume(0)
    s:play()
  end
end

-- The band stops for the bulb blowing.
function sound.stopMusic()
  for _, s in { base, hiStem, tension } do s:stop() end
end

-- The round's verdict after a beat of quiet.
function sound.verdict(newBest)
  later[#later + 1] = { 0.55, newBest and "best" or "over", 0.65, 1 }
end

function sound.pop()
  play("pop", 0.55, 1)
  haptic("heavy")
end

-- A tap on an answer (the judgement follows the same frame).
function sound.tap()
  play("tap", 0.5, 1)
end

-- This frame's cues, with their haptics.
function sound.cues(cues)
  for _, c in cues do
    local k = c.kind
    if k == "deal" then
      play("flip", 0.32 * vary(1.5), jitter())
      if c.golden then
        play("golden", 0.6, 1)
        duck = 0.6
      elseif c.tierUp then
        play("tick", 0.22, semis(TICK_STEPS[3]))
      end
    elseif k == "tick" then
      -- Higher and louder as it runs out.
      local i = 4 - math.clamp(c.left, 1, 3)
      play("tick", 0.32 + 0.08 * (i - 1), semis(TICK_STEPS[i]))
      if on then haptic("tap") end
    elseif k == "right" then
      local s = RIGHT_STEPS[math.min(math.max(c.streak, 1), #RIGHT_STEPS)]
      play("right", c.fast and 0.6 or 0.5, semis(s))
      if on then haptic("success") end
    elseif k == "wrong" then
      local p = jitter()
      play("wrong", 0.62, p)
      -- Watt's bulb stutters as the buzzer goes.
      later[#later + 1] = { 0.06, "fizzle", 0.38, p }
      if on then haptic("fail") end
      duck = 0.6
    elseif k == "timeout" then
      play("timeup", 0.62, 1)
      later[#later + 1] = { 0.1, "fizzle", 0.34, 1 }
      if on then haptic("fail") end
      duck = 0.6
    elseif k == "multUp" then
      later[#later + 1] = { 0.18, "multup", 0.55, 1 + 0.06 * (c.mult - 2) }
    elseif k == "heartBack" then
      later[#later + 1] = { 0.3, "heart", 0.6, 1 }
    elseif k == "ending" then
      sound.stopMusic()
    end
  end
end

-- How bright the band is (0…1: the streak, the clock) and how much of the
-- stopwatch to hear (0…1: how close the clock is).
function sound.intensity(brass, clock)
  hiTarget, tickTarget = brass, clock
end

function sound.update(dt)
  for i = #later, 1, -1 do
    local q = later[i]
    q[1] -= dt
    if q[1] <= 0 then
      table.remove(later, i)
      play(q[2], q[3], q[4])
    end
  end
  if not on then return end
  duck += (1 - duck) * (1 - math.exp(-dt / 0.5))
  hi += (hiTarget - hi) * (1 - math.exp(-dt / (hiTarget > hi and 0.4 or 1.2)))
  tick += (tickTarget - tick) * (1 - math.exp(-dt / (tickTarget > tick and 0.25 or 0.5)))
  -- The band steps back a little as the stopwatch takes over.
  local band = duck * (1 - 0.35 * tick)
  base:setVolume(MUSIC_VOL * band)
  hiStem:setVolume(MUSIC_VOL * band * (HI_FLOOR + (1 - HI_FLOOR) * hi))
  tension:setVolume(TENSION_VOL * tick * tick)
end

return sound
