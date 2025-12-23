local api = rover.server {}

-- Access request context: method, path, headers, query params
function api.echo.get(ctx)
    return api.json {
        method = ctx.method,
        path = ctx.path,
        user_agent = ctx:headers()["user-agent"] or "none",
        page = ctx:query().page or "1",
        limit = ctx:query().limit or "10"
    }
end

-- POST with request body and content-type header
function api.echo.post(ctx)
    return api.json {
        received_body = ctx:body() or "no body",
        content_type = ctx:headers()["content-type"] or "none"
    }
end

return api
