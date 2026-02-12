local ui = rover.ui

local function is_signal(v)
  return type(v) == "userdata"
end

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

local function value_or(v, fallback)
  if is_signal(v) then
    return v.val
  end
  if v == nil then
    return fallback
  end
  return v
end

local function clamp(n, minv, maxv)
  if n < minv then
    return minv
  end
  if n > maxv then
    return maxv
  end
  return n
end

local function list_from(items)
  if is_signal(items) then
    return items.val or {}
  end
  if type(items) == "table" then
    return items
  end
  return {}
end

local function to_number(v, fallback)
  local n = value_or(v, fallback)
  if type(n) ~= "number" then
    n = tonumber(n)
  end
  if type(n) ~= "number" then
    return fallback
  end
  return n
end

local function repeat_char(ch, n)
  if n <= 0 then
    return ""
  end
  return string.rep(ch, n)
end

local function key_char(token)
  local s = tostring(token)
  return s:match("^char:(.+)$")
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
  return ui.scroll_box(props)
end

function M.textarea(props)
  props = props or {}
  return ui.input {
    value = props.value or "",
    on_change = props.on_change,
    on_submit = props.on_submit,
  }
end

function M.separator(props)
  props = props or {}
  local width = math.max(1, tonumber(props.width) or 30)
  local ch = tostring(props.char or "-")
  if #ch == 0 then
    ch = "-"
  end
  return ui.text { repeat_char(ch:sub(1, 1), width) }
end

function M.badge(props)
  props = props or {}
  local label = tostring(props.label or "badge")
  local tone = tostring(props.tone or "neutral")
  local prefix = {
    info = "i",
    success = "+",
    warning = "!",
    danger = "x",
    neutral = "-",
  }
  local p = prefix[tone] or prefix.neutral
  return ui.text { "[" .. p .. " " .. label .. "]" }
end

function M.progress(props)
  props = props or {}
  local width = math.max(8, tonumber(props.width) or 24)
  local label = props.label and tostring(props.label) or nil
  local value = props.value or 0
  local max = props.max or 100

  local function render_text()
    local v = to_number(value, 0)
    local m = math.max(1, to_number(max, 100))
    local pct = clamp(math.floor((v / m) * 100 + 0.5), 0, 100)
    local fill = math.floor((pct / 100) * width)
    local bar = "[" .. repeat_char("=", fill) .. repeat_char("-", width - fill) .. "]"
    if label then
      return label .. " " .. bar .. " " .. tostring(pct) .. "%"
    end
    return bar .. " " .. tostring(pct) .. "%"
  end

  if is_signal(value) or is_signal(max) then
    return ui.text { rover.derive(render_text) }
  end
  return ui.text { render_text() }
end

function M.paginator(props)
  props = props or {}
  local page = props.page
  local total_pages = props.total_pages or 1
  local on_change = props.on_change
  local on_key = props.on_key

  local function total()
    return math.max(1, to_number(total_pages, 1))
  end

  local function set_page(next_page)
    local next = clamp(next_page, 1, total())
    if is_signal(page) then
      page.val = next
    end
    if type(on_change) == "function" then
      on_change(next)
    end
  end

  local function default_on_key(key)
    local current = clamp(to_number(page, 1), 1, total())
    if key == "left" or key == "char:h" then
      set_page(current - 1)
    elseif key == "right" or key == "char:l" then
      set_page(current + 1)
    elseif key == "home" then
      set_page(1)
    elseif key == "end" then
      set_page(total())
    elseif key == "page_up" then
      set_page(current - 5)
    elseif key == "page_down" then
      set_page(current + 5)
    end
  end

  local label_text = rover.derive(function()
    local current = clamp(to_number(page, 1), 1, total())
    return "Page " .. tostring(current) .. "/" .. tostring(total())
  end)

  local content = ui.row {
    ui.text { "<" },
    ui.text { " " },
    ui.text { label_text },
    ui.text { " " },
    ui.text { ">" },
  }

  return ui.key_area {
    on_key = function(key)
      if type(on_key) == "function" then
        on_key(key)
      else
        default_on_key(key)
      end
    end,
    content,
  }
end

function M.nav_list(props)
  props = props or {}
  local title = props.title or "list"
  local items = props.items or {}
  local selected = props.selected
  local query = props.query
  local on_submit = props.on_submit
  local on_key = props.on_key

  local visible_items = items
  if is_signal(query) then
    visible_items = rover.derive(function()
      local src = list_from(items)
      local q = tostring(query.val or ""):lower()
      if q == "" then
        return src
      end
      local out = {}
      for i = 1, #src do
        local label = item_label(src[i], i):lower()
        if string.find(label, q, 1, true) ~= nil then
          out[#out + 1] = src[i]
        end
      end
      return out
    end)
  end

  local function selected_index()
    local list = list_from(visible_items)
    local n = #list
    if n == 0 then
      return 0
    end
    return clamp(to_number(selected, 1), 1, n)
  end

  local function set_selected(idx)
    local list = list_from(visible_items)
    local n = #list
    if not is_signal(selected) or n == 0 then
      return
    end
    selected.val = clamp(idx, 1, n)
  end

  local function run_submit()
    if type(on_submit) ~= "function" then
      return
    end
    local list = list_from(visible_items)
    local idx = selected_index()
    if idx <= 0 or idx > #list then
      return
    end
    on_submit(list[idx], idx)
  end

  local function default_on_key(key)
    local list = list_from(visible_items)
    local n = #list

    if key == "up" or key == "char:k" then
      set_selected(selected_index() - 1)
      return
    end
    if key == "down" or key == "char:j" then
      set_selected(selected_index() + 1)
      return
    end
    if key == "home" then
      set_selected(1)
      return
    end
    if key == "end" then
      set_selected(n)
      return
    end
    if key == "enter" then
      run_submit()
      return
    end

    if is_signal(query) then
      if key == "backspace" then
        local q = tostring(query.val or "")
        query.val = q:sub(1, #q - 1)
        set_selected(1)
        return
      end
      if key == "esc" then
        query.val = ""
        set_selected(1)
        return
      end

      local c = key_char(key)
      if c ~= nil and #c == 1 and c ~= "\n" and c ~= "\r" then
        query.val = tostring(query.val or "") .. c
        set_selected(1)
      end
    end
  end

  local list_body = ui.each(visible_items, function(item, index)
    local marker = "  "
    if selected_index() == index then
      marker = "> "
    end
    return ui.text { marker .. item_label(item, index) }
  end, function(item, index)
    return item_label(item, index) .. ":" .. tostring(index)
  end)

  local children = { ui.text { tostring(title) } }
  if is_signal(query) then
    children[#children + 1] = ui.text { rover.derive(function()
      return "search: " .. tostring(query.val or "")
    end) }
  end
  children[#children + 1] = list_body

  local content = ui.column(children)

  return ui.key_area {
    on_key = function(key)
      if type(on_key) == "function" then
        on_key(key)
      else
        default_on_key(key)
      end
    end,
    content,
  }
end

function M.full_screen(props)
  props = props or {}
  local on_key = props.on_key
  local child = props[1]
  if child == nil then
    child = props.child
  end
  return ui.full_screen {
    on_key = on_key,
    child,
  }
end

return M
