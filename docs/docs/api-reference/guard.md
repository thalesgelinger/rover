---
sidebar_position: 5
---

# Guard Validation

`rover.guard` provides Zod-style validation for Lua tables.

## Validate Data

```lua
local schema = {
  id = rover.guard:integer():required(),
  email = rover.guard:string():required(),
  role = rover.guard:string():default("user")
}

local data = rover.guard({ id = 1, email = "a@b.com" }, schema)
```

Use `ctx:body():expect(schema)` for request validation:

```lua
function api.users.post(ctx)
  local user = ctx:body():expect(schema)
  return api.json(user)
end
```

Validation errors throw with a clean message. Use `rover.guard.validate` to wrap errors:

```lua
rover.guard.validate(function()
  rover.guard({ }, schema)
end)
```

## Types

- `rover.guard:string()`
- `rover.guard:number()`
- `rover.guard:integer()`
- `rover.guard:boolean()`
- `rover.guard:array(element_validator)`
- `rover.guard:object(schema)`

## Modifiers

- `:required([msg])`
- `:default(value)`
- `:enum(values)`
- `:nullable()`

## Schema Modifiers (DB)

- `:primary()`
- `:auto()`
- `:unique()`
- `:references("table.column")`
- `:index()`
