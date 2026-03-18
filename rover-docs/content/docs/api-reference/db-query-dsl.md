---
weight: 8
title: DB Query DSL
---

This reference lists the core query API exposed by `rover.db`.

## Query Entry

- `db.<table>:find()` -> `Query`
- `db.<table>:sql()` -> raw SQL query builder

## Insert

```lua
db.users:insert({ name = "Ada" })
```

## Filters

- `by_<field>(value)`
- `by_<field>_<operator>(value)`

Operators: `equals`, `not_equals`, `bigger_than`, `smaller_than`, `bigger_than_or_equals`, `smaller_than_or_equals`, `contains`, `starts_with`, `ends_with`, `between`, `in_list`, `not_in_list`, `is_null`, `is_not_null`

## Query Methods

- `select(...)`
- `group_by(...)`
- `order_by(column, direction)`
- `limit(n)`
- `offset(n)`
- `agg({ name = rover.db.count("*") })`
- `having_<agg>_<operator>(value)` (generated after `agg`)
- `exists(subquery)`
- `on(col_a, col_b)` (correlate after `exists`)
- `merge(query)`
- `with_<relation>()` (from schema relations)
- `all()`
- `first()`
- `count()`
- `inspect()`

## Raw SQL

```lua
db.users:sql():raw("SELECT * FROM users"):all()
```

## Update/Delete

- `db.<table>:update():set(values):exec()`
- `db.<table>:delete():exec()`

## Aggregates

- `rover.db.count(expr)`
- `rover.db.sum(expr)`
- `rover.db.avg(expr)`
- `rover.db.min(expr)`
- `rover.db.max(expr)`
