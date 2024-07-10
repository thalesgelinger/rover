local ui = require "rover.ui"
local header = require "header"

local function app()
    ui.view {
        mainAxisAlignment = "center",
        crossAxisAlignment = "center",
        ui.text {
            "Hello World"
        },
        header {
            text = "Hello"
        }
    }
end

rover.run(app)
