local rover = require "rover"
local keypressed = require "events".keypressed


function App()
    local x = rover.signal(1)
    local y = rover.signal(1)

    rover.effect(function()
        local key = keypressed.get()

        local currentX, currentY = x.get(), y.get()
        if key == "up" and currentY >= 1 then
            y.set(currentY - 1)
        elseif key == "down" then
            y.set(currentY + 1)
        elseif key == "left" and currentX >= 1 then
            x.set(currentX - 1)
        elseif key == "right" then
            x.set(currentX + 1)
        elseif key == "q" then
            os.exit()
        end
    end)

    return rover.view {
        "🐍",
        x = x,
        y = y,
    }
end

return App
