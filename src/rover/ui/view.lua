local utils = require "utils"

---@alias Alingment
---| "center"
---| "top"
---| "bottom"

---@class ViewChildren
---@field background_color? string
---@field mainAxis? Alingment
---@field crossAxis? Alingment
---@param children ViewChildren
function View(children)
    local parsed_children = utils.parse_children(children)
    parsed_children["id"] = "View"
    utils.show_all(parsed_children)
end

View.id = "View"

return View
