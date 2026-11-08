-- Trash Talk: the Rust UI Gallery's VOICE page (games/gallery, page 9) as a
-- Studio 2.0 game. Rocko and Granny face off in two big portraits; their
-- four lines play in the dialogue box (portraits whose mouths follow the lip
-- tracks, blinks, expressions, typed subtitles, the music dipping under each
-- voice). Then every tap lands a hit on one of them, in turn: a knock on
-- their portrait, an angry face, and a bark from voices/lines.csv's
-- "<who>_ouch" tag.

local LIPS = require("lips")

local TOP = 176                      -- the portraits' top edge (barks pop up above them)
local SHADOW = 0x00a7a0              -- riso's teal print shadow
local CELL = 192                    -- a portrait sheet's cell (images/portraits/*.toml)
local ROW = { neutral = 0, smug = 1, angry = 2, blink = 3 }
local SCRIPT = {
  { "rocko", "intro", "smug" },
  { "granny", "intro" },
  { "rocko", "taunt", "smug" },
  { "granny", "taunt", "smug" },
}

local pilots = {}                    -- by key: { panel, face, label, x, blinkAt, angryUntil }
local order = { "rocko", "granny" }
local speaking                       -- { who, line, face, t0 }: the line being heard
local ready = false                  -- the scripted lines are over: taps hit
local hits = 0
local hint, eq = nil, {}
local punch, bell, whoosh

-- The mouth (0 shut … 3 wide) the line's lip track has now, or nil once it's over.
local function mouthOf(s)
  local track = LIPS[s.who .. "/" .. s.line]
  if not track then return nil end
  local i = math.floor((round.time - s.t0) * track.hz) + 1
  if i < 1 or i > #track.mouth then return nil end
  return tonumber(track.mouth:sub(i, i))
end

local function pilot(key, name, x)
  local cx = x + 68
  local p = { x = cx, blinkAt = 1 + love.math.random() * 2, angryUntil = -1 }
  -- A card behind each face: tinted in the accent while they speak.
  -- (A print panel, as the identity draws its own: ink line, hard teal shadow.)
  p.shadow = Shape{ shape = "rect", w = 144, h = 152, x = cx + 5, y = TOP + 72 + 5, color = SHADOW }
  p.panel = Shape{ shape = "rect", w = 144, h = 152, x = cx, y = TOP + 72, color = ui.color("panel") }
  p.tint = Shape{ shape = "rect", w = 144, h = 152, x = cx, y = TOP + 72, color = ui.color("accent"), alpha = 0 }
  p.line = Shape{ shape = "rect", w = 144, h = 152, x = cx, y = TOP + 72, color = ui.color("text"), line = 3 }
  p.face = Sprite{ image = "portraits/" .. key, frame = 1, x = cx, y = TOP + 78, scale = 136 / CELL }
  p.label = Text{ text = name, font = "bricolage-grotesque-800", size = 20, align = "center", x = cx, y = TOP + 160, color = ui.color("text") }
  pilots[key] = p
  return p
end

