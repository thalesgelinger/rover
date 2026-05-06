---
weight: 8
title: Sessions and Cookies
aliases:
  - /docs/server/sessions-and-cookies/
  - /docs/security/sessions-and-cookies/
---

Use `rover.cookie` for header-safe cookie strings and `rover.session` for server-side session state.

## Cookie Parsing

```lua
local cookies = rover.cookie.parse("session=abc123; theme=dark")
-- cookies.session == "abc123"
-- cookies.theme == "dark"
```

Parse a `Set-Cookie` value too:

```lua
local parsed = rover.cookie.parse_set_cookie("session=abc123; Path=/; HttpOnly; SameSite=Lax")
```

## Cookie Building

```lua
local set_cookie = rover.cookie
    .set("session", "abc123")
    :path("/")
    :http_only()
    :secure()
    :same_site("lax")
    :build()
```

Delete cookie:

```lua
local expired = rover.cookie.delete("session", { path = "/" })
```

## Session Store

```lua
local sessions = rover.session.new {
    cookie_name = "auth_session",
    ttl = 3600,
    secure = true,
    http_only = true,
    same_site = "lax",
    path = "/",
}
```

Config fields:

- `cookie_name`
- `ttl`
- `secure`
- `http_only`
- `same_site`
- `domain`
- `path`

## Session Lifecycle

```lua
function api.login.post(ctx)
    local session = sessions:create()
    session:set("user_id", "u_123")
    session:set("role", "admin")
    session:save()

    return api.json {
        ok = true,
        session_id = session:id(),
        set_cookie = session:cookie(),
    }
end
```

Useful methods:

- `sessions:create()`
- `sessions:get(id)`
- `sessions:get_or_create(id)`
- `sessions:delete(id)`
- `sessions:exists(id)`

Session handle methods:

- `session:id()`
- `session:get(key)`
- `session:set(key, value)`
- `session:remove(key)`
- `session:has(key)`
- `session:save()`
- `session:destroy()`
- `session:regenerate()`
- `session:cookie()`
- `session:refresh()`
- `session:invalidate()`
- `session:is_expired()`
- `session:is_valid()`
- `session:state()`
- `session:created_at()`
- `session:last_accessed()`
- `session:len()`
- `session:is_empty()`

Stored values support `:as_string()`, `:as_integer()`, and `:as_bool()`.

## Practical Pattern

```lua
function api.me.p_sid.get(ctx)
    local session = sessions:get(ctx:params().sid)
    if not session or not session:is_valid() then
        return api:error(401, "invalid session")
    end

    local user_id = session:get("user_id")

    return api.json {
        user_id = user_id and user_id:as_string(),
        state = session:state(),
    }
end
```

## Recommendations

- regenerate session id after login or privilege change
- keep `secure = true` outside local HTTP dev
- prefer `http_only = true` for auth cookies
- keep cookie path/domain scoped narrowly

## Examples

- `examples/session_demo.lua`
- `examples/foundation_server_capabilities.lua`

## Related

- [Auth](/docs/security/auth/)
- [Configuration](/docs/server/configuration/)
