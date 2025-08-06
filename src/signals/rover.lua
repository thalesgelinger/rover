local signals = require "signals"
local inspect = require "utils".inspect

local Rover = {
    signal = signals.signal,
    effect = signals.effect,
    derive = signals.derive,
}

--- @class ViewProps
--- @field x Signal | number
--- @field y Signal | number
--- @field width Signal | number
--- @field height Signal | number
--- @property[1] string The symbol to display, can be a string or a table of strings
--- @return Signal
function Rover.view(props)
    local function parseSignalsToStatic(values)
        local staticValues = {}

        for key, value in pairs(values) do
            print(key, value)

            if type(value) == "table" and value.get then
                staticValues[key] = value.get()
            else
                staticValues[key] = value
            end

            inspect(staticValues)
        end
        return staticValues
    end

    return Rover.derive(function()
        local parsedView = parseSignalsToStatic(props)
        for _, value in ipairs(props) do
            local child = type(value) == "table" and value.get() or value
            if type(child) == "string" then
                table.insert(parsedView, child)
            elseif type(child) == "table" then
                table.insert(parsedView, parseSignalsToStatic(child))
            end
        end

        return parsedView
    end)
end

return Rover
