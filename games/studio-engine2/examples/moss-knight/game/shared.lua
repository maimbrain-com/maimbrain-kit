-- What main.lua shares with game/bot.lua (the bot can't see main.lua's
-- locals, and scene.all doesn't list 3D nodes): the knight, the goblin and
-- the wisps' areas (a set).
local shared: { knight: any, goblin: any, wisps: { [any]: boolean } } = { knight = nil, goblin = nil, wisps = {} }
return shared
