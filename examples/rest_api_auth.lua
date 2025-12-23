local api = rover.server {}

-- Endpoint with authorization check
function api.hello.get(ctx)
    local token = ctx:headers().Authorization

    if not token then
        return api.json:status(401, {
            message = "Unauthorized"
        })
    end

    return api.json:status(200, {
        message = "Hello World"
    })
end

-- Nested route with GET
function api.hello.world.get(ctx)
    return api.json {
        message = "Hello World Nested"
    }
end

-- Nested route with POST
function api.hello.world.post(ctx)
    return api.json {
        message = "Post to Hello World"
    }
end

-- GET users list
function api.users.get(ctx)
    return api.json {
        users = {}
    }
end

-- POST to create user (nested resource)
function api.users.create.post(ctx)
    return api.json {
        created = true
    }
end

return api
