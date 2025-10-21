local subscriber = nil
local batchDepth = 0
local pendingNotifications = {}
local Signals = {}

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
    __add = wrapper(function(a, b) return a + b end),
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

function Signals.signal(initialValue)
    local value = initialValue
    local subscriptions = {}
    local signalTable = {}

    signalTable.get = function()
        if subscriber and not subscriptions[subscriber] then
            subscriptions[subscriber] = true
        end
        return value
    end

    signalTable.set = function(updated)
        if value ~= updated then
            value = updated
            if batchDepth > 0 then
                -- We're in a batch, collect subscribers for later notification
                for fn, _ in pairs(subscriptions) do
                    pendingNotifications[fn] = true
                end
            else
                -- Not in a batch, notify immediately
                for fn, _ in pairs(subscriptions) do
                    fn()
                end
            end
        end
    end

    return setmetatable(signalTable, signalMetaTable)
end

-- Helper function to execute an effect with proper batching and subscriber tracking
local function runEffect(fn)
    local prev = subscriber
    subscriber = fn

    batchDepth = batchDepth + 1
    local success, result = pcall(fn)
    batchDepth = batchDepth - 1

    subscriber = prev

    if not success then
        error(result)
    end

    return result
end

function Signals.effect(fn)
    -- Initial effect execution
    runEffect(fn)

    -- Flush all pending notifications after the initial effect
    -- This handles cascading updates properly
    if batchDepth == 0 then
        while next(pendingNotifications) ~= nil do
            local notifications = pendingNotifications
            pendingNotifications = {}

            -- Run each effect in a batched context
            for notifyFn, _ in pairs(notifications) do
                runEffect(notifyFn)
            end
        end
    end
end

function Signals.derive(fn)
    local derived = Signals.signal(nil)
    Signals.effect(function()
        local value = fn()
        derived.set(value)
    end)
    return derived
end

-- Batch function for manual batching of multiple signal updates
function Signals.batch(fn)
    batchDepth = batchDepth + 1
    local success, result = pcall(fn)
    batchDepth = batchDepth - 1

    -- If we're back to depth 0, flush all pending notifications
    if batchDepth == 0 then
        while next(pendingNotifications) ~= nil do
            local notifications = pendingNotifications
            pendingNotifications = {}

            -- Run each effect in a batched context
            for notifyFn, _ in pairs(notifications) do
                runEffect(notifyFn)
            end
        end
    end

    if not success then
        error(result)
    end

    return result
end

return Signals

