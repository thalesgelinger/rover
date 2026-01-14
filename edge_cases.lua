-- Edge case tests for LSP type inference
-- These test cases are designed to be validated with rover check

-- ==========================================
-- 1. Type Mismatch in Binary Operations
-- ==========================================

function add_number_and_string()
    local num = 10
    local str = "hello"
    -- ERROR: Cannot add number and string
    return num + str
end

-- ==========================================
-- 2. Function Call with Wrong Types
-- ==========================================

function process_string(text)
    return text:upper()
end

function wrong_call()
    local num = 42
    -- ERROR: process_string expects string, got number
    return process_string(num)
end

-- ==========================================
-- 3. Assert-Based Parameter Constraints
-- ==========================================

-- Hovering over bar should show: bar: number (from assert)
-- Hovering over baz should show: baz: string (from assert)
function constrained_params(bar, baz)
    assert(type(bar) == "number")
    assert(type(baz) == "string")

    -- ERROR: baz is constrained to string, can't use in string.len
    -- Actually this is OK - baz IS a string
    return string.len(baz)
end

function wrong_after_assert(data)
    assert(type(data) == "table")
    -- ERROR: data is constrained to table, can't use as number
    return data + 10
end

-- ==========================================
-- 4. Return Type Mismatches
-- ==========================================

function inconsistent_returns(flag)
    if flag then
        return "string"
    else
        -- ERROR: Inconsistent return types (string vs number)
        return 42
    end
end

-- ==========================================
-- 5. Nested Narrowing Edge Cases
-- ==========================================

function nested_narrowing(value)
    if type(value) == "string" then
        if value ~= "" then
            -- value is narrowed to string (not empty)
            local len = string.len(value)
            return len
        end
    end
    return 0
end

-- ==========================================
-- 6. Multiple Return Values
-- ==========================================

function multiple_returns()
    return "first", "second", 42
end

-- Using only first return
local first = multiple_returns()

-- Using all returns (needs unpack)
local a, b, c = multiple_returns()

-- ==========================================
-- 7. Table Field Access Errors
-- ==========================================

function access_nonexistent_field()
    local obj = {name = "Alice", age = 30}
    -- ERROR: field 'email' doesn't exist
    return obj.email
end

-- ==========================================
-- 8. Wrong Type Assignment to Fields
-- ==========================================

function wrong_field_type()
    local obj = {name = "Bob"}
    -- ERROR: name is string, assigning number
    obj.name = 42
    return obj
end

-- ==========================================
-- 9. Method Call on Non-Object
-- ==========================================

function invalid_method_call()
    local num = 42
    -- ERROR: calling :upper() on number
    return num:upper()
end

-- ==========================================
-- 10. Variable Shadowing
-- ==========================================

local x = "global"

function shadow_test()
    local x = "local"
    -- x is "local", not "global"
    return x
end

-- ==========================================
-- 11. pcall with Non-Function
-- ==========================================

function pcall_invalid()
    -- ERROR: pcall first arg must be function
    local ok, result = pcall("not a function")
    return result
end

-- ==========================================
-- 12. xpcall with Wrong Handler Type
-- ==========================================

function xpcall_wrong_handler()
    local handler = "not a function"
    -- ERROR: xpcall handler must be function
    local ok, result = xpcall(function()
        return "success"
    end, handler)
    return result
end

-- ==========================================
-- 13. Comparison Operators
-- ==========================================

function compare_types()
    local str = "hello"
    local num = 10
    -- ERROR: Cannot compare string and number with <
    return str < num
end

-- ==========================================
-- 14. Nil Handling in Optional Values
-- ==========================================

function optional_params(required, optional)
    -- Hovering should show: required: any, optional: any | nil
    if optional ~= nil then
        return required + optional
    end
    return required
end

-- ==========================================
-- 15. Empty Table Edge Cases
-- ==========================================

function empty_table_operations()
    local empty = {}
    -- Accessing non-existent field in empty table
    -- ERROR or should be nil/unknown
    return empty.field
