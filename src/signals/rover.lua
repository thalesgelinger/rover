local signals = require "signals"

local Rover = {
    signal = signals.signal,
    effect = signals.effect,
    derive = signals.derive,
}


--- @class ViewProps
--- @property x signals.Signal
--- @property y signals.Signal
--- @return signals.Signal
function Rover.view(props)
    return Rover.derive(function()
        local x = props.x and props.x.get() or 0
        local y = props.y and props.y.get() or 0
        local symbol = props[1] and props[1] or "🐍"

        if type(symbol) == "table" then
            return {
                x = x,
                y = y,
                symbol.get(),
            }
        end
        return {
            x = x,
            y = y,
            symbol,
        }
    end)
end

return Rover
