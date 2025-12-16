local api = rover.server {}

function api.hello.get(ctx)
    return {
        message = "Hello World"
    }
end

function api.hello.world.get(ctx)
    return {
        message = "Hello World Nested"
    }
end

function api.hello.world.post(ctx)
    return {
        message = "Post to Hello World"
    }
end

function api.users.get(ctx)
    return {
        users = {}
    }
end

function api.users.create.post(ctx)
    return {
        created = true
    }
end

return api
