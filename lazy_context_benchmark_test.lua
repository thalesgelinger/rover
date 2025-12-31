-- Test script to measure actual context creation overhead

local api = rover.server {
    port = 3001,
    log_level = "nope"
}

-- Handler 1: Uses NO context fields (like current benchmark)
function api.test_none.get(ctx)
    return { result = "ok" }
end

-- Handler 2: Uses ONLY method (simple field, no closure)
function api.test_method.get(ctx)
    return { method = ctx.method }
end

-- Handler 3: Uses params (1 closure)
function api.test_params.p_id.get(ctx)
    return { id = ctx:params().id }
end

-- Handler 4: Uses ALL fields (4 closures)
function api.test_all.get(ctx)
    local h = ctx:headers()
    local q = ctx:query()
    local p = ctx:params()
    return {
        headers_count = #h,
        query_count = #q,
        params_count = #p
    }
end

-- Handler 5: Real-world example - typical API endpoint
function api.users.p_id.get(ctx)
    local params = ctx:params()
    -- In reality you'd fetch from DB here
    return {
        user_id = params.id,
        status = "active"
    }
end

return api
