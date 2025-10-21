local Signals = require "signals"

local wrapper = function(exec)
    return function(a, b)
        if type(a) == "table" and type(b) == "table" then
            return Signals.derive(function()
                return exec(a.get(), b.get())
            end)
        end

        if type(a) == "table" then
            return Signals.derive(function()
                return exec(a.get(), b)
            end)
        end
        if type(b) == "table" then
            return Signals.derive(function()
                return exec(a, b.get())
            end)
        end
    end
end

local signalMetaTable = {
    __sum = wrapper(function(a, b) return a + b end),
    __mul = wrapper(function(a, b) return a * b end),
    __concat = wrapper(function(a, b) return tostring(a) .. tostring(b) end),
    __mod = wrapper(function(a, b) return a % b end),
    __sub = wrapper(function(a, b) return a - b end),
    __div = wrapper(function(a, b) return a / b end),
    __pow = wrapper(function(a, b) return a ^ b end),
    __unm = wrapper(function(a) return -a end),
    __eq = wrapper(function(a, b) return a == b end),
    __lt = wrapper(function(a, b) return a < b end),
    __le = wrapper(function(a, b) return a <= b end),
    __tostring = function(self) return tostring(self.get()) end,
}

return signalMetaTable
