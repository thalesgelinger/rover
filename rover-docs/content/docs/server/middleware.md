---
weight: 4
title: Middleware
aliases:
  - /docs/server/middleware/
  - /docs/server/middleware/
---

Compose request behavior with named `before` and `after` handlers on the same `api` tree.

## Global Middleware

```lua
local api = rover.server {}

function api.before.log(ctx)
    ctx:set("started_at", os.time())
end

function api.after.log(ctx)
    local started_at = ctx:get("started_at")
    if started_at then
        print("request done in", os.time() - started_at)
    end
end
```

Global middleware applies to all routes.

## Route-Scoped Middleware

```lua
function api.admin.before.auth(ctx)
    local auth = ctx:headers()["Authorization"]
    if not auth then
        return api:error(401, "Unauthorized")
    end
    ctx:set("role", "admin")
end

function api.admin.get(ctx)
    return api.json {
        ok = true,
        role = ctx:get("role"),
    }
end
```

`api.admin.before.auth` applies to `/admin` route namespace only.

## Common Pattern

Use middleware for:

- authn/authz checks
- request logging
- request correlation via `ctx:request_id()`
- shared request state via `ctx:set` and `ctx:get`
- early deny / early return

## Ordering

- `before` middleware runs before route handler
- route handler runs next
- `after` middleware runs after handler
- request state stays available through the request lifecycle

Keep middleware small, deterministic, and side-effect-light.

## Best Practice

```lua
function api.before.trace(ctx)
    ctx:set("request_id", ctx:request_id())
end

function api.profile.before.auth(ctx)
    local token = ctx:headers()["Authorization"]
    if not token then
        return api:error(401, "missing auth")
    end
end

function api.profile.get(ctx)
    return api.json {
        request_id = ctx:get("request_id"),
    }
end
```

## Example

- `examples/middleware_test.lua`

## Related

- [Context API](/docs/server/context-api/)
- [Auth](/docs/security/auth/)
- [Configuration](/docs/server/configuration/)
