local api = rover.server {
    port = 3004,
}

function api.test1.get()
    return api.json '{"message":"hello"}'
end

function api.test2.get()
    return api.json {message="hello"}
end

return api
