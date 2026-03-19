---
weight: 5
title: Auth
aliases:
  - /docs/server/auth/
  - /docs/security/auth/
---

Rover keeps auth explicit: build guards with middleware, then use JWT or sessions as needed.

## Bearer Middleware

```lua
local api = rover.server {}

function api.protected.before.auth(ctx)
    local header = ctx:headers()["Authorization"]
    if not header then
        return api:error(401, "Missing Authorization header")
    end

    local token = header:match("Bearer%s+(.+)")
    if not token then
        return api:error(401, "Invalid Authorization format")
    end

    ctx:set("token", token)
end
```

## JWT With `rover.auth`

```lua
local SECRET = rover.env.JWT_SECRET or "dev-secret"

function api.login.post(ctx)
    local body = ctx:body():json() or {}
    local token = rover.auth.create({
        sub = body.user_id,
        role = body.role or "user",
        iat = os.time(),
        exp = os.time() + 3600,
    }, SECRET)

    return api.json { token = token }
end

function api.protected.before.jwt(ctx)
    local header = ctx:headers()["Authorization"]
    local token = header and header:match("Bearer%s+(.+)")
    if not token then
        return api:error(401, "Missing bearer token")
    end

    local result = rover.auth.verify(token, SECRET)
    if not result.valid then
        return api:error(401, "Invalid token")
    end

    ctx:set("user", { id = result.sub, role = result.role })
end
```

Helpers:

- `rover.auth.create(claims, secret)`
- `rover.auth.verify(token, secret)`
- `rover.auth.decode(token)`

## Session-Backed Auth

```lua
local sessions = rover.session.new {
    cookie_name = "auth_session",
    ttl = 3600,
    http_only = true,
    same_site = "lax",
}

function api.auth.login.post(ctx)
    local session = sessions:create()
    session:set("user_id", "u_123")
    session:save()

    return api.json {
        ok = true,
        cookie = session:cookie(),
    }
end
```

Use sessions when browser cookie flows fit better than bearer tokens.

## Management Auth Defaults

Management endpoints are isolated and auth-by-default.

```lua
local api = rover.server {
    docs = true,
    management_prefix = "/_rover",
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
    allow_unauthenticated_management = false,
}
```

Call protected management routes with:

```bash
curl -H "Authorization: Bearer $ROVER_MANAGEMENT_TOKEN" http://localhost:4242/_rover/docs
```

## Recommendations

- keep auth in `before` middleware
- store parsed auth state in `ctx:set`
- prefer env-backed secrets
- keep management token separate from app auth tokens

## Examples

- `examples/jwt_auth.lua`
- `examples/foundation_server_capabilities.lua`

## Related

- [Middleware](/docs/server/middleware/)
- [Configuration](/docs/server/configuration/)
- [Production Deployment](/docs/operations/production-deployment/)
