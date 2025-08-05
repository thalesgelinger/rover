local subscriber = nil

local wrapper = function(exec)
    return function(a, b)
        if type(a) == "table" and type(b) == "table" then
            return derive(function()
                return exec(a.get(), b.get())
            end)
        end

        if type(a) == "table" then
            return derive(function()
                return exec(a.get(), b)
            end)
        end
        if type(b) == "table" then
            return derive(function()
                return exec(a, b.get())
            end)
        end
    end
end

local signalMetaTable = {
    __sum = wrapper(function(a, b)
        return a + b
    end),
    __mul = wrapper(function(a, b)
        return a * b
    end),
    __concat = wrapper(function(a, b)
        return tostring(a) .. tostring(b)
    end),
    __mod = wrapper(function(a, b)
        return a % b
    end),
    __sub = wrapper(function(a, b)
        return a - b
    end),
    __div = wrapper(function(a, b)
        return a / b
    end),
    __pow = wrapper(function(a, b)
        return a ^ b
    end),
    __unm = wrapper(function(a)
        return -a
    end),
    __eq = wrapper(function(a, b)
        return a == b
    end),
    __lt = wrapper(function(a, b)
        return a < b
    end),
    __le = wrapper(function(a, b)
        return a <= b
    end),
    __tostring = function(self)
        return tostring(self.get())
    end,
}

--- Signal creation
--- @class Signal<T>
--- @field get fun(): T Returns the same type as initialValue
--- @field set fun(value: T)  Receives a parameter of the same type as initialValue

--- @generic T
--- @param initialValue T
--- @return Signal<T>
function signal(initialValue)
    local value = initialValue
    local subscriptions = {}

    local signalTable = {
        get = function()
            if subscriber and not subscriptions[subscriber] then
                subscriptions[subscriber] = true
            end
            return value
        end,
        set = function(updated)
            if value ~= updated then -- Only update if value changed
                value = updated
                for fn, _ in pairs(subscriptions) do
                    fn()
                end
            end
        end
    }

    return setmetatable(signalTable, signalMetaTable)
end

-- Effect function
---@param fn function
function effect(fn)
    local prev = subscriber
    subscriber = fn
    fn()
    subscriber = prev
end

-- Derive a new value from signals
---@param fn function
function derive(fn)
    local derived = signal()

    effect(function()
        local value = fn()
        derived.set(value)
    end)

    return derived
end

return {
    signal = signal,
    effect = effect,
    derive = derive
}
