local api = rover.server {}
local g = rover.guard

function api.test.post(ctx)
    local success, user = xpcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            email = g:string():required(),
            age = g:integer():default(25),
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

return api
