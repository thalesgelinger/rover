---
sidebar_position: 5
---

# Database

Rover includes an intent-based SQLite ORM via `rover.db`.

## Connect

```lua
local db = rover.db.connect { path = "rover.sqlite" }
```

`path` defaults to `rover.sqlite`.

## Basic Queries

```lua
-- All users
local users = db.users:find():all()

-- Filtered
local admin = db.users:find():by_role("admin"):first()
```

## Insert

```lua
db.users:insert({ name = "Ada", role = "admin" })
```

## Update

```lua
db.users
  :update()
  :by_id(1)
  :set({ name = "Ada Lovelace" })
  :exec()
```

## Delete

```lua
db.users
  :delete()
  :by_id(1)
  :exec()
```

## Schema DSL

Define schema to generate query helpers:

```lua
rover.db.schema.users {
  id = rover.db.guard:integer():primary():auto(),
  email = rover.db.guard:string():unique(),
  status = rover.db.guard:string():default("active")
}
```

Use `rover.db.guard` for database schemas — it's an extended version of `rover.guard` with schema modifiers like `:primary()`, `:auto()`, `:unique()`, `:references()`, and `:index()`. You can also use `rover.guard` directly since it includes these modifiers too.

## Next Steps

- [Query DSL](/docs/api-reference/db-query-dsl)
- [Migrations](/docs/guides/migrations)
