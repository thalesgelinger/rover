local ui = rover.ui

local function item_label(item, index)
  if type(item) == "table" then
    if item.label ~= nil then
      return tostring(item.label)
    end
    if item.text ~= nil then
      return tostring(item.text)
    end
    if item.id ~= nil then
      return tostring(item.id)
    end
  end
  if item == nil then
    return tostring(index)
  end
  return tostring(item)
end

local M = {}

function M.select(props)
  props = props or {}
  local title = props.title or "select"
  local items = props.items or {}

  if type(items) == "userdata" then
    return ui.column {
      ui.text { title },
      ui.each(items, function(item, index)
        return ui.text { "- " .. item_label(item, index) }
      end, function(item, index)
        return item_label(item, index) .. ":" .. tostring(index)
      end),
    }
  end

  local children = { ui.text { title } }
  for i, item in ipairs(items) do
    children[#children + 1] = ui.text { "- " .. item_label(item, i) }
  end
  return ui.column(children)
end

function M.tab_select(props)
  props = props or {}
  local options = props.options or {}
  local value = props.value
  local on_change = props.on_change

  local children = {}
  for i, option in ipairs(options) do
    local id = option
    local label = option
    if type(option) == "table" then
      id = option.id ~= nil and option.id or i
      label = option.label ~= nil and option.label or id
    end

    children[#children + 1] = ui.button {
      label = tostring(label),
      on_click = function()
        if type(value) == "userdata" then
          value.val = id
        end
        if type(on_change) == "function" then
          on_change(id)
        end
      end,
    }
  end

  return ui.row(children)
end

function M.scroll_box(props)
  props = props or {}
  local child = props.child
  if child ~= nil then
    return ui.view { child }
  end
  if type(props.children) == "table" then
    return ui.view(props.children)
  end
  return ui.view { ui.text { "scroll_box(empty)" } }
end

function M.textarea(props)
  props = props or {}
  return ui.input {
    value = props.value or "",
    on_change = props.on_change,
    on_submit = props.on_submit,
  }
end

return M
