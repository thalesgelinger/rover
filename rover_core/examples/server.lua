local api = rover.server {}

function api.hello.get(ctx)
    return api.json {
        message = "Hello World"
    }
end

function api.hello.p_id.get(ctx)
    return api.json {
        message = "Hello " .. ctx:params().id
    }
end

function api.users.p_id.posts.p_postId.get(ctx)
    return api.json {
        message = "User " .. ctx:params().id .. " - Post " .. ctx:params().postId
    }
end

function api.greet.p_name.get(ctx)
    return api.json {
        greeting = "Hi " .. ctx:params().name
    }
end

return api
