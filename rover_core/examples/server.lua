local api = rover.server {
    port = 3000
}

function api.hello.get(ctx)
    return {
        message = "Hello World"
    }
end

return api
