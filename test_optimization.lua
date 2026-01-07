local api = rover.server {
    port = 3003,
}

-- api.json() with pre-serialized string (optimized path - zero-copy after allocation)
function api.json_optimized.get()
    local json_str = '{"message":"Pre-serialized JSON","optimized":true}'
    return api.json(json_str)
end

-- api.json() with table (traditional path - table serialization)
function api.json_traditional.get()
    return api.json { message = "Table serialization", optimized = false }
end

-- api.json.status() with string (optimized)
function api.json_status_optimized.get()
    local json_str = '{"status":"created"}'
    return api.json.status(201, json_str)
end

-- api.json.status() with table (traditional)
function api.json_status_traditional.get()
    return api.json.status(201, { status = "created", method = "table" })
end

return api
