---
sidebar_position: 4
---

# Server Extras

Server behaviors beyond route declaration.

## Management Docs Endpoint

Enable docs UI:

```lua
local api = rover.server {
  docs = true,
  management_prefix = "/_rover",
}
```

Path:

- `/_rover/docs` by default
- `${management_prefix}/docs` if customized

Auth:

- `Authorization: Bearer <management_token>`
- `X-Rover-Management-Token: <management_token>`

If no token and unauthenticated access is disabled, endpoint returns `401`.

## Built-in Probes

Rover always serves:

- `GET/HEAD /healthz` -> liveness (`200 {"status":"ok"}`)
- `GET/HEAD /readyz` -> readiness (`200` or `503` based on lifecycle/dependencies)

Unsupported methods on probe paths return `405` with `Allow: GET, HEAD`.

## Global Error Handler

Set root-level `api.on_error` to customize uncaught handler errors.

```lua
local api = rover.server {}

api.on_error = function(err, ctx)
  return api:error(500, "internal_error")
end

return api
```

## Middleware Hooks (`before` / `after`)

Define middleware at root or route groups.

```lua
local api = rover.server {}

api.before = function(ctx)
  ctx:set("request_start", os.clock())
end

api.after = function(ctx)
  local start = ctx:get("request_start")
  if start then
    local elapsed_ms = (os.clock() - start) * 1000
    print("request ms", elapsed_ms)
  end
end

return api
```

Order:

- `global.before`
- `group.before`
- handler
- `group.after` (reverse)
- `global.after` (reverse)

## Static Mounts

Mount static files under a prefix:

```lua
api.assets.static {
  dir = "public",
  cache = "public, max-age=60",
}
```

- API routes at same prefix win over static catch-all.
- Includes traversal protection + ETag/Last-Modified + conditional `304`.

## Idempotent Route Wrapper

Wrap write handlers:

```lua
api.orders.post = api.idempotent(function(ctx)
  return api.json:status(201, { ok = true })
end)

api.payments.post = api.idempotent({
  header = "X-Payments-Key",
  ttl_ms = 60000,
}, function(ctx)
  return api.json { ok = true }
end)
```

Defaults:

- header: `Idempotency-Key`
- ttl: `300000` ms

## Streaming APIs

`api.stream(status, content_type, producer)`:

```lua
function api.logs.get(ctx)
  local i = 0
  return api.stream(200, "text/plain", function()
    i = i + 1
    if i > 3 then return nil end
    return "line " .. i .. "\n"
  end)
end
```

`api.stream_with_headers(status, content_type, headers, producer)` adds response headers.

## Server-Sent Events

```lua
function api.events.get(ctx)
  local i = 0
  return api.sse(function(writer)
    i = i + 1
    if i > 3 then return nil end
    return {
      event = "tick",
      data = { index = i },
    }
  end, 1500)
end
```

Also available:

- `api.sse:status(status, producer, retry_ms?)`
- `api.sse:with_headers(status, headers, producer, retry_ms?)`
