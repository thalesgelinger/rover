local rover = require("rover")
local read_key = require("events").read_key
local unwrap = require("utils").unwrap
local inspect = require("utils").inspect


local gridchar = " "

--- @class Opts
--- @field debug boolean

--- @param app Signal
--- @param opts Opts
function UI(app, opts)
    os.execute("clear")

    os.execute("stty raw -echo")
    rover.effect(function()
        io.write("\27[2J\27[H")
        Render(app)
        if opts.debug then
            inspect(app.get())
            io.write("Use arrow keys to move, 'q' to quit\r\n")
        end
        read_key()
        io.flush()
    end)

    os.execute("stty sane")
end

function Render(app)
    local c = app.get()
    local height = c.height or 10 -- Ensure a default value for height
    local width = c.width or 30   -- Ensure a default value for width
    for i = 1, height do
        for j = 1, width do
            local drawn = false
            local function render_component(compSignal)
                local comp = unwrap(compSignal)
                local x, y = unwrap(comp.x), unwrap(comp.y)

                if x and y then
                    if j == x and i == y then
                        io.write(unwrap(comp[1]))
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
end

return {
    UI = UI,
}
