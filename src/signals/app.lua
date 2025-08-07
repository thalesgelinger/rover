local rover = require "rover"
local keypressed = require "events".keypressed
local logger = require "logger"

local screen = {
    width = 30,
    height = 10,
}

logger.log("[APP INIT] Creating player signals\n")
local player = {
    position = {
        x = rover.signal(1),
        y = rover.signal(1),
    }
}
logger.log("[APP INIT] Player position signals created\n")

logger.log("[APP INIT] Creating food signals\n")
local food = {
    position = {
        x = rover.signal(math.random(screen.width)),
        y = rover.signal(math.random(screen.height)),
    }
}
logger.log("[APP INIT] Food position signals created\n")

logger.log("[APP INIT] Creating points signal\n")
local points = rover.signal(0)
logger.log("[APP INIT] Points signal created\n")

logger.log("[APP EFFECT] Setting up collision detection effect\n")
rover.effect(function()
    logger.log("[COLLISION CHECK] Checking for player-food collision\n")
    local x, y = player.position.x.get(), player.position.y.get()
    local food_x, food_y = food.position.x.get(), food.position.y.get()
    logger.log("[COLLISION DATA] Player at (" .. x .. ", " .. y .. "), Food at (" .. food_x .. ", " .. food_y .. ")\n")

    if x == food_x and y == food_y then
        logger.log("[COLLISION HIT] Player ate food at (" .. x .. ", " .. y .. ")\n")
        logger.log("[GAME ACTION] Increasing points\n")
        points.set(points.get() + 1)
        logger.log("[GAME STATE] Points updated to: " .. points.get() .. "\n")
        logger.log("[GAME ACTION] Repositioning food\n")
        food.position.x.set(math.random(screen.width))
        logger.log("[FOOD UPDATE] Food position x set to " .. food.position.x.get() .. "\n")
        food.position.y.set(math.random(screen.height))
        logger.log("[FOOD UPDATE] Food position y set to " .. food.position.y.get() .. "\n")
        logger.log("[COLLISION END] All collision updates complete\n")
    end
end)
logger.log("[APP EFFECT] Collision detection effect registered\n")


logger.log("[APP EFFECT] Setting up input handling effect\n")
rover.effect(function()
    logger.log("[INPUT CHECK] Checking for key press\n")
    local key = keypressed.get()
    logger.log("[INPUT DATA] Key pressed: " .. tostring(key) .. "\n")

    local x, y = player.position.x, player.position.y
    local current_x, current_y = x.get(), y.get()
    logger.log("[PLAYER STATE] Current player position: (" .. current_x .. ", " .. current_y .. ")\n")

    if key == "up" then
        local new_y = math.max(current_y - 1, 1)
        logger.log("[INPUT ACTION] Moving up from " .. current_y .. " to " .. new_y .. "\n")
        y.set(new_y)
    elseif key == "down" then
        local new_y = math.min(current_y + 1, screen.height)
        logger.log("[INPUT ACTION] Moving down from " .. current_y .. " to " .. new_y .. "\n")
        y.set(new_y)
    elseif key == "left" then
        local new_x = math.max(current_x - 1, 1)
        logger.log("[INPUT ACTION] Moving left from " .. current_x .. " to " .. new_x .. "\n")
        x.set(new_x)
    elseif key == "right" then
        local new_x = math.min(current_x + 1, screen.width)
        logger.log("[INPUT ACTION] Moving right from " .. current_x .. " to " .. new_x .. "\n")
        x.set(new_x)
    elseif key == "q" then
        logger.log("[INPUT ACTION] Quit key pressed, exiting application\n")
        os.exit()
    end
end)
logger.log("[APP EFFECT] Input handling effect registered\n")

logger.log("[APP VIEW] Creating application view\n")
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