end

-- ==========================================
-- 16. Array Index Out of Bounds
-- ==========================================

function array_index_error()
    local arr = {1, 2, 3}
    -- ERROR: arr only has 3 elements, index 4 out of bounds
    -- (Lua allows this but LSP should warn/show type issue)
    return arr[4]
end

-- ==========================================
-- 17. String Indexing
-- ==========================================

function string_indexing()
    local text = "hello"
    -- OK: returns 'h'
    local first = text[1]
    -- OK: returns 'e'
    local fifth = text[5]
    -- ERROR: string only has 5 chars, index 6 out of bounds
    local sixth = text[6]
    return first
end

-- ==========================================
-- 18. Truthy/Falsy Narrowing Edge Cases
-- ==========================================

function truthy_edge_cases(value)
    -- After this check, value is narrowed to truthy (not nil, not false)
    if value then
        -- value could be: number, string, table, function, userdata, thread
        -- Hovering should show type union of truthy types
        local is_string = type(value) == "string"
        return is_string
    end
    return false
end

-- ==========================================
-- 19. Function as Value (Higher-Order)
-- ==========================================

function apply(func, value)
    -- Hovering over func: function(any) -> any
    return func(value)
end

function double(x)
    return x * 2
end

-- ERROR/WARNING: double expects number, passing string
local result = apply(double, "not a number")

-- ==========================================
-- 20. Recursive Function Types
-- ==========================================

-- Hovering over factorial: (number) -> number
function factorial(n)
    if n <= 1 then
        return 1
    end
    return n * factorial(n - 1)
end

local fact5 = factorial(5)

-- ==========================================
-- 21. Self-Referential Tables
-- ==========================================

function circular_reference()
    local obj = {}
    obj.parent = obj
    -- ERROR: circular reference - LSP should handle this gracefully
    return obj.parent
end

-- ==========================================
-- 22. Boolean in Arithmetic Context
-- ==========================================

function boolean_arithmetic()
    local flag = true
    -- ERROR: Cannot use boolean in arithmetic
    return flag + 10
end

-- ==========================================
-- 23. Nil in Arithmetic Context
-- ==========================================

function nil_arithmetic()
    local nothing = nil
    -- ERROR: Cannot use nil in arithmetic
    return nothing + 10
end

-- ==========================================
-- 24. Multiple Asserts on Same Parameter
-- ==========================================

function multiple_constraints(param)
    assert(type(param) == "string")
    assert(type(param) == "string")
    assert(string.len(param) > 0)

    -- param is definitely string
    return param:upper()
end

-- ==========================================
-- 25. Chained Method Calls
-- ==========================================

function chained_methods(text)
    -- OK: chaining string methods
    return text:upper():sub(1, 3)
end

function invalid_chain()
    local num = 42
    -- ERROR: chaining on non-object
    return num:upper():sub(1, 3)
end

-- ==========================================
-- 26. Table Constructor with Mixed Types
-- ==========================================

function mixed_table()
    -- ERROR or WARNING: array elements should have consistent type
    local mixed = {1, "two", 3, "four"}
    return mixed
end

-- ==========================================
-- 27. Function with No Returns
-- ==========================================

function no_return_value(x)
    local y = x * 2
    -- No return statement - returns nil
    -- Hovering should show: (number) -> nil
end

local result = no_return_value(10)

-- ==========================================
-- 28. Upvalues (Closures)
-- ==========================================

function create_counter()
    local count = 0
    return function()
        count = count + 1
        return count
    end
end

local counter = create_counter()
local first = counter()
local second = counter()
-- Hovering over counter: () -> number (captures count)

-- ==========================================
-- 29. Type Coercion in Comparisons
-- ==========================================

function implicit_coercion()
    local num = 10
    local str = "10"
    -- Lua allows this (coerces to number)
    -- LSP should warn or show as comparison of any types
    return num == str
end

-- ==========================================
-- 30. Wrong Number of Arguments
-- ==========================================

