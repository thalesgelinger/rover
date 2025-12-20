---
sidebar_position: 2
---

# Route Patterns

Understand how Rover's automatic routing system works.

## How Routes Work

Routes are built from nested table access in Lua. Each level of nesting creates a new path segment.

## Static Routes

Static routes are defined by property names:

```lua
-- api.users.get → GET /users
function api.users.get(ctx)
    return { users = [] }
end

-- api.products.list.get → GET /products/list
function api.products.list.get(ctx)
    return { products = [] }
end
```

## Dynamic Routes (Parameters)

Use the `p_<name>` prefix to create path parameters:

```lua
-- api.users.p_id.get → GET /users/:id
function api.users.p_id.get(ctx)
    local id = ctx:params().id
    return { user_id = id }
end
```

The parameter name comes after the `p_` prefix. In this example, `p_id` creates a parameter named `id`.

## Multiple Parameters

You can have multiple parameters in a single route:

```lua
-- api.users.p_userId.posts.p_postId.get → GET /users/:userId/posts/:postId
function api.users.p_userId.posts.p_postId.get(ctx)
    local params = ctx:params()
    return {
        user_id = params.userId,
        post_id = params.postId
    }
end
```

## Route Pattern Examples

| Lua Function | HTTP Route | Example URL |
|--------------|------------|-------------|
| `api.hello.get` | `GET /hello` | `/hello` |
| `api.users.get` | `GET /users` | `/users` |
| `api.users.p_id.get` | `GET /users/:id` | `/users/123` |
| `api.posts.p_id.comments.get` | `GET /posts/:id/comments` | `/posts/5/comments` |
| `api.users.p_uid.posts.p_pid.get` | `GET /users/:uid/posts/:pid` | `/users/10/posts/20` |

## HTTP Methods

Rover supports all standard HTTP methods as function suffixes:

- `get` - GET requests
- `post` - POST requests
- `put` - PUT requests
- `patch` - PATCH requests
- `delete` - DELETE requests
- `head` - HEAD requests
- `options` - OPTIONS requests

Example with multiple methods on the same resource:

```lua
-- GET /users
function api.users.get(ctx)
    return { users = {...} }
end

-- POST /users
function api.users.post(ctx)
    local body = ctx:body()
    return api.json:status(201, { id = 123 })
end

-- GET /users/:id
function api.users.p_id.get(ctx)
    local id = ctx:params().id
    return { user = {...} }
end

-- PUT /users/:id
function api.users.p_id.put(ctx)
    local id = ctx:params().id
    local body = ctx:body()
    return { updated = true }
end

-- DELETE /users/:id
function api.users.p_id.delete(ctx)
    return api.no_content()
end
```

## Best Practices

1. **Use descriptive parameter names**: `p_userId` is better than `p_id` when you have multiple IDs
2. **Keep routes RESTful**: Follow REST conventions for resource naming
3. **Avoid deep nesting**: Try to keep routes no more than 3-4 levels deep
4. **Use plural nouns**: `users` instead of `user` for collections

## Next Steps

- [Context API](/guides/context-api) - Access route parameters and request data
- [Backend Server](/guides/backend-server) - Learn more about building APIs
