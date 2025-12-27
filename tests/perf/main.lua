local api = rover.server {
    port = 3000,
    log_level = "nope"
}

-- GET endpoint - echo query parameters and request info
function api.echo.get()
    return api.json:status(200, {
        method = "GET",
        path = "/echo",
        message = "Echo server GET endpoint"
    })
end

-- POST endpoint - echo back the request body
function api.echo.post()
    return api.json:status(200, {
        method = "POST",
        path = "/echo",
        message = "Echo server POST endpoint",
        timestamp = os.time()
    })
end

return api
