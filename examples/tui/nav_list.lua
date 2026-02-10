require("rover.tui")

local ui = rover.ui

function rover.render()
  local items = rover.signal({
    { id = "parser", label = "Parser cleanup" },
    { id = "tui", label = "TUI key routing" },
    { id = "docs", label = "Docs update" },
    { id = "tests", label = "Add tests" },
  })
  local selected = rover.signal(1)
  local query = rover.signal("")
  local status = rover.signal("use arrows or j/k, enter to pick")

  local visible = rover.derive(function()
    local q = tostring(query.val):lower()
    if q == "" then
      return items.val
    end

    local out = {}
    for i = 1, #items.val do
      local item = items.val[i]
      local label = tostring(item.label or item.id or i):lower()
      if string.find(label, q, 1, true) ~= nil then
        out[#out + 1] = item
      end
    end
    return out
  end)

  local function clamp_index(next_idx)
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

  return ui.column {
    ui.text { "nav_list example" },
    ui.badge { label = "app-controlled", tone = "info" },
    ui.separator { width = 48, char = "-" },
    ui.text { rover.derive(function()
      return "query: " .. tostring(query.val)
    end) },
    ui.nav_list {
      title = "Tasks",
      items = visible,
      selected = selected,
      query = query,
      on_key = function(key)
        if key == "up" or key == "char:k" then
          clamp_index(selected.val - 1)
          return
        end
        if key == "down" or key == "char:j" then
          clamp_index(selected.val + 1)
          return
        end
        if key == "home" then
          clamp_index(1)
          return
        end
        if key == "end" then
          clamp_index(#visible.val)
          return
        end
        if key == "backspace" then
          local q = tostring(query.val)
          query.val = q:sub(1, #q - 1)
          clamp_index(1)
          return
        end
        if key == "esc" then
          query.val = ""
          clamp_index(1)
          return
        end

        local c = tostring(key):match("^char:(.+)$")
        if c ~= nil and #c == 1 then
          query.val = tostring(query.val) .. c
          clamp_index(1)
          return
        end

        if key == "enter" then
          local item = visible.val[selected.val]
          if item ~= nil then
            status.val = "picked: " .. tostring(item.label or item.id)
          else
            status.val = "picked: none"
          end
        end
      end,
    },
    ui.separator { width = 48, char = "-" },
    ui.text { status },
    ui.text { "tokens: up/down/home/end/enter/esc/backspace/char:*" },
    ui.text { "vim: j/k" },
  }
end
