local signals = require "signals"
local inspect = require "utils".inspect

local Rover = {
    signal = signals.signal,
    effect = signals.effect,
    derive = signals.derive,
    batch = signals.batch,
}

--- @class ViewProps
--- @field x Signal | number
--- @field y Signal | number
--- @field width Signal | number
--- @field height Signal | number
--- @property[1] string The symbol to display, can be a string or a table of strings
--- @return Signal
function Rover.view(props)
    return Rover.derive(function()
        return props
    end)
end

return Rover
