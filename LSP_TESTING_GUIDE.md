# LSP Edge Case Testing Guide

This directory contains test files for validating Rover LSP type inference features.

## Test Files

### `lsp_test.lua`
Basic and intermediate type inference tests covering:
- Basic type inference (number, string, boolean, nil)
- Structural typing (table fields, property access)
- Function definitions and return types
- Assert-based parameter typing
- Type narrowing in control flow
- pcall/xpcall narrowing
- Binary and string operations
- Table constructor typing
- Method calls (string library)
- Complex nested types
- Multiple return values
- Varargs
- Boolean logic
- Table methods (table.*)
- Math operations
- Type checking with `type()`
- Error handling with xpcall
- Nil handling patterns
- Property assignment
- Nested function calls
- Standard library functions (io, os, pairs, ipairs)
- require() cross-file type flow

**Usage:**
```bash
# Test basic LSP features
rover check lsp_test.lua

# Or in an LSP client (VS Code, etc.)
# Open lsp_test.lua and hover over variables, functions, etc.
```

### `test_module.lua`
Module to test `require()` type inference:
- Exports `greet(name)` -> string
- Exports `add(a, b)` -> number
- Exports `get_config()` -> table
- Exports `version` variable (string)

**Expected behavior when requiring:**
```lua
local m = require("test_module")

-- Hovering over `m` should show: table
--   .greet: (string) -> string
--   .add: (number, number) -> number
--   .get_config: () -> table
--   .version: string
```

### `edge_cases.lua`
Comprehensive edge case tests covering 55 scenarios:

1. **Type Mismatch in Binary Operations** - `number + string` (should error)
2. **Function Call with Wrong Types** - Passing number to string function (should error)
3. **Assert-Based Parameter Constraints** - Using constrained types correctly
4. **Return Type Mismatches** - Inconsistent returns across branches (should error)
5. **Nested Narrowing Edge Cases** - Nested if statements with narrowing
6. **Multiple Return Values** - Functions returning multiple values
7. **Table Field Access Errors** - Accessing non-existent fields
8. **Wrong Type Assignment to Fields** - Assigning number to string field (should error)
9. **Method Call on Non-Object** - Calling `:upper()` on number (should error)
10. **Variable Shadowing** - Local shadowing outer scope
11. **pcall with Non-Function** - pcall first arg not a function (should error)
12. **xpcall with Wrong Handler Type** - Handler not a function (should error)
13. **Comparison Operators** - Comparing incompatible types (should error)
14. **Nil Handling in Optional Values** - `~= nil` narrowing
15. **Empty Table Edge Cases** - Accessing fields in empty table
16. **Array Index Out of Bounds** - Indexing beyond array size
17. **String Indexing** - String character access
18. **Truthy/Falsy Narrowing Edge Cases** - Complex truthy logic
19. **Function as Value (Higher-Order)** - Functions as parameters
20. **Recursive Function Types** - Factorial function
21. **Self-Referential Tables** - Circular references
22. **Boolean in Arithmetic Context** - `boolean + number` (should error)
23. **Nil in Arithmetic Context** - `nil + number` (should error)
24. **Multiple Asserts on Same Parameter** - Multiple type constraints
25. **Chained Method Calls** - `str:upper():sub()`
26. **Table Constructor with Mixed Types** - Mixed array elements
27. **Function with No Returns** - Implicit nil return
28. **Upvalues (Closures)** - Closure captured variables
29. **Type Coercion in Comparisons** - `number == "10"` (string coercion)
30. **Wrong Number of Arguments** - Too few/many arguments
31. **Module Return Value** - Hover over require() result
32. **pcall Result in Else Branch** - Error type narrowing
33. **require with Variable Argument** - Dynamic require (warning)
34. **Table as Array vs Object** - Numeric key access
35. **String Concatenation Edge Cases** - Concatenating nil
36. **Logical Operator Short-Circuiting** - `and`/`or` semantics
37. **Table Method Calls** - Using table methods properly
38. **For Loop Iterator Types** - `pairs()` vs `ipairs()` types
39. **Function Parameter Defaults** - Optional parameters
40. **Type Guard Pattern** - Complex type checking
41. **Error Function Never Returns** - `error()` function type
42. **Complex Nested Tables** - Deeply nested structures
43. **Function Composition** - Higher-order functions
44. **Vararg Function Types** - `...` parameter typing
45. **Table Metamethods (Advanced)** - `__add`, etc.
46. **Module with Conditional Exports** - Dynamic exports
47. **Multiple Type Constraints on Same Variable** - Intersection types
48. **String Method with Number** - Wrong method call
49. **Math Library with Wrong Types** - Math on strings
50. **os Library Return Types** - OS function types
51. **io Library Type Handling** - File handle types
52. **Coroutine Types** - Coroutine/thread typing
53. **Weak Tables** - Weak reference tables
54. **Function as Table Field** - Callback patterns
55. **Pattern Matching with string.match** - Regex pattern returns

