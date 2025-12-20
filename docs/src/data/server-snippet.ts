export default `local api = rover.server {}

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

return api`;
