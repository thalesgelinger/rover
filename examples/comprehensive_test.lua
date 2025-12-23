local api = rover.server {}

-- JSON response
function api.info.get(ctx)
    return api.json {
        version = "1.0.0",
        status = "running"
    }
end

-- Access request context
function api.reflect.get(ctx)
    return api.json {
        method = ctx.method,
        path = ctx.path,
        headers = ctx:headers(),
        query = ctx:query()
    }
end

-- POST with body
function api.users.post(ctx)
    local body = ctx:body()
    if not body then
        return api.json:status(400) {
            message = "Body required"
        }
    end
    
    return api.json {
        created = true,
        body = body
    }
end

return api
