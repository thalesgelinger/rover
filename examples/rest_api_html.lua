local api = rover.server {}

-- Serve HTML response instead of JSON
function api.get()
    return api.html [[
        <h1>Hello World</h1>
    ]]
end

return api
