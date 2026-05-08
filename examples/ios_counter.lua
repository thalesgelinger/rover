local ui = rover.ui

function rover.render()
  local count = rover.signal(0)
  local name = rover.signal("")
  local enabled = rover.signal(true)

  return ui.column {
    style = { padding = 24, gap = 12, width = "full" },
    ui.text { "Rover iOS" },
    ui.text { "Count: " .. count },
    ui.button {
      label = "Increment",
      on_click = function()
        count.val = count.val + 1
      end,
    },
    ui.input {
      value = name,
      on_change = function(value)
        name.val = value
      end,
    },
    ui.text { "Hello " .. name },
    ui.checkbox {
      checked = enabled.val,
      on_toggle = function(value)
        enabled.val = value
      end,
    },
    ui.scroll_view {
      style = { height = 160, width = "full" },
      ui.column {
        style = { gap = 8 },
        ui.text { "UIKit views" },
        ui.text { "Rover signals drive updates" },
        ui.text { "Typed native bridge, no JSON" },
      },
    },
  }
end
