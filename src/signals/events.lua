local rover = require "rover"

local keypressed = rover.signal("")

local function read_key()
    local key = io.read(1)
    if key == "\27" then
        local next1 = io.read(1)
        if next1 == "[" then
            local next2 = io.read(1)
            if next2 == "A" then
                keypressed.set "up"
            elseif next2 == "B" then
                keypressed.set "down"
            elseif next2 == "C" then
                keypressed.set "right"
            elseif next2 == "D" then
                keypressed.set "left"
            end
        end
    elseif key == "q" then
        keypressed.set "q"
    end
end

return {
    keypressed = keypressed,
    read_key = read_key
}
