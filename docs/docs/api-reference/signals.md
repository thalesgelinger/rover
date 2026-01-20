# Signals API Reference

Complete API reference for Rover's signal system.

## Functions

### rover.signal(initial_value)

Creates a new signal with the given initial value.

**Parameters:**
- `initial_value` (any): The initial value of the signal. Can be any Lua value (number, string, boolean, table, nil, etc.)

**Returns:**
- `Signal`: A signal object with a `.val` property

**Example:**
```lua
local count = rover.signal(0)
local name = rover.signal("Alice")
local config = rover.signal({ debug = true })
local empty = rover.signal(nil)
```

---

### rover.derive(compute_fn)

Creates a derived signal that automatically recomputes when its dependencies change.

**Parameters:**
- `compute_fn` (function): A function that computes the derived value. The function is called with no arguments and should return the computed value. It automatically tracks any signals accessed during execution.

**Returns:**
- `DerivedSignal`: A derived signal object with a `.val` property (read-only)

**Example:**
```lua
local first = rover.signal("John")
local last = rover.signal("Doe")

local full_name = rover.derive(function()
    return first.val .. " " .. last.val
end)

print(full_name.val)  -- "John Doe"
```

**Notes:**
- Derived signals are lazy - they only recompute when accessed after dependencies changed
- Results are automatically cached until dependencies change
- Derived signals are read-only (no `.val = ...` assignment)

---

### rover.effect(effect_fn)

Creates an effect that automatically re-runs when its dependencies change.

**Parameters:**
- `effect_fn` (function): A function that contains side effects. The function is called with no arguments. It can optionally return a cleanup function.

**Returns:**
- `nil` (currently; may return a disposer function in future versions)

**Example:**
```lua
local count = rover.signal(0)

rover.effect(function()
    print("Count changed to:", count.val)
end)
-- Immediately prints: "Count changed to: 0"

count.val = 5
-- Prints: "Count changed to: 5"
```

**With Cleanup:**
```lua
rover.effect(function()
    local timer_id = start_timer(count.val)

    -- Return cleanup function
    return function()
        stop_timer(timer_id)
    end
end)
```

**Notes:**
- Effects run immediately upon creation
- Cleanup functions (if provided) run before the next effect execution
- Effects automatically track signal dependencies during execution

---

### rover.any(...)

Creates a derived signal that returns `true` if any of the input signals are truthy.

**Parameters:**
- `...` (Signal|DerivedSignal|any): Any number of signals or values to check

**Returns:**
- `DerivedSignal`: A derived signal that's `true` if any input is truthy, `false` otherwise

**Example:**
```lua
local a = rover.signal(false)
local b = rover.signal(true)
local c = rover.signal(false)

local any_true = rover.any(a, b, c)
print(any_true.val)  -- true

b.val = false
print(any_true.val)  -- false
```

---

### rover.all(...)

Creates a derived signal that returns `true` if all of the input signals are truthy.

**Parameters:**
- `...` (Signal|DerivedSignal|any): Any number of signals or values to check

**Returns:**
- `DerivedSignal`: A derived signal that's `true` if all inputs are truthy, `false` otherwise

**Example:**
```lua
local a = rover.signal(true)
local b = rover.signal(true)
local c = rover.signal(false)

local all_true = rover.all(a, b, c)
print(all_true.val)  -- false

c.val = true
print(all_true.val)  -- true
```

---

## Signal Object

### Properties

#### signal.val

**Type:** `any` (read/write for Signals, read-only for DerivedSignals)

Get or set the signal's value.

**Example:**
```lua
local count = rover.signal(5)

-- Read
print(count.val)  -- 5

-- Write
count.val = 10
print(count.val)  -- 10
```

**Notes:**
- Reading `.val` automatically tracks the dependency in effects and derived signals
- Writing to a derived signal's `.val` will cause an error

---

### Supported Operators (Metamethods)

Signals and derived signals support the following operators, which automatically create new derived signals:

#### Arithmetic Operators

- `+` (addition): `signal + value` or `value + signal`
- `-` (subtraction): `signal - value` or `value - signal`
- `*` (multiplication): `signal * value` or `value * signal`
- `/` (division): `signal / value` or `value / signal`
- `%` (modulo): `signal % value` or `value % signal`
- `^` (power): `signal ^ value` or `value ^ signal`
- `-` (unary negation): `-signal`

**Example:**
```lua
local x = rover.signal(10)

local double = x * 2
local sum = x + 5
local negative = -x

print(double.val)    -- 20
print(sum.val)       -- 15
print(negative.val)  -- -10
```

#### String Concatenation

