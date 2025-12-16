local api = rover.server {}

function api.success.get(ctx)
    return "Plain text success"
end

function api.badrequest.get(ctx)
    return {
        status = 400,
        message = "Bad request - missing required field"
    }
end

function api.notfound.get(ctx)
    return {
        status = 404,
        message = "Resource not found"
    }
end

function api.servererror.get(ctx)
    return {
        status = 500,
        message = "Internal server error"
    }
end

return api
