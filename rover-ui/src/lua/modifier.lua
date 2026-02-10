local default_theme = {
  space = {
    none = 0,
    xs = 1,
    sm = 2,
    md = 3,
    lg = 4,
    xl = 6,
  },
  color = {
    surface = "#1f2937",
    surface_alt = "#374151",
    text = "#f9fafb",
    border = "#6b7280",
    accent = "#22c55e",
    danger = "#ef4444",
    warning = "#f59e0b",
    info = "#3b82f6",
  },
}

local function shallow_copy(t)
  local next = {}
  for k, v in pairs(t or {}) do
    next[k] = v
  end
  return next
end

local function copy_ops(ops)
  local out = {}
  for i = 1, #ops do
    local src = ops[i]
    out[i] = {
      kind = src.kind,
      value = src.value,
    }
  end
  return out
end

local function unwrap_reactive(value)
  if type(value) ~= "userdata" then
    return value, false
  end

  local ok, resolved = pcall(function()
    return value.val
  end)

  if ok then
    return resolved, true
  end

  return value, false
end

local function maybe_theme_color(theme, value)
  if type(value) ~= "string" then
    return value
  end
  if value:sub(1, 1) == "#" then
    return value
  end
  return ((theme or {}).color or {})[value] or value
end

local function maybe_theme_space(theme, value)
  if type(value) == "number" then
    return value
  end
  if type(value) ~= "string" then
    return 0
  end
  return ((theme or {}).space or {})[value] or tonumber(value) or 0
end

local core_methods = {}

function core_methods:_with_op(kind, value)
  local next = {
    _ops = copy_ops(self._ops or {}),
    _scalars = shallow_copy(self._scalars or {}),
    _theme = self._theme,
  }
  next._ops[#next._ops + 1] = { kind = kind, value = value }
  return setmetatable(next, { __index = self._root })
end

function core_methods:_with_scalar(key, value)
  local next = {
    _ops = copy_ops(self._ops or {}),
    _scalars = shallow_copy(self._scalars or {}),
    _theme = self._theme,
  }
  next._scalars[key] = value
  return setmetatable(next, { __index = self._root })
end

function core_methods:padding(value)
  return self:_with_op("padding", value)
end

function core_methods:bg_color(value)
  return self:_with_op("bg_color", value)
end

function core_methods:background_color(value)
  return self:bg_color(value)
end

function core_methods:border_color(value)
  return self:_with_op("border_color", value)
end

function core_methods:border_width(value)
  return self:_with_op("border_width", value)
end

function core_methods:width(value)
  return self:_with_scalar("width", value)
end

function core_methods:height(value)
  return self:_with_scalar("height", value)
end

function core_methods:position(value)
  return self:_with_scalar("position", value)
end

function core_methods:top(value)
  return self:_with_scalar("top", value)
end

function core_methods:left(value)
  return self:_with_scalar("left", value)
end

function core_methods:right(value)
  return self:_with_scalar("right", value)
end

function core_methods:bottom(value)
  return self:_with_scalar("bottom", value)
end

function core_methods:grow(value)
  return self:_with_scalar("grow", value)
end

function core_methods:gap(value)
  return self:_with_scalar("gap", value)
end

function core_methods:justify(value)
  return self:_with_scalar("justify", value)
end

function core_methods:align(value)
  return self:_with_scalar("align", value)
end

function core_methods:resolve()
  local out = {
    ops = {},
  }

  local theme = self._theme or default_theme
  for i = 1, #(self._ops or {}) do
    local op = self._ops[i]
    local value, _ = unwrap_reactive(op.value)

    if op.kind == "padding" then
      value = maybe_theme_space(theme, value)
    elseif op.kind == "bg_color" or op.kind == "border_color" then
      value = maybe_theme_color(theme, value)
    end

    out.ops[#out.ops + 1] = {
      kind = op.kind,
      value = value,
    }
  end

  for key, value in pairs(self._scalars or {}) do
    local resolved, _ = unwrap_reactive(value)
    if key == "gap" then
      resolved = maybe_theme_space(theme, resolved)
    end
    out[key] = resolved
  end

  return out
end

function core_methods:is_reactive()
  for i = 1, #(self._ops or {}) do
    local _, reactive = unwrap_reactive(self._ops[i].value)
    if reactive then
      return true
    end
  end

  for _, value in pairs(self._scalars or {}) do
    local _, reactive = unwrap_reactive(value)
    if reactive then
      return true
    end
  end

  return false
end

local function create_mod(theme)
  local mod = {
    _ops = {},
    _scalars = {},
    _theme = theme or default_theme,
  }

  mod._root = mod
  setmetatable(mod, {
    __index = function(tbl, key)
      local direct = rawget(tbl, key)
      if direct ~= nil then
        return direct
      end
      return core_methods[key]
    end,
  })

  return mod
end

return {
  default_theme = default_theme,
  create_mod = create_mod,
}
