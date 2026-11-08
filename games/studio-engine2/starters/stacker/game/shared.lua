-- What main.lua shares with game/bot.lua: the round's sim (the tower, the
-- bird, the snack it carries) and where that snack would land right now,
-- which main.lua keeps here and the difficulty bot reads as a player sees
-- the screen.
return { sim = nil :: any, pred = nil :: any }
