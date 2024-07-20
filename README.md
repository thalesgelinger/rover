# Rover 
Build mobile apps with the best interpreted language ever Lua #BrazilMentioned

```lua
function rover.run()
    return rover.view {
        height = "100",
        width = "full",
        color = "#0000ff",
        rover.text {
            "Hello Rover",
        }
    }
end
```

# The goal

the goal is to be able to make at least a View with a text inside, using lua table to design views

# How i plan to do that?

![image](https://github.com/thalesgelinger/rover/assets/55005400/ba59c58d-b750-4483-a394-99c86e8d8aad)


Since lua is an embeded language, my goal is to embed lua scripts to be reuse by android and ios platforms.
To make it happen, i intent to use rust as a bridge, so lua scripts are interpreted with rust, and rust compile that to use in native codes

