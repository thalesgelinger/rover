export default `local api = rover.server {}

function api.hello.get(ctx)
    return api.json {
        message = "Hello World"
    }
end

function api.hello.p_id.get(ctx)
    local id = ctx:params().id

    return api.json {
        message = "Hello " .. id
    }
end

return api`;
