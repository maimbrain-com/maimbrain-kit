-- Sound effects, haptics and the music, from the sim's cues.
--
-- Output only: nothing here is read back by the game, and the pitch and
-- level jitter come from this file's own generator, so sound can't change
-- a round. The effects are sounds/*.ogg (an analog-synth palette in the
-- music's key, G minor); the music is three loops in music/:
--   calm  the synthwave loop with its high layer down (the opening),
--   main  the full loop (from the darters on, or on the last heart),
--   boss  the Gloom Queen's loop (on A), while she's alive.
-- The WARNING sinks the music to a quarter under its siren; her arrival boom
-- starts her loop, and the main loop comes back 1.6 s after she explodes.
-- The big moments (a hit on Pip, the warning, her rage, her fall) duck the
-- music for a moment: the music bus's volume (audio.bus("music")).

local sound = {}

-- ---- The mix ------------------------------------------------------------------
-- The music is the reference (0.7). The frequent sounds (the auto-fire
-- ~270 a minute, hits and pops ~70, gems ~40) are the quietest, vary in
-- pitch and level, have a cap on voices at once (a new one stops the
-- oldest) and a least gap between starts. Rare ones (a hit on Pip, the
-- shield breaking, the warning, the queen, the death) are loud.
local MUSIC_VOL = 0.7
local MIX = {
  shot = { 0.075, 2, 0.12 },  -- every other volley: the softest thing in the game
  pop = { 0.18, 3, 0.05 },    -- every kill
  hit = { 0.07, 2, 0.08 },    -- a shot lands but doesn't kill
  gem = { 0.1, 3, 0.04 },
  start = { 0.4, 1, 0 },
  boom = { 0.36, 3, 0.05 },   -- big kills and the explosion chains
  power = { 0.3, 1, 0 },
  hurt = { 0.7, 1, 0 },
  shield = { 0.6, 1, 0 },     -- the shield breaking
  graze = { 0.13, 2, 0.12 },
  lock = { 0.22, 2, 0.1 },    -- a darter locks on
  new = { 0.32, 1, 0 },       -- a new enemy type
  warn = { 0.5, 1, 0 },
  boss_boom = { 0.85, 1, 0 },
  die = { 0.75, 1, 0 },
  over = { 0.6, 1, 0 },
  best = { 0.65, 1, 0 },
  bhit = { 0.09, 2, 0.08 },   -- a shot lands on the queen
  charge = { 0.3, 1, 0 },     -- the queen's attack telegraph
  rage = { 0.6, 1, 0 },       -- the queen enrages
}
-- The shield's hum, looped while it's up.
local HUM_VOL = 0.09
-- The G minor pentatonic (G Bb C D F), in semitones: kills and gems climb
-- it with the chain.
local PENTA = { 0, 3, 5, 7, 10 }
-- While the boss loop (on A) plays, tonal effects move up a whole tone.
local KEY_BOSS = 2
-- The main loop comes back this long after the queen falls.
local BOSS_TO_MAIN = 1.6
-- The calm loop gives way to the main one at this intensity (sim:intensity()).
local HOT = 0.45
-- Quiet before the queen: the music sinks this low under the warning.
local PRE_BOSS = 0.25

local voices = {} -- name → { list, next, last }
local rng, clock = nil, 0
local hum
local mood -- the loop playing: "calm", "main", "boss", or nil
local backAt -- when the main loop comes back after the boss
local bossKey = false
-- The music bus's level: a duck (dips on the big moments, back up over
-- ~0.5 s) times the pre-boss sink (eased over ~0.8 s).
local duck, pre, preBoss = 1, 1, false
local volley = 0
local booms = 0

local function semis(s) return 2 ^ (s / 12) end

-- Pentatonic step n up from the root, climbing octaves.
local function scale(n)
  return semis(PENTA[n % 5 + 1] + 12 * math.floor(n / 5))
end

-- 0, 1, … top, top−1, … 1, 0, 1, …: a ladder that climbs gently, turns round
-- at the octave and comes back.
local function bounce(n, top)
  if top == 0 then return 0 end
  local k = n % (2 * top)
  return if k <= top then k else 2 * top - k
end

local function jitter(amount) return 1 + (rng:random() * 2 - 1) * amount end
local function vary(db) return 10 ^ ((rng:random() * 2 - 1) * db / 20) end
local function key() return if bossKey then semis(KEY_BOSS) else 1 end

-- Makes every source once (call in love.load).
function sound.load()
  for name, m in MIX do
    local s = love.audio.newSource(name, "static")
    local list = { s }
    for _ = 2, m[2] do list[#list + 1] = s:clone() end
    voices[name] = { list = list, next = 1, last = -1 }
  end
  hum = love.audio.newSource("shield_hum", "static")
  hum:setLooping(true)
  hum:setVolume(HUM_VOL)
  rng = love.math.newRandomGenerator(0x5407e7)
end

-- Plays effect `name` at its mix level × vol, at `pitch`: the next of its
-- voices (the oldest, stopped if it's still ringing), unless one started
-- less than its gap ago.
local function play(name, vol, pitch)
  local v, m = voices[name], MIX[name]
  if m[3] > 0 and v.last >= 0 and clock - v.last < m[3] then return end
  v.last = clock
  local s = v.list[v.next]
  v.next = v.next % #v.list + 1
  s:stop()
  s:setVolume(math.min(m[1] * (vol or 1), 1))
  s:setPitch(pitch or 1)
  s:play()
end

local function setMood(name, fade)
  if mood == name then return end
  mood = name
  music.play(name, { fade = fade or 0.5, volume = MUSIC_VOL })
end

-- A round starts: the start sound and the calm loop.
function sound.start()
  clock, mood, backAt, bossKey = 0, nil, nil, false
  duck, pre, preBoss = 1, 1, false
  audio.bus("music").volume = 1
  play("start")
  setMood("calm", 0.2)
end

-- The game-over card is up: a beat later, the verdict.
function sound.over(newBest)
  timer.after(0.35, function()
    if newBest then
      play("best")
      haptic("success")
    else
      play("over")
    end
  end)
end

-- The pop's pitch: up the pentatonic with the combo, each enemy type
-- starting higher the faster it is (the big ones an octave down).
local FIRST = { jelly = 0, swooper = 1, darter = 2, bulb = 0, spinner = 0 }
local function typePitch(kind, combo)
  local first = FIRST[kind]
  local octave = if kind == "bulb" or kind == "spinner" then 0.5 else 1
  return key() * octave * scale(first + bounce(math.max(combo - 1, 0), 5 - first))
end

-- One cue from the sim (sim.lua). Only in play: the title card is silent.
function sound.cue(c)
  local t = c.type
  if t == "shot" then
    -- Every other volley: a soft patter, not a machine gun.
    volley += 1
    if volley % 2 == 0 then play("shot", vary(1.5), key() * jitter(0.03)) end
  elseif t == "hit" then
    play("hit", vary(2), jitter(0.08))
  elseif t == "bossHit" then
    play("bhit", vary(2), jitter(0.05))
  elseif t == "kill" then
    play("pop", vary(1), typePitch(c.kind, c.combo) * jitter(0.004))
    if c.kind == "bulb" or c.kind == "spinner" then
      play("boom", 1, jitter(0.04))
      haptic("heavy")
    else
      haptic("tap")
    end
  elseif t == "gem" then
    play("gem", vary(1), key() * scale(bounce(math.max(c.streak - 1, 0), 5)) * jitter(0.003))
  elseif t == "power" or t == "heart" then
    -- A heart: the same arpeggio a fourth up.
    play("power", 1, key() * (if t == "heart" then semis(5) else 1))
    haptic("success")
  elseif t == "graze" then
    play("graze", vary(1.5), key() * jitter(0.004))
  elseif t == "darterLock" then
    play("lock", 1, key())
  elseif t == "bossCharge" then
    play("charge")
  elseif t == "bossEnrage" then
    play("rage")
    duck = math.min(duck, 0.6)
    haptic("heavy")
  elseif t == "shieldBreak" then
    play("shield", 1, key())
    haptic("heavy")
  elseif t == "hurt" then
    play("hurt")
    duck = math.min(duck, 0.4)
    haptic("heavy")
  elseif t == "newEnemy" then
    play("new")
  elseif t == "warning" then
    play("warn")
    duck = math.min(duck, 0.5)
    preBoss = true
  elseif t == "bossArrive" then
    -- Her arrival boom, down to A (the boss loop's key), hides the switch.
    play("boom", 1.8, semis(-10))
    setMood("boss", 0.1)
    preBoss, pre = false, 1
    haptic("heavy")
  elseif t == "bossBeaten" then
    play("boom", 1.8, semis(-5))
    duck = math.min(duck, 0.5)
    mood = nil
    music.stop(0.8)
    haptic("heavy")
  elseif t == "bossDown" then
    play("boss_boom")
    backAt = clock + BOSS_TO_MAIN
    duck = math.min(duck, 0.2)
    haptic("success")
  elseif t == "boom" then
    if booms < 2 then
      booms += 1
      play("boom", 0.6 + 0.9 * c.size, jitter(0.05) * (1.15 - 0.35 * c.size))
      if c.size > 0.8 then haptic("heavy") end
    end
  elseif t == "dying" then
    play("die")
    -- The music drains away under the slow-motion death.
    mood = nil
    music.stop(1.2)
    haptic("fail")
  end
end

-- Each frame in play, after the cues: the music's mood and the shield's hum.
function sound.update(dt, s)
  clock += dt
  booms = 0
  bossKey = s:bossLive()
  if backAt and clock >= backAt then
    backAt = nil
    setMood("main", 0.4)
  end
  -- The calm loop opens up into the full one with the heat, the bullets or
  -- the last heart (for the rest of the loop: no switching back and forth).
  if mood == "calm" and s:alive() and s:intensity() >= HOT then setMood("main", 0.7) end
  -- The duck recovers, the sink eases in, and the bus carries both.
  duck += (1 - duck) * (1 - math.exp(-dt / 0.5))
  pre += ((if preBoss then PRE_BOSS else 1) - pre) * (1 - math.exp(-dt / 0.8))
  audio.bus("music").volume = duck * pre
  local shield = s.ship.shield and s:alive()
  if shield and not hum:isPlaying() then
    hum:setPitch(key())
    hum:play()
  elseif not shield and hum:isPlaying() then
    hum:stop()
  end
end

-- Leaving play (the round is over): the hum stops.
function sound.quiet()
  if hum:isPlaying() then hum:stop() end
end

return sound
