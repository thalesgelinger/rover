# Rover
<img width="1500" height="500" alt="image" src="https://github.com/user-attachments/assets/5605ca56-530a-4fe5-a719-dd0f862af3ac" />

Opinionated Lua runtime for building full-stack applications. Write backends, frontends (coming soon), and everything in between using Lua's speed and simplicity.

Rover is an all-in-one tool that maximizes Lua's power - not a framework, but a complete runtime with batteries included.

## Quick Start

Build and run:

```bash
cargo build --release
./target/release/rover your_app.lua
```

## What's Included

- ✅ **Backend Server**: HTTP server with built-in routing
- 🚧 **UI Framework**: Native UI components (comming soon)
- 🔧 **Zero Config**: Opinionated defaults that just work

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

### Status Codes & Headers (wip)

```lua
function api.hello.get(ctx)
    local auth = ctx:headers()["Authorization"]
    
    if not auth then
        return api.json:status(401) {
            error = "Unauthorized"
        }
    end
    
    return api.json:status(200) {
        message = "Authenticated"
    }
end
```

### UI Framework (Coming Soon)

```lua
local app = rover.app()

function app.init()
    return 0
end

function app.increase(state)
    return state + 1
end

function app.render(state)
    return rover.col {
        width = "full",
        height = 100,
        rover.text { "Count: " .. state },
        rover.row {
            rover.button { "Increase", press = "increase" }
        }
    }
end
```

## Performance Testing

Built for speed. Test with [wrk](https://github.com/wg/wrk):

Create `benchmark.lua`:

```lua
wrk.method = "GET"
wrk.path   = "/your/endpoint"
wrk.headers["Content-Type"] = "application/json"
```

Run:

```bash
wrk -s benchmark.lua http://localhost:3000
```

## Configuration

Server options:

```lua
rover.server {
    host = "127.0.0.1",       -- default: localhost
    port = 3000,              -- default: 4242
    log_level = "debug"       -- "debug" | "info" | "warn" | "error" | "nope"
}
```

## Route Patterns

- Static: `api.users.get` → `/users`
- Params: `api.users.p_id.get` → `/users/:id`
- Nested: `api.users.p_id.posts.p_pid.get` → `/users/:id/posts/:pid`

## HTTP Methods

Supported: `get`, `post`, `put`, `patch`, `delete`, `head`, `options`

## Roadmap

- [x] HTTP server with automatic routing
- [x] Context API (params, query, headers, body)
- [ ] WebSocket support
- [ ] Database integrations
- [ ] UI framework with reactive state for mobile/web/desktop
- [ ] Hot reload

