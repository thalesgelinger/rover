# Rover Component Catalog

Complete reference for all 27 Rover UI components with Material/shadcn-inspired defaults.

## Layout Components (4)

### `rover.col`
Vertical flex container with automatic spacing.

```lua
rover.col {
    width = 'full',
    height = 300,
    rover.text { "Item 1" },
    rover.text { "Item 2" },
}
```

**Props:**
- `width`: `'full'`, `'auto'`, number (px), or `{ kind = 'flex', value = 1 }`
- `height`: same as width

---

### `rover.row`
Horizontal flex container with automatic spacing.

```lua
rover.row {
    rover.button { "Left", on_click = actions.left() },
    rover.button { "Right", on_click = actions.right() },
}
```

**Props:**
- `width`, `height`: same as `col`

---

### `rover.stack`
Vertical stack with configurable gap and padding.

```lua
rover.stack {
    rover.text { "Stacked 1" },
    rover.text { "Stacked 2" },
}
```

**Props:**
- `width`, `height`: same as `col`

---

### `rover.text`
Text label with theme typography.

```lua
rover.text { "Hello, world!" }
```

**Props:**
- `[1]`: text content (string)

---

## Input Components (6)

### `rover.button`
Primary action button.

```lua
rover.button { "Click me", on_click = actions.submit() }
```

**Props:**
- `[1]`: button label (string)
- `on_click`: action reference (e.g., `actions.submit()`)
- `disabled`: boolean (default: false)

---

### `rover.input`
Single-line text input.

```lua
rover.input {
    value = state.username,
    placeholder = "Enter username...",
    on_change = actions.update_username,
}
```

**Props:**
- `value`: current input value (string)
- `placeholder`: placeholder text (string)
- `on_change`: action that receives new value
- `disabled`: boolean

---

### `rover.textarea`
Multi-line text input.

```lua
rover.textarea {
    value = state.description,
    placeholder = "Enter description...",
    on_change = actions.update_description,
}
```

**Props:**
- Same as `input`

---

### `rover.checkbox`
Toggle checkbox control.

```lua
rover.checkbox {
    checked = state.agreed,
    on_click = actions.toggle_agree(),
}
```

**Props:**
- `checked`: boolean
- `on_click`: action reference

---

### `rover.switch`
Toggle switch (styled differently from checkbox).

```lua
rover.switch {
    checked = state.enabled,
    on_click = actions.toggle_enabled(),
}
```

**Props:**
- Same as `checkbox`

---

### `rover.slider`
Slider for numeric input (WIP - registered but not fully implemented).

```lua
rover.slider {
    value = tostring(state.volume),
    on_change = actions.set_volume,
}
```

---

### `rover.radio_group`
Radio button group (WIP - registered but not fully implemented).

```lua
rover.radio_group {
    -- Implementation pending
}
```

---

## Feedback & Display Components (5)

### `rover.badge`
Small badge/label for status or count.

```lua
rover.badge { "New" }
rover.badge { "v1.0.2" }
```

**Props:**
- `[1]`: badge text (string)

---

### `rover.spinner`
Loading spinner animation.

```lua
rover.spinner {}
```

**Props:**
- None

---

### `rover.progress`
Progress bar (0.0 to 1.0).

```lua
rover.progress { value = tostring(state.progress) }
```

**Props:**
- `value`: progress as string ("0.0" to "1.0")

---

### `rover.avatar`
Circular avatar with initials or image.

```lua
rover.avatar { "AB" }  -- Shows initials
```

**Props:**
- `[1]`: initials (string, typically 1-2 chars)

---

### `rover.separator`
Horizontal divider line.

```lua
rover.separator {}
```

**Props:**
- None

---

## Container Components (7)

### `rover.card`
Card container with border and padding.

```lua
rover.card {
    rover.text { "Card content" },
}
```

**Props:**
- Children: card contents

---

### `rover.card_header`
Card header section (use inside `card`).

```lua
rover.card {
    rover.card_header {
        rover.text { "Title" },
    },
    rover.text { "Body text" },
}
```

---

### `rover.card_footer`
Card footer section (use inside `card`).

```lua
rover.card {
    rover.text { "Content" },
    rover.card_footer {
        rover.button { "OK", on_click = actions.ok() },
    },
}
```

---

### `rover.scroll_area`
Scrollable container with viewport clipping.

```lua
rover.scroll_area {
    -- Many children items
}
```

