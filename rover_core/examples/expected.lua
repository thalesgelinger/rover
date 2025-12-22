local api = rover.server {}
local g = rover.guard

function api.users.post(ctx)
    local user = ctx:body():expect {
        name = g:string():required("Missing name param"),
        email = g:string()
    }

    print(user)

    return api.json:status(201)
end

return api
