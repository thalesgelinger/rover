local ui = rover.ui
local tui = rover.tui

function rover.render()
  local loaded = rover.signal(true)
  local show_help = rover.signal(true)
  local status = rover.signal("idle")
  local tab = rover.signal("all")
  local notes = rover.signal("type and press enter")
  local items = rover.signal({
    { id = "all", label = "Fix parser" },
    { id = "all", label = "Ship tui module" },
    { id = "done", label = "Use rover.tui namespace" },
  })

  local visible = rover.derive(function()
    local out = {}
    for i = 1, #items.val do
      local item = items.val[i]
      if tab.val == "all" or item.id == tab.val then
        out[#out + 1] = item
      end
    end
    return out
  end)

  local load_label = rover.derive(function()
    if loaded.val then
      return "rover.tui available"
    end
    return "rover.tui missing"
  end)

  return ui.column {
    ui.text { "Rover TUI module sample" },
    ui.text { load_label },
    ui.text { status },

    ui.button {
      label = "toggle help",
      on_click = function()
        show_help.val = not show_help.val
      end,
    },

    ui.when(show_help, function()
      return ui.column {
        ui.text { "TUI APIs live under rover.tui" },
        ui.text { "APIs: select, tab_select, scroll_box, textarea, nav_list, separator, badge, progress, paginator" },
      }
    end),

    ui.text { "tabs" },
    tui.tab_select {
      value = tab,
      options = {
        { id = "all", label = "All" },
        { id = "done", label = "Done" },
      },
      on_change = function(next)
        status.val = "tab: " .. tostring(next)
      end,
    },

    ui.text { "items" },
    tui.scroll_box {
      tui.select {
        title = "Task list",
        items = visible,
      },
    },

    ui.text { "notes" },
    tui.textarea {
      value = notes,
      on_submit = function(val)
        status.val = "submitted: " .. tostring(val)
      end,
    },

    ui.button {
      label = "mark status ready",
      on_click = function()
        status.val = "ready"
      end,
    },
  }
end