function two_params(a, b)
    return a + b
end

-- ERROR: two_params expects 2 args, got 1
local too_few = two_params(10)

-- ERROR: two_params expects 2 args, got 3
local too_many = two_params(10, 20, 30)

-- ==========================================
-- 31. Module Return Value
-- ==========================================

-- This should be in test_module.lua
-- Hovering over module return should show table type

-- ==========================================
-- 32. pcall Result in Else Branch
-- ==========================================

function pcall_else_test()
    local ok, result = pcall(function()
        return "success value"
    end)

    if ok then
        -- result is "success value" (string)
        local success_value = result
        return success_value
    else
        -- result is error message (string)
        local error_msg = result
        return error_msg
    end
end

-- ==========================================
-- 33. require with Variable Argument
-- ==========================================

function dynamic_require(module_name)
    -- ERROR: require expects string literal for static analysis
    -- Hovering over module_name should show it's a string
    local m = require(module_name)
    return m
end

-- ==========================================
-- 34. Table as Array vs Object
-- ==========================================

function table_ambiguity()
    -- Is this an array or object with numeric keys?
    local t = {1, 2, 3}
    -- Valid: array access
    local first = t[1]
    -- Valid: accessing numeric key (same as array access)
    local one = t[1]
    return first
end

-- ==========================================
-- 35. String Concatenation Edge Cases
-- ==========================================

function concat_mixed()
    local num = 42
    local str = "value: "
    -- OK: coerces num to string
    return str .. num
end

function concat_nil()
    local nothing = nil
    -- ERROR: Cannot concatenate nil
    return "value: " .. nothing
end

-- ==========================================
-- 36. Logical Operator Short-Circuiting
-- ==========================================

function logical_short_circuit()
    local a = nil
    local b = "value"
    -- OK: returns b because a is falsy
    local result = a and b
    -- OK: returns a (truthy)
    local result2 = a or b
    return result
end

-- ==========================================
-- 37. Table Method Calls
-- ==========================================

function table_method_test()
    local list = {1, 2, 3}
    -- OK: table.insert is a method, but Lua uses table.insert(list, val)
    -- In OOP style this would be list:insert(val)
    -- Hovering over list should show: {number}
    return #list
end

-- ==========================================
-- 38. For Loop Iterator Types
-- ==========================================

function loop_iterator_types()
    local obj = {a = 1, b = 2, c = 3}

    for k, v in pairs(obj) do
        -- k: string, v: number
        -- Hovering over k should show: string
        -- Hovering over v should show: number
    end

    local arr = {10, 20, 30}
    for i, v in ipairs(arr) do
        -- i: number, v: number
        -- Hovering over i should show: number
        -- Hovering over v should show: number
    end

    return obj
end

-- ==========================================
-- 39. Function Parameter Defaults (Simulated)
-- ==========================================

function with_optional(param)
    if param == nil then
        param = "default"
    end
    -- param is now definitely string
    return param
end

-- ==========================================
-- 40. Type Guard Pattern
-- ==========================================

function type_guard(value)
    if type(value) == "number" then
        return value * 2
    elseif type(value) == "string" then
        return string.len(value)
    elseif type(value) == "table" then
        return #value
    else
        return 0
    end
end

-- Hovering over value in each branch shows narrowed type

-- ==========================================
-- 41. Error Function Never Returns
-- ==========================================

function never_returns()
    error("This function never returns normally")
    -- Hovering should show: () -> never
end

-- ==========================================
-- 42. Complex Nested Tables
-- ==========================================

function complex_nested()
    local config = {
        server = {
            host = "localhost",
            port = 8080,
            ssl = {
                enabled = true,
                cert = "/path/to/cert.pem"
            }
        },
        database = {
            name = "mydb",
            pool = {
                min = 1,
                max = 10
            }
        }
    }

    -- Access nested fields
    local host = config.server.host
    local cert = config.server.ssl.cert
    local max_pool = config.database.pool.max

    -- ERROR: Accessing non-existent field
    local invalid = config.server.ssl.invalid_field

    return config
