local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.hello.get()
    return api.json { message = "Hello World" }
end

function api.health.get()
    return api.json { status = "ok" }
end

function api.users.p_id.get(req)
    local id = req:params().id
    return api.json { id = id, name = "User " .. id }
end

function api.products.p_id.get(req)
    local id = req:params().id
    return api.json { id = id, name = "Product", stock = 42 }
end

return api
