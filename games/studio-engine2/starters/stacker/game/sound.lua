-- Sounds, music and haptics from the sim's cues. Output only: nothing here
-- feeds back into the game, and the pitch jitter comes from this file's own
-- random generator, never the game's, so sound can't change a round.
--
-- The kit's generated sounds: a jaunty acoustic loop (ukulele, whistled
-- melody, shaker and woodblock, 112 BPM, G major) in music/picnic.ogg, and
-- the same loop with its tension layer (nervous pizzicato, ticking
-- woodblock, tremolo cello) mixed in, music/picnic_tense.ogg: the music
-- swaps to it in twists, when the tower sways hard and on the last heart.
-- Landings step a soft marimba note up G major's pentatonic with the
-- streak. Thuds, the most frequent sound, sit well under the music, vary,
-- and go through audio.impact: as loud and low as the hit was hard, placed
-- where it happened, repeats folded and at most six at once (a collapse
-- would otherwise stack twenty). The music ducks under a lost heart and
-- the collapse, and comes back by itself (the music bus).

local sound = {}

-- ---- The mix ------------------------------------------------------------------
-- The landing marimba (a G) climbs G major pentatonic with the streak, D E
-- G A B D, within ±7 semitones so it never chipmunks, then starts over.
local LAND_STEPS = { -5, -3, 0, 2, 4, 7 }
-- The neat sparkle (a D chord) only fits G major at these shifts.
local NEAT_STEPS = { -2, 0, 5 }
-- The impacts' level, and the speed (px/s) at which a thud is at its
-- loudest and lowest (the kit's thuds reached full at 550).
local THUD_VOL = 0.55
local THUD_FULL = 550
-- The music ducks to this under a lost heart and the collapse, then comes
-- back over DUCK_BACK seconds.
local DUCK_LOST, DUCK_COLLAPSE, DUCK_BACK = 0.35, 0.2, 1.6
-- The new-best sting rings on G#: a semitone down puts it on G.
local BEST_PITCH = 0.943874
-- Music level, the reference everything else is mixed against; the tense
-- mix is louder (two stems), so it plays a little lower.
local MUSIC_VOL = 0.7
local TENSE_VOL = 0.58
-- How tense it gets before the music swaps to the tense mix, and how calm
-- before it swaps back (the gap keeps it from flapping).
local TENSE_ON, TENSE_OFF = 0.55, 0.22
-- The tower sways this hard: the crowd goes "ooh" (at most every OOH_GAP s).
local OOH_WOBBLE = 0.7
local OOH_GAP = 4.0

local NAMES = { "start", "drop", "thud", "thud2", "land", "neat", "lost", "new", "wind", "wobble", "over", "best", "ooh", "collapse" }
-- Voices a sound can play at once (each a Source made once and reused).
local VOICES = { land = 2, neat = 2, drop = 2, lost = 2, ooh = 2 }

local pools, turn = {}, {}
local later -- a delayed sting: { seconds, name, volume, pitch }
local rng
local on = false -- a round is playing (the title's attract mode is silent)
local hi = 0 -- how tense the music is (0–1), eased
local tense = false
local ooh = 0 -- seconds since the last "ooh"
local wobble = 0

local function semis(s) return 2 ^ (s / 12) end

function sound.load()
  rng = love.math.newRandomGenerator(0x5a1ad)
  for _, name in NAMES do
    local src = love.audio.newSource(name, "static")
    local pool = { src }
    for _ = 2, VOICES[name] or 1 do pool[#pool + 1] = src:clone() end
    pools[name], turn[name] = pool, 1
  end
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

-- A level change of ±db dB, so a sound heard often never repeats exactly.
local function vary(db)
  return 10 ^ ((rng:random() * 2 - 1) * db / 20)
end

local function jitter()
  return 0.95 + rng:random() * 0.1
end

-- The music dips under a big moment and comes back by itself.
local function duck(level)
  local bus = audio.bus("music")
  flux.to(bus, 0.08, { volume = level }):after(bus, DUCK_BACK, { volume = 1 })
end

-- A round starts: a sting and the music from the top.
function sound.start()
  on = true
  later = nil
  hi, tense, ooh = 0, false, 0
  play("start", 0.6, 1)
  music.play("picnic", { volume = MUSIC_VOL, fade = 0.3 })
end

-- The round's over (the plate has tipped): a beat of quiet after the
-- crash, then the sting, and the new-best fanfare if it was one.
function sound.over(best)
  if not on then return end
  music.stop(0.3)
  later = { 0.15, "over", 0.75, 1 }
  haptic("fail")
  if best then
    later.after = true
  end
end

-- This frame's cues. Off a round (the title's attract mode) nothing sounds.
function sound.cues(cues)
  if not on then return end
  local nthud, nlost = 0, 0
  for _, c in cues do
    local k = c.type
    if k == "thud" and nthud < 2 then
      nthud += 1
      local src = pools[if rng:random() < 0.5 then "thud" else "thud2"][1]
      src:setVolume(THUD_VOL * vary(1))
      src:setPitch(jitter())
      src:setPosition(c.x, c.y)
      audio.impact(src, c.speed, THUD_FULL)
      if c.vol > 0.3 then haptic(if c.heavy then "heavy" else "tap") end
    elseif k == "drop" then
      -- Tonal (a B): vary the level, barely the pitch.
      play("drop", 0.3 * vary(1.5), 0.992 + rng:random() * 0.016)
    elseif k == "land" and c.close and ooh > 1 then
      -- It only just stayed on.
      play("ooh", 0.5 * vary(1), 1)
      play("land", 0.42, 1)
      ooh = 0
      haptic("tap")
    elseif k == "land" then
      local step = math.max(c.streak, 1) - 1
      play("land", 0.42 * vary(1), semis(LAND_STEPS[step % #LAND_STEPS + 1]))
      if c.neat then
        play("neat", 0.4, semis(NEAT_STEPS[math.min(step // 2, #NEAT_STEPS - 1) + 1]))
      end
      haptic("tap")
    elseif k == "lost" and nlost < 1 then
      nlost += 1
      if c.counted then
        play("lost", 0.45, 1)
        duck(DUCK_LOST)
        haptic("fail")
      else
        play("lost", 0.1, jitter() * 1.15)
      end
    elseif k == "newkind" then
      play("new", 0.5, 1)
    elseif k == "heartback" then
      play("new", 0.6, 1.335)
      haptic("success")
    elseif k == "warn" then
      play(if c.kind == "gust" then "wind" else "wobble", 0.6, 1)
    elseif k == "start" then
      -- And again, lower, as it really starts.
      play(if c.kind == "gust" then "wind" else "wobble", 0.48, 0.88)
    elseif k == "collapse" then
      play("collapse", 0.7, 1)
      duck(DUCK_COLLAPSE)
      haptic("heavy")
    end
  end
end

-- Per frame: the delayed sting, the crowd's gasp at a hard sway, and the
-- music's tension (a twist, the tower swaying, the last heart, and a
-- little with height).
function sound.update(dt, s)
  if later then
    later[1] -= dt
    if later[1] <= 0 then
      play(later[2], later[3], later[4])
      if later.after then
        later = { 0.9, "best", 0.75, BEST_PITCH }
        haptic("success")
      else
        later = nil
      end
    end
  end
  if not on or not s then return end
  ooh += dt
  if not s.ending and s.wobble > OOH_WOBBLE and wobble <= OOH_WOBBLE and ooh > OOH_GAP then
    play("ooh", 0.45 * vary(1), 1)
    ooh = 0
  end
  wobble = s.wobble
  if s.ending then return end
  local danger = math.max(s.wobble, if s.twist.phase ~= "calm" then 0.8 else 0, if s.lives == 1 then 1 else 0)
  local want = math.max(danger, math.min(s.height / 1600, 0.3))
  local rate = if want > hi then 2.0 else 0.6
  hi += (want - hi) * (1 - math.exp(-dt * rate))
  if not tense and hi > TENSE_ON then
    tense = true
    music.play("picnic_tense", { volume = TENSE_VOL, fade = 0.6 })
  elseif tense and hi < TENSE_OFF then
    tense = false
    music.play("picnic", { volume = MUSIC_VOL, fade = 1.0 })
  end
end

return sound
