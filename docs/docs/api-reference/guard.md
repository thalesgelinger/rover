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

## Extending Guard

Guard supports extension for domain-specific modifiers via `guard:extend(methods)`:

```lua
-- Create a custom guard with additional methods
local db_guard = rover.guard:extend({
  primary = function(self)
    self._primary = true
    return self
  end,
  auto = function(self)
    self._auto = true
    return self
  end,
})

-- Use the extended guard
local schema = {
  id = db_guard:integer():primary():auto(),
  name = db_guard:string():required()
}
```

The `extend` method creates a new Guard instance that inherits all base methods plus your custom ones. Each custom method receives the validator instance (`self`) and should return `self` for chaining.

## Database Guard

Rover provides `rover.db.guard` — an extended guard pre-configured with database schema modifiers:

```lua
rover.db.schema.users {
  id = rover.db.guard:integer():primary():auto(),
  email = rover.db.guard:string():unique(),
  status = rover.db.guard:string():default("active")
}
```

### DB Modifiers

- `:primary()` — Mark as primary key
- `:auto()` — Auto-increment (for integers)
- `:unique()` — Unique constraint
- `:references("table.column")` — Foreign key reference
- `:index()` — Create index

These modifiers are also available in migrations:

```lua
function change()
  migration.users:create({
    id = rover.db.guard:integer():primary():auto(),
    email = rover.db.guard:string():unique()
  })
end
```

:::tip
Use `rover.guard` for general validation and `rover.db.guard` for database schemas. `rover.db.guard` extends the base guard with DB modifiers.
:::
