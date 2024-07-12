local header = require "header"

local function app()
    rover.view {
        mainAxisAlignment = "center",
        crossAxisAlignment = "center",
        rover.text {
            "Hello World"
        },
        header {
            text = "Hello"
        }
    }
end

rover.run(app)
