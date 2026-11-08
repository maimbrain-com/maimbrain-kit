-- Sounds, music and haptics from the sim's cues. Output only: nothing here
-- feeds back into the game, and the pitch jitter comes from this file's own
-- random generator, never the game's, so sound can't change a round.
--
-- The mix: the march (music/music_base.ogg) is the reference. A full board
-- fires a gumball, a hit and a splat several times a second, so those are
-- the quietest sounds, varied in pitch and level, rate-limited and held to
-- VOICES voices each (the oldest stops). The music came out in B♭ major,
-- and the mortar's thoomp is pitched a semitone down onto its B♭. Danger (a
-- jelly near the cake, the last heart, a king on the road) swaps the march
-- for its urgent twin (music/music_tension.ogg); they share a tempo but not
-- a downbeat, so they swap with a fade rather than stack. A bite ducks
-- the music (the music bus dips and springs back, a king's fanfare too),
-- and it drains away under the devouring.

local sound = {}

-- ---- The mix ----------------------------------------------------------------
-- Music level: the reference everything else is mixed against.
local MUSIC_VOL = 0.85
-- Voices at once for each rapid-fire sound (gumball, hit, splat, boom, lob).
local VOICES = 3
-- The mortar's thoomp rings on B: a semitone down puts it on B♭.
local LOB_PITCH = 0.943874
-- Danger at which the music turns urgent, and below which it calms again
-- (with a gap between, so it doesn't flicker).
local URGENT_ON = 0.8
local URGENT_OFF = 0.55
-- Semitones a splat rises per step of a quick chain, and the most steps.
local CHAIN_STEP = 1.5
local CHAIN_STEPS = 4
-- The music bus's level under a bite and a king's fanfare (it springs back
-- to 1), and how fast it comes back (swings a second, no overshoot).
local DUCK_BITE = 0.4
local DUCK_BOSS = 0.3
local DUCK_RETURN = 1.2

local NAMES = { "start", "build", "upgrade", "pop", "lob", "frost", "boom", "hit", "splat", "coin", "bite", "wave", "boss", "deny", "ui", "over", "best" }
local POOLED = { pop = VOICES, lob = VOICES, boom = VOICES, hit = VOICES, splat = VOICES, coin = 3, bite = 2, frost = 2, ui = 2, build = 2 }

local pools, turn, cool = {}, {}, {}
local later = {} -- delayed one-shots: { seconds, name, volume }
local rng
local on = false -- a round is playing (the title's attract mode is silent)
local urgent = false
local musicOn = false

-- Dips the music and lets it spring back.
local function duck(level)
  local bus = audio.bus("music")
  bus.volume = math.min(bus.volume, level)
  spring(bus, { volume = 1 }, { frequency = DUCK_RETURN, damping = 1 })
end

function sound.load()
  rng = love.math.newRandomGenerator(0x7dca4e)
  for _, name in NAMES do
    local src = love.audio.newSource(name, "static")
    local pool = { src }
    for _ = 2, POOLED[name] or 1 do pool[#pool + 1] = src:clone() end
    pools[name], turn[name], cool[name] = pool, 1, 0
  end
end

-- Plays a sound on its next voice (the oldest is cut).
local function play(name, vol, pitch)
  if not on then return end
  local pool = pools[name]
  local i = turn[name]
  turn[name] = i % #pool + 1
  local s = pool[i]
  s:stop()
  s:setVolume(math.clamp(vol, 0, 1))
  s:setPitch(pitch or 1)
  s:play()
end

local function jitter(amount) return 1 + (rng:random() * 2 - 1) * amount end

-- At most once per `gap` seconds, ±1.5 dB.
local function playCapped(name, vol, pitch, gap)
  if cool[name] > 0 then return end
  cool[name] = gap
  play(name, vol * 10 ^ ((rng:random() * 2 - 1) * 1.5 / 20), pitch)
end

local function playCool(name, vol, pitch, gap)
  if cool[name] > 0 then return end
  cool[name] = gap
  play(name, vol, pitch)
end

-- A round starts: the start blip, the march from the top.
function sound.start()
  on = true
  urgent = false
  play("start", 0.6, 1)
  music.play("music_base", { volume = MUSIC_VOL, fade = 0.3 })
  musicOn = true
end

-- Taps on the picker (opening it, closing it).
function sound.ui(up)
  play("ui", 0.4, if up then 1.12 else 0.9)
end

function sound.deny()
  play("deny", 0.55, 1)
  if on then haptic("tap") end
end

function sound.best()
  play("best", 0.8, 1)
  if on then haptic("success") end
end

-- This frame's cues.
function sound.cues(cues)
  if not on then return end
  for _, c in cues do
    local k = c.kind
    if k == "built" then
      play("build", 0.75, if c.tower == "pop" then 1 elseif c.tower == "chill" then 1.12 else 0.89)
      haptic("tap")
    elseif k == "upgraded" then
      play("upgrade", 0.6, if c.level == 2 then 1 else 1.19)
      haptic("success")
    elseif k == "denied" then
      sound.deny()
    elseif k == "fire" then
      if c.tower == "pop" then
        playCapped("pop", 0.15, jitter(0.06), 0.1)
      else
        -- Tonal: vary the level, barely the pitch.
        playCapped("lob", 0.34, jitter(0.008) * LOB_PITCH, 0.12)
      end
    elseif k == "frost" then
      playCool("frost", 0.3, jitter(0.04), 0.12)
    elseif k == "boom" then
      playCapped("boom", 0.24, jitter(0.06), 0.12)
    elseif k == "hit" then
      playCapped("hit", 0.17, jitter(0.1), 0.08)
    elseif k == "pop" then
      -- Quick pops climb gently; big jellies pop lower.
      local step = math.min(math.max(c.chain, 1) - 1, CHAIN_STEPS)
      local size = 0
      if c.creep == "king" then size = -5
      elseif c.creep == "splitter" or c.creep == "helmet" then size = -2
      elseif c.creep == "zipper" or c.creep == "mini" then size = 1.5 end
      local p = 2 ^ ((step * CHAIN_STEP + size) / 12) * jitter(0.03)
      playCapped("splat", if c.creep == "king" then 0.5 else 0.22, p, 0.06)
      if c.chain >= 3 or c.creep == "king" then haptic(if c.creep == "king" then "heavy" else "tap") end
    elseif k == "chomp" then
      playCool("bite", 0.5, jitter(0.12) * 1.15, 0.09)
    elseif k == "bite" then
      play("bite", 0.9, if c.hearts <= 1 then 0.85 else 1)
      duck(DUCK_BITE)
      haptic("heavy")
    elseif k == "wave" then
      if c.boss then
        play("boss", 0.8, 1)
        duck(DUCK_BOSS)
        haptic("heavy")
      else
        play("wave", 0.42, 1)
      end
    elseif k == "payday" then
      later[#later + 1] = { 0.25, "coin", 0.35 }
    elseif k == "early" then
      if c.bonus > 0 then
        play("coin", 0.7, 1)
        later[#later + 1] = { 0.08, "coin", 0.6 }
      end
    elseif k == "ending" then
      -- The music drains away under the devouring: the punchline's beat.
      flux.to(audio.bus("music"), 0.4, { volume = 0 }):ease("quadin")
      music.stop(0.5)
      musicOn = false
      haptic("fail")
    elseif k == "over" then
      play("over", 0.85, 1)
    end
  end
end

-- Per frame: cooldowns, delayed sounds, and the music's mood from `danger`
-- (0…1, with a king on the road counting as 1).
function sound.update(dt, danger)
  for name, c in cool do cool[name] = c - dt end
  for i = #later, 1, -1 do
    local q = later[i]
    q[1] -= dt
    if q[1] <= 0 then
      table.remove(later, i)
      play(q[2], q[3], 1)
    end
  end
  if not on or not musicOn then return end
  if not urgent and danger >= URGENT_ON then
    urgent = true
    music.play("music_tension", { volume = MUSIC_VOL, fade = 0.5 })
  elseif urgent and danger < URGENT_OFF then
    urgent = false
    music.play("music_base", { volume = MUSIC_VOL, fade = 1.2 })
  end
end

return sound
