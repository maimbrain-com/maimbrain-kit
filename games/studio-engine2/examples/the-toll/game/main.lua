-- The Toll (docs/STUDIO-2.md §6.4), Phase 4a's acceptance game. Its art
-- would come from generate: background "bridge", walker "pip", image "gruk",
-- image "gate" (frames: shut, open), portrait "gruk" (smug, annoyed,
-- laughing, bored), portrait "pip" (nervous, happy); voice design "gruk",
-- voice cast "pip", voice script voices/lines.csv; sfx "gate"; music
-- "bridge". Here it's placeholder art drawn by a script, mb voice's
-- offline stand-in voices, and synth sounds and music.

local RIDDLES = {
  { line = "riddle1", answers = { "A shadow", "A river", "A coat" }, right = 2, yes = "right1" },
  { line = "riddle2", answers = { "Light", "A secret", "An echo" }, right = 1, yes = "right2" },
}

local pip, gruk, gate, riddle, tries, solved, ask

-- Over the bridge: the gate goes up, Pip thanks him and walks on, and the
-- scene closes on the bridge with the credits.
local function cross()
  gate:play("open")
  love.audio.newSource("gate", "static"):play()
  fx.shake(3, 0.3)
  solved = solved + 1
  progress.set("solved", solved)             -- verified by the server's replay
  round.score = solved
  flux.to(pip, 4.5, { x = 420 }):ease("linear")
  cutscene{
    { "letterbox", 0.14 },
    { "say", "pip", "thanks", "happy" },
    { "camera", 205, 470, 1.15, 2.5, "inOutSine" }, "meanwhile",
    { "title", "ACROSS", "riddles answered: " .. solved, 2.4 },
    { "camera", 180, 320, 1, 3, "inOutSine" }, "meanwhile",
    { "credits", { { "THE TROLL", "GRUK" }, { "THE TRAVELLER", "PIP" }, { "THE BRIDGE", "OLD STONE" } }, 6 },
    { "fade", "out", 1 },
    done = function() round.over("ACROSS!", "solved", true) end,
  }
end

ask = function()
  dialogue{
    { "gruk", riddle.line, "smug", choices = riddle.answers },
    done = function(choice)
      if choice == riddle.right then
        cast("pip").expression = "happy"
        cast("gruk").expression = "annoyed"
        say("gruk", riddle.yes, "annoyed").done:connect(cross)   -- the gate opens when he's said it
      else
        tries = tries + 1
        bark("wrong")
        cast("pip"):hit(0.34)
        haptic("fail")
        fx.shake(2, 0.2)
        if tries == 3 then
          timer.after(2.2, function() round.over("TURNED BACK", "wrong") end)
        else
          timer.after(2.4, ask)
        end
      end
    end,
  }
end

function love.load()
  Sprite{ image = "bridge", x = 180, y = 320, scale = 0.5, order = -10 }
  gate = Sprite{ image = "gate", x = 214, y = 474 }
  gate:play("shut")
  gruk = Sprite{ image = "gruk", x = 290, y = 448 }
  pip = Walker{ image = "pip", height = 52, x = -40, y = 526, order = 2 }   -- on the near side of the deck
  cast("gruk", { portrait = "gruk", side = "right", name = "GRUK" })
  cast("pip", { portrait = "pip", side = "left", name = "PIP" })
  solved, tries = progress.get("solved") or 0, 0
  riddle = RIDDLES[solved % #RIDDLES + 1]
  music.play("bridge", { volume = 0.6 })
  timer.every(9, function() bark("idle") end)   -- barks never talk over a line

  -- Pip walks up; then the scene.
  flux.to(pip, 2.4, { x = 120 }):ease("linear"):oncomplete(function()
    cutscene{
      { "letterbox", 0.12 },
      { "camera", 200, 470, 1.35, 1, "inOutSine" },
      { "say", "gruk", "halt", "smug" },
      { "say", "pip", "please", "nervous" },
      done = ask,                            -- also when a long press skips it
    }
  end)
end

-- Gruk's breathing: a slow bob on his bridge.
function love.update(dt)
  gruk.sy = 1 + 0.015 * math.sin(round.time * 2.1)
end
