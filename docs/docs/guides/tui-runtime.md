---
sidebar_position: 8
---

# TUI Runtime

Use `rover.ui` + `require("rover.tui")` for terminal-specific components.

## Setup

```lua
require("rover.tui")
local ui = rover.ui
```

## TUI Helpers

- `ui.select { title = "...", items = table_or_signal }`
- `ui.tab_select { value = signal, options = {...}, on_change = fn }`
- `ui.scroll_box { content }`
- `ui.textarea { value = signal_or_string, on_change = fn, on_submit = fn }`
- `ui.nav_list { title, items, selected, query?, on_key? }`
- `ui.separator { width = 40, char = "-" }`
- `ui.badge { label = "...", tone = "info|success|warning|danger|neutral" }`
- `ui.progress { value, max, width?, label? }`
- `ui.paginator { page, total_pages, on_change?, on_key? }`
- `ui.full_screen { child }` (alternate screen + full terminal canvas)

## Table Style

Use positional content, not `child` / `children` props.

```lua
ui.key_area {
  on_key = function(key) end,
  content,
}
```

## Key Tokens

Runner forwards key tokens. App handles behavior.

- `up`, `down`, `left`, `right`
- `home`, `end`, `page_up`, `page_down`
- `enter`, `esc`, `tab`, `backtab`
- `backspace`, `delete`, `space`
- `char:x`, `ctrl+x`, `alt+x`

## App-Controlled Pattern

```lua
require("rover.tui")
local ui = rover.ui

function rover.render()
  local items = rover.signal({ "Parser", "TUI", "Docs" })
  local selected = rover.signal(1)
  local status = rover.signal("idle")

  return ui.nav_list {
    title = "Tasks",
    items = items,
    selected = selected,
    on_key = function(key)
      if key == "up" then
        selected.val = math.max(1, selected.val - 1)
      elseif key == "down" then
        selected.val = math.min(#items.val, selected.val + 1)
      elseif key == "enter" then
        status.val = "picked: " .. tostring(items.val[selected.val])
      end
    end,
  }
end
```

## Full Example

- `examples/tui/kitchen_sink.lua`
- `examples/tui/modifiers_showcase.lua`
