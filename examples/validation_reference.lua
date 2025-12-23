-- EXHAUSTIVE VALIDATION REFERENCE
-- This file demonstrates all validation patterns, edge cases, and configurations
local api = rover.server {
    port = 3000,
    log_level = "info"  -- Available: "debug", "info", "warn", "error", "nope"
}

local g = rover.guard

-- ============================================================================
-- BASIC TYPES AND REQUIRED VALIDATION
-- ============================================================================

-- String, Integer, Boolean, Number types with required validation
function api.basic_types.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Name is required"),
            age = g:integer():required("Age must be an integer"),
            active = g:boolean():required("Active must be boolean"),
            price = g:number():required("Price must be a number")
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- ENUM VALIDATION - Restrict values to specific set
-- ============================================================================

function api.enum_validation.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            status = g:string():enum({"active", "inactive", "pending"}):required(),
            role = g:string():enum({"admin", "user", "guest"}):required()
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- DEFAULT VALUES - Provide fallback when field is missing
-- ============================================================================

function api.defaults.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            theme = g:string():default("light"),
            notifications = g:boolean():default(true),
            maxItems = g:integer():default(10),
            apiTimeout = g:number():default(30.5)
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- OPTIONAL FIELDS - Fields that don't need to be provided
-- ============================================================================

function api.optional_fields.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            email = g:string():required(),
            phone = g:string(),           -- Optional: no required()
            bio = g:string(),             -- Optional
            website = g:string()          -- Optional
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- ARRAYS - Validate arrays of values
-- ============================================================================

function api.arrays.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            -- Array of strings
            tags = g:array(g:string()):required(),
            -- Array of integers
            scores = g:array(g:integer()),
            -- Array of numbers
            prices = g:array(g:number()),
            -- Array of booleans
            features = g:array(g:boolean())
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- NESTED OBJECTS - Validate complex nested structures
-- ============================================================================

function api.nested_objects.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            user = g:object({
                name = g:string():required(),
                email = g:string():required(),
                profile = g:object({
                    bio = g:string(),
                    avatar = g:string(),
                    social = g:object({
                        twitter = g:string(),
                        github = g:string()
                    })
                })
            })
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- ARRAYS OF OBJECTS - Mix arrays with nested validation
-- ============================================================================

function api.array_of_objects.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            items = g:array(g:object({
                id = g:integer():required(),
                name = g:string():required(),
                active = g:boolean()
            })):required()
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- COMPLEX NESTED WITH ALL FEATURES
-- ============================================================================

function api.complex.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            -- User info
            user = g:object({
                name = g:string():required(),
                email = g:string():required(),
                role = g:string():enum({"admin", "user"}):default("user"),
                age = g:integer()
            }),
            -- Multiple items
            items = g:array(g:object({
                productId = g:integer():required(),
                quantity = g:integer():required(),
                metadata = g:object({
                    color = g:string(),
                    size = g:string()
                })
            })),
            -- Configuration
            config = g:object({
                theme = g:string():default("light"),
                notifications = g:boolean():default(true),
                preferences = g:array(g:string())
            }),
            -- Basic fields
            totalAmount = g:number():required(),
            currency = g:string():enum({"USD", "EUR", "BRL"}):default("USD"),
            notes = g:string()
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- DIRECT GUARD USAGE (without body parsing)
-- ============================================================================

function api.direct_guard_validation.get(ctx)
    local success, result = pcall(function()
        local data = {
            name = "John",
            age = 30,
            status = "active"
        }

        return rover.guard(data, {
            name = g:string():required(),
            age = g:integer(),
            status = g:string():enum({"active", "inactive"})
        })
    end)

    if not success then
        return api:error(500, result)
    end

    return api.json { success = true, data = result }
end

-- ============================================================================
-- ERROR HANDLING WITH PCALL
-- ============================================================================

function api.error_handling_pcall.post(ctx)
    local success, user = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Name cannot be empty"),
            email = g:string():required("Email is mandatory"),
            age = g:integer()
        }
    end)

    if not success then
        return api:error(400, user)
    end

    return api.json { success = true, user = user }
end

-- ============================================================================
-- ERROR HANDLING WITH XPCALL (detailed error tracking)
-- ============================================================================

function api.error_handling_xpcall.post(ctx)
    local success, user = xpcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            email = g:string():required(),
            age = g:integer(),
            tags = g:array(g:string())
        }
    end, function(err)
        -- Clean up error message by removing stack trace
        local err_str = tostring(err):gsub("^runtime error: ", "")
        local stack_pos = err_str:find("\nstack traceback:")
        return stack_pos and err_str:sub(1, stack_pos - 1) or err_str
    end)

    if not success then
        return api:error(400, user)
    end

    return api.json {
        success = true,
        user = user
    }
end

-- ============================================================================
-- EDGE CASES AND COMBINATIONS
-- ============================================================================

-- Required + Enum + Default (note: required takes precedence over default)
function api.required_enum_default.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            status = g:string():enum({"a", "b", "c"}):required():default("a"),  -- default ignored
            optional_status = g:string():enum({"x", "y", "z"}):default("x")     -- default applied if missing
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- Empty arrays are valid
function api.empty_arrays.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            tags = g:array(g:string()),  -- [] is valid, nil uses default, missing is invalid if required
            items = g:array(g:object({
                id = g:integer():required()
            }))
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- Mixed required and optional in nested objects
function api.mixed_nested.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            user = g:object({
                name = g:string():required(),    -- required
                email = g:string():required(),   -- required
                phone = g:string(),              -- optional
                address = g:object({
                    street = g:string():required(),
                    city = g:string():required(),
                    zipCode = g:string(),        -- optional
                    country = g:string():default("USA")
                })
            })
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json { success = true, data = data }
end

-- ============================================================================
-- LOG LEVEL EXAMPLES
-- ============================================================================
-- Set log_level at top of this file to one of:
--   "debug"  - All messages (most verbose)
--   "info"   - Info and above
--   "warn"   - Warnings and above
--   "error"  - Only errors
--   "nope"   - No logs (disable logging)
-- Currently set to: "info"

function api.test_logging.get(ctx)
    return api.json {
        message = "Check server logs with different log_level settings"
    }
end

return api
