require("rover.tui")

local ui = rover.ui

function rover.render()
  local status = rover.signal("ready")
  local tab = rover.signal("all")
  local page = rover.signal(1)
  local total_pages = rover.signal(8)

  local query = rover.signal("")
  local selected = rover.signal(1)

  local notes = rover.signal("type and press Enter")

  local progress_value = rover.signal(0)
  local progress_max = rover.signal(100)
  local progress_running = rover.signal(true)

  local items = rover.signal({
    { id = "all", label = "Parser cleanup" },
    { id = "all", label = "TUI key routing" },
    { id = "done", label = "require(\"rover.tui\")" },
    { id = "all", label = "Add kitchen sink example" },
    { id = "done", label = "Expose helper components" },
  })

  local visible = rover.derive(function()
    local out = {}
    local q = tostring(query.val):lower()

    for i = 1, #items.val do
      local item = items.val[i]
      local in_tab = tab.val == "all" or item.id == tab.val
      local text = tostring(item.label or item.id or i)
      local in_query = q == "" or string.find(text:lower(), q, 1, true) ~= nil
      if in_tab and in_query then
        out[#out + 1] = item
      end
    end

    return out
  end)

  local function clamp_selected(next_idx)
    local n = #visible.val
    if n == 0 then
      selected.val = 1
      return
    end
    if next_idx < 1 then
      selected.val = 1
    elseif next_idx > n then
      selected.val = n
    else
      selected.val = next_idx
    end
  end

  local progress_task = rover.task(function()
    while true do
      rover.delay(120)
      if progress_running.val then
        progress_value.val = progress_value.val + 1
        if progress_value.val >= progress_max.val then
          progress_value.val = progress_max.val
          progress_running.val = false
          status.val = "progress done"
        end
      end
    end
  end)

  progress_task()

  rover.on_destroy(function()
    rover.task.cancel(progress_task)
  end)

  return ui.column {
    ui.text { "tui kitchen sink" },
    ui.badge { label = "all components", tone = "info" },
    ui.separator { width = 60, char = "=" },

    ui.text { "tab_select" },
    ui.tab_select {
      value = tab,
      options = {
        { id = "all", label = "All" },
        { id = "done", label = "Done" },
      },
      on_change = function(next)
        status.val = "tab -> " .. tostring(next)
        selected.val = 1
      end,
    },

    ui.separator { width = 60, char = "-" },

    ui.text { "nav_list (app-controlled keys + search)" },
    ui.nav_list {
      title = "Tasks",
      items = visible,
      selected = selected,
      query = query,
      on_key = function(key)
        if key == "up" or key == "char:k" then
          clamp_selected(selected.val - 1)
          return
        end
        if key == "down" or key == "char:j" then
          clamp_selected(selected.val + 1)
          return
        end
        if key == "home" then
          clamp_selected(1)
          return
        end
        if key == "end" then
          clamp_selected(#visible.val)
          return
        end
        if key == "enter" then
          local picked = visible.val[selected.val]
          if picked ~= nil then
            status.val = "picked -> " .. tostring(picked.label or picked.id)
          else
            status.val = "picked -> none"
          end
          return
        end
        if key == "backspace" then
          local q = tostring(query.val)
          query.val = q:sub(1, #q - 1)
          selected.val = 1
          return
        end
        if key == "esc" then
          query.val = ""
          selected.val = 1
          return
        end

        local c = tostring(key):match("^char:(.+)$")
        if c ~= nil and #c == 1 then
          query.val = tostring(query.val) .. c
          selected.val = 1
        end
      end,
    },

    ui.separator { width = 60, char = "-" },

    ui.text { "scroll_box + select" },
    ui.scroll_box {
      ui.select {
        title = "Visible items",
        items = visible,
      },
    },

    ui.separator { width = 60, char = "-" },

    ui.text { "textarea" },
    ui.textarea {
      value = notes,
      on_submit = function(val)
        status.val = "notes submitted -> " .. tostring(val)
      end,
    },

    ui.separator { width = 60, char = "-" },

    ui.text { "progress" },
    ui.progress {
      label = "Build",
      value = progress_value,
      max = progress_max,
      width = 32,
    },
    ui.row {
      ui.button {
        label = "resume",
        on_click = function()
          progress_running.val = true
          status.val = "progress running"
        end,
      },
      ui.button {
        label = "pause",
        on_click = function()
          progress_running.val = false
          status.val = "progress paused"
        end,
      },
      ui.button {
        label = "reset",
        on_click = function()
          progress_value.val = 0
          progress_running.val = true
          status.val = "progress reset"
        end,
      },
    },

    ui.separator { width = 60, char = "-" },

    ui.text { "paginator" },
    ui.paginator {
      page = page,
      total_pages = total_pages,
      on_change = function(next)
        status.val = "page -> " .. tostring(next)
      end,
    },

    ui.separator { width = 60, char = "=" },

    ui.text { status },
    ui.badge { label = "keys: arrows, j/k, enter, esc, backspace, tab", tone = "neutral" },
  }
end
