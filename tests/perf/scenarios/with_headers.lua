-- Headers access benchmark
-- Tests header parsing and cloning performance

local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.echo_headers.get(ctx)
    local headers = ctx:headers()

    return api.json:status(200, {
        received_headers = {
            content_type = headers["content-type"],
            user_agent = headers["user-agent"],
            authorization = headers.authorization,
            accept = headers.accept
        },
        header_count = 0  -- Would need to iterate to count
    })
end

return api
