local rover = require("rover")
local read_key = require("events").read_key


--- @class Opts
--- @field debug boolean

--- @param app Signal
--- @param opts Opts
function UI(app, opts)
    os.execute("clear")

    local width = 30
    local height = 20

    local gridchar = " "

    os.execute("stty raw -echo")
    rover.effect(function()
        io.write("\27[2J\27[H")
        local comp = app.get()
        for i = 1, height do
            for j = 1, width do
                if j == comp.x and i == comp.y then
                    io.write(comp[1])
                else
                    io.write(gridchar)
                end
                io.write(gridchar)
            end
            io.write("\r\n")
        end
        if opts.debug then
            io.write("Coordinatess: ( x: ", comp.x, ", y: ", comp.y, ")\r\n")
            io.write("Use arrow keys to move, 'q' to quit\r\n")
        end
        read_key()
        io.flush()
    end)

    os.execute("stty sane")
end

return UI
