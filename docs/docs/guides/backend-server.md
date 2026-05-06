---
sidebar_position: 1
---

# Backend Server

Build HTTP APIs with Rover's built-in server and automatic routing.

## Creating a Server

Initialize a server with the `rover.server` function:

```lua
local api = rover.server { }

function api.hello.get(ctx)
    return { message = "Hello World" }
end

return api
```

## Path Parameters

Use the `p_<name>` prefix to define path parameters:

```lua
function api.users.p_id.get(ctx)
    return {
        user_id = ctx:params().id
    }
end
```

This creates a route at `/users/:id`.

## Multiple Route Parameters

You can have multiple parameters in a single route:

```lua
function api.users.p_id.posts.p_postId.get(ctx)
    local params = ctx:params()
    return {
        user = params.id,
        post = params.postId
    }
end
```

This creates a route at `/users/:id/posts/:postId`.

## HTTP Methods

Rover supports all standard HTTP methods:

- `get` - GET requests
- `post` - POST requests
- `put` - PUT requests
- `patch` - PATCH requests
- `delete` - DELETE requests
- `head` - HEAD requests
- `options` - OPTIONS requests

Example:

```lua
function api.users.get(ctx)
    return { users = {...} }
end

function api.users.post(ctx)
    local body = ctx:body()
    -- Create user
    return { id = 123 }
end

function api.users.p_id.delete(ctx)
    local id = ctx:params().id
    -- Delete user
    return api.no_content()
end
```

## Route Patterns

Routes are built from nested table access:

- `api.users.get` → `/users` (GET)
- `api.users.p_id.get` → `/users/:id` (GET)
- `api.users.p_id.posts.p_pid.get` → `/users/:id/posts/:pid` (GET)

## Static Mounts

Use `api.<scope>.static { ... }` to mount static files under a route prefix.

```lua
local api = rover.server {}

api.assets.static {
    dir = "public",
    cache = "public, max-age=60"
}

function api.assets.health.get(ctx)
    return { ok = true }
end

return api
```

Behavior:

- `GET /assets/app.js` serves `public/app.js`.
- Static mounts include traversal protection, cache validators (`ETag`, `Last-Modified`), and conditional `304` handling.
- `cache` maps to the `Cache-Control` response header.
- Exact API routes under the same prefix (like `/assets/health`) take precedence over static mount catch-all paths.

### Static Mount DSL

The static mount DSL accepts a configuration table with the following fields:

- **`dir`** (required, string): Path to the directory containing static files. Must be a non-empty string pointing to an accessible directory.
- **`cache`** (optional, string): Value for the `Cache-Control` response header. If provided, must be a non-empty string (e.g., `"public, max-age=3600"`).

### Route Precedence

API routes and static mounts at the same prefix follow these precedence rules:

1. **Exact API routes** take precedence over static mount catch-all paths.
2. **Dynamic API routes** (with path parameters) take precedence over static mount catch-all paths.
3. **Static mount** serves as a fallback for paths not matching any API route.

Example precedence:

```lua
local api = rover.server {}

-- Static mount at /assets/*
api.assets.static { dir = "public" }

-- This exact route takes precedence over the static mount
function api.assets.health.get(ctx)
    return { status = "healthy" }
end

-- This dynamic route also takes precedence
function api.assets.p_filename.get(ctx)
    return { metadata = ctx:params().filename }
end

return api
```

Requests:
- `GET /assets/app.js` → serves `public/app.js` (static mount)
- `GET /assets/health` → returns `{ status = "healthy" }` (exact API route)
- `GET /assets/config.json` → returns metadata (dynamic API route)

### Security Features

- **Path traversal protection**: Requests attempting to access files outside the mounted directory (e.g., `../../../etc/passwd`) are rejected.
- **No directory listings**: Directory index/listing support is explicitly out of scope for this release. Requests to directory paths return 403 Forbidden.
- **Cache validation**: Static files are served with `ETag` and `Last-Modified` headers for efficient client-side caching.

### Cache Behavior

When the `cache` option is set, the response includes a `Cache-Control` header with the specified value:

```lua
api.assets.static {
    dir = "public",
    cache = "public, max-age=31536000, immutable"  -- 1 year cache for versioned assets
}
```

The static handler also supports conditional requests:
- Returns `304 Not Modified` when `If-None-Match` matches the file's `ETag`
- Returns `304 Not Modified` when `If-Modified-Since` is after the file's modification time

## Next Steps

- [Context API](/docs/guides/context-api) - Access request data
- [Response Builders](/docs/guides/response-builders) - Return structured responses
