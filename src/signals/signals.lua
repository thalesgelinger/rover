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
}

-- Signal creation
--- @class Signal
--- @field get function The position of the object
--- @field set function The health of the object
--- @param initialValue any
--- @return Signal
function signal(initialValue)
    local value = initialValue
    local subscriptions = {}

    local signalTable = {
        --- @return any initialValue
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
