-- Complex JSON response benchmark
-- Tests JSON serialization with nested objects and arrays

local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.complex.get()
    local users = {}
    for i = 1, 50 do
        users[i] = {
            id = i,
            name = "User " .. i,
            email = "user" .. i .. "@example.com",
            active = i % 2 == 0
        }
    end

    return api.json:status(200, {
        status = "success",
        data = {
            users = users,
            metadata = {
                total = 50,
                page = 1,
                per_page = 50,
                has_next = false
            },
            timestamp = 1704067200
        }
    })
end

return api
