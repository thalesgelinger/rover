local api = rover.server {}

-- Simple string response
function api.ping.get(ctx)
    return "pong"
end

-- JSON response
function api.info.get(ctx)
    return {
        version = "1.0.0",
        status = "running"
    }
end

-- Access request context
function api.reflect.get(ctx)
    return {
        method = ctx.method,
        path = ctx.path,
        headers = ctx.headers,
        query = ctx.query
    }
end

-- POST with body
function api.users.post(ctx)
    if not ctx.body then
        return {
            status = 400,
            message = "Body required"
        }
    end
    
    return {
        created = true,
        body = ctx.body
    }
end

-- Number response
function api.random.get(ctx)
    return 42
end

-- Boolean response
function api.enabled.get(ctx)
    return true
end

return api
