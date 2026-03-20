-- Static file serving example
-- Demonstrates static mount DSL and route precedence

local api = rover.server {}

-- Mount static files at /assets/*
-- The 'dir' parameter is required and points to the directory to serve
-- The 'cache' parameter is optional and sets Cache-Control header
api.assets.static {
    dir = "public",
    cache = "public, max-age=3600"
}

-- This exact route takes precedence over the static mount
-- GET /assets/health returns API response, not a file
function api.assets.health.get(ctx)
    return { status = "ok", timestamp = os.time() }
end

-- Dynamic route also takes precedence over static mount
-- GET /assets/info/:id returns metadata
function api.assets.p_id.get(ctx)
    return {
        file_id = ctx:params().id,
        requested_path = ctx.path
    }
end

-- Mount uploads directory separately
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0"
}

return api
