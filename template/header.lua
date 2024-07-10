local ui = require "rover.ui"

local function header(props, children)
    local value = signal(0)

    ui.view {
        ui.text {
            props.text
        }
    }
end

return rover.component(header)
