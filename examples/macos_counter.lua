local ui = rover.ui
local macos = require("rover.macos")

function rover.render()
  local count = rover.signal(0)

  return macos.window {
    title = "Rover Counter",
    width = 900,
    height = 640,

    ui.column {
      mod = ui.mod:padding(24):gap(12),
      ui.text { "Count: " .. count },
      ui.button {
        label = "Increment",
        on_click = function()
          count.val = count.val + 1
        end,
      },
      macos.scroll_view {
        ui.column {
          ui.text { "Native AppKit scroll area" },
          ui.text { "Rover computes layout in px" },
        },
      },
    },
  }
end
