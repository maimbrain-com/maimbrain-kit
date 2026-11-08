-- Sounds, music and haptics from the sim's cues. Output only: nothing here
-- feeds back into the game, and the pitch jitter comes from this file's own
-- random generator, never the game's, so sound can't change a round.
--
-- The lullaby (music/lullaby.ogg) is in F major. The tension drone
-- (sounds/tension.ogg) is an E minor 7 chord, so it plays two semitones
-- down (TENSION_PITCH: D minor 7, F major's relative minor) and fades in as
-- the poppets get drowsy. Pops, the most frequent sound, sit well under the
-- music with varied pitch and level, at most POP_VOICES at once; every
-- tonal effect is pitched onto F major's notes.

local sound = {}

-- ---- The mix ----------------------------------------------------------------
-- The cascade chime's notes from chain 2 up (semitones from its F): the F
-- major pentatonic C D F G B♭ C, within ±7 so the bell never chipmunks.
local CHIME_STEPS = { -5, -3, 0, 2, 5, 7 }
-- Pops rise a semitone per cascade step, up to this many.
local POP_RISE = 4
-- Pop voices at once (a cascade stacks them); the oldest stops.
local POP_VOICES = 3
-- Music level: the reference everything else is mixed against.
local MUSIC_VOL = 0.85
-- The tension drone at full drowsiness, relative to the lullaby.
local TENSION_VOL = 0.9
-- The drone is an E minor 7 chord: two semitones down is D minor 7, in F.
local TENSION_PITCH = 0.890899
-- The select tick is a low marimba E; a fourth up (A, in F major) makes a light tap.
local SELECT_PITCH = 1.33484
-- The swap swish rings on C#; a minor third up puts it on E.
local SWAP_PITCH = 1.189207

local NAMES = { "start", "select", "swap", "bounce", "pop", "chime", "line", "bomb", "rainbow", "made", "rock", "level", "shuffle", "drowsy", "phew", "over", "best", "ui", "thud" }
-- Voices a sound can play at once (each a Source made once and reused: a
-- round can make 512 sources, and a long round plays thousands of sounds).
local VOICES = { pop = POP_VOICES, chime = 2, line = 2, bomb = 2, made = 2, rock = 2, thud = 2, swap = 2, select = 2 }

local pools, turn = {}, {}
local pops = {} -- pop voices still sounding: { age, source }, oldest first
local later = {} -- delayed one-shots: { seconds, name, volume, pitch }
local tension
local hi, hiTarget = 0, 0
local tick = 0
local rng
local on = false -- a round is playing (the title's attract mode is silent)
local stopping = false

local function semis(s) return 2 ^ (s / 12) end

function sound.load()
  rng = love.math.newRandomGenerator(0x9099e75)
  for _, name in NAMES do
    local src = love.audio.newSource(name, "static")
    local pool = { src }
    for _ = 2, VOICES[name] or 1 do pool[#pool + 1] = src:clone() end
    pools[name], turn[name] = pool, 1
  end
  tension = love.audio.newSource("tension", "static")
  tension:setLooping(true)
  tension:setPitch(TENSION_PITCH)
  tension:setVolume(0)
end

-- Plays a sound on its next voice; returns the source.
local function play(name, vol, pitch)
  local pool = pools[name]
  local i = turn[name]
  turn[name] = i % #pool + 1
  local s = pool[i]
  s:stop()
  s:setVolume(math.clamp(vol, 0, 1))
  s:setPitch(pitch or 1)
  s:play()
  return s
end

-- Level and pitch varied a little, so a sound heard often never repeats exactly.
local function vary(vol, pitch, db, pct)
  local v = vol * 10 ^ ((rng:random() * 2 - 1) * db / 20)
  return v, pitch * (1 + (rng:random() * 2 - 1) * pct)
end

local function jitter(amount) return 1 + (rng:random() * 2 - 1) * amount end

-- A round starts: the start sting, the lullaby from the top, the drone (silent until they're drowsy).
function sound.start()
  on = true
  play("start", 0.6, 1)
  music.play("lullaby", { volume = MUSIC_VOL, fade = 0.3 })
  tension:setVolume(0)
  tension:play()
end

function sound.select()
  local v, p = vary(0.4, SELECT_PITCH, 1.5, 0.005)
  play("select", v, p)
  haptic("tap")
end

-- A rock poked (rocks don't move).
function sound.rockPoke()
  local v, p = vary(0.4, 0.7, 1.5, 0.05)
  play("rock", v, p)
  haptic("tap")
end

-- How much of the drone to mix in (0…1): how drowsy they are.
function sound.tension(k)
  hiTarget = math.clamp(k, 0, 1)
end

-- Per frame: the drone's level, the delayed sounds, the drowsy heartbeat.
function sound.update(dt, drowsy)
  for i = #later, 1, -1 do
    local q = later[i]
    q[1] -= dt
    if q[1] <= 0 then
      table.remove(later, i)
      if on then play(q[2], q[3], q[4]) end
    end
  end
  for i = #pops, 1, -1 do
    pops[i][1] += dt
    if pops[i][1] > 0.6 then table.remove(pops, i) end
  end
  local tau = hiTarget > hi and 0.6 or 1.5
  hi += (hiTarget - hi) * (1 - math.exp(-dt / tau))
  if stopping then hi = math.max(0, hi - dt / 0.35) end
  local v = MUSIC_VOL * TENSION_VOL * hi
  if math.abs(v - tension:getVolume()) > 0.004 then tension:setVolume(math.clamp(v, 0, 1)) end
  -- A soft heartbeat (two plush thuds) while they're nodding off.
  if drowsy and on then
    tick -= dt
    if tick <= 0 then
      tick = 0.7
      play("thud", 0.4, 1)
      later[#later + 1] = { 0.18, "thud", 0.28, 0.9 }
    end
  else
    tick = 0
  end
end

-- A new best, as the game-over card comes up.
function sound.best()
  if not on then return end
  play("best", 0.7, 1)
  haptic("success")
end

-- This frame's cues. Off a round (the title's attract mode) nothing sounds.
function sound.cues(cues)
  if not on then return end
  local fires = 0
  for _, c in cues do
    local k = c.kind
    if k == "swap" then
      local v, p = vary(0.45, SWAP_PITCH, 1.5, 0.008)
      play("swap", v, p)
    elseif k == "bounce" then
      play("bounce", 0.5, 1)
      haptic("tap")
    elseif k == "pop" then
      -- The most frequent sound: quiet, varied, capped. It rises gently with
      -- the cascade; the chime carries the melody.
      local step = math.max(c.chain, 1) - 1
      local v, p = vary(0.28 + 0.01 * math.min(c.pieces, 8), semis(math.min(step, POP_RISE)), 1.5, 0.03)
      if #pops >= POP_VOICES then
        pops[1][2]:stop()
        table.remove(pops, 1)
      end
      pops[#pops + 1] = { 0, play("pop", v, p) }
      if c.chain >= 2 then
        local s = CHIME_STEPS[math.min(c.chain - 1, #CHIME_STEPS)]
        local cv = vary(0.27, 1, 1, 0)
        play("chime", cv, semis(s))
      end
      if c.chain >= 3 then haptic("success") else haptic("tap") end
    elseif k == "made" then
      local v, p = vary(0.26, 1, 1, 0.02)
      play("made", v, p)
    elseif k == "fire" then
      fires += 1
      if fires <= 2 then
        local b = c.blast
        if b == "row" or b == "col" or b == "cross" then
          play("line", 0.55, jitter(0.04))
        elseif b == "area" then
          play("bomb", 0.75, jitter(0.04))
        else
          play("rainbow", 0.65, 1)
        end
        haptic("heavy")
      end
    elseif k == "combo" then
      play("bomb", 0.9, 0.8)
    elseif k == "rocks" then
      play("rock", 0.75, jitter(0.06))
    elseif k == "star" then
      play("phew", 0.6, 1.5)
    elseif k == "level" then
      -- Other news a fourth up (both in F major).
      play("level", 0.55, c.news == "faster" and 1 or semis(5))
      haptic("success")
    elseif k == "shuffle" then
      play("shuffle", 0.6, 1)
    elseif k == "drowsy" then
      play("drowsy", 0.55, 1)
      haptic("fail")
    elseif k == "phew" then
      play("phew", 0.7, 1)
    elseif k == "asleep" then
      play("over", 0.75, 1)
      haptic("fail")
      music.stop(0.35)
      stopping = true
    end
  end
end

return sound
