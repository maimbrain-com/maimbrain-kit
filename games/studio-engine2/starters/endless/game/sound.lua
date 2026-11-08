-- Sound effects, haptics and the music, from the run's cues.
--
-- Output only: nothing here is read back by the game, and the pitch and
-- level jitter come from this file's own random generator, so sound can't
-- change a run. The sounds are sounds/*.ogg, the music music/theme.ogg.

local sound = {}

-- The mix. The music is the reference (both of its parts at full come to
-- about -24 LUFS), and every effect's level is set against it. Frequent
-- sounds carry the least energy: coins (2–3 a second, in bursts) sit ~9 dB
-- under the music, yet each, short and bright, is heard; jumps ~10 dB under;
-- rare moments (CLOSE!, a new family) come close to it, and only the end
-- of a run (the bonk, the slide whistle) goes over, as the music fades out.
local MUSIC_VOL = 1
local VOL = {
  coin = 0.24, arc = 0.25, jump = 0.22, jump2 = 0.36, close = 0.47, faster = 0.22,
  new = 0.4, milestone = 0.3, bonk = 0.85, fall = 0.46, stuck = 0.53, bounce = 0.35,
  land = 0.45, start = 0.5, over = 0.67, best = 0.6,
}
-- A coin streak climbs these semitones, then stays at the top: coin.ogg is
-- B5 to E6, so E, A, B, notes the music (in E) always has. It starts again
-- with the next streak (the run resets it 0.45 s after the last coin).
local LADDER = { 0, 5, 7 }
-- Coins on an arc come ~0.1 s apart: one this soon after the last coin
-- sound is folded into it.
local COIN_GAP = 0.085
-- Voices a sound can have ringing at once; a new one stops the oldest.
local CAP = { jump = 2, land = 2, coin = 2, bounce = 2 }

local voices = {} -- name → { sources, next }
local rng, clock, coinAt, bounces = nil, 0, -1, 0

-- Makes every source, once (call in love.load).
function sound.load()
  for name in VOL do
    local s = love.audio.newSource(name, "static")
    local list = { s }
    for _ = 2, CAP[name] or 1 do list[#list + 1] = s:clone() end
    voices[name] = { list = list, next = 1 }
  end
  rng = love.math.newRandomGenerator(0x0b0b5eed)
end

local function semis(s)
  return 2 ^ (s / 12)
end

-- `amount` either way, as a factor (pitch).
local function jitter(amount)
  return 1 + (rng:random() * 2 - 1) * amount
end

-- A volume varied by up to ±db decibels.
local function vary(vol, db)
  return vol * 10 ^ ((rng:random() * 2 - 1) * db / 20)
end

-- Plays a sound: the next of its voices (the oldest, stopped if it's still ringing).
local function play(name, vol, pitch)
  local v = voices[name]
  local s = v.list[v.next]
  v.next = v.next % #v.list + 1
  s:stop()
  s:setVolume(math.min(vol or VOL[name], 1))
  s:setPitch(pitch or 1)
  s:play()
end
sound.play = play

-- A round starts: a little "go", and the music from the top.
function sound.start()
  play("start")
  bounces = 0
  music.play("theme", { fade = 0.2, volume = MUSIC_VOL })
end

-- The game-over card is up: a beat after the tumble, the verdict.
function sound.over(newBest)
  timer.after(0.15, function()
    if newBest then
      play("best")
      haptic("success")
    else
      play("over")
    end
  end)
end

local function fadeOut()
  music.stop(0.5)
end

-- One cue from the run (sim.lua). Not on the title card, which is silent.
function sound.cue(c, playing)
  if not playing then return end
  local kind = c.type
  if kind == "jump" then
    play("jump", vary(VOL.jump, 1.5), jitter(0.04))
    haptic("tap")
  elseif kind == "double" then
    play("jump2", vary(VOL.jump2, 1), jitter(0.03))
    haptic("tap")
  elseif kind == "land" then
    play("land", math.clamp(c.speed / 1300, 0.12, 0.45), jitter(0.06))
  elseif kind == "coin" then
    if clock - coinAt < COIN_GAP then return end
    coinAt = clock
    local step = math.min(math.max(c.streak, 1), #LADDER)
    -- Each step a touch quieter, so a long streak doesn't build up.
    play("coin", vary(VOL.coin * (1 - 0.07 * (step - 1)), 1.5), semis(LADDER[step]) * jitter(0.006))
  elseif kind == "arcdone" then
    local v = vary(VOL.arc, 1)
    timer.after(0.07, function() play("arc", v) end)
    haptic("tap")
  elseif kind == "close" then
    play("close")
    haptic("success")
  elseif kind == "faster" then
    play("faster")
  elseif kind == "newkind" then
    play("new")
  elseif kind == "milestone" then
    play("milestone")
  elseif kind == "hit" then
    play("bonk")
    haptic("heavy")
    fadeOut()
  elseif kind == "fall" then
    play("fall")
    haptic("fail")
    fadeOut()
  elseif kind == "bounce" then
    bounces += 1
    play("bounce", VOL.bounce / bounces, semis(2 * bounces))
    haptic("tap")
  elseif kind == "stuck" then
    play("stuck")
    haptic("heavy")
  end
end

-- Each frame (the coin's folding needs the time).
function sound.update(dt)
  clock += dt
end

return sound
