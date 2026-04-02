---
weight: 10
title: Idempotency Keys
aliases:
  - /docs/http-and-realtime/idempotency-keys/
  - /docs/server/idempotency-keys/
---

Make write endpoints safely retryable without duplicate side effects.

## Basic Usage

Wrap a handler with `api.idempotent`:

```lua
local api = rover.server {}

api.orders.post = api.idempotent(function(ctx)
    local body = ctx:body():expect {
        product_id = rover.guard:string():required(),
        quantity = rover.guard:integer():required(),
    }

    local order = create_order(body)
    return api.json:status(201, order)
end)

return api
```

Default header: `Idempotency-Key`

## Route-Level Configuration

Custom header name:

```lua
api.orders.post = api.idempotent({ header = "X-Orders-Key" }, function(ctx)
    return api.json { ok = true }
end)
```

Custom TTL (default: `300000` ms):

```lua
api.orders.post = api.idempotent({ ttl_ms = 60000 }, function(ctx)
    return api.json { ok = true }
end)
```

## Replay and Conflict Semantics

Rover fingerprints requests by:

- HTTP method
- route identity
- request body

Behavior:

1. First request with key: executes handler, stores response.
2. Same key + same fingerprint: returns stored response.
3. Same key + different fingerprint: returns `409 Conflict`.

Conflict body:

```json
{"error":"Idempotency key already used with different payload"}
```

## Storage Model

Current built-in store is in-memory.

- good for local dev/test
- good for single-instance deployments
- not shared across instances
- entries are lost on process restart

For multi-instance production, use a durable uniqueness boundary in your data layer (for example, unique key constraints in DB workflow) until shared backend wiring is available.

## Runnable Example

- `examples/idempotent_requests.lua`

Run:

```bash
cargo run -p rover_cli -- run examples/idempotent_requests.lua
```

## Related

- [Middleware](/docs/server/middleware/)
- [Response Optimization](/docs/http-and-realtime/response-optimization/)
- [Operations](/docs/operations/operations/)
