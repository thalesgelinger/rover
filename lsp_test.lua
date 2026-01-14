-- Test file for LSP type inference features
-- This file tests all type inference and narrowing capabilities

-- ==========================================
-- 1. Basic Type Inference
-- ==========================================

local number_val = 42
local string_val = "hello"
local bool_val = true
local nil_val = nil

-- ==========================================
-- 2. Structural Typing
-- ==========================================

local person = {
    name = "Alice",
    age = 30,
    email = "alice@example.com"
}

-- Access properties - should infer types
local name = person.name
local age = person.age

-- Update properties
person.city = "New York"

-- ==========================================
-- 3. Function Definitions & Return Types
-- ==========================================

function get_string()
    return "hello world"
end

function get_number()
    return 42
end

function add(a, b)
    return a + b
end

function get_person()
    return {
        id = 1,
        name = "Bob",
        active = true
    }
end

-- ==========================================
-- 4. Assert-based Parameter Typing
-- ==========================================

function process_string(data)
    assert(type(data) == "string")
    -- Here data is narrowed to string
    local len = string.len(data)
    return len
end

function process_table(item)
    assert(type(item) == "table")
    assert(item.name ~= nil)
    -- Here item.name is string, item is table
    return item.name
end

-- ==========================================
-- 5. Type Narrowing in Control Flow
-- ==========================================

local maybe_string = "hello"

if type(maybe_string) == "string" then
    -- Narrowed to string inside this branch
    local upper = string.upper(maybe_string)
end

local maybe_nil = nil

if maybe_nil ~= nil then
    -- Narrowed to exclude nil
    local value = maybe_string .. "!"
end

local truthy_value = "test"

if truthy_value then
    -- Narrowed to truthy type (not nil, not false)
    local processed = truthy_value
end

if not truthy_value then
    -- Narrowed to falsy (nil or false)
    local is_empty = false
end

-- ==========================================
-- 6. pcall/xpcall Narrowing
-- ==========================================

local success = function()
    return "operation completed"
end

local ok, result = pcall(success)

if ok then
    -- result is narrowed to string (success return type)
    local output = result
else
    -- result is narrowed to string (error type)
    local error_msg = result
end

function get_error()
    error("something went wrong")
end

local ok2, result2 = pcall(get_error)

if ok2 then
    -- Should never reach here
    local success_val = result2
else
    -- result2 is error string
    local err = result2
end

-- ==========================================
-- 7. Binary & String Operations
-- ==========================================

local a = 10
local b = 20
local sum = a + b
local product = a * b
local comparison = a < b

local str1 = "hello"
local str2 = "world"
local concatenated = str1 .. " " .. str2

-- ==========================================
-- 8. Table Constructor Typing
-- ==========================================

local config = {
    enabled = true,
    port = 8080,
    host = "localhost",
    debug = false
}

-- Array-like table
local items = {1, 2, 3, 4, 5}
local first = items[1]

-- ==========================================
-- 9. Method Calls
-- ==========================================

local text = "hello world"
local sub = string.sub(text, 1, 5)
local upper = string.upper(text)
local lower = string.lower(text)

-- ==========================================
-- 10. Complex Nested Types
-- ==========================================

local user = {
    profile = {
        name = "Charlie",
        age = 25
    },
    settings = {
        notifications = true,
        theme = "dark"
    }
}

local profile = user.profile
local theme = user.settings.theme

-- ==========================================
-- 11. Multiple Returns
-- ==========================================

function get_coords()
    return 10, 20
end

local x, y = get_coords()

-- ==========================================
-- 12. Varargs
-- ==========================================

function sum_all(...)
    local total = 0
    local args = {...}
    for i, v in ipairs(args) do
        total = total + v
    end
    return total
end

local total = sum_all(1, 2, 3, 4, 5)

-- ==========================================
-- 13. Boolean Logic
-- ==========================================

local flag1 = true
local flag2 = false

if flag1 and flag2 then
    -- This block won't execute
end

if flag1 or flag2 then
    -- This will execute
end

-- ==========================================
-- 14. Table Methods
-- ==========================================

local numbers = {5, 2, 8, 1, 9}
table.sort(numbers)

local fruits = {"apple", "banana", "cherry"}
table.insert(fruits, "date")

-- ==========================================
-- 15. Math Operations
-- ==========================================

local num = 5.5
local floor = math.floor(num)
local ceil = math.ceil(num)
local abs = math.abs(-10)

-- ==========================================
-- 16. Type Checking
-- ==========================================

function check_type(value)
    local t = type(value)
    if t == "string" then
        return "is string"
    elseif t == "number" then
        return "is number"
    elseif t == "table" then
        return "is table"
    elseif t == "boolean" then
        return "is boolean"
    else
        return "is unknown"
    end
end

local type_result = check_type("test")

-- ==========================================
-- 17. Error Handling with xpcall
-- ==========================================

local handler = function(err)
    return "handled: " .. err
end

local ok3, result3 = xpcall(function()
    return "success"
end, handler)

if ok3 then
    local val = result3
end

-- ==========================================
-- 18. Nil Handling Patterns
-- ==========================================

local optional = "value"

if optional ~= nil then
    local has_value = optional
else
    local no_value = optional
end

-- ==========================================
-- 19. Property Assignment
-- ==========================================

local config = {}

config.timeout = 30
config.max_retries = 3
config.url = "https://api.example.com"

-- ==========================================
-- 20. Nested Function Calls
-- ==========================================

function get_data()
    return {data = "important", count = 10}
end

function process_data()
    local d = get_data()
    return d.data
end

local processed = process_data()

-- ==========================================
-- 21. Standard Library Functions
-- ==========================================

-- IO functions
local file, err = io.open("test.txt", "r")
if file ~= nil then
    -- file is a table (file handle)
end

-- OS functions
local timestamp = os.time()
local date_str = os.date("%Y-%m-%d")

-- Pairs/ipairs
local settings = {key1 = "val1", key2 = "val2"}
for k, v in pairs(settings) do
    -- k and v are inferred
end

local array = {10, 20, 30}
for i, v in ipairs(array) do
    -- i is number, v is number
end

-- ==========================================
-- 22. require() Cross-File Type Flow
-- ==========================================

-- Note: This tests require() type inference
-- In a real LSP, the module would be loaded and typed

local test_module = require("test_module")

-- Access exported function
local greeting = test_module.greet("World")

-- Access exported variable
local module_version = test_module.version

-- Access exported function from table
local config = test_module.get_config()
local is_enabled = config.enabled

print("Test file loaded successfully")
