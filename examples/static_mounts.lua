-- Static file serving example
-- Demonstrates static mount DSL and route precedence

local api = rover.server {}

-- ============================================================================
-- Example 1: Basic Static Mount
-- ============================================================================

-- Mount static files at /assets/*
-- The 'dir' parameter is required and points to the directory to serve
-- The 'cache' parameter is optional and sets Cache-Control header
api.assets.static {
    dir = "public",
    cache = "public, max-age=3600"
}

-- ============================================================================
-- Example 2: Route Precedence
-- ============================================================================

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

-- ============================================================================
-- Example 3: Multiple Static Mounts
-- ============================================================================

-- Mount uploads directory separately with different cache settings
-- User uploads typically shouldn't be cached
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0"
}

-- Documentation with short cache (5 minutes)
api.docs.static {
    dir = "docs",
    cache = "public, max-age=300"
}

-- ============================================================================
-- Example 4: Upload Endpoint
-- ============================================================================

-- Upload endpoint for saving files to the uploads directory
-- Note: This is a simplified example - in production, validate file types,
-- limit file sizes, and scan for malware
function api.uploads.post(ctx)
    local body = ctx:body():json()
    
    -- Validate required fields
    if not body or not body.filename or not body.content then
        return api:error(400, "Missing filename or content")
    end
    
    -- Validate filename (prevent path traversal in filename)
    local filename = body.filename:gsub("\\", "_"):gsub("/", "_")
    if filename:match("^%.") or filename:match("%.") then
        return api:error(400, "Invalid filename")
    end
    
    local filepath = string.format("uploads/%s", filename)
    
    -- Write file
    local f = io.open(filepath, "wb")
    if not f then
        return api:error(500, "Failed to save file")
    end
    
    f:write(body.content)
    f:close()
    
    return api.json:status(201, {
        message = "File uploaded successfully",
        filename = filename,
        url = string.format("/uploads/%s", filename),
        size = #body.content,
    })
end

-- Get upload metadata (takes precedence over static mount)
function api.uploads.p_filename.get(ctx)
    local filename = ctx:params().filename:gsub("\\", "_"):gsub("/", "_")
    local filepath = string.format("uploads/%s", filename)
    
    -- Check if file exists
    local f = io.open(filepath, "rb")
    if not f then
        return api:error(404, "File not found")
    end
    
    local content = f:read("*a")
    f:close()
    
    return {
        filename = filename,
        size = #content,
        url = string.format("/uploads/%s", filename),
    }
end

-- ============================================================================
-- Example 5: Upload Stats API
-- ============================================================================

function api.uploads.stats.get(ctx)
    local files = {}
    local total_size = 0
    
    -- List files in uploads directory (simplified)
    -- In production, use proper directory listing or database
    local cmd = "ls uploads/"
    local handle = io.popen(cmd)
    
    if handle then
        for line in handle:lines() do
            -- Skip directories and hidden files
            if not line:match("^%.") then
                local filepath = string.format("uploads/%s", line)
                local attr = io.open(filepath, "rb")
                if attr then
                    local content = attr:read("*a")
                    local size = #content
                    attr:close()
                    
                    table.insert(files, {
                        filename = line,
                        size = size,
                    })
                    total_size = total_size + size
                end
            end
        end
        handle:close()
    end
    
    return {
        count = #files,
        total_size = total_size,
        files = files,
    }
end

-- ============================================================================
-- Example 6: Delete Upload
-- ============================================================================

function api.uploads.p_filename.delete(ctx)
    local filename = ctx:params().filename:gsub("\\", "_"):gsub("/", "_")
    local filepath = string.format("uploads/%s", filename)
    
    -- Check if file exists
    local f = io.open(filepath, "rb")
    if not f then
        return api:error(404, "File not found")
    end
    f:close()
    
    -- Delete file
    local ok, err = os.remove(filepath)
    if not ok then
        return api:error(500, "Failed to delete file: " .. (err or "unknown error"))
    end
    
    return api.no_content()
end

return api
