local api = rover.server {}

-- Simple GET endpoint
function api.hello.get(ctx)
    return api.json {
        message = "Hello World"
    }
end

-- Path parameter: /hello/:id
function api.hello.p_id.get(ctx)
    return api.json {
        message = "Hello " .. ctx:params().id
    }
end

-- Multiple path params: /users/:id/posts/:postId
function api.users.p_id.posts.p_postId.get(ctx)
    return api.json {
        message = "User " .. ctx:params().id .. " - Post " .. ctx:params().postId
    }
end

-- Path param with custom greeting: /greet/:name
function api.greet.p_name.get(ctx)
    return api.json {
        greeting = "Hi " .. ctx:params().name
    }
end

return api