end

-- ==========================================
-- 43. Function Composition
-- ==========================================

function compose(f, g)
    return function(x)
        return f(g(x))
    end
end

function add_one(x)
    return x + 1
end

function double(x)
    return x * 2
end

local add_then_double = compose(double, add_one)
-- add_then_double(5) = double(add_one(5)) = double(6) = 12

-- ==========================================
-- 44. Vararg Function Types
-- ==========================================

function sum_all(...)
    local total = 0
    local args = {...}

    for i, v in ipairs(args) do
        total = total + v
    end

    return total
end

-- Hovering over sum_all should show: (number, ...) -> number

-- ==========================================
-- 45. Table Metamethods (Advanced)
-- ==========================================

function with_metamethod()
    local mt = {
        __add = function(a, b)
            return {value = a.value + b.value}
        end
    }

    local num1 = {value = 10, __metatable = mt}
    local num2 = {value = 20, __metatable = mt}

    -- Uses __add metamethod
    local result = num1 + num2

    -- Hovering over num1, num2 should show type info
    return result.value
end

-- ==========================================
-- 46. Module with Conditional Exports
-- ==========================================

-- This pattern tests require() with conditional exports
-- (Would be in a separate module file)

-- ==========================================
-- 47. Multiple Type Constraints on Same Variable
-- ==========================================

function double_constrained(x)
    assert(type(x) == "number")
    assert(x > 0)

    -- x is now: number and > 0
    return x * 2
end

-- ==========================================
-- 48. String Method with Number
-- ==========================================

function string_method_wrong_type()
    local num = 42
    -- ERROR: calling string method on number
    return string.len(num)
end

-- ==========================================
-- 49. Math Library with Wrong Types
-- ==========================================

function math_wrong_type()
    local str = "hello"
    -- ERROR: math functions expect number
    return math.floor(str)
end

-- ==========================================
-- 50. os Library Return Types
-- ==========================================

function time_functions()
    local time_val = os.time()
    -- Hovering over time_val: number

    local date_str = os.date("%Y-%m-%d", time_val)
    -- Hovering over date_str: string

    local env_var = os.getenv("PATH")
    -- Hovering over env_var: string | nil

    return date_str
end

-- ==========================================
-- 51. io Library Type Handling
-- ==========================================

function io_types()
    local file, err = io.open("test.txt", "r")

    if file ~= nil then
        -- file is a table (file handle)
        -- Hovering over file: table with methods: close, read, write, etc.
        local content = file:read("*all")
        file:close()
        return content
    else
        -- err is string | nil
        return nil
    end
end

-- ==========================================
-- 52. Coroutine Types
-- ==========================================

function coroutine_test()
    local co = coroutine.create(function()
        return "coroutine result"
    end)

    -- Hovering over co: thread (coroutine)
    local success, result = coroutine.resume(co)

    if success then
        -- result is the return value from coroutine
        return result
    end

    return nil
end

-- ==========================================
-- 53. Weak Tables
-- ==========================================

function weak_table_test()
    -- Weak tables are valid but have special semantics
    local weak = {__mode = "v"}
    weak[1] = "first"
    weak[2] = "second"

    -- Hovering over weak should still work
    return weak
end

-- ==========================================
-- 54. Function as Table Field
-- ==========================================

local callbacks = {
    on_success = function(data)
        return "processed: " .. data
    end,
    on_error = function(msg)
        return "error: " .. msg
    end
}

-- Hovering over callbacks.on_success: (string) -> string
-- Hovering over callbacks.on_error: (string) -> string

-- ==========================================
-- 55. Pattern Matching with string.match
-- ==========================================

function pattern_matching()
    local text = "hello world 123"

    -- Extract number
    local num_str = string.match(text, "%d+")
    -- Hovering over num_str: string | nil

    -- Extract word
    local word = string.match(text, "%a+")
    -- Hovering over word: string | nil

    return num_str or word
end

print("Edge case test file loaded")
