---
weight: 1
title: Backend Server
aliases:
  - /docs/server/backend-server/
  - /docs/server/backend-server/
---

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

## Next Steps

- [Context API](/docs/server/context-api/) - Access request data
- [Response Builders](/docs/server/response-builders/) - Return structured responses
- [Middleware](/docs/server/middleware/) - Before/after pipeline, route guards, request state
- [Auth](/docs/security/auth/) - JWT, sessions, management auth defaults
- [Sessions and Cookies](/docs/security/sessions-and-cookies/) - Cookie builder, parsing, session lifecycle
- [CORS and Security](/docs/security/cors-and-security/) - Browser policy and secure defaults
- [Streaming](/docs/http-and-realtime/streaming/) - Chunked responses, SSE, lifecycle notes
- [Uploads and Static Assets](/docs/http-and-realtime/uploads-and-static-assets/) - Multipart body helpers and static asset behavior
- [Response Optimization](/docs/http-and-realtime/response-optimization/) - Compression, validators, cache semantics
- [OpenAPI and Versioning](/docs/http-and-realtime/openapi-and-versioning/) - Generated docs and versioned routes
- [Operations](/docs/operations/operations/) - Health, readiness, request IDs, logs
- [Configuration](/docs/server/configuration/) - Strict mode, TLS, CORS, limits
- [WebSocket Server](/docs/http-and-realtime/ws-server/) - Realtime endpoint DSL
- [Server Lifecycle](/docs/operations/server-lifecycle/) - Drain and TLS reload behavior
- [Production Deployment](/docs/operations/production-deployment/) - Proxy, topology, rollout checks
