local subscriber = nil

local function createOperatorFunction(fn)
    return function(a, b)
        local getA, getB

        if type(a) == "table" then
            getA = a.get
        else
            getA = function() return a end
        end

        if type(b) == "table" then
            getB = b.get
        else
            getB = function() return b end
        end

        return derive(function()
            return fn(getA(), getB())
        end)
    end
end

local signalMetaTable = {
    __add = createOperatorFunction(function(a, b)
        return a + b
    end),
    __sub = createOperatorFunction(function(a, b)
        return a - b
    end),
    __mul = createOperatorFunction(function(a, b)
        return a * b
    end),
    __div = createOperatorFunction(function(a, b)
        return a / b
    end),
    __unm = createOperatorFunction(function(a)
        return -a
    end),
    __mod = createOperatorFunction(function(a, b)
        return a % b
    end),
    __pow = createOperatorFunction(function(a, b)
        return a ^ b
    end),
    __idiv = createOperatorFunction(function(a, b)
        return a // b
    end),
    __band = createOperatorFunction(function(a, b)
        return a & b
    end),
    __bor = createOperatorFunction(function(a, b)
        return a | b
    end),
    __bxor = createOperatorFunction(function(a, b)
        return a ~ b
    end),
    __bnot = createOperatorFunction(function(a)
        return ~a
    end),
    __shl = createOperatorFunction(function(a, b)
        return a << b
    end),
    __shr = createOperatorFunction(function(a, b)
        return a >> b
    end),
    __eq = createOperatorFunction(function(a, b)
        return a == b
    end),
    __lt = createOperatorFunction(function(a, b)
        return a < b
    end),
    __le = createOperatorFunction(function(a, b)
        return a <= b
    end),
    __concat = createOperatorFunction(function(a, b)
        return a .. b
    end),
    __len = createOperatorFunction(function(a)
        return #a
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
