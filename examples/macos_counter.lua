local ui = rover.ui
local macos = require("rover.macos")

function rover.render()
  local count = rover.signal(0)

  return macos.window {
    title = "Rover Counter",
    width = 300,
    height = 300,

    ui.column {
      style = { padding = 24, gap = 12 },
      ui.text { "Count: " .. count },
      ui.button {
        label = "Increment",
        on_click = function()
          count.val = count.val + 1
        end,
      },
      macos.scroll_view {
        style = { height = 100, width = "full" },
        ui.column {
          ui.text { "Native AppKit scroll area" },
          ui.text { "Rover computes layout in px" },
          ui.text { "Row 3" },
          ui.text { "Row 4" },
          ui.text { "Row 5" },
          ui.text { "Row 6" },
          ui.text { "Row 7" },
          ui.text { "Row 8" },
        },
      },
    },
  }
end
