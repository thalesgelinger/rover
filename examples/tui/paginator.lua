local ui = rover.ui
local tui = rover.tui

function rover.render()
  local page = rover.signal(1)
  local total = rover.signal(25)
  local status = rover.signal("left/right, home/end, pgup/pgdn")

  local range = rover.derive(function()
    local start_idx = (page.val - 1) * 10 + 1
    local end_idx = math.min(start_idx + 9, total.val * 10)
    return "rows " .. tostring(start_idx) .. ".." .. tostring(end_idx)
  end)

  return ui.column {
    ui.text { "paginator example" },
    tui.separator { width = 44, char = "=" },
    tui.paginator {
      page = page,
      total_pages = total,
      on_change = function(next_page)
        status.val = "page -> " .. tostring(next_page)
      end,
    },
    ui.text { range },
    tui.badge { label = "h/l works too", tone = "neutral" },
    ui.text { status },
  }
end
