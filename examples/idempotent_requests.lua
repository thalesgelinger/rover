-- Idempotent Requests Example
-- Run with: rover run examples/idempotent_requests.lua
-- Test with: curl -X POST http://localhost:8080/orders -H "Idempotency-Key: order-123" -H "Content-Type: application/json" -d '{"product_id":"prod-001","quantity":2}'
-- Retry with same key: Response is replayed, order counter doesn't increase-- Different key: New order is created
-- Same key, different body: Returns 409 Conflict

local api = rover.server {
    idempotency = {
        backend = "sqlite",
        sqlite_path = "./data/idempotency.db",
    },
}
local counter = 0

-- Basic idempotent endpoint
api.orders.post = api.idempotent(function(ctx)
    counter = counter + 1
    local body = ctx:body():expect {
        product_id = rover.guard:string():required(),
        quantity = rover.guard:integer():min(1):required(),
    }

    return api.json:status(201, {
        order_id = counter,
        product_id = body.product_id,
        quantity = body.quantity,
        status = "created"
    })
end)

-- Custom idempotency header
api.payments.post = api.idempotent({ header = "X-Payment-Id" }, function(ctx)
    counter = counter + 1
    local body = ctx:body():expect {
        amount = rover.guard:number():min(0.01):required(),
        currency = rover.guard:string():required(),
    }

    return api.json:status(201, {
        payment_id = counter,
        amount = body.amount,
        currency = body.currency,
        status = "processed"
    })
end)

-- Short TTL for quick expiration
api.sessions.post = api.idempotent({ ttl_ms = 5000 }, function(ctx)
    counter = counter + 1

    return api.json:status(201, {
        session_id = counter,
        created_at = os.date("!%Y-%m-%dT%H:%M:%SZ")
    })
end)

return api
