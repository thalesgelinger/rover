---
sidebar_position: 5
---

# Static Assets

Serve static files like HTML, CSS, JavaScript, and images with Rover's built-in static file server.

## Basic Static Mounts

Use `api.<scope>.static { ... }` to mount a directory of static files under a URL path:

```lua
local api = rover.server {}

-- Serve files from the "public" directory at /assets/*
api.assets.static {
    dir = "public",
    cache = "public, max-age=3600"
}

return api
```

With this setup:
- `public/app.js` is served at `GET /assets/app.js`
- `public/css/style.css` is served at `GET /assets/css/style.css`
- `public/index.html` is served at `GET /assets/index.html`

### Static Mount DSL

The static mount accepts a configuration table:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `dir` | string | Yes | Path to the directory containing static files |
| `cache` | string | No | Value for the `Cache-Control` response header |

## Route Precedence

API routes always take precedence over static files at the same path:

```lua
local api = rover.server {}

-- Static files at /assets/*
api.assets.static { dir = "public" }

-- This API route takes precedence over static files
function api.assets.health.get(ctx)
    return { status = "healthy", timestamp = os.time() }
end

-- Dynamic routes also take precedence
function api.assets.p_filename.get(ctx)
    return { file = ctx:params().filename }
end

return api
```

Request behavior:
- `GET /assets/app.js` → serves `public/app.js` (static)
- `GET /assets/health` → API response `{ status = "healthy" }`
- `GET /assets/config.json` → API response with filename parameter

## Security Features

Static mounts include several security protections:

### Path Traversal Protection

Attempts to access files outside the mounted directory are blocked:

```
GET /assets/../../../etc/passwd  → 403 Forbidden
GET /assets/%2e%2e/%2e%2e/etc/passwd  → 403 Forbidden
```

### No Directory Listings

Directory index requests return 403 Forbidden:

```
GET /assets/  → 403 Forbidden
GET /assets/css/  → 403 Forbidden
```

## Cache Behavior

Static files are served with automatic cache headers:

| File Type | Default Cache-Control |
|-----------|----------------------|
| HTML files | `no-cache` |
| CSS, JS, images, fonts | `public, max-age=31536000, immutable` |
| Other files | `public, max-age=86400` |

Override with the `cache` option:

```lua
-- Aggressive caching for versioned assets
api.assets.static {
    dir = "public",
    cache = "public, max-age=31536000, immutable"
}

-- No caching for user uploads
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0, must-revalidate"
}
```

## Conditional Requests (304 Not Modified)

Static files support efficient client-side caching via conditional requests:

```
First request:
  GET /assets/app.js
  Response: 200 OK with ETag and Last-Modified headers

Subsequent request:
  GET /assets/app.js
  If-None-Match: "abc123"
  Response: 304 Not Modified (no body)
```

The server automatically:
- Generates ETags based on file content
- Sets Last-Modified based on file modification time
- Returns 304 when If-None-Match matches or If-Modified-Since is current

## Multiple Static Mounts

Mount different directories under different paths:

```lua
local api = rover.server {}

-- Assets with long-term caching
api.assets.static {
    dir = "public",
    cache = "public, max-age=31536000, immutable"
}

-- User uploads with no caching
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0"
}

-- Documentation with short cache
api.docs.static {
    dir = "docs/dist",
    cache = "public, max-age=300"
}

return api
```

## File Uploads with Static Serving

Combine upload endpoints with static mounts for a complete file serving solution:

```lua
local api = rover.server {}
local g = rover.guard

-- Serve uploaded files
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0"
}

-- Upload endpoint
function api.uploads.post(ctx)
    local file = ctx:body():expect {
        filename = g:string():required(),
        content = g:string():required(),
    }

    -- Save file to uploads directory
    local path = string.format("uploads/%s", file.filename)
    local f = io.open(path, "wb")
    if f then
        f:write(file.content)
        f:close()
    end

    return api.json:status(201, {
        url = string.format("/uploads/%s", file.filename),
        filename = file.filename,
        size = #file.content,
    })
end

-- Upload metadata API (takes precedence over static)
function api.uploads.p_filename.get(ctx)
    local filename = ctx:params().filename
    local path = string.format("uploads/%s", filename)

    local f = io.open(path, "rb")
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

return api
```

## Content Type Detection

Static files are served with appropriate Content-Type headers based on file extension:

| Extension | Content-Type |
|-----------|--------------|
| `.html`, `.htm` | `text/html` |
| `.css` | `text/css` |
| `.js`, `.mjs`, `.cjs` | `application/javascript` |
| `.json`, `.map` | `application/json` |
| `.png` | `image/png` |
| `.jpg`, `.jpeg` | `image/jpeg` |
| `.gif` | `image/gif` |
| `.webp` | `image/webp` |
| `.svg` | `image/svg+xml` |
| `.woff`, `.woff2`, `.ttf`, `.otf` | Font types |
| `.pdf` | `application/pdf` |

## Best Practices

1. **Use aggressive caching for versioned assets**: Add content hashes to filenames and use `immutable` cache control.

2. **Keep uploads separate from code**: Mount uploads outside your application directory.

3. **Protect sensitive files**: Don't mount directories containing configuration files or secrets.

4. **Combine with API routes**: Use static mounts for files, API routes for dynamic functionality.

5. **Monitor cache performance**: Use 304 responses to reduce bandwidth.

## Complete Example

```lua
local api = rover.server {}
local g = rover.guard

-- Static assets with aggressive caching
api.assets.static {
    dir = "public",
    cache = "public, max-age=31536000, immutable"
}

-- User uploads with no caching
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0, must-revalidate"
}

-- API endpoint for upload stats (precedes static mount)
function api.uploads.stats.get(ctx)
    local files = {}
    for filename in io.popen('ls uploads/'):lines() do
        local attr = io.open("uploads/" .. filename, "rb")
        if attr then
            local content = attr:read("*a")
            table.insert(files, {
                filename = filename,
                size = #content,
            })
            attr:close()
        end
    end
    return { files = files, count = #files }
end

-- File upload endpoint
function api.uploads.post(ctx)
    local data = ctx:body():expect {
        filename = g:string():required(),
        content_type = g:string():required(),
        data = g:string():required(),
    }

    local path = string.format("uploads/%s", data.filename)
    local f = io.open(path, "wb")
    if not f then
        return api:error(500, "Failed to save file")
    end

    -- In a real app, you'd decode base64 or handle multipart
    f:write(data.data)
    f:close()

    return api.json:status(201, {
        url = string.format("/uploads/%s", data.filename),
        filename = data.filename,
    })
end

return api
```

## Next Steps

- [Backend Server](/docs/guides/backend-server) - Learn more about routing
- [Response Builders](/docs/guides/response-builders) - Return different response types
- [Context API](/docs/guides/context-api) - Access request data
