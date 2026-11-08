-- What main.lua shares with game/bot.lua: the round as it is now (the
-- sim, what's selected, how many presses the game has taken), which
-- main.lua keeps here and the difficulty bot reads as a player sees the
-- screen. `presses` and `pressedAt` count the presses play has taken
-- and when the last one came.
return { game = nil :: any, sel = nil :: any, presses = 0, pressedAt = 0, live = false }
