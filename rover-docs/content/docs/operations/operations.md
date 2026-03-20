---
weight: 11
title: Operations
aliases:
  - /docs/operations/
  - /docs/operations/operations/
---

Use a few simple contracts for health, readiness, request IDs, and logs.

## Health and Readiness

Rover exposes built-in probe routes:

- `GET`/`HEAD /healthz`: liveness probe
- `GET`/`HEAD /readyz`: readiness probe

Contract and status codes:

- `200 /healthz` -> `{ "status": "ok" }`
- `200 /readyz` -> `{ "status": "ready" }`
- `503 /readyz` (draining or dependency outage) -> `{ "status": "not_ready" }`
- `503 /readyz` with dependency failures -> `{ "status": "not_ready", "reasons": [{ "code": "dependency_unavailable", "dependency": "<name>" }] }`
- `405 /healthz` and `405 /readyz` for non-`GET`/`HEAD` methods (with `Allow: GET, HEAD`)

Readiness dependency state comes from server config:

```lua
local api = rover.server {
    readiness = {
        dependencies = {
            database = true,
            redis = true,
        },
    },
}
```

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
