---
sidebar_position: 3
---

# Response Builders

Rover provides ergonomic response builders with optimal performance for returning different types of HTTP responses.

:::info Performance
All builders use pre-serialization for near-zero overhead (~182k req/s)
:::

## JSON Responses

Return JSON data with the `api.json` builder:

```lua
-- 200 OK with JSON
function api.users.get(ctx)
    return api.json { users = {...} }
end

-- Custom status code
function api.users.post(ctx)
    return api.json:status(201, { id = 123 })
end
```

### Fast Path: Plain Tables

For convenience, you can return plain Lua tables, which are automatically converted to JSON:

```lua
function api.simple.get(ctx)
    return { message = "Hello" }  -- Automatic JSON, 200 OK
end
```

## Text Responses

Return plain text with the `api.text` builder:

```lua
-- 200 OK with text/plain
function api.health.get(ctx)
    return api.text("OK")
end

-- Custom status code
function api.error.get(ctx)
    return api.text:status(503, "Service Unavailable")
end
```

## HTML Responses

Return HTML with the `api.html` builder:

```lua
-- 200 OK with text/html
function api.home.get(ctx)
    return api.html("<h1>Welcome</h1>")
end

-- Custom status code (404)
function api.notfound.get(ctx)
    return api.html:status(404, "<h1>Not Found</h1>")
end
```

## Redirects

Redirect to another URL with the `api.redirect` builder:

```lua
-- 302 Found (temporary redirect)
function api.old.get(ctx)
    return api.redirect("/new")
end

-- 301 Moved Permanently
function api.legacy.get(ctx)
    return api.redirect:permanent("/new-url")
end

-- Custom redirect status
function api.temp.get(ctx)
    return api.redirect:status(307, "/temporary")
end
```

## Error Responses

Return error responses with the `api.error` builder:

```lua
function api.protected.get(ctx)
    local auth = ctx:headers()["Authorization"]
    if not auth then
        return api.error(401, "Unauthorized")
    end
    return api.json { data = "secret" }
end
```

This returns a JSON response like:

```json
{
  "error": "Unauthorized"
}
```

## No Content

Return a 204 No Content response:

```lua
function api.items.p_id.delete(ctx)
    local id = ctx:params().id
    -- Delete the item...
    return api.no_content()
end
```

## Summary

| Builder | Default Status | Content-Type | Example |
|---------|---------------|--------------|---------|
| `api.json` | 200 | `application/json` | `api.json { data = "..." }` |
| `api.text` | 200 | `text/plain` | `api.text("Hello")` |
| `api.html` | 200 | `text/html` | `api.html("<h1>Hi</h1>")` |
| `api.redirect` | 302 | - | `api.redirect("/path")` |
| `api.error` | Custom | `application/json` | `api.error(401, "msg")` |
| `api.no_content` | 204 | - | `api.no_content()` |
| Plain table | 200 | `application/json` | `{ message = "..." }` |

## Next Steps

- [Configuration](/docs/api-reference/configuration) - Configure server options
- [Route Patterns](/docs/api-reference/route-patterns) - Learn about routing
