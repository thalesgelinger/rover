local ui = rover.ui

function rover.render()
  local count = rover.signal(0)
  local name = rover.signal("")
  local enabled = rover.signal(true)

  return ui.column {
    style = { padding = 24, gap = 12, width = "full", height = "full", bg_color = "#f8fafc" },
    ui.text {
      "Rover iOS",
      style = { color = "#0f172a" },
    },
    ui.text {
      "Native UIKit renderer",
      style = { color = "#2563eb" },
    },
    ui.text {
      "Count: " .. count,
      style = { color = "#16a34a" },
    },
    ui.button {
      label = "Increment",
      style = {
        padding = 8,
        bg_color = "#dbeafe",
        border_color = "#60a5fa",
        border_width = 1,
        color = "#1d4ed8",
      },
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
    ui.text {
      "Hello " .. name,
      style = { color = "#9333ea" },
    },
    ui.column {
      style = {
        padding = 12,
        gap = 8,
        width = "full",
        bg_color = "#ecfeff",
        border_color = "#06b6d4",
        border_width = 1,
      },
      ui.text {
        "Styled native section",
        style = { color = "#0e7490" },
      },
      ui.text {
        "Background, border, text color",
        style = { color = "#475569" },
      },
    },
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
        ui.text { "UIKit views", style = { color = "#dc2626" } },
        ui.text { "Rover signals drive updates", style = { color = "#ea580c" } },
        ui.text { "Typed native bridge, no JSON", style = { color = "#0891b2" } },
      },
    },
  }
end
