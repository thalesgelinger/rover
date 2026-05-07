# macOS Runtime

`rover-macos` is the native AppKit UI target.

- Portable UI stays in `rover.ui`.
- macOS-only UI lives in `rover.macos`.
- Children use positional Lua table entries, not `child = ...`.
- Layout is px-based and computed by Rover, then applied to native AppKit views.

```lua
local ui = rover.ui
local macos = require("rover.macos")

function rover.render()
  return macos.window {
    title = "Counter",
    width = 900,
    height = 640,

    ui.column {
      ui.text { "Hello macOS" },
      macos.scroll_view {
        ui.column {
          ui.text { "Native AppKit" },
        },
      },
    },
  }
end
```

Run:

```bash
rover run examples/macos_counter.lua --platform macos
```

Build/package support is intentionally deferred while the native dev loop stabilizes.
