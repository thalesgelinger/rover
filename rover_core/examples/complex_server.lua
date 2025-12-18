
local api = rover.server {}

function api.hello.get(ctx)
    local token = ctx:headers()["Authorization"]

    if not token then
        return api.json:status(401) {
            message = "Hello World"
        }
    end

    return api.json:status(200) {
        message = "Hello World"
    }
end

function api.hello.world.get(ctx)
    return api.json{
        message = "Hello World Nested"
    }
end

function api.hello.world.post(ctx)
    return api.json {
        message = "Post to Hello World"
    }
end

function api.users.get(ctx)
    return api.json{
        users = {}
    }
end

function api.users.create.post(ctx)
    return api.json{
        created = true
    }
end

return api
