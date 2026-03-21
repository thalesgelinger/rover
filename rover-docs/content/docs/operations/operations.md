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

### Response Contracts

#### Liveness Probe (`/healthz`)

| Status | Body | Meaning |
|--------|------|---------|
| `200` | `{ "status": "ok" }` | Server is alive and running |
| `405` | (with `Allow: GET, HEAD`) | Method not allowed for non-GET/HEAD requests |

#### Readiness Probe (`/readyz`)

| Status | Body | State | Meaning |
|--------|------|-------|---------|
| `200` | `{ "status": "ready" }` | Healthy | Ready to accept connections, all dependencies healthy |
| `503` | `{ "status": "not_ready" }` | Degraded | Draining/shutting down, not accepting new connections |
| `503` | `{ "status": "not_ready", "reasons": [...] }` | DependencyFailure | Dependencies unavailable, see structured reasons |
| `405` | (with `Allow: GET, HEAD`) | - | Method not allowed for non-GET/HEAD requests |

### Dependency Failure Response Structure

When one or more dependencies are unhealthy, the readiness probe returns a structured response with detailed failure reasons:

**Single Dependency Failure:**

```json
{
  "status": "not_ready",
  "reasons": [
    {
      "code": "dependency_unavailable",
      "dependency": "database"
    }
  ]
}
```

**Multiple Dependency Failures:**

```json
{
  "status": "not_ready",
  "reasons": [
    {
      "code": "dependency_unavailable",
      "dependency": "database"
    },
    {
      "code": "dependency_unavailable",
      "dependency": "redis"
    },
    {
      "code": "dependency_unavailable",
      "dependency": "payment_gateway"
    }
  ]
}
```

**Response Schema:**

| Field | Type | Description |
|-------|------|-------------|
| `status` | string | Always `"not_ready"` for dependency failures |
| `reasons` | array | List of failure reason objects |
| `reasons[].code` | string | Error code: `"dependency_unavailable"` |
| `reasons[].dependency` | string | Name of the failed dependency (from config) |

### Configuring Dependencies

Readiness dependency state comes from server config:

```lua
local api = rover.server {
    readiness = {
        dependencies = {
            database = true,
            redis = true,
            payment_gateway = true,
        },
    },
}
```

Each dependency is a boolean flag:
- `true` - dependency is considered healthy
- `false` - dependency is considered failed

### Operational Usage

**Kubernetes Example:**

```yaml
apiVersion: v1
kind: Pod
spec:
  containers:
    - name: app
      readinessProbe:
        httpGet:
          path: /readyz
          port: 3000
        initialDelaySeconds: 5
        periodSeconds: 10
        failureThreshold: 3
      livenessProbe:
        httpGet:
          path: /healthz
          port: 3000
        initialDelaySeconds: 10
        periodSeconds: 15
```

**Load Balancer Health Check:**

```bash
# Check liveness
curl -f http://localhost:3000/healthz || exit 1

# Check readiness with dependency state
curl -s http://localhost:3000/readyz | jq '.status'

# Full readiness response with reasons
curl -s http://localhost:3000/readyz | jq '.'
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
