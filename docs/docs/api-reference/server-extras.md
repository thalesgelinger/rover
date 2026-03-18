---
sidebar_position: 4
---

# Server Extras

Extra server behaviors beyond core routing.

## OpenAPI Docs

When `docs = true` in `rover.server`, Rover generates OpenAPI and serves docs at `/docs`.

```lua
local api = rover.server { docs = true }
```

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
