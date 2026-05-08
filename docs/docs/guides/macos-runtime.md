# macOS Runtime

`rover-macos` is the native AppKit UI target.

- Portable UI stays in `rover.ui`.
- macOS-only UI lives in `rover.macos`.
- Shared scroll containers use `rover.ui.scroll_view`.
- Children use positional Lua table entries, not `child = ...`.
- Layout is px-based and computed by Rover, then applied to native AppKit views.
- macOS shares Apple renderer ABI/layout primitives with iOS through `rover-apple`.
- The native bridge uses typed callbacks and raw view handles, not JSON.

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
      ui.scroll_view {
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

Run the broader component/style showcase:

```bash
ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/macos_showcase.lua --platform macos
```

Styles use snake_case keys. macOS currently applies `bg_color`, `border_color`, `border_width`, and text `color`/`fg_color`/`text_color` to native AppKit views.

During local `rover_cli` development, skip the web runtime asset build when you are only testing macOS:

```bash
ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/macos_counter.lua --platform macos
```

`cargo run -p rover_cli` builds the CLI first. The CLI build script prepares embedded web assets by default, even when the command runs the macOS target. `ROVER_WEB_SKIP_AUTO_BUILD=1` writes placeholder web assets instead, so macOS iteration does not require the web/WASM toolchain.

If you already have built web assets, point the build script at them instead:

```bash
ROVER_WEB_ASSETS_TAR_GZ=/absolute/path/to/rover_web_assets.tar.gz cargo run -p rover_cli -- run examples/macos_counter.lua --platform macos
```

Build/package support is intentionally deferred while the native dev loop stabilizes.
