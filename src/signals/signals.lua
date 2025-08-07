local logger = require "logger"
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
    local signalId = tostring(signalTable):match("0x(%x+)")
    
    logger.log("[SIGNAL CREATE] Signal " .. signalId .. " created with initial value: " .. tostring(initialValue) .. "\n")

    signalTable.get = function()
        logger.log("[SIGNAL GET] Signal " .. signalId .. " accessed, current value: " .. tostring(value) .. "\n")
        if subscriber and not subscriptions[subscriber] then
            local subscriberId = tostring(subscriber):match("0x(%x+)")
            logger.log("[SIGNAL SUBSCRIBE] Signal " .. signalId .. " subscribing function " .. subscriberId .. "\n")
            subscriptions[subscriber] = true
        end
        return value
    end

    signalTable.set = function(updated)
        logger.log("[SIGNAL SET] Signal " .. signalId .. " set from " .. tostring(value) .. " to " .. tostring(updated) .. "\n")
        if value ~= updated then
            value = updated
            logger.log("[SIGNAL UPDATE] Signal " .. signalId .. " value changed, batch depth: " .. batchDepth .. "\n")
            
            local subscriberCount = 0
            for _ in pairs(subscriptions) do subscriberCount = subscriberCount + 1 end
            logger.log("[SIGNAL NOTIFY] Signal " .. signalId .. " notifying " .. subscriberCount .. " subscribers\n")
            
            if batchDepth > 0 then
                -- We're in a batch, collect subscribers for later notification
                logger.log("[BATCH] Adding subscribers to pending notifications (batch depth: " .. batchDepth .. ")\n")
                for fn, _ in pairs(subscriptions) do
                    local fnId = tostring(fn):match("0x(%x+)")
                    logger.log("[BATCH PENDING] Function " .. fnId .. " added to pending notifications\n")
                    pendingNotifications[fn] = true
                end
            else
                -- Not in a batch, notify immediately
                logger.log("[IMMEDIATE NOTIFY] Notifying subscribers immediately\n")
                for fn, _ in pairs(subscriptions) do
                    local fnId = tostring(fn):match("0x(%x+)")
                    logger.log("[SUBSCRIBER CALL] Calling function " .. fnId .. "\n")
                    fn()
                end
            end
        else
            logger.log("[SIGNAL UNCHANGED] Signal " .. signalId .. " value unchanged, no notifications sent\n")
        end
    end

    return setmetatable(signalTable, signalMetaTable)
end

function Signals.effect(fn)
    local fnId = tostring(fn):match("0x(%x+)")
    logger.log("[EFFECT START] Running effect " .. fnId .. "\n")
    
    local prev = subscriber
    subscriber = fn
    logger.log("[EFFECT CONTEXT] Set current subscriber to " .. fnId .. "\n")

    -- Automatically batch all set operations during effect execution
    batchDepth = batchDepth + 1
    logger.log("[BATCH START] Batch depth increased to " .. batchDepth .. "\n")
    
    local success, result = pcall(fn)
    
    batchDepth = batchDepth - 1
    logger.log("[BATCH END] Batch depth decreased to " .. batchDepth .. "\n")

    -- Flush notifications after effect completes
    if batchDepth == 0 then
        local notificationCount = 0
        for _ in pairs(pendingNotifications) do notificationCount = notificationCount + 1 end
        logger.log("[FLUSH START] Flushing " .. notificationCount .. " pending notifications\n")
        
        local notifications = pendingNotifications
        pendingNotifications = {}
        
        for notifyFn, _ in pairs(notifications) do
            local notifyFnId = tostring(notifyFn):match("0x(%x+)")
            logger.log("[FLUSH CALL] Calling pending function " .. notifyFnId .. "\n")
            notifyFn()
        end
        
        logger.log("[FLUSH END] All pending notifications flushed\n")
    end

    subscriber = prev
    logger.log("[EFFECT CONTEXT] Restored previous subscriber\n")

    if not success then
        logger.log("[EFFECT ERROR] Effect " .. fnId .. " failed with error: " .. tostring(result) .. "\n")
        error(result)
    end

    logger.log("[EFFECT END] Effect " .. fnId .. " completed successfully\n")
    return result
end

function Signals.derive(fn)
    local fnId = tostring(fn):match("0x(%x+)")
    logger.log("[DERIVE START] Creating derived signal with function " .. fnId .. "\n")
    
    local derived = Signals.signal(nil)
    local derivedId = tostring(derived):match("0x(%x+)")
    logger.log("[DERIVE SIGNAL] Created derived signal " .. derivedId .. "\n")
    
    Signals.effect(function()
        logger.log("[DERIVE EFFECT] Running derivation function " .. fnId .. "\n")
        local value = fn()
        logger.log("[DERIVE VALUE] Derived function returned: " .. tostring(value) .. "\n")
        derived.set(value)
    end)
    
    logger.log("[DERIVE END] Derived signal " .. derivedId .. " setup complete\n")
    return derived
end

-- New batch function
function Signals.batch(fn)
    local fnId = tostring(fn):match("0x(%x+)")
    logger.log("[BATCH FUNCTION START] Starting batch function " .. fnId .. "\n")
    
    batchDepth = batchDepth + 1
    logger.log("[BATCH DEPTH] Batch depth increased to " .. batchDepth .. "\n")
    
    local success, result = pcall(fn)
    
    batchDepth = batchDepth - 1
    logger.log("[BATCH DEPTH] Batch depth decreased to " .. batchDepth .. "\n")

    -- If we're back to depth 0, flush all pending notifications
    if batchDepth == 0 then
        local notificationCount = 0
        for _ in pairs(pendingNotifications) do notificationCount = notificationCount + 1 end
        logger.log("[BATCH FLUSH START] Flushing " .. notificationCount .. " pending notifications\n")
        
        local notifications = pendingNotifications
        pendingNotifications = {}
        
        for notifyFn, _ in pairs(notifications) do
            local notifyFnId = tostring(notifyFn):match("0x(%x+)")
            logger.log("[BATCH FLUSH CALL] Calling pending function " .. notifyFnId .. "\n")
            notifyFn()
        end
        
        logger.log("[BATCH FLUSH END] All pending notifications flushed\n")
    end

    if not success then
        logger.log("[BATCH ERROR] Batch function " .. fnId .. " failed with error: " .. tostring(result) .. "\n")
        error(result)
    end

    logger.log("[BATCH FUNCTION END] Batch function " .. fnId .. " completed successfully\n")
    return result
end

return Signals
