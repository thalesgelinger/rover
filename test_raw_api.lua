local api = rover.server {
    port = 3001,
}

-- Test api.raw() with simple string
function api.raw_simple.get()
    return api.raw("Hello from raw!")
end

-- Test api.raw() with status 201
function api.raw_created.get()
    return api.raw.status(201, "Created via raw")
end

-- Test api.raw() with JSON (simulating pre-serialized data)
function api.raw_json_data.get()
    local json_str = '{"message":"Hello from raw JSON","value":42}'
    return api.raw(json_str)
end

-- Test api.json() still works (regression test)
function api.json_test.get()
    return api.json { message = "Hello from json" }
end

return api
