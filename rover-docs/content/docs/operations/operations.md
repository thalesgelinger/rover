---
weight: 11
title: Operations
aliases:
  - /docs/operations/
  - /docs/operations/operations/
---

Use a few simple contracts for health, readiness, request IDs, and logs.

## Health and Readiness

These are app-defined routes, not magic built-ins.

```lua
function api.healthz.get(ctx)
    return api.json { status = "ok" }
end

function api.readyz.get(ctx)
    return api.json { ready = true }
end
```

Common split:

- `healthz`: process alive
- `readyz`: dependencies ready for traffic

## Request IDs

Each request gets a request id. Rover uses inbound `X-Request-ID` when present, else generates one.

```lua
function api.debug.get(ctx)
    return api.json {
        request_id = ctx:request_id(),
    }
end
```

## Request Logging

Rover emits request logs according to `log_level`.

```lua
local api = rover.server {
    log_level = "info",
}
```

Use middleware if you want app-specific request enrichment.

```lua
function api.before.trace(ctx)
    ctx:set("request_id", ctx:request_id())
end
```

## Management Surface

Keep docs/admin paths isolated:

```lua
local api = rover.server {
    docs = true,
    management_prefix = "/_rover",
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
}
```

## Deploy Checks

- healthz returns success
- readyz reflects dependency state
- request ids visible in logs
- drain timeout tested during rollout
- management token required in non-dev

## Related

- [Middleware](/docs/server/middleware/)
- [Production Deployment](/docs/operations/production-deployment/)
- [Server Lifecycle](/docs/operations/server-lifecycle/)
