local api = rover.server {}
local g = rover.guard

-- Test endpoint that directly returns ValidationErrors without pcall
function api.test.structured.post(ctx)
    -- This will throw ValidationErrors directly
    local data = ctx:body():expect {
        name = g:string():required("Name is required"),
        email = g:string():required("Email is required"),
        age = g:integer():required("Age is required")
    }

    return api.json(data)
end

-- Test with pcall
function api.test.pcall.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            title = g:string():required("Title is required"),
            count = g:integer():required("Count is required")
        }
    end)

    if not success then
        return api:error(400, result)
    end

    return api.json(result)
end

return api