**Usage:**
```bash
# Test all edge cases
rover check edge_cases.lua

# Use with LSP for interactive testing
# - Open edge_cases.lua in VS Code
# - Hover over variables to see inferred types
# - Check for error diagnostics (red squigglies)
# - Navigate to function definitions to see signatures
```

## Expected LSP Behaviors

### Hover Information
- **Variables**: Show inferred type (e.g., `string`, `number`, `{name: string, age: number}`)
- **Functions**: Show signature `(params) -> return_type`
- **Parameters**: Show constrained type from asserts (e.g., `bar: number [from assert]`)
- **Module exports**: Show table type with exported fields

### Diagnostics (Errors)
Type mismatches should show as errors:
- Cannot add `number + string`
- Function `process_string` expects `string`, got `number`
- Field `email` does not exist on type `{name: string, age: number}`
- Inconsistent return types: `string` vs `number`

### Type Narrowing
Control flow should show narrowed types:
```lua
local x: string | nil
if type(x) == "string" then
    -- x is narrowed to string here
    local len = string.len(x)  -- Should work
end
```

### pcall/xpcall Handling
```lua
local ok, result = pcall(some_func)
if ok then
    -- result: narrowed to func return type
else
    -- result: narrowed to string (error type)
end
```

### require() Cross-File Flow
```lua
-- In test_module.lua:
--   return {greet = function(name) return "Hello, " .. name end}

-- In main file:
local m = require("test_module")
-- m: table with .greet: (string) -> string
```

## Testing Checklist

Use this checklist to verify LSP features:

- [ ] Basic types (number, string, boolean, nil, table, function)
- [ ] Literal inference (42 -> number, "hello" -> string)
- [ ] Table constructor typing ({a=1, b=2} -> {a: number, b: number})
- [ ] Property access (obj.field returns field type)
- [ ] Function signatures (hover shows params and return)
- [ ] Assert-based constraints (after `assert(type(x) == "type")`, x is narrowed)
- [ ] If/else narrowing (type narrows in branches)
- [ ] Truthy/falsy narrowing (`if x then` narrows x to truthy)
- [ ] Nil narrowing (`x ~= nil` excludes nil)
- [ ] pcall/xpcall result narrowing (if ok then result is success type)
- [ ] Multiple return values (`local a, b = func()`)
- [ ] Varargs functions (`function(...) ... end`)
- [ ] Standard library types (string.*, math.*, table.*, io.*, os.*)
- [ ] Method call typing (`string:upper()` returns string)
- [ ] Table methods (`table.insert`, `table.sort`, etc.)
- [ ] require() module typing (module exports as table)
- [ ] Error diagnostics (type mismatches, wrong field access)
- [ ] Nested structures (tables in tables, functions in tables)
- [ ] Recursive functions (self-referential types)
- [ ] Closures (upvalue typing)
- [ ] Metamethods (custom operator overloading)

## Known Limitations

Current implementation may have these limitations:
1. **Dynamic require** - `require(variable)` returns `Any` (can't analyze at compile time)
2. **Runtime type coercion** - Lua's implicit number<->string coercion may show warnings
3. **Complex union types** - May fall back to `Any` for very complex unions
4. **Metatable chains** - Deep metatable hierarchies may not be fully typed
5. **Weak tables** - Weak reference semantics not fully modeled
6. **Coroutines** - Coroutine types are simplified
7. **Module caching** - Cache not yet persisted across LSP sessions

## Advanced Testing Scenarios

### Stress Test
```lua
-- Deeply nested structures
local deep = {
    level1 = {
        level2 = {
            level3 = {
                value = 42
            }
        }
    }
}
```

### Performance Test
```lua
-- Large array
local huge = {}
for i = 1, 10000 do
    huge[i] = i
end

-- Hover should be instant
```

### Error Recovery
```lua
-- LSP should continue working after errors
function bad()
    return 42 + "string"  -- Error here
end

function good()
    return "hello"  -- Should still type correctly
end
```

## Integration with Rover CLI

```bash
# Check syntax and types
rover check lsp_test.lua

# Check all edge cases
rover check edge_cases.lua

# Format (if formatter is enabled)
rover fmt edge_cases.lua

# Check with LSP server running
# 1. Start LSP: rover lsp
# 2. Open file in VS Code (or other editor)
# 3. Hover over symbols for type info
# 4. Check diagnostics panel for errors
```

## Bug Reporting

If you find unexpected behavior, report:
1. The function/line number
2. Expected type vs actual type
3. Whether it's a false positive or false negative
4. Screenshot of LSP hover/diagnostic (if possible)

Example issue:
```
File: edge_cases.lua, line 5
Function: add_number_and_string()
Expected: Error (cannot add number + string)
Actual: No error, type inferred as unknown
```
