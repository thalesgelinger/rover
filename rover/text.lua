local utils = require "utils"

---@class Text any
function Text(children)
    local parsed_children = utils.parse_children(children)


    return parsed_children.children
end

Text.id = "Text"

return Text
