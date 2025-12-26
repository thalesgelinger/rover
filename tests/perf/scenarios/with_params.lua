-- URL parameters benchmark
-- Tests parameter extraction and cloning performance

local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api["users/:id"].get(ctx)
    local params = ctx:params()
    local user_id = params.id

    return api.json:status(200, {
        user_id = user_id,
        name = "User " .. user_id,
        email = "user" .. user_id .. "@example.com",
        profile = {
            bio = "This is user " .. user_id,
            location = "San Francisco",
            verified = true
        }
    })
end

return api
