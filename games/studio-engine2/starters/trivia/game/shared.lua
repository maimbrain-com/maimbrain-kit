-- What the difficulty bot (game/bot.lua) reads: the round's sim and where
-- the answers are. main.lua fills it in; both files `require("shared")`
-- and get this same table (a bot's globals aren't the game's).
return {
  game = nil, -- the round's sim (game/sim.lua), once a round is playing
  layout = nil, -- look.layout(): the answers' places
}
