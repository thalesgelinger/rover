-- Rotate through different user IDs
local counter = 0

wrk.method = "GET"
wrk.headers["Content-Type"] = "application/json"

request = function()
    counter = counter + 1
    local user_id = (counter % 1000) + 1
    return wrk.format("GET", "/users/" .. user_id)
end
