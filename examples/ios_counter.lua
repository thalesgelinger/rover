local ui = rover.ui

function rover.render()
  local count = rover.signal(0)
  local name = rover.signal("")
  local enabled = rover.signal(true)

  local function swatch(label, color)
    return ui.row {
      style = { gap = 8, width = "full" },
      ui.view {
        style = {
          width = 44,
          height = 24,
          bg_color = color,
          border_color = "#0f172a",
          border_width = 1,
        },
      },
      ui.text {
        label,
        style = { color = color },
      },
    }
  end

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

    ui.row {
      style = { gap = 8, width = "full" },
      ui.button {
        label = "-1",
        style = {
          padding = 8,
          bg_color = "#fee2e2",
          border_color = "#f87171",
          border_width = 1,
          color = "#b91c1c",
        },
        on_click = function()
          count.val = count.val - 1
        end,
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
      ui.button {
        label = "Reset",
        style = {
          padding = 8,
          bg_color = "#fef3c7",
          border_color = "#f59e0b",
          border_width = 1,
          color = "#92400e",
        },
        on_click = function()
          count.val = 0
        end,
      },
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
      ui.row {
        style = { gap = 8, width = "full" },
        ui.column {
          style = {
            padding = 8,
            bg_color = "#dcfce7",
            border_color = "#22c55e",
            border_width = 1,
          },
          ui.text { "left column", style = { color = "#15803d" } },
          ui.text { "inside row", style = { color = "#166534" } },
        },
        ui.column {
          style = {
            padding = 8,
            bg_color = "#f3e8ff",
            border_color = "#a855f7",
            border_width = 1,
          },
          ui.text { "right column", style = { color = "#7e22ce" } },
          ui.text { "side by side", style = { color = "#6b21a8" } },
        },
      },
    },

    ui.column {
      style = {
        padding = 12,
        gap = 8,
        width = "full",
        bg_color = "#fff7ed",
        border_color = "#fb923c",
        border_width = 1,
      },
      ui.text { "Row swatches", style = { color = "#c2410c" } },
      swatch("blue row item", "#2563eb"),
      swatch("green row item", "#16a34a"),
      swatch("purple row item", "#9333ea"),
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