**Props:**
- Children: scrollable content

---

### `rover.list`
Virtualized list container (clips at viewport, 56px row height).

```lua
local items = {}
for i = 1, 100 do
    table.insert(items, rover.list_item {
        "Item " .. i,
        icon = "user",
        on_click = actions.select,
    })
end

rover.list(items)
```

**Props:**
- Array of `list_item` children

---

### `rover.list_item`
List row with optional icon and click handler.

```lua
rover.list_item {
    "Item title",
    icon = "check",
    on_click = actions.select_item,
}
```

**Props:**
- `[1]`: item text (string)
- `icon`: icon name from Lucide set (see Icons section)
- `on_click`: action reference

---

## Overlay Components (4)

### `rover.dialog`
Modal dialog with backdrop (centered, 80% width/60% height).

```lua
rover.dialog {
    rover.text { "Dialog content" },
    rover.button { "Close", on_click = actions.close_dialog() },
}
```

**Props:**
- Children: dialog content
- Renders on overlay layer with dark backdrop

---

### `rover.sheet`
Modal sheet (similar to dialog, can be side/bottom anchored).

```lua
rover.sheet {
    rover.text { "Sheet content" },
}
```

**Props:**
- Same as `dialog`

---

### `rover.popover`
Non-modal popover (no backdrop).

```lua
rover.popover {
    "Popover text",
}
```

**Props:**
- `[1]`: popover text (string)

---

### `rover.tooltip`
Small tooltip (non-modal, no backdrop).

```lua
rover.tooltip {
    "Helpful tip",
}
```

**Props:**
- `[1]`: tooltip text (string)

---

## Icons

30 Lucide icons available via `icon = "name"`:

**Navigation & Actions:**
- `home`, `search`, `menu`, `settings`, `plus`, `minus`, `x`, `check`

**UI & Controls:**
- `chevron-down`, `chevron-up`, `chevron-left`, `chevron-right`
- `info`, `alert-circle`, `loader`

**User & Social:**
- `user`, `heart`, `star`, `bell`, `mail`

**Files & Data:**
- `file`, `download`, `upload`, `trash`, `edit`

**Media:**
- `eye`, `eye-off`

**Time:**
- `calendar`, `clock`

---

## Theme Defaults

All components use Material/shadcn defaults:

**Colors:**
- Primary: `#3B82F6` (blue)
- Success: `#22C55E` (green)
- Warning: `#FB923C` (orange)
- Error: `#EF4444` (red)
- Border: `#E2E8F0` (light gray)

**Spacing:**
- xs: 4px, sm: 8px, md: 16px, lg: 24px, xl: 32px
- Gap between children: 8px (default)

**Radii:**
- sm: 4px, md: 6px, lg: 8px

**Typography:**
- xs: 12px, sm: 14px, base: 16px, lg: 18px, xl: 20px

---

## Complete Example

```lua
local app = rover.app()

function app.init()
    return {
        count = 0,
        username = "",
        enabled = false,
        progress = 0.3,
    }
end

function app.increment(state)
    return { 
        count = state.count + 1,
        username = state.username,
        enabled = state.enabled,
        progress = state.progress,
    }
end

function app.toggle(state)
    return {
        count = state.count,
        username = state.username,
        enabled = not state.enabled,
        progress = state.progress,
    }
end

function app.render(state, actions)
    return rover.col {
        rover.card {
            rover.card_header {
                rover.text { "Demo App" },
            },
            
            rover.badge { "v1.0" },
            
            rover.button { "Count: " .. state.count, on_click = actions.increment() },
            
            rover.input {
                value = state.username,
                placeholder = "Username...",
            },
            
            rover.switch {
                checked = state.enabled,
                on_click = actions.toggle(),
            },
            
            rover.progress { value = tostring(state.progress) },
            
            rover.separator {},
            
            rover.card_footer {
                rover.avatar { "AB" },
            },
        },
    }
end

return app
```

---

## Notes

- **Events**: `on_click` and `on_change` dispatch to `app.*` actions; handlers don't return values
- **Styling**: Fixed defaults for MVP; custom styling API deferred
- **Animation**: Not yet implemented; planned for future
- **Virtualization**: Lists auto-clip at viewport boundary with 56px row height
- **Focus**: Runtime will track focused input (planned); keyboard is native (iOS/Android)
- **Modal behavior**: `dialog` and `sheet` render with backdrop; hit-testing blocks base layer
