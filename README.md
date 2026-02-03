# Rover
<img width="1500" height="500" alt="image" src="https://github.com/user-attachments/assets/5605ca56-530a-4fe5-a719-dd0f862af3ac" />

Opinionated Lua runtime for building REAL full-stack applications. Write backends, mobile apps, desktop apps, web frontends - everything - using Lua's speed and simplicity.

Rover is an all-in-one tool that maximizes Lua's power across all platforms - not a framework, but a complete runtime with batteries included for web, mobile, and desktop.

## Quick Start

Build and run:

```bash
cargo build --release
./target/release/rover your_app.lua
```

## What's Included

- ✅ **Backend Server**: HTTP server with built-in routing
- 🚧 **UI Framework**: Native UI components for mobile, desktop, and web (coming soon)
- 🔧 **Zero Config**: Opinionated defaults that just work
- 🌍 **Cross-Platform**: One codebase for web, mobile (iOS/Android), and desktop

## Examples

### Backend Server

```lua
local api = rover.server { }

function api.hello.get(ctx)
    return { message = "Hello World" }
end

-- Path params: p_<name>
function api.users.p_id.get(ctx)
    return { 
        user_id = ctx:params().id 
    }
end

return api
```

### Context Methods

Access request data through the context object:

```lua
function api.echo.get(ctx)
    return {
        method = ctx.method,
        path = ctx.path,
        headers = ctx:headers()["user-agent"],
        query = ctx:query().page
    }
end

function api.echo.post(ctx)
    return {
        body = ctx:body(),
        content_type = ctx:headers()["content-type"]
    }
end
```

### Multiple Route Params

```lua
function api.users.p_id.posts.p_postId.get(ctx)
    local params = ctx:params()
    return {
        user = params.id,
        post = params.postId
    }
end
```

### Response Builders

Rover provides ergonomic response builders with optimal performance:

```lua
-- JSON responses
function api.users.get(ctx)
    return api.json { users = {...} }                    -- 200 OK
    return api.json:status(201, { id = 123 })            -- 201 Created
end

-- Text responses
function api.health.get(ctx)
    return api.text("OK")                                -- 200 text/plain
    return api.text:status(503, "Service Unavailable")  -- 503
end

-- HTML responses
function api.home.get(ctx)
    return api.html("<h1>Welcome</h1>")                  -- 200 text/html
    return api.html:status(404, "<h1>Not Found</h1>")   -- 404
end

-- Redirects
function api.old.get(ctx)
    return api.redirect("/new")                          -- 302 Found
    return api.redirect:permanent("/new-url")            -- 301 Moved Permanently
    return api.redirect:status(307, "/temporary")        -- 307 Temporary
end

-- Error responses
function api.protected.get(ctx)
    local auth = ctx:headers()["Authorization"]
    if not auth then
        return api.error(401, "Unauthorized")            -- 401 { error: "..." }
    end
    return api.json { data = "secret" }
end

-- No content
function api.items.p_id.delete(ctx)
    -- delete item...
    return api.no_content()                              -- 204 No Content
end

-- Fast path: plain tables (automatic JSON)
function api.simple.get(ctx)
    return { message = "Hello" }                         -- 200 application/json
end
```

**Performance**: All builders use pre-serialization for near-zero overhead (~182k req/s)

### UI Runtime (Experimental)

```lua
local ru = rover.ui

function rover.render()
    local count = rover.signal(0)

    return ru.column {
        ru.text { "Count: " .. count },
        ru.button {
            label = "Increase",
            on_click = function()
                count.val = count.val + 1
            end,
        },
    }
end
```

## Performance

Built for speed with zero-copy response handling:

```
Requests/sec:   182,000
Latency (avg):  0.49ms
Latency (p99):  0.67ms
```

Test with [wrk](https://github.com/wg/wrk):

```bash
# Run built-in perf test
./target/release/rover tests/perf/main.lua &
cd tests/perf && bash test.sh

# Or create custom benchmark
wrk -t4 -c100 -d30s http://localhost:3000/endpoint
```

## Configuration

Server options:

```lua
rover.server {
    host = "127.0.0.1",       -- default: localhost
    port = 3000,              -- default: 4242
    log_level = "debug",     -- default: debug
    docs = true               -- default: true
}
```

## Route Patterns

- Static: `api.users.get` → `/users`
- Params: `api.users.p_id.get` → `/users/:id`
- Nested: `api.users.p_id.posts.p_pid.get` → `/users/:id/posts/:pid`

## HTTP Methods

Supported: `get`, `post`, `put`, `patch`, `delete`

## Roadmap

See `ROADMAP.md`.

