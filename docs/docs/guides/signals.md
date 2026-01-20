# Signals

Signals are Rover's reactive state management system, providing fine-grained reactivity for building dynamic UIs. Inspired by SolidJS and other reactive frameworks, signals offer automatic dependency tracking, lazy evaluation, and zero-allocation reads.

## Overview

Rover's signal system consists of three main primitives:

- **Signals**: Writable reactive values
- **Derived Signals**: Computed values that automatically update when dependencies change
- **Effects**: Side effects that automatically re-run when dependencies change

## Basic Usage

### Creating Signals

Create a signal with an initial value:

```lua
local count = rover.signal(0)
print(count.val)  -- 0
```

Update a signal by assigning to `.val`:

```lua
count.val = 5
print(count.val)  -- 5
```

### Derived Signals

#### Explicit Derivation

Create derived signals using `rover.derive()`:

```lua
local count = rover.signal(10)

local double = rover.derive(function()
    return count.val * 2
end)

print(double.val)  -- 20

count.val = 15
print(double.val)  -- 30 (automatically updated)
```

#### Implicit Derivation (Metamethods)

Rover supports creating derived signals implicitly using operators:

```lua
local count = rover.signal(5)

-- Arithmetic operators
local double = count * 2
local sum = count + 10
local difference = count - 3
local quotient = count / 2
local remainder = count % 3
local power = count ^ 2

print(double.val)      -- 10
print(sum.val)         -- 15
print(difference.val)  -- 2
print(quotient.val)    -- 2.5
print(remainder.val)   -- 2
print(power.val)       -- 25

-- String concatenation
local label = "Count: " .. count
print(label.val)  -- "Count: 5"

-- Works with signal on either side
local prefix = count .. " items"
print(prefix.val)  -- "5 items"
```

### Effects

Effects automatically track signal dependencies and re-run when they change:

```lua
local count = rover.signal(0)

rover.effect(function()
    print("Count is now:", count.val)
end)
-- Prints: "Count is now: 0"

count.val = 10
-- Prints: "Count is now: 10"

count.val = 20
-- Prints: "Count is now: 20"
```

Effects with cleanup:

```lua
rover.effect(function()
    local timer = start_timer(count.val)

    -- Return a cleanup function
    return function()
        stop_timer(timer)
    end
end)
```

### Utility Functions

#### rover.any()

Returns a derived signal that's `true` if any input is truthy:

```lua
local a = rover.signal(false)
local b = rover.signal(true)
local c = rover.signal(false)

local any_true = rover.any(a, b, c)
print(any_true.val)  -- true

b.val = false
print(any_true.val)  -- false
```

#### rover.all()

Returns a derived signal that's `true` if all inputs are truthy:

```lua
local a = rover.signal(true)
local b = rover.signal(true)
local c = rover.signal(false)

local all_true = rover.all(a, b, c)
print(all_true.val)  -- false

c.val = true
print(all_true.val)  -- true
```

## Important Limitations

### Comparison Operators

Due to Lua's semantics, **comparison operators (`<`, `>`, `<=`, `>=`, `==`, `~=`) cannot return derived signals**. They must return boolean values for use in conditionals.

**This will NOT work as expected:**

```lua
local count = rover.signal(10)
local is_big = count > 5  -- Returns a plain boolean, NOT a derived signal
print(is_big.val)  -- ERROR: is_big is a boolean, not a signal
```

**Instead, use `rover.derive()`:**

```lua
local count = rover.signal(10)

local is_big = rover.derive(function()
    return count.val > 5
end)

print(is_big.val)  -- true

count.val = 3
print(is_big.val)  -- false (automatically updates!)
```

### Why This Limitation Exists

Lua requires comparison metamethods (`__lt`, `__le`, `__eq`) to return boolean values. This is a language-level constraint that cannot be worked around while maintaining Lua compatibility.

The same applies to logical operators (`and`, `or`, `not`) - they evaluate to one of their operands rather than calling metamethods, so they cannot create derived signals automatically.

## Performance Characteristics

Rover's signal system is designed for high performance:

- **Zero-allocation reads**: Reading a signal value doesn't allocate memory
- **Lazy evaluation**: Derived signals only recompute when accessed and dependencies changed
- **Automatic memoization**: Derived values are cached until dependencies change
- **Efficient dependency tracking**: Only changed signals trigger updates
- **Batch updates**: Multiple signal changes are batched together for efficiency

## Advanced Patterns

### Conditional Derivation

```lua
local show_name = rover.signal(true)
local first_name = rover.signal("John")
local last_name = rover.signal("Doe")

local display_name = rover.derive(function()
    if show_name.val then
        return first_name.val .. " " .. last_name.val
    else
        return "Anonymous"
    end
end)

print(display_name.val)  -- "John Doe"

show_name.val = false
print(display_name.val)  -- "Anonymous"
```

### Chaining Derived Signals

```lua
local price = rover.signal(100)
local quantity = rover.signal(3)
local tax_rate = rover.signal(0.1)

local subtotal = rover.derive(function()
    return price.val * quantity.val
end)

local tax = rover.derive(function()
    return subtotal.val * tax_rate.val
end)

local total = rover.derive(function()
    return subtotal.val + tax.val
end)

print(total.val)  -- 330

quantity.val = 5
print(total.val)  -- 550 (all derived signals update automatically)
```

### Effect with Multiple Dependencies

```lua
local first_name = rover.signal("John")
local last_name = rover.signal("Doe")

rover.effect(function()
    -- Automatically tracks both signals
    print("Full name:", first_name.val, last_name.val)
end)
-- Prints: "Full name: John Doe"

first_name.val = "Jane"
-- Prints: "Full name: Jane Doe"

last_name.val = "Smith"
-- Prints: "Full name: Jane Smith"
```

## Best Practices

1. **Use signals for reactive state**: Keep UI state in signals for automatic updates
2. **Prefer derived signals over manual updates**: Let the system handle recomputation
3. **Use `rover.derive()` for comparisons**: Don't rely on comparison operators
4. **Keep effects pure**: Avoid side effects in derived signal computations
5. **Return cleanup functions from effects**: Always clean up resources like timers or event listeners
6. **Use utility functions**: `rover.any()` and `rover.all()` are optimized for common patterns

## Architecture Notes

### Signal Arena

Signals are stored in an arena allocator for efficient memory management and recycling:

- Signals that are no longer used can be recycled
- Version tracking prevents stale reads
- Constant-time access by ID

### Subscriber Graph

Dependencies are tracked in a directed graph:

- Signals know which derived signals/effects depend on them
- Updates propagate through the dependency graph
- Cycles are prevented by design (derived signals are lazy)

### Lazy Evaluation

Derived signals use lazy evaluation with caching:

- Only recompute when accessed after dependencies changed
- Results are cached until next dependency change
- Avoids unnecessary computation for unused derived signals

## See Also

- [Effects and Reactivity](#) (coming soon)
- [UI Components with Signals](#) (coming soon)
- [Performance Optimization](#) (coming soon)
