# Rover 
Build mobile apps with the best interpreted language ever Lua #BrazilMentioned

```lua
local ui = require "rover.ui"

function App()
    ui.view {
        mainAxisAlignment = "center",
        crossAxisAlignment = "center",
        ui.text {
            "Hello World"
        },
        ui.button {
            "Click Me",
            onClick = function()
                print "I came from the moon BTW"
            end
        }
    }
end

return App
```

# The goal

the goal is to be able to make at least a View with a text inside, using lua tagle to design views

# How i plan to do that?

![image](https://github.com/thalesgelinger/rover/assets/55005400/ba59c58d-b750-4483-a394-99c86e8d8aad)


Since lua is an embeded language, my goal is to embed lua scripts to be reuse by android and ios platforms.
To make it happen, i intent to use rust as a bridge, so lua scripts are interpreted with rust, and rust compile that to use in native codes

