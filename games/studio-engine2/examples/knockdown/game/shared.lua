-- What main.lua shares with game/bot.lua (the bot can't see main.lua's
-- locals, and scene.all doesn't list 3D nodes): the targets' sizes, which
-- main.lua fills in, and the round's own lists.
return {
  CAN_R = 0, CAN_H = 0, CRATE = 0, PLANK_W = 0, PLANK_H = 0,
  targets = {},   -- { body, kind = "can" | "crate" | "plank", down }
  balls = {},     -- { body, age }
  stack = 0,
  ballsLeft = 0,
  phase = "aim",  -- "aim" | "cleared" | "over"
  cooldown = 0,
  clock = 0,
  throws = 0,
}
