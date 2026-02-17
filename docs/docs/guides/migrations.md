---
sidebar_position: 6
---

# Migrations

Rover provides a migration DSL under `migration` (alias of `rover.db.migration`).

Migrations are the only path that changes the database. Schema DSL + intent only generate files; use `rover db` for manual changes.

## Migration File Shape

You can define either:

- `change()` for auto-reversible migrations
- `up()` / `down()` for manual control

## Example (change)

```lua
function change()
  migration.users:create({
    id = rover.db.guard:integer():primary():auto(),
    email = rover.db.guard:string():unique(),
    created_at = rover.db.guard:string()
  })
end
```

## Example (up/down)

```lua
function up()
  migration.users:add_column("name", rover.db.guard:string())
end

function down()
  migration.users:remove_column("name")
end
```

## Operations

- `migration.<table>:create(definition)`
- `migration.<table>:drop()`
- `migration.<table>:add_column(name, type)`
- `migration.<table>:remove_column(name)`
- `migration.<table>:rename_column(old, new)`
- `migration.<table>:create_index(name, columns)`
- `migration.<table>:drop_index(name)`
- `migration.<table>:rename(new_name)`
- `migration.<table>:alter_table():add_column(...)` (fluent chain)
- `migration.raw(sql)`

Raw SQL is not auto-reversible; use `up()`/`down()` if needed.
