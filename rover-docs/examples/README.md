# Rover Examples

This directory contains runnable examples demonstrating Rover's static assets and upload capabilities.

## Examples

### 1. Static Assets (`static-assets/`)

A complete example demonstrating static file serving with:

- **Static mount DSL**: `api.assets.static { dir = "...", cache = "..." }`
- **Route precedence**: API routes take priority over static files
- **Cache headers**: ETag, Last-Modified, and Cache-Control
- **304 Not Modified**: Conditional request handling
- **Security**: Path traversal protection

**Files:**
- `app.lua` - Server application
- `public/assets/site.css` - Stylesheet (served statically)
- `public/assets/app.js` - JavaScript (served statically)

**Run:**
```bash
cd rover-docs/examples/static-assets
cargo run -p rover_cli -- run app.lua
```

Then open http://localhost:8080 in your browser.

### 2. Uploads Demo (`uploads-demo/`)

A complete file upload application demonstrating:

- **Multipart uploads**: `ctx:body():file()` and `ctx:body():form()`
- **File validation**: Type checking and size limits
- **Security**: Path traversal protection and filename sanitization
- **Static serving**: Uploaded files served via static mount
- **REST API**: Full CRUD operations for uploaded files

**Files:**
- `app.lua` - Server application with multipart handling
- `public/app.js` - Client-side upload interface

**Run:**
```bash
cd rover-docs/examples/uploads-demo
cargo run -p rover_cli -- run app.lua
```

Then open http://localhost:8080 in your browser.

## Common DSL Patterns

### Static Mount

```lua
-- Serve files from 'public/' at '/assets/*'
api.assets.static {
    dir = "public",
    cache = "public, max-age=31536000, immutable"
}

-- Multiple static mounts
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0"
}
```

### Multipart Upload

```lua
function api.uploads.post(ctx)
    -- Get uploaded file
    local file = ctx:body():file("field_name")

    if file then
        -- file.name, file.size, file.type, file.data
        local f = io.open("uploads/" .. file.name, "wb")
        f:write(file.data)
        f:close()
    end

    -- Get form fields
    local form = ctx:body():form()
    -- form.field_name, form.description, etc.
end
```

### Route Precedence

```lua
-- Static mount
api.assets.static { dir = "public" }

-- API routes take precedence
function api.assets.health.get(ctx)
    return { status = "ok" }  -- GET /assets/health returns API, not file
end
```

## Testing

Each example includes integration tests. Run tests with:

```bash
# Test static assets example
cargo test -p rover-server --test multipart_static_integration

# Test route precedence
cargo test -p rover-server --test route_precedence_integration
```

## Documentation

For more details, see:
- [Uploads and Static Assets Guide](/docs/http-and-realtime/uploads-and-static-assets/)
- [Context API](/docs/server/context-api/)
- [Route Patterns](/docs/server/route-patterns/)
