local ui = require "rover.ui"
local Header = require "header"

function App()
    ui.view {
        mainAxisAlignment = "center",
        crossAxisAlignment = "center",
        ui.text {
            "Hello World"
        },
        Header {
            text = "Hello"
        }
    }
end

return App
