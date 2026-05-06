---
weight: 4
title: Server Extras
aliases:
  - /docs/server/server-extras/
  - /docs/server/server-extras/
---

Extra server behaviors beyond core routing.

## OpenAPI Docs

When `docs = true` in `rover.server`, Rover generates OpenAPI and serves docs UI from the management namespace.

```lua
local api = rover.server {
  docs = true,
  management_prefix = "/_rover",
  management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
}
```

By default that UI is available at `/_rover/docs` and requires `Authorization: Bearer <token>` unless you explicitly disable management auth.

## Versioned Routes

Versioning is path-based. Define route namespaces like `api.v1` and `api.v2`.

```lua
local api = rover.server { docs = true }

function api.v1.users.get(ctx)
  return api.json { version = "v1", users = {} }
end

function api.v2.users.get(ctx)
  return api.json { version = "v2", users = {}, meta = {} }
end
```

Generated OpenAPI includes both `/v1/...` and `/v2/...` paths.

## Management Isolation

Relevant config knobs:

- `management_prefix`
- `management_token`
- `allow_unauthenticated_management`

Use an isolated namespace and keep docs/admin surfaces off the public app path.

## Raw Responses

Use `api.raw` for raw bytes without automatic content-type:

```lua
function api.rawdata.get(ctx)
  return api.raw("raw")
end

function api.rawdata.post(ctx)
  return api.raw:status(201, "created")
end
```

## Related

- [Configuration](/docs/server/configuration/)
- [OpenAPI and Versioning](/docs/http-and-realtime/openapi-and-versioning/)
- [Auth](/docs/security/auth/)
