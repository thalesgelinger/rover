local api = rover.server {
    port = 3002,
}

-- Test that api.json() accepts strings (optimization path)
function api.json_string_optimized.get()
    local json_str = '{"message":"This is pre-serialized JSON","optimized":true}'
    return api.json(json_str)
end

-- Test that api.json() still accepts tables (backward compatible)
function api.json_table_compatible.get()
    return api.json { message = "This uses table serialization", optimized = false }
end

-- Test api.json.status() with string
function api.json_status_string.get()
    local json_str = '{"status":"created"}'
    return api.json.status(201, json_str)
end

-- Test api.raw() for zero-copy responses
function api.raw_zero_copy.get()
    local data = '{"message":"Zero copy via api.raw"}'
    return api.raw(data)
end

return api
