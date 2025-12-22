local api = rover.server {}
local g = rover.guard

-- Test 1: Basic types with required
function api.basic.post(ctx)
    local data = ctx:body():expect {
        name = g:string():required(),
        age = g:integer(),
        active = g:boolean()
    }
    return api.json(data)
end

-- Test 2: Enum validation
function api.enum.post(ctx)
    local data = ctx:body():expect {
        status = g:string():enum({"active", "inactive", "pending"}):required()
    }
    return api.json(data)
end

-- Test 3: Default values
function api.defaults.post(ctx)
    local data = ctx:body():expect {
        theme = g:string():default("light"),
        count = g:integer():default(10)
    }
    return api.json(data)
end

-- Test 4: Arrays
function api.arrays.post(ctx)
    local data = ctx:body():expect {
        tags = g:array(g:string()),
        scores = g:array(g:integer():required())
    }
    return api.json(data)
end

-- Test 5: Nested objects
function api.nested.post(ctx)
    local data = ctx:body():expect {
        user = g:object({
            name = g:string():required(),
            email = g:string():required()
        })
    }
    return api.json(data)
end

-- Test 6: Complex nested
function api.complex.post(ctx)
    local data = ctx:body():expect {
        user = g:object({
            name = g:string():required(),
            age = g:integer()
        }),
        items = g:array(g:object({
            id = g:integer():required(),
            name = g:string():required()
        }))
    }
    return api.json(data)
end

return api
