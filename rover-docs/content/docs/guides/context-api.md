---
weight: 2
title: Context API
---

Access request data through the context object passed to your route handlers.

## Available Methods

The context object (`ctx`) provides methods to access different parts of the HTTP request:

### `ctx.method`

Get the HTTP method of the request:

```lua
function api.echo.get(ctx)
    return { method = ctx.method }  -- "GET"
end
```

### `ctx.path`

Get the request path:

```lua
function api.echo.get(ctx)
    return { path = ctx.path }  -- "/echo"
end
```

### `ctx:headers()`

Access request headers:

```lua
function api.echo.get(ctx)
    local headers = ctx:headers()
    return {
        user_agent = headers["user-agent"],
        content_type = headers["content-type"]
    }
end
```

### `ctx:query()`

Access query string parameters:

```lua
function api.search.get(ctx)
    local query = ctx:query()
    return {
        page = query.page,
        limit = query.limit
    }
end
```

Example request: `GET /search?page=1&limit=10`

### `ctx:params()`

Access URL path parameters:

```lua
function api.users.p_id.get(ctx)
    local params = ctx:params()
    return {
        user_id = params.id
    }
end
```

Example request: `GET /users/123` → `params.id = "123"`

### `ctx:body()`

Access the request body (for POST, PUT, PATCH):

```lua
function api.users.post(ctx)
    local body = ctx:body()
    return {
        received = body
    }
end
```

`ctx:body()` returns a BodyValue with helpers:

- `:json()` - parse JSON to Lua table
- `:as_string()` - raw JSON string (zero-copy)
- `:text()` - body as text
- `:bytes()` - body as byte table
- `:expect(schema)` - validate with `rover.guard`

Example validation:

```lua
function api.users.post(ctx)
    local user = ctx:body():expect {
        name = rover.guard:string():required(),
        email = rover.guard:string():required(),
    }

    return api.json(user)
end
```

## Complete Example

Here's a comprehensive example using multiple context methods:

```lua
local api = rover.server { }

function api.echo.get(ctx)
    return {
        method = ctx.method,
        path = ctx.path,
        headers = ctx:headers()["user-agent"],
        query = ctx:query().page
    }
end

function api.echo.post(ctx)
    return {
        body = ctx:body(),
        content_type = ctx:headers()["content-type"]
    }
end

function api.users.p_id.posts.p_postId.get(ctx)
    local params = ctx:params()
    return {
        user = params.id,
        post = params.postId
    }
end

return api
```

## Next Steps

- [Response Builders](/docs/guides/response-builders) - Learn how to return structured responses
- [Configuration](/docs/api-reference/configuration) - Configure your server
