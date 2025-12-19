local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.yabadabadoo.get()
    return api.json:status(200, {
        message = "We are all good champs"
    })
end

return api
