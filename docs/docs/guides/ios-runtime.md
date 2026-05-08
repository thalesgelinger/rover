# iOS Runtime

`rover-ios` is the native UIKit UI target.

- Portable UI stays in `rover.ui`.
- iOS initially has no `rover.ios` Lua namespace.
- UIKit owns native views; Rover owns signals, dirty updates, and px layout.
- The bridge is typed C ABI callbacks with raw handles, not JSON.
- Generated native files live in `.rover/ios`.

```lua
local ui = rover.ui

function rover.render()
  local count = rover.signal(0)

  return ui.column {
    style = { padding = 24, gap = 12 },
    ui.text { "Count: " .. count },
    ui.button {
      label = "Increment",
      on_click = function()
        count.val = count.val + 1
      end,
    },
  }
end
```

Run on simulator:

```bash
ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/ios_counter.lua --platform ios
```

Run on device:

```bash
ROVER_IOS_TEAM_ID=ABCDE12345 ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/ios_counter.lua --platform ios --device
```

Optional `rover.lua` metadata:

```lua
return {
  name = "Counter",
  ios = {
    bundle_id = "lu.rover.generated.counter",
    team_id = "ABCDE12345",
  },
}
```

Defaults:

- `name`: entry file stem.
- `ios.bundle_id`: `lu.rover.generated.<sanitized-name>`.
- `ios.team_id`: optional for simulator, required for device.

Native extension direction:

- `.rover/ios` is generated and can be edited while exploring native changes.
- Managed native plugins are reserved under `native/ios/plugins/<name>/plugin.lua`.
- Future capture command shape: `rover capture -p ios <name>`.
- Plugin manifests reserve fields for `frameworks`, `info_plist`, `entitlements`, and `files`.
