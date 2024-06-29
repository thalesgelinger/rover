local ui = require "rover.ui"
local component = require "rover.core"

local function Header(props)
    local value = signal(0)

    ui.view {
        ui.text {
            props.text
        }
    }
end

return component(Header)
