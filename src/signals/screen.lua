local rover = require("rover")
local read_key = require("events").read_key
--- @class Opts
--- @field debug boolean

--- @param app Signal
--- @param opts Opts
function UI(app, opts)
    os.execute("clear")

    local gridchar = " "

    os.execute("stty raw -echo")
    rover.effect(function()
        io.write("\27[2J\27[H")
        local c = app.get()
        local height = c.height or 10 -- Ensure a default value for height
        local width = c.width or 30   -- Ensure a default value for width
        for i = 1, height do
            for j = 1, width do
                local drawn = false
                local function render_component(comp)
                    if comp.x and comp.y then
                        if j == comp.x and i == comp.y then
                            io.write(comp[1])
                            return true
                        end
                    end
                    if #comp >= 1 then
                        for _, child in ipairs(comp) do
                            if render_component(child) then
                                return true
                            end
                        end
                    end
                    return false
                end

                for _, comp in ipairs(c) do
                    if render_component(comp) then
                        drawn = true
                        break
                    end
                end
                if not drawn then
                    io.write(gridchar)
                end
                io.write(gridchar)
            end
            io.write("\r\n")
        end
        if opts.debug then
            for _, comp in ipairs(c) do
                io.write("Component: ( x: ", comp.x, ", y: ", comp.y, ")\r\n")
            end
            io.write("Use arrow keys to move, 'q' to quit\r\n")
        end
        read_key()
        io.flush()
    end)

    os.execute("stty sane")
end

return {
    UI = UI,
}
