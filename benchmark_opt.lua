local api = rover.server {
    port = 3005,
    log_level = "none"
}

-- Optimized: Pre-serialized JSON string
function api.optimized.get()
    local json_str = '{"message":"This is pre-serialized JSON","optimization":"string_input"}'
    return api.json(json_str)
end

-- Traditional: Table serialization
function api.traditional.get()
    return api.json {
        message = "This uses table serialization",
        optimization = "table_input"
    }
end

return api
