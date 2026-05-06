---
sidebar_position: 12
---

# Auth, Cookie, Session

Runtime modules exposed as `rover.auth`, `rover.cookie`, and `rover.session`.

## rover.auth

JWT helpers.

### Create Token

```lua
local token = rover.auth.create({
  sub = "user_123",
  exp = os.time() + 3600,
  role = "admin",
}, "super-secret")
```

### Verify Token

```lua
local verified = rover.auth.verify(token, "super-secret")
if verified.valid then
  print(verified.sub)
else
  print(verified.error)
end
```

### Decode Without Verify

```lua
local decoded = rover.auth.decode(token)
```

### Require Middleware

```lua
api.before.auth = rover.auth.require("super-secret")
```

### Key Rotation Helpers

```lua
local secrets = rover.auth.secrets({
  signing = {
    active = "k1",
    keys = { k1 = "secret-1" },
  },
})

rover.auth.rotate(secrets, "signing", "k2", "secret-2")
local active = rover.auth.active(secrets, "signing")
```

## rover.cookie

Cookie parse/build helpers.

### Parse Cookie Header

```lua
local cookies = rover.cookie.parse(ctx:headers().cookie or "")
local sid = cookies.session
```

### Parse Set-Cookie

```lua
local parsed = rover.cookie.parse_set_cookie("session=abc; Path=/; HttpOnly")
```

### Build Set-Cookie

```lua
local set_cookie = rover.cookie
  .set("session", "abc")
  :path("/")
  :http_only()
  :secure()
  :same_site("lax")
  :build()
```

### Delete Cookie

```lua
local set_cookie_delete = rover.cookie.delete("session", { path = "/" })
```

## rover.session

Session store + session objects.

### Create Store

```lua
local store = rover.session.new({
  cookie_name = "rover_session",
  ttl = 3600,
  secure = true,
  http_only = true,
  same_site = "lax", -- strict|lax|none
  path = "/",
})
```

Store methods:

- `store:create()`
- `store:get_or_create(session_id?)`
- `store:get(session_id)`
- `store:delete(session_id)`
- `store:exists(session_id)`
- `store:cookie_name()`

### Session Methods

From `local session = store:create()`:

- `session:id()`
- `session:get(key)` / `session:set(key, value)` / `session:remove(key)` / `session:has(key)`
- `session:save()` / `session:destroy()` / `session:regenerate()`
- `session:cookie()`
- `session:created_at()` / `session:last_accessed()`
- `session:len()` / `session:is_empty()`
- `session:is_expired()` / `session:is_valid()` / `session:state()`
- `session:refresh()` / `session:invalidate()`

Value types for `session:set`: string, integer/number, boolean.

### Session Middleware

```lua
local store = rover.session.new()
api.before.session = rover.session.middleware(store)

function api.me.get(ctx)
  local session = ctx:get("session")
  return api.json {
    session_id = session:id(),
  }
end
```
