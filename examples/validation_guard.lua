local api = rover.server {}
local g = rover.guard

-- Pattern 1: Basic guard validation with required fields (without pcall)
function api.basic.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required("Missing name param"),
        email = g:string():required("Email is required")
    }

    return api.json { name = user.name, email = user.email }
end

-- Pattern 2: Guard validation with pcall for error handling
function api.with_pcall.post(ctx)
    local success, user = pcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            email = g:string():required(),
            age = g:integer()
        }
    end)

    if not success then
        return api:error(400, user)
    end

    return api.json { name = user.name, email = user.email, age = user.age }
end

-- Pattern 3: Guard validation with xpcall for detailed error handling
function api.with_xpcall.post(ctx)
    local success, user = xpcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            email = g:string():required(),
            age = g:integer(),
            tags = g:array(g:string())
        }
    end, function(err)
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

-- Pattern 4: Direct guard usage (not via body)
function api.direct_guard.get(ctx)
    local success, result = pcall(function()
        local data = {
            name = "John",
            age = 30
        }

        local name_val = g:string()
        name_val:required()

        local age_val = g:integer()

        return rover.guard(data, {
            name = name_val,
            age = age_val
        })
    end)

    if not success then
        return api:error(400, result)
    end

    return api.json(result)
end

-- Pattern 5: Enum validation
function api.enum.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            status = g:string():enum({"active", "inactive", "pending"}):required()
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json(data)
end

-- Pattern 6: Default values
function api.defaults.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            theme = g:string():default("light"),
            count = g:integer():default(10)
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json(data)
end

-- Pattern 7: Arrays
function api.arrays.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            tags = g:array(g:string()),
            scores = g:array(g:integer():required())
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json(data)
end

-- Pattern 8: Nested objects
function api.nested.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            user = g:object({
                name = g:string():required(),
                email = g:string():required()
            })
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json(data)
end

-- Pattern 9: Complex nested with arrays
function api.complex.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            user = g:object({
                name = g:string():required(),
                age = g:integer()
            }),
            items = g:array(g:object({
                id = g:integer():required(),
                name = g:string():required()
            }))
        }
    end)

    if not success then
        return api:error(400, data)
    end

    return api.json(data)
end

return api
