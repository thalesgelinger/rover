local api = rover.server {}
local g = rover.guard

function api.test.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required("Missing name param"),
        email = g:string():required("Email is required")
    }

    return api.json { name = user.name, email = user.email }
end

return api
