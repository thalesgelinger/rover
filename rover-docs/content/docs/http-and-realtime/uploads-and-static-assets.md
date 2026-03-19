---
weight: 8
title: Uploads and Static Assets
aliases:
  - /docs/server/uploads-and-static-assets/
  - /docs/http-and-realtime/uploads-and-static-assets/
---

Foundation includes multipart parsing primitives and safe static asset serving behavior.

## Multipart Uploads

Use `ctx:body()` multipart helpers when request `Content-Type` is `multipart/form-data`.

```lua
function api.uploads.post(ctx)
    local form = ctx:body():form()
    local avatar = ctx:body():file("avatar")

    if not avatar then
        return api:error(400, "avatar required")
    end

    return api.json {
        username = form.username,
        file_name = avatar.name,
        file_size = avatar.size,
        file_type = avatar.type,
    }
end
```

Available helpers:

- `ctx:body():form()`
- `ctx:body():file(name)`
- `ctx:body():files(name)`
- `ctx:body():multipart()`

## Multipart Shape

```lua
local all = ctx:body():multipart()

-- all.fields.<name>
-- all.files.<field>[1].name
-- all.files.<field>[1].size
-- all.files.<field>[1].type
-- all.files.<field>[1].data
```

## Static Asset Serving

Foundation static file support includes:

- path traversal protection
- cache headers
- `ETag` and `Last-Modified`
- conditional `304` handling
- coexistence with API routes without ambiguity

Current runtime docs in this site focus on the observable behavior and safety guarantees. If you are exposing static assets behind a proxy, preserve cache validators and avoid rewriting asset paths in ways that bypass normalization.

## Cache Behavior

Static responses include standard cache metadata so clients and proxies can revalidate efficiently.

## Example

- Multipart/session examples: `examples/session_demo.lua`, `examples/foundation_server_capabilities.lua`

## Related

- [Context API](/docs/server/context-api/)
- [Configuration](/docs/server/configuration/)
- [Production Deployment](/docs/operations/production-deployment/)