-- The four lines as a cutscene: each waits for its voice, then moves on by
-- itself; a cue just before each line tells the big portrait up top to speak it.
local function trashTalk()
  local steps = {}
  for i, l in ipairs(SCRIPT) do
    steps[#steps + 1] = { "cue", "line" .. i }
    steps[#steps + 1] = { "say", l[1], l[2], l[3] }
    steps[#steps + 1] = { "wait", 0.35 }
    on("cue", "line" .. i, function()
      speaking = { who = l[1], line = l[2], face = l[3] or "neutral", t0 = round.time }
      whoosh:clone():play(); print(("line %s/%s %s %.2f"):format(l[1], l[2], l[3] or "neutral", round.time))
      fx.punch(pilots[l[1]].face, 0.06)
    end)
  end
  steps.done = function()
    speaking = nil
    ready = true
    flux.to(hint, 0.4, { alpha = 1 })
  end
  cutscene(steps)
end

-- A hit on whoever's turn it is: the knock, the face, the bark.
local function hit()
  hits = hits + 1
  round.score = hits
  local who = order[(hits - 1) % 2 + 1]
  local p = pilots[who]
  cast(who):hit(0.12)
  p.angryUntil = round.time + 1.2
  fx.punch(p.face, { x = who == "rocko" and -10 or 10, y = -4, scale = 0.08 })
  fx.shake(5, 0.2)
  fx.burst("sparks", p.x, TOP + 80, { count = 18, color = ui.color("gold") })
  fx.popup("OUCH!", p.x, TOP + 110, { color = ui.color("bad"), size = 22 })
  punch:clone():play()
  haptic("heavy")
  if bark(who .. "_ouch") then
    speaking = { who = who, line = "ouch", face = "angry", t0 = round.time }; print(("bark %s %.2f"):format(who, round.time))
  end
end

function love.load()
  love.graphics.setBackgroundColor(ui.color("background"))
  punch = love.audio.newSource("punch", "static")
  bell = love.audio.newSource("bell", "static")
  whoosh = love.audio.newSource("whoosh", "static")
  cast("rocko", { portrait = "portraits/rocko", side = "left", name = "ROCKO" })
  cast("granny", { portrait = "portraits/granny", side = "right", name = "GRANNY" })
  ui.pill{ text = "MAIN EVENT", icon = "trophy", x = 180, y = 120, tone = "accent" }
  pilot("rocko", "ROCKO", 22)
  pilot("granny", "GRANNY", 202)
  local vs = Text{ text = "VS", font = "bricolage-grotesque-800", size = 28, align = "center", x = 180, y = TOP + 56, color = ui.color("accent") }
  flux.to(vs, 0.5, { scale = 1.12 }):ease("sineinout"):yoyo()
  hint = Text{ text = "TAP: A HIT (THEY BARK)", size = 14, align = "center", x = 180, y = TOP + 222, color = ui.color("dim"), alpha = 0 }
  -- The music's level: bars that dance with it and sink when someone talks.
  for i = 1, 5 do
    eq[i] = Shape{ shape = "rect", w = 6, h = 20, x = 160 + (i - 1) * 10, y = TOP + 196, color = ui.color("accent") }
  end
  Text{ text = "MUSIC", size = 10, align = "center", x = 180, y = TOP + 210, color = ui.color("dim") }
  music.play("arena", { volume = 0.8 })
  bell:play()
  timer.after(0.8, trashTalk)
end

function love.mousepressed()
  if ready and not talking() then hit() end
end

function love.update(dt)
  local level = talking() and 0.45 or 1          -- the dialogue dips the music about 5 dB
  for i, b in ipairs(eq) do
    local h = 6 + 16 * level * (0.5 + 0.5 * math.abs(math.sin(round.time * (5 + i * 1.7) + i)))
    b.h = h
  end
  for _, key in ipairs(order) do
    local p = pilots[key]
    local on = speaking and speaking.who == key
    local face, mouth = "neutral", 0
    if on then
      mouth = mouthOf(speaking)
      if mouth == nil then
        mouth = 0
        if speaking.line == "ouch" then speaking = nil end
      end
      face = on and speaking and speaking.face or face
    end
    if round.time < p.angryUntil then face = "angry" end
    -- The neutral face blinks now and then.
    if face == "neutral" and round.time >= p.blinkAt then
      face = "blink"
      if not p.blinking then
        p.blinking = true
        print(("blink %s %.2f"):format(key, round.time))
      end
      if round.time > p.blinkAt + 0.13 then
        p.blinking = false
        p.blinkAt = round.time + 2.2 + love.math.random() * 2.5
      end
    end
    p.face.frame = ROW[face] * 4 + mouth + 1
    -- The speaker leans in; the other sits back.
    local y = TOP + (on and 78 or 84)
    p.face.y = p.face.y + (y - p.face.y) * math.min(1, dt * 14)
    p.tint.alpha = p.tint.alpha + ((on and 0.22 or 0) - p.tint.alpha) * math.min(1, dt * 10)
  end
end
