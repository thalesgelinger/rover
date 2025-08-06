local rover = require "rover"
local keypressed = require "events".keypressed

local screen = {
    width = 30,
    height = 10,
}

local player = {
    position = {
        x = rover.signal(1),
        y = rover.signal(1),
    }
}

local food = {
    position = {
        x = rover.signal(math.random(screen.width)),
        y = rover.signal(math.random(screen.height)),
    }
}

local points = rover.signal(0)

rover.effect(function()
    local x, y = player.position.x.get(), player.position.y.get()
    local food_x, food_y = food.position.x.get(), food.position.y.get()

    if x == food_x and y == food_y then
        points.set(points.get() + 1)
        food.position.x.set(math.random(screen.width))
        food.position.y.set(math.random(screen.height))
    end
end)


rover.effect(function()
    local key = keypressed.get()

    local x, y = player.position.x, player.position.y
    local current_x, current_y = x.get(), y.get()

    if key == "up" then
        y.set(math.max(current_y - 1, 1))
    elseif key == "down" then
        y.set(math.min(current_y + 1, screen.height))
    elseif key == "left" then
        x.set(math.max(current_x - 1, 1))
    elseif key == "right" then
        x.set(math.min(current_x + 1, screen.width))
    elseif key == "q" then
        os.exit()
    end
end)

return rover.view {
    width = screen.width,
    height = screen.height,
    rover.view {
        "🐶",
        x = player.position.x,
        y = player.position.y,
    },
    rover.view {
        "🦴",
        x = food.position.x,
        y = food.position.y,
    },
    rover.view {
        "Points: " .. points,
        x = 1,
        y = screen.height,
    },
}
