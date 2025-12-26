-- Query parameters benchmark
-- Tests query string parsing and cloning performance

local api = rover.server {
    port = 3000,
    log_level = "nope"
}

function api.search.get(ctx)
    local query = ctx:query()

    return api.json:status(200, {
        search = {
            q = query.q or "",
            page = query.page or "1",
            limit = query.limit or "10",
            sort = query.sort or "relevance",
            filter = query.filter or "all"
        },
        results = {}
    })
end

return api
