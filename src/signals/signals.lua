local subscriber = nil

local signalMetaTable = {
    __mul = function(a, b)
        if type(a) == "table" and type(b) == "table" then
            return derive(function()
                return a.get() * b.get()
            end)
        end

        if type(a) == "table" then
            return derive(function()
                return a.get() * b
            end)
        end
        if type(b) == "table" then
            return derive(function()
                return a * b.get()
            end)
        end
    end,
    __concat = function(a, b)
        if type(a) == "table" and type(b) == "table" then
            return derive(function()
                return a.get() .. b.get()
            end)
        end

        if type(a) == "table" then
            return derive(function()
                return a.get() .. b
            end)
        end
        if type(b) == "table" then
            return derive(function()
                return a .. b.get()
            end)
        end
    end,
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
            if subscriber then
                table.insert(subscriptions, subscriber)
            end
            return value
        end,
        set = function(updated)
            value = updated
            for _, fn in ipairs(subscriptions) do
                fn()
            end
        end
    }


    return setmetatable(signalTable, signalMetaTable)
end

-- Effect function
---@param fn function
function effect(fn)
    subscriber = fn
    fn()
    subscriber = nil
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

-- Component that parses the string and listens for signal changes
---@param param string | Signal
function component(param)
    if type(param) == "string" then
        print("Component: " .. param)
    else
        effect(function()
            print("Component: " .. param.get())
        end)
    end
end

-- Return the module
local rover = {
    signal = signal,
    effect = effect,
    derive = derive,
    component = component
}

return rover
