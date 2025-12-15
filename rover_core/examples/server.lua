local api = rover.server {}

function api.hello.get(ctx)
    return {
        message = "Hello World"
    }
end
