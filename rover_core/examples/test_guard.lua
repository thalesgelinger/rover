local api = rover.server {}
local g = rover.guard

function api.test.post(ctx)
    local success, user = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Missing name param"),
            email = g:string():required("Email is required"),
            age = g:integer()
        }
    end)

    if not success then
        return api:error(400, user)
    end

    return api.json { name = user.name, email = user.email, age = user.age }
end

return api
