-- What main.lua shares with bot.lua: the hook and the top of the tower, as
-- a player sees them (bot.lua can't see main.lua's locals).
return {
  crane = { x = 180, y = 0, vx = 0, phase = 0, amp = 60, speed = 0.9, holding = false, h = 0, age = 0, n = 0 },
  tower = { x = 180, y = 430 },
  gravity = 981,
}
