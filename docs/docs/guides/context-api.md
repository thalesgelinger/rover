---
sidebar_position: 2
---

# Context API

`ctx` gives request data + request-scoped state.

## Fields

- `ctx.method`
- `ctx.path`
- `ctx.client_ip` (`ctx.ip` alias)
- `ctx.client_proto` (`ctx.proto` alias)

```lua
function api.echo.get(ctx)
  return {
    method = ctx.method,
    path = ctx.path,
    ip = ctx.client_ip,
    proto = ctx.client_proto,
  }
end
```

## Request Accessors

- `ctx:headers()` -> table
- `ctx:query()` -> table
- `ctx:params()` -> table
- `ctx:body()` -> `BodyValue` (errors if no body)
- `ctx:request_id()` -> unique request id string

```lua
function api.users.p_id.get(ctx)
  local headers = ctx:headers()
  local query = ctx:query()
  local params = ctx:params()

  return {
    request_id = ctx:request_id(),
    ua = headers["user-agent"],
    page = query.page,
    id = params.id,
  }
end
```

## Request-scoped State

- `ctx:set(key, value)`
- `ctx:get(key)`
- `ctx:next()` (compat helper; returns `nil`)

```lua
api.before = function(ctx)
  ctx:set("start", os.clock())
end

api.after = function(ctx)
  local start = ctx:get("start")
  if start then
    print("elapsed_ms", (os.clock() - start) * 1000)
  end
end
```

## BodyValue API

From `local body = ctx:body()`:

- `body:json()` -> parsed JSON
- `body:raw()` -> parsed JSON (alias)
- `body:text()` -> string
- `body:as_string()` -> string
- `body:echo()` -> string
- `body:bytes()` -> numeric byte array
- `body:expect(schema)` -> validated table via `rover.guard`

JSON media-type rules:

- `json/raw/expect` require `application/json` (or `+json`)
- otherwise runtime returns `415 Unsupported Media Type`

```lua
function api.users.post(ctx)
  local input = ctx:body():expect {
    name = rover.guard:string():required(),
    email = rover.guard:string():required(),
  }
  return api.json:status(201, input)
end
```

## Multipart Helpers

For `multipart/form-data` payloads:

- `body:file(field_name)` -> first file or `nil`
- `body:files(field_name)` -> file array
- `body:form()` -> form fields table
- `body:multipart()` -> `{ fields = ..., files = ... }`

```lua
function api.upload.post(ctx)
  local body = ctx:body()
  local form = body:form()
  local avatar = body:file("avatar")

  return api.json {
    username = form.username,
    avatar_name = avatar and avatar.name,
    avatar_size = avatar and avatar.size,
  }
end
```
