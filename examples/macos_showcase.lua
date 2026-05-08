local ui = rover.ui
local macos = require("rover.macos")

ui.set_theme({
  space = { none = 0, xs = 4, sm = 8, md = 12, lg = 16, xl = 24 },
  color = {
    surface = "#0f172a",
    surface_alt = "#1e293b",
    text = "#e2e8f0",
    border = "#334155",
    accent = "#22c55e",
    warning = "#f59e0b",
    info = "#38bdf8",
  },
})

function rover.render()
  local count = rover.signal(0)
  local name = rover.signal("Rover")
  local submitted = rover.signal("(nothing submitted yet)")
  local agreed = rover.signal(false)
  local show_extra = rover.signal(true)
  local notes = rover.signal({
    "scroll row 1",
    "scroll row 2",
    "scroll row 3",
    "scroll row 4",
    "scroll row 5",
    "scroll row 6",
    "scroll row 7",
    "scroll row 8",
  })

  local greeting = rover.derive(function()
    if agreed.val then
      return "Hello, " .. name.val .. " (agreed)"
    end
    return "Hello, " .. name.val .. " (pending)"
  end)

  return macos.window {
    title = "Rover macOS Showcase",
    width = 820,
    height = 620,

    ui.column {
      style = {
        padding = "xl",
        gap = "md",
        width = "full",
        height = "full",
        bg_color = "surface",
      },

      ui.text { "Rover macOS UI showcase" },
      ui.text { greeting },
      ui.text { "Counter: " .. count },

      ui.row {
        style = { gap = "sm" },
        ui.button {
          label = "-1",
          on_click = function()
            count.val = count.val - 1
          end,
        },
        ui.button {
          label = "+1",
          on_click = function()
            count.val = count.val + 1
          end,
        },
        ui.button {
          label = "reset",
          on_click = function()
            count.val = 0
          end,
        },
        ui.button {
          label = "toggle extra",
          on_click = function()
            show_extra.val = not show_extra.val
          end,
        },
      },

      ui.row {
        style = { gap = "sm" },
        ui.input {
          value = name,
          on_change = function(value)
            name.val = value
          end,
          on_submit = function(value)
            submitted.val = value
          end,
        },
        ui.checkbox {
          checked = agreed,
          on_toggle = function(value)
            agreed.val = value
          end,
        },
      },

      ui.text { "Submitted input: " .. submitted },

      ui.when(show_extra, function()
        return ui.view {
          style = {
            padding = "md",
            gap = "sm",
            bg_color = "surface_alt",
            border_color = "info",
            border_width = 1,
          },
          ui.text { "Extra section (ui.when + ui.stack + ui.image)" },
          ui.stack {
            ui.text { "Stack base" },
            ui.text { "Stack overlay" },
          },
          ui.image { src = "examples/assets/fake.png" },
        }
      end),

      ui.text { "Scroll area (macos.scroll_view)" },
      macos.scroll_view {
        style = {
          height = 170,
          width = "full",
          padding = "sm",
          bg_color = "surface_alt",
          border_color = "border",
          border_width = 1,
        },
        ui.column {
          style = { gap = "xs" },
          ui.each(notes, function(item, index)
            return ui.text { index .. ": " .. item }
          end, function(item, index)
            return item .. "-" .. index
          end),
        },
      },

      ui.button {
        label = "append scroll row",
        on_click = function()
          local next = {}
          for i = 1, #notes.val do
            next[i] = notes.val[i]
          end
          next[#next + 1] = "scroll row " .. (#next + 1)
          notes.val = next
        end,
      },
    },
  }
end
