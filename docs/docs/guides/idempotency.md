---
sidebar_position: 13
---

# Idempotency Keys

Make write operations safely retryable without duplicating side effects.

## Overview

Idempotency keys allow clients to retry failed requests safely. When a client sends the same idempotency key with an identical request, Roverreplays the original response instead of executing the handler again. This prevents duplicate charges, duplicate orders, and other harmful side effects from networkretries.

## Basic Usage

Wrap any route handler with `api.idempotent`:

```lua
local api = rover.server {}

api.orders.post = api.idempotent(function(ctx)
    local body = ctx:body():expect {
        product_id = rover.guard:string():required(),
        quantity = rover.guard:integer():required(),
    }

    -- Create order (only executes once per idempotency key)
    local order = create_order(body)

    return api.json:status(201, order)
end)

return api
```

Clients include the `Idempotency-Key` header:

```bash
curl -X POST http://localhost:8080/orders \
  -H "Idempotency-Key: order-123" \
  -H "Content-Type: application/json" \
  -d '{"product_id": "prod-001", "quantity": 2}'
```

## Replay Semantics

When a request includes an `Idempotency-Key` header:

1. **First request**: Executes the handler, stores the response, returns it to the client.
2. **Subsequent requests with same key and same fingerprint**: Returns the stored response without executing the handler.
3. **Request with same key but different fingerprint**: Returns `409 Conflict` with an error message.

### Fingerprint Calculation

Rover creates a fingerprint from:

- HTTP method (POST, PUT, PATCH, etc.)
- Route identity
- Request body

This ensures that changing the request body with the same idempotency key results in a conflict, protecting against accidental misuse.

### Key Scope

Idempotency keys are scoped to the route. The same key can be used safely across different routes:

```lua
api.orders.post = api.idempotent(function(ctx)
    -- "order-123" key here is independent from payments
    ...
end)

api.payments.post = api.idempotent(function(ctx)
    -- "order-123" key here won't conflict with orders
    ...
end)
```

## Custom Header

Use a custom header name per route:

```lua
api.orders.post = api.idempotent({ header = "X-Orders-Key" }, function(ctx)
    -- ...
end)

api.payments.post = api.idempotent({ header = "X-Payments-Key" }, function(ctx)
    -- ...
end)
```

```bash
curl -X POST http://localhost:8080/orders \
  -H "X-Orders-Key: order-123" \
  -d '{"product_id": "prod-001"}'
```

## TTL Configuration

By default, idempotency entries expire after 5 minutes (300,000 ms). Configure a custom TTL:

```lua
api.orders.post = api.idempotent({ ttl_ms = 60000 }, function(ctx)
    -- Entry expires after 60 seconds
    ...
end)
```

## Conflict Response

When a client reuses an idempotency key with a different request body, Rover returns:

```json
{
  "error": "Idempotency key already used with different payload"
}
```

Status code: `409 Conflict`

This protects against accidental parameter changes on retry.

## Storage

### Development Mode (Default)

By default, Rover uses an in-memory store for idempotency entries. This is suitable for:

- Development
- Testing
- Single-instance deployments

**Limitations**:
- Entries are lost on server restart
- Not shared across multiple server instances
- Fine for MVP and single-instance production deployments

### Production Considerations

For multi-instance production deployments, you'll need shared storage. This is planned for a future release. Currently, if you need shared storage, consider:

1. Running a single instance (acceptable for many workloads)
2. Implementing idempotency at the database layer with unique constraints

## Example: E-commerce Order

```lua
local api = rover.server {}
local db = rover.db.connect { path = "orders.sqlite" }

-- Migration (run once)
-- db:migrate([[
--     CREATE TABLE IF NOT EXISTS orders (
--         id INTEGER PRIMARY KEY,
--         product_id TEXT NOT NULL,
--         quantity INTEGER NOT NULL,
--         idempotency_key TEXT UNIQUE,
--         created_at TEXT DEFAULT CURRENT_TIMESTAMP
--     )
-- ]])

api.orders.post = api.idempotent({ ttl_ms = 300000 }, function(ctx)
    local body = ctx:body():expect {
        product_id = rover.guard:string():required(),
        quantity = rover.guard:integer():min(1):required(),
    }

    -- Check for existing order with this idempotency key
    local idempotency_key = ctx:headers()["Idempotency-Key"]
    if idempotency_key then
        local existing = db.orders:find():where({ idempotency_key = idempotency_key }):first()
        if existing then
            return api.json:status(200, existing)
        end
    end

    -- Create new order
    local order = db.orders:insert({
        product_id = body.product_id,
        quantity = body.quantity,
        idempotency_key = idempotency_key,
    })

    return api.json:status(201, order)
end)

return api
```

## Best Practices

1. **Client-generated keys**: Clients should generate UUID v4 keys. Don't accept server-generated keys for idempotency.

2. **POST and PUT only**: Apply idempotency to write operations. GET, HEAD, OPTIONS are already idempotent by HTTP semantics.

3. **Database constraints**: For critical operations like payments, add a unique constraint on the idempotency key column as a defense-in-depth measure.

4. **Key length**: Keep idempotency keys under 255 characters for database compatibility.

5. **Timeout handling**: If a request times out, the client should retry with the same idempotency key to avoid duplicate operations.

6. **Document expected behavior**: Let your API consumers know that idempotency keys are supported and how long entries are retained.

## Testing Idempotency

```lua
-- test.lua
local api = rover.server {}
local counter = 0

api.orders.post = api.idempotent(function(ctx)
    counter = counter + 1
    return { order_id = counter }
end)

return api
```

```bash
# First request
curl -X POST http://localhost:8080/orders \
  -H "Idempotency-Key: test-1" \
  -H "Content-Type: application/json" \
  -d '{}'
# Response: {"order_id":1}

# Same key, same body - returns cached response
curl -X POST http://localhost:8080/orders \
  -H "Idempotency-Key: test-1" \
  -H "Content-Type: application/json" \
  -d '{}'
# Response: {"order_id":1}  (counter didn't increment)

# Different key - executes handler again
curl -X POST http://localhost:8080/orders \
  -H "Idempotency-Key: test-2" \
  -H "Content-Type: application/json" \
  -d '{}'
# Response: {"order_id":2}

# Same key, different body - conflict
curl -X POST http://localhost:8080/orders \
  -H "Idempotency-Key: test-1" \
  -H "Content-Type: application/json" \
  -d '{"different":"body"}'
# Response: 409 Conflict {"error":"Idempotency key already used with different payload"}
```