- `..` (concatenation): `signal .. value` or `value .. signal`

**Example:**
```lua
local count = rover.signal(42)

local label = "Count: " .. count
local suffix = count .. " items"

print(label.val)   -- "Count: 42"
print(suffix.val)  -- "42 items"
```

---

## Type Compatibility

Signals can store any Lua value:

| Type | Supported | Example |
|------|-----------|---------|
| `number` | ✅ | `rover.signal(42)` |
| `string` | ✅ | `rover.signal("hello")` |
| `boolean` | ✅ | `rover.signal(true)` |
| `nil` | ✅ | `rover.signal(nil)` |
| `table` | ✅ | `rover.signal({ x = 10 })` |
| `function` | ✅ | `rover.signal(function() end)` |
| `userdata` | ✅ | `rover.signal(some_userdata)` |

**Note:** Tables stored in signals are not deeply reactive. Changing a table's contents won't trigger updates:

```lua
local config = rover.signal({ debug = false })

-- This will NOT trigger effects:
config.val.debug = true

-- Instead, replace the entire table:
config.val = { debug = true }  -- This WILL trigger effects
```

---

## Comparison Operators (Limitation)

**Important:** Comparison operators cannot create derived signals due to Lua's semantics.

### ❌ This Does NOT Work:

```lua
local count = rover.signal(10)
local is_big = count > 5  -- Returns boolean, not a signal!
```

### ✅ Use rover.derive() Instead:

```lua
local count = rover.signal(10)

local is_big = rover.derive(function()
    return count.val > 5
end)

print(is_big.val)  -- true (reactive!)
```

**Affected operators:**
- `<` (less than)
- `>` (greater than)
- `<=` (less than or equal)
- `>=` (greater than or equal)
- `==` (equal)
- `~=` (not equal)

---

## Performance Characteristics

### Memory

- **Signal creation**: O(1) with arena allocation
- **Signal read**: Zero allocations
- **Signal write**: O(1)
- **Derived signal creation**: O(1)
- **Effect creation**: O(1)

### Computation

- **Dependency tracking**: O(1) per signal read
- **Update propagation**: O(D) where D is the number of direct dependents
- **Derived signal evaluation**: Lazy, only when `.val` is accessed

### Optimization Features

1. **Automatic memoization**: Derived signals cache results
2. **Lazy evaluation**: Derived signals only compute when needed
3. **Batch updates**: Multiple signal changes are batched
4. **Version tracking**: Prevents unnecessary recomputations
5. **Arena allocation**: Efficient memory reuse

---

## Error Handling

### Invalid Operations

**Writing to derived signals:**
```lua
local x = rover.signal(5)
local double = x * 2

double.val = 20  -- ERROR: Cannot write to derived signal
```

**Accessing `.val` on non-signals:**
```lua
local not_a_signal = 42
print(not_a_signal.val)  -- ERROR: attempt to index a number value
```

### Runtime Errors

Errors in derived signal computations propagate:

```lua
local x = rover.signal(0)

local divide = rover.derive(function()
    return 10 / x.val  -- Division by zero when x.val = 0
end)

-- This will error:
print(divide.val)  -- ERROR: division by zero
```

Errors in effects are caught and reported:

```lua
rover.effect(function()
    error("Something went wrong")
end)
-- Error is logged, but doesn't crash the program
```

---

## Implementation Details

### Architecture

The signal system consists of:

1. **SignalArena**: Manages signal storage and lifecycle
   - Stores signal values and versions
   - Recycles disposed signal IDs
   - O(1) access by SignalId

2. **SubscriberGraph**: Tracks dependencies
   - Maps signals → subscribers (derived signals/effects)
   - Handles subscription and unsubscription
   - Propagates updates through the graph

3. **DerivedSignals**: Lazy computed values
   - Store compute function in Lua registry
   - Cache computed values until dependencies change
   - Track version to detect stale values

4. **Effects**: Side effect runners
   - Stored with their Lua functions
   - Optional cleanup function support
   - Automatically re-run on dependency changes

### Version Tracking

Each signal has a version number that increments on write:
- Derived signals track the version they last computed with
- On access, if signal versions changed, recompute
- Prevents unnecessary recomputation when values don't change

### Dependency Tracking

During derived signal or effect execution:
- Reads are tracked in a stack-based tracker
- At the end, all read signals are subscribed
- On subsequent runs, subscriptions are updated

---

## See Also

- [Signals Guide](../guides/signals.md) - User guide with examples and patterns
- [Performance Guide](../performance.md) - Performance tuning and benchmarks
- [UI Components](#) - Using signals in UI components (coming soon)
