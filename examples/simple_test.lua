local api = rover.server {
    port = 3001,
}

function api.hello.get()
    return api.json { message = "Hello World" }
end

return api
