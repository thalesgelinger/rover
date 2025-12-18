local api = rover.server {
    port = 3000,
    log_level = "debug"  -- Options: "debug", "info", "warn", "error", "nope"
}

function api.hello.get(ctx)
    return {
        message = "Hello World"
    }
end

function api.hello.p_id.get(ctx)
    return {
        message = "Hello " .. ctx:params().id
    }
end

function api.users.p_id.posts.p_postId.get(ctx)
    return {
        message = "User " .. ctx:params().id .. " - Post " .. ctx:params().postId
    }
end

function api.greet.p_name.get(ctx)
    return {
        greeting = "Hi " .. ctx:params().name
    }
end

return api
