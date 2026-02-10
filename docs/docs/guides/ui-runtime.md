---
sidebar_position: 7
---

# UI Runtime

Rover UI is defined with `rover.ui` and a global `rover.render()` function.

## Entry Point

```lua
local ru = rover.ui

function rover.render()
  local count = rover.signal(0)

  local tick = rover.task(function()
    while true do
      rover.delay(1000)
      count.val = count.val + 1
    end
  end)

  tick()

  rover.on_destroy(function()
    rover.task.cancel(tick)
  end)

  return ru.text { "Count: " .. count }
end
```

## Components

- `ru.text { value }`
- `ru.button { label = "Click", on_click = function() end }`
- `ru.input { value = signal_or_string, on_change = function(val) end }`
- `ru.checkbox { checked = bool, on_toggle = function(val) end }`
- `ru.image { src = "path.png" }`
- `ru.column { ...children }`
- `ru.row { ...children }`
- `ru.view { ...children }`

Signals and derived values can be concatenated with strings (e.g., `"Count: " .. count`).

TUI-only helpers are in `require("rover.tui")`.

See [TUI Runtime](./tui-runtime).

## Conditional Rendering

```lua
ru.when(show, function()
  return ru.text { "Visible" }
end)
```

`show` can be a boolean, signal, or derived value.

## List Rendering

```lua
ru.each(items, function(item, index)
  return ru.text { index .. ": " .. item }
end, function(item, index)
  return item .. index
end)
```

`items` can be a table or a signal/derived table.

## Tasks + Delay

- `rover.task(fn)` creates a task
- `rover.delay(ms)` yields inside tasks
- `rover.task.cancel(task)` stops a task
- `rover.task.all(task1, task2, ...)` runs tasks in parallel

## Cleanup

Use `rover.on_destroy(fn)` to register cleanup callbacks.
