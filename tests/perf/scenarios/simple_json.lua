-- Simple JSON response benchmark
-- Tests basic JSON serialization performance

local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.simple.get()
    return api.json:status(200, {
        message = "Hello, World!",
        status = "ok",
        timestamp = 1704067200
    })
end

return api
