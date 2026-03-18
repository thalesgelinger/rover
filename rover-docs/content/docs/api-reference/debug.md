---
weight: 7
title: Debug Utilities
---

Rover extends `debug` with `debug.print`.

## debug.print(value, label?)

Pretty-prints Lua values with nesting and circular protection.

```lua
debug.print({ ok = true, items = { 1, 2 } }, "payload")
```

Returns the original value for chaining.
