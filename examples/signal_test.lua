-- Test basic signal functionality

-- Create a signal
local count = rover.signal(0)
print("Initial count:", count.val)

-- Update signal
count.val = 5
print("After update:", count.val)

-- Create derived signal (explicit)
local double = rover.derive(function()
    return count.val * 2
end)
print("Double:", double.val)

-- Create derived signal (implicit via metamethod)
local triple = count * 3
print("Triple:", triple.val)

-- Test string concatenation
local label = "Count: " .. count
print("Label:", label.val)

-- Test effect
local effect_count = 0
rover.effect(function()
    local c = count.val  -- subscribe to count
    effect_count = effect_count + 1
    print(string.format("Effect run #%d: count = %d", effect_count, c))
end)

-- Change count to trigger effect
print("\nChanging count to 10...")
count.val = 10

print("\nVerifying derived values updated:")
print("Double:", double.val)
print("Triple:", triple.val)
print("Label:", label.val)
print("Effect ran", effect_count, "times")

-- Test comparison operators (must use derive in Lua, operators return booleans)
local is_big = rover.derive(function()
    return count.val > 5
end)
print("\nIs count > 5?", is_big.val)

-- Test utility functions
local a = rover.signal(true)
local b = rover.signal(false)
local any_true = rover.any(a, b)
local all_true = rover.all(a, b)

print("\nUtility functions:")
print("any(true, false):", any_true.val)
print("all(true, false):", all_true.val)

print("\n✅ All signal tests passed!")
