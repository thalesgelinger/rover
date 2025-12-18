local api = rover.server {}

-- Test 2: Metadata not showing in JSON
function api.metadata.get(ctx)
    return api.json:status(201) {
        created = true,
        id = 123
    }
end

-- Test 3: Backward compatibility - plain table
function api.plain.get(ctx)
    return {
        message = "Plain table should work"
    }
end

-- Test 4: Multiple status codes
function api.multi.get(ctx)
    if ctx:query()["fail"] then
        return api.json:status(400) {
            error = "Bad Request"
        }
    end
    return api.json:status(201) {
        success = true
    }
end

return api
