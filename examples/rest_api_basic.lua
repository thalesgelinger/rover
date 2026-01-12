local api = rover.server {}

local count = 0

function api.hello.get(ctx)
    count = count + 1
    return api.json {
        message = "Hello World"
    }
end

function api.write.p_name.get(ctx)
    count = count + 1
    local file = io.open("/tmp/rover_test.txt", "w")
    file:write("Hello from Rover async I/O!\n")
    file:write("This is line 2\n")
    file:write("And line 3")
    file:close()

    return api.json {
        example = "File write",
        message = "Successfully wrote to /tmp/rover_test.txt"
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
