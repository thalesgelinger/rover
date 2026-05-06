---
sidebar_position: 3
---

# Response Builders

All response builders currently available on `api = rover.server {}`.

## JSON

```lua
return api.json { ok = true }
return api.json:status(201, { id = 1 })
```

Plain table returns are auto-converted to JSON `200`.

## Text

```lua
return api.text("OK")
return api.text:status(503, "unavailable")
```

## HTML

```lua
return api.html("<h1>Hello</h1>")
return api.html({ title = "Home" })[[<h1>{{ title }}</h1>]]
return api.html:status(404, "<h1>Not Found</h1>")
```

## Redirect

```lua
return api.redirect("/new")
return api.redirect:permanent("/new")
return api.redirect:status(307, "/tmp")
```

## Error

```lua
return api:error(401, "Unauthorized")
```

Returns JSON shape:

```json
{"error":"Unauthorized"}
```

## No Content

```lua
return api.no_content()
```

## Raw

```lua
return api.raw("raw-bytes")
return api.raw:status(201, "created")
```

## Stream

`api.stream(status, content_type, producer)`

```lua
function api.logs.get(ctx)
  local i = 0
  return api.stream(200, "text/plain", function()
    i = i + 1
    if i > 5 then return nil end
    return "line " .. i .. "\n"
  end)
end
```

`producer` returns `string` chunks, then `nil` to finish.

## Stream With Headers

`api.stream_with_headers(status, content_type, headers, producer)`

```lua
return api.stream_with_headers(200, "text/plain", {
  ["Cache-Control"] = "no-store",
}, function()
  return nil
end)
```

## SSE

```lua
return api.sse(function(writer)
  return {
    event = "tick",
    data = { now = os.time() },
  }
end, 1000)
```

Also:

- `api.sse:status(status, producer, retry_ms?)`
- `api.sse:with_headers(status, headers, producer, retry_ms?)`

## Summary

| Builder | Default Status | Content-Type |
|---|---:|---|
| `api.json` | 200 | `application/json` |
| `api.text` | 200 | `text/plain` |
| `api.html` | 200 | `text/html` |
| `api.redirect` | 302 | - |
| `api:error` | custom | `application/json` |
| `api.no_content` | 204 | - |
| `api.raw` | 200 | - |
| `api.stream` | custom | custom |
| `api.stream_with_headers` | custom | custom |
| `api.sse` | 200 | `text/event-stream` |
