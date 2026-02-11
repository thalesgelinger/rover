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
- `ru.stack { ...children }`

Signals and derived values can be concatenated with strings (e.g., `"Count: " .. count`).

TUI-only helpers are in `require("rover.tui")`.

See [TUI Runtime](./tui-runtime).

## Modifiers

`ru.mod` is a chainable style builder.

```lua
local ru = rover.ui
local mod = ru.mod

ru.view {
  mod = mod:width("full"):height("full"):bg_color("surface"):padding("md"),
}
```

- Order matters for wrapper ops (`bg_color`, `padding`, `border_*`).
- You can extend globally:

```lua
function rover.ui.mod:debug()
  return self:border_color("danger"):border_width(1)
end
```

- Theme tokens are available at `rover.ui.theme` (`space.*`, `color.*`).

### Theme

Default shape:

```lua
rover.ui.theme = {
  space = { none = 0, xs = 1, sm = 2, md = 3, lg = 4, xl = 6 },
  color = {
    surface = "#1f2937",
    surface_alt = "#374151",
    text = "#f9fafb",
    border = "#6b7280",
    accent = "#22c55e",
    danger = "#ef4444",
    warning = "#f59e0b",
    info = "#3b82f6",
  },
}
```

Modify theme in 3 ways:

```lua
local ui = rover.ui

-- merge patch (keeps missing keys)
ui.extend_theme({
  color = { accent = "#00d084" },
  space = { sm = 3 },
})

-- replace theme
ui.set_theme({
  space = { none = 0, sm = 2, md = 4 },
  color = { surface = "#101828", accent = "#00d084" },
})

-- assignment also replaces
ui.theme = {
  space = { sm = 2 },
  color = { accent = "#00d084" },
}
```

All modifiers resolve from current theme, including existing `ui.mod` chains.

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
end)
```

`items` can be a table or a signal/derived table.

`key_fn` is optional (reserved for keyed reconciliation). `ru.each` is a transparent helper: it does not add a visual/container layer,
its children are treated as direct children of the parent container.

## Tasks + Delay

- `rover.task(fn)` creates a task
- `rover.spawn(fn)` creates and starts a background task immediately
- `rover.delay(ms)` yields inside tasks
- `rover.interval(ms, fn)` runs `fn` now, then every `ms`
- `rover.task.cancel(task)` stops a task
- `rover.task.all(task1, task2, ...)` runs tasks in parallel
- `task:pid()` returns task id
- `task:kill()` cancels task (alias of `task:cancel()`)

## Cleanup

Use `rover.on_destroy(fn)` to register cleanup callbacks.
