-- Test to verify batch updates work correctly
package.path = package.path .. ";./src/signals/?.lua"

local Signals = require("signals")

print("Testing batch updates with self-modifying effects...\n")

-- Track how many times the effect runs
local effectRunCount = 0

-- Create signals
local counter = Signals.signal(0)
local doubled = Signals.signal(0)

-- Create an effect that modifies a signal it depends on
-- This should only run ONCE after all updates are done
Signals.effect(function()
    effectRunCount = effectRunCount + 1
    local c = counter.get()

    print(string.format("Effect run #%d: counter = %d", effectRunCount, c))

    -- Update doubled value (this signal is read by this effect)
    doubled.set(c * 2)

    print(string.format("  Set doubled to %d", c * 2))
end)

print("\n--- Triggering counter update ---\n")
counter.set(5)

print(string.format("\nFinal state:"))
print(string.format("  counter = %d", counter.get()))
print(string.format("  doubled = %d", doubled.get()))
print(string.format("  Effect ran %d times", effectRunCount))

-- Test cascading updates
print("\n\n=== Testing cascading effects ===\n")

local a = Signals.signal(1)
local b = Signals.signal(0)
local c = Signals.signal(0)

local aEffectCount = 0
local bEffectCount = 0

-- Effect that depends on 'a' and updates 'b'
Signals.effect(function()
    aEffectCount = aEffectCount + 1
    local val = a.get()
    print(string.format("Effect A (run #%d): a = %d, setting b = %d", aEffectCount, val, val + 10))
    b.set(val + 10)
end)

-- Effect that depends on 'b' and updates 'c'
Signals.effect(function()
    bEffectCount = bEffectCount + 1
    local val = b.get()
    print(string.format("Effect B (run #%d): b = %d, setting c = %d", bEffectCount, val, val + 100))
    c.set(val + 100)
end)

print("\n--- Updating 'a' to 5 ---\n")
a.set(5)

print(string.format("\nFinal state:"))
print(string.format("  a = %d", a.get()))
print(string.format("  b = %d", b.get()))
print(string.format("  c = %d", c.get()))
print(string.format("  Effect A ran %d times total", aEffectCount))
print(string.format("  Effect B ran %d times total", bEffectCount))

print("\n✅ Test complete! Effects should run exactly once per trigger.")
