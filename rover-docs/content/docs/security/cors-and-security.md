---
weight: 6
title: CORS and Security
aliases:
  - /docs/server/cors-and-security/
  - /docs/security/cors-and-security/
---

Rover ships secure defaults. Open them only with explicit config.

## CORS

```lua
local api = rover.server {
    cors_origin = "https://app.example.com",
    cors_methods = "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD",
    cors_headers = "Content-Type, Authorization",
    cors_credentials = true,
}
```

Notes:

- no `cors_origin` means CORS disabled
- strict mode blocks `cors_origin = "*"` with `cors_credentials = true`
- use `allow_wildcard_cors_credentials = true` only for explicit dev/test cases

## Security Headers

```lua
local api = rover.server {
    security_headers = true,
}
```

Default secure headers include:

- `X-Content-Type-Options`
- `X-Frame-Options`
- `Referrer-Policy`

Strict mode expects these to stay enabled.

## Public Bind Safety

```lua
local api = rover.server {
    host = "0.0.0.0",
    allow_public_bind = true,
    https_redirect = true,
}
```

If you want public HTTP without redirect, you must opt out with `allow_insecure_http = true`.

## Body Limits

```lua
local api = rover.server {
    body_size_limit = 1024 * 1024,
}
```

Strict mode expects bounded request bodies. To disable limit, set `body_size_limit = 0` and `allow_unbounded_body = true`.

## Practical Pattern

```lua
local api = rover.server {
    host = "0.0.0.0",
    allow_public_bind = true,
    https_redirect = true,
    body_size_limit = 1024 * 1024,
    security_headers = true,
    cors_origin = "https://app.example.com",
    cors_credentials = true,
}
```

## Examples

- `examples/cors_support.lua`
- `examples/security_headers.lua`
- `examples/body_size_limit.lua`

## Related

- [Configuration](/docs/server/configuration/)
- [Production Deployment](/docs/operations/production-deployment/)
