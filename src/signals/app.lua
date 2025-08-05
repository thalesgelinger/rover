local rover = require "rover"
local keypressed = require "events".keypressed

local player = {
    position = {
        x = rover.signal(1),
        y = rover.signal(1),
    }
}

rover.effect(function()
    local key = keypressed.get()

    local x, y = player.position.x, player.position.y

    if key == "up" then
        y.set(y.get() - 1)
    elseif key == "down" then
        y.set(y.get() + 1)
    elseif key == "left" then
        x.set(x.get() - 1)
    elseif key == "right" then
        x.set(x.get() + 1)
    elseif key == "q" then
        os.exit()
    end
end)

return rover.view {
    "🐍",
    x = player.position.x,
    y = player.position.y,
}
