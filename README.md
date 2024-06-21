# Rover 
Build mobile apps with the best interpreted language ever Lua #BrazilMentioned

```lua
local ui = require "./rover.ui"

return ui.view {
    backgroundColor = "#000fff",
    alignItems = "center",
    justifyContent = "center",

    ui.text { "Hello World" },

    ui.button {
        "Click Me",
        onClick = function()
            print "I came from the moon BTW"
        end
    }
}
```

# The goal

the goal is to be able to make at least a View with a text inside, using lua tagle to design views

# How i plan to do that?

Since lua is an embeded language, my goal is to embed lua scripts to be reuse by android and ios platforms.
To make it happen, i intent to use rust as a bridge, so lua scripts are interpreted with rust, and rust compile that to use in native codes

