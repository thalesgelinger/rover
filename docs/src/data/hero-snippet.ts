export default `local api = rover.server {}

function api.hello.get(ctx)
    return api.json {
        message = "Hello World"
    }
end

return api`;
