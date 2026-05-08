local ui = rover.ui

local M = {}

function M.window(props)
  props = props or {}
  return ui.__macos_window(props)
end

function M.scroll_view(props)
  props = props or {}
  return ui.scroll_view(props)
end

return M
