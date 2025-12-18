local api = rover.server {}

function api.test.get(ctx)
    local headers = ctx:headers()

    -- Print all headers
    for k, v in pairs(headers) do
        print(k .. ": " .. v)
    end

    return api.json {
        headers_received = headers
    }
end

return api
