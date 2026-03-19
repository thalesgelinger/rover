---
weight: 10
title: OpenAPI and Versioning
aliases:
  - /docs/server/openapi-and-versioning/
  - /docs/http-and-realtime/openapi-and-versioning/
---

Rover can generate OpenAPI docs from route definitions and serve them from the management namespace.

## Enable Docs

```lua
local api = rover.server {
    docs = true,
    management_prefix = "/_rover",
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
}
```

Docs UI then lives at `/_rover/docs` by default.

## Auth Model

Management docs are auth-protected by default.

```bash
curl -H "Authorization: Bearer $ROVER_MANAGEMENT_TOKEN" http://localhost:4242/_rover/docs
```

## Versioned Routes

Define versions in the route tree:

```lua
function api.v1.users.get(ctx)
    return api.json { version = "v1", users = {} }
end

function api.v2.users.get(ctx)
    return api.json { version = "v2", users = {}, meta = {} }
end
```

That yields:

- `/v1/users`
- `/v2/users`

Versioned routes can still use params and nested namespaces.

## Mixed Versioned + Unversioned APIs

```lua
function api.health.get(ctx)
    return api.json { status = "ok" }
end

function api.v1.users.p_id.get(ctx)
    return api.json { version = "v1", id = ctx:params().id }
end
```

This keeps operational endpoints unversioned while product APIs evolve by path.

## Recommendations

- keep health/readiness unversioned
- version product APIs by top-level namespace
- protect docs with management token outside local dev
- use docs UI as generated reference, not hand-maintained source of truth

## Example

- `examples/foundation_server_capabilities.lua`

## Related

- [Server Extras](/docs/server/server-extras/)
- [Configuration](/docs/server/configuration/)
- [Production Deployment](/docs/operations/production-deployment/)
