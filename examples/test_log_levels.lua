local api = rover.server {
    port = 3000,
    log_level = "info"  -- Change to: "debug", "info", "warn", "error", "nope"
}

function api.test.get(ctx)
    return {
        message = "Testing logs"
    }
end

return api
