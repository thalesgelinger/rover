local api = rover.server {}

function api.test.get(ctx)
    return api.json:status(201) {
        message = "Created successfully"
    }
end

return api
