local header = require "header"

function rover.run()
    rover.view {
        horizontal = "center",
        vertical = "center",
        rover.text {
            "Hello World"
        },
        header {
            text = "Hello",
        }
    }
end
