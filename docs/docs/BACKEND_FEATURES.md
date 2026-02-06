# Rover Backend Features Specification

Complete specification for all backend features needed to make Rover production-ready.
Each feature includes: status, API design, and usage examples — all following Rover's
nested-table DSL philosophy.

---

## Feature Status Overview

| # | Feature | Status |
|---|---------|--------|
| 1 | [Middleware System](#1-middleware-system) | **To Build** |
| 2 | [Authentication & Authorization](#2-authentication--authorization) | **To Build** |
| 3 | [Server-Sent Events (SSE)](#3-server-sent-events-sse) | **To Build** |
| 4 | [WebSockets](#4-websockets) | **Done** (needs docs) |
| 5 | [CORS](#5-cors) | **To Build** |
| 6 | [Rate Limiting](#6-rate-limiting) | **To Build** |
| 7 | [Session Management](#7-session-management) | **To Build** |
| 8 | [Cookie Handling](#8-cookie-handling) | **To Build** |
| 9 | [Static File Serving](#9-static-file-serving) | **To Build** |
| 10 | [File Uploads / Multipart](#10-file-uploads--multipart) | **To Build** |
| 11 | [Response Compression](#11-response-compression) | **To Build** |
| 12 | [Security Headers](#12-security-headers) | **To Build** |
| 13 | [Request Logging](#13-request-logging) | **Partial** (basic exists) |
| 14 | [Streaming Responses](#14-streaming-responses) | **To Build** |
| 15 | [Background Jobs](#15-background-jobs) | **To Build** |
| 16 | [Environment & Config](#16-environment--config) | **To Build** |
| 17 | [Graceful Shutdown](#17-graceful-shutdown) | **To Build** |
| 18 | [Error Handling Middleware](#18-error-handling-middleware) | **To Build** |
| 19 | [Request ID / Correlation](#19-request-id--correlation) | **To Build** |
| 20 | [Body Size Limits](#20-body-size-limits) | **To Build** |

---

## 1. Middleware System

**Priority: Critical** — Every other feature depends on this.

Middleware in Rover follows the same nested-table pattern as routes. Middleware
functions receive `ctx` and return either `nil` (continue) or a response (short-circuit).

### Global Middleware

Applies to every route on the server.

```lua
local api = rover.server {}

-- Global middleware: runs before every request
function api.middleware(ctx)
    -- Log every request
    print(ctx.method .. " " .. ctx.path)

    -- Return nil to continue to the next middleware/handler
    -- Return a response to short-circuit
end

function api.hello.get(ctx)
    return { message = "Hello" }
end

return api
```

### Route-Scoped Middleware

Applies to all routes under a path segment.

```lua
local api = rover.server {}

-- Middleware for all /admin/* routes
function api.admin.middleware(ctx)
    local token = ctx:headers().Authorization
    if not token then
        return api.json:status(401, { error = "Unauthorized" })
    end
end

-- Protected: middleware runs before this handler
function api.admin.users.get(ctx)
    return api.json { users = {} }
end

-- Protected: same middleware applies
function api.admin.settings.get(ctx)
    return api.json { theme = "dark" }
end

-- NOT protected: no middleware on this path
function api.hello.get(ctx)
    return { message = "Hello" }
end

return api
```

### Multiple Middleware (Ordered)

When a route has middleware at multiple levels, they execute top-down
(global first, then more specific).

```lua
local api = rover.server {}

-- 1st: Global middleware
function api.middleware(ctx)
    print("global")
end

-- 2nd: Scoped to /admin
function api.admin.middleware(ctx)
    print("admin auth check")
    local token = ctx:headers().Authorization
    if not token then
        return api.json:status(401, { error = "Unauthorized" })
    end
end

-- 3rd: Handler runs only if all middleware passed
function api.admin.dashboard.get(ctx)
    return { data = "secret" }
end

return api
```

**Execution order for `GET /admin/dashboard`:**
1. `api.middleware` (global)
2. `api.admin.middleware` (scoped)
3. `api.admin.dashboard.get` (handler)

If any middleware returns a response, execution stops.

### Named Middleware (Reusable)

Define middleware as functions and attach them to multiple paths.

```lua
local api = rover.server {}

-- Define reusable middleware
local function auth_middleware(ctx)
    local token = ctx:headers().Authorization
    if not token then
        return api.json:status(401, { error = "Unauthorized" })
    end
end

local function admin_only(ctx)
    local role = ctx:headers()["X-Role"]
    if role ~= "admin" then
        return api.json:status(403, { error = "Forbidden" })
    end
end

-- Attach to paths
api.users.middleware = auth_middleware
api.admin.middleware = admin_only

function api.users.get(ctx)
    return api.json { users = {} }
end

function api.admin.settings.get(ctx)
    return api.json { settings = {} }
end

return api
```

### Context Sharing Across Middleware

Middleware can attach data to the context for downstream handlers.

```lua
local api = rover.server {}

function api.middleware(ctx)
    local token = ctx:headers().Authorization
    if token then
        local user = decode_token(token)
        ctx:set("user", user)
        ctx:set("authenticated", true)
    end
end

function api.profile.get(ctx)
    local user = ctx:get("user")
    if not user then
        return api.json:status(401, { error = "Not logged in" })
    end
    return api.json { name = user.name, email = user.email }
end

return api
```

**Context store methods:**
- `ctx:set(key, value)` — store a value
- `ctx:get(key)` — retrieve a value (returns `nil` if not set)

---

## 2. Authentication & Authorization

**Priority: Critical** — Required for any production API.

Built on top of the middleware system. Rover provides auth helpers, not a full
auth framework — you choose JWT, session-based, or API key auth.

### Bearer Token / JWT Auth

```lua
local api = rover.server {}

-- JWT middleware
function api.auth.middleware(ctx)
    local header = ctx:headers().Authorization
    if not header then
        return api.json:status(401, { error = "Missing Authorization header" })
    end

    local token = header:match("^Bearer (.+)$")
    if not token then
        return api.json:status(401, { error = "Invalid Authorization format" })
    end

    local ok, claims = pcall(rover.jwt.verify, token, {
        secret = rover.env.JWT_SECRET,
        algorithms = { "HS256" },
    })

    if not ok then
        return api.json:status(401, { error = "Invalid token" })
    end

    ctx:set("user", claims)
end

-- Protected route
function api.auth.profile.get(ctx)
    local user = ctx:get("user")
    return api.json {
        id = user.sub,
        email = user.email,
    }
end

return api
```

### API Key Auth

```lua
local api = rover.server {}

function api.v1.middleware(ctx)
    local key = ctx:headers()["X-API-Key"] or ctx:query().api_key

    if not key then
        return api.json:status(401, { error = "API key required" })
    end

    -- Validate against database
    local api_key = db.api_keys:find():by_key_equals(key):one()
    if not api_key then
        return api.json:status(401, { error = "Invalid API key" })
    end

    ctx:set("api_key", api_key)
end

function api.v1.data.get(ctx)
    return api.json { data = "protected" }
end

return api
```

### Basic Auth

```lua
local api = rover.server {}

function api.admin.middleware(ctx)
    local header = ctx:headers().Authorization
    if not header then
        return api.error(401, "Unauthorized")
    end

    local encoded = header:match("^Basic (.+)$")
    if not encoded then
        return api.error(401, "Invalid auth format")
    end

    local decoded = rover.base64.decode(encoded)
    local username, password = decoded:match("^(.+):(.+)$")

    if username ~= "admin" or password ~= rover.env.ADMIN_PASSWORD then
        return api.error(401, "Invalid credentials")
    end

    ctx:set("user", { name = username, role = "admin" })
end

function api.admin.dashboard.get(ctx)
    return api.json { message = "Welcome, admin" }
end

return api
```

### Role-Based Authorization

```lua
local api = rover.server {}

-- Helper to create role-checking middleware
local function require_role(...)
    local allowed = { ... }
    return function(ctx)
        local user = ctx:get("user")
        if not user then
            return api.json:status(401, { error = "Not authenticated" })
        end

        local has_role = false
        for _, role in ipairs(allowed) do
            if user.role == role then
                has_role = true
                break
            end
        end

        if not has_role then
            return api.json:status(403, { error = "Insufficient permissions" })
        end
    end
end

-- Auth middleware for all routes
function api.middleware(ctx)
    local token = ctx:headers().Authorization
    if token then
        local user = decode_token(token:match("^Bearer (.+)$"))
        ctx:set("user", user)
    end
end

-- Only admins
api.admin.middleware = require_role("admin")

-- Admins and moderators
api.mod.middleware = require_role("admin", "moderator")

function api.admin.users.delete(ctx)
    return api.json { deleted = true }
end

function api.mod.posts.delete(ctx)
    return api.json { deleted = true }
end

return api
```

---

## 3. Server-Sent Events (SSE)

**Priority: High** — Essential for real-time features (AI streaming, live feeds, notifications).

```lua
local api = rover.server {}

-- SSE endpoint
function api.events.sse(ctx)
    -- Called once per connection
    -- Return an event emitter

    return function(emit)
        -- emit:send(data) — send a data-only event
        -- emit:event(name, data) — send a named event
        -- emit:comment(text) — send a comment (keep-alive)
        -- emit:close() — close the connection

        emit:event("connected", { status = "ok" })

        -- Example: send time every second
        for i = 1, 10 do
            rover.delay(1000)
            emit:send({ time = os.date(), count = i })
        end

        emit:event("done", { total = 10 })
        emit:close()
    end
end

return api
```

### SSE with Streaming AI Response

```lua
local api = rover.server {}

function api.chat.sse(ctx)
    local body = ctx:body():json()
    local prompt = body.prompt

    return function(emit)
        emit:event("start", { model = "gpt-4" })

        -- Stream from an LLM API
        rover.http.post("https://api.openai.com/v1/chat/completions", {
            model = "gpt-4",
            messages = { { role = "user", content = prompt } },
            stream = true,
        }, {
            on_chunk = function(chunk)
                emit:send({ text = chunk })
            end,
        })

        emit:event("done", {})
        emit:close()
    end
end

return api
```

### SSE with Broadcast (Multiple Clients)

```lua
local api = rover.server {}

local subscribers = {}

-- Clients connect here to receive events
function api.notifications.sse(ctx)
    return function(emit)
        local id = #subscribers + 1
        subscribers[id] = emit

        emit:event("connected", { id = id })

        -- Keep alive until client disconnects
        -- The runtime handles cleanup when the connection drops
    end
end

-- POST to broadcast to all connected clients
function api.notifications.post(ctx)
    local body = ctx:body():json()

    for id, emit in pairs(subscribers) do
        local ok = pcall(emit.send, emit, body)
        if not ok then
            subscribers[id] = nil -- Client disconnected
        end
    end

    return api.json { sent_to = #subscribers }
end

return api
```

---

## 4. WebSockets

**Priority: High** — Already implemented, needs documentation.

```lua
local api = rover.server {}

-- WebSocket endpoint: use `ws` instead of `get`/`post`
function api.chat.ws(ctx, ws)
    function ws.on.open()
        print("Client connected")
    end

    function ws.on.message(msg)
        -- Echo back
        ws.send("echo: " .. msg)
    end

    function ws.on.close()
        print("Client disconnected")
    end
end

return api
```

### WebSocket Chat Room

```lua
local api = rover.server {}

local rooms = {}

function api.rooms.p_room.ws(ctx, ws)
    local room_name = ctx:params().room

    if not rooms[room_name] then
        rooms[room_name] = {}
    end
    local room = rooms[room_name]

    function ws.on.open()
        table.insert(room, ws)
        -- Broadcast join message
        for _, client in ipairs(room) do
            if client ~= ws then
                client.send("Someone joined " .. room_name)
            end
        end
    end

    function ws.on.message(msg)
        -- Broadcast to all in room
        for _, client in ipairs(room) do
            if client ~= ws then
                client.send(msg)
            end
        end
    end

    function ws.on.close()
        for i, client in ipairs(room) do
            if client == ws then
                table.remove(room, i)
                break
            end
        end
    end
end

return api
```

### WebSocket with JSON Messages

```lua
local api = rover.server {}

function api.data.ws(ctx, ws)
    function ws.on.message(msg)
        -- Parse JSON message
        local data = rover.json.decode(msg)

        if data.type == "ping" then
            ws.send(rover.json.encode { type = "pong", time = os.time() })
        elseif data.type == "subscribe" then
            ws.send(rover.json.encode { type = "subscribed", channel = data.channel })
        end
    end
end

return api
```

---

## 5. CORS

**Priority: High** — Required for any API consumed by browsers.

### Server Config

```lua
local api = rover.server {
    cors = {
        origin = "*",                                  -- or { "https://app.com", "https://admin.app.com" }
        methods = { "GET", "POST", "PUT", "DELETE" },  -- default: all
        headers = { "Content-Type", "Authorization" }, -- allowed request headers
        expose = { "X-Request-Id" },                   -- headers the browser can read
        credentials = true,                            -- allow cookies/auth
        max_age = 3600,                                -- preflight cache (seconds)
    },
}

function api.data.get(ctx)
    return api.json { hello = "world" }
end

return api
```

### Per-Route CORS Override

```lua
local api = rover.server {
    cors = { origin = "https://app.com" },
}

-- This route allows any origin (overrides server default)
function api.public.data.get(ctx)
    ctx:set_header("Access-Control-Allow-Origin", "*")
    return api.json { public = true }
end

return api
```

---

## 6. Rate Limiting

**Priority: High** — Prevents abuse and protects resources.

### Server-Level Rate Limit

```lua
local api = rover.server {
    rate_limit = {
        window = 60,    -- seconds
        max = 100,      -- requests per window
        by = "ip",      -- "ip" | "header" | custom function
    },
}

function api.data.get(ctx)
    return api.json { data = "ok" }
end

return api
```

### Route-Scoped Rate Limit via Middleware

```lua
local api = rover.server {}

-- Stricter rate limit on auth endpoints
function api.auth.middleware(ctx)
    local limit = rover.rate_limit({
        window = 300,  -- 5 minutes
        max = 5,       -- 5 attempts
        by = "ip",
        key = "auth",  -- separate bucket from global
    })

    local result = limit:check(ctx)
    if not result.allowed then
        return api.json:status(429, {
            error = "Too many requests",
            retry_after = result.retry_after,
        })
    end

    -- Set rate limit headers
    ctx:set_header("X-RateLimit-Limit", result.limit)
    ctx:set_header("X-RateLimit-Remaining", result.remaining)
    ctx:set_header("X-RateLimit-Reset", result.reset)
end

function api.auth.login.post(ctx)
    local body = ctx:body():json()
    -- Login logic
    return api.json { token = "..." }
end

return api
```

---

## 7. Session Management

**Priority: Medium** — Needed for stateful web apps (not needed for pure API servers).

### Cookie-Based Sessions

```lua
local api = rover.server {
    session = {
        store = "memory",              -- "memory" | "sqlite" | "custom"
        cookie = "rover_session",      -- cookie name
        max_age = 86400,               -- 24 hours
        secure = true,                 -- HTTPS only
        http_only = true,              -- No JS access
        same_site = "lax",             -- "strict" | "lax" | "none"
    },
}

function api.login.post(ctx)
    local body = ctx:body():json()

    -- Validate credentials...
    local user = authenticate(body.email, body.password)
    if not user then
        return api.json:status(401, { error = "Invalid credentials" })
    end

    -- Store in session
    ctx:session():set("user_id", user.id)
    ctx:session():set("role", user.role)

    return api.json { message = "Logged in" }
end

function api.profile.get(ctx)
    local user_id = ctx:session():get("user_id")
    if not user_id then
        return api.json:status(401, { error = "Not logged in" })
    end

    return api.json { user_id = user_id }
end

function api.logout.post(ctx)
    ctx:session():destroy()
    return api.json { message = "Logged out" }
end

return api
```

### Session with SQLite Store

```lua
local api = rover.server {
    session = {
        store = "sqlite",
        path = "sessions.db",  -- defaults to rover_sessions.sqlite
        max_age = 604800,      -- 7 days
    },
}
```

---

## 8. Cookie Handling

**Priority: Medium** — Needed for sessions, preferences, tracking.

### Reading Cookies

```lua
local api = rover.server {}

function api.preferences.get(ctx)
    local theme = ctx:cookie("theme") or "light"
    local lang = ctx:cookie("lang") or "en"

    return api.json { theme = theme, lang = lang }
end

return api
```

### Setting Cookies

```lua
local api = rover.server {}

function api.preferences.post(ctx)
    local body = ctx:body():json()

    ctx:set_cookie("theme", body.theme, {
        max_age = 31536000,   -- 1 year
        path = "/",
        secure = true,
        http_only = false,    -- JS needs to read this
        same_site = "lax",
    })

    return api.json { saved = true }
end

return api
```

### Deleting Cookies

```lua
function api.logout.post(ctx)
    ctx:delete_cookie("session_token")
    ctx:delete_cookie("refresh_token")
    return api.json { message = "Logged out" }
end
```

---

## 9. Static File Serving

**Priority: Medium** — Needed for serving SPAs, assets, uploaded files.

### Serve a Directory

```lua
local api = rover.server {
    static = {
        path = "/public",       -- URL prefix
        dir = "./static",       -- filesystem directory
        index = "index.html",   -- default file for directories
        cache = 3600,           -- Cache-Control max-age (seconds)
    },
}

-- API routes work alongside static files
function api.data.get(ctx)
    return api.json { hello = "world" }
end

return api
```

### Multiple Static Mounts

```lua
local api = rover.server {
    static = {
        { path = "/assets", dir = "./public/assets", cache = 86400 },
        { path = "/uploads", dir = "./uploads", cache = 0 },
    },
}
```

---

## 10. File Uploads / Multipart

**Priority: Medium** — Needed for user-generated content, file management.

### Single File Upload

```lua
local api = rover.server {}

function api.upload.post(ctx)
    local file = ctx:body():file("avatar")

    if not file then
        return api.json:status(400, { error = "No file provided" })
    end

    -- file.name     — original filename
    -- file.size     — size in bytes
    -- file.type     — MIME type
    -- file.bytes    — raw bytes

    local path = "./uploads/" .. file.name
    local f = io.open(path, "wb")
    f:write(file.bytes)
    f:close()

    return api.json:status(201, {
        filename = file.name,
        size = file.size,
        type = file.type,
    })
end

return api
```

### Multiple Files

```lua
function api.gallery.post(ctx)
    local files = ctx:body():files("photos")
    local results = {}

    for _, file in ipairs(files) do
        local path = "./uploads/" .. os.time() .. "_" .. file.name
        local f = io.open(path, "wb")
        f:write(file.bytes)
        f:close()
        table.insert(results, { name = file.name, size = file.size })
    end

    return api.json:status(201, { uploaded = results })
end
```

### Multipart Form Data with Fields and Files

```lua
function api.posts.post(ctx)
    local form = ctx:body():multipart()

    -- form.fields — table of text fields
    -- form.files  — table of file fields

    local title = form.fields.title
    local content = form.fields.content
    local cover = form.files.cover_image  -- single file

    return api.json:status(201, {
        title = title,
        content = content,
        cover_name = cover and cover.name,
    })
end
```

---

## 11. Response Compression

**Priority: Medium** — Reduces bandwidth, improves performance.

### Server Config

```lua
local api = rover.server {
    compress = {
        enabled = true,              -- default: false
        algorithms = { "gzip", "br" }, -- gzip, br (brotli), deflate
        min_size = 1024,             -- don't compress responses < 1KB
        types = {                    -- only compress these MIME types
            "application/json",
            "text/html",
            "text/plain",
            "text/css",
            "application/javascript",
        },
    },
}
```

Compression is transparent — the runtime reads `Accept-Encoding`, picks the
best algorithm, compresses the response, and sets `Content-Encoding`.

---

## 12. Security Headers

**Priority: High** — Required for any web-facing application.

### Server Config

```lua
local api = rover.server {
    security = {
        hsts = true,                          -- Strict-Transport-Security
        no_sniff = true,                      -- X-Content-Type-Options: nosniff
        frame = "deny",                       -- X-Frame-Options: DENY | SAMEORIGIN
        xss = true,                           -- X-XSS-Protection: 1; mode=block
        referrer = "strict-origin",           -- Referrer-Policy
        csp = "default-src 'self'",           -- Content-Security-Policy
    },
}
```

### CSRF Protection

```lua
local api = rover.server {
    csrf = {
        enabled = true,
        cookie = "_csrf",
        header = "X-CSRF-Token",    -- check this header on state-changing methods
        methods = { "POST", "PUT", "PATCH", "DELETE" },
    },
}

-- The runtime auto-generates CSRF tokens in cookies.
-- Clients must read the cookie and send it back as a header.

-- For HTML forms:
function api.form.get(ctx)
    return api.html {} [[
        <form method="POST" action="/submit">
            <input type="hidden" name="_csrf" value="{{ csrf_token }}">
            <button type="submit">Submit</button>
        </form>
    ]]
end
```

---

## 13. Request Logging

**Priority: Medium** — Already partial (basic tracing exists). Needs structured format.

### Server Config

```lua
local api = rover.server {
    log_level = "info",  -- Already exists: "debug" | "info" | "warn" | "error" | "nope"
    log = {
        format = "structured",  -- "pretty" | "structured" (JSON)
        request_id = true,      -- Add X-Request-Id to every request
        include_headers = false, -- Log request headers (careful with auth headers)
        slow_threshold = 1000,  -- Log warning for requests > 1000ms
    },
}
```

### Custom Request Logging via Middleware

```lua
local api = rover.server {}

function api.middleware(ctx)
    local start = os.clock()

    -- After response (using after-middleware pattern)
    ctx:on_response(function(response)
        local duration = (os.clock() - start) * 1000
        print(string.format(
            "%s %s %d %.2fms",
            ctx.method, ctx.path, response.status, duration
        ))
    end)
end
```

---

## 14. Streaming Responses

**Priority: Medium** — Needed for large files, real-time data, AI responses.

### Chunked Transfer

```lua
local api = rover.server {}

function api.download.get(ctx)
    return api.stream(function(write)
        -- write(chunk) sends a chunk to the client
        -- Content-Type auto-detected or set manually

        local file = io.open("large_file.csv", "r")
        for line in file:lines() do
            write(line .. "\n")
        end
        file:close()
    end, {
        content_type = "text/csv",
        headers = {
            ["Content-Disposition"] = 'attachment; filename="data.csv"',
        },
    })
end

return api
```

### Streaming JSON Array

```lua
function api.export.get(ctx)
    return api.stream(function(write)
        write("[")

        local rows = db.users:find():all()
        for i, row in ipairs(rows) do
            if i > 1 then write(",") end
            write(rover.json.encode(row))
        end

        write("]")
    end, { content_type = "application/json" })
end
```

---

## 15. Background Jobs

**Priority: Low** — Nice to have. Most apps use external job queues.

### Simple Async Tasks

```lua
local api = rover.server {}

function api.orders.post(ctx)
    local body = ctx:body():json()

    -- Create order synchronously
    local order = db.orders:insert(body)

    -- Fire-and-forget background work
    rover.spawn(function()
        send_confirmation_email(order)
        notify_warehouse(order)
    end)

    -- Respond immediately
    return api.json:status(201, order)
end

return api
```

### Delayed Tasks

```lua
-- Run after a delay
rover.spawn(function()
    rover.delay(60000) -- Wait 60 seconds
    cleanup_expired_sessions()
end)
```

### Periodic Tasks

```lua
-- Run every 5 minutes
rover.interval(300000, function()
    local expired = db.sessions:find():by_expires_at_smaller_than(os.time()):all()
    for _, session in ipairs(expired) do
        db.sessions:delete():by_id_equals(session.id):run()
    end
end)
```

---

## 16. Environment & Config

**Priority: High** — Every production app needs this.

### Environment Variables

```lua
-- rover.env reads from .env file and system environment
local api = rover.server {
    port = tonumber(rover.env.PORT) or 4242,
}

function api.config.get(ctx)
    return api.json {
        database_url = rover.env.DATABASE_URL,
        -- rover.env.JWT_SECRET — exists but don't expose it!
        environment = rover.env.ROVER_ENV or "development",
    }
end

return api
```

### `.env` File

```
PORT=4242
DATABASE_URL=file:./app.db
JWT_SECRET=your-secret-key
ROVER_ENV=production
```

### Config File (`rover.toml`)

```toml
[server]
port = 4242
host = "0.0.0.0"

[database]
path = "./app.db"

[session]
secret = "${SESSION_SECRET}"  # Interpolate env vars
max_age = 86400
```

```lua
local config = rover.config  -- Reads rover.toml

local api = rover.server {
    port = config.server.port,
    host = config.server.host,
}
```

---

## 17. Graceful Shutdown

**Priority: Medium** — Prevents dropped connections during deploys.

### Server Config

```lua
local api = rover.server {
    shutdown = {
        timeout = 30,  -- Wait up to 30 seconds for in-flight requests
        on_shutdown = function()
            -- Cleanup: close DB connections, flush logs, etc.
            print("Server shutting down...")
            db:close()
        end,
    },
}
```

The runtime handles SIGTERM/SIGINT:
1. Stop accepting new connections
2. Wait for in-flight requests to complete (up to timeout)
3. Call `on_shutdown` callback
4. Exit

---

## 18. Error Handling Middleware

**Priority: High** — Centralized error handling instead of try/catch in every route.

### Global Error Handler

```lua
local api = rover.server {}

-- Catches any error thrown in routes or middleware
function api.on_error(ctx, err)
    -- err.message — error message string
    -- err.status  — HTTP status (if set, otherwise 500)
    -- err.stack   — stack trace (only in development)

    local status = err.status or 500

    if status >= 500 then
        -- Log server errors
        print("ERROR: " .. err.message)
    end

    return api.json:status(status, {
        error = err.message,
        -- Only include stack in development
        stack = rover.env.ROVER_ENV ~= "production" and err.stack or nil,
    })
end

function api.data.get(ctx)
    -- This error gets caught by on_error
    error("Something went wrong")
end

function api.users.p_id.get(ctx)
    local user = db.users:find():by_id_equals(ctx:params().id):one()
    if not user then
        -- Throw with a specific status
        rover.throw(404, "User not found")
    end
    return api.json(user)
end

return api
```

---

## 19. Request ID / Correlation

**Priority: Medium** — Essential for debugging in production.

### Server Config

```lua
local api = rover.server {
    request_id = true,  -- Auto-generate X-Request-Id for every request
}

-- Request IDs are:
-- 1. Generated if not present in the incoming request
-- 2. Passed through if already in X-Request-Id header
-- 3. Available via ctx:request_id()
-- 4. Included in all log output
-- 5. Added to the response headers

function api.data.get(ctx)
    local rid = ctx:request_id()
    -- Use in downstream calls
    local response = rover.http.get("https://api.example.com/data", {
        headers = { ["X-Request-Id"] = rid },
    })
    return api.json(response)
end

return api
```

---

## 20. Body Size Limits

**Priority: High** — Prevents memory exhaustion attacks.

### Server Config

```lua
local api = rover.server {
    max_body_size = "10mb",  -- default: 1mb
}
```

### Per-Route Override via Middleware

```lua
local api = rover.server {
    max_body_size = "1mb",  -- global default
}

-- Upload routes need more
function api.upload.middleware(ctx)
    ctx:set_body_limit("50mb")
end

function api.upload.post(ctx)
    local file = ctx:body():file("document")
    -- Process file up to 50mb
    return api.json { size = file.size }
end

return api
```

When a request exceeds the limit, the runtime returns `413 Payload Too Large`
before the body is fully read.

---

## Implementation Priority

### Phase 1 — Foundation (Must Have)
1. **Middleware System** — Everything else builds on this
2. **CORS** — Can't use the API from browsers without it
3. **Security Headers** — Basic protection
4. **Body Size Limits** — Prevent abuse
5. **Error Handling Middleware** — Centralized error handling
6. **Environment & Config** — Every app needs `rover.env`

### Phase 2 — Production Essentials
7. **Cookie Handling** — Foundation for sessions
8. **Session Management** — Stateful apps
9. **Rate Limiting** — Abuse prevention
10. **Request ID** — Debugging
11. **Request Logging** (enhanced) — Structured logs

### Phase 3 — Real-Time & Data
12. **Server-Sent Events** — AI streaming, live updates
13. **WebSocket docs** — Already implemented
14. **Streaming Responses** — Large payloads
15. **File Uploads / Multipart** — User content

### Phase 4 — Performance & DX
16. **Response Compression** — Bandwidth savings
17. **Static File Serving** — SPAs, assets
18. **Graceful Shutdown** — Zero-downtime deploys
19. **Background Jobs** — Async work
